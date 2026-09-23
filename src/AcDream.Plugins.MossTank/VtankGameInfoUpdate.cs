using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>What the game-information service answered, or why it did not.</summary>
internal readonly record struct VtankGameInfoAnswer(bool Succeeded, string Text);

/// <summary>
/// How the update check reaches the game-information service. The plugin
/// hands the panel the HTTP one; a panel built without one never asks.
/// </summary>
internal interface IVtankGameInfoTransport
{
    Task<VtankGameInfoAnswer> GetAsync(Uri address, CancellationToken cancellation);
}

/// <summary>A plain GET with a timeout and a size cap. Every failure comes back as a reason.</summary>
internal sealed class HttpVtankGameInfoTransport : IVtankGameInfoTransport, IDisposable
{
    /// <summary>How long the service has to answer before the check gives up.</summary>
    internal static readonly TimeSpan DefaultTimeout = TimeSpan.FromSeconds(30);

    /// <summary>
    /// The largest answer taken. A whole database is well under a megabyte;
    /// anything near this is not one.
    /// </summary>
    internal const long DefaultMaximumAnswerBytes = 32L * 1024 * 1024;

    private readonly HttpClient _client;
    private readonly TimeSpan _timeout;

    /// <param name="handler">What sends the request: the network, unless a test supplies one.</param>
    /// <param name="timeout">How long the service has to answer.</param>
    /// <param name="maximumAnswerBytes">The largest answer taken.</param>
    public HttpVtankGameInfoTransport(
        HttpMessageHandler? handler = null,
        TimeSpan? timeout = null,
        long maximumAnswerBytes = DefaultMaximumAnswerBytes)
    {
        _timeout = timeout ?? DefaultTimeout;
        _client = new HttpClient(handler ?? new HttpClientHandler(), disposeHandler: true)
        {
            Timeout = _timeout,
            MaxResponseContentBufferSize = maximumAnswerBytes,
        };
    }

    public async Task<VtankGameInfoAnswer> GetAsync(
        Uri address,
        CancellationToken cancellation)
    {
        try
        {
            using HttpResponseMessage response = await _client
                .GetAsync(address, HttpCompletionOption.ResponseContentRead, cancellation)
                .ConfigureAwait(false);
            if (!response.IsSuccessStatusCode)
            {
                return new VtankGameInfoAnswer(
                    false,
                    $"HTTP {(int)response.StatusCode} {response.ReasonPhrase}".TrimEnd());
            }
            string text = await response.Content
                .ReadAsStringAsync(cancellation)
                .ConfigureAwait(false);
            return new VtankGameInfoAnswer(true, text);
        }
        catch (HttpRequestException error)
        {
            return new VtankGameInfoAnswer(false, error.Message);
        }
        catch (TaskCanceledException) when (!cancellation.IsCancellationRequested)
        {
            return new VtankGameInfoAnswer(
                false,
                "no answer within "
                + _timeout.TotalSeconds.ToString("0.##", CultureInfo.InvariantCulture)
                + " seconds");
        }
    }

    public void Dispose() => _client.Dispose();
}

/// <summary>What one update check came to: the line to say, and the database to use if it changed.</summary>
internal sealed record VtankGameInfoUpdateResult(
    string Message,
    VtankGameInfoDatabase? Database);

/// <summary>
/// The reference client's game-information update. It asks the service for
/// everything that changed since the database's own last-update time, at the
/// database's own version; an answer with any record beyond its update-time
/// row is merged into the database table by table, keyed on each table's
/// index column, and the result is saved beside the profiles. An answer with
/// nothing else in it means the database is current, and nothing is saved.
/// </summary>
/// <remarks>
/// The request, the merge and the save run off the game thread; the outcome
/// waits for <see cref="Drain"/>, which the panel calls from its tick. Only an
/// answer whose every value reads is merged, only a merged database that
/// reads back is written, and the host's write replaces the file in one
/// step, so a failure at any point leaves the old file as it was. Once the
/// updater is disposed nothing more is written.
/// </remarks>
internal sealed class VtankGameInfoUpdater : IDisposable
{
    internal const string ServiceAddress =
        "http://auth.virindi.net/plugins/gamedb/get2.php";

    private const string KeptOld = " The game database you had is kept.";

    private readonly IPluginStorage _profiles;
    private readonly IVtankGameInfoTransport? _transport;
    private readonly CancellationTokenSource _cancellation = new();

    /// <summary>Held for the write, and by <see cref="Dispose"/> to wait one out.</summary>
    private readonly object _writeGate = new();
    private Task<VtankGameInfoUpdateResult>? _pending;
    private bool _disposed;

    public VtankGameInfoUpdater(IPluginStorage profiles, IVtankGameInfoTransport? transport)
    {
        _profiles = profiles ?? throw new ArgumentNullException(nameof(profiles));
        _transport = transport;
    }

    /// <summary>Can this session download at all: a service to ask and a folder to keep the answer in?</summary>
    public bool CanUpdate => _transport is not null && _profiles.IsAvailable;

    public bool IsRunning => _pending is not null;

    /// <summary>The check in flight, for a test to wait on.</summary>
    internal Task<VtankGameInfoUpdateResult>? PendingForTest => _pending;

    /// <summary>
    /// Starts a check unless one is already running or there is nothing to
    /// check with. The reason it did not start, or null when it did.
    /// </summary>
    public string? Start()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        if (_transport is null)
            return "Game database updates are not available in this session.";
        if (!_profiles.IsAvailable)
            return "Game database updates need a VTank profile folder to keep the database in.";
        if (_pending is not null)
            return "A game database update is already running.";
        IVtankGameInfoTransport transport = _transport;
        CancellationToken cancellation = _cancellation.Token;
        _pending = Task.Run(() => RunAsync(transport, cancellation), cancellation);
        return null;
    }

    /// <summary>
    /// On the game thread: once the check has finished, hands over the new
    /// database (if any) and then says how it ended, in one line.
    /// </summary>
    public void Drain(Action<string> say, Action<VtankGameInfoDatabase> apply)
    {
        ArgumentNullException.ThrowIfNull(say);
        ArgumentNullException.ThrowIfNull(apply);
        if (_pending is not { IsCompleted: true } finished)
            return;
        _pending = null;
        if (finished.IsCanceled)
            return;
        if (finished.IsFaulted)
        {
            Exception error = finished.Exception!.GetBaseException();
            say(Failed(error.Message).Message);
            return;
        }
        VtankGameInfoUpdateResult result = finished.Result;
        if (result.Database is not null)
            apply(result.Database);
        say(result.Message);
    }

    private async Task<VtankGameInfoUpdateResult> RunAsync(
        IVtankGameInfoTransport transport,
        CancellationToken cancellation)
    {
        VtankDatabase local = VtankGameInfoFile.LoadBase(_profiles);
        int date;
        try
        {
            date = LastUpdateTime(local);
        }
        catch (Exception error) when (error is FormatException or OverflowException)
        {
            return Failed("the database has no readable update time");
        }
        int version = VtankGameInfoFile.Version(local);

        var address = new Uri(
            ServiceAddress
            + "?date=" + date.ToString(CultureInfo.InvariantCulture)
            + "&dbver=" + version.ToString(CultureInfo.InvariantCulture));
        VtankGameInfoAnswer answer = await transport
            .GetAsync(address, cancellation)
            .ConfigureAwait(false);
        if (!answer.Succeeded)
            return Failed(answer.Text);

        VtankDatabase update;
        try
        {
            update = VtankDatabase.Parse(answer.Text);
        }
        catch (Exception error) when (
            error is FormatException or OverflowException or InvalidOperationException)
        {
            return Failed("the answer is not a game database (" + error.Message + ")");
        }
        if (!VtankGameInfoFile.EveryValueReads(update))
            return Failed("the answer is not a game database (a value in it does not read)");

        int changed = update.Tables.Sum(static entry => entry.Table.Rows.Count) - 1;
        if (changed <= 0)
            return new VtankGameInfoUpdateResult("Game database is up to date.", null);

        VtankGameInfoDatabase merged;
        string text;
        try
        {
            Merge(local, update);
            text = local.Render();
            // What is written is what is used: the saved text, read back.
            merged = VtankGameInfoDatabase.Parse(text);
        }
        catch (Exception error) when (
            error is FormatException or OverflowException or InvalidOperationException)
        {
            return Failed("the answer does not fit the database (" + error.Message + ")");
        }

        string count = changed.ToString(CultureInfo.InvariantCulture)
            + (changed == 1 ? " record changed" : " records changed");
        lock (_writeGate)
        {
            // A plugin that has been switched off writes nothing more.
            cancellation.ThrowIfCancellationRequested();
            try
            {
                _profiles.WriteText(VtankGameInfoDatabase.FileName, text);
            }
            catch (Exception error) when (
                error is IOException or UnauthorizedAccessException or NotSupportedException)
            {
                // The download is good; only keeping it failed. The reference
                // client goes on using a database it could not save, and so
                // does this.
                return new VtankGameInfoUpdateResult(
                    "Game database downloaded (" + count + ") but could not be saved: "
                    + error.Message.TrimEnd('.') + ". It is used until MossTank stops.",
                    merged);
            }
        }
        return new VtankGameInfoUpdateResult("Game database updated: " + count + ".", merged);
    }

    private static VtankGameInfoUpdateResult Failed(string reason) =>
        new("Game database update failed: " + reason.TrimEnd('.') + "." + KeptOld, null);

    /// <summary>The <c>DBLastUpdateTime</c> row's time, in seconds since 1970.</summary>
    internal static int LastUpdateTime(VtankDatabase database)
    {
        VtankTable? table = database.Find("DBLastUpdateTime");
        if (table is not { Rows.Count: > 0 } || table.Rows[0].Cells.Count < 2)
            throw new FormatException("DBLastUpdateTime has no row.");
        return table.Rows[0].Cells[1].AsInt();
    }

    /// <summary>
    /// Every table the database already has takes the rows the update has
    /// for it: a row whose index-column value matches an existing row
    /// overwrites that row's other columns, any other row is added. A table
    /// the database does not have, or one whose column count differs, is
    /// left out, exactly as the reference client leaves it out.
    /// </summary>
    internal static void Merge(VtankDatabase database, VtankDatabase update)
    {
        foreach ((string name, VtankTable table) in database.Tables)
        {
            VtankTable? incoming = update.Find(name);
            if (incoming is null || incoming.ColumnNames.Count != table.ColumnNames.Count)
                continue;
            int key = Math.Max(0, table.IndexFlags.IndexOf(true));
            if (key >= table.ColumnNames.Count)
                continue;
            // The first row with a key is the one a lookup finds.
            var byKey = new Dictionary<string, VtankRow>(StringComparer.OrdinalIgnoreCase);
            foreach (VtankRow row in table.Rows)
                byKey.TryAdd(KeyOf(row.Cells[key]), row);
            foreach (VtankRow row in incoming.Rows)
            {
                string rowKey = KeyOf(row.Cells[key]);
                if (!byKey.TryGetValue(rowKey, out VtankRow? existing))
                {
                    var added = new VtankRow();
                    added.Cells.AddRange(row.Cells);
                    table.Rows.Add(added);
                    byKey.Add(rowKey, added);
                    continue;
                }
                for (int column = 0; column < existing.Cells.Count; column++)
                {
                    if (column != key)
                        existing.Cells[column] = row.Cells[column];
                }
            }
        }
    }

    /// <summary>
    /// An index value as the lookup compares it: text without case, a number
    /// by its value, and values of two different kinds never alike. The
    /// lookup's comparer ignores case, which leaves numbers untouched.
    /// </summary>
    private static string KeyOf(VtankCell cell) => cell.Tag switch
    {
        "s" => "s\0" + cell.AsString(),
        "i" => "i\0" + cell.AsInt().ToString(CultureInfo.InvariantCulture),
        "u" => "u\0" + cell.AsUInt().ToString(CultureInfo.InvariantCulture),
        "d" => "d\0" + cell.AsDouble().ToString("R", CultureInfo.InvariantCulture),
        "f" => "f\0" + cell.AsFloat().ToString("R", CultureInfo.InvariantCulture),
        _ => throw new InvalidOperationException(
            $"An index value of kind '{cell.Tag}' cannot be compared."),
    };

    internal static DateTimeOffset FromUnixSeconds(int seconds) =>
        new DateTimeOffset(1970, 1, 1, 0, 0, 0, TimeSpan.Zero) + TimeSpan.FromSeconds(seconds);

    /// <summary>
    /// Stops a check for good: it is cancelled, a save already under way is
    /// waited out, and none starts after this returns. Whatever the check
    /// ends with is observed here, so nothing is left unobserved.
    /// </summary>
    public void Dispose()
    {
        if (_disposed)
            return;
        _disposed = true;
        _cancellation.Cancel();
        lock (_writeGate)
        {
            // Only waits for a write that had already started.
        }
        _pending?.ContinueWith(
            static finished => _ = finished.Exception,
            CancellationToken.None,
            TaskContinuationOptions.OnlyOnFaulted | TaskContinuationOptions.ExecuteSynchronously,
            TaskScheduler.Default);
        _pending = null;
        (_transport as IDisposable)?.Dispose();
    }
}

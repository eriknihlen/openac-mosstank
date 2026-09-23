using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>What the download answered, or why it did not.</summary>
internal readonly record struct VtankGameInfoAnswer(bool Succeeded, string Text);

/// <summary>
/// How the update check fetches the game database. The plugin hands the
/// panel the HTTP one; a panel built without one never downloads anything.
/// </summary>
internal interface IVtankGameInfoTransport
{
    Task<VtankGameInfoAnswer> GetAsync(Uri address, CancellationToken cancellation);
}

/// <summary>
/// A plain GET that follows redirects, with a timeout and a size cap. Every
/// failure comes back as a reason.
/// </summary>
internal sealed class HttpVtankGameInfoTransport : IVtankGameInfoTransport, IDisposable
{
    /// <summary>How long the download has to finish before the check gives up.</summary>
    internal static readonly TimeSpan DefaultTimeout = TimeSpan.FromSeconds(30);

    /// <summary>
    /// The largest answer taken. A whole database is a few megabytes; anything
    /// near this is not one.
    /// </summary>
    internal const long DefaultMaximumAnswerBytes = 32L * 1024 * 1024;

    private readonly HttpClient _client;
    private readonly TimeSpan _timeout;

    /// <param name="handler">What sends the request: the network, unless a test supplies one.</param>
    /// <param name="timeout">How long the download has to finish.</param>
    /// <param name="maximumAnswerBytes">The largest answer taken.</param>
    public HttpVtankGameInfoTransport(
        HttpMessageHandler? handler = null,
        TimeSpan? timeout = null,
        long maximumAnswerBytes = DefaultMaximumAnswerBytes)
    {
        _timeout = timeout ?? DefaultTimeout;
        // The release link answers with a redirect to the file itself; the
        // default handler follows it.
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
/// Keeps the player's game database current from openac-gamedata, an
/// independent project that publishes a complete database, generated from
/// the ACE server's world data, as a release file. Each check downloads the
/// whole file; one that reads as a whole database at the built-in version,
/// and was built from newer world data than the one the player has, replaces
/// the player's file. A database whose last check is younger than the window
/// is not checked again.
/// </summary>
/// <remarks>
/// The download, the checks and the save run off the game thread; the
/// outcome waits for <see cref="Drain"/>, which the panel calls from its
/// tick. Only a database whose every value reads is written, and the host's
/// write replaces the file in one step, so a failure at any point leaves the
/// old file as it was. Once the updater is disposed nothing more is written.
/// </remarks>
internal sealed class VtankGameInfoUpdater : IDisposable
{
    /// <summary>The newest published database; the link redirects to the file.</summary>
    internal const string SourceAddress =
        "https://github.com/eriknihlen/openac-gamedata/releases/latest/download/gameinfodb.ugd";

    /// <summary>Where the source is named in chat.</summary>
    internal const string SourceName = "openac-gamedata";

    /// <summary>
    /// The plugin's own note of when the database was last checked, in
    /// seconds since 1970. It lives beside the plugin's other state, not in
    /// the database, whose own time says which world data it was built from.
    /// </summary>
    internal const string LastCheckKey = "gamedb/last-check.txt";

    private const string KeptOld = " The game database you had is kept.";

    private readonly IPluginStorage _profiles;
    private readonly IPluginStorage _state;
    private readonly IVtankGameInfoTransport? _transport;
    private readonly Func<DateTimeOffset> _clock;
    private readonly CancellationTokenSource _cancellation = new();

    /// <summary>Held for the write, and by <see cref="Dispose"/> to wait one out.</summary>
    private readonly object _writeGate = new();
    private Task<VtankGameInfoUpdateResult>? _pending;
    private bool _disposed;

    /// <param name="profiles">The VTank profile folder the database is kept in.</param>
    /// <param name="state">The plugin's own storage, for the note of the last check.</param>
    /// <param name="transport">How the file is fetched; none means no downloads.</param>
    /// <param name="clock">The time now.</param>
    public VtankGameInfoUpdater(
        IPluginStorage profiles,
        IPluginStorage state,
        IVtankGameInfoTransport? transport,
        Func<DateTimeOffset>? clock = null)
    {
        _profiles = profiles ?? throw new ArgumentNullException(nameof(profiles));
        _state = state ?? throw new ArgumentNullException(nameof(state));
        _transport = transport;
        _clock = clock ?? (static () => DateTimeOffset.UtcNow);
    }

    /// <summary>Can this session download at all: a source to fetch from and a folder to keep the file in?</summary>
    public bool CanUpdate => _transport is not null && _profiles.IsAvailable;

    public bool IsRunning => _pending is not null;

    /// <summary>The check in flight, for a test to wait on.</summary>
    internal Task<VtankGameInfoUpdateResult>? PendingForTest => _pending;

    /// <summary>
    /// Starts a check unless one is already running or there is nothing to
    /// check with. The reason it did not start, or null when it did.
    /// </summary>
    /// <param name="skipIfCheckedWithin">
    /// A database last checked less than this long ago is not downloaded
    /// again; the check hands over what the file holds instead. Zero forces
    /// the download.
    /// </param>
    public string? Start(TimeSpan skipIfCheckedWithin = default)
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
        _pending = Task.Run(
            () => RunAsync(transport, skipIfCheckedWithin, cancellation),
            cancellation);
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
        TimeSpan skipIfCheckedWithin,
        CancellationToken cancellation)
    {
        VtankDatabase local = VtankGameInfoFile.LoadBase(_profiles);
        if (skipIfCheckedWithin > TimeSpan.Zero
            && ReadLastCheck(_state) is { } lastCheck)
        {
            DateTimeOffset nextCheck = lastCheck + skipIfCheckedWithin;
            if (_clock() < nextCheck)
            {
                // Another session may have downloaded since this one loaded
                // the file, so what the file holds now is handed over.
                return new VtankGameInfoUpdateResult(
                    "Game database checked recently; the next check is after "
                    + FormatUtc(nextCheck) + ".",
                    VtankGameInfoDatabase.From(local));
            }
        }
        int localTime = TryLastUpdateTime(local) ?? 0;

        VtankGameInfoAnswer answer = await transport
            .GetAsync(new Uri(SourceAddress), cancellation)
            .ConfigureAwait(false);
        if (!answer.Succeeded)
            return Failed(answer.Text);

        VtankDatabase downloaded;
        try
        {
            downloaded = VtankDatabase.Parse(answer.Text);
        }
        catch (Exception error) when (
            error is FormatException or OverflowException or InvalidOperationException)
        {
            return Failed("the download is not a game database (" + error.Message + ")");
        }
        if (!VtankGameInfoFile.EveryValueReads(downloaded))
            return Failed("the download is not a game database (a value in it does not read)");
        int version = VtankGameInfoFile.Version(downloaded);
        if (version != VtankGameInfoFile.BuiltInVersion)
        {
            return Failed(
                "the download is database version "
                + version.ToString(CultureInfo.InvariantCulture) + ", not "
                + VtankGameInfoFile.BuiltInVersion.ToString(CultureInfo.InvariantCulture));
        }
        if (TryLastUpdateTime(downloaded) is not { } downloadedTime)
            return Failed("the download does not say which world data it was built from");

        string built = FormatDate(downloadedTime);
        if (downloadedTime <= localTime)
        {
            lock (_writeGate)
            {
                cancellation.ThrowIfCancellationRequested();
                return new VtankGameInfoUpdateResult(
                    "Game database is up to date (" + built + ", " + SourceName + ")."
                    + NoteCheck(),
                    null);
            }
        }

        VtankGameInfoDatabase database = VtankGameInfoDatabase.From(downloaded);
        lock (_writeGate)
        {
            // A plugin that has been switched off writes nothing more.
            cancellation.ThrowIfCancellationRequested();
            try
            {
                _profiles.WriteText(VtankGameInfoDatabase.FileName, answer.Text);
            }
            catch (Exception error) when (
                error is IOException or UnauthorizedAccessException or NotSupportedException)
            {
                // The download is good; only keeping it failed, so it is used
                // for as long as the plugin runs.
                return new VtankGameInfoUpdateResult(
                    "Game database updated to " + built + " from " + SourceName
                    + " but could not be saved: " + error.Message.TrimEnd('.')
                    + ". It is used until MossTank stops.",
                    database);
            }
            return new VtankGameInfoUpdateResult(
                "Game database updated to " + built + " from " + SourceName + "."
                + NoteCheck(),
                database);
        }
    }

    /// <summary>
    /// Keeps the time of a check that reached the source, so a login within
    /// the window does not download again. The empty string when it is kept;
    /// otherwise the reason, for the end of the line.
    /// </summary>
    private string NoteCheck()
    {
        if (!_state.IsAvailable)
            return string.Empty;
        try
        {
            _state.WriteText(
                LastCheckKey,
                _clock().ToUnixTimeSeconds().ToString(CultureInfo.InvariantCulture));
            return string.Empty;
        }
        catch (Exception error) when (
            error is IOException or UnauthorizedAccessException or NotSupportedException)
        {
            return " (The time of this check could not be kept: " + error.Message.TrimEnd('.') + ".)";
        }
    }

    /// <summary>When the database was last checked, or null when no check has been kept.</summary>
    internal static DateTimeOffset? ReadLastCheck(IPluginStorage state)
    {
        ArgumentNullException.ThrowIfNull(state);
        return state.IsAvailable
            && long.TryParse(
                state.ReadText(LastCheckKey)?.Trim(),
                NumberStyles.Integer,
                CultureInfo.InvariantCulture,
                out long seconds)
            && seconds > 0
                ? DateTimeOffset.FromUnixTimeSeconds(seconds)
                : null;
    }

    private static VtankGameInfoUpdateResult Failed(string reason) =>
        new("Game database update failed: " + reason.TrimEnd('.') + "." + KeptOld, null);

    /// <summary>The <c>DBLastUpdateTime</c> row's time, in seconds since 1970, or null when it has none.</summary>
    internal static int? TryLastUpdateTime(VtankDatabase database)
    {
        VtankTable? table = database.Find("DBLastUpdateTime");
        return table is { Rows.Count: > 0 } && table.Rows[0].Cells.Count >= 2
            && int.TryParse(
                table.Rows[0].Cells[1].ScalarText,
                NumberStyles.Integer,
                CultureInfo.InvariantCulture,
                out int seconds)
                ? seconds
                : null;
    }

    internal static DateTimeOffset FromUnixSeconds(int seconds) =>
        new DateTimeOffset(1970, 1, 1, 0, 0, 0, TimeSpan.Zero) + TimeSpan.FromSeconds(seconds);

    internal static string FormatUtc(DateTimeOffset time) =>
        time.UtcDateTime.ToString("yyyy-MM-dd HH:mm 'UTC'", CultureInfo.InvariantCulture);

    /// <summary>A database's own time as the day its world data is from.</summary>
    internal static string FormatDate(int seconds) =>
        FromUnixSeconds(seconds).UtcDateTime.ToString("yyyy-MM-dd", CultureInfo.InvariantCulture);

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

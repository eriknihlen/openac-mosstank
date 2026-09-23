using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The game-database update against a transport that answers from the test:
/// what it fetches, when it replaces the player's file, and what it keeps
/// when the download is no good.
/// </summary>
public sealed class VtankGameInfoUpdaterTests
{
    internal static readonly string ExcerptText = File.ReadAllText(Path.Combine(
        AppContext.BaseDirectory, "Fixtures", "vtank", "gameinfodb-excerpt.ugd"));

    /// <summary>The fixture's own time: 2023-11-14 22:13:20 UTC.</summary>
    private const int ExcerptTime = 1700000000;

    private const int OneDay = 86400;

    /// <summary>
    /// A download built from newer world data than the player's file
    /// replaces it, as it came, and is handed over; the check is noted.
    /// Mutation: keep the player's file whatever the download's time (the
    /// newer database is reported up to date).
    /// </summary>
    [Fact]
    public async Task ANewerDatabaseReplacesTheFileAndIsHandedOver()
    {
        var profiles = new MemoryStorage();
        profiles.Text[VtankGameInfoDatabase.FileName] = ExcerptText;
        var state = new MemoryStorage();
        VtankGameInfoAnswer download = Download(ExcerptTime + OneDay, "Downloaded Arrow");
        var transport = new FakeTransport(download);
        DateTimeOffset now = VtankGameInfoUpdater.FromUnixSeconds(ExcerptTime + 2 * OneDay);

        (List<string> said, List<VtankGameInfoDatabase> applied) =
            await RunAsync(new VtankGameInfoUpdater(profiles, state, transport, () => now));

        Assert.Equal(VtankGameInfoUpdater.SourceAddress, Assert.Single(transport.Requests).ToString());
        Assert.Equal("Game database updated to 2023-11-15 from openac-gamedata.", said[^1]);
        Assert.Equal(download.Text, profiles.Text[VtankGameInfoDatabase.FileName]);
        Assert.Equal("Downloaded Arrow", Assert.Single(Assert.Single(applied).AmmunitionOptions).Name);
        Assert.Equal(now, VtankGameInfoUpdater.ReadLastCheck(state));
    }

    /// <summary>
    /// A download no newer than the player's file leaves the file alone and
    /// says it is up to date, naming the world data it holds; the check is
    /// still noted. Mutation: take a download of the same time as newer (the
    /// file is rewritten).
    /// </summary>
    [Fact]
    public async Task ADatabaseNoNewerThanTheFileIsUpToDate()
    {
        var profiles = new MemoryStorage();
        profiles.Text[VtankGameInfoDatabase.FileName] = ExcerptText;
        var state = new MemoryStorage();
        var transport = new FakeTransport(Download(ExcerptTime, "Downloaded Arrow"));

        (List<string> said, List<VtankGameInfoDatabase> applied) =
            await RunAsync(new VtankGameInfoUpdater(profiles, state, transport));

        Assert.Equal("Game database is up to date (2023-11-14, openac-gamedata).", said[^1]);
        Assert.Empty(applied);
        Assert.Equal(0, profiles.Writes);
        Assert.NotNull(VtankGameInfoUpdater.ReadLastCheck(state));
    }

    /// <summary>
    /// With no file, or one of another version (which is read as the empty
    /// built-in database), any good download is taken. Mutation: compare
    /// against the file's own time even when it was dropped for the built-in
    /// database (the version-4 file's newer time keeps the download out).
    /// </summary>
    [Fact]
    public async Task WithNoUsableFileAnyGoodDownloadIsTaken()
    {
        var none = new MemoryStorage();
        VtankDatabase oldVersion = VtankDatabase.Parse(ExcerptText);
        oldVersion.Find("DBVersion")!.Rows[0].Cells[0] = VtankCell.Int(4);
        oldVersion.Find("DBLastUpdateTime")!.Rows[0].Cells[1] = VtankCell.Int(ExcerptTime + 9 * OneDay);
        var other = new MemoryStorage();
        other.Text[VtankGameInfoDatabase.FileName] = oldVersion.Render();

        foreach (MemoryStorage profiles in new[] { none, other })
        {
            (List<string> said, _) = await RunAsync(new VtankGameInfoUpdater(
                profiles, new MemoryStorage(), new FakeTransport(Download(ExcerptTime))));

            Assert.Equal("Game database updated to 2023-11-14 from openac-gamedata.", said[^1]);
            Assert.Equal(1, profiles.Writes);
        }
    }

    /// <summary>
    /// A download of another database version is refused: the next start
    /// would drop it for the empty built-in one. Mutation: drop the version
    /// check, and it is written.
    /// </summary>
    [Fact]
    public async Task ADownloadOfAnotherVersionIsRefused()
    {
        var profiles = new MemoryStorage();
        var state = new MemoryStorage();
        VtankGameInfoAnswer download = Download(
            ExcerptTime,
            change: static database => database.Find("DBVersion")!.Rows[0].Cells[0] = VtankCell.Int(10));

        (List<string> said, List<VtankGameInfoDatabase> applied) = await RunAsync(
            new VtankGameInfoUpdater(profiles, state, new FakeTransport(download)));

        Assert.Equal(
            "Game database update failed: the download is database version 10, not 9. "
            + "The game database you had is kept.",
            said[^1]);
        Assert.Empty(applied);
        Assert.Equal(0, profiles.Writes);
        Assert.Null(VtankGameInfoUpdater.ReadLastCheck(state));
    }

    /// <summary>
    /// A download holding a value that does not read as its kind is refused
    /// whole. Mutation: skip the value check, and the broken file is written.
    /// </summary>
    [Fact]
    public async Task ADownloadWithAValueThatDoesNotReadIsRefused()
    {
        var profiles = new MemoryStorage();
        VtankGameInfoAnswer download = Download(
            ExcerptTime,
            change: static database =>
            {
                var row = new VtankRow();
                row.Cells.AddRange(
                [
                    VtankCell.String("Broken Kit"),
                    new VtankCell { Tag = "d", ScalarText = "lots" },
                    VtankCell.Int(0),
                    VtankCell.Int(1),
                ]);
                database.Find("HealKits")!.Rows.Add(row);
            });

        (List<string> said, _) = await RunAsync(
            new VtankGameInfoUpdater(profiles, new MemoryStorage(), new FakeTransport(download)));

        Assert.Equal(
            "Game database update failed: the download is not a game database "
            + "(a value in it does not read). The game database you had is kept.",
            said[^1]);
        Assert.Equal(0, profiles.Writes);
    }

    /// <summary>
    /// A download that is not a database at all, or one that does not say
    /// which world data it is from, is refused and the file kept.
    /// Mutation: take a download with no time as the oldest possible one
    /// (it is compared, and reported up to date instead of refused).
    /// </summary>
    [Fact]
    public async Task ADownloadThatIsNotADatedDatabaseIsRefused()
    {
        var profiles = new MemoryStorage();
        profiles.Text[VtankGameInfoDatabase.FileName] = ExcerptText;
        VtankGameInfoAnswer undated = Download(
            ExcerptTime,
            change: static database => database.Find("DBLastUpdateTime")!.Rows.Clear());

        (List<string> page, _) = await RunAsync(new VtankGameInfoUpdater(
            profiles, new MemoryStorage(),
            new FakeTransport(new VtankGameInfoAnswer(true, "<html>moved</html>"))));
        (List<string> noTime, _) = await RunAsync(new VtankGameInfoUpdater(
            profiles, new MemoryStorage(), new FakeTransport(undated)));

        Assert.StartsWith(
            "Game database update failed: the download is not a game database (",
            page[^1],
            StringComparison.Ordinal);
        Assert.Equal(
            "Game database update failed: the download does not say which world data "
            + "it was built from. The game database you had is kept.",
            noTime[^1]);
        Assert.Equal(0, profiles.Writes);
    }

    /// <summary>
    /// A download that fails says the transport's reason, keeps the file, and
    /// notes no check, so the next login tries again. Mutation: note the
    /// check before the download (the failure counts as a check).
    /// </summary>
    [Fact]
    public async Task AFailedDownloadSaysWhyAndNotesNoCheck()
    {
        var profiles = new MemoryStorage();
        profiles.Text[VtankGameInfoDatabase.FileName] = ExcerptText;
        var state = new MemoryStorage();
        var transport = new FakeTransport(new VtankGameInfoAnswer(false, "HTTP 503 Service Unavailable"));

        (List<string> said, List<VtankGameInfoDatabase> applied) =
            await RunAsync(new VtankGameInfoUpdater(profiles, state, transport));

        Assert.Equal(
            "Game database update failed: HTTP 503 Service Unavailable. The game database you had is kept.",
            said[^1]);
        Assert.Empty(applied);
        Assert.Equal(0, profiles.Writes);
        Assert.Null(VtankGameInfoUpdater.ReadLastCheck(state));
    }

    /// <summary>
    /// A newer download that cannot be saved is still used, and the line
    /// says it was not kept. Mutation: treat a failed save as a failed
    /// update, and nothing is handed over.
    /// </summary>
    [Fact]
    public async Task ADownloadThatCannotBeSavedIsStillUsed()
    {
        var profiles = new MemoryStorage { WriteFailure = new IOException("The disk is full.") };
        profiles.Text[VtankGameInfoDatabase.FileName] = ExcerptText;
        var transport = new FakeTransport(Download(ExcerptTime + OneDay, "Downloaded Arrow"));

        (List<string> said, List<VtankGameInfoDatabase> applied) =
            await RunAsync(new VtankGameInfoUpdater(profiles, new MemoryStorage(), transport));

        Assert.Equal(
            "Game database updated to 2023-11-15 from openac-gamedata but could not be saved: "
            + "The disk is full. It is used until MossTank stops.",
            said[^1]);
        Assert.Single(applied);
        Assert.Equal(ExcerptText, profiles.Text[VtankGameInfoDatabase.FileName]);
    }

    /// <summary>
    /// A check noted within the window is not repeated: nothing is fetched,
    /// what the file holds is handed over, and the line says when the next
    /// check is due. Past the window, or with no window (a forced check), it
    /// fetches. Mutation: ignore the window, and the recent check fetches.
    /// </summary>
    [Fact]
    public async Task ARecentCheckIsNotRepeated()
    {
        var profiles = new MemoryStorage();
        profiles.Text[VtankGameInfoDatabase.FileName] = ExcerptText;
        var state = new MemoryStorage();
        DateTimeOffset checkedAt = VtankGameInfoUpdater.FromUnixSeconds(ExcerptTime + OneDay);
        state.Text[VtankGameInfoUpdater.LastCheckKey] =
            checkedAt.ToUnixTimeSeconds().ToString(System.Globalization.CultureInfo.InvariantCulture);
        var transport = new FakeTransport(Download(ExcerptTime));
        TimeSpan window = TimeSpan.FromHours(6);

        (List<string> said, List<VtankGameInfoDatabase> applied) = await RunAsync(
            new VtankGameInfoUpdater(profiles, state, transport, () => checkedAt.AddHours(1)),
            window);

        Assert.Empty(transport.Requests);
        Assert.Equal(
            "Game database checked recently; the next check is after 2023-11-16 04:13 UTC.",
            said[^1]);
        Assert.Equal(7, Assert.Single(applied).AmmunitionOptions.Count);

        await RunAsync(
            new VtankGameInfoUpdater(profiles, state, transport, () => checkedAt.AddHours(7)),
            window);
        await RunAsync(
            new VtankGameInfoUpdater(profiles, state, transport, () => checkedAt.AddMinutes(1)),
            TimeSpan.Zero);
        Assert.Equal(2, transport.Requests.Count);
    }

    /// <summary>
    /// One check at a time: a second start while one is in flight is
    /// refused, and nothing is said about the first until it ends.
    /// Mutation: drop the in-flight refusal, and the source is asked twice.
    /// </summary>
    [Fact]
    public async Task OnlyOneCheckRunsAtATime()
    {
        var profiles = new MemoryStorage();
        var answer = new TaskCompletionSource<VtankGameInfoAnswer>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var transport = new FakeTransport(answer.Task);
        using var updater = new VtankGameInfoUpdater(profiles, new MemoryStorage(), transport);

        Assert.Null(updater.Start());
        Assert.True(SpinWait.SpinUntil(() => transport.Requests.Count == 1, 5000));
        Assert.Equal("A game database update is already running.", updater.Start());
        var said = new List<string>();
        updater.Drain(said.Add, static _ => { });
        Assert.Empty(said);

        answer.SetResult(Download(ExcerptTime));
        await updater.PendingForTest!.WaitAsync(TimeSpan.FromSeconds(5));
        updater.Drain(said.Add, static _ => { });

        Assert.Single(transport.Requests);
        Assert.Equal("Game database updated to 2023-11-14 from openac-gamedata.", said[^1]);
        Assert.False(updater.IsRunning);
    }

    /// <summary>
    /// Without a source or a folder to keep the file in there is no check,
    /// and the refusal says which.
    /// </summary>
    [Fact]
    public void ACheckNeedsASourceAndAFolder()
    {
        using var noSource = new VtankGameInfoUpdater(new MemoryStorage(), new MemoryStorage(), null);
        using var noFolder = new VtankGameInfoUpdater(
            NoOpPluginStorage.Instance, new MemoryStorage(), new FakeTransport(Download(0)));

        Assert.False(noSource.CanUpdate);
        Assert.False(noFolder.CanUpdate);
        Assert.Equal("Game database updates are not available in this session.", noSource.Start());
        Assert.Equal(
            "Game database updates need a VTank profile folder to keep the database in.",
            noFolder.Start());
    }

    /// <summary>
    /// A transport that throws instead of answering ends the check with the
    /// reason, on the tick, like any other failure. Mutation: pass over a
    /// faulted check in silence, and nothing is said.
    /// </summary>
    [Fact]
    public async Task ATransportThatThrowsSaysWhy()
    {
        var transport = new FakeTransport(
            Task.FromException<VtankGameInfoAnswer>(new InvalidOperationException("No route.")));
        using var updater = new VtankGameInfoUpdater(new MemoryStorage(), new MemoryStorage(), transport);

        Assert.Null(updater.Start());
        Task<VtankGameInfoUpdateResult> pending = updater.PendingForTest!;
        await Assert.ThrowsAsync<InvalidOperationException>(() => pending);
        var said = new List<string>();
        updater.Drain(said.Add, static _ => { });

        Assert.Equal(
            ["Game database update failed: No route. The game database you had is kept."],
            said);
        Assert.False(updater.IsRunning);
    }

    /// <summary>
    /// Disposing the updater (the plugin switched off) stops a check for
    /// good: a download that arrives afterwards is not written. Mutation:
    /// drop the cancellation test before the write, and it is.
    /// </summary>
    [Fact]
    public async Task DisposeDuringACheckWritesNothing()
    {
        var profiles = new MemoryStorage();
        profiles.Text[VtankGameInfoDatabase.FileName] = ExcerptText;
        var answer = new TaskCompletionSource<VtankGameInfoAnswer>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var transport = new FakeTransport(answer.Task);
        var updater = new VtankGameInfoUpdater(profiles, new MemoryStorage(), transport);
        Assert.Null(updater.Start());
        Task<VtankGameInfoUpdateResult> pending = updater.PendingForTest!;
        Assert.True(SpinWait.SpinUntil(() => transport.Requests.Count == 1, 5000));

        updater.Dispose();
        answer.SetResult(Download(ExcerptTime + OneDay, "Downloaded Arrow"));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => pending);

        Assert.Equal(0, profiles.Writes);
        Assert.False(updater.IsRunning);
    }

    /// <summary>
    /// A whole database as the source publishes one: every table, at the
    /// built-in version, with its world-data time and the ammunition rows
    /// named, changed however the test needs.
    /// </summary>
    internal static VtankGameInfoAnswer Download(
        int time,
        params string[] ammunition) => Download(time, null, ammunition);

    internal static VtankGameInfoAnswer Download(
        int time,
        Action<VtankDatabase>? change,
        params string[] ammunition)
    {
        VtankDatabase database = VtankGameInfoFile.BuiltIn();
        database.Find("DBLastUpdateTime")!.Rows[0].Cells[1] = VtankCell.Int(time);
        foreach (string name in ammunition)
        {
            var row = new VtankRow();
            row.Cells.AddRange(
            [
                VtankCell.String(name), VtankCell.Int(5), VtankCell.Int(0), VtankCell.Int(6),
                VtankCell.Int(1), VtankCell.Int(0), VtankCell.Int(0), VtankCell.Int(0),
            ]);
            database.Find("AmmunitionOptions")!.Rows.Add(row);
        }
        change?.Invoke(database);
        return new VtankGameInfoAnswer(true, database.Render());
    }

    private static async Task<(List<string> Said, List<VtankGameInfoDatabase> Applied)> RunAsync(
        VtankGameInfoUpdater updater,
        TimeSpan skipIfCheckedWithin = default)
    {
        using (updater)
        {
            Assert.Null(updater.Start(skipIfCheckedWithin));
            await updater.PendingForTest!.WaitAsync(TimeSpan.FromSeconds(5));
            var said = new List<string>();
            var applied = new List<VtankGameInfoDatabase>();
            updater.Drain(said.Add, applied.Add);
            return (said, applied);
        }
    }

    internal sealed class FakeTransport : IVtankGameInfoTransport
    {
        private readonly Task<VtankGameInfoAnswer> _answer;

        public FakeTransport(VtankGameInfoAnswer answer)
            : this(Task.FromResult(answer))
        {
        }

        public FakeTransport(Task<VtankGameInfoAnswer> answer) => _answer = answer;

        public List<Uri> Requests { get; } = [];

        public Task<VtankGameInfoAnswer> GetAsync(Uri address, CancellationToken cancellation)
        {
            lock (Requests)
                Requests.Add(address);
            return _answer;
        }
    }

    private sealed class MemoryStorage : IPluginStorage
    {
        public Dictionary<string, string> Text { get; } = new(StringComparer.Ordinal);
        public int Writes { get; private set; }
        public Exception? WriteFailure { get; init; }
        public bool IsAvailable => true;
        public string? ReadText(string key) =>
            Text.TryGetValue(key, out string? value) ? value : null;
        public void WriteText(string key, string content)
        {
            if (WriteFailure is not null)
                throw WriteFailure;
            Writes++;
            Text[key] = content;
        }
    }
}

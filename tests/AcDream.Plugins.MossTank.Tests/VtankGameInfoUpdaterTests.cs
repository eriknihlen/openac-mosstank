using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The game-information update against a transport that answers from the
/// test: what it asks, what it saves, and what it keeps when the answer is
/// no good.
/// </summary>
public sealed class VtankGameInfoUpdaterTests
{
    internal static readonly string ExcerptText = File.ReadAllText(Path.Combine(
        AppContext.BaseDirectory, "Fixtures", "vtank", "gameinfodb-excerpt.ugd"));

    /// <summary>The fixture's own update time: 2023-11-14 22:13:20 UTC.</summary>
    private const int ExcerptTime = 1700000000;

    /// <summary>
    /// With no file, the check asks for everything since 1970 at the
    /// built-in database's version; with one, since the file's own time.
    /// Mutation: send a fixed <c>date=0</c>, and the second request is wrong.
    /// </summary>
    [Fact]
    public async Task TheRequestCarriesTheDatabasesOwnTimeAndVersion()
    {
        var empty = new MemoryStorage();
        var transport = new FakeTransport(Answer(ExcerptTime + 1));
        await RunAsync(new VtankGameInfoUpdater(empty, transport));

        var profiles = new MemoryStorage();
        profiles.Text[VtankGameInfoDatabase.FileName] = ExcerptText;
        await RunAsync(new VtankGameInfoUpdater(profiles, transport));

        Assert.Equal(
            [
                VtankGameInfoUpdater.ServiceAddress + "?date=0&dbver=9",
                VtankGameInfoUpdater.ServiceAddress + "?date=" + ExcerptTime + "&dbver=9",
            ],
            transport.Requests.Select(static address => address.ToString()));
    }

    /// <summary>
    /// A file of another version is the reference client's cue to start
    /// over from its built-in database, so the check asks for everything.
    /// Mutation: take the file whatever its version, and the request carries
    /// its time.
    /// </summary>
    [Fact]
    public async Task AFileOfAnotherVersionIsUpdatedFromTheBuiltInDatabase()
    {
        VtankDatabase old = VtankDatabase.Parse(ExcerptText);
        old.Find("DBVersion")!.Rows[0].Cells[0] = VtankCell.Int(4);
        var profiles = new MemoryStorage();
        profiles.Text[VtankGameInfoDatabase.FileName] = old.Render();
        var transport = new FakeTransport(Answer(ExcerptTime + 1));

        await RunAsync(new VtankGameInfoUpdater(profiles, transport));

        Assert.Equal(
            VtankGameInfoUpdater.ServiceAddress + "?date=0&dbver=9",
            Assert.Single(transport.Requests).ToString());
    }

    /// <summary>
    /// An answer with records beyond its time row is merged in: a row whose
    /// index value matches (without case) overwrites that row, any other row
    /// is added, a table the database does not have is left out. The merged
    /// database is saved, handed over, and said with its record count.
    /// Mutation: leave the existing rows out of the lookup, so every row is
    /// added (the quarrel is there twice).
    /// </summary>
    [Fact]
    public async Task AnUpdateIsMergedSavedAndHandedOver()
    {
        var profiles = new MemoryStorage();
        profiles.Text[VtankGameInfoDatabase.FileName] = ExcerptText;
        var transport = new FakeTransport(Answer(
            ExcerptTime + 60,
            ("AmmunitionOptions", AmmoColumns,
            [
                Ammo("fixture quarrel", quality: 7),
                Ammo("Tested Fire Arrow", launcher: 5, element: 6, quality: 20),
            ]),
            ("NotInTheDatabase", ["Name"], [[VtankCell.String("x")]])));
        var updater = new VtankGameInfoUpdater(profiles, transport);

        (List<string> said, List<VtankGameInfoDatabase> applied) = await RunAsync(updater);

        Assert.Equal("Game database updated: 3 records changed.", said[^1]);
        VtankGameInfoDatabase database = Assert.Single(applied);
        Assert.Equal(ExcerptTime + 60, database.LastUpdateTime);
        Assert.Equal(8, database.AmmunitionOptions.Count);
        VtankAmmunitionOption quarrel = Assert.Single(
            database.AmmunitionOptions,
            static option => option.Name.Equals("Fixture Quarrel", StringComparison.OrdinalIgnoreCase));
        Assert.Equal("Fixture Quarrel", quarrel.Name);
        Assert.Equal(7, quarrel.Quality);
        Assert.Contains(database.AmmunitionOptions, static option => option.Name == "Tested Fire Arrow");

        string saved = profiles.Text[VtankGameInfoDatabase.FileName];
        VtankDatabase reread = VtankDatabase.Parse(saved);
        Assert.Null(reread.Find("NotInTheDatabase"));
        Assert.Equal(8, VtankGameInfoDatabase.Parse(saved).AmmunitionOptions.Count);
        // Everything the excerpt had that the answer did not touch is still there.
        Assert.Equal(
            VtankGameInfoDatabase.Parse(ExcerptText).HealKits,
            VtankGameInfoDatabase.Parse(saved).HealKits);
    }

    /// <summary>
    /// With no file at all, the downloaded tables land in the built-in
    /// database and are saved as the profile folder's first file.
    /// Mutation: merge into an empty database instead of the built-in one,
    /// and the saved file has no DBVersion table.
    /// </summary>
    [Fact]
    public async Task AFirstDownloadIsSavedOnTopOfTheBuiltInDatabase()
    {
        var profiles = new MemoryStorage();
        var transport = new FakeTransport(Answer(
            ExcerptTime,
            ("AmmunitionOptions", AmmoColumns, [Ammo("Tested Fire Arrow", launcher: 5, element: 6)])));

        (List<string> said, List<VtankGameInfoDatabase> applied) =
            await RunAsync(new VtankGameInfoUpdater(profiles, transport));

        Assert.Equal("Game database updated: 1 record changed.", said[^1]);
        VtankGameInfoDatabase saved = VtankGameInfoDatabase.Parse(
            profiles.Text[VtankGameInfoDatabase.FileName]);
        Assert.Equal(9, saved.Version);
        Assert.Equal(ExcerptTime, saved.LastUpdateTime);
        Assert.Equal("Tested Fire Arrow", Assert.Single(saved.AmmunitionOptions).Name);
        Assert.Equal("Tested Fire Arrow", Assert.Single(applied).AmmunitionOptions.Single().Name);
    }

    /// <summary>
    /// An answer holding nothing but its time row means the database is
    /// current. Only the time row is kept, so the file says when it was last
    /// known current; every table keeps what it had. Mutations: count every
    /// row as a change (the line says "updated"); keep nothing for a current
    /// answer (the file keeps its old time and the next login asks again).
    /// </summary>
    [Fact]
    public async Task AnAnswerWithOnlyItsTimeRowMeansCurrentAndKeepsItsTime()
    {
        var profiles = new MemoryStorage();
        profiles.Text[VtankGameInfoDatabase.FileName] = ExcerptText;
        var transport = new FakeTransport(Answer(ExcerptTime + 60));

        (List<string> said, List<VtankGameInfoDatabase> applied) =
            await RunAsync(new VtankGameInfoUpdater(profiles, transport));

        Assert.Equal("Game database is up to date.", said[^1]);
        VtankGameInfoDatabase saved = VtankGameInfoDatabase.Parse(
            profiles.Text[VtankGameInfoDatabase.FileName]);
        Assert.Equal(ExcerptTime + 60, saved.LastUpdateTime);
        Assert.Equal(ExcerptTime + 60, Assert.Single(applied).LastUpdateTime);
        VtankGameInfoDatabase before = VtankGameInfoDatabase.Parse(ExcerptText);
        Assert.Equal(before.AmmunitionOptions, saved.AmmunitionOptions);
        Assert.Equal(before.HealKits, saved.HealKits);
    }

    /// <summary>
    /// A database whose own time is inside the window is not asked about:
    /// the check hands over what the file holds and says when the next one
    /// is due. Past the window, or with no window (a forced check), it asks.
    /// Mutation: ignore the window, and the recent database is asked about.
    /// </summary>
    [Fact]
    public async Task ARecentlyCheckedDatabaseIsNotAskedAboutAgain()
    {
        var profiles = new MemoryStorage();
        profiles.Text[VtankGameInfoDatabase.FileName] = ExcerptText;
        var transport = new FakeTransport(Answer(ExcerptTime + 60));
        DateTimeOffset checkedAt = VtankGameInfoUpdater.FromUnixSeconds(ExcerptTime);
        TimeSpan window = TimeSpan.FromHours(6);

        (List<string> said, List<VtankGameInfoDatabase> applied) = await RunAsync(
            new VtankGameInfoUpdater(profiles, transport, () => checkedAt + TimeSpan.FromHours(1)),
            window);

        Assert.Empty(transport.Requests);
        Assert.Equal(
            "Game database checked recently; the next check is after 2023-11-15 04:13 UTC.",
            said[^1]);
        Assert.Equal(7, Assert.Single(applied).AmmunitionOptions.Count);
        Assert.Equal(0, profiles.Writes);

        await RunAsync(
            new VtankGameInfoUpdater(profiles, transport, () => checkedAt + TimeSpan.FromHours(7)),
            window);
        await RunAsync(
            new VtankGameInfoUpdater(profiles, transport, () => checkedAt + TimeSpan.FromMinutes(1)),
            TimeSpan.Zero);
        Assert.Equal(2, transport.Requests.Count);
    }

    /// <summary>
    /// An answer that is not a game database leaves the old file alone and
    /// says why. Mutation: write the answer before parsing it.
    /// </summary>
    [Fact]
    public async Task AnUnreadableAnswerKeepsTheOldFile()
    {
        var profiles = new MemoryStorage();
        profiles.Text[VtankGameInfoDatabase.FileName] = ExcerptText;
        var transport = new FakeTransport(new VtankGameInfoAnswer(
            true, "<html><body>Service moved</body></html>"));

        (List<string> said, List<VtankGameInfoDatabase> applied) =
            await RunAsync(new VtankGameInfoUpdater(profiles, transport));

        Assert.StartsWith(
            "Game database update failed: the answer is not a game database (",
            said[^1],
            StringComparison.Ordinal);
        Assert.Empty(applied);
        Assert.Equal(0, profiles.Writes);
        Assert.Equal(ExcerptText, profiles.Text[VtankGameInfoDatabase.FileName]);
    }

    /// <summary>
    /// An answer that reads as tables but holds a value that does not read
    /// as its kind (text tagged as a number) is refused whole before anything
    /// is merged or written. Mutation: skip the value check, and the broken
    /// value is merged and saved.
    /// </summary>
    [Fact]
    public async Task AnAnswerWithAValueThatDoesNotReadIsNotWritten()
    {
        var profiles = new MemoryStorage();
        profiles.Text[VtankGameInfoDatabase.FileName] = ExcerptText;
        VtankCell[] broken = Ammo("Broken Arrow");
        broken[1] = new VtankCell { Tag = "i", ScalarText = "bow" };
        var transport = new FakeTransport(Answer(
            ExcerptTime + 60,
            ("AmmunitionOptions", AmmoColumns, [broken])));

        (List<string> said, List<VtankGameInfoDatabase> applied) =
            await RunAsync(new VtankGameInfoUpdater(profiles, transport));

        Assert.Equal(
            "Game database update failed: the answer is not a game database "
            + "(a value in it does not read). The game database you had is kept.",
            said[^1]);
        Assert.Empty(applied);
        Assert.Equal(0, profiles.Writes);
    }

    /// <summary>
    /// A download that cannot be saved is still a good download: it is
    /// handed over and used, and the line says it was not kept. Mutation:
    /// treat a failed save as a failed update, and nothing is handed over.
    /// </summary>
    [Fact]
    public async Task ADownloadThatCannotBeSavedIsStillUsed()
    {
        var profiles = new MemoryStorage { WriteFailure = new IOException("The disk is full.") };
        profiles.Text[VtankGameInfoDatabase.FileName] = ExcerptText;
        var transport = new FakeTransport(Answer(
            ExcerptTime + 60,
            ("AmmunitionOptions", AmmoColumns, [Ammo("Tested Fire Arrow", launcher: 5, element: 6)])));

        (List<string> said, List<VtankGameInfoDatabase> applied) =
            await RunAsync(new VtankGameInfoUpdater(profiles, transport));

        Assert.Equal(
            "Game database downloaded (1 record changed) but could not be saved: "
            + "The disk is full. It is used until MossTank stops.",
            said[^1]);
        Assert.Contains(
            Assert.Single(applied).AmmunitionOptions,
            static option => option.Name == "Tested Fire Arrow");
        Assert.Equal(ExcerptText, profiles.Text[VtankGameInfoDatabase.FileName]);
    }

    /// <summary>
    /// A table is merged on its own index column, not on its first one:
    /// CraftInteractions is keyed on its ID (column 8), so a row with an ID
    /// already there replaces that recipe even when its items differ, and a
    /// row with a new ID is added. Mutation: key every table on column 0, and
    /// the changed recipe is added as a seventh instead of replacing 9002.
    /// </summary>
    [Fact]
    public async Task AMergeKeysOnTheTablesOwnIndexColumn()
    {
        var profiles = new MemoryStorage();
        profiles.Text[VtankGameInfoDatabase.FileName] = ExcerptText;
        string[] columns =
        [
            "UseItem1", "UseItem2", "ResultItem", "ResultCount", "SuccessMsg",
            "FailMsg", "ReqSkill", "ReqDiff", "ID",
        ];
        static VtankCell[] Recipe(string first, string result, int id) =>
        [
            VtankCell.String(first), VtankCell.String("Fixture Binding"),
            VtankCell.String(result), VtankCell.Int(1), VtankCell.String("Made."),
            VtankCell.String("Not made."), VtankCell.Int(21), VtankCell.Int(0),
            VtankCell.Int(id),
        ];
        var transport = new FakeTransport(Answer(
            ExcerptTime + 60,
            ("CraftInteractions", columns,
            [
                Recipe("Fixture Bandages", "Plentiful Healing Kit", 9002),
                Recipe("Fixture Bandages", "Fixture Kit", 9100),
            ])));

        (_, List<VtankGameInfoDatabase> applied) =
            await RunAsync(new VtankGameInfoUpdater(profiles, transport));

        VtankCraftDatabase crafts = Assert.Single(applied).Crafts;
        Assert.Equal(7, crafts.Recipes.Count);
        VtankCraftRecipe kit = Assert.Single(crafts.ForResult("Plentiful Healing Kit"));
        Assert.Equal(("Fixture Bandages", 9002), (kit.FirstItem, kit.Id));
        Assert.Single(crafts.ForResult("Fixture Kit"));
    }

    /// <summary>
    /// A table in the answer whose column count differs from the database's
    /// own is left out whole; the rest of the answer still lands. Mutation:
    /// drop the column-count test, and the merge breaks on the short rows.
    /// </summary>
    [Fact]
    public async Task ATableWhoseColumnsDifferIsLeftOut()
    {
        var profiles = new MemoryStorage();
        profiles.Text[VtankGameInfoDatabase.FileName] = ExcerptText;
        var transport = new FakeTransport(Answer(
            ExcerptTime + 60,
            ("HealKits", ["KitName", "RestoreBonus", "SkillBonus"],
            [[VtankCell.String("Fixture Healing Kit"), VtankCell.Double(9), VtankCell.Int(9)]]),
            ("AmmunitionOptions", AmmoColumns, [Ammo("Tested Fire Arrow", launcher: 5, element: 6)])));

        (List<string> said, List<VtankGameInfoDatabase> applied) =
            await RunAsync(new VtankGameInfoUpdater(profiles, transport));

        Assert.Equal("Game database updated: 2 records changed.", said[^1]);
        VtankGameInfoDatabase database = Assert.Single(applied);
        Assert.Equal(
            VtankGameInfoDatabase.Parse(ExcerptText).HealKits,
            database.HealKits);
        Assert.Contains(database.AmmunitionOptions, static option => option.Name == "Tested Fire Arrow");
    }

    /// <summary>
    /// A transport that throws instead of answering ends the check with the
    /// reason, on the tick, like any other failure. Mutation: pass over a
    /// faulted check in silence, and nothing is said.
    /// </summary>
    [Fact]
    public async Task ATransportThatThrowsSaysWhy()
    {
        var profiles = new MemoryStorage();
        var transport = new FakeTransport(
            Task.FromException<VtankGameInfoAnswer>(new InvalidOperationException("No route.")));
        using var updater = new VtankGameInfoUpdater(profiles, transport);

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
    /// good: an answer that arrives afterwards is not written. Mutation:
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
        var updater = new VtankGameInfoUpdater(profiles, transport);
        Assert.Null(updater.Start());
        Task<VtankGameInfoUpdateResult> pending = updater.PendingForTest!;
        Assert.True(SpinWait.SpinUntil(() => transport.Requests.Count == 1, 5000));

        updater.Dispose();
        answer.SetResult(Answer(
            ExcerptTime + 60,
            ("AmmunitionOptions", AmmoColumns, [Ammo("Tested Fire Arrow", launcher: 5, element: 6)])));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => pending);

        Assert.Equal(0, profiles.Writes);
        Assert.False(updater.IsRunning);
    }

    /// <summary>
    /// A request that fails says the transport's reason and keeps the file.
    /// Mutation: report a failed request as current, and the reason is lost.
    /// </summary>
    [Fact]
    public async Task AFailedRequestSaysWhyAndKeepsTheOldFile()
    {
        var profiles = new MemoryStorage();
        profiles.Text[VtankGameInfoDatabase.FileName] = ExcerptText;
        var transport = new FakeTransport(new VtankGameInfoAnswer(
            false, "HTTP 503 Service Unavailable"));

        (List<string> said, List<VtankGameInfoDatabase> applied) =
            await RunAsync(new VtankGameInfoUpdater(profiles, transport));

        Assert.Equal("Game database update failed: HTTP 503 Service Unavailable. The game database you had is kept.", said[^1]);
        Assert.Empty(applied);
        Assert.Equal(0, profiles.Writes);
    }

    /// <summary>
    /// One check at a time: a second start while one is in flight is
    /// refused, and nothing is said about the first until it ends.
    /// Mutation: drop the in-flight refusal, and the transport is asked twice.
    /// </summary>
    [Fact]
    public async Task OnlyOneCheckRunsAtATime()
    {
        var profiles = new MemoryStorage();
        var answer = new TaskCompletionSource<VtankGameInfoAnswer>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var transport = new FakeTransport(answer.Task);
        using var updater = new VtankGameInfoUpdater(profiles, transport);

        Assert.Null(updater.Start());
        Assert.True(SpinWait.SpinUntil(() => transport.Requests.Count == 1, 5000));
        Assert.Equal("A game database update is already running.", updater.Start());
        var said = new List<string>();
        updater.Drain(said.Add, static _ => { });
        Assert.Empty(said);

        answer.SetResult(Answer(ExcerptTime));
        await updater.PendingForTest!.WaitAsync(TimeSpan.FromSeconds(5));
        updater.Drain(said.Add, static _ => { });

        Assert.Single(transport.Requests);
        Assert.Equal("Game database is up to date.", said[^1]);
        Assert.False(updater.IsRunning);
    }

    /// <summary>
    /// Without a service or a folder to keep the answer in there is no
    /// check, and the refusal says which.
    /// </summary>
    [Fact]
    public void ACheckNeedsAServiceAndAFolder()
    {
        using var noService = new VtankGameInfoUpdater(new MemoryStorage(), null);
        using var noFolder = new VtankGameInfoUpdater(
            NoOpPluginStorage.Instance, new FakeTransport(Answer(0)));

        Assert.False(noService.CanUpdate);
        Assert.False(noFolder.CanUpdate);
        Assert.Equal("Game database updates are not available in this session.", noService.Start());
        Assert.Equal(
            "Game database updates need a VTank profile folder to keep the database in.",
            noFolder.Start());
    }

    internal static readonly string[] AmmoColumns =
    [
        "AmmoName", "LauncherType", "WieldReq", "Element",
        "Quality", "Special", "WieldReq2Skill", "WieldReq2Value",
    ];

    internal static VtankCell[] Ammo(
        string name,
        int launcher = 6,
        int wieldRequirement = 0,
        int element = 0,
        int quality = 1) =>
    [
        VtankCell.String(name),
        VtankCell.Int(launcher),
        VtankCell.Int(wieldRequirement),
        VtankCell.Int(element),
        VtankCell.Int(quality),
        VtankCell.Int(0),
        VtankCell.Int(0),
        VtankCell.Int(0),
    ];

    /// <summary>
    /// An answer as the service writes one: its time row, then each table
    /// with every index flag off.
    /// </summary>
    internal static VtankGameInfoAnswer Answer(
        int time,
        params (string Name, string[] Columns, VtankCell[][] Rows)[] tables)
    {
        var database = new VtankDatabase();
        database.Tables.Add(("DBLastUpdateTime", BuildTable(
            ["Zero", "Time"],
            [[VtankCell.Int(0), VtankCell.Int(time)]])));
        foreach ((string name, string[] columns, VtankCell[][] rows) in tables)
            database.Tables.Add((name, BuildTable(columns, rows)));
        return new VtankGameInfoAnswer(true, database.Render());
    }

    private static VtankTable BuildTable(string[] columns, VtankCell[][] rows)
    {
        var table = new VtankTable();
        table.ColumnNames.AddRange(columns);
        table.IndexFlags.AddRange(columns.Select(static _ => false));
        foreach (VtankCell[] cells in rows)
        {
            var row = new VtankRow();
            row.Cells.AddRange(cells);
            table.Rows.Add(row);
        }
        return table;
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

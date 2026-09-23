using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed partial class MossTankPanelTests
{
    /// <summary>
    /// The session checks the game database by itself, once, as soon as it
    /// is in the world, and every reader of the database takes the new one
    /// on the tick the answer arrives -- combat, crafting, the monster facts
    /// and the heal kit table. Mutations: leave any one reader out of the hand-over
    /// (its database stays the old one); drop the once-a-session latch (the
    /// source is asked again on the next tick).
    /// </summary>
    [Fact]
    public async Task TheSessionChecksTheGameDatabaseOnceAndEveryReaderTakesTheNewOne()
    {
        var automation = new FakeAutomation();
        var profiles = new MemoryStorage();
        var transport = new VtankGameInfoUpdaterTests.FakeTransport(UpdateWithAHealKit());
        var panel = new MossTankPanel(
            new FakeHost(automation, new MemoryStorage(), vtankProfiles: profiles),
            transport);
        VtankGameInfoDatabase before = panel.GameInfoReadersForTest[0];

        panel.OnTick(0.1d);
        await panel.GameInfoUpdatePendingForTest!.WaitAsync(TimeSpan.FromSeconds(5));
        panel.OnTick(0.1d);
        panel.OnTick(0.1d);

        Assert.Null(panel.GameInfoUpdatePendingForTest);
        Assert.Single(transport.Requests);
        Assert.Equal(
            "Game database updated to 2022-11-06 from openac-gamedata.",
            Assert.Single(automation.Messages, static line => line.StartsWith("Game database", StringComparison.Ordinal)));
        IReadOnlyList<VtankGameInfoDatabase> readers = panel.GameInfoReadersForTest;
        Assert.NotSame(before, readers[0]);
        Assert.All(readers, reader => Assert.Same(readers[0], reader));
        Assert.True(panel.HealKitsForTest.ContainsKey("Tested Healing Kit"));
        Assert.NotNull(profiles.ReadText(VtankGameInfoDatabase.FileName));
    }

    /// <summary>
    /// A new session checks again (with no freshness window, so the check
    /// reaches the source). Mutation: never clear the latch at the end of a
    /// session, and the second session asks nothing.
    /// </summary>
    [Fact]
    public async Task EachNewSessionChecksTheGameDatabaseAgain()
    {
        var automation = new FakeAutomation();
        var transport = new VtankGameInfoUpdaterTests.FakeTransport(
            VtankGameInfoUpdaterTests.Download(1));
        var panel = new MossTankPanel(
            new FakeHost(automation, new MemoryStorage(), vtankProfiles: new MemoryStorage()),
            transport);
        Command(panel, "gamedb interval 0");

        panel.OnTick(0.1d);
        await panel.GameInfoUpdatePendingForTest!.WaitAsync(TimeSpan.FromSeconds(5));
        panel.OnTick(0.1d);
        automation.IsAvailable = false;
        panel.OnTick(0.1d);
        automation.IsAvailable = true;
        panel.OnTick(0.1d);
        Assert.NotNull(panel.GameInfoUpdatePendingForTest);
        await panel.GameInfoUpdatePendingForTest!.WaitAsync(TimeSpan.FromSeconds(5));

        Assert.Equal(2, transport.Requests.Count);
    }

    /// <summary>
    /// A source that cannot be reached leaves the session on the database
    /// it had and says why; nothing is written. Mutation: hand over the
    /// built-in database on a failure, and the readers change.
    /// </summary>
    [Fact]
    public async Task AnUnreachableSourceKeepsTheDatabaseTheSessionHad()
    {
        var automation = new FakeAutomation();
        var profiles = new MemoryStorage();
        profiles.Text[VtankGameInfoDatabase.FileName] = VtankGameInfoUpdaterTests.ExcerptText;
        var transport = new VtankGameInfoUpdaterTests.FakeTransport(
            new VtankGameInfoAnswer(false, "No such host is known."));
        var panel = new MossTankPanel(
            new FakeHost(automation, new MemoryStorage(), vtankProfiles: profiles),
            transport);
        VtankGameInfoDatabase before = panel.GameInfoReadersForTest[0];

        panel.OnTick(0.1d);
        await panel.GameInfoUpdatePendingForTest!.WaitAsync(TimeSpan.FromSeconds(5));
        panel.OnTick(0.1d);

        Assert.Contains(
            "Game database update failed: No such host is known. The game database you had is kept.",
            automation.Messages);
        Assert.All(panel.GameInfoReadersForTest, reader => Assert.Same(before, reader));
        Assert.Equal(
            VtankGameInfoUpdaterTests.ExcerptText,
            profiles.ReadText(VtankGameInfoDatabase.FileName));
    }

    /// <summary>
    /// <c>/vt gamedb update</c> checks now, <c>/vt getdb</c> is the
    /// reference's word for the same, and <c>/vt gamedb</c> says what is
    /// loaded. Mutation: leave getdb on its old "nothing to download" line,
    /// and the source is asked only twice.
    /// </summary>
    [Fact]
    public async Task GameDbCommandsCheckNowAndReportWhatIsLoaded()
    {
        var automation = new FakeAutomation();
        var profiles = new MemoryStorage();
        var transport = new VtankGameInfoUpdaterTests.FakeTransport(UpdateWithAHealKit());
        var panel = new MossTankPanel(
            new FakeHost(automation, new MemoryStorage(), vtankProfiles: profiles),
            transport);

        Command(panel, "gamedb");
        Assert.Contains(
            "Game database: no gameinfodb.ugd in the VTank profile folder; /vt gamedb update "
            + "downloads it. Loaded: version 9, world data of none. Ammunition 0, "
            + "monster damage 0, species damage 0, species members 0, immunities 0, "
            + "heal kits 0, grenades 0, drain spells 0, martyr spells 0, craft recipes 0.",
            automation.Messages);
        Assert.Contains(
            "Game database: checked every 6 hours; the next login checks.",
            automation.Messages);

        // The login's own check, then the two asked for.
        panel.OnTick(0.1d);
        await panel.GameInfoUpdatePendingForTest!.WaitAsync(TimeSpan.FromSeconds(5));
        panel.OnTick(0.1d);
        Command(panel, "gamedb update");
        await panel.GameInfoUpdatePendingForTest!.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.Contains("Checking the game database for updates.", automation.Messages);
        panel.OnTick(0.1d);
        Command(panel, "getdb");
        await panel.GameInfoUpdatePendingForTest!.WaitAsync(TimeSpan.FromSeconds(5));
        panel.OnTick(0.1d);
        automation.Messages.Clear();
        Command(panel, "gamedb");

        Assert.Equal(3, transport.Requests.Count);
        Assert.Equal(2, automation.Messages.Count);
        Assert.StartsWith(
            "Game database: gameinfodb.ugd in the VTank profile folder. Loaded: version 9, "
            + "world data of 2022-11-06.",
            automation.Messages[0],
            StringComparison.Ordinal);
        Assert.Contains("heal kits 1,", automation.Messages[0], StringComparison.Ordinal);
        Assert.StartsWith(
            "Game database: checked every 6 hours; the next check is after ",
            automation.Messages[1],
            StringComparison.Ordinal);
    }

    /// <summary>
    /// A panel with no source to fetch from says so instead of trying, and its
    /// session never asks by itself.
    /// </summary>
    [Fact]
    public void WithoutASourceTheUpdateSaysItIsNotAvailable()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(
            new FakeHost(automation, new MemoryStorage(), vtankProfiles: new MemoryStorage()));

        panel.OnTick(0.1d);
        Command(panel, "gamedb update");

        Assert.Null(panel.GameInfoUpdatePendingForTest);
        Assert.Contains("Game database updates are not available in this session.", automation.Messages);
    }

    /// <summary>
    /// A check still running when the session ends is not lost: the next
    /// session lets it finish, hands its database over, and then asks for
    /// itself. Mutation: count the new session as checked before its own
    /// check starts, and the second session never asks.
    /// </summary>
    [Fact]
    public async Task ACheckRunningAtSessionEndIsHandedOverAndTheNextSessionAsksToo()
    {
        var automation = new FakeAutomation();
        var first = new TaskCompletionSource<VtankGameInfoAnswer>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var transport = new SequenceTransport(
            first.Task,
            Task.FromResult(VtankGameInfoUpdaterTests.Download(1667721149)));
        var panel = new MossTankPanel(
            new FakeHost(automation, new MemoryStorage(), vtankProfiles: new MemoryStorage()),
            transport);
        // No freshness window, so the second session's check reaches the source.
        Command(panel, "gamedb interval 0");

        panel.OnTick(0.1d);
        Task<VtankGameInfoUpdateResult> running = panel.GameInfoUpdatePendingForTest!;
        automation.IsAvailable = false;
        panel.OnTick(0.1d);
        automation.IsAvailable = true;
        panel.OnTick(0.1d);
        Assert.Same(running, panel.GameInfoUpdatePendingForTest);

        first.SetResult(UpdateWithAHealKit());
        await running.WaitAsync(TimeSpan.FromSeconds(5));
        panel.OnTick(0.1d);

        Assert.True(panel.HealKitsForTest.ContainsKey("Tested Healing Kit"));
        Assert.Contains("Game database updated to 2022-11-06 from openac-gamedata.", automation.Messages);
        Assert.NotNull(panel.GameInfoUpdatePendingForTest);
        await panel.GameInfoUpdatePendingForTest!.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.Equal(2, transport.Requests);
    }

    private sealed class SequenceTransport(params Task<VtankGameInfoAnswer>[] answers)
        : IVtankGameInfoTransport
    {
        private int _requests;

        public int Requests => Volatile.Read(ref _requests);

        public Task<VtankGameInfoAnswer> GetAsync(Uri address, CancellationToken cancellation) =>
            answers[Interlocked.Increment(ref _requests) - 1];
    }

    /// <summary>
    /// A login skips the check while the last one is younger than the
    /// preference's interval, and says so in one line; <c>/vt gamedb interval
    /// 0</c> makes every login ask again, and the preference is kept.
    /// Mutations: start the login's check with no window (the recent
    /// check is repeated); ignore the stored interval (the second
    /// session still skips).
    /// </summary>
    [Fact]
    public async Task ALoginSkipsTheCheckWhileTheLastOneIsRecent()
    {
        var automation = new FakeAutomation();
        var storage = new MemoryStorage();
        var profiles = new MemoryStorage();
        profiles.Text[VtankGameInfoDatabase.FileName] = VtankGameInfoUpdaterTests.ExcerptText;
        storage.Text[VtankGameInfoUpdater.LastCheckKey] = DateTimeOffset.UtcNow.AddHours(-1)
            .ToUnixTimeSeconds().ToString(System.Globalization.CultureInfo.InvariantCulture);
        var transport = new VtankGameInfoUpdaterTests.FakeTransport(
            VtankGameInfoUpdaterTests.Download(1700000000));
        var panel = new MossTankPanel(
            new FakeHost(automation, storage, vtankProfiles: profiles),
            transport);

        panel.OnTick(0.1d);
        await panel.GameInfoUpdatePendingForTest!.WaitAsync(TimeSpan.FromSeconds(5));
        panel.OnTick(0.1d);

        Assert.Empty(transport.Requests);
        Assert.Single(
            automation.Messages,
            static line => line.StartsWith(
                "Game database checked recently; the next check is after ",
                StringComparison.Ordinal));

        Command(panel, "gamedb interval 0");
        Assert.Contains(
            "Game database: checked at every login (/vt gamedb interval sets how often).",
            automation.Messages);
        Assert.Contains(
            "\"GameDbCheckIntervalHours\": 0",
            storage.Text["profiles/macro/preferences.json"],
            StringComparison.Ordinal);
        automation.IsAvailable = false;
        panel.OnTick(0.1d);
        automation.IsAvailable = true;
        panel.OnTick(0.1d);
        await panel.GameInfoUpdatePendingForTest!.WaitAsync(TimeSpan.FromSeconds(5));

        Assert.Single(transport.Requests);
    }

    private static VtankGameInfoAnswer UpdateWithAHealKit() =>
        VtankGameInfoUpdaterTests.Download(
            1667721149,
            change: static database =>
            {
                var kit = new VtankRow();
                kit.Cells.AddRange(
                [
                    VtankCell.String("Tested Healing Kit"),
                    VtankCell.Double(1.5),
                    VtankCell.Int(10),
                    VtankCell.Int(0),
                ]);
                database.Find("HealKits")!.Rows.Add(kit);
            });
}

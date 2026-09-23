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
    /// service is asked again on the next tick).
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
            "Game database updated: 1 record changed.",
            Assert.Single(automation.Messages, static line => line.StartsWith("Game database", StringComparison.Ordinal)));
        IReadOnlyList<VtankGameInfoDatabase> readers = panel.GameInfoReadersForTest;
        Assert.NotSame(before, readers[0]);
        Assert.All(readers, reader => Assert.Same(readers[0], reader));
        Assert.True(panel.HealKitsForTest.ContainsKey("Tested Healing Kit"));
        Assert.NotNull(profiles.ReadText(VtankGameInfoDatabase.FileName));
    }

    /// <summary>
    /// A new session checks again. Mutation: never clear the latch at the end
    /// of a session, and the second session asks nothing.
    /// </summary>
    [Fact]
    public async Task EachNewSessionChecksTheGameDatabaseAgain()
    {
        var automation = new FakeAutomation();
        var transport = new VtankGameInfoUpdaterTests.FakeTransport(
            VtankGameInfoUpdaterTests.Answer(1));
        var panel = new MossTankPanel(
            new FakeHost(automation, new MemoryStorage(), vtankProfiles: new MemoryStorage()),
            transport);

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
    /// A service that cannot be reached leaves the session on the database
    /// it had and says why; nothing is written. Mutation: hand over the
    /// built-in database on a failure, and the readers change.
    /// </summary>
    [Fact]
    public async Task AnUnreachableServiceKeepsTheDatabaseTheSessionHad()
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
    /// and the service is asked only once.
    /// </summary>
    [Fact]
    public async Task GameDbCommandsCheckNowAndReportWhatIsLoaded()
    {
        var automation = new FakeAutomation { IsAvailable = false };
        var profiles = new MemoryStorage();
        var transport = new VtankGameInfoUpdaterTests.FakeTransport(UpdateWithAHealKit());
        var panel = new MossTankPanel(
            new FakeHost(automation, new MemoryStorage(), vtankProfiles: profiles),
            transport);

        Command(panel, "gamedb");
        Assert.Contains(
            "Game database: no gameinfodb.ugd in the VTank profile folder; /vt gamedb update "
            + "downloads it. Loaded: version 9, last updated never. Ammunition 0, "
            + "monster damage 0, species damage 0, species members 0, immunities 0, "
            + "heal kits 0, drain spells 0, martyr spells 0, craft recipes 0.",
            automation.Messages);

        Command(panel, "gamedb update");
        await panel.GameInfoUpdatePendingForTest!.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.Contains("Checking the game database for updates.", automation.Messages);
        automation.IsAvailable = true;
        panel.OnTick(0.1d);
        Command(panel, "getdb");
        await panel.GameInfoUpdatePendingForTest!.WaitAsync(TimeSpan.FromSeconds(5));
        panel.OnTick(0.1d);
        automation.Messages.Clear();
        Command(panel, "gamedb");

        Assert.Equal(2, transport.Requests.Count);
        string status = Assert.Single(automation.Messages);
        Assert.StartsWith(
            "Game database: gameinfodb.ugd in the VTank profile folder. Loaded: version 9, "
            + "last updated 2022-11-06 07:52 UTC.",
            status,
            StringComparison.Ordinal);
        Assert.Contains("heal kits 1,", status, StringComparison.Ordinal);
    }

    /// <summary>
    /// A panel with no service to ask says so instead of trying, and its
    /// session never asks by itself.
    /// </summary>
    [Fact]
    public void WithoutAServiceTheUpdateSaysItIsNotAvailable()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(
            new FakeHost(automation, new MemoryStorage(), vtankProfiles: new MemoryStorage()));

        panel.OnTick(0.1d);
        Command(panel, "gamedb update");

        Assert.Null(panel.GameInfoUpdatePendingForTest);
        Assert.Contains("Game database updates are not available in this session.", automation.Messages);
    }

    private static VtankGameInfoAnswer UpdateWithAHealKit() =>
        VtankGameInfoUpdaterTests.Answer(
            1667721149,
            ("HealKits", ["KitName", "RestoreBonus", "SkillBonus", "WhichVital"],
            [
                [
                    VtankCell.String("Tested Healing Kit"),
                    VtankCell.Double(1.5),
                    VtankCell.Int(10),
                    VtankCell.Int(0),
                ],
            ]));
}

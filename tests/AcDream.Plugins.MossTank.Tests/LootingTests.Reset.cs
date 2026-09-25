using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// What the looter keeps and forgets across a reset: the chat already
/// written is not read again, and with the own-rare walk on, an emptied
/// corpse of this character's rare stays emptied.
/// </summary>
public sealed partial class LootingTests
{
    /// <summary>
    /// After a reset the looter reads only chat written after it, so an old
    /// rare announcement is not heard a second time.
    ///
    /// Mutation: put the chat cursor back to the start on reset and the
    /// announcement from before it is reported again.
    /// </summary>
    [Fact]
    public void AResetDoesNotReadTheChatAlreadyWrittenAgain()
    {
        var settings = new LootSettings { Enabled = true, LootOnlyRareCorpses = true };
        settings.Rules.Add(new LootRule { Expression = "*" });
        var automation = new Automation();
        var logged = new List<string>();
        var controller = new LootController(new Host(automation), settings)
        {
            Log = (_, message) => logged.Add(message),
        };
        Announce(automation, 5uL, "Tester has discovered the Warrior's Crystal!");
        controller.TickIdentification(0.1d);
        Assert.Single(logged, line => line.Contains("rare announced", StringComparison.Ordinal));

        controller.Reset();
        controller.TickIdentification(0.1d);

        Assert.Single(logged, line => line.Contains("rare announced", StringComparison.Ordinal));
        Announce(automation, 6uL, "Tester has discovered the Pearl of Blood Drinking!");
        controller.TickIdentification(0.1d);
        Assert.Equal(2, logged.Count(line => line.Contains("rare announced", StringComparison.Ordinal)));
    }

    /// <summary>
    /// With the own-rare walk on, a corpse of this character's rare that was
    /// emptied stays emptied across a reset, so the walk does not go back to
    /// it. With the option off a reset forgets it like any other corpse.
    ///
    /// Mutation: forget every completed corpse on reset and the emptied rare
    /// is the walk's pick again with the option on.
    /// </summary>
    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public void AnEmptiedOwnRareCorpseStaysEmptiedAcrossAResetWithTheOptionOn(bool walkToOwnRares)
    {
        LootSettings settings = ApproachSettings(range: 30d, walkToOwnRares);
        const uint rare = 0x70002050u;
        var automation = ApproachAutomation(OwnRareCorpse(rare, 3f));
        var controller = new LootController(new Host(automation), settings);

        Assert.True(controller.Tick(1d, canAct: true));
        Assert.Equal(new[] { rare }, automation.Opened);
        automation.Requested = rare;
        automation.Current = rare;
        automation.Contents = [];
        for (int pass = 0; pass < 60 && controller.Status != "Corpse complete."; pass++)
        {
            controller.TickIdentification(0.25d);
            controller.Tick(0.25d, canAct: true);
        }
        Assert.Equal("Corpse complete.", controller.Status);
        automation.Current = 0u;
        Assert.False(controller.TrySelectApproachCorpse(30d, out _));

        controller.Reset();

        Assert.Equal(
            !walkToOwnRares,
            controller.TrySelectApproachCorpse(30d, out _));
    }
}

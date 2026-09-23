using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// A corpse the server no longer has (it decayed while the character was
/// elsewhere) is answered with no description. Its answer is in: it no
/// longer holds the walks off, it is not asked about again, and with no
/// description to say who killed it, it is passed over.
/// </summary>
public sealed partial class LootingTests
{
    /// <summary>
    /// Mutations: wait on a corpse until it is described (the answered one
    /// holds the walks off for good); ask again about a corpse until it is
    /// described (it is asked on every turn).
    /// </summary>
    [Fact]
    public void ACorpseAnsweredWithoutADescriptionIsNeitherWaitedOnNorAskedAgain()
    {
        const uint decayed = 0x7000F101u;
        var automation = ApproachAutomation(
            Corpse(decayed, 3f) with { IsIdentified = false, IsAppraisalAnswered = true });
        var looter = new LootController(
            new Host(automation),
            DescriptionGiveUpSettings());

        Assert.False(looter.HasCorpseAwaitingDescriptionWithin(15d));
        looter.TickIdentification(0.25d);
        looter.TickIdentification(4d);

        Assert.Empty(automation.Identified);
        Assert.False(looter.HasCorpseAwaitingDescriptionWithin(15d));
    }

    /// <summary>
    /// Where the path ends: with no description, the corpse reads as a rare
    /// someone else killed (no "Killed by" line), which no setting loots, so
    /// it is never walked to or opened and nothing waits on it, however long
    /// it lies there. Mutation: wait on a corpse until it is described, and
    /// it holds the walks off throughout.
    /// </summary>
    [Fact]
    public void ACorpseAnsweredWithoutADescriptionIsPassedOver()
    {
        const uint decayed = 0x7000F111u;
        var settings = new LootSettings
        {
            Enabled = true,
            LootAllCorpses = true,
            LootFellowCorpses = true,
            ScanIntervalSeconds = 0.05d,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        var automation = new Automation
        {
            Corpses =
            [
                new PluginLootContainer(decayed, 1u, "Corpse", 3f, false, false, false)
                {
                    IsAppraisalAnswered = true,
                },
            ],
        };
        var controller = new LootController(new Host(automation), settings);

        foreach (double elapsed in new[] { 0.1d, 100.5d, 0.3d, 600d })
        {
            Assert.False(controller.Tick(elapsed, canAct: true));
            controller.TickIdentification(elapsed);
            Assert.False(controller.HasCorpseAwaitingDescriptionWithin(15d));
        }

        Assert.Empty(automation.Opened);
        Assert.Empty(automation.Identified);
        Assert.Equal("No nearby corpses.", controller.Status);
    }
}

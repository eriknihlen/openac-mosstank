namespace AcDream.Plugins.MossTank.Tests;

public sealed partial class LootingTests
{
    [Fact]
    public void FinishedCorpseClosesThroughTheLootSurface()
    {
        const uint corpse = 0x7000C001u;
        var settings = new LootSettings
        {
            Enabled = true,
            CorpseItemAppearanceTimeoutSeconds = 0d,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        var automation = ApproachAutomation(Corpse(corpse, 3f));
        var looter = new LootController(new Host(automation), settings);

        Assert.True(looter.Tick(0.25d, canAct: true));
        automation.Requested = corpse;
        automation.Current = corpse;
        Assert.True(looter.Tick(0.25d, canAct: true));

        Assert.Equal([corpse], automation.Closed);
        Assert.Empty(automation.Used);
    }
}

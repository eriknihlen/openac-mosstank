using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed partial class LootingTests
{
    [Fact]
    public void DescribedCorpseOpensWhileAnotherDescriptionIsPending()
    {
        const uint unanswered = 0x7000D001u;
        const uint ready = 0x7000D002u;
        const uint somethingElse = 0x7000D003u;
        var settings = DescriptionRetrySettings();
        var automation = ApproachAutomation(
            Corpse(unanswered, 3f) with { IsIdentified = false });
        var looter = new LootController(new Host(automation), settings);

        looter.TickIdentification(0.25d);
        Assert.Equal([unanswered], automation.Identified);

        automation.Corpses =
        [
            Corpse(unanswered, 3f) with { IsIdentified = false },
            Corpse(ready, 3f),
        ];
        Assert.True(looter.Tick(0.25d, canAct: true));
        Assert.Equal([ready], automation.Opened);

        automation.Requested = 0u;
        automation.Current = 0u;
        automation.Corpses =
            [Corpse(unanswered, 3f) with { IsIdentified = false }];
        automation.AppraisalState = new PluginAppraisalState(
            automation.AppraisalState.Revision + 1,
            0u,
            somethingElse);
        looter.TickIdentification(4d);
        Assert.Equal([unanswered, unanswered], automation.Identified);
    }

    [Fact]
    public void UnansweredCorpseDescriptionsRotateFairly()
    {
        const uint first = 0x7000D011u;
        const uint second = 0x7000D012u;
        const uint somethingElse = 0x7000D013u;
        var automation = ApproachAutomation(
            Corpse(first, 3f) with { IsIdentified = false },
            Corpse(second, 3f) with { IsIdentified = false });
        var looter = new LootController(
            new Host(automation),
            DescriptionRetrySettings());

        looter.TickIdentification(0.25d);
        Assert.Equal([first], automation.Identified);

        automation.AppraisalState = new PluginAppraisalState(
            automation.AppraisalState.Revision + 1,
            0u,
            somethingElse);
        looter.TickIdentification(4d);
        Assert.Equal([first, second], automation.Identified);

        automation.AppraisalState = new PluginAppraisalState(
            automation.AppraisalState.Revision + 1,
            0u,
            somethingElse);
        looter.TickIdentification(4d);
        Assert.Equal([first, second, first], automation.Identified);
    }

    [Fact]
    public void ACorpseWhoseDescriptionIsOvertakenIsAskedAgainNotWrittenOff()
    {
        const uint corpse = 0x7000E001u;
        const uint somethingElse = 0x7000E002u;
        var settings = DescriptionRetrySettings();
        var automation = ApproachAutomation(
            Corpse(corpse, 3f) with { IsIdentified = false });
        var looter = new LootController(new Host(automation), settings);

        looter.TickIdentification(0.25d);
        Assert.Equal([corpse], automation.Identified);

        automation.AppraisalState = new PluginAppraisalState(
            automation.AppraisalState.Revision + 1,
            0u,
            somethingElse);

        looter.TickIdentification(4d);
        looter.TickIdentification(0.25d);
        Assert.Equal([corpse, corpse], automation.Identified);

        automation.Corpses = [Corpse(corpse, 3f)];
        automation.AppraisalState = new PluginAppraisalState(
            automation.AppraisalState.Revision + 1,
            0u,
            corpse);
        Assert.True(looter.Tick(0.25d, canAct: true));
        Assert.Equal([corpse], automation.Opened);
    }

    private static LootSettings DescriptionRetrySettings()
    {
        var settings = new LootSettings
        {
            Enabled = true,
            ScanIntervalSeconds = 0.05d,
            CorpseOpenTimeoutSeconds = 1.5d,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        return settings;
    }
}

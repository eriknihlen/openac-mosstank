using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// What happens when the server never describes one corpse. The macro must
/// not stop: the description channel goes on serving everybody else, the
/// corpse nobody can describe is let go of, and it stops holding the walks
/// off. Its counterpart — a corpse whose description is genuinely on its way
/// still holds them — is pinned here too, because that is the behaviour the
/// give-up must not cost.
/// </summary>
public sealed partial class LootingTests
{
    [Fact]
    public void ACorpseThatNeverAnswersStopsHoldingTheWalksOff()
    {
        const uint silent = 0x7000F001u;
        var automation = ApproachAutomation(
            Corpse(silent, 3f) with { IsIdentified = false });
        automation.OneDescriptionAtATime = true;
        automation.NeverAnswers.Add(silent);
        var log = new List<(MacroLogChannel Channel, string Text)>();
        var looter = new LootController(
            new Host(automation),
            DescriptionGiveUpSettings())
        {
            Log = (channel, text) => log.Add((channel, text)),
        };

        // Three questions, three answers that never came. Only the third
        // gives the corpse up.
        for (int round = 0; round < 3; round++)
        {
            looter.TickIdentification(0.25d);
            Assert.True(looter.HasCorpseAwaitingDescriptionWithin(15d));
            automation.ReleaseStalledDescription();
            looter.TickIdentification(4d);
        }

        Assert.Equal(3, automation.Identified.Count);
        Assert.False(looter.HasCorpseAwaitingDescriptionWithin(15d));
        Assert.Contains(
            log,
            entry => entry.Channel == MacroLogChannel.Loot
                && entry.Text.Contains("Skipping corpse 0x7000F001", StringComparison.Ordinal));

        // And it is not asked again while it is skipped.
        looter.TickIdentification(4d);
        Assert.Equal(3, automation.Identified.Count);
    }

    [Fact]
    public void ACorpseWhoseDescriptionIsOnItsWayStillHoldsTheWalksOff()
    {
        const uint corpse = 0x7000F011u;
        var automation = ApproachAutomation(
            Corpse(corpse, 3f) with { IsIdentified = false });
        automation.OneDescriptionAtATime = true;
        automation.NeverAnswers.Add(corpse);
        var looter = new LootController(
            new Host(automation),
            DescriptionGiveUpSettings());

        looter.TickIdentification(0.25d);

        // Nothing has failed yet: the question is outstanding and inside its
        // own bound, so the character waits where it is.
        Assert.True(looter.HasCorpseAwaitingDescriptionWithin(15d));
        looter.TickIdentification(0.25d);
        Assert.True(looter.HasCorpseAwaitingDescriptionWithin(15d));
        Assert.Single(automation.Identified);
    }

    [Fact]
    public void ASilentCorpseDoesNotStarveTheCorpsesBehindIt()
    {
        const uint silent = 0x7000F021u;
        const uint next = 0x7000F022u;
        var automation = ApproachAutomation(
            Corpse(silent, 3f) with { IsIdentified = false },
            Corpse(next, 3f) with { IsIdentified = false });
        automation.OneDescriptionAtATime = true;
        automation.NeverAnswers.Add(silent);
        var looter = new LootController(
            new Host(automation),
            DescriptionGiveUpSettings());

        looter.TickIdentification(0.25d);
        Assert.Equal([silent], automation.Identified);

        // The client gives up on the silent one; the very next turn belongs
        // to the corpse behind it, which is answered at once.
        automation.ReleaseStalledDescription();
        looter.TickIdentification(4d);

        Assert.Equal([silent, next], automation.Identified);
        Assert.Equal(next, automation.AppraisalState.CurrentObjectId);
    }

    [Fact]
    public void AnItemDescriptionOutstandingWhenTheCorpseShutsDoesNotStopTheNextCorpse()
    {
        const uint corpse = 0x7000F031u;
        const uint item = 0x7000F032u;
        const uint later = 0x7000F033u;
        var automation = ApproachAutomation(Corpse(corpse, 3f));
        automation.OneDescriptionAtATime = true;
        automation.NeverAnswers.Add(item);
        // A profile that has to look at an item before it can decide about
        // it — a catch-all never asks for a description at all.
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(new LootRule
        {
            Name = "Coins",
            Expression = "name ~= coin",
            Action = LootAction.Keep,
            Priority = 3,
        });
        var looter = new LootController(new Host(automation), settings);

        // The open corpse's item is asked about and never answered.
        Assert.True(looter.Tick(0.25d, canAct: true));
        Assert.Equal([corpse], automation.Opened);
        automation.Requested = corpse;
        automation.Current = corpse;
        automation.Contents = [Item(item, "Colosseum coin", 77)];
        looter.TickIdentification(0.25d);
        Assert.Equal([item], automation.Identified);

        // The corpse shuts with the question still outstanding. The looter
        // used to hold that item latch for ever and never describe another
        // corpse for the rest of the session.
        automation.Current = 0u;
        automation.Contents = [];
        automation.ReleaseStalledDescription();
        automation.Corpses =
        [
            Corpse(corpse, 3f),
            Corpse(later, 3f) with { IsIdentified = false },
        ];

        looter.TickIdentification(0.25d);
        Assert.Equal([item, later], automation.Identified);
    }

    private static LootSettings DescriptionGiveUpSettings()
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

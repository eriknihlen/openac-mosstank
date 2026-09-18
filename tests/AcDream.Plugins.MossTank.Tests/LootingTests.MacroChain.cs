using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The corpse chain as the reference schedules it: the open rule holds the
/// pass while the item slot is held, descriptions and item ids are asked on
/// the frame, a pull is counted when it is issued, and a corpse is selected
/// on every turn. See the research note on the VTank macro state machine.
/// </summary>
public sealed partial class LootingTests
{
    private static LootSettings ChainSettings(string expression = "*")
    {
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(new LootRule
        {
            Name = "Rule",
            Expression = expression,
            Action = LootAction.Keep,
            Priority = 1,
        });
        return settings;
    }

    private static PluginLootContainer ChainCorpse(uint id, float distance = 3f) =>
        new(id, 1u, "Corpse", distance, false, false, false)
        {
            IsIdentified = true,
            LongDescription = "Killed by Tester.",
        };

    /// <summary>
    /// The reference's open rule is VALID while the item slot is held, and its
    /// turn does nothing: the pass is held through an open attempt and
    /// through every pull's window. Mutation: delete the item-slot hold at
    /// the top of <c>Tick</c> and the second turn pulls the coin again before
    /// the slot's window has run out.
    /// </summary>
    [Fact]
    public void TheRuleHoldsThePassWhileTheItemSlotIsHeld()
    {
        const uint corpse = 0x70001101u;
        const uint coin = 0x70001102u;
        var automation = new Automation { Corpses = [ChainCorpse(corpse)] };
        var locks = new ActionLockTable();
        var controller = new LootController(new Host(automation), ChainSettings());
        controller.BindActionLocks(locks);

        Assert.True(controller.Tick(0.3d, canAct: true));
        Assert.Equal([corpse], automation.Opened);
        automation.Requested = corpse;
        automation.Current = corpse;
        Assert.True(controller.ObserveCorpseOpened());
        automation.Contents = [Item(coin, "Colosseum coin", 77)];
        controller.TickIdentification(0.5d);

        Assert.True(controller.Tick(0.3d, canAct: true));
        Assert.Equal([coin], automation.Picked);
        Assert.True(locks.IsLocked(ActionLockKind.ItemUse));

        // The slot is up: the turn is held and nothing is pulled.
        Assert.True(controller.Tick(0.3d, canAct: true));
        Assert.Equal([coin], automation.Picked);

        // The slot's window runs out; the coin is still in the corpse, so it
        // is pulled again.
        locks.Advance(0.8d);
        Assert.True(controller.Tick(0.3d, canAct: true));
        Assert.Equal([coin, coin], automation.Picked);
    }

    /// <summary>
    /// A corpse's description is asked on the frame, never on the rule's
    /// turn: the turn finds nothing to open and says so. Before the port the
    /// turn sent the request itself and claimed the pass for it. Mutation:
    /// call <c>TryRequestNextCorpseDescription</c> from <c>Tick</c> again.
    /// </summary>
    [Fact]
    public void TheRulesTurnNeverAsksForACorpseDescription()
    {
        const uint corpse = 0x70001111u;
        var automation = new Automation
        {
            Corpses = [ChainCorpse(corpse) with { IsIdentified = false }],
        };
        var controller = new LootController(new Host(automation), ChainSettings());

        Assert.False(controller.Tick(0.3d, canAct: true));
        Assert.Empty(automation.Identified);
        Assert.Empty(automation.Opened);

        controller.TickIdentification(0.1d);
        Assert.Equal([corpse], automation.Identified);
    }

    /// <summary>
    /// An item that needs an id is asked for it on the frame and decided on
    /// the frame the answer lands, with no rule turn in between; the next
    /// turn works from that decision. Mutation: drop the
    /// <c>TickCorpseItemIdentification</c> call from
    /// <c>TickIdentification</c> and nothing is ever asked.
    /// </summary>
    [Fact]
    public void AnItemIsDescribedAndDecidedOnTheFrameWithoutARuleTurn()
    {
        const uint corpse = 0x70001121u;
        const uint coin = 0x70001122u;
        var automation = new Automation { Corpses = [ChainCorpse(corpse)] };
        var logged = new List<string>();
        var controller = new LootController(
            new Host(automation),
            ChainSettings("name ~= coin"))
        {
            Log = (_, message) => logged.Add(message),
        };

        Assert.True(controller.Tick(0.3d, canAct: true));
        automation.Requested = corpse;
        automation.Current = corpse;
        automation.Contents = [Item(coin, "Colosseum coin", 77)];

        controller.TickIdentification(0.1d);
        Assert.Equal([coin], automation.Identified);
        Assert.DoesNotContain(
            logged,
            line => line.StartsWith("LootDecision: ", StringComparison.Ordinal));

        automation.CompleteAppraisal(coin, presentInUi: false);
        controller.TickIdentification(0.1d);
        Assert.Contains(
            logged,
            line => line.StartsWith("LootDecision: Colosseum coin -> Keep", StringComparison.Ordinal));
        Assert.Empty(automation.Picked);

        Assert.True(controller.Tick(0.3d, canAct: true));
        Assert.Equal([coin], automation.Picked);
    }

    /// <summary>
    /// The reference counts a pull when it is issued and drops the item once
    /// the profile's attempt ceiling is reached; the corpse then closes.
    /// Mutation: remove the <c>IncrementAttempt</c> at the pull and the coin
    /// is pulled a third time instead of the corpse closing.
    /// </summary>
    [Fact]
    public void APullIsCountedWhenItIsIssuedAndDroppedAtTheAttemptCeiling()
    {
        const uint corpse = 0x70001131u;
        const uint coin = 0x70001132u;
        LootSettings settings = ChainSettings();
        settings.CorpseLootItemMaxAttempts = 2;
        var automation = new Automation { Corpses = [ChainCorpse(corpse)] };
        var controller = new LootController(new Host(automation), settings);

        Assert.True(controller.Tick(0.3d, canAct: true));
        automation.Requested = corpse;
        automation.Current = corpse;
        automation.Contents = [Item(coin, "Colosseum coin", 77)];
        controller.TickIdentification(0.5d);

        Assert.True(controller.Tick(0.3d, canAct: true));
        Assert.True(controller.Tick(0.3d, canAct: true));
        Assert.Equal([coin, coin], automation.Picked);

        Assert.True(controller.Tick(0.3d, canAct: true));
        Assert.Equal([coin, coin], automation.Picked);
        Assert.Equal([corpse], automation.Closed);
    }

    /// <summary>
    /// The local server writes the killer's name without the plus sign an
    /// admin character carries; the character's own plus is not part of the
    /// comparison (divergence register TS-101). Mutation: compare the name as
    /// given and the corpse is never opened.
    /// </summary>
    [Fact]
    public void AKillCreditedWithoutThePlusIsStillOurs()
    {
        const uint corpse = 0x70001161u;
        var automation = new Automation
        {
            Name = "+Tester",
            Corpses = [ChainCorpse(corpse)],
        };
        var controller = new LootController(new Host(automation), ChainSettings());

        Assert.True(controller.Tick(0.3d, canAct: true));
        Assert.Equal([corpse], automation.Opened);
    }

    /// <summary>
    /// The reference selects a corpse on every turn it is asked; there is no
    /// scan interval to wait out. With a five-second interval configured, a
    /// second corpse is still opened on the very next turn after the first
    /// closes. Before the port the turn waited the interval out.
    /// </summary>
    [Fact]
    public void ASecondCorpseIsOpenedOnTheVeryNextTurnWithoutAScanWait()
    {
        const uint first = 0x70001141u;
        const uint second = 0x70001142u;
        LootSettings settings = ChainSettings();
        settings.ScanIntervalSeconds = 5d;
        settings.CorpseItemAppearanceTimeoutSeconds = 0d;
        var automation = new Automation { Corpses = [ChainCorpse(first)] };
        var controller = new LootController(new Host(automation), settings);

        Assert.True(controller.Tick(0.3d, canAct: true));
        Assert.Equal([first], automation.Opened);
        automation.Requested = first;
        automation.Current = first;
        Assert.True(controller.Tick(0.3d, canAct: true));
        Assert.Equal([first], automation.Closed);

        automation.Requested = 0u;
        automation.Current = 0u;
        automation.Corpses = [ChainCorpse(second)];
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([first, second], automation.Opened);
    }

    /// <summary>
    /// Item ids go out one at a time on the reference's cadence, 499 ms
    /// between requests. Mutation: drop the cadence check and the second coin
    /// is asked for a tenth of a second after the first was answered.
    /// </summary>
    [Fact]
    public void ItemIdRequestsFollowTheReferenceCadence()
    {
        const uint corpse = 0x70001151u;
        const uint first = 0x70001152u;
        const uint second = 0x70001153u;
        var automation = new Automation { Corpses = [ChainCorpse(corpse)] };
        var controller = new LootController(
            new Host(automation),
            ChainSettings("name ~= coin"));

        Assert.True(controller.Tick(0.3d, canAct: true));
        automation.Requested = corpse;
        automation.Current = corpse;
        automation.Contents =
        [
            Item(first, "Colosseum coin", 77),
            Item(second, "Colosseum coin", 77),
        ];

        controller.TickIdentification(0d);
        Assert.Equal([first], automation.Identified);

        automation.CompleteAppraisal(first, presentInUi: false);
        controller.TickIdentification(0.1d);
        Assert.Equal([first], automation.Identified);

        controller.TickIdentification(0.4d);
        Assert.Equal([first, second], automation.Identified);
    }
}

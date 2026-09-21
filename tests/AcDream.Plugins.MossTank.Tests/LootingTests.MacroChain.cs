using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The corpse chain as the reference schedules it: the open rule holds the
/// pass while the item slot is held, descriptions and item ids are asked on
/// the frame, a pull is counted when it is issued, and a corpse is selected
/// on every turn. See the research note on the macro state machine.
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
    /// comparison. Mutation: compare the name as
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
    /// The reference arms the item slot, navigation and the corpse-open
    /// slot after EVERY open attempt, whether the use went out or came
    /// back refused, and that slot is what paces the retries. Without it
    /// the attempt ceiling is spent at the heartbeat -- thirty tries in a
    /// few seconds -- and a corpse that would have opened on the next pass
    /// is written off for the profile's blacklist timeout. Mutation: drop
    /// the <c>ArmCorpseOpenSlots</c> call from the refused-open branch and
    /// the second attempt lands on the very next turn.
    /// </summary>
    [Fact]
    public void ARefusedOpenPacesTheNextAttemptOnTheOpenSlot()
    {
        const uint corpse = 0x70001172u;
        LootSettings settings = ChainSettings();
        settings.CorpseOpenTimeoutSeconds = 1.5d;
        var automation = new Automation
        {
            Corpses = [ChainCorpse(corpse)],
            OpenResult = PluginItemCommandStatus.Refused,
        };
        var controller = new LootController(new Host(automation), settings);
        var locks = new ActionLockTable();
        controller.BindActionLocks(locks);

        controller.Tick(0.3d, canAct: true);
        Assert.Equal([corpse], automation.Opened);
        Assert.True(locks.IsLocked(ActionLockKind.CorpseOpenAttempt));

        // Inside the slot the rule holds the pass and attempts nothing.
        Assert.True(controller.Tick(0.3d, canAct: true));
        Assert.Equal([corpse], automation.Opened);

        locks.Advance(1.6d);
        controller.Tick(0.3d, canAct: true);
        Assert.Equal([corpse, corpse], automation.Opened);
    }

    /// <summary>
    /// An open still waiting for its container while another rule holds the
    /// item slot is held up, not failing: the wait belongs to the holder and
    /// must not be charged against the open timeout that writes a corpse
    /// off. Mutation: charge the state age on every turn and the corpse is
    /// counted as a failed open the moment the other rule lets go.
    /// </summary>
    [Fact]
    public void AnOpenHeldUpByAnotherRuleIsNotCountedAgainstTheCorpse()
    {
        const uint corpse = 0x70001173u;
        LootSettings settings = ChainSettings();
        settings.CorpseOpenTimeoutSeconds = 1.5d;
        var automation = new Automation { Corpses = [ChainCorpse(corpse)] };
        var controller = new LootController(new Host(automation), settings);
        var locks = new ActionLockTable();
        controller.BindActionLocks(locks);

        Assert.True(controller.Tick(0.3d, canAct: true));
        Assert.Equal([corpse], automation.Opened);
        automation.Requested = corpse;

        // Another rule uses an item of its own. Its window outlives the
        // open's, so the item slot stays held after the corpse-open slot has
        // gone -- which is what says the hold is somebody else's.
        locks.Arm(ActionLockKind.ItemUse, 10d);
        locks.Advance(2d);
        Assert.False(locks.IsLocked(ActionLockKind.CorpseOpenAttempt));
        Assert.True(locks.IsLocked(ActionLockKind.ItemUse));

        Assert.True(controller.Tick(2d, canAct: true));
        Assert.Equal([corpse], automation.Opened);

        // The other rule is done; the open is looked at again, still inside
        // its own timeout.
        locks.Advance(9d);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([corpse], automation.Opened);
        Assert.DoesNotContain("Retrying corpse", controller.Status, StringComparison.Ordinal);
        Assert.DoesNotContain("Blacklisted", controller.Status, StringComparison.Ordinal);
    }

    /// <summary>
    /// A busy answer to an open is the host saying "not yet" -- it paces two
    /// uses apart, and closing the corpse just before was one of them. That
    /// is not this corpse failing to open: it costs it no attempt and arms
    /// no slot, so the very next turn opens it. Mutation: fall through to the
    /// refused branch and the turn counts an attempt, arms the open slots and
    /// leaves "Retrying corpse" on the status line -- the pause a live
    /// session showed between one corpse and the next.
    /// </summary>
    [Fact]
    public void AnOpenTheHostIsPacingCostsTheCorpseNothing()
    {
        const uint first = 0x70001181u;
        const uint second = 0x70001182u;
        LootSettings settings = ChainSettings();
        settings.CorpseItemAppearanceTimeoutSeconds = 0d;
        settings.CorpseOpenTimeoutSeconds = 1.5d;
        var automation = new Automation { Corpses = [ChainCorpse(first)] };
        var controller = new LootController(new Host(automation), settings);
        var locks = new ActionLockTable();
        controller.BindActionLocks(locks);

        Assert.True(controller.Tick(0.3d, canAct: true));
        Assert.Equal([first], automation.Opened);
        automation.Requested = first;
        automation.Current = first;
        // The container is open: the frame gives the open's slots back, the
        // way the host does beside the rule pass.
        Assert.True(controller.ObserveCorpseOpened());
        Assert.True(controller.Tick(0.3d, canAct: true));
        Assert.Equal([first], automation.Closed);

        // The close was a use; the open of the next corpse lands inside the
        // host's pacing window and comes back busy.
        automation.Requested = 0u;
        automation.Current = 0u;
        automation.Corpses = [ChainCorpse(second)];
        automation.OpenResult = PluginItemCommandStatus.Busy;
        locks.Advance(0.1d);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([first, second], automation.Opened);
        Assert.DoesNotContain(
            "Retrying corpse",
            controller.Status,
            StringComparison.Ordinal);
        Assert.False(locks.IsLocked(ActionLockKind.ItemUse));
        Assert.False(locks.IsLocked(ActionLockKind.CorpseOpenAttempt));

        // The window has passed: the same open goes out on the next turn.
        automation.OpenResult = PluginItemCommandStatus.Started;
        locks.Advance(0.1d);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([first, second, second], automation.Opened);
        Assert.Contains("Opening", controller.Status, StringComparison.Ordinal);
    }

    /// <summary>
    /// The reference counts every open attempt, refused or not, and
    /// blacklists the corpse at the profile's attempt count. Mutation: drop
    /// the <c>BlacklistFailedCorpse</c> call from the refused-open branch and
    /// the third turn opens a third time.
    /// </summary>
    [Fact]
    public void ARefusedOpenCountsTowardsTheCorpseBlacklist()
    {
        const uint corpse = 0x70001171u;
        LootSettings settings = ChainSettings();
        settings.BlacklistCorpseOpenAttemptCount = 2;
        var automation = new Automation
        {
            Corpses = [ChainCorpse(corpse)],
            OpenResult = PluginItemCommandStatus.Refused,
        };
        var controller = new LootController(new Host(automation), settings);

        Assert.False(controller.Tick(0.3d, canAct: true));
        Assert.False(controller.Tick(0.3d, canAct: true));
        Assert.Equal([corpse, corpse], automation.Opened);
        Assert.Contains("Blacklisted", controller.Status);

        Assert.False(controller.Tick(0.3d, canAct: true));
        Assert.Equal([corpse, corpse], automation.Opened);
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

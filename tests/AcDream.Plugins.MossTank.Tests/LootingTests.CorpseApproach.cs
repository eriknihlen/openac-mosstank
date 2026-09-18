using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The looter's walk: which corpse it steers at, the two radii that bound it,
/// and what a lost pass does to it. It shares the looter's own fakes, because
/// it shares the looter's own corpse pick.
/// </summary>
public sealed partial class LootingTests
{
    /// <summary>
    /// A corpse the open step cannot reach is walked to: the mover is armed at
    /// it and the character is asked to run.
    ///
    /// Mutation: decline instead of steering, and nothing is ever asked of the
    /// character.
    /// </summary>
    [Fact]
    public void ACorpseBeyondArmsReachIsWalkedTo()
    {
        var settings = ApproachSettings(range: 30d);
        var automation = ApproachAutomation(Corpse(0x70002000u, 12f));
        CorpseApproachController approach = Approach(automation, settings);

        Assert.True(approach.ClaimFromRulePass(canAct: true));

        PluginMovementIntent intent = Assert.Single(automation.Intents);
        Assert.True(intent.Forward);
        Assert.True(intent.Run);
        Assert.Contains("Walking to", approach.Status, StringComparison.Ordinal);
    }

    /// <summary>
    /// The shipped corpse approach range is zero, and zero is not "no limit" —
    /// it selects nothing at all, so the looter reaches only what the open step
    /// can already touch. A profile that never set the range gets no walk.
    ///
    /// Mutation: read zero as unbounded (or as any fixed floor), and the corpse
    /// twelve metres off is walked to.
    /// </summary>
    [Fact]
    public void TheShippedZeroRangeWalksNowhere()
    {
        var settings = ApproachSettings(range: 0d);
        var automation = ApproachAutomation(Corpse(0x70002001u, 12f));
        CorpseApproachController approach = Approach(automation, settings);

        Assert.False(approach.ClaimFromRulePass(canAct: true));
        Assert.Empty(automation.Intents);
    }

    /// <summary>
    /// Beyond the range the profile set there is no goal at all — not a goal
    /// the mover walks part of the way to. The bound is applied twice, as the
    /// reference applies it: once when the corpse is picked and once when the
    /// rule asks whether it has a goal.
    ///
    /// Mutation: take the range off the pick and out of the rule's test, and
    /// the character sets off after a corpse the profile put out of bounds.
    /// </summary>
    [Fact]
    public void ACorpseFurtherThanTheRangeIsNotWalkedTo()
    {
        var settings = ApproachSettings(range: 15d);
        var automation = ApproachAutomation(Corpse(0x70002002u, 20f));
        CorpseApproachController approach = Approach(automation, settings);

        Assert.False(approach.ClaimFromRulePass(canAct: true));
        Assert.Empty(automation.Intents);
    }

    /// <summary>
    /// No active profile bars the approach rule even when an external
    /// classifier is configured and would otherwise make an empty native rule
    /// list eligible.
    /// Mutation <c>IgnoreInactiveProfileInCorpseApproach</c>: remove the
    /// <c>ProfileActive</c> test from
    /// <c>TrySelectApproachCorpse</c>; the character walks to the corpse.
    /// </summary>
    [Fact]
    public void NoActiveProfileNeverWalksToACorpseForAnExternalClassifier()
    {
        var settings = ApproachSettings(range: 30d);
        settings.ProfileActive = false;
        settings.ExternalClassifierId = "utility/loot";
        settings.Rules.Clear();
        var automation = ApproachAutomation(Corpse(0x70002010u, 12f));
        CorpseApproachController approach = Approach(automation, settings);

        Assert.False(approach.ClaimFromRulePass(canAct: true));
        Assert.Empty(automation.Intents);
    }

    /// <summary>
    /// The walk ends at the inner radius, not at the open's reach: the rule
    /// stops being valid there, which is what hands the corpse to the open. At
    /// four metres — already inside the five-metre open reach — the walk is
    /// still on, because the profile's inner stop is closer than that.
    ///
    /// Mutation: drop the inner test and the mover keeps pushing the character
    /// into the corpse.
    /// </summary>
    [Theory]
    [InlineData(4f, true)]
    [InlineData(3f, false)]
    public void TheWalkEndsAtTheInnerRadius(float distance, bool walks)
    {
        var settings = ApproachSettings(range: 30d);
        var automation = ApproachAutomation(Corpse(0x70002003u, distance));
        CorpseApproachController approach = Approach(automation, settings);

        Assert.Equal(walks, approach.ClaimFromRulePass(canAct: true));
        Assert.Equal(walks, automation.Intents.Count != 0);
    }

    /// <summary>
    /// The walk picks with the looter's own judgement, so a corpse the looter
    /// would refuse to open is not walked to either. A stranger's kill, before
    /// it is a hundred seconds old, is nobody's business.
    ///
    /// Mutation: pick over the corpses the client reports rather than over the
    /// ones the looter may have, and the character walks to somebody else's
    /// kill.
    /// </summary>
    [Fact]
    public void ACorpseTheCharacterMayNotLootIsNotWalkedTo()
    {
        var settings = ApproachSettings(range: 30d);
        var automation = ApproachAutomation(
            Corpse(0x70002004u, 12f) with { LongDescription = "Killed by Stranger." });
        CorpseApproachController approach = Approach(automation, settings);

        Assert.False(approach.ClaimFromRulePass(canAct: true));
        Assert.Empty(automation.Intents);
    }

    /// <summary>
    /// Losing the pass drops the keys. The reference's navigate rule stops its
    /// mover on that edge and ours does the same thing in this host's terms —
    /// the held movement intent is cleared, not left running under whatever
    /// rule won the pass.
    ///
    /// Mutation: disarm without clearing, and the character keeps running at
    /// the corpse while another rule is acting.
    /// </summary>
    [Fact]
    public void LosingThePassStopsTheWalk()
    {
        var settings = ApproachSettings(range: 30d);
        var automation = ApproachAutomation(Corpse(0x70002005u, 12f));
        CorpseApproachController approach = Approach(automation, settings);

        Assert.True(approach.ClaimFromRulePass(canAct: true));
        Assert.Equal(0, automation.MovementClears);

        approach.StopForLostTurn();

        Assert.Equal(1, automation.MovementClears);
    }

    /// <summary>
    /// The walk is a navigation goal, so it re-asks where the corpse is on the
    /// mover's own interval rather than once per rule pass. A pass every third
    /// of a second would steer a handful of times a second at most, and one
    /// held turn per pass is tens of degrees — several times the alignment
    /// band, which is how a mover ends up hunting around the bearing.
    ///
    /// Mutation: steer on every frame regardless of the interval, and the
    /// half-interval frame below moves the character too.
    /// </summary>
    [Fact]
    public void TheArmedWalkStepsOnTheMoverInterval()
    {
        var settings = ApproachSettings(range: 30d);
        var automation = ApproachAutomation(Corpse(0x70002006u, 12f));
        CorpseApproachController approach = Approach(automation, settings);

        Assert.True(approach.ClaimFromRulePass(canAct: true));
        Assert.Single(automation.Intents);

        approach.StepArmedMover(NavigationMover.MoverIntervalSeconds / 2d);
        Assert.Single(automation.Intents);

        approach.StepArmedMover(NavigationMover.MoverIntervalSeconds);
        Assert.Equal(2, automation.Intents.Count);
    }

    /// <summary>
    /// A disarmed mover steers at nothing: the walk only runs while the rule
    /// that armed it is winning passes.
    ///
    /// Mutation: step the mover whether or not it is armed, and a corpse gets
    /// walked at while another rule owns the character.
    /// </summary>
    [Fact]
    public void ADisarmedWalkDoesNotSteer()
    {
        var settings = ApproachSettings(range: 30d);
        var automation = ApproachAutomation(Corpse(0x70002007u, 12f));
        CorpseApproachController approach = Approach(automation, settings);

        approach.StepArmedMover(1d);

        Assert.Empty(automation.Intents);
    }

    /// <summary>
    /// Every pull holds navigation, which is what keeps the walk — a rule
    /// ABOVE the open — from steering the character off to the next corpse one
    /// item into emptying this one. The item slot goes with it, because that is
    /// what paces the pulls.
    ///
    /// Mutation: drop the two arms at the pull and the walk is free to take the
    /// character away mid-corpse.
    /// </summary>
    [Fact]
    public void APullHoldsNavigationSoTheWalkCannotTakeTheCharacterAway()
    {
        var settings = new LootSettings
        {
            Enabled = true,
            ScanIntervalSeconds = 0.05d,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        const uint corpse = 0x70002100u;
        const uint coin = 0x70002101u;
        var automation = ApproachAutomation(Corpse(corpse, 3f));
        var locks = new ActionLockTable();
        var controller = new LootController(new Host(automation), settings);
        controller.BindActionLocks(locks);

        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { corpse }, automation.Opened);

        // The container is open, so the open's own three slots go back.
        automation.Requested = corpse;
        automation.Current = corpse;
        automation.Contents = [Item(coin, "Colosseum coin", 77)];
        Assert.True(controller.ObserveCorpseOpened());
        Assert.False(locks.IsLocked(ActionLockKind.Navigation));

        // Read the contents, decide, pull.
        Assert.True(controller.Tick(0.2d, canAct: true));
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { coin }, automation.Picked);

        Assert.True(locks.IsLocked(ActionLockKind.Navigation));
        Assert.True(locks.IsLocked(ActionLockKind.ItemUse));
        locks.Advance(0.8d);
        Assert.False(locks.IsLocked(ActionLockKind.Navigation));
    }

    /// <summary>
    /// The order the corpse rules sit in: the walk outranks the open, the open
    /// outranks the route, and the same holds in the idle band. It is the order
    /// that makes the walk work — a route rule above it would steer the
    /// character away from every corpse it ever reached for.
    ///
    /// Mutation: swap any adjacent pair and the assertion fails.
    /// </summary>
    [Fact]
    public void TheWalkOutranksTheOpenAndTheRoute()
    {
        Assert.True(
            Position(MacroRuleSlot.NavigateCorpsePriority)
                < Position(MacroRuleSlot.OpenCorpsePriority));
        Assert.True(
            Position(MacroRuleSlot.OpenCorpsePriority)
                < Position(MacroRuleSlot.NavigateRoutePriority));
        Assert.True(
            Position(MacroRuleSlot.NavigateCorpseIdle)
                < Position(MacroRuleSlot.OpenCorpseIdle));
        Assert.True(
            Position(MacroRuleSlot.OpenCorpseIdle)
                < Position(MacroRuleSlot.NavigateRouteIdle));

        static int Position(MacroRuleSlot slot)
        {
            for (int index = 0; index < MacroRuleTable.Entries.Count; index++)
            {
                if (MacroRuleTable.Entries[index].Slot == slot)
                    return index;
            }
            return -1;
        }
    }

    private static LootSettings ApproachSettings(double range)
    {
        var settings = new LootSettings
        {
            Enabled = true,
            CorpseApproachRange = range,
            ScanIntervalSeconds = 0.05d,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        return settings;
    }

    /// <summary>The character's own kill, described, and placed due north.</summary>
    private static PluginLootContainer Corpse(uint objectId, float distance) =>
        new(objectId, 1u, "Corpse", distance, false, false, false)
        {
            IsIdentified = true,
            LongDescription = "Killed by Tester.",
            HasPosition = true,
            Position = new PluginNavigationPosition(
                0x7F7F0001u,
                0d,
                distance / 240d,
                0d,
                0f,
                IsOutdoor: true),
        };

    private static Automation ApproachAutomation(params PluginLootContainer[] corpses) =>
        new()
        {
            Corpses = corpses,
            NavigationSnapshot = new PluginNavigationSnapshot(
                IsAvailable: true,
                IsPortalSpace: false,
                LocalObjectId: Player,
                new PluginNavigationPosition(
                    0x7F7F0001u,
                    0d,
                    0d,
                    0d,
                    0f,
                    IsOutdoor: true),
                IsMoving: false,
                IsAirborne: false),
        };

    private static CorpseApproachController Approach(
        Automation automation,
        LootSettings settings)
    {
        var host = new Host(automation);
        return new CorpseApproachController(
            host,
            settings,
            new LootController(host, settings));
    }
}

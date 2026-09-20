using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class SpellCastTrackerTests
{
    /// <summary>The log a spell result is written to.</summary>
    private const uint MagicLog = 0x07u;

    private static SpellCastTracker Armed(
        out List<SpellCastOutcomeInfo> outcomes,
        bool hitsMultipleTargets = false,
        string spellName = "Flame Bolt VII",
        string targetName = "Drudge")
    {
        var tracker = new SpellCastTracker();
        List<SpellCastOutcomeInfo> log = [];
        tracker.Completed += log.Add;
        outcomes = log;
        tracker.Begin(
            100u,
            spellName,
            10u,
            targetName,
            hitsMultipleTargets,
            5L,
            school: SpellCastTracker.WarMagicSchool,
            canKill: true);
        return tracker;
    }

    private static PluginCastCompletion Receipt(
        long revision = 6L,
        uint spellId = 100u,
        uint weenieError = 0u) =>
        new(revision, spellId, 10u, weenieError);

    [Fact]
    public void ArmingLeavesIdle()
    {
        SpellCastTracker tracker = Armed(out _);

        Assert.True(tracker.IsBusy);
        Assert.Equal(SpellCastTrackerState.AwaitingLaunch, tracker.State);
        Assert.Equal(100u, tracker.SpellId);
        Assert.Equal(10u, tracker.TargetObjectId);
    }

    [Fact]
    public void ACleanReceiptStartsTheResultWaitAndDoesNotReleaseTheLatch()
    {
        SpellCastTracker tracker = Armed(out List<SpellCastOutcomeInfo> outcomes);

        tracker.ObserveCompletion(Receipt());

        Assert.True(tracker.IsBusy);
        Assert.Equal(SpellCastTrackerState.AwaitingResult, tracker.State);
        Assert.Empty(outcomes);
    }

    /// <summary>
    /// A success line ends the wait.
    /// Mutation: drop the <c>Success</c> arm from <c>ObserveChat</c> and this
    /// fails, because the tracker stays busy.
    /// </summary>
    [Fact]
    public void ASuccessLineReleasesTheLatch()
    {
        SpellCastTracker tracker = Armed(out List<SpellCastOutcomeInfo> outcomes);
        tracker.ObserveCompletion(Receipt());

        tracker.ObserveChat(
            1uL,
            "You blast Drudge for 42 points with Flame Bolt VII.",
            logTextType: MagicLog);

        Assert.False(tracker.IsBusy);
        Assert.Equal(SpellCastOutcome.Success, Assert.Single(outcomes).Outcome);
    }

    [Fact]
    public void AKillLineReleasesTheLatchAndNamesTheTarget()
    {
        SpellCastTracker tracker = Armed(out List<SpellCastOutcomeInfo> outcomes);
        tracker.ObserveCompletion(Receipt());

        tracker.ObserveChat(1uL, "You killed Drudge!");

        Assert.False(tracker.IsBusy);
        SpellCastOutcomeInfo info = Assert.Single(outcomes);
        Assert.Equal(SpellCastOutcome.Kill, info.Outcome);
        Assert.Equal(10u, info.TargetObjectId);
    }

    [Fact]
    public void AResultNamingAnotherSpellIsNotThisCastsResult()
    {
        SpellCastTracker tracker = Armed(out List<SpellCastOutcomeInfo> outcomes);
        tracker.ObserveCompletion(Receipt());

        tracker.ObserveChat(
            1uL,
            "You blast Drudge for 42 points with Frost Bolt VII.",
            logTextType: MagicLog);

        Assert.True(tracker.IsBusy);
        Assert.Empty(outcomes);
    }

    [Fact]
    public void OnlyTheKillSentenceInThePlainLogEndsTheWait()
    {
        SpellCastTracker tracker = Armed(out List<SpellCastOutcomeInfo> outcomes);
        tracker.ObserveCompletion(Receipt());

        // Those words in the magic log are not a kill notice at all.
        tracker.ObserveChat(
            1uL,
            "You killed Olthoi Soldier!",
            logTextType: MagicLog);

        Assert.True(tracker.IsBusy);
        Assert.Empty(outcomes);

        // In the plain log they are, whichever monster they name: the client
        // writes them for our own blow, and the name is not checked here.
        tracker.ObserveChat(2uL, "You killed Olthoi Soldier!");

        Assert.False(tracker.IsBusy);
        Assert.Equal(SpellCastOutcome.Kill, Assert.Single(outcomes).Outcome);
    }

    [Fact]
    public void APermanentFailIsIgnoredForAMultiTargetSpell()
    {
        SpellCastTracker tracker = Armed(
            out List<SpellCastOutcomeInfo> outcomes,
            hitsMultipleTargets: true);
        tracker.ObserveCompletion(Receipt());

        tracker.ObserveChat(
            1uL,
            "Drudge is an invalid target.",
            logTextType: MagicLog);

        Assert.True(tracker.IsBusy);
        Assert.Empty(outcomes);
    }

    /// <summary>
    /// And it DOES end the wait for a single-target one.
    /// Mutation: return <c>None</c> from the <c>PermanentFail</c> arm and this
    /// fails.
    /// </summary>
    [Fact]
    public void APermanentFailEndsTheWaitForASingleTargetSpell()
    {
        SpellCastTracker tracker = Armed(out List<SpellCastOutcomeInfo> outcomes);
        tracker.ObserveCompletion(Receipt());

        tracker.ObserveChat(
            1uL,
            "Drudge is an invalid target.",
            logTextType: MagicLog);

        Assert.False(tracker.IsBusy);
        Assert.Equal(
            SpellCastOutcome.PermanentFail,
            Assert.Single(outcomes).Outcome);
    }

    [Fact]
    public void TheResultWaitExpiresAtFourTicksOfNineHundredAndSevenMilliseconds()
    {
        SpellCastTracker tracker = Armed(out List<SpellCastOutcomeInfo> outcomes);
        tracker.ObserveCompletion(Receipt());

        tracker.Advance(3.6d);
        Assert.True(tracker.IsBusy);
        Assert.Empty(outcomes);

        tracker.Advance(0.03d);

        Assert.False(tracker.IsBusy);
        Assert.Equal(
            SpellCastOutcome.ResultTimeout,
            Assert.Single(outcomes).Outcome);
    }

    [Fact]
    public void TheLaunchWaitExpiresAfterFiveSeconds()
    {
        SpellCastTracker tracker = Armed(out List<SpellCastOutcomeInfo> outcomes);

        tracker.Advance(4.9d);
        Assert.True(tracker.IsBusy);

        tracker.Advance(0.2d);

        Assert.False(tracker.IsBusy);
        Assert.Equal(
            SpellCastOutcome.LaunchTimeout,
            Assert.Single(outcomes).Outcome);
    }

    [Fact]
    public void ARefusedReceiptReleasesImmediatelyAndCarriesTheError()
    {
        SpellCastTracker tracker = Armed(out List<SpellCastOutcomeInfo> outcomes);

        tracker.ObserveCompletion(Receipt(weenieError: 0x1Du));

        Assert.False(tracker.IsBusy);
        SpellCastOutcomeInfo info = Assert.Single(outcomes);
        Assert.Equal(SpellCastOutcome.Rejected, info.Outcome);
        Assert.Equal(0x1Du, info.WeenieError);
    }

    [Fact]
    public void TheReceiptTheCastWasIssuedAgainstIsNotItsAnswer()
    {
        SpellCastTracker tracker = Armed(out _);

        tracker.ObserveCompletion(Receipt(revision: 5L));

        Assert.Equal(SpellCastTrackerState.AwaitingLaunch, tracker.State);
    }

    [Fact]
    public void TheSameChatLineIsFoldedOnce()
    {
        SpellCastTracker tracker = Armed(out List<SpellCastOutcomeInfo> outcomes);
        tracker.ObserveCompletion(Receipt());

        tracker.ObserveChat(7uL, "You killed Drudge!");
        tracker.Begin(
            100u,
            "Flame Bolt VII",
            10u,
            "Drudge",
            false,
            8L,
            school: SpellCastTracker.WarMagicSchool,
            canKill: true);
        tracker.ObserveCompletion(Receipt(revision: 9L));
        tracker.ObserveChat(7uL, "You killed Drudge!");

        Assert.Single(outcomes);
        Assert.True(tracker.IsBusy);
    }

    [Fact]
    public void TheSameReceiptIsFoldedOnce()
    {
        SpellCastTracker tracker = Armed(out List<SpellCastOutcomeInfo> outcomes);
        tracker.ObserveCompletion(Receipt());
        tracker.Advance(3.0d);

        tracker.ObserveCompletion(Receipt());
        tracker.Advance(0.7d);

        Assert.Equal(
            SpellCastOutcome.ResultTimeout,
            Assert.Single(outcomes).Outcome);
    }

    [Fact]
    public void OnlyTheAwaitedTargetsDeletionEndsTheWait()
    {
        SpellCastTracker tracker = Armed(out List<SpellCastOutcomeInfo> outcomes);

        tracker.ResetForTarget(11u);
        Assert.True(tracker.IsBusy);

        tracker.ResetForTarget(10u);

        Assert.False(tracker.IsBusy);
        Assert.Empty(outcomes);
    }

    [Fact]
    public void AResultArrivingBeforeTheReceiptStillEndsTheWait()
    {
        SpellCastTracker tracker = Armed(out List<SpellCastOutcomeInfo> outcomes);

        tracker.ObserveChat(1uL, "You killed Drudge!");

        Assert.False(tracker.IsBusy);
        Assert.Equal(SpellCastOutcome.Kill, Assert.Single(outcomes).Outcome);
    }

    [Theory]
    [InlineData(new uint[0], 0u, false)]
    [InlineData(new[] { 110u, 77u }, 0u, true)]
    [InlineData(new[] { 77u, 110u }, 0u, false)]
    [InlineData(new uint[0], 638u, true)]
    public void HitsMultipleTargetsMatchesMySpell(
        uint[] components,
        uint family,
        bool expected)
    {
        var spell = new PluginSpellInfo(
            100u,
            "Spell",
            family,
            Tier: 1,
            Difficulty: 10,
            ManaCost: 5,
            DurationSeconds: 0f,
            School: 34u,
            "d",
            IsSelfTargeted: false,
            IsBeneficial: false)
        {
            FormulaComponentIds = components,
        };

        Assert.Equal(expected, SpellCastTracker.HitsMultipleTargetsFor(spell));
    }
    [Fact]
    public void TheSpokenEchoMovesTheTrackerFromLaunchToTheResultWait()
    {
        var tracker = new SpellCastTracker();
        tracker.Begin(1u, "Strength Self VI", 5u, "yourself", false, 0L, "casfaen");
        Assert.Equal(SpellCastTrackerState.AwaitingLaunch, tracker.State);

        // The echo is matched after ToLowerInvariant().Replace(" ", "").
        tracker.ObserveChat(
            1uL,
            "Cas Faen",
            ownSpeech: true,
            logTextType: CombatLogTextType.Spellcasting);

        Assert.Equal(SpellCastTrackerState.AwaitingResult, tracker.State);
    }

    [Fact]
    public void ASpokenEchoNamingAnotherSpellEndsTheWait()
    {
        var tracker = new SpellCastTracker();
        tracker.Begin(1u, "Strength Self VI", 5u, "yourself", false, 0L, "casfaen");

        tracker.ObserveChat(
            1uL,
            "Zharalim Retaanu",
            ownSpeech: true,
            logTextType: CombatLogTextType.Spellcasting);

        Assert.Equal(SpellCastTrackerState.Idle, tracker.State);
        Assert.False(tracker.IsBusy);
    }

    [Fact]
    public void AnotherPlayerSpeakingTheSameWordsIsNotOurEcho()
    {
        var tracker = new SpellCastTracker();
        tracker.Begin(1u, "Strength Self VI", 5u, "yourself", false, 0L, "casfaen");

        tracker.ObserveChat(
            1uL,
            "Cas Faen",
            ownSpeech: false,
            logTextType: CombatLogTextType.Spellcasting);

        Assert.Equal(SpellCastTrackerState.AwaitingLaunch, tracker.State);
    }

    /// <summary>
    /// The character typing the spell's own words into local chat is not a
    /// gesture: the words match and the speaker is us, but the line comes from
    /// the ordinary log rather than the spellcasting one.
    /// Mutation: drop the log-type half of the launch arm's test and this
    /// fails — a typed line advances a cast nobody gestured.
    /// </summary>
    [Fact]
    public void OurOwnTypedWordsAreNotTheGestureEcho()
    {
        var tracker = new SpellCastTracker();
        tracker.Begin(1u, "Strength Self VI", 5u, "yourself", false, 0L, "casfaen");

        tracker.ObserveChat(1uL, "Cas Faen", ownSpeech: true);

        Assert.Equal(SpellCastTrackerState.AwaitingLaunch, tracker.State);
    }

    [Fact]
    public void TheRavenFurySayingClassifiesItAsMultiTarget()
    {
        PluginSpellInfo raven = new(
            1u,
            "Curse of Raven Fury",
            Family: 500u,
            Tier: 1,
            Difficulty: 10,
            ManaCost: 5,
            DurationSeconds: 60f,
            School: 31u,
            Description: string.Empty,
            IsSelfTargeted: false,
            IsBeneficial: false)
        {
            Saying = "tugakquati",
        };

        Assert.True(SpellCastTracker.HitsMultipleTargetsFor(raven));
        Assert.False(SpellCastTracker.HitsMultipleTargetsFor(
            raven with { Saying = "casfaen" }));
    }

    /// <summary>
    /// Mutation: drop the <c>CanKill</c> term and this fails — a kill line
    /// arriving while a debuff is in flight would end the debuff's target.
    /// </summary>
    [Fact]
    public void OnlyASpellThatCanKillClaimsAKillingBlow()
    {
        var tracker = new SpellCastTracker();
        List<SpellCastOutcomeInfo> outcomes = [];
        tracker.Completed += outcomes.Add;
        tracker.Begin(
            200u,
            "Imperil Other VII",
            10u,
            "Drudge",
            false,
            5L,
            school: 31u,
            canKill: false);
        tracker.ObserveCompletion(new PluginCastCompletion(6L, 200u, 10u, 0u));

        tracker.ObserveChat(1uL, "You killed Drudge!");

        Assert.True(tracker.IsBusy);
        Assert.Empty(outcomes);
    }

    /// <summary>
    /// Mutation: name-filter the failure classes as well and this fails — the
    /// tracker would sit busy for the full result timeout after a resist whose
    /// sentence does not name the tracked target.
    /// </summary>
    [Fact]
    public void AResistEndsTheWaitWhateverNameItCarries()
    {
        SpellCastTracker tracker = Armed(out List<SpellCastOutcomeInfo> outcomes);
        tracker.ObserveCompletion(Receipt());

        tracker.ObserveChat(
            1uL,
            "Olthoi Soldier resists your spell",
            logTextType: MagicLog);

        Assert.False(tracker.IsBusy);
        Assert.Equal(SpellCastOutcome.Fail, Assert.Single(outcomes).Outcome);
    }

    /// <summary>
    /// Mutation: drop the lockout arm and this fails — a void/war hybrid would
    /// fire the other school inside its cooldown and be refused by the client.
    /// </summary>
    [Fact]
    public void FinishingAVoidCastHoldsWarOffForFiveAndAHalfSeconds()
    {
        var locks = new ActionLockTable();
        var tracker = new SpellCastTracker();
        tracker.BindActionLocks(locks);
        tracker.Begin(
            300u,
            "Nether Bolt VII",
            10u,
            "Drudge",
            false,
            5L,
            school: SpellCastTracker.VoidMagicSchool,
            canKill: false);
        tracker.ObserveCompletion(new PluginCastCompletion(6L, 300u, 10u, 0u));

        Assert.False(tracker.IsSchoolLockedOut(SpellCastTracker.WarMagicSchool));

        tracker.ObserveChat(
            1uL,
            "You cast Nether Bolt VII on Drudge",
            logTextType: MagicLog);

        Assert.True(tracker.IsSchoolLockedOut(SpellCastTracker.WarMagicSchool));
        Assert.False(tracker.IsSchoolLockedOut(SpellCastTracker.VoidMagicSchool));

        locks.Advance(5.4d);
        Assert.True(tracker.IsSchoolLockedOut(SpellCastTracker.WarMagicSchool));
        locks.Advance(0.2d);
        Assert.False(tracker.IsSchoolLockedOut(SpellCastTracker.WarMagicSchool));
    }

    /// <summary>
    /// Mutation: drop the arm's state test and this fails — a cast that never
    /// left the ground must not hold the other school off.
    /// </summary>
    [Fact]
    public void ACastThatNeverLaunchedArmsNoLockout()
    {
        var locks = new ActionLockTable();
        var tracker = new SpellCastTracker();
        tracker.BindActionLocks(locks);
        tracker.Begin(
            300u,
            "Nether Bolt VII",
            10u,
            "Drudge",
            false,
            5L,
            school: SpellCastTracker.VoidMagicSchool);

        tracker.Advance(SpellCastTracker.LaunchTimeoutSeconds + 0.1d);

        Assert.Equal(SpellCastTrackerState.Idle, tracker.State);
        Assert.False(tracker.IsSchoolLockedOut(SpellCastTracker.WarMagicSchool));
    }

    /// <summary>
    /// Mutation: delete the re-issue loop and this fails — a request the
    /// server ignored would cost a silent five seconds instead of being sent
    /// again roughly twenty-five times.
    /// </summary>
    [Fact]
    public void AnUnacknowledgedCastIsSentAgainUntilTheBudgetRunsOut()
    {
        var tracker = new SpellCastTracker();
        List<(uint Spell, uint Target)> reissues = [];
        tracker.ReissueCast = (spell, target) =>
        {
            reissues.Add((spell, target));
            return true;
        };
        tracker.Begin(100u, "Flame Bolt VII", 10u, "Drudge", false, 5L);

        for (int tick = 0; tick < 60; tick++)
            tracker.Advance(0.1d);

        Assert.Equal(SpellCastTrackerState.Idle, tracker.State);
        Assert.InRange(reissues.Count, 24, 25);
        Assert.All(reissues, entry => Assert.Equal((100u, 10u), entry));
    }

    /// <summary>
    /// Mutation: drop the low-mana interval and this fails — the count would
    /// stay at the ordinary rate.
    /// </summary>
    [Fact]
    public void NearlyOutOfManaTheCastIsSentAgainTwiceAsOften()
    {
        var tracker = new SpellCastTracker();
        int reissues = 0;
        tracker.ReissueCast = (_, _) =>
        {
            reissues++;
            return true;
        };
        tracker.Begin(
            100u,
            "Flame Bolt VII",
            10u,
            "Drudge",
            false,
            5L,
            currentMana: 9);

        for (int tick = 0; tick < 60; tick++)
            tracker.Advance(0.1d);

        Assert.InRange(reissues, 49, 50);
    }

    /// <summary>
    /// Mutation: make the window unbounded and this fails — the nudge would
    /// still be open a second after the cast was confirmed in flight.
    /// </summary>
    [Fact]
    public void TheNudgeWindowIsOneResultTickLong()
    {
        SpellCastTracker tracker = Armed(out _);
        Assert.False(tracker.JiggleWindowOpen);

        tracker.ObserveCompletion(Receipt());
        Assert.True(tracker.JiggleWindowOpen);

        tracker.Advance(SpellCastTracker.ResultTickSeconds - 0.01d);
        Assert.True(tracker.JiggleWindowOpen);

        tracker.Advance(0.02d);
        Assert.False(tracker.JiggleWindowOpen);
    }
}

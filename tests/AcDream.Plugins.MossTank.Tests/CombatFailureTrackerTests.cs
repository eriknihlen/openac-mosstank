using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class CombatFailureTrackerTests
{
    /// <summary>
    /// A monster whose health moves has not necessarily been reached BY US: a
    /// fellow's blow, its own regeneration or a heal all move it. Only our own
    /// damage line clears the attempt count, so an unhittable monster standing
    /// in a busy fight is still given up on.
    /// Mutation: clear <c>Attempts</c> when the health revision changes in
    /// <c>ObserveTargets</c> and this fails — the monster is never
    /// blacklisted.
    /// </summary>
    [Fact]
    public void SomebodyElsesDamageDoesNotCancelAccumulatedMisses()
    {
        var tracker = new CombatFailureTracker();
        var settings = new CombatSettings
        {
            BlacklistMonsterAttemptCount = 2,
            BlacklistMonsterTimeoutSeconds = 30,
        };
        tracker.ObserveTargets([Target(1)], 0, settings);
        tracker.RecordMiss(10, 1, settings);
        tracker.RecordMiss(10, 1, settings);

        tracker.ObserveTargets([Target(2)], 2, settings);
        tracker.RecordMiss(10, 3, settings);

        Assert.Equal(
            CombatSuppressionReason.Blacklisted,
            tracker.Reason(10, 3));

        // Our own damage report is the one thing that does clear it.
        tracker.ObserveTargets([Target(3)], 40, settings);
        tracker.RecordMiss(10, 40, settings);
        tracker.ResetAttempts(10);
        tracker.RecordMiss(10, 41, settings);
        tracker.RecordMiss(10, 41, settings);

        Assert.Equal(
            CombatSuppressionReason.None,
            tracker.Reason(10, 41));
    }

    /// <summary>
    /// Mutation: change <c>RecordMiss</c>'s
    /// <c>entry.Attempts &lt;= settings.BlacklistMonsterAttemptCount</c> to
    /// <c>&lt;</c> and the first assertion fails — the monster is given up on
    /// one attempt early.
    /// </summary>
    [Fact]
    public void TheAllowedAttemptCountIsSpentBeforeTheBlacklistTrips()
    {
        var tracker = new CombatFailureTracker();
        var settings = new CombatSettings
        {
            BlacklistMonsterAttemptCount = 2,
            BlacklistMonsterTimeoutSeconds = 20,
        };
        tracker.ObserveTargets([Target(1)], 0, settings);

        tracker.RecordMiss(10, 1, settings);
        tracker.RecordMiss(10, 1, settings);
        Assert.Equal(
            CombatSuppressionReason.None,
            tracker.Reason(10, 1));

        tracker.RecordMiss(10, 1, settings);
        Assert.Equal(
            CombatSuppressionReason.Blacklisted,
            tracker.Reason(10, 10));

        tracker.ObserveTargets([Target(1)], 23, settings);
        Assert.Equal(
            CombatSuppressionReason.None,
            tracker.Reason(10, 23));
    }

    /// <summary>
    /// A miss can be recorded against anything a cast was aimed at, the
    /// character's own guid included, but only a creature the hostile capture
    /// has named can be given up on. Nothing is suppressed for the rest and
    /// the caller is told nothing to announce.
    /// Mutation: drop the <c>IsCreature</c> guard from <c>RecordMiss</c> and
    /// the first assertion fails — the caster blacklists itself and the
    /// "cannot hit" notice goes out against its own guid.
    /// </summary>
    [Fact]
    public void OnlyACreatureThePassHasSeenCanBeGivenUpOn()
    {
        var tracker = new CombatFailureTracker();
        var settings = new CombatSettings
        {
            BlacklistMonsterAttemptCount = 1,
            BlacklistMonsterTimeoutSeconds = 120,
        };
        tracker.ObserveTargets([Target(0)], 0, settings);

        const uint self = 1342177290u;
        Assert.False(tracker.RecordMiss(self, 0, settings));
        Assert.False(tracker.RecordMiss(self, 0, settings));
        Assert.False(tracker.RecordMiss(self, 0, settings));
        Assert.Equal(CombatSuppressionReason.None, tracker.Reason(self, 1));
        Assert.False(tracker.IsKnown(self));

        // The monster beside it still trips on the attempt after the
        // allowance, so the guard narrows the verdict and nothing else.
        Assert.False(tracker.RecordMiss(10, 0, settings));
        Assert.True(tracker.RecordMiss(10, 0, settings));
        Assert.Equal(CombatSuppressionReason.Blacklisted, tracker.Reason(10, 1));
    }

    /// <summary>
    /// The same rule for the outright refusal: a permanent-fail sentence aimed
    /// at something the pass does not follow leaves nothing behind.
    /// Mutation: drop the <c>IsCreature</c> guard from <c>ForceBlacklist</c>
    /// and the first assertion fails.
    /// </summary>
    [Fact]
    public void AnOutrightRefusalOnlyBlacklistsACreature()
    {
        var tracker = new CombatFailureTracker();
        var settings = new CombatSettings { BlacklistMonsterTimeoutSeconds = 120 };
        tracker.ObserveTargets([Target(0)], 0, settings);

        tracker.ForceBlacklist(1342177290u, now: 0d, settings);
        Assert.Equal(
            CombatSuppressionReason.None,
            tracker.Reason(1342177290u, now: 1d));

        tracker.ForceBlacklist(10u, now: 0d, settings);
        Assert.Equal(CombatSuppressionReason.Blacklisted, tracker.Reason(10u, now: 1d));
    }

    /// <summary>
    /// Mutation: make <c>ExtendBlacklist</c> assign the deadline
    /// unconditionally and this fails — the second, shorter trip would cut the
    /// first one short.
    /// </summary>
    [Fact]
    public void ALaterShorterTripDoesNotShortenALiveBlacklist()
    {
        var tracker = new CombatFailureTracker();
        var settings = new CombatSettings
        {
            BlacklistMonsterAttemptCount = 0,
            BlacklistMonsterTimeoutSeconds = 100,
        };
        tracker.ObserveTargets([Target(0)], 0, settings);
        tracker.RecordMiss(10, 0, settings);

        settings.BlacklistMonsterTimeoutSeconds = 1;
        tracker.RecordMiss(10, 10, settings);

        Assert.Equal(
            CombatSuppressionReason.Blacklisted,
            tracker.Reason(10, 50));
    }

    /// <summary>
    /// Mutation: make <c>ResetAttempts</c> clear the deadline as well and this
    /// fails — a monster that answers a swing while blacklisted would become
    /// targetable again on the spot.
    /// </summary>
    [Fact]
    public void ReachingTheMonsterResetsTheCountWithoutLiftingALiveBlacklist()
    {
        var tracker = new CombatFailureTracker();
        var settings = new CombatSettings
        {
            BlacklistMonsterAttemptCount = 2,
            BlacklistMonsterTimeoutSeconds = 100,
        };
        tracker.ObserveTargets([Target(0)], 0, settings);
        tracker.RecordMiss(10, 0, settings);
        tracker.RecordMiss(10, 0, settings);
        tracker.ResetAttempts(10);

        // The count started over, so three more are needed to trip.
        tracker.RecordMiss(10, 0, settings);
        tracker.RecordMiss(10, 0, settings);
        Assert.Equal(CombatSuppressionReason.None, tracker.Reason(10, 1));

        tracker.RecordMiss(10, 0, settings);
        Assert.Equal(
            CombatSuppressionReason.Blacklisted,
            tracker.Reason(10, 1));

        tracker.ResetAttempts(10);
        Assert.Equal(
            CombatSuppressionReason.Blacklisted,
            tracker.Reason(10, 1));
    }

    [Fact]
    public void UnansweredCastsTripOnTheAttemptAfterTheAllowanceThenStartOver()
    {
        var tracker = new CombatFailureTracker();
        var settings = new CombatSettings
        {
            GhostMonsterSpellAttemptCount = 2,
        };
        tracker.ObserveTargets([Target(0)], 0, settings);

        Assert.False(tracker.RecordSpellAttempt(10, settings));
        Assert.False(tracker.RecordSpellAttempt(10, settings));
        Assert.True(tracker.RecordSpellAttempt(10, settings));

        // Tripping is not a verdict on the monster: nothing is suppressed and
        // the count starts over.
        Assert.Equal(CombatSuppressionReason.None, tracker.Reason(10, 100));
        Assert.False(tracker.RecordSpellAttempt(10, settings));
    }

    [Fact]
    public void AnAnsweredCastStartsTheUnansweredCountOver()
    {
        var tracker = new CombatFailureTracker();
        var settings = new CombatSettings
        {
            GhostMonsterSpellAttemptCount = 2,
        };
        tracker.ObserveTargets([Target(0)], 0, settings);

        Assert.False(tracker.RecordSpellAttempt(10, settings));
        Assert.False(tracker.RecordSpellAttempt(10, settings));
        tracker.ResetSpellAttempts(10);
        Assert.False(tracker.RecordSpellAttempt(10, settings));
        Assert.False(tracker.RecordSpellAttempt(10, settings));
        Assert.True(tracker.RecordSpellAttempt(10, settings));
    }

    private static PluginCombatTarget Target(long healthRevision) => new(
        10,
        "Drudge",
        100,
        5,
        0,
        true,
        1f)
    {
        HealthRevision = healthRevision,
        SecondsSinceHealthUpdate = 0,
    };
}

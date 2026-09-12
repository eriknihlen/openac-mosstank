using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class CombatFailureTrackerTests
{
    [Fact]
    public void HealthUpdateCancelsAccumulatedMisses()
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
            CombatSuppressionReason.None,
            tracker.Reason(10, 3));
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
    public void FailedSpellStartsMarkPersistentGhost()
    {
        var tracker = new CombatFailureTracker();
        var settings = new CombatSettings
        {
            DeleteGhostMonsters = true,
            GhostMonsterSpellAttemptCount = 2,
        };
        tracker.ObserveTargets([Target(0)], 0, settings);

        tracker.RecordSpellDidNotStart(10, settings);
        Assert.Equal(CombatSuppressionReason.None, tracker.Reason(10, 0));
        tracker.RecordSpellDidNotStart(10, settings);
        Assert.Equal(CombatSuppressionReason.Ghost, tracker.Reason(10, 100));
    }

    [Fact]
    public void HealthAgeDetectorRequiresEngagementAndConfiguredDelay()
    {
        var tracker = new CombatFailureTracker();
        var settings = new CombatSettings
        {
            DeleteGhostMonstersByHealthTracker = true,
            GhostDeleteHealthTrackerSeconds = 10,
        };
        PluginCombatTarget stale = Target(1) with
        {
            SecondsSinceHealthUpdate = 50,
        };

        tracker.ObserveTargets([stale], 0, settings);
        Assert.Equal(CombatSuppressionReason.None, tracker.Reason(10, 0));
        tracker.BeginEngagement(10, 0);
        tracker.ObserveTargets([stale], 9.9, settings);
        Assert.Equal(CombatSuppressionReason.None, tracker.Reason(10, 9.9));
        tracker.ObserveTargets([stale], 10, settings);
        Assert.Equal(CombatSuppressionReason.Ghost, tracker.Reason(10, 10));
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

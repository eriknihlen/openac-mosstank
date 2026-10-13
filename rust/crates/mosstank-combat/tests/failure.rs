//! The monsters the fight gives up on.

use ac_plugin_abi::combat::CombatTarget;
use mosstank_combat::failure::{CombatFailureTracker, CombatSuppressionReason};
use mosstank_settings::model::CombatSettings;

fn target(health_revision: i64) -> CombatTarget {
    CombatTarget {
        object_id: 10,
        name: "Drudge".to_owned(),
        weenie_class_id: 100,
        distance: 5.0,
        relative_angle_degrees: 0.0,
        is_health_known: true,
        health_fraction: 1.0,
        health_revision,
        seconds_since_health_update: 0.0,
        ..CombatTarget::default()
    }
}

fn settings(attempts: i32, timeout: f64) -> CombatSettings {
    CombatSettings {
        blacklist_monster_attempt_count: attempts,
        blacklist_monster_timeout_seconds: timeout,
        ..CombatSettings::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatFailureTrackerTests.cs :: SomebodyElsesDamageDoesNotCancelAccumulatedMisses
#[test]
fn somebody_elses_damage_does_not_cancel_accumulated_misses() {
    let mut tracker = CombatFailureTracker::new();
    let settings = settings(2, 30.0);
    tracker.observe_targets(&[target(1)], 0.0, &settings);
    tracker.record_miss(10, 1.0, &settings);
    tracker.record_miss(10, 1.0, &settings);

    tracker.observe_targets(&[target(2)], 2.0, &settings);
    tracker.record_miss(10, 3.0, &settings);

    assert_eq!(
        tracker.reason(10, 3.0),
        CombatSuppressionReason::Blacklisted
    );

    // Our own damage report is the one thing that does clear it.
    tracker.observe_targets(&[target(3)], 40.0, &settings);
    tracker.record_miss(10, 40.0, &settings);
    tracker.reset_attempts(10);
    tracker.record_miss(10, 41.0, &settings);
    tracker.record_miss(10, 41.0, &settings);

    assert_eq!(tracker.reason(10, 41.0), CombatSuppressionReason::None);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatFailureTrackerTests.cs :: TheAllowedAttemptCountIsSpentBeforeTheBlacklistTrips
#[test]
fn the_allowed_attempt_count_is_spent_before_the_blacklist_trips() {
    let mut tracker = CombatFailureTracker::new();
    let settings = settings(2, 20.0);
    tracker.observe_targets(&[target(1)], 0.0, &settings);

    tracker.record_miss(10, 1.0, &settings);
    tracker.record_miss(10, 1.0, &settings);
    assert_eq!(tracker.reason(10, 1.0), CombatSuppressionReason::None);

    tracker.record_miss(10, 1.0, &settings);
    assert_eq!(
        tracker.reason(10, 10.0),
        CombatSuppressionReason::Blacklisted
    );

    tracker.observe_targets(&[target(1)], 23.0, &settings);
    assert_eq!(tracker.reason(10, 23.0), CombatSuppressionReason::None);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatFailureTrackerTests.cs :: OnlyACreatureThePassHasSeenCanBeGivenUpOn
#[test]
fn only_a_creature_the_pass_has_seen_can_be_given_up_on() {
    let mut tracker = CombatFailureTracker::new();
    let settings = settings(1, 120.0);
    tracker.observe_targets(&[target(0)], 0.0, &settings);

    let me = 1_342_177_290_u32;
    assert!(!tracker.record_miss(me, 0.0, &settings));
    assert!(!tracker.record_miss(me, 0.0, &settings));
    assert!(!tracker.record_miss(me, 0.0, &settings));
    assert_eq!(tracker.reason(me, 1.0), CombatSuppressionReason::None);
    assert!(!tracker.is_known(me));

    // The monster beside it still trips on the attempt after the allowance.
    assert!(!tracker.record_miss(10, 0.0, &settings));
    assert!(tracker.record_miss(10, 0.0, &settings));
    assert_eq!(
        tracker.reason(10, 1.0),
        CombatSuppressionReason::Blacklisted
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatFailureTrackerTests.cs :: AnOutrightRefusalOnlyBlacklistsACreature
#[test]
fn an_outright_refusal_only_blacklists_a_creature() {
    let mut tracker = CombatFailureTracker::new();
    let settings = CombatSettings {
        blacklist_monster_timeout_seconds: 120.0,
        ..CombatSettings::default()
    };
    tracker.observe_targets(&[target(0)], 0.0, &settings);

    tracker.force_blacklist(1_342_177_290, 0.0, &settings);
    assert_eq!(
        tracker.reason(1_342_177_290, 1.0),
        CombatSuppressionReason::None
    );

    tracker.force_blacklist(10, 0.0, &settings);
    assert_eq!(
        tracker.reason(10, 1.0),
        CombatSuppressionReason::Blacklisted
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatFailureTrackerTests.cs :: ALaterShorterTripDoesNotShortenALiveBlacklist
#[test]
fn a_later_shorter_trip_does_not_shorten_a_live_blacklist() {
    let mut tracker = CombatFailureTracker::new();
    let mut settings = settings(0, 100.0);
    tracker.observe_targets(&[target(0)], 0.0, &settings);
    tracker.record_miss(10, 0.0, &settings);

    settings.blacklist_monster_timeout_seconds = 1.0;
    tracker.record_miss(10, 10.0, &settings);

    assert_eq!(
        tracker.reason(10, 50.0),
        CombatSuppressionReason::Blacklisted
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatFailureTrackerTests.cs :: ReachingTheMonsterResetsTheCountWithoutLiftingALiveBlacklist
#[test]
fn reaching_the_monster_resets_the_count_without_lifting_a_live_blacklist() {
    let mut tracker = CombatFailureTracker::new();
    let settings = settings(2, 100.0);
    tracker.observe_targets(&[target(0)], 0.0, &settings);
    tracker.record_miss(10, 0.0, &settings);
    tracker.record_miss(10, 0.0, &settings);
    tracker.reset_attempts(10);

    // The count started over, so three more are needed to trip.
    tracker.record_miss(10, 0.0, &settings);
    tracker.record_miss(10, 0.0, &settings);
    assert_eq!(tracker.reason(10, 1.0), CombatSuppressionReason::None);

    tracker.record_miss(10, 0.0, &settings);
    assert_eq!(
        tracker.reason(10, 1.0),
        CombatSuppressionReason::Blacklisted
    );

    tracker.reset_attempts(10);
    assert_eq!(
        tracker.reason(10, 1.0),
        CombatSuppressionReason::Blacklisted
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatFailureTrackerTests.cs :: UnansweredCastsTripOnTheAttemptAfterTheAllowanceThenStartOver
#[test]
fn unanswered_casts_trip_on_the_attempt_after_the_allowance_then_start_over() {
    let mut tracker = CombatFailureTracker::new();
    let settings = CombatSettings {
        ghost_monster_spell_attempt_count: 2,
        ..CombatSettings::default()
    };
    tracker.observe_targets(&[target(0)], 0.0, &settings);

    assert!(!tracker.record_spell_attempt(10, &settings));
    assert!(!tracker.record_spell_attempt(10, &settings));
    assert!(tracker.record_spell_attempt(10, &settings));

    // Tripping is not a verdict: nothing is suppressed and the count starts
    // over.
    assert_eq!(tracker.reason(10, 100.0), CombatSuppressionReason::None);
    assert!(!tracker.record_spell_attempt(10, &settings));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatFailureTrackerTests.cs :: AnAnsweredCastStartsTheUnansweredCountOver
#[test]
fn an_answered_cast_starts_the_unanswered_count_over() {
    let mut tracker = CombatFailureTracker::new();
    let settings = CombatSettings {
        ghost_monster_spell_attempt_count: 2,
        ..CombatSettings::default()
    };
    tracker.observe_targets(&[target(0)], 0.0, &settings);

    assert!(!tracker.record_spell_attempt(10, &settings));
    assert!(!tracker.record_spell_attempt(10, &settings));
    tracker.reset_spell_attempts(10);
    assert!(!tracker.record_spell_attempt(10, &settings));
    assert!(!tracker.record_spell_attempt(10, &settings));
    assert!(tracker.record_spell_attempt(10, &settings));
}

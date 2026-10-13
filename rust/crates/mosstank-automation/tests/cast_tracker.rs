//! One spell cast in flight.

use ac_plugin_abi::magic::SpellInfo;
use mosstank_automation::cast_tracker::{
    CastEvent, CastRequest, SpellCastOutcome, SpellCastOutcomeInfo, SpellCastTracker,
    SpellCastTrackerState,
};
use mosstank_automation::combat_text::log_text_type;
use mosstank_automation::locks::ActionLockTable;

/// The log a spell result is written to.
const MAGIC_LOG: u32 = 0x07;

fn armed_with(hits_multiple_targets: bool) -> SpellCastTracker {
    let mut tracker = SpellCastTracker::new();
    tracker.begin(&CastRequest {
        saying: "testwords",
        school: SpellCastTracker::WAR_MAGIC_SCHOOL,
        can_kill: true,
        ..CastRequest::new(100, "Flame Bolt VII", 10, "Drudge", hits_multiple_targets)
    });
    tracker
}

fn armed() -> SpellCastTracker {
    armed_with(false)
}

fn speak(tracker: &mut SpellCastTracker) {
    tracker.observe_chat(0, "testwords", true, log_text_type::SPELLCASTING);
}

fn plain(tracker: &mut SpellCastTracker, sequence: u64, text: &str) {
    tracker.observe_chat(sequence, text, false, log_text_type::DEFAULT);
}

fn magic(tracker: &mut SpellCastTracker, sequence: u64, text: &str) {
    tracker.observe_chat(sequence, text, false, MAGIC_LOG);
}

/// The outcomes raised since the last call (the C#'s `Completed` log).
fn outcomes(tracker: &mut SpellCastTracker) -> Vec<SpellCastOutcomeInfo> {
    tracker
        .take_events()
        .into_iter()
        .filter_map(|event| match event {
            CastEvent::Completed(info) => Some(info),
            _ => None,
        })
        .collect()
}

fn single(tracker: &mut SpellCastTracker) -> SpellCastOutcomeInfo {
    let mut all = outcomes(tracker);
    assert_eq!(all.len(), 1, "{all:?}");
    all.remove(0)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: ArmingLeavesIdle
#[test]
fn arming_leaves_idle() {
    let tracker = armed();
    assert!(tracker.is_busy());
    assert_eq!(tracker.state(), SpellCastTrackerState::AwaitingLaunch);
    assert_eq!(tracker.spell_id(), 100);
    assert_eq!(tracker.target_object_id(), 10);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: SpellSpeechStartsTheResultWaitAndDoesNotReleaseTheLatch
#[test]
fn spell_speech_starts_the_result_wait_and_does_not_release_the_latch() {
    let mut tracker = armed();
    speak(&mut tracker);
    assert!(tracker.is_busy());
    assert_eq!(tracker.state(), SpellCastTrackerState::AwaitingResult);
    assert!(outcomes(&mut tracker).is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: ASuccessLineReleasesTheLatch
#[test]
fn a_success_line_releases_the_latch() {
    let mut tracker = armed();
    speak(&mut tracker);
    magic(
        &mut tracker,
        1,
        "You blast Drudge for 42 points with Flame Bolt VII.",
    );
    assert!(!tracker.is_busy());
    assert_eq!(single(&mut tracker).outcome, SpellCastOutcome::Success);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: AKillLineReleasesTheLatchAndNamesTheTarget
#[test]
fn a_kill_line_releases_the_latch_and_names_the_target() {
    let mut tracker = armed();
    speak(&mut tracker);
    plain(&mut tracker, 1, "You killed Drudge!");
    assert!(!tracker.is_busy());
    let info = single(&mut tracker);
    assert_eq!(info.outcome, SpellCastOutcome::Kill);
    assert_eq!(info.target_object_id, 10);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: AResultNamingAnotherSpellIsNotThisCastsResult
#[test]
fn a_result_naming_another_spell_is_not_this_casts_result() {
    let mut tracker = armed();
    speak(&mut tracker);
    magic(
        &mut tracker,
        1,
        "You blast Drudge for 42 points with Frost Bolt VII.",
    );
    assert!(tracker.is_busy());
    assert!(outcomes(&mut tracker).is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: OnlyTheKillSentenceInThePlainLogEndsTheWait
#[test]
fn only_the_kill_sentence_in_the_plain_log_ends_the_wait() {
    let mut tracker = armed();
    speak(&mut tracker);
    magic(&mut tracker, 1, "You killed Olthoi Soldier!");
    assert!(tracker.is_busy());
    assert!(outcomes(&mut tracker).is_empty());

    plain(&mut tracker, 2, "You killed Olthoi Soldier!");
    assert!(!tracker.is_busy());
    assert_eq!(single(&mut tracker).outcome, SpellCastOutcome::Kill);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: APermanentFailIsIgnoredForAMultiTargetSpell
#[test]
fn a_permanent_fail_is_ignored_for_a_multi_target_spell() {
    let mut tracker = armed_with(true);
    speak(&mut tracker);
    magic(&mut tracker, 1, "Drudge is an invalid target.");
    assert!(tracker.is_busy());
    assert!(outcomes(&mut tracker).is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: APermanentFailEndsTheWaitForASingleTargetSpell
#[test]
fn a_permanent_fail_ends_the_wait_for_a_single_target_spell() {
    let mut tracker = armed();
    speak(&mut tracker);
    magic(&mut tracker, 1, "Drudge is an invalid target.");
    assert!(!tracker.is_busy());
    assert_eq!(
        single(&mut tracker).outcome,
        SpellCastOutcome::PermanentFail
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: TheResultWaitExpiresAtFourTicksOfNineHundredAndSevenMilliseconds
#[test]
fn the_result_wait_expires_at_four_ticks_of_nine_hundred_and_seven_milliseconds() {
    let mut tracker = armed();
    speak(&mut tracker);
    tracker.advance(3.6);
    assert!(tracker.is_busy());
    assert!(outcomes(&mut tracker).is_empty());
    tracker.advance(0.03);
    assert!(!tracker.is_busy());
    assert_eq!(
        single(&mut tracker).outcome,
        SpellCastOutcome::ResultTimeout
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: TheLaunchWaitExpiresAfterFiveSeconds
#[test]
fn the_launch_wait_expires_after_five_seconds() {
    let mut tracker = armed();
    tracker.advance(4.9);
    assert!(tracker.is_busy());
    tracker.advance(0.2);
    assert!(!tracker.is_busy());
    assert_eq!(
        single(&mut tracker).outcome,
        SpellCastOutcome::LaunchTimeout
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: TheSameChatLineIsFoldedOnce
#[test]
fn the_same_chat_line_is_folded_once() {
    let mut tracker = armed();
    speak(&mut tracker);
    plain(&mut tracker, 7, "You killed Drudge!");
    tracker.begin(&CastRequest {
        saying: "testwords",
        school: SpellCastTracker::WAR_MAGIC_SCHOOL,
        can_kill: true,
        ..CastRequest::new(100, "Flame Bolt VII", 10, "Drudge", false)
    });
    speak(&mut tracker);
    plain(&mut tracker, 7, "You killed Drudge!");
    assert_eq!(outcomes(&mut tracker).len(), 1);
    assert!(tracker.is_busy());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: RepeatedSpeechDoesNotRestartTheResultTimer
#[test]
fn repeated_speech_does_not_restart_the_result_timer() {
    let mut tracker = armed();
    speak(&mut tracker);
    tracker.advance(3.0);
    speak(&mut tracker);
    tracker.advance(0.7);
    assert_eq!(
        single(&mut tracker).outcome,
        SpellCastOutcome::ResultTimeout
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: OnlyTheAwaitedTargetsDeletionEndsTheWait
#[test]
fn only_the_awaited_targets_deletion_ends_the_wait() {
    let mut tracker = armed();
    tracker.reset_for_target(11);
    assert!(tracker.is_busy());
    tracker.reset_for_target(10);
    assert!(!tracker.is_busy());
    assert!(outcomes(&mut tracker).is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: AResultBeforeSpellSpeechDoesNotEndTheLaunchWait
#[test]
fn a_result_before_spell_speech_does_not_end_the_launch_wait() {
    let mut tracker = armed();
    plain(&mut tracker, 1, "You killed Drudge!");
    magic(
        &mut tracker,
        2,
        "You blast Drudge for 42 points with Flame Bolt VII.",
    );
    assert_eq!(tracker.state(), SpellCastTrackerState::AwaitingLaunch);
    assert!(outcomes(&mut tracker).is_empty());
    tracker.advance(SpellCastTracker::LAUNCH_TIMEOUT_SECONDS);
    assert_eq!(
        single(&mut tracker).outcome,
        SpellCastOutcome::LaunchTimeout
    );
}

fn spell(components: &[u32], family: u32) -> SpellInfo {
    SpellInfo {
        spell_id: 100,
        name: "Spell".to_owned(),
        family,
        tier: 1,
        difficulty: 10,
        mana_cost: 5,
        school: 34,
        description: "d".to_owned(),
        formula_component_ids: components.to_vec(),
        ..SpellInfo::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: HitsMultipleTargetsMatchesTheSpellTable
#[test]
fn hits_multiple_targets_matches_the_spell_table() {
    let cases: [(&[u32], u32, bool); 4] = [
        (&[], 0, false),
        (&[110, 77], 0, true),
        (&[77, 110], 0, false),
        (&[], 638, true),
    ];
    for (components, family, expected) in cases {
        assert_eq!(
            SpellCastTracker::hits_multiple_targets_for(&spell(components, family)),
            expected,
            "{components:?} {family}"
        );
    }
}

fn self_buff() -> SpellCastTracker {
    let mut tracker = SpellCastTracker::new();
    tracker.begin(&CastRequest {
        saying: "casfaen",
        ..CastRequest::new(1, "Strength Self VI", 5, "yourself", false)
    });
    tracker
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: TheSpokenEchoMovesTheTrackerFromLaunchToTheResultWait
#[test]
fn the_spoken_echo_moves_the_tracker_from_launch_to_the_result_wait() {
    let mut tracker = self_buff();
    assert_eq!(tracker.state(), SpellCastTrackerState::AwaitingLaunch);
    tracker.observe_chat(1, "Cas Faen", true, log_text_type::SPELLCASTING);
    assert_eq!(tracker.state(), SpellCastTrackerState::AwaitingResult);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: ASpokenEchoNamingAnotherSpellEndsTheWait
#[test]
fn a_spoken_echo_naming_another_spell_ends_the_wait() {
    let mut tracker = self_buff();
    tracker.observe_chat(1, "Zharalim Retaanu", true, log_text_type::SPELLCASTING);
    assert_eq!(tracker.state(), SpellCastTrackerState::Idle);
    assert!(!tracker.is_busy());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: AnotherPlayerSpeakingTheSameWordsIsNotOurEcho
#[test]
fn another_player_speaking_the_same_words_is_not_our_echo() {
    let mut tracker = self_buff();
    tracker.observe_chat(1, "Cas Faen", false, log_text_type::SPELLCASTING);
    assert_eq!(tracker.state(), SpellCastTrackerState::AwaitingLaunch);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: OurOwnTypedWordsAreNotTheGestureEcho
#[test]
fn our_own_typed_words_are_not_the_gesture_echo() {
    let mut tracker = self_buff();
    tracker.observe_chat(1, "Cas Faen", true, log_text_type::DEFAULT);
    assert_eq!(tracker.state(), SpellCastTrackerState::AwaitingLaunch);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: TheRavenFurySayingClassifiesItAsMultiTarget
#[test]
fn the_raven_fury_saying_classifies_it_as_multi_target() {
    let mut raven = SpellInfo {
        spell_id: 1,
        name: "Curse of Raven Fury".to_owned(),
        family: 500,
        tier: 1,
        difficulty: 10,
        mana_cost: 5,
        duration_seconds: 60.0,
        school: 31,
        saying: "tugakquati".to_owned(),
        ..SpellInfo::default()
    };
    assert!(SpellCastTracker::hits_multiple_targets_for(&raven));
    raven.saying = "casfaen".to_owned();
    assert!(!SpellCastTracker::hits_multiple_targets_for(&raven));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: OnlyASpellThatCanKillClaimsAKillingBlow
#[test]
fn only_a_spell_that_can_kill_claims_a_killing_blow() {
    let mut tracker = SpellCastTracker::new();
    tracker.begin(&CastRequest {
        saying: "testwords",
        school: 31,
        can_kill: false,
        ..CastRequest::new(200, "Imperil Other VII", 10, "Drudge", false)
    });
    speak(&mut tracker);
    plain(&mut tracker, 1, "You killed Drudge!");
    assert!(tracker.is_busy());
    assert!(outcomes(&mut tracker).is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: AResistEndsTheWaitWhateverNameItCarries
#[test]
fn a_resist_ends_the_wait_whatever_name_it_carries() {
    let mut tracker = armed();
    speak(&mut tracker);
    magic(&mut tracker, 1, "Olthoi Soldier resists your spell");
    assert!(!tracker.is_busy());
    assert_eq!(single(&mut tracker).outcome, SpellCastOutcome::Fail);
}

fn void_bolt(locks: &mosstank_automation::SharedActionLocks, can_kill: bool) -> SpellCastTracker {
    let mut tracker = SpellCastTracker::new();
    tracker.bind_action_locks(locks.clone());
    tracker.begin(&CastRequest {
        saying: "testwords",
        school: SpellCastTracker::VOID_MAGIC_SCHOOL,
        can_kill,
        ..CastRequest::new(300, "Nether Bolt VII", 10, "Drudge", false)
    });
    tracker
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: FinishingAVoidCastHoldsWarOffForFiveAndAHalfSeconds
#[test]
fn finishing_a_void_cast_holds_war_off_for_five_and_a_half_seconds() {
    let locks = ActionLockTable::shared();
    let mut tracker = void_bolt(&locks, false);
    speak(&mut tracker);
    assert!(!tracker.is_school_locked_out(SpellCastTracker::WAR_MAGIC_SCHOOL));

    magic(&mut tracker, 1, "You cast Nether Bolt VII on Drudge");
    assert!(tracker.is_school_locked_out(SpellCastTracker::WAR_MAGIC_SCHOOL));
    assert!(!tracker.is_school_locked_out(SpellCastTracker::VOID_MAGIC_SCHOOL));

    locks.borrow_mut().advance(5.4);
    assert!(tracker.is_school_locked_out(SpellCastTracker::WAR_MAGIC_SCHOOL));
    locks.borrow_mut().advance(0.2);
    assert!(!tracker.is_school_locked_out(SpellCastTracker::WAR_MAGIC_SCHOOL));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: ACastThatNeverLaunchedArmsNoLockout
#[test]
fn a_cast_that_never_launched_arms_no_lockout() {
    let locks = ActionLockTable::shared();
    let mut tracker = void_bolt(&locks, false);
    tracker.advance(SpellCastTracker::LAUNCH_TIMEOUT_SECONDS + 0.1);
    assert_eq!(tracker.state(), SpellCastTrackerState::Idle);
    assert!(!tracker.is_school_locked_out(SpellCastTracker::WAR_MAGIC_SCHOOL));
}

fn reissues(tracker: &mut SpellCastTracker) -> Vec<(u32, u32)> {
    tracker
        .take_events()
        .into_iter()
        .filter_map(|event| match event {
            CastEvent::Reissue {
                spell_id,
                target_object_id,
            } => Some((spell_id, target_object_id)),
            _ => None,
        })
        .collect()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: AnUnacknowledgedCastIsSentAgainUntilTheBudgetRunsOut
#[test]
fn an_unacknowledged_cast_is_sent_again_until_the_budget_runs_out() {
    let mut tracker = SpellCastTracker::new();
    tracker.begin(&CastRequest::new(
        100,
        "Flame Bolt VII",
        10,
        "Drudge",
        false,
    ));
    let mut sent = Vec::new();
    for _ in 0..60 {
        tracker.advance(0.1);
        sent.extend(reissues(&mut tracker));
    }
    assert_eq!(tracker.state(), SpellCastTrackerState::Idle);
    assert!((24..=25).contains(&sent.len()), "{}", sent.len());
    assert!(sent.iter().all(|&entry| entry == (100, 10)));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: NearlyOutOfManaTheCastIsSentAgainTwiceAsOften
#[test]
fn nearly_out_of_mana_the_cast_is_sent_again_twice_as_often() {
    let mut tracker = SpellCastTracker::new();
    tracker.begin(&CastRequest {
        saying: "testwords",
        current_mana: 9,
        ..CastRequest::new(100, "Flame Bolt VII", 10, "Drudge", false)
    });
    let mut count = 0;
    for _ in 0..60 {
        tracker.advance(0.1);
        count += reissues(&mut tracker).len();
    }
    assert!((49..=50).contains(&count), "{count}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/SpellCastTrackerTests.cs :: TheNudgeWindowIsOneResultTickLong
#[test]
fn the_nudge_window_is_one_result_tick_long() {
    let mut tracker = armed();
    assert!(!tracker.jiggle_window_open());
    speak(&mut tracker);
    assert!(tracker.jiggle_window_open());
    tracker.advance(SpellCastTracker::RESULT_TICK_SECONDS - 0.01);
    assert!(tracker.jiggle_window_open());
    tracker.advance(0.02);
    assert!(!tracker.jiggle_window_open());
}

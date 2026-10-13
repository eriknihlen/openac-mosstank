//! The swing, the turn, the kill sentence and giving a monster up
//! (`CombatControllerTests`, seventh part).

mod common;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ac_plugin_abi::combat::{AttackHeight, CombatMode, CombatTarget};
use ac_plugin_abi::magic::SpellInfo;
use ac_plugin_abi::types::ChatLine;
use common::{
    FakeAutomation, Fight, fire_attack_rule, in_mode, nav_object, navigation_at, physical,
    profile_fixture_weapon, rule, spell, target, wielded_caster, wielded_planned_weapon,
};
use mosstank_automation::locks::{ActionLockKind, ActionLockTable, SharedActionLocks};
use mosstank_settings::model::{CombatSettings, MeleeAttackHeight, TargetSelectionMethod};

/// `ChatLine`: a line the client logged; the log type names the log it
/// came from (0 a plain line, 0x16 the character's own combat log).
fn chat_line(sequence: u64, text: &str, log_text_type: u32) -> ChatLine {
    ChatLine {
        sequence,
        text: text.to_owned(),
        log_text_type,
        ..ChatLine::default()
    }
}

fn line(sequence: u64, text: &str) -> ChatLine {
    chat_line(sequence, text, 0)
}

fn selected(id: u32) -> ac_plugin_abi::combat::CombatSnapshot {
    let mut snapshot = physical();
    snapshot.selected_id = id;
    snapshot
}

/// `MeleeKillRig`: one drudge, the fixture weapon, the locks bound, the
/// fight started and one turn taken.
fn melee_kill_rig(
    settings: Option<CombatSettings>,
    tracks_attack_requests: bool,
) -> (Fight, SharedActionLocks) {
    let surface = FakeAutomation {
        combat_snapshot: selected(10),
        targets: vec![target(10, "Drudge", 2.0, 0.0)],
        equipment_items: vec![wielded_planned_weapon(991)],
        tracks_attack_requests,
        ..FakeAutomation::default()
    };
    let mut resolved = settings.unwrap_or_default();
    resolved.scan_interval_seconds = 0.05;
    profile_fixture_weapon(&mut resolved);
    let locks = Rc::new(RefCell::new(ActionLockTable::new()));
    let mut fight = Fight::new(surface, resolved);
    fight
        .controller
        .bind_action_locks(locks.clone(), Box::new(|| true));
    fight.toggle();
    fight.tick(0.25);
    (fight, locks)
}

fn blacklisting(timeout: f64) -> Option<CombatSettings> {
    Some(CombatSettings {
        blacklist_monster_attempt_count: 1,
        blacklist_monster_timeout_seconds: timeout,
        ..CombatSettings::default()
    })
}

fn locked(locks: &SharedActionLocks, kind: ActionLockKind) -> bool {
    locks.borrow_mut().is_locked(kind)
}

fn said(fight: &Fight, text: &str) -> bool {
    fight
        .surface
        .posted_system_messages
        .iter()
        .any(|message| message.contains(text))
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AnAutoRepeatSwingDoesNotFreezeTargetSelection
#[test]
fn an_auto_repeat_swing_does_not_freeze_target_selection() {
    let surface = FakeAutomation {
        combat_snapshot: physical(),
        targets: vec![target(10, "Drudge", 3.0, 0.0)],
        equipment_items: vec![wielded_planned_weapon(991)],
        ..FakeAutomation::default()
    };
    let mut settings = CombatSettings {
        maximum_range: 20.0,
        selection_method: TargetSelectionMethod::RANGE,
        scan_interval_seconds: 0.05,
        ..CombatSettings::default()
    };
    settings.rules.insert(0, rule("name#^Olthoi", 4));
    profile_fixture_weapon(&mut settings);
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_begin_target, 10);

    // The swing loop runs beside the rule pass and must not stop the pass
    // re-picking.
    let mut repeating = physical();
    repeating.repeat_attack_in_progress = true;
    fight.surface.combat_snapshot = repeating;
    fight.surface.targets = vec![
        target(10, "Drudge", 3.0, 0.0),
        target(20, "Olthoi Soldier", 15.0, 60.0),
    ];
    fight.tick(0.25);

    assert!(fight.controller.target_text().contains("Olthoi Soldier"));
}

fn turning_fight() -> Fight {
    let mut surface = FakeAutomation {
        combat_snapshot: in_mode(CombatMode::Magic),
        targets: vec![target(10, "Drudge", 5.0, 0.0)],
        known_attack_spells: vec![SpellInfo {
            requires_turn_to: true,
            ..spell(100, "Incantation of Flame Bolt")
        }],
        equipment_items: vec![wielded_caster(990)],
        navigation_snapshot: navigation_at(0.0),
        ..FakeAutomation::default()
    };
    surface
        .navigation_objects
        .insert(10, nav_object(10, "Drudge", 0.1, 0.0));
    Fight::new(
        surface,
        fire_attack_rule(CombatSettings {
            use_breakable_turn_to: true,
            ..CombatSettings::default()
        }),
    )
}

fn advance_held_turn(fight: &mut Fight, seconds: f64) {
    fight
        .controller
        .advance_held_turn(&mut fight.surface, seconds);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ATurnInFlightHoldsTheWholePassUntilTheCharacterHasFaced
#[test]
fn a_turn_in_flight_holds_the_whole_pass_until_the_character_has_faced() {
    let mut fight = turning_fight();
    let suspends = Rc::new(Cell::new(0));
    let resumes = Rc::new(Cell::new(0));
    let (s, r) = (suspends.clone(), resumes.clone());
    fight.controller.bind_pass_suspension(
        Box::new(move || s.set(s.get() + 1)),
        Box::new(move || r.set(r.get() + 1)),
    );

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(suspends.get(), 1);
    assert_eq!(resumes.get(), 0);
    assert!(fight.surface.cast_spell_ids.is_empty());

    // The pass is frozen; the turn is driven beside it and does not raise
    // a second hold.
    advance_held_turn(&mut fight, 0.25);
    advance_held_turn(&mut fight, 0.25);
    assert_eq!(suspends.get(), 1);
    assert_eq!(resumes.get(), 0);

    fight.surface.navigation_snapshot = navigation_at(90.0);
    advance_held_turn(&mut fight, 0.25);

    assert_eq!(resumes.get(), 1);

    fight.tick(0.25);
    assert_eq!(fight.surface.cast_spell_ids, [100]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: FramesDrivenDuringAHeldTurnAreNotCountedTwiceOnTheNextTurn
#[test]
fn frames_driven_during_a_held_turn_are_not_counted_twice_on_the_next_turn() {
    let mut fight = turning_fight();
    fight
        .controller
        .bind_pass_suspension(Box::new(|| {}), Box::new(|| {}));

    fight.toggle();
    fight.tick(0.25);
    assert!((fight.controller.clock_seconds() - 0.25).abs() < 1e-6);

    advance_held_turn(&mut fight, 0.25);
    advance_held_turn(&mut fight, 0.25);
    advance_held_turn(&mut fight, 0.25);
    assert!((fight.controller.clock_seconds() - 1.0).abs() < 1e-6);

    // The turn comes back after the whole 1.0 s gap; 0.75 s of it the
    // frames already counted.
    fight.surface.navigation_snapshot = navigation_at(90.0);
    fight.tick(1.0);
    assert!((fight.controller.clock_seconds() - 1.25).abs() < 1e-6);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ASwingThatNeverLeftTheClientLetsTheMonsterGoAndTakesTheNext
#[test]
fn a_swing_that_never_left_the_client_lets_the_monster_go_and_takes_the_next() {
    let mut surface = FakeAutomation {
        combat_snapshot: selected(10),
        targets: vec![
            target(10, "Drudge", 2.0, 0.0),
            target(20, "Banderling", 3.0, 0.0),
        ],
        equipment_items: vec![wielded_planned_weapon(991)],
        tracks_attack_requests: true,
        ..FakeAutomation::default()
    };
    // The creature is dead on the server; nothing the macro can see says so
    // yet, and the host will not let a swing out at it.
    surface.unreleasable_targets.insert(10);
    let mut settings = CombatSettings {
        scan_interval_seconds: 0.05,
        ..CombatSettings::default()
    };
    profile_fixture_weapon(&mut settings);
    let mut fight = Fight::new(surface, settings);
    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_begin_target, 10);

    // The bar fills and the macro lets go of the swing.
    fight.surface.combat_snapshot.power_bar_level = 1.0;
    fight.surface.combat_snapshot.desired_power = 1.0;
    fight.tick(0.25);
    assert_eq!(fight.surface.release_count, 1);

    // One pass past the shot floor the character swings at the live one.
    fight.tick(1.0);
    assert_eq!(fight.surface.last_begin_target, 20);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheProfilePowerAndHeightGoOutWhereverThePlayerLeftTheBar
#[test]
fn the_profile_power_and_height_go_out_wherever_the_player_left_the_bar() {
    // The player left the marker at a quarter.
    let mut snapshot = selected(10);
    snapshot.desired_power = 0.25;
    let surface = FakeAutomation {
        combat_snapshot: snapshot,
        targets: vec![target(10, "Drudge", 2.0, 0.0)],
        equipment_items: vec![wielded_planned_weapon(991)],
        ..FakeAutomation::default()
    };
    let mut settings = CombatSettings {
        scan_interval_seconds: 0.05,
        attack_height: MeleeAttackHeight::HIGH,
        auto_attack_power: true,
        use_recklessness: false,
        ..CombatSettings::default()
    };
    profile_fixture_weapon(&mut settings);
    let mut fight = Fight::new(surface, settings);
    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_begin_target, 10);
    assert!((fight.surface.last_begin_power - 1.0).abs() < 1e-3);
    assert_eq!(fight.surface.last_begin_height, AttackHeight::High);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AKillLineArrivingAfterTheHostDroppedTheSelectionStillCredits
#[test]
fn a_kill_line_arriving_after_the_host_dropped_the_selection_still_credits() {
    let (mut fight, locks) = melee_kill_rig(None, false);
    assert_eq!(fight.surface.last_begin_target, 10);
    fight.surface.targets = vec![
        target(10, "Drudge", 2.0, 0.0),
        target(20, "Mosswart", 3.0, 0.0),
    ];

    // The host answers the death first: the selection is gone before the
    // sentence is logged.
    fight.surface.combat_snapshot.selected_id = 0;
    fight.surface.chat_messages = vec![line(1, "You killed Drudge!")];
    fight.tick(0.25);
    fight.surface.chat_messages.clear();

    assert!(locked(&locks, ActionLockKind::Navigation));
    fight.tick(1.0);
    fight.tick(0.25);
    assert_eq!(fight.surface.last_begin_target, 20);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheHostsDeathFactAloneDropsTheTarget
#[test]
fn the_hosts_death_fact_alone_drops_the_target() {
    let (mut fight, _) = melee_kill_rig(None, false);
    assert_eq!(fight.surface.last_begin_target, 10);

    fight.surface.targets = vec![
        // Health was never asked for, so it still reads as untouched.
        CombatTarget {
            is_health_known: false,
            health_fraction: 1.0,
            is_dead: true,
            ..target(10, "Drudge", 2.0, 0.0)
        },
        target(20, "Mosswart", 3.0, 0.0),
    ];
    fight.tick(1.0);
    fight.tick(0.25);

    assert_eq!(fight.surface.last_begin_target, 20);
}

/// `CleavingWieldedWeapon`.
fn cleaving_fight(targets: Vec<CombatTarget>) -> Fight {
    let mut weapon = wielded_planned_weapon(991);
    weapon.cleaving = 2;
    let surface = FakeAutomation {
        combat_snapshot: selected(10),
        targets,
        equipment_items: vec![weapon],
        ..FakeAutomation::default()
    };
    let mut settings = CombatSettings {
        scan_interval_seconds: 0.05,
        ..CombatSettings::default()
    };
    profile_fixture_weapon(&mut settings);
    Fight::new(surface, settings)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ACleaveKillSentenceEndsTheMonsterItNames
#[test]
fn a_cleave_kill_sentence_ends_the_monster_it_names() {
    let mut fight = cleaving_fight(vec![
        target(10, "Drudge", 2.0, 0.0),
        target(20, "Mosswart", 2.0, 20.0),
    ]);
    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_begin_target, 10);

    fight.surface.chat_messages = vec![line(1, "You killed Mosswart!")];
    fight.tick(0.25);
    fight.surface.chat_messages.clear();

    // Our own monster is still the one being fought.
    assert!(fight.has_target());
    // And the one the cleave took is not picked up when it falls.
    fight.surface.targets = vec![target(20, "Mosswart", 2.0, 20.0)];
    fight.tick(0.25);
    fight.tick(0.25);
    assert_ne!(fight.surface.last_begin_target, 20);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ACleaveSentenceNamingOneOfTwoAlikeMonstersCreditsNeither
#[test]
fn a_cleave_sentence_naming_one_of_two_alike_monsters_credits_neither() {
    let mut fight = cleaving_fight(vec![
        target(10, "Drudge", 2.0, 0.0),
        target(20, "Mosswart", 2.0, 20.0),
        target(30, "Mosswart", 3.0, 40.0),
    ]);
    fight.toggle();
    fight.tick(0.25);

    fight.surface.chat_messages = vec![line(1, "You killed Mosswart!")];
    fight.tick(0.25);
    fight.surface.chat_messages.clear();

    assert!(fight.has_target());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AMeleeKillLineEndsTheTargetAndHoldsNavigation
#[test]
fn a_melee_kill_line_ends_the_target_and_holds_navigation() {
    let (mut fight, locks) = melee_kill_rig(None, false);
    assert_eq!(fight.surface.last_begin_target, 10);

    fight.surface.chat_messages = vec![line(1, "You killed Drudge!")];
    fight.tick(0.25);

    assert!(!fight.has_target());
    assert!(locked(&locks, ActionLockKind::Navigation));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: KillingTheTargetCancelsTheSwingThatWasOutstandingAtIt
#[test]
fn killing_the_target_cancels_the_swing_that_was_outstanding_at_it() {
    let (mut fight, _) = melee_kill_rig(None, true);
    fight.surface.targets = vec![
        target(10, "Drudge", 2.0, 0.0),
        target(20, "Mosswart", 3.0, 0.0),
    ];
    fight.surface.fill_power_bar();
    fight.tick(0.25);
    assert!(fight.surface.combat_snapshot.server_response_pending);
    let aborts_with_the_swing_out = fight.surface.abort_count;

    // The monster dies and the server never answers that last swing.
    fight.surface.chat_messages = vec![line(1, "You killed Drudge!")];
    fight.tick(0.25);
    fight.surface.chat_messages.clear();
    assert_eq!(fight.surface.abort_count, aborts_with_the_swing_out + 1);
    assert!(!fight.surface.combat_snapshot.server_response_pending);

    fight.tick(0.25);
    fight.tick(0.25);
    assert_eq!(fight.surface.last_begin_target, 20);
    assert!(!said(&fight, "Cannot hit Drudge ("));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AnAttackLeftOverFromADroppedMonsterIsEndedNotWaitedOn
#[test]
fn an_attack_left_over_from_a_dropped_monster_is_ended_not_waited_on() {
    let (mut fight, _) = melee_kill_rig(None, true);
    fight.surface.targets = vec![target(10, "Drudge", 2.0, 0.0)];
    fight.tick(0.25);
    assert_eq!(fight.surface.last_begin_target, 10);

    fight.surface.chat_messages = vec![line(1, "You killed Drudge!")];
    fight.tick(0.25);
    fight.surface.chat_messages.clear();
    fight.surface.targets = vec![target(20, "Mosswart", 3.0, 0.0)];
    // What the live client was seen to do: the attack goes on repeating.
    let snapshot = &mut fight.surface.combat_snapshot;
    snapshot.request_in_progress = false;
    snapshot.build_in_progress = false;
    snapshot.repeat_attack_in_progress = true;
    snapshot.selected_id = 20;

    let mut seen = Vec::new();
    for _ in 0..8 {
        fight.tick(0.25);
        seen.push(fight.controller.running_detail());
    }
    assert_eq!(fight.surface.last_begin_target, 20, "{}", seen.join(" | "));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AKillWhileTheNextSwingIsStillChargingDoesNotEndTheFight
#[test]
fn a_kill_while_the_next_swing_is_still_charging_does_not_end_the_fight() {
    let (mut fight, _) = melee_kill_rig(None, true);
    fight.surface.targets = vec![
        target(10, "Drudge", 2.0, 0.0),
        target(20, "Mosswart", 3.0, 0.0),
    ];
    fight.tick(0.25);
    assert!(fight.surface.combat_snapshot.build_in_progress);
    assert_eq!(fight.surface.last_begin_target, 10);

    fight.surface.chat_messages = vec![line(
        1,
        "Drudge catches your attack, with dire consequences!",
    )];
    fight.tick(0.25);
    fight.surface.chat_messages.clear();
    fight.surface.targets = vec![target(20, "Mosswart", 3.0, 0.0)];

    for _ in 0..12 {
        fight.tick(0.25);
    }
    assert_eq!(fight.surface.last_begin_target, 20);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: SomebodyTypingTheKillSentenceInChatChangesNothing
#[test]
fn somebody_typing_the_kill_sentence_in_chat_changes_nothing() {
    let (mut fight, locks) = melee_kill_rig(None, false);

    // Speech carries the local-speech log type, not the plain one.
    fight.surface.chat_messages = vec![chat_line(1, "You killed Drudge!", 0x02)];
    fight.tick(0.25);

    assert!(fight.has_target());
    assert!(!locked(&locks, ActionLockKind::Navigation));

    fight.surface.chat_messages = vec![line(2, "You killed Drudge!")];
    fight.tick(0.25);

    assert!(!fight.has_target());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AKillLineNamingAnotherCreatureDoesNotEndOurTarget
#[test]
fn a_kill_line_naming_another_creature_does_not_end_our_target() {
    let (mut fight, _) = melee_kill_rig(None, false);

    fight.surface.chat_messages = vec![line(1, "You killed Mosswart!")];
    fight.tick(0.25);

    assert!(fight.has_target());
    assert!(fight.controller.target_text().contains("Drudge"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: OnlyAShotIntoTheSceneryCountsTowardsGivingUpOnAMonster
#[test]
fn only_a_shot_into_the_scenery_counts_towards_giving_up_on_a_monster() {
    let (mut fight, _) = melee_kill_rig(blacklisting(300.0), false);
    fight.surface.chat_messages = vec![
        line(1, "Your missile attack hit the environment."),
        line(2, "Your missile attack hit the environment."),
    ];
    fight.tick(0.25);
    assert!(!fight.has_target());

    let (mut fight, _) = melee_kill_rig(blacklisting(300.0), false);
    fight.surface.chat_messages = vec![
        line(1, "You evade the Drudge!"),
        line(2, "The Drudge evades your attack!"),
        line(3, "You miss the Drudge!"),
    ];
    fight.tick(0.25);
    assert!(fight.has_target());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ALandedHitStartsTheGiveUpCountOver
#[test]
fn a_landed_hit_starts_the_give_up_count_over() {
    let (mut fight, _) = melee_kill_rig(blacklisting(300.0), false);

    fight.surface.chat_messages = vec![
        line(1, "Your missile attack hit the environment."),
        chat_line(
            2,
            "You slash Drudge for 43 points of slashing damage!",
            0x16,
        ),
        line(3, "Your missile attack hit the environment."),
    ];
    fight.tick(0.25);

    assert!(fight.has_target());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ASwingWhoseBarNeverMovesIsTornDownAndAskedForAgain
#[test]
fn a_swing_whose_bar_never_moves_is_torn_down_and_asked_for_again() {
    let (mut fight, _) = melee_kill_rig(blacklisting(300.0), true);
    assert_eq!(fight.surface.begin_count, 1);
    assert!(fight.surface.combat_snapshot.request_in_progress);
    let aborts_after_the_arm = fight.surface.abort_count;

    // The bar stays where it was: the host holds the request open.
    fight.tick(4.0);
    assert_eq!(fight.surface.abort_count, aborts_after_the_arm);
    assert_eq!(fight.surface.begin_count, 1);

    fight.tick(1.0);
    assert_eq!(fight.surface.abort_count, aborts_after_the_arm + 1);
    assert!(!fight.surface.combat_snapshot.request_in_progress);

    // The next pass asks again, and the monster has been charged nothing.
    fight.tick(0.25);
    assert_eq!(fight.surface.begin_count, 2);
    assert_eq!(fight.surface.last_begin_target, 10);
    assert!(!said(&fight, "Cannot hit"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ASwingThatIsNeverAnsweredIsCancelledOnceTheWaitRunsOut
#[test]
fn a_swing_that_is_never_answered_is_cancelled_once_the_wait_runs_out() {
    let (mut fight, _) = melee_kill_rig(None, true);
    assert_eq!(fight.surface.last_begin_target, 10);
    assert!(fight.surface.combat_snapshot.build_in_progress);
    let aborts_after_the_swing = fight.surface.abort_count;

    // Two seconds on the power bar before the swing is let go.
    fight.tick(2.0);
    assert_eq!(fight.surface.release_count, 0);

    fight.surface.fill_power_bar();
    fight.tick(0.25);
    assert_eq!(fight.surface.release_count, 1);
    assert!(fight.surface.combat_snapshot.server_response_pending);

    // Four seconds of silence after the send is still inside the wait.
    fight.tick(4.0);
    assert_eq!(fight.surface.abort_count, aborts_after_the_swing);

    fight.tick(0.75);
    assert_eq!(fight.surface.abort_count, aborts_after_the_swing + 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ASwingIsNotCutShortWhileTheServerIsStillClosingTheGround
#[test]
fn a_swing_is_not_cut_short_while_the_server_is_still_closing_the_ground() {
    let (mut fight, _) = melee_kill_rig(None, true);
    fight.surface.fill_power_bar();
    fight.tick(0.25);
    assert_eq!(fight.surface.release_count, 1);
    let aborts_after_the_swing = fight.surface.abort_count;

    // The range the swing went out at is the mark to measure from.
    fight.tick(4.0);

    // Most of a metre nearer: the run-in is working.
    fight.surface.targets = vec![target(10, "Drudge", 1.2, 0.0)];
    fight.tick(1.0);
    assert_eq!(fight.surface.abort_count, aborts_after_the_swing);

    // No more ground gained, and now the wait runs out.
    fight.tick(4.0);
    assert_eq!(fight.surface.abort_count, aborts_after_the_swing);
    fight.tick(1.0);
    assert_eq!(fight.surface.abort_count, aborts_after_the_swing + 1);
}

fn drive_swing_executor(fight: &mut Fight, seconds: f64) {
    fight
        .controller
        .drive_swing_executor(&mut fight.surface, seconds);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AChargedSwingIsLetGoOnTheHostFrameWithoutWaitingForAPass
#[test]
fn a_charged_swing_is_let_go_on_the_host_frame_without_waiting_for_a_pass() {
    let (mut fight, _) = melee_kill_rig(None, true);
    assert_eq!(fight.surface.begin_count, 1);
    assert_eq!(fight.surface.release_count, 0);

    fight.surface.fill_power_bar();
    // One host frame, and not a single rule pass.
    drive_swing_executor(&mut fight, 0.015);

    assert_eq!(fight.surface.release_count, 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheNextSwingIsAskedForOnTheExecutorsOwnPeriod
#[test]
fn the_next_swing_is_asked_for_on_the_executors_own_period() {
    let (mut fight, _) = melee_kill_rig(None, true);
    fight.surface.fill_power_bar();
    drive_swing_executor(&mut fight, 0.015);
    assert_eq!(fight.surface.release_count, 1);

    // The server answers and the swing is over: nothing is in flight.
    fight.surface.combat_snapshot.server_response_pending = false;
    fight.surface.combat_snapshot.power_bar_level = 0.0;

    // Half a second of frames is inside the floor the swing put up.
    for _ in 0..33 {
        drive_swing_executor(&mut fight, 0.015);
    }
    assert_eq!(fight.surface.begin_count, 1);

    // Past the floor, still without a rule pass, the next swing goes out.
    for _ in 0..34 {
        drive_swing_executor(&mut fight, 0.015);
    }
    assert_eq!(fight.surface.begin_count, 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: LosingTheTurnCallsTheSwingOffAndHoldsTheShotSlot
#[test]
fn losing_the_turn_calls_the_swing_off_and_holds_the_shot_slot() {
    let (mut fight, locks) = melee_kill_rig(None, true);
    let aborts_before = fight.surface.abort_count;

    fight.set_paused(true);

    assert_eq!(fight.surface.abort_count, aborts_before + 1);
    assert!(locked(&locks, ActionLockKind::MeleeAttackShot));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AnUnansweredSwingCostsTheMonsterNothing
#[test]
fn an_unanswered_swing_costs_the_monster_nothing() {
    let (mut fight, _) = melee_kill_rig(blacklisting(300.0), false);
    fight.surface.targets = vec![
        target(10, "Drudge", 2.0, 0.0),
        target(20, "Mosswart", 3.0, 0.0),
    ];
    fight.surface.combat_snapshot.server_response_pending = true;

    // The first pass registers the swing as sent; after that one wait runs
    // out per pass. However many run out, none of them is a miss.
    fight.tick(5.0);
    let aborts_before = fight.surface.abort_count;
    fight.tick(5.0);
    fight.tick(5.0);
    fight.tick(5.0);

    assert!(fight.surface.abort_count > aborts_before);
    assert!(!said(&fight, "Cannot hit"));
    assert!(fight.controller.target_text().contains("Drudge"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ARetiredTargetIsTakenUpAgainOnceItsWindowHasPassed
#[test]
fn a_retired_target_is_taken_up_again_once_its_window_has_passed() {
    let (mut fight, _) = melee_kill_rig(blacklisting(5.0), false);

    fight.surface.chat_messages = vec![
        line(1, "Your missile attack hit the environment."),
        line(2, "Your missile attack hit the environment."),
    ];
    fight.tick(0.25);
    fight.surface.chat_messages.clear();
    assert!(!fight.controller.target_text().contains("Drudge"));

    fight.tick(6.0);
    assert!(fight.controller.target_text().contains("Drudge"));
}

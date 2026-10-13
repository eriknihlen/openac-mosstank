//! The fight's target choice, refusals and the walk to a monster
//! (`CombatControllerTests`, first part).

mod common;

use common::{
    FakeAutomation, Fight, melee_settings, nav_object, navigation_at, physical, rule, target,
    target_with_health, wielded_planned_weapon,
};
use mosstank_combat::mover::MOVER_INTERVAL_SECONDS;
use mosstank_settings::model::CombatSettings;

fn three_monsters(health_first: f32) -> Vec<ac_plugin_abi::combat::CombatTarget> {
    vec![
        target_with_health(10, "First", 1.5, 0.0, health_first),
        target(20, "Second", 2.0, 5.0),
        target(30, "Third", 3.0, 10.0),
    ]
}

fn swinging(targets: Vec<ac_plugin_abi::combat::CombatTarget>) -> FakeAutomation {
    FakeAutomation {
        combat_snapshot: physical(),
        targets,
        equipment_items: vec![wielded_planned_weapon(991)],
        ..FakeAutomation::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ADeadMonsterIsDroppedAndTheNextOneIsAttacked
#[test]
fn a_dead_monster_is_dropped_and_the_next_one_is_attacked() {
    let mut fight = Fight::new(swinging(three_monsters(1.0)), melee_settings(8.0));

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_begin_target, 10);

    // The monster dies: the capture still carries it with no health, and
    // the host refuses a swing at it.
    fight.surface.combat_snapshot = physical();
    fight.surface.targets = three_monsters(0.0);
    fight.surface.unattackable_targets.insert(10);
    fight.surface.begin_targets.clear();
    fight.tick(1.0);
    fight.tick(0.25);

    assert_eq!(fight.surface.last_begin_target, 20);
    assert!(!fight.surface.begin_targets.contains(&10));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ARefusedSwingLetsTheTargetGoAndThePassChoosesAgain
#[test]
fn a_refused_swing_lets_the_target_go_and_the_pass_chooses_again() {
    let mut surface = swinging(vec![
        target(10, "First", 1.5, 0.0),
        target(20, "Second", 2.0, 5.0),
    ]);
    surface.unattackable_targets.insert(10);
    let mut fight = Fight::new(surface, melee_settings(8.0));

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.last_begin_target, 20);
    assert!(
        !fight
            .log_texts()
            .iter()
            .any(|line| line.to_lowercase().contains("blacklist"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: OnlyTheRefusedMonsterLeavesTheRunningAndOnlyForThatPass
#[test]
fn only_the_refused_monster_leaves_the_running_and_only_for_that_pass() {
    let mut surface = swinging(three_monsters(1.0));
    surface.unattackable_targets.insert(10);
    let mut fight = Fight::new(surface, melee_settings(8.0));

    fight.toggle();
    fight.tick(0.25);
    assert_eq!(fight.surface.last_begin_target, 20);

    fight.surface.unattackable_targets.clear();
    fight.surface.combat_snapshot = physical();
    fight.surface.targets = vec![target(10, "First", 1.5, 0.0)];
    fight.surface.begin_targets.clear();
    fight.tick(1.0);

    assert!(fight.surface.begin_targets.contains(&10));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ARepeatAtSomethingElseIsEndedRatherThanWaitedOn
#[test]
fn a_repeat_at_something_else_is_ended_rather_than_waited_on() {
    let mut snapshot = physical();
    snapshot.selected_id = 99;
    snapshot.repeat_attack_in_progress = true;
    let mut surface = swinging(vec![target(10, "Drudge", 2.0, 0.0)]);
    surface.combat_snapshot = snapshot;
    let mut fight = Fight::new(surface, melee_settings(8.0));

    fight.toggle();
    fight.tick(0.25);

    assert_eq!(fight.surface.abort_count, 1);
    assert!(fight.surface.begin_targets.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ARefusedLoneMonsterLeavesTheAttackWithNothingToClaim
#[test]
fn a_refused_lone_monster_leaves_the_attack_with_nothing_to_claim() {
    let mut surface = swinging(vec![target(10, "Only", 1.5, 0.0)]);
    surface.unattackable_targets.insert(10);
    let mut fight = Fight::new(surface, melee_settings(8.0));

    fight.toggle();
    fight.tick(0.25);

    assert!(!fight.has_target());
}

fn approach_settings(maximum_range: f64, approach_distance: f64) -> CombatSettings {
    let mut settings = CombatSettings {
        maximum_range,
        approach_distance,
        scan_interval_seconds: 0.05,
        ..CombatSettings::default()
    };
    common::profile_fixture_weapon(&mut settings);
    settings
}

fn walking(east_west: f64, north_south: f64, heading: f32) -> FakeAutomation {
    let mut surface = swinging(vec![target(10, "Drudge", 12.0, 0.0)]);
    surface.navigation_snapshot = navigation_at(heading);
    surface
        .navigation_objects
        .insert(10, nav_object(10, "Drudge", east_west, north_south));
    surface
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheAttacksTeardownLeavesTheMonsterWalkAlone
#[test]
fn the_attacks_teardown_leaves_the_monster_walk_alone() {
    let mut fight = Fight::new(walking(0.0, 0.1, 0.0), approach_settings(5.0, 20.0));

    fight.toggle();
    fight.tick(0.05);
    assert!(fight.approach(true));
    assert_eq!(fight.surface.movement_intents.len(), 1);
    let cleared_before = fight.surface.clear_movement_count;

    fight.set_paused(true);

    assert_eq!(fight.surface.clear_movement_count, cleared_before);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: LosingTheTurnLeavesTheAttacksOwnReasonStanding
#[test]
fn losing_the_turn_leaves_the_attacks_own_reason_standing() {
    let mut settings = CombatSettings {
        scan_interval_seconds: 0.05,
        ..CombatSettings::default()
    };
    common::profile_fixture_weapon(&mut settings);
    let mut fight = Fight::new(swinging(Vec::new()), settings);

    fight.toggle();
    fight.tick(0.25);
    let reason = fight.status();
    assert_eq!(reason, "Waiting for a target");

    fight.set_paused(true);

    assert_eq!(fight.status(), reason);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: LosingTheTurnStopsTheMonsterApproachWalk
#[test]
fn losing_the_turn_stops_the_monster_approach_walk() {
    let mut fight = Fight::new(walking(0.0, 0.1, 0.0), approach_settings(5.0, 20.0));

    fight.toggle();
    fight.tick(0.05);
    assert!(fight.approach(true));
    assert_eq!(fight.surface.movement_intents.len(), 1);
    let cleared_before = fight.surface.clear_movement_count;

    fight
        .controller
        .stop_monster_approach_for_lost_turn(&mut fight.surface);

    assert_eq!(fight.surface.clear_movement_count, cleared_before + 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: WalkingToAMonsterIsItsOwnJobBelowTheAttack
#[test]
fn walking_to_a_monster_is_its_own_job_below_the_attack() {
    let mut fight = Fight::new(walking(0.0, 0.1, 0.0), approach_settings(5.0, 20.0));

    fight.toggle();
    fight.tick(0.05);

    // The monster is out of weapon range: not one of the attack's
    // candidates.
    assert!(!fight.has_target());
    assert!(fight.surface.movement_intents.is_empty());
    assert_eq!(fight.surface.begin_count, 0);

    assert!(fight.approach(true));
    assert_eq!(fight.surface.movement_intents.len(), 1);
    assert!(fight.surface.movement_intents[0].forward);
    assert!(fight.status().contains("Approaching"));

    fight.surface.targets = vec![target(10, "Drudge", 4.0, 0.0)];
    fight.tick(0.05);

    assert_eq!(fight.surface.last_begin_target, 10);
    assert!(!fight.approach(true));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheApproachPicksItsOwnTargetAtTheApproachRange
#[test]
fn the_approach_picks_its_own_target_at_the_approach_range() {
    let mut surface = swinging(vec![
        target(10, "Drudge", 12.0, 0.0),
        target(20, "Olthoi Soldier", 18.0, 30.0),
    ]);
    surface.navigation_snapshot = navigation_at(0.0);
    surface
        .navigation_objects
        .insert(20, nav_object(20, "Olthoi Soldier", 0.1, 0.0));
    let mut settings = approach_settings(5.0, 20.0);
    settings.selection_method = mosstank_settings::model::TargetSelectionMethod::RANGE;
    settings.rules.insert(0, rule("name#^Olthoi", 4));
    let mut fight = Fight::new(surface, settings);

    fight.toggle();
    fight.tick(0.05);

    assert!(fight.approach(true));
    assert!(fight.status().contains("Olthoi Soldier"));
}

/// `ApproachRig`: started, the monster twelve metres off at a place.
fn approach_rig(heading: f32, east_west: f64, north_south: f64) -> Fight {
    let mut fight = Fight::new(
        walking(east_west, north_south, heading),
        approach_settings(5.0, 20.0),
    );
    fight.toggle();
    fight
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ApproachHoldsATurnAndKeepsRunningInsideTheFarRelaxation
#[test]
fn approach_holds_a_turn_and_keeps_running_inside_the_far_relaxation() {
    let mut fight = approach_rig(0.0, 0.0839, 0.1);

    fight.approach(true);

    assert_eq!(fight.surface.movement_intents.len(), 1);
    let intent = &fight.surface.movement_intents[0];
    assert!(intent.forward);
    assert!(intent.run);
    assert!(intent.turn_left ^ intent.turn_right);
    assert!(fight.surface.faced_headings.is_empty());
    assert_eq!(fight.surface.clear_movement_count, 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheFarRelaxationIsFortyFiveDegrees
#[test]
fn the_far_relaxation_is_forty_five_degrees() {
    for (east_west, expect_forward) in [(0.0966, true), (0.1036, false)] {
        let mut fight = approach_rig(0.0, east_west, 0.1);

        fight.approach(true);

        assert_eq!(fight.surface.movement_intents.len(), 1);
        let intent = &fight.surface.movement_intents[0];
        assert!(intent.turn_right);
        assert_eq!(intent.forward, expect_forward);
        assert!(intent.run);
        assert!(fight.surface.faced_headings.is_empty());
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ApproachOnlyFacesTheTargetWhileSomebodyIsTyping
#[test]
fn approach_only_faces_the_target_while_somebody_is_typing() {
    let mut fight = approach_rig(0.0, 0.0839, 0.1);
    fight.surface.chat_input_active = true;

    fight.approach(true);

    assert!(fight.surface.movement_intents.is_empty());
    assert_eq!(fight.surface.faced_headings.len(), 1);
    let faced = fight.surface.faced_headings[0];
    assert!((39.0..=41.0).contains(&faced), "{faced}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ApproachRunsForwardInsideTheFourDegreeBandWithoutFacingAgain
#[test]
fn approach_runs_forward_inside_the_four_degree_band_without_facing_again() {
    let mut fight = approach_rig(0.0, 0.003, 0.1);

    fight.approach(true);

    assert_eq!(fight.surface.movement_intents.len(), 1);
    let intent = &fight.surface.movement_intents[0];
    assert!(intent.forward);
    assert!(intent.run);
    assert!(!intent.turn_left);
    assert!(!intent.turn_right);
    assert!(fight.surface.faced_headings.is_empty());
    assert!(fight.status().contains("Approaching"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheArmedWalkStepsOnTheMoverInterval
#[test]
fn the_armed_walk_steps_on_the_mover_interval() {
    let mut fight = approach_rig(0.0, 0.003, 0.1);

    assert!(fight.claim_approach(true));
    assert_eq!(fight.surface.movement_intents.len(), 1);

    fight.step_mover(MOVER_INTERVAL_SECONDS / 2.0);
    assert_eq!(fight.surface.movement_intents.len(), 1);

    fight.step_mover(MOVER_INTERVAL_SECONDS);
    assert_eq!(fight.surface.movement_intents.len(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ADisarmedWalkDoesNotSteer
#[test]
fn a_disarmed_walk_does_not_steer() {
    let mut fight = approach_rig(0.0, 0.003, 0.1);

    fight.step_mover(1.0);

    assert!(fight.surface.movement_intents.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheWalkRepicksItsTargetOnEveryMoverFrame
#[test]
fn the_walk_repicks_its_target_on_every_mover_frame() {
    let mut fight = approach_rig(0.0, 0.003, 0.1);

    assert!(fight.claim_approach(true));
    assert!(fight.status().contains("Drudge"));

    fight.surface.targets = vec![target(20, "Mosswart", 9.0, 0.0)];
    fight
        .surface
        .navigation_objects
        .insert(20, nav_object(20, "Mosswart", 0.003, 0.1));

    fight.step_mover(MOVER_INTERVAL_SECONDS);

    assert!(fight.status().contains("Mosswart"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: TheWalkStopsAtTheAttacksOwnMaximumRange
#[test]
fn the_walk_stops_at_the_attacks_own_maximum_range() {
    let mut fight = Fight::new(walking(0.003, 0.1, 0.0), approach_settings(8.0, 25.0));
    fight.toggle();

    assert!(fight.claim_approach(true));
    let cleared_before = fight.surface.clear_movement_count;

    // Still outside melee range: the walk carries on.
    fight.surface.targets = vec![target(10, "Drudge", 8.5, 0.0)];
    assert!(fight.claim_approach(true));

    // One step inside it: the keys go down.
    fight.surface.targets = vec![target(10, "Drudge", 7.5, 0.0)];
    assert!(!fight.claim_approach(true));
    assert_eq!(fight.surface.clear_movement_count, cleared_before + 1);
}

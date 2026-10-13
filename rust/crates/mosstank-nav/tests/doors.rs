//! Opening the doors on the route (`NavigationTests`, the door half).

mod common;

use common::*;
use mosstank_automation::locks::{ActionLockKind, ActionLockTable, SharedActionLocks};
use mosstank_formats::route::{RouteMode, RouteWaypointType};
use mosstank_meta::scheduler::{
    MACRO_RULE_TABLE, MacroPassContext, MacroRule, MacroRuleEntry, MacroRuleSlot,
};
use mosstank_nav::OpenDoorRule;

/// A circular route a point a map unit east, with a closed door 2.4 m
/// along it (`DoorFixture`), and the shared lock table bound.
fn door_fixture(has_lock_state: bool) -> (Rig, SharedActionLocks) {
    let mut automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 90.0)));
    automation.world_objects.push(door(
        55,
        "Dungeon Door",
        position(0.01, 0.0),
        has_lock_state,
    ));
    let mut rig = rig(
        automation,
        RouteMode::Circular,
        vec![waypoint(RouteWaypointType::Point, position(1.0, 0.0))],
    );
    rig.settings.open_doors = true;
    let locks = ActionLockTable::shared();
    rig.bind_locks(&locks);
    (rig, locks)
}

/// The same route and door, without a lock table.
fn door_route() -> Rig {
    let mut automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 90.0)));
    automation
        .world_objects
        .push(door(55, "Dungeon Door", position(0.01, 0.0), true));
    let mut rig = rig(
        automation,
        RouteMode::Circular,
        vec![waypoint(RouteWaypointType::Point, position(1.0, 0.0))],
    );
    rig.settings.open_doors = true;
    rig
}

fn pass() -> MacroPassContext {
    MacroPassContext::new(0.05, true)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: ClosedDoorPausesRouteAndUsesCanonicalItemAction
#[test]
fn closed_door_pauses_route_and_uses_canonical_item_action() {
    let mut rig = door_route();
    // The door takes its own turn, ahead of the route.
    let mut rule = OpenDoorRule::new();

    assert!(rule.valid_now(&mut rig, pass()));
    assert_eq!(rig.automation.used_objects, [55]);
    assert!(rig.automation.intents.is_empty());

    rig.automation.world_objects[0].is_open = true;
    assert!(!rule.valid_now(&mut rig, pass()));
    assert!(rig.tick(0.05, true));
    assert_eq!(rig.automation.intents.len(), 1);
    assert!(rig.automation.intents[0].forward);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: TheNavigationClockMovesOnceAFrameHoweverManyTurnsAreTakenInIt
#[test]
fn the_navigation_clock_moves_once_a_frame_however_many_turns_are_taken_in_it() {
    let mut automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 0.0)));
    // The typing branch is the one that measures against the clock.
    automation.chat_input_active = true;
    let mut rig = rig(
        automation,
        RouteMode::Circular,
        vec![waypoint(RouteWaypointType::Point, position(1.0, 0.0))],
    );
    rig.settings.open_doors = true;

    let pass_frame = |rig: &mut Rig| {
        rig.controller.advance_clock(0.293);
        assert!(!rig.door(0.293, true));
        assert!(rig.tick(0.293, true));
    };

    // The first frame faces and stamps the clock. Two more frames are
    // 0.586 s after that stamp: inside the 0.7 s re-face interval, but past
    // it if each turn moved the clock itself. The fourth is past it either
    // way.
    pass_frame(&mut rig);
    assert_eq!(rig.automation.faced_headings, [90.0]);
    pass_frame(&mut rig);
    pass_frame(&mut rig);
    assert_eq!(rig.automation.faced_headings.len(), 1);
    pass_frame(&mut rig);
    assert_eq!(rig.automation.faced_headings.len(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: TheDoorRuleKeepsItsDoorAcrossAPassItDoesNotWin
#[test]
fn the_door_rule_keeps_its_door_across_a_pass_it_does_not_win() {
    let mut rig = door_route();

    // The door rule claims a pass and sends the first use.
    assert!(rig.door(0.05, true));
    assert_eq!(rig.automation.used_objects, [55]);

    // Something ahead of it takes the next pass.
    assert!(!rig.door(0.05, false));

    // The retry interval has not elapsed, so the pass it wins back works on
    // the same door and does not re-send.
    assert!(rig.door(0.05, true));
    assert_eq!(rig.automation.used_objects, [55]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: TheDoorIsNotUsedAgainInsideItsRetryInterval
#[test]
fn the_door_is_not_used_again_inside_its_retry_interval() {
    let mut rig = door_route();

    for _ in 0..4 {
        assert!(rig.door(0.293, true));
    }

    assert_eq!(rig.automation.used_objects, [55]);

    // Past the interval, it tries again.
    assert!(rig.door(1.5, true));
    assert_eq!(rig.automation.used_objects, [55, 55]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: TheDoorRuleTakesItsTurnAheadOfLootingAndAttacking
#[test]
fn the_door_rule_takes_its_turn_ahead_of_looting_and_attacking() {
    let slot_position = |slot: MacroRuleSlot| {
        MACRO_RULE_TABLE
            .iter()
            .position(|entry| matches!(entry, MacroRuleEntry::Rule(found) if *found == slot))
    };
    let door = slot_position(MacroRuleSlot::OpenDoor).expect("the door has a slot");
    assert!(
        door < slot_position(MacroRuleSlot::LootCorpsePriority).unwrap(),
        "the door must be claimed before priority looting."
    );
    assert!(
        door < slot_position(MacroRuleSlot::NavigateCorpsePriority).unwrap(),
        "the door must be claimed before the priority corpse approach."
    );
    assert!(
        door < slot_position(MacroRuleSlot::Attack).unwrap(),
        "the door must be claimed before attacking."
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: TheDoorRuleStandsDownWhileALockIsHeld
#[test]
fn the_door_rule_stands_down_while_a_lock_is_held() {
    let mut rig = door_route();
    rig.door_locked = true;
    let mut rule = OpenDoorRule::new();

    // Held: the rule declines and does not so much as look at the door.
    assert!(rule.is_locked(&mut rig));
    assert!(!rule.valid_now(&mut rig, pass()));
    assert_eq!(
        rule.decline_reason(&mut rig).as_deref(),
        Some("another rule holds a lock this one waits on")
    );
    assert!(rig.automation.used_objects.is_empty());
    assert_eq!(rig.door_armed, 0);

    // Released: it claims the pass, uses the door, and takes its own locks.
    rig.door_locked = false;
    assert!(!rule.is_locked(&mut rig));
    assert!(rule.valid_now(&mut rig, pass()));
    assert_eq!(rig.automation.used_objects, [55]);
    assert_eq!(rig.door_armed, 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: TheDoorRuleGivesItsOwnStableDeclineReason
#[test]
fn the_door_rule_gives_its_own_stable_decline_reason() {
    let mut rig = rig(
        FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 90.0))),
        RouteMode::Circular,
        vec![waypoint(RouteWaypointType::Point, position(1.0, 0.0))],
    );
    rig.settings.open_doors = true;
    let mut rule = OpenDoorRule::new();

    assert!(!rule.valid_now(&mut rig, pass()));
    let first = rule.decline_reason(&mut rig).expect("a reason");
    assert_eq!(first, "no door in reach");

    // The character has walked on; the reason has not moved with it.
    rig.automation.navigation_snapshot = snapshot(position_facing(0.05, 0.0, 90.0));
    assert!(!rule.valid_now(&mut rig, pass()));
    assert_eq!(
        rule.decline_reason(&mut rig).as_deref(),
        Some(first.as_str())
    );
    assert!(!first.contains("Waypoint"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: TheDoorRuleReportsItsOwnWorkOnThePassItWins
#[test]
fn the_door_rule_reports_its_own_work_on_the_pass_it_wins() {
    let (mut rig, _) = door_fixture(true);
    let mut rule = OpenDoorRule::new();

    assert!(rule.valid_now(&mut rig, pass()));
    assert_eq!(
        rule.running_detail(&mut rig).as_deref(),
        Some("opening door: Dungeon Door")
    );
    assert!(!rig.status().to_lowercase().contains("door"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: TheDoorRuleNamesTheDoorItIsWaitingOn
#[test]
fn the_door_rule_names_the_door_it_is_waiting_on() {
    let (mut rig, _) = door_fixture(false);
    let mut rule = OpenDoorRule::new();

    assert!(!rule.valid_now(&mut rig, pass()));
    let reason = rule.decline_reason(&mut rig);
    assert_eq!(reason.as_deref(), Some("identifying door: Dungeon Door"));
    assert_ne!(Some(rig.status()), reason);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: DoorAppraisalUsesWorldObjectsAndTheReplyAllowsOpening
#[test]
fn door_appraisal_uses_world_objects_and_the_reply_allows_opening() {
    let (mut rig, _) = door_fixture(false);
    rig.automation.world_appraisal_available = true;
    assert!(!rig.door(0.05, true));
    assert_eq!(rig.automation.identified_world_objects, [55]);
    rig.automation.world_objects[0].has_lock_state = true;
    rig.automation.world_objects[0].is_locked = false;
    assert!(rig.door(0.6, true));
    assert!(rig.automation.used_objects.contains(&55));
    rig.automation.world_objects[0].is_open = true;
    assert!(!rig.door(0.6, true));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: DoorsOutsideTheCurrentRouteAreNotUsedOrIdentified
#[test]
fn doors_outside_the_current_route_are_not_used_or_identified() {
    for (x, y, z) in [
        (0.01, 0.01, 0.0),
        (-0.01, 0.0, 0.0),
        (1.01, 0.0, 0.0),
        (0.01, 0.0, 0.02),
    ] {
        let (mut rig, locks) = door_fixture(false);
        let moved = &mut rig.automation.world_objects[0].position;
        moved.east_west = x;
        moved.north_south = y;
        moved.elevation = z;

        assert!(!rig.door(0.05, true));
        assert!(rig.automation.used_objects.is_empty());
        assert!(rig.automation.identified_world_objects.is_empty());
        assert!(!locks.borrow_mut().is_locked(ActionLockKind::Navigation));
        assert_eq!(rig.controller.door_status(), "no door in reach");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: NearbyOffRouteDoorDoesNotHideDoorOnRoute
#[test]
fn nearby_off_route_door_does_not_hide_door_on_route() {
    let (mut rig, _) = door_fixture(true);
    let mut off_route = rig.automation.world_objects[0].clone();
    off_route.object_id = 56;
    off_route.position = position(0.0, 0.005);
    rig.automation.world_objects.push(off_route);
    assert!(rig.door(0.05, true));
    assert_eq!(rig.automation.used_objects, [55]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: IdleOrPausedRouteDoesNotOpenNearbyDoors
#[test]
fn idle_or_paused_route_does_not_open_nearby_doors() {
    for pause in [false, true] {
        let mut automation = FakeAutomation::at(snapshot(position(0.0, 0.0)));
        automation
            .world_objects
            .push(door(55, "Door", position(0.01, 0.0), true));
        let mut rig = rig(automation, RouteMode::Circular, Vec::new());
        rig.settings.open_doors = true;
        if pause {
            rig.settings
                .waypoints
                .push(waypoint(RouteWaypointType::Pause, position(1.0, 0.0)));
        }
        assert!(!rig.door(0.05, true));
        assert!(rig.automation.used_objects.is_empty());
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: ActiveDoorIsDiscardedWhenPlayerPassesIt
#[test]
fn active_door_is_discarded_when_player_passes_it() {
    let (mut rig, locks) = door_fixture(true);
    assert!(rig.door(0.05, true));
    locks.borrow_mut().advance(6.0);
    rig.automation.navigation_snapshot = snapshot(position(0.02, 0.0));
    assert!(!rig.door(0.05, true));
    assert_eq!(rig.automation.used_objects, [55]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: ADoorUseHoldsTheItemNavigationAndDoorSlots
#[test]
fn a_door_use_holds_the_item_navigation_and_door_slots() {
    let (mut rig, locks) = door_fixture(true);

    assert!(rig.door(0.05, true));
    assert_eq!(rig.automation.used_objects, [55]);
    let mut locks = locks.borrow_mut();
    assert!(locks.is_locked(ActionLockKind::ItemUse));
    assert!(locks.is_locked(ActionLockKind::Navigation));
    assert!(locks.is_locked(ActionLockKind::DoorOpening));

    locks.advance(0.6);
    assert!(!locks.is_locked(ActionLockKind::ItemUse));
    assert!(locks.is_locked(ActionLockKind::Navigation));
    assert!(locks.is_locked(ActionLockKind::DoorOpening));

    locks.advance(4.5);
    assert!(!locks.is_locked(ActionLockKind::Navigation));
    assert!(!locks.is_locked(ActionLockKind::DoorOpening));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AnUnidentifiedDoorInReachHoldsNavigationAndDeclines
#[test]
fn an_unidentified_door_in_reach_holds_navigation_and_declines() {
    let (mut rig, locks) = door_fixture(false);

    assert!(!rig.door(0.05, true));
    assert!(rig.automation.used_objects.is_empty());
    let mut locks = locks.borrow_mut();
    assert!(locks.is_locked(ActionLockKind::Navigation));
    assert!(!locks.is_locked(ActionLockKind::ItemUse));
    assert!(!locks.is_locked(ActionLockKind::DoorOpening));
    locks.advance(0.6);
    assert!(!locks.is_locked(ActionLockKind::Navigation));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AnOpenedDoorReleasesTheItemAndDoorSlotsEarly
#[test]
fn an_opened_door_releases_the_item_and_door_slots_early() {
    let (mut rig, locks) = door_fixture(true);
    assert!(rig.door(0.05, true));

    rig.automation.world_objects[0].is_open = true;
    locks.borrow_mut().advance(0.1);
    assert!(!rig.door(0.05, true));
    let mut locks = locks.borrow_mut();
    assert!(!locks.is_locked(ActionLockKind::ItemUse));
    assert!(!locks.is_locked(ActionLockKind::DoorOpening));
    assert!(locks.is_locked(ActionLockKind::Navigation));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: TheDoorRuleLosingItsTurnLeavesTheRouteMoverArmed
#[test]
fn the_door_rule_losing_its_turn_leaves_the_route_mover_armed() {
    let mut rig = rig(
        FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 90.0))),
        RouteMode::Circular,
        vec![waypoint(RouteWaypointType::Point, position(10.0, 0.0))],
    );
    rig.settings.open_doors = true;
    let mut door = OpenDoorRule::new();
    door.set_running(&mut rig, true);

    // Phase one of the pass: the route rule claims and arms the mover.
    assert!(rig.claim(true));
    assert!(rig.automation.intents.iter().any(|intent| intent.forward));
    // Phase two: the door rule, which did not win, is marked not running.
    door.set_running(&mut rig, false);

    // The intent the route rule set still stands, and the next frame still
    // belongs to an armed mover.
    assert_eq!(rig.automation.clear_count, 0);
    rig.step(0.05, true);
    assert_eq!(rig.automation.clear_count, 0);
}

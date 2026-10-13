//! The waypoint actions: pause, chat, portal, NPC, vendor, jump
//! (`NavigationTests`).

mod common;

use ac_plugin_abi::items::ItemUseCompletion;
use ac_plugin_abi::navigation::{MoveChannel, MoveDirection, MovePace, MoveUnit};
use ac_plugin_abi::types::ChatLine;
use common::*;
use mosstank_formats::route::{RouteJumpDirection, RouteMode, RouteWaypointType};
use mosstank_nav::jump_power;

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: PauseAndChatActionsObserveOfficialInitialDelay
#[test]
fn pause_and_chat_actions_observe_official_initial_delay() {
    let mut pause = waypoint(RouteWaypointType::Pause, position(0.0, 0.0));
    pause.duration_milliseconds = 100;
    let mut chat = waypoint(RouteWaypointType::ChatCommand, position(0.0, 0.0));
    chat.text = "/say route".to_owned();
    let mut rig = rig(
        FakeAutomation::at(snapshot(position(0.0, 0.0))),
        RouteMode::Once,
        vec![pause, chat],
    );

    assert!(rig.tick(0.05, true));
    assert!(rig.tick(0.05, true));
    // The pause is done and the chat waypoint is the current one.
    assert_eq!(rig.index(), 1);
    assert!(rig.tick(0.19, true));
    assert!(rig.automation.submitted_chat.is_empty());
    assert!(rig.tick(0.01, true));
    assert_eq!(rig.automation.submitted_chat, ["/say route"]);
    assert!(!rig.tick(0.01, true));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: PortalWaypointWaitsForPortalExitRatherThanUseDispatch
#[test]
fn portal_waypoint_waits_for_portal_exit_rather_than_use_dispatch() {
    let mut automation = FakeAutomation::at(snapshot(position(0.0, 0.0)));
    automation.item_completion = ItemUseCompletion {
        revision: 4,
        source_id: 10,
        target_id: 0,
        weenie_error: 0,
    };
    let mut portal = waypoint(RouteWaypointType::Portal, position(0.0, 0.0));
    portal.object_id = 77;
    portal.object_name = "Town Crier".to_owned();
    let mut rig = rig(automation, RouteMode::Once, vec![portal]);

    assert!(rig.tick(0.05, true));
    assert_eq!(rig.automation.used_objects, [77]);
    assert!(rig.tick(0.05, true));
    rig.automation.item_completion = ItemUseCompletion {
        revision: 5,
        source_id: 77,
        target_id: 0,
        weenie_error: 0,
    };
    assert!(rig.tick(0.05, true));
    rig.automation.navigation_snapshot.is_portal_space = true;
    assert!(rig.tick(0.05, true));
    rig.automation.navigation_snapshot = snapshot(position(0.1, 0.0));
    assert!(rig.tick(0.05, true));
    assert!(!rig.tick(0.05, true));
}

/// A route that uses a portal, with the portal still standing 1 m east of
/// the start and a point beyond the destination.
fn portal_route(kind: RouteWaypointType) -> Rig {
    let mut automation = FakeAutomation::at(snapshot(position(0.0, 0.0)));
    automation
        .objects
        .insert(77, object(77, "Portal", position(1.0 / 240.0, 0.0)));
    let mut portal = waypoint(kind, position(0.0, 0.0));
    portal.object_id = 77;
    portal.object_name = "Portal".to_owned();
    portal.reference_position = position(1.0 / 240.0, 0.0);
    rig(
        automation,
        RouteMode::Once,
        vec![
            portal,
            waypoint(RouteWaypointType::Point, position(5.0, 0.0)),
        ],
    )
}

fn pass_through_portal_space(rig: &mut Rig, landing: ac_plugin_abi::world::Position) {
    rig.automation.navigation_snapshot.is_portal_space = true;
    assert!(rig.tick(0.05, true));
    rig.automation.navigation_snapshot = snapshot(landing);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: APortalWaypointIsDoneOnTheFirstTickAfterArrival
#[test]
fn a_portal_waypoint_is_done_on_the_first_tick_after_arrival() {
    for by_name in [false, true] {
        let mut rig = portal_route(if by_name {
            RouteWaypointType::PortalByName
        } else {
            RouteWaypointType::Portal
        });

        assert!(rig.tick(0.05, true));
        assert_eq!(rig.automation.used_objects, [77]);

        pass_through_portal_space(&mut rig, position(4.0, 0.0));
        rig.automation.intents.clear();
        assert!(rig.tick(0.05, true));

        assert_eq!(rig.index(), 1);
        assert!(!rig.automation.intents.iter().any(|intent| intent.forward));
        assert_eq!(rig.automation.used_objects, [77]);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: APortalByNameWaypointIsDoneOnArrivalWhenItsPortalIsGone
#[test]
fn a_portal_by_name_waypoint_is_done_on_arrival_when_its_portal_is_gone() {
    let mut rig = portal_route(RouteWaypointType::PortalByName);

    assert!(rig.tick(0.05, true));
    pass_through_portal_space(&mut rig, position(4.0, 0.0));
    rig.automation.objects.clear();

    assert!(rig.tick(0.05, true));

    assert_eq!(rig.index(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: APortalByNameExitNearItsOriginRetriesTheUse
#[test]
fn a_portal_by_name_exit_near_its_origin_retries_the_use() {
    let mut rig = portal_route(RouteWaypointType::PortalByName);

    assert!(rig.tick(0.05, true));
    assert_eq!(rig.automation.used_objects, [77]);

    // Out 10 m west of the start: 11 m from the portal, inside 15 m of the
    // origin.
    pass_through_portal_space(&mut rig, position(-10.0 / 240.0, 0.0));
    assert!(rig.tick(0.05, true));
    assert_eq!(rig.index(), 0);

    rig.automation.navigation_snapshot = snapshot(position(0.0, 0.0));
    assert!(rig.tick(0.05, true));

    assert_eq!(rig.index(), 0);
    assert_eq!(rig.automation.used_objects, [77, 77]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: UseNpcRepeatsUntilTheNpcRespondsInChat
#[test]
fn use_npc_repeats_until_the_npc_responds_in_chat() {
    let mut automation = FakeAutomation::at(snapshot(position(0.0, 0.0)));
    automation.found_object = Some(object(91, "Town Crier", position(0.01, 0.0)));
    let mut npc = waypoint(RouteWaypointType::UseNpc, position(0.0, 0.0));
    npc.object_name = "Town Crier".to_owned();
    let mut rig = rig(automation, RouteMode::Once, vec![npc]);

    assert!(rig.tick(0.05, true));
    rig.automation.chat_messages.push(ChatLine {
        sequence: 1,
        sender_id: 91,
        kind: 3,
        sender: "Town Crier".to_owned(),
        text: "Welcome.".to_owned(),
        log_text_type: 3,
        display_text: "Town Crier tells you, \"Welcome.\"".to_owned(),
        ..ChatLine::default()
    });
    assert!(rig.tick(0.05, true));
    assert!(!rig.tick(0.05, true));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: NamedNpcWaypointReacquiresChangedObjectId
#[test]
fn named_npc_waypoint_reacquires_changed_object_id() {
    let mut automation = FakeAutomation::at(snapshot(position(0.0, 0.0)));
    automation.found_object = Some(object(91, "Town Crier", position(0.01, 0.0)));
    let mut npc = waypoint(RouteWaypointType::UseNpc, position(0.0, 0.0));
    npc.object_id = 77;
    npc.object_name = "Town Crier".to_owned();
    let mut rig = rig(automation, RouteMode::Once, vec![npc]);

    assert!(rig.tick(0.05, true));

    assert_eq!(rig.settings.waypoints[0].object_id, 91);
    assert_eq!(rig.automation.used_objects, [91]);
    assert_eq!(rig.automation.find_name.as_deref(), Some("Town Crier"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: UseNpcCompletesOnlyOnItsOwnChannels
#[test]
fn use_npc_completes_only_on_its_own_channels() {
    // log-text type, sender object id, sender, text, completes
    for (log_text_type, sender_id, sender, text, completes) in [
        (3_u32, 500_u32, "Aun Tanua", "Greetings.", true),
        (0, 0, "", "Aun Tanua gives you a Token.", true),
        (0, 500, "Aun Tanua", "Greetings.", false),
        (3, 0, "", "Aun Tanua gives you a Token.", false),
        (3, 700, "Someone Else", "Greetings.", false),
        (8, 0, "", "Aun Tanua gives you a Token.", false),
        (3, 0x5000_0003, "Aun Tanua", "Greetings.", false),
    ] {
        let mut automation = FakeAutomation::at(snapshot(position(0.0, 0.0)));
        automation.found_object = Some(object(500, "Aun Tanua", position(0.001, 0.0)));
        let mut npc = waypoint(RouteWaypointType::UseNpc, position(0.0, 0.0));
        npc.object_name = "Aun Tanua".to_owned();
        let mut rig = rig(
            automation,
            RouteMode::Circular,
            vec![npc, waypoint(RouteWaypointType::Point, position(1.0, 0.0))],
        );

        assert!(rig.tick(0.05, true));
        rig.automation.chat_messages.push(ChatLine {
            sequence: 1,
            sender_id,
            // A tell's kind and its log-text type share the number three;
            // the kind is what the surface reports for that shape, and it is
            // not what decides the answer.
            kind: if sender_id != 0 { 3 } else { 4 },
            sender: sender.to_owned(),
            text: text.to_owned(),
            log_text_type,
            // The client words the whole line as the chat window prints it.
            display_text: if sender_id == 0 {
                text.to_owned()
            } else if (0x5000_0001..=0x6FFF_FFFF).contains(&sender_id) {
                format!(
                    "<Tell:IIDString:{sender_id}:{sender}>{sender}<\\Tell> tells you, \"{text}\""
                )
            } else {
                format!("{sender} tells you, \"{text}\"")
            },
            ..ChatLine::default()
        });

        assert!(rig.tick(0.05, true));

        assert_eq!(
            rig.index(),
            i32::from(completes),
            "{log_text_type} {sender_id} {text}"
        );
    }
}

fn vendor_route(with_object: bool, open_vendor: u32) -> Rig {
    let mut automation = FakeAutomation::at(snapshot(position(0.0, 0.0)));
    automation.active_vendor_object_id = open_vendor;
    if with_object {
        automation
            .objects
            .insert(404, object(404, "Shopkeeper", position(0.001, 0.0)));
    }
    let mut vendor = waypoint(RouteWaypointType::OpenVendor, position(0.0, 0.0));
    vendor.object_id = 404;
    vendor.object_name = "Shopkeeper".to_owned();
    rig(
        automation,
        RouteMode::Circular,
        vec![
            vendor,
            waypoint(RouteWaypointType::Point, position(1.0, 0.0)),
        ],
    )
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AVendorWaypointHoldsUntilTheWindowOpens
#[test]
fn a_vendor_waypoint_holds_until_the_window_opens() {
    let mut rig = vendor_route(true, 0);

    assert!(rig.tick(0.05, true));

    // The use went out, and the route stayed on the waypoint so the walk the
    // client started can finish.
    assert_eq!(rig.automation.used_objects, [404]);
    assert_eq!(rig.index(), 0);

    // Under two seconds: no second use.
    assert!(rig.tick(1.0, true));
    assert_eq!(rig.automation.used_objects, [404]);
    assert_eq!(rig.index(), 0);

    // Past two seconds with no window: the use is sent again.
    assert!(rig.tick(1.5, true));
    assert_eq!(rig.automation.used_objects, [404, 404]);
    assert_eq!(rig.index(), 0);

    // The window opens: the waypoint is done.
    rig.automation.active_vendor_object_id = 404;
    assert!(rig.tick(0.05, true));
    assert_eq!(rig.index(), 1);
    assert!(rig.automation.posted_system_messages.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AVendorThatNeverOpensSaysSoOnceAndContinues
#[test]
fn a_vendor_that_never_opens_says_so_once_and_continues() {
    let mut rig = vendor_route(true, 0);

    for _ in 0..6 {
        assert!(rig.tick(5.0, true));
    }

    assert_eq!(rig.index(), 1);
    assert_eq!(
        rig.automation.posted_system_messages,
        ["[MossTank] Vendor 'Shopkeeper' did not open; continuing route."]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AVendorWaypointCompletesWhenItsWindowIsAlreadyOpen
#[test]
fn a_vendor_waypoint_completes_when_its_window_is_already_open() {
    let mut rig = vendor_route(true, 404);

    assert!(rig.tick(0.05, true));

    assert!(rig.automation.used_objects.is_empty());
    assert_eq!(rig.index(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AMissingVendorSaysSoOnceAndHolds
#[test]
fn a_missing_vendor_says_so_once_and_holds() {
    let mut rig = vendor_route(false, 0);

    for _ in 0..5 {
        assert!(rig.tick(0.05, true));
    }

    assert!(rig.automation.used_objects.is_empty());
    assert_eq!(rig.index(), 0);
    assert_eq!(rig.automation.posted_system_messages.len(), 1);
    assert!(rig.automation.posted_system_messages[0].contains("not found"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: ThePortalUseDistanceDecidesBetweenWalkingAndPulling
#[test]
fn the_portal_use_distance_decides_between_walking_and_pulling() {
    for (portal_metres, uses) in [(10.0, false), (2.0, true)] {
        let mut automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 90.0)));
        automation.objects.insert(
            77,
            object(77, "Portal", position(portal_metres / 240.0, 0.0)),
        );
        let mut portal = waypoint(RouteWaypointType::Portal, position(0.0, 0.0));
        portal.object_id = 77;
        portal.object_name = "Portal".to_owned();
        let mut rig = rig(automation, RouteMode::Once, vec![portal]);
        rig.settings.portal_use_distance_meters = 4.0;

        assert!(rig.tick(0.05, true));
        assert_eq!(!rig.automation.used_objects.is_empty(), uses);
        assert_eq!(!rig.automation.intents.is_empty(), !uses);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AWiderPortalUseDistancePullsTheSamePortalFromFurtherOut
#[test]
fn a_wider_portal_use_distance_pulls_the_same_portal_from_further_out() {
    let mut automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 90.0)));
    automation
        .objects
        .insert(78, object(78, "Portal", position(10.0 / 240.0, 0.0)));
    let mut portal = waypoint(RouteWaypointType::Portal, position(0.0, 0.0));
    portal.object_id = 78;
    portal.object_name = "Portal".to_owned();
    let mut rig = rig(automation, RouteMode::Once, vec![portal]);
    rig.settings.portal_use_distance_meters = 15.0;

    assert!(rig.tick(0.05, true));
    assert_eq!(rig.automation.used_objects, [78]);
    assert!(rig.automation.intents.is_empty());
}

fn jump_route(heading: f32, jump_heading: f64, charge_milliseconds: i32) -> Rig {
    let mut jump = waypoint(RouteWaypointType::Jump, position(0.0, 0.0));
    jump.jump_heading_degrees = jump_heading;
    jump.jump_charge_milliseconds = charge_milliseconds;
    rig(
        FakeAutomation::at(snapshot(position_facing(0.0, 0.0, heading))),
        RouteMode::Once,
        vec![jump],
    )
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: JumpAlignsBeforeChargingAndWaitsForLanding
#[test]
fn jump_aligns_before_charging_and_waits_for_landing() {
    let mut rig = jump_route(0.0, 90.0, 100);

    assert!(rig.tick(0.05, true));
    assert!(rig.automation.intents.is_empty());
    assert_eq!(rig.automation.faced_headings, [90.0]);

    rig.automation.navigation_snapshot = snapshot(position_facing(0.0, 0.0, 90.0));
    assert!(rig.tick(0.05, true));
    assert_eq!(rig.automation.jumps, [0.1]);
    assert!(rig.tick(0.05, true));
    assert_eq!(rig.automation.jumps.len(), 1);

    rig.automation.navigation_snapshot = snapshot_airborne(position_facing(0.0, 0.0, 90.0), true);
    assert!(rig.tick(0.05, true));
    rig.automation.navigation_snapshot = snapshot(position_facing(0.0, 0.0, 90.0));
    assert!(rig.tick(0.25, true));
    assert!(!rig.tick(0.01, true));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AJumpWaypointHoldsThePassFromItsTurnUntilItLands
#[test]
fn a_jump_waypoint_holds_the_pass_from_its_turn_until_it_lands() {
    let mut rig = jump_route(0.0, 90.0, 100);
    assert!(!rig.controller.holds_pass());

    assert!(rig.claim(true));
    assert!(rig.controller.holds_pass());

    // Only frames from here: the pass is held.
    rig.automation.navigation_snapshot = snapshot(position_facing(0.0, 0.0, 90.0));
    rig.step(0.05, true);
    rig.step(0.1, true);
    assert_eq!(rig.automation.jumps, [0.1]);
    assert!(rig.controller.holds_pass());

    rig.automation.navigation_snapshot = snapshot_airborne(position_facing(0.0, 0.0, 90.0), true);
    rig.step(0.05, true);
    assert!(rig.controller.holds_pass());
    rig.automation.navigation_snapshot = snapshot(position_facing(0.0, 0.0, 90.0));
    rig.step(0.3, true);

    assert!(!rig.controller.holds_pass());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AJumpThatLostItsTurnHoldsNothing
#[test]
fn a_jump_that_lost_its_turn_holds_nothing() {
    let mut rig = jump_route(0.0, 90.0, 1000);

    assert!(rig.claim(true));
    rig.step(0.05, false);

    assert!(!rig.controller.holds_pass());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: ARouteJumpsShiftFlagWalksAndItsAbsenceRuns
#[test]
fn a_route_jumps_shift_flag_walks_and_its_absence_runs() {
    for (hold_shift, pace) in [(true, MovePace::Walk), (false, MovePace::Run)] {
        let mut rig = jump_route(90.0, 90.0, 300);
        rig.settings.waypoints[0].jump_hold_shift = hold_shift;

        assert!(rig.tick(0.05, true));

        assert_eq!(
            rig.automation.moves,
            [(MoveDirection::Forward, pace, 0.0, MoveUnit::MetersOrDegrees)]
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: JumpChargesBackwardWhenTheWaypointAsksForIt
#[test]
fn jump_charges_backward_when_the_waypoint_asks_for_it() {
    let mut rig = jump_route(90.0, 90.0, 100);
    rig.settings.waypoints[0].jump_direction = RouteJumpDirection::Backward;

    assert!(rig.tick(0.05, true));

    assert_eq!(rig.automation.jumps, [0.1]);
    assert_eq!(
        rig.automation.moves,
        [(
            MoveDirection::Backward,
            MovePace::Run,
            0.0,
            MoveUnit::MetersOrDegrees
        )]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: ARouteJumpLeavesWithExactlyThePowerItsChargeTimeAsks
#[test]
fn a_route_jump_leaves_with_exactly_the_power_its_charge_time_asks() {
    let mut rig = jump_route(90.0, 90.0, 450);
    rig.settings.waypoints[0].jump_hold_shift = true;

    assert!(rig.tick(0.05, true));
    assert_eq!(rig.automation.jumps, [0.45]);
    assert_eq!(
        rig.automation.moves,
        [(
            MoveDirection::Forward,
            MovePace::Walk,
            0.0,
            MoveUnit::MetersOrDegrees
        )]
    );
    assert!(!rig.automation.intents.iter().any(|intent| intent.jump));

    assert!(rig.tick(0.45, true));
    rig.automation.navigation_snapshot = snapshot_airborne(position_facing(0.0, 0.0, 90.0), true);
    assert!(rig.tick(0.05, true));
    assert!(rig.automation.stopped_moves.is_empty());
    rig.automation.navigation_snapshot = snapshot(position_facing(0.0, 0.0, 90.0));
    assert!(rig.tick(0.25, true));

    assert_eq!(rig.automation.jumps, [0.45]);
    assert_eq!(rig.automation.stopped_moves, [Some(MoveChannel::Travel)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: JumpChargeExecutionClampsAtAuthenticTwoThousandMillisecondCeiling
#[test]
fn jump_charge_execution_clamps_at_authentic_two_thousand_millisecond_ceiling() {
    let mut rig = jump_route(90.0, 90.0, 5000);

    assert!(rig.tick(0.9, true));
    assert_eq!(rig.automation.jumps, [1.0]);
    assert_eq!(rig.status(), "Charging jump: 2000ms.");

    assert!(rig.tick(0.2, true));
    assert_eq!(rig.status(), "Jump released.");
    assert_eq!(rig.automation.jumps.len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AZeroChargeRouteJumpTapsWithTheLeastPowerTheClientTakes
#[test]
fn a_zero_charge_route_jump_taps_with_the_least_power_the_client_takes() {
    let mut rig = jump_route(90.0, 90.0, 0);

    assert!(rig.tick(0.05, true));

    assert_eq!(rig.automation.jumps, [jump_power::LEAST]);
    assert_eq!(jump_power::LEAST.to_bits(), 1, "float.Epsilon");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: JumpAlignsFarMoreTightlyThanTheWalk
#[test]
fn jump_aligns_far_more_tightly_than_the_walk() {
    for (offset, charges) in [(3.9_f32, false), (0.05, false), (0.005, true)] {
        let mut jump = waypoint(RouteWaypointType::Jump, position(0.0, 0.0));
        jump.jump_heading_degrees = 90.0;
        jump.jump_charge_milliseconds = 500;
        let mut rig = rig(
            FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 90.0 - offset))),
            RouteMode::Circular,
            vec![jump],
        );

        assert!(rig.tick(0.05, true));

        assert_eq!(!rig.automation.jumps.is_empty(), charges, "{offset}");
        assert_eq!(
            rig.automation.faced_headings.len(),
            usize::from(!charges),
            "{offset}"
        );
    }
}

fn circular_jump_at_north() -> Rig {
    let mut jump = waypoint(RouteWaypointType::Jump, position(0.0, 0.0));
    jump.jump_heading_degrees = 90.0;
    rig(
        FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 0.0))),
        RouteMode::Circular,
        vec![jump],
    )
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: JumpDoesNotReissueFaceHeadingInsideTwoSeconds
#[test]
fn jump_does_not_reissue_face_heading_inside_two_seconds() {
    let mut rig = circular_jump_at_north();

    assert!(rig.frame(0.05));
    assert_eq!(rig.automation.faced_headings.len(), 1);

    // Past the walk's re-face interval and well short of the jump's.
    assert!(rig.frame(1.0));
    assert_eq!(rig.automation.faced_headings.len(), 1);

    assert!(rig.frame(1.2));
    assert_eq!(rig.automation.faced_headings.len(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: TheJumpReFaceWantsStrictlyMoreThanItsInterval
#[test]
fn the_jump_re_face_wants_strictly_more_than_its_interval() {
    let mut rig = circular_jump_at_north();

    // Both figures are exact in binary, so the second frame lands the gap on
    // the interval itself.
    assert!(rig.frame(0.25));
    assert_eq!(rig.automation.faced_headings.len(), 1);
    assert!(rig.frame(2.0));
    assert_eq!(rig.automation.faced_headings.len(), 1);
}

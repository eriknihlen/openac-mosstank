//! The route's steering, its mover, its clock and its round
//! (`NavigationTests`, the first half).

mod common;

use ac_plugin_abi::navigation::MovementIntent;
use common::*;
use mosstank_automation::heading::{desired_heading, prefers_left_turn, signed_heading_delta};
use mosstank_formats::route::{RouteMode, RouteWaypointType};
use mosstank_meta::scheduler::{ControllerMacroRule, MacroPassContext, MacroRule};

fn single(intents: &[MovementIntent]) -> MovementIntent {
    assert_eq!(intents.len(), 1, "{intents:?}");
    intents[0].clone()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: DesiredHeadingUsesVtankCompassConvention
#[test]
fn desired_heading_uses_vtank_compass_convention() {
    for (east_west, north_south, expected) in [
        (0.0, 1.0, 0.0_f32),
        (1.0, 0.0, 90.0),
        (0.0, -1.0, 180.0),
        (-1.0, 0.0, 270.0),
    ] {
        let origin = position(0.0, 0.0);
        let target = position(east_west, north_south);
        assert_eq!(desired_heading(&origin, &target), expected);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: SignedHeadingDeltaChoosesShortestAuthenticTurn
#[test]
fn signed_heading_delta_chooses_shortest_authentic_turn() {
    for (current, desired, expected) in [
        (350.0_f32, 10.0_f32, 20.0_f32),
        (10.0, 350.0, -20.0),
        (90.0, 270.0, 180.0),
    ] {
        assert_eq!(signed_heading_delta(current, desired), expected);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: PointSteeringHoldsATurnKeyAndWaitsBeyondTheFarRelaxation
#[test]
fn point_steering_holds_a_turn_key_and_waits_beyond_the_far_relaxation() {
    let automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 0.0)));
    let mut rig = rig(
        automation,
        RouteMode::Circular,
        vec![waypoint(RouteWaypointType::Point, position(1.0, 0.0))],
    );

    assert!(rig.tick(0.05, true));

    let intent = single(&rig.automation.intents);
    assert!(intent.turn_right);
    assert!(!intent.turn_left);
    assert!(!intent.forward);
    // Turning in place keeps the run key held, so the turn plays at its
    // fast rate.
    assert!(intent.run);
    assert!(rig.automation.faced_headings.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: PointSteeringRelaxesToFortyFiveDegreesBeyondThreeMetres
#[test]
fn point_steering_relaxes_to_forty_five_degrees_beyond_three_metres() {
    for (heading, turn_right, moves) in [
        (40.0_f32, true, false),
        (44.0, true, false),
        (46.0, true, true),
        (135.0, false, true),
        (136.0, false, false),
    ] {
        let automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, heading)));
        let mut rig = rig(
            automation,
            RouteMode::Circular,
            vec![waypoint(RouteWaypointType::Point, position(1.0, 0.0))],
        );

        assert!(rig.tick(0.05, true));

        let intent = single(&rig.automation.intents);
        assert_eq!(intent.turn_right, turn_right, "{heading}");
        assert_eq!(intent.turn_left, !turn_right, "{heading}");
        assert_eq!(intent.forward, moves, "{heading}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: PointSteeringTightensToFifteenDegreesInsideThreeMetres
#[test]
fn point_steering_tightens_to_fifteen_degrees_inside_three_metres() {
    for (heading, moves) in [(70.0_f32, false), (78.0, true)] {
        // 2.5 m east of the origin: inside the near tier, outside both the
        // creep band and the 2 m arrival radius.
        let goal = position(2.5 / 240.0, 0.0);
        let automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, heading)));
        let mut rig = rig(
            automation,
            RouteMode::Circular,
            vec![waypoint(RouteWaypointType::Point, goal)],
        );

        assert!(rig.tick(0.05, true));

        let intent = single(&rig.automation.intents);
        assert!(intent.turn_right);
        assert_eq!(intent.forward, moves, "{heading}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: PointSteeringWalksInsideTheCreepBand
#[test]
fn point_steering_walks_inside_the_creep_band() {
    // 1.0 m east: inside the 1.5 m creep band, outside the 0.5 m arrival.
    let goal = position(1.0 / 240.0, 0.0);
    let automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 90.0)));
    let mut rig = rig_min(
        automation,
        RouteMode::Circular,
        0.5,
        vec![waypoint(RouteWaypointType::Point, goal)],
    );

    assert!(rig.tick(0.05, true));

    let intent = single(&rig.automation.intents);
    assert!(intent.forward);
    assert!(!intent.run);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: ANarrowArrivalRadiusIsHonouredDownToOneWalkingStep
#[test]
fn a_narrow_arrival_radius_is_honoured_down_to_one_walking_step() {
    for (distance_meters, minimum_distance_meters, walks_on) in [
        (0.30, 0.15, true),
        (0.20, 0.15, true),
        (0.14, 0.15, false),
        (0.14, 0.01, false),
    ] {
        let goal = position(distance_meters / 240.0, 0.0);
        let automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 90.0)));
        let mut rig = rig_min(
            automation,
            RouteMode::Circular,
            minimum_distance_meters,
            vec![
                waypoint(RouteWaypointType::Point, goal),
                waypoint(RouteWaypointType::Point, position(0.0, 5.0 / 240.0)),
            ],
        );

        rig.tick(0.05, true);

        assert_eq!(
            rig.automation
                .intents
                .iter()
                .any(|intent| intent.forward && !intent.run),
            walks_on,
            "{distance_meters} {minimum_distance_meters}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: PointSteeringRunsOutsideTheCreepBand
#[test]
fn point_steering_runs_outside_the_creep_band() {
    let goal = position(2.5 / 240.0, 0.0);
    let automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 90.0)));
    let mut rig = rig_min(
        automation,
        RouteMode::Circular,
        0.5,
        vec![waypoint(RouteWaypointType::Point, goal)],
    );

    assert!(rig.tick(0.05, true));

    let intent = single(&rig.automation.intents);
    assert!(intent.forward);
    assert!(intent.run);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: TheMoverStepsOnFramesOnlyWhileTheRuleHoldsItsTurn
#[test]
fn the_mover_steps_on_frames_only_while_the_rule_holds_its_turn() {
    let goal = position(12.0 / 240.0, 0.0);
    let automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 90.0)));
    let mut rig = rig_min(
        automation,
        RouteMode::Circular,
        2.0,
        vec![waypoint(RouteWaypointType::Point, goal)],
    );

    rig.step(0.1, true);
    assert!(rig.automation.intents.is_empty());

    assert!(rig.claim(true));
    let armed = rig.automation.intents.len();
    assert_eq!(armed, 1);

    rig.step(0.1, true);
    assert_eq!(rig.automation.intents.len(), armed + 1);

    rig.stop_for_lost_turn();
    rig.step(0.1, true);
    rig.step(0.1, true);
    assert_eq!(rig.automation.intents.len(), armed + 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: TheMoverStepsNoFasterThanItsOwnInterval
#[test]
fn the_mover_steps_no_faster_than_its_own_interval() {
    let goal = position(12.0 / 240.0, 0.0);
    let automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 90.0)));
    let mut rig = rig_min(
        automation,
        RouteMode::Circular,
        2.0,
        vec![waypoint(RouteWaypointType::Point, goal)],
    );

    assert!(rig.claim(true));
    let armed = rig.automation.intents.len();

    // Four frames of a hundredth of a second: three short of the interval,
    // then one that carries it past.
    for _ in 0..4 {
        rig.step(0.01, true);
    }
    assert_eq!(rig.automation.intents.len(), armed);

    rig.step(0.01, true);
    assert_eq!(rig.automation.intents.len(), armed + 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: TurnDirectionFollowsTheAuthenticAlignmentTest
#[test]
fn turn_direction_follows_the_authentic_alignment_test() {
    for (current, desired, expect_left) in [
        (0.0_f32, 180.0_f32, true),
        (0.0, 90.0, false),
        (0.0, 270.0, true),
        (350.0, 10.0, false),
        (10.0, 350.0, true),
    ] {
        assert_eq!(
            prefers_left_turn(current, desired),
            expect_left,
            "{current} {desired}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: PointSteeringFacesTheHeadingWhileThePlayerIsTyping
#[test]
fn point_steering_faces_the_heading_while_the_player_is_typing() {
    let mut automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 0.0)));
    automation.chat_input_active = true;
    let mut rig = rig(
        automation,
        RouteMode::Circular,
        vec![waypoint(RouteWaypointType::Point, position(1.0, 0.0))],
    );

    assert!(rig.tick(0.05, true));

    assert!(rig.automation.intents.is_empty());
    // A stop only releases what was being held, and nothing was.
    assert_eq!(rig.automation.clear_count, 0);
    assert_eq!(rig.automation.faced_headings, [90.0]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: ThePlayerTypingStillWalksTheLastMetreInsteadOfRunningIt
#[test]
fn the_player_typing_still_walks_the_last_metre_instead_of_running_it() {
    let mut automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 90.0)));
    automation.chat_input_active = true;
    // A metre east: aimed already, and inside the creep band.
    let mut rig = rig_min(
        automation,
        RouteMode::Circular,
        0.5,
        vec![waypoint(
            RouteWaypointType::Point,
            position(1.0 / 240.0, 0.0),
        )],
    );

    assert!(rig.tick(0.05, true));

    let intent = single(&rig.automation.intents);
    assert!(intent.forward);
    assert!(!intent.run);
    assert!(rig.automation.faced_headings.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: PointSteeringDoesNotReissueFaceHeadingInsideSevenTenthsOfASecond
#[test]
fn point_steering_does_not_reissue_face_heading_inside_seven_tenths_of_a_second() {
    let mut automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 0.0)));
    automation.chat_input_active = true;
    let mut rig = rig(
        automation,
        RouteMode::Circular,
        vec![waypoint(RouteWaypointType::Point, position(1.0, 0.0))],
    );

    assert!(rig.frame(0.293));
    assert!(rig.frame(0.293));

    assert_eq!(rig.automation.faced_headings, [90.0]);

    assert!(rig.frame(0.293));
    assert_eq!(rig.automation.faced_headings.len(), 1);
    assert!(rig.frame(0.293));
    assert_eq!(rig.automation.faced_headings.len(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: PointSteeringRunsForwardInsideTheFourDegreeBand
#[test]
fn point_steering_runs_forward_inside_the_four_degree_band() {
    let automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 88.0)));
    let mut rig = rig(
        automation,
        RouteMode::Circular,
        vec![waypoint(RouteWaypointType::Point, position(1.0, 0.0))],
    );

    assert!(rig.tick(0.05, true));

    let intent = single(&rig.automation.intents);
    assert!(intent.forward);
    assert!(intent.run);
    assert!(!intent.turn_left);
    assert!(!intent.turn_right);
    assert!(rig.automation.faced_headings.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AMacroStopReleasesTheMovementTheRouteWasHolding
#[test]
fn a_macro_stop_releases_the_movement_the_route_was_holding() {
    let automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 88.0)));
    let mut rig = rig(
        automation,
        RouteMode::Circular,
        vec![waypoint(RouteWaypointType::Point, position(1.0, 0.0))],
    );
    assert!(rig.tick(0.05, true));
    assert_eq!(rig.automation.intents.len(), 1);
    let released_before = rig.automation.clear_count;

    rig.stop_for_macro_stop();

    assert!(rig.automation.clear_count > released_before);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: ANavigateTierThatLosesTheTurnClearsTheMovementIntent
#[test]
fn a_navigate_tier_that_loses_the_turn_clears_the_movement_intent() {
    let automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 0.0)));
    let mut rig = rig(
        automation,
        RouteMode::Circular,
        vec![waypoint(RouteWaypointType::Point, position(1.0, 0.0))],
    );
    let mut rule: ControllerMacroRule<Rig> = ControllerMacroRule::new(
        "NavigateRouteIdle",
        Box::new(|rig: &mut Rig, context: MacroPassContext| {
            rig.tick(context.elapsed_seconds, context.can_act)
        }),
    )
    .with_gate(Box::new(|_: &mut Rig| true))
    .with_lost_turn(Box::new(|rig: &mut Rig| rig.stop_for_lost_turn()));

    assert!(rule.valid_now(&mut rig, MacroPassContext::new(0.05, true)));
    rule.set_running(&mut rig, true);
    assert!(!rig.automation.intents.is_empty());

    rule.set_running(&mut rig, false);
    assert_eq!(rig.automation.clear_count, 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: SkippingWaypointsMovesTheCursorTheWayArrivingDoes
#[test]
fn skipping_waypoints_moves_the_cursor_the_way_arriving_does() {
    let far = position(5.0, 5.0);
    let points = || {
        vec![
            waypoint(RouteWaypointType::Point, position(0.0, 0.0)),
            waypoint(RouteWaypointType::Point, position(1.0, 0.0)),
            waypoint(RouteWaypointType::Point, position(2.0, 0.0)),
        ]
    };

    let mut circular = rig(
        FakeAutomation::at(snapshot(far.clone())),
        RouteMode::Circular,
        points(),
    );
    assert_eq!(circular.skip(2), 2);
    assert_eq!(circular.index(), 2);
    assert_eq!(circular.skip(1), 1);
    assert_eq!(circular.index(), 0);

    let mut once = rig(
        FakeAutomation::at(snapshot(far.clone())),
        RouteMode::Once,
        points(),
    );
    assert_eq!(once.skip(1), 1);
    assert_eq!(once.index(), 1);
    assert_eq!(once.skip(5), 2);
    assert!(once.controller.has_nothing_left_to_walk(&once.settings));
    assert_eq!(once.skip(1), 0);

    let mut follow = rig(
        FakeAutomation::at(snapshot(far)),
        RouteMode::Target,
        points(),
    );
    assert_eq!(follow.skip(1), 0);
}

fn step_back_points() -> Vec<mosstank_formats::route::RouteWaypoint> {
    vec![
        waypoint(RouteWaypointType::Point, position(0.0, 0.0)),
        waypoint(RouteWaypointType::Point, position(1.0, 0.0)),
        waypoint(RouteWaypointType::Point, position(2.0, 0.0)),
        waypoint(RouteWaypointType::Point, position(3.0, 0.0)),
    ]
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: SteppingBackOnAForwardRouteGoesDownTheListAndStopsAtTheFirst
#[test]
fn stepping_back_on_a_forward_route_goes_down_the_list_and_stops_at_the_first() {
    for mode in [RouteMode::Circular, RouteMode::Linear] {
        let mut route = rig(
            FakeAutomation::at(snapshot(position(5.0, 5.0))),
            mode,
            step_back_points(),
        );
        assert_eq!(route.skip(3), 3);
        assert_eq!(route.index(), 3);

        assert_eq!(route.step_back(1), 1);
        assert_eq!(route.index(), 2);
        assert_eq!(route.step_back(9), 2);
        assert_eq!(route.index(), 0);
        assert_eq!(route.step_back(1), 0);
        assert_eq!(route.index(), 0);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: SteppingBackOnALinearRouteOnItsWayBackGoesUpTheList
#[test]
fn stepping_back_on_a_linear_route_on_its_way_back_goes_up_the_list() {
    let mut route = rig(
        FakeAutomation::at(snapshot(position(5.0, 5.0))),
        RouteMode::Linear,
        step_back_points(),
    );
    // Out to the far end and two points back.
    route.skip(6);
    assert!(route.controller.reversing());
    assert_eq!(route.index(), 1);

    assert_eq!(route.step_back(1), 1);
    assert_eq!(route.index(), 2);
    assert_eq!(route.step_back(9), 1);
    assert_eq!(route.index(), 3);
    assert_eq!(route.step_back(1), 0);
    assert_eq!(route.index(), 3);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: SteppingBackOnAReversedRouteGoesUpTheList
#[test]
fn stepping_back_on_a_reversed_route_goes_up_the_list() {
    let mut route = rig(
        FakeAutomation::at(snapshot(position(5.0, 5.0))),
        RouteMode::Circular,
        step_back_points(),
    );
    route.controller.toggle_reverse();
    // Reversed from the first point: the last, then the one before it.
    route.skip(2);
    assert_eq!(route.index(), 2);

    assert_eq!(route.step_back(1), 1);
    assert_eq!(route.index(), 3);
    assert_eq!(route.step_back(1), 0);
    assert_eq!(route.index(), 3);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: CircularRouteWrapsAndOnceRouteStops
#[test]
fn circular_route_wraps_and_once_route_stops() {
    let first = waypoint(RouteWaypointType::Point, position(0.0, 0.0));
    let second = waypoint(RouteWaypointType::Point, position(1.0, 0.0));
    let mut circular = rig(
        FakeAutomation::at(snapshot(first.position.clone())),
        RouteMode::Circular,
        vec![first.clone(), second.clone()],
    );

    assert!(circular.tick(0.05, true));
    assert_eq!(circular.index(), 1);
    circular.automation.navigation_snapshot = snapshot(second.position.clone());
    assert!(circular.tick(0.05, true));
    assert_eq!(circular.index(), 0);

    let mut once = rig(
        FakeAutomation::at(snapshot(first.position.clone())),
        RouteMode::Once,
        vec![first],
    );

    assert!(once.tick(0.05, true));
    assert!(!once.tick(0.05, true));
    assert_eq!(once.status(), "Once route complete.");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AOnceRouteRunToItsEndStillHoldsEveryPointItStartedWith
#[test]
fn a_once_route_run_to_its_end_still_holds_every_point_it_started_with() {
    let here = position(0.0, 0.0);
    let mut route = rig(
        FakeAutomation::at(snapshot(here.clone())),
        RouteMode::Once,
        (0..3)
            .map(|_| waypoint(RouteWaypointType::Point, here.clone()))
            .collect(),
    );

    // A meta's "navigation route empty" is false while points remain.
    assert!(!route.controller.has_nothing_left_to_walk(&route.settings));

    // Three arrivals walk the cursor off the end of the route.
    assert!(route.tick(0.05, true));
    assert_eq!(route.index(), 1);
    assert_eq!(route.settings.waypoints.len(), 3);
    assert!(route.tick(0.05, true));
    assert_eq!(route.index(), 2);
    assert!(route.tick(0.05, true));
    assert_eq!(route.status(), "Once route complete.");

    // Done, idle, and the route is whole.
    assert!(!route.tick(0.05, true));
    assert_eq!(route.status(), "Once route complete.");
    assert_eq!(route.settings.waypoints.len(), 3);
    assert!(route.controller.has_nothing_left_to_walk(&route.settings));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AFinishedOnceRouteWalksAgainOnceTheRoundIsStartedAfresh
#[test]
fn a_finished_once_route_walks_again_once_the_round_is_started_afresh() {
    let here = position(0.0, 0.0);
    let mut route = rig(
        FakeAutomation::at(snapshot(here.clone())),
        RouteMode::Once,
        vec![
            waypoint(RouteWaypointType::Point, here.clone()),
            waypoint(RouteWaypointType::Point, here),
        ],
    );

    assert!(route.tick(0.05, true));
    assert!(route.tick(0.05, true));
    assert!(!route.tick(0.05, true));
    assert_eq!(route.status(), "Once route complete.");

    route.anchor();

    assert_eq!(route.index(), 0);
    assert!(route.tick(0.05, true));
    assert_eq!(route.index(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: FollowReadsMovingTargetAndHoldsAtMinimumDistance
#[test]
fn follow_reads_moving_target_and_holds_at_minimum_distance() {
    let mut automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 90.0)));
    automation
        .objects
        .insert(7, object(7, "Leader", position(1.0, 0.0)));
    let mut settings = route(
        RouteMode::Target,
        2.0,
        mosstank_formats::route::ClientPathing::WhenStuck,
        Vec::new(),
    );
    settings.follow_target_object_id = 7;
    let mut rig = Rig::new(automation, settings);

    assert!(rig.tick(0.05, true));
    assert!(single(&rig.automation.intents).forward);

    rig.automation
        .objects
        .insert(7, object(7, "Leader", position(0.005, 0.0)));
    assert!(!rig.tick(0.05, true));
    assert_eq!(rig.automation.clear_count, 1);
    assert!(rig.status().to_lowercase().contains("holding"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: FollowAroundCornersUsesOldestUnreachedBreadcrumb
#[test]
fn follow_around_corners_uses_oldest_unreached_breadcrumb() {
    let mut automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 90.0)));
    automation
        .objects
        .insert(7, object(7, "Leader", position(0.1, 0.0)));
    let mut settings = route(
        RouteMode::Target,
        2.0,
        mosstank_formats::route::ClientPathing::WhenStuck,
        Vec::new(),
    );
    settings.follow_target_object_id = 7;
    settings.follow_around_corners = true;
    let mut rig = Rig::new(automation, settings);

    assert!(rig.tick(0.05, true));
    rig.automation
        .objects
        .insert(7, object(7, "Leader", position(0.1, 0.1)));
    rig.automation.intents.clear();

    assert!(rig.tick(0.05, true));

    let intent = single(&rig.automation.intents);
    assert!(intent.forward);
    assert!(!intent.turn_left);
    assert!(!intent.turn_right);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: CheckpointWaitsForServerAcceptedPosition
#[test]
fn checkpoint_waits_for_server_accepted_position() {
    let point = position(0.0, 0.0);
    let mut start = snapshot(point.clone());
    start.confirmed_position = position(0.1, 0.0);
    start.confirmed_position_revision = 4;
    let mut rig = rig(
        FakeAutomation::at(start),
        RouteMode::Once,
        vec![waypoint(RouteWaypointType::Checkpoint, point.clone())],
    );

    assert!(rig.tick(1.0, true));
    assert!(rig.status().to_lowercase().contains("waiting for server"));
    assert!(rig.tick(14.0, true));
    assert!(single(&rig.automation.intents).forward);

    let mut confirmed = snapshot(point.clone());
    confirmed.confirmed_position = point;
    confirmed.confirmed_position_revision = 5;
    rig.automation.navigation_snapshot = confirmed;
    assert!(rig.tick(0.05, true));
    assert!(!rig.tick(0.05, true));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AStartSaysWhichPointItAnchoredTheRoundTo
#[test]
fn a_start_says_which_point_it_anchored_the_round_to() {
    let mut rig = rig(
        FakeAutomation::at(snapshot(position_facing(0.0, 15.0, 0.0))),
        RouteMode::Circular,
        vec![
            waypoint(RouteWaypointType::Point, position(0.0, 0.0)),
            waypoint(RouteWaypointType::Point, position(0.0, 8.0)),
            waypoint(RouteWaypointType::Point, position(0.0, 16.0)),
        ],
    );

    let said = rig.anchor().expect("one line");

    assert!(said.contains("Waypoint 3/3"), "{said}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AMacroStopDisarmsTheMoverTheRouteHadArmed
#[test]
fn a_macro_stop_disarms_the_mover_the_route_had_armed() {
    let mut rig = rig(
        FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 90.0))),
        RouteMode::Circular,
        vec![waypoint(RouteWaypointType::Point, position(10.0, 0.0))],
    );
    assert!(rig.claim(true));
    assert!(rig.automation.intents.iter().any(|intent| intent.forward));

    rig.stop_for_macro_stop();
    assert_eq!(rig.automation.clear_count, 1);
    let intents_after_stop = rig.automation.intents.len();
    rig.step(0.05, true);
    rig.step(0.05, true);
    assert_eq!(rig.automation.intents.len(), intents_after_stop);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: TheFarStopRangeBoundsWhichWaypointIsAGoalAtAll
#[test]
fn the_far_stop_range_bounds_which_waypoint_is_a_goal_at_all() {
    for (waypoint_metres, walks) in [(8.0, true), (20.0, false)] {
        let mut rig = rig(
            FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 90.0))),
            RouteMode::Circular,
            vec![waypoint(
                RouteWaypointType::Point,
                position(waypoint_metres / 240.0, 0.0),
            )],
        );
        rig.settings.maximum_distance_meters = 10.0;

        assert_eq!(rig.tick(0.05, true), walks);
        assert_eq!(!rig.automation.intents.is_empty(), walks);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: TheFarStopRangeAlsoBoundsAFollowTarget
#[test]
fn the_far_stop_range_also_bounds_a_follow_target() {
    for (target_metres, follows) in [(8.0, true), (20.0, false)] {
        let mut automation = FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 90.0)));
        automation.objects.insert(
            42,
            object(42, "Partner", position(target_metres / 240.0, 0.0)),
        );
        let mut rig = rig(automation, RouteMode::Target, Vec::new());
        rig.settings.maximum_distance_meters = 10.0;
        rig.settings.follow_target_object_id = 42;

        assert_eq!(rig.tick(0.05, true), follows);
        assert_eq!(!rig.automation.intents.is_empty(), follows);
    }
}

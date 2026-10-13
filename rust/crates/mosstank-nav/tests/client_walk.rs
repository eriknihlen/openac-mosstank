//! A stuck leg handed to the client's own pathing (`NavigationTests`).

mod common;

use ac_plugin_abi::navigation::{GoToState, NavigationStatus};
use common::*;
use mosstank_formats::route::{ClientPathing, RouteMode, RouteWaypointType};
use mosstank_nav::route_view::route_holds_client_walks;

/// A circular route of one point 12 m north, a character facing north.
fn twelve_metres_north(client_pathing: ClientPathing) -> Rig {
    Rig::new(
        FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 0.0))),
        route(
            RouteMode::Circular,
            2.0,
            client_pathing,
            vec![waypoint(
                RouteWaypointType::Point,
                position(0.0, 12.0 / 240.0),
            )],
        ),
    )
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AStraightWalkStuckForThreeSecondsHandsTheLegToTheClient
#[test]
fn a_straight_walk_stuck_for_three_seconds_hands_the_leg_to_the_client() {
    let goal = position(0.0, 12.0 / 240.0);
    let mut rig = twelve_metres_north(ClientPathing::WhenStuck);

    assert!(rig.claim(true));
    rig.step_frames(2.8);
    assert!(rig.automation.go_to_requests.is_empty());
    assert!(rig.automation.intents.iter().any(|intent| intent.forward));

    rig.step_frames(0.4);

    assert_eq!(rig.automation.go_to_requests, [(goal, 2.0)]);
    assert!(rig.status().contains("walked by the client"));
    // The walk is the route's own, so the route does not hold it.
    assert!(rig.controller.is_client_walking());
    assert!(!route_holds_client_walks(
        true,
        rig.controller.is_client_walking()
    ));
    let intents_at_hand_off = rig.automation.intents.len();
    rig.step_frames(1.0);
    assert_eq!(rig.automation.intents.len(), intents_at_hand_off);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: CoveringGroundKeepsTheStuckClockFromRunning
#[test]
fn covering_ground_keeps_the_stuck_clock_from_running() {
    let mut rig = Rig::new(
        FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 0.0))),
        route(
            RouteMode::Circular,
            2.0,
            ClientPathing::WhenStuck,
            vec![waypoint(
                RouteWaypointType::Point,
                position(0.0, 200.0 / 240.0),
            )],
        ),
    );

    assert!(rig.claim(true));
    for _ in 0..120 {
        rig.automation.navigation_snapshot.position.north_south += 0.05 / 240.0;
        rig.step(0.05, true);
    }

    assert!(rig.automation.go_to_requests.is_empty());
    assert!(rig.automation.posted_system_messages.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: LosingThePassResetsTheStuckClock
#[test]
fn losing_the_pass_resets_the_stuck_clock() {
    let mut rig = twelve_metres_north(ClientPathing::WhenStuck);

    assert!(rig.claim(true));
    rig.step_frames(2.0);
    rig.stop_for_lost_turn();
    rig.step_frames(2.0);
    assert!(rig.claim(true));
    rig.step_frames(2.0);

    assert!(rig.automation.go_to_requests.is_empty());

    rig.step_frames(1.3);

    assert_eq!(rig.automation.go_to_requests.len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: TheClientArrivingAdvancesTheRouteAndReturnsTheKeys
#[test]
fn the_client_arriving_advances_the_route_and_returns_the_keys() {
    let mut rig = Rig::new(
        FakeAutomation::at(snapshot(position_facing(0.0, 0.0, 0.0))),
        route(
            RouteMode::Circular,
            2.0,
            ClientPathing::WhenStuck,
            vec![
                waypoint(RouteWaypointType::Point, position(0.0, 12.0 / 240.0)),
                waypoint(RouteWaypointType::Point, position(12.0 / 240.0, 0.0)),
            ],
        ),
    );

    assert!(rig.claim(true));
    rig.step_frames(3.3);
    assert_eq!(rig.automation.go_to_requests.len(), 1);
    let intents_at_hand_off = rig.automation.intents.len();

    rig.automation.end_go_to(GoToState::Arrived, "arrived");
    rig.step_frames(0.2);

    assert!(rig.status().starts_with("Waypoint 2/2"), "{}", rig.status());
    assert!(rig.automation.intents.len() > intents_at_hand_off);
    assert_eq!(rig.automation.go_to_requests.len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AFailedClientWalkReturnsTheKeysAndTheWaypointGetsNoSecondHandOff
#[test]
fn a_failed_client_walk_returns_the_keys_and_the_waypoint_gets_no_second_hand_off() {
    for ending in [
        GoToState::NoRoute,
        GoToState::Blocked,
        GoToState::Interrupted,
        GoToState::Lost,
    ] {
        let mut rig = twelve_metres_north(ClientPathing::WhenStuck);

        assert!(rig.claim(true));
        rig.step_frames(3.3);
        assert_eq!(rig.automation.go_to_requests.len(), 1);
        let intents_at_hand_off = rig.automation.intents.len();

        rig.automation.end_go_to(ending, "no way");
        rig.step_frames(0.2);
        assert!(rig.automation.intents.len() > intents_at_hand_off);
        assert!(
            rig.automation.intents[intents_at_hand_off..]
                .iter()
                .any(|intent| intent.forward)
        );

        rig.step_frames(3.5);
        assert_eq!(rig.automation.go_to_requests.len(), 1);
        assert_eq!(rig.automation.posted_system_messages.len(), 1, "{ending:?}");
        assert!(rig.automation.posted_system_messages[0].contains("not covered ground"));

        rig.step_frames(3.5);
        assert_eq!(rig.automation.posted_system_messages.len(), 1);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: LosingThePassStopsTheClientsWalk
#[test]
fn losing_the_pass_stops_the_clients_walk() {
    let mut rig = twelve_metres_north(ClientPathing::WhenStuck);

    assert!(rig.claim(true));
    rig.step_frames(3.3);
    assert_eq!(rig.automation.go_to_requests.len(), 1);

    rig.stop_for_lost_turn();

    assert_eq!(rig.automation.stop_go_to_calls, 1);
    assert_eq!(rig.automation.go_to_report.state, GoToState::Stopped);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: ARefusedHandOffLeavesTheStraightWalkWalking
#[test]
fn a_refused_hand_off_leaves_the_straight_walk_walking() {
    let mut rig = twelve_metres_north(ClientPathing::WhenStuck);
    rig.automation.go_to_answer = NavigationStatus::Held;

    assert!(rig.claim(true));
    rig.step_frames(3.3);
    assert_eq!(rig.automation.go_to_requests.len(), 1);
    let intents_after_refusal = rig.automation.intents.len();

    rig.step_frames(0.2);

    assert!(rig.automation.intents.len() > intents_after_refusal);
    assert_eq!(rig.automation.stop_go_to_calls, 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: NeverKeepsTheKeysAndOnlySaysItStalled
#[test]
fn never_keeps_the_keys_and_only_says_it_stalled() {
    let mut rig = twelve_metres_north(ClientPathing::Never);

    assert!(rig.claim(true));
    rig.step_frames(7.0);

    assert!(rig.automation.go_to_requests.is_empty());
    assert_eq!(rig.automation.posted_system_messages.len(), 1);
    let intents = &rig.automation.intents;
    assert!(
        intents[intents.len() - 3..]
            .iter()
            .any(|intent| intent.forward)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/NavigationTests.cs :: AlwaysSendsEveryLegAndPausesOnALegTheClientCannotWalk
#[test]
fn always_sends_every_leg_and_pauses_on_a_leg_the_client_cannot_walk() {
    let mut rig = twelve_metres_north(ClientPathing::Always);

    assert!(rig.claim(true));
    assert_eq!(rig.automation.go_to_requests.len(), 1);
    assert!(rig.automation.intents.is_empty());

    rig.automation.end_go_to(GoToState::NoRoute, "no route");
    rig.step_frames(0.2);
    assert!(!rig.claim(true));
    assert!(rig.status().contains("paused"));
    assert_eq!(rig.automation.posted_system_messages.len(), 1);
    assert!(rig.automation.posted_system_messages[0].contains("paused"));
    assert!(!rig.claim(true));
    assert_eq!(rig.automation.go_to_requests.len(), 1);
    assert!(rig.automation.intents.is_empty());

    rig.reset();
    assert!(rig.claim(true));
    assert_eq!(rig.automation.go_to_requests.len(), 2);
}

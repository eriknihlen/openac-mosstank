//! The route as the panel measures and draws it (`RouteDistanceTests` and
//! the panel's route checks).

use ac_plugin_abi::ui::WorldLine;
use ac_plugin_abi::world::Position;
use mosstank_formats::af;
use mosstank_formats::fixtures;
use mosstank_formats::nav;
use mosstank_formats::route::{NavigationSettings, RouteMode, RouteWaypoint, RouteWaypointType};
use mosstank_nav::route_view::{
    MAX_LEG_METRES, nearest_route_distance_meters, route_holds_client_walks, route_lines,
    walk_pause_reason_for,
};

fn player(east_west: f64, north_south: f64) -> Position {
    Position {
        cell_id: 0x7D64_0013,
        east_west,
        north_south,
        elevation: 0.3,
        heading_degrees: 0.0,
        is_outdoor: true,
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/RouteDistanceTests.cs :: ARouteReadFromAFileIsMeasuredOnItsCoordinates
#[test]
fn a_route_read_from_a_file_is_measured_on_its_coordinates() {
    let text = "NAV: nav0 circular ~~ {\n\tpnt -70.5461029529572 -66.0203276952108 0.366362762451172\n\tpnt -70.2843772888184 -66.0715707778931 0.358354187011719\n";
    let mut route = NavigationSettings::default();
    af::try_load_nav(text, &mut route, None).expect("the route reads");
    // Standing a hundredth of a map unit (2.4 m) east of the first point.
    let here = player(-70.536_102_952_957_2, -66.020_327_695_210_8);

    let meters = nearest_route_distance_meters(&here, &route.waypoints);

    assert!((2.3..=2.5).contains(&meters), "{meters}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/RouteDistanceTests.cs :: AnEmptyRouteIsInfinitelyFar
#[test]
fn an_empty_route_is_infinitely_far() {
    let here = player(-70.5, -66.0);
    let meters = nearest_route_distance_meters(&here, &[]);
    assert!(meters.is_infinite() && meters > 0.0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheRouteNeverHoldsItsOwnClientWalk
#[test]
fn the_route_never_holds_its_own_client_walk() {
    for (route_enabled, route_owns_walk, holds) in [
        (true, false, true),
        (true, true, false),
        (false, false, false),
    ] {
        assert_eq!(
            route_holds_client_walks(route_enabled, route_owns_walk),
            holds
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ShowNavLinesDrawsTheLoadedRouteAndPersistsWithTheProfile (the drawing; the option and its side-car are the panel's)
#[test]
fn show_nav_lines_draws_the_loaded_route_as_rings_and_legs() {
    // The one point of the C# test's route is drawn as its arrival ring.
    let one = NavigationSettings {
        waypoints: vec![RouteWaypoint::at(
            RouteWaypointType::Point,
            -3.25,
            12.5,
            0.0,
        )],
        ..NavigationSettings::default()
    };
    let lines = route_lines(&one, 0);
    assert_eq!(lines.len(), 24);
    assert!(
        lines
            .iter()
            .all(|line| line.follow_terrain && line.width_metres == 0.25)
    );
    // The current point is drawn in its own colour.
    assert!(lines.iter().all(|line| line.color_rgb == 0x0080_FF40));

    // Two points of a circular route: two rings and a leg each way; a
    // portal leaves no leg behind it.
    let mut two = NavigationSettings {
        mode: RouteMode::Circular,
        waypoints: vec![
            RouteWaypoint::at(RouteWaypointType::Point, 0.0, 0.0, 0.0),
            RouteWaypoint::at(RouteWaypointType::Point, 1.0, 0.0, 0.0),
        ],
        ..NavigationSettings::default()
    };
    assert_eq!(route_lines(&two, 5).len(), 24 * 2 + 2);
    two.waypoints[1].kind = RouteWaypointType::Portal;
    assert_eq!(route_lines(&two, 5).len(), 24 * 2 + 1);
}

/// The segments of one arrival ring.
const RING: usize = 24;

/// A route of `waypoints`, run once.
fn once(waypoints: Vec<RouteWaypoint>) -> NavigationSettings {
    NavigationSettings {
        mode: RouteMode::Once,
        waypoints,
        ..NavigationSettings::default()
    }
}

fn at(kind: RouteWaypointType, east_west: f64, north_south: f64) -> RouteWaypoint {
    RouteWaypoint::at(kind, east_west, north_south, 0.0)
}

/// A portal or NPC found by name, written while the character stood at
/// (`stood_ew`, `stood_ns`), the object itself at (`ew`, `ns`).
fn by_name(
    kind: RouteWaypointType,
    stood_ew: f64,
    stood_ns: f64,
    ew: f64,
    ns: f64,
) -> RouteWaypoint {
    let mut waypoint = at(kind, stood_ew, stood_ns);
    waypoint.object_name = "Portal to Town Network".to_owned();
    waypoint.reference_position = mosstank_formats::route::map_position(ew, ns, 0.0);
    waypoint
}

/// The lines that are not ring segments: a ring's radius is at most 50 m,
/// so its 24 segments are at most 13.1 m long; every leg in these tests is
/// longer than 15 m.
fn legs(lines: &[WorldLine]) -> Vec<&WorldLine> {
    lines
        .iter()
        .filter(|line| length_metres(line) > 15.0)
        .collect()
}

fn length_metres(line: &WorldLine) -> f64 {
    (line.end.east_west - line.start.east_west).hypot(line.end.north_south - line.start.north_south)
        * 240.0
}

fn near(point: &ac_plugin_abi::ui::MapPoint, east_west: f64, north_south: f64) -> bool {
    (point.east_west - east_west).abs() < 1e-9 && (point.north_south - north_south).abs() < 1e-9
}

#[test]
fn a_chat_command_or_a_pause_is_not_a_place_on_the_drawn_route() {
    // A chat command and a pause written while the character stood in
    // another town: the leg goes from point to point past them, and
    // neither gets a ring.
    let route = once(vec![
        at(RouteWaypointType::Point, 10.0, 10.0),
        at(RouteWaypointType::ChatCommand, 59.3, -28.5),
        at(RouteWaypointType::Pause, 59.3, -28.5),
        at(RouteWaypointType::Point, 10.1, 10.0),
    ]);
    let lines = route_lines(&route, 1);
    assert_eq!(lines.len(), RING * 2 + 1);
    let drawn_legs = legs(&lines);
    assert_eq!(drawn_legs.len(), 1);
    assert!(near(&drawn_legs[0].start, 10.0, 10.0) && near(&drawn_legs[0].end, 10.1, 10.0));
    // The current waypoint is the chat command, which is not drawn: nothing
    // is in the current point's colour.
    assert!(lines.iter().all(|line| line.color_rgb != 0x0080_FF40));
}

#[test]
fn a_recall_or_a_vendor_is_not_a_place_and_a_recall_breaks_the_line() {
    // Neither route file keeps where a vendor stands: it is passed over.
    let route = once(vec![
        at(RouteWaypointType::Point, 10.0, 10.0),
        at(RouteWaypointType::OpenVendor, 40.0, 40.0),
        at(RouteWaypointType::Point, 10.1, 10.0),
        at(RouteWaypointType::Recall, 10.1, 10.0),
        at(RouteWaypointType::Point, 10.2, 10.0),
    ]);
    let lines = route_lines(&route, -1);
    assert_eq!(lines.len(), RING * 3 + 1);
    let drawn_legs = legs(&lines);
    assert_eq!(drawn_legs.len(), 1);
    assert!(near(&drawn_legs[0].start, 10.0, 10.0) && near(&drawn_legs[0].end, 10.1, 10.0));
}

#[test]
fn a_circular_route_does_not_close_across_an_unrecorded_teleport() {
    // The closing leg, 2 km, passes over a chat command at the start or at
    // the end of the route: it is a teleport, and is not drawn.
    let far = MAX_LEG_METRES * 2.0 / 240.0;
    let circular = |waypoints| NavigationSettings {
        mode: RouteMode::Circular,
        waypoints,
        ..NavigationSettings::default()
    };
    let at_the_start = circular(vec![
        at(RouteWaypointType::ChatCommand, 1.0, 1.0),
        at(RouteWaypointType::Point, 1.0, 1.0),
        at(RouteWaypointType::Point, 1.0 + far, 1.0),
    ]);
    assert_eq!(route_lines(&at_the_start, -1).len(), RING * 2 + 1);
    let at_the_end = circular(vec![
        at(RouteWaypointType::Point, 1.0, 1.0),
        at(RouteWaypointType::Point, 1.0 + far, 1.0),
        at(RouteWaypointType::ChatCommand, 1.0, 1.0),
    ]);
    assert_eq!(route_lines(&at_the_end, -1).len(), RING * 2 + 1);

    // With nothing between them, the closing leg is a walk, and is drawn.
    let walked = circular(vec![
        at(RouteWaypointType::Point, 1.0, 1.0),
        at(RouteWaypointType::Point, 1.0 + far, 1.0),
    ]);
    assert_eq!(route_lines(&walked, -1).len(), RING * 2 + 2);
}

#[test]
fn a_place_after_an_action_starts_a_fresh_leg() {
    // The chat command marks only the leg past it: the next leg, 2 km
    // straight from point to point, is drawn.
    let far = MAX_LEG_METRES * 2.0 / 240.0;
    let route = once(vec![
        at(RouteWaypointType::Point, 1.0, 1.0),
        at(RouteWaypointType::ChatCommand, 1.0, 1.0),
        at(RouteWaypointType::Point, 1.1, 1.0),
        at(RouteWaypointType::Point, 1.1 + far, 1.0),
    ]);
    let lines = route_lines(&route, -1);
    assert_eq!(lines.len(), RING * 3 + 2);
    assert!(
        legs(&lines)
            .iter()
            .any(|leg| near(&leg.start, 1.1, 1.0) && near(&leg.end, 1.1 + far, 1.0))
    );
}

#[test]
fn an_object_with_no_recorded_position_is_not_drawn() {
    // A file that leaves the object's position empty: the loaders read it
    // as the map's centre, marked outdoors.
    let mut empty = by_name(RouteWaypointType::UseNpc, 10.0, 10.0, 0.0, 0.0);
    empty.reference_position = mosstank_formats::route::map_position(0.0, 0.0, 0.0);
    assert!(route_lines(&once(vec![empty]), -1).is_empty());

    // A NAV file that marks the position not valid.
    let mut invalid = by_name(RouteWaypointType::PortalByName, 10.0, 10.0, 10.1, 10.0);
    invalid.legacy_reference_valid = false;
    assert!(route_lines(&once(vec![invalid]), -1).is_empty());
}

#[test]
fn a_jump_is_a_place_but_no_leg_leaves_it() {
    // Where the jump comes down is not recorded.
    let route = once(vec![
        at(RouteWaypointType::Point, 10.0, 10.0),
        at(RouteWaypointType::Jump, 10.1, 10.0),
        at(RouteWaypointType::Point, 10.2, 10.0),
    ]);
    let lines = route_lines(&route, -1);
    assert_eq!(lines.len(), RING * 3 + 1);
    let drawn_legs = legs(&lines);
    assert!(near(&drawn_legs[0].start, 10.0, 10.0) && near(&drawn_legs[0].end, 10.1, 10.0));
}

#[test]
fn a_portal_or_npc_by_name_is_drawn_where_the_object_is() {
    // Written while the character stood far away; the portal is just
    // past the last point. No leg leaves the portal.
    let route = once(vec![
        at(RouteWaypointType::Point, 59.30, -28.72),
        by_name(RouteWaypointType::PortalByName, 0.67, 13.57, 59.39, -28.72),
        at(RouteWaypointType::Point, 59.48, -28.72),
    ]);
    let lines = route_lines(&route, -1);
    assert_eq!(lines.len(), RING * 3 + 1);
    let drawn_legs = legs(&lines);
    assert_eq!(drawn_legs.len(), 1);
    assert!(near(&drawn_legs[0].end, 59.39, -28.72));
    assert!(lines.iter().all(|line| line.start.east_west > 59.0));

    // An NPC is a place too, and the route goes on from it.
    let npc = once(vec![
        at(RouteWaypointType::Point, 59.30, -28.72),
        by_name(RouteWaypointType::UseNpc, 0.67, 13.57, 59.39, -28.72),
        at(RouteWaypointType::Point, 59.48, -28.72),
    ]);
    assert_eq!(legs(&route_lines(&npc, -1)).len(), 2);

    // Without a recorded object position it is not drawn.
    let unknown = once(vec![at(RouteWaypointType::UseNpc, 0.67, 13.57)]);
    assert!(route_lines(&unknown, -1).is_empty());
}

#[test]
fn a_teleport_the_route_has_no_waypoint_for_draws_no_leg() {
    // `/ah` and a wait, then the route goes on 17 km away.
    let route = once(vec![
        at(RouteWaypointType::Point, 0.675, 13.57),
        at(RouteWaypointType::ChatCommand, 0.675, 13.57),
        at(RouteWaypointType::Point, 59.33, -28.59),
    ]);
    let lines = route_lines(&route, -1);
    assert_eq!(lines.len(), RING * 2);

    // Past a chat command, a leg just under the limit is a walk, and is
    // drawn; one just over it is not.
    let past_a_command = |metres: f64| {
        once(vec![
            at(RouteWaypointType::Point, 1.0, 1.0),
            at(RouteWaypointType::ChatCommand, 1.0, 1.0),
            at(RouteWaypointType::Point, 1.0 + metres / 240.0, 1.0),
        ])
    };
    assert_eq!(
        route_lines(&past_a_command(MAX_LEG_METRES * 0.99), -1).len(),
        RING * 2 + 1
    );
    assert_eq!(
        route_lines(&past_a_command(MAX_LEG_METRES * 1.01), -1).len(),
        RING * 2
    );

    // A leg straight from one point to the next is a walk however long.
    let far = MAX_LEG_METRES * 2.0 / 240.0;
    let route = once(vec![
        at(RouteWaypointType::Point, 1.0, 1.0),
        at(RouteWaypointType::Point, 1.0 + far, 1.0),
    ]);
    assert_eq!(route_lines(&route, -1).len(), RING * 2 + 1);
}

#[test]
fn a_circular_route_closes_over_what_is_not_a_place_but_not_over_a_recall() {
    let mut route = NavigationSettings {
        mode: RouteMode::Circular,
        waypoints: vec![
            at(RouteWaypointType::ChatCommand, 40.0, 40.0),
            at(RouteWaypointType::Point, 10.0, 10.0),
            at(RouteWaypointType::Point, 10.1, 10.0),
            at(RouteWaypointType::Pause, 40.0, 40.0),
        ],
        ..NavigationSettings::default()
    };
    let lines = route_lines(&route, -1);
    assert_eq!(lines.len(), RING * 2 + 2);
    assert!(
        legs(&lines)
            .iter()
            .any(|leg| near(&leg.start, 10.1, 10.0) && near(&leg.end, 10.0, 10.0))
    );

    route.waypoints[0].kind = RouteWaypointType::Recall;
    assert_eq!(route_lines(&route, -1).len(), RING * 2 + 1);
}

/// The places a route's lines may touch: every point, checkpoint, portal
/// and jump, and the recorded object of a portal or NPC by name.
fn place_of(waypoint: &RouteWaypoint) -> Option<(f64, f64)> {
    match waypoint.kind {
        RouteWaypointType::PortalByName | RouteWaypointType::UseNpc => Some((
            waypoint.reference_position.east_west,
            waypoint.reference_position.north_south,
        )),
        RouteWaypointType::Point
        | RouteWaypointType::Checkpoint
        | RouteWaypointType::Portal
        | RouteWaypointType::Jump => {
            Some((waypoint.position.east_west, waypoint.position.north_south))
        }
        _ => None,
    }
}

#[test]
fn the_fixture_routes_draw_no_line_across_the_world() {
    // Each of these draws a line kilometres long when every waypoint is
    // taken for a place: a chat command or a portal by name keeps where the
    // character stood when it was written, and `/ah` or `/ub use Gateway`
    // teleport with no waypoint saying so.
    for name in [
        "nav_ab.nav",
        "nav_empyrean.nav",
        "nav_jondortorgren.nav",
        "nav_lockandkeyjaw.nav",
        "barristunnels.nav",
        "jaw_1.nav",
    ] {
        let mut route = NavigationSettings::default();
        nav::try_load(
            &fixtures::read_vtank(&format!("nav/{name}")),
            &mut route,
            None,
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
        let lines = route_lines(&route, 0);
        assert!(!lines.is_empty(), "{name}");
        let places: Vec<(f64, f64)> = route.waypoints.iter().filter_map(place_of).collect();
        // A ring is at most 50 m from its place.
        let reach = 50.0 / 240.0 + 1e-9;
        for line in &lines {
            assert!(
                length_metres(line) <= MAX_LEG_METRES,
                "{name}: a {:.0} m line",
                length_metres(line)
            );
            for end in [&line.start, &line.end] {
                assert!(
                    places.iter().any(|&(ew, ns)| {
                        (end.east_west - ew).abs() <= reach && (end.north_south - ns).abs() <= reach
                    }),
                    "{name}: a line ends at {}, {} away from every place",
                    end.east_west,
                    end.north_south
                );
            }
        }
    }
}

#[test]
fn the_lock_and_key_jaw_route_is_one_line_through_the_dungeon_past_its_chat_commands() {
    // Points 6 and 8 are in the dungeon; chat command 7 between them was
    // written in town 40 km away. Before, that drew a line out to the town
    // and another back: a double line running off to the horizon.
    let mut route = NavigationSettings::default();
    nav::try_load(
        &fixtures::read_vtank("nav/nav_lockandkeyjaw.nav"),
        &mut route,
        None,
    )
    .expect("the route reads");
    let six = &route.waypoints[6].position;
    let eight = &route.waypoints[8].position;
    assert_eq!(route.waypoints[7].kind, RouteWaypointType::ChatCommand);
    let lines = route_lines(&route, -1);
    assert!(lines.iter().any(|line| {
        near(&line.start, six.east_west, six.north_south)
            && near(&line.end, eight.east_west, eight.north_south)
    }));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AWalkTheClientPlansWaitsWhileTheMacroHasSomethingToDo
#[test]
fn a_walk_the_client_plans_waits_while_the_macro_has_something_to_do() {
    for (running, last_rule, buffing, route_navigation, looting, seconds_since_attack, expected) in [
        (false, Some("Attack"), false, false, false, 99.0, None),
        (true, None, false, false, false, 99.0, None),
        (true, Some("IdlePeace"), false, false, false, 99.0, None),
        (true, Some("RandomHelper"), false, false, false, 99.0, None),
        (
            true,
            Some("NavigateRouteIdle"),
            false,
            false,
            false,
            99.0,
            None,
        ),
        (
            true,
            Some("Attack"),
            false,
            false,
            false,
            99.0,
            Some("MossTank is running Attack"),
        ),
        (
            true,
            Some("BuffSelf"),
            false,
            false,
            false,
            99.0,
            Some("MossTank is running BuffSelf"),
        ),
        (
            true,
            Some("NavigateRouteIdle"),
            false,
            true,
            false,
            99.0,
            Some("MossTank is following its route"),
        ),
        (
            true,
            Some("NavigateRouteIdle"),
            false,
            false,
            true,
            1.0,
            Some("MossTank is waiting for a corpse to loot"),
        ),
        (
            true,
            Some("NavigateRouteIdle"),
            false,
            false,
            false,
            1.0,
            None,
        ),
        (
            true,
            Some("NavigateRouteIdle"),
            false,
            false,
            true,
            3.0,
            None,
        ),
        (
            false,
            None,
            true,
            false,
            false,
            99.0,
            Some("MossTank is buffing"),
        ),
    ] {
        assert_eq!(
            walk_pause_reason_for(
                running,
                last_rule,
                buffing,
                route_navigation,
                looting,
                seconds_since_attack
            )
            .as_deref(),
            expected,
            "{running} {last_rule:?} {buffing} {route_navigation} {looting} {seconds_since_attack}"
        );
    }
}

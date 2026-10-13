//! The route as the panel measures and draws it (`RouteDistanceTests` and
//! the panel's route checks).

use ac_plugin_abi::world::Position;
use mosstank_formats::af;
use mosstank_formats::route::{NavigationSettings, RouteMode, RouteWaypoint, RouteWaypointType};
use mosstank_nav::route_view::{
    nearest_route_distance_meters, route_holds_client_walks, route_lines,
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
    let lines = route_lines(&one, 0, true);
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
    assert_eq!(route_lines(&two, 5, false).len(), 24 * 2 + 2);
    two.waypoints[1].kind = RouteWaypointType::Portal;
    assert_eq!(route_lines(&two, 5, false).len(), 24 * 2 + 1);
}

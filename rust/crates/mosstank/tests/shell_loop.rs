//! The macro's loop around its rules (ported from the C#'s
//! `MossTankPanelTests`): death, start and stop and the route's round, the
//! waypoint verbs, the meta interval and the log channels.

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::navigation::NavigationSnapshot;
use ac_plugin_abi::world::Position;
use common::{World, character};
use mosstank::MossTank;
use mosstank_formats::route::{NavigationSettings, RouteMode, RouteWaypoint, RouteWaypointType};

/// In the world at full health, standing at east-west 0 facing north
/// (`NavigationAt(0f)`).
fn standing_world() -> (World, MossTank) {
    let mut world = World::with_character("Barris");
    stand_at(&mut world, 0.0, 0.0);
    let plugin = world.enable();
    (world, plugin)
}

/// The character stands at `east_west`, at `elevation` (`StandAt`,
/// `StandAtHeight`).
fn stand_at(world: &mut World, east_west: f64, elevation: f64) {
    world.set_reply::<calls::NavigationSnapshot>(&NavigationSnapshot {
        is_available: true,
        local_object_id: 1,
        position: Position {
            cell_id: 0x0001_0001,
            east_west,
            elevation,
            is_outdoor: true,
            ..Position::default()
        },
        ..NavigationSnapshot::default()
    });
}

fn point(east_west: f64, elevation: f64) -> RouteWaypoint {
    RouteWaypoint {
        kind: RouteWaypointType::Point,
        position: Position {
            cell_id: 0x0001_0001,
            east_west,
            elevation,
            is_outdoor: true,
            ..Position::default()
        },
        ..RouteWaypoint::default()
    }
}

/// Writes a route and loads it by command.
fn load_route(world: &mut World, plugin: &mut MossTank, name: &str, route: &NavigationSettings) {
    let text = mosstank_formats::nav::save(route).unwrap();
    world
        .profiles
        .insert(&format!("mosstank/navs/{name}.nav"), &text);
    world.vt(plugin, &format!("nav load {name}"));
}

/// Two points, at east-west 0 and 50, with the character back at 0
/// (`TwoPointRoute`).
fn two_point_route(world: &mut World, plugin: &mut MossTank, mode: RouteMode) {
    let route = NavigationSettings {
        mode,
        waypoints: vec![point(0.0, 0.0), point(50.0, 0.0)],
        ..NavigationSettings::default()
    };
    load_route(world, plugin, "TwoPoints", &route);
    stand_at(world, 0.0, 0.0);
}

fn index(plugin: &MossTank) -> i32 {
    plugin.tank().unwrap().route_waypoint_index()
}

fn running(plugin: &MossTank) -> bool {
    plugin.tank().unwrap().macro_running()
}

fn option(plugin: &MossTank, name: &str) -> bool {
    plugin.tank().unwrap().option(name).is_truthy()
}

fn set_option(world: &mut World, plugin: &mut MossTank, name: &str, value: bool) {
    world.vt(plugin, &format!("opt set {name} {value}"));
}

fn dead(world: &mut World) {
    let mut info = character("Barris");
    info.health = (0, 100, 100);
    world.fake.character = Some(info);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: DeathWithStopMacroOnDeathKeepsTheMacroAndDisablesTheFourSwitches
#[test]
fn death_with_stop_macro_on_death_keeps_the_macro_and_disables_the_four_switches() {
    let (mut world, mut plugin) = standing_world();
    for name in [
        "StopMacroOnDeath",
        "EnableNav",
        "EnableLooting",
        "EnableBuffing",
        "EnableCombat",
    ] {
        set_option(&mut world, &mut plugin, name, true);
    }
    world.vt(&mut plugin, "start");
    world.tick(&mut plugin);
    assert!(running(&plugin));

    dead(&mut world);
    world.tick(&mut plugin);

    assert!(running(&plugin));
    for name in [
        "EnableNav",
        "EnableLooting",
        "EnableBuffing",
        "EnableCombat",
    ] {
        assert!(!option(&plugin, name), "{name}");
    }
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("deathrestore"))
    );

    world.vt(&mut plugin, "deathrestore");

    for name in [
        "EnableNav",
        "EnableLooting",
        "EnableBuffing",
        "EnableCombat",
    ] {
        assert!(option(&plugin, name), "{name}");
    }
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("back to their earlier values"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: DeathWithStopMacroOnDeathOffChangesNothing
#[test]
fn death_with_stop_macro_on_death_off_changes_nothing() {
    let (mut world, mut plugin) = standing_world();
    set_option(&mut world, &mut plugin, "StopMacroOnDeath", false);
    set_option(&mut world, &mut plugin, "EnableNav", true);
    world.vt(&mut plugin, "start");
    world.tick(&mut plugin);

    dead(&mut world);
    world.tick(&mut plugin);

    assert!(running(&plugin));
    assert!(option(&plugin, "EnableNav"));
    assert!(
        !world
            .chat()
            .iter()
            .any(|line| line.contains("stopped because"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: StoppingTheMacroKeepsTheWaypointTheRouteWasWalkingTo
#[test]
fn stopping_the_macro_keeps_the_waypoint_the_route_was_walking_to() {
    let (mut world, mut plugin) = standing_world();
    two_point_route(&mut world, &mut plugin, RouteMode::Circular);
    set_option(&mut world, &mut plugin, "EnableNav", true);
    world.vt(&mut plugin, "start");
    world.ticks(&mut plugin, 80);
    stand_at(&mut world, 25.0, 0.0);
    assert_eq!(index(&plugin), 1);

    world.vt(&mut plugin, "stop");

    assert!(!running(&plugin));
    assert_eq!(index(&plugin), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: DeathLeavesTheRouteOnTheWaypointItWasWalkingTo
#[test]
fn death_leaves_the_route_on_the_waypoint_it_was_walking_to() {
    let (mut world, mut plugin) = standing_world();
    two_point_route(&mut world, &mut plugin, RouteMode::Circular);
    set_option(&mut world, &mut plugin, "EnableNav", true);
    world.vt(&mut plugin, "start");
    world.ticks(&mut plugin, 80);
    stand_at(&mut world, 25.0, 0.0);
    assert_eq!(index(&plugin), 1);

    dead(&mut world);
    world.tick(&mut plugin);

    assert!(running(&plugin));
    assert_eq!(index(&plugin), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: StartingTheMacroAnchorsTheRoundToTheNearestPointOfTheRoute
#[test]
fn starting_the_macro_anchors_the_round_to_the_nearest_point_of_the_route() {
    let (mut world, mut plugin) = standing_world();
    two_point_route(&mut world, &mut plugin, RouteMode::Circular);
    set_option(&mut world, &mut plugin, "EnableNav", true);

    stand_at(&mut world, 48.0, 0.0);
    world.vt(&mut plugin, "start");
    assert_eq!(index(&plugin), 1);

    world.vt(&mut plugin, "stop");
    stand_at(&mut world, 2.0, 0.0);
    world.vt(&mut plugin, "start");

    assert_eq!(index(&plugin), 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: OnlyThePlacesOnARouteCanAnchorTheRound
#[test]
fn only_the_places_on_a_route_can_anchor_the_round() {
    let (mut world, mut plugin) = standing_world();
    let pause = RouteWaypoint {
        kind: RouteWaypointType::Pause,
        duration_milliseconds: 5000,
        ..point(99.0, 0.0)
    };
    let route = NavigationSettings {
        waypoints: vec![point(0.0, 0.0), point(50.0, 0.0), pause],
        ..NavigationSettings::default()
    };
    load_route(&mut world, &mut plugin, "Places", &route);
    set_option(&mut world, &mut plugin, "EnableNav", true);

    stand_at(&mut world, 98.0, 0.0);
    world.vt(&mut plugin, "start");

    assert_eq!(index(&plugin), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: HeightCountsWhenPickingTheNearestPointOfTheRoute
#[test]
fn height_counts_when_picking_the_nearest_point_of_the_route() {
    let (mut world, mut plugin) = standing_world();
    let route = NavigationSettings {
        waypoints: vec![point(0.0, 0.0), point(0.0, 10.0)],
        ..NavigationSettings::default()
    };
    load_route(&mut world, &mut plugin, "Floors", &route);
    set_option(&mut world, &mut plugin, "EnableNav", true);

    stand_at(&mut world, 0.0, 9.0);
    world.vt(&mut plugin, "start");

    assert_eq!(index(&plugin), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AOnceRouteStartsAtItsHead
#[test]
fn a_once_route_starts_at_its_head() {
    let (mut world, mut plugin) = standing_world();
    two_point_route(&mut world, &mut plugin, RouteMode::Once);
    set_option(&mut world, &mut plugin, "EnableNav", true);

    stand_at(&mut world, 48.0, 0.0);
    world.vt(&mut plugin, "start");

    assert_eq!(index(&plugin), 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: LoadingARouteStillPutsTheRoundBackToItsFirstPoint
#[test]
fn loading_a_route_still_puts_the_round_back_to_its_first_point() {
    let (mut world, mut plugin) = standing_world();
    // `TwoPointRoute`: the editor's Add Point at 0 and at 50.
    world.with_tank(&mut plugin, |tank, client| tank.add_route_point(client));
    stand_at(&mut world, 50.0, 0.0);
    world.with_tank(&mut plugin, |tank, client| tank.add_route_point(client));
    stand_at(&mut world, 0.0, 0.0);
    set_option(&mut world, &mut plugin, "EnableNav", true);
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    for _ in 0..4 {
        world.with_tank(&mut plugin, |tank, client| tank.on_tick(client, 0.3));
    }
    assert_eq!(index(&plugin), 1);

    world.with_tank(&mut plugin, |tank, client| tank.add_route_point(client));

    assert_eq!(index(&plugin), 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ACircuitLoadedWhileRunningStartsAtItsNearestPoint
#[test]
fn a_circuit_loaded_while_running_starts_at_its_nearest_point() {
    let mut world = World::with_character("Barris");
    world.profiles.insert(
        "mosstank/navs/Circle.af",
        "NAV: nav0 circular ~~ {\r\n\tpnt 0 0 0\r\n\tpnt 0.2 0 0\r\n\tpnt 0.4 0 0\r\n",
    );
    stand_at(&mut world, 0.0, 0.0);
    let mut plugin = world.enable();
    world.vt(&mut plugin, "start");
    assert!(running(&plugin));

    stand_at(&mut world, 0.4, 0.0);
    world.vt(&mut plugin, "nav load Circle");

    assert_eq!(index(&plugin), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheFakeDeathVerbRunsTheWholeDeathHandler
#[test]
fn the_fake_death_verb_runs_the_whole_death_handler() {
    let (mut world, mut plugin) = standing_world();
    set_option(&mut world, &mut plugin, "StopMacroOnDeath", true);
    world.vt(&mut plugin, "start");
    world.tick(&mut plugin);
    assert!(running(&plugin));

    world.vt(&mut plugin, "fakedeath");

    assert!(running(&plugin));
    assert!(!option(&plugin, "EnableCombat"));
    assert!(world.chat().iter().any(|line| line.contains("You died!")));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheDeathRestoreVerbIsListed
#[test]
fn the_death_restore_verb_is_listed() {
    let (mut world, mut plugin) = standing_world();
    world.vt(&mut plugin, "help");
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("deathrestore"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheMetaIntervalIsSavedAndReadBackAsMossTanksOwnPreference
#[test]
fn the_meta_interval_is_saved_and_read_back_as_mosstanks_own_preference() {
    let (mut world, mut plugin) = standing_world();
    world.vt(&mut plugin, "metainterval");
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("every 293 ms")),
        "{:?}",
        world.chat()
    );

    world.vt(&mut plugin, "metainterval 100");
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("every 100 ms"))
    );

    world.disable(&mut plugin);
    let mut again = world.enable();
    world.clear_chat();
    world.vt(&mut again, "metainterval");
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("every 100 ms")),
        "{:?}",
        world.chat()
    );
    for key in world.profiles.keys() {
        if key.to_lowercase().ends_with(".usd") {
            assert!(
                !world
                    .profiles
                    .get(key)
                    .unwrap()
                    .to_lowercase()
                    .contains("metainterval")
            );
        }
    }
}

fn circuit(mode: Option<RouteMode>) -> NavigationSettings {
    let text = common::csharp_fixture("vtank/nav/nav_ab.nav");
    let mut route = NavigationSettings::default();
    mosstank_formats::nav::try_load(&text, &mut route, None).unwrap();
    if let Some(mode) = mode {
        route.mode = mode;
    }
    route
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: NextWaypointSkipsOneByDefaultOrTheCountGiven
#[test]
fn next_waypoint_skips_one_by_default_or_the_count_given() {
    let (mut world, mut plugin) = standing_world();
    world.profiles.insert(
        "mosstank/navs/Circuit.nav",
        &common::csharp_fixture("vtank/nav/nav_ab.nav"),
    );
    world.vt(&mut plugin, "nav load Circuit");
    let count =
        i32::try_from(plugin.tank().unwrap().settings().navigation.waypoints.len()).unwrap();
    assert!(count > 3);
    let start = index(&plugin);

    world.vt(&mut plugin, "nextwp");
    assert_eq!(index(&plugin), (start + 1) % count);
    world.vt(&mut plugin, "nextwp 2");
    assert_eq!(index(&plugin), (start + 3) % count);
    world.vt(&mut plugin, "nextwp zero");
    assert_eq!(index(&plugin), (start + 3) % count);
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("Syntax: /vt nextwp"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: PreviousWaypointStepsBackAndStopsAtTheFirst
#[test]
fn previous_waypoint_steps_back_and_stops_at_the_first() {
    let (mut world, mut plugin) = standing_world();
    world.profiles.insert(
        "mosstank/navs/Circuit.nav",
        &common::csharp_fixture("vtank/nav/nav_ab.nav"),
    );
    world.vt(&mut plugin, "nav load Circuit");
    world.clear_chat();
    world.vt(&mut plugin, "prevwp");
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("once-through"))
    );

    load_route(
        &mut world,
        &mut plugin,
        "CircuitRound",
        &circuit(Some(RouteMode::Circular)),
    );
    let count =
        i32::try_from(plugin.tank().unwrap().settings().navigation.waypoints.len()).unwrap();
    assert!(count > 3);
    let start = index(&plugin);
    world.vt(&mut plugin, "nextwp 3");
    let advanced = index(&plugin);
    assert_eq!(advanced, (start + 3) % count);

    world.vt(&mut plugin, "prevwp");
    assert_eq!(index(&plugin), advanced - 1);
    world.vt(&mut plugin, "prevwp 99");
    assert_eq!(index(&plugin), 0);
    world.clear_chat();
    world.vt(&mut plugin, "prevwp");
    assert_eq!(index(&plugin), 0);
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("already at its first waypoint"))
    );
    world.vt(&mut plugin, "prevwp zero");
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("Syntax: /vt prevwp"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: PreviousWaypointOnAnEmptyRouteSaysItHasNoWaypoints
#[test]
fn previous_waypoint_on_an_empty_route_says_it_has_no_waypoints() {
    let (mut world, mut plugin) = standing_world();
    assert!(
        plugin
            .tank()
            .unwrap()
            .settings()
            .navigation
            .waypoints
            .is_empty()
    );
    world.clear_chat();

    world.vt(&mut plugin, "prevwp");

    let chat = world.chat();
    assert!(
        chat.iter()
            .any(|line| line.contains("The route has no waypoints to step back to."))
    );
    assert!(
        !chat
            .iter()
            .any(|line| line.contains("already at its first waypoint"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: PreviousWaypointOnAReversedRouteStopsAtTheLastWaypoint
#[test]
fn previous_waypoint_on_a_reversed_route_stops_at_the_last_waypoint() {
    let (mut world, mut plugin) = standing_world();
    load_route(
        &mut world,
        &mut plugin,
        "CircuitRound",
        &circuit(Some(RouteMode::Circular)),
    );
    let count =
        i32::try_from(plugin.tank().unwrap().settings().navigation.waypoints.len()).unwrap();
    world.vt(&mut plugin, "reverseroute");

    world.vt(&mut plugin, "prevwp 99");
    assert_eq!(index(&plugin), count - 1);
    world.clear_chat();
    world.vt(&mut plugin, "prevwp");

    assert_eq!(index(&plugin), count - 1);
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("already at its last waypoint"))
    );
}

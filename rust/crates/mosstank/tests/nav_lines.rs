//! "Show Nav Lines": the loaded route drawn in the world through the
//! client's line layers (ported from the C#'s `MossTankPanelTests`).

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::navigation::NavigationSnapshot;
use common::World;
use common::automation::navigation_at;
use mosstank::MossTank;

/// One point at 12.5 E, 3.25 S (the C# test's `Legacy.nav`).
const LEGACY_NAV: &str = "uTank2 NAV 1.2\n4\n1\n0\n12.5\n-3.25\n0\n0";

/// Two points, a linear route.
const TWO_POINT_NAV: &str = "uTank2 NAV 1.2\n2\n2\n0\n12.5\n-3.25\n0\n0\n0\n12.6\n-3.25\n0\n0";

fn fresh() -> (World, MossTank) {
    let mut world = World::panel_character("Navigator");
    world.set_reply::<calls::NavigationSnapshot>(&navigation_at(0.0));
    let plugin = world.enable();
    (world, plugin)
}

/// A line as the client holds it.
struct Drawn {
    start: (f64, f64),
    follow_terrain: bool,
    width_metres: f32,
}

/// Every line the client would draw now, from every layer.
fn drawn(world: &World) -> Vec<Drawn> {
    world
        .fake
        .ui
        .merged_lines()
        .iter()
        .map(|line| Drawn {
            start: (line.start.east_west, line.start.north_south),
            follow_terrain: line.follow_terrain,
            width_metres: line.width_metres,
        })
        .collect()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ShowNavLinesDrawsTheLoadedRouteAndPersistsWithTheProfile
#[test]
fn show_nav_lines_draws_the_loaded_route_and_clears_it_when_turned_off() {
    let (mut world, mut plugin) = fresh();
    world.plugin_data.insert("imports/Legacy.nav", LEGACY_NAV);
    world.vt(&mut plugin, "nav load Legacy.nav");
    world.tick(&mut plugin);

    // Off by default: the plugin asks the client for nothing.
    assert!(!world.ui_bool(&mut plugin, "ShowNavLinesEnabled"));
    assert!(world.requests::<calls::UiLinesCreate>().is_empty());
    assert!(drawn(&world).is_empty());

    world.act(&mut plugin, "ToggleShowNavLines");
    assert!(world.ui_bool(&mut plugin, "ShowNavLinesEnabled"));
    assert_eq!(world.requests::<calls::UiLinesCreate>().len(), 1);
    // The one point is drawn as its arrival ring, on the ground outdoors.
    let lines = drawn(&world);
    assert_eq!(lines.len(), 24);
    assert!(lines.iter().all(|line| line.follow_terrain));
    assert!(
        lines
            .iter()
            .all(|line| (line.width_metres - 0.25).abs() < 1e-6)
    );
    // Around the point (the ring is at most 50 m, a fifth of a map unit).
    assert!(lines.iter().all(|line| {
        (line.start.0 - 12.5).abs() <= 50.0 / 240.0 + 1e-9
            && (line.start.1 + 3.25).abs() <= 50.0 / 240.0 + 1e-9
    }));

    world.act(&mut plugin, "ToggleShowNavLines");
    assert!(!world.ui_bool(&mut plugin, "ShowNavLinesEnabled"));
    assert!(drawn(&world).is_empty());
}

#[test]
fn the_lines_follow_the_route_frame_by_frame_and_set_only_on_a_change() {
    let (mut world, mut plugin) = fresh();
    world.vt(&mut plugin, "opt set shownavlines true");
    world.tick(&mut plugin);
    // No route yet: an empty layer.
    assert!(drawn(&world).is_empty());

    world.plugin_data.insert("imports/Two.nav", TWO_POINT_NAV);
    world.vt(&mut plugin, "nav load Two.nav");
    world.tick(&mut plugin);
    // Two rings and the leg between them.
    assert_eq!(drawn(&world).len(), 24 * 2 + 1);
    let sets = world.requests::<calls::UiLinesSet>().len();
    world.ticks(&mut plugin, 3);
    assert_eq!(world.requests::<calls::UiLinesSet>().len(), sets);

    // Indoors the lines keep their heights.
    let mut indoors = navigation_at(0.0);
    indoors.position.is_outdoor = false;
    world.set_reply::<calls::NavigationSnapshot>(&indoors);
    world.tick(&mut plugin);
    assert!(drawn(&world).iter().all(|line| !line.follow_terrain));

    // Portal space clears them.
    world.set_reply::<calls::NavigationSnapshot>(&NavigationSnapshot {
        is_portal_space: true,
        ..navigation_at(0.0)
    });
    world.tick(&mut plugin);
    assert!(drawn(&world).is_empty());
}

#[test]
fn stopping_the_plugin_clears_its_lines() {
    let (mut world, mut plugin) = fresh();
    world.plugin_data.insert("imports/Legacy.nav", LEGACY_NAV);
    world.vt(&mut plugin, "nav load Legacy.nav");
    world.vt(&mut plugin, "opt set shownavlines true");
    world.tick(&mut plugin);
    assert_eq!(drawn(&world).len(), 24);

    world.disable(&mut plugin);
    assert!(drawn(&world).is_empty());
}

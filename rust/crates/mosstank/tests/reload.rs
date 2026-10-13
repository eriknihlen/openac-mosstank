//! Reloading a meta and a route (ported from the C#'s
//! `MossTankPanelTests.Reload`): a meta's own route goes with its meta, and
//! picking a route again reads its file (or its meta's file) again.

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::navigation::NavigationSnapshot;
use ac_plugin_abi::world::Position;
use common::World;
use mosstank::MossTank;

const RELOAD_META: &str = "STATE: {Default} ~~ {\r\n\tIF:\tAlways\r\n\t\tDO:\tDoAll\r\n\t\t\t\tEmbedNav nav0 {stipend.nav}\r\n\t\t\t\tSetState {Walk}\r\nSTATE: {Walk} ~~ {\r\n\tIF:\tNever\r\n\t\tDO:\tNone\r\nNAV: nav0 once ~~ {\r\n\tpnt 47.1 26.1 0.2\r\n";

const ONE_POINT_ROUTE: &str = "NAV: nav0 circular ~~ {\r\n\tpnt 47.1 26.1 0.2\r\n~~ }\r\n";

fn reload_world() -> World {
    let mut world = World::with_character("Barris");
    world
        .profiles
        .insert("mosstank/metas/Reload.af", RELOAD_META);
    world
}

/// `meta load Reload`, the meta switched on, the macro started and one
/// pass: the meta's Default state loads its route and moves to Walk.
fn running_reload_meta(world: &mut World) -> MossTank {
    let mut plugin = world.enable();
    world.vt(&mut plugin, "meta load Reload");
    world.vt(&mut plugin, "opt set EnableMeta true");
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    tick(world, &mut plugin);
    plugin
}

fn tick(world: &mut World, plugin: &mut MossTank) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, 0.3));
}

fn rows(plugin: &MossTank) -> usize {
    plugin.tank().unwrap().route_waypoint_count()
}

fn meta_state(plugin: &MossTank) -> String {
    plugin.tank().unwrap().meta_state().to_owned()
}

fn selected_route(world: &mut World, plugin: &mut MossTank) -> String {
    world.with_tank(plugin, |tank, client| tank.selected_route_profile(client))
}

fn reloading_meta_discards_its_previous_embedded_navigation(dropdown: bool) {
    let mut world = reload_world();
    let mut plugin = running_reload_meta(&mut world);
    assert_eq!(rows(&plugin), 1);
    assert_eq!(meta_state(&plugin), "Walk");

    if dropdown {
        world.with_tank(&mut plugin, |tank, client| {
            tank.select_meta_profile(client, "Reload");
        });
    } else {
        world.vt(&mut plugin, "meta load Reload");
    }

    assert_eq!(meta_state(&plugin), "Default");
    assert_eq!(rows(&plugin), 0);
    let selected = selected_route(&mut world, &mut plugin);
    assert!(!selected.contains("(embedded)"), "{selected}");
    tick(&mut world, &mut plugin);
    assert_eq!(rows(&plugin), 1);
    assert_eq!(meta_state(&plugin), "Walk");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Reload.cs :: ReloadingMetaDiscardsItsPreviousEmbeddedNavigation
#[test]
fn reloading_meta_discards_its_previous_embedded_navigation_by_command() {
    reloading_meta_discards_its_previous_embedded_navigation(false);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Reload.cs :: ReloadingMetaDiscardsItsPreviousEmbeddedNavigation
#[test]
fn reloading_meta_discards_its_previous_embedded_navigation_from_the_picker() {
    reloading_meta_discards_its_previous_embedded_navigation(true);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Reload.cs :: ReselectingEmbeddedRouteReadsTheMetaFileAgain
#[test]
fn reselecting_embedded_route_reads_the_meta_file_again() {
    let mut world = reload_world();
    let mut plugin = running_reload_meta(&mut world);
    world.profiles.insert(
        "mosstank/metas/Reload.af",
        &format!("{RELOAD_META}\tpnt 48.1 27.1 0.2\r\n"),
    );

    let selected = selected_route(&mut world, &mut plugin);
    world.with_tank(&mut plugin, |tank, client| {
        tank.select_route_profile(client, &selected);
    });

    assert_eq!(rows(&plugin), 2);
    assert_eq!(meta_state(&plugin), "Walk");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Reload.cs :: ReselectingRouteReadsDiskWithoutSavingStaleContents
#[test]
fn reselecting_route_reads_disk_without_saving_stale_contents() {
    let mut world = World::with_character("Barris");
    world
        .profiles
        .insert("mosstank/navs/Reload.af", ONE_POINT_ROUTE);
    let mut plugin = world.enable();
    world.with_tank(&mut plugin, |tank, client| {
        tank.select_route_profile(client, "Reload.af");
    });
    let changed = format!("{ONE_POINT_ROUTE}\tpnt 48.1 27.1 0.2\r\n");
    world.profiles.insert("mosstank/navs/Reload.af", &changed);

    world.with_tank(&mut plugin, |tank, client| {
        tank.select_route_profile(client, "Reload.af");
    });

    assert_eq!(
        world.profiles.get("mosstank/navs/Reload.af"),
        Some(changed.as_str())
    );
    assert_eq!(rows(&plugin), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Reload.cs :: MissingEmbeddedSourceLeavesCurrentRouteAndReportsFailure
#[test]
fn missing_embedded_source_leaves_current_route_and_reports_failure() {
    let mut world = reload_world();
    let mut plugin = running_reload_meta(&mut world);
    world.profiles.remove("mosstank/metas/Reload.af");

    let selected = selected_route(&mut world, &mut plugin);
    world.with_tank(&mut plugin, |tank, client| {
        tank.select_route_profile(client, &selected);
    });

    assert_eq!(rows(&plugin), 1);
    assert_eq!(meta_state(&plugin), "Walk");
    let notice = plugin.tank().unwrap().route_notice().to_owned();
    assert!(notice.contains("Could not reload"), "{notice}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Reload.cs :: EmbeddedOnceRouteStartsAtItsHeadInsteadOfTheOldCircularAnchor
#[test]
fn embedded_once_route_starts_at_its_head_instead_of_the_old_circular_anchor() {
    let mut world = reload_world();
    world.profiles.insert(
        "mosstank/navs/Old.af",
        "NAV: nav0 circular ~~ {\r\n\tpnt 20 20 0\r\n\tpnt 10 10 0\r\n\tpnt 0 0 0\r\n",
    );
    // `NavigationAt(0f)`.
    world.set_reply::<calls::NavigationSnapshot>(&NavigationSnapshot {
        is_available: true,
        local_object_id: 1,
        position: Position {
            cell_id: 0x0001_0001,
            is_outdoor: true,
            ..Position::default()
        },
        ..NavigationSnapshot::default()
    });
    let mut plugin = world.enable();
    world.vt(&mut plugin, "nav load Old.af");
    world.vt(&mut plugin, "meta load Reload");
    world.vt(&mut plugin, "opt set EnableMeta true");
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    assert_eq!(plugin.tank().unwrap().route_waypoint_index(), 2);

    tick(&mut world, &mut plugin);

    assert_eq!(meta_state(&plugin), "Walk");
    assert_eq!(plugin.tank().unwrap().route_waypoint_index(), 0);
    // The route editor's colours (no point marked out of reach) come with
    // the panel (MT-11).
    let chat = world.chat();
    assert!(
        chat.contains(&"[MossTank] Loaded stipend.nav (embedded) (1 points).".to_owned()),
        "{chat:#?}"
    );
    assert!(chat.contains(&"[MossTank] Meta state: Default -> Walk.".to_owned()));
    assert!(
        chat.iter()
            .any(|line| line.starts_with("[MossTank] Loaded route Old ("))
    );
    assert!(
        chat.iter().all(|line| line.starts_with("[MossTank]")),
        "{chat:#?}"
    );
    assert!(
        !chat
            .iter()
            .any(|line| line.starts_with("[MossTank] [MossTank]"))
    );
}

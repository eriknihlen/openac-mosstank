//! The profile pickers' delete and copy buttons, the loot classifier menu
//! and the once-route rows, driven through the window bindings (ported from
//! the C#'s `MossTankPanelTests`).

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::items::ClassifierInfo;
use ac_plugin_abi::navigation::NavigationSnapshot;
use ac_plugin_abi::world::Position;
use common::World;
use mosstank::MossTank;
use mosstank_expr::ExpressionValue;
use mosstank_stores::directory;

fn fresh(name: &str) -> (World, MossTank) {
    let mut world = World::panel_character(name);
    let plugin = world.enable();
    (world, plugin)
}

fn contains(list: &[String], name: &str) -> bool {
    list.iter().any(|entry| entry == name)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: LootClassifierSelectionIsVisibleAndPersistsWithMacroProfile
#[test]
fn the_loot_classifier_choice_shows_and_persists_with_the_macro_profile() {
    let mut world = World::panel_character("Looter");
    world.set_reply::<calls::LootClassifiers>(&vec![ClassifierInfo {
        id: "utility/loot".to_owned(),
        display_name: "Utility Loot".to_owned(),
    }]);
    let mut plugin = world.enable();
    let p = &mut plugin;
    assert!(contains(
        &world.ui_list(p, "LootClassifierNames"),
        "Utility Loot [utility/loot]"
    ));
    world.act_text(p, "SelectLootClassifier", "Utility Loot [utility/loot]");
    assert_eq!(
        world.ui_text(p, "SelectedLootClassifier"),
        "Utility Loot [utility/loot]"
    );
    world.disable(p);
    let mut restored = world.enable();
    let p = &mut restored;
    assert_eq!(
        world.ui_text(p, "SelectedLootClassifier"),
        "Utility Loot [utility/loot]"
    );
    world.act_text(p, "SelectLootClassifier", "VTClassic");
    assert_eq!(world.ui_text(p, "SelectedLootClassifier"), "VTClassic");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: NamedProfileCopyHotLoadsWithoutMixingByCharacterSettings
#[test]
fn a_named_profile_copy_hot_loads_without_mixing_by_character_settings() {
    let (mut world, mut plugin) = fresh("Moss Wart");
    let p = &mut plugin;
    world.act_float(p, "SetNormalHealth", 0.42);
    world.vt(p, "settings save Fellowship");
    let fellowship = "--Moss Wart__Fellowship.usd";
    assert_eq!(world.ui_text(p, "SelectedMacroProfile"), fellowship);
    assert!(contains(&world.ui_list(p, "MacroProfileNames"), fellowship));
    world.act_float(p, "SetNormalHealth", 0.88);
    world.act_text(p, "SelectMacroProfile", "By char");
    assert!((world.ui_float(p, "NormalHealthValue") - 0.42).abs() < 0.005);
    world.act_text(p, "SelectMacroProfile", "Fellowship");
    assert_eq!(world.ui_text(p, "SelectedMacroProfile"), fellowship);
    assert!((world.ui_float(p, "NormalHealthValue") - 0.88).abs() < 0.005);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: DeleteProfileRemovesTheRealFileAndFallsBackToByCharacter
#[test]
fn delete_profile_removes_the_file_and_falls_back_to_by_character() {
    let (mut world, mut plugin) = fresh("Moss Wart");
    let p = &mut plugin;
    world.act_float(p, "SetNormalHealth", 0.42);
    world.vt(p, "settings save Fellowship");
    let fellowship = "--Moss Wart__Fellowship.usd";
    assert_eq!(world.ui_text(p, "SelectedMacroProfile"), fellowship);
    world.act(p, "DeleteProfile");
    assert_eq!(world.ui_text(p, "SelectedMacroProfile"), "By char");
    assert!(!contains(
        &world.ui_list(p, "MacroProfileNames"),
        fellowship
    ));
    assert!(!world.profiles.keys().any(|key| key.ends_with(fellowship)));
    assert!(
        world
            .ui_text(p, "ProfileLifecycleNotice")
            .contains("Deleted")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: DeleteProfileRefusesToRemoveByCharacter
#[test]
fn delete_profile_refuses_the_by_character_profile() {
    let (mut world, mut plugin) = fresh("Keeper");
    let p = &mut plugin;
    world.act(p, "DeleteProfile");
    assert_eq!(world.ui_text(p, "SelectedMacroProfile"), "By char");
    assert!(
        world
            .ui_text(p, "ProfileLifecycleNotice")
            .contains("cannot be deleted")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: DeleteRouteProfileRemovesTheRealFileAndFallsBackToByCharacter
#[test]
fn delete_route_profile_removes_the_file_and_falls_back() {
    let (mut world, mut plugin) = fresh("Router");
    let p = &mut plugin;
    world.vt(p, "nav save Fellowship");
    assert_eq!(world.ui_text(p, "SelectedRouteProfile"), "Fellowship");
    assert!(contains(
        &world.ui_list(p, "RouteProfileNames"),
        "Fellowship"
    ));
    assert!(world.profiles.contains("mosstank/navs/Fellowship.nav"));
    world.act(p, "DeleteRouteProfile");
    assert_eq!(world.ui_text(p, "SelectedRouteProfile"), "By char");
    assert!(!contains(
        &world.ui_list(p, "RouteProfileNames"),
        "Fellowship"
    ));
    assert!(!world.profiles.contains("mosstank/navs/Fellowship.nav"));
    assert!(world.ui_text(p, "RouteNotice").contains("Deleted"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: DeleteRouteProfileRefusesToRemoveByCharacter
#[test]
fn delete_route_profile_refuses_the_by_character_route() {
    let (mut world, mut plugin) = fresh("Router");
    let p = &mut plugin;
    world.act(p, "DeleteRouteProfile");
    assert_eq!(world.ui_text(p, "SelectedRouteProfile"), "By char");
    assert!(
        world
            .ui_text(p, "RouteNotice")
            .contains("cannot be deleted")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: DeleteMetaProfileRemovesTheRealFileAndFallsBackToByCharacter
#[test]
fn delete_meta_profile_removes_the_file_and_falls_back() {
    let (mut world, mut plugin) = fresh("Metaman");
    let p = &mut plugin;
    world.vt(p, "meta save Fellowship");
    assert_eq!(world.ui_text(p, "SelectedMetaProfile"), "Fellowship");
    assert!(contains(
        &world.ui_list(p, "MetaProfileNames"),
        "Fellowship"
    ));
    assert!(world.profiles.contains("mosstank/metas/Fellowship.met"));
    world.act(p, "DeleteMetaProfile");
    assert_eq!(world.ui_text(p, "SelectedMetaProfile"), "By char");
    assert!(!contains(
        &world.ui_list(p, "MetaProfileNames"),
        "Fellowship"
    ));
    assert!(!world.profiles.contains("mosstank/metas/Fellowship.met"));
    assert!(world.ui_text(p, "MetaNotice").contains("Deleted"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: DeleteMetaProfileRefusesToRemoveByCharacter
#[test]
fn delete_meta_profile_refuses_the_by_character_meta() {
    let (mut world, mut plugin) = fresh("Metaman");
    let p = &mut plugin;
    world.act(p, "DeleteMetaProfile");
    assert_eq!(world.ui_text(p, "SelectedMetaProfile"), "By char");
    assert!(world.ui_text(p, "MetaNotice").contains("cannot be deleted"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: DeleteLootProfileRemovesTheRealFileAndFallsBackToByCharacter
#[test]
fn delete_loot_profile_removes_the_file_and_falls_back() {
    let (mut world, mut plugin) = fresh("Looter");
    let p = &mut plugin;
    world.vt(p, "loot new Fellowship");
    assert_eq!(world.ui_text(p, "LootProfileName"), "Fellowship");
    assert!(contains(
        &world.ui_list(p, "LootProfileNames"),
        "Fellowship"
    ));
    world.act(p, "DeleteLootProfile");
    assert_eq!(world.ui_text(p, "LootProfileName"), "By char");
    assert!(!contains(
        &world.ui_list(p, "LootProfileNames"),
        "Fellowship"
    ));
    assert!(!world.profiles.contains("mosstank/loot/Fellowship.utl"));
    assert!(world.ui_text(p, "LootEditorNotice").contains("Deleted"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: DeleteNamedLootProfileDoesNotActivateOrOverwriteMalformedByCharacterFallback
#[test]
fn deleting_a_named_loot_profile_does_not_activate_a_malformed_fallback() {
    let mut world = World::with_character("Looter");
    let by_character = format!(
        "mosstank/loot/{}",
        directory::auto_character_file_name("Looter", "Coldeve", "utl")
    );
    let malformed = "UTL\r\n1\r\n1\r\nunfinished";
    let mut plugin = world.enable();
    let p = &mut plugin;
    world.profiles.insert(&by_character, malformed);
    world.vt(p, "loot new Named");
    world.act(p, "AddLootRule");
    world.act(p, "DeleteLootProfile");
    world.act(p, "ToggleLootPriorityBoost");
    assert_eq!(
        world.ui_text(p, "LootProfileName"),
        mosstank_stores::loot_store::NO_ACTIVE_PROFILE
    );
    assert!(world.ui_list(p, "LootRuleRows").is_empty());
    assert_eq!(world.profiles.get(&by_character), Some(malformed));
    assert!(
        world
            .ui_text(p, "LootEditorNotice")
            .contains("could not be read")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: DeleteLootProfileRefusesToRemoveByCharacter
#[test]
fn delete_loot_profile_refuses_the_by_character_profile() {
    let (mut world, mut plugin) = fresh("Looter");
    let p = &mut plugin;
    world.act(p, "DeleteLootProfile");
    assert_eq!(world.ui_text(p, "LootProfileName"), "By char");
    assert!(
        world
            .ui_text(p, "LootEditorNotice")
            .contains("cannot be deleted")
    );
}

// ---- once routes -------------------------------------------------------------

fn standing(world: &mut World) {
    world.set_reply::<calls::NavigationSnapshot>(&NavigationSnapshot {
        is_available: true,
        local_object_id: 1,
        position: Position {
            cell_id: 0x7F7F_0001,
            is_outdoor: true,
            ..Position::default()
        },
        ..NavigationSnapshot::default()
    });
}

fn point_lines(world: &World, key: &str) -> usize {
    world
        .profiles
        .get(key)
        .unwrap_or("")
        .split('\n')
        .filter(|line| line.trim_start().starts_with("pnt"))
        .count()
}

fn route_with_three_points(mode: &str) -> (World, MossTank) {
    let mut world = World::panel_character("Runner");
    standing(&mut world);
    let mut plugin = world.enable();
    world.act_text(&mut plugin, "SelectRouteMode", mode);
    for _ in 0..3 {
        world.act(&mut plugin, "AddRoutePoint");
    }
    (world, plugin)
}

fn run(world: &mut World, plugin: &mut MossTank) {
    world
        .with_tank(plugin, |tank, client| {
            tank.set_meta_option(client, "EnableNav", &ExpressionValue::Boolean(true))
        })
        .unwrap();
    world.act(plugin, "ToggleCombat");
}

fn tick(world: &mut World, plugin: &mut MossTank) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, 0.3));
}

fn cursor(plugin: &MossTank) -> i32 {
    plugin.tank().unwrap().route_waypoint_index()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: RunningAOnceRouteLeavesItsProfileOnDiskWhole
#[test]
fn running_a_once_route_leaves_its_file_whole() {
    let (mut world, mut plugin) = route_with_three_points("Once");
    let p = &mut plugin;
    world.vt(p, "navaf save OnceRun");
    let file = "mosstank/navs/OnceRun.af";
    assert_eq!(point_lines(&world, file), 3);
    run(&mut world, p);
    tick(&mut world, p);
    assert_eq!(cursor(p), 1);
    world.act(p, "ToggleOpenDoors");
    assert_eq!(point_lines(&world, file), 3);
    for _ in 0..6 {
        tick(&mut world, p);
    }
    assert_eq!(world.ui_text(p, "NavigationStatus"), "Once route complete.");
    assert_eq!(world.ui_list(p, "RouteRows").len(), 3);
    assert_eq!(point_lines(&world, file), 3);
    world.act_text(p, "SelectRouteProfile", "OnceRun");
    assert_eq!(world.ui_list(p, "RouteRows").len(), 3);
    assert_eq!(cursor(p), 0);
    assert_eq!(point_lines(&world, file), 3);
}

const ROW: u32 = 0x00E8_DEC3;
const SPENT: u32 = 0x00C0_3838;

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AOnceRoutePaintsTheWaypointsItHasRunAndAReloadClearsThem
#[test]
fn a_once_route_paints_the_points_it_has_run_and_a_reload_clears_them() {
    let (mut world, mut plugin) = route_with_three_points("Once");
    let p = &mut plugin;
    world.vt(p, "nav save OnceColours");
    assert!(
        world
            .ui_uints(p, "RouteWaypointColors")
            .iter()
            .all(|c| *c == ROW)
    );
    run(&mut world, p);
    tick(&mut world, p);
    assert_eq!(cursor(p), 1);
    assert_eq!(world.ui_uints(p, "RouteWaypointColors"), [SPENT, ROW, ROW]);
    tick(&mut world, p);
    tick(&mut world, p);
    assert_eq!(cursor(p), 3);
    assert_eq!(
        world.ui_uints(p, "RouteWaypointColors"),
        [SPENT, SPENT, SPENT]
    );
    world.act_text(p, "SelectRouteProfile", "OnceColours");
    assert_eq!(world.ui_uints(p, "RouteWaypointColors"), [ROW, ROW, ROW]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ACircularRouteNeverPaintsAWaypointAsSpent
#[test]
fn a_circular_route_never_paints_a_point_as_spent() {
    let (mut world, mut plugin) = route_with_three_points("Circular");
    let p = &mut plugin;
    run(&mut world, p);
    for _ in 0..4 {
        tick(&mut world, p);
    }
    assert!(
        world
            .ui_uints(p, "RouteWaypointColors")
            .iter()
            .all(|c| *c == ROW)
    );
}

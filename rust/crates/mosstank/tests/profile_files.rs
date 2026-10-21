//! Which file a route or meta name reaches, and which files a follow
//! writes (ported from the C#'s `MossTankPanelTests.ProfileFiles` and the
//! follow half of `.Remote`). The window's own buttons (New, Copy to .af,
//! the mode menu, the item list) come with the panel.

mod common;

use std::collections::BTreeMap;

use ac_plugin_abi::calls;
use ac_plugin_abi::navigation::NavigationSnapshot;
use ac_plugin_abi::social::SelectionState;
use ac_plugin_abi::world::{ObjectClass, Position, WorldObject};
use common::World;
use mosstank::MossTank;
use mosstank_formats::fixtures;
use mosstank_formats::route::RouteMode;
use mosstank_stores::directory;
use mosstank_stores::storage::MemoryHost;

const ONE_POINT_ROUTE: &str = "NAV: nav0 circular ~~ {\r\n\tpnt 47.1 26.1 0.2\r\n~~ }\r\n";

const ONE_RULE_META: &str = "STATE: {Default} ~~ {\r\n\tIF:\tNever\r\n\t\tDO:\tNone\r\n";

const FOLLOW_ROUTE_KEY: &str = "mosstank/navs/UBFollow.af";

fn dropped_route() -> String {
    fixtures::read_vtank("nav/nav_ab.nav")
}

fn dropped_meta() -> String {
    fixtures::read_vtank("met/bella.met")
}

fn barris(files: &[(&str, &str)]) -> (World, MossTank) {
    let mut world = World::with_character("Barris");
    for (key, text) in files {
        world.profiles.insert(key, text);
    }
    let plugin = world.enable();
    (world, plugin)
}

fn rows(plugin: &MossTank) -> usize {
    plugin.tank().unwrap().route_waypoint_count()
}

fn meta_rows(plugin: &MossTank) -> usize {
    plugin.tank().unwrap().meta_rule_count()
}

fn selected_route(world: &mut World, plugin: &mut MossTank) -> String {
    world.with_tank(plugin, |tank, client| tank.selected_route_profile(client))
}

fn selected_meta(world: &mut World, plugin: &mut MossTank) -> String {
    world.with_tank(plugin, |tank, client| tank.selected_meta_profile(client))
}

fn route_names(world: &mut World, plugin: &mut MossTank) -> Vec<String> {
    world.with_tank(plugin, |tank, client| tank.route_profile_names(client))
}

fn meta_names(world: &mut World, plugin: &mut MossTank) -> Vec<String> {
    world.with_tank(plugin, |tank, client| tank.meta_profile_names(client))
}

fn file(world: &World, key: &str) -> String {
    world
        .profiles
        .get(key)
        .unwrap_or_else(|| panic!("{key}"))
        .to_owned()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: ANavLoadByBareNameLoadsTheNavFirstAndAnExtensionLoadsThatFile
#[test]
fn a_nav_load_by_bare_name_loads_the_nav_first_and_an_extension_loads_that_file() {
    let (mut world, mut plugin) = barris(&[
        ("mosstank/navs/foo.nav", &dropped_route()),
        ("mosstank/navs/foo.af", ONE_POINT_ROUTE),
    ]);

    world.vt(&mut plugin, "nav load foo");
    assert_eq!(selected_route(&mut world, &mut plugin), "foo");
    assert!(rows(&plugin) > 1);

    world.vt(&mut plugin, "nav load foo.af");
    assert_eq!(selected_route(&mut world, &mut plugin), "foo.af");
    assert_eq!(rows(&plugin), 1);
    let names = route_names(&mut world, &mut plugin);
    assert!(names.contains(&"foo.af".to_owned()), "{names:?}");
    assert!(names.contains(&"foo".to_owned()));
    assert!(!names.contains(&"foo.nav".to_owned()));

    world.vt(&mut plugin, "nav load foo.nav");
    assert!(rows(&plugin) > 1);
    let af: Vec<String> = names
        .into_iter()
        .filter(|name| name.ends_with(".af"))
        .collect();
    assert_eq!(af.len(), 1);
    world.vt(&mut plugin, &format!("nav load {}", af[0]));
    assert_eq!(rows(&plugin), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: ANavLoadFallsBackToTheFormThatIsThere
#[test]
fn a_nav_load_falls_back_to_the_form_that_is_there() {
    let (mut world, mut plugin) = barris(&[("mosstank/navs/foo.af", ONE_POINT_ROUTE)]);

    world.vt(&mut plugin, "nav load foo");
    assert_eq!(selected_route(&mut world, &mut plugin), "foo");
    assert_eq!(rows(&plugin), 1);

    world.vt(&mut plugin, "nav load foo.nav");
    assert_eq!(selected_route(&mut world, &mut plugin), "foo");
    assert!(route_names(&mut world, &mut plugin).contains(&"foo".to_owned()));
}

/// The route editor's Remove is the panel's; here the store saves the edited
/// route as the editor's save does.
// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: EditingARouteLoadedFromANavSavesItBackIntoTheNav
#[test]
fn editing_a_route_loaded_from_a_nav_saves_it_back_into_the_nav() {
    let mut host = MemoryHost::on("Coldeve");
    host.profiles
        .insert("mosstank/navs/foo.nav", &dropped_route());
    let mut store = mosstank_stores::route_store::RouteProfileStore::new();
    store.bind_character(&mut host, Some("Barris")).unwrap();
    assert!(store.select(&mut host, Some("foo"), false).unwrap());
    let mut route = mosstank_formats::route::NavigationSettings::default();
    store
        .load_current(
            &mut host,
            &mut route,
            Some(&mosstank_formats::route::NoSpells),
        )
        .unwrap();
    let points = route.waypoints.len();

    route.waypoints.remove(0);
    store.save_current(&mut host, &route);

    assert!(!host.profiles.contains("mosstank/navs/foo.af"));
    assert!(
        host.profiles
            .get("mosstank/navs/foo.nav")
            .unwrap()
            .starts_with("uTank2 NAV 1.2\r\n")
    );
    assert_eq!(store.selected(&mut host).unwrap(), "foo");
    let mut reloaded = mosstank_formats::route::NavigationSettings::default();
    store
        .load_current(
            &mut host,
            &mut reloaded,
            Some(&mosstank_formats::route::NoSpells),
        )
        .unwrap();
    assert_eq!(reloaded.waypoints.len(), points - 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: SwitchingFromANavRouteLeavesTheAfBesideItAlone
#[test]
fn switching_from_a_nav_route_leaves_the_af_beside_it_alone() {
    let original = dropped_route();
    let (mut world, mut plugin) = barris(&[
        ("mosstank/navs/foo.nav", &original),
        ("mosstank/navs/foo.af", ONE_POINT_ROUTE),
    ]);
    world.vt(&mut plugin, "nav load foo");

    world.with_tank(&mut plugin, |tank, client| {
        tank.select_route_profile(client, "foo.af");
    });

    assert_eq!(rows(&plugin), 1);
    assert_eq!(file(&world, "mosstank/navs/foo.nav"), original);
    assert_eq!(file(&world, "mosstank/navs/foo.af"), ONE_POINT_ROUTE);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: AMetaLoadByBareNameLoadsTheMetFirstAndAnExtensionLoadsThatFile
#[test]
fn a_meta_load_by_bare_name_loads_the_met_first_and_an_extension_loads_that_file() {
    let (mut world, mut plugin) = barris(&[
        ("mosstank/metas/foo.met", &dropped_meta()),
        ("mosstank/metas/foo.af", ONE_RULE_META),
    ]);

    world.vt(&mut plugin, "meta load foo");
    assert_eq!(selected_meta(&mut world, &mut plugin), "foo");
    assert!(meta_rows(&plugin) > 1);

    world.vt(&mut plugin, "meta load foo.af");
    assert_eq!(selected_meta(&mut world, &mut plugin), "foo.af");
    assert_eq!(meta_rows(&plugin), 1);
    let names = meta_names(&mut world, &mut plugin);
    assert!(names.contains(&"foo.af".to_owned()), "{names:?}");
    assert!(names.contains(&"foo".to_owned()));
    assert!(!names.contains(&"foo.met".to_owned()));

    world.vt(&mut plugin, "meta load foo.met");
    assert!(meta_rows(&plugin) > 1);
    world.with_tank(&mut plugin, |tank, client| {
        tank.select_meta_profile(client, "foo.af");
    });
    assert_eq!(meta_rows(&plugin), 1);
}

fn translate_world() -> (World, MossTank) {
    let mut world = World::new();
    world
        .profiles
        .insert("mosstank/navs/eo-east.nav", &dropped_route());
    world
        .profiles
        .insert("mosstank/navs/eo-east.af", ONE_POINT_ROUTE);
    let plugin = world.enable();
    (world, plugin)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: TranslateRouteReadsTheFileItsExtensionNames
#[test]
fn translate_route_reads_the_file_its_extension_names() {
    let (mut world, mut plugin) = translate_world();

    world.command(
        &mut plugin,
        "ub",
        "translateroute 0x00640371 eo-east.af 0x01640371 eo-main",
    );

    let saved = file(&world, "mosstank/navs/eo-main.nav");
    assert!(!saved.contains("/ah"));
    assert!(saved.contains("26.1"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: TranslateRouteByBareNameReadsTheNavFirst
#[test]
fn translate_route_by_bare_name_reads_the_nav_first() {
    let (mut world, mut plugin) = translate_world();

    world.command(
        &mut plugin,
        "ub",
        "translateroute 0x00640371 eo-east 0x01640371 eo-main",
    );

    let saved = file(&world, "mosstank/navs/eo-main.nav");
    assert!(saved.contains("/ah"));
    assert!(!saved.contains("26.1"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: SavingByABareNameWritesTheNavAndTheMetForm
#[test]
fn saving_by_a_bare_name_writes_the_nav_and_the_met_form() {
    let (mut world, mut plugin) = barris(&[("mosstank/navs/start.af", ONE_POINT_ROUTE)]);
    world.vt(&mut plugin, "nav load start");

    world.vt(&mut plugin, "nav save Foo");
    world.vt(&mut plugin, "meta save Foo");

    assert!(file(&world, "mosstank/navs/Foo.nav").starts_with("uTank2 NAV 1.2\r\n"));
    assert!(file(&world, "mosstank/metas/Foo.met").starts_with("1\r\nCondAct\r\n"));
    assert!(!world.profiles.contains("mosstank/navs/Foo.af"));
    assert!(!world.profiles.contains("mosstank/metas/Foo.af"));
    assert_eq!(selected_route(&mut world, &mut plugin), "Foo");
    assert_eq!(selected_meta(&mut world, &mut plugin), "Foo");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: TheAfCommandsSaveAndLoadTheAfOnly
#[test]
fn the_af_commands_save_and_load_the_af_only() {
    let (mut world, mut plugin) = barris(&[
        ("mosstank/navs/foo.nav", &dropped_route()),
        ("mosstank/metas/foo.met", &dropped_meta()),
    ]);
    world.vt(&mut plugin, "nav load foo");
    world.vt(&mut plugin, "meta load foo");
    let points = rows(&plugin);

    world.vt(&mut plugin, "navaf save foo.nav");
    world.vt(&mut plugin, "metaaf save foo");

    assert!(file(&world, "mosstank/navs/foo.af").contains("NAV: "));
    assert!(file(&world, "mosstank/metas/foo.af").contains("STATE: "));
    assert_eq!(selected_route(&mut world, &mut plugin), "foo.af");
    assert_eq!(selected_meta(&mut world, &mut plugin), "foo.af");

    world
        .profiles
        .insert("mosstank/navs/foo.af", ONE_POINT_ROUTE);
    world.vt(&mut plugin, "nav load foo");
    assert_eq!(rows(&plugin), points);
    world.vt(&mut plugin, "navaf load foo");
    assert_eq!(selected_route(&mut world, &mut plugin), "foo.af");
    assert_eq!(rows(&plugin), 1);

    world.clear_chat();
    world
        .profiles
        .insert("mosstank/navs/elsewhere.nav", &dropped_route());
    world
        .profiles
        .insert("mosstank/metas/elsewhere.met", &dropped_meta());
    world.vt(&mut plugin, "navaf load elsewhere");
    world.vt(&mut plugin, "metaaf load elsewhere");
    let chat = world.chat();
    assert!(chat.contains(&"[MossTank] Navigation profile elsewhere.af was not found.".to_owned()));
    assert!(chat.contains(&"[MossTank] Meta profile elsewhere.af was not found.".to_owned()));
    assert_eq!(selected_route(&mut world, &mut plugin), "foo.af");
    assert!(!world.profiles.contains("mosstank/metas/elsewhere.af"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: TheHelpDocumentsTheAfCommands
#[test]
fn the_help_documents_the_af_commands() {
    let (mut world, mut plugin) = barris(&[]);

    world.vt(&mut plugin, "help navaf");
    world.vt(&mut plugin, "help metaaf");

    let chat = world.chat();
    assert!(chat.contains(&"[MossTank] Syntax: /vt navaf [save/load] [filename]".to_owned()));
    assert!(chat.contains(&"[MossTank] Syntax: /vt metaaf [save/load] [filename]".to_owned()));
}

// ---- the follow route ------------------------------------------------------

fn player(object_id: u32, name: &str) -> WorldObject {
    WorldObject {
        object_id,
        name: name.to_owned(),
        object_class: ObjectClass::Player,
        ..WorldObject::default()
    }
}

/// Barris on Coldeve, standing at the origin, with Zero Cool in view and a
/// route file in place (`PanelWithRoute`).
fn world_with_route(key: &str, text: &str) -> (World, MossTank) {
    let mut world = World::with_character("Barris");
    world.profiles.insert(key, text);
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
    world.set_reply::<calls::WorldObjects>(&vec![player(0x5000_0ABC, "Zero Cool")]);
    let plugin = world.enable();
    (world, plugin)
}

/// The route files, by key (`RouteFiles`).
fn route_files(world: &World) -> BTreeMap<String, String> {
    world
        .profiles
        .keys()
        .filter(|key| key.starts_with("mosstank/navs/"))
        .map(|key| (key.to_owned(), world.profiles.get(key).unwrap().to_owned()))
        .collect()
}

fn mode(plugin: &MossTank) -> RouteMode {
    plugin.tank().unwrap().settings().navigation.mode
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: UbFollowAimsTheFollowRouteAndLeavesTheLoadedRouteFileAlone
#[test]
fn ub_follow_aims_the_follow_route_and_leaves_the_loaded_route_file_alone() {
    let (mut world, mut plugin) = world_with_route("mosstank/navs/jaw_1.af", ONE_POINT_ROUTE);
    world.vt(&mut plugin, "nav load jaw_1");
    let before = route_files(&world);

    world.command(&mut plugin, "ub", "follow Zero Cool");

    assert_eq!(file(&world, "mosstank/navs/jaw_1.af"), ONE_POINT_ROUTE);
    assert_eq!(selected_route(&mut world, &mut plugin), "UBFollow");
    assert_eq!(mode(&plugin), RouteMode::Target);
    assert!(file(&world, FOLLOW_ROUTE_KEY).contains("flw 50000ABC"));
    let mut after = route_files(&world);
    after.remove(FOLLOW_ROUTE_KEY);
    assert_eq!(before, after);

    // Back on the route, it is the route it was.
    world.vt(&mut plugin, "nav load jaw_1");
    assert_eq!(rows(&plugin), 1);
    assert_eq!(mode(&plugin), RouteMode::Circular);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: UbFollowWritesNothingBesideARouteLoadedFromANav
#[test]
fn ub_follow_writes_nothing_beside_a_route_loaded_from_a_nav() {
    let original = dropped_route();
    let (mut world, mut plugin) = world_with_route("mosstank/navs/jaw.nav", &original);
    world.vt(&mut plugin, "nav load jaw.nav");
    let before = route_files(&world);

    world.command(&mut plugin, "ub", "follow Zero Cool");

    let mut after = route_files(&world);
    assert!(after.remove(FOLLOW_ROUTE_KEY).is_some());
    assert_eq!(before, after);
    assert!(!world.profiles.contains("mosstank/navs/jaw.af"));
    assert_eq!(file(&world, "mosstank/navs/jaw.nav"), original);
}

/// What the macro's switches are (the Remote test's tuple).
fn switches(plugin: &MossTank) -> [bool; 6] {
    let tank = plugin.tank().unwrap();
    [
        tank.macro_running(),
        tank.option("EnableCombat").is_truthy(),
        tank.option("EnableNav").is_truthy(),
        tank.option("EnableLooting").is_truthy(),
        tank.option("EnableBuffing").is_truthy(),
        tank.meta_enabled(),
    ]
}

fn select(world: &mut World, object_id: u32) {
    world.set_reply::<calls::SelectionState>(&SelectionState {
        selected: (object_id != 0).then_some(object_id),
        previous: None,
    });
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Remote.cs :: RemoteFollowPreservesTheLoadedRouteAndMacroSwitches
#[test]
fn remote_follow_preserves_the_loaded_route_and_macro_switches() {
    let (mut world, mut plugin) = world_with_route("mosstank/navs/jaw.nav", &dropped_route());
    world.vt(&mut plugin, "nav load jaw.nav");
    select(&mut world, 0x5000_0ABC);
    let before = route_files(&world);
    let original_rows = rows(&plugin);
    let original_points = plugin
        .tank()
        .unwrap()
        .settings()
        .navigation
        .waypoints
        .clone();
    let switches_before = switches(&plugin);

    world.with_tank(&mut plugin, |tank, client| {
        tank.remote_follow_selected(client)
    });

    assert_eq!(selected_route(&mut world, &mut plugin), "UBFollow");
    assert_eq!(
        plugin
            .tank()
            .unwrap()
            .settings()
            .navigation
            .follow_target_name,
        "Zero Cool"
    );
    let mut after = route_files(&world);
    assert!(after.remove(FOLLOW_ROUTE_KEY).is_some());
    assert_eq!(before, after);
    assert_eq!(switches(&plugin), switches_before);
    world.vt(&mut plugin, "nav load jaw.nav");
    assert_eq!(rows(&plugin), original_rows);
    assert_eq!(
        plugin.tank().unwrap().settings().navigation.waypoints,
        original_points
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Remote.cs :: RemoteFollowRejectsMissingOrNonPlayerSelection
#[test]
fn remote_follow_rejects_missing_or_non_player_selection() {
    for selected in [0, 123, 1, 0x5000_0ABC] {
        let (mut world, mut plugin) = world_with_route("mosstank/navs/jaw_1.af", ONE_POINT_ROUTE);
        world.vt(&mut plugin, "nav load jaw_1");
        // The character itself is object 1.
        let mut me = common::character("Barris");
        me.id = 1;
        world.fake.character = Some(me);
        world.set_reply::<calls::WorldObjects>(&vec![
            WorldObject {
                object_id: 0x5000_0ABC,
                name: "Not a player".to_owned(),
                object_class: ObjectClass::Monster,
                ..WorldObject::default()
            },
            player(1, "Self"),
        ]);
        select(&mut world, selected);
        let before = route_files(&world);

        world.with_tank(&mut plugin, |tank, client| {
            tank.remote_follow_selected(client)
        });

        assert_eq!(
            selected_route(&mut world, &mut plugin),
            "jaw_1",
            "{selected:#x}"
        );
        assert_eq!(rows(&plugin), 1);
        assert_eq!(route_files(&world), before);
        assert!(
            world
                .chat()
                .iter()
                .any(|line| line.contains("Select a live player")),
            "{selected:#x}"
        );
    }
}

// ---- the stores ---------------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: EveryProfileStoreRebindsWhenServerArrivesOrChanges
#[test]
fn every_profile_store_rebinds_when_server_arrives_or_changes() {
    let mut host = MemoryHost::on("");
    let mut macro_store = mosstank_stores::profile_store::ProfileStore::new(&mut host);
    let mut loot = mosstank_stores::loot_store::LootProfileStore::new();
    let mut route = mosstank_stores::route_store::RouteProfileStore::new();
    let mut meta = mosstank_stores::meta_store::MetaProfileStore::new();
    let mut bind = |host: &mut MemoryHost| -> [bool; 4] {
        [
            macro_store.bind_character(host, Some("Barris")).unwrap(),
            loot.bind_character(host, Some("Barris")).unwrap(),
            route.bind_character(host, Some("Barris")).unwrap(),
            meta.bind_character(host, Some("Barris")).unwrap(),
        ]
    };
    assert_eq!(bind(&mut host), [true; 4]);
    for server in ["Coldeve", "Other"] {
        host.world = server.to_owned();
        assert_eq!(bind(&mut host), [true; 4], "{server}");
        assert_eq!(bind(&mut host), [false; 4], "{server}");
    }
    host.world = String::new();
    assert_eq!(bind(&mut host), [false; 4]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: ANavWithBareLineFeedsSavesBackWithThem
#[test]
fn a_nav_with_bare_line_feeds_saves_back_with_them() {
    let original = fixtures::read_vtank("nav/emp_lower_north.nav");
    assert!(!original.contains('\r'));
    let mut host = MemoryHost::on("Coldeve");
    host.profiles.insert("mosstank/navs/lf.nav", &original);
    let mut store = mosstank_stores::route_store::RouteProfileStore::new();
    store.bind_character(&mut host, Some("Barris")).unwrap();
    assert!(store.select(&mut host, Some("lf"), false).unwrap());
    let mut route = mosstank_formats::route::NavigationSettings::default();
    assert_eq!(
        store
            .load_current(
                &mut host,
                &mut route,
                Some(&mosstank_formats::route::NoSpells)
            )
            .unwrap(),
        mosstank_stores::profile_store::ProfileLoad::Loaded
    );

    assert!(store.save_current(&mut host, &route));

    assert_eq!(
        host.profiles.get("mosstank/navs/lf.nav"),
        Some(original.as_str())
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: EditingAMetaLoadedFromAMetSavesItBackIntoTheMet
#[test]
fn editing_a_meta_loaded_from_a_met_saves_it_back_into_the_met() {
    use mosstank_formats::meta::{MetaAction, MetaActionKind, MetaCondition, MetaRule};
    let mut host = MemoryHost::on("");
    host.profiles
        .insert("mosstank/metas/foo.met", &dropped_meta());
    let mut store = mosstank_stores::meta_store::MetaProfileStore::new();
    store.bind_character(&mut host, Some("Barris")).unwrap();
    assert!(store.select(&mut host, Some("foo"), false).unwrap());
    let mut loaded = store.load_current(&mut host, None).unwrap();
    let rules = loaded.rules.len();

    loaded.rules.push(MetaRule {
        state: "Added".to_owned(),
        condition: MetaCondition::always(),
        action: MetaAction {
            kind: MetaActionKind::ChatCommand,
            text: "/say added".to_owned(),
            ..MetaAction::default()
        },
        ..MetaRule::default()
    });
    assert!(store.save_current(&mut host, &loaded));

    assert!(!host.profiles.contains("mosstank/metas/foo.af"));
    assert!(
        host.profiles
            .get("mosstank/metas/foo.met")
            .unwrap()
            .starts_with("1\r\nCondAct\r\n")
    );
    let reloaded = store.load_current(&mut host, None).unwrap();
    assert_eq!(reloaded.rules.len(), rules + 1);
    assert_eq!(reloaded.rules.last().unwrap().action.text, "/say added");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: AMetaAMetCannotHoldIsNotSavedOverTheMet
#[test]
fn a_meta_a_met_cannot_hold_is_not_saved_over_the_met() {
    let original = dropped_meta();
    let mut host = MemoryHost::on("");
    host.profiles.insert("mosstank/metas/foo.met", &original);
    let mut store = mosstank_stores::meta_store::MetaProfileStore::new();
    store.bind_character(&mut host, Some("Barris")).unwrap();
    assert!(store.select(&mut host, Some("foo"), false).unwrap());
    let mut loaded = store.load_current(&mut host, None).unwrap();

    loaded.rules[0].enabled = false;

    assert!(!store.save_current(&mut host, &loaded));
    assert!(store.save_notice().unwrap().contains("switched off"));
    assert_eq!(
        host.profiles.get("mosstank/metas/foo.met"),
        Some(original.as_str())
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: DelayedServerIdentityRestoresSavedProfileInsteadOfOverwritingBinding
/// The C#'s item list part (Add Selected and its War Wand row) comes with
/// the panel; the binding and the profile's option are the same.
#[test]
fn delayed_server_identity_restores_saved_profile_instead_of_overwriting_binding() {
    let (mut world, mut first) = barris(&[]);
    world.vt(&mut first, "settings save Personal");
    world.vt(&mut first, "opt set AttackDistance 0.02");
    let selected = first.tank().unwrap().selected_settings_profile();
    let key = directory::cdf_file_name("Barris", "Coldeve");
    let binding = file(&world, &key);
    world.disable(&mut first);

    // The name arrives before the world.
    let mut nameless_world = common::character("Barris");
    nameless_world.world = String::new();
    world.fake.character = Some(nameless_world);
    let mut reopened = world.enable();
    assert_eq!(file(&world, &key), binding);

    world.fake.character = Some(common::character("Barris"));
    world.with_tank(&mut reopened, |tank, client| tank.on_tick(client, 0.3));

    assert_eq!(
        reopened.tank().unwrap().selected_settings_profile(),
        selected
    );
    let distance = world
        .with_tank(&mut reopened, |tank, client| {
            tank.evaluate_expression(client, "uboptget[`AttackDistance`]")
        })
        .unwrap()
        .as_number("uboptget")
        .unwrap();
    assert!((distance - 0.02).abs() < 1e-7, "{distance}");
    assert_eq!(file(&world, &key), binding);
}

// ---- the window's own buttons ----------------------------------------------

/// A War Wand owned and selected, appraised (`ItemEnchantAutomation`, the
/// part the item list reads).
fn own_selected_war_wand(world: &mut World) {
    world.set_reply::<calls::ItemsOwned>(&vec![ac_plugin_abi::items::InventoryItem {
        object_id: 10,
        name: "War Wand".to_owned(),
        item_type: 0x8000,
        container_id: 0x5000_0001,
        valid_locations: 0x0100_0000,
        stack_size: 1,
        ..ac_plugin_abi::items::InventoryItem::default()
    }]);
    world.set_reply::<calls::WorldObject>(&Some(WorldObject {
        object_id: 10,
        name: "War Wand".to_owned(),
        item_type: 0x8000,
        container_id: 0x5000_0001,
        is_owned: true,
        has_appraisal_data: true,
        last_id_time: 1,
        ..WorldObject::default()
    }));
    world.set_reply::<calls::ItemsProperties>(&Some(
        ac_plugin_abi::world::ItemProperties::default(),
    ));
    select(world, 10);
}

fn item_names(world: &mut World, plugin: &mut MossTank) -> Vec<String> {
    world.ui_list(plugin, "ItemNameColumn")
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: ItemsAddReportsFailureWhenEitherProfileFileCannotBeWritten
/// The fake storage refuses the file by name rather than by suffix, with
/// the client's own refusal text.
#[test]
fn adding_an_item_reports_a_profile_file_that_cannot_be_written() {
    for suffix in [".usd", ".json"] {
        let (mut world, mut plugin) = barris(&[]);
        own_selected_war_wand(&mut world);
        // The files an addition writes, found by making one.
        let (mut probe, mut probed) = barris(&[]);
        own_selected_war_wand(&mut probe);
        probe.act(&mut probed, "AddSelectedItem");
        let written = |storage: &mosstank_stores::storage::MemoryStorage| -> Vec<String> {
            storage
                .keys()
                .filter(|key| key.ends_with(suffix))
                .map(str::to_owned)
                .collect()
        };
        world.profiles.unwritable = written(&probe.profiles);
        world.plugin_data.unwritable = written(&probe.plugin_data);
        assert!(
            !(world.profiles.unwritable.is_empty() && world.plugin_data.unwritable.is_empty()),
            "{suffix}"
        );
        world.act(&mut plugin, "AddSelectedItem");
        let notice = world.ui_text(&mut plugin, "ProfileNotice");
        assert!(notice.contains("is denied"), "{suffix}: {notice}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: DelayedServerIdentityRestoresSavedProfileInsteadOfOverwritingBinding
/// The item list's part: the War Wand added before the restart is listed
/// again once the world arrives.
#[test]
fn delayed_server_identity_restores_the_saved_item_list() {
    let (mut world, mut first) = barris(&[]);
    own_selected_war_wand(&mut world);
    world.vt(&mut first, "settings save Personal");
    world.act(&mut first, "AddSelectedItem");
    assert!(item_names(&mut world, &mut first).contains(&"War Wand".to_owned()));
    world.disable(&mut first);

    let mut nameless_world = common::character("Barris");
    nameless_world.world = String::new();
    world.fake.character = Some(nameless_world);
    let mut reopened = world.enable();
    world.fake.character = Some(common::character("Barris"));
    world.with_tank(&mut reopened, |tank, client| tank.on_tick(client, 0.3));

    // The client names none of its buffs here, so the item is written
    // with no spell and read back as kept without buffs.
    assert!(item_names(&mut world, &mut reopened).contains(&"War Wand (NO BUFFS)".to_owned()));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: ABackwardJumpAddedToANavRouteIsRefusedAndSaidSo
#[test]
fn a_backward_jump_added_to_a_nav_route_is_refused_and_said_so() {
    let original = dropped_route();
    let (mut world, mut plugin) = world_with_route("mosstank/navs/foo.nav", &original);
    world.vt(&mut plugin, "nav load foo");
    world.clear_chat();

    world.vt(&mut plugin, "addnavjump 90 false 100 backward");

    assert_eq!(file(&world, "mosstank/navs/foo.nav"), original);
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("NOT saved") && line.contains("backward")),
        "{:?}",
        world.chat()
    );
    assert!(plugin.tank().unwrap().route_notice().contains("NOT saved"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: TheWindowsNewButtonsWriteTheNavAndMetFormUnlessTheNameSaysAf
#[test]
fn the_windows_new_buttons_write_the_nav_and_met_form_unless_the_name_says_af() {
    let (mut world, mut plugin) = barris(&[]);
    world.act_text(&mut plugin, "CreateNamedRouteProfile", "Bar");
    world.act_text(&mut plugin, "CreateNamedMetaProfile", "Bar");
    world.act_text(&mut plugin, "CreateNamedRouteProfile", "Baz.af");
    world.act_text(&mut plugin, "CreateNamedMetaProfile", "Baz.af");
    for key in [
        "mosstank/navs/Bar.nav",
        "mosstank/metas/Bar.met",
        "mosstank/navs/Baz.af",
        "mosstank/metas/Baz.af",
    ] {
        assert!(world.profiles.contains(key), "{key}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: TheCopyToAfButtonsStartTheAfSaveInChat
#[test]
fn the_copy_to_af_buttons_start_the_af_save_in_chat() {
    let (mut world, mut plugin) = barris(&[]);
    world.set_reply::<calls::ChatCompose>(&true);
    world.act(&mut plugin, "CopyRouteProfileToAf");
    world.act(&mut plugin, "CopyMetaProfileToAf");
    let composed: Vec<String> = world.requests::<calls::ChatCompose>().into_iter().collect();
    assert_eq!(composed, ["/vt navaf save ", "/vt metaaf save "]);
    assert!(
        !world
            .fake
            .submitted
            .iter()
            .any(|line| line.contains("save"))
    );
}

fn the_windows_follow_leaves_the_loaded_route_file_alone(from_mode_menu: bool) {
    let (mut world, mut plugin) = world_with_route("mosstank/navs/jaw_1.af", ONE_POINT_ROUTE);
    world.vt(&mut plugin, "nav load jaw_1");
    select(&mut world, 0x5000_0ABC);
    world.set_reply::<calls::NavigationObject>(&Some(
        ac_plugin_abi::navigation::NavigationObject {
            object_id: 0x5000_0ABC,
            name: "Zero Cool".to_owned(),
            ..ac_plugin_abi::navigation::NavigationObject::default()
        },
    ));

    if from_mode_menu {
        world.act_text(&mut plugin, "SelectRouteMode", "Follow");
    } else {
        world.act(&mut plugin, "SetFollowTarget");
    }

    assert_eq!(file(&world, "mosstank/navs/jaw_1.af"), ONE_POINT_ROUTE);
    assert_eq!(selected_route(&mut world, &mut plugin), "UBFollow");
    assert_eq!(
        world.ui_text(&mut plugin, "RouteFollowTargetText"),
        "Follow target: Zero Cool"
    );
    assert!(file(&world, FOLLOW_ROUTE_KEY).contains("Zero Cool"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: TheWindowsFollowLeavesTheLoadedRouteFileAlone
#[test]
fn the_windows_follow_leaves_the_loaded_route_file_alone_both_ways() {
    the_windows_follow_leaves_the_loaded_route_file_alone(false);
    the_windows_follow_leaves_the_loaded_route_file_alone(true);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ProfileFiles.cs :: TheModeMenusFollowWithNothingSelectedChangesNothing
#[test]
fn the_mode_menus_follow_with_nothing_selected_changes_nothing() {
    let (mut world, mut plugin) = world_with_route("mosstank/navs/jaw_1.af", ONE_POINT_ROUTE);
    world.vt(&mut plugin, "nav load jaw_1");

    world.act_text(&mut plugin, "SelectRouteMode", "Follow");

    assert_eq!(
        plugin.tank().unwrap().route_notice(),
        "Select a live object to follow first."
    );
    assert_eq!(mode(&plugin), RouteMode::Circular);
    assert_eq!(selected_route(&mut world, &mut plugin), "jaw_1");
    assert_eq!(file(&world, "mosstank/navs/jaw_1.af"), ONE_POINT_ROUTE);
    assert!(!world.profiles.contains(FOLLOW_ROUTE_KEY));
}

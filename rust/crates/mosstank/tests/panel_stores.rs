//! The settings, meta, route and loot stores as the panel reaches them: the
//! old JSON documents migrated once, files dropped into the profile folder,
//! and files that will not read kept rather than written over (ported from
//! the C#'s `MossTankPanelTests`).
//!
//! The C# test host has one storage behind both of the plugin's roots; here
//! a key under `profiles/`, `recovery/` or `onboarding/` is the plugin's own
//! data and a key under `mosstank/` is the shared profile folder.

mod common;

use common::{PROFILES_ROOT, World};
use mosstank::MossTank;
use mosstank_formats::fixtures;
use mosstank_formats::meta::{MetaAction, MetaActionKind, MetaProfile, MetaRule};
use mosstank_formats::usd::{VtankCell, VtankDatabase, VtankRow, VtankTable};
use mosstank_formats::utl::{LootRule, VtankLootRequirement};
use mosstank_settings::model::AllSettings;
use mosstank_settings::{defaults, profile};
use mosstank_stores::directory;
use mosstank_stores::loot_store::LootProfileStore;
use mosstank_stores::meta_store::MetaProfileStore;
use mosstank_stores::profile_store::ProfileLoad;
use mosstank_stores::recovery::sha256_hex;
use mosstank_stores::storage::{MemoryHost, display_path};
use openac_plugin_compat::text::casing;

fn settings_key(bare_file_name: &str) -> String {
    format!("{}/{bare_file_name}", directory::SETTINGS_FOLDER)
}

/// `LegacyByCharacterProfileKey`.
fn legacy_by_character_profile_key(character_name: &str) -> String {
    let identity = format!("char:{}", casing::to_upper_invariant(character_name.trim()));
    format!("profiles/macro/{}.json", sha256_hex(&identity))
}

/// `LegacyNamedProfileKey`.
fn legacy_named_profile_key(name: &str) -> String {
    let identity = format!("named:{}", casing::to_upper_invariant(name.trim()));
    format!("profiles/macro/{}.json", sha256_hex(&identity))
}

/// `LegacyMetaByCharacterKey`.
fn legacy_meta_by_character_key(character_name: &str) -> String {
    format!(
        "profiles/meta/by-character/{}.json",
        mosstank_stores::global_variables::hash(&casing::to_lower_invariant(character_name))
    )
}

/// `LegacyMetaNamedKey`.
fn legacy_meta_named_key(name: &str) -> String {
    format!(
        "profiles/meta/named/{}.json",
        mosstank_stores::global_variables::hash(&casing::to_lower_invariant(name))
    )
}

/// A one-rule meta as `JsonSerializer.Serialize` writes a `MetaProfile`
/// whose rule is a chat command (every other member at its default).
fn legacy_meta_json(text: &str) -> String {
    format!(
        "{{\"Rules\":[{{\"State\":\"Default\",\"Condition\":{{\"Kind\":1}},\"Action\":{{\"Kind\":2,\"Text\":\"{text}\"}},\"Enabled\":true}}]}}"
    )
}

fn chat_rule(text: &str) -> MetaRule {
    MetaRule {
        action: MetaAction {
            kind: MetaActionKind::ChatCommand,
            text: text.to_owned(),
            ..MetaAction::default()
        },
        ..MetaRule::default()
    }
}

/// `new FakeAutomation { Name = name, WorldName = world }`.
fn character(name: &str, world_name: &str) -> World {
    let mut world = World::panel_character(name);
    world.fake.character.as_mut().unwrap().world = world_name.to_owned();
    world
}

fn number(world: &mut World, plugin: &mut MossTank, source: &str) -> f64 {
    world
        .with_tank(plugin, |tank, client| {
            tank.evaluate_expression(client, source)
        })
        .unwrap()
        .as_number("test")
        .unwrap()
}

fn tick(world: &mut World, plugin: &mut MossTank, seconds: f64) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, seconds));
}

fn meta_store(host: &mut MemoryHost, character: &str) -> MetaProfileStore {
    let mut store = MetaProfileStore::new();
    store.bind_character(host, Some(character)).unwrap();
    store
}

/// `RangeOf`: the combat range a settings file holds.
fn range_of(world: &World, usd_key: &str) -> f64 {
    let text = world
        .profiles
        .get(usd_key)
        .unwrap_or_else(|| panic!("{usd_key}"));
    let mut settings = AllSettings::default();
    profile::load(text, &mut settings, None).unwrap();
    settings.combat.maximum_range
}

fn dropped_route() -> String {
    fixtures::read_vtank("nav/nav_ab.nav")
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: CorruptProfileIsPreservedAndReportedBeforeDefaultsLoad
#[test]
fn a_corrupt_profile_is_kept_and_reported_before_the_defaults_load() {
    let mut world = World::panel_character("Test Character");
    world
        .plugin_data
        .insert("profiles/index.json", "{ this is not json");
    let mut plugin = world.enable();

    let notice = world.ui_text(&mut plugin, "ProfileLifecycleNotice");
    assert!(notice.contains("Raw data was preserved"), "{notice}");
    let backups: Vec<&str> = world
        .plugin_data
        .keys()
        .filter(|key| key.starts_with("recovery/macro/"))
        .collect();
    assert_eq!(backups.len(), 1, "{backups:?}");
    let backup = world.plugin_data.get(backups[0]).unwrap();
    assert!(backup.contains("profiles/index.json"));
    assert!(backup.contains("{ this is not json"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: LoadingALootProfileWarnsOnceAboutAKeyTheClientCannotSupply
#[test]
fn loading_a_loot_profile_warns_once_about_a_key_the_client_cannot_supply() {
    let mut host = MemoryHost::on("Coldeve");
    let mut profiles = LootProfileStore::new();
    profiles.bind_character(&mut host, Some("Barris")).unwrap();
    assert!(
        profiles
            .create(&mut host, Some("Monarchy"), false, &[], None)
            .unwrap()
            .0
    );
    profiles
        .save_current(
            &mut host,
            &[{
                let mut rule = LootRule::default();
                rule.name = "(A) My monarch's gear".to_owned();
                rule.vtank_requirements = vec![VtankLootRequirement {
                    kind: 12,
                    payload: "1342177290\r\n218103820\r\n".to_owned(),
                }];
                rule
            }],
            None,
        )
        .unwrap();
    host.log.warnings.clear();

    let mut rules = Vec::new();
    assert_eq!(
        profiles.load_current(&mut host, &mut rules, None).unwrap(),
        ProfileLoad::Loaded
    );

    assert_eq!(host.log.warnings.len(), 1, "{:?}", host.log.warnings);
    assert!(
        host.log.warnings[0].contains("\"(A) My monarch's gear\" reads Monarch (218103820)"),
        "{}",
        host.log.warnings[0]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: CorruptSideCarMonsterRuleIsLoggedNotSilentlySwallowed
#[test]
fn a_corrupt_side_car_monster_rule_is_logged_not_swallowed() {
    let mut world = World::panel_character("Barris");
    let usd_key = settings_key(&directory::auto_character_file_name("Barris", "", "usd"));
    world
        .profiles
        .insert(&usd_key, &defaults::create().render());
    world.plugin_data.insert(
        "profiles/macro/sidecar/--Barris_.usd.json",
        "{\n  \"CombatRules\": [\n    { \"Expression\": \"(((\" },\n    { \"Expression\": \"DEFAULT\" }\n  ]\n}",
    );
    let _plugin = world.enable();

    let warnings = world.logs(ac_plugin_abi::LogLevel::Warn);
    assert!(
        warnings
            .iter()
            .any(|line| line.to_lowercase().contains("monster rule")),
        "{warnings:?}"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ClientPathingIsSavedWithTheProfileAndTheOldCheckboxReadsAsAlways
#[test]
fn client_pathing_is_saved_with_the_profile_and_the_old_checkbox_reads_as_always() {
    let mut world = World::panel_character("Barris");
    let mut plugin = world.enable();
    assert_eq!(
        world.ui_text(&mut plugin, "SelectedClientPathing"),
        "When stuck"
    );

    world.act_text(&mut plugin, "SelectClientPathing", "Always");

    assert_eq!(
        world.ui_text(&mut plugin, "SelectedClientPathing"),
        "Always"
    );
    world.disable(&mut plugin);
    let mut reloaded = world.enable();
    assert_eq!(
        world.ui_text(&mut reloaded, "SelectedClientPathing"),
        "Always"
    );
    world.disable(&mut reloaded);

    let keys: Vec<String> = world
        .plugin_data
        .keys()
        .filter(|key| key.contains("sidecar"))
        .map(str::to_owned)
        .collect();
    assert_eq!(keys.len(), 1, "{keys:?}");
    let key = &keys[0];
    let written = world.plugin_data.get(key).unwrap().to_owned();
    assert!(written.contains("NavigationClientPathing"));

    let always = regex_like_replace(
        &written,
        "\"Always\"",
        "\"NavigationWalkLegsWithClient\": true",
    );
    world.plugin_data.insert(key, &always);
    let mut legacy = world.enable();
    assert_eq!(
        world.ui_text(&mut legacy, "SelectedClientPathing"),
        "Always"
    );
    world.disable(&mut legacy);

    let silent = regex_like_replace(&written, "\"Always\",", "");
    world.plugin_data.insert(key, &silent);
    let mut silent_plugin = world.enable();
    assert_eq!(
        world.ui_text(&mut silent_plugin, "SelectedClientPathing"),
        "When stuck"
    );
}

/// The C#'s `Regex.Replace(written, "\"NavigationClientPathing\"\\s*:\\s*"
/// + value, replacement)`: the member's name, any blanks round its colon,
/// and `value`.
fn regex_like_replace(written: &str, value: &str, replacement: &str) -> String {
    let name = "\"NavigationClientPathing\"";
    let start = written.find(name).expect("the member");
    let rest = &written[start + name.len()..];
    let after_blanks = rest.trim_start();
    let after_colon = after_blanks
        .strip_prefix(':')
        .expect("a colon")
        .trim_start();
    let tail = after_colon.strip_prefix(value).unwrap_or_else(|| {
        // `"Always",?`: the comma is optional.
        after_colon
            .strip_prefix(value.trim_end_matches(','))
            .expect("the value")
    });
    format!("{}{replacement}{tail}", &written[..start])
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: FirstLoadMigratesLegacyJsonMacroProfileToUsdAndDeletesTheJsonKey
#[test]
fn the_first_load_migrates_an_old_json_profile_to_usd_and_deletes_the_json() {
    let mut world = World::panel_character("Barris");
    let legacy_key = legacy_by_character_profile_key("Barris");
    world.plugin_data.insert(
        &legacy_key,
        "{\n  \"Combat\": { \"MaximumRange\": 48.0 },\n  \"ItemNames\": [\"Wand of Testing\"]\n}",
    );

    let mut plugin = world.enable();

    assert!(!world.plugin_data.contains(&legacy_key));
    let usd_key = settings_key(&directory::auto_character_file_name("Barris", "", "usd"));
    assert!(world.profiles.contains(&usd_key));
    let distance = number(&mut world, &mut plugin, "uboptget[`AttackDistance`]");
    assert!((distance - 0.2).abs() < 1e-7, "{distance}");
    assert!(
        world
            .ui_text(&mut plugin, "ItemProfileText")
            .contains("Wand of Testing")
    );
    world.disable(&mut plugin);

    let mut reloaded = world.enable();
    let distance = number(&mut world, &mut reloaded, "uboptget[`AttackDistance`]");
    assert!((distance - 0.2).abs() < 1e-7, "{distance}");
    assert!(
        world
            .ui_text(&mut reloaded, "ItemProfileText")
            .contains("Wand of Testing")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ExistingUsdCounterpartLeavesLegacyJsonUntouchedAndUnread
#[test]
fn an_existing_usd_leaves_the_old_json_untouched_and_unread() {
    let mut world = World::panel_character("Barris");
    let legacy_key = legacy_by_character_profile_key("Barris");
    world
        .plugin_data
        .insert(&legacy_key, "{ \"Combat\": { \"MaximumRange\": 240.0 } }");
    let usd_key = settings_key(&directory::auto_character_file_name("Barris", "", "usd"));
    let mut seed = AllSettings::default();
    seed.combat.maximum_range = 120.0;
    world
        .profiles
        .insert(&usd_key, &profile::create_new(&mut seed).unwrap().render());

    let mut plugin = world.enable();

    assert!(world.plugin_data.contains(&legacy_key));
    let distance = number(&mut world, &mut plugin, "uboptget[`AttackDistance`]");
    assert!((distance - 0.5).abs() < 1e-7, "{distance}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: DropInUsdWithLootingEnabledAndNoSideCarLoadsLootingEnabled
#[test]
fn a_dropped_in_usd_with_looting_on_and_no_side_car_loads_looting_on() {
    let mut world = World::panel_character("Barris");
    let usd_key = settings_key(&directory::auto_character_file_name("Barris", "", "usd"));
    let mut database = defaults::create();
    let settings = database.find_mut("Settings").unwrap();
    let name_column = settings.column_index("Setting").unwrap();
    let value_column = settings.column_index("Value").unwrap();
    let row = settings
        .rows
        .iter_mut()
        .find(|row| {
            row.cells[name_column]
                .as_string()
                .eq_ignore_ascii_case("EnableLooting")
        })
        .unwrap();
    row.cells[value_column] = VtankCell::bool(true);
    world.profiles.insert(&usd_key, &database.render());

    // No side-car key at all: a real drop-in, not something MossTank itself
    // ever saved.
    let mut plugin = world.enable();

    assert!(world.ui_bool(&mut plugin, "LootEnabled"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: SettingsRosterSweepConvertsEveryNamedLegacyProfileOnce
#[test]
fn the_settings_roster_sweep_converts_every_named_old_profile_once() {
    let mut world = World::panel_character("Barris");
    world.plugin_data.insert(
        "profiles/index.json",
        "{\n  \"Version\": 1,\n  \"MineOnly\": false,\n  \"Profiles\": [\n    { \"Name\": \"Farming\", \"Owner\": \"Barris\" },\n    { \"Name\": \"Buffing\", \"Owner\": \"Barris\" }\n  ],\n  \"SelectedByCharacter\": {}\n}",
    );
    world.plugin_data.insert(
        &legacy_named_profile_key("Farming"),
        "{ \"Combat\": { \"MaximumRange\": 42.0 } }",
    );
    world.plugin_data.insert(
        &legacy_named_profile_key("Buffing"),
        "{ \"Combat\": { \"MaximumRange\": 24.0 } }",
    );

    let mut plugin = world.enable();

    // MineOnly recovered from the old roster's own shape...
    assert!(!world.ui_bool(&mut plugin, "MineOnlyEnabled"));
    let prefix = directory::sub_profile_prefix("Barris", "");
    let farming_usd = settings_key(&format!("{prefix}Farming.usd"));
    let buffing_usd = settings_key(&format!("{prefix}Buffing.usd"));
    assert!((range_of(&world, &farming_usd) - 42.0).abs() < f64::EPSILON);
    assert!((range_of(&world, &buffing_usd) - 24.0).abs() < f64::EPSILON);

    // Both old JSON keys and the whole (now fully swept) roster key are
    // gone; MineOnly now lives at its own key.
    assert!(
        !world
            .plugin_data
            .contains(&legacy_named_profile_key("Farming"))
    );
    assert!(
        !world
            .plugin_data
            .contains(&legacy_named_profile_key("Buffing"))
    );
    assert!(!world.plugin_data.contains("profiles/index.json"));
    assert!(
        world
            .plugin_data
            .contains("profiles/macro/preferences.json")
    );
    world.disable(&mut plugin);

    // Idempotent: a fresh panel against the same storage sweeps nothing more
    // and keeps both values.
    let mut reloaded = world.enable();
    assert!(!world.ui_bool(&mut reloaded, "MineOnlyEnabled"));
    assert!((range_of(&world, &farming_usd) - 42.0).abs() < f64::EPSILON);
    assert!((range_of(&world, &buffing_usd) - 24.0).abs() < f64::EPSILON);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MetaStoreMigratesLegacyJsonProfileToAfAndDeletesTheJsonKey
#[test]
fn the_meta_store_migrates_an_old_json_meta_to_af_and_deletes_the_json() {
    let mut host = MemoryHost::on("");
    let legacy_key = legacy_meta_by_character_key("Barris");
    host.plugin
        .insert(&legacy_key, &legacy_meta_json("/say hi"));

    let mut store = meta_store(&mut host, "Barris");
    let loaded = store.load_current(&mut host, None).unwrap();

    assert!(!host.plugin.contains(&legacy_key));
    assert_eq!(loaded.rules.len(), 1);
    assert_eq!(loaded.rules[0].action.text, "/say hi");

    // Idempotent second run: nothing left to migrate, loads straight from
    // the now-real .af file.
    let mut reopened = meta_store(&mut host, "Barris");
    let reloaded = reopened.load_current(&mut host, None).unwrap();
    assert_eq!(reloaded.rules.len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MetaStoreLeavesLegacyJsonUntouchedWhenAfCounterpartExists
#[test]
fn the_meta_store_leaves_the_old_json_alone_when_an_af_exists() {
    let mut host = MemoryHost::on("");
    let legacy_key = legacy_meta_by_character_key("Barris");
    host.plugin
        .insert(&legacy_key, &legacy_meta_json("/say stale"));
    let real_key = format!(
        "mosstank/metas/{}",
        directory::auto_character_file_name("Barris", "", "af")
    );
    let real = MetaProfile {
        rules: vec![chat_rule("/say real")],
    };
    host.profiles.insert(
        &real_key,
        &mosstank_formats::af::save_meta(&real, false).unwrap(),
    );

    let mut store = meta_store(&mut host, "Barris");
    let loaded = store.load_current(&mut host, None).unwrap();

    assert!(host.plugin.contains(&legacy_key));
    assert_eq!(loaded.rules.len(), 1);
    assert_eq!(loaded.rules[0].action.text, "/say real");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: EveryProfileLoadSaysWhichFileAndWhichFullPathItCameFrom
#[test]
fn every_profile_load_says_which_file_and_which_full_path_it_came_from() {
    let mut world = character("Barris", "Coldeve");
    world.profiles.insert(
        "mosstank/navs/Hunt.af",
        &mosstank_formats::af::save_nav(&mosstank_formats::route::NavigationSettings::default())
            .unwrap(),
    );
    world
        .profiles
        .insert(&settings_key("Shared.usd"), &defaults::create().render());
    let mut plugin = world.enable();
    tick(&mut world, &mut plugin, 0.1);
    world.clear_chat();

    world.vt(&mut plugin, "nav load Hunt");
    tick(&mut world, &mut plugin, 0.1);

    let expected_path = display_path(PROFILES_ROOT, "mosstank/navs/Hunt.af");
    let chat = world.chat();
    assert!(
        chat.iter()
            .any(|message| message.contains("navigation profile Hunt.af")
                && message.contains(&expected_path)),
        "{chat:?}"
    );

    world.clear_chat();
    world.vt(&mut plugin, "settings load Shared");
    tick(&mut world, &mut plugin, 0.1);

    let expected_path = display_path(PROFILES_ROOT, "mosstank/profiles/Shared.usd");
    let chat = world.chat();
    assert!(
        chat.iter()
            .any(|message| message.contains("settings profile Shared.usd")
                && message.contains(&expected_path)),
        "{chat:?}"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ADroppedRouteLoadsByCommandWithOrWithoutItsExtension
#[test]
fn a_dropped_route_loads_by_command_with_or_without_its_extension() {
    for arguments in ["nav load Dropped", "nav load Dropped.nav"] {
        let mut world = character("Barris", "Coldeve");
        world
            .profiles
            .insert("mosstank/navs/Dropped.nav", &dropped_route());
        let mut plugin = world.enable();

        world.vt(&mut plugin, arguments);

        assert_eq!(
            world.ui_text(&mut plugin, "SelectedRouteProfile"),
            "Dropped",
            "{arguments}"
        );
        assert!(!world.ui_list(&mut plugin, "RouteRows").is_empty());
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AMetaLoadingAMissingMetaSwitchesToANewEmptyOneAsTheReferenceDoes
/// The plugin runs a meta's own `/vt` line there and then, so the C#'s
/// submit hook has nothing to do here.
#[test]
fn a_meta_loading_a_missing_meta_switches_to_a_new_empty_one() {
    let mut world = character("Barris", "Coldeve");
    world.profiles.insert(
        "mosstank/metas/Leader.af",
        "STATE: {Default} ~~ {\r\n\tIF:\tAlways\r\n\t\tDO:\tDoAll\r\n\t\t\t\tDoExpr {setvar[ran,1]}\r\n\t\t\t\tChat {/vt meta load DailyJohnsSetup}\r\n\tIF:\tAlways\r\n\t\tDO:\tSetState {Stuck}\r\nSTATE: {Stuck} ~~ {\r\n\tIF:\tNever\r\n\t\tDO:\tNone\r\n",
    );
    let mut plugin = world.enable();
    let p = &mut plugin;
    world.vt(p, "meta load Leader");
    world.act(p, "ToggleMeta");
    world.act(p, "ToggleCombat");

    tick(&mut world, p, 0.3);

    assert!((number(&mut world, p, "getvar[ran]") - 1.0).abs() < f64::EPSILON);
    assert_eq!(world.ui_text(p, "SelectedMetaProfile"), "DailyJohnsSetup");
    assert_eq!(p.tank().unwrap().meta_state(), "Default");
    assert!(
        world
            .profiles
            .keys()
            .any(|key| key.starts_with("mosstank/metas/DailyJohnsSetup"))
    );
    assert!(world.ui_list(p, "MetaRows").is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnEmbeddedRouteShowsByNameAndNeverOverwritesTheSelectedRouteFile
#[test]
fn an_embedded_route_shows_by_name_and_never_overwrites_the_selected_route_file() {
    let mut world = character("Barris", "Coldeve");
    world
        .profiles
        .insert("mosstank/navs/Circuit.nav", &dropped_route());
    world.profiles.insert(
        "mosstank/metas/Stipend.af",
        "STATE: {Default} ~~ {\r\n\tIF:\tAlways\r\n\t\tDO:\tDoAll\r\n\t\t\t\tEmbedNav nav0__stipend_nav {stipend.nav}\r\n\t\t\t\tSetState {Walk}\r\nSTATE: {Walk} ~~ {\r\n\tIF:\tNever\r\n\t\tDO:\tNone\r\nNAV: nav0__stipend_nav once ~~ {\r\n\tpnt 47.1 26.1 0.2\r\n",
    );
    let mut plugin = world.enable();
    let p = &mut plugin;
    world.vt(p, "nav load Circuit");
    let circuit_points = world.ui_list(p, "RouteRows").len();
    let circuit_name = world.ui_text(p, "SelectedRouteProfile");
    assert!(circuit_points > 1);

    world.vt(p, "meta load Stipend");
    world.act(p, "ToggleMeta");
    world.act(p, "ToggleCombat");
    let before: Vec<(String, String)> = world
        .profiles
        .keys()
        .map(|key| (key.to_owned(), world.profiles.get(key).unwrap().to_owned()))
        .collect();
    tick(&mut world, p, 0.3);

    assert_eq!(p.tank().unwrap().meta_state(), "Walk");
    assert_eq!(
        world.ui_text(p, "SelectedRouteProfile"),
        "stipend.nav (embedded)"
    );
    assert!(
        world
            .ui_list(p, "RouteProfileNames")
            .contains(&"stipend.nav (embedded)".to_owned())
    );
    assert_eq!(world.ui_list(p, "RouteRows").len(), 1);
    for key in world.profiles.keys() {
        if key.starts_with("mosstank/navs/") {
            let earlier = before
                .iter()
                .find(|(was, _)| was == key)
                .map(|(_, text)| text.as_str());
            assert_eq!(earlier, world.profiles.get(key), "{key}");
        }
    }

    world.vt(p, "nav load Circuit");

    assert_eq!(world.ui_text(p, "SelectedRouteProfile"), circuit_name);
    assert!(
        !world
            .ui_list(p, "RouteProfileNames")
            .contains(&"stipend.nav (embedded)".to_owned())
    );
    assert_eq!(world.ui_list(p, "RouteRows").len(), circuit_points);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: SettingAnOptionLeavesTheLoadedMetaAndRouteFilesAlone
#[test]
fn setting_an_option_leaves_the_loaded_meta_and_route_files_alone() {
    let mut world = character("Barris", "Coldeve");
    world
        .profiles
        .insert("mosstank/navs/Circuit.nav", &dropped_route());
    world.profiles.insert(
        "mosstank/metas/Edited.af",
        "STATE: {Default} ~~ {\r\n\tIF:\tAlways\r\n\t\tDO:\tChat {/vt opt set enablebuffing true}\r\n",
    );
    let mut plugin = world.enable();
    let p = &mut plugin;
    world.vt(p, "nav load Circuit");
    world.vt(p, "meta load Edited");
    let edited_meta = "STATE: {Default} ~~ {\r\n\tIF:\tAlways\r\n\t\tDO:\tChat {/vt opt set enablelooting true}\r\n";
    let edited_route = "edited on disk";
    world
        .profiles
        .insert("mosstank/metas/Edited.af", edited_meta);
    world
        .profiles
        .insert("mosstank/navs/Circuit.nav", edited_route);

    world.vt(p, "opt set enablebuffing false");
    world.vt(p, "opt toggle enablelooting");

    assert_eq!(
        world.profiles.get("mosstank/metas/Edited.af"),
        Some(edited_meta)
    );
    assert_eq!(
        world.profiles.get("mosstank/navs/Circuit.nav"),
        Some(edited_route)
    );

    world.vt(p, "setattackbar 0.5");
    world.profiles.insert(
        "mosstank/metas/Other.af",
        "STATE: {Default} ~~ {\r\n\tIF:\tNever\r\n\t\tDO:\tNone\r\n",
    );
    world.vt(p, "meta load Other");

    assert_eq!(
        world.profiles.get("mosstank/metas/Edited.af"),
        Some(edited_meta)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ADroppedMetaLoadsByCommandWithOrWithoutItsExtension
#[test]
fn a_dropped_meta_loads_by_command_with_or_without_its_extension() {
    for arguments in ["meta load Dropped", "meta load Dropped.met"] {
        let mut world = character("Barris", "Coldeve");
        world.profiles.insert(
            "mosstank/metas/Dropped.met",
            &fixtures::read_vtank("met/bella.met"),
        );
        let mut plugin = world.enable();

        world.vt(&mut plugin, arguments);

        assert_eq!(
            world.ui_text(&mut plugin, "SelectedMetaProfile"),
            "Dropped",
            "{arguments}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AFailedLoadNamesTheFullPathItTried
#[test]
fn a_failed_load_names_the_full_path_it_tried() {
    let mut world = character("Barris", "Coldeve");
    world
        .profiles
        .insert("mosstank/navs/Broken.af", "not a route at all");
    let mut plugin = world.enable();
    tick(&mut world, &mut plugin, 0.1);
    world.clear_chat();

    world.vt(&mut plugin, "nav load Broken");
    tick(&mut world, &mut plugin, 0.1);

    let expected_path = display_path(PROFILES_ROOT, "mosstank/navs/Broken.af");
    let chat = world.chat();
    assert!(
        chat.iter()
            .any(|message| message.contains("Could not load") && message.contains(&expected_path)),
        "{chat:?}"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AFailedMetaLoadSaysWhy
#[test]
fn a_failed_meta_load_says_why() {
    let mut world = character("Barris", "Coldeve");
    world.profiles.insert(
        "mosstank/metas/Broken.met",
        "1\r\nCondAct\r\n5\r\nCType\r\nAType\r\nCData\r\nAData\r\nState\r\nn\r\nn\r\nn\r\nn\r\nn\r\n1\r\ni\r\n1\r\ni\r\n99\r\n",
    );
    let mut plugin = world.enable();
    tick(&mut world, &mut plugin, 0.1);
    world.clear_chat();

    world.vt(&mut plugin, "meta load Broken.met");
    tick(&mut world, &mut plugin, 0.1);

    let expected_path = display_path(PROFILES_ROOT, "mosstank/metas/Broken.met");
    let chat = world.chat();
    assert!(
        chat.iter().any(
            |message| message.contains("Could not load Meta profile Broken.met from ")
                && message.contains(&expected_path)
                && message.contains(": VTank Meta line 19: Unexpected end of VTank Meta data.")
        ),
        "{chat:?}"
    );
    // Nothing claims the load that failed.
    assert!(
        !chat
            .iter()
            .any(|message| message.contains("Loaded Meta profile")),
        "{chat:?}"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AMetDroppedIntoTheMetasFolderLoadsAndSavesBackIntoItself
#[test]
fn a_met_dropped_into_the_metas_folder_loads_and_saves_back_into_itself() {
    let mut host = MemoryHost::on("");
    host.profiles.insert(
        "mosstank/metas/Dropped.met",
        &fixtures::read_vtank("met/bella.met"),
    );
    let mut store = meta_store(&mut host, "Barris");

    assert!(
        store
            .available_names(&mut host)
            .unwrap()
            .contains(&"Dropped".to_owned())
    );
    assert!(store.select(&mut host, Some("Dropped"), false).unwrap());

    let loaded = store.load_current(&mut host, None).unwrap();
    assert!(!loaded.rules.is_empty());

    let dropped = host
        .profiles
        .get("mosstank/metas/Dropped.met")
        .unwrap()
        .to_owned();
    assert!(store.save_current(&mut host, &loaded));

    assert_eq!(
        host.profiles.get("mosstank/metas/Dropped.met"),
        Some(dropped.as_str())
    );
    assert!(!host.profiles.contains("mosstank/metas/Dropped.af"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MetaStoreRefusesToLoadANavOnlyFileWithNoticeNamingNavsFolder
#[test]
fn the_meta_store_refuses_a_nav_only_file_and_names_the_navs_folder() {
    let mut host = MemoryHost::on("");
    host.profiles.insert(
        "mosstank/metas/Misplaced.af",
        &fixtures::read_vtank("af/nav_ab.af"),
    );
    let mut store = meta_store(&mut host, "Barris");
    assert!(store.select(&mut host, Some("Misplaced"), false).unwrap());

    let loaded = store.load_current(&mut host, None).unwrap();

    assert!(loaded.rules.is_empty());
    let notice = store.recovery_notice().expect("a recovery notice");
    assert!(notice.contains("mosstank/navs/"), "{notice}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MetaRosterSweepConvertsEveryNamedLegacyProfileOnce
#[test]
fn the_meta_roster_sweep_converts_every_named_old_meta_once() {
    let mut host = MemoryHost::on("");
    host.plugin.insert(
        "profiles/meta/index.json",
        "{ \"Names\": [\"Farming\", \"Buffing\"] }",
    );
    host.plugin.insert(
        &legacy_meta_named_key("Farming"),
        &legacy_meta_json("/say farming"),
    );
    host.plugin.insert(
        &legacy_meta_named_key("Buffing"),
        &legacy_meta_json("/say buffing"),
    );

    let mut store = meta_store(&mut host, "Barris");
    store.load_current(&mut host, None).unwrap();

    assert!(!host.plugin.contains(&legacy_meta_named_key("Farming")));
    assert!(!host.plugin.contains(&legacy_meta_named_key("Buffing")));
    assert!(!host.plugin.contains("profiles/meta/index.json"));
    let farming = mosstank_formats::af::try_load_meta(
        host.profiles.get("mosstank/metas/Farming.af").unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(farming.rules.len(), 1);
    assert_eq!(farming.rules[0].action.text, "/say farming");
    let buffing = mosstank_formats::af::try_load_meta(
        host.profiles.get("mosstank/metas/Buffing.af").unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(buffing.rules.len(), 1);
    assert_eq!(buffing.rules[0].action.text, "/say buffing");

    // Idempotent: a fresh store against the same storage sweeps nothing
    // more.
    let mut reopened = meta_store(&mut host, "Barris");
    reopened.load_current(&mut host, None).unwrap();
    assert!(host.profiles.contains("mosstank/metas/Farming.af"));
    assert!(host.profiles.contains("mosstank/metas/Buffing.af"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MetaStoreRefusesToSaveADisabledRuleAndKeepsThePriorAfContent
#[test]
fn the_meta_store_refuses_to_save_a_disabled_rule_and_keeps_the_prior_af() {
    let mut host = MemoryHost::on("");
    let mut store = meta_store(&mut host, "Barris");
    let enabled_only = MetaProfile {
        rules: vec![chat_rule("/say good")],
    };
    assert!(store.save_current(&mut host, &enabled_only));
    let key = format!(
        "mosstank/metas/{}",
        directory::auto_character_file_name("Barris", "", "af")
    );
    let good_content = host.profiles.get(&key).unwrap().to_owned();

    let mut off = chat_rule("/say off");
    off.enabled = false;
    let with_disabled_rule = MetaProfile {
        rules: vec![chat_rule("/say good"), off],
    };

    let saved = store.save_current(&mut host, &with_disabled_rule);

    assert!(!saved);
    let notice = store.save_notice().expect("a save notice");
    assert!(notice.to_lowercase().contains("disabled"), "{notice}");
    assert_eq!(host.profiles.get(&key), Some(good_content.as_str()));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: FirstRunGuidanceExplainsProfilesImportsAndPersistentShelfOnce
#[test]
fn the_first_run_guidance_explains_profiles_imports_and_the_shelf_once() {
    let mut world = World::panel_character("Test Character");
    let mut plugin = world.enable();

    tick(&mut world, &mut plugin, 0.0);
    tick(&mut world, &mut plugin, 0.0);

    let chat = world.chat();
    let first_run: Vec<&String> = chat
        .iter()
        .filter(|message| message.contains("First run:"))
        .collect();
    assert_eq!(first_run.len(), 1, "{chat:?}");
    assert!(first_run[0].contains("plugin shelf"));
    assert!(first_run[0].contains(".nav/.utl/.met"));
    assert_eq!(world.plugin_data.get("onboarding/v1.txt"), Some("shown"));
}

// ---- Settings files that will not read ---------------------------------------

fn saver() -> (World, MossTank) {
    let mut world = character("Saver", "Rune");
    let plugin = world.enable();
    (world, plugin)
}

fn selected_settings(plugin: &MossTank) -> String {
    plugin.tank().unwrap().selected_settings_profile()
}

const MALFORMED: &str = "not a settings database";

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: SettingsLoadCommandPreservesTheActiveProfileWhenTheCandidateIsMalformed
#[test]
fn a_settings_load_keeps_the_active_profile_when_the_candidate_is_malformed() {
    let (mut world, mut plugin) = saver();
    let p = &mut plugin;
    world.vt(p, "settings save Safe");
    world.vt(p, "opt set AttackDistance 0.02");
    let safe = selected_settings(p);
    let cdf_key = directory::cdf_file_name("Saver", "Rune");
    let binding = world.profiles.get(&cdf_key).unwrap().to_owned();
    world
        .profiles
        .insert("mosstank/profiles/Broken.usd", MALFORMED);
    world.clear_chat();

    world.vt(p, "settings load Broken");
    world.vt(p, "opt set EnableBuffing false");

    assert_eq!(selected_settings(p), safe);
    let distance = number(&mut world, p, "uboptget[`AttackDistance`]");
    assert!((distance - 0.02).abs() < 1e-7, "{distance}");
    assert_eq!(world.profiles.get(&cdf_key), Some(binding.as_str()));
    assert_eq!(
        world.profiles.get("mosstank/profiles/Broken.usd"),
        Some(MALFORMED)
    );
    let chat = world.chat();
    assert!(
        chat.iter()
            .any(|message| message.contains("could not be read")),
        "{chat:?}"
    );
    assert!(
        !chat
            .iter()
            .any(|message| message.contains("Loaded settings profile Broken")),
        "{chat:?}"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: SettingsSaveCommandDoesNotOverwriteAnUnreadableTarget
#[test]
fn a_settings_save_does_not_overwrite_an_unreadable_target() {
    let (mut world, mut plugin) = saver();
    let p = &mut plugin;
    world.vt(p, "settings save Safe");
    let selected = selected_settings(p);
    let cdf_key = directory::cdf_file_name("Saver", "Rune");
    let binding = world.profiles.get(&cdf_key).unwrap().to_owned();
    let target = settings_key(&format!(
        "{}Target.usd",
        directory::sub_profile_prefix("Saver", "Rune")
    ));
    world.profiles.insert(&target, MALFORMED);

    world.vt(p, "settings save Target");

    assert_eq!(selected_settings(p), selected);
    assert_eq!(world.profiles.get(&cdf_key), Some(binding.as_str()));
    assert_eq!(world.profiles.get(&target), Some(MALFORMED));
    let chat = world.chat();
    assert!(
        chat.iter()
            .any(|message| message.contains("Cannot overwrite unreadable settings profile")),
        "{chat:?}"
    );
}

fn find<'a>(database: &'a VtankDatabase, name: &str) -> &'a VtankTable {
    database.find(name).unwrap()
}

fn has_buffed(table: &VtankTable, object: i32, spell: i32) -> bool {
    let object_column = table.column_index("Object").unwrap();
    let spell_column = table.column_index("Spell").unwrap();
    table.rows.iter().any(|row| {
        row.cells[object_column].as_int().unwrap() == object
            && row.cells[spell_column].as_int().unwrap() == spell
    })
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ImportedAssistItemsMapKindsAndPreserveUnknownCustomRowsOnSave
#[test]
fn imported_assist_items_map_kinds_and_keep_unknown_custom_rows_on_save() {
    use mosstank_settings::model::ConsumableCategory;

    let mut database = defaults::create();
    {
        let table = database.find_mut("AssistItems").unwrap();
        table.column_names.push("Extension".to_owned());
        table.index_flags.push(false);
        for (name, kind, extension) in [
            ("Health Kit", 0, "keep-health"),
            ("Bread", 1, "keep-food"),
            ("Unknown", 99, "keep-unknown"),
        ] {
            table.rows.push(VtankRow {
                cells: vec![
                    VtankCell::string(name),
                    VtankCell::int(kind),
                    VtankCell::string(extension),
                ],
            });
        }
    }
    let mut settings = AllSettings::default();
    profile::load(&database.render(), &mut settings, None).unwrap();
    mosstank_settings::items::assist_items::apply(&mut settings.combat);

    assert_eq!(
        settings.combat.consumable_categories.get("Health Kit"),
        Some(&ConsumableCategory::HealthKit)
    );
    assert_eq!(
        settings.combat.consumable_categories.get("Bread"),
        Some(&ConsumableCategory::HealthFood)
    );
    assert!(
        !settings
            .combat
            .consumable_names
            .iter()
            .any(|name| name == "Unknown")
    );

    settings
        .combat
        .consumable_names
        .retain(|name| name != "Health Kit");
    settings.combat.consumable_categories.remove("Health Kit");
    settings
        .combat
        .imported_assist_items
        .retain(|item| item.name != "Health Kit");
    profile::save(&mut database, &mut settings).unwrap();
    let saved = find(&database, "AssistItems");
    let object_column = saved.column_index("Object").unwrap();
    let type_column = saved.column_index("Type").unwrap();
    let extension_column = saved.column_index("Extension").unwrap();
    assert!(
        !saved
            .rows
            .iter()
            .any(|row| row.cells[object_column].as_string() == "Health Kit")
    );
    let food: Vec<&VtankRow> = saved
        .rows
        .iter()
        .filter(|row| row.cells[object_column].as_string() == "Bread")
        .collect();
    assert_eq!(food.len(), 1);
    assert_eq!(food[0].cells[type_column].as_int().unwrap(), 1);
    assert_eq!(food[0].cells[extension_column].as_string(), "keep-food");
    let unknown: Vec<&VtankRow> = saved
        .rows
        .iter()
        .filter(|row| row.cells[object_column].as_string() == "Unknown")
        .collect();
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0].cells[type_column].as_int().unwrap(), 99);
    assert_eq!(
        unknown[0].cells[extension_column].as_string(),
        "keep-unknown"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: InitialMalformedSettingsProfileStaysInactiveAndIsNeverSavedByAnOptionChange
#[test]
fn an_initial_malformed_settings_profile_stays_inactive_and_is_never_saved() {
    let mut world = character("Saver", "Rune");
    let profile = settings_key(&directory::auto_character_file_name("Saver", "Rune", "usd"));
    world.profiles.insert(&profile, MALFORMED);
    {
        let binding = directory::VtankCharacterBinding {
            settings_file_name: profile.clone(),
            loot_file_name: String::new(),
            nav_file_name: String::new(),
            meta_file_name: None,
        };
        directory::write_character_binding(&mut world.profiles, "Saver", "Rune", &binding).unwrap();
    }
    let mut plugin = world.enable();
    let p = &mut plugin;

    world.vt(p, "opt set EnableBuffing false");
    world.vt(p, "settings save Copy");
    world.act(p, "ToggleCombat");

    assert_eq!(world.profiles.get(&profile), Some(MALFORMED));
    assert!(!world.profiles.contains(&settings_key(&format!(
        "{}Copy.usd",
        directory::sub_profile_prefix("Saver", "Rune")
    ))));
    assert!(!world.ui_bool(p, "CombatMacroRunning"));
    let notice = world.ui_text(p, "ProfileLifecycleNotice");
    assert!(notice.contains("Raw data was preserved"), "{notice}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: OptionChangesDoNotOverwriteAnExternallyCorruptedActiveSettingsProfile
/// As in the C#, the corrupted text goes under the selected profile's bare
/// name.
#[test]
fn option_changes_do_not_overwrite_an_externally_corrupted_active_profile() {
    let (mut world, mut plugin) = saver();
    world.vt(&mut plugin, "settings save Safe");
    let profile = selected_settings(&plugin);
    world.profiles.insert(&profile, MALFORMED);

    world.act(&mut plugin, "ToggleCombatEnabled");

    assert_eq!(world.profiles.get(&profile), Some(MALFORMED));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: LateSettingsConversionFailureDoesNotMutateTheActiveProfile
#[test]
fn a_late_settings_conversion_failure_does_not_change_the_active_profile() {
    let (mut world, mut plugin) = saver();
    let p = &mut plugin;
    world.vt(p, "settings save Safe");
    world.vt(p, "opt set AttackDistance 0.02");
    let safe = selected_settings(p);

    let mut candidate = defaults::create();
    {
        let settings = candidate.find_mut("Settings").unwrap();
        let attack = setting_row(settings, "AttackDistance", VtankCell::double(0.1));
        settings.rows.push(attack);
        let late = setting_row(
            settings,
            "SpellDiffExcessThreshold-Hunt",
            VtankCell {
                tag: "i".to_owned(),
                scalar_text: Some("not-an-integer".to_owned()),
                table: None,
                blob_text: None,
            },
        );
        settings.rows.push(late);
    }
    let malformed = candidate.render();
    world
        .profiles
        .insert("mosstank/profiles/LateBroken.usd", &malformed);

    world.vt(p, "settings load LateBroken");
    world.vt(p, "opt set EnableBuffing false");

    assert_eq!(selected_settings(p), safe);
    let distance = number(&mut world, p, "uboptget[`AttackDistance`]");
    assert!((distance - 0.02).abs() < 1e-7, "{distance}");
    assert_eq!(
        world.profiles.get("mosstank/profiles/LateBroken.usd"),
        Some(malformed.as_str())
    );
}

/// `SettingRow`: a `Settings` row naming `name` with `value`.
fn setting_row(table: &VtankTable, name: &str, value: VtankCell) -> VtankRow {
    let mut row = VtankRow {
        cells: vec![VtankCell::string(""); table.column_names.len()],
    };
    row.cells[table.column_index("Setting").unwrap()] = VtankCell::string(name);
    row.cells[table.column_index("Value").unwrap()] = value;
    row
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: SetInAllPreservesAnExistingEmptySettingsFile
#[test]
fn set_in_all_keeps_an_existing_empty_settings_file() {
    let mut world = World::panel_character("Test Character");
    world.profiles.insert("mosstank/profiles/Empty.usd", "");
    let mut plugin = world.enable();

    world.vt(&mut plugin, "opt setinall AttackDistance 0.03");

    assert_eq!(world.profiles.get("mosstank/profiles/Empty.usd"), Some(""));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: SettingsCopyPreservesImportedBuffedItemsAndUnknownTablesIndependently
#[test]
fn a_settings_copy_keeps_imported_buffed_items_and_unknown_tables() {
    let (mut world, mut plugin) = item_enchant_saver();
    let p = &mut plugin;
    world.vt(p, "settings save Source");
    let source = settings_key(&selected_settings(p));
    let mut source_database = VtankDatabase::parse(world.profiles.get(&source).unwrap()).unwrap();
    {
        let buffed = source_database.find_mut("BuffedItems").unwrap();
        for (object, spell) in [(10, 101), (10, 102), (-1, 101)] {
            buffed.rows.push(VtankRow {
                cells: vec![VtankCell::int(object), VtankCell::int(spell)],
            });
        }
    }
    let custom = VtankTable {
        column_names: vec!["Marker".to_owned()],
        index_flags: vec![false],
        rows: vec![VtankRow {
            cells: vec![VtankCell::string("copy proof")],
        }],
    };
    source_database
        .tables
        .push(("CopyProof".to_owned(), custom));
    world.profiles.insert(&source, &source_database.render());

    world.vt(p, "settings load Source");
    world.vt(p, "settings save Copy");
    let copy = settings_key(&selected_settings(p));
    let copied = VtankDatabase::parse(world.profiles.get(&copy).unwrap()).unwrap();
    let copied_buffed = find(&copied, "BuffedItems");
    assert!(has_buffed(copied_buffed, 10, 101));
    assert!(has_buffed(copied_buffed, 10, 102));
    assert!(has_buffed(copied_buffed, -1, 101));
    assert_eq!(
        find(&copied, "CopyProof").rows[0].cells[0].as_string(),
        "copy proof"
    );

    world.vt(p, "opt set AttackDistance 0.02");
    let source_after = VtankDatabase::parse(world.profiles.get(&source).unwrap()).unwrap();
    assert_eq!(
        find(&source_after, "CopyProof").rows[0].cells[0].as_string(),
        "copy proof"
    );
    assert_eq!(find(&source_after, "BuffedItems").rows.len(), 3);

    world.profiles.insert(
        &settings_key(&selected_settings(p)),
        "externally corrupted source",
    );
    world.act(p, "ToggleAutoStack");
    assert!(!world.ui_bool(p, "AutoStackEnabled"));
    world.act_int(p, "DeleteItemRowAt", 0);
    world.vt(p, "settings save StaleCopy");
    let stale_copy = VtankDatabase::parse(
        world
            .profiles
            .get(&settings_key(&selected_settings(p)))
            .unwrap(),
    )
    .unwrap();
    let stale_settings = find(&stale_copy, "Settings");
    let setting_column = stale_settings.column_index("Setting").unwrap();
    let value_column = stale_settings.column_index("Value").unwrap();
    let auto_stack: Vec<&VtankRow> = stale_settings
        .rows
        .iter()
        .filter(|row| {
            row.cells[setting_column]
                .as_string()
                .eq_ignore_ascii_case("AutoStack")
        })
        .collect();
    assert_eq!(auto_stack.len(), 1);
    assert!(!auto_stack[0].cells[value_column].as_bool());
    let stale_buffed = find(&stale_copy, "BuffedItems");
    assert!(!has_buffed(stale_buffed, 10, 101));
    assert!(has_buffed(stale_buffed, 10, 102));
    assert_eq!(
        find(&stale_copy, "CopyProof").rows[0].cells[0].as_string(),
        "copy proof"
    );
}

/// `NamedSpell` in the item-enchantment school.
fn item_enchantment(spell_id: u32, family: u32, name: &str) -> ac_plugin_abi::magic::SpellInfo {
    ac_plugin_abi::magic::SpellInfo {
        spell_id,
        name: name.to_owned(),
        family,
        tier: 1,
        difficulty: 10,
        mana_cost: 5,
        duration_seconds: 1800.0,
        school: 32,
        description: "Increases a weapon's damage value by 2 points.".to_owned(),
        is_self_targeted: true,
        is_beneficial: true,
        ..ac_plugin_abi::magic::SpellInfo::default()
    }
}

/// `ItemEnchantAutomation` with a second, wielded wand.
fn item_enchant_saver() -> (World, MossTank) {
    let mut world = character("Saver", "Rune");
    world.know_spells(vec![
        item_enchantment(101, 201, "Aura of Defender Self I"),
        item_enchantment(102, 202, "Aura of Hermetic Link Self I"),
        item_enchantment(103, 203, "Aura of Spirit Drinker Self I"),
    ]);
    let mut equipped = common::item(11, "Equipped Wand", 0x8000);
    equipped.equipped_location = 0x0100_0000;
    world.own_items(vec![common::item(10, "War Wand", 0x8000), equipped]);
    let plugin = world.enable();
    (world, plugin)
}

// ---- The log channels a profile keeps -------------------------------------------

/// `LastLogState`: the last line `/vt log` answered.
fn last_log_state(world: &World) -> String {
    world
        .chat()
        .into_iter()
        .rev()
        .find(|message| {
            message.starts_with("[MossTank] Log state:")
                || message == "[MossTank] Not currently logging."
        })
        .expect("a log state line")
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AutostartLogChannelsAreTheSessionsAndSurviveAProfileLoad
#[test]
fn the_autostart_log_channels_are_the_sessions_and_survive_a_profile_load() {
    // An earlier session wrote the profile the run selects, with one channel
    // of the profile's own.
    let mut world = World::panel_character("Prover");
    let mut first = world.enable();
    world.vt(&mut first, "log ActiveRule on");
    world.vt(&mut first, "settings save vt-proof-settings");
    world.disable(&mut first);

    world.set_session(&[
        ("settingsProfile", "vt-proof-settings"),
        ("logChannels", "SpellCast,RuleInfo"),
    ]);
    let mut plugin = world.enable();
    let p = &mut plugin;
    world.with_tank(p, |tank, client| tank.tick_autostart(client));
    assert!(world.errors().is_empty(), "{:?}", world.errors());
    world.vt(p, "log");
    assert_eq!(
        last_log_state(&world),
        "[MossTank] Log state:  ActiveRule RuleInfo SpellCast"
    );

    // The run reloads the same profile from outside, as the proof does.
    world.vt(p, "settings load vt-proof-settings.usd");
    world.vt(p, "log");
    assert_eq!(
        last_log_state(&world),
        "[MossTank] Log state:  ActiveRule RuleInfo SpellCast"
    );

    // Turning a session channel off is honoured too.
    world.vt(p, "log SpellCast off");
    world.vt(p, "log");
    assert_eq!(
        last_log_state(&world),
        "[MossTank] Log state:  ActiveRule RuleInfo"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheVtLogChannelSelectionSurvivesAReload
#[test]
fn the_vt_log_channel_selection_survives_a_reload() {
    let mut world = World::panel_character("Log Persist");
    let mut first = world.enable();
    world.vt(&mut first, "log Misc on");
    world.vt(&mut first, "log SpellCast on");
    world.vt(&mut first, "log RuleInfo on");
    world.vt(&mut first, "log RuleInfo off");
    world.disable(&mut first);
    world.clear_chat();

    let mut second = world.enable();
    world.vt(&mut second, "log");

    let states: Vec<String> = world
        .chat()
        .into_iter()
        .filter(|message| message.starts_with("[MossTank] Log state:"))
        .collect();
    assert_eq!(states, ["[MossTank] Log state:  Misc SpellCast"]);
}

// ---- Loot profiles ----------------------------------------------------------------

/// `LegacyLootProfileKey`.
fn legacy_loot_profile_key(value: &str, by_character: bool) -> String {
    let identity = format!(
        "{}{}",
        if by_character { "char:" } else { "named:" },
        casing::to_upper_invariant(value.trim())
    );
    format!("profiles/loot/{}.json", sha256_hex(&identity))
}

fn loot_key(bare_file_name: &str) -> String {
    format!("{}/{bare_file_name}", directory::LOOT_FOLDER)
}

fn single_loot_row(world: &mut World, plugin: &mut MossTank) -> String {
    let rows = world.ui_list(plugin, "LootRuleRows");
    assert_eq!(rows.len(), 1, "{rows:?}");
    rows[0].clone()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: LootRosterSweepConvertsByCharacterAndEveryNamedLegacyProfileOnce
#[test]
fn the_loot_roster_sweep_converts_by_character_and_every_named_old_profile_once() {
    let mut world = World::panel_character("Barris");
    world.plugin_data.insert(
        &legacy_loot_profile_key("Barris", true),
        "{\n  \"Rules\": [\n    { \"Name\": \"Coins\", \"Expression\": \"name ~= coin\", \"Action\": 0, \"Priority\": 3 }\n  ]\n}",
    );
    world.plugin_data.insert(
        "profiles/loot/index.json",
        "{\n  \"Version\": 1,\n  \"Names\": [\"Farming\"],\n  \"SelectedByCharacter\": {}\n}",
    );
    world.plugin_data.insert(
        &legacy_loot_profile_key("Farming", false),
        "{\n  \"Rules\": [\n    { \"Name\": \"Salvage\", \"Expression\": \"name ~= salvage\", \"Action\": 0, \"Priority\": 1 }\n  ]\n}",
    );

    let mut plugin = world.enable();

    let by_character_file = loot_key(&directory::auto_character_file_name("Barris", "", "utl"));
    assert!(world.profiles.contains(&by_character_file));
    assert!(world.profiles.contains("mosstank/loot/Farming.utl"));
    assert!(
        !world
            .plugin_data
            .contains(&legacy_loot_profile_key("Barris", true))
    );
    assert!(
        !world
            .plugin_data
            .contains(&legacy_loot_profile_key("Farming", false))
    );
    assert!(!world.plugin_data.contains("profiles/loot/index.json"));

    assert!(single_loot_row(&mut world, &mut plugin).contains("name ~= coin"));

    world.act_text(&mut plugin, "SelectLootProfile", "Farming");
    assert!(single_loot_row(&mut world, &mut plugin).contains("name ~= salvage"));
    world.disable(&mut plugin);

    // Idempotent: a fresh panel against the same storage sweeps nothing more
    // and keeps both values.
    let mut reloaded = world.enable();
    assert!(single_loot_row(&mut world, &mut reloaded).contains("name ~= coin"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: CharacterRebindToMalformedLootProfileDoesNotRetainPreviousCharactersRules
#[test]
fn a_rebind_to_a_malformed_loot_profile_does_not_keep_the_previous_characters_rules() {
    let mut world = character("First", "Coldeve");
    let mut plugin = world.enable();
    let p = &mut plugin;
    world.vt(p, "loot new FirstLoot");
    world.act(p, "AddLootRule");
    assert_eq!(world.ui_list(p, "LootRuleRows").len(), 1);

    let second_settings = settings_key(&directory::auto_character_file_name(
        "Second", "Coldeve", "usd",
    ));
    world
        .profiles
        .insert(&second_settings, &defaults::create().render());
    let malformed = "UTL\r\n1\r\n1\r\nunfinished";
    world
        .profiles
        .insert("mosstank/loot/SecondLoot.utl", malformed);
    let binding = directory::VtankCharacterBinding {
        settings_file_name: second_settings,
        loot_file_name: "SecondLoot.utl".to_owned(),
        nav_file_name: String::new(),
        meta_file_name: None,
    };
    directory::write_character_binding(&mut world.profiles, "Second", "Coldeve", &binding).unwrap();

    name(&mut world, "Second");
    tick(&mut world, p, 0.1);

    assert_eq!(
        world.ui_text(p, "LootProfileName"),
        mosstank_stores::loot_store::NO_ACTIVE_PROFILE
    );
    assert!(world.ui_list(p, "LootRuleRows").is_empty());
    assert_eq!(
        world.profiles.get("mosstank/loot/SecondLoot.utl"),
        Some(malformed)
    );
    let notice = world.ui_text(p, "LootEditorNotice");
    assert!(notice.contains("No loot profile is active"), "{notice}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: LootCommandsImportAndExportExactVtclassicUtlFiles
#[test]
fn the_loot_commands_import_and_export_exact_vtclassic_utl_files() {
    use mosstank_formats::utl::{self, LootAction, VtankLootProfile, VtankSalvageCombineSettings};

    let mut world = World::panel_character("Test Character");
    let mut rule = LootRule::default();
    rule.name = "Pyreal".to_owned();
    rule.action = LootAction::KeepUpTo;
    rule.keep_count = 100;
    rule.priority = 7;
    rule.vtank_requirements = vec![VtankLootRequirement {
        kind: 1,
        payload: "^Pyreal$\r\n1\r\n".to_owned(),
    }];
    let legacy = VtankLootProfile {
        rules: vec![rule],
        salvage_combine: VtankSalvageCombineSettings {
            default_combine_string: "1-5, 6-10".to_owned(),
            material_combine_strings: [(61, "1-10".to_owned())].into_iter().collect(),
            material_value_mode_values: [(61, 75_000)].into_iter().collect(),
        },
        ..VtankLootProfile::default()
    };
    world
        .plugin_data
        .insert("imports/Legacy.utl", &utl::write(&legacy));
    let mut plugin = world.enable();
    let p = &mut plugin;

    world.vt(p, "loot load Legacy.utl");
    world.vt(p, "loot save Copy");

    assert_eq!(world.ui_text(p, "LootProfileName"), "Copy");
    let row = single_loot_row(&mut world, p);
    assert!(row.contains("KeepUpTo"), "{row}");
    let exported = world.profiles.get("mosstank/loot/Copy.utl").unwrap();
    let round_trip = utl::try_read(Some(exported)).unwrap();
    assert_eq!(
        round_trip.salvage_combine.default_combine_string,
        "1-5, 6-10"
    );
    assert_eq!(
        round_trip
            .salvage_combine
            .material_value_mode_values
            .get(&61),
        Some(&75_000)
    );
    assert_eq!(round_trip.rules.len(), 1);
    assert_eq!(round_trip.rules[0].vtank_requirements[0].kind, 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MacroSettingsRoundTripPreservesImportedLootConditions
#[test]
fn a_settings_round_trip_keeps_imported_loot_conditions() {
    use mosstank_dotnet::collections::{IgnoreCaseStringSet, OrdinalStringSet};
    use mosstank_stores::profile_store::ProfileStore;

    let mut host = MemoryHost::on("");
    let mut store = ProfileStore::new(&mut host);
    // A profile belongs to a character; the store writes nothing until it
    // knows whose it is.
    store.bind_character(&mut host, Some("Prover")).unwrap();
    let mut settings = AllSettings::default();
    let mut rule = LootRule::default();
    rule.name = "Only pyreals".to_owned();
    rule.custom_expression = "preserve me".to_owned();
    rule.vtank_requirements = vec![VtankLootRequirement {
        kind: 1,
        payload: "^Pyreal$\r\n1\r\n".to_owned(),
    }];
    settings.inventory.loot.rules.push(rule);
    let mut no_buff = OrdinalStringSet::new();
    let mut channels = IgnoreCaseStringSet::new();
    store
        .save_current(&mut host, &mut settings, &no_buff, &channels)
        .unwrap();
    settings.inventory.loot.rules.clear();
    store
        .load_current(&mut host, &mut settings, &mut no_buff, &mut channels)
        .unwrap();
    assert_eq!(settings.inventory.loot.rules.len(), 1);
    let restored = &settings.inventory.loot.rules[0];
    assert_eq!(restored.vtank_requirements.len(), 1);
    assert_eq!(restored.vtank_requirements[0].payload, "^Pyreal$\r\n1\r\n");
    assert_eq!(restored.custom_expression, "preserve me");
}

// ---- Settings files ----------------------------------------------------------------

/// `ProfileTextWithEnableMeta`.
fn profile_text_with_enable_meta(value: bool) -> String {
    let mut database = defaults::create();
    let settings = database.find_mut("Settings").unwrap();
    let name_column = settings.column_index("Setting").unwrap();
    let value_column = settings.column_index("Value").unwrap();
    let row = settings
        .rows
        .iter_mut()
        .find(|row| {
            row.cells[name_column]
                .as_string()
                .eq_ignore_ascii_case("EnableMeta")
        })
        .unwrap();
    row.cells[value_column] = VtankCell::bool(value);
    database.render()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AProfileWhoseEnableMetaIsTrueLoadsWithTheMetaEngineRunning
#[test]
fn a_profile_whose_enable_meta_is_true_loads_with_the_meta_running() {
    let mut world = World::panel_character("Metaphile");
    world.profiles.insert(
        &settings_key(&directory::auto_character_file_name("Metaphile", "", "usd")),
        &profile_text_with_enable_meta(true),
    );

    let mut plugin = world.enable();

    assert!(world.ui_bool(&mut plugin, "MetaEnabled"));
    assert!(
        world
            .with_tank(&mut plugin, |tank, client| tank
                .evaluate_expression(client, "uboptget[`EnableMeta`]"))
            .unwrap()
            .is_truthy()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: SetOptionInAllAppendsARowWhenAFileHasNoneForThatSetting
#[test]
fn set_option_in_all_appends_a_row_when_a_file_has_none_for_that_setting() {
    use mosstank_stores::profile_store::ProfileStore;

    let mut host = MemoryHost::on("");
    let minimal_usd = "1\r\nSettings\r\n4\r\nSetting\r\nValue\r\nDescription\r\nSettingType\r\ny\r\nn\r\nn\r\nn\r\n1\r\ns\r\nEnableNav\r\nb\r\nFalse\r\ns\r\n\r\ni\r\n1\r\n";
    host.profiles
        .insert("mosstank/profiles/Other.usd", minimal_usd);
    let mut store = ProfileStore::new(&mut host);
    let mut settings = AllSettings::default();
    settings.inventory.loot.enabled = true;

    let count = store
        .set_option_in_all(&mut host, "EnableLooting", &settings)
        .unwrap();

    let rewritten =
        VtankDatabase::parse(host.profiles.get("mosstank/profiles/Other.usd").unwrap()).unwrap();
    let table = find(&rewritten, "Settings");
    let name_column = table.column_index("Setting").unwrap();
    let value_column = table.column_index("Value").unwrap();
    let appended: Vec<&VtankRow> = table
        .rows
        .iter()
        .filter(|row| {
            row.cells[name_column]
                .as_string()
                .eq_ignore_ascii_case("EnableLooting")
        })
        .collect();
    assert_eq!(appended.len(), 1);
    assert!(appended[0].cells[value_column].as_bool());
    assert_eq!(appended[0].cells[2].tag, "0");
    assert_eq!(appended[0].cells[3].tag, "0");
    assert!(count >= 1);
}

fn files_ending(world: &World, suffix: &str) -> Vec<String> {
    let mut keys: Vec<String> = world
        .profiles
        .keys()
        .chain(world.plugin_data.keys())
        .filter(|key| key.ends_with(suffix))
        .map(str::to_owned)
        .collect();
    keys.sort();
    keys
}

fn name(world: &mut World, name: &str) {
    world.fake.character.as_mut().unwrap().name = name.to_owned();
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: LoginBindsExactlyOneSettingsFileAndOneBindingFile
/// The world reports the character's name without the server's display
/// marker until the character's own object has arrived, and with it
/// afterwards. Both spellings file under one profile.
#[test]
fn a_login_binds_exactly_one_settings_file_and_one_binding_file() {
    let mut world = character("Acdream", "sawato");
    let mut plugin = world.enable();
    world.act_float(&mut plugin, "SetNormalHealth", 0.33);

    name(&mut world, "+Acdream");
    tick(&mut world, &mut plugin, 0.1);
    world.act_float(&mut plugin, "SetNormalHealth", 0.44);

    let settings_files = files_ending(&world, ".usd");
    let binding_files = files_ending(&world, ".cdf");
    assert_eq!(settings_files.len(), 1, "{settings_files:?}");
    assert_eq!(binding_files.len(), 1, "{binding_files:?}");
    assert!(!settings_files.iter().any(|key| key.contains('+')));
    assert!(!binding_files.iter().any(|key| key.contains('+')));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ARelogKeepsTheCharactersOneProfile
/// A session's whole life as the client reports it: no name while the
/// plugin loads, the plain name, the marked name, no name again while it
/// logs out and back in, then the same two spellings. One character, one
/// profile.
#[test]
fn a_relog_keeps_the_characters_one_profile() {
    let mut world = character("", "sawato");
    let mut plugin = world.enable();
    let p = &mut plugin;
    tick(&mut world, p, 0.1);

    name(&mut world, "Mossy");
    tick(&mut world, p, 0.1);
    name(&mut world, "+Mossy");
    tick(&mut world, p, 0.1);
    world.act_float(p, "SetNormalHealth", 0.44);

    // The relog: the name goes away and comes back.
    name(&mut world, "");
    tick(&mut world, p, 0.1);
    // Nobody is named, so nobody else's settings take over meanwhile.
    assert!((world.ui_float(p, "NormalHealthValue") - 0.44).abs() < 0.0005);
    name(&mut world, "Mossy");
    tick(&mut world, p, 0.1);
    name(&mut world, "+Mossy");
    tick(&mut world, p, 0.1);

    assert_eq!(
        files_ending(&world, ".usd"),
        [settings_key("--Mossy_sawato.usd")]
    );
    assert_eq!(files_ending(&world, ".cdf").len(), 1);
    assert!(
        !world
            .profiles
            .keys()
            .chain(world.plugin_data.keys())
            .any(|key| key.contains("--_"))
    );
    assert!((world.ui_float(p, "NormalHealthValue") - 0.44).abs() < 0.0005);
}

/// `FrameLootSurface.CorpseId`.
const FRAME_CORPSE_ID: u32 = 0x7000_0D01;

/// The loot surface of `FrameLootSurface`: one identified corpse of the
/// character's own, three meters off, nothing open.
fn frame_loot_surface(world: &mut World) {
    use ac_plugin_abi::calls;
    use ac_plugin_abi::items::{LootContainer, LootState};
    world.set_reply::<calls::LootState>(&LootState {
        is_available: true,
        current_contents_ready: true,
        ..LootState::default()
    });
    world.set_reply::<calls::LootCorpses>(&vec![LootContainer {
        object_id: FRAME_CORPSE_ID,
        weenie_class_id: 1,
        name: "Corpse".to_owned(),
        distance: 3.0,
        is_identified: true,
        long_description: "Killed by Looter.".to_owned(),
        ..LootContainer::default()
    }]);
    world.set_reply::<calls::LootOpen>(&ac_plugin_abi::items::ItemResult::of(
        ac_plugin_abi::items::ItemStatus::Started,
    ));
}

/// `FrameLootSurface.Opened`: the container last asked open, 0 for none.
fn opened(world: &World) -> u32 {
    world
        .requests::<ac_plugin_abi::calls::LootOpen>()
        .last()
        .copied()
        .unwrap_or(0)
}

/// The client's loot classifiers (`FakeLootClassifierRegistry`).
fn utility_loot_classifier(world: &mut World) {
    world.set_reply::<ac_plugin_abi::calls::LootClassifiers>(&vec![
        ac_plugin_abi::items::ClassifierInfo {
            id: "utility/loot".to_owned(),
            display_name: "Utility Loot".to_owned(),
        },
    ]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MalformedStartupLootBindingIsInactiveAndCannotBeOverwrittenByLaterSaves
/// A malformed profile named by the startup binding inherits neither the
/// side-car's old rules nor its external classifier's looting: it stays
/// inactive, and ordinary saves overwrite neither file.
#[test]
fn a_malformed_startup_loot_binding_is_inactive_and_cannot_be_overwritten() {
    use common::automation::FakeAutomation;

    let mut looter = FakeAutomation::default();
    "Looter".clone_into(&mut looter.name);
    "Coldeve".clone_into(&mut looter.world_name);
    let mut world = World::with_automation(looter);
    utility_loot_classifier(&mut world);
    let mut first = world.enable();
    world.vt(&mut first, "loot new Loot5");
    world.act(&mut first, "AddLootRule");
    world.act_text(
        &mut first,
        "SelectLootClassifier",
        "Utility Loot [utility/loot]",
    );
    if !world.ui_bool(&mut first, "LootEnabled") {
        world.act(&mut first, "ToggleLooting");
    }
    world.disable(&mut first);

    let malformed = "UTL\r\n1\r\n1\r\nunfinished";
    world.profiles.insert("mosstank/loot/Loot5.utl", malformed);
    let cdf_key = directory::cdf_file_name("Looter", "Coldeve");
    let cdf_before = world.profiles.get(&cdf_key).unwrap().to_owned();
    frame_loot_surface(&mut world);
    world.fake.logs.clear();

    let mut restarted = world.enable();
    let p = &mut restarted;

    assert_eq!(
        world.ui_text(p, "LootProfileName"),
        mosstank_stores::loot_store::NO_ACTIVE_PROFILE
    );
    assert!(world.ui_list(p, "LootRuleRows").is_empty());
    assert_eq!(
        world.ui_text(p, "SelectedLootClassifier"),
        "Utility Loot [utility/loot]"
    );
    let notice = world.ui_text(p, "LootEditorNotice");
    assert!(notice.contains("No loot profile is active"), "{notice}");
    let errors = world.errors();
    assert!(
        errors
            .iter()
            .any(|message| message.contains("Loot5")
                && message.contains("No loot profile is active")),
        "{errors:?}"
    );

    world.act(p, "ToggleLootPriorityBoost");
    world.act(p, "ToggleCombat");
    for _ in 0..12 {
        tick(&mut world, p, 0.3);
    }

    assert_eq!(opened(&world), 0);
    assert_eq!(
        world.profiles.get("mosstank/loot/Loot5.utl"),
        Some(malformed)
    );
    assert_eq!(world.profiles.get(&cdf_key), Some(cdf_before.as_str()));

    let mut keep_all = LootRule::default();
    keep_all.action = mosstank_formats::utl::LootAction::Keep;
    let recovered = mosstank_formats::utl::VtankLootProfile {
        rules: vec![keep_all],
        ..mosstank_formats::utl::VtankLootProfile::default()
    };
    world.profiles.insert(
        "mosstank/loot/Recovered.utl",
        &mosstank_formats::utl::write(&recovered),
    );
    world.act_text(p, "SelectLootProfile", "Recovered");

    assert_eq!(world.ui_text(p, "LootProfileName"), "Recovered");
    assert_eq!(world.ui_list(p, "LootRuleRows").len(), 1);
    assert_eq!(
        world.profiles.get("mosstank/loot/Loot5.utl"),
        Some(malformed)
    );
    assert!(
        world
            .profiles
            .get(&cdf_key)
            .unwrap()
            .contains("Recovered.utl")
    );
    for _ in 0..12 {
        if opened(&world) != 0 {
            break;
        }
        tick(&mut world, p, 0.3);
    }
    assert_eq!(opened(&world), FRAME_CORPSE_ID);
}

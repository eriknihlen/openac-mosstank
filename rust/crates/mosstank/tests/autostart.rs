//! The autostart (ported from the C#'s `MossTankAutostartTests`).

mod common;

use common::{World, character, proof_fixture, write_session_proof_fixture};
use mosstank_formats::meta::{MetaAction, MetaActionKind, MetaProfile, MetaRule};
use mosstank_settings::model::AllSettings;
use mosstank_stores::loot_store::LootProfileStore;
use mosstank_stores::meta_store::MetaProfileStore;
use mosstank_stores::profile_store::ProfileStore;
use mosstank_stores::route_store::RouteProfileStore;
use mosstank_stores::storage::MemoryHost;

fn in_world() -> World {
    World::with_character("TestChar")
}

fn running(world: &World, plugin: &mosstank::MossTank) -> bool {
    let _ = world;
    plugin.tank().unwrap().macro_running()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: NoSessionSettingsDeclaredIsANoOp
#[test]
fn no_session_settings_declared_is_a_no_op() {
    let mut world = in_world();
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    assert!(!running(&world, &plugin));
    assert!(world.errors().is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: NotInWorldIsANoOpEvenWithSettingsDeclared
#[test]
fn not_in_world_is_a_no_op_even_with_settings_declared() {
    let mut world = World::new();
    world.set_session(&[("startMacro", "true")]);
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    assert!(!running(&world, &plugin));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: StartMacroTrueStartsTheMacroTheSamePathAsVtStart
#[test]
fn start_macro_true_starts_the_macro_the_same_path_as_vt_start() {
    let mut world = in_world();
    world.set_session(&[("startMacro", "true")]);
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    assert!(running(&world, &plugin));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: MissingNamedProfileLogsOneLineButKeepsGoingToStartTheMacro
#[test]
fn missing_named_profile_logs_one_line_but_keeps_going_to_start_the_macro() {
    let mut world = in_world();
    world.set_session(&[
        ("settingsProfile", "DoesNotExist"),
        ("metaProfile", "DoesNotExistEither"),
        ("navProfile", "NorThisOne"),
        ("lootProfile", "NorThisOneEither"),
        ("startMacro", "true"),
    ]);
    let mut plugin = world.enable();
    world.tick(&mut plugin);

    let errors = world.errors();
    assert_eq!(errors.len(), 4, "{errors:?}");
    assert!(errors.iter().any(|line| line.contains("DoesNotExist")));
    assert!(running(&world, &plugin));
}

fn save_four_profiles(world: &mut World, plugin: &mut mosstank::MossTank) {
    world.vt(plugin, "settings save myprofile");
    world.vt(plugin, "meta save myMeta");
    world.vt(plugin, "nav save myNav");
    world.vt(plugin, "loot new myLoot");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: RealNamedProfilesAreSelectedThroughTheSameStoreMethodsAsTheProfilesTab
#[test]
fn real_named_profiles_are_selected_through_the_same_store_methods_as_the_profiles_tab() {
    let mut world = in_world();
    let mut plugin = world.enable();
    save_four_profiles(&mut world, &mut plugin);
    world.set_session(&[
        ("settingsProfile", "myprofile"),
        ("metaProfile", "myMeta"),
        ("navProfile", "myNav"),
        ("lootProfile", "myLoot"),
    ]);

    world.tick(&mut plugin);

    assert!(world.errors().is_empty(), "{:?}", world.errors());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: SelectingAProfileSaysSoInTheLogNotOnlyInTheTabsNotice
#[test]
fn selecting_a_profile_says_so_in_the_log_not_only_in_the_tabs_notice() {
    let mut world = in_world();
    let mut plugin = world.enable();
    save_four_profiles(&mut world, &mut plugin);
    world.set_session(&[
        ("settingsProfile", "myprofile"),
        ("metaProfile", "myMeta"),
        ("navProfile", "myNav"),
        ("lootProfile", "myLoot"),
    ]);
    world.fake.logs.clear();

    world.tick(&mut plugin);

    assert!(world.errors().is_empty(), "{:?}", world.errors());
    let infos = world.infos();
    for profile in ["myprofile", "myMeta", "myNav", "myLoot"] {
        assert!(
            infos.iter().any(|line| line.contains("oaded")
                && line.to_lowercase().contains(&profile.to_lowercase())),
            "no load line for {profile}: {infos:?}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: EnableMetaTrueEnablesTheEngineAndSelectsItsDeclaredProfile
#[test]
fn enable_meta_true_enables_the_engine_and_selects_its_declared_profile() {
    let mut world = in_world();
    let mut plugin = world.enable();
    world.vt(&mut plugin, "meta save myMeta");
    assert!(!plugin.tank().unwrap().meta_enabled());
    world.set_session(&[("metaProfile", "myMeta"), ("enableMeta", "true")]);

    world.tick(&mut plugin);

    assert!(plugin.tank().unwrap().meta_enabled());
    let selected = world.with_tank(&mut plugin, |tank, client| {
        tank.selected_meta_profile(client)
    });
    assert_eq!(selected, "myMeta");
    assert!(world.errors().is_empty(), "{:?}", world.errors());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: AbsentEnableMetaKeyLeavesEnableMetaAsWhateverTheSettingsProfileSays
#[test]
fn absent_enable_meta_key_leaves_enable_meta_as_whatever_the_settings_profile_says() {
    let mut world = in_world();
    let mut plugin = world.enable();
    world.vt(&mut plugin, "settings save myprofile");
    world.vt(&mut plugin, "opt set enablemeta true");
    assert!(plugin.tank().unwrap().meta_enabled());
    world.vt(&mut plugin, "settings save plain");
    world.vt(&mut plugin, "opt set enablemeta false");
    assert!(!plugin.tank().unwrap().meta_enabled());
    world.set_session(&[("settingsProfile", "myprofile")]);

    world.tick(&mut plugin);

    assert!(plugin.tank().unwrap().meta_enabled());
    assert!(world.errors().is_empty(), "{:?}", world.errors());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: AutostartAppliesExactlyOncePerGenerationAndReappliesAfterReconnect
#[test]
fn autostart_applies_exactly_once_per_generation_and_reapplies_after_reconnect() {
    let mut world = in_world();
    world.set_session(&[("settingsProfile", "DoesNotExist")]);
    let mut plugin = world.enable();

    world.tick(&mut plugin);
    assert_eq!(world.errors().len(), 1);
    world.tick(&mut plugin);
    assert_eq!(world.errors().len(), 1);

    world.fake.character = None;
    world.tick(&mut plugin);
    assert_eq!(world.errors().len(), 1);

    world.fake.character = Some(character("TestChar"));
    world.tick(&mut plugin);
    assert_eq!(world.errors().len(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: AutostartSelectingASettingsProfileDoesNotDestroyItsSavedContent
#[test]
fn autostart_selecting_a_settings_profile_does_not_destroy_its_saved_content() {
    let mut world = in_world();
    let mut plugin = world.enable();
    world.vt(&mut plugin, "opt set SpellDiffExcessThreshold-Hunt 77");
    world.vt(&mut plugin, "settings save myprofile");
    let selected = plugin.tank().unwrap().selected_settings_profile();
    let key = format!("mosstank/profiles/{selected}");
    let saved = world.profiles.get(&key).map(str::to_owned).unwrap();
    assert!(!saved.is_empty());

    world.vt(&mut plugin, "settings save otherprofile");
    assert_ne!(plugin.tank().unwrap().selected_settings_profile(), selected);
    world.vt(&mut plugin, "opt set SpellDiffExcessThreshold-Hunt 13");
    world.set_session(&[("settingsProfile", "myprofile")]);

    world.tick(&mut plugin);

    assert_eq!(world.profiles.get(&key), Some(saved.as_str()));
    assert_eq!(plugin.tank().unwrap().selected_settings_profile(), selected);
    assert!(world.errors().is_empty(), "{:?}", world.errors());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: AutostartSelectingANavProfileDoesNotDestroyItsSavedContent
#[test]
fn autostart_selecting_a_nav_profile_does_not_destroy_its_saved_content() {
    let mut world = in_world();
    write_session_proof_fixture(&mut world);
    let mut plugin = world.enable();
    world.vt(&mut plugin, "nav save otherNav");
    // The target carries real waypoints: the proof's own route under the
    // name the autostart asks for.
    let route = proof_fixture("navs/vt-proof-route.af");
    world.profiles.insert("mosstank/navs/myNav.af", &route);
    world.vt(&mut plugin, "nav load otherNav");
    let selected = world.with_tank(&mut plugin, |tank, client| {
        tank.selected_route_profile(client)
    });
    assert_eq!(selected, "otherNav");
    world.set_session(&[("navProfile", "myNav")]);

    world.tick(&mut plugin);

    assert_eq!(
        world.profiles.get("mosstank/navs/myNav.af"),
        Some(route.as_str())
    );
    let selected = world.with_tank(&mut plugin, |tank, client| {
        tank.selected_route_profile(client)
    });
    assert_eq!(selected, "myNav");
    assert_eq!(
        plugin.tank().unwrap().settings().navigation.waypoints.len(),
        4
    );
    assert!(world.errors().is_empty(), "{:?}", world.errors());
}

/// Every file in both storages.
fn snapshot(host: &MemoryHost) -> Vec<(String, String)> {
    let mut files = Vec::new();
    for storage in [&host.plugin, &host.profiles] {
        for key in storage.keys() {
            files.push((
                key.to_owned(),
                storage.get(key).unwrap_or_default().to_owned(),
            ));
        }
    }
    files
}

fn named_host() -> MemoryHost {
    MemoryHost::on("Coldeve")
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: SettingsProfileStoreExistsProbeDoesNotMutateSelectionOrStorage
#[test]
fn settings_profile_store_exists_probe_does_not_mutate_selection_or_storage() {
    let mut host = named_host();
    let mut store = ProfileStore::new(&mut host);
    store.bind_character(&mut host, Some("TestChar")).unwrap();
    let mut settings = AllSettings::default();
    let mut names = Default::default();
    let mut channels = Default::default();
    assert!(
        store
            .create(
                &mut host,
                Some("myprofile"),
                true,
                &mut settings,
                &mut names,
                &mut channels
            )
            .unwrap()
            .0
    );
    assert!(
        store
            .create(
                &mut host,
                Some("otherprofile"),
                true,
                &mut settings,
                &mut names,
                &mut channels
            )
            .unwrap()
            .0
    );
    let selected = store.selected();
    let before = snapshot(&host);

    assert!(store.exists(&mut host, Some("myprofile")).unwrap());
    assert!(!store.exists(&mut host, Some("doesNotExist")).unwrap());

    assert_eq!(store.selected(), selected);
    assert_eq!(before, snapshot(&host));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: ASettingsProfileFileIsSelectableByNameEvenWhileTheMineOnlyPickerHidesIt
#[test]
fn a_settings_profile_file_is_selectable_by_name_even_while_the_mine_only_picker_hides_it() {
    let mut host = named_host();
    host.profiles.insert(
        "mosstank/profiles/vt-proof-settings.usd",
        &proof_fixture("vt-proof-settings.usd"),
    );
    let mut store = ProfileStore::new(&mut host);
    store.bind_character(&mut host, Some("TestChar")).unwrap();

    assert!(store.mine_only());
    assert!(
        !store
            .available_names(&mut host)
            .unwrap()
            .contains(&"vt-proof-settings.usd".to_owned())
    );
    assert!(store.exists(&mut host, Some("vt-proof-settings")).unwrap());
    assert!(store.select(&mut host, Some("vt-proof-settings")).unwrap());
    assert_eq!(store.selected(), "vt-proof-settings.usd");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: ANamedLookupStillRefusesAnotherCharactersReservedProfileFile
#[test]
fn a_named_lookup_still_refuses_another_characters_reserved_profile_file() {
    let mut host = named_host();
    host.profiles.insert(
        "mosstank/profiles/--SomeoneElse_Coldeve.usd",
        &proof_fixture("vt-proof-settings.usd"),
    );
    let mut store = ProfileStore::new(&mut host);
    store.bind_character(&mut host, Some("TestChar")).unwrap();

    assert!(
        !store
            .exists(&mut host, Some("--SomeoneElse_Coldeve"))
            .unwrap()
    );
    assert!(
        !store
            .select(&mut host, Some("--SomeoneElse_Coldeve"))
            .unwrap()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: AutostartAppliesASettingsProfileThatOnlyExistsAsAFileInTheProfileDirectory
#[test]
fn autostart_applies_a_settings_profile_that_only_exists_as_a_file_in_the_profile_directory() {
    let mut world = in_world();
    world.profiles.insert(
        "mosstank/profiles/vt-proof-settings.usd",
        &proof_fixture("vt-proof-settings.usd"),
    );
    let mut plugin = world.enable();
    world.set_session(&[("settingsProfile", "vt-proof-settings")]);

    world.tick(&mut plugin);

    assert!(world.errors().is_empty(), "{:?}", world.errors());
    assert_eq!(
        plugin.tank().unwrap().selected_settings_profile(),
        "vt-proof-settings.usd"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: MetaProfileStoreExistsProbeDoesNotMutateSelectionOrStorage
#[test]
fn meta_profile_store_exists_probe_does_not_mutate_selection_or_storage() {
    let mut host = named_host();
    let mut store = MetaProfileStore::new();
    store.bind_character(&mut host, Some("TestChar")).unwrap();
    let empty = MetaProfile::default();
    assert!(
        store
            .create(&mut host, Some("myMeta"), false, &empty)
            .unwrap()
            .0
    );
    assert!(
        store
            .create(&mut host, Some("otherMeta"), false, &empty)
            .unwrap()
            .0
    );
    let selected = store.selected(&mut host).unwrap();
    let before = snapshot(&host);

    assert!(store.exists(&mut host, Some("myMeta")).unwrap());
    assert!(!store.exists(&mut host, Some("doesNotExist")).unwrap());

    assert_eq!(store.selected(&mut host).unwrap(), selected);
    assert_eq!(before, snapshot(&host));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: RouteProfileStoreExistsProbeDoesNotMutateSelectionOrStorage
#[test]
fn route_profile_store_exists_probe_does_not_mutate_selection_or_storage() {
    let mut host = named_host();
    let mut store = RouteProfileStore::new();
    store.bind_character(&mut host, Some("TestChar")).unwrap();
    let route = mosstank_formats::route::NavigationSettings::default();
    assert!(
        store
            .create(&mut host, Some("myNav"), true, &route)
            .unwrap()
            .0
    );
    assert!(
        store
            .create(&mut host, Some("otherNav"), true, &route)
            .unwrap()
            .0
    );
    let selected = store.selected(&mut host).unwrap();
    let before = snapshot(&host);

    assert!(store.exists(&mut host, Some("myNav")).unwrap());
    assert!(!store.exists(&mut host, Some("doesNotExist")).unwrap());

    assert_eq!(store.selected(&mut host).unwrap(), selected);
    assert_eq!(before, snapshot(&host));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: LootProfileStoreExistsProbeDoesNotMutateSelectionOrStorage
#[test]
fn loot_profile_store_exists_probe_does_not_mutate_selection_or_storage() {
    let mut host = named_host();
    let mut store = LootProfileStore::new();
    store.bind_character(&mut host, Some("TestChar")).unwrap();
    assert!(
        store
            .create(&mut host, Some("myLoot"), false, &[], None)
            .unwrap()
            .0
    );
    assert!(
        store
            .create(&mut host, Some("otherLoot"), false, &[], None)
            .unwrap()
            .0
    );
    let selected = store.selected();
    let before = snapshot(&host);

    assert!(store.exists(&mut host, Some("myLoot")).unwrap());
    assert!(!store.exists(&mut host, Some("doesNotExist")).unwrap());

    assert_eq!(store.selected(), selected);
    assert_eq!(before, snapshot(&host));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: AutostartAppliesTheSessionProofsOwnAuthoredProfilesWithNoSidecar
#[test]
fn autostart_applies_the_session_proofs_own_authored_profiles_with_no_sidecar() {
    let mut world = in_world();
    write_session_proof_fixture(&mut world);
    let mut plugin = world.enable();
    world.set_session(&[
        ("settingsProfile", "vt-proof-settings"),
        ("lootProfile", "vt-proof-loot"),
        ("navProfile", "vt-proof-route"),
        ("enableMeta", "false"),
        ("startMacro", "true"),
    ]);

    world.tick(&mut plugin);

    assert!(world.errors().is_empty(), "{:?}", world.errors());
    let tank = plugin.tank().unwrap();
    assert_eq!(tank.selected_settings_profile(), "vt-proof-settings.usd");
    assert_eq!(tank.selected_loot_profile(), "vt-proof-loot");
    for option in [
        "enablebuffing",
        "enablecombat",
        "enablenav",
        "enablelooting",
    ] {
        assert!(tank.option(option).is_truthy(), "{option}");
    }
    assert_eq!(tank.settings().navigation.waypoints.len(), 4);
    assert_eq!(
        tank.settings().navigation.mode,
        mosstank_formats::route::RouteMode::Circular
    );
    assert!(tank.macro_running());
    let selected = world.with_tank(&mut plugin, |tank, client| {
        tank.selected_route_profile(client)
    });
    assert_eq!(selected, "vt-proof-route");
    let chat = world.chat();
    assert!(
        chat.iter().any(|line| line.contains("Macro started.")),
        "{chat:?}"
    );
    assert!(
        !chat
            .iter()
            .any(|line| line.contains("Macro stopped.") || line.contains("Not in world"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: AutostartTakesTheProofsWandFromTheCompanionDocumentBesideItsUsd
#[test]
fn autostart_takes_the_proofs_wand_from_the_companion_document_beside_its_usd() {
    const SIDECAR: &str = "profiles/macro/sidecar/vt-proof-settings.usd.json";
    let mut world = in_world();
    write_session_proof_fixture(&mut world);
    world.plugin_data.insert(
        SIDECAR,
        &proof_fixture(&format!("plugin-storage/acdream.mosstank/{SIDECAR}")),
    );
    let mut plugin = world.enable();
    world.set_session(&[("settingsProfile", "vt-proof-settings")]);

    world.tick(&mut plugin);

    assert!(world.errors().is_empty(), "{:?}", world.errors());
    let tank = plugin.tank().unwrap();
    assert_eq!(tank.selected_settings_profile(), "vt-proof-settings.usd");
    assert!(tank.settings().combat.combat_item_names.contains("Wand"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: ARouteProfileThatWillNotParseKeepsItsFileAndIsNotReportedLoaded
#[test]
fn a_route_profile_that_will_not_parse_keeps_its_file_and_is_not_reported_loaded() {
    const UNREADABLE: &str = "~~ {\nNAV: broken 4\n";
    let mut world = in_world();
    world
        .profiles
        .insert("mosstank/navs/vt-broken-route.af", UNREADABLE);
    write_session_proof_fixture(&mut world);
    let mut plugin = world.enable();
    world.set_session(&[("navProfile", "vt-proof-route")]);
    world.tick(&mut plugin);
    assert_eq!(
        plugin.tank().unwrap().settings().navigation.waypoints.len(),
        4
    );
    world.fake.logs.clear();

    world.set_session(&[("navProfile", "vt-broken-route")]);
    world.fake.character = None;
    world.tick(&mut plugin);
    world.fake.character = Some(character("TestChar"));
    world.tick(&mut plugin);

    assert_eq!(
        world.profiles.get("mosstank/navs/vt-broken-route.af"),
        Some(UNREADABLE)
    );
    assert!(
        !world
            .infos()
            .iter()
            .any(|line| line.contains("Loaded route profile")
                || (line.contains("vt-broken-route") && line.contains("oaded")))
    );
    assert!(
        world
            .errors()
            .iter()
            .any(|line| line.contains("vt-broken-route"))
    );
    assert!(
        plugin
            .tank()
            .unwrap()
            .route_notice()
            .contains("could not be read")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: AutostartOpensTheDeclaredLogChannelsBeforeItStartsTheMacro
#[test]
fn autostart_opens_the_declared_log_channels_before_it_starts_the_macro() {
    let mut world = in_world();
    let mut plugin = world.enable();
    world.set_session(&[
        ("logChannels", "misc, RuleInfo activerule"),
        ("startMacro", "true"),
    ]);

    world.tick(&mut plugin);
    world.tick(&mut plugin);

    assert!(world.errors().is_empty(), "{:?}", world.errors());
    assert!(plugin.tank().unwrap().macro_running());
    assert!(world.infos().iter().any(|line| line
        .contains("[vt log ActiveRule] ----------- Primary logic loop started")));
    world.vt(&mut plugin, "log");
    assert!(world.chat().iter().any(|line| line.contains("ActiveRule")
        && line.contains("Misc")
        && line.contains("RuleInfo")));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: AutostartWaitsForTheCharactersNameAndItsChoiceThenSurvivesTheBind
#[test]
fn autostart_waits_for_the_characters_name_and_its_choice_then_survives_the_bind() {
    let mut world = World::with_character("");
    write_session_proof_fixture(&mut world);
    let mut plugin = world.enable();
    world.set_session(&[
        ("settingsProfile", "vt-proof-settings"),
        ("navProfile", "vt-proof-route"),
        ("logChannels", "RuleInfo"),
    ]);

    world.tick(&mut plugin);
    assert_ne!(
        plugin.tank().unwrap().selected_settings_profile(),
        "vt-proof-settings.usd"
    );

    world.fake.character = Some(character("TestChar"));
    world.tick(&mut plugin);
    assert_eq!(
        plugin.tank().unwrap().selected_settings_profile(),
        "vt-proof-settings.usd"
    );
    assert_eq!(
        plugin.tank().unwrap().settings().navigation.waypoints.len(),
        4
    );

    world.tick(&mut plugin);
    world.tick(&mut plugin);

    let tank = plugin.tank().unwrap();
    assert_eq!(tank.selected_settings_profile(), "vt-proof-settings.usd");
    assert_eq!(tank.settings().navigation.waypoints.len(), 4);
    assert!(tank.option("enablenav").is_truthy());
    let selected = world.with_tank(&mut plugin, |tank, client| {
        tank.selected_route_profile(client)
    });
    assert_eq!(selected, "vt-proof-route");
    world.vt(&mut plugin, "log");
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("Log state") && line.contains("RuleInfo"))
    );
    assert!(world.errors().is_empty(), "{:?}", world.errors());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: AutostartSelectingAMetaProfileDoesNotDestroyItsSavedContentOrRuleCount
#[test]
fn autostart_selecting_a_meta_profile_does_not_destroy_its_saved_content_or_rule_count() {
    // The C# builds myMeta in the meta editor (the panel's): one rule that
    // runs setvar[`x`,1]. Here it is written as the editor saves it.
    let profile = MetaProfile {
        rules: vec![MetaRule {
            action: MetaAction {
                kind: MetaActionKind::ExpressionAction,
                text: "setvar[`x`,1]".to_owned(),
                ..MetaAction::default()
            },
            ..MetaRule::default()
        }],
    };
    let meta = mosstank_formats::met::save(&profile).unwrap();
    let mut world = in_world();
    world.profiles.insert("mosstank/metas/myMeta.met", &meta);
    let mut plugin = world.enable();
    world.vt(&mut plugin, "meta load myMeta");
    assert_eq!(plugin.tank().unwrap().meta_rule_count(), 1);
    // A meta that does not exist is made, empty, and selected (the C#
    // presses New in the meta tab).
    world.vt(&mut plugin, "meta load otherMeta");
    assert_eq!(plugin.tank().unwrap().meta_rule_count(), 0);
    let selected = world.with_tank(&mut plugin, |tank, client| {
        tank.selected_meta_profile(client)
    });
    assert_eq!(selected, "otherMeta");
    world.set_session(&[("metaProfile", "myMeta")]);

    world.tick(&mut plugin);

    assert_eq!(
        world.profiles.get("mosstank/metas/myMeta.met"),
        Some(meta.as_str())
    );
    let selected = world.with_tank(&mut plugin, |tank, client| {
        tank.selected_meta_profile(client)
    });
    assert_eq!(selected, "myMeta");
    assert_eq!(plugin.tank().unwrap().meta_rule_count(), 1);
    assert!(world.errors().is_empty(), "{:?}", world.errors());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankAutostartTests.cs :: AutostartSelectingALootProfileDoesNotDestroyItsSavedContent
#[test]
fn autostart_selecting_a_loot_profile_does_not_destroy_its_saved_content() {
    // The loot profile with a rule is an authored one (the C# adds it in the
    // loot editor, which comes with the panel).
    let loot = proof_fixture("vt-proof-loot.utl");
    let mut world = in_world();
    world.profiles.insert("mosstank/loot/myLoot.utl", &loot);
    let mut plugin = world.enable();
    world.vt(&mut plugin, "loot load myLoot");
    assert_eq!(plugin.tank().unwrap().live_loot_rules().len(), 1);
    world.vt(&mut plugin, "loot new otherLoot");
    assert!(plugin.tank().unwrap().live_loot_rules().is_empty());
    assert_eq!(plugin.tank().unwrap().selected_loot_profile(), "otherLoot");
    world.set_session(&[("lootProfile", "myLoot")]);

    world.tick(&mut plugin);

    assert_eq!(
        world.profiles.get("mosstank/loot/myLoot.utl"),
        Some(loot.as_str())
    );
    assert_eq!(plugin.tank().unwrap().selected_loot_profile(), "myLoot");
    assert_eq!(plugin.tank().unwrap().live_loot_rules().len(), 1);
    assert!(world.errors().is_empty(), "{:?}", world.errors());
}

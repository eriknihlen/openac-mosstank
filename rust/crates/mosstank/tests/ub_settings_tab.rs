//! The UB settings page as the rest of the plugin reads it: the sharing
//! rows the combat controller follows, the rows whose value belongs to a
//! running owner, the handler rows that are the meta profile, and where
//! the UB tree is kept (ported from the C#'s
//! `MossTankPanelTests.UbSettingsTab`).

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::peers::PeerCast;
use common::World;
use mosstank::MossTank;
use mosstank_stores::ub_store;
use mosstank_stores::ub_tree_migration::LEGACY_ROOT;

/// `UbPanel`: Acdream on Coldeve.
fn ub_world() -> World {
    let mut world = World::panel_character("Acdream");
    world.fake.character.as_mut().unwrap().world = "Coldeve".to_owned();
    world
}

/// The other clients on this computer, through a hub that is there.
fn with_peers(world: &mut World, casts: Vec<PeerCast>) {
    world.set_reply::<calls::PeersSubscribe>(&Some(7));
    world.set_reply::<calls::PeersIsConnected>(&true);
    world.set_reply::<calls::PeersCasts>(&casts);
    world.set_reply::<calls::PeersClients>(&Vec::new());
}

/// `PeerNetworkProbe.CastCaptures`.
fn cast_captures(world: &World) -> usize {
    world.requests::<calls::PeersCasts>().len()
}

/// `PeerNetworkProbe.ClientCaptures`.
fn client_captures(world: &World) -> usize {
    world.requests::<calls::PeersClients>().len()
}

fn tick(world: &mut World, plugin: &mut MossTank) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, 0.1));
}

fn values(world: &mut World, plugin: &mut MossTank) -> Vec<String> {
    world.ui_list(plugin, "UbSettingValues")
}

/// `new PluginPeerCast(sequence, 7, 0x50000002, 10, 70, 300, seconds, true)`.
fn cast(sequence: i64, seconds_remaining: f64) -> PeerCast {
    PeerCast {
        sequence,
        client_id: 7,
        caster_id: 0x5000_0002,
        target_id: 10,
        spell_id: 70,
        effective_skill: 300,
        seconds_remaining,
        landed: true,
        ..PeerCast::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: TheSharingSwitchOnThePageGatesWhatIsReadFromTheOtherClients
#[test]
fn the_sharing_switch_on_the_page_gates_what_is_read_from_the_other_clients() {
    let mut world = ub_world();
    with_peers(&mut world, Vec::new());
    let mut plugin = world.enable();
    let p = &mut plugin;

    tick(&mut world, p);
    assert_eq!(cast_captures(&world), 0);
    world.act(p, "ToggleUbVitalSharing");
    tick(&mut world, p);
    assert_eq!(cast_captures(&world), 1);

    world.act_text(p, "SetUbFilterText", "Sharing.Vitals");
    assert_eq!(values(&mut world, p)[0], "True");
    world.act_int(p, "ClickUbSettingValue", 0);
    assert_eq!(values(&mut world, p)[0], "False");
    tick(&mut world, p);
    tick(&mut world, p);
    assert_eq!(cast_captures(&world), 1);

    world.act_int(p, "ClickUbSettingValue", 0);
    tick(&mut world, p);
    assert_eq!(cast_captures(&world), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: TheCastTagOnThePageReachesTheControllerThatFiltersThePeers
/// The C# probe answers only the casts after the sequence asked from; the
/// fake answers what it is given, so the second look is given both.
#[test]
fn the_cast_tag_on_the_page_reaches_the_controller_that_filters_the_peers() {
    let mut world = ub_world();
    with_peers(&mut world, vec![cast(1, 45.0)]);
    let mut plugin = world.enable();
    let p = &mut plugin;

    world.act(p, "ToggleUbVitalSharing");
    tick(&mut world, p);
    assert_eq!(cast_captures(&world), 1);
    assert_eq!(client_captures(&world), 0);

    world.act_text(p, "SetUbFilterText", "Sharing.CastTag");
    world.act_int(p, "SelectUbSetting", 0);
    assert_eq!(world.ui_text(p, "UbSettingValueDraft"), "");
    world.act_text(p, "SubmitUbSetting", "debuffs");
    assert_eq!(values(&mut world, p)[0], "debuffs");

    world.set_reply::<calls::PeersCasts>(&vec![cast(2, 40.0)]);
    tick(&mut world, p);
    assert_eq!(cast_captures(&world), 2);
    assert_eq!(client_captures(&world), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: ASettingWithNoEditorIsNowEditedAndComesBackAfterAReload
/// Five settings that had nowhere to change them: the four numbers that
/// pace handing items over, and how many times a jump is tried. Their rows
/// reach the running settings rather than a second copy in the UB files.
#[test]
fn a_setting_with_no_editor_is_now_edited_and_comes_back_after_a_reload() {
    for (name, shipped, typed, stored_as) in [
        ("ItemGiver.Range", "15", "22", "InventoryGiveRangeMeters"),
        ("ItemGiver.Delay", "0", "1.5", "InventoryGiveDelaySeconds"),
        (
            "ItemGiver.BusyRetryLimit",
            "10",
            "7",
            "InventoryGiveBusyRetryLimit",
        ),
        (
            "ItemGiver.FailureLimit",
            "3",
            "5",
            "InventoryGiveFailureLimit",
        ),
        ("Jumper.Attempts", "3", "8", "NavigationJumpAttempts"),
    ] {
        let mut world = ub_world();
        let mut plugin = world.enable();
        let p = &mut plugin;
        world.act_text(p, "SetUbFilterText", name);
        world.act_int(p, "SelectUbSetting", 0);
        assert_eq!(values(&mut world, p)[0], shipped, "{name}");

        world.act_text(p, "SubmitUbSetting", typed);
        assert_eq!(values(&mut world, p)[0], typed, "{name}");

        // It went to the profile the macro actually reads, not to a second
        // copy of the number in the new settings tree.
        let side_cars: Vec<&str> = world
            .plugin_data
            .keys()
            .chain(world.profiles.keys())
            .filter(|key| key.contains("sidecar"))
            .collect();
        assert_eq!(side_cars.len(), 1, "{side_cars:?}");
        let side_car = world
            .plugin_data
            .get(side_cars[0])
            .or_else(|| world.profiles.get(side_cars[0]))
            .unwrap();
        assert!(side_car.contains(stored_as), "{name}: {side_car}");
        assert!(
            !world
                .profiles
                .keys()
                .any(|key| key.starts_with(ub_store::ROOT)
                    && world.profiles.get(key).unwrap().contains(name)),
            "{name}"
        );
        world.disable(p);

        let mut reloaded = world.enable();
        world.act_text(&mut reloaded, "SetUbFilterText", name);
        assert_eq!(values(&mut world, &mut reloaded)[0], typed, "{name}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: PuttingALiveOwnedRowBackToItsDefaultWritesTheDefaultIntoIt
#[test]
fn putting_a_live_owned_row_back_to_its_default_writes_the_default_into_it() {
    let mut world = ub_world();
    let mut plugin = world.enable();
    let p = &mut plugin;
    world.act_text(p, "SetUbFilterText", "Jumper.Attempts");
    world.act_int(p, "SelectUbSetting", 0);
    world.act_text(p, "SubmitUbSetting", "8");

    world.act(p, "ResetUbSettingToDefault");

    assert_eq!(values(&mut world, p)[0], "3");
    world.disable(p);
    let mut reloaded = world.enable();
    world.act_text(&mut reloaded, "SetUbFilterText", "Jumper.Attempts");
    assert_eq!(values(&mut world, &mut reloaded)[0], "3");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: ExactlyTheSettingsWithALiveOwnerStayOutOfTheNewFiles
/// Saving every row in turn and reading the UB files back: the rows missing
/// from them are exactly those whose value belongs to something running.
#[test]
fn exactly_the_settings_with_a_live_owner_stay_out_of_the_new_files() {
    let mut world = ub_world();
    let mut plugin = world.enable();
    let p = &mut plugin;
    let names = world.ui_list(p, "UbSettingNames");
    assert_eq!(names.len(), 168);

    for row in 0..names.len() {
        world.act_int(p, "SelectUbSetting", i32::try_from(row).unwrap());
        world.act(p, "SubmitUbSettingDraft");
    }

    let files: Vec<String> = world
        .profiles
        .keys()
        .filter(|key| key.starts_with(ub_store::ROOT))
        .map(|key| world.profiles.get(key).unwrap().to_owned())
        .collect();
    let files = files.join("\n");
    let mut absent: Vec<&String> = names
        .iter()
        .filter(|name| !files.contains(&format!("\"{name}\"")))
        .collect();
    absent.sort();

    assert_eq!(
        absent,
        [
            "Aliases.DefinedAliases",
            "GameEvents.Handlers",
            "GameEvents.Profile",
            "ItemGiver.BusyRetryLimit",
            "ItemGiver.Delay",
            "ItemGiver.FailureLimit",
            "ItemGiver.Range",
            "Jumper.Attempts",
        ]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: HandlerLinesAreMetaRulesAndMetaRulesAreHandlerLines
/// The handlers row is an editor over the meta profile, not a list of its
/// own.
#[test]
fn handler_lines_are_meta_rules_and_meta_rules_are_handler_lines() {
    let mut world = ub_world();
    let mut plugin = world.enable();
    let p = &mut plugin;

    world.act_text(p, "SetUbFilterText", "GameEvents.Handlers");
    world.act_int(p, "ClickUbSettingValue", 0);
    world.act_text(p, "AddUbListEntry", "LoginComplete = /framerate");

    assert_eq!(
        world.ui_list(p, "UbListRows"),
        ["LoginComplete = /framerate"]
    );
    assert_eq!(world.ui_list(p, "MetaRows").len(), 1);
    assert_eq!(world.ui_list(p, "MetaStateColumn")[0], "Default");
    assert!(world.ui_list(p, "MetaConditionColumn")[0].contains("LoginComplete"));
    assert!(world.ui_list(p, "MetaActionColumn")[0].contains("/framerate"));

    world.act(p, "HideUbListEditor");
    world.act_text(p, "SetMetaStateDraft", "Default");
    world.act_text(p, "SelectMetaCondition", "CharacterDeath");
    world.act_text(p, "SelectMetaAction", "ChatCommand");
    world.act_text(p, "SetMetaActionTextDraft", "/f I died");
    world.act(p, "AddMetaRule");
    assert_eq!(
        values(&mut world, p)[0],
        "LoginComplete = /framerate, CharacterDeath = /f I died"
    );
    world.disable(p);

    let mut next = world.enable();
    world.act_text(&mut next, "SetUbFilterText", "GameEvents.Handlers");
    assert_eq!(
        values(&mut world, &mut next)[0],
        "LoginComplete = /framerate, CharacterDeath = /f I died"
    );
    assert_eq!(world.ui_list(&mut next, "MetaRows").len(), 2);
    assert!(
        !world
            .profiles
            .keys()
            .chain(world.plugin_data.keys())
            .any(|key| {
                world
                    .profiles
                    .get(key)
                    .or_else(|| world.plugin_data.get(key))
                    .unwrap()
                    .contains("GameEvents.Handlers")
            })
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: NoUbFileOrFolderEverLandsInThePluginsOwnStorage
/// The UB tree belongs to the person: it lives in the shared profile
/// folder, and nothing UB-shaped lands in the plugin's own storage.
#[test]
fn no_ub_file_or_folder_ever_lands_in_the_plugins_own_storage() {
    let mut world = ub_world();
    let mut plugin = world.enable();
    let p = &mut plugin;

    // A login's folder tree, a settings write, the marker file the map lays
    // down when its folder is empty, and an equip profile save.
    tick(&mut world, p);
    world.act_text(p, "SetUbFilterText", "Sharing.Vitals");
    world.act_int(p, "ClickUbSettingValue", 0);
    world.act(p, "ShowUbMap");
    world.command(p, "ub", "equip create pin");

    let legacy = |key: &str| key.to_ascii_lowercase().starts_with(LEGACY_ROOT);
    assert!(!world.plugin_data.keys().any(legacy));
    assert!(
        !world
            .plugin_data
            .directories
            .iter()
            .any(|folder| legacy(folder))
    );

    // And the same writes did land, so the assertions above are not green
    // because nothing happened at all.
    assert!(
        world
            .profiles
            .keys()
            .any(|key| key.starts_with(ub_store::ROOT))
    );
    assert!(
        world
            .profiles
            .contains(&format!("{}maps/markers.csv", ub_store::ROOT))
    );
    assert!(
        world
            .profiles
            .keys()
            .any(|key| key.starts_with(ub_store::ROOT) && key.ends_with("/equip/pin.utl"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: TheUbTreeLeftInThePluginsOwnStorageComesAcrossOnce
/// What an earlier version left in the plugin's own storage comes across on
/// the first start that finds it, and the person is told once, in one line.
#[test]
fn the_ub_tree_left_in_the_plugins_own_storage_comes_across_once() {
    let mut world = ub_world();
    world
        .plugin_data
        .insert("ub/maps/markers.csv", "Town, Holtburg, 42.1, 33.6");
    world
        .plugin_data
        .insert("ub/Coldeve/Acdream/settings.json", "{}");
    let mut plugin = world.enable();
    tick(&mut world, &mut plugin);

    assert_eq!(
        world
            .profiles
            .get(&format!("{}maps/markers.csv", ub_store::ROOT)),
        Some("Town, Holtburg, 42.1, 33.6")
    );
    assert_eq!(
        world
            .profiles
            .get(&format!("{}Coldeve/Acdream/settings.json", ub_store::ROOT)),
        Some("{}")
    );
    let lines: Vec<String> = world
        .infos()
        .into_iter()
        .filter(|message| message.contains("UtilityBelt file(s)"))
        .collect();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains("copied 2 UtilityBelt file(s)"),
        "{}",
        lines[0]
    );
}

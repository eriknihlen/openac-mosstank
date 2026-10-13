//! What the shell keeps of a session beside its rules (ported from the
//! C#'s `MossTankPanelTests.ActionHistory`, `.GameInfo`, `.Status` and
//! `.StorageLayout`): the action history and the debug log, the game
//! database's own check and its commands, the meta state other plugins
//! read, and the folders a start lays out.

mod common;

use std::rc::Rc;

use ac_plugin_abi::Event;
use ac_plugin_abi::calls;
use ac_plugin_abi::net::{HttpReply, NetError, NetEvent};
use common::{World, character};
use mosstank::MossTank;
use mosstank_formats::gameinfo::{FILE_NAME, VtankGameInfoDatabase};
use mosstank_formats::usd::{VtankCell, VtankDatabase, VtankRow};
use mosstank_session::game_info::{LAST_CHECK_KEY, VtankGameInfoAnswer};
use mosstank_session::testing::download;
use mosstank_stores::layout;

fn tank(plugin: &MossTank) -> &mosstank::tank::Tank {
    plugin.tank().expect("an enabled plugin")
}

/// `ToggleCombat`.
fn toggle_combat(world: &mut World, plugin: &mut MossTank) {
    let running = tank(plugin).macro_running();
    world.with_tank(plugin, |tank, client| {
        tank.set_macro_running(client, !running)
    });
}

fn on_tick(world: &mut World, plugin: &mut MossTank, seconds: f64) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, seconds));
}

// ---- MossTankPanelTests.ActionHistory ------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ActionHistory.cs :: DebugButtonSavesHistoryAndStopsWritingWhenDisabled
#[test]
fn debug_button_saves_history_and_stops_writing_when_disabled() {
    let mut world = World::with_character("Barris");
    let mut plugin = world.enable();
    world.with_tank(&mut plugin, |tank, client| tank.toggle_debug_log(client));
    let keys: Vec<String> = world
        .plugin_data
        .keys()
        .filter(|key| key.starts_with("debug/"))
        .map(str::to_owned)
        .collect();
    assert_eq!(keys.len(), 1, "{keys:?}");
    let key = &keys[0];
    assert!(world.plugin_data.get(key).unwrap().contains("Combat off"));
    assert_eq!(tank(&plugin).debug_log_button_text(), "Debug log: ON");
    toggle_combat(&mut world, &mut plugin);
    world.with_tank(&mut plugin, |tank, client| tank.toggle_debug_log(client));
    let saved = world.plugin_data.get(key).unwrap().to_owned();
    assert!(saved.contains("Logging stopped."));
    toggle_combat(&mut world, &mut plugin);
    assert_eq!(world.plugin_data.get(key), Some(saved.as_str()));
    assert_eq!(tank(&plugin).debug_log_button_text(), "Debug log: off");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ActionHistory.cs :: ActivityIncludesNavigationAndDoesNotRepeatUnchangedState
#[test]
fn activity_includes_navigation_and_does_not_repeat_unchanged_state() {
    let mut world = World::with_character("Barris");
    let mut plugin = world.enable();
    toggle_combat(&mut world, &mut plugin);
    on_tick(&mut world, &mut plugin, 0.5);
    let navigation = |plugin: &MossTank| {
        tank(plugin)
            .action_history_lines()
            .iter()
            .filter(|line| line.contains("Navigation:"))
            .count()
    };
    let count = navigation(&plugin);
    assert!(count > 0);
    on_tick(&mut world, &mut plugin, 0.5);
    assert_eq!(navigation(&plugin), count);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ActionHistory.cs :: RecordsEveryTransitionWhileClosedWithoutDependingOnTicksOrReads
#[test]
fn records_every_transition_while_closed_without_depending_on_ticks_or_reads() {
    let mut world = World::with_character("Barris");
    let mut plugin = world.enable();
    assert!(!tank(&plugin).action_history_visible());
    let lines = tank(&plugin).action_history_lines();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].ends_with("Combat off"));
    toggle_combat(&mut world, &mut plugin);
    let running = tank(&plugin).combat_status().to_owned();
    toggle_combat(&mut world, &mut plugin);
    assert!(!tank(&plugin).action_history_visible());
    let lines = tank(&plugin).action_history_lines();
    assert!(lines.iter().any(|line| line.ends_with(&running)));
    assert!(
        lines
            .last()
            .unwrap()
            .ends_with(tank(&plugin).combat_status())
    );
    assert!(lines.len() >= 3);
    let snapshot = tank(&plugin).action_history_lines();
    world.with_tank(&mut plugin, |tank, _| tank.show_action_history());
    assert!(tank(&plugin).action_history_visible());
    world.with_tank(&mut plugin, |tank, _| tank.hide_action_history());
    assert!(!tank(&plugin).action_history_visible());
    assert!(Rc::ptr_eq(&snapshot, &tank(&plugin).action_history_lines()));
    let current = tank(&plugin).combat_status().to_owned();
    world.with_tank(&mut plugin, |tank, _| tank.clear_action_history());
    assert!(tank(&plugin).action_history_lines().is_empty());
    assert_eq!(tank(&plugin).combat_status(), current);
    toggle_combat(&mut world, &mut plugin);
    assert!(!tank(&plugin).action_history_lines().is_empty());
}

// ---- MossTankPanelTests.GameInfo -----------------------------------------

/// The handle the fake client gives every request.
const REQUEST: u32 = 41;

/// In the world with an HTTP client that takes every request.
fn online_world() -> World {
    let mut world = World::with_character("Barris");
    world.fake.clock.utc_unix_ms = 1_790_000_000_000;
    world.set_reply::<calls::NetHttpRequest>(&Ok(REQUEST));
    world
}

fn requests(world: &World) -> usize {
    world.requests::<calls::NetHttpRequest>().len()
}

/// The client's answer to the check's request.
fn answer(world: &mut World, plugin: &mut MossTank, answer: &VtankGameInfoAnswer) {
    let reply = if answer.succeeded {
        HttpReply {
            handle: REQUEST,
            status: 200,
            reason: "OK".to_owned(),
            body: answer.text.as_bytes().to_vec(),
            ..HttpReply::default()
        }
    } else {
        HttpReply {
            handle: REQUEST,
            error: NetError::Connect,
            message: answer.text.clone(),
            ..HttpReply::default()
        }
    };
    world.send(plugin, Event::Net(NetEvent::HttpCompleted(reply)));
}

/// A download from 2022-11-06 with one heal kit (`UpdateWithAHealKit`).
fn update_with_a_heal_kit() -> VtankGameInfoAnswer {
    let change = |database: &mut VtankDatabase| {
        database.find_mut("HealKits").unwrap().rows.push(VtankRow {
            cells: vec![
                VtankCell::string("Tested Healing Kit"),
                VtankCell::double(1.5),
                VtankCell::int(10),
                VtankCell::int(0),
            ],
        });
    };
    download(1_667_721_149, Some(&change), &[])
}

fn readers(plugin: &MossTank) -> Vec<VtankGameInfoDatabase> {
    tank(plugin)
        .game_info_readers()
        .iter()
        .map(|database| (*database).clone())
        .collect()
}

fn excerpt() -> String {
    mosstank_formats::fixtures::read_vtank("gameinfodb-excerpt.ugd")
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.GameInfo.cs :: TheSessionChecksTheGameDatabaseOnceAndEveryReaderTakesTheNewOne
#[test]
fn the_session_checks_the_game_database_once_and_every_reader_takes_the_new_one() {
    let mut world = online_world();
    let mut plugin = world.enable();
    let before = readers(&plugin)[0].clone();

    on_tick(&mut world, &mut plugin, 0.1);
    answer(&mut world, &mut plugin, &update_with_a_heal_kit());
    on_tick(&mut world, &mut plugin, 0.1);
    on_tick(&mut world, &mut plugin, 0.1);

    assert!(!tank(&plugin).game_info_update_running());
    assert_eq!(requests(&world), 1);
    let said: Vec<String> = world
        .chat()
        .into_iter()
        .filter(|line| line.starts_with("[MossTank] Game database"))
        .collect();
    assert_eq!(
        said,
        ["[MossTank] Game database updated to 2022-11-06 from openac-gamedata."]
    );
    let readers = readers(&plugin);
    assert_ne!(before, readers[0]);
    assert!(readers.iter().all(|reader| *reader == readers[0]));
    assert!(tank(&plugin).has_heal_kit("Tested Healing Kit"));
    assert!(world.profiles.get(FILE_NAME).is_some());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.GameInfo.cs :: EachNewSessionChecksTheGameDatabaseAgain
#[test]
fn each_new_session_checks_the_game_database_again() {
    let mut world = online_world();
    let mut plugin = world.enable();
    world.vt(&mut plugin, "gamedb interval 0");

    on_tick(&mut world, &mut plugin, 0.1);
    answer(&mut world, &mut plugin, &download(1, None, &[]));
    on_tick(&mut world, &mut plugin, 0.1);
    world.fake.character = None;
    on_tick(&mut world, &mut plugin, 0.1);
    world.fake.character = Some(character("Barris"));
    on_tick(&mut world, &mut plugin, 0.1);
    assert!(tank(&plugin).game_info_update_running());

    assert_eq!(requests(&world), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.GameInfo.cs :: AnUnreachableSourceKeepsTheDatabaseTheSessionHad
#[test]
fn an_unreachable_source_keeps_the_database_the_session_had() {
    let mut world = online_world();
    world.profiles.insert(FILE_NAME, &excerpt());
    let mut plugin = world.enable();
    let before = readers(&plugin)[0].clone();

    on_tick(&mut world, &mut plugin, 0.1);
    answer(
        &mut world,
        &mut plugin,
        &VtankGameInfoAnswer::new(false, "No such host is known."),
    );
    on_tick(&mut world, &mut plugin, 0.1);

    assert!(world.chat().contains(
        &"[MossTank] Game database update failed: No such host is known. The game database you had is kept."
            .to_owned()
    ));
    assert!(readers(&plugin).iter().all(|reader| *reader == before));
    assert_eq!(world.profiles.get(FILE_NAME), Some(excerpt().as_str()));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.GameInfo.cs :: GameDbCommandsCheckNowAndReportWhatIsLoaded
#[test]
fn game_db_commands_check_now_and_report_what_is_loaded() {
    let mut world = online_world();
    let mut plugin = world.enable();

    world.vt(&mut plugin, "gamedb");
    let chat = world.chat();
    assert!(chat.contains(
        &"[MossTank] Game database: no gameinfodb.ugd in the VTank profile folder; /vt gamedb update \
          downloads it. Loaded: version 9, world data of none. Ammunition 0, \
          monster damage 0, species damage 0, species members 0, immunities 0, \
          heal kits 0, grenades 0, drain spells 0, martyr spells 0, craft recipes 0."
            .to_owned()
    ), "{chat:#?}");
    assert!(chat.contains(
        &"[MossTank] Game database: checked every 6 hours; the next login checks.".to_owned()
    ));

    // The login's own check, then the two asked for.
    on_tick(&mut world, &mut plugin, 0.1);
    answer(&mut world, &mut plugin, &update_with_a_heal_kit());
    on_tick(&mut world, &mut plugin, 0.1);
    world.vt(&mut plugin, "gamedb update");
    answer(&mut world, &mut plugin, &update_with_a_heal_kit());
    assert!(
        world
            .chat()
            .contains(&"[MossTank] Checking the game database for updates.".to_owned())
    );
    on_tick(&mut world, &mut plugin, 0.1);
    world.vt(&mut plugin, "getdb");
    answer(&mut world, &mut plugin, &update_with_a_heal_kit());
    on_tick(&mut world, &mut plugin, 0.1);
    world.clear_chat();
    world.vt(&mut plugin, "gamedb");

    assert_eq!(requests(&world), 3);
    let chat = world.chat();
    assert_eq!(chat.len(), 2, "{chat:#?}");
    assert!(chat[0].starts_with(
        "[MossTank] Game database: gameinfodb.ugd in the VTank profile folder. Loaded: version 9, \
         world data of 2022-11-06."
    ), "{}", chat[0]);
    assert!(chat[0].contains("heal kits 1,"));
    assert!(
        chat[1].starts_with(
            "[MossTank] Game database: checked every 6 hours; the next check is after "
        )
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.GameInfo.cs :: ACheckRunningAtSessionEndIsHandedOverAndTheNextSessionAsksToo
#[test]
fn a_check_running_at_session_end_is_handed_over_and_the_next_session_asks_too() {
    let mut world = online_world();
    let mut plugin = world.enable();
    // No freshness window, so the second session's check reaches the source.
    world.vt(&mut plugin, "gamedb interval 0");

    on_tick(&mut world, &mut plugin, 0.1);
    assert_eq!(tank(&plugin).game_info_request(), Some(REQUEST));
    world.fake.character = None;
    on_tick(&mut world, &mut plugin, 0.1);
    world.fake.character = Some(character("Barris"));
    on_tick(&mut world, &mut plugin, 0.1);
    assert_eq!(tank(&plugin).game_info_request(), Some(REQUEST));
    assert_eq!(requests(&world), 1);

    answer(&mut world, &mut plugin, &update_with_a_heal_kit());
    on_tick(&mut world, &mut plugin, 0.1);

    assert!(tank(&plugin).has_heal_kit("Tested Healing Kit"));
    assert!(world.chat().contains(
        &"[MossTank] Game database updated to 2022-11-06 from openac-gamedata.".to_owned()
    ));
    assert!(tank(&plugin).game_info_update_running());
    assert_eq!(requests(&world), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.GameInfo.cs :: ALoginSkipsTheCheckWhileTheLastOneIsRecent
#[test]
fn a_login_skips_the_check_while_the_last_one_is_recent() {
    let mut world = online_world();
    world.profiles.insert(FILE_NAME, &excerpt());
    let an_hour_ago = world.fake.clock.utc_unix_ms / 1000 - 3600;
    world
        .plugin_data
        .insert(LAST_CHECK_KEY, &an_hour_ago.to_string());
    let mut plugin = world.enable();

    on_tick(&mut world, &mut plugin, 0.1);
    on_tick(&mut world, &mut plugin, 0.1);

    assert_eq!(requests(&world), 0);
    let skipped: Vec<String> = world
        .chat()
        .into_iter()
        .filter(|line| {
            line.starts_with("[MossTank] Game database checked recently; the next check is after ")
        })
        .collect();
    assert_eq!(skipped.len(), 1);

    world.vt(&mut plugin, "gamedb interval 0");
    assert!(world.chat().contains(
        &"[MossTank] Game database: checked at every login (/vt gamedb interval sets how often)."
            .to_owned()
    ));
    assert!(
        world
            .plugin_data
            .get("profiles/macro/preferences.json")
            .unwrap()
            .contains("\"GameDbCheckIntervalHours\": 0")
    );
    world.fake.character = None;
    on_tick(&mut world, &mut plugin, 0.1);
    world.fake.character = Some(character("Barris"));
    on_tick(&mut world, &mut plugin, 0.1);

    assert_eq!(requests(&world), 1);
}

// ---- MossTankPanelTests.Status -------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Status.cs :: TheMetaStateIsPublishedForOtherPlugins
#[test]
fn the_meta_state_is_published_for_other_plugins() {
    let mut world = World::with_character("Barris");
    world.set_reply::<calls::StatusPublish>(&true);
    world.profiles.insert(
        "mosstank/metas/Bella.met",
        &mosstank_formats::fixtures::read_vtank("met/bella.met"),
    );
    let mut plugin = world.enable();
    world.vt(&mut plugin, "meta load Bella");
    let published = |world: &World| {
        world
            .requests::<calls::StatusPublish>()
            .last()
            .cloned()
            .expect("a line published")
    };

    on_tick(&mut world, &mut plugin, 0.1);
    assert_eq!(
        published(&world),
        ("metaState".to_owned(), Some("Default".to_owned()))
    );

    world.vt(&mut plugin, "setmetastate mp_primary");
    on_tick(&mut world, &mut plugin, 0.1);
    assert_eq!(
        published(&world),
        ("metaState".to_owned(), Some("mp_primary".to_owned()))
    );
}

// ---- MossTankPanelTests.StorageLayout ------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.StorageLayout.cs :: AStartLaysOutEveryFixedFolderAndTheLoggedInCharactersOwn
#[test]
fn a_start_lays_out_every_fixed_folder_and_the_logged_in_characters_own() {
    let mut world = World::with_character("Barris");

    let _plugin = world.enable();

    for folder in layout::vtank_profile_folders() {
        assert!(world.profiles.directories.contains(&folder), "{folder}");
    }
    for folder in layout::PLUGIN_STORAGE_FOLDERS {
        assert!(
            world
                .plugin_data
                .directories
                .iter()
                .any(|made| made == folder),
            "{folder}"
        );
    }
    for folder in layout::character_folders(Some("Coldeve"), Some("Barris")) {
        assert!(world.profiles.directories.contains(&folder), "{folder}");
    }
}

// ---- the kill statistics in the plugin ----------------------------------

/// The plugin hands its recorder the client's chat and writes the segment
/// when it stops (the C# panel's `_killStatistics`).
#[test]
fn the_plugin_counts_kills_when_kill_statistics_are_on() {
    let mut world = World::with_character("Barris");
    world
        .plugin_data
        .insert(mosstank_session::kill_statistics::ENABLE_KEY, "true");
    let mut plugin = world.enable();
    let kill = ac_plugin_abi::types::ChatLine {
        sequence: 1,
        text: "You killed Drudge!".to_owned(),
        ..ac_plugin_abi::types::ChatLine::default()
    };
    world.send(&mut plugin, Event::ChatReceived(kill));

    world.disable(&mut plugin);

    let snapshots: Vec<String> = world
        .plugin_data
        .keys()
        .filter(|key| key.starts_with(mosstank_session::kill_statistics::SESSIONS_PREFIX))
        .map(|key| world.plugin_data.get(key).unwrap().to_owned())
        .collect();
    assert_eq!(snapshots.len(), 1);
    assert!(
        snapshots[0].contains("\"TotalKills\":1"),
        "{}",
        snapshots[0]
    );
    assert!(snapshots[0].contains("\"Closed\":true"));
}

// ---- Not a port of a C# test: a kept time past DateTimeOffset's range ------
//
// `DescribeNextGameInfoCheck` reads the kept time with
// `DateTimeOffset.FromUnixTimeSeconds` and adds the interval; past
// 9999-12-31 either throws, and the /vt command's catch says so (the
// message checked against .NET 10). Plain addition overflowed here instead.

#[test]
fn a_kept_time_past_the_last_date_fails_the_gamedb_command() {
    let mut world = World::with_character("Barris");
    world.plugin_data.insert(LAST_CHECK_KEY, "253402300799");
    let mut plugin = world.enable();
    world.clear_chat();

    world.vt(&mut plugin, "gamedb interval 6");

    assert_eq!(
        vec![
            "[MossTank] Command failed: The added or subtracted value results in an \
             un-representable DateTime. (Parameter 't')"
                .to_owned()
        ],
        world.chat()
    );
}

//! What the shell keeps of a session beside its rules (ported from the
//! C#'s `MossTankPanelTests.ActionHistory`, `.GameInfo`, `.Status` and
//! `.StorageLayout`): the action history and the debug log, the game
//! database's own check and its commands, the meta state other plugins
//! read, and the folders a start lays out.

mod common;

use std::rc::Rc;

use ac_plugin_abi::calls;
use ac_plugin_abi::net::{HttpReply, NetError, NetEvent};
use ac_plugin_abi::{Event, LogLevel};
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
// The debug log goes to the host's log at the debug level, not to a file in
// the plugin's storage (register META-7).
#[test]
fn debug_button_saves_history_and_stops_writing_when_disabled() {
    let mut world = World::with_character("Barris");
    let mut plugin = world.enable();
    assert_eq!(tank(&plugin).debug_log_button_text(), "Debug log: off");
    assert_eq!(
        tank(&plugin).debug_log_location(),
        "Save detailed macro activity in the session's plugin log."
    );
    world.clear_chat();
    world.with_tank(&mut plugin, |tank, client| tank.toggle_debug_log(client));

    // The header, then the history so far, in order.
    let history = tank(&plugin).action_history_lines();
    let started = world.logs(LogLevel::Debug);
    let character = character("Barris");
    let mut expected = vec![format!(
        "MossTank {}; character={}; world={}",
        mosstank::VERSION,
        character.name,
        character.world
    )];
    expected.extend(history.iter().cloned());
    assert_eq!(started, expected);
    assert!(started.iter().any(|line| line.contains("Combat off")));
    assert_eq!(tank(&plugin).debug_log_button_text(), "Debug log: ON");
    let location = "the session's plugin log \
         (logs\\<server>\\<character>\\<session>\\plugins\\acdream.mosstank.log)";
    assert_eq!(tank(&plugin).debug_log_location(), location);
    assert_eq!(
        world.chat(),
        vec![format!("[MossTank] MossTank debug log: {location}")]
    );

    // A change while it is on reaches the log in the same call.
    toggle_combat(&mut world, &mut plugin);
    let running = world.logs(LogLevel::Debug);
    assert_eq!(running[..started.len()], started[..]);
    assert!(
        running[started.len()..]
            .iter()
            .any(|line| line.starts_with("Combat: "))
    );

    world.with_tank(&mut plugin, |tank, client| tank.toggle_debug_log(client));
    let stopped = world.logs(LogLevel::Debug);
    assert_eq!(stopped.last().map(String::as_str), Some("Logging stopped."));
    toggle_combat(&mut world, &mut plugin);
    on_tick(&mut world, &mut plugin, 1.0);
    assert_eq!(world.logs(LogLevel::Debug), stopped);
    assert_eq!(tank(&plugin).debug_log_button_text(), "Debug log: off");

    // Debug lines only at the debug level, and nothing in the storage.
    for line in &stopped {
        assert!(!world.logs(LogLevel::Info).contains(line), "{line}");
    }
    assert!(
        world
            .plugin_data
            .keys()
            .all(|key| !key.starts_with("debug/")),
        "{:?}",
        world.plugin_data.keys().collect::<Vec<_>>()
    );
}

#[test]
fn the_debug_log_gets_every_new_history_line_in_order() {
    let mut world = World::with_character("Barris");
    let mut plugin = world.enable();
    world.with_tank(&mut plugin, |tank, client| tank.toggle_debug_log(client));
    let before = tank(&plugin).action_history_lines().len();
    let logged = world.logs(LogLevel::Debug).len();
    toggle_combat(&mut world, &mut plugin);
    on_tick(&mut world, &mut plugin, 0.5);
    // The history stamps its lines, and names the fight's without
    // "Combat: "; the host's log stamps the debug lines.
    let recorded: Vec<String> = tank(&plugin).action_history_lines()[before..]
        .iter()
        .map(|line| line.split_once("] ").expect("a stamped line").1.to_owned())
        .collect();
    let debug = world.logs(LogLevel::Debug)[logged..].to_vec();
    assert!(recorded.len() > 2, "{recorded:?}");
    assert_eq!(debug.len(), recorded.len(), "{debug:?} {recorded:?}");
    for (line, history) in debug.iter().zip(&recorded) {
        assert!(
            line == history || *line == format!("Combat: {history}"),
            "{line} / {history}"
        );
    }
}

// ---- /vt debuglog (MossTank's own; no C# source) ------------------------

const DEBUG_LOG_PLACE: &str = "the session's plugin log \
     (logs\\<server>\\<character>\\<session>\\plugins\\acdream.mosstank.log)";

/// The header and the history so far, as starting the debug log writes them.
fn debug_log_start_lines(plugin: &MossTank) -> Vec<String> {
    let character = character("Barris");
    let mut lines = vec![format!(
        "MossTank {}; character={}; world={}",
        mosstank::VERSION,
        character.name,
        character.world
    )];
    lines.extend(tank(plugin).action_history_lines().iter().cloned());
    lines
}

/// `/vt debuglog on` starts the log as the panel's button does: the header
/// and the history, then live lines, and the button shows it.
#[test]
fn debuglog_on_starts_the_log_as_the_button_does() {
    let mut world = World::with_character("Barris");
    let mut plugin = world.enable();
    world.clear_chat();
    world.vt(&mut plugin, "debuglog on");

    assert_eq!(world.logs(LogLevel::Debug), debug_log_start_lines(&plugin));
    assert_eq!(tank(&plugin).debug_log_button_text(), "Debug log: ON");
    assert_eq!(tank(&plugin).debug_log_location(), DEBUG_LOG_PLACE);
    assert_eq!(
        world.chat(),
        vec![format!("[MossTank] MossTank debug log: {DEBUG_LOG_PLACE}")]
    );
    let started = world.logs(LogLevel::Debug).len();
    toggle_combat(&mut world, &mut plugin);
    assert!(
        world.logs(LogLevel::Debug)[started..]
            .iter()
            .any(|line| line.starts_with("Combat: "))
    );
}

/// A second `on` (a login command run again, say) leaves the log as it is:
/// the history is not written twice.
#[test]
fn debuglog_on_again_changes_nothing_and_says_so() {
    let mut world = World::with_character("Barris");
    let mut plugin = world.enable();
    world.vt(&mut plugin, "debuglog on");
    let logged = world.logs(LogLevel::Debug);
    world.clear_chat();

    world.vt(&mut plugin, "debuglog ON");

    assert_eq!(world.logs(LogLevel::Debug), logged);
    assert_eq!(tank(&plugin).debug_log_button_text(), "Debug log: ON");
    assert_eq!(
        world.chat(),
        vec![format!(
            "[MossTank] MossTank debug log is already on: {DEBUG_LOG_PLACE}"
        )]
    );
}

/// `/vt debuglog off` stops the log as the button does, and the button
/// shows it; the button then starts it again.
#[test]
fn debuglog_off_stops_the_log_as_the_button_does() {
    let mut world = World::with_character("Barris");
    let mut plugin = world.enable();
    world.vt(&mut plugin, "debuglog on");
    world.clear_chat();

    world.vt(&mut plugin, "debuglog off");

    let stopped = world.logs(LogLevel::Debug);
    assert_eq!(stopped.last().map(String::as_str), Some("Logging stopped."));
    assert_eq!(tank(&plugin).debug_log_button_text(), "Debug log: off");
    assert_eq!(world.chat(), vec!["[MossTank] MossTank debug log stopped."]);
    toggle_combat(&mut world, &mut plugin);
    on_tick(&mut world, &mut plugin, 1.0);
    assert_eq!(world.logs(LogLevel::Debug), stopped);

    world.with_tank(&mut plugin, |tank, client| tank.toggle_debug_log(client));
    assert_eq!(tank(&plugin).debug_log_button_text(), "Debug log: ON");
    let restarted = world.logs(LogLevel::Debug)[stopped.len()..].to_vec();
    assert_eq!(restarted, debug_log_start_lines(&plugin));
}

/// `off` while it is off writes nothing to the log and says so.
#[test]
fn debuglog_off_again_changes_nothing_and_says_so() {
    let mut world = World::with_character("Barris");
    let mut plugin = world.enable();
    world.clear_chat();

    world.vt(&mut plugin, "debuglog off");

    assert!(world.logs(LogLevel::Debug).is_empty());
    assert_eq!(tank(&plugin).debug_log_button_text(), "Debug log: off");
    assert_eq!(
        world.chat(),
        vec!["[MossTank] MossTank debug log is already off."]
    );
}

/// `/vt debuglog` alone says whether the log is on and where it writes,
/// and changes nothing.
#[test]
fn debuglog_alone_reports_the_state_and_the_place() {
    let mut world = World::with_character("Barris");
    let mut plugin = world.enable();
    world.clear_chat();

    world.vt(&mut plugin, "debuglog");
    assert_eq!(
        world.chat(),
        vec![format!(
            "[MossTank] MossTank debug log: off; /vt debuglog on writes it to {DEBUG_LOG_PLACE}"
        )]
    );
    assert!(world.logs(LogLevel::Debug).is_empty());

    world.with_tank(&mut plugin, |tank, client| tank.toggle_debug_log(client));
    let logged = world.logs(LogLevel::Debug);
    world.clear_chat();
    world.vt(&mut plugin, "debuglog");
    assert_eq!(
        world.chat(),
        vec![format!(
            "[MossTank] MossTank debug log: ON, writing to {DEBUG_LOG_PLACE}"
        )]
    );
    assert_eq!(world.logs(LogLevel::Debug), logged);
}

/// Any other argument gets the syntax line and changes nothing.
#[test]
fn debuglog_with_a_bad_argument_prints_the_syntax() {
    let mut world = World::with_character("Barris");
    let mut plugin = world.enable();
    for line in ["debuglog maybe", "debuglog on now"] {
        world.clear_chat();
        world.vt(&mut plugin, line);
        assert_eq!(
            world.chat(),
            vec!["[MossTank] Syntax: /vt debuglog [on/off]"],
            "{line}"
        );
        assert!(world.logs(LogLevel::Debug).is_empty(), "{line}");
        assert_eq!(tank(&plugin).debug_log_button_text(), "Debug log: off");
    }
}

/// `/vt help debuglog` gives its usage, and `/vt help` lists it.
#[test]
fn debuglog_has_its_help() {
    let mut world = World::with_character("Barris");
    let mut plugin = world.enable();
    world.clear_chat();
    world.vt(&mut plugin, "help debuglog");
    let chat = world.chat();
    assert_eq!(
        chat[0], "[MossTank] Syntax: /vt debuglog [on/off]",
        "{chat:?}"
    );
    world.clear_chat();
    world.vt(&mut plugin, "help");
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("documented:") && line.contains("debuglog")),
        "{:?}",
        world.chat()
    );
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

fn evaluate(world: &mut World, plugin: &mut MossTank, source: &str) -> f64 {
    let value = world
        .with_tank(plugin, |tank, client| {
            tank.evaluate_expression(client, source)
        })
        .unwrap();
    match value {
        mosstank_expr::ExpressionValue::Number(number) => number,
        other => panic!("{source} is {other:?}"),
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: SameCharacterReconnectClearsOnlySessionStateAndCanRestartCleanly
/// Losing the session stops the macro and the meta and forgets the
/// session's variables and delayed runs; the persistent and global
/// variables stay. Back in the world, the macro starts cleanly.
#[test]
fn same_character_reconnect_clears_only_session_state_and_can_restart_cleanly() {
    let mut automation = common::automation::FakeAutomation::default();
    "Relogger".clone_into(&mut automation.name);
    let mut world = World::with_automation(automation);
    let mut plugin = world.enable();
    world.act(&mut plugin, "ToggleMeta");
    world.act(&mut plugin, "ToggleCombat");
    evaluate(
        &mut world,
        &mut plugin,
        "$session=11;@persistent=22;&global=33;delayexec[60000,`$late=1`]",
    );
    evaluate(&mut world, &mut plugin, "vtsetmetastate[`Hunt`]");

    world.automation().is_available = false;
    tick(&mut world, &mut plugin, 0.1);

    assert_eq!(world.ui_text(&mut plugin, "CombatButtonText"), "Run Macro");
    assert!(!world.ui_bool(&mut plugin, "MetaEnabled"));
    assert_eq!(
        world.ui_text(&mut plugin, "MetaState"),
        mosstank_meta::engine::DEFAULT_STATE_NAME
    );
    assert_eq!(evaluate(&mut world, &mut plugin, "$session"), 0.0);
    assert_eq!(
        world.ui_text(&mut plugin, "BuffStatus"),
        "Lost the session."
    );

    world.automation().is_available = true;
    tick(&mut world, &mut plugin, 0.1);
    // The C#'s fake keeps its character while the session is away and reads
    // these then; the client hands out no character outside the world
    // (TEST-2), so they are read back with the character.
    assert_eq!(evaluate(&mut world, &mut plugin, "@persistent"), 22.0);
    assert_eq!(evaluate(&mut world, &mut plugin, "&global"), 33.0);
    tick(&mut world, &mut plugin, 61.0);

    assert_eq!(evaluate(&mut world, &mut plugin, "$late"), 0.0);
    assert_eq!(world.ui_text(&mut plugin, "BuffStatus"), "Idle.");
    world.act(&mut plugin, "ToggleCombat");
    assert_eq!(world.ui_text(&mut plugin, "CombatButtonText"), "Stop Macro");
}

fn tick(world: &mut World, plugin: &mut MossTank, seconds: f64) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, seconds));
}

// Not applicable: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.GameInfo.cs :: WithoutASourceTheUpdateSaysItIsNotAvailable: the plugin always reaches the release host through the client, so there is no session without a source; a client that refuses the request ends the check with its reason (register GAMEDB-1)

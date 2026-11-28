//! The macro while the character can do nothing in the world (register
//! MACRO-1): from a death until the server has carried it out (the death
//! teleport over and the health back, whether or not the health showed
//! the death), and in portal space, no rule acts and the meta's chat lines
//! wait. Not a port: the C# and the reference run their rules through both.

mod common;

use ac_plugin_abi::combat::{CombatMode, CombatSnapshot};
use ac_plugin_abi::items::{EquipmentItem, EquipmentPlacement};
use ac_plugin_abi::magic::CastResult;
use ac_plugin_abi::navigation::NavigationSnapshot;
use ac_plugin_abi::world::{ObjectClass, PortalTransition, Position, WorldObject};
use ac_plugin_abi::{Event, LogLevel, calls};
use common::{World, character};
use mosstank::MossTank;
use mosstank_formats::route::{
    NavigationSettings, RouteMode, RouteRecallKind, RouteWaypoint, RouteWaypointType,
};

/// What a frame lasts in the C# tests.
const FRAME_SECONDS: f64 = 0.05;

/// Lifestone Recall.
const LIFESTONE_RECALL: u32 = 1635;

fn frames(world: &mut World, plugin: &mut MossTank, count: usize) {
    for _ in 0..count {
        world.with_tank(plugin, |tank, client| tank.on_tick(client, FRAME_SECONDS));
    }
}

fn here() -> Position {
    Position {
        cell_id: 0x0001_0001,
        is_outdoor: true,
        ..Position::default()
    }
}

/// In the world (or in portal space) at `here`.
fn stand(world: &mut World, portal_space: bool) {
    world.set_reply::<calls::NavigationSnapshot>(&NavigationSnapshot {
        is_available: true,
        local_object_id: 1,
        position: here(),
        is_portal_space: portal_space,
        ..NavigationSnapshot::default()
    });
}

fn health(world: &mut World, current: u32) {
    let mut info = character("Acdream");
    info.health = (current, 100, 100);
    world.fake.character = Some(info);
}

/// A wand in hand, the mode it keeps the character in being magic.
fn wield_wand(world: &mut World) {
    const WAND: u32 = 11;
    const WAND_SLOT: u32 = 0x0100_0000;
    world.set_reply::<calls::EquipmentOwned>(&vec![EquipmentItem {
        object_id: WAND,
        name: "War Wand".to_owned(),
        item_type: 0x8000,
        valid_locations: WAND_SLOT,
        equipped_location: WAND_SLOT,
        container_id: 0x5000_0001,
        wielder_id: 0x5000_0001,
        object_class: ObjectClass::WandStaffOrb,
        ..EquipmentItem::default()
    }]);
    world.set_reply::<calls::WorldObject>(&Some(WorldObject {
        object_id: WAND,
        name: "War Wand".to_owned(),
        object_class: ObjectClass::WandStaffOrb,
        is_owned: true,
        ..WorldObject::default()
    }));
    world.set_reply::<calls::EquipmentPlacements>(&vec![EquipmentPlacement {
        object_id: WAND,
        equipped_location: WAND_SLOT,
    }]);
    world.set_reply::<calls::CombatSnapshot>(&CombatSnapshot {
        mode: CombatMode::Magic,
        ..CombatSnapshot::default()
    });
}

/// A once-through route that begins with Lifestone Recall, as the vitae
/// run's route a meta loads on a death does.
fn recall_route() -> NavigationSettings {
    NavigationSettings {
        enabled: true,
        mode: RouteMode::Once,
        waypoints: vec![RouteWaypoint {
            kind: RouteWaypointType::Recall,
            position: here(),
            recall: RouteRecallKind::LifestoneRecall,
            recall_spell_id: LIFESTONE_RECALL,
            recall_spell_name: "Lifestone Recall".to_owned(),
            ..RouteWaypoint::default()
        }],
        ..NavigationSettings::default()
    }
}

fn recalls(world: &World) -> usize {
    world
        .requests::<calls::MagicCast>()
        .into_iter()
        .filter(|request| request.spell_id == LIFESTONE_RECALL)
        .count()
}

/// The character dies with the macro running, and the route that begins
/// with the recall is loaded and turned on while it lies dead (the meta's
/// death state does both at once).
fn dead_with_the_recall_route() -> (World, MossTank) {
    let mut world = World::with_character("Acdream");
    stand(&mut world, false);
    wield_wand(&mut world);
    world.set_reply::<calls::MagicCast>(&CastResult::Sent);
    let route = mosstank_formats::nav::save(&recall_route()).unwrap();
    world.profiles.insert("mosstank/navs/VitaeRun.nav", &route);
    let mut plugin = world.enable();
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    frames(&mut world, &mut plugin, 4);

    health(&mut world, 0);
    frames(&mut world, &mut plugin, 1);
    world.vt(&mut plugin, "nav load VitaeRun");
    world.vt(&mut plugin, "opt set EnableNav true");
    (world, plugin)
}

#[test]
fn a_dead_character_casts_no_recall_until_the_server_gives_its_health_back() {
    let (mut world, mut plugin) = dead_with_the_recall_route();

    // Lying dead, then carried to the lifestone in portal space: ten
    // seconds of each.
    frames(&mut world, &mut plugin, 200);
    assert_eq!(
        recalls(&world),
        0,
        "no recall while the character lies dead"
    );
    stand(&mut world, true);
    frames(&mut world, &mut plugin, 200);
    assert_eq!(recalls(&world), 0, "no recall in portal space");

    // Out of portal space, still dead until the server's revival.
    stand(&mut world, false);
    frames(&mut world, &mut plugin, 60);
    assert_eq!(recalls(&world), 0, "no recall before the health comes back");

    health(&mut world, 75);
    frames(&mut world, &mut plugin, 60);
    assert_eq!(
        recalls(&world),
        1,
        "the recall goes once the character can act"
    );
}

// The route already waits in portal space by itself (it did before the hold);
// this keeps that true when the health comes back first.
#[test]
fn health_back_in_portal_space_still_waits_for_the_world() {
    let (mut world, mut plugin) = dead_with_the_recall_route();
    stand(&mut world, true);
    health(&mut world, 75);

    frames(&mut world, &mut plugin, 200);
    assert_eq!(recalls(&world), 0, "no recall in portal space");

    stand(&mut world, false);
    frames(&mut world, &mut plugin, 60);
    assert_eq!(recalls(&world), 1);
}

#[test]
fn the_pause_names_why_the_character_waits() {
    let (mut world, mut plugin) = dead_with_the_recall_route();
    frames(&mut world, &mut plugin, 20);
    stand(&mut world, true);
    health(&mut world, 75);
    frames(&mut world, &mut plugin, 20);
    stand(&mut world, false);
    frames(&mut world, &mut plugin, 20);

    let lines = plugin.tank().expect("enabled").action_history_lines();
    let pauses: Vec<&String> = lines
        .iter()
        .filter(|line| line.contains("Pause: "))
        .collect();
    let position = |text: &str| pauses.iter().position(|line| line.ends_with(text));
    let dead = position("Pause: the character is dead").expect("dead named");
    let portal = position("Pause: the character is in portal space").expect("portal named");
    assert!(dead < portal, "{pauses:?}");
}

#[test]
fn nothing_at_all_is_cast_or_used_while_the_character_lies_dead() {
    let (mut world, mut plugin) = dead_with_the_recall_route();
    frames(&mut world, &mut plugin, 200);

    assert!(world.requests::<calls::MagicCast>().is_empty());
    assert!(world.requests::<calls::ItemsUse>().is_empty());
}

/// Where a teleport the client reports has got to.
#[derive(Clone, Copy, PartialEq)]
enum Trip {
    Entered,
    Completed,
    Cancelled,
}

/// A teleport of `generation` as the client reports it, portal space with it.
fn trip(world: &mut World, plugin: &mut MossTank, generation: i64, at: Trip) {
    stand(world, at == Trip::Entered);
    world.send(
        plugin,
        Event::PortalTransition(PortalTransition {
            generation,
            kind: 2,
            is_completed: at == Trip::Completed,
            is_cancelled: at == Trip::Cancelled,
            ..PortalTransition::default()
        }),
    );
}

/// The death teleport (the session's second teleport): portal space begins
/// (`entered`), or the character is out of it again.
fn death_teleport(world: &mut World, plugin: &mut MossTank, entered: bool) {
    let at = if entered {
        Trip::Entered
    } else {
        Trip::Completed
    };
    trip(world, plugin, 2, at);
}

/// The macro runs with the recall route on, and the client says the
/// character died; the health (still 100) never shows it.
fn dead_by_the_clients_word_alone() -> (World, MossTank) {
    let mut world = World::with_character("Acdream");
    stand(&mut world, false);
    wield_wand(&mut world);
    world.set_reply::<calls::MagicCast>(&CastResult::Sent);
    let route = mosstank_formats::nav::save(&recall_route()).unwrap();
    world.profiles.insert("mosstank/navs/VitaeRun.nav", &route);
    let mut plugin = world.enable();
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    frames(&mut world, &mut plugin, 4);

    world.send(&mut plugin, Event::LocalPlayerDied("You died!".to_owned()));
    world.vt(&mut plugin, "nav load VitaeRun");
    world.vt(&mut plugin, "opt set EnableNav true");
    (world, plugin)
}

#[test]
fn the_clients_word_of_a_death_holds_until_the_death_teleport_is_over() {
    let (mut world, mut plugin) = dead_by_the_clients_word_alone();

    // Past the old 5 s, and past 30 s: no teleport yet.
    frames(&mut world, &mut plugin, 120);
    assert_eq!(recalls(&world), 0, "held past 5 s");
    frames(&mut world, &mut plugin, 500);
    assert_eq!(recalls(&world), 0, "held past 30 s");

    // The death teleport: in portal space, then out of it.
    death_teleport(&mut world, &mut plugin, true);
    frames(&mut world, &mut plugin, 60);
    assert_eq!(recalls(&world), 0, "no recall in portal space");
    death_teleport(&mut world, &mut plugin, false);
    frames(&mut world, &mut plugin, 60);
    assert_eq!(recalls(&world), 1, "the recall goes after the teleport");
}

#[test]
fn a_death_teleport_alone_does_not_end_a_death_the_health_still_shows() {
    let (mut world, mut plugin) = dead_by_the_clients_word_alone();
    death_teleport(&mut world, &mut plugin, true);
    frames(&mut world, &mut plugin, 20);
    // Out of portal space, and the health has caught up with the death.
    health(&mut world, 0);
    death_teleport(&mut world, &mut plugin, false);
    frames(&mut world, &mut plugin, 1);
    // The health's death turned the route off; the meta turns it on again.
    world.vt(&mut plugin, "opt set EnableNav true");
    frames(&mut world, &mut plugin, 100);
    assert_eq!(recalls(&world), 0, "no recall while the health is 0");

    health(&mut world, 75);
    frames(&mut world, &mut plugin, 60);
    assert_eq!(recalls(&world), 1);
}

/// The log outs asked for.
fn logouts(world: &World) -> usize {
    world.requests::<calls::LoginLogout>().len()
}

/// The chat lines saying the death teleport never came.
fn said_lost(world: &World) -> usize {
    posted(world, "Death teleport never came")
}

#[test]
fn a_death_teleport_that_never_comes_is_said_once_and_still_holds() {
    // A window shows the session: not logged out by default.
    let mut world = World::with_character("Acdream");
    world.set_reply::<calls::HostWindowAttached>(&true);
    stand(&mut world, false);
    wield_wand(&mut world);
    world.set_reply::<calls::MagicCast>(&CastResult::Sent);
    let route = mosstank_formats::nav::save(&recall_route()).unwrap();
    world.profiles.insert("mosstank/navs/VitaeRun.nav", &route);
    let mut plugin = world.enable();
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    frames(&mut world, &mut plugin, 4);
    world.send(&mut plugin, Event::LocalPlayerDied("You died!".to_owned()));
    world.vt(&mut plugin, "nav load VitaeRun");
    world.vt(&mut plugin, "opt set EnableNav true");
    let said = |world: &World| {
        world
            .chat()
            .iter()
            .filter(|line| line.contains("Death teleport never came"))
            .count()
    };

    // 59 s: nothing said yet.
    frames(&mut world, &mut plugin, 1180);
    assert_eq!(said(&world), 0);
    // Past 60 s: said once, in the chat and the log.
    frames(&mut world, &mut plugin, 40);
    assert_eq!(said(&world), 1);
    assert!(
        world
            .logs(LogLevel::Warn)
            .iter()
            .any(|line| line.contains("the character is likely lost")),
        "{:?}",
        world.all_logs()
    );
    // Another minute: still held, said no more.
    frames(&mut world, &mut plugin, 1200);
    assert_eq!(said(&world), 1);
    assert_eq!(recalls(&world), 0, "still held");
    assert_eq!(logouts(&world), 0, "a windowed client is not logged out");
}

#[test]
fn portal_space_without_a_death_holds_only_while_it_lasts() {
    let mut world = World::with_character("Acdream");
    stand(&mut world, false);
    let mut plugin = world.enable();
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    frames(&mut world, &mut plugin, 4);

    death_teleport(&mut world, &mut plugin, true);
    frames(&mut world, &mut plugin, 20);
    death_teleport(&mut world, &mut plugin, false);
    frames(&mut world, &mut plugin, 1);

    let lines = plugin.tank().expect("enabled").action_history_lines();
    let pauses: Vec<&String> = lines
        .iter()
        .filter(|line| line.contains("Pause: "))
        .collect();
    assert_eq!(pauses.len(), 2, "{pauses:?}");
    assert!(
        pauses[0].ends_with("Pause: the character is in portal space"),
        "{pauses:?}"
    );
    assert!(pauses[1].ends_with("Pause: Resumed."), "{pauses:?}");
}

/// A meta whose death state says two lines and changes one option.
const DEATH_LINES_META: &str = "STATE: {Default} ~~ {\r\n\tIF:\tDeath\r\n\t\tDO:\tDoAll\r\n\t\t\t\tChat {/say first}\r\n\t\t\t\tChat {/vt opt set EnableLooting true}\r\n\t\t\t\tChat {/say second}\r\n\t\t\t\tSetState {Dead}\r\n~~ }\r\nSTATE: {Dead} ~~ {\r\n\tIF:\tNever\r\n\t\tDO:\tNone\r\n~~ }\r\n";

fn said_lines(world: &World) -> Vec<String> {
    world
        .requests::<calls::ChatSubmit>()
        .into_iter()
        .filter(|line| line.starts_with("/say "))
        .collect()
}

/// The death meta loaded and on, the macro running, and the client's word
/// of a death; the health (still 100) never shows it.
fn dead_with_the_death_lines_meta() -> (World, MossTank) {
    dead_with_meta(DEATH_LINES_META)
}

/// A meta of one state's rules: on a death, `death_actions` (a DoAll's
/// lines), then the state Dead, which holds `dead_rules`.
fn death_meta(death_actions: &str, dead_rules: &str) -> String {
    format!(
        "STATE: {{Default}} ~~ {{\r\n\tIF:\tDeath\r\n\t\tDO:\tDoAll\r\n{death_actions}\t\t\t\tSetState {{Dead}}\r\n~~ }}\r\nSTATE: {{Dead}} ~~ {{\r\n{dead_rules}\tIF:\tNever\r\n\t\tDO:\tNone\r\n~~ }}\r\nSTATE: {{Other}} ~~ {{\r\n\tIF:\tNever\r\n\t\tDO:\tNone\r\n~~ }}\r\n"
    )
}

/// One action line of a DoAll.
fn act(action: &str) -> String {
    format!("\t\t\t\t{action}\r\n")
}

/// The meta `text` loaded and on, the macro running, and the client's word
/// of a death.
fn dead_with_meta(text: &str) -> (World, MossTank) {
    let mut world = World::with_character("Acdream");
    stand(&mut world, false);
    world.profiles.insert("mosstank/metas/Deaths.af", text);
    world.profiles.insert(
        "mosstank/metas/Second.af",
        &death_meta(&act("Chat {/say second meta}"), ""),
    );
    let mut plugin = world.enable();
    world.vt(&mut plugin, "meta load Deaths");
    world.vt(&mut plugin, "opt set EnableMeta true");
    world.vt(&mut plugin, "opt set EnableLooting false");
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    frames(&mut world, &mut plugin, 4);
    world.send(&mut plugin, Event::LocalPlayerDied("You died!".to_owned()));
    (world, plugin)
}

#[test]
fn a_meta_chat_line_waits_for_the_hold_to_end_and_goes_in_order() {
    let (mut world, mut plugin) = dead_with_the_death_lines_meta();
    frames(&mut world, &mut plugin, 200);
    // The meta ran: its state changed and its /vt line took, on time.
    assert_eq!(plugin.tank().unwrap().meta_state(), "Dead");
    assert_eq!(world.option_text(&mut plugin, "EnableLooting"), "True");
    assert!(said_lines(&world).is_empty(), "{:?}", said_lines(&world));

    death_teleport(&mut world, &mut plugin, true);
    frames(&mut world, &mut plugin, 60);
    assert!(said_lines(&world).is_empty(), "held in portal space");

    death_teleport(&mut world, &mut plugin, false);
    frames(&mut world, &mut plugin, 20);
    assert_eq!(said_lines(&world), vec!["/say first", "/say second"]);
}

#[test]
fn held_meta_lines_are_dropped_when_the_macro_stops() {
    let (mut world, mut plugin) = dead_with_the_death_lines_meta();
    frames(&mut world, &mut plugin, 40);
    assert_eq!(plugin.tank().unwrap().meta_state(), "Dead");
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, false)
    });
    death_teleport(&mut world, &mut plugin, true);
    frames(&mut world, &mut plugin, 20);
    death_teleport(&mut world, &mut plugin, false);
    frames(&mut world, &mut plugin, 4);
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    frames(&mut world, &mut plugin, 20);
    assert!(said_lines(&world).is_empty(), "{:?}", said_lines(&world));
}

/// The chat lines posted that contain `text`.
fn posted(world: &World, text: &str) -> usize {
    world
        .chat()
        .iter()
        .filter(|line| line.contains(text))
        .count()
}

/// Leaves the death behind: the death teleport, then a second out of it.
fn after_the_death_teleport(world: &mut World, plugin: &mut MossTank) {
    death_teleport(world, plugin, true);
    frames(world, plugin, 20);
    death_teleport(world, plugin, false);
    frames(world, plugin, 20);
}

#[test]
fn a_death_as_a_portal_trip_begins_holds_past_that_trips_arrival() {
    let mut world = World::with_character("Acdream");
    stand(&mut world, false);
    wield_wand(&mut world);
    world.set_reply::<calls::MagicCast>(&CastResult::Sent);
    let route = mosstank_formats::nav::save(&recall_route()).unwrap();
    world.profiles.insert("mosstank/navs/VitaeRun.nav", &route);
    let mut plugin = world.enable();
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    frames(&mut world, &mut plugin, 4);

    // A portal trip begins, and the character dies as it does.
    trip(&mut world, &mut plugin, 1, Trip::Entered);
    frames(&mut world, &mut plugin, 2);
    world.send(&mut plugin, Event::LocalPlayerDied("You died!".to_owned()));
    world.vt(&mut plugin, "nav load VitaeRun");
    world.vt(&mut plugin, "opt set EnableNav true");
    frames(&mut world, &mut plugin, 20);
    // That trip arrives: it is not the death teleport.
    trip(&mut world, &mut plugin, 1, Trip::Completed);
    frames(&mut world, &mut plugin, 100);
    assert_eq!(recalls(&world), 0, "held past the trip's arrival");

    after_the_death_teleport(&mut world, &mut plugin);
    frames(&mut world, &mut plugin, 40);
    assert_eq!(
        recalls(&world),
        1,
        "the recall goes after the death teleport"
    );
}

#[test]
fn a_cancelled_teleport_is_not_the_death_teleport() {
    let (mut world, mut plugin) = dead_by_the_clients_word_alone();
    trip(&mut world, &mut plugin, 1, Trip::Entered);
    frames(&mut world, &mut plugin, 20);
    trip(&mut world, &mut plugin, 1, Trip::Cancelled);
    frames(&mut world, &mut plugin, 100);
    assert_eq!(recalls(&world), 0, "held past the cancelled teleport");

    after_the_death_teleport(&mut world, &mut plugin);
    frames(&mut world, &mut plugin, 40);
    assert_eq!(recalls(&world), 1);
}

#[test]
fn the_clients_word_of_a_death_is_handled_once_as_the_health_would_be() {
    let (mut world, mut plugin) = dead_by_the_clients_word_alone();
    frames(&mut world, &mut plugin, 4);
    // The macro's own death handling ran on the word: its notice, the
    // switches it turns off.
    assert_eq!(posted(&world, "You died!"), 1, "{:#?}", world.chat());
    assert_eq!(world.option_text(&mut plugin, "EnableCombat"), "False");

    // The health that shows the death later is the same death.
    health(&mut world, 0);
    frames(&mut world, &mut plugin, 20);
    assert_eq!(posted(&world, "You died!"), 1, "{:#?}", world.chat());
}

#[test]
fn meta_chat_lines_wait_in_portal_space_without_a_death() {
    let mut world = World::with_character("Acdream");
    stand(&mut world, false);
    world.profiles.insert(
        "mosstank/metas/Trip.af",
        "STATE: {Default} ~~ {\r\n\tIF:\tAlways\r\n\t\tDO:\tChat {/say in portal space}\r\n~~ }\r\n",
    );
    let mut plugin = world.enable();
    world.vt(&mut plugin, "meta load Trip");
    world.vt(&mut plugin, "opt set EnableMeta true");
    trip(&mut world, &mut plugin, 1, Trip::Entered);
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    frames(&mut world, &mut plugin, 40);
    assert!(said_lines(&world).is_empty(), "{:?}", said_lines(&world));

    trip(&mut world, &mut plugin, 1, Trip::Completed);
    frames(&mut world, &mut plugin, 4);
    assert_eq!(said_lines(&world), vec!["/say in portal space"]);
}

#[test]
fn held_meta_lines_are_dropped_when_the_session_ends() {
    let (mut world, mut plugin) = dead_with_the_death_lines_meta();
    frames(&mut world, &mut plugin, 40);
    assert_eq!(plugin.tank().unwrap().meta_state(), "Dead");
    let gone = world.fake.character.take();
    frames(&mut world, &mut plugin, 4);
    world.fake.character = gone;
    stand(&mut world, false);
    world.vt(&mut plugin, "opt set EnableMeta true");
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    frames(&mut world, &mut plugin, 40);
    assert!(said_lines(&world).is_empty(), "{:?}", said_lines(&world));
}

#[test]
fn held_meta_lines_are_dropped_when_another_meta_loads() {
    let (mut world, mut plugin) = dead_with_the_death_lines_meta();
    frames(&mut world, &mut plugin, 40);
    assert_eq!(plugin.tank().unwrap().meta_state(), "Dead");
    world.vt(&mut plugin, "meta load Second");
    after_the_death_teleport(&mut world, &mut plugin);
    assert!(said_lines(&world).is_empty(), "{:?}", said_lines(&world));
}

#[test]
fn a_meta_that_loads_another_in_its_pass_drops_what_it_held() {
    let meta = death_meta(
        &[
            act("Chat {/say before the load}"),
            act("Chat {/vt meta load Second}"),
            act("Chat {/say after the load}"),
        ]
        .concat(),
        "",
    );
    let (mut world, mut plugin) = dead_with_meta(&meta);
    frames(&mut world, &mut plugin, 40);
    after_the_death_teleport(&mut world, &mut plugin);
    assert!(said_lines(&world).is_empty(), "{:?}", said_lines(&world));
}

#[test]
fn a_held_ub_line_runs_once_the_hold_ends() {
    let meta = death_meta(
        &[
            act("Chat {/ub printcolors}"),
            act("Chat {/ub mexec echo[`local line ran`,1]}"),
        ]
        .concat(),
        "",
    );
    let (mut world, mut plugin) = dead_with_meta(&meta);
    frames(&mut world, &mut plugin, 40);
    assert_eq!(posted(&world, "[PrintColors]"), 0, "held");
    // A /ub line that only runs an expression goes at once.
    assert!(
        world.chat().iter().any(|line| line == "local line ran"),
        "{:#?}",
        world.chat()
    );

    after_the_death_teleport(&mut world, &mut plugin);
    assert!(posted(&world, "[PrintColors]") > 0, "{:#?}", world.chat());
}

#[test]
fn a_chatbox_line_waits_for_the_hold_to_end() {
    let meta = death_meta(&act("DoExpr {chatbox[`/say boxed`]}"), "");
    let (mut world, mut plugin) = dead_with_meta(&meta);
    frames(&mut world, &mut plugin, 40);
    assert!(said_lines(&world).is_empty(), "{:?}", said_lines(&world));

    after_the_death_teleport(&mut world, &mut plugin);
    assert_eq!(said_lines(&world), vec!["/say boxed"]);
}

#[test]
fn a_ub_delay_line_that_comes_due_in_the_hold_waits_for_its_end() {
    let mut world = World::with_character("Acdream");
    stand(&mut world, false);
    let mut plugin = world.enable();
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    frames(&mut world, &mut plugin, 4);
    // Said before the death, due during the hold.
    world.command(&mut plugin, "ub", "delay 1000 /say delayed");
    world.send(&mut plugin, Event::LocalPlayerDied("You died!".to_owned()));
    frames(&mut world, &mut plugin, 100);
    assert!(said_lines(&world).is_empty(), "{:?}", said_lines(&world));

    after_the_death_teleport(&mut world, &mut plugin);
    assert_eq!(said_lines(&world), vec!["/say delayed"]);
}

#[test]
fn a_rule_that_fires_again_and_again_holds_one_copy_of_its_line() {
    // Dead and Again send each other back and forth every pass.
    let meta = "STATE: {Default} ~~ {\r\n\tIF:\tDeath\r\n\t\tDO:\tSetState {Dead}\r\n~~ }\r\nSTATE: {Dead} ~~ {\r\n\tIF:\tAlways\r\n\t\tDO:\tDoAll\r\n\t\t\t\tChat {/say again}\r\n\t\t\t\tChat {/say and again}\r\n\t\t\t\tSetState {Again}\r\n~~ }\r\nSTATE: {Again} ~~ {\r\n\tIF:\tAlways\r\n\t\tDO:\tDoAll\r\n\t\t\t\tChat {/say again}\r\n\t\t\t\tSetState {Dead}\r\n~~ }\r\n";
    let (mut world, mut plugin) = dead_with_meta(meta);
    frames(&mut world, &mut plugin, 200);
    // Stopped going round before the hold ends.
    world.vt(&mut plugin, "setmetastate Other");
    after_the_death_teleport(&mut world, &mut plugin);
    assert_eq!(said_lines(&world), vec!["/say again", "/say and again"]);
}

#[test]
fn no_more_than_sixteen_lines_are_held_and_the_rest_are_said_dropped_once() {
    let actions: String = (1..=20)
        .map(|line| act(&format!("Chat {{/say line {line}}}")))
        .collect();
    let (mut world, mut plugin) = dead_with_meta(&death_meta(&actions, ""));
    world.vt(&mut plugin, "log RuleInfo on");
    frames(&mut world, &mut plugin, 40);
    after_the_death_teleport(&mut world, &mut plugin);
    let expected: Vec<String> = (1..=16).map(|line| format!("/say line {line}")).collect();
    assert_eq!(said_lines(&world), expected);
    let dropped = world
        .all_logs()
        .iter()
        .filter(|line| line.contains("the rest are dropped"))
        .count();
    assert_eq!(dropped, 1, "{:#?}", world.all_logs());
}

#[test]
fn a_meta_function_that_acts_answers_refused_while_the_world_is_held() {
    let meta = death_meta(
        &[
            act("DoExpr {setvar[`moved`,setmotion[`Forward`,1]]}"),
            act("DoExpr {setvar[`cast`,actiontrycastbyid[1635]]}"),
        ]
        .concat(),
        "",
    );
    let (mut world, mut plugin) = dead_with_meta(&meta);
    world.set_reply::<calls::NavigationSetIntent>(
        &ac_plugin_abi::navigation::NavigationStatus::Accepted,
    );
    let intents_before = world.requests::<calls::NavigationSetIntent>().len();
    frames(&mut world, &mut plugin, 40);
    assert_eq!(plugin.tank().unwrap().meta_state(), "Dead");
    assert_eq!(
        world.requests::<calls::NavigationSetIntent>().len(),
        intents_before,
        "no motion while held"
    );
    world.clear_chat();
    world.vt(&mut plugin, "mexec getvar[`moved`]");
    world.vt(&mut plugin, "mexec getvar[`cast`]");
    // Both answered as refused: false, and "not attempted".
    let answers: Vec<String> = world
        .chat()
        .into_iter()
        .filter(|line| line.contains("Result:"))
        .collect();
    assert_eq!(
        answers,
        ["[MossTank] Result: 0", "[MossTank] Result: 0"],
        "{answers:#?}"
    );
}

// The server gives the health back 3 s after the death teleport begins,
// while the character is still in portal space (ACE
// `WorldObjects/Player_Death.cs`): the health back is no sign the teleport
// is over.
#[test]
fn health_back_mid_teleport_still_holds_until_the_teleport_is_over() {
    let (mut world, mut plugin) = dead_with_the_recall_route();
    frames(&mut world, &mut plugin, 20);
    // The client reports the death teleport under way; this frame's look
    // at the world does not show portal space yet.
    world.send(
        &mut plugin,
        Event::PortalTransition(PortalTransition {
            generation: 1,
            kind: 2,
            ..PortalTransition::default()
        }),
    );
    health(&mut world, 75);
    frames(&mut world, &mut plugin, 100);
    assert_eq!(recalls(&world), 0, "held while the teleport is under way");

    trip(&mut world, &mut plugin, 1, Trip::Completed);
    frames(&mut world, &mut plugin, 60);
    assert_eq!(recalls(&world), 1, "the recall goes after the teleport");
}

// As a bot's log showed it: dead, then the health back, and no portal space
// seen in between.
#[test]
fn health_back_before_the_death_teleport_still_holds() {
    let (mut world, mut plugin) = dead_with_the_recall_route();
    frames(&mut world, &mut plugin, 100);
    health(&mut world, 75);
    frames(&mut world, &mut plugin, 200);
    assert_eq!(recalls(&world), 0, "held until the death teleport");

    after_the_death_teleport(&mut world, &mut plugin);
    frames(&mut world, &mut plugin, 40);
    assert_eq!(recalls(&world), 1);
}

#[test]
fn a_death_teleport_that_never_comes_logs_a_bot_out_to_enter_it_again() {
    // No window shows the session: a bot.
    let (mut world, mut plugin) = dead_by_the_clients_word_alone();
    world.set_reply::<calls::LoginLogout>(&true);
    world.set_reply::<calls::LoginSetNext>(&true);
    frames(&mut world, &mut plugin, 1180);
    assert_eq!(logouts(&world), 0, "not before 60 s");

    frames(&mut world, &mut plugin, 40);
    assert_eq!(logouts(&world), 1);
    assert_eq!(
        world.requests::<calls::LoginSetNext>(),
        vec![0x5000_0001],
        "the same character is entered again"
    );
    assert_eq!(said_lost(&world), 1);
    assert!(
        world
            .logs(LogLevel::Warn)
            .iter()
            .any(|line| line.contains("Logging out to enter it again")),
        "{:?}",
        world.all_logs()
    );
    // Asked once; still held until the session ends.
    frames(&mut world, &mut plugin, 1200);
    assert_eq!(logouts(&world), 1);
    assert_eq!(recalls(&world), 0, "still held");
}

#[test]
fn a_bot_set_not_to_relog_holds_without_logging_out() {
    let (mut world, mut plugin) = dead_by_the_clients_word_alone();
    world.set_reply::<calls::LoginLogout>(&true);
    world.vt(&mut plugin, "opt set RelogWhenDeathTeleportLost false");
    frames(&mut world, &mut plugin, 1300);
    assert_eq!(logouts(&world), 0);
    assert_eq!(said_lost(&world), 1, "{:#?}", world.chat());
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("Holding until a relog")),
        "{:#?}",
        world.chat()
    );
    assert_eq!(recalls(&world), 0, "still held");
}

#[test]
fn a_windowed_client_set_to_relog_logs_out() {
    let (mut world, mut plugin) = dead_by_the_clients_word_alone();
    world.set_reply::<calls::HostWindowAttached>(&true);
    world.set_reply::<calls::LoginLogout>(&true);
    world.vt(&mut plugin, "opt set RelogWhenDeathTeleportLost true");
    frames(&mut world, &mut plugin, 1300);
    assert_eq!(logouts(&world), 1);
}

#[test]
fn the_relog_option_follows_the_window_until_set() {
    let mut world = World::with_character("Acdream");
    let mut plugin = world.enable();
    let value = |world: &mut World, plugin: &mut MossTank| {
        world.clear_chat();
        world.vt(plugin, "opt get RelogWhenDeathTeleportLost");
        world
            .chat()
            .into_iter()
            .find(|line| line.contains("RelogWhenDeathTeleportLost"))
            .unwrap_or_default()
    };
    assert!(
        value(&mut world, &mut plugin).ends_with("= True"),
        "no window"
    );
    world.send(&mut plugin, Event::WindowAttached(true));
    assert!(
        value(&mut world, &mut plugin).ends_with("= False"),
        "a window"
    );
    world.vt(&mut plugin, "opt set RelogWhenDeathTeleportLost true");
    assert!(value(&mut world, &mut plugin).ends_with("= True"), "set");
}

/// The macro running with the meta `text` on, and a death the health
/// shows (at 0).
fn dead_at_0_with_meta(text: &str) -> (World, MossTank) {
    let mut world = World::with_character("Acdream");
    stand(&mut world, false);
    world.profiles.insert("mosstank/metas/Deaths.af", text);
    let mut plugin = world.enable();
    world.vt(&mut plugin, "meta load Deaths");
    world.vt(&mut plugin, "opt set EnableMeta true");
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    frames(&mut world, &mut plugin, 4);
    health(&mut world, 0);
    world.send(&mut plugin, Event::LocalPlayerDied("You died!".to_owned()));
    frames(&mut world, &mut plugin, 1);
    (world, plugin)
}

/// One rule of a state: `condition`, then a DoAll of `actions`.
fn rule(condition: &str, actions: &str) -> String {
    format!("\tIF:\t{condition}\r\n\t\tDO:\tDoAll\r\n{actions}")
}

#[test]
fn an_exit_portal_rule_still_fires_in_the_hold_and_its_line_waits() {
    let meta = death_meta(
        "",
        &rule(
            "ExitPortal",
            &[act("Chat {/say arrived}"), act("SetState {Other}")].concat(),
        ),
    );
    let (mut world, mut plugin) = dead_at_0_with_meta(&meta);
    frames(&mut world, &mut plugin, 20);
    assert_eq!(plugin.tank().unwrap().meta_state(), "Dead");
    // The teleport ends while the health still shows the death.
    death_teleport(&mut world, &mut plugin, true);
    frames(&mut world, &mut plugin, 20);
    death_teleport(&mut world, &mut plugin, false);
    frames(&mut world, &mut plugin, 20);
    assert_eq!(
        plugin.tank().unwrap().meta_state(),
        "Other",
        "the rule fired"
    );
    assert!(said_lines(&world).is_empty(), "{:?}", said_lines(&world));

    health(&mut world, 75);
    frames(&mut world, &mut plugin, 4);
    assert_eq!(said_lines(&world), vec!["/say arrived"]);
}

#[test]
fn a_ub_line_said_in_the_hold_goes_only_after_the_teleport() {
    let meta = death_meta(&act("Chat {/ub printcolors}"), "");
    let (mut world, mut plugin) = dead_at_0_with_meta(&meta);
    frames(&mut world, &mut plugin, 20);
    death_teleport(&mut world, &mut plugin, true);
    frames(&mut world, &mut plugin, 20);
    // The health back mid-teleport, as the server sends it.
    health(&mut world, 75);
    frames(&mut world, &mut plugin, 60);
    assert_eq!(posted(&world, "[PrintColors]"), 0, "held mid-teleport");

    death_teleport(&mut world, &mut plugin, false);
    frames(&mut world, &mut plugin, 4);
    assert!(posted(&world, "[PrintColors]") > 0, "{:#?}", world.chat());
}

#[test]
fn a_vt_jump_said_in_the_hold_waits_and_runs_after_the_teleport() {
    let meta = death_meta(
        &[
            act("Chat {/vt jump 0 false 100}"),
            act("Chat {/vt opt set EnableLooting true}"),
        ]
        .concat(),
        "",
    );
    let (mut world, mut plugin) = dead_with_meta(&meta);
    let accepted = ac_plugin_abi::navigation::NavigationStatus::Accepted;
    world.set_reply::<calls::NavigationJump>(&accepted);
    world.set_reply::<calls::NavigationMove>(&accepted);
    world.set_reply::<calls::NavigationSetIntent>(&accepted);
    world.set_reply::<calls::NavigationClearIntent>(&accepted);
    world.set_reply::<calls::NavigationFace>(&accepted);
    frames(&mut world, &mut plugin, 40);
    assert_eq!(plugin.tank().unwrap().meta_state(), "Dead");
    // The option line ran at once; the jump waits.
    assert_eq!(world.option_text(&mut plugin, "EnableLooting"), "True");
    assert!(world.requests::<calls::NavigationJump>().is_empty());

    death_teleport(&mut world, &mut plugin, true);
    frames(&mut world, &mut plugin, 20);
    assert!(
        world.requests::<calls::NavigationJump>().is_empty(),
        "no jump in portal space"
    );
    death_teleport(&mut world, &mut plugin, false);
    frames(&mut world, &mut plugin, 20);
    assert_eq!(world.requests::<calls::NavigationJump>().len(), 1);
}

/// A meta that on a death says `death_actions` and goes to Dead, holding
/// `dead_rules`; Other holds `other_rules`.
fn death_meta_with_other(death_actions: &str, dead_rules: &str, other_rules: &str) -> String {
    format!(
        "STATE: {{Default}} ~~ {{\r\n\tIF:\tDeath\r\n\t\tDO:\tDoAll\r\n{death_actions}\t\t\t\tSetState {{Dead}}\r\n~~ }}\r\nSTATE: {{Dead}} ~~ {{\r\n{dead_rules}\tIF:\tNever\r\n\t\tDO:\tNone\r\n~~ }}\r\nSTATE: {{Other}} ~~ {{\r\n{other_rules}\tIF:\tNever\r\n\t\tDO:\tNone\r\n~~ }}\r\n"
    )
}

/// The RuleInfo log has a line containing `text`.
fn logged(world: &World, text: &str) -> bool {
    world.all_logs().iter().any(|line| line.contains(text))
}

#[test]
fn a_line_said_at_the_death_goes_after_a_45_second_death_and_arrival() {
    let meta = death_meta(&act("Chat {/say recall}"), "");
    let (mut world, mut plugin) = dead_with_meta(&meta);
    // 40 s until the death teleport begins, 5 s in portal space.
    frames(&mut world, &mut plugin, 800);
    death_teleport(&mut world, &mut plugin, true);
    frames(&mut world, &mut plugin, 100);
    death_teleport(&mut world, &mut plugin, false);
    frames(&mut world, &mut plugin, 4);
    assert_eq!(said_lines(&world), vec!["/say recall"]);
}

#[test]
fn a_line_said_in_a_45_second_portal_trip_goes_after_it() {
    let mut world = World::with_character("Acdream");
    stand(&mut world, false);
    world.profiles.insert(
        "mosstank/metas/Trip.af",
        "STATE: {Default} ~~ {\r\n\tIF:\tIntoPortal\r\n\t\tDO:\tChat {/say in portal space}\r\n~~ }\r\n",
    );
    let mut plugin = world.enable();
    world.vt(&mut plugin, "meta load Trip");
    world.vt(&mut plugin, "opt set EnableMeta true");
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    frames(&mut world, &mut plugin, 4);
    trip(&mut world, &mut plugin, 1, Trip::Entered);
    frames(&mut world, &mut plugin, 900);
    trip(&mut world, &mut plugin, 1, Trip::Completed);
    frames(&mut world, &mut plugin, 4);
    assert_eq!(said_lines(&world), vec!["/say in portal space"]);
}

/// The death teleport over while the health still shows the death, which
/// lasts `seconds` more: the hold could have ended, and still holds.
fn arrive_still_dead_for(world: &mut World, plugin: &mut MossTank, seconds: usize) {
    death_teleport(world, plugin, true);
    frames(world, plugin, 20);
    death_teleport(world, plugin, false);
    frames(world, plugin, seconds * 20);
}

#[test]
fn a_line_said_again_and_again_is_never_stale() {
    // Dead and Other send each other back and forth every pass.
    let meta = death_meta_with_other(
        "",
        &rule(
            "Always",
            &[act("Chat {/say again}"), act("SetState {Other}")].concat(),
        ),
        &rule("Always", &act("SetState {Dead}")),
    );
    let (mut world, mut plugin) = dead_at_0_with_meta(&meta);
    world.vt(&mut plugin, "log RuleInfo on");
    frames(&mut world, &mut plugin, 20);
    arrive_still_dead_for(&mut world, &mut plugin, 35);
    health(&mut world, 75);
    frames(&mut world, &mut plugin, 1);
    assert_eq!(
        said_lines(&world).first().map(String::as_str),
        Some("/say again"),
        "{:#?}",
        world.all_logs()
    );
    assert!(
        logged(&world, "1 held chat lines sent"),
        "{:#?}",
        world.all_logs()
    );
    assert!(!logged(&world, "held over 30 s"), "{:#?}", world.all_logs());
}

#[test]
fn stale_lines_make_room_under_the_limit_and_are_dropped() {
    let actions: String = (1..=16)
        .map(|line| act(&format!("Chat {{/say line {line}}}")))
        .collect();
    let meta = death_meta_with_other(&actions, "", &rule("Always", &act("Chat {/say fresh}")));
    let (mut world, mut plugin) = dead_at_0_with_meta(&meta);
    world.vt(&mut plugin, "log RuleInfo on");
    frames(&mut world, &mut plugin, 20);
    // The sixteen go stale after the arrival; then a fresh line is said.
    arrive_still_dead_for(&mut world, &mut plugin, 35);
    world.vt(&mut plugin, "setmetastate Other");
    frames(&mut world, &mut plugin, 20);
    health(&mut world, 75);
    frames(&mut world, &mut plugin, 1);
    assert_eq!(
        said_lines(&world).first().map(String::as_str),
        Some("/say fresh")
    );
    assert!(
        !said_lines(&world)
            .iter()
            .any(|line| line.starts_with("/say line"))
    );
    assert!(
        logged(
            &world,
            "16 held chat lines dropped: held over 30 s (first: /say line 1)"
        ),
        "{:#?}",
        world.all_logs()
    );
    assert!(
        !logged(&world, "the rest are dropped"),
        "{:#?}",
        world.all_logs()
    );
}

#[test]
fn the_held_lines_past_four_are_said_once_to_be_counted() {
    let actions: String = (1..=6)
        .map(|line| act(&format!("Chat {{/say line {line}}}")))
        .collect();
    let (mut world, mut plugin) = dead_with_meta(&death_meta(&actions, ""));
    world.vt(&mut plugin, "log RuleInfo on");
    frames(&mut world, &mut plugin, 20);
    let count = |world: &World, text: &str| {
        world
            .all_logs()
            .iter()
            .filter(|line| line.contains(text))
            .count()
    };
    assert_eq!(count(&world, "(meta) chat line held while"), 4);
    assert_eq!(count(&world, "more chat lines held while"), 1);
}

/// The death of a bot whose death teleport never comes, with the client
/// answering `logout` to the log out.
fn lost_bot(logout: bool) -> (World, MossTank) {
    let (mut world, plugin) = dead_by_the_clients_word_alone();
    world.set_reply::<calls::LoginLogout>(&logout);
    world.set_reply::<calls::LoginSetNext>(&true);
    world.set_reply::<calls::LoginClearNext>(&true);
    (world, plugin)
}

fn next_login_clears(world: &World) -> usize {
    world.requests::<calls::LoginClearNext>().len()
}

#[test]
fn a_refused_logout_clears_the_next_login_it_set() {
    let (mut world, mut plugin) = lost_bot(false);
    frames(&mut world, &mut plugin, 1220);
    assert_eq!(world.requests::<calls::LoginSetNext>(), vec![0x5000_0001]);
    assert_eq!(next_login_clears(&world), 1);
    assert!(
        world.logs(LogLevel::Warn).iter().any(|line| line
            .contains("The client did not log the character out")
            && line.contains("The next login set for it was cleared")),
        "{:?}",
        world.all_logs()
    );
}

#[test]
fn a_next_login_already_set_is_kept() {
    let (mut world, mut plugin) = lost_bot(false);
    world.set_reply::<calls::LoginState>(&ac_plugin_abi::social::LoginState {
        is_available: true,
        next_login_id: 0x5000_0002,
        ..ac_plugin_abi::social::LoginState::default()
    });
    frames(&mut world, &mut plugin, 1220);
    assert_eq!(logouts(&world), 1);
    assert!(world.requests::<calls::LoginSetNext>().is_empty());
    assert_eq!(next_login_clears(&world), 0);
}

#[test]
fn a_logout_that_never_happens_clears_the_next_login_after_60_seconds() {
    let (mut world, mut plugin) = lost_bot(true);
    frames(&mut world, &mut plugin, 1220);
    assert_eq!(logouts(&world), 1);
    frames(&mut world, &mut plugin, 1180);
    assert_eq!(next_login_clears(&world), 0, "not before 60 s");
    frames(&mut world, &mut plugin, 40);
    assert_eq!(next_login_clears(&world), 1);
    assert!(
        world
            .logs(LogLevel::Warn)
            .iter()
            .any(|line| line.contains("The log out did not happen")),
        "{:?}",
        world.all_logs()
    );
}

#[test]
fn a_logout_that_happens_keeps_the_next_login() {
    let (mut world, mut plugin) = lost_bot(true);
    frames(&mut world, &mut plugin, 1220);
    assert_eq!(logouts(&world), 1);
    world.send(&mut plugin, Event::Logoff);
    frames(&mut world, &mut plugin, 1300);
    assert_eq!(next_login_clears(&world), 0);
}

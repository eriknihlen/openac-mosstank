//! The macro while the character can do nothing in the world (register
//! MACRO-1): from a death until the server has carried it out (the health
//! back, or for a death the health never showed, the death teleport over),
//! and in portal space, no rule acts and the meta's chat lines wait. Not a
//! port: the C# and the reference run their rules through both.

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

/// The death teleport as the client reports it: portal space begins
/// (`entered`), or the character is out of it again (the teleport completed).
fn death_teleport(world: &mut World, plugin: &mut MossTank, entered: bool) {
    stand(world, entered);
    world.send(
        plugin,
        Event::PortalTransition(PortalTransition {
            kind: 2,
            is_completed: !entered,
            ..PortalTransition::default()
        }),
    );
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

#[test]
fn a_death_teleport_that_never_comes_is_said_once_and_still_holds() {
    let (mut world, mut plugin) = dead_by_the_clients_word_alone();
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
    let mut world = World::with_character("Acdream");
    stand(&mut world, false);
    world
        .profiles
        .insert("mosstank/metas/Deaths.af", DEATH_LINES_META);
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

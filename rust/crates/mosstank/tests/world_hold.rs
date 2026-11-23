//! The macro while the character can do nothing in the world (register
//! MACRO-1): from a death until the server gives the character its health
//! back, and in portal space, no rule acts. Not a port: the C# and the
//! reference run their rules through both.

mod common;

use ac_plugin_abi::combat::{CombatMode, CombatSnapshot};
use ac_plugin_abi::items::{EquipmentItem, EquipmentPlacement};
use ac_plugin_abi::magic::CastResult;
use ac_plugin_abi::navigation::NavigationSnapshot;
use ac_plugin_abi::world::{ObjectClass, Position, WorldObject};
use ac_plugin_abi::{Event, calls};
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

#[test]
fn the_clients_word_of_a_death_holds_the_macro_before_the_health_shows_it() {
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

    // The death message comes; the health (still 100 here) has not caught up.
    world.send(&mut plugin, Event::LocalPlayerDied("You died!".to_owned()));
    world.vt(&mut plugin, "nav load VitaeRun");
    world.vt(&mut plugin, "opt set EnableNav true");
    frames(&mut world, &mut plugin, 60);
    assert_eq!(recalls(&world), 0, "the death holds the route");

    // A health that never shows the death lets the hold run out.
    frames(&mut world, &mut plugin, 60);
    assert_eq!(recalls(&world), 1);
}

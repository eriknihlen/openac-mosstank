//! The server refuses a corpse someone else has open with a status-line
//! notice, which the client hands the plugin as its own event (never as a
//! chat line); the corpse is then left for ten seconds instead of being
//! asked for again on every pass (ported from the C#'s
//! `MossTankPanelTests.CorpseRefusal`). Those tests are not yet in this
//! branch's C# tree, so they are named in "C# source" lines the census does
//! not read; make them Source lines when the C# tree catches up.

mod common;

use ac_plugin_abi::Event;
use ac_plugin_abi::calls;
use ac_plugin_abi::items::{ItemResult, ItemStatus, LootContainer, LootState};
use ac_plugin_abi::navigation::NavigationSnapshot;
use ac_plugin_abi::world::Position;
use common::World;
use mosstank::MossTank;
use mosstank_formats::utl::{LootRule, VtankLootProfile};

const CORPSE: u32 = 0x7000_0F01;

fn corpse() -> LootContainer {
    LootContainer {
        object_id: CORPSE,
        weenie_class_id: 1,
        name: "Corpse of War Armoredillo".to_owned(),
        distance: 3.0,
        is_identified: true,
        long_description: "Killed by Jerry Seinfeld.".to_owned(),
        position: Some(Position {
            cell_id: 0x0001_0001,
            north_south: 3.0 / 240.0,
            is_outdoor: true,
            ..Position::default()
        }),
        ..LootContainer::default()
    }
}

/// Looting on, every corpse wanted, one corpse in reach, the macro running.
fn looting_world() -> (World, MossTank) {
    let mut world = World::with_character("Jerry Seinfeld");
    world.chat_log = Some(Vec::new());
    world.set_reply::<calls::LootState>(&LootState {
        is_available: true,
        current_contents_ready: true,
        ..LootState::default()
    });
    world.set_reply::<calls::LootOpen>(&ItemResult {
        status: ItemStatus::Started,
        ..ItemResult::default()
    });
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
    world.set_reply::<calls::NavigationSetIntent>(
        &ac_plugin_abi::navigation::NavigationStatus::Accepted,
    );
    let mut rule = LootRule::default();
    "Rule 1".clone_into(&mut rule.name);
    let profile = VtankLootProfile {
        rules: vec![rule],
        ..VtankLootProfile::default()
    };
    world.profiles.insert(
        "mosstank/loot/Refusal.utl",
        &mosstank_formats::utl::write(&profile),
    );
    let mut plugin = world.enable();
    world.vt(&mut plugin, "loot load Refusal");
    world.vt(&mut plugin, "opt set EnableLooting true");
    world.vt(&mut plugin, "opt set LootAllCorpses true");
    world.vt(&mut plugin, "log Loot on");
    world.set_reply::<calls::LootCorpses>(&vec![corpse()]);
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true)
    });
    world.clear_chat();
    (world, plugin)
}

fn tick(world: &mut World, plugin: &mut MossTank, seconds: f64) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, seconds));
}

fn opens(world: &World) -> usize {
    world
        .requests::<calls::LootOpen>()
        .iter()
        .filter(|&&id| id == CORPSE)
        .count()
}

// C# source: MossTankPanelTests.CorpseRefusal.cs :: ACorpseTheServerSaysIsInUseIsLeftForTenSeconds
#[test]
fn a_corpse_the_server_says_is_in_use_is_left_for_ten_seconds() {
    let (mut world, mut plugin) = looting_world();
    for _ in 0..10 {
        if opens(&world) > 0 {
            break;
        }
        tick(&mut world, &mut plugin, 0.3);
    }
    assert_eq!(opens(&world), 1, "{:#?}", world.chat());

    world.send(
        &mut plugin,
        Event::StatusTextReceived(
            "The Corpse of War Armoredillo is already in use by Unkle Leo!".to_owned(),
        ),
    );
    for _ in 0..30 {
        tick(&mut world, &mut plugin, 0.3);
    }

    assert_eq!(opens(&world), 1, "{:#?}", world.chat());
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("refused, skipping it for 10 seconds")),
        "{:#?}",
        world.chat()
    );
}

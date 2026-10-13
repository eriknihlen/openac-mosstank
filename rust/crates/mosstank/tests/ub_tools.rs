//! The UB tools on the plugin's own frame, through the shell (ported from
//! the C#'s `MossTankPanelTests.UbCommands`).

mod common;

use ac_plugin_abi::items::{InventoryItem, ItemResult, ItemStatus};
use ac_plugin_abi::{TICK_SECONDS, calls};
use common::{World, character};

fn bare(id: u32, name: &str) -> InventoryItem {
    InventoryItem {
        object_id: id,
        name: name.to_owned(),
        container_id: 0x5000_0001,
        stack_size: 1,
        ..InventoryItem::default()
    }
}

/// Ticks for so many seconds of the C#'s frames.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn ticks_for(seconds: f64) -> usize {
    (seconds / TICK_SECONDS).round() as usize
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: ClearBuggedRunsOnThePanelTickAndStopsWithTheSession
#[test]
fn clear_bugged_runs_on_the_panel_tick_and_stops_with_the_session() {
    let mut world = World::with_character("Test Character");
    world.set_reply::<calls::ItemsOwned>(&vec![
        bare(0x8000_0010, "Phantom"),
        bare(0x8000_0011, "Other"),
    ]);
    world.set_reply::<calls::WorldIdentify>(&ItemResult::of(ItemStatus::Started));
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    world.clear_chat();

    world.command(&mut plugin, "ub", "clearbugged");
    world.ticks(&mut plugin, ticks_for(0.3));

    assert_eq!(
        "[UB] InventoryManager: ClearBugged: Identifying 2 items, to check for bugged items...",
        world.chat()[0]
    );
    assert_eq!(vec![0x8000_0010], world.requests::<calls::WorldIdentify>());

    world.fake.character = None;
    world.tick(&mut plugin);
    world.fake.character = Some(character("Test Character"));
    world.clear_chat();
    world.ticks(&mut plugin, ticks_for(20.0));

    assert_eq!(vec![0x8000_0010], world.requests::<calls::WorldIdentify>());
    assert!(
        !world
            .chat()
            .iter()
            .any(|line| line.starts_with("ClearBugged"))
    );
}

// ---- Not a port of a C# test: what holds the pass --------------------------
//
// The C# has no test for the owner the suspension report names; this one
// checks that a running give is named as the give, not as a command jump.

#[test]
fn a_running_give_is_named_as_what_holds_the_pass() {
    use ac_plugin_abi::navigation::NavigationSnapshot;
    use ac_plugin_abi::world::{ObjectClass, Position, WorldObject};

    let mut world = World::with_character("Barris");
    let here = Position {
        cell_id: 0x0001_0001,
        is_outdoor: true,
        ..Position::default()
    };
    world.set_reply::<calls::NavigationSnapshot>(&NavigationSnapshot {
        is_available: true,
        local_object_id: 1,
        position: here.clone(),
        ..NavigationSnapshot::default()
    });
    let mule = WorldObject {
        object_id: 0x5000_0ABC,
        name: "Mule".to_owned(),
        object_class: ObjectClass::Player,
        position: Some(here),
        ..WorldObject::default()
    };
    world.set_reply::<calls::WorldObjects>(&vec![mule.clone()]);
    world.set_reply::<calls::WorldObject>(&Some(mule));
    world.set_reply::<calls::ItemsOwned>(&vec![bare(0x8000_0010, "Pyreal")]);
    world.set_reply::<calls::ItemsGive>(&ItemResult::of(ItemStatus::Started));
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    world.with_tank(&mut plugin, |tank, client| {
        tank.set_macro_running(client, true);
    });
    world.command(&mut plugin, "ub", "give Pyreal to Mule");
    world.ticks(&mut plugin, ticks_for(1.0));

    let lines = plugin.tank().expect("enabled").action_history_lines();
    assert!(
        lines
            .iter()
            .any(|line| line.ends_with("Pause: a profile give owns the character")),
        "{lines:?}"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("a command jump owns the character")),
        "{lines:?}"
    );
}

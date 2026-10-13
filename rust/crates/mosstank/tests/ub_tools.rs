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

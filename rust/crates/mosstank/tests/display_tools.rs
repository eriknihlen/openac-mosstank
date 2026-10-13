//! The display tools' canvases as the client holds them: the countryside
//! map registered when its window opens, painted when it asks, driven by
//! pointer input on its canvas, and given back with the plugin
//! (`MossTankPanel.UbDisplay`).

mod common;

use ac_plugin_abi::Event;
use ac_plugin_abi::calls;
use ac_plugin_abi::ui::{PointerEvent, UiEvent};
use common::World;
use mosstank::MossTank;

fn canvas(world: &World, canvas_id: &str) -> Option<u32> {
    world
        .fake
        .ui
        .canvases()
        .find(|canvas| canvas.descriptor.canvas_id == canvas_id)
        .map(|canvas| canvas.id)
}

fn paints(world: &World) -> usize {
    world.requests::<calls::UiCanvasPaint>().len()
}

fn open_map() -> (World, MossTank) {
    let mut world = World::with_character("Mapper");
    let mut plugin = world.enable();
    world.act(&mut plugin, "ShowUbMap");
    world.tick(&mut plugin);
    (world, plugin)
}

// Source: src/AcDream.Plugins.MossTank/MossTankPanel.UbDisplay.cs :: TickDisplayTools
/// Opening the map window registers its canvas and paints it on the frame.
#[test]
fn opening_the_map_registers_and_paints_its_canvas() {
    let (world, _plugin) = open_map();
    assert!(canvas(&world, "ub-map").is_some());
    assert!(paints(&world) >= 1);
}

/// A wheel notch over the canvas zooms the map, so it paints again.
#[test]
fn the_wheel_over_the_map_zooms_and_repaints() {
    let (mut world, mut plugin) = open_map();
    let id = canvas(&world, "ub-map").unwrap();
    let before = paints(&world);
    world.send(
        &mut plugin,
        Event::Ui(UiEvent::Pointer(PointerEvent {
            canvas: id,
            kind: 3,
            x: 100.0,
            y: 100.0,
            button: 0,
            modifiers: 0,
            wheel_delta: 1,
        })),
    );
    world.tick(&mut plugin);
    assert!(paints(&world) > before);
}

/// Closing the window hides the canvas; the plugin's end removes it.
#[test]
fn the_canvas_hides_with_its_window_and_goes_with_the_plugin() {
    let (mut world, mut plugin) = open_map();
    world.act(&mut plugin, "HideUbMap");
    world.tick(&mut plugin);
    let id = canvas(&world, "ub-map").unwrap();
    assert!(world.requests::<calls::UiCanvasSet>().iter().any(
        |set| set.canvas == id && set.change == ac_plugin_abi::ui::CanvasChange::Visible(false)
    ));
    world.disable(&mut plugin);
    assert!(world.requests::<calls::UiCanvasRemove>().contains(&id));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbDungeon.cs :: TheDungeonWindowDrivesTheMapAndTheZoomReachesTheRow
#[test]
fn the_dungeon_window_drives_the_map_and_the_zoom_reaches_the_row() {
    let mut world = World::with_character("Acdream");
    let mut plugin = world.enable();
    let p = &mut plugin;
    world.with_tank(p, |tank, client| tank.on_tick(client, 0.1));

    assert!(!world.ui_bool(p, "UbDungeonVisible"));
    world.act(p, "ShowUbDungeon");
    assert!(world.ui_bool(p, "UbDungeonVisible"));
    assert_eq!(world.ui_text(p, "UbDungeonStatusText"), "Not in a dungeon.");
    assert_eq!(world.ui_text(p, "UbDungeonFollowText"), "Follow: on");
    assert_eq!(world.ui_text(p, "UbDungeonLayersText"), "Layers: near");
    world.act(p, "ToggleUbDungeonLayers");
    assert_eq!(world.ui_text(p, "UbDungeonLayersText"), "Layers: all");

    world.act_text(p, "SetUbFilterText", "DungeonMaps.MapZoom");
    assert_eq!(world.ui_list(p, "UbSettingValues")[0], "4.2");
    world.act(p, "UbDungeonZoomIn");
    world.act_text(p, "SetUbFilterText", "DungeonMaps.MapZoom");
    assert_eq!(world.ui_list(p, "UbSettingValues")[0], "4");

    world.act_float(p, "SetUbDungeonHeight", 50.0);
    assert_eq!(world.ui_text(p, "UbDungeonFollowText"), "Follow: off");
    world.act(p, "ToggleUbDungeonFollow");
    assert_eq!(world.ui_text(p, "UbDungeonFollowText"), "Follow: on");
    world.act(p, "HideUbDungeon");
    assert!(!world.ui_bool(p, "UbDungeonVisible"));
    world.disable(p);
}

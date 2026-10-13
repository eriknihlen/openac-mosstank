//! The meta's windows through the SDK's plugin UI: a view a meta creates is
//! drawn with its controls' text and visibility and its buttons' clicks, the
//! status list is drawn with its rows, and both go with the session and the
//! plugin.
//!
//! Not ports of C# tests: every C# panel test runs without a window
//! (`HasUi` false), and `MetaViewManagerTests` (ported in `mosstank-meta`)
//! drive the views over a recording host. These check that the shell hands
//! the meta's windows to the client and back.

mod common;

use ac_plugin_abi::Event;
use ac_plugin_abi::ui::{ActionValue, UiAction, UiEvent, UiValue};
use common::World;
use mosstank::MossTank;
use mosstank_formats::meta::{MetaAction, MetaActionKind, MetaProfile, MetaRule};
use mosstank_meta::view_manager::window_id;

/// A board of buttons in the shape the published multi-client metas use
/// (the C#'s `MetaViewManagerTests.PartyView`).
const BOARD: &str = "<?xml version=\"1.0\"?> <view width=\"240\" height=\"100\" title=\"PartyBoard\">\t<control type=\"layout\" name=\"Grid\" text=\"Command\">
\t\t<control type=\"button\" name=\"Member1\" left=\"0\" top=\"0\" width=\"90\" height=\"25\" text=\"Barris\" />
\t\t<control type=\"button\" name=\"Member1State\" left=\"90\" top=\"0\" width=\"150\" height=\"25\" text=\"Ping\" actionexpr=\"setvar[`pinged`,1]\" />
\t\t<control type=\"button\" name=\"Home\" left=\"0\" top=\"75\" width=\"240\" height=\"25\" text=\"Home\" setstate=\"Default\" />
\t</control></view>";

/// A meta whose first pass draws the board and moves on.
fn board_meta() -> MetaProfile {
    MetaProfile {
        rules: vec![MetaRule {
            action: MetaAction {
                kind: MetaActionKind::All,
                children: vec![
                    MetaAction {
                        kind: MetaActionKind::CreateView,
                        text: "Board".to_owned(),
                        secondary_text: BOARD.to_owned(),
                        ..MetaAction::default()
                    },
                    MetaAction {
                        kind: MetaActionKind::SetMetaState,
                        text: "Shown".to_owned(),
                        ..MetaAction::default()
                    },
                ],
                ..MetaAction::default()
            },
            ..MetaRule::default()
        }],
    }
}

/// In the world with a window attached.
fn windowed() -> (World, MossTank) {
    let mut world = World::with_character("Barris");
    world.fake.window_attached = true;
    world.fake.ui.set_window_attached(true);
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    (world, plugin)
}

fn panel_id(world: &World, window: &str) -> Option<u32> {
    world
        .fake
        .ui
        .panels()
        .find(|panel| panel.descriptor.window_id == window)
        .map(|panel| panel.id)
}

fn run_board(world: &mut World, plugin: &mut MossTank) -> u32 {
    let meta = mosstank_formats::met::save(&board_meta()).unwrap();
    world.profiles.insert("mosstank/metas/Board.met", &meta);
    world.vt(plugin, "meta load Board");
    world.vt(plugin, "opt set EnableMeta true");
    // The meta runs on the macro's pass.
    world.with_tank(plugin, |tank, client| tank.set_macro_running(client, true));
    for _ in 0..40 {
        world.tick(plugin);
        if let Some(panel) = panel_id(world, &window_id("Board")) {
            // The values follow within a push (a tenth of a second).
            world.ticks(plugin, 10);
            return panel;
        }
    }
    panic!("the board was not drawn: {:?}", world.chat());
}

#[test]
fn a_view_a_meta_creates_is_drawn_with_its_controls() {
    let (mut world, mut plugin) = windowed();

    let panel = run_board(&mut world, &mut plugin);

    let drawn = world
        .fake
        .ui
        .panels()
        .find(|view| view.id == panel)
        .unwrap();
    assert_eq!(drawn.descriptor.icon_text.as_deref(), Some("B"));
    assert_eq!(drawn.descriptor.icon_text.as_deref(), Some("B"));
    assert_eq!(
        world.fake.panel_value(panel, "Text1"),
        Some(UiValue::Text("Barris".to_owned()))
    );
    assert_eq!(
        world.fake.panel_value(panel, "Text2"),
        Some(UiValue::Text("Ping".to_owned()))
    );
    assert_eq!(
        world.fake.panel_value(panel, "Visible0"),
        Some(UiValue::Bool(true))
    );
    assert_eq!(plugin.meta_windows().count(), 1);
}

#[test]
fn a_click_on_a_drawn_button_runs_the_control_it_was_drawn_from() {
    let (mut world, mut plugin) = windowed();
    let panel = run_board(&mut world, &mut plugin);

    world.send(
        &mut plugin,
        Event::Ui(UiEvent::Action(UiAction {
            panel,
            handler: "Click2".to_owned(),
            value: ActionValue::None,
        })),
    );

    let pinged = world.with_tank(&mut plugin, |tank, client| {
        tank.evaluate_expression(client, "getvar[`pinged`]")
    });
    assert_eq!(pinged.unwrap().to_display_string(), "1");
}

#[test]
fn uisetlabel_shows_on_the_drawn_window() {
    let (mut world, mut plugin) = windowed();
    let panel = run_board(&mut world, &mut plugin);

    world.vt(
        &mut plugin,
        "mexec uisetlabel[uigetcontrol[`Board`,`Member1`],`Horan`]",
    );

    assert_eq!(
        world.fake.panel_value(panel, "Text1"),
        Some(UiValue::Text("Horan".to_owned()))
    );
}

#[test]
fn the_meta_windows_go_with_the_session() {
    let (mut world, mut plugin) = windowed();
    run_board(&mut world, &mut plugin);
    world.vt(&mut plugin, "mexec statushud[`Kills`,`5`]");
    assert!(panel_id(&world, "vtank-meta-status").is_some());

    world.fake.character = None;
    world.tick(&mut plugin);

    assert_eq!(panel_id(&world, &window_id("Board")), None);
    assert_eq!(panel_id(&world, "vtank-meta-status"), None);
    assert_eq!(plugin.meta_windows().count(), 0);
}

#[test]
fn the_meta_windows_go_with_the_plugin() {
    let (mut world, mut plugin) = windowed();
    run_board(&mut world, &mut plugin);

    world.disable(&mut plugin);

    assert_eq!(panel_id(&world, &window_id("Board")), None);
    assert_eq!(plugin.meta_windows().count(), 0);
}

#[test]
fn the_status_list_is_drawn_with_its_rows() {
    let (mut world, mut plugin) = windowed();

    world.vt(&mut plugin, "mexec statushud[`Kills`,`5`]");
    world.vt(&mut plugin, "mexec statushudcolored[`State`,`Buffing`,255]");
    world.vt(&mut plugin, "mexec statushud[`Kills`,`6`]");

    let panel = panel_id(&world, "vtank-meta-status").expect("the status window");
    let drawn = world
        .fake
        .ui
        .panels()
        .find(|view| view.id == panel)
        .unwrap();
    assert_eq!(drawn.descriptor.title, "VTank Meta Status");
    assert_eq!(drawn.descriptor.icon_text.as_deref(), Some("S"));
    assert_eq!(
        world.fake.panel_value(panel, "Rows"),
        Some(UiValue::TextList(vec![
            "Kills: 6".to_owned(),
            "State: Buffing".to_owned()
        ]))
    );
    assert_eq!(
        world.fake.panel_value(panel, "SelectedRow"),
        Some(UiValue::Int(-1))
    );
    assert_eq!(
        world.fake.panel_value(panel, "WindowAvailable"),
        Some(UiValue::Bool(true))
    );
}

#[test]
fn without_a_window_nothing_is_drawn() {
    let mut world = World::with_character("Barris");
    let mut plugin = world.enable();
    world.tick(&mut plugin);

    world.vt(&mut plugin, "mexec statushud[`Kills`,`5`]");

    assert_eq!(panel_id(&world, "vtank-meta-status"), None);
    assert_eq!(plugin.meta_windows().count(), 0);
}

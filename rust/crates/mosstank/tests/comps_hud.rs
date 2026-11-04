//! The Comps HUD as the player reaches it through the plugin: `/vt
//! compshud`, its box on the Consumables page (Show HUD, the layout bar,
//! Edit items, the summary, Add to HUD), the Edit items window, the strip's
//! buttons on its canvas, and the theme it is painted in.

mod common;

use ac_plugin_abi::Event;
use ac_plugin_abi::calls;
use ac_plugin_abi::ui::{
    ActionValue, Color, PaintCmd, PointerEvent, ThemeInfo, ThemeKind, ThemePalette, UiEvent,
    UiValue,
};
use ac_plugin_abi::world::{ObjectClass, WorldObject};
use common::World;
use mosstank::MossTank;
use mosstank_ub::comps_hud::{HudButton, HudGeometry, HudLayout};

const PEA_ID: u32 = 0x8000_0012;
const PEA_ICON: u32 = 0x0600_1F61;
const SCARAB_ID: u32 = 0x8000_0010;
const SCARAB_ICON: u32 = 0x0600_1F5B;

fn owned(object_id: u32, name: &str, stack: i32, icon: u32) -> WorldObject {
    WorldObject {
        object_id,
        name: name.to_owned(),
        object_class: ObjectClass::SpellComponent,
        container_id: 0x5000_0001,
        is_owned: true,
        stack_size: stack,
        icon_id: icon,
        ..WorldObject::default()
    }
}

/// Acdream in the world on Coldeve, carrying a pea and some scarabs.
fn carrying() -> (World, MossTank) {
    let mut world = World::with_character("Acdream");
    world.set_reply::<calls::WorldObjects>(&vec![
        owned(PEA_ID, "Gold Pea", 8, PEA_ICON),
        owned(SCARAB_ID, "Platinum Scarab", 3, SCARAB_ICON),
    ]);
    let mut plugin = world.enable();
    world.ticks(&mut plugin, 2);
    (world, plugin)
}

fn canvas(world: &World) -> Option<(u32, bool, i32, i32)> {
    world
        .fake
        .ui
        .canvases()
        .filter(|canvas| canvas.descriptor.canvas_id == "comps-hud")
        .last()
        .map(|canvas| {
            (
                canvas.id,
                canvas.visible,
                canvas.descriptor.width,
                canvas.descriptor.height,
            )
        })
}

fn click(world: &mut World, plugin: &mut MossTank, at: (f64, f64)) {
    let (id, ..) = canvas(world).expect("the HUD's canvas");
    for kind in [0, 1] {
        #[allow(clippy::cast_possible_truncation)]
        world.send(
            plugin,
            Event::Ui(UiEvent::Pointer(PointerEvent {
                canvas: id,
                kind,
                x: at.0 as f32,
                y: at.1 as f32,
                button: 1,
                modifiers: 0,
                wheel_delta: 0,
            })),
        );
    }
}

fn centre((x, y, width, height): (f64, f64, f64, f64)) -> (f64, f64) {
    (x + width / 2.0, y + height / 2.0)
}

fn rows(world: &mut World, plugin: &mut MossTank) -> Vec<String> {
    world.ui_list(plugin, "CompsHudRowNames")
}

/// `line` and `column` lay the strip out, as the layout bar shows; out of
/// the world they say so.
#[test]
fn the_command_lays_the_hud_out_as_a_line_or_a_column() {
    let (mut world, mut plugin) = carrying();
    let p = &mut plugin;
    world.vt(p, "compshud on");
    world.ticks(p, 2);
    assert_eq!(0, world.ui_int(p, "CompsHudLayoutIndex"));

    world.vt(p, "compshud line");
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("Comps HUD: laid out as a line."))
    );
    assert_eq!(1, world.ui_int(p, "CompsHudLayoutIndex"));
    world.ticks(p, 2);
    let (_, visible, width, height) = canvas(&world).unwrap();
    assert!(visible);
    assert_eq!(
        HudGeometry::new(HudLayout::Line, &[]).size(),
        (width, height)
    );

    world.vt(p, "compshud COLUMN");
    assert_eq!(0, world.ui_int(p, "CompsHudLayoutIndex"));
    world.disable(p);

    let mut away = World::new();
    let mut plugin = away.enable();
    away.vt(&mut plugin, "compshud line");
    assert!(
        away.chat()
            .iter()
            .any(|line| line.contains("Comps HUD: not in world."))
    );
}

/// Show HUD is the shared switch, the layout bar sets the layout, Edit
/// items opens the window, and the summary names what the HUD counts.
#[test]
fn the_consumables_box_drives_the_hud() {
    let (mut world, mut plugin) = carrying();
    let p = &mut plugin;
    assert!(!world.ui_bool(p, "CompsHudShown"));
    assert_eq!("No items yet.", world.ui_text(p, "CompsHudSummary"));

    world.act_with(p, "SetCompsHudShown", ActionValue::Int(1));
    assert!(world.ui_bool(p, "CompsHudShown"));
    world.ticks(p, 2);
    assert!(canvas(&world).unwrap().1);

    world.act_int(p, "SetCompsHudLayout", 1);
    assert_eq!(1, world.ui_int(p, "CompsHudLayoutIndex"));
    world.act_int(p, "SetCompsHudLayout", 0);
    assert_eq!(0, world.ui_int(p, "CompsHudLayoutIndex"));

    assert!(!world.ui_bool(p, "CompsHudEditorVisible"));
    world.act(p, "ShowCompsHudEditor");
    assert!(world.ui_bool(p, "CompsHudEditorVisible"));
    world.act_int(p, "ToggleCompsHudCarriedAt", 0);
    world.act_int(p, "ToggleCompsHudCarriedAt", 1);
    assert_eq!(
        "2 items: Gold Pea, Platinum Scarab",
        world.ui_text(p, "CompsHudSummary")
    );

    world.act_with(p, "SetCompsHudShown", ActionValue::Int(0));
    assert!(!world.ui_bool(p, "CompsHudShown"));
    world.ticks(p, 2);
    assert!(!canvas(&world).unwrap().1);
    world.disable(p);
}

/// Add to HUD puts the item selected in the game on the HUD, whichever
/// Consumables row is highlighted (a click on a row takes it off the page),
/// and says why in chat when it cannot: nothing selected, or listed
/// already.
#[test]
fn add_to_hud_adds_the_item_selected_in_the_game() {
    let (mut world, mut plugin) = carrying();
    let p = &mut plugin;
    assert_eq!(
        ["Asheron's Benediction", "Blackmoor's Favor"],
        world.ui_list(p, "ConsumableRows").as_slice()
    );

    world.clear_chat();
    world.act(p, "AddConsumableToCompsHud");
    assert!(rows(&mut world, p).is_empty(), "no row is added");
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("Comps HUD: select an item first."))
    );
    assert_eq!("Select an item first.", world.ui_text(p, "ProfileNotice"));

    world.set_reply::<calls::WorldObject>(&Some(owned(PEA_ID, "Gold Pea", 8, PEA_ICON)));
    world.select_object(PEA_ID);
    world.act(p, "AddConsumableToCompsHud");
    assert_eq!(["Gold Pea"], rows(&mut world, p).as_slice());
    // The window draws it from the carried object, once it is counted.
    world.act(p, "ShowCompsHudEditor");
    world.ticks(p, 1);
    assert_eq!(vec![PEA_ID], world.ui_uints(p, "CompsHudRowIcons"));
    assert_eq!(
        "Added Gold Pea to the Comps HUD.",
        world.ui_text(p, "ProfileNotice")
    );

    world.clear_chat();
    world.act(p, "AddConsumableToCompsHud");
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("Comps HUD: Gold Pea is listed already."))
    );
    world.disable(p);
}

/// Out of the world the layout bar changes nothing and says why.
#[test]
fn the_layout_bar_out_of_the_world_says_so() {
    let mut world = World::new();
    let mut plugin = world.enable();
    let p = &mut plugin;
    world.act_int(p, "SetCompsHudLayout", 1);
    assert_eq!(0, world.ui_int(p, "CompsHudLayoutIndex"));
    assert_eq!(
        "Comps HUD: not in world.",
        world.ui_text(p, "ProfileNotice")
    );
    world.disable(p);
}

/// `/vt compshud reset` puts the strip back at its first place.
#[test]
fn the_reset_word_puts_the_strip_back() {
    let (mut world, mut plugin) = carrying();
    let p = &mut plugin;
    world.vt(p, "compshud on");
    world.ticks(p, 2);
    let (id, ..) = canvas(&world).unwrap();
    for (kind, at) in [(0, (4.0, 30.0)), (1, (104.0, 90.0))] {
        world.send(
            p,
            Event::Ui(UiEvent::Pointer(PointerEvent {
                canvas: id,
                kind,
                x: at.0,
                y: at.1,
                button: 1,
                modifiers: 0,
                wheel_delta: 0,
            })),
        );
    }
    world.ticks(p, 2);
    assert_eq!(Some((300.0, 210.0)), offset(&world, "comps-hud"));
    world.vt(p, "compshud reset");
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("Comps HUD: back at its first place."))
    );
    world.ticks(p, 2);
    assert_eq!(Some((200.0, 150.0)), offset(&world, "comps-hud"));
    world.disable(p);
}

fn offset(world: &World, canvas_id: &str) -> Option<(f32, f32)> {
    world
        .fake
        .ui
        .canvases()
        .filter(|canvas| canvas.descriptor.canvas_id == canvas_id)
        .last()
        .map(|canvas| canvas.offset)
}

/// Hovering an item on the strip shows its name in the tooltip canvas,
/// painted in the theme; leaving the strip hides it.
#[test]
fn hovering_the_strip_shows_the_items_name() {
    let (mut world, mut plugin) = carrying();
    let p = &mut plugin;
    world.vt(p, "compshud on");
    world.set_reply::<calls::WorldObject>(&Some(owned(PEA_ID, "Gold Pea", 8, PEA_ICON)));
    world.select_object(PEA_ID);
    world.vt(p, "compshud add");
    world.ticks(p, 3);
    let (id, ..) = canvas(&world).unwrap();
    let tip_shown = |world: &World| {
        world
            .fake
            .ui
            .canvases()
            .find(|canvas| canvas.descriptor.canvas_id == "comps-hud-tip")
            .map(|canvas| canvas.visible)
    };
    assert_eq!(Some(false), tip_shown(&world));
    let paints_before = world.requests::<calls::UiCanvasPaint>().len();

    let column = HudGeometry::new(HudLayout::Column, &[7.0]);
    let at = centre(column.items[0].cell);
    #[allow(clippy::cast_possible_truncation)]
    world.send(
        p,
        Event::Ui(UiEvent::Pointer(PointerEvent {
            canvas: id,
            kind: PointerEvent::HOVER,
            x: at.0 as f32,
            y: at.1 as f32,
            button: 0,
            modifiers: 0,
            wheel_delta: 0,
        })),
    );
    world.ticks(p, 2);
    assert_eq!(Some(true), tip_shown(&world));
    let named = world
        .requests::<calls::UiCanvasPaint>()
        .iter()
        .skip(paints_before)
        .any(|paint| {
            paint
                .commands
                .iter()
                .any(|command| matches!(command, PaintCmd::Text(text, ..) if text == "Gold Pea"))
        });
    assert!(named, "the tip is painted with the item's name");

    world.send(
        p,
        Event::Ui(UiEvent::Pointer(PointerEvent {
            canvas: id,
            kind: PointerEvent::LEAVE,
            x: 0.0,
            y: 0.0,
            button: 0,
            modifiers: 0,
            wheel_delta: 0,
        })),
    );
    world.ticks(p, 2);
    assert_eq!(Some(false), tip_shown(&world));
    world.disable(p);
}

/// The Edit items window lists what is carried, filters it, ticks items
/// onto the HUD, orders them, sets the selected one's warn level, removes
/// it, and adds the game's selection; each change shows at once.
#[test]
fn the_edit_items_window_edits_the_hud() {
    let (mut world, mut plugin) = carrying();
    let p = &mut plugin;
    world.act(p, "ShowCompsHudEditor");
    assert_eq!("You carry", world.ui_text(p, "CompsHudCarriedHeading"));
    assert!(
        world
            .ui_text(p, "CompsHudRowsHeading")
            .contains("nothing yet")
    );
    assert_eq!(-1, world.ui_int(p, "SelectedCompsHudRowIndex"));
    assert_eq!(0, world.ui_int(p, "CompsHudSelectedWarn"));
    world.act_int(p, "SetCompsHudSelectedWarn", 9);
    assert_eq!(0, world.ui_int(p, "CompsHudSelectedWarn"));
    assert_eq!(
        ["Gold Pea", "Platinum Scarab"],
        world.ui_list(p, "CompsHudCarriedNames").as_slice()
    );
    assert_eq!(
        ["8", "3"],
        world.ui_list(p, "CompsHudCarriedCounts").as_slice()
    );
    // Each row's icon is its object's, composed as the game draws it.
    assert_eq!(
        vec![PEA_ID, SCARAB_ID],
        world.ui_uints(p, "CompsHudCarriedIcons")
    );
    assert_eq!(
        vec![false, false],
        world.ui_bools(p, "CompsHudCarriedListed")
    );
    assert_eq!("All", world.ui_text(p, "CompsHudKind"));

    world.act_int(p, "ToggleCompsHudCarriedAt", 1);
    world.act_int(p, "ToggleCompsHudCarriedAt", 0);
    assert_eq!(vec![true, true], world.ui_bools(p, "CompsHudCarriedListed"));
    assert_eq!(
        ["Platinum Scarab", "Gold Pea"],
        rows(&mut world, p).as_slice()
    );

    world.act_text(p, "SetCompsHudSearch", "scarab");
    assert_eq!(
        ["Platinum Scarab"],
        world.ui_list(p, "CompsHudCarriedNames").as_slice()
    );
    world.act_text(p, "SetCompsHudSearch", "");
    world.act_text(p, "SelectCompsHudKind", "Peas and salvage");
    assert_eq!(
        ["Gold Pea"],
        world.ui_list(p, "CompsHudCarriedNames").as_slice()
    );
    world.act_text(p, "SelectCompsHudKind", "All");

    world.act_int(p, "SelectCompsHudRow", 1);
    world.act(p, "MoveCompsHudRowUp");
    assert_eq!(
        ["Gold Pea", "Platinum Scarab"],
        rows(&mut world, p).as_slice()
    );
    assert_eq!(0, world.ui_int(p, "SelectedCompsHudRowIndex"));
    world.act_int(p, "SetCompsHudSelectedWarn", 20);
    assert_eq!(20, world.ui_int(p, "CompsHudSelectedWarn"));
    assert_eq!(
        ["warn 20", ""],
        world.ui_list(p, "CompsHudRowWarns").as_slice()
    );
    world.act(p, "MoveCompsHudRowDown");
    assert_eq!(
        ["Platinum Scarab", "Gold Pea"],
        rows(&mut world, p).as_slice()
    );
    assert_eq!(20, world.ui_int(p, "CompsHudSelectedWarn"));
    world.act(p, "RemoveCompsHudRow");
    assert_eq!(["Platinum Scarab"], rows(&mut world, p).as_slice());

    world.set_reply::<calls::WorldObject>(&Some(owned(SCARAB_ID, "Platinum Scarab", 3, 0)));
    world.select_object(SCARAB_ID);
    world.act(p, "AddSelectedToCompsHud");
    assert_eq!(
        "Platinum Scarab is listed already.",
        world.ui_text(p, "CompsHudEditorNotice")
    );

    world.act(p, "HideCompsHudEditor");
    assert!(!world.ui_bool(p, "CompsHudEditorVisible"));
    world.disable(p);
}

/// The strip's buttons, clicked on its canvas: the pencil opens Edit
/// items, the layout switch lays it out as a line, and the cross hides it
/// and unticks Show HUD.
#[test]
fn the_strips_buttons_open_edit_items_switch_the_layout_and_hide_it() {
    let (mut world, mut plugin) = carrying();
    let p = &mut plugin;
    world.vt(p, "compshud on");
    world.ticks(p, 2);
    let column = HudGeometry::new(HudLayout::Column, &[]);

    click(&mut world, p, centre(column.button(HudButton::Edit)));
    assert!(world.ui_bool(p, "CompsHudEditorVisible"));

    click(&mut world, p, centre(column.button(HudButton::Layout)));
    assert_eq!(1, world.ui_int(p, "CompsHudLayoutIndex"));
    world.ticks(p, 2);
    let line = HudGeometry::new(HudLayout::Line, &[]);
    let (_, _, width, height) = canvas(&world).unwrap();
    assert_eq!(line.size(), (width, height));

    click(&mut world, p, centre(line.button(HudButton::Hide)));
    assert!(!world.ui_bool(p, "CompsHudShown"));
    world.ticks(p, 2);
    assert!(!canvas(&world).unwrap().1);
    world.disable(p);
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b, a: 255 }
}

/// A theme picked while the HUD shows repaints it in that theme: its face
/// and its text colour, the text in the bundled font.
#[test]
fn a_theme_change_repaints_the_strip_in_the_new_theme() {
    let (mut world, mut plugin) = carrying();
    let p = &mut plugin;
    world.vt(p, "compshud on");
    world.select_object(PEA_ID);
    world.set_reply::<calls::WorldObject>(&Some(owned(PEA_ID, "Gold Pea", 8, PEA_ICON)));
    world.vt(p, "compshud add");
    world.ticks(p, 3);
    let painted = world.requests::<calls::UiCanvasPaint>().len();
    assert!(painted >= 1);
    assert!(world.requests::<calls::UiFontBundled>().is_empty());

    let face = rgb(0xED, 0xE7, 0xDC);
    let ink = rgb(0x2A, 0x25, 0x20);
    world.set_reply::<calls::UiFontBundled>(&ac_plugin_abi::ui::FontInfo {
        handle: 9,
        pixel_size: 11.0,
        line_height: 15.0,
        ascent: 11.0,
    });
    world.send(
        p,
        Event::ThemeChanged(ThemeInfo {
            kind: ThemeKind::Parchment,
            display_name: "Parchment".to_owned(),
            palette: Some(ThemePalette {
                background: face.clone(),
                field: rgb(0xFA, 0xF7, 0xF1),
                border: rgb(0xC9, 0xBF, 0xAE),
                text: ink.clone(),
                muted: rgb(0x65, 0x5C, 0x50),
                accent: rgb(0xB4, 0x47, 0x1F),
                selected: rgb(0xDD, 0xCF, 0xB6),
            }),
        }),
    );
    world.ticks(p, 2);
    let paints = world.requests::<calls::UiCanvasPaint>();
    assert!(paints.len() > painted, "the theme change repaints");
    let last = paints.last().unwrap();
    assert!(
        last.commands
            .iter()
            .any(|command| matches!(command, PaintCmd::FillRect(_, color) if *color == face))
    );
    let counts: Vec<(&str, &Color, u32)> = last
        .commands
        .iter()
        .filter_map(|command| match command {
            PaintCmd::Text(text, _, _, color, _, font) => Some((text.as_str(), color, *font)),
            _ => None,
        })
        .collect();
    assert_eq!(vec![("8", &ink, 9)], counts);
    assert_eq!(1, world.requests::<calls::UiFontBundled>().len());
    world.disable(p);
}

/// `/vt compshud` with an unknown word answers with the usage, which
/// names `line` and `column`.
#[test]
fn the_usage_names_the_layouts() {
    let (mut world, mut plugin) = carrying();
    let p = &mut plugin;
    world.vt(p, "compshud sideways");
    let usage = mosstank_ub::help::COMPONENT_HUD_SYNTAX;
    assert!(usage.contains("line") && usage.contains("column") && usage.contains("reset"));
    assert!(world.chat().iter().any(|line| line.contains(usage)));
    assert!(matches!(
        world.ui(p, "CompsHudKindNames"),
        UiValue::TextList(kinds) if kinds.len() == 6
    ));
    world.disable(p);
}

/// A move and the release in one frame: the host measures the release
/// from where the plugin last put the canvas, so the move's new place goes
/// to the host before the release comes, and the step is not counted
/// twice (canvas at 200, grab at 10, a move to 240 puts it at 230; the
/// release at 241 then lands it at 231, not 261).
#[test]
fn a_move_and_a_release_in_one_frame_land_where_the_pointer_is() {
    let (mut world, mut plugin) = carrying();
    let p = &mut plugin;
    world.vt(p, "compshud on");
    world.ticks(p, 2);
    let (id, ..) = canvas(&world).unwrap();
    let screen = |world: &World, at: (f32, f32)| {
        // As the host reports it: from where it has the canvas now.
        let (x, y) = offset(world, "comps-hud").unwrap();
        (at.0 - x, at.1 - y)
    };
    let send = |world: &mut World, p: &mut MossTank, kind: u8, at: (f32, f32)| {
        world.send(
            p,
            Event::Ui(UiEvent::Pointer(PointerEvent {
                canvas: id,
                kind,
                x: at.0,
                y: at.1,
                button: 1,
                modifiers: 0,
                wheel_delta: 0,
            })),
        );
    };
    assert_eq!(Some((200.0, 150.0)), offset(&world, "comps-hud"));
    send(&mut world, p, PointerEvent::DOWN, (10.0, 20.0));
    let at = screen(&world, (240.0, 170.0));
    send(&mut world, p, PointerEvent::MOVE, at);
    let at = screen(&world, (241.0, 170.0));
    send(&mut world, p, PointerEvent::UP, at);
    assert_eq!(Some((231.0, 150.0)), offset(&world, "comps-hud"));
    world.ticks(p, 2);
    assert_eq!(Some((231.0, 150.0)), offset(&world, "comps-hud"));
    world.disable(p);
}

/// At logoff the display tools' resets go out at once: the frame that
/// would hand them to the host does not run while logged off, so without
/// it the HUD stayed shown (and its images held) until the next login.
#[test]
fn the_session_end_hides_the_hud_at_once() {
    let (mut world, mut plugin) = carrying();
    let p = &mut plugin;
    world.vt(p, "compshud on");
    world.ticks(p, 2);
    assert!(canvas(&world).unwrap().1);
    let me = world.fake.character.take();
    world.ticks(p, 1);
    assert!(!canvas(&world).unwrap().1, "hidden at the session's end");
    world.ticks(p, 3);
    assert!(!canvas(&world).unwrap().1);
    world.fake.character = me;
    world.disable(p);
}

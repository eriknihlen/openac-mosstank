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
use mosstank_loot::crafting::ALL_PEAS;
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

/// Add to HUD puts the selected Consumables row's item on the HUD (with
/// the icon of one the character owns), refuses it twice and refuses the
/// all-peas rule; with no row, it adds the item selected in the game.
#[test]
fn add_to_hud_adds_the_selected_consumable_row_or_else_the_game_selection() {
    let (mut world, mut plugin) = carrying();
    let p = &mut plugin;
    world.own_items(vec![common::item(31, "Blackmoor's Favor", 0x800)]);
    let consumables = world.ui_list(p, "ConsumableRows");
    assert_eq!(
        ["Asheron's Benediction", "Blackmoor's Favor"],
        consumables.as_slice()
    );

    world.act(p, "AddConsumableToCompsHud");
    assert_eq!(["Asheron's Benediction"], rows(&mut world, p).as_slice());
    assert_eq!(
        "Added Asheron's Benediction to the Comps HUD.",
        world.ui_text(p, "ProfileNotice")
    );
    world.act(p, "AddConsumableToCompsHud");
    assert_eq!(
        "Asheron's Benediction is listed already.",
        world.ui_text(p, "ProfileNotice")
    );

    // The row is the selected one, not the first: a click on a row takes
    // it off the page and leaves the next one selected.
    world.act(p, "AddAllPeas");
    world.act_int(p, "SelectConsumableRow", 1);
    assert_eq!(
        ["Asheron's Benediction", ALL_PEAS],
        world.ui_list(p, "ConsumableRows").as_slice()
    );
    assert_eq!(1, world.ui_int(p, "SelectedConsumableRowIndex"));
    world.act(p, "AddConsumableToCompsHud");
    assert!(world.ui_text(p, "ProfileNotice").contains("every pea"));
    assert_eq!(1, rows(&mut world, p).len());

    // With no rows left, the game's selection.
    for _ in 0..2 {
        world.act(p, "RemoveSelectedConsumable");
    }
    assert!(world.ui_list(p, "ConsumableRows").is_empty());
    world.set_reply::<calls::WorldObject>(&Some(owned(PEA_ID, "Gold Pea", 8, PEA_ICON)));
    world.select_object(PEA_ID);
    world.act(p, "AddConsumableToCompsHud");
    assert_eq!(
        ["Asheron's Benediction", "Gold Pea"],
        rows(&mut world, p).as_slice()
    );
    assert_eq!(vec![0, PEA_ICON], world.ui_uints(p, "CompsHudRowIcons"));
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
    assert_eq!(
        ["Gold Pea", "Platinum Scarab"],
        world.ui_list(p, "CompsHudCarriedNames").as_slice()
    );
    assert_eq!(
        ["8", "3"],
        world.ui_list(p, "CompsHudCarriedCounts").as_slice()
    );
    assert_eq!(
        vec![PEA_ICON, SCARAB_ICON],
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
    assert!(usage.contains("line") && usage.contains("column"));
    assert!(world.chat().iter().any(|line| line.contains(usage)));
    assert!(matches!(
        world.ui(p, "CompsHudKindNames"),
        UiValue::TextList(kinds) if kinds.len() == 6
    ));
    world.disable(p);
}

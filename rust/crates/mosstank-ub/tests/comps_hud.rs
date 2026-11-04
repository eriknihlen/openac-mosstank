//! The Comps HUD, as Virindi HUDs draws it (`ComponentHudControllerTests`):
//! one row per listed item with its icon and the count the character
//! carries, a plus button that adds the selection, a minus button that
//! starts a flashing removal the next row click finishes, a list kept per
//! character and an on switch kept for every character.

use std::collections::HashSet;

use ac_plugin_abi::ui::{Color, PointerEvent};
use ac_plugin_abi::world::{ObjectClass, WorldObject};
use mosstank_meta::testing::SharedStorage;
use mosstank_ub::canvas::{MapCanvasSpec, MapImage, MapPainter, MapUi};
use mosstank_ub::comps_hud::{
    self, ComponentHudController, ComponentHudEntry, HEADER_HEIGHT, HUD_WIDTH, HudRect,
    MINUS_BUTTON, PLUS_BUTTON, REMOVAL_BRIGHT, REMOVAL_CANCELLED, REMOVAL_DARK, REMOVAL_PROMPT,
    ROW_HEIGHT,
};
use mosstank_ub::testing::FakeUbHost;

const SELF_ID: u32 = 0x5000_0001;
const SCARAB_ID: u32 = 0x8000_0010;
const TAPER_ID: u32 = 0x8000_0011;
const SCARAB_ICON: u32 = 0x0600_1F5B;

fn item(id: u32, name: &str, stack: i32, owned: bool, icon: u32) -> WorldObject {
    WorldObject {
        object_id: id,
        weenie_class_id: 1,
        name: name.to_owned(),
        object_class: ObjectClass::SpellComponent,
        container_id: if owned { SELF_ID } else { 0 },
        is_owned: owned,
        stack_size: stack,
        icon_id: icon,
        ..WorldObject::default()
    }
}

#[derive(Debug, Clone)]
struct FakeCanvas {
    spec: MapCanvasSpec,
    visible: bool,
    offset: (f64, f64),
    removed: bool,
}

/// `LandscapeMapControllerTests.FakeUi`: the canvases in the order they
/// were registered, and the client art it has and was asked for.
#[derive(Debug, Default)]
struct FakeUi {
    canvases: Vec<FakeCanvas>,
    client_art: HashSet<u32>,
    requested: Vec<u32>,
}

impl FakeUi {
    fn canvas(&mut self, canvas: u32) -> &mut FakeCanvas {
        &mut self.canvases[usize::try_from(canvas).unwrap() - 1]
    }
}

impl MapUi for FakeUi {
    fn register_canvas(&mut self, spec: &MapCanvasSpec) -> Option<u32> {
        self.canvases.push(FakeCanvas {
            spec: spec.clone(),
            visible: spec.start_visible,
            offset: spec.offset,
            removed: false,
        });
        Some(u32::try_from(self.canvases.len()).unwrap())
    }

    fn set_canvas_visible(&mut self, canvas: u32, visible: bool) {
        self.canvas(canvas).visible = visible;
    }

    fn set_canvas_offset(&mut self, canvas: u32, offset: (f64, f64)) {
        self.canvas(canvas).offset = offset;
    }

    fn invalidate_canvas(&mut self, _canvas: u32) {}

    fn remove_canvas(&mut self, canvas: u32) {
        self.canvas(canvas).removed = true;
    }

    fn object_icon(&mut self, _object_id: u32) -> MapImage {
        MapImage::default()
    }

    fn client_art(&mut self, surface_id: u32) -> MapImage {
        self.requested.push(surface_id);
        if self.client_art.contains(&surface_id) {
            MapImage {
                handle: surface_id,
                width: 32,
                height: 32,
            }
        } else {
            MapImage::default()
        }
    }

    fn release_image(&mut self, _image: MapImage) {}
}

/// `LandscapeMapControllerTests.RecordingPainter`: the texts and the
/// images.
#[derive(Debug, Default)]
struct RecordingPainter {
    texts: Vec<String>,
    images: Vec<MapImage>,
}

impl MapPainter for RecordingPainter {
    fn width(&self) -> i32 {
        HUD_WIDTH
    }

    fn height(&self) -> i32 {
        0
    }

    fn clear(&mut self, _color: &Color) {}

    fn fill_rect(&mut self, _rect: (f64, f64, f64, f64), _color: &Color) {}

    fn stroke_rect(&mut self, _rect: (f64, f64, f64, f64), _color: &Color, _thickness: f32) {}

    fn draw_line(&mut self, _from: (f64, f64), _to: (f64, f64), _color: &Color, _thickness: f32) {}

    fn draw_text(&mut self, text: &str, _at: (f64, f64), _color: &Color, _outline: bool) {
        self.texts.push(text.to_owned());
    }

    #[allow(clippy::cast_precision_loss)]
    fn measure_text(&mut self, text: &str) -> (f64, f64) {
        (text.len() as f64 * 6.0, 12.0)
    }

    fn draw_image(&mut self, image: MapImage, _rect: (f64, f64, f64, f64), _tint: &Color) {
        self.images.push(image);
    }

    fn push_clip(&mut self, _rect: (f64, f64, f64, f64)) {}

    fn pop_clip(&mut self) {}
}

struct Rig {
    host: FakeUbHost,
    ui: FakeUi,
    notices: Vec<&'static str>,
    controller: ComponentHudController,
}

impl Rig {
    fn new() -> Self {
        Self::with(SharedStorage::new(), true)
    }

    /// A HUD over `storage`, turned on when `enabled` (left as the storage
    /// says otherwise), with Acdream on Coldeve in the world.
    fn with(storage: SharedStorage, enabled: bool) -> Self {
        let mut host = FakeUbHost::new();
        host.meta.storage = storage;
        {
            let me = host.meta.character_mut();
            me.id = SELF_ID;
            "Acdream".clone_into(&mut me.name);
            "Coldeve".clone_into(&mut me.world);
        }
        let controller = ComponentHudController::new(&mut host.meta.storage);
        let mut rig = Self {
            host,
            ui: FakeUi::default(),
            notices: Vec::new(),
            controller,
        };
        if enabled {
            rig.controller.set_enabled(&mut rig.host, true);
        }
        rig
    }

    fn identity(&mut self, world: &str, name: &str) {
        let me = self.host.meta.character_mut();
        world.clone_into(&mut me.world);
        name.clone_into(&mut me.name);
    }

    fn canvas(&self) -> &FakeCanvas {
        self.ui.canvases.last().unwrap()
    }

    fn ticks(&mut self, count: usize, each: f64) {
        for _ in 0..count {
            self.controller.on_tick(&mut self.host, &mut self.ui, each);
        }
    }

    /// Selects an item and presses plus, as the player would.
    fn add(&mut self, object_id: u32) {
        self.host.meta.selection.selected = Some(object_id);
        self.ticks(1, 0.015);
        self.press(PLUS_BUTTON);
        self.ticks(1, 0.015);
    }

    fn pointer(&mut self, kind: u8, x: f64, y: f64) {
        assert!(self.canvas().spec.accepts_pointer_input);
        #[allow(clippy::cast_possible_truncation)]
        let event = PointerEvent {
            canvas: u32::try_from(self.ui.canvases.len()).unwrap(),
            kind,
            x: x as f32,
            y: y as f32,
            button: 1,
            modifiers: 0,
            wheel_delta: 0,
        };
        if let Some(notice) = self
            .controller
            .on_pointer(&mut self.host, &mut self.ui, &event)
        {
            self.notices.push(notice);
        }
    }

    fn press(&mut self, button: HudRect) {
        let (x, y, width, height) = button;
        let (x, y) = (x + width / 2.0, y + height / 2.0);
        self.pointer(0, x, y);
        self.pointer(1, x, y);
    }

    fn click_row(&mut self, row: i32) {
        let y = f64::from(HEADER_HEIGHT + row * ROW_HEIGHT) + 5.0;
        self.pointer(0, 10.0, y);
        self.pointer(1, 10.0, y);
    }

    fn names(&self) -> Vec<&str> {
        self.controller
            .entries()
            .iter()
            .map(|entry| entry.name.as_str())
            .collect()
    }
}

fn scarab_entry() -> ComponentHudEntry {
    ComponentHudEntry {
        name: "Lead Scarab".to_owned(),
        icon_id: SCARAB_ICON,
    }
}

// C# source: ComponentHudControllerTests.cs :: ACountAddsTheOwnedStacksOfExactlyThatName
/// A count adds every owned stack of exactly that name, and an object with
/// no stack counts one; an item on the ground or of another name is not
/// the character's.
#[test]
fn a_count_adds_the_owned_stacks_of_exactly_that_name() {
    let objects = [
        item(1, "Prismatic Taper", 100, true, 0),
        item(2, "Prismatic Taper", 37, true, 0),
        item(3, "Prismatic Taper", 500, false, 0),
        item(4, "Prismatic Tapers", 9, true, 0),
        item(5, "prismatic taper", 9, true, 0),
    ];
    assert_eq!(137, comps_hud::count_owned(&objects, "Prismatic Taper"));
    assert_eq!(
        1,
        comps_hud::count_owned(&[item(6, "Lead Scarab", 0, true, 0)], "Lead Scarab")
    );
}

// C# source: ComponentHudControllerTests.cs :: PlusAddsTheSelectionOnceAndTheCountsFollowTheInventory
/// Plus adds the selected object by its name and icon; the same name twice
/// is one row; the counts are taken at once and every 600 ms after.
#[test]
fn plus_adds_the_selection_once_and_the_counts_follow_the_inventory() {
    let mut rig = Rig::new();
    rig.host
        .meta
        .objects
        .push(item(SCARAB_ID, "Lead Scarab", 12, true, SCARAB_ICON));
    rig.add(SCARAB_ID);
    rig.add(SCARAB_ID);

    assert_eq!([scarab_entry()], rig.controller.entries());
    assert_eq!([12], rig.controller.counts());

    rig.host
        .meta
        .objects
        .push(item(0x8000_0099, "Lead Scarab", 3, true, 0));
    rig.ticks(1, 0.3);
    assert_eq!([12], rig.controller.counts());
    rig.ticks(1, 0.31);
    assert_eq!([15], rig.controller.counts());
}

// C# source: ComponentHudControllerTests.cs :: ARowPaintsTheAddedIconAndTheCount
/// Each row paints its icon from the client art the item was added with,
/// and its count.
#[test]
fn a_row_paints_the_added_icon_and_the_count() {
    let mut rig = Rig::new();
    rig.ui.client_art.insert(SCARAB_ICON);
    rig.host
        .meta
        .objects
        .push(item(SCARAB_ID, "Lead Scarab", 12, true, SCARAB_ICON));
    rig.add(SCARAB_ID);

    let mut painter = RecordingPainter::default();
    rig.controller.paint(&mut painter, &mut rig.ui);
    assert!(rig.ui.requested.contains(&SCARAB_ICON));
    assert_eq!(1, painter.images.len());
    assert_eq!(["12"], painter.texts.as_slice());
}

// C# source: ComponentHudControllerTests.cs :: TheCanvasIsResizedToTheList
/// The canvas is as tall as the list: the title strip and a row per item,
/// or 16 pixels of empty list; a list that grew takes a new canvas at the
/// same place.
#[test]
fn the_canvas_is_resized_to_the_list() {
    let mut rig = Rig::new();
    rig.host
        .meta
        .objects
        .push(item(SCARAB_ID, "Lead Scarab", 1, true, 0));
    rig.host
        .meta
        .objects
        .push(item(TAPER_ID, "Prismatic Taper", 1, true, 0));
    rig.ticks(1, 0.015);
    assert_eq!(HEADER_HEIGHT + 16, rig.canvas().spec.height);
    assert!(rig.canvas().visible);

    rig.add(SCARAB_ID);
    rig.add(TAPER_ID);
    assert_eq!(HEADER_HEIGHT + 40, rig.canvas().spec.height);
    let (last, earlier) = rig.ui.canvases.split_last().unwrap();
    assert!(earlier.iter().all(|canvas| canvas.removed));
    assert!(!last.removed);
}

// C# source: ComponentHudControllerTests.cs :: MinusThenARowClickRemovesThatRow
/// Minus starts a removal and says how to finish it; the counts flash
/// between bright and dark red; the next row clicked comes off and the
/// removal ends.
#[test]
fn minus_then_a_row_click_removes_that_row() {
    let mut rig = Rig::new();
    rig.host
        .meta
        .objects
        .push(item(SCARAB_ID, "Lead Scarab", 1, true, 0));
    rig.host
        .meta
        .objects
        .push(item(TAPER_ID, "Prismatic Taper", 1, true, 0));
    rig.add(SCARAB_ID);
    rig.add(TAPER_ID);

    rig.press(MINUS_BUTTON);
    assert!(rig.controller.removing());
    assert_eq!([REMOVAL_PROMPT], rig.notices.as_slice());
    rig.ticks(1, 0.6);
    let first = rig.controller.count_color();
    rig.ticks(1, 0.6);
    assert_ne!(first, rig.controller.count_color());
    assert!([REMOVAL_BRIGHT, REMOVAL_DARK].contains(&rig.controller.count_color()));

    rig.click_row(0);
    assert!(!rig.controller.removing());
    assert_eq!(["Prismatic Taper"], rig.names().as_slice());
    assert_eq!(comps_hud::TEXT, rig.controller.count_color());

    rig.ticks(1, 0.015);
    rig.click_row(0);
    assert_eq!(1, rig.controller.entries().len());
}

// C# source: ComponentHudControllerTests.cs :: ARemovalIsCancelledByMinusOrLapsesAfterThirtySeconds
/// Minus twice cancels and says so; a removal left alone lapses after 30
/// seconds.
#[test]
fn a_removal_is_cancelled_by_minus_or_lapses_after_thirty_seconds() {
    let mut rig = Rig::new();
    rig.host
        .meta
        .objects
        .push(item(SCARAB_ID, "Lead Scarab", 1, true, 0));
    rig.add(SCARAB_ID);

    rig.press(MINUS_BUTTON);
    rig.press(MINUS_BUTTON);
    assert!(!rig.controller.removing());
    assert_eq!(Some(&REMOVAL_CANCELLED), rig.notices.last());

    rig.press(MINUS_BUTTON);
    rig.ticks(49, 0.6);
    assert!(rig.controller.removing());
    rig.ticks(3, 0.6);
    assert!(!rig.controller.removing());
    assert_eq!(1, rig.controller.entries().len());
}

// C# source: ComponentHudControllerTests.cs :: ADragMovesTheHudAndKeepsWhereItStands
/// A row clicked with no removal under way stays; the press is a drag that
/// moves the HUD by how far the pointer went, kept with the list.
#[test]
fn a_drag_moves_the_hud_and_keeps_where_it_stands() {
    let storage = SharedStorage::new();
    let mut rig = Rig::with(storage.clone(), true);
    rig.host
        .meta
        .objects
        .push(item(SCARAB_ID, "Lead Scarab", 1, true, 0));
    rig.add(SCARAB_ID);

    rig.pointer(0, 10.0, 25.0);
    rig.pointer(2, 25.0, 15.0);
    rig.pointer(1, 40.0, 5.0);
    assert_eq!(1, rig.controller.entries().len());
    let moved = (
        f64::from(comps_hud::DEFAULT_X + 30),
        f64::from(comps_hud::DEFAULT_Y - 20),
    );
    assert_eq!(moved, rig.canvas().offset);

    let mut again = Rig::with(storage, false);
    again.ticks(1, 0.015);
    assert_eq!(
        (comps_hud::DEFAULT_X + 30, comps_hud::DEFAULT_Y - 20),
        again.controller.position()
    );
}

// C# source: ComponentHudControllerTests.cs :: TheListIsKeptPerCharacterAndTheSwitchForAll
/// The list is the character's: another character on the same storage
/// starts empty, and the first one's list comes back at its next login.
/// The on switch is shared, as the reference keeps it.
#[test]
fn the_list_is_kept_per_character_and_the_switch_for_all() {
    let storage = SharedStorage::new();
    let mut rig = Rig::with(storage.clone(), true);
    rig.host
        .meta
        .objects
        .push(item(SCARAB_ID, "Lead Scarab", 1, true, SCARAB_ICON));
    rig.add(SCARAB_ID);

    rig.controller.reset(&mut rig.ui);
    rig.identity("Coldeve", "Sigrid");
    rig.ticks(1, 0.015);
    assert!(rig.controller.entries().is_empty());

    let mut returning = Rig::with(storage, false);
    assert!(returning.controller.enabled());
    returning.ticks(1, 0.015);
    assert_eq!([scarab_entry()], returning.controller.entries());
}

// C# source: ComponentHudControllerTests.cs :: NothingShowsWhileOffOrOutOfTheWorld
/// Off, or before a character is in the world, there is nothing to show.
#[test]
fn nothing_shows_while_off_or_out_of_the_world() {
    let mut rig = Rig::with(SharedStorage::new(), false);
    let me = rig.host.meta.character.take();
    rig.ticks(1, 0.015);
    assert!(rig.ui.canvases.is_empty());

    rig.controller.set_enabled(&mut rig.host, true);
    rig.ticks(1, 0.015);
    assert!(rig.ui.canvases.is_empty());

    rig.host.meta.character = me.clone();
    rig.ticks(1, 0.015);
    assert!(rig.canvas().visible);
    rig.ticks(1, 0.015);
    rig.controller.set_enabled(&mut rig.host, false);
    rig.ticks(1, 0.015);
    assert!(!rig.canvas().visible);

    rig.controller.set_enabled(&mut rig.host, true);
    rig.ticks(1, 0.015);
    rig.controller.reset(&mut rig.ui);
    rig.host.meta.character = None;
    rig.ticks(1, 0.015);
    assert!(!rig.canvas().visible);
}

// C# source: ComponentHudControllerTests.cs :: PlusWithoutAKnownSelectionAddsNothing
/// The plus button with nothing selected, or a selection the client does
/// not know, adds nothing.
#[test]
fn plus_without_a_known_selection_adds_nothing() {
    let mut rig = Rig::new();
    rig.ticks(1, 0.015);
    rig.press(PLUS_BUTTON);
    rig.host.meta.selection.selected = Some(0x8000_FFFF);
    rig.press(PLUS_BUTTON);
    assert!(rig.controller.entries().is_empty());
}

// C# source: ComponentHudControllerTests.cs :: RemovingByNameIgnoresCase
/// Removing by name ignores case and reports the listed spelling.
#[test]
fn removing_by_name_ignores_case() {
    let mut rig = Rig::new();
    rig.host
        .meta
        .objects
        .push(item(SCARAB_ID, "Lead Scarab", 1, true, 0));
    rig.add(SCARAB_ID);
    assert_eq!(
        Some("Lead Scarab".to_owned()),
        rig.controller.remove(&mut rig.host, " lead scarab ")
    );
    assert!(rig.controller.entries().is_empty());
    assert_eq!(None, rig.controller.remove(&mut rig.host, "Lead Scarab"));
}

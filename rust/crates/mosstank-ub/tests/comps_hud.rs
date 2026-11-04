//! The Comps HUD (`ComponentHudControllerTests`): a strip with one item
//! per listed name, its icon and the count the character carries, as a
//! column or a line, painted in the plugin theme; three buttons (the layout
//! switch, Edit items, hide); a drag that moves it; the Edit items
//! window's listing and editing; a list kept per character with its order,
//! warn levels, layout and place, and an on switch kept for every
//! character.

use std::collections::HashSet;

use ac_plugin_abi::ui::{Color, PointerEvent, ThemeInfo, ThemeKind, ThemePalette, theme_metrics};
use ac_plugin_abi::world::{ObjectClass, WorldObject};
use mosstank_meta::testing::SharedStorage;
use mosstank_stores::storage::PluginStorage;
use mosstank_ub::canvas::{MapCanvasSpec, MapFont, MapImage, MapPainter, MapUi};
use mosstank_ub::comps_hud::layout::{
    self, COLUMN_BUTTON, COLUMN_MIN_WIDTH, ICON_GAP, ICON_SIZE, ITEM_GAP, LINE_BUTTON, LINE_HEIGHT,
    ROW_HEIGHT, TITLE_HEIGHT,
};
use mosstank_ub::comps_hud::style::{FONT_SIZE, WARNING_ON_DARK, WARNING_ON_LIGHT};
use mosstank_ub::comps_hud::{
    self, AddRefusal, CarriedKind, ComponentHudController, ComponentHudEntry, HudButton,
    HudGeometry, HudLayout, HudRect, HudStyle,
};
use mosstank_ub::testing::FakeUbHost;

const SELF_ID: u32 = 0x5000_0001;
const SCARAB_ID: u32 = 0x8000_0010;
const TAPER_ID: u32 = 0x8000_0011;
const PEA_ID: u32 = 0x8000_0012;
const KIT_ID: u32 = 0x8000_0013;
const SCARAB_ICON: u32 = 0x0600_1F5B;
const TAPER_ICON: u32 = 0x0600_1F60;
const FONT_HANDLE: u32 = 77;

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

fn of_class(object: WorldObject, class: ObjectClass) -> WorldObject {
    WorldObject {
        object_class: class,
        ..object
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b, a: 255 }
}

/// The light theme's palette, as the client hands it over.
fn parchment() -> ThemeInfo {
    ThemeInfo {
        kind: ThemeKind::Parchment,
        display_name: "Parchment".to_owned(),
        palette: Some(ThemePalette {
            background: rgb(0xED, 0xE7, 0xDC),
            field: rgb(0xFA, 0xF7, 0xF1),
            border: rgb(0xC9, 0xBF, 0xAE),
            text: rgb(0x2A, 0x25, 0x20),
            muted: rgb(0x65, 0x5C, 0x50),
            accent: rgb(0xB4, 0x47, 0x1F),
            selected: rgb(0xDD, 0xCF, 0xB6),
        }),
    }
}

/// A dark theme's palette.
fn moss() -> ThemeInfo {
    ThemeInfo {
        kind: ThemeKind::Moss,
        display_name: "Charcoal + moss".to_owned(),
        palette: Some(ThemePalette {
            background: rgb(0x17, 0x1D, 0x1B),
            field: rgb(0x10, 0x16, 0x13),
            border: rgb(0x35, 0x44, 0x3B),
            text: rgb(0xE0, 0xE8, 0xE2),
            muted: rgb(0x9A, 0xA9, 0x9E),
            accent: rgb(0x97, 0xBE, 0x81),
            selected: rgb(0x34, 0x4B, 0x37),
        }),
    }
}

#[derive(Debug, Clone)]
struct FakeCanvas {
    spec: MapCanvasSpec,
    visible: bool,
    /// Where the host has the canvas: what pointer events are measured
    /// from.
    offset: (f64, f64),
    /// A move asked for and not handed to the host yet.
    pending: Option<(f64, f64)>,
    removed: bool,
}

/// `LandscapeMapControllerTests.FakeUi`: the canvases in the order they
/// were registered, the client art it has and was asked for, the repaints
/// asked for and the fonts handed out.
#[derive(Debug, Default)]
struct FakeUi {
    canvases: Vec<FakeCanvas>,
    client_art: HashSet<u32>,
    requested: Vec<u32>,
    invalidated: usize,
    /// The objects the client has a composed icon for.
    object_art: HashSet<u32>,
    /// The objects whose composed icon was asked for.
    object_requests: Vec<u32>,
    released: Vec<MapImage>,
    fonts_asked: Vec<f32>,
    /// Every text measured.
    measured: Vec<String>,
    fonts_released: Vec<u32>,
}

impl FakeUi {
    /// Hands the moves asked for to the host.
    fn flush(&mut self) {
        for canvas in &mut self.canvases {
            if let Some(offset) = canvas.pending.take() {
                canvas.offset = offset;
            }
        }
    }

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
            pending: None,
            removed: false,
        });
        Some(u32::try_from(self.canvases.len()).unwrap())
    }

    fn set_canvas_visible(&mut self, canvas: u32, visible: bool) {
        self.canvas(canvas).visible = visible;
    }

    fn set_canvas_offset(&mut self, canvas: u32, offset: (f64, f64)) {
        self.canvas(canvas).pending = Some(offset);
    }

    fn invalidate_canvas(&mut self, _canvas: u32) {
        self.invalidated += 1;
    }

    fn remove_canvas(&mut self, canvas: u32) {
        self.canvas(canvas).removed = true;
    }

    fn object_icon(&mut self, object_id: u32) -> MapImage {
        self.object_requests.push(object_id);
        if self.object_art.contains(&object_id) {
            MapImage {
                handle: composed(object_id),
                width: 32,
                height: 32,
            }
        } else {
            MapImage::default()
        }
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

    fn release_image(&mut self, image: MapImage) {
        self.released.push(image);
    }

    fn bundled_font(&mut self, pixel_size: f32) -> MapFont {
        self.fonts_asked.push(pixel_size);
        MapFont {
            handle: FONT_HANDLE,
            pixel_size,
            line_height: 14.0,
            ascent: 11.0,
        }
    }

    fn release_font(&mut self, font: MapFont) {
        self.fonts_released.push(font.handle);
    }

    #[allow(clippy::cast_precision_loss)]
    fn measure_text_in(&mut self, _font: MapFont, text: &str) -> Option<(f64, f64)> {
        self.measured.push(text.to_owned());
        Some((text.chars().count() as f64 * 6.0, 14.0))
    }
}

/// A text drawn: what, where, in what colour and font.
#[derive(Debug, Clone, PartialEq)]
struct Drawn {
    text: String,
    at: (f64, f64),
    color: Color,
    font: u32,
}

/// The paint as drawn: the texts, the images and the fills.
#[derive(Debug, Default)]
struct RecordingPainter {
    texts: Vec<Drawn>,
    images: Vec<(MapImage, HudRect)>,
    fills: Vec<(HudRect, Color)>,
}

impl RecordingPainter {
    fn count_colors(&self) -> Vec<Color> {
        self.texts.iter().map(|text| text.color.clone()).collect()
    }
}

impl MapPainter for RecordingPainter {
    fn width(&self) -> i32 {
        COLUMN_MIN_WIDTH
    }

    fn height(&self) -> i32 {
        0
    }

    fn clear(&mut self, _color: &Color) {}

    fn fill_rect(&mut self, rect: (f64, f64, f64, f64), color: &Color) {
        self.fills.push((rect, color.clone()));
    }

    fn stroke_rect(&mut self, _rect: (f64, f64, f64, f64), _color: &Color, _thickness: f32) {}

    fn draw_line(&mut self, _from: (f64, f64), _to: (f64, f64), _color: &Color, _thickness: f32) {}

    fn draw_text(&mut self, text: &str, at: (f64, f64), color: &Color, _outline: bool) {
        self.draw_text_in(MapFont::default(), text, at, color, false);
    }

    #[allow(clippy::cast_precision_loss)]
    fn measure_text(&mut self, text: &str) -> (f64, f64) {
        (text.len() as f64 * 6.0, 12.0)
    }

    fn draw_text_in(
        &mut self,
        font: MapFont,
        text: &str,
        at: (f64, f64),
        color: &Color,
        _outline: bool,
    ) {
        self.texts.push(Drawn {
            text: text.to_owned(),
            at,
            color: color.clone(),
            font: font.handle,
        });
    }

    #[allow(clippy::cast_precision_loss)]
    fn measure_text_in(&mut self, _font: MapFont, text: &str) -> (f64, f64) {
        (text.len() as f64 * 6.0, 14.0)
    }

    fn draw_image(&mut self, image: MapImage, rect: (f64, f64, f64, f64), _tint: &Color) {
        self.images.push((image, rect));
    }

    fn push_clip(&mut self, _rect: (f64, f64, f64, f64)) {}

    fn pop_clip(&mut self) {}
}

struct Rig {
    host: FakeUbHost,
    ui: FakeUi,
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

    fn own(&mut self, object: WorldObject) {
        self.host.meta.objects.push(object);
    }

    /// The strip's canvas: the last one registered under its id.
    fn canvas(&self) -> &FakeCanvas {
        &self.ui.canvases[self.canvas_index(comps_hud::CANVAS_ID)]
    }

    /// The tooltip's canvas.
    fn tip_canvas(&self) -> &FakeCanvas {
        &self.ui.canvases[self.canvas_index(comps_hud::tip::TIP_CANVAS_ID)]
    }

    fn canvas_index(&self, id: &str) -> usize {
        self.ui
            .canvases
            .iter()
            .rposition(|canvas| canvas.spec.canvas_id == id)
            .unwrap_or_else(|| panic!("no canvas {id}"))
    }

    /// Frames as the plugin runs them: the count characters measured, the
    /// tick, and what it asked of the canvases handed to the host.
    fn ticks(&mut self, count: usize, each: f64) {
        for _ in 0..count {
            self.controller.prepare_metrics(&mut self.ui);
            self.controller.on_tick(&mut self.host, &mut self.ui, each);
            self.ui.flush();
        }
    }

    /// Selects an item and adds it, as `/vt compshud add` does.
    fn add(&mut self, object_id: u32) {
        self.host.meta.selection.selected = Some(object_id);
        self.ticks(1, 0.015);
        self.controller.add_selection(&mut self.host).unwrap();
        self.ticks(1, 0.015);
    }

    fn pointer(&mut self, kind: u8, x: f64, y: f64) {
        assert!(self.canvas().spec.accepts_pointer_input);
        #[allow(clippy::cast_possible_truncation)]
        let event = PointerEvent {
            canvas: u32::try_from(self.canvas_index(comps_hud::CANVAS_ID) + 1).unwrap(),
            kind,
            x: x as f32,
            y: y as f32,
            button: 1,
            modifiers: 0,
            wheel_delta: 0,
        };
        self.controller
            .on_pointer(&mut self.host, &mut self.ui, &event);
        // The plugin hands a pointer event's canvas moves over at once.
        self.ui.flush();
    }

    /// A click on one of the strip's buttons, where it is drawn now.
    fn press(&mut self, button: HudButton) {
        let (x, y, width, height) = self.controller.geometry().button(button);
        let (x, y) = (x + width / 2.0, y + height / 2.0);
        self.pointer(0, x, y);
        self.pointer(1, x, y);
    }

    fn paint(&mut self) -> RecordingPainter {
        let mut painter = RecordingPainter::default();
        self.controller.paint(&mut painter, &mut self.ui);
        painter
    }

    fn names(&self) -> Vec<&str> {
        self.controller
            .entries()
            .iter()
            .map(|entry| entry.name.as_str())
            .collect()
    }

    fn stored(&mut self) -> String {
        let key = comps_hud::key_for("Coldeve", "Acdream");
        self.host
            .meta
            .storage
            .read_text(&key)
            .unwrap()
            .unwrap_or_default()
    }
}

fn scarab_entry() -> ComponentHudEntry {
    ComponentHudEntry {
        name: "Lead Scarab".to_owned(),
        icon_id: SCARAB_ICON,
        warn: 0,
    }
}

/// The widths counts reading `counts` are laid out with before they are
/// measured.
fn widths(counts: &[&str]) -> Vec<f64> {
    counts
        .iter()
        .map(|count| layout::estimated_width(count))
        .collect()
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
/// Adding the selected object takes its name and icon; the same name twice
/// is refused; the counts are taken at once and every 600 ms after.
#[test]
fn adding_the_selection_lists_it_once_and_the_counts_follow_the_inventory() {
    let mut rig = Rig::new();
    rig.own(item(SCARAB_ID, "Lead Scarab", 12, true, SCARAB_ICON));
    rig.add(SCARAB_ID);
    assert_eq!(
        Err(AddRefusal::Listed("Lead Scarab".to_owned())),
        rig.controller.add_selection(&mut rig.host)
    );

    assert_eq!([scarab_entry()], rig.controller.entries());
    assert_eq!([12], rig.controller.counts());

    rig.own(item(0x8000_0099, "Lead Scarab", 3, true, 0));
    rig.ticks(1, 0.3);
    assert_eq!([12], rig.controller.counts());
    rig.ticks(1, 0.31);
    assert_eq!([15], rig.controller.counts());
}

// C# source: ComponentHudControllerTests.cs :: ARowPaintsTheAddedIconAndTheCount
/// Each item paints its icon from the client art it was added with, and
/// its count.
#[test]
fn an_item_paints_the_added_icon_and_the_count() {
    let mut rig = Rig::new();
    rig.ui.client_art.insert(SCARAB_ICON);
    rig.own(item(SCARAB_ID, "Lead Scarab", 12, true, SCARAB_ICON));
    rig.add(SCARAB_ID);

    let painter = rig.paint();
    assert!(rig.ui.requested.contains(&SCARAB_ICON));
    assert_eq!(1, painter.images.len());
    let texts: Vec<&str> = painter
        .texts
        .iter()
        .map(|text| text.text.as_str())
        .collect();
    assert_eq!(["12"], texts.as_slice());
}
/// The column: a 12-pixel title line holding the layout switch, the pencil
/// and the cross left to right at its right end, then a 16-pixel row per
/// item, the 14-pixel icon at the left and the count just after it; as
/// wide as its widest count needs, never narrower than its buttons; an
/// empty list keeps one row.
#[test]
fn the_column_is_a_thin_title_line_and_a_row_per_item() {
    let column = HudGeometry::new(HudLayout::Column, &[12.0, 24.0]);
    assert_eq!((12, 16, 14), (TITLE_HEIGHT, ROW_HEIGHT, ICON_SIZE));
    assert_eq!(TITLE_HEIGHT + 2 * ROW_HEIGHT + 2, column.height);

    let buttons: Vec<HudButton> = column.buttons.iter().map(|(button, _)| *button).collect();
    assert_eq!(
        [HudButton::Layout, HudButton::Edit, HudButton::Hide],
        buttons.as_slice()
    );
    let mut right = 0.0;
    for (_, (x, y, width, height)) in column.buttons {
        assert!(x >= right, "the buttons stand left to right");
        assert_eq!(
            (f64::from(COLUMN_BUTTON), f64::from(COLUMN_BUTTON)),
            (width, height)
        );
        assert!(y >= 0.0 && y + height <= f64::from(TITLE_HEIGHT));
        right = x + width;
    }
    let width = f64::from(column.width);
    assert!(right <= width && right >= width - 4.0);

    for (row, slot) in column.items.iter().enumerate() {
        let top =
            f64::from(TITLE_HEIGHT) + f64::from(ROW_HEIGHT) * f64::from(u8::try_from(row).unwrap());
        assert_eq!(top, slot.cell.1);
        assert_eq!(f64::from(ROW_HEIGHT), slot.cell.3);
        let (icon_x, icon_y, icon_w, icon_h) = slot.icon;
        assert_eq!(
            (f64::from(ICON_SIZE), f64::from(ICON_SIZE)),
            (icon_w, icon_h)
        );
        assert!(icon_x < 5.0 && icon_y >= top && icon_y + icon_h <= top + f64::from(ROW_HEIGHT));
        let (count_x, _, count_w, _) = slot.count;
        assert_eq!(
            icon_x + icon_w + f64::from(ICON_GAP),
            count_x,
            "just after the icon"
        );
        assert_eq!(24.0, count_w, "as wide as the widest count");
    }
    assert!(
        width <= column.items[0].count.0 + 24.0 + 4.0,
        "no room past the count"
    );

    let empty = HudGeometry::new(HudLayout::Column, &[]);
    assert_eq!(
        (COLUMN_MIN_WIDTH, TITLE_HEIGHT + ROW_HEIGHT + 2),
        empty.size()
    );
    assert_eq!(1, empty.items.len());
    assert_eq!(Some(HudButton::Hide), column.button_at((width - 4.0, 6.0)));
    assert_eq!(None, column.button_at((4.0, 6.0)));
    assert_eq!(Some(1), column.item_at((10.0, 30.0)));
}

/// The line: one 20-pixel row of icon, count, icon, count, each count's
/// room as wide as its digits, then a thin divider and the three buttons;
/// it grows with the items.
#[test]
fn the_line_is_icon_count_pairs_then_a_divider_and_the_buttons() {
    let line = HudGeometry::new(HudLayout::Line, &widths(&["12", "300", "5"]));
    assert_eq!(LINE_HEIGHT, line.height);
    assert_eq!(3, line.items.len());
    let mut x = 0.0;
    for slot in &line.items {
        let (icon_x, icon_y, icon_w, icon_h) = slot.icon;
        assert!(icon_x >= x, "the items stand left to right");
        assert_eq!(
            (f64::from(ICON_SIZE), f64::from(ICON_SIZE)),
            (icon_w, icon_h)
        );
        assert!(icon_y >= 0.0 && icon_y + icon_h <= f64::from(LINE_HEIGHT));
        assert!(
            slot.count.0 >= icon_x + icon_w,
            "the count follows its icon"
        );
        x = slot.count.0 + slot.count.2;
    }
    assert!(line.items[1].count.2 > line.items[0].count.2);
    let divider = line.divider.0;
    assert!(divider > x);
    for (_, (button_x, y, width, height)) in line.buttons {
        assert!(button_x > divider);
        assert_eq!(
            (f64::from(LINE_BUTTON), f64::from(LINE_BUTTON)),
            (width, height)
        );
        assert!(y >= 0.0 && y + height <= f64::from(LINE_HEIGHT));
    }
    let (last_x, _, last_w, _) = line.button(HudButton::Hide);
    assert!(last_x + last_w <= f64::from(line.width));

    let longer = HudGeometry::new(HudLayout::Line, &widths(&["12", "300", "5", "40"]));
    assert!(longer.width > line.width);
    assert_eq!(LINE_HEIGHT, longer.height);
}

// C# source: ComponentHudControllerTests.cs :: TheCanvasIsResizedToTheList
/// The canvas is the strip's size: a list that grew takes a new canvas at
/// the same place, and switching to the line takes one of the line's size.
#[test]
fn the_canvas_follows_the_strips_size() {
    let mut rig = Rig::new();
    rig.own(item(SCARAB_ID, "Lead Scarab", 1, true, 0));
    rig.own(item(TAPER_ID, "Prismatic Taper", 1, true, 0));
    rig.ticks(1, 0.015);
    assert_eq!(TITLE_HEIGHT + ROW_HEIGHT + 2, rig.canvas().spec.height);
    assert!(rig.canvas().visible);

    rig.add(SCARAB_ID);
    rig.add(TAPER_ID);
    assert_eq!(TITLE_HEIGHT + 2 * ROW_HEIGHT + 2, rig.canvas().spec.height);
    assert_eq!(
        HudGeometry::new(HudLayout::Column, &[6.0, 6.0]).width,
        rig.canvas().spec.width
    );
    let strips: Vec<&FakeCanvas> = rig
        .ui
        .canvases
        .iter()
        .filter(|canvas| canvas.spec.canvas_id == comps_hud::CANVAS_ID)
        .collect();
    let (last, earlier) = strips.split_last().unwrap();
    assert!(earlier.iter().all(|canvas| canvas.removed));
    assert!(!last.removed);

    rig.press(HudButton::Layout);
    rig.ticks(1, 0.015);
    let line = HudGeometry::new(HudLayout::Line, &[6.0, 6.0]);
    assert_eq!(
        line.size(),
        (rig.canvas().spec.width, rig.canvas().spec.height)
    );
    assert_eq!(
        (
            f64::from(comps_hud::DEFAULT_X),
            f64::from(comps_hud::DEFAULT_Y)
        ),
        rig.canvas().offset
    );
}

/// The layout button switches between the column and the line, and the
/// character's next login comes back to the one it left.
#[test]
fn the_layout_switch_is_kept_for_the_character() {
    let storage = SharedStorage::new();
    let mut rig = Rig::with(storage.clone(), true);
    rig.ticks(1, 0.015);
    assert_eq!(HudLayout::Column, rig.controller.layout());
    rig.press(HudButton::Layout);
    assert_eq!(HudLayout::Line, rig.controller.layout());

    let mut again = Rig::with(storage.clone(), false);
    again.ticks(1, 0.015);
    assert_eq!(HudLayout::Line, again.controller.layout());

    again.press_without_canvas_switches_back();
    let mut third = Rig::with(storage, false);
    third.ticks(1, 0.015);
    assert_eq!(HudLayout::Column, third.controller.layout());
}

impl Rig {
    /// The layout switched back through the controller itself (the HUD is
    /// off, so it has no canvas to click).
    fn press_without_canvas_switches_back(&mut self) {
        self.controller
            .press_button(&mut self.host, HudButton::Layout);
        assert_eq!(HudLayout::Column, self.controller.layout());
    }
}

/// The cross hides the HUD: it is off (Show HUD unticks) for every
/// character, and its canvas goes.
#[test]
fn the_cross_hides_the_hud_and_turns_its_switch_off() {
    let storage = SharedStorage::new();
    let mut rig = Rig::with(storage.clone(), true);
    rig.ticks(1, 0.015);
    assert!(rig.canvas().visible);

    rig.press(HudButton::Hide);
    assert!(!rig.controller.enabled());
    rig.ticks(1, 0.015);
    assert!(!rig.canvas().visible);
    assert!(!ComponentHudController::new(&mut SharedStorage::clone(&storage)).enabled());
}

/// The pencil opens the Edit items window, with what the character carries
/// listed at once.
#[test]
fn the_pencil_opens_edit_items() {
    let mut rig = Rig::new();
    rig.own(item(SCARAB_ID, "Lead Scarab", 4, true, SCARAB_ICON));
    rig.ticks(1, 0.015);
    assert!(!rig.controller.editor_visible());
    rig.press(HudButton::Edit);
    assert!(rig.controller.editor_visible());
    let names: Vec<&str> = rig
        .controller
        .carried_rows()
        .iter()
        .map(|item| item.name.as_str())
        .collect();
    assert_eq!(["Lead Scarab"], names.as_slice());
}

/// A button acts only when the press is let go over it: one dragged off
/// does nothing, and a press beside the buttons is a drag.
#[test]
fn a_button_acts_only_when_let_go_over_it() {
    let mut rig = Rig::new();
    rig.ticks(1, 0.015);
    let (x, y, width, height) = rig.controller.geometry().button(HudButton::Hide);
    rig.pointer(0, x + width / 2.0, y + height / 2.0);
    rig.pointer(1, x + width / 2.0, y + height + 20.0);
    assert!(rig.controller.enabled());
    assert_eq!(
        (comps_hud::DEFAULT_X, comps_hud::DEFAULT_Y),
        rig.controller.position()
    );
}

impl Rig {
    /// A left press, moves and release at points of the screen, each
    /// reported as the host reports it: in canvas pixels from where the
    /// plugin last put the canvas (which may have moved since the press).
    fn drag_on_screen(&mut self, points: &[(f64, f64)]) {
        let last = points.len() - 1;
        for (index, point) in points.iter().enumerate() {
            let kind = match index {
                0 => PointerEvent::DOWN,
                i if i == last => PointerEvent::UP,
                _ => PointerEvent::MOVE,
            };
            let (x, y) = self.canvas().offset;
            self.event(kind, (point.0 - x, point.1 - y), 1);
        }
    }
}

// C# source: ComponentHudControllerTests.cs :: ADragMovesTheHudAndKeepsWhereItStands
/// A press away from the buttons is a drag that moves the HUD by how far
/// the pointer went, kept with the list.
#[test]
fn a_drag_moves_the_hud_and_keeps_where_it_stands() {
    let storage = SharedStorage::new();
    let mut rig = Rig::with(storage.clone(), true);
    rig.own(item(SCARAB_ID, "Lead Scarab", 1, true, 0));
    rig.add(SCARAB_ID);

    let (x, y) = (
        f64::from(comps_hud::DEFAULT_X),
        f64::from(comps_hud::DEFAULT_Y),
    );
    rig.drag_on_screen(&[
        (x + 10.0, y + 25.0),
        (x + 25.0, y + 15.0),
        (x + 40.0, y + 5.0),
    ]);
    assert_eq!(1, rig.controller.entries().len());
    assert_eq!((x + 30.0, y - 20.0), rig.canvas().offset);

    let mut again = Rig::with(storage, false);
    again.ticks(1, 0.015);
    assert_eq!(
        (comps_hud::DEFAULT_X + 30, comps_hud::DEFAULT_Y - 20),
        again.controller.position()
    );
}

/// While the button is held the strip follows the pointer, the grabbed
/// point staying under it (no jump at the start), stopping at the screen's
/// top and left edges, with no tooltip; nothing is saved until the button
/// comes up, and then the last place is saved once.
#[test]
fn the_strip_follows_a_drag_and_is_saved_once_when_let_go() {
    let storage = SharedStorage::new();
    let key = comps_hud::key_for("Coldeve", "Acdream");
    let mut rig = Rig::with(storage.clone(), true);
    rig.own(item(SCARAB_ID, "Lead Scarab", 1, true, 0));
    rig.add(SCARAB_ID);
    let saved = storage.get(&key).unwrap();
    let column = rig.controller.geometry().clone();
    rig.hover(centre(column.items[0].cell));
    assert!(rig.controller.tip().is_some());

    let (x, y) = (
        f64::from(comps_hud::DEFAULT_X),
        f64::from(comps_hud::DEFAULT_Y),
    );
    let grab = (12.0, 20.0);
    rig.event(PointerEvent::DOWN, grab, 1);
    assert_eq!((x, y), rig.canvas().offset, "no jump at the press");
    assert!(rig.controller.tip().is_none());
    for (screen_x, screen_y) in [
        (x + 40.0, y + 30.0),
        (x + 90.0, y + 75.0),
        (x - 30.0, y + 10.0),
    ] {
        let (cx, cy) = rig.canvas().offset;
        rig.event(PointerEvent::MOVE, (screen_x - cx, screen_y - cy), 1);
        let (nx, ny) = rig.canvas().offset;
        assert_eq!(
            (screen_x - nx, screen_y - ny),
            grab,
            "the grabbed point stays under the pointer"
        );
        assert!(rig.controller.tip().is_none(), "no tooltip in a drag");
        assert_eq!(Some(saved.clone()), storage.get(&key), "nothing saved yet");
    }
    // Past the left edge it stops at 0, and the grab is kept: it moves
    // again only once the pointer is back over the grabbed point.
    let (cx, cy) = rig.canvas().offset;
    rig.event(PointerEvent::MOVE, (-400.0 - cx, y - cy), 1);
    assert_eq!(0.0, rig.canvas().offset.0);
    let (cx, cy) = rig.canvas().offset;
    rig.event(PointerEvent::MOVE, (5.0 - cx, y + 20.0 - cy), 1);
    assert_eq!(0.0, rig.canvas().offset.0);
    let (cx, cy) = rig.canvas().offset;
    rig.event(PointerEvent::UP, (60.0 - cx, y + 20.0 - cy), 1);
    assert_eq!((48.0, y), rig.canvas().offset);
    assert_eq!((48, comps_hud::DEFAULT_Y), rig.controller.position());
    let after = storage.get(&key).unwrap();
    assert_ne!(saved, after);
    assert!(after.contains("\"X\": 48"), "{after}");

    // A press let go where it began saves nothing.
    storage.insert(&key, "kept");
    rig.event(PointerEvent::DOWN, grab, 1);
    rig.event(PointerEvent::UP, grab, 1);
    assert_eq!(Some("kept".to_owned()), storage.get(&key));
}

/// Counts of one, four and five digits fit in both layouts, each count just
/// after its icon in the room it measures; the line reads as pairs (more
/// room between one item and the next than between an icon and its count).
#[test]
fn one_four_and_five_digit_counts_fit_in_both_layouts() {
    // The test painter's measure: six pixels a character.
    let widths = [6.0, 24.0, 30.0];
    let column = HudGeometry::new(HudLayout::Column, &widths);
    let count_x = column.items[0].count.0;
    assert_eq!(
        column.items[0].icon.0 + f64::from(ICON_SIZE + ICON_GAP),
        count_x
    );
    assert!(column.items.iter().all(|slot| slot.count.2 >= 30.0));
    assert!(f64::from(column.width) >= count_x + 30.0);
    assert!(f64::from(column.width) <= count_x + 30.0 + 4.0);
    assert_eq!(
        COLUMN_MIN_WIDTH,
        HudGeometry::new(HudLayout::Column, &[6.0]).width,
        "a one-digit column is as narrow as its buttons"
    );
    let four = HudGeometry::new(HudLayout::Column, &[24.0]);
    assert!(four.items[0].count.2 >= 24.0);
    assert!(f64::from(four.width) >= four.items[0].count.0 + 24.0);

    let line = HudGeometry::new(HudLayout::Line, &widths);
    for (slot, width) in line.items.iter().zip(widths) {
        assert_eq!(slot.icon.0 + f64::from(ICON_SIZE + ICON_GAP), slot.count.0);
        assert_eq!(width, slot.count.2);
    }
    for pair in line.items.windows(2) {
        let between = pair[1].icon.0 - (pair[0].count.0 + pair[0].count.2);
        assert_eq!(f64::from(ITEM_GAP), between);
        let inside = pair[0].count.0 - (pair[0].icon.0 + pair[0].icon.2);
        assert!(between > inside, "the line reads as pairs");
    }
}

/// A count that gains or loses a digit resizes the strip to fit it.
#[test]
fn the_strip_resizes_when_a_count_gains_or_loses_a_digit() {
    let mut rig = Rig::new();
    rig.own(item(TAPER_ID, "Prismatic Taper", 999, true, 0));
    rig.add(TAPER_ID);
    let settle = |rig: &mut Rig| {
        rig.ticks(1, 0.6);
        rig.paint();
        rig.ticks(1, 0.015);
        rig.canvas().spec.width
    };
    let three = settle(&mut rig);
    rig.own(item(0x8000_0050, "Prismatic Taper", 4001, true, 0));
    let four = settle(&mut rig);
    assert!(four > three, "{three} -> {four}");
    rig.own(item(0x8000_0051, "Prismatic Taper", 7345, true, 0));
    let five = settle(&mut rig);
    assert!(five > four, "{four} -> {five}");
    let painted = rig.paint();
    let slot = rig.controller.geometry().items[0].count;
    assert_eq!("12345", painted.texts[0].text);
    assert!(painted.texts[0].at.0 + 30.0 <= slot.0 + slot.2);
    rig.host
        .meta
        .objects
        .retain(|object| object.object_id == TAPER_ID);
    assert_eq!(three, settle(&mut rig));
}

/// A count below its item's warn level is drawn in the warning colour; at
/// or above it, or with no warn level, in the text colour.
#[test]
fn a_count_below_its_warn_level_is_drawn_in_the_warning_colour() {
    let mut rig = Rig::new();
    rig.controller.set_theme(&moss());
    rig.own(item(SCARAB_ID, "Lead Scarab", 5, true, 0));
    rig.own(item(TAPER_ID, "Prismatic Taper", 50, true, 0));
    rig.add(SCARAB_ID);
    rig.add(TAPER_ID);
    let style = rig.controller.style().clone();
    assert_eq!(WARNING_ON_DARK, style.warning);

    assert_eq!(vec![style.text.clone(); 2], rig.paint().count_colors());

    assert!(rig.controller.set_warn(&mut rig.host, 0, 10));
    assert!(rig.controller.set_warn(&mut rig.host, 1, 50));
    assert!(rig.controller.warns(0));
    assert!(!rig.controller.warns(1));
    assert_eq!(
        vec![style.warning.clone(), style.text.clone()],
        rig.paint().count_colors()
    );

    rig.own(item(0x8000_0098, "Lead Scarab", 5, true, 0));
    rig.ticks(1, 0.6);
    assert!(!rig.controller.warns(0));
    assert_eq!(vec![style.text.clone(); 2], rig.paint().count_colors());
}

/// The strip is painted in the theme the player picked: its face, edge and
/// text colours, a dark red warning on the light theme, rounded corners,
/// and the bundled font (asked for once); Classic keeps square corners and
/// the interface font. A theme picked while the HUD shows repaints it.
#[test]
fn the_strip_is_painted_in_the_players_theme_and_follows_a_change() {
    let mut rig = Rig::new();
    rig.own(item(SCARAB_ID, "Lead Scarab", 5, true, 0));
    rig.add(SCARAB_ID);
    rig.ticks(1, 0.015);

    let classic = rig.paint();
    assert_eq!(HudStyle::classic(), *rig.controller.style());
    assert_eq!(0, classic.texts[0].font);
    assert!(rig.ui.fonts_asked.is_empty());
    // Square: the face is one fill of the whole strip.
    let face = (
        1.0,
        1.0,
        f64::from(rig.controller.geometry().width) - 2.0,
        f64::from(TITLE_HEIGHT + ROW_HEIGHT),
    );
    assert!(
        classic
            .fills
            .contains(&(face, HudStyle::classic().background))
    );

    let painted = rig.ui.invalidated;
    rig.controller.set_theme(&parchment());
    rig.ticks(1, 0.015);
    assert!(
        rig.ui.invalidated > painted,
        "a theme change repaints the strip"
    );

    let theme = parchment();
    let palette = theme.palette.as_ref().unwrap();
    let style = rig.controller.style().clone();
    assert_eq!(palette.background, style.background);
    assert_eq!(palette.border, style.border);
    assert_eq!(palette.text, style.text);
    assert_eq!(palette.muted, style.muted);
    assert_eq!(WARNING_ON_LIGHT, style.warning);
    assert_eq!(f64::from(theme_metrics::CONTROL_RADIUS), style.radius);

    let themed = rig.paint();
    assert_eq!(FONT_HANDLE, themed.texts[0].font);
    assert_eq!(palette.text, themed.texts[0].color);
    assert_eq!([FONT_SIZE], rig.ui.fonts_asked.as_slice());
    assert!(!themed.fills.contains(&(face, palette.background.clone())));
    let top_row = themed
        .fills
        .iter()
        .find(|(_, color)| *color == palette.border)
        .unwrap();
    assert!(top_row.0.0 > 0.0, "the corners are rounded");
    assert!(
        themed
            .fills
            .iter()
            .any(|(_, color)| *color == palette.background),
        "the face is the theme's"
    );

    rig.paint();
    assert_eq!(1, rig.ui.fonts_asked.len());
    rig.controller.dispose(&mut rig.ui);
    assert_eq!([FONT_HANDLE], rig.ui.fonts_released.as_slice());
}

// C# source: ComponentHudControllerTests.cs :: TheListIsKeptPerCharacterAndTheSwitchForAll
/// The list is the character's: another character on the same storage
/// starts empty, and the first one's list comes back at its next login.
/// The on switch is shared.
#[test]
fn the_list_is_kept_per_character_and_the_switch_for_all() {
    let storage = SharedStorage::new();
    let mut rig = Rig::with(storage.clone(), true);
    rig.own(item(SCARAB_ID, "Lead Scarab", 1, true, SCARAB_ICON));
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
/// Adding with nothing selected, or a selection the client does not know,
/// adds nothing.
#[test]
fn adding_without_a_known_selection_adds_nothing() {
    let mut rig = Rig::new();
    rig.ticks(1, 0.015);
    assert_eq!(
        Err(AddRefusal::NothingSelected),
        rig.controller.add_selection(&mut rig.host)
    );
    rig.host.meta.selection.selected = Some(0x8000_FFFF);
    assert_eq!(
        Err(AddRefusal::NothingSelected),
        rig.controller.add_selection(&mut rig.host)
    );
    assert!(rig.controller.entries().is_empty());
}

// C# source: ComponentHudControllerTests.cs :: RemovingByNameIgnoresCase
/// Removing by name ignores case and reports the listed spelling.
#[test]
fn removing_by_name_ignores_case() {
    let mut rig = Rig::new();
    rig.own(item(SCARAB_ID, "Lead Scarab", 1, true, 0));
    rig.add(SCARAB_ID);
    assert_eq!(
        Some("Lead Scarab".to_owned()),
        rig.controller.remove(&mut rig.host, " lead scarab ")
    );
    assert!(rig.controller.entries().is_empty());
    assert_eq!(None, rig.controller.remove(&mut rig.host, "Lead Scarab"));
}

/// Edit items lists what the character carries, packs and worn: one row
/// per name with its first icon and its total, sorted by name, nothing on
/// the ground; the search and the kind narrow it.
#[test]
fn edit_items_lists_what_the_character_carries_by_name_and_kind() {
    let mut rig = Rig::new();
    rig.own(item(TAPER_ID, "Prismatic Taper", 100, true, TAPER_ICON));
    rig.own(item(0x8000_0020, "Prismatic Taper", 25, true, 0));
    rig.own(item(SCARAB_ID, "Lead Scarab", 3, true, SCARAB_ICON));
    rig.own(item(PEA_ID, "Gold Pea", 8, true, 0));
    rig.own(of_class(
        item(KIT_ID, "Treated Healing Kit", 1, true, 0),
        ObjectClass::HealingKit,
    ));
    rig.own(item(0x8000_0021, "Lead Scarab", 99, false, 0));
    rig.ticks(1, 0.015);
    rig.controller.show_editor(&mut rig.host);

    let rows: Vec<(String, u32, i32)> = rig
        .controller
        .carried_rows()
        .iter()
        .map(|item| (item.name.clone(), item.icon_id, item.count))
        .collect();
    assert_eq!(
        vec![
            ("Gold Pea".to_owned(), 0, 8),
            ("Lead Scarab".to_owned(), SCARAB_ICON, 3),
            ("Prismatic Taper".to_owned(), TAPER_ICON, 125),
            ("Treated Healing Kit".to_owned(), 0, 1),
        ],
        rows
    );

    rig.controller.set_search("SCAR");
    assert_eq!(1, rig.controller.carried_rows().len());
    rig.controller.set_search("");
    rig.controller.set_kind(CarriedKind::PeasAndSalvage);
    assert_eq!("Gold Pea", rig.controller.carried_rows()[0].name);
    rig.controller.set_kind(CarriedKind::SpellComponents);
    assert_eq!(2, rig.controller.carried_rows().len());
    rig.controller.set_kind(CarriedKind::HealingKits);
    assert_eq!("Treated Healing Kit", rig.controller.carried_rows()[0].name);
    assert_eq!(
        Some(CarriedKind::FoodAndPotions),
        CarriedKind::from_label("food and potions")
    );
}

/// Ticking a carried row puts it on the HUD with its icon, unticking takes
/// it off; the order can be changed, a warn level set and an item removed,
/// each at once and kept for the character.
#[test]
fn edit_items_adds_orders_warns_and_removes_and_keeps_it() {
    let storage = SharedStorage::new();
    let mut rig = Rig::with(storage.clone(), true);
    rig.own(item(TAPER_ID, "Prismatic Taper", 100, true, TAPER_ICON));
    rig.own(item(SCARAB_ID, "Lead Scarab", 3, true, SCARAB_ICON));
    rig.own(item(PEA_ID, "Gold Pea", 8, true, 0));
    rig.ticks(1, 0.015);
    rig.controller.show_editor(&mut rig.host);

    // Gold Pea, Lead Scarab, Prismatic Taper.
    assert!(rig.controller.toggle_carried(&mut rig.host, 2));
    assert!(rig.controller.toggle_carried(&mut rig.host, 1));
    assert!(rig.controller.toggle_carried(&mut rig.host, 0));
    assert_eq!(
        ["Prismatic Taper", "Lead Scarab", "Gold Pea"],
        rig.names().as_slice()
    );
    assert_eq!(TAPER_ICON, rig.controller.entries()[0].icon_id);
    assert!(rig.controller.is_listed("Gold Pea"));
    assert!(rig.controller.toggle_carried(&mut rig.host, 0));
    assert_eq!(["Prismatic Taper", "Lead Scarab"], rig.names().as_slice());
    assert!(!rig.controller.toggle_carried(&mut rig.host, 7));

    rig.controller.select_entry(1);
    assert!(rig.controller.move_entry(&mut rig.host, 1, true));
    assert_eq!(["Lead Scarab", "Prismatic Taper"], rig.names().as_slice());
    assert_eq!(
        Some(0),
        rig.controller.selected_entry(),
        "the selection follows"
    );
    let mut moved = Rig::with(storage.clone(), false);
    moved.ticks(1, 0.015);
    assert_eq!(["Lead Scarab", "Prismatic Taper"], moved.names().as_slice());
    assert!(!rig.controller.move_entry(&mut rig.host, 0, true));
    assert!(rig.controller.move_entry(&mut rig.host, 0, false));
    assert!(!rig.controller.move_entry(&mut rig.host, 1, false));
    assert_eq!(["Prismatic Taper", "Lead Scarab"], rig.names().as_slice());

    assert!(rig.controller.set_warn(&mut rig.host, 1, 20));
    assert!(rig.controller.set_warn(&mut rig.host, 0, -5));
    assert_eq!(0, rig.controller.entries()[0].warn);
    rig.ticks(1, 0.6);
    assert!(rig.controller.warns(1));

    let mut again = Rig::with(storage.clone(), false);
    again.ticks(1, 0.015);
    let kept: Vec<(&str, i32)> = again
        .controller
        .entries()
        .iter()
        .map(|entry| (entry.name.as_str(), entry.warn))
        .collect();
    assert_eq!(vec![("Prismatic Taper", 0), ("Lead Scarab", 20)], kept);

    assert!(rig.controller.remove_at(&mut rig.host, 0));
    assert_eq!(["Lead Scarab"], rig.names().as_slice());
    let mut third = Rig::with(storage, false);
    third.ticks(1, 0.015);
    assert_eq!(1, third.controller.entries().len());
}

/// An item listed by name alone (from the Consumables page, say) takes the
/// icon of the first such item the character owns when it is counted, and
/// keeps it.
#[test]
fn an_item_listed_without_an_icon_takes_the_carried_ones() {
    let mut rig = Rig::new();
    rig.ticks(1, 0.015);
    rig.controller.add(&mut rig.host, "Lead Scarab", 0).unwrap();
    rig.ticks(1, 0.015);
    assert_eq!(0, rig.controller.entries()[0].icon_id);
    rig.own(item(SCARAB_ID, "Lead Scarab", 2, true, SCARAB_ICON));
    rig.ticks(1, 0.6);
    assert_eq!(SCARAB_ICON, rig.controller.entries()[0].icon_id);
    assert!(rig.stored().contains(&SCARAB_ICON.to_string()));
}

/// A list saved in the first form (place and entries by name and icon, no
/// layout and no warn levels) loads as a column whose items never warn, at
/// its place and in its order; the next change writes the current form.
#[test]
fn a_list_saved_in_the_first_form_loads_into_the_current_one() {
    let storage = SharedStorage::new();
    let key = comps_hud::key_for("Coldeve", "Acdream");
    SharedStorage::clone(&storage)
        .write_text(
            &key,
            "{\r\n  \"X\": 340,\r\n  \"Y\": 90,\r\n  \"Entries\": [\r\n    {\r\n      \"Name\": \"Prismatic Taper\",\r\n      \"IconId\": 100671328\r\n    },\r\n    {\r\n      \"Name\": \"Lead Scarab\",\r\n      \"IconId\": 100671323\r\n    }\r\n  ]\r\n}",
        )
        .unwrap();
    let mut rig = Rig::with(storage, true);
    rig.ticks(1, 0.015);
    assert_eq!((340, 90), rig.controller.position());
    assert_eq!(HudLayout::Column, rig.controller.layout());
    assert_eq!(
        [
            ComponentHudEntry {
                name: "Prismatic Taper".to_owned(),
                icon_id: TAPER_ICON,
                warn: 0,
            },
            scarab_entry(),
        ],
        rig.controller.entries()
    );

    assert!(rig.controller.set_warn(&mut rig.host, 1, 12));
    let stored = rig.stored();
    assert!(stored.contains("\"Layout\": \"Column\""), "{stored}");
    assert!(stored.contains("\"Warn\": 12"), "{stored}");
    assert!(stored.contains("\"X\": 340"), "{stored}");
}

impl Rig {
    /// A pointer event of any kind with `button` held (0: none).
    fn event(&mut self, kind: u8, at: (f64, f64), button: u8) {
        #[allow(clippy::cast_possible_truncation)]
        let event = PointerEvent {
            canvas: u32::try_from(self.canvas_index(comps_hud::CANVAS_ID) + 1).unwrap(),
            kind,
            x: at.0 as f32,
            y: at.1 as f32,
            button,
            modifiers: 0,
            wheel_delta: 0,
        };
        self.controller
            .on_pointer(&mut self.host, &mut self.ui, &event);
        // The plugin hands a pointer event's canvas moves over at once.
        self.ui.flush();
    }

    /// A hover, and the frame after it, where the tip's canvas follows.
    fn hover(&mut self, at: (f64, f64)) {
        self.event(PointerEvent::HOVER, at, 0);
        self.ticks(1, 0.015);
    }

    fn tip_text(&self) -> Option<&str> {
        self.controller.tip().map(|tip| tip.text.as_str())
    }

    fn logged(&self, part: &str) -> bool {
        self.host
            .meta
            .logs
            .iter()
            .any(|(_, line)| line.contains(part))
    }
}

fn centre((x, y, width, height): HudRect) -> (f64, f64) {
    (x + width / 2.0, y + height / 2.0)
}

/// The pointer over an item names it, in both layouts, and over a button
/// says what it does; the tip stands on a canvas of its own that takes no
/// pointer input, beside a column (left of it when the strip is far enough
/// from the screen's left edge) and above a line. Leaving the strip, or
/// pressing on it, clears it.
#[test]
fn hovering_names_the_item_or_the_button_under_the_pointer() {
    let mut rig = Rig::new();
    rig.controller.set_theme(&moss());
    rig.own(item(SCARAB_ID, "Lead Scarab", 3, true, 0));
    rig.own(item(TAPER_ID, "Prismatic Taper", 9, true, 0));
    rig.add(SCARAB_ID);
    rig.add(TAPER_ID);
    rig.ticks(1, 0.015);
    assert!(!rig.tip_canvas().spec.accepts_pointer_input);
    assert!(!rig.tip_canvas().visible);

    let column = rig.controller.geometry().clone();
    rig.hover(centre(column.items[1].cell));
    assert_eq!(Some("Prismatic Taper"), rig.tip_text());
    assert!(rig.tip_canvas().visible);
    let (x, y) = rig.controller.position();
    let row = column.items[1].cell;
    assert_eq!(
        f64::from(x + column.width + 4),
        rig.tip_canvas().offset.0,
        "beside the column, on its right near the screen's left edge"
    );
    #[allow(clippy::cast_possible_truncation)]
    let middle = f64::from(y) + row.1 + (row.3 - f64::from(comps_hud::tip::TIP_HEIGHT)) / 2.0;
    assert!((rig.tip_canvas().offset.1 - middle).abs() <= 1.0);

    let mut painter = RecordingPainter::default();
    rig.controller.paint_tip(&mut painter, &mut rig.ui);
    assert_eq!(1, painter.texts.len());
    assert_eq!("Prismatic Taper", painter.texts[0].text);
    assert_eq!(rig.controller.style().text, painter.texts[0].color);
    assert_eq!(FONT_HANDLE, painter.texts[0].font);
    assert!(
        painter
            .fills
            .iter()
            .any(|(_, color)| *color == rig.controller.style().background)
    );

    for (button, text) in [
        (HudButton::Layout, "Line layout"),
        (HudButton::Edit, "Edit items"),
        (HudButton::Hide, "Hide"),
    ] {
        rig.hover(centre(column.button(button)));
        assert_eq!(Some(text), rig.tip_text());
    }
    rig.hover((4.0, f64::from(column.height) - 1.0));
    assert_eq!(None, rig.tip_text(), "the strip's bottom edge");
    assert!(!rig.tip_canvas().visible);

    rig.hover(centre(column.items[0].cell));
    assert_eq!(Some("Lead Scarab"), rig.tip_text());
    rig.event(PointerEvent::LEAVE, (60.0, 5.0), 0);
    assert_eq!(None, rig.tip_text());
    rig.ticks(1, 0.015);
    assert!(!rig.tip_canvas().visible);
    rig.hover(centre(column.items[0].cell));
    rig.event(PointerEvent::DOWN, centre(column.items[0].cell), 1);
    assert_eq!(None, rig.tip_text(), "a press clears it");
    rig.event(PointerEvent::UP, centre(column.items[0].cell), 1);

    rig.press(HudButton::Layout);
    rig.ticks(1, 0.015);
    let line = rig.controller.geometry().clone();
    rig.hover(centre(line.items[1].cell));
    assert_eq!(Some("Prismatic Taper"), rig.tip_text());
    assert_eq!(
        (
            f64::from(x) + line.items[1].cell.0,
            f64::from(y - 4 - comps_hud::tip::TIP_HEIGHT)
        ),
        rig.tip_canvas().offset,
        "above the line"
    );
    rig.hover(centre(line.button(HudButton::Layout)));
    assert_eq!(Some("Column layout"), rig.tip_text());

    // Far from the screen's left edge, a column's tip stands left of it.
    rig.press(HudButton::Layout);
    rig.ticks(1, 0.015);
    rig.pointer(0, 4.0, 30.0);
    rig.pointer(1, 404.0, 30.0);
    rig.ticks(1, 0.015);
    rig.hover(centre(column.items[0].cell));
    let tip = rig.controller.tip().unwrap().clone();
    assert!(tip.right_aligned);
    assert_eq!(
        f64::from(600 - 4 - comps_hud::tip::TIP_WIDTH),
        rig.tip_canvas().offset.0
    );
}

/// A count is given the room it takes in the HUD's font: a five-digit
/// count widens the column past its 50 pixels and gets its own room on
/// the line, so no digit is cut off.
#[test]
fn a_five_digit_count_is_measured_and_given_its_room() {
    let mut rig = Rig::new();
    rig.own(item(TAPER_ID, "Prismatic Taper", 12345, true, 0));
    rig.add(TAPER_ID);
    rig.ticks(1, 0.015);
    let painter = rig.paint();
    rig.ticks(1, 0.015);
    // The test painter measures six pixels a character.
    let measured = 30.0;
    let column = rig.controller.geometry().clone();
    assert!(column.width > COLUMN_MIN_WIDTH);
    assert_eq!(column.width, rig.canvas().spec.width);
    let (box_x, _, box_w, _) = column.items[0].count;
    assert!(box_w >= measured, "the count's box holds all five digits");
    let painted = rig.paint();
    let text = &painted.texts[0];
    assert_eq!("12345", text.text);
    assert!(text.at.0 >= box_x && text.at.0 + measured <= box_x + box_w);
    assert_eq!("12345", painter.texts[0].text);

    rig.press(HudButton::Layout);
    rig.ticks(1, 0.015);
    rig.paint();
    rig.ticks(1, 0.015);
    let line = rig.controller.geometry().clone();
    assert!(line.items[0].count.2 >= measured);
    assert_eq!(line.width, rig.canvas().spec.width);
}

/// A kept list that is not JSON is said in the log and copied aside before
/// anything is written over it; the HUD starts empty, and the next change
/// writes a new list.
#[test]
fn a_corrupt_list_is_kept_aside_and_said_in_the_log() {
    let storage = SharedStorage::new();
    let key = comps_hud::key_for("Coldeve", "Acdream");
    let corrupt = "{ \"X\": 12, \"Entries\": [ { \"Name\": ";
    storage.insert(&key, corrupt);
    let mut rig = Rig::with(storage.clone(), true);
    rig.ticks(1, 0.015);
    assert!(rig.controller.entries().is_empty());
    assert!(rig.logged("could not be loaded"));
    let copies: Vec<String> = storage
        .keys()
        .into_iter()
        .filter(|key| key.starts_with("recovery/comps-hud/"))
        .collect();
    assert_eq!(1, copies.len());
    assert!(storage.get(&copies[0]).unwrap().contains(corrupt));

    rig.own(item(SCARAB_ID, "Lead Scarab", 1, true, 0));
    rig.add(SCARAB_ID);
    assert!(storage.get(&key).unwrap().contains("Lead Scarab"));
    assert!(storage.get(&copies[0]).unwrap().contains(corrupt));
}

/// A list that reads only in part gives what it can (an entry with no
/// readable name is left out, any other value that does not read is its
/// default), and the original is kept aside and said in the log.
#[test]
fn a_half_valid_list_keeps_what_reads_and_the_original() {
    let storage = SharedStorage::new();
    let key = comps_hud::key_for("Coldeve", "Acdream");
    let half = "{ \"X\": \"far\", \"Y\": 40, \"Layout\": \"Line\", \"Entries\": [ { \"Name\": \"Lead Scarab\", \"IconId\": 100671323, \"Warn\": \"lots\" }, { \"Name\": 7 }, { \"Name\": \"Prismatic Taper\", \"Warn\": 5 } ] }";
    storage.insert(&key, half);
    let mut rig = Rig::with(storage.clone(), true);
    rig.ticks(1, 0.015);
    assert_eq!((comps_hud::DEFAULT_X, 40), rig.controller.position());
    assert_eq!(HudLayout::Line, rig.controller.layout());
    let kept: Vec<(&str, i32)> = rig
        .controller
        .entries()
        .iter()
        .map(|entry| (entry.name.as_str(), entry.warn))
        .collect();
    assert_eq!(vec![("Lead Scarab", 0), ("Prismatic Taper", 5)], kept);
    assert!(rig.logged("could not be loaded"));
    assert!(
        storage
            .keys()
            .iter()
            .any(|key| key.starts_with("recovery/comps-hud/")
                && storage.get(key).unwrap().contains(half))
    );
}

/// A list the storage cannot read is said in the log and not written
/// over: what the session changes is not saved.
#[test]
fn an_unreadable_list_is_not_written_over() {
    let storage = SharedStorage::new();
    let key = comps_hud::key_for("Coldeve", "Acdream");
    storage.insert(&key, "{ \"X\": 5, \"Y\": 5, \"Entries\": [] }");
    storage.fail("read", "in use by another program");
    let mut rig = Rig::with(storage.clone(), true);
    rig.ticks(1, 0.015);
    assert!(rig.logged("in use by another program"));
    rig.own(item(SCARAB_ID, "Lead Scarab", 1, true, 0));
    rig.add(SCARAB_ID);
    assert_eq!(1, rig.controller.entries().len());
    assert_eq!(
        Some("{ \"X\": 5, \"Y\": 5, \"Entries\": [] }".to_owned()),
        storage.get(&key)
    );
    assert!(
        !storage
            .keys()
            .iter()
            .any(|key| key.starts_with("recovery/"))
    );
}

/// The strip never stands past the screen's top or left edge: a kept place
/// there, or a drag past it, stops at 0; `reset_place` puts it back at its
/// first place and keeps that.
#[test]
fn the_strip_stays_on_the_screen_and_resets_to_its_first_place() {
    let storage = SharedStorage::new();
    let key = comps_hud::key_for("Coldeve", "Acdream");
    storage.insert(&key, "{ \"X\": -40, \"Y\": -5, \"Entries\": [] }");
    let mut rig = Rig::with(storage.clone(), true);
    rig.ticks(1, 0.015);
    assert_eq!((0, 0), rig.controller.position());
    assert_eq!((0.0, 0.0), rig.canvas().offset);

    rig.pointer(0, 20.0, 20.0);
    rig.pointer(1, 80.0, 50.0);
    assert_eq!((60, 30), rig.controller.position());
    rig.pointer(0, 20.0, 20.0);
    rig.pointer(1, -500.0, -500.0);
    assert_eq!((0, 0), rig.controller.position());
    assert_eq!((0.0, 0.0), rig.canvas().offset);

    assert!(rig.controller.reset_place(&mut rig.host));
    rig.ticks(1, 0.015);
    let first = (comps_hud::DEFAULT_X, comps_hud::DEFAULT_Y);
    assert_eq!(first, rig.controller.position());
    assert_eq!(
        (f64::from(first.0), f64::from(first.1)),
        rig.canvas().offset
    );
    let mut again = Rig::with(storage, false);
    again.ticks(1, 0.015);
    assert_eq!(first, again.controller.position());
}

/// Another character starts with its own HUD: a column at the first place,
/// with nothing listed, whatever layout, place and warn levels the first
/// one left; an item both list has each its own warn level.
#[test]
fn another_character_does_not_inherit_the_layout_place_or_warn_levels() {
    let storage = SharedStorage::new();
    let mut rig = Rig::with(storage, true);
    rig.own(item(SCARAB_ID, "Lead Scarab", 3, true, 0));
    rig.add(SCARAB_ID);
    rig.controller.set_warn(&mut rig.host, 0, 50);
    rig.press(HudButton::Layout);
    rig.ticks(1, 0.015);
    rig.pointer(0, 4.0, 4.0);
    rig.pointer(1, 104.0, 64.0);
    assert_eq!(HudLayout::Line, rig.controller.layout());

    rig.controller.reset(&mut rig.ui);
    rig.identity("Coldeve", "Sigrid");
    rig.ticks(1, 0.015);
    assert_eq!(HudLayout::Column, rig.controller.layout());
    assert_eq!(
        (comps_hud::DEFAULT_X, comps_hud::DEFAULT_Y),
        rig.controller.position()
    );
    assert!(rig.controller.entries().is_empty());
    rig.add(SCARAB_ID);
    assert_eq!(0, rig.controller.entries()[0].warn);
}

/// The session's end lets go of the font through the interface, as the
/// strip's other resources go.
#[test]
fn the_font_is_let_go_at_the_sessions_end() {
    let mut rig = Rig::new();
    rig.controller.set_theme(&moss());
    rig.own(item(SCARAB_ID, "Lead Scarab", 3, true, 0));
    rig.add(SCARAB_ID);
    rig.paint();
    assert_eq!(1, rig.ui.fonts_asked.len());
    rig.controller.reset(&mut rig.ui);
    assert_eq!([FONT_HANDLE], rig.ui.fonts_released.as_slice());
    rig.ticks(1, 0.015);
    rig.paint();
    assert_eq!(2, rig.ui.fonts_asked.len(), "the next session asks again");
}

/// A click on a carried row acts on the item the window showed in that
/// row, even after what is carried was taken again with a new item ahead
/// of it.
#[test]
fn a_carried_row_click_acts_on_the_item_shown_there() {
    let mut rig = Rig::new();
    rig.own(item(SCARAB_ID, "Lead Scarab", 3, true, 0));
    rig.own(item(TAPER_ID, "Prismatic Taper", 9, true, 0));
    rig.ticks(1, 0.015);
    rig.controller.show_editor(&mut rig.host);
    assert_eq!(
        vec!["Lead Scarab", "Prismatic Taper"],
        rig.controller.note_shown_carried()
    );
    // A pea arrives and sorts first; the window still shows the old rows.
    rig.own(item(PEA_ID, "Gold Pea", 8, true, 0));
    rig.ticks(1, 0.6);
    assert_eq!("Gold Pea", rig.controller.carried_rows()[0].name);
    assert!(rig.controller.toggle_carried(&mut rig.host, 0));
    assert_eq!(["Lead Scarab"], rig.names().as_slice());
    rig.controller.select_carried(1);
    assert_eq!(Some(1), rig.controller.selected_carried());
    // The next push shows the new rows, and the selection follows its name.
    rig.controller.note_shown_carried();
    assert_eq!(Some(2), rig.controller.selected_carried());
}

/// The window's empty states: a line saying nothing is carried, that
/// nothing matches, or that the HUD is empty; an empty HUD list selects
/// nothing.
#[test]
fn edit_items_says_when_its_lists_are_empty() {
    let mut rig = Rig::new();
    rig.ticks(1, 0.015);
    rig.controller.show_editor(&mut rig.host);
    assert_eq!("You carry nothing yet", rig.controller.carried_heading());
    assert!(rig.controller.entries_heading().contains("nothing yet"));
    assert_eq!(None, rig.controller.selected_entry());
    assert_eq!(None, rig.controller.selected_carried());

    rig.own(item(SCARAB_ID, "Lead Scarab", 3, true, 0));
    rig.controller.show_editor(&mut rig.host);
    assert_eq!("You carry", rig.controller.carried_heading());
    rig.controller.set_search("zzz");
    assert_eq!(
        "Nothing you carry matches",
        rig.controller.carried_heading()
    );
    rig.controller.set_search("");
    assert!(rig.controller.toggle_carried(&mut rig.host, 0));
    assert_eq!("In the HUD", rig.controller.entries_heading());
    assert_eq!(Some(0), rig.controller.selected_entry());
}

/// Out of the world there is no character to keep a layout or a place
/// for: both are refused.
#[test]
fn the_layout_and_place_are_refused_out_of_the_world() {
    let mut rig = Rig::with(SharedStorage::new(), true);
    rig.host.meta.character = None;
    rig.ticks(1, 0.015);
    assert!(!rig.controller.set_layout(&mut rig.host, HudLayout::Line));
    assert_eq!(HudLayout::Column, rig.controller.layout());
    assert!(!rig.controller.reset_place(&mut rig.host));
}

/// The handle the test client gives an object's composed icon.
fn composed(object_id: u32) -> u32 {
    0x7000_0000 | (object_id & 0xFFFF)
}

/// An item the character carries is drawn from its object's composed icon
/// (the icon with its underlay, overlay and effects, as the game draws the
/// item), not the bare icon art; one it carries none of falls back to the
/// bare icon, and the composed one is let go. A new object of that name is
/// drawn once the old one is gone.
#[test]
fn an_item_carried_is_drawn_from_its_objects_composed_icon() {
    let mut rig = Rig::new();
    rig.ui.client_art.insert(SCARAB_ICON);
    rig.ui.object_art.insert(SCARAB_ID);
    rig.ui.object_art.insert(0x8000_0077);
    rig.own(item(SCARAB_ID, "Lead Scarab", 12, true, SCARAB_ICON));
    rig.add(SCARAB_ID);
    rig.ticks(1, 0.6);
    assert_eq!(Some(SCARAB_ID), rig.controller.drawn_object("Lead Scarab"));
    let painted = rig.paint();
    assert_eq!(
        vec![composed(SCARAB_ID)],
        painted
            .images
            .iter()
            .map(|(image, _)| image.handle)
            .collect::<Vec<_>>()
    );
    assert!(!rig.ui.requested.contains(&SCARAB_ICON), "no bare art");

    // Carried no more: the bare icon, and the composed one let go.
    rig.host.meta.objects.clear();
    rig.ticks(1, 0.6);
    assert_eq!(None, rig.controller.drawn_object("Lead Scarab"));
    let painted = rig.paint();
    assert_eq!(SCARAB_ICON, painted.images[0].0.handle);
    assert!(
        rig.ui
            .released
            .iter()
            .any(|image| image.handle == composed(SCARAB_ID))
    );

    // Another stack of it: drawn from that object.
    rig.own(item(0x8000_0077, "Lead Scarab", 3, true, SCARAB_ICON));
    rig.ticks(1, 0.6);
    assert_eq!(composed(0x8000_0077), rig.paint().images[0].0.handle);
}

impl Rig {
    /// The strip's canvases registered so far.
    fn strip_canvases(&self) -> usize {
        self.ui
            .canvases
            .iter()
            .filter(|canvas| canvas.spec.canvas_id == comps_hud::CANVAS_ID)
            .count()
    }
}

/// The count characters are measured once, before the first frame, so a
/// strip is registered at its size at once, and a count that gains a digit
/// replaces the canvas once.
#[test]
fn a_count_change_replaces_the_canvas_at_most_once() {
    let mut rig = Rig::new();
    rig.own(item(TAPER_ID, "Prismatic Taper", 12345, true, 0));
    rig.add(TAPER_ID);
    rig.ticks(3, 0.015);
    let first = rig.strip_canvases();
    let column = rig.controller.geometry().clone();
    assert!(column.items[0].count.2 >= 30.0);
    assert_eq!(column.width, rig.canvas().spec.width, "sized right at once");
    let measured = rig.ui.measured.len();
    assert_eq!(11, measured, "the ten digits and the minus, once");

    rig.own(item(0x8000_0050, "Prismatic Taper", 90_000, true, 0));
    for _ in 0..5 {
        rig.ticks(1, 0.6);
        rig.paint();
    }
    assert_eq!(first + 1, rig.strip_canvases(), "one replacement");
    assert_eq!(measured, rig.ui.measured.len(), "nothing measured again");
}

/// A canvas is not replaced while a press holds it: a count that changes
/// mid-drag waits, the strip goes on following the pointer, its place is
/// saved on release, and the next frame gives it its new size.
#[test]
fn a_resize_waits_for_a_drag_to_end() {
    let storage = SharedStorage::new();
    let key = comps_hud::key_for("Coldeve", "Acdream");
    let mut rig = Rig::with(storage.clone(), true);
    rig.own(item(TAPER_ID, "Prismatic Taper", 9, true, 0));
    rig.add(TAPER_ID);
    rig.ticks(1, 0.015);
    let before = rig.strip_canvases();
    let (x, y) = rig.canvas().offset;
    rig.event(PointerEvent::DOWN, (8.0, 20.0), 1);
    rig.own(item(0x8000_0050, "Prismatic Taper", 99_990, true, 0));
    rig.ticks(1, 0.6);
    assert_eq!(before, rig.strip_canvases(), "not replaced mid-drag");
    let (cx, cy) = rig.canvas().offset;
    rig.event(PointerEvent::MOVE, (x + 58.0 - cx, y + 40.0 - cy), 1);
    assert_eq!((x + 50.0, y + 20.0), rig.canvas().offset);
    let (cx, cy) = rig.canvas().offset;
    rig.event(PointerEvent::UP, (x + 58.0 - cx, y + 40.0 - cy), 1);
    assert!(
        storage
            .get(&key)
            .unwrap()
            .contains(&format!("\"X\": {}", x + 50.0))
    );
    rig.ticks(1, 0.015);
    assert_eq!(before + 1, rig.strip_canvases());
    assert_eq!((x + 50.0, y + 20.0), rig.canvas().offset);
}

/// A canvas replaced while its tip shows takes the tip down: the old
/// canvas's leave never comes.
#[test]
fn replacing_the_canvas_takes_the_tip_down() {
    let mut rig = Rig::new();
    rig.own(item(TAPER_ID, "Prismatic Taper", 9, true, 0));
    rig.add(TAPER_ID);
    rig.ticks(1, 0.015);
    let cell = rig.controller.geometry().items[0].cell;
    rig.hover(centre(cell));
    assert!(rig.tip_canvas().visible);
    rig.own(item(0x8000_0050, "Prismatic Taper", 99_990, true, 0));
    rig.ticks(1, 0.6);
    assert_eq!(None, rig.tip_text());
    assert!(!rig.tip_canvas().visible);
}

/// A list that does not read whole, whose copy aside cannot be written,
/// is not written over: not by a change, nor by the recount filling in an
/// icon.
#[test]
fn a_bad_list_is_not_written_over_when_its_copy_fails() {
    let storage = SharedStorage::new();
    let key = comps_hud::key_for("Coldeve", "Acdream");
    let half = "{ \"X\": \"far\", \"Entries\": [ { \"Name\": \"Lead Scarab\", \"IconId\": 0 } ] }";
    storage.insert(&key, half);
    storage.refuse_writes(&mosstank_stores::recovery::recovery_key(
        comps_hud::RECOVERY_FAMILY,
        &key,
        half,
    ));
    let mut rig = Rig::with(storage.clone(), true);
    rig.own(item(SCARAB_ID, "Lead Scarab", 3, true, SCARAB_ICON));
    rig.ticks(1, 0.015);
    rig.ticks(1, 0.6);
    assert_eq!(SCARAB_ICON, rig.controller.entries()[0].icon_id);
    assert!(rig.logged("not saved over"));
    rig.own(item(TAPER_ID, "Prismatic Taper", 1, true, 0));
    rig.add(TAPER_ID);
    assert_eq!(Some(half.to_owned()), storage.get(&key));
}

/// The session's end lets go of the bare and the composed icons as well
/// as the font.
#[test]
fn the_icons_are_let_go_at_the_sessions_end() {
    let mut rig = Rig::new();
    rig.ui.client_art.insert(TAPER_ICON);
    rig.ui.object_art.insert(SCARAB_ID);
    rig.own(item(SCARAB_ID, "Lead Scarab", 3, true, SCARAB_ICON));
    rig.add(SCARAB_ID);
    rig.controller
        .add(&mut rig.host, "Prismatic Taper", TAPER_ICON)
        .unwrap();
    rig.ticks(1, 0.6);
    let painted = rig.paint();
    assert_eq!(2, painted.images.len());
    rig.controller.reset(&mut rig.ui);
    let released: Vec<u32> = rig.ui.released.iter().map(|image| image.handle).collect();
    assert!(released.contains(&composed(SCARAB_ID)), "{released:?}");
    assert!(released.contains(&TAPER_ICON), "{released:?}");
}

/// A kept place far off the screen (a damaged file) comes back within
/// reach, and the tip beside it is placed without overflowing.
#[test]
fn a_far_off_place_comes_back_and_its_tip_does_not_overflow() {
    let storage = SharedStorage::new();
    let key = comps_hud::key_for("Coldeve", "Acdream");
    storage.insert(
        &key,
        "{ \"X\": 2147483647, \"Y\": 2147483647, \"Layout\": \"Line\", \"Entries\": [ { \"Name\": \"Lead Scarab\" } ] }",
    );
    let mut rig = Rig::with(storage, true);
    rig.ticks(1, 0.015);
    assert_eq!(
        (comps_hud::MAX_PLACE, comps_hud::MAX_PLACE),
        rig.controller.position()
    );
    let geometry = rig.controller.geometry().clone();
    let tip = comps_hud::tip::place(
        (i32::MAX, i32::MAX),
        &geometry,
        geometry.items[0].cell,
        "Lead Scarab",
    );
    assert!(tip.offset.0 > 0 && tip.offset.1 > 0);
    let tip = comps_hud::tip::place(
        (i32::MAX, 0),
        &HudGeometry::new(HudLayout::Column, &[6.0]),
        geometry.items[0].cell,
        "Lead Scarab",
    );
    assert!(tip.offset.0 > 0);
    rig.hover(centre(geometry.items[0].cell));
    assert_eq!(Some("Lead Scarab"), rig.tip_text());
}

/// The tip's canvas changes on the frame, not with the pointer event: its
/// move, visibility and repaint reach the host together, so it never
/// shows its old text at its new place.
#[test]
fn the_tip_moves_shows_and_repaints_on_the_same_frame() {
    let mut rig = Rig::new();
    rig.own(item(SCARAB_ID, "Lead Scarab", 3, true, 0));
    rig.own(item(TAPER_ID, "Prismatic Taper", 9, true, 0));
    rig.add(SCARAB_ID);
    rig.add(TAPER_ID);
    rig.ticks(1, 0.015);
    let column = rig.controller.geometry().clone();
    rig.hover(centre(column.items[0].cell));
    let first = rig.tip_canvas().offset;
    assert!(rig.tip_canvas().visible);

    let painted = rig.ui.invalidated;
    rig.event(PointerEvent::HOVER, centre(column.items[1].cell), 0);
    assert_eq!(Some("Prismatic Taper"), rig.tip_text());
    assert_eq!(first, rig.tip_canvas().offset, "not moved with the event");
    assert_eq!(painted, rig.ui.invalidated, "no paint asked with the event");
    rig.ticks(1, 0.015);
    assert_ne!(first, rig.tip_canvas().offset);
    assert!(rig.ui.invalidated > painted, "moved and repainted together");
}

/// A right press then a left one: the host keeps the pointer for the right
/// button, sends the moves with it held (button 2, which the strip does not
/// follow) and drops the left one's release. The right release ends the
/// left press without moving or saving anything, so a resize waiting for
/// it goes ahead; a hover (sent only with no button held) does the same
/// as a fallback for a dropped release.
#[test]
fn a_left_press_under_a_right_one_ends_with_the_right_release_or_a_hover() {
    let storage = SharedStorage::new();
    let key = comps_hud::key_for("Coldeve", "Acdream");
    let mut rig = Rig::with(storage.clone(), true);
    rig.own(item(TAPER_ID, "Prismatic Taper", 9, true, 0));
    rig.add(TAPER_ID);
    rig.ticks(1, 0.015);
    let before = rig.strip_canvases();
    let place = rig.canvas().offset;
    let saved = storage.get(&key);

    rig.event(PointerEvent::DOWN, (8.0, 20.0), 2);
    rig.event(PointerEvent::DOWN, (8.0, 20.0), 1);
    rig.event(PointerEvent::MOVE, (28.0, 20.0), 2);
    assert_eq!(
        place,
        rig.canvas().offset,
        "a right-held move moves nothing"
    );
    rig.own(item(0x8000_0050, "Prismatic Taper", 99_990, true, 0));
    rig.ticks(1, 0.6);
    assert_eq!(before, rig.strip_canvases(), "waits for the press");
    rig.event(PointerEvent::UP, (28.0, 20.0), 2);
    assert_eq!(saved, storage.get(&key), "nothing saved");
    rig.ticks(1, 0.015);
    assert_eq!(before + 1, rig.strip_canvases(), "the resize goes ahead");
    assert_eq!(place, rig.canvas().offset);

    // The same chord with its release dropped, ended by a hover.
    rig.event(PointerEvent::DOWN, (8.0, 20.0), 2);
    rig.event(PointerEvent::DOWN, (8.0, 20.0), 1);
    rig.host
        .meta
        .objects
        .retain(|object| object.object_id == TAPER_ID);
    rig.ticks(1, 0.6);
    let held = rig.strip_canvases();
    rig.event(PointerEvent::HOVER, (8.0, 20.0), 0);
    rig.ticks(1, 0.015);
    assert_eq!(held + 1, rig.strip_canvases());
}

/// A left drag then a right press: the host keeps the pointer for the left
/// button, so the drag goes on (the right press ends nothing) and the left
/// release keeps its place.
#[test]
fn a_right_press_during_a_left_drag_does_not_end_it() {
    let storage = SharedStorage::new();
    let key = comps_hud::key_for("Coldeve", "Acdream");
    let mut rig = Rig::with(storage.clone(), true);
    rig.own(item(TAPER_ID, "Prismatic Taper", 9, true, 0));
    rig.add(TAPER_ID);
    rig.ticks(1, 0.015);
    let (x, y) = rig.canvas().offset;
    rig.event(PointerEvent::DOWN, (8.0, 20.0), 1);
    rig.event(PointerEvent::MOVE, (28.0, 20.0), 1);
    rig.event(PointerEvent::DOWN, (8.0, 20.0), 2);
    rig.event(PointerEvent::MOVE, (18.0, 30.0), 1);
    assert_eq!((x + 30.0, y + 10.0), rig.canvas().offset, "still following");
    rig.event(PointerEvent::UP, (8.0, 20.0), 1);
    let stored = storage.get(&key).unwrap();
    assert!(stored.contains(&format!("\"X\": {}", x + 30.0)), "{stored}");
    assert!(stored.contains(&format!("\"Y\": {}", y + 10.0)), "{stored}");
}

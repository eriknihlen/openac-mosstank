//! The Comps HUD (`ComponentHudController`), as Virindi HUDs draws it
//! beside VTank: a narrow strip with one row per item the player chose,
//! each row the item's icon and how many of it the character carries,
//! re-counted every 600 ms.
//!
//! The reference is a 62-pixel window whose title bar carries a plus and a
//! minus button. Plus adds the selected object, by name and icon, unless
//! that name is listed already. Minus starts a removal: the counts flash
//! red and dark red, and the next row clicked is taken off; minus again, or
//! 30 seconds, cancels it. The list and the window's place are kept for
//! each character; whether the HUD is on is kept once for every character,
//! as the reference keeps it in its shared settings file.
//!
//! A count is the reference's: every object the character owns, in a pack
//! or worn, whose name is the listed name exactly, adding its stack size,
//! or one for an object that has none.
//!
//! Here the HUD is a canvas the host paints under every window, so the
//! title bar is painted too: a strip [`HEADER_HEIGHT`] tall with the two
//! buttons at its right. A left drag anywhere else moves the HUD, as the
//! window's title bar does in the reference; it moves when the button
//! comes up, as the clients readout does, for the reason given there.

use std::collections::HashMap;

use ac_plugin_abi::ui::{Color, PointerEvent};
use ac_plugin_abi::world::WorldObject;
use mosstank_dotnet::text::{casing, ordinal};
use mosstank_meta::host::LogLevel;
use mosstank_stores::recovery::sha256_hex;
use mosstank_stores::stj::{self, JsonError, Object};
use mosstank_stores::storage::PluginStorage;
use openac_plugin_compat::json::{self, JsonValue};

use crate::canvas::{
    BUTTON_LEFT, MapCanvasSpec, MapImage, MapPainter, MapUi, POINTER_CANCELLED, POINTER_DOWN,
    POINTER_UP,
};
use crate::host::UbHost;

/// The canvas's id.
pub const CANVAS_ID: &str = "comps-hud";
/// The HUD's width, pixels.
pub const HUD_WIDTH: i32 = 62;
/// One row's height, pixels.
pub const ROW_HEIGHT: i32 = 20;
/// An icon's size, pixels.
pub const ICON_SIZE: i32 = 20;
/// Where a count is written from, pixels from the left.
pub const TEXT_X: i32 = 22;
/// The title strip's height, pixels.
pub const HEADER_HEIGHT: i32 = 16;
/// A button's size, pixels.
pub const BUTTON_SIZE: i32 = 14;
/// The height of an empty list, as the reference sizes its window with
/// nothing in it.
pub const EMPTY_LIST_HEIGHT: i32 = 16;
/// The place a character's HUD starts at, pixels from the screen's left.
pub const DEFAULT_X: i32 = 200;
/// The place a character's HUD starts at, pixels from the screen's top.
pub const DEFAULT_Y: i32 = 150;
/// How often the counts are taken again, as the reference's 600 ms timer
/// does.
pub const REFRESH_SECONDS: f64 = 0.6;
/// How long a removal waits for a row to be clicked before it lapses.
pub const REMOVAL_TIMEOUT_SECONDS: f64 = 30.0;

/// Where the on switch is kept, for every character.
pub const ENABLED_KEY: &str = "comps-hud/enabled.txt";
/// Where each character's list is kept.
pub const CHARACTER_PREFIX: &str = "comps-hud/characters/";

/// What minus says when it starts a removal.
pub const REMOVAL_PROMPT: &str =
    "Comps HUD: click a row to take it off the list (- again cancels).";
/// What minus says when it cancels one.
pub const REMOVAL_CANCELLED: &str = "Comps HUD: nothing taken off.";

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

const BACK: Color = rgba(0, 0, 0, 150);
const HEADER_BACK: Color = rgba(110, 45, 25, 200);
const BUTTON_BACK: Color = rgba(0, 0, 0, 120);
const EDGE: Color = rgba(0, 0, 0, 255);
const WHITE: Color = rgba(255, 255, 255, 255);
/// The counts' colour outside a removal.
pub const TEXT: Color = rgba(255, 255, 255, 255);
/// The bright half of the removal flash.
pub const REMOVAL_BRIGHT: Color = rgba(255, 0, 0, 255);
/// The dark half of the removal flash.
pub const REMOVAL_DARK: Color = rgba(139, 0, 0, 255);

/// One item the HUD counts: the name it is counted by and the icon it was
/// added with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentHudEntry {
    /// The name, matched exactly.
    pub name: String,
    /// The icon (a dat id), 0 for none.
    pub icon_id: u32,
}

/// A rectangle in the canvas's pixels: x, y, width, height.
pub type HudRect = (f64, f64, f64, f64);

/// The plus button, in the canvas's pixels.
pub const PLUS_BUTTON: HudRect = (
    (HUD_WIDTH - BUTTON_SIZE - 1) as f64,
    1.0,
    BUTTON_SIZE as f64,
    BUTTON_SIZE as f64,
);

/// The minus button, in the canvas's pixels.
pub const MINUS_BUTTON: HudRect = (
    (HUD_WIDTH - 2 * BUTTON_SIZE - 3) as f64,
    1.0,
    BUTTON_SIZE as f64,
    BUTTON_SIZE as f64,
);

/// The HUD.
#[derive(Debug)]
pub struct ComponentHudController {
    icons: HashMap<u32, MapImage>,
    icon_order: Vec<u32>,
    entries: Vec<ComponentHudEntry>,
    counts: Vec<i32>,
    enabled: bool,
    scope: Option<String>,
    key: Option<String>,
    x: i32,
    y: i32,
    canvas: Option<u32>,
    canvas_visible: bool,
    canvas_height: i32,
    refresh_remaining: f64,
    dirty: bool,
    removing: bool,
    removal_elapsed: f64,
    /// The bright half of the removal flash; the reference starts on it.
    flash_bright: bool,
    /// The point of the canvas a left press took hold of, while it is held.
    grab: Option<(f64, f64)>,
}

impl ComponentHudController {
    /// A HUD, on when `storage` says it was left on.
    #[must_use]
    pub fn new(storage: &mut dyn PluginStorage) -> Self {
        let enabled = storage.is_available()
            && storage
                .read_text(ENABLED_KEY)
                .ok()
                .flatten()
                .is_some_and(|text| ordinal::equals_ignore_case(text.trim(), "true"));
        Self {
            icons: HashMap::new(),
            icon_order: Vec::new(),
            entries: Vec::new(),
            counts: Vec::new(),
            enabled,
            scope: None,
            key: None,
            x: DEFAULT_X,
            y: DEFAULT_Y,
            canvas: None,
            canvas_visible: false,
            canvas_height: 0,
            refresh_remaining: 0.0,
            dirty: true,
            removing: false,
            removal_elapsed: 0.0,
            flash_bright: true,
            grab: None,
        }
    }

    /// Whether the HUD is on, for every character; kept across sessions.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Turns the HUD on or off, and keeps that for every character.
    pub fn set_enabled(&mut self, host: &mut dyn UbHost, value: bool) {
        if self.enabled == value {
            return;
        }
        self.enabled = value;
        self.dirty = true;
        if !value {
            self.cancel_removal();
        }
        let storage = host.storage();
        if storage.is_available()
            && let Err(error) =
                storage.write_text(ENABLED_KEY, if value { "true" } else { "false" })
        {
            host.log(
                LogLevel::Warn,
                &format!("The Comps HUD could not save its switch: {error}"),
            );
        }
    }

    /// Whether a character is logged in for the list to be its.
    #[must_use]
    pub fn is_bound(&self) -> bool {
        self.key.is_some()
    }

    /// The listed items, in the order they were added.
    #[must_use]
    pub fn entries(&self) -> &[ComponentHudEntry] {
        &self.entries
    }

    /// The counts as last taken, one for each entry.
    #[must_use]
    pub fn counts(&self) -> &[i32] {
        &self.counts
    }

    /// Whether a removal is waiting for a row to be clicked.
    #[must_use]
    pub fn removing(&self) -> bool {
        self.removing
    }

    /// Where the HUD stands, pixels from the screen's top-left.
    #[must_use]
    pub fn position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    /// The canvas, once registered.
    #[must_use]
    pub fn canvas(&self) -> Option<u32> {
        self.canvas
    }

    /// The colour every count is written in now: the flash during a
    /// removal, white otherwise.
    #[must_use]
    pub fn count_color(&self) -> Color {
        if !self.removing {
            TEXT
        } else if self.flash_bright {
            REMOVAL_BRIGHT
        } else {
            REMOVAL_DARK
        }
    }

    /// The canvas height the list needs: the title strip, then a row for
    /// each entry.
    #[must_use]
    pub fn canvas_height(&self) -> i32 {
        let rows = if self.entries.is_empty() {
            EMPTY_LIST_HEIGHT
        } else {
            i32::try_from(self.entries.len())
                .unwrap_or(i32::MAX)
                .saturating_mul(ROW_HEIGHT)
        };
        HEADER_HEIGHT.saturating_add(rows)
    }

    /// The frame: bind the logged-in character's list, keep the canvas
    /// sized to it and shown while the HUD is on, and take the counts every
    /// 600 ms, flashing the removal on the same beat.
    pub fn on_tick(&mut self, host: &mut dyn UbHost, ui: &mut dyn MapUi, elapsed_seconds: f64) {
        let elapsed = elapsed_seconds.max(0.0);
        self.bind(host, ui);
        let visible = self.enabled && self.key.is_some();
        if visible && (self.canvas.is_none() || self.canvas_height != self.canvas_height()) {
            self.register(ui);
        }
        let Some(canvas) = self.canvas else {
            return;
        };
        if self.canvas_visible != visible {
            self.canvas_visible = visible;
            ui.set_canvas_visible(canvas, visible);
            self.dirty = true;
        }
        if !visible {
            return;
        }

        self.refresh_remaining -= elapsed;
        if self.removing {
            self.removal_elapsed += elapsed;
        }
        if self.refresh_remaining <= 0.0 {
            self.refresh_remaining = REFRESH_SECONDS;
            self.refresh(host);
        }
        if !self.dirty {
            return;
        }
        self.dirty = false;
        ui.invalidate_canvas(canvas);
    }

    /// Adds the selected object, by its name and icon, as the plus button
    /// does: the name added, or `None` when nothing is selected, the client
    /// does not know the selection, or the name is listed already.
    pub fn add_selection(&mut self, host: &mut dyn UbHost) -> Option<String> {
        self.key.as_ref()?;
        let selected = host.selection().selected?;
        let item = host.object(selected)?;
        if item.name.is_empty() || self.entries.iter().any(|entry| entry.name == item.name) {
            return None;
        }
        self.entries.push(ComponentHudEntry {
            name: item.name.clone(),
            icon_id: item.icon_id,
        });
        self.changed(host);
        Some(item.name)
    }

    /// Takes one entry off the list; false when there is no such row.
    pub fn remove_at(&mut self, host: &mut dyn UbHost, index: usize) -> bool {
        if index >= self.entries.len() {
            return false;
        }
        self.removing = false;
        self.entries.remove(index);
        self.changed(host);
        true
    }

    /// Takes the entry with this name off the list, ignoring case: the
    /// name removed, or `None`.
    pub fn remove(&mut self, host: &mut dyn UbHost, name: &str) -> Option<String> {
        let wanted = name.trim();
        let index = self
            .entries
            .iter()
            .position(|entry| ordinal::equals_ignore_case(&entry.name, wanted))?;
        let removed = self.entries[index].name.clone();
        self.remove_at(host, index);
        Some(removed)
    }

    /// What the minus button does: start a removal, or cancel the one under
    /// way; the line that says so.
    pub fn toggle_removal(&mut self) -> &'static str {
        if self.removing {
            self.cancel_removal();
            return REMOVAL_CANCELLED;
        }
        self.removing = true;
        self.removal_elapsed = 0.0;
        self.flash_bright = true;
        self.dirty = true;
        REMOVAL_PROMPT
    }

    /// The session ended: the interface's images went with it, and the next
    /// login binds its own list.
    pub fn reset(&mut self, ui: &mut dyn MapUi) {
        self.icons.clear();
        self.icon_order.clear();
        self.grab = None;
        self.removing = false;
        self.scope = None;
        self.key = None;
        self.entries.clear();
        self.counts.clear();
        self.dirty = true;
        if let Some(canvas) = self.canvas {
            self.canvas_visible = false;
            ui.set_canvas_visible(canvas, false);
        }
    }

    /// Lets go of the icons and the canvas.
    pub fn dispose(&mut self, ui: &mut dyn MapUi) {
        for id in self.icon_order.drain(..) {
            if let Some(icon) = self.icons.remove(&id) {
                ui.release_image(icon);
            }
        }
        self.icons.clear();
        if let Some(canvas) = self.canvas.take() {
            ui.remove_canvas(canvas);
        }
    }

    /// The pointer: a press on plus or minus is that button; a press on a
    /// row during a removal takes that row off; any other left press starts
    /// a drag that moves the HUD when the button comes up. The line minus
    /// says, if it was pressed.
    pub fn on_pointer(
        &mut self,
        host: &mut dyn UbHost,
        ui: &mut dyn MapUi,
        event: &PointerEvent,
    ) -> Option<&'static str> {
        if event.button != BUTTON_LEFT {
            return None;
        }
        let at = (f64::from(event.x), f64::from(event.y));
        match event.kind {
            POINTER_DOWN if contains(PLUS_BUTTON, at) => {
                self.grab = None;
                self.add_selection(host);
            }
            POINTER_DOWN if contains(MINUS_BUTTON, at) => {
                self.grab = None;
                return Some(self.toggle_removal());
            }
            POINTER_DOWN if self.removing && at.1 >= f64::from(HEADER_HEIGHT) => {
                self.grab = None;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let row = ((at.1 - f64::from(HEADER_HEIGHT)) / f64::from(ROW_HEIGHT)) as usize;
                self.remove_at(host, row);
            }
            POINTER_DOWN => self.grab = Some(at),
            POINTER_UP => {
                let grab = self.grab.take()?;
                let dx = whole_pixels(at.0 - grab.0);
                let dy = whole_pixels(at.1 - grab.1);
                if (dx != 0 || dy != 0)
                    && let Some(canvas) = self.canvas
                {
                    self.x = self.x.saturating_add(dx);
                    self.y = self.y.saturating_add(dy);
                    ui.set_canvas_offset(canvas, self.offset());
                    self.save(host);
                }
            }
            POINTER_CANCELLED => self.grab = None,
            _ => {}
        }
        None
    }

    fn offset(&self) -> (f64, f64) {
        (f64::from(self.x), f64::from(self.y))
    }

    /// Binds the logged-in character's list when the character is a new
    /// one; out of the world, the list stays as it is.
    fn bind(&mut self, host: &mut dyn UbHost, ui: &mut dyn MapUi) {
        let Some(character) = host.character() else {
            return;
        };
        let (world, name) = (character.world.trim(), character.name.trim());
        if world.is_empty() || name.is_empty() {
            return;
        }
        let scope = scope_of(world, name);
        if self.scope.as_deref() == Some(scope.as_str()) {
            return;
        }
        self.key = Some(key_for(world, name));
        self.scope = Some(scope);
        self.entries.clear();
        self.counts.clear();
        self.x = DEFAULT_X;
        self.y = DEFAULT_Y;
        self.removing = false;
        self.dirty = true;
        self.refresh_remaining = 0.0;
        self.load(host.storage());
        if let Some(canvas) = self.canvas {
            ui.set_canvas_offset(canvas, self.offset());
        }
    }

    /// Reads the character's list and place; a file that does not read
    /// starts an empty list at the default place, and the next change
    /// rewrites it.
    fn load(&mut self, storage: &mut dyn PluginStorage) {
        let Some(key) = &self.key else {
            return;
        };
        if !storage.is_available() {
            return;
        }
        let Some(text) = storage.read_text(key).ok().flatten() else {
            return;
        };
        let Ok(Some(document)) = read_document(&text) else {
            return;
        };
        self.x = document.x;
        self.y = document.y;
        for entry in document.entries {
            if !entry.name.is_empty() && self.entries.iter().all(|held| held.name != entry.name) {
                self.entries.push(entry);
            }
        }
    }

    fn save(&self, host: &mut dyn UbHost) {
        let Some(key) = &self.key else {
            return;
        };
        let storage = host.storage();
        if !storage.is_available() {
            return;
        }
        let text = write_document(self.x, self.y, &self.entries);
        if let Err(error) = storage.write_text(key, &text) {
            host.log(
                LogLevel::Warn,
                &format!("The Comps HUD could not save its list: {error}"),
            );
        }
    }

    fn changed(&mut self, host: &mut dyn UbHost) {
        self.save(host);
        self.refresh_remaining = 0.0;
        self.dirty = true;
    }

    fn cancel_removal(&mut self) {
        if !self.removing {
            return;
        }
        self.removing = false;
        self.dirty = true;
    }

    /// Takes the counts again and steps the removal: it lapses after 30
    /// seconds without a click, and flips between its two reds otherwise.
    fn refresh(&mut self, host: &mut dyn UbHost) {
        if self.removing && self.removal_elapsed > REMOVAL_TIMEOUT_SECONDS {
            self.removing = false;
            self.dirty = true;
        }
        if self.removing {
            self.flash_bright = !self.flash_bright;
            self.dirty = true;
        }
        let objects = host.objects();
        let counts: Vec<i32> = self
            .entries
            .iter()
            .map(|entry| count_owned(&objects, &entry.name))
            .collect();
        if counts != self.counts {
            self.counts = counts;
            self.dirty = true;
        }
    }

    /// Registers the canvas at the list's height; the host sizes a canvas
    /// once, so a list that grew or shrank takes a new one in its place.
    fn register(&mut self, ui: &mut dyn MapUi) {
        if let Some(canvas) = self.canvas.take() {
            ui.remove_canvas(canvas);
        }
        self.canvas_height = self.canvas_height();
        self.canvas = ui.register_canvas(&MapCanvasSpec {
            canvas_id: CANVAS_ID.to_owned(),
            width: HUD_WIDTH,
            height: self.canvas_height,
            offset: self.offset(),
            start_visible: false,
            accepts_pointer_input: true,
        });
        self.canvas_visible = false;
        self.dirty = true;
    }

    fn icon(&mut self, ui: &mut dyn MapUi, icon_id: u32) -> MapImage {
        if icon_id == 0 {
            return MapImage::default();
        }
        if let Some(held) = self.icons.get(&icon_id)
            && held.is_valid()
        {
            return *held;
        }
        let icon = ui.client_art(icon_id);
        if icon.is_valid() && self.icons.insert(icon_id, icon).is_none() {
            self.icon_order.push(icon_id);
        }
        icon
    }

    /// The title strip with its two buttons, then each row's icon and
    /// count.
    pub fn paint(&mut self, painter: &mut dyn MapPainter, ui: &mut dyn MapUi) {
        painter.clear(&rgba(0, 0, 0, 0));
        let height = f64::from(self.canvas_height());
        let width = f64::from(HUD_WIDTH);
        painter.fill_rect((0.0, 0.0, width, height), &BACK);
        painter.fill_rect((0.0, 0.0, width, f64::from(HEADER_HEIGHT)), &HEADER_BACK);
        paint_button(painter, MINUS_BUTTON, false, self.removing);
        paint_button(painter, PLUS_BUTTON, true, false);

        let color = self.count_color();
        let mut top = f64::from(HEADER_HEIGHT);
        for index in 0..self.entries.len() {
            let icon = self.icon(ui, self.entries[index].icon_id);
            if icon.is_valid() {
                painter.draw_image(
                    icon,
                    (0.0, top, f64::from(ICON_SIZE), f64::from(ICON_SIZE)),
                    &WHITE,
                );
            }
            let count = self.counts.get(index).copied().unwrap_or(0).to_string();
            let (_, text_height) = painter.measure_text(&count);
            painter.draw_text(
                &count,
                (
                    f64::from(TEXT_X),
                    top + ((f64::from(ROW_HEIGHT) - text_height) / 2.0),
                ),
                &color,
                true,
            );
            top += f64::from(ROW_HEIGHT);
        }
        painter.stroke_rect((0.0, 0.0, width, height), &EDGE, 1.0);
    }
}

fn paint_button(painter: &mut dyn MapPainter, button: HudRect, plus: bool, lit: bool) {
    let (x, y, width, height) = button;
    painter.fill_rect(button, if lit { &REMOVAL_DARK } else { &BUTTON_BACK });
    painter.stroke_rect(button, &EDGE, 1.0);
    let cx = x + (width / 2.0);
    let cy = y + (height / 2.0);
    let arm = (width / 2.0) - 3.0;
    painter.draw_line((cx - arm, cy), (cx + arm, cy), &TEXT, 2.0);
    if plus {
        painter.draw_line((cx, cy - arm), (cx, cy + arm), &TEXT, 2.0);
    }
}

fn contains(rect: HudRect, point: (f64, f64)) -> bool {
    let (x, y, width, height) = rect;
    point.0 >= x && point.0 < x + width && point.1 >= y && point.1 < y + height
}

/// A drag's distance as whole pixels, halves away from zero.
#[allow(clippy::cast_possible_truncation)]
fn whole_pixels(value: f64) -> i32 {
    value
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

fn scope_of(world: &str, character: &str) -> String {
    format!(
        "{}\0{}",
        casing::to_upper_invariant(world.trim()),
        casing::to_upper_invariant(character.trim())
    )
}

/// The storage key a character's list is kept under.
#[must_use]
pub fn key_for(world: &str, character: &str) -> String {
    format!(
        "{CHARACTER_PREFIX}{}.json",
        sha256_hex(&scope_of(world, character))
    )
}

/// How many of an item the character carries: every owned object of that
/// exact name, adding its stack size, or one for an object that has none.
#[must_use]
pub fn count_owned(objects: &[WorldObject], name: &str) -> i32 {
    objects
        .iter()
        .filter(|item| item.is_owned && item.name == name)
        .fold(0i32, |count, item| {
            count.wrapping_add(item.stack_size.max(1))
        })
}

/// A character's list as it is kept.
struct Document {
    x: i32,
    y: i32,
    entries: Vec<ComponentHudEntry>,
}

/// The document `{ "X", "Y", "Entries": [{ "Name", "IconId" }] }`, names
/// matched exactly; `None` for a JSON `null`. A missing entry list is
/// none, a `null` entry is skipped, and a missing name is no name.
fn read_document(text: &str) -> Result<Option<Document>, JsonError> {
    let document = json::parse(text).map_err(|error| error.to_string())?;
    let Some(o) = Object::of(&document, "$", "Document")? else {
        return Ok(None);
    };
    let o = o.case_sensitive();
    let rows = o
        .objects("Entries", None, "ComponentHudEntry")?
        .unwrap_or_default();
    let mut entries = Vec::with_capacity(rows.len());
    for row in rows.into_iter().flatten() {
        let row = row.case_sensitive();
        entries.push(ComponentHudEntry {
            name: row.string("Name", "")?.unwrap_or_default(),
            icon_id: row.uint("IconId", 0)?,
        });
    }
    Ok(Some(Document {
        x: o.int("X", DEFAULT_X)?,
        y: o.int("Y", DEFAULT_Y)?,
        entries,
    }))
}

fn write_document(x: i32, y: i32, entries: &[ComponentHudEntry]) -> String {
    stj::write(&JsonValue::object([
        ("X", stj::int(x)),
        ("Y", stj::int(y)),
        (
            "Entries",
            JsonValue::array(entries.iter().map(|entry| {
                JsonValue::object([
                    ("Name", stj::string(&entry.name)),
                    ("IconId", stj::uint(entry.icon_id)),
                ])
            })),
        ),
    ]))
}

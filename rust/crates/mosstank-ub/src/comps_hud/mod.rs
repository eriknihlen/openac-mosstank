//! The Comps HUD (`ComponentHudController`): a small strip that counts the
//! items the player chose, each its icon and how many of it the character
//! carries, re-counted every 600 ms. The idea is Virindi HUDs' comps
//! counter beside VTank; its look and editing are MossTank's own.
//!
//! The strip is a canvas the host paints under every window, drawn in the
//! plugin theme's colours, font and corners ([`style`]), as a column or a
//! line ([`layout`]), each count given the room it takes in that font.
//! Three small buttons sit on it: the layout switch, a pencil that opens
//! the Edit items window, and a cross that hides the HUD. A left drag
//! anywhere else drags the strip, which follows the pointer as it goes and
//! is kept where it is let go. A count below its item's warn level is drawn in
//! the warning colour. The pointer over an item names it, and over a
//! button says what it does, in a tooltip ([`tip`]).
//!
//! Items are added and taken off, put in order and given warn levels in the
//! Edit items window, whose state is kept here ([`CarriedKind`], the search,
//! the selected rows), or with `/vt compshud`. The list (in order, with the
//! warn levels), the layout and the strip's place are kept for each
//! character ([`document`]); whether the HUD is on is kept once for every
//! character. A kept list that does not read whole is copied aside before
//! it is written again, and not written over when the copy fails; one
//! whose file cannot be read is not written over either.
//!
//! A count is every object the character owns, in a pack or worn, whose
//! name is the listed name exactly, adding its stack size, or one for an
//! object that has none ([`carried::count_owned`]).

pub mod carried;
mod document;
mod editor;
pub mod layout;
mod paint;
pub mod style;
pub mod tip;

use std::collections::HashMap;

use ac_plugin_abi::ui::{PointerEvent, ThemeInfo};
use mosstank_dotnet::text::{casing, ordinal};
use mosstank_meta::host::LogLevel;
use mosstank_stores::recovery::{self, sha256_hex};
use mosstank_stores::storage::{PluginStorage, Root, StoreHost, StoreLog};

pub use carried::{CarriedItem, CarriedKind, count_owned};
pub use layout::{HudButton, HudGeometry, HudLayout, HudRect};
pub use style::HudStyle;
pub use tip::Tip;

use crate::canvas::{
    BUTTON_LEFT, MapCanvasSpec, MapFont, MapImage, MapPainter, MapUi, POINTER_CANCELLED,
    POINTER_DOWN, POINTER_HOVER, POINTER_LEAVE, POINTER_MOVE, POINTER_UP,
};
use crate::host::UbHost;
use document::Document;
use editor::Editor;
use paint::ItemPaint;

/// The canvas's id.
pub const CANVAS_ID: &str = "comps-hud";
/// The place a character's HUD starts at, pixels from the screen's left.
pub const DEFAULT_X: i32 = 200;
/// The place a character's HUD starts at, pixels from the screen's top.
pub const DEFAULT_Y: i32 = 150;
/// How often the counts are taken again.
pub const REFRESH_SECONDS: f64 = 0.6;

/// Where the on switch is kept, for every character.
pub const ENABLED_KEY: &str = "comps-hud/enabled.txt";
/// Where each character's list is kept.
pub const CHARACTER_PREFIX: &str = "comps-hud/characters/";
/// The family a list that did not read is kept aside under
/// (`recovery/comps-hud/...`).
pub const RECOVERY_FAMILY: &str = "comps-hud";
/// The characters a count is drawn with: the ten digits and the minus
/// sign (a count below zero, or an empty list's dash).
const COUNT_CHARACTERS: [char; 11] = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '-'];
/// The farthest from the screen's top-left a kept place may stand: a far
/// larger one (a damaged file) comes back where it can be seen.
pub const MAX_PLACE: i32 = 16_384;

/// One item the HUD counts: the name it is counted by, the icon it is shown
/// with, and the count below which it warns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentHudEntry {
    /// The name, matched exactly.
    pub name: String,
    /// The icon (a dat id), 0 for none yet.
    pub icon_id: u32,
    /// The count is drawn in the warning colour below this; 0 never warns.
    pub warn: i32,
}

/// Why an item was not added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddRefusal {
    /// No character is in the world for the list to be its.
    NotInWorld,
    /// Nothing is selected, or the client does not know the selection.
    NothingSelected,
    /// The item has no name to count by.
    NoName,
    /// The name is listed already.
    Listed(String),
}

/// What a left press on the strip took hold of.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Press {
    /// A button, which acts when the press is let go over it.
    Button(HudButton),
    /// The strip, to drag: the point of the canvas the press took hold of
    /// (kept under the pointer as the strip follows it), and where the
    /// strip stood when the press began.
    Strip { grab: (f64, f64), from: (i32, i32) },
}

/// The HUD.
#[derive(Debug)]
pub struct ComponentHudController {
    /// The bare icons (by icon id), for items the character carries none of.
    icons: HashMap<u32, MapImage>,
    icon_order: Vec<u32>,
    /// The composed icons (by object id) of the objects drawn for the
    /// listed items.
    object_icons: HashMap<u32, MapImage>,
    /// For each listed name the character carries, the object its icon is
    /// drawn from: the first the recount finds, kept while it stays.
    drawn_objects: HashMap<String, u32>,
    style: HudStyle,
    /// The bundled font, once asked for (none: refused, or not asked yet).
    font: MapFont,
    font_asked: bool,
    /// Each of [`COUNT_CHARACTERS`]' width in the font in use, measured once
    /// per font: a count's width is the sum of its characters'.
    character_widths: Option<[f64; 11]>,
    entries: Vec<ComponentHudEntry>,
    counts: Vec<i32>,
    /// The strip laid out for the list, layout and widths as they stand;
    /// none once one of them changes.
    geometry: Option<HudGeometry>,
    /// What the character carries, taken with the counts while the Edit
    /// items window is open.
    carried: Vec<CarriedItem>,
    editor: Editor,
    layout: HudLayout,
    enabled: bool,
    scope: Option<String>,
    key: Option<String>,
    /// The character's file could not be read: it is not written over.
    save_blocked: bool,
    x: i32,
    y: i32,
    canvas: Option<u32>,
    canvas_visible: bool,
    canvas_size: (i32, i32),
    /// Where the canvas was last put.
    placed: (i32, i32),
    tip: Option<Tip>,
    tip_canvas: Option<u32>,
    tip_shown: bool,
    refresh_remaining: f64,
    dirty: bool,
    tip_dirty: bool,
    press: Option<Press>,
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
            object_icons: HashMap::new(),
            drawn_objects: HashMap::new(),
            style: HudStyle::classic(),
            font: MapFont::default(),
            font_asked: false,
            character_widths: None,
            entries: Vec::new(),
            counts: Vec::new(),
            geometry: None,
            carried: Vec::new(),
            editor: Editor::default(),
            layout: HudLayout::Column,
            enabled,
            scope: None,
            key: None,
            save_blocked: false,
            x: DEFAULT_X,
            y: DEFAULT_Y,
            canvas: None,
            canvas_visible: false,
            canvas_size: (0, 0),
            placed: (DEFAULT_X, DEFAULT_Y),
            tip: None,
            tip_canvas: None,
            tip_shown: false,
            refresh_remaining: 0.0,
            dirty: true,
            tip_dirty: false,
            press: None,
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
        self.refresh_remaining = 0.0;
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

    /// The listed items, in order.
    #[must_use]
    pub fn entries(&self) -> &[ComponentHudEntry] {
        &self.entries
    }

    /// The counts as last taken, one for each entry.
    #[must_use]
    pub fn counts(&self) -> &[i32] {
        &self.counts
    }

    /// Whether entry `index`'s count is below its warn level now.
    #[must_use]
    pub fn warns(&self, index: usize) -> bool {
        let Some(entry) = self.entries.get(index) else {
            return false;
        };
        entry.warn > 0 && self.counts.get(index).copied().unwrap_or(0) < entry.warn
    }

    /// The layout, the character's.
    #[must_use]
    pub fn layout(&self) -> HudLayout {
        self.layout
    }

    /// Lays the strip out as a column or a line, and keeps that for the
    /// character; false out of the world, where there is no character to
    /// keep it for.
    pub fn set_layout(&mut self, host: &mut dyn UbHost, layout: HudLayout) -> bool {
        if self.key.is_none() {
            return false;
        }
        if self.layout != layout {
            self.layout = layout;
            self.reshape();
            self.hide_tip();
            self.save(host);
        }
        true
    }

    /// Where the HUD stands, pixels from the screen's top-left.
    #[must_use]
    pub fn position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    /// Puts the strip back at its first place, and keeps that for the
    /// character; false out of the world.
    pub fn reset_place(&mut self, host: &mut dyn UbHost) -> bool {
        if self.key.is_none() {
            return false;
        }
        self.hide_tip();
        self.place(DEFAULT_X, DEFAULT_Y);
        self.save(host);
        true
    }

    /// The canvas, once registered.
    #[must_use]
    pub fn canvas(&self) -> Option<u32> {
        self.canvas
    }

    /// The tooltip's canvas, once registered.
    #[must_use]
    pub fn tip_canvas(&self) -> Option<u32> {
        self.tip_canvas
    }

    /// The tooltip shown now, if any.
    #[must_use]
    pub fn tip(&self) -> Option<&Tip> {
        self.tip.as_ref()
    }

    /// The look the strip is painted in now.
    #[must_use]
    pub fn style(&self) -> &HudStyle {
        &self.style
    }

    /// The plugin theme the player picked (at the start, and at each
    /// change): the strip is painted in it from the next frame.
    pub fn set_theme(&mut self, theme: &ThemeInfo) {
        let style = HudStyle::from_theme(theme);
        if style != self.style {
            // Another font: its characters are measured again.
            if style.themed != self.style.themed {
                self.character_widths = None;
            }
            self.style = style;
            self.reshape();
            self.tip_dirty = true;
        }
    }

    /// The strip laid out for the list as it stands (worked out again only
    /// after the counts, the list, the layout, the measured widths or the
    /// theme change).
    pub fn geometry(&mut self) -> &HudGeometry {
        if self.geometry.is_none() {
            let widths = self.count_widths();
            self.geometry = Some(HudGeometry::new(self.layout, &widths));
        }
        self.geometry
            .get_or_insert_with(|| HudGeometry::new(HudLayout::Column, &[]))
    }

    fn count_texts(&self) -> Vec<String> {
        (0..self.entries.len())
            .map(|index| self.counts.get(index).copied().unwrap_or(0).to_string())
            .collect()
    }

    /// Each count's width: the sum of its characters' widths in the font in
    /// use, or estimated until they are measured.
    fn count_widths(&self) -> Vec<f64> {
        self.count_texts()
            .iter()
            .map(|text| self.text_width(text))
            .collect()
    }

    fn text_width(&self, text: &str) -> f64 {
        let Some(widths) = &self.character_widths else {
            return layout::estimated_width(text);
        };
        text.chars()
            .map(|character| {
                COUNT_CHARACTERS
                    .iter()
                    .position(|known| *known == character)
                    .map_or(f64::from(layout::DIGIT_WIDTH), |index| widths[index])
            })
            .sum()
    }

    /// Measures the characters counts are drawn with, once per font, so a
    /// count of any size is laid out right the first time (and the canvas
    /// sized once). Called before each frame with a view that can ask the
    /// client; a view that cannot leaves the estimate.
    pub fn prepare_metrics(&mut self, ui: &mut dyn MapUi) {
        if self.character_widths.is_some() || !self.enabled {
            return;
        }
        let font = self.font(ui);
        let mut widths = [0.0; 11];
        for (width, character) in widths.iter_mut().zip(COUNT_CHARACTERS) {
            let Some((measured, _)) = ui.measure_text_in(font, character.encode_utf8(&mut [0; 4]))
            else {
                return;
            };
            *width = measured;
        }
        self.character_widths = Some(widths);
        self.reshape();
    }

    /// The strip's shape may have changed: lay it out again and repaint.
    fn reshape(&mut self) {
        self.geometry = None;
        self.dirty = true;
    }

    /// The list in a line for the Consumables page: how many items and
    /// their names in order ("3 items: Gold Pea, Platinum Scarab, ...").
    #[must_use]
    pub fn summary(&self) -> String {
        if self.entries.is_empty() {
            return "No items yet.".to_owned();
        }
        let names: Vec<&str> = self
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        let noun = if names.len() == 1 { "item" } else { "items" };
        format!("{} {noun}: {}", names.len(), names.join(", "))
    }

    /// The frame: bind the logged-in character's list, take the counts
    /// every 600 ms while the HUD or the Edit items window shows them, and
    /// keep the canvases sized to the strip, in place, and shown while the
    /// HUD is on.
    pub fn on_tick(&mut self, host: &mut dyn UbHost, ui: &mut dyn MapUi, elapsed_seconds: f64) {
        let elapsed = elapsed_seconds.max(0.0);
        self.bind(host);
        let visible = self.enabled && self.key.is_some();
        if visible || (self.editor.visible && self.key.is_some()) {
            self.refresh_remaining -= elapsed;
            if self.refresh_remaining <= 0.0 {
                self.refresh_remaining = REFRESH_SECONDS;
                self.refresh(host);
            }
        }
        // A new size waits for a press to end: the canvas taking a press
        // goes with its replacement, and its release with it.
        if visible
            && (self.canvas.is_none()
                || (self.press.is_none() && self.geometry().size() != self.canvas_size))
        {
            self.register(ui);
        }
        if visible && self.tip_canvas.is_none() {
            self.tip_canvas = ui.register_canvas(&MapCanvasSpec {
                canvas_id: tip::TIP_CANVAS_ID.to_owned(),
                width: tip::TIP_WIDTH,
                height: tip::TIP_HEIGHT,
                offset: (0.0, 0.0),
                start_visible: false,
                accepts_pointer_input: false,
            });
        }
        if !visible {
            self.hide_tip();
        }
        self.show_tip(ui);
        self.place_canvas(ui);
        let Some(canvas) = self.canvas else {
            return;
        };
        if self.canvas_visible != visible {
            self.canvas_visible = visible;
            ui.set_canvas_visible(canvas, visible);
            self.dirty = true;
        }
        if !visible || !self.dirty {
            return;
        }
        self.dirty = false;
        ui.invalidate_canvas(canvas);
    }

    /// Puts the tooltip's canvas where the tip is, shown while there is
    /// one, and asks for its paint when it changed.
    fn show_tip(&mut self, ui: &mut dyn MapUi) {
        let Some(canvas) = self.tip_canvas else {
            return;
        };
        let shown = self.tip.is_some();
        if let Some(tip) = &self.tip
            && self.tip_dirty
        {
            ui.set_canvas_offset(canvas, (f64::from(tip.offset.0), f64::from(tip.offset.1)));
            ui.invalidate_canvas(canvas);
        }
        self.tip_dirty = false;
        if self.tip_shown != shown {
            self.tip_shown = shown;
            ui.set_canvas_visible(canvas, shown);
        }
    }

    fn hide_tip(&mut self) {
        if self.tip.take().is_some() {
            self.tip_dirty = true;
        }
    }

    /// Adds the object selected in the game, by its name and icon: the name
    /// added.
    ///
    /// # Errors
    /// Why nothing was added.
    pub fn add_selection(&mut self, host: &mut dyn UbHost) -> Result<String, AddRefusal> {
        if self.key.is_none() {
            return Err(AddRefusal::NotInWorld);
        }
        let item = host
            .selection()
            .selected
            .and_then(|selected| host.object(selected))
            .ok_or(AddRefusal::NothingSelected)?;
        self.add(host, &item.name, item.icon_id)?;
        Ok(item.name)
    }

    /// Adds an item by name, with its icon (0: found among what the
    /// character carries when it is counted).
    ///
    /// # Errors
    /// Why nothing was added.
    pub fn add(
        &mut self,
        host: &mut dyn UbHost,
        name: &str,
        icon_id: u32,
    ) -> Result<(), AddRefusal> {
        if self.key.is_none() {
            return Err(AddRefusal::NotInWorld);
        }
        if name.is_empty() {
            return Err(AddRefusal::NoName);
        }
        if self.entries.iter().any(|entry| entry.name == name) {
            return Err(AddRefusal::Listed(name.to_owned()));
        }
        self.entries.push(ComponentHudEntry {
            name: name.to_owned(),
            icon_id,
            warn: 0,
        });
        self.changed(host);
        Ok(())
    }

    /// Takes one entry off the list; false when there is no such row.
    pub fn remove_at(&mut self, host: &mut dyn UbHost, index: usize) -> bool {
        if index >= self.entries.len() {
            return false;
        }
        self.entries.remove(index);
        if index < self.counts.len() {
            self.counts.remove(index);
        }
        self.editor.selected_entry = clamp(self.editor.selected_entry, self.entries.len());
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

    /// Swaps entry `index` with the one before it (`up`) or after it; the
    /// selection follows the item. False at either end.
    pub fn move_entry(&mut self, host: &mut dyn UbHost, index: usize, up: bool) -> bool {
        let other = if up {
            index.checked_sub(1)
        } else {
            index.checked_add(1)
        };
        let Some(other) = other.filter(|other| *other < self.entries.len()) else {
            return false;
        };
        if index >= self.entries.len() {
            return false;
        }
        self.entries.swap(index, other);
        if index < self.counts.len() && other < self.counts.len() {
            self.counts.swap(index, other);
        }
        if self.editor.selected_entry == index {
            self.editor.selected_entry = other;
        }
        self.changed(host);
        true
    }

    /// Sets entry `index`'s warn level (0 or less: never warns).
    pub fn set_warn(&mut self, host: &mut dyn UbHost, index: usize, warn: i32) -> bool {
        let warn = warn.max(0);
        let Some(entry) = self.entries.get_mut(index) else {
            return false;
        };
        if entry.warn == warn {
            return true;
        }
        entry.warn = warn;
        self.dirty = true;
        self.save(host);
        true
    }

    /// The session ended: the icons and the font are let go, the Edit items
    /// window closes, and the next login binds its own list.
    pub fn reset(&mut self, ui: &mut dyn MapUi) {
        for id in self.icon_order.drain(..) {
            if let Some(icon) = self.icons.remove(&id) {
                ui.release_image(icon);
            }
        }
        self.icons.clear();
        for (_, icon) in self.object_icons.drain() {
            ui.release_image(icon);
        }
        self.drawn_objects.clear();
        if self.font.is_valid() {
            ui.release_font(self.font);
        }
        self.font = MapFont::default();
        self.font_asked = false;
        self.character_widths = None;
        self.press = None;
        self.scope = None;
        self.key = None;
        self.save_blocked = false;
        self.entries.clear();
        self.counts.clear();
        self.hide_editor();
        self.hide_tip();
        self.show_tip(ui);
        self.reshape();
        if let Some(canvas) = self.canvas {
            self.canvas_visible = false;
            ui.set_canvas_visible(canvas, false);
        }
    }

    /// Lets go of the icons, the font and the canvases.
    pub fn dispose(&mut self, ui: &mut dyn MapUi) {
        for id in self.icon_order.drain(..) {
            if let Some(icon) = self.icons.remove(&id) {
                ui.release_image(icon);
            }
        }
        self.icons.clear();
        for (_, icon) in self.object_icons.drain() {
            ui.release_image(icon);
        }
        self.drawn_objects.clear();
        if self.font.is_valid() {
            ui.release_font(self.font);
        }
        self.font = MapFont::default();
        if let Some(canvas) = self.canvas.take() {
            ui.remove_canvas(canvas);
        }
        if let Some(canvas) = self.tip_canvas.take() {
            ui.remove_canvas(canvas);
        }
    }

    /// The pointer: over the strip with no button held, the tooltip names
    /// what is under it, and leaving the strip clears it; a left press let
    /// go over the button it went down on is that button (the layout
    /// switch, Edit items, hide); a left press anywhere else drags the
    /// strip: it follows the pointer while the button is held, the grabbed
    /// point staying under it, and its place is kept once, when the button
    /// comes up. The host captures the pointer for the press and reports
    /// each move from where the plugin last put the canvas, so the strip
    /// neither loses the pointer nor counts a step twice.
    pub fn on_pointer(&mut self, host: &mut dyn UbHost, ui: &mut dyn MapUi, event: &PointerEvent) {
        let at = (f64::from(event.x), f64::from(event.y));
        match event.kind {
            POINTER_HOVER => self.hover(at),
            POINTER_LEAVE => self.hide_tip(),
            _ if event.button != BUTTON_LEFT => {}
            POINTER_DOWN => {
                self.hide_tip();
                self.press = Some(match self.geometry().button_at(at) {
                    Some(button) => Press::Button(button),
                    None => Press::Strip {
                        grab: at,
                        from: (self.x, self.y),
                    },
                });
            }
            POINTER_MOVE => {
                if let Some(Press::Strip { grab, .. }) = self.press {
                    self.follow(grab, at);
                }
            }
            POINTER_UP => match self.press.take() {
                Some(Press::Button(button)) if self.geometry().button_at(at) == Some(button) => {
                    self.press_button(host, button);
                }
                Some(Press::Strip { grab, from }) => {
                    self.follow(grab, at);
                    self.keep_place(host, from);
                }
                _ => {}
            },
            POINTER_CANCELLED => {
                if let Some(Press::Strip { from, .. }) = self.press.take() {
                    self.keep_place(host, from);
                }
            }
            _ => {}
        }
        self.place_canvas(ui);
        self.show_tip(ui);
    }

    /// Moves the canvas to where the strip stands, when it is elsewhere.
    fn place_canvas(&mut self, ui: &mut dyn MapUi) {
        if let Some(canvas) = self.canvas
            && self.placed != (self.x, self.y)
        {
            self.placed = (self.x, self.y);
            ui.set_canvas_offset(canvas, self.offset());
        }
    }

    /// The pointer is over the strip at `at`: the tip for what is there,
    /// or none.
    fn hover(&mut self, at: (f64, f64)) {
        let strip = (self.x, self.y);
        let layout = self.layout;
        let geometry = self.geometry().clone();
        let target = if let Some(button) = geometry.button_at(at) {
            Some((
                tip::button_text(button, layout).to_owned(),
                geometry.button(button),
            ))
        } else {
            geometry.item_at(at).and_then(|index| {
                self.entries
                    .get(index)
                    .map(|entry| (entry.name.clone(), geometry.items[index].cell))
            })
        };
        let tip = target.map(|(text, rect)| tip::place(strip, &geometry, rect, &text));
        if tip != self.tip {
            self.tip = tip;
            self.tip_dirty = true;
        }
    }

    /// What a button does.
    pub fn press_button(&mut self, host: &mut dyn UbHost, button: HudButton) {
        match button {
            HudButton::Layout => {
                self.set_layout(host, self.layout.other());
            }
            HudButton::Edit => self.show_editor(host),
            HudButton::Hide => self.set_enabled(host, false),
        }
    }

    /// Moves the strip so the grabbed point is under the pointer at `at`
    /// (canvas pixels from where the canvas was last put); nothing is saved.
    fn follow(&mut self, grab: (f64, f64), at: (f64, f64)) {
        let dx = whole_pixels(at.0 - grab.0);
        let dy = whole_pixels(at.1 - grab.1);
        if (dx == 0 && dy == 0) || self.canvas.is_none() {
            return;
        }
        self.place(self.x.saturating_add(dx), self.y.saturating_add(dy));
    }

    /// The drag ended: its place is kept, once, when the strip moved.
    fn keep_place(&mut self, host: &mut dyn UbHost, from: (i32, i32)) {
        if (self.x, self.y) != from {
            self.save(host);
        }
    }

    /// Stands the strip at a place, never past the screen's top or left
    /// edge, nor farther off than [`MAX_PLACE`] (a plugin cannot learn the
    /// screen's size, so not exactly its right or bottom one).
    fn place(&mut self, x: i32, y: i32) {
        self.x = x.clamp(0, MAX_PLACE);
        self.y = y.clamp(0, MAX_PLACE);
    }

    fn offset(&self) -> (f64, f64) {
        (f64::from(self.x), f64::from(self.y))
    }

    /// Binds the logged-in character's list when the character is a new
    /// one; out of the world, the list stays as it is.
    fn bind(&mut self, host: &mut dyn UbHost) {
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
        self.place(DEFAULT_X, DEFAULT_Y);
        self.layout = HudLayout::Column;
        self.save_blocked = false;
        self.editor.selected_entry = 0;
        self.hide_tip();
        self.reshape();
        self.refresh_remaining = 0.0;
        self.load(host);
    }

    /// Reads the character's list, layout and place. No file starts an
    /// empty column at the default place. A file that does not read whole
    /// gives what it can, is copied aside (`recovery/comps-hud/...`) and
    /// said in the log, and the next change writes the list over it; when
    /// the copy cannot be made, nothing is saved over it this session. A
    /// file the storage cannot read is said in the log and left alone the
    /// same way.
    fn load(&mut self, host: &mut dyn UbHost) {
        let Some(key) = self.key.clone() else {
            return;
        };
        let read = {
            let storage = host.storage();
            if !storage.is_available() {
                return;
            }
            storage.read_text(&key)
        };
        let text = match read {
            Ok(Some(text)) => text,
            Ok(None) => return,
            Err(error) => {
                self.save_blocked = true;
                host.log(
                    LogLevel::Warn,
                    &format!(
                        "The Comps HUD could not read this character's list ({error}); it is left as it is and not saved over until the next login."
                    ),
                );
                return;
            }
        };
        let (document, problem) = match document::read(&text) {
            Ok(read) if read.problems.is_empty() => (Some(read.document), None),
            Ok(read) => (Some(read.document), Some(read.problems.join("; "))),
            Err(error) => (None, Some(error)),
        };
        if let Some(problem) = problem {
            let preserved = recovery::preserve_reporting(
                &mut HudStores(&mut *host),
                RECOVERY_FAMILY,
                &key,
                Some(&text),
                &problem,
            );
            host.log(LogLevel::Warn, &format!("Comps HUD: {}", preserved.notice));
            // No copy aside: the original is the only one, and stays.
            if !preserved.kept {
                self.save_blocked = true;
                host.log(
                    LogLevel::Warn,
                    "The Comps HUD could not keep a copy of this character's list, so it is not saved over until the next login.",
                );
            }
        }
        let Some(document) = document else {
            return;
        };
        self.place(document.x, document.y);
        self.layout = document.layout;
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
        if self.save_blocked {
            return;
        }
        let storage = host.storage();
        if !storage.is_available() {
            return;
        }
        let text = document::write(&Document {
            x: self.x,
            y: self.y,
            layout: self.layout,
            entries: self.entries.clone(),
        });
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
        self.reshape();
    }

    /// Takes the counts again (and, while the Edit items window is open,
    /// what the character carries), and gives an item listed without an
    /// icon the icon of the first such object the character owns.
    fn refresh(&mut self, host: &mut dyn UbHost) {
        let objects = host.objects();
        let counts: Vec<i32> = self
            .entries
            .iter()
            .map(|entry| count_owned(&objects, &entry.name))
            .collect();
        if counts != self.counts {
            self.counts = counts;
            self.reshape();
        }
        self.choose_drawn_objects(&objects);
        let mut found_icon = false;
        for entry in self.entries.iter_mut().filter(|entry| entry.icon_id == 0) {
            if let Some(icon_id) = objects
                .iter()
                .find(|item| item.is_owned && item.name == entry.name && item.icon_id != 0)
                .map(|item| item.icon_id)
            {
                entry.icon_id = icon_id;
                found_icon = true;
            }
        }
        if found_icon {
            self.dirty = true;
            self.save(host);
        }
        if self.editor.visible {
            self.carried = carried::gather(&objects);
        }
    }

    /// Registers the canvas at the strip's size; the host sizes a canvas
    /// once, so a strip that grew or shrank takes a new one in its place.
    fn register(&mut self, ui: &mut dyn MapUi) {
        // The old canvas's leave never comes once it is gone.
        self.hide_tip();
        if let Some(canvas) = self.canvas.take() {
            ui.remove_canvas(canvas);
        }
        self.canvas_size = self.geometry().size();
        self.canvas = ui.register_canvas(&MapCanvasSpec {
            canvas_id: CANVAS_ID.to_owned(),
            width: self.canvas_size.0,
            height: self.canvas_size.1,
            offset: self.offset(),
            start_visible: false,
            accepts_pointer_input: true,
        });
        self.placed = (self.x, self.y);
        self.canvas_visible = false;
        self.dirty = true;
    }

    /// For each listed name, the carried object its icon is drawn from: the
    /// one drawn before while the character still owns it under that name,
    /// else the first the recount finds; none when it carries none. The
    /// strip repaints when one changes.
    fn choose_drawn_objects(&mut self, objects: &[ac_plugin_abi::world::WorldObject]) {
        let mut chosen = HashMap::with_capacity(self.entries.len());
        for entry in &self.entries {
            let owned = |id: u32| {
                objects
                    .iter()
                    .any(|item| item.object_id == id && item.is_owned && item.name == entry.name)
            };
            let object = self
                .drawn_objects
                .get(&entry.name)
                .copied()
                .filter(|id| owned(*id))
                .or_else(|| {
                    objects
                        .iter()
                        .find(|item| item.is_owned && item.name == entry.name)
                        .map(|item| item.object_id)
                });
            if let Some(object) = object {
                chosen.insert(entry.name.clone(), object);
            }
        }
        if chosen != self.drawn_objects {
            self.drawn_objects = chosen;
            self.dirty = true;
        }
    }

    /// The object an entry's icon is drawn from, when the character
    /// carries one.
    #[must_use]
    pub fn drawn_object(&self, name: &str) -> Option<u32> {
        self.drawn_objects.get(name).copied()
    }

    /// An entry's icon: the composed icon (underlay, overlay and effects
    /// as the game draws it) of the object it is drawn from, or the bare
    /// icon when the character carries none.
    fn entry_icon(&mut self, ui: &mut dyn MapUi, index: usize) -> MapImage {
        let Some(entry) = self.entries.get(index) else {
            return MapImage::default();
        };
        let icon_id = entry.icon_id;
        let Some(object) = self.drawn_objects.get(&entry.name).copied() else {
            return self.icon(ui, icon_id);
        };
        if let Some(held) = self.object_icons.get(&object)
            && held.is_valid()
        {
            return *held;
        }
        let icon = ui.object_icon(object);
        if !icon.is_valid() {
            return self.icon(ui, icon_id);
        }
        self.object_icons.insert(object, icon);
        icon
    }

    /// Lets go of the composed icons of objects no longer drawn.
    fn release_stale_object_icons(&mut self, ui: &mut dyn MapUi) {
        let drawn: Vec<u32> = self.drawn_objects.values().copied().collect();
        let stale: Vec<u32> = self
            .object_icons
            .keys()
            .copied()
            .filter(|object| !drawn.contains(object))
            .collect();
        for object in stale {
            if let Some(icon) = self.object_icons.remove(&object) {
                ui.release_image(icon);
            }
        }
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

    /// The font the counts are drawn in: under a theme the bundled font,
    /// asked for once; under Classic, and when it is refused, the interface
    /// font.
    fn font(&mut self, ui: &mut dyn MapUi) -> MapFont {
        if !self.style.themed {
            return MapFont::default();
        }
        if !self.font_asked {
            self.font_asked = true;
            self.font = ui.bundled_font(style::FONT_SIZE);
        }
        self.font
    }

    /// The strip in the theme's look: the face, each item's icon and count,
    /// and the buttons.
    pub fn paint(&mut self, painter: &mut dyn MapPainter, ui: &mut dyn MapUi) {
        let font = self.font(ui);
        let texts = self.count_texts();
        self.release_stale_object_icons(ui);
        let icons: Vec<MapImage> = (0..self.entries.len())
            .map(|index| self.entry_icon(ui, index))
            .collect();
        let warns: Vec<bool> = (0..self.entries.len())
            .map(|index| self.warns(index))
            .collect();
        let geometry = self.geometry().clone();
        let items: Vec<ItemPaint<'_>> = texts
            .iter()
            .zip(icons)
            .zip(warns)
            .map(|((count, icon), warn)| ItemPaint { icon, count, warn })
            .collect();
        paint::paint(painter, &geometry, &self.style, font, &items);
    }

    /// The tooltip, in the theme's look.
    pub fn paint_tip(&mut self, painter: &mut dyn MapPainter, ui: &mut dyn MapUi) {
        let font = self.font(ui);
        let style = self.style.clone();
        match &self.tip {
            Some(tip) => tip::paint(painter, tip, &style, font),
            None => painter.clear(&ac_plugin_abi::ui::Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            }),
        }
    }
}

/// The profile stores' view of the UB host, for keeping a list that did
/// not read aside (`recovery::preserve`).
struct HudStores<'a>(&'a mut dyn UbHost);

impl StoreHost for HudStores<'_> {
    fn storage(&mut self, root: Root) -> &mut dyn PluginStorage {
        match root {
            Root::Plugin => self.0.storage(),
            Root::Profiles => self.0.profile_storage(),
        }
    }

    fn log(&mut self) -> &mut dyn StoreLog {
        self
    }

    fn world_name(&mut self) -> String {
        self.0
            .character()
            .map(|character| character.world)
            .unwrap_or_default()
    }
}

impl StoreLog for HudStores<'_> {
    fn info(&mut self, message: &str) {
        self.0.log(LogLevel::Info, message);
    }

    fn warn(&mut self, message: &str) {
        self.0.log(LogLevel::Warn, message);
    }

    fn error(&mut self, message: &str) {
        self.0.log(LogLevel::Error, message);
    }
}

fn clamp(row: usize, count: usize) -> usize {
    row.min(count.saturating_sub(1))
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

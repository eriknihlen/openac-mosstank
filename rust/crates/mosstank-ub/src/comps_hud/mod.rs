//! The Comps HUD (`ComponentHudController`): a small strip that counts the
//! items the player chose, each its icon and how many of it the character
//! carries, re-counted every 600 ms. The idea is Virindi HUDs' comps
//! counter beside VTank; its look and editing are MossTank's own.
//!
//! The strip is a canvas the host paints under every window, drawn in the
//! plugin theme's colours, font and corners ([`style`]), as a column or a
//! line ([`layout`]). Three small buttons sit on it: the layout switch, a
//! pencil that opens the Edit items window, and a cross that hides the HUD.
//! A left drag anywhere else moves the strip; it moves when the button
//! comes up, as the clients readout does. A count below its item's warn
//! level is drawn in the warning colour.
//!
//! Items are added and taken off, put in order and given warn levels in the
//! Edit items window, whose state is kept here ([`CarriedKind`], the search,
//! the selected rows), or with `/vt compshud`. The list (in order, with the
//! warn levels), the layout and the strip's place are kept for each
//! character ([`document`]); whether the HUD is on is kept once for every
//! character.
//!
//! A count is every object the character owns, in a pack or worn, whose
//! name is the listed name exactly, adding its stack size, or one for an
//! object that has none ([`carried::count_owned`]).

pub mod carried;
mod document;
pub mod layout;
mod paint;
pub mod style;

use std::collections::HashMap;

use ac_plugin_abi::ui::{PointerEvent, ThemeInfo};
use mosstank_dotnet::text::{casing, ordinal};
use mosstank_meta::host::LogLevel;
use mosstank_stores::recovery::sha256_hex;
use mosstank_stores::storage::PluginStorage;

pub use carried::{CarriedItem, CarriedKind, count_owned};
pub use layout::{HudButton, HudGeometry, HudLayout, HudRect};
pub use style::HudStyle;

use crate::canvas::{
    BUTTON_LEFT, MapCanvasSpec, MapFont, MapImage, MapPainter, MapUi, POINTER_CANCELLED,
    POINTER_DOWN, POINTER_UP,
};
use crate::host::UbHost;
use document::Document;
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

/// The Edit items window's state.
#[derive(Debug, Clone, Default)]
struct Editor {
    visible: bool,
    search: String,
    kind: CarriedKind,
    /// The selected row of what the character carries, as filtered.
    selected_carried: usize,
    /// The selected row of the HUD's list.
    selected_entry: usize,
}

/// What a left press on the strip took hold of.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Press {
    /// A button, which acts when the press is let go over it.
    Button(HudButton),
    /// The strip, at this point of the canvas, to drag.
    Strip((f64, f64)),
}

/// The HUD.
#[derive(Debug)]
pub struct ComponentHudController {
    icons: HashMap<u32, MapImage>,
    icon_order: Vec<u32>,
    style: HudStyle,
    /// The bundled font, once asked for (none: refused, or not asked yet).
    font: MapFont,
    font_asked: bool,
    entries: Vec<ComponentHudEntry>,
    counts: Vec<i32>,
    /// What the character carries, taken with the counts while the Edit
    /// items window is open.
    carried: Vec<CarriedItem>,
    editor: Editor,
    layout: HudLayout,
    enabled: bool,
    scope: Option<String>,
    key: Option<String>,
    x: i32,
    y: i32,
    canvas: Option<u32>,
    canvas_visible: bool,
    canvas_size: (i32, i32),
    refresh_remaining: f64,
    dirty: bool,
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
            style: HudStyle::classic(),
            font: MapFont::default(),
            font_asked: false,
            entries: Vec::new(),
            counts: Vec::new(),
            carried: Vec::new(),
            editor: Editor::default(),
            layout: HudLayout::Column,
            enabled,
            scope: None,
            key: None,
            x: DEFAULT_X,
            y: DEFAULT_Y,
            canvas: None,
            canvas_visible: false,
            canvas_size: (0, 0),
            refresh_remaining: 0.0,
            dirty: true,
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
    /// character.
    pub fn set_layout(&mut self, host: &mut dyn UbHost, layout: HudLayout) {
        if self.layout == layout {
            return;
        }
        self.layout = layout;
        self.dirty = true;
        self.save(host);
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
            self.style = style;
            self.dirty = true;
        }
    }

    /// The strip laid out for the list as it stands.
    #[must_use]
    pub fn geometry(&self) -> HudGeometry {
        HudGeometry::new(self.layout, &self.count_texts())
    }

    fn count_texts(&self) -> Vec<String> {
        (0..self.entries.len())
            .map(|index| self.counts.get(index).copied().unwrap_or(0).to_string())
            .collect()
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
    /// keep the canvas sized to the strip and shown while the HUD is on.
    pub fn on_tick(&mut self, host: &mut dyn UbHost, ui: &mut dyn MapUi, elapsed_seconds: f64) {
        let elapsed = elapsed_seconds.max(0.0);
        self.bind(host, ui);
        let visible = self.enabled && self.key.is_some();
        if visible || (self.editor.visible && self.key.is_some()) {
            self.refresh_remaining -= elapsed;
            if self.refresh_remaining <= 0.0 {
                self.refresh_remaining = REFRESH_SECONDS;
                self.refresh(host);
            }
        }
        if visible && (self.canvas.is_none() || self.canvas_size != self.geometry().size()) {
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
        if !visible || !self.dirty {
            return;
        }
        self.dirty = false;
        ui.invalidate_canvas(canvas);
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

    // ---- the Edit items window ----

    /// Whether the Edit items window is open.
    #[must_use]
    pub fn editor_visible(&self) -> bool {
        self.editor.visible
    }

    /// Opens the Edit items window, with what the character carries taken
    /// at once.
    pub fn show_editor(&mut self, host: &mut dyn UbHost) {
        self.editor.visible = true;
        self.carried = carried::gather(&host.objects());
        self.refresh_remaining = REFRESH_SECONDS;
    }

    /// Closes it.
    pub fn hide_editor(&mut self) {
        self.editor.visible = false;
        self.carried.clear();
    }

    /// What the window's search holds.
    #[must_use]
    pub fn search(&self) -> &str {
        &self.editor.search
    }

    /// Sets the search.
    pub fn set_search(&mut self, text: &str) {
        text.clone_into(&mut self.editor.search);
        self.editor.selected_carried = 0;
    }

    /// The kind the window lists.
    #[must_use]
    pub fn kind(&self) -> CarriedKind {
        self.editor.kind
    }

    /// Sets the kind it lists.
    pub fn set_kind(&mut self, kind: CarriedKind) {
        self.editor.kind = kind;
        self.editor.selected_carried = 0;
    }

    /// What the character carries, as the window filters it.
    #[must_use]
    pub fn carried_rows(&self) -> Vec<&CarriedItem> {
        carried::filter(&self.carried, &self.editor.search, self.editor.kind)
    }

    /// Whether a carried name is on the HUD.
    #[must_use]
    pub fn is_listed(&self, name: &str) -> bool {
        self.entries.iter().any(|entry| entry.name == name)
    }

    /// The selected carried row.
    #[must_use]
    pub fn selected_carried(&self) -> usize {
        self.editor.selected_carried
    }

    /// Selects a carried row.
    pub fn select_carried(&mut self, row: usize) {
        self.editor.selected_carried = clamp(row, self.carried_rows().len());
    }

    /// Puts carried row `row` on the HUD, or takes it off when it is on;
    /// whether the row was there.
    pub fn toggle_carried(&mut self, host: &mut dyn UbHost, row: usize) -> bool {
        let Some(item) = self.carried_rows().get(row).map(|item| (*item).clone()) else {
            return false;
        };
        self.editor.selected_carried = row;
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.name == item.name)
        {
            self.remove_at(host, index);
        } else {
            // Refused only out of the world, where there is nothing to list.
            let _ = self.add(host, &item.name, item.icon_id);
        }
        true
    }

    /// The selected row of the HUD's list.
    #[must_use]
    pub fn selected_entry(&self) -> usize {
        self.editor.selected_entry
    }

    /// Selects a row of the HUD's list.
    pub fn select_entry(&mut self, row: usize) {
        self.editor.selected_entry = clamp(row, self.entries.len());
    }

    /// The session ended: the interface's images and fonts went with it,
    /// the Edit items window closes, and the next login binds its own list.
    pub fn reset(&mut self, ui: &mut dyn MapUi) {
        self.icons.clear();
        self.icon_order.clear();
        self.font = MapFont::default();
        self.font_asked = false;
        self.press = None;
        self.scope = None;
        self.key = None;
        self.entries.clear();
        self.counts.clear();
        self.hide_editor();
        self.dirty = true;
        if let Some(canvas) = self.canvas {
            self.canvas_visible = false;
            ui.set_canvas_visible(canvas, false);
        }
    }

    /// Lets go of the icons, the font and the canvas.
    pub fn dispose(&mut self, ui: &mut dyn MapUi) {
        for id in self.icon_order.drain(..) {
            if let Some(icon) = self.icons.remove(&id) {
                ui.release_image(icon);
            }
        }
        self.icons.clear();
        if self.font.is_valid() {
            ui.release_font(self.font);
        }
        self.font = MapFont::default();
        if let Some(canvas) = self.canvas.take() {
            ui.remove_canvas(canvas);
        }
    }

    /// The pointer: a left press let go over the button it went down on is
    /// that button (the layout switch, Edit items, hide); a left press
    /// anywhere else drags the strip, which moves when the button comes up.
    pub fn on_pointer(&mut self, host: &mut dyn UbHost, ui: &mut dyn MapUi, event: &PointerEvent) {
        if event.button != BUTTON_LEFT {
            return;
        }
        let at = (f64::from(event.x), f64::from(event.y));
        match event.kind {
            POINTER_DOWN => {
                self.press = Some(match self.geometry().button_at(at) {
                    Some(button) => Press::Button(button),
                    None => Press::Strip(at),
                });
            }
            POINTER_UP => match self.press.take() {
                Some(Press::Button(button)) if self.geometry().button_at(at) == Some(button) => {
                    self.press_button(host, button);
                }
                Some(Press::Strip(grab)) => self.drag(host, ui, grab, at),
                _ => {}
            },
            POINTER_CANCELLED => self.press = None,
            _ => {}
        }
    }

    /// What a button does.
    pub fn press_button(&mut self, host: &mut dyn UbHost, button: HudButton) {
        match button {
            HudButton::Layout => self.set_layout(host, self.layout.other()),
            HudButton::Edit => self.show_editor(host),
            HudButton::Hide => self.set_enabled(host, false),
        }
    }

    fn drag(
        &mut self,
        host: &mut dyn UbHost,
        ui: &mut dyn MapUi,
        grab: (f64, f64),
        at: (f64, f64),
    ) {
        let dx = whole_pixels(at.0 - grab.0);
        let dy = whole_pixels(at.1 - grab.1);
        if (dx == 0 && dy == 0) || self.canvas.is_none() {
            return;
        }
        self.x = self.x.saturating_add(dx);
        self.y = self.y.saturating_add(dy);
        if let Some(canvas) = self.canvas {
            ui.set_canvas_offset(canvas, self.offset());
        }
        self.save(host);
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
        self.layout = HudLayout::Column;
        self.editor.selected_entry = 0;
        self.dirty = true;
        self.refresh_remaining = 0.0;
        self.load(host.storage());
        if let Some(canvas) = self.canvas {
            ui.set_canvas_offset(canvas, self.offset());
        }
    }

    /// Reads the character's list, layout and place; a file that does not
    /// read starts an empty column at the default place, and the next
    /// change rewrites it.
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
        let Ok(Some(document)) = document::read(&text) else {
            return;
        };
        self.x = document.x;
        self.y = document.y;
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
        self.dirty = true;
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
            self.dirty = true;
        }
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

    /// The strip in the theme's look: the face, each item's icon and count,
    /// and the buttons. Under a theme the counts are set in the bundled
    /// font, asked for once.
    pub fn paint(&mut self, painter: &mut dyn MapPainter, ui: &mut dyn MapUi) {
        if self.style.themed && !self.font_asked {
            self.font_asked = true;
            self.font = ui.bundled_font(style::FONT_SIZE);
        }
        let font = if self.style.themed {
            self.font
        } else {
            MapFont::default()
        };
        let texts = self.count_texts();
        let icons: Vec<MapImage> = (0..self.entries.len())
            .map(|index| self.icon(ui, self.entries[index].icon_id))
            .collect();
        let items: Vec<ItemPaint<'_>> = texts
            .iter()
            .zip(icons)
            .enumerate()
            .map(|(index, (count, icon))| ItemPaint {
                icon,
                count,
                warn: self.warns(index),
            })
            .collect();
        let geometry = HudGeometry::new(self.layout, &texts);
        paint::paint(painter, &geometry, &self.style, font, &items);
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

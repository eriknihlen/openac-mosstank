//! The clients readout (`NetworkHudController`): this character and the
//! others played on this computer, this one first, then one row each with
//! a health bar, half-bars for stamina and mana, the name and the
//! percentages, an arrow to where they stand tinted by how far, the
//! distance, and the counts of the items being tracked.
//!
//! The rows are rebuilt at most twenty times a second and the canvas is
//! repainted only when a row differs from the last time. The canvas takes
//! the pointer: a shift-click on a row selects that row's character, and a
//! left drag moves the readout. Where it stands is kept for this character
//! in the `NetworkUI.WindowPositionX` and `WindowPositionY` rows, as the
//! reference keeps its window there with the readout 4 pixels right of and
//! 60 below it, so the readout comes back to that place.
//!
//! A drag moves the readout when the button comes up, by how far the
//! pointer went since the press. Moving it under a held pointer would make
//! the next positions the client reports relative to a place the readout
//! may not have reached yet, since the client takes a new place only on
//! its next frame; held still, every position is relative to one place. A
//! place written elsewhere during a press is taken up when the press ends.
//! The reference drags only with Ctrl held and moves under the pointer
//! (divergence register, UB-10).
//!
//! Distances are measured as the reference measures them: across the
//! ground and up together, and from where a character the world shows
//! stands rather than from what its client last said.
//!
//! The network reports the other clients only, so this character's row is
//! built from the character itself; another client does not publish item
//! counts, so the tracked counts are shown on this character's row alone.

use std::collections::HashMap;

use ac_plugin_abi::ui::{Color, PointerEvent};
use ac_plugin_abi::world::{METRES_PER_MAP_UNIT, Position};
use mosstank_dotnet::text::ordinal;
use openac_plugin_compat::number;

use mosstank_settings::ub::UbSettingValue;

use crate::canvas::{
    BUTTON_LEFT, MODIFIER_SHIFT, MapCanvasSpec, MapImage, MapPainter, MapUi, POINTER_CANCELLED,
    POINTER_DOWN, POINTER_UP,
};
use crate::catalog::UbCatalog;
use crate::host::UbHost;

/// The canvas's id.
pub const CANVAS_ID: &str = "ub-clients";
/// One row's height, pixels.
pub const ROW_HEIGHT: i32 = 20;
/// The readout's width, pixels.
pub const HUD_WIDTH: i32 = 355;
/// The bars' width, pixels.
pub const NAME_WIDTH: i32 = 180;
/// The health bar's height, pixels.
pub const HEALTH_BAR_HEIGHT: i32 = 11;
/// One tracked item's width, pixels.
pub const TRACKED_ITEM_WIDTH: i32 = 24;
/// The distance's width, pixels.
pub const RANGE_WIDTH: i32 = 34;
/// The gap after the bars, pixels.
pub const PADDING: i32 = 4;
/// The most rows shown.
pub const MAX_ROWS: usize = 12;
/// The canvas's height, pixels.
pub const CANVAS_HEIGHT: i32 = 12 * ROW_HEIGHT + 5;
/// The least time between two rebuilds of the rows.
pub const REPAINT_INTERVAL_SECONDS: f64 = 1.0 / 20.0;
/// How often the page's rows are re-read for a change.
pub const SETTINGS_POLL_SECONDS: f64 = 0.25;
/// How far right of the saved window position the readout sits, pixels.
pub const HUD_X_OFFSET: i32 = 4;
/// How far below the saved window position the readout sits, pixels.
pub const HUD_Y_OFFSET: i32 = 60;
/// The window position a character starts with, pixels from the left.
pub const DEFAULT_WINDOW_X: i32 = 22;
/// The window position a character starts with, pixels from the top.
pub const DEFAULT_WINDOW_Y: i32 = 105;

const WINDOW_X: &str = "NetworkUI.WindowPositionX";
const WINDOW_Y: &str = "NetworkUI.WindowPositionY";

const ALL_TAG: &str = "All";

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

const HEALTH_BACK: Color = rgba(60, 0, 0, 180);
const HEALTH_FILL: Color = rgba(255, 0, 0, 180);
const STAMINA_BACK: Color = rgba(70, 30, 0, 180);
const STAMINA_FILL: Color = rgba(255, 180, 0, 180);
const MANA_BACK: Color = rgba(0, 20, 55, 180);
const MANA_FILL: Color = rgba(0, 80, 255, 180);
const EDGE: Color = rgba(0, 0, 0, 255);
const TEXT: Color = rgba(255, 255, 255, 255);
const SMALL_TEXT: Color = rgba(0, 0, 0, 255);
const UNKNOWN: Color = rgba(128, 128, 128, 255);
const WHITE: Color = rgba(255, 255, 255, 255);

/// The rows on the UB page the readout reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkHudSettings {
    /// Whether the readout exists at all.
    pub enabled: bool,
    /// Whether it stays on screen with its window shut.
    pub show_hud_when_closed: bool,
    /// Which tag's characters are listed; "All" lists every one.
    pub selected_tag: String,
    /// The item names whose counts are shown, one a line.
    pub tracked_items: String,
    /// The saved window position, pixels from the screen's left.
    pub window_x: i32,
    /// The saved window position, pixels from the screen's top.
    pub window_y: i32,
}

impl Default for NetworkHudSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            show_hud_when_closed: true,
            selected_tag: ALL_TAG.to_owned(),
            tracked_items: String::new(),
            window_x: DEFAULT_WINDOW_X,
            window_y: DEFAULT_WINDOW_Y,
        }
    }
}

impl NetworkHudSettings {
    /// Where the readout sits, pixels from the screen's top-left: the saved
    /// window position plus the readout's place in that window.
    #[must_use]
    pub fn canvas_offset(&self) -> (f64, f64) {
        (
            f64::from(self.window_x) + f64::from(HUD_X_OFFSET),
            f64::from(self.window_y) + f64::from(HUD_Y_OFFSET),
        )
    }

    /// The tracked item names, trimmed, blanks dropped.
    #[must_use]
    pub fn tracked_item_names(&self) -> Vec<String> {
        self.tracked_items
            .split('\n')
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect()
    }
}

/// Where the readout's rows are read from.
pub trait NetworkHudSettingsSource {
    /// The rows as they stand.
    fn read(&self) -> NetworkHudSettings;
    /// Keeps the window position a drag left the readout at.
    fn save_window_position(&mut self, x: i32, y: i32);
}

/// The readout's rows from the UB settings catalogue
/// (`ReadNetworkHudSettings`).
#[must_use]
pub fn read_settings(catalog: &dyn UbCatalog) -> NetworkHudSettings {
    NetworkHudSettings {
        enabled: catalog.flag("NetworkUI.Enabled"),
        show_hud_when_closed: catalog.flag("NetworkUI.ShowHudWhenClosed"),
        selected_tag: catalog.value("NetworkUI.SelectedTag").text,
        tracked_items: catalog.value("NetworkUI.TrackedItems").items.join("\n"),
        window_x: catalog.value(WINDOW_X).as_int32(),
        window_y: catalog.value(WINDOW_Y).as_int32(),
    }
}

/// The catalogue as the readout's settings.
pub struct CatalogHudSettings<'a>(pub &'a mut dyn UbCatalog);

impl std::fmt::Debug for CatalogHudSettings<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CatalogHudSettings")
    }
}

impl NetworkHudSettingsSource for CatalogHudSettings<'_> {
    fn read(&self) -> NetworkHudSettings {
        read_settings(self.0)
    }

    fn save_window_position(&mut self, x: i32, y: i32) {
        let row = self.0.require(WINDOW_X);
        self.0.set(&row, UbSettingValue::from_int(x));
        let row = self.0.require(WINDOW_Y);
        self.0.set(&row, UbSettingValue::from_int(y));
    }
}

/// One tracked item on one row: its icon, if the client has one, and its
/// count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkHudTrackedItem {
    /// The icon.
    pub icon: MapImage,
    /// The count, as written.
    pub count: String,
}

/// One row of the readout, worked out once per repaint and compared with
/// the last so an unchanged readout is never repainted.
#[derive(Debug, Clone, PartialEq)]
pub struct NetworkHudRow {
    /// The client's number (0 for this character).
    pub client_id: u32,
    /// The character's object id.
    pub player_id: u32,
    /// The character's name.
    pub name: String,
    /// This character's own row.
    pub is_self: bool,
    /// Health, 0 to 1.
    pub health_fraction: f64,
    /// Stamina, 0 to 1.
    pub stamina_fraction: f64,
    /// Mana, 0 to 1.
    pub mana_fraction: f64,
    /// Health as a percentage.
    pub health_text: String,
    /// Stamina as a percentage.
    pub stamina_text: String,
    /// Mana as a percentage.
    pub mana_text: String,
    /// How far away, metres, as written ("??" unknown, "" for this one).
    pub distance_text: String,
    /// Which way the arrow points, radians clockwise from straight up.
    pub arrow_radians: f64,
    /// The arrow's and the distance's green and blue.
    pub distance_tint: u8,
    /// The tracked counts.
    pub tracked: Vec<NetworkHudTrackedItem>,
}

/// The readout.
#[derive(Debug)]
pub struct NetworkHudController {
    icons: HashMap<u32, MapImage>,
    icon_order: Vec<u32>,
    canvas: Option<u32>,
    canvas_visible: bool,
    settings: NetworkHudSettings,
    settings_poll_remaining: f64,
    repaint_remaining: f64,
    shown: bool,
    dirty: bool,
    rows: Vec<NetworkHudRow>,
    /// The point of the canvas a left press took hold of, while it is held.
    grab: Option<(f64, f64)>,
    /// Where the canvas was last put, pixels from the screen's top-left.
    placed: (f64, f64),
}

impl NetworkHudController {
    /// A readout reading its rows from `settings` now.
    #[must_use]
    pub fn new(settings: &dyn NetworkHudSettingsSource) -> Self {
        Self {
            icons: HashMap::new(),
            icon_order: Vec::new(),
            canvas: None,
            canvas_visible: false,
            settings: settings.read(),
            settings_poll_remaining: 0.0,
            repaint_remaining: 0.0,
            shown: false,
            dirty: true,
            rows: Vec::new(),
            grab: None,
            placed: (0.0, 0.0),
        }
    }

    /// Puts the canvas where the saved window position says, when it is
    /// not there already.
    fn place(&mut self, ui: &mut dyn MapUi) {
        let Some(canvas) = self.canvas else {
            return;
        };
        let target = self.settings.canvas_offset();
        if self.placed != target {
            self.placed = target;
            ui.set_canvas_offset(canvas, target);
        }
    }

    /// The rows as last built: this character, then the others in the
    /// order the network reported them.
    #[must_use]
    pub fn rows(&self) -> &[NetworkHudRow] {
        &self.rows
    }

    /// The canvas, once registered.
    #[must_use]
    pub fn canvas(&self) -> Option<u32> {
        self.canvas
    }

    /// Whether the canvas is shown.
    #[must_use]
    pub fn canvas_visible(&self) -> bool {
        self.canvas_visible
    }

    /// Whether the readout's window is open.
    #[must_use]
    pub fn shown(&self) -> bool {
        self.shown
    }

    /// Opens or shuts the readout's window.
    pub fn set_shown(&mut self, value: bool) {
        if self.shown == value {
            return;
        }
        self.shown = value;
        self.dirty = true;
    }

    /// Whether the canvas should be on screen: on, and either open or
    /// allowed to stay.
    #[must_use]
    pub fn should_show(&self) -> bool {
        self.settings.enabled && (self.shown || self.settings.show_hud_when_closed)
    }

    /// The tag caption.
    #[must_use]
    pub fn tag_text(&self) -> String {
        format!("Tag: {}", self.settings.selected_tag)
    }

    /// Selects the character on one row, the way a click on the row would;
    /// the name selected, or `None` when there is no such row.
    pub fn select_row(&mut self, host: &mut dyn UbHost, index: i32) -> Option<String> {
        let row = self.rows.get(usize::try_from(index).ok()?)?;
        host.select(row.player_id);
        Some(row.name.clone())
    }

    /// The frame: re-read the page now and then, rebuild the rows at most
    /// twenty times a second, and repaint only when a row changed.
    pub fn on_tick(
        &mut self,
        host: &mut dyn UbHost,
        settings: &dyn NetworkHudSettingsSource,
        ui: &mut dyn MapUi,
        elapsed_seconds: f64,
    ) {
        let elapsed = elapsed_seconds.max(0.0);
        self.settings_poll_remaining -= elapsed;
        if self.settings_poll_remaining <= 0.0 {
            self.settings_poll_remaining = SETTINGS_POLL_SECONDS;
            let next = settings.read();
            if next != self.settings {
                // A place written elsewhere (the page, `/ub opt set`) moves
                // the readout, unless a drag is under way.
                self.settings = next;
                self.dirty = true;
                if self.grab.is_none() {
                    self.place(ui);
                }
            }
        }
        let visible = self.should_show();
        if visible && self.canvas.is_none() {
            self.canvas = ui.register_canvas(&MapCanvasSpec {
                canvas_id: CANVAS_ID.to_owned(),
                width: HUD_WIDTH,
                height: CANVAS_HEIGHT,
                offset: self.settings.canvas_offset(),
                start_visible: false,
                accepts_pointer_input: true,
            });
            self.placed = self.settings.canvas_offset();
            self.canvas_visible = false;
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

        self.repaint_remaining -= elapsed;
        if self.repaint_remaining > 0.0 && !self.dirty {
            return;
        }
        self.repaint_remaining = REPAINT_INTERVAL_SECONDS;
        let rows = self.build_rows(host, ui);
        if rows != self.rows {
            self.rows = rows;
            self.dirty = true;
        }
        if !self.dirty {
            return;
        }
        self.dirty = false;
        ui.invalidate_canvas(canvas);
    }

    /// The session ended: the interface's images went with it.
    pub fn reset(&mut self) {
        self.icons.clear();
        self.icon_order.clear();
        self.rows.clear();
        self.grab = None;
        self.dirty = true;
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

    /// The pointer over the canvas: a shift-click selects the character on
    /// the row the press lands in; a left drag moves the readout by how far
    /// the pointer went when the button comes up, and keeps the window
    /// position that puts it there. Nothing drawn changes, so nothing is
    /// repainted.
    pub fn on_pointer(
        &mut self,
        host: &mut dyn UbHost,
        settings: &mut dyn NetworkHudSettingsSource,
        ui: &mut dyn MapUi,
        event: &PointerEvent,
    ) {
        if event.button != BUTTON_LEFT {
            return;
        }
        match event.kind {
            POINTER_DOWN if event.modifiers & MODIFIER_SHIFT != 0 => {
                self.grab = None;
                if event.y < 0.0 {
                    return;
                }
                #[allow(clippy::cast_possible_truncation)]
                let row = (f64::from(event.y) / f64::from(ROW_HEIGHT)) as i32;
                self.select_row(host, row);
            }
            POINTER_DOWN => self.grab = Some((f64::from(event.x), f64::from(event.y))),
            POINTER_UP => {
                let Some((x, y)) = self.grab.take() else {
                    return;
                };
                let dx = whole_pixels(f64::from(event.x) - x);
                let dy = whole_pixels(f64::from(event.y) - y);
                if (dx != 0 || dy != 0) && self.canvas.is_some() {
                    // Measured from where the readout stood under the press.
                    let window_x =
                        whole_pixels(self.placed.0 - f64::from(HUD_X_OFFSET)).saturating_add(dx);
                    let window_y =
                        whole_pixels(self.placed.1 - f64::from(HUD_Y_OFFSET)).saturating_add(dy);
                    self.settings.window_x = window_x;
                    self.settings.window_y = window_y;
                    settings.save_window_position(window_x, window_y);
                }
                // A place written elsewhere during the press is taken up now.
                self.place(ui);
            }
            POINTER_CANCELLED => {
                self.grab = None;
                self.place(ui);
            }
            _ => {}
        }
    }

    /// The rows: this character first, from the character itself, then the
    /// clients the network reports, filtered by the selected tag. A client
    /// that did report this character's id is the same row and is not
    /// listed twice.
    pub fn build_rows(&mut self, host: &mut dyn UbHost, ui: &mut dyn MapUi) -> Vec<NetworkHudRow> {
        let tag = self.settings.selected_tag.trim().to_owned();
        let me = host.character().unwrap_or_default();
        let own_id = me.id;
        let snapshot = host.navigation();
        let mine = snapshot.is_available.then_some(snapshot.position);
        let tracked = self.settings.tracked_item_names();
        let mut rows = Vec::new();
        if own_id != 0 {
            let tracked = self.count_tracked(host, ui, &tracked);
            rows.push(NetworkHudRow {
                client_id: 0,
                player_id: own_id,
                name: me.name.clone(),
                is_self: true,
                health_fraction: fraction(me.health.0, me.health.1),
                stamina_fraction: fraction(me.stamina.0, me.stamina.1),
                mana_fraction: fraction(me.mana.0, me.mana.1),
                health_text: percent(me.health.0, me.health.1),
                stamina_text: percent(me.stamina.0, me.stamina.1),
                mana_text: percent(me.mana.0, me.mana.1),
                distance_text: String::new(),
                arrow_radians: 0.0,
                distance_tint: 255,
                tracked,
            });
        }
        if !host.peers_connected() {
            return rows;
        }
        for client in host.peer_clients() {
            if rows.len() >= MAX_ROWS {
                break;
            }
            if client.player_id == own_id {
                continue;
            }
            // Ignoring case, as the cast-sharing tag filter does, so the two
            // read one tag list the same way.
            if !tag.is_empty()
                && !ordinal::equals_ignore_case(&tag, ALL_TAG)
                && !client
                    .tags
                    .iter()
                    .any(|held| ordinal::equals_ignore_case(held, &tag))
            {
                continue;
            }
            // A character the world shows is measured where it stands, as
            // the reference does: what its client said is only as fresh as
            // its last word, and a client that does not share its state
            // keeps the place it entered the world at. The arrow aims at the
            // same place, where the reference aims it at the shared
            // coordinates (divergence register, UB-9).
            let seen = if client.player_id == 0 {
                None
            } else {
                host.object(client.player_id).and_then(|o| o.position)
            };
            let theirs = seen.as_ref().unwrap_or(&client.position);
            let (distance, arrow) = match &mine {
                Some(from) => (
                    Some(distance_metres(from, theirs)),
                    arrow_radians(from, theirs),
                ),
                None => (None, 0.0),
            };
            rows.push(NetworkHudRow {
                client_id: client.client_id,
                player_id: client.player_id,
                name: client.name.clone(),
                is_self: false,
                health_fraction: fraction(client.current_health, client.max_health),
                stamina_fraction: fraction(client.current_stamina, client.max_stamina),
                mana_fraction: fraction(client.current_mana, client.max_mana),
                health_text: percent(client.current_health, client.max_health),
                stamina_text: percent(client.current_stamina, client.max_stamina),
                mana_text: percent(client.current_mana, client.max_mana),
                distance_text: distance.map_or_else(|| "??".to_owned(), format_number),
                arrow_radians: arrow,
                distance_tint: distance.map_or(255, distance_tint),
                tracked: Vec::new(),
            });
        }
        rows
    }

    /// This character's own count of each tracked item, by name, summing
    /// stacks, with the icon of the first such item the client holds.
    fn count_tracked(
        &mut self,
        host: &mut dyn UbHost,
        ui: &mut dyn MapUi,
        names: &[String],
    ) -> Vec<NetworkHudTrackedItem> {
        if names.is_empty() {
            return Vec::new();
        }
        let mut counts = vec![0i32; names.len()];
        let mut first = vec![0u32; names.len()];
        for item in host.objects() {
            if !item.is_owned {
                continue;
            }
            for (index, name) in names.iter().enumerate() {
                if !ordinal::equals_ignore_case(&item.name, name) {
                    continue;
                }
                counts[index] += item.stack_size.max(1);
                if first[index] == 0 {
                    first[index] = item.object_id;
                }
            }
        }
        (0..names.len())
            .map(|index| NetworkHudTrackedItem {
                icon: if first[index] == 0 {
                    MapImage::default()
                } else {
                    self.icon(ui, first[index])
                },
                count: format_number(f64::from(counts[index])),
            })
            .collect()
    }

    fn icon(&mut self, ui: &mut dyn MapUi, object_id: u32) -> MapImage {
        if let Some(held) = self.icons.get(&object_id)
            && held.is_valid()
        {
            return *held;
        }
        let icon = ui.object_icon(object_id);
        if icon.is_valid() && self.icons.insert(object_id, icon).is_none() {
            self.icon_order.push(object_id);
        }
        icon
    }

    /// Paints every row: bars, edge, name and percentages, arrow, distance,
    /// tracked items.
    pub fn paint(&self, painter: &mut dyn MapPainter) {
        painter.clear(&rgba(0, 0, 0, 0));
        let mut offset = 0;
        for row in &self.rows {
            paint_bars(painter, offset, row);
            if !row.is_self {
                paint_arrow(painter, offset, row);
                let color = if row.distance_text == "??" {
                    UNKNOWN
                } else {
                    rgba(255, row.distance_tint, row.distance_tint, 255)
                };
                painter.draw_text(
                    &row.distance_text,
                    (
                        f64::from(NAME_WIDTH + PADDING + ROW_HEIGHT) - 4.0,
                        f64::from(offset) + 5.0,
                    ),
                    &color,
                    false,
                );
            }
            paint_tracked(painter, offset, row);
            // Every row is labelled with the character's name alone; the
            // client's number in front of it read as a list of ids.
            painter.draw_text(&row.name, (2.0, f64::from(offset) + 1.0), &TEXT, false);
            let (health_width, _) = painter.measure_text(&row.health_text);
            painter.draw_text(
                &row.health_text,
                (
                    f64::from(NAME_WIDTH) - health_width - 2.0,
                    f64::from(offset) + 1.0,
                ),
                &TEXT,
                false,
            );
            painter.draw_text(
                &row.stamina_text,
                (2.0, f64::from(offset + HEALTH_BAR_HEIGHT)),
                &SMALL_TEXT,
                false,
            );
            painter.draw_text(
                &row.mana_text,
                (
                    2.0 + f64::from(NAME_WIDTH) / 2.0,
                    f64::from(offset + HEALTH_BAR_HEIGHT),
                ),
                &SMALL_TEXT,
                false,
            );
            offset += ROW_HEIGHT;
        }
    }
}

fn paint_bars(painter: &mut dyn MapPainter, offset: i32, row: &NetworkHudRow) {
    let top = f64::from(offset) + 1.0;
    let height = f64::from(ROW_HEIGHT) - 2.0;
    let health = f64::from(HEALTH_BAR_HEIGHT);
    let lower = height - health;
    let width = f64::from(NAME_WIDTH);
    let half = width / 2.0;
    painter.fill_rect((1.0, top, width, health), &HEALTH_BACK);
    painter.fill_rect(
        (1.0, top, width * row.health_fraction, health),
        &HEALTH_FILL,
    );
    painter.fill_rect((1.0, top + health, half, lower), &STAMINA_BACK);
    painter.fill_rect(
        (1.0, top + health, half * row.stamina_fraction, lower),
        &STAMINA_FILL,
    );
    painter.fill_rect((1.0 + half, top + health, half, lower), &MANA_BACK);
    painter.fill_rect(
        (1.0 + half, top + health, half * row.mana_fraction, lower),
        &MANA_FILL,
    );
    painter.stroke_rect((1.0, top, width, height), &EDGE, 1.0);
}

fn paint_arrow(painter: &mut dyn MapPainter, offset: i32, row: &NetworkHudRow) {
    let size = f64::from(ROW_HEIGHT) * 0.75;
    let cx = f64::from(NAME_WIDTH + PADDING) + (size / 2.0);
    let cy = f64::from(offset) + (f64::from(ROW_HEIGHT) / 2.0);
    let tint = rgba(255, row.distance_tint, row.distance_tint, 255);
    let tip = along(cx, cy, row.arrow_radians, size / 2.0);
    let left = along(
        cx,
        cy,
        row.arrow_radians + (std::f64::consts::PI * 0.78),
        size * 0.45,
    );
    let right = along(
        cx,
        cy,
        row.arrow_radians - (std::f64::consts::PI * 0.78),
        size * 0.45,
    );
    painter.draw_line(tip, left, &tint, 2.0);
    painter.draw_line(tip, right, &tint, 2.0);
    painter.draw_line(left, right, &tint, 2.0);
}

fn paint_tracked(painter: &mut dyn MapPainter, offset: i32, row: &NetworkHudRow) {
    let mut x = f64::from(NAME_WIDTH + PADDING + RANGE_WIDTH);
    let icon_size = ROW_HEIGHT - 1;
    for item in &row.tracked {
        if item.icon.is_valid() {
            painter.draw_image(
                item.icon,
                (
                    x + f64::from(TRACKED_ITEM_WIDTH - icon_size) / 2.0,
                    f64::from(offset) + 1.0,
                    f64::from(icon_size),
                    f64::from(icon_size),
                ),
                &WHITE,
            );
        }
        let (width, _) = painter.measure_text(&item.count);
        painter.draw_text(
            &item.count,
            (
                x + (f64::from(TRACKED_ITEM_WIDTH) - width) / 2.0,
                f64::from(offset + ROW_HEIGHT) - 10.0,
            ),
            &TEXT,
            true,
        );
        x += f64::from(TRACKED_ITEM_WIDTH);
    }
}

fn along(x: f64, y: f64, radians: f64, distance: f64) -> (f64, f64) {
    (
        x + (radians.sin() * distance),
        y - (radians.cos() * distance),
    )
}

/// Which way an arrow drawn on the screen points at another character:
/// their compass bearing from this one, less the way this one faces, in
/// radians clockwise from straight up.
#[must_use]
pub fn arrow_radians(from: &Position, to: &Position) -> f64 {
    let bearing = (to.east_west - from.east_west).atan2(to.north_south - from.north_south);
    let facing = f64::from(from.heading_degrees) * std::f64::consts::PI / 180.0;
    let mut relative = bearing - facing;
    let turn = std::f64::consts::PI * 2.0;
    while relative < 0.0 {
        relative += turn;
    }
    while relative >= turn {
        relative -= turn;
    }
    relative
}

/// How far apart two places are, metres: across the ground and up together,
/// as the reference measures between two sets of coordinates.
#[must_use]
pub fn distance_metres(from: &Position, to: &Position) -> f64 {
    let east = to.east_west - from.east_west;
    let north = to.north_south - from.north_south;
    let up = to.elevation - from.elevation;
    (east * east + north * north + up * up).sqrt() * METRES_PER_MAP_UNIT
}

/// White up close, redder with every metre: the green and blue drop two per
/// metre.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn distance_tint(meters: f64) -> u8 {
    (255.0 - (meters * 2.0)).clamp(0.0, 255.0) as u8
}

/// A count or a distance as the readout writes it: whole up to a thousand,
/// then thousands.
#[must_use]
pub fn format_number(value: f64) -> String {
    if value < 1000.0 {
        n(value, "N0")
    } else if value < 10000.0 {
        n(value / 1000.0, "N1") + "k"
    } else {
        n(value / 1000.0, "N0") + "k"
    }
}

/// `value.ToString(format, InvariantCulture)` for a format the kit
/// reproduces.
fn n(value: f64, format: &str) -> String {
    number::format_f64(value, format).expect("a number format the kit reproduces")
}

/// A drag's distance as whole pixels, the way a window position is kept.
#[allow(clippy::cast_possible_truncation)]
fn whole_pixels(value: f64) -> i32 {
    value
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

fn fraction(current: u32, max: u32) -> f64 {
    if max == 0 {
        0.0
    } else {
        (f64::from(current) / f64::from(max)).clamp(0.0, 1.0)
    }
}

fn percent(current: u32, max: u32) -> String {
    n(f64::from(current) / f64::from(max.max(1)) * 100.0, "N0") + "%"
}

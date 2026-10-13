//! The countryside map on a canvas (`LandscapeMapController`): the client's
//! own map art, the character on it, the points of interest around them,
//! coordinate rulers down the sides, and the coordinate at the middle.
//!
//! The canvas is retained: the client keeps what was last painted and
//! repaints only when asked. So the map asks only when something it draws
//! changed: the view panned or zoomed, the character moved by a whole
//! canvas pixel or turned, the marker set changed, the page changed, or the
//! map was shown.
//!
//! The canvas takes the pointer: a drag slides the map and the wheel zooms
//! about the cursor. The client reports the pointer only while a button is
//! held or the wheel turns, so the readout is the position under the cursor
//! while a button is held and the middle otherwise.

use std::collections::HashMap;

use ac_plugin_abi::ui::{Color, PointerEvent};
use ac_plugin_abi::world::Position;
use mosstank_meta::host::MetaHost;
use mosstank_stores::storage::{PluginStorage, StorageError};

use super::markers::{self, LandscapeMarkerCatalog};
use super::placer::{MapLabelPlacer, Rect};
use super::view::{self, LandscapeMapView};
use crate::canvas::{
    BUTTON_LEFT, MapCanvasSpec, MapImage, MapPainter, MapUi, POINTER_CANCELLED, POINTER_DOWN,
    POINTER_MOVE, POINTER_UP, POINTER_WHEEL,
};
use crate::catalog::UbCatalog;

/// The canvas's id.
pub const CANVAS_ID: &str = "ub-map";
/// The canvas's width, pixels.
pub const CANVAS_WIDTH: i32 = 320;
/// The canvas's height, pixels.
pub const CANVAS_HEIGHT: i32 = 280;
/// The rulers' width, pixels.
pub const GUTTER_WIDTH: i32 = 20;
/// A marker icon's size, pixels.
pub const ICON_SIZE: i32 = 16;
/// How far one arrow button slides the map, pixels.
pub const PAN_STEP_PIXELS: f64 = 40.0;
/// The client's overworld map art, the one its map page draws.
pub const MAP_ART_SURFACE_ID: u32 = 0x0600_0261;
/// How often the page's rows are re-read for a change.
pub const SETTINGS_POLL_SECONDS: f64 = 0.25;

/// The map art's size when the client has none to hand, so the grid has a
/// shape.
const FALLBACK_IMAGE_SIZE: i32 = 2048;

const BACKGROUND: Color = rgba(29, 33, 59, 255);
const GUTTER_FILL: Color = rgba(0, 0, 0, 140);
const TICK_COLOR: Color = rgba(128, 128, 128, 255);
const TICK_TEXT: Color = rgba(211, 211, 211, 255);
const GRID_LINE: Color = rgba(70, 80, 110, 255);
const PLAYER_COLOR: Color = rgba(255, 0, 255, 255);
const LABEL_COLOR: Color = rgba(255, 255, 255, 255);
const WHITE: Color = rgba(255, 255, 255, 255);

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

/// The two rows on the UB page the map reads (`LandscapeMapSettings`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LandscapeMapSettings {
    /// The map draws.
    pub enabled: bool,
    /// The background's alpha.
    pub opacity: i32,
}

impl Default for LandscapeMapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            opacity: 16,
        }
    }
}

/// Where the map's rows are read from.
pub trait LandscapeMapSettingsSource {
    /// The rows as they stand.
    fn read(&self) -> LandscapeMapSettings;
}

/// The rows from the UB settings catalogue (`ReadLandscapeMapSettings`).
#[must_use]
pub fn read_settings(catalog: &dyn UbCatalog) -> LandscapeMapSettings {
    LandscapeMapSettings {
        enabled: catalog.flag("LandscapeMaps.Enabled"),
        opacity: catalog.value("LandscapeMaps.Opacity").as_int32(),
    }
}

/// The catalogue as the map's settings.
pub struct CatalogLandscapeSettings<'a>(pub &'a dyn UbCatalog);

impl std::fmt::Debug for CatalogLandscapeSettings<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CatalogLandscapeSettings")
    }
}

impl LandscapeMapSettingsSource for CatalogLandscapeSettings<'_> {
    fn read(&self) -> LandscapeMapSettings {
        read_settings(self.0)
    }
}

impl LandscapeMapSettingsSource for LandscapeMapSettings {
    fn read(&self) -> LandscapeMapSettings {
        *self
    }
}

/// Where the character stands, when the client places them.
pub fn self_position(host: &mut dyn MetaHost) -> Option<Position> {
    let snapshot = host.navigation();
    snapshot.is_available.then_some(snapshot.position)
}

/// The countryside map.
#[derive(Debug)]
pub struct LandscapeMapController {
    view: LandscapeMapView,
    markers: LandscapeMarkerCatalog,
    images: HashMap<u32, MapImage>,
    image_order: Vec<u32>,
    placer: MapLabelPlacer,
    canvas: Option<u32>,
    canvas_visible: bool,
    settings: LandscapeMapSettings,
    settings_poll_remaining: f64,
    shown: bool,
    dirty: bool,
    last_player_x: i32,
    last_player_y: i32,
    last_heading: f32,
    seen_marker_revision: i32,
    last_player: Option<Position>,
    drag_from: Option<(f64, f64)>,
    pointer: Option<(f64, f64)>,
    invalidate_count: usize,
}

impl LandscapeMapController {
    /// A map reading its rows from `settings` now; no canvas until it is
    /// first shown.
    #[must_use]
    pub fn new(settings: &dyn LandscapeMapSettingsSource) -> Self {
        Self {
            view: LandscapeMapView::new(
                CANVAS_WIDTH,
                CANVAS_HEIGHT,
                FALLBACK_IMAGE_SIZE,
                FALLBACK_IMAGE_SIZE,
            ),
            markers: LandscapeMarkerCatalog::new(),
            images: HashMap::new(),
            image_order: Vec::new(),
            placer: MapLabelPlacer::new(),
            canvas: None,
            canvas_visible: false,
            settings: settings.read(),
            settings_poll_remaining: 0.0,
            shown: false,
            dirty: true,
            last_player_x: i32::MIN,
            last_player_y: i32::MIN,
            last_heading: f32::NAN,
            seen_marker_revision: -1,
            last_player: None,
            drag_from: None,
            pointer: None,
            invalidate_count: 0,
        }
    }

    /// The view arithmetic; the buttons and the paint both go through it.
    #[must_use]
    pub fn view(&self) -> &LandscapeMapView {
        &self.view
    }

    /// The view, to change directly.
    pub fn view_mut(&mut self) -> &mut LandscapeMapView {
        &mut self.view
    }

    /// The points of interest.
    #[must_use]
    pub fn markers(&self) -> &LandscapeMarkerCatalog {
        &self.markers
    }

    /// Re-reads the marker files (the map window does on opening).
    ///
    /// # Errors
    /// The storage failed.
    pub fn load_markers(&mut self, storage: &mut dyn PluginStorage) -> Result<bool, StorageError> {
        self.markers.load(storage)
    }

    /// The window's marker line: how many markers, from where.
    #[must_use]
    pub fn marker_count_text(&self) -> String {
        format!(
            "{} markers from {}",
            self.markers.markers().len(),
            markers::FOLDER
        )
    }

    /// The canvas, once registered.
    #[must_use]
    pub fn canvas(&self) -> Option<u32> {
        self.canvas
    }

    /// Whether the map's window is open.
    #[must_use]
    pub fn shown(&self) -> bool {
        self.shown
    }

    /// Opens or closes the map's window; the canvas shows only while open.
    pub fn set_shown(&mut self, value: bool) {
        if self.shown == value {
            return;
        }
        self.shown = value;
        self.dirty = true;
    }

    /// How many repaints have been asked for.
    #[must_use]
    pub fn invalidate_count(&self) -> usize {
        self.invalidate_count
    }

    /// The coordinate under the cursor while a button is held over the map,
    /// else at the middle, as the in-game readout writes it.
    #[must_use]
    pub fn readout(&self) -> String {
        let (x, y) = self.pointer.unwrap_or((
            f64::from(CANVAS_WIDTH) / 2.0,
            f64::from(CANVAS_HEIGHT) / 2.0,
        ));
        let (east_west, north_south) = self.view.to_world(x, y);
        view::position_text(east_west, north_south)
    }

    /// The follow button's caption.
    #[must_use]
    pub fn follow_text(&self) -> &'static str {
        if self.view.follow {
            "Follow: on"
        } else {
            "Follow: off"
        }
    }

    /// Zooms in one step about the middle.
    pub fn zoom_in(&mut self) {
        self.zoom_about(
            f64::from(CANVAS_WIDTH) / 2.0,
            f64::from(CANVAS_HEIGHT) / 2.0,
            view::ZOOM_STEP,
        );
    }

    /// Zooms out one step about the middle.
    pub fn zoom_out(&mut self) {
        self.zoom_about(
            f64::from(CANVAS_WIDTH) / 2.0,
            f64::from(CANVAS_HEIGHT) / 2.0,
            -view::ZOOM_STEP,
        );
    }

    /// Zooms about a canvas point, the way the wheel does; a step past the
    /// limit changes nothing.
    pub fn zoom_about(&mut self, x: f64, y: f64, delta: f64) {
        let before = self.view.zoom();
        self.view.zoom_about(x, y, delta);
        #[allow(clippy::float_cmp)]
        if self.view.zoom() != before {
            self.dirty = true;
        }
    }

    /// Slides the map by canvas pixels, the way a drag does.
    pub fn pan(&mut self, dx_pixels: f64, dy_pixels: f64) {
        self.view.pan_by(dx_pixels, dy_pixels);
        self.dirty = true;
    }

    /// Follow on: centre on the character and come in to at least the
    /// follow zoom. Follow off: leave the view where it is.
    pub fn toggle_follow(&mut self, player: Option<Position>) {
        self.view.follow = !self.view.follow;
        if self.view.follow {
            if self.view.zoom() < view::FOLLOW_ZOOM {
                self.view.set_zoom(view::FOLLOW_ZOOM);
            }
            if let Some(player) = player {
                self.view.center_on(player.east_west, player.north_south);
            }
        }
        self.dirty = true;
    }

    /// The frame: re-read the page now and then, re-centre while following,
    /// and ask for a repaint only when something drawn changed.
    pub fn on_tick(
        &mut self,
        settings: &dyn LandscapeMapSettingsSource,
        ui: &mut dyn MapUi,
        player: Option<Position>,
        elapsed_seconds: f64,
    ) {
        self.settings_poll_remaining -= elapsed_seconds.max(0.0);
        if self.settings_poll_remaining <= 0.0 {
            self.settings_poll_remaining = SETTINGS_POLL_SECONDS;
            let next = settings.read();
            if next != self.settings {
                self.settings = next;
                self.dirty = true;
            }
        }
        let visible = self.shown && self.settings.enabled;
        if visible && self.canvas.is_none() {
            self.canvas = ui.register_canvas(&MapCanvasSpec {
                canvas_id: CANVAS_ID.to_owned(),
                width: CANVAS_WIDTH,
                height: CANVAS_HEIGHT,
                offset: (40.0, 200.0),
                start_visible: false,
                accepts_pointer_input: true,
            });
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

        self.last_player.clone_from(&player);
        if let Some(at) = &player {
            // Measured against the view as it was, before any re-centring,
            // so a step while following still reads as a change.
            let (x, y) = self.view.to_canvas(at.east_west, at.north_south);
            if self.view.follow {
                self.view.center_on(at.east_west, at.north_south);
            }
            #[allow(clippy::cast_possible_truncation)]
            let (px, py) = (x as i32, y as i32);
            if px != self.last_player_x || py != self.last_player_y {
                self.last_player_x = px;
                self.last_player_y = py;
                self.dirty = self.dirty
                    || self.view.follow
                    || self.view.contains(f64::from(px), f64::from(py));
            }
            #[allow(clippy::float_cmp)]
            if at.heading_degrees != self.last_heading {
                self.last_heading = at.heading_degrees;
                self.dirty = true;
            }
        }
        if self.markers.revision() != self.seen_marker_revision {
            self.seen_marker_revision = self.markers.revision();
            self.dirty = true;
        }
        if !self.dirty {
            return;
        }
        self.dirty = false;
        self.invalidate_count += 1;
        ui.invalidate_canvas(canvas);
    }

    /// The session ended: the interface's images went with it, so the
    /// handles are dropped and asked for again on the next paint.
    pub fn reset(&mut self) {
        self.images.clear();
        self.image_order.clear();
        self.last_player_x = i32::MIN;
        self.last_player_y = i32::MIN;
        self.last_heading = f32::NAN;
        self.dirty = true;
    }

    /// Lets go of the images and the canvas.
    pub fn dispose(&mut self, ui: &mut dyn MapUi) {
        for surface in self.image_order.drain(..) {
            if let Some(image) = self.images.remove(&surface) {
                ui.release_image(image);
            }
        }
        self.images.clear();
        self.drag_from = None;
        self.pointer = None;
        if let Some(canvas) = self.canvas.take() {
            ui.remove_canvas(canvas);
        }
    }

    /// The pointer over the canvas: a left press starts a drag and puts the
    /// readout under the cursor, each held move slides the map by its delta,
    /// the release or a cancel ends the drag and returns the readout to the
    /// middle, and the wheel zooms about the cursor.
    pub fn on_pointer(&mut self, event: &PointerEvent) {
        let at = (f64::from(event.x), f64::from(event.y));
        match event.kind {
            POINTER_DOWN if event.button == BUTTON_LEFT => {
                self.drag_from = Some(at);
                self.move_pointer(Some(at));
            }
            POINTER_MOVE => {
                let Some(from) = self.drag_from else {
                    return;
                };
                let dx = at.0 - from.0;
                let dy = at.1 - from.1;
                if dx != 0.0 || dy != 0.0 {
                    self.pan(dx, dy);
                }
                self.drag_from = Some(at);
                self.move_pointer(Some(at));
            }
            POINTER_UP | POINTER_CANCELLED => {
                self.drag_from = None;
                self.move_pointer(None);
            }
            POINTER_WHEEL => {
                self.zoom_about(at.0, at.1, f64::from(event.wheel_delta) * view::ZOOM_STEP);
            }
            _ => {}
        }
    }

    /// Moves the readout's point; a repaint only when its text changed.
    fn move_pointer(&mut self, at: Option<(f64, f64)>) {
        let before = self.readout();
        self.pointer = at;
        if self.readout() != before {
            self.dirty = true;
        }
    }

    // ---- Painting ---------------------------------------------------------

    /// Paints the whole map: background, the map art or a grid in its
    /// place, the markers and their names, the rulers, the character, and
    /// the readout. Called on the frame after an invalidate.
    pub fn paint(&mut self, painter: &mut dyn MapPainter, ui: &mut dyn MapUi) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let alpha = self.settings.opacity.clamp(0, 255) as u8;
        painter.clear(&Color {
            a: alpha,
            ..BACKGROUND
        });

        let map = self.image(ui, MAP_ART_SURFACE_ID);
        if map.is_valid() {
            #[allow(clippy::cast_possible_wrap)]
            let (width, height) = (map.width as i32, map.height as i32);
            if self.view.image_width() != width || self.view.image_height() != height {
                self.adopt_image_size(width, height);
            }
        }

        let (w, h) = (f64::from(CANVAS_WIDTH), f64::from(CANVAS_HEIGHT));
        painter.push_clip((0.0, 0.0, w, h));
        if map.is_valid() {
            let scale = self.view.scale();
            painter.draw_image(
                map,
                (
                    self.view.canvas_left(),
                    self.view.canvas_top(),
                    f64::from(self.view.image_width()) * scale,
                    f64::from(self.view.image_height()) * scale,
                ),
                &WHITE,
            );
        } else {
            self.paint_grid(painter);
        }
        self.paint_markers(painter, ui);
        painter.pop_clip();

        self.paint_rulers(painter);
        if let Some(player) = &self.last_player {
            self.paint_player(painter, player);
        }
        let readout = self.readout();
        let (width, height) = painter.measure_text(&readout);
        painter.draw_text(
            &readout,
            (
                (w - width) / 2.0,
                h - f64::from(GUTTER_WIDTH) - height - 4.0,
            ),
            &LABEL_COLOR,
            true,
        );
    }

    fn adopt_image_size(&mut self, width: i32, height: i32) {
        let (east_west, north_south) = self.view.to_world(
            f64::from(CANVAS_WIDTH) / 2.0,
            f64::from(CANVAS_HEIGHT) / 2.0,
        );
        let mut next = LandscapeMapView::new(CANVAS_WIDTH, CANVAS_HEIGHT, width, height);
        next.set_zoom(self.view.zoom());
        next.follow = self.view.follow;
        next.center_on(east_west, north_south);
        self.view = next;
    }

    fn image(&mut self, ui: &mut dyn MapUi, surface_id: u32) -> MapImage {
        if let Some(held) = self.images.get(&surface_id)
            && held.is_valid()
        {
            return *held;
        }
        let image = ui.client_art(surface_id);
        if image.is_valid() && self.images.insert(surface_id, image).is_none() {
            self.image_order.push(surface_id);
        }
        image
    }

    /// The grid drawn in place of the map art, on the rulers' steps.
    fn paint_grid(&self, painter: &mut dyn MapPainter) {
        let (w, h) = (f64::from(CANVAS_WIDTH), f64::from(CANVAS_HEIGHT));
        let increment = self
            .view
            .horizontal_increment(&mut |text| painter.measure_text(text));
        let min_east = self.view.to_world(0.0, 0.0).0;
        let max_east = self.view.to_world(w, 0.0).0;
        let mut c = (min_east / increment).floor() * increment;
        while c <= max_east {
            let x = self.view.to_canvas(c, 0.0).0;
            painter.draw_line((x, 0.0), (x, h), &GRID_LINE, 1.0);
            c += increment;
        }
        let increment = self
            .view
            .vertical_increment(&mut |text| painter.measure_text(text));
        let max_north = self.view.to_world(0.0, 0.0).1;
        let min_north = self.view.to_world(0.0, h).1;
        let mut c = (min_north / increment).floor() * increment;
        while c <= max_north {
            let y = self.view.to_canvas(0.0, c).1;
            painter.draw_line((0.0, y), (w, y), &GRID_LINE, 1.0);
            c += increment;
        }
    }

    fn paint_markers(&mut self, painter: &mut dyn MapPainter, ui: &mut dyn MapUi) {
        self.placer.reset();
        let zoom = self.view.zoom();
        let icon = f64::from(ICON_SIZE);
        let mut labelled = Vec::new();
        for index in 0..self.markers.markers().len() {
            let marker = &self.markers.markers()[index];
            let display = markers::display_of(marker.kind);
            if !display.shows_marker_at(zoom) {
                continue;
            }
            let (x, y) = self.view.to_canvas(marker.east_west, marker.north_south);
            if !self.view.contains(x, y) {
                continue;
            }
            let icon_rect = Rect::new(x - icon / 2.0, y - icon / 2.0, icon, icon);
            let image = self.image(ui, display.icon_surface_id);
            if image.is_valid() {
                painter.draw_image(image, icon_rect.tuple(), &WHITE);
            } else {
                painter.stroke_rect(icon_rect.tuple(), &LABEL_COLOR, 1.0);
            }
            if display.shows_label_at(zoom) {
                self.placer.occupy(icon_rect);
                labelled.push((index, x, y));
            }
        }
        for (index, x, y) in labelled {
            let name = self.markers.markers()[index].name.clone();
            let (width, height) = painter.measure_text(&name);
            if let Some(placed) = self.placer.try_place(x, y, width, height) {
                painter.draw_text(&name, (placed.x + 2.0, placed.y), &LABEL_COLOR, true);
            }
        }
    }

    fn paint_rulers(&self, painter: &mut dyn MapPainter) {
        let (w, h) = (f64::from(CANVAS_WIDTH), f64::from(CANVAS_HEIGHT));
        let gutter = f64::from(GUTTER_WIDTH);
        painter.fill_rect((0.0, 0.0, w, gutter), &GUTTER_FILL);
        painter.fill_rect((0.0, h - gutter, w, gutter), &GUTTER_FILL);
        painter.fill_rect((0.0, gutter, gutter, h - 2.0 * gutter), &GUTTER_FILL);
        painter.fill_rect((w - gutter, gutter, gutter, h - 2.0 * gutter), &GUTTER_FILL);

        for tick in self
            .view
            .horizontal_ticks(&mut |text| painter.measure_text(text))
        {
            painter.draw_line((tick.pixel, 0.0), (tick.pixel, 4.0), &TICK_COLOR, 1.0);
            painter.draw_line((tick.pixel, h - 4.0), (tick.pixel, h), &TICK_COLOR, 1.0);
            let (width, _) = painter.measure_text(&tick.label);
            painter.draw_text(
                &tick.label,
                (tick.pixel - width / 2.0, 4.0),
                &TICK_TEXT,
                false,
            );
            painter.draw_text(
                &tick.label,
                (tick.pixel - width / 2.0, h - gutter + 2.0),
                &TICK_TEXT,
                false,
            );
        }
        for tick in self
            .view
            .vertical_ticks(&mut |text| painter.measure_text(text))
        {
            if tick.pixel < gutter || tick.pixel > h - gutter {
                continue;
            }
            painter.draw_line((0.0, tick.pixel), (4.0, tick.pixel), &TICK_COLOR, 1.0);
            painter.draw_line((w - 4.0, tick.pixel), (w, tick.pixel), &TICK_COLOR, 1.0);
            let (width, height) = painter.measure_text(&tick.label);
            painter.draw_text(
                &tick.label,
                (4.0, tick.pixel - height / 2.0),
                &TICK_TEXT,
                false,
            );
            painter.draw_text(
                &tick.label,
                (w - width - 4.0, tick.pixel - height / 2.0),
                &TICK_TEXT,
                false,
            );
        }
    }

    /// The character as an arrow pointing the way they face. Heading is
    /// degrees clockwise from north and the canvas's y runs down, so north
    /// is straight up.
    fn paint_player(&self, painter: &mut dyn MapPainter, player: &Position) {
        let (x, y) = if self.view.follow {
            (
                f64::from(CANVAS_WIDTH) / 2.0,
                f64::from(CANVAS_HEIGHT) / 2.0,
            )
        } else {
            self.view.to_canvas(player.east_west, player.north_south)
        };
        if !self.view.contains(x, y) {
            return;
        }
        let heading = f64::from(player.heading_degrees) * std::f64::consts::PI / 180.0;
        let tip = along(x, y, heading, 8.0);
        let left = along(x, y, heading + std::f64::consts::PI * 0.78, 7.0);
        let right = along(x, y, heading - std::f64::consts::PI * 0.78, 7.0);
        painter.draw_line(tip, left, &PLAYER_COLOR, 2.0);
        painter.draw_line(tip, right, &PLAYER_COLOR, 2.0);
        painter.draw_line(left, right, &PLAYER_COLOR, 2.0);
    }
}

fn along(x: f64, y: f64, heading_radians: f64, distance: f64) -> (f64, f64) {
    (
        x + heading_radians.sin() * distance,
        y - heading_radians.cos() * distance,
    )
}

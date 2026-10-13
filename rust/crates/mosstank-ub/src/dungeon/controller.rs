//! The dungeon map on a canvas (`DungeonMapController`): the floor plan of
//! the sealed dungeon the character stands in, turned with their heading,
//! with the things in it marked and the cells they have walked shaded.
//!
//! The canvas is retained, so the map asks for a repaint only when something
//! it draws changed: the landblock, which storeys are drawn and how dim, the
//! camera by at least a screen pixel or a degree, a marker placed or moved
//! past its threshold, a newly walked cell, or a row on the page. The world
//! is read thirty times a second at most.
//!
//! The canvas takes the pointer: a drag pans, a shift-drag turns, and the
//! wheel zooms about the cursor, each through the same calls the control
//! strip makes with fixed steps. A drag lets go of the player; a press the
//! host cancels ends the drag where it is.
//!
//! The canvas and the images come in through [`MapUi`], the paint goes out
//! through [`MapPainter`], and the settings rows through
//! [`DungeonMapSettingsSource`]: the plugin answers them with the SDK's
//! canvas and painter and the settings catalogue.

use std::collections::HashMap;

use ac_plugin_abi::navigation::NavigationSnapshot;
use ac_plugin_abi::ui::{Color, PointerEvent};
use ac_plugin_abi::world::Floorplan;
use mosstank_meta::host::LogLevel;

use super::camera::DungeonMapCamera;
use super::markers::{DungeonMarkerKind, DungeonMarkerStyle};
use super::picture::{DungeonMapLayerPicture, DungeonMapPicture};
use super::shade::DungeonLayerShade;
use super::tracker::{DungeonObjectTracker, DungeonTrackedObject};
use super::visited::DungeonVisitedCells;
use super::{Vec2, to_landblock_local};
use crate::catalog::UbCatalog;
use crate::host::UbHost;
use mosstank_settings::ub::UbSettingValue;

/// The canvas's id.
pub const CANVAS_ID: &str = "ub-dungeon";
/// The canvas's width, pixels.
pub const CANVAS_WIDTH: i32 = 320;
/// The canvas's height, pixels.
pub const CANVAS_HEIGHT: i32 = 280;
/// How far one pan button moves the map, pixels.
pub const PAN_STEP_PIXELS: i32 = 40;
/// How far one turn button turns it, degrees.
pub const ROTATE_STEP_DEGREES: f64 = 15.0;
/// How often the page's rows are re-read for a change.
pub const SETTINGS_POLL_SECONDS: f64 = 0.25;
/// The least time between two reads of the world: thirty a second.
pub const WORLD_READ_SECONDS: f64 = 1.0 / 30.0;

const COMPASS_RADIUS: f64 = 12.0;
const COMPASS_INSET: f64 = 18.0;

/// One drawn part of the map: its switch and its colour, as `#AARRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DungeonMapPart {
    /// Drawn.
    pub enabled: bool,
    /// Its colour.
    pub color: u32,
}

/// The eleven marker rows, one style each, in [`super::markers::KINDS`]
/// order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DungeonMarkerStyles(pub [DungeonMarkerStyle; 11]);

impl DungeonMarkerStyles {
    /// The style of one kind.
    #[must_use]
    pub fn of(&self, kind: DungeonMarkerKind) -> DungeonMarkerStyle {
        let index = super::markers::KINDS
            .iter()
            .position(|known| *known == kind)
            .unwrap_or(10);
        self.0[index]
    }

    /// The shipped values of the marker rows.
    #[must_use]
    pub const fn shipped() -> Self {
        Self([
            DungeonMarkerStyle::new(true, false, false, 0xFFFF_0000, 3),
            DungeonMarkerStyle::new(true, true, true, 0xFFFF_FFFF, 3),
            DungeonMarkerStyle::new(true, true, true, 0xFFFF_FFFF, 3),
            DungeonMarkerStyle::new(true, true, false, 0xFFFF_A500, 3),
            DungeonMarkerStyle::new(true, true, false, 0xFFFF_FF00, 3),
            DungeonMarkerStyle::new(true, true, true, 0xFFFF_0000, 3),
            DungeonMarkerStyle::new(false, true, false, 0xFFF5_F5F5, 3),
            DungeonMarkerStyle::new(true, true, true, 0xFFFF_F0FF, 3),
            DungeonMarkerStyle::new(true, true, false, 0xFFF4_A460, 3),
            DungeonMarkerStyle::new(true, false, false, 0xFFA5_2A2A, 3),
            DungeonMarkerStyle::new(false, true, false, 0xFFF5_F5F5, 3),
        ])
    }
}

/// The rows on the UB page the dungeon map reads, as one value so a change
/// to any of them is one comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct DungeonMapSettings {
    /// The map is on.
    pub enabled: bool,
    /// The map shows with its window shut.
    pub draw_when_closed: bool,
    /// The walked cells are shaded.
    pub show_visited_tiles: bool,
    /// Their colour.
    pub visited_tiles_color: u32,
    /// The compass is drawn.
    pub show_compass: bool,
    /// How solid the map is, out of twenty.
    pub opacity: i32,
    /// The zoom row (five minus the scale).
    pub map_zoom: f32,
    /// The cell id after the name.
    pub debug: bool,
    /// The name across the top.
    pub dungeon_name: DungeonMapPart,
    /// The walls.
    pub walls: DungeonMapPart,
    /// The floors.
    pub floors: DungeonMapPart,
    /// The marks.
    pub markers: DungeonMarkerStyles,
}

impl Default for DungeonMapSettings {
    /// The shipped values of the rows.
    fn default() -> Self {
        Self {
            enabled: true,
            draw_when_closed: true,
            show_visited_tiles: true,
            visited_tiles_color: 0xFFFF_96FF,
            show_compass: true,
            opacity: 16,
            map_zoom: 4.2,
            debug: false,
            dungeon_name: DungeonMapPart {
                enabled: true,
                color: 0xFFFF_FFFF,
            },
            walls: DungeonMapPart {
                enabled: true,
                color: 0xFF00_007F,
            },
            floors: DungeonMapPart {
                enabled: true,
                color: 0xFF00_7FBF,
            },
            markers: DungeonMarkerStyles::shipped(),
        }
    }
}

/// Where the map's rows are read from and its zoom written back to.
pub trait DungeonMapSettingsSource {
    /// The rows as they stand.
    fn read(&self) -> DungeonMapSettings;
    /// Writes the zoom row.
    fn save_map_zoom(&mut self, zoom: f32);
}

/// The rows from the UB settings catalogue (`MossTankPanel.UbDungeon`).
pub struct CatalogMapSettings<'a>(pub &'a mut dyn UbCatalog);

impl std::fmt::Debug for CatalogMapSettings<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CatalogMapSettings")
    }
}

impl DungeonMapSettingsSource for CatalogMapSettings<'_> {
    fn read(&self) -> DungeonMapSettings {
        read_settings(self.0)
    }

    fn save_map_zoom(&mut self, zoom: f32) {
        let row = self.0.require("DungeonMaps.MapZoom");
        self.0.set(&row, UbSettingValue::from_single(zoom));
    }
}

/// The map's rows from the catalogue (`ReadDungeonMapSettings`).
#[must_use]
pub fn read_settings(catalog: &dyn UbCatalog) -> DungeonMapSettings {
    let part = |group: &str| {
        let prefix = format!("DungeonMaps.Display.{group}.");
        DungeonMapPart {
            enabled: catalog.flag(&format!("{prefix}Enabled")),
            color: catalog.value(&format!("{prefix}Color")).as_color(),
        }
    };
    let marker = |kind: DungeonMarkerKind| {
        let prefix = format!(
            "DungeonMaps.Display.Markers.{}.",
            super::markers::group_name(kind)
        );
        DungeonMarkerStyle {
            enabled: catalog.flag(&format!("{prefix}Enabled")),
            use_icon: catalog.flag(&format!("{prefix}UseIcon")),
            show_label: catalog.flag(&format!("{prefix}ShowLabel")),
            color: catalog.value(&format!("{prefix}Color")).as_color(),
            size: catalog.value(&format!("{prefix}Size")).as_int32(),
        }
    };
    DungeonMapSettings {
        enabled: catalog.flag("DungeonMaps.Enabled"),
        draw_when_closed: catalog.flag("DungeonMaps.DrawWhenClosed"),
        show_visited_tiles: catalog.flag("DungeonMaps.ShowVisitedTiles"),
        visited_tiles_color: catalog.value("DungeonMaps.VisitedTilesColor").as_color(),
        show_compass: catalog.flag("DungeonMaps.ShowCompass"),
        opacity: catalog.value("DungeonMaps.Opacity").as_int32(),
        map_zoom: catalog.value("DungeonMaps.MapZoom").as_single(),
        debug: catalog.flag("DungeonMaps.Debug"),
        dungeon_name: part("DungeonName"),
        walls: part("Walls"),
        floors: part("Floors"),
        markers: DungeonMarkerStyles(super::markers::KINDS.map(marker)),
    }
}

/// An image the client holds for the plugin (handle 0: none).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MapImage {
    /// The client's number for it.
    pub handle: u32,
    /// Width, pixels.
    pub width: u32,
    /// Height, pixels.
    pub height: u32,
}

impl MapImage {
    /// The client issued it.
    #[must_use]
    pub fn is_valid(self) -> bool {
        self.handle != 0
    }
}

/// What the map's canvas is registered with (`PluginCanvasDescriptor`).
#[derive(Debug, Clone, PartialEq)]
pub struct MapCanvasSpec {
    /// Its id.
    pub canvas_id: String,
    /// Width, pixels.
    pub width: i32,
    /// Height, pixels.
    pub height: i32,
    /// Pixels from the screen's top-left.
    pub offset: (f64, f64),
    /// Shown at once.
    pub start_visible: bool,
    /// Takes pointer input.
    pub accepts_pointer_input: bool,
}

/// The canvas and the images, as the client keeps them.
pub trait MapUi {
    /// Registers a canvas; its handle, `None` when refused.
    fn register_canvas(&mut self, spec: &MapCanvasSpec) -> Option<u32>;
    /// Shows or hides it.
    fn set_canvas_visible(&mut self, canvas: u32, visible: bool);
    /// Asks for a repaint: [`DungeonMapController::paint`] on the next
    /// frame.
    fn invalidate_canvas(&mut self, canvas: u32);
    /// Removes it.
    fn remove_canvas(&mut self, canvas: u32);
    /// An object's icon, or no image.
    fn object_icon(&mut self, object_id: u32) -> MapImage;
    /// Lets go of an image.
    fn release_image(&mut self, image: MapImage);
}

/// What the paint draws with (`IPluginPainter`): pixels from the canvas's
/// top-left, y down.
pub trait MapPainter {
    /// The canvas's width.
    fn width(&self) -> i32;
    /// The canvas's height.
    fn height(&self) -> i32;
    /// Fills the canvas.
    fn clear(&mut self, color: &Color);
    /// A straight line.
    fn draw_line(&mut self, from: (f64, f64), to: (f64, f64), color: &Color, thickness: f32);
    /// One line of text with its top-left at `at`.
    fn draw_text(&mut self, text: &str, at: (f64, f64), color: &Color, outline: bool);
    /// How big a line of text is: width and height.
    fn measure_text(&mut self, text: &str) -> (f64, f64);
    /// An image stretched over a rectangle (x, y, width, height), tinted.
    fn draw_image(&mut self, image: MapImage, rect: (f64, f64, f64, f64), tint: &Color);
    /// Clips what follows to a rectangle.
    fn push_clip(&mut self, rect: (f64, f64, f64, f64));
    /// Undoes the last clip.
    fn pop_clip(&mut self);
}

const POINTER_DOWN: u8 = 0;
const POINTER_UP: u8 = 1;
const POINTER_MOVE: u8 = 2;
const POINTER_WHEEL: u8 = 3;
const POINTER_CANCELLED: u8 = 4;
const BUTTON_LEFT: u8 = 1;
const MODIFIER_SHIFT: u8 = 1;

/// One storey's shade, compared for a change.
type ShadeSignature = (Vec<(bool, u32, u32)>, i64);

/// The dungeon map.
#[derive(Debug)]
pub struct DungeonMapController {
    camera: DungeonMapCamera,
    tracker: DungeonObjectTracker,
    visited: DungeonVisitedCells,
    icons: HashMap<u32, MapImage>,
    icon_order: Vec<u32>,
    canvas: Option<u32>,
    canvas_visible: bool,
    settings: DungeonMapSettings,
    settings_poll_remaining: f64,
    world_read_remaining: f64,
    shown: bool,
    dirty: bool,
    in_dungeon: bool,
    plan: Floorplan,
    picture: DungeonMapPicture,
    landblock_id: u32,
    draw_z: f64,
    height_percent: i32,
    show_all_layers: bool,
    applied_map_zoom: f32,
    camera_signature: Option<(i32, i32, i32, u32)>,
    shade_signature: Option<ShadeSignature>,
    drag_from: Option<(f64, f64)>,
    invalidate_count: usize,
}

impl DungeonMapController {
    /// A map reading its rows from `settings` now.
    #[must_use]
    pub fn new(settings: &dyn DungeonMapSettingsSource) -> Self {
        let settings = settings.read();
        let mut camera = DungeonMapCamera::new();
        camera.set_scale(DungeonMapCamera::scale_from_setting(settings.map_zoom));
        Self {
            camera,
            tracker: DungeonObjectTracker::new(),
            visited: DungeonVisitedCells::new(),
            icons: HashMap::new(),
            icon_order: Vec::new(),
            canvas: None,
            canvas_visible: false,
            applied_map_zoom: settings.map_zoom,
            settings,
            settings_poll_remaining: 0.0,
            world_read_remaining: 0.0,
            shown: false,
            dirty: true,
            in_dungeon: false,
            plan: Floorplan::default(),
            picture: DungeonMapPicture::empty(),
            landblock_id: 0,
            draw_z: 0.0,
            height_percent: 0,
            show_all_layers: false,
            camera_signature: None,
            shade_signature: None,
            drag_from: None,
            invalidate_count: 0,
        }
    }

    /// Where the map looks from; the strip and the paint both go through it.
    #[must_use]
    pub fn camera(&self) -> &DungeonMapCamera {
        &self.camera
    }

    /// The camera, to drive directly.
    pub fn camera_mut(&mut self) -> &mut DungeonMapCamera {
        &mut self.camera
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

    /// Whether the map's own window is open. With draw-when-closed off, the
    /// canvas shows only while it is.
    #[must_use]
    pub fn shown(&self) -> bool {
        self.shown
    }

    /// Opens or shuts the map's window.
    pub fn set_shown(&mut self, value: bool) {
        if self.shown == value {
            return;
        }
        self.shown = value;
        self.dirty = true;
    }

    /// Whether every storey is drawn plainly, however far from the player.
    #[must_use]
    pub fn show_all_layers(&self) -> bool {
        self.show_all_layers
    }

    /// Draws every storey plainly, or only the near ones.
    pub fn set_show_all_layers(&mut self, value: bool) {
        if self.show_all_layers == value {
            return;
        }
        self.show_all_layers = value;
        self.dirty = true;
    }

    /// The height the map is drawn from, in metres: the player's while
    /// following, else the slider's.
    #[must_use]
    pub fn draw_z(&self) -> f64 {
        self.draw_z
    }

    /// The height slider's position, 0 at the lowest storey to 100 at the
    /// highest.
    #[must_use]
    pub fn height_percent(&self) -> i32 {
        self.height_percent
    }

    /// How many times the host has been asked to repaint.
    #[must_use]
    pub fn invalidate_count(&self) -> usize {
        self.invalidate_count
    }

    /// The follow button's caption.
    #[must_use]
    pub fn follow_text(&self) -> &'static str {
        if self.camera.following() {
            "Follow: on"
        } else {
            "Follow: off"
        }
    }

    /// The layers button's caption.
    #[must_use]
    pub fn layers_text(&self) -> &'static str {
        if self.show_all_layers {
            "Layers: all"
        } else {
            "Layers: near"
        }
    }

    /// Which dungeon the map is of and how much of it has been walked, or
    /// that it is nowhere.
    #[must_use]
    pub fn status(&self) -> String {
        if self.in_dungeon && !self.picture.is_empty() {
            format!(
                "Dungeon {}: {} storeys, {} of {} cells walked.",
                landblock_name(self.landblock_id),
                self.picture.layers.len(),
                self.visited.len(),
                self.picture.cells.len()
            )
        } else {
            "Not in a dungeon.".to_owned()
        }
    }

    /// One zoom step closer, saved to the page's row.
    pub fn zoom_in(&mut self, settings: &mut dyn DungeonMapSettingsSource) {
        self.camera.zoom_in();
        self.save_zoom(settings);
    }

    /// One zoom step further out, saved to the page's row.
    pub fn zoom_out(&mut self, settings: &mut dyn DungeonMapSettingsSource) {
        self.camera.zoom_out();
        self.save_zoom(settings);
    }

    /// Zooms so many steps about a canvas point, the way the wheel does,
    /// saved to the page's row. A step past the limit changes nothing.
    pub fn zoom_about(
        &mut self,
        settings: &mut dyn DungeonMapSettingsSource,
        x: f64,
        y: f64,
        steps: i32,
    ) {
        let before = self.camera.scale();
        self.camera.zoom_about(
            x,
            y,
            steps,
            f64::from(CANVAS_WIDTH),
            f64::from(CANVAS_HEIGHT),
        );
        #[allow(clippy::float_cmp)]
        if self.camera.scale() != before {
            self.save_zoom(settings);
        }
    }

    /// Slides the map by a number of canvas pixels, the way a drag does.
    pub fn pan(&mut self, dx_pixels: f64, dy_pixels: f64) {
        self.camera.pan_by(dx_pixels, dy_pixels);
        self.dirty = true;
    }

    /// Turns the map by a drag's pixels, the way a shift-drag does.
    pub fn rotate_by_drag(&mut self, dx_pixels: f64, dy_pixels: f64) {
        self.camera.rotate_by_drag(dx_pixels, dy_pixels);
        self.dirty = true;
    }

    /// Turns the map by so many degrees clockwise.
    pub fn rotate(&mut self, degrees: f64) {
        self.camera.rotate_by(degrees);
        self.dirty = true;
    }

    /// The height slider: picks the storey drawn and lets go of the player.
    pub fn set_height_percent(&mut self, percent: i32) {
        self.height_percent = percent.clamp(0, 100);
        self.camera.stop_following();
        self.draw_z = DungeonLayerShade::draw_z_from_slider(
            self.picture.min_z,
            self.picture.max_z,
            self.height_percent,
        );
        self.dirty = true;
    }

    /// Follow on: back onto the player and their storey. Follow off: stay
    /// put.
    pub fn toggle_follow(&mut self) {
        if self.camera.following() {
            self.camera.stop_following();
        } else {
            self.camera.resume_following();
        }
        self.dirty = true;
    }

    /// The frame: re-read the page now and then, read the world thirty times
    /// a second at most, and ask for a repaint only when something drawn
    /// changed.
    pub fn on_tick(
        &mut self,
        host: &mut dyn UbHost,
        settings: &dyn DungeonMapSettingsSource,
        ui: &mut dyn MapUi,
        elapsed_seconds: f64,
    ) {
        let elapsed_seconds = elapsed_seconds.max(0.0);
        self.settings_poll_remaining -= elapsed_seconds;
        if self.settings_poll_remaining <= 0.0 {
            self.settings_poll_remaining = SETTINGS_POLL_SECONDS;
            let next = settings.read();
            if next != self.settings {
                self.settings = next;
                self.dirty = true;
            }
            if (self.settings.map_zoom - self.applied_map_zoom).abs() > 0.001 {
                self.applied_map_zoom = self.settings.map_zoom;
                self.camera
                    .set_scale(DungeonMapCamera::scale_from_setting(self.settings.map_zoom));
            }
        }

        self.world_read_remaining -= elapsed_seconds;
        if self.world_read_remaining > 0.0 && !self.dirty {
            return;
        }
        self.world_read_remaining = WORLD_READ_SECONDS;

        let player = player(host);
        self.in_dungeon = self.settings.enabled
            && player
                .as_ref()
                .is_some_and(|snapshot| host.is_sealed_dungeon(snapshot.position.cell_id));
        if self.in_dungeon
            && let Some(snapshot) = &player
        {
            self.enter_landblock(host, ui, snapshot.position.cell_id & 0xFFFF_0000);
        }

        let visible = self.in_dungeon
            && !self.picture.is_empty()
            && (self.settings.draw_when_closed || self.shown);
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
        let Some(snapshot) = player else {
            return;
        };

        let position = snapshot.position;
        let local = to_landblock_local(&position);
        if self.camera.following() {
            self.camera.follow(local.xy(), position.heading_degrees);
            self.draw_z = f64::from(local.z);
            self.height_percent = DungeonLayerShade::slider_from_draw_z(
                self.picture.min_z,
                self.picture.max_z,
                self.draw_z,
            );
        }

        match self.visited.mark(host.storage(), position.cell_id) {
            Ok(true) => self.dirty = true,
            Ok(false) => {}
            Err(error) => {
                // The cell counts for the session; only the file is behind.
                self.dirty = true;
                host.log(
                    LogLevel::Warn,
                    &format!("The dungeon map could not save the cells walked: {error}"),
                );
            }
        }

        let (self_id, self_name) = own(host);
        let objects = host.objects();
        if self
            .tracker
            .update(&objects, &self.plan, &position, self_id, &self_name)
        {
            self.dirty = true;
        }

        let shade = self.shade_signature();
        if self.shade_signature.as_ref() != Some(&shade) {
            self.shade_signature = Some(shade);
            self.dirty = true;
        }

        // Quantised to whole screen pixels and degrees: a step too small to
        // move the picture is not a change.
        let center = self
            .picture
            .map_pixel_from_local_xy(self.camera.center_meters())
            * self.camera.scale();
        #[allow(clippy::cast_possible_truncation)]
        let signature = (
            f64::from(center.x).round_ties_even() as i32,
            f64::from(center.y).round_ties_even() as i32,
            self.camera.rotation_degrees().round_ties_even() as i32,
            self.camera.scale().to_bits(),
        );
        if self.camera_signature != Some(signature) {
            self.camera_signature = Some(signature);
            self.dirty = true;
        }

        if !self.dirty {
            return;
        }
        self.dirty = false;
        self.invalidate_count += 1;
        ui.invalidate_canvas(canvas);
    }

    /// The session ended: the interface's images are gone with it, so the
    /// handles are dropped and asked for again on the next paint.
    pub fn reset(&mut self) {
        self.icons.clear();
        self.icon_order.clear();
        self.tracker.clear();
        self.camera_signature = None;
        self.dirty = true;
    }

    /// Lets go of the icons and the canvas.
    pub fn dispose(&mut self, ui: &mut dyn MapUi) {
        self.release_icons(ui);
        self.drag_from = None;
        if let Some(canvas) = self.canvas.take() {
            ui.remove_canvas(canvas);
        }
    }

    /// The pointer over the canvas: a left press starts a drag, each held
    /// move pans by its delta, or turns by it with shift held, the release
    /// or a cancel ends it, and the wheel zooms about the cursor.
    pub fn on_pointer(
        &mut self,
        settings: &mut dyn DungeonMapSettingsSource,
        event: &PointerEvent,
    ) {
        let at = (f64::from(event.x), f64::from(event.y));
        match event.kind {
            POINTER_DOWN if event.button == BUTTON_LEFT => self.drag_from = Some(at),
            POINTER_MOVE => {
                let Some(from) = self.drag_from else {
                    return;
                };
                let dx = at.0 - from.0;
                let dy = at.1 - from.1;
                #[allow(clippy::float_cmp)]
                if dx == 0.0 && dy == 0.0 {
                    return;
                }
                if event.modifiers & MODIFIER_SHIFT != 0 {
                    self.rotate_by_drag(dx, dy);
                } else {
                    self.pan(dx, dy);
                }
                self.drag_from = Some(at);
            }
            POINTER_UP | POINTER_CANCELLED => self.drag_from = None,
            POINTER_WHEEL => self.zoom_about(settings, at.0, at.1, event.wheel_delta),
            _ => {}
        }
    }

    fn enter_landblock(&mut self, host: &mut dyn UbHost, ui: &mut dyn MapUi, landblock_id: u32) {
        if landblock_id == self.landblock_id {
            return;
        }
        self.landblock_id = landblock_id;
        self.plan = host.dungeon_floorplan(landblock_id).unwrap_or_default();
        self.picture = DungeonMapPicture::build(&self.plan);
        self.tracker.clear();
        self.visited.bind(host.storage(), landblock_id);
        self.release_icons(ui);
        self.shade_signature = None;
        self.dirty = true;
    }

    fn save_zoom(&mut self, settings: &mut dyn DungeonMapSettingsSource) {
        self.applied_map_zoom = DungeonMapCamera::setting_from_scale(self.camera.scale());
        settings.save_map_zoom(self.applied_map_zoom);
        self.dirty = true;
    }

    fn release_icons(&mut self, ui: &mut dyn MapUi) {
        for id in self.icon_order.drain(..) {
            if let Some(icon) = self.icons.remove(&id) {
                ui.release_image(icon);
            }
        }
        self.icons.clear();
    }

    fn shade_signature(&self) -> ShadeSignature {
        let shades = self
            .picture
            .layers
            .iter()
            .map(|layer| {
                let shade = DungeonLayerShade::of(self.draw_z, layer.z, self.show_all_layers);
                (
                    shade.visible,
                    shade.brightness.to_bits(),
                    shade.opacity.to_bits(),
                )
            })
            .collect();
        (shades, storey_of(self.draw_z))
    }

    // ---- Painting ---------------------------------------------------------

    /// Paints the whole map: each drawn storey's floor, the shading of the
    /// cells walked, the walls and the marks, lowest storey first so the
    /// nearer ones lie on top; then every label upright, the compass and the
    /// name. Called on the frame after an invalidate.
    pub fn paint(
        &mut self,
        painter: &mut dyn MapPainter,
        ui: &mut dyn MapUi,
        player: Option<&NavigationSnapshot>,
    ) {
        painter.clear(&Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        });
        if self.picture.is_empty() {
            return;
        }

        #[allow(clippy::cast_precision_loss)]
        let opacity = (self.settings.opacity as f32 / 20.0).clamp(0.0, 1.0);
        let width = f64::from(painter.width());
        let height = f64::from(painter.height());
        let center_map_pixel = self
            .picture
            .map_pixel_from_local_xy(self.camera.center_meters());
        let mut labelled: Vec<(DungeonTrackedObject, Vec2, DungeonMarkerStyle, f64)> = Vec::new();

        painter.push_clip((0.0, 0.0, width, height));
        let layers = self.picture.layers.clone();
        for layer in &layers {
            let shade = DungeonLayerShade::of(self.draw_z, layer.z, self.show_all_layers);
            if !shade.visible {
                continue;
            }
            self.paint_floor(
                painter,
                layer,
                shade,
                opacity,
                center_map_pixel,
                width,
                height,
            );
            self.paint_walls(
                painter,
                layer,
                shade,
                opacity,
                center_map_pixel,
                width,
                height,
            );
            self.paint_marks(
                painter,
                ui,
                layer,
                shade,
                opacity,
                width,
                height,
                &mut labelled,
            );
        }
        painter.pop_clip();

        for (subject, at, style, radius) in &labelled {
            let (text_width, _) = painter.measure_text(&subject.name);
            painter.draw_text(
                &subject.name,
                (
                    f64::from(at.x) - (text_width / 2.0),
                    f64::from(at.y) + radius + 2.0,
                ),
                &tint(style.color, 1.0, opacity),
                true,
            );
        }

        if self.settings.show_compass {
            self.paint_compass(painter, opacity, width);
        }
        if self.settings.dungeon_name.enabled {
            let mut name = format!(
                "Dungeon {} (Z:{})",
                landblock_name(self.landblock_id),
                storey_of(self.draw_z)
            );
            if self.settings.debug
                && let Some(snapshot) = player.filter(|snapshot| snapshot.is_available)
            {
                name.push_str(&format!(" {:08X}", snapshot.position.cell_id));
            }
            let (text_width, _) = painter.measure_text(&name);
            painter.draw_text(
                &name,
                ((width - text_width) / 2.0, 2.0),
                &tint(self.settings.dungeon_name.color, 1.0, opacity),
                true,
            );
        }
    }

    /// Each floor run is a rectangle in map pixels; turned with the camera it
    /// is still a rectangle, which one line of the right thickness along its
    /// middle draws exactly. A run of a walked cell is drawn again in the
    /// visited colour.
    #[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
    fn paint_floor(
        &self,
        painter: &mut dyn MapPainter,
        layer: &DungeonMapLayerPicture,
        shade: DungeonLayerShade,
        opacity: f32,
        center: Vec2,
        width: f64,
        height: f64,
    ) {
        if !self.settings.floors.enabled {
            return;
        }
        let floor = tint(
            self.settings.floors.color,
            shade.brightness,
            opacity * shade.opacity,
        );
        let visited = tint(
            self.settings.visited_tiles_color,
            shade.brightness,
            opacity * shade.opacity,
        );
        for run in &layer.floors {
            let rect = run.rect;
            let mid_y = rect.y as f32 + (rect.height as f32 / 2.0);
            let from =
                self.camera
                    .screen_from_map(Vec2::new(rect.x as f32, mid_y), center, width, height);
            let to = self.camera.screen_from_map(
                Vec2::new((rect.x + rect.width) as f32, mid_y),
                center,
                width,
                height,
            );
            let thickness = rect.height as f32 * self.camera.scale();
            painter.draw_line(point(from), point(to), &floor, thickness);
            if self.settings.show_visited_tiles
                && let Ok(index) = usize::try_from(run.cell_index)
                && let Some(cell) = self.picture.cells.get(index)
                && self.visited.contains(cell.cell_id)
            {
                painter.draw_line(point(from), point(to), &visited, thickness);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_walls(
        &self,
        painter: &mut dyn MapPainter,
        layer: &DungeonMapLayerPicture,
        shade: DungeonLayerShade,
        opacity: f32,
        center: Vec2,
        width: f64,
        height: f64,
    ) {
        if !self.settings.walls.enabled {
            return;
        }
        let color = tint(
            self.settings.walls.color,
            shade.brightness,
            opacity * shade.opacity,
        );
        let thickness = self.camera.scale().max(1.0);
        for wall in &layer.walls {
            let from =
                self.camera
                    .screen_from_map(Vec2::new(wall.x1, wall.y1), center, width, height);
            let to =
                self.camera
                    .screen_from_map(Vec2::new(wall.x2, wall.y2), center, width, height);
            painter.draw_line(point(from), point(to), &color, thickness);
        }
    }

    /// The marks on one storey: the object's icon when its row asks for one
    /// and the client has it, else a dot in the row's colour. Labels are put
    /// aside for the pass after every storey, and only for the storey the
    /// map is drawn from unless every storey is shown.
    #[allow(clippy::too_many_arguments, clippy::float_cmp)]
    fn paint_marks(
        &mut self,
        painter: &mut dyn MapPainter,
        ui: &mut dyn MapUi,
        layer: &DungeonMapLayerPicture,
        shade: DungeonLayerShade,
        opacity: f32,
        width: f64,
        height: f64,
        labelled: &mut Vec<(DungeonTrackedObject, Vec2, DungeonMarkerStyle, f64)>,
    ) {
        #[allow(clippy::cast_possible_truncation)]
        let draw_band = DungeonLayerShade::band_of(self.draw_z as f32);
        let label_this = self.show_all_layers || layer.z == draw_band;
        let subjects: Vec<DungeonTrackedObject> = self.tracker.objects().cloned().collect();
        for subject in subjects {
            if !subject.is_placed || subject.layer_z != layer.z {
                continue;
            }
            let style = self.settings.markers.of(subject.kind);
            if !style.enabled {
                continue;
            }
            let at = self
                .camera
                .screen_from_local(&self.picture, subject.position, width, height);
            let mut radius = f64::from(style.size.max(1));
            let icon = if style.use_icon {
                self.icon(ui, subject.id)
            } else {
                MapImage::default()
            };
            if icon.is_valid() {
                let side = f64::from(style.size) * 32.0 / 5.0;
                radius = side / 2.0;
                painter.draw_image(
                    icon,
                    (
                        f64::from(at.x) - radius,
                        f64::from(at.y) - radius,
                        side,
                        side,
                    ),
                    &tint(0xFFFF_FFFF, shade.brightness, opacity * shade.opacity),
                );
            } else {
                paint_dot(
                    painter,
                    at,
                    radius,
                    &tint(style.color, shade.brightness, opacity * shade.opacity),
                );
            }
            if label_this && style.show_label {
                labelled.push((subject, at, style, radius));
            }
        }
    }

    /// A needle from the top-right corner's circle to north, with an N at
    /// its tip.
    #[allow(clippy::cast_possible_truncation)]
    fn paint_compass(&self, painter: &mut dyn MapPainter, opacity: f32, width: f64) {
        let center = Vec2::new((width - COMPASS_INSET) as f32, COMPASS_INSET as f32);
        let north = self.camera.north_on_screen();
        let radius = COMPASS_RADIUS as f32;
        let tip = center + (north * radius);
        let color = tint(0xFFFF_FFFF, 1.0, opacity);
        painter.draw_line(point(center - (north * radius)), point(tip), &color, 2.0);
        let (text_width, text_height) = painter.measure_text("N");
        painter.draw_text(
            "N",
            (
                f64::from(tip.x) - (text_width / 2.0),
                f64::from(tip.y) - (text_height / 2.0),
            ),
            &color,
            true,
        );
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
}

/// The player's snapshot, when the character is in the world.
fn player(host: &mut dyn UbHost) -> Option<NavigationSnapshot> {
    let snapshot = host.navigation();
    snapshot.is_available.then_some(snapshot)
}

/// The character's id and name (0 and empty out of the world).
fn own(host: &mut dyn UbHost) -> (u32, String) {
    host.character()
        .map_or((0, String::new()), |info| (info.id, info.name))
}

/// `(int)Math.Floor((drawZ + 3d) / 6d)`: the storey number of a height.
#[allow(clippy::cast_possible_truncation)]
fn storey_of(draw_z: f64) -> i64 {
    ((draw_z + 3.0) / 6.0).floor() as i64
}

fn landblock_name(landblock_id: u32) -> String {
    format!("{:04X}", (landblock_id >> 16) & 0xFFFF)
}

fn point(v: Vec2) -> (f64, f64) {
    (f64::from(v.x), f64::from(v.y))
}

/// A filled circle, as the rows of a disc: one line per pixel row.
#[allow(clippy::cast_possible_truncation)]
fn paint_dot(painter: &mut dyn MapPainter, at: Vec2, radius: f64, color: &Color) {
    let rows = radius.ceil() as i32;
    for row in -rows..=rows {
        let row = f64::from(row);
        let half = ((radius * radius) - (row * row)).max(0.0).sqrt();
        if half <= 0.0 {
            continue;
        }
        painter.draw_line(
            (f64::from(at.x) - half, f64::from(at.y) + row),
            (f64::from(at.x) + half, f64::from(at.y) + row),
            color,
            1.0,
        );
    }
}

/// An `#AARRGGBB` colour multiplied by a brightness and an opacity.
#[must_use]
pub fn tint(argb: u32, brightness: f32, opacity: f32) -> Color {
    Color {
        r: channel((argb >> 16) & 0xFF, brightness),
        g: channel((argb >> 8) & 0xFF, brightness),
        b: channel(argb & 0xFF, brightness),
        a: channel((argb >> 24) & 0xFF, opacity),
    }
}

/// `(byte)Math.Clamp(Math.Round(value * factor), 0, 255)`: the product is
/// single precision, rounded half to even.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn channel(value: u32, factor: f32) -> u8 {
    let product = f64::from(value as f32 * factor);
    product.round_ties_even().clamp(0.0, 255.0) as u8
}

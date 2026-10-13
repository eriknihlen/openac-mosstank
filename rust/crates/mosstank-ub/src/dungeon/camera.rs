//! Where the dungeon map looks from (`DungeonMapCamera`): the
//! landblock-local point at the middle of the canvas, how far it is zoomed,
//! and how it is turned. Following, it sits on the player and turns with
//! their heading so that what lies ahead is up; a pan, a turn or the height
//! slider lets go of the player until follow is asked for again.
//!
//! The picture is north-up, so turning it by the heading is the whole of
//! the rotation: a point d map pixels from the middle lands at d scaled and
//! turned anticlockwise on screen by the heading, which puts the point
//! ahead of the player above the middle.

use super::picture::{DungeonMapPicture, PIXELS_PER_METER};
use super::{Vec2, Vec3};

/// As far out as the map zooms: screen pixels per map pixel.
pub const MIN_SCALE: f32 = 0.3;

/// As far in as the map zooms.
pub const MAX_SCALE: f32 = 5.0;

/// How much one zoom step changes the scale.
pub const SCALE_STEP: f32 = 0.2;

/// The camera.
#[derive(Debug, Clone, PartialEq)]
pub struct DungeonMapCamera {
    scale: f32,
    rotation: f64,
    center_meters: Vec2,
    following: bool,
}

impl Default for DungeonMapCamera {
    fn default() -> Self {
        Self {
            scale: Self::scale_from_setting(4.2),
            rotation: 0.0,
            center_meters: Vec2::default(),
            following: true,
        }
    }
}

/// `MathF.Round(value, 1)`: to one decimal, half to even.
fn round_tenth(value: f32) -> f32 {
    // .NET rounds the scaled value in single precision.
    let scaled = value * 10.0;
    scaled.round_ties_even() / 10.0
}

impl DungeonMapCamera {
    /// The shipped camera: scale 0.8, following.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Screen pixels per map pixel, from 0.3 to 5.
    #[must_use]
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Sets the scale, held to its range.
    pub fn set_scale(&mut self, value: f32) {
        self.scale = value.clamp(MIN_SCALE, MAX_SCALE);
    }

    /// How far the picture is turned, in degrees clockwise from north-up, 0
    /// to 360.
    #[must_use]
    pub fn rotation_degrees(&self) -> f64 {
        self.rotation
    }

    fn set_rotation(&mut self, value: f64) {
        self.rotation = ((value % 360.0) + 360.0) % 360.0;
    }

    /// The landblock-local point at the middle of the canvas, in metres.
    #[must_use]
    pub fn center_meters(&self) -> Vec2 {
        self.center_meters
    }

    /// Whether the camera sits on the player and turns with them.
    #[must_use]
    pub fn following(&self) -> bool {
        self.following
    }

    /// The settings row keeps the zoom as five minus the scale, so the
    /// shipped 4.2 is a scale of 0.8: four screen pixels to the metre.
    #[must_use]
    pub fn scale_from_setting(map_zoom: f32) -> f32 {
        (MAX_SCALE - map_zoom).clamp(MIN_SCALE, MAX_SCALE)
    }

    /// The settings row's value for a scale.
    #[must_use]
    pub fn setting_from_scale(scale: f32) -> f32 {
        MAX_SCALE - scale
    }

    /// Sits on the player, if following.
    pub fn follow(&mut self, player_meters: Vec2, heading_degrees: f32) {
        if !self.following {
            return;
        }
        self.center_meters = player_meters;
        self.set_rotation(f64::from(heading_degrees));
    }

    /// Goes back to sitting on the player from the next [`Self::follow`].
    pub fn resume_following(&mut self) {
        self.following = true;
    }

    /// Stays where it is while the player moves.
    pub fn stop_following(&mut self) {
        self.following = false;
    }

    /// One step closer, to the nearest tenth so the steps land on round
    /// numbers.
    pub fn zoom_in(&mut self) {
        self.set_scale(round_tenth(self.scale + SCALE_STEP));
    }

    /// One step further out.
    pub fn zoom_out(&mut self) {
        self.set_scale(round_tenth(self.scale - SCALE_STEP));
    }

    /// Zooms by so many steps about a canvas point, as a wheel does: the
    /// spot under the point before is the spot under it after. Following,
    /// only the scale moves, since the middle is put back on the player
    /// anyway. Does not let go of the player.
    #[allow(clippy::cast_precision_loss)]
    pub fn zoom_about(
        &mut self,
        x: f64,
        y: f64,
        steps: i32,
        canvas_width: f64,
        canvas_height: f64,
    ) {
        let before = self.scale;
        self.set_scale(round_tenth(self.scale + (steps as f32 * SCALE_STEP)));
        #[allow(clippy::float_cmp)]
        if self.following || self.scale == before {
            return;
        }
        // The spot sits d pixels from the middle; scaled it would sit at d
        // times the ratio, so the picture slides by the difference.
        let ratio = f64::from(self.scale / before);
        self.shift_by(
            (x - (canvas_width / 2.0)) * (1.0 - ratio),
            (y - (canvas_height / 2.0)) * (1.0 - ratio),
        );
    }

    /// Moves the picture by a screen delta, as a drag does: the middle of
    /// the canvas moves the other way, turned back into the map's frame and
    /// scaled into metres. Lets go of the player.
    pub fn pan_by(&mut self, dx_pixels: f64, dy_pixels: f64) {
        self.following = false;
        self.shift_by(dx_pixels, dy_pixels);
    }

    #[allow(clippy::cast_possible_truncation)]
    fn shift_by(&mut self, dx_pixels: f64, dy_pixels: f64) {
        let radians = self.rotation * std::f64::consts::PI / 180.0;
        let cos = radians.cos();
        let sin = radians.sin();
        // Turn the screen delta back into the picture's frame (undoing the
        // anticlockwise turn by the heading), then into metres, y up.
        let map_dx = (-dx_pixels * cos) - (-dy_pixels * sin);
        let map_dy = (-dx_pixels * sin) + (-dy_pixels * cos);
        let scale = f64::from(self.scale);
        let per_meter = f64::from(PIXELS_PER_METER);
        self.center_meters = self.center_meters
            + Vec2::new(
                (map_dx / scale / per_meter) as f32,
                (-map_dy / scale / per_meter) as f32,
            );
    }

    /// Turns the picture by a drag: a degree for every pixel dragged left
    /// and every pixel dragged up, as the reference does. Lets go of the
    /// player.
    pub fn rotate_by_drag(&mut self, dx_pixels: f64, dy_pixels: f64) {
        self.rotate_by(-(dx_pixels + dy_pixels));
    }

    /// Turns the picture by so many degrees clockwise. Lets go of the
    /// player.
    pub fn rotate_by(&mut self, degrees: f64) {
        self.following = false;
        self.set_rotation(self.rotation + degrees);
    }

    /// Where a map pixel lands on a canvas of the given size.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn screen_from_map(
        &self,
        map_pixel: Vec2,
        center_map_pixel: Vec2,
        canvas_width: f64,
        canvas_height: f64,
    ) -> Vec2 {
        let d = (map_pixel - center_map_pixel) * self.scale;
        let radians = -self.rotation * std::f64::consts::PI / 180.0;
        let cos = radians.cos();
        let sin = radians.sin();
        let (dx, dy) = (f64::from(d.x), f64::from(d.y));
        Vec2::new(
            ((dx * cos) - (dy * sin) + (canvas_width / 2.0)) as f32,
            ((dx * sin) + (dy * cos) + (canvas_height / 2.0)) as f32,
        )
    }

    /// Where a landblock-local point lands on a canvas of the given size.
    #[must_use]
    pub fn screen_from_local(
        &self,
        picture: &DungeonMapPicture,
        local: Vec3,
        canvas_width: f64,
        canvas_height: f64,
    ) -> Vec2 {
        self.screen_from_map(
            picture.map_pixel_from_local(local),
            picture.map_pixel_from_local_xy(self.center_meters),
            canvas_width,
            canvas_height,
        )
    }

    /// The direction north points on screen, as a unit vector, y down.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn north_on_screen(&self) -> Vec2 {
        let radians = -self.rotation * std::f64::consts::PI / 180.0;
        Vec2::new(radians.sin() as f32, (-radians.cos()) as f32)
    }
}

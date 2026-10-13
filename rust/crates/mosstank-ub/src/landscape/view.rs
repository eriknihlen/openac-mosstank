//! The countryside map's view (`LandscapeMapView`): which part of the map
//! image the canvas shows and how large. Pure arithmetic over the canvas
//! size, the image size, a zoom in 0..1 and the image pixel at the canvas's
//! centre, so every conversion can be checked without a canvas.
//!
//! The image is the client's own map of the world: a position's fraction
//! across the image is its coordinate over twice [`MAX_COORDINATE`], north
//! at the top and the top row at [`NORTH_EDGE`], one texel short of the
//! west edge's reach, as the client's own marker rule has it.
//!
//! Zooming is anchored at a canvas point: the position under it before the
//! zoom is the position under it after. The scale grows geometrically with
//! the zoom so one step feels the same close up and far out.

use openac_plugin_compat::number;

/// How far, in map units, the map art reaches from the origin on each axis:
/// a coordinate's tenths over a 2048-wide span.
pub const MAX_COORDINATE: f64 = 102.4;

/// Where the top row of the image sits, in map units north: one texel short
/// of [`MAX_COORDINATE`] while the span stays the same (row zero is 102.3
/// north, the last row 102.5 south).
pub const NORTH_EDGE: f64 = 102.3;

/// How much one zoom step moves the zoom.
pub const ZOOM_STEP: f64 = 0.04;

/// The zoom the follow button will not go wider than.
pub const FOLLOW_ZOOM: f64 = 0.24;

/// The least room, canvas pixels, left between two neighbouring ruler
/// labels.
pub const LABEL_GAP: f64 = 6.0;

/// The ruler steps there are, map units, widest first.
const INCREMENTS: [f64; 9] = [50.0, 20.0, 10.0, 5.0, 2.0, 1.0, 0.5, 0.2, 0.1];

/// One tick on a coordinate ruler (`MapRulerTick`): where it sits along the
/// gutter, canvas pixels, and what it says.
#[derive(Debug, Clone, PartialEq)]
pub struct MapRulerTick {
    /// Canvas pixels along the gutter.
    pub pixel: f64,
    /// The coordinate and its hemisphere.
    pub label: String,
}

/// The view.
#[derive(Debug, Clone, PartialEq)]
pub struct LandscapeMapView {
    canvas_width: i32,
    canvas_height: i32,
    image_width: i32,
    image_height: i32,
    offset_x: f64,
    offset_y: f64,
    zoom: f64,
    /// Whether the view re-centres on the character as they move.
    pub follow: bool,
}

/// What a measure answers: a line of text's width and height.
pub type Measure<'a> = dyn FnMut(&str) -> (f64, f64) + 'a;

impl LandscapeMapView {
    /// A view of an image of this size on a canvas of that size, centred on
    /// the image, at the widest zoom but one step.
    #[must_use]
    pub fn new(canvas_width: i32, canvas_height: i32, image_width: i32, image_height: i32) -> Self {
        Self {
            canvas_width,
            canvas_height,
            image_width,
            image_height,
            offset_x: f64::from(image_width) / 2.0,
            offset_y: f64::from(image_height) / 2.0,
            zoom: 0.04,
            follow: false,
        }
    }

    /// The canvas's width, pixels.
    #[must_use]
    pub fn canvas_width(&self) -> i32 {
        self.canvas_width
    }

    /// The canvas's height, pixels.
    #[must_use]
    pub fn canvas_height(&self) -> i32 {
        self.canvas_height
    }

    /// The image's width, pixels.
    #[must_use]
    pub fn image_width(&self) -> i32 {
        self.image_width
    }

    /// The image's height, pixels.
    #[must_use]
    pub fn image_height(&self) -> i32 {
        self.image_height
    }

    /// The image column at the canvas's horizontal centre.
    #[must_use]
    pub fn offset_x(&self) -> f64 {
        self.offset_x
    }

    /// The image row at the canvas's vertical centre.
    #[must_use]
    pub fn offset_y(&self) -> f64 {
        self.offset_y
    }

    /// The zoom, from 0 (the whole world) to 1 (as close as it goes).
    #[must_use]
    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    /// Sets the zoom, held to 0..1.
    pub fn set_zoom(&mut self, value: f64) {
        self.zoom = value.clamp(0.0, 1.0);
    }

    /// How many canvas pixels one image pixel takes at this zoom.
    #[must_use]
    pub fn scale(&self) -> f64 {
        1.5f64.powf(self.zoom * 12.0) - 0.8
    }

    /// The canvas x of the image's left edge.
    #[must_use]
    pub fn canvas_left(&self) -> f64 {
        f64::from(self.canvas_width) / 2.0 - self.offset_x * self.scale()
    }

    /// The canvas y of the image's top edge.
    #[must_use]
    pub fn canvas_top(&self) -> f64 {
        f64::from(self.canvas_height) / 2.0 - self.offset_y * self.scale()
    }

    /// The image pixel a position lands on.
    #[must_use]
    pub fn to_image(&self, east_west: f64, north_south: f64) -> (f64, f64) {
        (
            (east_west + MAX_COORDINATE) / (MAX_COORDINATE * 2.0) * f64::from(self.image_width),
            (NORTH_EDGE - north_south) / (MAX_COORDINATE * 2.0) * f64::from(self.image_height),
        )
    }

    /// The canvas pixel a position lands on, fractional.
    #[must_use]
    pub fn to_canvas(&self, east_west: f64, north_south: f64) -> (f64, f64) {
        let (image_x, image_y) = self.to_image(east_west, north_south);
        let scale = self.scale();
        (
            self.canvas_left() + image_x * scale,
            self.canvas_top() + image_y * scale,
        )
    }

    /// The position under a canvas pixel: east-west, north-south.
    #[must_use]
    pub fn to_world(&self, x: f64, y: f64) -> (f64, f64) {
        let scale = self.scale();
        let image_x = (x - self.canvas_left()) / scale;
        let image_y = (y - self.canvas_top()) / scale;
        (
            image_x / f64::from(self.image_width) * MAX_COORDINATE * 2.0 - MAX_COORDINATE,
            NORTH_EDGE - image_y / f64::from(self.image_height) * MAX_COORDINATE * 2.0,
        )
    }

    /// Whether a canvas pixel lies on the canvas.
    #[must_use]
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= 0.0
            && y >= 0.0
            && x < f64::from(self.canvas_width)
            && y < f64::from(self.canvas_height)
    }

    /// Puts a position at the canvas's centre without touching the zoom.
    pub fn center_on(&mut self, east_west: f64, north_south: f64) {
        (self.offset_x, self.offset_y) = self.to_image(east_west, north_south);
    }

    /// Changes the zoom so that the position under a canvas point stays
    /// under it. While following, the view is re-centred on the character
    /// anyway, so only the zoom moves.
    pub fn zoom_about(&mut self, x: f64, y: f64, delta: f64) {
        let (east_west, north_south) = self.to_world(x, y);
        self.set_zoom(self.zoom + delta);
        if self.follow {
            return;
        }
        let (image_x, image_y) = self.to_image(east_west, north_south);
        let scale = self.scale();
        self.offset_x = image_x - (x - f64::from(self.canvas_width) / 2.0) / scale;
        self.offset_y = image_y - (y - f64::from(self.canvas_height) / 2.0) / scale;
    }

    /// Slides the map by canvas pixels, the way a drag does: dragging the
    /// map right shows what lies to its left. Panning lets go of the
    /// character.
    pub fn pan_by(&mut self, dx_pixels: f64, dy_pixels: f64) {
        self.follow = false;
        let scale = self.scale();
        self.offset_x -= dx_pixels / scale;
        self.offset_y -= dy_pixels / scale;
    }

    /// How far apart the ruler ticks are, map units, at a zoom; each axis
    /// widens it as far as its labels need ([`Self::horizontal_increment`]).
    #[must_use]
    pub fn coordinate_increment(zoom: f64) -> f64 {
        if zoom < 0.01 {
            50.0
        } else if zoom < 0.05 {
            20.0
        } else if zoom < 0.13 {
            10.0
        } else if zoom < 0.3 {
            5.0
        } else if zoom < 0.5 {
            2.0
        } else if zoom < 0.65 {
            1.0
        } else if zoom < 0.85 {
            0.5
        } else if zoom < 0.94 {
            0.2
        } else {
            0.1
        }
    }

    /// The next narrower step, or the step itself at the narrowest.
    #[must_use]
    pub fn narrower_increment(increment: f64) -> f64 {
        #[allow(clippy::float_cmp)]
        match INCREMENTS.iter().position(|step| *step == increment) {
            Some(at) if at + 1 < INCREMENTS.len() => INCREMENTS[at + 1],
            _ => increment,
        }
    }

    /// The next wider step, or the step itself at the widest.
    #[must_use]
    pub fn wider_increment(increment: f64) -> f64 {
        #[allow(clippy::float_cmp)]
        match INCREMENTS.iter().position(|step| *step == increment) {
            Some(at) if at > 0 => INCREMENTS[at - 1],
            _ => increment,
        }
    }

    /// Canvas pixels one map unit takes east to west at this zoom.
    #[must_use]
    pub fn pixels_per_unit_x(&self) -> f64 {
        f64::from(self.image_width) / (MAX_COORDINATE * 2.0) * self.scale()
    }

    /// Canvas pixels one map unit takes north to south at this zoom.
    #[must_use]
    pub fn pixels_per_unit_y(&self) -> f64 {
        f64::from(self.image_height) / (MAX_COORDINATE * 2.0) * self.scale()
    }

    /// The east-west step: the zoom's own, widened until the widest label it
    /// would draw fits between two ticks with a gap to spare. These labels
    /// are written across the gutter, so their width has to fit.
    pub fn horizontal_increment(&self, measure: &mut Measure<'_>) -> f64 {
        let min = self.to_world(0.0, 0.0).0;
        let max = self.to_world(f64::from(self.canvas_width), 0.0).0;
        self.fitting_increment(
            measure,
            self.pixels_per_unit_x(),
            &|_, c| axis_label(c, "F1", "W", "E"),
            |size| size.0,
            min,
            max,
        )
    }

    /// The north-south step: the zoom's own, widened until the labels'
    /// height fits (these labels stack).
    pub fn vertical_increment(&self, measure: &mut Measure<'_>) -> f64 {
        let min = self.to_world(0.0, f64::from(self.canvas_height)).1;
        let max = self.to_world(0.0, 0.0).1;
        self.fitting_increment(
            measure,
            self.pixels_per_unit_y(),
            &|increment, c| axis_label(c, if increment < 1.0 { "F1" } else { "F0" }, "S", "N"),
            |size| size.1,
            min,
            max,
        )
    }

    fn fitting_increment(
        &self,
        measure: &mut Measure<'_>,
        pixels_per_unit: f64,
        label: &dyn Fn(f64, f64) -> String,
        extent: fn((f64, f64)) -> f64,
        min: f64,
        max: f64,
    ) -> f64 {
        let mut increment = Self::coordinate_increment(self.zoom);
        loop {
            let mut widest = 0.0f64;
            let mut c = (min / increment).floor() * increment;
            while c <= max {
                widest = widest.max(extent(measure(&label(increment, c))));
                c += increment;
            }
            let wider = Self::wider_increment(increment);
            #[allow(clippy::float_cmp)]
            if increment * pixels_per_unit >= widest + LABEL_GAP || wider == increment {
                return increment;
            }
            increment = wider;
        }
    }

    /// The ticks along the top and bottom gutters, one per step across the
    /// visible east-west span, each with its coordinate and hemisphere.
    pub fn horizontal_ticks(&self, measure: &mut Measure<'_>) -> Vec<MapRulerTick> {
        let increment = self.horizontal_increment(measure);
        let min = self.to_world(0.0, 0.0).0;
        let max = self.to_world(f64::from(self.canvas_width), 0.0).0;
        let mut ticks = Vec::new();
        let mut c = (min / increment).floor() * increment;
        while c <= max {
            let pixel = self.to_canvas(c, 0.0).0;
            if (0.0..=f64::from(self.canvas_width)).contains(&pixel) {
                ticks.push(MapRulerTick {
                    pixel,
                    label: axis_label(c, "F1", "W", "E"),
                });
            }
            c += increment;
        }
        ticks
    }

    /// The ticks along the left and right gutters, one per step across the
    /// visible north-south span, the bottom one first.
    pub fn vertical_ticks(&self, measure: &mut Measure<'_>) -> Vec<MapRulerTick> {
        let increment = self.vertical_increment(measure);
        let max = self.to_world(0.0, 0.0).1;
        let min = self.to_world(0.0, f64::from(self.canvas_height)).1;
        let format = if increment < 1.0 { "F1" } else { "F0" };
        let mut ticks = Vec::new();
        let mut c = (min / increment).floor() * increment;
        while c <= max {
            let pixel = self.to_canvas(0.0, c).1;
            if (0.0..=f64::from(self.canvas_height)).contains(&pixel) {
                ticks.push(MapRulerTick {
                    pixel,
                    label: axis_label(c, format, "S", "N"),
                });
            }
            c += increment;
        }
        ticks
    }
}

/// A coordinate the way the in-game readout writes it: the magnitude in
/// `format`, then the hemisphere (none for zero).
///
/// # Panics
/// `format` is not one the compatibility kit prints: the formats are this
/// crate's constants.
#[must_use]
pub fn axis_label(value: f64, format: &str, negative: &str, positive: &str) -> String {
    let magnitude = number::format_f64(value.abs(), format).expect("a fixed-point format");
    let suffix = if value > 0.0 {
        positive
    } else if value < 0.0 {
        negative
    } else {
        ""
    };
    magnitude + suffix
}

/// A position the way the in-game readout writes it: north-south first, two
/// decimals (`PositionText`).
#[must_use]
pub fn position_text(east_west: f64, north_south: f64) -> String {
    format!(
        "{}, {}",
        axis_label(north_south, "F2", "S", "N"),
        axis_label(east_west, "F2", "W", "E")
    )
}

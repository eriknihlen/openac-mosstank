//! Whether one storey of a dungeon is drawn from a given height, and how
//! dim (`DungeonLayerShade`): the storey the height falls in is drawn as it
//! is, the one above is washed grey and half transparent so it reads as a
//! ceiling, and the ones below darken with distance until they are black.
//! Storeys more than ten metres above or twenty-four below are not drawn.

/// How a storey is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DungeonLayerShade {
    /// Whether the storey is drawn.
    pub visible: bool,
    /// What its colours are multiplied by, 0 to 1.
    pub brightness: f32,
    /// What its opacity is multiplied by, 0 to 1.
    pub opacity: f32,
}

/// Storeys are six metres tall.
pub const BAND_HEIGHT: f32 = 6.0;

impl DungeonLayerShade {
    /// A storey drawn exactly as it is.
    pub const PLAIN: Self = Self {
        visible: true,
        brightness: 1.0,
        opacity: 1.0,
    };

    /// A storey that is not drawn.
    pub const HIDDEN: Self = Self {
        visible: false,
        brightness: 0.0,
        opacity: 0.0,
    };

    /// The band a height falls in, in metres: bands are six metres tall and
    /// start three metres under their nominal height, so a height of five is
    /// in the band at six and a height of two in the one at nought.
    #[must_use]
    pub fn band_of(z: f32) -> f32 {
        ((z + BAND_HEIGHT / 2.0) / BAND_HEIGHT).floor() * BAND_HEIGHT
    }

    /// The shade of the storey at `layer_z` seen from `draw_z` (the height
    /// the map is drawn from: the player's, or the slider's).
    #[must_use]
    pub fn of(draw_z: f64, layer_z: f32, show_all_layers: bool) -> Self {
        if show_all_layers {
            return Self::PLAIN;
        }
        let above = draw_z - f64::from(layer_z);
        if !(-10.0..=24.0).contains(&above) {
            return Self::HIDDEN;
        }
        // The storey above the player's own: a grey wash, half transparent.
        if above < -3.0 {
            return Self {
                visible: true,
                brightness: 151.0 / 255.0,
                opacity: 121.0 / 255.0,
            };
        }
        // The player's own storey.
        if above.abs() < 3.0 {
            return Self::PLAIN;
        }
        // Below: dim by four tenths per storey, to black at two and a half
        // storeys down.
        #[allow(clippy::cast_possible_truncation)]
        let dim = (above.abs() / f64::from(BAND_HEIGHT) * 0.4).min(1.0) as f32;
        Self {
            visible: true,
            brightness: 1.0 - dim,
            opacity: 1.0,
        }
    }

    /// The height a slider position picks: nought is the lowest storey and a
    /// hundred the highest.
    #[must_use]
    pub fn draw_z_from_slider(min_z: f32, max_z: f32, percent: i32) -> f64 {
        f64::from(min_z) + f64::from(max_z - min_z) * (f64::from(percent.clamp(0, 100)) / 100.0)
    }

    /// The slider position that shows a height; nought for a dungeon of one
    /// storey.
    #[must_use]
    pub fn slider_from_draw_z(min_z: f32, max_z: f32, draw_z: f64) -> i32 {
        let depth = f64::from(max_z - min_z);
        if depth <= 0.0 {
            return 0;
        }
        let share = ((draw_z - f64::from(min_z)) / depth).clamp(0.0, 1.0);
        #[allow(clippy::cast_possible_truncation)]
        let percent = (share * 100.0).round_ties_even() as i32;
        percent
    }
}

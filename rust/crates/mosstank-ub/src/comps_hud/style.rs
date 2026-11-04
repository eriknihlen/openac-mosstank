//! The Comps HUD's look, taken from the plugin theme the player picked, so
//! the strip reads as one of the plugin's themed windows: the theme's face,
//! edge, text and caption colours, its font, and its rounded corners. Under
//! Classic, where themed windows draw unthemed, it takes the colours
//! MossTank's windows fall back to there, square corners, and the interface
//! font.
//!
//! A theme has no colour of its own for a warning (its palette is the face,
//! fields, edges, text, captions, the accent and the selection), so a count
//! below its warn level is red: a light red on the dark themes and Classic,
//! a dark red on a light theme (Parchment), each readable on that face.

use ac_plugin_abi::ui::{Color, ThemeInfo, theme_metrics};

/// The text size of the counts, pixels.
pub const FONT_SIZE: f32 = 11.0;

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

/// A warning on a dark face.
pub const WARNING_ON_DARK: Color = rgba(0xFF, 0x6B, 0x6B, 0xFF);
/// A warning on a light face.
pub const WARNING_ON_LIGHT: Color = rgba(0xB3, 0x26, 0x1E, 0xFF);

/// The colours, corners and font one theme draws the strip in.
#[derive(Debug, Clone, PartialEq)]
pub struct HudStyle {
    /// The strip's face.
    pub background: Color,
    /// Its edge and the line's divider.
    pub border: Color,
    /// A button's face.
    pub button: Color,
    /// The counts.
    pub text: Color,
    /// The buttons' marks and an empty list's dash.
    pub muted: Color,
    /// A count below its warn level.
    pub warning: Color,
    /// The strip's corner radius, pixels (0 under Classic).
    pub radius: f64,
    /// A button's corner radius, pixels.
    pub button_radius: f64,
    /// Whether a theme applies: the counts are set in the bundled font, as
    /// themed windows are, not in the interface font.
    pub themed: bool,
    /// Whether the counts are outlined (the interface font is, on the
    /// game's art).
    pub outline: bool,
}

impl Default for HudStyle {
    fn default() -> Self {
        Self::classic()
    }
}

impl HudStyle {
    /// Classic: the colours MossTank's windows fall back to there
    /// (`theme:field|#FF0C0906`, `theme:border|#FF8C7B4E`,
    /// `theme:text|#FFE8DEC3`, `theme:muted|#FFC7B98F`), square corners.
    #[must_use]
    pub fn classic() -> Self {
        Self {
            background: rgba(0x0C, 0x09, 0x06, 0xFF),
            border: rgba(0x8C, 0x7B, 0x4E, 0xFF),
            button: rgba(0x26, 0x20, 0x16, 0xFF),
            text: rgba(0xE8, 0xDE, 0xC3, 0xFF),
            muted: rgba(0xC7, 0xB9, 0x8F, 0xFF),
            warning: WARNING_ON_DARK,
            radius: 0.0,
            button_radius: 0.0,
            themed: false,
            outline: true,
        }
    }

    /// The style of the theme the player picked.
    #[must_use]
    pub fn from_theme(theme: &ThemeInfo) -> Self {
        let Some(palette) = &theme.palette else {
            return Self::classic();
        };
        Self {
            background: palette.background.clone(),
            border: palette.border.clone(),
            button: palette.field.clone(),
            text: palette.text.clone(),
            muted: palette.muted.clone(),
            warning: if theme.is_light() {
                WARNING_ON_LIGHT
            } else {
                WARNING_ON_DARK
            },
            radius: f64::from(theme_metrics::CONTROL_RADIUS),
            button_radius: 2.0,
            themed: true,
            outline: false,
        }
    }
}

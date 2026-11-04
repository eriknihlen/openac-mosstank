//! The Comps HUD's tooltip: the name of the item the pointer is over, or
//! what the button under it does, in a small box in the theme's colours.
//!
//! The host offers a canvas no tooltip of its own, so the tip is painted on
//! a second canvas beside the strip, which takes no pointer input: a click
//! on its empty part reaches whatever lies beneath. A plugin cannot learn
//! the screen's size, so the tip goes where the strip leaves room for it:
//! beside a column (to its left when the strip stands far enough from the
//! screen's left edge, the box then right-aligned against the strip), and
//! above a line (below it near the screen's top).

use ac_plugin_abi::ui::Color;

use super::layout::{HudButton, HudGeometry, HudLayout, HudRect};
use super::paint::fill_rounded;
use super::style::HudStyle;
use crate::canvas::{MapFont, MapPainter};

/// The tooltip canvas's id.
pub const TIP_CANVAS_ID: &str = "comps-hud-tip";
/// The tooltip canvas's width: room for a long item name.
pub const TIP_WIDTH: i32 = 260;
/// The tooltip canvas's height.
pub const TIP_HEIGHT: i32 = 18;
/// The room between the strip and the tip.
const GAP: i32 = 4;
/// The room between the box's edge and its text.
const TEXT_PADDING: f64 = 5.0;

/// A tooltip shown: what it says, where its canvas stands, and which side
/// of its canvas the box keeps to.
#[derive(Debug, Clone, PartialEq)]
pub struct Tip {
    /// The text.
    pub text: String,
    /// The tip canvas's place, pixels from the screen's top-left.
    pub offset: (i32, i32),
    /// The box keeps to the canvas's right edge (it stands left of the
    /// strip).
    pub right_aligned: bool,
}

/// What a button's tip says: the layout button names the layout it
/// switches to.
#[must_use]
pub fn button_text(button: HudButton, layout: HudLayout) -> &'static str {
    match (button, layout) {
        (HudButton::Layout, HudLayout::Column) => "Line layout",
        (HudButton::Layout, HudLayout::Line) => "Column layout",
        (HudButton::Edit, _) => "Edit items",
        (HudButton::Hide, _) => "Hide",
    }
}

/// Where a tip for `target` (a rectangle of the strip) stands, for a strip
/// at `strip`.
#[must_use]
pub fn place(strip: (i32, i32), geometry: &HudGeometry, target: HudRect, text: &str) -> Tip {
    let (x, y) = strip;
    #[allow(clippy::cast_possible_truncation)]
    let (target_x, target_y) = (target.0.round() as i32, target.1.round() as i32);
    #[allow(clippy::cast_possible_truncation)]
    let middle = target_y + ((target.3.round() as i32) - TIP_HEIGHT) / 2;
    let (offset, right_aligned) = match geometry.layout {
        HudLayout::Column if x >= TIP_WIDTH + GAP => ((x - GAP - TIP_WIDTH, y + middle), true),
        HudLayout::Column => ((x + geometry.width + GAP, y + middle), false),
        HudLayout::Line if y >= TIP_HEIGHT + GAP => ((x + target_x, y - GAP - TIP_HEIGHT), false),
        HudLayout::Line => ((x + target_x, y + geometry.height + GAP), false),
    };
    Tip {
        text: text.to_owned(),
        offset,
        right_aligned,
    }
}

/// Paints the tip: a box as wide as its text (no wider than the canvas),
/// in the theme's face and edge, the text in its text colour.
pub(super) fn paint(painter: &mut dyn MapPainter, tip: &Tip, style: &HudStyle, font: MapFont) {
    painter.clear(&Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    });
    let (text_width, text_height) = painter.measure_text_in(font, &tip.text);
    let canvas = f64::from(TIP_WIDTH);
    let height = f64::from(TIP_HEIGHT);
    let width = (text_width + 2.0 * TEXT_PADDING).ceil().min(canvas);
    let left = if tip.right_aligned {
        canvas - width
    } else {
        0.0
    };
    let face = (left, 0.0, width, height);
    let radius = style.radius.min(4.0);
    fill_rounded(painter, face, radius, &style.border);
    fill_rounded(
        painter,
        (left + 1.0, 1.0, width - 2.0, height - 2.0),
        (radius - 1.0).max(0.0),
        &style.background,
    );
    let top = ((height - text_height) / 2.0).max(0.0).round();
    painter.push_clip((left + 1.0, 1.0, width - 2.0, height - 2.0));
    painter.draw_text_in(
        font,
        &tip.text,
        (left + TEXT_PADDING, top),
        &style.text,
        style.outline,
    );
    painter.pop_clip();
}

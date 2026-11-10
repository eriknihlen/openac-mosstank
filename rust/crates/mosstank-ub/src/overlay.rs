//! The look of what MossTank draws straight on the game, with no frame and
//! no background (the Comps HUD, the Mag tab's mana overlay): text in the
//! interface font, outlined so it reads on any ground, words in white and
//! numbers in gold; a warning in light red; and small marks (a close
//! cross, a button's sign) as a white line over a dark edge, for the same
//! reason.

use ac_plugin_abi::ui::Color;

use crate::canvas::MapPainter;

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

/// Words: names, a tooltip, an empty list's line.
pub const TEXT: Color = rgba(255, 255, 255, 255);
/// Numbers: counts, mana, time left.
pub const NUMBERS: Color = rgba(255, 215, 113, 255);
/// A number that warns (a count below its warn level).
pub const WARNING: Color = rgba(255, 107, 107, 255);
/// A mark's dark edge.
pub const MARK_EDGE: Color = rgba(20, 14, 8, 230);
/// A mark's light line.
pub const MARK: Color = rgba(255, 255, 255, 255);
/// An image drawn as it is.
pub const NO_TINT: Color = rgba(255, 255, 255, 255);
/// How thick a mark's edge is, pixels.
pub const EDGE_THICKNESS: f32 = 3.5;
/// How thick a mark's line is, pixels.
pub const MARK_THICKNESS: f32 = 1.5;

/// A straight line: from, to (canvas pixels).
pub type Segment = ((f64, f64), (f64, f64));

/// A mark of straight lines: all of them in the dark edge, then all of
/// them in the light line on top, so no edge covers a line.
pub fn marked_lines(painter: &mut dyn MapPainter, lines: &[Segment]) {
    for (color, thickness) in [(MARK_EDGE, EDGE_THICKNESS), (MARK, MARK_THICKNESS)] {
        for (from, to) in lines {
            painter.draw_line(*from, *to, &color, thickness);
        }
    }
}

/// A mark of dots, each two pixels across, centred on the points given.
pub fn marked_dots(painter: &mut dyn MapPainter, centres: &[(f64, f64)]) {
    for (cx, cy) in centres {
        painter.fill_rect((cx - 2.0, cy - 2.0, 4.0, 4.0), &MARK_EDGE);
    }
    for (cx, cy) in centres {
        painter.fill_rect((cx - 1.0, cy - 1.0, 2.0, 2.0), &MARK);
    }
}

/// A × in a box (x, y, width, height), two pixels in from its sides.
pub fn cross(painter: &mut dyn MapPainter, rect: (f64, f64, f64, f64)) {
    let (x, y, w, h) = rect;
    let (left, top, right, bottom) = (x + 2.0, y + 2.0, x + w - 2.0, y + h - 2.0);
    marked_lines(
        painter,
        &[
            ((left, top), (right, bottom)),
            ((left, bottom), (right, top)),
        ],
    );
}

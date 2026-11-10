//! Painting the Comps HUD straight on the game, as the mana overlay is
//! ([`crate::overlay`]): no face, edge or background, each item's icon and
//! its count in outlined text (gold, or red below its warn level), and the
//! three buttons as outlined marks only while the pointer is over the HUD.

use ac_plugin_abi::ui::Color;

use super::layout::{HudButton, HudGeometry, HudLayout, HudRect};
use crate::canvas::{MapImage, MapPainter};
use crate::overlay::{self, NO_TINT, NUMBERS, TEXT, WARNING};

const CLEAR: Color = Color {
    r: 0,
    g: 0,
    b: 0,
    a: 0,
};

/// What one item shows.
#[derive(Debug, Clone, Copy)]
pub(super) struct ItemPaint<'a> {
    /// Its icon, or no image.
    pub(super) icon: MapImage,
    /// Its count as written.
    pub(super) count: &'a str,
    /// Whether the count is below its warn level.
    pub(super) warn: bool,
}

/// Paints the strip: every item, then the buttons while `hovered`.
pub(super) fn paint(
    painter: &mut dyn MapPainter,
    geometry: &HudGeometry,
    items: &[ItemPaint<'_>],
    hovered: bool,
) {
    painter.clear(&CLEAR);
    for (index, slot) in geometry.items.iter().enumerate() {
        let Some(item) = items.get(index) else {
            // The empty list's one slot: a dash where a count would be.
            draw_count(painter, slot.count, "-", &TEXT);
            continue;
        };
        if item.icon.is_valid() {
            painter.draw_image(item.icon, slot.icon, &NO_TINT);
        }
        let color = if item.warn { &WARNING } else { &NUMBERS };
        draw_count(painter, slot.count, item.count, color);
    }
    if hovered {
        for (button, rect) in &geometry.buttons {
            draw_mark(painter, *button, *rect, geometry.layout);
        }
    }
}

/// A count in its box, from its left (just after the icon), centred top to
/// bottom, outlined; kept to its box widened by the outline's pixel.
fn draw_count(painter: &mut dyn MapPainter, rect: HudRect, text: &str, color: &Color) {
    let (left, y, width, height) = rect;
    let (_, text_height) = painter.measure_text(text);
    let top = y + ((height - text_height) / 2.0).max(0.0);
    painter.push_clip((left - 1.0, y, width + 2.0, height));
    painter.draw_text(text, (left.round(), top.round()), color, true);
    painter.pop_clip();
}

/// A button's mark: the layout it switches to (three dots side by side for
/// the line, stacked for the column), a pencil for Edit items, a × for
/// hide.
fn draw_mark(painter: &mut dyn MapPainter, button: HudButton, rect: HudRect, layout: HudLayout) {
    let (x, y, width, height) = rect;
    let (cx, cy) = (x + width / 2.0, y + height / 2.0);
    match button {
        HudButton::Layout => {
            let dots = [-3.0, 0.0, 3.0].map(|step| match layout.other() {
                HudLayout::Line => (cx + step, cy),
                HudLayout::Column => (cx, cy + step),
            });
            overlay::marked_dots(painter, &dots);
        }
        HudButton::Edit => {
            let reach = (width / 2.0 - 2.0).max(1.0);
            overlay::marked_lines(
                painter,
                &[
                    // The shaft, from the point at the bottom left.
                    (
                        (cx - reach, cy + reach),
                        (cx + reach - 1.0, cy - reach + 1.0),
                    ),
                    // The pencil's end, across its top.
                    (
                        (cx + reach - 3.0, cy - reach),
                        (cx + reach, cy - reach + 3.0),
                    ),
                ],
            );
        }
        HudButton::Hide => overlay::cross(painter, rect),
    }
}

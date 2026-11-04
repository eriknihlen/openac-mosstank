//! Painting the Comps HUD: the face with its edge and corners, each item's
//! icon and count, and the three buttons, in the theme's colours
//! ([`HudStyle`]).
//!
//! The canvas has rectangles and lines but no curves, so a rounded corner
//! is filled a pixel row at a time, each row inset by how far the circle
//! of the corner's radius stands in from the edge at that height. The edge
//! is the face's shape filled in the edge colour, with the face filled one
//! pixel smaller on top of it.

use ac_plugin_abi::ui::Color;

use super::layout::{HudButton, HudGeometry, HudLayout, HudRect};
use super::style::HudStyle;
use crate::canvas::{MapFont, MapImage, MapPainter};

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

const CLEAR: Color = rgba(0, 0, 0, 0);
const WHITE: Color = rgba(255, 255, 255, 255);

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

/// Paints the strip: the face, every item, then the buttons.
pub(super) fn paint(
    painter: &mut dyn MapPainter,
    geometry: &HudGeometry,
    style: &HudStyle,
    font: MapFont,
    items: &[ItemPaint<'_>],
) {
    painter.clear(&CLEAR);
    let face = (
        0.0,
        0.0,
        f64::from(geometry.width),
        f64::from(geometry.height),
    );
    fill_rounded(painter, face, style.radius, &style.border);
    fill_rounded(
        painter,
        inset(face, 1.0),
        (style.radius - 1.0).max(0.0),
        &style.background,
    );
    match geometry.layout {
        HudLayout::Column => {
            // The title line ends in a hairline across the strip.
            let (x, y, width, _) = geometry.divider;
            painter.fill_rect((x + 1.0, y, width - 2.0, 1.0), &style.border);
        }
        HudLayout::Line => {
            let (x, y, _, height) = geometry.divider;
            painter.fill_rect((x, y, 1.0, height), &style.border);
        }
    }

    for (index, slot) in geometry.items.iter().enumerate() {
        let Some(item) = items.get(index) else {
            // The empty list's one slot: a dash where a count would be.
            draw_count(
                painter,
                geometry.layout,
                slot.count,
                "-",
                &style.muted,
                style,
                font,
            );
            continue;
        };
        if item.icon.is_valid() {
            painter.draw_image(item.icon, slot.icon, &WHITE);
        }
        let color = if item.warn {
            &style.warning
        } else {
            &style.text
        };
        draw_count(
            painter,
            geometry.layout,
            slot.count,
            item.count,
            color,
            style,
            font,
        );
    }

    for (button, rect) in &geometry.buttons {
        fill_rounded(painter, *rect, style.button_radius, &style.button);
        draw_mark(painter, *button, *rect, geometry.layout, &style.muted);
    }
}

/// A count in its box: right-aligned in the column, from the left on the
/// line, centred top to bottom.
fn draw_count(
    painter: &mut dyn MapPainter,
    layout: HudLayout,
    rect: HudRect,
    text: &str,
    color: &Color,
    style: &HudStyle,
    font: MapFont,
) {
    let (x, y, width, height) = rect;
    let (text_width, text_height) = painter.measure_text_in(font, text);
    let left = match layout {
        HudLayout::Column => x + width - text_width,
        HudLayout::Line => x,
    };
    let top = y + ((height - text_height) / 2.0).max(0.0);
    painter.push_clip(rect);
    painter.draw_text_in(
        font,
        text,
        (left.round(), top.round()),
        color,
        style.outline,
    );
    painter.pop_clip();
}

/// A button's mark, in lines and small squares: the layout it switches to
/// (three dots side by side for the line, stacked for the column), a
/// pencil for Edit items, a cross for hide.
fn draw_mark(
    painter: &mut dyn MapPainter,
    button: HudButton,
    rect: HudRect,
    layout: HudLayout,
    color: &Color,
) {
    let (x, y, width, height) = rect;
    let (cx, cy) = (x + width / 2.0, y + height / 2.0);
    match button {
        HudButton::Layout => {
            for step in [-3.0, 0.0, 3.0] {
                let (dx, dy) = match layout.other() {
                    HudLayout::Line => (step, 0.0),
                    HudLayout::Column => (0.0, step),
                };
                painter.fill_rect((cx + dx - 1.0, cy + dy - 1.0, 2.0, 2.0), color);
            }
        }
        HudButton::Edit => {
            let reach = (width / 2.0 - 2.5).max(1.0);
            painter.draw_line(
                (cx - reach, cy + reach),
                (cx + reach - 1.0, cy - reach + 1.0),
                color,
                2.0,
            );
            // The pencil's end, across its top.
            painter.draw_line(
                (cx + reach - 3.0, cy - reach),
                (cx + reach, cy - reach + 3.0),
                color,
                1.0,
            );
        }
        HudButton::Hide => {
            let reach = (width / 2.0 - 2.5).max(1.0);
            painter.draw_line(
                (cx - reach, cy - reach),
                (cx + reach, cy + reach),
                color,
                1.5,
            );
            painter.draw_line(
                (cx + reach, cy - reach),
                (cx - reach, cy + reach),
                color,
                1.5,
            );
        }
    }
}

fn inset(rect: HudRect, by: f64) -> HudRect {
    let (x, y, width, height) = rect;
    (
        x + by,
        y + by,
        (width - 2.0 * by).max(0.0),
        (height - 2.0 * by).max(0.0),
    )
}

/// Fills a rectangle with rounded corners of `radius` pixels (none at 0).
pub(super) fn fill_rounded(
    painter: &mut dyn MapPainter,
    rect: HudRect,
    radius: f64,
    color: &Color,
) {
    let (x, y, width, height) = rect;
    let radius = radius.min(width / 2.0).min(height / 2.0).floor();
    if radius < 1.0 {
        painter.fill_rect(rect, color);
        return;
    }
    // Each pixel row of a corner, inset to where the circle's edge
    // crosses that row's middle.
    let rows = radius_rows(radius);
    for (row, indent) in rows.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let row = row as f64;
        let span = (width - 2.0 * indent).max(0.0);
        painter.fill_rect((x + indent, y + row, span, 1.0), color);
        painter.fill_rect((x + indent, y + height - 1.0 - row, span, 1.0), color);
    }
    painter.fill_rect((x, y + radius, width, height - 2.0 * radius), color);
}

/// How far each of a corner's pixel rows stands in, top row first.
fn radius_rows(radius: f64) -> Vec<f64> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = radius as usize;
    (0..count)
        .map(|row| {
            #[allow(clippy::cast_precision_loss)]
            let rise = radius - row as f64 - 0.5;
            (radius - (radius * radius - rise * rise).max(0.0).sqrt()).round()
        })
        .collect()
}

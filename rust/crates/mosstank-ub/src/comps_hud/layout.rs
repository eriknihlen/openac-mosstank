//! Where everything sits on the Comps HUD's canvas: its size, each item's
//! icon and count, and the three buttons, in interface pixels at 100% UI
//! scale (the client scales the canvas with the rest of the plugin UI).
//!
//! Two layouts. A column is a strip 50 pixels wide: a thin title line with
//! the buttons at its right, then a row per item, the icon at the left and
//! the count right-aligned. A line is one row 20 pixels tall: icon, count,
//! icon, count, then a thin divider and the buttons. Each count has the
//! room it measures in the HUD's font: the column widens for a count too
//! wide for its box, and each count on the line is as wide as it reads.

/// How the strip lays its items out; kept per character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HudLayout {
    /// A narrow column, one row per item.
    #[default]
    Column,
    /// One row, the items side by side.
    Line,
}

impl HudLayout {
    /// The name the saved list keeps it under.
    #[must_use]
    pub fn saved_name(self) -> &'static str {
        match self {
            Self::Column => "Column",
            Self::Line => "Line",
        }
    }

    /// The layout a saved name stands for, ignoring case.
    #[must_use]
    pub fn from_saved_name(name: &str) -> Option<Self> {
        [Self::Column, Self::Line]
            .into_iter()
            .find(|layout| layout.saved_name().eq_ignore_ascii_case(name.trim()))
    }

    /// The other layout: what the layout button switches to.
    #[must_use]
    pub fn other(self) -> Self {
        match self {
            Self::Column => Self::Line,
            Self::Line => Self::Column,
        }
    }
}

/// One of the strip's buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudButton {
    /// Switches between the column and the line.
    Layout,
    /// Opens the Edit items window.
    Edit,
    /// Hides the HUD.
    Hide,
}

/// The buttons in the order they stand, left to right.
pub const BUTTONS: [HudButton; 3] = [HudButton::Layout, HudButton::Edit, HudButton::Hide];

/// A rectangle in the canvas's pixels: x, y, width, height.
pub type HudRect = (f64, f64, f64, f64);

/// The column's width.
pub const COLUMN_WIDTH: i32 = 50;
/// The column's title line, which holds the buttons.
pub const TITLE_HEIGHT: i32 = 12;
/// One item's row in the column.
pub const ROW_HEIGHT: i32 = 16;
/// A button in the column's title line.
pub const COLUMN_BUTTON: i32 = 10;
/// The line's height.
pub const LINE_HEIGHT: i32 = 20;
/// A button on the line.
pub const LINE_BUTTON: i32 = 12;
/// An item's icon.
pub const ICON_SIZE: i32 = 14;
/// The room a count's character is given before the count is measured.
pub const DIGIT_WIDTH: i32 = 7;
/// The room between the edge and what is inside.
const PADDING: i32 = 2;
/// The room between two buttons.
const BUTTON_GAP: i32 = 2;
/// The room between an icon and its count.
const ICON_GAP: i32 = 2;
/// The room between one item and the next on the line.
const ITEM_GAP: i32 = 5;
/// The room on either side of the line's divider.
const DIVIDER_GAP: i32 = 4;
/// The line's room at either end.
const LINE_PADDING: i32 = 4;

/// One item's place: its icon and the box its count is drawn in (right
/// aligned in the column, from the left on the line).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemSlot {
    /// The whole item: what a press on it lands on.
    pub cell: HudRect,
    /// The icon.
    pub icon: HudRect,
    /// The count's box.
    pub count: HudRect,
}

/// The strip as one layout lays it out.
#[derive(Debug, Clone, PartialEq)]
pub struct HudGeometry {
    /// Which layout.
    pub layout: HudLayout,
    /// The canvas's width.
    pub width: i32,
    /// The canvas's height.
    pub height: i32,
    /// Each item's place, in list order; an empty list has one empty slot.
    pub items: Vec<ItemSlot>,
    /// The buttons, in [`BUTTONS`] order.
    pub buttons: [(HudButton, HudRect); 3],
    /// The column's title line, or the line's divider (a vertical line at
    /// its x, from its top to its bottom).
    pub divider: HudRect,
}

impl HudGeometry {
    /// The strip laid out for items whose counts are `count_widths` pixels
    /// wide as drawn; an empty list keeps one slot so the strip is never
    /// nothing but buttons. The column widens past 50 pixels only for a
    /// count too wide for its box; on the line each count has its own room.
    #[must_use]
    pub fn new(layout: HudLayout, count_widths: &[f64]) -> Self {
        match layout {
            HudLayout::Column => Self::column(count_widths),
            HudLayout::Line => Self::line(count_widths),
        }
    }

    fn column(count_widths: &[f64]) -> Self {
        let rows = i32::try_from(count_widths.len().max(1)).unwrap_or(i32::MAX);
        let height = TITLE_HEIGHT
            .saturating_add(rows.saturating_mul(ROW_HEIGHT))
            .saturating_add(PADDING);
        let count_x = PADDING + 1 + ICON_SIZE + ICON_GAP;
        let widest = whole(count_widths.iter().copied().fold(0.0, f64::max));
        let width = COLUMN_WIDTH.max(count_x + widest + PADDING + 2);
        let slots = (0..rows)
            .map(|row| {
                let top = f64::from(TITLE_HEIGHT + row * ROW_HEIGHT);
                let icon_top = top + f64::from(ROW_HEIGHT - ICON_SIZE) / 2.0;
                ItemSlot {
                    cell: (0.0, top, f64::from(width), f64::from(ROW_HEIGHT)),
                    icon: (
                        f64::from(PADDING + 1),
                        icon_top,
                        f64::from(ICON_SIZE),
                        f64::from(ICON_SIZE),
                    ),
                    count: (
                        f64::from(count_x),
                        top,
                        f64::from(width - PADDING - 2 - count_x),
                        f64::from(ROW_HEIGHT),
                    ),
                }
            })
            .collect();
        let button_top = f64::from(TITLE_HEIGHT - COLUMN_BUTTON) / 2.0;
        let mut x = width - PADDING - 3 * COLUMN_BUTTON - 2 * BUTTON_GAP;
        let buttons = BUTTONS.map(|button| {
            let rect = (
                f64::from(x),
                button_top,
                f64::from(COLUMN_BUTTON),
                f64::from(COLUMN_BUTTON),
            );
            x += COLUMN_BUTTON + BUTTON_GAP;
            (button, rect)
        });
        Self {
            layout: HudLayout::Column,
            width,
            height,
            items: slots,
            buttons,
            divider: (0.0, f64::from(TITLE_HEIGHT), f64::from(width), 0.0),
        }
    }

    fn line(count_widths: &[f64]) -> Self {
        let empty = [f64::from(DIGIT_WIDTH)];
        let count_widths = if count_widths.is_empty() {
            &empty[..]
        } else {
            count_widths
        };
        let icon_top = f64::from(LINE_HEIGHT - ICON_SIZE) / 2.0;
        let mut x = LINE_PADDING;
        let mut items = Vec::with_capacity(count_widths.len());
        for (index, count_width) in count_widths.iter().enumerate() {
            if index > 0 {
                x += ITEM_GAP;
            }
            let count_width = whole(*count_width).max(1);
            let start = x;
            let count_x = x + ICON_SIZE + ICON_GAP;
            x = count_x.saturating_add(count_width);
            items.push(ItemSlot {
                cell: (
                    f64::from(start),
                    0.0,
                    f64::from(x - start),
                    f64::from(LINE_HEIGHT),
                ),
                icon: (
                    f64::from(start),
                    icon_top,
                    f64::from(ICON_SIZE),
                    f64::from(ICON_SIZE),
                ),
                count: (
                    f64::from(count_x),
                    0.0,
                    f64::from(count_width),
                    f64::from(LINE_HEIGHT),
                ),
            });
        }
        let divider_x = x + DIVIDER_GAP;
        let mut x = divider_x + 1 + DIVIDER_GAP;
        let button_top = f64::from(LINE_HEIGHT - LINE_BUTTON) / 2.0;
        let buttons = BUTTONS.map(|button| {
            let rect = (
                f64::from(x),
                button_top,
                f64::from(LINE_BUTTON),
                f64::from(LINE_BUTTON),
            );
            x += LINE_BUTTON + BUTTON_GAP;
            (button, rect)
        });
        let width = x - BUTTON_GAP + LINE_PADDING;
        Self {
            layout: HudLayout::Line,
            width,
            height: LINE_HEIGHT,
            items,
            buttons,
            divider: (
                f64::from(divider_x),
                f64::from(LINE_PADDING),
                0.0,
                f64::from(LINE_HEIGHT - 2 * LINE_PADDING),
            ),
        }
    }

    /// The canvas's size: width and height.
    #[must_use]
    pub fn size(&self) -> (i32, i32) {
        (self.width, self.height)
    }

    /// The button at a point of the canvas, if any.
    #[must_use]
    pub fn button_at(&self, point: (f64, f64)) -> Option<HudButton> {
        self.buttons
            .iter()
            .find(|(_, rect)| contains(*rect, point))
            .map(|(button, _)| *button)
    }

    /// Where a button sits.
    #[must_use]
    pub fn button(&self, button: HudButton) -> HudRect {
        self.buttons
            .iter()
            .find(|(held, _)| *held == button)
            .map_or((0.0, 0.0, 0.0, 0.0), |(_, rect)| *rect)
    }

    /// The item at a point of the canvas, if any (in list order).
    #[must_use]
    pub fn item_at(&self, point: (f64, f64)) -> Option<usize> {
        self.items
            .iter()
            .position(|slot| contains(slot.cell, point))
    }
}

/// Whether a rectangle holds a point (its left and top edges in, its right
/// and bottom out).
#[must_use]
pub fn contains(rect: HudRect, point: (f64, f64)) -> bool {
    let (x, y, width, height) = rect;
    point.0 >= x && point.0 < x + width && point.1 >= y && point.1 < y + height
}

/// How wide a count is taken to be before the client has measured it in
/// the HUD's font: [`DIGIT_WIDTH`] a character.
#[must_use]
pub fn estimated_width(text: &str) -> f64 {
    let characters = u32::try_from(text.chars().count().max(1)).unwrap_or(u32::MAX);
    f64::from(characters) * f64::from(DIGIT_WIDTH)
}

/// A measured width as whole pixels, rounded up.
#[allow(clippy::cast_possible_truncation)]
fn whole(width: f64) -> i32 {
    width.ceil().clamp(0.0, f64::from(i32::MAX / 2)) as i32
}

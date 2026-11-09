//! The mana overlay: the Mana page's rows drawn straight on the game, on a
//! canvas with no frame and no background, while "Show mana overlay" is
//! ticked. Each row is the item's icon, its name, the state icon, its mana
//! and the time it lasts, in outlined text so it reads on any ground; the
//! rows, their order and their texts are the Mana page's
//! ([`crate::mana::ManaView`]).
//!
//! The canvas is as tall as its rows: the host sizes a canvas once, so a
//! row more or less takes a new canvas in its place (never during a
//! press). A left drag anywhere moves it, following the pointer; where it
//! is let go is kept ([`OverlayAction::Moved`]). With the pointer over it a
//! small × shows at its top-right; a click on it closes the overlay
//! ([`OverlayAction::Closed`]). It is painted again only when what it
//! shows changed. Not Mag-Tools': MossTank's own.

use std::collections::HashMap;

use ac_plugin_abi::ui::{Color, PointerEvent};
use mosstank_ub::canvas::{BUTTON_LEFT, MapCanvasSpec, MapImage, MapPainter, MapUi};
use mosstank_ub::overlay::{self, NO_TINT, NUMBERS, TEXT};

use crate::mana::ManaRow;

/// The canvas's id.
pub const CANVAS_ID: &str = "mag-mana-overlay";
/// A row's height, pixels (a 16 px icon and a line of text).
pub const ROW_HEIGHT: i32 = 18;
/// Where each part of a row starts, pixels from the canvas's left: the
/// Mana page's column widths (fitted to the client's list font).
pub const NAME_X: f64 = 20.0;
/// The name's width.
pub const NAME_WIDTH: f64 = 148.0;
/// The state icon.
pub const STATE_X: f64 = NAME_X + NAME_WIDTH + 2.0;
/// The mana.
pub const MANA_X: f64 = STATE_X + 20.0;
/// The mana's width.
pub const MANA_WIDTH: f64 = 78.0;
/// The time left.
pub const TIME_X: f64 = MANA_X + MANA_WIDTH;
/// The time's width.
pub const TIME_WIDTH: f64 = 44.0;
/// The close box: x, y, width, height (the first row's right end).
pub const CLOSE_BOX: (f64, f64, f64, f64) = (TIME_X + TIME_WIDTH + 2.0, 2.0, 12.0, 12.0);
/// The canvas's width.
#[allow(clippy::cast_possible_truncation)]
pub const WIDTH: i32 = (CLOSE_BOX.0 + CLOSE_BOX.2) as i32 + 2;
/// The most rows drawn (a taller list is cut).
pub const MAX_ROWS: usize = 40;
/// Where the overlay starts before it was ever moved, pixels from the
/// screen's top-left.
pub const DEFAULT_PLACE: (i32, i32) = (420, 150);
/// The farthest from the screen's top-left a kept place may stand (a
/// plugin cannot learn the screen's size).
pub const MAX_PLACE: i32 = 16_384;
/// Said when nothing is worn.
pub const EMPTY_TEXT: &str = "No worn items.";

/// The canvas's height for `rows` rows (at least one: the empty line).
#[must_use]
pub fn height_for(rows: usize) -> i32 {
    let rows = i32::try_from(rows.clamp(1, MAX_ROWS)).unwrap_or(1);
    rows * ROW_HEIGHT + 2
}

/// What the player did with the overlay, for the shell to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayAction {
    /// It was dragged and let go here (pixels from the screen's top-left).
    Moved(i32, i32),
    /// Its × was clicked: the overlay is off.
    Closed,
}

/// One row as painted.
#[derive(Debug, Clone, PartialEq)]
struct RowFrame {
    object_id: u32,
    name: String,
    state_icon: u32,
    mana: String,
    time: String,
}

/// What the overlay shows, compared to skip painting the same again.
#[derive(Debug, Clone, PartialEq, Default)]
struct Frame {
    rows: Vec<RowFrame>,
    hovered: bool,
}

/// What a left press took hold of.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Press {
    /// The close box, which closes when let go over it.
    Close,
    /// The overlay, to drag: the point grabbed, and where it stood.
    Drag { grab: (f64, f64), from: (i32, i32) },
}

/// The overlay.
#[derive(Debug, Default)]
pub struct ManaOverlay {
    canvas: Option<u32>,
    /// The rows the canvas was sized for.
    canvas_rows: usize,
    canvas_visible: bool,
    /// Where it stands, and where the canvas was last put.
    place: (i32, i32),
    placed: (i32, i32),
    press: Option<Press>,
    hovered: bool,
    /// What is to be shown, and what was last painted.
    frame: Frame,
    painted: Option<Frame>,
    /// The icons asked for, by object id and by state icon.
    object_icons: HashMap<u32, MapImage>,
    state_icons: HashMap<u32, MapImage>,
}

fn inside(rect: (f64, f64, f64, f64), at: (f64, f64)) -> bool {
    at.0 >= rect.0 && at.0 < rect.0 + rect.2 && at.1 >= rect.1 && at.1 < rect.1 + rect.3
}

#[allow(clippy::cast_possible_truncation)]
fn whole_pixels(value: f64) -> i32 {
    value
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

impl ManaOverlay {
    /// The canvas (the registry's number), once registered.
    #[must_use]
    pub fn canvas(&self) -> Option<u32> {
        self.canvas
    }

    /// One frame: shown while `wanted`, at `place` (the kept one) unless a
    /// drag holds it, as tall as `rows`, and painted again when they or
    /// the × changed.
    pub fn on_frame(
        &mut self,
        ui: &mut dyn MapUi,
        wanted: bool,
        place: (i32, i32),
        rows: &[ManaRow],
    ) {
        if !wanted {
            self.press = None;
            self.hovered = false;
            if let Some(canvas) = self.canvas
                && self.canvas_visible
            {
                self.canvas_visible = false;
                ui.set_canvas_visible(canvas, false);
            }
            return;
        }
        if self.press.is_none() {
            self.place = (place.0.clamp(0, MAX_PLACE), place.1.clamp(0, MAX_PLACE));
        }
        let count = rows.len().clamp(1, MAX_ROWS);
        if self.canvas.is_none() || (self.press.is_none() && count != self.canvas_rows) {
            self.register(ui, count);
        }
        self.place_canvas(ui);
        let Some(canvas) = self.canvas else {
            return;
        };
        if !self.canvas_visible {
            self.canvas_visible = true;
            ui.set_canvas_visible(canvas, true);
        }
        self.frame = Frame {
            rows: rows
                .iter()
                .take(MAX_ROWS)
                .map(|row| RowFrame {
                    object_id: row.object_id,
                    name: row.name.clone(),
                    state_icon: row.state.icon(),
                    mana: row.mana_text.clone(),
                    time: row.time_text.clone(),
                })
                .collect(),
            hovered: self.hovered,
        };
        if self.painted.as_ref() != Some(&self.frame) {
            ui.invalidate_canvas(canvas);
        }
    }

    /// The pointer: over it with no button held the × shows, leaving hides
    /// it; a left press let go on the × closes it; a left press anywhere
    /// else drags it (the grabbed point stays under the pointer), its place
    /// kept once, when let go. The host reports each move from where the
    /// canvas was last put.
    pub fn on_pointer(
        &mut self,
        ui: &mut dyn MapUi,
        event: &PointerEvent,
    ) -> Option<OverlayAction> {
        let at = (f64::from(event.x), f64::from(event.y));
        let action = match event.kind {
            PointerEvent::HOVER => {
                // The host hovers only with no button held: a press still
                // open lost its release.
                let ended = self.end_press();
                self.hovered = true;
                ended
            }
            PointerEvent::LEAVE => {
                self.hovered = false;
                None
            }
            PointerEvent::UP | PointerEvent::CANCELLED if event.button != BUTTON_LEFT => {
                self.end_press()
            }
            _ if event.button != BUTTON_LEFT => None,
            PointerEvent::DOWN => {
                self.press = Some(if inside(CLOSE_BOX, at) {
                    Press::Close
                } else {
                    Press::Drag {
                        grab: at,
                        from: self.place,
                    }
                });
                None
            }
            PointerEvent::MOVE => {
                if let Some(Press::Drag { grab, .. }) = self.press {
                    self.follow(grab, at);
                }
                None
            }
            PointerEvent::UP => match self.press.take() {
                Some(Press::Close) if inside(CLOSE_BOX, at) => Some(OverlayAction::Closed),
                Some(Press::Drag { grab, from }) => {
                    self.follow(grab, at);
                    self.moved_from(from)
                }
                _ => None,
            },
            PointerEvent::CANCELLED => self.end_press(),
            _ => None,
        };
        self.place_canvas(ui);
        action
    }

    /// The session ended: hidden, its icons let go; the next session
    /// paints afresh.
    pub fn reset(&mut self, ui: &mut dyn MapUi) {
        self.release_icons(ui);
        self.press = None;
        self.hovered = false;
        self.painted = None;
        if let Some(canvas) = self.canvas
            && self.canvas_visible
        {
            self.canvas_visible = false;
            ui.set_canvas_visible(canvas, false);
        }
    }

    /// The plugin stops: icons and canvas given back.
    pub fn dispose(&mut self, ui: &mut dyn MapUi) {
        self.reset(ui);
        if let Some(canvas) = self.canvas.take() {
            ui.remove_canvas(canvas);
        }
    }

    /// Paints the rows: per row the item's icon, its name (cut at its
    /// column), the state icon, the mana and the time, in outlined text;
    /// the × while the pointer is over it.
    pub fn paint(&mut self, painter: &mut dyn MapPainter, ui: &mut dyn MapUi) {
        let frame = self.frame.clone();
        self.release_stale_icons(ui, &frame);
        painter.clear(&Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        });
        if frame.rows.is_empty() {
            painter.draw_text(EMPTY_TEXT, (2.0, 2.0), &TEXT, true);
        }
        for (index, row) in frame.rows.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let top = 1.0 + index as f64 * f64::from(ROW_HEIGHT);
            let icon = self.object_icon(ui, row.object_id);
            if icon.is_valid() {
                painter.draw_image(icon, (0.0, top + 1.0, 16.0, 16.0), &NO_TINT);
            }
            let row_height = f64::from(ROW_HEIGHT);
            painter.push_clip((NAME_X, top, NAME_WIDTH - 2.0, row_height));
            painter.draw_text(&row.name, (NAME_X, top + 2.0), &TEXT, true);
            painter.pop_clip();
            let state = self.state_icon(ui, row.state_icon);
            if state.is_valid() {
                painter.draw_image(state, (STATE_X, top + 1.0, 16.0, 16.0), &NO_TINT);
            }
            painter.push_clip((MANA_X, top, MANA_WIDTH - 2.0, row_height));
            painter.draw_text(&row.mana, (MANA_X, top + 2.0), &NUMBERS, true);
            painter.pop_clip();
            painter.push_clip((TIME_X, top, TIME_WIDTH - 2.0, row_height));
            painter.draw_text(&row.time, (TIME_X, top + 2.0), &NUMBERS, true);
            painter.pop_clip();
        }
        if frame.hovered {
            overlay::cross(painter, CLOSE_BOX);
        }
        self.painted = Some(frame);
    }

    fn register(&mut self, ui: &mut dyn MapUi, rows: usize) {
        if let Some(canvas) = self.canvas.take() {
            ui.remove_canvas(canvas);
        }
        // The old canvas's leave never comes once it is gone.
        self.hovered = false;
        self.canvas_rows = rows;
        self.canvas = ui.register_canvas(&MapCanvasSpec {
            canvas_id: CANVAS_ID.to_owned(),
            width: WIDTH,
            height: height_for(rows),
            offset: (f64::from(self.place.0), f64::from(self.place.1)),
            start_visible: false,
            accepts_pointer_input: true,
        });
        self.placed = self.place;
        self.canvas_visible = false;
        self.painted = None;
    }

    fn place_canvas(&mut self, ui: &mut dyn MapUi) {
        if let Some(canvas) = self.canvas
            && self.placed != self.place
        {
            self.placed = self.place;
            ui.set_canvas_offset(canvas, (f64::from(self.place.0), f64::from(self.place.1)));
        }
    }

    fn follow(&mut self, grab: (f64, f64), at: (f64, f64)) {
        let dx = whole_pixels(at.0 - grab.0);
        let dy = whole_pixels(at.1 - grab.1);
        self.place = (
            self.place.0.saturating_add(dx).clamp(0, MAX_PLACE),
            self.place.1.saturating_add(dy).clamp(0, MAX_PLACE),
        );
    }

    fn moved_from(&self, from: (i32, i32)) -> Option<OverlayAction> {
        (self.place != from).then_some(OverlayAction::Moved(self.place.0, self.place.1))
    }

    /// Ends a press without acting: a drag keeps where it got to.
    fn end_press(&mut self) -> Option<OverlayAction> {
        match self.press.take() {
            Some(Press::Drag { from, .. }) => self.moved_from(from),
            _ => None,
        }
    }

    fn object_icon(&mut self, ui: &mut dyn MapUi, object_id: u32) -> MapImage {
        if let Some(icon) = self.object_icons.get(&object_id) {
            return *icon;
        }
        let icon = ui.object_icon(object_id);
        if icon.is_valid() {
            self.object_icons.insert(object_id, icon);
        }
        icon
    }

    fn state_icon(&mut self, ui: &mut dyn MapUi, surface: u32) -> MapImage {
        if let Some(icon) = self.state_icons.get(&surface) {
            return *icon;
        }
        let icon = ui.client_art(surface);
        if icon.is_valid() {
            self.state_icons.insert(surface, icon);
        }
        icon
    }

    /// Lets go of the icons of items no longer shown.
    fn release_stale_icons(&mut self, ui: &mut dyn MapUi, frame: &Frame) {
        let stale: Vec<u32> = self
            .object_icons
            .keys()
            .copied()
            .filter(|id| !frame.rows.iter().any(|row| row.object_id == *id))
            .collect();
        for id in stale {
            if let Some(icon) = self.object_icons.remove(&id) {
                ui.release_image(icon);
            }
        }
    }

    fn release_icons(&mut self, ui: &mut dyn MapUi) {
        for (_, icon) in self.object_icons.drain() {
            ui.release_image(icon);
        }
        for (_, icon) in self.state_icons.drain() {
            ui.release_image(icon);
        }
    }
}

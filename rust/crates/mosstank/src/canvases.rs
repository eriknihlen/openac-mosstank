//! The UB tools' canvases (the countryside map, the dungeon map, the other
//! clients' readout) as the SDK holds them.
//!
//! The tools ask for a canvas, show it and ask for a repaint in the middle
//! of their frame, while the frame holds the client for the tool's own
//! reads. Those requests are kept here ([`Recorder`]) and carried out once
//! the tool's frame is over ([`Canvases::flush`]), and the repaint runs
//! then, with the client to itself ([`Canvases::paint_due`]). The canvas a
//! tool holds is this registry's number for it, so it is the same number
//! before and after the client takes it.

use std::cell::RefCell;

use ac_plugin_abi::ui::Color;
use mosstank_ub::canvas::{MapCanvasSpec, MapImage, MapPainter, MapUi};
use openac_plugin::ui::Canvas;

use crate::client::TankClient;

/// One drawing command, kept until the paint is handed over.
#[derive(Debug, Clone, PartialEq)]
pub enum MapPaint {
    /// Fills the canvas.
    Clear(Color),
    /// Fills a rectangle.
    FillRect((f32, f32, f32, f32), Color),
    /// Outlines a rectangle.
    StrokeRect((f32, f32, f32, f32), Color, f32),
    /// A straight line.
    Line((f32, f32), (f32, f32), Color, f32),
    /// One line of text.
    Text(String, (f32, f32), Color, bool),
    /// An image over a rectangle.
    Image(MapImage, (f32, f32, f32, f32), Color),
    /// Clips what follows.
    PushClip((f32, f32, f32, f32)),
    /// Undoes the last clip.
    PopClip,
}

#[derive(Debug)]
struct Entry {
    handle: u32,
    spec: MapCanvasSpec,
    canvas: Option<Canvas>,
    refused: bool,
    visible: bool,
    shown: Option<bool>,
    offset: (f64, f64),
    placed: (f64, f64),
    paint_due: bool,
    removed: bool,
}

/// The canvases the tools hold.
#[derive(Debug, Default)]
pub struct Canvases {
    entries: Vec<Entry>,
    next: u32,
    released: Vec<MapImage>,
}

impl Canvases {
    /// The view the tools' frames reach.
    pub fn recorder(&mut self) -> Recorder<'_> {
        Recorder(self)
    }

    fn entry(&mut self, handle: u32) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|entry| entry.handle == handle)
    }

    /// The registry's number for the client's canvas `canvas_id` (the
    /// pointer events' canvas).
    #[must_use]
    pub fn handle_of(&self, canvas_id: u32) -> Option<u32> {
        self.entries
            .iter()
            .find(|entry| entry.canvas.is_some_and(|canvas| canvas.id() == canvas_id))
            .map(|entry| entry.handle)
    }

    /// Carries out what the tools asked for: canvases registered, shown or
    /// hidden, moved, removed, images let go.
    pub fn flush(&mut self, client: &mut dyn TankClient) {
        for image in self.released.drain(..) {
            client.image_release(image);
        }
        let mut kept = Vec::with_capacity(self.entries.len());
        for mut entry in self.entries.drain(..) {
            if entry.removed {
                if let Some(canvas) = entry.canvas {
                    client.canvas_remove(canvas);
                }
                continue;
            }
            if entry.canvas.is_none() && !entry.refused {
                match client.canvas_register(&entry.spec) {
                    Ok(canvas) => entry.canvas = Some(canvas),
                    Err(error) => {
                        // Said once; the tool keeps its number and draws
                        // nothing, as it would on a client with no windows.
                        entry.refused = true;
                        client.rules().log_warn(&format!(
                            "MossTank canvas '{}' was not registered: {error}",
                            entry.spec.canvas_id
                        ));
                    }
                }
            }
            if let Some(canvas) = entry.canvas
                && entry.shown != Some(entry.visible)
            {
                client.canvas_set_visible(canvas, entry.visible);
                entry.shown = Some(entry.visible);
            }
            if let Some(canvas) = entry.canvas
                && entry.placed != entry.offset
            {
                #[allow(clippy::cast_possible_truncation)]
                client.canvas_set_offset(canvas, entry.offset.0 as f32, entry.offset.1 as f32);
                entry.placed = entry.offset;
            }
            kept.push(entry);
        }
        self.entries = kept;
    }

    /// Takes the repaint asked for on `handle`, if one was: the client's
    /// canvas to paint, or `None`.
    pub fn take_paint_due(&mut self, handle: Option<u32>) -> Option<Canvas> {
        let entry = self.entry(handle?)?;
        if !entry.paint_due {
            return None;
        }
        entry.paint_due = false;
        entry.canvas
    }
}

/// [`MapUi`] for a tool's frame: the requests are kept for [`Canvases::flush`].
#[derive(Debug)]
pub struct Recorder<'a>(&'a mut Canvases);

impl MapUi for Recorder<'_> {
    fn register_canvas(&mut self, spec: &MapCanvasSpec) -> Option<u32> {
        self.0.next += 1;
        let handle = self.0.next;
        self.0.entries.push(Entry {
            handle,
            spec: spec.clone(),
            canvas: None,
            refused: false,
            visible: spec.start_visible,
            shown: Some(spec.start_visible),
            offset: spec.offset,
            placed: spec.offset,
            paint_due: false,
            removed: false,
        });
        Some(handle)
    }

    fn set_canvas_visible(&mut self, canvas: u32, visible: bool) {
        if let Some(entry) = self.0.entry(canvas) {
            entry.visible = visible;
        }
    }

    fn set_canvas_offset(&mut self, canvas: u32, offset: (f64, f64)) {
        if let Some(entry) = self.0.entry(canvas) {
            entry.offset = offset;
        }
    }

    fn invalidate_canvas(&mut self, canvas: u32) {
        if let Some(entry) = self.0.entry(canvas) {
            entry.paint_due = true;
        }
    }

    fn remove_canvas(&mut self, canvas: u32) {
        if let Some(entry) = self.0.entry(canvas) {
            entry.removed = true;
        }
    }

    // Images are asked for only while painting ([`PaintUi`]); a frame has
    // none to give.
    fn object_icon(&mut self, _object_id: u32) -> MapImage {
        MapImage::default()
    }

    fn client_art(&mut self, _surface_id: u32) -> MapImage {
        MapImage::default()
    }

    fn release_image(&mut self, image: MapImage) {
        self.0.released.push(image);
    }
}

/// [`MapUi`] during a paint: images come from the client at once.
pub struct PaintUi<'a, 'c, 'd> {
    client: &'a RefCell<&'c mut (dyn TankClient + 'd)>,
    canvases: &'a mut Canvases,
}

impl std::fmt::Debug for PaintUi<'_, '_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PaintUi")
    }
}

impl<'a, 'c, 'd> PaintUi<'a, 'c, 'd> {
    /// The paint's view of the client and the registry.
    pub fn new(
        client: &'a RefCell<&'c mut (dyn TankClient + 'd)>,
        canvases: &'a mut Canvases,
    ) -> Self {
        Self { client, canvases }
    }
}

impl MapUi for PaintUi<'_, '_, '_> {
    fn register_canvas(&mut self, spec: &MapCanvasSpec) -> Option<u32> {
        self.canvases.recorder().register_canvas(spec)
    }

    fn set_canvas_visible(&mut self, canvas: u32, visible: bool) {
        self.canvases.recorder().set_canvas_visible(canvas, visible);
    }

    fn set_canvas_offset(&mut self, canvas: u32, offset: (f64, f64)) {
        self.canvases.recorder().set_canvas_offset(canvas, offset);
    }

    fn invalidate_canvas(&mut self, canvas: u32) {
        self.canvases.recorder().invalidate_canvas(canvas);
    }

    fn remove_canvas(&mut self, canvas: u32) {
        self.canvases.recorder().remove_canvas(canvas);
    }

    fn object_icon(&mut self, object_id: u32) -> MapImage {
        self.client.borrow_mut().image_object_icon(object_id)
    }

    fn client_art(&mut self, surface_id: u32) -> MapImage {
        self.client.borrow_mut().image_client_art(surface_id)
    }

    fn release_image(&mut self, image: MapImage) {
        self.client.borrow_mut().image_release(image);
    }
}

/// [`MapPainter`] that keeps the commands for the client; text is measured
/// by the client as it is drawn.
pub struct RecordingPainter<'a, 'c, 'd> {
    client: &'a RefCell<&'c mut (dyn TankClient + 'd)>,
    width: i32,
    height: i32,
    /// The commands so far.
    pub commands: Vec<MapPaint>,
}

impl std::fmt::Debug for RecordingPainter<'_, '_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecordingPainter")
            .field("commands", &self.commands)
            .finish_non_exhaustive()
    }
}

impl<'a, 'c, 'd> RecordingPainter<'a, 'c, 'd> {
    /// A paint of `canvas`.
    pub fn new(client: &'a RefCell<&'c mut (dyn TankClient + 'd)>, canvas: Canvas) -> Self {
        Self {
            client,
            width: canvas.width(),
            height: canvas.height(),
            commands: Vec::new(),
        }
    }
}

#[allow(clippy::cast_possible_truncation)]
fn rect(r: (f64, f64, f64, f64)) -> (f32, f32, f32, f32) {
    (r.0 as f32, r.1 as f32, r.2 as f32, r.3 as f32)
}

#[allow(clippy::cast_possible_truncation)]
fn point(p: (f64, f64)) -> (f32, f32) {
    (p.0 as f32, p.1 as f32)
}

impl MapPainter for RecordingPainter<'_, '_, '_> {
    fn width(&self) -> i32 {
        self.width
    }

    fn height(&self) -> i32 {
        self.height
    }

    fn clear(&mut self, color: &Color) {
        self.commands.push(MapPaint::Clear(color.clone()));
    }

    fn fill_rect(&mut self, r: (f64, f64, f64, f64), color: &Color) {
        self.commands
            .push(MapPaint::FillRect(rect(r), color.clone()));
    }

    fn stroke_rect(&mut self, r: (f64, f64, f64, f64), color: &Color, thickness: f32) {
        self.commands
            .push(MapPaint::StrokeRect(rect(r), color.clone(), thickness));
    }

    fn draw_line(&mut self, from: (f64, f64), to: (f64, f64), color: &Color, thickness: f32) {
        self.commands.push(MapPaint::Line(
            point(from),
            point(to),
            color.clone(),
            thickness,
        ));
    }

    fn draw_text(&mut self, text: &str, at: (f64, f64), color: &Color, outline: bool) {
        self.commands.push(MapPaint::Text(
            text.to_owned(),
            point(at),
            color.clone(),
            outline,
        ));
    }

    fn measure_text(&mut self, text: &str) -> (f64, f64) {
        let (w, h) = self.client.borrow_mut().measure_text(text);
        (f64::from(w), f64::from(h))
    }

    fn draw_image(&mut self, image: MapImage, r: (f64, f64, f64, f64), tint: &Color) {
        self.commands
            .push(MapPaint::Image(image, rect(r), tint.clone()));
    }

    fn push_clip(&mut self, r: (f64, f64, f64, f64)) {
        self.commands.push(MapPaint::PushClip(rect(r)));
    }

    fn pop_clip(&mut self) {
        self.commands.push(MapPaint::PopClip);
    }
}

/// Replays kept commands into the SDK's painter.
pub fn replay(commands: &[MapPaint], painter: &mut openac_plugin::ui::Painter) {
    for command in commands {
        match command {
            MapPaint::Clear(color) => painter.clear(color.clone()),
            MapPaint::FillRect((x, y, w, h), color) => {
                painter.fill_rect(*x, *y, *w, *h, color.clone())
            }
            MapPaint::StrokeRect((x, y, w, h), color, thickness) => {
                painter.stroke_rect(*x, *y, *w, *h, color.clone(), *thickness);
            }
            MapPaint::Line(from, to, color, thickness) => {
                painter.line(*from, *to, color.clone(), *thickness);
            }
            MapPaint::Text(text, at, color, outline) => {
                painter.text(text, *at, color.clone(), *outline);
            }
            MapPaint::Image(image, (x, y, w, h), tint) => painter.image(
                openac_plugin::ui::Image {
                    handle: image.handle,
                    width: image.width,
                    height: image.height,
                },
                *x,
                *y,
                *w,
                *h,
                tint.clone(),
            ),
            MapPaint::PushClip((x, y, w, h)) => painter.push_clip(*x, *y, *w, *h),
            MapPaint::PopClip => painter.pop_clip(),
        }
    }
}

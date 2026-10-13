//! What a UB tool that draws on a canvas reaches the client's interface
//! through (`IUiRegistry`, `IPluginCanvas`, `IPluginPainter`): the canvas
//! and the images ([`MapUi`]) and the paint ([`MapPainter`]). The plugin
//! answers them with the SDK's canvas and painter; the dungeon map and the
//! clients readout draw through them.

use ac_plugin_abi::ui::Color;

/// A pointer press (`PointerEvent::kind`).
pub const POINTER_DOWN: u8 = 0;
/// A release.
pub const POINTER_UP: u8 = 1;
/// A move.
pub const POINTER_MOVE: u8 = 2;
/// A wheel notch.
pub const POINTER_WHEEL: u8 = 3;
/// A press the host cancelled.
pub const POINTER_CANCELLED: u8 = 4;
/// The left button (`PointerEvent::button`).
pub const BUTTON_LEFT: u8 = 1;
/// Shift held (`PointerEvent::modifiers`).
pub const MODIFIER_SHIFT: u8 = 1;

/// An image the client holds for the plugin (handle 0: none).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MapImage {
    /// The client's number for it.
    pub handle: u32,
    /// Width, pixels.
    pub width: u32,
    /// Height, pixels.
    pub height: u32,
}

impl MapImage {
    /// The client issued it.
    #[must_use]
    pub fn is_valid(self) -> bool {
        self.handle != 0
    }
}

/// What the map's canvas is registered with (`PluginCanvasDescriptor`).
#[derive(Debug, Clone, PartialEq)]
pub struct MapCanvasSpec {
    /// Its id.
    pub canvas_id: String,
    /// Width, pixels.
    pub width: i32,
    /// Height, pixels.
    pub height: i32,
    /// Pixels from the screen's top-left.
    pub offset: (f64, f64),
    /// Shown at once.
    pub start_visible: bool,
    /// Takes pointer input.
    pub accepts_pointer_input: bool,
}

/// The canvas and the images, as the client keeps them.
pub trait MapUi {
    /// Registers a canvas; its handle, `None` when refused.
    fn register_canvas(&mut self, spec: &MapCanvasSpec) -> Option<u32>;
    /// Shows or hides it.
    fn set_canvas_visible(&mut self, canvas: u32, visible: bool);
    /// Asks for a repaint: [`DungeonMapController::paint`] on the next
    /// frame.
    fn invalidate_canvas(&mut self, canvas: u32);
    /// Removes it.
    fn remove_canvas(&mut self, canvas: u32);
    /// An object's icon, or no image.
    fn object_icon(&mut self, object_id: u32) -> MapImage;
    /// A piece of the client's own art (a surface id), or no image
    /// (`IPluginImages.FromClientArt`).
    fn client_art(&mut self, surface_id: u32) -> MapImage;
    /// Lets go of an image.
    fn release_image(&mut self, image: MapImage);
}

/// What the paint draws with (`IPluginPainter`): pixels from the canvas's
/// top-left, y down.
pub trait MapPainter {
    /// The canvas's width.
    fn width(&self) -> i32;
    /// The canvas's height.
    fn height(&self) -> i32;
    /// Fills the canvas.
    fn clear(&mut self, color: &Color);
    /// Fills a rectangle (x, y, width, height).
    fn fill_rect(&mut self, rect: (f64, f64, f64, f64), color: &Color);
    /// Outlines a rectangle, inside its edges.
    fn stroke_rect(&mut self, rect: (f64, f64, f64, f64), color: &Color, thickness: f32);
    /// A straight line.
    fn draw_line(&mut self, from: (f64, f64), to: (f64, f64), color: &Color, thickness: f32);
    /// One line of text with its top-left at `at`.
    fn draw_text(&mut self, text: &str, at: (f64, f64), color: &Color, outline: bool);
    /// How big a line of text is: width and height.
    fn measure_text(&mut self, text: &str) -> (f64, f64);
    /// An image stretched over a rectangle (x, y, width, height), tinted.
    fn draw_image(&mut self, image: MapImage, rect: (f64, f64, f64, f64), tint: &Color);
    /// Clips what follows to a rectangle.
    fn push_clip(&mut self, rect: (f64, f64, f64, f64));
    /// Undoes the last clip.
    fn pop_clip(&mut self);
}

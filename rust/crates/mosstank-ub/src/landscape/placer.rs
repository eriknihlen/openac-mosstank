//! Where each marker's name goes (`MapLabelPlacer`): a spot that covers
//! neither another name nor another marker's icon, tried at eight places
//! around the marker in a fixed order (above, top right, right, bottom
//! right, below, bottom left, left, top left), so a crowded town reads the
//! same from one frame to the next. A name with no free place is left out.

/// A rectangle: x, y, width, height (`PluginRect`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    /// Left.
    pub x: f64,
    /// Top.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

impl Rect {
    /// A rectangle.
    #[must_use]
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// As the painter takes it.
    #[must_use]
    pub fn tuple(self) -> (f64, f64, f64, f64) {
        (self.x, self.y, self.width, self.height)
    }
}

/// The names placed this frame.
#[derive(Debug, Clone, Default)]
pub struct MapLabelPlacer {
    occupied: Vec<Rect>,
}

impl MapLabelPlacer {
    /// Nothing claimed.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every rectangle claimed this frame.
    #[must_use]
    pub fn occupied(&self) -> &[Rect] {
        &self.occupied
    }

    /// Starts a new frame with nothing claimed.
    pub fn reset(&mut self) {
        self.occupied.clear();
    }

    /// Claims a rectangle names must keep clear of, such as an icon.
    pub fn occupy(&mut self, rect: Rect) {
        self.occupied.push(rect);
    }

    /// The eight places a name of this size may go around a marker at a
    /// canvas point, in the order they are tried.
    #[must_use]
    pub fn candidates(x: f64, y: f64, width: f64, height: f64) -> [Rect; 8] {
        let w = width;
        let h = height;
        let left = x - w / 2.0;
        let top = y - h / 2.0 - 16.0;
        [
            Rect::new(left, top, w, h),                         // above
            Rect::new(left + 4.0 + w / 2.0, top + 2.0, w, h),   // top right
            Rect::new(left + 8.0 + w / 2.0, top + 15.0, w, h),  // right
            Rect::new(left + 8.0 + w / 2.0, top + 26.0, w, h),  // bottom right
            Rect::new(left, top + 28.0, w, h),                  // below
            Rect::new(left - 10.0 - w / 2.0, top + 24.0, w, h), // bottom left
            Rect::new(left - 10.0 - w / 2.0, top + 15.0, w, h), // left
            Rect::new(left - 6.0 - w / 2.0, top + 4.0, w, h),   // top left
        ]
    }

    /// Claims the first free candidate around a marker for a name of this
    /// size; `None` when all eight are taken.
    pub fn try_place(&mut self, x: f64, y: f64, width: f64, height: f64) -> Option<Rect> {
        for candidate in Self::candidates(x, y, width, height) {
            if self
                .occupied
                .iter()
                .any(|taken| Self::intersects(candidate, *taken))
            {
                continue;
            }
            self.occupied.push(candidate);
            return Some(candidate);
        }
        None
    }

    /// Whether two rectangles share any area; touching edges do not.
    #[must_use]
    pub fn intersects(a: Rect, b: Rect) -> bool {
        a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
    }
}

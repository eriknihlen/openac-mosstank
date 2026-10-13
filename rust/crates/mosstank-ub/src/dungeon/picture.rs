//! A floorplan turned into a picture in map pixels (`DungeonMapPicture`):
//! five pixels to the metre, north up, east to the right, the pixel origin
//! at the north-west corner of the plan's bounds. The floor of each storey
//! is rasterised once, at build, into rectangles that each know their cell,
//! so the painter fills rectangles rather than polygons and a visited cell
//! can be tinted without touching the picture again. Colours are not part
//! of the picture: they come from the settings at paint time.
//!
//! The reference picture mirrors x so that east reads to the left, then
//! turns the whole map half a circle when it draws it. Mirroring one axis
//! and turning by a half circle together are exactly a flip of the other
//! axis: north up. So this picture flips y and nothing else, and the camera
//! turns it by the heading alone.

use std::collections::HashMap;

use ac_plugin_abi::world::{DungeonCell, DungeonLayer, Floorplan};

use super::{Vec2, Vec3};

/// Map pixels per metre.
pub const PIXELS_PER_METER: f32 = 5.0;

/// A rectangle in map pixels, y down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DungeonMapRect {
    /// Left.
    pub x: i32,
    /// Top.
    pub y: i32,
    /// Width.
    pub width: i32,
    /// Height.
    pub height: i32,
}

/// A straight line in map pixels, y down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DungeonMapLine {
    /// One end, x.
    pub x1: f32,
    /// One end, y.
    pub y1: f32,
    /// The other end, x.
    pub x2: f32,
    /// The other end, y.
    pub y2: f32,
}

/// One run of floor: a rectangle of map pixels and the cell it belongs to,
/// as an index into [`DungeonMapPicture::cells`], or -1 when the plan
/// carried floor with no cell near it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DungeonMapFloorRun {
    /// The rectangle.
    pub rect: DungeonMapRect,
    /// The cell's index, -1 for none.
    pub cell_index: i32,
}

/// One storey of the picture: its floor runs and its walls.
#[derive(Debug, Clone, PartialEq)]
pub struct DungeonMapLayerPicture {
    /// The lowest height of the storey's six-metre band, in metres.
    pub z: f32,
    /// The floor, as rectangles that together cover it exactly once.
    pub floors: Vec<DungeonMapFloorRun>,
    /// The walls, one line each.
    pub walls: Vec<DungeonMapLine>,
}

/// A floorplan rasterised.
#[derive(Debug, Clone, Default)]
pub struct DungeonMapPicture {
    /// The landblock the picture is of, with a zero low half.
    pub landblock_id: u32,
    /// The landblock-local point, in metres, at pixel (0, 0): the west edge
    /// and the north edge of the plan's bounds.
    pub origin_meters: Vec2,
    /// The picture's width in map pixels.
    pub width: i32,
    /// The picture's height in map pixels.
    pub height: i32,
    /// The lowest storey's band height, in metres.
    pub min_z: f32,
    /// The highest storey's band height, in metres.
    pub max_z: f32,
    /// The storeys, lowest first.
    pub layers: Vec<DungeonMapLayerPicture>,
    /// The plan's cells, in the order the floor runs index them.
    pub cells: Vec<DungeonCell>,
    cell_index: HashMap<u32, usize>,
}

const NO_FLOOR: i32 = -1;
const FLOOR_WITHOUT_CELL: i32 = -2;

impl DungeonMapPicture {
    /// The picture of nothing.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// True when there is nothing to draw.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Where a landblock-local point falls in the picture, in map pixels.
    #[must_use]
    pub fn map_pixel_from_local(&self, local: Vec3) -> Vec2 {
        self.map_pixel_from_local_xy(local.xy())
    }

    /// Where a landblock-local point (x and y) falls in the picture.
    #[must_use]
    pub fn map_pixel_from_local_xy(&self, local: Vec2) -> Vec2 {
        Vec2::new(
            (local.x - self.origin_meters.x) * PIXELS_PER_METER,
            (self.origin_meters.y - local.y) * PIXELS_PER_METER,
        )
    }

    /// The index of a cell in [`Self::cells`], by its full id.
    #[must_use]
    pub fn find_cell_index(&self, cell_id: u32) -> Option<usize> {
        self.cell_index.get(&cell_id).copied()
    }

    /// Rasterises a plan. The plan of nothing is the picture of nothing.
    #[must_use]
    pub fn build(plan: &Floorplan) -> Self {
        if plan.layers.is_empty() {
            return Self::empty();
        }
        let origin = Vec2::new(plan.bounds_min.0, plan.bounds_max.1);
        let width = pixels(plan.bounds_max.0 - plan.bounds_min.0);
        let height = pixels(plan.bounds_max.1 - plan.bounds_min.1);

        let mut layers = Vec::with_capacity(plan.layers.len());
        let mut min_z = f32::MAX;
        let mut max_z = f32::MIN;
        let size = usize::try_from(width).unwrap_or(0) * usize::try_from(height).unwrap_or(0);
        let mut owners = vec![NO_FLOOR; size];
        for layer in &plan.layers {
            min_z = min_z.min(layer.z);
            max_z = max_z.max(layer.z);
            layers.push(build_layer(
                layer,
                &plan.cells,
                origin,
                width,
                height,
                &mut owners,
            ));
        }
        // A stable sort, as `List.Sort` is not; the plan's storeys come
        // lowest first, so the two agree on every plan the host builds.
        layers.sort_by(|a, b| a.z.total_cmp(&b.z));

        let mut cell_index = HashMap::with_capacity(plan.cells.len());
        for (index, cell) in plan.cells.iter().enumerate() {
            cell_index.entry(cell.cell_id).or_insert(index);
        }
        Self {
            landblock_id: plan.landblock_id,
            origin_meters: origin,
            width,
            height,
            min_z,
            max_z,
            layers,
            cells: plan.cells.clone(),
            cell_index,
        }
    }
}

/// `Math.Max(1, (int)MathF.Ceiling(metres * PixelsPerMeter))`.
fn pixels(metres: f32) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let pixels = (metres * PIXELS_PER_METER).ceil() as i32;
    pixels.max(1)
}

fn pixel_of(local: (f32, f32), origin: Vec2) -> Vec2 {
    Vec2::new(
        (local.0 - origin.x) * PIXELS_PER_METER,
        (origin.y - local.1) * PIXELS_PER_METER,
    )
}

fn build_layer(
    layer: &DungeonLayer,
    cells: &[DungeonCell],
    origin: Vec2,
    width: i32,
    height: i32,
    owners: &mut [i32],
) -> DungeonMapLayerPicture {
    owners.fill(NO_FLOOR);
    for polygon in &layer.floors {
        let owner = nearest_cell(polygon, layer.z, cells);
        fill_polygon(polygon, owner, origin, width, height, owners);
    }
    let walls = layer
        .walls
        .iter()
        .map(|wall| {
            let a = pixel_of(wall.start, origin);
            let b = pixel_of(wall.end, origin);
            DungeonMapLine {
                x1: a.x,
                y1: a.y,
                x2: b.x,
                y2: b.y,
            }
        })
        .collect();
    DungeonMapLayerPicture {
        z: layer.z,
        floors: extract_runs(owners, width, height),
        walls,
    }
}

/// The cell a floor polygon belongs to: the one on the same storey whose
/// centre is nearest the polygon's centroid. The plan does not say which
/// polygon came from which cell, and the nearest centre on the same storey
/// is right for every cell whose floor is one piece.
fn nearest_cell(polygon: &[(f32, f32)], layer_z: f32, cells: &[DungeonCell]) -> i32 {
    if polygon.is_empty() || cells.is_empty() {
        return FLOOR_WITHOUT_CELL;
    }
    let mut centroid = Vec2::default();
    for vertex in polygon {
        centroid = centroid + Vec2::new(vertex.0, vertex.1);
    }
    #[allow(clippy::cast_precision_loss)]
    let count = polygon.len() as f32;
    centroid = Vec2::new(centroid.x / count, centroid.y / count);

    let mut best = FLOOR_WITHOUT_CELL;
    let mut best_distance = f32::MAX;
    for pass in 0..2 {
        if best != FLOOR_WITHOUT_CELL {
            break;
        }
        for (index, cell) in cells.iter().enumerate() {
            if pass == 0 && (cell.layer_z - layer_z).abs() > 0.01 {
                continue;
            }
            let distance = centroid.distance_squared(Vec2::new(cell.center.0, cell.center.1));
            if distance < best_distance {
                best_distance = distance;
                best = i32::try_from(index).unwrap_or(i32::MAX);
            }
        }
    }
    best
}

/// Scanline fill, even-odd, sampled at pixel centres, so two polygons that
/// share an edge never both own the pixels along it.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn fill_polygon(
    polygon: &[(f32, f32)],
    owner: i32,
    origin: Vec2,
    width: i32,
    height: i32,
    owners: &mut [i32],
) {
    let n = polygon.len();
    if n < 3 {
        return;
    }
    let mut xs = Vec::with_capacity(n);
    let mut ys = Vec::with_capacity(n);
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for vertex in polygon {
        let p = pixel_of(*vertex, origin);
        xs.push(p.x);
        ys.push(p.y);
        min_y = min_y.min(p.y);
        max_y = max_y.max(p.y);
    }

    let row_start = (min_y.floor() as i32).max(0);
    let row_end = (max_y.ceil() as i32).min(height - 1);
    let mut crossings: Vec<f32> = Vec::with_capacity(8);
    for row in row_start..=row_end {
        let cy = row as f32 + 0.5;
        crossings.clear();
        for i in 0..n {
            let j = (i + 1) % n;
            let (y0, y1) = (ys[i], ys[j]);
            if (y0 <= cy) == (y1 <= cy) {
                continue;
            }
            crossings.push(xs[i] + (cy - y0) / (y1 - y0) * (xs[j] - xs[i]));
        }
        if crossings.len() < 2 {
            continue;
        }
        crossings.sort_by(f32::total_cmp);
        let mut k = 0;
        while k + 1 < crossings.len() {
            let first = ((crossings[k] - 0.5).ceil() as i32).max(0);
            let last = ((crossings[k + 1] - 0.5).floor() as i32).min(width - 1);
            for col in first..=last {
                if let Ok(at) = usize::try_from(row * width + col) {
                    owners[at] = owner;
                }
            }
            k += 2;
        }
    }
}

/// Turns the owner grid into rectangles: horizontal runs of one owner,
/// merged downwards while the run beneath is the same width and owner. A
/// rectangular room becomes one rectangle.
fn extract_runs(owners: &[i32], width: i32, height: i32) -> Vec<DungeonMapFloorRun> {
    let mut runs: Vec<DungeonMapFloorRun> = Vec::new();
    // The runs that ended on the previous row, keyed by (x, width, owner),
    // as indices into runs; only those may grow by one more row.
    let mut open: HashMap<(i32, i32, i32), usize> = HashMap::new();
    let mut next: HashMap<(i32, i32, i32), usize> = HashMap::new();
    let at = |row: i32, col: i32| owners[usize::try_from(row * width + col).unwrap_or(0)];
    for row in 0..height {
        next.clear();
        let mut col = 0;
        while col < width {
            let owner = at(row, col);
            if owner == NO_FLOOR {
                col += 1;
                continue;
            }
            let start = col;
            while col < width && at(row, col) == owner {
                col += 1;
            }
            let key = (start, col - start, owner);
            if let Some(&index) = open.get(&key) {
                runs[index].rect.height += 1;
                next.insert(key, index);
            } else {
                runs.push(DungeonMapFloorRun {
                    rect: DungeonMapRect {
                        x: start,
                        y: row,
                        width: col - start,
                        height: 1,
                    },
                    cell_index: if owner >= 0 { owner } else { -1 },
                });
                next.insert(key, runs.len() - 1);
            }
        }
        std::mem::swap(&mut open, &mut next);
    }
    runs
}

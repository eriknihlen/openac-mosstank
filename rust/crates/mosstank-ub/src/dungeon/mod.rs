//! The dungeon map: the floor plan of the sealed dungeon the character
//! stands in, turned with their heading, with the things in it marked and
//! the cells they have walked shaded.
//!
//! - [`markers`]: the eleven kinds of mark and how a world object is sorted
//!   into one (`DungeonMarkers`).
//! - [`shade`]: which storeys are drawn from a height, and how dim
//!   (`DungeonLayerShade`).
//! - [`tracker`]: the objects on the map, placed on their storey
//!   (`DungeonObjectTracker`).
//! - [`visited`]: the cells walked, kept per landblock
//!   (`DungeonVisitedCells`).
//! - [`picture`]: a floorplan rasterised into map pixels
//!   (`DungeonMapPicture`).
//! - [`camera`]: where the map looks from (`DungeonMapCamera`).
//! - [`controller`]: the map on a canvas (`DungeonMapController`).
//!
//! Positions are landblock-local metres: x grows east and y north from the
//! landblock's south-west corner, z up.

pub mod camera;
pub mod controller;
pub mod markers;
pub mod picture;
pub mod shade;
pub mod tracker;
pub mod visited;

use ac_plugin_abi::world::Position;

/// A point or a step in two dimensions, single precision (`Vector2`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    /// East, or right.
    pub x: f32,
    /// North in metres; down in pixels.
    pub y: f32,
}

impl Vec2 {
    /// The point (x, y).
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// The squared distance to another point.
    #[must_use]
    pub fn distance_squared(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx * dx + dy * dy
    }
}

impl std::ops::Add for Vec2 {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }
}

impl std::ops::Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, factor: f32) -> Self {
        Self::new(self.x * factor, self.y * factor)
    }
}

/// A point in three dimensions, single precision (`Vector3`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    /// East.
    pub x: f32,
    /// North.
    pub y: f32,
    /// Up.
    pub z: f32,
}

impl Vec3 {
    /// The point (x, y, z).
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// The distance to another point.
    #[must_use]
    pub fn distance(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    /// The x and y.
    #[must_use]
    pub fn xy(self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }
}

/// A position in the plan's frame: metres east and north of the south-west
/// corner of the position's own landblock, and metres up
/// (`PluginDungeonFloorplan.ToLandblockLocal`).
#[must_use]
pub fn to_landblock_local(position: &Position) -> Vec3 {
    let [x, y, z] = position.landblock_local();
    Vec3::new(x, y, z)
}

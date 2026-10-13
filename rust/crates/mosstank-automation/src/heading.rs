//! Bearings between places (`NavigationController.DesiredHeading`,
//! `SignedHeadingDelta` and friends): which way a place lies from another,
//! how far round one heading is from another, and the ground between two
//! places. Shared by every walking rule and the corpse pick.

use ac_plugin_abi::world::Position;

/// The compass heading (degrees, 0 north, 90 east) from one place to
/// another.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn desired_heading(from: &Position, to: &Position) -> f32 {
    let east = to.east_west - from.east_west;
    let north = to.north_south - from.north_south;
    let mut heading = east.atan2(north) * 180.0 / std::f64::consts::PI;
    if heading < 0.0 {
        heading += 360.0;
    }
    heading as f32
}

/// How far, and which way, `desired` is round from `current`: -180 to 180
/// degrees.
#[must_use]
pub fn signed_heading_delta(current: f32, desired: f32) -> f32 {
    let mut delta = (desired - current) % 360.0;
    if delta > 180.0 {
        delta -= 360.0;
    } else if delta < -180.0 {
        delta += 360.0;
    }
    delta
}

/// The smaller of the two arcs between headings, never negative.
#[must_use]
pub fn unsigned_heading_delta(left: f32, right: f32) -> f32 {
    let high = if left >= right { left } else { right };
    let low = if left >= right { right } else { left };
    let inner = high - low;
    let outer = low - high + 360.0;
    if inner < outer { inner } else { outer }
}

/// A heading folded into 0..360.
#[must_use]
pub fn normalize_heading(value: f32) -> f32 {
    let wrapped = value % 360.0;
    if wrapped < 0.0 {
        wrapped + 360.0
    } else {
        wrapped
    }
}

/// Whether turning left reaches the heading by the shorter arc.
#[must_use]
pub fn prefers_left_turn(current: f32, desired: f32) -> bool {
    let offset = unsigned_heading_delta(current, desired);
    unsigned_heading_delta(normalize_heading(current - offset), desired) < 1.0
}

/// Ground distance between two places, metres.
#[must_use]
pub fn horizontal_distance_meters(a: &Position, b: &Position) -> f64 {
    let dx = a.east_west - b.east_west;
    let dy = a.north_south - b.north_south;
    (dx * dx + dy * dy).sqrt() * ac_plugin_abi::world::METRES_PER_MAP_UNIT
}

/// How far `point` is from the segment `start`–`end` across the ground,
/// metres (`NavigationController.DistanceToSegmentMeters`).
#[must_use]
pub fn distance_to_segment_meters(point: &Position, start: &Position, end: &Position) -> f64 {
    let dx = end.east_west - start.east_west;
    let dy = end.north_south - start.north_south;
    let length_squared = dx * dx + dy * dy;
    // `double.Epsilon`: the smallest positive double.
    if length_squared <= f64::from_bits(1) {
        return horizontal_distance_meters(point, start);
    }
    let projection = (((point.east_west - start.east_west) * dx
        + (point.north_south - start.north_south) * dy)
        / length_squared)
        .clamp(0.0, 1.0);
    let nearest_x = start.east_west + projection * dx;
    let nearest_y = start.north_south + projection * dy;
    let delta_x = point.east_west - nearest_x;
    let delta_y = point.north_south - nearest_y;
    (delta_x * delta_x + delta_y * delta_y).sqrt() * ac_plugin_abi::world::METRES_PER_MAP_UNIT
}

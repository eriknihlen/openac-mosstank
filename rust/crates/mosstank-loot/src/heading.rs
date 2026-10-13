//! The two bearings the corpse pick reads (`NavigationController
//! .DesiredHeading`, `SignedHeadingDelta`): which way a place lies from
//! another, and how far round a heading is from it.

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

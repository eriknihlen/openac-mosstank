//! The compass arithmetic the steering shares. A heading is degrees
//! clockwise from north (0 north, 90 east), as the game's compass reads;
//! map positions are in map units of 240 metres.

use ac_plugin_abi::navigation::{MovementIntent, NavigationStatus};
use ac_plugin_abi::world::{METRES_PER_MAP_UNIT, Position};

use crate::client::NavigationAutomation;
use crate::mover::{
    FACE_HEADING_REISSUE_SECONDS, HEADING_TOLERANCE_DEGREES, NO_FACE_HEADING_STAMP,
};

/// The compass heading from one position to another.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn desired_heading(from: &Position, to: &Position) -> f32 {
    let dx = to.east_west - from.east_west;
    let dy = to.north_south - from.north_south;
    let mut heading = dx.atan2(dy) * 180.0 / std::f64::consts::PI;
    if heading < 0.0 {
        heading += 360.0;
    }
    heading as f32
}

/// The shortest turn from `current` to `desired`, signed: positive is
/// clockwise (right), in (-180, 180]; half a turn stays 180.
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

/// Whether the turn from `current` to `desired` is to the left. Turning
/// down from the current heading by the unsigned offset and landing on the
/// bearing means the bearing is counter-clockwise. This agrees with the
/// sign of the wrapped difference everywhere except at exactly half a
/// turn, where the choice is arbitrary and this one is the reference's.
#[must_use]
pub fn prefers_left_turn(current: f32, desired: f32) -> bool {
    let offset = unsigned_heading_delta(current, desired);
    unsigned_heading_delta(normalize_heading(current - offset), desired) < 1.0
}

/// The smaller of the two arcs between two headings, never negative.
#[must_use]
pub fn unsigned_heading_delta(left: f32, right: f32) -> f32 {
    let high = if left >= right { left } else { right };
    let low = if left >= right { right } else { left };
    let inner = high - low;
    let outer = low - high + 360.0;
    if inner < outer { inner } else { outer }
}

/// A heading wrapped into [0, 360).
#[must_use]
pub fn normalize_heading(value: f32) -> f32 {
    let wrapped = value % 360.0;
    if wrapped < 0.0 {
        wrapped + 360.0
    } else {
        wrapped
    }
}

/// How far `point` is from the segment `start`–`end` across the ground,
/// metres.
#[must_use]
pub fn distance_to_segment_meters(point: &Position, start: &Position, end: &Position) -> f64 {
    let dx = end.east_west - start.east_west;
    let dy = end.north_south - start.north_south;
    let length_squared = dx * dx + dy * dy;
    // `double.Epsilon`: the smallest positive double.
    if length_squared <= f64::from_bits(1) {
        return point.horizontal_distance_metres(start);
    }
    let mut projection = ((point.east_west - start.east_west) * dx
        + (point.north_south - start.north_south) * dy)
        / length_squared;
    projection = projection.clamp(0.0, 1.0);
    let nearest_x = start.east_west + projection * dx;
    let nearest_y = start.north_south + projection * dy;
    let delta_x = point.east_west - nearest_x;
    let delta_y = point.north_south - nearest_y;
    (delta_x * delta_x + delta_y * delta_y).sqrt() * METRES_PER_MAP_UNIT
}

/// The steering of a caller that cannot hold a turn key while the player is
/// typing: outside the alignment band it stops and re-faces the goal at
/// most once per [`FACE_HEADING_REISSUE_SECONDS`]; inside it, it moves
/// forward and forgets the stamp so the next departure re-faces at once.
pub fn steer_towards(
    navigation: &mut dyn NavigationAutomation,
    signed_heading_delta_degrees: f32,
    desired_heading_degrees: f32,
    now: f64,
    face_heading_stamp: &mut f64,
    run: bool,
) -> NavigationStatus {
    if signed_heading_delta_degrees.abs() > HEADING_TOLERANCE_DEGREES {
        let stopped = navigation.clear_movement_intent();
        if stopped != NavigationStatus::Accepted {
            return stopped;
        }
        if now - *face_heading_stamp >= FACE_HEADING_REISSUE_SECONDS {
            *face_heading_stamp = now;
            let faced = navigation.face_heading(desired_heading_degrees);
            if faced != NavigationStatus::Accepted {
                return faced;
            }
        }
        return NavigationStatus::Accepted;
    }
    *face_heading_stamp = NO_FACE_HEADING_STAMP;
    navigation.set_movement_intent(MovementIntent {
        forward: true,
        run,
        ..MovementIntent::default()
    })
}

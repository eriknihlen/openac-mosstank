//! The route as the panel measures and draws it: the distance to the
//! nearest point (what a meta's "distance to route" reads), whether the
//! route holds the client's walks, and the "Show Nav Lines" drawing.

use ac_plugin_abi::ui::{MapPoint, WorldLine};
use ac_plugin_abi::world::{METRES_PER_MAP_UNIT, Position};
use mosstank_formats::route::{NavigationSettings, RouteMode, RouteWaypoint, RouteWaypointType};

use mosstank_combat::mover::MINIMUM_ARRIVAL_RADIUS_METERS;

/// How far the nearest route point is, metres. A point read from a route
/// file carries only coordinates (no cell), and that is a real position:
/// the distance is measured on the map coordinates. Only a point with no
/// position at all is left out; an empty route is infinitely far.
#[must_use]
pub fn nearest_route_distance_meters(player: &Position, waypoints: &[RouteWaypoint]) -> f64 {
    let mut nearest = f64::INFINITY;
    for waypoint in waypoints {
        if waypoint.position == Position::default() {
            continue;
        }
        nearest = nearest.min(player.horizontal_distance_metres(&waypoint.position));
    }
    nearest
}

/// Whether following the route holds a walk the client plans. It does,
/// except while the walk is the route's own: a leg the steering could not
/// cover is handed to the client's pathing, and holding that walk left the
/// character standing until the meta's watchdog restarted the route.
#[must_use]
pub fn route_holds_client_walks(route_enabled: bool, route_owns_client_walk: bool) -> bool {
    route_enabled && !route_owns_client_walk
}

/// The most points the drawing goes through.
const MAX_DRAWN_POINTS: usize = 2048;

/// The colour of the point the route is heading for.
const CURRENT_COLOR: u32 = 0x0080_FF40;

/// The colour of every other point and of the legs.
const ROUTE_COLOR: u32 = 0x00FF_40FF;

/// The segments of a point's arrival ring.
const RING_SEGMENTS: u32 = 24;

/// A hair above the ground so the line is not buried in the terrain.
fn lift(point: &Position) -> MapPoint {
    MapPoint {
        east_west: point.east_west,
        north_south: point.north_south,
        elevation: point.elevation + 0.05 / METRES_PER_MAP_UNIT,
    }
}

/// The longest leg drawn past a waypoint that is not a place, metres. A
/// route's legs are walks, but a chat command or a pause can teleport the
/// character with no waypoint saying so (`/ah`, `/ub use Gateway`, a recall
/// a meta casts during a pause): a leg longer than this past one of them
/// joins the two sides of such a teleport, and would be drawn as a line
/// across the world. A leg straight from one place to the next is a walk
/// however long it is, and is always drawn.
pub const MAX_LEG_METRES: f64 = 1000.0;

/// An object's recorded position, or `None` when the waypoint has none: a
/// NAV file marks it not valid, and a file or a waypoint that never had one
/// leaves it at the map's centre (0, 0), where no route's object stands.
fn recorded_object(waypoint: &RouteWaypoint) -> Option<&Position> {
    let position = &waypoint.reference_position;
    let unset = position.east_west == 0.0 && position.north_south == 0.0;
    (waypoint.legacy_reference_valid && !unset).then_some(position)
}

/// Where a waypoint is drawn, or `None` when it is not a place on the
/// route. A point, a checkpoint, a portal and a jump are where the
/// character stands. A portal or NPC found by name and a vendor are where
/// the object was recorded (not where the character stood when the
/// waypoint was written): the character walks to the object to use it.
/// One with no recorded object position is not drawn. A chat command, a
/// pause and a recall are done where the character is: their position is
/// wherever it was when they were added, often far from the route, and the
/// steering never goes there.
fn drawn_place(waypoint: &RouteWaypoint) -> Option<&Position> {
    match waypoint.kind {
        RouteWaypointType::Point
        | RouteWaypointType::Checkpoint
        | RouteWaypointType::Portal
        | RouteWaypointType::Jump => Some(&waypoint.position),
        RouteWaypointType::PortalByName
        | RouteWaypointType::UseNpc
        | RouteWaypointType::OpenVendor => recorded_object(waypoint),
        RouteWaypointType::Pause | RouteWaypointType::ChatCommand | RouteWaypointType::Recall => {
            None
        }
    }
}

/// Whether no leg leaves the waypoint: a portal and a recall teleport, and
/// where a jump comes down is not recorded (the steering goes on to the
/// next point from wherever it lands).
fn breaks_after(kind: RouteWaypointType) -> bool {
    matches!(
        kind,
        RouteWaypointType::Portal
            | RouteWaypointType::PortalByName
            | RouteWaypointType::Recall
            | RouteWaypointType::Jump
    )
}

/// Where a leg starts: the place's index and position, and whether a
/// waypoint that is not a place has been passed since.
struct LegStart<'a> {
    index: usize,
    from: &'a Position,
    passed_over: bool,
}

/// The loaded route as world lines ("Show Nav Lines"): a ring of the
/// arrival radius around each place on the route ([`drawn_place`]), the
/// current one in its own colour, and a leg from each place to the next
/// (back to the first on a circular route). The waypoints that are not
/// places are passed over; no leg leaves a portal, a recall or a jump, and
/// none that passes over a waypoint is longer than [`MAX_LEG_METRES`].
/// Lines follow the terrain outdoors.
#[must_use]
pub fn route_lines(
    settings: &NavigationSettings,
    current_waypoint_index: i32,
    is_outdoor: bool,
) -> Vec<WorldLine> {
    let mut lines = Vec::new();
    let points = &settings.waypoints;
    let count = points.len().min(MAX_DRAWN_POINTS);
    // The ring is the arrival radius, in map units.
    let radius = settings
        .minimum_distance_meters
        .clamp(MINIMUM_ARRIVAL_RADIUS_METERS, 50.0)
        / METRES_PER_MAP_UNIT;
    let line = |start: MapPoint, end: MapPoint, color_rgb: u32| WorldLine {
        start,
        end,
        color_rgb,
        follow_terrain: is_outdoor,
        width_metres: 0.25,
    };
    let leg = |lines: &mut Vec<WorldLine>, start: &LegStart<'_>, to: &Position| {
        if !start.passed_over || start.from.horizontal_distance_metres(to) <= MAX_LEG_METRES {
            lines.push(line(lift(start.from), lift(to), ROUTE_COLOR));
        }
    };
    // Where the next leg starts; none after a break.
    let mut previous: Option<LegStart<'_>> = None;
    for (i, waypoint) in points.iter().enumerate().take(count) {
        let Some(place) = drawn_place(waypoint) else {
            if breaks_after(waypoint.kind) {
                previous = None;
            } else if let Some(start) = previous.as_mut() {
                start.passed_over = true;
            }
            continue;
        };
        let centre = lift(place);
        let current = usize::try_from(current_waypoint_index).is_ok_and(|index| index == i);
        let color = if current { CURRENT_COLOR } else { ROUTE_COLOR };
        for j in 0..RING_SEGMENTS {
            let a = f64::from(j) * std::f64::consts::TAU / f64::from(RING_SEGMENTS);
            let b = f64::from(j + 1) * std::f64::consts::TAU / f64::from(RING_SEGMENTS);
            lines.push(line(
                MapPoint {
                    east_west: centre.east_west + a.cos() * radius,
                    north_south: centre.north_south + a.sin() * radius,
                    elevation: centre.elevation,
                },
                MapPoint {
                    east_west: centre.east_west + b.cos() * radius,
                    north_south: centre.north_south + b.sin() * radius,
                    elevation: centre.elevation,
                },
                color,
            ));
        }
        if let Some(start) = &previous {
            leg(&mut lines, start, place);
        }
        previous = (!breaks_after(waypoint.kind)).then_some(LegStart {
            index: i,
            from: place,
            passed_over: false,
        });
    }
    // A circular route goes on from its last place to its first, unless a
    // recall comes between them.
    if settings.mode == RouteMode::Circular
        && let Some(mut start) = previous
    {
        for (i, waypoint) in points.iter().enumerate().take(count) {
            if i == start.index {
                break;
            }
            if let Some(place) = drawn_place(waypoint) {
                leg(&mut lines, &start, place);
                break;
            }
            if breaks_after(waypoint.kind) {
                break;
            }
            start.passed_over = true;
        }
    }
    lines
}

/// How long after the macro last attacked it may still loot what it
/// killed, so a walk the client plans waits that long for the corpse to
/// appear, seconds.
pub const CORPSE_WAIT_SECONDS: f64 = 2.5;

/// What the macro is doing that needs the character, so a walk the client
/// plans waits for it, or `None` while the macro is idle
/// (`WalkPauseReasonFor`). The macro needs the character while it buffs,
/// while it steers the character along its own route, while a rule that
/// outranks navigation won its last pass, and, when it loots, for
/// [`CORPSE_WAIT_SECONDS`] after it last attacked. It is idle while stopped,
/// and when navigation or a rule below navigation won.
#[must_use]
pub fn walk_pause_reason_for(
    running: bool,
    last_rule: Option<&str>,
    buffing: bool,
    route_navigation: bool,
    looting: bool,
    seconds_since_attack: f64,
) -> Option<String> {
    if buffing {
        return Some("MossTank is buffing".to_owned());
    }
    if !running {
        return None;
    }
    match last_rule {
        None | Some("RandomHelper" | "IdlePeace") => {}
        Some("NavigateRoutePriority" | "NavigateRouteIdle") => {
            if route_navigation {
                return Some("MossTank is following its route".to_owned());
            }
        }
        Some(rule) => return Some(format!("MossTank is running {rule}")),
    }
    (looting && seconds_since_attack < CORPSE_WAIT_SECONDS)
        .then(|| "MossTank is waiting for a corpse to loot".to_owned())
}

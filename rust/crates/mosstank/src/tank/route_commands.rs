//! The `/vt` verbs that jump or add to the route from the chat line
//! (`MossTankCommands`: `tapjump`, `jump`, `addnavjump`, `addnavpt`,
//! `addnavcheckpoint`).

use std::sync::LazyLock;

use mosstank_automation::heading::normalize_heading;
use mosstank_dotnet::text::ordinal;
use mosstank_formats::route::{RouteJumpDirection, RouteWaypoint, RouteWaypointType};
use mosstank_ub::commands::jumper::JumpRequest;
use openac_plugin_compat::number::{self, NumberStyles};
use openac_plugin_compat::regex::{DotNetRegex, RegexOptions};

use super::options::parse_bool;
use super::{Core, TankCx};
use ac_plugin_abi::world::Position;

/// Coordinates as a player writes them: `12.3N, 45.6E` (`RoutePointCoordinates`).
static ROUTE_POINT_COORDINATES: LazyLock<DotNetRegex> = LazyLock::new(|| {
    DotNetRegex::new(
        r"(?<ns>[0-9]{1,3}(\.[0-9]*)?)(?<nschr>[nNsS])[ ]*,?[ ]*(?<ew>[0-9]{1,3}(\.[0-9]*)?)(?<ewchr>[eEwW])",
        RegexOptions::CULTURE_INVARIANT,
    )
    .expect("a fixed pattern the kit reproduces")
});

/// `TryParseJumpDirection`: nothing or `forward` is forward.
fn parse_jump_direction(value: Option<&str>) -> Option<RouteJumpDirection> {
    let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
        return Some(RouteJumpDirection::Forward);
    };
    if ordinal::equals_ignore_case(value, "forward") {
        Some(RouteJumpDirection::Forward)
    } else if ordinal::equals_ignore_case(value, "strafeleft") {
        Some(RouteJumpDirection::StrafeLeft)
    } else if ordinal::equals_ignore_case(value, "straferight") {
        Some(RouteJumpDirection::StrafeRight)
    } else if ordinal::equals_ignore_case(value, "backward") {
        Some(RouteJumpDirection::Backward)
    } else {
        None
    }
}

impl TankCx<'_> {
    /// `/vt tapjump`: a short hop the way the character faces.
    pub(crate) fn tap_jump(&mut self) {
        let heading = self
            .client
            .rules()
            .navigation_snapshot()
            .position
            .heading_degrees;
        self.start_command_jump(&JumpRequest::new(heading, 100));
    }

    /// `StartCommandJump` with the macro's held-key turn.
    fn start_command_jump(&mut self, request: &JumpRequest) {
        self.with_ub(|commands, call| commands.jump_mut().start(call, request));
    }

    /// `HandleJumpCommand`: `/vt jump` jumps now; `/vt addnavjump` adds the
    /// jump to the route where the character stands.
    pub(crate) fn jump_command(&mut self, arguments: &str, add_to_route: bool) {
        let parts: Vec<&str> = arguments.split(' ').filter(|p| !p.is_empty()).collect();
        let parsed = (3..=4).contains(&parts.len()).then(|| {
            let heading = number::parse_f32(parts[0], NumberStyles::FLOAT)?;
            let shift = parse_bool(parts[1])?;
            let milliseconds = number::parse_i32(parts[2], NumberStyles::INTEGER)
                .filter(|ms| (0..=5000).contains(ms))?;
            let direction = parse_jump_direction(parts.get(3).copied())?;
            Some((heading, shift, milliseconds, direction))
        });
        let Some(Some((heading, shift, milliseconds, direction))) = parsed else {
            Core::write_vtank(
                self.client,
                if add_to_route {
                    "Syntax: /vt addnavjump [heading] [shift: true or false] [milliseconds] [forward|backward|strafeleft|straferight]"
                } else {
                    "Syntax: /vt jump [heading] [shift: true or false] [milliseconds] [forward|backward|strafeleft|straferight]"
                },
            );
            return;
        };
        if add_to_route {
            let position = self.client.rules().navigation_snapshot().position;
            self.core.settings.navigation.waypoints.push(RouteWaypoint {
                kind: RouteWaypointType::Jump,
                position,
                jump_heading_degrees: f64::from(normalize_heading(heading)),
                jump_hold_shift: shift,
                jump_charge_milliseconds: milliseconds,
                jump_direction: direction,
                ..RouteWaypoint::default()
            });
            let saved = self.core.save_route_profile(self.client);
            self.refresh_route_editor();
            Core::write_vtank(self.client, "Added jump to the current route.");
            if !saved {
                // The route's file cannot hold what was just added; said now
                // rather than at the next save.
                let notice = self.core.route_notice.clone();
                Core::write_vtank(self.client, &notice);
            } else if direction == RouteJumpDirection::Backward {
                // The interchange format has no backward jump, so a saved
                // route reads this one back as forward.
                Core::write_vtank(
                    self.client,
                    "Note: a saved route file has no backward jump; this one will read back as forward.",
                );
            }
            return;
        }
        let mut request = JumpRequest::new(heading, milliseconds);
        request.shift = shift;
        request.direction = Some(direction);
        self.start_command_jump(&request);
    }

    /// `AddCommandRoutePoint`: `/vt addnavpt` and `/vt addnavcheckpoint`,
    /// where the character stands or at the coordinates given.
    pub(crate) fn add_command_route_point(&mut self, coordinates: &str, checkpoint: bool) {
        let position = if coordinates.is_empty() {
            self.client.rules().navigation_snapshot().position
        } else if let Some(position) = self.parse_coordinates(coordinates) {
            position
        } else {
            Core::write_vtank(
                self.client,
                if checkpoint {
                    "Syntax: /vt addnavcheckpoint [coords], or /vt addnavcheckpoint alone"
                } else {
                    "Syntax: /vt addnavpt [coords], or /vt addnavpt alone"
                },
            );
            return;
        };
        self.core.settings.navigation.waypoints.push(RouteWaypoint {
            kind: if checkpoint {
                RouteWaypointType::Checkpoint
            } else {
                RouteWaypointType::Point
            },
            position,
            ..RouteWaypoint::default()
        });
        self.core.save_route_profile(self.client);
        self.refresh_route_editor();
        Core::write_vtank(
            self.client,
            if checkpoint {
                "Added navigation checkpoint."
            } else {
                "Added navigation point."
            },
        );
    }

    /// `TryParseCoordinates`: on the landscape, in the cell and facing the
    /// way the character is now.
    fn parse_coordinates(&mut self, source: &str) -> Option<Position> {
        let found = ROUTE_POINT_COORDINATES.find(source).ok()??;
        let mut north_south = number::parse_f64(found.group("ns").value, NumberStyles::FLOAT)?;
        let mut east_west = number::parse_f64(found.group("ew").value, NumberStyles::FLOAT)?;
        if matches!(found.group("nschr").value, "s" | "S") {
            north_south = -north_south;
        }
        if matches!(found.group("ewchr").value, "w" | "W") {
            east_west = -east_west;
        }
        let current = self.client.rules().navigation_snapshot().position;
        Some(Position {
            cell_id: current.cell_id,
            east_west,
            north_south,
            elevation: 0.0,
            heading_degrees: current.heading_degrees,
            is_outdoor: true,
        })
    }
}

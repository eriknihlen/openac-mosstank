//! The calls the C# panel's controls make that the shell already does the
//! work of: picking a route or a meta, adding a route point. The windows
//! (MT-11) bind their controls to these; the shell's tests call them as the
//! C# tests press the buttons.

use mosstank_formats::route::{RouteWaypoint, RouteWaypointType};

use super::{Core, Tank};
use crate::client::TankClient;

impl Tank {
    /// Picks a route by name (`SelectRouteProfile`): the meta's own route
    /// when it is that route's label.
    pub fn select_route_profile(&mut self, client: &mut dyn TankClient, name: &str) {
        self.begin_call(client);
        let (mut cx, _) = self.cx(client);
        cx.select_route_profile(name);
        cx.core.write_out(cx.client);
        self.end_call(client);
    }

    /// Picks a meta by name (`SelectMetaProfile`).
    pub fn select_meta_profile(&mut self, client: &mut dyn TankClient, name: &str) {
        self.begin_call(client);
        let (mut cx, _) = self.cx(client);
        cx.select_meta_profile(name);
        cx.core.write_out(cx.client);
        self.end_call(client);
    }

    /// Adds a point where the character stands, at the end of the route
    /// (`AddRoutePoint` with the editor's default placement).
    pub fn add_route_point(&mut self, client: &mut dyn TankClient) {
        self.begin_call(client);
        let snapshot = client.rules().navigation_snapshot();
        if snapshot.is_available {
            let notice = format!(
                "Added point {}.",
                mosstank_formats::route::format_position(&snapshot.position)
            );
            let waypoint = RouteWaypoint {
                kind: RouteWaypointType::Point,
                position: snapshot.position,
                ..RouteWaypoint::default()
            };
            self.core.add_route_waypoint(client, waypoint);
            self.core.route_notice = notice;
        } else {
            "Current position is unavailable.".clone_into(&mut self.core.route_notice);
        }
        self.core.write_out(client);
        self.end_call(client);
    }

    /// The route's points, as the route editor lists them (`RouteRows`).
    #[must_use]
    pub fn route_waypoint_count(&self) -> usize {
        self.core.settings.navigation.waypoints.len()
    }
}

impl Core {
    /// `AddRouteWaypoint` at the end of the route: the round starts over and
    /// the route is saved.
    pub(crate) fn add_route_waypoint(
        &mut self,
        client: &mut dyn TankClient,
        waypoint: RouteWaypoint,
    ) {
        self.settings.navigation.waypoints.push(waypoint);
        {
            let mut turn = super::rules::route_turn!(client, self);
            self.navigation.reset(&mut turn);
        }
        self.save_route_profile(client);
    }
}

impl Tank {
    /// The one option write path (`SetMetaOption`): false for a name that is
    /// no option of the reference's.
    ///
    /// # Errors
    /// A value of the wrong type for the option: the message.
    pub fn set_meta_option(
        &mut self,
        client: &mut dyn TankClient,
        name: &str,
        value: &mosstank_expr::ExpressionValue,
    ) -> Result<bool, String> {
        self.begin_call(client);
        let (mut cx, _) = self.cx(client);
        let written = cx
            .set_meta_option(name, value)
            .map_err(|fault| fault.message());
        cx.core.write_out(cx.client);
        self.end_call(client);
        written
    }
}

impl Tank {
    /// The Remote window's Follow Selected (`RemoteFollowSelected`).
    pub fn remote_follow_selected(&mut self, client: &mut dyn TankClient) {
        self.begin_call(client);
        let (mut cx, _) = self.cx(client);
        cx.with_ub(|_, call| mosstank_ub::commands::follow_selected(call));
        cx.core.write_out(cx.client);
        self.end_call(client);
    }

    /// The route picker's names (`RouteProfileNames`): the meta's own route
    /// first while one is loaded.
    pub fn route_profile_names(&self, client: &mut dyn TankClient) -> Vec<String> {
        let mut names: Vec<String> = self.core.embedded_route_label.iter().cloned().collect();
        names.extend(
            self.core
                .route_profiles
                .available_names(client.stores())
                .unwrap_or_default(),
        );
        names
    }

    /// The meta picker's names (`MetaProfileNames`).
    pub fn meta_profile_names(&self, client: &mut dyn TankClient) -> Vec<String> {
        self.core
            .meta_profiles
            .available_names(client.stores())
            .unwrap_or_default()
    }
}

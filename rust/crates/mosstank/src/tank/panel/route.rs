//! The Route page: the waypoint grid, the add buttons, the route's mode,
//! where new points go, the recall and pause and chat drafts, the follow
//! target and the client's pathing.

use ac_plugin_abi::ui::UiValue;
use mosstank_automation::heading::horizontal_distance_meters;
use mosstank_formats::route::{
    ClientPathing, RouteInsertMode, RouteJumpDirection, RouteMode, RouteRecallKind, RouteWaypoint,
    RouteWaypointType, format_position,
};
use openac_plugin_compat::number::{self, NumberStyles};

use super::{Arg, b, clamp_row, i, row, t, tl};
use crate::tank::TankCx;

/// The colour an ordinary waypoint row draws in.
const ROUTE_WAYPOINT_COLOR: u32 = 0x00E8_DEC3;
/// The colour a once route's spent waypoint draws in.
const ROUTE_WAYPOINT_CONSUMED_COLOR: u32 = 0x00C0_3838;

/// `RouteModeNames`.
const ROUTE_MODE_NAMES: [&str; 4] = ["Circular", "Linear", "Follow", "Once"];
/// `RouteInsertModeNames`.
const ROUTE_INSERT_MODE_NAMES: [&str; 3] = ["Add to End", "Insert Above", "Insert Below"];
/// `ClientPathingNames`.
const CLIENT_PATHING_NAMES: [&str; 3] = ["Never", "When stuck", "Always"];

/// `ClientPathingName`.
pub(crate) fn client_pathing_name(value: ClientPathing) -> &'static str {
    match value {
        ClientPathing::Never => "Never",
        ClientPathing::Always => "Always",
        ClientPathing::WhenStuck => "When stuck",
    }
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_owned()).collect()
}

pub(super) fn value(cx: &mut TankCx<'_>, name: &str) -> Option<UiValue> {
    match name {
        "RouteRows" | "RouteWaypointTextColumn" => tl(cx.route_rows()),
        "RouteWaypointCountColumn" => {
            let count = cx.core.settings.navigation.waypoints.len();
            tl((1..=count).map(|index| index.to_string()).collect())
        }
        "RouteWaypointColors" => {
            let core = &cx.core;
            let count = core.settings.navigation.waypoints.len();
            Some(UiValue::UintList(
                (0..count)
                    .map(|index| {
                        let index = i32::try_from(index).unwrap_or(i32::MAX);
                        if core
                            .navigation
                            .is_waypoint_consumed(&core.settings.navigation, index)
                        {
                            ROUTE_WAYPOINT_CONSUMED_COLOR
                        } else {
                            ROUTE_WAYPOINT_COLOR
                        }
                    })
                    .collect(),
            ))
        }
        "RouteWaypointFillerColumn" => tl(Vec::new()),
        "SelectedRouteWaypointIndex" => {
            let count = cx.core.settings.navigation.waypoints.len();
            let selected = clamp_row(cx.core.panel.selected_route_waypoint, count);
            cx.core.panel.selected_route_waypoint = selected;
            i(selected)
        }
        "RouteModeNames" => tl(names(&ROUTE_MODE_NAMES)),
        "SelectedRouteMode" => t(if cx.core.settings.navigation.mode == RouteMode::Target {
            "Follow"
        } else {
            cx.core.settings.navigation.mode.name()
        }),
        "ClientPathingNames" => tl(names(&CLIENT_PATHING_NAMES)),
        "SelectedClientPathing" => t(client_pathing_name(
            cx.core.settings.navigation.client_pathing,
        )),
        "RouteRecallNames" => tl(RouteRecallKind::ALL
            .iter()
            .map(|kind| kind.short_caption().to_owned())
            .collect()),
        "SelectedRouteRecall" => t(cx.core.panel.route_recall_kind.short_caption()),
        "RouteNotice" => t(cx.core.route_notice.clone()),
        "RouteChatDraft" => t(cx.core.panel.route_chat_draft.clone()),
        "RoutePauseSecondsFieldText" => t(cx.core.panel.route_pause_seconds.to_string()),
        "RouteMinimumDistanceText" => t(format!(
            "Follow/Nav Min Distance: {}m",
            number::format_f64(cx.core.settings.navigation.minimum_distance_meters, "0.0")
                .unwrap_or_default()
        )),
        "RouteFollowTargetText" => {
            let navigation = &cx.core.settings.navigation;
            t(if navigation.follow_target_object_id == 0 {
                "Follow target: [None]".to_owned()
            } else {
                format!("Follow target: {}", navigation.follow_target_name)
            })
        }
        "RouteInsertModeNames" => tl(names(&ROUTE_INSERT_MODE_NAMES)),
        "SelectedRouteInsertMode" => t(match cx.core.panel.route_insert_mode {
            RouteInsertMode::InsertAbove => "Insert Above",
            RouteInsertMode::InsertBelow => "Insert Below",
            RouteInsertMode::AddToEnd => "Add to End",
        }),
        "OpenDoorsEnabled" => b(cx.core.settings.navigation.open_doors),
        _ => None,
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn act(cx: &mut TankCx<'_>, name: &str, arg: &Arg) -> bool {
    match name {
        "SelectRouteWaypoint" => {
            let count = cx.core.settings.navigation.waypoints.len();
            cx.core.panel.selected_route_waypoint = clamp_row(arg.int(), count);
        }
        "DeleteRouteWaypointAt" => cx.delete_route_waypoint_at(arg.int()),
        "RouteWaypointFillerClick" => {}
        "SelectNearestRouteWaypoint" => cx.select_nearest_route_waypoint(),
        "SkipRouteWaypoint" => {
            let line = cx.skip_route_waypoints(1);
            crate::tank::Core::write_vtank(cx.client, &line);
        }
        "SelectRouteMode" => cx.select_route_mode(arg.text()),
        "SelectClientPathing" => {
            let chosen = match mosstank_dotnet::text::casing::to_lower_invariant(arg.text().trim())
                .as_str()
            {
                "never" => ClientPathing::Never,
                "always" => ClientPathing::Always,
                _ => ClientPathing::WhenStuck,
            };
            if chosen == cx.core.settings.navigation.client_pathing {
                return true;
            }
            cx.core.settings.navigation.client_pathing = chosen;
            let core = &mut *cx.core;
            let mut turn = crate::tank::rules::route_turn!(cx.client, core);
            core.navigation.client_pathing_changed(&mut turn);
            cx.core.request_profile_save();
        }
        "SelectRouteRecall" => {
            let wanted = arg.text();
            if let Some(kind) = RouteRecallKind::ALL.iter().copied().find(|kind| {
                mosstank_dotnet::text::ordinal::equals_ignore_case(kind.short_caption(), wanted)
            }) {
                cx.core.panel.route_recall_kind = kind;
            }
        }
        "SelectRouteInsertMode" => {
            cx.core.panel.route_insert_mode = match arg.text() {
                "Insert Above" => RouteInsertMode::InsertAbove,
                "Insert Below" => RouteInsertMode::InsertBelow,
                _ => RouteInsertMode::AddToEnd,
            };
        }
        "AddRoutePoint" => {
            let snapshot = cx.client.rules().navigation_snapshot();
            if !snapshot.is_available {
                "Current position is unavailable.".clone_into(&mut cx.core.route_notice);
                return true;
            }
            cx.add_route_waypoint(RouteWaypoint {
                kind: RouteWaypointType::Point,
                position: snapshot.position.clone(),
                ..RouteWaypoint::default()
            });
            cx.core.route_notice = format!("Added point {}.", format_position(&snapshot.position));
        }
        "AddRouteCheckpoint" => {
            let snapshot = cx.client.rules().navigation_snapshot();
            if !snapshot.is_available {
                "Current position is unavailable.".clone_into(&mut cx.core.route_notice);
                return true;
            }
            cx.add_route_waypoint(RouteWaypoint {
                kind: RouteWaypointType::Checkpoint,
                position: snapshot.position,
                ..RouteWaypoint::default()
            });
            "Added checkpoint.".clone_into(&mut cx.core.route_notice);
        }
        "AddRouteUseSelected" => cx.add_selected_object_waypoint(RouteWaypointType::UseNpc),
        "AddRouteOpenVendor" => cx.add_selected_object_waypoint(RouteWaypointType::OpenVendor),
        "AddRoutePortal" => cx.add_selected_object_waypoint(RouteWaypointType::PortalByName),
        "AddRouteRecall" => {
            let kind = cx.core.panel.route_recall_kind;
            let name = kind.display_name();
            let position = cx.client.rules().navigation_snapshot().position;
            cx.add_route_waypoint(RouteWaypoint {
                kind: RouteWaypointType::Recall,
                recall: kind,
                recall_spell_id: kind.spell_id(),
                recall_spell_name: name.to_owned(),
                position,
                ..RouteWaypoint::default()
            });
            cx.core.route_notice = format!("Added {name}.");
        }
        "AddRoutePause" => {
            let seconds = cx.core.panel.route_pause_seconds;
            let position = cx.client.rules().navigation_snapshot().position;
            cx.add_route_waypoint(RouteWaypoint {
                kind: RouteWaypointType::Pause,
                duration_milliseconds: seconds * 1000,
                position,
                ..RouteWaypoint::default()
            });
            cx.core.route_notice = format!("Added {seconds}-second pause.");
        }
        "AddRouteChat" => {
            let text = cx.core.panel.route_chat_draft.trim().to_owned();
            if text.is_empty() {
                "Enter a chat command first.".clone_into(&mut cx.core.route_notice);
                return true;
            }
            let position = cx.client.rules().navigation_snapshot().position;
            cx.add_route_waypoint(RouteWaypoint {
                kind: RouteWaypointType::ChatCommand,
                text: text.clone(),
                position,
                ..RouteWaypoint::default()
            });
            cx.core.route_notice = format!("Added chat command {text}.");
        }
        "AddRouteJump" => {
            let snapshot = cx.client.rules().navigation_snapshot();
            if !snapshot.is_available {
                "Current heading is unavailable.".clone_into(&mut cx.core.route_notice);
                return true;
            }
            let heading = f64::from(snapshot.position.heading_degrees);
            cx.add_route_waypoint(RouteWaypoint {
                kind: RouteWaypointType::Jump,
                position: snapshot.position,
                jump_heading_degrees: heading,
                jump_hold_shift: false,
                jump_charge_milliseconds: 1000,
                jump_direction: RouteJumpDirection::Forward,
                ..RouteWaypoint::default()
            });
            "Added forward jump.".clone_into(&mut cx.core.route_notice);
        }
        "RemoveRouteWaypoint" => {
            let count = cx.core.settings.navigation.waypoints.len();
            if count == 0 {
                "The route is empty.".clone_into(&mut cx.core.route_notice);
                return true;
            }
            let index = clamp_row(cx.core.panel.selected_route_waypoint, count);
            let index_usize = usize::try_from(index).unwrap_or(0);
            let removed = cx.core.settings.navigation.waypoints[index_usize].display_text();
            cx.core.settings.navigation.waypoints.remove(index_usize);
            cx.core.panel.selected_route_waypoint =
                clamp_row(index, cx.core.settings.navigation.waypoints.len());
            cx.reset_navigation();
            cx.core.save_route_profile(cx.client);
            cx.core.route_notice = format!("Removed {removed}.");
        }
        "MoveRouteWaypointUp" => cx.move_route_waypoint(-1),
        "MoveRouteWaypointDown" => cx.move_route_waypoint(1),
        "RouteMinimumDistanceDown" => {
            let navigation = &mut cx.core.settings.navigation;
            navigation.minimum_distance_meters =
                (navigation.minimum_distance_meters - 0.5).max(0.5);
            cx.core.save_route_profile(cx.client);
        }
        "RouteMinimumDistanceUp" => {
            let navigation = &mut cx.core.settings.navigation;
            navigation.minimum_distance_meters =
                (navigation.minimum_distance_meters + 0.5).min(50.0);
            cx.core.save_route_profile(cx.client);
        }
        "ToggleOpenDoors" => {
            cx.core.settings.navigation.open_doors = !cx.core.settings.navigation.open_doors;
            cx.core.save_route_profile(cx.client);
        }
        "SetRouteChatDraft" => arg.text().clone_into(&mut cx.core.panel.route_chat_draft),
        "SetRoutePauseSecondsText" => {
            if let Some(seconds) = number::parse_i32(arg.text(), NumberStyles::INTEGER) {
                cx.core.panel.route_pause_seconds = seconds.clamp(0, 3600);
            }
        }
        "SetFollowTarget" => cx.capture_follow_target(),
        _ => return false,
    }
    true
}

impl TankCx<'_> {
    /// The waypoint rows, the one being walked to marked
    /// (`RefreshRouteEditor`'s rows).
    pub(crate) fn route_rows(&mut self) -> Vec<String> {
        let active = self.core.navigation.current_waypoint_index();
        let enabled = self.core.settings.navigation.enabled;
        let core = &mut *self.core;
        let waypoints = &core.settings.navigation.waypoints;
        // The waypoints' words are kept while the waypoints are the same:
        // the grid is pushed often and wording a waypoint is costly.
        let memo = &mut core.memo.route_texts;
        if memo.as_ref().is_none_or(|(kept, _)| kept != waypoints) {
            let texts = waypoints.iter().map(RouteWaypoint::display_text).collect();
            *memo = Some((waypoints.clone(), texts));
        }
        let texts = memo.as_ref().map_or(&[][..], |(_, texts)| texts.as_slice());
        texts
            .iter()
            .enumerate()
            .map(|(index, text)| {
                let marker = if i32::try_from(index).ok() == Some(active) && enabled {
                    "<<"
                } else {
                    "  "
                };
                format!("{marker} {text}")
            })
            .collect()
    }

    /// `RefreshRouteEditor`: the selection kept inside the route (the rows
    /// are read fresh).
    pub(crate) fn refresh_route_editor(&mut self) {
        let count = self.core.settings.navigation.waypoints.len();
        self.core.panel.selected_route_waypoint =
            clamp_row(self.core.panel.selected_route_waypoint, count);
    }

    /// `SkipRouteWaypoints`: what the route now heads for, in words.
    pub(crate) fn skip_route_waypoints(&mut self, count: i32) -> String {
        if self.core.settings.navigation.mode == RouteMode::Target {
            return "A follow route has no waypoints to skip.".to_owned();
        }
        let core = &mut *self.core;
        let mut turn = crate::tank::rules::route_turn!(self.client, core);
        let skipped = core.navigation.skip_waypoints(&mut turn, count);
        if skipped == 0 {
            return "The route has no waypoint left to skip.".to_owned();
        }
        self.refresh_route_editor();
        let core = &*self.core;
        let noun = if skipped == 1 {
            "waypoint"
        } else {
            "waypoints"
        };
        if core
            .navigation
            .has_nothing_left_to_walk(&core.settings.navigation)
        {
            format!("Skipped {skipped} {noun}; the route is complete.")
        } else {
            format!(
                "Skipped {skipped} {noun}; now heading for waypoint {}/{}.",
                core.navigation.current_waypoint_index() + 1,
                core.settings.navigation.waypoints.len()
            )
        }
    }

    /// `AddRouteWaypoint`: at the end, above or below the selection.
    fn add_route_waypoint(&mut self, waypoint: RouteWaypoint) {
        let count = self.core.settings.navigation.waypoints.len();
        let selected = usize::try_from(self.core.panel.selected_route_waypoint.max(0)).unwrap_or(0);
        let insertion = match self.core.panel.route_insert_mode {
            RouteInsertMode::InsertAbove => selected.min(count),
            RouteInsertMode::InsertBelow => count.min(selected + 1),
            RouteInsertMode::AddToEnd => count,
        };
        self.core
            .settings
            .navigation
            .waypoints
            .insert(insertion, waypoint);
        self.core.panel.selected_route_waypoint = i32::try_from(insertion).unwrap_or(0);
        self.reset_navigation();
        self.refresh_route_editor();
        self.core.save_route_profile(self.client);
    }

    /// `AddSelectedObjectWaypoint`.
    fn add_selected_object_waypoint(&mut self, kind: RouteWaypointType) {
        let selected = self.client.rules().selected_object().unwrap_or(0);
        let target = if selected == 0 {
            None
        } else {
            self.client.rules().navigation_object(selected)
        };
        let Some(target) = target else {
            "Select a live portal, NPC, or vendor first.".clone_into(&mut self.core.route_notice);
            return;
        };
        let position = self.client.rules().navigation_snapshot().position;
        self.add_route_waypoint(RouteWaypoint {
            kind,
            position,
            reference_position: target.position.clone(),
            object_id: target.object_id,
            object_name: target.name.clone(),
            ..RouteWaypoint::default()
        });
        self.core.route_notice = format!("Added {}: {}.", kind.name(), target.name);
    }

    /// `DeleteRouteWaypointAtCore`.
    fn delete_route_waypoint_at(&mut self, index: i32) {
        let waypoints = &mut self.core.settings.navigation.waypoints;
        let Some(at) = row(index, waypoints.len()) else {
            return;
        };
        let removed = waypoints.remove(at).display_text();
        self.core.panel.selected_route_waypoint = clamp_row(
            self.core.panel.selected_route_waypoint,
            self.core.settings.navigation.waypoints.len(),
        );
        self.reset_navigation();
        self.core.save_route_profile(self.client);
        self.core.route_notice = format!("Removed {removed}.");
    }

    /// `SelectNearestRouteWaypointCore`.
    fn select_nearest_route_waypoint(&mut self) {
        let player = self.client.rules().navigation_snapshot();
        let waypoints = &self.core.settings.navigation.waypoints;
        if !player.is_available || waypoints.is_empty() {
            "Current position is unavailable.".clone_into(&mut self.core.route_notice);
            return;
        }
        let mut best = 0;
        let mut best_distance = f64::INFINITY;
        for (index, waypoint) in waypoints.iter().enumerate() {
            if waypoint.position == ac_plugin_abi::world::Position::default() {
                continue;
            }
            let distance = horizontal_distance_meters(&player.position, &waypoint.position);
            if distance < best_distance {
                best_distance = distance;
                best = index;
            }
        }
        let text = waypoints[best].display_text();
        self.core.panel.selected_route_waypoint = i32::try_from(best).unwrap_or(0);
        self.core.route_notice = format!("Selected nearest waypoint ({text}).");
    }

    /// `MoveRouteWaypoint`.
    fn move_route_waypoint(&mut self, direction: i32) {
        let count = self.core.settings.navigation.waypoints.len();
        if count < 2 {
            return;
        }
        let source = clamp_row(self.core.panel.selected_route_waypoint, count);
        let destination = source + direction.signum();
        let Some(destination) = row(destination, count) else {
            return;
        };
        let source = usize::try_from(source).unwrap_or(0);
        let waypoints = &mut self.core.settings.navigation.waypoints;
        let waypoint = waypoints.remove(source);
        waypoints.insert(destination, waypoint);
        self.core.panel.selected_route_waypoint = i32::try_from(destination).unwrap_or(0);
        self.reset_navigation();
        self.refresh_route_editor();
        self.core.save_route_profile(self.client);
    }

    /// `SelectRouteMode`: Follow aims the follow route; any other mode is
    /// the loaded route's.
    fn select_route_mode(&mut self, value: &str) {
        let normalized = if mosstank_dotnet::text::ordinal::equals_ignore_case(value, "Follow") {
            "Target"
        } else {
            value
        };
        let Some(mode) = [
            RouteMode::Circular,
            RouteMode::Linear,
            RouteMode::Target,
            RouteMode::Once,
        ]
        .into_iter()
        .find(|mode| {
            mosstank_dotnet::text::ordinal::equals_ignore_case(mode.name(), normalized.trim())
        }) else {
            return;
        };
        if mode == RouteMode::Target {
            self.capture_follow_target();
            return;
        }
        self.core.settings.navigation.mode = mode;
        self.reset_navigation();
        self.refresh_route_editor();
        self.core.save_route_profile(self.client);
    }

    /// `CaptureFollowTarget`: the follow route aimed at the selection.
    pub(crate) fn capture_follow_target(&mut self) {
        let selected = self.client.rules().selected_object().unwrap_or(0);
        let target = if selected == 0 {
            None
        } else {
            self.client.rules().navigation_object(selected)
        };
        let Some(target) = target else {
            "Select a live object to follow first.".clone_into(&mut self.core.route_notice);
            return;
        };
        if !self.follow_on_follow_route(target.object_id, &target.name) {
            let selected = crate::tank::profiles::report!(
                self.client,
                self.core.route_profiles.selected(self.client.stores())
            )
            .unwrap_or_default();
            self.core.route_notice = format!("Route profile {selected} could not be read.");
            return;
        }
        self.core.route_notice = if self.core.settings.navigation.enabled {
            format!("Following {}.", target.name)
        } else {
            format!(
                "Following {}. Turn Enable Navigation on to start.",
                target.name
            )
        };
    }

    /// `FollowOnFollowRoute`, with the route editor rebuilt.
    pub(crate) fn follow_on_follow_route(&mut self, object_id: u32, name: &str) -> bool {
        let (followed, meta_enabled) =
            self.core
                .follow_on_follow_route(self.client, object_id, name);
        if let Some(enabled) = meta_enabled {
            self.meta.set_enabled(enabled);
        }
        if followed {
            self.refresh_route_editor();
        }
        followed
    }
}

//! The route follower (`NavigationController`): which waypoint the route
//! is heading for, how it gets there, what it does on arrival, and the door
//! turn that shares its mover and door settings.
//!
//! The route itself (the waypoints and how they are run) is the player's
//! [`NavigationSettings`], handed in with the client for each call as a
//! [`RouteTurn`]; the controller keeps only the round's position and what
//! is in flight. Every edit to the route list resets the controller (the
//! panel does so), which is why the waypoint being worked is known by its
//! place in the list.

use std::collections::HashSet;
use std::rc::Rc;

use ac_plugin_abi::combat::CombatMode;
use ac_plugin_abi::magic::{CastResult, SpellList};
use ac_plugin_abi::navigation::{
    GoToState, MoveChannel, MoveDirection, MovePace, MoveReport, MoveUnit, MovementIntent,
    NavigationObject, NavigationSnapshot, NavigationStatus,
};
use ac_plugin_abi::types::ChatLine;
use ac_plugin_abi::world::{METRES_PER_MAP_UNIT, Position};
use mosstank_automation::Automation;
use mosstank_automation::heading::{distance_to_segment_meters, signed_heading_delta};
use mosstank_automation::locks::{ActionLockKind, SharedActionLocks};
use mosstank_combat::gate::{GateTurn, Prepare, SharedCombatModeGate};
use mosstank_combat::mover::{
    CREEP_DISTANCE_METERS, CreepSettings, FACE_HEADING_REISSUE_SECONDS, HEADING_TOLERANCE_DEGREES,
    MINIMUM_ARRIVAL_RADIUS_METERS, NO_FACE_HEADING_STAMP, NavigationMover, STUCK_SECONDS,
};
use mosstank_formats::route::{
    ClientPathing, NavigationSettings, RouteJumpDirection, RouteMode, RouteRecallKind,
    RouteStepBackBlock, RouteWaypoint, RouteWaypointType, SpellLookup,
};
use openac_plugin_compat::number::{format_f32, format_f64};
use openac_plugin_compat::text::{casing, ordinal};

use crate::client::{self, NavigationAutomation};
use crate::jump_power;

const CHAT_INITIAL_DELAY_SECONDS: f64 = 0.2;
const USE_RETRY_SECONDS: f64 = 2.0;
const PORTAL_TIMEOUT_SECONDS: f64 = 30.0;
const OBJECT_REACQUIRE_RADIUS_METERS: f64 = 2.5;
const PORTAL_EXIT_DISTANCE_METERS: f64 = 15.0;
const RECALL_EXIT_DISTANCE_METERS: f64 = 2.4;

/// How far the character may have drifted between ticks and still count as
/// standing still for a recall.
const RECALL_STATIONARY_TOLERANCE_METERS: f64 = 2.4;
const JUMP_LAUNCH_GRACE_SECONDS: f64 = 0.25;
const JUMP_COMPLETION_TIMEOUT_SECONDS: f64 = 3.0;
const JUMP_CHARGE_CEILING_MILLISECONDS: i32 = 2000;

/// A jump is aimed near-exactly, not to the walking band: the alignment
/// holds until the heading error is under a hundredth of a degree.
const JUMP_HEADING_TOLERANCE_DEGREES: f32 = 0.01;

/// The jump's own re-face interval, far longer than the walk's.
const JUMP_FACE_HEADING_REISSUE_SECONDS: f64 = 2.0;

/// The log-text type an NPC's "tells you," answer arrives with.
const NPC_TELL_LOG_TEXT_TYPE: u32 = 3;

/// The log-text type an NPC's "gives you" line arrives with.
const NPC_GIVE_LOG_TEXT_TYPE: u32 = 0;
const CHECKPOINT_RETRY_SECONDS: f64 = 15.0;
const FOLLOW_BREADCRUMB_SPACING_METERS: f64 = 0.096;
const FOLLOW_PATH_CAPTURE_RANGE_METERS: f64 = 240.0;
const FOLLOW_PATH_ARRIVAL_METERS: f64 = 2.4;
const DOOR_ACTION_TIMEOUT_SECONDS: f64 = 5.0;
const LOCKPICK_PUBLIC_FLAG: u32 = 0x0002_0000;
const LOCKPICK_SKILL_ID: u32 = 23;
const LOCKPICK_MODIFIER_PROPERTY: u32 = 40;

/// How long a checkpoint nudge holds the pass, seconds.
const CHECKPOINT_NUDGE_SECONDS: f64 = 0.1;

/// The warning a tight waypoint walked at in peace mode posts, once a run.
pub const LOW_WAYPOINT_DISTANCE_WARNING: &str =
    "Warning: Idle peace selected with low waypoint minimum distance. Will switch to magic mode.";

/// What one call works with: the client, the route, and the combat and
/// vital profiles the macro's combat-mode gate reads (the creep push and a
/// recall ask it for magic mode); without them the gate is not asked, as
/// with no gate bound.
#[allow(missing_debug_implementations)]
pub struct RouteTurn<'a> {
    /// The client.
    pub automation: &'a mut dyn NavigationAutomation,
    /// The route and how it is run.
    pub settings: &'a mut NavigationSettings,
    /// The combat and vital settings, read live as the C# reads them.
    pub creep: Option<CreepSettings<'a>>,
}

impl<'a> RouteTurn<'a> {
    /// A turn without the combat profile.
    pub fn new(
        automation: &'a mut dyn NavigationAutomation,
        settings: &'a mut NavigationSettings,
    ) -> Self {
        Self {
            automation,
            settings,
            creep: None,
        }
    }
}

/// The route's status line and its once-per-run tight-goal warning.
#[derive(Debug)]
struct RouteVoice {
    status: String,
    low_waypoint_warning_posted: bool,
}

impl RouteVoice {
    fn warn_low_waypoint_distance(&mut self, automation: &mut dyn Automation) {
        if self.low_waypoint_warning_posted {
            return;
        }
        self.low_waypoint_warning_posted = true;
        client::post(
            automation,
            &format!("{} {LOW_WAYPOINT_DISTANCE_WARNING}", client::CHAT_TAG),
        );
    }
}

/// The steering of a caller that cannot hold a turn key while the player is
/// typing (`NavigationController.SteerTowards`): outside the alignment band
/// it stops and re-faces the goal at most once per re-face interval; inside
/// it, it moves forward and forgets the stamp so the next departure
/// re-faces at once.
pub fn steer_towards(
    navigation: &mut dyn Automation,
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

/// A recall name looked up in the live spell catalogue: what
/// [`RouteWaypoint::resolve_recall_spell`] asks of it, read once.
struct CatalogLookup {
    found: Option<u32>,
    known: Vec<(u32, String)>,
}

impl SpellLookup for CatalogLookup {
    fn find_by_name(&self, _name: &str) -> Option<u32> {
        self.found
    }

    fn known_spells(&self) -> Vec<(u32, String)> {
        self.known.clone()
    }

    fn name_of(&self, _spell_id: u32) -> Option<String> {
        None
    }
}

/// The waypoint whose action is being worked, by its place in the route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActiveAction {
    index: usize,
    kind: RouteWaypointType,
}

/// The route follower.
pub struct NavigationController {
    index: i32,
    reverse: bool,
    once_complete: bool,
    active_action: Option<ActiveAction>,
    action_elapsed: f64,
    retry_elapsed: f64,
    use_completion_baseline: i64,
    chat_baseline: u64,
    action_sent: bool,
    saw_portal_space: bool,
    jump_released: bool,
    jump_aligned: bool,
    jump_saw_airborne: bool,
    jump_charge_elapsed: f64,
    jump_release_elapsed: f64,
    /// Whether the client has been asked for this waypoint's jump.
    jump_asked: bool,
    /// The channel a route jump's direction key is held on by the client,
    /// from the moment the jump is asked for until the route lets it go.
    jump_move_channel: Option<MoveChannel>,
    checkpoint_elapsed: f64,
    /// When the last checkpoint nudge was pressed, on the mover's clock.
    checkpoint_nudge_stamp: f64,
    follow_path: Vec<Position>,
    active_door_object_id: u32,
    action_locks: Option<SharedActionLocks>,
    active_lockpick_object_id: u32,
    door_elapsed: f64,
    door_retry_elapsed: f64,
    portal_origin: Position,
    has_portal_origin: bool,
    recall_needs_position_capture: bool,
    recall_last_position: Position,
    /// A recall cast has gone out and nothing has been made of its outcome
    /// yet; with the completion revision standing when it was sent, this is
    /// how the waypoint learns the cast is over without portal space.
    recall_cast_pending: bool,
    /// The last recall cast completed without an error: the teleport is on
    /// its way and the spell must not be sent again.
    recall_cast_landed: bool,
    recall_cast_revision: i64,
    /// The route's own close-in mover.
    mover: NavigationMover,
    /// A leg of the route is being walked by the client's own pathing.
    client_walking: bool,
    client_walk_sequence: i64,
    client_hand_off_index: i32,
    client_stall_posted: bool,
    client_leg_paused: bool,
    /// The jump's own re-face stamp; it cannot share the mover's, which is
    /// cleared every time the mover stops.
    jump_face_heading_stamp: f64,
    voice: RouteVoice,
    /// Warnings this run has already said once.
    posted_warnings: HashSet<String>,
    /// The macro's one combat-mode gate (the mover holds the same one).
    combat_mode_gate: Option<SharedCombatModeGate>,
    door_status: String,
}

impl std::fmt::Debug for NavigationController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NavigationController")
            .field("index", &self.index)
            .field("reverse", &self.reverse)
            .field("once_complete", &self.once_complete)
            .field("status", &self.voice.status)
            .field("door_status", &self.door_status)
            .finish_non_exhaustive()
    }
}

impl Default for NavigationController {
    fn default() -> Self {
        Self::new()
    }
}

fn f1(value: f64) -> String {
    format_f64(value, "0.0").expect("a supported format")
}

fn bounded_minimum_distance(settings: &NavigationSettings) -> f64 {
    settings
        .minimum_distance_meters
        .clamp(MINIMUM_ARRIVAL_RADIUS_METERS, 50.0)
}

fn bounded_maximum_distance(settings: &NavigationSettings) -> f64 {
    bounded_minimum_distance(settings).max(settings.maximum_distance_meters)
}

#[allow(clippy::cast_possible_truncation)]
fn count_of(settings: &NavigationSettings) -> i32 {
    settings.waypoints.len() as i32
}

/// The three waypoint kinds a round can be anchored at: places to be, not
/// things to do.
fn is_anchor_candidate(kind: RouteWaypointType) -> bool {
    matches!(
        kind,
        RouteWaypointType::Point | RouteWaypointType::Portal | RouteWaypointType::Checkpoint
    )
}

/// Straight-line distance including height: a point directly below on the
/// floor of a dungeon is not the one the character stands at.
fn spatial_distance_meters(from: &Position, to: &Position) -> f64 {
    let east_west = from.east_west - to.east_west;
    let north_south = from.north_south - to.north_south;
    let elevation = from.elevation - to.elevation;
    (east_west * east_west + north_south * north_south + elevation * elevation).sqrt()
        * METRES_PER_MAP_UNIT
}

/// The line the chat window shows for a tell: the host's own wording when
/// it gives one, otherwise rebuilt from the sender and message.
fn shown_tell_line(message: &ChatLine) -> String {
    if !message.display_text.is_empty() {
        message.display_text.clone()
    } else if message.sender_id != 0 {
        format!("{} tells you, \"{}\"", message.sender, message.text)
    } else {
        format!("You tell {}, \"{}\"", message.sender, message.text)
    }
}

impl NavigationController {
    /// A controller at the top of an empty round.
    #[must_use]
    pub fn new() -> Self {
        Self {
            index: 0,
            reverse: false,
            once_complete: false,
            active_action: None,
            action_elapsed: 0.0,
            retry_elapsed: 0.0,
            use_completion_baseline: 0,
            chat_baseline: 0,
            action_sent: false,
            saw_portal_space: false,
            jump_released: false,
            jump_aligned: false,
            jump_saw_airborne: false,
            jump_charge_elapsed: 0.0,
            jump_release_elapsed: 0.0,
            jump_asked: false,
            jump_move_channel: None,
            checkpoint_elapsed: 0.0,
            checkpoint_nudge_stamp: f64::NEG_INFINITY,
            follow_path: Vec::new(),
            active_door_object_id: 0,
            action_locks: None,
            active_lockpick_object_id: 0,
            door_elapsed: 0.0,
            door_retry_elapsed: 0.0,
            portal_origin: Position::default(),
            has_portal_origin: false,
            recall_needs_position_capture: true,
            recall_last_position: Position::default(),
            recall_cast_pending: false,
            recall_cast_landed: false,
            recall_cast_revision: 0,
            mover: NavigationMover::new(),
            client_walking: false,
            client_walk_sequence: 0,
            client_hand_off_index: -1,
            client_stall_posted: false,
            client_leg_paused: false,
            jump_face_heading_stamp: NO_FACE_HEADING_STAMP,
            voice: RouteVoice {
                status: "Navigation disabled.".to_owned(),
                low_waypoint_warning_posted: false,
            },
            posted_warnings: HashSet::new(),
            combat_mode_gate: None,
            door_status: "no door in reach".to_owned(),
        }
    }

    /// Binds the shared cooldown slots. Opening a door holds the item slot
    /// for the use, and the navigation and door slots while the door swings;
    /// an unidentified door in reach holds navigation for the identify.
    pub fn bind_action_locks(&mut self, locks: SharedActionLocks) {
        self.action_locks = Some(locks);
    }

    /// Binds the macro's one combat-mode gate: the mover uses it for the
    /// creep push, a recall waypoint to stand the character in magic mode.
    pub fn bind_combat_mode_gate(&mut self, gate: SharedCombatModeGate) {
        self.mover.bind_combat_mode_gate(Rc::clone(&gate));
        self.combat_mode_gate = Some(gate);
    }

    /// The route's status line.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.voice.status
    }

    /// Why the door turn did what it did. It is deliberately not the
    /// route's status: every sentence here is stable while the situation
    /// is, so the scheduler can suppress a repeat.
    #[must_use]
    pub fn door_status(&self) -> &str {
        &self.door_status
    }

    /// The waypoint the route is heading for.
    #[must_use]
    pub fn current_waypoint_index(&self) -> i32 {
        self.index
    }

    /// Whether the route is walked backwards.
    #[must_use]
    pub fn reversing(&self) -> bool {
        self.reverse
    }

    /// Whether a waypoint action is being worked.
    #[must_use]
    pub fn has_active_action(&self) -> bool {
        self.active_action.is_some()
    }

    /// Whether a leg of the route is being walked by the client's own
    /// pathing.
    #[must_use]
    pub fn is_client_walking(&self) -> bool {
        self.client_walking
    }

    /// The route's mover (its clock and armed state).
    #[must_use]
    pub fn mover(&self) -> &NavigationMover {
        &self.mover
    }

    /// Whether the route has nothing left to walk: no points at all, or a
    /// once route that has run to its end. A once route keeps its points,
    /// so the point count alone cannot answer this.
    #[must_use]
    pub fn has_nothing_left_to_walk(&self, settings: &NavigationSettings) -> bool {
        settings.waypoints.is_empty() || self.once_complete
    }

    /// Whether a once route has already run this waypoint: it consumes by
    /// moving its cursor, so the waypoints behind the cursor are the spent
    /// ones.
    #[must_use]
    pub fn is_waypoint_consumed(&self, settings: &NavigationSettings, index: i32) -> bool {
        settings.mode == RouteMode::Once && index >= 0 && index < self.index
    }

    /// Why the route cannot step back at all, or `None` when it can.
    #[must_use]
    pub fn step_back_block(&self, settings: &NavigationSettings) -> RouteStepBackBlock {
        match settings.mode {
            RouteMode::Target => RouteStepBackBlock::FollowRoute,
            RouteMode::Once => RouteStepBackBlock::OnceRoute,
            _ if settings.waypoints.is_empty() => RouteStepBackBlock::EmptyRoute,
            _ => RouteStepBackBlock::None,
        }
    }

    /// The goal this rule steers at, in the reference's own shape: the
    /// range to it and where it is. It rides the rule's "Running" line.
    pub fn running_detail(&mut self, turn: &mut RouteTurn<'_>) -> String {
        let settings = &*turn.settings;
        if settings.mode == RouteMode::Target
            || self.once_complete
            || self.index < 0
            || self.index >= count_of(settings)
        {
            return String::new();
        }
        let goal = settings.waypoints[self.index as usize].position.clone();
        let distance = turn
            .automation
            .navigation_snapshot()
            .position
            .horizontal_distance_metres(&goal);
        let six = |value: f64| format_f64(value, "0.######").expect("a supported format");
        format!(
            "[targ range {}, targ loc {}, {}, {} ]",
            format_f64(distance, "0.###").expect("a supported format"),
            six(goal.east_west),
            six(goal.north_south),
            six(goal.elevation)
        )
    }

    /// True while the route does something the reference raises its busy
    /// count for, which stops its whole pass: a jump waypoint, from the turn
    /// to its heading until it has landed, and the tenth of a second a
    /// checkpoint nudge holds the forward key. A route that has lost its
    /// turn holds nothing.
    #[must_use]
    pub fn holds_pass(&self) -> bool {
        self.mover.is_armed()
            && (self.active_action.map(|action| action.kind) == Some(RouteWaypointType::Jump)
                || self.mover.now() - self.checkpoint_nudge_stamp < CHECKPOINT_NUDGE_SECONDS)
    }

    /// Moves the route on past the waypoint it is heading for, as if it had
    /// been reached, `count` times; a walk the client is making toward it
    /// ends. A once route that runs out is complete. Returns how many were
    /// skipped.
    pub fn skip_waypoints(&mut self, turn: &mut RouteTurn<'_>, count: i32) -> i32 {
        if count <= 0
            || turn.settings.mode == RouteMode::Target
            || self.has_nothing_left_to_walk(turn.settings)
        {
            return 0;
        }
        let automation = &mut *turn.automation;
        let settings = &mut *turn.settings;
        self.stop_client_walk(automation);
        let mut skipped = 0;
        while skipped < count && !self.once_complete {
            self.advance_waypoint(automation, settings);
            skipped += 1;
        }
        skipped
    }

    /// Moves the cursor back to the waypoint before the one the route is
    /// heading for, `count` times, stopping at the first waypoint in the
    /// order the route is walked; a walk the client is making ends.
    /// Returns how many steps were taken.
    pub fn step_back_waypoints(&mut self, turn: &mut RouteTurn<'_>, count: i32) -> i32 {
        if count <= 0 || self.step_back_block(turn.settings) != RouteStepBackBlock::None {
            return 0;
        }
        let automation = &mut *turn.automation;
        self.stop_client_walk(automation);
        let last = count_of(turn.settings) - 1;
        let mut stepped = 0;
        while stepped < count
            && (if self.reverse {
                self.index < last
            } else {
                self.index > 0
            })
        {
            self.clear_action(automation);
            self.client_hand_off_index = -1;
            self.client_stall_posted = false;
            self.checkpoint_elapsed = 0.0;
            self.index += if self.reverse { 1 } else { -1 };
            stepped += 1;
        }
        stepped
    }

    /// Turns the route round.
    pub fn toggle_reverse(&mut self) {
        self.reverse = !self.reverse;
        self.voice.status = format!(
            "Nav backwards is {}.",
            if self.reverse { "True" } else { "False" }
        );
    }

    /// Forgets the warnings this run has said once.
    pub fn reset_once_per_run_warnings(&mut self) {
        self.voice.low_waypoint_warning_posted = false;
        self.posted_warnings.clear();
    }

    /// The route's warning about a waypoint tight enough to be walked at in
    /// peace mode, said once a run (another mover of the macro shares it).
    pub fn warn_low_waypoint_distance(&mut self, automation: &mut dyn Automation) {
        self.voice.warn_low_waypoint_distance(automation);
    }

    /// Back to the top of the route: for a route that has changed under the
    /// controller (loaded, edited, cleared) and for the end of a session.
    /// Stopping the macro is [`Self::stop_for_macro_stop`].
    pub fn reset(&mut self, turn: &mut RouteTurn<'_>) {
        self.stop_for_macro_stop(turn);
        self.index = 0;
        self.reverse = false;
        self.once_complete = false;
    }

    /// Where the round begins when the macro starts: the nearest point the
    /// character could walk to (a point, a portal or a checkpoint), or the
    /// head of a once route. A follow route has no round. Returns the line
    /// for the macro log's rule-info channel saying where it started.
    pub fn anchor_round_to_start(&mut self, turn: &mut RouteTurn<'_>) -> Option<String> {
        let automation = &mut *turn.automation;
        let settings = &*turn.settings;
        if settings.mode == RouteMode::Target {
            return None;
        }
        if settings.mode == RouteMode::Once {
            self.index = 0;
            self.once_complete = false;
        } else {
            let here = automation.navigation_snapshot().position;
            let mut nearest = 0usize;
            let mut best = f64::MAX;
            for (index, waypoint) in settings.waypoints.iter().enumerate() {
                if !is_anchor_candidate(waypoint.kind) {
                    continue;
                }
                let distance = spatial_distance_meters(&here, &waypoint.position);
                if distance < best {
                    best = distance;
                    nearest = index;
                }
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            {
                self.index = nearest as i32;
            }
        }
        if settings.waypoints.is_empty() {
            return None;
        }
        self.clear_action(automation);
        Some(format!(
            "Route starts at Waypoint {}/{}{}",
            self.index + 1,
            settings.waypoints.len(),
            if settings.mode == RouteMode::Once {
                " (a once-through route starts at its head)"
            } else {
                " (the nearest point to the character)"
            }
        ))
    }

    /// What stopping the macro does to the route: everything in flight is
    /// put down (the movement, the door, the action, the checkpoint clock)
    /// and the round's own position is kept.
    pub fn stop_for_macro_stop(&mut self, turn: &mut RouteTurn<'_>) {
        let automation = &mut *turn.automation;
        self.stop_for_lost_turn_with(automation);
        self.mover.clear_face_heading_stamp();
        self.client_leg_paused = false;
        self.client_hand_off_index = -1;
        self.client_stall_posted = false;
        self.reset_once_per_run_warnings();
        self.checkpoint_elapsed = 0.0;
        self.follow_path.clear();
        self.clear_door();
        self.clear_action(automation);
        self.voice.status = if turn.settings.enabled {
            "Route ready."
        } else {
            "Navigation disabled."
        }
        .to_owned();
    }

    /// The "let go of whatever you are holding and try again" command: it
    /// keeps the follow trail and the once-per-run warnings.
    pub fn clear_action_locks(&mut self, turn: &mut RouteTurn<'_>) {
        let automation = &mut *turn.automation;
        self.mover.stop_for_lost_turn(automation);
        self.mover.clear_face_heading_stamp();
        self.client_leg_paused = false;
        self.client_hand_off_index = -1;
        self.client_stall_posted = false;
        self.checkpoint_elapsed = 0.0;
        self.clear_door();
        self.clear_action(automation);
        self.voice.status = if turn.settings.enabled {
            "Route action locks cleared."
        } else {
            "Navigation disabled."
        }
        .to_owned();
    }

    /// The reference's gate on the route rule's idle-peace fallback: normal
    /// movement is allowed while the goal is further than the creep
    /// distance and the waypoint is not a recall.
    pub fn allows_normal_movement(&mut self, turn: &mut RouteTurn<'_>) -> bool {
        let snapshot = turn.automation.navigation_snapshot();
        let settings = &*turn.settings;
        if !snapshot.is_available
            || settings.mode == RouteMode::Target
            || self.once_complete
            || settings.waypoints.is_empty()
        {
            return true;
        }
        let waypoint = &settings.waypoints[self.index.clamp(0, count_of(settings) - 1) as usize];
        if waypoint.kind == RouteWaypointType::Recall {
            return false;
        }
        snapshot
            .position
            .horizontal_distance_metres(&waypoint.position)
            >= CREEP_DISTANCE_METERS
    }

    /// The route rule's own turn: whether the rule claims the pass, and on
    /// the pass it claims, the mover is armed.
    pub fn claim_from_rule_pass(&mut self, turn: &mut RouteTurn<'_>, can_act: bool) -> bool {
        let pending = self.mover.take_pending_seconds();
        let claimed = self.tick(turn, pending, can_act);
        self.mover.arm(claimed);
        claimed
    }

    /// One frame of the armed mover. The host calls this every frame; the
    /// mover steers no faster than its own interval. `navigation_slots_are_clear`
    /// is whether the cooldown slots that hold navigation off are down on
    /// this frame: a closed slot is the losing-the-turn edge.
    pub fn step_armed_mover(
        &mut self,
        turn: &mut RouteTurn<'_>,
        elapsed_seconds: f64,
        navigation_slots_are_clear: bool,
    ) {
        if !navigation_slots_are_clear {
            self.mover.advance_clock(elapsed_seconds);
            if self.mover.is_armed() {
                self.stop_for_lost_turn(turn);
            }
            return;
        }
        if let Some(due) = self.mover.try_take_mover_frame(elapsed_seconds) {
            let _ = self.tick(turn, due, true);
        }
    }

    /// Moves the navigation clock on by one frame; the one place it moves.
    pub fn advance_clock(&mut self, elapsed_seconds: f64) {
        self.mover.advance_clock(elapsed_seconds);
    }

    /// The navigate rule's teardown on the losing-the-turn edge: the held
    /// movement keys, the client's walk and a jump's held key are let go.
    pub fn stop_for_lost_turn(&mut self, turn: &mut RouteTurn<'_>) {
        self.stop_for_lost_turn_with(turn.automation);
    }

    fn stop_for_lost_turn_with(&mut self, automation: &mut dyn NavigationAutomation) {
        self.stop_client_walk(automation);
        self.stop_route_jump_move(automation);
        self.mover.stop_for_lost_turn(automation);
    }

    /// The Client pathing setting changed: whatever walk it allowed is let
    /// go, and a paused route may try again.
    pub fn client_pathing_changed(&mut self, turn: &mut RouteTurn<'_>) {
        self.stop_client_walk(turn.automation);
        self.client_leg_paused = false;
        self.client_hand_off_index = -1;
        self.client_stall_posted = false;
    }

    /// One turn of the route: whether it claims the pass.
    pub fn tick(&mut self, turn: &mut RouteTurn<'_>, elapsed_seconds: f64, can_act: bool) -> bool {
        let elapsed = if elapsed_seconds.is_finite() {
            elapsed_seconds.max(0.0)
        } else {
            0.0
        };
        let creep = turn.creep.as_ref();
        let automation = &mut *turn.automation;
        let settings = &mut *turn.settings;
        let snapshot = automation.navigation_snapshot();
        if !settings.enabled || !snapshot.is_available {
            self.stop_movement(automation);
            self.voice.status = if settings.enabled {
                "Waiting for the world."
            } else {
                "Navigation disabled."
            }
            .to_owned();
            return false;
        }
        if snapshot.is_portal_space {
            self.stop_movement(automation);
            if matches!(
                self.active_action.map(|action| action.kind),
                Some(
                    RouteWaypointType::Portal
                        | RouteWaypointType::PortalByName
                        | RouteWaypointType::Recall
                )
            ) {
                self.saw_portal_space = true;
            }
            "Waiting for portal space.".clone_into(&mut self.voice.status);
            return self.active_action.is_some();
        }
        if !can_act {
            self.stop_movement(automation);
            "Navigation paused.".clone_into(&mut self.voice.status);
            return false;
        }

        if settings.mode == RouteMode::Target {
            return self.tick_follow(automation, settings, creep, &snapshot);
        }
        if self.once_complete || settings.waypoints.is_empty() {
            self.stop_movement(automation);
            self.voice.status = if self.once_complete {
                "Once route complete."
            } else {
                "Route is empty."
            }
            .to_owned();
            return false;
        }

        if self.client_leg_paused {
            self.stop_movement(automation);
            return false;
        }

        self.index = self.index.clamp(0, count_of(settings) - 1);
        let mut index = self.index as usize;
        let kind = settings.waypoints[index].kind;
        if kind == RouteWaypointType::Point {
            let mut distance = snapshot
                .position
                .horizontal_distance_metres(&settings.waypoints[index].position);
            if distance > bounded_maximum_distance(settings) {
                self.stop_movement(automation);
                self.voice.status =
                    format!("Waypoint is outside NavFarStopRange ({}m).", f1(distance));
                return false;
            }
            if distance <= bounded_minimum_distance(settings) {
                // Arrival is not a stop: the next point is taken on this
                // same frame and the mover decides, from the angle to it,
                // whether the character turns while running or halts to
                // turn. Only a point that ends the route, or one that is not
                // a point, stops.
                self.advance_waypoint(automation, settings);
                if self.once_complete || settings.waypoints.is_empty() {
                    self.stop_movement(automation);
                    "Once route complete.".clone_into(&mut self.voice.status);
                    return true;
                }
                self.index = self.index.clamp(0, count_of(settings) - 1);
                index = self.index as usize;
                if settings.waypoints[index].kind != RouteWaypointType::Point {
                    self.stop_movement(automation);
                    return true;
                }
                distance = snapshot
                    .position
                    .horizontal_distance_metres(&settings.waypoints[index].position);
            }
            self.voice.status = format!(
                "Waypoint {}/{}: {}m",
                self.index + 1,
                settings.waypoints.len(),
                f1(distance)
            );
            return self.steer_leg(automation, settings, creep, &snapshot, index, distance);
        }
        if kind == RouteWaypointType::Checkpoint {
            return self.tick_checkpoint(automation, settings, creep, &snapshot, index, elapsed);
        }

        // The walk stops for an action; a jump under way keeps its key
        // until the action itself lets go of it.
        self.stop_walking(automation);
        self.tick_action(automation, settings, creep, index, elapsed, &snapshot)
    }

    fn tick_follow(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        settings: &NavigationSettings,
        creep: Option<&CreepSettings<'_>>,
        snapshot: &NavigationSnapshot,
    ) -> bool {
        let target = if settings.follow_target_object_id == 0 {
            None
        } else {
            automation.navigation_object(settings.follow_target_object_id)
        };
        let Some(target) = target else {
            self.stop_movement(automation);
            "Follow target unavailable.".clone_into(&mut self.voice.status);
            return false;
        };
        let distance = snapshot
            .position
            .horizontal_distance_metres(&target.position);
        if distance > bounded_maximum_distance(settings) {
            self.stop_movement(automation);
            self.voice.status = format!(
                "Follow target is outside NavFarStopRange ({}m).",
                f1(distance)
            );
            return false;
        }
        if distance <= bounded_minimum_distance(settings) {
            self.stop_movement(automation);
            self.voice.status = format!("Following {}: holding {}m.", target.name, f1(distance));
            return false;
        }
        let destination =
            self.capture_follow_destination(settings, &snapshot.position, &target.position);
        let destination_distance = snapshot.position.horizontal_distance_metres(&destination);
        self.voice.status = format!("Following {}: {}m.", target.name, f1(distance));
        self.steer(
            automation,
            creep,
            &snapshot.position,
            &destination,
            destination_distance,
        )
    }

    fn capture_follow_destination(
        &mut self,
        settings: &NavigationSettings,
        current: &Position,
        target: &Position,
    ) -> Position {
        if !settings.follow_around_corners {
            self.follow_path.clear();
            return target.clone();
        }
        if self.follow_path.last().is_none_or(|last| {
            last.horizontal_distance_metres(target) >= FOLLOW_BREADCRUMB_SPACING_METERS
        }) {
            self.follow_path.push(target.clone());
        }
        for index in (1..self.follow_path.len()).rev() {
            if distance_to_segment_meters(
                current,
                &self.follow_path[index - 1],
                &self.follow_path[index],
            ) < FOLLOW_PATH_ARRIVAL_METERS
                && current.horizontal_distance_metres(&self.follow_path[index])
                    < FOLLOW_PATH_CAPTURE_RANGE_METERS
            {
                self.follow_path.drain(0..index);
                break;
            }
        }
        self.follow_path
            .first()
            .cloned()
            .unwrap_or_else(|| target.clone())
    }

    /// Opening a door, as its own turn, taken before anything that might
    /// want the same tick. It lives here because it shares the mover and
    /// the door settings; [`crate::door_rule::OpenDoorRule`] drives it from
    /// the door's own place in the rule order.
    pub fn tick_door_rule(
        &mut self,
        turn: &mut RouteTurn<'_>,
        elapsed_seconds: f64,
        can_act: bool,
    ) -> bool {
        let elapsed = if elapsed_seconds.is_finite() {
            elapsed_seconds.max(0.0)
        } else {
            0.0
        };
        if !can_act {
            // Losing the pass to a rule ahead of this one is a decline, not
            // a reset: the door being worked has to still be there on the
            // pass this rule wins back.
            "another rule has the turn".clone_into(&mut self.door_status);
            return false;
        }
        let automation = &mut *turn.automation;
        let settings = &*turn.settings;
        let snapshot = automation.navigation_snapshot();
        if !settings.enabled
            || !settings.open_doors
            || !snapshot.is_available
            || snapshot.is_portal_space
        {
            self.door_status = if !settings.enabled || !settings.open_doors {
                "door opening is off"
            } else {
                "waiting for the world"
            }
            .to_owned();
            self.clear_door();
            return false;
        }
        self.tick_door(automation, settings, &snapshot, elapsed)
    }

    fn tick_door(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        settings: &NavigationSettings,
        snapshot: &NavigationSnapshot,
        elapsed_seconds: f64,
    ) -> bool {
        if !settings.open_doors {
            "door opening is off".clone_into(&mut self.door_status);
            self.clear_door();
            return false;
        }

        let objects = automation.navigation_objects();
        if self.active_door_object_id != 0
            && objects
                .iter()
                .any(|tracked| tracked.object_id == self.active_door_object_id && tracked.is_open)
            && let Some(locks) = &self.action_locks
        {
            // The door has swung: the use is over and the doorway is free,
            // so both slots go down early rather than running out their
            // windows.
            let mut locks = locks.borrow_mut();
            locks.release(ActionLockKind::ItemUse);
            locks.release(ActionLockKind::DoorOpening);
        }
        let mut door: Option<&NavigationObject> = None;
        let mut nearest = settings.door_identify_range_meters;
        for candidate in &objects {
            if !candidate.is_door
                || candidate.is_open
                || !self.is_door_on_route(
                    automation,
                    settings,
                    &snapshot.position,
                    &candidate.position,
                )
            {
                continue;
            }
            let distance = snapshot
                .position
                .horizontal_distance_metres(&candidate.position);
            if self.active_door_object_id != 0 && candidate.object_id == self.active_door_object_id
            {
                door = Some(candidate);
                nearest = distance;
                break;
            }
            if self.active_door_object_id == 0 && distance <= nearest {
                door = Some(candidate);
                nearest = distance;
            }
        }

        let Some(door) = door else {
            "no door in reach".clone_into(&mut self.door_status);
            self.clear_door();
            return false;
        };
        if !door.has_lock_state {
            if nearest <= settings.door_identify_range_meters
                && automation.appraisal_awaiting_id() == 0
            {
                // Doors belong to the world, not the owned or container set.
                let _ = automation.identify(door.object_id);
            }
            if nearest <= settings.door_open_range_meters {
                self.stop_movement(automation);
                self.door_status = format!("identifying door: {}", door.name);
                if let Some(locks) = &self.action_locks {
                    // While the door is being identified nobody walks: the
                    // navigation slot is held for half a second and the pass
                    // is declined.
                    locks.borrow_mut().arm(ActionLockKind::Navigation, 0.5);
                    return false;
                }
                return true;
            }
            self.door_status = format!("waiting for the lock state of {}", door.name);
            return false;
        }
        if nearest > settings.door_open_range_meters {
            self.door_status = format!("{} is not in open range", door.name);
            self.clear_door();
            return false;
        }

        if self.active_door_object_id == 0 {
            self.active_door_object_id = door.object_id;
            // A door just taken up is due an attempt straight away.
            self.door_retry_elapsed = USE_RETRY_SECONDS;
            self.active_lockpick_object_id = if door.is_locked {
                self.select_lockpick(automation, settings, door.lock_difficulty)
            } else {
                0
            };
            if door.is_locked && self.active_lockpick_object_id == 0 {
                self.door_status = format!("locked door skipped: {}", door.name);
                self.clear_door();
                return false;
            }
        }

        self.stop_movement(automation);
        self.door_elapsed += elapsed_seconds;
        self.door_retry_elapsed += elapsed_seconds;
        if self.door_elapsed >= DOOR_ACTION_TIMEOUT_SECONDS {
            self.door_status = format!("door timed out: {}", door.name);
            self.clear_door();
            return false;
        }
        if self.door_retry_elapsed >= USE_RETRY_SECONDS {
            // Every door action holds the item slot for its use. A plain use
            // also holds navigation and the door slot for as long as a door
            // takes to swing; a lockpick holds navigation only briefly.
            if let Some(locks) = &self.action_locks {
                let mut locks = locks.borrow_mut();
                locks.arm(ActionLockKind::ItemUse, 0.5);
                if self.active_lockpick_object_id == 0 {
                    locks.arm(ActionLockKind::Navigation, 5.0);
                    locks.arm(ActionLockKind::DoorOpening, 5.0);
                } else {
                    locks.arm(ActionLockKind::Navigation, 1.0);
                }
            }
            let result = if self.active_lockpick_object_id == 0 {
                automation.use_item(door.object_id)
            } else {
                automation.apply_item(self.active_lockpick_object_id, door.object_id)
            };
            self.door_retry_elapsed = 0.0;
            self.door_status = if result.accepted() {
                if self.active_lockpick_object_id == 0 {
                    format!("opening door: {}", door.name)
                } else {
                    format!("picking lock: {}", door.name)
                }
            } else {
                format!("waiting for door: {}", door.name)
            };
        }
        true
    }

    /// Whether a door stands on the leg being walked. Door bounds are not
    /// exposed by the client, so a narrow corridor of a metre either side
    /// of the straight leg (and within a metre of its height) stands in for
    /// them, instead of treating every door in reach as a blocker.
    fn is_door_on_route(
        &self,
        automation: &mut dyn NavigationAutomation,
        settings: &NavigationSettings,
        current: &Position,
        door: &Position,
    ) -> bool {
        let goal = if settings.mode == RouteMode::Target {
            let Some(target) = automation.navigation_object(settings.follow_target_object_id)
            else {
                return false;
            };
            if current.horizontal_distance_metres(&target.position)
                <= bounded_minimum_distance(settings)
            {
                return false;
            }
            self.follow_path.first().cloned().unwrap_or(target.position)
        } else {
            if self.once_complete || self.index < 0 || self.index >= count_of(settings) {
                return false;
            }
            let waypoint = &settings.waypoints[self.index as usize];
            if !matches!(
                waypoint.kind,
                RouteWaypointType::Point
                    | RouteWaypointType::Checkpoint
                    | RouteWaypointType::Portal
                    | RouteWaypointType::PortalByName
            ) {
                return false;
            }
            let mut goal = waypoint.position.clone();
            if matches!(
                waypoint.kind,
                RouteWaypointType::Portal | RouteWaypointType::PortalByName
            ) && let Some(portal) = automation.navigation_object(waypoint.object_id)
            {
                goal = portal.position;
            }
            goal
        };

        let dx = (goal.east_west - current.east_west) * METRES_PER_MAP_UNIT;
        let dy = (goal.north_south - current.north_south) * METRES_PER_MAP_UNIT;
        let px = (door.east_west - current.east_west) * METRES_PER_MAP_UNIT;
        let py = (door.north_south - current.north_south) * METRES_PER_MAP_UNIT;
        let length_squared = dx * dx + dy * dy;
        if length_squared <= 0.0 {
            return false;
        }
        let fraction = (px * dx + py * dy) / length_squared;
        if fraction <= 0.0 || fraction > 1.0 {
            return false;
        }
        let offset_x = px - fraction * dx;
        let offset_y = py - fraction * dy;
        let height = current.elevation + fraction * (goal.elevation - current.elevation);
        const CORRIDOR_METERS: f64 = 1.0;
        offset_x * offset_x + offset_y * offset_y <= CORRIDOR_METERS * CORRIDOR_METERS
            && (door.elevation - height).abs() * METRES_PER_MAP_UNIT <= CORRIDOR_METERS
    }

    /// The best lockpick for a lock: none unless the lockpick skill clears
    /// the lock's difficulty by the profile's margin, else the carried pick
    /// with the highest modifier.
    fn select_lockpick(
        &self,
        automation: &mut dyn NavigationAutomation,
        settings: &NavigationSettings,
        difficulty: i32,
    ) -> u32 {
        let Some(skill) = automation.try_get_skill(LOCKPICK_SKILL_ID) else {
            return 0;
        };
        let needed = 0.max(difficulty.wrapping_add(settings.door_lockpick_excess_threshold));
        if i64::from(skill.current) < i64::from(needed) {
            return 0;
        }
        let mut selected = 0;
        let mut best_modifier = f64::MIN;
        for item in automation.owned_items() {
            if item.public_flags & LOCKPICK_PUBLIC_FLAG == 0 {
                continue;
            }
            let modifier = automation
                .item_properties(item.object_id)
                .and_then(|properties| {
                    properties
                        .floats
                        .iter()
                        .find(|(key, _)| *key == LOCKPICK_MODIFIER_PROPERTY)
                        .map(|(_, value)| *value)
                })
                .unwrap_or(0.0);
            if modifier <= best_modifier {
                continue;
            }
            best_modifier = modifier;
            selected = item.object_id;
        }
        selected
    }

    fn clear_door(&mut self) {
        self.active_door_object_id = 0;
        self.active_lockpick_object_id = 0;
        self.door_elapsed = 0.0;
        self.door_retry_elapsed = USE_RETRY_SECONDS;
    }

    fn tick_checkpoint(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        settings: &mut NavigationSettings,
        creep: Option<&CreepSettings<'_>>,
        snapshot: &NavigationSnapshot,
        index: usize,
        elapsed_seconds: f64,
    ) -> bool {
        let goal = settings.waypoints[index].position.clone();
        let live_distance = snapshot.position.horizontal_distance_metres(&goal);
        if live_distance > bounded_maximum_distance(settings) {
            self.stop_movement(automation);
            self.checkpoint_elapsed = 0.0;
            self.voice.status = format!(
                "Checkpoint is outside NavFarStopRange ({}m).",
                f1(live_distance)
            );
            return false;
        }
        if live_distance > bounded_minimum_distance(settings) {
            self.checkpoint_elapsed = 0.0;
            self.voice.status = format!(
                "Checkpoint {}/{}: {}m",
                self.index + 1,
                settings.waypoints.len(),
                f1(live_distance)
            );
            return self.steer_leg(automation, settings, creep, snapshot, index, live_distance);
        }

        self.stop_movement(automation);
        let confirmed = if snapshot.confirmed_position_revision == 0 {
            snapshot.position.clone()
        } else {
            snapshot.confirmed_position.clone()
        };
        let confirmed_distance = confirmed.horizontal_distance_metres(&goal);
        if confirmed_distance <= bounded_minimum_distance(settings) {
            self.checkpoint_elapsed = 0.0;
            self.advance_waypoint(automation, settings);
            return true;
        }

        self.checkpoint_elapsed += elapsed_seconds;
        self.voice.status = format!(
            "Checkpoint: waiting for server ({}m).",
            f1(confirmed_distance)
        );
        if self.checkpoint_elapsed >= CHECKPOINT_RETRY_SECONDS {
            self.checkpoint_elapsed = 0.0;
            let held = automation.set_movement_intent(MovementIntent {
                forward: true,
                run: false,
                ..MovementIntent::default()
            }) == NavigationStatus::Accepted;
            self.mover.note_movement_intent(held);
            "Checkpoint: nudging for server confirmation.".clone_into(&mut self.voice.status);
            self.checkpoint_nudge_stamp = self.mover.now();
        }
        true
    }

    /// Steers the route's mover at a goal, and takes what the mover had to
    /// say: the tight-goal warning (once a run) and its status line.
    fn steer(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        creep: Option<&CreepSettings<'_>>,
        current: &Position,
        target: &Position,
        distance: f64,
    ) -> bool {
        let claimed = self
            .mover
            .steer(automation, creep, current, target, distance);
        if self.mover.take_low_stop_distance_warning() {
            self.voice.warn_low_waypoint_distance(automation);
        }
        if let Some(status) = self.mover.take_status() {
            self.voice.status = status;
        }
        claimed
    }

    /// Steers one leg of the route: the reference way, straight at the
    /// point with held keys, unless the profile hands every leg to the
    /// client, or the straight walk is stuck and the profile allows one
    /// hand-off per visit to a waypoint. While the client walks, the rule
    /// keeps its turn and only watches the report.
    fn steer_leg(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        settings: &mut NavigationSettings,
        creep: Option<&CreepSettings<'_>>,
        snapshot: &NavigationSnapshot,
        index: usize,
        distance: f64,
    ) -> bool {
        if self.client_walking {
            return self.watch_client_walk(automation, settings, distance);
        }
        if settings.client_pathing == ClientPathing::Always {
            return self.ask_client_walk(automation, settings, index, distance);
        }

        let goal = settings.waypoints[index].position.clone();
        let claimed = self.steer(automation, creep, &snapshot.position, &goal, distance);
        if !self.mover.is_stuck() {
            return claimed;
        }
        self.mover.reset_stuck_clock();
        if settings.client_pathing == ClientPathing::WhenStuck
            && self.client_hand_off_index != self.index
        {
            self.client_hand_off_index = self.index;
            self.mover.stop_movement(automation);
            return self.ask_client_walk(automation, settings, index, distance);
        }
        if !self.client_stall_posted {
            self.client_stall_posted = true;
            client::post(
                automation,
                &format!(
                    "{} The route has not covered ground for {} seconds on the way to waypoint {}.",
                    client::CHAT_TAG,
                    format_f64(STUCK_SECONDS, "0").expect("a supported format"),
                    self.index + 1
                ),
            );
        }
        claimed
    }

    #[allow(clippy::cast_possible_truncation)]
    fn ask_client_walk(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        settings: &NavigationSettings,
        index: usize,
        distance: f64,
    ) -> bool {
        let asked = automation.go_to_place(
            settings.waypoints[index].position.clone(),
            bounded_minimum_distance(settings) as f32,
        );
        if asked != NavigationStatus::Accepted {
            let why = match asked {
                NavigationStatus::Unavailable => "this client cannot path",
                NavigationStatus::Held => "another plugin or the player is driving the character",
                _ => "the client refused the walk",
            };
            if settings.client_pathing == ClientPathing::Always {
                self.pause_route(
                    automation,
                    &format!("Waypoint {} could not be walked ({why})", self.index + 1),
                );
                return false;
            }
            self.voice.status = format!(
                "Waypoint {}/{}: {}m; {why}, walking on",
                self.index + 1,
                settings.waypoints.len(),
                f1(distance)
            );
            return true;
        }
        self.client_walking = true;
        self.client_walk_sequence = automation.go_to_report().sequence;
        self.voice.status = self.client_walk_status(settings, distance);
        true
    }

    fn watch_client_walk(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        settings: &mut NavigationSettings,
        distance: f64,
    ) -> bool {
        let report = automation.go_to_report();
        if report.sequence != self.client_walk_sequence {
            // A newer walk replaced ours: whoever asked for it has the
            // character, and the keys come back on the next steered frame.
            self.client_walking = false;
            return true;
        }
        match report.state {
            GoToState::Planning | GoToState::Walking | GoToState::Waiting => {
                self.voice.status = self.client_walk_status(settings, distance);
                true
            }
            GoToState::Arrived | GoToState::ArrivedWithoutSight => {
                self.client_walking = false;
                self.advance_waypoint(automation, settings);
                true
            }
            GoToState::Stopped => {
                self.client_walking = false;
                true
            }
            state => {
                self.client_walking = false;
                let reason = match &report.reason {
                    Some(reason) if !casing::is_white_space_only(reason) => reason.clone(),
                    _ => format!("{state:?}"),
                };
                if settings.client_pathing == ClientPathing::Always {
                    self.pause_route(
                        automation,
                        &format!("Waypoint {} could not be walked ({reason})", self.index + 1),
                    );
                    return false;
                }
                self.voice.status = format!(
                    "Waypoint {}/{}: {}m; the client's walk ended ({reason}), walking on",
                    self.index + 1,
                    settings.waypoints.len(),
                    f1(distance)
                );
                true
            }
        }
    }

    fn client_walk_status(&self, settings: &NavigationSettings, distance: f64) -> String {
        format!(
            "Waypoint {}/{}: {}m, walked by the client",
            self.index + 1,
            settings.waypoints.len(),
            f1(distance)
        )
    }

    /// With every leg on the client, a leg it cannot walk stops the route
    /// where it stands and says so once; the route goes on when it is reset
    /// or the setting changes.
    fn pause_route(&mut self, automation: &mut dyn NavigationAutomation, why: &str) {
        self.client_leg_paused = true;
        self.voice.status =
            format!("{why}; the route is paused. Reset the route or change Client pathing.");
        let line = format!("{} {}", client::CHAT_TAG, self.voice.status);
        client::post(automation, &line);
    }

    /// Ends the walk the route asked the client for, if it is still under
    /// way.
    fn stop_client_walk(&mut self, automation: &mut dyn NavigationAutomation) {
        if !self.client_walking {
            return;
        }
        self.client_walking = false;
        let report = automation.go_to_report();
        if report.sequence == self.client_walk_sequence
            && matches!(
                report.state,
                GoToState::Planning | GoToState::Walking | GoToState::Waiting
            )
        {
            let _ = automation.stop_go_to();
        }
    }

    fn tick_action(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        settings: &mut NavigationSettings,
        creep: Option<&CreepSettings<'_>>,
        index: usize,
        elapsed_seconds: f64,
        snapshot: &NavigationSnapshot,
    ) -> bool {
        let kind = settings.waypoints[index].kind;
        if self.active_action.map(|action| action.index) != Some(index) {
            self.clear_action(automation);
            self.active_action = Some(ActiveAction { index, kind });
            self.use_completion_baseline = automation.last_item_completion().revision;
            self.chat_baseline = automation
                .chat_messages(0)
                .iter()
                .map(|message| message.sequence)
                .max()
                .unwrap_or(0);
        }
        self.action_elapsed += elapsed_seconds;
        self.retry_elapsed += elapsed_seconds;

        match kind {
            RouteWaypointType::Pause => {
                let duration = settings.waypoints[index].duration_milliseconds;
                self.voice.status = format!(
                    "Pause: {}s",
                    f1((f64::from(duration) / 1000.0 - self.action_elapsed).max(0.0))
                );
                if self.action_elapsed * 1000.0 >= f64::from(duration.max(0)) {
                    self.complete_action(automation, settings);
                }
                true
            }
            RouteWaypointType::ChatCommand => {
                let text = settings.waypoints[index].text.clone();
                self.voice.status = format!("Chat command: {text}");
                if self.action_elapsed < CHAT_INITIAL_DELAY_SECONDS {
                    return true;
                }
                if !self.action_sent {
                    self.action_sent = automation.submit_chat(&text);
                    if !self.action_sent {
                        "Chat command was refused.".clone_into(&mut self.voice.status);
                        return true;
                    }
                }
                self.complete_action(automation, settings);
                true
            }
            RouteWaypointType::Recall => {
                self.tick_recall(automation, settings, creep, index, snapshot)
            }
            RouteWaypointType::OpenVendor => self.tick_open_vendor(automation, settings, index),
            RouteWaypointType::Portal
            | RouteWaypointType::PortalByName
            | RouteWaypointType::UseNpc => {
                self.tick_use(automation, settings, creep, index, snapshot)
            }
            RouteWaypointType::Jump => {
                self.tick_jump(automation, settings, index, elapsed_seconds, snapshot)
            }
            _ => {
                self.complete_action(automation, settings);
                true
            }
        }
    }

    /// A vendor waypoint opens the vendor and holds until its window is up.
    /// Using something out of reach only starts a walk to it, so the hold
    /// is what lets that walk finish. The use is re-sent every two seconds
    /// until the window opens, and after thirty the route says so once and
    /// carries on. A window already open finishes the waypoint at once.
    fn tick_open_vendor(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        settings: &mut NavigationSettings,
        index: usize,
    ) -> bool {
        let object_id = settings.waypoints[index].object_id;
        let object_name = settings.waypoints[index].object_name.clone();
        if object_id != 0 && Self::vendor_window_is_open_for(automation, object_id) {
            self.voice.status = format!("Vendor window is open: {object_name}.");
            self.complete_action(automation, settings);
            return true;
        }

        let vendor = if object_id == 0 {
            None
        } else {
            automation.navigation_object(object_id)
        };
        let Some(vendor) = vendor else {
            self.warn_once(
                automation,
                &format!(
                    "OpenVendor waypoint action ignored, vendor {object_id} ({object_name}) not found."
                ),
            );
            self.voice.status = format!("Vendor not found: {object_name}.");
            return self.hold_vendor_waypoint(automation, settings);
        };

        if self.action_elapsed >= PORTAL_TIMEOUT_SECONDS {
            self.warn_once(
                automation,
                &format!("Vendor '{object_name}' did not open; continuing route."),
            );
            self.voice.status = format!("Vendor did not open: {object_name}.");
            self.complete_action(automation, settings);
            return true;
        }

        if !self.action_sent || self.retry_elapsed >= USE_RETRY_SECONDS {
            let result = automation.use_item(object_id);
            // Sent is sent whether or not the client took it: a refusal waits
            // the same two seconds as a use that has not landed.
            self.action_sent = true;
            self.retry_elapsed = 0.0;
            self.voice.status = if result.accepted() {
                format!("Opening vendor: {}.", vendor.name)
            } else {
                format!("Waiting to use vendor: {}.", vendor.name)
            };
        }
        true
    }

    /// Whether the vendor window that is up belongs to this object: the
    /// item side's open vendor or the trade side's, either counts.
    fn vendor_window_is_open_for(
        automation: &mut dyn NavigationAutomation,
        object_id: u32,
    ) -> bool {
        automation.active_vendor_id() == object_id || automation.open_vendor_id() == object_id
    }

    /// The vendor waypoint's hold, with a ceiling on it.
    fn hold_vendor_waypoint(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        settings: &mut NavigationSettings,
    ) -> bool {
        if self.action_elapsed < PORTAL_TIMEOUT_SECONDS {
            return true;
        }
        self.complete_action(automation, settings);
        true
    }

    fn warn_once(&mut self, automation: &mut dyn NavigationAutomation, text: &str) {
        if !self.posted_warnings.insert(text.to_owned()) {
            return;
        }
        client::post(automation, &format!("{} {text}", client::CHAT_TAG));
    }

    fn tick_use(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        settings: &mut NavigationSettings,
        creep: Option<&CreepSettings<'_>>,
        index: usize,
        snapshot: &NavigationSnapshot,
    ) -> bool {
        // Coming out of portal space decides the waypoint before anything
        // looks at the portal again: the portal used stays in the object
        // table for a while after the teleport.
        if snapshot.is_portal_space {
            self.saw_portal_space = true;
        }
        let kind = settings.waypoints[index].kind;
        if self.saw_portal_space && !snapshot.is_portal_space {
            if kind == RouteWaypointType::PortalByName
                && self.has_portal_origin
                && snapshot
                    .position
                    .horizontal_distance_metres(&self.portal_origin)
                    <= PORTAL_EXIT_DISTANCE_METERS
            {
                self.saw_portal_space = false;
                self.action_sent = false;
                self.retry_elapsed = USE_RETRY_SECONDS;
                "Portal exit stayed near its origin; retrying.".clone_into(&mut self.voice.status);
                return true;
            }
            self.complete_action(automation, settings);
            return true;
        }

        if matches!(
            kind,
            RouteWaypointType::PortalByName | RouteWaypointType::UseNpc
        ) {
            let waypoint = &settings.waypoints[index];
            let current_still_exists = waypoint.object_id != 0
                && automation
                    .navigation_object(waypoint.object_id)
                    .is_some_and(|current| {
                        casing::is_white_space_only(&waypoint.object_name)
                            || ordinal::equals_ignore_case(&current.name, &waypoint.object_name)
                    });
            if !current_still_exists {
                // Search near the object's own recorded position, not where
                // the character stood when the waypoint was written: the
                // object can be far from the approach point.
                let reacquired = automation.find_object(
                    &waypoint.object_name,
                    waypoint.reference_position.clone(),
                    OBJECT_REACQUIRE_RADIUS_METERS,
                );
                let Some(reacquired) = reacquired else {
                    self.voice.status = format!("Finding {}.", waypoint.object_name);
                    if self.action_elapsed >= PORTAL_TIMEOUT_SECONDS {
                        self.complete_action(automation, settings);
                    }
                    return true;
                };
                settings.waypoints[index].object_id = reacquired.object_id;
                self.action_sent = false;
                self.retry_elapsed = USE_RETRY_SECONDS;
            }
        }
        let object_id = settings.waypoints[index].object_id;
        let object_name = settings.waypoints[index].object_name.clone();
        if object_id == 0 {
            "Waypoint object is unavailable; continuing.".clone_into(&mut self.voice.status);
            self.complete_action(automation, settings);
            return true;
        }

        if matches!(
            kind,
            RouteWaypointType::Portal | RouteWaypointType::PortalByName
        ) && let Some(portal) = automation.navigation_object(object_id)
        {
            let distance = snapshot
                .position
                .horizontal_distance_metres(&portal.position);
            if distance > settings.portal_use_distance_meters.clamp(0.5, 50.0) {
                self.voice.status = format!("Approaching {object_name} ({}m).", f1(distance));
                return self.steer(
                    automation,
                    creep,
                    &snapshot.position,
                    &portal.position,
                    distance,
                );
            }
        }

        if kind == RouteWaypointType::UseNpc && self.has_npc_response(automation, &object_name) {
            self.complete_action(automation, settings);
            return true;
        }

        let completion = automation.last_item_completion();
        if self.action_sent
            && completion.revision > self.use_completion_baseline
            && completion.source_id == object_id
        {
            self.use_completion_baseline = completion.revision;
            if !(completion.revision != 0 && completion.weenie_error == 0) {
                self.action_sent = false;
                self.retry_elapsed = USE_RETRY_SECONDS;
            }
        }

        if self.action_elapsed >= PORTAL_TIMEOUT_SECONDS {
            self.voice.status = format!("Use timed out: {object_name}.");
            self.complete_action(automation, settings);
            return true;
        }
        if !self.action_sent || self.retry_elapsed >= USE_RETRY_SECONDS {
            let result = automation.use_item(object_id);
            self.action_sent |= result.accepted();
            if result.accepted() && !self.has_portal_origin {
                self.portal_origin = snapshot.position.clone();
                self.has_portal_origin = true;
            }
            self.retry_elapsed = 0.0;
            self.voice.status = if result.accepted() {
                format!("Using {object_name}.")
            } else {
                format!("Waiting to use {object_name}.")
            };
        }
        true
    }

    /// The NPC answered. Only two lines count, each only with its own
    /// log-text type: a tell that opens "<name> tells you, " and a plain
    /// line that opens "<name> gives you". The test is over the line the
    /// chat window shows; a message's kind cannot stand in for its log-text
    /// type.
    fn has_npc_response(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        npc_name: &str,
    ) -> bool {
        let messages = automation.chat_messages(self.chat_baseline);
        for message in &messages {
            self.chat_baseline = self.chat_baseline.max(message.sequence);
            let answered = match message.log_text_type {
                NPC_TELL_LOG_TEXT_TYPE => {
                    shown_tell_line(message).starts_with(&format!("{npc_name} tells you, "))
                }
                NPC_GIVE_LOG_TEXT_TYPE => {
                    message.text.starts_with(&format!("{npc_name} gives you"))
                }
                _ => false,
            };
            if answered {
                return true;
            }
        }
        false
    }

    /// The live spell catalogue's answer for a recall written by a name the
    /// recall table does not know.
    fn catalog_lookup(automation: &mut dyn NavigationAutomation, name: &str) -> CatalogLookup {
        let found = automation
            .spells()
            .try_find_by_name(name, false)
            .map(|spell| spell.spell_id);
        let mut known = Vec::new();
        for list in [SpellList::SelfBuffs, SpellList::Attacks, SpellList::Combat] {
            known.extend(
                automation
                    .known_spell_list(list)
                    .into_iter()
                    .map(|spell| (spell.spell_id, spell.name)),
            );
        }
        CatalogLookup { found, known }
    }

    fn tick_recall(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        settings: &mut NavigationSettings,
        creep: Option<&CreepSettings<'_>>,
        index: usize,
        snapshot: &NavigationSnapshot,
    ) -> bool {
        {
            let waypoint = &mut settings.waypoints[index];
            if waypoint.recall_spell_id == 0 && waypoint.recall != RouteRecallKind::Marketplace {
                // The route may have been read before the character's spell
                // tables were available, so the name gets one more pass
                // against the live catalogue before it is given up on.
                let name = waypoint.recall_spell_name.trim().to_owned();
                if name.is_empty() || RouteRecallKind::from_name(&name).is_some() {
                    waypoint.resolve_recall_spell(None);
                } else {
                    let lookup = Self::catalog_lookup(automation, &name);
                    waypoint.resolve_recall_spell(Some(&lookup));
                }
            }
        }
        let waypoint = settings.waypoints[index].clone();
        if waypoint.recall_spell_id == 0 && waypoint.recall != RouteRecallKind::Marketplace {
            self.voice.status = if casing::is_white_space_only(&waypoint.recall_spell_name) {
                "Recall waypoint has no spell; skipping.".to_owned()
            } else {
                format!(
                    "Recall spell '{}' not found; skipping waypoint.",
                    waypoint.recall_spell_name
                )
            };
            // The status line lives on a panel the player may not have
            // open; a skipped waypoint is silent without this.
            let status = self.voice.status.clone();
            self.warn_once(automation, &status);
            self.complete_action(automation, settings);
            return true;
        }

        // Arrival is read before the standing-still check: the teleport is
        // itself a position jump.
        if snapshot.is_portal_space {
            self.saw_portal_space = true;
        }
        if self.saw_portal_space && !snapshot.is_portal_space {
            if !self.has_portal_origin
                || snapshot
                    .position
                    .horizontal_distance_metres(&self.portal_origin)
                    > RECALL_EXIT_DISTANCE_METERS
            {
                self.complete_action(automation, settings);
                return true;
            }
            self.saw_portal_space = false;
            self.action_sent = false;
            self.recall_cast_pending = false;
            self.recall_cast_landed = false;
            self.retry_elapsed = USE_RETRY_SECONDS;
        }

        if !self.is_standing_still_for_recall(snapshot) {
            "Recall: waiting to come to a stop.".clone_into(&mut self.voice.status);
            return true;
        }

        if self.action_elapsed >= PORTAL_TIMEOUT_SECONDS {
            "Recall timed out; continuing route.".clone_into(&mut self.voice.status);
            self.complete_action(automation, settings);
            return true;
        }

        let label = waypoint.recall_label();
        if automation.is_casting() {
            // The cast is already doing the waypoint's job, and a second
            // request on top of a pending one is turned away, so the retry
            // clock is wound back while the cast is in the air.
            self.retry_elapsed = 0.0;
            self.voice.status = format!("Recall: {label}.");
            return true;
        }
        if self.recall_cast_pending
            && automation.last_cast_completion().revision != self.recall_cast_revision
        {
            // The cast finished. Without an error the teleport is on its
            // way; with one (a fizzle or a refusal) the next attempt goes out
            // on this tick.
            self.recall_cast_pending = false;
            if automation.last_cast_completion().is_success() {
                self.recall_cast_landed = true;
            } else {
                self.retry_elapsed = USE_RETRY_SECONDS;
            }
        }

        // A recall is a spell, and the server drops any cast made outside
        // magic mode, so the character is stood in magic mode first.
        if waypoint.recall_spell_id != 0 && !self.is_ready_to_cast_recall(automation, creep) {
            "Recall: switching to magic mode.".clone_into(&mut self.voice.status);
            return true;
        }

        if !self.recall_cast_landed
            && (!self.action_sent || self.retry_elapsed >= USE_RETRY_SECONDS)
        {
            let issue_revision = automation.last_cast_completion().revision;
            let accepted = self.submit_recall(automation, &waypoint);
            if accepted && waypoint.recall_spell_id != 0 {
                self.recall_cast_pending = true;
                self.recall_cast_revision = issue_revision;
            }
            self.action_sent |= accepted;
            if accepted && !self.has_portal_origin {
                self.portal_origin = snapshot.position.clone();
                self.has_portal_origin = true;
            }
            self.retry_elapsed = 0.0;
        }
        self.voice.status = format!("Recall: {label}.");
        true
    }

    /// A recall is only cast from a standstill: a position comparison
    /// against the last tick, not a movement flag.
    fn is_standing_still_for_recall(&mut self, snapshot: &NavigationSnapshot) -> bool {
        if self.recall_needs_position_capture {
            self.recall_needs_position_capture = false;
            self.recall_last_position = snapshot.position.clone();
            return true;
        }
        if snapshot
            .position
            .horizontal_distance_metres(&self.recall_last_position)
            > RECALL_STATIONARY_TOLERANCE_METERS
        {
            self.recall_needs_position_capture = true;
            return false;
        }
        self.recall_last_position = snapshot.position.clone();
        true
    }

    /// Whether the character stands in magic mode, asking the macro's gate
    /// for it on every tick until it does. With no gate bound there is
    /// nothing to ask and the cast goes out.
    fn is_ready_to_cast_recall(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        creep: Option<&CreepSettings<'_>>,
    ) -> bool {
        let (Some(gate), Some(settings)) = (self.combat_mode_gate.clone(), creep) else {
            return true;
        };
        let mut turn = GateTurn {
            automation,
            combat: settings.combat,
            vitals: settings.vitals,
        };
        let prepared =
            gate.borrow_mut()
                .try_prepare(&mut turn, Prepare::mode(CombatMode::Magic), None);
        prepared && automation.combat_snapshot().mode == CombatMode::Magic
    }

    fn submit_recall(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        waypoint: &RouteWaypoint,
    ) -> bool {
        if waypoint.recall_spell_id == 0 {
            return automation.submit_chat("/marketplace");
        }
        let request = automation.request_cast(waypoint.recall_spell_id, None);
        if request == CastResult::Sent {
            return true;
        }
        // "Unavailable" while a cast is in the air is the recall this
        // waypoint itself started; saying so would report a working recall
        // as failed. Every other refusal speaks.
        if request == CastResult::Unavailable && automation.is_casting() {
            return false;
        }
        self.warn_once(
            automation,
            &format!("Recall '{}' refused: {request:?}.", waypoint.recall_label()),
        );
        false
    }

    #[allow(clippy::cast_possible_truncation)]
    fn tick_jump(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        settings: &mut NavigationSettings,
        index: usize,
        elapsed_seconds: f64,
        snapshot: &NavigationSnapshot,
    ) -> bool {
        if !self.jump_released {
            let waypoint = &settings.waypoints[index];
            let heading = waypoint.jump_heading_degrees as f32;
            if !self.jump_aligned {
                let delta = signed_heading_delta(snapshot.position.heading_degrees, heading);
                // A jump is aimed far more tightly than a walk, and waits
                // longer between attempts: a few degrees of error is a missed
                // ledge.
                if delta.abs() >= JUMP_HEADING_TOLERANCE_DEGREES {
                    let cleared = automation.clear_movement_intent() == NavigationStatus::Accepted;
                    let held = cleared && self.mover.has_movement_intent();
                    self.mover.note_movement_intent(held);
                    if self.mover.now() - self.jump_face_heading_stamp
                        > JUMP_FACE_HEADING_REISSUE_SECONDS
                    {
                        self.jump_face_heading_stamp = self.mover.now();
                        let _ = automation.face_heading(heading);
                    }
                    self.voice.status = format!(
                        "Aligning jump: {}d.",
                        format_f32(delta.abs(), "0.0").expect("a supported format")
                    );
                    return true;
                }
                self.jump_face_heading_stamp = NO_FACE_HEADING_STAMP;
                self.jump_aligned = true;
            }

            let effective_charge_milliseconds = waypoint
                .jump_charge_milliseconds
                .clamp(0, JUMP_CHARGE_CEILING_MILLISECONDS);
            // The client charges and lets go by itself, at exactly the power
            // the charge time comes to.
            let power = jump_power::from_milliseconds(effective_charge_milliseconds);
            if !self.jump_asked {
                let direction = waypoint.jump_direction;
                let hold_shift = waypoint.jump_hold_shift;
                self.begin_route_jump(automation, direction, hold_shift, power);
            }
            self.jump_charge_elapsed += elapsed_seconds;
            if self.jump_charge_elapsed < jump_power::charge_seconds(effective_charge_milliseconds)
            {
                self.voice.status = format!("Charging jump: {effective_charge_milliseconds}ms.");
                return true;
            }
            self.jump_released = true;
            self.jump_release_elapsed = 0.0;
            "Jump released.".clone_into(&mut self.voice.status);
            return true;
        }
        self.jump_release_elapsed += elapsed_seconds;
        self.jump_saw_airborne |= snapshot.is_airborne;
        if snapshot.is_airborne
            || (!self.jump_saw_airborne
                && self.jump_release_elapsed < JUMP_COMPLETION_TIMEOUT_SECONDS)
            || (self.jump_saw_airborne && self.jump_release_elapsed < JUMP_LAUNCH_GRACE_SECONDS)
        {
            return true;
        }
        self.complete_action(automation, settings);
        true
    }

    /// Asks the client for a route jump: the walk's held keys are let go,
    /// the waypoint's direction is held as a client-driven move at its pace,
    /// and the jump charges to `power` and leaves. The key stays held until
    /// the route lets go of the jump.
    fn begin_route_jump(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        direction: RouteJumpDirection,
        hold_shift: bool,
        power: f32,
    ) {
        self.jump_asked = true;
        let _ = automation.clear_movement_intent();
        self.mover.note_movement_intent(false);
        let direction = match direction {
            RouteJumpDirection::StrafeLeft => MoveDirection::StrafeLeft,
            RouteJumpDirection::StrafeRight => MoveDirection::StrafeRight,
            RouteJumpDirection::Backward => MoveDirection::Backward,
            RouteJumpDirection::Forward => MoveDirection::Forward,
        };
        self.jump_move_channel = Some(MoveReport::channel_of(direction));
        let _ = automation.move_by(
            direction,
            if hold_shift {
                MovePace::Walk
            } else {
                MovePace::Run
            },
            0.0,
            MoveUnit::MetersOrDegrees,
        );
        let _ = automation.jump(power);
    }

    /// Lets go of the key a route jump held, if one is held.
    fn stop_route_jump_move(&mut self, automation: &mut dyn NavigationAutomation) {
        if let Some(channel) = self.jump_move_channel.take() {
            let _ = automation.stop_moving_channel(channel);
        }
    }

    fn complete_action(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        settings: &mut NavigationSettings,
    ) {
        self.clear_action(automation);
        self.advance_waypoint(automation, settings);
    }

    fn advance_waypoint(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        settings: &NavigationSettings,
    ) {
        self.clear_action(automation);
        self.client_hand_off_index = -1;
        self.client_stall_posted = false;
        self.checkpoint_elapsed = 0.0;
        let count = count_of(settings);
        if count == 0 {
            return;
        }
        match settings.mode {
            RouteMode::Circular => {
                self.index = if self.reverse {
                    (self.index - 1 + count) % count
                } else {
                    (self.index + 1) % count
                };
            }
            RouteMode::Linear => {
                if self.reverse {
                    self.index -= 1;
                    if self.index < 0 {
                        self.index = 0;
                        self.reverse = false;
                    }
                } else {
                    self.index += 1;
                    if self.index >= count {
                        self.index = 0.max(count - 1);
                        self.reverse = true;
                    }
                }
            }
            RouteMode::Once => {
                // A once route consumes its points in memory only: the
                // cursor walks forward and the list is left whole, so a run
                // to the end never writes an empty route over the file.
                self.index += 1;
                self.once_complete = self.index >= count;
            }
            RouteMode::Target => {}
        }
    }

    fn clear_action(&mut self, automation: &mut dyn NavigationAutomation) {
        self.stop_route_jump_move(automation);
        self.jump_asked = false;
        self.active_action = None;
        self.action_elapsed = 0.0;
        self.retry_elapsed = 0.0;
        self.use_completion_baseline = 0;
        self.chat_baseline = 0;
        self.action_sent = false;
        self.saw_portal_space = false;
        self.jump_released = false;
        self.jump_aligned = false;
        self.jump_saw_airborne = false;
        self.jump_charge_elapsed = 0.0;
        self.jump_release_elapsed = 0.0;
        self.portal_origin = Position::default();
        self.has_portal_origin = false;
        self.recall_needs_position_capture = true;
        self.recall_last_position = Position::default();
        self.recall_cast_pending = false;
        self.recall_cast_landed = false;
        self.recall_cast_revision = 0;
        self.jump_face_heading_stamp = NO_FACE_HEADING_STAMP;
    }

    fn stop_movement(&mut self, automation: &mut dyn NavigationAutomation) {
        self.stop_route_jump_move(automation);
        self.stop_walking(automation);
    }

    /// Stops the walk, the client's and the mover's, and nothing else.
    fn stop_walking(&mut self, automation: &mut dyn NavigationAutomation) {
        self.stop_client_walk(automation);
        self.mover.stop_movement(automation);
    }
}

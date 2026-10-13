//! The close-in mover (`NavigationMover`): turns "the goal is over there"
//! into held movement keys, paced on its own interval once a rule has armed
//! it. Each walking rule owns one, so the rule that loses a pass tears down
//! only its own. With the heading helpers the route shares
//! (`NavigationController.DesiredHeading` and friends).

use ac_plugin_abi::combat::CombatMode;
use ac_plugin_abi::navigation::{MovementIntent, NavigationStatus};
use ac_plugin_abi::world::Position;
use mosstank_automation::Automation;
use mosstank_settings::model::{CombatSettings, VitalSettings};

use crate::gate::{GateTurn, Prepare, SharedCombatModeGate};

/// Degrees inside which the mover walks straight on.
pub const HEADING_TOLERANCE_DEGREES: f32 = 4.0;
/// How often the armed mover may steer, seconds.
pub const MOVER_INTERVAL_SECONDS: f64 = 0.047;
/// The walking pace, metres a second.
pub const WALK_SPEED_METERS_PER_SECOND: f64 = 3.12;
/// The narrowest arrival radius the mover can honour, metres.
pub const MINIMUM_ARRIVAL_RADIUS_METERS: f64 =
    WALK_SPEED_METERS_PER_SECOND * MOVER_INTERVAL_SECONDS;
/// How often an absolute re-face may be sent, seconds.
pub const FACE_HEADING_REISSUE_SECONDS: f64 = 0.7;
/// How long the forward key may be held without progress, seconds.
pub const STUCK_SECONDS: f64 = 3.0;
/// The ground that counts as progress, metres.
pub const STUCK_PROGRESS_METERS: f64 = 0.75;
/// No re-face sent yet.
pub const NO_FACE_HEADING_STAMP: f64 = f64::NEG_INFINITY;
/// The near/far split of the heading relaxation, metres.
const NEAR_TARGET_METERS: f64 = 3.0;
/// Inside this the mover walks, metres.
pub const CREEP_DISTANCE_METERS: f64 = 240.0 / 160.0;
const FAR_HEADING_RELAXATION_DEGREES: f32 = 45.0;
const NEAR_HEADING_RELAXATION_DEGREES: f32 = 15.0;

pub use mosstank_automation::heading::{
    desired_heading, horizontal_distance_meters, normalize_heading, prefers_left_turn,
    signed_heading_delta, unsigned_heading_delta,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TurnHold {
    None,
    Left,
    Right,
}

/// What the creep band's push into magic mode works with.
#[allow(missing_debug_implementations)]
pub struct CreepSettings<'a> {
    /// The combat settings.
    pub combat: &'a CombatSettings,
    /// The vital settings.
    pub vitals: &'a VitalSettings,
}

/// The close-in mover.
#[allow(missing_debug_implementations)]
pub struct NavigationMover {
    gate: Option<SharedCombatModeGate>,
    had_movement_intent: bool,
    armed: bool,
    pending_seconds: f64,
    now: f64,
    face_heading_stamp: f64,
    forward_held: bool,
    stuck_anchored: bool,
    stuck_anchor: Position,
    stuck_anchor_at: f64,
    is_stuck: bool,
    status: Option<String>,
    low_stop_distance_warning: bool,
}

impl Default for NavigationMover {
    fn default() -> Self {
        Self::new()
    }
}

impl NavigationMover {
    /// A disarmed mover.
    #[must_use]
    pub fn new() -> Self {
        Self {
            gate: None,
            had_movement_intent: false,
            armed: false,
            pending_seconds: 0.0,
            now: 0.0,
            face_heading_stamp: NO_FACE_HEADING_STAMP,
            forward_held: false,
            stuck_anchored: false,
            stuck_anchor: Position::default(),
            stuck_anchor_at: 0.0,
            is_stuck: false,
            status: None,
            low_stop_distance_warning: false,
        }
    }

    /// Shares the stance gate the creep band pushes into magic through.
    pub fn bind_combat_mode_gate(&mut self, gate: SharedCombatModeGate) {
        self.gate = Some(gate);
    }

    /// The status line the creep push wrote, once.
    pub fn take_status(&mut self) -> Option<String> {
        self.status.take()
    }

    /// Whether a goal tight enough to creep at was walked at in peace mode
    /// (the owner warns once per run).
    pub fn take_low_stop_distance_warning(&mut self) -> bool {
        std::mem::take(&mut self.low_stop_distance_warning)
    }

    /// The mover's clock, seconds.
    #[must_use]
    pub fn now(&self) -> f64 {
        self.now
    }

    /// Whether the last rule pass claimed the turn.
    #[must_use]
    pub fn is_armed(&self) -> bool {
        self.armed
    }

    /// Whether this mover holds the character moving.
    #[must_use]
    pub fn has_movement_intent(&self) -> bool {
        self.had_movement_intent
    }

    /// Whether the walk is stuck.
    #[must_use]
    pub fn is_stuck(&self) -> bool {
        self.is_stuck
    }

    /// Moves the clock on by one frame (the one place it moves).
    pub fn advance_clock(&mut self, elapsed_seconds: f64) {
        if !elapsed_seconds.is_finite() || elapsed_seconds <= 0.0 {
            return;
        }
        self.now += elapsed_seconds;
    }

    /// Arms or disarms the mover on the rule pass's answer.
    pub fn arm(&mut self, claimed: bool) {
        self.armed = claimed;
    }

    /// The time the mover was owed, given up.
    pub fn take_pending_seconds(&mut self) -> f64 {
        std::mem::take(&mut self.pending_seconds)
    }

    /// One host frame: whether it is a steering frame, and for how long.
    pub fn try_take_mover_frame(&mut self, elapsed_seconds: f64) -> Option<f64> {
        let elapsed = if elapsed_seconds.is_finite() && elapsed_seconds > 0.0 {
            elapsed_seconds
        } else {
            0.0
        };
        self.advance_clock(elapsed);
        if !self.armed {
            // A disarmed mover holds no time.
            self.pending_seconds = elapsed;
            return None;
        }
        self.pending_seconds += elapsed;
        if self.pending_seconds < MOVER_INTERVAL_SECONDS {
            return None;
        }
        Some(self.take_pending_seconds())
    }

    /// The rule lost its turn: the keys come up.
    pub fn stop_for_lost_turn(&mut self, automation: &mut dyn Automation) {
        self.armed = false;
        self.pending_seconds = 0.0;
        self.stop_movement(automation);
    }

    /// Forgets the re-face throttle.
    pub fn clear_face_heading_stamp(&mut self) {
        self.face_heading_stamp = NO_FACE_HEADING_STAMP;
    }

    /// Forgets the stuck clock and its verdict.
    pub fn reset_stuck_clock(&mut self) {
        self.stuck_anchored = false;
        self.is_stuck = false;
    }

    fn track_progress(&mut self, current: &Position) {
        if !self.forward_held {
            self.stuck_anchored = false;
            return;
        }
        if !self.stuck_anchored
            || horizontal_distance_meters(current, &self.stuck_anchor) >= STUCK_PROGRESS_METERS
        {
            self.stuck_anchored = true;
            self.stuck_anchor = current.clone();
            self.stuck_anchor_at = self.now;
            return;
        }
        if self.now - self.stuck_anchor_at >= STUCK_SECONDS {
            self.is_stuck = true;
        }
    }

    /// Something else set or cleared the movement intent.
    pub fn note_movement_intent(&mut self, held: bool) {
        self.had_movement_intent = held;
    }

    /// Lets go of whatever this mover holds.
    pub fn stop_movement(&mut self, automation: &mut dyn Automation) {
        self.forward_held = false;
        self.reset_stuck_clock();
        if !self.had_movement_intent {
            return;
        }
        let _ = automation.clear_movement_intent();
        self.had_movement_intent = false;
    }

    /// Steers at a goal; true while it holds the character.
    pub fn steer(
        &mut self,
        automation: &mut dyn Automation,
        creep: Option<&CreepSettings<'_>>,
        current: &Position,
        target: &Position,
        distance_meters: f64,
    ) -> bool {
        let claimed = self.steer_core(automation, creep, current, target, distance_meters);
        self.track_progress(current);
        claimed
    }

    fn steer_core(
        &mut self,
        automation: &mut dyn Automation,
        creep: Option<&CreepSettings<'_>>,
        current: &Position,
        target: &Position,
        distance_meters: f64,
    ) -> bool {
        let desired = desired_heading(current, target);
        let delta = signed_heading_delta(current.heading_degrees, desired);
        let offset = delta.abs();

        if automation.chat_input_active() {
            // A held turn key would go into the chat entry: stop and re-face
            // instead, at most once per re-face interval.
            if offset > HEADING_TOLERANCE_DEGREES {
                let claimed =
                    self.resolve_stop_decision(automation, creep, false, 0.0, TurnHold::None);
                if self.now - self.face_heading_stamp >= FACE_HEADING_REISSUE_SECONDS {
                    self.face_heading_stamp = self.now;
                    let _ = automation.face_heading(desired);
                }
                return claimed;
            }
            self.face_heading_stamp = NO_FACE_HEADING_STAMP;
            return self.resolve_stop_decision(
                automation,
                creep,
                true,
                distance_meters,
                TurnHold::None,
            );
        }

        if offset <= HEADING_TOLERANCE_DEGREES {
            return self.resolve_stop_decision(
                automation,
                creep,
                true,
                distance_meters,
                TurnHold::None,
            );
        }

        let turn = if prefers_left_turn(current.heading_degrees, desired) {
            TurnHold::Left
        } else {
            TurnHold::Right
        };
        let relaxation = if distance_meters > NEAR_TARGET_METERS {
            FAR_HEADING_RELAXATION_DEGREES
        } else {
            NEAR_HEADING_RELAXATION_DEGREES
        };
        if offset > relaxation {
            self.resolve_stop_decision(automation, creep, false, 0.0, turn)
        } else {
            self.resolve_stop_decision(automation, creep, true, distance_meters, turn)
        }
    }

    fn resolve_stop_decision(
        &mut self,
        automation: &mut dyn Automation,
        creep_settings: Option<&CreepSettings<'_>>,
        should_move: bool,
        distance_meters: f64,
        turn: TurnHold,
    ) -> bool {
        let mut creep = should_move && distance_meters < CREEP_DISTANCE_METERS;
        let far = should_move && distance_meters >= CREEP_DISTANCE_METERS;
        // The run key stays on for everything but the creep band.
        let run = !creep;
        if creep && !self.try_prepare_creep_combat_mode(automation, creep_settings) {
            creep = false;
        }
        let forward = creep || far;
        if !forward && turn == TurnHold::None {
            self.stop_movement(automation);
            return true;
        }
        self.had_movement_intent = automation.set_movement_intent(MovementIntent {
            forward,
            turn_left: turn == TurnHold::Left,
            turn_right: turn == TurnHold::Right,
            run,
            ..MovementIntent::default()
        }) == NavigationStatus::Accepted;
        self.forward_held = forward && self.had_movement_intent;
        self.had_movement_intent
    }

    /// The forced push into magic mode inside the creep band.
    fn try_prepare_creep_combat_mode(
        &mut self,
        automation: &mut dyn Automation,
        settings: Option<&CreepSettings<'_>>,
    ) -> bool {
        let (Some(gate), Some(settings)) = (self.gate.clone(), settings) else {
            return true;
        };
        if automation.combat_snapshot().mode != CombatMode::Peace {
            return true;
        }
        if settings.combat.idle_peace_mode {
            self.low_stop_distance_warning = true;
        }
        let mut turn = GateTurn {
            automation,
            combat: settings.combat,
            vitals: settings.vitals,
        };
        if gate
            .borrow_mut()
            .try_prepare(&mut turn, Prepare::mode(CombatMode::Magic), None)
        {
            return true;
        }
        self.status = Some("Switching to magic mode at the waypoint.".to_owned());
        false
    }
}

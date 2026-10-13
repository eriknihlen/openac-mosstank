//! The close-in mover (`NavigationMover`): the piece that turns "the goal
//! is over there" into held keys, and the clock those keys are paced
//! against. It is not the rule and not the goal: each navigation rule
//! builds its own and hands it its own goal, so the route, the corpse walk
//! and the monster walk all steer the same way while owning their own
//! armed state, their own re-face throttle and their own idea of what they
//! walk at.
//!
//! Sharing one mover between two rules would be wrong rather than
//! thrifty: the rule that loses a pass tears its mover down, so two rules
//! on one mover means the loser's teardown disarms the winner.

use std::cell::RefCell;
use std::rc::Rc;

use ac_plugin_abi::combat::CombatMode;
use ac_plugin_abi::navigation::{MovementIntent, NavigationStatus};
use ac_plugin_abi::world::Position;
use mosstank_automation::{Automation, CombatModePreparation};

use crate::client::NavigationAutomation;
use crate::heading::{desired_heading, prefers_left_turn, signed_heading_delta};

/// The alignment band: inside it the mover walks straight.
pub const HEADING_TOLERANCE_DEGREES: f32 = 4.0;

/// How often the mover may steer once the rule has armed it. The rule's
/// turn only arms it; it then runs on the client's own frame, no faster
/// than this. That is what makes the alignment band reachable: one rule
/// pass of held turn is tens of degrees, so a mover stepped once per pass
/// overshoots every turn and hunts around the bearing for ever.
pub const MOVER_INTERVAL_SECONDS: f64 = 0.047;

/// The character's walking pace, metres per second; the mover walks inside
/// the creep band.
pub const WALK_SPEED_METERS_PER_SECOND: f64 = 3.12;

/// The narrowest arrival radius the mover can honour: the ground one
/// steering step covers at a walk, about 0.15 m. The mover only sees where
/// the character is once a step, so a circle crossed in less than a step
/// can be walked straight through between two looks.
pub const MINIMUM_ARRIVAL_RADIUS_METERS: f64 =
    WALK_SPEED_METERS_PER_SECOND * MOVER_INTERVAL_SECONDS;

/// How long the walking branch waits between re-faces while the player
/// types.
pub const FACE_HEADING_REISSUE_SECONDS: f64 = 0.7;

/// How long the mover may hold the forward key without the character
/// covering [`STUCK_PROGRESS_METERS`] before it reports itself stuck.
/// Longer than any turn in place, shorter than a player's patience at a
/// wall. The clock runs only while a forward key is held.
pub const STUCK_SECONDS: f64 = 3.0;

/// Ground the character must cover inside [`STUCK_SECONDS`] to count as
/// moving.
pub const STUCK_PROGRESS_METERS: f64 = 0.75;

/// The re-face stamp that means "never": the next departure re-faces at
/// once.
pub const NO_FACE_HEADING_STAMP: f64 = f64::NEG_INFINITY;

/// The near/far split the heading relaxation switches on, metres.
const NEAR_TARGET_METERS: f64 = 3.0;

/// Inside this the mover walks instead of running. A waypoint whose
/// arrival radius is wider than this is never approached at a walk, which
/// is what the low-minimum-distance warning is about.
pub const CREEP_DISTANCE_METERS: f64 = 240.0 / 160.0;

const FAR_HEADING_RELAXATION_DEGREES: f32 = 45.0;
const NEAR_HEADING_RELAXATION_DEGREES: f32 = 15.0;

/// Where the mover's owner hears what the mover has to say: the one status
/// line the creep push writes, and the warning about walking at a tight
/// goal in peace mode (the owner keeps the once-per-run bookkeeping).
pub trait MoverNotes {
    /// The mover's status line.
    fn status(&mut self, text: &str);

    /// A goal this tight is being walked at in peace mode.
    fn warn_low_stop_distance(&mut self, automation: &mut dyn NavigationAutomation);
}

/// Which way the mover holds the turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TurnHold {
    None,
    Left,
    Right,
}

/// One rule's mover.
pub struct NavigationMover {
    combat_mode_gate: Option<Rc<RefCell<dyn CombatModePreparation>>>,
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
}

impl std::fmt::Debug for NavigationMover {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NavigationMover")
            .field("armed", &self.armed)
            .field("now", &self.now)
            .field("had_movement_intent", &self.had_movement_intent)
            .field("is_stuck", &self.is_stuck)
            .finish_non_exhaustive()
    }
}

impl Default for NavigationMover {
    fn default() -> Self {
        Self::new()
    }
}

impl NavigationMover {
    /// A disarmed mover at time zero.
    #[must_use]
    pub fn new() -> Self {
        Self {
            combat_mode_gate: None,
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
        }
    }

    /// Binds the macro's combat-mode gate, which the creep push asks for
    /// magic mode.
    pub fn bind_combat_mode_gate(&mut self, gate: Rc<RefCell<dyn CombatModePreparation>>) {
        self.combat_mode_gate = Some(gate);
    }

    /// The mover's own clock, seconds since it was made.
    #[must_use]
    pub fn now(&self) -> f64 {
        self.now
    }

    /// Whether the last rule pass claimed the turn.
    #[must_use]
    pub fn is_armed(&self) -> bool {
        self.armed
    }

    /// Whether the character is currently held moving by this mover.
    #[must_use]
    pub fn has_movement_intent(&self) -> bool {
        self.had_movement_intent
    }

    /// Moves the clock on by one frame. This is the one place it moves:
    /// the clock stands for wall time, and a turn or a pass is not a unit
    /// of it.
    pub fn advance_clock(&mut self, elapsed_seconds: f64) {
        if !elapsed_seconds.is_finite() || elapsed_seconds <= 0.0 {
            return;
        }
        self.now += elapsed_seconds;
    }

    /// Arms or disarms the mover on the rule pass's own answer.
    pub fn arm(&mut self, claimed: bool) {
        self.armed = claimed;
    }

    /// The time the mover has gathered since its last steer, taken.
    pub fn take_pending_seconds(&mut self) -> f64 {
        let due = self.pending_seconds;
        self.pending_seconds = 0.0;
        due
    }

    /// One host frame of the armed mover's bookkeeping: whether this frame
    /// is a steering frame, and how much time that steer is for. What the
    /// mover makes of a frame never disarms it: only the rule that armed it
    /// can take the turn back.
    pub fn try_take_mover_frame(&mut self, elapsed_seconds: f64) -> Option<f64> {
        let elapsed = if elapsed_seconds.is_finite() && elapsed_seconds > 0.0 {
            elapsed_seconds
        } else {
            0.0
        };
        self.advance_clock(elapsed);
        if !self.armed {
            // A disarmed mover holds no time: the pass that arms it starts
            // from this frame, not from however long it was idle.
            self.pending_seconds = elapsed;
            return None;
        }
        self.pending_seconds += elapsed;
        if self.pending_seconds < MOVER_INTERVAL_SECONDS {
            return None;
        }
        Some(self.take_pending_seconds())
    }

    /// The rule's teardown on the losing-the-turn edge: the held movement
    /// keys are let go.
    pub fn stop_for_lost_turn(&mut self, automation: &mut dyn NavigationAutomation) {
        self.armed = false;
        self.pending_seconds = 0.0;
        self.stop_movement(automation);
    }

    /// Forgets the re-face throttle, so the next departure re-faces at
    /// once.
    pub fn clear_face_heading_stamp(&mut self) {
        self.face_heading_stamp = NO_FACE_HEADING_STAMP;
    }

    /// The straight walk has held the forward key for [`STUCK_SECONDS`]
    /// without covering ground. Stays set until [`Self::reset_stuck_clock`],
    /// so the rule that reads it decides what to do exactly once.
    #[must_use]
    pub fn is_stuck(&self) -> bool {
        self.is_stuck
    }

    /// Forgets the stuck clock's anchor and its verdict; the next held
    /// frame starts it over.
    pub fn reset_stuck_clock(&mut self) {
        self.stuck_anchored = false;
        self.is_stuck = false;
    }

    /// One steering frame's worth of the stuck clock: anchored where the
    /// character stood when the forward key was first held, moved every
    /// time it covers the progress distance, read against the mover's
    /// clock.
    fn track_progress(&mut self, current: &Position) {
        if !self.forward_held {
            self.stuck_anchored = false;
            return;
        }
        if !self.stuck_anchored
            || current.horizontal_distance_metres(&self.stuck_anchor) >= STUCK_PROGRESS_METERS
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

    /// Records that something other than the mover's own steer set or
    /// cleared the movement intent, so a later stop knows whether it has
    /// anything to clear.
    pub fn note_movement_intent(&mut self, held: bool) {
        self.had_movement_intent = held;
    }

    /// Lets go of the held keys, if any.
    pub fn stop_movement(&mut self, automation: &mut dyn NavigationAutomation) {
        self.forward_held = false;
        self.reset_stuck_clock();
        if !self.had_movement_intent {
            return;
        }
        let _ = automation.clear_movement_intent();
        self.had_movement_intent = false;
    }

    /// Steers at a goal. Outside the alignment band the mover holds a turn
    /// key and keeps walking, so the character curves onto the bearing; the
    /// two relaxation tiers decide only whether it moves while it turns.
    /// The absolute re-face is the typing branch, where a held key would go
    /// into the chat entry. `idle_peace_mode` is the combat profile's
    /// setting, which only decides whether the tight-goal warning is said.
    pub fn steer(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        current: &Position,
        target: &Position,
        distance_meters: f64,
        idle_peace_mode: bool,
        notes: &mut dyn MoverNotes,
    ) -> bool {
        let claimed = self.steer_core(
            automation,
            current,
            target,
            distance_meters,
            idle_peace_mode,
            notes,
        );
        self.track_progress(current);
        claimed
    }

    fn steer_core(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        current: &Position,
        target: &Position,
        distance_meters: f64,
        idle_peace_mode: bool,
        notes: &mut dyn MoverNotes,
    ) -> bool {
        let desired = desired_heading(current, target);
        let delta = signed_heading_delta(current.heading_degrees, desired);
        let offset = delta.abs();

        if automation.chat_input_active() {
            // A held turn key would go into the chat entry, so this branch
            // stops and re-faces the goal instead, at most once per re-face
            // interval. Inside the band it makes the same stop decision as
            // every other branch.
            if offset > HEADING_TOLERANCE_DEGREES {
                let claimed = self.resolve_stop_decision(
                    automation,
                    false,
                    0.0,
                    TurnHold::None,
                    idle_peace_mode,
                    notes,
                );
                if self.now - self.face_heading_stamp >= FACE_HEADING_REISSUE_SECONDS {
                    self.face_heading_stamp = self.now;
                    let _ = automation.face_heading(desired);
                }
                return claimed;
            }
            self.face_heading_stamp = NO_FACE_HEADING_STAMP;
            return self.resolve_stop_decision(
                automation,
                true,
                distance_meters,
                TurnHold::None,
                idle_peace_mode,
                notes,
            );
        }

        if offset <= HEADING_TOLERANCE_DEGREES {
            return self.resolve_stop_decision(
                automation,
                true,
                distance_meters,
                TurnHold::None,
                idle_peace_mode,
                notes,
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
            self.resolve_stop_decision(automation, false, 0.0, turn, idle_peace_mode, notes)
        } else {
            self.resolve_stop_decision(
                automation,
                true,
                distance_meters,
                turn,
                idle_peace_mode,
                notes,
            )
        }
    }

    /// Turns "should I be moving, and how far away is the goal" into the
    /// one movement intent the client takes. Inside the creep band the mover
    /// walks rather than runs, and while it walks in peace mode it keeps
    /// asking for magic mode: a goal that tight is meant to be stood on.
    fn resolve_stop_decision(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        should_move: bool,
        distance_meters: f64,
        turn: TurnHold,
        idle_peace_mode: bool,
        notes: &mut dyn MoverNotes,
    ) -> bool {
        let mut creep = should_move && distance_meters < CREEP_DISTANCE_METERS;
        let far = should_move && distance_meters >= CREEP_DISTANCE_METERS;
        // The run key stays on for everything but the creep band, turning in
        // place included: a turn is played at its fast rate only while the
        // run key counts as held.
        let run = !creep;
        if creep && !self.try_prepare_creep_combat_mode(automation, idle_peace_mode, notes) {
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

    /// The forced magic-mode push, retried on every tick that wants to
    /// creep.
    fn try_prepare_creep_combat_mode(
        &mut self,
        automation: &mut dyn NavigationAutomation,
        idle_peace_mode: bool,
        notes: &mut dyn MoverNotes,
    ) -> bool {
        let Some(gate) = self.combat_mode_gate.clone() else {
            return true;
        };
        if automation.combat_mode() != CombatMode::Peace {
            return true;
        }
        if idle_peace_mode {
            notes.warn_low_stop_distance(automation);
        }
        let rules: &mut dyn Automation = automation;
        if gate
            .borrow_mut()
            .try_prepare(rules, CombatMode::Magic, 0, true)
        {
            return true;
        }
        notes.status("Switching to magic mode at the waypoint.");
        false
    }
}

//! The command jump and turn (`StartCommandJump`, `TickCommandJump`): turn
//! to a heading, then ask the client for a jump of exactly the power the
//! charge time comes to, a full charge per second, while it holds whichever
//! movement keys the jump leans on. `/ub jump`, `/ub simplejump` and
//! `/ub face` use it with the client's own turn; the macro's `/vt jump`
//! with held turn keys ([`JumpTurn::HeldKeys`]), whose lines are the
//! macro's own.

use mosstank_automation::heading::{normalize_heading, signed_heading_delta};
use mosstank_automation::locks::ActionLockKind;
use mosstank_formats::route::RouteJumpDirection;
use openac_plugin_compat::number;

use super::call::UbCall;
use crate::chat::{self, tools};
use crate::host::{MoveChannel, MoveDirection, MovePace, MoveUnit, NavigationStatus};
use ac_plugin_abi::navigation::MovementIntent;

/// How long after the charge is let go the character is given to settle
/// before the client is asked whether a jump happened at all.
pub const SETTLE_SECONDS: f64 = 0.25;
/// How long past the hold a charge is given to become a jump before that
/// attempt counts as never having taken.
pub const GRACE_SECONDS: f64 = 5.0;
/// How long a jump holds the route still when Jumper.PauseNav is on (a
/// ceiling: the hold is dropped the moment the jump ends).
pub const JUMP_NAVIGATION_HOLD_SECONDS: f64 = 20.0;
/// The same ceiling for a turn.
pub const TURN_NAVIGATION_HOLD_SECONDS: f64 = 15.0;
/// How often a client turn is looked at.
pub const CLIENT_TURN_CHECK_SECONDS: f64 = 0.1;
/// How close a client turn has to get to count as done, degrees.
pub const CLIENT_TURN_TOLERANCE_DEGREES: f32 = 1.0;
/// How long a client turn is given before it counts as failed.
pub const CLIENT_TURN_GIVE_UP_SECONDS: f64 = 5.0;

/// How a command jump or turn gets the character pointing the right way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JumpTurn {
    /// Hold the turn keys until within a few degrees: the macro's own jump.
    #[default]
    HeldKeys,
    /// Ask the client to turn to the exact heading and look again every
    /// tenth of a second: the reference's face and jump.
    Client,
    /// No turn at all: the reference's jump with no heading.
    None,
}

/// What a jump is asked to do.
#[derive(Debug, Clone, PartialEq)]
pub struct JumpRequest {
    /// Where to face first, degrees.
    pub heading: f32,
    /// Walk rather than run (the game's shift key).
    pub shift: bool,
    /// How long the charge is held, milliseconds.
    pub milliseconds: i32,
    /// The one direction the jump leaves in, when the keys are not given.
    pub direction: Option<RouteJumpDirection>,
    /// Turn only; no jump.
    pub face_only: bool,
    /// Press no movement key at all: straight up.
    pub omit_direction: bool,
    /// How the character is pointed at the heading.
    pub turn: JumpTurn,
    /// The movement keys to hold, when named outright; the shift and the
    /// jump are added to them.
    pub keys: Option<MovementIntent>,
}

impl JumpRequest {
    /// A jump toward `heading` holding `milliseconds`, turning on held keys.
    #[must_use]
    pub fn new(heading: f32, milliseconds: i32) -> Self {
        Self {
            heading,
            shift: false,
            milliseconds,
            direction: None,
            face_only: false,
            omit_direction: false,
            turn: JumpTurn::HeldKeys,
            keys: None,
        }
    }
}

/// The jump or turn in progress.
#[derive(Debug, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct CommandJump {
    active: bool,
    turn: JumpTurn,
    turning: bool,
    turn_since_check: f64,
    held_navigation: bool,
    released: bool,
    charging: bool,
    face_only: bool,
    saw_airborne: bool,
    attempt: i32,
    sequence: i64,
    elapsed: f64,
    turn_elapsed: f64,
    charge_seconds: f64,
    heading: f32,
    intent: MovementIntent,
    exact: bool,
    power: f32,
    moving: bool,
}

/// `{heading:0.#}`.
fn heading_text(heading: f32) -> String {
    number::format_f32(heading, "0.#").expect("a number format the kit reproduces")
}

impl CommandJump {
    /// Nothing in the air.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a jump or turn is under way.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Begins a jump or turn.
    pub fn start(&mut self, call: &mut UbCall<'_>, request: &JumpRequest) {
        let snapshot = call.host.navigation();
        if !snapshot.is_available || snapshot.is_portal_space {
            let refusal = if request.face_only {
                "Turning unavailable outside the world."
            } else {
                "Jump unavailable outside the world."
            };
            // The macro's own jump answers in its own words; the UtilityBelt
            // commands answer as the jumper tool's error.
            if request.turn == JumpTurn::HeldKeys {
                call.write_vtank(refusal);
            } else {
                call.post_ub(&chat::tool_error(tools::JUMPER, refusal));
            }
            return;
        }
        let resolved = request.direction.unwrap_or(RouteJumpDirection::Forward);
        self.heading = normalize_heading(request.heading);
        // A caller that names its keys outright holds exactly those;
        // otherwise the one direction. Shift is the game's walk key.
        self.intent = match request.keys.clone() {
            Some(held) => MovementIntent {
                run: !request.shift,
                jump: true,
                ..held
            },
            None => {
                let leans = |direction| !request.omit_direction && resolved == direction;
                MovementIntent {
                    forward: leans(RouteJumpDirection::Forward),
                    backward: leans(RouteJumpDirection::Backward),
                    strafe_left: leans(RouteJumpDirection::StrafeLeft),
                    strafe_right: leans(RouteJumpDirection::StrafeRight),
                    turn_left: false,
                    turn_right: false,
                    run: !request.shift,
                    jump: true,
                }
            }
        };
        // Forward with backward, or left with right, is two keys on one
        // channel, which a client-driven move cannot hold; that jump charges
        // on the held keys instead.
        let travel_pair = self.intent.forward && self.intent.backward;
        let strafe_pair = self.intent.strafe_left && self.intent.strafe_right;
        self.exact = !travel_pair && !strafe_pair;
        self.power = mosstank_nav::jump_power::from_milliseconds(request.milliseconds);
        self.moving = false;
        self.charge_seconds = if self.exact {
            mosstank_nav::jump_power::charge_seconds(request.milliseconds)
        } else {
            (f64::from(request.milliseconds) / 1000.0).clamp(0.05, 5.0)
        };
        self.elapsed = 0.0;
        self.turn_elapsed = 0.0;
        self.released = false;
        self.charging = false;
        self.face_only = request.face_only;
        self.saw_airborne = false;
        self.attempt = 0;
        self.sequence = 0;
        self.turn = request.turn;
        self.turning = request.turn == JumpTurn::Client;
        // The first look comes on the very next tick.
        self.turn_since_check = CLIENT_TURN_CHECK_SECONDS;
        self.active = true;
        self.hold_navigation(
            call,
            if request.face_only {
                TURN_NAVIGATION_HOLD_SECONDS
            } else {
                JUMP_NAVIGATION_HOLD_SECONDS
            },
        );
        if request.turn == JumpTurn::Client {
            // Whatever it answers, the checks that follow ask again, and the
            // time limit ends a turn that never comes.
            call.host.face_heading(self.heading);
            return;
        }
        if request.turn == JumpTurn::HeldKeys {
            let heading = heading_text(self.heading);
            call.write_vtank(&if request.face_only {
                format!("Turning to heading {heading}.")
            } else {
                format!("Turning to heading {heading} for jump.")
            });
        }
    }

    /// One tick of a client turn: every tenth of a second it looks at the
    /// heading; within a degree ends the turn, anything else asks the client
    /// to turn again. Five seconds without getting there fails the command.
    fn tick_client_turn(&mut self, call: &mut UbCall<'_>, heading: f32, elapsed_seconds: f64) {
        let elapsed = elapsed_seconds.max(0.0);
        self.turn_elapsed += elapsed;
        self.turn_since_check += elapsed;
        if self.turn_since_check >= CLIENT_TURN_CHECK_SECONDS {
            self.turn_since_check = 0.0;
            let delta = signed_heading_delta(heading, self.heading);
            if delta.abs() < CLIENT_TURN_TOLERANCE_DEGREES {
                self.turning = false;
            } else {
                call.host.face_heading(self.heading);
            }
        }
        if self.turning && self.turn_elapsed >= CLIENT_TURN_GIVE_UP_SECONDS {
            self.turning = false;
            self.fail(call, "Turning failed");
        }
    }

    /// Holds the route still for the length of a jump or turn, when
    /// Jumper.PauseNav asks for it.
    fn hold_navigation(&mut self, call: &mut UbCall<'_>, seconds: f64) {
        if !call.flag("Jumper.PauseNav") {
            return;
        }
        self.held_navigation = true;
        call.locks
            .borrow_mut()
            .arm(ActionLockKind::Navigation, seconds);
        call.locks
            .borrow_mut()
            .arm(ActionLockKind::CorpseOpenAttempt, seconds);
    }

    /// Lets the route go again; only a hold this command took is dropped.
    fn release_navigation(&mut self, call: &mut UbCall<'_>) {
        if !self.held_navigation {
            return;
        }
        self.held_navigation = false;
        call.locks.borrow_mut().release(ActionLockKind::Navigation);
        call.locks
            .borrow_mut()
            .release(ActionLockKind::CorpseOpenAttempt);
    }

    /// Ends a jump or turn that did not come off: the route is let go, and
    /// the reason goes out as a think when Jumper.ThinkFail asks for it and
    /// as a written line otherwise.
    fn fail(&mut self, call: &mut UbCall<'_>, reason: &str) {
        self.active = false;
        self.release_navigation(call);
        if call.flag("Jumper.ThinkFail") {
            call.think(reason);
        } else if self.turn == JumpTurn::HeldKeys {
            call.write_vtank(reason);
        } else {
            call.write_ub(reason);
        }
    }

    /// One frame; true while the jump owns the character.
    pub fn tick(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64) -> bool {
        if !self.active {
            return false;
        }
        let snapshot = call.host.navigation();
        if !snapshot.is_available || snapshot.is_portal_space {
            self.release_keys(call);
            self.fail(
                call,
                if self.face_only {
                    "Turn canceled because the character left the world."
                } else {
                    "Jump canceled because the character left the world."
                },
            );
            return false;
        }

        if !self.charging {
            let heading = snapshot.position.heading_degrees;
            if self.turn == JumpTurn::HeldKeys {
                self.turn_elapsed += elapsed_seconds.max(0.0);
                let delta = signed_heading_delta(heading, self.heading);
                if delta.abs() > 4.0 {
                    let turned = self.turn_elapsed <= 10.0
                        && call.host.set_movement_intent(MovementIntent {
                            turn_left: delta < 0.0,
                            turn_right: delta > 0.0,
                            run: self.intent.run,
                            ..MovementIntent::default()
                        }) == NavigationStatus::Accepted;
                    if !turned {
                        call.host.clear_movement_intent();
                        self.fail(
                            call,
                            if self.face_only {
                                "Could not turn to the requested heading."
                            } else {
                                "Jump command could not align to the requested heading."
                            },
                        );
                        return false;
                    }
                    return true;
                }
                // Facing a heading is this loop and nothing after it.
                if self.face_only {
                    call.host.clear_movement_intent();
                    self.active = false;
                    self.release_navigation(call);
                    call.write_vtank(&format!("Facing heading {}.", heading_text(self.heading)));
                    if call.flag("Jumper.ThinkComplete") {
                        call.think("Turning Success");
                    }
                    return false;
                }
            } else {
                if self.turning {
                    self.tick_client_turn(call, heading, elapsed_seconds);
                    if self.turning || !self.active {
                        return self.active;
                    }
                }
                // Facing a heading is the turn and nothing after it; the
                // client's turn is left to land rather than stopped.
                if self.face_only {
                    self.active = false;
                    self.release_navigation(call);
                    if call.flag("Jumper.ThinkComplete") {
                        call.think("Turning Success");
                    }
                    return false;
                }
            }

            // The client counts the jumps it has begun; this charge has to
            // move that count on.
            self.sequence = call.host.move_report().jump_sequence;
            self.charging = if self.exact {
                self.begin_exact(call)
            } else {
                call.host.set_movement_intent(self.intent.clone()) == NavigationStatus::Accepted
            };
            if !self.charging {
                self.release_keys(call);
                self.fail(call, "Jump command was refused by the host.");
                return false;
            }
            self.elapsed = 0.0;
            self.released = false;
            self.saw_airborne = false;
            self.attempt += 1;
            if self.turn == JumpTurn::HeldKeys {
                let heading = heading_text(self.heading);
                call.write_vtank(&if self.attempt == 1 {
                    format!("Jump charging at heading {heading}.")
                } else {
                    format!(
                        "Jump charging at heading {heading} (attempt {}).",
                        self.attempt
                    )
                });
            }
        }

        self.elapsed += elapsed_seconds.max(0.0);
        if !self.released && self.elapsed >= self.charge_seconds {
            self.released = true;
            // An exact jump is let go by the client itself, at its power.
            if !self.exact {
                call.host.set_movement_intent(MovementIntent {
                    jump: false,
                    ..self.intent.clone()
                });
            }
        }
        if self.elapsed < self.charge_seconds + SETTLE_SECONDS {
            return true;
        }

        // Either the character left the ground or the count of jumps begun
        // moved on: the attempt is over.
        self.saw_airborne |= snapshot.is_airborne;
        if self.saw_airborne || call.host.move_report().jump_sequence != self.sequence {
            self.release_keys(call);
            self.active = false;
            self.charging = false;
            self.release_navigation(call);
            if call.flag("Jumper.ThinkComplete") {
                call.think("Jumper Success");
            }
            return false;
        }
        if self.elapsed < self.charge_seconds + GRACE_SECONDS {
            return true;
        }

        // No jump reported: charge again from the alignment, up to the
        // attempt ceiling, and then let the character go.
        if self.attempt >= call.services.jump_attempts().max(1) {
            self.release_keys(call);
            self.charging = false;
            // The reference's commands give up in the reference's words.
            let reason = if self.turn == JumpTurn::HeldKeys {
                format!(
                    "Jump gave up after {} attempt(s) with no jump reported.",
                    self.attempt
                )
            } else {
                "You have failed to jump too many times.".to_owned()
            };
            self.fail(call, &reason);
            return false;
        }
        // The keys an exact charge held are let go before the next
        // alignment, which a held turn could not make under them.
        if self.moving {
            self.release_keys(call);
        }
        self.charging = false;
        self.turn_elapsed = 0.0;
        // The hold was taken against one charge; the next one takes its own.
        self.hold_navigation(call, JUMP_NAVIGATION_HOLD_SECONDS);
        true
    }

    /// Asks the client for the jump at its exact power, holding the jump's
    /// movement keys as client-driven moves at the jump's pace for the
    /// charge. False when the client refuses any part of it.
    fn begin_exact(&mut self, call: &mut UbCall<'_>) -> bool {
        if self.turn == JumpTurn::HeldKeys {
            call.host.clear_movement_intent();
        }
        let pace = if self.intent.run {
            MovePace::Run
        } else {
            MovePace::Walk
        };
        self.moving = true;
        let mut accepted = true;
        for (held, direction) in [
            (self.intent.forward, MoveDirection::Forward),
            (self.intent.backward, MoveDirection::Backward),
            (self.intent.strafe_left, MoveDirection::StrafeLeft),
            (self.intent.strafe_right, MoveDirection::StrafeRight),
        ] {
            if held {
                accepted &= call
                    .host
                    .move_by(direction, pace, 0.0, MoveUnit::MetersOrDegrees)
                    == NavigationStatus::Accepted;
            }
        }
        accepted && call.host.jump(self.power) == NavigationStatus::Accepted
    }

    /// Lets go of the keys a jump holds: the channels an exact jump's moves
    /// are on and nothing else, or the held keys.
    fn release_keys(&mut self, call: &mut UbCall<'_>) {
        if !self.moving {
            call.host.clear_movement_intent();
            return;
        }
        self.moving = false;
        if self.intent.forward || self.intent.backward {
            call.host.stop_moving_channel(MoveChannel::Travel);
        }
        if self.intent.strafe_left || self.intent.strafe_right {
            call.host.stop_moving_channel(MoveChannel::Strafe);
        }
    }

    /// The session ended: whatever was in the air is let go
    /// (`ResetCommandSession`).
    pub fn reset(&mut self, call: &mut UbCall<'_>) {
        if self.active || self.charging {
            self.release_keys(call);
        }
        let held = self.held_navigation;
        *self = Self {
            held_navigation: held,
            ..Self::default()
        };
        self.release_navigation(call);
    }
}

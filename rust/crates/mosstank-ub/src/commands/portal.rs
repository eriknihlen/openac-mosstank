//! The portal command's use (`UsePortal`, `TickPortalUse`,
//! `OnPortalUseTransition`, `FinishPortal`): the route is held still from
//! the command until the character enters portal space, the portal is tried
//! every `Plugin.PortalTimeout` milliseconds up to `Plugin.PortalAttempts`
//! times, and the use is given up after the last try.

use ac_plugin_abi::world::{PortalTransition, WorldObject};
use mosstank_automation::locks::ActionLockKind;

use super::call::UbCall;
use crate::chat::{self, tools};

/// The transition kind of a portal or recall while already in the world.
const PORTAL_TRANSITION_PORTAL: u8 = 2;

/// The portal use under way.
#[derive(Debug, Default)]
pub struct PortalUse {
    /// The portal's id and name.
    target_id: u32,
    target_name: String,
    /// The number of the next try; zero when no use is under way.
    attempts: i32,
    /// Milliseconds since the last try.
    since_try_milliseconds: f64,
    /// When the route hold this use took runs out, on the lock clock.
    hold_until: f64,
    /// The newest portal generation the client has reported, and the one
    /// that was newest when the use started: only a hop newer than that is
    /// this portal's.
    seen_generation: i64,
    start_generation: i64,
}

/// How long a try at the portal is given before the next, in milliseconds.
fn timeout_milliseconds(call: &UbCall<'_>) -> i32 {
    call.settings.value("Plugin.PortalTimeout").as_int32()
}

/// The route hold one try takes, seconds: the timeout and half a second.
fn hold_seconds(timeout: i32) -> f64 {
    f64::from(500 + timeout) / 1000.0
}

/// Holds the route for one try; when the hold runs out, on the lock clock.
fn hold_route(call: &UbCall<'_>, timeout: i32) -> f64 {
    let seconds = hold_seconds(timeout);
    let mut locks = call.locks.borrow_mut();
    locks.arm(ActionLockKind::Navigation, seconds);
    locks.now() + seconds
}

impl PortalUse {
    /// Nothing under way.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a portal use is under way.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.attempts > 0
    }

    /// Starts using a portal. The route is held still for as long as the
    /// use is under way -- otherwise a route whose next point lies past the
    /// portal walks the character off the portal on the very next frame --
    /// the character's own run is stopped, and the first try goes out a
    /// quarter of a second from now.
    pub fn start(&mut self, call: &mut UbCall<'_>, portal: &WorldObject) {
        self.target_id = portal.object_id;
        self.target_name.clone_from(&portal.name);
        let timeout = timeout_milliseconds(call);
        self.hold_until = hold_route(call, timeout);
        self.attempts = 1;
        self.start_generation = self.seen_generation;
        // As if the last try went out a quarter second short of the
        // timeout, so the first one goes on the frame a quarter second from
        // now.
        self.since_try_milliseconds = f64::from(timeout - 250);
        // The reference turns autorun off here. Neither client gives a
        // plugin the autorun switch, so this lets go of the movement keys the
        // macro holds, which is what runs the route (register UB-8).
        call.host.clear_movement_intent();
        call.write_ub_tool_debug(
            tools::PLUGIN,
            &format!("Attempting to use portal {}", portal.name),
        );
    }

    /// One frame: once the timeout has passed since the last try, the next
    /// try goes out with the route held for another timeout; after the last
    /// one the use is given up.
    pub fn tick(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64) {
        if self.attempts <= 0 {
            return;
        }
        if elapsed_seconds.is_finite() && elapsed_seconds > 0.0 {
            self.since_try_milliseconds += elapsed_seconds * 1000.0;
        }
        let timeout = timeout_milliseconds(call);
        if self.since_try_milliseconds <= f64::from(timeout) {
            return;
        }

        if self.attempts <= call.settings.value("Plugin.PortalAttempts").as_int32() {
            if self.attempts > 1 {
                call.write_ub_tool_debug(tools::PLUGIN, "Use Portal Timed out, trying again");
            }
            self.hold_until = hold_route(call, timeout);
            self.attempts += 1;
            self.since_try_milliseconds = 0.0;
            call.host.use_item(self.target_id);
            return;
        }

        let line = chat::tool(
            tools::PLUGIN,
            &format!("Unable to use portal {}", self.target_name),
        );
        call.post_ub(&line);
        // A turn of one degree cancels the last use still walking to the
        // portal.
        let heading = call.host.navigation().position.heading_degrees;
        call.host.face_heading(heading - 1.0);
        self.finish(call, false);
    }

    /// Every lock was just cleared (the macro started): a use under way
    /// takes back the rest of its route hold.
    pub fn rearm_hold(&self, call: &UbCall<'_>) {
        if self.attempts <= 0 {
            return;
        }
        let mut locks = call.locks.borrow_mut();
        let left = self.hold_until - locks.now();
        if left > 0.0 {
            locks.arm(ActionLockKind::Navigation, left);
        }
    }

    /// A portal transition the client reported. Entering portal space while
    /// a use is under way is the use done -- but only a hop newer than the
    /// one under way when the use started: a hop the character was already
    /// in (a recall the route counted done before it landed) goes on
    /// reporting, and its notices are not this portal. The first notice of a
    /// newer hop is its entry, or its end for a client that reports a short
    /// hop only by its end.
    pub fn on_portal_transition(&mut self, call: &mut UbCall<'_>, transition: &PortalTransition) {
        let newer = transition.generation > self.start_generation;
        self.seen_generation = self.seen_generation.max(transition.generation);
        if self.attempts > 0
            && newer
            && transition.kind == PORTAL_TRANSITION_PORTAL
            && !transition.is_cancelled
        {
            self.finish(call, true);
        }
    }

    /// Ends the use either way: says how it went (a think when
    /// `Plugin.PortalThink` is on) and lets the route go.
    fn finish(&mut self, call: &mut UbCall<'_>, success: bool) {
        let text = if success {
            "portal used successfully"
        } else {
            "failed to use portal"
        };
        if call.flag("Plugin.PortalThink") {
            call.think(text);
        } else {
            call.write_ub(text);
        }
        self.target_id = 0;
        self.target_name.clear();
        self.attempts = 0;
        call.locks.borrow_mut().release(ActionLockKind::Navigation);
    }

    /// The session ended: a use under way ends with it, silently.
    pub fn reset(&mut self, call: &mut UbCall<'_>) {
        if self.attempts > 0 {
            call.locks.borrow_mut().release(ActionLockKind::Navigation);
        }
        *self = Self::default();
    }
}

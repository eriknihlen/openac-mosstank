//! Opening a door in the way, as its own turn of the macro (`OpenDoorRule`).
//!
//! A closed door is a wall, and a wall in front of a bot that is trying to
//! walk somewhere stops everything downstream of it. That is why it is
//! claimed before looting a corpse, before approaching one, and before
//! attacking: whatever those rules want to do, they want to do it on the
//! other side. The door logic itself stays on the navigation controller,
//! which owns the mover and the door ranges; this is the turn it takes.

use mosstank_meta::scheduler::{MacroFamily, MacroPassContext, MacroRule};

/// What the door rule asks of the plugin's state for the call.
pub trait DoorRuleHost {
    /// The rule's own gate (the C#'s `enabled`).
    fn door_rule_enabled(&mut self) -> bool;

    /// Whether something else holds a lock this rule must respect.
    fn door_rule_locked(&mut self) -> bool {
        false
    }

    /// Takes the locks this rule holds while it opens a door.
    fn door_rule_arm(&mut self) {}

    /// The navigation controller's door turn
    /// ([`crate::NavigationController::tick_door_rule`]).
    fn tick_door_rule(&mut self, elapsed_seconds: f64, can_act: bool) -> bool;

    /// The navigation controller's door status
    /// ([`crate::NavigationController::door_status`]).
    fn door_status(&mut self) -> String;
}

/// The door rule.
#[derive(Debug, Default)]
pub struct OpenDoorRule {
    running: bool,
    gate_closed: bool,
    lock_held: bool,
}

impl OpenDoorRule {
    /// A rule that has not been asked yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether something else currently holds a lock this rule must
    /// respect.
    pub fn is_locked<C: DoorRuleHost + ?Sized>(&self, cx: &mut C) -> bool {
        cx.door_rule_locked()
    }
}

impl<F: MacroFamily> MacroRule<F> for OpenDoorRule
where
    for<'a> F::Cx<'a>: DoorRuleHost,
{
    fn name(&self) -> String {
        "OpenDoor".to_owned()
    }

    fn shape(&self) -> mosstank_meta::scheduler::RuleShape {
        mosstank_meta::scheduler::RuleShape::Other("OpenDoorRule")
    }

    fn valid_now(&mut self, cx: &mut F::Cx<'_>, context: MacroPassContext) -> bool {
        let gate_open = cx.door_rule_enabled();
        self.gate_closed = !gate_open;
        self.lock_held = gate_open && cx.door_rule_locked();
        if !gate_open || self.lock_held {
            return false;
        }
        let allowed = context.can_act;
        let claimed = cx.tick_door_rule(context.elapsed_seconds, allowed);
        if allowed && claimed {
            cx.door_rule_arm();
        }
        allowed && claimed
    }

    fn running(&self) -> bool {
        self.running
    }

    /// Losing the turn changes nothing here: the door is not moving the
    /// character (the route rules are), so a door rule that stopped the
    /// mover on its way out would stop a turn it never owned.
    fn set_running(&mut self, _cx: &mut F::Cx<'_>, running: bool) {
        self.running = running;
    }

    /// What the door is doing on the pass it wins: the same sentences the
    /// decline says.
    fn running_detail(&mut self, cx: &mut F::Cx<'_>) -> Option<String> {
        Some(cx.door_status())
    }

    /// Why this rule declined, in its own words: stable while the
    /// situation is, so the scheduler can suppress a repeat.
    fn decline_reason(&mut self, cx: &mut F::Cx<'_>) -> Option<String> {
        Some(if self.gate_closed {
            "the rule's own gate is closed".to_owned()
        } else if self.lock_held {
            "another rule holds a lock this one waits on".to_owned()
        } else {
            cx.door_status()
        })
    }
}

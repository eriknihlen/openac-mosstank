//! How long a death holds the macro (register MACRO-1).
//!
//! The server carries a death out in steps (ACE
//! `WorldObjects/Player_Death.cs`): the health goes to 0 at once; the death
//! animation plays and a second later the character is teleported to its
//! lifestone; some seconds after that teleport its health, stamina and mana
//! come back. Until all of that is over the character can do nothing in
//! the world, and a line sent for it meanwhile (a recall above all) has
//! left it unplaced on the server until a relog.
//!
//! The client may say the character died without its health ever showing
//! 0. Such a death holds until the death teleport has begun and ended (the
//! character has left portal space) and the health is not 0; one whose
//! health showed 0 holds until the health comes back, as before.

use ac_plugin_abi::world::PortalTransition;

/// A portal transition's kind for a teleport (not a login).
const PORTAL_TRANSITION_PORTAL: u8 = 2;

/// How long after a death the death teleport may take to begin before the
/// character is taken for lost, seconds. Not a game value: our own margin,
/// well beyond the death animation and the second after it the server
/// waits before the teleport.
pub(crate) const DEATH_TELEPORT_WAIT_SECONDS: f64 = 60.0;

/// What is said once when the death teleport never began.
pub(crate) const DEATH_TELEPORT_LOST_NOTICE: &str =
    "Death teleport never came; the character is likely lost. Holding until a relog.";

/// Where the death teleport has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Teleport {
    /// Not begun.
    Awaited,
    /// In portal space.
    UnderWay,
    /// Over: the character left portal space.
    Done,
}

/// A death being held.
#[derive(Debug, Clone)]
pub(crate) struct DeathHold {
    /// Seconds since the death.
    seconds: f64,
    teleport: Teleport,
    /// The health showed 0 during this death.
    health_showed_death: bool,
    /// The lost-character notice was said.
    lost_said: bool,
}

/// What one frame's look at a held death decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeathHoldStep {
    /// The death still holds.
    pub(crate) holds: bool,
    /// Say now that the character is likely lost.
    pub(crate) say_lost: bool,
}

impl DeathHold {
    /// A death has just happened.
    pub(crate) fn begin() -> Self {
        Self {
            seconds: 0.0,
            teleport: Teleport::Awaited,
            health_showed_death: false,
            lost_said: false,
        }
    }

    /// The client reported a portal or login transition: a teleport's
    /// reports begin and end the death teleport (a short one can begin and
    /// end between two frames, unseen in portal space).
    pub(crate) fn on_portal_transition(&mut self, transition: &PortalTransition) {
        if transition.kind != PORTAL_TRANSITION_PORTAL {
            return;
        }
        if transition.is_completed {
            self.teleport = Teleport::Done;
        } else if !transition.is_cancelled && self.teleport == Teleport::Awaited {
            self.teleport = Teleport::UnderWay;
        }
    }

    /// One frame: whether the death still holds, with the health at 0 now
    /// (`dead`) and the character in portal space now or not.
    pub(crate) fn step(
        &mut self,
        elapsed_seconds: f64,
        dead: bool,
        portal_space: bool,
    ) -> DeathHoldStep {
        self.seconds += elapsed_seconds.max(0.0);
        if dead {
            self.health_showed_death = true;
        }
        match (self.teleport, portal_space) {
            (Teleport::Awaited, true) => self.teleport = Teleport::UnderWay,
            (Teleport::UnderWay, false) => self.teleport = Teleport::Done,
            _ => {}
        }
        let holds = dead || !(self.health_showed_death || self.teleport == Teleport::Done);
        let say_lost = holds
            && !self.lost_said
            && self.teleport == Teleport::Awaited
            && self.seconds >= DEATH_TELEPORT_WAIT_SECONDS;
        if say_lost {
            self.lost_said = true;
        }
        DeathHoldStep { holds, say_lost }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn portal(completed: bool) -> PortalTransition {
        PortalTransition {
            kind: PORTAL_TRANSITION_PORTAL,
            is_completed: completed,
            ..PortalTransition::default()
        }
    }

    #[test]
    fn a_word_alone_holds_through_the_teleport_and_lets_go_after_it() {
        let mut hold = DeathHold::begin();
        assert!(hold.step(30.0, false, false).holds);
        hold.on_portal_transition(&portal(false));
        assert!(hold.step(1.0, false, true).holds);
        hold.on_portal_transition(&portal(true));
        assert!(!hold.step(0.05, false, false).holds);
    }

    #[test]
    fn a_teleport_seen_only_in_portal_space_ends_when_it_is_left() {
        let mut hold = DeathHold::begin();
        assert!(hold.step(1.0, false, true).holds);
        assert!(!hold.step(0.05, false, false).holds);
    }

    #[test]
    fn a_login_is_no_death_teleport() {
        let mut hold = DeathHold::begin();
        hold.on_portal_transition(&PortalTransition {
            kind: 1,
            is_completed: true,
            ..PortalTransition::default()
        });
        assert!(hold.step(1.0, false, false).holds);
    }

    #[test]
    fn the_health_at_0_holds_after_the_teleport() {
        let mut hold = DeathHold::begin();
        hold.on_portal_transition(&portal(true));
        assert!(hold.step(1.0, true, false).holds);
        assert!(!hold.step(1.0, false, false).holds);
    }

    #[test]
    fn the_lost_notice_comes_once_and_the_hold_stays() {
        let mut hold = DeathHold::begin();
        let first = hold.step(DEATH_TELEPORT_WAIT_SECONDS - 0.1, false, false);
        assert_eq!(
            first,
            DeathHoldStep {
                holds: true,
                say_lost: false
            }
        );
        let second = hold.step(0.1, false, false);
        assert_eq!(
            second,
            DeathHoldStep {
                holds: true,
                say_lost: true
            }
        );
        let third = hold.step(600.0, false, false);
        assert_eq!(
            third,
            DeathHoldStep {
                holds: true,
                say_lost: false
            }
        );
    }
}

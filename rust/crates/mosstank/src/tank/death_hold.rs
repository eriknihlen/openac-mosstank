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
//! 0. Such a death holds until a teleport that began after the death has
//! ended (the character has left portal space) and the health is not 0;
//! one whose health showed 0 holds until the health comes back, as before.
//! A portal trip already under way at the death, a login and a cancelled
//! teleport are not the death teleport.

use ac_plugin_abi::world::PortalTransition;

/// A portal transition's kind for a login.
const PORTAL_TRANSITION_LOGIN: u8 = 1;

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
    /// In portal space: the teleport's generation when its report named
    /// one, 0 when only portal space showed it.
    UnderWay(i64),
    /// Over: the character left portal space.
    Done,
}

/// A death being held.
#[derive(Debug, Clone)]
pub(crate) struct DeathHold {
    /// Seconds since the death.
    seconds: f64,
    teleport: Teleport,
    /// The newest transition generation reported before the death: a
    /// transition of it or an older one began before the death.
    generation_at_death: i64,
    /// The character was in portal space on the last look.
    was_portal_space: bool,
    /// The newest transition reported was a login's.
    last_report_login: bool,
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
    /// A death has just happened: the newest transition generation the
    /// client has reported, and whether the character is in portal space
    /// now (a trip that began before the death is not its teleport).
    pub(crate) fn begin(generation_at_death: i64, portal_space: bool) -> Self {
        Self {
            seconds: 0.0,
            teleport: Teleport::Awaited,
            generation_at_death,
            was_portal_space: portal_space,
            last_report_login: false,
            health_showed_death: false,
            lost_said: false,
        }
    }

    /// The client reported a portal or login transition: a teleport's
    /// reports begin and end the death teleport (a short one can begin and
    /// end between two frames, unseen in portal space).
    pub(crate) fn on_portal_transition(&mut self, transition: &PortalTransition) {
        self.last_report_login = transition.kind == PORTAL_TRANSITION_LOGIN;
        if transition.kind != PORTAL_TRANSITION_PORTAL {
            return;
        }
        let generation = transition.generation;
        if generation != 0 && generation <= self.generation_at_death {
            // A trip that began before the death.
            return;
        }
        let this_trip =
            |under_way: i64| under_way == 0 || generation == 0 || under_way == generation;
        match self.teleport {
            Teleport::Done => {}
            Teleport::UnderWay(under_way) if transition.is_cancelled && this_trip(under_way) => {
                self.teleport = Teleport::Awaited;
            }
            _ if transition.is_cancelled => {}
            _ if transition.is_completed => self.teleport = Teleport::Done,
            Teleport::Awaited => self.teleport = Teleport::UnderWay(generation),
            Teleport::UnderWay(_) => {}
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
        // Portal space entered after the death begins a teleport (a login's
        // does not); left again, it ends one. The reports above say the
        // same, and a cancelled one has already put the teleport back.
        let entered = portal_space && !self.was_portal_space;
        let left = !portal_space && self.was_portal_space;
        self.was_portal_space = portal_space;
        match self.teleport {
            Teleport::Awaited if entered && !self.last_report_login => {
                self.teleport = Teleport::UnderWay(0);
            }
            Teleport::UnderWay(_) if left => self.teleport = Teleport::Done,
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

    fn portal(generation: i64, completed: bool, cancelled: bool) -> PortalTransition {
        PortalTransition {
            generation,
            kind: PORTAL_TRANSITION_PORTAL,
            is_completed: completed,
            is_cancelled: cancelled,
            ..PortalTransition::default()
        }
    }

    fn holding(holds: bool, say_lost: bool) -> DeathHoldStep {
        DeathHoldStep { holds, say_lost }
    }

    #[test]
    fn a_word_alone_holds_through_the_teleport_and_lets_go_after_it() {
        let mut hold = DeathHold::begin(0, false);
        assert!(hold.step(30.0, false, false).holds);
        hold.on_portal_transition(&portal(1, false, false));
        assert!(hold.step(1.0, false, true).holds);
        hold.on_portal_transition(&portal(1, true, false));
        assert!(!hold.step(0.05, false, false).holds);
    }

    #[test]
    fn a_teleport_seen_only_in_portal_space_ends_when_it_is_left() {
        let mut hold = DeathHold::begin(0, false);
        assert!(hold.step(1.0, false, true).holds);
        assert!(!hold.step(0.05, false, false).holds);
    }

    #[test]
    fn a_login_is_no_death_teleport() {
        let mut hold = DeathHold::begin(0, false);
        let login = PortalTransition {
            generation: 1,
            kind: PORTAL_TRANSITION_LOGIN,
            ..PortalTransition::default()
        };
        hold.on_portal_transition(&login);
        assert!(hold.step(1.0, false, true).holds);
        hold.on_portal_transition(&PortalTransition {
            is_completed: true,
            ..login
        });
        assert!(hold.step(1.0, false, false).holds);
    }

    #[test]
    fn a_trip_under_way_at_the_death_is_not_its_teleport() {
        let mut hold = DeathHold::begin(4, true);
        assert!(hold.step(1.0, false, true).holds);
        hold.on_portal_transition(&portal(4, true, false));
        assert!(hold.step(1.0, false, false).holds, "that trip's arrival");
        hold.on_portal_transition(&portal(5, false, false));
        assert!(hold.step(1.0, false, true).holds);
        hold.on_portal_transition(&portal(5, true, false));
        assert!(!hold.step(1.0, false, false).holds, "the death teleport");
    }

    #[test]
    fn a_cancelled_teleport_is_not_the_death_teleport() {
        let mut hold = DeathHold::begin(0, false);
        hold.on_portal_transition(&portal(1, false, false));
        assert!(hold.step(1.0, false, true).holds);
        hold.on_portal_transition(&portal(1, false, true));
        assert!(hold.step(1.0, false, false).holds);
        hold.on_portal_transition(&portal(2, false, false));
        hold.on_portal_transition(&portal(2, true, false));
        assert!(!hold.step(1.0, false, false).holds);
    }

    #[test]
    fn the_health_at_0_holds_after_the_teleport() {
        let mut hold = DeathHold::begin(0, false);
        hold.on_portal_transition(&portal(1, true, false));
        assert!(hold.step(1.0, true, false).holds);
        assert!(!hold.step(1.0, false, false).holds);
    }

    #[test]
    fn the_lost_notice_comes_once_and_the_hold_stays() {
        let mut hold = DeathHold::begin(0, false);
        let first = hold.step(DEATH_TELEPORT_WAIT_SECONDS - 0.1, false, false);
        assert_eq!(first, holding(true, false));
        assert_eq!(hold.step(0.1, false, false), holding(true, true));
        assert_eq!(hold.step(600.0, false, false), holding(true, false));
    }
}

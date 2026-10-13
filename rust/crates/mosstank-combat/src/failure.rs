//! Which monsters the fight has given up on, and why (`CombatFailureTracker`):
//! a monster that keeps not being reached is blacklisted for a while, one
//! reported dead stays dead, and casts no one answers are counted towards
//! calling a monster a ghost.

use std::collections::{HashMap, HashSet};

use ac_plugin_abi::combat::CombatTarget;
use mosstank_settings::model::CombatSettings;

/// Why a monster is out of the fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatSuppressionReason {
    /// It is not.
    None,
    /// It was given up on for a while.
    Blacklisted,
    /// It died.
    Dead,
}

#[derive(Debug, Clone, Default)]
struct Entry {
    incarnation: u16,
    /// Whether the hostile capture has ever named this id; only these get
    /// a blacklist and its notice.
    is_creature: bool,
    last_seen_at: f64,
    attempts: i32,
    spell_attempts: i32,
    blacklisted_until: f64,
    is_dead: bool,
    reported_death_until: f64,
    death_health_revision: i64,
}

/// The fight's verdicts on monsters.
#[derive(Debug, Clone, Default)]
pub struct CombatFailureTracker {
    entries: HashMap<u32, Entry>,
}

impl CombatFailureTracker {
    /// An empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Folds one scan of the hostile monsters in.
    pub fn observe_targets(
        &mut self,
        targets: &[CombatTarget],
        now: f64,
        settings: &CombatSettings,
    ) {
        let mut live = HashSet::new();
        for target in targets {
            live.insert(target.object_id);
            let entry = self.entries.entry(target.object_id).or_default();
            if entry.incarnation != target.incarnation {
                *entry = Entry {
                    incarnation: target.incarnation,
                    ..Entry::default()
                };
            }
            // Seeing it in the hostile capture is what makes it a creature
            // this tracker has an opinion about.
            entry.is_creature = true;
            entry.last_seen_at = now;
            if entry.reported_death_until > 0.0
                && target.is_health_known
                && target.health_fraction > 0.0
                && target.health_revision > entry.death_health_revision
            {
                // A later health response disproves the chat-based inference.
                entry.reported_death_until = 0.0;
            }
            if entry.blacklisted_until <= now {
                entry.blacklisted_until = 0.0;
            }
        }

        let keep_for = settings.blacklist_monster_timeout_seconds.max(300.0);
        self.entries.retain(|object_id, entry| {
            live.contains(object_id) || now - entry.last_seen_at <= keep_for
        });
    }

    /// One more cast at a monster that has not answered; true on the
    /// attempt after the allowance, and the count then starts over.
    pub fn record_spell_attempt(&mut self, object_id: u32, settings: &CombatSettings) -> bool {
        if object_id == 0 {
            return false;
        }
        let entry = self.entries.entry(object_id).or_default();
        entry.spell_attempts += 1;
        if entry.spell_attempts <= settings.ghost_monster_spell_attempt_count {
            return false;
        }
        entry.spell_attempts = 0;
        true
    }

    /// The monster answered: the silence count starts over.
    pub fn reset_spell_attempts(&mut self, object_id: u32) {
        if object_id != 0
            && let Some(entry) = self.entries.get_mut(&object_id)
        {
            entry.spell_attempts = 0;
        }
    }

    /// One attempt that did not reach the monster; true on the attempt that
    /// tripped the blacklist (only a creature the pass has seen trips it).
    pub fn record_miss(&mut self, object_id: u32, now: f64, settings: &CombatSettings) -> bool {
        if object_id == 0 {
            return false;
        }
        let entry = self.entries.entry(object_id).or_default();
        entry.attempts += 1;
        if entry.attempts <= settings.blacklist_monster_attempt_count {
            return false;
        }
        entry.attempts = 0;
        if !entry.is_creature {
            return false;
        }
        extend_blacklist(entry, now, settings);
        true
    }

    /// The monster was reached: the attempt count starts over.
    pub fn reset_attempts(&mut self, object_id: u32) {
        if object_id == 0 {
            return;
        }
        if let Some(entry) = self.entries.get_mut(&object_id) {
            entry.attempts = 0;
        }
    }

    /// Gives a creature up outright.
    pub fn force_blacklist(&mut self, object_id: u32, now: f64, settings: &CombatSettings) {
        if object_id == 0 {
            return;
        }
        let entry = self.entries.entry(object_id).or_default();
        entry.attempts = 0;
        if !entry.is_creature {
            return;
        }
        extend_blacklist(entry, now, settings);
    }

    /// The monster died.
    pub fn mark_dead(&mut self, object_id: u32) {
        if object_id == 0 {
            return;
        }
        self.entries.entry(object_id).or_default().is_dead = true;
    }

    /// The chat says the monster died; a later health report can disprove
    /// it.
    pub fn report_death(&mut self, object_id: u32, now: f64, health_revision: i64) {
        if object_id == 0 {
            return;
        }
        let entry = self.entries.entry(object_id).or_default();
        entry.reported_death_until = now + 4.0;
        entry.death_health_revision = health_revision;
    }

    /// Why the monster is out, if it is.
    #[must_use]
    pub fn reason(&self, object_id: u32, now: f64) -> CombatSuppressionReason {
        let Some(entry) = self.entries.get(&object_id) else {
            return CombatSuppressionReason::None;
        };
        if entry.is_dead || entry.reported_death_until > now {
            return CombatSuppressionReason::Dead;
        }
        if entry.blacklisted_until > now {
            CombatSuppressionReason::Blacklisted
        } else {
            CombatSuppressionReason::None
        }
    }

    /// Whether the fight has ever seen this monster.
    #[must_use]
    pub fn is_known(&self, object_id: u32) -> bool {
        self.entries
            .get(&object_id)
            .is_some_and(|entry| entry.is_creature)
    }

    /// Forgets everything.
    pub fn reset(&mut self) {
        self.entries.clear();
    }
}

/// Pushes the deadline out, never in.
fn extend_blacklist(entry: &mut Entry, now: f64, settings: &CombatSettings) {
    let deadline = now + settings.blacklist_monster_timeout_seconds.max(0.0);
    if deadline > entry.blacklisted_until {
        entry.blacklisted_until = deadline;
    }
}

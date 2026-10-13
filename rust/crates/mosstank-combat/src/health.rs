//! What the macro believes about the one monster it fights
//! (`MonsterHealthTracker`): when it was picked, when its health last moved,
//! how much is left in points (only for a monster the profile's database
//! gives a ceiling for), and how much the last blow took off.

use ac_plugin_abi::combat::CombatTarget;
use mosstank_settings::monster::MonsterFactTable;

/// The estimate before anything has been observed (a real zero means dead).
const NOTHING_OBSERVED: i32 = -1;

/// The tracked monster's health.
#[derive(Debug, Clone)]
pub struct MonsterHealthTracker {
    estimate: i32,
    observed_revision: i64,
    target_object_id: u32,
    target_name: String,
    acquired_at: f64,
    last_health_change_at: Option<f64>,
    last_damage: i32,
}

impl Default for MonsterHealthTracker {
    fn default() -> Self {
        Self {
            estimate: NOTHING_OBSERVED,
            observed_revision: 0,
            target_object_id: 0,
            target_name: String::new(),
            acquired_at: 0.0,
            last_health_change_at: None,
            last_damage: 0,
        }
    }
}

impl MonsterHealthTracker {
    /// No monster.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The monster followed.
    #[must_use]
    pub fn target_object_id(&self) -> u32 {
        self.target_object_id
    }

    /// Its name.
    #[must_use]
    pub fn target_name(&self) -> &str {
        &self.target_name
    }

    /// When it was picked.
    #[must_use]
    pub fn acquired_at(&self) -> f64 {
        self.acquired_at
    }

    /// When its health last moved.
    #[must_use]
    pub fn last_health_change_at(&self) -> Option<f64> {
        self.last_health_change_at
    }

    /// The last blow, points.
    #[must_use]
    pub fn last_damage(&self) -> i32 {
        self.last_damage
    }

    /// Health left in points: 0 with no monster, no ceiling, or a monster
    /// the client forgot; `i32::MAX` for "listed, but bottomless".
    /// `is_tracked` says whether the client still knows an object.
    pub fn remaining_health(
        &self,
        facts: &MonsterFactTable,
        is_tracked: &mut dyn FnMut(u32) -> bool,
    ) -> i32 {
        if self.target_object_id == 0 || !is_tracked(self.target_object_id) {
            return 0;
        }
        if self.estimate != NOTHING_OBSERVED {
            return self.estimate;
        }
        if facts.is_listed(&self.target_name) {
            facts.maximum_health(&self.target_name)
        } else {
            0
        }
    }

    /// Points the tracker at a monster; the one it holds changes nothing.
    pub fn set_target(&mut self, object_id: u32, name: &str, now: f64) {
        if object_id == self.target_object_id {
            return;
        }
        self.target_object_id = object_id;
        name.clone_into(&mut self.target_name);
        self.estimate = NOTHING_OBSERVED;
        self.observed_revision = 0;
        self.last_damage = 0;
        self.acquired_at = now;
        self.last_health_change_at = None;
    }

    /// A fresh health report; only one about a monster with a ceiling
    /// counts.
    #[allow(clippy::cast_possible_truncation)]
    pub fn observe(&mut self, target: &CombatTarget, now: f64, facts: &MonsterFactTable) {
        if self.target_object_id == 0
            || target.object_id != self.target_object_id
            || !target.is_health_known
            || target.health_revision == 0
            || target.health_revision == self.observed_revision
        {
            return;
        }
        self.observed_revision = target.health_revision;
        if !facts.is_listed(&self.target_name) {
            return;
        }
        let ceiling = facts.maximum_health(&self.target_name);
        self.estimate = if ceiling < 0 {
            i32::MAX
        } else {
            f64::from(target.health_fraction * ceiling as f32).round_ties_even() as i32
        };
        self.last_health_change_at = Some(now);
    }

    /// One blow read from combat text, taken off the estimate.
    pub fn record_damage(&mut self, object_id: u32, points: i32) {
        if object_id == 0 || object_id != self.target_object_id || points <= 0 {
            return;
        }
        self.last_damage = points;
        if self.estimate != NOTHING_OBSERVED && self.estimate != i32::MAX {
            self.estimate -= points;
        }
    }

    /// No monster.
    pub fn clear(&mut self, now: f64) {
        self.set_target(0, "", now);
    }

    /// Forgets everything.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

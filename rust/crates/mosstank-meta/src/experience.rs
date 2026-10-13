//! The experience meter (`ExperienceMeter`): experience and luminance
//! earned since the last reset, read from the character's own properties.

use openac_plugin_compat::number;

use crate::host::MetaHost;

/// The character's total experience (int64 property 1).
const TOTAL_EXPERIENCE: u32 = 1;
/// The character's available luminance (int64 property 6).
const AVAILABLE_LUMINANCE: u32 = 6;

/// Experience and luminance earned, and over how long.
#[derive(Debug, Clone, Default)]
pub struct ExperienceMeter {
    last_experience: i64,
    last_luminance: i64,
    has_baseline: bool,
    /// Seconds measured.
    pub duration_seconds: f64,
    /// Experience earned.
    pub experience: i64,
    /// Luminance earned.
    pub luminance: i64,
}

impl ExperienceMeter {
    /// Experience per hour.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn experience_per_hour(&self) -> f64 {
        if self.duration_seconds > 0.0 {
            self.experience as f64 / self.duration_seconds * 3600.0
        } else {
            0.0
        }
    }

    /// Luminance per hour.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn luminance_per_hour(&self) -> f64 {
        if self.duration_seconds > 0.0 {
            self.luminance as f64 / self.duration_seconds * 3600.0
        } else {
            0.0
        }
    }

    /// One tick: what changed since the last reading counts.
    pub fn on_tick(&mut self, host: &mut dyn MetaHost, elapsed_seconds: f64) {
        let properties = match host.character() {
            Some(character) => host.properties(character.id),
            None => None,
        };
        let Some(properties) = properties else {
            self.has_baseline = false;
            return;
        };
        let read = |key: u32| {
            properties
                .int64s
                .iter()
                .find(|(id, _)| *id == key)
                .map_or(0, |(_, value)| *value)
        };
        let experience = read(TOTAL_EXPERIENCE);
        let luminance = read(AVAILABLE_LUMINANCE);
        if self.has_baseline {
            if experience >= self.last_experience {
                self.experience += experience - self.last_experience;
            }
            if luminance >= self.last_luminance {
                self.luminance += luminance - self.last_luminance;
            }
        } else {
            self.has_baseline = true;
        }
        self.last_experience = experience;
        self.last_luminance = luminance;
        self.duration_seconds += elapsed_seconds;
    }

    /// Starts over.
    pub fn reset(&mut self) {
        self.duration_seconds = 0.0;
        self.experience = 0;
        self.luminance = 0;
        self.has_baseline = false;
    }

    /// The meter as `xpmeter[]` prints it.
    #[must_use]
    pub fn format(&self) -> String {
        let n0 = |value: f64| number::format_f64(value, "N0").unwrap_or_default();
        let whole = |value: i64| number::format_i64(value, "N0").unwrap_or_default();
        let mut result = format!("{} XP", whole(self.experience));
        if self.luminance != 0 {
            result += &format!(" and {} LUM", whole(self.luminance));
        }
        result += &format!(
            ", {}s, {} XP/hr",
            n0(self.duration_seconds),
            n0(self.experience_per_hour())
        );
        if self.luminance != 0 {
            result += &format!(" and {} LUM/hr", n0(self.luminance_per_hour()));
        }
        result
    }
}

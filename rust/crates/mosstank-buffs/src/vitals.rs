//! The vital plan: which vital needs recharging, the mana-from-stamina
//! decision, and the strongest castable spell by name stem (`VitalPlan`).

use std::collections::HashMap;

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::magic::SpellInfo;
use mosstank_dotnet::text::ordinal;
use mosstank_settings::model::{VitalKind, VitalSettings};

/// The heal spell's name stem.
pub const HEAL_SELF_STEM: &str = "Heal Self";
/// The stamina-to-mana spell's name stem.
pub const STAMINA_TO_MANA_STEM: &str = "Stamina to Mana";
/// The stamina spell's name stem.
pub const REVITALIZE_STEM: &str = "Revitalize";

/// The three vitals, now and at full.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Vitals {
    /// Health now and at full.
    pub health: (u32, u32),
    /// Stamina now and at full.
    pub stamina: (u32, u32),
    /// Mana now and at full.
    pub mana: (u32, u32),
}

impl Vitals {
    /// One vital, now and at full.
    #[must_use]
    pub fn read(&self, vital: VitalKind) -> (u32, u32) {
        match vital {
            VitalKind::Health => self.health,
            VitalKind::Stamina => self.stamina,
            VitalKind::Mana => self.mana,
        }
    }
}

impl From<&CharacterInfo> for Vitals {
    fn from(character: &CharacterInfo) -> Self {
        Self {
            health: (character.health.0, character.health.1),
            stamina: (character.stamina.0, character.stamina.1),
            mana: (character.mana.0, character.mana.1),
        }
    }
}

/// What the mana upkeep would do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VitalAction {
    /// Nothing.
    #[default]
    None,
    /// Turn stamina into mana.
    StaminaToMana,
    /// Restore stamina first.
    Revitalize,
}

/// The first vital (health, stamina, mana) below its threshold, the normal
/// or the no-target one; an adjustment pretends that much less of a vital
/// is there (a recharge boost).
#[must_use]
pub fn decide_need(
    vitals: &Vitals,
    settings: &VitalSettings,
    no_target: bool,
    adjustments: (i32, i32, i32),
) -> Option<VitalKind> {
    if !settings.enabled {
        return None;
    }
    for (vital, adjustment) in [
        (VitalKind::Health, adjustments.0),
        (VitalKind::Stamina, adjustments.1),
        (VitalKind::Mana, adjustments.2),
    ] {
        let (mut current, maximum) = vitals.read(vital);
        if maximum == 0 {
            continue;
        }
        if adjustment > 0 {
            current =
                u32::try_from((i64::from(current) - i64::from(adjustment)).max(0)).unwrap_or(0);
        }
        if f64::from(current) / f64::from(maximum) < settings.threshold(vital, no_target) {
            return Some(vital);
        }
    }
    None
}

/// Whether a vital is below its normal threshold.
#[must_use]
pub fn is_below_normal(vitals: &Vitals, settings: &VitalSettings, vital: VitalKind) -> bool {
    let (current, maximum) = vitals.read(vital);
    maximum != 0 && f64::from(current) / f64::from(maximum) < settings.normal_threshold(vital)
}

/// A vital's whole percentage (100 when its maximum is unknown).
#[must_use]
pub fn percent(vitals: &Vitals, vital: VitalKind) -> i32 {
    let (current, maximum) = vitals.read(vital);
    if maximum == 0 {
        return 100;
    }
    // `100u * current / maximum` in 32-bit arithmetic, as the C# computes it.
    i32::try_from(100u32.wrapping_mul(current) / maximum).unwrap_or(i32::MAX)
}

/// The mana upkeep's choice: nothing while mana is at its normal threshold,
/// else stamina to mana while stamina is above its own, else revitalize.
#[must_use]
pub fn decide(vitals: &Vitals, settings: &VitalSettings) -> VitalAction {
    if !settings.enabled || vitals.mana.1 == 0 || vitals.stamina.1 == 0 {
        return VitalAction::None;
    }
    let mana = f64::from(vitals.mana.0) / f64::from(vitals.mana.1);
    if mana >= settings.normal_mana {
        return VitalAction::None;
    }
    let stamina = f64::from(vitals.stamina.0) / f64::from(vitals.stamina.1);
    if stamina > settings.normal_stamina {
        VitalAction::StaminaToMana
    } else {
        VitalAction::Revitalize
    }
}

/// The strongest known spell whose name holds `stem` (ignoring case) that
/// the casting skill carries with `skill_excess`: the highest quality, then
/// tier.
#[must_use]
pub fn try_find(
    known: &[SpellInfo],
    stem: &str,
    skill_levels: &HashMap<u32, u32>,
    skill_excess: i32,
) -> Option<SpellInfo> {
    let mut pick: Option<&SpellInfo> = None;
    for spell in known {
        if !ordinal::contains_ignore_case(&spell.name, stem) {
            continue;
        }
        if spell.school != 0
            && let Some(&level) = skill_levels.get(&spell.school)
            && i64::from(level) < i64::from(spell.difficulty) + i64::from(skill_excess)
        {
            continue;
        }
        let better = match pick {
            None => true,
            Some(current) => {
                spell.quality() > current.quality()
                    || (spell.quality() == current.quality() && spell.tier > current.tier)
            }
        };
        if better {
            pick = Some(spell);
        }
    }
    pick.cloned()
}

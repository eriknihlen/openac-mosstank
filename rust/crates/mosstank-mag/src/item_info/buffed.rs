//! The buffed block (`ItemInfo.cs:116-143`): a weapon's or caster's values
//! with its own spells counted and the spells active on it taken off, as
//! Mag-Tools' `MyWorldObject` works them out (`Shared/MyWorldObject.cs`,
//! its quirks kept), with the spell effect tables of
//! `Shared/Constants/Dictionaries.cs` (43ee2ec).

use ac_plugin_abi::world::ObjectClass;

use super::spells::SpellLists;
use std::fmt::Write as _;

use super::{Values, key, put};
use crate::fmt;

/// One spell's effect on a key: what it adds when it is on the item
/// (`Bonus`) and what it added when active on the item (`Change`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct SpellEffect<T> {
    /// The spell.
    pub(crate) spell: u32,
    /// The Decal key it changes.
    pub(crate) key: u32,
    /// `Change`.
    pub(crate) change: T,
    /// `Bonus` (0 when the source gives none).
    pub(crate) bonus: T,
}

const fn int_effect(spell: u32, key: u32, change: i32, bonus: i32) -> SpellEffect<i32> {
    SpellEffect {
        spell,
        key,
        change,
        bonus,
    }
}

const fn double_effect(spell: u32, key: u32, change: f64, bonus: f64) -> SpellEffect<f64> {
    SpellEffect {
        spell,
        key,
        change,
        bonus,
    }
}

/// `LongValueKeySpellEffects`, `Dictionaries.cs:345-371` (16 entries).
pub(crate) const LONG_SPELL_EFFECTS: [SpellEffect<i32>; 16] = [
    int_effect(1616, key::MAX_DAMAGE, 20, 0), // Blood Drinker VI
    int_effect(2096, key::MAX_DAMAGE, 22, 0), // Infected Caress
    int_effect(5183, key::MAX_DAMAGE, 24, 0), // Incantation of Blood Drinker Post Feb-2013
    int_effect(4395, key::MAX_DAMAGE, 24, 0), // Incantation of Blood Drinker Post Feb-2013
    int_effect(2598, key::MAX_DAMAGE, 2, 2),  // Minor Blood Thirst
    int_effect(2586, key::MAX_DAMAGE, 4, 4),  // Major Blood Thirst
    int_effect(4661, key::MAX_DAMAGE, 7, 7),  // Epic Blood Thirst
    int_effect(6089, key::MAX_DAMAGE, 10, 10), // Legendary Blood Thirst
    int_effect(3688, key::MAX_DAMAGE, 300, 0), // Prodigal Blood Drinker
    int_effect(1486, key::ARMOR_LEVEL, 200, 0), // Impenetrability VI
    int_effect(2108, key::ARMOR_LEVEL, 220, 0), // Brogard's Defiance
    int_effect(4407, key::ARMOR_LEVEL, 240, 0), // Incantation of Impenetrability
    int_effect(2604, key::ARMOR_LEVEL, 20, 20), // Minor Impenetrability
    int_effect(2592, key::ARMOR_LEVEL, 40, 40), // Major Impenetrability
    int_effect(4667, key::ARMOR_LEVEL, 60, 60), // Epic Impenetrability
    int_effect(6095, key::ARMOR_LEVEL, 80, 80), // Legendary Impenetrability
];

const VS_MONSTERS: u32 = key::ELEMENTAL_VS_MONSTERS;
const ATTACK: u32 = key::ATTACK_BONUS;
const MELEE_D: u32 = key::MELEE_DEFENSE_BONUS;
const MANA_C: u32 = key::MANA_C_BONUS;

/// `DoubleValueKeySpellEffects`, `Dictionaries.cs:379-434` (34 entries).
pub(crate) const DOUBLE_SPELL_EFFECTS: [SpellEffect<f64>; 34] = [
    double_effect(3258, VS_MONSTERS, 0.06, 0.0), // Spirit Drinker VI
    double_effect(3259, VS_MONSTERS, 0.07, 0.0), // Infected Spirit Caress
    double_effect(5182, VS_MONSTERS, 0.08, 0.0), // Incantation of Spirit Drinker Post Feb-2013
    double_effect(4414, VS_MONSTERS, 0.08, 0.0), // Incantation of Spirit Drinker Post Feb-2013
    double_effect(3251, VS_MONSTERS, 0.01, 0.01), // Minor Spirit Thirst
    double_effect(3250, VS_MONSTERS, 0.03, 0.03), // Major Spirit Thirst
    double_effect(4670, VS_MONSTERS, 0.05, 0.05), // Epic Spirit Thirst
    double_effect(6098, VS_MONSTERS, 0.07, 0.07), // Legendary Spirit Thirst
    double_effect(3735, VS_MONSTERS, 0.15, 0.0), // Prodigal Spirit Drinker
    double_effect(1592, ATTACK, 0.15, 0.0),      // Heart Seeker VI
    double_effect(2106, ATTACK, 0.17, 0.0),      // Elysa's Sight
    double_effect(4405, ATTACK, 0.20, 0.0),      // Incantation of Heart Seeker
    double_effect(2603, ATTACK, 0.03, 0.03),     // Minor Heart Thirst
    double_effect(2591, ATTACK, 0.05, 0.05),     // Major Heart Thirst
    double_effect(4666, ATTACK, 0.07, 0.07),     // Epic Heart Thirst
    double_effect(6094, ATTACK, 0.09, 0.09),     // Legendary Heart Thirst
    double_effect(1605, MELEE_D, 0.15, 0.0),     // Defender VI
    double_effect(2101, MELEE_D, 0.17, 0.0),     // Cragstone's Will
    double_effect(4400, MELEE_D, 0.20, 0.0),     // Incantation of Defender Post Feb-2013
    double_effect(2600, MELEE_D, 0.03, 0.03),    // Minor Defender
    double_effect(3985, MELEE_D, 0.04, 0.04),    // Mukkir Sense
    double_effect(2588, MELEE_D, 0.05, 0.05),    // Major Defender
    double_effect(4663, MELEE_D, 0.07, 0.07),    // Epic Defender
    double_effect(6091, MELEE_D, 0.09, 0.09),    // Legendary Defender
    double_effect(3699, MELEE_D, 0.25, 0.0),     // Prodigal Defender
    double_effect(1480, MANA_C, 1.60, 0.0),      // Hermetic Link VI
    double_effect(2117, MANA_C, 1.70, 0.0),      // Mystic's Blessing
    double_effect(4418, MANA_C, 1.80, 0.0),      // Incantation of Hermetic Link
    double_effect(3201, MANA_C, 1.05, 1.05),     // Feeble Hermetic Link
    double_effect(3199, MANA_C, 1.10, 1.10),     // Minor Hermetic Link
    double_effect(3202, MANA_C, 1.15, 1.15),     // Moderate Hermetic Link
    double_effect(3200, MANA_C, 1.20, 1.20),     // Major Hermetic Link
    double_effect(6086, MANA_C, 1.25, 1.25),     // Epic Hermetic Link
    double_effect(6087, MANA_C, 1.30, 1.30),     // Legendary Hermetic Link
];

/// .NET's `Double.Epsilon`: the smallest positive double (so `|x| <
/// Double.Epsilon` holds only for zero).
const DOUBLE_EPSILON: f64 = f64::from_bits(1);

/// The table's effect of `spell` when it changes `key`.
fn effect<T: Copy>(table: &[SpellEffect<T>], spell: u32, key: u32) -> Option<SpellEffect<T>> {
    table
        .iter()
        .find(|e| e.spell == spell)
        .filter(|e| e.key == key)
        .copied()
}

/// `GetBuffedIntValueKey(key, default)`.
pub(crate) fn buffed_int(v: &Values<'_>, lists: &SpellLists, key: u32, default: i32) -> i32 {
    let Some(mut value) = v.long_opt(key) else {
        return default;
    };
    for &spell in &lists.active {
        if let Some(e) = effect(&LONG_SPELL_EFFECTS, spell, key) {
            value = value.wrapping_sub(e.change);
        }
    }
    for &spell in &lists.spells {
        if let Some(e) = effect(&LONG_SPELL_EFFECTS, spell, key) {
            value = value.wrapping_add(e.bonus);
        }
    }
    value
}

/// `GetBuffedDoubleValueKey(key, default)`.
pub(crate) fn buffed_double(v: &Values<'_>, lists: &SpellLists, key: u32, default: f64) -> f64 {
    let Some(mut value) = v.double_opt(key) else {
        return default;
    };
    for &spell in &lists.active {
        if let Some(e) = effect(&DOUBLE_SPELL_EFFECTS, spell, key) {
            if (e.change - 1.0).abs() < DOUBLE_EPSILON {
                value /= e.change;
            } else {
                value -= e.change;
            }
        }
    }
    for &spell in &lists.spells {
        if let Some(e) = effect(&DOUBLE_SPELL_EFFECTS, spell, key)
            && (e.bonus - 0.0).abs() > DOUBLE_EPSILON
        {
            if (e.change - 1.0).abs() < DOUBLE_EPSILON {
                value *= e.bonus;
            } else {
                value += e.bonus;
            }
        }
    }
    value
}

/// `CalculateDamageOverTime(maxDamage, variance)`, a critical chance of
/// .1 and a multiplier of 2.
fn damage_over_time(max_damage: i32, variance: f64) -> f64 {
    let crit_chance = 0.1;
    let crit_multiplier = 2.0;
    f64::from(max_damage)
        * ((1.0 - crit_chance) * (2.0 - variance) / 2.0 + (crit_chance * crit_multiplier))
}

/// `CalcedBuffedTinkedDoT`: the damage over time once the tinks left are
/// spent best (each an iron or a granite), -1 without a weapon profile.
fn calced_buffed_tinked_dot(v: &Values<'_>, lists: &SpellLists) -> f64 {
    let (Some(mut variance), true) = (v.double_opt(key::VARIANCE), v.has_long(key::MAX_DAMAGE))
    else {
        return -1.0;
    };
    let mut max_damage = buffed_int(v, lists, key::MAX_DAMAGE, 0);
    let tinks = v.long_opt(key::TINKS).unwrap_or(-1);
    let mut left = (10 - tinks.max(0)).max(0);
    // Factor in an imbue tink.
    if v.long_opt(key::IMBUED).unwrap_or(0) == 0 {
        left -= 1;
    }
    // An item not loot generated cannot be tinked.
    if v.long_opt(key::MATERIAL).unwrap_or(0) == 0 {
        left = 0;
    }
    for _ in 1..=left {
        let iron = damage_over_time(max_damage.wrapping_add(24).wrapping_add(1), variance);
        let granite = damage_over_time(max_damage.wrapping_add(24), variance * 0.8);
        if iron >= granite {
            max_damage = max_damage.wrapping_add(1);
        } else {
            variance *= 0.8;
        }
    }
    damage_over_time(max_damage.wrapping_add(24), variance)
}

/// `CalcedBuffedMissileDamage`: -1 without the damage, its modifier or the
/// elemental bonus.
fn calced_buffed_missile_damage(v: &Values<'_>, lists: &SpellLists) -> f64 {
    if !v.has_long(key::MAX_DAMAGE)
        || v.double_opt(key::DAMAGE_BONUS).is_none()
        || !v.has_long(key::ELEMENTAL_DMG_BONUS)
    {
        return -1.0;
    }
    f64::from(buffed_int(v, lists, key::MAX_DAMAGE, 0))
        + ((buffed_double(v, lists, key::DAMAGE_BONUS, 0.0) - 1.0) * 100.0) / 3.0
        + f64::from(buffed_int(v, lists, key::ELEMENTAL_DMG_BONUS, 0))
}

/// Append the block: `", (" + damage + " " + attack/defence/mana + ")"`.
pub(crate) fn append(sb: &mut String, v: &Values<'_>, lists: &SpellLists, class: ObjectClass) {
    sb.push_str(", (");

    // (Damage)
    match class {
        ObjectClass::MeleeWeapon => {
            sb.push_str(&fmt::n1(calced_buffed_tinked_dot(v, lists)));
            put!(sb, "/{}", buffed_int(v, lists, key::MAX_DAMAGE, 0));
        }
        ObjectClass::MissileWeapon => {
            sb.push_str(&fmt::n1(calced_buffed_missile_damage(v, lists)));
        }
        ObjectClass::WandStaffOrb => {
            let vs = buffed_double(v, lists, key::ELEMENTAL_VS_MONSTERS, 0.0);
            sb.push_str(&fmt::n1((vs - 1.0) * 100.0));
        }
        _ => {}
    }

    // (AttackBonus/MeleeDefenseBonus/ManaCBonus)
    sb.push(' ');
    if v.double(key::ATTACK_BONUS, 1.0) != 1.0 {
        let attack = buffed_double(v, lists, key::ATTACK_BONUS, 0.0);
        sb.push_str(&fmt::n1(fmt::round((attack - 1.0) * 100.0, 0)));
        sb.push('/');
    }
    if v.double(key::MELEE_DEFENSE_BONUS, 1.0) != 1.0 {
        let defense = buffed_double(v, lists, key::MELEE_DEFENSE_BONUS, 0.0);
        sb.push_str(&fmt::n1(fmt::round((defense - 1.0) * 100.0, 0)));
    }
    if v.double(key::MANA_C_BONUS, 0.0) != 0.0 {
        let mana_c = buffed_double(v, lists, key::MANA_C_BONUS, 0.0);
        sb.push('/');
        sb.push_str(&fmt::g15(fmt::round(mana_c * 100.0, 0)));
    }

    sb.push(')');
}

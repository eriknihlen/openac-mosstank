//! The power bar the automatic attack power sets (`AutoAttackPower`): a
//! slash-and-pierce weapon picks its element with the bar, the rest swing
//! at full, and Recklessness keeps the bar inside its band.

use ac_plugin_abi::character::{Skill, Training};
use ac_plugin_abi::items::InventoryItem;
use mosstank_data::attack_spells::find_skill;
use mosstank_data::damage::MonsterDamageType;
use mosstank_settings::model::CombatSettings;
use mosstank_settings::monster::MonsterRuleActions;

const MELEE_WEAPON: u32 = 0x0000_0001;
const MISSILE_WEAPON: u32 = 0x0000_0100;
const SHIELD_LOCATION: u32 = 0x0020_0000;
const SLASH_DAMAGE: i32 = 0x0001;
const PIERCE_DAMAGE: i32 = 0x0002;
const SLASH_AND_PIERCE: i32 = SLASH_DAMAGE | PIERCE_DAMAGE;
const TRIPLE_SLASH_ATTACK: i32 = 0x0040;
const UNARMED_WEAPON_TYPE: i32 = 1;
const RECKLESSNESS_SKILL: u32 = 50;

/// The bar for this swing; `None` with the automatic power off (the
/// player's own bar stands).
#[must_use]
pub fn resolve(
    actions: &MonsterRuleActions,
    element: MonsterDamageType,
    settings: &CombatSettings,
    skills: &[Skill],
    inventory: &[InventoryItem],
) -> Option<f32> {
    if !settings.auto_attack_power {
        return None;
    }
    let mut power = 1.0_f32;
    if let Some(selected) = find_weapon(actions, inventory)
        && selected.item_type & MELEE_WEAPON != 0
    {
        let offhand = find_offhand(actions, selected, inventory);
        let offhand_melee = offhand.is_some_and(|held| held.item_type & MELEE_WEAPON != 0);
        let offhand_shield =
            offhand.is_some_and(|shield| shield.valid_locations == SHIELD_LOCATION);
        // A slash-and-pierce weapon: the bar chooses the element.
        let dual_edged = selected.damage_type & SLASH_AND_PIERCE == SLASH_AND_PIERCE;
        let triple_slash = selected.attack_type & TRIPLE_SLASH_ATTACK != 0;
        let pierce = element == MonsterDamageType::Pierce;
        power = if selected.weapon_type == UNARMED_WEAPON_TYPE && !offhand_melee {
            if element != MonsterDamageType::Slash || !dual_edged {
                0.0
            } else {
                0.5
            }
        } else if pierce && dual_edged && !triple_slash {
            0.2
        } else if pierce && dual_edged && triple_slash && offhand_melee {
            0.49
        } else if !pierce || !dual_edged || !triple_slash || offhand_shield {
            1.0
        } else {
            0.2
        };
    }
    Some(clamp_for_recklessness(power, settings, skills))
}

fn find_weapon<'i>(
    actions: &MonsterRuleActions,
    inventory: &'i [InventoryItem],
) -> Option<&'i InventoryItem> {
    let mut equipped = None;
    let mut named = None;
    for item in inventory {
        if actions.weapon_object_id != 0 && item.object_id == actions.weapon_object_id {
            return Some(item);
        }
        if !actions.weapon_name.trim().is_empty() && item.name == actions.weapon_name {
            named.get_or_insert(item);
        }
        if item.equipped_location != 0 && item.item_type & (MELEE_WEAPON | MISSILE_WEAPON) != 0 {
            equipped.get_or_insert(item);
        }
    }
    named.or(equipped)
}

fn find_offhand<'i>(
    actions: &MonsterRuleActions,
    weapon: &InventoryItem,
    inventory: &'i [InventoryItem],
) -> Option<&'i InventoryItem> {
    let mut equipped = None;
    let mut named = None;
    for item in inventory {
        if item.object_id == weapon.object_id {
            continue;
        }
        if actions.offhand_object_id != 0 && item.object_id == actions.offhand_object_id {
            return Some(item);
        }
        if !actions.offhand_name.trim().is_empty() && item.name == actions.offhand_name {
            named.get_or_insert(item);
        }
        if item.equipped_location != 0
            && (item.item_type & MELEE_WEAPON != 0 || item.valid_locations == SHIELD_LOCATION)
        {
            equipped.get_or_insert(item);
        }
    }
    named.or(equipped)
}

fn clamp_for_recklessness(power: f32, settings: &CombatSettings, skills: &[Skill]) -> f32 {
    let trained = find_skill(skills, RECKLESSNESS_SKILL)
        .is_some_and(|skill| matches!(skill.training, Training::Trained | Training::Specialized));
    if !settings.use_recklessness || !trained {
        return power;
    }
    power.clamp(0.11, 0.9)
}

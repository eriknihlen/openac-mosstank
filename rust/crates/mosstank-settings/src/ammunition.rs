//! Which arrows, quarrels or darts a launcher is fed: the best row of the
//! game database's `AmmunitionOptions` table for the launcher and element
//! that the character can wield and has (or can make).

use ac_plugin_abi::character::{Skill, Training};
use mosstank_data::attack_spells::find_skill;
use mosstank_data::damage::MonsterDamageType;
use mosstank_formats::gameinfo::VtankAmmunitionOption;

/// The missile weapons skill.
const MISSILE_WEAPONS_SKILL: u32 = 47;
/// The table's prismatic element, and its old spelling.
const PRISMATIC: i32 = 11;
const PRISMATIC_OLD: i32 = 100;

/// What prismatic ammunition is to the choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VtankPrismaticAmmoPolicy {
    /// Prismatic stands in for any element it covers.
    Any,
    /// Prismatic only when nothing else will do.
    NoPrismatic,
    /// Prismatic whenever there is any.
    ForcePrismatic,
}

/// The table's element number for a damage type (-1 for one it has none
/// for).
fn element(damage: MonsterDamageType) -> i32 {
    match damage {
        MonsterDamageType::Pierce => 0,
        MonsterDamageType::Bludgeon => 1,
        MonsterDamageType::Slash => 2,
        MonsterDamageType::Acid => 3,
        MonsterDamageType::Electric => 4,
        MonsterDamageType::Cold => 5,
        MonsterDamageType::Fire => 6,
        MonsterDamageType::Harm => 7,
        MonsterDamageType::Auto => 8,
        MonsterDamageType::VoidBasic => 9,
        MonsterDamageType::DrainAuto => 10,
        MonsterDamageType::Prismatic => 11,
        MonsterDamageType::Random => 12,
        MonsterDamageType::Fists => 13,
        MonsterDamageType::None => 98,
        MonsterDamageType::Physical => 99,
        MonsterDamageType::PlayerAuto => 101,
        MonsterDamageType::Nether => -1,
    }
}

/// The best option for this launcher type and element, or `None` (also
/// with no launcher, or no table). Prismatic rows stand in for any element;
/// the policy moves them a thousand points up or down. Ties go to the later
/// row.
#[must_use]
pub fn select<'a>(
    options: &'a [VtankAmmunitionOption],
    launcher_type: i32,
    damage: MonsterDamageType,
    prismatic: VtankPrismaticAmmoPolicy,
    enabled_special_mask: i32,
    skills: &[Skill],
    is_available: &dyn Fn(&str) -> bool,
) -> Option<&'a VtankAmmunitionOption> {
    let desired = element(damage);
    if launcher_type == 0 {
        return None;
    }
    let mut best = None;
    let mut best_quality = i32::MIN;
    for option in options {
        if option.launcher_type != launcher_type {
            continue;
        }
        let option_element = if option.element == PRISMATIC_OLD {
            PRISMATIC
        } else {
            option.element
        };
        let mut quality = option.quality;
        if prismatic == VtankPrismaticAmmoPolicy::ForcePrismatic && option_element != PRISMATIC {
            quality = quality.wrapping_sub(1000);
        }
        if option_element != desired {
            if option_element != PRISMATIC {
                continue;
            }
            if prismatic == VtankPrismaticAmmoPolicy::NoPrismatic {
                quality = quality.wrapping_sub(1000);
            }
        }
        if quality < best_quality
            || !meets_requirements(option, skills)
            || (option.special_mask != 0 && option.special_mask & enabled_special_mask == 0)
            || !is_available(&option.name)
        {
            continue;
        }
        best_quality = quality;
        best = Some(option);
    }
    best
}

fn meets_requirements(option: &VtankAmmunitionOption, skills: &[Skill]) -> bool {
    if option.wield_requirement > 0 {
        let Some(missile) = find_skill(skills, MISSILE_WEAPONS_SKILL) else {
            return false;
        };
        if matches!(missile.training, Training::Unknown | Training::Untrained)
            || i64::from(missile.base) < i64::from(option.wield_requirement)
        {
            return false;
        }
    }
    if option.secondary_skill == 0 || option.secondary_requirement == 0 {
        return true;
    }
    find_skill(skills, option.secondary_skill).is_some_and(|secondary| {
        secondary.training != Training::Untrained
            && i64::from(secondary.current) >= i64::from(option.secondary_requirement)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(name: &str, element: i32, quality: i32, wield: i32) -> VtankAmmunitionOption {
        VtankAmmunitionOption {
            name: name.to_owned(),
            launcher_type: 5,
            wield_requirement: wield,
            element,
            quality,
            special_mask: 0,
            secondary_skill: 0,
            secondary_requirement: 0,
        }
    }

    fn pick<'a>(
        options: &'a [VtankAmmunitionOption],
        damage: MonsterDamageType,
        policy: VtankPrismaticAmmoPolicy,
        skills: &[Skill],
    ) -> Option<&'a str> {
        select(options, 5, damage, policy, 0, skills, &|_| true).map(|o| o.name.as_str())
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankAmmunitionDatabaseTests.cs :: CurrentAndLegacyPrismaticRowsRemainCandidatesForHarm
    #[test]
    fn current_and_legacy_prismatic_rows_remain_candidates_for_harm() {
        let options = [
            option("Current Prism", 11, 300, 0),
            option("Legacy Prism", 100, 301, 0),
        ];
        let any = VtankPrismaticAmmoPolicy::Any;
        assert_eq!(
            pick(&options[..1], MonsterDamageType::Harm, any, &[]),
            Some("Current Prism")
        );
        assert_eq!(
            pick(&options, MonsterDamageType::Harm, any, &[]),
            Some("Legacy Prism")
        );
        assert_eq!(options[1].element, 100);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankAmmunitionDatabaseTests.cs :: PrimaryRequirementRejectsUnknownTraining
    #[test]
    fn primary_requirement_rejects_unknown_training() {
        let skills = [Skill {
            skill_id: 47,
            name: "Missile Weapons".to_owned(),
            training: Training::Unknown,
            current: 500,
            base: 500,
            ..Skill::default()
        }];
        let options = [option("Trained Arrow", 6, 100, 300)];
        assert_eq!(
            pick(
                &options,
                MonsterDamageType::Fire,
                VtankPrismaticAmmoPolicy::NoPrismatic,
                &skills
            ),
            None
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankAmmunitionDatabaseTests.cs :: RequestedPrismaticElementIsDistinctFromItsPolicy
    #[test]
    fn requested_prismatic_element_is_distinct_from_its_policy() {
        let options = [option("Prism", 11, 200, 0), option("Pierce", 0, 2000, 0)];
        assert_eq!(
            pick(
                &options,
                MonsterDamageType::Prismatic,
                VtankPrismaticAmmoPolicy::ForcePrismatic,
                &[]
            ),
            Some("Prism")
        );
    }
}

//! The ammunition choice against the game database excerpt fixture's
//! `AmmunitionOptions` rows (all written for the tests).

use ac_plugin_abi::character::{Skill, Training};
use mosstank_data::damage::MonsterDamageType;
use mosstank_formats::fixtures;
use mosstank_formats::gameinfo::{VtankAmmunitionOption, VtankGameInfoDatabase};
use mosstank_settings::ammunition::{VtankPrismaticAmmoPolicy, select};

fn fixture_options() -> Vec<VtankAmmunitionOption> {
    VtankGameInfoDatabase::parse(&fixtures::read_vtank("gameinfodb-excerpt.ugd"))
        .unwrap()
        .ammunition_options
}

fn skill(id: u32, name: &str, level: u32, base: u32) -> Skill {
    Skill {
        skill_id: id,
        name: name.to_owned(),
        training: Training::Trained,
        current: level,
        base,
        ..Skill::default()
    }
}

fn pick(
    options: &[VtankAmmunitionOption],
    damage: MonsterDamageType,
    policy: VtankPrismaticAmmoPolicy,
    special_mask: i32,
    skills: &[Skill],
) -> String {
    select(options, 5, damage, policy, special_mask, skills, &|_| true)
        .unwrap()
        .name
        .clone()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankAmmunitionDatabaseTests.cs :: SpecialMaskSelectsRaiderOnlyWhenEnabled
#[test]
fn special_mask_selects_raider_only_when_enabled() {
    let options = fixture_options();
    let skills = [skill(47, "Missile Weapons", 300, 300)];
    let no_prismatic = VtankPrismaticAmmoPolicy::NoPrismatic;
    assert_eq!(
        pick(
            &options,
            MonsterDamageType::Electric,
            no_prismatic,
            0,
            &skills
        ),
        "Fixture Lightning Arrow"
    );
    assert_eq!(
        pick(
            &options,
            MonsterDamageType::Electric,
            no_prismatic,
            1,
            &skills
        ),
        "Fixture Raider Lightning Arrow"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankAmmunitionDatabaseTests.cs :: PrismaticAmmoRequiresBothOfficialSkills
#[test]
fn prismatic_ammo_requires_both_official_skills() {
    let options = fixture_options();
    let missile_only = [skill(47, "Missile Weapons", 400, 400)];
    let both = [
        skill(47, "Missile Weapons", 400, 400),
        skill(37, "Fletching", 400, 0),
    ];
    let force = VtankPrismaticAmmoPolicy::ForcePrismatic;
    assert_eq!(
        pick(&options, MonsterDamageType::Fire, force, 0, &missile_only),
        "Fixture Fire Arrow"
    );
    assert_eq!(
        pick(&options, MonsterDamageType::Fire, force, 0, &both),
        "Fixture Prismatic Arrow"
    );
}

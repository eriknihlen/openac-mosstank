//! The vital plan. Expected values are the C# tests' (`VitalPlanTests.cs`).

use std::collections::HashMap;

use ac_plugin_abi::magic::SpellInfo;
use mosstank_buffs::vitals::{self, STAMINA_TO_MANA_STEM, VitalAction, Vitals};
use mosstank_settings::model::VitalSettings;

fn character(mana: u32, stamina: u32) -> Vitals {
    Vitals {
        health: (0, 100),
        stamina: (stamina, 100),
        mana: (mana, 100),
    }
}

fn spell(id: u32, name: &str, tier: i32, difficulty: i32) -> SpellInfo {
    SpellInfo {
        spell_id: id,
        name: name.to_owned(),
        family: 89,
        tier,
        difficulty,
        school: 33,
        is_self_targeted: true,
        is_beneficial: true,
        ..SpellInfo::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalPlanTests.cs :: ConvertsStaminaToManaWhenManaIsLowAndStaminaIsAvailable
#[test]
fn converts_stamina_to_mana_when_mana_is_low_and_stamina_is_available() {
    assert_eq!(
        vitals::decide(&character(10, 90), &VitalSettings::default()),
        VitalAction::StaminaToMana
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalPlanTests.cs :: RevitalizesWhenBothManaAndStaminaAreLow
#[test]
fn revitalizes_when_both_mana_and_stamina_are_low() {
    assert_eq!(
        vitals::decide(&character(10, 10), &VitalSettings::default()),
        VitalAction::Revitalize
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalPlanTests.cs :: DoesNothingWhenManaIsHealthy
#[test]
fn does_nothing_when_mana_is_healthy() {
    assert_eq!(
        vitals::decide(&character(95, 20), &VitalSettings::default()),
        VitalAction::None
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalPlanTests.cs :: DoesNothingWhenVitalsAreUnknown
#[test]
fn does_nothing_when_vitals_are_unknown() {
    let unknown = Vitals {
        health: (0, 100),
        stamina: (0, 0),
        mana: (0, 0),
    };
    assert_eq!(
        vitals::decide(&unknown, &VitalSettings::default()),
        VitalAction::None
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalPlanTests.cs :: PicksTheStrongestCastableConversionByName
#[test]
fn picks_the_strongest_castable_conversion_by_name() {
    let known = [
        spell(1, "Stamina to Mana Self I", 1, 50),
        spell(2, "Stamina to Mana Self VI", 6, 50),
        spell(3, "Stamina to Health Self VII", 7, 50),
    ];
    let levels = HashMap::from([(33, 300)]);

    let pick = vitals::try_find(&known, STAMINA_TO_MANA_STEM, &levels, 10).unwrap();
    assert_eq!(pick.spell_id, 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalPlanTests.cs :: SkipsConversionTiersTheSkillCannotCarry
#[test]
fn skips_conversion_tiers_the_skill_cannot_carry() {
    let known = [
        spell(1, "Stamina to Mana Self I", 1, 50),
        spell(2, "Stamina to Mana Self VII", 7, 400),
    ];
    let levels = HashMap::from([(33, 100)]);

    let pick = vitals::try_find(&known, STAMINA_TO_MANA_STEM, &levels, 10).unwrap();
    assert_eq!(pick.spell_id, 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalPlanTests.cs :: DoesNothingWhenVitalUpkeepIsTurnedOff
#[test]
fn does_nothing_when_vital_upkeep_is_turned_off() {
    let off = VitalSettings {
        enabled: false,
        ..VitalSettings::default()
    };
    assert_eq!(vitals::decide(&character(1, 100), &off), VitalAction::None);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VitalPlanTests.cs :: ReportsNoConversionWhenTheCharacterKnowsNone
#[test]
fn reports_no_conversion_when_the_character_knows_none() {
    let known = [spell(1, "Strength Self I", 1, 50)];
    assert_eq!(
        vitals::try_find(&known, STAMINA_TO_MANA_STEM, &HashMap::new(), 10),
        None
    );
}

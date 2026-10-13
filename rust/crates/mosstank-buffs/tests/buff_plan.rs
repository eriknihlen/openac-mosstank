//! The buff lines and the buff plan. Expected values are the C# tests'
//! (`BuffPlanTests.cs`).

#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;

use ac_plugin_abi::character::{ActiveEnchantment, Attribute, Skill, Training};
use ac_plugin_abi::magic::SpellInfo;
use mosstank_automation::StaticSpellCatalog;
use mosstank_buffs::plan::{self, BuildOptions};
use mosstank_buffs::profile::{self, BuffLine, BuffTargetKind};
use mosstank_settings::model::BuffSettings;

const LIFE_MAGIC_SKILL: u32 = 33;
const CREATURE_ENCHANTMENT_SKILL: u32 = 31;
const ITEM_ENCHANTMENT_SKILL: u32 = 32;

fn spell_with(
    id: u32,
    family: u32,
    tier: i32,
    description: &str,
    difficulty: i32,
    mana: i32,
    school: u32,
    duration: f32,
) -> SpellInfo {
    SpellInfo {
        spell_id: id,
        name: format!("spell-{id}"),
        family,
        tier,
        difficulty,
        mana_cost: mana,
        duration_seconds: duration,
        school,
        description: description.to_owned(),
        is_self_targeted: true,
        is_beneficial: true,
        ..SpellInfo::default()
    }
}

fn spell(id: u32, family: u32, tier: i32, description: &str) -> SpellInfo {
    spell_with(
        id,
        family,
        tier,
        description,
        50,
        10,
        CREATURE_ENCHANTMENT_SKILL,
        1800.0,
    )
}

fn skill(id: u32, name: &str, training: Training, level: u32) -> Skill {
    Skill {
        skill_id: id,
        name: name.to_owned(),
        training,
        current: level,
        ..Skill::default()
    }
}

fn attribute(kind: i32, name: &str) -> Attribute {
    Attribute {
        kind,
        name: name.to_owned(),
        current: 200,
        ..Attribute::default()
    }
}

fn active(spell_id: u32, family: u32, tier: i32, seconds: f64) -> ActiveEnchantment {
    ActiveEnchantment {
        spell_id,
        family,
        tier,
        seconds_remaining: seconds,
    }
}

fn lines(spells: &[SpellInfo]) -> Vec<BuffLine> {
    profile::build(spells)
}

fn ids(plan: &[SpellInfo]) -> Vec<u32> {
    plan.iter().map(|spell| spell.spell_id).collect()
}

fn build_plan(
    lines: &[BuffLine],
    skills: &[Skill],
    attributes: &[Attribute],
    enchantments: &[ActiveEnchantment],
    settings: &BuffSettings,
) -> Vec<SpellInfo> {
    plan::build(
        lines,
        skills,
        attributes,
        enchantments,
        settings,
        BuildOptions::default(),
    )
}

fn forced(
    lines: &[BuffLine],
    skills: &[Skill],
    attributes: &[Attribute],
    settings: &BuffSettings,
) -> Vec<SpellInfo> {
    plan::build(
        lines,
        skills,
        attributes,
        &[],
        settings,
        BuildOptions {
            force: true,
            ..BuildOptions::default()
        },
    )
}

fn buff_other(value: bool) -> BuffSettings {
    BuffSettings {
        buff_other: value,
        ..BuffSettings::default()
    }
}

fn creature_specialized(level: u32) -> Skill {
    skill(
        CREATURE_ENCHANTMENT_SKILL,
        "Creature Enchantment",
        Training::Specialized,
        level,
    )
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: ParsesBuffTargetFromDescription
#[test]
fn parses_buff_target_from_description() {
    for (description, kind, target) in [
        (
            "Increases the caster's Life Magic skill by 10 points.",
            BuffTargetKind::Skill,
            "Life Magic",
        ),
        (
            "Increases the caster's Strength by 10 points.",
            BuffTargetKind::Attribute,
            "Strength",
        ),
        (
            "Increases the caster's Self by 10 points.",
            BuffTargetKind::Attribute,
            "Self",
        ),
        (
            "Increases the caster's Assess Monster skill by 10 points.",
            BuffTargetKind::Skill,
            "Assess Creature",
        ),
    ] {
        assert_eq!(
            profile::try_parse_target(Some(description)),
            Some((kind, target.to_owned())),
            "{description}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: ClassifiesProtectionsAndAuras
#[test]
fn classifies_protections_and_auras() {
    for (description, expected) in [
        (
            "Reduces damage the caster takes from Fire by 9%.",
            BuffTargetKind::Protection,
        ),
        (
            "Increases the caster's natural armor by 20 points.",
            BuffTargetKind::Protection,
        ),
        (
            "Increases a weapon's damage value by 2 points.",
            BuffTargetKind::Aura,
        ),
        (
            "Improves a weapon's speed by 10 points.",
            BuffTargetKind::Aura,
        ),
        (
            "Increases the Melee Defense skill modifier of a weapon or magic caster by 3%.",
            BuffTargetKind::Aura,
        ),
        (
            "Increases the elemental damage bonus of an elemental magic caster by 1%.",
            BuffTargetKind::Aura,
        ),
    ] {
        assert_eq!(
            profile::classify(Some(description)).0,
            expected,
            "{description}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: ProtectionsAreSelfCandidatesAndAurasAreNot
#[test]
fn protections_are_self_candidates_and_auras_are_not() {
    let lines = lines(&[
        spell(
            1,
            109,
            1,
            "Reduces damage the caster takes from Fire by 9%.",
        ),
        spell(2, 154, 1, "Increases a weapon's damage value by 2 points."),
    ]);

    let all = build_plan(&lines, &[], &[], &[], &BuffSettings::default());
    assert_eq!(ids(&all), vec![1]);

    let none = build_plan(
        &lines,
        &[],
        &[],
        &[],
        &BuffSettings {
            buff_protections: false,
            ..BuffSettings::default()
        },
    );
    assert!(none.is_empty());
}

const BANE: &str = "Increases a shield or piece of armor's resistance to slashing damage by 10%. Target yourself to cast this spell on all of your equipped armor.";

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: BanesAreClassifiedFromAuthenticTargetYourselfInstruction
#[test]
fn banes_are_classified_from_authentic_target_yourself_instruction() {
    assert_eq!(profile::classify(Some(BANE)).0, BuffTargetKind::Bane);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: BanesAreNeverSelfListCandidates
#[test]
fn banes_are_never_self_list_candidates() {
    let bane = SpellInfo {
        spell_id: 1,
        name: "Blade Bane I".to_owned(),
        family: 174,
        tier: 1,
        difficulty: 50,
        mana_cost: 10,
        duration_seconds: 1800.0,
        school: CREATURE_ENCHANTMENT_SKILL,
        description: BANE.to_owned(),
        is_self_targeted: false,
        is_beneficial: true,
        ..SpellInfo::default()
    };
    let lines = profile::build(&[bane]);

    assert_eq!(lines[0].kind, BuffTargetKind::Bane);
    assert!(build_plan(&lines, &[], &[], &[], &BuffSettings::default()).is_empty());
    assert!(
        build_plan(
            &lines,
            &[],
            &[],
            &[],
            &BuffSettings {
                buff_banes: false,
                ..BuffSettings::default()
            }
        )
        .is_empty()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: ExtraExemplarAddsItsFamilyAndAntiExemplarRemovesIt
#[test]
fn extra_exemplar_adds_its_family_and_anti_exemplar_removes_it() {
    let exemplar = spell(100, 700, 1, "An optional self effect.");
    let known_higher_tier = spell(101, 700, 6, "An optional self effect.");
    let mut settings = buff_other(false);
    settings.extra_buff_spell_ids.push(exemplar.spell_id);
    let skills = [creature_specialized(300)];

    let mut catalog = StaticSpellCatalog::of(vec![exemplar.clone()]);
    let extra = plan::build(
        &lines(std::slice::from_ref(&known_higher_tier)),
        &skills,
        &[],
        &[],
        &settings,
        BuildOptions {
            spell_catalog: Some(&mut catalog),
            ..BuildOptions::default()
        },
    );
    assert_eq!(ids(&extra), vec![known_higher_tier.spell_id]);

    settings.anti_extra_buff_spell_ids.push(exemplar.spell_id);
    let mut catalog = StaticSpellCatalog::of(vec![exemplar]);
    assert!(
        plan::build(
            &lines(&[known_higher_tier]),
            &skills,
            &[],
            &[],
            &settings,
            BuildOptions {
                spell_catalog: Some(&mut catalog),
                ..BuildOptions::default()
            },
        )
        .is_empty()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: TheBuffMarginIsTheHeadroomTheProfileInsistsOn
#[test]
fn the_buff_margin_is_the_headroom_the_profile_insists_on() {
    for (margin, expected) in [(5, 2u32), (50, 1)] {
        let settings = BuffSettings {
            buff_other: false,
            skill_excess_over_difficulty: margin,
            ..BuffSettings::default()
        };
        let strength = "Increases the caster's Strength by 10 points.";
        let lines = lines(&[
            spell_with(
                1,
                700,
                1,
                strength,
                100,
                10,
                CREATURE_ENCHANTMENT_SKILL,
                1800.0,
            ),
            spell_with(
                2,
                700,
                6,
                strength,
                290,
                10,
                CREATURE_ENCHANTMENT_SKILL,
                1800.0,
            ),
        ]);

        let plan = build_plan(
            &lines,
            &[creature_specialized(300)],
            &[attribute(1, "Strength")],
            &[],
            &settings,
        );

        assert_eq!(ids(&plan), vec![expected], "margin {margin}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: TheRebuffThresholdDecidesWhenAHeldBuffIsTooShort
#[test]
fn the_rebuff_threshold_decides_when_a_held_buff_is_too_short() {
    for (rebuff_under, recasts) in [(300.0, false), (900.0, true)] {
        let settings = BuffSettings {
            buff_other: false,
            rebuff_when_under_seconds: rebuff_under,
            ..BuffSettings::default()
        };
        let held = spell(1, 700, 1, "Increases the caster's Strength by 10 points.");

        let plan = build_plan(
            &lines(std::slice::from_ref(&held)),
            &[creature_specialized(300)],
            &[attribute(1, "Strength")],
            &[active(held.spell_id, 700, 1, 600.0)],
            &settings,
        );

        assert_eq!(!plan.is_empty(), recasts, "threshold {rebuff_under}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: TheUntrainedLifeCeilingIsItsOwnProfileNumber
#[test]
fn the_untrained_life_ceiling_is_its_own_profile_number() {
    for (character_level, casts) in [(59, true), (61, false)] {
        let settings = BuffSettings {
            buff_other: false,
            // Unlike the other two ceilings, so reading one of those
            // instead answers differently.
            buff_with_untrained_life_skill: 60,
            buff_with_untrained_item_skill: 200,
            buff_with_untrained_creature_skill: 200,
            ..BuffSettings::default()
        };
        let lines = lines(&[spell_with(
            1,
            105,
            1,
            "Increases the caster's natural armor by 20 points.",
            50,
            10,
            LIFE_MAGIC_SKILL,
            1800.0,
        )]);

        let plan = plan::build(
            &lines,
            &[skill(
                LIFE_MAGIC_SKILL,
                "Life Magic",
                Training::Untrained,
                300,
            )],
            &[],
            &[],
            &settings,
            BuildOptions {
                character_level,
                ..BuildOptions::default()
            },
        );

        assert_eq!(!plan.is_empty(), casts, "level {character_level}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: ExtraExemplarStillRequiresAnAvailableMagicSchool
#[test]
fn extra_exemplar_still_requires_an_available_magic_school() {
    let exemplar = spell(100, 700, 1, "An optional self effect.");
    let known_higher_tier = spell(101, 700, 6, "An optional self effect.");
    let mut settings = buff_other(false);
    settings.extra_buff_spell_ids.push(exemplar.spell_id);
    let level = settings.buff_with_untrained_creature_skill + 1;
    let mut catalog = StaticSpellCatalog::of(vec![exemplar]);

    assert!(
        plan::build(
            &lines(&[known_higher_tier]),
            &[],
            &[],
            &[],
            &settings,
            BuildOptions {
                character_level: level,
                spell_catalog: Some(&mut catalog),
                ..BuildOptions::default()
            },
        )
        .is_empty()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: LegacyExtraNameResolvesAnUnknownExemplarThroughTheCatalog
#[test]
fn legacy_extra_name_resolves_an_unknown_exemplar_through_the_catalog() {
    let exemplar = spell(100, 700, 1, "An optional self effect.");
    let known_higher_tier = spell(101, 700, 6, "An optional self effect.");
    let mut settings = buff_other(false);
    settings
        .extra_buff_spell_names
        .insert(exemplar.name.clone());
    let mut catalog = StaticSpellCatalog::of(vec![exemplar]);

    let extra = plan::build(
        &lines(std::slice::from_ref(&known_higher_tier)),
        &[creature_specialized(300)],
        &[],
        &[],
        &settings,
        BuildOptions {
            spell_catalog: Some(&mut catalog),
            ..BuildOptions::default()
        },
    );

    assert_eq!(ids(&extra), vec![known_higher_tier.spell_id]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: ImpenetrabilityLeavesTheSelfListWithTheBanes
#[test]
fn impenetrability_leaves_the_self_list_with_the_banes() {
    let impenetrability = SpellInfo {
        spell_id: 2,
        name: "Impenetrability I".to_owned(),
        family: 173,
        tier: 1,
        difficulty: 50,
        mana_cost: 10,
        duration_seconds: 1800.0,
        school: ITEM_ENCHANTMENT_SKILL,
        description: "Increases a shield or piece of armor's armor value by 20 points. Target yourself to cast this spell on all of your equipped armor.".to_owned(),
        is_self_targeted: false,
        is_beneficial: true,
        ..SpellInfo::default()
    };
    let armor_self = SpellInfo {
        spell_id: 3,
        name: "Armor Self I".to_owned(),
        family: 105,
        tier: 1,
        difficulty: 50,
        mana_cost: 10,
        duration_seconds: 1800.0,
        school: LIFE_MAGIC_SKILL,
        description: "Increases the caster's natural armor by 20 points.".to_owned(),
        is_self_targeted: true,
        is_beneficial: true,
        ..SpellInfo::default()
    };
    let lines = profile::build(&[impenetrability, armor_self]);

    let plan = build_plan(
        &lines,
        &[skill(
            ITEM_ENCHANTMENT_SKILL,
            "Item Enchantment",
            Training::Trained,
            300,
        )],
        &[],
        &[],
        &BuffSettings::default(),
    );

    // Armor Self has an entry of its own and stays; Impenetrability does not.
    assert_eq!(ids(&plan), vec![3]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: UnrecognisedSelfBuffsFallIntoOtherAndAreOffByDefault
#[test]
fn unrecognised_self_buffs_fall_into_other_and_are_off_by_default() {
    let lines = lines(&[spell(
        1,
        93,
        1,
        "Restores 10 points of the caster's Health over 20 seconds.",
    )]);

    assert_eq!(lines[0].kind, BuffTargetKind::Other);
    assert!(build_plan(&lines, &[], &[], &[], &BuffSettings::default()).is_empty());
    assert_eq!(
        build_plan(&lines, &[], &[], &[], &buff_other(true)).len(),
        1
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: IgnoresSpellsWhoseDescriptionSaysNothingAboutAStat
#[test]
fn ignores_spells_whose_description_says_nothing_about_a_stat() {
    // A vital transfer describes a drain, not a buff.
    assert_eq!(
        profile::try_parse_target(Some(
            "Drains one-half of the caster's Stamina and gives 90% of that to his/her Mana."
        )),
        None
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: ExcludesInstantaneousSpellsFromBuffLines
#[test]
fn excludes_instantaneous_spells_from_buff_lines() {
    let instant = spell_with(
        1,
        89,
        1,
        "Increases the caster's Strength by 10 points.",
        50,
        10,
        CREATURE_ENCHANTMENT_SKILL,
        0.0,
    );
    assert!(profile::build(&[instant]).is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: BuffsTrainedSkillsAndSkipsUntrainedOnes
#[test]
fn buffs_trained_skills_and_skips_untrained_ones() {
    let lines = lines(&[
        spell(
            1,
            47,
            1,
            "Increases the caster's Life Magic skill by 10 points.",
        ),
        spell(
            2,
            71,
            1,
            "Increases the caster's Leadership skill by 10 points.",
        ),
    ]);

    let plan = build_plan(
        &lines,
        &[
            skill(LIFE_MAGIC_SKILL, "Life Magic", Training::Specialized, 300),
            skill(35, "Leadership", Training::Untrained, 300),
        ],
        &[],
        &[],
        &BuffSettings::default(),
    );

    assert_eq!(ids(&plan), vec![1]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: BuffsEveryAttribute
#[test]
fn buffs_every_attribute() {
    let lines = lines(&[
        spell(1, 1, 1, "Increases the caster's Strength by 10 points."),
        spell(2, 11, 1, "Increases the caster's Self by 10 points."),
    ]);

    let plan = build_plan(
        &lines,
        &[],
        &[attribute(0, "Strength"), attribute(5, "Self")],
        &[],
        &BuffSettings::default(),
    );

    assert_eq!(plan.len(), 2);
}

fn life(id: u32, tier: i32, difficulty: i32) -> SpellInfo {
    spell_with(
        id,
        47,
        tier,
        "Increases the caster's Life Magic skill by 10 points.",
        difficulty,
        10,
        LIFE_MAGIC_SKILL,
        1800.0,
    )
}

fn life_trained(level: u32) -> Skill {
    skill(LIFE_MAGIC_SKILL, "Life Magic", Training::Trained, level)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: PicksTheStrongestTierTheCastingSkillCanCarry
#[test]
fn picks_the_strongest_tier_the_casting_skill_can_carry() {
    let lines = lines(&[life(1, 7, 400), life(2, 4, 200)]);

    let plan = build_plan(
        &lines,
        &[life_trained(300)],
        &[],
        &[],
        &BuffSettings::default(),
    );

    assert_eq!(ids(&plan), vec![2]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: SkipsAFamilyAlreadyInForceAtAnEqualTierWithTimeLeft
#[test]
fn skips_a_family_already_in_force_at_an_equal_tier_with_time_left() {
    let lines = lines(&[life(1, 4, 100)]);

    let plan = build_plan(
        &lines,
        &[life_trained(300)],
        &[],
        &[active(1, 47, 4, 900.0)],
        &BuffSettings::default(),
    );

    assert!(plan.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: RecastsWhenTheBuffInForceIsWeaker
#[test]
fn recasts_when_the_buff_in_force_is_weaker() {
    let lines = lines(&[life(2, 7, 100)]);

    let plan = build_plan(
        &lines,
        &[life_trained(300)],
        &[],
        &[active(1, 47, 2, 900.0)],
        &BuffSettings::default(),
    );

    assert_eq!(plan.len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: RefreshesBelowVirindiTanksFiveMinuteThreshold
#[test]
fn refreshes_below_virindi_tanks_five_minute_threshold() {
    let lines = lines(&[life(1, 4, 100)]);
    let skills = [life_trained(300)];

    let comfortable = build_plan(
        &lines,
        &skills,
        &[],
        &[active(1, 47, 4, 301.0)],
        &BuffSettings::default(),
    );
    let expiring = build_plan(
        &lines,
        &skills,
        &[],
        &[active(1, 47, 4, 299.0)],
        &BuffSettings::default(),
    );

    assert!(comfortable.is_empty());
    assert_eq!(expiring.len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: OrdersCheapestFirstSoAPartialPassLandsMoreBuffs
#[test]
fn orders_cheapest_first_so_a_partial_pass_lands_more_buffs() {
    let attribute_spell = |id, family, name: &str, mana| {
        spell_with(
            id,
            family,
            1,
            &format!("Increases the caster's {name} by 10 points."),
            50,
            mana,
            CREATURE_ENCHANTMENT_SKILL,
            1800.0,
        )
    };
    let lines = lines(&[
        attribute_spell(1, 1, "Strength", 500),
        attribute_spell(2, 3, "Endurance", 5),
        attribute_spell(3, 5, "Quickness", 50),
    ]);

    let plan = build_plan(
        &lines,
        &[],
        &[
            attribute(0, "Strength"),
            attribute(1, "Endurance"),
            attribute(2, "Quickness"),
        ],
        &[],
        &BuffSettings::default(),
    );

    assert_eq!(ids(&plan), vec![2, 3, 1]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: ForceQueuesBuffsThatAreAlreadyInForce
#[test]
fn force_queues_buffs_that_are_already_in_force() {
    // VTank's Force Buff recasts everything rather than only what has
    // lapsed, which is what the Buff button does.
    let lines = lines(&[life(1, 4, 100)]);
    let skills = [life_trained(300)];
    let held = [active(1, 47, 4, 1800.0)];

    assert!(build_plan(&lines, &skills, &[], &held, &BuffSettings::default()).is_empty());
    let forced_plan = plan::build(
        &lines,
        &skills,
        &[],
        &held,
        &BuffSettings::default(),
        BuildOptions {
            force: true,
            ..BuildOptions::default()
        },
    );
    assert_eq!(forced_plan.len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: ForceStillRespectsSkillAndTrainingFilters
#[test]
fn force_still_respects_skill_and_training_filters() {
    let lines = lines(&[spell(
        1,
        71,
        1,
        "Increases the caster's Leadership skill by 10 points.",
    )]);

    let plan = forced(
        &lines,
        &[skill(35, "Leadership", Training::Untrained, 300)],
        &[],
        &BuffSettings::default(),
    );

    assert!(plan.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: EmptySpellbookProducesNoPlan
#[test]
fn empty_spellbook_produces_no_plan() {
    assert!(build_plan(&[], &[], &[], &[], &BuffSettings::default()).is_empty());
}

fn ordering_spellbook() -> Vec<BuffLine> {
    let s = |id, family, description: &str, mana, school| {
        spell_with(id, family, 1, description, 50, mana, school, 1800.0)
    };
    lines(&[
        s(
            1,
            101,
            "Increases the caster's Creature Enchantment skill by 10 points.",
            90,
            CREATURE_ENCHANTMENT_SKILL,
        ),
        s(
            2,
            102,
            "Increases the caster's Focus by 10 points.",
            80,
            CREATURE_ENCHANTMENT_SKILL,
        ),
        s(
            3,
            103,
            "Increases the caster's Self by 10 points.",
            70,
            CREATURE_ENCHANTMENT_SKILL,
        ),
        s(
            4,
            104,
            "Increases the caster's Endurance by 10 points.",
            60,
            CREATURE_ENCHANTMENT_SKILL,
        ),
        s(
            5,
            105,
            "Increases the caster's Strength by 10 points.",
            50,
            CREATURE_ENCHANTMENT_SKILL,
        ),
        s(
            6,
            106,
            "Increases the caster's Life Magic skill by 10 points.",
            40,
            CREATURE_ENCHANTMENT_SKILL,
        ),
        s(
            7,
            107,
            "Increases the caster's maximum burden by 100 units.",
            30,
            ITEM_ENCHANTMENT_SKILL,
        ),
        s(
            8,
            108,
            "Increases a weapon's damage value by 2 points.",
            20,
            ITEM_ENCHANTMENT_SKILL,
        ),
        // Life Magic (school 33)
        s(
            9,
            109,
            "Reduces damage the caster takes from Fire by 9%.",
            10,
            LIFE_MAGIC_SKILL,
        ),
    ])
}

fn ordered_plan() -> Vec<SpellInfo> {
    forced(
        &ordering_spellbook(),
        &[
            creature_specialized(300),
            skill(
                ITEM_ENCHANTMENT_SKILL,
                "Item Enchantment",
                Training::Trained,
                300,
            ),
            skill(LIFE_MAGIC_SKILL, "Life Magic", Training::Trained, 300),
        ],
        &[
            attribute(0, "Strength"),
            attribute(1, "Endurance"),
            attribute(4, "Focus"),
            attribute(5, "Self"),
        ],
        &buff_other(true),
    )
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: CreatureSpellsCastFirst_ThenItem_ThenLifeLast
#[test]
fn creature_spells_cast_first_then_item_then_life_last() {
    let plan = ordered_plan();
    let last = |school| plan.iter().rposition(|s| s.school == school).unwrap();
    let first = |school| plan.iter().position(|s| s.school == school).unwrap();

    assert!(
        last(CREATURE_ENCHANTMENT_SKILL) < first(ITEM_ENCHANTMENT_SKILL),
        "creature spells must all be cast before item spells"
    );
    assert!(
        last(ITEM_ENCHANTMENT_SKILL) < first(LIFE_MAGIC_SKILL),
        "item spells must all be cast before life spells"
    );
    assert_eq!(first(LIFE_MAGIC_SKILL), plan.len() - 1); // the protection goes last
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: CreatureGroupLeadsWithMagicSkill_ThenFocus_Willpower_Endurance
#[test]
fn creature_group_leads_with_magic_skill_then_focus_willpower_endurance() {
    let plan = ordered_plan();

    assert_eq!(ids(&plan[..4]), vec![1, 2, 3, 4]);
    let mut rest: Vec<u32> = plan[4..]
        .iter()
        .filter(|s| s.school == CREATURE_ENCHANTMENT_SKILL)
        .map(|s| s.spell_id)
        .collect();
    rest.sort_unstable();
    assert_eq!(rest, vec![5, 6]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: WithinOneGroupTheCheapestStillCastsFirst
#[test]
fn within_one_group_the_cheapest_still_casts_first() {
    let plan = ordered_plan();
    let tail: Vec<&SpellInfo> = plan[4..]
        .iter()
        .filter(|s| s.school == CREATURE_ENCHANTMENT_SKILL)
        .collect();

    assert_eq!(tail.len(), 2);
    assert!(tail[0].mana_cost <= tail[1].mana_cost);
    assert_eq!(tail[0].spell_id, 6); // the 40-mana line before the 50-mana one
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: RegenerationRatesAreClassifiedPerVital
#[test]
fn regeneration_rates_are_classified_per_vital() {
    for (description, vital) in [
        ("Increase caster's natural healing rate by 10%.", "Health"),
        (
            "Increases your Health Regeneration Rate by 50%. This effect can be layered with normal spell effects.",
            "Health",
        ),
        (
            "Increases the rate at which the caster regains Stamina by 10%.",
            "Stamina",
        ),
        (
            "Increases your Stamina Regeneration Rate by 50%.",
            "Stamina",
        ),
        ("Increases the caster's natural mana rate by 10%.", "Mana"),
        ("Increases your Mana Regeneration Rate by 50%.", "Mana"),
    ] {
        assert_eq!(
            profile::classify(Some(description)),
            (BuffTargetKind::Regeneration, vital.to_owned()),
            "{description}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: HermeticLinkIsAWandAura
#[test]
fn hermetic_link_is_a_wand_aura() {
    assert_eq!(
        profile::classify(Some(
            "Increases a magic casting implement's mana conversion bonus by 10%."
        ))
        .0,
        BuffTargetKind::Aura
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: RegenerationRatesFinishThePass
#[test]
fn regeneration_rates_finish_the_pass() {
    let s = |id, family, description: &str, mana, school| {
        spell_with(id, family, 1, description, 50, mana, school, 1800.0)
    };
    let book = lines(&[
        s(
            1,
            201,
            "Increases the caster's Focus by 10 points.",
            10,
            CREATURE_ENCHANTMENT_SKILL,
        ),
        s(
            2,
            202,
            "Reduces damage the caster takes from Fire by 9%.",
            10,
            LIFE_MAGIC_SKILL,
        ),
        s(
            3,
            203,
            "Increases the caster's natural armor by 20 points.",
            10,
            LIFE_MAGIC_SKILL,
        ),
        s(
            4,
            204,
            "Increase caster's natural healing rate by 10%.",
            1,
            LIFE_MAGIC_SKILL,
        ),
        s(
            5,
            205,
            "Increases the rate at which the caster regains Stamina by 10%.",
            1,
            LIFE_MAGIC_SKILL,
        ),
        s(
            6,
            206,
            "Increases the caster's natural mana rate by 10%.",
            1,
            LIFE_MAGIC_SKILL,
        ),
    ]);

    let plan = forced(
        &book,
        &[
            creature_specialized(300),
            skill(LIFE_MAGIC_SKILL, "Life Magic", Training::Trained, 300),
        ],
        &[attribute(4, "Focus")],
        &buff_other(true),
    );

    assert_eq!(plan.len(), 6);
    // ...the protections precede them...
    assert_eq!(ids(&plan[1..3]), vec![2, 3]);
    let mut last: Vec<u32> = ids(&plan[3..]);
    last.sort_unstable();
    assert_eq!(last, vec![4, 5, 6]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: RegenerationRatesCanBeSwitchedOff
#[test]
fn regeneration_rates_can_be_switched_off() {
    let book = lines(&[spell_with(
        1,
        301,
        1,
        "Increase caster's natural healing rate by 10%.",
        50,
        10,
        LIFE_MAGIC_SKILL,
        1800.0,
    )]);
    let off = BuffSettings {
        buff_regeneration: false,
        ..BuffSettings::default()
    };

    assert_eq!(forced(&book, &[], &[], &BuffSettings::default()).len(), 1);
    assert!(forced(&book, &[], &[], &off).is_empty());
}

/// Hyssop, Powdered Agate, Gypsum, Rowan.
const MASTERY_SELF: (u32, u32, u32, u32) = (7, 25, 42, 60);
/// The same family's other line: only the talisman (Poplar) differs.
const MASTERY_OTHER: (u32, u32, u32, u32) = (7, 25, 42, 49);

#[allow(clippy::fn_params_excessive_bools)]
fn tier(
    id: u32,
    name: &str,
    tier: i32,
    difficulty: i32,
    component_set: (u32, u32, u32, u32),
    self_targeted: bool,
    fellowship: bool,
    untargeted: bool,
    school: u32,
) -> SpellInfo {
    SpellInfo {
        spell_id: id,
        name: name.to_owned(),
        family: 43,
        tier,
        difficulty,
        mana_cost: 10,
        duration_seconds: 1800.0,
        school,
        description: "Increases the caster's Creature Enchantment skill by 10 points.".to_owned(),
        is_self_targeted: self_targeted,
        is_beneficial: true,
        is_fellowship: fellowship,
        is_untargeted: untargeted,
        component_set,
        ..SpellInfo::default()
    }
}

fn self_tier(id: u32, name: &str, rank: i32, difficulty: i32) -> SpellInfo {
    tier(
        id,
        name,
        rank,
        difficulty,
        MASTERY_SELF,
        true,
        false,
        false,
        CREATURE_ENCHANTMENT_SKILL,
    )
}

fn plan_for(creature_skill: u32, book: &[SpellInfo]) -> Vec<SpellInfo> {
    forced(
        &lines(book),
        &[
            creature_specialized(creature_skill),
            skill(LIFE_MAGIC_SKILL, "Life Magic", Training::Specialized, 450),
        ],
        &[],
        &buff_other(true),
    )
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: AGemSpellCarryingTheOtherLinesCompSetIsNeverATierCandidate
#[test]
fn a_gem_spell_carrying_the_other_lines_comp_set_is_never_a_tier_candidate() {
    let plan = plan_for(
        350,
        &[
            self_tier(0x022D, "Creature Enchantment Mastery Self I", 1, 1),
            self_tier(0x0232, "Creature Enchantment Mastery Self VI", 6, 250),
            tier(
                0x08A6,
                "Adja's Boon",
                7,
                300,
                MASTERY_OTHER,
                false,
                false,
                false,
                CREATURE_ENCHANTMENT_SKILL,
            ),
            self_tier(
                0x11B2,
                "Incantation of Creature Enchantment Mastery Self",
                8,
                400,
            ),
        ],
    );

    assert_eq!(ids(&plan), vec![0x0232]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: TheSelfGemOfTheSameFamilyStaysACandidate
#[test]
fn the_self_gem_of_the_same_family_stays_a_candidate() {
    let plan = plan_for(
        350,
        &[
            self_tier(0x022D, "Creature Enchantment Mastery Self I", 1, 1),
            self_tier(0x08A7, "Adja's Blessing", 7, 300),
        ],
    );

    assert_eq!(ids(&plan), vec![0x08A7]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: TheOtherLinesIncantationIsNotATierCandidateAtTheSameQuality
#[test]
fn the_other_lines_incantation_is_not_a_tier_candidate_at_the_same_quality() {
    let plan = plan_for(
        450,
        &[
            self_tier(0x022D, "Creature Enchantment Mastery Self I", 1, 1),
            tier(
                0x11B1,
                "Incantation of Creature Enchantment Mastery Other",
                8,
                400,
                MASTERY_OTHER,
                false,
                false,
                false,
                CREATURE_ENCHANTMENT_SKILL,
            ),
            self_tier(
                0x11B2,
                "Incantation of Creature Enchantment Mastery Self",
                8,
                400,
            ),
        ],
    );

    assert_eq!(ids(&plan), vec![0x11B2]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: AFellowshipMemberOfTheFamilyIsNotATierCandidate
#[test]
fn a_fellowship_member_of_the_family_is_not_a_tier_candidate() {
    let plan = plan_for(
        450,
        &[
            self_tier(0x022D, "Creature Enchantment Mastery Self I", 1, 1),
            tier(
                0x0D3B,
                "Superior Conjurant Chant",
                7,
                325,
                MASTERY_SELF,
                true,
                true,
                false,
                CREATURE_ENCHANTMENT_SKILL,
            ),
        ],
    );

    assert_eq!(ids(&plan), vec![0x022D]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: AnUntargetedMemberOfATargetedLineIsNotATierCandidate
#[test]
fn an_untargeted_member_of_a_targeted_line_is_not_a_tier_candidate() {
    let plan = plan_for(
        450,
        &[
            self_tier(0x022D, "Creature Enchantment Mastery Self I", 1, 1),
            tier(
                0x0999,
                "Mastery Ring",
                7,
                325,
                MASTERY_SELF,
                true,
                false,
                true,
                CREATURE_ENCHANTMENT_SKILL,
            ),
        ],
    );

    assert_eq!(ids(&plan), vec![0x022D]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: ACrossSchoolMemberOfTheFamilyIsNotATierCandidate
#[test]
fn a_cross_school_member_of_the_family_is_not_a_tier_candidate() {
    let plan = plan_for(
        450,
        &[
            self_tier(0x022D, "Creature Enchantment Mastery Self I", 1, 1),
            tier(
                0x0998,
                "Life Impostor",
                7,
                325,
                MASTERY_SELF,
                true,
                false,
                false,
                LIFE_MAGIC_SKILL,
            ),
        ],
    );

    assert_eq!(ids(&plan), vec![0x022D]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: TheLineReferenceIsTheWeakestSelfMemberAndClassifiesTheLine
#[test]
fn the_line_reference_is_the_weakest_self_member_and_classifies_the_line() {
    let strength = |id, name: &str, tier, difficulty, self_targeted, description: &str| SpellInfo {
        spell_id: id,
        name: name.to_owned(),
        family: 1,
        tier,
        difficulty,
        mana_cost: 10,
        duration_seconds: 1800.0,
        school: CREATURE_ENCHANTMENT_SKILL,
        description: description.to_owned(),
        is_self_targeted: self_targeted,
        is_beneficial: true,
        ..SpellInfo::default()
    };
    let other = strength(
        0x0001,
        "Strength Other I",
        1,
        1,
        false,
        "Increases the target's Strength by 10 points.",
    );
    let self_six = strength(
        0x0534,
        "Strength Self VI",
        6,
        250,
        true,
        "Increases the caster's Strength by 10 points.",
    );
    let self_one = strength(
        0x0002,
        "Strength Self I",
        1,
        1,
        true,
        "Increases the caster's Strength by 10 points.",
    );

    let lines = lines(&[other, self_six, self_one]);

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].reference().spell_id, 0x0002);
    assert_eq!(lines[0].kind, BuffTargetKind::Attribute);
    assert_eq!(lines[0].target_name, "Strength");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffPlanTests.cs :: ASecondFamilyRaisingTheSameSkillIsNotABuffLine
#[test]
fn a_second_family_raising_the_same_skill_is_not_a_buff_line() {
    const RAISES: &str = "Increases the caster's Creature Enchantment skill by ";
    let exemplar = SpellInfo {
        spell_id: 0x022D,
        name: "Creature Enchantment Mastery Self I".to_owned(),
        family: 43,
        tier: 1,
        difficulty: 1,
        mana_cost: 10,
        duration_seconds: 1800.0,
        school: CREATURE_ENCHANTMENT_SKILL,
        description: format!("{RAISES}10 points."),
        is_self_targeted: true,
        is_beneficial: true,
        ..SpellInfo::default()
    };
    let mastery = SpellInfo {
        spell_id: 0x0232,
        name: "Creature Enchantment Mastery Self VI".to_owned(),
        tier: 6,
        difficulty: 250,
        duration_seconds: 2700.0,
        ..exemplar.clone()
    };
    let special = SpellInfo {
        spell_id: 0x0E6F,
        name: "Prodigal Creature Enchantment Mastery".to_owned(),
        family: 470,
        tier: 1,
        difficulty: 300,
        mana_cost: 70,
        duration_seconds: 900.0,
        school: CREATURE_ENCHANTMENT_SKILL,
        description: format!("{RAISES}250 points."),
        is_self_targeted: true,
        is_beneficial: true,
        ..SpellInfo::default()
    };
    let skills = [creature_specialized(400)];
    let book = lines(&[mastery.clone(), special.clone()]);
    let catalog =
        || StaticSpellCatalog::of(vec![exemplar.clone(), mastery.clone(), special.clone()]);

    let mut covered_catalog = catalog();
    let covered = plan::build(
        &book,
        &skills,
        &[],
        &[
            active(mastery.spell_id, 43, 6, 2600.0),
            active(special.spell_id, 470, 1, 890.0),
        ],
        &BuffSettings::default(),
        BuildOptions {
            rebuff_when_under_seconds: Some(1700.0),
            spell_catalog: Some(&mut covered_catalog),
            ..BuildOptions::default()
        },
    );
    assert!(covered.is_empty());

    let mut due_catalog = catalog();
    let due = plan::build(
        &book,
        &skills,
        &[],
        &[],
        &BuffSettings::default(),
        BuildOptions {
            rebuff_when_under_seconds: Some(1700.0),
            spell_catalog: Some(&mut due_catalog),
            ..BuildOptions::default()
        },
    );
    assert_eq!(ids(&due), vec![mastery.spell_id]);
}

/// The tier walk on its own, with skill levels by school.
#[test]
fn a_tier_out_of_reach_falls_to_the_next() {
    let line = &lines(&[life(1, 7, 400), life(2, 4, 200)])[0];
    let mut levels = HashMap::new();
    levels.insert(LIFE_MAGIC_SKILL, 300);
    assert_eq!(
        plan::try_pick_tier(line, &levels, 10, None).map(|pick| pick.spell_id),
        Some(2)
    );
    levels.insert(LIFE_MAGIC_SKILL, 100);
    assert_eq!(plan::try_pick_tier(line, &levels, 10, None), None);
    levels.clear();
    // The school's skill unknown: the weakest tier.
    assert_eq!(
        plan::try_pick_tier(line, &levels, 10, None).map(|pick| pick.spell_id),
        Some(2)
    );
}

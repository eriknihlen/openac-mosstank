//! VTClassic's typed requirements (`VtankLootRequirementEvaluator`).

mod common;

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::{ArmorProfile, ItemProperties, ObjectClass, WeaponProfile};
use mosstank_formats::utl::VtankLootRequirement;
use mosstank_loot::requirements::{early_match, is_match, unsupported_key_warning};

// Spell 2604 grants +20 to int key 28, which is not one of the item's named
// fields, so it only "exists" through the raw property bag.
const BONUS_INT_KEY: u32 = 28;
const BONUS_INT_SPELL_ID: u32 = 2604;
const BONUS_DOUBLE_KEY: u32 = 29;
const BONUS_DOUBLE_ADDITIVE_SPELL_ID: u32 = 2600;

fn req(kind: i32, payload: &str) -> VtankLootRequirement {
    VtankLootRequirement {
        kind,
        payload: payload.to_owned(),
    }
}

fn test_item() -> InventoryItem {
    common::item(0, "Test Item", 0)
}

fn empty() -> ItemProperties {
    ItemProperties::default()
}

fn weapon() -> WeaponProfile {
    WeaponProfile {
        damage_type: 1,
        weapon_time: 40,
        weapon_skill: 44,
        damage: 50,
        damage_variance: 0.5,
        damage_mod: 1.0,
        weapon_length: 1.0,
        max_velocity: 1.0,
        weapon_offense: 1.0,
        max_velocity_estimated: 0,
    }
}

fn matches(
    requirements: &[VtankLootRequirement],
    item: &InventoryItem,
    properties: &ItemProperties,
) -> (bool, Option<String>) {
    is_match(requirements, item, properties, None)
}

// The "(T) Rare!" rule as Loot5.utl writes it: type 12 on IconUnderlay with
// 23308, the rare backdrop 0x06005B0C without the icon-id prefix.
fn rare_underlay_rule() -> VtankLootRequirement {
    req(12, "23308\r\n218103850\r\n")
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: TheRareRuleMatchesAnItemWithTheRareBackdrop
#[test]
fn the_rare_rule_matches_an_item_with_the_rare_backdrop() {
    let rare = InventoryItem {
        icon_underlay_id: 0x0600_5B0C,
        ..test_item()
    };
    assert_eq!(
        matches(&[rare_underlay_rule()], &rare, &empty()),
        (true, None)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: TheRareRuleDoesNotMatchAnItemWithoutTheBackdrop
#[test]
fn the_rare_rule_does_not_match_an_item_without_the_backdrop() {
    let ordinary = InventoryItem {
        icon_underlay_id: 0,
        ..test_item()
    };
    let other = InventoryItem {
        icon_underlay_id: 0x0600_6C0B,
        ..test_item()
    };
    assert!(!matches(&[rare_underlay_rule()], &ordinary, &empty()).0);
    assert!(!matches(&[rare_underlay_rule()], &other, &empty()).0);
}

fn shirt_coverage_rule() -> VtankLootRequirement {
    req(12, "104\r\n218103821\r\n")
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: TheShirtCoverageRuleMatchesAShirtBeforeAppraisal
#[test]
fn the_shirt_coverage_rule_matches_a_shirt_before_appraisal() {
    let shirt = InventoryItem {
        coverage_mask: 104,
        ..test_item()
    };
    assert_eq!(
        matches(&[shirt_coverage_rule()], &shirt, &empty()),
        (true, None)
    );
    assert_eq!(
        early_match(&shirt_coverage_rule(), &shirt, &empty(), None),
        (true, true)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: TheShirtCoverageRuleDoesNotMatchPants
#[test]
fn the_shirt_coverage_rule_does_not_match_pants() {
    let pants = InventoryItem {
        coverage_mask: 22,
        ..test_item()
    };
    assert!(!matches(&[shirt_coverage_rule()], &pants, &empty()).0);
}

// Loot5.utl's "(H) 430 Insane": a heavy melee weapon that can reach tinked
// damage 109 with a melee defense and an attack bonus of 1.17.
fn insane_melee_rule() -> VtankLootRequirement {
    req(2008, "109\r\n1.17\r\n1.17\r\n")
}

fn untinkerable_sword() -> InventoryItem {
    InventoryItem {
        object_class: ObjectClass::MeleeWeapon,
        damage: 90,
        damage_variance: 0.3,
        ..test_item()
    }
}

fn melee_properties(weapon_offense: f64) -> ItemProperties {
    ItemProperties {
        floats: vec![(29, 1.18)],
        weapon: Some(WeaponProfile {
            weapon_offense,
            ..weapon()
        }),
        ..empty()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: TheInsaneMeleeRuleReadsTheAttackBonusFromTheAppraisedWeapon
#[test]
fn the_insane_melee_rule_reads_the_attack_bonus_from_the_appraised_weapon() {
    assert_eq!(
        matches(
            &[insane_melee_rule()],
            &untinkerable_sword(),
            &melee_properties(1.18)
        ),
        (true, None)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: TheInsaneMeleeRuleRejectsAWeaponWhoseAttackBonusFallsShort
#[test]
fn the_insane_melee_rule_rejects_a_weapon_whose_attack_bonus_falls_short() {
    assert!(
        !matches(
            &[insane_melee_rule()],
            &untinkerable_sword(),
            &melee_properties(1.10)
        )
        .0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: ABuffedAttackBonusRuleAddsTheWeaponsOwnAttackCantrip
#[test]
fn a_buffed_attack_bonus_rule_adds_the_weapons_own_attack_cantrip() {
    let rule = req(2005, "1.19\r\n167772172\r\n");
    let sword = InventoryItem {
        appraised_spell_ids: vec![2591],
        ..test_item()
    };
    let properties = ItemProperties {
        weapon: Some(WeaponProfile {
            weapon_offense: 1.15,
            ..weapon()
        }),
        ..empty()
    };
    assert_eq!(matches(&[rule], &sword, &properties), (true, None));
}

fn bow_damage_rule() -> VtankLootRequirement {
    req(2001, "78.6\r\n")
}

fn bow_properties(damage_mod: f64) -> ItemProperties {
    ItemProperties {
        ints: vec![(204, 22)],
        weapon: Some(WeaponProfile {
            damage: 0,
            damage_mod,
            ..weapon()
        }),
        ..empty()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: TheBowDamageRuleReadsTheDamageModifierFromTheAppraisedWeapon
#[test]
fn the_bow_damage_rule_reads_the_damage_modifier_from_the_appraised_weapon() {
    let bow = InventoryItem {
        object_class: ObjectClass::MissileWeapon,
        ..test_item()
    };
    assert_eq!(
        matches(&[bow_damage_rule()], &bow, &bow_properties(2.70)),
        (true, None)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: TheBowDamageRuleRejectsABowWithTooSmallAModifier
#[test]
fn the_bow_damage_rule_rejects_a_bow_with_too_small_a_modifier() {
    let bow = InventoryItem {
        object_class: ObjectClass::MissileWeapon,
        ..test_item()
    };
    assert!(!matches(&[bow_damage_rule()], &bow, &bow_properties(2.60)).0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: RangeIsTheLaunchSpeedTheAppraisedWeaponReports
#[test]
fn range_is_the_launch_speed_the_appraised_weapon_reports() {
    let rule = req(5, "25\r\n167772173\r\n");
    let fast = ItemProperties {
        weapon: Some(WeaponProfile {
            max_velocity: 27.3,
            ..weapon()
        }),
        ..empty()
    };
    let slow = ItemProperties {
        weapon: Some(WeaponProfile {
            max_velocity: 18.0,
            ..weapon()
        }),
        ..empty()
    };
    assert!(matches(std::slice::from_ref(&rule), &test_item(), &fast).0);
    assert!(!matches(std::slice::from_ref(&rule), &test_item(), &slow).0);
    assert!(!matches(&[rule], &test_item(), &empty()).0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: WeaponSpeedIsTheAppraisedSpeedRatingNotTheCombatRole
#[test]
fn weapon_speed_is_the_appraised_speed_rating_not_the_combat_role() {
    let rule = req(12, "25\r\n218103839\r\n");
    let sword = InventoryItem {
        combat_use: 1,
        ..test_item()
    };
    let appraised = ItemProperties {
        weapon: Some(WeaponProfile {
            weapon_time: 25,
            ..weapon()
        }),
        ..empty()
    };
    assert!(matches(std::slice::from_ref(&rule), &sword, &appraised).0);
    let role_25 = InventoryItem {
        combat_use: 25,
        ..sword
    };
    assert!(!matches(&[rule], &role_25, &empty()).0);
}

// Values the server sends with the object itself, every field distinct.
fn created_item() -> InventoryItem {
    InventoryItem {
        icon_id: 0x0600_1F2A,
        item_type: 0x2,
        public_flags: 0x10,
        useability: 0x8_0000,
        target_type: 0x10,
        combat_use: 2,
        ammo_type: 1,
        equipped_location: 0x200,
        structure: 7,
        maximum_structure: 50,
        container_id: 0,
        wielder_id: 0x5000_000A,
        use_radius: 2.5,
        ..test_item()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: AValueTheObjectCarriesDecidesBeforeAppraisal
#[test]
fn a_value_the_object_carries_decides_before_appraisal() {
    let rows: [(u32, i32); 12] = [
        (218_103_809, 7978),
        (218_103_834, 0x2),
        (218_103_835, 0x10),
        (218_103_843, 0x8_0000),
        (218_103_826, 0x10),
        (218_103_823, 2),
        (218_103_825, 1),
        (10, 0x200),
        (92, 7),
        (91, 50),
        (218_103_810, 0x5000_000A),
        (218_103_817, -1),
    ];
    for (key, expected) in rows {
        let rule = req(12, &format!("{expected}\r\n{key}\r\n"));
        let item = created_item();
        assert_eq!(
            matches(std::slice::from_ref(&rule), &item, &empty()),
            (true, None),
            "key {key}"
        );
        assert_eq!(
            early_match(&rule, &item, &empty(), None),
            (true, true),
            "key {key}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: AnItemInAPackHasItsPackAsContainerAndNoSlotMark
#[test]
fn an_item_in_a_pack_has_its_pack_as_container_and_no_slot_mark() {
    let packed = InventoryItem {
        container_id: 0x8000_1234,
        wielder_id: 0,
        ..created_item()
    };
    #[allow(clippy::cast_possible_wrap)]
    let container = req(12, &format!("{}\r\n218103810\r\n", 0x8000_1234u32 as i32));
    let wielded_mark = req(12, "-1\r\n218103817\r\n");
    assert_eq!(matches(&[container], &packed, &empty()), (true, None));
    assert!(!matches(&[wielded_mark], &packed, &empty()).0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: ApproachDistanceIsTheUseRadiusTheObjectCarries
#[test]
fn approach_distance_is_the_use_radius_the_object_carries() {
    let rules = [req(4, "3\r\n167772168\r\n"), req(5, "2\r\n167772168\r\n")];
    assert_eq!(matches(&rules, &created_item(), &empty()), (true, None));
    let no_radius = InventoryItem {
        use_radius: 0.0,
        ..created_item()
    };
    assert!(!matches(&rules, &no_radius, &empty()).0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: ThePluralNameIsTheNameTheObjectCarriesForMoreThanOne
#[test]
fn the_plural_name_is_the_name_the_object_carries_for_more_than_one() {
    let rule = req(1, "^Pyreals$\r\n184549376\r\n");
    let coins = InventoryItem {
        plural_name: "Pyreals".to_owned(),
        ..test_item()
    };
    assert_eq!(
        matches(std::slice::from_ref(&rule), &coins, &empty()),
        (true, None)
    );
    let motes = InventoryItem {
        plural_name: "Pyreal Motes".to_owned(),
        ..test_item()
    };
    assert!(!matches(&[rule], &motes, &empty()).0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: AKeyVtankHasNoValueForReadsNothingWhateverTheItemIs
#[test]
fn a_key_vtank_has_no_value_for_reads_nothing_whatever_the_item_is() {
    for key in [
        218_103_816u32,
        218_103_818,
        218_103_819,
        218_103_831,
        218_103_846,
    ] {
        let rule = req(13, &format!("0\r\n{key}\r\n"));
        let item = InventoryItem {
            spell_id: 1234,
            wielder_id: 0x5000_000A,
            equipped_location: 0x200,
            public_flags: 0x10,
            appraised_spell_ids: vec![2591, 2603],
            ..test_item()
        };
        assert_eq!(
            matches(&[rule], &item, &empty()),
            (false, None),
            "key {key}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: WorkmanshipIsTheAppraisedWholeNumberNotTheObjectsFraction
#[test]
fn workmanship_is_the_appraised_whole_number_not_the_objects_fraction() {
    let rule = req(3, "7\r\n105\r\n");
    let item = InventoryItem {
        workmanship: 7.8,
        ..test_item()
    };
    let appraised = ItemProperties {
        ints: vec![(105, 8)],
        ..empty()
    };
    assert!(!matches(std::slice::from_ref(&rule), &item, &empty()).0);
    assert_eq!(matches(&[rule], &item, &appraised), (true, None));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: AnEnabledRuleOnAKeyTheClientCannotSupplyIsNamedInOneWarning
#[test]
fn an_enabled_rule_on_a_key_the_client_cannot_supply_is_named_in_one_warning() {
    let monarch_rule = common::vtank_rule(
        "(A) My monarch's gear",
        mosstank_formats::utl::LootAction::Keep,
        vec![req(7, "1\r\n"), req(12, "1342177290\r\n218103820\r\n")],
    );
    let disabled_rule = common::vtank_rule(
        "(A) Disabled allegiance rule",
        mosstank_formats::utl::LootAction::Keep,
        vec![req(12, "5\r\n218103853\r\n"), req(9999, "true\r\n")],
    );
    let coverage_rule = common::vtank_rule(
        "(C) Shirt",
        mosstank_formats::utl::LootAction::Keep,
        vec![shirt_coverage_rule()],
    );

    let warning = unsupported_key_warning("Loot5", [&coverage_rule, &monarch_rule, &disabled_rule])
        .expect("a warning");

    assert!(warning.contains("Loot5"));
    assert!(warning.contains("\"(A) My monarch's gear\" reads Monarch (218103820)"));
    assert!(!warning.contains("Disabled"));
    assert!(!warning.contains("Shirt"));
    assert_eq!(
        unsupported_key_warning("Loot5", [&coverage_rule, &disabled_rule]),
        None
    );
}

fn armor() -> ArmorProfile {
    ArmorProfile {
        armor_level: 500,
        slash_mod: 0.1,
        pierce_mod: 0.2,
        bludgeon_mod: 0.3,
        cold_mod: 0.4,
        fire_mod: 0.5,
        acid_mod: 0.6,
        nether_mod: 0.7,
        electric_mod: 0.8,
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: EachProtectionKeyReadsItsOwnAppraisedArmorNumber
#[test]
fn each_protection_key_reads_its_own_appraised_armor_number() {
    let rows = [
        (167_772_160u32, "0.1"),
        (167_772_161, "0.2"),
        (167_772_162, "0.3"),
        (167_772_163, "0.6"),
        (167_772_164, "0.8"),
        (167_772_165, "0.5"),
        (167_772_166, "0.4"),
    ];
    for (key, expected) in rows {
        let rules = [
            req(5, &format!("{expected}\r\n{key}\r\n")),
            req(4, &format!("{expected}\r\n{key}\r\n")),
        ];
        let armored = ItemProperties {
            armor: Some(armor()),
            ..empty()
        };
        assert_eq!(
            matches(&rules, &test_item(), &armored),
            (true, None),
            "key {key}"
        );
        assert!(!matches(&rules, &test_item(), &empty()).0, "key {key}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: BuffedIntRequirementDoesNotApplyBonusWhenBaseKeyIsAbsent
#[test]
fn buffed_int_requirement_does_not_apply_bonus_when_base_key_is_absent() {
    let item = InventoryItem {
        appraised_spell_ids: vec![BONUS_INT_SPELL_ID],
        ..test_item()
    };
    let requirement = req(2003, &format!("10\r\n{BONUS_INT_KEY}\r\n"));
    assert_eq!(matches(&[requirement], &item, &empty()), (false, None));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: BuffedIntRequirementAppliesBonusWhenBaseKeyExists
#[test]
fn buffed_int_requirement_applies_bonus_when_base_key_exists() {
    let item = InventoryItem {
        appraised_spell_ids: vec![BONUS_INT_SPELL_ID],
        ..test_item()
    };
    let properties = ItemProperties {
        ints: vec![(BONUS_INT_KEY, 0)],
        ..empty()
    };
    let requirement = req(2003, &format!("10\r\n{BONUS_INT_KEY}\r\n"));
    assert_eq!(matches(&[requirement], &item, &properties), (true, None));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: BuffedDoubleRequirementDoesNotApplyBonusWhenBaseKeyIsAbsent
#[test]
fn buffed_double_requirement_does_not_apply_bonus_when_base_key_is_absent() {
    let item = InventoryItem {
        appraised_spell_ids: vec![BONUS_DOUBLE_ADDITIVE_SPELL_ID],
        ..test_item()
    };
    let requirement = req(2005, &format!("0.02\r\n{BONUS_DOUBLE_KEY}\r\n"));
    assert_eq!(matches(&[requirement], &item, &empty()), (false, None));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: BuffedDoubleRequirementAppliesAdditiveBonusWhenBaseKeyExists
#[test]
fn buffed_double_requirement_applies_additive_bonus_when_base_key_exists() {
    let item = InventoryItem {
        appraised_spell_ids: vec![BONUS_DOUBLE_ADDITIVE_SPELL_ID],
        ..test_item()
    };
    let properties = ItemProperties {
        floats: vec![(BONUS_DOUBLE_KEY, 0.0)],
        ..empty()
    };
    let requirement = req(2005, &format!("0.02\r\n{BONUS_DOUBLE_KEY}\r\n"));
    assert_eq!(matches(&[requirement], &item, &properties), (true, None));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootRequirementEvaluatorTests.cs :: BuffedDoubleRequirementAppliesMultiplicativeBonusWhenChangeIsSet
#[test]
fn buffed_double_requirement_applies_multiplicative_bonus_when_change_is_set() {
    let item = InventoryItem {
        appraised_spell_ids: vec![3201],
        ..test_item()
    };
    let properties = ItemProperties {
        floats: vec![(144, 10.0)],
        ..empty()
    };
    // Additive would give 11.05; multiplicative gives 10.5, short of 10.6.
    let requirement = req(2005, "10.6\r\n144\r\n");
    assert_eq!(matches(&[requirement], &item, &properties), (false, None));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ABuffedKeyOnlyGainsItsSpellBonusWhenTheItemCarriesTheBaseKey
#[test]
fn a_buffed_key_only_gains_its_spell_bonus_when_the_item_carries_the_base_key() {
    // Spell 2598 is Blood Drinker: +2 to the item's damage key.
    let no_damage = InventoryItem {
        appraised_spell_ids: vec![2598],
        ..common::item(10, "Ruby", 273)
    };
    let weapon = InventoryItem {
        damage: 5,
        ..no_damage.clone()
    };
    let at_least_one = [common::requirement(2003, &["1", "218103842"])];
    assert!(!matches(&at_least_one, &no_damage, &empty()).0);
    assert!(matches(&at_least_one, &weapon, &empty()).0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ABuffedDoubleMultipliesOnlyWhenTheAuthoredOperationIsOne
#[test]
fn a_buffed_double_multiplies_only_when_the_authored_operation_is_one() {
    // Spell 3199 authors 1.10, which truncates to 1 and multiplies; spell
    // 2588 authors 0.05, which truncates to 0 and adds.
    let rows: [(u32, u32, f64, &str, bool); 4] = [
        (3199, 144, 1.0, "1.1", true),
        (3199, 144, 1.0, "1.11", false),
        (2588, 29, 0.10, "0.15", true),
        (2588, 29, 0.10, "0.16", false),
    ];
    for (spell_id, key, base, threshold, expected) in rows {
        let item = InventoryItem {
            appraised_spell_ids: vec![spell_id],
            ..common::item(10, "Sword", 273)
        };
        let properties = ItemProperties {
            floats: vec![(key, base)],
            ..empty()
        };
        let requirement = common::requirement(2005, &[threshold, &key.to_string()]);
        assert_eq!(
            matches(&[requirement], &item, &properties).0,
            expected,
            "spell {spell_id}, threshold {threshold}"
        );
    }
}

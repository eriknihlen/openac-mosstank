//! The rule list, MossTank's own expressions, the corpse killer's name, and
//! the salvage and mana planners (`LootingTests.cs`).

mod common;

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::{ItemProperties, ObjectClass};
use common::{World, item, requirement, rule, vtank_rule};
use mosstank_dotnet::collections::IgnoreCaseStringMap;
use mosstank_formats::utl::{LootAction, VtankSalvageCombineSettings};
use mosstank_loot::controller::LootController;
use mosstank_loot::corpse_scan::killer_name;
use mosstank_loot::expression::LootRuleExpression;
use mosstank_loot::mana::{self, TransferChecks};
use mosstank_loot::rules::{decide, needs_identify};
use mosstank_loot::salvage;
use mosstank_settings::model::LootSettings;

fn empty() -> ItemProperties {
    ItemProperties::default()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: CorpseKillerParsingMatchesVtankLongDescription
#[test]
fn corpse_killer_parsing_matches_vtank_long_description() {
    assert_eq!(killer_name("Killed by Tester."), "Tester");
    assert_eq!(
        killer_name("Killed by Moss Wart's War Magic."),
        "Moss Wart's War Magic"
    );
    assert_eq!(killer_name("Generated treasure"), "");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: LootExpressionMatchesProjectedAndRawProperties
#[test]
fn loot_expression_matches_projected_and_raw_properties() {
    let item = InventoryItem {
        value: 100,
        workmanship: 9.5,
        material_type: 12,
        stack_size: 5,
        ..item(10, "Heavy Pyreal", 44)
    };
    let properties = ItemProperties {
        ints: vec![(105, 7)],
        ..empty()
    };
    for (source, expected) in [
        ("name ~= Pyreal && value >= 100", true),
        ("name == 'Heavy Pyreal' || wcid == 55", true),
        ("workmanship > 8 && material == 12", true),
        ("int[105] == 7", true),
        ("stack > 10", false),
    ] {
        let compiled = LootRuleExpression::compile(source).expect("compiles");
        assert_eq!(
            compiled.is_match(&item, &properties),
            Ok(expected),
            "{source}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: RuleEngineUsesFirstMatchAndHonorsKeepUpToCounts
#[test]
fn rule_engine_uses_first_match_and_honors_keep_up_to_counts() {
    let corpse_item = InventoryItem {
        stack_size: 2,
        ..item(10, "Health Elixir", 44)
    };
    let held = InventoryItem {
        stack_size: 8,
        ..item(20, "Health Elixir", 44)
    };
    let mut elixirs = rule("Elixirs", "name ~= elixir", LootAction::KeepUpTo);
    elixirs.keep_count = 10;
    let rules = vec![elixirs, rule("All", "*", LootAction::Keep)];
    let mut pending = IgnoreCaseStringMap::new();
    pending.insert("Health Elixir".to_owned(), 2);

    assert_eq!(
        decide(
            &corpse_item,
            &empty(),
            &rules,
            &[held],
            Some(&pending),
            None
        )
        .0,
        None
    );

    let decision = decide(&corpse_item, &empty(), &rules, &[], None, None)
        .0
        .expect("a decision");
    assert_eq!(decision.action, LootAction::KeepUpTo);
    assert_eq!(decision.rule_name, "Elixirs");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ImportedVtankRequirementsExecuteAsExactAndSet
#[test]
fn imported_vtank_requirements_execute_as_exact_and_set() {
    let item = InventoryItem {
        value: 25_000,
        object_class: ObjectClass::MeleeWeapon,
        appraised_spell_ids: vec![777],
        palettes: vec![common::palette(0x0400_ABCD, 255, 0, 0)],
        gear_damage: 3,
        gear_critical_chance: 2,
        ..item(10, "Epic Sword", 44)
    };
    let mut world = World {
        character_level: 275,
        main_pack_slots: 101,
        known_spell: Some(common::spell(777, "Legendary Blood Thirst", 1, 32)),
        skills: vec![ac_plugin_abi::character::Skill {
            base: 450,
            ..common::skill(44, "Heavy Weapons", 500)
        }],
        ..World::default()
    };
    let mut rule = vtank_rule(
        "VTClassic",
        LootAction::Keep,
        vec![
            requirement(1, &["^Epic.*", "1"]),
            requirement(3, &["25000", "19"]),
            requirement(7, &["1"]),
            requirement(9, &["Legendary.*", "", "1"]),
            requirement(14, &["255", "0", "0", "0", "0"]),
            requirement(15, &["255", "0", "0", "0", "0", "Amuli Coat (Chest)"]),
            requirement(16, &["255", "0", "0", "0", "0", "0"]),
            requirement(17, &["0", "43981"]),
            requirement(1000, &["500", "44"]),
            requirement(1001, &["101"]),
            requirement(1002, &["275"]),
            requirement(1004, &["44", "400", "460"]),
            requirement(2007, &["5"]),
            requirement(9999, &["false"]),
        ],
    );

    assert!(
        decide(
            &item,
            &empty(),
            std::slice::from_ref(&rule),
            &[],
            None,
            Some(&mut world)
        )
        .0
        .is_some()
    );

    "25001\r\n19\r\n".clone_into(&mut rule.vtank_requirements[1].payload);
    assert!(
        decide(&item, &empty(), &[rule], &[], None, Some(&mut world))
            .0
            .is_none()
    );
}

fn money() -> InventoryItem {
    InventoryItem {
        object_class: ObjectClass::Money,
        ..item(10, "Pyreal", 273)
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ADecidedRuleMatchMeansTheItemNeedsNoAppraisal
#[test]
fn a_decided_rule_match_means_the_item_needs_no_appraisal() {
    let rules = vec![
        vtank_rule("Money", LootAction::Keep, vec![requirement(7, &["7"])]),
        vtank_rule(
            "Good weapons",
            LootAction::Salvage,
            vec![requirement(2003, &["100", "218103842"])],
        ),
    ];
    assert!(!needs_identify(&money(), &empty(), &rules, None));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: AnOpenRuleIsMootWhenALaterDecidedRuleCarriesTheSameAction
#[test]
fn an_open_rule_is_moot_when_a_later_decided_rule_carries_the_same_action() {
    let good_weapons = || {
        vtank_rule(
            "Good weapons",
            LootAction::Keep,
            vec![requirement(2003, &["100", "218103842"])],
        )
    };
    let same_action = vec![
        good_weapons(),
        vtank_rule("Money", LootAction::Keep, vec![requirement(7, &["7"])]),
    ];
    let different_action = vec![
        good_weapons(),
        vtank_rule("Money", LootAction::Salvage, vec![requirement(7, &["7"])]),
    ];
    assert!(!needs_identify(&money(), &empty(), &same_action, None));
    assert!(needs_identify(&money(), &empty(), &different_action, None));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: SpellRequirementsStayOpenOnlyWhileTheItemClaimsToBeMagical
#[test]
fn spell_requirements_stay_open_only_while_the_item_claims_to_be_magical() {
    let rules = vec![
        vtank_rule("Enchanted", LootAction::Keep, vec![requirement(8, &["1"])]),
        vtank_rule("Everything else", LootAction::NoLoot, Vec::new()),
    ];
    let plain = item(10, "Rock", 273);
    let magical = InventoryItem {
        effects: 1,
        ..plain.clone()
    };
    assert!(!needs_identify(&plain, &empty(), &rules, None));
    assert!(needs_identify(&magical, &empty(), &rules, None));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: SalvageCombineUsesVtanksExactWorkmanshipBands
#[test]
fn salvage_combine_uses_vtanks_exact_workmanship_bands() {
    for (left, right, expected) in [
        (6.9f32, 6.0f32, true),
        (6.9, 7.0, false),
        (7.0, 8.99, true),
        (8.99, 9.0, false),
        (9.0, 9.99, true),
        (9.99, 10.0, false),
        (10.0, 10.0, true),
    ] {
        assert_eq!(
            salvage::same_vtank_workmanship_band(left, right),
            expected,
            "{left} {right}"
        );
    }
}

fn bag(id: u32, name: &str, workmanship: f32, value: i32, structure: i32) -> InventoryItem {
    InventoryItem {
        material_type: 12,
        workmanship,
        value,
        structure,
        ..item(id, name, 1)
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ImportedSalvageRangesAndValueModeDriveThePlanner
#[test]
fn imported_salvage_ranges_and_value_mode_drive_the_planner() {
    assert!(!salvage::same_combine_band(5.0, 6.0, "1-5, 6-10"));
    let settings = VtankSalvageCombineSettings {
        default_combine_string: "1-5, 6-10".to_owned(),
        material_combine_strings: std::collections::BTreeMap::new(),
        material_value_mode_values: [(12, 100)].into_iter().collect(),
    };
    let bags = [
        bag(1, "Salvaged Iron (4)", 4.0, 60, 30),
        bag(2, "Salvaged Iron (5)", 5.0, 50, 30),
        bag(3, "Salvaged Iron (6)", 6.0, 500, 30),
    ];
    let plan = salvage::plan(&bags, None, Some(&settings)).expect("a plan");
    assert_eq!(plan.object_ids, [1, 2]);
}

/// The planner's questions, answered as the test file's lambdas answer them.
struct Checks {
    profiled: fn(&InventoryItem) -> bool,
}

impl TransferChecks for Checks {
    fn is_profiled_mana_stone(&mut self, item: &InventoryItem) -> bool {
        (self.profiled)(item)
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ManaStonePlannerUsesStoneOnQualifiedHighestManaTank
#[test]
fn mana_stone_planner_uses_stone_on_qualified_highest_mana_tank() {
    let stone = InventoryItem {
        item_type: 0x0008_0000,
        object_class: ObjectClass::ManaStone,
        ..item(1, "Mana Stone", 1)
    };
    let tank = |id: u32, name: &str, mana: i32| InventoryItem {
        item_current_mana: mana,
        value: 1,
        workmanship: 5.0,
        effects: 1,
        ..item(id, name, id)
    };
    let low = tank(2, "Low Tank", 1200);
    let high = tank(3, "High Tank", 3000);
    let classified = |id: u32| match id {
        1 => Some(LootAction::ManaStone),
        2 | 3 => Some(LootAction::ManaTank),
        _ => None,
    };
    let mut checks = Checks {
        profiled: |candidate| {
            candidate.object_class == ObjectClass::ManaStone && candidate.name == "Mana Stone"
        },
    };
    let plan = mana::plan(&[stone, low, high], &classified, 1000, &mut checks).expect("a plan");
    assert_eq!(plan.stone_object_id, 1);
    assert_eq!(plan.tank_object_id, 3);
}

// No C# source: the Rust kit compiles a pattern far more slowly than .NET
// builds a `Regex`, and a large profile compiled every pattern again for
// each item decided (owner's report 2026-10-15: 53-77 ms ticks while
// looting). The rule list's patterns are compiled ahead, a budget a tick,
// once each; a list loaded afterwards is warmed afresh.
#[test]
fn the_rule_patterns_are_compiled_ahead_a_few_a_tick() {
    let rules: Vec<_> = (0..5)
        .map(|index| {
            vtank_rule(
                &format!("Rule {index}"),
                LootAction::Keep,
                vec![requirement(1, &[&format!("warm-ahead-name-{index}"), "1"])],
            )
        })
        .collect();
    let mut loot = LootController::new(LootSettings {
        rules,
        ..LootSettings::default()
    });
    let mut ticks = 0;
    while loot.warm_rule_patterns(2) {
        ticks += 1;
        assert!(ticks < 10, "the warm-up never ends");
    }
    assert_eq!(ticks, 3, "two patterns a tick, five patterns");
    assert!(!loot.warm_rule_patterns(2));

    // A spell requirement matches with an include and an exclude pattern.
    loot.settings.rules = vec![vtank_rule(
        "Spells",
        LootAction::Keep,
        vec![requirement(
            9,
            &["warm-ahead-include", "warm-ahead-exclude", "1"],
        )],
    )];
    assert!(loot.warm_rule_patterns(1));
    assert!(!loot.warm_rule_patterns(1));

    // Warming changes no decision: the warmed rule still matches by name.
    let named = item(7, "warm-ahead-include", 44);
    loot.settings.rules = vec![vtank_rule(
        "Name",
        LootAction::Keep,
        vec![requirement(1, &["warm-ahead-name-3", "1"])],
    )];
    let (decision, _) = decide(
        &item(8, "warm-ahead-name-3", 44),
        &empty(),
        &loot.settings.rules,
        &[],
        None,
        None,
    );
    assert_eq!(decision.map(|d| d.action), Some(LootAction::Keep));
    let (decision, _) = decide(&named, &empty(), &loot.settings.rules, &[], None, None);
    assert_eq!(decision, None);
}

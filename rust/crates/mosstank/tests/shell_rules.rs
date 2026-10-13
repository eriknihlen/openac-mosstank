//! The rule list the shell builds (`MossTankPanelTests` on the panel's
//! `MacroRules`): every slot of VTank's list is filled, in order, with the
//! C#'s rule kinds.

mod common;

use common::World;
use mosstank_meta::scheduler::{MACRO_RULE_TABLE, MacroRuleEntry, RuleShape};

fn shapes() -> Vec<(String, RuleShape)> {
    let mut world = World::with_character("Shell Tester");
    let plugin = world.enable();
    plugin.tank().unwrap().rule_shapes()
}

fn index_of(shapes: &[(String, RuleShape)], name: &str) -> usize {
    shapes
        .iter()
        .position(|(rule, _)| rule == name)
        .unwrap_or_else(|| panic!("{name} is not in the list"))
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: PanelFillsEveryPositionOfVtanksRuleList
#[test]
fn panel_fills_every_position_of_vtanks_rule_list() {
    let rules = shapes();

    assert_eq!(rules.len(), 66);
    let sentinels = MACRO_RULE_TABLE
        .iter()
        .filter(|entry| matches!(entry, MacroRuleEntry::Sentinel(_)))
        .count();
    assert_eq!(
        rules
            .iter()
            .filter(|(_, shape)| *shape == RuleShape::Sentinel)
            .count(),
        sentinels
    );
    assert_eq!(rules[65].0, "IdlePeace");
    assert_eq!(rules[64].0, "Sentinel END");
    for (name, shape) in &rules {
        if let RuleShape::Absent(reason) = shape {
            assert!(!reason.is_empty(), "{name} is absent without a reason");
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheDoorSlotHoldsALiveRule
#[test]
fn the_door_slot_holds_a_live_rule() {
    let rules = shapes();
    let door = index_of(&rules, "OpenDoor");
    assert_eq!(rules[door].1, RuleShape::Other("OpenDoorRule"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: WieldedManaRefillOutranksBuffSelf
#[test]
fn wielded_mana_refill_outranks_buff_self() {
    let rules = shapes();
    assert!(index_of(&rules, "RefillWieldedMana") < index_of(&rules, "BuffSelf"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: HelperRechargeSitsBelowBuffSelfAndSelfRechargeAboveIt
#[test]
fn helper_recharge_sits_below_buff_self_and_self_recharge_above_it() {
    let rules = shapes();
    let own = index_of(&rules, "RechargeSelfNormal");
    let buff = index_of(&rules, "BuffSelf");
    let helper = index_of(&rules, "UseHealersHeart");
    assert!(own < buff);
    assert!(buff < helper);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: BothSelfRechargeRowsAreLiveRules
#[test]
fn both_self_recharge_rows_are_live_rules() {
    let rules = shapes();
    let normal = index_of(&rules, "RechargeSelfNormal");
    let idle = index_of(&rules, "RechargeSelfNoTarget");
    assert_eq!(rules[normal].1, RuleShape::Controller);
    assert_eq!(rules[idle].1, RuleShape::Controller);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheMonsterApproachCarriesTheIdlePeaceFallback
#[test]
fn the_monster_approach_carries_the_idle_peace_fallback() {
    let rules = shapes();
    let approach = index_of(&rules, "NavigateMonster");
    assert_eq!(rules[approach].1, RuleShape::PreChain);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MonsterApproachSitsBelowIdleBuffAndAboveTheRoute
#[test]
fn monster_approach_sits_below_idle_buff_and_above_the_route() {
    let rules = shapes();
    let idle_buff = index_of(&rules, "BuffSelfIdle");
    let approach = index_of(&rules, "NavigateMonster");
    let route = index_of(&rules, "NavigateRouteIdle");
    assert!(idle_buff < approach);
    assert!(approach < route);
}

/// The stopped-macro list is the worn-mana rule alone, under its own
/// name, and a different rule from the running list's.
#[test]
fn the_stopped_macro_list_is_the_worn_mana_rule() {
    let mut world = World::with_character("Shell Tester");
    let plugin = world.enable();
    let tank = plugin.tank().unwrap();
    assert_eq!(tank.macro_disabled_rule_names(), ["RefillWieldedMana"]);
    let rules = tank.rule_shapes();
    let refill = index_of(&rules, "RefillWieldedMana");
    let recharge = index_of(&rules, "RechargeSelfNormal");
    let buff = index_of(&rules, "BuffSelf");
    assert!(recharge < refill && refill < buff);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPluginTests.cs :: EnableRegistersVtAndUbOnTheirOwnHandlersAndDisableRevokesBoth
#[test]
fn enable_registers_vt_and_ub_on_their_own_handlers_and_disable_revokes_both() {
    let mut world = World::with_character("Shell Tester");
    let mut plugin = world.enable();
    let verbs: Vec<String> = world
        .fake
        .commands
        .iter()
        .map(|command| command.verb.clone())
        .collect();
    assert_eq!(verbs, ["vt", "ub"]);

    // Each word reaches its own command set: an unknown word says which.
    world.vt(&mut plugin, "nosuchword");
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("Unknown /vt command. Use /vt help."))
    );

    world.disable(&mut plugin);
    assert!(world.fake.commands.is_empty());
}

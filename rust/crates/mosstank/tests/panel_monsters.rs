//! The Monsters page's grid, driven through the window bindings (ported
//! from the C#'s `MossTankPanelTests`).

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::combat::CombatTarget;
use ac_plugin_abi::world::{ObjectClass, WorldObject};
use common::{World, item};
use mosstank::MossTank;

fn fresh(name: &str) -> (World, MossTank) {
    let mut world = World::with_character(name);
    let plugin = world.enable();
    (world, plugin)
}

fn add_rule(world: &mut World, plugin: &mut MossTank, expression: &str) {
    world.act_text(plugin, "SetMonsterExpressionDraft", expression);
    world.act(plugin, "AddMonsterRule");
}

fn names(world: &mut World, plugin: &mut MossTank) -> Vec<String> {
    world.ui_list(plugin, "MonsterNameColumn")
}

fn cell(world: &mut World, plugin: &mut MossTank, column: &str, row: usize) -> String {
    world.ui_list(plugin, column)[row].clone()
}

fn flag(world: &mut World, plugin: &mut MossTank, column: &str, row: usize) -> bool {
    world.ui_bools(plugin, column)[row]
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MonstersGridMutationsPersistAcrossSessions
#[test]
fn grid_changes_persist_across_sessions() {
    let (mut world, mut first) = fresh("Rule Maker");
    let p = &mut first;
    add_rule(&mut world, p, "species==drudge");
    world.act_int(p, "ToggleMonsterImperilAt", 1);
    world.act_int(p, "CycleMonsterDamageAt", 1);
    world.act_int(p, "CycleMonsterPriorityAt", 1);
    assert_eq!(names(&mut world, p), ["DEFAULT", "species==drudge"]);
    assert!(flag(&mut world, p, "MonsterImperilColumn", 1));
    assert_eq!(cell(&mut world, p, "MonsterDamageColumn", 1), "Void Basic");
    assert_eq!(cell(&mut world, p, "MonsterPriorityColumn", 1), "2");
    assert_eq!(world.ui_text(p, "MonsterExpressionDraft"), "");
    world.disable(p);

    let mut second = world.enable();
    let p = &mut second;
    assert_eq!(names(&mut world, p), ["<DEFAULT>", "species==drudge"]);
    assert!(flag(&mut world, p, "MonsterImperilColumn", 1));
    assert_eq!(cell(&mut world, p, "MonsterDamageColumn", 1), "Void Basic");
    assert_eq!(cell(&mut world, p, "MonsterPriorityColumn", 1), "2");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ToggleMonsterFlagAtWritesOnlyTheTargetedRow
#[test]
fn a_flag_toggle_writes_only_its_row() {
    let (mut world, mut plugin) = fresh("Flagger");
    let p = &mut plugin;
    add_rule(&mut world, p, "drudge");
    assert!(!flag(&mut world, p, "MonsterImperilColumn", 0));
    assert!(!flag(&mut world, p, "MonsterImperilColumn", 1));
    world.act_int(p, "ToggleMonsterImperilAt", 1);
    assert!(!flag(&mut world, p, "MonsterImperilColumn", 0));
    assert!(flag(&mut world, p, "MonsterImperilColumn", 1));
    world.act_int(p, "ToggleMonsterImperilAt", 1);
    assert!(!flag(&mut world, p, "MonsterImperilColumn", 1));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: CycleMonsterPriorityAtWrapsExactlyNegativeOneThroughFour
#[test]
fn the_priority_cycles_minus_one_through_four() {
    let (mut world, mut plugin) = fresh("Prioritiser");
    let p = &mut plugin;
    assert_eq!(cell(&mut world, p, "MonsterPriorityColumn", 0), "1");
    for next in ["2", "3", "4", "-1", "0", "1"] {
        world.act_int(p, "CycleMonsterPriorityAt", 0);
        assert_eq!(cell(&mut world, p, "MonsterPriorityColumn", 0), next);
    }
}

fn cycles_exactly(
    world: &mut World,
    plugin: &mut MossTank,
    column: &str,
    handler: &str,
    expected: &[&str],
) {
    let mut guard = 0;
    while cell(world, plugin, column, 0) != expected[0] {
        world.act_int(plugin, handler, 0);
        guard += 1;
        assert!(
            guard <= expected.len(),
            "{column} never reached {}",
            expected[0]
        );
    }
    for next in expected.iter().skip(1).chain(std::iter::once(&expected[0])) {
        world.act_int(plugin, handler, 0);
        assert_eq!(cell(world, plugin, column, 0), *next, "{column}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MonsterDamageColumnsCycleInTheExactAuthenticOrder
#[test]
fn the_damage_columns_cycle_in_the_authentic_order() {
    let (mut world, mut plugin) = fresh("Cycler");
    let p = &mut plugin;
    cycles_exactly(
        &mut world,
        p,
        "MonsterDamageColumn",
        "CycleMonsterDamageAt",
        &[
            "Pierce",
            "Bludgeon",
            "Slash",
            "Acid",
            "Lightning",
            "Cold",
            "Fire",
            "Harm",
            "Auto",
            "Void Basic",
            "Drain Auto",
            "Prismatic",
            "Random",
            "Fists",
        ],
    );
    cycles_exactly(
        &mut world,
        p,
        "MonsterExtraVulnColumn",
        "CycleMonsterExtraVulnerabilityAt",
        &[
            "Pierce",
            "Bludgeon",
            "Slash",
            "Acid",
            "Lightning",
            "Cold",
            "Fire",
            "Auto",
            "None",
        ],
    );
    cycles_exactly(
        &mut world,
        p,
        "MonsterPetDamageColumn",
        "CycleMonsterPetDamageAt",
        &[
            "Pierce",
            "Bludgeon",
            "Slash",
            "Acid",
            "Lightning",
            "Cold",
            "Fire",
            "PAuto",
            "Auto",
            "None",
        ],
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheDefaultRowsExtraVulnerabilityIsEditableAndSurvivesAReload
#[test]
fn the_default_rows_extra_vulnerability_is_editable_and_survives_a_reload() {
    let (mut world, mut first) = fresh("Vuln Setter");
    let p = &mut first;
    assert_eq!(cell(&mut world, p, "MonsterNameColumn", 0), "DEFAULT");
    assert_eq!(cell(&mut world, p, "MonsterExtraVulnColumn", 0), "None");
    let mut guard = 0;
    while cell(&mut world, p, "MonsterExtraVulnColumn", 0) != "Auto" {
        world.act_int(p, "CycleMonsterExtraVulnerabilityAt", 0);
        guard += 1;
        assert!(guard <= 12);
    }
    world.disable(p);

    let mut second = world.enable();
    let p = &mut second;
    assert_eq!(cell(&mut world, p, "MonsterExtraVulnColumn", 0), "Auto");
    let mut guard = 0;
    while cell(&mut world, p, "MonsterExtraVulnColumn", 0) != "None" {
        world.act_int(p, "CycleMonsterExtraVulnerabilityAt", 0);
        guard += 1;
        assert!(guard <= 12);
    }
    world.disable(p);

    let mut third = world.enable();
    assert_eq!(
        cell(&mut world, &mut third, "MonsterExtraVulnColumn", 0),
        "None"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: DeleteMonsterRuleAtRemovesNonDefaultRowsButNeverDefault
#[test]
fn delete_removes_rows_but_never_the_default() {
    let (mut world, mut plugin) = fresh("Deleter");
    let p = &mut plugin;
    add_rule(&mut world, p, "drudge");
    add_rule(&mut world, p, "mosswart");
    assert_eq!(names(&mut world, p), ["DEFAULT", "drudge", "mosswart"]);
    world.act_int(p, "DeleteMonsterRuleAt", 0);
    assert_eq!(names(&mut world, p), ["DEFAULT", "drudge", "mosswart"]);
    world.act_int(p, "DeleteMonsterRuleAt", 1);
    assert_eq!(names(&mut world, p), ["DEFAULT", "mosswart"]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MoveMonsterRuleAtReordersButNeverDisplacesDefault
#[test]
fn moving_reorders_but_never_displaces_the_default() {
    let (mut world, mut plugin) = fresh("Mover");
    let p = &mut plugin;
    add_rule(&mut world, p, "drudge");
    add_rule(&mut world, p, "mosswart");
    world.act_int(p, "MoveMonsterRuleDownAt", 1);
    assert_eq!(names(&mut world, p), ["DEFAULT", "mosswart", "drudge"]);
    world.act_int(p, "MoveMonsterRuleUpAt", 2);
    assert_eq!(names(&mut world, p), ["DEFAULT", "drudge", "mosswart"]);
    world.act_int(p, "MoveMonsterRuleUpAt", 1);
    assert_eq!(names(&mut world, p), ["DEFAULT", "drudge", "mosswart"]);
    let up = world.ui_uints(p, "MonsterMoveUpIcons");
    assert_eq!(up.len(), 3);
    assert!(up.iter().all(|id| *id == 0x0600_28FC));
    assert!(
        world
            .ui_uints(p, "MonsterMoveDownIcons")
            .iter()
            .all(|id| *id == 0x0600_28FD)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: EveryNewMonsterRowStartsAtPriorityOneWithAttackAndStreak
#[test]
fn every_new_row_starts_at_priority_one_with_attack_and_streak() {
    let (mut world, mut plugin) = fresh("Fresh Rows");
    let p = &mut plugin;
    add_rule(&mut world, p, "species==drudge");
    assert_eq!(names(&mut world, p), ["DEFAULT", "species==drudge"]);
    for row in 0..2 {
        assert_eq!(cell(&mut world, p, "MonsterPriorityColumn", row), "1");
        assert!(flag(&mut world, p, "MonsterAttackColumn", row));
        assert!(flag(&mut world, p, "MonsterStreakColumn", row));
        assert!(!flag(&mut world, p, "MonsterRingColumn", row));
        assert!(!flag(&mut world, p, "MonsterImperilColumn", row));
        assert!(!flag(&mut world, p, "MonsterVulnerabilityColumn", row));
    }
}

fn hostile(world: &mut World, object_id: u32, name: &str) {
    world.set_reply::<calls::CombatHostileTargets>(&vec![CombatTarget {
        object_id,
        name: name.to_owned(),
        weenie_class_id: 700,
        distance: 2.0,
        is_health_known: true,
        health_fraction: 1.0,
        ..CombatTarget::default()
    }]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AddMonsterRuleUsesTheDraftTextAndAddSelectedMonsterUsesTheWorldTarget
#[test]
fn add_uses_the_draft_and_add_selected_uses_the_world_target() {
    let (mut world, mut plugin) = fresh("Adder");
    let p = &mut plugin;
    hostile(&mut world, 30, "Drudge");
    add_rule(&mut world, p, "species==drudge");
    assert_eq!(names(&mut world, p), ["DEFAULT", "species==drudge"]);
    assert_eq!(world.ui_text(p, "MonsterExpressionDraft"), "");
    world.select_object(30);
    world.act(p, "AddSelectedMonster");
    assert_eq!(
        names(&mut world, p),
        ["DEFAULT", "species==drudge", "Drudge"]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AddWithAnEmptyExpressionTakesTheSelectedCreaturesName
#[test]
fn add_with_an_empty_expression_takes_the_selected_creatures_name() {
    let (mut world, mut plugin) = fresh("Namer");
    let p = &mut plugin;
    hostile(&mut world, 30, "Drudge Slinker");
    world.select_object(30);
    world.act(p, "AddMonsterRule");
    assert_eq!(names(&mut world, p), ["DEFAULT", "Drudge Slinker"]);
    assert!(
        world
            .ui_text(p, "MonsterEditorNotice")
            .contains("Drudge Slinker")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AddWithNoExpressionAndNoSelectionAddsNothingAndSaysSo
#[test]
fn add_with_nothing_typed_or_selected_adds_nothing_and_says_so() {
    let (mut world, mut plugin) = fresh("Nobody");
    let p = &mut plugin;
    world.act(p, "AddMonsterRule");
    assert_eq!(names(&mut world, p), ["DEFAULT"]);
    assert_eq!(
        world.ui_text(p, "MonsterEditorNotice"),
        "Select a monster in the world first."
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AddSelectedMonsterFallsBackToTheObjectTableForANonHostileCreature
#[test]
fn add_selected_falls_back_to_the_object_table_for_a_peaceful_creature() {
    let (mut world, mut plugin) = fresh("Peaceful");
    let p = &mut plugin;
    let sclavus = WorldObject {
        object_id: 42,
        name: "Sclavus".to_owned(),
        object_class: ObjectClass::Monster,
        ..WorldObject::default()
    };
    world.set_reply::<calls::WorldObjects>(&vec![sclavus.clone()]);
    world.set_reply::<calls::WorldObject>(&Some(sclavus));
    world.select_object(42);
    world.act(p, "AddSelectedMonster");
    assert_eq!(names(&mut world, p), ["DEFAULT", "Sclavus"]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MonsterWeaponColumnCyclesTheRegisteredRosterAndPersistsByNameAcrossSessions
#[test]
fn the_weapon_column_cycles_the_roster_and_persists_by_name() {
    let (mut world, mut first) = fresh("Rule Maker");
    let p = &mut first;
    world.own_items(vec![item(10, "Fire Sword", 1)]);
    world.select_object(10);
    world.act(p, "AddSelectedItem");
    assert_eq!(cell(&mut world, p, "MonsterWeaponColumn", 0), "<AUTO>");
    world.act_int(p, "CycleMonsterWeaponAt", 0);
    assert_eq!(cell(&mut world, p, "MonsterWeaponColumn", 0), "Fire Sword");
    world.act_int(p, "CycleMonsterWeaponAt", 0);
    assert_eq!(cell(&mut world, p, "MonsterWeaponColumn", 0), "<AUTO>");
    world.act_int(p, "CycleMonsterWeaponAt", 0);
    assert_eq!(cell(&mut world, p, "MonsterWeaponColumn", 0), "Fire Sword");
    world.disable(p);

    world.own_items(vec![item(99, "Fire Sword", 1)]);
    let mut second = world.enable();
    assert_eq!(
        cell(&mut world, &mut second, "MonsterWeaponColumn", 0),
        "Fire Sword"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AMonsterWeaponCellNamingAnUnlistedItemReadsInvalid
#[test]
fn a_weapon_cell_naming_an_unlisted_item_reads_invalid() {
    let (mut world, mut plugin) = fresh("Rule Maker");
    let p = &mut plugin;
    world.own_items(vec![
        item(0x8000_13FA, "Blade", 1),
        item(0x8000_13FB, "Sword", 1),
    ]);
    world.select_object(0x8000_13FA);
    world.act(p, "AddSelectedItem");
    world.select_object(0x8000_13FB);
    world.act(p, "AddSelectedItem");
    let mut guard = 0;
    while cell(&mut world, p, "MonsterWeaponColumn", 0) != "Blade" {
        world.act_int(p, "CycleMonsterWeaponAt", 0);
        guard += 1;
        assert!(guard <= 6, "the weapon cycle never reached Blade");
    }
    let row = world.ui_index(p, "ItemNameColumn", "Blade");
    world.act_int(p, "SelectItemRow", row);
    world.act(p, "RemoveSelectedItem");
    assert_eq!(
        cell(&mut world, p, "MonsterWeaponColumn", 0),
        "<INVALID 0x800013FA>"
    );
}

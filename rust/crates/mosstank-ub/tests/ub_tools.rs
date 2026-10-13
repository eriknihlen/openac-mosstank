//! The UB tools a command starts: stacking and cramming
//! (`InventoryMaintenanceTests`), spending experience
//! (`ExperienceSpendControllerTests`) and dressing by profile
//! (`EquipProfileControllerTests`), with the panel tests that reach them by
//! `/ub`.

mod common;

use std::collections::HashSet;

use ac_plugin_abi::character::{
    AdvancementKind, AdvancementResult, AdvancementStatus, Attribute, Skill, Training, Vital,
};
use ac_plugin_abi::items::{
    EquipmentItem, EquipmentObservation, InventoryCommandKind, InventoryCompletion, InventoryItem,
};
use ac_plugin_abi::world::{ItemProperties, ObjectClass, WorldObject};
use common::Rig;
use mosstank_automation::locks::ActionLockKind;
use mosstank_formats::utl::{self, LootAction, LootRule, VtankLootRequirement};
use mosstank_loot::rules;
use mosstank_settings::model::InventorySettings;
use mosstank_settings::ub::UbSettingValue;
use mosstank_stores::storage::NullLog;
use mosstank_ub::catalog::UbCatalog;
use mosstank_ub::chat;
use mosstank_ub::testing::FakeUbHost;
use mosstank_ub::tools::stack_cram::{
    self, InventoryMaintenanceController, InventoryMaintenanceKind, StackCramCommandRun,
};
use openac_plugin_compat::regex::{DotNetRegex, RegexOptions};

const PLAYER: u32 = 1;

// ---- InventoryMaintenanceTests --------------------------------------------------

fn item(id: u32, name: &str, wcid: u32, container: u32, stack: i32, maximum: i32) -> InventoryItem {
    InventoryItem {
        object_id: id,
        weenie_class_id: wcid,
        name: name.to_owned(),
        item_type: 0x80,
        container_id: container,
        stack_size: stack,
        maximum_stack_size: maximum,
        ..InventoryItem::default()
    }
}

fn plain(id: u32, name: &str, container: u32) -> InventoryItem {
    item(id, name, 0, container, 1, 1)
}

fn arrows(first: i32, second: i32) -> Vec<InventoryItem> {
    vec![
        item(20, "Arrow", 77, PLAYER, first, 10),
        item(21, "Arrow", 77, PLAYER, second, 10),
    ]
}

/// The C# fake: in the world, the item commands up, a merge leaves them
/// busy until the receipt.
fn maintenance_host(inventory: Vec<InventoryItem>) -> FakeUbHost {
    let mut host = FakeUbHost::new();
    host.meta.item_state.is_available = true;
    host.meta.owned_items = inventory;
    host.merge_makes_busy = true;
    host
}

fn receipt(host: &mut FakeUbHost, revision: i64, weenie_error: u32) {
    host.meta.item_state.is_busy = false;
    host.meta.item_state.last_inventory_completion = InventoryCompletion {
        revision,
        kind: InventoryCommandKind::Merge,
        source_id: 20,
        weenie_error,
    };
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryMaintenanceTests.cs :: PlannerAlwaysStacksBeforeCramming
#[test]
fn planner_always_stacks_before_cramming() {
    let settings = InventorySettings {
        auto_stack: true,
        auto_cram: true,
        ..InventorySettings::default()
    };
    let pack = InventoryItem {
        items_capacity: 24,
        container_slot: 1,
        burden: 8,
        ..plain(10, "Pack", PLAYER)
    };
    let source = InventoryItem {
        container_slot: 2,
        burden: 1,
        ..item(20, "Arrow", 77, PLAYER, 10, 10)
    };
    let target = InventoryItem {
        container_slot: 0,
        burden: 1,
        ..item(21, "Arrow", 77, 10, 6, 10)
    };

    let plan = stack_cram::plan(&[pack, source, target], PLAYER, &settings, &HashSet::new())
        .expect("a plan");
    assert_eq!(InventoryMaintenanceKind::Merge, plan.kind);
    assert_eq!(20, plan.source_object_id);
    assert_eq!(21, plan.target_object_id);
    assert_eq!(4, plan.amount);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryMaintenanceTests.cs :: CramMovesOneMainPackItemIntoFirstSidePackWithRoom
#[test]
fn cram_moves_one_main_pack_item_into_first_side_pack_with_room() {
    let settings = InventorySettings {
        auto_stack: false,
        auto_cram: true,
        ..InventorySettings::default()
    };
    let full = InventoryItem {
        items_capacity: 1,
        container_slot: 0,
        ..plain(10, "Full pack", PLAYER)
    };
    let destination = InventoryItem {
        items_capacity: 2,
        container_slot: 1,
        ..plain(11, "Open pack", PLAYER)
    };
    let occupant = plain(12, "Occupant", 10);
    let source = InventoryItem {
        container_slot: 4,
        ..plain(20, "Loose item", PLAYER)
    };
    let foci = InventoryItem {
        container_slot: 2,
        public_flags: 0x0080_0000,
        ..plain(21, "Focus", PLAYER)
    };

    let plan = stack_cram::plan(
        &[full, destination, occupant, foci, source],
        PLAYER,
        &settings,
        &HashSet::new(),
    )
    .expect("a plan");
    assert_eq!(InventoryMaintenanceKind::Cram, plan.kind);
    assert_eq!(20, plan.source_object_id);
    assert_eq!(11, plan.target_object_id);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryMaintenanceTests.cs :: ControllerWaitsForAuthoritativeReceiptBeforeReplanning
#[test]
fn controller_waits_for_authoritative_receipt_before_replanning() {
    let mut host = maintenance_host(arrows(3, 8));
    let settings = InventorySettings {
        auto_stack: true,
        ..InventorySettings::default()
    };
    let mut controller = InventoryMaintenanceController::new();

    assert!(controller.tick(&mut host, &settings, 1.0, true));
    assert_eq!(vec![(20, 21, 2)], host.merges);

    assert!(controller.tick(&mut host, &settings, 1.0, true));
    assert_eq!(1, host.merges.len());

    receipt(&mut host, 1, 0);
    host.meta.owned_items = vec![item(21, "Arrow", 77, PLAYER, 10, 10)];

    assert!(!controller.tick(&mut host, &settings, 1.0, true));
    assert_eq!(1, host.merges.len());
    assert_eq!("Stack/Cram idle", controller.status());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryMaintenanceTests.cs :: AStuckStackCramPairIsAbandonedOnlyAfterEightyAttempts
#[test]
fn a_stuck_stack_cram_pair_is_abandoned_only_after_eighty_attempts() {
    let settings = InventorySettings {
        auto_stack: true,
        ..InventorySettings::default()
    };
    let mut host = maintenance_host(arrows(10, 6));
    let mut controller = InventoryMaintenanceController::new();

    // Each pass observes the previous pass's refusal and then tries again.
    for attempt in 1..=81 {
        assert!(controller.tick(&mut host, &settings, 1.0, true));
        assert_eq!(attempt, host.merges.len());
        receipt(&mut host, i64::try_from(attempt).unwrap(), 0x10);
    }
    assert!(host.posted_text().is_empty());

    // The eighty-first refusal is the one it gives up on.
    assert!(!controller.tick(&mut host, &settings, 1.0, true));
    assert_eq!(
        vec!["[MossTank] Abandoned trying to stack/cram two bugged items.".to_owned()],
        host.posted_text()
    );
    assert_eq!(81, host.merges.len());
}

fn stack_rig(inventory: Vec<InventoryItem>) -> Rig {
    let mut rig = Rig::new();
    rig.host.meta.item_state.is_available = true;
    rig.host.meta.owned_items = inventory;
    rig.host.merge_makes_busy = true;
    rig.host.meta.character_mut().id = PLAYER;
    rig
}

fn run_tick(rig: &mut Rig, run: &mut StackCramCommandRun) -> bool {
    rig.with_call(|call, _| run.tick(call, 1.0, true))
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryMaintenanceTests.cs :: AStackPassByCommandRunsUntilNothingIsLeftAndSaysSo
#[test]
fn a_stack_pass_by_command_runs_until_nothing_is_left_and_says_so() {
    let mut rig = stack_rig(arrows(3, 8));
    let mut run = StackCramCommandRun::new();

    rig.with_call(|call, _| run.start(call, true));
    assert_eq!(vec!["[UB] AutoStack running".to_owned()], rig.messages());
    assert!(run_tick(&mut rig, &mut run));
    assert_eq!(vec![(20, 21, 2)], rig.host.merges);

    receipt(&mut rig.host, 1, 0);
    rig.host.meta.owned_items = vec![item(21, "Arrow", 77, PLAYER, 10, 10)];

    assert!(!run_tick(&mut rig, &mut run));
    assert!(!run.is_running());
    assert_eq!(
        vec![
            "[UB] AutoStack running".to_owned(),
            "[UB] AutoStack complete.".to_owned()
        ],
        rig.messages()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryMaintenanceTests.cs :: APassWithNothingToDoSaysSoAndDoesNotStart
#[test]
fn a_pass_with_nothing_to_do_says_so_and_does_not_start() {
    let mut rig = stack_rig(arrows(3, 8));
    let mut run = StackCramCommandRun::new();

    rig.with_call(|call, _| run.start(call, false));

    assert_eq!(
        vec!["[UB] AutoCram - nothing to do".to_owned()],
        rig.messages()
    );
    assert!(!run.is_running());
    assert!(!run_tick(&mut rig, &mut run));
    assert!(rig.host.merges.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: AutoStackAndAutoCramSayWhenThereIsNothingToDo
#[test]
fn auto_stack_and_auto_cram_say_when_there_is_nothing_to_do() {
    let mut rig = Rig::new();
    rig.ub("autostack");
    rig.ub("autocram");
    assert_eq!(
        vec![
            "[UB] AutoStack - nothing to do".to_owned(),
            "[UB] AutoCram - nothing to do".to_owned()
        ],
        rig.messages()
    );
}

// ---- ExperienceSpendControllerTests ----------------------------------------------

const DEFAULT_POLICY: [&str; 3] = ["Strength = 1", "Health = 1.4", "WarMagic = 10"];

struct XpWorld {
    unassigned: i64,
    luminance: i64,
    health_stat_id: u32,
    strength_stat_id: u32,
    war_magic_training: Training,
}

impl Default for XpWorld {
    fn default() -> Self {
        Self {
            unassigned: 300,
            luminance: 0,
            health_stat_id: 1,
            strength_stat_id: 1,
            war_magic_training: Training::Specialized,
        }
    }
}

/// The C# test's fake: Strength (no ranks), Endurance (five), Health (no
/// ranks), War Magic (no ranks), the banked experience and luminance.
fn xp_rig(world: &XpWorld, policy: &[&str]) -> Rig {
    let mut rig = Rig::new();
    rig.host.meta.character_mut().id = 1;
    rig.host.meta.attributes = vec![
        Attribute {
            kind: 0,
            name: "Strength".to_owned(),
            current: 100,
            stat_id: world.strength_stat_id,
            ranks: 0,
            ..Attribute::default()
        },
        Attribute {
            kind: 1,
            name: "Endurance".to_owned(),
            current: 100,
            stat_id: 2,
            ranks: 5,
            ..Attribute::default()
        },
    ];
    rig.host.vitals = vec![Vital {
        kind: 0,
        name: "Health".to_owned(),
        current: 100,
        maximum: 100,
        stat_id: world.health_stat_id,
        ranks: 0,
        ..Vital::default()
    }];
    rig.host.meta.skills = vec![Skill {
        skill_id: 34,
        name: "War Magic".to_owned(),
        training: world.war_magic_training,
        current: 100,
        ranks: 0,
        ..Skill::default()
    }];
    rig.host.meta.properties.insert(
        1,
        ItemProperties {
            int64s: vec![(2, world.unassigned), (6, world.luminance)],
            ..ItemProperties::default()
        },
    );
    set_policy(&mut rig, policy);
    rig
}

fn set_policy(rig: &mut Rig, policy: &[&str]) {
    let row = UbCatalog::require(&rig.catalog, "AutoXp.Policy");
    rig.catalog
        .set(&row, UbSettingValue::from_collection(policy));
}

fn policy(rig: &Rig) -> Vec<String> {
    rig.catalog.value("AutoXp.Policy").items
}

fn xp(rig: &mut Rig, arguments: &str) -> Option<Vec<String>> {
    rig.with_call(|call, commands| commands.tools_mut().xp.command(call, arguments))
}

fn xp_tick(rig: &mut Rig, seconds: f64) -> bool {
    rig.with_call(|call, commands| commands.tools_mut().xp.tick(call, seconds))
}

fn xp_running(rig: &mut Rig) -> bool {
    rig.commands.tools().xp.is_running()
}

fn costs(rig: &Rig) -> Vec<u64> {
    rig.host.advances.iter().map(|(_, _, cost)| *cost).collect()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExperienceSpendControllerTests.cs :: LevelSendsChunksAndSlowSendsOneRankPerInterval
#[test]
fn level_sends_chunks_and_slow_sends_one_rank_per_interval() {
    let mut rig = xp_rig(&XpWorld::default(), &DEFAULT_POLICY);

    assert!(xp(&mut rig, "level").unwrap()[0].contains("1 step"));
    assert!(xp_running(&mut rig));
    xp_tick(&mut rig, 0.0);
    assert_eq!(vec![(AdvancementKind::Skill, 34, 282)], rig.host.advances);
    xp_tick(&mut rig, 0.1);
    assert!(!xp_running(&mut rig));
    assert!(rig.commands.tools().xp.status().contains("1 target"));

    rig.host.advances.clear();
    assert!(xp(&mut rig, "slow").unwrap()[0].contains("6 step"));
    xp_tick(&mut rig, 0.0);
    assert_eq!(vec![(AdvancementKind::Skill, 34, 23)], rig.host.advances);
    xp_tick(&mut rig, 0.1);
    assert_eq!(1, rig.host.advances.len());
    xp_tick(&mut rig, 0.25);
    assert_eq!(33, rig.host.advances[1].2);
    for _ in 0..4 {
        xp_tick(&mut rig, 0.3);
    }
    assert_eq!(vec![23, 33, 41, 52, 62, 71], costs(&rig));
    xp_tick(&mut rig, 0.3);
    assert!(!xp_running(&mut rig));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExperienceSpendControllerTests.cs :: PlanReadsExperienceNotLuminance
#[test]
fn plan_reads_experience_not_luminance() {
    let mut rig = xp_rig(
        &XpWorld {
            unassigned: 300,
            luminance: 0,
            ..XpWorld::default()
        },
        &DEFAULT_POLICY,
    );
    assert!(xp(&mut rig, "level").unwrap()[0].contains("1 step"));
    xp_tick(&mut rig, 0.0);
    assert_eq!(vec![(AdvancementKind::Skill, 34, 282)], rig.host.advances);

    let mut rich = xp_rig(
        &XpWorld {
            unassigned: 0,
            luminance: 5_000,
            ..XpWorld::default()
        },
        &DEFAULT_POLICY,
    );
    xp(&mut rich, "level");
    xp_tick(&mut rich, 0.0);
    assert!(rich.host.advances.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExperienceSpendControllerTests.cs :: StatIdsComeFromTheHostRecordWithTheCatalogueAsFallback
#[test]
fn stat_ids_come_from_the_host_record_with_the_catalogue_as_fallback() {
    let mut rig = xp_rig(
        &XpWorld {
            unassigned: 200,
            health_stat_id: 42,
            strength_stat_id: 0,
            war_magic_training: Training::Untrained,
            ..XpWorld::default()
        },
        &["Strength = 1", "Health = 1"],
    );

    xp(&mut rig, "slow");
    for _ in 0..3 {
        xp_tick(&mut rig, 0.3);
    }
    assert_eq!(
        vec![
            (AdvancementKind::Vital, 42, 73),
            (AdvancementKind::Attribute, 1, 110)
        ],
        rig.host.advances
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExperienceSpendControllerTests.cs :: RefusesWhileTheMacroIsEnabled
#[test]
fn refuses_while_the_macro_is_enabled() {
    let mut rig = xp_rig(&XpWorld::default(), &DEFAULT_POLICY);
    rig.services.macro_enabled = true;

    assert!(
        xp(&mut rig, "level").unwrap()[0]
            .to_lowercase()
            .contains("macro")
    );
    assert!(!xp_running(&mut rig));
    assert!(rig.host.advances.is_empty());

    rig.services.macro_enabled = false;
    xp(&mut rig, "slow");
    xp_tick(&mut rig, 0.0);
    assert_eq!(1, rig.host.advances.len());
    rig.services.macro_enabled = true;
    xp_tick(&mut rig, 0.3);
    assert!(!xp_running(&mut rig));
    assert_eq!(1, rig.host.advances.len());
    assert!(
        rig.commands
            .tools()
            .xp
            .status()
            .to_lowercase()
            .contains("macro")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExperienceSpendControllerTests.cs :: ARefusalStopsTheRunAndASecondCommandHaltsIt
#[test]
fn a_refusal_stops_the_run_and_a_second_command_halts_it() {
    let mut rig = xp_rig(&XpWorld::default(), &DEFAULT_POLICY);
    rig.host.advance_result = AdvancementResult {
        status: AdvancementStatus::Refused,
        notice: Some("not now".to_owned()),
    };

    xp(&mut rig, "slow");
    xp_tick(&mut rig, 0.0);
    assert_eq!(1, rig.host.advances.len());
    assert!(!xp_running(&mut rig));
    assert!(rig.commands.tools().xp.status().contains("not now"));

    rig.host.advance_result = AdvancementResult {
        status: AdvancementStatus::Sent,
        notice: None,
    };
    xp(&mut rig, "slow");
    assert!(xp_running(&mut rig));
    rig.clear_messages();
    assert!(xp(&mut rig, "level").unwrap().is_empty());
    assert!(!xp_running(&mut rig));
    // The reference's stop says it is stopping, then how far it got.
    assert_eq!(
        vec![
            "[UB] Stopping AutoXp.".to_owned(),
            "[UB] Finished leveling 0 targets.".to_owned()
        ],
        rig.messages()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ExperienceSpendControllerTests.cs :: TestExportAndImportSpeakTheReferenceFormat
#[test]
fn test_export_and_import_speak_the_reference_format() {
    let mut rig = xp_rig(&XpWorld::default(), &DEFAULT_POLICY);

    let weights = xp(&mut rig, "").unwrap();
    assert!(weights.iter().any(|line| line.contains("WarMagic: 10")));
    assert!(
        xp(&mut rig, "test")
            .unwrap()
            .iter()
            .any(|line| line.contains("WarMagic: 6 levels for 282 xp"))
    );
    assert_eq!(
        "[UB] Strength=1;Health=1.4;WarMagic=10",
        xp(&mut rig, "export").unwrap()[0]
    );

    let imported = xp(&mut rig, "import Alchemy=2;Bogus=1;WarMagic=0").unwrap();
    assert!(imported.iter().any(|line| line.contains("Bogus")));
    assert_eq!(
        vec![
            "Strength = 1".to_owned(),
            "Health = 1.4".to_owned(),
            "WarMagic = 0".to_owned(),
            "Alchemy = 2".to_owned()
        ],
        policy(&rig)
    );
    assert_eq!(
        "[UB] Strength=1;Health=1.4;WarMagic=0;Alchemy=2",
        xp(&mut rig, "export").unwrap()[0]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: ExperienceCommandsReadAndWriteThePolicyRowOnThePage
#[test]
fn experience_commands_read_and_write_the_policy_row_on_the_page() {
    let mut rig = Rig::new();
    rig.ub("xp import Alchemy=7;Bogus=1");
    rig.ub("xp export");

    let messages = rig.messages();
    assert!(messages.iter().any(|line| line.contains("Bogus")));
    let export = messages
        .iter()
        .rev()
        .find(|line| line.starts_with("[UB] Strength="))
        .expect("the export");
    assert!(export.contains("Alchemy=7"));
    assert!(export.starts_with("[UB] Strength=1;Endurance=1;"));
}

// ---- EquipProfileControllerTests --------------------------------------------------

const SWORD: u32 = 10;
const BRACELET: u32 = 11;
const HAUBERK: u32 = 12;
const RATION: u32 = 13;
const BRACELET_SLOT: u32 = 0x0004_0000;
const HAUBERK_SLOT: u32 = 0x0000_0004;
const WEAPON_SLOT: u32 = 0x0100_0000;
const LORE_REQUIREMENT_PROPERTY: u32 = 109;

fn wearable(id: u32, name: &str, valid_locations: u32, equipped: u32) -> InventoryItem {
    InventoryItem {
        object_id: id,
        weenie_class_id: id + 2000,
        name: name.to_owned(),
        item_type: 0x2,
        container_id: PLAYER,
        wielder_id: if equipped == 0 { 0 } else { PLAYER },
        valid_locations,
        equipped_location: equipped,
        stack_size: 1,
        value: 100,
        maximum_stack_size: 1,
        object_class: ObjectClass::Armor,
        ..InventoryItem::default()
    }
}

fn consumable(id: u32, name: &str) -> InventoryItem {
    InventoryItem {
        object_id: id,
        weenie_class_id: id + 2000,
        name: name.to_owned(),
        item_type: 0x20,
        container_id: PLAYER,
        stack_size: 1,
        value: 5,
        maximum_stack_size: 25,
        object_class: ObjectClass::Food,
        ..InventoryItem::default()
    }
}

fn props(ints: &[(u32, i32)]) -> ItemProperties {
    ItemProperties {
        ints: ints.to_vec(),
        ..ItemProperties::default()
    }
}

fn rule(name: &str, expression: &str, action: LootAction) -> LootRule {
    let mut rule = LootRule::default();
    name.clone_into(&mut rule.name);
    rule.action = action;
    rule.set_expression(expression);
    rule
}

/// Keep everything named "Set A ..."; leave the rest.
fn write_set_a_profile(rig: &Rig, key: &str) {
    rig.host.profiles.insert(
        key,
        &utl::serialize_rules(&[
            rule("set a", "name ~= Set A", LootAction::Keep),
            rule("rest", "*", LootAction::NoLoot),
        ]),
    );
}

/// The C# rig: "Acdream" on Coldeve, the UB store bound to them over the
/// profile folder, and the appraised set.
struct EquipRig {
    rig: Rig,
    appraised: HashSet<u32>,
}

impl EquipRig {
    fn new() -> Self {
        let mut rig = Rig::new();
        {
            let character = rig.host.meta.character_mut();
            character.id = PLAYER;
            "Acdream".clone_into(&mut character.name);
            "Coldeve".clone_into(&mut character.world);
        }
        rig.host.meta.item_state.is_available = true;
        let mut profiles = rig.host.profiles.clone();
        rig.services.ub_store.bind(
            &mut profiles,
            &mut NullLog,
            Some("Coldeve"),
            Some("Acdream"),
        );
        Self {
            rig,
            appraised: HashSet::new(),
        }
    }

    fn owned(&mut self, items: Vec<InventoryItem>) {
        self.rig.host.meta.owned_items = items;
    }

    /// The world records and the equipment list follow the packs, as the C#
    /// fake derives them.
    fn sync(&mut self) {
        let owned = self.rig.host.meta.owned_items.clone();
        self.rig.host.meta.objects = owned
            .iter()
            .map(|item| WorldObject {
                object_id: item.object_id,
                weenie_class_id: item.weenie_class_id,
                name: item.name.clone(),
                object_class: item.object_class,
                item_type: item.item_type,
                container_id: item.container_id,
                wielder_id: item.wielder_id,
                has_appraisal_data: self.appraised.contains(&item.object_id),
                is_owned: true,
                ..WorldObject::default()
            })
            .collect();
        self.rig.host.meta.equipment = owned
            .iter()
            .filter(|item| item.valid_locations != 0)
            .map(|item| EquipmentItem {
                object_id: item.object_id,
                name: item.name.clone(),
                item_type: item.item_type,
                valid_locations: item.valid_locations,
                equipped_location: item.equipped_location,
                container_id: item.container_id,
                wielder_id: item.wielder_id,
                ..EquipmentItem::default()
            })
            .collect();
    }

    fn command(&mut self, arguments: &str) -> Option<Vec<String>> {
        self.sync();
        self.rig
            .with_call(|call, commands| commands.tools_mut().equip.command(call, arguments))
    }

    fn tick(&mut self, seconds: f64) -> bool {
        self.sync();
        self.rig.locks.borrow_mut().advance(seconds);
        self.rig
            .with_call(|call, commands| commands.tools_mut().equip.tick(call, seconds, true))
    }

    fn running(&self) -> bool {
        self.rig.commands.tools().equip.is_running()
    }

    fn observe(&mut self, object_id: u32, location: u32, is_removal: bool) {
        for item in &mut self.rig.host.meta.owned_items {
            if item.object_id == object_id {
                item.equipped_location = location;
                item.wielder_id = if location == 0 { 0 } else { PLAYER };
            }
        }
        self.rig
            .commands
            .tools_mut()
            .equip
            .on_equipment_placement(EquipmentObservation {
                object_id,
                equipped_location: location,
                is_removal,
                is_initial_placement: false,
            });
    }

    fn equip_calls(&self) -> Vec<(u32, u32)> {
        self.rig.host.equips.clone()
    }

    fn move_calls(&self) -> Vec<(u32, u32)> {
        self.rig
            .host
            .meta
            .moves
            .iter()
            .map(|(item, container, ..)| (*item, *container))
            .collect()
    }

    /// Every line shown and every line submitted (`automation.Messages`).
    fn messages(&self) -> Vec<String> {
        let mut lines = self.rig.messages();
        lines.extend(self.rig.submitted());
        lines
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/EquipProfileControllerTests.cs :: LoadIsRefusedWhileTheCombatControllerHasATarget
#[test]
fn load_is_refused_while_the_combat_controller_has_a_target() {
    let mut equip = EquipRig::new();
    equip.owned(vec![wearable(BRACELET, "Set A Bracelet", BRACELET_SLOT, 0)]);
    equip.rig.services.combat_has_target = true;
    write_set_a_profile(&equip.rig, "mosstank/ub/equip/suit.utl");

    let lines = equip.command("load suit").unwrap();

    assert!(!equip.running());
    assert!(lines[0].to_lowercase().contains("monster"));
    assert!(!equip.tick(0.1));
    assert!(equip.equip_calls().is_empty());
    assert!(equip.move_calls().is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/EquipProfileControllerTests.cs :: TheFinishedLineIsThoughtAsARealTell
#[test]
fn the_finished_line_is_thought_as_a_real_tell() {
    let mut equip = EquipRig::new();
    let row = UbCatalog::require(&equip.rig.catalog, "EquipmentManager.Think");
    equip.rig.catalog.set(&row, UbSettingValue::from_bool(true));
    write_set_a_profile(&equip.rig, "mosstank/ub/equip/suit.utl");

    assert!(equip.command("load suit").unwrap().is_empty());
    for _ in 0..10 {
        if !equip.running() {
            break;
        }
        equip.tick(0.1);
    }

    assert!(!equip.running());
    assert!(equip.rig.submitted().iter().any(|line| {
        line.starts_with("/t Acdream, Equipment Manager: Finished equipping items in ")
    }));
    assert!(
        !equip
            .messages()
            .iter()
            .any(|line| line.starts_with("You think"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/EquipProfileControllerTests.cs :: EachRefusalIsSaidTheWayItsCauseIs
#[test]
fn each_refusal_is_said_the_way_its_cause_is() {
    let mut equip = EquipRig::new();

    assert!(equip.command("load suit").unwrap().is_empty());

    equip
        .rig
        .host
        .profiles
        .insert("mosstank/ub/equip/broken.utl", "not a loot profile");
    let lines = equip.command("load broken").unwrap();
    assert_eq!(
        vec![
            "[UB] Error: EquipmentManager: Equip profile could not be read: mosstank/ub/equip/broken.utl"
                .to_owned()
        ],
        lines
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/EquipProfileControllerTests.cs :: TheFinishedLineWritesItsSecondsAsTheReferenceDoes
#[test]
fn the_finished_line_writes_its_seconds_as_the_reference_does() {
    let mut equip = EquipRig::new();
    write_set_a_profile(&equip.rig, "mosstank/ub/equip/suit.utl");

    assert!(equip.command("load suit").unwrap().is_empty());
    for _ in 0..10 {
        if !equip.running() {
            break;
        }
        equip.tick(1.0);
    }

    let finished: Vec<String> = equip
        .messages()
        .into_iter()
        .filter(|text| text.contains("Finished equipping"))
        .collect();
    assert_eq!(1, finished.len());
    let pattern = DotNetRegex::new(
        r"^\[UB\] Equipment Manager: Finished equipping items in [1-9]\d*s$",
        RegexOptions::NONE,
    )
    .unwrap();
    assert!(pattern.is_match(&finished[0]).unwrap(), "{}", finished[0]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/EquipProfileControllerTests.cs :: LoadDequipsThenEquipsInLoreOrderUnderTheItemLock
#[test]
fn load_dequips_then_equips_in_lore_order_under_the_item_lock() {
    let mut equip = EquipRig::new();
    equip.owned(vec![
        wearable(SWORD, "Sword", WEAPON_SLOT, WEAPON_SLOT),
        wearable(HAUBERK, "Set A Hauberk", HAUBERK_SLOT, 0),
        wearable(BRACELET, "Set A Bracelet", BRACELET_SLOT, 0),
        consumable(RATION, "Set A Ration"),
    ]);
    equip
        .rig
        .host
        .meta
        .properties
        .insert(HAUBERK, props(&[(LORE_REQUIREMENT_PROPERTY, 0)]));
    equip
        .rig
        .host
        .meta
        .properties
        .insert(BRACELET, props(&[(LORE_REQUIREMENT_PROPERTY, 50)]));
    equip.appraised.extend([SWORD, HAUBERK, BRACELET]);
    write_set_a_profile(&equip.rig, "mosstank/ub/equip/suit.utl");

    assert!(equip.command("load suit").unwrap().is_empty());
    assert!(equip.running());

    assert!(equip.tick(0.1));
    assert!(
        equip
            .rig
            .locks
            .borrow_mut()
            .is_locked(ActionLockKind::ItemUse)
    );
    assert_eq!(vec![(SWORD, PLAYER)], equip.move_calls());
    assert!(equip.equip_calls().is_empty());

    // Nothing more until the sword is seen leaving its slot.
    assert!(equip.tick(0.1));
    assert_eq!(1, equip.move_calls().len());
    equip.observe(SWORD, 0, true);

    assert!(equip.tick(0.1));
    assert_eq!(vec![(HAUBERK, 0)], equip.equip_calls());
    assert!(equip.tick(0.1));
    assert_eq!(1, equip.equip_calls().len());
    equip.observe(HAUBERK, HAUBERK_SLOT, false);

    assert!(equip.tick(0.1));
    assert_eq!(vec![(HAUBERK, 0), (BRACELET, 0)], equip.equip_calls());
    equip.observe(BRACELET, BRACELET_SLOT, false);

    assert!(!equip.tick(0.1));
    assert!(!equip.running());
    assert!(
        !equip
            .rig
            .locks
            .borrow_mut()
            .is_locked(ActionLockKind::ItemUse)
    );
    assert!(
        equip
            .messages()
            .iter()
            .any(|line| line.contains("Finished equipping"))
    );
    // The ration matched by name but cannot be worn; it is never sent.
    assert!(!equip.equip_calls().iter().any(|(item, _)| *item == RATION));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/EquipProfileControllerTests.cs :: ItemsTheRulesCannotDecideAreIdentifiedFirst
#[test]
fn items_the_rules_cannot_decide_are_identified_first() {
    let mut equip = EquipRig::new();
    equip.owned(vec![wearable(HAUBERK, "Hauberk", HAUBERK_SLOT, 0)]);
    // Armor level 300 or better: an appraisal-only key.
    let mut good_armor = rule("good armor", "*", LootAction::Keep);
    good_armor.has_imported_requirements = true;
    good_armor.vtank_requirements = vec![VtankLootRequirement {
        kind: 3,
        payload: "300\r\n28\r\n".to_owned(),
    }];
    equip.rig.host.profiles.insert(
        "mosstank/ub/equip/suit.utl",
        &utl::serialize_rules(&[good_armor]),
    );

    equip.command("load suit");
    assert!(equip.tick(0.1));
    assert_eq!(vec![HAUBERK], equip.rig.host.identified);
    assert!(equip.equip_calls().is_empty());

    assert!(equip.tick(1.0));
    assert_eq!(1, equip.rig.host.identified.len());

    equip
        .rig
        .host
        .meta
        .properties
        .insert(HAUBERK, props(&[(28, 320)]));
    equip.appraised.insert(HAUBERK);
    assert!(equip.tick(0.1));
    assert!(equip.tick(0.1));
    assert_eq!(vec![(HAUBERK, 0)], equip.equip_calls());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/EquipProfileControllerTests.cs :: TwoRingsAreEquippedWithNoSlotRequestSoTheClientPicksTheFreeFinger
#[test]
fn two_rings_are_equipped_with_no_slot_request_so_the_client_picks_the_free_finger() {
    const LEFT_RING: u32 = 14;
    const RIGHT_RING: u32 = 15;
    const BOTH_FINGERS: u32 = 0x000C_0000;
    let mut equip = EquipRig::new();
    equip.owned(vec![
        wearable(LEFT_RING, "Set A Ring of Dawn", BOTH_FINGERS, 0),
        wearable(RIGHT_RING, "Set A Ring of Dusk", BOTH_FINGERS, 0),
    ]);
    equip.appraised.extend([LEFT_RING, RIGHT_RING]);
    write_set_a_profile(&equip.rig, "mosstank/ub/equip/rings.utl");

    assert!(equip.command("load rings").unwrap().is_empty());
    assert!(equip.tick(0.1));
    assert_eq!(1, equip.equip_calls().len());
    assert_eq!(0, equip.equip_calls()[0].1);
    let first = equip.equip_calls()[0].0;
    equip.observe(first, 0x0004_0000, false);

    assert!(equip.tick(0.1));
    assert_eq!(2, equip.equip_calls().len());
    assert_eq!(0, equip.equip_calls()[1].1);
    assert_ne!(first, equip.equip_calls()[1].0);
    let second = equip.equip_calls()[1].0;
    equip.observe(second, 0x0008_0000, false);

    assert!(!equip.tick(0.1));
    assert!(!equip.running());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/EquipProfileControllerTests.cs :: TestReportsTheOrderAndTouchesNothing
#[test]
fn test_reports_the_order_and_touches_nothing() {
    let mut equip = EquipRig::new();
    equip.owned(vec![
        wearable(SWORD, "Sword", WEAPON_SLOT, WEAPON_SLOT),
        wearable(HAUBERK, "Set A Hauberk", HAUBERK_SLOT, 0),
        wearable(BRACELET, "Set A Bracelet", BRACELET_SLOT, 0),
    ]);
    equip
        .rig
        .host
        .meta
        .properties
        .insert(BRACELET, props(&[(LORE_REQUIREMENT_PROPERTY, 50)]));
    equip.appraised.extend([SWORD, HAUBERK, BRACELET]);
    write_set_a_profile(&equip.rig, "mosstank/ub/equip/suit.utl");

    equip.command("test suit");
    assert!(!equip.tick(0.1));
    assert!(!equip.running());
    assert!(equip.equip_calls().is_empty());
    assert!(equip.move_calls().is_empty());
    assert!(
        !equip
            .rig
            .locks
            .borrow_mut()
            .is_locked(ActionLockKind::ItemUse)
    );
    let messages = equip.messages();
    let bracelet = messages
        .iter()
        .position(|line| line.contains("Set A Bracelet"));
    let hauberk = messages
        .iter()
        .position(|line| line.contains("Set A Hauberk"));
    assert!(
        matches!((hauberk, bracelet), (Some(h), Some(b)) if b > h),
        "{messages:?}"
    );
}

fn parse(text: &str) -> Vec<LootRule> {
    utl::try_parse_rules(Some(text)).expect("the profile reads back")
}

fn decides(owned: &[InventoryItem], index: usize, rules_read: &[LootRule]) -> bool {
    rules::decide(
        &owned[index],
        &ItemProperties::default(),
        rules_read,
        owned,
        None,
        None,
    )
    .0
    .is_some()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/EquipProfileControllerTests.cs :: CreateWritesTheWornSetAndConfirmsBeforeOverwriting
#[test]
fn create_writes_the_worn_set_and_confirms_before_overwriting() {
    let mut equip = EquipRig::new();
    let owned = vec![
        wearable(SWORD, "Sword", WEAPON_SLOT, WEAPON_SLOT),
        wearable(BRACELET, "Set A Bracelet", BRACELET_SLOT, BRACELET_SLOT),
        wearable(HAUBERK, "Set A Hauberk", HAUBERK_SLOT, 0),
    ];
    equip.owned(owned.clone());
    const KEY: &str = "mosstank/ub/Coldeve/Acdream/equip/mine.utl";

    assert!(
        equip.command("create mine").unwrap()[0]
            .to_lowercase()
            .contains("created")
    );
    let first = equip.rig.host.profiles.get(KEY).expect("the profile");
    let read = parse(&first);
    assert_eq!(2, read.len());
    assert!(decides(&owned, 0, &read));
    assert!(decides(&owned, 1, &read));
    assert!(!decides(&owned, 2, &read));

    // Now the hauberk is worn instead; the file must not change yet.
    equip.owned(vec![wearable(
        HAUBERK,
        "Set A Hauberk",
        HAUBERK_SLOT,
        HAUBERK_SLOT,
    )]);
    let second = equip.command("create mine").unwrap();
    assert!(second[0].to_lowercase().contains("already exists"));
    assert_eq!(Some(first.clone()), equip.rig.host.profiles.get(KEY));

    equip.tick(1.0);
    assert!(
        equip.command("create mine").unwrap()[0]
            .to_lowercase()
            .contains("created")
    );
    let again = equip.rig.host.profiles.get(KEY).expect("the profile");
    assert_ne!(first, again);
    assert_eq!(1, parse(&again).len());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/EquipProfileControllerTests.cs :: AnOverwriteConfirmationExpires
#[test]
fn an_overwrite_confirmation_expires() {
    let mut equip = EquipRig::new();
    equip.owned(vec![wearable(SWORD, "Sword", WEAPON_SLOT, WEAPON_SLOT)]);
    const KEY: &str = "mosstank/ub/Coldeve/Acdream/equip/mine.utl";

    equip.command("create mine");
    let first = equip.rig.host.profiles.get(KEY).expect("the profile");
    equip.command("create mine");
    equip.tick(31.0);
    assert!(
        equip.command("create mine").unwrap()[0]
            .to_lowercase()
            .contains("already exists")
    );
    assert_eq!(Some(first), equip.rig.host.profiles.get(KEY));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/EquipProfileControllerTests.cs :: ProfilesResolveByTierAndListShowsEveryDepth
#[test]
fn profiles_resolve_by_tier_and_list_shows_every_depth() {
    let mut equip = EquipRig::new();
    equip.owned(vec![wearable(BRACELET, "Set A Bracelet", BRACELET_SLOT, 0)]);
    equip.appraised.insert(BRACELET);

    // A missing profile is said only in the debug output; the refusal
    // itself is the status.
    assert!(equip.command("load suit").unwrap().is_empty());
    assert!(
        equip
            .rig
            .commands
            .tools()
            .equip
            .status()
            .starts_with("No equip profile")
    );
    assert!(!equip.running());

    write_set_a_profile(&equip.rig, "mosstank/ub/Coldeve/equip/default.utl");
    assert!(equip.command("test suit").unwrap().is_empty());
    assert!(equip.running());
    let locks = equip.rig.locks.clone();
    equip
        .rig
        .commands
        .tools_mut()
        .equip
        .reset(&mut locks.borrow_mut());

    write_set_a_profile(&equip.rig, "mosstank/ub/Coldeve/Acdream/equip/suit.utl");
    write_set_a_profile(&equip.rig, "mosstank/ub/equip/other.utl");
    let listing = equip.command("list").unwrap();
    assert!(
        listing
            .iter()
            .any(|line| line.contains("suit.utl") && line.contains("Coldeve/Acdream"))
    );
    assert!(listing.iter().any(|line| line.contains("default.utl")));
    assert!(listing.iter().any(|line| line.contains("other.utl")));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/EquipProfileControllerTests.cs :: ARunWithNoProgressBailsAndReleasesTheLock
#[test]
fn a_run_with_no_progress_bails_and_releases_the_lock() {
    let mut equip = EquipRig::new();
    equip.owned(vec![wearable(BRACELET, "Set A Bracelet", BRACELET_SLOT, 0)]);
    equip.appraised.insert(BRACELET);
    write_set_a_profile(&equip.rig, "mosstank/ub/equip/suit.utl");

    equip.command("load suit");
    assert!(equip.tick(0.1));
    assert_eq!(1, equip.equip_calls().len());
    assert!(
        equip
            .rig
            .locks
            .borrow_mut()
            .is_locked(ActionLockKind::ItemUse)
    );

    assert!(equip.tick(5.0));
    assert!(!equip.tick(6.0));
    assert!(!equip.running());
    assert!(
        !equip
            .rig
            .locks
            .borrow_mut()
            .is_locked(ActionLockKind::ItemUse)
    );
    assert!(
        equip
            .messages()
            .iter()
            .any(|line| line.to_lowercase().contains("timeout"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: EquipCommandsRouteThroughThePanelToChat
#[test]
fn equip_commands_route_through_the_panel_to_chat() {
    let mut equip = EquipRig::new();
    equip.rig.ub("equip list");
    equip.rig.ub("equip load nothing");

    assert!(
        equip
            .rig
            .posted()
            .contains(&("[UB] Equip Profiles:".to_owned(), chat::GENERIC_CHAT_TYPE))
    );
    assert!(
        !equip
            .rig
            .messages()
            .iter()
            .any(|line| line.contains("No equip profile exists"))
    );

    equip.rig.ub("opt set Plugin.Debug true");
    equip.rig.clear_messages();
    equip.rig.ub("equip load nothing");

    let posted = equip.rig.posted();
    assert_eq!(1, posted.len(), "{posted:?}");
    assert!(
        posted[0]
            .0
            .starts_with("[UB] EquipmentManager: No equip profile exists: ")
    );
    assert!(posted[0].0.contains("nothing.utl"));
    assert_eq!(chat::DEBUG_CHAT_TYPE, posted[0].1);
}

// ---- the name tags over the page's rows -----------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbSettingsTab.cs :: TheNametagSwitchOnThePageTakesTheLabelsBackFromTheHost
#[test]
fn the_nametag_switch_on_the_page_takes_the_labels_back_from_the_host() {
    use ac_plugin_abi::world::Position;
    use mosstank_ub::tools::nametags::{CatalogNametagSettings, NametagController};

    let mut rig = Rig::new().at_heading(0.0);
    rig.host.meta.character_mut().id = 0x5000_0001;
    rig.host.meta.objects = vec![WorldObject {
        object_id: 0x8000_0001,
        weenie_class_id: 1,
        name: "Drudge Slinker".to_owned(),
        object_class: ObjectClass::Monster,
        is_landscape: true,
        position: Some(Position {
            east_west: 0.01,
            is_outdoor: true,
            ..Position::default()
        }),
        ..WorldObject::default()
    }];
    let mut tags = NametagController::new(&CatalogNametagSettings(&rig.catalog));
    let frame = |rig: &mut Rig, tags: &mut NametagController| {
        tags.on_tick(&mut rig.host, &CatalogNametagSettings(&rig.catalog), 0.1);
    };

    frame(&mut rig, &mut tags);
    assert_eq!(1, rig.host.label_sets.len());
    assert_eq!("Drudge Slinker", rig.host.label_sets[0][0].text);

    let row = UbCatalog::require(&rig.catalog, "Nametags.Enabled");
    rig.catalog.set(&row, UbSettingValue::from_bool(false));
    for _ in 0..3 {
        frame(&mut rig, &mut tags);
    }
    assert_eq!(2, rig.host.label_sets.len());
    assert!(rig.host.label_sets[1].is_empty());

    rig.catalog.set(&row, UbSettingValue::from_bool(true));
    for _ in 0..3 {
        frame(&mut rig, &mut tags);
    }
    assert_eq!(3, rig.host.label_sets.len());
    assert_eq!(1, rig.host.label_sets[2].len());
    assert_eq!("Drudge Slinker", rig.host.label_sets[2][0].text);
}

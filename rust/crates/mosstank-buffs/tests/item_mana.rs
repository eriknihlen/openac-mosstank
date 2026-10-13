//! Topping up worn gear's mana: the planner and the controller.

use std::collections::HashMap;

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::Skill;
use ac_plugin_abi::combat::{CombatMode, CombatResult, CombatStatus};
use ac_plugin_abi::items::{InventoryItem, ItemResult, ItemStatus, ItemUseCompletion};
use ac_plugin_abi::magic::CastGate;
use ac_plugin_abi::types::ChatLine;
use ac_plugin_abi::world::{ItemProperties, ObjectClass, WorldObject};
use mosstank_automation::locks::{ActionLockKind, ActionLockTable};
use mosstank_automation::{Automation, SpellCatalog, StaticSpellCatalog};
use mosstank_buffs::mana::{self, ItemManaRechargeController, ItemManaRechargePlan, ManaTurn};
use mosstank_dotnet::collections::OrdinalStringMap;
use mosstank_settings::model::{CombatSettings, ConsumableCategory, InventorySettings};

/// The test file's client (`ItemManaRechargeTests.Surface`): appraisals
/// and their property bags are what the test sets, every identify and
/// apply is taken and recorded.
#[derive(Default)]
struct Surface {
    chat_messages: Vec<ChatLine>,
    objects: HashMap<u32, WorldObject>,
    properties: HashMap<u32, ItemProperties>,
    inventory: Vec<InventoryItem>,
    last_item_completion: ItemUseCompletion,
    apply_calls: Vec<(u32, u32)>,
    identify_requests: Vec<u32>,
    property_captures: Vec<u32>,
    spells: StaticSpellCatalog,
    infos: Vec<String>,
}

impl Surface {
    fn of(inventory: Vec<InventoryItem>) -> Self {
        Self {
            inventory,
            ..Self::default()
        }
    }

    fn assess(&mut self, item: &InventoryItem, version: i32, ints: &[(u32, i32)]) {
        self.objects.insert(
            item.object_id,
            WorldObject {
                object_id: item.object_id,
                weenie_class_id: item.weenie_class_id,
                name: item.name.clone(),
                object_class: item.object_class,
                item_type: item.item_type,
                container_id: item.container_id,
                wielder_id: item.wielder_id,
                is_owned: true,
                has_appraisal_data: true,
                last_id_time: version,
                ..WorldObject::default()
            },
        );
        self.properties.insert(
            item.object_id,
            ItemProperties {
                ints: ints.to_vec(),
                ..ItemProperties::default()
            },
        );
    }

    fn replace_assessment_version(&mut self, object_id: u32, version: i32) {
        self.objects
            .get_mut(&object_id)
            .expect("an assessed object")
            .last_id_time = version;
    }

    fn info_count(&self, matches: impl Fn(&str) -> bool) -> usize {
        self.infos.iter().filter(|line| matches(line)).count()
    }
}

impl Automation for Surface {
    fn is_available(&mut self) -> bool {
        true
    }

    fn character(&mut self) -> CharacterInfo {
        CharacterInfo {
            id: 1,
            health: (100, 100, 0),
            stamina: (100, 100, 0),
            mana: (100, 100, 0),
            ..CharacterInfo::default()
        }
    }

    fn skills(&mut self) -> Vec<Skill> {
        Vec::new()
    }

    fn spells(&mut self) -> &mut dyn SpellCatalog {
        &mut self.spells
    }

    fn evaluate_gate(&mut self, _spell_id: u32, _target: Option<u32>) -> CastGate {
        CastGate::Unavailable
    }

    fn cast(&mut self, _spell_id: u32, _target: Option<u32>) -> bool {
        false
    }

    fn combat_mode(&mut self) -> CombatMode {
        CombatMode::Unknown
    }

    fn enter_combat_mode(&mut self, _mode: CombatMode) -> CombatResult {
        CombatResult {
            status: CombatStatus::Unavailable,
            notice: None,
        }
    }

    fn items_available(&mut self) -> bool {
        true
    }

    fn owned_items(&mut self) -> Vec<InventoryItem> {
        self.inventory.clone()
    }

    fn last_item_completion(&mut self) -> ItemUseCompletion {
        self.last_item_completion.clone()
    }

    fn use_item(&mut self, _object_id: u32) -> ItemResult {
        ItemResult {
            status: ItemStatus::Unavailable,
            notice: None,
        }
    }

    fn apply_item(&mut self, object_id: u32, target_id: u32) -> ItemResult {
        self.apply_calls.push((object_id, target_id));
        ItemResult {
            status: ItemStatus::Started,
            notice: None,
        }
    }

    fn item_properties(&mut self, object_id: u32) -> Option<ItemProperties> {
        self.property_captures.push(object_id);
        self.properties.get(&object_id).cloned()
    }

    fn objects_available(&mut self) -> bool {
        true
    }

    fn object(&mut self, object_id: u32) -> Option<WorldObject> {
        self.objects.get(&object_id).cloned()
    }

    fn identify(&mut self, object_id: u32) -> ItemResult {
        self.identify_requests.push(object_id);
        ItemResult {
            status: ItemStatus::Started,
            notice: None,
        }
    }

    fn chat_messages(&mut self, after_sequence: u64) -> Vec<ChatLine> {
        self.chat_messages
            .iter()
            .filter(|message| message.sequence > after_sequence)
            .cloned()
            .collect()
    }

    fn post_system_message(&mut self, _text: &str) {}

    fn log_info(&mut self, text: &str) {
        self.infos.push(text.to_owned());
    }

    fn log_warn(&mut self, _text: &str) {}
}

fn item(id: u32, name: &str) -> InventoryItem {
    item_of_type(id, name, 0x80)
}

fn item_of_type(id: u32, name: &str, item_type: u32) -> InventoryItem {
    InventoryItem {
        object_id: id,
        name: name.to_owned(),
        item_type,
        container_id: 1,
        stack_size: 1,
        ..InventoryItem::default()
    }
}

fn worn(id: u32, name: &str, location: u32, current: i32, maximum: i32) -> InventoryItem {
    InventoryItem {
        equipped_location: location,
        item_current_mana: current,
        item_maximum_mana: maximum,
        ..item(id, name)
    }
}

/// A charged one-shot charge the planner knows by its profile kind.
fn mana_charge(id: u32) -> InventoryItem {
    InventoryItem {
        item_current_mana: 100,
        effects: 0x0000_0001,
        ..item_of_type(id, "Mana Charge", 0x0008_0000)
    }
}

fn kinds(rows: &[(&str, ConsumableCategory)]) -> OrdinalStringMap<ConsumableCategory> {
    let mut map = OrdinalStringMap::new();
    for &(name, kind) in rows {
        map.insert(name.to_owned(), kind);
    }
    map
}

fn plan(
    inventory: &[InventoryItem],
    kinds: &OrdinalStringMap<ConsumableCategory>,
    threshold: i32,
    wield_order: Option<&[u32]>,
) -> (Option<ItemManaRechargePlan>, bool) {
    mana::plan(inventory, kinds, threshold, wield_order, None)
}

fn refill_settings(percent: i32) -> InventorySettings {
    InventorySettings {
        refill_worn_mana: true,
        refill_worn_mana_percent: percent,
        ..InventorySettings::default()
    }
}

fn profiled(rows: &[(&str, ConsumableCategory)], named: bool) -> CombatSettings {
    let mut profiles = CombatSettings::default();
    for &(name, kind) in rows {
        if named {
            profiles.consumable_names.insert(name.to_owned());
        }
        profiles.consumable_categories.insert(name.to_owned(), kind);
    }
    profiles
}

/// One controller with the client and settings it is ticked with.
struct Rig {
    surface: Surface,
    settings: InventorySettings,
    profiles: CombatSettings,
    controller: ItemManaRechargeController,
}

impl Rig {
    fn new(surface: Surface, settings: InventorySettings, profiles: CombatSettings) -> Self {
        Self {
            surface,
            settings,
            profiles,
            controller: ItemManaRechargeController::new(0),
        }
    }

    fn tick(&mut self, elapsed: f64) -> bool {
        let mut turn = ManaTurn {
            automation: &mut self.surface,
            settings: &self.settings,
            profiles: &self.profiles,
        };
        self.controller.tick(&mut turn, true, elapsed)
    }

    fn observe(&mut self, elapsed: f64) {
        let mut turn = ManaTurn {
            automation: &mut self.surface,
            settings: &self.settings,
            profiles: &self.profiles,
        };
        self.controller.observe_pending_receipt(&mut turn, elapsed);
    }

    fn status(&self) -> &str {
        self.controller.status()
    }
}

fn receipt(revision: i64, source_id: u32, target_id: u32, error: u32) -> ItemUseCompletion {
    ItemUseCompletion {
        revision,
        source_id,
        target_id,
        weenie_error: error,
    }
}

fn refill_line(sequence: u64, items: &str) -> ChatLine {
    ChatLine {
        sequence,
        text: format!("The Mana Stone gives 1,200 points of mana to the following items: {items}"),
        ..ChatLine::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ItemManaRechargeTests.cs :: PlannerUsesProfiledManaChargeOnMostDepletedWornItem
#[test]
fn planner_uses_profiled_mana_charge_on_most_depleted_worn_item() {
    let names = kinds(&[("Mana Charge", ConsumableCategory::ManaSource)]);
    let (plan, _) = plan(
        &[
            mana_charge(10),
            worn(20, "Low Wand", 0x0100_0000, 10, 100),
            worn(21, "Other Wand", 0x0100_0000, 20, 100),
        ],
        &names,
        33,
        None,
    );
    let plan = plan.expect("a plan");
    assert_eq!(plan.charge_object_id, 10);
    assert_eq!(plan.target_object_id, 20);
    assert_eq!(plan.current_mana, 10);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ItemManaRechargeTests.cs :: TheOldestQueuedWornItemIsChargedFirstNotTheMostDepleted
#[test]
fn the_oldest_queued_worn_item_is_charged_first_not_the_most_depleted() {
    let names = kinds(&[("Mana Charge", ConsumableCategory::ManaSource)]);
    let inventory = [
        mana_charge(10),
        worn(20, "Low Wand", 0x0100_0000, 10, 100),
        worn(21, "Older Wand", 0x0200_0000, 20, 100),
    ];
    let queued = plan(&inventory, &names, 33, Some(&[21, 20]))
        .0
        .expect("a plan");
    assert_eq!(queued.target_object_id, 21);
    let depleted = plan(&inventory, &names, 33, None).0.expect("a plan");
    assert_eq!(depleted.target_object_id, 20);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ItemManaRechargeTests.cs :: PlannerRequiresProfileMembershipAndBelowThreshold
#[test]
fn planner_requires_profile_membership_and_below_threshold() {
    let charge = mana_charge(10);
    let wand = worn(20, "Wand", 0x0100_0000, 34, 100);

    // Nothing in the profile names the charge: a want and no source.
    let low_wand = InventoryItem {
        item_current_mana: 10,
        ..wand.clone()
    };
    let (none, missing) = plan(&[charge.clone(), low_wand], &kinds(&[]), 33, None);
    assert!(none.is_none());
    assert!(missing);
    // The charge is named, but the wand is above the threshold.
    let (none, missing) = plan(
        &[charge, wand],
        &kinds(&[("Mana Charge", ConsumableCategory::ManaSource)]),
        33,
        None,
    );
    assert!(none.is_none());
    assert!(!missing);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ItemManaRechargeTests.cs :: SuccessfulPlayerRefillCannotReuseStalePositiveCharge
#[test]
fn successful_player_refill_cannot_reuse_stale_positive_charge() {
    let charge = mana_charge(10);
    let missing_current_mana = worn(19, "Unready Helm", 0x0000_0001, 0, 100);
    let threshold_item = worn(20, "Low Gloves", 0x0000_0002, 10, 100);
    let mut surface = Surface::of(vec![
        charge.clone(),
        missing_current_mana.clone(),
        threshold_item.clone(),
    ]);
    surface.assess(&charge, 100, &[(107, 100)]);
    surface.assess(&missing_current_mana, 100, &[(108, 100)]);
    surface.assess(&threshold_item, 100, &[(107, 10), (108, 100)]);
    let profiles = profiled(&[("Mana Charge", ConsumableCategory::ManaSource)], true);
    let mut rig = Rig::new(surface, refill_settings(33), profiles);

    // Worn gear is asked about first, one question at a time, in turn.
    assert!(!rig.tick(0.0));
    assert_eq!(rig.status(), "Waiting for item assessment");
    assert_eq!(rig.surface.identify_requests, [19]);
    assert!(rig.surface.apply_calls.is_empty());
    rig.surface.replace_assessment_version(19, 101);
    assert!(!rig.tick(0.0));
    assert_eq!(rig.surface.identify_requests, [19, 20]);
    rig.surface.replace_assessment_version(20, 101);
    rig.surface.identify_requests.clear();

    assert!(rig.tick(0.0));
    assert_eq!(rig.surface.apply_calls, [(10, 1)]);
    assert!(
        rig.surface
            .info_count(|line| line.contains("thresholdItem=Low Gloves (0x00000014)"))
            >= 1
    );
    assert!(rig.surface.property_captures.contains(&19));
    assert!(rig.surface.property_captures.contains(&20));

    assert!(rig.tick(0.0));
    assert_eq!(rig.surface.apply_calls, [(10, 1)]);

    rig.surface.last_item_completion = receipt(1, 999, 1, 0);
    assert!(rig.tick(0.0));
    assert_eq!(rig.surface.apply_calls, [(10, 1)]);

    rig.surface.last_item_completion = receipt(2, charge.object_id, 1, 0);
    // The completion can arrive before the live charged-effect update; the
    // spent source stays blocked while every cached value still looks like
    // the charged stone.
    assert!(!rig.tick(0.0));
    assert_eq!(rig.surface.apply_calls, [(10, 1)]);
    rig.surface
        .replace_assessment_version(charge.object_id, 101);
    assert!(!rig.tick(0.0));
    assert_eq!(rig.surface.apply_calls, [(10, 1)]);
    rig.surface.inventory = vec![
        InventoryItem {
            effects: 0,
            ..charge.clone()
        },
        missing_current_mana.clone(),
        threshold_item.clone(),
    ];

    assert!(!rig.tick(0.0));
    // The spent charge, and the worn gear the refill made a stranger of.
    assert_eq!(rig.surface.identify_requests, [10, 19]);
    assert_eq!(rig.surface.apply_calls, [(10, 1)]);

    // A newer stamp alone is not enough: an empty stone's omitted 107 can
    // stay merged in.
    rig.surface
        .replace_assessment_version(charge.object_id, 101);
    assert!(!rig.tick(0.0));
    assert_eq!(rig.surface.apply_calls, [(10, 1)]);
    assert!(
        rig.surface.info_count(|line| line.contains("success=True")
            && line.contains("expected=0x00000001, actual=0x00000001"))
            >= 1
    );

    // A real empty-to-charged transition may refill to exactly the old
    // amount.
    rig.surface.inventory = vec![charge.clone(), missing_current_mana, threshold_item];
    assert!(!rig.tick(0.0)); // Still the appraisal from before charging.
    // The worn gear answers the questions the refill made it owe.
    rig.surface.replace_assessment_version(19, 102);
    assert!(!rig.tick(0.0));
    rig.surface.replace_assessment_version(20, 102);
    assert!(!rig.tick(0.0));
    rig.surface
        .replace_assessment_version(charge.object_id, 102);
    assert!(rig.tick(0.0));
    assert_eq!(rig.surface.apply_calls, [(10, 1), (10, 1)]);
    rig.surface.last_item_completion = receipt(3, 10, 999, 0);
    assert!(rig.tick(0.0));
    assert!(!rig.tick(15.0));
    assert!(rig.status().contains("unconfirmed"), "{}", rig.status());
    assert!(!rig.tick(0.0));
    assert_eq!(rig.surface.apply_calls, [(10, 1), (10, 1)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ItemManaRechargeTests.cs :: EmptyStoneWithStalePositiveManaIsNeverARefillCandidate
#[test]
fn empty_stone_with_stale_positive_mana_is_never_a_refill_candidate() {
    let empty_stone = InventoryItem {
        object_class: ObjectClass::ManaStone,
        item_current_mana: 100,
        effects: 0,
        ..item_of_type(10, "Mana Stone", 0x0008_0000)
    };
    let armor = worn(20, "Worn Armor", 1, 10, 100);
    let (none, missing) = plan(
        &[empty_stone, armor],
        &kinds(&[("Mana Stone", ConsumableCategory::ManaStone)]),
        33,
        None,
    );
    assert!(none.is_none());
    assert!(missing);
}

fn gem_charge() -> InventoryItem {
    InventoryItem {
        object_class: ObjectClass::Gem,
        ..item(11, "Great Mana Charge")
    }
}

fn charged_stone() -> InventoryItem {
    InventoryItem {
        object_class: ObjectClass::ManaStone,
        effects: 0x0000_0001,
        ..item(12, "Major Mana Stone")
    }
}

const STONE_AND_CHARGE: [(&str, ConsumableCategory); 2] = [
    ("Great Mana Charge", ConsumableCategory::ManaSource),
    ("Major Mana Stone", ConsumableCategory::ManaStone),
];

// Source: tests/AcDream.Plugins.MossTank.Tests/ItemManaRechargeTests.cs :: AChargedStoneOutranksACharge_AndACharge_IsAnyKindSixItem
#[test]
fn a_charged_stone_outranks_a_charge_and_a_charge_is_any_kind_six_item() {
    let wand = worn(20, "Low Wand", 1, 10, 100);
    let kinds = kinds(&STONE_AND_CHARGE);
    let both = plan(
        &[gem_charge(), charged_stone(), wand.clone()],
        &kinds,
        33,
        None,
    )
    .0
    .expect("a plan");
    assert_eq!(both.charge_object_id, 12);
    let charge_only = plan(&[gem_charge(), wand], &kinds, 33, None)
        .0
        .expect("a plan");
    assert_eq!(charge_only.charge_object_id, 11);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ItemManaRechargeTests.cs :: TheControllerSpendsAStoneWhenThereIsOneAndACharge_Otherwise
#[test]
fn the_controller_spends_a_stone_when_there_is_one_and_a_charge_otherwise() {
    for (holding_a_stone, expected) in [(true, 12), (false, 11)] {
        let wand = worn(20, "Low Wand", 0x0000_0002, 10, 100);
        let inventory = if holding_a_stone {
            vec![gem_charge(), charged_stone(), wand.clone()]
        } else {
            vec![gem_charge(), wand.clone()]
        };
        let mut surface = Surface::of(inventory);
        surface.assess(&wand, 100, &[(107, 10), (108, 100)]);
        let mut rig = Rig::new(
            surface,
            refill_settings(33),
            profiled(&STONE_AND_CHARGE, false),
        );

        assert!(!rig.tick(0.0));
        assert_eq!(rig.surface.identify_requests, [20]);
        rig.surface.replace_assessment_version(20, 101);

        assert!(rig.tick(0.0));
        assert_eq!(
            rig.surface.apply_calls,
            [(expected, 1)],
            "stone {holding_a_stone}"
        );
        assert!(
            rig.surface
                .info_count(|line| line == "Item Low Wand low on mana, 10%, 10/100")
                >= 1
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ItemManaRechargeTests.cs :: TheRefillPercentageIsTheProfilesOwnNumber
#[test]
fn the_refill_percentage_is_the_profiles_own_number() {
    for (threshold, refills) in [(50, true), (30, false)] {
        let charge = mana_charge(10);
        let wand = worn(20, "Half Wand", 0x0000_0002, 40, 100);
        let mut surface = Surface::of(vec![charge.clone(), wand.clone()]);
        surface.assess(&charge, 100, &[(107, 100)]);
        surface.assess(&wand, 100, &[(107, 40), (108, 100)]);
        let mut rig = Rig::new(
            surface,
            refill_settings(threshold),
            profiled(&[("Mana Charge", ConsumableCategory::ManaSource)], true),
        );

        // The gear is asked about first; one question at a time.
        assert!(!rig.tick(0.0));
        rig.surface.replace_assessment_version(20, 101);

        assert_eq!(rig.tick(0.0), refills, "threshold {threshold}");
        assert_eq!(!rig.surface.apply_calls.is_empty(), refills);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ItemManaRechargeTests.cs :: NothingToSpendOnLowGearIsSaidOnceARun
#[test]
fn nothing_to_spend_on_low_gear_is_said_once_a_run() {
    let wand = worn(20, "Low Wand", 0x0000_0002, 10, 100);
    let mut surface = Surface::of(vec![wand.clone()]);
    surface.assess(&wand, 100, &[(107, 10), (108, 100)]);
    let mut rig = Rig::new(surface, refill_settings(33), CombatSettings::default());

    assert!(!rig.tick(0.0));
    rig.surface.replace_assessment_version(20, 101);
    for _ in 0..5 {
        assert!(!rig.tick(0.3));
    }

    assert_eq!(rig.status(), "No mana charge or stone to spend");
    let warnings = |surface: &Surface| {
        surface.info_count(|line| line.starts_with("Warning: No mana charges/stones available"))
    };
    assert_eq!(warnings(&rig.surface), 1);
    // The item's own line is said once too, while it says the same thing.
    assert_eq!(
        rig.surface
            .info_count(|line| line == "Item Low Wand low on mana, 10%, 10/100"),
        1
    );

    rig.controller.reset_once_per_run_warnings();
    assert!(!rig.tick(0.3));
    assert_eq!(warnings(&rig.surface), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ItemManaRechargeTests.cs :: ARefillEndsEveryReadingOfWornGear
#[test]
fn a_refill_ends_every_reading_of_worn_gear() {
    for through_the_spoken_line in [false, true] {
        let charges = [mana_charge(10), mana_charge(11), mana_charge(12)];
        let gauntlets = worn(20, "Low Gauntlets", 0x0000_0002, 10, 100);
        let mut inventory = charges.to_vec();
        inventory.push(gauntlets.clone());
        let mut surface = Surface::of(inventory);
        for charge in &charges {
            surface.assess(charge, 100, &[(107, 100)]);
        }
        surface.assess(&gauntlets, 100, &[(107, 10), (108, 100)]);
        let mut rig = Rig::new(
            surface,
            refill_settings(33),
            profiled(&[("Mana Charge", ConsumableCategory::ManaSource)], true),
        );

        assert!(!rig.tick(0.0));
        rig.surface.replace_assessment_version(20, 101);
        assert!(rig.tick(0.0));
        assert_eq!(rig.surface.apply_calls, [(10, 1)]);

        if through_the_spoken_line {
            // A receipt that says nothing good: the spoken line is the only
            // thing that can end the readings.
            rig.surface.last_item_completion = receipt(1, 10, 1, 0x1D);
            rig.surface
                .chat_messages
                .push(refill_line(1, "Low Gauntlets"));
        } else {
            rig.surface.last_item_completion = receipt(1, 10, 1, 0);
        }

        for _ in 0..10 {
            rig.tick(0.3);
        }
        assert_eq!(
            rig.surface.apply_calls,
            [(10, 1)],
            "spoken line {through_the_spoken_line}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ItemManaRechargeTests.cs :: AChargeInFlightHoldsTheItemSlotUntilTheFrameReadsTheAnswer
#[test]
fn a_charge_in_flight_holds_the_item_slot_until_the_frame_reads_the_answer() {
    let charge = mana_charge(10);
    let gauntlets = worn(20, "Low Gauntlets", 0x0000_0002, 10, 100);
    let mut surface = Surface::of(vec![charge.clone(), gauntlets.clone()]);
    surface.assess(&charge, 100, &[(107, 100)]);
    surface.assess(&gauntlets, 100, &[(107, 10), (108, 100)]);
    let mut rig = Rig::new(
        surface,
        refill_settings(33),
        profiled(&[("Mana Charge", ConsumableCategory::ManaSource)], true),
    );
    let locks = ActionLockTable::shared();
    rig.controller.bind_action_locks(locks.clone());

    assert!(!rig.tick(0.0));
    rig.surface.replace_assessment_version(20, 101);
    assert!(!locks.borrow_mut().is_locked(ActionLockKind::ItemUse));

    assert!(rig.tick(0.0));
    assert_eq!(rig.surface.apply_calls, [(10, 1)]);
    assert!(locks.borrow_mut().is_locked(ActionLockKind::ItemUse));

    // No turn from here: only frames run while the pass is held.
    rig.surface.last_item_completion = receipt(1, 10, 1, 0);
    rig.observe(0.05);
    assert!(!locks.borrow_mut().is_locked(ActionLockKind::ItemUse));
}

fn three_full_pieces() -> Surface {
    let worn_items = [
        worn(20, "Gauntlets", 0x0000_0002, 90, 100),
        worn(21, "Helm", 0x0000_0004, 90, 100),
        worn(22, "Girth", 0x0000_0008, 90, 100),
    ];
    let mut surface = Surface::of(worn_items.to_vec());
    for item in &worn_items {
        surface.assess(item, 100, &[(107, 90), (108, 100)]);
    }
    surface
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ItemManaRechargeTests.cs :: WornGearIsAskedAboutOneAtATimeAndInTurn
#[test]
fn worn_gear_is_asked_about_one_at_a_time_and_in_turn() {
    let mut rig = Rig::new(
        three_full_pieces(),
        refill_settings(33),
        CombatSettings::default(),
    );

    assert!(!rig.tick(0.3));
    assert_eq!(rig.surface.identify_requests, [20]);

    // Nobody answers for the first, and the question is let go after ten
    // seconds; the turn then passes to the next piece.
    assert!(!rig.tick(11.0));
    assert_eq!(rig.surface.identify_requests, [20, 21]);
    assert!(!rig.tick(11.0));
    assert_eq!(rig.surface.identify_requests, [20, 21, 22]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ItemManaRechargeTests.cs :: ARefillLeavesTheQuestionAlreadyOutStandingButNotItsAnswer
#[test]
fn a_refill_leaves_the_question_already_out_standing_but_not_its_answer() {
    let mut surface = three_full_pieces();
    surface.inventory.truncate(2);
    let mut rig = Rig::new(surface, refill_settings(33), CombatSettings::default());

    assert!(!rig.tick(0.3));
    assert_eq!(rig.surface.identify_requests, [20]);

    // A refill lands while that question is still out.
    rig.surface
        .chat_messages
        .push(refill_line(1, "Gauntlets, Helm"));
    assert!(!rig.tick(0.3));
    assert_eq!(rig.surface.identify_requests, [20]);

    // The answer arrives; the turn passes to the other piece.
    rig.surface.replace_assessment_version(20, 101);
    assert!(!rig.tick(0.3));
    assert_eq!(rig.surface.identify_requests, [20, 21]);

    // Nobody answers for that one, and the turn comes back round: the first
    // piece is due again, its answer having been about the old mana.
    assert!(!rig.tick(11.0));
    assert_eq!(rig.surface.identify_requests, [20, 21, 20]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ItemManaRechargeTests.cs :: WornGearIsAppraisedAgainOnlyOnceItsNumbersHaveGoneStale
#[test]
fn worn_gear_is_appraised_again_only_once_its_numbers_have_gone_stale() {
    let charge = mana_charge(10);
    // Comfortably above the threshold: the passes measure the appraisal
    // cadence alone.
    let gauntlets = worn(20, "Full Gauntlets", 0x0000_0002, 90, 100);
    let mut surface = Surface::of(vec![charge.clone(), gauntlets.clone()]);
    surface.assess(&charge, 100, &[(107, 100)]);
    surface.assess(&gauntlets, 100, &[(107, 90), (108, 100)]);
    let mut rig = Rig::new(
        surface,
        refill_settings(33),
        profiled(&[("Mana Charge", ConsumableCategory::ManaSource)], true),
    );

    assert!(!rig.tick(0.0));
    assert_eq!(rig.surface.identify_requests, [20]);
    rig.surface.replace_assessment_version(20, 101);
    rig.surface.identify_requests.clear();

    // The pass that takes the answer in: the wait starts here, at 0.1 s.
    assert!(!rig.tick(0.1));
    assert!(rig.surface.identify_requests.is_empty());
    assert_eq!(rig.status(), "Worn mana ready");

    // Two minutes on is inside every draw of the band.
    assert!(!rig.tick(119.9));
    assert!(rig.surface.identify_requests.is_empty());

    // Six minutes on is past every draw of it.
    assert!(!rig.tick(240.2));
    assert_eq!(rig.surface.identify_requests, [20]);
    assert!(rig.surface.apply_calls.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ItemManaRechargeTests.cs :: NoDescriptionIsAskedForOnAPassThatHasSomethingToSpend
#[test]
fn no_description_is_asked_for_on_a_pass_that_has_something_to_spend() {
    let charge = mana_charge(10);
    // Worn but holding no mana: never a target, never answered.
    let helm = InventoryItem {
        equipped_location: 0x0000_0001,
        ..item(19, "Plain Helm")
    };
    let wand = worn(20, "Low Wand", 0x0000_0002, 10, 100);
    let mut surface = Surface::of(vec![charge.clone(), helm, wand.clone()]);
    surface.assess(&charge, 100, &[(107, 100)]);
    surface.assess(&wand, 100, &[(107, 10), (108, 100)]);
    let mut rig = Rig::new(
        surface,
        refill_settings(33),
        profiled(&[("Mana Charge", ConsumableCategory::ManaSource)], false),
    );

    // Nothing is spendable yet, so the gear is asked about, in turn.
    assert!(!rig.tick(0.0));
    assert_eq!(rig.surface.identify_requests, [19]);
    assert!(!rig.tick(11.0));
    assert_eq!(rig.surface.identify_requests, [19, 20]);
    rig.surface.replace_assessment_version(20, 101);

    // The wand's answer landed: this pass spends instead of asking.
    assert!(rig.tick(11.0));
    assert_eq!(rig.surface.apply_calls, [(10, 1)]);
    assert_eq!(rig.surface.identify_requests, [19, 20]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ItemManaRechargeTests.cs :: AnUnansweredRefillStopsClaimingThePassWhenItsWindowRunsOut
#[test]
fn an_unanswered_refill_stops_claiming_the_pass_when_its_window_runs_out() {
    let charge = mana_charge(10);
    let wand = worn(20, "Low Wand", 0x0000_0002, 10, 100);
    let mut surface = Surface::of(vec![charge.clone(), wand.clone()]);
    surface.assess(&charge, 100, &[(107, 100)]);
    surface.assess(&wand, 100, &[(107, 10), (108, 100)]);
    let mut rig = Rig::new(
        surface,
        refill_settings(33),
        profiled(&[("Mana Charge", ConsumableCategory::ManaSource)], false),
    );
    let locks = ActionLockTable::shared();
    rig.controller.bind_action_locks(locks.clone());

    assert!(!rig.tick(0.0));
    rig.surface.replace_assessment_version(20, 101);
    assert!(rig.tick(0.0));
    assert_eq!(rig.surface.apply_calls, [(10, 1)]);
    assert!(locks.borrow_mut().is_locked(ActionLockKind::ItemUse));

    // Inside the window the charge owns the character.
    locks.borrow_mut().advance(1.0);
    assert!(rig.tick(1.0));

    // Past it, with no answer: the slot is free and so is the pass.
    locks.borrow_mut().advance(1.0);
    assert!(!locks.borrow_mut().is_locked(ActionLockKind::ItemUse));
    assert!(!rig.tick(1.0));
    assert_eq!(rig.surface.apply_calls, [(10, 1)]);
}

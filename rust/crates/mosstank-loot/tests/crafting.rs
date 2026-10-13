//! Crafting, ported from the C#'s `CraftingTests`.

use std::cell::Cell;
use std::rc::Rc;

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::{Skill, Training};
use ac_plugin_abi::combat::{CombatMode, CombatResult, CombatStatus};
use ac_plugin_abi::items::{
    InventoryCommandKind, InventoryCompletion, InventoryItem, ItemResult, ItemStatus,
    ItemUseCompletion,
};
use ac_plugin_abi::magic::CastGate;
use ac_plugin_abi::world::ItemProperties;
use mosstank_automation::{Automation, SpellCatalog, StaticSpellCatalog};
use mosstank_formats::fixtures;
use mosstank_formats::gameinfo::VtankGameInfoDatabase;
use mosstank_loot::crafting::{
    ALL_PEAS, CraftTurn, CraftingController, CraftingPlanner, NO_RECIPES_WARNING, classify,
    pea_consumable_names,
};
use mosstank_settings::model::{
    AssistItem, CombatSettings, ConsumableCategory, InventorySettings, VitalSettings,
};

/// The fixture game database, whose rows were all written for the tests.
fn game_info() -> VtankGameInfoDatabase {
    VtankGameInfoDatabase::parse(&fixtures::read_vtank("gameinfodb-excerpt.ugd")).unwrap()
}

fn item(id: u32, name: &str) -> InventoryItem {
    InventoryItem {
        object_id: id,
        name: name.to_owned(),
        item_type: 0x80,
        container_id: 1,
        stack_size: 1,
        ..InventoryItem::default()
    }
}

fn stack(id: u32, name: &str, size: i32) -> InventoryItem {
    InventoryItem {
        stack_size: size,
        ..item(id, name)
    }
}

fn trained(skill_id: u32) -> Vec<Skill> {
    if skill_id == 0 {
        return Vec::new();
    }
    vec![Skill {
        skill_id,
        name: "Craft".to_owned(),
        training: Training::Trained,
        current: 300,
        ..Skill::default()
    }]
}

/// The client: an inventory, the moves and applies asked of it.
#[derive(Default)]
struct Client {
    trained_skill: u32,
    busy: bool,
    inventory_completion: InventoryCompletion,
    use_completion: ItemUseCompletion,
    inventory: Vec<InventoryItem>,
    moves: Vec<(u32, u32, u32)>,
    applies: Vec<(u32, u32)>,
    spells: StaticSpellCatalog,
}

impl Automation for Client {
    fn is_available(&mut self) -> bool {
        true
    }
    fn character(&mut self) -> CharacterInfo {
        CharacterInfo {
            id: 1,
            ..CharacterInfo::default()
        }
    }
    fn skills(&mut self) -> Vec<Skill> {
        trained(self.trained_skill)
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
        CombatMode::Peace
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
    fn items_busy(&mut self) -> bool {
        self.busy
    }
    fn owned_items(&mut self) -> Vec<InventoryItem> {
        self.inventory.clone()
    }
    fn last_item_completion(&mut self) -> ItemUseCompletion {
        self.use_completion.clone()
    }
    fn use_item(&mut self, _object_id: u32) -> ItemResult {
        ItemResult::of(ItemStatus::Unavailable)
    }
    fn apply_item(&mut self, object_id: u32, target_id: u32) -> ItemResult {
        self.applies.push((object_id, target_id));
        self.busy = true;
        ItemResult::of(ItemStatus::Started)
    }
    fn item_properties(&mut self, _object_id: u32) -> Option<ItemProperties> {
        None
    }
    fn inventory_completion(&mut self) -> InventoryCompletion {
        self.inventory_completion.clone()
    }
    fn split_stack(&mut self, object_id: u32, container_id: u32, amount: u32) -> ItemResult {
        self.moves.push((object_id, container_id, amount));
        self.busy = true;
        ItemResult::of(ItemStatus::Started)
    }
    fn post_system_message(&mut self, _text: &str) {}
    fn log_info(&mut self, _text: &str) {}
    fn log_warn(&mut self, _text: &str) {}
}

fn settings(auto_craft: bool, split_peas: bool) -> InventorySettings {
    InventorySettings {
        auto_craft_items: auto_craft,
        split_peas,
        ..InventorySettings::default()
    }
}

fn wanting(name: &str) -> CombatSettings {
    let mut profiles = CombatSettings::default();
    profiles.consumable_names.insert(name.to_owned());
    profiles
}

fn turn<'a>(
    client: &'a mut Client,
    settings: &'a InventorySettings,
    profiles: &'a CombatSettings,
) -> CraftTurn<'a> {
    CraftTurn {
        automation: client,
        settings,
        profiles,
        vitals: Box::leak(Box::new(VitalSettings::default())),
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: TheGameDatabaseCraftTableIsReadByColumn
#[test]
fn the_game_database_craft_table_is_read_by_column() {
    let info = game_info();
    assert_eq!(info.crafts.recipes().len(), 6);
    let kits = info.crafts.for_result("Plentiful Healing Kit");
    assert_eq!(kits.len(), 1);
    let kit = &kits[0];
    assert_eq!(kit.first_item, "Soft Bandages");
    assert_eq!(kit.second_item, "Combined Hyssop and Mandrake");
    assert_eq!(kit.required_skill, 21);
    assert_eq!(kit.id, 9002);
    let arrows = info.crafts.for_result("deadly fire arrow");
    assert_eq!(arrows.len(), 1);
    assert_eq!(arrows[0].result_count, 250);
    assert_eq!(arrows[0].required_skill, 37);
    assert_eq!(arrows[0].difficulty, 200);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: WithoutACraftTableCraftingSaysSoOnceAndTakesALaterTable
#[test]
fn without_a_craft_table_crafting_says_so_once_and_takes_a_later_table() {
    let mut client = Client {
        trained_skill: 21,
        inventory: vec![
            item(1, "Soft Bandages"),
            item(2, "Combined Hyssop and Mandrake"),
        ],
        ..Client::default()
    };
    let mut profiles = wanting("Plentiful Healing Kit");
    profiles.consumable_categories.insert(
        "Plentiful Healing Kit".to_owned(),
        ConsumableCategory::HealthKit,
    );
    let settings = settings(true, false);
    let mut controller = CraftingController::new(VtankGameInfoDatabase::load_default());
    let said = Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let sink = said.clone();
    controller.warning = Some(Box::new(move |text: &str| {
        sink.borrow_mut().push(text.to_owned())
    }));

    assert!(!controller.tick(&mut turn(&mut client, &settings, &profiles), 1.0, true));
    assert!(!controller.tick(&mut turn(&mut client, &settings, &profiles), 1.0, true));
    assert!(!controller.request(
        &mut turn(&mut client, &settings, &profiles),
        "Plentiful Healing Kit",
        1
    ));

    assert_eq!(*said.borrow(), [NO_RECIPES_WARNING]);
    assert!(client.applies.is_empty());
    assert_eq!(controller.status(), "AutoCraft has no recipes");

    controller.replace_game_info(game_info());

    assert!(controller.tick(&mut turn(&mut client, &settings, &profiles), 1.0, true));
    assert_eq!(client.applies, [(1, 2)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: PlannerRecursivelyCraftsMissingPrerequisiteBeforeDesiredItem
#[test]
fn planner_recursively_crafts_missing_prerequisite_before_desired_item() {
    let info = game_info();
    let skills = trained(21);
    let inventory = vec![
        item(1, "Soft Bandages"),
        item(2, "Treated Mandrake"),
        item(3, "Treated Hyssop"),
    ];
    let wanted = ["Plentiful Healing Kit".to_owned()];

    let first = CraftingPlanner::plan(&info.crafts, &inventory, &wanted, &skills, 1, 10).unwrap();
    assert_eq!(first.recipe.result_item, "Combined Hyssop and Mandrake");
    assert_eq!(first.first_object_id, 2);
    assert_eq!(first.second_object_id, 3);

    let last = CraftingPlanner::plan(
        &info.crafts,
        &[
            inventory[0].clone(),
            item(4, "Combined Hyssop and Mandrake"),
        ],
        &wanted,
        &skills,
        1,
        10,
    )
    .unwrap();
    assert_eq!(last.recipe.result_item, "Plentiful Healing Kit");
    assert_eq!(last.first_object_id, 1);
    assert_eq!(last.second_object_id, 4);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: PlannerRejectsRecipesForUntrainedRequiredSkill
#[test]
fn planner_rejects_recipes_for_untrained_required_skill() {
    let plan = CraftingPlanner::plan(
        &game_info().crafts,
        &[
            item(1, "Soft Bandages"),
            item(2, "Combined Hyssop and Mandrake"),
        ],
        &["Plentiful Healing Kit".to_owned()],
        &trained(0),
        1,
        10,
    );
    assert!(plan.is_none());
}

fn ignore_case_profile(names: &[&str]) -> impl Fn(&str) -> bool {
    let names: Vec<String> = names.iter().map(|name| name.to_lowercase()).collect();
    move |name: &str| names.contains(&name.to_lowercase())
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: AllPeasSplitsTheFirstProfiledPeaBelowTheComponentThreshold
#[test]
fn all_peas_splits_the_first_profiled_pea_below_the_component_threshold() {
    let profile = ignore_case_profile(&[ALL_PEAS]);
    let plan = CraftingPlanner::plan_pea_split(
        &game_info().crafts,
        &[item(1, "Splitting Tool"), item(2, "Brimstone Pea")],
        &profile,
        20,
    )
    .unwrap();
    assert_eq!(plan.recipe.result_item, "Brimstone");
    assert_eq!(plan.first_object_id, 1);
    assert_eq!(plan.second_object_id, 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: ImportedNonPeaAssistRowDoesNotAuthorizePeaSplitting
#[test]
fn imported_non_pea_assist_row_does_not_authorize_pea_splitting() {
    let mut profiles = CombatSettings::default();
    for name in ["Gold Pea", "Silver Pea"] {
        profiles.consumable_names.insert(name.to_owned());
        profiles
            .consumable_categories
            .insert(name.to_owned(), ConsumableCategory::HealthKit);
    }
    profiles.imported_assist_items.push(AssistItem {
        name: "Gold Pea".to_owned(),
        category: ConsumableCategory::HealthKit,
    });

    let peas = pea_consumable_names(&profiles);

    assert!(!peas.contains("Gold Pea"));
    assert!(peas.contains("Silver Pea"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: PeaSplitStopsAtTheRequestedComponentCount
#[test]
fn pea_split_stops_at_the_requested_component_count() {
    let profile = ignore_case_profile(&["Brimstone Pea"]);
    assert!(
        CraftingPlanner::plan_pea_split(
            &game_info().crafts,
            &[
                item(1, "Splitting Tool"),
                item(2, "Brimstone Pea"),
                stack(3, "Brimstone", 20),
            ],
            &profile,
            20,
        )
        .is_none()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: SameInputRecipeStagesAnExactOneUnitSplitBeforeApplying
#[test]
fn same_input_recipe_stages_an_exact_one_unit_split_before_applying() {
    let plan = CraftingPlanner::plan(
        &game_info().crafts,
        &[stack(1, "Chorizite Oil", 2)],
        &["Strong Chorizite Oil".to_owned()],
        &trained(0),
        1,
        10,
    )
    .unwrap();
    assert!(plan.requires_split_first_stack);
    assert_eq!(plan.first_object_id, 1);
    assert_eq!(plan.split_container_object_id, 1);
    assert_eq!(plan.second_object_id, 0);
}

fn oil_client() -> Client {
    Client {
        inventory: vec![stack(20, "Chorizite Oil", 2)],
        ..Client::default()
    }
}

fn split_answered(client: &mut Client) {
    client.inventory_completion = InventoryCompletion {
        revision: 1,
        kind: InventoryCommandKind::SplitToContainer,
        source_id: 20,
        weenie_error: 0,
    };
    client.busy = false;
}

fn two_oils() -> Vec<InventoryItem> {
    vec![stack(20, "Chorizite Oil", 1), stack(21, "Chorizite Oil", 1)]
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: ControllerWaitsForSplitReceiptAndBothPublishedStacksBeforeApplying
#[test]
fn controller_waits_for_split_receipt_and_both_published_stacks_before_applying() {
    let mut client = oil_client();
    let settings = settings(true, false);
    let profiles = wanting("Strong Chorizite Oil");
    let mut controller = CraftingController::new(game_info());

    assert!(controller.tick(&mut turn(&mut client, &settings, &profiles), 0.0, true));
    assert_eq!(client.moves, [(20, 1, 1)]);
    assert!(client.applies.is_empty());

    split_answered(&mut client);
    assert!(controller.tick(&mut turn(&mut client, &settings, &profiles), 0.05, true));
    assert!(client.applies.is_empty());
    assert_eq!(controller.status(), "AutoCraft waiting for split inventory");

    client.inventory = two_oils();
    assert!(controller.tick(&mut turn(&mut client, &settings, &profiles), 0.05, true));
    assert_eq!(client.applies, [(20, 21)]);
    assert_eq!(controller.status(), "Crafting Strong Chorizite Oil");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: ARuleBelowTheWinnerObservesTheSplitButDoesNotResumeTheCraft
#[test]
fn a_rule_below_the_winner_observes_the_split_but_does_not_resume_the_craft() {
    let mut client = oil_client();
    let settings = settings(true, false);
    let profiles = wanting("Strong Chorizite Oil");
    let mut controller = CraftingController::new(game_info());

    assert!(controller.tick(&mut turn(&mut client, &settings, &profiles), 0.0, true));
    assert_eq!(client.moves, [(20, 1, 1)]);

    split_answered(&mut client);
    client.inventory = two_oils();

    // A rule above won this pass. The receipt is still observed...
    assert!(controller.tick(&mut turn(&mut client, &settings, &profiles), 0.05, false));
    assert!(client.applies.is_empty());

    assert!(controller.tick(&mut turn(&mut client, &settings, &profiles), 0.05, true));
    assert_eq!(client.applies, [(20, 21)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: SplitTimeoutIsWallClockNotThreeTimesTheRuleListDepth
#[test]
fn split_timeout_is_wall_clock_not_three_times_the_rule_list_depth() {
    let mut client = oil_client();
    let settings = settings(true, false);
    let profiles = wanting("Strong Chorizite Oil");
    let mut controller = CraftingController::new(game_info());

    assert!(controller.tick(&mut turn(&mut client, &settings, &profiles), 0.0, true));
    assert_eq!(client.moves, [(20, 1, 1)]);

    for _ in 0..9 {
        assert!(controller.tick_critical(&mut turn(&mut client, &settings, &profiles), 1.0, true));
        assert!(controller.tick(&mut turn(&mut client, &settings, &profiles), 1.0, true));
        assert!(controller.tick_idle(&mut turn(&mut client, &settings, &profiles), 1.0, true));
    }
    assert_ne!(controller.status(), "AutoCraft split timed out");

    assert!(controller.tick_critical(&mut turn(&mut client, &settings, &profiles), 1.5, true));
    assert_eq!(controller.status(), "AutoCraft split timed out");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: CraftingWaitsForPeaceModeBeforeApplyingARecipe
#[test]
fn crafting_waits_for_peace_mode_before_applying_a_recipe() {
    let mut client = Client {
        inventory: vec![stack(1, "Chorizite Oil", 2)],
        ..Client::default()
    };
    let settings = settings(true, false);
    let profiles = wanting("Strong Chorizite Oil");
    let mut controller = CraftingController::new(game_info());
    let in_peace = Rc::new(Cell::new(false));
    let gate = in_peace.clone();
    controller.ready_to_craft_in_peace = Some(Box::new(move |_turn| gate.get()));

    assert!(controller.tick(&mut turn(&mut client, &settings, &profiles), 0.0, true));
    assert!(client.applies.is_empty());
    assert!(client.moves.is_empty());
    assert!(controller.status().contains("peace mode"));

    in_peace.set(true);
    assert!(controller.tick(&mut turn(&mut client, &settings, &profiles), 0.5, true));
    assert!(!client.moves.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: ACraftUseIsReportedInFlightOnceItHasBeenIssued
#[test]
fn a_craft_use_is_reported_in_flight_once_it_has_been_issued() {
    let mut client = Client {
        inventory: vec![stack(1, "Chorizite Oil", 2)],
        ..Client::default()
    };
    let settings = settings(true, false);
    let profiles = wanting("Strong Chorizite Oil");
    let mut controller = CraftingController::new(game_info());
    controller.ready_to_craft_in_peace = Some(Box::new(|_turn| true));
    assert!(!controller.use_in_flight());

    assert!(controller.tick(&mut turn(&mut client, &settings, &profiles), 0.5, true));

    assert!(!client.moves.is_empty());
    assert!(controller.use_in_flight());
}

fn arrow_client(head: u32, shaft: u32) -> Client {
    Client {
        trained_skill: 37,
        inventory: vec![
            item(head, "Wrapped Bundle of Deadly Fire Arrowheads"),
            item(shaft, "Wrapped Bundle of Arrowshafts"),
        ],
        ..Client::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: AmmunitionRequestCraftsEvenWhenGeneralAutoCraftIsDisabled
#[test]
fn ammunition_request_crafts_even_when_general_auto_craft_is_disabled() {
    let mut client = arrow_client(1, 2);
    let settings = settings(false, true);
    let profiles = CombatSettings::default();
    let mut controller = CraftingController::new(game_info());

    assert!(controller.request(
        &mut turn(&mut client, &settings, &profiles),
        "Deadly Fire Arrow",
        1
    ));

    assert_eq!(client.applies, [(1, 2)]);
    assert_eq!(controller.status(), "Crafting Deadly Fire Arrow");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: TheServersAnswerToACraftIsReadOnTheFrameWithoutAPass
#[test]
fn the_servers_answer_to_a_craft_is_read_on_the_frame_without_a_pass() {
    let mut client = arrow_client(1, 2);
    let settings = settings(false, true);
    let profiles = CombatSettings::default();
    let mut controller = CraftingController::new(game_info());

    assert!(controller.request(
        &mut turn(&mut client, &settings, &profiles),
        "Deadly Fire Arrow",
        1
    ));
    assert!(controller.use_in_flight());

    // No tick from here: the pass is held, only frames run.
    client.use_completion = ItemUseCompletion {
        revision: 1,
        source_id: 1,
        target_id: 0,
        weenie_error: 0,
    };
    controller.observe_pending_receipt(&mut client, &settings, 0.05);

    assert!(!controller.use_in_flight());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: AmmunitionCraftAppliesTheResolvedComponentPair
#[test]
fn ammunition_craft_applies_the_resolved_component_pair() {
    let mut client = arrow_client(10, 20);
    let settings = InventorySettings::default();
    let profiles = CombatSettings::default();
    let mut controller = CraftingController::new(game_info());
    let chosen = controller
        .resolve_request_plan(
            &mut turn(&mut client, &settings, &profiles),
            "Deadly Fire Arrow",
            1,
        )
        .unwrap();
    assert_eq!((chosen.first_object_id, chosen.second_object_id), (10, 20));

    client.inventory = vec![
        item(30, "Wrapped Bundle of Deadly Fire Arrowheads"),
        item(40, "Wrapped Bundle of Arrowshafts"),
    ];
    assert!(controller.request_resolved(&mut turn(&mut client, &settings, &profiles), &chosen));
    assert_eq!(client.applies, [(10, 20)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: ConsumableClassificationUsesAuthenticPropertiesAndExactKitExceptions
#[test]
fn consumable_classification_uses_authentic_properties_and_exact_kit_exceptions() {
    let cases = [
        (
            "Greater Stamina Kit",
            0x0001_0000,
            0,
            ConsumableCategory::StaminaKit,
        ),
        (
            "Greater Mana Kit",
            0x0001_0000,
            0,
            ConsumableCategory::ManaKit,
        ),
        (
            "Greater Healing Kit",
            0x0001_0000,
            0,
            ConsumableCategory::HealthKit,
        ),
        ("Mana Food", 0, 6, ConsumableCategory::ManaFood),
        ("Stamina Food", 0, 4, ConsumableCategory::StaminaFood),
        ("Health Food", 0, 2, ConsumableCategory::HealthFood),
        (
            "Intricate Lockpick",
            0x0002_0000,
            0,
            ConsumableCategory::Lockpick,
        ),
    ];
    for (name, public_flags, booster_vital, expected) in cases {
        let item = InventoryItem {
            public_flags,
            booster_vital,
            ..item(1, name)
        };
        assert_eq!(classify(&item), expected, "{name}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: IdleCraftingRestocksTheConfiguredConsumableCategoryCount
#[test]
fn idle_crafting_restocks_the_configured_consumable_category_count() {
    let mut client = Client {
        trained_skill: 21,
        inventory: vec![
            item(1, "Soft Bandages"),
            item(2, "Combined Hyssop and Mandrake"),
            item(3, "Plentiful Healing Kit"),
        ],
        ..Client::default()
    };
    let settings = InventorySettings {
        idle_health_kit_count: 2,
        ..settings(true, false)
    };
    let mut profiles = wanting("Plentiful Healing Kit");
    profiles.consumable_categories.insert(
        "Plentiful Healing Kit".to_owned(),
        ConsumableCategory::HealthKit,
    );
    let mut controller = CraftingController::new(game_info());

    assert!(controller.tick_idle(&mut turn(&mut client, &settings, &profiles), 0.0, true));
    assert_eq!(client.applies, [(1, 2)]);
    assert_eq!(controller.status(), "Crafting Plentiful Healing Kit");
}

/// One idle restock pass with every category's count at zero but the
/// named one's (`TickIdleWithCategoryCount`).
fn tick_idle_with_category_count(category: ConsumableCategory, count: i32) -> bool {
    let mut client = Client {
        trained_skill: 21,
        inventory: vec![
            item(1, "Soft Bandages"),
            item(2, "Combined Hyssop and Mandrake"),
            item(3, "Plentiful Healing Kit"),
        ],
        ..Client::default()
    };
    let mut settings = InventorySettings {
        idle_health_kit_count: 0,
        idle_stamina_kit_count: 0,
        idle_mana_kit_count: 0,
        idle_health_food_count: 0,
        idle_stamina_food_count: 0,
        idle_mana_food_count: 0,
        ..settings(true, false)
    };
    match category {
        ConsumableCategory::HealthKit => settings.idle_health_kit_count = count,
        ConsumableCategory::StaminaKit => settings.idle_stamina_kit_count = count,
        ConsumableCategory::ManaKit => settings.idle_mana_kit_count = count,
        ConsumableCategory::HealthFood => settings.idle_health_food_count = count,
        ConsumableCategory::StaminaFood => settings.idle_stamina_food_count = count,
        ConsumableCategory::ManaFood => settings.idle_mana_food_count = count,
        _ => {}
    }
    let mut profiles = wanting("Plentiful Healing Kit");
    profiles
        .consumable_categories
        .insert("Plentiful Healing Kit".to_owned(), category);
    let mut controller = CraftingController::new(game_info());
    controller.tick_idle(&mut turn(&mut client, &settings, &profiles), 0.0, true)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CraftingTests.cs :: IdleRestockingReadsTheCountForItsOwnCategory
#[test]
fn idle_restocking_reads_the_count_for_its_own_category() {
    for category in [
        ConsumableCategory::HealthKit,
        ConsumableCategory::StaminaKit,
        ConsumableCategory::ManaKit,
        ConsumableCategory::HealthFood,
        ConsumableCategory::StaminaFood,
        ConsumableCategory::ManaFood,
    ] {
        assert!(tick_idle_with_category_count(category, 2), "{category:?}");
        assert!(!tick_idle_with_category_count(category, 0), "{category:?}");
    }
}

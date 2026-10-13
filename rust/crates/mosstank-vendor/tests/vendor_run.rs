//! The vendor run as an owner of the character (`VendorTradeControllerTests`):
//! it wakes when a shop opens, finds its profile the tiered way, stacks
//! first, then buys and sells round by round through the staged lists and
//! the two commit calls, waiting on the server's answer between rounds.

use std::collections::{HashMap, HashSet};

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::Skill;
use ac_plugin_abi::combat::{CombatMode, CombatResult, CombatStatus};
use ac_plugin_abi::items::{InventoryCompletion, InventoryItem, ItemResult, ItemStatus};
use ac_plugin_abi::magic::{CastGate, SpellInfo};
use ac_plugin_abi::navigation::{
    MovementIntent, NavigationObject, NavigationSnapshot, NavigationStatus,
};
use ac_plugin_abi::trade::{
    NO_VALUE_LIMIT, VendorItem, VendorProfile, VendorResult, VendorState, VendorStatus,
    VendorTransaction, VendorTransactionKind,
};
use ac_plugin_abi::world::{ItemProperties, ObjectClass, WorldObject};
use mosstank_automation::locks::{ActionLockKind, ActionLockTable, SharedActionLocks};
use mosstank_automation::{Automation, SpellCatalog};
use mosstank_formats::utl::{self, LootAction, LootRule};
use mosstank_nav::NavigationAutomation;
use mosstank_stores::storage::{MemoryStorage, NullLog, PluginStorage, StorageError};
use mosstank_stores::ub_store::UbSettingStore;
use mosstank_vendor::ub_chat::UbChatSink;
use mosstank_vendor::{
    VendorAutomation, VendorContext, VendorTradeController, VendorTradeSettings,
};

const PLAYER: u32 = 1;
const SHOPKEEPER: u32 = 500;
const RATION_LISTING: u32 = 9001;
const SWORD: u32 = 20;
const COINS: u32 = 30;

/// The vendor tests' client (`VendorTradeControllerTests.FakeAutomation`).
#[derive(Debug, Default)]
struct FakeShop {
    messages: Vec<String>,
    owned: Vec<InventoryItem>,
    world_objects: Vec<WorldObject>,
    properties: HashMap<u32, ItemProperties>,
    listings: Vec<VendorItem>,
    shop_profile: VendorProfile,
    staged: Vec<(u32, i32)>,
    staged_sells: Vec<u32>,
    buy_all_calls: Vec<Vec<(u32, i32)>>,
    sell_all_calls: Vec<Vec<u32>>,
    uses: Vec<u32>,
    /// Every move asked for, a split included.
    moves: Vec<(u32, u32, u32)>,
    active_vendor_object_id: u32,
    last_inventory_completion: InventoryCompletion,
}

fn sent() -> VendorResult {
    VendorResult {
        status: VendorStatus::Sent,
        notice: None,
    }
}

/// `PluginVendorProfile.Unset`: what the client reports with no vendor
/// open.
fn unset_profile() -> VendorProfile {
    VendorProfile {
        buy_rate: 0.0,
        deals_in_item_types: 0,
        minimum_value: NO_VALUE_LIMIT,
        maximum_value: NO_VALUE_LIMIT,
        deals_in_magical_items: false,
        alternate_currency_weenie_class_id: 0,
        alternate_currency_amount: 0,
        alternate_currency_name: None,
    }
}

impl SpellCatalog for FakeShop {
    fn known_self_buffs(&mut self) -> Vec<SpellInfo> {
        Vec::new()
    }

    fn try_get(&mut self, _spell_id: u32) -> Option<SpellInfo> {
        None
    }
}

impl Automation for FakeShop {
    /// No navigation: every object is as near as every other.
    fn navigation_snapshot(&mut self) -> NavigationSnapshot {
        NavigationSnapshot::default()
    }

    fn navigation_object(&mut self, _object_id: u32) -> Option<NavigationObject> {
        None
    }

    fn set_movement_intent(&mut self, _intent: MovementIntent) -> NavigationStatus {
        NavigationStatus::Unavailable
    }

    fn clear_movement_intent(&mut self) -> NavigationStatus {
        NavigationStatus::Unavailable
    }

    fn is_available(&mut self) -> bool {
        true
    }

    fn character(&mut self) -> CharacterInfo {
        CharacterInfo {
            id: PLAYER,
            name: "Acdream".to_owned(),
            world: "Coldeve".to_owned(),
            main_pack_free_slots: 10,
            ..CharacterInfo::default()
        }
    }

    fn skills(&mut self) -> Vec<Skill> {
        Vec::new()
    }

    fn spells(&mut self) -> &mut dyn SpellCatalog {
        self
    }

    fn evaluate_gate(&mut self, _spell_id: u32, _target: Option<u32>) -> CastGate {
        CastGate::Refused
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
        self.owned.clone()
    }

    fn use_item(&mut self, object_id: u32) -> ItemResult {
        self.uses.push(object_id);
        ItemResult::of(ItemStatus::Started)
    }

    fn apply_item(&mut self, _object_id: u32, _target_id: u32) -> ItemResult {
        ItemResult::of(ItemStatus::Unavailable)
    }

    /// Every item answers, with nothing in it unless the test set it.
    fn item_properties(&mut self, object_id: u32) -> Option<ItemProperties> {
        Some(self.properties.get(&object_id).cloned().unwrap_or_default())
    }

    fn post_system_message(&mut self, text: &str) {
        self.messages.push(text.to_owned());
    }

    fn log_info(&mut self, _text: &str) {}

    fn log_warn(&mut self, _text: &str) {}
}

impl NavigationAutomation for FakeShop {
    fn submit_chat(&mut self, text: &str) -> bool {
        self.messages.push(text.to_owned());
        true
    }

    fn active_vendor_id(&mut self) -> u32 {
        self.active_vendor_object_id
    }
}

impl UbChatSink for FakeShop {
    fn post_message(&mut self, text: &str, _chat_type: u32) {
        self.messages.push(text.to_owned());
    }
}

impl VendorAutomation for FakeShop {
    fn vendor_state(&mut self) -> VendorState {
        let is_open = self.active_vendor_object_id != 0;
        VendorState {
            is_available: true,
            is_open,
            vendor_id: self.active_vendor_object_id,
            vendor_name: if is_open { "Shopkeeper" } else { "" }.to_owned(),
            items: if is_open {
                self.listings.clone()
            } else {
                Vec::new()
            },
            profile: if is_open {
                self.shop_profile.clone()
            } else {
                unset_profile()
            },
            is_busy: false,
            buy_list: self.staged.clone(),
            sell_list: self.staged_sells.clone(),
        }
    }

    fn vendor_item_properties(&mut self, template_id: u32) -> Option<ItemProperties> {
        self.item_properties(template_id)
    }

    fn vendor_add_to_buy_list(&mut self, template_id: u32, count: i32) -> VendorResult {
        match self
            .staged
            .iter_mut()
            .find(|(template, _)| *template == template_id)
        {
            Some(staged) => staged.1 += count,
            None => self.staged.push((template_id, count)),
        }
        sent()
    }

    fn vendor_add_to_sell_list(&mut self, item_id: u32) -> VendorResult {
        self.staged_sells.push(item_id);
        sent()
    }

    fn vendor_clear_buy_list(&mut self) -> VendorResult {
        self.staged.clear();
        sent()
    }

    fn vendor_clear_sell_list(&mut self) -> VendorResult {
        self.staged_sells.clear();
        sent()
    }

    fn vendor_buy_all(&mut self) -> VendorResult {
        if self.staged.is_empty() {
            return VendorResult {
                status: VendorStatus::InvalidItem,
                notice: None,
            };
        }
        self.buy_all_calls.push(std::mem::take(&mut self.staged));
        sent()
    }

    fn vendor_sell_all(&mut self) -> VendorResult {
        if self.staged_sells.is_empty() {
            return VendorResult {
                status: VendorStatus::InvalidItem,
                notice: None,
            };
        }
        self.sell_all_calls
            .push(std::mem::take(&mut self.staged_sells));
        sent()
    }

    fn last_inventory_completion(&mut self) -> InventoryCompletion {
        self.last_inventory_completion.clone()
    }

    fn split_item(
        &mut self,
        object_id: u32,
        container_id: u32,
        amount: u32,
        _placement: i32,
    ) -> ItemResult {
        self.moves.push((object_id, container_id, amount));
        ItemResult::of(ItemStatus::Started)
    }

    fn world_objects(&mut self) -> Vec<WorldObject> {
        self.world_objects.clone()
    }
}

/// The plugin's side of a call: the client, the profile storage (one
/// memory storage, as the C# test host gives for both), the UB settings
/// store bound to Coldeve/Acdream, the switches.
struct ShopContext {
    automation: FakeShop,
    storage: MemoryStorage,
    store: UbSettingStore,
    settings: VendorTradeSettings,
    stack_cram: Box<dyn FnMut(f64, bool) -> bool>,
    route_owns_vendor_open: bool,
    protected: HashSet<u32>,
}

impl VendorContext for ShopContext {
    fn automation(&mut self) -> &mut dyn VendorAutomation {
        &mut self.automation
    }

    fn settings(&mut self) -> VendorTradeSettings {
        self.settings.clone()
    }

    fn resolve_profile_key(
        &mut self,
        folder: &str,
        file_name: &str,
    ) -> Result<String, StorageError> {
        self.store
            .resolve_profile_key(&mut self.storage, folder, file_name)
    }

    fn read_profile(&mut self, key: &str) -> Result<Option<String>, StorageError> {
        if !self.storage.is_available() {
            return Ok(None);
        }
        self.storage.read_text(key)
    }

    fn stack_cram_pass(&mut self, elapsed_seconds: f64, first: bool) -> bool {
        (self.stack_cram)(elapsed_seconds, first)
    }

    fn route_owns_vendor_open(&mut self) -> bool {
        self.route_owns_vendor_open
    }

    fn protected_items(&mut self) -> HashSet<u32> {
        self.protected.clone()
    }
}

struct Shop {
    cx: ShopContext,
    controller: VendorTradeController,
}

impl Shop {
    fn tick(&mut self, elapsed_seconds: f64, can_act: bool) -> bool {
        self.controller.tick(&mut self.cx, elapsed_seconds, can_act)
    }

    fn command(&mut self, line: &str) -> Option<Vec<String>> {
        self.controller.vendor_command(&mut self.cx, line)
    }

    fn try_start(&mut self, profile: Option<&str>) -> bool {
        self.controller.try_start(&mut self.cx, profile)
    }

    fn stop_requested(&mut self) -> bool {
        self.controller.stop_requested(&mut self.cx)
    }

    fn fake(&mut self) -> &mut FakeShop {
        &mut self.cx.automation
    }

    /// The shop opens (`FakeAutomation.Open`).
    fn open(&mut self, vendor_object_id: u32) {
        self.cx.automation.active_vendor_object_id = vendor_object_id;
        self.controller.on_vendor_opened(vendor_object_id);
    }

    /// The shop closes, taking the staged lists with it.
    fn close(&mut self) {
        self.cx.automation.active_vendor_object_id = 0;
        self.cx.automation.staged.clear();
        self.cx.automation.staged_sells.clear();
        self.controller.on_vendor_closed();
    }

    fn complete(&mut self, kind: VendorTransactionKind, success: bool) {
        self.controller.on_transaction_completed(VendorTransaction {
            kind,
            success,
            notice: None,
        });
    }

    /// Keep up to ten rations; sell every sword; leave the rest.
    fn write_profile(&mut self, key: &str) {
        let mut rations = LootRule::named("rations", "name ~= ration");
        rations.action = LootAction::KeepUpTo;
        rations.keep_count = 10;
        let mut swords = LootRule::named("swords", "name ~= sword");
        swords.action = LootAction::Sell;
        let mut rest = LootRule::named("rest", "*");
        rest.action = LootAction::NoLoot;
        self.cx
            .storage
            .insert(key, &utl::serialize_rules(&[rations, swords, rest]));
    }
}

fn shop_with_rations(list_rations: bool) -> FakeShop {
    FakeShop {
        listings: if list_rations {
            vec![VendorItem {
                template_id: RATION_LISTING,
                weenie_class_id: 5000,
                name: "Ration".to_owned(),
                object_class: ObjectClass::Food,
                unit_price: 5,
                stack_size: 1,
                max_stack_size: 25,
                item_type: 0x20,
            }]
        } else {
            Vec::new()
        },
        shop_profile: VendorProfile {
            buy_rate: 0.75,
            deals_in_item_types: 0x1 | 0x20,
            minimum_value: NO_VALUE_LIMIT,
            maximum_value: NO_VALUE_LIMIT,
            deals_in_magical_items: true,
            alternate_currency_weenie_class_id: 0,
            alternate_currency_amount: 0,
            alternate_currency_name: None,
        },
        ..FakeShop::default()
    }
}

/// The C# `Controller(automation, settings, locks)`.
fn shop(
    automation: FakeShop,
    settings: VendorTradeSettings,
    locks: Option<&SharedActionLocks>,
) -> Shop {
    let mut storage = MemoryStorage::ordinal();
    let mut store = UbSettingStore::new();
    store.bind(&mut storage, &mut NullLog, Some("Coldeve"), Some("Acdream"));
    let mut controller = VendorTradeController::new();
    if let Some(locks) = locks {
        controller.bind_action_locks(std::rc::Rc::clone(locks));
    }
    Shop {
        cx: ShopContext {
            automation,
            storage,
            store,
            settings,
            stack_cram: Box::new(|_, _| false),
            route_owns_vendor_open: false,
            protected: HashSet::new(),
        },
        controller,
    }
}

fn owned(id: u32, name: &str, value: i32, stack: i32, item_type: u32) -> InventoryItem {
    InventoryItem {
        object_id: id,
        weenie_class_id: id + 2000,
        name: name.to_owned(),
        item_type,
        container_id: PLAYER,
        stack_size: stack,
        value,
        maximum_stack_size: stack.max(1),
        object_class: ObjectClass::MeleeWeapon,
        ..InventoryItem::default()
    }
}

fn coin(amount: i32) -> InventoryItem {
    InventoryItem {
        weenie_class_id: 273,
        object_class: ObjectClass::Money,
        maximum_stack_size: 25000,
        ..owned(COINS, "Pyreal", amount, amount, 0x10)
    }
}

fn vendor(object_id: u32, name: &str) -> WorldObject {
    WorldObject {
        object_id,
        name: name.to_owned(),
        object_class: ObjectClass::Vendor,
        ..WorldObject::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradeControllerTests.cs :: PeasAreNotCountedAsCoinWhenSizingABuy
#[test]
fn peas_are_not_counted_as_coin_when_sizing_a_buy() {
    let mut automation = shop_with_rations(true);
    automation.owned = vec![
        coin(30),
        InventoryItem {
            weenie_class_id: 8331,
            object_class: ObjectClass::Money,
            maximum_stack_size: 100,
            ..owned(41, "Gold Pea", 22500, 20, 0x40)
        },
    ];
    let mut shop = shop(automation, VendorTradeSettings::default(), None);
    shop.write_profile("mosstank/ub/autovendor/Shopkeeper.utl");

    shop.open(SHOPKEEPER);
    for _ in 0..6 {
        if !shop.fake().buy_all_calls.is_empty() {
            break;
        }
        shop.tick(0.1, true);
    }

    // 30 pyreals at 5 a ration buy 5 once the server's rounding is allowed
    // for, not the 9 that 30 + 20 peas would.
    assert_eq!(shop.fake().buy_all_calls, [vec![(RATION_LISTING, 5)]]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradeControllerTests.cs :: AVisitBuysThenSellsThenFinishes
#[test]
fn a_visit_buys_then_sells_then_finishes() {
    let mut automation = shop_with_rations(true);
    automation.owned = vec![owned(SWORD, "Sword", 40, 1, 0x1), coin(100)];
    let mut shop = shop(automation, VendorTradeSettings::default(), None);
    shop.write_profile("mosstank/ub/autovendor/Shopkeeper.utl");
    let stack_passes = std::rc::Rc::new(std::cell::Cell::new(0));
    let counted = std::rc::Rc::clone(&stack_passes);
    shop.cx.stack_cram = Box::new(move |_, _| {
        counted.set(counted.get() + 1);
        counted.get() < 3
    });

    shop.open(SHOPKEEPER);
    assert!(shop.tick(0.1, true));
    assert!(shop.controller.is_running());
    assert!(shop.fake().buy_all_calls.is_empty());
    // Two more ticks of stacking before the planner runs.
    assert!(shop.tick(0.1, true));
    assert!(shop.tick(0.1, true));
    assert_eq!(stack_passes.get(), 3);

    assert!(shop.tick(0.1, true));
    // 100 coin at 5 a ration buys 20, and the rule wants 10.
    assert_eq!(shop.fake().buy_all_calls, [vec![(RATION_LISTING, 10)]]);
    assert!(shop.fake().sell_all_calls.is_empty());

    // Nothing moves until the server answers.
    assert!(shop.tick(1.0, true));
    assert_eq!(shop.fake().buy_all_calls.len(), 1);

    let sword = shop.fake().owned[0].clone();
    shop.fake().owned = vec![sword, coin(50), owned(40, "Ration", 50, 10, 0x20)];
    shop.complete(VendorTransactionKind::Buy, true);
    assert!(shop.tick(0.1, true));
    assert!(shop.tick(0.1, true));
    assert_eq!(shop.fake().sell_all_calls, [vec![SWORD]]);
    assert_eq!(shop.fake().buy_all_calls.len(), 1);

    let rations = shop.fake().owned[2].clone();
    shop.fake().owned = vec![coin(80), rations];
    shop.complete(VendorTransactionKind::Sell, true);
    assert!(shop.tick(0.1, true));
    assert!(!shop.tick(0.1, true));
    assert!(!shop.controller.is_running());
    assert!(shop.controller.status().to_lowercase().contains("finished"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradeControllerTests.cs :: AddBuyPrintsTheLinesAProfileWaitsOn
#[test]
fn add_buy_prints_the_lines_a_profile_waits_on() {
    let mut automation = shop_with_rations(true);
    automation.listings = vec![VendorItem {
        template_id: RATION_LISTING,
        weenie_class_id: 5001,
        name: "Powdered Malachite".to_owned(),
        object_class: ObjectClass::Misc,
        unit_price: 5,
        stack_size: 1,
        ..VendorItem::default()
    }];
    let mut shop = shop(automation, VendorTradeSettings::default(), None);
    shop.open(SHOPKEEPER);

    let lines = shop.command("addbuy 1 Powdered Malachite").unwrap();

    assert_eq!(
        lines,
        [
            "[UB] Name: Powdered Malachite count: 1 param:1 Powdered Malachite",
            "[UB] Added item to buy list: Powdered Malachite * 1",
        ]
    );
    assert!(lines[0].starts_with("[UB] Name: Powdered"));
    assert!(lines[0].starts_with("[UB]"));
    assert!(lines[1].starts_with("[UB] Added item to buy list: Powdered"));
    assert_eq!(shop.fake().staged, [(RATION_LISTING, 1)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradeControllerTests.cs :: EveryEndOfARunThinksTheFinishedLine
#[test]
fn every_end_of_a_run_thinks_the_finished_line() {
    let mut automation = shop_with_rations(true);
    automation.owned = vec![coin(100)];
    let mut shop = shop(
        automation,
        VendorTradeSettings {
            think: true,
            ..VendorTradeSettings::default()
        },
        None,
    );
    shop.write_profile("mosstank/ub/autovendor/Shopkeeper.utl");
    shop.open(SHOPKEEPER);
    shop.tick(0.1, true);
    assert!(shop.controller.is_running());
    shop.fake().messages.clear();

    assert!(shop.stop_requested());
    assert!(!shop.stop_requested());

    assert_eq!(
        shop.fake().messages,
        [
            "/t Acdream, AutoVendor finished: Shopkeeper",
            "/t Acdream, AutoVendor finished: Shopkeeper",
        ]
    );
    assert!(
        !shop
            .fake()
            .messages
            .iter()
            .any(|line| line.starts_with("You think"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradeControllerTests.cs :: TestModeReportsAndDoesNotTransact
#[test]
fn test_mode_reports_and_does_not_transact() {
    let mut automation = shop_with_rations(true);
    automation.owned = vec![owned(SWORD, "Sword", 40, 1, 0x1), coin(100)];
    let mut shop = shop(
        automation,
        VendorTradeSettings {
            test_mode: true,
            ..VendorTradeSettings::default()
        },
        None,
    );
    shop.write_profile("mosstank/ub/autovendor/Shopkeeper.utl");

    shop.open(SHOPKEEPER);
    assert!(!shop.tick(0.1, true));
    assert!(!shop.controller.is_running());
    let fake = shop.fake();
    assert!(fake.buy_all_calls.is_empty());
    assert!(fake.sell_all_calls.is_empty());
    assert!(fake.staged.is_empty());
    assert!(fake.messages.iter().any(|line| line.contains("Ration")));
    assert!(fake.messages.iter().any(|line| line.contains("Sword")));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradeControllerTests.cs :: TestModeListsReadAsTheReferencesDo
#[test]
fn test_mode_lists_read_as_the_references_do() {
    const PACK: u32 = 60;
    const HIGH_SWORD: u32 = 0x8000_0020;
    let mut automation = shop_with_rations(true);
    automation.owned = vec![
        owned(SWORD, "Sword", 40, 1, 0x1),
        InventoryItem {
            object_class: ObjectClass::Container,
            container_slot: 1,
            ..owned(PACK, "Pack", 0, 1, 0x200)
        },
        InventoryItem {
            container_id: PACK,
            container_slot: 2,
            ..owned(HIGH_SWORD, "Sword", 40, 1, 0x1)
        },
        coin(100),
    ];
    let mut shop = shop(
        automation,
        VendorTradeSettings {
            test_mode: true,
            ..VendorTradeSettings::default()
        },
        None,
    );
    shop.write_profile("mosstank/ub/autovendor/Shopkeeper.utl");

    shop.open(SHOPKEEPER);
    shop.tick(0.1, true);

    let messages = &shop.fake().messages;
    assert!(
        messages
            .iter()
            .any(|line| line == "[UB] AutoVendor TEST MODE")
    );
    let buys: Vec<&String> = messages
        .iter()
        .filter(|line| line.starts_with("[UB] Buy Items:"))
        .collect();
    assert_eq!(buys.len(), 1);
    // ^\[UB\] Buy Items:\n  .+ -> Ration \* 10 - rations$
    let buy = buys[0];
    let body = buy
        .strip_prefix("[UB] Buy Items:\n  ")
        .expect("one listed buy");
    assert!(!body.contains('\n'));
    let category = body
        .strip_suffix(" -> Ration * 10 - rations")
        .expect("the ration line");
    assert!(!category.is_empty());
    let sells: Vec<&String> = messages
        .iter()
        .filter(|line| line.starts_with("[UB] Sell Items:"))
        .collect();
    assert_eq!(
        sells,
        [
            "[UB] Sell Items:\n  Main Pack: <Tell:IIDString:438347936:select|20>Sword</Tell> - swords\n  Pack #2: <Tell:IIDString:438347936:select|-2147483616>Sword</Tell> - swords"
        ]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradeControllerTests.cs :: ProfileIsResolvedByTierAndAMissingOneRefusesToStart
#[test]
fn profile_is_resolved_by_tier_and_a_missing_one_refuses_to_start() {
    let mut automation = shop_with_rations(true);
    automation.owned = vec![coin(100)];
    let mut shop = shop(automation, VendorTradeSettings::default(), None);

    shop.open(SHOPKEEPER);
    assert!(!shop.tick(0.1, true));
    assert!(!shop.controller.is_running());
    assert!(shop.controller.status().contains("Shopkeeper.utl"));

    // A default in the character's folder is found for any vendor.
    shop.write_profile("mosstank/ub/Coldeve/Acdream/autovendor/default.utl");
    assert!(shop.try_start(None));
    assert!(shop.controller.is_running());
    shop.controller.reset();

    // A named profile on the command line resolves the same way.
    shop.write_profile("mosstank/ub/autovendor/recomp.utl");
    assert!(shop.try_start(Some("recomp")));
    assert!(shop.controller.is_running());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradeControllerTests.cs :: ProtectedItemsAreNeverStagedForSale
#[test]
fn protected_items_are_never_staged_for_sale() {
    let mut automation = shop_with_rations(false);
    let wand = owned(21, "Sword Wand", 40, 1, 0x1);
    let worn = InventoryItem {
        equipped_location: 1,
        ..owned(22, "Sword Worn", 40, 1, 0x1)
    };
    let inscribed = owned(23, "Sword Inscribed", 40, 1, 0x1);
    let tinkered = InventoryItem {
        num_times_tinkered: 1,
        ..owned(24, "Sword Tinkered", 40, 1, 0x1)
    };
    let attuned = owned(25, "Sword Attuned", 40, 1, 0x1);
    let retained = owned(26, "Sword Retained", 40, 1, 0x1);
    let plain = owned(27, "Sword Plain", 40, 1, 0x1);
    automation.properties.insert(
        inscribed.object_id,
        ItemProperties {
            strings: vec![(7, "keep".to_owned())],
            ..ItemProperties::default()
        },
    );
    automation.properties.insert(
        attuned.object_id,
        ItemProperties {
            ints: vec![(114, 1)],
            ..ItemProperties::default()
        },
    );
    automation.properties.insert(
        retained.object_id,
        ItemProperties {
            bools: vec![(88, true)],
            ..ItemProperties::default()
        },
    );
    automation.owned = vec![
        wand.clone(),
        worn,
        inscribed,
        tinkered,
        attuned,
        retained,
        plain.clone(),
        coin(10),
    ];
    let mut shop = shop(automation, VendorTradeSettings::default(), None);
    shop.cx.protected = HashSet::from([wand.object_id]);
    shop.write_profile("mosstank/ub/autovendor/Shopkeeper.utl");

    shop.open(SHOPKEEPER);
    assert!(shop.tick(0.1, true));
    assert!(shop.tick(0.1, true));
    assert_eq!(shop.fake().sell_all_calls, [vec![plain.object_id]]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradeControllerTests.cs :: AStagingLineWithoutANameOrWithACountBelowOneIsTheUsage
#[test]
fn a_staging_line_without_a_name_or_with_a_count_below_one_is_the_usage() {
    for command in [
        "addsellp 5",
        "addbuyp 5",
        "addsell -5 Sword",
        "addsell 0 Sword",
        "addbuy 0 Ration",
        "addbuy -2 Ration",
    ] {
        let mut automation = shop_with_rations(true);
        automation.owned = vec![owned(SWORD, "Sword", 40, 3, 0x1), coin(10)];
        let mut shop = shop(automation, VendorTradeSettings::default(), None);
        shop.open(SHOPKEEPER);

        assert_eq!(shop.command(command), None, "{command}");
        assert!(shop.fake().staged.is_empty());
        assert!(shop.fake().staged_sells.is_empty());
        assert!(shop.fake().moves.is_empty());
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradeControllerTests.cs :: VendorCommandsStageCommitClearAndOpen
#[test]
fn vendor_commands_stage_commit_clear_and_open() {
    let mut automation = shop_with_rations(true);
    automation.owned = vec![owned(SWORD, "Sword", 40, 1, 0x1), coin(10)];
    let mut shop = shop(automation, VendorTradeSettings::default(), None);

    assert_eq!(
        shop.command("addbuy 5 Ration").unwrap(),
        [
            "[UB] Name: Ration count: 5 param:5 Ration",
            "[UB] Error: addbuy: No vendor open"
        ]
    );

    shop.open(SHOPKEEPER);
    shop.command("addbuy 5 Ration");
    assert_eq!(shop.fake().staged, [(RATION_LISTING, 5)]);
    shop.command("addbuyp 2 rat");
    assert_eq!(shop.fake().staged, [(RATION_LISTING, 7)]);
    assert_eq!(
        shop.command("addbuy Rock").unwrap()[1],
        "[UB] Error: addbuy: Unable to find item named 'Rock' in vendor sell list"
    );

    shop.command("addsellp swo");
    assert_eq!(shop.fake().staged_sells, [SWORD]);
    shop.command("clearsell");
    assert!(shop.fake().staged_sells.is_empty());
    shop.command("clearbuy");
    assert!(shop.fake().staged.is_empty());

    shop.command("addsell Sword");
    shop.command("sellall");
    assert_eq!(shop.fake().sell_all_calls.len(), 1);
    shop.command("addbuy Ration");
    shop.command("buyall");
    assert_eq!(shop.fake().buy_all_calls.len(), 1);

    shop.close();
    shop.fake().world_objects = vec![vendor(SHOPKEEPER, "Shopkeeper")];
    shop.cx.route_owns_vendor_open = true;
    assert!(
        shop.command("open Shopkeeper").unwrap()[0]
            .to_lowercase()
            .contains("route")
    );
    assert!(shop.fake().uses.is_empty());

    shop.cx.route_owns_vendor_open = false;
    shop.command("openp shop");
    assert!(shop.tick(0.1, true));
    assert_eq!(shop.fake().uses, [SHOPKEEPER]);
    // No retry until the tries interval has passed.
    assert!(shop.tick(1.0, true));
    assert_eq!(shop.fake().uses.len(), 1);
    assert!(shop.tick(5.0, true));
    assert_eq!(shop.fake().uses.len(), 2);
    shop.command("opencancel");
    assert!(!shop.tick(5.0, true));
    assert_eq!(shop.fake().uses.len(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradeControllerTests.cs :: VendorOpenFindsTheVendorByIdHexIdNameOrNearest
#[test]
fn vendor_open_finds_the_vendor_by_id_hex_id_name_or_nearest() {
    for command in ["open 600", "open 0x258", "open Armorer", "open"] {
        let mut shop = shop(
            shop_with_rations(true),
            VendorTradeSettings::default(),
            None,
        );
        shop.fake().world_objects = vec![vendor(600, "Armorer"), vendor(SHOPKEEPER, "Shopkeeper")];

        assert_eq!(shop.command(command), Some(Vec::new()), "{command}");
        assert!(shop.tick(0.1, true));

        assert_eq!(shop.fake().uses, [600], "{command}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradeControllerTests.cs :: AMacroStartDropsAnOpenBegunBeforeIt
#[test]
fn a_macro_start_drops_an_open_begun_before_it() {
    let mut automation = shop_with_rations(true);
    automation.owned = vec![coin(10)];
    let mut shop = shop(automation, VendorTradeSettings::default(), None);
    shop.fake().world_objects = vec![vendor(SHOPKEEPER, "Shopkeeper")];

    shop.command("open Shopkeeper");
    assert!(shop.tick(0.1, true));
    assert_eq!(shop.fake().uses, [SHOPKEEPER]);

    shop.controller.on_macro_started();
    assert!(!shop.tick(5.0, true));
    assert!(!shop.tick(5.0, true));
    assert_eq!(shop.fake().uses.len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradeControllerTests.cs :: ARunBailsOnSilenceAndStopsWhenTheShopCloses
#[test]
fn a_run_bails_on_silence_and_stops_when_the_shop_closes() {
    let mut automation = shop_with_rations(true);
    automation.owned = vec![coin(100)];
    let mut shop = shop(automation, VendorTradeSettings::default(), None);
    shop.write_profile("mosstank/ub/autovendor/Shopkeeper.utl");

    shop.open(SHOPKEEPER);
    assert!(shop.tick(0.1, true));
    assert!(shop.tick(0.1, true));
    assert_eq!(shop.fake().buy_all_calls.len(), 1);
    assert!(shop.tick(30.0, true));
    assert!(!shop.tick(31.0, true));
    assert!(!shop.controller.is_running());
    assert!(shop.controller.status().to_lowercase().contains("timeout"));

    shop.close();
    shop.open(SHOPKEEPER);
    assert!(shop.tick(0.1, true));
    assert!(shop.controller.is_running());
    shop.close();
    assert!(!shop.tick(0.1, true));
    assert!(!shop.controller.is_running());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VendorTradeControllerTests.cs :: ARunHoldsTheNavigationLockUntilItEnds
#[test]
fn a_run_holds_the_navigation_lock_until_it_ends() {
    let mut automation = shop_with_rations(true);
    automation.owned = vec![coin(100)];
    let locks = ActionLockTable::shared();
    let mut shop = shop(automation, VendorTradeSettings::default(), Some(&locks));
    shop.write_profile("mosstank/ub/autovendor/Shopkeeper.utl");

    shop.open(SHOPKEEPER);
    assert!(shop.tick(0.1, true));
    assert!(shop.controller.is_running());
    assert!(locks.borrow_mut().is_locked(ActionLockKind::Navigation));
    assert!(locks.borrow_mut().is_locked(ActionLockKind::ItemUse));

    // Longer than one arming: the run re-arms while it lives.
    for i in 0..4 {
        locks.borrow_mut().advance_to(10.0 * f64::from(i + 1));
        assert!(shop.tick(10.0, false));
        assert!(locks.borrow_mut().is_locked(ActionLockKind::Navigation));
    }

    shop.close();
    assert!(!shop.tick(0.1, true));
    assert!(!shop.controller.is_running());
    assert!(!locks.borrow_mut().is_locked(ActionLockKind::Navigation));
    assert!(!locks.borrow_mut().is_locked(ActionLockKind::ItemUse));
}

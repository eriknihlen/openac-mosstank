//! A client kept in memory for the Mag features' tests ([`FakeMagHost`]):
//! the meta engine's fake ([`FakeHost`], every record it answers a field to
//! arrange) and the calls only the Mag features make; and a loot profile
//! in memory ([`FakeVerdict`]).
//!
//! The owned items, the open container and the character are the meta
//! fake's (`meta.owned_items`, `meta.open_container`, `meta.character`);
//! an owned item's properties are the meta fake's appraisals
//! (`meta.properties`).

use std::collections::{HashMap, HashSet, VecDeque};

use ac_plugin_abi::character::{ActiveEnchantment, Attribute, Skill};
use ac_plugin_abi::combat::{CombatMode, CombatResult, CombatSnapshot, CombatTarget};
use ac_plugin_abi::items::{
    EquipmentItem, EquipmentResult, InventoryItem, ItemResult, ItemState, ItemStatus, LootContainer,
};
use ac_plugin_abi::magic::{CastResult, ComponentInfo, SpellInfo, SpellList};
use ac_plugin_abi::navigation::{MovementIntent, NavigationSnapshot, NavigationStatus};
use ac_plugin_abi::peers::{NetworkClient, PeerCast};
use ac_plugin_abi::social::{FellowMember, FellowshipState, LoginState, SelectionState};
use ac_plugin_abi::world::{ItemProperties, WorldObject, WorldTime};
use ac_plugin_abi::{CharacterInfo, ChatLine, LogLevel};
use mosstank_meta::host::{MetaHost, PanelDescriptor, SpellExtras, UbDisplay, UbLineKind};
pub use mosstank_meta::testing::{FakeHost, SharedStorage};
use mosstank_stores::storage::{PluginStorage, StorageError};

use crate::host::{LootVerdict, MagHost, Verdict};

/// A client in memory.
#[derive(Debug)]
pub struct FakeMagHost {
    /// Everything the meta engine reaches.
    pub meta: FakeHost,
    /// The owned list is complete.
    pub complete: bool,
    /// Every enchantment on the character, surpassed ones too.
    pub all_enchantments: Vec<ActiveEnchantment>,
    /// What the open container holds.
    pub loot: Vec<InventoryItem>,
    /// The monotonic clock, milliseconds.
    pub now_ms: f64,
    /// Every object an appraisal was asked for, in order.
    pub identified: Vec<u32>,
    /// What the next appraisals answer, in order; `Started` once empty.
    pub identify_answers: VecDeque<ItemStatus>,
    /// How many times the owned items were listed.
    pub owned_listings: usize,
    /// How many times an owned item's properties were read.
    pub property_reads: usize,
    /// The UTC clock, Unix milliseconds.
    pub utc_ms: i64,
    /// The plugin's own data: the meta fake's storage (`meta.storage`,
    /// shared), each write noted.
    pub storage: CountingStorage,
}

impl Default for FakeMagHost {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeMagHost {
    /// The meta fake's character in the world, its owned list complete,
    /// the clock at 0, its storage empty and available.
    #[must_use]
    pub fn new() -> Self {
        let mut meta = FakeHost::new();
        // The plugin's own data is always there.
        meta.storage = SharedStorage::new();
        let storage = CountingStorage {
            inner: meta.storage.clone(),
            writes: Vec::new(),
        };
        Self {
            meta,
            complete: true,
            all_enchantments: Vec::new(),
            loot: Vec::new(),
            now_ms: 0.0,
            identified: Vec::new(),
            identify_answers: VecDeque::new(),
            owned_listings: 0,
            property_reads: 0,
            utc_ms: 1_762_560_000_000,
            storage,
        }
    }

    /// The text of every line shown in chat.
    #[must_use]
    pub fn posted_text(&self) -> Vec<String> {
        self.meta.posted_text()
    }
}

impl MetaHost for FakeMagHost {
    fn log(&mut self, level: LogLevel, message: &str) {
        self.meta.log(level, message)
    }

    fn chat_post(&mut self, text: &str, log_text_type: u32) {
        self.meta.chat_post(text, log_text_type)
    }

    fn chat_submit(&mut self, line: &str) -> bool {
        self.meta.chat_submit(line)
    }

    fn chat_compose(&mut self, text: &str) -> bool {
        self.meta.chat_compose(text)
    }

    fn chat_captured(&mut self, after_sequence: u64) -> Vec<ChatLine> {
        self.meta.chat_captured(after_sequence)
    }

    fn chat_add_filter(&mut self) -> u32 {
        self.meta.chat_add_filter()
    }

    fn chat_remove_filter(&mut self, handle: u32) -> bool {
        self.meta.chat_remove_filter(handle)
    }

    fn ub_display(&mut self, kind: UbLineKind) -> UbDisplay {
        self.meta.ub_display(kind)
    }

    fn character(&mut self) -> Option<CharacterInfo> {
        self.meta.character()
    }

    fn skill(&mut self, skill_id: u32) -> Option<Skill> {
        self.meta.skill(skill_id)
    }

    fn attributes(&mut self) -> Vec<Attribute> {
        self.meta.attributes()
    }

    fn enchantments(&mut self) -> Vec<ActiveEnchantment> {
        self.meta.enchantments()
    }

    fn objects(&mut self) -> Vec<WorldObject> {
        self.meta.objects()
    }

    fn object(&mut self, object_id: u32) -> Option<WorldObject> {
        self.meta.object(object_id)
    }

    fn properties(&mut self, object_id: u32) -> Option<ItemProperties> {
        self.meta.properties(object_id)
    }

    fn int_property(&mut self, object_id: u32, property: u32) -> Option<i32> {
        self.meta.int_property(object_id, property)
    }

    fn identify(&mut self, object_id: u32) -> ItemResult {
        self.identified.push(object_id);
        let status = self
            .identify_answers
            .pop_front()
            .unwrap_or(ItemStatus::Started);
        ItemResult::of(status)
    }

    fn open_container(&mut self) -> u32 {
        self.meta.open_container()
    }

    fn world_time(&mut self) -> WorldTime {
        self.meta.world_time()
    }

    fn navigation(&mut self) -> NavigationSnapshot {
        self.meta.navigation()
    }

    fn set_movement_intent(&mut self, intent: MovementIntent) -> NavigationStatus {
        self.meta.set_movement_intent(intent)
    }

    fn clear_movement_intent(&mut self) -> NavigationStatus {
        self.meta.clear_movement_intent()
    }

    fn selection(&mut self) -> SelectionState {
        self.meta.selection()
    }

    fn select(&mut self, object_id: u32) -> bool {
        self.meta.select(object_id)
    }

    fn item_state(&mut self) -> ItemState {
        self.meta.item_state()
    }

    fn owned_items(&mut self) -> Vec<InventoryItem> {
        self.owned_listings += 1;
        self.meta.owned_items()
    }

    fn use_item(&mut self, object_id: u32) -> ItemResult {
        self.meta.use_item(object_id)
    }

    fn apply_item(&mut self, object_id: u32, target_id: u32) -> ItemResult {
        self.meta.apply_item(object_id, target_id)
    }

    fn give_item(&mut self, object_id: u32, target_id: u32, amount: u32) -> ItemResult {
        self.meta.give_item(object_id, target_id, amount)
    }

    fn drop_item(&mut self, object_id: u32, amount: u32) -> ItemResult {
        self.meta.drop_item(object_id, amount)
    }

    fn move_item(
        &mut self,
        object_id: u32,
        container_id: u32,
        amount: u32,
        placement: i32,
        join_stack: bool,
    ) -> ItemResult {
        self.meta
            .move_item(object_id, container_id, amount, placement, join_stack)
    }

    fn salvage(&mut self, tool_id: u32, item_ids: Vec<u32>) -> ItemResult {
        self.meta.salvage(tool_id, item_ids)
    }

    fn corpses(&mut self, maximum_metres: f32) -> Vec<LootContainer> {
        self.meta.corpses(maximum_metres)
    }

    fn owned_equipment(&mut self) -> Vec<EquipmentItem> {
        self.meta.owned_equipment()
    }

    fn equip(&mut self, object_id: u32, location: u32) -> EquipmentResult {
        self.meta.equip(object_id, location)
    }

    fn equipment_busy(&mut self) -> bool {
        self.meta.equipment_busy()
    }

    fn spell(&mut self, spell_id: u32) -> Option<SpellInfo> {
        self.meta.spell(spell_id)
    }

    fn known_spells(&mut self, list: SpellList) -> Vec<SpellInfo> {
        self.meta.known_spells(list)
    }

    fn is_spell_known(&mut self, spell_id: u32) -> bool {
        self.meta.is_spell_known(spell_id)
    }

    fn spell_component(&mut self, component_id: u32) -> Option<ComponentInfo> {
        self.meta.spell_component(component_id)
    }

    fn cooldown_remaining(&mut self, cooldown_id: u32) -> f64 {
        self.meta.cooldown_remaining(cooldown_id)
    }

    fn is_casting(&mut self) -> bool {
        self.meta.is_casting()
    }

    fn has_components(&mut self, spell_id: u32) -> bool {
        self.meta.has_components(spell_id)
    }

    fn cast(&mut self, spell_id: u32, target_id: u32) -> CastResult {
        self.meta.cast(spell_id, target_id)
    }

    fn spell_extras(&mut self, spell_id: u32) -> SpellExtras {
        self.meta.spell_extras(spell_id)
    }

    fn combat(&mut self) -> CombatSnapshot {
        self.meta.combat()
    }

    fn hostile_targets(&mut self, maximum_metres: f32) -> Vec<CombatTarget> {
        self.meta.hostile_targets(maximum_metres)
    }

    fn enter_combat_mode(&mut self, mode: CombatMode) -> CombatResult {
        self.meta.enter_combat_mode(mode)
    }

    fn fellowship(&mut self) -> FellowshipState {
        self.meta.fellowship()
    }

    fn fellowship_roster(&mut self) -> Vec<FellowMember> {
        self.meta.fellowship_roster()
    }

    fn login(&mut self) -> LoginState {
        self.meta.login()
    }

    fn login_set_next(&mut self, character_id: u32) -> bool {
        self.meta.login_set_next(character_id)
    }

    fn login_clear_next(&mut self) -> bool {
        self.meta.login_clear_next()
    }

    fn peer_clients(&mut self) -> Vec<NetworkClient> {
        self.meta.peer_clients()
    }

    fn peer_casts(&mut self, after_sequence: i64) -> Vec<PeerCast> {
        self.meta.peer_casts(after_sequence)
    }

    fn has_ui(&mut self) -> bool {
        self.meta.has_ui()
    }

    fn register_panel(&mut self, descriptor: &PanelDescriptor, markup: &str) -> Option<u32> {
        self.meta.register_panel(descriptor, markup)
    }

    fn unregister_panel(&mut self, handle: u32) {
        self.meta.unregister_panel(handle)
    }

    fn ui_view_exists(&mut self, view: &str) -> bool {
        self.meta.ui_view_exists(view)
    }

    fn ui_view_visible(&mut self, view: &str) -> bool {
        self.meta.ui_view_visible(view)
    }

    fn ui_control_exists(&mut self, view: &str, control: &str) -> bool {
        self.meta.ui_control_exists(view, control)
    }

    fn ui_set_control_label(&mut self, view: &str, control: &str, label: &str) -> bool {
        self.meta.ui_set_control_label(view, control, label)
    }

    fn ui_set_control_visible(&mut self, view: &str, control: &str, visible: bool) -> bool {
        self.meta.ui_set_control_visible(view, control, visible)
    }

    fn storage(&mut self) -> &mut dyn PluginStorage {
        &mut self.storage
    }
}

impl MagHost for FakeMagHost {
    fn owned_inventory_complete(&mut self) -> bool {
        self.complete
    }

    fn item_properties(&mut self, object_id: u32) -> Option<ItemProperties> {
        self.property_reads += 1;
        self.meta.properties.get(&object_id).cloned()
    }

    fn all_enchantments(&mut self) -> Vec<ActiveEnchantment> {
        self.all_enchantments.clone()
    }

    fn loot_contents(&mut self) -> Vec<InventoryItem> {
        self.loot.clone()
    }

    fn now_ms(&mut self) -> f64 {
        self.now_ms
    }

    fn utc_unix_ms(&mut self) -> i64 {
        self.utc_ms
    }
}

/// A loot profile in memory: the verdict on each item by object id (no
/// verdict: no profile for that item), and the items it must appraise
/// before it decides.
#[derive(Debug, Default)]
pub struct FakeVerdict(pub HashMap<u32, Verdict>, pub HashSet<u32>);

impl LootVerdict for FakeVerdict {
    fn verdict(&mut self, item: &InventoryItem, _properties: &ItemProperties) -> Option<Verdict> {
        self.0.get(&item.object_id).cloned()
    }

    fn needs_identify(&mut self, item: &InventoryItem, _properties: &ItemProperties) -> bool {
        self.1.contains(&item.object_id)
    }
}

/// Storage in memory that notes every key written.
#[derive(Debug, Default)]
pub struct CountingStorage {
    /// The storage itself (shared with every clone).
    pub inner: SharedStorage,
    /// Every key written, in order.
    pub writes: Vec<String>,
}

impl PluginStorage for CountingStorage {
    fn is_available(&mut self) -> bool {
        self.inner.is_available()
    }

    fn root_path(&mut self) -> Option<String> {
        self.inner.root_path()
    }

    fn read_text(&mut self, key: &str) -> Result<Option<String>, StorageError> {
        self.inner.read_text(key)
    }

    fn list(&mut self, prefix: &str) -> Result<Vec<String>, StorageError> {
        self.inner.list(prefix)
    }

    fn write_text(&mut self, key: &str, content: &str) -> Result<(), StorageError> {
        self.writes.push(key.to_owned());
        self.inner.write_text(key, content)
    }

    fn ensure_directory(&mut self, prefix: &str) -> Result<bool, StorageError> {
        self.inner.ensure_directory(prefix)
    }

    fn delete(&mut self, key: &str) -> Result<bool, StorageError> {
        self.inner.delete(key)
    }
}

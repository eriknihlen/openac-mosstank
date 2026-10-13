//! A client kept in memory for the UB tab's tests ([`FakeUbHost`]): the
//! meta engine's fake ([`FakeHost`], every record it answers a field to
//! arrange) and the calls only the UB commands make, each recorded.

use std::collections::HashMap;

use ac_plugin_abi::character::{ActiveEnchantment, Attribute, OptionResult, OptionStatus, Skill};
use ac_plugin_abi::combat::{CombatMode, CombatResult, CombatSnapshot, CombatTarget};
use ac_plugin_abi::items::{
    EquipmentItem, EquipmentResult, InventoryItem, ItemResult, ItemState, ItemStatus, LootContainer,
};
use ac_plugin_abi::magic::{CastResult, ComponentInfo, SpellInfo, SpellList};
use ac_plugin_abi::navigation::{
    MoveChannel, MoveDirection, MovePace, MoveReport, MoveUnit, MovementIntent, NavigationSnapshot,
    NavigationStatus,
};
use ac_plugin_abi::peers::{NetworkClient, PeerCast};
use ac_plugin_abi::social::{
    AllegianceResult, AllegianceStatus, FellowMember, FellowshipState, LoginState, SelectionState,
};
use ac_plugin_abi::world::{Floorplan, ItemProperties, WorldObject, WorldTime};
use ac_plugin_abi::{CharacterInfo, ChatLine, LogLevel};
use mosstank_meta::host::{MetaHost, PanelDescriptor, SpellExtras, UbDisplay, UbLineKind};
pub use mosstank_meta::testing::{FakeHost, SharedStorage};
use mosstank_stores::storage::PluginStorage;

use crate::host::UbHost;

/// A client in memory.
#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct FakeUbHost {
    /// Everything the meta engine reaches.
    pub meta: FakeHost,
    /// The client takes no chat line (`Submit` answers false).
    pub refuse_submits: bool,
    /// The chat input interceptors installed.
    pub interceptors: Vec<u32>,
    /// How many were removed.
    pub interceptors_removed: usize,
    next_interceptor: u32,
    /// Every heading the client was asked to face.
    pub faced_headings: Vec<f32>,
    /// What a face answers.
    pub face_status: NavigationStatus,
    /// Every client-driven move begun.
    pub moves: Vec<(MoveDirection, MovePace, f32, MoveUnit)>,
    /// What a move answers.
    pub move_status: NavigationStatus,
    /// Every channel stopped.
    pub stopped_moves: Vec<MoveChannel>,
    /// Every jump's power.
    pub jumps: Vec<f32>,
    /// What a jump answers.
    pub jump_status: NavigationStatus,
    /// The move report.
    pub move_report: MoveReport,
    /// The character options.
    pub character_options: Vec<(String, bool)>,
    /// What a change of an option the client has answers.
    pub option_status: OptionStatus,
    /// Every option change asked for.
    pub option_changes: Vec<(String, bool)>,
    /// What an allegiance command answers.
    pub allegiance_result: AllegianceResult,
    /// Every swear, by patron.
    pub swears: Vec<u32>,
    /// Every break, by target.
    pub breaks: Vec<u32>,
    /// Every container opened.
    pub opened_loot: Vec<u32>,
    /// Every container closed.
    pub closed_loot: Vec<u32>,
    /// How many times the session was asked to close.
    pub close_requests: usize,
    /// What a close answers.
    pub close_result: bool,
    /// Every broadcast: line, tags, delay.
    pub broadcasts: Vec<(String, Vec<String>, i32)>,
    /// What a broadcast answers.
    pub broadcast_result: bool,
    /// The labels this client answers to, as last set.
    pub peer_tags: Vec<String>,
    /// The leases taken: handle and capabilities.
    pub peer_leases: Vec<(u32, u32)>,
    /// Every lease ended.
    pub peer_unsubscribed: Vec<u32>,
    next_lease: u32,
    /// The other clients are read and written.
    pub peers_connected: bool,
    /// How many times the other clients were listed.
    pub peer_capture_count: usize,
    /// Every object selected, in order.
    pub selections: Vec<u32>,
    /// Cells that are sealed dungeon cells.
    pub sealed_cells: Vec<u32>,
    /// Every cell is a sealed dungeon cell.
    pub all_sealed: bool,
    /// Floorplans by landblock.
    pub floorplans: HashMap<u32, Floorplan>,
    /// How many floorplans were asked for.
    pub floorplan_requests: usize,
}

impl Default for FakeUbHost {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeUbHost {
    /// The meta fake's character in the world, every command accepted.
    #[must_use]
    pub fn new() -> Self {
        Self {
            meta: FakeHost::new(),
            refuse_submits: false,
            interceptors: Vec::new(),
            interceptors_removed: 0,
            next_interceptor: 1,
            faced_headings: Vec::new(),
            face_status: NavigationStatus::Accepted,
            moves: Vec::new(),
            move_status: NavigationStatus::Accepted,
            stopped_moves: Vec::new(),
            jumps: Vec::new(),
            jump_status: NavigationStatus::Accepted,
            move_report: MoveReport::default(),
            character_options: Vec::new(),
            option_status: OptionStatus::Accepted,
            option_changes: Vec::new(),
            allegiance_result: AllegianceResult {
                status: AllegianceStatus::Sent,
                notice: None,
            },
            swears: Vec::new(),
            breaks: Vec::new(),
            opened_loot: Vec::new(),
            closed_loot: Vec::new(),
            close_requests: 0,
            close_result: true,
            broadcasts: Vec::new(),
            broadcast_result: true,
            peer_tags: Vec::new(),
            peer_leases: Vec::new(),
            peer_unsubscribed: Vec::new(),
            next_lease: 1,
            peers_connected: true,
            peer_capture_count: 0,
            selections: Vec::new(),
            sealed_cells: Vec::new(),
            all_sealed: false,
            floorplans: HashMap::new(),
            floorplan_requests: 0,
        }
    }

    /// The text of every line shown in chat.
    #[must_use]
    pub fn posted_text(&self) -> Vec<String> {
        self.meta.posted_text()
    }
}

impl MetaHost for FakeUbHost {
    fn log(&mut self, level: LogLevel, message: &str) {
        self.meta.log(level, message)
    }

    fn chat_post(&mut self, text: &str, log_text_type: u32) {
        self.meta.chat_post(text, log_text_type)
    }

    fn chat_submit(&mut self, line: &str) -> bool {
        if self.refuse_submits {
            return false;
        }
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
        self.meta.identify(object_id)
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
        self.selections.push(object_id);
        self.meta.select(object_id)
    }

    fn item_state(&mut self) -> ItemState {
        self.meta.item_state()
    }

    fn owned_items(&mut self) -> Vec<InventoryItem> {
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
        self.peer_capture_count += 1;
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
        self.meta.storage()
    }

    fn shared_storage(&mut self) -> Option<Box<dyn PluginStorage>> {
        self.meta.shared_storage()
    }
}

impl UbHost for FakeUbHost {
    fn chat_add_interceptor(&mut self) -> Result<u32, String> {
        let handle = self.next_interceptor;
        self.next_interceptor += 1;
        self.interceptors.push(handle);
        Ok(handle)
    }

    fn chat_remove_interceptor(&mut self, handle: u32) -> bool {
        let before = self.interceptors.len();
        self.interceptors.retain(|installed| *installed != handle);
        let removed = before != self.interceptors.len();
        if removed {
            self.interceptors_removed += 1;
        }
        removed
    }

    fn face_heading(&mut self, heading_degrees: f32) -> NavigationStatus {
        self.faced_headings.push(heading_degrees);
        self.face_status
    }

    fn move_by(
        &mut self,
        direction: MoveDirection,
        pace: MovePace,
        amount: f32,
        unit: MoveUnit,
    ) -> NavigationStatus {
        self.moves.push((direction, pace, amount, unit));
        self.move_status
    }

    fn stop_moving_channel(&mut self, channel: MoveChannel) -> NavigationStatus {
        self.stopped_moves.push(channel);
        NavigationStatus::Accepted
    }

    fn jump(&mut self, power: f32) -> NavigationStatus {
        self.jumps.push(power);
        self.jump_status
    }

    fn move_report(&mut self) -> MoveReport {
        self.move_report.clone()
    }

    fn skills(&mut self) -> Vec<Skill> {
        self.meta.skills.clone()
    }

    fn character_options(&mut self) -> Vec<(String, bool)> {
        self.character_options.clone()
    }

    fn set_character_option(&mut self, name: &str, value: bool) -> OptionResult {
        self.option_changes.push((name.to_owned(), value));
        let status = self.option_status;
        let Some((_, held)) = self
            .character_options
            .iter_mut()
            .find(|(option, _)| option.eq_ignore_ascii_case(name))
        else {
            return OptionResult {
                status: OptionStatus::UnknownOption,
                notice: None,
            };
        };
        if status == OptionStatus::Accepted {
            *held = value;
        }
        OptionResult {
            status,
            notice: None,
        }
    }

    fn allegiance_swear(&mut self, patron_id: u32) -> AllegianceResult {
        self.swears.push(patron_id);
        self.allegiance_result.clone()
    }

    fn allegiance_break(&mut self, target_id: u32) -> AllegianceResult {
        self.breaks.push(target_id);
        self.allegiance_result.clone()
    }

    fn open_loot(&mut self, container_id: u32) -> ItemResult {
        self.opened_loot.push(container_id);
        ItemResult::of(ItemStatus::Started)
    }

    fn close_loot(&mut self, container_id: u32) -> ItemResult {
        self.closed_loot.push(container_id);
        ItemResult::of(ItemStatus::Started)
    }

    fn request_close(&mut self) -> bool {
        self.close_requests += 1;
        self.close_result
    }

    fn broadcast_command(&mut self, line: &str, tags: &[String], delay_ms: i32) -> bool {
        self.broadcasts
            .push((line.to_owned(), tags.to_vec(), delay_ms));
        self.broadcast_result
    }

    fn set_peer_tags(&mut self, tags: &[String]) -> bool {
        self.peer_tags = tags.to_vec();
        true
    }

    fn peer_subscribe(&mut self, capabilities: u32) -> Option<u32> {
        let handle = self.next_lease;
        self.next_lease += 1;
        self.peer_leases.push((handle, capabilities));
        Some(handle)
    }

    fn peer_unsubscribe(&mut self, handle: u32) -> bool {
        self.peer_unsubscribed.push(handle);
        true
    }

    fn peers_connected(&mut self) -> bool {
        self.peers_connected
    }

    fn is_sealed_dungeon(&mut self, cell_id: u32) -> bool {
        self.all_sealed || self.sealed_cells.contains(&cell_id)
    }

    fn dungeon_floorplan(&mut self, landblock_id: u32) -> Option<Floorplan> {
        self.floorplan_requests += 1;
        self.floorplans.get(&(landblock_id & 0xFFFF_0000)).cloned()
    }
}

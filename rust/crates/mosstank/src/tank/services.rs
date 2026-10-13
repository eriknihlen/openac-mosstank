//! What the meta engine asks of the rest of the plugin (`MetaServices`,
//! the panel's side of it), and how the engine and those services share the
//! one client of the call.
//!
//! The engine is handed the client as a [`MetaHost`] and the services as a
//! [`MetaServices`], and it calls them in turn, never both at once. The
//! services need the whole client (to save a profile, to buff, to run a
//! `/vt` line a meta submits), so the client sits in a [`ClientCell`]: the
//! engine reaches it through a [`ClientProxy`] that borrows it for each of
//! its calls, the services borrow it for each of theirs. Everything stays
//! synchronous, as in the C#.

use std::cell::RefCell;

use mosstank_expr::{EvaluationContext, ExpressionError, ExpressionValue, Fault};
use mosstank_formats::meta::MetaProfile;
use mosstank_formats::route::NavigationSettings;
use mosstank_formats::utl::LootRule;
use mosstank_meta::engine::MetaEngine;
use mosstank_meta::host::{
    ActiveEnchantment, Attribute, CastResult, CharacterInfo, ChatLine, CombatMode, CombatResult,
    CombatSnapshot, CombatTarget, ComponentInfo, EquipmentItem, EquipmentResult, FellowMember,
    FellowshipState, InventoryItem, ItemProperties, ItemResult, ItemState, LogLevel, LoginState,
    LootContainer, MetaHost, MovementIntent, NavigationSnapshot, NavigationStatus, NetworkClient,
    PanelDescriptor, PeerCast, SelectionState, Skill, SpellExtras, SpellInfo, SpellList, UbDisplay,
    UbLineKind, WorldObject, WorldTime,
};
use mosstank_meta::services::{MetaServices, PeerNeed};
use mosstank_settings::options;
use mosstank_stores::storage::{PluginStorage, StorageError};
use mosstank_ub::host::{
    AdvancementKind, AdvancementResult, AllegianceResult, AllegianceState, AppraisalState,
    Floorplan, LootDecision, MoveChannel, MoveDirection, MovePace, MoveReport, MoveUnit,
    OptionResult, UbHost, Vital, WorldLabel,
};
use mosstank_ub::tools::count::InventoryCountController;
use openac_plugin_compat::number::{self, NumberStyles};

use super::commands;
use super::{Core, TankCx};
use crate::client::TankClient;

/// The client of the call, shared by the engine's host and the services.
pub(crate) struct ClientCell<'c>(pub(crate) RefCell<&'c mut dyn TankClient>);

/// The shell's parts for the call, shared by the meta's services and the
/// UB tab's, each borrowing them for one of its calls.
pub(crate) struct CoreCell<'c>(pub(crate) RefCell<&'c mut Core>);

/// What the services ask of the engine once its call has returned: a
/// meta's own `/vt` line that needs the engine, or its `/ub` line.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EngineRequest {
    /// A `/vt` line (register META-9).
    VtLine(String),
    /// A `/ub` line (register META-9).
    UbLine(String),
    /// `EnableMeta` written by a UB option function.
    MetaEnabled(bool),
    /// What follows a meta load the engine took (register META-2): the
    /// route a meta carried goes, the options come back.
    AfterMetaLoad,
}

/// The engine's host over the shared client.
pub(crate) struct ClientProxy<'s, 'c> {
    cell: &'s ClientCell<'c>,
    storage: StorageProxy<'s, 'c>,
    profiles: ProfileStorageProxy<'s, 'c>,
}

impl<'s, 'c> ClientProxy<'s, 'c> {
    pub(crate) fn new(cell: &'s ClientCell<'c>) -> Self {
        Self {
            cell,
            storage: StorageProxy { cell },
            profiles: ProfileStorageProxy { cell },
        }
    }
}

/// The meta's storage over the shared client.
struct StorageProxy<'s, 'c> {
    cell: &'s ClientCell<'c>,
}

impl PluginStorage for StorageProxy<'_, '_> {
    fn is_available(&mut self) -> bool {
        self.cell.0.borrow_mut().meta().storage().is_available()
    }

    fn root_path(&mut self) -> Option<String> {
        self.cell.0.borrow_mut().meta().storage().root_path()
    }

    fn read_text(&mut self, key: &str) -> Result<Option<String>, StorageError> {
        self.cell.0.borrow_mut().meta().storage().read_text(key)
    }

    fn list(&mut self, prefix: &str) -> Result<Vec<String>, StorageError> {
        self.cell.0.borrow_mut().meta().storage().list(prefix)
    }

    fn write_text(&mut self, key: &str, content: &str) -> Result<(), StorageError> {
        self.cell
            .0
            .borrow_mut()
            .meta()
            .storage()
            .write_text(key, content)
    }

    fn ensure_directory(&mut self, prefix: &str) -> Result<bool, StorageError> {
        self.cell
            .0
            .borrow_mut()
            .meta()
            .storage()
            .ensure_directory(prefix)
    }

    fn delete(&mut self, key: &str) -> Result<bool, StorageError> {
        self.cell.0.borrow_mut().meta().storage().delete(key)
    }

    fn lock(&mut self, key: &str) -> Result<u32, StorageError> {
        self.cell.0.borrow_mut().meta().storage().lock(key)
    }

    fn unlock(&mut self, handle: u32) -> bool {
        self.cell.0.borrow_mut().meta().storage().unlock(handle)
    }
}

/// The shared profile folder over the shared client, as the UB tools reach
/// it.
struct ProfileStorageProxy<'s, 'c> {
    cell: &'s ClientCell<'c>,
}

/// One call to the profile folder.
macro_rules! profiles {
    ($self:ident.$method:ident($($argument:expr),*)) => {
        $self.cell.0.borrow_mut().ub().profile_storage().$method($($argument),*)
    };
}

impl PluginStorage for ProfileStorageProxy<'_, '_> {
    fn is_available(&mut self) -> bool {
        profiles!(self.is_available())
    }

    fn root_path(&mut self) -> Option<String> {
        profiles!(self.root_path())
    }

    fn read_text(&mut self, key: &str) -> Result<Option<String>, StorageError> {
        profiles!(self.read_text(key))
    }

    fn list(&mut self, prefix: &str) -> Result<Vec<String>, StorageError> {
        profiles!(self.list(prefix))
    }

    fn write_text(&mut self, key: &str, content: &str) -> Result<(), StorageError> {
        profiles!(self.write_text(key, content))
    }

    fn ensure_directory(&mut self, prefix: &str) -> Result<bool, StorageError> {
        profiles!(self.ensure_directory(prefix))
    }

    fn delete(&mut self, key: &str) -> Result<bool, StorageError> {
        profiles!(self.delete(key))
    }

    fn lock(&mut self, key: &str) -> Result<u32, StorageError> {
        profiles!(self.lock(key))
    }

    fn unlock(&mut self, handle: u32) -> bool {
        profiles!(self.unlock(handle))
    }
}

/// One forwarded call to the client's meta view.
macro_rules! meta {
    ($self:ident.$method:ident($($argument:expr),*)) => {
        $self.cell.0.borrow_mut().meta().$method($($argument),*)
    };
}

/// One forwarded call to the client's UB view.
macro_rules! ub {
    ($self:ident.$method:ident($($argument:expr),*)) => {
        $self.cell.0.borrow_mut().ub().$method($($argument),*)
    };
}

impl MetaHost for ClientProxy<'_, '_> {
    fn log(&mut self, level: LogLevel, message: &str) {
        meta!(self.log(level, message));
    }
    fn chat_post(&mut self, text: &str, log_text_type: u32) {
        meta!(self.chat_post(text, log_text_type));
    }
    fn chat_submit(&mut self, line: &str) -> bool {
        meta!(self.chat_submit(line))
    }
    fn chat_compose(&mut self, text: &str) -> bool {
        meta!(self.chat_compose(text))
    }
    fn chat_captured(&mut self, after_sequence: u64) -> Vec<ChatLine> {
        meta!(self.chat_captured(after_sequence))
    }
    fn chat_add_filter(&mut self) -> u32 {
        meta!(self.chat_add_filter())
    }
    fn chat_remove_filter(&mut self, handle: u32) -> bool {
        meta!(self.chat_remove_filter(handle))
    }
    fn ub_display(&mut self, kind: UbLineKind) -> UbDisplay {
        meta!(self.ub_display(kind))
    }
    fn character(&mut self) -> Option<CharacterInfo> {
        meta!(self.character())
    }
    fn skill(&mut self, skill_id: u32) -> Option<Skill> {
        meta!(self.skill(skill_id))
    }
    fn attributes(&mut self) -> Vec<Attribute> {
        meta!(self.attributes())
    }
    fn enchantments(&mut self) -> Vec<ActiveEnchantment> {
        meta!(self.enchantments())
    }
    fn objects(&mut self) -> Vec<WorldObject> {
        meta!(self.objects())
    }
    fn object(&mut self, object_id: u32) -> Option<WorldObject> {
        meta!(self.object(object_id))
    }
    fn properties(&mut self, object_id: u32) -> Option<ItemProperties> {
        meta!(self.properties(object_id))
    }
    fn int_property(&mut self, object_id: u32, property: u32) -> Option<i32> {
        meta!(self.int_property(object_id, property))
    }
    fn identify(&mut self, object_id: u32) -> ItemResult {
        meta!(self.identify(object_id))
    }
    fn open_container(&mut self) -> u32 {
        meta!(self.open_container())
    }
    fn world_time(&mut self) -> WorldTime {
        meta!(self.world_time())
    }
    fn navigation(&mut self) -> NavigationSnapshot {
        meta!(self.navigation())
    }
    fn set_movement_intent(&mut self, intent: MovementIntent) -> NavigationStatus {
        meta!(self.set_movement_intent(intent))
    }
    fn clear_movement_intent(&mut self) -> NavigationStatus {
        meta!(self.clear_movement_intent())
    }
    fn selection(&mut self) -> SelectionState {
        meta!(self.selection())
    }
    fn select(&mut self, object_id: u32) -> bool {
        meta!(self.select(object_id))
    }
    fn item_state(&mut self) -> ItemState {
        meta!(self.item_state())
    }
    fn owned_items(&mut self) -> Vec<InventoryItem> {
        meta!(self.owned_items())
    }
    fn use_item(&mut self, object_id: u32) -> ItemResult {
        meta!(self.use_item(object_id))
    }
    fn apply_item(&mut self, object_id: u32, target_id: u32) -> ItemResult {
        meta!(self.apply_item(object_id, target_id))
    }
    fn give_item(&mut self, object_id: u32, target_id: u32, amount: u32) -> ItemResult {
        meta!(self.give_item(object_id, target_id, amount))
    }
    fn drop_item(&mut self, object_id: u32, amount: u32) -> ItemResult {
        meta!(self.drop_item(object_id, amount))
    }
    fn move_item(
        &mut self,
        object_id: u32,
        container_id: u32,
        amount: u32,
        placement: i32,
        join_stack: bool,
    ) -> ItemResult {
        meta!(self.move_item(object_id, container_id, amount, placement, join_stack))
    }
    fn salvage(&mut self, tool_id: u32, item_ids: Vec<u32>) -> ItemResult {
        meta!(self.salvage(tool_id, item_ids))
    }
    fn corpses(&mut self, maximum_metres: f32) -> Vec<LootContainer> {
        meta!(self.corpses(maximum_metres))
    }
    fn owned_equipment(&mut self) -> Vec<EquipmentItem> {
        meta!(self.owned_equipment())
    }
    fn equip(&mut self, object_id: u32, location: u32) -> EquipmentResult {
        meta!(self.equip(object_id, location))
    }
    fn equipment_busy(&mut self) -> bool {
        meta!(self.equipment_busy())
    }
    fn spell(&mut self, spell_id: u32) -> Option<SpellInfo> {
        meta!(self.spell(spell_id))
    }
    fn known_spells(&mut self, list: SpellList) -> Vec<SpellInfo> {
        meta!(self.known_spells(list))
    }
    fn is_spell_known(&mut self, spell_id: u32) -> bool {
        meta!(self.is_spell_known(spell_id))
    }
    fn spell_component(&mut self, component_id: u32) -> Option<ComponentInfo> {
        meta!(self.spell_component(component_id))
    }
    fn cooldown_remaining(&mut self, cooldown_id: u32) -> f64 {
        meta!(self.cooldown_remaining(cooldown_id))
    }
    fn is_casting(&mut self) -> bool {
        meta!(self.is_casting())
    }
    fn has_components(&mut self, spell_id: u32) -> bool {
        meta!(self.has_components(spell_id))
    }
    fn cast(&mut self, spell_id: u32, target_id: u32) -> CastResult {
        meta!(self.cast(spell_id, target_id))
    }
    fn spell_extras(&mut self, spell_id: u32) -> SpellExtras {
        meta!(self.spell_extras(spell_id))
    }
    fn combat(&mut self) -> CombatSnapshot {
        meta!(self.combat())
    }
    fn hostile_targets(&mut self, maximum_metres: f32) -> Vec<CombatTarget> {
        meta!(self.hostile_targets(maximum_metres))
    }
    fn enter_combat_mode(&mut self, mode: CombatMode) -> CombatResult {
        meta!(self.enter_combat_mode(mode))
    }
    fn fellowship(&mut self) -> FellowshipState {
        meta!(self.fellowship())
    }
    fn fellowship_roster(&mut self) -> Vec<FellowMember> {
        meta!(self.fellowship_roster())
    }
    fn login(&mut self) -> LoginState {
        meta!(self.login())
    }
    fn login_set_next(&mut self, character_id: u32) -> bool {
        meta!(self.login_set_next(character_id))
    }
    fn login_clear_next(&mut self) -> bool {
        meta!(self.login_clear_next())
    }
    fn peer_clients(&mut self) -> Vec<NetworkClient> {
        meta!(self.peer_clients())
    }
    fn peer_casts(&mut self, after_sequence: i64) -> Vec<PeerCast> {
        meta!(self.peer_casts(after_sequence))
    }
    fn has_ui(&mut self) -> bool {
        meta!(self.has_ui())
    }
    fn register_panel(&mut self, descriptor: &PanelDescriptor, markup: &str) -> Option<u32> {
        meta!(self.register_panel(descriptor, markup))
    }
    fn unregister_panel(&mut self, handle: u32) {
        meta!(self.unregister_panel(handle));
    }
    fn ui_view_exists(&mut self, view: &str) -> bool {
        meta!(self.ui_view_exists(view))
    }
    fn ui_view_visible(&mut self, view: &str) -> bool {
        meta!(self.ui_view_visible(view))
    }
    fn ui_control_exists(&mut self, view: &str, control: &str) -> bool {
        meta!(self.ui_control_exists(view, control))
    }
    fn ui_set_control_label(&mut self, view: &str, control: &str, label: &str) -> bool {
        meta!(self.ui_set_control_label(view, control, label))
    }
    fn ui_set_control_visible(&mut self, view: &str, control: &str, visible: bool) -> bool {
        meta!(self.ui_set_control_visible(view, control, visible))
    }
    fn storage(&mut self) -> &mut dyn PluginStorage {
        &mut self.storage
    }
    fn shared_storage(&mut self) -> Option<Box<dyn PluginStorage>> {
        meta!(self.shared_storage())
    }
}

impl UbHost for ClientProxy<'_, '_> {
    fn chat_add_interceptor(&mut self) -> Result<u32, String> {
        ub!(self.chat_add_interceptor())
    }
    fn chat_remove_interceptor(&mut self, handle: u32) -> bool {
        ub!(self.chat_remove_interceptor(handle))
    }
    fn face_heading(&mut self, heading_degrees: f32) -> NavigationStatus {
        ub!(self.face_heading(heading_degrees))
    }
    fn move_by(
        &mut self,
        direction: MoveDirection,
        pace: MovePace,
        amount: f32,
        unit: MoveUnit,
    ) -> NavigationStatus {
        ub!(self.move_by(direction, pace, amount, unit))
    }
    fn stop_moving_channel(&mut self, channel: MoveChannel) -> NavigationStatus {
        ub!(self.stop_moving_channel(channel))
    }
    fn jump(&mut self, power: f32) -> NavigationStatus {
        ub!(self.jump(power))
    }
    fn move_report(&mut self) -> MoveReport {
        ub!(self.move_report())
    }
    fn skills(&mut self) -> Vec<Skill> {
        ub!(self.skills())
    }
    fn character_options(&mut self) -> Vec<(String, bool)> {
        ub!(self.character_options())
    }
    fn set_character_option(&mut self, name: &str, value: bool) -> OptionResult {
        ub!(self.set_character_option(name, value))
    }
    fn allegiance_swear(&mut self, patron_id: u32) -> AllegianceResult {
        ub!(self.allegiance_swear(patron_id))
    }
    fn allegiance_break(&mut self, target_id: u32) -> AllegianceResult {
        ub!(self.allegiance_break(target_id))
    }
    fn answer_dialog(&mut self, context_id: u32, accept: bool) -> bool {
        ub!(self.answer_dialog(context_id, accept))
    }
    fn open_loot(&mut self, container_id: u32) -> ItemResult {
        ub!(self.open_loot(container_id))
    }
    fn close_loot(&mut self, container_id: u32) -> ItemResult {
        ub!(self.close_loot(container_id))
    }
    fn request_close(&mut self) -> bool {
        ub!(self.request_close())
    }
    fn broadcast_command(&mut self, line: &str, tags: &[String], delay_ms: i32) -> bool {
        ub!(self.broadcast_command(line, tags, delay_ms))
    }
    fn set_peer_tags(&mut self, tags: &[String]) -> bool {
        ub!(self.set_peer_tags(tags))
    }
    fn peer_subscribe(&mut self, capabilities: u32) -> Option<u32> {
        ub!(self.peer_subscribe(capabilities))
    }
    fn peer_unsubscribe(&mut self, handle: u32) -> bool {
        ub!(self.peer_unsubscribe(handle))
    }
    fn peers_connected(&mut self) -> bool {
        ub!(self.peers_connected())
    }
    fn is_sealed_dungeon(&mut self, cell_id: u32) -> bool {
        ub!(self.is_sealed_dungeon(cell_id))
    }
    fn dungeon_floorplan(&mut self, landblock_id: u32) -> Option<Floorplan> {
        ub!(self.dungeon_floorplan(landblock_id))
    }
    fn loot_rule_matches(
        &mut self,
        rule: &LootRule,
        item: &InventoryItem,
        properties: &ItemProperties,
    ) -> bool {
        ub!(self.loot_rule_matches(rule, item, properties))
    }
    fn loot_needs_identify(
        &mut self,
        item: &InventoryItem,
        properties: &ItemProperties,
        rules: &[LootRule],
    ) -> bool {
        ub!(self.loot_needs_identify(item, properties, rules))
    }
    fn loot_decide(
        &mut self,
        item: &InventoryItem,
        properties: &ItemProperties,
        rules: &[LootRule],
        owned: &[InventoryItem],
    ) -> Option<LootDecision> {
        ub!(self.loot_decide(item, properties, rules, owned))
    }
    fn profile_storage(&mut self) -> &mut dyn PluginStorage {
        &mut self.profiles
    }
    fn merge_items(&mut self, source_id: u32, target_id: u32, amount: u32) -> ItemResult {
        ub!(self.merge_items(source_id, target_id, amount))
    }
    fn appraisal(&mut self) -> AppraisalState {
        ub!(self.appraisal())
    }
    fn forget_stale_item(&mut self, object_id: u32) -> ItemResult {
        ub!(self.forget_stale_item(object_id))
    }
    fn vitals(&mut self) -> Vec<Vital> {
        ub!(self.vitals())
    }
    fn advance(&mut self, kind: AdvancementKind, stat_id: u32, cost: u64) -> AdvancementResult {
        ub!(self.advance(kind, stat_id, cost))
    }
    fn allegiance(&mut self) -> AllegianceState {
        ub!(self.allegiance())
    }
    fn show_labels(&mut self, labels: &[WorldLabel]) -> bool {
        ub!(self.show_labels(labels))
    }
}

/// The plugin as the meta sees it (the panel's `MetaServices` and
/// `ExpressionHostPolicy`).
pub(crate) struct TankServices<'s, 'c> {
    pub(crate) core: &'s CoreCell<'c>,
    pub(crate) client: &'s ClientCell<'c>,
    /// The UB settings, for the meta's event switch and debug setting.
    pub(crate) ub: std::rc::Rc<mosstank_settings::ub::UbSettingCatalog<super::ub::StoreValues>>,
    /// The UB settings store, for `uboptget` and `uboptset`.
    pub(crate) ub_values: super::ub::StoreValues,
    /// The item giver the `/ub` commands share, for `actiontrygiveprofile`.
    pub(crate) ub_give: std::rc::Rc<RefCell<mosstank_ub::tools::give::ProfileGiveController>>,
    /// The counter the `/ub` commands share, for `vtcountprofile`.
    pub(crate) ub_count: std::rc::Rc<RefCell<InventoryCountController>>,
    /// Whether the meta runs, read before the engine's call.
    pub(crate) meta_enabled: bool,
    /// Whether the network readout.s window is open, read before the call
    /// (nothing in a call opens or shuts it).
    pub(crate) readout_shown: bool,
    /// What a meta's own lines ask of the engine, done after its call.
    pub(crate) requests: Vec<EngineRequest>,
}

impl TankServices<'_, '_> {
    /// `BeginExpressionCast`: an expression's cast goes through the macro's
    /// cast tracker; with a cast in flight nothing is sent. The pass is held
    /// at once, as the reference raises its busy count inside the cast.
    fn begin_expression_cast(&mut self, spell_id: u32, target: Option<u32>) {
        if self.core.0.borrow_mut().tracker.borrow().is_busy() {
            return;
        }
        let mut client = self.client.0.borrow_mut();
        let automation = client.rules();
        let Some(spell) = automation.spells().try_get(spell_id) else {
            return;
        };
        if automation.combat_mode() != CombatMode::Magic
            || self
                .core
                .0
                .borrow()
                .tracker
                .borrow()
                .is_school_locked_out(spell.school)
        {
            return;
        }
        if !automation.cast(spell_id, target) {
            return;
        }
        // The untargeted form is tracked with no target.
        let tracked_target = target.unwrap_or(0);
        let target_name = if tracked_target == 0 {
            String::new()
        } else if tracked_target == automation.character().id {
            "yourself".to_owned()
        } else {
            automation
                .object(tracked_target)
                .map(|found| found.name)
                .unwrap_or_default()
        };
        self.core.0.borrow_mut().queue_log(
            mosstank_meta::scheduler::MacroLogChannel::SpellCast,
            &format!(
                "Casting: {} on {tracked_target} ({target_name})",
                spell.name
            ),
        );
        let mut request = mosstank_automation::cast_tracker::CastRequest::new(
            spell.spell_id,
            &spell.name,
            tracked_target,
            &target_name,
            mosstank_automation::cast_tracker::SpellCastTracker::hits_multiple_targets_for(&spell),
        );
        request.saying = &spell.saying;
        request.school = spell.school;
        request.can_kill =
            mosstank_automation::cast_tracker::SpellCastTracker::can_kill_for(&spell);
        request.current_mana = i32::try_from(automation.character().mana.0).unwrap_or(i32::MAX);
        self.core
            .0
            .borrow_mut()
            .tracker
            .borrow_mut()
            .begin(&request);
        self.core.0.borrow_mut().queue_log(
            mosstank_meta::scheduler::MacroLogChannel::CastInfo,
            "SpellCaster: Begin",
        );
        if !self.core.0.borrow_mut().transaction_suspension_held {
            self.core.0.borrow_mut().hold_pass();
        }
    }
}

impl MetaServices for TankServices<'_, '_> {
    /// A meta's chat line: the plugin's own `/vt` verbs run there and then
    /// (register META-2); its `/ub` line runs right after the engine's call
    /// (register META-9); anything else goes to the client.
    fn submit_chat(&mut self, _host: &mut dyn MetaHost, line: &str) -> Option<MetaProfile> {
        if let Some((_, verb, arguments)) = ac_plugin_abi::verbs::split_command_line(line) {
            if verb.eq_ignore_ascii_case(commands::VTANK_VERB) {
                if let Some((name, af)) = commands::meta_load_request(arguments) {
                    let mut client = self.client.0.borrow_mut();
                    let profile = self.core.0.borrow_mut().load_meta_profile_command(
                        &mut **client,
                        &name,
                        af,
                    );
                    if profile.is_some() {
                        self.requests.push(EngineRequest::AfterMetaLoad);
                    }
                    return profile;
                }
                self.requests
                    .push(EngineRequest::VtLine(arguments.to_owned()));
                return None;
            }
            if verb.eq_ignore_ascii_case(commands::UB_VERB) {
                self.requests
                    .push(EngineRequest::UbLine(arguments.to_owned()));
                return None;
            }
        }
        self.client.0.borrow_mut().meta().chat_submit(line);
        None
    }

    fn is_navigation_route_empty(&mut self, _host: &mut dyn MetaHost) -> bool {
        let core = self.core.0.borrow();
        core.navigation
            .has_nothing_left_to_walk(&core.settings.navigation)
    }

    fn needs_buff(&mut self, _host: &mut dyn MetaHost) -> bool {
        let mut client = self.client.0.borrow_mut();
        let mut core = self.core.0.borrow_mut();
        let core = &mut **core;
        core.buff_rule
            .has_anything_due(client.rules(), &core.settings.buffs)
    }

    /// `DistanceFromAnyRoutePoint`.
    fn distance_from_any_route_point(&mut self, _host: &mut dyn MetaHost) -> f64 {
        let snapshot = self.client.0.borrow_mut().rules().navigation_snapshot();
        if !snapshot.is_available {
            return f64::INFINITY;
        }
        mosstank_nav::route_view::nearest_route_distance_meters(
            &snapshot.position,
            &self.core.0.borrow_mut().settings.navigation.waypoints,
        )
    }

    /// `CountMonstersByPriority`.
    fn count_monsters_by_priority(
        &mut self,
        _host: &mut dyn MetaHost,
        priority: i32,
        distance: f64,
    ) -> i32 {
        #[allow(clippy::cast_possible_truncation)]
        let targets = self
            .client
            .0
            .borrow_mut()
            .rules()
            .hostile_targets(distance as f32);
        let count = targets
            .iter()
            .filter(|target| {
                self.core
                    .0
                    .borrow_mut()
                    .settings
                    .combat
                    .resolve_rule(target)
                    .priority()
                    == priority
            })
            .count();
        i32::try_from(count).unwrap_or(i32::MAX)
    }

    fn load_embedded_navigation_route(
        &mut self,
        _host: &mut dyn MetaHost,
        route: Option<&NavigationSettings>,
        name: &str,
    ) {
        let mut client = self.client.0.borrow_mut();
        TankCx::load_embedded_navigation_route(
            &mut self.core.0.borrow_mut(),
            &mut **client,
            route,
            name,
        );
    }

    fn get_option(&mut self, _host: &mut dyn MetaHost, name: &str) -> ExpressionValue {
        self.core
            .0
            .borrow_mut()
            .get_meta_option(self.meta_enabled, name)
    }

    /// Writes an option; `EnableMeta` is taken by the engine itself
    /// (register META-5).
    fn set_option(
        &mut self,
        _host: &mut dyn MetaHost,
        name: &str,
        value: &ExpressionValue,
    ) -> Result<bool, Fault> {
        let written = {
            let mut client = self.client.0.borrow_mut();
            self.core
                .0
                .borrow_mut()
                .set_option(&mut **client, name, value)?
        };
        if written && is_enable_meta(name) {
            self.meta_enabled = value.is_truthy();
        }
        Ok(written)
    }

    /// The UB settings page's switch for the event handlers.
    fn game_event_handlers_enabled(&mut self) -> bool {
        mosstank_ub::catalog::UbCatalog::flag(&*self.ub, "GameEvents.Enabled")
    }

    /// Whether the UtilityBelt debug setting is on.
    fn debug(&mut self) -> bool {
        mosstank_ub::catalog::UbCatalog::ub_debug(&*self.ub)
    }

    /// `_meta.StateChanged = Announce`.
    fn state_changed(&mut self, _host: &mut dyn MetaHost, message: &str, _state: &str) {
        let mut client = self.client.0.borrow_mut();
        Core::announce(&mut **client, message);
    }

    fn meta_state_set(&mut self, state: &str) {
        state.clone_into(&mut self.core.0.borrow_mut().settings.combat.meta_state);
    }

    /// `RequireExpressionPeers`.
    fn require_peers(&mut self, _host: &mut dyn MetaHost, need: PeerNeed) {
        let mut client = self.client.0.borrow_mut();
        self.core.0.borrow_mut().require_expression_peers(
            &mut **client,
            &*self.ub,
            self.meta_enabled,
            self.readout_shown,
            need,
        );
    }

    fn skill_margin(&mut self, hunting: bool) -> i32 {
        if hunting {
            self.core
                .0
                .borrow_mut()
                .settings
                .combat
                .hunt_skill_excess_over_difficulty
        } else {
            self.core
                .0
                .borrow_mut()
                .settings
                .buffs
                .skill_excess_over_difficulty
        }
    }

    fn begin_cast(&mut self, _host: &mut dyn MetaHost, spell_id: u32, target: Option<u32>) -> bool {
        self.begin_expression_cast(spell_id, target);
        true
    }

    fn call_function(
        &mut self,
        host: &mut dyn MetaHost,
        function: &str,
        _context: &mut EvaluationContext<'_>,
        arguments: &[ExpressionValue],
    ) -> Option<Result<ExpressionValue, Fault>> {
        Some(match function {
            "vtgetmeta" => {
                let mut client = self.client.0.borrow_mut();
                self.core
                    .0
                    .borrow_mut()
                    .meta_profiles
                    .selected(client.stores())
                    .map(|name| ExpressionValue::string(&name))
                    .map_err(|error| ExpressionError::evaluation(error).into())
            }
            "vtsetsetting" => self.vt_set_setting(arguments),
            "vtgetsetting" => self.vt_get_setting(arguments),
            "uboptget" | "uboptset" => self.ub_option(function, arguments),
            "vtmacroenabled" => Ok(ExpressionValue::Boolean(
                self.core.0.borrow_mut().combat.enabled(),
            )),
            // The counts as numbers, so a meta can branch on the figure.
            "vtcountitem" => (|| {
                let pattern = arguments[0].as_str("vtcountitem")?;
                InventoryCountController::try_count_by_name(host, pattern)
                    .map(|tally| ExpressionValue::Number(f64::from(tally.total)))
                    .map_err(|error| {
                        ExpressionError::evaluation(format!("vtcountitem: {error}")).into()
                    })
            })(),
            "vtcountprofile" => (|| {
                let profile = arguments[0].as_str("vtcountprofile")?.to_owned();
                let count = self.ub_count.clone();
                Ok(ExpressionValue::Number(self.with_ub_call(|call| {
                    count.borrow_mut().profile_count(call, &profile)
                })))
            })(),
            "actiontrygiveprofile" => (|| {
                let profile = arguments[0].as_str("actiontrygiveprofile")?.to_owned();
                let target = arguments[1].as_str("actiontrygiveprofile")?.to_owned();
                let give = self.ub_give.clone();
                Ok(ExpressionValue::Boolean(self.with_ub_call(|call| {
                    give.borrow_mut().try_start(call, &profile, &target, false)
                })))
            })(),
            "vtcountplayer" => arguments[0].as_number("vtcountplayer").map(|range| {
                ExpressionValue::Number(f64::from(InventoryCountController::count_players_within(
                    host, range,
                )))
            }),
            _ => return None,
        })
    }
}

impl TankServices<'_, '_> {
    /// `vtsetsetting[setting,value]`: an unknown name is 1 and nothing
    /// written; a text that reads as a number is that number.
    fn vt_set_setting(&mut self, arguments: &[ExpressionValue]) -> Result<ExpressionValue, Fault> {
        let name = arguments[0].as_str("vtsetsetting")?.to_owned();
        if !options::is_known(&name) {
            return Ok(ExpressionValue::one());
        }
        let mut value = arguments[1].clone();
        if let ExpressionValue::String(text) = &value
            && let Some(number) = number::parse_f64(text, NumberStyles::FLOAT)
        {
            value = ExpressionValue::Number(number);
        }
        let written = {
            let mut client = self.client.0.borrow_mut();
            self.core
                .0
                .borrow_mut()
                .set_option(&mut **client, &name, &value)?
        };
        // The panel's one write path starts or stops the meta itself.
        if written && is_enable_meta(&name) {
            self.meta_enabled = value.is_truthy();
            self.requests
                .push(EngineRequest::MetaEnabled(self.meta_enabled));
        }
        Ok(ExpressionValue::Boolean(written))
    }

    /// `vtgetsetting[setting]`: a true or false reads as 1 or 0; an unknown
    /// name fails the expression.
    fn vt_get_setting(&mut self, arguments: &[ExpressionValue]) -> Result<ExpressionValue, Fault> {
        let name = arguments[0].as_str("vtgetsetting")?;
        if !options::is_known(name) {
            return Err(ExpressionError::evaluation("Error getting setting from database").into());
        }
        Ok(
            match self
                .core
                .0
                .borrow_mut()
                .get_meta_option(self.meta_enabled, name)
            {
                ExpressionValue::Boolean(flag) => {
                    ExpressionValue::Number(if flag { 1.0 } else { 0.0 })
                }
                other => other,
            },
        )
    }
}

/// The plugin's functions the meta's expressions call, registered with the
/// engine's runtime (`RegisterVtankExpressionFunctions`, the part the
/// shell has; the engine registers `vtsetmetastate` and `vtgetmetastate`).
pub(crate) fn register_service_functions(meta: &mut MetaEngine) {
    let runtime = meta.expressions();
    runtime.register_service_function("vtgetmeta", 0, 0, "vtgetmeta[]");
    runtime.register_service_function("vtsetsetting", 2, 2, "vtsetsetting[setting,value]");
    runtime.register_service_function("vtgetsetting", 1, 1, "vtgetsetting[setting]");
    runtime.register_service_function("vtmacroenabled", 0, 0, "vtmacroenabled[]");
    runtime.register_service_function("uboptset", 2, 2, "uboptset[setting,value]");
    runtime.register_service_function("uboptget", 1, 1, "uboptget[setting]");
    runtime.register_service_function("vtcountitem", 1, 1, "vtcountitem[namepattern]");
    runtime.register_service_function("vtcountplayer", 1, 1, "vtcountplayer[range]");
    runtime.register_service_function("vtcountprofile", 1, 1, "vtcountprofile[lootprofile]");
    runtime.register_service_function(
        "actiontrygiveprofile",
        2,
        2,
        "actiontrygiveprofile[lootprofile,target]",
    );
}

impl TankCx<'_> {
    /// Runs `f` with the engine, its host and the services over the one
    /// client, then does what the services asked of the engine.
    pub(crate) fn with_engine<R>(
        &mut self,
        f: impl FnOnce(&mut MetaEngine, &mut dyn MetaHost, &mut dyn MetaServices) -> R,
    ) -> R {
        let meta_enabled = self.meta.enabled();
        let (result, requests) = {
            let cell = ClientCell(RefCell::new(&mut *self.client));
            let core = CoreCell(RefCell::new(&mut *self.core));
            let mut proxy = ClientProxy::new(&cell);
            let mut services = TankServices {
                core: &core,
                client: &cell,
                ub: self.ub.reader.clone(),
                ub_values: self.ub.values(),
                ub_give: self.ub.commands.tools().give.clone(),
                ub_count: self.ub.commands.tools().count.clone(),
                meta_enabled,
                readout_shown: self.ub.hud.shown(),
                requests: Vec::new(),
            };
            let result = f(self.meta, &mut proxy, &mut services);
            (result, services.requests)
        };
        for request in requests {
            self.run_engine_request(request);
        }
        result
    }

    /// One thing a meta's own line asked of the engine.
    pub(crate) fn run_engine_request(&mut self, request: EngineRequest) {
        match request {
            EngineRequest::VtLine(arguments) => commands::execute_vtank(self, &arguments),
            EngineRequest::UbLine(arguments) => commands::execute_ub(self, &arguments),
            EngineRequest::AfterMetaLoad => self.after_meta_load(),
            EngineRequest::MetaEnabled(enabled) => self.meta.set_enabled(enabled),
        }
    }
}

/// Whether an option name is `EnableMeta`.
pub(crate) fn is_enable_meta(name: &str) -> bool {
    options::canonical(name).is_some_and(|canonical| {
        openac_plugin_compat::text::ordinal::equals_ignore_case(canonical, "EnableMeta")
    })
}

//! The meta engine's view of the client ([`MetaHost`]) over the SDK host,
//! for the length of one plugin call: every method is one SDK call, and the
//! plugin's own storage is the SDK's plugin-data root.
//!
//! The meta's views and its status list are drawn through the SDK's plugin
//! UI ([`crate::meta_windows`]) when the plugin's call carries its windows.
//! What the SDK does not offer yet answers the way a client without it
//! does: stacks not joined on a move. The global variables are read and
//! written through the plugin-data storage of the call that reaches them.

use mosstank_meta::host::{
    ActiveEnchantment, Attribute, CastResult, CharacterInfo, ChatLine, CombatMode, CombatResult,
    CombatSnapshot, CombatTarget, ComponentInfo, EquipmentItem, EquipmentResult, FellowMember,
    FellowshipState, InventoryItem, ItemProperties, ItemResult, ItemState, LogLevel, LoginState,
    LootContainer, MetaHost, MovementIntent, NavigationSnapshot, NavigationStatus, NetworkClient,
    PanelDescriptor, PeerCast, SelectionState, Skill, SpellInfo, SpellList, UbDisplay, UbLineKind,
    WorldObject, WorldTime,
};
use mosstank_stores::storage::PluginStorage;

use crate::automation::{HostAutomation, StorageView};

/// The meta's view of the client: the one client of the call.
pub type ClientHost<'h, 'a> = HostAutomation<'h, 'a>;

impl MetaHost for HostAutomation<'_, '_> {
    fn log(&mut self, level: LogLevel, message: &str) {
        self.host.log(level, message);
    }

    fn chat_post(&mut self, text: &str, log_text_type: u32) {
        self.reader().chat_post_with(text, log_text_type);
    }

    fn chat_submit(&mut self, line: &str) -> bool {
        self.host.chat_submit(line);
        true
    }

    fn chat_compose(&mut self, text: &str) -> bool {
        self.host.chat_compose(text)
    }

    fn chat_captured(&mut self, after_sequence: u64) -> Vec<ChatLine> {
        self.reader().chat_captured(after_sequence)
    }

    fn chat_add_filter(&mut self) -> u32 {
        self.host.chat_add_filter()
    }

    fn chat_remove_filter(&mut self, handle: u32) -> bool {
        self.host.chat_remove_filter(handle)
    }

    /// The UB settings' display for the kind, once the shell said; the
    /// reference's defaults before.
    fn ub_display(&mut self, kind: UbLineKind) -> UbDisplay {
        let index = match kind {
            UbLineKind::Generic => 0,
            UbLineKind::Debug => 1,
            UbLineKind::Expression => 2,
            UbLineKind::Error => 3,
        };
        self.ub_displays.map_or_else(
            || mosstank_meta::chat::ub_default_display(kind),
            |displays| displays[index],
        )
    }

    // The character, an object and what the character owns are the reads
    // every view of the call shares (`HostAutomation`'s kept reads).
    fn character(&mut self) -> Option<CharacterInfo> {
        self.character_info()
    }

    fn skill(&mut self, skill_id: u32) -> Option<Skill> {
        self.reader().skill(skill_id)
    }

    fn attributes(&mut self) -> Vec<Attribute> {
        self.reader().attributes()
    }

    fn enchantments(&mut self) -> Vec<ActiveEnchantment> {
        self.reader().enchantments()
    }

    fn objects(&mut self) -> Vec<WorldObject> {
        self.reader().objects()
    }

    fn object(&mut self, object_id: u32) -> Option<WorldObject> {
        mosstank_automation::Automation::object(self, object_id)
    }

    fn properties(&mut self, object_id: u32) -> Option<ItemProperties> {
        self.reader().properties(object_id)
    }

    fn int_property(&mut self, object_id: u32, property: u32) -> Option<i32> {
        self.reader().int_property(object_id, property)
    }

    fn identify(&mut self, object_id: u32) -> ItemResult {
        self.host.identify(object_id)
    }

    fn open_container(&mut self) -> u32 {
        self.host.open_container()
    }

    fn world_time(&mut self) -> WorldTime {
        self.reader().world_time()
    }

    fn navigation(&mut self) -> NavigationSnapshot {
        self.reader().navigation()
    }

    fn set_movement_intent(&mut self, intent: MovementIntent) -> NavigationStatus {
        self.host.set_movement_intent(intent)
    }

    fn clear_movement_intent(&mut self) -> NavigationStatus {
        self.host.clear_movement_intent()
    }

    fn selection(&mut self) -> SelectionState {
        self.reader().selection()
    }

    fn select(&mut self, object_id: u32) -> bool {
        self.host.select(object_id)
    }

    fn item_state(&mut self) -> ItemState {
        self.reader().item_state()
    }

    fn owned_items(&mut self) -> Vec<InventoryItem> {
        self.owned_shared().to_vec()
    }

    fn owned_items_shared(&mut self) -> std::rc::Rc<[InventoryItem]> {
        self.owned_shared()
    }

    fn use_item(&mut self, object_id: u32) -> ItemResult {
        self.host.use_item(object_id)
    }

    fn apply_item(&mut self, object_id: u32, target_id: u32) -> ItemResult {
        self.host.apply_item(object_id, target_id)
    }

    fn give_item(&mut self, object_id: u32, target_id: u32, amount: u32) -> ItemResult {
        self.host.give_item(object_id, target_id, amount)
    }

    fn drop_item(&mut self, object_id: u32, amount: u32) -> ItemResult {
        self.host.drop_item(object_id, amount)
    }

    fn move_item(
        &mut self,
        object_id: u32,
        container_id: u32,
        amount: u32,
        placement: i32,
        _join_stack: bool,
    ) -> ItemResult {
        self.host
            .move_item(object_id, container_id, amount, placement)
    }

    fn salvage(&mut self, tool_id: u32, item_ids: Vec<u32>) -> ItemResult {
        self.host.salvage(tool_id, item_ids)
    }

    fn corpses(&mut self, maximum_metres: f32) -> Vec<LootContainer> {
        self.reader().corpses(maximum_metres)
    }

    fn owned_equipment(&mut self) -> Vec<EquipmentItem> {
        self.reader().owned_equipment()
    }

    fn equip(&mut self, object_id: u32, location: u32) -> EquipmentResult {
        self.host.equip(object_id, location)
    }

    fn equipment_busy(&mut self) -> bool {
        self.reader().equipment_busy()
    }

    fn spell(&mut self, spell_id: u32) -> Option<SpellInfo> {
        self.reader().spell(spell_id)
    }

    fn known_spells(&mut self, list: SpellList) -> Vec<SpellInfo> {
        self.host.known_spells(list)
    }

    fn is_spell_known(&mut self, spell_id: u32) -> bool {
        self.reader().is_spell_known(spell_id)
    }

    fn spell_component(&mut self, component_id: u32) -> Option<ComponentInfo> {
        self.reader().spell_component(component_id)
    }

    fn cooldown_remaining(&mut self, cooldown_id: u32) -> f64 {
        self.host.cooldown_remaining(cooldown_id)
    }

    fn is_casting(&mut self) -> bool {
        self.reader().is_casting()
    }

    fn has_components(&mut self, spell_id: u32) -> bool {
        self.host.has_components(spell_id)
    }

    fn cast(&mut self, spell_id: u32, target_id: u32) -> CastResult {
        self.host.cast(spell_id, target_id)
    }

    fn combat(&mut self) -> CombatSnapshot {
        self.reader().combat()
    }

    fn hostile_targets(&mut self, maximum_metres: f32) -> Vec<CombatTarget> {
        self.reader().hostile_targets(maximum_metres)
    }

    fn enter_combat_mode(&mut self, mode: CombatMode) -> CombatResult {
        self.host.enter_combat_mode(mode)
    }

    fn fellowship(&mut self) -> FellowshipState {
        self.reader().fellowship()
    }

    fn fellowship_roster(&mut self) -> Vec<FellowMember> {
        self.reader().fellowship_roster()
    }

    fn login(&mut self) -> LoginState {
        self.host.login()
    }

    fn login_set_next(&mut self, character_id: u32) -> bool {
        self.host.login_set_next(character_id)
    }

    fn login_clear_next(&mut self) -> bool {
        self.host.login_clear_next()
    }

    fn peer_clients(&mut self) -> Vec<NetworkClient> {
        self.host.peer_clients()
    }

    fn peer_casts(&mut self, after_sequence: i64) -> Vec<PeerCast> {
        self.host.peer_casts(after_sequence)
    }

    /// A window is attached to the session (`IPluginHost.HasUi`): the
    /// client shows windows only then.
    fn has_ui(&mut self) -> bool {
        self.service().window_attached()
    }

    /// Registers one of the meta's windows (`RegisterPanelContent`) and
    /// keeps it for the plugin; a window the client refuses is logged and
    /// left undrawn, as a view the host cannot draw is.
    fn register_panel(&mut self, descriptor: &PanelDescriptor, markup: &str) -> Option<u32> {
        let windows = self.meta_windows.as_deref_mut()?;
        match windows.register(&mut self.host, descriptor, markup) {
            Ok(panel) => Some(panel),
            Err(error) => {
                self.host.warn(&format!(
                    "MossTank window '{}' was not registered: {error}",
                    descriptor.window_id
                ));
                None
            }
        }
    }

    /// Takes one of the meta's windows down (the registration's `Dispose`).
    fn unregister_panel(&mut self, handle: u32) {
        if let Some(windows) = self.meta_windows.as_deref_mut() {
            windows.unregister(&mut self.host, handle);
        }
    }

    fn ui_view_exists(&mut self, view: &str) -> bool {
        self.host.view_exists(view)
    }

    fn ui_view_visible(&mut self, view: &str) -> bool {
        self.host.view_visible(view)
    }

    fn ui_control_exists(&mut self, view: &str, control: &str) -> bool {
        self.host.control_exists(view, control)
    }

    fn ui_set_control_label(&mut self, view: &str, control: &str, label: &str) -> bool {
        self.host.set_control_label(view, control, label)
    }

    fn ui_set_control_visible(&mut self, view: &str, control: &str, visible: bool) -> bool {
        self.host.set_control_visible(view, control, visible)
    }

    fn storage(&mut self) -> &mut dyn PluginStorage {
        self.storage_view = StorageView::Meta;
        self
    }
}

#[cfg(test)]
mod tests {
    use mosstank_meta::host::{CharacterInfo, LogLevel, MetaHost};
    use mosstank_meta::runtime::ExpressionRuntime;
    use mosstank_meta::services::NoServices;
    use openac_plugin::Host;
    use openac_plugin::testing::FakeHost;

    use super::ClientHost;

    #[test]
    fn the_client_host_reads_the_character_and_writes_chat_and_the_log_through_the_sdk() {
        let mut fake = FakeHost::new("acdream.mosstank");
        fake.character = Some(CharacterInfo {
            id: 1,
            name: "Meta Tester".to_owned(),
            world: "Coldeve".to_owned(),
            ..CharacterInfo::default()
        });
        {
            let mut host = Host::with_transport(&mut fake);
            let mut client = ClientHost::new(&mut host);
            client.chat_post("[MossTank] hello", 5);
            client.log(LogLevel::Warn, "a line");
            assert_eq!(
                client.character().map(|info| info.name).as_deref(),
                Some("Meta Tester")
            );
        }
        assert!(
            fake.logs
                .iter()
                .any(|(level, line)| *level == LogLevel::Warn && line == "a line")
        );
        assert!(
            fake.posted
                .iter()
                .any(|post| post.text == "[MossTank] hello" && post.log_text_type == 5)
        );
    }

    #[test]
    fn an_expression_runs_against_the_sdk_host() {
        let mut fake = FakeHost::new("acdream.mosstank");
        fake.character = Some(CharacterInfo {
            id: 1,
            name: "Meta Tester".to_owned(),
            world: "Coldeve".to_owned(),
            ..CharacterInfo::default()
        });
        let mut host = Host::with_transport(&mut fake);
        let mut client = ClientHost::new(&mut host);
        let mut runtime = ExpressionRuntime::new(
            &mut client,
            Some(1),
            std::rc::Rc::new(mosstank_expr::FixedClock::default()),
        );
        let world = runtime
            .evaluate(&mut client, &mut NoServices, "getworldname[]")
            .unwrap();
        assert_eq!(world.as_str("").unwrap(), "Coldeve");
    }
}

//! The UB tab's view of the client ([`UbHost`]): the calls only the UB
//! commands make, each one SDK call.

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::ItemProperties;
use mosstank_formats::utl::LootRule;
use mosstank_loot::rules;
use mosstank_stores::storage::{PluginStorage, Root, StoreHost};
use mosstank_ub::host::{
    AdvancementKind, AdvancementResult, AllegianceResult, AllegianceState, AppraisalState,
    Floorplan, ItemResult, LootDecision, MoveChannel, MoveDirection, MovePace, MoveReport,
    MoveUnit, NavigationStatus, OptionResult, Skill, UbHost, Vital, WorldLabel,
};

use crate::automation::HostAutomation;

impl UbHost for HostAutomation<'_, '_> {
    fn chat_add_interceptor(&mut self) -> Result<u32, String> {
        self.host.chat_add_interceptor()
    }

    fn chat_remove_interceptor(&mut self, handle: u32) -> bool {
        self.host.chat_remove_interceptor(handle)
    }

    fn face_heading(&mut self, heading_degrees: f32) -> NavigationStatus {
        self.host.face_heading(heading_degrees)
    }

    fn move_by(
        &mut self,
        direction: MoveDirection,
        pace: MovePace,
        amount: f32,
        unit: MoveUnit,
    ) -> NavigationStatus {
        self.host.move_by(direction, pace, amount, unit)
    }

    fn stop_moving_channel(&mut self, channel: MoveChannel) -> NavigationStatus {
        self.host.stop_moving_channel(channel)
    }

    fn jump(&mut self, power: f32) -> NavigationStatus {
        self.host.jump(power)
    }

    fn move_report(&mut self) -> MoveReport {
        self.reader().move_report()
    }

    fn skills(&mut self) -> Vec<Skill> {
        self.reader().skills()
    }

    fn character_options(&mut self) -> Vec<(String, bool)> {
        self.reader().character_options()
    }

    fn set_character_option(&mut self, name: &str, value: bool) -> OptionResult {
        self.host.set_character_option(name, value)
    }

    fn allegiance_swear(&mut self, patron_id: u32) -> AllegianceResult {
        self.host.allegiance_swear(patron_id)
    }

    fn allegiance_break(&mut self, target_id: u32) -> AllegianceResult {
        self.host.allegiance_break(target_id)
    }

    fn answer_dialog(&mut self, context_id: u32, accept: bool) -> bool {
        self.host.answer_dialog(context_id, accept)
    }

    fn open_loot(&mut self, container_id: u32) -> ItemResult {
        self.host.open_loot(container_id)
    }

    fn close_loot(&mut self, container_id: u32) -> ItemResult {
        self.host.close_loot(container_id)
    }

    fn request_close(&mut self) -> bool {
        self.host.request_close()
    }

    fn broadcast_command(&mut self, line: &str, tags: &[String], delay_ms: i32) -> bool {
        self.host.broadcast_command(line, tags, delay_ms)
    }

    fn set_peer_tags(&mut self, tags: &[String]) -> bool {
        self.host.set_peer_tags(tags)
    }

    fn peer_subscribe(&mut self, capabilities: u32) -> Option<u32> {
        self.host.peer_subscribe(capabilities)
    }

    fn peer_unsubscribe(&mut self, handle: u32) -> bool {
        self.host.peer_unsubscribe(handle)
    }

    fn peers_connected(&mut self) -> bool {
        self.reader().peers_connected()
    }

    fn is_sealed_dungeon(&mut self, cell_id: u32) -> bool {
        self.reader().is_sealed_dungeon(cell_id)
    }

    fn dungeon_floorplan(&mut self, landblock_id: u32) -> Option<Floorplan> {
        self.host.dungeon_floorplan(landblock_id)
    }

    fn loot_rule_matches(
        &mut self,
        rule: &LootRule,
        item: &InventoryItem,
        properties: &ItemProperties,
    ) -> bool {
        rules::rule_matches(rule, item, properties, Some(self)).0
    }

    fn loot_needs_identify(
        &mut self,
        item: &InventoryItem,
        properties: &ItemProperties,
        rules: &[LootRule],
    ) -> bool {
        rules::needs_identify(item, properties, rules, Some(self))
    }

    fn loot_decide(
        &mut self,
        item: &InventoryItem,
        properties: &ItemProperties,
        rules: &[LootRule],
        owned: &[InventoryItem],
    ) -> Option<LootDecision> {
        rules::decide(item, properties, rules, owned, None, Some(self)).0
    }

    fn profile_storage(&mut self) -> &mut dyn PluginStorage {
        StoreHost::storage(self, Root::Profiles)
    }

    fn merge_items(&mut self, source_id: u32, target_id: u32, amount: u32) -> ItemResult {
        self.host.merge_items(source_id, target_id, amount)
    }

    fn appraisal(&mut self) -> AppraisalState {
        self.host.loot_state().appraisal
    }

    fn forget_stale_item(&mut self, object_id: u32) -> ItemResult {
        self.host.forget_stale_item(object_id)
    }

    fn vitals(&mut self) -> Vec<Vital> {
        self.reader().vitals()
    }

    fn advance(&mut self, kind: AdvancementKind, stat_id: u32, cost: u64) -> AdvancementResult {
        self.host.advance(kind, stat_id, cost)
    }

    fn allegiance(&mut self) -> AllegianceState {
        self.host.allegiance()
    }

    fn show_labels(&mut self, labels: &[WorldLabel]) -> bool {
        self.host.show_labels(labels)
    }
}

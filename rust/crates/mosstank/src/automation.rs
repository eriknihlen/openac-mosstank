//! The rules' view of the client ([`Automation`]) over the SDK's host, for
//! the length of one plugin call.

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::{ActiveEnchantment, Attribute, Skill};
use ac_plugin_abi::combat::{AttackHeight, CombatMode, CombatResult, CombatSnapshot, CombatTarget};
use ac_plugin_abi::items::{
    EquipmentItem, EquipmentObservation, EquipmentPlacement, EquipmentResult, InventoryCompletion,
    InventoryItem, ItemResult, ItemUseCompletion,
};
use ac_plugin_abi::magic::{
    CastCompletion, CastGate, CastResult, ComponentInfo, SpellInfo, SpellList, TrackedEnchantment,
};
use ac_plugin_abi::navigation::{
    MovementIntent, NavigationObject, NavigationSnapshot, NavigationStatus,
};
use ac_plugin_abi::peers::{NetworkClient, PeerCast};
use ac_plugin_abi::projectile::{ProjectilePathQuery, ProjectilePathResult};
use ac_plugin_abi::social::{BusyState, FellowMember, FellowshipStatus, SelectionAction};
use ac_plugin_abi::types::ChatLine;
use ac_plugin_abi::ui::ProjectileSample;
use ac_plugin_abi::world::{ItemProperties, Position, WorldObject};
use mosstank_automation::{Automation, SpellCatalog};
use openac_plugin::Host;

/// The client for one call, as every part of the macro sees it: the rules
/// ([`Automation`]), the looter ([`mosstank_loot::host::LootHost`]), the
/// route ([`mosstank_nav::NavigationAutomation`]), the meta
/// ([`mosstank_meta::host::MetaHost`]) and the profile stores
/// ([`mosstank_stores::storage::StoreHost`]) all reach the one SDK host
/// through it.
#[derive(Debug)]
pub struct HostAutomation<'h, 'a> {
    pub(crate) host: &'h mut Host<'a>,
    /// The placements the client announced since the last read
    /// (`Event::EquipmentPlacement`), kept by the plugin between calls.
    observations: Option<&'h mut Vec<EquipmentObservation>>,
    /// Which storage the last storage accessor handed out: the meta's own
    /// (always there) or a profile store's place.
    pub(crate) storage_view: StorageView,
    /// How each kind of UB line is shown, once the UB settings said so.
    pub(crate) ub_displays: Option<[mosstank_meta::host::UbDisplay; 4]>,
}

/// Which storage a [`HostAutomation`] answers as.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StorageView {
    /// The meta's storage: the plugin's own data, always available.
    Meta,
    /// A profile store's storage at a place, available when the client
    /// describes it.
    Store(ac_plugin_abi::storage::StoragePlace),
}

impl<'h, 'a> HostAutomation<'h, 'a> {
    /// The rules' view of `host`.
    pub fn new(host: &'h mut Host<'a>) -> Self {
        Self {
            host,
            observations: None,
            storage_view: StorageView::Meta,
            ub_displays: None,
        }
    }

    /// The same view, reading the equipment placements the plugin kept
    /// from its events.
    pub fn with_equipment_observations(
        host: &'h mut Host<'a>,
        observations: &'h mut Vec<EquipmentObservation>,
    ) -> Self {
        Self {
            host,
            observations: Some(observations),
            storage_view: StorageView::Meta,
            ub_displays: None,
        }
    }

    /// The host, for the other views of the client built on this one.
    pub(crate) fn host(&mut self) -> &mut Host<'a> {
        self.host
    }
}

impl SpellCatalog for HostAutomation<'_, '_> {
    fn known_self_buffs(&mut self) -> Vec<SpellInfo> {
        self.host.known_spells(SpellList::SelfBuffs)
    }

    fn try_get(&mut self, spell_id: u32) -> Option<SpellInfo> {
        self.host.spell(spell_id)
    }

    fn known_attack_spells(&mut self) -> Vec<SpellInfo> {
        self.host.known_spells(SpellList::Attacks)
    }

    fn known_combat_spells(&mut self) -> Vec<SpellInfo> {
        self.host.known_spells(SpellList::Combat)
    }

    fn is_known(&mut self, spell_id: u32) -> bool {
        self.host.is_spell_known(spell_id)
    }

    fn cooldown_remaining(&mut self, cooldown_id: u32) -> f64 {
        self.host.cooldown_remaining(cooldown_id)
    }

    fn all(&mut self) -> Vec<SpellInfo> {
        self.host.all_spells()
    }

    fn try_get_component(&mut self, component_id: u32) -> Option<ComponentInfo> {
        self.host.spell_component(component_id)
    }
}

impl Automation for HostAutomation<'_, '_> {
    /// In the world: the client has a character to describe.
    fn is_available(&mut self) -> bool {
        self.host.character().is_some()
    }

    fn character(&mut self) -> CharacterInfo {
        self.host.character().unwrap_or_default()
    }

    fn skills(&mut self) -> Vec<Skill> {
        self.host.skills()
    }

    fn try_get_skill(&mut self, skill_id: u32) -> Option<Skill> {
        self.host.skill(skill_id)
    }

    fn spells(&mut self) -> &mut dyn SpellCatalog {
        self
    }

    /// The SDK's target 0 is the selection.
    fn evaluate_gate(&mut self, spell_id: u32, target: Option<u32>) -> CastGate {
        self.host.cast_gate(spell_id, target.unwrap_or(0))
    }

    fn cast(&mut self, spell_id: u32, target: Option<u32>) -> bool {
        self.host.cast(spell_id, target.unwrap_or(0)) == CastResult::Sent
    }

    fn has_components(&mut self, spell_id: u32) -> bool {
        self.host.has_components(spell_id)
    }

    fn last_cast_completion(&mut self) -> CastCompletion {
        self.host.last_cast()
    }

    fn combat_mode(&mut self) -> CombatMode {
        self.host.combat().mode
    }

    fn enter_combat_mode(&mut self, mode: CombatMode) -> CombatResult {
        self.host.enter_combat_mode(mode)
    }

    fn items_available(&mut self) -> bool {
        self.host.item_state().is_available
    }

    fn items_busy(&mut self) -> bool {
        self.host.item_state().is_busy
    }

    fn owned_items(&mut self) -> Vec<InventoryItem> {
        self.host.owned_items()
    }

    fn last_item_completion(&mut self) -> ItemUseCompletion {
        self.host.item_state().last_completion
    }

    fn use_item(&mut self, object_id: u32) -> ItemResult {
        self.host.use_item(object_id)
    }

    fn apply_item(&mut self, object_id: u32, target_id: u32) -> ItemResult {
        self.host.apply_item(object_id, target_id)
    }

    fn item_properties(&mut self, object_id: u32) -> Option<ItemProperties> {
        self.host.item_properties(object_id)
    }

    /// The object table is there whenever the character is.
    fn objects_available(&mut self) -> bool {
        self.is_available()
    }

    fn object(&mut self, object_id: u32) -> Option<WorldObject> {
        self.host.object(object_id)
    }

    fn identify(&mut self, object_id: u32) -> ItemResult {
        self.host.identify(object_id)
    }

    fn in_fellowship(&mut self) -> bool {
        self.host.fellowship().is_in_fellowship
    }

    fn fellowship_members(&mut self) -> Vec<FellowMember> {
        self.host.fellowship_members()
    }

    fn request_fellow_vitals(&mut self, requested: bool) -> FellowshipStatus {
        self.host.fellowship_request_vitals(requested)
    }

    fn network_available(&mut self) -> bool {
        self.host.peers_connected()
    }

    fn network_clients(&mut self) -> Vec<NetworkClient> {
        self.host.peer_clients()
    }

    fn position(&mut self) -> Position {
        self.host.navigation().position
    }

    fn chat_messages(&mut self, after_sequence: u64) -> Vec<ChatLine> {
        self.host.chat_captured(after_sequence)
    }

    fn post_system_message(&mut self, text: &str) {
        self.host.chat_post(text);
    }

    fn log_info(&mut self, text: &str) {
        self.host.info(text);
    }

    fn log_warn(&mut self, text: &str) {
        self.host.warn(text);
    }

    fn chat_input_active(&mut self) -> bool {
        self.host.chat_input_active()
    }

    fn is_casting(&mut self) -> bool {
        self.host.is_casting()
    }

    /// The SDK's target 0 is the selection.
    fn request_cast(&mut self, spell_id: u32, target: Option<u32>) -> CastResult {
        self.host.cast(spell_id, target.unwrap_or(0))
    }

    fn combat_snapshot(&mut self) -> CombatSnapshot {
        self.host.combat()
    }

    fn move_item(&mut self, object_id: u32, container_id: u32) -> ItemResult {
        self.host.move_item(object_id, container_id, 0, 0)
    }

    fn split_stack(&mut self, object_id: u32, container_id: u32, amount: u32) -> ItemResult {
        self.host.move_item(object_id, container_id, amount, 0)
    }

    fn inventory_completion(&mut self) -> InventoryCompletion {
        self.host.item_state().last_inventory_completion
    }

    fn capture_objects(&mut self) -> Vec<WorldObject> {
        self.host.objects()
    }

    fn navigation_snapshot(&mut self) -> NavigationSnapshot {
        self.host.navigation()
    }

    fn navigation_object(&mut self, object_id: u32) -> Option<NavigationObject> {
        self.host.navigation_object(object_id)
    }

    fn set_movement_intent(&mut self, intent: MovementIntent) -> NavigationStatus {
        self.host.set_movement_intent(intent)
    }

    fn clear_movement_intent(&mut self) -> NavigationStatus {
        self.host.clear_movement_intent()
    }

    fn face_heading(&mut self, heading_degrees: f32) -> NavigationStatus {
        self.host.face_heading(heading_degrees)
    }

    fn selected_object(&mut self) -> Option<u32> {
        self.host.selection().selected
    }

    fn attributes(&mut self) -> Vec<Attribute> {
        self.host.attributes()
    }

    fn active_enchantments(&mut self) -> Vec<ActiveEnchantment> {
        self.host.enchantments()
    }

    fn timed_enchantments(&mut self) -> Vec<ActiveEnchantment> {
        self.host.timed_enchantments()
    }

    fn hostile_targets(&mut self, maximum_metres: f32) -> Vec<CombatTarget> {
        self.host.hostile_targets(maximum_metres)
    }

    fn begin_attack(&mut self, target_id: u32, height: AttackHeight, power: f32) -> CombatResult {
        self.host.begin_attack(target_id, height, power)
    }

    fn release_attack(&mut self) -> CombatResult {
        self.host.release_attack()
    }

    fn abort_attack(&mut self) -> CombatResult {
        self.host.abort_attack()
    }

    fn dismiss_ghost(&mut self, target_id: u32) -> CombatResult {
        self.host.dismiss_ghost(target_id)
    }

    fn combat_control(&mut self, acquire: bool) -> bool {
        self.host.combat_control(acquire)
    }

    /// Equipping is wired whenever the character is in the world.
    fn equipment_available(&mut self) -> bool {
        self.is_available()
    }

    /// An equip the client started is still in flight (OpenAC's
    /// `IsAutoWieldBusy`).
    fn equipment_busy(&mut self) -> bool {
        self.host.equipment_busy()
    }

    fn owned_equipment(&mut self) -> Vec<EquipmentItem> {
        self.host.owned_equipment()
    }

    fn equipment_placements(&mut self) -> Vec<EquipmentPlacement> {
        self.host.equipment_placements()
    }

    fn take_equipment_observations(&mut self) -> Vec<EquipmentObservation> {
        self.observations
            .as_mut()
            .map(|kept| std::mem::take(&mut **kept))
            .unwrap_or_default()
    }

    /// Slot 0: the client chooses where it goes.
    fn equip(&mut self, object_id: u32) -> EquipmentResult {
        self.host.equip(object_id, 0)
    }

    fn equip_secondary(&mut self, object_id: u32) -> EquipmentResult {
        self.host.equip_secondary(object_id)
    }

    fn busy_state(&mut self) -> BusyState {
        self.host.busy_state()
    }

    fn projectiles_available(&mut self) -> bool {
        self.host.projectiles_available()
    }

    fn evaluate_projectile_path(&mut self, query: &ProjectilePathQuery) -> ProjectilePathResult {
        self.host.evaluate_projectile_path(query)
    }

    fn show_projectile_samples(&mut self, samples: &[ProjectileSample]) {
        self.host.show_projectile_samples(samples);
    }

    fn select(&mut self, object_id: u32) -> bool {
        self.host.select(object_id)
    }

    fn clear_selection(&mut self) -> bool {
        self.host.clear_selection()
    }

    fn selection_step(&mut self, action: SelectionAction) -> bool {
        self.host.selection_step(action)
    }

    fn tracked_enchantments(&mut self, target_id: u32) -> Vec<TrackedEnchantment> {
        self.host.tracked_enchantments(target_id)
    }

    fn report_cast(&mut self, target_id: u32, spell_id: u32, duration_seconds: f64) -> bool {
        self.host.report_cast(target_id, spell_id, duration_seconds)
    }

    fn forget_enchantments(&mut self, target_id: u32) {
        self.host.forget_enchantments(target_id);
    }

    fn announce_cast_attempt(&mut self, target_id: u32, spell_id: u32, skill: i32) -> bool {
        self.host.announce_cast_attempt(target_id, spell_id, skill)
    }

    fn announce_cast_success(
        &mut self,
        target_id: u32,
        spell_id: u32,
        skill: i32,
        duration_seconds: f64,
    ) -> bool {
        self.host
            .announce_cast_success(target_id, spell_id, skill, duration_seconds)
    }

    fn peer_casts(&mut self, after_sequence: i64) -> Vec<PeerCast> {
        self.host.peer_casts(after_sequence)
    }
}

#[cfg(test)]
mod tests {
    use ac_plugin_abi::calls;
    use ac_plugin_abi::magic::CastRequest;
    use openac_plugin::testing::FakeHost;
    use openac_plugin::{Plugin, StartInfo};

    use super::*;

    /// A plugin that casts once at the selection and once at an object.
    #[derive(Debug, Default)]
    struct Caster {
        sent: Vec<bool>,
    }

    impl Plugin for Caster {
        fn enable(&mut self, host: &mut Host<'_>, _info: &StartInfo) -> Result<(), String> {
            let mut automation = HostAutomation::new(host);
            self.sent.push(automation.cast(7, None));
            self.sent.push(automation.cast(7, Some(42)));
            Ok(())
        }
    }

    #[test]
    fn a_cast_at_the_selection_is_target_zero_and_sent_means_true() {
        let mut fake = FakeHost::new("acdream.mosstank");
        fake.set_reply::<calls::MagicCast>(&CastResult::Sent);
        let mut plugin = Caster::default();
        fake.enable(&mut plugin).unwrap();

        assert_eq!(plugin.sent, [true, true]);
        let requests: Vec<CastRequest> = fake.requests::<calls::MagicCast>();
        let targets: Vec<u32> = requests.iter().map(|r| r.target_id).collect();
        assert_eq!(targets, [0, 42]);
    }

    /// A plugin that equips once and reads the placements it kept twice.
    #[derive(Debug, Default)]
    struct Wielder {
        kept: Vec<EquipmentObservation>,
        read: Vec<usize>,
    }

    impl Plugin for Wielder {
        fn enable(&mut self, host: &mut Host<'_>, _info: &StartInfo) -> Result<(), String> {
            let mut automation = HostAutomation::with_equipment_observations(host, &mut self.kept);
            automation.equip(9);
            let first = automation.take_equipment_observations().len();
            let second = automation.take_equipment_observations().len();
            self.read = vec![first, second];
            Ok(())
        }
    }

    #[test]
    fn an_equip_lets_the_client_choose_the_slot_and_kept_placements_are_read_once() {
        let mut fake = FakeHost::new("acdream.mosstank");
        let mut plugin = Wielder {
            kept: vec![EquipmentObservation {
                object_id: 9,
                equipped_location: 0x0010_0000,
                ..EquipmentObservation::default()
            }],
            read: Vec::new(),
        };
        fake.enable(&mut plugin).unwrap();

        let requests = fake.requests::<calls::EquipmentEquip>();
        assert_eq!(requests.len(), 1);
        assert_eq!((requests[0].object_id, requests[0].location), (9, 0));
        assert_eq!(plugin.read, [1, 0]);
        assert!(plugin.kept.is_empty());
    }
}

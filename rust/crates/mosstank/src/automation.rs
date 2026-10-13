//! The rules' view of the client ([`Automation`]) over the SDK's host, for
//! the length of one plugin call.

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::Skill;
use ac_plugin_abi::combat::{CombatMode, CombatResult};
use ac_plugin_abi::items::{InventoryItem, ItemResult, ItemUseCompletion};
use ac_plugin_abi::magic::{
    CastCompletion, CastGate, CastResult, ComponentInfo, SpellInfo, SpellList,
};
use ac_plugin_abi::peers::NetworkClient;
use ac_plugin_abi::social::{FellowMember, FellowshipStatus};
use ac_plugin_abi::types::ChatLine;
use ac_plugin_abi::world::{ItemProperties, Position, WorldObject};
use mosstank_automation::{Automation, SpellCatalog};
use openac_plugin::Host;

/// The client, as the rules see it.
#[derive(Debug)]
pub struct HostAutomation<'h, 'a> {
    host: &'h mut Host<'a>,
}

impl<'h, 'a> HostAutomation<'h, 'a> {
    /// The rules' view of `host`.
    pub fn new(host: &'h mut Host<'a>) -> Self {
        Self { host }
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
}

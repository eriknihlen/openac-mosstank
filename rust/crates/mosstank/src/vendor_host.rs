//! The vendor run's, the fellowship manager's and `/ub fellow`'s view of
//! the client ([`VendorAutomation`], [`FellowshipAutomation`],
//! [`FellowCommandAutomation`]) over the SDK's host.

use ac_plugin_abi::items::{InventoryCompletion, ItemResult};
use ac_plugin_abi::social::{FellowMember, FellowshipState, FellowshipStatus};
use ac_plugin_abi::trade::{VendorResult, VendorState};
use ac_plugin_abi::world::{ItemProperties, WorldObject};
use mosstank_vendor::fellow_command::FellowCommandAutomation;
use mosstank_vendor::ub_chat::UbChatSink;
use mosstank_vendor::{FellowshipAutomation, VendorAutomation};

use crate::automation::HostAutomation;

/// A vendor or fellowship line goes through the UB settings' display for
/// its kind once the shell has said what they are (`UbChat.Post`): hidden
/// when the kind is switched off, in the kind's text class otherwise. Before
/// that, the kind's default class.
impl UbChatSink for HostAutomation<'_, '_> {
    fn post_message(&mut self, text: &str, chat_type: u32) {
        let index = if chat_type == mosstank_vendor::ub_chat::ERROR_CHAT_TYPE {
            3
        } else {
            0
        };
        match self.ub_displays.map(|displays| displays[index]) {
            Some(display) if !display.show => {}
            Some(display) => self.host().chat_post_with(text, display.chat_type),
            None => self.host().chat_post_with(text, chat_type),
        }
    }
}

impl VendorAutomation for HostAutomation<'_, '_> {
    fn vendor_state(&mut self) -> VendorState {
        self.host().vendor()
    }

    fn vendor_item_properties(&mut self, template_id: u32) -> Option<ItemProperties> {
        self.host().vendor_item_properties(template_id)
    }

    fn vendor_add_to_buy_list(&mut self, template_id: u32, count: i32) -> VendorResult {
        self.host().vendor_add_to_buy_list(template_id, count)
    }

    fn vendor_add_to_sell_list(&mut self, item_id: u32) -> VendorResult {
        self.host().vendor_add_to_sell_list(item_id)
    }

    fn vendor_clear_buy_list(&mut self) -> VendorResult {
        self.host().vendor_clear_buy_list()
    }

    fn vendor_clear_sell_list(&mut self) -> VendorResult {
        self.host().vendor_clear_sell_list()
    }

    fn vendor_buy_all(&mut self) -> VendorResult {
        self.host().vendor_buy_all()
    }

    fn vendor_sell_all(&mut self) -> VendorResult {
        self.host().vendor_sell_all()
    }

    fn last_inventory_completion(&mut self) -> InventoryCompletion {
        self.host().item_state().last_inventory_completion
    }

    fn split_item(
        &mut self,
        object_id: u32,
        container_id: u32,
        amount: u32,
        placement: i32,
    ) -> ItemResult {
        self.host()
            .move_item(object_id, container_id, amount, placement)
    }

    fn world_objects(&mut self) -> Vec<WorldObject> {
        self.host().objects()
    }

    fn open_container_id(&mut self) -> u32 {
        self.host().open_container()
    }
}

impl FellowshipAutomation for HostAutomation<'_, '_> {
    fn fellowship_state(&mut self) -> FellowshipState {
        self.host().fellowship()
    }

    fn fellowship_roster(&mut self) -> Vec<FellowMember> {
        self.host().fellowship_roster()
    }

    fn fellowship_recruit(&mut self, player_id: u32) -> FellowshipStatus {
        self.host().fellowship_recruit(player_id)
    }

    fn fellowship_dismiss(&mut self, player_id: u32) -> FellowshipStatus {
        self.host().fellowship_dismiss(player_id)
    }

    fn fellowship_assign_leader(&mut self, player_id: u32) -> FellowshipStatus {
        self.host().fellowship_assign_leader(player_id)
    }

    fn fellowship_set_open(&mut self, open: bool) -> FellowshipStatus {
        self.host().fellowship_set_open(open)
    }
}

impl FellowCommandAutomation for HostAutomation<'_, '_> {
    fn fellowship_create(&mut self, name: &str, share_experience: bool) -> FellowshipStatus {
        self.host().fellowship_create(name, share_experience)
    }

    fn fellowship_quit(&mut self, disband: bool) -> FellowshipStatus {
        self.host().fellowship_quit(disband)
    }

    fn fellowship_shares_experience(&mut self) -> bool {
        self.host().fellowship().shares_experience
    }

    fn fellowship_splits_experience_evenly(&mut self) -> bool {
        self.host().fellowship().splits_experience_evenly
    }

    fn fellow_level(&mut self, member: &FellowMember) -> u32 {
        member.level
    }

    fn world_objects(&mut self) -> Vec<WorldObject> {
        self.host().objects()
    }
}

#[cfg(test)]
mod tests {
    use ac_plugin_abi::calls;
    use ac_plugin_abi::types::ChatPost;
    use mosstank_vendor::ub_chat;
    use openac_plugin::testing::FakeHost;
    use openac_plugin::{Host, Plugin, StartInfo};

    use super::*;

    /// A plugin that prints one error line the UB way.
    #[derive(Debug, Default)]
    struct Says;

    impl Plugin for Says {
        fn enable(&mut self, host: &mut Host<'_>, _info: &StartInfo) -> Result<(), String> {
            let mut automation = HostAutomation::new(host);
            ub_chat::post(&mut automation, &ub_chat::error("addbuy: No vendor open"));
            Ok(())
        }
    }

    #[test]
    fn a_ub_error_line_is_posted_in_the_help_class() {
        let mut fake = FakeHost::new("acdream.mosstank");
        let mut plugin = Says;
        fake.enable(&mut plugin).unwrap();

        let posted: Vec<ChatPost> = fake.requests::<calls::ChatPost>();
        assert_eq!(posted.len(), 1);
        assert_eq!(posted[0].text, "[UB] Error: addbuy: No vendor open");
        assert_eq!(posted[0].log_text_type, ub_chat::ERROR_CHAT_TYPE);
    }
}

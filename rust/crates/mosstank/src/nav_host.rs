//! The route's view of the client ([`NavigationAutomation`]) over the SDK's
//! host: the same [`HostAutomation`] the other rules use, with the
//! navigation, chat-bar, vendor-window and cast-request calls a route needs.

use ac_plugin_abi::magic::{CastResult, SpellInfo, SpellList};
use ac_plugin_abi::navigation::{
    GoToReport, MoveChannel, MoveDirection, MovePace, MoveUnit, MovementIntent, NavigationObject,
    NavigationSnapshot, NavigationStatus,
};
use ac_plugin_abi::world::Position;
use mosstank_nav::NavigationAutomation;

use crate::automation::HostAutomation;

impl NavigationAutomation for HostAutomation<'_, '_> {
    fn navigation(&mut self) -> NavigationSnapshot {
        self.host().navigation()
    }

    fn navigation_object(&mut self, object_id: u32) -> Option<NavigationObject> {
        self.host().navigation_object(object_id)
    }

    fn find_object(
        &mut self,
        name: &str,
        near: Position,
        maximum_distance_meters: f64,
    ) -> Option<NavigationObject> {
        self.host()
            .navigation_find(name, near, maximum_distance_meters)
    }

    fn navigation_objects(&mut self) -> Vec<NavigationObject> {
        self.host().navigation_objects()
    }

    fn set_movement_intent(&mut self, intent: MovementIntent) -> NavigationStatus {
        self.host().set_movement_intent(intent)
    }

    fn clear_movement_intent(&mut self) -> NavigationStatus {
        self.host().clear_movement_intent()
    }

    fn face_heading(&mut self, heading_degrees: f32) -> NavigationStatus {
        self.host().face_heading(heading_degrees)
    }

    fn move_by(
        &mut self,
        direction: MoveDirection,
        pace: MovePace,
        amount: f32,
        unit: MoveUnit,
    ) -> NavigationStatus {
        self.host().move_by(direction, pace, amount, unit)
    }

    fn stop_moving_channel(&mut self, channel: MoveChannel) -> NavigationStatus {
        self.host().stop_moving_channel(channel)
    }

    fn jump(&mut self, power: f32) -> NavigationStatus {
        self.host().jump(power)
    }

    fn go_to_place(&mut self, position: Position, arrival_meters: f32) -> NavigationStatus {
        self.host().go_to_place(position, arrival_meters)
    }

    fn stop_go_to(&mut self) -> NavigationStatus {
        self.host().stop_go_to()
    }

    fn go_to_report(&mut self) -> GoToReport {
        self.host().go_to_report()
    }

    fn chat_input_active(&mut self) -> bool {
        self.host().chat_input_active()
    }

    /// The SDK takes every line it is handed; it runs once the plugin's
    /// call returns.
    fn submit_chat(&mut self, text: &str) -> bool {
        self.host().chat_submit(text);
        true
    }

    fn active_vendor_id(&mut self) -> u32 {
        self.host().item_state().active_vendor_id
    }

    fn open_vendor_id(&mut self) -> u32 {
        let vendor = self.host().vendor();
        if vendor.is_open { vendor.vendor_id } else { 0 }
    }

    fn appraisal_awaiting_id(&mut self) -> u32 {
        self.host().loot_state().appraisal.awaiting_id
    }

    fn is_casting(&mut self) -> bool {
        self.host().is_casting()
    }

    /// At the selection: the SDK's target 0.
    fn request_cast(&mut self, spell_id: u32) -> CastResult {
        self.host().cast(spell_id, 0)
    }

    fn known_spell_list(&mut self, list: SpellList) -> Vec<SpellInfo> {
        self.host().known_spells(list)
    }
}

#[cfg(test)]
mod tests {
    use ac_plugin_abi::calls;
    use ac_plugin_abi::magic::CastRequest;
    use ac_plugin_abi::trade::VendorState;
    use openac_plugin::testing::FakeHost;
    use openac_plugin::{Host, Plugin, StartInfo};

    use super::*;

    /// A plugin that asks the route's questions once.
    #[derive(Debug, Default)]
    struct Route {
        cast: Option<CastResult>,
        open_vendor: u32,
        submitted: bool,
    }

    impl Plugin for Route {
        fn enable(&mut self, host: &mut Host<'_>, _info: &StartInfo) -> Result<(), String> {
            let mut automation = HostAutomation::new(host);
            self.cast = Some(automation.request_cast(1635));
            self.open_vendor = automation.open_vendor_id();
            self.submitted = automation.submit_chat("/marketplace");
            Ok(())
        }
    }

    #[test]
    fn a_recall_is_cast_at_the_selection_and_a_closed_vendor_is_none() {
        let mut fake = FakeHost::new("acdream.mosstank");
        fake.set_reply::<calls::MagicCast>(&CastResult::MissingComponents);
        fake.set_reply::<calls::VendorState>(&VendorState {
            is_open: false,
            vendor_id: 404,
            ..VendorState::default()
        });
        let mut plugin = Route::default();
        fake.enable(&mut plugin).unwrap();

        assert_eq!(plugin.cast, Some(CastResult::MissingComponents));
        let requests: Vec<CastRequest> = fake.requests::<calls::MagicCast>();
        assert_eq!(requests.len(), 1);
        assert_eq!((requests[0].spell_id, requests[0].target_id), (1635, 0));
        assert_eq!(plugin.open_vendor, 0);
        assert!(plugin.submitted);
        assert_eq!(fake.requests::<calls::ChatSubmit>(), ["/marketplace"]);
    }
}

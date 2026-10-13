//! The route's view of the client ([`NavigationAutomation`]) over the SDK's
//! host: the same [`HostAutomation`] the other rules use, with the
//! navigation, chat-bar, vendor-window and cast-request calls a route needs.

use ac_plugin_abi::magic::{SpellInfo, SpellList};
use ac_plugin_abi::navigation::{
    GoToReport, MoveChannel, MoveDirection, MovePace, MoveUnit, NavigationObject, NavigationStatus,
};
use ac_plugin_abi::world::Position;
use mosstank_nav::NavigationAutomation;
use openac_plugin::{Host, WalkPause};

use crate::automation::HostAutomation;

/// The plugin's hold on the walks the client plans (the C#'s
/// `PauseGoToWhile`): taken when the plugin starts, told on every frame
/// what the macro needs the character for
/// ([`mosstank_nav::route_view::walk_pause_reason_for`]), and let go when it
/// stops. While the reason is not empty, a walk the client plans waits.
#[derive(Debug, Default)]
pub struct GoToPause {
    pause: Option<WalkPause>,
    reason: String,
}

impl GoToPause {
    /// Takes the pause, needing nothing yet; holds nothing when the client
    /// refuses it.
    pub fn take(host: &mut Host<'_>) -> Self {
        Self {
            pause: host.navigation_pause(""),
            reason: String::new(),
        }
    }

    /// Says what the macro needs the character for now (`None` for
    /// nothing); the client is told only when that changes. Whether the
    /// client was told.
    pub fn need(&mut self, host: &mut Host<'_>, reason: Option<&str>) -> bool {
        let reason = reason.unwrap_or("");
        if reason == self.reason {
            return false;
        }
        let called = if let Some(pause) = self.pause {
            let _ = host.navigation_set_pause(pause, reason);
            true
        } else {
            false
        };
        reason.clone_into(&mut self.reason);
        called
    }

    /// Lets the pause go.
    pub fn release(&mut self, host: &mut Host<'_>) {
        if let Some(pause) = self.pause.take() {
            let _ = host.navigation_release_pause(pause);
        }
        self.reason.clear();
    }
}

impl NavigationAutomation for HostAutomation<'_, '_> {
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
        self.reader().navigation_objects()
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

    /// The SDK takes every line it is handed; it runs once the plugin's
    /// call returns.
    fn submit_chat(&mut self, text: &str) -> bool {
        self.host().chat_submit(text);
        true
    }

    fn active_vendor_id(&mut self) -> u32 {
        self.reader().item_state().active_vendor_id
    }

    fn open_vendor_id(&mut self) -> u32 {
        let vendor = self.reader().vendor();
        if vendor.is_open { vendor.vendor_id } else { 0 }
    }

    fn appraisal_awaiting_id(&mut self) -> u32 {
        self.host().loot_state().appraisal.awaiting_id
    }

    fn known_spell_list(&mut self, list: SpellList) -> Vec<SpellInfo> {
        self.known_list(list)
    }
}

#[cfg(test)]
mod tests {
    use ac_plugin_abi::calls;
    use ac_plugin_abi::magic::{CastRequest, CastResult};
    use ac_plugin_abi::trade::VendorState;
    use mosstank_automation::Automation;
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
            self.cast = Some(automation.request_cast(1635, None));
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

    /// A plugin that takes the walk pause, needs it twice for the same
    /// thing, then for nothing, and lets it go.
    #[derive(Debug, Default)]
    struct Pauses;

    impl Plugin for Pauses {
        fn enable(&mut self, host: &mut Host<'_>, _info: &StartInfo) -> Result<(), String> {
            let mut pause = GoToPause::take(host);
            pause.need(host, Some("MossTank is buffing"));
            pause.need(host, Some("MossTank is buffing"));
            pause.need(host, None);
            pause.release(host);
            Ok(())
        }
    }

    #[test]
    fn the_walk_pause_is_told_only_when_its_reason_changes() {
        let mut fake = FakeHost::new("acdream.mosstank");
        fake.set_reply::<calls::NavigationPause>(&7);
        let mut plugin = Pauses;
        fake.enable(&mut plugin).unwrap();

        assert_eq!(fake.requests::<calls::NavigationPause>(), [String::new()]);
        let changes: Vec<(u32, String)> = fake
            .requests::<calls::NavigationPauseSet>()
            .into_iter()
            .map(|change| (change.handle, change.reason))
            .collect();
        assert_eq!(
            changes,
            [(7, "MossTank is buffing".to_owned()), (7, String::new())]
        );
        assert_eq!(fake.requests::<calls::NavigationPauseRelease>(), [7]);
    }
}

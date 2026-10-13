//! The client as the macro shell reaches it during one call
//! ([`TankClient`]): every view the macro's parts take, over one borrow of
//! the SDK host ([`crate::automation::HostAutomation`] implements them all).

use mosstank_loot::host::LootHost;
use mosstank_meta::host::MetaHost;
use mosstank_stores::storage::StoreHost;
use mosstank_ub::host::UbHost;
use mosstank_vendor::FellowshipAutomation;
use mosstank_vendor::fellow_command::FellowCommandAutomation;

/// The client for one call: the rules' view ([`mosstank_automation::Automation`],
/// through the looter's and the route's), the looter's, the route's and the
/// fellowship's, the meta's and the UB tab's, and the profile stores'.
pub trait TankClient:
    LootHost + FellowshipAutomation + FellowCommandAutomation + UbHost + StoreHost + SessionClient
{
    /// The client as the rules see it.
    fn rules(&mut self) -> &mut dyn mosstank_automation::Automation;
    /// The client as the looter sees it.
    fn looter(&mut self) -> &mut dyn LootHost;
    /// The client as the route sees it.
    fn route(&mut self) -> &mut dyn mosstank_nav::NavigationAutomation;
    /// The client as the fellowship manager sees it.
    fn fellows(&mut self) -> &mut dyn FellowshipAutomation;
    /// The client as `/ub fellow` sees it.
    fn fellow_commands(&mut self) -> &mut dyn FellowCommandAutomation;
    /// The client as the meta sees it.
    fn meta(&mut self) -> &mut dyn MetaHost;
    /// The client as the UB tab sees it.
    fn ub(&mut self) -> &mut dyn UbHost;
    /// The client as the profile stores see it.
    fn stores(&mut self) -> &mut dyn StoreHost;
}

impl<T> TankClient for T
where
    T: LootHost
        + FellowshipAutomation
        + FellowCommandAutomation
        + UbHost
        + StoreHost
        + SessionClient,
{
    fn rules(&mut self) -> &mut dyn mosstank_automation::Automation {
        self
    }

    fn looter(&mut self) -> &mut dyn LootHost {
        self
    }

    fn route(&mut self) -> &mut dyn mosstank_nav::NavigationAutomation {
        self
    }

    fn fellows(&mut self) -> &mut dyn FellowshipAutomation {
        self
    }

    fn fellow_commands(&mut self) -> &mut dyn FellowCommandAutomation {
        self
    }

    fn meta(&mut self) -> &mut dyn MetaHost {
        self
    }

    fn ub(&mut self) -> &mut dyn UbHost {
        self
    }

    fn stores(&mut self) -> &mut dyn StoreHost {
        self
    }
}

/// What the shell reads of the session itself: the client's clocks and the
/// settings the session was started with.
pub trait SessionClient {
    /// The client's clocks and a fresh random seed.
    fn clock(&mut self) -> ac_plugin_abi::host::Clock;
    /// The plugin's session settings (autostart reads them).
    fn session_settings(&mut self) -> Vec<(String, String)>;
    /// How each kind of UB line is shown, from the UB settings (the four
    /// kinds in `UbLineKind` order).
    fn set_ub_displays(&mut self, displays: [mosstank_meta::host::UbDisplay; 4]);
    /// Shows or hides one of the plugin's windows; whether the client took
    /// it (`IUiRegistry.ShowPanel` and `HidePanel`).
    fn show_window(&mut self, window_id: &str, visible: bool) -> bool;
    /// The loot classifiers the client's plugins offer
    /// (`LootClassifiers.Available`).
    fn loot_classifiers(&mut self) -> Vec<ac_plugin_abi::items::ClassifierInfo>;
}

impl SessionClient for crate::automation::HostAutomation<'_, '_> {
    fn clock(&mut self) -> ac_plugin_abi::host::Clock {
        self.host.clock()
    }

    fn session_settings(&mut self) -> Vec<(String, String)> {
        self.host.settings()
    }

    fn set_ub_displays(&mut self, displays: [mosstank_meta::host::UbDisplay; 4]) {
        self.ub_displays = Some(displays);
    }

    fn show_window(&mut self, window_id: &str, visible: bool) -> bool {
        self.host.show_panel(window_id, visible)
    }

    fn loot_classifiers(&mut self) -> Vec<ac_plugin_abi::items::ClassifierInfo> {
        self.host.loot_classifiers()
    }
}

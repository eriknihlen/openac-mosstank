//! The vendor run as the panel binds it (`MossTankPanel.UbSettings`'s
//! `_vendorTrade` wiring): its switches read live from the UB catalogue,
//! its profiles looked for the tiered way in the shared profile folder, the
//! one stack-and-cram owner for its opening pass, the route owning vendor
//! opening while the macro follows it, and the macro's own items never
//! sold.

use std::collections::HashSet;

use mosstank_settings::ub::UbSettingCatalog;
use mosstank_stores::storage::{Root, StorageError};
use mosstank_ub::catalog::UbCatalog;
use mosstank_vendor::{
    VendorAutomation, VendorContext, VendorTradeController, VendorTradeSettings,
};

use super::Core;
use super::ub::StoreValues;
use crate::client::TankClient;

/// The plugin's side of one call into the vendor run.
pub(crate) struct ShellVendor<'a> {
    pub(crate) client: &'a mut dyn TankClient,
    pub(crate) core: &'a mut Core,
    pub(crate) catalog: &'a UbSettingCatalog<StoreValues>,
}

impl ShellVendor<'_> {
    fn switch(&self, name: &str) -> bool {
        UbCatalog::value(self.catalog, name).boolean
    }

    fn number(&self, name: &str) -> i32 {
        UbCatalog::value(self.catalog, name).as_int32()
    }
}

impl VendorContext for ShellVendor<'_> {
    fn automation(&mut self) -> &mut dyn VendorAutomation {
        self.client.vendor()
    }

    /// The rows as they stand, so a switch flipped mid-visit is honoured on
    /// the next round.
    fn settings(&mut self) -> VendorTradeSettings {
        VendorTradeSettings {
            enabled: self.switch("AutoVendor.Enabled"),
            enable_buying: self.switch("AutoVendor.EnableBuying"),
            enable_selling: self.switch("AutoVendor.EnableSelling"),
            test_mode: self.switch("AutoVendor.TestMode"),
            think: self.switch("AutoVendor.Think"),
            show_merchant_info: self.switch("AutoVendor.ShowMerchantInfo"),
            only_from_main_pack: self.switch("AutoVendor.OnlyFromMainPack"),
            tries: self.number("AutoVendor.Tries"),
            tries_time_milliseconds: self.number("AutoVendor.TriesTime"),
        }
    }

    fn resolve_profile_key(
        &mut self,
        folder: &str,
        file_name: &str,
    ) -> Result<String, StorageError> {
        let store = self.catalog.values().store();
        let storage = self.client.stores().storage(Root::Profiles);
        store
            .borrow()
            .resolve_profile_key(storage, folder, file_name)
    }

    fn read_profile(&mut self, key: &str) -> Result<Option<String>, StorageError> {
        let storage = self.client.stores().storage(Root::Profiles);
        if !storage.is_available() {
            return Ok(None);
        }
        storage.read_text(key)
    }

    /// The one stack-and-cram owner does the run's opening pass: the
    /// scheduler is suspended while the run owns the character, so the two
    /// never drive it at once.
    fn stack_cram_pass(&mut self, elapsed_seconds: f64, first: bool) -> bool {
        let core = &mut *self.core;
        if first {
            core.inventory_maintenance.reset();
        }
        core.inventory_maintenance.tick(
            self.client.ub(),
            &core.settings.inventory,
            elapsed_seconds,
            true,
        )
    }

    /// While the macro follows its route, the route's open-vendor waypoint
    /// is the one thing that opens vendors.
    fn route_owns_vendor_open(&mut self) -> bool {
        self.core.scheduler_running && self.core.settings.navigation.enabled
    }

    fn protected_items(&mut self) -> HashSet<u32> {
        self.core
            .settings
            .combat
            .combat_item_object_ids
            .iter()
            .copied()
            .collect()
    }
}

/// Runs `f` with the vendor run and its side of the call. The controller is
/// lent out of the shell's parts for the length of `f`, which hands the rest
/// of them to the run.
pub(crate) fn with_vendor<R>(
    client: &mut dyn TankClient,
    core: &mut Core,
    catalog: &UbSettingCatalog<StoreValues>,
    f: impl FnOnce(&mut VendorTradeController, &mut ShellVendor<'_>) -> R,
) -> R {
    let mut controller = std::mem::take(&mut core.vendor_trade);
    let result = {
        let mut cx = ShellVendor {
            client,
            core: &mut *core,
            catalog,
        };
        f(&mut controller, &mut cx)
    };
    core.vendor_trade = controller;
    result
}

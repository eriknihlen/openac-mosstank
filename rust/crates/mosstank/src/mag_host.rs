//! The Mag tab's view of the client ([`MagHost`]): the calls only the Mag
//! features make, each one SDK call.

use ac_plugin_abi::character::ActiveEnchantment;
use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::ItemProperties;
use mosstank_mag::host::MagHost;

use crate::automation::HostAutomation;

impl MagHost for HostAutomation<'_, '_> {
    fn owned_inventory_complete(&mut self) -> bool {
        self.reader().owned_inventory_complete()
    }

    fn item_properties(&mut self, object_id: u32) -> Option<ItemProperties> {
        self.reader().item_properties(object_id)
    }

    fn all_enchantments(&mut self) -> Vec<ActiveEnchantment> {
        self.reader().all_enchantments()
    }

    fn loot_contents(&mut self) -> Vec<InventoryItem> {
        self.reader().loot_contents()
    }

    #[allow(clippy::cast_precision_loss)]
    fn now_ms(&mut self) -> f64 {
        self.service().clock().monotonic_ms as f64
    }

    fn utc_unix_ms(&mut self) -> i64 {
        self.service().clock().utc_unix_ms
    }
}

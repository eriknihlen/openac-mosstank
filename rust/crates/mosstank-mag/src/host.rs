//! The client as the Mag features reach it: everything the meta engine
//! reaches ([`MetaHost`]: the character, the objects, appraisals, the
//! selection, spells, chat, storage, the owned items and the open
//! container) and the few calls only the Mag features make. The loot
//! profile comes in through [`LootVerdict`].

use ac_plugin_abi::character::ActiveEnchantment;
use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::ItemProperties;
use mosstank_meta::host::MetaHost;

/// The client calls the Mag features make beyond the meta engine's.
pub trait MagHost: MetaHost {
    /// The owned list is complete (do not treat a missing item as gone
    /// before).
    fn owned_inventory_complete(&mut self) -> bool;
    /// An owned item's properties by game property id.
    fn item_properties(&mut self, object_id: u32) -> Option<ItemProperties>;
    /// Every enchantment on the character, surpassed ones too (API 1.10).
    fn all_enchantments(&mut self) -> Vec<ActiveEnchantment>;
    /// What the open outside container holds.
    fn loot_contents(&mut self) -> Vec<InventoryItem>;
    /// Monotonic milliseconds for this call.
    fn now_ms(&mut self) -> f64;
}

/// A loot profile's verdict on one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// A rule keeps it (`+`), or not (`-`).
    pub keep: bool,
    /// The deciding rule's name; empty when no rule matched.
    pub rule_name: String,
    /// The deciding rule salvages.
    pub salvage: bool,
}

/// MossTank's loaded loot profile, as the item-info line asks it.
pub trait LootVerdict {
    /// `None` when no profile is loaded. `needs_identify` items answer
    /// `None` until their appraisal is in.
    fn verdict(&mut self, item: &InventoryItem, properties: &ItemProperties) -> Option<Verdict>;
    /// The profile needs an appraisal of this item before it can decide.
    fn needs_identify(&mut self, item: &InventoryItem, properties: &ItemProperties) -> bool;
}

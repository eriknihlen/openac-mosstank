//! MossTank's Mag tab and Inventory window: the features ported from
//! Mag-Tools by Mag-nus (used with the author's permission; see
//! `rust/NOTICE.md`), as host-free state the plugin drives.
//!
//! - [`combat`]: the combat tracker: the chat's attacks, kills and
//!   surges, the current session's and the persistent stats, their files.
//! - [`fmt`]: .NET Framework number formatting (en-US).
//! - [`host`]: the client as the features reach it ([`host::MagHost`]) and
//!   the loot profile's verdict on an item ([`host::LootVerdict`]).
//! - [`identify`]: the paced identify queue the features share.
//! - [`inv_items`]: the Inv. Items page: consumables, their use rates and
//!   the profit block.
//! - [`inventory_log`]: the inventory logger, one JSON file per
//!   character.
//! - [`inventory_search`]: the Inventory window: the search across every
//!   character the logger saved.
//! - [`item_info`]: the one-line description of an identified item.
//! - [`mana`]: the mana list: the worn items, their mana and time left.
//! - [`printer`]: the identify chat line with the loot verdict, and its
//!   two triggers.
//! - [`settings`]: the Mag settings' names in the UB settings catalogue.
//! - `testing` (native builds only): a client and a loot profile in memory
//!   for the tests ([`testing::FakeMagHost`], [`testing::FakeVerdict`]).
//!
//! The client comes in through [`host::MagHost`] (the meta engine's
//! [`mosstank_meta::host::MetaHost`] and the calls only the Mag features
//! make).

pub mod combat;
pub mod fmt;
pub mod host;
pub mod identify;
pub mod inv_items;
pub mod inventory_filter;
pub mod inventory_log;
pub mod inventory_search;
pub mod item_info;
pub mod mana;
pub mod printer;
pub mod settings;
#[cfg(not(target_arch = "wasm32"))]
pub mod testing;

/// The Mag tab's pages, one sub-tab each (Mag-Tools' tracker pages).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MagSubPage {
    /// The combat tracker, the first page.
    #[default]
    Combat,
    /// The mana list.
    Mana,
    /// The Inv. Items tracker.
    InvItems,
}

/// Everything the Mag tab and the Inventory window keep between calls.
#[derive(Debug, Default)]
pub struct MagParts {
    /// The Mag tab's page shown.
    pub sub_page: MagSubPage,
    /// The Combat page's tab shown.
    pub combat_page: combat::gui::CombatPage,
    /// The Inventory window.
    pub inventory_search: inventory_search::InventorySearch,
    /// The Inventory filters window.
    pub filter_window: inventory_filter::window::FilterWindow,
    /// The appraisals the features asked for.
    pub identify: identify::IdentifyQueue,
    /// The identify chat line's detectors.
    pub printer: printer::Printer,
    /// The inventory logger.
    pub inventory_log: inventory_log::InventoryLogger,
    /// The mana list.
    pub mana: mana::ManaTracker,
    /// The mana overlay window.
    pub mana_overlay: mana::ManaOverlay,
    /// The Inv. Items page.
    pub inv_items: inv_items::InvItemsTracker,
    /// The combat tracker.
    pub combat: combat::Combat,
}

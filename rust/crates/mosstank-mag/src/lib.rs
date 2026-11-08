//! MossTank's Mag tab and Inventory window: the features ported from
//! Mag-Tools by Mag-nus (used with the author's permission; see
//! `rust/NOTICE.md`), as host-free state the plugin drives.
//!
//! - [`fmt`]: .NET Framework number formatting (en-US).
//! - [`host`]: the client as the features reach it ([`host::MagHost`]) and
//!   the loot profile's verdict on an item ([`host::LootVerdict`]).
//! - [`identify`]: the paced identify queue the features share.
//! - [`inventory_log`]: the inventory logger, one JSON file per
//!   character.
//! - [`item_info`]: the one-line description of an identified item.
//! - [`printer`]: the identify chat line with the loot verdict, and its
//!   two triggers.
//! - [`settings`]: the Mag settings' names in the UB settings catalogue.
//! - `testing` (native builds only): a client and a loot profile in memory
//!   for the tests ([`testing::FakeMagHost`], [`testing::FakeVerdict`]).
//!
//! The client comes in through [`host::MagHost`] (the meta engine's
//! [`mosstank_meta::host::MetaHost`] and the calls only the Mag features
//! make).

pub mod fmt;
pub mod host;
pub mod identify;
pub mod inventory_log;
pub mod item_info;
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
    /// The Inventory window's search pattern, as typed.
    pub inventory_search: String,
    /// The appraisals the features asked for.
    pub identify: identify::IdentifyQueue,
    /// The identify chat line's detectors.
    pub printer: printer::Printer,
    /// The inventory logger.
    pub inventory_log: inventory_log::InventoryLogger,
}

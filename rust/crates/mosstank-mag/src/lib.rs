//! MossTank's Mag tab and Inventory window: the features ported from
//! Mag-Tools by Mag-nus (used with the author's permission; see
//! `rust/NOTICE.md`), as host-free state the plugin drives.
//!
//! - [`fmt`]: .NET Framework number formatting (en-US).
//! - [`host`]: the client as the features reach it ([`host::MagHost`]) and
//!   the loot profile's verdict on an item ([`host::LootVerdict`]).
//!
//! The client comes in through [`host::MagHost`] (the meta engine's
//! [`mosstank_meta::host::MetaHost`] and the calls only the Mag features
//! make).

pub mod fmt;
pub mod host;
#[cfg(not(target_arch = "wasm32"))]
pub mod testing;

/// Everything the Mag tab and the Inventory window keep between calls.
#[derive(Debug, Default)]
pub struct MagParts {}

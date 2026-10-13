//! MossTank's stores and storage layout.
//!
//! - [`storage`]: the storage a store is handed ([`storage::PluginStorage`]),
//!   the log it reports to, and an in-memory storage.
//! - [`directory`]: the shared VTank profile folder: folders, a character's
//!   file names, the binding file, the pickers' lists.
//! - [`layout`]: the folder tree and the preparer that makes it.
//! - [`layout_migration`]: the first-run copy from the old flat layout.
//! - [`ub_tree_migration`]: the one-time copy of the UB tree into the shared
//!   folder.
//! - [`ub_store`]: the UB settings files, profile files and alias lists.
//! - [`recovery`]: a profile that would not load is kept.
//! - [`global_variables`]: the global variables every client on a server
//!   shares, one file each, under a lock.
//! - [`profile_store`]: the settings profile store, with its side-car
//!   ([`sidecar`]) and the older JSON documents it migrates ([`legacy`]).
//! - [`loot_store`]: the loot profile store.
//! - [`route_store`]: the route store (`.af` and dropped `.nav` routes).
//! - [`stj`]: the JSON documents read and written as System.Text.Json does.
//!
//! Pure: storage comes in through [`storage::PluginStorage`] (a profile store
//! takes a [`storage::StoreHost`]), handed to each call that needs it.

pub mod directory;
pub mod global_variables;
pub mod layout;
pub mod layout_migration;
pub mod legacy;
pub mod loot_store;
pub mod profile_store;
pub mod recovery;
pub mod route_store;
pub mod sidecar;
pub mod stj;
pub mod storage;
pub mod ub_store;
pub mod ub_tree_migration;

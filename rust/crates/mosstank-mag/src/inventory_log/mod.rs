//! The inventory logger: a port of Mag-Tools' `InventoryLogger`, saving
//! each character's inventory to its own file in our own JSON (owner,
//! 2026-11-08), which the Inventory window searches.
//!
//! - [`record`]: one item as the log keeps it, and the merge with the
//!   last save (`Combine`).
//! - [`file`]: the file, its key, and reading it a few items at a time.

pub mod file;
pub mod record;

pub use file::{LogHeader, LogReader, ReadError, VERSION, read, storage_key, write};
pub use record::{ItemRecord, combine, merge, same_item};

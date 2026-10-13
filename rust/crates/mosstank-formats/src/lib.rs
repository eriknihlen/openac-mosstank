//! VTank and MossTank profile formats, read and written byte for byte as
//! the C# plugin does.
//!
//! - [`usd`]: VTank's settings database text (`.usd`, `.ast`, game tables).
//! - [`utl`]: VTClassic loot profiles.
//! - [`line_endings`]: keeping a file's own line breaks on save.
//!
//! Pure: no host calls. Text comes in and goes out as `&str`/`String`;
//! reading and writing files is the caller's (the plugin's storage).

pub mod line_endings;
pub mod usd;
pub mod utl;

#[cfg(not(target_arch = "wasm32"))]
pub mod fixtures;

/// A profile file that does not parse (the C# `FormatException`), with the
/// C# message.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct FormatError(pub String);

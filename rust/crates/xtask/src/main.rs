//! Repository chores, run from `rust/`:
//!
//! - `cargo run -p xtask -- spell-ids`: regenerate
//!   `crates/mosstank-data/src/spell_id.rs` from the C# `SpellId.g.cs`.
//! - `cargo run -p xtask -- census`: rewrite `tests/census.md`, every C#
//!   test with its Rust counterpart, "n/a" or "not yet".
//! - `cargo run -p xtask -- wasm-imports`: check that the built plugin
//!   imports from the client's `openac` module only (run by
//!   `scripts/check.sh` after the WebAssembly build).
//!
//! The tests of this crate fail when either file is out of date.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod census;
mod spell_ids;
mod wasm_imports;

/// The `rust/` folder.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The repository root (the C# lives here).
fn repository_root() -> PathBuf {
    workspace_root().join("..")
}

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    let result = match task.as_str() {
        "spell-ids" => spell_ids::write(),
        "census" => census::write(),
        "wasm-imports" => wasm_imports::check_built_module_imports_only_the_client(),
        _ => Err("usage: cargo run -p xtask -- <spell-ids|census|wasm-imports>".to_owned()),
    };
    match result {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

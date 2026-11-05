//! Repository chores, run from `rust/`:
//!
//! - `cargo run -p xtask -- wasm-imports`: check that the built plugin
//!   imports from the client's `openac` module only (run by
//!   `scripts/check.sh` after the WebAssembly build).
//!
//! The C#-to-Rust test census (`tests/census.md`) and the spell table
//! (`crates/mosstank-data/src/spell_id.rs`) were generated here from the C#
//! plugin while it sat beside this port; that plugin is on the `deprecated`
//! branch now, and both files are kept as they were last generated.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod wasm_imports;

/// The `rust/` folder.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    let result = match task.as_str() {
        "wasm-imports" => wasm_imports::check_built_module_imports_only_the_client(),
        _ => Err("usage: cargo run -p xtask -- wasm-imports".to_owned()),
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

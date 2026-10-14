#!/usr/bin/env sh
# Format, lint, test, build for WebAssembly, and hold every call of the built
# plugin to half its instruction threshold (no call may cause lag). Run
# before every commit.
set -eu
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --target wasm32-unknown-unknown --release
cargo run -q -p xtask -- wasm-imports
# The performance gate: every measured call under half its threshold
# (crates/mosstank-fuel; MOSSTANK_FUEL_TRACE=1 profiles the worst calls).
cargo test -q -p mosstank-fuel --test budgets -- --ignored
echo "plugin: target/wasm32-unknown-unknown/release/mosstank.wasm (manifest: plugin.json)"

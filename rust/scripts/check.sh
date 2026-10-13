#!/usr/bin/env sh
# Format, lint, test, and build for WebAssembly. Run before every commit.
set -eu
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --target wasm32-unknown-unknown --release

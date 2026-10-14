# Format, lint, test, build for WebAssembly, and hold every call of the built
# plugin to half of the host's fuel budget. Run before every commit.
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
cargo fmt --all --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo clippy --workspace --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo test --workspace
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo build --workspace --target wasm32-unknown-unknown --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo run -q -p xtask -- wasm-imports
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
# The fuel gate: every measured call under half its budget
# (crates/mosstank-fuel; MOSSTANK_FUEL_TRACE=1 profiles the worst calls).
cargo test -q -p mosstank-fuel --test budgets -- --ignored
exit $LASTEXITCODE

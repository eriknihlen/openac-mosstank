# MossTank for openac-rust

This folder is MossTank built as a WebAssembly plugin for
[openac-rust](https://github.com/eriknihlen/openac-rust), the Rust Asheron's
Call client. It is a conversion of the C# plugin in `../src` to Rust: the
same macro and automation features (VTank-compatible settings, loot, meta and
route profiles; buffing, combat, looting, navigation, vendoring, the
UtilityBelt commands), built for `wasm32-unknown-unknown` and tested
natively. The same plugin runs in a client session with a game window and in
one without.

The code in this folder is licensed under the MIT license (see
[LICENSE](LICENSE)); [NOTICE.md](NOTICE.md) lists what was not converted and
why. The C# plugin beside it keeps its own terms.

## Building

The toolchain is pinned in `rust-toolchain.toml` (Rust 1.95.0). Install the
WebAssembly target once, then build the release module:

```bash
rustup target add wasm32-unknown-unknown
cargo build --workspace --target wasm32-unknown-unknown --release
# -> target/wasm32-unknown-unknown/release/mosstank.wasm
```

The SDK crates (`openac-plugin`, `openac-plugin-compat`, `ac-plugin-abi`)
come from the openac-rust repository by git, pinned to a commit in
`Cargo.toml`.

`scripts/check.ps1` (or `scripts/check.sh`) runs the full check: formatting,
clippy, the tests, the WebAssembly build, the module's imports, and the
per-call time budgets.

## Packaging a release

A plugin folder holds `plugin.json` and the module it names
(`mosstank.wasm`). Copy both into an empty folder and pack it with
`plugincheck` from the openac-rust repository (see its
`docs/plugin-development.md`):

```bash
cargo install --locked --git https://github.com/eriknihlen/openac-rust plugincheck
plugincheck pack <plugin folder> <out dir>
plugincheck <out dir>/acdream.mosstank-<version>.zip
```

`pack` writes `plugin.json`, `acdream.mosstank-<version>.zip` and
`acdream.mosstank-<version>.zip.sha256`. They are published as the assets of
a release tagged `wasm-v<version>`, which keeps these builds apart from the C#
plugin's `v<version>` releases.

## Installing

In the openac-rust launcher, on the plugins tab:

- **Discover** lists the plugins published for openac-rust; find MossTank
  and install it.
- **Add from URL** installs it straight from this repository:
  `https://github.com/eriknihlen/openac-mosstank`.

The launcher installs it from this repository's `wasm-v` releases, checks
them for updates, and stages the plugin disabled; enabling it is a separate
choice on that tab.

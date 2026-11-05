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
`Cargo.toml`: openac-rust v0.1.12, plugin API 1.8 (a canvas hears the
pointer hovering over it from API 1.6), so MossTank needs client 0.1.11 or
later; its fellowship rules see fellows out of sight from client 0.1.12.

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

## The Comps HUD

A small strip that counts the items you choose (spell components, peas,
healing kits, anything you carry), after the comps counter of Virindi HUDs.
Each item shows its icon (as the game draws the item you carry) and how many
you carry, in packs or worn, counted again every 0.6 seconds. Hover over an
item for its name, or over a button for what it does. It is drawn in your
plugin theme (Appearance), and follows a theme change at once.

- **Show it**: tick **Show HUD** in the Comps HUD box on the Consumables page,
  or type `/vt compshud on`. The switch is the same for all your characters.
- **Layout**: **Column** (a narrow strip, one row per item: icon, then its
  count) or **Line** (one row: icon and count, icon and count). The strip is
  as wide as its counts need, and resizes when a count gains or loses a digit.
  Pick it in the box, with the strip's layout button, or with
  `/vt compshud column` and `/vt compshud line`.
- **Choose the items**: **Edit items...** in the box, or the pencil on the
  strip, opens Edit items. Tick what you carry (search it by name or filter it
  by kind) to put it on the HUD; on the right, select an item to move it up or
  down, remove it, or set **Warn below**: its count turns red below that
  number (0 never warns). **Add selected item** there, **Add to HUD** beside
  Remove on the Consumables page, and `/vt compshud add` add the item
  selected in the game. `/vt compshud remove <name>` takes one off.
- **Move it**: drag the strip anywhere but its buttons (it follows the pointer
  and is saved where you let go); it stops at the screen's top and left
  edges. `/vt compshud reset` puts it back at its first place. **Hide it**:
  its cross (this unticks Show HUD).
- Each character keeps its own list (in order, with the warn levels), layout
  and place. `/vt compshud` alone lists what it counts. A saved list that does
  not read whole is copied to `recovery/comps-hud/` in MossTank's storage and
  said in the log; it is written again only when that copy was made, and
  otherwise left as it is for the session.

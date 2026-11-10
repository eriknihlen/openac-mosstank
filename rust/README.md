# MossTank for OpenAC

This folder is MossTank built as a WebAssembly plugin for OpenAC, the Rust
Asheron's Call client (released from
[eriknihlen/OpenAC](https://github.com/eriknihlen/OpenAC)). It is a
conversion to Rust of the C# plugin, which is now on this repository's
`deprecated` branch: the same macro and automation features
(VTank-compatible settings, loot, meta and route profiles; buffing, combat,
looting, navigation, vendoring, the UtilityBelt commands), built for
`wasm32-unknown-unknown` and tested natively. The same plugin runs in a
client session with a game window and in one without. The `// Source:`
comments in the tests name the C# tests each one ports, on that branch.

The code in this folder is licensed under the MIT license (see
[LICENSE](LICENSE)); [NOTICE.md](NOTICE.md) lists what was not converted and
why. The files the C# plugin left outside this folder, which the port reads
in place (its panel markup and experience table in `../src`, its test
fixtures in `../tests`), keep the repository's own terms (`../LICENSE`).

## Building

The toolchain is pinned in `rust-toolchain.toml` (Rust 1.95.0). Install the
WebAssembly target once, then build the release module:

```bash
rustup target add wasm32-unknown-unknown
cargo build --workspace --target wasm32-unknown-unknown --release
# -> target/wasm32-unknown-unknown/release/mosstank.wasm
```

The SDK crates (`openac-plugin`, `openac-plugin-compat`, `ac-plugin-abi`)
come from the OpenAC repository by git, pinned to a commit in
`Cargo.toml`: OpenAC `main` at a9a59935, plugin API 1.11 (who changed
the selection; 1.10: every enchantment on the character, right-aligned
list columns), so MossTank needs OpenAC client 0.2.3 or later.

`scripts/check.ps1` (or `scripts/check.sh`) runs the full check: formatting,
clippy, the tests, the WebAssembly build, the module's imports, and the
per-call time budgets.

## Packaging a release

A plugin folder holds `plugin.json` and the module it names
(`mosstank.wasm`). Copy both into an empty folder and pack it with
`plugincheck` from the OpenAC repository (see its
`docs/plugin-development.md`):

```bash
cargo install --locked --git https://github.com/eriknihlen/OpenAC plugincheck
plugincheck pack <plugin folder> <out dir>
plugincheck <out dir>/acdream.mosstank-<version>.zip
```

`pack` writes `plugin.json`, `acdream.mosstank-<version>.zip` and
`acdream.mosstank-<version>.zip.sha256`. They are published as the assets of
a release tagged `wasm-v<version>`, which keeps these builds apart from the C#
plugin's `v<version>` releases.

## Installing

In the OpenAC launcher, on the plugins tab:

- **Discover** lists the plugins published for OpenAC; find MossTank
  and install it.
- **Add from URL** installs it straight from this repository:
  `https://github.com/eriknihlen/openac-mosstank`.

The launcher installs it from this repository's `wasm-v` releases, checks
them for updates, and stages the plugin disabled; enabling it is a separate
choice on that tab.

## The UB looter

A port of UtilityBelt's looter: it takes what the **active loot profile**
keeps out of chests and your own corpse, judged by MossTank's own loot rules
(the corpse looter's). Keep, Keep # (up to its count, counting what you carry)
and Salvage take an item (salvage is looted, not salvaged); an item whose
verdict needs an appraisal is appraised first, one at a time. Monster corpses
stay the macro's corpse looter's, and a container it is working is left to it.

- **Chests and your own corpse, as they open.** With `Looter.Enabled` on, a
  chest (with `Looter.EnableChests`) or your own corpse (with
  `Looter.EnableMyCorpse`) is looted when it opens, by hand or otherwise, and
  left open. Your own corpse gives back everything in it, whatever the loot
  profile says (as UtilityBelt does); while it does, the
  macro's route, attack spells and item use are held, and with
  `Looter.BlockVtankMelee` its melee and missile attacks too. Only a container
  with room for 48 items or more is looted (chests and corpses, never packs).
- **`/ub use <chest or corpse>`** opens it (up to `Looter.MaxOpenAttempts`
  tries, `Looter.DelaySpeed` milliseconds apart). With `Looter.Enabled` on it
  then loots it and closes it: a chest with `Looter.EnableChests`, your own
  corpse with `Looter.EnableMyCorpse` (everything in it), and a monster's
  corpse always. Otherwise it is only opened. A locked chest:
  `Looter: Chest is locked... bailing`. The key looter works whatever these
  switches say.
- **The key looter**: on the UB page, **Set key** (select a key first),
  **Set chest** (select the chest) and **Start**; or select the chest and type
  `/ub keyloot <key name>` (MossTank's own command; UtilityBelt has only the
  button), `/ub keyloot stop` to stop. Each round uses a key on the chest (up
  to `Looter.MaxUnlockAttempts` tries), opens it, loots it and closes it, and
  the next round starts while keys of that name are left.
- `Looter.AttemptsBeforeBlacklisting` (500) is how many steps one item is tried
  before it is given up on; `Looter.OverallSpeed` (100, the least allowed) is
  how often, in milliseconds, the looter steps. `Looter.JumpWhenLooting`,
  `JumpHeight`, `AutoSalvageAfterLooting` and `TestMode` are kept for macros
  and do nothing here.

## The Comps HUD

A small strip that counts the items you choose (spell components, peas,
healing kits, anything you carry), after the comps counter of Virindi HUDs.
Each item shows its icon (as the game draws the item you carry) and how many
you carry, in packs or worn, counted again every 0.6 seconds. Like the mana
overlay it is drawn straight on the game, with no window around it: icons
and outlined counts. With the pointer over it its three buttons show (the
layout switch, the pencil and a ×); hover over an item for its name, or over
a button for what it does.

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
  its × (this unticks Show HUD).
- Each character keeps its own list (in order, with the warn levels), layout
  and place. `/vt compshud` alone lists what it counts. A saved list that does
  not read whole is copied to `recovery/comps-hud/` in MossTank's storage and
  said in the log; it is written again only when that copy was made, and
  otherwise left as it is for the session.

## Item info

After UtilityBelt's ItemInfo: select an item (in your packs, on a corpse or
on the ground) and a line about it is written in chat, starting with the
loot rule the looter would decide it by in the loot profile you have loaded:

```text
[UB] ItemInfo: Keep armor - Silver Breastplate, Soldier's Set, AL 512, AR, Tinks 2, Epic Strength, Major Impenetrability, Wield Lvl 150, Craft 7, [D 2]
```

UtilityBelt makes the name a link; here it is plain text, as the client
shows the tags in a plugin's chat line as written. An item not appraised yet
is appraised first. It is off until you turn it on, as in UtilityBelt:
`/ub opt set ItemInfo.Enabled true` and
`/ub opt set ItemInfo.DescribeOnSelect true` (or tick both on the UB
settings page). `DescribeOnSelect` describes every selection, however it
was made. To describe only what you click instead, turn on
`ItemInfo.DescribeOnLeftClick` or `ItemInfo.DescribeOnRightClick`: the
client says that the mouse made a selection but not with which button, so
either switch describes every selection made with the mouse, whichever
button. `ItemInfo.ShowValueAndBurden` adds the value and burden;
`ItemInfo.ShowDoorInfo` and `ItemInfo.ShowNpcInfo` describe doors and NPCs
too. UtilityBelt's buffed weapon values, damage calculations and retail
comparisons are not part of it (see `docs/divergences.md`, UB-13).

## The Mag tab and the Inventory window

The **Mag** tab holds Mag-Tools' tracker pages (Combat, Mana and Inv. Items),
and the **Inventory** window, with its own button on the dock, searches the
items of every character logged on this server; both port Mag-Tools by
Mag-nus (https://github.com/Mag-nus/Mag-Plugins), used with his permission
(see [NOTICE.md](NOTICE.md)). Its **Filters...** button opens the filters
window (item type and slot, cantrips at a tier or better, ratings, set,
requirements, the characters searched: the filters of the owner's
MosswartOverlord inventory search), and a click on a column heading sorts
the rows by it.

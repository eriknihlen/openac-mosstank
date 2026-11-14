# MossTank

A macro and automation plugin for **OpenAC**, the Rust Asheron's Call client
(released from [eriknihlen/OpenAC](https://github.com/eriknihlen/OpenAC)),
built for players coming from VTank. It reads the profile formats you already
have — settings, loot rules, metas and navigation routes — and runs the same
kind of unattended session: buff upkeep, combat, corpse looting, vitals and
mana management, fellowship handling, crafting and salvage.

It is a WebAssembly plugin: one module that runs unchanged wherever the client
runs, in a client session with a game window and in one without. With a window
it also brings its own panels — the main control panel, an advanced page, a
buff picker, a loot-rule editor, a meta editor, the UB pages and the Comps
HUD. The code is in [`rust/`](rust/); [its README](rust/README.md) covers
building, the full check, packaging a release, and the Comps HUD.

## Installing

In the OpenAC launcher, on the **Plugins** tab:

- **Discover** lists the plugins published for OpenAC
  ([eriknihlen/openac-plugins](https://github.com/eriknihlen/openac-plugins));
  find MossTank and install it.
- **Add from URL** installs it straight from this repository:
  `https://github.com/eriknihlen/openac-mosstank`.

The launcher installs it from this repository's `wasm-v<version>` releases,
checks them for updates, and stages the plugin **disabled**. Enabling it is a
separate, explicit choice you make on that tab.

Commands are registered under `/vt` and `/ub`, as VTank and UtilityBelt register them:
the macro's commands answer on `/vt`, the UtilityBelt ones on `/ub`, so command lines
written for either work as they are. `/vt help` and `/ub help` list each word's commands.
UtilityBelt's looter is there too: chests and your own corpse looted by the active loot
profile as they open (`Looter.Enabled`, `Looter.EnableChests`, `Looter.EnableMyCorpse`),
`/ub use` on a chest or corpse to open it and, with those switches on, loot and close it (your
own corpse gives back everything, whatever the profile says), and a key looter on the UB page
and as `/ub keyloot <key name>` ([rust/README.md](rust/README.md#the-ub-looter)).

Optional [personal kill statistics](KILL-STATISTICS.md) record durable per-character
counters for kills/hour reporting without relying on retained chat logs.

## The C# plugin (retired)

MossTank began as a C# plugin for the C# OpenAC client, which is retired
(2026-11-05). The C# plugin is on this repository's
[`deprecated`](https://github.com/eriknihlen/openac-mosstank/tree/deprecated)
branch and in the history; its last release is
[`v0.5.10`](https://github.com/eriknihlen/openac-mosstank/releases/tag/v0.5.10),
and it gets no further changes. The Rust plugin is a conversion of it with
the same behaviour; the `// Source:` comments in the Rust tests name the C#
test each one ports, on that branch.

The C# plugin left a few files outside `rust/` that the Rust plugin reads in
place: its panel markup (`src/AcDream.Plugins.MossTank/*.xml`), the
experience cost table beside it (`ExperienceCosts.tsv`), and the test
fixtures, real VTank and metaf profiles
(`tests/AcDream.Plugins.MossTank.Tests/Fixtures/`).

## Your existing profiles

MossTank parses the real formats byte-for-byte: `.usd` settings, `.utl` loot
profiles, `.met` and `.af` metas, and `.nav` routes. Its round-trip tests assert
the exact bytes, including line endings, against real profiles.

A route or meta saves back into the file it was loaded from, in that file's
form. `/vt nav save <name>` and `/vt meta save <name>` write a `.nav` or `.met`;
`/vt navaf save <name>` and `/vt metaaf save <name>` write MossTank's `.af`. A
bare name loads the `.nav` or `.met` first and the `.af` second.

**It never touches a file outside the folders the host hands it.** Every read
and write goes through a host storage surface, and there are two of them:

- **Everything you edit, copy or share** lives under `mosstank\` in the host's
  shared profile directory: `profiles\` settings, `metas\`, `navs\`,
  `loot\`, and `ub\` with the whole UtilityBelt tree beneath it — its
  settings tiers, named profiles, aliases, `maps\markers.csv`, dungeon maps
  and the `equip\`, `autovendor\` and `itemgiver\` profile folders, at the
  plugin, server and character tiers.
  Copy the profiles you want it to use in here.
- **State the plugin keeps for itself** — saved expressions, macro side-cars,
  recovery copies, enchantment timers, the onboarding marker — stays in the
  plugin's own storage folder and is not something you need to open.

An earlier version wrote the UtilityBelt tree into the plugin's own storage
under `ub\`. On the first start after upgrading, those files are **copied**
into the new place and the originals are left exactly where they were; one
line in chat says how many moved across. Its panel markup is built into the
module, so it opens no other path. The originals of any profiles you copy in,
wherever they live, are not read, not moved and not rewritten. The panel
tells you where the folders are.

The game information a VTank profile folder keeps in `gameinfodb.ugd`
(ammunition, grenades, monster damage and species, drain and martyr spells,
craft recipes) is not shipped with MossTank. It comes from
[openac-gamedata](https://github.com/eriknihlen/openac-gamedata), an
independent project (AGPL-3.0) that generates a complete `gameinfodb.ugd` from
the ACE server's world data and publishes it as a release file. At login
MossTank downloads the newest release and, unless the file you have is that
same release, puts it in `gameinfodb.ugd` in the VTank profile folder,
replacing any database that came from elsewhere, and says the outcome in one
chat line. A check made within the last 6 hours is
not repeated, so many sessions logging in do not all download;
`/vt gamedb interval [hours]` changes that (0 checks at every login). A failed
check keeps the file you had. Until the first download there are no
ammunition choices, grenades or craft recipes. `/vt gamedb` shows what is
loaded and when the next check is due; `/vt gamedb update` checks now.

## Building, checking and releasing

See [rust/README.md](rust/README.md). In short: the toolchain is pinned in
`rust/rust-toolchain.toml`, and `rust/scripts/check.ps1` (or `check.sh`) runs
the full check — formatting, clippy, the tests, the WebAssembly build, the
module's imports and the per-call fuel budgets. CI runs the same check on
Linux and Windows for every push to `main` or `rust-port` and every pull
request into them. A release is the module and its `plugin.json` packed with
`plugincheck` and published as a release tagged `wasm-v<version>`; the plain
`v<version>` tags are the C# plugin's.

## Attribution

The UB tab, the UB settings, the Tinker tab and the `/ub` commands are ports of
**UtilityBelt** (MIT, by its contributors: Aquafir, Brycter, Cosmic Jester,
delasteve, dpbarrett, FlaggAC, enknamel, Harli, Schneebly, trevis, Yonneh),
rewritten against this client's plugin API. MossTank is also interoperable with **VTank** by Virindi: it reads and writes
VTank's file formats and answers its command and expression names, with no
VTank code included; the information needed for that was obtained by
decompilation for interoperability as permitted by Article 6 of the EU
Software Directive (2009/24/EC). MossTank's own messages are written in its
own words. Its `.af` files carry the credit header of **metaf** by Eskarina
(https://github.com/JJEII/metaf); the C# plugin's `.af`, `.met` and `.nav`
readers and writers were adapted from metaf, and the Rust plugin's were
written from the formats instead. Details and license texts:
[NOTICE.md](NOTICE.md) and [rust/NOTICE.md](rust/NOTICE.md).

## Licence

The Rust plugin (everything under `rust/`) is under the MIT license: see
[rust/LICENSE](rust/LICENSE) and [rust/NOTICE.md](rust/NOTICE.md). The rest
of this repository (the files the C# plugin left, the kill-statistics
collector in `tools/`, this documentation) stays under the GNU General Public
License v3 or later, as the C# plugin was: see [LICENSE](LICENSE). The Rust
plugin compiles in the panel markup and the experience table from `src/`, so
a built `mosstank.wasm` includes those GPL parts. MossTank 0.5.0 and earlier
were released under MIT. Third-party notices, including metaf's:
[NOTICE.md](NOTICE.md).

## Plugin appearance and option search

On hosts with plugin themes, MossTank follows the selected theme, including
its secondary windows. The shared controls provide searchable choices for
settings, navigation, loot and meta profiles; filtering does not load a profile.

Advanced Options keeps the original setting names and descriptions. Search
matches every word against the name or description, together with the category
checkboxes. Select a row to edit it below: switches and choices apply immediately,
while numeric values apply with Enter or Apply. Filtering never changes a setting.
A selected setting hidden by a filter cannot be edited until it is shown again.

The Options-tab switches **Walk to Rare Corpse**, **Loot Own Death Corpse**
and **Show Nav Lines** also accept `/vt opt get`, `set` and `toggle` using
`WalkToRareCorpse`, `LootOwnDeathCorpse` and `ShowNavLines`. `WalkToRareCorpse`
and `LootOwnDeathCorpse` also appear under Looting in Advanced Options. The old
`WalkToOwnRareCorpses` and `WalkToOwnCorpses` command names
remain accepted for existing macros; replies use the new name. The rare walk
still applies only to this character's own rare corpse. `LootOwnDeathCorpse`
(on by default) loots this character's own death corpse as soon as it is in
range, whoever the killer was, and takes everything in it rather than only what
the loot rules keep, even with Loot Only Rare Corpses on. These
switches save with the selected settings profile.

Selecting Custom for `BuffProfile_Prots` or `BuffProfile_Banes` also shows the
corresponding `BuffProfile-Prots` or `BuffProfile-Banes` text field. Enter element
letters (A acid, L lightning, F fire, C cold, B bludgeon, P pierce, S slash), then
press Enter or Apply. An empty value selects no elements. These are element
profiles, not spell levels or numeric masks; the existing setting keys remain
unchanged.

### Compact macro controls

The **R** button in MossTank's top-right corner switches to a compact remote;
**M** restores the main window. Switching windows leaves the macro running.
The remote shares the main window's On, Buff, Combat, Navigate, Loot, Meta,
FollowAroundCorners and TargetLock settings, plus Force Buff (**F**) and
Cancel Force Buff (**CF**). It supports Classic and both modern plugin themes.

**FC** follows the selected player through the same separate `UBFollow` route
used by `/ub follow`. It preserves the previously loaded navigation file and
does not change any macro switches. Enable navigation and the macro to move.
The host remembers the remote's position like other plugin windows.

### Action history

The **History** button beside the current action on Options opens the current
status and timestamped action changes. It follows new lines while at the bottom;
scrolling up keeps your reading position until you return to the bottom. Actions
continue to be recorded while the window is closed. The latest 1,000 changes are
kept for the session; consecutive repeats are omitted. **Clear** empties the
history without affecting the macro or current status, and **X** closes the window.

## Peer features

### Team play

MossTank bots heal each other and share targeting, on one computer or
several. The team is the fellowship plus your other characters the peer
network reports (on this computer, or brought in from another by a bridge).
Four switches, on the Vitals tab and under `/vt opt get|set|toggle|setinall`
(so `/ub bc` can set a whole group), saved with the settings profile:

- `HelpFellows` (on): heal, restam and remana fellows, and share debuff
  targeting with them. It is VTank's `DoHelp`; both names work.
- `HelpNetworkPeers` (on): the same for the network peers, fellows or not.
  The old UB switch `Sharing.Vitals` (and `VTank.VitalSharing`) is accepted
  by `/ub opt` as this switch.
- `TeamFocusDebuffed` (on): see below.
- `TeamOverlay` (off): the team overlay, the readout of this character and
  the team (bars, arrow, distance; "(net)" after a character on another
  computer). The UB tab's Network UI button and `NetworkUI.Enabled` are this
  switch; a value a UB settings file kept for `NetworkUI.Enabled` becomes
  `TeamOverlay` once for each character.

While either help switch is on, this character tells the others its vitals,
place and casts, and takes in theirs. A debuff a teammate has landed is not
cast again while it holds, and one a teammate is casting counts as up on its
monster for six seconds (a claim), so two debuffers split two monsters.
Casts read again from the start of the list, when the macro starts, give
their landed debuffs but no claims.

With `TeamFocusDebuffed`, a fighter on a monster the team has not debuffed
switches to one in range that a teammate has debuffed (this character's own
debuffs and claims do not count), chosen among those by the usual order.
Then it stays: that monster is kept while it is a valid target and the
team's debuff holds, whatever else appears, even a higher-priority monster.
The only move is from an undebuffed monster to a team-debuffed one, so it
never flip-flops, and a monster that could not be attacked a moment ago is
left alone for five seconds. The player's locked selection (Target Lock),
and a monster this character still owes a debuff to while it debuffs first,
are never overridden. A bot with no teammates chooses as before. VTank has
no such switch.

`/vt team` lists the team, claims and debuffs in force on monsters, and why
the helper and the target choice last did what they did; `/vt team overlay
[on|off]` switches the overlay.

### Broadcasts

Receive broadcasts defaults on so `/ub bc` works between logged-in characters on this computer in the same world. It is stored per character; an explicitly saved choice is preserved. Receive broadcasts allows this client to execute incoming tagged broadcast commands. Sending a broadcast does not enable reception. The team (above), the network readout and metas containing netclients[] or netcasts[] request their own capabilities. Requests end when the feature stops or the plugin unloads. A meta needing peers waits for the initial connection before evaluating peer-dependent rules.

### Action history and debug files

Action History includes navigation, active rule, meta state, pause reasons, loot,
buffs and recharge status. Changes are recorded without repeating unchanged
states or every distance/countdown update. Reading older entries keeps your scroll position.

The main window's **Debug log** button captures detailed macro channels without
turning on chat spam. The path appears in chat and the button tooltip. Files live
under `debug/` in MossTank plugin storage, are unique to the recording (safe with
multiple clients), and retain at most 2 MiB of the newest complete UTF-8 entries.
The file is flushed every five seconds and when logging stops or the plugin unloads;
an abrupt crash can lose the last five seconds. Logging starts off each session.

### Mana donor recovery

Items collected specifically for mana stones are remembered per character and
server in plugin storage (`mana-donors/`). Updates preserve these files.
Dropped, sold, traded, or consumed donors lose their drain permission when the
client confirms inventory ownership; moving a donor between packs retains it.
Incomplete login inventories do not erase saved entries. Existing untracked
items are never automatically adopted as donors.

Unanswered drain requests remain reserved across relogs. Late inventory changes
release the reservation; they do not trigger a blind repeat of a destructive use.
Busy/unavailable commands remain queued, while permanent refusals stay recorded
for review and are reported in loot history/debug logging. Queued donors and
held stones count against further donor pickups. Protected or equipped items
remain subject to the normal safety checks. A storage error disables draining
rather than risking a repeat after restart.

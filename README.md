# MossTank

A macro and automation plugin for the [OpenAC](https://github.com/eriknihlen/OpenAC)
client, built for players coming from VTank. It reads the profile formats you
already have — settings, loot rules, metas and navigation routes — and runs the
same kind of unattended session: buff upkeep, combat, corpse looting, vitals and
mana management, fellowship handling, crafting and salvage.

It runs on both OpenAC hosts, unchanged: the windowed client and the windowless
host (`acdream-headless`), through the same plugin contract. On the windowed
client it also brings its own panels — the main control panel, an advanced page,
a buff picker, a loot-rule editor and a meta editor.

## Installing

Through the OpenAC launcher, on the **Plugins** tab:

- **Discover** lists plugins from
  [eriknihlen/openac-plugins](https://github.com/eriknihlen/openac-plugins);
  find MossTank and install it.
- **Add from URL** installs it straight from this repository if it is not
  listed yet: `https://github.com/eriknihlen/openac-mosstank`.

Either way the launcher stages the plugin **disabled**. Enabling it is a
separate, explicit choice you make on that tab. A hand install works too: unzip
the release's `acdream.mosstank-<version>.zip` into its own subdirectory of your
plugins folder (`%LOCALAPPDATA%\acdream\plugins` on Windows).

Commands are registered under `/vt` and `/ub`, as VTank and UtilityBelt register them:
the macro's commands answer on `/vt`, the UtilityBelt ones on `/ub`, so command lines
written for either work as they are. `/vt help` and `/ub help` list each word's commands.

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
  shared profile directory (`%LOCALAPPDATA%\acdream\vtank\mosstank\` on
  Windows): `profiles\` settings, `metas\`, `navs\`, `loot\`, and `ub\` with
  the whole UtilityBelt tree beneath it — its settings tiers, named profiles,
  aliases, `maps\markers.csv`, dungeon maps and the `equip\`, `autovendor\`
  and `itemgiver\` profile folders, at the plugin, server and character tiers.
  Copy the profiles you want it to use in here.
- **State the plugin keeps for itself** — saved expressions, macro side-cars,
  recovery copies, enchantment timers, the onboarding marker — stays in the
  plugin's own storage folder and is not something you need to open.

An earlier version wrote the UtilityBelt tree into the plugin's own storage
under `ub\`. On the first start after upgrading, those files are **copied**
into the new place and the originals are left exactly where they were; one
line in chat says how many moved across. The only other path the plugin opens
is its own installed folder, to load its panel markup. The originals of any
profiles you copy in, wherever they live, are not read, not moved and not
rewritten. The panel tells you where the folders are.

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

## Building from source

The plugin builds against the `AcDream.Plugin.Abstractions` package, not a
client checkout. The package is published as an asset on an OpenAC GitHub
release; drop the `.nupkg` into `./packages-local`, which `NuGet.Config`
registers as a folder feed. From an OpenAC checkout you can pack it yourself:

```
dotnet pack <OpenAcRoot>/src/AcDream.Plugin.Abstractions -c Release -o ./packages-local
```

`PluginApiPackageVersion` in `Directory.Build.props` names the contract version
to resolve; it has to match the package in the feed.

```
dotnet restore MossTank.slnx
dotnet build MossTank.slnx -c Release
dotnet test tests/AcDream.Plugins.MossTank.Tests -c Release --no-build
```

To produce the release zip and its checksum locally — the same script CI runs:

```
pwsh tools/build-release-zip.ps1 -Configuration Release
```

## Releasing

Tag `v<version>`, where `<version>` is `Version` in `Directory.Build.props` and
the `version` in `src/AcDream.Plugins.MossTank/plugin.json`; a test fails when
those two drift. CI builds, tests, assembles the zip, checks it with OpenAC's
own `acdream-plugincheck`, and publishes the release with the three assets the
launcher expects. A tag with a `-` suffix publishes a pre-release, which the
launcher offers only to players who opt this plugin into its Beta channel.

## Attribution

The UB tab, the UB settings, the Tinker tab and the `/ub` commands are ports of
**UtilityBelt** (MIT, by its contributors: Aquafir, Brycter, Cosmic Jester,
delasteve, dpbarrett, FlaggAC, enknamel, Harli, Schneebly, trevis, Yonneh),
rewritten against this client's plugin API. MossTank is also interoperable with **VTank** by Virindi: it reads and writes
VTank's file formats and answers its command and expression names, with no
VTank code included; the information needed for that was obtained by
decompilation for interoperability as permitted by Article 6 of the EU
Software Directive (2009/24/EC). MossTank's own messages are written in its
own words. Its `.af` files and its `.met`/`.nav` readers and writers are adapted
from **metaf** by Eskarina (https://github.com/JJEII/metaf). Details and license texts:
[NOTICE.md](NOTICE.md).

## Licence

GNU General Public License v3 or later. See [LICENSE](LICENSE).
MossTank 0.5.0 and earlier were released under MIT. Third-party notices,
including metaf's: [NOTICE.md](NOTICE.md).

## Plugin appearance and option search

On hosts with plugin themes, MossTank follows the selected theme, including
its secondary windows. The shared controls provide searchable choices for
settings, navigation, loot and meta profiles; filtering does not load a profile.

Advanced Options keeps the original setting names and descriptions. Search
matches every word against the name or description, together with the category
checkboxes. Select a row to edit it below: switches and choices apply immediately,
while numeric values apply with Enter or Apply. Filtering never changes a setting.
A selected setting hidden by a filter cannot be edited until it is shown again.

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

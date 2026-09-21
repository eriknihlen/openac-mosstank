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

Commands are registered under `/vt`, so VTank command lines work as they are.

## Your existing profiles

MossTank parses the real formats byte-for-byte: `.usd` settings, `.utl` loot
profiles, `.met` and `.af` metas, and `.nav` routes. Its round-trip tests assert
the exact bytes, including line endings, against real profiles.

**It never touches a file outside its own storage folder.** Every profile read
and write goes through the host's plugin-storage surface, rooted at one
directory the host owns for this plugin; the only other path the plugin opens is
its own installed folder, to load its panel markup. So copy the profiles you
want it to use into that folder — the originals wherever they live are not read,
not moved and not rewritten. The panel tells you where the folder is.

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

## Licence

MIT. See [LICENSE](LICENSE).

# Third-Party Notices

MossTank is free software, licensed under the GNU General Public License,
version 3 or (at your option) any later version (see [LICENSE](LICENSE)).

    MossTank, an automation plugin for the OpenAC Asheron's Call client
    Copyright (C) 2026 Erik Nihlén and OpenAC contributors

    This program is distributed in the hope that it will be useful, but
    WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General
    Public License for more details.

MossTank 0.5.0 and earlier were released under the MIT license. Those
releases stay under MIT for anyone who has them, and the MIT notice for that
code is kept below. This file also lists the third-party work MossTank builds
on, with the license terms and credits that work carries.

---

## UtilityBelt

MossTank's UB tab, UB settings, Tinker tab, and the `/ub` commands that mirror
UtilityBelt's are ports of features from **UtilityBelt**, the Decal plugin
for Asheron's Call (https://gitlab.com/utilitybelt/utilitybelt.gitlab.io),
MIT-licensed as stated in its README and confirmed by its author.

What was taken from UtilityBelt, rewritten as MossTank code against this
client's plugin API rather than copied:

- the command names, argument grammars, chat wording and settings catalogue
  (names, defaults, scopes) of the ported tools: item giver, auto vendor,
  counter, aliases, game events, login switching, equipment profiles,
  experience spending, vital and cast sharing, nametags, dungeon and
  landscape maps, jumper, networking, auto tinker;
- the numeric tables and formulas of the auto-tinker calculator (tinkering
  difficulty and success, material modifiers, imbue rules);
- the on-disk layout of its settings and profile folders, so existing
  UtilityBelt files keep working;
- the meta expression grammar and function set, for compatibility with
  metas written for UtilityBelt and VTank.

UtilityBelt contributors, as credited in its README: Aquafir, Brycter,
Cosmic Jester, delasteve, dpbarrett, FlaggAC, enknamel, Harli, Schneebly,
trevis, Yonneh.

MIT license text as it applies to UtilityBelt:

```
MIT License

Copyright (c) UtilityBelt contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## VTank

MossTank is an independently written plugin that is **interoperable with
VTank** (uTank2, by Virindi): it reads and writes VTank's profile formats
(`.usd`, `.utl`, `.met`, `.nav`), answers the same `/vt` command names and
meta expression function names, and reproduces the macro behaviour those
profiles and metas rely on, so a player's existing files run unchanged.

How that interoperability was achieved, stated plainly:

- No VTank code, assets or resources are included in MossTank. Every line of
  MossTank was written for this client's plugin API.
- Information needed to read VTank's file formats and to answer its command
  and expression names was obtained by decompiling the VTank plugin, as
  permitted for interoperability by Article 6 of Directive 2009/24/EC (the
  EU Software Directive), which applies in the maintainers' jurisdiction.
  That information was used for interoperability only; the decompilation is
  not distributed.
- Names that a profile, meta or player types (setting keys, spell and item
  names, command and expression function names, file headers) are kept
  identical, because compatibility requires it. MossTank's own status,
  warning and help text is written in MossTank's own words.
- Chat patterns MossTank reads are the game's own messages, not VTank's.
- The game-information database (`gameinfodb.ugd`) is not included. MossTank
  downloads it at run time, on the player's own machine, into that player's
  VTank profile folder, from openac-gamedata (see below), not from any VTank
  service.
- The documents a new settings profile and an empty game database start from
  are built by MossTank in code, in VTank's file formats, from MossTank's own
  option defaults and table definitions; no VTank file is shipped for them.
  The option descriptions shown in MossTank are written in MossTank's own words.

VTank is not open source. Its license terms are Virindi's; this notice
claims no license to VTank and grants none.

## openac-gamedata

The game database MossTank downloads comes from **openac-gamedata**
(https://github.com/eriknihlen/openac-gamedata), an independent project
licensed under the GNU Affero General Public License v3.0. It generates a
complete `gameinfodb.ugd` from the world data of the ACE (Asheron's Call
Emulator) server and publishes it as a release file. MossTank bundles none of
it: the file is fetched at run time into the player's own profile folder, and
its license terms are that project's.

## metaf

MossTank's `.af` meta and route files, and its readers and writers for
VTank's `.met` and `.nav` files, are adapted from **metaf** by Eskarina of
Morningthaw and Coldeve:

**https://github.com/JJEII/metaf**

metaf created the `.af` format and is the best documentation there is of
VTank's file formats. If you want to understand those formats, or build
something that reads or writes them, start there.

The code was adapted by way of a Python port of metaf, and is licensed
under the GNU General Public License v3, as metaf is. It is included with
the knowledge and permission of metaf's author, whose one request is that
anything built on metaf credits it and links back to it. Every `.af` file
MossTank writes begins with metaf's own header, which names metaf and links
to it.

## MossTank under the MIT license (0.5.0 and earlier)

```
MIT License

Copyright (c) 2026 Erik Nihlén and OpenAC contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

# Notices for the Rust conversion

Everything under `rust/` is licensed under the MIT license (see
[LICENSE](LICENSE)), by the owner's decision of 2026-10-13. The C# plugin
next to it (`../src`, `../tests`) keeps its own terms (`../LICENSE`,
`../NOTICE.md`).

## metaf: what is not translated, and why

MossTank's C# readers and writers for `.af`, `.met` and `.nav` files are
adapted from **metaf** by Eskarina of Morningthaw and Coldeve
(https://github.com/JJEII/metaf), which is GPL-3. That code cannot be
relicensed under MIT, so it is **not translated** into this port.

The metaf-derived C# files, identified by their header comment
("Adapted from metaf ... See NOTICE.md"), by the metaf section of
`../NOTICE.md`, and by commit `6110bc4` ("Relicense MossTank under GPLv3 and
credit metaf properly"), which added that header to exactly these files:

| C# file | What it is |
|---|---|
| `src/AcDream.Plugins.MossTank/MetafSerializer.cs` | `.af` (metaf text) reader and writer for metas and routes |
| `src/AcDream.Plugins.MossTank/VtankMetaProfileSerializer.cs` | VTank `.met` reader and writer |
| `src/AcDream.Plugins.MossTank/VtankNavRouteSerializer.cs` | VTank `.nav` reader and writer |

All three entered the history together in `91b4083` ("MossTank 0.2.5", the
first commit), so the history cannot narrow the set further; no other source
file carries the header or mentions a metaf origin. The meta model these
serializers fill (`Meta.cs`: condition and action kinds, rules, states) and
the route model (`Navigation.cs`: waypoints) are MossTank's own and are
converted normally.

How the Rust readers and writers for these formats were made:

- Written from the file formats themselves: the fixture files in
  `../tests/AcDream.Plugins.MossTank.Tests/Fixtures/vtank` (`af/`, `met/`,
  `nav/`) and the C# tests' inputs and expected outputs, used as golden data.
  The three files above were not read for the conversion.
- Test data is used as it is: the fixtures (including `.af` files metaf
  wrote) are read in place from the C# test folder, never copied in.
- The header every `.af` file starts with is metaf's own credit block; metaf's
  author asks that anything built on metaf credits it and links to it, and
  MossTank's `.af` files must stay readable by metaf. The Rust writer emits
  that header unchanged, kept as data in
  `crates/mosstank-formats/src/af/header.txt`. **For the owner to confirm:**
  the header is metaf's text (a credit line, a link, and the format's keyword
  list), carried for attribution and compatibility, not code.

## UtilityBelt

The UB tab, its settings and the `/ub` commands port features of
**UtilityBelt** (https://gitlab.com/utilitybelt/utilitybelt.gitlab.io), MIT
licensed; see `../NOTICE.md` for what was taken and its MIT notice, which
applies here too.

## VTank

MossTank reads and writes VTank's profile formats and answers its command
and expression names for interoperability; no VTank code is included. The
statement in `../NOTICE.md` ("VTank") applies to this port unchanged.

## The openAC plugin SDK

The plugin links `openac-plugin`, `openac-plugin-compat` and `ac-plugin-abi`
from the openAC-rust repository, MIT licensed.

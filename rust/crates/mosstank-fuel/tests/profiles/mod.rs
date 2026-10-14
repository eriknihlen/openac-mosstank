//! A real player's VTank profile folder, as the fuel gate measures it
//! (owner, 2026-10-14: MossTank ran out of fuel on play.coldeve.ac loading
//! a character's profile set out of a folder of hundreds of metas, routes
//! and settings files and 400 KB loot profiles, and spent 33-46 M a tick
//! listing it).
//!
//! The folder is the owner's (`%LOCALAPPDATA%\openac-rust\vtank\mosstank`,
//! or `MOSSTANK_FUEL_PROFILES=<folder>`), read in place at test time and
//! never copied into this repository; without it, a generated stand-in
//! with its counts and sizes, built from the C# tests' fixtures.

use std::path::{Path, PathBuf};

use crate::common;

/// The world the characters play on.
pub const WORLD: &str = "Coldeve";

/// The folder the profiles come from, and what was found there.
pub struct Folder {
    /// Where it came from: the folder, or "generated".
    pub source: String,
    /// Every file, keyed as the profile storage keys it
    /// (`mosstank/<folder>/<file>`).
    pub files: Vec<(String, String)>,
    /// The character whose profile set is the heaviest: its name.
    pub character: String,
}

impl Folder {
    /// The files under `folder` (`metas`, `loot`, ...), and their bytes.
    pub fn count(&self, folder: &str) -> (usize, usize) {
        let prefix = format!("mosstank/{folder}/");
        self.files
            .iter()
            .filter(|(key, _)| key.starts_with(&prefix))
            .fold((0, 0), |(count, bytes), (_, text)| {
                (count + 1, bytes + text.len())
            })
    }
}

/// The owner's folder: `MOSSTANK_FUEL_PROFILES`, else the client's VTank
/// folder under the local application data.
fn owner_folder() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("MOSSTANK_FUEL_PROFILES") {
        return Some(path.into());
    }
    let local = std::env::var_os("LOCALAPPDATA")?;
    let folder = Path::new(&local).join("openac-rust/vtank/mosstank");
    folder.is_dir().then_some(folder)
}

/// The text a file reads as on the client: a byte-order mark dropped,
/// what is not UTF-8 replaced (openac-rust `ac_plugin::storage::decode_text`
/// for the files VTank writes).
fn decode(bytes: &[u8]) -> String {
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn walk(root: &Path, folder: &Path, files: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk(root, &path, files);
        } else if let Ok(bytes) = std::fs::read(&path) {
            let relative = path
                .strip_prefix(root)
                .expect("beneath the root")
                .to_string_lossy()
                .replace('\\', "/");
            files.push((format!("mosstank/{relative}"), decode(&bytes)));
        }
    }
}

/// The character on [`WORLD`] whose binding names the most bytes of
/// profiles: its name.
fn heaviest_character(files: &[(String, String)]) -> Option<String> {
    let size = |key: &str| {
        files
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
            .map_or(0, |(_, text)| text.len())
    };
    let prefix = format!("mosstank/profiles/{WORLD}_");
    files
        .iter()
        .filter_map(|(key, text)| {
            let name = key.strip_prefix(&prefix)?.strip_suffix(".cdf")?;
            let bytes: usize = text.lines().skip(1).map(|line| size(line.trim())).sum();
            Some((bytes, name.to_owned()))
        })
        .max()
        .map(|(_, name)| name)
}

/// The folder measured: the owner's, else the stand-in.
pub fn folder() -> Folder {
    if let Some(root) = owner_folder() {
        let mut files = Vec::new();
        walk(&root, &root, &mut files);
        if let Some(character) = heaviest_character(&files) {
            return Folder {
                source: root.display().to_string(),
                files,
                character,
            };
        }
    }
    generated()
}

/// The stand-in's counts and sizes: the owner's folder on 2026-10-14.
const METAS: usize = 257;
const NAVS: usize = 436;
const PROFILES: usize = 313;
const LARGE_LOOT_PROFILES: usize = 8;
const LARGE_LOOT_BYTES: usize = 400 * 1024;
const SMALL_LOOT_PROFILES: usize = 6;
const UB_FILES: usize = 8;
const UB_BYTES: usize = 808 * 1024;

/// A loot profile of about `bytes`: the C# fixture's rules over and over.
fn loot_profile(bytes: usize) -> String {
    let fixture = common::csharp_fixture("vtank/loot-c.utl");
    let mut lines = fixture.split("\r\n");
    let (Some(_), Some(version), Some(count)) = (lines.next(), lines.next(), lines.next()) else {
        panic!("the loot fixture has no header");
    };
    let count: usize = count.trim().parse().expect("a rule count");
    let rules: Vec<&str> = lines.collect();
    let rules = rules.join("\r\n");
    let rules = rules.trim_end_matches("\r\n");
    let copies = bytes / rules.len() + 1;
    let mut text = format!("UTL\r\n{version}\r\n{}\r\n", count * copies);
    for _ in 0..copies {
        text.push_str(rules);
        text.push_str("\r\n");
    }
    text
}

/// The generated folder: [`METAS`] metas and [`NAVS`] routes cycled from
/// the C# fixtures, [`PROFILES`] settings files and character bindings,
/// [`LARGE_LOOT_PROFILES`] loot profiles of 400 KB and the UB folder's
/// files; the character bound to the largest of each.
fn generated() -> Folder {
    // The character's own meta first (84 KB, as the owner's), then a
    // cycle of others that brings the folder to about its 2.9 MB.
    const MET: [&str; 11] = [
        "neftet",
        "lockandkey",
        "empyrean_facility",
        "example_sort_meta",
        "gauntlet_leader",
        "augments",
        "army_linear",
        "bella",
        "legendary_chests",
        "bobo",
        "createview",
    ];
    const NAV: [&str; 5] = [
        "barristunnels",
        "jaw_1",
        "nav_lockandkeyjaw",
        "nav_empyrean",
        "nav_ab",
    ];
    let character = "Mossy Hunter".to_owned();
    let mut files = Vec::new();
    let metas: Vec<String> = MET
        .iter()
        .map(|name| common::csharp_fixture(&format!("vtank/met/{name}.met")))
        .collect();
    for index in 0..METAS {
        let text = if index == 0 {
            &metas[0]
        } else {
            &metas[1 + (index - 1) % (metas.len() - 1)]
        };
        files.push((format!("mosstank/metas/meta{index:03}.met"), text.clone()));
    }
    let navs: Vec<String> = NAV
        .iter()
        .map(|name| common::csharp_fixture(&format!("vtank/nav/{name}.nav")))
        .collect();
    for index in 0..NAVS {
        files.push((
            format!("mosstank/navs/route{index:03}.nav"),
            navs[index % navs.len()].clone(),
        ));
    }
    let settings = common::csharp_fixture("vtank/owner-a.usd");
    let mut names: Vec<String> = (1..PROFILES / 2).map(|n| format!("Alt {n:03}")).collect();
    names.push(character.clone());
    if PROFILES % 2 == 1 {
        files.push((
            format!("mosstank/profiles/--{character}_{WORLD}_army.usd"),
            settings.clone(),
        ));
    }
    for name in &names {
        files.push((
            format!("mosstank/profiles/--{name}_{WORLD}.usd"),
            settings.clone(),
        ));
        let binding = format!(
            "uTank2 CDF 1.0\r\nmosstank/profiles/--{name}_{WORLD}.usd\r\nmosstank/loot/Loot0.utl\r\nmosstank/navs/route001.nav\r\nmosstank/metas/meta000.met\r\n"
        );
        files.push((format!("mosstank/profiles/{WORLD}_{name}.cdf"), binding));
    }
    let large = loot_profile(LARGE_LOOT_BYTES);
    for index in 0..LARGE_LOOT_PROFILES {
        files.push((format!("mosstank/loot/Loot{index}.utl"), large.clone()));
    }
    let small = common::csharp_fixture("vtank/loot-a.utl");
    for index in 0..SMALL_LOOT_PROFILES {
        files.push((format!("mosstank/loot/small{index}.utl"), small.clone()));
    }
    let vendor = loot_profile(UB_BYTES / UB_FILES);
    for index in 0..UB_FILES {
        files.push((
            format!("mosstank/ub/autovendor/vendor{index}.utl"),
            vendor.clone(),
        ));
    }
    Folder {
        source: "generated".to_owned(),
        files,
        character,
    }
}

//! The shared VTank profile folder: where each kind of profile lives inside
//! the plugin's `mosstank` folder, the names VTank gives a character's own
//! files, the per-character binding file (`.cdf`), and what each picker
//! lists.

use mosstank_dotnet::text::ordinal;

use crate::storage::{PluginStorage, StorageError};

/// VTank's reserved prefix: a character's own files start with it.
pub const HIDDEN_PREFIX: &str = "--";

/// The route-only reserved prefix; such routes are never listed.
pub const NAV_HIDDEN_PREFIX: &str = "~~";

/// The picker label of the character's own profile.
pub const BY_CHARACTER_LABEL: &str = "[By char]";
/// The picker label of the default settings.
pub const DEFAULT_LABEL: &str = "[Default]";
/// The picker label of no profile.
pub const NONE_LABEL: &str = "[None]";

/// A marker the world puts in front of some characters' display names; not
/// part of the name.
const DISPLAY_MARKER: char = '+';

/// The one folder the plugin keeps its files in, inside the shared folder.
pub const ROOT: &str = "mosstank";
/// Settings documents and the per-character binding.
pub const SETTINGS_FOLDER: &str = "mosstank/profiles";
/// Metas.
pub const META_FOLDER: &str = "mosstank/metas";
/// Routes.
pub const NAV_FOLDER: &str = "mosstank/navs";
/// Loot profiles.
pub const LOOT_FOLDER: &str = "mosstank/loot";
/// The old flat layout's route folder, read by the first-run copy.
pub const LEGACY_NAV_FOLDER: &str = "navs";
/// The old flat layout's meta folder.
pub const LEGACY_META_FOLDER: &str = "metas";

/// The suffix a settings file keeps when it loses a name collision.
pub const TWIN_SUFFIX: &str = ".twin-";

const LEGACY_NAV_MARKER: &str = "nav_";

/// The version line a valid `.cdf` starts with.
pub const CDF_HEADER: &str = "uTank2 CDF 1.0";

/// The one name a character's files are filed under, whatever spelling the
/// world reports: trimmed, without the display marker in front.
#[must_use]
pub fn canonical_character_key(reported_name: Option<&str>) -> String {
    match reported_name {
        Some(name) if !name.trim().is_empty() => name
            .trim()
            .trim_start_matches(DISPLAY_MARKER)
            .trim()
            .to_owned(),
        _ => String::new(),
    }
}

/// A file no picker offers: something left beside a real profile.
#[must_use]
pub fn is_side_file(bare_file_name: &str) -> bool {
    ordinal::contains_ignore_case(bare_file_name, ".bak")
        || ordinal::contains_ignore_case(bare_file_name, TWIN_SUFFIX)
        || ordinal::ends_with_ignore_case(bare_file_name, ".tmp")
        || bare_file_name.starts_with('.')
}

/// The bare file name inside `folder` (the key unchanged when it is not
/// inside it).
#[must_use]
pub fn strip_folder<'a>(key: &'a str, folder: &str) -> &'a str {
    let prefix = format!("{folder}/");
    if ordinal::starts_with_ignore_case(key, &prefix) {
        // The folder is ASCII, so a match ignoring case has its length.
        key.get(prefix.len()..).unwrap_or(key)
    } else {
        key
    }
}

/// Whether a flat-layout `.af` file is a route (it carries the `nav_`
/// marker, after the reserved prefix if any).
#[must_use]
pub fn is_legacy_flat_route_file_name(bare_file_name: &str) -> bool {
    bare_file_name.starts_with(LEGACY_NAV_MARKER)
        || bare_file_name
            .strip_prefix(HIDDEN_PREFIX)
            .is_some_and(|rest| rest.starts_with(LEGACY_NAV_MARKER))
}

/// The name without its `nav_` marker (which it must carry).
#[must_use]
pub fn strip_legacy_nav_marker(bare_file_name: &str) -> String {
    match bare_file_name.strip_prefix(HIDDEN_PREFIX) {
        Some(rest) => format!(
            "{HIDDEN_PREFIX}{}",
            rest.strip_prefix(LEGACY_NAV_MARKER).unwrap_or(rest)
        ),
        None => bare_file_name
            .strip_prefix(LEGACY_NAV_MARKER)
            .unwrap_or(bare_file_name)
            .to_owned(),
    }
}

/// Every settings document in the profiles folder, whoever owns it.
///
/// # Errors
/// The folder cannot be listed.
pub fn list_every_settings_key(
    storage: &mut dyn PluginStorage,
) -> Result<Vec<String>, StorageError> {
    Ok(folder_file_names(storage, SETTINGS_FOLDER, &[".usd"])?
        .into_iter()
        .map(|bare| format!("{SETTINGS_FOLDER}/{bare}"))
        .collect())
}

/// The file VTank names a character's own profile of one kind:
/// `--<character>_<server>.<extension>`.
#[must_use]
pub fn auto_character_file_name(character_name: &str, server: &str, extension: &str) -> String {
    format!(
        "{HIDDEN_PREFIX}{character_name}_{server}.{}",
        extension.trim_start_matches('.')
    )
}

/// The prefix of a character's named settings profiles:
/// `--<character>_<server>_`.
#[must_use]
pub fn sub_profile_prefix(character_name: &str, server: &str) -> String {
    format!("{HIDDEN_PREFIX}{character_name}_{server}_")
}

/// Whether a file belongs to another character's reserved family.
#[must_use]
pub fn is_hidden_from_other_characters(
    file_name: &str,
    character_name: &str,
    server: &str,
) -> bool {
    file_name.starts_with(HIDDEN_PREFIX)
        && !file_name.starts_with(&sub_profile_prefix(character_name, server))
}

/// The label VTank shows for one of this character's named profiles
/// (`[Char] name`), `None` for any other file.
#[must_use]
pub fn try_display_name(file_name: &str, character_name: &str, server: &str) -> Option<String> {
    let without_prefix = file_name.strip_prefix(&sub_profile_prefix(character_name, server))?;
    let suffix = without_prefix
        .rfind('.')
        .map_or(without_prefix, |dot| &without_prefix[..dot]);
    (!suffix.is_empty()).then(|| format!("[Char] {suffix}"))
}

/// One picker entry: the file's key (empty for a built-in entry) and the
/// label VTank would show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileEntry {
    /// The key, inside its folder.
    pub file_name: String,
    /// The label.
    pub display_name: String,
}

impl ProfileEntry {
    fn new(file_name: &str, display_name: &str) -> Self {
        Self {
            file_name: file_name.to_owned(),
            display_name: display_name.to_owned(),
        }
    }
}

/// The settings picker: the two built-in entries, this character's named
/// profiles, and the shared ones (only the current one when `mine_only`).
///
/// # Errors
/// The folder cannot be listed.
pub fn list_settings_profiles(
    storage: &mut dyn PluginStorage,
    character_name: &str,
    server: &str,
    mine_only: bool,
    current_file_name: Option<&str>,
) -> Result<Vec<ProfileEntry>, StorageError> {
    let mut entries = vec![
        ProfileEntry::new("", DEFAULT_LABEL),
        ProfileEntry::new("", BY_CHARACTER_LABEL),
    ];
    let current_bare_name = current_file_name.map(|name| strip_folder(name, SETTINGS_FOLDER));
    for file_name in folder_file_names(storage, SETTINGS_FOLDER, &[".usd"])? {
        let key = format!("{SETTINGS_FOLDER}/{file_name}");
        if let Some(display) = try_display_name(&file_name, character_name, server) {
            entries.push(ProfileEntry {
                file_name: key,
                display_name: display,
            });
            continue;
        }
        if file_name.starts_with(HIDDEN_PREFIX) {
            continue; // someone else's --Name_Server family.
        }
        if mine_only && current_bare_name != Some(file_name.as_str()) {
            continue;
        }
        entries.push(ProfileEntry {
            file_name: key,
            display_name: file_name,
        });
    }
    Ok(entries)
}

fn listed(
    storage: &mut dyn PluginStorage,
    folder: &str,
    extensions: &[&str],
    hidden: &[&str],
) -> Result<Vec<ProfileEntry>, StorageError> {
    let mut entries = vec![
        ProfileEntry::new("", NONE_LABEL),
        ProfileEntry::new("", BY_CHARACTER_LABEL),
    ];
    for bare_name in folder_file_names(storage, folder, extensions)? {
        if hidden.iter().any(|prefix| bare_name.starts_with(prefix)) {
            continue;
        }
        entries.push(ProfileEntry {
            file_name: format!("{folder}/{bare_name}"),
            display_name: bare_name,
        });
    }
    Ok(entries)
}

/// The route picker: `.af` and the older `.nav` form, offered as they are.
///
/// # Errors
/// The folder cannot be listed.
pub fn list_navigation_profiles(
    storage: &mut dyn PluginStorage,
) -> Result<Vec<ProfileEntry>, StorageError> {
    listed(
        storage,
        NAV_FOLDER,
        &[".af", ".nav"],
        &[HIDDEN_PREFIX, NAV_HIDDEN_PREFIX],
    )
}

/// The meta picker: `.af` and the older `.met` form.
///
/// # Errors
/// The folder cannot be listed.
pub fn list_meta_profiles(
    storage: &mut dyn PluginStorage,
) -> Result<Vec<ProfileEntry>, StorageError> {
    listed(storage, META_FOLDER, &[".af", ".met"], &[HIDDEN_PREFIX])
}

/// The loot picker.
///
/// # Errors
/// The folder cannot be listed.
pub fn list_loot_profiles(
    storage: &mut dyn PluginStorage,
) -> Result<Vec<ProfileEntry>, StorageError> {
    listed(storage, LOOT_FOLDER, &[".utl"], &[HIDDEN_PREFIX])
}

/// The character's `.ast` file name.
#[must_use]
pub fn ast_file_name(character_name: &str, server: &str) -> String {
    format!("{character_name}_{server}.ast")
}

/// The key of the character's binding file: `<server>_<character>.cdf`.
#[must_use]
pub fn cdf_file_name(character_name: &str, server: &str) -> String {
    format!("{SETTINGS_FOLDER}/{server}_{character_name}.cdf")
}

/// One name out of a binding file, put into the folder its kind lives in.
fn re_root(stored_name: &str, folder: &str) -> String {
    if stored_name.is_empty() {
        return String::new();
    }
    let bare_name = stored_name
        .rfind('/')
        .map_or(stored_name, |slash| &stored_name[slash + 1..]);
    format!("{folder}/{bare_name}")
}

/// What a character's binding file names: the settings, loot, route and
/// (optionally) meta files it last used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VtankCharacterBinding {
    /// The settings file's key.
    pub settings_file_name: String,
    /// The loot profile's key.
    pub loot_file_name: String,
    /// The route's key.
    pub nav_file_name: String,
    /// The meta's key, when the file names one.
    pub meta_file_name: Option<String>,
}

/// The character's binding, `None` when it has none or the file is not a
/// binding (or either name is blank).
///
/// # Errors
/// The file exists but cannot be read.
pub fn try_read_character_binding(
    storage: &mut dyn PluginStorage,
    character_name: &str,
    server: &str,
) -> Result<Option<VtankCharacterBinding>, StorageError> {
    if character_name.trim().is_empty() || server.trim().is_empty() || !storage.is_available() {
        return Ok(None);
    }
    let Some(text) = storage.read_text(&cdf_file_name(character_name, server))? else {
        return Ok(None);
    };
    let normalized = text.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    if lines.len() < 4 || lines[0] != CDF_HEADER {
        return Ok(None);
    }
    let settings = if ordinal::ends_with_ignore_case(lines[1], ".uts") {
        format!("{}.usd", &lines[1][..lines[1].len() - 4])
    } else {
        lines[1].to_owned()
    };
    let meta = (lines.len() >= 5 && !lines[4].is_empty()).then(|| lines[4]);
    Ok(Some(VtankCharacterBinding {
        settings_file_name: re_root(&settings, SETTINGS_FOLDER),
        loot_file_name: re_root(lines[2], LOOT_FOLDER),
        nav_file_name: re_root(lines[3], NAV_FOLDER),
        meta_file_name: meta.map(|meta| re_root(meta, META_FOLDER)),
    }))
}

/// Writes the character's binding (nothing when either name is blank or
/// the storage is not there).
///
/// # Errors
/// The file cannot be written.
pub fn write_character_binding(
    storage: &mut dyn PluginStorage,
    character_name: &str,
    server: &str,
    binding: &VtankCharacterBinding,
) -> Result<(), StorageError> {
    if character_name.trim().is_empty() || server.trim().is_empty() || !storage.is_available() {
        return Ok(());
    }
    let mut lines = vec![
        CDF_HEADER,
        binding.settings_file_name.as_str(),
        binding.loot_file_name.as_str(),
        binding.nav_file_name.as_str(),
    ];
    if let Some(meta) = binding.meta_file_name.as_deref().filter(|m| !m.is_empty()) {
        lines.push(meta);
    }
    storage.write_text(
        &cdf_file_name(character_name, server),
        &format!("{}\r\n", lines.join("\r\n")),
    )
}

/// The profiles directly inside `folder` with one of `extensions`, side
/// files left out, sorted ignoring case.
fn folder_file_names(
    storage: &mut dyn PluginStorage,
    folder: &str,
    extensions: &[&str],
) -> Result<Vec<String>, StorageError> {
    if !storage.is_available() {
        return Ok(Vec::new());
    }
    let folder_prefix = format!("{folder}/");
    let mut names: Vec<String> = storage
        .list(folder)?
        .into_iter()
        .filter_map(|key| key.strip_prefix(&folder_prefix).map(str::to_owned))
        .filter(|bare| !bare.contains('/'))
        .filter(|bare| {
            extensions
                .iter()
                .any(|extension| ordinal::ends_with_ignore_case(bare, extension))
        })
        .filter(|bare| !is_side_file(bare))
        .collect();
    names.sort_by(|a, b| ordinal::compare_ignore_case(a, b));
    Ok(names)
}

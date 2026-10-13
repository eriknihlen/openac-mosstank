//! The three files behind the UB settings catalogue, in the shared profile
//! folder under `mosstank/ub/`, and the profile files and alias lists kept
//! beside them:
//!
//! ```text
//! mosstank/ub/settings.json                          the installation
//! mosstank/ub/profiles/<name>.settings.json          a named, shareable set
//! mosstank/ub/<Server>/<Character>/settings.json     this character
//! mosstank/ub/<folder>/<file>                        a profile file, at three depths
//! ```
//!
//! Each settings file is a flat JSON map of setting name to the text its
//! value shape reads back; keys this version does not know are kept and
//! written back. A file that does not read (bad JSON, or the original
//! tool's nested document copied in) is reported, the defaults stand, and
//! it is never written over.
//!
//! The tiers are read when the store is bound to a character (and the
//! profile tier when a profile is chosen), and a changed tier reaches its
//! file at [`UbSettingStore::flush`], which the plugin calls before its
//! call returns (register row STORE-1).

use std::collections::HashMap;

use mosstank_dotnet::collections::IgnoreCaseStringMap;
use mosstank_dotnet::text::{casing, ordinal};
use mosstank_settings::ub::{UbSettingScope, UbSettingValue, UbSettingValues, definitions};
use openac_plugin_compat::json::{self, JsonValue, system_text_json};

use crate::layout;
use crate::storage::{PluginStorage, StorageError, StoreLog};

/// Everything this store writes lives under this prefix, inside the shared
/// profile folder.
pub const ROOT: &str = "mosstank/ub/";

/// The profile opened when nobody has chosen one.
pub const DEFAULT_PROFILE_NAME: &str = "default";

/// The key the chosen profile name is kept under, in the character's file.
const PROFILE_NAME_KEY: &str = "$profile";

const DEFAULT_PROFILE_FILE_NAME: &str = "default.utl";

/// The alias profile value that means the character's own file.
pub const CHARACTER_ALIAS_PROFILE: &str = "[character]";

const ALIAS_FILE_NAME: &str = "aliases.txt";

#[derive(Debug, Clone, Default)]
struct Tier {
    /// Name to text, names in any case; a JSON null stays null.
    values: IgnoreCaseStringMap<Option<String>>,
    loaded: bool,
    /// The file did not read, so it is not written over.
    read_only: bool,
    /// Changed since it was last saved.
    dirty: bool,
}

impl Tier {
    fn forget(&mut self) {
        self.loaded = false;
        self.read_only = false;
        self.dirty = false;
        self.values.clear();
    }
}

fn index(scope: UbSettingScope) -> usize {
    match scope {
        UbSettingScope::Global => 0,
        UbSettingScope::Profile => 1,
        UbSettingScope::Character => 2,
    }
}

const SCOPES: [UbSettingScope; 3] = [
    UbSettingScope::Global,
    UbSettingScope::Profile,
    UbSettingScope::Character,
];

/// The UB settings files and the profile files beside them.
#[derive(Debug, Clone)]
pub struct UbSettingStore {
    tiers: [Tier; 3],
    /// Plain-line files read once and served from here until the
    /// character changes.
    line_files: HashMap<String, Vec<String>>,
    server: String,
    character: String,
    profile: String,
    storage_available: bool,
}

impl Default for UbSettingStore {
    fn default() -> Self {
        Self::new()
    }
}

impl UbSettingStore {
    /// A store bound to nobody, nothing read yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tiers: Default::default(),
            line_files: HashMap::new(),
            server: String::new(),
            character: String::new(),
            profile: DEFAULT_PROFILE_NAME.to_owned(),
            storage_available: false,
        }
    }

    /// The named profile in use.
    #[must_use]
    pub fn profile_name(&self) -> &str {
        &self.profile
    }

    /// Reads every tier that has a file and has not been read yet.
    pub fn load(&mut self, storage: &mut dyn PluginStorage, log: &mut dyn StoreLog) {
        self.storage_available = storage.is_available();
        for scope in SCOPES {
            self.load_tier(scope, storage, log);
        }
    }

    /// Binds the character tier to a server and character; called again on
    /// every login.
    pub fn bind(
        &mut self,
        storage: &mut dyn PluginStorage,
        log: &mut dyn StoreLog,
        server: Option<&str>,
        character: Option<&str>,
    ) -> &mut Self {
        let new_server = layout::safe(server);
        let new_character = layout::safe(character);
        if new_server == self.server && new_character == self.character {
            self.load(storage, log);
            return self;
        }
        self.server = new_server;
        self.character = new_character;
        self.tiers[index(UbSettingScope::Character)].forget();
        self.line_files.clear();
        // Which profile this character opens is its own choice, so it is
        // known only once the character is.
        self.load_tier(UbSettingScope::Character, storage, log);
        self.profile = match self.read_raw(UbSettingScope::Character, PROFILE_NAME_KEY) {
            Some(chosen) if !chosen.is_empty() => chosen.to_owned(),
            _ => DEFAULT_PROFILE_NAME.to_owned(),
        };
        self.tiers[index(UbSettingScope::Profile)].forget();
        self.load(storage, log);
        self
    }

    /// Opens a named profile and remembers the choice for this character.
    pub fn select_profile(
        &mut self,
        storage: &mut dyn PluginStorage,
        log: &mut dyn StoreLog,
        name: &str,
    ) {
        let mut chosen = layout::safe(Some(name));
        if chosen.is_empty() {
            chosen = DEFAULT_PROFILE_NAME.to_owned();
        }
        if chosen == self.profile {
            return;
        }
        self.profile = chosen.clone();
        self.tiers[index(UbSettingScope::Profile)].forget();
        self.load(storage, log);
        self.write_raw(UbSettingScope::Character, PROFILE_NAME_KEY, &chosen);
        self.flush(storage, log);
    }

    /// Makes a named profile and opens it; the file is written even empty,
    /// so the profile can be listed. With `copy_current` the open profile's
    /// values go into it. The notice is one line for whoever pressed the
    /// button; false when the name is blank or taken.
    ///
    /// # Errors
    /// The profiles folder cannot be listed.
    pub fn create_profile(
        &mut self,
        storage: &mut dyn PluginStorage,
        log: &mut dyn StoreLog,
        name: &str,
        copy_current: bool,
    ) -> Result<(bool, String), StorageError> {
        let chosen = layout::safe(Some(name));
        if chosen.is_empty() {
            return Ok((false, "Type a name for the new profile first.".to_owned()));
        }
        if self
            .available_profiles(storage)?
            .iter()
            .any(|known| ordinal::equals_ignore_case(known, &chosen))
        {
            return Ok((
                false,
                format!("A profile called '{chosen}' already exists."),
            ));
        }
        // What is copied is read before the tier is pointed elsewhere.
        self.load(storage, log);
        let carried = if copy_current {
            self.tiers[index(UbSettingScope::Profile)].values.clone()
        } else {
            IgnoreCaseStringMap::new()
        };
        self.profile = chosen.clone();
        let tier = &mut self.tiers[index(UbSettingScope::Profile)];
        tier.forget();
        tier.loaded = true;
        for (key, text) in carried.iter() {
            tier.values.insert(key.clone(), text.clone());
        }
        tier.dirty = true;
        self.write_raw(UbSettingScope::Character, PROFILE_NAME_KEY, &chosen);
        self.flush(storage, log);
        let notice = if copy_current {
            format!("Profile '{chosen}' is open, holding a copy of what the last one held.")
        } else {
            format!("Profile '{chosen}' is open.")
        };
        Ok((true, notice))
    }

    /// The named profiles already written, in name order (ignoring case),
    /// `default` always among them.
    ///
    /// # Errors
    /// The profiles folder cannot be listed.
    pub fn available_profiles(
        &mut self,
        storage: &mut dyn PluginStorage,
    ) -> Result<Vec<String>, StorageError> {
        const SUFFIX: &str = ".settings.json";
        let mut names = vec![DEFAULT_PROFILE_NAME.to_owned()];
        if storage.is_available() {
            for key in storage.list(&format!("{ROOT}profiles"))? {
                let file = key
                    .rfind('/')
                    .map_or(key.as_str(), |slash| &key[slash + 1..]);
                if !ordinal::ends_with_ignore_case(file, SUFFIX) {
                    continue;
                }
                let name = &file[..file.len() - SUFFIX.len()];
                if !name.is_empty() && !names.iter().any(|n| ordinal::equals_ignore_case(n, name)) {
                    names.push(name.to_owned());
                }
            }
        }
        names.sort_by(|a, b| ordinal::compare_ignore_case(a, b));
        Ok(names)
    }

    /// Where a profile file is, looked for in this character's folder, this
    /// server's, the installation's, and then the same three for
    /// `default.utl`; where a new one would go when none exists.
    ///
    /// # Errors
    /// A candidate exists but cannot be read.
    pub fn resolve_profile_key(
        &self,
        storage: &mut dyn PluginStorage,
        folder: &str,
        file_name: &str,
    ) -> Result<String, StorageError> {
        let safe_folder = layout::safe(Some(folder));
        let safe_file = layout::safe(Some(file_name));
        let own = self.character_folder();
        let character = if own.is_empty() {
            String::new()
        } else {
            format!("{own}{safe_folder}/")
        };
        let server = if self.server.is_empty() {
            String::new()
        } else {
            format!("{ROOT}{}/{safe_folder}/", self.server)
        };
        let plugin = format!("{ROOT}{safe_folder}/");
        let candidates = [
            (!character.is_empty()).then(|| format!("{character}{safe_file}")),
            (!server.is_empty()).then(|| format!("{server}{safe_file}")),
            Some(format!("{plugin}{safe_file}")),
            (!character.is_empty()).then(|| format!("{character}{DEFAULT_PROFILE_FILE_NAME}")),
            (!server.is_empty()).then(|| format!("{server}{DEFAULT_PROFILE_FILE_NAME}")),
            Some(format!("{plugin}{DEFAULT_PROFILE_FILE_NAME}")),
        ];
        for candidate in candidates.into_iter().flatten() {
            if exists(storage, &candidate)? {
                return Ok(candidate);
            }
        }
        Ok(format!("{plugin}{safe_file}"))
    }

    /// Where a profile file of this character's own goes: the character's
    /// folder when one is bound, otherwise the installation's.
    #[must_use]
    pub fn character_profile_key(&self, folder: &str, file_name: &str) -> String {
        let safe_folder = layout::safe(Some(folder));
        let safe_file = layout::safe(Some(file_name));
        let character = self.character_folder();
        if character.is_empty() {
            format!("{ROOT}{safe_folder}/{safe_file}")
        } else {
            format!("{character}{safe_folder}/{safe_file}")
        }
    }

    /// The folders a profile file is looked for in, in lookup order, each
    /// with a trailing slash; only the ones that can be named.
    #[must_use]
    pub fn profile_folder_prefixes(&self, folder: &str) -> Vec<String> {
        let safe_folder = layout::safe(Some(folder));
        let mut prefixes = Vec::with_capacity(3);
        let character = self.character_folder();
        if !character.is_empty() {
            prefixes.push(format!("{character}{safe_folder}/"));
        }
        if !self.server.is_empty() {
            prefixes.push(format!("{ROOT}{}/{safe_folder}/", self.server));
        }
        prefixes.push(format!("{ROOT}{safe_folder}/"));
        prefixes
    }

    /// Where the alias list of one alias profile is: the character's own
    /// file for `[character]` (or a blank name), `profiles/<name>.aliases.txt`
    /// for a named one; `None` for the character's file before a character
    /// is known.
    #[must_use]
    pub fn alias_file_key(&self, profile: Option<&str>) -> Option<String> {
        let name = layout::safe(profile);
        if name.is_empty() || ordinal::equals_ignore_case(&name, CHARACTER_ALIAS_PROFILE) {
            let folder = self.character_folder();
            return (!folder.is_empty()).then(|| format!("{folder}{ALIAS_FILE_NAME}"));
        }
        Some(format!("{ROOT}profiles/{name}.{ALIAS_FILE_NAME}"))
    }

    /// The lines of one file, trimmed, blank ones left out; empty when there
    /// is no such file, no key or nowhere to read from.
    pub fn read_lines(
        &mut self,
        storage: &mut dyn PluginStorage,
        log: &mut dyn StoreLog,
        key: Option<&str>,
    ) -> Vec<String> {
        let Some(key) = key else {
            return Vec::new();
        };
        if let Some(cached) = self.line_files.get(key) {
            return cached.clone();
        }
        let mut lines = Vec::new();
        if storage.is_available() {
            match storage.read_text(key) {
                Ok(Some(text)) => {
                    for line in text.split('\n') {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            lines.push(trimmed.to_owned());
                        }
                    }
                }
                Ok(None) => {}
                Err(error) => log.warn(&format!(
                    "MossTank could not read {key} ({error}); it is treated as empty."
                )),
            }
        }
        self.line_files.insert(key.to_owned(), lines.clone());
        lines
    }

    /// Writes one file of lines, one a line. False when there is no key or
    /// nowhere to write; the lines are kept for this session either way.
    pub fn write_lines<S: AsRef<str>>(
        &mut self,
        storage: &mut dyn PluginStorage,
        log: &mut dyn StoreLog,
        key: Option<&str>,
        lines: &[S],
    ) -> bool {
        let Some(key) = key else {
            return false;
        };
        let kept: Vec<String> = lines
            .iter()
            .map(|line| line.as_ref().trim().to_owned())
            .filter(|line| !line.is_empty())
            .collect();
        self.line_files.insert(key.to_owned(), kept.clone());
        if !storage.is_available() {
            return false;
        }
        let text: String = kept.iter().map(|line| format!("{line}\n")).collect();
        match storage.write_text(key, &text) {
            Ok(()) => true,
            Err(error) => {
                log.warn(&format!("MossTank could not save {key}: {error}"));
                false
            }
        }
    }

    /// Saves every tier changed since it was last saved.
    pub fn flush(&mut self, storage: &mut dyn PluginStorage, log: &mut dyn StoreLog) {
        for scope in SCOPES {
            if self.tiers[index(scope)].dirty {
                self.save(scope, storage, log);
            }
        }
    }

    /// Whether a tier has changes not yet saved.
    #[must_use]
    pub fn has_unsaved_changes(&self) -> bool {
        self.tiers.iter().any(|tier| tier.dirty)
    }

    fn read_raw(&self, scope: UbSettingScope, name: &str) -> Option<&str> {
        self.tiers[index(scope)]
            .values
            .get(name)
            .and_then(|text| text.as_deref())
    }

    fn write_raw(&mut self, scope: UbSettingScope, name: &str, text: &str) {
        let tier = &mut self.tiers[index(scope)];
        tier.values.insert(name.to_owned(), Some(text.to_owned()));
        tier.dirty = true;
    }

    fn load_tier(
        &mut self,
        scope: UbSettingScope,
        storage: &mut dyn PluginStorage,
        log: &mut dyn StoreLog,
    ) {
        let key = self.key_of(scope);
        let tier = &mut self.tiers[index(scope)];
        if tier.loaded {
            return;
        }
        tier.loaded = true;
        tier.values.clear();
        let Some(key) = key else {
            return;
        };
        if !storage.is_available() {
            return;
        }
        let text = match storage.read_text(&key) {
            Ok(text) => text,
            Err(error) => {
                // Nothing is known about what is in the file, so it is not
                // written over either.
                log.warn(&format!(
                    "MossTank could not read {key} ({error}); its settings are at their defaults and it will not be written until it can be read."
                ));
                tier.read_only = true;
                return;
            }
        };
        let Some(text) = text.filter(|t| !casing::is_white_space_only(t)) else {
            return;
        };
        match read_document(&text) {
            Ok(values) => {
                for (name, value) in values {
                    tier.values.insert(name, value);
                }
            }
            Err(error) => {
                // The file stays as it is: a person can repair it.
                log.warn(&format!(
                    "MossTank could not read {key} ({error}); its settings are at their defaults until the file is repaired."
                ));
                tier.read_only = true;
            }
        }
    }

    fn save(
        &mut self,
        scope: UbSettingScope,
        storage: &mut dyn PluginStorage,
        log: &mut dyn StoreLog,
    ) {
        let key = self.key_of(scope);
        let tier = &mut self.tiers[index(scope)];
        tier.dirty = false;
        let Some(key) = key else {
            return;
        };
        // A tier never read is not written: what its file holds is unknown.
        if !tier.loaded || tier.read_only || !storage.is_available() {
            return;
        }
        let mut entries: Vec<(&String, &Option<String>)> = tier.values.iter().collect();
        entries.sort_by(|a, b| ordinal::compare(a.0, b.0));
        let document = JsonValue::object(entries.into_iter().map(|(name, value)| {
            (
                name.clone(),
                value
                    .as_ref()
                    .map_or(JsonValue::Null, |text| JsonValue::String(text.clone())),
            )
        }));
        let text = system_text_json::to_string(&document, &system_text_json::Options::INDENTED)
            .expect("a map of strings always serializes");
        if let Err(error) = storage.write_text(&key, &text) {
            log.warn(&format!("MossTank could not save {key}: {error}"));
        }
    }

    /// The file one tier lives in; `None` for the character tier before
    /// login.
    fn key_of(&self, scope: UbSettingScope) -> Option<String> {
        match scope {
            UbSettingScope::Global => Some(format!("{ROOT}settings.json")),
            UbSettingScope::Profile => {
                Some(format!("{ROOT}profiles/{}.settings.json", self.profile))
            }
            UbSettingScope::Character => {
                let folder = self.character_folder();
                (!folder.is_empty()).then(|| format!("{folder}settings.json"))
            }
        }
    }

    fn character_folder(&self) -> String {
        if self.server.is_empty() || self.character.is_empty() {
            String::new()
        } else {
            format!("{ROOT}{}/{}/", self.server, self.character)
        }
    }
}

fn exists(storage: &mut dyn PluginStorage, key: &str) -> Result<bool, StorageError> {
    Ok(storage.is_available() && storage.read_text(key)?.is_some())
}

/// A settings file as `JsonSerializer.Deserialize<Dictionary<string,
/// string>>` reads it: an object of strings (or nulls), a later duplicate
/// replacing an earlier one; `null` is an empty map.
fn read_document(text: &str) -> Result<Vec<(String, Option<String>)>, String> {
    let document = json::parse(text).map_err(|error| error.to_string())?;
    match document {
        JsonValue::Null => Ok(Vec::new()),
        JsonValue::Object(members) => {
            let mut values: Vec<(String, Option<String>)> = Vec::new();
            let mut seen: HashMap<String, usize> = HashMap::new();
            for (name, value) in members {
                let value = match value {
                    JsonValue::String(text) => Some(text),
                    JsonValue::Null => None,
                    other => {
                        return Err(format!(
                            "The JSON value could not be converted to System.String. Path: $.{name} ({})",
                            kind_name(&other)
                        ));
                    }
                };
                match seen.get(&name) {
                    Some(&at) => values[at].1 = value,
                    None => {
                        seen.insert(name.clone(), values.len());
                        values.push((name, value));
                    }
                }
            }
            Ok(values)
        }
        other => Err(format!(
            "The JSON value could not be converted to System.Collections.Generic.Dictionary`2[System.String,System.String]. Path: $ ({})",
            kind_name(&other)
        )),
    }
}

fn kind_name(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "a boolean",
        JsonValue::Number(_) => "a number",
        JsonValue::String(_) => "a string",
        JsonValue::Array(_) => "an array",
        JsonValue::Object(_) => "an object",
    }
}

impl UbSettingValues for UbSettingStore {
    fn read(&self, scope: UbSettingScope, name: &str) -> Option<UbSettingValue> {
        if name.starts_with('$') {
            return None;
        }
        let text = self.read_raw(scope, name)?;
        UbSettingValue::try_parse(definitions::kind_of(name), text)
    }

    fn write(&mut self, scope: UbSettingScope, name: &str, value: UbSettingValue) {
        self.write_raw(scope, name, &value.to_storage_string());
    }

    fn clear(&mut self, scope: UbSettingScope, name: &str) -> bool {
        let tier = &mut self.tiers[index(scope)];
        if tier.values.remove(name).is_none() {
            return false;
        }
        tier.dirty = true;
        true
    }

    fn is_writable(&self, scope: UbSettingScope) -> bool {
        self.key_of(scope).is_some()
            && self.storage_available
            && !self.tiers[index(scope)].read_only
    }
}

//! The settings profile store: which `.usd` in `mosstank/profiles/` is this
//! character's, the side-car beside it in the plugin's own storage, the
//! character's binding file, creating, deleting and saving profiles, the
//! one-time migration of older MossTank JSON profiles, and `/vt setinall`.
//!
//! Every call that reaches storage is handed the [`StoreHost`]; an error
//! the C# would let escape as an exception comes back as `Err`.

use mosstank_dotnet::collections::{IgnoreCaseStringSet, OrdinalStringSet};
use mosstank_dotnet::text::{casing, ordinal};
use mosstank_formats::usd::{VtankCell, VtankDatabase, VtankRow};
use mosstank_settings::items::assist_items;
use mosstank_settings::model::AllSettings;
use mosstank_settings::{defaults, options, profile};

use crate::directory::{self, SETTINGS_FOLDER, VtankCharacterBinding};
use crate::layout;
use crate::legacy::{
    LegacyProfileDocument, LegacyRosterDocument, PreferencesDocument, meta_interval,
};
use crate::recovery::{self, sha256_hex};
use crate::sidecar::{DynamicSettingDocument, SideCarDocument};
use crate::storage::{Root, StoreHost};

/// The built-in character profile's name in the picker.
pub const BY_CHARACTER: &str = "By char";

const OLD_SINGLE_PROFILE_KEY: &str = "profile.json";
const LEGACY_ROSTER_KEY: &str = "profiles/index.json";
const PREFERENCES_KEY: &str = "profiles/macro/preferences.json";
const SIDE_CAR_DIRECTORY: &str = "profiles/macro/sidecar/";

/// The longest automatic game-database checks may be apart, in hours.
pub const MAXIMUM_GAME_DB_CHECK_INTERVAL_HOURS: i32 = 168;

/// An error the C# would let escape from the store as an exception: a
/// storage failure, or a settings document that would not save.
pub type StoreError = String;

/// What happened when a store was asked for the selected profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileLoad {
    /// The selected profile was read and applied.
    Loaded,
    /// Every complete rule before an incomplete tail was applied; the
    /// source stays write-protected until it reads completely.
    Partial,
    /// The selected profile has never been written; writing the current
    /// settings out under its name is the first-run behaviour.
    Missing,
    /// The selected profile exists but could not be read; nothing was
    /// applied and the file is left as it is.
    Failed,
}

/// The settings profile store (`MossTankProfileStore`).
#[derive(Debug, Clone)]
pub struct ProfileStore {
    preferences: PreferencesDocument,
    character_name: String,
    selected: String,
    pending_legacy_bare_name: Option<String>,
    current_database: Option<VtankDatabase>,
    current_database_file_name: Option<String>,
    active_file_name: Option<String>,
    has_active_profile: bool,
    roster_swept: bool,
    server: String,
    recovery_notice: Option<String>,
    load_failure_notice: Option<String>,
    save_failure_notice: Option<String>,
    last_load_key: Option<String>,
}

fn to_key(bare_file_name: &str) -> String {
    format!("{SETTINGS_FOLDER}/{bare_file_name}")
}

/// The name the picker and commands use: the file name without its folder
/// (`By char` as it is).
fn strip_folder(key: &str) -> String {
    if ordinal::equals_ignore_case(key, BY_CHARACTER) {
        key.to_owned()
    } else {
        directory::strip_folder(key, SETTINGS_FOLDER).to_owned()
    }
}

fn to_file_name(bare_name: &str) -> String {
    if ordinal::ends_with_ignore_case(bare_name, ".usd") {
        bare_name.to_owned()
    } else {
        format!("{bare_name}.usd")
    }
}

fn normalize_name(name: Option<&str>) -> String {
    name.map(str::trim).unwrap_or_default().to_owned()
}

/// The key an old JSON profile was saved under.
fn legacy_profile_key(value: &str, by_character: bool) -> String {
    let identity = format!(
        "{}{}",
        if by_character { "char:" } else { "named:" },
        casing::to_upper_invariant(value.trim())
    );
    format!("profiles/macro/{}.json", sha256_hex(&identity))
}

/// The side-car's key, built from the settings file's own name, never its
/// folder.
fn side_car_key(file_name: &str) -> String {
    format!(
        "{SIDE_CAR_DIRECTORY}{}.json",
        layout::portable_file_name(&strip_folder(file_name))
    )
}

fn valid_named_profile(name: &str) -> Result<(), String> {
    let length = openac_plugin_compat::text::utf16::len(name);
    if !(1..=64).contains(&length) {
        return Err("Enter a profile name (1-64 characters).".to_owned());
    }
    if ordinal::equals_ignore_case(name, BY_CHARACTER) {
        return Err("'By char' is the built-in character profile.".to_owned());
    }
    Ok(())
}

/// The defaults document with the live settings seeded from it (the same
/// reader a load uses, so every table the previous profile filled is
/// dropped).
fn seed_from_defaults(settings: &mut AllSettings) -> VtankDatabase {
    let database = defaults::create();
    profile::apply_seed_database(&database, settings)
        .expect("the default document has its Settings table");
    database
}

fn read_json<T>(
    host: &mut dyn StoreHost,
    recovery_notice: &mut Option<String>,
    key: &str,
    parse: impl Fn(&str) -> Result<Option<T>, String>,
) -> Option<T> {
    let storage = host.storage(Root::Plugin);
    if !storage.is_available() {
        return None;
    }
    let text = match storage.read_text(key) {
        Ok(text) => text,
        Err(error) => {
            let notice = recovery::preserve(host, "macro", key, None, &error);
            host.log().warn(&notice);
            *recovery_notice = Some(notice);
            return None;
        }
    };
    let text = text.filter(|text| !casing::is_white_space_only(text))?;
    match parse(&text) {
        Ok(document) => document,
        Err(error) => {
            let notice = recovery::preserve(host, "macro", key, Some(&text), &error);
            host.log().warn(&notice);
            *recovery_notice = Some(notice);
            None
        }
    }
}

impl ProfileStore {
    /// The store, its preferences read (or the old roster's, or defaults).
    pub fn new(host: &mut dyn StoreHost) -> Self {
        let mut recovery_notice = None;
        let preferences = read_json(
            host,
            &mut recovery_notice,
            PREFERENCES_KEY,
            PreferencesDocument::read,
        )
        .or_else(|| {
            read_json(
                host,
                &mut recovery_notice,
                LEGACY_ROSTER_KEY,
                PreferencesDocument::read,
            )
        })
        .unwrap_or_default();
        Self {
            preferences,
            character_name: String::new(),
            selected: BY_CHARACTER.to_owned(),
            pending_legacy_bare_name: None,
            current_database: None,
            current_database_file_name: None,
            active_file_name: None,
            has_active_profile: false,
            roster_swept: false,
            server: String::new(),
            recovery_notice,
            load_failure_notice: None,
            save_failure_notice: None,
            last_load_key: None,
        }
    }

    /// The selected profile's name.
    #[must_use]
    pub fn selected(&self) -> String {
        strip_folder(&self.selected)
    }

    /// Whether the picker shows only this character's profiles.
    #[must_use]
    pub fn mine_only(&self) -> bool {
        self.preferences.mine_only
    }

    /// How often the meta is looked at, held in the engine's range.
    #[must_use]
    pub fn meta_interval_milliseconds(&self) -> i32 {
        self.preferences.meta_interval_milliseconds.clamp(
            meta_interval::MINIMUM_MILLISECONDS,
            meta_interval::MAXIMUM_MILLISECONDS,
        )
    }

    /// Sets how often the meta is looked at, and saves the preference.
    pub fn set_meta_interval_milliseconds(&mut self, host: &mut dyn StoreHost, milliseconds: i32) {
        self.preferences.meta_interval_milliseconds = milliseconds.clamp(
            meta_interval::MINIMUM_MILLISECONDS,
            meta_interval::MAXIMUM_MILLISECONDS,
        );
        self.save_preferences(host);
    }

    /// How long a game database stays fresh, in hours (0: every login).
    #[must_use]
    pub fn game_db_check_interval_hours(&self) -> i32 {
        self.preferences
            .game_db_check_interval_hours
            .clamp(0, MAXIMUM_GAME_DB_CHECK_INTERVAL_HOURS)
    }

    /// Sets it, and saves the preference.
    pub fn set_game_db_check_interval_hours(&mut self, host: &mut dyn StoreHost, hours: i32) {
        self.preferences.game_db_check_interval_hours =
            hours.clamp(0, MAXIMUM_GAME_DB_CHECK_INTERVAL_HOURS);
        self.save_preferences(host);
    }

    /// What was said when a document had to be preserved.
    #[must_use]
    pub fn recovery_notice(&self) -> Option<&str> {
        self.recovery_notice.as_deref()
    }

    /// What was said when the selected profile would not read.
    #[must_use]
    pub fn load_failure_notice(&self) -> Option<&str> {
        self.load_failure_notice.as_deref()
    }

    /// Why the last save failed.
    #[must_use]
    pub fn save_failure_notice(&self) -> Option<&str> {
        self.save_failure_notice.as_deref()
    }

    /// Whether a complete set of settings is in force.
    #[must_use]
    pub fn has_active_profile(&self) -> bool {
        self.has_active_profile
    }

    /// The file the last load read, or tried to read.
    #[must_use]
    pub fn last_load_key(&self) -> Option<&str> {
        self.last_load_key.as_deref()
    }

    /// Whether a character has been named: until then there is nowhere real
    /// to write.
    #[must_use]
    pub fn has_character_name(&self) -> bool {
        !self.character_name.is_empty()
    }

    fn can_bind_files(&self) -> bool {
        !self.character_name.is_empty() && !self.server.is_empty()
    }

    /// The picker's names: `By char`, then the settings files.
    ///
    /// # Errors
    /// The profiles folder cannot be listed.
    pub fn available_names(&self, host: &mut dyn StoreHost) -> Result<Vec<String>, StoreError> {
        let current = self.current_file_name();
        let mut names = vec![BY_CHARACTER.to_owned()];
        for entry in directory::list_settings_profiles(
            host.storage(Root::Profiles),
            &self.character_name,
            &self.server,
            self.preferences.mine_only,
            Some(&current),
        )? {
            if entry.file_name.is_empty() {
                continue;
            }
            names.push(strip_folder(&entry.file_name));
        }
        Ok(names)
    }

    /// Binds the store to a character on the current world; a partial
    /// identity never replaces an established one. Whether it changed.
    ///
    /// # Errors
    /// The binding file exists but cannot be read.
    pub fn bind_character(
        &mut self,
        host: &mut dyn StoreHost,
        character_name: Option<&str>,
    ) -> Result<bool, StoreError> {
        let normalized = directory::canonical_character_key(character_name);
        let server = host.world_name();
        if normalized.is_empty() || (server.is_empty() && !self.server.is_empty()) {
            return Ok(false);
        }
        if ordinal::equals_ignore_case(&normalized, &self.character_name)
            && ordinal::equals_ignore_case(&server, &self.server)
        {
            return Ok(false);
        }
        self.character_name = normalized;
        self.server = server;
        self.pending_legacy_bare_name = None;
        self.current_database = None;
        self.current_database_file_name = None;
        self.active_file_name = None;
        self.has_active_profile = false;
        let binding = if self.can_bind_files() {
            directory::try_read_character_binding(
                host.storage(Root::Profiles),
                &self.character_name,
                &self.server,
            )?
        } else {
            None
        };
        self.selected = match binding {
            Some(bound) if !bound.settings_file_name.is_empty() => bound.settings_file_name,
            _ => BY_CHARACTER.to_owned(),
        };
        Ok(true)
    }

    /// Shows only this character's profiles, or every one; a selection the
    /// picker no longer offers goes back to `By char`.
    ///
    /// # Errors
    /// The profiles folder cannot be listed.
    pub fn set_mine_only(
        &mut self,
        host: &mut dyn StoreHost,
        value: bool,
    ) -> Result<(), StoreError> {
        if self.preferences.mine_only == value {
            return Ok(());
        }
        self.preferences.mine_only = value;
        let selected = strip_folder(&self.selected);
        if !self.available_names(host)?.contains(&selected) {
            self.selected = BY_CHARACTER.to_owned();
        }
        self.save_preferences(host);
        Ok(())
    }

    fn find_listed(
        &self,
        host: &mut dyn StoreHost,
        normalized: &str,
    ) -> Result<Option<String>, StoreError> {
        let names = self.available_names(host)?;
        Ok(names
            .iter()
            .find(|candidate| *candidate == normalized)
            .or_else(|| {
                names
                    .iter()
                    .find(|candidate| ordinal::equals_ignore_case(candidate, normalized))
            })
            .cloned())
    }

    fn sub_profile_key(&self, normalized: &str) -> String {
        to_key(&format!(
            "{}{normalized}.usd",
            directory::sub_profile_prefix(&self.character_name, &self.server)
        ))
    }

    fn profiles_have(host: &mut dyn StoreHost, key: &str) -> Result<bool, StoreError> {
        let storage = host.storage(Root::Profiles);
        Ok(storage.is_available() && storage.read_text(key)?.is_some())
    }

    fn plugin_has(host: &mut dyn StoreHost, key: &str) -> Result<bool, StoreError> {
        let storage = host.storage(Root::Plugin);
        Ok(storage.is_available() && storage.read_text(key)?.is_some())
    }

    /// Selects a profile by name: a listed one, this character's named one,
    /// any file of that name in the folder, or an old JSON profile the next
    /// load migrates. Whether there was one.
    ///
    /// # Errors
    /// A storage failure.
    pub fn select(
        &mut self,
        host: &mut dyn StoreHost,
        name: Option<&str>,
    ) -> Result<bool, StoreError> {
        let normalized = normalize_name(name);
        if normalized.is_empty() {
            return Ok(false);
        }
        if ordinal::equals_ignore_case(&normalized, BY_CHARACTER) {
            self.selected = BY_CHARACTER.to_owned();
            self.pending_legacy_bare_name = None;
            return Ok(true);
        }
        if let Some(existing) = self.find_listed(host, &normalized)? {
            self.selected = to_key(&existing);
            self.pending_legacy_bare_name = None;
            return Ok(true);
        }
        let sub_profile = self.sub_profile_key(&normalized);
        if Self::profiles_have(host, &sub_profile)? {
            self.selected = sub_profile;
            self.pending_legacy_bare_name = None;
            return Ok(true);
        }
        if let Some(named) = self.try_resolve_profile_file_name(host, &normalized)? {
            self.selected = named;
            self.pending_legacy_bare_name = None;
            return Ok(true);
        }
        if Self::plugin_has(host, &legacy_profile_key(&normalized, false))? {
            // Not on disk as a .usd yet: the next load's migration writes it
            // at this sub-profile name.
            self.selected = sub_profile;
            self.pending_legacy_bare_name = Some(normalized);
            return Ok(true);
        }
        Ok(false)
    }

    /// Whether a profile of that name exists, by the same rules.
    ///
    /// # Errors
    /// A storage failure.
    pub fn exists(&self, host: &mut dyn StoreHost, name: Option<&str>) -> Result<bool, StoreError> {
        let normalized = normalize_name(name);
        if normalized.is_empty() {
            return Ok(false);
        }
        if ordinal::equals_ignore_case(&normalized, BY_CHARACTER) {
            return Ok(true);
        }
        if self.find_listed(host, &normalized)?.is_some() {
            return Ok(true);
        }
        if Self::profiles_have(host, &self.sub_profile_key(&normalized))? {
            return Ok(true);
        }
        if self
            .try_resolve_profile_file_name(host, &normalized)?
            .is_some()
        {
            return Ok(true);
        }
        Self::plugin_has(host, &legacy_profile_key(&normalized, false))
    }

    /// A profile name resolved against the folder itself rather than the
    /// picker: "mine only" decides what is shown, never what exists; another
    /// character's reserved family stays invisible.
    fn try_resolve_profile_file_name(
        &self,
        host: &mut dyn StoreHost,
        normalized: &str,
    ) -> Result<Option<String>, StoreError> {
        if !host.storage(Root::Profiles).is_available() {
            return Ok(None);
        }
        let file_name = to_file_name(normalized);
        if directory::is_hidden_from_other_characters(
            &file_name,
            &self.character_name,
            &self.server,
        ) {
            return Ok(None);
        }
        let key = to_key(&file_name);
        Ok(Self::profiles_have(host, &key)?.then_some(key))
    }

    /// Makes a named profile of this character's and selects it: a copy of
    /// the whole current document, or the defaults. The notice says what
    /// happened; false when refused.
    ///
    /// # Errors
    /// A storage failure, or the settings would not save into the document.
    pub fn create(
        &mut self,
        host: &mut dyn StoreHost,
        name: Option<&str>,
        copy_current: bool,
        settings: &mut AllSettings,
        no_buff_item_names: &mut OrdinalStringSet,
        log_channels: &mut IgnoreCaseStringSet,
    ) -> Result<(bool, String), StoreError> {
        let normalized = normalize_name(name);
        if let Err(notice) = valid_named_profile(&normalized) {
            return Ok((false, notice));
        }
        if copy_current
            && !self.has_active_profile
            && self
                .read_usd_text(host, &self.current_file_name())?
                .is_some()
        {
            return Ok((
                false,
                "Cannot copy settings because no complete active profile is available.".to_owned(),
            ));
        }
        let file_name = self.sub_profile_key(&normalized);
        if copy_current && let Err(notice) = self.can_replace_existing(host, &file_name)? {
            return Ok((false, notice));
        }
        let (database, sidecar) = if copy_current {
            // The whole document: imported tables and custom cells are
            // profile data.
            let database = match &self.current_database {
                None => profile::create_new(settings).map_err(|error| error.0)?,
                Some(current) => {
                    let mut copy =
                        VtankDatabase::parse(&current.render()).map_err(|error| error.0)?;
                    profile::save(&mut copy, settings).map_err(|error| error.0)?;
                    copy
                }
            };
            let sidecar = SideCarDocument::capture(settings, no_buff_item_names, log_channels);
            (database, sidecar)
        } else {
            let database = seed_from_defaults(settings);
            let sidecar = SideCarDocument::create_defaults();
            let mut warnings = Vec::new();
            sidecar.apply(settings, no_buff_item_names, log_channels, &mut |line| {
                warnings.push(line.to_owned());
            });
            for warning in warnings {
                host.log().warn(&warning);
            }
            assist_items::apply(&mut settings.combat);
            (database, sidecar)
        };
        self.write_usd_text(host, &file_name, &database.render());
        self.write_json(host, &side_car_key(&file_name), sidecar.write());
        self.current_database = Some(database);
        self.current_database_file_name = Some(file_name.clone());
        self.active_file_name = Some(file_name.clone());
        self.has_active_profile = true;
        self.selected = file_name.clone();
        self.pending_legacy_bare_name = None;
        self.write_binding(host)?;
        let shown = strip_folder(&file_name);
        Ok((
            true,
            if copy_current {
                format!("Copied current settings to {shown}.")
            } else {
                format!("Created profile {shown}.")
            },
        ))
    }

    /// Deletes the selected named profile and its side-car, and goes back
    /// to `By char`.
    ///
    /// # Errors
    /// A storage failure.
    pub fn delete(&mut self, host: &mut dyn StoreHost) -> Result<(bool, String), StoreError> {
        if ordinal::equals_ignore_case(&self.selected, BY_CHARACTER) {
            return Ok((
                false,
                "'By char' is the built-in character profile and cannot be deleted.".to_owned(),
            ));
        }
        let file_name = self.selected.clone();
        let profiles = host.storage(Root::Profiles);
        if profiles.is_available() {
            profiles.delete(&file_name)?;
        }
        let plugin = host.storage(Root::Plugin);
        if plugin.is_available() {
            plugin.delete(&side_car_key(&file_name))?;
        }
        self.selected = BY_CHARACTER.to_owned();
        self.pending_legacy_bare_name = None;
        self.current_database = None;
        self.current_database_file_name = None;
        self.active_file_name = None;
        self.has_active_profile = false;
        self.write_binding(host)?;
        Ok((
            true,
            format!("Deleted profile {}.", strip_folder(&file_name)),
        ))
    }

    fn apply_side_car(
        host: &mut dyn StoreHost,
        sidecar: &SideCarDocument,
        settings: &mut AllSettings,
        no_buff_item_names: &mut OrdinalStringSet,
        log_channels: &mut IgnoreCaseStringSet,
    ) {
        let mut warnings = Vec::new();
        sidecar.apply(settings, no_buff_item_names, log_channels, &mut |line| {
            warnings.push(line.to_owned());
        });
        for warning in warnings {
            host.log().warn(&warning);
        }
        assist_items::apply(&mut settings.combat);
    }

    /// Loads the selected profile into the live settings (migrating an old
    /// JSON profile first). A missing file seeds the defaults; one that will
    /// not read is preserved and nothing is applied.
    ///
    /// # Errors
    /// A storage failure, or settings that would not save into a migrated
    /// document.
    pub fn load_current(
        &mut self,
        host: &mut dyn StoreHost,
        settings: &mut AllSettings,
        no_buff_item_names: &mut OrdinalStringSet,
        log_channels: &mut IgnoreCaseStringSet,
    ) -> Result<ProfileLoad, StoreError> {
        self.sweep_legacy_roster_if_needed(host)?;
        self.migrate_legacy_if_needed(host, settings, no_buff_item_names)?;
        if !self.has_character_name() && ordinal::equals_ignore_case(&self.selected, BY_CHARACTER) {
            // Nobody is named yet: the shipped defaults stand in, in memory
            // only, until a name arrives.
            seed_from_defaults(settings);
            Self::apply_side_car(
                host,
                &SideCarDocument::create_defaults(),
                settings,
                no_buff_item_names,
                log_channels,
            );
            self.last_load_key = Some(String::new());
            self.current_database = None;
            self.current_database_file_name = None;
            self.active_file_name = None;
            self.has_active_profile = true;
            self.load_failure_notice = None;
            return Ok(ProfileLoad::Missing);
        }
        let file_name = self.current_file_name();
        self.last_load_key = Some(file_name.clone());
        let Some(text) = self.read_usd_text(host, &file_name)? else {
            // VTank seeds a missing file from the shipped defaults.
            let fresh = seed_from_defaults(settings);
            Self::apply_side_car(
                host,
                &SideCarDocument::create_defaults(),
                settings,
                no_buff_item_names,
                log_channels,
            );
            self.current_database = Some(fresh);
            self.current_database_file_name = Some(file_name.clone());
            self.activate(host, &file_name)?;
            return Ok(ProfileLoad::Missing);
        };
        let mut warnings = Vec::new();
        let loaded = profile::load(&text, &mut AllSettings::default(), None).and_then(|_| {
            profile::load(
                &text,
                settings,
                Some(&mut |line: &str| warnings.push(line.to_owned())),
            )
        });
        for warning in warnings {
            host.log().warn(&format!("MossTank {warning}"));
        }
        let database = match loaded {
            Ok(database) => database,
            Err(error) => {
                let notice = recovery::preserve(host, "macro", &file_name, Some(&text), &error.0);
                host.log().warn(&notice);
                self.recovery_notice = Some(notice);
                let mut failure = format!(
                    "Settings profile '{file_name}' could not be read; source was preserved."
                );
                if self.has_active_profile {
                    failure.push_str(&format!(
                        " Active profile remains {}.",
                        self.active_file_name.as_deref().unwrap_or_default()
                    ));
                }
                host.log().error(&format!("MossTank {failure}"));
                self.load_failure_notice = Some(failure);
                if self.has_active_profile
                    && let Some(active) = &self.active_file_name
                {
                    self.selected = active.clone();
                }
                return Ok(ProfileLoad::Failed);
            }
        };
        self.current_database = Some(database);
        self.current_database_file_name = Some(file_name.clone());
        let sidecar = read_json(
            host,
            &mut self.recovery_notice,
            &side_car_key(&file_name),
            SideCarDocument::read,
        )
        .unwrap_or_else(SideCarDocument::create_defaults);
        Self::apply_side_car(host, &sidecar, settings, no_buff_item_names, log_channels);
        self.activate(host, &file_name)?;
        Ok(ProfileLoad::Loaded)
    }

    /// Saves the live settings into the selected profile and its side-car.
    /// Refused (false) before a character is named, when the file on disk
    /// was never fully loaded, or when it will not read.
    ///
    /// # Errors
    /// A storage failure, or settings that would not save into the
    /// document.
    pub fn save_current(
        &mut self,
        host: &mut dyn StoreHost,
        settings: &mut AllSettings,
        no_buff_item_names: &OrdinalStringSet,
        log_channels: &IgnoreCaseStringSet,
    ) -> Result<bool, StoreError> {
        // Saving under a blank name writes a file the character, once known,
        // never reads: refusing says so.
        self.save_failure_notice = None;
        if !self.has_character_name() {
            return Ok(false);
        }
        let file_name = self.current_file_name();
        if !self.has_active_profile && self.read_usd_text(host, &file_name)?.is_some() {
            return Ok(false);
        }
        if self.can_replace_existing(host, &file_name)?.is_err() {
            return Ok(false);
        }
        let mut database = match self.current_database.take() {
            Some(current) if self.current_database_file_name.as_deref() == Some(&file_name) => {
                current
            }
            other => {
                self.current_database = other;
                profile::create_new(settings).map_err(|error| error.0)?
            }
        };
        let text = profile::save(&mut database, settings).map_err(|error| error.0)?;
        if !self.write_usd_text(host, &file_name, &text)
            || !self.write_json(
                host,
                &side_car_key(&file_name),
                SideCarDocument::capture(settings, no_buff_item_names, log_channels).write(),
            )
        {
            return Ok(false);
        }
        self.current_database = Some(database);
        self.current_database_file_name = Some(file_name.clone());
        self.activate(host, &file_name)?;
        Ok(true)
    }

    /// Puts the selected profile back to the defaults, on disk and live.
    ///
    /// # Errors
    /// A storage failure.
    pub fn clear_current(
        &mut self,
        host: &mut dyn StoreHost,
        settings: &mut AllSettings,
        no_buff_item_names: &mut OrdinalStringSet,
        log_channels: &mut IgnoreCaseStringSet,
    ) -> Result<(), StoreError> {
        let database = seed_from_defaults(settings);
        let mut warnings = Vec::new();
        SideCarDocument::create_defaults().apply(
            settings,
            no_buff_item_names,
            log_channels,
            &mut |line| warnings.push(line.to_owned()),
        );
        for warning in warnings {
            host.log().warn(&warning);
        }
        let file_name = self.current_file_name();
        self.write_usd_text(host, &file_name, &database.render());
        self.write_json(
            host,
            &side_car_key(&file_name),
            SideCarDocument::create_defaults().write(),
        );
        self.current_database = Some(database);
        self.current_database_file_name = Some(file_name.clone());
        self.activate(host, &file_name)
    }

    /// `/vt setinall`: writes one option's live value into every settings
    /// file (and its side-car's dynamic value, when it has one). How many
    /// files were considered; 0 for an unknown option or no storage.
    ///
    /// # Errors
    /// A blank name, or a storage failure.
    pub fn set_option_in_all(
        &mut self,
        host: &mut dyn StoreHost,
        name: &str,
        current: &AllSettings,
    ) -> Result<usize, StoreError> {
        if casing::is_white_space_only(name) {
            return Err(
                "The value cannot be an empty string or composed entirely of whitespace. (Parameter 'name')"
                    .to_owned(),
            );
        }
        let Some(canonical) = options::canonical(name) else {
            return Ok(0);
        };
        if !host.storage(Root::Profiles).is_available() {
            return Ok(0);
        }
        let Some(captured) = profile::capture(canonical, current) else {
            return Ok(0);
        };
        let mut file_names = directory::list_every_settings_key(host.storage(Root::Profiles))?;
        let current_file = self.current_file_name();
        if !file_names.contains(&current_file) {
            file_names.push(current_file);
        }
        let dynamic_document = current
            .combat
            .dynamic_settings
            .get(canonical)
            .map(DynamicSettingDocument::from_value);

        for file_name in &file_names {
            let text = self.read_usd_text(host, file_name)?;
            if self.can_replace_existing(host, file_name)?.is_err() {
                continue;
            }
            let mut database = match text {
                None => defaults::create(),
                Some(text) => match VtankDatabase::parse(&text) {
                    Ok(database) => database,
                    Err(error) => {
                        host.log().warn(&format!(
                            "MossTank could not update '{file_name}' for setinall: {}",
                            error.0
                        ));
                        continue;
                    }
                },
            };
            let Some(table) = database.find_mut("Settings") else {
                continue;
            };
            let (Some(name_column), Some(value_column)) =
                (table.column_index("Setting"), table.column_index("Value"))
            else {
                continue;
            };
            let found = table.rows.iter().position(|row| {
                row.cells
                    .get(name_column)
                    .is_some_and(|cell| ordinal::equals_ignore_case(cell.as_string(), canonical))
            });
            let at = match found {
                Some(at) => at,
                None => {
                    let mut row = VtankRow {
                        cells: (0..table.column_names.len())
                            .map(|_| VtankCell::tagged("0"))
                            .collect(),
                    };
                    if let Some(cell) = row.cells.get_mut(name_column) {
                        *cell = VtankCell::string(canonical);
                    }
                    table.rows.push(row);
                    table.rows.len() - 1
                }
            };
            let Some(cell) = table.rows[at].cells.get_mut(value_column) else {
                return Err("Index was out of range. Must be non-negative and less than the size of the collection. (Parameter 'index')".to_owned());
            };
            *cell = captured.clone();
            self.write_usd_text(host, file_name, &database.render());
            if self.current_database_file_name.as_deref() == Some(file_name.as_str()) {
                self.current_database = Some(database);
            }
            if let Some(dynamic) = &dynamic_document {
                let key = side_car_key(file_name);
                let mut sidecar =
                    read_json(host, &mut self.recovery_notice, &key, SideCarDocument::read)
                        .unwrap_or_else(SideCarDocument::create_defaults);
                let entries = sidecar.combat_dynamic_settings.get_or_insert_with(Vec::new);
                match entries.iter_mut().find(|(known, _)| known == canonical) {
                    Some(entry) => entry.1 = Some(dynamic.clone()),
                    None => entries.push((canonical.to_owned(), Some(dynamic.clone()))),
                }
                self.write_json(host, &key, sidecar.write());
            }
        }
        Ok(file_names.len())
    }

    // ---- Legacy JSON -> .usd and side-car --------------------------------

    fn sweep_legacy_roster_if_needed(
        &mut self,
        host: &mut dyn StoreHost,
    ) -> Result<(), StoreError> {
        if self.roster_swept {
            return Ok(());
        }
        self.roster_swept = true;
        let roster = read_json(
            host,
            &mut self.recovery_notice,
            LEGACY_ROSTER_KEY,
            LegacyRosterDocument::read,
        );
        let Some(profiles) = roster
            .and_then(|roster| roster.profiles)
            .filter(|profiles| !profiles.is_empty())
        else {
            return Ok(());
        };
        let mut remaining = Vec::new();
        let mut migrated = 0;
        for entry in profiles {
            // A null row would throw in the C# (register row STORE-5).
            let Some(entry) = entry else {
                continue;
            };
            let name = entry.name.as_deref().unwrap_or_default().trim().to_owned();
            if name.is_empty() || ordinal::equals_ignore_case(&name, BY_CHARACTER) {
                continue; // a stale row; dropped rather than looped on.
            }
            let owned = match entry.owner.as_deref() {
                None => true,
                Some(owner) if casing::is_white_space_only(owner) => true,
                Some(owner) => ordinal::equals_ignore_case(owner.trim(), &self.character_name),
            };
            if !owned {
                remaining.push(Some(entry));
                continue;
            }
            let legacy_key = legacy_profile_key(&name, false);
            let Some(legacy) = read_json(
                host,
                &mut self.recovery_notice,
                &legacy_key,
                LegacyProfileDocument::read,
            ) else {
                continue;
            };
            let file_name = self.sub_profile_key(&name);
            if self.read_usd_text(host, &file_name)?.is_none() {
                let mut settings = AllSettings::default();
                let mut no_buff_item_names = OrdinalStringSet::new();
                let mut warnings = Vec::new();
                legacy.apply(
                    &mut settings.combat,
                    &mut settings.buffs,
                    &mut settings.vitals,
                    &mut settings.inventory,
                    &mut no_buff_item_names,
                    &mut |line| warnings.push(line.to_owned()),
                );
                for warning in warnings {
                    host.log().warn(&warning);
                }
                let database = profile::create_new(&mut settings).map_err(|error| error.0)?;
                self.write_usd_text(host, &file_name, &database.render());
                let sidecar = SideCarDocument::capture(
                    &settings,
                    &no_buff_item_names,
                    &IgnoreCaseStringSet::new(),
                );
                self.write_json(host, &side_car_key(&file_name), sidecar.write());
            }
            let plugin = host.storage(Root::Plugin);
            if plugin.is_available() {
                plugin.delete(&legacy_key)?;
            }
            migrated += 1;
        }
        if migrated == 0 {
            return Ok(());
        }
        if remaining.is_empty() {
            let plugin = host.storage(Root::Plugin);
            if plugin.is_available() {
                plugin.delete(LEGACY_ROSTER_KEY)?;
            }
        } else {
            let roster = LegacyRosterDocument {
                profiles: Some(remaining),
            };
            self.write_json(host, LEGACY_ROSTER_KEY, Ok(roster.write()));
        }
        let preferences = self.preferences.write();
        self.write_json(host, PREFERENCES_KEY, Ok(preferences));
        host.log().warn(&format!(
            "Migrated {migrated} legacy MossTank named settings profile(s) from the old roster."
        ));
        Ok(())
    }

    /// When the selected `.usd` does not exist but its old JSON profile
    /// does: the JSON goes into the live settings, the `.usd` and side-car
    /// are written, and the JSON key is removed. Never overwrites a `.usd`.
    fn migrate_legacy_if_needed(
        &mut self,
        host: &mut dyn StoreHost,
        settings: &mut AllSettings,
        no_buff_item_names: &mut OrdinalStringSet,
    ) -> Result<(), StoreError> {
        let file_name = self.current_file_name();
        if self.read_usd_text(host, &file_name)?.is_some() {
            self.pending_legacy_bare_name = None;
            return Ok(());
        }
        let by_character = ordinal::equals_ignore_case(&self.selected, BY_CHARACTER);
        let mut legacy_key = match &self.pending_legacy_bare_name {
            Some(bare) if !bare.is_empty() => Some(legacy_profile_key(bare, false)),
            _ if by_character => Some(legacy_profile_key(&self.character_name, true)),
            _ => None,
        };
        let mut legacy = match &legacy_key {
            None => None,
            Some(key) => read_json(
                host,
                &mut self.recovery_notice,
                key,
                LegacyProfileDocument::read,
            ),
        };
        if legacy_key.is_none() {
            legacy_key = Some(OLD_SINGLE_PROFILE_KEY.to_owned());
        }
        if legacy.is_none() && by_character {
            legacy = read_json(
                host,
                &mut self.recovery_notice,
                OLD_SINGLE_PROFILE_KEY,
                LegacyProfileDocument::read,
            );
            legacy_key = Some(OLD_SINGLE_PROFILE_KEY.to_owned());
        }
        let Some(legacy) = legacy else {
            self.pending_legacy_bare_name = None;
            return Ok(());
        };
        let legacy_key = legacy_key.unwrap_or_default();
        let mut warnings = Vec::new();
        legacy.apply(
            &mut settings.combat,
            &mut settings.buffs,
            &mut settings.vitals,
            &mut settings.inventory,
            no_buff_item_names,
            &mut |line| warnings.push(line.to_owned()),
        );
        for warning in warnings {
            host.log().warn(&warning);
        }
        let database = profile::create_new(settings).map_err(|error| error.0)?;
        self.write_usd_text(host, &file_name, &database.render());
        let sidecar =
            SideCarDocument::capture(settings, no_buff_item_names, &IgnoreCaseStringSet::new());
        self.write_json(host, &side_car_key(&file_name), sidecar.write());
        self.current_database = Some(database);
        self.current_database_file_name = Some(file_name.clone());
        let plugin = host.storage(Root::Plugin);
        if plugin.is_available() {
            plugin.delete(&legacy_key)?;
        }
        self.pending_legacy_bare_name = None;
        host.log().warn(&format!(
            "Migrated legacy MossTank settings profile '{legacy_key}' to '{file_name}'."
        ));
        Ok(())
    }

    // ---- File naming and storage -------------------------------------------

    /// The selected profile's key (`By char` names the character's own).
    #[must_use]
    pub fn current_file_name(&self) -> String {
        if ordinal::equals_ignore_case(&self.selected, BY_CHARACTER) {
            to_key(&directory::auto_character_file_name(
                &self.character_name,
                &self.server,
                "usd",
            ))
        } else {
            self.selected.clone()
        }
    }

    fn activate(&mut self, host: &mut dyn StoreHost, file_name: &str) -> Result<(), StoreError> {
        self.active_file_name = Some(file_name.to_owned());
        self.has_active_profile = true;
        self.load_failure_notice = None;
        self.write_binding(host)
    }

    /// Whether the file may be written over: it is not there, or it reads.
    /// A file that will not read is preserved and the refusal said.
    fn can_replace_existing(
        &mut self,
        host: &mut dyn StoreHost,
        file_name: &str,
    ) -> Result<Result<(), String>, StoreError> {
        let Some(text) = self.read_usd_text(host, file_name)? else {
            return Ok(Ok(()));
        };
        match profile::load(&text, &mut AllSettings::default(), None) {
            Ok(_) => Ok(Ok(())),
            Err(error) => {
                let notice = recovery::preserve(host, "macro", file_name, Some(&text), &error.0);
                host.log().warn(&notice);
                self.recovery_notice = Some(notice);
                Ok(Err(format!(
                    "Cannot overwrite unreadable settings profile {file_name}; source was preserved."
                )))
            }
        }
    }

    fn write_binding(&self, host: &mut dyn StoreHost) -> Result<(), StoreError> {
        let storage = host.storage(Root::Profiles);
        if !self.can_bind_files() || !storage.is_available() {
            return Ok(());
        }
        let current = self.current_file_name();
        let existing =
            directory::try_read_character_binding(storage, &self.character_name, &self.server)?
                .unwrap_or(VtankCharacterBinding {
                    settings_file_name: current.clone(),
                    loot_file_name: String::new(),
                    nav_file_name: String::new(),
                    meta_file_name: None,
                });
        directory::write_character_binding(
            storage,
            &self.character_name,
            &self.server,
            &VtankCharacterBinding {
                settings_file_name: current,
                ..existing
            },
        )
    }

    fn read_usd_text(
        &self,
        host: &mut dyn StoreHost,
        file_name: &str,
    ) -> Result<Option<String>, StoreError> {
        let storage = host.storage(Root::Profiles);
        if storage.is_available() {
            storage.read_text(file_name)
        } else {
            Ok(None)
        }
    }

    fn write_usd_text(&mut self, host: &mut dyn StoreHost, file_name: &str, text: &str) -> bool {
        let storage = host.storage(Root::Profiles);
        if !storage.is_available() {
            self.save_failure_notice = Some("Profile storage is unavailable.".to_owned());
            return false;
        }
        match storage.write_text(file_name, text) {
            Ok(()) => true,
            Err(error) => {
                host.log().warn(&format!(
                    "MossTank could not save settings profile '{file_name}': {error}"
                ));
                self.save_failure_notice = Some(error);
                false
            }
        }
    }

    fn write_json(
        &mut self,
        host: &mut dyn StoreHost,
        key: &str,
        text: Result<String, String>,
    ) -> bool {
        let storage = host.storage(Root::Plugin);
        if !storage.is_available() {
            self.save_failure_notice = Some("Profile storage is unavailable.".to_owned());
            return false;
        }
        match text.and_then(|text| {
            storage
                .write_text(key, &text)
                .map_err(|error| error.to_string())
        }) {
            Ok(()) => true,
            Err(error) => {
                host.log()
                    .warn(&format!("MossTank profile could not be saved: {error}"));
                self.save_failure_notice = Some(error);
                false
            }
        }
    }

    fn save_preferences(&mut self, host: &mut dyn StoreHost) {
        let text = self.preferences.write();
        self.write_json(host, PREFERENCES_KEY, Ok(text));
    }
}

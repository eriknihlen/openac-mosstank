//! The loot profile store: which `.utl` in `mosstank/loot/` is this
//! character's (the binding file, `By char`, named profiles), loading with
//! a partial read kept write-protected, saving with the blocks another tool
//! added kept, item giver profiles, importing a VTClassic file, and the
//! one-time migration of older MossTank JSON loot profiles.

use mosstank_dotnet::text::{casing, ordinal};
use mosstank_dotnet::{parse_bool, try_parse_uint};
use mosstank_formats::utl::{
    self, LootRule, VtankLootExtraBlock, VtankLootProfile, VtankSalvageCombineSettings,
};
use mosstank_settings::model::LootSettings;
use openac_plugin_compat::json;

use crate::directory::{self, LOOT_FOLDER, VtankCharacterBinding};
use crate::layout;
use crate::profile_store::{ProfileLoad, StoreError};
use crate::recovery::{self, sha256_hex};
use crate::sidecar::LootRuleDocument;
use crate::stj::{self, JsonError, Object};
use crate::storage::{Root, StoreHost};

/// The built-in character profile's name in the picker.
pub const BY_CHARACTER: &str = "By char";

/// What the picker shows while no loot profile is active.
pub const NO_ACTIVE_PROFILE: &str = "No active loot profile";

const LEGACY_ROSTER_KEY: &str = "profiles/loot/index.json";

/// The name a person sees and types: neither folder nor extension.
fn strip_utl(name: &str) -> String {
    if ordinal::equals_ignore_case(name, BY_CHARACTER) {
        return name.to_owned();
    }
    let bare = directory::strip_folder(name, LOOT_FOLDER);
    match bare.len().checked_sub(4) {
        Some(at) if ordinal::ends_with_ignore_case(bare, ".utl") => bare[..at].to_owned(),
        _ => bare.to_owned(),
    }
}

fn to_file_name(name: &str) -> String {
    let mut bare = directory::strip_folder(name, LOOT_FOLDER).to_owned();
    if !ordinal::ends_with_ignore_case(&bare, ".utl") {
        bare.push_str(".utl");
    }
    format!("{LOOT_FOLDER}/{bare}")
}

fn without_utl(name: &str) -> &str {
    match name.len().checked_sub(4) {
        Some(at) if ordinal::ends_with_ignore_case(name, ".utl") => &name[..at],
        _ => name,
    }
}

fn profile_key(value: &str, by_character: bool) -> String {
    let identity = format!(
        "{}{}",
        if by_character { "char:" } else { "named:" },
        casing::to_upper_invariant(value.trim())
    );
    format!("profiles/loot/{}.json", sha256_hex(&identity))
}

/// The int keys VTank fills from the server that the client does not hand
/// a plugin, so a rule on one reads zero here.
const VTANK_INT_BASE: u32 = 218_103_808;
const UNSUPPORTED_INT_KEYS: [(u32, &str); 12] = [
    (95, "RadarBlipColor"),
    (133, "RadarVisibility"),
    (VTANK_INT_BASE + 12, "Monarch"),
    (VTANK_INT_BASE + 20, "HookMask"),
    (VTANK_INT_BASE + 24, "CreateFlags1"),
    (VTANK_INT_BASE + 25, "CreateFlags2"),
    (VTANK_INT_BASE + 38, "Unknown8000000"),
    (VTANK_INT_BASE + 39, "PhysicsDataFlags"),
    (VTANK_INT_BASE + 43, "EquippedBy"),
    (VTANK_INT_BASE + 44, "LastAttacker"),
    (VTANK_INT_BASE + 45, "AllegianceObject"),
    (VTANK_INT_BASE + 46, "OwnedBy"),
];

fn payload_lines(payload: &str) -> Vec<String> {
    payload
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .map(str::to_owned)
        .collect()
}

/// The warning a load gives when enabled rules read a value the client
/// cannot supply (they are decided on a zero); `None` when none do.
#[must_use]
pub fn unsupported_key_warning(profile_name: &str, rules: &[LootRule]) -> Option<String> {
    const NAMED: usize = 5;
    let mut uses = Vec::new();
    for rule in rules {
        // VTClassic's "disabled" requirement set to true: never matches.
        let disabled = rule.vtank_requirements.iter().any(|requirement| {
            requirement.kind == 9999
                && parse_bool(payload_lines(&requirement.payload)[0].trim()) == Ok(true)
        });
        if disabled {
            continue;
        }
        for requirement in &rule.vtank_requirements {
            if !matches!(requirement.kind, 2 | 3 | 11 | 12 | 13 | 2003) {
                continue;
            }
            let values = payload_lines(&requirement.payload);
            let Some(key) = values.get(1).and_then(|text| try_parse_uint(text)) else {
                continue;
            };
            if let Some((_, name)) = UNSUPPORTED_INT_KEYS.iter().find(|(known, _)| *known == key) {
                uses.push(format!("\"{}\" reads {name} ({key})", rule.name.trim()));
            }
        }
    }
    if uses.is_empty() {
        return None;
    }
    let mut named = uses
        .iter()
        .take(NAMED)
        .cloned()
        .collect::<Vec<_>>()
        .join("; ");
    if uses.len() > NAMED {
        named.push_str(&format!("; and {} more", uses.len() - NAMED));
    }
    Some(format!(
        "Loot profile {profile_name}: {} enabled rule requirement(s) read a value MossTank cannot get from the client yet, so they will not match as in VTank: {named}.",
        uses.len()
    ))
}

/// The loot profile store (`MossTankLootProfileStore`).
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct LootProfileStore {
    character_name: String,
    selected: String,
    pending_selection: Option<String>,
    pending_selection_should_persist: bool,
    has_active_profile: bool,
    active_profile_is_partial: bool,
    roster_swept: bool,
    server: String,
    recovery_notice: Option<String>,
    load_failure_notice: Option<String>,
    load_notice: Option<String>,
    last_load_key: Option<String>,
}

impl Default for LootProfileStore {
    fn default() -> Self {
        Self::new()
    }
}

impl LootProfileStore {
    /// A store bound to nobody, no profile active.
    #[must_use]
    pub fn new() -> Self {
        Self {
            character_name: String::new(),
            selected: NO_ACTIVE_PROFILE.to_owned(),
            pending_selection: None,
            pending_selection_should_persist: false,
            has_active_profile: false,
            active_profile_is_partial: false,
            roster_swept: false,
            server: String::new(),
            recovery_notice: None,
            load_failure_notice: None,
            load_notice: None,
            last_load_key: None,
        }
    }

    /// The selected profile's name.
    #[must_use]
    pub fn selected(&self) -> String {
        strip_utl(&self.selected)
    }

    /// Whether a profile is active.
    #[must_use]
    pub fn has_active_profile(&self) -> bool {
        self.has_active_profile
    }

    /// Whether the active profile was read only up to an incomplete tail.
    #[must_use]
    pub fn active_profile_is_partial(&self) -> bool {
        self.active_profile_is_partial
    }

    /// What was said when a file had to be preserved.
    #[must_use]
    pub fn recovery_notice(&self) -> Option<&str> {
        self.recovery_notice.as_deref()
    }

    /// What was said when the selected profile would not read.
    #[must_use]
    pub fn load_failure_notice(&self) -> Option<&str> {
        self.load_failure_notice.as_deref()
    }

    /// What was said when only part of the profile read.
    #[must_use]
    pub fn load_notice(&self) -> Option<&str> {
        self.load_notice.as_deref()
    }

    /// The file the last load read, or tried to read.
    #[must_use]
    pub fn last_load_key(&self) -> Option<&str> {
        self.last_load_key.as_deref()
    }

    fn can_bind_files(&self) -> bool {
        !self.character_name.is_empty() && !self.server.is_empty()
    }

    /// The picker's names: the inactive marker while nothing is active,
    /// `By char`, then the loot files.
    ///
    /// # Errors
    /// The loot folder cannot be listed.
    pub fn available_names(&self, host: &mut dyn StoreHost) -> Result<Vec<String>, StoreError> {
        let mut names = Vec::new();
        if !self.has_active_profile {
            names.push(NO_ACTIVE_PROFILE.to_owned());
        }
        names.push(BY_CHARACTER.to_owned());
        for entry in directory::list_loot_profiles(host.storage(Root::Profiles))? {
            if !entry.file_name.is_empty() {
                names.push(strip_utl(&entry.file_name));
            }
        }
        Ok(names)
    }

    /// Binds the store to a character on the current world (a partial
    /// identity never replaces an established one); the binding's loot file,
    /// or `By char`, is loaded next. Whether it changed.
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
        let binding = if self.can_bind_files() {
            directory::try_read_character_binding(
                host.storage(Root::Profiles),
                &self.character_name,
                &self.server,
            )?
        } else {
            None
        };
        self.selected = NO_ACTIVE_PROFILE.to_owned();
        self.has_active_profile = false;
        self.active_profile_is_partial = false;
        self.pending_selection = Some(match binding {
            Some(bound) if !bound.loot_file_name.is_empty() => bound.loot_file_name,
            _ => BY_CHARACTER.to_owned(),
        });
        self.pending_selection_should_persist = false;
        self.load_failure_notice = None;
        self.load_notice = None;
        Ok(true)
    }

    fn profiles_have(host: &mut dyn StoreHost, key: &str) -> Result<bool, StoreError> {
        let storage = host.storage(Root::Profiles);
        Ok(storage.is_available() && storage.read_text(key)?.is_some())
    }

    /// Chooses the profile the next load reads; whether there is one.
    ///
    /// # Errors
    /// A storage failure.
    pub fn select(
        &mut self,
        host: &mut dyn StoreHost,
        name: Option<&str>,
    ) -> Result<bool, StoreError> {
        let normalized = name.map(str::trim).unwrap_or_default();
        if normalized.is_empty() {
            return Ok(false);
        }
        if ordinal::equals_ignore_case(normalized, BY_CHARACTER) {
            self.pending_selection = Some(BY_CHARACTER.to_owned());
            self.pending_selection_should_persist = true;
            return Ok(true);
        }
        let candidate = to_file_name(normalized);
        if Self::profiles_have(host, &candidate)? {
            self.pending_selection = Some(candidate);
            self.pending_selection_should_persist = true;
            return Ok(true);
        }
        Ok(false)
    }

    /// Whether a profile of that name exists.
    ///
    /// # Errors
    /// A storage failure.
    pub fn exists(&self, host: &mut dyn StoreHost, name: Option<&str>) -> Result<bool, StoreError> {
        let normalized = name.map(str::trim).unwrap_or_default();
        if normalized.is_empty() {
            return Ok(false);
        }
        if ordinal::equals_ignore_case(normalized, BY_CHARACTER) {
            return Ok(true);
        }
        Self::profiles_have(host, &to_file_name(normalized))
    }

    /// Makes a named profile (a copy of the current rules and salvage
    /// settings, or empty) and selects it. The notice says what happened.
    ///
    /// # Errors
    /// A storage failure.
    pub fn create(
        &mut self,
        host: &mut dyn StoreHost,
        name: Option<&str>,
        copy_current: bool,
        current: &[LootRule],
        settings: Option<&mut LootSettings>,
    ) -> Result<(bool, String), StoreError> {
        let normalized = name.map(str::trim).unwrap_or_default();
        let length = openac_plugin_compat::text::utf16::len(normalized);
        if !(1..=64).contains(&length) {
            return Ok((
                false,
                "Enter a loot profile name (1-64 characters).".to_owned(),
            ));
        }
        if ordinal::equals_ignore_case(normalized, BY_CHARACTER) {
            return Ok((false, "'By char' is the built-in loot profile.".to_owned()));
        }
        if copy_current && (!self.has_active_profile || self.active_profile_is_partial) {
            return Ok((
                false,
                if self.active_profile_is_partial {
                    "Cannot copy an incomplete loot profile; source was preserved."
                } else {
                    "Cannot copy loot rules because no complete active profile is available."
                }
                .to_owned(),
            ));
        }
        let file_name = to_file_name(normalized);
        let existing = {
            let storage = host.storage(Root::Profiles);
            if storage.is_available() {
                storage.read_text(&file_name)?
            } else {
                None
            }
        };
        if let Some(existing) = existing
            && let Err(error) = utl::try_read(Some(&existing))
        {
            let notice = recovery::preserve(host, "loot", &file_name, Some(&existing), &error);
            host.log().warn(&notice);
            self.recovery_notice = Some(notice);
            return Ok((
                false,
                format!(
                    "Cannot overwrite unreadable loot profile {}; source was preserved.",
                    strip_utl(&file_name)
                ),
            ));
        }
        let salvage = if copy_current {
            settings
                .as_ref()
                .map(|loot| loot.salvage_combine.clone())
                .unwrap_or_default()
        } else {
            VtankSalvageCombineSettings::default()
        };
        let mut profile = VtankLootProfile {
            rules: if copy_current {
                current.to_vec()
            } else {
                Vec::new()
            },
            salvage_combine: salvage,
            ..VtankLootProfile::default()
        };
        Self::write_utl(host, &file_name, &mut profile);
        self.selected = file_name;
        self.has_active_profile = true;
        self.active_profile_is_partial = false;
        self.pending_selection = None;
        self.pending_selection_should_persist = false;
        if let Some(loot) = settings {
            loot.profile_active = true;
        }
        self.write_binding(host)?;
        let shown = self.selected();
        Ok((
            true,
            if copy_current {
                format!("Copied loot rules to {shown}.")
            } else {
                format!("Created loot profile {shown}.")
            },
        ))
    }

    /// Loads the selected (or pending) profile's rules into `target` and its
    /// salvage settings; a profile read only up to an incomplete tail is
    /// applied and kept write-protected, one that will not read leaves the
    /// active one in force.
    ///
    /// # Errors
    /// A storage failure.
    pub fn load_current(
        &mut self,
        host: &mut dyn StoreHost,
        target: &mut Vec<LootRule>,
        mut settings: Option<&mut LootSettings>,
    ) -> Result<ProfileLoad, StoreError> {
        self.sweep_legacy_roster_if_needed(host)?;
        let file_name = self.pending_or_current_file_name();
        self.last_load_key = Some(file_name.clone());
        if file_name.is_empty() {
            self.load_failure_notice = Some("No loot profile is active.".to_owned());
            self.deactivate_if_needed(target, settings);
            return Ok(ProfileLoad::Failed);
        }
        let text = {
            let storage = host.storage(Root::Profiles);
            if storage.is_available() {
                storage.read_text(&file_name)?
            } else {
                None
            }
        };
        let Some(text) = text else {
            if self.is_by_character(&file_name) {
                self.activate(host, BY_CHARACTER, settings, false)?;
                return Ok(ProfileLoad::Missing);
            }
            return Ok(self.reject_load(
                &file_name,
                "The selected loot profile was not found.",
                target,
                settings,
            ));
        };
        let read = utl::read_allowing_partial(Some(&text));
        let mut profile = read.profile;
        if !read.is_complete {
            let notice = recovery::preserve(host, "loot", &file_name, Some(&text), &read.error);
            host.log().warn(&notice);
            self.recovery_notice = Some(notice);
            if profile.rules.is_empty() {
                return Ok(self.reject_load(&file_name, &read.error, target, settings));
            }
            utl::apply_mosstank_expressions(&mut profile);
            target.clone_from(&profile.rules);
            self.warn_unsupported_keys(host, &file_name, &profile.rules);
            if let Some(loot) = settings.as_deref_mut() {
                loot.salvage_combine = profile.salvage_combine.clone();
            }
            self.activate(host, &file_name, settings, true)?;
            self.load_notice = Some(format!(
                "Loaded {} rules; incomplete profile tail was not loaded and source was preserved.",
                profile.rules.len()
            ));
            return Ok(ProfileLoad::Partial);
        }
        utl::apply_mosstank_expressions(&mut profile);
        target.clone_from(&profile.rules);
        self.warn_unsupported_keys(host, &file_name, &profile.rules);
        if let Some(loot) = settings.as_deref_mut() {
            loot.salvage_combine = profile.salvage_combine.clone();
        }
        self.activate(host, &file_name, settings, false)?;
        Ok(ProfileLoad::Loaded)
    }

    fn warn_unsupported_keys(&self, host: &mut dyn StoreHost, file_name: &str, rules: &[LootRule]) {
        if let Some(warning) = unsupported_key_warning(&strip_utl(file_name), rules) {
            host.log().warn(&warning);
        }
    }

    fn read_rules(
        host: &mut dyn StoreHost,
        key: &str,
    ) -> Result<Option<Vec<LootRule>>, StoreError> {
        let text = host.storage(Root::Profiles).read_text(key)?;
        let Some(text) = text else {
            return Ok(None);
        };
        Ok(utl::try_read(Some(&text)).ok().map(|mut profile| {
            utl::apply_mosstank_expressions(&mut profile);
            profile.rules
        }))
    }

    /// Loads an item giver profile from the shared `itemgiver` folder;
    /// false when there is no such file or it will not read.
    ///
    /// # Errors
    /// A storage failure.
    pub fn try_load_item_giver(
        &self,
        host: &mut dyn StoreHost,
        name: Option<&str>,
        target: &mut Vec<LootRule>,
    ) -> Result<bool, StoreError> {
        let normalized = name.map(str::trim).unwrap_or_default();
        if normalized.is_empty() || !host.storage(Root::Profiles).is_available() {
            return Ok(false);
        }
        match Self::read_rules(host, &layout::item_giver_profile_key(normalized))? {
            Some(rules) => {
                *target = rules;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Loads a named profile's rules without selecting it; false when there
    /// is no such file or it will not read.
    ///
    /// # Errors
    /// A storage failure.
    pub fn try_load_named(
        &self,
        host: &mut dyn StoreHost,
        name: Option<&str>,
        target: &mut Vec<LootRule>,
    ) -> Result<bool, StoreError> {
        let normalized = without_utl(name.map(str::trim).unwrap_or_default());
        if normalized.is_empty() {
            return Ok(false);
        }
        let file_name = if ordinal::equals_ignore_case(normalized, BY_CHARACTER) {
            self.current_file_name()
        } else {
            to_file_name(normalized)
        };
        if !host.storage(Root::Profiles).is_available() {
            return Ok(false);
        }
        match Self::read_rules(host, &file_name)? {
            Some(rules) => {
                *target = rules;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// The character's own file of a character nobody has named yet does
    /// not exist.
    fn files_under_nobody(&self) -> bool {
        self.character_name.is_empty() && self.is_by_character(&self.selected)
    }

    /// Saves the rules and salvage settings into the active profile,
    /// keeping the blocks another tool added; nothing while no complete
    /// profile is active or the file on disk will not read.
    ///
    /// # Errors
    /// A storage failure.
    pub fn save_current(
        &mut self,
        host: &mut dyn StoreHost,
        rules: &[LootRule],
        settings: Option<&LootSettings>,
    ) -> Result<(), StoreError> {
        if self.files_under_nobody() || !self.has_active_profile || self.active_profile_is_partial {
            return Ok(());
        }
        let file_name = self.current_file_name();
        let mut profile = VtankLootProfile {
            rules: rules.to_vec(),
            salvage_combine: settings
                .map(|loot| loot.salvage_combine.clone())
                .unwrap_or_default(),
            ..VtankLootProfile::default()
        };
        let existing = {
            let storage = host.storage(Root::Profiles);
            if storage.is_available() {
                storage.read_text(&file_name)?
            } else {
                None
            }
        };
        if let Some(existing_text) = existing {
            match utl::try_read(Some(&existing_text)) {
                Ok(existing) => profile.unknown_blocks = existing.unknown_blocks,
                Err(error) => {
                    let notice =
                        recovery::preserve(host, "loot", &file_name, Some(&existing_text), &error);
                    host.log().warn(&notice);
                    self.recovery_notice = Some(notice);
                    return Ok(());
                }
            }
        }
        Self::write_utl(host, &file_name, &mut profile);
        Ok(())
    }

    /// Deletes the selected named profile; `By char` comes next.
    ///
    /// # Errors
    /// A storage failure.
    pub fn delete(&mut self, host: &mut dyn StoreHost) -> Result<(bool, String), StoreError> {
        if !self.has_active_profile {
            return Ok((false, "No loot profile is active.".to_owned()));
        }
        if ordinal::equals_ignore_case(&self.selected, BY_CHARACTER) {
            return Ok((
                false,
                "'By char' is the built-in loot profile and cannot be deleted.".to_owned(),
            ));
        }
        let file_name = self.selected.clone();
        let storage = host.storage(Root::Profiles);
        if storage.is_available() {
            storage.delete(&file_name)?;
        }
        self.selected = NO_ACTIVE_PROFILE.to_owned();
        self.has_active_profile = false;
        self.active_profile_is_partial = false;
        self.pending_selection = Some(BY_CHARACTER.to_owned());
        self.pending_selection_should_persist = true;
        Ok((
            true,
            format!("Deleted loot profile {}.", strip_utl(&file_name)),
        ))
    }

    /// Empties the rules and salvage settings, and saves.
    ///
    /// # Errors
    /// A storage failure.
    pub fn clear_current(
        &mut self,
        host: &mut dyn StoreHost,
        target: &mut Vec<LootRule>,
        mut settings: Option<&mut LootSettings>,
    ) -> Result<(), StoreError> {
        target.clear();
        if let Some(loot) = settings.as_deref_mut() {
            loot.salvage_combine = VtankSalvageCombineSettings::default();
        }
        self.save_current(host, target, settings.as_deref())
    }

    /// Imports a VTClassic loot file from the plugin's `imports` (or
    /// `exports`) folder as a named profile and selects it.
    ///
    /// # Errors
    /// A storage failure.
    pub fn try_import_legacy(
        &mut self,
        host: &mut dyn StoreHost,
        name: Option<&str>,
        target: &mut Vec<LootRule>,
        mut settings: Option<&mut LootSettings>,
    ) -> Result<(bool, String), StoreError> {
        let normalized = without_utl(name.map(str::trim).unwrap_or_default()).to_owned();
        let plugin = host.storage(Root::Plugin);
        if !plugin.is_available() || normalized.is_empty() {
            return Ok((
                false,
                "Legacy loot-profile storage is unavailable.".to_owned(),
            ));
        }
        let mut candidates = plugin.list("imports")?;
        candidates.extend(plugin.list("exports")?);
        let key = candidates.into_iter().find(|candidate| {
            let file = candidate
                .rfind(['/', '\\'])
                .map_or(candidate.as_str(), |at| &candidate[at + 1..]);
            ordinal::ends_with_ignore_case(candidate, ".utl")
                && ordinal::equals_ignore_case(file_stem(file), &normalized)
        });
        let source = match &key {
            Some(key) => plugin.read_text(key)?,
            None => None,
        };
        let Some(source) = source.filter(|text| !casing::is_white_space_only(text)) else {
            return Ok((
                false,
                format!("VTClassic loot file '{normalized}.utl' was not found in imports."),
            ));
        };
        let key = key.unwrap_or_default();
        let mut imported = match utl::try_read(Some(&source)) {
            Ok(profile) => profile,
            Err(error) => {
                let file = key
                    .rfind(['/', '\\'])
                    .map_or(key.as_str(), |at| &key[at + 1..]);
                return Ok((false, format!("Could not import {file}: {error}")));
            }
        };
        let file_name = to_file_name(&normalized);
        Self::write_utl(host, &file_name, &mut imported);
        self.selected = file_name;
        self.has_active_profile = true;
        self.active_profile_is_partial = false;
        self.pending_selection = None;
        self.pending_selection_should_persist = false;
        if let Some(loot) = settings.as_deref_mut() {
            loot.profile_active = true;
        }
        self.write_binding(host)?;
        target.clone_from(&imported.rules);
        if let Some(loot) = settings {
            loot.salvage_combine = imported.salvage_combine.clone();
        }
        Ok((
            true,
            format!("Imported VTClassic loot profile {}.", self.selected),
        ))
    }

    fn sweep_legacy_roster_if_needed(
        &mut self,
        host: &mut dyn StoreHost,
    ) -> Result<(), StoreError> {
        if self.roster_swept {
            return Ok(());
        }
        self.roster_swept = true;
        if !host.storage(Root::Plugin).is_available() {
            return Ok(());
        }
        let by_character_file_name = self.by_character_key();
        if host
            .storage(Root::Profiles)
            .read_text(&by_character_file_name)?
            .is_none()
        {
            let key = profile_key(&self.character_name, true);
            if let Some(document) = self.read_legacy(host, &key, read_loot_profile_document) {
                let mut profile = document;
                Self::write_utl(host, &by_character_file_name, &mut profile);
                host.storage(Root::Plugin).delete(&key)?;
            }
        }
        let roster = self.read_legacy(host, LEGACY_ROSTER_KEY, read_roster_names);
        let Some(names) = roster.flatten().filter(|names| !names.is_empty()) else {
            return Ok(());
        };
        let mut migrated = 0;
        for raw in names {
            let name = raw.as_deref().unwrap_or_default().trim().to_owned();
            if name.is_empty() || ordinal::equals_ignore_case(&name, BY_CHARACTER) {
                continue; // a stale row; dropped rather than looped on.
            }
            let legacy_key = profile_key(&name, false);
            let Some(mut legacy) = self.read_legacy(host, &legacy_key, read_loot_profile_document)
            else {
                continue;
            };
            let file_name = to_file_name(&name);
            if host
                .storage(Root::Profiles)
                .read_text(&file_name)?
                .is_none()
            {
                Self::write_utl(host, &file_name, &mut legacy);
            }
            host.storage(Root::Plugin).delete(&legacy_key)?;
            migrated += 1;
        }
        if migrated == 0 {
            return Ok(());
        }
        // No row is ever kept back, so the roster goes.
        host.storage(Root::Plugin).delete(LEGACY_ROSTER_KEY)?;
        host.log().warn(&format!(
            "Migrated {migrated} legacy MossTank named loot profile(s) from the old roster."
        ));
        Ok(())
    }

    fn read_legacy<T>(
        &mut self,
        host: &mut dyn StoreHost,
        key: &str,
        parse: fn(&str) -> Result<Option<T>, JsonError>,
    ) -> Option<T> {
        let storage = host.storage(Root::Plugin);
        if !storage.is_available() {
            return None;
        }
        let failed = match storage.read_text(key) {
            Ok(text) => {
                let text = text.filter(|text| !casing::is_white_space_only(text))?;
                match parse(&text) {
                    Ok(document) => return document,
                    Err(error) => (Some(text), error),
                }
            }
            Err(error) => (None, error),
        };
        let notice = recovery::preserve(host, "loot", key, failed.0.as_deref(), &failed.1);
        host.log().warn(&notice);
        self.recovery_notice = Some(notice);
        None
    }

    // ---- File naming and storage -------------------------------------------

    /// The selected profile's key (`By char` names the character's own).
    #[must_use]
    pub fn current_file_name(&self) -> String {
        if self.is_by_character(&self.selected) {
            self.by_character_key()
        } else {
            self.selected.clone()
        }
    }

    fn pending_or_current_file_name(&self) -> String {
        let selection = match &self.pending_selection {
            Some(pending) => pending.clone(),
            None if self.has_active_profile => self.selected.clone(),
            None => String::new(),
        };
        if self.is_by_character(&selection) {
            self.by_character_key()
        } else {
            selection
        }
    }

    fn is_by_character(&self, file_name: &str) -> bool {
        ordinal::equals_ignore_case(file_name, BY_CHARACTER)
            || ordinal::equals_ignore_case(file_name, &self.by_character_key())
    }

    fn by_character_key(&self) -> String {
        to_file_name(&directory::auto_character_file_name(
            &self.character_name,
            &self.server,
            "utl",
        ))
    }

    fn activate(
        &mut self,
        host: &mut dyn StoreHost,
        file_name: &str,
        settings: Option<&mut LootSettings>,
        partial: bool,
    ) -> Result<(), StoreError> {
        let write_binding = self.pending_selection_should_persist;
        self.selected = if self.is_by_character(file_name) {
            BY_CHARACTER.to_owned()
        } else {
            file_name.to_owned()
        };
        self.pending_selection = None;
        self.pending_selection_should_persist = false;
        self.has_active_profile = true;
        self.active_profile_is_partial = partial;
        self.load_failure_notice = None;
        self.load_notice = None;
        if let Some(loot) = settings {
            loot.profile_active = true;
        }
        if write_binding {
            self.write_binding(host)?;
        }
        Ok(())
    }

    fn reject_load(
        &mut self,
        file_name: &str,
        error: &str,
        target: &mut Vec<LootRule>,
        settings: Option<&mut LootSettings>,
    ) -> ProfileLoad {
        self.pending_selection = None;
        self.pending_selection_should_persist = false;
        let requested = strip_utl(file_name);
        self.load_failure_notice = Some(if self.has_active_profile {
            format!(
                "Loot profile '{requested}' could not be read: {error} Active profile remains {}.",
                self.selected()
            )
        } else {
            format!(
                "Loot profile '{requested}' could not be read: {error} No loot profile is active."
            )
        });
        self.deactivate_if_needed(target, settings);
        ProfileLoad::Failed
    }

    fn deactivate_if_needed(
        &mut self,
        target: &mut Vec<LootRule>,
        settings: Option<&mut LootSettings>,
    ) {
        if self.has_active_profile {
            return;
        }
        self.selected = NO_ACTIVE_PROFILE.to_owned();
        self.active_profile_is_partial = false;
        target.clear();
        if let Some(loot) = settings {
            loot.salvage_combine = VtankSalvageCombineSettings::default();
            loot.profile_active = false;
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
                    settings_file_name: String::new(),
                    loot_file_name: current.clone(),
                    nav_file_name: String::new(),
                    meta_file_name: None,
                });
        directory::write_character_binding(
            storage,
            &self.character_name,
            &self.server,
            &VtankCharacterBinding {
                loot_file_name: current,
                ..existing
            },
        )
    }

    fn write_utl(host: &mut dyn StoreHost, file_name: &str, profile: &mut VtankLootProfile) {
        if !host.storage(Root::Profiles).is_available() {
            return;
        }
        utl::attach_mosstank_expressions(profile);
        let text = utl::write(profile);
        if let Err(error) = host.storage(Root::Profiles).write_text(file_name, &text) {
            host.log().warn(&format!(
                "MossTank loot profile could not be saved: {error}"
            ));
        }
    }
}

/// `Path.GetFileNameWithoutExtension` for a bare file name.
fn file_stem(file: &str) -> &str {
    file.rfind('.').map_or(file, |dot| &file[..dot])
}

/// The old roster's names (`LegacyRosterDocument.Names`); `Some(None)` for
/// a roster whose names are null.
fn read_roster_names(text: &str) -> Result<Option<Option<Vec<Option<String>>>>, JsonError> {
    let document = json::parse(text).map_err(|error| error.to_string())?;
    let Some(o) = Object::of(&document, "$", "LegacyRosterDocument")? else {
        return Ok(None);
    };
    o.int("Version", 1)?;
    o.map("SelectedByCharacter", "Dictionary`2[String,String]")?;
    Ok(Some(o.strings("Names")?))
}

/// An old JSON loot profile (`LootProfileDocument`), as the profile it
/// becomes.
fn read_loot_profile_document(text: &str) -> Result<Option<VtankLootProfile>, JsonError> {
    let document = json::parse(text).map_err(|error| error.to_string())?;
    let Some(o) = Object::of(&document, "$", "LootProfileDocument")? else {
        return Ok(None);
    };
    o.int("Version", 2)?;
    let rules = match o.objects("Rules", Some(Vec::new()), "LootRuleDocument")? {
        None => Vec::new(),
        Some(items) => items
            .iter()
            .map(|item| item.as_ref().map(LootRuleDocument::read).transpose())
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .map(|rule| rule.to_rule())
            .collect(),
    };
    let salvage_combine = match o.get("SalvageCombine") {
        None => VtankSalvageCombineSettings::default(),
        Some(value) => {
            match Object::of(value, "$.SalvageCombine", "VtankSalvageCombineSettings")? {
                None => VtankSalvageCombineSettings::default(),
                Some(salvage) => read_salvage(&salvage)?,
            }
        }
    };
    let unknown_blocks = match o.objects(
        "UnknownBlocks",
        Some(Vec::new()),
        "VtankLootExtraBlockDocument",
    )? {
        None => Vec::new(),
        Some(items) => items
            .iter()
            .flatten()
            .map(|block| -> Result<_, JsonError> {
                Ok(VtankLootExtraBlock {
                    kind: block.string("Type", "")?.unwrap_or_default(),
                    payload: block.string("Payload", "")?.unwrap_or_default(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok(Some(VtankLootProfile {
        rules,
        salvage_combine,
        unknown_blocks,
        ..VtankLootProfile::default()
    }))
}

fn read_salvage(o: &Object<'_>) -> Result<VtankSalvageCombineSettings, JsonError> {
    let defaults = VtankSalvageCombineSettings::default();
    let key_number = |key: &str| -> Result<i32, JsonError> {
        openac_plugin_compat::number::parse_i32(
            key,
            openac_plugin_compat::number::NumberStyles::INTEGER,
        )
        .ok_or_else(|| {
            format!("The JSON property name '{key}' could not be converted to System.Int32.")
        })
    };
    // A property the document has replaces the default map; a null one is
    // empty (the C# would throw cloning it).
    let material_combine_strings = match o
        .map("MaterialCombineStrings", "Dictionary`2[Int32,String]")?
    {
        _ if o.get("MaterialCombineStrings").is_none() => defaults.material_combine_strings.clone(),
        None => std::collections::BTreeMap::new(),
        Some(entries) => entries
            .into_iter()
            .map(|(key, value)| {
                Ok((
                    key_number(key)?,
                    stj::string_value(value, "MaterialCombineStrings")?.unwrap_or_default(),
                ))
            })
            .collect::<Result<_, JsonError>>()?,
    };
    let material_value_mode_values =
        match o.map("MaterialValueModeValues", "Dictionary`2[Int32,Int32]")? {
            None => std::collections::BTreeMap::new(),
            Some(entries) => entries
                .into_iter()
                .map(|(key, value)| {
                    Ok((
                        key_number(key)?,
                        stj::int_value(value, "MaterialValueModeValues")?,
                    ))
                })
                .collect::<Result<_, JsonError>>()?,
        };
    Ok(VtankSalvageCombineSettings {
        default_combine_string: o
            .string("DefaultCombineString", &defaults.default_combine_string)?
            .unwrap_or_default(),
        material_combine_strings,
        material_value_mode_values,
    })
}

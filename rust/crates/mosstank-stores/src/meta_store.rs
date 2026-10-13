//! The meta store: which meta in `mosstank/metas/` is this character's,
//! read and written in the file's own form (MossTank's `.af`, or a `.met`
//! dropped into the folder, which stays a `.met`), named metas, importing a
//! VTank `.met`, and the one-time migration of older JSON metas.

use mosstank_formats::af;
use mosstank_formats::line_endings;
use mosstank_formats::met;
use mosstank_formats::meta::{
    MetaAction, MetaActionKind, MetaCondition, MetaConditionKind, MetaProfile, MetaRule,
};
use mosstank_formats::route::{
    ClientPathing, NavigationSettings, Position, RouteJumpDirection, RouteMode, RouteRecallKind,
    RouteWaypoint, RouteWaypointType, SpellLookup,
};
use openac_plugin_compat::json::{self, JsonValue};
use openac_plugin_compat::text::{casing, ordinal};

use crate::directory::{self, META_FOLDER, VtankCharacterBinding};
use crate::global_variables::hash as short_hash;
use crate::profile_store::StoreError;
use crate::recovery;
use crate::stj::{JsonError, Object};
use crate::storage::{Root, StoreHost};

/// The built-in character meta's name in the picker.
pub const BY_CHARACTER: &str = "By char";

const LEGACY_ROSTER_KEY: &str = "profiles/meta/index.json";

fn folder_prefix() -> String {
    format!("{META_FOLDER}/")
}

/// A meta named in MossTank's own form unless the name carries `.af` or
/// `.met`.
fn to_file_name(name: &str) -> String {
    let mut bare = directory::strip_folder(name, META_FOLDER).to_owned();
    if !ordinal::ends_with_ignore_case(&bare, ".af")
        && !ordinal::ends_with_ignore_case(&bare, ".met")
    {
        bare.push_str(".af");
    }
    format!("{META_FOLDER}/{bare}")
}

/// A meta's name without the extension it may carry.
#[must_use]
pub fn bare_name(name: &str) -> &str {
    for extension in [".met", ".af"] {
        if name.len() > extension.len() && ordinal::ends_with_ignore_case(name, extension) {
            return &name[..name.len() - extension.len()];
        }
    }
    name
}

fn new_file_name(name: &str) -> String {
    if bare_name(name) == name {
        to_file_name(&format!("{name}.met"))
    } else {
        to_file_name(name)
    }
}

fn is_dropped_foreign_format(key: &str) -> bool {
    ordinal::ends_with_ignore_case(key, ".met")
}

/// Where a meta dropped in as the older `.met` form sits.
fn dropped_file_name(bare_name: &str) -> String {
    format!("{META_FOLDER}/{bare_name}.met")
}

fn normalize(name: Option<&str>) -> String {
    name.map(str::trim).unwrap_or_default().to_owned()
}

fn legacy_named_key(name: &str) -> String {
    format!(
        "profiles/meta/named/{}.json",
        short_hash(&casing::to_lower_invariant(name))
    )
}

fn name_of(file_name: &str, dropped_exists: &mut dyn FnMut(&str) -> bool) -> String {
    if ordinal::equals_ignore_case(file_name, BY_CHARACTER) {
        return file_name.to_owned();
    }
    let value = file_name
        .strip_prefix(&folder_prefix())
        .unwrap_or(file_name);
    if ordinal::ends_with_ignore_case(value, ".met") {
        return value[..value.len() - 4].to_owned();
    }
    if !ordinal::ends_with_ignore_case(value, ".af") {
        return value.to_owned();
    }
    let bare = &value[..value.len() - 3];
    if dropped_exists(bare) {
        value.to_owned()
    } else {
        bare.to_owned()
    }
}

/// A meta as the C#'s JSON round trip copies it: everything but an
/// embedded route's waypoints, which System.Text.Json does not read back
/// into the getter-only list they live in.
#[must_use]
pub fn json_clone(profile: &MetaProfile) -> MetaProfile {
    fn strip(action: &mut MetaAction) {
        if let Some(route) = &mut action.embedded_route {
            route.waypoints.clear();
        }
        for child in &mut action.children {
            strip(child);
        }
    }
    let mut copy = profile.clone();
    for rule in &mut copy.rules {
        strip(&mut rule.action);
    }
    copy
}

/// The meta store (`MossTankMetaProfileStore`).
#[derive(Debug, Clone)]
pub struct MetaProfileStore {
    character: String,
    selected: String,
    pending_legacy_bare_name: Option<String>,
    roster_swept: bool,
    server: String,
    recovery_notice: Option<String>,
    last_load_key: Option<String>,
    last_load_succeeded: bool,
    last_load_error: Option<String>,
    save_notice: Option<String>,
}

impl Default for MetaProfileStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MetaProfileStore {
    /// A store bound to nobody, `By char` selected.
    #[must_use]
    pub fn new() -> Self {
        Self {
            character: String::new(),
            selected: BY_CHARACTER.to_owned(),
            pending_legacy_bare_name: None,
            roster_swept: false,
            server: String::new(),
            recovery_notice: None,
            last_load_key: None,
            last_load_succeeded: false,
            last_load_error: None,
            save_notice: None,
        }
    }

    /// The selected meta's name.
    ///
    /// # Errors
    /// A storage failure.
    pub fn selected(&self, host: &mut dyn StoreHost) -> Result<String, StoreError> {
        self.name_of_key(host, &self.selected)
    }

    /// What was said when a meta had to be preserved.
    #[must_use]
    pub fn recovery_notice(&self) -> Option<&str> {
        self.recovery_notice.as_deref()
    }

    /// The file the last load read, or tried to read.
    #[must_use]
    pub fn last_load_key(&self) -> Option<&str> {
        self.last_load_key.as_deref()
    }

    /// Whether the last load found and read its file.
    #[must_use]
    pub fn last_load_succeeded(&self) -> bool {
        self.last_load_succeeded
    }

    /// Why the last load could not read its file.
    #[must_use]
    pub fn last_load_error(&self) -> Option<&str> {
        self.last_load_error.as_deref()
    }

    /// Why the last save wrote nothing.
    #[must_use]
    pub fn save_notice(&self) -> Option<&str> {
        self.save_notice.as_deref()
    }

    fn can_bind_files(&self) -> bool {
        !self.character.is_empty() && !self.server.is_empty()
    }

    fn name_of_key(&self, host: &mut dyn StoreHost, file_name: &str) -> Result<String, StoreError> {
        let mut failure = None;
        let name = name_of(file_name, &mut |bare| {
            let storage = host.storage(Root::Profiles);
            if !storage.is_available() {
                return false;
            }
            match storage.read_text(&dropped_file_name(bare)) {
                Ok(text) => text.is_some(),
                Err(error) => {
                    failure = Some(error);
                    false
                }
            }
        });
        match failure {
            Some(error) => Err(error),
            None => Ok(name),
        }
    }

    /// The picker's names: `By char`, then every meta by the name that
    /// loads it.
    ///
    /// # Errors
    /// The metas folder cannot be listed.
    pub fn available_names(&self, host: &mut dyn StoreHost) -> Result<Vec<String>, StoreError> {
        let entries = directory::list_meta_profiles(host.storage(Root::Profiles))?;
        let listed: Vec<String> = entries
            .iter()
            .map(|entry| entry.file_name.clone())
            .collect();
        let mut names = vec![BY_CHARACTER.to_owned()];
        for entry in &entries {
            if entry.file_name.is_empty() {
                continue;
            }
            names.push(name_of(&entry.file_name, &mut |bare| {
                let dropped = dropped_file_name(bare);
                listed
                    .iter()
                    .any(|known| ordinal::equals_ignore_case(known, &dropped))
            }));
        }
        Ok(names)
    }

    /// Binds the store to a character on the current world (a partial
    /// identity never replaces an established one). Whether it changed.
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
        if ordinal::equals_ignore_case(&normalized, &self.character)
            && ordinal::equals_ignore_case(&server, &self.server)
        {
            return Ok(false);
        }
        self.character = normalized;
        self.server = server;
        self.pending_legacy_bare_name = None;
        let binding = if self.can_bind_files() {
            directory::try_read_character_binding(
                host.storage(Root::Profiles),
                &self.character,
                &self.server,
            )?
        } else {
            None
        };
        self.selected = match binding.and_then(|bound| bound.meta_file_name) {
            Some(meta) if !meta.is_empty() => meta,
            _ => BY_CHARACTER.to_owned(),
        };
        Ok(true)
    }

    /// Loads the selected meta in its file's own form (migrating an old JSON
    /// meta first); an empty meta when there is none or it will not read.
    ///
    /// # Errors
    /// A storage failure.
    pub fn load_current(
        &mut self,
        host: &mut dyn StoreHost,
        spells: Option<&dyn SpellLookup>,
    ) -> Result<MetaProfile, StoreError> {
        self.sweep_legacy_roster_if_needed(host)?;
        self.migrate_legacy_if_needed(host)?;
        let file_name = self.current_file_name();
        self.last_load_key = Some(file_name.clone());
        self.last_load_succeeded = false;
        self.last_load_error = None;
        let text = {
            let storage = host.storage(Root::Profiles);
            if storage.is_available() {
                storage.read_text(&file_name)?
            } else {
                None
            }
        };
        let Some(text) = text else {
            return Ok(MetaProfile::default());
        };
        let loaded = if is_dropped_foreign_format(&file_name) {
            met::try_load(&text, spells)
        } else {
            af::try_load_meta(&text, spells)
        };
        match loaded {
            Ok(profile) => {
                self.last_load_succeeded = true;
                Ok(profile)
            }
            Err(error) => {
                let notice = recovery::preserve(host, "meta", &file_name, Some(&text), &error);
                host.log().warn(&notice);
                self.recovery_notice = Some(notice);
                self.last_load_error = Some(error);
                Ok(MetaProfile::default())
            }
        }
    }

    fn files_under_nobody(&self) -> bool {
        self.character.is_empty() && ordinal::equals_ignore_case(&self.selected, BY_CHARACTER)
    }

    /// Writes the meta back to the file it was loaded from, in that file's
    /// own form; false, with [`Self::save_notice`] saying why, when nothing
    /// was written.
    pub fn save_current(&mut self, host: &mut dyn StoreHost, profile: &MetaProfile) -> bool {
        if self.files_under_nobody() {
            return false;
        }
        let file_name = self.current_file_name();
        self.save_to(host, &file_name, profile).is_ok()
    }

    /// Selects a meta by name (with `exact_only`, a name that carries its
    /// extension reaches that file or nothing); an old JSON meta of that
    /// name is migrated by the next load. Whether there was one.
    ///
    /// # Errors
    /// A storage failure.
    pub fn select(
        &mut self,
        host: &mut dyn StoreHost,
        name: Option<&str>,
        exact_only: bool,
    ) -> Result<bool, StoreError> {
        let normalized = normalize(name);
        if normalized.is_empty() {
            return Ok(false);
        }
        if ordinal::equals_ignore_case(&normalized, BY_CHARACTER) {
            self.selected = BY_CHARACTER.to_owned();
            self.pending_legacy_bare_name = None;
            self.write_binding(host)?;
            return Ok(true);
        }
        if let Some(found) = self.resolve_existing(host, &normalized, exact_only)? {
            self.selected = found;
            self.pending_legacy_bare_name = None;
            self.write_binding(host)?;
            return Ok(true);
        }
        let bare = bare_name(&normalized).to_owned();
        let plugin = host.storage(Root::Plugin);
        if plugin.is_available() && plugin.read_text(&legacy_named_key(&bare))?.is_some() {
            self.selected = to_file_name(&bare);
            self.pending_legacy_bare_name = Some(bare);
            self.write_binding(host)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Whether a meta of that name exists (or an old JSON meta).
    ///
    /// # Errors
    /// A storage failure.
    pub fn exists(&self, host: &mut dyn StoreHost, name: Option<&str>) -> Result<bool, StoreError> {
        let normalized = normalize(name);
        if normalized.is_empty() {
            return Ok(false);
        }
        if ordinal::equals_ignore_case(&normalized, BY_CHARACTER) {
            return Ok(true);
        }
        if self.resolve_existing(host, &normalized, false)?.is_some() {
            return Ok(true);
        }
        let plugin = host.storage(Root::Plugin);
        Ok(plugin.is_available()
            && plugin
                .read_text(&legacy_named_key(bare_name(&normalized)))?
                .is_some())
    }

    /// Writes a meta under a new name (a bare name as a `.met`) and selects
    /// it. The notice says what happened.
    ///
    /// # Errors
    /// A storage failure.
    pub fn create(
        &mut self,
        host: &mut dyn StoreHost,
        name: Option<&str>,
        copy_current: bool,
        current: &MetaProfile,
    ) -> Result<(bool, String), StoreError> {
        let normalized = normalize(name);
        let length = openac_plugin_compat::text::utf16::len(&normalized);
        if !(1..=64).contains(&length) || ordinal::equals_ignore_case(&normalized, BY_CHARACTER) {
            return Ok((
                false,
                "Enter a unique Meta profile name (1-64 characters).".to_owned(),
            ));
        }
        let file_name = new_file_name(&normalized);
        let document = if copy_current {
            json_clone(current)
        } else {
            MetaProfile::default()
        };
        if let Err(notice) = self.save_to(host, &file_name, &document) {
            return Ok((false, notice));
        }
        self.selected = file_name.clone();
        self.pending_legacy_bare_name = None;
        self.write_binding(host)?;
        let shown = self.name_of_key(host, &file_name)?;
        Ok((
            true,
            if copy_current {
                format!("Copied Meta profile to {shown}.")
            } else {
                format!("Created Meta profile {shown}.")
            },
        ))
    }

    fn find_legacy_import(
        host: &mut dyn StoreHost,
        normalized: &str,
    ) -> Result<Option<String>, StoreError> {
        let plugin = host.storage(Root::Plugin);
        let mut candidates = plugin.list("imports")?;
        candidates.extend(plugin.list("exports")?);
        Ok(candidates.into_iter().find(|candidate| {
            let file = candidate
                .rfind(['/', '\\'])
                .map_or(candidate.as_str(), |at| &candidate[at + 1..]);
            let stem = file.rfind('.').map_or(file, |dot| &file[..dot]);
            ordinal::ends_with_ignore_case(candidate, ".met")
                && ordinal::equals_ignore_case(stem, normalized)
        }))
    }

    /// Whether a VTank `.met` of this name waits in imports or exports.
    ///
    /// # Errors
    /// A storage failure.
    pub fn legacy_import_exists(
        &self,
        host: &mut dyn StoreHost,
        name: Option<&str>,
    ) -> Result<bool, StoreError> {
        let normalized = normalize(name);
        Ok(host.storage(Root::Plugin).is_available()
            && !normalized.is_empty()
            && Self::find_legacy_import(host, &normalized)?.is_some())
    }

    /// Imports a VTank `.met` from the plugin's `imports` (or `exports`)
    /// folder as a meta of that name and selects it: the meta read and the
    /// notice.
    ///
    /// # Errors
    /// A storage failure.
    pub fn try_import_legacy(
        &mut self,
        host: &mut dyn StoreHost,
        name: Option<&str>,
        spells: Option<&dyn SpellLookup>,
    ) -> Result<(bool, MetaProfile, String), StoreError> {
        let normalized = normalize(name);
        if !host.storage(Root::Plugin).is_available() || normalized.is_empty() {
            return Ok((
                false,
                MetaProfile::default(),
                "Legacy Meta storage is unavailable.".to_owned(),
            ));
        }
        let key = Self::find_legacy_import(host, &normalized)?;
        let source = match &key {
            Some(key) => host.storage(Root::Plugin).read_text(key)?,
            None => None,
        };
        let Some(source) = source.filter(|text| !casing::is_white_space_only(text)) else {
            return Ok((
                false,
                MetaProfile::default(),
                format!("VTank Meta file '{normalized}.met' was not found in imports."),
            ));
        };
        let profile = match met::try_load(&source, spells) {
            Ok(profile) => profile,
            Err(error) => {
                let key = key.unwrap_or_default();
                let file = key
                    .rfind(['/', '\\'])
                    .map_or(key.as_str(), |at| &key[at + 1..]);
                return Ok((
                    false,
                    MetaProfile::default(),
                    format!("Could not import {file}: {error}"),
                ));
            }
        };
        let file_name = to_file_name(&normalized);
        if let Err(notice) = self.save_to(host, &file_name, &profile) {
            return Ok((false, profile, notice));
        }
        self.selected = file_name.clone();
        self.pending_legacy_bare_name = None;
        self.write_binding(host)?;
        let shown = self.name_of_key(host, &file_name)?;
        Ok((
            true,
            profile,
            format!("Imported VTank Meta profile {shown}."),
        ))
    }

    /// Deletes the selected named meta; `By char` comes next.
    ///
    /// # Errors
    /// A storage failure.
    pub fn delete(&mut self, host: &mut dyn StoreHost) -> Result<(bool, String), StoreError> {
        if ordinal::equals_ignore_case(&self.selected, BY_CHARACTER) {
            return Ok((
                false,
                "'By char' is the built-in Meta profile and cannot be deleted.".to_owned(),
            ));
        }
        let file_name = self.selected.clone();
        let storage = host.storage(Root::Profiles);
        if storage.is_available() {
            storage.delete(&file_name)?;
        }
        self.selected = BY_CHARACTER.to_owned();
        self.pending_legacy_bare_name = None;
        self.write_binding(host)?;
        let shown = self.name_of_key(host, &file_name)?;
        Ok((true, format!("Deleted Meta profile {shown}.")))
    }

    /// Saves an empty meta over the selected one, and returns it.
    pub fn clear_current(&mut self, host: &mut dyn StoreHost) -> MetaProfile {
        let empty = MetaProfile::default();
        self.save_current(host, &empty);
        empty
    }

    // ---- Legacy JSON -> .af ------------------------------------------------

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
        let names = self.read_roster_names(host);
        let Some(names) = names.filter(|names| !names.is_empty()) else {
            return Ok(());
        };
        let mut remaining: Vec<String> = Vec::new();
        let mut migrated = 0;
        for raw in names {
            let name = raw.as_deref().unwrap_or_default().trim().to_owned();
            if name.is_empty() || ordinal::equals_ignore_case(&name, BY_CHARACTER) {
                continue; // a stale row; dropped rather than looped on.
            }
            let legacy_key = legacy_named_key(&name);
            let Some(legacy) = self.read_legacy_json(host, &legacy_key) else {
                continue;
            };
            let bare = if ordinal::ends_with_ignore_case(&name, ".af") {
                name.clone()
            } else {
                format!("{name}.af")
            };
            let file_name = format!("{META_FOLDER}/{bare}");
            let storage = host.storage(Root::Profiles);
            if storage.is_available()
                && storage.read_text(&file_name)?.is_none()
                && let Err(notice) = self.save_to(host, &file_name, &legacy)
            {
                // A switched-off rule cannot go into the file: the row is
                // kept so this can be tried again once it is resolved.
                host.log().warn(&format!(
                    "MossTank could not sweep legacy Meta profile '{name}': {notice}"
                ));
                remaining.push(name);
                continue;
            }
            host.storage(Root::Plugin).delete(&legacy_key)?;
            migrated += 1;
        }
        if migrated == 0 {
            return Ok(());
        }
        if remaining.is_empty() {
            host.storage(Root::Plugin).delete(LEGACY_ROSTER_KEY)?;
        } else {
            let document = JsonValue::object([(
                "Names",
                JsonValue::Array(remaining.into_iter().map(JsonValue::String).collect()),
            )]);
            host.storage(Root::Plugin)
                .write_text(LEGACY_ROSTER_KEY, &crate::stj::write(&document))?;
        }
        host.log().warn(&format!(
            "Migrated {migrated} legacy MossTank named Meta profile(s) from the old roster."
        ));
        Ok(())
    }

    fn read_roster_names(&mut self, host: &mut dyn StoreHost) -> Option<Vec<Option<String>>> {
        let failed = match host.storage(Root::Plugin).read_text(LEGACY_ROSTER_KEY) {
            Ok(text) => {
                let text = text.filter(|text| !casing::is_white_space_only(text))?;
                match read_roster(&text) {
                    Ok(names) => return names,
                    Err(error) => (Some(text), error),
                }
            }
            Err(error) => (None, error),
        };
        let notice = recovery::preserve(
            host,
            "meta",
            LEGACY_ROSTER_KEY,
            failed.0.as_deref(),
            &failed.1,
        );
        host.log().warn(&notice);
        self.recovery_notice = Some(notice);
        None
    }

    fn migrate_legacy_if_needed(&mut self, host: &mut dyn StoreHost) -> Result<(), StoreError> {
        let file_name = self.current_file_name();
        let storage = host.storage(Root::Profiles);
        if !storage.is_available() || storage.read_text(&file_name)?.is_some() {
            self.pending_legacy_bare_name = None;
            return Ok(());
        }
        let legacy_key = match &self.pending_legacy_bare_name {
            Some(bare) if !bare.is_empty() => legacy_named_key(bare),
            _ => format!(
                "profiles/meta/by-character/{}.json",
                short_hash(&casing::to_lower_invariant(&self.character))
            ),
        };
        let Some(legacy) = self.read_legacy_json(host, &legacy_key) else {
            self.pending_legacy_bare_name = None;
            return Ok(());
        };
        if let Err(notice) = self.save_to(host, &file_name, &legacy) {
            // A switched-off rule blocks the migration rather than being
            // dropped: the JSON stays until it is resolved.
            host.log().warn(&format!(
                "MossTank could not migrate legacy Meta profile: {notice}"
            ));
            self.pending_legacy_bare_name = None;
            return Ok(());
        }
        let plugin = host.storage(Root::Plugin);
        if plugin.is_available() {
            plugin.delete(&legacy_key)?;
        }
        self.pending_legacy_bare_name = None;
        host.log().warn(&format!(
            "Migrated legacy MossTank Meta profile '{legacy_key}' to '{file_name}'."
        ));
        Ok(())
    }

    fn read_legacy_json(&mut self, host: &mut dyn StoreHost, key: &str) -> Option<MetaProfile> {
        let storage = host.storage(Root::Plugin);
        if !storage.is_available() {
            return None;
        }
        let failed = match storage.read_text(key) {
            Ok(text) => {
                let text = text.filter(|text| !casing::is_white_space_only(text))?;
                match read_legacy_meta(&text) {
                    Ok(profile) => return profile,
                    Err(error) => (Some(text), error),
                }
            }
            Err(error) => (None, error),
        };
        let notice = recovery::preserve(host, "meta", key, failed.0.as_deref(), &failed.1);
        host.log().error(&notice);
        self.recovery_notice = Some(notice);
        None
    }

    /// Writes a meta in its file's own form with the file's line breaks; a
    /// meta the form cannot hold is not written, and the notice says why.
    fn save_to(
        &mut self,
        host: &mut dyn StoreHost,
        file_name: &str,
        profile: &MetaProfile,
    ) -> Result<(), String> {
        let text = if is_dropped_foreign_format(file_name) {
            met::save(profile)
        } else {
            af::save_meta(profile, false)
        };
        let text = match text {
            Ok(text) => text,
            Err(error) => {
                let notice = format!("Meta profile '{file_name}' was NOT saved: {error}");
                host.log().warn(&notice);
                self.save_notice = Some(notice.clone());
                return Err(notice);
            }
        };
        if !host.storage(Root::Profiles).is_available() {
            return Err("VTank Meta storage is unavailable.".to_owned());
        }
        let written = host
            .storage(Root::Profiles)
            .read_text(file_name)
            .and_then(|existing| {
                let text = line_endings::match_existing(&text, existing.as_deref());
                host.storage(Root::Profiles).write_text(file_name, &text)
            });
        if let Err(error) = written {
            let notice = format!("Meta profile '{file_name}' could not be saved: {error}");
            host.log().warn(&notice);
            self.save_notice = Some(notice.clone());
            return Err(notice);
        }
        self.save_notice = None;
        Ok(())
    }

    // ---- File naming and storage -------------------------------------------

    /// The selected meta's key (`By char` names the character's own `.af`).
    #[must_use]
    pub fn current_file_name(&self) -> String {
        if ordinal::equals_ignore_case(&self.selected, BY_CHARACTER) {
            format!(
                "{META_FOLDER}/{}",
                directory::auto_character_file_name(&self.character, &self.server, "af")
            )
        } else {
            self.selected.clone()
        }
    }

    fn resolve_existing(
        &self,
        host: &mut dyn StoreHost,
        name: &str,
        exact_only: bool,
    ) -> Result<Option<String>, StoreError> {
        let storage = host.storage(Root::Profiles);
        if !storage.is_available() {
            return Ok(None);
        }
        let bare = bare_name(name);
        if bare != name {
            let exact = to_file_name(name);
            if storage.read_text(&exact)?.is_some() {
                return Ok(Some(exact));
            }
            if exact_only {
                return Ok(None);
            }
        }
        let dropped = dropped_file_name(bare);
        if storage.read_text(&dropped)?.is_some() {
            return Ok(Some(dropped));
        }
        let own = to_file_name(bare);
        Ok(storage.read_text(&own)?.is_some().then_some(own))
    }

    fn write_binding(&self, host: &mut dyn StoreHost) -> Result<(), StoreError> {
        let storage = host.storage(Root::Profiles);
        if !self.can_bind_files() || !storage.is_available() {
            return Ok(());
        }
        let current = self.current_file_name();
        let existing =
            directory::try_read_character_binding(storage, &self.character, &self.server)?
                .unwrap_or(VtankCharacterBinding {
                    settings_file_name: String::new(),
                    loot_file_name: String::new(),
                    nav_file_name: String::new(),
                    meta_file_name: Some(current.clone()),
                });
        directory::write_character_binding(
            storage,
            &self.character,
            &self.server,
            &VtankCharacterBinding {
                meta_file_name: Some(current),
                ..existing
            },
        )
    }
}

fn read_roster(text: &str) -> Result<Option<Vec<Option<String>>>, JsonError> {
    let document = json::parse(text).map_err(|error| error.to_string())?;
    let Some(o) = Object::of(&document, "$", "LegacyRosterDocument")? else {
        return Ok(None);
    };
    o.strings("Names")
}

/// An enum number this build has no member for (register row STORE-5).
fn unknown(path: &str, number: i32) -> JsonError {
    format!("The JSON value could not be converted to an enum: {number} at {path}")
}

fn read_condition(o: &Object<'_>) -> Result<MetaCondition, JsonError> {
    let number = o.int("Kind", 1)?;
    let kind = usize::try_from(number)
        .ok()
        .and_then(|index| MetaConditionKind::ALL.get(index).copied())
        .ok_or_else(|| unknown("Condition.Kind", number))?;
    let children = match o.objects("Children", Some(Vec::new()), "MetaCondition")? {
        None => Vec::new(),
        Some(items) => items
            .iter()
            .flatten()
            .map(read_condition)
            .collect::<Result<_, _>>()?,
    };
    Ok(MetaCondition {
        kind,
        text: o.string("Text", "")?.unwrap_or_default(),
        secondary_text: o.string("SecondaryText", "")?.unwrap_or_default(),
        number: o.double("Number", 0.0)?,
        secondary_number: o.double("SecondaryNumber", 0.0)?,
        tertiary_number: o.double("TertiaryNumber", 0.0)?,
        children,
    })
}

fn read_position(value: Option<&JsonValue>, path: &str) -> Result<Position, JsonError> {
    let Some(value) = value else {
        return Ok(Position::default());
    };
    let Some(o) = Object::of(value, path, "PluginNavigationPosition")? else {
        return Err(format!(
            "The JSON value could not be converted to PluginNavigationPosition. Path: {path}"
        ));
    };
    Ok(Position {
        cell_id: o.uint("CellId", 0)?,
        east_west: o.double("EastWest", 0.0)?,
        north_south: o.double("NorthSouth", 0.0)?,
        elevation: o.double("Elevation", 0.0)?,
        heading_degrees: o.float("HeadingDegrees", 0.0)?,
        is_outdoor: o.bool("IsOutdoor", false)?,
    })
}

/// An embedded route as System.Text.Json reads `NavigationSettings`: its
/// waypoints live in a getter-only list, so they are not read back.
fn read_route(o: &Object<'_>) -> Result<NavigationSettings, JsonError> {
    let defaults = NavigationSettings::default();
    let mode = match o.int("Mode", 0)? {
        0 => RouteMode::Circular,
        1 => RouteMode::Linear,
        2 => RouteMode::Target,
        3 => RouteMode::Once,
        other => return Err(unknown("EmbeddedRoute.Mode", other)),
    };
    let client_pathing = match o.int("ClientPathing", 1)? {
        0 => ClientPathing::Never,
        1 => ClientPathing::WhenStuck,
        2 => ClientPathing::Always,
        other => return Err(unknown("EmbeddedRoute.ClientPathing", other)),
    };
    // Read for their kind only: System.Text.Json does not set the getter-only
    // list they live in, so an embedded route comes back without them.
    if let Some(items) = o.objects("Waypoints", Some(Vec::new()), "RouteWaypoint")? {
        for item in items.iter().flatten() {
            read_waypoint(item)?;
        }
    }
    Ok(NavigationSettings {
        show_nav_lines: o.bool("ShowNavLines", false)?,
        enabled: o.bool("Enabled", false)?,
        priority: o.bool("Priority", false)?,
        mode,
        minimum_distance_meters: o
            .double("MinimumDistanceMeters", defaults.minimum_distance_meters)?,
        maximum_distance_meters: o
            .double("MaximumDistanceMeters", defaults.maximum_distance_meters)?,
        portal_use_distance_meters: o.double(
            "PortalUseDistanceMeters",
            defaults.portal_use_distance_meters,
        )?,
        follow_target_object_id: o.uint("FollowTargetObjectId", 0)?,
        follow_target_name: o.string("FollowTargetName", "")?.unwrap_or_default(),
        follow_around_corners: o.bool("FollowAroundCorners", true)?,
        client_pathing,
        open_doors: o.bool("OpenDoors", false)?,
        door_identify_range_meters: o.double(
            "DoorIdentifyRangeMeters",
            defaults.door_identify_range_meters,
        )?,
        door_open_range_meters: o.double("DoorOpenRangeMeters", defaults.door_open_range_meters)?,
        door_lockpick_excess_threshold: o.int(
            "DoorLockpickExcessThreshold",
            defaults.door_lockpick_excess_threshold,
        )?,
        jump_attempts: o.int("JumpAttempts", defaults.jump_attempts)?,
        waypoints: Vec::new(),
    })
}

fn read_waypoint(o: &Object<'_>) -> Result<RouteWaypoint, JsonError> {
    let kind_number = o.int("Type", 0)?;
    let recall_number = o.int("Recall", 0)?;
    let direction_number = o.int("JumpDirection", 0)?;
    Ok(RouteWaypoint {
        kind: RouteWaypointType::from_i32(kind_number)
            .ok_or_else(|| unknown("Waypoints.Type", kind_number))?,
        position: read_position(o.get("Position"), "Position")?,
        reference_position: read_position(o.get("ReferencePosition"), "ReferencePosition")?,
        object_id: o.uint("ObjectId", 0)?,
        object_name: o.string("ObjectName", "")?.unwrap_or_default(),
        legacy_object_class: o.int("LegacyObjectClass", 0)?,
        legacy_reference_valid: o.bool("LegacyReferenceValid", true)?,
        text: o.string("Text", "")?.unwrap_or_default(),
        duration_milliseconds: o.int("DurationMilliseconds", 5000)?,
        recall: usize::try_from(recall_number)
            .ok()
            .and_then(|index| RouteRecallKind::ALL.get(index).copied())
            .ok_or_else(|| unknown("Waypoints.Recall", recall_number))?,
        recall_spell_id: o.uint("RecallSpellId", 0)?,
        recall_spell_name: o.string("RecallSpellName", "")?.unwrap_or_default(),
        jump_heading_degrees: o.double("JumpHeadingDegrees", 0.0)?,
        jump_hold_shift: o.bool("JumpHoldShift", false)?,
        jump_charge_milliseconds: o.int("JumpChargeMilliseconds", 1000)?,
        jump_direction: match direction_number {
            0 => RouteJumpDirection::Forward,
            1 => RouteJumpDirection::StrafeLeft,
            2 => RouteJumpDirection::StrafeRight,
            3 => RouteJumpDirection::Backward,
            other => return Err(unknown("Waypoints.JumpDirection", other)),
        },
    })
}

fn read_action(o: &Object<'_>) -> Result<MetaAction, JsonError> {
    let number = o.int("Kind", 0)?;
    let kind = usize::try_from(number)
        .ok()
        .and_then(|index| MetaActionKind::ALL.get(index).copied())
        .ok_or_else(|| unknown("Action.Kind", number))?;
    let children = match o.objects("Children", Some(Vec::new()), "MetaAction")? {
        None => Vec::new(),
        Some(items) => items
            .iter()
            .flatten()
            .map(read_action)
            .collect::<Result<_, _>>()?,
    };
    let embedded_route = match o.get("EmbeddedRoute") {
        None => None,
        Some(value) => match Object::of(value, "EmbeddedRoute", "NavigationSettings")? {
            None => None,
            Some(route) => Some(Box::new(read_route(&route)?)),
        },
    };
    Ok(MetaAction {
        kind,
        text: o.string("Text", "")?.unwrap_or_default(),
        secondary_text: o.string("SecondaryText", "")?.unwrap_or_default(),
        number: o.double("Number", 0.0)?,
        secondary_number: o.double("SecondaryNumber", 0.0)?,
        children,
        embedded_route,
    })
}

/// An old JSON meta (the C#'s `MetaProfile` as System.Text.Json reads it).
fn read_legacy_meta(text: &str) -> Result<Option<MetaProfile>, JsonError> {
    let document = json::parse(text).map_err(|error| error.to_string())?;
    let Some(o) = Object::of(&document, "$", "MetaProfile")? else {
        return Ok(None);
    };
    let mut rules = Vec::new();
    if let Some(items) = o.objects("Rules", Some(Vec::new()), "MetaRule")? {
        for item in items.iter().flatten() {
            if let Some(id) = item.get("Id")
                && !matches!(id, JsonValue::String(_))
            {
                return Err(
                    "The JSON value could not be converted to System.Guid. Path: $.Rules.Id"
                        .to_owned(),
                );
            }
            let condition = match item.get("Condition") {
                None => MetaCondition::always(),
                Some(value) => match Object::of(value, "Condition", "MetaCondition")? {
                    // A null condition would throw when run (register row
                    // STORE-5); it reads as the default.
                    None => MetaCondition::always(),
                    Some(condition) => read_condition(&condition)?,
                },
            };
            let action = match item.get("Action") {
                None => MetaAction::default(),
                Some(value) => match Object::of(value, "Action", "MetaAction")? {
                    None => MetaAction::default(),
                    Some(action) => read_action(&action)?,
                },
            };
            rules.push(MetaRule {
                state: item
                    .string("State", mosstank_formats::meta::DEFAULT_STATE)?
                    .unwrap_or_default(),
                condition,
                action,
                enabled: item.bool("Enabled", true)?,
            });
        }
    }
    Ok(Some(MetaProfile { rules }))
}

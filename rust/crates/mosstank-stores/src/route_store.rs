//! The route profile store: which route in `mosstank/navs/` is this
//! character's, read and written in the file's own form (MossTank's `.af`,
//! or a `.nav` dropped into the folder, which stays a `.nav`), named routes,
//! the follow route, translating a route onto another landblock, importing
//! a VTank `.nav`, and the one-time migration of older JSON routes.

use mosstank_dotnet::text::{casing, ordinal};
use mosstank_formats::af;
use mosstank_formats::line_endings;
use mosstank_formats::nav;
use mosstank_formats::route::{
    NavigationSettings, Position, RouteJumpDirection, RouteMode, RouteRecallKind, RouteWaypoint,
    RouteWaypointType, SpellLookup,
};
use openac_plugin_compat::json;

use crate::directory::{self, NAV_FOLDER, VtankCharacterBinding};
use crate::profile_store::{ProfileLoad, StoreError};
use crate::recovery::{self, sha256_hex};
use crate::stj::{JsonError, Object};
use crate::storage::{Root, StoreHost};

/// The built-in character route's name in the picker.
pub const BY_CHARACTER: &str = "By char";

/// The route a follow walks: following someone switches to it rather than
/// writing a waypoint-less route over the loaded one.
pub const FOLLOW_ROUTE_NAME: &str = "UBFollow";

const LEGACY_ROSTER_KEY: &str = "profiles/route/index.json";

fn folder_prefix() -> String {
    format!("{NAV_FOLDER}/")
}

/// Where a route dropped in as the older `.nav` form sits.
fn dropped_file_name(bare_name: &str) -> String {
    format!("{NAV_FOLDER}/{bare_name}.nav")
}

/// A route named in MossTank's own form unless the name carries `.nav` or
/// `.af`.
fn to_file_name(bare_name: &str) -> String {
    if ordinal::ends_with_ignore_case(bare_name, ".nav")
        || ordinal::ends_with_ignore_case(bare_name, ".af")
    {
        format!("{NAV_FOLDER}/{bare_name}")
    } else {
        format!("{NAV_FOLDER}/{bare_name}.af")
    }
}

/// A route's name without the extension it may carry.
#[must_use]
pub fn bare_name(name: &str) -> &str {
    for extension in [".nav", ".af"] {
        if name.len() > extension.len() && ordinal::ends_with_ignore_case(name, extension) {
            return &name[..name.len() - extension.len()];
        }
    }
    name
}

/// The file a route written under a new name goes to: the `.nav` form for
/// a bare name, exactly the file a name with its extension says.
fn new_file_name(name: &str) -> String {
    if bare_name(name) == name {
        to_file_name(&format!("{name}.nav"))
    } else {
        to_file_name(name)
    }
}

fn is_dropped_foreign_format(key: &str) -> bool {
    ordinal::ends_with_ignore_case(key, ".nav")
}

fn normalize(name: Option<&str>) -> String {
    name.map(str::trim).unwrap_or_default().to_owned()
}

fn legacy_profile_key(value: &str, by_character: bool) -> String {
    let identity = format!(
        "{}{}",
        if by_character { "char:" } else { "named:" },
        casing::to_upper_invariant(value.trim())
    );
    format!("profiles/route/{}.json", sha256_hex(&identity))
}

/// The name a file goes by, which also loads it again: a `.nav` by its bare
/// name, MossTank's `.af` too unless a `.nav` of that name sits beside it.
fn name_of(file_name: &str, dropped_exists: &mut dyn FnMut(&str) -> bool) -> String {
    if ordinal::equals_ignore_case(file_name, BY_CHARACTER) {
        return file_name.to_owned();
    }
    let value = file_name
        .strip_prefix(&folder_prefix())
        .unwrap_or(file_name);
    if ordinal::ends_with_ignore_case(value, ".nav") {
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

/// The route store (`MossTankRouteProfileStore`).
#[derive(Debug, Clone)]
pub struct RouteProfileStore {
    character_name: String,
    selected: String,
    pending_legacy_bare_name: Option<String>,
    roster_swept: bool,
    server: String,
    recovery_notice: Option<String>,
    last_load_key: Option<String>,
    save_notice: Option<String>,
}

impl Default for RouteProfileStore {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteProfileStore {
    /// A store bound to nobody, `By char` selected.
    #[must_use]
    pub fn new() -> Self {
        Self {
            character_name: String::new(),
            selected: BY_CHARACTER.to_owned(),
            pending_legacy_bare_name: None,
            roster_swept: false,
            server: String::new(),
            recovery_notice: None,
            last_load_key: None,
            save_notice: None,
        }
    }

    /// The selected route's name.
    ///
    /// # Errors
    /// A storage failure.
    pub fn selected(&self, host: &mut dyn StoreHost) -> Result<String, StoreError> {
        self.name_of_key(host, &self.selected)
    }

    /// The selected file's key, as kept (what [`Self::selected`] names).
    #[must_use]
    pub fn selected_key(&self) -> &str {
        &self.selected
    }

    /// What was said when a route had to be preserved.
    #[must_use]
    pub fn recovery_notice(&self) -> Option<&str> {
        self.recovery_notice.as_deref()
    }

    /// The file the last load read, or tried to read.
    #[must_use]
    pub fn last_load_key(&self) -> Option<&str> {
        self.last_load_key.as_deref()
    }

    /// Why the last save wrote nothing.
    #[must_use]
    pub fn save_notice(&self) -> Option<&str> {
        self.save_notice.as_deref()
    }

    fn can_bind_files(&self) -> bool {
        !self.character_name.is_empty() && !self.server.is_empty()
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

    /// The picker's names: `By char`, then every route by the name that
    /// loads it.
    ///
    /// # Errors
    /// The routes folder cannot be listed.
    pub fn available_names(&self, host: &mut dyn StoreHost) -> Result<Vec<String>, StoreError> {
        let entries = directory::list_navigation_profiles(host.storage(Root::Profiles))?;
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
        if ordinal::equals_ignore_case(&normalized, &self.character_name)
            && ordinal::equals_ignore_case(&server, &self.server)
        {
            return Ok(false);
        }
        self.character_name = normalized;
        self.server = server;
        self.pending_legacy_bare_name = None;
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
            Some(bound) if !bound.nav_file_name.is_empty() => bound.nav_file_name,
            _ => BY_CHARACTER.to_owned(),
        };
        Ok(true)
    }

    /// Selects a route by name (with `exact_only`, a name that carries its
    /// extension reaches that file or nothing); an old JSON route of that
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
        if plugin.is_available()
            && plugin
                .read_text(&legacy_profile_key(&bare, false))?
                .is_some()
        {
            self.selected = to_file_name(&bare);
            self.pending_legacy_bare_name = Some(bare);
            self.write_binding(host)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Whether the follow route is the selected route.
    #[must_use]
    pub fn follow_route_selected(&self) -> bool {
        ordinal::equals_ignore_case(&self.selected, &to_file_name(FOLLOW_ROUTE_NAME))
    }

    /// Selects the follow route, writing it empty first when it is not
    /// there; the route that was selected keeps its file as it is.
    ///
    /// # Errors
    /// A storage failure.
    pub fn select_follow_route(&mut self, host: &mut dyn StoreHost) -> Result<(), StoreError> {
        let file_name = to_file_name(FOLLOW_ROUTE_NAME);
        let storage = host.storage(Root::Profiles);
        if storage.is_available() && storage.read_text(&file_name)?.is_none() {
            self.write_route(host, &file_name, &NavigationSettings::default());
        }
        self.selected = file_name;
        self.pending_legacy_bare_name = None;
        self.write_binding(host)
    }

    /// Whether a route of that name exists (or an old JSON route).
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
                .read_text(&legacy_profile_key(bare_name(&normalized), false))?
                .is_some())
    }

    /// Writes a route under a new name (a bare name as a `.nav`) and
    /// selects it. The notice says what happened.
    ///
    /// # Errors
    /// A storage failure.
    pub fn create(
        &mut self,
        host: &mut dyn StoreHost,
        name: Option<&str>,
        copy_current: bool,
        current: &NavigationSettings,
    ) -> Result<(bool, String), StoreError> {
        let normalized = normalize(name);
        let length = openac_plugin_compat::text::utf16::len(&normalized);
        if !(1..=64).contains(&length) {
            return Ok((
                false,
                "Enter a route profile name (1-64 characters).".to_owned(),
            ));
        }
        if ordinal::equals_ignore_case(&normalized, BY_CHARACTER) {
            return Ok((false, "'By char' is the built-in route profile.".to_owned()));
        }
        let file_name = new_file_name(&normalized);
        let fresh = NavigationSettings::default();
        let source = if copy_current { current } else { &fresh };
        if !self.write_route(host, &file_name, source) {
            return Ok((
                false,
                self.save_notice
                    .clone()
                    .unwrap_or_else(|| "Route storage is unavailable.".to_owned()),
            ));
        }
        self.selected = file_name.clone();
        self.pending_legacy_bare_name = None;
        self.write_binding(host)?;
        let shown = self.name_of_key(host, &file_name)?;
        Ok((
            true,
            if copy_current {
                format!("Copied route to {shown}.")
            } else {
                format!("Created route profile {shown}.")
            },
        ))
    }

    /// Loads the selected route into `target` in its file's own form (and
    /// migrates an old JSON route first); a route that will not read is
    /// preserved and nothing is applied.
    ///
    /// # Errors
    /// A storage failure.
    pub fn load_current(
        &mut self,
        host: &mut dyn StoreHost,
        target: &mut NavigationSettings,
        spells: Option<&dyn SpellLookup>,
    ) -> Result<ProfileLoad, StoreError> {
        self.sweep_legacy_roster_if_needed(host)?;
        self.migrate_legacy_if_needed(host, target)?;
        let file_name = self.current_file_name();
        self.last_load_key = Some(file_name.clone());
        let text = {
            let storage = host.storage(Root::Profiles);
            if storage.is_available() {
                storage.read_text(&file_name)?
            } else {
                None
            }
        };
        let Some(text) = text else {
            return Ok(ProfileLoad::Missing);
        };
        let loaded = if is_dropped_foreign_format(&file_name) {
            nav::try_load(&text, target, spells)
        } else {
            af::try_load_nav(&text, target, spells)
        };
        match loaded {
            Ok(()) => Ok(ProfileLoad::Loaded),
            Err(error) => {
                let notice = recovery::preserve(host, "route", &file_name, Some(&text), &error);
                host.log().warn(&notice);
                self.recovery_notice = Some(notice);
                Ok(ProfileLoad::Failed)
            }
        }
    }

    /// The character's own route of a character nobody has named yet does
    /// not exist.
    fn files_under_nobody(&self) -> bool {
        self.character_name.is_empty() && ordinal::equals_ignore_case(&self.selected, BY_CHARACTER)
    }

    /// Writes the route back to the file it was loaded from, in that file's
    /// own form; false, with [`Self::save_notice`] saying why, when nothing
    /// was written.
    pub fn save_current(
        &mut self,
        host: &mut dyn StoreHost,
        settings: &NavigationSettings,
    ) -> bool {
        self.save_notice = None;
        if self.files_under_nobody() {
            return false;
        }
        let file_name = self.current_file_name();
        self.write_route(host, &file_name, settings)
    }

    /// Imports a VTank `.nav` from the plugin's `imports` (or `exports`)
    /// folder as a route of that name and selects it.
    ///
    /// # Errors
    /// A storage failure.
    pub fn try_import_legacy(
        &mut self,
        host: &mut dyn StoreHost,
        name: Option<&str>,
        target: &mut NavigationSettings,
        spells: Option<&dyn SpellLookup>,
    ) -> Result<(bool, String), StoreError> {
        let normalized = normalize(name);
        let plugin = host.storage(Root::Plugin);
        if !plugin.is_available() || normalized.is_empty() {
            return Ok((
                false,
                "Legacy navigation storage is unavailable.".to_owned(),
            ));
        }
        let mut candidates = plugin.list("imports")?;
        candidates.extend(plugin.list("exports")?);
        let key = candidates.into_iter().find(|candidate| {
            let file = candidate
                .rfind(['/', '\\'])
                .map_or(candidate.as_str(), |at| &candidate[at + 1..]);
            let stem = file.rfind('.').map_or(file, |dot| &file[..dot]);
            ordinal::ends_with_ignore_case(candidate, ".nav")
                && ordinal::equals_ignore_case(stem, &normalized)
        });
        let source = match &key {
            Some(key) => plugin.read_text(key)?,
            None => None,
        };
        let Some(source) = source.filter(|text| !casing::is_white_space_only(text)) else {
            return Ok((
                false,
                format!("VTank navigation file '{normalized}.nav' was not found in imports."),
            ));
        };
        if let Err(error) = nav::try_load(&source, target, spells) {
            let key = key.unwrap_or_default();
            let file = key
                .rfind(['/', '\\'])
                .map_or(key.as_str(), |at| &key[at + 1..]);
            return Ok((false, format!("Could not import {file}: {error}")));
        }
        let file_name = to_file_name(&normalized);
        if !self.write_route(host, &file_name, target) {
            return Ok((
                false,
                self.save_notice
                    .clone()
                    .unwrap_or_else(|| "Route storage is unavailable.".to_owned()),
            ));
        }
        self.selected = file_name.clone();
        self.pending_legacy_bare_name = None;
        self.write_binding(host)?;
        let shown = self.name_of_key(host, &file_name)?;
        Ok((true, format!("Imported VTank navigation profile {shown}.")))
    }

    /// Copies a saved route onto another landblock, every point shifted by
    /// the offsets (in map units); the selection is untouched. On success
    /// the notice is the name it was saved under and the count is its
    /// waypoints; otherwise the notice says why nothing was written.
    ///
    /// # Errors
    /// A storage failure.
    #[allow(clippy::too_many_arguments)]
    pub fn try_translate(
        &mut self,
        host: &mut dyn StoreHost,
        source_name: Option<&str>,
        target_name: Option<&str>,
        east_west_offset: f64,
        north_south_offset: f64,
        force: bool,
        spells: Option<&dyn SpellLookup>,
    ) -> Result<(bool, String, usize), StoreError> {
        let source = normalize(source_name);
        let target = normalize(target_name);
        if source.is_empty() || target.is_empty() {
            return Ok((
                false,
                "Name both the route to translate and the route to save it as.".to_owned(),
                0,
            ));
        }
        if !host.storage(Root::Profiles).is_available() {
            return Ok((false, "Route storage is unavailable.".to_owned(), 0));
        }
        let source_key = self.resolve_existing(host, &source, false)?;
        let text = match &source_key {
            Some(key) => host.storage(Root::Profiles).read_text(key)?,
            None => None,
        };
        let (Some(source_key), Some(text)) = (source_key, text) else {
            return Ok((false, format!("Could not find route to load: {source}"), 0));
        };
        let target_key = new_file_name(&target);
        if !force
            && host
                .storage(Root::Profiles)
                .read_text(&target_key)?
                .is_some()
        {
            let shown = self.name_of_key(host, &target_key)?;
            return Ok((
                false,
                format!("Output path already exists! Run with force flag to overwrite: {shown}"),
                0,
            ));
        }
        let mut scratch = NavigationSettings::default();
        let parsed = if is_dropped_foreign_format(&source_key) {
            nav::try_load(&text, &mut scratch, spells)
        } else {
            af::try_load_nav(&text, &mut scratch, spells)
        };
        if parsed.is_err() {
            return Ok((false, "Unable to parse route".to_owned(), 0));
        }
        if !scratch
            .waypoints
            .iter()
            .any(|point| point.kind == RouteWaypointType::Point)
        {
            return Ok((
                false,
                "Unable to translate route, no nav points found!".to_owned(),
                0,
            ));
        }
        let shift = |position: &mut Position| {
            position.east_west += east_west_offset;
            position.north_south += north_south_offset;
        };
        for point in &mut scratch.waypoints {
            shift(&mut point.position);
            shift(&mut point.reference_position);
        }
        if !self.write_route(host, &target_key, &scratch) {
            return Ok((
                false,
                self.save_notice
                    .clone()
                    .unwrap_or_else(|| "Route storage is unavailable.".to_owned()),
                0,
            ));
        }
        let shown = self.name_of_key(host, &target_key)?;
        Ok((true, shown, scratch.waypoints.len()))
    }

    /// Deletes the selected named route; `By char` comes next.
    ///
    /// # Errors
    /// A storage failure.
    pub fn delete(&mut self, host: &mut dyn StoreHost) -> Result<(bool, String), StoreError> {
        if ordinal::equals_ignore_case(&self.selected, BY_CHARACTER) {
            return Ok((
                false,
                "'By char' is the built-in route profile and cannot be deleted.".to_owned(),
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
        Ok((true, format!("Deleted route profile {shown}.")))
    }

    /// Resets only what the route owns (mode, waypoints, follow target);
    /// the rest belongs to the settings profile. Saves.
    pub fn clear_current(&mut self, host: &mut dyn StoreHost, target: &mut NavigationSettings) {
        target.mode = RouteMode::Circular;
        target.follow_target_object_id = 0;
        target.follow_target_name = String::new();
        target.waypoints.clear();
        self.save_current(host, target);
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
        let names = self.read_legacy(host, LEGACY_ROSTER_KEY, read_roster_names, true);
        let Some(names) = names.flatten().filter(|names| !names.is_empty()) else {
            return Ok(());
        };
        let mut migrated = 0;
        for raw in names {
            let name = raw.as_deref().unwrap_or_default().trim().to_owned();
            if name.is_empty() || ordinal::equals_ignore_case(&name, BY_CHARACTER) {
                continue; // a stale row; dropped rather than looped on.
            }
            let legacy_key = legacy_profile_key(&name, false);
            let Some(legacy) = self.read_legacy(host, &legacy_key, read_legacy_route, false) else {
                continue;
            };
            let file_name = to_file_name(&name);
            let storage = host.storage(Root::Profiles);
            if storage.is_available() && storage.read_text(&file_name)?.is_none() {
                let mut scratch = NavigationSettings::default();
                legacy.apply_route_only(&mut scratch);
                self.write_route(host, &file_name, &scratch);
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
            "Migrated {migrated} legacy MossTank named route profile(s) from the old roster."
        ));
        Ok(())
    }

    fn migrate_legacy_if_needed(
        &mut self,
        host: &mut dyn StoreHost,
        target: &mut NavigationSettings,
    ) -> Result<(), StoreError> {
        let file_name = self.current_file_name();
        let storage = host.storage(Root::Profiles);
        if !storage.is_available() || storage.read_text(&file_name)?.is_some() {
            self.pending_legacy_bare_name = None;
            return Ok(());
        }
        let legacy_key = match &self.pending_legacy_bare_name {
            Some(bare) if !bare.is_empty() => legacy_profile_key(bare, false),
            _ => legacy_profile_key(&self.character_name, true),
        };
        let Some(legacy) = self.read_legacy(host, &legacy_key, read_legacy_route, false) else {
            self.pending_legacy_bare_name = None;
            return Ok(());
        };
        legacy.apply_route_only(target);
        self.write_route(host, &file_name, target);
        let plugin = host.storage(Root::Plugin);
        if plugin.is_available() {
            plugin.delete(&legacy_key)?;
        }
        self.pending_legacy_bare_name = None;
        host.log().warn(&format!(
            "Migrated legacy MossTank route profile '{legacy_key}' to '{file_name}'."
        ));
        Ok(())
    }

    /// Reads an old JSON document; one that will not read is preserved and
    /// reads as nothing. The roster is read without the availability check
    /// its caller already made.
    fn read_legacy<T>(
        &mut self,
        host: &mut dyn StoreHost,
        key: &str,
        parse: fn(&str) -> Result<Option<T>, JsonError>,
        skip_availability: bool,
    ) -> Option<T> {
        let storage = host.storage(Root::Plugin);
        if !skip_availability && !storage.is_available() {
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
        let notice = recovery::preserve(host, "route", key, failed.0.as_deref(), &failed.1);
        host.log().warn(&notice);
        self.recovery_notice = Some(notice);
        None
    }

    // ---- File naming and storage -------------------------------------------

    /// The selected route's key (`By char` names the character's own `.af`).
    #[must_use]
    pub fn current_file_name(&self) -> String {
        if ordinal::equals_ignore_case(&self.selected, BY_CHARACTER) {
            format!(
                "{NAV_FOLDER}/{}",
                directory::auto_character_file_name(&self.character_name, &self.server, "af")
            )
        } else {
            self.selected.clone()
        }
    }

    /// The file a name stands for, if one is there: a name with its
    /// extension that exact file first; otherwise (or when it is not there,
    /// unless `exact_only`) the dropped `.nav`, then MossTank's `.af`.
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
            let exact = format!("{NAV_FOLDER}/{name}");
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
            directory::try_read_character_binding(storage, &self.character_name, &self.server)?
                .unwrap_or(VtankCharacterBinding {
                    settings_file_name: String::new(),
                    loot_file_name: String::new(),
                    nav_file_name: current.clone(),
                    meta_file_name: None,
                });
        directory::write_character_binding(
            storage,
            &self.character_name,
            &self.server,
            &VtankCharacterBinding {
                nav_file_name: current,
                ..existing
            },
        )
    }

    /// Writes a route in its file's own form, with the file's line breaks;
    /// a route the form cannot hold is not written, and the notice says why.
    fn write_route(
        &mut self,
        host: &mut dyn StoreHost,
        file_name: &str,
        settings: &NavigationSettings,
    ) -> bool {
        let text = if is_dropped_foreign_format(file_name) {
            nav::save(settings)
        } else {
            af::save_nav(settings)
        };
        let text = match text {
            Ok(text) => text,
            Err(error) => {
                let shown = self
                    .name_of_key(host, file_name)
                    .unwrap_or_else(|_| file_name.to_owned());
                let notice = format!("Route profile {shown} was NOT saved: {error}");
                host.log().warn(&notice);
                self.save_notice = Some(notice);
                return false;
            }
        };
        if !host.storage(Root::Profiles).is_available() {
            return false;
        }
        let written = host
            .storage(Root::Profiles)
            .read_text(file_name)
            .and_then(|existing| {
                let text = line_endings::match_existing(&text, existing.as_deref());
                host.storage(Root::Profiles).write_text(file_name, &text)
            });
        if let Err(error) = written {
            let shown = self
                .name_of_key(host, file_name)
                .unwrap_or_else(|_| file_name.to_owned());
            let notice = format!("Route profile {shown} could not be saved: {error}");
            host.log().warn(&notice);
            self.save_notice = Some(notice);
            return false;
        }
        self.save_notice = None;
        true
    }
}

/// The old roster's names; `Some(None)` for a roster whose names are null.
fn read_roster_names(text: &str) -> Result<Option<Option<Vec<Option<String>>>>, JsonError> {
    let document = json::parse(text).map_err(|error| error.to_string())?;
    let Some(o) = Object::of(&document, "$", "LegacyRosterDocument")? else {
        return Ok(None);
    };
    Ok(Some(o.strings("Names")?))
}

/// An old JSON route (`LegacyRouteDocument`), reduced to what the route
/// owns.
#[derive(Debug, Clone, PartialEq)]
struct LegacyRoute {
    mode: i32,
    follow_target_object_id: u32,
    follow_target_name: Option<String>,
    waypoints: Option<Vec<Option<RouteWaypoint>>>,
}

impl LegacyRoute {
    fn apply_route_only(&self, value: &mut NavigationSettings) {
        value.mode = match self.mode {
            1 => RouteMode::Linear,
            2 => RouteMode::Target,
            3 => RouteMode::Once,
            _ => RouteMode::Circular,
        };
        value.follow_target_object_id = self.follow_target_object_id;
        value.follow_target_name = self.follow_target_name.clone().unwrap_or_default();
        value.waypoints.clear();
        // A null waypoint would throw in the C# (register row STORE-5).
        value
            .waypoints
            .extend(self.waypoints.iter().flatten().flatten().cloned());
    }
}

/// An old route's recall number as the recall it meant.
fn map_legacy_recall(ordinal: i32) -> RouteRecallKind {
    match ordinal {
        0 => RouteRecallKind::LifestoneRecall,
        1 => RouteRecallKind::Marketplace,
        3 => RouteRecallKind::SecondaryPortalRecall,
        _ => RouteRecallKind::PrimaryPortalRecall,
    }
}

fn read_waypoint(o: &Object<'_>) -> Result<RouteWaypoint, JsonError> {
    let position = |cell: &str,
                    ew: &str,
                    ns: &str,
                    el: &str,
                    heading: &str,
                    outdoor: &str|
     -> Result<Position, JsonError> {
        Ok(Position {
            cell_id: o.uint(cell, 0)?,
            east_west: o.double(ew, 0.0)?,
            north_south: o.double(ns, 0.0)?,
            elevation: o.double(el, 0.0)?,
            heading_degrees: o.float(heading, 0.0)?,
            is_outdoor: o.bool(outdoor, false)?,
        })
    };
    let recall = map_legacy_recall(o.int("Recall", 0)?);
    let jump_heading = o.float("JumpHeadingDegrees", 0.0)?;
    let kind = o.int("Type", 0)?;
    let direction = o.int("JumpDirection", 0)?;
    // The document's own text fields are read for their kind only.
    o.string("RecallSpellName", "")?;
    o.uint("RecallSpellId", 0)?;
    Ok(RouteWaypoint {
        kind: RouteWaypointType::from_i32(kind).unwrap_or(RouteWaypointType::Point),
        position: position(
            "CellId",
            "EastWest",
            "NorthSouth",
            "Elevation",
            "HeadingDegrees",
            "IsOutdoor",
        )?,
        reference_position: position(
            "ReferenceCellId",
            "ReferenceEastWest",
            "ReferenceNorthSouth",
            "ReferenceElevation",
            "ReferenceHeadingDegrees",
            "ReferenceIsOutdoor",
        )?,
        object_id: o.uint("ObjectId", 0)?,
        object_name: o.string("ObjectName", "")?.unwrap_or_default(),
        legacy_object_class: o.int("LegacyObjectClass", 0)?,
        legacy_reference_valid: o.bool("LegacyReferenceValid", true)?,
        text: o.string("Text", "")?.unwrap_or_default(),
        duration_milliseconds: o.int("DurationMilliseconds", 5000)?.clamp(0, 3_600_000),
        recall,
        recall_spell_id: recall.spell_id(),
        recall_spell_name: recall.display_name().to_owned(),
        jump_heading_degrees: if jump_heading.is_finite() {
            f64::from(jump_heading)
        } else {
            0.0
        },
        jump_hold_shift: o.bool("JumpRun", false)?,
        jump_charge_milliseconds: o.int("JumpChargeMilliseconds", 1000)?,
        jump_direction: match direction {
            1 => RouteJumpDirection::StrafeLeft,
            2 => RouteJumpDirection::StrafeRight,
            3 => RouteJumpDirection::Backward,
            _ => RouteJumpDirection::Forward,
        },
    })
}

fn read_legacy_route(text: &str) -> Result<Option<LegacyRoute>, JsonError> {
    let document = json::parse(text).map_err(|error| error.to_string())?;
    let Some(o) = Object::of(&document, "$", "LegacyRouteDocument")? else {
        return Ok(None);
    };
    // Read for their kind: the settings profile owns these now.
    o.int("Version", 1)?;
    o.bool("Enabled", false)?;
    o.bool("Priority", false)?;
    o.double("MinimumDistanceMeters", 2.0)?;
    o.bool("FollowAroundCorners", true)?;
    o.bool("OpenDoors", false)?;
    o.double("DoorIdentifyRangeMeters", 20.0)?;
    o.double("DoorOpenRangeMeters", 4.0)?;
    o.int("DoorLockpickExcessThreshold", -50)?;
    let waypoints = match o.objects("Waypoints", Some(Vec::new()), "LegacyWaypointDocument")? {
        None => None,
        Some(items) => Some(
            items
                .iter()
                .map(|item| item.as_ref().map(read_waypoint).transpose())
                .collect::<Result<Vec<_>, _>>()?,
        ),
    };
    Ok(Some(LegacyRoute {
        mode: o.int("Mode", 0)?,
        follow_target_object_id: o.uint("FollowTargetObjectId", 0)?,
        follow_target_name: o.string("FollowTargetName", "")?,
        waypoints,
    }))
}

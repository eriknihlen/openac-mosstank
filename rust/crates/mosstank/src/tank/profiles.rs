//! The profiles in force (`MossTankPanel`'s load and save paths and
//! `MossTankPanel.FileLayout`): the character's binding, the settings,
//! loot, route and meta profiles, the one line each load produces, and the
//! option overrides a settings profile carries.
//!
//! A storage failure the C# lets throw out of the panel is logged as an
//! error here and the call goes on, as the client does with an exception
//! out of a tick.

use mosstank_formats::gameinfo::{GameInfoMemo, ProfileText, VtankGameInfoDatabase};
use mosstank_formats::route::{NavigationSettings, SpellLookup};
use mosstank_settings::monster::MonsterFactTable;
use mosstank_stores::loot_store::{LootLoad, LootLoadStart};
use mosstank_stores::profile_store::{ProfileLoad, StoreError};
use mosstank_stores::startup;
use mosstank_stores::storage::{PluginStorage, Root};
use openac_plugin_compat::datetime::DateTime;

use super::rules::{lent_loot, route_turn};

use super::staged_load::{LOAD_BYTES_PER_TICK, StagedLoad};
use super::{Core, TankCx};
use crate::client::TankClient;

/// The side-car's loot rules written ahead in a call when a save is due:
/// a 1.4 MB set takes eleven calls of some 4 M instructions each (register
/// STORE-7).
const SIDE_CAR_BYTES_PER_CALL: usize = 128 * 1024;

/// The calls a due save waits for one that ran no macro pass, at most.
const SETTINGS_SAVE_WAITS: u8 = 8;

/// The spells the client knows, read once for a profile load
/// (`IPluginSpells` as a route or meta reads its recall names): the shared
/// lists, named only when a reader asks.
pub(crate) struct SpellsSnapshot {
    table: std::rc::Rc<mosstank_automation::SpellTable>,
    known: [std::rc::Rc<[ac_plugin_abi::magic::SpellInfo]>; 3],
}

impl SpellsSnapshot {
    pub(crate) fn read(client: &mut dyn TankClient) -> Self {
        let spells = client.rules().spells();
        let table = spells.table();
        let known = [
            spells.known_self_buffs_shared(),
            spells.known_attack_spells_shared(),
            spells.known_combat_spells_shared(),
        ];
        Self { table, known }
    }
}

impl SpellLookup for SpellsSnapshot {
    fn find_by_name(&self, name: &str) -> Option<u32> {
        self.table
            .spells()
            .iter()
            .find(|spell| spell.name == name)
            .map(|spell| spell.spell_id)
    }

    fn known_spells(&self) -> Vec<(u32, String)> {
        self.known
            .iter()
            .flat_map(|list| list.iter())
            .map(|spell| (spell.spell_id, spell.name.clone()))
            .collect()
    }

    fn name_of(&self, spell_id: u32) -> Option<String> {
        self.table
            .spells()
            .iter()
            .find(|spell| spell.spell_id == spell_id)
            .map(|spell| spell.name.clone())
    }
}

/// The profile folder as the game database reads it.
struct ProfileReader<'a>(std::cell::RefCell<&'a mut dyn PluginStorage>);

impl ProfileText for ProfileReader<'_> {
    fn is_available(&self) -> bool {
        self.0.borrow_mut().is_available()
    }

    fn read_text(&self, key: &str) -> Option<String> {
        self.0.borrow_mut().read_text(key).ok().flatten()
    }
}

/// The game database in the profile folder.
pub(crate) fn load_game_info(
    client: &mut dyn TankClient,
    memo: &mut GameInfoMemo,
) -> VtankGameInfoDatabase {
    let storage = client.stores().storage(Root::Profiles);
    memo.load(&ProfileReader(std::cell::RefCell::new(storage)))
}

/// Logs a storage failure the C# would have thrown.
pub(crate) fn report_failure<T>(
    client: &mut dyn TankClient,
    result: Result<T, StoreError>,
) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            client
                .rules()
                .log_warn(&format!("MossTank storage failure: {error}"));
            None
        }
    }
}

/// [`report_failure`] with the result evaluated before the client is
/// borrowed again.
macro_rules! report {
    ($client:expr, $result:expr $(,)?) => {{
        let result = $result;
        $crate::tank::profiles::report_failure($client, result)
    }};
}
pub(crate) use report;

impl Core {
    /// Hands a game database to every reader (`ApplyGameInfo`, the part the
    /// shell has).
    pub(crate) fn apply_game_info(&mut self, game_info: VtankGameInfoDatabase) {
        self.settings.combat.monster_facts = MonsterFactTable::new(game_info.clone());
        let mut heal_kits = mosstank_dotnet::collections::IgnoreCaseStringMap::new();
        for kit in &game_info.heal_kits {
            heal_kits.insert(kit.name.clone(), kit.clone());
        }
        self.settings.combat.heal_kits = heal_kits;
        self.combat.replace_game_info(game_info.clone());
        self.crafting.replace_game_info(game_info.clone());
        self.game_info = game_info;
    }

    /// `MigrateFileLayout`.
    pub(crate) fn migrate_file_layout(&mut self, client: &mut dyn TankClient) {
        let now = {
            let clock = client.clock();
            DateTime::local_from_unix_millis(clock.utc_unix_ms, clock.utc_offset_minutes)
        };
        for line in startup::migrate_file_layout(client.stores(), now) {
            self.queue_announcement(client, &line);
        }
    }

    /// The one line a load produces (`AnnounceLoadOutcome`).
    pub(crate) fn announce_load_outcome(
        &mut self,
        client: &mut dyn TankClient,
        kind: &str,
        storage_key: Option<&str>,
        outcome: ProfileLoad,
        reason: Option<&str>,
    ) {
        let root = client.stores().storage(Root::Profiles).root_path();
        if let Some(line) =
            startup::load_announcement(kind, storage_key, outcome, reason, root.as_deref())
        {
            self.queue_announcement(client, &line);
        }
    }

    /// `SaveSettingsProfile`.
    pub(crate) fn save_settings_profile(&mut self, client: &mut dyn TankClient) -> bool {
        // This save writes whatever the call's option changes left due.
        self.settings_save_due = false;
        let saved = self.profiles.save_current(
            client.stores(),
            &mut self.settings,
            &self.no_buff_item_names,
            &self.command_log_types,
        );
        report!(client, saved).unwrap_or(false)
    }

    /// The saves the changes left due, made now: before a settings, loot
    /// or route profile is read, switched or written over by anything
    /// else, and before the plugin stops (register STORE-7).
    pub(crate) fn flush_saves(&mut self, client: &mut dyn TankClient) {
        if self.profile_save_due {
            self.save_profile(client);
        } else if self.settings_save_due {
            self.save_settings_profile(client);
        }
    }

    /// A change the panel or a command made to the profile set: the
    /// settings, the loot profile and the route are saved (`SaveProfile`)
    /// once the call's changes are made and the large files are written
    /// ahead (register STORE-7).
    pub(crate) fn request_profile_save(&mut self) {
        self.settings_save_due = true;
        self.profile_save_due = true;
    }

    /// A call's end with a save due: the side-car's loot rules and the loot
    /// profile are written ahead, [`SIDE_CAR_BYTES_PER_CALL`] of each a
    /// call, and the save is made in the call they are ready in. A call
    /// that ran the macro's pass (`pass_ran`) leaves it to the next,
    /// [`SETTINGS_SAVE_WAITS`] times at most (register STORE-7).
    pub(crate) fn settle_settings_save(&mut self, client: &mut dyn TankClient, pass_ran: bool) {
        if !self.settings_save_due && !self.profile_save_due {
            self.settings_save_waits = 0;
            return;
        }
        if pass_ran && self.settings_save_waits < SETTINGS_SAVE_WAITS {
            self.settings_save_waits += 1;
            return;
        }
        if self.saves_ready() {
            self.settings_save_waits = 0;
            self.flush_saves(client);
        }
    }

    /// Whether the due saves would only copy what was written ahead (or
    /// none is due); when not, [`SIDE_CAR_BYTES_PER_CALL`] more of the
    /// side-car's loot rules and of the loot profile are written ahead. A
    /// load or a binding that would switch a profile waits for it in a
    /// frame (register STORE-7).
    pub(crate) fn saves_ready(&mut self) -> bool {
        let settings = !self.settings_save_due
            || self
                .profiles
                .prepare_save(&self.settings, SIDE_CAR_BYTES_PER_CALL);
        let loot = &self.settings.inventory.loot;
        let profile = !self.profile_save_due
            || self
                .loot_profiles
                .prepare_save(&loot.rules, Some(loot), SIDE_CAR_BYTES_PER_CALL);
        settings && profile
    }

    /// `SaveProfile`: the settings, the loot profile and the route (not
    /// the meta, which is the author's).
    pub(crate) fn save_profile(&mut self, client: &mut dyn TankClient) -> bool {
        self.profile_save_due = false;
        let saved = self.save_settings_profile(client);
        let loot = &self.settings.inventory.loot;
        let written = self
            .loot_profiles
            .save_current(client.stores(), &loot.rules, Some(loot));
        report!(client, written);
        self.save_route_profile(client);
        saved
    }

    /// `SaveRouteProfile`: a route a meta carries is never written over the
    /// selected file.
    pub(crate) fn save_route_profile(&mut self, client: &mut dyn TankClient) -> bool {
        if self.embedded_route_label.is_some() {
            return true;
        }
        self.route_profiles
            .save_current(client.stores(), &self.settings.navigation);
        match self.route_profiles.save_notice() {
            Some(notice) => {
                self.route_notice = notice.to_owned();
                false
            }
            None => true,
        }
    }

    /// `LoadLootProfile`.
    pub(crate) fn load_loot_profile(&mut self, client: &mut dyn TankClient) -> bool {
        match self.begin_loot_profile_load(client) {
            Ok(mut load) => {
                while !load.step(usize::MAX) {}
                self.finish_loot_profile_load(client, load)
            }
            Err(loaded) => loaded,
        }
    }

    /// `LoadLootProfile` up to the parse of the file's rules: the load,
    /// to be parsed ([`LootLoad::step`]) and finished
    /// ([`Self::finish_loot_profile_load`]), or `LoadLootProfile`'s result
    /// when there was no file to parse.
    pub(crate) fn begin_loot_profile_load(
        &mut self,
        client: &mut dyn TankClient,
    ) -> Result<LootLoad, bool> {
        // A save still due goes to the profile it was made for (register
        // STORE-7).
        self.flush_saves(client);
        let loot = &mut self.settings.inventory.loot;
        let mut rules = std::mem::take(&mut loot.rules);
        let begun = self
            .loot_profiles
            .begin_load(client.stores(), &mut rules, Some(loot));
        loot.rules = rules;
        match report!(client, begun) {
            None => Err(false),
            Some(LootLoadStart::Done(outcome)) => {
                Err(self.after_loot_profile_load(client, outcome))
            }
            Some(LootLoadStart::Reading(load)) => Ok(*load),
        }
    }

    /// The rest of `LoadLootProfile` once `load` has parsed its file.
    pub(crate) fn finish_loot_profile_load(
        &mut self,
        client: &mut dyn TankClient,
        load: LootLoad,
    ) -> bool {
        let loot = &mut self.settings.inventory.loot;
        let mut rules = std::mem::take(&mut loot.rules);
        let loaded = self
            .loot_profiles
            .finish_load(client.stores(), load, &mut rules, Some(loot));
        loot.rules = rules;
        let Some(outcome) = report!(client, loaded) else {
            return false;
        };
        self.after_loot_profile_load(client, outcome)
    }

    /// What `LoadLootProfile` does with the store's outcome.
    fn after_loot_profile_load(
        &mut self,
        client: &mut dyn TankClient,
        outcome: ProfileLoad,
    ) -> bool {
        let key = self.loot_profiles.last_load_key().map(str::to_owned);
        self.announce_load_outcome(client, "loot", key.as_deref(), outcome, None);
        if outcome == ProfileLoad::Missing {
            let loot = &self.settings.inventory.loot;
            let written = self
                .loot_profiles
                .save_current(client.stores(), &loot.rules, Some(loot));
            report!(client, written);
        }
        if outcome == ProfileLoad::Failed {
            self.loot_editor_notice = self
                .loot_profiles
                .load_failure_notice()
                .unwrap_or("No loot profile is active.")
                .to_owned();
            client.looter().log_error(&self.loot_editor_notice.clone());
            self.refresh_loot_editor(false);
            return false;
        }
        if outcome == ProfileLoad::Partial {
            self.loot_editor_notice = self
                .loot_profiles
                .load_notice()
                .unwrap_or("Loaded the complete rules before an incomplete profile tail.")
                .to_owned();
            client.rules().log_warn(&self.loot_editor_notice.clone());
        }
        let line = if self.loot_profiles.has_active_profile() {
            format!(
                "Loot profile {} active ({} rules).",
                self.loot_profiles.selected(),
                self.settings.inventory.loot.rules.len()
            )
        } else {
            "No loot profile is active.".to_owned()
        };
        client.rules().log_info(&line);
        {
            let lent = lent_loot!(self);
            lent.loot.reset(client.looter());
        }
        self.refresh_loot_editor(false);
        true
    }

    /// `LoadRouteProfile`: a route that will not parse keeps its file and
    /// the route in memory.
    pub(crate) fn load_route_profile(&mut self, client: &mut dyn TankClient) -> bool {
        // A save of the profile set still due goes to the route it was made
        // for (register STORE-7).
        if self.profile_save_due {
            self.flush_saves(client);
        }
        let spells = SpellsSnapshot::read(client);
        let loaded = self.route_profiles.load_current(
            client.stores(),
            &mut self.settings.navigation,
            Some(&spells),
        );
        let Some(outcome) = report!(client, loaded) else {
            return false;
        };
        let key = self.route_profiles.last_load_key().map(str::to_owned);
        self.announce_load_outcome(client, "navigation", key.as_deref(), outcome, None);
        if outcome == ProfileLoad::Missing {
            self.route_profiles
                .save_current(client.stores(), &self.settings.navigation);
        }
        if outcome == ProfileLoad::Failed {
            let selected =
                report!(client, self.route_profiles.selected(client.stores())).unwrap_or_default();
            client.looter().log_error(&format!(
                "Route profile {selected} could not be read; the file was left as it is and the route in memory is unchanged."
            ));
            return false;
        }
        self.embedded_route_label = None;
        true
    }

    /// `RestartRouteRound`: a route loaded while the macro runs starts the
    /// way a macro start does.
    pub(crate) fn restart_route_round(&mut self, client: &mut dyn TankClient) {
        let mut turn = route_turn!(client, self);
        self.navigation.reset(&mut turn);
        if self.combat.enabled()
            && let Some(line) = self.navigation.anchor_round_to_start(&mut turn)
        {
            self.queue_log(mosstank_meta::scheduler::MacroLogChannel::RuleInfo, &line);
        }
    }

    /// The looter's helper list from the profile (the C# shares the live
    /// collections).
    pub(crate) fn refresh_loot_consumables(&mut self) {
        self.loot.set_configured_consumables(
            self.settings.combat.consumable_names.clone(),
            self.settings.combat.consumable_categories.clone(),
        );
    }
}

impl TankCx<'_> {
    /// The constructor's loads, for whoever is in the world: the settings
    /// (with the game database), the loot profile, the route and the meta,
    /// then what the constructor does with them; a step a tick when the set
    /// is large (register PROF-1).
    pub(crate) fn load_initial_profiles(&mut self, bytes: usize) {
        self.core.staged_load = Some(StagedLoad::initial());
        self.step_staged_load(bytes);
    }

    /// The constructor's settings load, with the game database; the name
    /// the set is bound to.
    pub(crate) fn initial_settings(&mut self) -> String {
        let name = self.client.rules().character().name;
        let game_info = load_game_info(self.client, &mut self.core.game_info_memo);
        self.core.apply_game_info(game_info);
        report!(
            self.client,
            self.core
                .profiles
                .bind_character(self.client.stores(), Some(&name))
        );
        let core = &mut *self.core;
        let loaded = core.profiles.load_current(
            self.client.stores(),
            &mut core.settings,
            &mut core.no_buff_item_names,
            &mut core.command_log_types,
        );
        if let Some(outcome) = report!(self.client, loaded) {
            let key = core.profiles.last_load_key().map(str::to_owned);
            core.announce_load_outcome(self.client, "settings", key.as_deref(), outcome, None);
            if outcome == ProfileLoad::Failed {
                core.profile_lifecycle_notice = core
                    .profiles
                    .load_failure_notice()
                    .unwrap_or("The settings profile could not be read.")
                    .to_owned();
            }
        }
        core.reapply_game_info();
        core.refresh_loot_consumables();
        name
    }

    /// The constructor's route read: a route that will not parse is said
    /// only by its load line.
    pub(crate) fn initial_route(&mut self, name: &str) {
        let core = &mut *self.core;
        report!(
            self.client,
            core.route_profiles
                .bind_character(self.client.stores(), Some(name))
        );
        let spells = SpellsSnapshot::read(self.client);
        let loaded = core.route_profiles.load_current(
            self.client.stores(),
            &mut core.settings.navigation,
            Some(&spells),
        );
        if let Some(outcome) = report!(self.client, loaded) {
            let key = core.route_profiles.last_load_key().map(str::to_owned);
            core.announce_load_outcome(self.client, "navigation", key.as_deref(), outcome, None);
            if outcome == ProfileLoad::Missing {
                core.route_profiles
                    .save_current(self.client.stores(), &core.settings.navigation);
            }
        }
    }

    /// The constructor's meta: the engine is built with it, no transition,
    /// nothing announced.
    pub(crate) fn initial_meta(&mut self, name: &str) {
        let core = &mut *self.core;
        report!(
            self.client,
            core.meta_profiles
                .bind_character(self.client.stores(), Some(name))
        );
        let spells = SpellsSnapshot::read(self.client);
        let profile = report!(
            self.client,
            core.meta_profiles
                .load_current(self.client.stores(), Some(&spells)),
        )
        .unwrap_or_default();
        self.meta.set_initial_profile(profile);
    }

    /// What the constructor does once the set is read: the meta's interval
    /// and switch from the profile, the profile's options, and the editors.
    pub(crate) fn initial_finish(&mut self) {
        let interval = self.core.profiles.meta_interval_milliseconds();
        self.core.meta_interval_pending = Some(f64::from(interval) / 1000.0);
        self.meta.set_interval_milliseconds(interval);
        self.meta.set_enabled(self.core.settings.meta.enabled);
        self.core.initialized = true;
        self.apply_persisted_option_overrides();
        self.refresh_editors();
        self.refresh_live_ub_rows(true);
        self.refresh_meta_editor();
    }
}

impl Core {
    /// The constructor's loot read, up to the parse of the file: the load
    /// to parse, or `Err` when there was none (no binding, no file).
    pub(crate) fn begin_initial_loot_load(
        &mut self,
        client: &mut dyn TankClient,
        name: &str,
    ) -> Result<LootLoad, bool> {
        let bound = report!(
            client,
            self.loot_profiles
                .bind_character(client.stores(), Some(name))
        )
        .unwrap_or(false);
        if !bound {
            self.settings.inventory.loot.rules.clear();
            self.settings.inventory.loot.profile_active = false;
            return Err(false);
        }
        // The constructor's own read: no "active" line and no looter
        // reset, as the full LoadLootProfile has.
        let loot = &mut self.settings.inventory.loot;
        let mut rules = std::mem::take(&mut loot.rules);
        let begun = self
            .loot_profiles
            .begin_load(client.stores(), &mut rules, Some(loot));
        loot.rules = rules;
        match report!(client, begun) {
            None => Err(false),
            Some(LootLoadStart::Done(outcome)) => {
                self.after_initial_loot_load(client, outcome);
                Err(false)
            }
            Some(LootLoadStart::Reading(load)) => Ok(*load),
        }
    }

    /// The rest of the constructor's loot read once `load` has parsed its
    /// file.
    pub(crate) fn finish_initial_loot_load(&mut self, client: &mut dyn TankClient, load: LootLoad) {
        let loot = &mut self.settings.inventory.loot;
        let mut rules = std::mem::take(&mut loot.rules);
        let loaded = self
            .loot_profiles
            .finish_load(client.stores(), load, &mut rules, Some(loot));
        loot.rules = rules;
        if let Some(outcome) = report!(client, loaded) {
            self.after_initial_loot_load(client, outcome);
        }
    }

    /// What the constructor does with its loot read's outcome.
    fn after_initial_loot_load(&mut self, client: &mut dyn TankClient, outcome: ProfileLoad) {
        let key = self.loot_profiles.last_load_key().map(str::to_owned);
        self.announce_load_outcome(client, "loot", key.as_deref(), outcome, None);
        match outcome {
            ProfileLoad::Missing => {
                let loot = &self.settings.inventory.loot;
                let written =
                    self.loot_profiles
                        .save_current(client.stores(), &loot.rules, Some(loot));
                report!(client, written);
            }
            ProfileLoad::Failed => {
                self.loot_editor_notice = self
                    .loot_profiles
                    .load_failure_notice()
                    .unwrap_or("No loot profile is active.")
                    .to_owned();
                client.looter().log_error(&self.loot_editor_notice.clone());
            }
            ProfileLoad::Partial => {
                self.loot_editor_notice = self
                    .loot_profiles
                    .load_notice()
                    .unwrap_or("Loaded the complete rules before an incomplete profile tail.")
                    .to_owned();
                client.rules().log_warn(&self.loot_editor_notice.clone());
            }
            ProfileLoad::Loaded => {}
        }
    }
}

impl TankCx<'_> {
    /// `EnsureCharacterProfile`: the binding waits for a name; the same
    /// character returning is no change, a different one rebinds.
    pub(crate) fn ensure_character_profile(&mut self) {
        // A load under way goes on, on a frame that has room for it; the
        // binding it was started for stands.
        if self.core.staged_load.is_some() {
            if !self.core.heavy_frame {
                self.step_staged_load_in_frame(true);
            }
            return;
        }
        let character = self.client.rules().character();
        let name = character.name;
        if mosstank_stores::directory::canonical_character_key(Some(&name)).is_empty() {
            return;
        }
        // The binding may name another settings file: what is due goes to
        // the one it was changed in, once its side-car is ready.
        if !self.core.saves_ready() {
            return;
        }
        self.core.flush_saves(self.client);
        {
            let core = &mut *self.core;
            self.ub.bind(self.client, &character.world, &name);
            core.folders.prepare_character(
                self.client.stores().storage(Root::Profiles),
                Some(&character.world),
                Some(&name),
            );
        }
        let core = &mut *self.core;
        let macro_changed = report!(
            self.client,
            core.profiles
                .bind_character(self.client.stores(), Some(&name))
        )
        .unwrap_or(false);
        let loot_changed = report!(
            self.client,
            core.loot_profiles
                .bind_character(self.client.stores(), Some(&name)),
        )
        .unwrap_or(false);
        let route_changed = report!(
            self.client,
            core.route_profiles
                .bind_character(self.client.stores(), Some(&name)),
        )
        .unwrap_or(false);
        let meta_changed = report!(
            self.client,
            core.meta_profiles
                .bind_character(self.client.stores(), Some(&name)),
        )
        .unwrap_or(false);
        if !macro_changed && !loot_changed && !route_changed && !meta_changed {
            return;
        }
        // As the C# has it: the loads below follow a good settings load too,
        // so after one the loot profile and route are read twice and the
        // meta, when it changed, twice as well. They run a step a tick
        // (register PROF-1).
        self.core.staged_load = Some(StagedLoad::new(
            macro_changed,
            loot_changed,
            route_changed,
            meta_changed,
        ));
        // A frame that already did a frame's worth (the name tags' first
        // look at a crowded world) leaves the first step to the next.
        if self.core.heavy_frame {
            return;
        }
        self.step_staged_load_in_frame(false);
    }

    /// A frame's steps of the staged load (`under_way`: begun in an earlier
    /// frame). A frame that loaded a tick's worth, leaves more for the
    /// next or ends a load of several frames counts as a heavy one: its
    /// other once-only work and the window push wait a frame.
    fn step_staged_load_in_frame(&mut self, under_way: bool) {
        let (more, spent) = self.step_staged_load(LOAD_BYTES_PER_TICK);
        if more || under_way || spent >= LOAD_BYTES_PER_TICK {
            self.core.heavy_frame = true;
        }
    }

    /// `LoadSelectedProfile`.
    pub(crate) fn load_selected_profile(&mut self) -> Option<ProfileLoad> {
        let (outcome, retained) = self.load_selected_settings()?;
        if outcome == ProfileLoad::Failed {
            return Some(outcome);
        }
        let loot_loaded = self.core.load_loot_profile(self.client);
        self.keep_active_loot(loot_loaded, Some(retained));
        self.load_selected_route();
        Some(outcome)
    }

    /// `LoadRouteProfile` with what follows a good load.
    pub(crate) fn load_route_profile(&mut self) -> bool {
        let (loaded, meta_enabled) = self.core.load_route_profile_in_full(self.client);
        if let Some(enabled) = meta_enabled {
            self.meta.set_enabled(enabled);
        }
        loaded
    }

    /// `LoadMetaProfile`.
    pub(crate) fn load_meta_profile(&mut self) {
        let profile = self.core.load_meta_profile_current(self.client);
        self.discard_embedded_route();
        self.with_engine(|engine, host, services| engine.replace_profile(host, services, profile));
        if self.core.initialized {
            self.apply_persisted_option_overrides();
        }
    }

    /// `DiscardEmbeddedRoute`: a route a meta carried goes with its meta,
    /// leaving no route (the meta that loads next may carry its own).
    pub(crate) fn discard_embedded_route(&mut self) {
        self.core.discard_embedded_route(self.client);
    }

    /// `ResetProfileConsumers` (the part the shell has).
    pub(crate) fn reset_profile_consumers(&mut self) {
        let core = &mut *self.core;
        core.vital_recharge.reset();
        core.vital_helper_recharge.reset();
        core.inventory_maintenance.reset();
        core.dispel.reset();
        core.crafting.reset();
        core.item_mana.reset();
        {
            let lent = lent_loot!(core);
            lent.loot.reset(self.client.looter());
        }
        {
            let mut turn = route_turn!(self.client, core);
            core.navigation.reset(&mut turn);
        }
        self.core.panel.vitals.forget_coverage();
        self.refresh_editors();
    }

    /// `ApplyPersistedOptionOverrides`: the options a settings profile
    /// carries are put back through the one write path, without saving.
    pub(crate) fn apply_persisted_option_overrides(&mut self) {
        if let Some(enabled) = self.core.apply_persisted_option_overrides(self.client) {
            self.meta.set_enabled(enabled);
        }
    }

    /// `SelectProfile`.
    pub(crate) fn select_profile(&mut self, name: &str) {
        self.core.save_profile(self.client);
        let selected = report!(
            self.client,
            self.core.profiles.select(self.client.stores(), Some(name)),
        )
        .unwrap_or(false);
        if !selected {
            self.core.profile_lifecycle_notice = format!("Profile '{name}' is unavailable.");
            return;
        }
        if self.load_selected_profile() == Some(ProfileLoad::Failed) {
            return;
        }
        let selected = self.core.profiles.selected();
        self.core.profile_lifecycle_notice = format!("Loaded {selected}.");
        self.client
            .rules()
            .log_info(&format!("Loaded settings profile {selected}."));
    }

    /// `SelectLootProfileCore`.
    pub(crate) fn select_loot_profile(&mut self, name: &str) {
        let selected = report!(
            self.client,
            self.core
                .loot_profiles
                .select(self.client.stores(), Some(name)),
        )
        .unwrap_or(false);
        if !selected {
            self.core.loot_editor_notice = format!("Loot profile '{name}' is unavailable.");
            return;
        }
        if !self.core.load_loot_profile(self.client) {
            return;
        }
        if self.core.loot_profiles.active_profile_is_partial() {
            return;
        }
        let selected = self.core.loot_profiles.selected();
        self.core.loot_editor_notice = format!("Loaded loot profile {selected}.");
        self.client
            .rules()
            .log_info(&format!("Loaded loot profile {selected}."));
    }

    /// `SelectRouteProfileCore`.
    pub(crate) fn select_route_profile(&mut self, name: &str) {
        if self.core.embedded_route_label.as_deref() == Some(name) {
            // The meta's own route, picked again: read it from the meta's
            // file again.
            self.reload_embedded_route(name);
            return;
        }
        let selected = report!(
            self.client,
            self.core
                .route_profiles
                .select(self.client.stores(), Some(name), false),
        )
        .unwrap_or(false);
        if !selected {
            self.core.route_notice = format!("Route profile '{name}' is unavailable.");
            return;
        }
        let selected = report!(
            self.client,
            self.core.route_profiles.selected(self.client.stores())
        )
        .unwrap_or_default();
        if !self.load_route_profile() {
            self.core.route_notice = format!("Route profile {selected} could not be read.");
            return;
        }
        self.core.route_notice = format!("Loaded route {selected}.");
        self.client
            .rules()
            .log_info(&format!("Loaded route profile {selected}."));
    }

    /// `SelectMetaProfileCore`.
    pub(crate) fn select_meta_profile(&mut self, name: &str) {
        if pick_meta_profile(self.core, self.client, name) {
            self.load_picked_meta_profile();
        }
    }

    /// The rest of `SelectMetaProfileCore` once the profile is picked: the
    /// engine loads it, and the notice says how that went.
    pub(crate) fn load_picked_meta_profile(&mut self) {
        self.load_meta_profile();
        let selected = report!(
            self.client,
            self.core.meta_profiles.selected(self.client.stores())
        )
        .unwrap_or_default();
        if let Some(error) = self.core.meta_profiles.last_load_error() {
            self.core.meta_notice = format!("Could not load Meta profile {selected}: {error}");
            return;
        }
        self.core.meta_notice = format!("Loaded Meta profile {selected}.");
        self.client
            .rules()
            .log_info(&format!("Loaded Meta profile {selected}."));
    }

    /// `LoadEmbeddedNavigationRoute`: a route a meta carries.
    pub(crate) fn load_embedded_navigation_route(
        core: &mut Core,
        client: &mut dyn TankClient,
        route: Option<&NavigationSettings>,
        name: &str,
    ) {
        let Some(route) = route else {
            core.route_notice = "Embedded route rejected: unresolved Nav tag.".to_owned();
            client
                .rules()
                .log_warn("MossTank Meta embedded route rejected: unresolved Nav tag.");
            return;
        };
        mosstank_formats::nav::apply(&mut core.settings.navigation, route.clone());
        core.restart_route_round(client);
        let label = embedded_route_label(name);
        core.route_notice = format!(
            "Loaded {label} ({} points).",
            core.settings.navigation.waypoints.len()
        );
        core.embedded_route_label = Some(label);
        let notice = core.route_notice.clone();
        Core::announce(client, &notice);
    }
}

impl Core {
    /// Reads the selected meta and says so (the load half of
    /// `LoadMetaProfile`).
    pub(crate) fn load_meta_profile_current(
        &mut self,
        client: &mut dyn TankClient,
    ) -> mosstank_formats::meta::MetaProfile {
        let spells = SpellsSnapshot::read(client);
        let profile = report!(
            client,
            self.meta_profiles
                .load_current(client.stores(), Some(&spells)),
        )
        .unwrap_or_default();
        self.announce_meta_load(client);
        profile
    }

    /// The line a meta load produces, from what the store says of it.
    pub(crate) fn announce_meta_load(&mut self, client: &mut dyn TankClient) {
        let outcome = if self.meta_profiles.last_load_succeeded() {
            ProfileLoad::Loaded
        } else if self.meta_profiles.last_load_error().is_none() {
            ProfileLoad::Missing
        } else {
            ProfileLoad::Failed
        };
        let key = self.meta_profiles.last_load_key().map(str::to_owned);
        let reason = self.meta_profiles.last_load_error().map(str::to_owned);
        self.announce_load_outcome(client, "Meta", key.as_deref(), outcome, reason.as_deref());
    }
}

impl Core {
    /// `LoadRouteProfile` in full, for a caller holding only the core (the
    /// follow route): the read, the options the profile carries, the
    /// route's new round and the line that says so. Whether the route in
    /// memory is now the file's, and `EnableMeta` when the options wrote it
    /// (the caller starts or stops the engine).
    pub(crate) fn load_route_profile_in_full(
        &mut self,
        client: &mut dyn TankClient,
    ) -> (bool, Option<bool>) {
        if !self.load_route_profile(client) {
            return (false, None);
        }
        let mut meta_enabled = None;
        if self.initialized {
            meta_enabled = self.apply_persisted_option_overrides(client);
        }
        self.restart_route_round(client);
        if self.initialized {
            let selected =
                report!(client, self.route_profiles.selected(client.stores())).unwrap_or_default();
            let count = self.settings.navigation.waypoints.len();
            Core::announce(
                client,
                &format!("Loaded route {selected} ({count} points)."),
            );
        }
        (true, meta_enabled)
    }

    /// `ApplyPersistedOptionOverrides` over the core: `EnableMeta`'s last
    /// value when one of the options wrote it.
    pub(crate) fn apply_persisted_option_overrides(
        &mut self,
        client: &mut dyn TankClient,
    ) -> Option<bool> {
        let overrides: Vec<(String, mosstank_settings::monster::expression::MonsterValue)> = self
            .settings
            .combat
            .dynamic_settings
            .iter()
            .filter(|(name, _)| mosstank_settings::options::is_known(name))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        if overrides.is_empty() {
            return None;
        }
        let mut meta_enabled = None;
        self.applying_profile_options = true;
        for (name, value) in overrides {
            let value = super::options::to_expression_value(&value);
            match self.set_option(&name, &value) {
                Ok(true) if super::services::is_enable_meta(&name) => {
                    meta_enabled = Some(value.is_truthy());
                }
                Ok(_) => {}
                Err(error) => client.rules().log_warn(&format!(
                    "MossTank option {name} could not be applied: {}",
                    error.message()
                )),
            }
        }
        self.applying_profile_options = false;
        meta_enabled
    }

    /// `DiscardEmbeddedRoute`: the route a meta carried goes with its meta.
    /// Nothing comes in its place and nothing is said.
    pub(crate) fn discard_embedded_route(&mut self, client: &mut dyn TankClient) {
        if self.embedded_route_label.is_none() {
            return;
        }
        {
            let mut turn = route_turn!(client, self);
            self.navigation.reset(&mut turn);
        }
        mosstank_formats::nav::apply(&mut self.settings.navigation, NavigationSettings::default());
        self.embedded_route_label = None;
    }
}

impl TankCx<'_> {
    /// `ReloadEmbeddedRoute`: the meta's own route, read from the meta's
    /// file again; a meta that does not read, or does not name the route
    /// once, leaves the route as it is and says why.
    pub(crate) fn reload_embedded_route(&mut self, label: &str) {
        let spells = SpellsSnapshot::read(self.client);
        let fresh = self
            .core
            .meta_profiles
            .load_current(self.client.stores(), Some(&spells));
        let fresh = match fresh {
            Ok(fresh) if self.core.meta_profiles.last_load_succeeded() => fresh,
            Ok(_) => {
                let reason = self
                    .core
                    .meta_profiles
                    .last_load_error()
                    .unwrap_or("the meta file is missing.")
                    .to_owned();
                self.core.route_notice = format!("Could not reload the embedded route: {reason}");
                return;
            }
            Err(error) => {
                self.core.route_notice = format!("Could not reload the embedded route: {error}");
                return;
            }
        };
        let mut matches = Vec::new();
        for rule in &fresh.rules {
            embedded_actions(&rule.action, &mut matches);
        }
        matches.retain(|action| embedded_route_label(&action.secondary_text) == label);
        if matches.len() != 1 || matches[0].embedded_route.is_none() {
            "Could not reload the embedded route: its name is missing or ambiguous in the meta file."
                .clone_into(&mut self.core.route_notice);
            return;
        }
        let action = matches[0];
        Self::load_embedded_navigation_route(
            self.core,
            self.client,
            action.embedded_route.as_deref(),
            &action.secondary_text,
        );
    }
}

/// Every action that loads an embedded route, in the order the C# visits
/// them (`EmbeddedActions`).
fn embedded_actions<'a>(
    action: &'a mosstank_formats::meta::MetaAction,
    found: &mut Vec<&'a mosstank_formats::meta::MetaAction>,
) {
    if action.kind == mosstank_formats::meta::MetaActionKind::LoadEmbeddedNavigationRoute {
        found.push(action);
    }
    for child in &action.children {
        embedded_actions(child, found);
    }
}

impl Core {
    /// The monster facts and heal kits come from the game database, not
    /// the profile: put back after a settings load replaced them.
    pub(crate) fn reapply_game_info(&mut self) {
        let game_info = self.game_info.clone();
        self.apply_game_info(game_info);
    }
}

/// What the route selector shows for a route a meta carries
/// (`EmbeddedRouteLabel`).
pub(crate) fn embedded_route_label(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        "(embedded)".to_owned()
    } else {
        format!("{trimmed} (embedded)")
    }
}

/// The pick of `SelectMetaProfileCore`: whether the store took the name; a
/// name it does not have is said in the meta notice.
pub(crate) fn pick_meta_profile(core: &mut Core, client: &mut dyn TankClient, name: &str) -> bool {
    let selected = report!(
        client,
        core.meta_profiles
            .select(client.stores(), Some(name), false),
    )
    .unwrap_or(false);
    if !selected {
        core.meta_notice = format!("Meta profile '{name}' is unavailable.");
    }
    selected
}

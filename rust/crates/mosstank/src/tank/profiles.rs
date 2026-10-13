//! The profiles in force (`MossTankPanel`'s load and save paths and
//! `MossTankPanel.FileLayout`): the character's binding, the settings,
//! loot, route and meta profiles, the one line each load produces, and the
//! option overrides a settings profile carries.
//!
//! A storage failure the C# lets throw out of the panel is logged as an
//! error here and the call goes on, as the client does with an exception
//! out of a tick.

use ac_plugin_abi::magic::{SpellInfo, SpellList};
use mosstank_formats::gameinfo::{ProfileText, VtankGameInfoDatabase};
use mosstank_formats::route::{NavigationSettings, SpellLookup};
use mosstank_settings::monster::MonsterFactTable;
use mosstank_stores::profile_store::{ProfileLoad, StoreError};
use mosstank_stores::startup;
use mosstank_stores::storage::{PluginStorage, Root};
use openac_plugin_compat::datetime::DateTime;

use super::rules::{lent_loot, route_turn};

use super::{Core, TankCx};
use crate::client::TankClient;

/// The spells the client knows, read once for a profile load
/// (`IPluginSpells` as a route or meta reads its recall names).
pub(crate) struct SpellsSnapshot {
    all: Vec<SpellInfo>,
    known: Vec<(u32, String)>,
}

impl SpellsSnapshot {
    pub(crate) fn read(client: &mut dyn TankClient) -> Self {
        let rules = client.rules();
        let all = rules.spells().all();
        let mut known = Vec::new();
        for list in [SpellList::SelfBuffs, SpellList::Attacks, SpellList::Combat] {
            for spell in client.route().known_spell_list(list) {
                known.push((spell.spell_id, spell.name.clone()));
            }
        }
        Self { all, known }
    }
}

impl SpellLookup for SpellsSnapshot {
    fn find_by_name(&self, name: &str) -> Option<u32> {
        self.all
            .iter()
            .find(|spell| spell.name == name)
            .map(|spell| spell.spell_id)
    }

    fn known_spells(&self) -> Vec<(u32, String)> {
        self.known.clone()
    }

    fn name_of(&self, spell_id: u32) -> Option<String> {
        self.all
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
pub(crate) fn load_game_info(client: &mut dyn TankClient) -> VtankGameInfoDatabase {
    let storage = client.stores().storage(Root::Profiles);
    VtankGameInfoDatabase::load(&ProfileReader(std::cell::RefCell::new(storage)))
}

/// Logs a storage failure the C# would have thrown.
fn report_failure<T>(client: &mut dyn TankClient, result: Result<T, StoreError>) -> Option<T> {
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
        report_failure($client, result)
    }};
}

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
        let saved = self.profiles.save_current(
            client.stores(),
            &mut self.settings,
            &self.no_buff_item_names,
            &self.command_log_types,
        );
        report!(client, saved).unwrap_or(false)
    }

    /// `SaveProfile`: the settings, the loot profile and the route (not
    /// the meta, which is the author's).
    pub(crate) fn save_profile(&mut self, client: &mut dyn TankClient) -> bool {
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
        let loot = &mut self.settings.inventory.loot;
        let mut rules = std::mem::take(&mut loot.rules);
        let loaded = self
            .loot_profiles
            .load_current(client.stores(), &mut rules, Some(loot));
        loot.rules = rules;
        let Some(outcome) = report!(client, loaded) else {
            return false;
        };
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
        let lent = lent_loot!(self);
        lent.loot.reset(client.looter());
        true
    }

    /// `LoadRouteProfile`: a route that will not parse keeps its file and
    /// the route in memory.
    pub(crate) fn load_route_profile(&mut self, client: &mut dyn TankClient) -> bool {
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
    /// The constructor's loads, for whoever is in the world.
    pub(crate) fn load_initial_profiles(&mut self) {
        let name = self.client.rules().character().name;
        let name = Some(name.as_str());
        let game_info = load_game_info(self.client);
        self.core.apply_game_info(game_info);
        report!(
            self.client,
            self.core
                .profiles
                .bind_character(self.client.stores(), name)
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

        let bound = report!(
            self.client,
            core.loot_profiles
                .bind_character(self.client.stores(), name)
        )
        .unwrap_or(false);
        if bound {
            // The constructor's own read: no "active" line and no looter
            // reset, as the full LoadLootProfile has.
            let loot = &mut core.settings.inventory.loot;
            let mut rules = std::mem::take(&mut loot.rules);
            let loaded =
                core.loot_profiles
                    .load_current(self.client.stores(), &mut rules, Some(loot));
            loot.rules = rules;
            if let Some(outcome) = report!(self.client, loaded) {
                let key = core.loot_profiles.last_load_key().map(str::to_owned);
                core.announce_load_outcome(self.client, "loot", key.as_deref(), outcome, None);
                match outcome {
                    ProfileLoad::Missing => {
                        let loot = &core.settings.inventory.loot;
                        let written = core.loot_profiles.save_current(
                            self.client.stores(),
                            &loot.rules,
                            Some(loot),
                        );
                        report!(self.client, written);
                    }
                    ProfileLoad::Failed => {
                        core.loot_editor_notice = core
                            .loot_profiles
                            .load_failure_notice()
                            .unwrap_or("No loot profile is active.")
                            .to_owned();
                        self.client
                            .looter()
                            .log_error(&core.loot_editor_notice.clone());
                    }
                    ProfileLoad::Partial => {
                        core.loot_editor_notice = core
                            .loot_profiles
                            .load_notice()
                            .unwrap_or(
                                "Loaded the complete rules before an incomplete profile tail.",
                            )
                            .to_owned();
                        self.client
                            .rules()
                            .log_warn(&core.loot_editor_notice.clone());
                    }
                    ProfileLoad::Loaded => {}
                }
            }
        } else {
            core.settings.inventory.loot.rules.clear();
            core.settings.inventory.loot.profile_active = false;
        }

        report!(
            self.client,
            core.route_profiles
                .bind_character(self.client.stores(), name)
        );
        // The constructor's own read: a route that will not parse is said
        // only by its load line.
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

        report!(
            self.client,
            core.meta_profiles
                .bind_character(self.client.stores(), name)
        );
        let spells = SpellsSnapshot::read(self.client);
        let profile = report!(
            self.client,
            core.meta_profiles
                .load_current(self.client.stores(), Some(&spells)),
        )
        .unwrap_or_default();
        // The engine is built with this profile: no transition, nothing
        // announced.
        self.meta.set_initial_profile(profile);
    }

    /// `EnsureCharacterProfile`: the binding waits for a name; the same
    /// character returning is no change, a different one rebinds.
    pub(crate) fn ensure_character_profile(&mut self) {
        let character = self.client.rules().character();
        let name = character.name;
        if mosstank_stores::directory::canonical_character_key(Some(&name)).is_empty() {
            return;
        }
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
        // meta, when it changed, twice as well.
        if macro_changed && self.load_selected_profile() == Some(ProfileLoad::Failed) {
            return;
        }
        if loot_changed {
            self.core.load_loot_profile(self.client);
        }
        if route_changed {
            self.load_route_profile();
        }
        if meta_changed {
            self.load_meta_profile();
        }
        if macro_changed && meta_changed {
            self.load_meta_profile();
        }
        let who = self.client.rules().character().name;
        self.core.profile_lifecycle_notice = format!(
            "Loaded {} for {}",
            self.core.profiles.selected(),
            if who.is_empty() {
                "this character.".to_owned()
            } else {
                format!("{who}.")
            }
        );
    }

    /// `LoadSelectedProfile`.
    pub(crate) fn load_selected_profile(&mut self) -> Option<ProfileLoad> {
        let core = &mut *self.core;
        let retain_active_loot = core.loot_profiles.has_active_profile();
        let active_rules = core.settings.inventory.loot.rules.clone();
        let active_salvage = core.settings.inventory.loot.salvage_combine.clone();
        let loaded = core.profiles.load_current(
            self.client.stores(),
            &mut core.settings,
            &mut core.no_buff_item_names,
            &mut core.command_log_types,
        );
        let outcome = report!(self.client, loaded)?;
        let key = core.profiles.last_load_key().map(str::to_owned);
        core.announce_load_outcome(self.client, "settings", key.as_deref(), outcome, None);
        core.reapply_game_info();
        core.refresh_loot_consumables();
        if outcome == ProfileLoad::Failed {
            core.profile_lifecycle_notice = core
                .profiles
                .load_failure_notice()
                .unwrap_or("The settings profile could not be read.")
                .to_owned();
            return Some(outcome);
        }
        // The profile's own EnableMeta decides whether the meta runs; the
        // pass is still gated on the macro running.
        self.meta.set_enabled(self.core.settings.meta.enabled);
        let core = &mut *self.core;
        let loot_loaded = core.load_loot_profile(self.client);
        if !loot_loaded {
            if retain_active_loot {
                let loot = &mut core.settings.inventory.loot;
                loot.rules = active_rules;
                loot.salvage_combine = active_salvage;
                loot.profile_active = true;
            }
        } else if retain_active_loot && core.loot_profiles.active_profile_is_partial() {
            let loot = &mut core.settings.inventory.loot;
            loot.rules = active_rules;
            loot.salvage_combine = active_salvage;
            core.loot_editor_notice = format!(
                "Loot profile {} has an incomplete tail; current rules were retained and source was preserved.",
                core.loot_profiles.selected()
            );
            self.client
                .rules()
                .log_warn(&core.loot_editor_notice.clone());
        }
        self.load_route_profile();
        self.apply_persisted_option_overrides();
        self.reset_profile_consumers();
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
        let mut turn = route_turn!(self.client, core);
        core.navigation.reset(&mut turn);
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
        let selected = report!(
            self.client,
            self.core
                .meta_profiles
                .select(self.client.stores(), Some(name), false),
        )
        .unwrap_or(false);
        if !selected {
            self.core.meta_notice = format!("Meta profile '{name}' is unavailable.");
            return;
        }
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
        profile
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
            match self.set_option(client, &name, &value) {
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

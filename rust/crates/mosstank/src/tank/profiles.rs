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
use openac_plugin_compat::text::ordinal;

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
            core.load_loot_profile(self.client);
        } else {
            core.settings.inventory.loot.rules.clear();
            core.settings.inventory.loot.profile_active = false;
        }

        report!(
            self.client,
            core.route_profiles
                .bind_character(self.client.stores(), name)
        );
        core.load_route_profile(self.client);

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
        self.with_engine(|engine, host, services| engine.replace_profile(host, services, profile));
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
        if macro_changed {
            if self.load_selected_profile() == Some(ProfileLoad::Failed) {
                return;
            }
        } else {
            if loot_changed {
                self.core.load_loot_profile(self.client);
            }
            if route_changed {
                self.load_route_profile();
            }
            if meta_changed {
                self.load_meta_profile();
            }
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
        if !self.core.load_route_profile(self.client) {
            return false;
        }
        if self.core.initialized {
            self.apply_persisted_option_overrides();
        }
        self.core.restart_route_round(self.client);
        if self.core.initialized {
            let selected = report!(
                self.client,
                self.core.route_profiles.selected(self.client.stores())
            )
            .unwrap_or_default();
            let count = self.core.settings.navigation.waypoints.len();
            Core::announce(
                self.client,
                &format!("Loaded route {selected} ({count} points)."),
            );
        }
        true
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
    /// and the selected route file comes back.
    pub(crate) fn discard_embedded_route(&mut self) {
        if self.core.embedded_route_label.take().is_none() {
            return;
        }
        self.load_route_profile();
    }

    /// `ResetProfileConsumers` (the part the shell has).
    pub(crate) fn reset_profile_consumers(&mut self) {
        let core = &mut *self.core;
        core.vital_recharge.reset();
        core.vital_helper_recharge.reset();
        core.dispel.reset();
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
        let overrides: Vec<(String, mosstank_settings::monster::expression::MonsterValue)> = self
            .core
            .settings
            .combat
            .dynamic_settings
            .iter()
            .filter(|(name, _)| mosstank_settings::options::is_known(name))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        if overrides.is_empty() {
            return;
        }
        self.core.applying_profile_options = true;
        for (name, value) in overrides {
            let value = super::options::to_expression_value(&value);
            if let Err(error) = self.set_meta_option(&name, &value) {
                self.client.rules().log_warn(&format!(
                    "MossTank option {name} could not be applied: {}",
                    error.message()
                ));
            }
        }
        self.core.applying_profile_options = false;
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
            // The meta's own route is what is shown: nothing to read.
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

/// Whether two profile names are the same file (ordinal, any case).
#[allow(dead_code)]
pub(crate) fn same_name(a: &str, b: &str) -> bool {
    ordinal::equals_ignore_case(a, b)
}

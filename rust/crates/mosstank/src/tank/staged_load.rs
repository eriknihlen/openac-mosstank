//! The character's profile set, and the profiles a player picks, loaded a
//! step a tick (register PROF-1).
//!
//! `EnsureCharacterProfile` loads, in the frame that sees a character bind,
//! the settings profile, the loot profile, the route and the meta, the loot
//! profile and the route once more after a good settings load and the meta
//! twice when both changed; the constructor reads the set of whoever is in
//! the world when the plugin starts. A pick on the Profiles page, or `/vt
//! settings load`, `/vt loot load` and `/vt meta load`, loads one profile
//! (a settings profile with its loot profile and route). A real player's
//! folder makes that more than one plugin call should run (a 400 KB loot
//! profile parses for some 40 M instructions, a 955 KB meta for 120 M, and
//! a settings profile's side-car carries every loot rule), so here the
//! same loads run in the same order with the same lines, a tick's worth of
//! text at a time, a large file a run at a time; a set of the usual size
//! still loads in one tick. The frames in between stop after the load's
//! step: nothing of the macro acts on a set that is half loaded, and the
//! autostart, the commands and the windows wait for it. A window action
//! that arrives meanwhile finishes the load first, as it would find it
//! finished in the C#.

use std::collections::VecDeque;

use mosstank_formats::meta::MetaProfile;
use mosstank_formats::utl::{LootRule, VtankSalvageCombineSettings};
use mosstank_stores::loot_store::LootLoad;
use mosstank_stores::meta_store::{MetaLoad, MetaLoadStart};
use mosstank_stores::profile_store::{ProfileLoad, SettingsLoad, SettingsLoadStart};

use super::profiles::{SpellsSnapshot, report};
use super::{Core, TankCx};

/// How much profile text a tick's loads read and parse: some 10 M
/// instructions. Loads of smaller files run on in the same tick, so a set
/// of the usual size still loads in the frame that binds the character.
pub(crate) const LOAD_BYTES_PER_TICK: usize = 64 * 1024;

/// One load of the set, in `EnsureCharacterProfile`'s order or the
/// constructor's, or a part of a pick.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    /// `LoadSelectedProfile` up to its loot load: the settings profile.
    SelectedSettings,
    /// Its loot load, then the active loot kept when that one failed.
    SelectedLoot,
    /// Its route, the profile's options and the consumers' reset.
    SelectedRoute,
    /// `LoadLootProfile`, for a loot binding that changed or a pick.
    Loot,
    /// `LoadRouteProfile`, for a route binding that changed.
    Route,
    /// The selected meta read, a run of rules at a time.
    ReadMeta,
    /// The meta read taken by the engine (the rest of `LoadMetaProfile`).
    TakeMeta,
    /// The notice that says what was loaded.
    Notice,
    /// The constructor's settings load, with the game database.
    InitialSettings,
    /// The constructor's loot read.
    InitialLoot,
    /// The constructor's route read.
    InitialRoute,
    /// The constructor's meta.
    InitialMeta,
    /// What the constructor does once the set is read.
    InitialFinish,
    /// A loot profile picked (`SelectLootProfileCore`), once the saves due
    /// are made: the pick (`command`: `/vt loot load`).
    PickLoot { name: String, command: bool },
    /// A settings profile picked on the Profiles page (`SelectProfile`
    /// once the profile set's save is made): the pick.
    PickSettings(String),
    /// What a pick does once its loads are done.
    Finish(Finish),
}

/// The end of a pick: what it says and what it leaves for the windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Finish {
    /// `SelectLootProfileCore`'s rest; `/vt loot load` says the notice.
    LootPicked { command: bool },
    /// The rest of `SelectMetaProfileCore` from the Profiles page.
    MetaPicked,
    /// `/vt meta load`: the line said, the engine takes the meta.
    MetaCommand,
    /// `SelectProfile`'s rest from the Profiles page, or `/vt settings
    /// load`'s.
    SettingsPicked { command: bool },
}

/// The active loot `LoadSelectedProfile` keeps when the new profile's loot
/// does not load, taken before the settings load.
#[derive(Debug, Clone)]
pub(crate) struct RetainedLoot {
    retain: bool,
    rules: Vec<LootRule>,
    salvage: VtankSalvageCombineSettings,
}

/// A load of the character's profile set, or of picks, under way.
#[derive(Debug)]
pub(crate) struct StagedLoad {
    steps: VecDeque<Step>,
    retained: Option<RetainedLoot>,
    /// The settings profile whose side-car is being read.
    settings: Option<Box<SettingsLoad>>,
    /// The loot profile whose file is being parsed.
    loot: Option<LootLoad>,
    /// The meta whose file is being parsed, and the one read.
    meta: Option<Box<MetaLoad>>,
    meta_profile: Option<MetaProfile>,
    /// The last loot load's answer (`LoadLootProfile`).
    loot_loaded: bool,
    /// The last settings pick found no such profile.
    pick_failed: bool,
    /// The last settings load would not read.
    settings_failed: bool,
    /// The character the constructor's loads bind to.
    name: String,
    /// How many calls have run steps of it.
    calls: u32,
}

impl StagedLoad {
    /// The loads `EnsureCharacterProfile` makes for these binding changes.
    pub(crate) fn new(macro_changed: bool, loot: bool, route: bool, meta: bool) -> Self {
        let mut steps = VecDeque::new();
        if macro_changed {
            steps.extend([
                Step::SelectedSettings,
                Step::SelectedLoot,
                Step::SelectedRoute,
            ]);
        }
        if loot {
            steps.push_back(Step::Loot);
        }
        if route {
            steps.push_back(Step::Route);
        }
        if meta {
            steps.extend([Step::ReadMeta, Step::TakeMeta]);
        }
        // As the C# has it: after a settings load the meta, when it changed,
        // is read again.
        if macro_changed && meta {
            steps.extend([Step::ReadMeta, Step::TakeMeta]);
        }
        steps.push_back(Step::Notice);
        Self::of(steps)
    }

    /// The constructor's loads.
    pub(crate) fn initial() -> Self {
        Self::of(VecDeque::from([
            Step::InitialSettings,
            Step::InitialLoot,
            Step::InitialRoute,
            Step::InitialMeta,
            Step::InitialFinish,
        ]))
    }

    fn of(steps: VecDeque<Step>) -> Self {
        Self {
            steps,
            retained: None,
            settings: None,
            loot: None,
            meta: None,
            meta_profile: None,
            loot_loaded: false,
            pick_failed: false,
            settings_failed: false,
            name: String::new(),
            calls: 0,
        }
    }
}

impl TankCx<'_> {
    /// Runs the staged load's next steps until they have read or parsed
    /// `bytes` of profile text (a large file a run at a time, a loot
    /// profile's rules taken in as much again), or the load is over.
    /// Whether it has more to do, and the bytes the steps came to.
    pub(crate) fn step_staged_load(&mut self, bytes: usize) -> (bool, usize) {
        let Some(mut load) = self.core.staged_load.take() else {
            return (false, 0);
        };
        load.calls = load.calls.saturating_add(1);
        let mut spent = 0usize;
        let mut more = true;
        while more && spent < bytes {
            let read_before = self.client.storage_bytes_read();
            let (still, cost) = self.run_step(&mut load, bytes - spent, spent == 0);
            more = still;
            // A large file counts as it is parsed and taken in; any other
            // load by what it read.
            let cost = cost
                .unwrap_or_else(|| self.client.storage_bytes_read().saturating_sub(read_before));
            spent = spent.saturating_add(cost);
        }
        // A pick made while the steps ran comes after them.
        if let Some(added) = self.core.staged_load.take() {
            load.steps.extend(added.steps);
            more = true;
        }
        if more {
            self.core.staged_load = Some(load);
        }
        (more, spent)
    }

    /// Runs the staged load to its end (a window action came while it was
    /// under way, or the plugin stops).
    pub(crate) fn finish_staged_load(&mut self) {
        while self.step_staged_load(usize::MAX).0 {}
    }

    /// Puts `steps` after the loads under way (or starts them) and runs a
    /// tick's worth now: a profile of the usual size loads in this call.
    fn stage(&mut self, steps: impl IntoIterator<Item = Step>) {
        match self.core.staged_load.as_mut() {
            Some(load) => load.steps.extend(steps),
            None => self.core.staged_load = Some(StagedLoad::of(steps.into_iter().collect())),
        }
        self.step_staged_load(LOAD_BYTES_PER_TICK);
    }

    /// `SelectLootProfileCore`, its load a step a tick (`command`: `/vt
    /// loot load`, which says the notice).
    pub(crate) fn pick_loot_profile(&mut self, name: &str, command: bool) {
        self.stage([
            Step::PickLoot {
                name: name.to_owned(),
                command,
            },
            Step::Loot,
            Step::Finish(Finish::LootPicked { command }),
        ]);
    }

    /// `SelectMetaProfileCore` from the Profiles page, the meta read a step
    /// a tick.
    pub(crate) fn pick_meta_profile(&mut self, name: &str) {
        if super::profiles::pick_meta_profile(self.core, self.client, name) {
            self.stage([
                Step::ReadMeta,
                Step::TakeMeta,
                Step::Finish(Finish::MetaPicked),
            ]);
        } else {
            self.after_meta_profile_change();
        }
    }

    /// `/vt meta load <name>` when the name picks a meta: the meta read a
    /// step a tick. False when it picks none (the command goes on as the
    /// C#'s does, making or importing one).
    pub(crate) fn pick_meta_for_command(&mut self, name: &str, af: bool) -> bool {
        let file_name = super::commands::meta_file_name(name, af);
        match self
            .core
            .meta_profiles
            .select(self.client.stores(), Some(&file_name), af)
        {
            Ok(true) => {
                self.stage([Step::ReadMeta, Step::Finish(Finish::MetaCommand)]);
                true
            }
            Ok(false) => false,
            Err(error) => {
                super::commands::command_failed(self.client, &error);
                true
            }
        }
    }

    /// `SelectProfile` from the Profiles page: the profile set saved, the
    /// pick, and `LoadSelectedProfile`, a step a tick.
    pub(crate) fn pick_settings_profile(&mut self, name: &str) {
        // `SaveProfile` first, made once what it writes is ready.
        self.core.request_profile_save();
        self.stage([
            Step::PickSettings(name.to_owned()),
            Step::SelectedSettings,
            Step::SelectedLoot,
            Step::SelectedRoute,
            Step::Finish(Finish::SettingsPicked { command: false }),
        ]);
    }

    /// `/vt settings load`'s `LoadSelectedProfile` once the profile is
    /// picked, a step a tick.
    pub(crate) fn load_picked_settings_for_command(&mut self) {
        self.stage([
            Step::SelectedSettings,
            Step::SelectedLoot,
            Step::SelectedRoute,
            Step::Finish(Finish::SettingsPicked { command: true }),
        ]);
    }

    /// One step of `load`, with `budget` bytes left in the tick (`fresh`:
    /// nothing else ran in it yet). Whether steps remain, and for a large
    /// file's step what it came to.
    #[allow(clippy::too_many_lines)]
    fn run_step(
        &mut self,
        load: &mut StagedLoad,
        budget: usize,
        fresh: bool,
    ) -> (bool, Option<usize>) {
        let Some(step) = load.steps.front().cloned() else {
            return (false, None);
        };
        // A save still due goes to the profiles it was made for before a
        // step reads or switches one, once what it writes is ready (register
        // STORE-7).
        if self.core.settings_save_due || self.core.profile_save_due {
            if !self.core.saves_ready() {
                return (true, Some(budget));
            }
            self.core.flush_saves(self.client);
        }
        let mut cost = None;
        match step {
            Step::SelectedSettings => match load.settings.take() {
                None => {
                    let Some((start, retained)) = self.begin_selected_settings() else {
                        // The storage failed: the rest of
                        // `LoadSelectedProfile` does not run.
                        load.steps.pop_front();
                        load.steps.retain(|step| {
                            !matches!(step, Step::SelectedLoot | Step::SelectedRoute)
                        });
                        return (!load.steps.is_empty(), None);
                    };
                    load.retained = Some(retained);
                    match start {
                        SettingsLoadStart::Done(outcome) => {
                            load.steps.pop_front();
                            self.end_selected_settings(load, Some(outcome));
                        }
                        // The side-car is read by the next steps.
                        SettingsLoadStart::Reading(file) => {
                            load.settings = Some(file);
                            return (true, None);
                        }
                    }
                }
                Some(mut file) if !file.is_read() => {
                    let (_, read) = file.step(budget);
                    load.settings = Some(file);
                    return (true, Some(read));
                }
                // The rest of the document is read on a tick of its own
                // when a large side-car's rules filled this one.
                Some(file) if !fresh && file.side_car_len() > budget => {
                    load.settings = Some(file);
                    return (true, Some(budget));
                }
                Some(file) => {
                    load.steps.pop_front();
                    let outcome = self.finish_selected_settings(*file);
                    self.end_selected_settings(load, outcome);
                }
            },
            Step::SelectedLoot | Step::Loot | Step::InitialLoot => {
                let loaded = match load.loot.take() {
                    None => match if step == Step::InitialLoot {
                        self.core.begin_initial_loot_load(self.client, &load.name)
                    } else {
                        self.core.begin_loot_profile_load(self.client)
                    } {
                        // The file is parsed by the next steps.
                        Ok(file) => {
                            load.loot = Some(file);
                            return (true, Some(0));
                        }
                        Err(loaded) => loaded,
                    },
                    Some(mut file) if !file.is_parsed() => {
                        let before = file.position();
                        file.step(budget);
                        let parsed = file.position() - before;
                        load.loot = Some(file);
                        return (true, Some(parsed));
                    }
                    // Taking the rules in costs about what parsing them
                    // did: a tick of its own for a large file.
                    Some(file) if !fresh && file.byte_len() > budget => {
                        load.loot = Some(file);
                        return (true, Some(budget));
                    }
                    Some(file) => {
                        cost = Some(file.byte_len());
                        if step == Step::InitialLoot {
                            self.core.finish_initial_loot_load(self.client, file);
                            false
                        } else {
                            self.core.finish_loot_profile_load(self.client, file)
                        }
                    }
                };
                load.steps.pop_front();
                load.loot_loaded = loaded;
                if step == Step::SelectedLoot {
                    let retained = load.retained.take();
                    self.keep_active_loot(loaded, retained);
                }
            }
            Step::SelectedRoute => {
                load.steps.pop_front();
                self.load_selected_route();
            }
            Step::Route => {
                load.steps.pop_front();
                self.load_route_profile();
            }
            Step::ReadMeta => {
                let spells = SpellsSnapshot::read(self.client);
                match load.meta.take() {
                    None => {
                        let begun = report!(
                            self.client,
                            self.core.meta_profiles.begin_load(self.client.stores())
                        );
                        match begun {
                            Some(MetaLoadStart::Reading(file)) => {
                                // The file is parsed by the next steps.
                                load.meta = Some(file);
                                return (true, None);
                            }
                            Some(MetaLoadStart::Done(profile)) => {
                                load.meta_profile = Some(profile);
                            }
                            None => load.meta_profile = Some(MetaProfile::default()),
                        }
                    }
                    Some(mut file) if !file.is_parsed() => {
                        let (_, parsed) = file.step(Some(&spells), budget);
                        load.meta = Some(file);
                        return (true, Some(parsed));
                    }
                    Some(file) => {
                        let profile = self.core.meta_profiles.finish_load(
                            self.client.stores(),
                            *file,
                            Some(&spells),
                        );
                        load.meta_profile = Some(profile);
                        cost = Some(0);
                    }
                }
                load.steps.pop_front();
                self.core.announce_meta_load(self.client);
            }
            Step::TakeMeta => {
                load.steps.pop_front();
                let profile = load.meta_profile.take().unwrap_or_default();
                self.discard_embedded_route();
                self.with_engine(|engine, host, services| {
                    engine.replace_profile(host, services, profile);
                });
                if self.core.initialized {
                    self.apply_persisted_option_overrides();
                }
            }
            Step::Notice => {
                load.steps.pop_front();
                self.say_profile_loaded();
            }
            Step::InitialSettings => {
                load.steps.pop_front();
                load.name = self.initial_settings();
                // The game database it reads is the start's own work, read
                // whatever the set; the settings file is small.
                cost = Some(0);
            }
            Step::InitialRoute => {
                load.steps.pop_front();
                self.initial_route(&load.name);
            }
            Step::InitialMeta => {
                load.steps.pop_front();
                self.initial_meta(&load.name);
            }
            Step::InitialFinish => {
                // After a load of several ticks, the editors' rebuild (some
                // thousands of rows) is a tick's work of its own.
                if load.calls > 1 {
                    if !fresh {
                        return (true, Some(budget));
                    }
                    cost = Some(budget);
                }
                load.steps.pop_front();
                self.initial_finish();
            }
            Step::PickLoot { name, command } => {
                load.steps.pop_front();
                let selected = report!(
                    self.client,
                    self.core
                        .loot_profiles
                        .select(self.client.stores(), Some(&name)),
                )
                .unwrap_or(false);
                if !selected {
                    self.core.loot_editor_notice = format!("Loot profile '{name}' is unavailable.");
                    if command {
                        let notice = self.core.loot_editor_notice.clone();
                        Core::write_vtank(self.client, &notice);
                    }
                    // Its load and its end do not run.
                    while let Some(step) = load.steps.pop_front() {
                        if matches!(step, Step::Finish(_)) {
                            break;
                        }
                    }
                }
            }
            Step::PickSettings(name) => {
                load.steps.pop_front();
                let selected = report!(
                    self.client,
                    self.core.profiles.select(self.client.stores(), Some(&name)),
                )
                .unwrap_or(false);
                if !selected {
                    self.core.profile_lifecycle_notice =
                        format!("Profile '{name}' is unavailable.");
                    load.pick_failed = true;
                    // Its loads do not run; its end does.
                    while load
                        .steps
                        .front()
                        .is_some_and(|step| !matches!(step, Step::Finish(_)))
                    {
                        load.steps.pop_front();
                    }
                }
            }
            Step::Finish(finish) => {
                load.steps.pop_front();
                self.finish_pick(load, finish);
            }
        }
        (!load.steps.is_empty(), cost)
    }

    /// What a pick does once its loads are done.
    fn finish_pick(&mut self, load: &mut StagedLoad, finish: Finish) {
        match finish {
            Finish::LootPicked { command } => {
                if std::mem::take(&mut load.loot_loaded)
                    && !self.core.loot_profiles.active_profile_is_partial()
                {
                    let selected = self.core.loot_profiles.selected();
                    self.core.loot_editor_notice = format!("Loaded loot profile {selected}.");
                    self.client
                        .rules()
                        .log_info(&format!("Loaded loot profile {selected}."));
                }
                if command {
                    let notice = self.core.loot_editor_notice.clone();
                    Core::write_vtank(self.client, &notice);
                }
            }
            Finish::MetaPicked => {
                let selected = report!(
                    self.client,
                    self.core.meta_profiles.selected(self.client.stores())
                )
                .unwrap_or_default();
                if let Some(error) = self.core.meta_profiles.last_load_error() {
                    self.core.meta_notice =
                        format!("Could not load Meta profile {selected}: {error}");
                } else {
                    self.core.meta_notice = format!("Loaded Meta profile {selected}.");
                    self.client
                        .rules()
                        .log_info(&format!("Loaded Meta profile {selected}."));
                }
                self.after_meta_profile_change();
            }
            Finish::MetaCommand => {
                // A file that could not be read has already been announced.
                if self.core.meta_profiles.last_load_error().is_none() {
                    match self.core.meta_profiles.selected(self.client.stores()) {
                        Ok(selected) => Core::write_vtank(
                            self.client,
                            &format!("Loaded Meta profile {selected}."),
                        ),
                        Err(error) => super::commands::command_failed(self.client, &error),
                    }
                }
                let profile = load.meta_profile.take().unwrap_or_default();
                self.with_engine(|engine, host, services| {
                    engine.replace_profile(host, services, profile);
                });
                self.after_meta_load();
            }
            Finish::SettingsPicked { command } => {
                let failed = std::mem::take(&mut load.settings_failed);
                let unavailable = std::mem::take(&mut load.pick_failed);
                if command {
                    if failed {
                        let notice = self.core.profile_lifecycle_notice.clone();
                        Core::write_vtank(self.client, &notice);
                    } else {
                        let selected = self.core.profiles.selected();
                        Core::write_vtank(
                            self.client,
                            &format!("Loaded settings profile {selected}."),
                        );
                    }
                    return;
                }
                if !failed && !unavailable {
                    let selected = self.core.profiles.selected();
                    self.core.profile_lifecycle_notice = format!("Loaded {selected}.");
                    self.client
                        .rules()
                        .log_info(&format!("Loaded settings profile {selected}."));
                }
                self.after_profile_change();
            }
        }
    }

    /// `EnsureCharacterProfile`'s notice of what was loaded.
    fn say_profile_loaded(&mut self) {
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

    /// `LoadSelectedProfile` up to its loot load: the outcome, and the
    /// active loot to keep should the new loot not load; `None` when the
    /// storage failed (the rest does not run).
    pub(crate) fn load_selected_settings(&mut self) -> Option<(ProfileLoad, RetainedLoot)> {
        let (start, retained) = self.begin_selected_settings()?;
        let outcome = match start {
            SettingsLoadStart::Done(outcome) => outcome,
            SettingsLoadStart::Reading(file) => self.finish_selected_settings(*file)?,
        };
        Some((self.after_selected_settings(outcome), retained))
    }

    /// The settings load begun: the active loot kept aside, the settings
    /// file read and applied, its side-car to be read.
    fn begin_selected_settings(&mut self) -> Option<(SettingsLoadStart, RetainedLoot)> {
        self.core.flush_saves(self.client);
        let core = &mut *self.core;
        let retained = RetainedLoot {
            retain: core.loot_profiles.has_active_profile(),
            rules: core.settings.inventory.loot.rules.clone(),
            salvage: core.settings.inventory.loot.salvage_combine.clone(),
        };
        let begun = core.profiles.begin_load(
            self.client.stores(),
            &mut core.settings,
            &mut core.no_buff_item_names,
        );
        let start = report!(self.client, begun)?;
        Some((start, retained))
    }

    /// The settings load's side-car taken in; `None` when the storage
    /// failed.
    fn finish_selected_settings(&mut self, file: SettingsLoad) -> Option<ProfileLoad> {
        let core = &mut *self.core;
        let finished = core.profiles.finish_load(
            self.client.stores(),
            file,
            &mut core.settings,
            &mut core.no_buff_item_names,
            &mut core.command_log_types,
        );
        report!(self.client, finished)
    }

    /// A staged settings load's end: what follows the load, or the rest of
    /// `LoadSelectedProfile` given up.
    fn end_selected_settings(&mut self, load: &mut StagedLoad, outcome: Option<ProfileLoad>) {
        let Some(outcome) = outcome else {
            load.steps
                .retain(|step| !matches!(step, Step::SelectedLoot | Step::SelectedRoute));
            return;
        };
        if self.after_selected_settings(outcome) == ProfileLoad::Failed {
            // A settings profile that will not read ends the load; a pick's
            // end still says so.
            load.settings_failed = true;
            load.steps.retain(|step| matches!(step, Step::Finish(_)));
        }
    }

    /// What follows a settings load: its line, the game database and the
    /// consumables applied, and the profile's `EnableMeta`.
    fn after_selected_settings(&mut self, outcome: ProfileLoad) -> ProfileLoad {
        let core = &mut *self.core;
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
            return outcome;
        }
        // The profile's own EnableMeta decides whether the meta runs; the
        // pass is still gated on the macro running.
        self.meta.set_enabled(self.core.settings.meta.enabled);
        outcome
    }

    /// `LoadSelectedProfile` after its loot load: the active loot kept when
    /// the new loot did not load, or loaded only up to an incomplete tail.
    pub(crate) fn keep_active_loot(&mut self, loot_loaded: bool, retained: Option<RetainedLoot>) {
        let Some(RetainedLoot {
            retain,
            rules,
            salvage,
        }) = retained
        else {
            return;
        };
        let core = &mut *self.core;
        if !loot_loaded {
            if retain {
                let loot = &mut core.settings.inventory.loot;
                loot.rules = rules;
                loot.salvage_combine = salvage;
                loot.profile_active = true;
            }
        } else if retain && core.loot_profiles.active_profile_is_partial() {
            let loot = &mut core.settings.inventory.loot;
            loot.rules = rules;
            loot.salvage_combine = salvage;
            core.loot_editor_notice = format!(
                "Loot profile {} has an incomplete tail; current rules were retained and source was preserved.",
                core.loot_profiles.selected()
            );
            self.client
                .rules()
                .log_warn(&core.loot_editor_notice.clone());
        }
    }

    /// `LoadSelectedProfile`'s last part: the route, the options the
    /// profile carries and the consumers' reset.
    pub(crate) fn load_selected_route(&mut self) {
        self.load_route_profile();
        self.apply_persisted_option_overrides();
        self.reset_profile_consumers();
    }
}

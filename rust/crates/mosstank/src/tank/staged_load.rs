//! The character's profile set, loaded a step a tick (register PROF-1).
//!
//! `EnsureCharacterProfile` loads, in the frame that sees a character bind,
//! the settings profile, the loot profile, the route and the meta, the loot
//! profile and the route once more after a good settings load and the meta
//! twice when both changed; the constructor reads the set of whoever is in
//! the world when the plugin starts. A real player's folder makes that more
//! than one plugin call may run (a 400 KB loot profile parses for some 60 M
//! instructions, and is read twice), so here the same loads run in the same
//! order with the same lines, a tick's worth of text at a time, a large loot
//! profile's rules a run at a time; a set of the usual size still loads in
//! one tick. The frames in between stop after the load's step: nothing of
//! the macro acts on a set that is half loaded, and the autostart and the
//! windows wait for it. A command or a window action that arrives meanwhile
//! finishes the load first, as it would find it finished in the C#.

use std::collections::VecDeque;

use mosstank_formats::utl::{LootRule, VtankSalvageCombineSettings};
use mosstank_stores::loot_store::LootLoad;
use mosstank_stores::profile_store::ProfileLoad;

use super::TankCx;

/// How much profile text a tick's loads read and parse: some 10 M
/// instructions. Loads of smaller files run on in the same tick, so a set
/// of the usual size still loads in the frame that binds the character.
pub(crate) const LOAD_BYTES_PER_TICK: usize = 64 * 1024;

/// One load of the set, in `EnsureCharacterProfile`'s order or the
/// constructor's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// `LoadSelectedProfile` up to its loot load: the settings profile.
    SelectedSettings,
    /// Its loot load, then the active loot kept when that one failed.
    SelectedLoot,
    /// Its route, the profile's options and the consumers' reset.
    SelectedRoute,
    /// `LoadLootProfile`, for a loot binding that changed.
    Loot,
    /// `LoadRouteProfile`, for a route binding that changed.
    Route,
    /// `LoadMetaProfile`, for a meta binding that changed.
    Meta,
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
}

/// The active loot `LoadSelectedProfile` keeps when the new profile's loot
/// does not load, taken before the settings load.
#[derive(Debug, Clone)]
pub(crate) struct RetainedLoot {
    retain: bool,
    rules: Vec<LootRule>,
    salvage: VtankSalvageCombineSettings,
}

/// A load of the character's profile set under way.
#[derive(Debug)]
pub(crate) struct StagedLoad {
    steps: VecDeque<Step>,
    retained: Option<RetainedLoot>,
    /// The loot profile whose file is being parsed.
    loot: Option<LootLoad>,
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
            steps.push_back(Step::Meta);
        }
        // As the C# has it: after a settings load the meta, when it changed,
        // is read again.
        if macro_changed && meta {
            steps.push_back(Step::Meta);
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
            loot: None,
            name: String::new(),
            calls: 0,
        }
    }
}

impl TankCx<'_> {
    /// Runs the staged load's next steps until they have read or parsed
    /// `bytes` of profile text (a loot profile's rules a run at a time, and
    /// its rules taken in as much again), or the load is over. Whether it
    /// has more to do, and the bytes the steps came to.
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
            // A loot profile's file counts as it is parsed and taken in; any
            // other load by what it read.
            let cost = cost
                .unwrap_or_else(|| self.client.storage_bytes_read().saturating_sub(read_before));
            spent = spent.saturating_add(cost);
        }
        if more {
            self.core.staged_load = Some(load);
        }
        (more, spent)
    }

    /// Runs the staged load to its end (a command or a window action came
    /// while it was under way, or the plugin stops).
    pub(crate) fn finish_staged_load(&mut self) {
        while self.step_staged_load(usize::MAX).0 {}
    }

    /// One step of `load`, with `budget` bytes left in the tick (`fresh`:
    /// nothing else ran in it yet). Whether steps remain, and for a loot
    /// profile's step what it came to.
    fn run_step(
        &mut self,
        load: &mut StagedLoad,
        budget: usize,
        fresh: bool,
    ) -> (bool, Option<usize>) {
        let Some(step) = load.steps.front().copied() else {
            return (false, None);
        };
        let mut cost = None;
        match step {
            Step::SelectedSettings => {
                load.steps.pop_front();
                match self.load_selected_settings() {
                    // The storage failed: the rest of `LoadSelectedProfile`
                    // does not run.
                    None => {
                        load.steps.retain(|step| {
                            !matches!(step, Step::SelectedLoot | Step::SelectedRoute)
                        });
                    }
                    // A settings profile that will not read ends the load.
                    Some((ProfileLoad::Failed, _)) => load.steps.clear(),
                    Some((_, retained)) => load.retained = Some(retained),
                }
            }
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
            Step::Meta => {
                load.steps.pop_front();
                self.load_meta_profile();
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
        }
        (!load.steps.is_empty(), cost)
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
        let core = &mut *self.core;
        let retained = RetainedLoot {
            retain: core.loot_profiles.has_active_profile(),
            rules: core.settings.inventory.loot.rules.clone(),
            salvage: core.settings.inventory.loot.salvage_combine.clone(),
        };
        let loaded = core.profiles.load_current(
            self.client.stores(),
            &mut core.settings,
            &mut core.no_buff_item_names,
            &mut core.command_log_types,
        );
        let outcome = super::profiles::report!(self.client, loaded)?;
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
            return Some((outcome, retained));
        }
        // The profile's own EnableMeta decides whether the meta runs; the
        // pass is still gated on the macro running.
        self.meta.set_enabled(self.core.settings.meta.enabled);
        Some((outcome, retained))
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

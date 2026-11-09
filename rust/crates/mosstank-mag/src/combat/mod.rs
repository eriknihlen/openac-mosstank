//! The combat tracker (Mag-Tools' `CombatTracker`): the chat's attacks,
//! kills and surges per source and target, kept twice: the current
//! session's stats and the persistent ones, which are read from the stats
//! file at the login and written at the logoff (and every 10 minutes).
//!
//! - [`patterns`]: the chat lines, Mag-Tools' patterns and the fixes.
//! - [`info`]: what is kept.
//! - [`xml`]: the stats files.
//! - [`gui`]: the Combat page's two lists of each tracker.
//!
//! [`Combat`] is what the plugin drives: the patterns (compiled a few a
//! tick), both trackers, the files. A line is parsed once and counted by
//! both trackers (Mag-Tools parsed it once per tracker, to the same
//! result). Each info's text in the stats file is kept current a few a
//! tick, so that writing a file (the logoff is one call) only joins them.
//!
//! Where it differs from Mag-Tools: the fixes F1-F6 ([`patterns`]); F10: a
//! line the plugin posted itself is not read back ([`PostedLines`]); F15:
//! an "Unable to parse …" line is written once (Mag-Tools' two trackers
//! each wrote it); F16: a persistent file that does not read is kept
//! aside (`<name>.CombatTracker.broken.xml`) before it is written over.

pub mod gui;
pub mod info;
pub mod patterns;
pub mod xml;

use std::collections::{HashMap, VecDeque};

use mosstank_dotnet::collections::LookupMap;

use ac_plugin_abi::{ChatLine, LogLevel};

use self::gui::{CombatGui, StatsPage};
use self::info::{CombatEvent, CombatInfo, SurgeEvent, SurgeInfo};
use self::patterns::{ParsedLine, Patterns};
use self::xml::{ImportError, ImportFailure, StagedImport};
use crate::host::MagHost;

/// The patterns compiled a tick, by their cost ([`patterns::compile_cost`]):
/// the dearest (two Unicode word classes) alone, or about six plain ones.
pub const COMPILE_BUDGET_PER_TICK: usize = 12;

/// Lines that arrived before the patterns were compiled, kept to be
/// read once they are; the ones past this are not read.
pub const WAITING_LINES_MAX: usize = 512;

/// Waiting lines read a tick once the patterns are compiled (some two
/// million instructions each at worst); a tick that reads them does
/// nothing else of the tracker's.
pub const DRAIN_LINES_PER_TICK: usize = 10;

/// Infos whose stats-file text is written again a tick, in each tracker.
pub const TEXTS_PER_TICK: usize = 100;

/// How often the persistent stats are written while in the world
/// (Mag-Tools' `savePersistentStatsTimer`, 10 minutes).
pub const SAVE_INTERVAL_MS: f64 = 600_000.0;

/// Mag-Tools' own chat lines: this prefix, in the system colour.
pub const CHAT_PREFIX: &str = "<{Mag-Tools}>: ";
/// The chat type Mag-Tools' lines are written in.
pub const CHAT_TYPE: u32 = 5;

/// The updates the pages follow, at most this many kept before they are
/// read (past it: [`TrackerUpdate::Overflowed`]).
pub const UPDATES_MAX: usize = 10_000;

/// What changed in a tracker (Mag-Tools' `CombatInfoUpdated`,
/// `AetheriaInfoUpdated`, `CloakInfoUpdated`, `InfoCleared`,
/// `StatsLoaded`), for the lists to follow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackerUpdate {
    /// The combat info of `source` on `target` changed.
    Combat {
        /// Who attacked.
        source: String,
        /// Who was attacked.
        target: String,
    },
    /// The aetheria info of `source` on `target` changed.
    Aetheria {
        /// Whose aetheria.
        source: String,
        /// On whom.
        target: String,
    },
    /// The cloak info of `source` on `target` changed.
    Cloak {
        /// Whose cloak.
        source: String,
        /// On whom.
        target: String,
    },
    /// Everything was cleared.
    Cleared,
    /// The stats file was read in.
    Loaded,
    /// More changed than was kept: build everything again.
    Overflowed,
}

/// The stats of one tracker: its three lists in the order first seen.
#[derive(Debug, Default)]
pub struct CombatTracker {
    combat: Vec<CombatInfo>,
    combat_index: HashMap<(String, String), usize>,
    /// The combat infos each name is the source or target of, in order
    /// (`GetCombatInfos` without a walk of every info).
    by_name: LookupMap<String, Vec<usize>>,
    /// Each combat info's text in the stats file, kept current a few a
    /// tick ([`Self::refresh_texts`]) so that a write only joins them;
    /// empty while stale.
    texts: Vec<String>,
    /// The infos whose text is stale, in the order they changed.
    stale: VecDeque<usize>,
    is_stale: Vec<bool>,
    aetheria: Vec<SurgeInfo>,
    cloak: Vec<SurgeInfo>,
    updates: VecDeque<TrackerUpdate>,
    revision: u64,
}

impl CombatTracker {
    /// The combat infos, in the order first seen.
    #[must_use]
    pub fn combat_infos(&self) -> &[CombatInfo] {
        &self.combat
    }

    /// The aetheria infos, in the order first seen.
    #[must_use]
    pub fn aetheria_infos(&self) -> &[SurgeInfo] {
        &self.aetheria
    }

    /// The cloak infos, in the order first seen.
    #[must_use]
    pub fn cloak_infos(&self) -> &[SurgeInfo] {
        &self.cloak
    }

    /// `GetCombatInfos(name)`: the infos `name` is the source or target
    /// of, in the order first seen.
    pub fn combat_infos_of<'a>(&'a self, name: &str) -> impl Iterator<Item = &'a CombatInfo> {
        self.by_name
            .get(name)
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .map(|&at| &self.combat[at])
    }

    /// The info at `at` noted under its source's and its target's names.
    fn index_names(&mut self, at: usize) {
        let info = &self.combat[at];
        let (source, target) = (info.source.clone(), info.target.clone());
        if target != source {
            self.by_name.entry(target).or_default().push(at);
        }
        self.by_name.entry(source).or_default().push(at);
    }

    /// `GetAetheriaInfos(name)`.
    pub fn aetheria_infos_of<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a SurgeInfo> {
        self.aetheria
            .iter()
            .filter(move |info| info.source == name || info.target == name)
    }

    /// `GetCloakInfos(name)`.
    pub fn cloak_infos_of<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a SurgeInfo> {
        self.cloak
            .iter()
            .filter(move |info| info.source == name || info.target == name)
    }

    /// Nothing tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.combat.is_empty() && self.aetheria.is_empty() && self.cloak.is_empty()
    }

    /// Counts up on every change (to know whether a list must be built
    /// again).
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The changes since last asked, in order.
    pub fn take_updates(&mut self) -> Vec<TrackerUpdate> {
        self.updates.drain(..).collect()
    }

    fn note(&mut self, update: TrackerUpdate) {
        self.revision += 1;
        if self.updates.len() >= UPDATES_MAX {
            self.updates.clear();
            self.updates.push_back(TrackerUpdate::Overflowed);
        }
        if self.updates.back() != Some(&TrackerUpdate::Overflowed) {
            self.updates.push_back(update);
        } else if matches!(update, TrackerUpdate::Cleared | TrackerUpdate::Loaded) {
            // The lists must hear it before they build everything again (a
            // load after a clear: both).
            let cleared = update == TrackerUpdate::Loaded
                && self.updates.front() == Some(&TrackerUpdate::Cleared);
            self.updates.clear();
            if cleared {
                self.updates.push_back(TrackerUpdate::Cleared);
            }
            self.updates.push_back(update);
            self.updates.push_back(TrackerUpdate::Overflowed);
        }
    }

    /// `standardTracker_CombatEvent`: an event `me` took part in is added
    /// to its info (made at the end when new).
    pub fn add_combat(&mut self, me: &str, event: &CombatEvent) {
        if event.source != me && event.target != me {
            return;
        }
        let key = (event.source.clone(), event.target.clone());
        let at = match self.combat_index.get(&key) {
            Some(&at) => at,
            None => {
                self.combat
                    .push(CombatInfo::new(&event.source, &event.target));
                self.texts.push(String::new());
                self.is_stale.push(false);
                self.combat_index.insert(key, self.combat.len() - 1);
                self.index_names(self.combat.len() - 1);
                self.combat.len() - 1
            }
        };
        self.combat[at].add(event);
        if !self.is_stale[at] {
            self.is_stale[at] = true;
            self.texts[at].clear();
            self.stale.push_back(at);
        }
        self.note(TrackerUpdate::Combat {
            source: event.source.clone(),
            target: event.target.clone(),
        });
    }

    /// `aetheriaTracker_SurgeEvent`: only our own surges.
    pub fn add_aetheria(&mut self, me: &str, event: &SurgeEvent) {
        if event.source != me {
            return;
        }
        add_surge(&mut self.aetheria, event);
        self.note(TrackerUpdate::Aetheria {
            source: event.source.clone(),
            target: event.target.clone(),
        });
    }

    /// `cloakTracker_SurgeEvent`: only our own surges.
    pub fn add_cloak(&mut self, me: &str, event: &SurgeEvent) {
        if event.source != me {
            return;
        }
        add_surge(&mut self.cloak, event);
        self.note(TrackerUpdate::Cloak {
            source: event.source.clone(),
            target: event.target.clone(),
        });
    }

    /// Everything a parsed line told.
    pub fn add_parsed(&mut self, me: &str, parsed: &ParsedLine) {
        if let Some(event) = &parsed.combat {
            self.add_combat(me, event);
        }
        if let Some(event) = &parsed.aetheria {
            self.add_aetheria(me, event);
        }
        if let Some(event) = &parsed.cloak {
            self.add_cloak(me, event);
        }
    }

    /// `ClearStats`: the three lists emptied.
    pub fn clear(&mut self) {
        self.clear_lists();
        self.note(TrackerUpdate::Cleared);
    }

    /// The lists emptied, telling no one (the import's first step).
    pub(crate) fn clear_lists(&mut self) {
        self.replace_lists(Self::default());
    }

    /// The lists replaced by `other`'s (a file read in aside), telling no
    /// one.
    pub(crate) fn replace_lists(&mut self, other: Self) {
        *self = Self {
            updates: std::mem::take(&mut self.updates),
            revision: self.revision,
            ..other
        };
    }

    /// The stats file's text of the combat info `at`, `None` while stale.
    #[must_use]
    pub(crate) fn kept_text(&self, at: usize) -> Option<&str> {
        (!self.is_stale.get(at).copied().unwrap_or(true)).then(|| self.texts[at].as_str())
    }

    /// Writes again the text of up to `budget` infos that changed, in the
    /// order they changed: how many.
    pub fn refresh_texts(&mut self, budget: usize) -> usize {
        let mut done = 0;
        while done < budget {
            let Some(at) = self.stale.pop_front() else {
                break;
            };
            self.texts[at] = xml::info_text(&self.combat[at]);
            self.is_stale[at] = false;
            done += 1;
        }
        done
    }

    /// How many infos' texts are stale.
    #[must_use]
    pub fn stale_texts(&self) -> usize {
        self.stale.len()
    }

    /// An info read from a file, added at the end as it is, with its text.
    pub(crate) fn push_combat_info(&mut self, info: CombatInfo, text: String) {
        let key = (info.source.clone(), info.target.clone());
        self.combat.push(info);
        self.texts.push(text);
        self.is_stale.push(false);
        // A file with the same pair twice: the first is the one found.
        self.combat_index
            .entry(key)
            .or_insert(self.combat.len() - 1);
        self.index_names(self.combat.len() - 1);
    }

    pub(crate) fn push_aetheria_info(&mut self, info: SurgeInfo) {
        self.aetheria.push(info);
    }

    pub(crate) fn push_cloak_info(&mut self, info: SurgeInfo) {
        self.cloak.push(info);
    }

    pub(crate) fn note_loaded(&mut self) {
        self.note(TrackerUpdate::Loaded);
    }
}

fn add_surge(infos: &mut Vec<SurgeInfo>, event: &SurgeEvent) {
    let info = match infos
        .iter()
        .position(|info| info.source == event.source && info.target == event.target)
    {
        Some(at) => &mut infos[at],
        None => {
            infos.push(SurgeInfo::new(&event.source, &event.target));
            infos.last_mut().expect("just pushed")
        }
    };
    info.total_surges = info.total_surges.wrapping_add(1);
}

/// F10: the lines the plugin posted itself, by text, so that hearing one
/// back does not count it (an "Unable to parse …" line quotes a combat
/// line, which a pattern would take again). A line is heard back right
/// after the call that posted it, as the client shows it (trimmed); one
/// not heard by the second tick after is forgotten.
#[derive(Debug, Default)]
pub struct PostedLines {
    lines: Vec<(String, u8)>,
}

impl PostedLines {
    /// The plugin posted `text`.
    pub fn note(&mut self, text: String) {
        let trimmed = text.trim();
        let text = if trimmed.len() == text.len() {
            text
        } else {
            trimmed.to_owned()
        };
        self.lines.push((text, 0));
    }

    /// The line `text` was heard: true (and forgotten) when the plugin
    /// posted it.
    pub fn take(&mut self, text: &str) -> bool {
        match self.lines.iter().position(|(posted, _)| posted == text) {
            Some(at) => {
                self.lines.remove(at);
                true
            }
            None => false,
        }
    }

    /// A tick passed.
    pub fn tick(&mut self) {
        for (_, age) in &mut self.lines {
            *age = age.saturating_add(1);
        }
        self.lines.retain(|(_, age)| *age < 2);
    }

    /// Nothing waits to be heard back.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// A line waiting for the patterns.
#[derive(Debug, Clone)]
struct Waiting {
    display_text: String,
    kind: u32,
    combat_kind: u32,
    /// Heard before the login completed.
    before_login: bool,
    /// Who counts it: a tracker cleared after the line was heard does not
    /// (Mag-Tools counted it before the clear).
    counted_by: CountedBy,
}

/// The trackers a line is counted in.
#[derive(Debug, Clone, Copy)]
struct CountedBy {
    current: bool,
    persistent: bool,
}

impl CountedBy {
    const BOTH: Self = Self {
        current: true,
        persistent: true,
    };
}

/// What the persistent tracker heard while its file was read in, added
/// after it.
#[derive(Debug, Default)]
struct Heard {
    /// Heard before the login completed: Mag-Tools counted them before it
    /// read the file in, which replaced them; kept only when the file
    /// replaced nothing.
    before_login: Vec<ParsedLine>,
    /// Heard after.
    after_login: Vec<ParsedLine>,
}

/// Where the persistent stats' file is in the session.
#[derive(Debug, Default)]
enum Persistence {
    /// Before the login completed.
    #[default]
    Idle,
    /// Being read in, a step a tick; what the persistent tracker heard
    /// meanwhile waits to be added after.
    Importing {
        import: Box<StagedImport>,
        heard: Heard,
    },
    /// In the world: written every [`SAVE_INTERVAL_MS`].
    InWorld {
        /// When it was last written (or the login completed).
        saved_ms: f64,
    },
}

/// The settings the combat tracker reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CombatSettings {
    /// `Mag.CombatTracker.Persistent`: keep the persistent stats' file.
    pub persistent: bool,
    /// `Mag.CombatTracker.ExportOnLogOff`.
    pub export_on_logoff: bool,
    /// `Mag.CombatTracker.SortAlphabetically`: the monster lists sorted by
    /// name.
    pub sort_alphabetically: bool,
}

/// The combat tracker as the plugin drives it.
#[derive(Debug, Default)]
pub struct Combat {
    patterns: Patterns,
    /// The current session's stats.
    pub current: CombatTracker,
    /// The stats kept across sessions.
    pub persistent: CombatTracker,
    posted: PostedLines,
    waiting: VecDeque<Waiting>,
    /// A line was dropped (past [`WAITING_LINES_MAX`]) and logged.
    dropped: bool,
    /// The character's name and world, once known.
    me: Option<(String, String)>,
    persistence: Persistence,
    /// F16: the persistent file did not read and could not be kept aside:
    /// it is not written over this session.
    keep_persistent_file: bool,
    /// The current stats' lists.
    current_gui: CombatGui,
    /// The persistent stats' lists.
    persistent_gui: CombatGui,
}

impl Combat {
    /// The current stats' lists.
    #[must_use]
    pub fn current_gui(&self) -> &CombatGui {
        &self.current_gui
    }

    /// The persistent stats' lists.
    #[must_use]
    pub fn persistent_gui(&self) -> &CombatGui {
        &self.persistent_gui
    }

    /// One tracker's lists.
    #[must_use]
    pub fn gui(&self, page: StatsPage) -> &CombatGui {
        match page {
            StatsPage::Current => &self.current_gui,
            StatsPage::Persistent => &self.persistent_gui,
        }
    }

    /// The character's name as known ("" before it is).
    fn me_name(&self) -> String {
        self.me
            .as_ref()
            .map(|(name, _)| name.clone())
            .unwrap_or_default()
    }

    /// One tracker and its lists.
    fn page_parts(&mut self, page: StatsPage) -> (&mut CombatTracker, &mut CombatGui) {
        match page {
            StatsPage::Current => (&mut self.current, &mut self.current_gui),
            StatsPage::Persistent => (&mut self.persistent, &mut self.persistent_gui),
        }
    }

    /// Both lists follow their tracker's updates (the names; the numbers
    /// wait for [`Self::refresh_page`]). Cheap: every frame.
    pub fn follow_pages(&mut self, sort: bool) {
        let me = self.me_name();
        self.current_gui.follow(&mut self.current, &me, sort);
        self.persistent_gui.follow(&mut self.persistent, &me, sort);
    }

    /// One page's numbers worked out (while it is shown).
    pub fn refresh_page(&mut self, page: StatsPage) {
        let me = self.me_name();
        let (tracker, gui) = self.page_parts(page);
        gui.refresh_some(tracker, &me, gui::ROWS_PER_REFRESH);
    }

    /// Both lists followed and worked out.
    pub fn update_pages(&mut self, host: &mut dyn MagHost, sort: bool) {
        self.me(host);
        self.follow_pages(sort);
        self.refresh_page(StatsPage::Current);
        self.refresh_page(StatsPage::Persistent);
    }

    /// A click on a page's monster list.
    pub fn click_monster(&mut self, page: StatsPage, row: usize, sort: bool) {
        let me = self.me_name();
        let (tracker, gui) = self.page_parts(page);
        gui.click(tracker, &me, sort, row);
    }

    /// The "Clear Current Stats" button.
    pub fn clear_current(&mut self) {
        self.current.clear();
        for waiting in &mut self.waiting {
            waiting.counted_by.current = false;
        }
    }

    /// The "Clear Persistent Stats" button: the persistent stats cleared
    /// (a file being read in is no longer), and their file deleted, with
    /// `File deleted: <path>` in chat when there was one.
    pub fn clear_persistent(&mut self, host: &mut dyn MagHost) {
        if self.is_importing() {
            self.persistence = Persistence::InWorld {
                saved_ms: host.now_ms(),
            };
        }
        self.persistent.clear();
        // The lines still waiting were heard before the clear (and those
        // heard before the login, before the file would have replaced them).
        for waiting in &mut self.waiting {
            waiting.counted_by.persistent = false;
        }
        if self.me(host).is_none() {
            return;
        }
        let (name, world) = self.me.clone().unwrap_or_default();
        let key = xml::persistent_key(&world, &name);
        match host.storage().delete(&key) {
            Ok(true) => {
                // Nothing left to keep from being written over.
                self.keep_persistent_file = false;
                let path = shown_path(host, &key);
                let text = format!("{CHAT_PREFIX}File deleted: {path}");
                host.chat_post(&text, CHAT_TYPE);
                self.posted.note(text);
            }
            Ok(false) => {}
            Err(error) => host.log(
                LogLevel::Warn,
                &format!("The combat stats {key} were not deleted: {error}"),
            ),
        }
    }

    /// The patterns are compiled.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.patterns.is_ready()
    }

    /// The character's name, asked for once.
    fn me(&mut self, host: &mut dyn MagHost) -> Option<&str> {
        if self.me.is_none() {
            let character = host.character().filter(|c| !c.name.is_empty())?;
            self.me = Some((character.name, character.world));
        }
        self.me.as_ref().map(|(name, _)| name.as_str())
    }

    /// A chat line arrived: read now, or kept until the patterns are
    /// compiled.
    pub fn on_chat(&mut self, host: &mut dyn MagHost, line: &ChatLine) {
        if line.display_text.is_empty() || self.posted.take(&line.display_text) {
            return;
        }
        if !self.patterns.is_ready() || !self.waiting.is_empty() {
            if self.waiting.len() < WAITING_LINES_MAX {
                self.waiting.push_back(Waiting {
                    display_text: line.display_text.clone(),
                    kind: line.kind,
                    combat_kind: line.combat_kind,
                    before_login: false,
                    counted_by: CountedBy::BOTH,
                });
            } else if !self.dropped {
                self.dropped = true;
                host.log(
                    LogLevel::Warn,
                    &format!(
                        "The combat tracker had {WAITING_LINES_MAX} chat lines waiting for it: later ones are not counted until it catches up."
                    ),
                );
            }
            return;
        }
        self.read(host, line, false, CountedBy::BOTH);
    }

    fn read(
        &mut self,
        host: &mut dyn MagHost,
        line: &ChatLine,
        before_login: bool,
        counted_by: CountedBy,
    ) {
        let Some(me) = self.me(host).map(str::to_owned) else {
            return;
        };
        let Some(parsed) = self.patterns.parse(&me, line) else {
            return;
        };
        if parsed.combat.is_none() && parsed.aetheria.is_none() && parsed.cloak.is_none() {
            return;
        }
        // F15: once (Mag-Tools' two trackers each wrote it).
        for warning in &parsed.warnings {
            let text = format!("{CHAT_PREFIX}{warning}");
            host.chat_post(&text, CHAT_TYPE);
            self.posted.note(text);
        }
        if counted_by.current {
            self.current.add_parsed(&me, &parsed);
        }
        if !counted_by.persistent {
            return;
        }
        match &mut self.persistence {
            Persistence::Importing { heard, .. } if before_login => heard.before_login.push(parsed),
            Persistence::Importing { heard, .. } => heard.after_login.push(parsed),
            _ => self.persistent.add_parsed(&me, &parsed),
        }
    }

    /// One tick: a few patterns compiled; else a few of the lines that
    /// waited for them; else the persistent file's import step, or its
    /// periodic write; else the stats' texts kept current. True when it
    /// did a tick's worth of work.
    pub fn tick(&mut self, host: &mut dyn MagHost, settings: CombatSettings) -> bool {
        self.posted.tick();
        if !self.patterns.is_ready() {
            self.patterns.compile_some(COMPILE_BUDGET_PER_TICK);
            return true;
        }
        if !self.waiting.is_empty() {
            for _ in 0..DRAIN_LINES_PER_TICK {
                let Some(waiting) = self.waiting.pop_front() else {
                    break;
                };
                let line = ChatLine {
                    display_text: waiting.display_text,
                    kind: waiting.kind,
                    combat_kind: waiting.combat_kind,
                    ..ChatLine::default()
                };
                self.read(host, &line, waiting.before_login, waiting.counted_by);
            }
            if self.waiting.is_empty() {
                self.dropped = false;
            }
            return true;
        }
        match &mut self.persistence {
            Persistence::Idle => {}
            Persistence::Importing { import, heard } => {
                if !import.step(host, &mut self.persistent) {
                    return true;
                }
                let heard = std::mem::take(heard);
                let outcome = import.take_outcome();
                let me = self
                    .me
                    .as_ref()
                    .map(|(name, _)| name.clone())
                    .unwrap_or_default();
                if !outcome.replaced {
                    for parsed in &heard.before_login {
                        self.persistent.add_parsed(&me, parsed);
                    }
                }
                for parsed in &heard.after_login {
                    self.persistent.add_parsed(&me, parsed);
                }
                self.persistence = Persistence::InWorld {
                    saved_ms: host.now_ms(),
                };
                self.keep_aside(host, outcome.failure);
                return true;
            }
            Persistence::InWorld { saved_ms } => {
                let now = host.now_ms();
                if now - *saved_ms >= SAVE_INTERVAL_MS {
                    *saved_ms = now;
                    if settings.persistent {
                        self.save_persistent(host);
                        return true;
                    }
                }
            }
        }
        let refreshed = self.current.refresh_texts(TEXTS_PER_TICK)
            + self.persistent.refresh_texts(TEXTS_PER_TICK);
        refreshed >= TEXTS_PER_TICK
    }

    /// F16: a persistent file that did not read is copied aside before
    /// anything is written over it; one that cannot be (not read, or the
    /// copy not written) is left alone this session.
    fn keep_aside(&mut self, host: &mut dyn MagHost, failure: Option<ImportFailure>) {
        let Some(failure) = failure else {
            return;
        };
        let (name, world) = self.me.clone().unwrap_or_default();
        let key = xml::persistent_key(&world, &name);
        let ImportFailure::Broken(text) = failure else {
            self.keep_persistent_file = true;
            host.log(
                LogLevel::Warn,
                &format!(
                    "The combat stats {key} could not be read: they are not written over this session."
                ),
            );
            return;
        };
        let aside = xml::broken_key(&world, &name);
        match host.storage().write_text(&aside, &text) {
            Ok(()) => host.log(
                LogLevel::Warn,
                &format!("The combat stats {key} did not read: kept as {aside}, and written anew."),
            ),
            Err(error) => {
                self.keep_persistent_file = true;
                host.log(
                    LogLevel::Warn,
                    &format!(
                        "The combat stats {key} did not read and could not be kept as {aside} ({error}): they are not written over this session."
                    ),
                );
            }
        }
    }

    /// The login completed: the persistent stats' file is read in (a step
    /// a tick) when they are kept. The lines still waiting were heard
    /// before it.
    pub fn on_login_complete(&mut self, host: &mut dyn MagHost, settings: CombatSettings) {
        self.me = None;
        let me = self.me(host).is_some();
        // The persistent lists worked out before the file replaces the
        // counts (Mag-Tools drew them as each line came).
        let name = self.me_name();
        self.persistent_gui
            .update(&mut self.persistent, &name, settings.sort_alphabetically);
        for waiting in &mut self.waiting {
            waiting.before_login = true;
        }
        self.persistence = match (settings.persistent, me, self.me.clone()) {
            (true, true, Some((name, world))) => Persistence::Importing {
                import: Box::new(StagedImport::new(xml::persistent_key(&world, &name))),
                heard: Heard::default(),
            },
            _ => Persistence::InWorld {
                saved_ms: host.now_ms(),
            },
        };
    }

    /// Logging off: the persistent stats written when they are kept, and
    /// the current ones exported when asked (one call, which only joins
    /// the texts kept current; an import not yet finished writes nothing
    /// over the file).
    pub fn on_logoff(&mut self, host: &mut dyn MagHost, settings: CombatSettings) {
        let importing = matches!(self.persistence, Persistence::Importing { .. });
        if settings.persistent && !importing {
            self.save_persistent(host);
        }
        if settings.export_on_logoff {
            self.export_current(host, false);
        }
        self.persistence = Persistence::Idle;
    }

    /// The character left the world: the session's stats and state are
    /// forgotten (the compiled patterns are kept).
    pub fn reset(&mut self) {
        let patterns = std::mem::take(&mut self.patterns);
        *self = Self {
            patterns,
            ..Self::default()
        };
    }

    /// `ExportPersistentStats`: the persistent stats to their file (nothing
    /// written while they are empty, nor over a file F16 left alone).
    pub fn save_persistent(&mut self, host: &mut dyn MagHost) {
        if self.keep_persistent_file {
            return;
        }
        let Some(me) = self.me(host).map(str::to_owned) else {
            return;
        };
        let world = self.me.as_ref().map(|(_, w)| w.clone()).unwrap_or_default();
        let key = xml::persistent_key(&world, &me);
        write_stats(host, &self.persistent, &key);
    }

    /// The "Export Current Stats" button (and the logoff export): the
    /// current stats to a file named for the minute; with `show_message`,
    /// `Stats exported to: <path>` in chat. The key written, `None` when
    /// nothing was (no stats, no character, or the write failed).
    pub fn export_current(&mut self, host: &mut dyn MagHost, show_message: bool) -> Option<String> {
        let me = self.me(host)?.to_owned();
        let world = self.me.as_ref().map(|(_, w)| w.clone()).unwrap_or_default();
        if self.current.is_empty() {
            return None;
        }
        let local = openac_plugin_compat::datetime::DateTime::local_from_unix_millis(
            host.utc_unix_ms(),
            host.local_offset_minutes(),
        );
        let key = xml::export_key(&world, &me, local);
        if !write_stats(host, &self.current, &key) {
            return None;
        }
        if show_message {
            let path = shown_path(host, &key);
            let text = format!("{CHAT_PREFIX}Stats exported to: {path}");
            host.chat_post(&text, CHAT_TYPE);
            self.posted.note(text);
        }
        Some(key)
    }

    /// The persistent file is being read in.
    #[must_use]
    pub fn is_importing(&self) -> bool {
        matches!(self.persistence, Persistence::Importing { .. })
    }

    /// Between the login and the logoff: what logging off writes is due.
    #[must_use]
    pub fn is_logged_in(&self) -> bool {
        !matches!(self.persistence, Persistence::Idle)
    }
}

/// `ExportStats`: nothing written for an empty tracker. True when written.
fn write_stats(host: &mut dyn MagHost, tracker: &CombatTracker, key: &str) -> bool {
    if tracker.is_empty() {
        return false;
    }
    let text = xml::export(tracker);
    match host.storage().write_text(key, &text) {
        Ok(()) => true,
        Err(error) => {
            host.log(
                LogLevel::Warn,
                &format!("The combat stats {key} were not written: {error}"),
            );
            false
        }
    }
}

/// Where a key is on disk, as the player reads it (the key alone when the
/// storage has no folder).
fn shown_path(host: &mut dyn MagHost, key: &str) -> String {
    match host.storage().root_path() {
        Some(root) if root.contains('\\') => {
            format!(
                "{}\\{}",
                root.trim_end_matches('\\'),
                key.replace('/', "\\")
            )
        }
        Some(root) => format!("{}/{key}", root.trim_end_matches('/')),
        None => key.to_owned(),
    }
}

/// Why an import stopped (for the log line).
pub(crate) fn import_failed(host: &mut dyn MagHost, key: &str, error: &ImportError) {
    host.log(
        LogLevel::Warn,
        &format!("The combat stats {key} were not read in whole: {error}"),
    );
}

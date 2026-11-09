//! The combat tracker (Mag-Tools' `CombatTracker`): the chat's attacks,
//! kills and surges per source and target, kept twice: the current
//! session's stats and the persistent ones, which are read from the stats
//! file at the login and written at the logoff (and every 10 minutes).
//!
//! - [`patterns`]: the chat lines, Mag-Tools' patterns and the fixes.
//! - [`info`]: what is kept.
//! - [`xml`]: the stats files.
//!
//! [`Combat`] is what the plugin drives: the patterns (compiled a few a
//! tick), both trackers, the files. A line is parsed once and counted by
//! both trackers (Mag-Tools parsed it once per tracker, to the same
//! result); the "Unable to parse …" lines are still written once per
//! tracker, as Mag-Tools did.
//!
//! Where it differs from Mag-Tools: the fixes F1-F6 ([`patterns`]) and
//! F10: a line the plugin posted itself is not read back ([`PostedLines`]).

pub mod info;
pub mod patterns;
pub mod xml;

use std::collections::{HashMap, VecDeque};

use ac_plugin_abi::{ChatLine, LogLevel};

use self::info::{CombatEvent, CombatInfo, SurgeEvent, SurgeInfo};
use self::patterns::{ParsedLine, Patterns};
use self::xml::{ImportError, StagedImport};
use crate::host::MagHost;

/// The patterns compiled a tick, by their cost ([`patterns::compile_cost`]):
/// the dearest (two Unicode word classes) alone, or about six plain ones.
pub const COMPILE_BUDGET_PER_TICK: usize = 12;

/// Lines that arrived before the patterns were compiled, kept to be
/// read once they are; the ones past this are not read.
pub const WAITING_LINES_MAX: usize = 512;

/// Waiting lines read a tick once the patterns are compiled.
pub const WAITING_LINES_PER_TICK: usize = 25;

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

    /// `GetCombatInfos(name)`: the infos `name` is the source or target of.
    pub fn combat_infos_of<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a CombatInfo> {
        self.combat
            .iter()
            .filter(move |info| info.source == name || info.target == name)
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
                self.combat_index.insert(key, self.combat.len() - 1);
                self.combat.len() - 1
            }
        };
        self.combat[at].add(event);
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
        self.combat.clear();
        self.combat_index.clear();
        self.aetheria.clear();
        self.cloak.clear();
    }

    /// An info read from a file, added at the end as it is.
    pub(crate) fn push_combat_info(&mut self, info: CombatInfo) {
        let key = (info.source.clone(), info.target.clone());
        self.combat.push(info);
        // A file with the same pair twice: the first is the one found.
        self.combat_index
            .entry(key)
            .or_insert(self.combat.len() - 1);
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
/// after the call that posted it; one not heard by the second tick after
/// is forgotten.
#[derive(Debug, Default)]
pub struct PostedLines {
    lines: Vec<(String, u8)>,
}

impl PostedLines {
    /// The plugin posted `text`.
    pub fn note(&mut self, text: String) {
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
        heard: Vec<ParsedLine>,
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
    /// The character's name and world, once known.
    me: Option<(String, String)>,
    persistence: Persistence,
}

impl Combat {
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
                });
            }
            return;
        }
        self.read(host, line);
    }

    fn read(&mut self, host: &mut dyn MagHost, line: &ChatLine) {
        let Some(me) = self.me(host).map(str::to_owned) else {
            return;
        };
        let Some(parsed) = self.patterns.parse(&me, line) else {
            return;
        };
        if parsed.combat.is_none() && parsed.aetheria.is_none() && parsed.cloak.is_none() {
            return;
        }
        // Each tracker writes its own warnings, as Mag-Tools' two did.
        for _tracker in 0..2 {
            for warning in &parsed.warnings {
                let text = format!("{CHAT_PREFIX}{warning}");
                host.chat_post(&text, CHAT_TYPE);
                self.posted.note(text);
            }
        }
        self.current.add_parsed(&me, &parsed);
        match &mut self.persistence {
            Persistence::Importing { heard, .. } => heard.push(parsed),
            _ => self.persistent.add_parsed(&me, &parsed),
        }
    }

    /// One tick: a few patterns compiled, then the lines that waited for
    /// them; the persistent file's import step; its periodic write. True
    /// when it did a tick's worth of work.
    pub fn tick(&mut self, host: &mut dyn MagHost, settings: CombatSettings) -> bool {
        self.posted.tick();
        if !self.patterns.is_ready() {
            self.patterns.compile_some(COMPILE_BUDGET_PER_TICK);
            return true;
        }
        let mut busy = false;
        if !self.waiting.is_empty() {
            for _ in 0..WAITING_LINES_PER_TICK {
                let Some(waiting) = self.waiting.pop_front() else {
                    break;
                };
                let line = ChatLine {
                    display_text: waiting.display_text,
                    kind: waiting.kind,
                    combat_kind: waiting.combat_kind,
                    ..ChatLine::default()
                };
                self.read(host, &line);
            }
            busy = true;
        }
        match &mut self.persistence {
            Persistence::Idle => {}
            Persistence::Importing { import, heard } => {
                busy = true;
                if import.step(host, &mut self.persistent) {
                    let heard = std::mem::take(heard);
                    let me = self
                        .me
                        .as_ref()
                        .map(|(name, _)| name.clone())
                        .unwrap_or_default();
                    for parsed in &heard {
                        self.persistent.add_parsed(&me, parsed);
                    }
                    self.persistence = Persistence::InWorld {
                        saved_ms: host.now_ms(),
                    };
                }
            }
            Persistence::InWorld { saved_ms } => {
                let now = host.now_ms();
                if now - *saved_ms >= SAVE_INTERVAL_MS {
                    *saved_ms = now;
                    if settings.persistent {
                        self.save_persistent(host);
                        busy = true;
                    }
                }
            }
        }
        busy
    }

    /// The login completed: the persistent stats' file is read in (a step
    /// a tick) when they are kept.
    pub fn on_login_complete(&mut self, host: &mut dyn MagHost, settings: CombatSettings) {
        self.me = None;
        let me = self.me(host).is_some();
        self.persistence = match (settings.persistent, me, self.me.clone()) {
            (true, true, Some((name, world))) => Persistence::Importing {
                import: Box::new(StagedImport::new(xml::persistent_key(&world, &name))),
                heard: Vec::new(),
            },
            _ => Persistence::InWorld {
                saved_ms: host.now_ms(),
            },
        };
    }

    /// Logging off: the persistent stats written when they are kept, and
    /// the current ones exported when asked (one call; an import not yet
    /// finished writes nothing over the file).
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
    /// written while they are empty).
    pub fn save_persistent(&mut self, host: &mut dyn MagHost) {
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

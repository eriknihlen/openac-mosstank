//! Optional cumulative kill counters, independent of how long chat and the
//! log are kept (`KillStatistics`).
//!
//! The recorder is off unless `kill-statistics/enabled.txt` in the plugin's
//! storage says `true`. While it is on, every stretch of time one character
//! spends in one world is a segment with its own snapshot file, written
//! once a minute and when the segment ends; a kill is one of this
//! character's killing-blow sentences in the plain combat log.
//!
//! The C# subscribes to the client's chat, logoff and login events; here the
//! shell hands each event to the recorder, and a recorder that is not
//! [`KillStatistics::is_listening`] (off, failed or disposed) ignores them,
//! as an unsubscribed handler would never have been called.

use mosstank_automation::combat_text;
use mosstank_stores::storage::PluginStorage;
use openac_plugin_compat::json::{JsonNumber, JsonValue, system_text_json};
use openac_plugin_compat::text::{casing, ordinal};

use crate::clock::{SessionClock, json_utc};

/// The switch that turns recording on.
pub const ENABLE_KEY: &str = "kill-statistics/enabled.txt";
/// Where the segments' snapshots go, one file each.
pub const SESSIONS_PREFIX: &str = "kill-statistics/sessions/";

/// The plain combat log's text class (`CombatLogTextType.Default`).
const DEFAULT_LOG_TEXT_TYPE: u32 = 0;

/// The character the client has, as the recorder reads it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KillCharacter {
    /// Whether the character is in the world.
    pub in_world: bool,
    /// Its object id.
    pub object_id: u32,
    /// Its name.
    pub name: String,
    /// Its world's name.
    pub world: String,
}

/// What the recorder reaches in the client during a call.
pub trait KillStatisticsHost {
    /// The plugin's own storage.
    fn storage(&mut self) -> &mut dyn PluginStorage;
    /// Whether the client is up at all (`IAutomationSurface.IsAvailable`).
    fn is_available(&mut self) -> bool;
    /// The character (`Automation.Character`).
    fn character(&mut self) -> KillCharacter;
    /// A failure for the session log.
    fn log_error(&mut self, message: &str);
    /// A fresh session id: 32 lowercase hex digits (`Guid.NewGuid():N`).
    fn new_session_id(&mut self) -> String;
}

/// One segment's snapshot, as written (`KillStatistics.Snapshot`).
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// Always 1.
    pub schema_version: i32,
    /// The segment's id.
    pub session_id: String,
    /// The character's name.
    pub character: String,
    /// The world's name.
    pub world: String,
    /// The character's object id.
    pub object_id: u32,
    /// When the segment began, UTC ticks since the Unix epoch.
    pub started_utc: i64,
    /// When it was last written.
    pub updated_utc: i64,
    /// When it ended; `None` while it is open.
    pub ended_utc: Option<i64>,
    /// The segment's length on the monotonic clock, seconds.
    pub covered_seconds: f64,
    /// The kills counted in it.
    pub total_kills: i64,
    /// Whether the segment has ended.
    pub closed: bool,
    /// The last chat line's sequence heard.
    pub last_sequence: u64,
}

impl Snapshot {
    /// The snapshot as `JsonSerializer.Serialize` writes the record:
    /// compact, its properties in declaration order.
    #[must_use]
    pub fn to_json(&self) -> String {
        fn value(value: JsonValue) -> String {
            system_text_json::to_string(&value, &system_text_json::Options::COMPACT)
                .expect("a snapshot holds only finite numbers")
        }
        // A date goes in as written: System.Text.Json's converter for one
        // does not escape the offset's `+` as it does inside a string.
        let date = |ticks: i64| format!("\"{}\"", json_utc(ticks));
        let members = [
            (
                "SchemaVersion",
                value(JsonValue::Number(JsonNumber::I64(i64::from(
                    self.schema_version,
                )))),
            ),
            (
                "SessionId",
                value(JsonValue::String(self.session_id.clone())),
            ),
            (
                "Character",
                value(JsonValue::String(self.character.clone())),
            ),
            ("World", value(JsonValue::String(self.world.clone()))),
            (
                "ObjectId",
                value(JsonValue::Number(JsonNumber::I64(i64::from(
                    self.object_id,
                )))),
            ),
            ("StartedUtc", date(self.started_utc)),
            ("UpdatedUtc", date(self.updated_utc)),
            (
                "EndedUtc",
                self.ended_utc.map_or_else(|| "null".to_owned(), date),
            ),
            (
                "CoveredSeconds",
                value(JsonValue::Number(JsonNumber::F64(self.covered_seconds))),
            ),
            (
                "TotalKills",
                value(JsonValue::Number(JsonNumber::I64(self.total_kills))),
            ),
            ("Closed", value(JsonValue::Bool(self.closed))),
            (
                "LastSequence",
                value(JsonValue::Number(JsonNumber::U64(self.last_sequence))),
            ),
        ];
        let body: Vec<String> = members
            .iter()
            .map(|(name, text)| format!("\"{name}\":{text}"))
            .collect();
        format!("{{{}}}", body.join(","))
    }
}

/// The recorder.
#[derive(Debug, Default)]
pub struct KillStatistics {
    enabled: bool,
    disposed: bool,
    subscribed: bool,
    awaiting_login: bool,
    session: Option<Snapshot>,
    started: u64,
    flushed: u64,
    last_sequence: u64,
}

impl KillStatistics {
    /// Reads the switch and, when it is on, starts listening and opens the
    /// segment the character is in.
    pub fn new(host: &mut dyn KillStatisticsHost, clock: &dyn SessionClock) -> Self {
        let mut recorder = Self::default();
        if host.storage().is_available() {
            match host.storage().read_text(ENABLE_KEY) {
                Ok(text) => {
                    recorder.enabled = text.is_some_and(|text| {
                        ordinal::equals_ignore_case(
                            text.trim_matches(casing::is_white_space),
                            "true",
                        )
                    });
                }
                Err(error) => recorder.fail(host, &error),
            }
        }
        if recorder.enabled {
            recorder.subscribed = true;
            recorder.tick(host, clock);
        }
        recorder
    }

    /// Whether the recorder hears the client's chat, logoff and login.
    #[must_use]
    pub fn is_listening(&self) -> bool {
        self.subscribed
    }

    /// Follows the character, and writes the open segment once a minute.
    pub fn tick(&mut self, host: &mut dyn KillStatisticsHost, clock: &dyn SessionClock) {
        if !self.enabled || self.disposed {
            return;
        }
        self.synchronize_session(host, clock);
        if self.enabled && self.session.is_some() && elapsed_seconds(clock, self.flushed) >= 60.0 {
            self.flush(host, clock, false);
        }
    }

    fn synchronize_session(&mut self, host: &mut dyn KillStatisticsHost, clock: &dyn SessionClock) {
        let available = host.is_available();
        let character = host.character();
        let world_available = available && character.in_world;
        // Logoff precedes teardown. Do not reopen from the still-live
        // character until login is announced or an out-of-world transition
        // was observed.
        if !world_available {
            self.awaiting_login = false;
        }
        let in_world = !self.awaiting_login
            && world_available
            && character.object_id != 0
            && !character.name.is_empty();
        if let Some(session) = &self.session
            && (!in_world
                || session.character != character.name
                || session.world != character.world
                || session.object_id != character.object_id)
        {
            self.flush(host, clock, true);
            self.session = None;
        }
        if !self.enabled || !in_world || self.session.is_some() {
            return;
        }
        let now = clock.utc_now_100ns();
        self.started = clock.monotonic_ms();
        self.session = Some(Snapshot {
            schema_version: 1,
            session_id: host.new_session_id(),
            character: character.name,
            world: character.world,
            object_id: character.object_id,
            started_utc: now,
            updated_utc: now,
            ended_utc: None,
            covered_seconds: 0.0,
            total_kills: 0,
            closed: false,
            last_sequence: self.last_sequence,
        });
        self.flush(host, clock, false);
    }

    /// A chat line the client received (`Chat.Received`).
    pub fn on_chat(
        &mut self,
        host: &mut dyn KillStatisticsHost,
        clock: &dyn SessionClock,
        sequence: u64,
        text: &str,
        log_text_type: u32,
    ) {
        if !self.subscribed || !self.enabled || self.disposed {
            return;
        }
        self.synchronize_session(host, clock);
        if !self.enabled || sequence <= self.last_sequence {
            return;
        }
        self.last_sequence = sequence;
        if let Some(session) = &mut self.session
            && log_text_type == DEFAULT_LOG_TEXT_TYPE
            && combat_text::is_killing_blow(text).is_some()
        {
            session.total_kills += 1;
        }
    }

    /// The character logged off (`Events.Logoff`).
    pub fn on_logoff(&mut self, host: &mut dyn KillStatisticsHost, clock: &dyn SessionClock) {
        if !self.subscribed || !self.enabled || self.disposed {
            return;
        }
        self.awaiting_login = true;
        self.flush(host, clock, true);
        self.session = None;
    }

    /// A login finished (`Events.LoginComplete`).
    pub fn on_login_complete(
        &mut self,
        host: &mut dyn KillStatisticsHost,
        clock: &dyn SessionClock,
    ) {
        if !self.subscribed || !self.enabled || self.disposed {
            return;
        }
        self.flush(host, clock, true);
        self.session = None;
        self.awaiting_login = false;
        self.synchronize_session(host, clock);
    }

    fn flush(&mut self, host: &mut dyn KillStatisticsHost, clock: &dyn SessionClock, closed: bool) {
        if !self.enabled {
            return;
        }
        let Some(session) = &mut self.session else {
            return;
        };
        let now = clock.utc_now_100ns();
        session.updated_utc = now;
        session.ended_utc = closed.then_some(now);
        session.closed = closed;
        session.covered_seconds = elapsed_seconds(clock, self.started);
        session.last_sequence = self.last_sequence;
        let key = format!("{SESSIONS_PREFIX}{}.json", session.session_id);
        let json = session.to_json();
        match host.storage().write_text(&key, &json) {
            Ok(()) => self.flushed = clock.monotonic_ms(),
            Err(error) => self.fail(host, &error),
        }
    }

    fn fail(&mut self, host: &mut dyn KillStatisticsHost, error: &str) {
        self.enabled = false;
        self.subscribed = false;
        host.log_error(&format!(
            "Kill statistics recording stopped because storage failed. \
             The last saved snapshot is incomplete; later time is a measurement gap. {error}"
        ));
    }

    /// Stops listening and closes the open segment, once.
    pub fn dispose(&mut self, host: &mut dyn KillStatisticsHost, clock: &dyn SessionClock) {
        if self.disposed {
            return;
        }
        self.subscribed = false;
        self.flush(host, clock, true);
        self.disposed = true;
    }
}

/// `GetElapsedTime(since).TotalSeconds`.
fn elapsed_seconds(clock: &dyn SessionClock, since: u64) -> f64 {
    let now = clock.monotonic_ms();
    #[allow(clippy::cast_precision_loss)]
    let elapsed = if now >= since {
        (now - since) as f64
    } else {
        -((since - now) as f64)
    };
    elapsed / 1000.0
}

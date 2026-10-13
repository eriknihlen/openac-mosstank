//! The character's quest flags, read from the server's `/myquests` answer
//! (`QuestTracker`). A refresh this tracker asks for itself (at login and
//! `/ub myquests`) hides the quest lines it reads, as UtilityBelt does, and
//! says once when the list is in; a `/myquests` the player types is read
//! the same way but left on screen.

use std::collections::HashMap;

use mosstank_dotnet::compat::regex::{DotNetRegex, RegexOptions};
use mosstank_dotnet::text::casing;

use crate::chat;
use crate::host::MetaHost;

const COMPLETION_SILENCE_SECONDS: f64 = 1.0;
const RETRY_SECONDS: f64 = 15.0;
const MAXIMUM_ATTEMPTS: i32 = 3;

const MY_QUEST_LINE: &str = "(?<key>\\S+) \\- (?<solves>\\d+) solves \\((?<completedOn>\\d{0,11})\\)\"?((?<description>.*)\" (?<maxSolves>.*) (?<repeatTime>\\d{0,11}))?.*$";
/// The kill-task keys, as the reference reads them ([`is_kill_task`]).
#[cfg(test)]
const KILL_TASK_KEY: &str = "(killtask|killcount|slayerquest|totalgolem.*dead|(kills$))";

#[derive(Debug, Clone, Copy)]
struct QuestFlag {
    solves: i32,
    max_solves: i32,
    completed_on_unix_seconds: i64,
    repeat_seconds: i64,
}

/// The quest flags and the refresh under way.
#[derive(Debug)]
pub struct QuestTracker {
    flags: HashMap<String, QuestFlag>,
    chat_sequence: u64,
    identity: String,
    silence_seconds: f64,
    attempts_remaining: i32,
    received_flag: bool,
    hide_output: bool,
    is_refreshing: bool,
    filter: Option<u32>,
    quest_line: DotNetRegex,
}

impl QuestTracker {
    /// A tracker with its chat filter registered.
    ///
    /// # Panics
    /// Never: the two patterns are fixed.
    pub fn new(host: &mut dyn MetaHost) -> Self {
        let filter = host.chat_add_filter();
        Self {
            flags: HashMap::new(),
            chat_sequence: 0,
            identity: String::new(),
            silence_seconds: 0.0,
            attempts_remaining: 0,
            received_flag: false,
            hide_output: false,
            is_refreshing: false,
            filter: Some(filter),
            quest_line: DotNetRegex::new(MY_QUEST_LINE, RegexOptions::CULTURE_INVARIANT)
                .expect("the quest line pattern compiles"),
        }
    }

    /// A refresh is under way.
    #[must_use]
    pub fn is_refreshing(&self) -> bool {
        self.is_refreshing
    }

    /// How many flags are known.
    #[must_use]
    pub fn count(&self) -> usize {
        self.flags.len()
    }

    /// Lets go of the chat filter.
    pub fn dispose(&mut self, host: &mut dyn MetaHost) {
        if let Some(filter) = self.filter.take() {
            host.chat_remove_filter(filter);
        }
    }

    /// The flags belong to one character; another one starts over.
    pub fn bind_identity(&mut self, host: &mut dyn MetaHost, identity: &str) {
        if identity == self.identity {
            return;
        }
        identity.clone_into(&mut self.identity);
        self.flags.clear();
        self.is_refreshing = false;
        self.hide_output = false;
        self.received_flag = false;
        self.silence_seconds = 0.0;
        if !casing::is_white_space_only(identity) {
            self.refresh(host, false);
        }
    }

    /// Asks the server for the quest list. A refresh already under way is
    /// left to finish unless `restart` asks for a new one.
    pub fn refresh(&mut self, host: &mut dyn MetaHost, restart: bool) {
        if self.is_refreshing && !restart {
            return;
        }
        self.flags.clear();
        self.attempts_remaining = MAXIMUM_ATTEMPTS;
        self.received_flag = false;
        self.silence_seconds = 0.0;
        self.is_refreshing = true;
        self.hide_output = true;
        self.submit_request(host);
    }

    /// One tick: the chat read, the refresh's timers.
    pub fn on_tick(&mut self, host: &mut dyn MetaHost, elapsed_seconds: f64) {
        self.capture_chat(host);
        if !self.is_refreshing {
            return;
        }
        self.silence_seconds += elapsed_seconds;
        if self.received_flag && self.silence_seconds > COMPLETION_SILENCE_SECONDS {
            self.finish();
            chat::ub_post(host, &chat::ub_line("Quest data updated."));
            return;
        }
        if !self.received_flag && self.silence_seconds > RETRY_SECONDS {
            self.submit_request(host);
        }
    }

    /// Whether the flag is known (completed at least once).
    #[must_use]
    pub fn has_completed(&self, key: &str) -> bool {
        self.flags.contains_key(&normalize(key))
    }

    /// Whether the quest can be done now: no flag, or its timer ran out
    /// and it is not a once-only flag.
    #[must_use]
    pub fn is_ready(&self, key: &str, now_unix_ms: i64) -> bool {
        let key = normalize(key);
        let Some(flag) = self.flags.get(&key) else {
            return true;
        };
        let next_ms = flag
            .completed_on_unix_seconds
            .saturating_add(flag.repeat_seconds)
            .saturating_mul(1000);
        if next_ms > now_unix_ms {
            return false;
        }
        !self.is_once(&key, flag)
    }

    /// A flag done once and never again: one solve of one allowed. A kill
    /// task's flag is never that, whatever its counts say.
    fn is_once(&self, key: &str, flag: &QuestFlag) -> bool {
        !(is_kill_task(key) && flag.max_solves >= 0) && flag.max_solves == 1 && flag.solves <= 1
    }

    /// The flag's solves.
    #[must_use]
    pub fn progress(&self, key: &str) -> i32 {
        self.flags
            .get(&normalize(key))
            .map_or(0, |flag| flag.solves)
    }

    /// The flag's allowed solves.
    #[must_use]
    pub fn required(&self, key: &str) -> i32 {
        self.flags
            .get(&normalize(key))
            .map_or(0, |flag| flag.max_solves)
    }

    fn capture_chat(&mut self, host: &mut dyn MetaHost) {
        let mut lines = host.chat_captured(self.chat_sequence);
        lines.sort_by_key(|line| line.sequence);
        for line in lines {
            self.chat_sequence = self.chat_sequence.max(line.sequence);
            let text = line.text.trim();
            if text == "Quest list is empty."
                || text == "The command \"myquests\" is not currently enabled on this server."
            {
                self.finish();
                continue;
            }
            self.try_record(text);
        }
    }

    /// The chat filter: while a refresh of our own is under way, a quest line
    /// is read here and kept off the screen. True drops the line.
    pub fn filter(&mut self, text: &str) -> bool {
        self.hide_output && self.is_refreshing && self.try_record(text.trim())
    }

    fn try_record(&mut self, text: &str) -> bool {
        let Ok(Some(found)) = self.quest_line.find(text) else {
            return false;
        };
        let (Some(solves), Some(completed)) = (
            mosstank_dotnet::try_parse_int(found.group("solves").value),
            mosstank_dotnet::try_parse_long(found.group("completedOn").value),
        ) else {
            return false;
        };
        let maximum = mosstank_dotnet::try_parse_int(found.group("maxSolves").value).unwrap_or(0);
        let repeat = mosstank_dotnet::try_parse_long(found.group("repeatTime").value).unwrap_or(0);
        let key = normalize(found.group("key").value);
        self.flags.insert(
            key,
            QuestFlag {
                solves,
                max_solves: maximum,
                completed_on_unix_seconds: completed.max(0),
                repeat_seconds: repeat.max(0),
            },
        );
        self.received_flag = true;
        self.silence_seconds = 0.0;
        true
    }

    fn finish(&mut self) {
        self.is_refreshing = false;
        self.hide_output = false;
    }

    fn submit_request(&mut self, host: &mut dyn MetaHost) {
        if self.attempts_remaining <= 0 {
            self.finish();
            return;
        }
        self.attempts_remaining -= 1;
        self.silence_seconds = 0.0;
        host.chat_submit("/myquests");
    }
}

fn normalize(key: &str) -> String {
    casing::to_lower_invariant(key.trim())
}

/// Whether a flag's key names a kill task: [`KILL_TASK_KEY`] (case kept,
/// `.` short of a line break, `$` at the end or before a final line break),
/// read by hand. Building the pattern costs the plugin's start some tens of
/// millions of instructions; this answers the same.
fn is_kill_task(key: &str) -> bool {
    if key.contains("killtask") || key.contains("killcount") || key.contains("slayerquest") {
        return true;
    }
    const GOLEM: &str = "totalgolem";
    let mut from = 0;
    while let Some(at) = key[from..].find(GOLEM) {
        let start = from + at + GOLEM.len();
        let line_end = key[start..].find('\n').map_or(key.len(), |n| start + n);
        if key[start..line_end].contains("dead") {
            return true;
        }
        from += at + 1;
    }
    key.ends_with("kills") || key.ends_with("kills\n")
}

#[cfg(test)]
mod kill_task_tests {
    use super::*;

    // The hand reading answers as the pattern does.
    #[test]
    fn a_kill_task_key_is_read_as_the_pattern_reads_it() {
        let pattern = DotNetRegex::new(KILL_TASK_KEY, RegexOptions::CULTURE_INVARIANT).unwrap();
        let keys = [
            "",
            "killtask",
            "mykilltaskfoo",
            "KillTask",
            "killcount_x",
            "slayerquest",
            "totalgolemdead",
            "totalgolem_sand_dead_count",
            "totalgolem\ndead",
            "totalgolem\ntotalgolemdead",
            "deadtotalgolem",
            "totalgole",
            "kills",
            "ratkills",
            "ratkills\n",
            "ratkills\n\n",
            "ratkillsx",
            "kill",
            "Kills",
            "a\nkills",
            "kills\nx",
            "pathwardenfound1111",
            "ü-totalgolem-ü-dead",
        ];
        for key in keys {
            assert_eq!(is_kill_task(key), pattern.is_match(key).unwrap(), "{key:?}");
        }
    }
}

//! The `/ub` lines the commands ran lately ([`UbLineLog`]), so that whoever
//! submitted one to the client (a route's chat waypoint) can tell when that
//! line, and not just any `/ub` line, has been run, and when the line a
//! `/ub delay` or `/ub bc` put off has been run in its turn.

use std::collections::VecDeque;

use mosstank_dotnet::text::ordinal;

/// The verb the client hands the `/ub` lines back under.
const UB_VERB: &str = "ub";

/// The words that end a tool's run when they follow its command.
const STOP_WORDS: [&str; 4] = ["stop", "cancel", "quit", "abort"];

/// How many entries are kept: far more than a pass runs.
const KEPT: usize = 256;

/// What became of a submitted line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UbLineState {
    /// Not run yet.
    NotRun,
    /// Run, but the line it put off (`/ub delay`, `/ub bc`) has not run
    /// yet.
    PutOff,
    /// Run, and whatever it put off as well.
    Run,
}

/// One entry of the log.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    /// A `/ub` line was run.
    Ran {
        sequence: u64,
        arguments: String,
        /// It put a line off to run later.
        put_off: bool,
    },
    /// The line put off by the run `from` went to the client; a `/ub` line
    /// comes back to be run after `after`.
    Released {
        from: u64,
        after: u64,
        ub_arguments: Option<String>,
    },
}

/// The `/ub` lines run lately, oldest first.
#[derive(Debug, Default)]
pub struct UbLineLog {
    last: u64,
    entries: VecDeque<Entry>,
    /// The run under way, while a line is being run.
    running: Option<u64>,
}

/// The arguments of a `/ub` line, when `text` is one.
#[must_use]
pub fn ub_arguments(text: &str) -> Option<&str> {
    ac_plugin_abi::verbs::split_command_line(text)
        .filter(|(_, verb, _)| verb.eq_ignore_ascii_case(UB_VERB))
        .map(|(_, _, arguments)| arguments)
}

/// Whether the `/ub` line's arguments only stop a tool: a command and a
/// stop word, as `give stop`, `ig cancel` or `autopack stop`.
#[must_use]
pub fn is_stop_line(arguments: &str) -> bool {
    let mut words = arguments.split_whitespace();
    matches!(
        (words.next(), words.next(), words.next()),
        (Some(_), Some(word), None)
            if STOP_WORDS.iter().any(|stop| ordinal::equals_ignore_case(word, stop))
    )
}

impl UbLineLog {
    /// The number of the last line run (0 before the first).
    #[must_use]
    pub fn last_sequence(&self) -> u64 {
        self.last
    }

    /// What became of a line with these arguments submitted when the last
    /// line run was `since`: the first such line run after it is taken as
    /// the one submitted.
    #[must_use]
    pub fn state(&self, since: u64, arguments: &str) -> UbLineState {
        let Some(mut run) = self.find_run(since, arguments.trim()) else {
            return UbLineState::NotRun;
        };
        loop {
            let Entry::Ran {
                sequence, put_off, ..
            } = run
            else {
                return UbLineState::Run;
            };
            if !put_off {
                return UbLineState::Run;
            }
            let released = self.entries.iter().find_map(|entry| match entry {
                Entry::Released {
                    from,
                    after,
                    ub_arguments,
                } if from == sequence => Some((*after, ub_arguments.as_deref())),
                _ => None,
            });
            match released {
                None => return UbLineState::PutOff,
                Some((_, None)) => return UbLineState::Run,
                Some((after, Some(next))) => match self.find_run(after, next) {
                    None => return UbLineState::PutOff,
                    Some(next_run) => run = next_run,
                },
            }
        }
    }

    fn find_run(&self, since: u64, arguments: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| {
            matches!(entry, Entry::Ran { sequence, arguments: ran, .. }
                if *sequence > since && ran == arguments)
        })
    }

    /// A line begins to run.
    pub(crate) fn begin(&mut self, arguments: &str) {
        self.last = self.last.wrapping_add(1);
        self.running = Some(self.last);
        self.push(Entry::Ran {
            sequence: self.last,
            arguments: arguments.trim().to_owned(),
            put_off: false,
        });
    }

    /// The line run ended.
    pub(crate) fn end(&mut self) {
        self.running = None;
    }

    /// The line being run puts a line off: the run it came from, if any.
    pub(crate) fn put_off(&mut self) -> Option<u64> {
        let running = self.running?;
        for entry in &mut self.entries {
            if let Entry::Ran {
                sequence, put_off, ..
            } = entry
                && *sequence == running
            {
                *put_off = true;
            }
        }
        Some(running)
    }

    /// A line put off by the run `from` goes to the client now.
    pub(crate) fn released(&mut self, from: u64, line: &str) {
        self.push(Entry::Released {
            from,
            after: self.last,
            ub_arguments: ub_arguments(line).map(|arguments| arguments.trim().to_owned()),
        });
    }

    fn push(&mut self, entry: Entry) {
        if self.entries.len() == KEPT {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_run_only_once_a_line_with_its_own_words_runs() {
        let mut log = UbLineLog::default();
        log.begin("listvars");
        log.end();
        assert_eq!(log.state(0, "give Taper to Mule"), UbLineState::NotRun);
        log.begin("give Taper to Mule");
        log.end();
        assert_eq!(log.state(0, "give Taper to Mule"), UbLineState::Run);
        // One run before the submit is not the submitted line's.
        assert_eq!(log.state(2, "give Taper to Mule"), UbLineState::NotRun);
    }

    #[test]
    fn a_delayed_ub_line_is_run_when_the_line_it_put_off_has_run() {
        let mut log = UbLineLog::default();
        log.begin("delay 500 /ub give Taper to Mule");
        let from = log.put_off().unwrap();
        log.end();
        let line = "delay 500 /ub give Taper to Mule";
        assert_eq!(log.state(0, line), UbLineState::PutOff);
        log.released(from, "/ub give Taper to Mule");
        assert_eq!(log.state(0, line), UbLineState::PutOff);
        log.begin("give Taper to Mule");
        log.end();
        assert_eq!(log.state(0, line), UbLineState::Run);
    }

    #[test]
    fn a_delayed_line_for_someone_else_is_run_once_it_went_to_the_client() {
        let mut log = UbLineLog::default();
        log.begin("delay 100 /say hi");
        let from = log.put_off().unwrap();
        log.end();
        log.released(from, "/say hi");
        assert_eq!(log.state(0, "delay 100 /say hi"), UbLineState::Run);
    }

    #[test]
    fn a_stop_line_is_a_command_and_a_stop_word() {
        assert!(is_stop_line("give stop"));
        assert!(is_stop_line("ig Cancel"));
        assert!(is_stop_line("autopack quit"));
        assert!(!is_stop_line("give quit to Mule"));
        assert!(!is_stop_line("quit"));
        assert_eq!(ub_arguments("/ub give stop"), Some("give stop"));
        assert_eq!(ub_arguments("/say give stop"), None);
    }
}

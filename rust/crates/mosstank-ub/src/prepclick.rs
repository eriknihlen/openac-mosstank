//! `prepclick {stop|yes <secondstowatch>|no <secondstowatch>}`
//! (`PrepClickController`): answers the first yes/no confirmation the
//! server raises within a window, once, then lets go. It listens only while
//! armed, so a confirmation raised before the command is never its business.
//! Every line repeats the reference's wording, tag included
//! (`[UB] PrepClick: Will click yes ...`), since a macro may wait on it.

use std::sync::LazyLock;

use ac_plugin_abi::world::Confirmation;
use openac_plugin_compat::number::{self, NumberStyles};
use openac_plugin_compat::regex::{DotNetRegex, RegexOptions};
use openac_plugin_compat::text::casing;

use crate::chat::{TAG, tools};
use crate::host::UbHost;

/// The command's usage line.
pub const USAGE: &str = "/ub prepclick {stop|yes <secondstowatch>|no <secondstowatch>}";

/// The longest window the command will watch, in seconds.
const MAXIMUM_SECONDS: f64 = 3600.0;

static COMMAND: LazyLock<DotNetRegex> = LazyLock::new(|| {
    DotNetRegex::new(
        r"^(?<choice>yes|no|stop)( (?<seconds>\d+))?$",
        RegexOptions::IGNORE_CASE | RegexOptions::CULTURE_INVARIANT,
    )
    .expect("a fixed pattern the kit reproduces")
});

/// The watch.
#[derive(Debug, Default)]
pub struct PrepClickController {
    armed: bool,
    answer_yes: bool,
    seconds: f64,
    remaining: f64,
}

/// `double.ToString(InvariantCulture)`.
fn text(value: f64) -> String {
    number::to_string_f64(value)
}

impl PrepClickController {
    /// Not watching.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a confirmation would be answered now.
    #[must_use]
    pub fn is_armed(&self) -> bool {
        self.armed
    }

    /// Runs one `prepclick` line; `write` prints a line (the transcript's
    /// writer: a tagged line is a UB line, anything else the macro's own).
    pub fn command(&mut self, arguments: &str, write: &mut dyn FnMut(&str)) {
        let input = arguments.trim();
        let found = COMMAND.find(input).ok().flatten();
        let Some(found) = found else {
            write("Bad command syntax");
            write(&format!("Usage: {USAGE}"));
            return;
        };
        let choice = casing::to_lower_invariant(found.group("choice").value);
        if choice == "stop" {
            if !self.armed {
                say(write, "Message boxes are not currently being watched");
                return;
            }
            let passed = number::round_digits(
                self.seconds - self.remaining,
                2,
                number::MidpointRounding::ToEven,
            );
            say(
                write,
                &format!(
                    "Stopping... {}s passed out of expected {}s",
                    text(passed),
                    text(self.seconds)
                ),
            );
            self.disarm();
            return;
        }

        // The window is optional in the grammar, and a missing one reads as
        // zero: armed, and over at the next tick.
        let seconds_group = found.group("seconds");
        let seconds = if seconds_group.success {
            number::parse_f64(seconds_group.value, NumberStyles::NONE).unwrap_or(f64::INFINITY)
        } else {
            0.0
        };
        if seconds > MAXIMUM_SECONDS {
            say(
                write,
                &format!(
                    "{} is not a valid number of seconds to wait",
                    seconds_group.value
                ),
            );
            return;
        }

        self.seconds = seconds;
        self.remaining = seconds;
        self.answer_yes = choice == "yes";
        self.armed = true;
        say(
            write,
            &format!(
                "Will click {choice} on the next dialog to appear within {} seconds",
                text(seconds)
            ),
        );
    }

    /// Counts the window down; at its end the watch is dropped.
    pub fn on_tick(&mut self, elapsed_seconds: f64, write: &mut dyn FnMut(&str)) {
        if !self.armed {
            return;
        }
        self.remaining -= elapsed_seconds.max(0.0);
        if self.remaining >= 0.0 {
            return;
        }
        say(write, &format!("Time has expired: {}", text(self.seconds)));
        self.disarm();
    }

    /// The server asks a yes/no question: answered once inside a window
    /// that has length, and the watch is dropped whichever way it went.
    pub fn on_confirmation(
        &mut self,
        host: &mut dyn UbHost,
        confirmation: &Confirmation,
        write: &mut dyn FnMut(&str),
    ) {
        if !self.armed {
            return;
        }
        let answer = self.seconds > 0.0;
        let accept = self.answer_yes;
        self.disarm();
        if !answer {
            return;
        }
        say(
            write,
            &format!(
                "{}{}",
                if accept {
                    "Click Yes on "
                } else {
                    "Click No on "
                },
                confirmation.text
            ),
        );
        host.answer_dialog(confirmation.context_id, accept);
    }

    /// Drops the watch without a word: the session it was armed in is gone.
    pub fn reset(&mut self) {
        self.disarm();
    }

    fn disarm(&mut self) {
        self.armed = false;
        self.seconds = 0.0;
        self.remaining = 0.0;
        self.answer_yes = false;
    }
}

/// One of the tool's own lines, under the tool's tag.
fn say(write: &mut dyn FnMut(&str), text: &str) {
    write(&format!("{TAG}{}: {text}", tools::PREP_CLICK));
}

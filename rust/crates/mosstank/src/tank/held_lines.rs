//! The meta's chat lines held while the character can do nothing in the
//! world (register MACRO-1): dead, or in portal space.

use mosstank_meta::scheduler::MacroLogChannel;

use super::Core;

/// How many different lines are held at most. A death route says a handful;
/// more are a rule firing again and again through a long hold (a death
/// teleport that never comes holds for good), and sending dozens in the one
/// frame the hold ends would flood the server's chat.
pub(crate) const HELD_META_LINES_LIMIT: usize = 16;

/// The meta's held lines and what was said about them.
#[derive(Debug, Default)]
pub(crate) struct HeldMetaLines {
    lines: Vec<String>,
    /// The first line of this hold was logged.
    began_said: bool,
    /// A line over the limit was dropped and said.
    overflow_said: bool,
    /// A meta was loaded during the call under way: what the old meta
    /// still says in it is dropped with what it held.
    closed_for_call: bool,
}

impl HeldMetaLines {
    pub(crate) fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The lines to send now; the hold is over.
    pub(crate) fn take(&mut self) -> Vec<String> {
        self.began_said = false;
        self.overflow_said = false;
        std::mem::take(&mut self.lines)
    }

    /// A call into the meta begins.
    pub(crate) fn open_call(&mut self) {
        self.closed_for_call = false;
    }
}

impl Core {
    /// Whether the world is held for the meta: the character can do nothing
    /// in it while the macro runs (with the macro off the meta does not run,
    /// and what the player runs goes at once).
    pub(crate) fn meta_world_held(&self) -> Option<&'static str> {
        self.world_hold.filter(|_| self.combat.enabled())
    }

    /// Holds a meta line while the world is held: true when it was taken
    /// (held, or dropped as a copy, over the limit, or the old meta's).
    pub(crate) fn hold_meta_line(&mut self, line: &str) -> bool {
        let Some(why) = self.meta_world_held() else {
            return false;
        };
        let held = &mut self.held_meta_lines;
        if held.closed_for_call || held.lines.iter().any(|kept| kept == line) {
            return true;
        }
        if held.lines.len() >= HELD_META_LINES_LIMIT {
            if !held.overflow_said {
                held.overflow_said = true;
                self.queue_log(
                    MacroLogChannel::RuleInfo,
                    &format!(
                        "(meta) more than {HELD_META_LINES_LIMIT} chat lines held while {why}; \
                         the rest are dropped"
                    ),
                );
            }
            return true;
        }
        held.lines.push(line.to_owned());
        if !held.began_said {
            held.began_said = true;
            self.queue_log(
                MacroLogChannel::RuleInfo,
                &format!("(meta) chat lines held while {why}"),
            );
        }
        true
    }

    /// Drops the held lines, saying so once when there were any.
    pub(crate) fn drop_held_meta_lines(&mut self, why: &str) {
        let count = self.held_meta_lines.take().len();
        if count > 0 {
            self.queue_log(
                MacroLogChannel::RuleInfo,
                &format!("(meta) {count} held chat lines dropped: {why}"),
            );
        }
    }

    /// Another meta was loaded: the old one's held lines go, and so does
    /// what it still says in the call under way.
    pub(crate) fn meta_replaced(&mut self) {
        self.drop_held_meta_lines("another meta was loaded");
        self.held_meta_lines.closed_for_call = true;
    }
}

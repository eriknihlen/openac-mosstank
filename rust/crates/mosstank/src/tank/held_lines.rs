//! The meta's chat lines held while the character can do nothing in the
//! world (register MACRO-1): dead, or in portal space.

use mosstank_meta::scheduler::MacroLogChannel;

use super::Core;

/// How many different lines are held at most. A death route says a handful;
/// more are a rule firing again and again through a long hold (a death
/// teleport that never comes holds for good), and sending dozens in the one
/// frame the hold ends would flood the server's chat.
pub(crate) const HELD_META_LINES_LIMIT: usize = 16;

/// How long a held line stays worth sending, seconds, counted only while
/// the hold could have ended (the teleport over, out of portal space) and
/// still holds, as when the health has not come back: one older is dropped,
/// as the world it was said for has moved on. A line said before the hold
/// could end is never stale on release, however long a death and its
/// teleport took. Not a game value: our own margin.
pub(crate) const HELD_META_LINE_MAX_AGE_SECONDS: f64 = 30.0;

/// How many held lines of one hold are logged one by one; the rest of the
/// hold's are counted when it ends.
const HELD_META_LINES_LOGGED: usize = 4;

/// One held line and when it was last said.
#[derive(Debug)]
struct HeldLine {
    text: String,
    /// The age clock when the line was last said.
    held_at: f64,
}

/// The held lines a hold's end hands out.
#[derive(Debug, Default)]
pub(crate) struct HeldRelease {
    /// The lines still worth sending, in the order said.
    pub(crate) fresh: Vec<String>,
    /// How many were dropped as held too long, this hold.
    pub(crate) stale: usize,
    /// The first of those.
    pub(crate) first_stale: Option<String>,
}

/// The meta's held lines and what was said about them.
#[derive(Debug, Default)]
pub(crate) struct HeldMetaLines {
    lines: Vec<HeldLine>,
    /// The lines' age: seconds of frames in which the hold could have
    /// ended but still held.
    clock: f64,
    /// The lines of this hold logged one by one.
    logged: usize,
    /// More lines than those logged were held, and that was said.
    more_said: bool,
    /// A line over the limit was dropped and said.
    overflow_said: bool,
    /// The lines of this hold already dropped as held too long.
    stale: usize,
    first_stale: Option<String>,
    /// A meta was loaded during the call under way: what the old meta
    /// still says in it is dropped with what it held.
    closed_for_call: bool,
}

impl HeldMetaLines {
    pub(crate) fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.stale == 0
    }

    /// A frame of `elapsed_seconds` went by; `could_end` when the hold
    /// could have ended in it (the teleport over, out of portal space).
    pub(crate) fn advance(&mut self, elapsed_seconds: f64, could_end: bool) {
        if could_end && elapsed_seconds.is_finite() && elapsed_seconds > 0.0 {
            self.clock += elapsed_seconds;
        }
    }

    /// Drops the lines held too long, keeping count of them.
    fn prune_stale(&mut self) {
        let clock = self.clock;
        let (fresh, stale): (Vec<HeldLine>, Vec<HeldLine>) = std::mem::take(&mut self.lines)
            .into_iter()
            .partition(|line| clock - line.held_at <= HELD_META_LINE_MAX_AGE_SECONDS);
        self.lines = fresh;
        if let Some(first) = stale.first()
            && self.first_stale.is_none()
        {
            self.first_stale = Some(first.text.clone());
        }
        self.stale += stale.len();
    }

    /// Every held line, the hold being over.
    pub(crate) fn take(&mut self) -> HeldRelease {
        self.prune_stale();
        self.logged = 0;
        self.more_said = false;
        self.overflow_said = false;
        HeldRelease {
            fresh: std::mem::take(&mut self.lines)
                .into_iter()
                .map(|line| line.text)
                .collect(),
            stale: std::mem::take(&mut self.stale),
            first_stale: self.first_stale.take(),
        }
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
        if held.closed_for_call {
            return true;
        }
        let clock = held.clock;
        if let Some(kept) = held.lines.iter_mut().find(|kept| kept.text == line) {
            // Said again: as fresh as now, in the place it was first said.
            kept.held_at = clock;
            return true;
        }
        // Lines held too long make room before the limit is reached.
        held.prune_stale();
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
        held.lines.push(HeldLine {
            text: line.to_owned(),
            held_at: clock,
        });
        if held.logged < HELD_META_LINES_LOGGED {
            held.logged += 1;
            self.queue_log(
                MacroLogChannel::RuleInfo,
                &format!("(meta) chat line held while {why}: {line}"),
            );
        } else if !held.more_said {
            held.more_said = true;
            self.queue_log(
                MacroLogChannel::RuleInfo,
                &format!(
                    "(meta) more chat lines held while {why}; they are counted when the hold ends"
                ),
            );
        }
        true
    }

    /// The hold is over: the lines still worth sending, in order. Those held
    /// longer than [`HELD_META_LINE_MAX_AGE_SECONDS`] are dropped, and both
    /// are said in one line each.
    pub(crate) fn take_held_meta_lines(&mut self) -> Vec<String> {
        let release = self.held_meta_lines.take();
        if let Some(first) = &release.first_stale {
            self.queue_log(
                MacroLogChannel::RuleInfo,
                &format!(
                    "(meta) {} held chat lines dropped: held over {HELD_META_LINE_MAX_AGE_SECONDS} s \
                     (first: {first})",
                    release.stale
                ),
            );
        }
        if !release.fresh.is_empty() {
            self.queue_log(
                MacroLogChannel::RuleInfo,
                &format!("(meta) {} held chat lines sent", release.fresh.len()),
            );
        }
        release.fresh
    }

    /// Drops the held lines, saying so once when there were any.
    pub(crate) fn drop_held_meta_lines(&mut self, why: &str) {
        let release = self.held_meta_lines.take();
        let count = release.fresh.len() + release.stale;
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

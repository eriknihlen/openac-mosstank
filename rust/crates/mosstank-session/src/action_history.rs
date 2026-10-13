//! The macro's action history (`ActionHistory`): a bounded session history
//! whose entries keep their positions across trimming and clearing.

use std::collections::VecDeque;
use std::rc::Rc;

use crate::clock::{SessionClock, utc};

/// The history's length.
pub const CAPACITY: usize = 1000;

/// The history: each change of what the macro is doing, stamped with the
/// local time.
#[derive(Debug)]
pub struct ActionHistory {
    entries: VecDeque<String>,
    last_status: Option<String>,
    lines: Rc<[String]>,
    first_index: usize,
}

impl Default for ActionHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl ActionHistory {
    /// An empty history.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            last_status: None,
            lines: Rc::from(Vec::new()),
            first_index: 0,
        }
    }

    /// The lines, oldest first. The same lines are handed out until the
    /// next change; reading never rebuilds them.
    #[must_use]
    pub fn lines(&self) -> Rc<[String]> {
        Rc::clone(&self.lines)
    }

    /// The absolute position of the first line: how many lines have been
    /// trimmed or cleared before it.
    #[must_use]
    pub fn first_index(&self) -> usize {
        self.first_index
    }

    /// Records `status` unless it is the status recorded last.
    pub fn record(&mut self, status: &str, clock: &dyn SessionClock) {
        if self.last_status.as_deref() == Some(status) {
            return;
        }
        self.last_status = Some(status.to_owned());
        let local = utc(clock.utc_now_100ns())
            .add_ticks(i64::from(clock.local_offset_seconds()) * 10_000_000);
        let timestamp = local.format("HH:mm:ss").expect("a pattern the kit writes");
        self.entries.push_back(format!("[{timestamp}] {status}"));
        if self.entries.len() > CAPACITY {
            self.entries.pop_front();
            self.first_index += 1;
        }
        self.lines = self.entries.iter().cloned().collect();
    }

    /// Empties the history. Clearing does not create a new action: the next
    /// genuine change starts the new history.
    pub fn clear(&mut self) {
        self.first_index += self.entries.len();
        self.entries.clear();
        self.lines = Rc::from(Vec::new());
    }
}

//! The macro's debug log (`MacroDebugLog`): the newest complete UTF-8
//! records within 2 MiB, written out in one piece when flushed.

use std::collections::VecDeque;

use mosstank_dotnet::compat::datetime::DateTime;
use mosstank_stores::storage::{PluginStorage, StorageError};

/// The most bytes the log keeps.
pub const MAXIMUM_BYTES: usize = 2 * 1024 * 1024;

/// The log.
#[derive(Debug, Clone)]
pub struct MacroDebugLog {
    key: String,
    lines: VecDeque<String>,
    bytes: usize,
    dirty: bool,
}

fn stamp(unix_ms: i64) -> String {
    // DateTimeOffset.UtcNow in the round-trip form: +00:00, not Z.
    DateTime::local_from_unix_millis(unix_ms, 0)
        .format("O")
        .unwrap_or_default()
}

impl MacroDebugLog {
    /// A log written under `key`.
    #[must_use]
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_owned(),
            lines: VecDeque::new(),
            bytes: 0,
            dirty: false,
        }
    }

    /// Where it is written.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Records one line at `unix_ms`; line breaks in it become spaces. A
    /// record larger than the whole log is replaced by a note.
    pub fn record(&mut self, unix_ms: i64, message: &str) {
        let mut line = format!(
            "[{}] {}\n",
            stamp(unix_ms),
            message.replace(['\r', '\n'], " ")
        );
        if line.len() > MAXIMUM_BYTES {
            line = format!("[{}] Record omitted: exceeds 2 MiB.\n", stamp(unix_ms));
        }
        self.bytes += line.len();
        self.lines.push_back(line);
        while self.bytes > MAXIMUM_BYTES {
            let Some(oldest) = self.lines.pop_front() else {
                break;
            };
            self.bytes -= oldest.len();
        }
        self.dirty = true;
    }

    /// Writes the log when something was recorded since the last write.
    ///
    /// # Errors
    /// The storage cannot be written.
    pub fn flush(&mut self, storage: &mut dyn PluginStorage) -> Result<(), StorageError> {
        if !self.dirty {
            return Ok(());
        }
        let text: String = self.lines.iter().map(String::as_str).collect();
        storage.write_text(&self.key, &text)?;
        self.dirty = false;
        Ok(())
    }
}

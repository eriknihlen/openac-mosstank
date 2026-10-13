//! The session crate's test doubles: a clock moved by hand and a storage
//! that counts writes and fails on demand.
#![allow(dead_code)]

use std::cell::Cell;
use std::collections::BTreeMap;

use mosstank_session::SessionClock;
use mosstank_stores::storage::{PluginStorage, StorageError};

/// A clock: the monotonic clock in whole seconds, and the wall clock the
/// Unix epoch plus those seconds plus a wall-clock change.
#[derive(Debug, Default)]
pub struct Clock {
    seconds: Cell<i64>,
    /// A change of the wall clock, seconds.
    pub utc_offset: Cell<i64>,
    /// Seconds the local time is ahead of UTC.
    pub local_offset: Cell<i32>,
}

impl Clock {
    /// At a wall time, Unix seconds.
    #[must_use]
    pub fn at(unix_seconds: i64) -> Self {
        let clock = Self::default();
        clock.utc_offset.set(unix_seconds);
        clock
    }

    pub fn advance(&self, seconds: i64) {
        self.seconds.set(self.seconds.get() + seconds);
    }

    /// The wall time, Unix seconds.
    #[must_use]
    pub fn utc_seconds(&self) -> i64 {
        self.seconds.get() + self.utc_offset.get()
    }
}

impl SessionClock for Clock {
    fn utc_now_100ns(&self) -> i64 {
        self.utc_seconds() * 10_000_000
    }

    fn local_offset_seconds(&self) -> i32 {
        self.local_offset.get()
    }

    fn monotonic_ms(&self) -> u64 {
        u64::try_from(self.seconds.get() * 1000).unwrap()
    }
}

/// Storage in memory, keyed exactly, counting writes.
#[derive(Debug, Default)]
pub struct Store {
    pub values: BTreeMap<String, String>,
    pub writes: usize,
    /// Every write fails with this message.
    pub fail: Option<String>,
    /// Every read fails.
    pub fail_read: bool,
    /// Report the storage as not there.
    pub unavailable: bool,
}

impl PluginStorage for Store {
    fn is_available(&mut self) -> bool {
        !self.unavailable
    }

    fn root_path(&mut self) -> Option<String> {
        None
    }

    fn read_text(&mut self, key: &str) -> Result<Option<String>, StorageError> {
        if self.fail_read {
            return Err("Test read failure".to_owned());
        }
        Ok(self.values.get(key).cloned())
    }

    fn list(&mut self, _prefix: &str) -> Result<Vec<String>, StorageError> {
        Ok(Vec::new())
    }

    fn write_text(&mut self, key: &str, content: &str) -> Result<(), StorageError> {
        self.writes += 1;
        if let Some(message) = &self.fail {
            return Err(message.clone());
        }
        self.values.insert(key.to_owned(), content.to_owned());
        Ok(())
    }

    fn ensure_directory(&mut self, _prefix: &str) -> Result<bool, StorageError> {
        Ok(true)
    }

    fn delete(&mut self, key: &str) -> Result<bool, StorageError> {
        Ok(self.values.remove(key).is_some())
    }
}

//! Where the stores keep their files: a root of `/`-separated keys, the
//! plugin's own data or the shared VTank profile folder (the C#'s
//! `IPluginStorage`). The plugin hands a store the storage each call needs;
//! a store never holds one, because the client is reached only during a
//! plugin call.

use std::collections::BTreeMap;

use openac_plugin_compat::text::{casing, ordinal};

/// Why a storage call failed: the message the C#'s exception carried.
pub type StorageError = String;

/// One root of keyed text files (`IPluginStorage`).
pub trait PluginStorage {
    /// Whether this storage can read and write at all; false for the inert
    /// storage of a client with nowhere to write.
    fn is_available(&mut self) -> bool;

    /// Where the root is on disk, to show the player; `None` when the
    /// storage is not backed by files.
    fn root_path(&mut self) -> Option<String>;

    /// The text under `key`, `None` when nothing is stored there.
    ///
    /// # Errors
    /// The file exists but cannot be read.
    fn read_text(&mut self, key: &str) -> Result<Option<String>, StorageError>;

    /// The keys of the files beneath `prefix` (`""`: all), sorted ignoring
    /// case.
    ///
    /// # Errors
    /// The folder cannot be listed.
    fn list(&mut self, prefix: &str) -> Result<Vec<String>, StorageError>;

    /// Replaces what is under `key` with `content`.
    ///
    /// # Errors
    /// The storage is unavailable, or the file cannot be written.
    fn write_text(&mut self, key: &str, content: &str) -> Result<(), StorageError>;

    /// Creates the folder `prefix` names, with its parents; whether it
    /// exists afterwards (false when the storage is unavailable).
    ///
    /// # Errors
    /// The prefix is absolute or climbs out, or the folder cannot be made.
    fn ensure_directory(&mut self, prefix: &str) -> Result<bool, StorageError>;

    /// Removes what is under `key`; false when nothing was.
    ///
    /// # Errors
    /// The file cannot be removed.
    fn delete(&mut self, key: &str) -> Result<bool, StorageError>;

    /// Takes a lock other clients on this computer honour, named by `key`:
    /// its handle.
    ///
    /// # Errors
    /// The storage cannot lock, or another client holds the lock.
    fn lock(&mut self, key: &str) -> Result<u32, StorageError> {
        let _ = key;
        Err("Plugin storage is unavailable.".to_owned())
    }

    /// Releases a lock; false for an unknown handle.
    fn unlock(&mut self, handle: u32) -> bool {
        let _ = handle;
        false
    }
}

/// The message .NET's inert storage throws on a write.
pub const UNAVAILABLE: &str = "Plugin storage is unavailable.";

/// Storage that is not there (`NoOpPluginStorage`): nothing reads, writes
/// are refused.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoStorage;

impl PluginStorage for NoStorage {
    fn is_available(&mut self) -> bool {
        false
    }

    fn root_path(&mut self) -> Option<String> {
        None
    }

    fn read_text(&mut self, _key: &str) -> Result<Option<String>, StorageError> {
        Ok(None)
    }

    fn list(&mut self, _prefix: &str) -> Result<Vec<String>, StorageError> {
        Ok(Vec::new())
    }

    fn write_text(&mut self, _key: &str, _content: &str) -> Result<(), StorageError> {
        Err(UNAVAILABLE.to_owned())
    }

    fn ensure_directory(&mut self, _prefix: &str) -> Result<bool, StorageError> {
        Ok(false)
    }

    fn delete(&mut self, _key: &str) -> Result<bool, StorageError> {
        Ok(false)
    }
}

/// Where a store reports what went wrong (`IPluginLogger`).
pub trait StoreLog {
    /// A line at information level.
    fn info(&mut self, message: &str);
    /// A line a person should see.
    fn warn(&mut self, message: &str);
    /// A failure.
    fn error(&mut self, message: &str);
}

/// A log that keeps every line, for tests and for a caller that forwards
/// them later.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordingLog {
    /// Information lines.
    pub infos: Vec<String>,
    /// Warnings.
    pub warnings: Vec<String>,
    /// Errors.
    pub errors: Vec<String>,
}

impl StoreLog for RecordingLog {
    fn info(&mut self, message: &str) {
        self.infos.push(message.to_owned());
    }

    fn warn(&mut self, message: &str) {
        self.warnings.push(message.to_owned());
    }

    fn error(&mut self, message: &str) {
        self.errors.push(message.to_owned());
    }
}

/// A log that says nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NullLog;

impl StoreLog for NullLog {
    fn info(&mut self, _message: &str) {}
    fn warn(&mut self, _message: &str) {}
    fn error(&mut self, _message: &str) {}
}

/// `Path.Combine(root, relative)` for a path shown to the player: the
/// relative key's slashes become the root's own separator (a backslash for
/// a Windows root), and no separator is doubled.
#[must_use]
pub fn display_path(root: &str, relative: &str) -> String {
    let separator = if root.contains('\\') { '\\' } else { '/' };
    let relative = relative.replace('/', &separator.to_string());
    if root.is_empty() {
        return relative;
    }
    if relative.is_empty() {
        return root.to_owned();
    }
    if root.ends_with(['\\', '/']) {
        format!("{root}{relative}")
    } else {
        format!("{root}{separator}{relative}")
    }
}

/// Files kept in memory: the test double of every storage the C# tests
/// fake, and a session's storage when nothing else is given.
#[derive(Debug, Clone)]
pub struct MemoryStorage {
    files: BTreeMap<String, (String, String)>,
    case_sensitive: bool,
    /// Whether the storage is there at all.
    pub available: bool,
    /// What [`PluginStorage::root_path`] answers.
    pub root: Option<String>,
    /// Every folder asked for, in order, repeats included.
    pub directories: Vec<String>,
    /// Folders that answer no rather than being made.
    pub refuse_directories: Vec<String>,
    /// Folders whose creation fails outright.
    pub throw_directories: Vec<String>,
    /// Keys whose read fails outright.
    pub unreadable: Vec<String>,
    /// Keys whose write fails outright.
    pub unwritable: Vec<String>,
    /// Whether listing fails outright.
    pub unlistable: bool,
    /// Locks held, by handle.
    pub locks: Vec<(u32, String)>,
    next_lock: u32,
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryStorage {
    /// Empty storage whose keys ignore case (as Windows' file system does),
    /// rooted at `C:\memory`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            case_sensitive: false,
            available: true,
            root: Some("C:\\memory".to_owned()),
            directories: Vec::new(),
            refuse_directories: Vec::new(),
            throw_directories: Vec::new(),
            unreadable: Vec::new(),
            unwritable: Vec::new(),
            unlistable: false,
            locks: Vec::new(),
            next_lock: 1,
        }
    }

    /// Empty storage whose keys match case exactly.
    #[must_use]
    pub fn ordinal() -> Self {
        Self {
            case_sensitive: true,
            ..Self::new()
        }
    }

    /// Storage that is not there, as [`NoStorage`], but whose fields can
    /// still be looked at.
    #[must_use]
    pub fn unavailable() -> Self {
        Self {
            available: false,
            root: None,
            ..Self::new()
        }
    }

    fn fold(&self, key: &str) -> String {
        if self.case_sensitive {
            key.to_owned()
        } else {
            casing::to_upper_invariant(key)
        }
    }

    fn matches(&self, list: &[String], key: &str) -> bool {
        list.iter().any(|entry| {
            if self.case_sensitive {
                entry == key
            } else {
                ordinal::equals_ignore_case(entry, key)
            }
        })
    }

    /// Puts a file in place, as a test arranges it.
    pub fn insert(&mut self, key: &str, content: &str) {
        self.files
            .insert(self.fold(key), (key.to_owned(), content.to_owned()));
    }

    /// The text under `key`, if any.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.files
            .get(&self.fold(key))
            .map(|(_, text)| text.as_str())
    }

    /// Whether a file is stored under `key`.
    #[must_use]
    pub fn contains(&self, key: &str) -> bool {
        self.files.contains_key(&self.fold(key))
    }

    /// Takes a file away, as a test arranges it.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.files.remove(&self.fold(key)).map(|(_, text)| text)
    }

    /// Every key, as it was first written, in no particular order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.files.values().map(|(key, _)| key.as_str())
    }
}

impl PluginStorage for MemoryStorage {
    fn is_available(&mut self) -> bool {
        self.available
    }

    fn root_path(&mut self) -> Option<String> {
        if self.available {
            self.root.clone()
        } else {
            None
        }
    }

    fn read_text(&mut self, key: &str) -> Result<Option<String>, StorageError> {
        if !self.available {
            return Ok(None);
        }
        if self.matches(&self.unreadable, key) {
            return Err(format!("{key} is in use by another process."));
        }
        Ok(self.get(key).map(str::to_owned))
    }

    fn list(&mut self, prefix: &str) -> Result<Vec<String>, StorageError> {
        if !self.available {
            return Ok(Vec::new());
        }
        if self.unlistable {
            return Err("The folder cannot be listed.".to_owned());
        }
        // As the client lists a folder: every file beneath it.
        let folder = if prefix.is_empty() || prefix.ends_with('/') {
            prefix.to_owned()
        } else {
            format!("{prefix}/")
        };
        let mut keys: Vec<String> = self
            .files
            .values()
            .map(|(key, _)| key)
            .filter(|key| {
                if self.case_sensitive {
                    key.starts_with(&folder)
                } else {
                    ordinal::starts_with_ignore_case(key, &folder)
                }
            })
            .cloned()
            .collect();
        keys.sort_by(|a, b| ordinal::compare_ignore_case(a, b));
        Ok(keys)
    }

    fn write_text(&mut self, key: &str, content: &str) -> Result<(), StorageError> {
        if !self.available {
            return Err(UNAVAILABLE.to_owned());
        }
        if self.matches(&self.unwritable, key) {
            return Err(format!("Access to the path '{key}' is denied."));
        }
        let folded = self.fold(key);
        // A file keeps the spelling it was first written under, as a
        // case-blind file system does.
        let spelling = self
            .files
            .get(&folded)
            .map_or_else(|| key.to_owned(), |(existing, _)| existing.clone());
        self.files.insert(folded, (spelling, content.to_owned()));
        Ok(())
    }

    fn ensure_directory(&mut self, prefix: &str) -> Result<bool, StorageError> {
        if !self.available {
            return Ok(false);
        }
        self.directories.push(prefix.to_owned());
        if self.matches(&self.throw_directories, prefix) {
            return Err(format!("{prefix} is not writable."));
        }
        Ok(!self.matches(&self.refuse_directories, prefix))
    }

    fn delete(&mut self, key: &str) -> Result<bool, StorageError> {
        if !self.available {
            return Ok(false);
        }
        Ok(self.remove(key).is_some())
    }

    fn lock(&mut self, key: &str) -> Result<u32, StorageError> {
        if !self.available {
            return Err(UNAVAILABLE.to_owned());
        }
        let handle = self.next_lock;
        self.next_lock += 1;
        self.locks.push((handle, key.to_owned()));
        Ok(handle)
    }

    fn unlock(&mut self, handle: u32) -> bool {
        let before = self.locks.len();
        self.locks.retain(|(held, _)| *held != handle);
        self.locks.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_display_path_takes_the_roots_separator() {
        assert_eq!(
            display_path("C:\\memory", "mosstank/ub/maps"),
            "C:\\memory\\mosstank\\ub\\maps"
        );
        assert_eq!(
            display_path("/home/a", "mosstank/ub"),
            "/home/a/mosstank/ub"
        );
        assert_eq!(display_path("C:\\memory\\", "x"), "C:\\memory\\x");
    }

    #[test]
    fn memory_storage_ignores_case_and_keeps_the_first_spelling() {
        let mut storage = MemoryStorage::new();
        storage.write_text("A/b.txt", "1").unwrap();
        storage.write_text("a/B.TXT", "2").unwrap();
        assert_eq!(storage.read_text("a/b.txt").unwrap().as_deref(), Some("2"));
        assert_eq!(storage.list("a/").unwrap(), vec!["A/b.txt".to_owned()]);
    }
}

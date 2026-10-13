//! The global (`&`, `getgvar`) variables, shared by every character on one
//! server, live: each variable is its own file under
//! `expressions/global/<server hash>/`, so two clients writing different
//! variables never touch the same file, and every read and write holds a
//! lock other clients honour.

use mosstank_expr::json::{self as value_json, OPTIONS};
use mosstank_expr::value::ObjectNames;
use mosstank_expr::{ChangeCounter, ExpressionError, ExpressionValue, GlobalVariableStore};
use openac_plugin_compat::json::{self, JsonValue, system_text_json};
use openac_plugin_compat::text::{casing, ordinal};
use sha2::{Digest, Sha256};

use crate::recovery::hex_upper;
use crate::storage::{PluginStorage, StorageError};

const ROOT: &str = "expressions/global/";

/// Twelve bytes of SHA-256 in lower-case hex: how a server and a variable
/// name become a folder and a file name.
#[must_use]
pub fn hash(text: &str) -> String {
    casing::to_lower_invariant(&hex_upper(&Sha256::digest(text.as_bytes())[..12]))
}

/// What one variable file holds.
enum Row {
    Absent,
    Found(ExpressionValue),
    Damaged(String),
}

/// A storage failure: the expression or command that met it fails.
fn storage_failure(error: &StorageError) -> ExpressionError {
    ExpressionError::evaluation(format!(
        "The global variables could not be read or written: {error}"
    ))
}

/// The global variables of one server, in a storage the caller hands in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageGlobalVariableStore {
    folder: String,
}

impl StorageGlobalVariableStore {
    /// The variables of `server`.
    #[must_use]
    pub fn new(server: &str) -> Self {
        Self {
            folder: format!("{ROOT}{}", hash(server)),
        }
    }

    /// The folder the variables live in.
    #[must_use]
    pub fn folder(&self) -> &str {
        &self.folder
    }

    /// A variable's own file, keyed by its exact name.
    #[must_use]
    pub fn key_for(&self, name: &str) -> String {
        format!("{}/{}.json", self.folder, hash(&format!("name:{name}")))
    }

    /// Where earlier versions, whose names ignored case, saved a variable.
    #[must_use]
    pub fn old_key_for(&self, name: &str) -> String {
        format!(
            "{}/{}.json",
            self.folder,
            hash(&casing::to_upper_invariant(name))
        )
    }

    /// Runs `action` under the lock other clients honour (only storage
    /// backed by a folder is shared, so only it is locked); a storage
    /// failure fails the expression.
    fn locked<T>(
        &self,
        storage: &mut dyn PluginStorage,
        action: impl FnOnce(&mut dyn PluginStorage) -> Result<T, Failure>,
    ) -> Result<T, ExpressionError> {
        let handle = if storage.root_path().is_some() {
            Some(
                storage
                    .lock(&self.folder)
                    .map_err(|error| storage_failure(&error))?,
            )
        } else {
            None
        };
        let result = action(storage);
        if let Some(handle) = handle {
            storage.unlock(handle);
        }
        result.map_err(|failure| match failure {
            Failure::Storage(error) => storage_failure(&error),
            Failure::Expression(error) => error,
        })
    }

    fn read_key(
        storage: &mut dyn PluginStorage,
        key: &str,
        name: &str,
        names: Option<&ObjectNames>,
        counter: &ChangeCounter,
    ) -> Result<Row, Failure> {
        let Some(text) = storage.read_text(key)? else {
            return Ok(Row::Absent);
        };
        Ok(match parse(&text, names, counter) {
            Err(damage) => Row::Damaged(damage),
            // The file is keyed by a hash of the name; another name that
            // shares it, or the same name in another case, is not this one.
            Ok((stored, _)) if stored != name => Row::Absent,
            Ok((_, value)) => Row::Found(value),
        })
    }

    fn read(
        &self,
        storage: &mut dyn PluginStorage,
        name: &str,
        names: Option<&ObjectNames>,
        counter: &ChangeCounter,
    ) -> Result<Row, Failure> {
        match Self::read_key(storage, &self.key_for(name), name, names, counter)? {
            Row::Absent => Self::read_key(storage, &self.old_key_for(name), name, names, counter),
            row => Ok(row),
        }
    }

    /// One variable; a variable whose file cannot be read back fails the
    /// expression reading it.
    ///
    /// # Errors
    /// The file is damaged, or the storage fails.
    pub fn try_get(
        &self,
        storage: &mut dyn PluginStorage,
        names: Option<&ObjectNames>,
        counter: &ChangeCounter,
        name: &str,
    ) -> Result<Option<ExpressionValue>, ExpressionError> {
        let row = self.locked(storage, |storage| self.read(storage, name, names, counter))?;
        match row {
            Row::Found(value) => Ok(Some(value)),
            Row::Damaged(damage) => Err(ExpressionError::evaluation(format!(
                "Global variable {name} could not be read: {damage}"
            ))),
            Row::Absent => Ok(None),
        }
    }

    /// Whether a variable exists; a damaged one still does.
    ///
    /// # Errors
    /// The storage fails.
    pub fn contains(
        &self,
        storage: &mut dyn PluginStorage,
        counter: &ChangeCounter,
        name: &str,
    ) -> Result<bool, ExpressionError> {
        let row = self.locked(storage, |storage| self.read(storage, name, None, counter))?;
        Ok(!matches!(row, Row::Absent))
    }

    /// Writes one variable under its own key, dropping a copy saved under
    /// the old case-blind key.
    ///
    /// # Errors
    /// The value cannot be saved, or the storage fails.
    pub fn set(
        &self,
        storage: &mut dyn PluginStorage,
        counter: &ChangeCounter,
        name: &str,
        value: &ExpressionValue,
    ) -> Result<(), ExpressionError> {
        let document = JsonValue::object([
            ("Name", JsonValue::String(name.to_owned())),
            ("Value", value_json::store(value)?),
        ]);
        let text = system_text_json::to_string(&document, &OPTIONS)
            .map_err(|error| ExpressionError::evaluation(error.to_string()))?;
        self.locked(storage, |storage| {
            storage.write_text(&self.key_for(name), &text)?;
            let old = self.old_key_for(name);
            if matches!(
                Self::read_key(storage, &old, name, None, counter)?,
                Row::Found(_)
            ) {
                storage.delete(&old)?;
            }
            Ok(())
        })
    }

    /// Removes one variable (under either key); whether it existed.
    ///
    /// # Errors
    /// The storage fails.
    pub fn remove(
        &self,
        storage: &mut dyn PluginStorage,
        counter: &ChangeCounter,
        name: &str,
    ) -> Result<bool, ExpressionError> {
        self.locked(storage, |storage| {
            let mut removed = false;
            for key in [self.key_for(name), self.old_key_for(name)] {
                if !matches!(
                    Self::read_key(storage, &key, name, None, counter)?,
                    Row::Absent
                ) {
                    removed |= storage.delete(&key)?;
                }
            }
            Ok(removed)
        })
    }

    /// Removes every variable on this server.
    ///
    /// # Errors
    /// The storage fails.
    pub fn clear(&self, storage: &mut dyn PluginStorage) -> Result<(), ExpressionError> {
        self.locked(storage, |storage| {
            for key in self.variable_keys(storage)? {
                storage.delete(&key)?;
            }
            Ok(())
        })
    }

    /// Every variable on this server; one damaged file fails the whole
    /// capture.
    ///
    /// # Errors
    /// A file is damaged, or the storage fails.
    pub fn capture(
        &self,
        storage: &mut dyn PluginStorage,
        names: Option<&ObjectNames>,
        counter: &ChangeCounter,
    ) -> Result<Vec<(String, ExpressionValue)>, ExpressionError> {
        self.locked(storage, |storage| {
            let mut result: Vec<(String, ExpressionValue)> = Vec::new();
            let mut own_keys: Vec<String> = Vec::new();
            for key in self.variable_keys(storage)? {
                let Some(text) = storage.read_text(&key)? else {
                    continue;
                };
                let (name, value) = parse(&text, names, counter).map_err(|damage| {
                    let file = key
                        .rfind('/')
                        .map_or(key.as_str(), |slash| &key[slash + 1..]);
                    Failure::Expression(ExpressionError::evaluation(format!(
                        "A global variable file could not be read ({file}): {damage}"
                    )))
                })?;
                // A variable under its own key wins over an old copy.
                if key == self.key_for(&name) {
                    own_keys.push(name.clone());
                } else if own_keys.contains(&name) {
                    continue;
                }
                match result.iter_mut().find(|(known, _)| *known == name) {
                    Some(entry) => entry.1 = value,
                    None => result.push((name, value)),
                }
            }
            Ok(result)
        })
    }

    fn variable_keys(&self, storage: &mut dyn PluginStorage) -> Result<Vec<String>, Failure> {
        let own_prefix = format!("{}/", self.folder);
        Ok(storage
            .list(&self.folder)?
            .into_iter()
            .map(|key| key.replace('\\', "/"))
            .map(|key| {
                if key.starts_with(&own_prefix) {
                    key
                } else {
                    format!("{own_prefix}{key}")
                }
            })
            .filter(|key| {
                let file = key
                    .rfind('/')
                    .map_or(key.as_str(), |slash| &key[slash + 1..]);
                ordinal::ends_with_ignore_case(key, ".json") && !file.starts_with('.')
            })
            .collect())
    }
}

/// Why a locked operation failed.
enum Failure {
    Storage(StorageError),
    Expression(ExpressionError),
}

impl From<StorageError> for Failure {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

/// Reads a variable file back: its name and value, or why it is damaged
/// (not JSON, empty, missing its name or value, or holding a value that
/// cannot be rebuilt).
fn parse(
    text: &str,
    names: Option<&ObjectNames>,
    counter: &ChangeCounter,
) -> Result<(String, ExpressionValue), String> {
    let document = json::parse(text).map_err(|error| error.to_string())?;
    let name = match document.get_ignore_case("Name") {
        None | Some(JsonValue::Null) => String::new(),
        Some(JsonValue::String(name)) => name.clone(),
        Some(_) => {
            return Err(
                "The JSON value could not be converted to System.String. Path: $.Name".to_owned(),
            );
        }
    };
    let value = match document.get_ignore_case("Value") {
        None => Some(JsonValue::object::<&str>([])),
        Some(JsonValue::Null) => None,
        Some(value) => Some(value.clone()),
    };
    if !matches!(document, JsonValue::Object(_)) || name.is_empty() {
        return Err("it holds no variable".to_owned());
    }
    let Some(value) = value else {
        return Err("it holds no variable".to_owned());
    };
    let value = value_json::restore(&value, names, counter)?;
    Ok((name, value))
}

/// The store over a storage lent for one use of the variables, as the
/// expression state takes it ([`GlobalVariableStore`]): the plugin's
/// storage is reached only during its call, so the store is made for each
/// use from the storage that call holds.
pub struct GlobalVariables<'s> {
    /// The store.
    pub store: &'s StorageGlobalVariableStore,
    /// The storage, for this use.
    pub storage: &'s mut dyn PluginStorage,
    /// The counter values coming back count their changes on (the
    /// expression state's).
    pub counter: &'s ChangeCounter,
    /// How saved world objects print.
    pub names: Option<&'s ObjectNames>,
}

impl std::fmt::Debug for GlobalVariables<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlobalVariables")
            .field("store", &self.store)
            .finish_non_exhaustive()
    }
}

impl GlobalVariableStore for GlobalVariables<'_> {
    fn try_get(&mut self, name: &str) -> Result<Option<ExpressionValue>, ExpressionError> {
        self.store
            .try_get(&mut *self.storage, self.names, self.counter, name)
    }

    fn contains(&mut self, name: &str) -> Result<bool, ExpressionError> {
        self.store.contains(&mut *self.storage, self.counter, name)
    }

    fn set(&mut self, name: &str, value: &ExpressionValue) -> Result<(), ExpressionError> {
        self.store
            .set(&mut *self.storage, self.counter, name, value)
    }

    fn remove(&mut self, name: &str) -> Result<bool, ExpressionError> {
        self.store.remove(&mut *self.storage, self.counter, name)
    }

    fn clear(&mut self) -> Result<(), ExpressionError> {
        self.store.clear(&mut *self.storage)
    }

    fn capture(&mut self) -> Result<Vec<(String, ExpressionValue)>, ExpressionError> {
        self.store
            .capture(&mut *self.storage, self.names, self.counter)
    }
}

/// The store with a storage of its own ([`GlobalVariableStore`]): every
/// call reaches the shared files.
pub struct SharedGlobalVariables<S: PluginStorage> {
    store: StorageGlobalVariableStore,
    storage: S,
    counter: ChangeCounter,
    names: Option<ObjectNames>,
}

impl<S: PluginStorage> std::fmt::Debug for SharedGlobalVariables<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedGlobalVariables")
            .field("store", &self.store)
            .finish_non_exhaustive()
    }
}

impl<S: PluginStorage> SharedGlobalVariables<S> {
    /// The variables of `server` in `storage`; values come back counting
    /// their changes on `counter` (the expression state's), saved world
    /// objects printing by `names`.
    pub fn new(
        server: &str,
        storage: S,
        counter: ChangeCounter,
        names: Option<ObjectNames>,
    ) -> Self {
        Self {
            store: StorageGlobalVariableStore::new(server),
            storage,
            counter,
            names,
        }
    }

    /// The store.
    #[must_use]
    pub fn store(&self) -> &StorageGlobalVariableStore {
        &self.store
    }

    /// The storage, to look at.
    pub fn storage(&mut self) -> &mut S {
        &mut self.storage
    }

    fn lent(&mut self) -> GlobalVariables<'_> {
        GlobalVariables {
            store: &self.store,
            storage: &mut self.storage,
            counter: &self.counter,
            names: self.names.as_ref(),
        }
    }
}

impl<S: PluginStorage> GlobalVariableStore for SharedGlobalVariables<S> {
    fn try_get(&mut self, name: &str) -> Result<Option<ExpressionValue>, ExpressionError> {
        self.lent().try_get(name)
    }

    fn contains(&mut self, name: &str) -> Result<bool, ExpressionError> {
        self.lent().contains(name)
    }

    fn set(&mut self, name: &str, value: &ExpressionValue) -> Result<(), ExpressionError> {
        self.lent().set(name, value)
    }

    fn remove(&mut self, name: &str) -> Result<bool, ExpressionError> {
        self.lent().remove(name)
    }

    fn clear(&mut self) -> Result<(), ExpressionError> {
        self.lent().clear()
    }

    fn capture(&mut self) -> Result<Vec<(String, ExpressionValue)>, ExpressionError> {
        self.lent().capture()
    }
}

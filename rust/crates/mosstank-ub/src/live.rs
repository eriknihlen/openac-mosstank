//! The UB settings store shared between the catalogue and the rows whose
//! value lives somewhere other than the settings files ([`SharedUbStore`]),
//! and the store-backed live rows of `MossTankPanel.UbLiveOwners`: the
//! settings profile this character has open, the alias list (the file the
//! `Aliases.Profile` row names), and the two switches whose choice is
//! already made here.
//!
//! The rows owned by the macro's own settings (the item giver's four
//! numbers, the jump attempts) and by the meta profile (the event handlers)
//! are the plugin's to bind; [`UbSettingBinding`] takes a pair of closures
//! over whatever owns them.

use std::cell::RefCell;
use std::rc::Rc;

use mosstank_settings::ub::{
    UbSettingBinding, UbSettingScope, UbSettingValue, UbSettingValues, definitions,
};
use mosstank_stores::storage::{PluginStorage, StoreLog};
use mosstank_stores::ub_store::UbSettingStore;

/// Storage that outlives a plugin call (a test keeps one in memory).
pub type SharedStorageHandle = Rc<RefCell<Box<dyn PluginStorage>>>;
/// A log that outlives a plugin call.
pub type SharedLogHandle = Rc<RefCell<Box<dyn StoreLog>>>;

/// The UB settings store, its storage and its log, shared.
#[derive(Clone)]
pub struct SharedUbStore {
    store: Rc<RefCell<UbSettingStore>>,
    storage: SharedStorageHandle,
    log: SharedLogHandle,
}

impl std::fmt::Debug for SharedUbStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedUbStore")
            .field("profile", &self.store.borrow().profile_name())
            .finish_non_exhaustive()
    }
}

/// The profiles the settings page offers and opens.
pub trait UbProfiles {
    /// The named profile in use.
    fn profile_name(&self) -> String;
    /// Every profile that can be opened, `default` first.
    fn available_profiles(&mut self) -> Vec<String>;
    /// Opens a named profile and remembers the choice for this character.
    fn select_profile(&mut self, name: &str);
    /// Makes a named profile (with the open one's values when
    /// `copy_current`) and opens it; whether it was made, and the notice.
    fn create_profile(&mut self, name: &str, copy_current: bool) -> (bool, String);
}

impl SharedUbStore {
    /// A store over `storage`, read (`Load`).
    #[must_use]
    pub fn new(storage: SharedStorageHandle, log: SharedLogHandle) -> Self {
        let shared = Self {
            store: Rc::new(RefCell::new(UbSettingStore::new())),
            storage,
            log,
        };
        shared.with(|store, storage, log| store.load(storage, log));
        shared
    }

    fn with<R>(
        &self,
        f: impl FnOnce(&mut UbSettingStore, &mut dyn PluginStorage, &mut dyn StoreLog) -> R,
    ) -> R {
        let mut store = self.store.borrow_mut();
        let mut storage = self.storage.borrow_mut();
        let mut log = self.log.borrow_mut();
        f(&mut store, storage.as_mut(), log.as_mut())
    }

    /// Points the character tier at whoever is logged in now.
    pub fn bind(&self, server: &str, character: &str) {
        self.with(|store, storage, log| {
            store.bind(storage, log, Some(server), Some(character));
        });
    }

    /// Saves every tier changed since it was last saved; the plugin calls
    /// it before its call returns (register STORE-1).
    pub fn flush(&self) {
        self.with(|store, storage, log| store.flush(storage, log));
    }

    /// The value one row holds in the files, walking character, profile,
    /// installation and then the default.
    #[must_use]
    pub fn stored_value(&self, name: &str) -> Option<UbSettingValue> {
        let store = self.store.borrow();
        [
            UbSettingScope::Character,
            UbSettingScope::Profile,
            UbSettingScope::Global,
        ]
        .iter()
        .find_map(|scope| store.read(*scope, name))
        .or_else(|| {
            definitions::all()
                .iter()
                .find(|row| row.name == name)
                .map(|row| row.default.clone())
        })
    }

    /// The alias file the `Aliases.Profile` row names: the character's own
    /// until a shared one is named.
    #[must_use]
    pub fn alias_file_key(&self) -> Option<String> {
        let profile = self
            .stored_value("Aliases.Profile")
            .map(|value| value.text)
            .unwrap_or_default();
        self.store.borrow().alias_file_key(Some(&profile))
    }

    /// The alias list's lines.
    #[must_use]
    pub fn alias_lines(&self) -> Vec<String> {
        let key = self.alias_file_key();
        self.with(|store, storage, log| store.read_lines(storage, log, key.as_deref()))
    }

    /// Writes the alias list.
    pub fn write_alias_lines(&self, lines: &[String]) -> bool {
        let key = self.alias_file_key();
        self.with(|store, storage, log| store.write_lines(storage, log, key.as_deref(), lines))
    }

    /// The store-backed live rows (`UbLiveOwners`, the part the store
    /// owns): `Plugin.SettingsProfile`, `Aliases.DefinedAliases`, and the
    /// two switches whose choice is already made here.
    #[must_use]
    pub fn bindings(&self) -> Vec<(String, UbSettingBinding)> {
        let profile_read = self.clone();
        let mut profile_write = self.clone();
        let aliases_read = self.clone();
        let aliases_write = self.clone();
        let aliases_saved = self.clone();
        let mut aliases = UbSettingBinding::new(
            move || UbSettingValue::from_collection(&aliases_read.alias_lines()),
            move |value| {
                aliases_write.write_alias_lines(&value.items);
            },
        );
        aliases.can_save = Box::new(move || {
            aliases_saved.alias_file_key().is_some()
                && aliases_saved.storage.borrow_mut().is_available()
        });
        vec![
            (
                "Plugin.SettingsProfile".to_owned(),
                UbSettingBinding::new(
                    move || UbSettingValue::from_text(&profile_read.profile_name()),
                    move |value| profile_write.select_profile(value.text.trim()),
                ),
            ),
            ("Aliases.DefinedAliases".to_owned(), aliases),
            (
                "VTank.PatchExpressionEngine".to_owned(),
                UbSettingBinding::new(|| UbSettingValue::from_bool(true), |_| {}),
            ),
            (
                "InventoryManager.TreatStackAsSingleItem".to_owned(),
                UbSettingBinding::new(|| UbSettingValue::from_bool(false), |_| {}),
            ),
        ]
    }
}

impl UbSettingValues for SharedUbStore {
    fn read(&self, scope: UbSettingScope, name: &str) -> Option<UbSettingValue> {
        self.store.borrow().read(scope, name)
    }

    fn write(&mut self, scope: UbSettingScope, name: &str, value: UbSettingValue) {
        self.store.borrow_mut().write(scope, name, value);
    }

    fn clear(&mut self, scope: UbSettingScope, name: &str) -> bool {
        self.store.borrow_mut().clear(scope, name)
    }

    fn is_writable(&self, scope: UbSettingScope) -> bool {
        self.store.borrow().is_writable(scope)
    }
}

impl UbProfiles for SharedUbStore {
    fn profile_name(&self) -> String {
        self.store.borrow().profile_name().to_owned()
    }

    fn available_profiles(&mut self) -> Vec<String> {
        let names = self.with(|store, storage, _| store.available_profiles(storage));
        names.unwrap_or_else(|error| {
            self.log
                .borrow_mut()
                .warn(&format!("MossTank could not list the UB profiles: {error}"));
            vec![self.profile_name()]
        })
    }

    fn select_profile(&mut self, name: &str) {
        self.with(|store, storage, log| store.select_profile(storage, log, name));
    }

    fn create_profile(&mut self, name: &str, copy_current: bool) -> (bool, String) {
        let made =
            self.with(|store, storage, log| store.create_profile(storage, log, name, copy_current));
        made.unwrap_or_else(|error| (false, format!("The profile could not be made: {error}")))
    }
}

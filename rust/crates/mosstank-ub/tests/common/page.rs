//! The C# `UbPanel(storage)` for the UB tab's pages: the settings
//! catalogue over the real UB store on "Acdream" of Coldeve, with the
//! store's live rows and the panel's own (the item giver's numbers, the
//! jump attempts, the event handlers), and the settings page over it.
//! Every action ends as a plugin call does, with the store flushed.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use mosstank_settings::ub::{UbSettingBinding, UbSettingCatalog, UbSettingValue};
use mosstank_stores::storage::{PluginStorage, StoreLog};
use mosstank_ub::catalog::UbCatalog;
use mosstank_ub::live::{SharedUbStore, UbProfiles};
use mosstank_ub::settings_page::UbSettingsPage;
use mosstank_ub::testing::SharedStorage;

/// A store log that keeps what it was told.
#[derive(Debug, Default, Clone)]
pub struct RecordingLog(pub Rc<RefCell<Vec<String>>>);

impl StoreLog for RecordingLog {
    fn info(&mut self, message: &str) {
        self.0.borrow_mut().push(format!("info {message}"));
    }
    fn warn(&mut self, message: &str) {
        self.0.borrow_mut().push(format!("warn {message}"));
    }
    fn error(&mut self, message: &str) {
        self.0.borrow_mut().push(format!("error {message}"));
    }
}

/// The page's rig.
pub struct PageRig {
    pub storage: SharedStorage,
    pub store: SharedUbStore,
    pub catalog: UbSettingCatalog<SharedUbStore>,
    pub page: UbSettingsPage,
    pub handlers: Rc<RefCell<Vec<String>>>,
    pub jump_attempts: Rc<Cell<i32>>,
}

fn whole(cell: &Rc<Cell<i32>>, low: i32, high: i32) -> UbSettingBinding {
    let read = cell.clone();
    let write = cell.clone();
    UbSettingBinding::new(
        move || UbSettingValue::from_int(read.get()),
        move |value| write.set(value.as_int32().clamp(low, high)),
    )
}

fn decimal(cell: &Rc<Cell<f64>>, low: f64, high: f64) -> UbSettingBinding {
    let read = cell.clone();
    let write = cell.clone();
    UbSettingBinding::new(
        move || UbSettingValue::from_double(read.get()),
        move |value| write.set(value.as_double().clamp(low, high)),
    )
}

impl PageRig {
    /// `UbPanel(storage)` for Acdream of Coldeve.
    pub fn new(storage: &SharedStorage) -> Self {
        Self::for_character(storage, "Acdream")
    }

    /// The same for another character on the same computer.
    pub fn for_character(storage: &SharedStorage, character: &str) -> Self {
        let handle: Rc<RefCell<Box<dyn PluginStorage>>> =
            Rc::new(RefCell::new(Box::new(storage.clone())));
        let log: Rc<RefCell<Box<dyn StoreLog>>> =
            Rc::new(RefCell::new(Box::new(RecordingLog::default())));
        let store = SharedUbStore::new(handle, log);
        store.bind("Coldeve", character);
        let jump_attempts = Rc::new(Cell::new(3));
        let handlers = Rc::new(RefCell::new(Vec::new()));
        let mut live = store.bindings();
        live.push((
            "ItemGiver.Range".to_owned(),
            decimal(&Rc::new(Cell::new(15.0)), 0.0, 1000.0),
        ));
        live.push((
            "ItemGiver.Delay".to_owned(),
            decimal(&Rc::new(Cell::new(0.0)), 0.0, 60.0),
        ));
        live.push((
            "ItemGiver.BusyRetryLimit".to_owned(),
            whole(&Rc::new(Cell::new(10)), 1, 100),
        ));
        live.push((
            "ItemGiver.FailureLimit".to_owned(),
            whole(&Rc::new(Cell::new(3)), 0, 100),
        ));
        live.push(("Jumper.Attempts".to_owned(), whole(&jump_attempts, 1, 20)));
        let handlers_read = handlers.clone();
        let handlers_write = handlers.clone();
        live.push((
            "GameEvents.Handlers".to_owned(),
            UbSettingBinding::new(
                move || UbSettingValue::from_collection(&handlers_read.borrow()),
                move |value| *handlers_write.borrow_mut() = value.items,
            ),
        ));
        live.push((
            "GameEvents.Profile".to_owned(),
            UbSettingBinding::new(|| UbSettingValue::from_text("[character]"), |_| {}),
        ));
        let catalog = UbSettingCatalog::new(store.clone(), live);
        let page = UbSettingsPage::new(&catalog);
        Self {
            storage: storage.clone(),
            store,
            catalog,
            page,
            handlers,
            jump_attempts,
        }
    }

    fn done(&mut self) {
        self.store.flush();
    }

    pub fn filter(&mut self, text: &str) {
        self.page.set_filter_text(&self.catalog, text);
        self.done();
    }

    pub fn select(&mut self, index: i64) {
        self.page.select_setting(&self.catalog, index);
        self.done();
    }

    pub fn click(&mut self, index: i64) {
        self.page.click_value(&mut self.catalog, index);
        self.done();
    }

    pub fn submit(&mut self, text: &str) {
        self.page.submit(&mut self.catalog, text);
        self.done();
    }

    pub fn submit_draft(&mut self) {
        self.page.submit_draft(&mut self.catalog);
        self.done();
    }

    pub fn reset(&mut self) {
        self.page.reset_to_default(&mut self.catalog);
        self.done();
    }

    pub fn add_list_entry(&mut self, text: &str) {
        self.page.add_list_entry(&mut self.catalog, text);
        self.done();
    }

    pub fn add_list_entry_from_draft(&mut self) {
        self.page.add_list_entry_from_draft(&mut self.catalog);
        self.done();
    }

    pub fn replace_list_entry(&mut self) {
        self.page.replace_list_entry(&mut self.catalog);
        self.done();
    }

    pub fn remove_list_entry(&mut self) {
        self.page.remove_list_entry(&mut self.catalog);
        self.done();
    }

    pub fn apply_color(&mut self) {
        self.page.apply_color(&mut self.catalog);
        self.done();
    }

    pub fn create_profile(&mut self) {
        let mut profiles = self.store.clone();
        self.page.create_profile(&self.catalog, &mut profiles);
        self.done();
    }

    pub fn create_named_profile(&mut self, name: &str) {
        let mut profiles = self.store.clone();
        self.page
            .create_named_profile(&self.catalog, &mut profiles, name);
        self.done();
    }

    pub fn copy_profile(&mut self) {
        let mut profiles = self.store.clone();
        self.page.copy_profile(&self.catalog, &mut profiles);
        self.done();
    }

    pub fn select_profile(&mut self, name: &str) {
        let mut profiles = self.store.clone();
        self.page.select_profile(&self.catalog, &mut profiles, name);
        self.done();
    }

    pub fn profile_names(&mut self) -> Vec<String> {
        self.store.clone().available_profiles()
    }

    pub fn selected_profile(&self) -> String {
        self.store.profile_name()
    }

    pub fn scope_text(&self) -> String {
        self.page.scope_text(&self.store)
    }

    pub fn names(&self) -> Vec<String> {
        self.page.setting_names().to_vec()
    }

    pub fn values(&self) -> Vec<String> {
        self.page.setting_values().to_vec()
    }

    pub fn value(&self, index: usize) -> String {
        self.page.setting_values()[index].clone()
    }

    pub fn notice(&self) -> String {
        self.page.notice().to_owned()
    }

    pub fn file(&self, key: &str) -> String {
        self.storage
            .get(key)
            .unwrap_or_else(|| panic!("no file {key}"))
    }

    /// The value column for one row by name.
    pub fn row_value(&mut self, name: &str) -> String {
        let catalog: &dyn UbCatalog = &self.catalog;
        let row = catalog.require(name);
        catalog.display(&row)
    }
}

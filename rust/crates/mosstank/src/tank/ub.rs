//! The UB tab's wiring (`MossTankPanel.UbSettings` and the panel's side of
//! `MossTankCommands.Ub`): the UB settings store and catalogue, the `/ub`
//! word through the UB tab's dispatcher, the UB commands' frame, and the
//! rest of the plugin as the UB tab reaches it ([`UbServices`]).
//!
//! The settings store is read when the shell starts and bound to whoever
//! is in the world; a changed tier is written at the end of every call
//! (register STORE-1). The catalogue's live rows (`UbLiveOwners`) reach
//! their owners: the item giver's numbers and the jump attempts in the
//! macro's settings, the event handlers and their profile in the meta
//! profile, the alias list in its own file, the open UB profile in the
//! store (register UB-5).

use std::cell::RefCell;
use std::rc::Rc;

use ac_plugin_abi::callback::InterceptDecision;
use mosstank_expr::{ExpressionError, ExpressionState, ExpressionValue};
use mosstank_formats::utl::LootRule;
use mosstank_meta::engine::MetaEngine;
use mosstank_meta::held_motions::HeldMotions;
use mosstank_meta::host::{MetaHost, UbDisplay, UbLineKind};
use mosstank_settings::model::InventorySettings;
use mosstank_settings::options;
use mosstank_settings::ub::{
    UbSettingBinding, UbSettingCatalog, UbSettingScope, UbSettingValue, UbSettingValues,
};
use mosstank_stores::storage::{RecordingLog, Root};
use mosstank_stores::ub_store::UbSettingStore;
use mosstank_ub::catalog::{UbCatalog, message_display};
use mosstank_ub::commands::{UbCall, UbCommands, UbServices};
use mosstank_ub::expressions::UbExpressions;
use mosstank_ub::host::UbHost;

use super::services::{ClientCell, ClientProxy, CoreCell, TankServices};
use super::{Core, TankCx};
use crate::client::TankClient;

/// The UB settings store and the live rows' values, shared by the
/// catalogues that read them.
#[derive(Clone)]
pub(crate) struct StoreValues(
    pub(crate) Rc<RefCell<UbSettingStore>>,
    pub(crate) Rc<RefCell<LiveRows>>,
);

impl StoreValues {
    /// The store itself.
    pub(crate) fn store(&self) -> Rc<RefCell<UbSettingStore>> {
        self.0.clone()
    }
}

impl UbSettingValues for StoreValues {
    fn read(&self, scope: UbSettingScope, name: &str) -> Option<UbSettingValue> {
        self.0.borrow().read(scope, name)
    }

    fn write(&mut self, scope: UbSettingScope, name: &str, value: UbSettingValue) {
        self.0.borrow_mut().write(scope, name, value);
    }

    fn clear(&mut self, scope: UbSettingScope, name: &str) -> bool {
        self.0.borrow_mut().clear(scope, name)
    }

    fn is_writable(&self, scope: UbSettingScope) -> bool {
        self.0.borrow().is_writable(scope)
    }
}

/// A running number a UB row edits (`LiveUbDecimal`, `LiveUbWhole`), held
/// to the bound the macro itself is held to.
#[derive(Debug, Clone, Copy)]
enum LiveNumber {
    Decimal(f64, f64),
    Whole(i32, i32),
}

impl LiveNumber {
    /// The value as the owner takes it: inside the bound.
    fn held(self, value: &UbSettingValue) -> UbSettingValue {
        match self {
            Self::Decimal(lowest, highest) => {
                UbSettingValue::from_double(value.as_double().clamp(lowest, highest))
            }
            Self::Whole(lowest, highest) => {
                UbSettingValue::from_int(value.as_int32().clamp(lowest, highest))
            }
        }
    }
}

/// The four numbers that pace handing items over, and how many times a
/// jump is tried: the macro's own settings, saved with its profile.
const LIVE_NUMBERS: [(&str, LiveNumber); 5] = [
    ("ItemGiver.Range", LiveNumber::Decimal(0.0, 1000.0)),
    ("ItemGiver.Delay", LiveNumber::Decimal(0.0, 60.0)),
    ("ItemGiver.BusyRetryLimit", LiveNumber::Whole(1, 100)),
    ("ItemGiver.FailureLimit", LiveNumber::Whole(0, 100)),
    ("Jumper.Attempts", LiveNumber::Whole(1, 20)),
];

/// The event handlers: rules in the meta profile, shown as lines.
const HANDLERS: &str = "GameEvents.Handlers";
/// The handlers' profile row: the meta profile picker.
const HANDLER_PROFILE: &str = "GameEvents.Profile";
/// The per-character meta profile, as the handlers' profile row names it.
const CHARACTER_META: &str = "[character]";
/// The alias list: a file of its own, named by the `Aliases.Profile` row.
const ALIASES: &str = "Aliases.DefinedAliases";
/// The UB settings profile this character has open.
const SETTINGS_PROFILE: &str = "Plugin.SettingsProfile";

/// The rows whose value belongs to something running (`UbLiveOwners`): the
/// values the shell last read from their owners, and the writes made to
/// them during a call, which reach the owners before the call returns
/// (register UB-5).
#[derive(Debug, Default)]
pub(crate) struct LiveRows {
    values: std::collections::HashMap<&'static str, UbSettingValue>,
    /// Whether an alias list written now outlives the session.
    alias_can_save: bool,
    written: Vec<(&'static str, UbSettingValue)>,
}

impl LiveRows {
    fn get(&self, name: &'static str) -> UbSettingValue {
        self.values
            .get(name)
            .cloned()
            .unwrap_or_else(|| UbSettingValue::from_text(""))
    }

    /// A write: the row reads it at once, and its owner takes it at the end
    /// of the call.
    fn write(&mut self, name: &'static str, value: UbSettingValue) {
        self.values.insert(name, value.clone());
        self.written.push((name, value));
    }
}

/// A row over a value the shell keeps for its owner.
fn live_binding(
    live: &Rc<RefCell<LiveRows>>,
    name: &'static str,
    held: impl Fn(&UbSettingValue) -> UbSettingValue + 'static,
) -> UbSettingBinding {
    let read = live.clone();
    let write = live.clone();
    UbSettingBinding::new(
        move || read.borrow().get(name),
        move |value| write.borrow_mut().write(name, held(&value)),
    )
}

/// A row over a value the shell keeps for its owner, written only through
/// [`CallCatalog`].
fn read_only_binding(live: &Rc<RefCell<LiveRows>>, name: &'static str) -> UbSettingBinding {
    let read = live.clone();
    UbSettingBinding::new(move || read.borrow().get(name), |_| {})
}

/// The rows whose value is owned by something other than the UB settings
/// files (`UbLiveOwners`): their reads and writes reach that owner, so the
/// page and the macro never hold two different values.
pub(crate) fn owned_rows(values: &StoreValues) -> Vec<(String, UbSettingBinding)> {
    let mut rows = Vec::new();
    for (name, number) in LIVE_NUMBERS {
        rows.push((
            name.to_owned(),
            live_binding(&values.1, name, move |value| number.held(value)),
        ));
    }
    rows.push((
        HANDLERS.to_owned(),
        live_binding(&values.1, HANDLERS, Clone::clone),
    ));
    // The meta profile picked, the alias list and the UB profile open live in
    // storage: a call writes them through its own catalogue ([`CallCatalog`]),
    // which reaches the client, so the rows here only read.
    rows.push((
        HANDLER_PROFILE.to_owned(),
        read_only_binding(&values.1, HANDLER_PROFILE),
    ));
    let mut aliases = read_only_binding(&values.1, ALIASES);
    let can_save = values.1.clone();
    aliases.can_save = Box::new(move || can_save.borrow().alias_can_save);
    rows.push((ALIASES.to_owned(), aliases));
    // The profile name is the store's choice for this character.
    let store = values.0.clone();
    rows.push((
        SETTINGS_PROFILE.to_owned(),
        UbSettingBinding::new(
            move || UbSettingValue::from_text(store.borrow().profile_name()),
            |_| {},
        ),
    ));
    // Two choices already made here: they read what is in force, and a
    // write leaves it.
    rows.push((
        "VTank.PatchExpressionEngine".to_owned(),
        UbSettingBinding::new(|| UbSettingValue::from_bool(true), |_| {}),
    ));
    rows.push((
        "InventoryManager.TreatStackAsSingleItem".to_owned(),
        UbSettingBinding::new(|| UbSettingValue::from_bool(false), |_| {}),
    ));
    rows
}

/// What a catalogue write reaches during a call: the client and the
/// shell's parts, held directly or through the cells the call shares.
pub(crate) enum OwnerReach<'a, 'c> {
    /// Held directly (a window action).
    Direct(&'a mut dyn TankClient, &'a mut Core),
    /// Shared with the call's other parts (a `/ub` command, an expression).
    Shared(&'a ClientCell<'c>, &'a CoreCell<'c>),
}

impl OwnerReach<'_, '_> {
    fn with<R>(&mut self, f: impl FnOnce(&mut dyn TankClient, &mut Core) -> R) -> R {
        match self {
            Self::Direct(client, core) => f(&mut **client, core),
            Self::Shared(client, core) => {
                let mut client = client.0.borrow_mut();
                let mut core = core.0.borrow_mut();
                f(&mut **client, &mut core)
            }
        }
    }
}

/// The catalogue as one call writes it. The rows whose owner lives in
/// storage take a write at once, through the call's client, as the C#
/// setters do: the UB profile open (`SelectProfile`, then the page is
/// refreshed), the meta profile picked (`SelectMetaProfileCore`; the
/// engine loads it when the call's step ends, register UB-5) and the alias
/// list (written to the file the profile row names now). Every other row is
/// the catalogue's.
pub(crate) struct CallCatalog<'a, 'c> {
    /// The catalogue.
    pub(crate) catalog: &'a mut UbSettingCatalog<StoreValues>,
    /// The client and the shell's parts.
    pub(crate) reach: OwnerReach<'a, 'c>,
}

impl CallCatalog<'_, '_> {
    /// `_ubStore.SelectProfile(name)`; the page is refreshed when the
    /// call's step ends.
    fn select_ub_profile(&mut self, name: &str) {
        let store = self.catalog.values().0.clone();
        self.reach.with(|client, _| {
            let mut log = RecordingLog::default();
            {
                let storage = client.stores().storage(Root::Profiles);
                store.borrow_mut().select_profile(storage, &mut log, name);
            }
            forward_log(client, &log);
        });
        self.catalog
            .values()
            .1
            .borrow_mut()
            .written
            .push((SETTINGS_PROFILE, UbSettingValue::from_text(name)));
    }

    /// `SelectMetaProfileCore`: the pick is made now and the row reads the
    /// store's choice; the engine loads the profile when the step ends.
    fn select_meta_profile(&mut self, text: &str) {
        let name = if mosstank_dotnet::text::ordinal::equals_ignore_case(text, CHARACTER_META) {
            mosstank_stores::meta_store::BY_CHARACTER
        } else {
            text
        };
        let shown = self.reach.with(|client, core| {
            if !super::profiles::pick_meta_profile(core, client, name) {
                return None;
            }
            let selected =
                super::profiles::report!(client, core.meta_profiles.selected(client.stores()))
                    .unwrap_or_default();
            Some(shown_meta_profile(selected))
        });
        if let Some(shown) = shown {
            let mut live = self.catalog.values().1.borrow_mut();
            live.values
                .insert(HANDLER_PROFILE, UbSettingValue::from_text(&shown));
            live.written
                .push((HANDLER_PROFILE, UbSettingValue::from_text(&shown)));
        }
    }

    /// The alias list's write, to the file the profile row names now.
    fn write_aliases(&mut self, value: UbSettingValue) {
        let store = self.catalog.values().0.clone();
        let profile = UbCatalog::value(&*self.catalog, "Aliases.Profile");
        let key = store.borrow().alias_file_key(Some(&profile.text));
        self.reach.with(|client, _| {
            let mut log = RecordingLog::default();
            {
                let storage = client.stores().storage(Root::Profiles);
                store
                    .borrow_mut()
                    .write_lines(storage, &mut log, key.as_deref(), &value.items);
            }
            forward_log(client, &log);
        });
        self.catalog
            .values()
            .1
            .borrow_mut()
            .values
            .insert(ALIASES, value);
    }
}

/// The meta profile's name as its row shows it: `[character]` for the
/// per-character profile.
fn shown_meta_profile(selected: String) -> String {
    if mosstank_dotnet::text::ordinal::equals_ignore_case(
        &selected,
        mosstank_stores::meta_store::BY_CHARACTER,
    ) {
        CHARACTER_META.to_owned()
    } else {
        selected
    }
}

impl UbCatalog for CallCatalog<'_, '_> {
    fn settings(&self) -> &[mosstank_settings::ub::UbSetting] {
        UbCatalog::settings(&*self.catalog)
    }

    fn categories(&self) -> &[String] {
        UbCatalog::categories(&*self.catalog)
    }

    fn try_get(&self, name: &str) -> Option<mosstank_settings::ub::UbSetting> {
        UbCatalog::try_get(&*self.catalog, name)
    }

    fn get(&self, row: &mosstank_settings::ub::UbSetting) -> UbSettingValue {
        UbCatalog::get(&*self.catalog, row)
    }

    fn set(&mut self, row: &mosstank_settings::ub::UbSetting, value: UbSettingValue) {
        if row.has_live_owner {
            let is =
                |name: &str| mosstank_dotnet::text::ordinal::equals_ignore_case(row.name(), name);
            if is(SETTINGS_PROFILE) {
                return self.select_ub_profile(value.text.trim());
            }
            if is(HANDLER_PROFILE) {
                return self.select_meta_profile(value.text.trim());
            }
            if is(ALIASES) {
                return self.write_aliases(value);
            }
        }
        UbCatalog::set(&mut *self.catalog, row, value);
    }

    fn can_save(&self, row: &mosstank_settings::ub::UbSetting) -> bool {
        UbCatalog::can_save(&*self.catalog, row)
    }

    fn display(&self, row: &mosstank_settings::ub::UbSetting) -> String {
        UbCatalog::display(&*self.catalog, row)
    }

    fn full_display_value(&self, row: &mosstank_settings::ub::UbSetting) -> String {
        UbCatalog::full_display_value(&*self.catalog, row)
    }

    fn reset_to_default(&mut self, row: &mosstank_settings::ub::UbSetting) {
        if row.has_live_owner {
            let default = row.definition.default.clone();
            self.set(row, default);
        } else {
            UbCatalog::reset_to_default(&mut *self.catalog, row);
        }
    }
}

/// The UB tab's parts.
#[allow(missing_debug_implementations)]
pub struct UbParts {
    pub(crate) store: Rc<RefCell<UbSettingStore>>,
    /// The catalogue the `/ub` commands reach.
    pub(crate) catalog: UbSettingCatalog<StoreValues>,
    /// The same catalogue, for the readers outside a `/ub` call (the
    /// fight's cast sharing, the meta's event switch and debug setting).
    pub(crate) reader: Rc<UbSettingCatalog<StoreValues>>,
    /// What the `/ub` commands keep between calls.
    pub(crate) commands: UbCommands,
    /// The UB settings page and its colour and list windows.
    pub(crate) page: mosstank_ub::settings_page::UbSettingsPage,
    /// The countryside map.
    pub(crate) landscape: mosstank_ub::landscape::controller::LandscapeMapController,
    /// The dungeon map.
    pub(crate) dungeon: mosstank_ub::dungeon::controller::DungeonMapController,
    /// The other clients' readout.
    pub(crate) hud: mosstank_ub::hud::NetworkHudController,
    /// The three tools' canvases.
    pub(crate) canvases: crate::canvases::Canvases,
    /// The labels last handed to the client (`PublishNetworkTags`).
    pub(crate) tags: mosstank_ub::network::TagPublisher,
    /// The aliases' one interceptor on typed chat (`InitializeAliases`).
    pub(crate) aliases: mosstank_ub::alias::AliasController,
}

/// The four kinds of UB line.
const KINDS: [UbLineKind; 4] = [
    UbLineKind::Generic,
    UbLineKind::Debug,
    UbLineKind::Expression,
    UbLineKind::Error,
];

/// Writes a store's log lines to the client's log.
pub(crate) fn forward_log(client: &mut dyn TankClient, log: &RecordingLog) {
    for line in &log.lines {
        if let Some(text) = line.strip_prefix("info ") {
            client.rules().log_info(text);
        } else if let Some(text) = line.strip_prefix("warn ") {
            client.rules().log_warn(text);
        } else if let Some(text) = line.strip_prefix("error ") {
            client.looter().log_error(text);
        }
    }
}

impl UbParts {
    /// The store read from the profile folder and bound to whoever is in
    /// the world, and the catalogue over it (`InitializeUbSettings`).
    pub(crate) fn new(client: &mut dyn TankClient) -> Self {
        let store = Rc::new(RefCell::new(UbSettingStore::new()));
        let mut log = RecordingLog::default();
        {
            let storage = client.stores().storage(Root::Profiles);
            store.borrow_mut().load(storage, &mut log);
        }
        let character = client.rules().character();
        {
            let storage = client.stores().storage(Root::Profiles);
            store.borrow_mut().bind(
                storage,
                &mut log,
                Some(&character.world),
                Some(&character.name),
            );
        }
        forward_log(client, &log);
        let values = StoreValues(store.clone(), Rc::new(RefCell::new(LiveRows::default())));
        let mut catalog = UbSettingCatalog::new(values.clone(), owned_rows(&values));
        let page = mosstank_ub::settings_page::UbSettingsPage::new(&catalog);
        let mut landscape = mosstank_ub::landscape::controller::LandscapeMapController::new(
            &mosstank_ub::landscape::controller::CatalogLandscapeSettings(&catalog),
        );
        if let Err(error) = landscape.load_markers(client.stores().storage(Root::Profiles)) {
            client
                .rules()
                .log_warn(&format!("MossTank could not read the map markers: {error}"));
        }
        let dungeon = mosstank_ub::dungeon::controller::DungeonMapController::new(
            &mosstank_ub::dungeon::controller::CatalogMapSettings(&mut catalog),
        );
        let hud = mosstank_ub::hud::NetworkHudController::new(
            &mosstank_ub::hud::CatalogHudSettings(&catalog),
        );
        // Handed over while the tab is built (`PublishNetworkTags(onLoad)`).
        let mut tags = mosstank_ub::network::TagPublisher::new();
        tags.publish(
            client.ub(),
            mosstank_ub::catalog::UbCatalog::value(&catalog, "Networking.Tags").items,
            true,
        );
        Self {
            catalog,
            reader: Rc::new(UbSettingCatalog::new(values.clone(), owned_rows(&values))),
            store,
            commands: UbCommands::new(),
            page,
            landscape,
            dungeon,
            hud,
            canvases: crate::canvases::Canvases::default(),
            tags,
            aliases: mosstank_ub::alias::AliasController::new(),
        }
    }

    /// Points the character tier at whoever is logged in now
    /// (`BindUbSettingsToCharacter`).
    pub(crate) fn bind(&mut self, client: &mut dyn TankClient, world: &str, character: &str) {
        let mut log = RecordingLog::default();
        {
            let storage = client.stores().storage(Root::Profiles);
            self.store
                .borrow_mut()
                .bind(storage, &mut log, Some(world), Some(character));
        }
        forward_log(client, &log);
    }

    /// Writes every tier changed during the call (register STORE-1).
    pub(crate) fn flush(&mut self, client: &mut dyn TankClient) {
        let mut log = RecordingLog::default();
        {
            let storage = client.stores().storage(Root::Profiles);
            self.store.borrow_mut().flush(storage, &mut log);
        }
        forward_log(client, &log);
    }

    /// The settings store, shared.
    pub(crate) fn values(&self) -> StoreValues {
        self.catalog.values().clone()
    }

    /// How each kind of UB line is shown now (`UbMessageDisplay`).
    pub(crate) fn displays(&self) -> [UbDisplay; 4] {
        KINDS.map(|kind| message_display(&self.catalog, kind))
    }
}

/// The meta engine's expressions as the UB tab reaches them, with the
/// plugin's services.
struct EngineExpressions<'s, 'c> {
    engine: &'s mut MetaEngine,
    services: TankServices<'s, 'c>,
}

impl UbExpressions for EngineExpressions<'_, '_> {
    fn evaluate(
        &mut self,
        host: &mut dyn MetaHost,
        source: &str,
    ) -> Result<ExpressionValue, ExpressionError> {
        self.engine.evaluate(host, &mut self.services, source)
    }

    fn display(&mut self, host: &mut dyn MetaHost, value: &ExpressionValue) -> String {
        self.engine.display(host, value)
    }

    fn state(&mut self) -> &mut ExpressionState {
        self.engine.expressions().state()
    }

    fn capture_variables(
        &mut self,
        host: &mut dyn MetaHost,
        scope: mosstank_expr::ExpressionVariableScope,
    ) -> Result<Vec<(String, ExpressionValue)>, ExpressionError> {
        self.engine.expressions().capture_variables(host, scope)
    }

    fn held_motions(&mut self) -> &mut HeldMotions {
        self.engine.expressions().held_motions()
    }

    fn refresh_quests(&mut self, host: &mut dyn MetaHost) {
        self.engine.expressions().refresh_quests(host);
    }
}

/// The rest of the plugin as the UB commands reach it.
struct ShellUbServices<'s, 'c> {
    core: &'s CoreCell<'c>,
    client: &'s ClientCell<'c>,
    reader: Rc<UbSettingCatalog<StoreValues>>,
    meta_enabled: bool,
    /// `EnableMeta` written during the call, for the engine afterwards.
    meta_written: Option<bool>,
}

impl ShellUbServices<'_, '_> {
    /// Runs `f` with the vendor run and the whole client and parts of the
    /// call.
    fn with_vendor<R>(
        &mut self,
        f: impl FnOnce(
            &mut mosstank_vendor::VendorTradeController,
            &mut super::vendor::ShellVendor<'_>,
        ) -> R,
    ) -> R {
        let mut client = self.client.0.borrow_mut();
        let mut core = self.core.0.borrow_mut();
        super::vendor::with_vendor(&mut **client, &mut core, &self.reader, f)
    }
}

impl UbServices for ShellUbServices<'_, '_> {
    fn plugin_version(&self) -> String {
        crate::VERSION.to_owned()
    }

    fn now_utc_ms(&mut self) -> i64 {
        self.client.0.borrow_mut().clock().utc_unix_ms
    }

    fn local_offset_minutes(&mut self) -> i32 {
        self.client.0.borrow_mut().clock().utc_offset_minutes
    }

    #[allow(clippy::cast_precision_loss)]
    fn monotonic_ms(&mut self) -> f64 {
        self.client.0.borrow_mut().clock().monotonic_ms as f64
    }

    fn is_vtank_option(&mut self, name: &str) -> bool {
        options::is_known(name)
    }

    fn get_meta_option(&mut self, _host: &mut dyn UbHost, name: &str) -> ExpressionValue {
        let meta_enabled = self.meta_written.unwrap_or(self.meta_enabled);
        self.core.0.borrow_mut().get_meta_option(meta_enabled, name)
    }

    /// The macro's one option-write path; a value of the wrong type is
    /// refused.
    fn set_meta_option(
        &mut self,
        _host: &mut dyn UbHost,
        name: &str,
        value: &ExpressionValue,
    ) -> bool {
        let written = {
            let mut client = self.client.0.borrow_mut();
            self.core
                .0
                .borrow_mut()
                .set_option(&mut **client, name, value)
        };
        let written = written.unwrap_or(false);
        if written
            && options::canonical(name).is_some_and(|canonical| {
                mosstank_dotnet::text::ordinal::equals_ignore_case(canonical, "EnableMeta")
            })
        {
            self.meta_written = Some(value.is_truthy());
        }
        written
    }

    /// `/ub vendor` (`VendorTradeController.VendorCommand`).
    fn vendor_command(&mut self, _host: &mut dyn UbHost, arguments: &str) -> Option<Vec<String>> {
        self.with_vendor(|controller, cx| controller.vendor_command(cx, arguments))
    }

    /// `/ub autovendor [profile]`: the run says what it has to say itself,
    /// nothing when it starts and its errors when it cannot.
    fn auto_vendor_start(&mut self, _host: &mut dyn UbHost, profile: Option<&str>) {
        self.with_vendor(|controller, cx| {
            controller.try_start(cx, profile);
        });
    }

    /// `/ub autovendor stop` (or cancel, quit).
    fn auto_vendor_stop(&mut self, _host: &mut dyn UbHost) {
        self.with_vendor(|controller, cx| {
            controller.stop_requested(cx);
        });
    }

    fn vendor_tick(&mut self, _host: &mut dyn UbHost, elapsed_seconds: f64, can_act: bool) -> bool {
        self.with_vendor(|controller, cx| controller.tick(cx, elapsed_seconds, can_act))
    }

    /// `HandleFellowCommand`.
    fn fellow_command(&mut self, _host: &mut dyn UbHost, arguments: &str) -> bool {
        let debug = self.reader.ub_debug();
        let mut client = self.client.0.borrow_mut();
        mosstank_vendor::fellow_command::run(client.fellow_commands(), arguments, debug)
            == mosstank_vendor::fellow_command::FellowCommandOutcome::Done
    }

    fn inventory_settings(&mut self) -> InventorySettings {
        self.core.0.borrow().settings.inventory.clone()
    }

    /// `TryLoadItemGiver`; a storage failure is logged and reads as no file.
    fn item_giver_rules(&mut self, _host: &mut dyn UbHost, name: &str) -> Option<Vec<LootRule>> {
        let mut client = self.client.0.borrow_mut();
        let mut rules = Vec::new();
        let loaded = self.core.0.borrow().loot_profiles.try_load_item_giver(
            client.stores(),
            Some(name),
            &mut rules,
        );
        loaded_rules(&mut **client, loaded, rules)
    }

    /// `TryLoadNamed`; a storage failure is logged and reads as no file.
    fn loot_profile_rules(&mut self, _host: &mut dyn UbHost, name: &str) -> Option<Vec<LootRule>> {
        let mut client = self.client.0.borrow_mut();
        let mut rules = Vec::new();
        let loaded = self.core.0.borrow().loot_profiles.try_load_named(
            client.stores(),
            Some(name),
            &mut rules,
        );
        loaded_rules(&mut **client, loaded, rules)
    }

    fn corpse_identifier_busy(&mut self) -> bool {
        self.core.0.borrow().loot.is_identifying()
    }

    fn macro_enabled(&mut self) -> bool {
        self.core.0.borrow().combat.enabled()
    }

    fn combat_has_target(&mut self) -> bool {
        self.core.0.borrow().combat.has_target()
    }

    fn ub_store(&mut self) -> UbSettingStore {
        self.reader.values().0.borrow().clone()
    }

    /// `FollowOnFollowRoute`.
    fn follow(&mut self, _host: &mut dyn UbHost, player_id: u32, name: &str) -> bool {
        let mut client = self.client.0.borrow_mut();
        let mut core = self.core.0.borrow_mut();
        let (followed, meta_enabled) = core.follow_on_follow_route(&mut **client, player_id, name);
        if let Some(enabled) = meta_enabled {
            self.meta_written = Some(enabled);
        }
        followed
    }

    fn navigation_enabled(&mut self) -> bool {
        self.core.0.borrow().settings.navigation.enabled
    }

    fn jump_attempts(&mut self) -> i32 {
        self.core.0.borrow().settings.navigation.jump_attempts
    }

    fn translate_route(
        &mut self,
        _host: &mut dyn UbHost,
        from: &str,
        to: &str,
        east_west: f64,
        north_south: f64,
        force: bool,
    ) -> Result<(String, i32), String> {
        let mut client = self.client.0.borrow_mut();
        let spells = super::profiles::SpellsSnapshot::read(&mut **client);
        let mut core = self.core.0.borrow_mut();
        let (translated, notice, records) = core.route_profiles.try_translate(
            client.stores(),
            Some(from),
            Some(to),
            east_west,
            north_south,
            force,
            Some(&spells),
        )?;
        if translated {
            Ok((notice, i32::try_from(records).unwrap_or(i32::MAX)))
        } else {
            Err(notice)
        }
    }
}

impl Core {
    /// `FollowOnFollowRoute`: the follow route aimed at a player, read in
    /// full when it is not the route in memory (`LoadRouteProfile`). Whether
    /// it is aimed, and `EnableMeta` when the route's load wrote it.
    pub(crate) fn follow_on_follow_route(
        &mut self,
        client: &mut dyn TankClient,
        object_id: u32,
        name: &str,
    ) -> (bool, Option<bool>) {
        let mut meta_enabled = None;
        if self.embedded_route_label.is_some() || !self.route_profiles.follow_route_selected() {
            if let Err(error) = self.route_profiles.select_follow_route(client.stores()) {
                client
                    .rules()
                    .log_warn(&format!("MossTank storage failure: {error}"));
            }
            let (loaded, written) = self.load_route_profile_in_full(client);
            if !loaded {
                return (false, written);
            }
            meta_enabled = written;
        }
        self.settings.navigation.follow_target_object_id = object_id;
        name.clone_into(&mut self.settings.navigation.follow_target_name);
        self.settings.navigation.mode = mosstank_formats::route::RouteMode::Target;
        {
            let mut turn = super::rules::route_turn!(client, self);
            self.navigation.reset(&mut turn);
        }
        self.save_route_profile(client);
        (true, meta_enabled)
    }
}

impl TankCx<'_> {
    /// Runs `f` with one call into the UB tab, then does what its
    /// expressions asked of the engine and writes out its lines.
    pub(crate) fn with_ub<R>(
        &mut self,
        f: impl FnOnce(&mut UbCommands, &mut UbCall<'_>) -> R,
    ) -> R {
        let meta_enabled = self.meta.enabled();
        let locks = self.core.locks.clone();
        let reader = self.ub.reader.clone();
        let (result, requests, meta_written) = {
            let cell = ClientCell(RefCell::new(&mut *self.client));
            let core = CoreCell(RefCell::new(&mut *self.core));
            let mut proxy = ClientProxy::new(&cell);
            let mut expressions = EngineExpressions {
                engine: &mut *self.meta,
                services: TankServices {
                    core: &core,
                    client: &cell,
                    ub: reader.clone(),
                    ub_values: self.ub.values(),
                    ub_give: self.ub.commands.tools().give.clone(),
                    ub_count: self.ub.commands.tools().count.clone(),
                    meta_enabled,
                    readout_should_show: self.ub.hud.should_show(),
                    requests: Vec::new(),
                },
            };
            let mut services = ShellUbServices {
                core: &core,
                client: &cell,
                reader,
                meta_enabled,
                meta_written: None,
            };
            let mut catalog = CallCatalog {
                catalog: &mut self.ub.catalog,
                reach: OwnerReach::Shared(&cell, &core),
            };
            let mut call = UbCall {
                host: &mut proxy,
                settings: &mut catalog,
                expressions: &mut expressions,
                services: &mut services,
                locks: &locks,
            };
            let result = f(&mut self.ub.commands, &mut call);
            (result, expressions.services.requests, services.meta_written)
        };
        if let Some(enabled) = meta_written {
            self.meta.set_enabled(enabled);
        }
        let displays = self.ub.displays();
        self.client.set_ub_displays(displays);
        for request in requests {
            self.run_engine_request(request);
        }
        self.core.write_out(self.client);
        result
    }
}

impl TankCx<'_> {
    /// Reads the live rows' values from their owners: the running numbers
    /// and the handler lines; with `files`, also the meta profile picked and
    /// the alias list, which are read from storage.
    pub(crate) fn refresh_live_ub_rows(&mut self, files: bool) {
        let settings = &self.core.settings;
        let mut values = vec![
            (
                LIVE_NUMBERS[0].0,
                UbSettingValue::from_double(settings.inventory.give_range_meters),
            ),
            (
                LIVE_NUMBERS[1].0,
                UbSettingValue::from_double(settings.inventory.give_delay_seconds),
            ),
            (
                LIVE_NUMBERS[2].0,
                UbSettingValue::from_int(settings.inventory.give_busy_retry_limit),
            ),
            (
                LIVE_NUMBERS[3].0,
                UbSettingValue::from_int(settings.inventory.give_failure_limit),
            ),
            (
                LIVE_NUMBERS[4].0,
                UbSettingValue::from_int(settings.navigation.jump_attempts),
            ),
            (
                HANDLERS,
                UbSettingValue::from_collection(&mosstank_meta::events::lines(self.meta.profile())),
            ),
        ];
        let mut alias_can_save = None;
        if files {
            let selected = self
                .core
                .meta_profiles
                .selected(self.client.stores())
                .unwrap_or_default();
            values.push((
                HANDLER_PROFILE,
                UbSettingValue::from_text(&shown_meta_profile(selected)),
            ));
            let key = self.alias_file_key();
            let mut log = RecordingLog::default();
            let lines = {
                let storage = self.client.stores().storage(Root::Profiles);
                self.ub
                    .store
                    .borrow_mut()
                    .read_lines(storage, &mut log, key.as_deref())
            };
            forward_log(self.client, &log);
            values.push((ALIASES, UbSettingValue::from_collection(&lines)));
            alias_can_save =
                Some(key.is_some() && self.client.stores().storage(Root::Profiles).is_available());
        }
        let live = self.ub.catalog.values().1.clone();
        let mut live = live.borrow_mut();
        for (name, value) in values {
            live.values.insert(name, value);
        }
        if let Some(can_save) = alias_can_save {
            live.alias_can_save = can_save;
        }
    }

    /// The alias file the `Aliases.Profile` row names (`AliasFileKey`).
    fn alias_file_key(&self) -> Option<String> {
        let profile = mosstank_ub::catalog::UbCatalog::value(&self.ub.catalog, "Aliases.Profile");
        self.ub.store.borrow().alias_file_key(Some(&profile.text))
    }

    /// Hands the writes made to the live rows during the call to their
    /// owners, as the C# bindings' setters do at once (register UB-5).
    pub(crate) fn apply_live_ub_writes(&mut self) {
        let written = std::mem::take(&mut self.ub.catalog.values().1.borrow_mut().written);
        if written.is_empty() {
            return;
        }
        let mut refresh_page = false;
        for (name, value) in written {
            match name {
                HANDLERS => {
                    self.apply_handler_lines(&value.items);
                    refresh_page = true;
                }
                // Picked by the call's catalogue; the engine takes it now.
                HANDLER_PROFILE => self.load_picked_meta_profile(),
                // Opened by the call's catalogue; the page shows its values.
                SETTINGS_PROFILE => refresh_page = true,
                _ => {
                    let settings = &mut self.core.settings;
                    match name {
                        "ItemGiver.Range" => {
                            settings.inventory.give_range_meters = value.as_double();
                        }
                        "ItemGiver.Delay" => {
                            settings.inventory.give_delay_seconds = value.as_double();
                        }
                        "ItemGiver.BusyRetryLimit" => {
                            settings.inventory.give_busy_retry_limit = value.as_int32();
                        }
                        "ItemGiver.FailureLimit" => {
                            settings.inventory.give_failure_limit = value.as_int32();
                        }
                        _ => settings.navigation.jump_attempts = value.as_int32(),
                    }
                    self.core.save_profile(self.client);
                }
            }
        }
        self.refresh_live_ub_rows(true);
        if refresh_page {
            self.ub.page.refresh(&self.ub.catalog);
        }
    }

    /// The handlers row's write: the lines become the meta profile's
    /// handler rules, a line that cannot be read is said in chat and
    /// dropped, and the meta profile is saved.
    fn apply_handler_lines(&mut self, lines: &[String]) {
        let mut profile = self.meta.profile().clone();
        let rejected = mosstank_meta::events::apply(&mut profile, lines);
        // The engine's own rules, edited in place: the handler rules out,
        // the lines' rules in at the end, every other rule where it was.
        for index in (0..self.meta.profile().rules.len()).rev() {
            if mosstank_meta::events::is_handler(&self.meta.profile().rules[index]) {
                self.meta.remove_rule(index);
            }
        }
        let kept = self.meta.profile().rules.len();
        for rule in profile.rules.into_iter().skip(kept) {
            self.meta.add_rule(rule);
        }
        for line in rejected {
            super::post_chat(self.client.rules(), &line);
        }
        self.save_meta_profile();
        let count = self.meta.profile().rules.len();
        self.core.panel.meta.selected =
            super::panel::clamp_row(self.core.panel.meta.selected, count);
        self.refresh_meta_editor();
    }
}

/// Runs one line typed under `/ub` (`ExecuteUbCommand`).
pub(crate) fn execute(cx: &mut TankCx<'_>, input: &str) {
    cx.with_ub(|commands, call| commands.execute(call, input));
}

/// The expression runtime for a UB call made from inside an expression
/// (`uboptget`, `uboptset`), which never runs one of its own.
#[derive(Default)]
struct NoExpressions {
    state: ExpressionState,
    held: HeldMotions,
}

impl UbExpressions for NoExpressions {
    fn evaluate(
        &mut self,
        _host: &mut dyn MetaHost,
        _source: &str,
    ) -> Result<ExpressionValue, ExpressionError> {
        Ok(ExpressionValue::zero())
    }

    fn display(&mut self, _host: &mut dyn MetaHost, value: &ExpressionValue) -> String {
        value.to_display_string()
    }

    fn state(&mut self) -> &mut ExpressionState {
        &mut self.state
    }

    fn held_motions(&mut self) -> &mut HeldMotions {
        &mut self.held
    }

    fn refresh_quests(&mut self, _host: &mut dyn MetaHost) {}
}

impl TankServices<'_, '_> {
    /// `uboptget[setting]` and `uboptset[setting,value]` (the panel's
    /// `GetUbOption` and `SetUbOption`, which the UB tab carries).
    pub(crate) fn ub_option(
        &mut self,
        function: &str,
        arguments: &[ExpressionValue],
    ) -> Result<ExpressionValue, mosstank_expr::Fault> {
        let name = arguments[0].as_str(function)?.to_owned();
        let locks = self.core.0.borrow().locks.clone();
        let mut catalog =
            UbSettingCatalog::new(self.ub_values.clone(), owned_rows(&self.ub_values));
        let mut proxy = ClientProxy::new(self.client);
        let mut expressions = NoExpressions::default();
        let mut services = ShellUbServices {
            core: self.core,
            client: self.client,
            reader: self.ub.clone(),
            meta_enabled: self.meta_enabled,
            meta_written: None,
        };
        let value = {
            let mut catalog = CallCatalog {
                catalog: &mut catalog,
                reach: OwnerReach::Shared(self.client, self.core),
            };
            let mut call = UbCall {
                host: &mut proxy,
                settings: &mut catalog,
                expressions: &mut expressions,
                services: &mut services,
                locks: &locks,
            };
            if function == "uboptget" {
                mosstank_ub::commands::opt::ub_option_get(&mut call, &name)
            } else {
                ExpressionValue::Boolean(mosstank_ub::commands::opt::ub_option_set(
                    &mut call,
                    &name,
                    &arguments[1],
                ))
            }
        };
        if let Some(enabled) = services.meta_written {
            self.meta_enabled = enabled;
            self.requests
                .push(super::services::EngineRequest::MetaEnabled(enabled));
        }
        Ok(value)
    }
}

/// A profile's rules when it loaded; a storage failure is said in the log
/// and reads as no file, as the C# store's catch does.
fn loaded_rules(
    client: &mut dyn TankClient,
    loaded: Result<bool, mosstank_stores::storage::StorageError>,
    rules: Vec<LootRule>,
) -> Option<Vec<LootRule>> {
    match loaded {
        Ok(true) => Some(rules),
        Ok(false) => None,
        Err(error) => {
            client.meta().log(
                mosstank_meta::host::LogLevel::Warn,
                &format!("A loot profile could not be read: {error}"),
            );
            None
        }
    }
}

impl TankServices<'_, '_> {
    /// Runs `f` with one call into the UB tab from inside an expression
    /// (`vtcountprofile`, `actiontrygiveprofile`), which never runs one of
    /// its own.
    pub(crate) fn with_ub_call<R>(&mut self, f: impl FnOnce(&mut UbCall<'_>) -> R) -> R {
        let locks = self.core.0.borrow().locks.clone();
        let mut catalog =
            UbSettingCatalog::new(self.ub_values.clone(), owned_rows(&self.ub_values));
        let mut proxy = ClientProxy::new(self.client);
        let mut expressions = NoExpressions::default();
        let mut services = ShellUbServices {
            core: self.core,
            client: self.client,
            reader: self.ub.clone(),
            meta_enabled: self.meta_enabled,
            meta_written: None,
        };
        let mut catalog = CallCatalog {
            catalog: &mut catalog,
            reach: OwnerReach::Shared(self.client, self.core),
        };
        let mut call = UbCall {
            host: &mut proxy,
            settings: &mut catalog,
            expressions: &mut expressions,
            services: &mut services,
            locks: &locks,
        };
        f(&mut call)
    }
}

impl TankCx<'_> {
    /// Installs the aliases' interceptor (`InitializeAliases`); a client
    /// that refuses it is said in the log, and typed lines then pass.
    pub(crate) fn attach_aliases(&mut self) {
        let mut aliases = std::mem::take(&mut self.ub.aliases);
        let attached = self.with_ub(|_, call| aliases.attach(call.host));
        self.ub.aliases = aliases;
        if let Err(error) = attached {
            self.client.rules().log_warn(&format!(
                "MossTank alias interceptor was not registered: {error}"
            ));
        }
    }

    /// The interceptor's handle, once attached.
    pub(crate) fn alias_registration(&self) -> Option<u32> {
        self.ub.aliases.registration()
    }

    /// A typed line the client asks the aliases about, against the switch
    /// and the list the settings hold now.
    pub(crate) fn intercept_alias(&mut self, line: &str) -> InterceptDecision {
        let mut aliases = std::mem::take(&mut self.ub.aliases);
        let decision = self.with_ub(|_, call| {
            let enabled = call.settings.flag("Aliases.Enabled");
            let lines = call.settings.value("Aliases.DefinedAliases").items;
            aliases.intercept(call.host, call.expressions, enabled, &lines, line)
        });
        self.ub.aliases = aliases;
        decision
    }

    /// Removes the aliases' interceptor (`DisposeAliases`).
    pub(crate) fn dispose_aliases(&mut self) {
        let mut aliases = std::mem::take(&mut self.ub.aliases);
        self.with_ub(|_, call| aliases.dispose(call.host));
        self.ub.aliases = aliases;
    }
}

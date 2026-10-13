//! The UB tab's wiring (`MossTankPanel.UbSettings` and the panel's side of
//! `MossTankCommands.Ub`): the UB settings store and catalogue, the `/ub`
//! word through the UB tab's dispatcher, the UB commands' frame, and the
//! rest of the plugin as the UB tab reaches it ([`UbServices`]).
//!
//! The settings store is read when the shell starts and bound to whoever
//! is in the world; a changed tier is written at the end of every call
//! (register STORE-1). The catalogue's live rows the shell binds are the
//! two whose choice is fixed; the rows other owners keep (the open UB
//! profile, the alias list, the item giver's numbers, the jump attempts,
//! the event handlers) read from the settings files until those owners are
//! converted, and the UB tools the UB tab does not carry yet answer that.

use std::cell::RefCell;
use std::rc::Rc;

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
use mosstank_ub::catalog::{CatalogDisplays, UbCatalog, message_display};
use mosstank_ub::commands::{UbCall, UbCommands, UbServices};
use mosstank_ub::expressions::UbExpressions;
use mosstank_ub::host::UbHost;

use super::services::{ClientCell, ClientProxy, CoreCell, TankServices};
use super::{Core, TankCx};
use crate::client::TankClient;

/// The UB settings store, shared by the catalogues that read it.
#[derive(Clone)]
pub(crate) struct StoreValues(Rc<RefCell<UbSettingStore>>);

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

/// The rows whose choice is already made here (`UbLiveOwners`, that part).
fn fixed_rows() -> Vec<(String, UbSettingBinding)> {
    vec![
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
        let values = StoreValues(store.clone());
        let mut catalog = UbSettingCatalog::new(values.clone(), fixed_rows());
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
            reader: Rc::new(UbSettingCatalog::new(values, fixed_rows())),
            store,
            commands: UbCommands::new(),
            page,
            landscape,
            dungeon,
            hud,
            canvases: crate::canvases::Canvases::default(),
            tags,
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
        StoreValues(self.store.clone())
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

/// A UB tool the UB tab does not carry yet.
fn not_converted(name: &str) -> String {
    format!("{name} is not in the Rust conversion of MossTank yet.")
}

impl ShellUbServices<'_, '_> {
    /// A plain UB line, in the generic line's display.
    fn write_ub(&self, text: &str) {
        let mut client = self.client.0.borrow_mut();
        mosstank_ub::chat::post(
            client.meta(),
            &CatalogDisplays(&*self.reader),
            &mosstank_ub::chat::line(text),
        );
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
                openac_plugin_compat::text::ordinal::equals_ignore_case(canonical, "EnableMeta")
            })
        {
            self.meta_written = Some(value.is_truthy());
        }
        written
    }

    fn vendor_command(&mut self, _host: &mut dyn UbHost, _arguments: &str) -> Option<Vec<String>> {
        Some(vec![not_converted("The vendor tool")])
    }

    fn auto_vendor_start(&mut self, _host: &mut dyn UbHost, _profile: Option<&str>) {
        self.write_ub(&not_converted("AutoVendor"));
    }

    fn auto_vendor_stop(&mut self, _host: &mut dyn UbHost) {
        self.write_ub(&not_converted("AutoVendor"));
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
            let mut call = UbCall {
                host: &mut proxy,
                settings: &mut self.ub.catalog,
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
        let mut catalog = UbSettingCatalog::new(self.ub_values.clone(), fixed_rows());
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
        let mut catalog = UbSettingCatalog::new(self.ub_values.clone(), fixed_rows());
        let mut proxy = ClientProxy::new(self.client);
        let mut expressions = NoExpressions::default();
        let mut services = ShellUbServices {
            core: self.core,
            client: self.client,
            reader: self.ub.clone(),
            meta_enabled: self.meta_enabled,
            meta_written: None,
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

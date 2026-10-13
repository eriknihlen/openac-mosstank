//! The C# panel tests' rig for the `/ub` commands: a client in memory, the
//! UB settings catalogue, an expression runtime, the rest of the plugin as
//! recording fakes, and the shared action locks.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use ac_plugin_abi::navigation::NavigationSnapshot;
use ac_plugin_abi::world::{ObjectClass, Position, WorldObject};
use mosstank_automation::locks::ActionLockTable;
use mosstank_expr::{ExpressionValue, FixedClock};
use mosstank_meta::runtime::ExpressionRuntime;
use mosstank_meta::services::NoServices;
use mosstank_settings::ub::{
    UbSettingBinding, UbSettingCatalog, UbSettingValue, UbSettingValueBag,
};
use mosstank_ub::commands::{UbCall, UbCommands, UbServices};
use mosstank_ub::expressions::RuntimeExpressions;
use mosstank_ub::host::UbHost;
use mosstank_ub::testing::FakeUbHost;
use openac_plugin_compat::text::ordinal;

/// The plugin's version the tests see.
pub const PLUGIN_VERSION: &str = "0.6.0-test";

/// 2026-10-13 00:00 UTC.
pub const NOW_UTC_MS: i64 = 1_791_849_600_000;

/// The rest of the plugin, recording what it was asked.
#[derive(Default)]
pub struct Services {
    pub vtank_options: HashMap<String, ExpressionValue>,
    pub vendor_lines: Vec<String>,
    pub vendor_reply: Option<Vec<String>>,
    pub xp_reply: Option<Vec<String>>,
    pub equip_reply: Option<Vec<String>>,
    pub auto_vendor: Vec<Option<String>>,
    pub auto_vendor_stops: usize,
    pub fellow_lines: Vec<String>,
    pub tool_verbs: Vec<(String, String)>,
    pub follows: Vec<(u32, String)>,
    pub follow_result: bool,
    pub navigation_enabled: bool,
    pub jump_attempts: Rc<Cell<i32>>,
    pub translations: Vec<(String, String, f64, f64, bool)>,
    pub translate_result: Option<Result<(String, i32), String>>,
    pub monotonic: f64,
    pub offset_minutes: i32,
}

impl UbServices for Services {
    fn plugin_version(&self) -> String {
        PLUGIN_VERSION.to_owned()
    }

    fn now_utc_ms(&mut self) -> i64 {
        NOW_UTC_MS
    }

    fn local_offset_minutes(&mut self) -> i32 {
        self.offset_minutes
    }

    fn monotonic_ms(&mut self) -> f64 {
        self.monotonic += 0.25;
        self.monotonic
    }

    fn is_vtank_option(&mut self, name: &str) -> bool {
        self.vtank_options
            .keys()
            .any(|known| ordinal::equals_ignore_case(known, name))
    }

    fn get_meta_option(&mut self, _host: &mut dyn UbHost, name: &str) -> ExpressionValue {
        self.vtank_options
            .iter()
            .find(|(known, _)| ordinal::equals_ignore_case(known, name))
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    }

    fn set_meta_option(
        &mut self,
        _host: &mut dyn UbHost,
        name: &str,
        value: &ExpressionValue,
    ) -> bool {
        self.vtank_options.insert(name.to_owned(), value.clone());
        true
    }

    fn vendor_command(&mut self, _host: &mut dyn UbHost, arguments: &str) -> Option<Vec<String>> {
        self.vendor_lines.push(arguments.to_owned());
        self.vendor_reply.clone()
    }

    fn auto_vendor_start(&mut self, _host: &mut dyn UbHost, profile: Option<&str>) {
        self.auto_vendor.push(profile.map(str::to_owned));
    }

    fn auto_vendor_stop(&mut self, _host: &mut dyn UbHost) {
        self.auto_vendor_stops += 1;
    }

    fn fellow_command(&mut self, _host: &mut dyn UbHost, arguments: &str) {
        self.fellow_lines.push(arguments.to_owned());
    }

    fn xp_command(&mut self, _host: &mut dyn UbHost, _arguments: &str) -> Option<Vec<String>> {
        self.xp_reply.clone()
    }

    fn equip_command(&mut self, _host: &mut dyn UbHost, _arguments: &str) -> Option<Vec<String>> {
        self.equip_reply.clone()
    }

    fn run_tool_verb(&mut self, _host: &mut dyn UbHost, verb: &str, arguments: &str) {
        self.tool_verbs
            .push((verb.to_owned(), arguments.to_owned()));
    }

    fn follow(&mut self, _host: &mut dyn UbHost, player_id: u32, name: &str) -> bool {
        self.follows.push((player_id, name.to_owned()));
        self.follow_result
    }

    fn navigation_enabled(&mut self) -> bool {
        self.navigation_enabled
    }

    fn jump_attempts(&mut self) -> i32 {
        self.jump_attempts.get()
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
        self.translations.push((
            from.to_owned(),
            to.to_owned(),
            east_west,
            north_south,
            force,
        ));
        self.translate_result
            .clone()
            .unwrap_or_else(|| Err(format!("Could not find route to load: {from}")))
    }
}

/// The values of the rows the panel binds to its own running settings.
#[derive(Debug)]
pub struct LiveRows {
    pub give_range: Rc<Cell<f64>>,
    pub give_delay: Rc<Cell<f64>>,
    pub give_busy_retry_limit: Rc<Cell<i32>>,
    pub give_failure_limit: Rc<Cell<i32>>,
    pub jump_attempts: Rc<Cell<i32>>,
    pub profile: Rc<RefCell<String>>,
}

impl LiveRows {
    fn new() -> Self {
        Self {
            give_range: Rc::new(Cell::new(15.0)),
            give_delay: Rc::new(Cell::new(0.5)),
            give_busy_retry_limit: Rc::new(Cell::new(10)),
            give_failure_limit: Rc::new(Cell::new(3)),
            jump_attempts: Rc::new(Cell::new(3)),
            profile: Rc::new(RefCell::new("default".to_owned())),
        }
    }

    /// The panel's `UbLiveOwners`, for the rows these tests reach.
    fn bindings(&self) -> Vec<(String, UbSettingBinding)> {
        fn decimal(cell: &Rc<Cell<f64>>, low: f64, high: f64) -> UbSettingBinding {
            let read = cell.clone();
            let write = cell.clone();
            UbSettingBinding::new(
                move || UbSettingValue::from_double(read.get()),
                move |value| write.set(value.as_double().clamp(low, high)),
            )
        }
        fn whole(cell: &Rc<Cell<i32>>, low: i32, high: i32) -> UbSettingBinding {
            let read = cell.clone();
            let write = cell.clone();
            UbSettingBinding::new(
                move || UbSettingValue::from_int(read.get()),
                move |value| write.set(value.as_int32().clamp(low, high)),
            )
        }
        let profile_read = self.profile.clone();
        let profile_write = self.profile.clone();
        vec![
            (
                "ItemGiver.Range".to_owned(),
                decimal(&self.give_range, 0.0, 1000.0),
            ),
            (
                "ItemGiver.Delay".to_owned(),
                decimal(&self.give_delay, 0.0, 60.0),
            ),
            (
                "ItemGiver.BusyRetryLimit".to_owned(),
                whole(&self.give_busy_retry_limit, 1, 100),
            ),
            (
                "ItemGiver.FailureLimit".to_owned(),
                whole(&self.give_failure_limit, 0, 100),
            ),
            (
                "Jumper.Attempts".to_owned(),
                whole(&self.jump_attempts, 1, 20),
            ),
            (
                "VTank.PatchExpressionEngine".to_owned(),
                UbSettingBinding::new(|| UbSettingValue::from_bool(true), |_| {}),
            ),
            (
                "InventoryManager.TreatStackAsSingleItem".to_owned(),
                UbSettingBinding::new(|| UbSettingValue::from_bool(false), |_| {}),
            ),
            (
                "Plugin.SettingsProfile".to_owned(),
                UbSettingBinding::new(
                    move || UbSettingValue::from_text(&profile_read.borrow()),
                    move |value| *profile_write.borrow_mut() = value.text.trim().to_owned(),
                ),
            ),
        ]
    }
}

/// The rig.
pub struct Rig {
    pub host: FakeUbHost,
    pub catalog: UbSettingCatalog<UbSettingValueBag>,
    pub runtime: ExpressionRuntime,
    pub services: Services,
    pub locks: ActionLockTable,
    pub commands: UbCommands,
    pub live: LiveRows,
}

impl Default for Rig {
    fn default() -> Self {
        Self::new()
    }
}

impl Rig {
    /// "Test Character" in the world, standing at heading 0 when
    /// [`Rig::at_heading`] is called.
    pub fn new() -> Self {
        let mut host = FakeUbHost::new();
        "Test Character".clone_into(&mut host.meta.character_mut().name);
        let runtime = ExpressionRuntime::new(&mut host, Some(1234), Rc::new(clock()));
        let live = LiveRows::new();
        let catalog = UbSettingCatalog::new(UbSettingValueBag::default(), live.bindings());
        let services = Services {
            follow_result: true,
            jump_attempts: live.jump_attempts.clone(),
            ..Services::default()
        };
        Self {
            host,
            catalog,
            runtime,
            services,
            locks: ActionLockTable::new(),
            commands: UbCommands::new(),
            live,
        }
    }

    /// The character stands in the world facing `heading`.
    pub fn at_heading(mut self, heading: f32) -> Self {
        self.host.meta.navigation = navigation_at(heading);
        self
    }

    /// Runs `f` with one call into the UB tab.
    pub fn with_call<R>(&mut self, f: impl FnOnce(&mut UbCall<'_>, &mut UbCommands) -> R) -> R {
        let mut meta_services = NoServices;
        let mut expressions = RuntimeExpressions {
            runtime: &mut self.runtime,
            services: &mut meta_services,
        };
        let mut call = UbCall {
            host: &mut self.host,
            settings: &mut self.catalog,
            expressions: &mut expressions,
            services: &mut self.services,
            locks: &mut self.locks,
        };
        f(&mut call, &mut self.commands)
    }

    /// `/ub <line>`.
    pub fn ub(&mut self, line: &str) {
        self.with_call(|call, commands| commands.execute(call, line));
    }

    /// One frame of the panel: the locks' clock, then the commands.
    pub fn tick(&mut self, seconds: f64) -> bool {
        self.locks.advance(seconds);
        self.with_call(|call, commands| commands.on_tick(call, seconds))
    }

    /// An expression, as the panel's `EvaluateExpression`.
    pub fn evaluate(&mut self, source: &str) -> ExpressionValue {
        let mut meta_services = NoServices;
        self.runtime
            .evaluate(&mut self.host, &mut meta_services, source)
            .expect("the expression runs")
    }

    /// Every line shown in chat (`automation.Messages`).
    pub fn messages(&self) -> Vec<String> {
        self.host.posted_text()
    }

    /// Every line shown, with its class (`automation.Posted`).
    pub fn posted(&self) -> Vec<(String, u32)> {
        self.host.meta.posted.clone()
    }

    /// Forgets what was shown.
    pub fn clear_messages(&mut self) {
        self.host.meta.posted.clear();
    }

    /// Every line submitted.
    pub fn submitted(&self) -> Vec<String> {
        self.host.meta.submitted.clone()
    }

    /// The single line shown.
    pub fn single_message(&self) -> String {
        let messages = self.messages();
        assert_eq!(1, messages.len(), "{messages:?}");
        messages[0].clone()
    }
}

/// The clock the tests run on: 2026-10-13 00:00 UTC, standing still.
pub fn clock() -> FixedClock {
    let clock = FixedClock::default();
    clock.utc_ms.set(NOW_UTC_MS);
    clock
}

/// The character in the world at a heading (`NavigationAt`).
pub fn navigation_at(heading: f32) -> NavigationSnapshot {
    NavigationSnapshot {
        is_available: true,
        is_portal_space: false,
        local_object_id: 1,
        position: Position {
            cell_id: 0x0001_0001,
            heading_degrees: heading,
            is_outdoor: true,
            ..Position::default()
        },
        ..NavigationSnapshot::default()
    }
}

/// A player standing at the origin (`Player`).
pub fn player(object_id: u32, name: &str) -> WorldObject {
    landscape(object_id, name, ObjectClass::Player, 0.0)
}

/// Something standing in the world, `east_west` map units east
/// (`Landscape`).
pub fn landscape(
    object_id: u32,
    name: &str,
    object_class: ObjectClass,
    east_west: f64,
) -> WorldObject {
    WorldObject {
        object_id,
        name: name.to_owned(),
        object_class,
        is_landscape: true,
        position: Some(Position {
            cell_id: 0x0001_0001,
            east_west,
            is_outdoor: true,
            ..Position::default()
        }),
        ..WorldObject::default()
    }
}

/// Something the character carries (`Carried`).
pub fn carried(object_id: u32, name: &str) -> WorldObject {
    WorldObject {
        object_id,
        name: name.to_owned(),
        object_class: ObjectClass::SpellComponent,
        is_owned: true,
        container_id: 1,
        has_appraisal_data: true,
        ..WorldObject::default()
    }
}

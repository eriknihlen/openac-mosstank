//! What the meta tests share: the C# tests' hosts and services, and their
//! `Evaluate` helpers.

#![allow(dead_code)]

use std::rc::Rc;

use mosstank_expr::{ExpressionError, ExpressionValue, Fault, FixedClock};
use mosstank_formats::meta::{
    DEFAULT_STATE, MetaAction, MetaActionKind, MetaCondition, MetaConditionKind, MetaProfile,
    MetaRule,
};
use mosstank_meta::engine::MetaEngine;
use mosstank_meta::host::{ChatLine, LogLevel, MetaHost};
use mosstank_meta::runtime::ExpressionRuntime;
use mosstank_meta::services::{MetaServices, PeerNeed};
use mosstank_meta::testing::FakeHost;

/// The clock the tests run on: 2026-10-13 00:00 UTC, standing still (the
/// C# tests read the machine's clock; their quest times are years before).
pub fn clock() -> Rc<FixedClock> {
    let clock = FixedClock::default();
    clock.utc_ms.set(1_791_849_600_000);
    Rc::new(clock)
}

/// A runtime on `host` (the C#'s `new MossTankExpressionRuntime(host)`).
pub fn runtime(host: &mut dyn MetaHost) -> ExpressionRuntime {
    ExpressionRuntime::new(host, Some(1234), clock())
}

/// An engine for `profile` with its own runtime.
pub fn engine(host: &mut dyn MetaHost, profile: MetaProfile) -> MetaEngine {
    let expressions = runtime(host);
    MetaEngine::new(host, expressions, profile)
}

/// The C# tests' `Rule(condition, action, text, state)`.
pub fn rule(condition: MetaConditionKind, action: MetaActionKind, text: &str) -> MetaRule {
    rule_in(condition, action, text, DEFAULT_STATE)
}

/// [`rule`] in a state.
pub fn rule_in(
    condition: MetaConditionKind,
    action: MetaActionKind,
    text: &str,
    state: &str,
) -> MetaRule {
    MetaRule::new(
        state,
        MetaCondition::of(condition),
        MetaAction {
            kind: action,
            text: text.to_owned(),
            ..MetaAction::default()
        },
    )
}

/// A profile of rules.
pub fn profile(rules: Vec<MetaRule>) -> MetaProfile {
    MetaProfile { rules }
}

/// An action of a kind with a text.
pub fn action(kind: MetaActionKind, text: &str) -> MetaAction {
    MetaAction {
        kind,
        text: text.to_owned(),
        ..MetaAction::default()
    }
}

type SetOption = Box<dyn FnMut(&str, &ExpressionValue) -> Result<bool, Fault>>;
type GetOption = Box<dyn FnMut(&str) -> ExpressionValue>;
type OnSubmit = Box<dyn FnMut(&str) -> Option<MetaProfile>>;

/// The C#'s `MetaServices` and `ExpressionHostPolicy` with what a test
/// sets.
#[derive(Default)]
pub struct TestServices {
    pub set_option: Option<SetOption>,
    pub get_option: Option<GetOption>,
    pub handlers_enabled: Option<bool>,
    pub on_submit: Option<OnSubmit>,
    pub state_changes: Vec<String>,
    pub margins: Option<(i32, i32)>,
    pub casts: Option<Vec<(u32, Option<u32>)>>,
    pub peers_required: Vec<PeerNeed>,
    pub debug: bool,
}

impl MetaServices for TestServices {
    fn submit_chat(&mut self, host: &mut dyn MetaHost, line: &str) -> Option<MetaProfile> {
        host.chat_submit(line);
        self.on_submit.as_mut().and_then(|submit| submit(line))
    }

    fn get_option(&mut self, _host: &mut dyn MetaHost, name: &str) -> ExpressionValue {
        self.get_option
            .as_mut()
            .map_or_else(ExpressionValue::zero, |get| get(name))
    }

    fn set_option(
        &mut self,
        _host: &mut dyn MetaHost,
        name: &str,
        value: &ExpressionValue,
    ) -> Result<bool, Fault> {
        match self.set_option.as_mut() {
            Some(set) => set(name, value),
            None => Ok(false),
        }
    }

    fn game_event_handlers_enabled(&mut self) -> bool {
        self.handlers_enabled.unwrap_or(true)
    }

    fn state_changed(&mut self, _host: &mut dyn MetaHost, message: &str, _state: &str) {
        self.state_changes.push(message.to_owned());
    }

    fn require_peers(&mut self, _host: &mut dyn MetaHost, need: PeerNeed) {
        self.peers_required.push(need);
    }

    fn skill_margin(&mut self, hunting: bool) -> i32 {
        self.margins
            .map_or(0, |(hunt, buff)| if hunting { hunt } else { buff })
    }

    fn begin_cast(&mut self, _host: &mut dyn MetaHost, spell_id: u32, target: Option<u32>) -> bool {
        match self.casts.as_mut() {
            Some(casts) => {
                casts.push((spell_id, target));
                true
            }
            None => false,
        }
    }

    fn debug(&mut self) -> bool {
        self.debug
    }
}

/// Runs an expression in the engine; panics on an error.
pub fn value(
    engine: &mut MetaEngine,
    host: &mut FakeHost,
    services: &mut TestServices,
    source: &str,
) -> ExpressionValue {
    engine
        .evaluate(host, services, source)
        .unwrap_or_else(|error| panic!("{source}: {error}"))
}

/// The number an expression answers in the engine.
pub fn number(
    engine: &mut MetaEngine,
    host: &mut FakeHost,
    services: &mut TestServices,
    source: &str,
) -> f64 {
    value(engine, host, services, source)
        .as_number("")
        .unwrap_or_else(|fault| panic!("{source}: {}", fault.message()))
}

/// The string an expression answers in the engine.
pub fn string(
    engine: &mut MetaEngine,
    host: &mut FakeHost,
    services: &mut TestServices,
    source: &str,
) -> String {
    value(engine, host, services, source)
        .as_str("")
        .unwrap_or_else(|fault| panic!("{source}: {}", fault.message()))
        .to_owned()
}

/// The runtime's `Evaluate`, with no services.
pub fn eval(
    runtime: &mut ExpressionRuntime,
    host: &mut FakeHost,
    source: &str,
) -> Result<ExpressionValue, ExpressionError> {
    runtime.evaluate(host, &mut TestServices::default(), source)
}

/// The number an expression answers in a runtime.
pub fn eval_number(runtime: &mut ExpressionRuntime, host: &mut FakeHost, source: &str) -> f64 {
    eval(runtime, host, source)
        .unwrap_or_else(|error| panic!("{source}: {error}"))
        .as_number("")
        .unwrap_or_else(|fault| panic!("{source}: {}", fault.message()))
}

/// The string an expression answers in a runtime.
pub fn eval_string(runtime: &mut ExpressionRuntime, host: &mut FakeHost, source: &str) -> String {
    eval(runtime, host, source)
        .unwrap_or_else(|error| panic!("{source}: {error}"))
        .as_str("")
        .unwrap_or_else(|fault| panic!("{source}: {}", fault.message()))
        .to_owned()
}

/// Whether an expression answers true in a runtime.
pub fn eval_truthy(runtime: &mut ExpressionRuntime, host: &mut FakeHost, source: &str) -> bool {
    eval(runtime, host, source)
        .unwrap_or_else(|error| panic!("{source}: {error}"))
        .is_truthy()
}

/// What an expression prints as in a runtime.
pub fn eval_display(runtime: &mut ExpressionRuntime, host: &mut FakeHost, source: &str) -> String {
    let value = eval(runtime, host, source).unwrap_or_else(|error| panic!("{source}: {error}"));
    runtime.display(host, &value)
}

/// The warnings logged.
pub fn warnings(host: &FakeHost) -> Vec<String> {
    host.logged(LogLevel::Warn)
}

/// A line as the host hands it over: its parts, its chat text type, its
/// channel number and the host's own wording of the whole line.
#[allow(clippy::too_many_arguments)]
pub fn worded(
    sender_id: u32,
    kind: u32,
    sender: &str,
    text: &str,
    channel: &str,
    log_text_type: u32,
    display_text: &str,
    channel_id: u32,
) -> ChatLine {
    ChatLine {
        sequence: 1,
        sender_id,
        kind,
        sender: sender.to_owned(),
        text: text.to_owned(),
        channel: channel.to_owned(),
        log_text_type,
        combat_kind: 0,
        received_unix_ms: 0,
        channel_id,
        display_text: display_text.to_owned(),
    }
}

use ac_plugin_abi::character::{Attribute, Skill, Training};
use ac_plugin_abi::magic::SpellInfo;
use ac_plugin_abi::world::{ItemProperties, ObjectClass, Position, WorldObject};

/// The C#'s `new PluginWorldObject(id, weenieClassId, name, objectClass,
/// itemType, containerId, wielderId)`.
pub fn object(
    id: u32,
    weenie_class_id: u32,
    name: &str,
    object_class: ObjectClass,
    item_type: u32,
    container_id: u32,
    wielder_id: u32,
) -> WorldObject {
    WorldObject {
        object_id: id,
        weenie_class_id,
        name: name.to_owned(),
        object_class,
        item_type,
        container_id,
        wielder_id,
        stack_size: 1,
        ..WorldObject::default()
    }
}

/// The C#'s `Properties(ints, int64s, strings)`.
pub fn properties(
    ints: &[(u32, i32)],
    int64s: &[(u32, i64)],
    strings: &[(u32, &str)],
) -> ItemProperties {
    ItemProperties {
        ints: ints.to_vec(),
        int64s: int64s.to_vec(),
        strings: strings
            .iter()
            .map(|(key, text)| (*key, (*text).to_owned()))
            .collect(),
        ..ItemProperties::default()
    }
}

/// The C#'s `Spell(id, school, difficulty)`.
pub fn spell(id: u32, school: u32, difficulty: i32) -> SpellInfo {
    SpellInfo {
        spell_id: id,
        name: format!("Spell {id}"),
        family: 1,
        tier: 1,
        difficulty,
        mana_cost: 10,
        duration_seconds: 30.0,
        school,
        is_beneficial: true,
        ..SpellInfo::default()
    }
}

/// The position the C#'s `CreateAutomation` stands at.
pub fn automation_position() -> Position {
    Position {
        cell_id: 0x7F7F_0001,
        east_west: 10.0,
        north_south: 20.0,
        elevation: 3.0,
        heading_degrees: 90.0,
        is_outdoor: true,
    }
}

/// The C#'s `CreateAutomation()`: "Expression Tester" (id 1) with a
/// health elixir, a small pack and a drudge nearby.
pub fn create_automation() -> FakeHost {
    let mut host = FakeHost::new();
    {
        let character = host.character_mut();
        "Expression Tester".clone_into(&mut character.name);
        character.character_index = 2;
        character.health = (90, 100, 80);
        character.stamina = (80, 100, 70);
        character.mana = (70, 100, 60);
    }
    let position = automation_position();
    host.navigation.position = position.clone();
    host.attributes = vec![Attribute {
        kind: 0,
        name: "Strength".to_owned(),
        current: 110,
        base: 100,
        stat_id: 1,
        ..Attribute::default()
    }];
    host.skills = vec![Skill {
        skill_id: 34,
        name: "War Magic".to_owned(),
        training: Training::Trained,
        current: 300,
        base: 275,
        ..Skill::default()
    }];
    host.known_spell_ids.insert(1001);
    host.login.is_available = true;
    let mut player = object(1, 1, "Expression Tester", ObjectClass::Player, 0x10, 0, 0);
    player.is_landscape = true;
    player.position = Some(position.clone());
    player.items_capacity = 10;
    player.containers_capacity = 2;
    host.objects.push(player);
    let mut elixir = object(10, 100, "Health Elixir", ObjectClass::Food, 0x20, 1, 0);
    elixir.is_owned = true;
    elixir.stack_size = 10;
    host.objects.push(elixir);
    let mut pack = object(11, 101, "Small Pack", ObjectClass::Container, 0x200, 1, 0);
    pack.is_owned = true;
    pack.items_capacity = 8;
    host.objects.push(pack);
    let mut drudge = object(20, 200, "Drudge", ObjectClass::Monster, 0x10, 0, 0);
    drudge.is_landscape = true;
    drudge.position = Some(Position {
        east_west: 10.1,
        ..position
    });
    drudge.has_appraisal_data = true;
    drudge.last_id_time = 54321;
    host.objects.push(drudge);
    host.properties.insert(
        1,
        properties(&[(5, 50), (96, 100)], &[], &[(1, "Expression Tester")]),
    );
    host.properties
        .insert(20, properties(&[(25, 77)], &[], &[]));
    host
}

/// Marks the client busy the way the C#'s fake is: items and equipment.
pub fn set_busy(host: &mut FakeHost, busy: bool) {
    host.item_state.is_busy = busy;
    host.equipment_busy = busy;
}

/// An object, to change.
pub fn object_mut(host: &mut FakeHost, id: u32) -> &mut WorldObject {
    host.objects
        .iter_mut()
        .find(|object| object.object_id == id)
        .expect("the object is there")
}

/// The reason an expression's failure gives.
pub fn eval_error(
    runtime: &mut ExpressionRuntime,
    host: &mut FakeHost,
    source: &str,
) -> ExpressionError {
    match eval(runtime, host, source) {
        Ok(value) => panic!("{source} answered {value:?}"),
        Err(error) => error,
    }
}

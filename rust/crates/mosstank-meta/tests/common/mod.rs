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

/// The clock the tests run on: 2023-11-14 22:13:20 UTC, standing still.
pub fn clock() -> Rc<FixedClock> {
    let clock = FixedClock::default();
    clock.utc_ms.set(1_700_000_000_000);
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

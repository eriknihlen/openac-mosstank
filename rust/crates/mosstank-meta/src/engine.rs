//! The meta engine (`MetaEngine`): a meta's rules run state by state. Every
//! decision interval one pass looks at the current state's rules in order;
//! a rule whose condition holds fires its action once per state entry, and
//! the pass ends at an action that changed the state, answered false, or
//! loaded a profile.
//!
//! The engine owns the expression runtime its conditions and actions run
//! in and the views its metas create; the client and the rest of the
//! plugin are handed to every call. The client's notifications reach it
//! through the `on_*` methods, which the plugin calls from its events.

use std::collections::HashSet;

use ac_plugin_abi::LogLevel;
use ac_plugin_abi::items::ItemUseCompletion;
use ac_plugin_abi::magic::SpellList;
use ac_plugin_abi::world::{Confirmation, PortalTransition, Position};
use mosstank_dotnet::compat::regex::{DotNetRegex, RegexError, RegexOptions};
use mosstank_dotnet::text::{casing, ordinal};
use mosstank_expr::{
    DEFAULT_INSTRUCTION_BUDGET, EvaluationContext, ExpressionError, ExpressionValue,
    ExpressionVariableScope, Fault,
};
use mosstank_formats::meta::{
    DEFAULT_STATE, MetaAction, MetaActionKind, MetaCondition, MetaConditionKind, MetaProfile,
    MetaRule,
};
use openac_plugin_compat::number;

use crate::chat;
use crate::events;
use crate::host::{Character, MetaHost};
use crate::runtime::{ExpressionRuntime, FunctionExtension, Reach};
use crate::services::MetaServices;
use crate::view_manager::{ButtonAction, MetaViewControls, MetaViewManager};

/// The state a meta starts in.
pub const DEFAULT_STATE_NAME: &str = DEFAULT_STATE;

/// How often the reference macro looks at its meta, seconds.
pub const DECISION_INTERVAL_SECONDS: f64 = 0.293;

/// The shortest interval, milliseconds.
pub const MINIMUM_INTERVAL_MILLISECONDS: i32 = 50;

/// The longest interval, milliseconds.
pub const MAXIMUM_INTERVAL_MILLISECONDS: i32 = 2000;

/// How deep calls may nest.
pub const MAXIMUM_CALL_DEPTH: usize = 10_000;

/// A portal transition's kind for a portal or recall (1 is a login).
const PORTAL_TRANSITION_PORTAL: u8 = 2;

const UNKNOWN_SET_OPTION_WARNING: &str =
    "SetVTOption Action: Specified setting doesn't exist. Will not execute.";

#[derive(Debug, Clone)]
struct Watchdog {
    state: String,
    range_meters: f64,
    time_span_seconds: f64,
    total_seconds: f64,
    sample_seconds: f64,
    samples: [Position; 10],
}

/// The engine's own state: everything but the runtime and the views.
#[derive(Debug)]
struct Core {
    profile: MetaProfile,
    /// Bumped each time `profile` is replaced (not when its rules are
    /// edited in place).
    profile_generation: u64,
    /// Bumped each time `profile` is replaced or one of its rules edited.
    profile_revision: u64,
    rule_ids: Vec<u64>,
    next_rule_id: u64,
    fired: HashSet<u64>,
    call_stack: Vec<String>,
    chat_batch: Vec<(String, u32)>,
    shown_warnings: HashSet<String>,
    profile_just_loaded: bool,
    decision_accumulator: f64,
    interval_seconds: f64,
    state_seconds: f64,
    persistent_state_seconds: f64,
    chat_sequence: u64,
    was_portal_space: bool,
    was_dead: bool,
    portal_entered: bool,
    portal_exited: bool,
    death_edge: bool,
    login_complete_edge: bool,
    logoff_edge: bool,
    portal_transition_edge: bool,
    item_use_edge: bool,
    container_opened_edge: bool,
    container_closed_edge: bool,
    confirmation_edge: bool,
    watchdog: Option<Watchdog>,
    status: String,
    enabled: bool,
    current_state: String,
}

/// The meta engine.
#[derive(Debug)]
pub struct MetaEngine {
    core: Core,
    expressions: ExpressionRuntime,
    views: MetaViewManager,
}

fn normalize_state(state: &str) -> String {
    if casing::is_white_space_only(state) {
        DEFAULT_STATE.to_owned()
    } else {
        state.trim().to_owned()
    }
}

fn is_dead(host: &mut dyn MetaHost) -> bool {
    let character = Character::of(host);
    character.in_world && character.health.1 > 0 && character.health.0 == 0
}

/// One line for a meta condition; a compound one lists what it holds.
#[must_use]
pub fn describe_condition(condition: &MetaCondition) -> String {
    use MetaConditionKind as K;
    match condition.kind {
        K::All => compound("All", &condition.children),
        K::Any => compound("Any", &condition.children),
        K::Not => {
            if condition.children.len() == 1 {
                format!("Not {}", describe_condition(&condition.children[0]))
            } else {
                compound("Not", &condition.children)
            }
        }
        K::Expression => format!("Expression: {}", condition.text),
        K::ChatMessage | K::ChatMessageCapture => {
            format!("{}: {}", condition.kind.name(), condition.text)
        }
        K::Always
        | K::Never
        | K::CharacterDeath
        | K::NavigationRouteEmpty
        | K::AnyVendorOpen
        | K::VendorClosed
        | K::NeedToBuff
        | K::PortalspaceEntered
        | K::PortalspaceExited
        | K::LoginComplete
        | K::Logoff
        | K::PortalTransition
        | K::ItemUseCompleted
        | K::ContainerOpened
        | K::ContainerClosed
        | K::ConfirmationRequested => condition.kind.name().to_owned(),
        _ => format!(
            "{} {}",
            condition.kind.name(),
            number::format_f64(condition.number, "0.###").unwrap_or_default()
        ),
    }
}

fn compound(name: &str, children: &[MetaCondition]) -> String {
    if children.is_empty() {
        return name.to_owned();
    }
    let parts: Vec<String> = children.iter().map(describe_condition).collect();
    format!("{name} ({})", parts.join("; "))
}

/// One line for a meta action; one that does several things lists them.
#[must_use]
pub fn describe_action(action: &MetaAction) -> String {
    match action.kind {
        MetaActionKind::All => {
            if action.children.is_empty() {
                "All".to_owned()
            } else {
                let parts: Vec<String> = action.children.iter().map(describe_action).collect();
                format!("All ({})", parts.join("; "))
            }
        }
        MetaActionKind::SetMetaState
        | MetaActionKind::CallMetaState
        | MetaActionKind::ChatCommand
        | MetaActionKind::ExpressionAction
        | MetaActionKind::ChatExpression => format!("{}: {}", action.kind.name(), action.text),
        _ => action.kind.name().to_owned(),
    }
}

fn describe_executing(action: &MetaAction) -> String {
    match action.kind {
        MetaActionKind::SetMetaState => format!("Set Meta State {}", action.text),
        MetaActionKind::CallMetaState => format!("Call Meta State {}", action.text),
        MetaActionKind::ChatCommand => format!("Chat {}", action.text),
        other => other.name().to_owned(),
    }
}

/// A chat condition's colour list: `None` takes every line; a list that
/// does not read takes none.
fn parse_colors(source: &str) -> Option<Vec<u32>> {
    if casing::is_white_space_only(source) {
        return None;
    }
    let mut result = Vec::new();
    for part in source.split(';').filter(|part| !part.is_empty()) {
        // The list reads the same on every machine, whatever its culture.
        match mosstank_dotnet::try_parse_int(part.trim()) {
            Some(kind) => result.push(kind.cast_unsigned()),
            None => return Some(Vec::new()),
        }
    }
    Some(result)
}

/// `checked((int)value)`.
fn checked_int(value: f64) -> Result<i32, String> {
    let whole = value.trunc();
    if value.is_nan() || whole < f64::from(i32::MIN) || whole > f64::from(i32::MAX) {
        return Err("Arithmetic operation resulted in an overflow.".to_owned());
    }
    #[allow(clippy::cast_possible_truncation)]
    Ok(whole as i32)
}

/// `checked((uint)value)`.
fn checked_uint(value: f64) -> Result<u32, String> {
    let whole = value.trunc();
    if value.is_nan() || whole < 0.0 || whole > f64::from(u32::MAX) {
        return Err("Arithmetic operation resulted in an overflow.".to_owned());
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(whole as u32)
}

impl Core {
    fn new(profile: MetaProfile, portal: bool, dead: bool) -> Self {
        let mut core = Self {
            profile: MetaProfile::default(),
            profile_generation: 0,
            profile_revision: 0,
            rule_ids: Vec::new(),
            next_rule_id: 1,
            fired: HashSet::new(),
            call_stack: Vec::new(),
            chat_batch: Vec::new(),
            shown_warnings: HashSet::new(),
            profile_just_loaded: false,
            decision_accumulator: 0.0,
            interval_seconds: DECISION_INTERVAL_SECONDS,
            state_seconds: 0.0,
            persistent_state_seconds: 0.0,
            chat_sequence: 0,
            was_portal_space: portal,
            was_dead: dead,
            portal_entered: false,
            portal_exited: false,
            death_edge: false,
            login_complete_edge: false,
            logoff_edge: false,
            portal_transition_edge: false,
            item_use_edge: false,
            container_opened_edge: false,
            container_closed_edge: false,
            confirmation_edge: false,
            watchdog: None,
            status: "Meta disabled.".to_owned(),
            enabled: false,
            current_state: DEFAULT_STATE.to_owned(),
        };
        core.set_profile(profile);
        core
    }

    fn set_profile(&mut self, profile: MetaProfile) {
        self.rule_ids = (0..profile.rules.len())
            .map(|_| {
                let id = self.next_rule_id;
                self.next_rule_id += 1;
                id
            })
            .collect();
        self.profile = profile;
        self.profile_generation += 1;
        self.profile_revision += 1;
    }

    fn transition(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        state: &str,
    ) {
        let previous = std::mem::replace(&mut self.current_state, normalize_state(state));
        self.fired.clear();
        self.state_seconds = 0.0;
        self.persistent_state_seconds = 0.0;
        self.watchdog = None;
        self.status = format!("Meta transitioned to {}.", self.current_state);
        let message = format!("Meta state: {previous} -> {}.", self.current_state);
        services.state_changed(host, &message, &self.current_state);
    }

    fn replace_profile(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        profile: MetaProfile,
    ) {
        self.set_profile(profile);
        self.transition(host, services, DEFAULT_STATE);
        // A load from one of the old profile's own actions ends that pass.
        self.profile_just_loaded = true;
    }

    fn clear_event_edges(&mut self, keep_login_complete: bool) {
        if !keep_login_complete {
            self.login_complete_edge = false;
        }
        self.logoff_edge = false;
        self.portal_transition_edge = false;
        self.item_use_edge = false;
        self.container_opened_edge = false;
        self.container_closed_edge = false;
        self.confirmation_edge = false;
    }

    /// Reports a meta warning the first time it comes up; the list is
    /// cleared when the meta is started again.
    fn warn_once(&mut self, host: &mut dyn MetaHost, warning: &str) {
        if !self.shown_warnings.insert(warning.to_owned()) {
            return;
        }
        host.log(LogLevel::Warn, warning);
        chat::post_mosstank(host, warning);
    }

    fn disable_with_error(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        message: &str,
    ) {
        self.enabled = false;
        // A meta that shut itself down over a broken profile stays down
        // across a reload: the setting is cleared here too.
        let _ = services.set_option(host, "EnableMeta", &ExpressionValue::Boolean(false));
        self.status = format!("{message} Meta disabled.");
        chat::post_mosstank(host, &self.status);
        host.log(LogLevel::Error, &self.status);
    }

    fn set_enabled(&mut self, enabled: bool) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        if enabled {
            self.state_seconds = 0.0;
            self.decision_accumulator = self.interval_seconds;
            self.shown_warnings.clear();
            self.status = format!("Meta running: {}.", self.current_state);
        } else {
            "Meta disabled.".clone_into(&mut self.status);
            self.watchdog = None;
        }
    }
}

/// The engine's own expression functions: the meta's state.
struct EngineFunctions<'e> {
    core: &'e mut Core,
}

impl FunctionExtension for EngineFunctions<'_> {
    fn call(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        function: &str,
        _context: &mut EvaluationContext<'_>,
        arguments: &[ExpressionValue],
    ) -> Option<Result<ExpressionValue, Fault>> {
        match function {
            "vtsetmetastate" => Some((|| {
                let state = arguments[0].as_str("vtsetmetastate")?.to_owned();
                self.core.transition(host, services, &state);
                services.meta_state_set(&self.core.current_state);
                Ok(ExpressionValue::one())
            })()),
            "vtgetmetastate" => Some(Ok(ExpressionValue::string(&self.core.current_state))),
            _ => None,
        }
    }

    fn profile_loaded(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        profile: MetaProfile,
    ) {
        self.core.replace_profile(host, services, profile);
    }
}

impl MetaEngine {
    /// An engine for `profile`, with its expression runtime.
    pub fn new(
        host: &mut dyn MetaHost,
        expressions: ExpressionRuntime,
        profile: MetaProfile,
    ) -> Self {
        let portal = host.navigation().is_portal_space;
        let dead = is_dead(host);
        let mut expressions = expressions;
        expressions.register_extension_function("vtsetmetastate", 1, 1, "vtsetmetastate[state]");
        expressions.register_extension_function("vtgetmetastate", 0, 0, "vtgetmetastate[]");
        Self {
            core: Core::new(profile, portal, dead),
            expressions,
            views: MetaViewManager::new(),
        }
    }

    /// The expression runtime.
    pub fn expressions(&mut self) -> &mut ExpressionRuntime {
        &mut self.expressions
    }

    /// The views metas created.
    pub fn views(&mut self) -> &mut MetaViewManager {
        &mut self.views
    }

    /// The profile.
    #[must_use]
    pub fn profile(&self) -> &MetaProfile {
        &self.core.profile
    }

    /// Which profile is in force: a number that changes each time the
    /// profile is replaced (a load, an import, a clear) and not when its
    /// rules are edited in place, as the C# tells profiles apart by
    /// reference.
    #[must_use]
    pub fn profile_generation(&self) -> u64 {
        self.core.profile_generation
    }

    /// A number that changes whenever the profile's rules may have: a
    /// new profile, or a rule added, replaced, removed or moved.
    #[must_use]
    pub fn profile_revision(&self) -> u64 {
        self.core.profile_revision
    }

    /// Adds a rule at the end, as a new rule (`_metaProfile.Rules.Add`).
    pub fn add_rule(&mut self, rule: MetaRule) {
        let id = self.core.next_rule_id;
        self.core.next_rule_id += 1;
        self.core.profile.rules.push(rule);
        self.core.rule_ids.push(id);
        self.core.profile_revision += 1;
    }

    /// Puts `rule` in place of the rule at `index`, which keeps its
    /// identity (`replacement.Id = current.Id`).
    pub fn replace_rule(&mut self, index: usize, rule: MetaRule) {
        if let Some(slot) = self.core.profile.rules.get_mut(index) {
            *slot = rule;
            self.core.profile_revision += 1;
        }
    }

    /// Removes the rule at `index`.
    pub fn remove_rule(&mut self, index: usize) {
        if index < self.core.profile.rules.len() {
            self.core.profile.rules.remove(index);
            self.core.rule_ids.remove(index);
            self.core.profile_revision += 1;
        }
    }

    /// Moves the rule at `from` to `to`, with its identity.
    pub fn move_rule(&mut self, from: usize, to: usize) {
        let rules = &mut self.core.profile.rules;
        if from >= rules.len() || to >= rules.len() {
            return;
        }
        let rule = rules.remove(from);
        rules.insert(to, rule);
        let id = self.core.rule_ids.remove(from);
        self.core.rule_ids.insert(to, id);
        self.core.profile_revision += 1;
    }

    /// Whether the meta runs.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.core.enabled
    }

    /// The state it is in.
    #[must_use]
    pub fn current_state(&self) -> &str {
        &self.core.current_state
    }

    /// What it is doing, in words.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.core.status
    }

    /// How deep the calls are nested.
    #[must_use]
    pub fn call_depth(&self) -> usize {
        self.core.call_stack.len()
    }

    /// How many rules fired in this state entry.
    #[must_use]
    pub fn fired_rule_count(&self) -> usize {
        self.core.fired.len()
    }

    /// The profile's states and Default, each once ignoring case, sorted
    /// ignoring case.
    #[must_use]
    pub fn states(&self) -> Vec<String> {
        let mut states: Vec<String> = Vec::new();
        for state in self
            .core
            .profile
            .rules
            .iter()
            .map(|rule| normalize_state(&rule.state))
            .chain(std::iter::once(DEFAULT_STATE.to_owned()))
        {
            if !states
                .iter()
                .any(|known| ordinal::equals_ignore_case(known, &state))
            {
                states.push(state);
            }
        }
        states.sort_by(|a, b| ordinal::compare_ignore_case(a, b));
        states
    }

    /// How often the meta is looked at, milliseconds.
    #[must_use]
    pub fn interval_milliseconds(&self) -> i32 {
        #[allow(clippy::cast_possible_truncation)]
        let milliseconds = (self.core.interval_seconds * 1000.0).round_ties_even() as i32;
        milliseconds
    }

    /// Sets how often the meta is looked at, clamped to 50..2000 ms.
    pub fn set_interval_milliseconds(&mut self, value: i32) {
        self.core.interval_seconds =
            f64::from(value.clamp(MINIMUM_INTERVAL_MILLISECONDS, MAXIMUM_INTERVAL_MILLISECONDS))
                / 1000.0;
    }

    /// Starts or stops the meta.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.core.set_enabled(enabled);
    }

    /// A new session: back to Default, nothing fired, nothing pending but
    /// the login edge (the client announces the login before the plugin's
    /// own session reset runs).
    pub fn reset_session(&mut self, host: &mut dyn MetaHost) {
        let core = &mut self.core;
        core.enabled = false;
        DEFAULT_STATE.clone_into(&mut core.current_state);
        core.fired.clear();
        core.call_stack.clear();
        core.chat_batch.clear();
        core.decision_accumulator = 0.0;
        core.state_seconds = 0.0;
        core.persistent_state_seconds = 0.0;
        core.chat_sequence = 0;
        core.was_portal_space = host.navigation().is_portal_space;
        core.was_dead = is_dead(host);
        core.portal_entered = false;
        core.portal_exited = false;
        core.death_edge = false;
        core.clear_event_edges(true);
        core.watchdog = None;
        "Meta disabled.".clone_into(&mut core.status);
    }

    /// Replaces the profile; the meta goes to Default, and a load from one
    /// of the old profile's own actions ends that pass.
    pub fn replace_profile(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        profile: MetaProfile,
    ) {
        self.core.replace_profile(host, services, profile);
    }

    /// Takes the profile the engine is built with, before anything runs:
    /// no transition, so nothing is announced (the C#'s constructor
    /// argument).
    pub fn set_initial_profile(&mut self, profile: MetaProfile) {
        self.core.set_profile(profile);
    }

    /// Moves the meta to a state.
    pub fn transition(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        state: &str,
    ) {
        self.core.transition(host, services, state);
    }

    /// Runs an expression with everything the meta's expressions reach:
    /// the engine's own functions and the views.
    ///
    /// # Errors
    /// The text is not an expression, or the run failed.
    pub fn evaluate(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        source: &str,
    ) -> Result<ExpressionValue, ExpressionError> {
        let mut functions = EngineFunctions {
            core: &mut self.core,
        };
        let result = self.expressions.evaluate_with(
            host,
            Reach {
                services,
                views: Some(&mut self.views),
                extension: Some(&mut functions),
            },
            source,
            DEFAULT_INSTRUCTION_BUDGET,
        );
        // The verbs its `chatbox[]` lines submitted run before the caller
        // goes on.
        services.run_pending_lines(self, host);
        result
    }

    /// What a value prints as.
    pub fn display(&mut self, host: &mut dyn MetaHost, value: &ExpressionValue) -> String {
        self.expressions.display(host, value)
    }

    /// Hits a button of a view the way a click on the drawn window does:
    /// its expression runs, then the meta moves to its state.
    pub fn click_view(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        view: &str,
        control: &str,
    ) -> bool {
        let Self {
            core,
            expressions,
            views,
        } = self;
        let mut pending: Vec<ButtonRequest> = Vec::new();
        let found = views.click(host, view, control, &mut |_, action| {
            pending.push(ButtonRequest::from(action));
            Ok(())
        });
        if !found {
            return false;
        }
        run_button(
            core,
            expressions,
            views,
            host,
            services,
            view,
            control,
            pending,
        );
        true
    }

    /// Hits the control a drawn view bound to `slot`.
    pub fn click_view_slot(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        view: &str,
        slot: usize,
    ) {
        let name = self
            .views
            .find(view)
            .and_then(|parsed| parsed.controls().get(slot))
            .map(|control| control.name.clone());
        if let Some(name) = name {
            // The slot's own control runs even when another shares its
            // name: run through the slot, not the name.
            let Self {
                core,
                expressions,
                views,
            } = self;
            let mut pending: Vec<ButtonRequest> = Vec::new();
            views.click_slot(host, view, slot, &mut |_, action| {
                pending.push(ButtonRequest::from(action));
                Ok(())
            });
            run_button(
                core,
                expressions,
                views,
                host,
                services,
                view,
                &name,
                pending,
            );
        }
    }

    /// One tick: the runtime's own tick, the edges and chat, the clocks;
    /// a pass once the decision interval has gone by.
    pub fn on_tick(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        elapsed_seconds: f64,
    ) {
        if elapsed_seconds < 0.0 || !elapsed_seconds.is_finite() {
            return;
        }
        {
            let mut functions = EngineFunctions {
                core: &mut self.core,
            };
            self.expressions.on_tick_with(
                host,
                services,
                Some(&mut self.views),
                Some(&mut functions),
                elapsed_seconds,
            );
        }
        // A delayed expression's lines run before the pass looks at a rule.
        services.run_pending_lines(self, host);
        self.capture_edges_and_chat(host, services);
        if !self.core.enabled {
            return;
        }
        let core = &mut self.core;
        core.state_seconds += elapsed_seconds;
        core.persistent_state_seconds += elapsed_seconds;
        core.decision_accumulator += elapsed_seconds;
        self.update_watchdog(host, elapsed_seconds);
        let core = &mut self.core;
        if core.decision_accumulator < core.interval_seconds {
            return;
        }
        core.decision_accumulator %= core.interval_seconds;
        self.evaluate_pass(host, services);
        let core = &mut self.core;
        core.portal_entered = false;
        core.portal_exited = false;
        core.death_edge = false;
        core.clear_event_edges(false);
        core.chat_batch.clear();
    }

    /// One pass over the current state's rules.
    pub fn evaluate_pass(&mut self, host: &mut dyn MetaHost, services: &mut dyn MetaServices) {
        if !self.core.enabled {
            return;
        }
        if self.watchdog_expired(host) {
            if self.core.call_stack.len() >= MAXIMUM_CALL_DEPTH {
                self.core.disable_with_error(
                    host,
                    services,
                    "Meta Error: Call stack overflow (watchdog loop?).",
                );
                return;
            }
            let target = self
                .core
                .watchdog
                .as_ref()
                .map(|watchdog| watchdog.state.clone())
                .unwrap_or_default();
            let current = self.core.current_state.clone();
            self.core.call_stack.push(current);
            self.core.transition(host, services, &target);
            self.core.status = format!("Meta watchdog expired; calling {target}.");
            return;
        }

        let current = self.core.current_state.clone();
        let rules: Vec<(u64, MetaRule)> = self
            .core
            .profile
            .rules
            .iter()
            .zip(&self.core.rule_ids)
            .filter(|(rule, _)| {
                rule.enabled && ordinal::equals_ignore_case(&normalize_state(&rule.state), &current)
            })
            .map(|(rule, id)| (*id, rule.clone()))
            .collect();
        let handlers_enabled = services.game_event_handlers_enabled();
        self.run_pass(host, services, &rules, handlers_enabled);
        // An edge is an occurrence: its rule may fire again on the next one,
        // so it is re-armed once the pass is over.
        for (id, rule) in &rules {
            if events::is_event_edge(rule.condition.kind) {
                self.core.fired.remove(id);
            }
        }
    }

    fn run_pass(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        rules: &[(u64, MetaRule)],
        handlers_enabled: bool,
    ) {
        // Every condition in the pass reads the same object capture; an
        // action can change what the client holds, so it starts a new one.
        self.expressions.capture().begin_pass();
        // Only a rule of the state the pass began in may fire, and the pass
        // ends after an action that changed the state, answered false, or
        // loaded a profile.
        let pass_state = self.core.current_state.clone();
        for (id, rule) in rules {
            if self.core.current_state != pass_state {
                break;
            }
            if !handlers_enabled && events::is_handler(rule) {
                continue;
            }
            if self.core.fired.contains(id)
                || !self.evaluate_condition(host, services, &rule.condition)
            {
                continue;
            }
            self.core.fired.insert(*id);
            self.core.status = format!("Meta executing {}.", describe_executing(&rule.action));
            self.core.profile_just_loaded = false;
            let continue_pass = self.execute_action(host, services, &rule.action);
            self.expressions.capture().invalidate();
            if self.core.profile_just_loaded {
                self.core.profile_just_loaded = false;
                break;
            }
            if !continue_pass || self.core.current_state != pass_state {
                break;
            }
        }
        self.expressions.capture().end_pass();
    }

    /// A death the player asked for (the panel's test button): the death
    /// edge for one pass.
    pub fn trigger_fake_death(&mut self, host: &mut dyn MetaHost, services: &mut dyn MetaServices) {
        self.core.death_edge = true;
        if self.core.enabled {
            self.evaluate_pass(host, services);
        }
        self.core.death_edge = false;
    }

    // ---- the client's notifications ----

    /// The login completed: an edge for the next pass.
    pub fn on_login_complete(&mut self) {
        self.core.login_complete_edge = true;
    }

    /// The character is logging off: there is no next pass in a session
    /// that is ending, so the pass runs now.
    pub fn on_logoff(&mut self, host: &mut dyn MetaHost, services: &mut dyn MetaServices) {
        self.set_character_variables(host);
        self.core.logoff_edge = true;
        if self.core.enabled {
            self.evaluate_pass(host, services);
        }
        self.core.logoff_edge = false;
    }

    /// The character died: the server's message.
    pub fn on_local_player_died(&mut self, message: &str) {
        self.set_event_variable("deathmessage", ExpressionValue::string(message));
        self.core.death_edge = true;
    }

    /// A portal or login transition moved on; the edge is the arrival.
    pub fn on_portal_transition(&mut self, transition: &PortalTransition) {
        // A portal's own reports are its portal-space edges, as the game's
        // portal-mode events are: a short hop can begin and end between two
        // ticks, and the portal-space flag is then never seen on. A login's
        // reports are not a portal.
        if transition.kind == PORTAL_TRANSITION_PORTAL {
            if transition.is_completed {
                self.core.portal_exited = true;
            } else if !transition.is_cancelled {
                self.core.portal_entered = true;
            }
        }
        if !transition.is_completed {
            return;
        }
        self.set_event_variable(
            "destinationcell",
            ExpressionValue::Number(f64::from(transition.destination_cell)),
        );
        self.core.portal_transition_edge = true;
    }

    /// A use the client awaited ended.
    pub fn on_item_use_completed(&mut self, completion: &ItemUseCompletion) {
        self.set_event_variable(
            "sourceid",
            ExpressionValue::object_id_number(completion.source_id),
        );
        self.set_event_variable(
            "targetid",
            ExpressionValue::object_id_number(completion.target_id),
        );
        self.set_event_variable(
            "error",
            ExpressionValue::Number(f64::from(completion.weenie_error)),
        );
        self.core.item_use_edge = true;
    }

    /// An outside container opened.
    pub fn on_container_opened(&mut self, container_id: u32) {
        self.set_event_variable(
            "containerid",
            ExpressionValue::object_id_number(container_id),
        );
        self.core.container_opened_edge = true;
    }

    /// The outside container closed.
    pub fn on_container_closed(&mut self, container_id: u32) {
        self.set_event_variable(
            "containerid",
            ExpressionValue::object_id_number(container_id),
        );
        self.core.container_closed_edge = true;
    }

    /// The server asked a yes/no question.
    pub fn on_confirmation_requested(&mut self, confirmation: &Confirmation) {
        self.set_event_variable(
            "contextid",
            ExpressionValue::Number(f64::from(confirmation.context_id)),
        );
        self.set_event_variable(
            "confirmtype",
            ExpressionValue::Number(f64::from(confirmation.kind)),
        );
        self.set_event_variable("text", ExpressionValue::string(&confirmation.text));
        self.core.confirmation_edge = true;
    }

    /// An event's field for expressions: `gevt_` and the field, a session
    /// variable as a chat capture's groups are.
    fn set_event_variable(&mut self, name: &str, value: ExpressionValue) {
        let _ = self.expressions.state().set(
            ExpressionVariableScope::Session,
            &format!("gevt_{name}"),
            value,
        );
    }

    fn set_character_variables(&mut self, host: &mut dyn MetaHost) {
        let character = Character::of(host);
        self.set_event_variable("id", ExpressionValue::object_id_number(character.id));
        self.set_event_variable("name", ExpressionValue::string(&character.name));
    }

    // ---- conditions ----

    fn evaluate_condition(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        condition: &MetaCondition,
    ) -> bool {
        match condition.kind {
            MetaConditionKind::Never => false,
            MetaConditionKind::Always => true,
            MetaConditionKind::All => condition
                .children
                .iter()
                .all(|child| self.evaluate_condition(host, services, child)),
            MetaConditionKind::Any => condition
                .children
                .iter()
                .any(|child| self.evaluate_condition(host, services, child)),
            MetaConditionKind::Not => {
                !condition.children.is_empty()
                    && !self.evaluate_condition(host, services, &condition.children[0])
            }
            _ => match self.evaluate_leaf_condition(host, services, condition) {
                Ok(holds) => holds,
                // A condition that cannot be decided is false and reported
                // once; it never stops the pass.
                Err(message) => {
                    self.core.warn_once(
                        host,
                        &format!(
                            "Error in meta condition {}: {message}",
                            condition.kind.name()
                        ),
                    );
                    false
                }
            },
        }
    }

    fn evaluate_number(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        source: &str,
    ) -> Result<f64, String> {
        let value = self
            .evaluate(host, services, source)
            .map_err(|error| error.message())?;
        value.as_number(source).map_err(|fault| fault.message())
    }

    #[allow(clippy::too_many_lines)]
    fn evaluate_leaf_condition(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        condition: &MetaCondition,
    ) -> Result<bool, String> {
        use MetaConditionKind as K;
        Ok(match condition.kind {
            K::ChatMessage => self.chat_match(condition, false)?,
            K::ChatMessageCapture => self.chat_match(condition, true)?,
            K::PackSlotsLessThanOrEqual => {
                self.evaluate_number(host, services, "getfreeitemslots[]")? <= condition.number
            }
            K::SecondsInStateGreaterThanOrEqual => self.core.state_seconds >= condition.number,
            K::PersistentSecondsInStateGreaterThanOrEqual => {
                self.core.persistent_state_seconds >= condition.number
            }
            K::NavigationRouteEmpty => services.is_navigation_route_empty(host),
            K::CharacterDeath => self.core.death_edge,
            K::AnyVendorOpen => host.item_state().active_vendor_id != 0,
            K::VendorClosed => host.item_state().active_vendor_id == 0,
            K::InventoryItemCountLessThanOrEqual => {
                self.inventory_count(host, services, &condition.text)? <= condition.number
            }
            K::InventoryItemCountGreaterThanOrEqual => {
                self.inventory_count(host, services, &condition.text)? >= condition.number
            }
            K::MonsterNameCountWithinDistance => {
                f64::from(self.monster_count(host, &condition.text, condition.secondary_number)?)
                    >= condition.number
            }
            K::MonsterPriorityCountWithinDistance => {
                let priority = checked_int(condition.tertiary_number)?;
                f64::from(services.count_monsters_by_priority(
                    host,
                    priority,
                    condition.secondary_number,
                )) >= condition.number
            }
            K::NeedToBuff => services.needs_buff(host),
            K::NoMonstersWithinDistance => {
                #[allow(clippy::cast_possible_truncation)]
                let distance = condition.number as f32;
                host.hostile_targets(distance).is_empty()
            }
            K::LandblockEquals => {
                let wanted = checked_int(condition.number)?.cast_unsigned();
                host.navigation().position.cell_id & 0xFFFF_0000 == wanted
            }
            K::LandcellEquals => {
                let wanted = checked_int(condition.number)?.cast_unsigned();
                host.navigation().position.cell_id == wanted
            }
            K::PortalspaceEntered => self.core.portal_entered,
            K::PortalspaceExited => self.core.portal_exited,
            K::TimeLeftOnSpellGreaterThanOrEqual => {
                self.spell_time_left(host, condition)? >= condition.secondary_number
            }
            K::BurdenPercentGreaterThanOrEqual => {
                self.evaluate_number(host, services, "getcharburden[]")? >= condition.number
            }
            K::DistanceFromAnyRoutePointGreaterThanOrEqual => {
                services.distance_from_any_route_point(host) >= condition.number
            }
            K::Expression => self.expression_condition(host, services, &condition.text),
            K::LoginComplete => {
                if self.core.login_complete_edge {
                    self.set_character_variables(host);
                    true
                } else {
                    false
                }
            }
            K::Logoff => self.core.logoff_edge,
            K::PortalTransition => self.core.portal_transition_edge,
            K::ItemUseCompleted => self.core.item_use_edge,
            K::ContainerOpened => self.core.container_opened_edge,
            K::ContainerClosed => self.core.container_closed_edge,
            K::ConfirmationRequested => self.core.confirmation_edge,
            K::Never | K::Always | K::All | K::Any | K::Not => false,
        })
    }

    /// An expression condition: one that cannot be read or run is false
    /// and reported once; the pass goes on.
    fn expression_condition(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        source: &str,
    ) -> bool {
        if source.is_empty() {
            return false;
        }
        match self.evaluate(host, services, source) {
            Ok(value) => value.is_truthy(),
            Err(error) => {
                self.core.warn_once(
                    host,
                    &format!("Error in meta expression: {source} ({})", error.message()),
                );
                false
            }
        }
    }

    fn chat_match(&mut self, condition: &MetaCondition, capture: bool) -> Result<bool, String> {
        let Ok(regex) = DotNetRegex::new(&condition.text, RegexOptions::CULTURE_INVARIANT) else {
            return Ok(false);
        };
        let accepted = parse_colors(&condition.secondary_text);
        let batch = self.core.chat_batch.clone();
        for (text, color) in &batch {
            if accepted
                .as_ref()
                .is_some_and(|colors| !colors.contains(color))
            {
                continue;
            }
            let found = regex.find(text).map_err(|error| regex_message(&error))?;
            let Some(found) = found else {
                continue;
            };
            if capture {
                let state = self.expressions.state();
                for name in regex.group_names() {
                    let variable = format!("capturegroup_{name}");
                    let group = found.group(name);
                    if group.success {
                        let _ = state.set(
                            ExpressionVariableScope::Session,
                            &variable,
                            ExpressionValue::string(group.value),
                        );
                    } else {
                        let _ = state.clear(ExpressionVariableScope::Session, &variable);
                    }
                }
                let _ = state.set(
                    ExpressionVariableScope::Session,
                    "capturecolor",
                    ExpressionValue::Number(f64::from(*color)),
                );
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn inventory_count(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        name: &str,
    ) -> Result<f64, String> {
        // The backtick is the quote, so a name only has to escape a
        // backslash and a backtick.
        let escaped = name.replace('\\', "\\\\").replace('`', "\\`");
        self.evaluate_number(
            host,
            services,
            &format!("getitemcountininventorybyname[`{escaped}`]"),
        )
    }

    fn monster_count(
        &mut self,
        host: &mut dyn MetaHost,
        pattern: &str,
        distance: f64,
    ) -> Result<i32, String> {
        let Ok(regex) = DotNetRegex::new(
            pattern,
            RegexOptions::IGNORE_CASE | RegexOptions::CULTURE_INVARIANT,
        ) else {
            return Ok(0);
        };
        #[allow(clippy::cast_possible_truncation)]
        let distance = distance as f32;
        let mut count = 0;
        for target in host.hostile_targets(distance) {
            if regex
                .is_match(&target.name)
                .map_err(|error| regex_message(&error))?
            {
                count += 1;
            }
        }
        Ok(count)
    }

    fn spell_time_left(
        &mut self,
        host: &mut dyn MetaHost,
        condition: &MetaCondition,
    ) -> Result<f64, String> {
        let spell_id = if condition.number > 0.0 {
            checked_uint(condition.number)?
        } else {
            host.known_spells(SpellList::SelfBuffs)
                .into_iter()
                .chain(host.known_spells(SpellList::Combat))
                .find(|spell| ordinal::equals_ignore_case(&spell.name, &condition.text))
                .map_or(0, |spell| spell.spell_id)
        };
        Ok(host
            .enchantments()
            .iter()
            .find(|enchantment| enchantment.spell_id == spell_id)
            .map_or(0.0, |enchantment| enchantment.seconds_remaining))
    }

    // ---- actions ----

    fn execute_action(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        action: &MetaAction,
    ) -> bool {
        match action.kind {
            MetaActionKind::None => true,
            MetaActionKind::SetMetaState => {
                self.core.transition(host, services, &action.text);
                false
            }
            MetaActionKind::ChatCommand => {
                self.submit(host, services, &action.text);
                true
            }
            // Every action of a DoAll runs and the DoAll counts as done; a
            // state change or a load among them ends the pass once the DoAll
            // is over.
            MetaActionKind::All => {
                for child in &action.children {
                    self.execute_action(host, services, child);
                }
                true
            }
            MetaActionKind::LoadEmbeddedNavigationRoute => {
                services.load_embedded_navigation_route(
                    host,
                    action.embedded_route.as_deref(),
                    &action.secondary_text,
                );
                true
            }
            MetaActionKind::CallMetaState => {
                if self.core.call_stack.len() >= MAXIMUM_CALL_DEPTH {
                    self.core.disable_with_error(
                        host,
                        services,
                        "Meta Error: too many nested calls; check for a call loop.",
                    );
                    return false;
                }
                let return_state = if casing::is_white_space_only(&action.secondary_text) {
                    self.core.current_state.clone()
                } else {
                    normalize_state(&action.secondary_text)
                };
                self.core.call_stack.push(return_state);
                self.core.transition(host, services, &action.text);
                false
            }
            MetaActionKind::ReturnFromCall => {
                let Some(state) = self.core.call_stack.pop() else {
                    self.core.disable_with_error(
                        host,
                        services,
                        "Meta Error: a return with no matching call.",
                    );
                    return false;
                };
                self.core.transition(host, services, &state);
                false
            }
            MetaActionKind::ExpressionAction => {
                self.run_expression_action(host, services, &action.text, false)
            }
            MetaActionKind::ChatExpression => {
                self.run_expression_action(host, services, &action.text, true)
            }
            MetaActionKind::SetWatchdog => {
                let range = if action.number <= 0.0 {
                    5.0
                } else {
                    action.number
                };
                let seconds = if action.secondary_number <= 0.0 {
                    10.0
                } else {
                    action.secondary_number
                };
                let position = host.navigation().position;
                self.core.watchdog = Some(Watchdog {
                    state: normalize_state(&action.text),
                    range_meters: range.max(0.0),
                    time_span_seconds: seconds.max(0.001),
                    total_seconds: 0.0,
                    sample_seconds: 0.0,
                    samples: std::array::from_fn(|_| position.clone()),
                });
                true
            }
            MetaActionKind::ClearWatchdog => {
                self.core.watchdog = None;
                true
            }
            MetaActionKind::GetVtankOption => {
                self.run_get_option(host, services, &action.text, &action.secondary_text)
            }
            MetaActionKind::SetVtankOption => {
                self.run_set_option(host, services, &action.text, &action.secondary_text)
            }
            // A view action succeeds whether or not a window was shown or
            // found, so a refused or absent view never stops a DoAll.
            MetaActionKind::CreateView => {
                self.views
                    .create(host, &action.text, &action.secondary_text);
                true
            }
            MetaActionKind::DestroyView => {
                self.views.destroy(host, &action.text);
                true
            }
            MetaActionKind::DestroyAllViews => {
                self.views.destroy_all(host);
                true
            }
        }
    }

    /// Submits a chat line through the plugin, which runs its own verbs
    /// before this returns; a line that loaded a meta profile replaces this
    /// one.
    fn submit(&mut self, host: &mut dyn MetaHost, services: &mut dyn MetaServices, line: &str) {
        if let Some(profile) = services.submit_chat(host, line) {
            self.core.replace_profile(host, services, profile);
        }
        services.run_pending_lines(self, host);
    }

    /// An expression action or a chat expression action: an empty one does
    /// nothing and ends the pass; one that cannot be read or run is
    /// reported once and still counts as done.
    fn run_expression_action(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        source: &str,
        chat: bool,
    ) -> bool {
        if source.is_empty() {
            return false;
        }
        let result = match self.evaluate(host, services, source) {
            Ok(result) => result,
            Err(error) => {
                let warning = if chat {
                    format!(
                        "Error in meta expression chat action: {source} ({})",
                        error.message()
                    )
                } else {
                    format!(
                        "Error in meta expression action: {source} ({})",
                        error.message()
                    )
                };
                self.core.warn_once(host, &warning);
                return true;
            }
        };
        if chat {
            let text = self.expressions.display(host, &result);
            if !text.is_empty() {
                self.submit(host, services, &text);
            }
        }
        true
    }

    /// A SetOpt action: it always counts as done; an empty option or
    /// value, a value that fails, a value of the wrong type or an option
    /// that does not exist is reported once.
    fn run_set_option(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        option: &str,
        value_source: &str,
    ) -> bool {
        if option.is_empty() || value_source.is_empty() {
            self.core.warn_once(
                host,
                "SetVTOption Action: Empty option or expression. Will not execute.",
            );
            return true;
        }
        // Whether the option exists is asked before the value is looked at.
        if !mosstank_settings::options::is_known(option) {
            self.core.warn_once(host, UNKNOWN_SET_OPTION_WARNING);
            return true;
        }
        let value = match self.evaluate(host, services, value_source) {
            Ok(value) => value,
            Err(error) => {
                self.core.warn_once(
                    host,
                    &format!(
                        "Error in Set VT Option meta expression: {value_source} ({})",
                        error.message()
                    ),
                );
                return true;
            }
        };
        match services.set_option(host, option, &value) {
            Err(fault) => {
                self.core.warn_once(
                    host,
                    &format!(
                        "SetVTOption Action: Attempted to set {option} with the wrong type of value ({})",
                        fault.message()
                    ),
                );
            }
            Ok(false) => self.core.warn_once(host, UNKNOWN_SET_OPTION_WARNING),
            Ok(true) => {
                // The option owner's write of EnableMeta starts or stops
                // this engine too.
                if mosstank_settings::options::canonical(option)
                    .is_some_and(|name| ordinal::equals_ignore_case(name, "EnableMeta"))
                {
                    self.core.set_enabled(value.is_truthy());
                }
            }
        }
        true
    }

    /// A GetOpt action: it always counts as done; an empty option or
    /// variable, or an option that does not exist, is reported once and no
    /// variable is written.
    fn run_get_option(
        &mut self,
        host: &mut dyn MetaHost,
        services: &mut dyn MetaServices,
        option: &str,
        variable: &str,
    ) -> bool {
        if option.is_empty() || variable.is_empty() {
            self.core.warn_once(
                host,
                "GetVTOption Action: Empty option or variable. Will not execute.",
            );
            return true;
        }
        if !mosstank_settings::options::is_known(option) {
            self.core.warn_once(
                host,
                "GetVTOption Action: Specified setting doesn't exist. Will not execute.",
            );
            return true;
        }
        let value = services.get_option(host, option);
        let _ = self
            .expressions
            .state()
            .set(ExpressionVariableScope::Session, variable, value);
        true
    }

    // ---- edges, chat and the watchdog ----

    fn capture_edges_and_chat(&mut self, host: &mut dyn MetaHost, services: &mut dyn MetaServices) {
        let _ = services;
        let portal = host.navigation().is_portal_space;
        let core = &mut self.core;
        core.portal_entered |= !core.was_portal_space && portal;
        core.portal_exited |= core.was_portal_space && !portal;
        core.was_portal_space = portal;
        let dead = is_dead(host);
        let core = &mut self.core;
        core.death_edge |= !core.was_dead && dead;
        core.was_dead = dead;

        let messages = host.chat_captured(self.core.chat_sequence);
        let character = Character::of(host);
        for message in messages {
            // The chat conditions read the line the reference chat window
            // prints and its chat text type.
            let line = chat::compose_line(&message, character.id, &character.name);
            self.core.chat_batch.push((line, message.log_text_type));
            self.core.chat_sequence = self.core.chat_sequence.max(message.sequence);
            self.capture_dropped_items(&message.text);
        }
    }

    /// What a death cost, read from the server's own line about it.
    fn capture_dropped_items(&mut self, text: &str) {
        const LOST: &str = "You've lost ";
        if let Some(rest) = text.strip_prefix(LOST) {
            self.set_event_variable("droppeditems", ExpressionValue::string(rest));
        } else if text.starts_with("You have retained all your items.")
            || text.starts_with("You are enveloped in a feeling of warmth")
        {
            self.set_event_variable("droppeditems", ExpressionValue::string(""));
        }
    }

    fn update_watchdog(&mut self, host: &mut dyn MetaHost, elapsed_seconds: f64) {
        let Some(watchdog) = self.core.watchdog.as_mut() else {
            return;
        };
        watchdog.total_seconds += elapsed_seconds;
        watchdog.sample_seconds += elapsed_seconds;
        let interval = watchdog.time_span_seconds / 10.0;
        if watchdog.sample_seconds >= interval {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let index =
                ((watchdog.total_seconds / interval).floor() as i64).rem_euclid(10) as usize;
            watchdog.samples[index] = host.navigation().position;
            watchdog.sample_seconds %= interval;
        }
    }

    fn watchdog_expired(&mut self, host: &mut dyn MetaHost) -> bool {
        let Some(watchdog) = self.core.watchdog.as_ref() else {
            return false;
        };
        if watchdog.total_seconds < watchdog.time_span_seconds {
            return false;
        }
        let current = host.navigation().position;
        watchdog
            .samples
            .iter()
            .all(|sample| sample.horizontal_distance_metres(&current) <= watchdog.range_meters)
    }
}

fn regex_message(error: &RegexError) -> String {
    match error {
        RegexError::StepLimitExceeded => {
            "The RegEx engine has timed out while trying to match a pattern to an input string."
                .to_owned()
        }
        other => other.to_string(),
    }
}

/// A button's request, owned so it outlives the view's borrow.
enum ButtonRequest {
    Expression(String),
    SetState(String),
}

impl From<ButtonAction<'_>> for ButtonRequest {
    fn from(action: ButtonAction<'_>) -> Self {
        match action {
            ButtonAction::Expression(source) => Self::Expression(source.to_owned()),
            ButtonAction::SetState(state) => Self::SetState(state.to_owned()),
        }
    }
}

/// Runs what a hit button asked for: its expression, then its state; an
/// expression that fails is reported once per distinct error and does not
/// stop the state change.
#[allow(clippy::too_many_arguments)]
fn run_button(
    core: &mut Core,
    expressions: &mut ExpressionRuntime,
    views: &mut MetaViewManager,
    host: &mut dyn MetaHost,
    services: &mut dyn MetaServices,
    _view: &str,
    _control: &str,
    pending: Vec<ButtonRequest>,
) {
    let mut failure = None;
    for request in pending {
        match request {
            ButtonRequest::Expression(source) => {
                let mut functions = EngineFunctions { core: &mut *core };
                if let Err(error) = expressions.evaluate_with(
                    host,
                    Reach {
                        services: &mut *services,
                        views: Some(&mut *views as &mut dyn MetaViewControls),
                        extension: Some(&mut functions),
                    },
                    &source,
                    DEFAULT_INSTRUCTION_BUDGET,
                ) {
                    failure = Some(error.message());
                }
            }
            ButtonRequest::SetState(state) => core.transition(host, services, &state),
        }
    }
    if let Some(failure) = failure {
        views.report_button_failure(host, &failure);
    }
}

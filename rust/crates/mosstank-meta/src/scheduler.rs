//! The macro scheduler (`MacroScheduler`, `MacroRuleTable`, the rule
//! adapters): VTank's list of rules, asked in order every 293 ms
//! heartbeat; the first valid rule wins the pass and runs, every other rule
//! is told it is not running. The meta is looked at inside the pass.
//!
//! The rules and the scheduler's hooks work with the plugin's state for the
//! length of one call. That state borrows the client, so it lives no longer
//! than the call; the scheduler and its rules outlive every call. They are
//! therefore generic over a [`MacroFamily`], a marker naming the state type
//! for any borrow (`F::Cx<'a>`), and each method is handed that call's
//! state.

use std::cell::Cell;
use std::rc::Rc;

use crate::host::MetaHost;

/// What kind of rule fills a slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleShape {
    /// A marker (`MacroRuleSentinel`).
    Sentinel,
    /// A slot with no rule (`AbsentMacroRule`), and why.
    Absent(String),
    /// A controller's tick (`ControllerMacroRule`).
    Controller,
    /// A rule behind gates and fallbacks (`MacroRulePreChain`).
    PreChain,
    /// Peace when idle (`IdlePeaceRule`).
    IdlePeace,
    /// Any other rule, by its type's name.
    Other(&'static str),
}

/// The plugin's state for one call, for every length of call: `Cx<'a>`
/// borrows what it needs for `'a`. The family itself is a marker that
/// outlives every call, so the rules built over it can be kept.
pub trait MacroFamily: 'static {
    /// The state for a call that borrows for `'a`.
    type Cx<'a>: MacroContext;
}

/// The scheduler's hold count and its poke, shared with the owners that
/// hold the pass from inside a rule or a frame (the C#'s `Suspend`,
/// `Resume` and `Poke` delegates): a hold taken during a pass is seen by
/// the scheduler at once, as the C#'s is.
#[derive(Debug, Clone, Default)]
pub struct PassHold {
    suspension: Rc<Cell<i32>>,
    poked: Rc<Cell<bool>>,
    running: Rc<Cell<bool>>,
}

impl PassHold {
    /// Holds the pass while a blocking action is in flight.
    pub fn suspend(&self) {
        self.suspension.set(self.suspension.get() + 1);
    }

    /// Releases one hold; the last one pokes the pass.
    pub fn resume(&self) {
        let count = self.suspension.get() - 1;
        self.suspension.set(count);
        if count > 0 {
            return;
        }
        self.suspension.set(0);
        self.poke();
    }

    /// Asks for a pass now (ignored while the macro is stopped).
    pub fn poke(&self) {
        if self.running.get() {
            self.poked.set(true);
        }
    }

    /// How many holds are up.
    #[must_use]
    pub fn count(&self) -> i32 {
        self.suspension.get()
    }
}

/// What a rule is handed when it is asked whether it is valid: the time
/// since this rule was last asked, and whether it may act (always, from the
/// scheduler).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MacroPassContext {
    /// Seconds since the rule was last asked.
    pub elapsed_seconds: f64,
    /// The rule may act.
    pub can_act: bool,
}

impl MacroPassContext {
    /// A context.
    #[must_use]
    pub fn new(elapsed_seconds: f64, can_act: bool) -> Self {
        Self {
            elapsed_seconds,
            can_act,
        }
    }
}

/// The channels the macro's log lines go to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacroLogChannel {
    /// The pass header and its winner.
    ActiveRule,
    /// What a rule did or why it declined.
    RuleInfo,
    /// Busy state.
    BusyState,
    /// Spell casts.
    SpellCast,
    /// Debuff choices.
    DebuffChoice,
    /// Cast results.
    CastInfo,
    /// What the looter opened, decided and picked up.
    Loot,
    /// Everything else.
    Misc,
    /// Timers.
    Timers,
}

/// One rule of the list.
pub trait MacroRule<F: MacroFamily> {
    /// VTank's friendly name; the scheduler's status and log name.
    fn name(&self) -> String;

    /// Whether the rule would run now.
    fn valid_now(&mut self, cx: &mut F::Cx<'_>, context: MacroPassContext) -> bool;

    /// Whether the rule runs.
    fn running(&self) -> bool;

    /// Tells the rule it runs, or not.
    fn set_running(&mut self, cx: &mut F::Cx<'_>, running: bool);

    /// What the rule appends to its own "Running" line.
    fn running_detail(&mut self, cx: &mut F::Cx<'_>) -> Option<String> {
        let _ = cx;
        None
    }

    /// Why the rule last declined; `None` for a rule with nothing to say.
    fn decline_reason(&mut self, cx: &mut F::Cx<'_>) -> Option<String> {
        let _ = cx;
        None
    }

    /// What kind of rule this is (the C#'s rule types, for the tests that
    /// check which rule fills a slot).
    fn shape(&self) -> RuleShape {
        RuleShape::Other("rule")
    }
}

/// The scheduler's hooks into the plugin.
pub trait MacroContext {
    /// Whether a meta pass is bound.
    fn has_meta_pass(&self) -> bool {
        false
    }

    /// Looks at the meta, with the time since it was last looked at.
    fn meta_pass(&mut self, elapsed_seconds: f64) {
        let _ = elapsed_seconds;
    }

    /// The top of every pass, before the meta and the rules: the per-pass
    /// latches clear here.
    fn pass_starting(&mut self) {}

    /// Whether the log is bound.
    fn has_log(&self) -> bool {
        false
    }

    /// One log line.
    fn log(&mut self, channel: MacroLogChannel, message: &str) {
        let _ = (channel, message);
    }

    /// The lock columns after the "Picked" line.
    fn lock_state_suffix(&mut self) -> String {
        String::new()
    }
}

/// A marker in the list: never valid, never running.
#[derive(Debug, Clone)]
pub struct MacroRuleSentinel {
    /// Its marker.
    pub marker: String,
}

impl MacroRuleSentinel {
    /// A sentinel.
    #[must_use]
    pub fn new(marker: &str) -> Self {
        Self {
            marker: marker.to_owned(),
        }
    }
}

impl<F: MacroFamily> MacroRule<F> for MacroRuleSentinel {
    fn shape(&self) -> RuleShape {
        RuleShape::Sentinel
    }

    fn name(&self) -> String {
        format!("Sentinel {}", self.marker)
    }

    fn valid_now(&mut self, _cx: &mut F::Cx<'_>, _context: MacroPassContext) -> bool {
        false
    }

    fn running(&self) -> bool {
        false
    }

    fn set_running(&mut self, _cx: &mut F::Cx<'_>, _running: bool) {}
}

/// A gate a pre-chain asks first: any `FnMut(&mut F::Cx<'_>) -> bool`.
///
/// The closures a rule is built from are kept behind these small traits
/// rather than as `dyn FnMut` over the family's projection, which the
/// compiler does not coerce to reliably.
pub trait RuleGate<F: MacroFamily> {
    /// Whether the gate is open.
    fn open(&mut self, cx: &mut F::Cx<'_>) -> bool;
}

impl<F: MacroFamily, G> RuleGate<F> for G
where
    G: for<'a, 'b> FnMut(&'b mut F::Cx<'a>) -> bool,
{
    fn open(&mut self, cx: &mut F::Cx<'_>) -> bool {
        self(cx)
    }
}

/// A gate, kept.
pub type MacroGate<F> = Box<dyn RuleGate<F>>;

/// A rule behind gates, with fallbacks that run in its place when valid
/// (`MacroRulePreChain`).
pub struct MacroRulePreChain<F: MacroFamily> {
    primary: Box<dyn MacroRule<F>>,
    gates: Vec<MacroGate<F>>,
    fallbacks: Vec<Box<dyn MacroRule<F>>>,
}

impl<F: MacroFamily> std::fmt::Debug for MacroRulePreChain<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MacroRulePreChain")
    }
}

impl<F: MacroFamily> MacroRulePreChain<F> {
    /// A chain.
    #[must_use]
    pub fn new(
        primary: Box<dyn MacroRule<F>>,
        gates: Vec<MacroGate<F>>,
        fallbacks: Vec<Box<dyn MacroRule<F>>>,
    ) -> Self {
        Self {
            primary,
            gates,
            fallbacks,
        }
    }
}

impl<F: MacroFamily> MacroRule<F> for MacroRulePreChain<F> {
    fn shape(&self) -> RuleShape {
        RuleShape::PreChain
    }

    fn name(&self) -> String {
        self.primary.name()
    }

    fn running_detail(&mut self, cx: &mut F::Cx<'_>) -> Option<String> {
        self.primary.running_detail(cx)
    }

    fn decline_reason(&mut self, cx: &mut F::Cx<'_>) -> Option<String> {
        self.primary.decline_reason(cx)
    }

    fn valid_now(&mut self, cx: &mut F::Cx<'_>, context: MacroPassContext) -> bool {
        for gate in &mut self.gates {
            if !gate.open(cx) {
                return false;
            }
        }
        self.primary.valid_now(cx, context)
    }

    fn running(&self) -> bool {
        self.primary.running() || self.fallbacks.iter().any(|fallback| fallback.running())
    }

    fn set_running(&mut self, cx: &mut F::Cx<'_>, running: bool) {
        if !running {
            for fallback in &mut self.fallbacks {
                fallback.set_running(cx, false);
            }
            self.primary.set_running(cx, false);
            return;
        }
        let context = MacroPassContext::new(0.0, true);
        for fallback in &mut self.fallbacks {
            if !fallback.valid_now(cx, context) {
                continue;
            }
            fallback.set_running(cx, true);
            return;
        }
        self.primary.set_running(cx, true);
    }
}

/// A call a rule makes on an edge: any `FnMut(&mut F::Cx<'_>)`.
pub trait RuleHook<F: MacroFamily> {
    /// Makes the call.
    fn call(&mut self, cx: &mut F::Cx<'_>);
}

impl<F: MacroFamily, H> RuleHook<F> for H
where
    H: for<'a, 'b> FnMut(&'b mut F::Cx<'a>),
{
    fn call(&mut self, cx: &mut F::Cx<'_>) {
        self(cx);
    }
}

/// A call on an edge, kept.
pub type MacroHook<F> = Box<dyn RuleHook<F>>;

/// Text a rule reads off its controller: any
/// `FnMut(&mut F::Cx<'_>) -> Option<String>`.
pub trait RuleText<F: MacroFamily> {
    /// Reads the text.
    fn read(&mut self, cx: &mut F::Cx<'_>) -> Option<String>;
}

impl<F: MacroFamily, T> RuleText<F> for T
where
    T: for<'a, 'b> FnMut(&'b mut F::Cx<'a>) -> Option<String>,
{
    fn read(&mut self, cx: &mut F::Cx<'_>) -> Option<String> {
        self(cx)
    }
}

/// Text, kept.
pub type MacroText<F> = Box<dyn RuleText<F>>;

/// A controller's tick as a rule (`ControllerMacroRule`): any
/// `FnMut(&mut F::Cx<'_>, MacroPassContext) -> bool`.
pub trait RuleTick<F: MacroFamily> {
    /// One tick; whether the controller claims the pass.
    fn tick(&mut self, cx: &mut F::Cx<'_>, context: MacroPassContext) -> bool;
}

impl<F: MacroFamily, T> RuleTick<F> for T
where
    T: for<'a, 'b> FnMut(&'b mut F::Cx<'a>, MacroPassContext) -> bool,
{
    fn tick(&mut self, cx: &mut F::Cx<'_>, context: MacroPassContext) -> bool {
        self(cx, context)
    }
}

/// A tick, kept.
pub type ControllerTick<F> = Box<dyn RuleTick<F>>;

/// A rule whose body is a controller's tick. The gate is its first
/// refusal: while it is closed the controller is not consulted; losing the
/// turn is one call on the edge.
pub struct ControllerMacroRule<F: MacroFamily> {
    name: String,
    tick: ControllerTick<F>,
    gate: Option<MacroGate<F>>,
    on_lost_turn: Option<MacroHook<F>>,
    running_detail: Option<MacroText<F>>,
    decline_reason: Option<MacroText<F>>,
    running: bool,
    gate_closed: bool,
}

impl<F: MacroFamily> std::fmt::Debug for ControllerMacroRule<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ControllerMacroRule({})", self.name)
    }
}

impl<F: MacroFamily> ControllerMacroRule<F> {
    /// A rule named `name` ticking `tick`.
    #[must_use]
    pub fn new(name: &str, tick: ControllerTick<F>) -> Self {
        Self {
            name: name.to_owned(),
            tick,
            gate: None,
            on_lost_turn: None,
            running_detail: None,
            decline_reason: None,
            running: false,
            gate_closed: false,
        }
    }

    /// With a gate.
    #[must_use]
    pub fn with_gate(mut self, gate: MacroGate<F>) -> Self {
        self.gate = Some(gate);
        self
    }

    /// With a call on losing the turn.
    #[must_use]
    pub fn with_lost_turn(mut self, on_lost_turn: MacroHook<F>) -> Self {
        self.on_lost_turn = Some(on_lost_turn);
        self
    }

    /// With a running detail.
    #[must_use]
    pub fn with_running_detail(mut self, detail: MacroText<F>) -> Self {
        self.running_detail = Some(detail);
        self
    }

    /// With a decline reason.
    #[must_use]
    pub fn with_decline_reason(mut self, reason: MacroText<F>) -> Self {
        self.decline_reason = Some(reason);
        self
    }
}

impl<F: MacroFamily> MacroRule<F> for ControllerMacroRule<F> {
    fn shape(&self) -> RuleShape {
        RuleShape::Controller
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn running_detail(&mut self, cx: &mut F::Cx<'_>) -> Option<String> {
        self.running_detail
            .as_mut()
            .and_then(|detail| detail.read(cx))
    }

    /// The controller's own reason, unless the rule's gate is what closed.
    fn decline_reason(&mut self, cx: &mut F::Cx<'_>) -> Option<String> {
        if self.gate_closed {
            return Some("the rule's own gate is closed".to_owned());
        }
        self.decline_reason
            .as_mut()
            .and_then(|reason| reason.read(cx))
    }

    fn valid_now(&mut self, cx: &mut F::Cx<'_>, context: MacroPassContext) -> bool {
        let gate_open = self.gate.as_mut().is_none_or(|gate| gate.open(cx));
        self.gate_closed = !gate_open;
        if !gate_open || !context.can_act {
            return false;
        }
        self.tick
            .tick(cx, MacroPassContext::new(context.elapsed_seconds, true))
    }

    fn running(&self) -> bool {
        self.running
    }

    fn set_running(&mut self, cx: &mut F::Cx<'_>, running: bool) {
        if self.running == running {
            return;
        }
        self.running = running;
        if !running && let Some(on_lost_turn) = self.on_lost_turn.as_mut() {
            on_lost_turn.call(cx);
        }
    }
}

/// A slot whose rule is not there (`AbsentMacroRule`): never valid.
#[derive(Debug, Clone)]
pub struct AbsentMacroRule {
    name: String,
    /// Why it is absent.
    pub reason: String,
}

impl AbsentMacroRule {
    /// An absent rule.
    #[must_use]
    pub fn new(name: &str, reason: &str) -> Self {
        Self {
            name: name.to_owned(),
            reason: reason.to_owned(),
        }
    }
}

impl<F: MacroFamily> MacroRule<F> for AbsentMacroRule {
    fn shape(&self) -> RuleShape {
        RuleShape::Absent(self.reason.clone())
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn valid_now(&mut self, _cx: &mut F::Cx<'_>, _context: MacroPassContext) -> bool {
        false
    }

    fn running(&self) -> bool {
        false
    }

    fn set_running(&mut self, _cx: &mut F::Cx<'_>, _running: bool) {}
}

/// What the peace-when-idle rule reads from the plugin.
pub trait IdlePeaceContext {
    /// The client.
    fn meta_host(&mut self) -> &mut dyn MetaHost;
    /// The profile's IdlePeaceMode switch.
    fn idle_peace_mode(&mut self) -> bool;
}

/// Goes to peace mode when nothing else runs (`IdlePeaceRule`).
#[derive(Debug, Clone, Default)]
pub struct IdlePeaceRule {
    running: bool,
    /// The last request's outcome, for the panel's status line.
    pub status: Option<String>,
}

impl<F: MacroFamily> MacroRule<F> for IdlePeaceRule
where
    for<'a> F::Cx<'a>: IdlePeaceContext,
{
    fn shape(&self) -> RuleShape {
        RuleShape::IdlePeace
    }

    fn name(&self) -> String {
        "IdlePeace".to_owned()
    }

    fn valid_now(&mut self, cx: &mut F::Cx<'_>, context: MacroPassContext) -> bool {
        use ac_plugin_abi::combat::CombatMode;
        if !context.can_act || !cx.idle_peace_mode() {
            return false;
        }
        let mode = cx.meta_host().combat().mode;
        !matches!(mode, CombatMode::Peace | CombatMode::Unknown)
    }

    fn running(&self) -> bool {
        self.running
    }

    fn set_running(&mut self, cx: &mut F::Cx<'_>, running: bool) {
        use ac_plugin_abi::combat::{CombatMode, CombatStatus};
        self.running = running;
        if !running {
            self.status = None;
            return;
        }
        let result = cx.meta_host().enter_combat_mode(CombatMode::Peace);
        self.status = Some(if result.status == CombatStatus::Refused {
            result
                .notice
                .unwrap_or_else(|| "Cannot enter peace mode".to_owned())
        } else {
            "Entering peace mode".to_owned()
        });
    }
}

/// The scheduler.
pub struct MacroScheduler<F: MacroFamily> {
    main: Vec<Box<dyn MacroRule<F>>>,
    independent: Vec<Box<dyn MacroRule<F>>>,
    macro_disabled: Vec<Box<dyn MacroRule<F>>>,
    reported_declines: Vec<(String, String)>,
    until_pass: f64,
    since_pass: f64,
    /// The hold count, the poke and the running flag, shared with the
    /// owners that hold the pass ([`Self::pass_hold`]).
    hold: PassHold,
    /// Holds the pass from outside (the plugin's own suspension).
    pub external_suspension: bool,
    last_executed: Option<usize>,
    pass_count: i64,
    suspended_meta_seconds: f64,
    /// How often the meta is looked at; shorter than the heartbeat, it is
    /// also looked at between passes.
    pub meta_interval_seconds: f64,
    meta_seconds_given_between_passes: f64,
    pass_clock: f64,
    last_evaluated_at: Vec<f64>,
}

impl<F: MacroFamily> std::fmt::Debug for MacroScheduler<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MacroScheduler")
            .field("is_running", &self.hold.running.get())
            .field("pass_count", &self.pass_count)
            .finish_non_exhaustive()
    }
}

/// The scheduler's heartbeat, seconds.
pub const HEARTBEAT_SECONDS: f64 = 0.293;

impl<F: MacroFamily> MacroScheduler<F> {
    /// A scheduler over the main list, the independent list and the list
    /// that runs while the macro is stopped.
    #[must_use]
    pub fn new(
        main: Vec<Box<dyn MacroRule<F>>>,
        independent: Vec<Box<dyn MacroRule<F>>>,
        macro_disabled: Vec<Box<dyn MacroRule<F>>>,
    ) -> Self {
        let count = main.len();
        Self {
            main,
            independent,
            macro_disabled,
            reported_declines: Vec::new(),
            until_pass: 0.0,
            since_pass: 0.0,
            hold: PassHold::default(),
            external_suspension: false,
            last_executed: None,
            pass_count: 0,
            suspended_meta_seconds: 0.0,
            meta_interval_seconds: HEARTBEAT_SECONDS,
            meta_seconds_given_between_passes: 0.0,
            pass_clock: 0.0,
            last_evaluated_at: vec![0.0; count],
        }
    }

    /// The main list.
    #[must_use]
    pub fn main_rules(&self) -> &[Box<dyn MacroRule<F>>] {
        &self.main
    }

    /// A rule of the main list by name, to ask it directly.
    pub fn main_rule_mut(&mut self, name: &str) -> Option<&mut dyn MacroRule<F>> {
        self.main
            .iter_mut()
            .find(|rule| rule.name() == name)
            .map(|rule| &mut **rule as &mut dyn MacroRule<F>)
    }

    /// The independent list.
    #[must_use]
    pub fn independent_rules(&self) -> &[Box<dyn MacroRule<F>>] {
        &self.independent
    }

    /// The stopped-macro list.
    #[must_use]
    pub fn macro_disabled_rules(&self) -> &[Box<dyn MacroRule<F>>] {
        &self.macro_disabled
    }

    /// VTank's single "the macro is running" flag.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.hold.running.get()
    }

    /// Whether the pass is held.
    #[must_use]
    pub fn is_suspended(&self) -> bool {
        self.hold.suspension.get() > 0 || self.external_suspension
    }

    /// The main rule that won the last pass.
    #[must_use]
    pub fn last_executed_index(&self) -> Option<usize> {
        self.last_executed
    }

    /// How many passes ran.
    #[must_use]
    pub fn pass_count(&self) -> i64 {
        self.pass_count
    }

    /// Holds the pass while a blocking action is in flight.
    pub fn suspend(&mut self) {
        self.hold.suspend();
    }

    /// Releases one hold; the last one pokes the pass.
    pub fn resume(&mut self) {
        self.hold.resume();
    }

    /// Asks for a pass now.
    pub fn poke(&mut self) {
        self.hold.poke();
    }

    /// The hold count and the poke, for an owner that holds the pass from
    /// inside a rule or a frame.
    #[must_use]
    pub fn pass_hold(&self) -> PassHold {
        self.hold.clone()
    }

    /// Starts the macro. The hold count belongs to whoever raised it and
    /// is not reset.
    pub fn start(&mut self, cx: &mut F::Cx<'_>) {
        for rule in &mut self.macro_disabled {
            rule.set_running(cx, false);
        }
        self.hold.running.set(true);
        self.until_pass = 0.0;
        self.hold.poked.set(true);
        self.last_executed = None;
        self.suspended_meta_seconds = 0.0;
        self.meta_seconds_given_between_passes = 0.0;
        self.pass_clock = 0.0;
        self.last_evaluated_at.iter_mut().for_each(|at| *at = 0.0);
    }

    /// Stops the macro: every rule is told it is not running.
    pub fn stop(&mut self, cx: &mut F::Cx<'_>) {
        for rule in self.main.iter_mut().chain(self.independent.iter_mut()) {
            rule.set_running(cx, false);
        }
        self.hold.running.set(false);
        self.last_executed = None;
        self.hold.poked.set(false);
        self.until_pass = HEARTBEAT_SECONDS;
    }

    /// One frame's time; true when a pass ran.
    pub fn advance(&mut self, cx: &mut F::Cx<'_>, elapsed_seconds: f64) -> bool {
        let elapsed = elapsed_seconds.max(0.0);
        self.since_pass += elapsed;
        if !self.hold.running.get() {
            if self.macro_disabled.is_empty() {
                self.since_pass = 0.0;
                return false;
            }
            self.until_pass -= elapsed;
            if self.until_pass > 0.0 {
                return false;
            }
            self.until_pass = HEARTBEAT_SECONDS;
            let off_pass = self.since_pass;
            self.since_pass = 0.0;
            self.run_macro_disabled_pass(cx, off_pass);
            return true;
        }

        self.until_pass -= elapsed;
        if self.hold.poked.get() {
            self.hold.poked.set(false);
            self.until_pass = 0.0;
        } else if self.until_pass > 0.0 {
            self.run_meta_between_passes(cx);
            return false;
        }

        self.until_pass = HEARTBEAT_SECONDS;
        let pass_elapsed = self.since_pass;
        self.since_pass = 0.0;
        self.run_pass(cx, pass_elapsed);
        true
    }

    fn run_meta_between_passes(&mut self, cx: &mut F::Cx<'_>) {
        if self.meta_interval_seconds >= HEARTBEAT_SECONDS
            || self.is_suspended()
            || !cx.has_meta_pass()
        {
            return;
        }
        let pending = self.since_pass - self.meta_seconds_given_between_passes;
        if pending < self.meta_interval_seconds {
            return;
        }
        cx.meta_pass(pending);
        self.meta_seconds_given_between_passes += pending;
    }

    /// One pass of the stopped-macro list: first valid rule wins, with no
    /// meta and no independent rules.
    pub fn run_macro_disabled_pass(&mut self, cx: &mut F::Cx<'_>, elapsed_seconds: f64) {
        let elapsed = elapsed_seconds.max(0.0);
        cx.pass_starting();
        if self.is_suspended() {
            return;
        }
        let context = MacroPassContext::new(elapsed, true);
        let mut winner = None;
        for (index, rule) in self.macro_disabled.iter_mut().enumerate() {
            if rule.valid_now(cx, context) {
                winner = Some(index);
                break;
            }
        }
        for (index, rule) in self.macro_disabled.iter_mut().enumerate() {
            if Some(index) != winner {
                rule.set_running(cx, false);
            }
        }
        let Some(winner) = winner else {
            return;
        };
        self.macro_disabled[winner].set_running(cx, true);
        let name = self.macro_disabled[winner].name();
        if cx.has_log() {
            cx.log(MacroLogChannel::RuleInfo, &format!("({name}) Running"));
        }
    }

    /// One pass: the meta, the independent rules, then the main list's
    /// first valid rule.
    pub fn run_pass(&mut self, cx: &mut F::Cx<'_>, elapsed_seconds: f64) {
        let elapsed = elapsed_seconds.max(0.0);
        cx.pass_starting();

        if self.is_suspended() {
            self.suspended_meta_seconds += elapsed;
        } else {
            if cx.has_meta_pass() {
                cx.meta_pass(
                    (elapsed + self.suspended_meta_seconds
                        - self.meta_seconds_given_between_passes)
                        .max(0.0),
                );
            }
            self.suspended_meta_seconds = 0.0;
        }
        self.meta_seconds_given_between_passes = 0.0;

        self.pass_clock += elapsed;
        let independent_context = MacroPassContext::new(elapsed, true);
        for rule in &mut self.independent {
            let valid = rule.valid_now(cx, independent_context);
            rule.set_running(cx, valid);
        }

        if self.is_suspended() {
            return;
        }

        if cx.has_log() {
            cx.log(
                MacroLogChannel::ActiveRule,
                &format!(
                    "----------- Primary logic loop started ({} rules) -----------",
                    self.main.len()
                ),
            );
        }

        // First valid rule wins, in list order, and the scan stops there.
        // Every rule that is asked is handed the time since it was last
        // asked.
        let mut winner = None;
        let mut evaluated = 0;
        for index in 0..self.main.len() {
            let since_asked = self.pass_clock - self.last_evaluated_at[index];
            self.last_evaluated_at[index] = self.pass_clock;
            evaluated = index + 1;
            let context = MacroPassContext::new(since_asked, true);
            if self.main[index].valid_now(cx, context) {
                winner = Some(index);
                break;
            }
        }

        let suffix = cx.lock_state_suffix();
        if cx.has_log() {
            let line = match winner {
                Some(index) => format!("Picked {} P: {index}{suffix}", self.main[index].name()),
                None => format!("All rules inactive.{suffix}"),
            };
            cx.log(MacroLogChannel::ActiveRule, &line);
        }

        for (index, rule) in self.main.iter_mut().enumerate() {
            if Some(index) != winner {
                rule.set_running(cx, false);
            }
        }
        if let Some(index) = winner {
            self.main[index].set_running(cx, true);
            let detail = self.main[index]
                .running_detail(cx)
                .filter(|text| !text.is_empty())
                .map(|text| format!(" {text}"))
                .unwrap_or_default();
            if cx.has_log() {
                let name = self.main[index].name();
                cx.log(
                    MacroLogChannel::RuleInfo,
                    &format!("({name}) Running{detail}"),
                );
            }
        }

        self.report_declines(cx, winner, evaluated);

        self.last_executed = winner;
        self.pass_count += 1;
    }

    /// Says once, per rule, why a rule that could have run did not, and
    /// again only when the reason changes.
    fn report_declines(&mut self, cx: &mut F::Cx<'_>, winner: Option<usize>, evaluated: usize) {
        if !cx.has_log() {
            return;
        }
        for index in 0..evaluated {
            let name = self.main[index].name();
            if Some(index) == winner {
                self.reported_declines.retain(|(rule, _)| *rule != name);
                continue;
            }
            let Some(reason) = self.main[index]
                .decline_reason(cx)
                .filter(|reason| !reason.is_empty())
            else {
                self.reported_declines.retain(|(rule, _)| *rule != name);
                continue;
            };
            if self
                .reported_declines
                .iter()
                .any(|(rule, already)| *rule == name && *already == reason)
            {
                continue;
            }
            self.reported_declines.retain(|(rule, _)| *rule != name);
            self.reported_declines.push((name.clone(), reason.clone()));
            cx.log(
                MacroLogChannel::RuleInfo,
                &format!("({name}) declined: {reason}"),
            );
        }
    }
}

/// A slot of the running list.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacroRuleSlot {
    SplitPeasCritical,
    CraftFoodCritical,
    RechargeSelfNormal,
    RefillWieldedMana,
    BuffSelfNormal,
    SplitPeasNormal,
    DispelSelf,
    UseDispelItem,
    UseHealersHeart,
    RechargeOther,
    DispelAllies,
    CraftFood,
    RefillPetChargesNormal,
    FellowshipManager,
    OpenDoor,
    ReadScrollPriority,
    StackCramPriority,
    SalvageItemsPriority,
    NavigateCorpsePriority,
    OpenCorpsePriority,
    LootCorpsePriority,
    CorpseWaitPriority,
    NavigateRoutePriority,
    Attack,
    SplitPeasIdle,
    CraftFoodIdle,
    RefillPetChargesIdle,
    ReadScrollIdle,
    StackCramIdle,
    SalvageItemsIdle,
    NavigateCorpseIdle,
    OpenCorpseIdle,
    LootCorpseIdle,
    CorpseWaitIdle,
    BuffSelfIdle,
    NavigateMonster,
    RechargeSelfNoTarget,
    NavigateRouteIdle,
    RandomHelper,
    IdlePeace,
    /// The same worn-mana job, in the list that runs while the macro is
    /// stopped. Not part of the running list.
    RefillWieldedManaWhenOff,
}

impl MacroRuleSlot {
    /// The slot's name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::SplitPeasCritical => "SplitPeasCritical",
            Self::CraftFoodCritical => "CraftFoodCritical",
            Self::RechargeSelfNormal => "RechargeSelfNormal",
            Self::RefillWieldedMana => "RefillWieldedMana",
            Self::BuffSelfNormal => "BuffSelfNormal",
            Self::SplitPeasNormal => "SplitPeasNormal",
            Self::DispelSelf => "DispelSelf",
            Self::UseDispelItem => "UseDispelItem",
            Self::UseHealersHeart => "UseHealersHeart",
            Self::RechargeOther => "RechargeOther",
            Self::DispelAllies => "DispelAllies",
            Self::CraftFood => "CraftFood",
            Self::RefillPetChargesNormal => "RefillPetChargesNormal",
            Self::FellowshipManager => "FellowshipManager",
            Self::OpenDoor => "OpenDoor",
            Self::ReadScrollPriority => "ReadScrollPriority",
            Self::StackCramPriority => "StackCramPriority",
            Self::SalvageItemsPriority => "SalvageItemsPriority",
            Self::NavigateCorpsePriority => "NavigateCorpsePriority",
            Self::OpenCorpsePriority => "OpenCorpsePriority",
            Self::LootCorpsePriority => "LootCorpsePriority",
            Self::CorpseWaitPriority => "CorpseWaitPriority",
            Self::NavigateRoutePriority => "NavigateRoutePriority",
            Self::Attack => "Attack",
            Self::SplitPeasIdle => "SplitPeasIdle",
            Self::CraftFoodIdle => "CraftFoodIdle",
            Self::RefillPetChargesIdle => "RefillPetChargesIdle",
            Self::ReadScrollIdle => "ReadScrollIdle",
            Self::StackCramIdle => "StackCramIdle",
            Self::SalvageItemsIdle => "SalvageItemsIdle",
            Self::NavigateCorpseIdle => "NavigateCorpseIdle",
            Self::OpenCorpseIdle => "OpenCorpseIdle",
            Self::LootCorpseIdle => "LootCorpseIdle",
            Self::CorpseWaitIdle => "CorpseWaitIdle",
            Self::BuffSelfIdle => "BuffSelfIdle",
            Self::NavigateMonster => "NavigateMonster",
            Self::RechargeSelfNoTarget => "RechargeSelfNoTarget",
            Self::NavigateRouteIdle => "NavigateRouteIdle",
            Self::RandomHelper => "RandomHelper",
            Self::IdlePeace => "IdlePeace",
            Self::RefillWieldedManaWhenOff => "RefillWieldedManaWhenOff",
        }
    }
}

/// A slot of the independent list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacroIndependentSlot {
    /// Keeps a pet out.
    SummonPet,
}

impl MacroIndependentSlot {
    /// The slot's name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::SummonPet => "SummonPet",
        }
    }
}

/// One row of the list: a sentinel or an acting slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroRuleEntry {
    /// A sentinel marker.
    Sentinel(&'static str),
    /// A slot.
    Rule(MacroRuleSlot),
}

impl MacroRuleEntry {
    /// The row's name.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Sentinel(marker) => format!("Sentinel {marker}"),
            Self::Rule(slot) => slot.name().to_owned(),
        }
    }

    /// The slot, for a rule row.
    #[must_use]
    pub fn slot(self) -> Option<MacroRuleSlot> {
        match self {
            Self::Sentinel(_) => None,
            Self::Rule(slot) => Some(slot),
        }
    }
}

/// Supplies the rule instance behind each slot.
pub trait MacroRuleProvider<F: MacroFamily> {
    /// A running-list slot's rule.
    fn create(&mut self, slot: MacroRuleSlot) -> Box<dyn MacroRule<F>>;
    /// An independent slot's rule.
    fn create_independent(&mut self, slot: MacroIndependentSlot) -> Box<dyn MacroRule<F>>;
}

/// VTank's sixty-six rows, in order.
pub const MACRO_RULE_TABLE: [MacroRuleEntry; 66] = {
    use MacroRuleEntry::{Rule, Sentinel};
    use MacroRuleSlot as S;
    [
        Sentinel("START"),
        Rule(S::SplitPeasCritical),
        Rule(S::CraftFoodCritical),
        Rule(S::RechargeSelfNormal),
        Rule(S::RefillWieldedMana),
        Rule(S::BuffSelfNormal),
        Rule(S::SplitPeasNormal),
        Sentinel("POSTBUFF"),
        Rule(S::DispelSelf),
        Rule(S::UseDispelItem),
        Rule(S::UseHealersHeart),
        Rule(S::RechargeOther),
        Rule(S::DispelAllies),
        Rule(S::CraftFood),
        Rule(S::RefillPetChargesNormal),
        Sentinel("POSTHELPER"),
        Rule(S::FellowshipManager),
        Sentinel("POSTAUTOFELLOW"),
        Rule(S::OpenDoor),
        Sentinel("PREPRIORITYLOOTACTIONS"),
        Rule(S::ReadScrollPriority),
        Rule(S::StackCramPriority),
        Rule(S::SalvageItemsPriority),
        Sentinel("POSTPRIORITYLOOTACTIONS"),
        Sentinel("PREPRIORITYLOOT"),
        Rule(S::NavigateCorpsePriority),
        Rule(S::OpenCorpsePriority),
        Rule(S::LootCorpsePriority),
        Rule(S::CorpseWaitPriority),
        Sentinel("POSTPRIORITYLOOT"),
        Sentinel("PREPRIORITYNAV"),
        Rule(S::NavigateRoutePriority),
        Sentinel("POSTPRIORITYNAV"),
        Sentinel("PREATTACK"),
        Rule(S::Attack),
        Sentinel("POSTATTACK"),
        Sentinel("PREIDLESTATUS"),
        Rule(S::SplitPeasIdle),
        Rule(S::CraftFoodIdle),
        Rule(S::RefillPetChargesIdle),
        Sentinel("PREIDLELOOTACTIONS"),
        Rule(S::ReadScrollIdle),
        Rule(S::StackCramIdle),
        Rule(S::SalvageItemsIdle),
        Sentinel("POSTIDLELOOTACTIONS"),
        Sentinel("PREIDLELOOT"),
        Rule(S::NavigateCorpseIdle),
        Rule(S::OpenCorpseIdle),
        Rule(S::LootCorpseIdle),
        Rule(S::CorpseWaitIdle),
        Sentinel("POSTIDLELOOT"),
        Sentinel("PREIDLEBUFF"),
        Rule(S::BuffSelfIdle),
        Sentinel("POSTIDLEBUFF"),
        Sentinel("PRETARGETAPPROACH"),
        Rule(S::NavigateMonster),
        Sentinel("POSTTARGETAPPROACH"),
        Sentinel("PREIDLERECHARGE"),
        Rule(S::RechargeSelfNoTarget),
        Sentinel("POSTIDLERECHARGE"),
        Sentinel("PRENAVROUTE"),
        Rule(S::NavigateRouteIdle),
        Sentinel("POSTNAVROUTE"),
        Rule(S::RandomHelper),
        Sentinel("END"),
        Rule(S::IdlePeace),
    ]
};

/// The independent list's slots.
pub const INDEPENDENT_TABLE: [MacroIndependentSlot; 1] = [MacroIndependentSlot::SummonPet];

/// Builds the scheduler from the table: the running list, the independent
/// list, and the stopped-macro list (keeping gear charged, the one job the
/// macro still does with everything else switched off).
pub fn build<F: MacroFamily>(provider: &mut dyn MacroRuleProvider<F>) -> MacroScheduler<F> {
    let main = MACRO_RULE_TABLE
        .iter()
        .map(|entry| match entry {
            MacroRuleEntry::Sentinel(marker) => {
                Box::new(MacroRuleSentinel::new(marker)) as Box<dyn MacroRule<F>>
            }
            MacroRuleEntry::Rule(slot) => provider.create(*slot),
        })
        .collect();
    let independent = INDEPENDENT_TABLE
        .iter()
        .map(|slot| provider.create_independent(*slot))
        .collect();
    let macro_disabled = vec![provider.create(MacroRuleSlot::RefillWieldedManaWhenOff)];
    MacroScheduler::new(main, independent, macro_disabled)
}

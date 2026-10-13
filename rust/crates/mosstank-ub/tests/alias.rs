//! The alias list and its interceptor, against `AliasTableTests` and
//! `AliasControllerTests`.

use std::collections::HashMap;
use std::rc::Rc;

use ac_plugin_abi::LogLevel;
use ac_plugin_abi::callback::InterceptDecision;
use mosstank_expr::{ExpressionVariableScope, FixedClock};
use mosstank_meta::runtime::ExpressionRuntime;
use mosstank_meta::services::NoServices;
use mosstank_ub::alias::{
    AliasActionHost, AliasActionKind, AliasController, AliasOutcome, AliasTable,
};
use mosstank_ub::expressions::RuntimeExpressions;
use mosstank_ub::testing::FakeUbHost;

/// A host whose expressions are a lookup table, so the table's decision can
/// be checked without an expression engine behind it.
#[derive(Default)]
struct ScriptedActions {
    results: HashMap<String, String>,
    evaluated: Vec<String>,
    captures: HashMap<String, Option<String>>,
    order: Vec<String>,
}

impl AliasActionHost for ScriptedActions {
    fn set_capture(&mut self, name: &str, value: Option<&str>) {
        self.captures
            .insert(name.to_owned(), value.map(str::to_owned));
        self.order.push(format!("capture {name}"));
    }

    fn evaluate(&mut self, expression: &str) -> Result<String, String> {
        self.evaluated.push(expression.to_owned());
        self.order.push(format!("evaluate {expression}"));
        Ok(self.results.get(expression).cloned().unwrap_or_default())
    }
}

struct ThrowingActions;

impl AliasActionHost for ThrowingActions {
    fn set_capture(&mut self, _name: &str, _value: Option<&str>) {}

    fn evaluate(&mut self, _expression: &str) -> Result<String, String> {
        Err("no such function".to_owned())
    }
}

// ---- The line format ------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasTableTests.cs :: ALineIsPatternSeparatorAction
#[test]
fn a_line_is_pattern_separator_action() {
    for (line, pattern, kind, text, eat) in [
        (
            "^/ubpd$ = /ub propertydump",
            "^/ubpd$",
            AliasActionKind::ChatCommand,
            "/ub propertydump",
            false,
        ),
        (
            "^/lsr$ -> actiontrycastbyid[1635]",
            "^/lsr$",
            AliasActionKind::Expression,
            "actiontrycastbyid[1635]",
            false,
        ),
        (
            "^/tloc (?<name>.*)$ => \\/tell +getvar[capturegroup_name]",
            "^/tloc (?<name>.*)$",
            AliasActionKind::ChatExpression,
            "\\/tell +getvar[capturegroup_name]",
            false,
        ),
        (
            "[eat] ^/ubpd$ = /ub propertydump",
            "^/ubpd$",
            AliasActionKind::ChatCommand,
            "/ub propertydump",
            true,
        ),
        (
            "  [eat]   ^/x$   =   /y  ",
            "^/x$",
            AliasActionKind::ChatCommand,
            "/y",
            true,
        ),
    ] {
        let alias = AliasTable::try_parse_line(line).expect(line);
        assert_eq!(pattern, alias.pattern, "{line}");
        assert_eq!(kind, alias.kind, "{line}");
        assert_eq!(text, alias.text, "{line}");
        assert_eq!(eat, alias.eat, "{line}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasTableTests.cs :: TheEarliestSeparatorSplitsTheLine
#[test]
fn the_earliest_separator_splits_the_line() {
    let alias = AliasTable::try_parse_line("^a$ = b -> c => d").unwrap();
    assert_eq!("^a$", alias.pattern);
    assert_eq!(AliasActionKind::ChatCommand, alias.kind);
    assert_eq!("b -> c => d", alias.text);

    let alias = AliasTable::try_parse_line("^(?=a)b$ -> x = y").unwrap();
    assert_eq!("^(?=a)b$", alias.pattern);
    assert_eq!(AliasActionKind::Expression, alias.kind);
    assert_eq!("x = y", alias.text);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasTableTests.cs :: AMalformedLineIsRefusedWithAReason
#[test]
fn a_malformed_line_is_refused_with_a_reason() {
    for line in [
        "just words",
        "^/x$ =",
        " = /y",
        "[eat] = /y",
        "^(/x$ = /y",
        "",
    ] {
        let problem = AliasTable::try_parse_line(line).expect_err(line);
        assert!(!problem.trim().is_empty(), "{line}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasTableTests.cs :: ABadRegexSaysWhatIsWrongWithIt
#[test]
fn a_bad_regex_says_what_is_wrong_with_it() {
    let problem = AliasTable::try_parse_line("^(/x$ = /y").unwrap_err();
    assert!(problem.contains("^(/x$"), "{problem}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasTableTests.cs :: ALineRoundTrips
#[test]
fn a_line_round_trips() {
    for line in [
        "^/ubpd$ = /ub propertydump",
        "[eat] ^/lsr$ -> actiontrycastbyid[1635]",
        "^(?<start>.*)(?<loc>\\$LOC)(?<end>.*)$ => $capturegroup_start + getplayercoordinates[] + $capturegroup_end",
    ] {
        let alias = AliasTable::try_parse_line(line).unwrap();
        assert_eq!(line, alias.to_line());
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasTableTests.cs :: ParsingReportsEveryBadLineAndKeepsTheGoodOnes
#[test]
fn parsing_reports_every_bad_line_and_keeps_the_good_ones() {
    let table = AliasTable::parse(["^/a$ = /b", "nonsense", "^(/c$ = /d", "^/e$ = /f"]);

    assert_eq!(2, table.aliases.len());
    let patterns: Vec<&str> = table.aliases.iter().map(|a| a.pattern.as_str()).collect();
    assert_eq!(vec!["^/a$", "^/e$"], patterns);
    assert_eq!(2, table.problems.len());
    assert!(table.problems[0].starts_with("Line 2:"));
    assert!(table.problems[1].starts_with("Line 3:"));
}

// ---- Matching ---------------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasTableTests.cs :: ALineNoAliasMatchesPasses
#[test]
fn table_a_line_no_alias_matches_passes() {
    let table = AliasTable::parse(["^/ubpd$ = /ub propertydump"]);
    let result = table.resolve("/ub propertydump", &mut ScriptedActions::default());
    assert_eq!(AliasOutcome::Pass, result.outcome);
    assert_eq!(None, result.rewrite);
    assert!(result.submit.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasTableTests.cs :: AChatCommandThatDoesNotEatRewritesTheLine
#[test]
fn a_chat_command_that_does_not_eat_rewrites_the_line() {
    let table = AliasTable::parse(["^/ubpd$ = /ub propertydump"]);
    let result = table.resolve("/ubpd", &mut ScriptedActions::default());
    assert_eq!(AliasOutcome::Rewrite, result.outcome);
    assert_eq!(Some("/ub propertydump"), result.rewrite.as_deref());
    assert!(result.submit.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasTableTests.cs :: MatchingIgnoresCase
#[test]
fn matching_ignores_case() {
    let table = AliasTable::parse(["^/UBPD$ = /ub propertydump"]);
    assert_eq!(
        AliasOutcome::Rewrite,
        table
            .resolve("/ubpd", &mut ScriptedActions::default())
            .outcome
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasTableTests.cs :: AnEatingAliasSuppressesTheLineAndSubmitsItsOutput
#[test]
fn an_eating_alias_suppresses_the_line_and_submits_its_output() {
    let table = AliasTable::parse(["[eat] ^/ubpd$ = /ub propertydump"]);
    let result = table.resolve("/ubpd", &mut ScriptedActions::default());
    assert_eq!(AliasOutcome::Suppress, result.outcome);
    assert_eq!(None, result.rewrite);
    assert_eq!(vec!["/ub propertydump".to_owned()], result.submit);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasTableTests.cs :: APlainExpressionRunsAndTheLinePassesUnlessEaten
#[test]
fn a_plain_expression_runs_and_the_line_passes_unless_eaten() {
    let mut actions = ScriptedActions::default();
    let table = AliasTable::parse(["^/lsr$ -> actiontrycastbyid[1635]"]);
    let result = table.resolve("/lsr", &mut actions);
    assert_eq!(AliasOutcome::Pass, result.outcome);
    assert_eq!(
        vec!["actiontrycastbyid[1635]".to_owned()],
        actions.evaluated
    );

    let table = AliasTable::parse(["[eat] ^/lsr$ -> actiontrycastbyid[1635]"]);
    let result = table.resolve("/lsr", &mut actions);
    assert_eq!(AliasOutcome::Suppress, result.outcome);
    assert!(result.submit.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasTableTests.cs :: AChatExpressionRewritesTheLineFromItsCaptureGroups
#[test]
fn a_chat_expression_rewrites_the_line_from_its_capture_groups() {
    const EXPRESSION: &str = "$capturegroup_start + getplayercoordinates[] + $capturegroup_end";
    let mut actions = ScriptedActions::default();
    actions.results.insert(
        EXPRESSION.to_owned(),
        "meet me at 12.3N, 45.6E please".to_owned(),
    );
    let table = AliasTable::parse([format!(
        "^(?<start>.*)(?<loc>\\$LOC)(?<end>.*)$ => {EXPRESSION}"
    )]);

    let result = table.resolve("meet me at $LOC please", &mut actions);

    assert_eq!(AliasOutcome::Rewrite, result.outcome);
    assert_eq!(
        Some("meet me at 12.3N, 45.6E please"),
        result.rewrite.as_deref()
    );
    let capture = |name: &str| actions.captures[name].clone();
    assert_eq!(
        Some("meet me at ".to_owned()),
        capture("capturegroup_start")
    );
    assert_eq!(Some("$LOC".to_owned()), capture("capturegroup_loc"));
    assert_eq!(Some(" please".to_owned()), capture("capturegroup_end"));
    assert_eq!(
        Some("meet me at $LOC please".to_owned()),
        capture("capturegroup_0")
    );
    assert_eq!(
        format!("evaluate {EXPRESSION}"),
        *actions.order.last().unwrap()
    );
    for step in &actions.order[..actions.order.len() - 1] {
        assert!(step.starts_with("capture "), "{step}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasTableTests.cs :: ABlankChatExpressionResultSendsNothing
#[test]
fn a_blank_chat_expression_result_sends_nothing() {
    let table = AliasTable::parse(["^/quiet$ => nothing[]"]);
    let result = table.resolve("/quiet", &mut ScriptedActions::default());
    assert_eq!(AliasOutcome::Pass, result.outcome);
    assert!(result.submit.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasTableTests.cs :: AnUnfilledGroupIsCleared
#[test]
fn an_unfilled_group_is_cleared() {
    let mut actions = ScriptedActions::default();
    let table = AliasTable::parse(["^/go(?: (?<where>\\w+))?$ -> 1"]);
    table.resolve("/go", &mut actions);
    assert_eq!(Some(&None), actions.captures.get("capturegroup_where"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasTableTests.cs :: EveryMatchingAliasFiresInOrder
#[test]
fn every_matching_alias_fires_in_order() {
    let mut actions = ScriptedActions::default();
    let table = AliasTable::parse(["^/both$ = /first", "^/bo.*$ -> note[]", "^/both$ = /second"]);

    let result = table.resolve("/both", &mut actions);
    assert_eq!(AliasOutcome::Rewrite, result.outcome);
    assert_eq!(Some("/first"), result.rewrite.as_deref());
    assert_eq!(vec!["/second".to_owned()], result.submit);
    assert_eq!(vec!["note[]".to_owned()], actions.evaluated);

    let table = AliasTable::parse(["^/both$ = /first", "[eat] ^/both$ = /second"]);
    let result = table.resolve("/both", &mut actions);
    assert_eq!(AliasOutcome::Suppress, result.outcome);
    assert_eq!(
        vec!["/first".to_owned(), "/second".to_owned()],
        result.submit
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasTableTests.cs :: AFailingActionIsReportedAndTheRestStillRun
#[test]
fn a_failing_action_is_reported_and_the_rest_still_run() {
    let table = AliasTable::parse(["^/x$ -> boom[]", "^/x$ = /after"]);
    let result = table.resolve("/x", &mut ThrowingActions);
    assert_eq!(AliasOutcome::Rewrite, result.outcome);
    assert_eq!(Some("/after"), result.rewrite.as_deref());
    assert_eq!(1, result.problems.len());
    assert!(result.problems[0].contains("boom[]"));
    assert!(result.problems[0].contains("no such function"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasTableTests.cs :: AnEmptyTablePassesEverything
#[test]
fn an_empty_table_passes_everything() {
    let result = AliasTable::empty().resolve("/anything", &mut ScriptedActions::default());
    assert_eq!(AliasOutcome::Pass, result.outcome);
}

// ---- The controller -----------------------------------------------------------

/// The C# rig: a chat surface that hands the interceptor every line it is
/// asked to submit and records what came out the other end. The client
/// carries out a plugin's submitted lines as typed lines right after the
/// call, so [`Rig::type_line`] hands each one back past the interceptor
/// once the controller returns.
struct Rig {
    host: FakeUbHost,
    runtime: ExpressionRuntime,
    controller: AliasController,
    enabled: bool,
    lines: Vec<String>,
    sent: Vec<String>,
}

impl Rig {
    fn new(lines: &[&str]) -> Self {
        let mut host = FakeUbHost::new();
        let clock = Rc::new(FixedClock::default());
        let runtime = ExpressionRuntime::new(&mut host, Some(1234), clock);
        // The runtime asks for the quest list as it starts; the C# rig's
        // chat has no interceptor yet then, and the line is not this test's.
        host.meta.submitted.clear();
        let mut controller = AliasController::new();
        controller.attach(&mut host).unwrap();
        Self {
            host,
            runtime,
            controller,
            enabled: true,
            lines: lines.iter().map(|line| (*line).to_owned()).collect(),
            sent: Vec::new(),
        }
    }

    fn intercept(&mut self, line: &str) -> InterceptDecision {
        let mut services = NoServices;
        let mut expressions = RuntimeExpressions {
            runtime: &mut self.runtime,
            services: &mut services,
        };
        self.controller.intercept(
            &mut self.host,
            &mut expressions,
            self.enabled,
            &self.lines,
            line,
        )
    }

    /// What the host does with a line the player typed.
    fn type_line(&mut self, line: &str) -> InterceptDecision {
        let decision = self.intercept(line.trim());
        self.deliver();
        decision
    }

    /// The client carries out what the call submitted, as typed lines.
    fn deliver(&mut self) {
        while !self.host.meta.submitted.is_empty() {
            let line = self.host.meta.submitted.remove(0);
            self.submit(&line);
        }
    }

    fn submit(&mut self, text: &str) {
        let line = text.trim().to_owned();
        if line.is_empty() {
            return;
        }
        match self.intercept(&line) {
            InterceptDecision::Suppress => {}
            InterceptDecision::Rewrite(rewrite) => self.submit(&rewrite),
            InterceptDecision::Pass => self.sent.push(line),
        }
    }

    fn logs(&self) -> Vec<String> {
        self.host
            .meta
            .logs
            .iter()
            .map(|(level, line)| format!("{} {line}", level_name(*level)))
            .collect()
    }

    fn session(&mut self, name: &str) -> String {
        let value = self
            .runtime
            .state()
            .get(ExpressionVariableScope::Session, name)
            .unwrap();
        value.to_display_string()
    }
}

fn level_name(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "error",
        LogLevel::Warn => "warn",
        _ => "info",
    }
}

fn is_pass(decision: &InterceptDecision) -> bool {
    matches!(decision, InterceptDecision::Pass)
}

fn rewritten(decision: &InterceptDecision) -> Option<&str> {
    match decision {
        InterceptDecision::Rewrite(text) => Some(text),
        _ => None,
    }
}

fn count_logs(rig: &Rig, needle: &str) -> usize {
    rig.logs()
        .iter()
        .filter(|line| line.contains(needle))
        .count()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasControllerTests.cs :: AttachInstallsOneInterceptorAndDisposeRevokesIt
#[test]
fn attach_installs_one_interceptor_and_dispose_revokes_it() {
    let mut rig = Rig::new(&[]);
    assert_eq!(1, rig.host.interceptors.len());
    rig.controller.attach(&mut rig.host).unwrap();
    assert_eq!(1, rig.host.interceptors.len());

    rig.controller.dispose(&mut rig.host);
    assert!(rig.host.interceptors.is_empty());
    assert_eq!(1, rig.host.interceptors_removed);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasControllerTests.cs :: ALineNoAliasMatchesPasses
#[test]
fn controller_a_line_no_alias_matches_passes() {
    let mut rig = Rig::new(&["^/ubpd$ = /ub propertydump"]);
    let decision = rig.type_line("/ub propertydump");
    assert!(is_pass(&decision));
    assert!(rig.sent.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasControllerTests.cs :: AChatCommandAliasRewritesTheTypedLine
#[test]
fn a_chat_command_alias_rewrites_the_typed_line() {
    let mut rig = Rig::new(&["^/ubpd$ = /ub propertydump"]);
    let decision = rig.type_line("/ubpd");
    assert_eq!(Some("/ub propertydump"), rewritten(&decision));
    assert!(rig.sent.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasControllerTests.cs :: TheEnabledSwitchIsReadOnEveryLine
#[test]
fn the_enabled_switch_is_read_on_every_line() {
    let mut rig = Rig::new(&["^/ubpd$ = /ub propertydump"]);
    rig.enabled = false;
    assert!(is_pass(&rig.type_line("/ubpd")));
    rig.enabled = true;
    assert!(rewritten(&rig.type_line("/ubpd")).is_some());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasControllerTests.cs :: CaptureGroupsAreSessionVariablesTheExpressionReads
#[test]
fn capture_groups_are_session_variables_the_expression_reads() {
    let mut rig = Rig::new(&[r"^/tloc (?<name>.*)$ => \/tell +getvar[capturegroup_name]+\, hello"]);

    let decision = rig.type_line("/tloc Bob");

    assert_eq!(Some("/tell Bob, hello"), rewritten(&decision));
    assert_eq!("Bob", rig.session("capturegroup_name"));
    assert_eq!("/tloc Bob", rig.session("capturegroup_0"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasControllerTests.cs :: AMarkerInsideATypedLineIsRewrittenInPlace
#[test]
fn a_marker_inside_a_typed_line_is_rewritten_in_place() {
    let mut rig = Rig::new(&[
        r"^(?<start>.*)(?<loc>\$LOC)(?<end>.*)$ => $capturegroup_start + `here` + $capturegroup_end",
    ]);

    let decision = rig.type_line("meet me at $LOC please");

    assert_eq!(Some("meet me at here please"), rewritten(&decision));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasControllerTests.cs :: AnEatingAliasSuppressesAndItsOutputIsNotAliasedAgain
#[test]
fn an_eating_alias_suppresses_and_its_output_is_not_aliased_again() {
    let mut rig = Rig::new(&["[eat] ^/echo.*$ = /echo again"]);

    let decision = rig.type_line("/echo");

    assert!(matches!(decision, InterceptDecision::Suppress));
    assert_eq!(vec!["/echo again".to_owned()], rig.sent);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasControllerTests.cs :: APlainExpressionAliasRunsAndTheLinePasses
#[test]
fn a_plain_expression_alias_runs_and_the_line_passes() {
    let mut rig = Rig::new(&["^/setx$ -> setvar[x, 7]"]);
    assert!(is_pass(&rig.type_line("/setx")));
    assert_eq!("7", rig.session("x"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasControllerTests.cs :: ARewriteThatComesBackRoundIsLetThroughOnTheSecondLap
#[test]
fn a_rewrite_that_comes_back_round_is_let_through_on_the_second_lap() {
    let mut rig = Rig::new(&["^/x$ = /y", "^/y$ = /x"]);

    let first = rig.type_line("/x");
    assert_eq!(Some("/y"), rewritten(&first));

    let second = rig.type_line("/y");
    assert!(is_pass(&second));
    assert_eq!(1, count_logs(&rig, "^/x$"));

    // The next thing typed is a new line, not the tail of that chain.
    assert!(rewritten(&rig.type_line("/x")).is_some());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasControllerTests.cs :: ARewriteIntoTheSameLinePassesAtOnce
#[test]
fn a_rewrite_into_the_same_line_passes_at_once() {
    let mut rig = Rig::new(&["^/loop$ = /loop"]);
    assert!(is_pass(&rig.type_line("/loop")));
    assert_eq!(1, count_logs(&rig, "^/loop$"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasControllerTests.cs :: ARewriteMayBecomeAnotherAlias
#[test]
fn a_rewrite_may_become_another_alias() {
    let mut rig = Rig::new(&["^/a$ = /b", "^/b$ = /c"]);
    assert_eq!(Some("/b"), rewritten(&rig.type_line("/a")));
    assert_eq!(Some("/c"), rewritten(&rig.type_line("/b")));
    assert!(is_pass(&rig.type_line("/c")));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasControllerTests.cs :: ExtraOutputsGoOutThroughTheChatBar
#[test]
fn extra_outputs_go_out_through_the_chat_bar() {
    let mut rig = Rig::new(&["^/both$ = /first", "^/both$ = /second"]);
    let decision = rig.type_line("/both");
    assert_eq!(Some("/first"), rewritten(&decision));
    assert_eq!(vec!["/second".to_owned()], rig.sent);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasControllerTests.cs :: TheListIsReadLiveAndBadLinesAreReportedOncePerChange
#[test]
fn the_list_is_read_live_and_bad_lines_are_reported_once_per_change() {
    let mut rig = Rig::new(&[]);
    assert!(is_pass(&rig.type_line("/ubpd")));

    rig.lines = vec![
        "^/ubpd$ = /ub propertydump".to_owned(),
        "^(broken = /x".to_owned(),
    ];
    assert!(rewritten(&rig.type_line("/ubpd")).is_some());
    rig.type_line("/ubpd");
    rig.type_line("/ubpd");
    assert_eq!(1, count_logs(&rig, "Line 2"));

    rig.lines = vec!["^/ubpd$ = /ub propertydump".to_owned()];
    rig.type_line("/ubpd");
    assert_eq!(1, count_logs(&rig, "Line 2"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AliasControllerTests.cs :: AFailingExpressionIsReportedAndTheLinePasses
#[test]
fn a_failing_expression_is_reported_and_the_line_passes() {
    let mut rig = Rig::new(&["^/bad$ => nosuchfunction[]"]);
    assert!(is_pass(&rig.type_line("/bad")));
    assert!(
        rig.logs()
            .iter()
            .any(|line| line.starts_with("error") && line.contains("nosuchfunction"))
    );
}

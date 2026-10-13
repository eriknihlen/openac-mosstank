//! The meta engine: rules, states, calls, chat conditions, warnings, the
//! watchdog. Ported from the C#'s `MetaEngineTests`.

mod common;

use common::*;
use mosstank_expr::{ExpressionValue, Fault};
use mosstank_formats::meta::{
    DEFAULT_STATE, MetaAction, MetaActionKind, MetaCondition, MetaConditionKind, MetaRule,
};
use mosstank_meta::chat::compose_line;
use mosstank_meta::engine::{DECISION_INTERVAL_SECONDS, MAXIMUM_CALL_DEPTH, MetaEngine};
use mosstank_meta::runtime::REPORTED_REPAIR_LIMIT;
use mosstank_meta::testing::FakeHost;

const CHANNEL_KIND: u32 = 2;
const TELL_KIND: u32 = 3;
const LOCAL_SPEECH_KIND: u32 = 0;
const SPEECH_LOG_TEXT_TYPE: u32 = 0x02;
const TELL_LOG_TEXT_TYPE: u32 = 0x03;
const SOCIAL_LOG_TEXT_TYPE: u32 = 0x0A;
const FELLOWSHIP_LOG_TEXT_TYPE: u32 = 0x13;
const FELLOWSHIP_CHANNEL: u32 = 0x800;
const PATRON_CHANNEL: u32 = 0x1000;
const CO_VASSALS_CHANNEL: u32 = 0x100_0000;
const OWN_OBJECT_ID: u32 = 1;

/// The command capture a RynCMD meta puts on every chat command.
const RYN_CMD_COMMAND_CAPTURE: &str = r#"(^(\[[A-z]+?\] |)You|.*\<Tell:IIDString:.+:(?<name>[^\<]*)\>.+\<\\Tell\>) (?<saythink>.*), \"!atk\"$"#;
const RYN_CMD_COMMAND_COLORS: &str = "2;3;4;8;9;10;11;18;19";

fn host() -> FakeHost {
    let mut host = FakeHost::new();
    host.navigation.position.cell_id = 0x7F7F_0001;
    host.navigation.position.east_west = 10.0;
    host.navigation.position.north_south = 20.0;
    host.navigation.position.is_outdoor = true;
    host
}

fn enabled(host: &mut FakeHost, rules: Vec<MetaRule>) -> MetaEngine {
    let mut engine = engine(host, profile(rules));
    engine.set_enabled(true);
    engine
}

fn chat_rule_engine(
    host: &mut FakeHost,
    kind: MetaConditionKind,
    pattern: &str,
    colors: &str,
) -> MetaEngine {
    let rule = MetaRule::new(
        DEFAULT_STATE,
        MetaCondition {
            kind,
            text: pattern.to_owned(),
            secondary_text: colors.to_owned(),
            ..MetaCondition::default()
        },
        action(MetaActionKind::ExpressionAction, "setvar[`matched`,1]"),
    );
    enabled(host, vec![rule])
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: RuleFiresOncePerStateEntryAndCanFireAfterReentry
#[test]
fn rule_fires_once_per_state_entry_and_can_fire_after_reentry() {
    let mut host = host();
    let mut services = TestServices::default();
    let mut engine = enabled(
        &mut host,
        vec![rule(
            MetaConditionKind::Always,
            MetaActionKind::ExpressionAction,
            "setvar[`count`,getvar[`count`]+1]",
        )],
    );

    engine.evaluate_pass(&mut host, &mut services);
    engine.evaluate_pass(&mut host, &mut services);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`count`]"),
        1.0
    );
    assert_eq!(engine.fired_rule_count(), 1);

    engine.transition(&mut host, &mut services, DEFAULT_STATE);
    engine.evaluate_pass(&mut host, &mut services);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`count`]"),
        2.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: StateTransitionStopsTheOldStatesOrderedPass
#[test]
fn state_transition_stops_the_old_states_ordered_pass() {
    let mut host = host();
    let mut services = TestServices::default();
    let mut engine = enabled(
        &mut host,
        vec![
            rule(
                MetaConditionKind::Always,
                MetaActionKind::SetMetaState,
                "Next",
            ),
            rule(
                MetaConditionKind::Always,
                MetaActionKind::ExpressionAction,
                "setvar[`wrong`,1]",
            ),
            rule_in(
                MetaConditionKind::Always,
                MetaActionKind::ExpressionAction,
                "setvar[`right`,1]",
                "Next",
            ),
        ],
    );

    engine.evaluate_pass(&mut host, &mut services);
    assert_eq!(engine.current_state(), "Next");
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`wrong`]"),
        0.0
    );
    engine.evaluate_pass(&mut host, &mut services);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`right`]"),
        1.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: CallAndReturnUseTheVtankReturnStateStack
#[test]
fn call_and_return_use_the_vtank_return_state_stack() {
    let mut host = host();
    let mut services = TestServices::default();
    let mut call = rule(
        MetaConditionKind::Always,
        MetaActionKind::CallMetaState,
        "Worker",
    );
    "ReturnHere".clone_into(&mut call.action.secondary_text);
    let mut engine = enabled(
        &mut host,
        vec![
            call,
            rule_in(
                MetaConditionKind::Always,
                MetaActionKind::ReturnFromCall,
                "",
                "Worker",
            ),
            rule_in(
                MetaConditionKind::Always,
                MetaActionKind::ExpressionAction,
                "setvar[`returned`,1]",
                "ReturnHere",
            ),
        ],
    );

    engine.evaluate_pass(&mut host, &mut services);
    assert_eq!(engine.current_state(), "Worker");
    assert_eq!(engine.call_depth(), 1);
    engine.evaluate_pass(&mut host, &mut services);
    assert_eq!(engine.current_state(), "ReturnHere");
    assert_eq!(engine.call_depth(), 0);
    engine.evaluate_pass(&mut host, &mut services);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`returned`]"),
        1.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: ChatCapturePublishesGroupsAndColorToExpressionVariables
#[test]
fn chat_capture_publishes_groups_and_color_to_expression_variables() {
    let mut host = host();
    let mut services = TestServices::default();
    let mut engine = chat_rule_engine(
        &mut host,
        MetaConditionKind::ChatMessageCapture,
        "^(?<name>.+) tells you, \\\"(?<words>.+)\\\"$",
        "3;4",
    );
    host.chat.push(worded(
        0x8000_0020,
        TELL_KIND,
        "Horan",
        "ready",
        "",
        TELL_LOG_TEXT_TYPE,
        "Horan tells you, \"ready\"",
        0,
    ));

    engine.on_tick(&mut host, &mut services, DECISION_INTERVAL_SECONDS);

    assert_eq!(
        string(
            &mut engine,
            &mut host,
            &mut services,
            "getvar[`capturegroup_name`]"
        ),
        "Horan"
    );
    assert_eq!(
        string(
            &mut engine,
            &mut host,
            &mut services,
            "getvar[`capturegroup_words`]"
        ),
        "ready"
    );
    assert_eq!(
        number(
            &mut engine,
            &mut host,
            &mut services,
            "getvar[`capturecolor`]"
        ),
        3.0
    );
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`matched`]"),
        1.0
    );
}

/// The RynCMD rows: sender, sender id, kind, channel name, channel id, text
/// type, the line the window shows, who and say/think.
#[allow(clippy::type_complexity)]
const RYN_CMD_COMMAND_LINES: [(&str, u32, u32, &str, u32, u32, &str, &str, &str); 6] = [
    (
        "Horan",
        0,
        CHANNEL_KIND,
        "",
        FELLOWSHIP_CHANNEL,
        FELLOWSHIP_LOG_TEXT_TYPE,
        "[Fellowship] <Tell:IIDString:0:Horan>Horan<\\Tell> says, \"!atk\"",
        "Horan",
        "says",
    ),
    (
        "",
        0,
        CHANNEL_KIND,
        "",
        FELLOWSHIP_CHANNEL,
        FELLOWSHIP_LOG_TEXT_TYPE,
        "[Fellowship] You say, \"!atk\"",
        "[Fellowship] You",
        "say",
    ),
    (
        "Horan",
        0x5000_0002,
        TELL_KIND,
        "",
        0,
        TELL_LOG_TEXT_TYPE,
        "<Tell:IIDString:1342177282:Horan>Horan<\\Tell> tells you, \"!atk\"",
        "Horan",
        "tells you",
    ),
    (
        "Meta Tester",
        OWN_OBJECT_ID,
        TELL_KIND,
        "",
        0,
        TELL_LOG_TEXT_TYPE,
        "You think, \"!atk\"",
        "You",
        "think",
    ),
    (
        "You",
        OWN_OBJECT_ID,
        LOCAL_SPEECH_KIND,
        "",
        0,
        SPEECH_LOG_TEXT_TYPE,
        "You say, \"!atk\"",
        "You",
        "say",
    ),
    (
        "Horan",
        0,
        CHANNEL_KIND,
        "Allegiance",
        7,
        0x12,
        "[Allegiance] <Tell:IIDString:0:Horan>Horan<\\Tell> says, \"!atk\"",
        "Horan",
        "says",
    ),
];

fn assert_ryn_cmd_capture(
    message: mosstank_meta::host::ChatLine,
    log_text_type: u32,
    expected_who: &str,
    expected_say_think: &str,
) {
    let mut host = host();
    let mut services = TestServices::default();
    let mut engine = chat_rule_engine(
        &mut host,
        MetaConditionKind::ChatMessageCapture,
        RYN_CMD_COMMAND_CAPTURE,
        RYN_CMD_COMMAND_COLORS,
    );
    host.chat.push(message);

    engine.on_tick(&mut host, &mut services, DECISION_INTERVAL_SECONDS);

    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`matched`]"),
        1.0
    );
    let who = if value(
        &mut engine,
        &mut host,
        &mut services,
        "testvar[`capturegroup_name`]",
    )
    .is_truthy()
    {
        string(
            &mut engine,
            &mut host,
            &mut services,
            "getvar[`capturegroup_name`]",
        )
    } else {
        string(
            &mut engine,
            &mut host,
            &mut services,
            "getvar[`capturegroup_1`]",
        )
    };
    assert_eq!(who, expected_who);
    assert_eq!(
        string(
            &mut engine,
            &mut host,
            &mut services,
            "getvar[`capturegroup_saythink`]"
        ),
        expected_say_think
    );
    assert_eq!(
        number(
            &mut engine,
            &mut host,
            &mut services,
            "getvar[`capturecolor`]"
        ),
        f64::from(log_text_type)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: ChatCaptureSeesTheLineTheReferenceChatWindowShows
#[test]
fn chat_capture_sees_the_line_the_reference_chat_window_shows() {
    for (sender, id, kind, channel, channel_id, text_type, line, who, say_think) in
        RYN_CMD_COMMAND_LINES
    {
        assert_ryn_cmd_capture(
            worded(
                id, kind, sender, "!atk", channel, text_type, line, channel_id,
            ),
            text_type,
            who,
            say_think,
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: AnOlderHostsPartsAreRebuiltIntoTheSameLine
#[test]
fn an_older_hosts_parts_are_rebuilt_into_the_same_line() {
    for (sender, id, kind, channel, _channel_id, text_type, line, who, say_think) in
        RYN_CMD_COMMAND_LINES
    {
        // An older host words nothing and carries no channel number.
        let parts = worded(id, kind, sender, "!atk", channel, text_type, "", 0);
        assert_eq!(compose_line(&parts, OWN_OBJECT_ID, "Meta Tester"), line);
        assert_ryn_cmd_capture(parts, text_type, who, say_think);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: ChatCaptureReadsTheHostsWordingOfANumberedChannel
#[test]
fn chat_capture_reads_the_hosts_wording_of_a_numbered_channel() {
    for (channel_id, display, say_think) in [
        (
            PATRON_CHANNEL,
            "Your patron <Tell:IIDString:0:Horan>Horan<\\Tell> says to you, \"!atk\"",
            "says to you",
        ),
        (
            CO_VASSALS_CHANNEL,
            "[Co-Vassals] <Tell:IIDString:0:Horan>Horan<\\Tell> says, \"!atk\"",
            "says",
        ),
    ] {
        assert_ryn_cmd_capture(
            worded(
                0,
                CHANNEL_KIND,
                "Horan",
                "!atk",
                "",
                SOCIAL_LOG_TEXT_TYPE,
                display,
                channel_id,
            ),
            SOCIAL_LOG_TEXT_TYPE,
            "Horan",
            say_think,
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: ChatMatchReadsTheHostsWordingOverItsParts
#[test]
fn chat_match_reads_the_hosts_wording_over_its_parts() {
    let mut host = host();
    let mut services = TestServices::default();
    let mut engine = chat_rule_engine(
        &mut host,
        MetaConditionKind::ChatMessage,
        "^\\[Fellowship\\] <Tell:IIDString:0:Horan>Horan<\\\\Tell> says, \"go\"$",
        "",
    );
    host.chat.push(worded(
        0,
        CHANNEL_KIND,
        "Horan",
        "go",
        "Somewhere Else",
        FELLOWSHIP_LOG_TEXT_TYPE,
        "[Fellowship] <Tell:IIDString:0:Horan>Horan<\\Tell> says, \"go\"",
        FELLOWSHIP_CHANNEL,
    ));

    engine.on_tick(&mut host, &mut services, DECISION_INTERVAL_SECONDS);

    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`matched`]"),
        1.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: ChatMatchSeesAnNpcTellUnlinkedAndATellToYourselfAsAThought
#[test]
fn chat_match_sees_an_npc_tell_unlinked_and_a_tell_to_yourself_as_a_thought() {
    let mut host = host();
    let mut services = TestServices::default();
    let mut npc = chat_rule_engine(
        &mut host,
        MetaConditionKind::ChatMessage,
        "^Master Arbitrator tells you\\, \"Welcome to Colosseum!\"$",
        "",
    );
    host.chat.push(worded(
        0x8000_1234,
        TELL_KIND,
        "Master Arbitrator",
        "Welcome to Colosseum!",
        "",
        TELL_LOG_TEXT_TYPE,
        "Master Arbitrator tells you, \"Welcome to Colosseum!\"",
        0,
    ));
    npc.on_tick(&mut host, &mut services, DECISION_INTERVAL_SECONDS);
    assert_eq!(
        number(&mut npc, &mut host, &mut services, "getvar[`matched`]"),
        1.0
    );

    let mut thought_host = self::host();
    let mut thought = chat_rule_engine(
        &mut thought_host,
        MetaConditionKind::ChatMessage,
        "^You think, \"Jumper Success\"",
        "",
    );
    thought_host.chat.push(worded(
        OWN_OBJECT_ID,
        TELL_KIND,
        "Meta Tester",
        "Jumper Success",
        "",
        TELL_LOG_TEXT_TYPE,
        "You think, \"Jumper Success\"",
        0,
    ));
    thought.on_tick(&mut thought_host, &mut services, DECISION_INTERVAL_SECONDS);
    assert_eq!(
        number(
            &mut thought,
            &mut thought_host,
            &mut services,
            "getvar[`matched`]"
        ),
        1.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: ChatColorListFiltersOnTheChatTextTypeNotTheHostKind
#[test]
fn chat_color_list_filters_on_the_chat_text_type_not_the_host_kind() {
    const LINE: &str = "[Fellowship] <Tell:IIDString:0:Horan>Horan<\\Tell> says, \"!atk\"";
    let mut services = TestServices::default();
    for (colors, matched) in [("19", 1.0), ("2", 0.0)] {
        let mut host = host();
        let mut engine = chat_rule_engine(
            &mut host,
            MetaConditionKind::ChatMessageCapture,
            "!atk",
            colors,
        );
        host.chat.push(worded(
            0,
            CHANNEL_KIND,
            "Horan",
            "!atk",
            "",
            FELLOWSHIP_LOG_TEXT_TYPE,
            LINE,
            FELLOWSHIP_CHANNEL,
        ));
        engine.on_tick(&mut host, &mut services, DECISION_INTERVAL_SECONDS);
        assert_eq!(
            number(&mut engine, &mut host, &mut services, "getvar[`matched`]"),
            matched,
            "{colors}"
        );
    }
}

/// The C# test sets an odd current culture; the Rust reading is invariant
/// whatever the machine, so the list "+3" is read as 3.
// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: AColourListReadsTheSameInEveryCulture
#[test]
fn a_colour_list_reads_the_same_in_every_culture() {
    let mut host = host();
    let mut services = TestServices::default();
    let mut engine = chat_rule_engine(&mut host, MetaConditionKind::ChatMessage, "ready", "+3");
    host.chat.push(worded(
        0x8000_0020,
        TELL_KIND,
        "Horan",
        "ready",
        "",
        TELL_LOG_TEXT_TYPE,
        "Horan tells you, \"ready\"",
        0,
    ));

    engine.on_tick(&mut host, &mut services, DECISION_INTERVAL_SECONDS);

    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`matched`]"),
        1.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: AFailingExpressionConditionIsFalseWarnsOnceAndThePassGoesOn
#[test]
fn a_failing_expression_condition_is_false_warns_once_and_the_pass_goes_on() {
    let mut host = host();
    let mut services = TestServices::default();
    let mut broken = rule(
        MetaConditionKind::Expression,
        MetaActionKind::ExpressionAction,
        "setvar[`broken`,1]",
    );
    "getvar[a]+".clone_into(&mut broken.condition.text);
    let after = rule(
        MetaConditionKind::Always,
        MetaActionKind::ExpressionAction,
        "setvar[`after`,getvar[`after`]+1]",
    );
    let mut engine = enabled(&mut host, vec![broken, after]);

    engine.evaluate_pass(&mut host, &mut services);
    engine.transition(&mut host, &mut services, DEFAULT_STATE);
    engine.evaluate_pass(&mut host, &mut services);

    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`broken`]"),
        0.0
    );
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`after`]"),
        2.0
    );
    let posted = host.posted_text();
    assert_eq!(posted.len(), 1);
    assert!(posted[0].starts_with("[MossTank] Error in meta expression: getvar[a]+ ("));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: ARuntimeErrorInAnExpressionConditionIsFalseAndThePassGoesOn
#[test]
fn a_runtime_error_in_an_expression_condition_is_false_and_the_pass_goes_on() {
    for source in ["(0/0)&1", "`abc`#`(`"] {
        let mut host = host();
        let mut services = TestServices::default();
        let mut broken = rule(
            MetaConditionKind::Expression,
            MetaActionKind::ExpressionAction,
            "setvar[`broken`,1]",
        );
        source.clone_into(&mut broken.condition.text);
        let after = rule(
            MetaConditionKind::Always,
            MetaActionKind::ExpressionAction,
            "setvar[`after`,1]",
        );
        let mut engine = enabled(&mut host, vec![broken, after]);

        engine.on_tick(&mut host, &mut services, DECISION_INTERVAL_SECONDS);

        assert_eq!(
            number(&mut engine, &mut host, &mut services, "getvar[`broken`]"),
            0.0
        );
        assert_eq!(
            number(&mut engine, &mut host, &mut services, "getvar[`after`]"),
            1.0
        );
        let posted = host.posted_text();
        assert_eq!(posted.len(), 1, "{source}");
        assert!(
            posted[0].starts_with(&format!("[MossTank] Error in meta expression: {source} (")),
            "{}",
            posted[0]
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: AChatPatternThatTimesOutIsFalseAndThePassGoesOn
#[test]
fn a_chat_pattern_that_times_out_is_false_and_the_pass_goes_on() {
    let mut host = host();
    let mut services = TestServices::default();
    let mut slow = rule(
        MetaConditionKind::ChatMessage,
        MetaActionKind::ExpressionAction,
        "setvar[`slow`,1]",
    );
    // The C#'s `^(x+x+)+y$` runs in linear time in the compatibility kit
    // (its engine needs no backtracking there); a backreference makes the
    // kit backtrack until its step limit, its stand-in for .NET's timeout.
    "^(x+x+)+\\1y$".clone_into(&mut slow.condition.text);
    let after = rule(
        MetaConditionKind::Always,
        MetaActionKind::ExpressionAction,
        "setvar[`after`,1]",
    );
    let mut engine = enabled(&mut host, vec![slow, after]);
    let line = "x".repeat(40);
    host.chat.push(worded(
        0x8000_0020,
        TELL_KIND,
        "Horan",
        &line,
        "",
        TELL_LOG_TEXT_TYPE,
        &line,
        0,
    ));

    engine.on_tick(&mut host, &mut services, DECISION_INTERVAL_SECONDS);

    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`slow`]"),
        0.0
    );
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`after`]"),
        1.0
    );
    let posted = host.posted_text();
    assert_eq!(posted.len(), 1);
    assert!(posted[0].starts_with("[MossTank] Error in meta condition ChatMessage: "));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: AFailingExpressionActionWarnsOnceAndStillReturnsTrue
#[test]
fn a_failing_expression_action_warns_once_and_still_returns_true() {
    let mut host = host();
    let mut services = TestServices::default();
    let mut all = rule(MetaConditionKind::Always, MetaActionKind::All, "");
    all.action.children = vec![
        action(MetaActionKind::ExpressionAction, "nosuchfunction[]"),
        action(MetaActionKind::ChatExpression, "nosuchfunction[]"),
        action(MetaActionKind::SetMetaState, "Next"),
    ];
    let mut engine = enabled(&mut host, vec![all]);

    engine.evaluate_pass(&mut host, &mut services);

    assert_eq!(engine.current_state(), "Next");
    // The runtime's own quest refresh is the only line sent.
    assert!(host.submitted.iter().all(|text| text == "/myquests"));
    let posted = host.posted_text();
    assert_eq!(posted.len(), 2);
    assert!(
        posted[0].starts_with("[MossTank] Error in meta expression action: nosuchfunction[] (")
    );
    assert!(
        posted[1]
            .starts_with("[MossTank] Error in meta expression chat action: nosuchfunction[] (")
    );

    engine.transition(&mut host, &mut services, DEFAULT_STATE);
    engine.evaluate_pass(&mut host, &mut services);
    assert_eq!(host.posted.len(), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: AProfileLoadedByAnActionEndsThePass
#[test]
fn a_profile_loaded_by_an_action_ends_the_pass() {
    let mut host = host();
    let cleanup = profile(vec![rule(
        MetaConditionKind::Always,
        MetaActionKind::ExpressionAction,
        "setvar[`cleanupran`,1]",
    )]);
    let mut services = TestServices {
        on_submit: Some(Box::new(move |text| {
            (text == "/vt meta load Cleanup").then(|| cleanup.clone())
        })),
        ..TestServices::default()
    };
    let mut engine = enabled(
        &mut host,
        vec![
            rule(
                MetaConditionKind::Always,
                MetaActionKind::ChatCommand,
                "/vt meta load Cleanup",
            ),
            rule(MetaConditionKind::Always, MetaActionKind::SetMetaState, "X"),
            rule_in(
                MetaConditionKind::Always,
                MetaActionKind::ExpressionAction,
                "setvar[`wrong`,1]",
                "X",
            ),
        ],
    );

    engine.evaluate_pass(&mut host, &mut services);
    assert_eq!(engine.current_state(), DEFAULT_STATE);

    engine.evaluate_pass(&mut host, &mut services);
    assert_eq!(
        number(
            &mut engine,
            &mut host,
            &mut services,
            "getvar[`cleanupran`]"
        ),
        1.0
    );
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`wrong`]"),
        0.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: ADoAllRunsEveryActionAndThePassEndsOnTheStateChange
#[test]
fn a_do_all_runs_every_action_and_the_pass_ends_on_the_state_change() {
    let mut host = host();
    let mut services = TestServices::default();
    let mut all = rule(MetaConditionKind::Always, MetaActionKind::All, "");
    all.action.children = vec![
        action(MetaActionKind::SetMetaState, "Next"),
        action(MetaActionKind::ExpressionAction, "setvar[`after`,1]"),
    ];
    let mut engine = enabled(
        &mut host,
        vec![
            all,
            rule(
                MetaConditionKind::Always,
                MetaActionKind::ExpressionAction,
                "setvar[`wrong`,1]",
            ),
        ],
    );

    engine.evaluate_pass(&mut host, &mut services);

    assert_eq!(engine.current_state(), "Next");
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`after`]"),
        1.0
    );
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`wrong`]"),
        0.0
    );

    let mut loading_host = self::host();
    let picked = profile(vec![rule_in(
        MetaConditionKind::Always,
        MetaActionKind::ExpressionAction,
        "setvar[`picked`,1]",
        "Picked",
    )]);
    let mut loading_services = TestServices {
        on_submit: Some(Box::new(move |_| Some(picked.clone()))),
        ..TestServices::default()
    };
    let mut loading = rule(MetaConditionKind::Always, MetaActionKind::All, "");
    loading.action.children = vec![
        action(MetaActionKind::ChatCommand, "/vt meta load Other"),
        action(MetaActionKind::SetMetaState, "Picked"),
    ];
    let mut loading_engine = enabled(&mut loading_host, vec![loading]);

    loading_engine.evaluate_pass(&mut loading_host, &mut loading_services);
    assert_eq!(loading_engine.current_state(), "Picked");
    loading_engine.evaluate_pass(&mut loading_host, &mut loading_services);
    assert_eq!(
        number(
            &mut loading_engine,
            &mut loading_host,
            &mut loading_services,
            "getvar[`picked`]"
        ),
        1.0
    );
}

/// The C# test registers its own `vtsetmetastate`; the engine carries the
/// function itself here.
// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: AConditionThatChangesTheStateStopsTheOldStatesRules
#[test]
fn a_condition_that_changes_the_state_stops_the_old_states_rules() {
    let mut host = host();
    let mut services = TestServices::default();
    let mut moves = rule(
        MetaConditionKind::Expression,
        MetaActionKind::ExpressionAction,
        "setvar[`moved`,1]",
    );
    "vtsetmetastate[`Other`]==0".clone_into(&mut moves.condition.text);
    let mut engine = enabled(
        &mut host,
        vec![
            moves,
            rule(
                MetaConditionKind::Always,
                MetaActionKind::ExpressionAction,
                "setvar[`wrong`,1]",
            ),
        ],
    );

    engine.evaluate_pass(&mut host, &mut services);

    assert_eq!(engine.current_state(), "Other");
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`moved`]"),
        0.0
    );
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`wrong`]"),
        0.0
    );
}

fn set_opt_rule(option: &str, value: &str) -> MetaRule {
    MetaRule::new(
        DEFAULT_STATE,
        MetaCondition::always(),
        MetaAction {
            kind: MetaActionKind::SetVtankOption,
            text: option.to_owned(),
            secondary_text: value.to_owned(),
            ..MetaAction::default()
        },
    )
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: AFailingSetOptWarnsOnceAndTheDoAllGoesOn
#[test]
fn a_failing_set_opt_warns_once_and_the_do_all_goes_on() {
    for (value_source, warning) in [
        (
            "nosuchfunction[]",
            "Error in Set VT Option meta expression: nosuchfunction[] (",
        ),
        (
            "`text`",
            "SetVTOption Action: Attempted to set AttackDistance with the wrong type of value (",
        ),
        (
            "1",
            "SetVTOption Action: Specified setting doesn't exist. Will not execute.",
        ),
    ] {
        let mut host = host();
        let mut services = TestServices {
            set_option: Some(Box::new(|name, value| match name {
                "AttackDistance" => Ok(value.as_number("AttackDistance")? >= 0.0),
                _ => Ok(false),
            })),
            ..TestServices::default()
        };
        let option = if value_source == "1" {
            "NoSuchOption"
        } else {
            "AttackDistance"
        };
        let mut all = rule(MetaConditionKind::Always, MetaActionKind::All, "");
        all.action.children = vec![
            set_opt_rule(option, value_source).action,
            action(MetaActionKind::SetMetaState, "Next"),
        ];
        let mut engine = enabled(&mut host, vec![all]);

        engine.evaluate_pass(&mut host, &mut services);
        engine.transition(&mut host, &mut services, DEFAULT_STATE);
        engine.evaluate_pass(&mut host, &mut services);

        assert_eq!(engine.current_state(), "Next");
        let posted = host.posted_text();
        assert_eq!(posted.len(), 1, "{value_source}");
        assert!(
            posted[0].starts_with(&format!("[MossTank] {warning}")),
            "{}",
            posted[0]
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: ASetOptOfANameTheReferenceDoesNotHaveIsRefusedBeforeItsValueIsRead
#[test]
fn a_set_opt_of_a_name_the_reference_does_not_have_is_refused_before_its_value_is_read() {
    let mut host = host();
    let written = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let record = written.clone();
    let mut services = TestServices {
        set_option: Some(Box::new(move |name, _| {
            record.borrow_mut().push(name.to_owned());
            Ok(true)
        })),
        ..TestServices::default()
    };
    let mut engine = enabled(
        &mut host,
        vec![set_opt_rule("MonsterRange", "nosuchfunction[]")],
    );

    engine.evaluate_pass(&mut host, &mut services);
    engine.evaluate_pass(&mut host, &mut services);

    assert!(written.borrow().is_empty());
    assert_eq!(
        host.posted_text(),
        ["[MossTank] SetVTOption Action: Specified setting doesn't exist. Will not execute."]
    );
}

fn get_opt_rule(option: &str, variable: &str) -> MetaRule {
    MetaRule::new(
        DEFAULT_STATE,
        MetaCondition::always(),
        MetaAction {
            kind: MetaActionKind::GetVtankOption,
            text: option.to_owned(),
            secondary_text: variable.to_owned(),
            ..MetaAction::default()
        },
    )
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: AGetOptTheReferenceRefusesWritesNoVariable
#[test]
fn a_get_opt_the_reference_refuses_writes_no_variable() {
    for (option, variable, warning) in [
        (
            "MonsterRange",
            "into",
            "GetVTOption Action: Specified setting doesn't exist. Will not execute.",
        ),
        (
            "AttackDistance",
            "",
            "GetVTOption Action: Empty option or variable. Will not execute.",
        ),
        (
            "",
            "into",
            "GetVTOption Action: Empty option or variable. Will not execute.",
        ),
    ] {
        let mut host = host();
        let read = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let record = read.clone();
        let mut services = TestServices {
            get_option: Some(Box::new(move |name| {
                record.borrow_mut().push(name.to_owned());
                ExpressionValue::Number(7.0)
            })),
            ..TestServices::default()
        };
        let mut engine = enabled(&mut host, vec![get_opt_rule(option, variable)]);

        engine.evaluate_pass(&mut host, &mut services);
        engine.evaluate_pass(&mut host, &mut services);

        assert!(read.borrow().is_empty());
        assert_eq!(host.posted_text(), [format!("[MossTank] {warning}")]);
        assert!(!value(&mut engine, &mut host, &mut services, "testvar[`into`]").is_truthy());
        assert!(!value(&mut engine, &mut host, &mut services, "testvar[`option`]").is_truthy());
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: AGetOptOfARealOptionWritesTheNamedVariable
#[test]
fn a_get_opt_of_a_real_option_writes_the_named_variable() {
    let mut host = host();
    let mut services = TestServices {
        get_option: Some(Box::new(|_| ExpressionValue::Number(7.0))),
        ..TestServices::default()
    };
    let mut engine = enabled(&mut host, vec![get_opt_rule("attackdistance", "into")]);

    engine.evaluate_pass(&mut host, &mut services);

    assert!(host.posted.is_empty());
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`into`]"),
        7.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: TheRecordOfReportedRepairsIsBounded
#[test]
fn the_record_of_reported_repairs_is_bounded() {
    let mut host = host();
    let mut expressions = runtime(&mut host);

    eval(&mut expressions, &mut host, "setvar[route,my.route]").unwrap();
    for index in 0..REPORTED_REPAIR_LIMIT {
        eval(
            &mut expressions,
            &mut host,
            &format!("setvar[route{index},my.route]"),
        )
        .unwrap();
    }
    eval(&mut expressions, &mut host, "setvar[route,my.route]").unwrap();

    let warnings = warnings(&host);
    assert_eq!(warnings.len(), REPORTED_REPAIR_LIMIT + 2);
    assert!(warnings[warnings.len() - 1].ends_with("Expression: setvar[route,my.route]"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: AnExpressionTheReaderRepairedIsLoggedOncePerText
#[test]
fn an_expression_the_reader_repaired_is_logged_once_per_text() {
    let mut host = host();
    let mut expressions = runtime(&mut host);

    eval(&mut expressions, &mut host, "setvar[route,my.route]").unwrap();
    eval(&mut expressions, &mut host, "setvar[route,my.route]").unwrap();
    eval(
        &mut expressions,
        &mut host,
        "listfilter[listcreate[1,2],`$1!0&&1`]",
    )
    .unwrap();
    eval(&mut expressions, &mut host, "setvar[plain,1]").unwrap();

    assert_eq!(
        eval_string(&mut expressions, &mut host, "getvar[route]"),
        "my"
    );
    let warnings = warnings(&host);
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(warnings[0].contains("dropped '.r' at 15"));
    assert!(warnings[0].contains("skipped 'oute' at 17"));
    assert!(warnings[0].ends_with("Expression: setvar[route,my.route]"));
    assert!(warnings[1].contains("dropped '!0' at 2"));
    assert!(warnings[1].ends_with("Expression: $1!0&&1"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: WatchdogCallsRecoveryStateOnlyWhenMovementStalls
#[test]
fn watchdog_calls_recovery_state_only_when_movement_stalls() {
    let mut host = host();
    let mut services = TestServices::default();
    let mut set_watchdog = rule(
        MetaConditionKind::Always,
        MetaActionKind::SetWatchdog,
        "Recover",
    );
    set_watchdog.action.number = 5.0;
    set_watchdog.action.secondary_number = 1.0;
    let mut engine = enabled(&mut host, vec![set_watchdog]);
    engine.evaluate_pass(&mut host, &mut services);

    for _ in 0..13 {
        engine.on_tick(&mut host, &mut services, 0.1);
    }

    assert_eq!(engine.current_state(), "Recover");
    assert_eq!(engine.call_depth(), 1);
}

fn call_loop() -> MetaRule {
    let mut call = rule_in(
        MetaConditionKind::Always,
        MetaActionKind::CallMetaState,
        "Loop",
        "Loop",
    );
    "Loop".clone_into(&mut call.action.secondary_text);
    call
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: RecursiveCallOverflowDisablesMetaLikeVtank
#[test]
fn recursive_call_overflow_disables_meta_like_vtank() {
    let mut host = host();
    let mut services = TestServices::default();
    let mut engine = engine(&mut host, profile(vec![call_loop()]));
    engine.transition(&mut host, &mut services, "Loop");
    engine.set_enabled(true);

    for _ in 0..=MAXIMUM_CALL_DEPTH {
        engine.evaluate_pass(&mut host, &mut services);
    }

    assert!(!engine.enabled());
    assert!(
        engine
            .status()
            .to_lowercase()
            .contains("too many nested calls")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaEngineTests.cs :: ARecursiveCallOverflowAlsoClearsTheStoredEnableMetaSetting
#[test]
fn a_recursive_call_overflow_also_clears_the_stored_enable_meta_setting() {
    let mut host = host();
    let written = std::rc::Rc::new(std::cell::RefCell::new(Vec::<(String, bool)>::new()));
    let record = written.clone();
    let mut services = TestServices {
        set_option: Some(Box::new(move |name, value| {
            record
                .borrow_mut()
                .push((name.to_owned(), value.is_truthy()));
            Ok::<bool, Fault>(true)
        })),
        ..TestServices::default()
    };
    let mut engine = engine(&mut host, profile(vec![call_loop()]));
    engine.transition(&mut host, &mut services, "Loop");
    engine.set_enabled(true);

    for _ in 0..=MAXIMUM_CALL_DEPTH {
        engine.evaluate_pass(&mut host, &mut services);
    }

    assert!(!engine.enabled());
    assert_eq!(*written.borrow(), [("EnableMeta".to_owned(), false)]);
}

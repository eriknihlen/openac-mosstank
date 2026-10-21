//! The client's notifications as meta condition edges, the settings page's
//! handler lines over the profile, and the meta window's rule lines.
//! Ported from the C#'s `MetaGameEventTests`, `GameEventHandlersTests` and
//! `MetaRuleDescriptionTests`.

mod common;

use ac_plugin_abi::items::ItemUseCompletion;
use ac_plugin_abi::world::{Confirmation, PortalTransition};
use common::*;
use mosstank_formats::af;
use mosstank_formats::meta::{
    DEFAULT_STATE, MetaAction, MetaActionKind, MetaCondition, MetaConditionKind, MetaProfile,
    MetaRule,
};
use mosstank_meta::engine::{DECISION_INTERVAL_SECONDS, describe_action, describe_condition};
use mosstank_meta::events;
use mosstank_meta::testing::FakeHost;

fn tick(engine: &mut mosstank_meta::engine::MetaEngine, host: &mut FakeHost) {
    engine.on_tick(
        host,
        &mut TestServices::default(),
        DECISION_INTERVAL_SECONDS,
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaGameEventTests.cs :: LoginCompleteFiresWithCharacterVariablesAndReArms
#[test]
fn login_complete_fires_with_character_variables_and_re_arms() {
    let mut host = FakeHost::new();
    let mut services = TestServices::default();
    let mut engine = engine(
        &mut host,
        profile(vec![rule(
            MetaConditionKind::LoginComplete,
            MetaActionKind::ExpressionAction,
            "setvar[`logins`,getvar[`logins`]+1]",
        )]),
    );
    engine.set_enabled(true);

    tick(&mut engine, &mut host);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`logins`]"),
        0.0
    );

    engine.on_login_complete();
    tick(&mut engine, &mut host);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`logins`]"),
        1.0
    );
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`gevt_id`]"),
        1.0
    );
    assert_eq!(
        string(&mut engine, &mut host, &mut services, "getvar[`gevt_name`]"),
        "Meta Tester"
    );

    // No second firing without a second event.
    tick(&mut engine, &mut host);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`logins`]"),
        1.0
    );

    engine.on_login_complete();
    tick(&mut engine, &mut host);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`logins`]"),
        2.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaGameEventTests.cs :: LoginCompleteEdgeSurvivesTheSessionReset
#[test]
fn login_complete_edge_survives_the_session_reset() {
    let mut host = FakeHost::new();
    let mut services = TestServices::default();
    let mut engine = engine(
        &mut host,
        profile(vec![rule(
            MetaConditionKind::LoginComplete,
            MetaActionKind::ExpressionAction,
            "setvar[`logins`,1]",
        )]),
    );

    engine.on_login_complete();
    engine.reset_session(&mut host);
    engine.set_enabled(true);
    tick(&mut engine, &mut host);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`logins`]"),
        1.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaGameEventTests.cs :: LogoffRunsItsPassImmediately
#[test]
fn logoff_runs_its_pass_immediately() {
    let mut host = FakeHost::new();
    let mut services = TestServices::default();
    let mut engine = engine(
        &mut host,
        profile(vec![rule(
            MetaConditionKind::Logoff,
            MetaActionKind::ExpressionAction,
            "setvar[`logoffs`,getvar[`logoffs`]+1]",
        )]),
    );
    engine.set_enabled(true);

    engine.on_logoff(&mut host, &mut services);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`logoffs`]"),
        1.0
    );
    assert_eq!(
        string(&mut engine, &mut host, &mut services, "getvar[`gevt_name`]"),
        "Meta Tester"
    );

    // The edge is consumed by that pass; the next tick does not repeat it.
    tick(&mut engine, &mut host);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`logoffs`]"),
        1.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaGameEventTests.cs :: ContainerEdgesCarryTheContainerId
#[test]
fn container_edges_carry_the_container_id() {
    let mut host = FakeHost::new();
    let mut services = TestServices::default();
    let mut engine = engine(
        &mut host,
        profile(vec![
            rule(
                MetaConditionKind::ContainerOpened,
                MetaActionKind::ExpressionAction,
                "setvar[`opened`,getvar[`gevt_containerid`]]",
            ),
            rule(
                MetaConditionKind::ContainerClosed,
                MetaActionKind::ExpressionAction,
                "setvar[`closed`,getvar[`gevt_containerid`]]",
            ),
        ]),
    );
    engine.set_enabled(true);

    engine.on_container_opened(77);
    tick(&mut engine, &mut host);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`opened`]"),
        77.0
    );
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`closed`]"),
        0.0
    );

    engine.on_container_closed(77);
    tick(&mut engine, &mut host);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`closed`]"),
        77.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaGameEventTests.cs :: EventObjectIdsAreSignedLikeObjectIds
#[test]
fn event_object_ids_are_signed_like_object_ids() {
    let mut host = FakeHost::new();
    let mut services = TestServices::default();
    let mut engine = engine(
        &mut host,
        profile(vec![
            rule(
                MetaConditionKind::ContainerOpened,
                MetaActionKind::ExpressionAction,
                "setvar[`opened`,getvar[`gevt_containerid`]]",
            ),
            rule(
                MetaConditionKind::ItemUseCompleted,
                MetaActionKind::ExpressionAction,
                "setvar[`source`,getvar[`gevt_sourceid`]];setvar[`target`,getvar[`gevt_targetid`]]",
            ),
        ]),
    );
    engine.set_enabled(true);

    engine.on_container_opened(0x8000_1234);
    engine.on_item_use_completed(&ItemUseCompletion {
        revision: 5,
        source_id: 0x8000_08B5,
        target_id: 0x8000_1234,
        weenie_error: 0,
    });
    tick(&mut engine, &mut host);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`opened`]"),
        -2_147_478_988.0
    );
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`source`]"),
        -2_147_481_419.0
    );
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`target`]"),
        -2_147_478_988.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaGameEventTests.cs :: PortalTransitionFiresOnCompletionOnly
#[test]
fn portal_transition_fires_on_completion_only() {
    let mut host = FakeHost::new();
    let mut services = TestServices::default();
    let mut engine = engine(
        &mut host,
        profile(vec![rule(
            MetaConditionKind::PortalTransition,
            MetaActionKind::ExpressionAction,
            "setvar[`arrived`,getvar[`gevt_destinationcell`]]",
        )]),
    );
    engine.set_enabled(true);

    engine.on_portal_transition(&PortalTransition {
        revision: 1,
        generation: 1,
        destination_cell: 0xA9B4_0012,
        is_ready: true,
        ..PortalTransition::default()
    });
    tick(&mut engine, &mut host);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`arrived`]"),
        0.0
    );

    engine.on_portal_transition(&PortalTransition {
        revision: 2,
        generation: 1,
        destination_cell: 0xA9B4_0012,
        is_ready: true,
        is_materialized: true,
        is_completed: true,
        ..PortalTransition::default()
    });
    tick(&mut engine, &mut host);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`arrived`]"),
        f64::from(0xA9B4_0012_u32)
    );
}

/// A short portal hop can begin and end between two ticks, so the client's
/// portal-space flag is never seen on: the portal's own reports still give
/// the entered and exited edges, as the game's portal-mode events do. A
/// login's reports give neither.
#[test]
fn a_portal_reported_between_two_ticks_still_enters_and_exits_portal_space() {
    let mut host = FakeHost::new();
    let mut services = TestServices::default();
    let mut engine = engine(
        &mut host,
        profile(vec![
            rule(
                MetaConditionKind::PortalspaceEntered,
                MetaActionKind::ExpressionAction,
                "setvar[`entered`,1]",
            ),
            rule(
                MetaConditionKind::PortalspaceExited,
                MetaActionKind::ExpressionAction,
                "setvar[`exited`,1]",
            ),
        ]),
    );
    engine.set_enabled(true);

    // A login: no edges.
    engine.on_portal_transition(&PortalTransition {
        revision: 1,
        generation: 1,
        kind: 1,
        ..PortalTransition::default()
    });
    engine.on_portal_transition(&PortalTransition {
        revision: 2,
        generation: 1,
        is_completed: true,
        kind: 1,
        ..PortalTransition::default()
    });
    tick(&mut engine, &mut host);
    assert_eq!(
        number(
            &mut engine,
            &mut host,
            &mut services,
            "getvar[`entered`]+getvar[`exited`]"
        ),
        0.0
    );

    // A portal, begun and arrived before the next tick.
    engine.on_portal_transition(&PortalTransition {
        revision: 3,
        generation: 2,
        destination_cell: 0xB349_0034,
        kind: 2,
        ..PortalTransition::default()
    });
    engine.on_portal_transition(&PortalTransition {
        revision: 4,
        generation: 2,
        destination_cell: 0xB349_0034,
        is_ready: true,
        is_materialized: true,
        is_completed: true,
        kind: 2,
        ..PortalTransition::default()
    });
    tick(&mut engine, &mut host);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`entered`]"),
        1.0
    );
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`exited`]"),
        1.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaGameEventTests.cs :: ItemUseAndConfirmationEdgesExposeTheirFields
#[test]
fn item_use_and_confirmation_edges_expose_their_fields() {
    let mut host = FakeHost::new();
    let mut services = TestServices::default();
    let mut engine = engine(
        &mut host,
        profile(vec![
            rule(
                MetaConditionKind::ItemUseCompleted,
                MetaActionKind::ExpressionAction,
                "setvar[`used`,getvar[`gevt_sourceid`]*1000+getvar[`gevt_error`]]",
            ),
            rule(
                MetaConditionKind::ConfirmationRequested,
                MetaActionKind::ExpressionAction,
                "setvar[`asked`,getvar[`gevt_text`]]",
            ),
        ]),
    );
    engine.set_enabled(true);

    engine.on_item_use_completed(&ItemUseCompletion {
        revision: 5,
        source_id: 42,
        target_id: 0,
        weenie_error: 3,
    });
    engine.on_confirmation_requested(&Confirmation {
        context_id: 9,
        kind: 5,
        text: "Continue?".to_owned(),
    });
    tick(&mut engine, &mut host);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`used`]"),
        42003.0
    );
    assert_eq!(
        string(&mut engine, &mut host, &mut services, "getvar[`asked`]"),
        "Continue?"
    );
    assert_eq!(
        number(
            &mut engine,
            &mut host,
            &mut services,
            "getvar[`gevt_contextid`]"
        ),
        9.0
    );
    assert_eq!(
        number(
            &mut engine,
            &mut host,
            &mut services,
            "getvar[`gevt_confirmtype`]"
        ),
        5.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaGameEventTests.cs :: DeathVariablesComeFromTheNotificationAndTheChatLine
#[test]
fn death_variables_come_from_the_notification_and_the_chat_line() {
    let mut host = FakeHost::new();
    let mut services = TestServices::default();
    let mut engine = engine(&mut host, MetaProfile::default());
    engine.set_enabled(true);

    engine.on_local_player_died("You are liquified by Olthoi Slasher's attack!");
    host.chat.push(worded(
        0,
        0,
        "",
        "You've lost 70 Pyreals, and your 4 Resister's Crystals!",
        "",
        0,
        "",
        0,
    ));
    tick(&mut engine, &mut host);
    assert_eq!(
        string(
            &mut engine,
            &mut host,
            &mut services,
            "getvar[`gevt_deathmessage`]"
        ),
        "You are liquified by Olthoi Slasher's attack!"
    );
    assert_eq!(
        string(
            &mut engine,
            &mut host,
            &mut services,
            "getvar[`gevt_droppeditems`]"
        ),
        "70 Pyreals, and your 4 Resister's Crystals!"
    );

    let mut retained = worded(0, 0, "", "You have retained all your items.", "", 0, "", 0);
    retained.sequence = 2;
    host.chat.push(retained);
    tick(&mut engine, &mut host);
    assert_eq!(
        string(
            &mut engine,
            &mut host,
            &mut services,
            "getvar[`gevt_droppeditems`]"
        ),
        ""
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaGameEventTests.cs :: HandlersSwitchSkipsOnlyHandlerShapedRules
#[test]
fn handlers_switch_skips_only_handler_shaped_rules() {
    let mut host = FakeHost::new();
    let mut services = TestServices {
        handlers_enabled: Some(false),
        ..TestServices::default()
    };
    let mut engine = engine(
        &mut host,
        profile(vec![
            rule(
                MetaConditionKind::LoginComplete,
                MetaActionKind::ExpressionAction,
                "setvar[`handler`,1]",
            ),
            rule(
                MetaConditionKind::LoginComplete,
                MetaActionKind::SetMetaState,
                "Next",
            ),
        ]),
    );
    engine.set_enabled(true);

    engine.on_login_complete();
    engine.on_tick(&mut host, &mut services, DECISION_INTERVAL_SECONDS);
    assert_eq!(
        number(&mut engine, &mut host, &mut services, "getvar[`handler`]"),
        0.0
    );
    assert_eq!(engine.current_state(), "Next");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/GameEventHandlersTests.cs :: LinesRoundTripThroughRules
#[test]
fn lines_round_trip_through_rules() {
    for (line, condition, action, text) in [
        (
            "LoginComplete = /framerate",
            MetaConditionKind::LoginComplete,
            MetaActionKind::ChatCommand,
            "/framerate",
        ),
        (
            "CharacterDeath = chatexpr: \\/f I died",
            MetaConditionKind::CharacterDeath,
            MetaActionKind::ChatExpression,
            "\\/f I died",
        ),
        (
            "Logoff = expr: setvar[`x`,1]",
            MetaConditionKind::Logoff,
            MetaActionKind::ExpressionAction,
            "setvar[`x`,1]",
        ),
        (
            "ContainerOpened = /ub opt",
            MetaConditionKind::ContainerOpened,
            MetaActionKind::ChatCommand,
            "/ub opt",
        ),
    ] {
        let rule = events::try_parse(line).unwrap_or_else(|error| panic!("{line}: {error}"));
        assert_eq!(rule.state, DEFAULT_STATE);
        assert_eq!(rule.condition.kind, condition);
        assert_eq!(rule.action.kind, action);
        assert_eq!(rule.action.text, text);
        assert_eq!(events::format(&rule), line);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/GameEventHandlersTests.cs :: ReferenceEventNamesAreAccepted
#[test]
fn reference_event_names_are_accepted() {
    for (line, expected) in [
        ("Login = /x", MetaConditionKind::LoginComplete),
        ("logout = /x", MetaConditionKind::Logoff),
        ("death = /x", MetaConditionKind::CharacterDeath),
        ("portaltransition = /x", MetaConditionKind::PortalTransition),
        ("ItemUseCompleted = /x", MetaConditionKind::ItemUseCompleted),
        (
            "ConfirmationRequested = /x",
            MetaConditionKind::ConfirmationRequested,
        ),
    ] {
        assert_eq!(
            events::try_parse(line).unwrap().condition.kind,
            expected,
            "{line}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/GameEventHandlersTests.cs :: ABadLineIsRejectedWithAReason
#[test]
fn a_bad_line_is_rejected_with_a_reason() {
    for line in [
        "",
        "no separator",
        "NotAnEvent = /x",
        "LoginComplete =",
        "LoginComplete = expr: setvar[",
    ] {
        let error = events::try_parse(line).expect_err(line);
        assert!(!error.is_empty());
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/GameEventHandlersTests.cs :: ApplyReplacesHandlerRulesAndLeavesTheRestAlone
#[test]
fn apply_replaces_handler_rules_and_leaves_the_rest_alone() {
    let keep_state = rule_in(
        MetaConditionKind::LoginComplete,
        MetaActionKind::ChatCommand,
        "/a",
        "Hunting",
    );
    let keep_shape = rule(
        MetaConditionKind::LoginComplete,
        MetaActionKind::SetMetaState,
        "Next",
    );
    let mut keep_disabled = rule(MetaConditionKind::Logoff, MetaActionKind::ChatCommand, "/b");
    keep_disabled.enabled = false;
    let replaced = rule(
        MetaConditionKind::CharacterDeath,
        MetaActionKind::ChatCommand,
        "/old",
    );
    let mut meta = profile(vec![
        keep_state.clone(),
        replaced.clone(),
        keep_shape.clone(),
        keep_disabled.clone(),
    ]);

    assert_eq!(events::lines(&meta), ["CharacterDeath = /old"]);

    let rejected = events::apply(
        &mut meta,
        &[
            "LoginComplete = /framerate".to_owned(),
            "garbage".to_owned(),
            "Logoff = /bye".to_owned(),
        ],
    );

    assert_eq!(rejected.len(), 1);
    assert!(rejected[0].contains("garbage"));
    assert_eq!(
        events::lines(&meta),
        ["LoginComplete = /framerate", "Logoff = /bye"]
    );
    assert!(meta.rules.contains(&keep_state));
    assert!(meta.rules.contains(&keep_shape));
    assert!(meta.rules.contains(&keep_disabled));
    assert!(!meta.rules.contains(&replaced));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/GameEventHandlersTests.cs :: EventEdgesRoundTripThroughTheProfileFormat
#[test]
fn event_edges_round_trip_through_the_profile_format() {
    let mut meta = MetaProfile::default();
    events::apply(
        &mut meta,
        &[
            "LoginComplete = /a",
            "Logoff = /b",
            "PortalTransition = /c",
            "ItemUseCompleted = /d",
            "ContainerOpened = /e",
            "ContainerClosed = /f",
            "ConfirmationRequested = /g",
        ]
        .map(str::to_owned),
    );
    let text = af::save_meta(&meta, false).unwrap();
    let reloaded = af::try_load_meta(&text, None).unwrap();
    assert_eq!(events::lines(&meta), events::lines(&reloaded));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaRuleDescriptionTests.cs :: ACompoundConditionListsWhatItHolds
#[test]
fn a_compound_condition_lists_what_it_holds() {
    let condition = MetaCondition {
        kind: MetaConditionKind::All,
        children: vec![
            MetaCondition {
                kind: MetaConditionKind::SecondsInStateGreaterThanOrEqual,
                number: 2.0,
                ..MetaCondition::default()
            },
            MetaCondition {
                kind: MetaConditionKind::Not,
                children: vec![MetaCondition {
                    kind: MetaConditionKind::Expression,
                    text: "testvar[x]".to_owned(),
                    ..MetaCondition::default()
                }],
                ..MetaCondition::default()
            },
            MetaCondition::of(MetaConditionKind::NavigationRouteEmpty),
        ],
        ..MetaCondition::default()
    };

    assert_eq!(
        describe_condition(&condition),
        "All (SecondsInStateGreaterThanOrEqual 2; Not Expression: testvar[x]; NavigationRouteEmpty)"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaRuleDescriptionTests.cs :: ADoAllActionListsItsActionsInOrder
#[test]
fn a_do_all_action_lists_its_actions_in_order() {
    let action = MetaAction {
        kind: MetaActionKind::All,
        children: vec![
            action(MetaActionKind::ChatCommand, "/vt opt set enablenav false"),
            action(MetaActionKind::SetMetaState, "loot"),
        ],
        ..MetaAction::default()
    };

    assert_eq!(
        describe_action(&action),
        "All (ChatCommand: /vt opt set enablenav false; SetMetaState: loot)"
    );
}

#[test]
fn a_rule_in_another_state_is_no_handler() {
    let rule = MetaRule::new(
        "Hunting",
        MetaCondition::of(MetaConditionKind::LoginComplete),
        action(MetaActionKind::ChatCommand, "/a"),
    );
    assert!(!events::is_handler(&rule));
}

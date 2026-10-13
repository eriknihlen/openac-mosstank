//! Real metas run through the engine: a native `.met` meta, and the Neftet
//! hunt from its `.af` file; and the object capture a pass shares. Ported
//! from the C#'s `AuthenticMetaExecutionTests`; the panel's option table is
//! stood in for by the options a `/vt opt set` line writes.

mod common;

use std::collections::HashMap;

use ac_plugin_abi::world::{ObjectClass, Position};
use common::*;
use mosstank_expr::{ExpressionValue, Fault};
use mosstank_formats::meta::{
    MetaAction, MetaActionKind, MetaCondition, MetaConditionKind, MetaProfile, MetaRule,
};
use mosstank_formats::{af, fixtures, met};
use mosstank_meta::chat::compose_line;
use mosstank_meta::engine::DECISION_INTERVAL_SECONDS;
use mosstank_meta::host::{ChatLine, MetaHost};
use mosstank_meta::services::MetaServices;
use mosstank_meta::testing::{FakeHost, SharedStorage};
use openac_plugin_compat::text::{casing, ordinal};

/// The panel's options as the meta sees them: `/vt opt set` lines and
/// SetOpt actions write them.
#[derive(Default)]
struct PanelOptions {
    options: HashMap<String, String>,
}

impl PanelOptions {
    fn set(&mut self, name: &str, value: &str) {
        self.options
            .insert(casing::to_lower_invariant(name), value.to_owned());
    }

    fn enabled(&self, name: &str) -> bool {
        self.options
            .get(&casing::to_lower_invariant(name))
            .is_some_and(|value| ordinal::equals_ignore_case(value, "true") || value == "1")
    }
}

impl MetaServices for PanelOptions {
    fn submit_chat(&mut self, host: &mut dyn MetaHost, line: &str) -> Option<MetaProfile> {
        host.chat_submit(line);
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.len() == 5
            && ordinal::equals_ignore_case(words[0], "/vt")
            && ordinal::equals_ignore_case(words[1], "opt")
            && ordinal::equals_ignore_case(words[2], "set")
        {
            self.set(words[3], words[4]);
        }
        None
    }

    fn set_option(
        &mut self,
        _host: &mut dyn MetaHost,
        name: &str,
        value: &ExpressionValue,
    ) -> Result<bool, Fault> {
        self.set(name, if value.is_truthy() { "true" } else { "false" });
        Ok(true)
    }
}

/// The C# test's client: "Meta Tester" with a rock nearby and storage.
fn host() -> FakeHost {
    let mut host = FakeHost::new();
    host.navigation.position = Position {
        cell_id: 0x0001_0001,
        is_outdoor: true,
        ..Position::default()
    };
    host.storage = SharedStorage::new();
    let mut rock = object(10, 1, "Rock", ObjectClass::Misc, 0, 0, 0);
    rock.position = Some(Position {
        cell_id: 0x0001_0001,
        east_west: 10.0,
        north_south: 10.0,
        is_outdoor: true,
        ..Position::default()
    });
    host.objects.push(rock);
    host
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AuthenticMetaExecutionTests.cs :: ControlledNativeMetaTransitionsAndSetsThePanelOptionOwner
#[test]
fn controlled_native_meta_transitions_and_sets_the_panel_option_owner() {
    let fixture = fixtures::read("vt-proof/rynthify-controlled-meta.met");
    let profile = met::try_load(&fixture, None).unwrap();
    assert_eq!(profile.rules.len(), 2);
    let mut host = host();
    let mut panel = PanelOptions::default();
    let mut engine = engine(&mut host, profile);

    panel.set("LootPriorityBoost", "false");
    assert!(!panel.enabled("LootPriorityBoost"));
    engine.set_enabled(true);

    engine.evaluate_pass(&mut host, &mut panel);
    assert_eq!(engine.current_state(), "RynthReady");
    assert!(!panel.enabled("LootPriorityBoost"));

    engine.evaluate_pass(&mut host, &mut panel);
    assert!(panel.enabled("LootPriorityBoost"));
}

fn carrying(kind: MetaConditionKind, name: &str) -> MetaCondition {
    MetaCondition {
        kind,
        text: name.to_owned(),
        number: 5.0,
        ..MetaCondition::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AuthenticMetaExecutionTests.cs :: AMetaPassListsTheObjectsOnceNotOncePerCondition
#[test]
fn a_meta_pass_lists_the_objects_once_not_once_per_condition() {
    let mut host = host();
    let mut services = TestServices::default();
    let rules = ["Mana Stone", "Pyreal", "Arrow", "Healing Kit"]
        .map(|name| {
            MetaRule::new(
                "Default",
                carrying(
                    MetaConditionKind::InventoryItemCountGreaterThanOrEqual,
                    name,
                ),
                MetaAction::default(),
            )
        })
        .to_vec();
    let mut engine = engine(&mut host, profile(rules));
    engine.set_enabled(true);

    let before = host.world_capture_count;
    engine.evaluate_pass(&mut host, &mut services);
    assert_eq!(host.world_capture_count - before, 1);

    let before = host.world_capture_count;
    value(
        &mut engine,
        &mut host,
        &mut services,
        "getitemcountininventorybyname[`Pyreal`]",
    );
    value(
        &mut engine,
        &mut host,
        &mut services,
        "getitemcountininventorybyname[`Pyreal`]",
    );
    assert_eq!(host.world_capture_count - before, 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AuthenticMetaExecutionTests.cs :: AnActionInAMetaPassStartsAFreshObjectList
#[test]
fn an_action_in_a_meta_pass_starts_a_fresh_object_list() {
    let mut host = host();
    let mut services = TestServices::default();
    let mut engine = engine(
        &mut host,
        profile(vec![
            MetaRule::new(
                "Default",
                carrying(
                    MetaConditionKind::InventoryItemCountLessThanOrEqual,
                    "Pyreal",
                ),
                action(MetaActionKind::ExpressionAction, "setvar[x,1]"),
            ),
            MetaRule::new(
                "Default",
                carrying(
                    MetaConditionKind::InventoryItemCountLessThanOrEqual,
                    "Arrow",
                ),
                MetaAction::default(),
            ),
        ]),
    );
    engine.set_enabled(true);

    let before = host.world_capture_count;
    engine.evaluate_pass(&mut host, &mut services);

    assert_eq!(host.world_capture_count - before, 2);
}

/// Someone speaking on the fellowship channel, as the host delivers it.
fn fellowship_speech(sequence: u64, sender: &str, words: &str) -> ChatLine {
    let mut line = worded(
        0,
        2,
        sender,
        words,
        "",
        0x13,
        &format!("[Fellowship] <Tell:IIDString:0:{sender}>{sender}<\\Tell> says, \"{words}\""),
        0x800,
    );
    line.sequence = sequence;
    line
}

fn system_line(text: &str) -> ChatLine {
    worded(0, 4, "", text, "", 0, text, 0)
}

fn neftet() -> MetaProfile {
    af::try_load_meta(&fixtures::read_vtank("af/neftet.af"), None).unwrap()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AuthenticMetaExecutionTests.cs :: FullNeftetHuntCallAndWorkerExecuteWithoutExpressionParseErrors
#[test]
fn full_neftet_hunt_call_and_worker_execute_without_expression_parse_errors() {
    let mut host = host();
    let mut panel = PanelOptions::default();
    let mut engine = engine(&mut host, neftet());

    panel.set("EnableCombat", "false");
    panel.set("IdlePeaceMode", "true");
    for source in [
        "setvar[isLeader,1]",
        "setvar[navRoute,`neftet_route`]",
        "setvar[pickFlowers,0]",
    ] {
        engine.evaluate(&mut host, &mut panel, source).unwrap();
    }
    engine.set_enabled(true);
    engine.transition(&mut host, &mut panel, "Hunt");

    host.add_chat(fellowship_speech(0, "Horan", "#toggle_flowers"));
    engine.on_tick(&mut host, &mut panel, DECISION_INTERVAL_SECONDS + 0.01);

    assert!(panel.enabled("EnableCombat"));
    assert!(!panel.enabled("IdlePeaceMode"));
    assert_eq!(
        engine.current_state(),
        "toggle_flowers",
        "status={}",
        engine.status()
    );

    engine.on_tick(&mut host, &mut panel, DECISION_INTERVAL_SECONDS + 0.01);

    assert!(
        engine
            .evaluate(&mut host, &mut panel, "getvar[pickFlowers]")
            .unwrap()
            .is_truthy()
    );
    assert_eq!(engine.current_state(), "Hunt");
    assert!(!engine.status().to_lowercase().contains("failed"));
}

fn is_scenario_rule(rule: &MetaRule) -> bool {
    (rule.state == "Hunt"
        && rule.action.kind == MetaActionKind::ChatCommand
        && (rule.action.text == "/vt opt set enablecombat true"
            || rule.action.text == "/vt opt set IdlePeaceMode false"))
        || (rule.action.kind == MetaActionKind::CallMetaState
            && rule.action.text == "toggle_flowers")
        || rule.state == "toggle_flowers"
        || rule.condition.children.iter().any(|condition| {
            condition.kind == MetaConditionKind::ChatMessageCapture
                && condition.text.contains("has left your Fellowship")
        })
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AuthenticMetaExecutionTests.cs :: SelectedNeftetRulesRunCapturedChatVtankOptionsAndConfiguredCallReturn
#[test]
fn selected_neftet_rules_run_captured_chat_vtank_options_and_configured_call_return() {
    let mut host = host();
    let mut panel = PanelOptions::default();
    let mut scenario: Vec<MetaRule> = neftet()
        .rules
        .into_iter()
        .filter(is_scenario_rule)
        .collect();
    assert_eq!(scenario.len(), 5);
    let worker = scenario
        .iter()
        .position(|rule| rule.state == "toggle_flowers")
        .unwrap();
    let returns: Vec<MetaAction> = scenario[worker]
        .action
        .children
        .iter()
        .filter(|action| action.kind == MetaActionKind::ReturnFromCall)
        .cloned()
        .collect();
    assert_eq!(returns.len(), 1);
    scenario[worker] = MetaRule::new(
        &scenario[worker].state,
        scenario[worker].condition.clone(),
        MetaAction {
            kind: MetaActionKind::All,
            children: returns,
            ..MetaAction::default()
        },
    );
    let mut engine = engine(&mut host, MetaProfile { rules: scenario });

    panel.set("EnableCombat", "false");
    panel.set("IdlePeaceMode", "true");
    engine
        .evaluate(&mut host, &mut panel, "setvar[isLeader,0]")
        .unwrap();
    engine
        .evaluate(&mut host, &mut panel, "setvar[followTarget,`Horan`]")
        .unwrap();
    engine.set_enabled(true);
    engine.transition(&mut host, &mut panel, "Hunt");

    host.add_chat(system_line("Horan has left your Fellowship"));
    engine.on_tick(&mut host, &mut panel, DECISION_INTERVAL_SECONDS + 0.01);

    assert!(panel.enabled("EnableCombat"));
    assert!(!panel.enabled("IdlePeaceMode"));
    assert_eq!(
        engine
            .evaluate(&mut host, &mut panel, "getvar[`capturegroup_who`]")
            .unwrap()
            .as_str("")
            .unwrap(),
        "Horan"
    );
    assert_eq!(engine.current_state(), "turn_in_quests");

    engine
        .evaluate(&mut host, &mut panel, "setvar[isLeader,1]")
        .unwrap();
    assert_eq!(
        engine
            .evaluate(&mut host, &mut panel, "getvar[isLeader]")
            .unwrap()
            .as_number("")
            .unwrap(),
        1.0
    );
    engine.transition(&mut host, &mut panel, "Hunt");
    const TOGGLE_MESSAGE: &str =
        "[Fellowship] <Tell:IIDString:0:Horan>Horan<\\Tell> says, \"#toggle_flowers\"";
    let call_rule = engine
        .profile()
        .rules
        .iter()
        .find(|rule| {
            rule.action.kind == MetaActionKind::CallMetaState
                && rule.action.text == "toggle_flowers"
        })
        .unwrap()
        .clone();
    assert_eq!(call_rule.action.secondary_text, "Hunt");
    let chat_condition = call_rule
        .condition
        .children
        .iter()
        .find(|condition| condition.kind == MetaConditionKind::ChatMessage)
        .unwrap();
    assert!(
        mosstank_dotnet::compat::regex::DotNetRegex::new(
            &chat_condition.text,
            mosstank_dotnet::compat::regex::RegexOptions::NONE
        )
        .unwrap()
        .is_match(TOGGLE_MESSAGE)
        .unwrap()
    );
    let parts = fellowship_speech(0, "Horan", "#toggle_flowers");
    assert_eq!(compose_line(&parts, 1, "Meta Tester"), TOGGLE_MESSAGE);
    host.add_chat(fellowship_speech(0, "Horan", "#toggle_flowers"));
    engine.on_tick(&mut host, &mut panel, DECISION_INTERVAL_SECONDS + 0.01);
    assert_eq!(
        engine.current_state(),
        "toggle_flowers",
        "status={}; fired={}",
        engine.status(),
        engine.fired_rule_count()
    );

    engine.on_tick(&mut host, &mut panel, DECISION_INTERVAL_SECONDS + 0.01);
    assert_eq!(engine.current_state(), "Hunt");
}

//! The macro's options and log channels through the shell (ported from the
//! C#'s `MossTankPanelTests`).

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::combat::{CombatMode, CombatSnapshot};
use common::World;
use mosstank::MossTank;
use mosstank_expr::{ExpressionValue, ExpressionValueKind};
use mosstank_settings::options::{self, VtankSettingValueType};

fn fresh() -> (World, MossTank) {
    let mut world = World::with_character("Options Tester");
    let plugin = world.enable();
    (world, plugin)
}

fn evaluate(world: &mut World, plugin: &mut MossTank, source: &str) -> ExpressionValue {
    world
        .with_tank(plugin, |tank, client| {
            tank.evaluate_expression(client, source)
        })
        .unwrap_or_else(|error| panic!("{source}: {error}"))
}

fn number(world: &mut World, plugin: &mut MossTank, source: &str) -> f64 {
    evaluate(world, plugin, source).as_number(source).unwrap()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: EveryVtankOptionReadsBackWithTheReferenceType
#[test]
fn every_vtank_option_reads_back_with_the_reference_type() {
    for name in options::NAMES.iter() {
        let (mut world, mut plugin) = fresh();
        let read = format!("vtgetsetting[`{name}`]");
        let value = evaluate(&mut world, &mut plugin, &read);
        match options::declared_type(name) {
            VtankSettingValueType::String | VtankSettingValueType::Custom => {
                assert_eq!(value.kind(), ExpressionValueKind::String, "{name}");
            }
            VtankSettingValueType::Bool => {
                assert_eq!(value.kind(), ExpressionValueKind::Number, "{name}");
                let read_back = value.as_number(name).unwrap();
                let default = if options::default(name).boolean {
                    1.0
                } else {
                    0.0
                };
                assert!((read_back - default).abs() < f64::EPSILON, "{name}");
                let flipped = if read_back == 1.0 { "0" } else { "1" };
                evaluate(
                    &mut world,
                    &mut plugin,
                    &format!("vtsetsetting[`{name}`,`{flipped}`]"),
                );
                let again = number(&mut world, &mut plugin, &read);
                assert!(
                    (again - flipped.parse::<f64>().unwrap()).abs() < f64::EPSILON,
                    "{name}"
                );
            }
            _ => {
                assert_eq!(value.kind(), ExpressionValueKind::Number, "{name}");
                let written = options::default(name).number;
                assert!(
                    (value.as_number(name).unwrap() - written).abs() < 1e-6,
                    "{name}"
                );
                let text = openac_plugin_compat::number::framework::round_trip_f64(written);
                evaluate(
                    &mut world,
                    &mut plugin,
                    &format!("vtsetsetting[`{name}`,`{text}`]"),
                );
                let back = evaluate(&mut world, &mut plugin, &read);
                assert_eq!(back.kind(), ExpressionValueKind::Number, "{name}");
                assert!(
                    (back.as_number(name).unwrap() - written).abs() < 1e-6,
                    "{name}"
                );
            }
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnIntegerVtankSettingComparesWithANumber
#[test]
fn an_integer_vtank_setting_compares_with_a_number() {
    let (mut world, mut plugin) = fresh();
    assert!(
        evaluate(
            &mut world,
            &mut plugin,
            "600<vtgetsetting[IdleBuffTopoffTimeSeconds]"
        )
        .is_truthy()
    );
    assert!(evaluate(&mut world, &mut plugin, "vtgetsetting[AutoStack]==1").is_truthy());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: NavCloseStopRangeKeepsTheRawValueItIsGiven
#[test]
fn nav_close_stop_range_keeps_the_raw_value_it_is_given() {
    let (mut world, mut plugin) = fresh();
    world.vt(&mut plugin, "opt set NavCloseStopRange 0.000625");
    let value = number(&mut world, &mut plugin, "vtgetsetting[`NavCloseStopRange`]");
    assert!((value - 0.000_625).abs() < 1e-9, "{value}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: RunMacroAndEnableCombatAreIndependentVtankStates
#[test]
fn run_macro_and_enable_combat_are_independent_vtank_states() {
    let (mut world, mut plugin) = fresh();
    assert!(plugin.tank().unwrap().option("EnableCombat").is_truthy());

    world.vt(&mut plugin, "start");
    world.vt(&mut plugin, "opt toggle EnableCombat");
    world.tick(&mut plugin);

    assert!(plugin.tank().unwrap().macro_running());
    assert!(!plugin.tank().unwrap().option("EnableCombat").is_truthy());
    assert!(evaluate(&mut world, &mut plugin, "vtmacroenabled[]").is_truthy());
    assert!(!evaluate(&mut world, &mut plugin, "uboptget[`EnableCombat`]").is_truthy());
}

/// The settings file the macro profile of a named character lives in.
fn settings_key(world: &World, character: &str) -> String {
    let wanted = format!("mosstank/profiles/--{character}");
    world
        .profiles
        .keys()
        .find(|key| key.starts_with(&wanted) && key.ends_with(".usd"))
        .unwrap_or_else(|| panic!("no settings file for {character}"))
        .to_owned()
}

/// Whether a settings file says EnableMeta is on (`StoredEnableMeta`).
fn stored_enable_meta(text: &str) -> bool {
    let database = mosstank_formats::usd::VtankDatabase::parse(text).unwrap();
    let mut settings = mosstank_settings::model::AllSettings::default();
    mosstank_settings::profile::apply_database(&database, &mut settings, None).unwrap();
    settings.meta.enabled
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheMetaCheckboxIsStoredInTheProfileAndOutlivesReloadAndReconnect
#[test]
fn the_meta_checkbox_is_stored_in_the_profile_and_outlives_reload_and_reconnect() {
    let mut world = World::with_character("Metaphile");
    let mut plugin = world.enable();
    assert!(!plugin.tank().unwrap().meta_enabled());

    // The Meta tab's checkbox writes the option (`ToggleMeta`).
    world.vt(&mut plugin, "opt toggle EnableMeta");

    assert!(plugin.tank().unwrap().meta_enabled());
    let key = settings_key(&world, "Metaphile");
    assert!(stored_enable_meta(world.profiles.get(&key).unwrap()));

    world.fake.character = None;
    world.tick(&mut plugin);
    world.fake.character = Some(common::character("Metaphile"));
    world.tick(&mut plugin);
    assert!(plugin.tank().unwrap().meta_enabled());

    world.disable(&mut plugin);
    let reloaded = world.enable();
    assert!(reloaded.tank().unwrap().meta_enabled());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: OfficialVtankOptionDefaultsAndDynamicOverridesAreDurable
#[test]
fn official_vtank_option_defaults_and_dynamic_overrides_are_durable() {
    let (mut world, mut first) = fresh();
    assert_eq!(options::NAMES.len(), 137);
    assert!(
        (number(
            &mut world,
            &mut first,
            "uboptget[`ArrowheadFletchDiffExcessThreshold`]"
        ) - 10.0)
            .abs()
            < 1e-9
    );
    assert!(
        (number(&mut world, &mut first, "uboptget[`DoorIDRange`]") - 0.083_333_333_333_333_3).abs()
            < 1e-14
    );
    assert!(evaluate(&mut world, &mut first, "uboptget[`ManaChargesWhenOff`]").is_truthy());
    for (name, expected) in [
        ("SpellCompMin-Critical", 4.0),
        ("SpellCompMin-Normal", 20.0),
        ("SpellCompMin-Idle", 20.0),
        ("IdleCraftCount_HealthKits", 2.0),
        ("IdleCraftCount_ManaFood", 15.0),
    ] {
        let value = number(&mut world, &mut first, &format!("uboptget[`{name}`]"));
        assert!((value - expected).abs() < 1e-9, "{name} = {value}");
    }
    assert!(
        evaluate(
            &mut world,
            &mut first,
            "uboptset[`ArrowheadFletchDiffExcessThreshold`,22]"
        )
        .is_truthy()
    );
    assert!(
        evaluate(
            &mut world,
            &mut first,
            "uboptset[`IdleCraftCount_HealthKits`,7]"
        )
        .is_truthy()
    );

    world.disable(&mut first);
    let mut second = world.enable();
    assert!(
        (number(
            &mut world,
            &mut second,
            "uboptget[`arrowheadfletchdiffexcessthreshold`]"
        ) - 22.0)
            .abs()
            < 1e-9
    );
    assert!(
        (number(
            &mut world,
            &mut second,
            "uboptget[`idlecraftcount_healthkits`]"
        ) - 7.0)
            .abs()
            < 1e-9
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtankSetInAllRewritesEveryKnownMacroProfile
#[test]
fn vtank_set_in_all_rewrites_every_known_macro_profile() {
    let (mut world, mut plugin) = fresh();
    world.vt(&mut plugin, "settings save First");
    world.vt(&mut plugin, "opt set AttackDistance 0.01");
    world.vt(&mut plugin, "settings save Second");
    world.vt(&mut plugin, "opt set AttackDistance 0.02");
    world.vt(&mut plugin, "opt setinall AttackDistance 0.03");
    world.vt(&mut plugin, "settings load First");
    assert!((number(&mut world, &mut plugin, "uboptget[`AttackDistance`]") - 0.03).abs() < 1e-7);
    world.vt(&mut plugin, "settings load Second");
    assert!((number(&mut world, &mut plugin, "uboptget[`AttackDistance`]") - 0.03).abs() < 1e-7);

    world.disable(&mut plugin);
    let mut reloaded = world.enable();
    world.vt(&mut reloaded, "settings load First");
    assert!((number(&mut world, &mut reloaded, "uboptget[`AttackDistance`]") - 0.03).abs() < 1e-7);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtankSetInAllReportsAuthenticExactMessageText
#[test]
fn vtank_set_in_all_reports_authentic_exact_message_text() {
    let (mut world, mut plugin) = fresh();
    world.vt(&mut plugin, "opt setinall AttackDistance 0.03");
    let lines: Vec<String> = world
        .chat()
        .into_iter()
        .filter(|line| line.starts_with("[MossTank] Done saving setting"))
        .collect();
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].starts_with(
            "[MossTank] Done saving setting AttackDistance to all profiles. (Changed "
        )
    );
    assert!(lines[0].ends_with(" profiles)"));
    assert!(!lines[0].contains('='));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtankOptSetRejectsAWrongTypedValueWithAuthenticExactText
#[test]
fn vtank_opt_set_rejects_a_wrong_typed_value_with_authentic_exact_text() {
    for operation in ["set", "setinall"] {
        let (mut world, mut plugin) = fresh();
        world.clear_chat();
        world.vt(
            &mut plugin,
            &format!("opt {operation} EnableLooting notaboolean"),
        );
        assert_eq!(
            world.chat(),
            ["[MossTank] Option set: bad value. EnableLooting takes a System.Boolean."],
            "{operation}"
        );
    }
}

/// In melee, peace-when-idle on, the `/vt log` channel `channel` on.
fn idle_in_melee(channel: Option<&str>) -> (World, MossTank) {
    let (mut world, mut plugin) = fresh();
    world.set_reply::<calls::CombatSnapshot>(&CombatSnapshot {
        mode: CombatMode::Melee,
        ..CombatSnapshot::default()
    });
    world.vt(&mut plugin, "opt set IdlePeaceMode true");
    if let Some(channel) = channel {
        world.vt(&mut plugin, &format!("log {channel} on"));
    }
    world.clear_chat();
    (world, plugin)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtLogActiveRuleOnPostsThePickedLineNamingTheWinner
#[test]
fn vt_log_active_rule_on_posts_the_picked_line_naming_the_winner() {
    let (mut world, mut plugin) = idle_in_melee(Some("ActiveRule"));
    world.vt(&mut plugin, "start");
    world.tick(&mut plugin);
    let chat = world.chat();
    assert!(
        chat.iter()
            .any(|line| line == "[MossTank] Picked IdlePeace P: 65   I=False, N=False, S=False"),
        "{chat:?}"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtLogActiveRuleOnPostsAllRulesInactiveWhenNothingIsValid
#[test]
fn vt_log_active_rule_on_posts_all_rules_inactive_when_nothing_is_valid() {
    let (mut world, mut plugin) = fresh();
    world.set_reply::<calls::CombatSnapshot>(&CombatSnapshot {
        mode: CombatMode::Peace,
        ..CombatSnapshot::default()
    });
    world.vt(&mut plugin, "opt set IdlePeaceMode true");
    world.vt(&mut plugin, "log ActiveRule on");
    world.clear_chat();
    world.vt(&mut plugin, "start");
    world.tick(&mut plugin);
    let chat = world.chat();
    assert!(
        chat.iter()
            .any(|line| line == "[MossTank] All rules inactive.   I=False, N=False, S=False"),
        "{chat:?}"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtLogActiveRuleOffPostsNothing
#[test]
fn vt_log_active_rule_off_posts_nothing() {
    let (mut world, mut plugin) = idle_in_melee(None);
    world.vt(&mut plugin, "start");
    world.tick(&mut plugin);
    assert!(
        !world
            .chat()
            .iter()
            .any(|line| line.starts_with("[MossTank] Picked")
                || line.starts_with("[MossTank] All rules inactive"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtLogRuleInfoOnPostsRuleRunningLine
#[test]
fn vt_log_rule_info_on_posts_rule_running_line() {
    let (mut world, mut plugin) = idle_in_melee(Some("RuleInfo"));
    world.vt(&mut plugin, "start");
    world.tick(&mut plugin);
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line == "[MossTank] (IdlePeace) Running"),
        "{:?}",
        world.chat()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtLogOffStopsPostingAfterHavingBeenOn
#[test]
fn vt_log_off_stops_posting_after_having_been_on() {
    let (mut world, mut plugin) = idle_in_melee(Some("ActiveRule"));
    world.vt(&mut plugin, "start");
    world.tick(&mut plugin);
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.starts_with("[MossTank] Picked"))
    );

    world.vt(&mut plugin, "log ActiveRule off");
    world.clear_chat();
    world.ticks(&mut plugin, 30);

    assert!(
        !world
            .chat()
            .iter()
            .any(|line| line.starts_with("[MossTank] Picked"))
    );
}

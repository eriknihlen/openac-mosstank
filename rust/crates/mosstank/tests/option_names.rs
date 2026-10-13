//! The macro's option names and the UB message displays (ported from the
//! C#'s `MossTankPanelTests.VtankOptionNames`, `.OptionParity` and
//! `.ExpressionErrors`).

mod common;

use common::World;
use mosstank::MossTank;
use mosstank_expr::ExpressionValue;
use mosstank_meta::chat::UB_ERROR_CHAT_TYPE;
use mosstank_settings::ub::definitions;

fn fresh(name: &str) -> (World, MossTank) {
    let mut world = World::with_character(name);
    let plugin = world.enable();
    world.clear_chat();
    (world, plugin)
}

fn set(world: &mut World, plugin: &mut MossTank, name: &str, value: ExpressionValue) -> bool {
    world
        .with_tank(plugin, |tank, client| {
            tank.set_meta_option(client, name, &value)
        })
        .unwrap()
}

fn evaluate(
    world: &mut World,
    plugin: &mut MossTank,
    source: &str,
) -> Result<ExpressionValue, String> {
    world.with_tank(plugin, |tank, client| {
        tank.evaluate_expression(client, source)
    })
}

fn posted(world: &World) -> Vec<(String, u32)> {
    world
        .fake
        .posted
        .iter()
        .map(|post| (post.text.clone(), post.log_text_type))
        .collect()
}

// ---- MossTankPanelTests.VtankOptionNames ---------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.VtankOptionNames.cs :: TheOptionWritePathRefusesANameTheReferenceDoesNotHold
#[test]
fn the_option_write_path_refuses_a_name_the_reference_does_not_hold() {
    let (mut world, mut plugin) = fresh("Options Tester");

    assert!(!set(
        &mut world,
        &mut plugin,
        "MonsterRange",
        ExpressionValue::Number(42.0)
    ));
    assert!(!set(
        &mut world,
        &mut plugin,
        "MyOwnFlag",
        ExpressionValue::Boolean(true)
    ));
    assert!(set(
        &mut world,
        &mut plugin,
        "enablecombat",
        ExpressionValue::Boolean(false)
    ));
    assert!(!plugin.tank().unwrap().option("EnableCombat").is_truthy());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.VtankOptionNames.cs :: VtSettingFunctionsTreatAnUnknownNameAsTheReferenceDoes
#[test]
fn vt_setting_functions_treat_an_unknown_name_as_the_reference_does() {
    let (mut world, mut plugin) = fresh("Options Tester");

    let written = evaluate(&mut world, &mut plugin, "vtsetsetting[`MyOwnFlag`,`1`]").unwrap();
    assert_eq!(written.as_number("vtsetsetting").unwrap(), 1.0);
    let error = evaluate(&mut world, &mut plugin, "vtgetsetting[`MyOwnFlag`]").unwrap_err();
    assert!(
        error.contains("Error getting setting from database"),
        "{error}"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.VtankOptionNames.cs :: VtOptAnswersUnknownNamesInTheReferenceWords
#[test]
fn vt_opt_answers_unknown_names_in_the_reference_words() {
    let (mut world, mut plugin) = fresh("Namespace Tester");

    world.vt(&mut plugin, "opt set MyOwnFlag true");
    assert_eq!(
        world.chat(),
        ["[MossTank] Option set: Invalid option specified."]
    );

    world.clear_chat();
    world.vt(&mut plugin, "opt get MyOwnFlag");
    assert_eq!(
        world.chat(),
        ["[MossTank] Option get: Invalid option specified."]
    );

    world.clear_chat();
    world.vt(&mut plugin, "opt get WhoYouGonnaCall");
    assert_eq!(world.chat(), ["[MossTank] Option WhoYouGonnaCall = True"]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.VtankOptionNames.cs :: UbOptOfANameNoOneHasIsAnInvalidOption
#[test]
fn ub_opt_of_a_name_no_one_has_is_an_invalid_option() {
    let (mut world, mut plugin) = fresh("Namespace Tester");

    let set = evaluate(&mut world, &mut plugin, "uboptset[`MyOwnFlag`,1]").unwrap();
    assert_eq!(set.as_number("uboptset").unwrap(), 0.0);
    assert_eq!(world.chat(), ["[UB] Error: Invalid option: myownflag"]);

    world.clear_chat();
    let got = evaluate(&mut world, &mut plugin, "uboptget[`MyOwnFlag`]").unwrap();
    assert_eq!(got.as_number("uboptget").unwrap(), 0.0);
    assert_eq!(world.chat(), ["[UB] Error: Invalid option: myownflag"]);
}

// ---- MossTankPanelTests.OptionParity -------------------------------------

fn walk_to_own_rares(plugin: &MossTank) -> bool {
    plugin
        .tank()
        .unwrap()
        .settings()
        .inventory
        .loot
        .walk_to_own_rare_corpses
}

fn show_nav_lines(plugin: &MossTank) -> bool {
    plugin.tank().unwrap().settings().navigation.show_nav_lines
}

fn said(world: &World, text: &str) -> bool {
    world.chat().iter().any(|line| line.contains(text))
}

/// The same storage, a new panel (`new MossTankPanel(new FakeHost(...,
/// storage))`).
fn restart(world: &mut World, plugin: &mut MossTank) -> MossTank {
    world.disable(plugin);
    world.clear_chat();
    world.enable()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.OptionParity.cs :: OptionsTabSwitchesCanBeListedReadSetToggledAndReloadedThroughVtankCommands
#[test]
fn options_tab_switches_can_be_listed_read_set_toggled_and_reloaded_through_vtank_commands() {
    let (mut world, mut plugin) = fresh("Prover");

    world.vt(&mut plugin, "opt list");
    assert!(said(&world, "WalkToRareCorpse"));
    assert!(said(&world, "ShowNavLines"));

    world.vt(&mut plugin, "opt set WalkToRareCorpse true");
    world.vt(&mut plugin, "opt set shownavlines true");
    assert!(walk_to_own_rares(&plugin));
    assert!(show_nav_lines(&plugin));

    world.vt(&mut plugin, "opt get WalkToRareCorpse");
    assert!(said(&world, "Option WalkToRareCorpse = True"));
    world.vt(&mut plugin, "opt set WalkToRareCorpse false");
    world.vt(&mut plugin, "opt get WalkToRareCorpse");
    assert!(said(&world, "Option WalkToRareCorpse = False"));
    assert!(!walk_to_own_rares(&plugin));

    // Existing macros still read and write the same saved switch.
    world.vt(&mut plugin, "opt set walktoowncorpses true");
    world.vt(&mut plugin, "opt get WalkToOwnCorpses");
    world.vt(&mut plugin, "opt get WalkToOwnRareCorpses");
    world.vt(&mut plugin, "opt get ShowNavLines");
    assert!(said(&world, "Option WalkToRareCorpse = True"));
    assert!(said(&world, "Option ShowNavLines = True"));

    let restarted = restart(&mut world, &mut plugin);
    assert!(walk_to_own_rares(&restarted));
    assert!(show_nav_lines(&restarted));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.OptionParity.cs :: NumericOptionsTabValueSetThroughChatSurvivesProfileReload
#[test]
fn numeric_options_tab_value_set_through_chat_survives_profile_reload() {
    let (mut world, mut plugin) = fresh("Prover");

    world.vt(&mut plugin, "opt set PetMonsterDensity 7");
    world.vt(&mut plugin, "opt get PetMonsterDensity");
    assert!(said(&world, "Option PetMonsterDensity = 7"));

    let mut restarted = restart(&mut world, &mut plugin);
    world.vt(&mut restarted, "opt get PetMonsterDensity");
    assert!(said(&world, "Option PetMonsterDensity = 7"));
}

// ---- MossTankPanelTests.ExpressionErrors ---------------------------------

/// The debug setting's second line is the error's type and message rather
/// than .NET's whole exception with its stack (register META-6).
// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ExpressionErrors.cs :: AFailedCodeStringRunFollowsTheDebugAndErrorDisplaySettings
#[test]
fn a_failed_code_string_run_follows_the_debug_and_error_display_settings() {
    let (mut world, mut plugin) = fresh("Expression Tester");

    let _ = evaluate(&mut world, &mut plugin, "exec[`listcount[5]`]");
    assert_eq!(
        posted(&world),
        [
            (
                "[UB] Error: Error running string expression: listcount[5]<EOF>".to_owned(),
                UB_ERROR_CHAT_TYPE
            ),
            (
                "[UB] Error: listcount[List] expects argument #1/1 to be a List but a number was passed instead. Passed value: 5".to_owned(),
                UB_ERROR_CHAT_TYPE
            ),
        ]
    );

    world.command(&mut plugin, "ub", "opt set Plugin.Debug true");
    world.clear_chat();
    let _ = evaluate(&mut world, &mut plugin, "exec[`listcount[5]`]");
    let lines = posted(&world);
    assert_eq!(lines.len(), 2, "{lines:#?}");
    let (text, kind) = &lines[1];
    assert_eq!(*kind, UB_ERROR_CHAT_TYPE);
    assert!(text.starts_with("[UB] Error: "));
    assert!(
        text.contains("listcount[List] expects argument #1/1"),
        "{text}"
    );

    world.command(
        &mut plugin,
        "ub",
        "opt set Plugin.ErrorMessageDisplay.Enabled false",
    );
    world.clear_chat();
    let _ = evaluate(&mut world, &mut plugin, "exec[`listcount[5]`]");
    assert!(posted(&world).is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ExpressionErrors.cs :: TheUbMessageDisplaysShowAndColourTheirLines
#[test]
fn the_ub_message_displays_show_and_colour_their_lines() {
    let (mut world, mut plugin) = fresh("Expression Tester");

    for (name, value) in [
        ("Plugin.GenericMessageDisplay.Enabled", "True"),
        ("Plugin.GenericMessageDisplay.Color", "5"),
        ("Plugin.DebugMessageDisplay.Enabled", "True"),
        ("Plugin.DebugMessageDisplay.Color", "14"),
        ("Plugin.ExpressionMessageDisplay.Enabled", "True"),
        ("Plugin.ExpressionMessageDisplay.Color", "5"),
        ("Plugin.ErrorMessageDisplay.Enabled", "True"),
        ("Plugin.ErrorMessageDisplay.Color", "15"),
    ] {
        let row = definitions::all()
            .iter()
            .find(|row| row.name == name)
            .unwrap_or_else(|| panic!("{name}"));
        assert_eq!(row.default.to_storage_string(), value, "{name}");
    }

    let ub = |world: &mut World, plugin: &mut MossTank, line: &str| {
        world.command(plugin, "ub", line);
    };
    ub(
        &mut world,
        &mut plugin,
        "opt set Plugin.ErrorMessageDisplay.Color 3",
    );
    ub(
        &mut world,
        &mut plugin,
        "opt set Plugin.ExpressionMessageDisplay.Color 7",
    );
    world.clear_chat();
    ub(&mut world, &mut plugin, "opt set Nope.Missing 1");
    ub(&mut world, &mut plugin, "mexec 1+1");
    let lines = posted(&world);
    assert_eq!(
        lines[0],
        ("[UB] Error: Invalid option: nope.missing".to_owned(), 3)
    );
    assert!(lines.contains(&("[UB] Evaluating expression: \"1+1\"".to_owned(), 7)));

    ub(
        &mut world,
        &mut plugin,
        "opt set Plugin.GenericMessageDisplay.Enabled false",
    );
    world.clear_chat();
    ub(&mut world, &mut plugin, "opt set Jumper.Attempts 4");
    assert!(posted(&world).is_empty());

    ub(&mut world, &mut plugin, "opt set Plugin.Debug true");
    ub(
        &mut world,
        &mut plugin,
        "opt set Plugin.DebugMessageDisplay.Color 6",
    );
    world.clear_chat();
    ub(&mut world, &mut plugin, "opt set Jumper.Attempts 5");
    assert_eq!(
        posted(&world),
        [("[UB] Jumper.Attempts (Profile) = 5".to_owned(), 6)]
    );
    ub(
        &mut world,
        &mut plugin,
        "opt set Plugin.DebugMessageDisplay.Enabled false",
    );
    world.clear_chat();
    ub(&mut world, &mut plugin, "opt set Jumper.Attempts 6");
    assert!(posted(&world).is_empty());
}

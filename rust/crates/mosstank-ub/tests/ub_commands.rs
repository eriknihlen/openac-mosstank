//! The `/ub` commands, against `MossTankPanelTests.UbCommands`. The C#
//! drives the whole panel; these drive the UB commands with the rest of the
//! plugin as fakes, so a test whose point is another slice's behaviour (the
//! macro's `/vt` words, the item giver, the route store) is ported where
//! that slice lives.

mod common;

use ac_plugin_abi::character::OptionStatus;
use ac_plugin_abi::items::LootContainer;
use ac_plugin_abi::magic::SpellInfo;
use ac_plugin_abi::navigation::{MoveChannel, MoveDirection, MovePace, MoveUnit};
use ac_plugin_abi::social::{AllegianceResult, AllegianceStatus, LoginCharacter};
use ac_plugin_abi::world::{
    ItemProperties, ObjectClass, PortalTransition, Position, WeaponProfile, WorldObject,
};
use common::{NOW_UTC_MS, PLUGIN_VERSION, Rig, carried, landscape, navigation_at, player};
use mosstank_automation::locks::ActionLockKind;
use mosstank_expr::ExpressionValue;
use mosstank_meta::host::CombatMode;
use mosstank_settings::ub::{UbSettingScope, definitions};
use mosstank_ub::chat::{DEBUG_CHAT_TYPE, ERROR_CHAT_TYPE, GENERIC_CHAT_TYPE, MESSAGE_TYPES};
use mosstank_ub::commands::opt::{ub_option_get, ub_option_set};
use mosstank_ub::help;
use openac_plugin_compat::datetime::DateTime;

fn held_key(direction: MoveDirection, pace: MovePace) -> (MoveDirection, MovePace, f32, MoveUnit) {
    (direction, pace, 0.0, MoveUnit::MetersOrDegrees)
}

/// One command jump's hold, settle and retry grace, ticked through with the
/// client reporting no jump at all (`ExpireCommandJumpAttempt`).
fn expire_attempt(rig: &mut Rig) {
    rig.tick(0.1);
    rig.tick(0.25);
    rig.tick(5.0);
}

// ---- /ub, /ub help ----------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TheCompatibilityVersionLineNamesThePluginVersion
#[test]
fn the_compatibility_version_line_names_the_plugin_version() {
    let mut rig = Rig::new();
    // `/vt ub` is the same line as a bare `/ub`.
    rig.ub("");
    assert_eq!(
        format!("[UB] MossTank UB compatibility {PLUGIN_VERSION}"),
        rig.messages()[0]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: HelpForOneCommandPrintsItsUsageLine
#[test]
fn help_for_one_command_prints_its_usage_line() {
    let mut rig = Rig::new();
    rig.ub("help jump");
    assert_eq!(
        "[UB] Usage: /ub jump[swzxc] [heading] [holdtime]",
        rig.messages()[0]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: FullHelpPrintsTheReferencesExamples
#[test]
fn full_help_prints_the_references_examples() {
    let mut rig = Rig::new();
    rig.ub("delay 1000");
    assert_eq!(
        vec![
            "[UB] Error: Bad command syntax",
            "[UB] Usage: /ub delay <millisecondDelay> <command>",
            "Description: Runs a command after a delay.",
            "Examples:",
            " /ub delay 5000 /say hello",
            "  Runs \"/say hello\" after a 3000ms delay (3 seconds)",
        ],
        rig.messages()
    );

    rig.clear_messages();
    rig.ub("help vitae");
    assert_eq!(
        vec![
            "[UB] Usage: /ub vitae",
            "Description: Thinks to yourself with your current vitae percentage.",
            "Examples:",
        ],
        rig.messages()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: MexecmRunsTheExpressionSilently
#[test]
fn mexecm_runs_the_expression_silently() {
    let mut rig = Rig::new();
    rig.ub("mexecm setvar[quiet,7]");
    assert!(rig.messages().is_empty());
    assert_eq!("7", rig.evaluate("getvar[quiet]").to_display_string());

    rig.ub("mexecm 1+");
    let ub: Vec<String> = rig
        .messages()
        .into_iter()
        .filter(|line| line.starts_with("[UB]"))
        .collect();
    assert_eq!(vec!["[UB] Error: Error in expression: 1+".to_owned()], ub);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TheVendorAndNetworkCommandsHaveTheirOwnUsageLines
#[test]
fn the_vendor_and_network_commands_have_their_own_usage_lines() {
    for (command, usage) in [
        (
            "vendor",
            "/ub vendor {open[p] <vendorname,vendorid,vendorhex> | buyall | sellall | clearbuy | clearsell | opencancel | addbuy[p] <item> | addsell[p] <item>}",
        ),
        ("autovendor", "/ub autovendor <cancel|lootProfile>"),
        ("netclients", "/ub netclients <tag>"),
    ] {
        let mut rig = Rig::new();
        rig.ub(&format!("help {command}"));
        let messages = rig.messages();
        assert_eq!(format!("[UB] Usage: {usage}"), messages[0]);
        assert!(!messages[1].is_empty());
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TheNetworkCommandsAreDescribedRatherThanWarnedAbout
#[test]
fn the_network_commands_are_described_rather_than_warned_about() {
    let mut rig = Rig::new();
    rig.ub("help netclients");
    assert!(!rig.messages()[1].contains("not available on this client yet"));

    rig.clear_messages();
    rig.ub("help bc");
    assert_eq!(
        "[UB] Usage: /ub bc [millisecondDelay] <command>",
        rig.messages()[0]
    );

    rig.clear_messages();
    rig.ub("help bct");
    assert_eq!(
        "[UB] Usage: /ub bct <tags> [millisecondDelay] <command>",
        rig.messages()[0]
    );
}

// ---- /ub face ---------------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: UbFaceAsksTheClientForTheExactHeadingUntilWithinOneDegree
#[test]
fn ub_face_asks_the_client_for_the_exact_heading_until_within_one_degree() {
    let mut rig = Rig::new().at_heading(7.593);
    rig.ub("opt set Jumper.ThinkComplete true");
    rig.clear_messages();

    rig.ub("face 9");
    assert_eq!(vec![9.0f32], rig.host.faced_headings);

    // The first look is at once: still 1.4 degrees off, so ask again.
    rig.tick(0.02);
    assert_eq!(vec![9.0f32, 9.0], rig.host.faced_headings);
    // Inside the tenth of a second nothing is asked.
    rig.tick(0.05);
    assert_eq!(2, rig.host.faced_headings.len());
    rig.tick(0.06);
    assert_eq!(3, rig.host.faced_headings.len());
    let success = "/t Test Character, Turning Success".to_owned();
    assert!(!rig.submitted().contains(&success));

    rig.host.meta.navigation = navigation_at(9.0);
    rig.tick(0.1);
    assert!(rig.submitted().contains(&success));
    assert_eq!(3, rig.host.faced_headings.len());
    assert!(rig.host.meta.intents.is_empty());
    assert!(!rig.locks.borrow_mut().is_locked(ActionLockKind::Navigation));
    // The reference says nothing in chat about a turn that worked.
    assert!(
        !rig.messages()
            .iter()
            .any(|line| line.to_lowercase().contains("heading"))
    );

    rig.tick(1.0);
    assert_eq!(3, rig.host.faced_headings.len());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: UbFaceGivesUpAfterFiveSeconds
#[test]
fn ub_face_gives_up_after_five_seconds() {
    let mut rig = Rig::new().at_heading(0.0);

    rig.ub("face 90");
    for _ in 0..49 {
        rig.tick(0.1);
    }
    assert!(rig.locks.borrow_mut().is_locked(ActionLockKind::Navigation));
    assert!(!rig.messages().contains(&"[UB] Turning failed".to_owned()));

    rig.tick(0.2);
    assert_eq!("[UB] Turning failed", rig.messages().last().unwrap());
    assert!(!rig.locks.borrow_mut().is_locked(ActionLockKind::Navigation));
    let asked = rig.host.faced_headings.len();
    rig.tick(1.0);
    assert_eq!(asked, rig.host.faced_headings.len());

    rig.ub("opt set Jumper.ThinkFail true");
    rig.ub("face 90");
    rig.tick(5.1);
    assert!(
        rig.submitted()
            .contains(&"/t Test Character, Turning failed".to_owned())
    );
}

// ---- /ub jump, /ub simplejump -------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: UbJumpWithAHeadingTurnsThroughTheClientBeforeCharging
#[test]
fn ub_jump_with_a_heading_turns_through_the_client_before_charging() {
    let mut rig = Rig::new().at_heading(7.593);

    rig.ub("jumpw 9 100");
    assert_eq!(vec![9.0f32], rig.host.faced_headings);
    rig.tick(0.02);
    assert!(rig.host.jumps.is_empty());

    rig.host.meta.navigation = navigation_at(8.5);
    rig.tick(0.1);
    assert_eq!(vec![0.1f32], rig.host.jumps);
    assert_eq!(
        vec![held_key(MoveDirection::Forward, MovePace::Run)],
        rig.host.moves
    );
    assert!(rig.host.meta.intents.is_empty());
    assert!(
        !rig.messages()
            .iter()
            .any(|line| line.to_lowercase().contains("heading"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: UbJumpWithoutAHeadingChargesAtOnce
#[test]
fn ub_jump_without_a_heading_charges_at_once() {
    let mut rig = Rig::new().at_heading(123.0);
    rig.ub("jumpsw 500");
    rig.tick(0.02);
    assert!(rig.host.faced_headings.is_empty());
    assert_eq!(vec![0.5f32], rig.host.jumps);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: UbJumpGivesUpWithTheReferenceLine
#[test]
fn ub_jump_gives_up_with_the_reference_line() {
    let mut rig = Rig::new().at_heading(180.0);
    rig.ub("opt set Jumper.ThinkFail true");

    rig.ub("jumpw 100");
    rig.tick(0.05);
    for _ in 2..=3 {
        expire_attempt(&mut rig);
        rig.tick(0.05);
    }
    expire_attempt(&mut rig);

    assert_eq!(vec![0.1f32, 0.1, 0.1], rig.host.jumps);
    // Each charge's key is let go before the next, and after the last.
    assert_eq!(
        vec![
            MoveChannel::Travel,
            MoveChannel::Travel,
            MoveChannel::Travel
        ],
        rig.host.stopped_moves
    );
    assert!(
        rig.submitted()
            .contains(&"/t Test Character, You have failed to jump too many times.".to_owned())
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: AWalkingForwardJumpLeavesWithExactlyThePowerAsked
#[test]
fn a_walking_forward_jump_leaves_with_exactly_the_power_asked() {
    let mut rig = Rig::new().at_heading(180.0);
    rig.ub("jumpsw 300");
    rig.tick(0.02);

    assert_eq!(vec![0.3f32], rig.host.jumps);
    assert_eq!(
        vec![held_key(MoveDirection::Forward, MovePace::Walk)],
        rig.host.moves
    );
    assert!(!rig.host.meta.intents.iter().any(|intent| intent.jump));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: ADiagonalJumpHoldsForwardAndStrafeLeftThroughTheCharge
#[test]
fn a_diagonal_jump_holds_forward_and_strafe_left_through_the_charge() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.ub("jumpwz 100");
    rig.tick(0.02);

    assert_eq!(vec![0.1f32], rig.host.jumps);
    assert_eq!(
        vec![
            held_key(MoveDirection::Forward, MovePace::Run),
            held_key(MoveDirection::StrafeLeft, MovePace::Run),
        ],
        rig.host.moves
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: AZeroHoldJumpsWithTheLeastPowerTheClientTakes
#[test]
fn a_zero_hold_jumps_with_the_least_power_the_client_takes() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.ub("jump");
    rig.tick(0.02);

    assert_eq!(vec![f32::from_bits(1)], rig.host.jumps);
    assert!(rig.host.moves.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: AJumpLetsGoOfItsKeysOnceItIsOver
#[test]
fn a_jump_lets_go_of_its_keys_once_it_is_over() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.ub("jumpwz 100");
    rig.tick(0.02);
    assert!(rig.host.stopped_moves.is_empty());

    rig.host.move_report.jump_sequence = 1;
    rig.tick(0.1);
    rig.tick(0.25);

    assert_eq!(
        vec![MoveChannel::Travel, MoveChannel::Strafe],
        rig.host.stopped_moves
    );
    assert!(!rig.locks.borrow_mut().is_locked(ActionLockKind::Navigation));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: ForwardAndBackwardTogetherStayOnTheHeldKeys
#[test]
fn forward_and_backward_together_stay_on_the_held_keys() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.ub("jumpwx 100");
    rig.tick(0.02);

    assert!(rig.host.jumps.is_empty());
    let intent = rig.host.meta.intents.last().unwrap();
    assert!(intent.jump && intent.forward && intent.backward && intent.run);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TheShiftLetterWalksTheHeldKeys
#[test]
fn the_shift_letter_walks_the_held_keys() {
    for (command, run) in [("jumpwx 100", true), ("jumpswx 100", false)] {
        let mut rig = Rig::new().at_heading(0.0);
        rig.ub(command);
        rig.tick(0.02);
        assert_eq!(run, rig.host.meta.intents.last().unwrap().run, "{command}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: AShiftForwardJumpWalksForward
#[test]
fn a_shift_forward_jump_walks_forward() {
    let mut rig = Rig::new().at_heading(180.0);
    rig.ub("jumpsw 180 500");
    rig.tick(0.1);

    assert_eq!(
        vec![held_key(MoveDirection::Forward, MovePace::Walk)],
        rig.host.moves
    );
    assert_eq!(vec![0.5f32], rig.host.jumps);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: ABackwardJumpPressesBackwardAndNotForward
#[test]
fn a_backward_jump_presses_backward_and_not_forward() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.ub("jumpsx 300");
    rig.tick(0.1);
    assert_eq!(
        vec![held_key(MoveDirection::Backward, MovePace::Walk)],
        rig.host.moves
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TheStrafeFlagsPressTheirOwnKeys
#[test]
fn the_strafe_flags_press_their_own_keys() {
    let mut left = Rig::new().at_heading(0.0);
    left.ub("jumpz 100");
    left.tick(0.1);
    let mut right = Rig::new().at_heading(0.0);
    right.ub("jumpc 100");
    right.tick(0.1);

    assert_eq!(
        vec![held_key(MoveDirection::StrafeLeft, MovePace::Run)],
        left.host.moves
    );
    assert_eq!(
        vec![held_key(MoveDirection::StrafeRight, MovePace::Run)],
        right.host.moves
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TwoDirectionLettersHoldTwoKeys
#[test]
fn two_direction_letters_hold_two_keys() {
    for (verb, forward, backward, left, right) in [
        ("jumpwz", true, false, true, false),
        ("jumpswc", true, false, false, true),
        ("jumpxz", false, true, true, false),
    ] {
        let mut rig = Rig::new().at_heading(0.0);
        rig.ub(&format!("{verb} 100"));
        rig.tick(0.01);

        assert_eq!(vec![0.1f32], rig.host.jumps, "{verb}");
        let held: Vec<MoveDirection> = rig.host.moves.iter().map(|m| m.0).collect();
        assert_eq!(
            (forward, backward, left, right),
            (
                held.contains(&MoveDirection::Forward),
                held.contains(&MoveDirection::Backward),
                held.contains(&MoveDirection::StrafeLeft),
                held.contains(&MoveDirection::StrafeRight)
            ),
            "{verb}"
        );
        assert_eq!(2, held.len(), "{verb}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: ABareJumpPressesNoMovementKeyAtAll
#[test]
fn a_bare_jump_presses_no_movement_key_at_all() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.ub("jump");
    rig.tick(0.01);
    assert_eq!(1, rig.host.jumps.len());
    assert!(rig.host.moves.is_empty());
    assert!(rig.host.meta.intents.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: OneNumberIsTheHoldTimeAndNotAHeading
#[test]
fn one_number_is_the_hold_time_and_not_a_heading() {
    let mut rig = Rig::new().at_heading(45.0);
    rig.ub("jump 300");
    rig.tick(0.01);
    assert!(rig.host.faced_headings.is_empty());
    assert_eq!(vec![0.3f32], rig.host.jumps);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: AHoldTimeOverASecondIsRefused
#[test]
fn a_hold_time_over_a_second_is_refused() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.ub("jumpw 0 2000");
    assert_eq!(
        "[UB] Error: Jumper: holdtime should be a number between 0 and 1000",
        rig.single_message()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: AHeadingOutsideTheCompassIsRefused
#[test]
fn a_heading_outside_the_compass_is_refused() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.ub("jumpw 400 100");
    assert_eq!(
        "[UB] Error: Jumper: direction should be a number between 0 and 359",
        rig.single_message()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: AHeadingThatIsNotANumberIsBadSyntax
#[test]
fn a_heading_that_is_not_a_number_is_bad_syntax() {
    for heading in ["NaN", "Infinity", "1e2"] {
        let mut rig = Rig::new().at_heading(0.0);
        rig.ub(&format!("jumpw {heading} 100"));
        rig.tick(0.1);
        assert_eq!("[UB] Error: Bad command syntax", rig.messages()[0]);
        assert!(rig.host.faced_headings.is_empty());
        assert!(rig.host.meta.intents.is_empty());
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: ASecondJumpWhileOneIsInTheAirIsRefused
#[test]
fn a_second_jump_while_one_is_in_the_air_is_refused() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.ub("jumpw 0 100");
    rig.clear_messages();
    rig.ub("jumpw 0 100");
    assert_eq!(
        "[UB] Error: Jumper: You are already jumping. try again later.",
        rig.single_message()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: SimpleJumpJumpsWhereTheCharacterAlreadyStands
#[test]
fn simple_jump_jumps_where_the_character_already_stands() {
    let mut rig = Rig::new().at_heading(123.0);
    rig.ub("simplejump 200");
    rig.tick(0.1);
    assert!(rig.host.faced_headings.is_empty());
    assert_eq!(vec![0.2f32], rig.host.jumps);
    assert!(rig.host.moves.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: SimpleJumpRefusesAHoldTimeOverASecond
#[test]
fn simple_jump_refuses_a_hold_time_over_a_second() {
    let mut rig = Rig::new();
    rig.ub("simplejump 5000");
    assert_eq!(
        "[UB] Error: Jumper: holdtime should be a number between 0 and 1000",
        rig.single_message()
    );
}

// ---- /ub calcdamage -----------------------------------------------------------

fn bow(rig: &mut Rig, id: u32, spells: Vec<u32>, ints: Vec<(u32, i32)>, examined: bool) {
    rig.host.meta.objects.push(WorldObject {
        object_id: id,
        name: "Ice Bow".to_owned(),
        object_class: ObjectClass::MissileWeapon,
        has_appraisal_data: examined,
        spell_ids: spells,
        ..WorldObject::default()
    });
    if examined {
        rig.host.meta.properties.insert(
            id,
            ItemProperties {
                ints,
                weapon: Some(WeaponProfile {
                    damage_type: 0,
                    weapon_time: 20,
                    weapon_skill: 0,
                    damage: 50,
                    damage_variance: 0.5,
                    damage_mod: 2.0,
                    weapon_length: 1.0,
                    max_velocity: 1.0,
                    weapon_offense: 1.0,
                    max_velocity_estimated: 0,
                }),
                ..ItemProperties::default()
            },
        );
    }
    rig.host.meta.selection.selected = Some(id);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: CalcDamageAddsTheCantripAndTheTinkersStillAvailable
#[test]
fn calc_damage_adds_the_cantrip_and_the_tinkers_still_available() {
    let mut rig = Rig::new();
    // Major Blood Thirst: +4 to the maximum damage.
    bow(
        &mut rig,
        70,
        vec![2586],
        vec![(204, 6), (171, 2), (179, 0)],
        true,
    );

    rig.ub("calcdamage");

    // Ten tinkers less two used and one reserved for the imbue: seven.
    let messages = rig.messages();
    assert!(messages.contains(&"[UB] 7 mahogany salvage adds 0.28 to DamageModifier".to_owned()));
    assert!(
        messages
            .iter()
            .any(|line| line.starts_with("[UB] Calculated Formula: (50(+4 from cantrips) + 6)"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: CalcDamageNamesEveryCantripThatLiftsTheMaximumDamage
#[test]
fn calc_damage_names_every_cantrip_that_lifts_the_maximum_damage() {
    let mut rig = Rig::new();
    bow(
        &mut rig,
        72,
        vec![2586, 1, 2598],
        vec![(204, 0), (171, 10), (179, 1)],
        true,
    );
    for (id, name) in [(2586, "Major Blood Thirst"), (2598, "Minor Blood Thirst")] {
        rig.host.meta.spells.insert(
            id,
            SpellInfo {
                spell_id: id,
                name: name.to_owned(),
                ..SpellInfo::default()
            },
        );
    }

    rig.ub("calcdamage");

    let messages = rig.messages();
    assert_eq!(
        vec![
            "[UB] Spell Major Blood Thirst buffs MaxDamage by 4",
            "[UB] Spell Minor Blood Thirst buffs MaxDamage by 2",
        ],
        messages[..2]
    );
    assert!(
        messages
            .iter()
            .any(|line| line.starts_with("[UB] Calculated Formula: (50(+6 from cantrips) + 0)"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: CalcDamageRefusesAnItemThatHasNotBeenExamined
#[test]
fn calc_damage_refuses_an_item_that_has_not_been_examined() {
    let mut rig = Rig::new();
    bow(&mut rig, 71, Vec::new(), Vec::new(), false);
    rig.ub("calcdamage");
    assert_eq!(
        "[UB] Error: Ice Bow does not have id data, please examine it first.",
        rig.single_message()
    );
}

// ---- /ub opt -------------------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: AnOptionNameWithADotReachesTheUbSettings
#[test]
fn an_option_name_with_a_dot_reaches_the_ub_settings() {
    let mut rig = Rig::new();
    rig.ub("opt set AutoVendor.Enabled true");
    rig.clear_messages();
    rig.ub("opt get AutoVendor.Enabled");
    assert_eq!(
        "[UB] AutoVendor.Enabled (Profile) = True",
        rig.single_message()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: AnOptValueLineIsTheReferencesFullDisplayValue
#[test]
fn an_opt_value_line_is_the_references_full_display_value() {
    for (name, line) in [
        (
            "ItemGiver.Think",
            "InventoryManager.IGThink (Profile) = False",
        ),
        (
            "InventoryManager.IGRange",
            "InventoryManager.IGRange (Profile) = 15",
        ),
        (
            "Nametags.Player.TagColor",
            "Nametags.Player.TagColor (Profile) = 0xFF00FFFF",
        ),
        ("Nametags.MaxRange", "Nametags.MaxRange (Profile) = 35"),
        (
            "Plugin.ErrorMessageDisplay.Color",
            "Plugin.ErrorMessageDisplay.Color (Profile) = Help",
        ),
        (
            "NetworkUI.SelectedTag",
            "NetworkUI.SelectedTag (State) = All",
        ),
        ("Networking.Tags", "Networking.Tags (State) = []"),
    ] {
        let mut rig = Rig::new();
        rig.ub(&format!("opt get {name}"));
        assert_eq!(format!("[UB] {line}"), rig.single_message());
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: AnOptSetWithoutAValueIsBadSyntax
#[test]
fn an_opt_set_without_a_value_is_bad_syntax() {
    let mut rig = Rig::new();
    rig.ub("opt set Jumper.Attempts");
    let messages = rig.messages();
    assert_eq!("[UB] Error: Bad command syntax", messages[0]);
    assert!(messages[1].starts_with("[UB] Usage: /ub opt "));
    assert!(
        !messages
            .iter()
            .any(|line| line.contains("Jumper.Attempts ("))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: OptSetEditsAListSettingWithAddRemoveAndClear
#[test]
fn opt_set_edits_a_list_setting_with_add_remove_and_clear() {
    let mut rig = Rig::new();
    rig.ub("opt set Networking.Tags add RynTest");
    rig.ub("opt set Networking.Tags add Other,RynTest,Third");
    rig.clear_messages();
    rig.ub("opt get Networking.Tags");
    assert_eq!(
        "[UB] Networking.Tags (State) = [RynTest,Other,Third]",
        rig.single_message()
    );

    rig.ub("opt set Networking.Tags remove Other,Missing");
    rig.clear_messages();
    rig.ub("opt set Networking.Tags replace everything");
    assert_eq!("[UB] Error: Unknown verb: everything", rig.single_message());
    rig.clear_messages();
    rig.ub("opt set Networking.Tags replace");
    assert_eq!("[UB] Error: Unknown verb: replace", rig.single_message());
    rig.clear_messages();
    rig.ub("opt get Networking.Tags");
    assert_eq!(
        "[UB] Networking.Tags (State) = [RynTest,Third]",
        rig.single_message()
    );

    rig.clear_messages();
    rig.ub("opt set Networking.Tags clear");
    assert_eq!("[UB] Networking.Tags (State) = []", rig.single_message());
}

fn opt_get(rig: &mut Rig, name: &str) -> ExpressionValue {
    rig.with_call(|call, _| ub_option_get(call, name))
}

fn opt_set(rig: &mut Rig, name: &str, value: &ExpressionValue) -> bool {
    rig.with_call(|call, _| ub_option_set(call, name, value))
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: UbOptGetAndSetReachTheUbSettings
#[test]
fn ub_opt_get_and_set_reach_the_ub_settings() {
    let mut rig = Rig::new();
    rig.ub("opt set Networking.Tags add RynTest");
    let ExpressionValue::List(tags) = opt_get(&mut rig, "Networking.Tags") else {
        panic!("a list setting reads as a list");
    };
    assert_eq!("RynTest", tags.items()[0].to_display_string());

    assert!(opt_set(
        &mut rig,
        "AutoVendor.Enabled",
        &ExpressionValue::zero()
    ));
    assert!(!opt_get(&mut rig, "AutoVendor.Enabled").is_truthy());
    rig.clear_messages();
    rig.ub("opt get AutoVendor.Enabled");
    assert_eq!(
        "[UB] AutoVendor.Enabled (Profile) = False",
        rig.single_message()
    );

    let list = rig.evaluate("listcreate[`A`,`B`]");
    assert!(opt_set(&mut rig, "Networking.Tags", &list));
    assert!(!opt_set(
        &mut rig,
        "Networking.Tags",
        &ExpressionValue::string("A")
    ));
    rig.clear_messages();
    rig.ub("opt get Networking.Tags");
    assert_eq!("[UB] Networking.Tags (State) = [A,B]", rig.single_message());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: OptSetAcceptsTheSettingNamesMacrosUse
#[test]
fn opt_set_accepts_the_setting_names_macros_use() {
    let mut rig = Rig::new().at_heading(0.0);
    for command in [
        "opt set Plugin.SettingsProfile CMD",
        "opt set VTank.PatchExpressionEngine False",
        "opt set VTank.VitalSharing False",
        "opt set InventoryManager.IGThink True",
        "opt set InventoryManager.TreatStackAsSingleItem True",
        "opt set Plugin.PortalThink True",
        "opt set AutoSalvage.Think True",
        "opt set AutoTrade.Think True",
        "opt set Plugin.VideoPatch True",
        "opt set Plugin.VideoPatchFocus True",
        "opt set Looter.EnableChests True",
    ] {
        rig.clear_messages();
        rig.ub(command);
        assert!(
            !rig.messages()
                .iter()
                .any(|line| line.to_lowercase().contains("invalid option")),
            "{command}"
        );
    }

    assert!(opt_get(&mut rig, "VTank.PatchExpressionEngine").is_truthy());
    assert!(!opt_get(&mut rig, "InventoryManager.TreatStackAsSingleItem").is_truthy());
    assert!(!opt_get(&mut rig, "Sharing.Vitals").is_truthy());
    assert!(opt_get(&mut rig, "ItemGiver.Think").is_truthy());
    assert_eq!("CMD", *rig.live.profile.borrow());
    assert_eq!(
        "CMD",
        opt_get(&mut rig, "Plugin.SettingsProfile").to_display_string()
    );
    assert!(opt_get(&mut rig, "Plugin.VideoPatch").is_truthy());
    assert!(opt_get(&mut rig, "Looter.EnableChests").is_truthy());
    assert!(opt_set(
        &mut rig,
        "Looter.EnableChests",
        &ExpressionValue::zero()
    ));
    assert!(!opt_get(&mut rig, "Looter.EnableChests").is_truthy());

    // The portal switch is wired: the not-found line becomes a think.
    rig.clear_messages();
    rig.ub("portal Gateway");
    assert!(
        rig.submitted()
            .contains(&"/t Test Character, Could not find a portal".to_owned())
    );
}

// ---- myquests, quit, playeroption, toggle ---------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: MyQuestsAsksForTheQuestListAgain
#[test]
fn my_quests_asks_for_the_quest_list_again() {
    let mut rig = Rig::new();
    assert!(rig.submitted().contains(&"/myquests".to_owned()));
    rig.host.meta.submitted.clear();
    rig.clear_messages();

    rig.ub("myquests");

    assert_eq!("[UB] QuestTracker: Refreshing quests", rig.single_message());
    assert!(rig.submitted().contains(&"/myquests".to_owned()));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: QuitAsksTheClientToClose
#[test]
fn quit_asks_the_client_to_close() {
    let mut rig = Rig::new();
    rig.ub("quit");
    assert_eq!(1, rig.host.close_requests);
    assert_eq!("[UB] Quitting Client", rig.single_message());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: PlayerOptionChangesTheCharactersOwnOption
#[test]
fn player_option_changes_the_characters_own_option() {
    let mut rig = Rig::new();
    rig.host.character_options = vec![
        ("AllowGive".to_owned(), true),
        ("FellowshipAutoAcceptRequests".to_owned(), false),
    ];
    rig.host.option_status = OptionStatus::Accepted;

    rig.ub("playeroption allowgive off");
    rig.ub("playeroption FellowshipAutoAcceptRequests true");

    assert_eq!(
        vec![
            ("allowgive".to_owned(), false),
            ("FellowshipAutoAcceptRequests".to_owned(), true),
        ],
        rig.host.option_changes
    );
    assert_eq!(
        vec![
            "[UB] Setting AllowGive = False",
            "[UB] Setting FellowshipAutoAcceptRequests = True",
        ],
        rig.messages()
    );

    rig.clear_messages();
    rig.ub("playeroption NoSuchOption on");
    rig.ub("playeroption list");
    for line in rig.messages() {
        assert!(
            line.contains("AllowGive, FellowshipAutoAcceptRequests"),
            "{line}"
        );
    }
    assert_eq!(2, rig.host.option_changes.len());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: ToggleFlipsAUbSwitch
#[test]
fn toggle_flips_a_ub_switch() {
    let mut rig = Rig::new();
    rig.ub("opt get DungeonMaps.Enabled");
    let before = rig.single_message();
    rig.clear_messages();

    rig.ub("opt toggle DungeonMaps.Enabled");

    let after = rig.single_message();
    assert_ne!(before, after);
    assert_eq!(
        if before.ends_with("True") {
            "[UB] DungeonMaps.Enabled (Profile) = False"
        } else {
            "[UB] DungeonMaps.Enabled (Profile) = True"
        },
        after
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: AnInvalidOptionListsTheCharacterSettings
#[test]
fn an_invalid_option_lists_the_character_settings() {
    for command in [
        "opt get Nope.Missing",
        "opt set Nope.Missing 1",
        "opt toggle Nope.Missing",
    ] {
        let mut rig = Rig::new();
        rig.ub(command);

        let character: Vec<String> = definitions::all()
            .iter()
            .filter(|row| row.scope == UbSettingScope::Character)
            .map(|row| format!("[UB] Error:   - {}", row.name))
            .collect();
        assert!(!character.is_empty());
        let mut expected = vec!["[UB] Error: Invalid option: nope.missing".to_owned()];
        expected.extend(character);
        let messages = rig.messages();
        assert_eq!(expected, messages, "{command}");
        assert!(messages.contains(&"[UB] Error:   - Networking.Tags".to_owned()));
        assert!(!messages.contains(&"[UB] Error:   - Jumper.Attempts".to_owned()));
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TogglingSomethingThatIsNotASwitchSaysSo
#[test]
fn toggling_something_that_is_not_a_switch_says_so() {
    let mut rig = Rig::new();
    rig.ub("opt toggle DungeonMaps.Opacity");
    assert_eq!(
        "[UB] Error: Unable to toggle setting DungeonMaps.Opacity: Specified cast is not valid.",
        rig.single_message()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: EachOptionListShowsOnlyItsOwnGroup
#[test]
fn each_option_list_shows_only_its_own_group() {
    // The `/vt opt list` half is the macro's own options, ported with the
    // `/vt` words; here the `/ub opt list` half.
    let mut rig = Rig::new();
    rig.ub("opt list");
    let messages = rig.messages();
    assert_eq!("[UB] All Settings:", messages[0]);
    assert!(
        messages
            .iter()
            .any(|line| line.starts_with("DungeonMaps.Enabled (Profile) = "))
    );
    assert!(
        !messages
            .iter()
            .any(|line| line.starts_with("Available options:"))
    );
}

// ---- pos, id, vitae, combatstate, date -------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: PosPrintsTheSelectedObjectsCoordinatesAndCell
#[test]
fn pos_prints_the_selected_objects_coordinates_and_cell() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.host.meta.objects.push(WorldObject {
        object_id: 80,
        name: "Lifestone".to_owned(),
        object_class: ObjectClass::Lifestone,
        is_landscape: true,
        position: Some(Position {
            cell_id: 0x00AB_0102,
            east_west: 3.5,
            north_south: -1.25,
            elevation: 12.0,
            heading_degrees: 0.0,
            is_outdoor: true,
        }),
        ..WorldObject::default()
    });
    rig.host.meta.selection.selected = Some(80);

    rig.ub("pos");

    let messages = rig.messages();
    assert_eq!("[UB] Id: 80 ( 0x00000050 )", messages[0]);
    assert_eq!("[UB] Coords: 1.2S, 3.5E", messages[1]);
    assert_eq!("[UB] Landcell: 0x00AB0102", messages[2]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: PosWithNothingSelectedSaysSo
#[test]
fn pos_with_nothing_selected_says_so() {
    let mut rig = Rig::new();
    rig.ub("pos");
    assert_eq!("[UB] Error: pos: No object selected", rig.single_message());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: IdPrintsTheSelectedObjectsIdBothWaysRound
#[test]
fn id_prints_the_selected_objects_id_both_ways_round() {
    let mut rig = Rig::new();
    rig.host.meta.objects.push(player(0x5000_0123, "Mule"));
    rig.host.meta.selection.selected = Some(0x5000_0123);
    rig.ub("id");
    assert_eq!("[UB] Id: 1342177571 ( 0x50000123 )", rig.single_message());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: VitaeIsThoughtAsAPercentage
#[test]
fn vitae_is_thought_as_a_percentage() {
    let mut rig = Rig::new();
    rig.host.meta.properties.insert(
        1,
        ItemProperties {
            floats: vec![(129, 0.95)],
            ..ItemProperties::default()
        },
    );
    rig.ub("vitae");
    assert!(
        rig.submitted()
            .contains(&"/t Test Character, My vitae is 5%".to_owned())
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: AThinkThatCannotBeSentPrintsNothingInItsPlace
#[test]
fn a_think_that_cannot_be_sent_prints_nothing_in_its_place() {
    let mut rig = Rig::new();
    rig.host.meta.character_mut().name.clear();
    rig.clear_messages();
    rig.host.meta.submitted.clear();

    rig.ub("vitae");

    assert!(rig.submitted().is_empty());
    assert!(
        !rig.messages()
            .iter()
            .any(|line| line.starts_with("You think"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: CombatStateAsksTheClientToChangeStance
#[test]
fn combat_state_asks_the_client_to_change_stance() {
    let mut rig = Rig::new();
    rig.ub("combatstate melee");
    assert_eq!(Some(CombatMode::Melee), rig.host.meta.requested_mode);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: AnUnknownCombatStateIsRefusedByName
#[test]
fn an_unknown_combat_state_is_refused_by_name() {
    let mut rig = Rig::new();
    rig.ub("combatstate sideways");
    assert_eq!(
        "[UB] Error: sideways is not a valid option",
        rig.single_message()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TheUtcDateIsFormattedInTheInvariantCulture
#[test]
fn the_utc_date_is_formatted_in_the_invariant_culture() {
    let mut rig = Rig::new();
    rig.ub("dateutc dddd");
    assert_eq!(
        format!(
            "[UB] Current Date: {}",
            DateTime::utc_from_unix_millis(NOW_UTC_MS)
                .format("dddd")
                .unwrap()
        ),
        rig.single_message()
    );
    assert_eq!("[UB] Current Date: Tuesday", rig.single_message());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: ABareDateUsesTheDefaultFormat
#[test]
fn a_bare_date_uses_the_default_format() {
    let mut rig = Rig::new();
    rig.services.offset_minutes = 120;
    rig.ub("date");
    assert_eq!(
        format!(
            "[UB] Current Date: {}",
            DateTime::local_from_unix_millis(NOW_UTC_MS, 120)
                .format("dddd dd MMMM HH:mm:ss")
                .unwrap()
        ),
        rig.single_message()
    );
    assert_eq!(
        "[UB] Current Date: Tuesday 13 October 02:00:00",
        rig.single_message()
    );
}

// ---- delay, text classes ---------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: ADelayedCommandRunsOnlyOnceItsWaitIsOver
#[test]
fn a_delayed_command_runs_only_once_its_wait_is_over() {
    let mut rig = Rig::new();
    rig.ub("delay 1000 /say hello");
    rig.tick(0.5);
    assert!(!rig.submitted().contains(&"/say hello".to_owned()));
    rig.tick(0.6);
    assert!(rig.submitted().contains(&"/say hello".to_owned()));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: ADelayedCommandIsReportedOnlyInTheDebugOutput
#[test]
fn a_delayed_command_is_reported_only_in_the_debug_output() {
    let mut rig = Rig::new();
    rig.ub("delay 1000 /say hello");
    rig.tick(1.1);
    assert!(rig.submitted().contains(&"/say hello".to_owned()));
    assert!(rig.messages().is_empty());

    rig.ub("opt set Plugin.Debug true");
    rig.clear_messages();
    rig.ub("delay 250 /say again");
    rig.tick(0.3);

    assert_eq!(
        vec![
            (
                "[UB] Scheduling command `/say again` with delay of 250ms".to_owned(),
                DEBUG_CHAT_TYPE
            ),
            (
                "[UB] Plugin: Executing command `/say again` (delay was 250ms)".to_owned(),
                DEBUG_CHAT_TYPE
            ),
        ],
        rig.posted()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: UbLinesUseTheReferencesTextClasses
#[test]
fn ub_lines_use_the_references_text_classes() {
    let mut rig = Rig::new();
    rig.ub("opt set Jumper.Attempts 4");
    rig.ub("opt set Nope.Missing 1");
    rig.ub("opt set Plugin.Debug true");
    rig.ub("opt set Jumper.Attempts 5");

    let posted: Vec<(String, u32)> = rig
        .posted()
        .into_iter()
        .filter(|(text, _)| !text.starts_with("[UB] Error:   - "))
        .collect();
    assert_eq!(
        vec![
            (
                "[UB] Jumper.Attempts (Profile) = 4".to_owned(),
                GENERIC_CHAT_TYPE
            ),
            (
                "[UB] Error: Invalid option: nope.missing".to_owned(),
                ERROR_CHAT_TYPE
            ),
            (
                "[UB] Plugin.Debug (Profile) = True".to_owned(),
                DEBUG_CHAT_TYPE
            ),
            (
                "[UB] Jumper.Attempts (Profile) = 5".to_owned(),
                DEBUG_CHAT_TYPE
            ),
        ],
        posted
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TheLinesBelowAUbHeadingShareItsClassAndItsSwitch
#[test]
fn the_lines_below_a_ub_heading_share_its_class_and_its_switch() {
    let mut rig = Rig::new();
    rig.host.meta.login.roster = vec![LoginCharacter {
        object_id: 0x50,
        name: "Zeke".to_owned(),
        active_index: 0,
        is_pending_delete: false,
    }];
    rig.evaluate("setvar[myvar,1]");
    rig.clear_messages();

    for line in [
        "",
        "help",
        "help jump",
        "opt list",
        "listvars",
        "login list",
    ] {
        rig.ub(line);
    }
    assert!(
        rig.posted()
            .iter()
            .all(|(_, kind)| *kind == GENERIC_CHAT_TYPE)
    );
    assert!(
        rig.messages()
            .contains(&" Type `/ub help` or `/ub help <command>` for help.".to_owned())
    );

    rig.clear_messages();
    rig.ub("mexec 1+");
    let posted = rig.posted();
    assert_eq!(
        2,
        posted
            .iter()
            .filter(|(_, kind)| *kind == ERROR_CHAT_TYPE)
            .count()
    );
    assert!(posted.last().unwrap().0.starts_with("  "));

    rig.ub("opt set Plugin.GenericMessageDisplay.Enabled false");
    rig.ub("opt set Plugin.ErrorMessageDisplay.Enabled false");
    rig.ub("opt set Plugin.ExpressionMessageDisplay.Enabled false");
    rig.clear_messages();
    for line in [
        "",
        "help",
        "help jump",
        "opt list",
        "listvars",
        "login list",
        "mexec 1+",
    ] {
        rig.ub(line);
    }
    assert!(rig.messages().is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: AToolLineThatDoesNotParsePrintsTheFullHelp
#[test]
fn a_tool_line_that_does_not_parse_prints_the_full_help() {
    for (line, verb) in [
        ("vendor frobnicate", "vendor"),
        ("vendor addbuyp 5", "vendor"),
        ("equip wear profile.utl", "equip"),
        ("xp spend", "xp"),
    ] {
        let mut rig = Rig::new();
        let usage = help::require(verb);
        assert!(!usage.examples.is_empty());

        // The tool's own parser refuses the line (`None`); the answer is the
        // UB side's.
        rig.ub(line);

        let mut expected = vec![
            "[UB] Error: Bad command syntax".to_owned(),
            format!("[UB] Usage: {}", usage.usage),
            format!("Description: {}", usage.summary),
            "Examples:".to_owned(),
        ];
        for example in usage.examples {
            expected.push(format!(" {}", example.command));
            expected.push(format!("  {}", example.description));
        }
        assert_eq!(expected, rig.messages(), "{line}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: ADelayWithoutACommandPrintsItsUsage
#[test]
fn a_delay_without_a_command_prints_its_usage() {
    let mut rig = Rig::new();
    rig.ub("delay 1000");
    assert_eq!(
        vec![
            "[UB] Error: Bad command syntax",
            "[UB] Usage: /ub delay <millisecondDelay> <command>",
        ],
        rig.messages()[..2]
    );
}

// ---- portals, follow, allegiance ---------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: ClosestPortalUsesTheNearestPortal
#[test]
fn closest_portal_uses_the_nearest_portal() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.host.meta.objects = vec![
        landscape(90, "Far Portal", ObjectClass::Portal, 40.0),
        landscape(91, "Near Portal", ObjectClass::Portal, 5.0),
    ];
    rig.ub("closestportal");
    // The first try goes out on the frame a quarter second later.
    assert!(rig.host.used.is_empty());
    rig.tick(0.3);
    assert_eq!(vec![91], rig.host.used);
    assert!(
        rig.messages()
            .contains(&"[UB] Attempting to use portal: Near Portal".to_owned())
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: PortalByPartialNameUsesTheMatchingPortal
#[test]
fn portal_by_partial_name_uses_the_matching_portal() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.host.meta.objects = vec![
        landscape(92, "Gateway", ObjectClass::Portal, 5.0),
        landscape(93, "Holtburg Portal", ObjectClass::Portal, 40.0),
    ];
    rig.ub("portalp Holt");
    rig.tick(0.3);
    assert_eq!(vec![93], rig.host.used);
}

/// Entering portal space, as the client reports it.
fn enter_portal_space(rig: &mut Rig, revision: i64) {
    let transition = PortalTransition {
        revision,
        generation: revision,
        kind: 2,
        ..PortalTransition::default()
    };
    rig.with_call(|call, commands| commands.on_portal_transition(call, &transition));
}

fn navigation_locked(rig: &Rig) -> bool {
    rig.locks.borrow_mut().is_locked(ActionLockKind::Navigation)
}

/// A notice of hop `generation`, completed or still entering.
fn portal_hop(rig: &mut Rig, generation: i64, completed: bool) {
    let transition = PortalTransition {
        revision: generation,
        generation,
        is_ready: completed,
        is_materialized: completed,
        is_completed: completed,
        kind: 2,
        ..PortalTransition::default()
    };
    rig.with_call(|call, commands| commands.on_portal_transition(call, &transition));
}

// C# source: MossTankPanelTests.UbCommands.cs :: AHopAlreadyUnderWayDoesNotEndThePortalUse (on main; not yet in this branch's C# tree)
#[test]
fn a_hop_already_under_way_does_not_end_the_portal_use() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.host.meta.objects = vec![landscape(92, "Gateway", ObjectClass::Portal, 5.0)];
    portal_hop(&mut rig, 7, false);

    rig.ub("portal Gateway");
    portal_hop(&mut rig, 7, true);

    assert!(
        !rig.messages()
            .contains(&"[UB] portal used successfully".to_owned())
    );
    assert!(navigation_locked(&rig));
    rig.tick(0.3);
    assert_eq!(vec![92], rig.host.used);

    portal_hop(&mut rig, 8, false);

    assert!(
        rig.messages()
            .contains(&"[UB] portal used successfully".to_owned())
    );
    assert!(!navigation_locked(&rig));
}

// C# source: MossTankPanelTests.UbCommands.cs :: APortalNeverEnteredIsTriedPortalAttemptsTimesThenGivenUp (on main; not yet in this branch's C# tree)
#[test]
fn a_portal_never_entered_is_tried_portal_attempts_times_then_given_up() {
    let mut rig = Rig::new().at_heading(90.0);
    rig.host.meta.objects = vec![landscape(92, "Gateway", ObjectClass::Portal, 5.0)];

    rig.ub("portal Gateway");
    assert!(navigation_locked(&rig));

    // Tries at a quarter second, then five seconds apart.
    rig.tick(0.2);
    assert!(rig.host.used.is_empty());
    rig.tick(0.1);
    assert_eq!(1, rig.host.used.len());
    rig.tick(4.9);
    assert_eq!(1, rig.host.used.len());
    rig.tick(0.2);
    assert_eq!(2, rig.host.used.len());
    rig.tick(5.1);
    assert_eq!(vec![92, 92, 92], rig.host.used);
    assert!(navigation_locked(&rig));
    assert!(
        !rig.messages()
            .iter()
            .any(|line| line.contains("Unable to use portal"))
    );

    rig.tick(5.1);

    assert_eq!(3, rig.host.used.len());
    let messages = rig.messages();
    assert_eq!(
        [
            "[UB] Plugin: Unable to use portal Gateway",
            "[UB] failed to use portal"
        ],
        messages[messages.len() - 2..]
    );
    assert_eq!(vec![89.0f32], rig.host.faced_headings);
    assert!(!navigation_locked(&rig));

    // Over: nothing more is tried.
    rig.tick(10.0);
    assert_eq!(3, rig.host.used.len());
}

// C# source: MossTankPanelTests.UbCommands.cs :: ThePortalSettingsShapeTheUseAndItsOutcome (on main; not yet in this branch's C# tree)
#[test]
fn the_portal_settings_shape_the_use_and_its_outcome() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.host.meta.objects = vec![landscape(92, "Gateway", ObjectClass::Portal, 5.0)];
    rig.ub("opt set Plugin.PortalThink True");
    rig.ub("opt set Plugin.PortalTimeout 1000");
    rig.ub("opt set Plugin.PortalAttempts 1");

    rig.ub("portal Gateway");
    rig.tick(0.3);
    assert_eq!(1, rig.host.used.len());
    rig.tick(1.1);

    assert_eq!(1, rig.host.used.len());
    assert_eq!(
        Some(&"/t Test Character, failed to use portal".to_owned()),
        rig.submitted().last()
    );
    assert!(!navigation_locked(&rig));

    rig.ub("portal Gateway");
    rig.tick(0.3);
    enter_portal_space(&mut rig, 2);

    assert_eq!(
        Some(&"/t Test Character, portal used successfully".to_owned()),
        rig.submitted().last()
    );
    assert!(!navigation_locked(&rig));
}

// C# source: MossTankPanelTests.UbCommands.cs :: APortalUseHoldsTheRouteUntilPortalSpaceAndThenLetsItGo (on main; not yet in this branch's C# tree)
// (the lock and its release; the route standing still is pinned in the
// plugin crate's panel_ub_commands).
#[test]
fn entering_portal_space_ends_the_portal_use_and_lets_the_route_go() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.host.meta.objects = vec![landscape(92, "Gateway", ObjectClass::Portal, 5.0)];

    rig.ub("use Gateway");
    assert!(
        rig.messages()
            .contains(&"[UB] Attempting to use portal: Gateway".to_owned())
    );
    for _ in 0..60 {
        rig.tick(0.05);
        assert!(navigation_locked(&rig));
    }
    assert_eq!(vec![92], rig.host.used);

    enter_portal_space(&mut rig, 1);

    assert_eq!(
        Some(&"[UB] portal used successfully".to_owned()),
        rig.messages().last()
    );
    assert!(!navigation_locked(&rig));
    rig.tick(10.0);
    assert_eq!(vec![92], rig.host.used);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: APortalNobodyCanSeeIsReported
#[test]
fn a_portal_nobody_can_see_is_reported() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.ub("portal Gateway");
    assert_eq!("[UB] Could not find a portal", rig.single_message());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: FollowByPartialNameNamesThePlayerItLockedOn
#[test]
fn follow_by_partial_name_names_the_player_it_locked_on() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.host.meta.objects = vec![player(0x5000_0ABC, "Zero Cool")];
    rig.ub("followp Zero");
    assert!(
        rig.messages()
            .contains(&"[UB] Following Zero Cool[0x50000ABC]".to_owned())
    );
    assert_eq!(
        vec![(0x5000_0ABC, "Zero Cool".to_owned())],
        rig.services.follows
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: FollowingNobodySaysWhoWasNotFound
#[test]
fn following_nobody_says_who_was_not_found() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.ub("follow Zero Cool");
    assert_eq!(
        "[UB] Error: Could not find player Zero Cool",
        rig.single_message()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: AnAllegianceCommandThatFindsNobodyNamesTheVerb
#[test]
fn an_allegiance_command_that_finds_nobody_names_the_verb() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.ub("swearallegiancep Zero");
    assert_eq!(
        "[UB] Error: Could not find player Zero Command:swearallegiancep",
        rig.single_message()
    );
}

// ---- use, select, close ----------------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: UseOnItsOwnUsesTheNamedItem
#[test]
fn use_on_its_own_uses_the_named_item() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.host.meta.objects = vec![carried(100, "Prismatic Taper")];
    rig.ub("use Prismatic Taper");
    assert_eq!(vec![100], rig.host.used);
    assert!(
        rig.messages()
            .contains(&"[UB] using Prismatic Taper".to_owned())
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: UseOnASecondItemAppliesTheFirstToTheSecond
#[test]
fn use_on_a_second_item_applies_the_first_to_the_second() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.host.meta.objects = vec![carried(101, "Splitting Tool"), carried(102, "Gold Pea")];
    rig.ub("usep splitting on gold pea");
    assert_eq!(vec![(101, 102)], rig.host.applied);
    assert!(
        rig.messages()
            .contains(&"[UB] using Splitting Tool on Gold Pea".to_owned())
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: UseRefusesBothPlaceFlagsAtOnce
#[test]
fn use_refuses_both_place_flags_at_once() {
    let mut rig = Rig::new();
    rig.ub("useli Cake");
    assert_eq!(
        "[UB] l and i cannot be used in the same command",
        rig.single_message()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TheInventoryFlagWillNotReachTheLandscape
#[test]
fn the_inventory_flag_will_not_reach_the_landscape() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.host.meta.objects = vec![landscape(103, "Cake", ObjectClass::Food, 3.0)];
    rig.ub("usei Cake");
    assert!(rig.host.used.is_empty());
    assert!(
        rig.messages()
            .contains(&"[UB] Error: Could not find object: Cake".to_owned())
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: SelectPicksTheNamedItem
#[test]
fn select_picks_the_named_item() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.host.meta.objects = vec![carried(104, "Gold Scarab")];
    rig.ub("selectp gold");
    assert_eq!(Some(104), rig.host.meta.selection.selected);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: CloseCorpseClosesTheOpenCorpse
#[test]
fn close_corpse_closes_the_open_corpse() {
    let mut rig = Rig::new();
    rig.host.meta.open_container = 110;
    rig.host.meta.objects = vec![WorldObject {
        object_id: 110,
        name: "Corpse of a Drudge".to_owned(),
        object_class: ObjectClass::Corpse,
        ..WorldObject::default()
    }];
    rig.ub("close corpse");
    assert_eq!(vec![110], rig.host.closed_loot);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: CloseCorpseWithNothingOpenSaysSo
#[test]
fn close_corpse_with_nothing_open_says_so() {
    let mut rig = Rig::new();
    rig.ub("close corpse");
    assert_eq!("[UB] No container is currently open.", rig.single_message());
    assert!(rig.host.closed_loot.is_empty());
    let _ = LootContainer::default();
}

// ---- swearallegiance, breakallegiance --------------------------------------------------

fn allegiance_rig(status: AllegianceStatus, notice: Option<&str>) -> Rig {
    let mut rig = Rig::new().at_heading(0.0);
    rig.host.meta.objects = vec![player(0x5000_0AAA, "Yonneh")];
    rig.host.allegiance_result = AllegianceResult {
        status,
        notice: notice.map(str::to_owned),
    };
    rig
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: SwearingAllegianceSendsTheResolvedPlayerToTheClient
#[test]
fn swearing_allegiance_sends_the_resolved_player_to_the_client() {
    let mut rig = allegiance_rig(AllegianceStatus::Sent, None);
    rig.ub("swearallegiancep Yon");
    assert_eq!(vec![0x5000_0AAA], rig.host.swears);
    assert_eq!(
        "[UB] Swearing Allegiance to Yonneh[0x50000AAA]",
        rig.single_message()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: BreakingAllegianceSendsTheResolvedPlayerToTheClient
#[test]
fn breaking_allegiance_sends_the_resolved_player_to_the_client() {
    let mut rig = allegiance_rig(AllegianceStatus::Sent, None);
    rig.ub("breakallegiance Yonneh");
    assert_eq!(vec![0x5000_0AAA], rig.host.breaks);
    assert_eq!(
        "[UB] Breaking Allegiance from Yonneh[0x50000AAA]",
        rig.single_message()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: EveryAllegianceAnswerHasItsOwnLine
#[test]
fn every_allegiance_answer_has_its_own_line() {
    for (status, notice, swear_line, break_line) in [
        (
            AllegianceStatus::InvalidTarget,
            None,
            "Cannot swear allegiance to Yonneh[0x50000AAA].",
            "Yonneh[0x50000AAA] is not in your allegiance.",
        ),
        (
            AllegianceStatus::Refused,
            Some("You are already sworn to somebody."),
            "You are already sworn to somebody.",
            "You are already sworn to somebody.",
        ),
        (
            AllegianceStatus::Refused,
            None,
            "Refused to swear allegiance to Yonneh[0x50000AAA].",
            "Refused to break allegiance from Yonneh[0x50000AAA].",
        ),
        (
            AllegianceStatus::Unavailable,
            None,
            "Cannot swear allegiance right now.",
            "Cannot break allegiance right now.",
        ),
    ] {
        let mut rig = allegiance_rig(status, notice);
        rig.ub("swearallegiance Yonneh");
        assert_eq!(format!("[UB] Error: {swear_line}"), rig.single_message());
        rig.clear_messages();
        rig.ub("breakallegiance Yonneh");
        assert_eq!(format!("[UB] Error: {break_line}"), rig.single_message());
    }
}

// ---- printcolors, listvars -------------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: PrintColorsWritesOneLineInEveryTextClass
#[test]
fn print_colors_writes_one_line_in_every_text_class() {
    let mut rig = Rig::new();
    rig.ub("printcolors");
    let posted = rig.posted();
    assert_eq!(MESSAGE_TYPES.len(), posted.len());
    assert_eq!(("[PrintColors]Broadcast (0)".to_owned(), 0), posted[0]);
    assert!(posted.contains(&("[PrintColors]Magic (7)".to_owned(), 7)));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: ListVarsPrintsTheSessionStore
#[test]
fn list_vars_prints_the_session_store() {
    let mut rig = Rig::new();
    rig.evaluate("setvar[myvar,1]");
    rig.clear_messages();
    rig.ub("listvars");
    let messages = rig.messages();
    assert_eq!("[UB] Defined variables:", messages[0]);
    assert!(messages.iter().any(|line| line.starts_with("myvar (")));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: ListVarsNamesEachTypeAsTheReferenceDoes
#[test]
fn list_vars_names_each_type_as_the_reference_does() {
    let mut rig = Rig::new();
    rig.evaluate("setvar[anumber,2.5]");
    rig.evaluate("setvar[astring,`hi`]");
    rig.evaluate("setvar[aflag,1==1]");
    rig.evaluate("setvar[alist,listcreate[1,2]]");
    rig.clear_messages();
    rig.ub("listvars");
    assert_eq!(
        vec![
            "[UB] Defined variables:",
            "aflag (number) = 1",
            "alist (List) = [1,2]",
            "anumber (number) = 2.5",
            "astring (string) = hi",
        ],
        rig.messages()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: ListPvarsAndListGvarsReadTheirOwnStores
#[test]
fn list_pvars_and_list_gvars_read_their_own_stores() {
    let mut rig = Rig::new();
    rig.evaluate("setvar[sessiononly,1]");
    rig.evaluate("setgvar[serverwide,2]");
    rig.clear_messages();
    rig.ub("listpvars");
    rig.ub("listgvars");
    let messages = rig.messages();
    assert_eq!("[UB] Defined persistent variables:", messages[0]);
    assert!(!messages.iter().any(|line| line.starts_with("sessiononly ")));
    assert!(messages.iter().any(|line| line.starts_with("serverwide (")));
}

// ---- translateroute --------------------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TranslateRouteReportsOnlyInTheDebugOutput
#[test]
fn translate_route_reports_only_in_the_debug_output() {
    let mut rig = Rig::new().at_heading(0.0);
    // The route store's translation is its own test's; here the store
    // answers that it saved one record.
    rig.services.translate_result = Some(Ok(("eo-main.nav".to_owned(), 1)));
    rig.ub("translateroute 0x00640371 eo-east.nav 0x01640371 eo-main.nav");
    assert!(rig.messages().is_empty());

    rig.ub("opt set Plugin.Debug true");
    rig.clear_messages();
    rig.ub("translateroute 0x00640371 eo-east.nav 0x01640371 eo-main.nav");

    let posted = rig.posted();
    assert_eq!(2, posted.len());
    assert!(posted.iter().all(|(_, kind)| *kind == DEBUG_CHAT_TYPE));
    assert_eq!(
        "[UB] VTank: Translating route: RouteToLoad:eo-east.nav StartLandblock:0x00640371 EndLandblock:0x01640371 RouteToSaveAs:eo-main.nav Force:False",
        posted[0].0
    );
    assert!(posted[1].0.starts_with(
        "[UB] VTank: Translated 1 records from 00640371 to 01640371 by adding offsets NS:0 EW:0.8\nSaved to file: "
    ));
    // One landblock step east is 192 m, which is 0.8 map units.
    let (from, to, east_west, north_south, force) = rig.services.translations[1].clone();
    assert_eq!(("eo-east.nav", "eo-main.nav"), (from.as_str(), to.as_str()));
    assert!((east_west - 0.8).abs() < 1e-12);
    assert!(north_south.abs() < 1e-12);
    assert!(!force);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TranslateRouteRefusesALandblockThatIsNotHex
#[test]
fn translate_route_refuses_a_landblock_that_is_not_hex() {
    let mut rig = Rig::new();
    rig.ub("translateroute zzz a.nav 0x00650371 b.nav");
    assert_eq!(
        "[UB] Error: VTank: Could not parse hex value from StartLandblock: zzz",
        rig.single_message()
    );
}

// ---- Not a port of a C# test: names outside ASCII ------------------------------
//
// A regression test for the line split: the ignore-case search answers in
// UTF-16 units, and cutting "Épée on Pépite" at that number as a byte offset
// panicked (a cut inside "é").

#[test]
fn use_splits_a_line_with_accented_names_at_on() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.host.meta.objects = vec![carried(101, "Épée"), carried(102, "Pépite 🍰")];
    rig.ub("use Épée on Pépite 🍰");
    assert_eq!(vec![(101, 102)], rig.host.applied);
    assert!(
        rig.messages()
            .contains(&"[UB] using Épée on Pépite 🍰".to_owned())
    );
}

//! The `/ub` word and the two commands metas lean on that the client had
//! no answer for, setmotion and prepclick, against
//! `MossTankPanelTests.UbVerb`. The `/vt` halves of the C# tests (the same
//! line on `/vt` is an unknown command) belong to the `/vt` dispatcher and
//! are ported with it.

mod common;

use ac_plugin_abi::navigation::{MoveDirection, MovePace, MoveUnit, MovementIntent};
use ac_plugin_abi::world::Confirmation;
use common::{PLUGIN_VERSION, Rig, carried};

fn confirm(rig: &mut Rig, context_id: u32, text: &str) {
    let confirmation = Confirmation {
        context_id,
        kind: 2,
        text: text.to_owned(),
    };
    rig.with_call(|call, commands| commands.on_confirmation(call, &confirmation));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: UbFaceTurnsAndVtFaceIsUnknown
#[test]
fn ub_face_turns() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.ub("face 90");
    rig.tick(0.05);
    assert_eq!(Some(&90.0f32), rig.host.faced_headings.last());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: UbJumpswJumps
#[test]
fn ub_jumpsw_jumps() {
    let mut rig = Rig::new().at_heading(180.0);
    rig.ub("jumpsw 180 500");
    rig.tick(0.1);
    assert_eq!(
        vec![(
            MoveDirection::Forward,
            MovePace::Walk,
            0.0,
            MoveUnit::MetersOrDegrees
        )],
        rig.host.moves
    );
    assert_eq!(vec![0.5f32], rig.host.jumps);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: UbUseUsesAndVtUseIsUnknown
#[test]
fn ub_use_uses() {
    let mut rig = Rig::new().at_heading(0.0);
    rig.host.meta.objects = vec![carried(100, "Prismatic Taper")];
    rig.ub("use Prismatic Taper");
    assert_eq!(vec![100], rig.host.used);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: BareUbPrintsTheCompatibilityLineAndUbHelpIsItsOwn
#[test]
fn bare_ub_prints_the_compatibility_line_and_ub_help_is_its_own() {
    let mut rig = Rig::new();
    rig.ub("");
    assert_eq!(
        vec![
            format!("[UB] MossTank UB compatibility {PLUGIN_VERSION}"),
            " Type `/ub help` or `/ub help <command>` for help.".to_owned(),
        ],
        rig.messages()
    );

    rig.clear_messages();
    rig.ub("help");
    let help = rig.messages();
    assert!(help[0].starts_with("[UB] All available UB commands: /ub {"));
    assert!(help[0].contains("setmotion"));
    assert!(!help[0].contains("setmetastate"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: SetMotionPressesTheNamedKey
#[test]
fn set_motion_presses_the_named_key() {
    for (typed, key) in [
        ("forward", "Forward"),
        ("BACKWARD", "Backward"),
        ("TurnRight", "TurnRight"),
        ("turnleft", "TurnLeft"),
        ("StrafeRight", "StrafeRight"),
        ("strafeLeft", "StrafeLeft"),
    ] {
        let mut rig = Rig::new();
        rig.ub(&format!("setmotion {typed} 1"));
        assert_eq!(1, rig.host.meta.intents.len(), "{typed}");
        assert_eq!(
            MovementIntent {
                forward: key == "Forward",
                backward: key == "Backward",
                strafe_left: key == "StrafeLeft",
                strafe_right: key == "StrafeRight",
                turn_left: key == "TurnLeft",
                turn_right: key == "TurnRight",
                run: true,
                jump: false,
            },
            rig.host.meta.intents[0],
            "{typed}"
        );
        assert!(rig.messages().is_empty());
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: ForwardOnThenOffLeavesNoIntent
#[test]
fn forward_on_then_off_leaves_no_intent() {
    let mut rig = Rig::new();
    rig.ub("setmotion forward 1");
    rig.ub("setmotion forward 0");
    assert_eq!(1, rig.host.meta.intents.len());
    assert_eq!(1, rig.host.meta.clear_movement_count);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: ForwardAndTurnRightAreHeldTogether
#[test]
fn forward_and_turn_right_are_held_together() {
    let mut rig = Rig::new();
    rig.ub("setmotion forward 1");
    rig.ub("setmotion turnright 1");
    let both = rig.host.meta.intents.last().unwrap().clone();
    assert!(both.forward && both.turn_right && both.run);

    rig.ub("setmotion turnright 0");
    let forward_only = rig.host.meta.intents.last().unwrap().clone();
    assert!(forward_only.forward);
    assert!(!forward_only.turn_right);
    assert_eq!(0, rig.host.meta.clear_movement_count);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: WalkTurnsRunOffForTheHeldKeysAndMovesNothingAlone
#[test]
fn walk_turns_run_off_for_the_held_keys_and_moves_nothing_alone() {
    let mut rig = Rig::new();
    rig.ub("setmotion walk 1");
    assert!(rig.host.meta.intents.is_empty());

    rig.ub("setmotion forward 1");
    assert_eq!(
        MovementIntent {
            forward: true,
            run: false,
            ..MovementIntent::default()
        },
        *rig.host.meta.intents.last().unwrap()
    );

    rig.ub("setmotion walk 0");
    assert_eq!(
        MovementIntent {
            forward: true,
            run: true,
            ..MovementIntent::default()
        },
        *rig.host.meta.intents.last().unwrap()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: TheCommandReleasesAKeyTheExpressionPressed
#[test]
fn the_command_releases_a_key_the_expression_pressed() {
    let mut rig = Rig::new();
    // The C# presses it with `/vt mexec`; the expression is the same.
    rig.evaluate("setmotion[`Forward`,1]");
    rig.ub("setmotion turnleft 1");
    let last = rig.host.meta.intents.last().unwrap().clone();
    assert!(last.forward && last.turn_left);

    rig.ub("setmotion turnleft 0");
    rig.ub("setmotion forward 0");
    assert_eq!(1, rig.host.meta.clear_movement_count);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: BadSetMotionInputPrintsTheUsageLine
#[test]
fn bad_set_motion_input_prints_the_usage_line() {
    for line in [
        "setmotion",
        "setmotion forward",
        "setmotion forward 2",
        "setmotion forward on",
    ] {
        let mut rig = Rig::new();
        rig.ub(line);
        assert!(rig.host.meta.intents.is_empty(), "{line}");
        assert_eq!(
            vec![
                "[UB] Error: Bad command syntax",
                "[UB] Usage: /ub setmotion <Forward|Backward|TurnRight|TurnLeft|StrafeRight|StrafeLeft|Walk> <0|1>",
            ],
            rig.messages()[..2],
            "{line}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: AnUnknownMotionNameListsTheValidOnes
#[test]
fn an_unknown_motion_name_lists_the_valid_ones() {
    let mut rig = Rig::new();
    rig.ub("setmotion sideways 1");
    assert!(rig.host.meta.intents.is_empty());
    assert_eq!(
        "[UB] Error: Invalid option (sideways). Valid values are: Forward, Backward, TurnRight, TurnLeft, StrafeRight, StrafeLeft, Walk",
        rig.single_message()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: PrepClickAnswersTheNextConfirmationWithTheChoice
#[test]
fn prep_click_answers_the_next_confirmation_with_the_choice() {
    for (choice, accept, said) in [
        (
            "yes",
            true,
            "[UB] PrepClick: Click Yes on Swear allegiance?",
        ),
        ("no", false, "[UB] PrepClick: Click No on Swear allegiance?"),
    ] {
        let mut rig = Rig::new();
        rig.ub(&format!("prepclick {choice} 10"));
        assert_eq!(
            format!(
                "[UB] PrepClick: Will click {choice} on the next dialog to appear within 10 seconds"
            ),
            *rig.messages().last().unwrap()
        );
        confirm(&mut rig, 11, "Swear allegiance?");
        assert_eq!(vec![(11, accept)], rig.host.answered);
        assert_eq!(said, rig.messages().last().unwrap());
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: PrepClickAnswersOnlyOneConfirmation
#[test]
fn prep_click_answers_only_one_confirmation() {
    let mut rig = Rig::new();
    rig.ub("prepclick yes 10");
    confirm(&mut rig, 11, "first");
    confirm(&mut rig, 12, "second");
    assert_eq!(vec![(11, true)], rig.host.answered);
    assert!(!rig.commands.prep_click().is_armed());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: PrepClickExpiresAtTheEndOfItsWindow
#[test]
fn prep_click_expires_at_the_end_of_its_window() {
    let mut rig = Rig::new();
    rig.ub("prepclick yes 2");
    rig.tick(1.5);
    assert!(
        !rig.messages()
            .iter()
            .any(|line| line.contains("Time has expired"))
    );
    rig.tick(1.0);
    assert_eq!(
        "[UB] PrepClick: Time has expired: 2",
        rig.messages().last().unwrap()
    );
    confirm(&mut rig, 11, "late");
    assert!(rig.host.answered.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: PrepClickStopCancelsTheWatch
#[test]
fn prep_click_stop_cancels_the_watch() {
    let mut rig = Rig::new();
    rig.ub("prepclick stop");
    assert_eq!(
        "[UB] PrepClick: Message boxes are not currently being watched",
        rig.messages().last().unwrap()
    );

    rig.ub("prepclick no 10");
    rig.tick(2.5);
    rig.ub("prepclick stop");
    assert_eq!(
        "[UB] PrepClick: Stopping... 2.5s passed out of expected 10s",
        rig.messages().last().unwrap()
    );
    confirm(&mut rig, 11, "after stop");
    assert!(rig.host.answered.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: AConfirmationBeforeTheCommandIsNotAnswered
#[test]
fn a_confirmation_before_the_command_is_not_answered() {
    let mut rig = Rig::new();
    confirm(&mut rig, 10, "before");
    rig.ub("prepclick yes 10");
    assert!(rig.host.answered.is_empty());
    confirm(&mut rig, 11, "after");
    assert_eq!(vec![(11, true)], rig.host.answered);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: BadPrepClickInputPrintsTheUsageLine
#[test]
fn bad_prep_click_input_prints_the_usage_line() {
    for line in ["prepclick", "prepclick maybe 10", "prepclick yes ten"] {
        let mut rig = Rig::new();
        rig.ub(line);
        assert!(!rig.commands.prep_click().is_armed());
        assert_eq!(
            vec![
                "[MossTank] Bad command syntax",
                "[MossTank] Usage: /ub prepclick {stop|yes <secondstowatch>|no <secondstowatch>}",
            ],
            rig.messages(),
            "{line}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: AWindowOverAnHourIsRefused
#[test]
fn a_window_over_an_hour_is_refused() {
    let mut rig = Rig::new();
    rig.ub("prepclick yes 3601");
    assert!(!rig.commands.prep_click().is_armed());
    assert_eq!(
        "[UB] PrepClick: 3601 is not a valid number of seconds to wait",
        rig.single_message()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbVerb.cs :: TheNewCommandsHaveUsageLines
#[test]
fn the_new_commands_have_usage_lines() {
    for (command, usage) in [
        (
            "setmotion",
            "[UB] Usage: /ub setmotion <Forward|Backward|TurnRight|TurnLeft|StrafeRight|StrafeLeft|Walk> <0|1>",
        ),
        (
            "prepclick",
            "[UB] Usage: /ub prepclick {stop|yes <secondstowatch>|no <secondstowatch>}",
        ),
        ("clearmotion", "[UB] Usage: /ub clearmotion"),
    ] {
        let mut rig = Rig::new();
        rig.ub(&format!("help {command}"));
        assert_eq!(usage, rig.messages()[0]);
    }
}

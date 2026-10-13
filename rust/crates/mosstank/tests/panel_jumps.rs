//! The macro's own jumps (`/vt jump`, `tapjump`, `addnavjump`) and the
//! route editor's jump button, as the client sees them (ported from the
//! C#'s `MossTankPanelTests`).

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::navigation::{
    MoveChannel, MoveDirection, MovePace, MoveRequest, MoveUnit, NavigationSnapshot,
    NavigationStatus,
};
use ac_plugin_abi::world::Position;
use common::World;
use mosstank::MossTank;

fn facing(world: &mut World, heading: f32) {
    world.set_reply::<calls::NavigationSnapshot>(&NavigationSnapshot {
        is_available: true,
        local_object_id: 1,
        position: Position {
            cell_id: 0x7F7F_0001,
            heading_degrees: heading,
            is_outdoor: true,
            ..Position::default()
        },
        ..NavigationSnapshot::default()
    });
}

/// A client that takes every movement (`FakeAutomation`'s answers).
fn jumper(heading: f32) -> (World, MossTank) {
    let mut world = World::panel_character("Jumper");
    facing(&mut world, heading);
    let accepted = NavigationStatus::Accepted;
    world.set_reply::<calls::NavigationJump>(&accepted);
    world.set_reply::<calls::NavigationMove>(&accepted);
    world.set_reply::<calls::NavigationSetIntent>(&accepted);
    world.set_reply::<calls::NavigationClearIntent>(&accepted);
    world.set_reply::<calls::NavigationFace>(&accepted);
    let plugin = world.enable();
    (world, plugin)
}

fn tick(world: &mut World, plugin: &mut MossTank, seconds: f64) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, seconds));
}

fn jumps(world: &World) -> Vec<f32> {
    world.requests::<calls::NavigationJump>()
}

fn moves(world: &World) -> Vec<MoveRequest> {
    world.requests::<calls::NavigationMove>()
}

fn held(direction: MoveDirection, pace: MovePace) -> MoveRequest {
    MoveRequest {
        direction,
        pace,
        amount: 0.0,
        unit: MoveUnit::MetersOrDegrees,
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtankJumpTurnsToTheRequestedHeadingBeforeChargingAndReleasing
#[test]
fn the_jump_turns_to_its_heading_before_charging_and_releasing() {
    let (mut world, mut plugin) = jumper(90.0);
    let p = &mut plugin;
    world.vt(p, "jump 180 false 100");
    tick(&mut world, p, 0.05);
    let intents = world.requests::<calls::NavigationSetIntent>();
    assert!(intents.last().unwrap().turn_right);
    assert!(jumps(&world).is_empty());

    facing(&mut world, 180.0);
    let clears = world.requests::<calls::NavigationClearIntent>().len();
    tick(&mut world, p, 0.01);
    assert_eq!(
        world.requests::<calls::NavigationClearIntent>().len(),
        clears + 1
    );
    assert_eq!(jumps(&world), [0.1]);
    assert_eq!(moves(&world), [held(MoveDirection::Forward, MovePace::Run)]);
    let intents = world.requests::<calls::NavigationSetIntent>().len();
    tick(&mut world, p, 0.1);
    assert_eq!(
        world.requests::<calls::NavigationSetIntent>().len(),
        intents
    );
    assert_eq!(jumps(&world).len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtankJumpCanGoBackwardsAndARouteJumpRemembersThat
#[test]
fn a_jump_can_go_backwards_and_a_route_jump_remembers_it() {
    let (mut world, mut plugin) = jumper(180.0);
    let p = &mut plugin;
    world.vt(p, "jump 180 false 100 backward");
    tick(&mut world, p, 0.05);
    assert_eq!(
        moves(&world),
        [held(MoveDirection::Backward, MovePace::Run)]
    );
    assert_eq!(jumps(&world), [0.1]);

    world.vt(p, "addnavjump 180 false 100 backward");
    let rows = world.ui_list(p, "RouteRows");
    assert!(rows.last().unwrap().contains("Backward"), "{rows:?}");
    assert!(world.chat().iter().any(|line| {
        let lower = line.to_lowercase();
        lower.contains("backward") && lower.contains("forward")
    }));
}

fn shift_walks(word: &str, command: &str, pace: MovePace) {
    let (mut world, mut plugin) = jumper(180.0);
    let p = &mut plugin;
    world.command(p, word, command);
    tick(&mut world, p, 0.05);
    assert_eq!(
        moves(&world),
        [held(MoveDirection::Forward, pace)],
        "/{word} {command}"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ShiftWalksAndEveryOtherJumpRuns
#[test]
fn shift_walks_and_every_other_jump_runs() {
    shift_walks("vt", "jump 180 true 300", MovePace::Walk);
    shift_walks("vt", "jump 180 false 300", MovePace::Run);
    shift_walks("vt", "tapjump", MovePace::Run);
    shift_walks("ub", "jumpsw 300", MovePace::Walk);
    shift_walks("ub", "jumpw 300", MovePace::Run);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TheRouteEditorsJumpButtonAddsARunningJump
#[test]
fn the_route_editors_jump_button_adds_a_running_jump() {
    let (mut world, mut plugin) = jumper(180.0);
    let p = &mut plugin;
    world.act(p, "AddRouteJump");
    let rows = world.ui_list(p, "RouteRows");
    let last = rows.last().unwrap();
    assert!(last.contains("Jump:"), "{last}");
    assert!(!last.contains("Shift"), "{last}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtankJumpsLeaveWithExactlyThePowerAsked
#[test]
fn the_jumps_leave_with_exactly_the_power_asked() {
    for (command, power) in [
        ("tapjump", 0.1f32),
        ("jump 180 false 250", 0.25),
        ("jump 180 true 5000", 1.0),
    ] {
        let (mut world, mut plugin) = jumper(180.0);
        let p = &mut plugin;
        world.vt(p, command);
        tick(&mut world, p, 0.05);
        assert_eq!(jumps(&world), [power], "{command}");
        assert!(
            !world
                .requests::<calls::NavigationSetIntent>()
                .iter()
                .any(|intent| intent.jump),
            "{command}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AJumpTheClientRefusesFailsAndLetsGoOfItsKey
#[test]
fn a_jump_the_client_refuses_fails_and_lets_go_of_its_key() {
    let (mut world, mut plugin) = jumper(180.0);
    let p = &mut plugin;
    world.set_reply::<calls::NavigationJump>(&NavigationStatus::Rejected);
    world.vt(p, "jump 180 true 300");
    tick(&mut world, p, 0.05);
    assert_eq!(
        world.chat().last().unwrap(),
        "[MossTank] Jump command was refused by the host."
    );
    assert_eq!(
        world.requests::<calls::NavigationStopMoving>(),
        [Some(MoveChannel::Travel)]
    );
}

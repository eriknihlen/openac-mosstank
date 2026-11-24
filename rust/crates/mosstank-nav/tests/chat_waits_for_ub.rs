//! A chat waypoint waits on the plugin's UB tools: it sends nothing while
//! one runs, and after sending a `/ub` line it holds the route until the
//! plugin has run the line and the tool it started has finished.
//!
//! Not a port of a C# test: the reference's chat waypoint knows nothing of
//! the UB tools (a MossTank extension, register NAV-8).

mod common;

use common::{FakeAutomation, Rig, position, rig, snapshot, waypoint};
use mosstank_formats::route::{RouteMode, RouteWaypointType};
use mosstank_nav::UbActivity;

fn chat_then_pause(text: &str) -> Rig {
    let mut chat = waypoint(RouteWaypointType::ChatCommand, position(0.0, 0.0));
    chat.text = text.to_owned();
    let mut pause = waypoint(RouteWaypointType::Pause, position(0.0, 0.0));
    pause.duration_milliseconds = 1000;
    rig(
        FakeAutomation::at(snapshot(position(0.0, 0.0))),
        RouteMode::Once,
        vec![chat, pause],
    )
}

fn busy(rig: &mut Rig, tool: Option<&'static str>) {
    rig.automation.ub_activity.busy_tool = tool;
}

/// The plugin runs the line the route submitted.
fn run_line(rig: &mut Rig) {
    rig.automation.ub_activity.lines_run += 1;
}

/// Ticks for so many seconds, a tenth of a second at a time.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn run_for(rig: &mut Rig, seconds: f64) {
    for _ in 0..(seconds / 0.1).round() as usize {
        rig.tick(0.1, true);
    }
}

/// With no UB tool about, a chat waypoint is as before: the 200 ms guard,
/// the line, and the route goes on in the same tick.
#[test]
fn a_chat_line_with_no_ub_tool_running_is_sent_after_the_guard_and_goes_on() {
    let mut rig = chat_then_pause("/say route");

    assert!(rig.tick(0.19, true));
    assert!(rig.automation.submitted_chat.is_empty());
    assert!(rig.tick(0.01, true));
    assert_eq!(rig.automation.submitted_chat, ["/say route"]);
    assert_eq!(rig.index(), 1);
    assert!(
        rig.automation.logged.is_empty(),
        "{:?}",
        rig.automation.logged
    );
}

/// A `/ub` line that starts nothing holds the route only until the plugin
/// has run it.
#[test]
fn a_ub_line_holds_the_route_until_the_plugin_has_run_it() {
    let mut rig = chat_then_pause("/ub use Gateway");

    assert!(rig.tick(0.2, true));
    assert_eq!(rig.automation.submitted_chat, ["/ub use Gateway"]);
    assert!(rig.tick(0.1, true));
    assert_eq!(rig.index(), 0);
    assert_eq!(rig.status(), "Waiting for UB to run: /ub use Gateway");

    run_line(&mut rig);
    assert!(rig.tick(0.1, true));
    assert_eq!(rig.index(), 1);
}

/// While a UB tool runs, the line is not sent; it goes once the tool is
/// done.
#[test]
fn a_chat_line_waits_for_a_running_ub_tool_before_it_is_sent() {
    let mut rig = chat_then_pause("/ub give Taper to Mule");
    busy(&mut rig, Some("give"));

    run_for(&mut rig, 5.0);
    assert!(rig.automation.submitted_chat.is_empty());
    assert_eq!(rig.index(), 0);
    assert_eq!(rig.status(), "Waiting for UB give to finish.");
    assert_eq!(rig.automation.logged, ["Waiting for UB give to finish."]);

    busy(&mut rig, None);
    assert!(rig.tick(0.1, true));
    assert_eq!(rig.automation.submitted_chat, ["/ub give Taper to Mule"]);
}

/// A give the line started holds the route until it ends, a stop as well as
/// a finish; the pause after it then counts its whole second.
#[test]
fn the_give_a_line_started_holds_the_route_until_it_is_stopped() {
    let mut rig = chat_then_pause("/ub give Taper to Mule");

    assert!(rig.tick(0.2, true));
    run_line(&mut rig);
    busy(&mut rig, Some("give"));
    run_for(&mut rig, 7.0);
    assert_eq!(rig.index(), 0);
    assert_eq!(rig.automation.logged, ["Waiting for UB give to finish."]);

    // `/ub give stop`.
    busy(&mut rig, None);
    assert!(rig.tick(0.1, true));
    assert_eq!(rig.index(), 1);
    assert!(rig.tick(0.5, true));
    assert_eq!(rig.index(), 1, "{}", rig.status());
    assert!(rig.tick(0.5, true));
    assert!(!rig.tick(0.1, true), "the once route is done");
}

/// A tool that never finishes holds the route for 120 s, then the waypoint
/// says so and goes on.
#[test]
fn a_ub_tool_that_never_finishes_lets_the_route_go_on_after_120_seconds() {
    let mut rig = chat_then_pause("/ub give Taper to Mule");

    assert!(rig.tick(0.2, true));
    run_line(&mut rig);
    busy(&mut rig, Some("give"));
    run_for(&mut rig, 119.0);
    assert_eq!(rig.index(), 0);
    assert!(rig.automation.posted_system_messages.is_empty());

    run_for(&mut rig, 1.5);
    assert_eq!(rig.index(), 1);
    let line = "UB give still running after 120 s, continuing.";
    assert_eq!(
        rig.automation.posted_system_messages,
        [format!("[MossTank] {line}")]
    );
    assert_eq!(
        rig.automation.logged,
        ["Waiting for UB give to finish.", line]
    );
}

/// A tool running before the line was sent counts its 120 s from the start
/// of the wait, and the line still goes out when they run out.
#[test]
fn a_line_held_back_by_a_tool_that_never_finishes_is_sent_after_120_seconds() {
    let mut rig = chat_then_pause("/say done");
    rig.automation.ub_activity = UbActivity {
        busy_tool: Some("autopack"),
        lines_run: 0,
    };

    run_for(&mut rig, 119.0);
    assert!(rig.automation.submitted_chat.is_empty());
    run_for(&mut rig, 1.5);
    assert_eq!(rig.automation.submitted_chat, ["/say done"]);
    assert_eq!(rig.index(), 1);
    assert_eq!(
        rig.automation.posted_system_messages,
        ["[MossTank] UB autopack still running after 120 s, continuing."]
    );
}

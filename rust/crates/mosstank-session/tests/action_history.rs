//! The action history, against `ActionHistoryTests`.

mod common;

use std::rc::Rc;

use common::Clock;
use mosstank_session::action_history::{ActionHistory, CAPACITY};

/// 2026-09-26 12:34:56 UTC, local time UTC (`FixedClock`).
fn fixed_clock() -> Clock {
    Clock::at(1_790_426_096)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ActionHistoryTests.cs :: CapturesTimestampAndOnlyConsecutiveChangesWithoutRebuildingOnRead
#[test]
fn captures_timestamp_and_only_consecutive_changes_without_rebuilding_on_read() {
    let clock = fixed_clock();
    let mut history = ActionHistory::new();
    history.record("Scanning for targets", &clock);
    let first = history.lines();
    history.record("Scanning for targets", &clock);
    assert!(Rc::ptr_eq(&first, &history.lines()));
    assert_eq!(
        &["[12:34:56] Scanning for targets".to_owned()][..],
        &*history.lines()
    );
    history.record("Attacking target", &clock);
    history.record("Scanning for targets", &clock);
    assert_eq!(3, history.lines().len());
    assert_eq!(0, history.first_index());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ActionHistoryTests.cs :: TrimmingAndClearKeepAbsolutePositionsAndDoNotInventAnAction
#[test]
fn trimming_and_clear_keep_absolute_positions_and_do_not_invent_an_action() {
    let clock = fixed_clock();
    let mut history = ActionHistory::new();
    for i in 0..CAPACITY + 5 {
        history.record(&format!("Action {i}"), &clock);
    }
    assert_eq!(CAPACITY, history.lines().len());
    assert_eq!(5, history.first_index());
    assert_eq!("[12:34:56] Action 5", history.lines()[0]);
    history.clear();
    assert!(history.lines().is_empty());
    assert_eq!(1005, history.first_index());
    history.record("Action 1004", &clock);
    assert!(history.lines().is_empty());
    history.record("Next action", &clock);
    assert_eq!(
        &["[12:34:56] Next action".to_owned()][..],
        &*history.lines()
    );
    assert_eq!(1005, history.first_index());
}

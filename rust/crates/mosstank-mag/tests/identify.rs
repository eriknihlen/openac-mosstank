//! The paced identify queue: one appraisal every half second, as the
//! looter paces its own (`IDENTIFY_REQUEST_INTERVAL_SECONDS`), yielding to
//! the looter, a busy slot retried, an unknown item dropped.

use ac_plugin_abi::items::ItemStatus;
use mosstank_mag::identify::IdentifyQueue;
use mosstank_mag::testing::FakeMagHost;

#[test]
fn one_request_per_half_second_and_busy_retries() {
    let mut host = FakeMagHost::new();
    host.identify_answers = [ItemStatus::Busy, ItemStatus::Started, ItemStatus::Started].into();
    let mut q = IdentifyQueue::default();
    q.push(7);
    q.push(8);
    q.push(7);
    host.now_ms = 0.0;
    q.tick(&mut host, false); // Busy: 7 stays
    assert_eq!(host.identified, vec![7]);
    host.now_ms = 100.0;
    q.tick(&mut host, false); // too soon
    assert_eq!(host.identified, vec![7]);
    host.now_ms = 600.0;
    q.tick(&mut host, false); // 7 sent
    assert_eq!(host.identified, vec![7, 7]);
    host.now_ms = 1200.0;
    q.tick(&mut host, false); // 8 sent
    assert_eq!(host.identified, vec![7, 7, 8]);
    assert!(q.is_empty());
}

#[test]
fn waits_while_the_looter_identifies() {
    let mut host = FakeMagHost::new();
    let mut q = IdentifyQueue::default();
    q.push(7);
    q.tick(&mut host, true);
    assert!(host.identified.is_empty());
    assert!(q.contains(7));
    q.tick(&mut host, false);
    assert_eq!(host.identified, vec![7]);
}

#[test]
fn an_unknown_item_is_dropped() {
    let mut host = FakeMagHost::new();
    host.identify_answers = [ItemStatus::InvalidItem].into();
    let mut q = IdentifyQueue::default();
    q.push(7);
    q.push(8);
    q.tick(&mut host, false);
    assert!(!q.contains(7));
    assert!(q.contains(8));
    host.now_ms = 500.0;
    q.tick(&mut host, false);
    assert_eq!(host.identified, vec![7, 8]);
    assert!(q.is_empty());
}

#[test]
fn an_id_already_waiting_is_not_queued_twice() {
    let mut q = IdentifyQueue::default();
    q.push(7);
    q.push(7);
    assert_eq!(q.len(), 1);
}

#[test]
fn the_pace_is_499_milliseconds() {
    let mut host = FakeMagHost::new();
    let mut q = IdentifyQueue::default();
    q.push(7);
    q.push(8);
    host.now_ms = 1000.0;
    q.tick(&mut host, false);
    assert_eq!(host.identified, vec![7]);
    host.now_ms = 1498.0;
    q.tick(&mut host, false);
    assert_eq!(host.identified, vec![7], "498 ms after the last: refused");
    host.now_ms = 1499.0;
    q.tick(&mut host, false);
    assert_eq!(host.identified, vec![7, 8], "499 ms after the last: sent");
}

#[test]
fn an_item_asked_for_outside_the_world_stays_first() {
    let mut host = FakeMagHost::new();
    host.identify_answers = [ItemStatus::Unavailable].into();
    let mut q = IdentifyQueue::default();
    q.push(7);
    q.push(8);
    q.tick(&mut host, false);
    assert!(q.contains(7));
    host.now_ms = 500.0;
    q.tick(&mut host, false);
    assert_eq!(host.identified, vec![7, 7]);
    assert!(q.contains(8) && !q.contains(7));
}

#[test]
fn clear_forgets_every_waiting_item_and_the_pace() {
    let mut host = FakeMagHost::new();
    let mut q = IdentifyQueue::default();
    q.push(7);
    q.push(8);
    host.now_ms = 1000.0;
    q.tick(&mut host, false);
    q.clear();
    assert!(q.is_empty());
    q.push(9);
    host.now_ms = 1001.0;
    q.tick(&mut host, false);
    assert_eq!(host.identified, vec![7, 9], "a cleared queue sends at once");
}

#[test]
fn a_removed_object_is_not_asked_for() {
    let mut host = FakeMagHost::new();
    let mut q = IdentifyQueue::default();
    q.push(7);
    q.push(8);
    q.remove(7);
    assert!(!q.contains(7));
    q.tick(&mut host, false);
    assert_eq!(host.identified, vec![8]);
}

#[test]
fn an_urgent_request_goes_before_a_logger_backlog() {
    // A first-run logger queues hundreds of items; a corpse item the
    // identify line waits on is sent next.
    let mut host = FakeMagHost::new();
    let mut q = IdentifyQueue::default();
    for id in 1000..1300 {
        q.push(id);
    }
    q.push_urgent(7);
    assert_eq!(q.len(), 301);
    q.tick(&mut host, false);
    assert_eq!(host.identified, vec![7]);
    host.now_ms = 500.0;
    q.tick(&mut host, false);
    assert_eq!(host.identified, vec![7, 1000], "then the backlog, in order");
}

#[test]
fn urgent_requests_keep_their_own_order() {
    let mut host = FakeMagHost::new();
    let mut q = IdentifyQueue::default();
    q.push(1);
    q.push_urgent(7);
    q.push_urgent(8);
    q.push_urgent(7);
    assert_eq!(q.len(), 3);
    for step in 0..3 {
        host.now_ms = f64::from(step) * 500.0;
        q.tick(&mut host, false);
    }
    assert_eq!(host.identified, vec![7, 8, 1]);
}

#[test]
fn an_urgent_push_of_a_waiting_id_moves_it_forward() {
    let mut host = FakeMagHost::new();
    let mut q = IdentifyQueue::default();
    q.push(1);
    q.push(2);
    q.push(3);
    q.push_urgent(3);
    assert_eq!(q.len(), 3, "still queued once");
    q.tick(&mut host, false);
    assert_eq!(host.identified, vec![3]);
    // A plain push of an urgent id leaves it urgent.
    q.push(4);
    q.push_urgent(4);
    q.push(4);
    host.now_ms = 500.0;
    q.tick(&mut host, false);
    assert_eq!(host.identified, vec![3, 4]);
}

#[test]
fn a_busy_urgent_request_stays_first() {
    let mut host = FakeMagHost::new();
    host.identify_answers = [ItemStatus::Busy].into();
    let mut q = IdentifyQueue::default();
    q.push(1);
    q.push_urgent(7);
    q.tick(&mut host, false);
    host.now_ms = 500.0;
    q.tick(&mut host, false);
    assert_eq!(host.identified, vec![7, 7]);
    q.remove(1);
    assert!(q.is_empty());
}

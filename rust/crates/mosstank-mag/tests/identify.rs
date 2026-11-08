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
    host.now_ms = 100.0;
    q.tick(&mut host, false); // too soon
    host.now_ms = 600.0;
    q.tick(&mut host, false); // 7 sent
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

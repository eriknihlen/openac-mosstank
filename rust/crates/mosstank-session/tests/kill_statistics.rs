//! The kill statistics, against `KillStatisticsTests`. The C# recorder
//! subscribes to the client's chat, logoff and login; here the test hands
//! it each event, and "subscribed" is [`KillStatistics::is_listening`].

mod common;

use common::{Clock, Store};
use mosstank_session::clock::json_utc;
use mosstank_session::kill_statistics::{
    ENABLE_KEY, KillCharacter, KillStatistics, KillStatisticsHost, SESSIONS_PREFIX,
};
use mosstank_stores::storage::PluginStorage;
use openac_plugin_compat::json::{JsonValue, parse};

/// The 36 killing-blow sentences (`CombatResultTextTests.AuthenticKillLines`).
const AUTHENTIC_KILL_LINES: [&str; 36] = [
    "You knock X into next Morningthaw!",
    "You obliterate X!",
    "X is utterly destroyed by your attack!",
    "X catches your attack, with dire consequences!",
    "The deadly force of your attack is so strong that X's ancestors feel it!",
    "You smite X mightily!",
    "You slay X viciously enough to impart death several times over!",
    "You killed X!",
    "X is torn to ribbons by your assault!",
    "You cleave X in twain!",
    "Your killing blow nearly turns X inside-out!",
    "You split X apart!",
    "The thunder of crushing X is followed by the deafening silence of death!",
    "You beat X to a lifeless pulp!",
    "X is shattered by your assault!",
    "You flatten X's body with the force of your assault!",
    "X is fatally punctured!",
    "X's perforated corpse falls before you!",
    "You run X through!",
    "X's death is preceded by a sharp, stabbing pain!",
    "You bring X to a fiery end!",
    "X is incinerated by your assault!",
    "X is reduced to cinders!",
    "X's seared corpse smolders before you!",
    "Your lightning coruscates over X's mortal remains!",
    "Blistered by lightning, X falls!",
    "Electricity tears X apart!",
    "Your assault sends X to an icy death!",
    "Your attack stops X cold!",
    "X suffers a frozen fate!",
    "X's last strength dissolves before you!",
    "X is liquified by your attack!",
    "You reduce X to a sizzling, oozing mass!",
    "You reduce X to a drained, twisted corpse!",
    "X is dessicated by your attack!",
    "X's last strength withers before you!",
];

/// The test's client (`Host` with its `Store` and `Surface`).
#[derive(Debug)]
struct Host {
    store: Store,
    errors: Vec<String>,
    in_world: bool,
    name: String,
    world: String,
    object_id: u32,
    character_reads: usize,
    ids: u32,
}

impl Host {
    fn new() -> Self {
        Self {
            store: Store::default(),
            errors: Vec::new(),
            in_world: true,
            name: "First".to_owned(),
            world: "Coldeve".to_owned(),
            object_id: 1,
            character_reads: 0,
            ids: 0,
        }
    }

    /// Recording switched on (`Enabled`).
    fn enabled() -> Self {
        let mut host = Self::new();
        host.store
            .values
            .insert(ENABLE_KEY.to_owned(), "true".to_owned());
        host
    }

    fn snapshots(&self) -> Vec<Snap> {
        self.store
            .values
            .iter()
            .filter(|(key, _)| key.starts_with(SESSIONS_PREFIX))
            .map(|(_, text)| Snap::read(text))
            .collect()
    }
}

impl KillStatisticsHost for Host {
    fn storage(&mut self) -> &mut dyn PluginStorage {
        &mut self.store
    }

    fn is_available(&mut self) -> bool {
        true
    }

    fn character(&mut self) -> KillCharacter {
        self.character_reads += 1;
        KillCharacter {
            in_world: self.in_world,
            object_id: self.object_id,
            name: self.name.clone(),
            world: self.world.clone(),
        }
    }

    fn log_error(&mut self, message: &str) {
        self.errors.push(message.to_owned());
    }

    fn new_session_id(&mut self) -> String {
        self.ids += 1;
        format!("{:032x}", self.ids)
    }
}

/// A snapshot read back from its file.
#[derive(Debug)]
struct Snap {
    session_id: String,
    character: String,
    world: String,
    ended_utc: Option<String>,
    covered_seconds: f64,
    total_kills: i64,
    closed: bool,
    last_sequence: u64,
}

impl Snap {
    fn read(text: &str) -> Self {
        let document = parse(text).unwrap();
        let text_of = |name: &str| document.get(name).unwrap().as_str().unwrap().to_owned();
        Self {
            session_id: text_of("SessionId"),
            character: text_of("Character"),
            world: text_of("World"),
            ended_utc: match document.get("EndedUtc").unwrap() {
                JsonValue::Null => None,
                value => Some(value.as_str().unwrap().to_owned()),
            },
            covered_seconds: document.get("CoveredSeconds").unwrap().as_f64().unwrap(),
            total_kills: document.get("TotalKills").unwrap().as_i64().unwrap(),
            closed: document.get("Closed").unwrap().as_bool().unwrap(),
            last_sequence: u64::try_from(document.get("LastSequence").unwrap().as_i64().unwrap())
                .unwrap(),
        }
    }
}

fn send(recorder: &mut KillStatistics, host: &mut Host, clock: &Clock, sequence: u64, text: &str) {
    send_typed(recorder, host, clock, sequence, text, 0);
}

fn send_typed(
    recorder: &mut KillStatistics,
    host: &mut Host,
    clock: &Clock,
    sequence: u64,
    text: &str,
    log_text_type: u32,
) {
    recorder.on_chat(host, clock, sequence, text, log_text_type);
}

fn covered(host: &Host) -> f64 {
    host.snapshots().iter().map(|s| s.covered_seconds).sum()
}

fn kills(host: &Host) -> i64 {
    host.snapshots().iter().map(|s| s.total_kills).sum()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/KillStatisticsTests.cs :: AllPersonalKillSentencesCountIncludingNonselectedAndUnnamedTargets
#[test]
fn all_personal_kill_sentences_count_including_nonselected_and_unnamed_targets() {
    let mut host = Host::enabled();
    let clock = Clock::default();
    let mut recorder = KillStatistics::new(&mut host, &clock);
    let mut sequence = 0;
    for line in AUTHENTIC_KILL_LINES {
        sequence += 1;
        send(&mut recorder, &mut host, &clock, sequence, line);
    }
    recorder.dispose(&mut host, &clock);
    let snapshots = host.snapshots();
    assert_eq!(1, snapshots.len());
    assert_eq!(36, snapshots[0].total_kills);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/KillStatisticsTests.cs :: DisabledDoesNotSubscribeReadWorldOrWrite
#[test]
fn disabled_does_not_subscribe_read_world_or_write() {
    let mut host = Host::new();
    let clock = Clock::default();
    let mut recorder = KillStatistics::new(&mut host, &clock);
    recorder.tick(&mut host, &clock);
    assert!(!recorder.is_listening());
    recorder.dispose(&mut host, &clock);
    assert_eq!(0, host.store.writes);
    assert_eq!(0, host.character_reads);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/KillStatisticsTests.cs :: LogoffClosesBeforeTeardownAndSameIdentityReconnectNeedsNoTick
#[test]
fn logoff_closes_before_teardown_and_same_identity_reconnect_needs_no_tick() {
    let mut host = Host::enabled();
    let clock = Clock::default();
    let mut recorder = KillStatistics::new(&mut host, &clock);
    send(&mut recorder, &mut host, &clock, 1, "You killed Drudge!");
    clock.advance(10);
    recorder.on_logoff(&mut host, &clock);
    let snapshots = host.snapshots();
    assert_eq!(1, snapshots.len());
    assert!(snapshots[0].closed);
    clock.advance(100);
    // Teardown still exposes the old character and may deliver chat/ticks.
    send(&mut recorder, &mut host, &clock, 2, "You killed Drudge!");
    assert_eq!(1, host.snapshots().len());
    recorder.on_login_complete(&mut host, &clock);
    send(&mut recorder, &mut host, &clock, 3, "You killed Drudge!");
    clock.advance(20);
    recorder.dispose(&mut host, &clock);
    assert_eq!(2, host.snapshots().len());
    assert_eq!(30.0, covered(&host));
    assert_eq!(2, kills(&host));
    assert!(!recorder.is_listening());
    recorder.on_login_complete(&mut host, &clock);
    recorder.on_logoff(&mut host, &clock);
    assert_eq!(2, host.snapshots().len());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/KillStatisticsTests.cs :: ObservedWorldTransitionCanReopenAfterLogoffWithoutLoginEvent
#[test]
fn observed_world_transition_can_reopen_after_logoff_without_login_event() {
    let mut host = Host::enabled();
    let clock = Clock::default();
    let mut recorder = KillStatistics::new(&mut host, &clock);
    clock.advance(10);
    recorder.on_logoff(&mut host, &clock);
    recorder.tick(&mut host, &clock);
    assert_eq!(1, host.snapshots().len());
    host.in_world = false;
    recorder.tick(&mut host, &clock);
    clock.advance(100);
    host.in_world = true;
    recorder.tick(&mut host, &clock);
    clock.advance(20);
    recorder.dispose(&mut host, &clock);
    assert_eq!(2, host.snapshots().len());
    assert_eq!(30.0, covered(&host));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/KillStatisticsTests.cs :: OutsideWorldHasNoCoverageOrKillsAndElapsedTimeIgnoresWallClockChanges
#[test]
fn outside_world_has_no_coverage_or_kills_and_elapsed_time_ignores_wall_clock_changes() {
    let mut host = Host::enabled();
    let clock = Clock::default();
    host.in_world = false;
    let mut recorder = KillStatistics::new(&mut host, &clock);
    clock.advance(100);
    send(&mut recorder, &mut host, &clock, 1, "You killed Drudge!");
    recorder.tick(&mut host, &clock);
    assert!(host.snapshots().is_empty());
    host.in_world = true;
    recorder.tick(&mut host, &clock);
    // Old delivery stays rejected on login.
    send(&mut recorder, &mut host, &clock, 1, "You killed Drudge!");
    clock.advance(60);
    clock.utc_offset.set(-500);
    recorder.tick(&mut host, &clock);
    let saved = host.snapshots();
    assert_eq!(1, saved.len());
    assert_eq!(60.0, saved[0].covered_seconds);
    assert_eq!(0, saved[0].total_kills);
    drop(recorder);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/KillStatisticsTests.cs :: InitialStorageFailureDisablesRecordingAndReportsIt
#[test]
fn initial_storage_failure_disables_recording_and_reports_it() {
    let mut host = Host::enabled();
    host.store.fail = Some("Test failure".to_owned());
    let clock = Clock::default();
    let mut recorder = KillStatistics::new(&mut host, &clock);
    recorder.tick(&mut host, &clock);
    send(&mut recorder, &mut host, &clock, 1, "You killed Drudge!");
    recorder.dispose(&mut host, &clock);
    assert_eq!(1, host.errors.len());
    assert!(host.snapshots().is_empty());
    assert!(!recorder.is_listening());
    assert_eq!(1, host.store.writes);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/KillStatisticsTests.cs :: UnreadableEnableFlagReportsFailureWithoutSubscribing
#[test]
fn unreadable_enable_flag_reports_failure_without_subscribing() {
    let mut host = Host::enabled();
    host.store.fail_read = true;
    let clock = Clock::default();
    let mut recorder = KillStatistics::new(&mut host, &clock);
    recorder.tick(&mut host, &clock);
    recorder.dispose(&mut host, &clock);
    assert_eq!(1, host.errors.len());
    assert!(!recorder.is_listening());
    assert_eq!(0, host.store.writes);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/KillStatisticsTests.cs :: DeliveredPersonalKillsDeduplicateAndFlushOncePerMinute
#[test]
fn delivered_personal_kills_deduplicate_and_flush_once_per_minute() {
    let mut host = Host::enabled();
    let clock = Clock::default();
    let mut recorder = KillStatistics::new(&mut host, &clock);
    send(&mut recorder, &mut host, &clock, 1, "You killed Drudge!");
    send(&mut recorder, &mut host, &clock, 1, "You killed Drudge!");
    send_typed(&mut recorder, &mut host, &clock, 2, "You killed Drudge!", 7);
    send(&mut recorder, &mut host, &clock, 3, "A nearby Drudge died.");
    send(
        &mut recorder,
        &mut host,
        &clock,
        4,
        "You cleave Drudge in twain!",
    );
    clock.advance(59);
    recorder.tick(&mut host, &clock);
    assert_eq!(1, host.store.writes);
    clock.advance(1);
    recorder.tick(&mut host, &clock);
    let snapshots = host.snapshots();
    assert_eq!(1, snapshots.len());
    assert_eq!(2, snapshots[0].total_kills);
    assert_eq!(60.0, snapshots[0].covered_seconds);
    assert_eq!(4, snapshots[0].last_sequence);
    assert!(!snapshots[0].closed);
    assert_eq!(2, host.store.writes);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/KillStatisticsTests.cs :: LogoutAndCharacterOrWorldChangesCreateSeparateSegments
#[test]
fn logout_and_character_or_world_changes_create_separate_segments() {
    let mut host = Host::enabled();
    let clock = Clock::default();
    let mut recorder = KillStatistics::new(&mut host, &clock);
    send(&mut recorder, &mut host, &clock, 1, "You killed Drudge!");
    clock.advance(10);
    host.in_world = false;
    recorder.tick(&mut host, &clock);
    clock.advance(100);
    host.in_world = true;
    recorder.tick(&mut host, &clock);
    clock.advance(20);
    host.name = "Second".to_owned();
    host.object_id = 2;
    recorder.tick(&mut host, &clock);
    clock.advance(30);
    host.world = "Other world".to_owned();
    recorder.tick(&mut host, &clock);
    recorder.dispose(&mut host, &clock);
    let segments = host.snapshots();
    assert_eq!(4, segments.len());
    assert!(segments.iter().all(|s| s.closed));
    assert_eq!(60.0, covered(&host));
    assert_eq!(1, kills(&host));
    let mut ids: Vec<&str> = segments.iter().map(|s| s.session_id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(4, ids.len());
    assert!(
        segments
            .iter()
            .any(|s| s.character == "Second" && s.world == "Other world")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/KillStatisticsTests.cs :: RestartLeavesPreviousSnapshotAndNeverCountsOfflineTime
#[test]
fn restart_leaves_previous_snapshot_and_never_counts_offline_time() {
    let mut host = Host::enabled();
    let clock = Clock::default();
    {
        let mut first = KillStatistics::new(&mut host, &clock);
        clock.advance(20);
        send(&mut first, &mut host, &clock, 10, "You killed Drudge!");
        first.dispose(&mut host, &clock);
    }
    clock.advance(500);
    let mut second = KillStatistics::new(&mut host, &clock);
    clock.advance(40);
    send(&mut second, &mut host, &clock, 1, "You killed Drudge!");
    second.dispose(&mut host, &clock);
    assert_eq!(2, host.snapshots().len());
    assert_eq!(60.0, covered(&host));
    assert_eq!(2, kills(&host));
    assert!(!second.is_listening());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/KillStatisticsTests.cs :: DisposeFlushesOnceAndUnsubscribes
#[test]
fn dispose_flushes_once_and_unsubscribes() {
    let mut host = Host::enabled();
    let clock = Clock::default();
    let mut recorder = KillStatistics::new(&mut host, &clock);
    send(&mut recorder, &mut host, &clock, 1, "You killed Drudge!");
    clock.advance(7);
    recorder.dispose(&mut host, &clock);
    recorder.dispose(&mut host, &clock);
    recorder.tick(&mut host, &clock);
    send(&mut recorder, &mut host, &clock, 2, "You killed Drudge!");
    let snapshots = host.snapshots();
    assert_eq!(1, snapshots.len());
    assert_eq!(1, snapshots[0].total_kills);
    assert!(snapshots[0].closed);
    assert_eq!(
        Some(json_utc(clock.utc_seconds() * 10_000_000)),
        snapshots[0].ended_utc
    );
    assert_eq!(7.0, snapshots[0].covered_seconds);
    assert_eq!(2, host.store.writes);
    assert!(!recorder.is_listening());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/KillStatisticsTests.cs :: StorageFailureIsReportedAndRecordingStopsWithoutRetryOrFalseZero
#[test]
fn storage_failure_is_reported_and_recording_stops_without_retry_or_false_zero() {
    let mut host = Host::enabled();
    let clock = Clock::default();
    let mut recorder = KillStatistics::new(&mut host, &clock);
    send(&mut recorder, &mut host, &clock, 1, "You killed Drudge!");
    host.store.fail = Some("Test failure".to_owned());
    clock.advance(60);
    recorder.tick(&mut host, &clock);
    clock.advance(60);
    recorder.tick(&mut host, &clock);
    recorder.dispose(&mut host, &clock);
    assert_eq!(1, host.errors.len());
    assert!(!recorder.is_listening());
    assert_eq!(2, host.store.writes);
    let saved = host.snapshots();
    assert_eq!(1, saved.len());
    assert!(!saved[0].closed);
    assert_eq!(0.0, saved[0].covered_seconds);
}

/// The snapshot's file is the record as System.Text.Json writes it.
#[test]
fn a_snapshot_is_written_as_the_csharp_record() {
    let mut host = Host::enabled();
    let clock = Clock::at(1_700_000_000);
    let mut recorder = KillStatistics::new(&mut host, &clock);
    clock.advance(30);
    send(&mut recorder, &mut host, &clock, 5, "You killed Drudge!");
    recorder.dispose(&mut host, &clock);
    let text = host
        .store
        .values
        .values()
        .find(|v| v.contains("Schema"))
        .unwrap();
    assert_eq!(
        text,
        "{\"SchemaVersion\":1,\"SessionId\":\"00000000000000000000000000000001\",\
         \"Character\":\"First\",\"World\":\"Coldeve\",\"ObjectId\":1,\
         \"StartedUtc\":\"2023-11-14T22:13:20+00:00\",\"UpdatedUtc\":\"2023-11-14T22:13:50+00:00\",\
         \"EndedUtc\":\"2023-11-14T22:13:50+00:00\",\"CoveredSeconds\":30,\"TotalKills\":1,\
         \"Closed\":true,\"LastSequence\":5}"
    );
}

//! The game-database update against a transport that answers from the
//! test (`VtankGameInfoUpdaterTests`), and the HTTP request and the reading
//! of the client's answers (`HttpVtankGameInfoTransportTests`).
//!
//! The C# check runs on a task the test awaits; here the request goes out
//! on [`VtankGameInfoUpdater::start`] and the test hands the answer back
//! with [`VtankGameInfoUpdater::receive`], as the shell does when the
//! client's reply arrives.

mod common;

use ac_plugin_abi::net::{HttpReply, NetError};
use common::Store;
use mosstank_formats::fixtures;
use mosstank_formats::gameinfo::{self, VtankGameInfoDatabase};
use mosstank_formats::usd::{VtankCell, VtankDatabase, VtankRow};
use mosstank_session::game_info::{
    GameInfoTransport, HttpGameInfoTransport, LAST_CHECK_KEY, SOURCE_ADDRESS, VtankGameInfoAnswer,
    VtankGameInfoUpdater, read_last_check,
};
use mosstank_session::testing::download;

/// The fixture's own time: 2023-11-14 22:13:20 UTC.
const EXCERPT_TIME: i32 = 1_700_000_000;
const ONE_DAY: i32 = 86_400;

fn excerpt_text() -> String {
    fixtures::read_vtank("gameinfodb-excerpt.ugd")
}

/// Answers every request with the next handle, and records the addresses.
#[derive(Debug, Default)]
struct FakeTransport {
    requests: Vec<String>,
    /// Refuse to send, with this reason.
    refuse: Option<String>,
}

impl GameInfoTransport for FakeTransport {
    fn get(&mut self, address: &str) -> Result<u32, String> {
        self.requests.push(address.to_owned());
        match &self.refuse {
            Some(reason) => Err(reason.clone()),
            None => Ok(u32::try_from(self.requests.len()).unwrap()),
        }
    }
}

fn profiles_with_excerpt() -> Store {
    let mut profiles = Store::default();
    profiles
        .values
        .insert(gameinfo::FILE_NAME.to_owned(), excerpt_text());
    profiles
}

/// One check: started, answered with `answer` if it asked, and drained
/// (`RunAsync`): the lines said and the databases handed over.
fn run(
    profiles: &mut Store,
    state: &mut Store,
    transport: &mut FakeTransport,
    answer: &VtankGameInfoAnswer,
    now: i64,
    window: i64,
) -> (Vec<String>, Vec<VtankGameInfoDatabase>) {
    let mut updater = VtankGameInfoUpdater::new(true);
    assert_eq!(
        None,
        updater.start(profiles, state, Some(transport), now, window)
    );
    if let Some(handle) = updater.waiting_handle() {
        assert!(updater.receive(handle, answer, profiles, state, now));
    }
    let mut said = Vec::new();
    let mut applied = Vec::new();
    if let Some(result) = updater.drain() {
        if let Some(database) = result.database {
            applied.push(database);
        }
        said.push(result.message);
    }
    updater.dispose();
    (said, applied)
}

fn now_default() -> i64 {
    1_790_000_000
}

fn last(said: &[String]) -> &str {
    said.last().expect("a line")
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankGameInfoUpdaterTests.cs :: ANewerDatabaseReplacesTheFileAndIsHandedOver
#[test]
fn a_newer_database_replaces_the_file_and_is_handed_over() {
    let mut profiles = profiles_with_excerpt();
    let mut state = Store::default();
    let answer = download(EXCERPT_TIME + ONE_DAY, None, &["Downloaded Arrow"]);
    let mut transport = FakeTransport::default();
    let now = i64::from(EXCERPT_TIME + 2 * ONE_DAY);

    let (said, applied) = run(&mut profiles, &mut state, &mut transport, &answer, now, 0);

    assert_eq!(vec![SOURCE_ADDRESS.to_owned()], transport.requests);
    assert_eq!(
        "Game database updated to 2023-11-15 from openac-gamedata.",
        last(&said)
    );
    assert_eq!(Some(&answer.text), profiles.values.get(gameinfo::FILE_NAME));
    assert_eq!(1, applied.len());
    assert_eq!(1, applied[0].ammunition_options.len());
    assert_eq!("Downloaded Arrow", applied[0].ammunition_options[0].name);
    assert_eq!(Ok(Some(now)), read_last_check(&mut state));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankGameInfoUpdaterTests.cs :: AForeignDatabaseWithALaterTimeIsReplaced
#[test]
fn a_foreign_database_with_a_later_time_is_replaced() {
    let mut profiles = profiles_with_excerpt();
    let answer = download(EXCERPT_TIME - ONE_DAY, None, &["Downloaded Arrow"]);

    let (said, applied) = run(
        &mut profiles,
        &mut Store::default(),
        &mut FakeTransport::default(),
        &answer,
        now_default(),
        0,
    );

    assert_eq!(
        "Game database replaced with openac-gamedata's (2023-11-13).",
        last(&said)
    );
    assert_eq!(Some(&answer.text), profiles.values.get(gameinfo::FILE_NAME));
    assert_eq!(1, applied.len());
    assert_eq!("Downloaded Arrow", applied[0].ammunition_options[0].name);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankGameInfoUpdaterTests.cs :: ADatabaseWithTheFilesTimeIsUpToDate
#[test]
fn a_database_with_the_files_time_is_up_to_date() {
    let mut profiles = profiles_with_excerpt();
    let mut state = Store::default();
    let answer = download(EXCERPT_TIME, None, &["Downloaded Arrow"]);

    let (said, applied) = run(
        &mut profiles,
        &mut state,
        &mut FakeTransport::default(),
        &answer,
        now_default(),
        0,
    );

    assert_eq!(
        "Game database is up to date (2023-11-14, openac-gamedata).",
        last(&said)
    );
    assert!(applied.is_empty());
    assert_eq!(0, profiles.writes);
    assert!(read_last_check(&mut state).unwrap().is_some());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankGameInfoUpdaterTests.cs :: WithNoUsableFileAnyGoodDownloadIsTaken
#[test]
fn with_no_usable_file_any_good_download_is_taken() {
    let none = Store::default();
    let mut old_version = VtankDatabase::parse(&excerpt_text()).unwrap();
    old_version.find_mut("DBVersion").unwrap().rows[0].cells[0] = VtankCell::int(4);
    old_version.find_mut("DBLastUpdateTime").unwrap().rows[0].cells[1] =
        VtankCell::int(EXCERPT_TIME + 9 * ONE_DAY);
    let mut other = Store::default();
    other
        .values
        .insert(gameinfo::FILE_NAME.to_owned(), old_version.render());

    for mut profiles in [none, other] {
        let (said, _) = run(
            &mut profiles,
            &mut Store::default(),
            &mut FakeTransport::default(),
            &download(EXCERPT_TIME, None, &[]),
            now_default(),
            0,
        );

        assert_eq!(
            "Game database updated to 2023-11-14 from openac-gamedata.",
            last(&said)
        );
        assert_eq!(1, profiles.writes);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankGameInfoUpdaterTests.cs :: ADownloadOfAnotherVersionIsRefused
#[test]
fn a_download_of_another_version_is_refused() {
    let mut profiles = Store::default();
    let mut state = Store::default();
    let change = |database: &mut VtankDatabase| {
        database.find_mut("DBVersion").unwrap().rows[0].cells[0] = VtankCell::int(10);
    };
    let answer = download(EXCERPT_TIME, Some(&change), &[]);

    let (said, applied) = run(
        &mut profiles,
        &mut state,
        &mut FakeTransport::default(),
        &answer,
        now_default(),
        0,
    );

    assert_eq!(
        "Game database update failed: the download is database version 10, not 9. \
         The game database you had is kept.",
        last(&said)
    );
    assert!(applied.is_empty());
    assert_eq!(0, profiles.writes);
    assert_eq!(Ok(None), read_last_check(&mut state));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankGameInfoUpdaterTests.cs :: ADownloadWithAValueThatDoesNotReadIsRefused
#[test]
fn a_download_with_a_value_that_does_not_read_is_refused() {
    let mut profiles = Store::default();
    let change = |database: &mut VtankDatabase| {
        let mut lots = VtankCell::tagged("d");
        lots.scalar_text = Some("lots".to_owned());
        database.find_mut("HealKits").unwrap().rows.push(VtankRow {
            cells: vec![
                VtankCell::string("Broken Kit"),
                lots,
                VtankCell::int(0),
                VtankCell::int(1),
            ],
        });
    };
    let answer = download(EXCERPT_TIME, Some(&change), &[]);

    let (said, _) = run(
        &mut profiles,
        &mut Store::default(),
        &mut FakeTransport::default(),
        &answer,
        now_default(),
        0,
    );

    assert_eq!(
        "Game database update failed: the download is not a game database \
         (a value in it does not read). The game database you had is kept.",
        last(&said)
    );
    assert_eq!(0, profiles.writes);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankGameInfoUpdaterTests.cs :: ADownloadThatIsNotADatedDatabaseIsRefused
#[test]
fn a_download_that_is_not_a_dated_database_is_refused() {
    let mut profiles = profiles_with_excerpt();
    let change = |database: &mut VtankDatabase| {
        database.find_mut("DBLastUpdateTime").unwrap().rows.clear();
    };
    let undated = download(EXCERPT_TIME, Some(&change), &[]);

    let (page, _) = run(
        &mut profiles,
        &mut Store::default(),
        &mut FakeTransport::default(),
        &VtankGameInfoAnswer::new(true, "<html>moved</html>"),
        now_default(),
        0,
    );
    let (no_time, _) = run(
        &mut profiles,
        &mut Store::default(),
        &mut FakeTransport::default(),
        &undated,
        now_default(),
        0,
    );

    assert!(
        last(&page)
            .starts_with("Game database update failed: the download is not a game database ("),
        "{page:?}"
    );
    assert_eq!(
        "Game database update failed: the download does not say which world data \
         it was built from. The game database you had is kept.",
        last(&no_time)
    );
    assert_eq!(0, profiles.writes);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankGameInfoUpdaterTests.cs :: AFailedDownloadSaysWhyAndNotesNoCheck
#[test]
fn a_failed_download_says_why_and_notes_no_check() {
    let mut profiles = profiles_with_excerpt();
    let mut state = Store::default();

    let (said, applied) = run(
        &mut profiles,
        &mut state,
        &mut FakeTransport::default(),
        &VtankGameInfoAnswer::new(false, "HTTP 503 Service Unavailable"),
        now_default(),
        0,
    );

    assert_eq!(
        "Game database update failed: HTTP 503 Service Unavailable. The game database you had is kept.",
        last(&said)
    );
    assert!(applied.is_empty());
    assert_eq!(0, profiles.writes);
    assert_eq!(Ok(None), read_last_check(&mut state));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankGameInfoUpdaterTests.cs :: ADownloadThatCannotBeSavedIsStillUsed
#[test]
fn a_download_that_cannot_be_saved_is_still_used() {
    let mut profiles = profiles_with_excerpt();
    profiles.fail = Some("The disk is full.".to_owned());

    let (said, applied) = run(
        &mut profiles,
        &mut Store::default(),
        &mut FakeTransport::default(),
        &download(EXCERPT_TIME + ONE_DAY, None, &["Downloaded Arrow"]),
        now_default(),
        0,
    );

    assert_eq!(
        "Game database updated to 2023-11-15 from openac-gamedata but could not be saved: \
         The disk is full. It is used until MossTank stops.",
        last(&said)
    );
    assert_eq!(1, applied.len());
    assert_eq!(
        Some(&excerpt_text()),
        profiles.values.get(gameinfo::FILE_NAME)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankGameInfoUpdaterTests.cs :: ARecentCheckIsNotRepeated
#[test]
fn a_recent_check_is_not_repeated() {
    let mut profiles = profiles_with_excerpt();
    let mut state = Store::default();
    let checked_at = i64::from(EXCERPT_TIME + ONE_DAY);
    state
        .values
        .insert(LAST_CHECK_KEY.to_owned(), checked_at.to_string());
    let mut transport = FakeTransport::default();
    let window = 6 * 3600;
    let answer = download(EXCERPT_TIME, None, &[]);

    let (said, applied) = run(
        &mut profiles,
        &mut state,
        &mut transport,
        &answer,
        checked_at + 3600,
        window,
    );

    assert!(transport.requests.is_empty());
    assert_eq!(
        "Game database checked recently; the next check is after 2023-11-16 04:13 UTC.",
        last(&said)
    );
    assert_eq!(1, applied.len());
    assert_eq!(7, applied[0].ammunition_options.len());

    run(
        &mut profiles,
        &mut state,
        &mut transport,
        &answer,
        checked_at + 7 * 3600,
        window,
    );
    run(
        &mut profiles,
        &mut state,
        &mut transport,
        &answer,
        checked_at + 60,
        0,
    );
    assert_eq!(2, transport.requests.len());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankGameInfoUpdaterTests.cs :: OnlyOneCheckRunsAtATime
#[test]
fn only_one_check_runs_at_a_time() {
    let mut profiles = Store::default();
    let mut state = Store::default();
    let mut transport = FakeTransport::default();
    let mut updater = VtankGameInfoUpdater::new(true);
    let now = now_default();

    assert_eq!(
        None,
        updater.start(&mut profiles, &mut state, Some(&mut transport), now, 0)
    );
    assert_eq!(1, transport.requests.len());
    assert_eq!(
        Some("A game database update is already running.".to_owned()),
        updater.start(&mut profiles, &mut state, Some(&mut transport), now, 0)
    );
    assert_eq!(None, updater.drain());

    let handle = updater.waiting_handle().unwrap();
    assert!(updater.receive(
        handle,
        &download(EXCERPT_TIME, None, &[]),
        &mut profiles,
        &mut state,
        now
    ));
    let said = updater.drain().unwrap().message;

    assert_eq!(1, transport.requests.len());
    assert_eq!(
        "Game database updated to 2023-11-14 from openac-gamedata.",
        said
    );
    assert!(!updater.is_running());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankGameInfoUpdaterTests.cs :: ACheckNeedsASourceAndAFolder
#[test]
fn a_check_needs_a_source_and_a_folder() {
    let mut no_source = VtankGameInfoUpdater::new(false);
    let mut no_folder = VtankGameInfoUpdater::new(true);
    let mut folder = Store::default();
    let mut missing = Store {
        unavailable: true,
        ..Store::default()
    };
    let mut transport = FakeTransport::default();

    assert!(!no_source.can_update(&mut folder));
    assert!(!no_folder.can_update(&mut missing));
    assert_eq!(
        Some("Game database updates are not available in this session.".to_owned()),
        no_source.start(&mut folder, &mut Store::default(), None, 0, 0)
    );
    assert_eq!(
        Some(
            "Game database updates need a VTank profile folder to keep the database in.".to_owned()
        ),
        no_folder.start(
            &mut missing,
            &mut Store::default(),
            Some(&mut transport),
            0,
            0
        )
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankGameInfoUpdaterTests.cs :: ATransportThatThrowsSaysWhy
#[test]
fn a_transport_that_throws_says_why() {
    let mut transport = FakeTransport {
        refuse: Some("No route.".to_owned()),
        ..FakeTransport::default()
    };
    let mut updater = VtankGameInfoUpdater::new(true);

    assert_eq!(
        None,
        updater.start(
            &mut Store::default(),
            &mut Store::default(),
            Some(&mut transport),
            now_default(),
            0
        )
    );
    let said = updater.drain().map(|result| result.message);

    assert_eq!(
        Some(
            "Game database update failed: No route. The game database you had is kept.".to_owned()
        ),
        said
    );
    assert!(!updater.is_running());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankGameInfoUpdaterTests.cs :: DisposeDuringACheckWritesNothing
#[test]
fn dispose_during_a_check_writes_nothing() {
    let mut profiles = profiles_with_excerpt();
    let mut state = Store::default();
    let mut transport = FakeTransport::default();
    let mut updater = VtankGameInfoUpdater::new(true);
    assert_eq!(
        None,
        updater.start(
            &mut profiles,
            &mut state,
            Some(&mut transport),
            now_default(),
            0
        )
    );
    let handle = updater.waiting_handle().unwrap();
    assert_eq!(1, transport.requests.len());

    assert_eq!(Some(handle), updater.dispose());
    let taken = updater.receive(
        handle,
        &download(EXCERPT_TIME + ONE_DAY, None, &["Downloaded Arrow"]),
        &mut profiles,
        &mut state,
        now_default(),
    );

    assert!(!taken);
    assert_eq!(0, profiles.writes);
    assert!(!updater.is_running());
}

// ---- HttpVtankGameInfoTransportTests ----------------------------------------

fn reply(status: u32, reason: &str, body: &str) -> HttpReply {
    HttpReply {
        handle: 1,
        status,
        reason: reason.to_owned(),
        headers: Vec::new(),
        body: body.as_bytes().to_vec(),
        error: NetError::None,
        message: String::new(),
    }
}

fn failure(error: NetError, message: &str) -> HttpReply {
    HttpReply {
        error,
        message: message.to_owned(),
        ..reply(0, "", "")
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/HttpVtankGameInfoTransportTests.cs :: AnAnswerIsReadWhole
#[test]
fn an_answer_is_read_whole() {
    let transport = HttpGameInfoTransport::default();
    let request = transport.request(SOURCE_ADDRESS);
    assert_eq!("GET", request.method);
    assert_eq!(SOURCE_ADDRESS, request.url);
    assert!(request.follow_redirects);

    let answer = transport.answer(&reply(200, "OK", "1\r\nDBVersion\r\n"));

    assert_eq!(VtankGameInfoAnswer::new(true, "1\r\nDBVersion\r\n"), answer);
}

/// Mutation: accept any status, and the error page reads as an answer.
// Source: tests/AcDream.Plugins.MossTank.Tests/HttpVtankGameInfoTransportTests.cs :: AnErrorStatusIsAReason
#[test]
fn an_error_status_is_a_reason() {
    let transport = HttpGameInfoTransport::default();

    let answer = transport.answer(&reply(503, "Service Unavailable", "<html>down</html>"));

    assert_eq!(
        VtankGameInfoAnswer::new(false, "HTTP 503 Service Unavailable"),
        answer
    );
}

/// The client could not make the request: its reason comes back.
// Source: tests/AcDream.Plugins.MossTank.Tests/HttpVtankGameInfoTransportTests.cs :: AFailedRequestIsAReason
#[test]
fn a_failed_request_is_a_reason() {
    let transport = HttpGameInfoTransport::default();

    let answer = transport.answer(&failure(NetError::Connect, "No such host is known."));

    assert_eq!(
        VtankGameInfoAnswer::new(false, "No such host is known."),
        answer
    );
}

/// The request carries the timeout, and a service that does not answer in
/// time (the client gives up) is a reason.
// Source: tests/AcDream.Plugins.MossTank.Tests/HttpVtankGameInfoTransportTests.cs :: ASilentServiceTimesOut
#[test]
fn a_silent_service_times_out() {
    let transport = HttpGameInfoTransport {
        timeout_ms: 50,
        ..HttpGameInfoTransport::default()
    };
    assert_eq!(50, transport.request(SOURCE_ADDRESS).timeout_ms);

    let answer = transport.answer(&failure(NetError::Timeout, "timed out"));

    assert_eq!(
        VtankGameInfoAnswer::new(false, "no answer within 0.05 seconds"),
        answer
    );
}

/// The request carries the cap, and an answer larger than it (the client
/// refuses it) is not taken.
// Source: tests/AcDream.Plugins.MossTank.Tests/HttpVtankGameInfoTransportTests.cs :: AnOversizedAnswerIsRefused
#[test]
fn an_oversized_answer_is_refused() {
    let transport = HttpGameInfoTransport {
        maximum_answer_bytes: 1024,
        ..HttpGameInfoTransport::default()
    };
    assert_eq!(1024, transport.request(SOURCE_ADDRESS).max_bytes);

    let answer = transport.answer(&failure(
        NetError::TooLarge,
        "The reply was larger than allowed.",
    ));

    assert!(!answer.succeeded);
}

/// The defaults are the C#'s: thirty seconds and 32 MiB.
#[test]
fn the_default_request_waits_thirty_seconds_for_at_most_32_mib() {
    let request = HttpGameInfoTransport::default().request(SOURCE_ADDRESS);
    assert_eq!(30_000, request.timeout_ms);
    assert_eq!(32 * 1024 * 1024, request.max_bytes);
}

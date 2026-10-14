//! Keeps the player's game database current from openac-gamedata
//! (`VtankGameInfoUpdater`), an independent project that publishes a
//! complete database, generated from the ACE server's world data, as a
//! release file; and the HTTP request that fetches it
//! (`HttpVtankGameInfoTransport`).
//!
//! Each check downloads the whole file; one that reads as a whole database
//! at the built-in version, and whose time differs from the database the
//! player has, replaces the player's file: an older release, or a database
//! from anywhere else. A database whose last check is younger than the
//! window is not checked again.
//!
//! The C# runs the check off the game thread and hands the outcome to the
//! tick. Here nothing blocks: [`VtankGameInfoUpdater::start`] sends the
//! request, the client's answer comes back as an event the shell hands to
//! [`VtankGameInfoUpdater::receive`] (which checks and saves it), and
//! [`VtankGameInfoUpdater::drain`] hands the outcome over on the tick.

use std::cell::RefCell;

use ac_plugin_abi::net::{HttpReply, HttpRequest, NetError};
use mosstank_dotnet::text::casing;
use mosstank_formats::gameinfo::{
    self, DownloadRead, DownloadReading, GameInfoMemo, ProfileText, VtankGameInfoDatabase,
};
use mosstank_formats::usd::VtankDatabase;
use mosstank_stores::storage::{PluginStorage, StorageError};
use openac_plugin_compat::datetime::DateTime;
use openac_plugin_compat::number::{self, NumberStyles};
use openac_plugin_compat::text;

/// The newest published database; the link redirects to the file.
pub const SOURCE_ADDRESS: &str =
    "https://github.com/eriknihlen/openac-gamedata/releases/latest/download/gameinfodb.ugd";

/// Where the source is named in chat.
pub const SOURCE_NAME: &str = "openac-gamedata";

/// The plugin's own note of when the database was last checked, in seconds
/// since 1970. It lives beside the plugin's other state, not in the
/// database, whose own time says which world data it was built from.
pub const LAST_CHECK_KEY: &str = "gamedb/last-check.txt";

const KEPT_OLD: &str = " The game database you had is kept.";

/// How long the download has to finish before the check gives up,
/// milliseconds.
pub const DEFAULT_TIMEOUT_MS: u32 = 30_000;

/// The largest answer taken. A whole database is a few megabytes; anything
/// near this is not one.
pub const DEFAULT_MAXIMUM_ANSWER_BYTES: u32 = 32 * 1024 * 1024;

/// What the download answered, or why it did not (`VtankGameInfoAnswer`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VtankGameInfoAnswer {
    /// Whether there is a database text.
    pub succeeded: bool,
    /// The text, or the reason there is none.
    pub text: String,
}

impl VtankGameInfoAnswer {
    /// An answer.
    #[must_use]
    pub fn new(succeeded: bool, text: impl Into<String>) -> Self {
        Self {
            succeeded,
            text: text.into(),
        }
    }
}

/// A plain GET that follows redirects, with a timeout and a size cap; every
/// failure comes back as a reason (`HttpVtankGameInfoTransport`). The
/// client sends it and answers it later; this is the request and the
/// reading of the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpGameInfoTransport {
    /// How long the download has to finish, milliseconds.
    pub timeout_ms: u32,
    /// The largest answer taken, bytes.
    pub maximum_answer_bytes: u32,
}

impl Default for HttpGameInfoTransport {
    fn default() -> Self {
        Self {
            timeout_ms: DEFAULT_TIMEOUT_MS,
            maximum_answer_bytes: DEFAULT_MAXIMUM_ANSWER_BYTES,
        }
    }
}

impl HttpGameInfoTransport {
    /// The request for `address`. The release link answers with a redirect
    /// to the file itself, which is followed.
    #[must_use]
    pub fn request(&self, address: &str) -> HttpRequest {
        HttpRequest {
            method: "GET".to_owned(),
            url: address.to_owned(),
            headers: Vec::new(),
            body: Vec::new(),
            timeout_ms: self.timeout_ms,
            max_bytes: self.maximum_answer_bytes,
            follow_redirects: true,
        }
    }

    /// What the client's answer to [`Self::request`] says.
    #[must_use]
    pub fn answer(&self, reply: &HttpReply) -> VtankGameInfoAnswer {
        match self.answer_body(reply) {
            Ok(body) => VtankGameInfoAnswer::new(true, text::read_all_text(body)),
            Err(answer) => answer,
        }
    }

    /// [`Self::answer`] before the text is decoded: the body of an answer
    /// that has a database text, or the failed answer.
    ///
    /// # Errors
    /// The download failed: the answer that says why.
    pub fn answer_body<'r>(&self, reply: &'r HttpReply) -> Result<&'r [u8], VtankGameInfoAnswer> {
        match reply.error {
            NetError::None => {}
            NetError::Timeout => {
                return Err(VtankGameInfoAnswer::new(
                    false,
                    format!("no answer within {} seconds", seconds_text(self.timeout_ms)),
                ));
            }
            _ => return Err(VtankGameInfoAnswer::new(false, reply.message.clone())),
        }
        if !(200..=299).contains(&reply.status) {
            return Err(VtankGameInfoAnswer::new(
                false,
                format!("HTTP {} {}", reply.status, reply.reason)
                    .trim_end_matches(casing::is_white_space)
                    .to_owned(),
            ));
        }
        Ok(&reply.body)
    }
}

/// Milliseconds as seconds, `ToString("0.##")`.
fn seconds_text(milliseconds: u32) -> String {
    let hundredths = (u64::from(milliseconds) + 5) / 10;
    let whole = hundredths / 100;
    let fraction = hundredths % 100;
    if fraction == 0 {
        return whole.to_string();
    }
    let digits = format!("{fraction:02}");
    format!("{whole}.{}", digits.trim_end_matches('0'))
}

/// What one update check came to: the line to say, and the database to use
/// if it changed (`VtankGameInfoUpdateResult`).
#[derive(Debug, Clone, PartialEq)]
pub struct VtankGameInfoUpdateResult {
    /// The line.
    pub message: String,
    /// The database to hand over, if any.
    pub database: Option<VtankGameInfoDatabase>,
}

/// How the check asks for the file (`IVtankGameInfoTransport`).
pub trait GameInfoTransport {
    /// Sends the request for `address`: its handle, which the answer will
    /// carry.
    ///
    /// # Errors
    /// The request could not be sent; the check ends with the reason.
    fn get(&mut self, address: &str) -> Result<u32, String>;
}

/// A check under way.
#[derive(Debug)]
enum Pending {
    /// The request is out.
    Waiting {
        /// Its handle.
        handle: u32,
        /// The player's database's own time.
        local_time: i32,
    },
    /// The answer is in, worked through on the ticks.
    Received(Box<Received>),
    /// The outcome, waiting for the tick; an error is the check's fault.
    Finished(Box<Result<VtankGameInfoUpdateResult, String>>),
}

/// The update check (`VtankGameInfoUpdater`).
#[derive(Debug, Default)]
pub struct VtankGameInfoUpdater {
    has_transport: bool,
    pending: Option<Pending>,
    disposed: bool,
}

/// Storage read as the database readers read it, keeping the first failure.
struct Reader<'a> {
    storage: RefCell<&'a mut dyn PluginStorage>,
    failure: RefCell<Option<StorageError>>,
}

impl<'a> Reader<'a> {
    fn new(storage: &'a mut dyn PluginStorage) -> Self {
        Self {
            storage: RefCell::new(storage),
            failure: RefCell::new(None),
        }
    }
}

impl ProfileText for Reader<'_> {
    fn is_available(&self) -> bool {
        self.storage.borrow_mut().is_available()
    }

    fn read_text(&self, key: &str) -> Option<String> {
        match self.storage.borrow_mut().read_text(key) {
            Ok(text) => text,
            Err(error) => {
                self.failure.borrow_mut().get_or_insert(error);
                None
            }
        }
    }
}

impl VtankGameInfoUpdater {
    /// An updater; `has_transport` false means this session never
    /// downloads.
    #[must_use]
    pub fn new(has_transport: bool) -> Self {
        Self {
            has_transport,
            pending: None,
            disposed: false,
        }
    }

    /// Can this session download at all: a source to fetch from and a
    /// folder to keep the file in?
    pub fn can_update(&self, profiles: &mut dyn PluginStorage) -> bool {
        self.has_transport && profiles.is_available()
    }

    /// Whether a check is under way or its outcome not yet handed over.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.pending.is_some()
    }

    /// Whether the check under way has its outcome, for a test.
    #[must_use]
    pub fn is_finished_for_test(&self) -> bool {
        matches!(self.pending, Some(Pending::Finished(_)))
    }

    /// The handle of the request the check is waiting on.
    #[must_use]
    pub fn waiting_handle(&self) -> Option<u32> {
        match self.pending {
            Some(Pending::Waiting { handle, .. }) => Some(handle),
            _ => None,
        }
    }

    /// Starts a check unless one is already running or there is nothing to
    /// check with: the reason it did not start, or `None` when it did.
    ///
    /// A database last checked less than `skip_if_checked_within_seconds`
    /// ago is not downloaded again; the check hands over what the file
    /// holds instead. Zero forces the download.
    ///
    /// # Panics
    /// The updater was disposed (`ObjectDisposedException`).
    pub fn start(
        &mut self,
        profiles: &mut dyn PluginStorage,
        state: &mut dyn PluginStorage,
        transport: Option<&mut dyn GameInfoTransport>,
        now_unix_seconds: i64,
        skip_if_checked_within_seconds: i64,
    ) -> Option<String> {
        self.start_with_memo(
            profiles,
            state,
            transport,
            now_unix_seconds,
            skip_if_checked_within_seconds,
            &mut GameInfoMemo::default(),
        )
    }

    /// [`Self::start`], reading the profile folder's database through
    /// `memo` (the session's, so the file the session already read is not
    /// parsed again).
    ///
    /// # Panics
    /// The updater was disposed (`ObjectDisposedException`).
    pub fn start_with_memo(
        &mut self,
        profiles: &mut dyn PluginStorage,
        state: &mut dyn PluginStorage,
        transport: Option<&mut dyn GameInfoTransport>,
        now_unix_seconds: i64,
        skip_if_checked_within_seconds: i64,
        memo: &mut GameInfoMemo,
    ) -> Option<String> {
        assert!(!self.disposed, "the game database updater was disposed");
        let Some(transport) = transport.filter(|_| self.has_transport) else {
            return Some("Game database updates are not available in this session.".to_owned());
        };
        if !profiles.is_available() {
            return Some(
                "Game database updates need a VTank profile folder to keep the database in."
                    .to_owned(),
            );
        }
        if self.pending.is_some() {
            return Some("A game database update is already running.".to_owned());
        }
        self.pending = Some(Self::begin(
            profiles,
            state,
            transport,
            now_unix_seconds,
            skip_if_checked_within_seconds,
            memo,
        ));
        None
    }

    fn begin(
        profiles: &mut dyn PluginStorage,
        state: &mut dyn PluginStorage,
        transport: &mut dyn GameInfoTransport,
        now_unix_seconds: i64,
        skip_if_checked_within_seconds: i64,
        memo: &mut GameInfoMemo,
    ) -> Pending {
        let reader = Reader::new(profiles);
        let text = gameinfo::read_base_text(&reader);
        if let Some(error) = reader.failure.into_inner() {
            return Pending::Finished(Box::new(Err(error)));
        }
        let (local, local_time) = memo.read(text);
        if skip_if_checked_within_seconds > 0 {
            match read_last_check(state) {
                Err(error) => return Pending::Finished(Box::new(Err(error))),
                Ok(Some(last_check)) => {
                    let next_check =
                        match next_check_after(last_check, skip_if_checked_within_seconds) {
                            Ok(next_check) => next_check,
                            Err(error) => return Pending::Finished(Box::new(Err(error))),
                        };
                    if now_unix_seconds < next_check {
                        // Another session may have downloaded since this one
                        // loaded the file, so what the file holds now is
                        // handed over.
                        return Pending::Finished(Box::new(Ok(VtankGameInfoUpdateResult {
                            message: format!(
                                "Game database checked recently; the next check is after {}.",
                                format_utc(next_check)
                            ),
                            database: Some(local),
                        })));
                    }
                }
                Ok(None) => {}
            }
        }
        let local_time = local_time.unwrap_or(0);
        match transport.get(SOURCE_ADDRESS) {
            Ok(handle) => Pending::Waiting { handle, local_time },
            Err(error) => Pending::Finished(Box::new(Err(error))),
        }
    }

    /// The client's answer to the request `handle`: checked, saved if it is
    /// a database to keep, and kept for [`Self::drain`]. An answer to
    /// another request, or one after [`Self::dispose`], is passed over;
    /// whether it was this check's.
    pub fn receive(
        &mut self,
        handle: u32,
        answer: &VtankGameInfoAnswer,
        profiles: &mut dyn PluginStorage,
        state: &mut dyn PluginStorage,
        now_unix_seconds: i64,
    ) -> bool {
        let Some(local_time) = self.waiting_for(handle) else {
            return false;
        };
        let result = if answer.succeeded {
            let mut received = Received {
                local_time,
                now_unix_seconds,
                stage: Stage::Reading(Box::new(DownloadReading::new(answer.text.clone()))),
            };
            loop {
                if let Some(result) = received.step(usize::MAX, profiles, state) {
                    break result;
                }
            }
        } else {
            failed(&answer.text)
        };
        self.pending = Some(Pending::Finished(Box::new(Ok(result))));
        true
    }

    /// [`Self::receive`] of the client's reply as `transport` reads it, the
    /// work spread over the ticks: a failed download ends the check at
    /// once; a database is kept, and decoded, read, checked, built and
    /// saved a stage a tick ([`Self::step`]), so no one plugin call does it
    /// all. Whether it was this check's.
    pub fn receive_reply(
        &mut self,
        reply: &HttpReply,
        transport: &HttpGameInfoTransport,
        now_unix_seconds: i64,
    ) -> bool {
        let Some(local_time) = self.waiting_for(reply.handle) else {
            return false;
        };
        self.pending = Some(match transport.answer_body(reply) {
            Ok(body) => Pending::Received(Box::new(Received {
                local_time,
                now_unix_seconds,
                stage: Stage::Bytes(body.to_vec()),
            })),
            Err(answer) => Pending::Finished(Box::new(Ok(failed(&answer.text)))),
        });
        true
    }

    /// On the tick, before [`Self::drain`]: a received download one stage
    /// on.
    pub fn step(&mut self, profiles: &mut dyn PluginStorage, state: &mut dyn PluginStorage) {
        if let Some(Pending::Received(received)) = self.pending.as_mut()
            && let Some(result) = received.step(WORK_PER_TICK, profiles, state)
        {
            self.pending = Some(Pending::Finished(Box::new(Ok(result))));
        }
    }

    /// The player's database's time, when the check waits on `handle` and
    /// was not disposed.
    fn waiting_for(&self, handle: u32) -> Option<i32> {
        match self.pending {
            Some(Pending::Waiting {
                handle: waiting,
                local_time,
            }) if waiting == handle && !self.disposed => Some(local_time),
            _ => None,
        }
    }

    /// On the tick: once the check has finished, the new database (if any)
    /// and the line that says how it ended.
    pub fn drain(&mut self) -> Option<VtankGameInfoUpdateResult> {
        if !matches!(self.pending, Some(Pending::Finished(_))) {
            return None;
        }
        let Some(Pending::Finished(outcome)) = self.pending.take() else {
            return None;
        };
        Some((*outcome).unwrap_or_else(|error| failed(&error)))
    }

    /// Stops a check for good: an answer that arrives afterwards is not
    /// written, and none starts after this. The handle the client may still
    /// answer, for the caller to forget.
    pub fn dispose(&mut self) -> Option<u32> {
        if self.disposed {
            return None;
        }
        self.disposed = true;
        let handle = self.waiting_handle();
        self.pending = None;
        handle
    }
}

/// How much of a download's work a tick does: bytes of its text read, or
/// their weight in the other stages (a two-megabyte database takes a
/// dozen ticks or so).
pub const WORK_PER_TICK: usize = 200_000;

/// Bytes decoded for one unit of work.
const DECODE_BYTES_PER_UNIT: usize = 8;

/// Bytes of text whose database is built for one unit of work.
const BUILD_BYTES_PER_UNIT: usize = 4;

/// Bytes saved for one unit of work.
const SAVE_BYTES_PER_UNIT: usize = 1;

/// A download that came in, worked through a stage at a time
/// ([`VtankGameInfoUpdater::step`]): the text decoded, read a run of rows a
/// step, the database built, saved. Every stage comes to what the checks
/// and the save come to on the whole answer at once (`RunAsync` after the
/// download).
#[derive(Debug)]
struct Received {
    local_time: i32,
    now_unix_seconds: i64,
    stage: Stage,
}

#[derive(Debug)]
enum Stage {
    /// The answer's bytes, not yet decoded.
    Bytes(Vec<u8>),
    /// The text, being read.
    Reading(Box<DownloadReading>),
    /// Read and checked: the database is built next.
    Building {
        read: Box<DownloadRead>,
        downloaded_time: i32,
    },
    /// Built: saved next.
    Saving {
        text: String,
        database: Box<VtankGameInfoDatabase>,
        downloaded_time: i32,
    },
}

impl Received {
    /// Stages on, for about `work` units of work (bytes of text read, or
    /// their weight in decoding, building and saving): the outcome once
    /// there is one. A stage that weighs more than a whole tick has a tick
    /// of its own.
    fn step(
        &mut self,
        work: usize,
        profiles: &mut dyn PluginStorage,
        state: &mut dyn PluginStorage,
    ) -> Option<VtankGameInfoUpdateResult> {
        let mut left = work;
        loop {
            let cost = match &self.stage {
                Stage::Bytes(bytes) => bytes.len() / DECODE_BYTES_PER_UNIT,
                Stage::Reading(_) => 0,
                Stage::Building { read, .. } => read.text.len() / BUILD_BYTES_PER_UNIT,
                Stage::Saving { text, .. } => text.len() / SAVE_BYTES_PER_UNIT,
            };
            if left == 0 || (cost > left && left < work) {
                return None;
            }
            left = left.saturating_sub(cost);
            let stage = std::mem::replace(&mut self.stage, Stage::Bytes(Vec::new()));
            match stage {
                Stage::Bytes(bytes) => {
                    let text = read_all_text(bytes);
                    self.stage = Stage::Reading(Box::new(DownloadReading::new(text)));
                }
                Stage::Reading(mut reading) => {
                    let before = reading.position();
                    let done = reading.step(left);
                    left = left.saturating_sub(reading.position() - before);
                    if !done {
                        self.stage = Stage::Reading(reading);
                        return None;
                    }
                    let read = reading.finish()?;
                    if let Some(outcome) = self.checked(read, state) {
                        return Some(outcome);
                    }
                }
                Stage::Building {
                    read,
                    downloaded_time,
                } => {
                    let (text, database) = read.database();
                    self.stage = Stage::Saving {
                        text,
                        database: Box::new(database),
                        downloaded_time,
                    };
                }
                Stage::Saving {
                    text,
                    database,
                    downloaded_time,
                } => {
                    return Some(self.saved(&text, *database, downloaded_time, profiles, state));
                }
            }
        }
    }

    /// The checks on what was read: an outcome when one ends the check,
    /// else the database is built next.
    fn checked(
        &mut self,
        read: DownloadRead,
        state: &mut dyn PluginStorage,
    ) -> Option<VtankGameInfoUpdateResult> {
        if let Err(error) = &read.layout {
            return Some(failed(&format!(
                "the download is not a game database ({})",
                error.0
            )));
        }
        if !read.every_value_reads {
            return Some(failed(
                "the download is not a game database (a value in it does not read)",
            ));
        }
        let version = read.version;
        if version != gameinfo::BUILT_IN_VERSION {
            return Some(failed(&format!(
                "the download is database version {version}, not {}",
                gameinfo::BUILT_IN_VERSION
            )));
        }
        let Some(downloaded_time) = read.last_update_time else {
            return Some(failed(
                "the download does not say which world data it was built from",
            ));
        };
        // Any other time means another database: an older release, or one
        // that came from somewhere else (a VTank install, another service),
        // whose time may even be later. Only openac-gamedata's own is kept.
        if downloaded_time == self.local_time {
            let built = format_date(downloaded_time);
            return Some(VtankGameInfoUpdateResult {
                message: format!(
                    "Game database is up to date ({built}, {SOURCE_NAME}).{}",
                    note_check(state, self.now_unix_seconds)
                ),
                database: None,
            });
        }
        self.stage = Stage::Building {
            read: Box::new(read),
            downloaded_time,
        };
        None
    }

    /// The save, and the line that says how it went.
    fn saved(
        &self,
        text: &str,
        database: VtankGameInfoDatabase,
        downloaded_time: i32,
        profiles: &mut dyn PluginStorage,
        state: &mut dyn PluginStorage,
    ) -> VtankGameInfoUpdateResult {
        let built = format_date(downloaded_time);
        if let Err(error) = profiles.write_text(gameinfo::FILE_NAME, text) {
            // The download is good; only keeping it failed, so it is used
            // for as long as the plugin runs.
            return VtankGameInfoUpdateResult {
                message: format!(
                    "Game database updated to {built} from {SOURCE_NAME} but could not be saved: {}. It is used until MossTank stops.",
                    error.trim_end_matches('.')
                ),
                database: Some(database),
            };
        }
        let line = if downloaded_time < self.local_time {
            format!("Game database replaced with {SOURCE_NAME}'s ({built}).")
        } else {
            format!("Game database updated to {built} from {SOURCE_NAME}.")
        };
        VtankGameInfoUpdateResult {
            message: line + &note_check(state, self.now_unix_seconds),
            database: Some(database),
        }
    }
}

/// `File.ReadAllText(path, Encoding.UTF8)` of an answer
/// ([`text::read_all_text`]): a text that is valid UTF-8 after its mark (the
/// whole answer, as a rule) is checked in one pass and taken as it is,
/// which is what decoding it a character at a time gives.
fn read_all_text(bytes: Vec<u8>) -> String {
    match text::detect_encoding(&bytes) {
        (text::DetectedEncoding::Utf8, 0) => match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(error) => text::read_all_text(error.as_bytes()),
        },
        (text::DetectedEncoding::Utf8, mark) => match std::str::from_utf8(&bytes[mark..]) {
            Ok(text) => text.to_owned(),
            Err(_) => text::read_all_text(&bytes),
        },
        _ => text::read_all_text(&bytes),
    }
}

/// Keeps the time of a check that reached the source, so a login within
/// the window does not download again. The empty string when it is kept;
/// otherwise the reason, for the end of the line.
fn note_check(state: &mut dyn PluginStorage, now_unix_seconds: i64) -> String {
    if !state.is_available() {
        return String::new();
    }
    match state.write_text(LAST_CHECK_KEY, &now_unix_seconds.to_string()) {
        Ok(()) => String::new(),
        Err(error) => format!(
            " (The time of this check could not be kept: {}.)",
            error.trim_end_matches('.')
        ),
    }
}

/// When the database was last checked, seconds since 1970, or `None` when
/// no check has been kept.
///
/// # Errors
/// The note cannot be read.
pub fn read_last_check(state: &mut dyn PluginStorage) -> Result<Option<i64>, StorageError> {
    if !state.is_available() {
        return Ok(None);
    }
    let text = state.read_text(LAST_CHECK_KEY)?;
    Ok(text
        .as_deref()
        .map(|text| text.trim_matches(casing::is_white_space))
        .and_then(|text| number::parse_i64(text, NumberStyles::INTEGER))
        .filter(|seconds| *seconds > 0))
}

/// The last second .NET's `DateTimeOffset` holds, in Unix seconds
/// (9999-12-31 23:59:59 UTC).
const MAX_UNIX_SECONDS: i64 = 253_402_300_799;

/// The last check moved on by `interval_seconds`, with the range checks the
/// C#'s `DateTimeOffset` makes: a kept time past its last second throws out
/// of `FromUnixTimeSeconds` (in `ReadLastCheck`), and a sum past it out of
/// the addition.
///
/// # Errors
/// The message of the `ArgumentOutOfRangeException` the C# throws there
/// (checked against .NET 10).
pub fn next_check_after(last_check: i64, interval_seconds: i64) -> Result<i64, String> {
    if last_check > MAX_UNIX_SECONDS {
        return Err(format!(
            "Valid values are between -62135596800 and {MAX_UNIX_SECONDS}, inclusive. (Parameter 'seconds')\r\nActual value was {last_check}."
        ));
    }
    match last_check.checked_add(interval_seconds) {
        Some(next) if next <= MAX_UNIX_SECONDS => Ok(next),
        _ => Err(
            "The added or subtracted value results in an un-representable DateTime. (Parameter 't')"
                .to_owned(),
        ),
    }
}

fn failed(reason: &str) -> VtankGameInfoUpdateResult {
    VtankGameInfoUpdateResult {
        message: format!(
            "Game database update failed: {}.{KEPT_OLD}",
            reason.trim_end_matches('.')
        ),
        database: None,
    }
}

/// The `DBLastUpdateTime` row's time, in seconds since 1970, or `None` when
/// it has none.
#[must_use]
pub fn try_last_update_time(database: &VtankDatabase) -> Option<i32> {
    gameinfo::last_update_time(database)
}

fn utc_seconds(seconds: i64) -> DateTime {
    DateTime::utc_from_unix_millis(seconds.saturating_mul(1000))
}

/// A time as `yyyy-MM-dd HH:mm 'UTC'`.
#[must_use]
pub fn format_utc(unix_seconds: i64) -> String {
    format!(
        "{} UTC",
        utc_seconds(unix_seconds)
            .format("yyyy-MM-dd HH:mm")
            .expect("a pattern the kit writes")
    )
}

/// A database's own time as the day its world data is from.
#[must_use]
pub fn format_date(seconds: i32) -> String {
    utc_seconds(i64::from(seconds))
        .format("yyyy-MM-dd")
        .expect("a pattern the kit writes")
}

#[cfg(test)]
mod tests {
    use super::seconds_text;

    #[test]
    fn seconds_are_written_with_at_most_two_decimals() {
        assert_eq!(seconds_text(30_000), "30");
        assert_eq!(seconds_text(50), "0.05");
        assert_eq!(seconds_text(1_500), "1.5");
    }
}

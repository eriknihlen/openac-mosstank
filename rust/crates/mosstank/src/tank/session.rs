//! What the shell keeps of a session beside its rules
//! (`MossTankPanel.ActionHistory`, `.GameInfo`, `.Status` and the kill
//! statistics): the action history and the macro debug log, the opt-in
//! kill counters, the game database's update check, and the meta state
//! published for other plugins.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::LazyLock;

use ac_plugin_abi::net::{HttpReply, HttpRequest};
use mosstank_meta::debug_log::MacroDebugLog;
use mosstank_session::action_history::ActionHistory;
use mosstank_session::game_info::{
    self, GameInfoTransport, HttpGameInfoTransport, VtankGameInfoUpdater,
};
use mosstank_session::kill_statistics::{KillCharacter, KillStatistics, KillStatisticsHost};
use mosstank_stores::storage::{PluginStorage, Root};
use openac_plugin_compat::number;
use openac_plugin_compat::regex::{DotNetRegex, RegexOptions};
use openac_plugin_compat::text::casing;

use super::{Core, HostClock, Tank, TankCx, TankFamily};
use crate::client::TankClient;
use mosstank_ub::commands::split_head;

/// The status line other plugins read for the macro's current meta state,
/// the value VTank's external interface reported as its current meta state.
pub(crate) const META_STATE_STATUS_KEY: &str = "metaState";

/// `/vt gamedb`'s syntax line.
pub(crate) const GAME_DB_SYNTAX: &str = "Syntax: /vt gamedb [update | interval [hours]]";

/// A number followed by a unit, which an activity line's repeats differ
/// in (`ActivityMeasurements`).
static ACTIVITY_MEASUREMENTS: LazyLock<DotNetRegex> = LazyLock::new(|| {
    DotNetRegex::new(
        r"\d+(?:[.,]\d+)?(?=[msd]\b)",
        RegexOptions::CULTURE_INVARIANT,
    )
    .expect("a fixed pattern the kit reproduces")
});

/// The action history and the debug log, shared with the fight's status
/// hook.
pub(crate) struct HistoryLog {
    pub(crate) history: ActionHistory,
    pub(crate) debug_log: Option<MacroDebugLog>,
    clock: Rc<HostClock>,
}

impl HistoryLog {
    /// `RecordActionHistory`: the fight's status changed.
    pub(crate) fn record_combat(&mut self, status: &str) {
        self.history.record(status, &*self.clock);
        self.write_debug(&format!("Combat: {status}"));
    }

    fn write_debug(&mut self, message: &str) {
        let now = mosstank_expr::ExpressionClock::utc_now_ms(&*self.clock);
        if let Some(log) = &mut self.debug_log {
            log.record(now, message);
        }
    }
}

/// The shared history.
pub(crate) type SharedHistory = Rc<RefCell<HistoryLog>>;

/// The session's own records.
pub(crate) struct SessionParts {
    pub(crate) history: SharedHistory,
    activity_states: HashMap<String, String>,
    activity_elapsed: f64,
    debug_flush_elapsed: f64,
    pub(crate) action_history_visible: bool,
    pub(crate) kill_statistics: KillStatistics,
    pub(crate) game_info_updater: VtankGameInfoUpdater,
    game_info_checked_this_session: bool,
    game_info_transport: HttpGameInfoTransport,
    published_meta_state: Option<String>,
}

impl SessionParts {
    /// The parts, with the kill statistics' switch read (`KillStatistics`'s
    /// constructor).
    pub(crate) fn new(client: &mut dyn TankClient, clock: Rc<HostClock>) -> Self {
        let kill_statistics = KillStatistics::new(&mut KillHost(client), &*clock);
        Self {
            history: Rc::new(RefCell::new(HistoryLog {
                history: ActionHistory::new(),
                debug_log: None,
                clock,
            })),
            activity_states: HashMap::new(),
            activity_elapsed: 0.0,
            debug_flush_elapsed: 0.0,
            action_history_visible: false,
            kill_statistics,
            // The plugin hands the panel the HTTP transport.
            game_info_updater: VtankGameInfoUpdater::new(true),
            game_info_checked_this_session: false,
            game_info_transport: HttpGameInfoTransport::default(),
            published_meta_state: None,
        }
    }
}

/// The client as the kill statistics reach it.
struct KillHost<'a>(&'a mut dyn TankClient);

impl KillStatisticsHost for KillHost<'_> {
    fn storage(&mut self) -> &mut dyn PluginStorage {
        self.0.stores().storage(Root::Plugin)
    }

    fn is_available(&mut self) -> bool {
        self.0.rules().is_available()
    }

    /// The client has a character only while it is in the world.
    fn character(&mut self) -> KillCharacter {
        let in_world = self.0.rules().is_available();
        let character = self.0.rules().character();
        KillCharacter {
            in_world,
            object_id: character.id,
            name: character.name,
            world: character.world,
        }
    }

    fn log_error(&mut self, message: &str) {
        self.0.looter().log_error(message);
    }

    fn new_session_id(&mut self) -> String {
        new_guid_n(self.0)
    }
}

/// A fresh id as `Guid.NewGuid().ToString("N")` writes one, from the
/// client's random seeds.
fn new_guid_n(client: &mut dyn TankClient) -> String {
    let high = client.clock().random_seed;
    let low = client.clock().random_seed;
    format!("{high:016x}{low:016x}")
}

/// The game database request over the client.
struct ClientTransport<'a, 'b> {
    client: &'a RefCell<&'b mut dyn TankClient>,
    transport: HttpGameInfoTransport,
}

impl GameInfoTransport for ClientTransport<'_, '_> {
    fn get(&mut self, address: &str) -> Result<u32, String> {
        let request: HttpRequest = self.transport.request(address);
        self.client.borrow_mut().http_request(request)
    }
}

impl Core {
    /// `ObserveActivity`: a part's line, unless it says what it said last
    /// time but for its numbers.
    pub(crate) fn observe_activity(&mut self, category: &str, status: &str) {
        let key = ACTIVITY_MEASUREMENTS
            .replace(status, "#")
            .unwrap_or_else(|_| status.to_owned());
        if self.session.activity_states.get(category) == Some(&key) {
            return;
        }
        self.session
            .activity_states
            .insert(category.to_owned(), key);
        let line = format!("{category}: {status}");
        let mut history = self.session.history.borrow_mut();
        let clock = Rc::clone(&history.clock);
        history.history.record(&line, &*clock);
        history.write_debug(&line);
    }

    /// `FlushDebugLog`: a log that cannot be written stops, and says why.
    pub(crate) fn flush_debug_log(&mut self, client: &mut dyn TankClient) {
        let failed = {
            let mut history = self.session.history.borrow_mut();
            let Some(log) = &mut history.debug_log else {
                return;
            };
            log.flush(client.stores().storage(Root::Plugin)).err()
        };
        if let Some(error) = failed {
            self.session.history.borrow_mut().debug_log = None;
            client
                .looter()
                .log_error(&format!("MossTank debug logging stopped: {error}"));
            super::post_chat(
                client.rules(),
                &format!("MossTank debug logging stopped: {error}"),
            );
        }
    }

    /// `Debug log: off` or `Debug log: ON` (`DebugLogButtonText`).
    pub(crate) fn debug_log_button_text(&self) -> &'static str {
        if self.session.history.borrow().debug_log.is_none() {
            "Debug log: off"
        } else {
            "Debug log: ON"
        }
    }

    /// Where the log is written, or what the button does
    /// (`DebugLogLocation`).
    pub(crate) fn debug_log_location(&self, client: &mut dyn TankClient) -> String {
        let key = self
            .session
            .history
            .borrow()
            .debug_log
            .as_ref()
            .map(|log| log.key().to_owned());
        match key {
            Some(key) => {
                let root = client
                    .stores()
                    .storage(Root::Plugin)
                    .root_path()
                    .unwrap_or_default();
                format!("{root}/{key}")
            }
            None => "Save detailed macro activity (newest 2 MiB).".to_owned(),
        }
    }

    /// `PublishStatus`: the meta state on the host's status board, when it
    /// changed.
    pub(crate) fn publish_status(&mut self, client: &mut dyn TankClient, state: &str) {
        if self.session.published_meta_state.as_deref() == Some(state) {
            return;
        }
        if client.status_publish(META_STATE_STATUS_KEY, Some(state)) {
            self.session.published_meta_state = Some(state.to_owned());
        }
    }

    /// The kill statistics, with the client.
    pub(crate) fn tick_kill_statistics(&mut self, client: &mut dyn TankClient) {
        let clock = Rc::clone(&self.clock);
        self.session
            .kill_statistics
            .tick(&mut KillHost(client), &*clock);
    }

    /// `TickGameInfoUpdate`: an answer that came in is handed over and
    /// said; a session in the world checks once, unless the last check is
    /// younger than the preference's interval.
    pub(crate) fn tick_game_info_update(&mut self, client: &mut dyn TankClient) {
        if let Some(result) = self.session.game_info_updater.drain() {
            if let Some(database) = result.database {
                self.apply_game_info(database);
            }
            Core::write_vtank(client, &result.message);
        }
        // The session counts as checked only once its own check has
        // started. One still running from the session before (a quick
        // relog) is let finish and handed over first, and this session asks
        // after it. A session with nothing to check with has nothing to say.
        if !self.session.game_info_checked_this_session
            && self
                .session
                .game_info_updater
                .can_update(client.stores().storage(Root::Profiles))
            && !self.session.game_info_updater.is_running()
        {
            let interval = i64::from(self.profiles.game_db_check_interval_hours()) * 3600;
            self.session.game_info_checked_this_session =
                self.start_game_info_update(client, interval).is_none();
        }
    }

    /// Starts a check: the reason it did not start, or `None`.
    fn start_game_info_update(
        &mut self,
        client: &mut dyn TankClient,
        skip_if_checked_within_seconds: i64,
    ) -> Option<String> {
        let now = client.clock().utc_unix_ms.div_euclid(1000);
        let cell = RefCell::new(client);
        let mut profiles = OneStorage {
            client: &cell,
            root: Root::Profiles,
        };
        let mut state = OneStorage {
            client: &cell,
            root: Root::Plugin,
        };
        let mut transport = ClientTransport {
            client: &cell,
            transport: self.session.game_info_transport,
        };
        self.session.game_info_updater.start(
            &mut profiles,
            &mut state,
            Some(&mut transport),
            now,
            skip_if_checked_within_seconds,
        )
    }

    /// The client answered an HTTP request: the game database check's is
    /// checked and saved; whether it was that.
    pub(crate) fn on_http_reply(&mut self, client: &mut dyn TankClient, reply: &HttpReply) -> bool {
        if self.session.game_info_updater.waiting_handle() != Some(reply.handle) {
            return false;
        }
        let answer = self.session.game_info_transport.answer(reply);
        let now = client.clock().utc_unix_ms.div_euclid(1000);
        let cell = RefCell::new(client);
        let mut profiles = OneStorage {
            client: &cell,
            root: Root::Profiles,
        };
        let mut state = OneStorage {
            client: &cell,
            root: Root::Plugin,
        };
        self.session.game_info_updater.receive(
            reply.handle,
            &answer,
            &mut profiles,
            &mut state,
            now,
        )
    }

    /// `/vt gamedb [update | interval [hours]]` (`HandleGameDbCommand`).
    pub(crate) fn game_db_command(&mut self, client: &mut dyn TankClient, arguments: &str) {
        let (verb, rest) = split_head(arguments.trim());
        match casing::to_lower_invariant(&verb).as_str() {
            "" => {
                let described = self.describe_game_info(client);
                Core::write_vtank(client, &described);
                self.write_next_game_info_check(client);
            }
            "update" => self.start_game_info_update_command(client),
            "interval" => self.game_db_interval_command(client, rest.trim()),
            _ => Core::write_vtank(client, GAME_DB_SYNTAX),
        }
    }

    /// `StartGameInfoUpdate`: `/vt gamedb update` and `/vt getdb`.
    pub(crate) fn start_game_info_update_command(&mut self, client: &mut dyn TankClient) {
        let line = self
            .start_game_info_update(client, 0)
            .unwrap_or_else(|| "Checking the game database for updates.".to_owned());
        Core::write_vtank(client, &line);
    }

    fn game_db_interval_command(&mut self, client: &mut dyn TankClient, value: &str) {
        if !value.is_empty() {
            match number::parse_i32(value, number::NumberStyles::INTEGER) {
                Some(hours) if hours >= 0 => {
                    self.profiles
                        .set_game_db_check_interval_hours(client.stores(), hours);
                }
                _ => {
                    Core::write_vtank(
                        client,
                        &format!(
                            "Syntax: /vt gamedb interval [0-{} hours]",
                            mosstank_stores::profile_store::MAXIMUM_GAME_DB_CHECK_INTERVAL_HOURS
                        ),
                    );
                    return;
                }
            }
        }
        self.write_next_game_info_check(client);
    }

    /// Writes [`Self::describe_next_game_info_check`]; a time out of .NET's
    /// range ends the command as the C#'s throw does.
    fn write_next_game_info_check(&mut self, client: &mut dyn TankClient) {
        match self.describe_next_game_info_check(client) {
            Ok(next) => Core::write_vtank(client, &next),
            Err(error) => super::commands::command_failed(client, &error),
        }
    }

    /// When the next login will check, and how that is decided
    /// (`DescribeNextGameInfoCheck`), or the message of what the C# throws
    /// for a kept time out of `DateTimeOffset`'s range.
    fn describe_next_game_info_check(
        &mut self,
        client: &mut dyn TankClient,
    ) -> Result<String, String> {
        let hours = self.profiles.game_db_check_interval_hours();
        if hours == 0 {
            return Ok(
                "Game database: checked at every login (/vt gamedb interval sets how often)."
                    .to_owned(),
            );
        }
        let every = format!(
            "every {hours}{}",
            if hours == 1 { " hour" } else { " hours" }
        );
        Ok(
            match game_info::read_last_check(client.stores().storage(Root::Plugin)) {
                Ok(Some(last_check)) => {
                    let next = game_info::next_check_after(last_check, i64::from(hours) * 3600)?;
                    format!(
                        "Game database: checked {every}; the next check is after {}.",
                        game_info::format_utc(next)
                    )
                }
                Ok(None) => format!("Game database: checked {every}; the next login checks."),
                Err(error) => {
                    client.looter().log_error(&format!(
                        "MossTank could not read the game database's last check: {error}"
                    ));
                    format!("Game database: checked {every}; the next login checks.")
                }
            },
        )
    }

    /// `DescribeGameInfo`.
    fn describe_game_info(&mut self, client: &mut dyn TankClient) -> String {
        let profiles = client.stores().storage(Root::Profiles);
        let file_exists = profiles.is_available()
            && profiles
                .read_text(mosstank_formats::gameinfo::FILE_NAME)
                .ok()
                .flatten()
                .is_some_and(|text| !casing::is_white_space_only(&text));
        let source = if file_exists {
            format!(
                "{} in the VTank profile folder",
                mosstank_formats::gameinfo::FILE_NAME
            )
        } else {
            format!(
                "no {} in the VTank profile folder; /vt gamedb update downloads it",
                mosstank_formats::gameinfo::FILE_NAME
            )
        };
        let database = &self.game_info;
        let version = database
            .version
            .map_or_else(|| "none".to_owned(), |number| number.to_string());
        let built = match database.last_update_time {
            Some(seconds) if seconds > 0 => game_info::format_date(seconds),
            _ => "none".to_owned(),
        };
        let running = if self.session.game_info_updater.is_running() {
            " An update is running."
        } else {
            ""
        };
        format!(
            "Game database: {source}. Loaded: version {version}, world data of {built}. \
             Ammunition {}, monster damage {}, species damage {}, species members {}, \
             immunities {}, heal kits {}, grenades {}, drain spells {}, martyr spells {}, \
             craft recipes {}.{running}",
            database.ammunition_options.len(),
            database.monster_damage_override_count(),
            database.species_damages.len(),
            database.species_member_count(),
            database.monster_immunity_count(),
            database.heal_kits.len(),
            database.grenade_options.len(),
            database.drain_spell_options.len(),
            database.martyr_spell_options.len(),
            database.crafts.recipes().len(),
        )
    }
}

/// One of the two storages over a client that the check's other storage
/// and its transport reach too, each in turn.
struct OneStorage<'a, 'b> {
    client: &'a RefCell<&'b mut dyn TankClient>,
    root: Root,
}

impl PluginStorage for OneStorage<'_, '_> {
    fn is_available(&mut self) -> bool {
        self.client
            .borrow_mut()
            .stores()
            .storage(self.root)
            .is_available()
    }

    fn root_path(&mut self) -> Option<String> {
        self.client
            .borrow_mut()
            .stores()
            .storage(self.root)
            .root_path()
    }

    fn read_text(&mut self, key: &str) -> Result<Option<String>, String> {
        self.client
            .borrow_mut()
            .stores()
            .storage(self.root)
            .read_text(key)
    }

    fn list(&mut self, prefix: &str) -> Result<Vec<String>, String> {
        self.client
            .borrow_mut()
            .stores()
            .storage(self.root)
            .list(prefix)
    }

    fn write_text(&mut self, key: &str, content: &str) -> Result<(), String> {
        self.client
            .borrow_mut()
            .stores()
            .storage(self.root)
            .write_text(key, content)
    }

    fn ensure_directory(&mut self, prefix: &str) -> Result<bool, String> {
        self.client
            .borrow_mut()
            .stores()
            .storage(self.root)
            .ensure_directory(prefix)
    }

    fn delete(&mut self, key: &str) -> Result<bool, String> {
        self.client
            .borrow_mut()
            .stores()
            .storage(self.root)
            .delete(key)
    }
}

impl TankCx<'_> {
    /// `ObserveMacroActivity`: the debug log is written every five seconds,
    /// and every half second each part's state goes in the history when it
    /// changed.
    pub(crate) fn observe_macro_activity(
        &mut self,
        scheduler: &mosstank_meta::scheduler::MacroScheduler<TankFamily>,
        elapsed_seconds: f64,
    ) {
        let elapsed = elapsed_seconds.max(0.0);
        self.core.session.debug_flush_elapsed += elapsed;
        if self.core.session.debug_flush_elapsed >= 5.0 {
            self.core.session.debug_flush_elapsed = 0.0;
            self.core.flush_debug_log(self.client);
        }
        self.core.session.activity_elapsed += elapsed;
        if self.core.session.activity_elapsed < 0.5 {
            return;
        }
        self.core.session.activity_elapsed = 0.0;
        let available = self.client.rules().is_available();
        let running = self.core.combat.enabled();
        let macro_line = if !available {
            "Waiting for login.".to_owned()
        } else if !running {
            "Stopped.".to_owned()
        } else if scheduler.is_suspended() {
            format!(
                "Paused: {}",
                self.core
                    .reported_suspension
                    .as_deref()
                    .unwrap_or("waiting for an action to finish")
            )
        } else {
            "Running.".to_owned()
        };
        self.core.observe_activity("Macro", &macro_line);
        if !available || !running {
            return;
        }
        let rule = scheduler.last_executed_index().map_or_else(
            || "Waiting for work.".to_owned(),
            |index| scheduler.main_rules()[index].name(),
        );
        self.core.observe_activity("Rule", &rule);
        let navigation = self.core.navigation.status().to_owned();
        self.core.observe_activity("Navigation", &navigation);
        let meta = if self.meta.enabled() {
            format!("{} - {}", self.meta.current_state(), self.meta.status())
        } else {
            "Disabled.".to_owned()
        };
        self.core.observe_activity("Meta", &meta);
        let loot = self.core.loot.status.clone();
        self.core.observe_activity("Loot", &loot);
        let buffs = self.core.status.clone();
        self.core.observe_activity("Buffs", &buffs);
        let vitals = self.core.vital_status().to_owned();
        self.core.observe_activity("Vitals", &vitals);
        let item_mana = self.core.item_mana.status().to_owned();
        self.core.observe_activity("Item mana", &item_mana);
    }

    /// `ToggleDebugLogging`.
    fn toggle_debug_logging(&mut self) {
        let running = self.core.session.history.borrow().debug_log.is_some();
        if running {
            self.core
                .session
                .history
                .borrow_mut()
                .write_debug("Logging stopped.");
            self.core.flush_debug_log(self.client);
            self.core.session.history.borrow_mut().debug_log = None;
            return;
        }
        if !self.client.stores().storage(Root::Plugin).is_available() {
            super::post_chat(
                self.client.rules(),
                "MossTank debug log: plugin storage is unavailable.",
            );
            return;
        }
        let now = self.client.clock().utc_unix_ms;
        let stamp = openac_plugin_compat::datetime::DateTime::utc_from_unix_millis(now)
            .format("yyyyMMdd-HHmmss")
            .expect("a pattern the kit writes");
        let key = format!("debug/macro-{stamp}-{}.log", new_guid_n(self.client));
        let character = self.client.rules().character();
        {
            let mut history = self.core.session.history.borrow_mut();
            history.debug_log = Some(MacroDebugLog::new(&key));
            history.write_debug(&format!(
                "MossTank {}; character={}; world={}",
                crate::VERSION,
                character.name,
                character.world
            ));
            let lines = history.history.lines();
            for line in lines.iter() {
                history.write_debug(line);
            }
        }
        self.core.flush_debug_log(self.client);
        if self.core.session.history.borrow().debug_log.is_some() {
            let location = self.core.debug_log_location(self.client);
            super::post_chat(
                self.client.rules(),
                &format!("MossTank debug log: {location}"),
            );
        }
    }
}

impl Core {
    /// `VitalStatus`: the helper's status while the character's own vitals
    /// are idle.
    pub(crate) fn vital_status(&self) -> &str {
        if self.vital_recharge.status() == mosstank_buffs::recharge_controller::IDLE_STATUS {
            self.vital_helper_recharge.status()
        } else {
            self.vital_recharge.status()
        }
    }

    /// The session ends: the next one checks the game database again, as a
    /// new login does in the reference client.
    pub(crate) fn end_session_records(&mut self) {
        self.session.game_info_checked_this_session = false;
    }

    /// The plugin stops (`Dispose`, the session's part): the debug log is
    /// written and closed, the kill statistics close their segment, and the
    /// game database check stops for good.
    pub(crate) fn dispose_session_records(&mut self, client: &mut dyn TankClient) {
        self.flush_debug_log(client);
        self.session.history.borrow_mut().debug_log = None;
        let clock = Rc::clone(&self.clock);
        self.session
            .kill_statistics
            .dispose(&mut KillHost(client), &*clock);
        if let Some(handle) = self.session.game_info_updater.dispose() {
            client.net_close(handle);
        }
    }

    /// The client's notifications the kill statistics follow.
    pub(crate) fn kill_statistics_event(
        &mut self,
        client: &mut dyn TankClient,
        event: &ac_plugin_abi::Event,
    ) {
        use ac_plugin_abi::Event;
        let clock = Rc::clone(&self.clock);
        let recorder = &mut self.session.kill_statistics;
        let mut host = KillHost(client);
        match event {
            Event::ChatReceived(line) => recorder.on_chat(
                &mut host,
                &*clock,
                line.sequence,
                &line.text,
                line.log_text_type,
            ),
            Event::Logoff => recorder.on_logoff(&mut host, &*clock),
            Event::LoginComplete => recorder.on_login_complete(&mut host, &*clock),
            _ => {}
        }
    }
}

impl Tank {
    /// The action history's lines, oldest first; the same lines until the
    /// next change (`ActionHistoryLines`).
    #[must_use]
    pub fn action_history_lines(&self) -> Rc<[String]> {
        self.core.session.history.borrow().history.lines()
    }

    /// The first line's absolute position (`ActionHistoryFirstIndex`).
    #[must_use]
    pub fn action_history_first_index(&self) -> usize {
        self.core.session.history.borrow().history.first_index()
    }

    /// Whether the history window is open (`ActionHistoryVisible`).
    #[must_use]
    pub fn action_history_visible(&self) -> bool {
        self.core.session.action_history_visible
    }

    /// Opens the history window (`ShowActionHistory`).
    pub fn show_action_history(&mut self) {
        self.core.session.action_history_visible = true;
    }

    /// Closes it (`HideActionHistory`).
    pub fn hide_action_history(&mut self) {
        self.core.session.action_history_visible = false;
    }

    /// Empties the history (`ClearActionHistory`).
    pub fn clear_action_history(&mut self) {
        self.core.session.history.borrow_mut().history.clear();
    }

    /// The fight's status line (`CombatStatus`).
    #[must_use]
    pub fn combat_status(&self) -> &str {
        self.core.combat.status()
    }

    /// `Debug log: off` or `Debug log: ON`.
    #[must_use]
    pub fn debug_log_button_text(&self) -> &'static str {
        self.core.debug_log_button_text()
    }

    /// Where the debug log is written (`DebugLogLocation`).
    pub fn debug_log_location(&self, client: &mut dyn TankClient) -> String {
        self.core.debug_log_location(client)
    }

    /// Starts or stops the debug log (`ToggleDebugLog`).
    pub fn toggle_debug_log(&mut self, client: &mut dyn TankClient) {
        self.begin_call(client);
        let (mut cx, _) = self.cx(client);
        cx.toggle_debug_logging();
        cx.core.write_out(cx.client);
        self.end_call(client);
    }

    /// The database each of its readers is using now: the shell's, the
    /// fight's, the monster facts' and crafting's (`GameInfoReadersForTest`).
    #[must_use]
    pub fn game_info_readers(&self) -> [&mosstank_formats::gameinfo::VtankGameInfoDatabase; 4] {
        [
            &self.core.game_info,
            self.core.combat.game_info(),
            self.core.settings.combat.monster_facts.database(),
            self.core.crafting.game_info(),
        ]
    }

    /// Whether the heal kit table names `kit` (`HealKitsForTest`).
    #[must_use]
    pub fn has_heal_kit(&self, kit: &str) -> bool {
        self.core.settings.combat.heal_kits.contains_key(kit)
    }

    /// Whether a game database check is under way or not yet handed over
    /// (`GameInfoUpdatePendingForTest` is not null).
    #[must_use]
    pub fn game_info_update_running(&self) -> bool {
        self.core.session.game_info_updater.is_running()
    }

    /// The request the game database check is waiting on.
    #[must_use]
    pub fn game_info_request(&self) -> Option<u32> {
        self.core.session.game_info_updater.waiting_handle()
    }
}

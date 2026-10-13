//! `/ub equip` (`EquipProfileController`): puts on a set of gear named by a
//! loot profile under the UB tree's `equip` folder. Everything not in the
//! set comes off, then the set goes on with the lore-gated pieces last, so
//! a piece that raises lore is worn before the piece that needs it. It also
//! writes such a profile from what is worn now, and lists and dry-runs them.
//!
//! The combat controller owns what is wielded while a monster is fought, so
//! this is a prologue owner only: it refuses to start while the combat
//! controller has a target, and holds the item-use and navigation locks for
//! its whole run. Each step waits for the client's placement receipt
//! ([`EquipProfileController::on_equipment_placement`]); a run with no
//! progress for ten seconds gives up.

use std::collections::{HashMap, HashSet, VecDeque};

use ac_plugin_abi::LogLevel;
use ac_plugin_abi::items::{EquipmentObservation, EquipmentStatus, InventoryItem, ItemStatus};
use ac_plugin_abi::world::{ItemProperties, ObjectClass};
use mosstank_automation::locks::{ActionLockKind, ActionLockTable};
use mosstank_formats::utl::{self, LootAction, LootRule, VtankLootRequirement};
use mosstank_loot::rules;
use mosstank_stores::storage::StorageError;
use openac_plugin_compat::number;
use openac_plugin_compat::regex;
use openac_plugin_compat::text::{casing, ordinal};

use crate::chat::{self, tools};
use crate::commands::UbCall;

/// A run with no progress for this long gives up.
const BAIL_SECONDS: f64 = 10.0;
/// How long a step holds the locks; re-armed every frame of a load and let
/// go when the run ends.
const LOCK_SECONDS: f64 = 30.0;
/// One appraisal request in flight at a time, this far apart.
const IDENTIFY_INTERVAL_SECONDS: f64 = 0.5;
/// How long one appraisal is waited for before it counts as failed.
const IDENTIFY_TIMEOUT_SECONDS: f64 = 10.0;
const MAXIMUM_IDENTIFY_ATTEMPTS_PER_ITEM: i32 = 3;
/// How often the wait for appraisals is mentioned in chat.
const IDENTIFY_NOTICE_SECONDS: f64 = 15.0;
/// How often one piece is asked to go on before it is skipped.
const MAXIMUM_EQUIP_ATTEMPTS_PER_ITEM: i32 = 15;
/// How long a create keeps its "say it again to overwrite" offer.
const OVERWRITE_CONFIRMATION_SECONDS: f64 = 30.0;

const PROFILE_FOLDER: &str = "equip";
const PROFILE_EXTENSION: &str = ".utl";

/// The item property carrying the lore needed to use it.
const LORE_REQUIREMENT_PROPERTY: u32 = 109;
/// The material property, in a created profile.
const MATERIAL_PROPERTY: u32 = 131;
/// The value property, in a created profile.
const VALUE_PROPERTY: u32 = 19;
/// The loot-profile key space for an item's own fields.
const PROFILE_INT_BASE: u32 = 218_103_808;
/// The slots an item can occupy, in that key space.
const PROFILE_EQUIPABLE_SLOTS_KEY: u32 = PROFILE_INT_BASE + 14;
/// The name string key in a loot profile requirement.
const PROFILE_NAME_KEY: u32 = 1;
const STRING_MATCH_REQUIREMENT: i32 = 1;
const INT_EQUALS_REQUIREMENT: i32 = 12;

/// The row that turns the finished line into a think.
const THINK_SETTING: &str = "EquipmentManager.Think";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Identifying,
    Dequipping,
    Equipping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Load,
    Test,
}

/// The equipment profile run.
#[derive(Debug, Clone)]
pub struct EquipProfileController {
    rules: Vec<LootRule>,
    candidates: Vec<InventoryItem>,
    to_equip: VecDeque<u32>,
    profile_items: HashSet<u32>,
    identify_attempts: HashMap<u32, i32>,
    observations: VecDeque<EquipmentObservation>,
    phase: Phase,
    mode: Mode,
    profile_key: String,
    now: f64,
    since_progress: f64,
    run_seconds: f64,
    since_identify_request: f64,
    awaiting_identify_seconds: f64,
    last_identify_notice: f64,
    awaiting_identify: u32,
    dequipping: u32,
    equipping: u32,
    equip_attempts: i32,
    refusal: String,
    refusal_is_debug_only: bool,
    pending_overwrite_key: String,
    pending_overwrite_at: f64,
    status: String,
}

impl Default for EquipProfileController {
    fn default() -> Self {
        Self::new()
    }
}

/// What a command answers: its lines, or a storage failure out of it.
type CommandResult = Result<Option<Vec<String>>, StorageError>;

impl EquipProfileController {
    /// Idle.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            candidates: Vec::new(),
            to_equip: VecDeque::new(),
            profile_items: HashSet::new(),
            identify_attempts: HashMap::new(),
            observations: VecDeque::new(),
            phase: Phase::Idle,
            mode: Mode::Load,
            profile_key: String::new(),
            now: 0.0,
            since_progress: 0.0,
            run_seconds: 0.0,
            since_identify_request: 0.0,
            awaiting_identify_seconds: 0.0,
            last_identify_notice: f64::NEG_INFINITY,
            awaiting_identify: 0,
            dequipping: 0,
            equipping: 0,
            equip_attempts: 0,
            refusal: String::new(),
            refusal_is_debug_only: false,
            pending_overwrite_key: String::new(),
            pending_overwrite_at: f64::NEG_INFINITY,
            status: "Equip profile idle.".to_owned(),
        }
    }

    /// Whether a load or a dry run is under way.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.phase != Phase::Idle
    }

    /// What it last did, in words.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// A placement the client announced (`PlacementObserved`): the plugin
    /// hands every one over as it arrives; the next frame reads them.
    pub fn on_equipment_placement(&mut self, observation: EquipmentObservation) {
        self.observations.push_back(observation);
    }

    /// The `/ub equip` verbs: list, load, test and create. `None` when the
    /// line does not parse, which the caller answers with the full help.
    pub fn command(&mut self, call: &mut UbCall<'_>, arguments: &str) -> Option<Vec<String>> {
        match self.command_lines(call, arguments) {
            Ok(lines) => lines,
            Err(error) => {
                // An exception out of a /ub command, as the dispatcher says it.
                call.host.log(
                    LogLevel::Error,
                    &format!("MossTank /ub command failed. {error}"),
                );
                Some(vec![chat::error(&format!("Command failed: {error}"))])
            }
        }
    }

    fn command_lines(&mut self, call: &mut UbCall<'_>, arguments: &str) -> CommandResult {
        let text = arguments.trim();
        let (verb, rest) = match text.find(' ') {
            None => (text, ""),
            Some(at) => (&text[..at], text[at + 1..].trim()),
        };
        Ok(match casing::to_lower_invariant(verb).as_str() {
            "list" => Some(Self::list(call, rest)?),
            // A run that starts says nothing until it has something to say;
            // a refusal is the tool's error.
            "load" => Some(if self.try_start(call, rest, Mode::Load)? {
                Vec::new()
            } else {
                self.refusal_lines(call)
            }),
            "test" => Some(if self.try_start(call, rest, Mode::Test)? {
                Vec::new()
            } else {
                self.refusal_lines(call)
            }),
            "create" => Some(vec![self.create(call, rest)?]),
            _ => None,
        })
    }

    /// Why the last start was refused, as the tool says it: a missing
    /// profile only in the debug output, anything else as its error.
    fn refusal_lines(&self, call: &mut UbCall<'_>) -> Vec<String> {
        if !self.refusal_is_debug_only {
            return vec![chat::tool_error(tools::EQUIPMENT_MANAGER, &self.refusal)];
        }
        call.write_ub_tool_debug(tools::EQUIPMENT_MANAGER, &self.refusal);
        Vec::new()
    }

    fn refuse(&mut self, reason: &str, debug_only: bool) -> bool {
        reason.clone_into(&mut self.status);
        reason.clone_into(&mut self.refusal);
        self.refusal_is_debug_only = debug_only;
        false
    }

    /// Starts a load or a dry run (the expression and the panel's form);
    /// false, with the reason in [`Self::status`], when refused.
    ///
    /// # Errors
    /// The profile storage failed.
    pub fn try_start_profile(
        &mut self,
        call: &mut UbCall<'_>,
        profile_name: &str,
        test_only: bool,
    ) -> Result<bool, StorageError> {
        self.try_start(
            call,
            profile_name,
            if test_only { Mode::Test } else { Mode::Load },
        )
    }

    fn try_start(
        &mut self,
        call: &mut UbCall<'_>,
        profile_name: &str,
        mode: Mode,
    ) -> Result<bool, StorageError> {
        if call.host.character().is_none() {
            return Ok(self.refuse("Equip profile refused: not in the world.", false));
        }
        if self.is_running() {
            return Ok(self.refuse(
                "Equip profile refused: a run is already in progress.",
                false,
            ));
        }
        if call.services.combat_has_target() {
            return Ok(self.refuse(
                "Equip profile refused: the macro has a monster target and owns what is \
                 wielded; stop the macro or wait for the fight to end.",
                false,
            ));
        }
        let file_name = file_name(call, profile_name);
        let key = call.services.ub_store().resolve_profile_key(
            call.host.profile_storage(),
            PROFILE_FOLDER,
            &file_name,
        )?;
        let storage = call.host.profile_storage();
        let text = if storage.is_available() {
            storage.read_text(&key)?
        } else {
            None
        };
        // A missing profile is said only in the debug output, as the
        // reference says it.
        let Some(text) = text else {
            return Ok(self.refuse(&format!("No equip profile exists: {key}"), true));
        };
        let Some(rules) = utl::try_parse_rules(Some(&text)) else {
            return Ok(self.refuse(&format!("Equip profile could not be read: {key}"), false));
        };
        self.rules = rules;

        self.mode = mode;
        self.profile_key.clone_from(&key);
        self.candidates = equippable_items(call);
        self.to_equip.clear();
        self.profile_items.clear();
        self.identify_attempts.clear();
        self.observations.clear();
        self.awaiting_identify = 0;
        self.awaiting_identify_seconds = 0.0;
        self.since_identify_request = IDENTIFY_INTERVAL_SECONDS;
        self.last_identify_notice = f64::NEG_INFINITY;
        self.dequipping = 0;
        self.equipping = 0;
        self.equip_attempts = 0;
        self.since_progress = 0.0;
        self.run_seconds = 0.0;
        self.phase = Phase::Identifying;
        let count = self.candidates.len();
        self.status = match mode {
            Mode::Test => format!("Equip profile test started: {key} ({count} candidate(s))."),
            Mode::Load => format!("Equip profile started: {key} ({count} candidate(s))."),
        };
        Ok(true)
    }

    /// Advances the run; true while it owns the character. The locks are
    /// re-armed on every frame of a load so a slow appraisal round cannot let
    /// them lapse mid-run.
    pub fn tick(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64, can_act: bool) -> bool {
        let step = elapsed_seconds.max(0.0);
        self.now += step;
        if !self.is_running() {
            self.observations.clear();
            return false;
        }
        if call.host.character().is_none() {
            let reason = "Equip profile stopped: the session ended.";
            self.say_error(call, reason);
            self.stop(call, Some(reason.to_owned()));
            return false;
        }
        self.run_seconds += step;
        self.since_progress += step;
        if self.mode == Mode::Load {
            call.locks.arm(ActionLockKind::ItemUse, LOCK_SECONDS);
            call.locks.arm(ActionLockKind::Navigation, LOCK_SECONDS);
        }
        self.drain_observations();
        if self.since_progress > BAIL_SECONDS {
            call.write_ub(&chat::tool(
                tools::EQUIPMENT_MANAGER,
                "bail, timeout expired",
            ));
            self.stop(
                call,
                Some("Equip profile bailed: timeout expired.".to_owned()),
            );
            return false;
        }
        if !can_act {
            return true;
        }

        // The phases fall through within one frame: a phase with nothing
        // left to wait for hands over at once rather than costing a frame.
        if self.phase == Phase::Identifying {
            self.tick_identify(call, step);
            if self.awaiting_identify != 0 || self.next_to_identify(call).is_some() {
                return true;
            }
            self.build_queue(call);
            if self.mode == Mode::Test {
                self.report_test(call);
                let notice = format!(
                    "Equip profile test finished: {} item(s).",
                    self.to_equip.len()
                );
                self.stop(call, Some(notice));
                return false;
            }
            self.phase = Phase::Dequipping;
            self.since_progress = 0.0;
        }
        if self.phase == Phase::Dequipping {
            if is_busy(call) {
                return true;
            }
            if self.dequipping != 0 {
                if is_equipped(call, self.dequipping) {
                    return true;
                }
                self.dequipping = 0;
                self.since_progress = 0.0;
            }
            if let Some(worn) = self.next_to_dequip(call) {
                let player = call.host.character().map_or(0, |character| character.id);
                let moved = call.host.move_item(worn, player, 0, 0, false);
                if moved.status == ItemStatus::Started {
                    self.dequipping = worn;
                } else {
                    let reason = format!(
                        "Equip profile stopped: could not take off {} ({:?}).",
                        name(call, worn),
                        moved.status
                    );
                    self.say_error(call, &reason);
                    self.stop(call, Some(reason));
                }
                return self.is_running();
            }
            self.phase = Phase::Equipping;
            self.since_progress = 0.0;
        }
        self.tick_equip(call);
        self.is_running()
    }

    /// Puts down a run without a word, and forgets an overwrite offer.
    pub fn reset(&mut self, locks: &mut ActionLockTable) {
        if self.is_running() {
            self.clear_run(locks);
            "Equip profile idle.".clone_into(&mut self.status);
        }
        self.observations.clear();
        self.pending_overwrite_key.clear();
        self.pending_overwrite_at = f64::NEG_INFINITY;
    }

    // ---- the run --------------------------------------------------------

    fn tick_identify(&mut self, call: &mut UbCall<'_>, step: f64) {
        self.since_identify_request += step;
        if self.awaiting_identify != 0 {
            self.awaiting_identify_seconds += step;
            if has_appraisal_data(call, self.awaiting_identify) {
                self.awaiting_identify = 0;
                self.awaiting_identify_seconds = 0.0;
                self.since_progress = 0.0;
                return;
            }
            if self.awaiting_identify_seconds < IDENTIFY_TIMEOUT_SECONDS {
                return;
            }
            *self
                .identify_attempts
                .entry(self.awaiting_identify)
                .or_default() += 1;
            self.awaiting_identify = 0;
            self.awaiting_identify_seconds = 0.0;
        }
        if self.since_identify_request < IDENTIFY_INTERVAL_SECONDS {
            return;
        }
        let Some(next) = self.next_to_identify(call) else {
            return;
        };
        self.since_identify_request = 0.0;
        if self.run_seconds - self.last_identify_notice >= IDENTIFY_NOTICE_SECONDS {
            self.last_identify_notice = self.run_seconds;
            let candidates = self.candidates.clone();
            let remaining = candidates
                .iter()
                .filter(|item| {
                    !has_appraisal_data(call, item.object_id)
                        && self.attempts(item.object_id) < MAXIMUM_IDENTIFY_ATTEMPTS_PER_ITEM
                        && self.needs_identify(call, item)
                })
                .count();
            call.write_ub(&chat::tool(
                tools::EQUIPMENT_MANAGER,
                &format!(
                    "waiting to id {remaining} items, this will take approximately {remaining} seconds."
                ),
            ));
        }
        let asked = call.host.identify(next);
        if asked.status == ItemStatus::Started {
            self.awaiting_identify = next;
            self.awaiting_identify_seconds = 0.0;
        } else {
            *self.identify_attempts.entry(next).or_default() += 1;
        }
    }

    /// The first candidate the rules cannot decide without an appraisal not
    /// yet answered, with attempts left.
    fn next_to_identify(&self, call: &mut UbCall<'_>) -> Option<u32> {
        self.candidates
            .iter()
            .find(|item| {
                !has_appraisal_data(call, item.object_id)
                    && self.attempts(item.object_id) < MAXIMUM_IDENTIFY_ATTEMPTS_PER_ITEM
                    && self.needs_identify(call, item)
            })
            .map(|item| item.object_id)
    }

    fn needs_identify(&self, call: &mut UbCall<'_>, item: &InventoryItem) -> bool {
        let properties = properties_of(call, item.object_id);
        rules::needs_identify(item, &properties, &self.rules, Some(call.host.automation()))
    }

    /// The pieces the profile keeps, in the order they go on: by the lore
    /// they require, lowest first (a stable sort, so equal ones keep pack
    /// order).
    fn build_queue(&mut self, call: &mut UbCall<'_>) {
        let owned = call.host.owned_items();
        let mut chosen: Vec<(u32, i32)> = Vec::new();
        for item in &self.candidates {
            let properties = properties_of(call, item.object_id);
            let (decision, _) = rules::decide(
                item,
                &properties,
                &self.rules,
                &owned,
                None,
                Some(call.host.automation()),
            );
            if !decision.is_some_and(|decision| {
                matches!(decision.action, LootAction::Keep | LootAction::KeepUpTo)
            }) {
                continue;
            }
            let lore = properties.int(LORE_REQUIREMENT_PROPERTY).unwrap_or(0);
            chosen.push((item.object_id, lore));
        }
        chosen.sort_by_key(|(_, lore)| *lore);
        self.to_equip.clear();
        self.profile_items.clear();
        for (object_id, _) in chosen {
            self.to_equip.push_back(object_id);
            self.profile_items.insert(object_id);
        }
    }

    fn report_test(&self, call: &mut UbCall<'_>) {
        call.write_ub("Will attempt to equip the following items in order:");
        for object_id in &self.to_equip {
            let line = format!(" * {} <{object_id}>", name(call, *object_id));
            call.write_ub(&line);
        }
    }

    fn next_to_dequip(&self, call: &mut UbCall<'_>) -> Option<u32> {
        call.host
            .owned_equipment()
            .into_iter()
            .find(|item| {
                item.equipped_location != 0 && !self.profile_items.contains(&item.object_id)
            })
            .map(|item| item.object_id)
    }

    fn tick_equip(&mut self, call: &mut UbCall<'_>) {
        if is_busy(call) {
            return;
        }
        while let Some(&head) = self.to_equip.front() {
            let Some(item) = find_owned(call, head) else {
                self.say_error(
                    call,
                    &format!("Could not find item with id: {head} - SKIPPING"),
                );
                self.to_equip.pop_front();
                self.since_progress = 0.0;
                continue;
            };
            if item.equipped_location != 0 {
                self.to_equip.pop_front();
                self.since_progress = 0.0;
                if self.equipping == head {
                    self.equipping = 0;
                }
                continue;
            }
            if self.equipping == head {
                // Sent and not yet seen in its slot: wait for the receipt.
                return;
            }
            self.equipping = head;
            self.equip_attempts = 0;
            self.equip_attempts += 1;
            if self.equip_attempts > MAXIMUM_EQUIP_ATTEMPTS_PER_ITEM {
                call.write_ub_tool_debug(
                    tools::EQUIPMENT_MANAGER,
                    &format!("Too many equip attempts ({}) - SKIPPING", item.name),
                );
                self.to_equip.pop_front();
                self.equipping = 0;
                self.since_progress = 0.0;
                continue;
            }
            // No slot request: the client picks the slot, as the reference
            // tool does, so the second of a pair (two rings) takes the free
            // one rather than being refused.
            let sent = call.host.equip(head, 0);
            match sent.status {
                EquipmentStatus::Started | EquipmentStatus::Busy => return,
                EquipmentStatus::AlreadyEquipped => {
                    self.to_equip.pop_front();
                    self.equipping = 0;
                    self.since_progress = 0.0;
                }
                status => {
                    // Refused, unknown to the client or unavailable: the
                    // piece is skipped rather than asked for again.
                    self.equipping = 0;
                    let notice = match sent.notice.as_deref() {
                        Some(notice) if !notice.is_empty() => format!(": {notice}"),
                        _ => String::new(),
                    };
                    self.say_error(
                        call,
                        &format!(
                            "Equip profile: {} was not accepted ({status:?}{notice}); skipping.",
                            item.name
                        ),
                    );
                    self.to_equip.pop_front();
                    self.since_progress = 0.0;
                }
            }
        }
        let notice = format!(
            "Finished equipping items in {}s",
            number::format_f64(self.run_seconds, "0.0").expect("a number format the kit prints")
        );
        self.stop(call, Some(notice));
    }

    /// The client's placement receipts, read on the frame they arrive: a
    /// piece seen in its slot advances the equip queue, a piece seen leaving
    /// finishes the dequip in flight.
    fn drain_observations(&mut self) {
        while let Some(observation) = self.observations.pop_front() {
            if observation.is_removal {
                if observation.object_id == self.dequipping {
                    self.dequipping = 0;
                    self.since_progress = 0.0;
                }
                continue;
            }
            if self.to_equip.front() == Some(&observation.object_id) {
                self.to_equip.pop_front();
                self.equipping = 0;
                self.since_progress = 0.0;
            }
        }
    }

    /// Clears the run and lets go of the locks; whether it was a load.
    fn clear_run(&mut self, locks: &mut ActionLockTable) -> bool {
        let was_load = self.mode == Mode::Load && self.phase != Phase::Idle;
        self.phase = Phase::Idle;
        self.to_equip.clear();
        self.profile_items.clear();
        self.candidates.clear();
        self.awaiting_identify = 0;
        self.dequipping = 0;
        self.equipping = 0;
        locks.release(ActionLockKind::ItemUse);
        locks.release(ActionLockKind::Navigation);
        was_load
    }

    /// Ends a run. Every end of a load says "Equipment Manager: Finished
    /// equipping items in <n>s", as a think when EquipmentManager.Think is
    /// on, as the reference's does; a dry run ends in silence.
    fn stop(&mut self, call: &mut UbCall<'_>, notice: Option<String>) {
        let was_load = self.clear_run(call.locks);
        let Some(notice) = notice else {
            "Equip profile idle.".clone_into(&mut self.status);
            return;
        };
        self.status = notice;
        if was_load {
            // The seconds as the reference writes a double: every digit
            // that counts, no fixed decimal places ("2s", "2.35s").
            let line = format!(
                "Equipment Manager: Finished equipping items in {}s",
                number::format_f64(self.run_seconds, "G15")
                    .expect("a number format the kit prints")
            );
            if call.flag(THINK_SETTING) {
                call.think(&line);
            } else {
                call.write_ub(&line);
            }
        }
    }

    // ---- create and list ------------------------------------------------

    /// Writes what is worn as a profile in the character's own folder: one
    /// keep rule per piece, matched by exact name, the slots it fits, its
    /// value and, when it has one, its material. An existing file is not
    /// written over until the same command is given again within half a
    /// minute.
    fn create(
        &mut self,
        call: &mut UbCall<'_>,
        profile_name: &str,
    ) -> Result<String, StorageError> {
        if call.host.character().is_none() {
            return Ok(chat::tool_error(
                tools::EQUIPMENT_MANAGER,
                "Equip profile create refused: not in the world.",
            ));
        }
        if !call.host.profile_storage().is_available() {
            return Ok(chat::tool_error(
                tools::EQUIPMENT_MANAGER,
                "Equip profile create refused: storage is unavailable.",
            ));
        }
        let file_name = file_name(call, profile_name);
        let key = call
            .services
            .ub_store()
            .character_profile_key(PROFILE_FOLDER, &file_name);
        let exists = call.host.profile_storage().read_text(&key)?.is_some();
        let confirmed = exists
            && key == self.pending_overwrite_key
            && self.now - self.pending_overwrite_at <= OVERWRITE_CONFIRMATION_SECONDS;
        if exists && !confirmed {
            self.pending_overwrite_key.clone_from(&key);
            self.pending_overwrite_at = self.now;
            return Ok(chat::tool(
                tools::EQUIPMENT_MANAGER,
                &format!(
                    "Equip profile '{file_name}' already exists at {key}; give the same \
                     command again within {OVERWRITE_CONFIRMATION_SECONDS:.0} seconds to overwrite it."
                ),
            ));
        }
        self.pending_overwrite_key.clear();
        self.pending_overwrite_at = f64::NEG_INFINITY;

        let mut rules = Vec::new();
        for item in call.host.owned_items() {
            if item.equipped_location == 0 || item.valid_locations == 0 {
                continue;
            }
            let mut requirements = vec![
                requirement(
                    STRING_MATCH_REQUIREMENT,
                    &format!("^{}$", regex::escape(&item.name)),
                    PROFILE_NAME_KEY,
                ),
                requirement(
                    INT_EQUALS_REQUIREMENT,
                    &item.valid_locations.to_string(),
                    PROFILE_EQUIPABLE_SLOTS_KEY,
                ),
                requirement(
                    INT_EQUALS_REQUIREMENT,
                    &item.value.to_string(),
                    VALUE_PROPERTY,
                ),
            ];
            if item.material_type != 0 {
                requirements.push(requirement(
                    INT_EQUALS_REQUIREMENT,
                    &item.material_type.to_string(),
                    MATERIAL_PROPERTY,
                ));
            }
            let mut rule = LootRule::default();
            rule.name.clone_from(&item.name);
            rule.action = LootAction::Keep;
            rule.has_imported_requirements = true;
            rule.vtank_requirements = requirements;
            rules.push(rule);
        }
        call.host
            .profile_storage()
            .write_text(&key, &utl::serialize_rules(&rules))?;
        Ok(chat::line(&format!("Profile created at {key}")))
    }

    /// Every equipment profile in the folders a load looks in, each line a
    /// message of its own under the tag, as the reference prints them.
    fn list(call: &mut UbCall<'_>, pattern: &str) -> Result<Vec<String>, StorageError> {
        let mut lines = vec![chat::line("Equip Profiles:")];
        if !call.host.profile_storage().is_available() {
            return Ok(lines);
        }
        let wanted = pattern.trim();
        let prefixes = call
            .services
            .ub_store()
            .profile_folder_prefixes(PROFILE_FOLDER);
        for prefix in prefixes {
            for key in call.host.profile_storage().list(&prefix)? {
                let file = key.rsplit('/').next().unwrap_or(&key);
                if !ordinal::ends_with_ignore_case(file, PROFILE_EXTENSION) {
                    continue;
                }
                if !wanted.is_empty() && !ordinal::contains_ignore_case(file, wanted) {
                    continue;
                }
                let folder = &key[..key.len() - file.len() - 1];
                lines.push(chat::line(&format!(" * {file} ({folder})")));
            }
        }
        Ok(lines)
    }

    // ---- helpers ----------------------------------------------------------

    fn attempts(&self, object_id: u32) -> i32 {
        self.identify_attempts
            .get(&object_id)
            .copied()
            .unwrap_or_default()
    }

    fn say_error(&self, call: &mut UbCall<'_>, text: &str) {
        call.post_ub(&chat::tool_error(tools::EQUIPMENT_MANAGER, text));
    }
}

/// The profile's file name: as given less its extension, or the character's
/// name when none is given, with the extension.
fn file_name(call: &mut UbCall<'_>, profile_name: &str) -> String {
    let mut requested = profile_name.trim();
    if ordinal::ends_with_ignore_case(requested, PROFILE_EXTENSION) {
        requested = &requested[..requested.len() - PROFILE_EXTENSION.len()];
    }
    let requested = if requested.is_empty() {
        call.host
            .character()
            .map(|character| character.name)
            .unwrap_or_default()
    } else {
        requested.to_owned()
    };
    requested + PROFILE_EXTENSION
}

/// What can be worn or wielded at all: the classes the reference tool
/// dresses with, and only an item that names a slot it fits.
fn equippable_items(call: &mut UbCall<'_>) -> Vec<InventoryItem> {
    call.host
        .owned_items()
        .into_iter()
        .filter(|item| {
            item.valid_locations != 0
                && matches!(
                    item.object_class,
                    ObjectClass::Armor
                        | ObjectClass::Clothing
                        | ObjectClass::Gem
                        | ObjectClass::Jewelry
                        | ObjectClass::MeleeWeapon
                        | ObjectClass::MissileWeapon
                        | ObjectClass::WandStaffOrb
                )
        })
        .collect()
}

fn is_busy(call: &mut UbCall<'_>) -> bool {
    call.host.item_state().is_busy || call.host.equipment_busy()
}

fn find_owned(call: &mut UbCall<'_>, object_id: u32) -> Option<InventoryItem> {
    call.host
        .owned_items()
        .into_iter()
        .find(|item| item.object_id == object_id)
}

fn is_equipped(call: &mut UbCall<'_>, object_id: u32) -> bool {
    find_owned(call, object_id).is_some_and(|item| item.equipped_location != 0)
}

fn has_appraisal_data(call: &mut UbCall<'_>, object_id: u32) -> bool {
    call.host
        .object(object_id)
        .is_some_and(|known| known.has_appraisal_data)
}

fn properties_of(call: &mut UbCall<'_>, object_id: u32) -> ItemProperties {
    call.host.properties(object_id).unwrap_or_default()
}

fn name(call: &mut UbCall<'_>, object_id: u32) -> String {
    find_owned(call, object_id).map_or_else(|| object_id.to_string(), |item| item.name)
}

fn requirement(kind: i32, value: &str, key: u32) -> VtankLootRequirement {
    VtankLootRequirement {
        kind,
        payload: format!("{value}\r\n{key}\r\n"),
    }
}

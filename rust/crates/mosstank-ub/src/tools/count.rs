//! `/ub count` (`InventoryCountController` and `HandleCountCommand`): counts
//! what the character carries and who stands nearby. Three questions, each
//! independent: a name pattern over the packs, summed per distinct name; a
//! loot profile over the packs, tallied by rule and by name, which waits
//! whenever an item's decision still turns on an appraisal; and the players
//! standing within a distance. Only the profile question takes time, and
//! only it ever holds the character.

use std::collections::{HashMap, HashSet};

use ac_plugin_abi::items::{InventoryItem, ItemStatus};
use ac_plugin_abi::world::{ItemProperties, ObjectClass};
use mosstank_dotnet::text::{casing, ordinal};
use mosstank_formats::utl::{LootAction, LootRule};
use mosstank_meta::host::MetaHost;
use openac_plugin_compat::number::{self, NumberStyles};
use openac_plugin_compat::regex::{DotNetRegex, RegexOptions};

use crate::chat::{self, tools};
use crate::commands::{UbCall, split_head, strip_extension};

const IDLE_STATUS: &str = "Item counter idle.";
/// How often a waiting count says how much is left.
const PROGRESS_INTERVAL_SECONDS: f64 = 10.0;
/// One appraisal request every 499 ms, the corpse identifier's cadence.
const IDENTIFY_REQUEST_INTERVAL_SECONDS: f64 = 0.499;
/// How long one request waits before it counts as unanswered.
const IDENTIFY_TIMEOUT_SECONDS: f64 = 10.0;
/// Failed requests one item gets before it is decided from what the client
/// already holds.
const MAXIMUM_IDENTIFY_ATTEMPTS_PER_ITEM: i32 = 3;
/// The outside limit on one count, three minutes.
const COUNT_DEADLINE_SECONDS: f64 = 180.0;

/// One counted name and how many the count found under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryCountEntry {
    /// The name.
    pub name: String,
    /// How many.
    pub count: i32,
}

/// What a finished count found (`InventoryCountTally`): one entry per
/// distinct item name, one per rule that decided one of them, and the sum.
/// Both tallies sum stack sizes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InventoryCountTally {
    /// Per name, in name order.
    pub by_name: Vec<InventoryCountEntry>,
    /// Per rule, in name order; empty for a name count.
    pub by_rule: Vec<InventoryCountEntry>,
    /// The sum.
    pub total: i32,
}

#[derive(Debug, Clone)]
struct CountedItem {
    rule_name: String,
    item_name: String,
    count: i32,
}

/// The counter.
#[derive(Debug, Clone)]
pub struct InventoryCountController {
    rules: Vec<LootRule>,
    counted: Vec<(u32, CountedItem)>,
    outstanding: HashSet<u32>,
    identify_attempts: HashMap<u32, i32>,
    profile_name: String,
    reported_missing_profile: Option<String>,
    foreground: bool,
    deferred: bool,
    awaiting_object_id: u32,
    awaiting_seconds: f64,
    since_request: f64,
    since_progress: f64,
    since_start: f64,
    is_running: bool,
    status: String,
    last_profile_tally: Option<InventoryCountTally>,
}

impl Default for InventoryCountController {
    fn default() -> Self {
        Self::new()
    }
}

impl InventoryCountController {
    /// Idle.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            counted: Vec::new(),
            outstanding: HashSet::new(),
            identify_attempts: HashMap::new(),
            profile_name: String::new(),
            reported_missing_profile: None,
            foreground: false,
            deferred: false,
            awaiting_object_id: 0,
            awaiting_seconds: 0.0,
            since_request: 0.0,
            since_progress: 0.0,
            since_start: 0.0,
            is_running: false,
            status: IDLE_STATUS.to_owned(),
            last_profile_tally: None,
        }
    }

    /// Whether a profile count is still waiting on appraisals.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// What the counter is doing or last did.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// The answer the last profile count arrived at.
    #[must_use]
    pub fn last_profile_tally(&self) -> Option<&InventoryCountTally> {
        self.last_profile_tally.as_ref()
    }

    // ---- the command ----------------------------------------------------

    /// `/ub count item <pattern>`, `profile <lootprofile>`, `player <range>`
    /// and `stop` (`HandleCountCommand`).
    pub fn command(&mut self, call: &mut UbCall<'_>, arguments: &str) {
        let (mode, subject) = split_head(arguments);
        match casing::to_lower_invariant(&mode).as_str() {
            "item" => self.report_name_count(call, &subject),
            "profile" => {
                // The reference says the profile's name first, then only its
                // own refusal when the file cannot be had.
                if self.is_running {
                    call.post_ub(&chat::tool_error(
                        tools::COUNTER,
                        "Counter already running.",
                    ));
                    return;
                }
                call.write_ub(subject.trim());
                let name = strip_extension(&subject, &[".utl", ".json"]);
                if !self.try_start_profile(call, &name, true) {
                    call.post_ub(&chat::tool(
                        tools::COUNTER,
                        &format!("Profile does not exist: {}", subject.trim()),
                    ));
                }
            }
            "player" => {
                let range = number::parse_f64(&subject, NumberStyles::FLOAT)
                    .filter(|range| range.is_finite() && *range > 0.0);
                let Some(range) = range else {
                    call.post_ub(&chat::tool_error(
                        tools::COUNTER,
                        &format!("bad player count range: {subject}"),
                    ));
                    return;
                };
                Self::report_player_count(call, range);
            }
            "stop" => {
                // A profile count holds the character while it waits, so it
                // needs a way back that is not a relog.
                let line = self.cancel();
                call.post_ub(&chat::tool(tools::COUNTER, &line));
            }
            _ => call.write_ub_bad_syntax("count"),
        }
    }

    // ---- a name pattern over the packs ------------------------------------

    /// Sums every owned item whose name the regular expression `pattern`
    /// matches, per distinct name; worn and wielded gear included. The
    /// error when the pattern is unusable.
    ///
    /// # Errors
    /// No pattern, or one that does not compile or match.
    pub fn try_count_by_name(
        host: &mut dyn MetaHost,
        pattern: &str,
    ) -> Result<InventoryCountTally, String> {
        let text = pattern.trim();
        if text.is_empty() {
            return Err("no name pattern given.".to_owned());
        }
        let matcher = DotNetRegex::new(
            text,
            RegexOptions::IGNORE_CASE | RegexOptions::CULTURE_INVARIANT,
        )
        .map_err(|failure| format!("bad name pattern: {failure}"))?;

        let mut by_name = Vec::new();
        let mut total = 0;
        for item in host.owned_items_shared().iter() {
            if !matcher
                .is_match(&item.name)
                .map_err(|failure| format!("bad name pattern: {failure}"))?
            {
                continue;
            }
            let size = item.stack_size.max(1);
            add(&mut by_name, &item.name, size);
            total += size;
        }
        Ok(InventoryCountTally {
            by_name: sorted(by_name),
            by_rule: Vec::new(),
            total,
        })
    }

    /// Counts by name and thinks the answer.
    pub fn report_name_count(&self, call: &mut UbCall<'_>, pattern: &str) {
        match Self::try_count_by_name(call.host, pattern) {
            Ok(tally) => report_tally(call, &tally, pattern.trim()),
            Err(error) => say_error(call, &format!("Item Count: {error}")),
        }
    }

    // ---- the players standing nearby --------------------------------------

    /// How many players stand within `range_meters`, flat along the ground;
    /// the character's own object is one of them, as the reference counts.
    pub fn count_players_within(host: &mut dyn MetaHost, range_meters: f64) -> i32 {
        if !range_meters.is_finite() || range_meters <= 0.0 {
            return 0;
        }
        let player = host.navigation();
        if !player.is_available {
            return 0;
        }
        let count = host
            .objects()
            .iter()
            .filter(|candidate| {
                candidate.object_class == ObjectClass::Player
                    && candidate.is_landscape
                    && candidate.position.as_ref().is_some_and(|position| {
                        position.horizontal_distance_metres(&player.position) < range_meters
                    })
            })
            .count();
        i32::try_from(count).unwrap_or(i32::MAX)
    }

    /// Counts players in range and thinks the answer.
    pub fn report_player_count(call: &mut UbCall<'_>, range_meters: f64) {
        let count = Self::count_players_within(call.host, range_meters);
        think(call, &format!("Player Count: {count}"));
    }

    // ---- a loot profile over the packs ------------------------------------

    /// Begins a profile count; it finishes here when every item can already
    /// be decided. A `foreground` count (asked for by command) holds the
    /// character while it waits and prints its progress and answer; a count
    /// a macro asked for does neither. True when it started (or finished).
    pub fn try_start_profile(
        &mut self,
        call: &mut UbCall<'_>,
        profile_name: &str,
        foreground: bool,
    ) -> bool {
        if self.is_running {
            "Item counter already running.".clone_into(&mut self.status);
            return false;
        }
        let requested = profile_name.trim().to_owned();
        self.rules.clear();
        let Some(rules) = call.services.loot_profile_rules(call.host, &requested) else {
            self.status = format!("Item counter profile not found: {requested}.");
            return false;
        };
        self.rules = rules;

        self.counted.clear();
        self.outstanding.clear();
        self.identify_attempts.clear();
        self.awaiting_object_id = 0;
        self.awaiting_seconds = 0.0;
        // Armed, so the first request can go out on the first frame.
        self.since_request = IDENTIFY_REQUEST_INTERVAL_SECONDS;
        self.since_progress = 0.0;
        self.since_start = 0.0;
        self.deferred = false;
        self.reported_missing_profile = None;
        self.profile_name = requested;
        self.foreground = foreground;
        self.last_profile_tally = None;
        self.is_running = true;

        self.scan(call);
        if self.outstanding.is_empty() {
            self.complete(call);
            return true;
        }
        self.status = format!(
            "Counting {}: {} item(s) to identify.",
            self.profile_name,
            self.outstanding.len()
        );
        if self.foreground {
            let line = format!("Items remaining to ID: {}", self.outstanding.len());
            say(call, &line);
        }
        true
    }

    /// The number an expression reads (`vtcountprofile`): the profile's
    /// total once every appraisal has landed, -1 while there is no answer.
    pub fn profile_count(&mut self, call: &mut UbCall<'_>, profile_name: &str) -> f64 {
        let requested = profile_name.trim().to_owned();
        // A count under way answers when it finishes, whoever asked.
        if self.is_running {
            return -1.0;
        }
        if !self.try_start_profile(call, &requested, false) {
            self.report_missing_profile_once(call, &requested);
            return -1.0;
        }
        match (&self.last_profile_tally, self.is_running) {
            (Some(tally), false) => f64::from(tally.total),
            _ => -1.0,
        }
    }

    /// Drives a waiting profile count: one request at a time, never while
    /// the looter uses the appraisal slot, and a progress line every ten
    /// seconds. True while a foreground count owns the character.
    pub fn tick(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64, can_act: bool) -> bool {
        if !self.is_running {
            return false;
        }
        if call.host.character().is_none() {
            self.stop("Item counter stopped: the session went away.");
            return false;
        }
        let elapsed = elapsed_seconds.max(0.0);
        self.since_request += elapsed;
        self.since_progress += elapsed;
        self.since_start += elapsed;
        self.observe_appraisal(call, elapsed);
        self.scan(call);
        if self.outstanding.is_empty() {
            self.complete(call);
            return false;
        }
        if self.since_start >= COUNT_DEADLINE_SECONDS {
            self.give_up(call);
            return false;
        }
        self.status = format!(
            "Counting {}: {} item(s) to identify.",
            self.profile_name,
            self.outstanding.len()
        );
        self.report_progress(call);
        if can_act {
            self.request_next_appraisal(call);
        }
        self.foreground
    }

    /// Puts down whatever a count had in flight.
    pub fn reset(&mut self) {
        self.stop(IDLE_STATUS);
    }

    /// Cancels a count and says what was cancelled: the line to show.
    pub fn cancel(&mut self) -> String {
        if !self.is_running {
            self.stop(IDLE_STATUS);
            return "Item counter is not running.".to_owned();
        }
        let cancelled = self.profile_name.clone();
        self.stop(&format!("Item counter stopped: {cancelled}."));
        self.status.clone()
    }

    // ---- the count itself ---------------------------------------------------

    /// One pass over the packs: every item that can be decided now is
    /// decided and counted once; everything whose decision still turns on an
    /// appraisal is left outstanding.
    fn scan(&mut self, call: &mut UbCall<'_>) {
        self.outstanding.clear();
        for item in call.host.owned_items_shared().iter() {
            // Worn and wielded gear is in use, not carried.
            if item.equipped_location != 0 || item.wielder_id != 0 {
                continue;
            }
            if self.counted.iter().any(|(id, _)| *id == item.object_id) {
                continue;
            }
            // An item the client's object table has no record of can be
            // neither appraised nor decided, so it is passed over.
            let Some(known) = call.host.object(item.object_id) else {
                continue;
            };
            let properties = call.host.properties(item.object_id).unwrap_or_default();
            if self.needs_appraisal(call, item, &properties, known.has_appraisal_data) {
                self.outstanding.insert(item.object_id);
                continue;
            }
            let Some(rule_name) = self.try_decide_keep(call, item, &properties) else {
                continue;
            };
            self.counted.push((
                item.object_id,
                CountedItem {
                    rule_name,
                    item_name: item.name.clone(),
                    count: item.stack_size.max(1),
                },
            ));
        }
    }

    /// True when an appraisal could still change which rule decides the
    /// item, it has attempts left, and the client holds none for it yet.
    fn needs_appraisal(
        &self,
        call: &mut UbCall<'_>,
        item: &InventoryItem,
        properties: &ItemProperties,
        has_appraisal_data: bool,
    ) -> bool {
        if !call.host.loot_needs_identify(item, properties, &self.rules) {
            return false;
        }
        if self.attempts_spent(item.object_id) >= MAXIMUM_IDENTIFY_ATTEMPTS_PER_ITEM {
            return false;
        }
        !has_appraisal_data
    }

    /// The first rule that matches decides; the item counts when it says
    /// keep or keep-up-to (whose ceiling is deliberately not applied: it is
    /// measured against the very inventory being counted). The rule's name.
    fn try_decide_keep(
        &self,
        call: &mut UbCall<'_>,
        item: &InventoryItem,
        properties: &ItemProperties,
    ) -> Option<String> {
        for (index, rule) in self.rules.iter().enumerate() {
            if !call.host.loot_rule_matches(rule, item, properties) {
                continue;
            }
            let name = if casing::is_white_space_only(&rule.name) {
                format!("Rule {}", index + 1)
            } else {
                rule.name.clone()
            };
            return matches!(rule.action, LootAction::Keep | LootAction::KeepUpTo).then_some(name);
        }
        None
    }

    /// Watches the request in flight: an answer frees the slot; silence past
    /// the timeout, or a question the client has given up on, spends one of
    /// the item's attempts.
    fn observe_appraisal(&mut self, call: &mut UbCall<'_>, elapsed: f64) {
        if self.awaiting_object_id == 0 {
            return;
        }
        self.awaiting_seconds += elapsed;
        if call
            .host
            .object(self.awaiting_object_id)
            .is_some_and(|known| known.has_appraisal_data)
        {
            self.awaiting_object_id = 0;
            self.awaiting_seconds = 0.0;
            return;
        }
        let abandoned = call.host.appraisal().last_abandoned_id == self.awaiting_object_id;
        if !abandoned && self.awaiting_seconds < IDENTIFY_TIMEOUT_SECONDS {
            return;
        }
        self.spend_attempt(self.awaiting_object_id);
        self.awaiting_object_id = 0;
        self.awaiting_seconds = 0.0;
    }

    fn attempts_spent(&self, object_id: u32) -> i32 {
        self.identify_attempts
            .get(&object_id)
            .copied()
            .unwrap_or_default()
    }

    fn spend_attempt(&mut self, object_id: u32) {
        *self.identify_attempts.entry(object_id).or_default() += 1;
    }

    /// Asks about the next outstanding item when the appraisal slot is free
    /// to ask with: the looter and this count share it, and the answer
    /// belongs to whoever asked last.
    fn request_next_appraisal(&mut self, call: &mut UbCall<'_>) {
        if self.awaiting_object_id != 0 {
            return;
        }
        if call.services.corpse_identifier_busy() || call.host.appraisal().awaiting_id != 0 {
            self.deferred = true;
            return;
        }
        self.deferred = false;
        if self.since_request < IDENTIFY_REQUEST_INTERVAL_SECONDS {
            return;
        }
        let mut order: Vec<u32> = self.outstanding.iter().copied().collect();
        order.sort_unstable();
        for object_id in order {
            if self.attempts_spent(object_id) >= MAXIMUM_IDENTIFY_ATTEMPTS_PER_ITEM {
                continue;
            }
            let result = call.host.identify(object_id);
            if result.accepted() {
                self.awaiting_object_id = object_id;
                self.awaiting_seconds = 0.0;
                self.since_request = 0.0;
                return;
            }
            // Busy is the client's own backpressure and says nothing about
            // this item; anything else is a refusal of this item and costs
            // it one attempt.
            if result.status == ItemStatus::Busy {
                return;
            }
            self.spend_attempt(object_id);
        }
    }

    fn report_progress(&mut self, call: &mut UbCall<'_>) {
        if !self.foreground || self.since_progress < PROGRESS_INTERVAL_SECONDS {
            return;
        }
        self.since_progress = 0.0;
        let line = if self.deferred {
            format!(
                "Items remaining to ID: {} (waiting for the looter)",
                self.outstanding.len()
            )
        } else {
            format!("Items remaining to ID: {}", self.outstanding.len())
        };
        say(call, &line);
    }

    fn complete(&mut self, call: &mut UbCall<'_>) {
        let tally = self.build_tally();
        self.is_running = false;
        self.awaiting_object_id = 0;
        self.awaiting_seconds = 0.0;
        self.deferred = false;
        self.status = format!("Counted {}: {}.", self.profile_name, tally.total);
        if self.foreground {
            say(call, "Finished IDing Items");
            report_tally(call, &tally, &self.profile_name);
        }
        self.last_profile_tally = Some(tally);
        self.foreground = false;
    }

    /// Ends a count past its deadline: the character comes back and no
    /// answer is given rather than a short one.
    fn give_up(&mut self, call: &mut UbCall<'_>) {
        let remaining = self.outstanding.len();
        let name = self.profile_name.clone();
        let foreground = self.foreground;
        self.stop(&format!(
            "Item counter gave up on {name}: {remaining} item(s) never identified."
        ));
        if foreground {
            let status = self.status.clone();
            say_error(call, &status);
        }
    }

    fn build_tally(&self) -> InventoryCountTally {
        let mut by_name = Vec::new();
        let mut by_rule = Vec::new();
        let mut total = 0;
        for (_, counted) in &self.counted {
            add(&mut by_name, &counted.item_name, counted.count);
            add(&mut by_rule, &counted.rule_name, counted.count);
            total += counted.count;
        }
        InventoryCountTally {
            by_name: sorted(by_name),
            by_rule: sorted(by_rule),
            total,
        }
    }

    /// A profile an expression names that cannot be read is said once per
    /// name.
    fn report_missing_profile_once(&mut self, call: &mut UbCall<'_>, profile_name: &str) {
        if self
            .reported_missing_profile
            .as_deref()
            .is_some_and(|reported| ordinal::equals_ignore_case(reported, profile_name))
        {
            return;
        }
        self.reported_missing_profile = Some(profile_name.to_owned());
        let status = self.status.clone();
        say_error(call, &status);
    }

    fn stop(&mut self, status: &str) {
        self.rules.clear();
        self.counted.clear();
        self.outstanding.clear();
        self.identify_attempts.clear();
        self.profile_name.clear();
        self.reported_missing_profile = None;
        self.foreground = false;
        self.deferred = false;
        self.awaiting_object_id = 0;
        self.awaiting_seconds = 0.0;
        self.since_request = 0.0;
        self.since_progress = 0.0;
        self.since_start = 0.0;
        self.is_running = false;
        self.last_profile_tally = None;
        status.clone_into(&mut self.status);
    }
}

/// The answer, thought as the reference thinks it: what each rule accounted
/// for, what each name did, and the sum; a count that found nothing still
/// says so against what was asked for.
fn report_tally(call: &mut UbCall<'_>, tally: &InventoryCountTally, subject: &str) {
    for entry in &tally.by_rule {
        think(
            call,
            &format!("Rule Count: {} - {}", entry.name, entry.count),
        );
    }
    if tally.by_name.is_empty() {
        think(call, &format!("Item Count: {subject} - 0"));
    }
    for entry in &tally.by_name {
        think(
            call,
            &format!("Item Count: {} - {}", entry.name, entry.count),
        );
    }
    think(call, &format!("Total Item Count: {}", tally.total));
}

/// "[UB] Counter: ...".
fn say(call: &mut UbCall<'_>, text: &str) {
    call.post_ub(&chat::tool(tools::COUNTER, text));
}

fn say_error(call: &mut UbCall<'_>, text: &str) {
    call.post_ub(&chat::tool_error(tools::COUNTER, text));
}

/// An answer, thought under the tool's name so a chat trigger can read it.
fn think(call: &mut UbCall<'_>, text: &str) {
    call.think(&format!("{}: {text}", tools::COUNTER));
}

/// Adds to a name's count, the first spelling of a name kept (a dictionary
/// that ignores case).
fn add(target: &mut Vec<InventoryCountEntry>, key: &str, count: i32) {
    match target
        .iter_mut()
        .find(|entry| ordinal::equals_ignore_case(&entry.name, key))
    {
        Some(entry) => entry.count += count,
        None => target.push(InventoryCountEntry {
            name: key.to_owned(),
            count,
        }),
    }
}

fn sorted(mut entries: Vec<InventoryCountEntry>) -> Vec<InventoryCountEntry> {
    entries.sort_by(|a, b| ordinal::compare_ignore_case(&a.name, &b.name));
    entries
}

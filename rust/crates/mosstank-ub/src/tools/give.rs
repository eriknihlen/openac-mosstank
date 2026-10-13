//! `/ub give` and `/ub ig` (`ProfileGiveController` and its commands in
//! `MossTankCommands`): hands items to another character one at a time,
//! waiting for each to leave the packs before asking for the next. What goes
//! in the queue is decided by a loot profile or by a typed name; everything
//! after that is the same for both: the same queue, the same wait on the
//! client's answer, the same retry ladder and the same ceilings.

use std::collections::{HashSet, VecDeque};

use ac_plugin_abi::items::{InventoryCommandKind, InventoryItem};
use ac_plugin_abi::world::{ItemProperties, ObjectClass, WorldObject};
use mosstank_dotnet::text::{casing, ordinal};
use mosstank_formats::utl::{LootAction, LootRule};
use mosstank_settings::model::InventorySettings;
use openac_plugin_compat::number::{self, NumberStyles};
use openac_plugin_compat::regex::{DotNetRegex, RegexOptions};

use crate::chat::{self, tools};
use crate::commands::{GiveNameMatch, UbCall, split_at_first, split_head, strip_extension};
use crate::search;

/// How long a run may go without moving on to its next item before it
/// bails: the whole run's clock, restarted when a new item is first asked
/// for.
pub const BAIL_SECONDS: f64 = 10.0;

/// The line printed when a give is missing its target.
const GIVE_USAGE: &str = "/ub give[p|P|r] [itemCount] <itemName> to <target>";
/// The line printed when a profile give is missing its target.
const PROFILE_GIVE_USAGE: &str = "/ub ig[p] <lootProfile> to <target>";

/// Who a hand-over may go to.
const TARGET_CLASSES: [ObjectClass; 2] = [ObjectClass::Player, ObjectClass::Npc];

/// The words that call a run off.
const STOP_WORDS: [&str; 4] = ["stop", "cancel", "quit", "abort"];

/// The row that turns the end line into a think.
const THINK_SETTING: &str = "ItemGiver.Think";

/// One item queued to be given: the whole item (amount 0) or so many out
/// of its stack (`PendingGive`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingGive {
    /// The item.
    pub object_id: u32,
    /// How many, or 0 for the whole item.
    pub amount: u32,
}

/// The item giver.
#[derive(Debug, Clone)]
pub struct ProfileGiveController {
    pending: VecDeque<PendingGive>,
    target_object_id: u32,
    waiting_object_id: u32,
    completion_revision: i64,
    since_give: f64,
    attempts: i32,
    given: i32,
    failed: i32,
    subject: String,
    target_name: String,
    typed_target: String,
    give_calls: i32,
    since_progress: f64,
    current_object_id: u32,
    run_seconds: f64,
    is_running: bool,
    status: String,
    refusal: String,
    finished_line: String,
    end_error: String,
}

impl Default for ProfileGiveController {
    fn default() -> Self {
        Self::new()
    }
}

impl ProfileGiveController {
    /// Idle.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            target_object_id: 0,
            waiting_object_id: 0,
            completion_revision: 0,
            since_give: f64::MAX,
            attempts: 0,
            given: 0,
            failed: 0,
            subject: String::new(),
            target_name: String::new(),
            typed_target: String::new(),
            give_calls: 0,
            since_progress: 0.0,
            current_object_id: 0,
            run_seconds: 0.0,
            is_running: false,
            status: "Item giver idle.".to_owned(),
            refusal: String::new(),
            finished_line: String::new(),
            end_error: String::new(),
        }
    }

    /// Whether a run is under way.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// What it last did, in words.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Why the last start was refused, in the reference's words where it
    /// has them ("player Bob not found").
    #[must_use]
    pub fn refusal(&self) -> &str {
        &self.refusal
    }

    /// The line the last run ended on: "ItemGiver finished: <what> to
    /// <who>. took <time> to give <n> item(s). <asks that gave nothing>".
    #[must_use]
    pub fn finished_line(&self) -> &str {
        &self.finished_line
    }

    /// The error the last run ended on ("ItemGiver bail, Timeout expired"),
    /// or empty.
    #[must_use]
    pub fn end_error(&self) -> &str {
        &self.end_error
    }

    // ---- the commands ---------------------------------------------------

    /// `/ub give[p|P|r] [count] <item> to <target>`, and `/ub give stop` (or
    /// cancel, quit, abort). The line is split at the first " to ", because
    /// the item name is the short half (`HandleGiveCommand`).
    pub fn give_command(
        &mut self,
        call: &mut UbCall<'_>,
        arguments: &str,
        matching: GiveNameMatch,
        partial_target: bool,
    ) {
        let text = arguments.trim();
        if is_stop_word(text) {
            self.stop_command(call);
            return;
        }
        let Some((item, target)) = split_at_first(text, " to ") else {
            call.write_ub_bad_syntax("give");
            return;
        };
        let mut item = item.trim().to_owned();
        let target = target.trim().to_owned();

        // A leading whole number is a count, but only when something is left
        // to be the item name: "/ub give 10 to Bob" gives an item called 10.
        let mut count = 0;
        let (head, rest) = split_head(&item);
        if !rest.is_empty()
            && let Some(parsed) = number::parse_i32(&head, NumberStyles::INTEGER)
            && parsed > 0
        {
            count = parsed;
            item = rest;
        }

        self.start_command(call, "give", |controller, call| {
            controller.try_start_by_name(call, &item, matching, count, &target, partial_target)
        });
    }

    /// `/ub ig[p] <lootProfile> to <target>`, and `/ub ig stop`
    /// (`HandleItemGiverCommand`).
    pub fn item_giver_command(
        &mut self,
        call: &mut UbCall<'_>,
        arguments: &str,
        partial_target: bool,
    ) {
        let text = arguments.trim();
        if is_stop_word(text) {
            self.stop_command(call);
            return;
        }
        // Split at the first " to " for the same reason the name give does.
        let Some((profile, target)) = split_at_first(text, " to ") else {
            call.write_ub_bad_syntax("ig");
            return;
        };
        let profile = strip_extension(profile.trim(), &[".utl", ".json"]);
        let target = target.trim().to_owned();
        self.start_command(call, "ig", |controller, call| {
            controller.try_start(call, &profile, &target, partial_target)
        });
    }

    /// Starts a hand-over, or says why not as the reference's inventory tool
    /// does: a second run while one is going, then whatever the start
    /// refused. A run that starts says nothing until it ends (`StartGive`).
    fn start_command(
        &mut self,
        call: &mut UbCall<'_>,
        stop_verb: &str,
        start: impl FnOnce(&mut Self, &mut UbCall<'_>) -> bool,
    ) {
        if self.is_running {
            call.post_ub(&chat::tool_error(
                tools::INVENTORY_MANAGER,
                &format!(
                    "Already running.  Please wait until it completes or use /ub {stop_verb} stop to quit previous session"
                ),
            ));
            return;
        }
        if start(self, call) {
            return;
        }
        let refusal = self.refusal.clone();
        call.post_ub(&chat::tool_error(tools::INVENTORY_MANAGER, &refusal));
    }

    /// A stop: the run's end line when one was going, the reference's error
    /// when none was (`StopGive`).
    fn stop_command(&mut self, call: &mut UbCall<'_>) {
        if self.stop_requested() {
            self.report_end(call);
        } else {
            call.write_ub_error("ItemGiver is not running.");
        }
    }

    /// Says how a hand-over ended, as a think when ItemGiver.Think is on and
    /// as a plain line otherwise; a run that bailed says so first, as the
    /// tool's error (`ReportGiveEnd`).
    fn report_end(&self, call: &mut UbCall<'_>) {
        if !self.end_error.is_empty() {
            call.post_ub(&chat::tool_error(tools::INVENTORY_MANAGER, &self.end_error));
        }
        if call.flag(THINK_SETTING) {
            call.think(&self.finished_line);
        } else {
            call.write_ub(&self.finished_line);
        }
    }

    // ---- starting -------------------------------------------------------

    /// Queues every item a loot profile says to keep: the shared item giver
    /// folder first, then a loot profile of that name. Whole items go.
    /// `partial_target` lets part of the target's name do.
    pub fn try_start(
        &mut self,
        call: &mut UbCall<'_>,
        profile_name: &str,
        target_name: &str,
        partial_target: bool,
    ) -> bool {
        if !self.can_start(call) {
            return false;
        }
        let requested = profile_name.trim().to_owned();
        let Some(target) =
            self.try_resolve_target(call, target_name, partial_target, PROFILE_GIVE_USAGE)
        else {
            return false;
        };

        let rules = call
            .services
            .item_giver_rules(call.host, &requested)
            .or_else(|| call.services.loot_profile_rules(call.host, &requested));
        let Some(rules) = rules else {
            self.status = format!("Item giver profile not found: {requested}.");
            self.refusal = format!("ItemGiver Profile does not exist: {requested}");
            return false;
        };

        let owned = call.host.owned_items();
        self.pending.clear();
        for item in givable(&owned) {
            let properties = call.host.properties(item.object_id).unwrap_or_default();
            if matches_give_profile(call, item, &properties, &rules) {
                self.pending.push_back(PendingGive {
                    object_id: item.object_id,
                    amount: 0,
                });
            }
        }

        // The reference names a profile hand-over by the file it loaded, so
        // the end line keeps the extension.
        let file = if ordinal::ends_with_ignore_case(&requested, ".utl") {
            requested
        } else {
            requested + ".utl"
        };
        self.begin(&target, &file, target_name);
        true
    }

    /// Queues every item whose name answers to `pattern` (the whole name,
    /// part of it, or a regular expression), `count` units in all counting a
    /// stack as its size (0 for everything); the last item is split when the
    /// count runs out part way through its stack.
    pub fn try_start_by_name(
        &mut self,
        call: &mut UbCall<'_>,
        pattern: &str,
        matching: GiveNameMatch,
        count: i32,
        target_name: &str,
        partial_target: bool,
    ) -> bool {
        if !self.can_start(call) {
            return false;
        }
        let text = pattern.trim();
        if text.is_empty() {
            "Item giver was given no item name.".clone_into(&mut self.status);
            self.refusal.clone_from(&self.status);
            return false;
        }

        let matcher = if matching == GiveNameMatch::Pattern {
            match DotNetRegex::new(
                text,
                RegexOptions::IGNORE_CASE | RegexOptions::CULTURE_INVARIANT,
            ) {
                Ok(regex) => Some(regex),
                Err(failure) => {
                    self.status = format!("Item giver pattern refused: {failure}");
                    self.refusal.clone_from(&self.status);
                    return false;
                }
            }
        } else {
            None
        };

        // The range is checked here, not per give: a target that walks off
        // mid-run is caught by the frame losing sight of it.
        let Some(target) = self.try_resolve_target(call, target_name, partial_target, GIVE_USAGE)
        else {
            return false;
        };

        let mut remaining = if count > 0 { count } else { i32::MAX };
        let owned = call.host.owned_items();
        let mut queued = VecDeque::new();
        for item in givable(&owned) {
            if remaining <= 0 {
                break;
            }
            let matched = match (matching, &matcher) {
                (GiveNameMatch::Pattern, Some(regex)) => match regex.is_match(&item.name) {
                    Ok(matched) => matched,
                    Err(failure) => {
                        self.status = format!("Item giver pattern refused: {failure}");
                        self.refusal.clone_from(&self.status);
                        return false;
                    }
                },
                (GiveNameMatch::Pattern, None) => false,
                (GiveNameMatch::Partial, _) => ordinal::contains_ignore_case(&item.name, text),
                (GiveNameMatch::Exact, _) => ordinal::equals_ignore_case(&item.name, text),
            };
            if !matched {
                continue;
            }
            let stack = item.stack_size.max(1);
            if stack <= remaining {
                // The whole item goes, so the give says nothing about amount.
                queued.push_back(PendingGive {
                    object_id: item.object_id,
                    amount: 0,
                });
                remaining -= stack;
                continue;
            }
            queued.push_back(PendingGive {
                object_id: item.object_id,
                amount: remaining.cast_unsigned(),
            });
            remaining = 0;
        }
        self.pending = queued;

        // The reference matches a name in lower case and names the run by
        // it; a pattern is kept as it was written.
        let subject = if matching == GiveNameMatch::Pattern {
            text.to_owned()
        } else {
            casing::to_lower_invariant(text)
        };
        self.begin(&target, &subject, target_name);
        true
    }

    /// Clears the last refusal, then says whether a start may go ahead: not
    /// while a run is going, and not outside the world.
    fn can_start(&mut self, call: &mut UbCall<'_>) -> bool {
        self.refusal.clear();
        if self.is_running {
            "Already running.".clone_into(&mut self.refusal);
            return false;
        }
        if call.host.character().is_none() {
            "Item giver refused: not in the world.".clone_into(&mut self.status);
            self.refusal.clone_from(&self.status);
            return false;
        }
        true
    }

    /// Stops a run on the player's say-so; false when nothing was running.
    pub fn stop_requested(&mut self) -> bool {
        if !self.is_running {
            "Item giver is not running.".clone_into(&mut self.status);
            return false;
        }
        let status = format!(
            "Item giver stopped: {} item(s) given to {}.",
            self.given, self.target_name
        );
        self.stop(&status);
        true
    }

    // ---- the run --------------------------------------------------------

    /// One frame; true while it owns the character. A run that ends here
    /// says how it ended (the panel's `ReportGiveEnd` after its tick).
    pub fn tick(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64, can_act: bool) -> bool {
        let was_running = self.is_running;
        let owns = self.step(call, elapsed_seconds, can_act);
        if was_running && !self.is_running {
            self.report_end(call);
        }
        owns
    }

    fn step(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64, can_act: bool) -> bool {
        if !self.is_running {
            return false;
        }
        let step = elapsed_seconds.max(0.0);
        self.since_progress += step;
        let (owns, check_bail) = self.tick_run(call, step, can_act);
        // The clock is looked at only on a step that waited: not inside the
        // pause between asks, and not on a step that asked.
        if !self.is_running || !check_bail || self.since_progress <= BAIL_SECONDS {
            return owns;
        }
        "ItemGiver bail, Timeout expired".clone_into(&mut self.end_error);
        self.stop(&format!(
            "Item giver bailed: no progress for {BAIL_SECONDS:.0} seconds."
        ));
        false
    }

    /// One step, in the reference's order: the pause since the last ask, the
    /// wait on that ask's answer, everything that left the packs counted as
    /// given, then the next ask. Whether it owns the character, and whether
    /// it waited (when the run's clock is looked at).
    fn tick_run(&mut self, call: &mut UbCall<'_>, step: f64, can_act: bool) -> (bool, bool) {
        self.run_seconds += step;
        let target = call.host.object(self.target_object_id);
        let present = call.host.character().is_some()
            && target.is_some_and(|target| TARGET_CLASSES.contains(&target.object_class));
        if !present {
            self.stop("Item giver stopped: target vanished.");
            return (false, false);
        }

        if self.since_give < 1e9 {
            self.since_give += step;
        }
        // A pause between asks, counted from the last ask; zero is none.
        let settings = call.services.inventory_settings();
        if self.since_give < settings.give_delay_seconds.max(0.0) {
            return (true, false);
        }

        if self.waiting_object_id != 0 {
            let completion = call.host.item_state().last_inventory_completion;
            let still_owned = call
                .host
                .owned_items()
                .iter()
                .any(|item| item.object_id == self.waiting_object_id);
            let answered = completion.revision > self.completion_revision
                && completion.kind == InventoryCommandKind::Give
                && completion.source_id == self.waiting_object_id;
            if still_owned && !answered {
                return (true, true);
            }
            self.waiting_object_id = 0;
            let succeeded = completion.revision != 0 && completion.weenie_error == 0;
            if !still_owned || succeeded {
                self.pending.pop_front();
                self.given += 1;
            }
            // An error leaves the item in the packs and at the head of the
            // queue, to be asked for again.
        }

        self.count_items_that_left(call);
        if self.pending.is_empty() {
            let status = format!(
                "Item giver finished: {} item(s) given to {}.",
                self.given, self.target_name
            );
            self.stop(&status);
            return (false, false);
        }
        if !can_act || call.host.item_state().is_busy {
            return (true, true);
        }

        let next = self.pending[0];
        self.ask(call, next, &settings);
        (self.is_running, false)
    }

    /// Counts every queued item no longer in the packs as given and drops
    /// it, however it left.
    fn count_items_that_left(&mut self, call: &mut UbCall<'_>) {
        if self.pending.is_empty() {
            return;
        }
        let owned: HashSet<u32> = call
            .host
            .owned_items()
            .iter()
            .map(|item| item.object_id)
            .collect();
        let before = self.pending.len();
        self.pending.retain(|give| owned.contains(&give.object_id));
        let left = before - self.pending.len();
        self.given += i32::try_from(left).unwrap_or(i32::MAX);
    }

    /// Asks for one item: the first ask of a new item restarts the run's
    /// clock and its count of asks; every ask counts, whether or not it went
    /// out; the ask that takes an item past the busy count writes it off at
    /// once, without waiting for its answer.
    fn ask(&mut self, call: &mut UbCall<'_>, next: PendingGive, settings: &InventorySettings) {
        if next.object_id != self.current_object_id {
            self.attempts = 0;
            self.since_progress = 0.0;
        }
        self.current_object_id = next.object_id;
        self.attempts += 1;
        self.give_calls += 1;

        let baseline = call.host.item_state().last_inventory_completion.revision;
        let result = call
            .host
            .give_item(next.object_id, self.target_object_id, next.amount);
        self.since_give = 0.0;
        if self.attempts > settings.give_busy_retry_limit.max(1) {
            self.write_off(settings);
            return;
        }
        if result.accepted() {
            self.waiting_object_id = next.object_id;
            self.completion_revision = baseline;
            self.status = format!(
                "Giving item {} to {}\u{2026}",
                self.given + 1,
                self.target_name
            );
        }
    }

    /// Writes off the item at the head of the queue, and stops the run once
    /// more have been written off than the failure count allows.
    fn write_off(&mut self, settings: &InventorySettings) {
        self.pending.pop_front();
        self.failed += 1;
        if self.failed > settings.give_failure_limit.max(0) {
            let status = format!(
                "Item giver gave up after {} failure(s); {} item(s) went to {}.",
                self.failed, self.given, self.target_name
            );
            self.stop(&status);
        }
    }

    /// Puts down a run without a word, as a new session does.
    pub fn reset(&mut self) {
        self.pending.clear();
        self.target_object_id = 0;
        self.waiting_object_id = 0;
        self.current_object_id = 0;
        self.completion_revision = 0;
        self.since_give = f64::MAX;
        self.attempts = 0;
        self.given = 0;
        self.failed = 0;
        self.is_running = false;
        "Item giver idle.".clone_into(&mut self.status);
    }

    fn begin(&mut self, target: &WorldObject, subject: &str, typed_target: &str) {
        typed_target.trim().clone_into(&mut self.typed_target);
        self.give_calls = 0;
        self.target_object_id = target.object_id;
        subject.clone_into(&mut self.subject);
        self.target_name.clone_from(&target.name);
        self.waiting_object_id = 0;
        self.current_object_id = 0;
        self.attempts = 0;
        self.given = 0;
        self.failed = 0;
        self.since_give = f64::MAX;
        self.run_seconds = 0.0;
        self.since_progress = 0.0;
        self.end_error.clear();
        self.is_running = true;
        self.status = if self.pending.is_empty() {
            format!("No items match {}.", self.subject)
        } else {
            format!(
                "Giving {} item(s) to {}.",
                self.pending.len(),
                self.target_name
            )
        };
    }

    /// Finds the player or non-player character of that name, nearest
    /// first, and refuses one further off than the give range allows, each
    /// refusal in the reference's words for that command.
    fn try_resolve_target(
        &mut self,
        call: &mut UbCall<'_>,
        target_name: &str,
        partial_target: bool,
        usage: &str,
    ) -> Option<WorldObject> {
        let requested = target_name.trim();
        // Part of a name is enough for the partial flag, and the empty
        // string is part of every name: without this, a line that lost its
        // target would hand the packs to whoever stood nearest.
        if requested.is_empty() {
            self.status = format!("Syntax: {usage}");
            self.refusal.clone_from(&self.status);
            return None;
        }
        // The reference's object search: an id, a hex id or "selected"
        // first, then the nearest player or non-player character of the name.
        let Some(target) =
            search::try_find_nearest(call.host, requested, partial_target, &TARGET_CLASSES)
        else {
            self.status = format!("Item giver target not found: {requested}.");
            self.refusal = format!("player {requested} not found");
            return None;
        };
        if Some(target.object_id) == call.host.character().map(|character| character.id) {
            "Item giver refused: the target is yourself.".clone_into(&mut self.status);
            "You can't give to yourself".clone_into(&mut self.refusal);
            return None;
        }
        let range = call
            .services
            .inventory_settings()
            .give_range_meters
            .max(0.0);
        let distance = distance_from_player(call, &target);
        if distance > range {
            self.status = format!(
                "{} is {} m away; the give range is {} m.",
                target.name,
                format(distance, "0.##"),
                format(range, "0.##")
            );
            let shown_range = mosstank_dotnet::double_to_string(range);
            self.refusal = if usage == PROFILE_GIVE_USAGE {
                format!(
                    "ItemGiver {requested} is {} meters away. IGRange is set to {shown_range}",
                    format(distance, "n2")
                )
            } else {
                format!(
                    "{requested} is {} meters away, IGRange is set to {shown_range}. bailing.",
                    format(distance, "n2")
                )
            };
            return None;
        }
        Some(target)
    }

    fn stop(&mut self, status: &str) {
        self.finished_line = format!(
            "ItemGiver finished: {} to {}. took {} to give {} item(s). {}",
            self.subject,
            self.typed_target,
            friendly_time(self.run_seconds),
            self.given,
            self.give_calls - self.given
        );
        self.pending.clear();
        self.target_object_id = 0;
        self.waiting_object_id = 0;
        self.completion_revision = 0;
        self.since_give = f64::MAX;
        self.attempts = 0;
        self.is_running = false;
        status.clone_into(&mut self.status);
    }
}

fn is_stop_word(text: &str) -> bool {
    STOP_WORDS
        .iter()
        .any(|word| ordinal::equals_ignore_case(text, word))
}

/// `{value:format}` in the invariant culture.
fn format(value: f64, format: &str) -> String {
    number::format_f64(value, format).expect("a number format the kit prints")
}

/// Items that can be handed over (nothing worn or wielded), by id.
fn givable(owned: &[InventoryItem]) -> Vec<&InventoryItem> {
    let mut items: Vec<&InventoryItem> = owned
        .iter()
        .filter(|item| item.equipped_location == 0 && item.wielder_id == 0)
        .collect();
    items.sort_by_key(|item| item.object_id);
    items
}

/// The first rule that matches decides; the item goes when it says keep or
/// keep-up-to.
fn matches_give_profile(
    call: &mut UbCall<'_>,
    item: &InventoryItem,
    properties: &ItemProperties,
    rules: &[LootRule],
) -> bool {
    for rule in rules {
        if !call.host.loot_rule_matches(rule, item, properties) {
            continue;
        }
        return matches!(rule.action, LootAction::Keep | LootAction::KeepUpTo);
    }
    false
}

/// Flat ground distance in metres: a target one floor up and a target
/// beside it are the same distance away.
fn distance_from_player(call: &mut UbCall<'_>, target: &WorldObject) -> f64 {
    let player = call.host.navigation();
    match (&target.position, player.is_available) {
        (Some(position), true) => player.position.horizontal_distance_metres(position),
        _ => f64::MAX,
    }
}

/// A span as the reference writes one: the days, hours, minutes and
/// seconds that are not zero ("1m 5s"), or "0s".
#[must_use]
pub fn friendly_time(seconds: f64) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let total = seconds.floor().max(0.0) as u64;
    let days = total / 86_400;
    let hours = total / 3_600 % 24;
    let minutes = total / 60 % 60;
    let secs = total % 60;
    let mut parts = Vec::with_capacity(4);
    for (value, unit) in [(days, "d"), (hours, "h"), (minutes, "m"), (secs, "s")] {
        if value > 0 {
            parts.push(format!("{value}{unit}"));
        }
    }
    if parts.is_empty() {
        "0s".to_owned()
    } else {
        parts.join(" ")
    }
}

//! `/ub autopack`: Mag-Tools' Inventory Packer
//! (`MagTools.Macros.InventoryPacker` by Mag-nus, used with the author's
//! permission; see `rust/NOTICE.md`). A loot profile says which pack each
//! item belongs in: a rule whose action is keep-up-to names the packs in
//! the digits of its count, the first choice first: 0 is the main pack and
//! a side pack is its slot on the character plus one, the slots foci take
//! counted (so a focus in the first slot makes the first side pack 2), and
//! `312` reads "pack 3, else pack 1, else pack 2".
//! The first rule that matches an item decides it; any other action leaves
//! the item where it is.
//!
//! A run first pours part stacks of the same name in the same pack
//! together, then moves one item at a time to the best pack of its list
//! that has room, until nothing is left to move. An item whose rule turns
//! on an appraisal is appraised first, one request at a time through the
//! shared appraisal slot; one whose appraisal does not come (three
//! unanswered requests, or ten seconds waiting for a slot somebody else
//! holds) is packed by what the client knows of it. An item that will not
//! move for ten seconds is left alone for the rest of the run.
//!
//! The profile is a loot profile: the one named on the command line, or
//! `<character>.AutoPack`, or `Default.AutoPack` (Mag-Tools' file names).

use std::collections::{HashMap, HashSet};

use ac_plugin_abi::items::{InventoryItem, ItemStatus};
use ac_plugin_abi::world::ItemProperties;
use mosstank_dotnet::text::ordinal;
use mosstank_formats::utl::{LootAction, LootRule};
use mosstank_loot::requirements;

use crate::commands::{UbCall, strip_extension};

/// Public weenie flag of a focus: never packed, and never packed into.
const PUBLIC_WEENIE_FOCI: u32 = 0x0080_0000;

/// The pause between two looks at the packs (the reference thinks every
/// 100 ms).
pub const THINK_INTERVAL_SECONDS: f64 = 0.1;
/// How long one item may stay the item being worked on, counting only time
/// in which the run could act, before it is left alone for the rest of the
/// run.
pub const GIVE_UP_SECONDS: f64 = 10.0;
/// How long a move's receipt is waited for before the packs are looked at
/// again without it.
pub const RECEIPT_WAIT_SECONDS: f64 = 3.0;
/// One appraisal request every 499 ms, the corpse identifier's cadence.
const IDENTIFY_REQUEST_INTERVAL_SECONDS: f64 = 0.499;
/// How long one request waits before it counts as unanswered.
const IDENTIFY_TIMEOUT_SECONDS: f64 = 10.0;
/// Failed requests one item gets before it is decided from what the client
/// already holds.
const MAXIMUM_IDENTIFY_ATTEMPTS_PER_ITEM: i32 = 3;
/// How long the next item to appraise may wait for the shared appraisal
/// slot while somebody else holds it (the looter, the counter, a question
/// the client asked itself), counting only time in which the run could
/// act, before it is packed by what the client already holds: as long as
/// one request of ours is given to be answered.
pub const APPRAISAL_SLOT_WAIT_SECONDS: f64 = IDENTIFY_TIMEOUT_SECONDS;
/// How many items one look judges against the rules at most. Judging one
/// is dear in the plugin (a match per rule, a large profile has hundreds),
/// so a full inventory is judged over a few looks, and after that only
/// what changed.
pub const JUDGEMENTS_PER_LOOK: usize = 10;
/// How many of the profile's patterns one look compiles ahead (a compile
/// costs the plugin about as much as judging a few items).
const PATTERNS_PER_LOOK: usize = 4;
/// The reference tries a rule's first ten packs.
const MAXIMUM_PACK_CHOICES: usize = 10;

/// The words that call a run off.
const STOP_WORDS: [&str; 4] = ["stop", "cancel", "quit", "abort"];

/// Something loose in a pack: not worn, not wielded.
fn is_carried(item: &InventoryItem) -> bool {
    item.equipped_location == 0 && item.wielder_id == 0 && item.container_id != 0
}

/// A pack or a focus: it has its own slot and is never moved.
fn is_container(item: &InventoryItem) -> bool {
    item.items_capacity > 0
        || item.containers_capacity > 0
        || item.public_flags & PUBLIC_WEENIE_FOCI != 0
}

/// The part stack to pour and the part stack of the same name in the same
/// pack to pour it onto (`DoAutoStack`). A stack left alone is neither
/// poured nor poured onto.
#[must_use]
pub fn plan_stack(items: &[InventoryItem], ignored: &HashSet<u32>) -> Option<(u32, u32)> {
    let has_room = |item: &InventoryItem| {
        !ignored.contains(&item.object_id)
            && is_carried(item)
            && item.maximum_stack_size > 1
            && item.stack_size != item.maximum_stack_size
    };
    for item in items {
        if !has_room(item) {
            continue;
        }
        let onto = items.iter().find(|second| {
            second.object_id != item.object_id
                && second.container_id == item.container_id
                && second.name == item.name
                && has_room(second)
        });
        if let Some(onto) = onto {
            return Some((item.object_id, onto.object_id));
        }
    }
    None
}

/// The packs by the number a rule calls them: the character (the main
/// pack) at 0, then each side pack at its slot plus one. A number with no
/// pack behind it is 0.
#[must_use]
pub fn numbered_packs(items: &[InventoryItem], player_object_id: u32) -> Vec<u32> {
    let mut packs = vec![player_object_id];
    for pack in items.iter().filter(|item| {
        item.container_id == player_object_id
            && item.items_capacity > 0
            && item.public_flags & PUBLIC_WEENIE_FOCI == 0
            && item.container_slot >= 0
    }) {
        let number = usize::try_from(pack.container_slot).unwrap_or_default() + 1;
        if packs.len() <= number {
            packs.resize(number + 1, 0);
        }
        packs[number] = pack.object_id;
    }
    packs
}

/// A keep-up-to count read as pack numbers, one decimal digit each
/// (`123` is packs 1, 2 and 3); `None` for a negative count, which names
/// none.
#[must_use]
pub fn pack_numbers(keep_count: i32) -> Option<Vec<usize>> {
    if keep_count < 0 {
        return None;
    }
    Some(
        keep_count
            .to_string()
            .bytes()
            .map(|digit| usize::from(digit - b'0'))
            .collect(),
    )
}

/// What a rule that matches does with an item: the pack numbers of a
/// keep-up-to, `None` (leave it where it is) for any other action.
fn packing_of(rule: &LootRule) -> Option<Vec<usize>> {
    if rule.action != LootAction::KeepUpTo {
        return None;
    }
    pack_numbers(rule.keep_count)
}

/// An item that is not in its first pack, and the packs it may go to, best
/// first (0 where the rule names a pack that is not there).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Misplaced {
    /// The item.
    pub object_id: u32,
    /// The pack it is in.
    pub container_id: u32,
    /// How many are in its stack.
    pub amount: u32,
    /// Its packs, best first.
    pub targets: Vec<u32>,
}

/// The one move to make now: the first choices are tried for every item
/// before anybody's second, an item already in the pack a round offers it
/// stays there, and a pack takes an item only while it has a free slot
/// (`DoAutoPack`'s second half). The item and the pack.
#[must_use]
pub fn plan_move(
    misplaced: &[Misplaced],
    free_slots: &dyn Fn(u32) -> i32,
) -> Option<(u32, u32, u32)> {
    let mut settled: HashSet<u32> = HashSet::new();
    for choice in 0..MAXIMUM_PACK_CHOICES {
        // The reference walks its list from the end.
        for item in misplaced.iter().rev() {
            let Some(&target) = item.targets.get(choice) else {
                continue;
            };
            if settled.contains(&item.object_id) {
                continue;
            }
            if item.container_id == target {
                settled.insert(item.object_id);
                continue;
            }
            if target != 0 && free_slots(target) > 0 {
                return Some((item.object_id, target, item.amount));
            }
        }
    }
    None
}

/// What one look at the packs came to.
enum Step {
    /// A request went out, or the look was spent giving an item up.
    Acted,
    /// Nothing can move until an appraisal lands.
    Waiting,
    /// Nothing is left to do.
    Done,
}

/// The inventory packer.
#[derive(Debug, Clone, Default)]
pub struct InventoryPacker {
    rules: Vec<LootRule>,
    is_running: bool,
    /// Items that would not move, left alone for the rest of the run.
    ignored: HashSet<u32>,
    working_object_id: u32,
    working_seconds: f64,
    /// The request in flight: the item, and the completion count before it.
    in_flight: Option<(u32, i64)>,
    in_flight_seconds: f64,
    since_think: f64,
    identify_attempts: HashMap<u32, i32>,
    awaiting_object_id: u32,
    awaiting_seconds: f64,
    since_request: f64,
    /// The next item to appraise while somebody else holds the slot, and
    /// how long it has waited for it.
    slot_wait_object_id: u32,
    slot_wait_seconds: f64,
    /// How many rules' patterns are compiled ahead.
    patterns_warmed: usize,
    /// What the rules made of each item, and the item as they judged it.
    judged: HashMap<u32, (JudgedState, Verdict)>,
}

/// What the rules made of an item.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Verdict {
    /// Appraise it first.
    Appraise,
    /// Leave it where it is.
    Stay,
    /// Its pack numbers, best first.
    Packs(Vec<usize>),
}

/// The item as the rules judged it. A judgement holds while these hold: a
/// move, a pour, an appraisal landing or the appraisal given up on judges
/// the item again. The rules match against much more than this, but within
/// one run of the packer the rest of an item does not change by its doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct JudgedState {
    container_id: u32,
    stack_size: i32,
    has_appraisal_data: bool,
    out_of_attempts: bool,
}

impl InventoryPacker {
    /// Idle.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a run is under way.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// `/ub autopack [lootProfile]` and `/ub autopack stop` (or cancel,
    /// quit, abort).
    pub fn command(&mut self, call: &mut UbCall<'_>, arguments: &str) {
        let text = arguments.trim();
        if STOP_WORDS
            .iter()
            .any(|word| ordinal::equals_ignore_case(text, word))
        {
            if self.is_running {
                self.finish(call, "Auto Pack - Stopped.");
            } else {
                call.write_ub_error("Auto Pack is not running.");
            }
            return;
        }
        if self.is_running {
            call.write_ub_error("Auto Pack is already running. Use /ub autopack stop to end it.");
            return;
        }
        let Some(character) = call.host.character() else {
            call.write_ub_error("Auto Pack needs a character in the world.");
            return;
        };

        let candidates = if text.is_empty() {
            vec![
                format!("{}.AutoPack", character.name),
                "Default.AutoPack".to_owned(),
            ]
        } else {
            vec![strip_extension(text, &[".utl", ".json"])]
        };
        let rules = candidates
            .iter()
            .find_map(|name| call.services.loot_profile_rules(call.host, name));
        let Some(rules) = rules else {
            call.write_ub_error(&format!(
                "Auto Pack profile does not exist: {}",
                candidates
                    .iter()
                    .map(|name| format!("{name}.utl"))
                    .collect::<Vec<_>>()
                    .join(" or ")
            ));
            return;
        };

        *self = Self {
            rules,
            is_running: true,
            // Armed, so the first look and the first request need no wait.
            since_think: THINK_INTERVAL_SECONDS,
            since_request: IDENTIFY_REQUEST_INTERVAL_SECONDS,
            ..Self::default()
        };
        call.write_ub("Auto Pack - Started.");
    }

    /// One frame; true while it owns the character this frame.
    pub fn tick(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64, can_act: bool) -> bool {
        if !self.is_running {
            return false;
        }
        if call.host.character().is_none() {
            self.reset();
            return false;
        }
        let elapsed = elapsed_seconds.max(0.0);
        self.since_think += elapsed;
        self.since_request += elapsed;
        self.observe_appraisal(call, elapsed);

        let state = call.host.item_state();
        // Only time in which the run could act counts against the item
        // being worked on: waiting on its own request does, while a buff
        // burst, a portal, an earlier owner or somebody else's item command
        // holds the character it does not.
        let able = can_act && state.is_available;
        if able && (self.in_flight.is_some() || !state.is_busy) {
            self.working_seconds += elapsed;
        }
        if able {
            self.slot_wait_seconds += elapsed;
        }
        if let Some((_, baseline)) = self.in_flight {
            self.in_flight_seconds += elapsed;
            let answered = state.last_inventory_completion.revision != baseline;
            if state.is_busy || (!answered && self.in_flight_seconds < RECEIPT_WAIT_SECONDS) {
                return true;
            }
            self.in_flight = None;
        }
        if !able {
            return false;
        }
        if state.is_busy || self.since_think < THINK_INTERVAL_SECONDS {
            return true;
        }
        self.since_think = 0.0;

        match self.think(call) {
            Step::Acted | Step::Waiting => true,
            Step::Done => {
                self.finish(call, "Auto Pack - Completed.");
                false
            }
        }
    }

    /// Drops a run without a word, as a new session does.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn finish(&mut self, call: &mut UbCall<'_>, line: &str) {
        self.reset();
        call.write_ub(line);
    }

    /// One look at the packs (`Think`): a stack first, then a move.
    fn think(&mut self, call: &mut UbCall<'_>) -> Step {
        let Some(character) = call.host.character() else {
            return Step::Done;
        };
        let owned = call.host.owned_items_shared();
        self.ignored
            .retain(|id| owned.iter().any(|item| item.object_id == *id));

        if let Some((source, onto)) = plan_stack(&owned, &self.ignored) {
            if self.may_work_on(call, source, &owned) {
                let result = call.host.merge_items(source, onto, 0);
                self.sent(call, source, result.accepted());
            }
            return Step::Acted;
        }

        if !self.warm_patterns() {
            return Step::Waiting;
        }

        let packs = numbered_packs(&owned, character.id);
        let owned_ids: HashSet<u32> = owned.iter().map(|item| item.object_id).collect();
        self.judged.retain(|id, _| owned_ids.contains(id));
        let mut misplaced = Vec::new();
        let mut unidentified = Vec::new();
        let mut judgements_left = JUDGEMENTS_PER_LOOK;
        let mut all_judged = true;
        for item in owned.iter() {
            if self.ignored.contains(&item.object_id) || !is_carried(item) || is_container(item) {
                continue;
            }
            // An item the client's object table has no record of can be
            // neither appraised nor decided, so it is passed over.
            let Some(known) = call.host.object(item.object_id) else {
                continue;
            };
            let state = JudgedState {
                container_id: item.container_id,
                stack_size: item.stack_size,
                has_appraisal_data: known.has_appraisal_data,
                out_of_attempts: self.attempts_spent(item.object_id)
                    >= MAXIMUM_IDENTIFY_ATTEMPTS_PER_ITEM,
            };
            let kept = self
                .judged
                .get(&item.object_id)
                .filter(|(judged, _)| *judged == state)
                .map(|(_, verdict)| verdict.clone());
            let verdict = match kept {
                Some(verdict) => verdict,
                None if judgements_left > 0 => {
                    judgements_left -= 1;
                    let verdict = self.judge(call, item, known.has_appraisal_data);
                    self.judged.insert(item.object_id, (state, verdict.clone()));
                    verdict
                }
                None => {
                    all_judged = false;
                    continue;
                }
            };
            let numbers = match verdict {
                Verdict::Appraise => {
                    unidentified.push(item.object_id);
                    continue;
                }
                Verdict::Stay => continue,
                Verdict::Packs(numbers) => numbers,
            };
            let targets: Vec<u32> = numbers
                .iter()
                .map(|number| packs.get(*number).copied().unwrap_or_default())
                .collect();
            if targets.first() == Some(&item.container_id) {
                continue;
            }
            misplaced.push(Misplaced {
                object_id: item.object_id,
                container_id: item.container_id,
                amount: item.stack_size.max(1).cast_unsigned(),
                targets,
            });
        }
        self.request_next_appraisal(call, &unidentified, &owned);
        // Which item gets a free slot turns on every item's packs, so
        // nothing moves until the last item is judged.
        if !all_judged {
            return Step::Waiting;
        }

        // What each side pack holds, counted once for the look.
        let mut held: HashMap<u32, i32> = HashMap::new();
        for item in owned.iter() {
            if is_carried(item) && !is_container(item) {
                *held.entry(item.container_id).or_default() += 1;
            }
        }
        let capacity: HashMap<u32, i32> = owned
            .iter()
            .filter(|item| item.items_capacity > 0)
            .map(|item| (item.object_id, item.items_capacity))
            .collect();
        let free_slots = |pack: u32| {
            if pack == character.id {
                return character.main_pack_free_slots;
            }
            capacity.get(&pack).copied().unwrap_or_default()
                - held.get(&pack).copied().unwrap_or_default()
        };
        if let Some((source, pack, amount)) = plan_move(&misplaced, &free_slots) {
            if self.may_work_on(call, source, &owned) {
                let result = call.host.move_item(source, pack, amount, 0, true);
                self.sent(call, source, result.accepted());
            }
            return Step::Acted;
        }
        if unidentified.is_empty() {
            Step::Done
        } else {
            Step::Waiting
        }
    }

    /// Compiles the profile's patterns ahead, [`PATTERNS_PER_LOOK`] a look,
    /// so the first look that judges items does not compile them all in
    /// one frame. True once every rule's are compiled.
    fn warm_patterns(&mut self) -> bool {
        let mut compiled = 0;
        while compiled < PATTERNS_PER_LOOK && self.patterns_warmed < self.rules.len() {
            for requirement in &self.rules[self.patterns_warmed].vtank_requirements {
                compiled += requirements::warm_patterns(requirement);
            }
            self.patterns_warmed += 1;
        }
        self.patterns_warmed >= self.rules.len()
    }

    /// What the rules make of an item now.
    fn judge(
        &self,
        call: &mut UbCall<'_>,
        item: &InventoryItem,
        has_appraisal_data: bool,
    ) -> Verdict {
        let properties = call.host.properties(item.object_id).unwrap_or_default();
        if self.needs_appraisal(call, item, &properties, has_appraisal_data) {
            return Verdict::Appraise;
        }
        self.pack_choices(call, item, &properties)
            .map_or(Verdict::Stay, Verdict::Packs)
    }

    /// The pack numbers the first rule that matches gives the item; `None`
    /// when no rule matches or the rule is not a keep-up-to.
    fn pack_choices(
        &self,
        call: &mut UbCall<'_>,
        item: &InventoryItem,
        properties: &ItemProperties,
    ) -> Option<Vec<usize>> {
        let rule = self
            .rules
            .iter()
            .find(|rule| call.host.loot_rule_matches(rule, item, properties))?;
        packing_of(rule)
    }

    /// Whether the item may be asked for: the first ask of a new item
    /// starts its clock; an item still being asked for after
    /// [`GIVE_UP_SECONDS`] is left alone instead, and said.
    fn may_work_on(
        &mut self,
        call: &mut UbCall<'_>,
        object_id: u32,
        owned: &[InventoryItem],
    ) -> bool {
        if self.working_object_id != object_id {
            self.working_object_id = object_id;
            self.working_seconds = 0.0;
            return true;
        }
        if self.working_seconds <= GIVE_UP_SECONDS {
            return true;
        }
        self.ignored.insert(object_id);
        let name = owned
            .iter()
            .find(|item| item.object_id == object_id)
            .map_or("", |item| item.name.as_str());
        call.write_ub(&format!("Auto Pack - Skipping item: {object_id}, {name}"));
        false
    }

    /// Remembers a request that went out, to wait for its receipt.
    fn sent(&mut self, call: &mut UbCall<'_>, object_id: u32, accepted: bool) {
        if !accepted {
            return;
        }
        let baseline = call.host.item_state().last_inventory_completion.revision;
        self.in_flight = Some((object_id, baseline));
        self.in_flight_seconds = 0.0;
    }

    // ---- appraisals -------------------------------------------------------

    /// True when an appraisal could still change where the item goes, it
    /// has attempts left, and the client holds none for it yet.
    fn needs_appraisal(
        &self,
        call: &mut UbCall<'_>,
        item: &InventoryItem,
        properties: &ItemProperties,
        has_appraisal_data: bool,
    ) -> bool {
        !has_appraisal_data
            && self.attempts_spent(item.object_id) < MAXIMUM_IDENTIFY_ATTEMPTS_PER_ITEM
            && self.appraisal_could_change_packing(call, item, properties)
    }

    /// Whether the first rule that matches the item cannot be known without
    /// an appraisal, and knowing it could change where the item goes: some
    /// rule before the first one that decides without appraisal data is
    /// still open, and it would pack the item differently (other packs, or
    /// leave it) from the rule that decides. The looter's own test is not
    /// enough here: it treats two rules with one action as one outcome, and
    /// every rule the packer follows is a keep-up-to that differs only in
    /// its packs.
    fn appraisal_could_change_packing(
        &self,
        call: &mut UbCall<'_>,
        item: &InventoryItem,
        properties: &ItemProperties,
    ) -> bool {
        let mut open: Vec<Option<Vec<usize>>> = Vec::new();
        for rule in &self.rules {
            let (decided, matched) = call.host.loot_rule_early_match(rule, item, properties);
            if decided && !matched {
                continue;
            }
            let packing = packing_of(rule);
            if decided {
                return open.iter().any(|earlier| *earlier != packing);
            }
            if !open.contains(&packing) {
                open.push(packing);
            }
        }
        // No rule decides: without one the item is left where it is.
        open.iter().any(Option::is_some)
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

    /// Watches the request in flight: an answer frees the slot; silence past
    /// the timeout, or a question the client has given up on, spends one of
    /// the item's attempts.
    fn observe_appraisal(&mut self, call: &mut UbCall<'_>, elapsed: f64) {
        if self.awaiting_object_id == 0 {
            return;
        }
        self.awaiting_seconds += elapsed;
        let answered = call
            .host
            .object(self.awaiting_object_id)
            .is_some_and(|known| known.has_appraisal_data);
        if !answered {
            let abandoned = call.host.appraisal().last_abandoned_id == self.awaiting_object_id;
            if !abandoned && self.awaiting_seconds < IDENTIFY_TIMEOUT_SECONDS {
                return;
            }
            self.spend_attempt(self.awaiting_object_id);
        }
        self.awaiting_object_id = 0;
        self.awaiting_seconds = 0.0;
    }

    /// Asks about the next unidentified item when the appraisal slot is
    /// free to ask with: the looter, the counter and this run share it. An
    /// item that waits for the slot longer than
    /// [`APPRAISAL_SLOT_WAIT_SECONDS`] while somebody else holds it is
    /// packed by what the client already holds, and said.
    fn request_next_appraisal(
        &mut self,
        call: &mut UbCall<'_>,
        unidentified: &[u32],
        owned: &[InventoryItem],
    ) {
        let Some(&next) = unidentified.first() else {
            self.slot_wait_object_id = 0;
            return;
        };
        // Our own request in flight, or our own pause between requests:
        // the slot is ours, nobody keeps the item waiting.
        if self.awaiting_object_id != 0 || self.since_request < IDENTIFY_REQUEST_INTERVAL_SECONDS {
            self.slot_wait_seconds = 0.0;
            return;
        }
        let mut held =
            call.services.corpse_identifier_busy() || call.host.appraisal().awaiting_id != 0;
        if !held {
            for &object_id in unidentified {
                let result = call.host.identify(object_id);
                if result.accepted() {
                    self.awaiting_object_id = object_id;
                    self.awaiting_seconds = 0.0;
                    self.since_request = 0.0;
                    self.slot_wait_seconds = 0.0;
                    return;
                }
                // Busy is the client's own backpressure and says nothing
                // about this item; anything else is a refusal of this item
                // and costs it one attempt.
                if result.status == ItemStatus::Busy {
                    held = true;
                    break;
                }
                self.spend_attempt(object_id);
            }
        }
        if !held {
            self.slot_wait_seconds = 0.0;
            return;
        }
        if self.slot_wait_object_id != next {
            self.slot_wait_object_id = next;
            self.slot_wait_seconds = 0.0;
            return;
        }
        if self.slot_wait_seconds < APPRAISAL_SLOT_WAIT_SECONDS {
            return;
        }
        // Out of attempts: the next look decides it from what is known.
        self.identify_attempts
            .insert(next, MAXIMUM_IDENTIFY_ATTEMPTS_PER_ITEM);
        self.slot_wait_object_id = 0;
        self.slot_wait_seconds = 0.0;
        let name = owned
            .iter()
            .find(|item| item.object_id == next)
            .map_or("", |item| item.name.as_str());
        call.write_ub(&format!(
            "Auto Pack - Could not appraise item: {next}, {name}"
        ));
    }
}

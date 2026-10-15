//! The looter (`LootController`): its turn in the rule pass, the frame's
//! identification, the item half of a corpse (deciding, pulling, waiting
//! for each pull), and the work on what was looted (salvage, selling,
//! combining bags, draining donors into stones). The corpse half is in
//! [`crate::corpse_scan`], this character's own rare in [`crate::own_rare`].

use std::collections::{HashMap, HashSet};

use ac_plugin_abi::items::{InventoryItem, ItemStatus, LootContext, LootedItem};
use ac_plugin_abi::world::{ItemProperties, ObjectClass};
use mosstank_automation::locks::{ActionLockKind, SharedActionLocks, item_use_lock};
use mosstank_dotnet::collections::{
    DotNetDictionary, IgnoreCaseStringMap, Ordinal, OrdinalStringMap, OrdinalStringSet,
};
use mosstank_dotnet::text::{casing, ordinal};
use mosstank_formats::utl::LootAction;
use mosstank_settings::model::{ConsumableCategory, LootSettings};
use openac_plugin_compat::number;

use crate::donors::{DonorStore, ManaDonorJournal};
use crate::host::{LootHost, status_name};
use crate::mana::{self, ManaFillOutcome, ManaStoneTransferPlan, TransferChecks};
use crate::rules::{self, LootDecision, LootRefusal, LootRefusalKind};
use crate::salvage::{self, SalvageBagCombinePlan};
use crate::scroll;

/// How long the salvage, sell, mana-fill and combine steps wait for their
/// answer.
const PICKUP_TIMEOUT_SECONDS: f64 = 4.0;

/// How long one pull holds the item slot and navigation. It is the pace of
/// emptying a corpse, and it is what keeps the walk rule above this one off
/// the character between pulls.
const PICKUP_HOLD_SECONDS: f64 = 0.75;

/// How long a corpse the server refused stays skipped. This one is fixed;
/// it is not one of the corpse-blacklist settings.
pub(crate) const DENIAL_SKIP_SECONDS: f64 = 10.0;

/// How often rare-only looting notes which corpses are new.
pub const CORPSE_SIGHTING_INTERVAL_SECONDS: f64 = 2.0;

/// The reference's id queue sends one request every 499 ms, round robin
/// over everything waiting for an id.
const IDENTIFY_REQUEST_INTERVAL_SECONDS: f64 = 0.499;

/// How many of an open corpse's items one frame weighs against the rule
/// list (register LOOT-7; the C# weighs them all in one frame).
pub const ITEMS_WEIGHED_PER_FRAME: usize = 1;

/// The item type bit of a salvaging tool.
const TINKERING_TOOL: u32 = 0x2000_0000;

/// The free-slot count is not something the client can answer yet.
const UNKNOWN_FREE_SLOTS: i32 = -1;

/// A callback the looter writes its lines to.
pub type LineSink = Box<dyn FnMut(&str)>;

/// `Math.Clamp` for doubles.
pub(crate) fn clamp(value: f64, minimum: f64, maximum: f64) -> f64 {
    value.max(minimum).min(maximum)
}

/// A number as `{value:0}` prints it.
pub(crate) fn whole(value: f64) -> String {
    number::format_f64(value, "0").expect("a supported format")
}

/// The journal's storage and log over the client.
pub(crate) struct HostDonorStore<'a> {
    pub(crate) host: &'a mut dyn LootHost,
}

impl DonorStore for HostDonorStore<'_> {
    fn is_available(&mut self) -> bool {
        self.host.storage_available()
    }

    fn read_text(&mut self, key: &str) -> Result<Option<String>, String> {
        self.host.storage_read_text(key)
    }

    fn write_text(&mut self, key: &str, content: &str) -> Result<(), String> {
        self.host.storage_write_text(key, content)
    }

    fn info(&mut self, message: &str) {
        self.host.log_info(message);
    }

    fn error(&mut self, message: &str) {
        self.host.log_error(message);
    }
}

/// The looter.
#[allow(clippy::struct_excessive_bools)]
pub struct LootController {
    /// The profile's loot settings; read afresh on every call.
    pub settings: LootSettings,
    /// A warning the player should see in chat, such as salvage waiting for
    /// a tool. The loot work goes on regardless; this only tells.
    pub warning: Option<LineSink>,
    /// The macro log's Loot channel: what the looter opened, decided and
    /// took. Every line it writes goes to that channel.
    pub log: Option<LineSink>,
    /// What the looter is doing, for the panel.
    pub status: String,

    pub(crate) configured_consumable_names: OrdinalStringSet,
    pub(crate) configured_consumable_kinds: OrdinalStringMap<ConsumableCategory>,
    pub(crate) warned_no_salvage_tool: bool,
    pub(crate) completed_corpses: HashMap<u32, f64>,
    /// The completed corpses that held this character's own rare. With the
    /// own-rare walk on they outlive [`Self::reset`]: an emptied rare corpse
    /// still describes itself as one.
    pub(crate) completed_own_rare_corpses: HashSet<u32>,
    /// The corpse last opened, when it held this character's own rare.
    pub(crate) opened_own_rare_corpse: u32,
    pub(crate) corpse_open_attempts: HashMap<u32, i32>,
    pub(crate) corpse_blacklisted_at: HashMap<u32, f64>,
    pub(crate) item_attempts: HashMap<u32, i32>,
    pub(crate) pending_by_name: IgnoreCaseStringMap<i32>,
    pub(crate) classified_owned_items: DotNetDictionary<u32, LootAction, Ordinal<u32>>,
    pub(crate) external_classifier_by_item: DotNetDictionary<u32, String, Ordinal<u32>>,
    pub(crate) decisions: HashMap<u32, Option<LootDecision>>,
    pub(crate) corpse_first_seen: HashMap<u32, f64>,
    /// When each corpse last came into the client's known set.
    pub(crate) corpse_last_seen: HashMap<u32, f64>,
    /// The corpses the client has stopped reporting.
    pub(crate) released_corpses: HashSet<u32>,
    pub(crate) present_corpses: HashSet<u32>,
    pub(crate) evicted_corpses: Vec<u32>,
    pub(crate) corpse_denied_at: HashMap<u32, f64>,
    /// How many times each corpse has been asked to describe itself without
    /// an answer coming back.
    pub(crate) corpse_description_attempts: HashMap<u32, i32>,
    /// Items already reported as dropped from the pass for spending their
    /// attempts.
    pub(crate) abandoned_items: HashSet<u32>,
    /// The items of the corpse now open that the client would not take.
    pub(crate) refused_items: HashSet<u32>,
    /// The corpse the pass gave up on, still open and waiting for its
    /// closing use.
    pub(crate) abandoned_corpse: u32,
    pub(crate) selected_corpse: u32,
    pub(crate) chat_sequence: u64,
    /// When the server last announced that this character found a rare, on
    /// the controller's own clock; minus infinity when it never has.
    pub(crate) rare_announced_at: f64,
    pub(crate) state_age: f64,
    pub(crate) active_corpse: u32,
    pub(crate) active_corpse_saw_contents: bool,
    pub(crate) active_corpse_is_own_death: bool,
    pub(crate) waiting_item: u32,
    pub(crate) waiting_name: String,
    pub(crate) waiting_action: LootAction,
    pub(crate) waiting_quantity: i32,
    pub(crate) waiting_item_snapshot: InventoryItem,
    pub(crate) waiting_classifier_id: String,
    pub(crate) waiting_pickup_accepted: bool,
    pub(crate) waiting_pickup_completion_revision: i64,
    pub(crate) awaiting_appraisal: u32,
    pub(crate) awaiting_corpse_appraisal: u32,
    pub(crate) last_corpse_description_request: u32,
    pub(crate) lifetime: f64,
    pub(crate) pending_scroll_reads: DotNetDictionary<u32, u32, Ordinal<u32>>,
    pub(crate) salvage_pending_item: u32,
    pub(crate) salvage_pending_name: String,
    pub(crate) salvage_attempts: i32,
    pub(crate) sell_pending_item: u32,
    pub(crate) sell_pending_name: String,
    pub(crate) mana_transfer: Option<ManaStoneTransferPlan>,
    pub(crate) mana_transfer_revision: i64,
    /// When the item-slot window the outstanding drain armed runs out.
    pub(crate) mana_transfer_hold_until: f64,
    pub(crate) combine_pending: Option<SalvageBagCombinePlan>,
    pub(crate) combine_attempts: HashMap<u32, i32>,
    pub(crate) abandoned_combine_bags: HashSet<u32>,
    pub(crate) action_locks: Option<SharedActionLocks>,
    pub(crate) identify_age: f64,
    pub(crate) since_corpse_sighting: f64,
    pub(crate) since_identify_request: f64,
    pub(crate) mana_donors: ManaDonorJournal,
    /// How far the pattern warm-up has got: the rule list it walks (its
    /// address and length, which change when a profile is loaded or
    /// edited) and the next rule in it.
    pub(crate) pattern_warmup: (usize, usize, usize),
}

impl std::fmt::Debug for LootController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LootController")
            .field("status", &self.status)
            .field("active_corpse", &self.active_corpse)
            .field("waiting_item", &self.waiting_item)
            .finish_non_exhaustive()
    }
}

/// What the drain planner asks, answered by the looter's own records.
struct ControllerChecks<'a> {
    host: &'a mut dyn LootHost,
    journal: &'a ManaDonorJournal,
    kinds: &'a OrdinalStringMap<ConsumableCategory>,
}

impl TransferChecks for ControllerChecks<'_> {
    fn is_profiled_mana_stone(&mut self, item: &InventoryItem) -> bool {
        is_profiled_mana_stone(self.kinds, item)
    }

    /// Whether a stone may be filled. Empty or charged comes with the
    /// object itself, so no appraisal is waited for. A pair whose last use
    /// could not be accounted for is left alone.
    fn can_use_stone(&mut self, object_id: u32) -> bool {
        let mut store = HostDonorStore {
            host: &mut *self.host,
        };
        self.journal.can_operate(&mut store) && !self.journal.stone_held(object_id)
    }

    /// Whether an item may be emptied. How much mana it holds is known only
    /// from an appraisal, so this one waits for one.
    fn can_drain_tank(&mut self, object_id: u32) -> bool {
        let ready = {
            let mut store = HostDonorStore {
                host: &mut *self.host,
            };
            self.journal.donor_ready(&mut store, object_id)
        };
        ready && mosstank_automation::surface::is_assessed(self.host, object_id)
    }

    fn reads_writing(&self) -> bool {
        true
    }

    fn capture(&mut self, object_id: u32) -> Option<ItemProperties> {
        self.host.item_properties(object_id)
    }
}

/// A mana stone the profile knows by that name: an item the client classes
/// as a mana stone AND the profile's helper list names as one. The kind
/// matters, not merely the name.
pub(crate) fn is_profiled_mana_stone(
    kinds: &OrdinalStringMap<ConsumableCategory>,
    item: &InventoryItem,
) -> bool {
    item.object_class == ObjectClass::ManaStone
        && kinds.get(item.name.as_str()) == Some(&ConsumableCategory::ManaStone)
}

fn public_action(action: LootAction) -> ac_plugin_abi::items::LootAction {
    u8::try_from(action as i32)
        .ok()
        .and_then(ac_plugin_abi::items::LootAction::from_tag)
        .unwrap_or(ac_plugin_abi::items::LootAction::NoLoot)
}

impl LootController {
    /// A looter with these settings and no helper list.
    #[must_use]
    pub fn new(settings: LootSettings) -> Self {
        Self::with_consumables(settings, OrdinalStringSet::new(), OrdinalStringMap::new())
    }

    /// A looter with these settings and the profile's helper list (names,
    /// and each name's kind).
    #[must_use]
    pub fn with_consumables(
        settings: LootSettings,
        configured_consumable_names: OrdinalStringSet,
        configured_consumable_kinds: OrdinalStringMap<ConsumableCategory>,
    ) -> Self {
        Self {
            settings,
            warning: None,
            log: None,
            status: "Looting disabled.".to_owned(),
            configured_consumable_names,
            configured_consumable_kinds,
            warned_no_salvage_tool: false,
            completed_corpses: HashMap::new(),
            completed_own_rare_corpses: HashSet::new(),
            opened_own_rare_corpse: 0,
            corpse_open_attempts: HashMap::new(),
            corpse_blacklisted_at: HashMap::new(),
            item_attempts: HashMap::new(),
            pending_by_name: IgnoreCaseStringMap::new(),
            classified_owned_items: DotNetDictionary::new(),
            external_classifier_by_item: DotNetDictionary::new(),
            decisions: HashMap::new(),
            corpse_first_seen: HashMap::new(),
            corpse_last_seen: HashMap::new(),
            released_corpses: HashSet::new(),
            present_corpses: HashSet::new(),
            evicted_corpses: Vec::new(),
            corpse_denied_at: HashMap::new(),
            corpse_description_attempts: HashMap::new(),
            abandoned_items: HashSet::new(),
            refused_items: HashSet::new(),
            abandoned_corpse: 0,
            selected_corpse: 0,
            chat_sequence: 0,
            rare_announced_at: f64::NEG_INFINITY,
            state_age: 0.0,
            active_corpse: 0,
            active_corpse_saw_contents: false,
            active_corpse_is_own_death: false,
            waiting_item: 0,
            waiting_name: String::new(),
            waiting_action: LootAction::NoLoot,
            waiting_quantity: 0,
            waiting_item_snapshot: InventoryItem::default(),
            waiting_classifier_id: String::new(),
            waiting_pickup_accepted: false,
            waiting_pickup_completion_revision: 0,
            awaiting_appraisal: 0,
            awaiting_corpse_appraisal: 0,
            last_corpse_description_request: 0,
            lifetime: 0.0,
            pending_scroll_reads: DotNetDictionary::new(),
            salvage_pending_item: 0,
            salvage_pending_name: String::new(),
            salvage_attempts: 0,
            sell_pending_item: 0,
            sell_pending_name: String::new(),
            mana_transfer: None,
            mana_transfer_revision: 0,
            mana_transfer_hold_until: 0.0,
            combine_pending: None,
            combine_attempts: HashMap::new(),
            abandoned_combine_bags: HashSet::new(),
            action_locks: None,
            identify_age: 0.0,
            since_corpse_sighting: CORPSE_SIGHTING_INTERVAL_SECONDS,
            since_identify_request: IDENTIFY_REQUEST_INTERVAL_SECONDS,
            mana_donors: ManaDonorJournal::new(),
            pattern_warmup: (0, 0, 0),
        }
    }

    /// Compiles ahead, a few at a time, the patterns the rule list matches
    /// items with, so the first item decided after a profile load does not
    /// compile them all in one call. True when it compiled any.
    pub fn warm_rule_patterns(&mut self, budget: usize) -> bool {
        let rules = &self.settings.rules;
        let list = (rules.as_ptr() as usize, rules.len());
        if (self.pattern_warmup.0, self.pattern_warmup.1) != list {
            self.pattern_warmup = (list.0, list.1, 0);
        }
        let mut compiled = 0;
        while compiled < budget && self.pattern_warmup.2 < rules.len() {
            for requirement in &rules[self.pattern_warmup.2].vtank_requirements {
                compiled += crate::requirements::warm_patterns(requirement);
            }
            self.pattern_warmup.2 += 1;
        }
        compiled > 0
    }

    /// The profile's helper list again, after a load or an edit (the C#
    /// shares the profile's live collections with the looter).
    pub fn set_configured_consumables(
        &mut self,
        names: OrdinalStringSet,
        kinds: OrdinalStringMap<ConsumableCategory>,
    ) {
        self.configured_consumable_names = names;
        self.configured_consumable_kinds = kinds;
    }

    /// The names on the profile's helper list.
    #[must_use]
    pub fn configured_consumable_names(&self) -> &OrdinalStringSet {
        &self.configured_consumable_names
    }

    /// The shared action-lock table. A corpse is closed with an item use,
    /// and that use waits its turn behind whatever else is holding the item
    /// slot, the same way every other item rule does.
    pub fn bind_action_locks(&mut self, locks: SharedActionLocks) {
        self.action_locks = Some(locks);
    }

    /// What the looter took each owned item as.
    #[must_use]
    pub fn classified_owned_items(&self) -> &DotNetDictionary<u32, LootAction, Ordinal<u32>> {
        &self.classified_owned_items
    }

    /// The scrolls picked up for reading, spell id to item id. Reading them
    /// is a separate rule's job (`ReadScrollController`).
    #[must_use]
    pub fn pending_scroll_reads(&self) -> &DotNetDictionary<u32, u32, Ordinal<u32>> {
        &self.pending_scroll_reads
    }

    /// The mana donor journal.
    #[must_use]
    pub fn mana_donors(&self) -> &ManaDonorJournal {
        &self.mana_donors
    }

    /// True while a description this controller asked for is still
    /// outstanding. The appraisal slot holds one answer at a time, so a
    /// second reader of it can stand down on this.
    #[must_use]
    pub fn is_identifying(&self) -> bool {
        self.awaiting_appraisal != 0 || self.awaiting_corpse_appraisal != 0
    }

    pub(crate) fn write_log(&mut self, message: &str) {
        if let Some(log) = self.log.as_mut() {
            log(message);
        }
    }

    pub(crate) fn lock_held(&self, kind: ActionLockKind) -> Option<bool> {
        self.action_locks
            .as_ref()
            .map(|locks| locks.borrow_mut().is_locked(kind))
    }

    pub(crate) fn arm(&self, kind: ActionLockKind, seconds: f64) {
        if let Some(locks) = &self.action_locks {
            locks.borrow_mut().arm(kind, seconds);
        }
    }

    /// Whether the item slot is held: the lock table's answer when one is
    /// bound, otherwise the client's own busy flag.
    pub(crate) fn item_slot_held(&self, host: &mut dyn LootHost) -> bool {
        match self.lock_held(ActionLockKind::ItemUse) {
            Some(held) => held,
            None => host.items_busy(),
        }
    }

    fn classified_contains(&self, action: LootAction) -> bool {
        self.classified_owned_items
            .values()
            .any(|value| *value == action)
    }

    /// Drops the queued scrolls whose item has left the character's hands.
    /// A scroll that is still held stays queued however many times reading
    /// it has failed.
    pub fn forget_unowned_scroll_reads(&mut self, owned: &[InventoryItem]) {
        if self.pending_scroll_reads.is_empty() {
            return;
        }
        let queued: Vec<(u32, u32)> = self
            .pending_scroll_reads
            .iter()
            .map(|(spell, item)| (*spell, *item))
            .collect();
        for (spell_id, item_id) in queued {
            if !owned.iter().any(|item| item.object_id == item_id) {
                self.pending_scroll_reads.remove(&spell_id);
            }
        }
    }

    /// Drops the queued scrolls whose spell the character now knows.
    pub fn forget_known_scroll_reads(&mut self, host: &mut dyn LootHost) {
        if self.pending_scroll_reads.is_empty() {
            return;
        }
        let spells: Vec<u32> = self.pending_scroll_reads.keys().copied().collect();
        for spell_id in spells {
            if host.is_spell_known(spell_id) {
                self.pending_scroll_reads.remove(&spell_id);
            }
        }
    }

    /// The looter's turn in the rule pass; true when it holds the pass.
    pub fn tick(&mut self, host: &mut dyn LootHost, elapsed_seconds: f64, can_act: bool) -> bool {
        self.observe_mana_donors(host);
        if !self.settings.profile_active {
            self.reset_transient();
            "No loot profile is active.".clone_into(&mut self.status);
            return false;
        }
        if !self.settings.enabled {
            self.reset_transient();
            "Looting disabled.".clone_into(&mut self.status);
            return false;
        }
        if !host.is_available() || !host.loot_available() {
            self.reset(host);
            "Looting unavailable.".clone_into(&mut self.status);
            return false;
        }
        if self.settings.rules.is_empty()
            && casing::is_white_space_only(&self.settings.external_classifier_id)
        {
            self.reset_transient();
            "Loot profile has no rules.".clone_into(&mut self.status);
            return false;
        }

        // "Is a corpse open?" is re-answered from the client before anything
        // else this pass asks it, because the rest of the loot work is
        // barred while one is.
        let current = host.current_container_id();
        if self.active_corpse != 0 && current != self.active_corpse && self.waiting_item != 0 {
            let waiting = self.waiting_item;
            if host
                .owned_items_shared()
                .iter()
                .any(|item| item.object_id == waiting)
            {
                self.complete_waiting_pickup(host);
            } else {
                self.abandon_waiting_item("the corpse closed before it arrived");
            }
        }
        if self.active_corpse != 0
            && current != self.active_corpse
            && self.is_corpse_finished(self.active_corpse)
        {
            self.abandoned_corpse = 0;
            self.active_corpse = 0;
            self.active_corpse_saw_contents = false;
            self.active_corpse_is_own_death = false;
            self.state_age = 0.0;
        }

        if self.active_corpse == 0 && self.waiting_item == 0 {
            self.prune_removed_external_items(host);
        }

        let elapsed = elapsed_seconds.max(0.0);
        // An open still waiting for its container, held up behind somebody
        // else's item use, is not an open that is failing: the wait belongs
        // to the holder. Our own open holds the corpse-open slot as well,
        // which is what tells the two apart.
        let delayed_by_another_holder = self.active_corpse != 0
            && current != self.active_corpse
            && self.lock_held(ActionLockKind::ItemUse) == Some(true)
            && self.lock_held(ActionLockKind::CorpseOpenAttempt) == Some(false);
        if !delayed_by_another_holder {
            self.state_age += elapsed;
        }
        self.lifetime += elapsed;
        self.observe_ownership_denials(host);

        // The reference's open rule holds the pass doing nothing for as long
        // as the item slot is held. A drain this controller already issued
        // is the one thing read through a held slot: the slot it waits
        // behind is its own.
        if self.mana_transfer.is_none() && self.lock_held(ActionLockKind::ItemUse) == Some(true) {
            self.status = if self.waiting_item != 0 {
                format!("Waiting for {}…", self.waiting_name)
            } else if self.active_corpse != 0 && current != self.active_corpse {
                "Waiting for corpse contents…".to_owned()
            } else {
                "Waiting for the item slot…".to_owned()
            };
            return true;
        }

        if (self.mana_transfer.is_some() || self.has_mana_stone_transfer(host))
            && self.continue_mana_stone_transfer(host, can_act)
        {
            return true;
        }
        if self.active_corpse == 0
            && (self.salvage_pending_item != 0 || self.classified_contains(LootAction::Salvage))
            && self.continue_salvage(host, can_act)
        {
            return true;
        }
        if self.active_corpse == 0
            && self.settings.combine_salvage
            && (self.combine_pending.is_some() || self.has_salvage_bag_combine(host))
            && self.continue_salvage_bag_combine(host, can_act)
        {
            return true;
        }
        if self.active_corpse == 0
            && (self.sell_pending_item != 0 || self.classified_contains(LootAction::Sell))
            && self.continue_sell(host, can_act)
        {
            return true;
        }

        if self.active_corpse != 0 && current == self.active_corpse {
            return self.continue_current_corpse(host, can_act);
        }

        if self.active_corpse != 0 && host.requested_container_id() == self.active_corpse {
            if self.state_age <= self.settings.corpse_open_timeout_seconds.max(0.25) {
                "Waiting for corpse contents…".clone_into(&mut self.status);
                return true;
            }
            let failed_corpse = self.active_corpse;
            self.blacklist_failed_corpse(failed_corpse);
            self.active_corpse = 0;
            self.active_corpse_saw_contents = false;
            self.active_corpse_is_own_death = false;
            self.state_age = 0.0;
            if self.is_corpse_blacklisted(failed_corpse) {
                return false;
            }
        }

        if !can_act {
            return false;
        }

        // Every pass: the reference selects afresh each time it is asked.
        let known = host.loot_corpses(f32::MAX);
        self.prune_corpse_cache(&known);

        // Descriptions are the frame's business (tick_identification). Only a
        // described corpse within arm's reach is ready work here; the walk is
        // its own rule, one position above this one.
        self.selected_corpse = 0;
        if let Some(corpse) =
            self.select_corpse(host, &known, crate::CORPSE_OPEN_RANGE_METERS, true, false)
        {
            self.selected_corpse = corpse.object_id;
            let opened = host.loot_open(corpse.object_id);
            if !opened.accepted() {
                if opened.status == ItemStatus::Busy {
                    "Waiting to open corpse…".clone_into(&mut self.status);
                    return true;
                }
                // The reference counts every open attempt, refused or not,
                // and arms the same three slots after every attempt, which
                // is what paces the retries.
                self.status = format!(
                    "Could not open {} ({}).",
                    corpse.name,
                    status_name(opened.status)
                );
                self.write_log(&format!(
                    "LootCorpse: open of {} (0x{:08X}) refused: {}",
                    corpse.name,
                    corpse.object_id,
                    status_name(opened.status)
                ));
                self.blacklist_failed_corpse(corpse.object_id);
                self.arm_corpse_open_slots();
                return false;
            }
            self.active_corpse = corpse.object_id;
            self.active_corpse_saw_contents = false;
            self.active_corpse_is_own_death = self.is_own_death_corpse(host, &corpse);
            self.opened_own_rare_corpse = if self.is_own_rare_corpse(host, &corpse) {
                corpse.object_id
            } else {
                0
            };
            self.refused_items.clear();
            self.state_age = 0.0;
            // The open holds the item slot, the corpse-open slot and
            // navigation until the container actually opens; the timeout is
            // the ceiling for an open that never lands.
            self.arm_corpse_open_slots();
            self.status = format!("Opening {}…", corpse.name);
            self.write_log(&format!(
                "LootCorpse: opening {} (0x{:08X})",
                corpse.name, corpse.object_id
            ));
            return true;
        }

        // A sale still queued for a vendor keeps its own status line.
        if self.sell_pending_item == 0 && !self.classified_contains(LootAction::Sell) {
            "No nearby corpses.".clone_into(&mut self.status);
        }
        false
    }

    pub(crate) fn can_process_loot(&self, host: &mut dyn LootHost) -> bool {
        self.settings.profile_active
            && self.settings.enabled
            && host.is_available()
            && (!self.settings.rules.is_empty()
                || !casing::is_white_space_only(&self.settings.external_classifier_id))
    }

    /// Asks for descriptions and item ids on the host's frame rather than
    /// on the loot rule's turn: every corpse on the radar as it appears, and
    /// every item of an opened corpse as the contents arrive, one request
    /// at a time on the reference's cadence.
    pub fn tick_identification(&mut self, host: &mut dyn LootHost, elapsed_seconds: f64) {
        if !self.can_process_loot(host) {
            return;
        }
        if !host.loot_available() {
            return;
        }
        // The chat is read on the frame as well as on the rule's turn, so a
        // rare announcement is known before the pass that follows it.
        self.observe_ownership_denials(host);
        let elapsed = elapsed_seconds.max(0.0);
        self.identify_age += elapsed;
        self.since_identify_request += elapsed;
        if self.active_corpse != 0 && host.current_container_id() == self.active_corpse {
            self.tick_corpse_item_identification(host);
            return;
        }
        // The item latch belongs to the corpse that is open: with none open
        // there is no item left to be waiting for.
        self.awaiting_appraisal = 0;
        if self.awaiting_corpse_appraisal != 0 {
            let appraisal = host.appraisal();
            let awaiting = self.awaiting_corpse_appraisal;
            let answered = appraisal.current_id == awaiting && appraisal.awaiting_id != awaiting;
            // The client says outright when it has given up on a question.
            let dropped = appraisal.last_abandoned_id == awaiting;
            let expired =
                self.identify_age >= (self.settings.corpse_open_timeout_seconds * 2.0).max(1.0);
            if !answered && !dropped && !expired {
                return;
            }
            if !answered {
                self.note_corpse_description_failed(awaiting);
            }
            self.awaiting_corpse_appraisal = 0;
            self.identify_age = 0.0;
        }
        // Rare-only looting has nothing to describe until the server
        // announces this character's rare; it only notes, every couple of
        // seconds, when each corpse first appeared.
        if self.settings.loot_only_rare_corpses {
            self.since_corpse_sighting += elapsed;
            if self.since_corpse_sighting >= CORPSE_SIGHTING_INTERVAL_SECONDS {
                self.since_corpse_sighting = 0.0;
                let known = host.loot_corpses(f32::MAX);
                self.prune_corpse_cache(&known);
            }
        }
        if !self.rare_window_open_or_not_rare_only() {
            return;
        }
        let known = host.loot_corpses(f32::MAX);
        let _ = self.try_request_next_corpse_description(host, &known);
    }

    /// The item half of the frame's identification: while a corpse is open,
    /// each item that needs an id is asked for one, and each answer becomes
    /// a decision the rule's next turn can act on. Items that need no id are
    /// decided as they are seen.
    fn tick_corpse_item_identification(&mut self, host: &mut dyn LootHost) {
        if !host.current_contents_ready() {
            return;
        }
        let contents = host.loot_contents();
        let owned = host.owned_items_shared();
        if self.awaiting_appraisal != 0 {
            let appraisal = host.appraisal();
            let awaiting = self.awaiting_appraisal;
            if appraisal.current_id == awaiting && appraisal.awaiting_id != awaiting {
                if let Some(answered) = contents
                    .iter()
                    .find(|item| item.object_id == awaiting)
                    .filter(|item| item.object_id != 0)
                {
                    let identified = host.loot_properties(answered.object_id).unwrap_or_default();
                    let (decision, refusal) = self.decide_item(host, answered, &identified, &owned);
                    self.record_decision(answered, decision, &refusal);
                }
                self.awaiting_appraisal = 0;
                self.identify_age = 0.0;
            } else if appraisal.last_abandoned_id != awaiting
                && self.identify_age
                    < clamp(
                        self.settings.corpse_item_identify_timeout_seconds,
                        1.0,
                        600.0,
                    )
            {
                // A question the client has already given up on is a
                // failure now, not a wait to sit out.
                return;
            } else {
                self.increment_attempt(awaiting);
                self.awaiting_appraisal = 0;
                self.identify_age = 0.0;
            }
        }

        // One item a frame (register LOOT-7): each is weighed against
        // the whole rule list, and a full corpse against a large profile is
        // more than one plugin call should run.
        let mut weighed = 0;
        for item in &contents {
            if self.decisions.contains_key(&item.object_id) {
                continue;
            }
            if weighed >= ITEMS_WEIGHED_PER_FRAME {
                return;
            }
            weighed += 1;
            let properties = host.loot_properties(item.object_id).unwrap_or_default();
            if !self.needs_identify(host, item, &properties, &owned) {
                let (decision, refusal) = self.decide_item(host, item, &properties, &owned);
                self.record_decision(item, decision, &refusal);
                continue;
            }
            if self.since_identify_request < IDENTIFY_REQUEST_INTERVAL_SECONDS {
                return;
            }
            let identify = host.loot_identify(item.object_id);
            if identify.accepted() {
                self.awaiting_appraisal = item.object_id;
                self.identify_age = 0.0;
                self.since_identify_request = 0.0;
            }
            // Busy or refused: asked again on a later frame.
            return;
        }
    }

    /// Forgets everything about corpses and items (the reset the panel's
    /// start, a profile change and leaving the world make).
    pub fn reset(&mut self, host: &mut dyn LootHost) {
        let notices: Vec<(u32, String)> = self
            .external_classifier_by_item
            .iter()
            .map(|(id, classifier)| (*id, classifier.clone()))
            .collect();
        for (object_id, classifier_id) in notices {
            host.notify_item_removed(&classifier_id, object_id);
        }
        self.reset_transient();
        self.completed_corpses.clear();
        if self.settings.walk_to_own_rare_corpses {
            for own_rare in &self.completed_own_rare_corpses {
                self.completed_corpses.insert(*own_rare, 0.0);
            }
        } else {
            self.completed_own_rare_corpses.clear();
        }
        self.corpse_open_attempts.clear();
        self.corpse_description_attempts.clear();
        self.corpse_blacklisted_at.clear();
        self.item_attempts.clear();
        self.abandoned_items.clear();
        self.refused_items.clear();
        self.abandoned_corpse = 0;
        self.pending_by_name.clear();
        self.classified_owned_items.clear();
        self.external_classifier_by_item.clear();
        self.decisions.clear();
        self.corpse_first_seen.clear();
        self.corpse_last_seen.clear();
        self.released_corpses.clear();
        self.present_corpses.clear();
        self.evicted_corpses.clear();
        self.corpse_denied_at.clear();
        self.pending_scroll_reads.clear();
        self.selected_corpse = 0;
        // The chat already written is not read again: replaying it re-opened
        // a rare announcement's window and re-applied old refusals.
        self.chat_sequence = Self::latest_chat_sequence(host);
        self.rare_announced_at = f64::NEG_INFINITY;
        self.lifetime = 0.0;
        self.salvage_pending_item = 0;
        self.salvage_pending_name.clear();
        self.salvage_attempts = 0;
        self.sell_pending_item = 0;
        self.sell_pending_name.clear();
        self.mana_transfer = None;
        self.mana_transfer_revision = 0;
        self.mana_transfer_hold_until = 0.0;
        self.combine_pending = None;
        self.combine_attempts.clear();
        self.abandoned_combine_bags.clear();
        self.status = if self.settings.enabled {
            "Idle.".to_owned()
        } else {
            "Looting disabled.".to_owned()
        };
    }

    fn continue_current_corpse(&mut self, host: &mut dyn LootHost, can_act: bool) -> bool {
        // Once the item pass is done the corpse is marked looted and the
        // only thing left is the closing use, retried every pass until the
        // container actually shuts.
        if self.is_corpse_finished(self.active_corpse) {
            return self.close_finished_corpse(host, self.active_corpse, can_act);
        }

        if !host.current_contents_ready() {
            "Waiting for corpse item data…".clone_into(&mut self.status);
            return true;
        }

        let contents = host.loot_contents();
        if !contents.is_empty() {
            self.active_corpse_saw_contents = true;
        }
        if contents.is_empty()
            && !self.active_corpse_saw_contents
            && self.state_age
                < clamp(
                    self.settings.corpse_item_appearance_timeout_seconds,
                    0.0,
                    300.0,
                )
        {
            "Waiting for corpse items to appear…".clone_into(&mut self.status);
            return true;
        }

        self.observe_pickup(host, &contents);

        let waiting = self.waiting_item;
        let waiting_in_corpse = contents.iter().any(|item| item.object_id == waiting);
        if waiting != 0 && self.waiting_pickup_accepted && waiting_in_corpse && host.items_busy() {
            self.status = format!("Waiting for {}…", self.waiting_name);
            return true;
        }

        // Once the host is no longer busy, the corpse contents decide whether
        // a pull succeeded. Retire a receipt at its retry ceiling before
        // choosing another item, or the next accepted pull would inherit the
        // old receipt.
        let maximum_attempts = self.settings.corpse_loot_item_max_attempts.clamp(1, 100);
        let waiting = self.waiting_item;
        if waiting != 0
            && let Some(&waiting_attempts) = self.item_attempts.get(&waiting)
            && waiting_attempts >= maximum_attempts
            && contents.iter().any(|item| item.object_id == waiting)
        {
            self.abandoned_items.insert(waiting);
            self.abandon_waiting_item(&format!("{waiting_attempts} attempts spent"));
        }

        // Items still waiting on their description are the frame's business;
        // this turn works from the decisions already made.
        let mut identifying = false;
        let mut candidates: Vec<(&InventoryItem, LootDecision)> = Vec::new();
        for item in &contents {
            let Some(cached) = self.decisions.get(&item.object_id).cloned() else {
                identifying = true;
                continue;
            };
            // An item the client already refused for this corpse is out of
            // the pass; the refusal said its one line when it happened.
            if self.refused_items.contains(&item.object_id) {
                continue;
            }
            if let Some(&attempts) = self.item_attempts.get(&item.object_id)
                && attempts >= maximum_attempts
            {
                if self.abandoned_items.insert(item.object_id) {
                    self.write_log(&format!(
                        "LootPickup: abandoned {}: {attempts} attempts spent",
                        item.name
                    ));
                }
                continue;
            }
            if let Some(decision) = cached {
                candidates.push((item, decision));
            }
        }

        if candidates.is_empty() {
            if identifying {
                if self.state_age
                    < clamp(
                        self.settings.corpse_item_identify_timeout_seconds,
                        1.0,
                        600.0,
                    )
                {
                    "Identifying corpse items…".clone_into(&mut self.status);
                    return true;
                }
                self.write_log(&format!(
                    "CorpseWait: abandoned 0x{:08X}, unable to receive an id for every item",
                    self.active_corpse
                ));
            }
            for item in &contents {
                self.decisions.remove(&item.object_id);
            }
            let finished = self.active_corpse;
            // A corpse whose wanted items were all refused was never emptied,
            // so it is set aside for another visit rather than recorded as
            // looted, and it says so once.
            if !self.refused_items.is_empty() {
                if self.abandoned_corpse != finished {
                    self.give_up_on_corpse(
                        finished,
                        "the client would not take any of what is in it",
                    );
                }
            } else {
                self.mark_corpse_complete(finished);
            }
            self.state_age = 0.0;
            self.write_log(&format!("CorpseWait: closing 0x{finished:08X}"));
            return self.close_finished_corpse(host, finished, can_act);
        }
        if !can_act {
            return true;
        }

        let waiting = self.waiting_item;
        if waiting != 0
            && self.waiting_pickup_accepted
            && let Some(item) = contents.iter().find(|item| item.object_id == waiting)
            && let Some(Some(decision)) = self.decisions.get(&waiting).cloned()
        {
            let item = item.clone();
            self.issue_pickup(host, &item, &decision);
            return true;
        }

        candidates.sort_by(|left, right| {
            right
                .1
                .priority
                .cmp(&left.1.priority)
                .then_with(|| left.1.rule_index.cmp(&right.1.rule_index))
                .then_with(|| left.0.container_slot.cmp(&right.0.container_slot))
                .then_with(|| left.0.object_id.cmp(&right.0.object_id))
        });
        let (item, decision) = candidates.swap_remove(0);
        let item = item.clone();
        self.issue_pickup(host, &item, &decision)
    }

    fn issue_pickup(
        &mut self,
        host: &mut dyn LootHost,
        item: &InventoryItem,
        decision: &LootDecision,
    ) -> bool {
        let pickup = host.loot_pickup(item.object_id);

        // Every pull holds the item slot and navigation for three quarters
        // of a second, whether or not it went out: the item slot paces the
        // pulls, and navigation stops the walk-to-a-corpse rule steering the
        // character away from the corpse it is emptying.
        self.arm(ActionLockKind::ItemUse, PICKUP_HOLD_SECONDS);
        self.arm(ActionLockKind::Navigation, PICKUP_HOLD_SECONDS);

        if pickup.status == ItemStatus::Busy {
            // Busy is the client's own one-request-at-a-time gate: it never
            // looked at this item, so this is not an attempt on it.
            if self.waiting_item == item.object_id {
                self.waiting_pickup_accepted = false;
            }
            self.status = format!("Waiting to take {}…", item.name);
            return true;
        }

        // The pull is counted when it is issued, as the reference counts it.
        self.increment_attempt(item.object_id);

        if !pickup.accepted() {
            // The client decided against sending this one, so this item is
            // retired from the corpse's pass rather than asked again.
            let notice = match pickup.notice.as_deref() {
                Some(notice) if !casing::is_white_space_only(notice) => notice.to_owned(),
                _ => status_name(pickup.status).to_owned(),
            };
            if self.refused_items.insert(item.object_id) {
                self.write_log(&format!("LootPickup: refused {}: {notice}", item.name));
            }
            if self.waiting_item == item.object_id {
                self.release_waiting_reservation();
                self.clear_waiting_item();
            }
            self.status = format!(
                "Pickup refused: {} ({}).",
                item.name,
                status_name(pickup.status)
            );
            // A pack with nowhere to put anything is the one refusal that
            // does end the corpse, bar a stack that merges into one held.
            if self.is_pack_full(host, pickup.notice.as_deref()) {
                self.retire_what_cannot_fit(host, &notice);
            }
            return true;
        }

        if self.waiting_item == 0 {
            self.waiting_item = item.object_id;
            self.waiting_name.clone_from(&item.name);
            self.waiting_action = decision.action;
            self.waiting_quantity = item.stack_size.max(1);
            self.waiting_item_snapshot = item.clone();
            self.waiting_classifier_id
                .clone_from(&decision.classifier_id);
            if decision.action == LootAction::KeepUpTo {
                let pending = self
                    .pending_by_name
                    .get(item.name.as_str())
                    .copied()
                    .unwrap_or(0);
                self.pending_by_name
                    .insert(item.name.clone(), pending + self.waiting_quantity);
            }
        }
        self.waiting_pickup_accepted = true;
        self.waiting_pickup_completion_revision = host.last_inventory_completion().revision;
        self.status = format!("Looting {} ({})…", item.name, decision.rule_name);
        self.write_log(&format!(
            "LootPickup: taking {} x{} ({:?}, {})",
            item.name, self.waiting_quantity, decision.action, decision.rule_name
        ));
        true
    }

    /// Whether the client's refusal means there is nowhere left to put
    /// anything: the notice names the room, or no loose-item slot is free.
    fn is_pack_full(&self, host: &mut dyn LootHost, notice: Option<&str>) -> bool {
        notice.is_some_and(|notice| {
            !notice.is_empty() && ordinal::contains_ignore_case(notice, "room")
        }) || Self::free_item_slots(host) == 0
    }

    /// How many loose-item slots the character has left, counting the main
    /// pack and every pack inside it; unknown when the client has not
    /// described the character's own container, or any pack in it.
    fn free_item_slots(host: &mut dyn LootHost) -> i32 {
        let player_id = host.character().id;
        let Some(player) = (player_id != 0).then(|| host.object(player_id)).flatten() else {
            return UNKNOWN_FREE_SLOTS;
        };
        if player.items_capacity <= 0 {
            return UNKNOWN_FREE_SLOTS;
        }
        let all = host.objects();
        let mut free = free_slots_in(player.object_id, player.items_capacity, &all);
        for held in &all {
            if held.container_id != player_id || held.object_class != ObjectClass::Container {
                continue;
            }
            if held.items_capacity <= 0 {
                return UNKNOWN_FREE_SLOTS;
            }
            free += free_slots_in(held.object_id, held.items_capacity, &all);
        }
        free
    }

    /// Whether the item would go into a stack the character already holds
    /// rather than needing a slot of its own.
    fn merges_into_held_stack(host: &mut dyn LootHost, item: &InventoryItem) -> bool {
        if item.maximum_stack_size <= 1 {
            return false;
        }
        host.owned_items_shared().iter().any(|held| {
            held.weenie_class_id == item.weenie_class_id
                && held.maximum_stack_size > 1
                && held.stack_size.max(1) < held.maximum_stack_size
        })
    }

    /// Ends the corpse once the client has said there is nowhere to put
    /// anything: everything left that would need a slot of its own is
    /// retired without being asked for.
    fn retire_what_cannot_fit(&mut self, host: &mut dyn LootHost, reason: &str) {
        let mut anything_still_fits = false;
        for item in host.loot_contents() {
            if self.refused_items.contains(&item.object_id) {
                continue;
            }
            if Self::merges_into_held_stack(host, &item) {
                anything_still_fits = true;
                continue;
            }
            self.refused_items.insert(item.object_id);
        }
        if !anything_still_fits {
            self.give_up_on_corpse(
                self.active_corpse,
                &format!("nothing left in it will fit: {reason}"),
            );
        }
    }

    /// Stores a decision and says what it was, or why there was none.
    fn record_decision(
        &mut self,
        item: &InventoryItem,
        decision: Option<LootDecision>,
        refusal: &LootRefusal,
    ) {
        let line = match &decision {
            Some(chosen) => format!(
                "LootDecision: {} -> {:?} ({})",
                item.name, chosen.action, chosen.rule_name
            ),
            None => format!("LootDecision: {} -> {}", item.name, refusal.describe()),
        };
        self.decisions.insert(item.object_id, decision);
        self.write_log(&line);
    }

    /// The pull issued last turn, judged by the corpse: an item no longer in
    /// the container was taken, one still in it is pulled again.
    fn observe_pickup(&mut self, host: &mut dyn LootHost, contents: &[InventoryItem]) {
        if self.waiting_item == 0 {
            return;
        }
        let completion = host.last_inventory_completion();
        if self.waiting_pickup_accepted
            && completion.revision > self.waiting_pickup_completion_revision
            && matches!(
                completion.kind,
                ac_plugin_abi::items::InventoryCommandKind::Pickup
                    | ac_plugin_abi::items::InventoryCommandKind::Merge
            )
            && completion.source_id == self.waiting_item
            && completion.weenie_error != 0
        {
            self.waiting_pickup_accepted = false;
        }
        let waiting = self.waiting_item;
        if contents.iter().any(|item| item.object_id == waiting) {
            self.status = format!("Retrying {}.", self.waiting_name);
        } else {
            self.complete_waiting_pickup(host);
        }
    }

    fn complete_waiting_pickup(&mut self, host: &mut dyn LootHost) {
        let waiting = self.waiting_item;
        // An item taken for its mana keeps what it was taken as unless the
        // second look, now that it is in hand, says somebody wrote on it or
        // tinkered it. A description that cannot be read yet is not that
        // verdict.
        let keeps_classification = self.waiting_action != LootAction::ManaTank
            || host
                .item_properties(waiting)
                .is_none_or(|written| mana::is_drainable_in_hand(&written));
        if keeps_classification {
            self.classified_owned_items
                .insert(waiting, self.waiting_action);
            if self.waiting_action == LootAction::ManaTank {
                let snapshot = self.waiting_item_snapshot.clone();
                self.remember_mana_donor(host, &snapshot);
            }
        }
        if !self.waiting_classifier_id.is_empty() {
            self.external_classifier_by_item
                .insert(waiting, self.waiting_classifier_id.clone());
            host.notify_looted(
                &self.waiting_classifier_id.clone(),
                LootedItem {
                    item: self.waiting_item_snapshot.clone(),
                    action: public_action(self.waiting_action),
                },
            );
        }
        self.decisions.remove(&waiting);
        self.status = format!("Looted {}.", self.waiting_name);
        let line = format!(
            "LootPickup: took {} ({:?})",
            self.waiting_name, self.waiting_action
        );
        self.write_log(&line);
        self.item_attempts.remove(&waiting);
        self.abandoned_items.remove(&waiting);
        if self.waiting_action == LootAction::Read && self.waiting_item_snapshot.spell_id != 0 {
            self.pending_scroll_reads
                .try_add(self.waiting_item_snapshot.spell_id, waiting);
        }
        self.release_waiting_reservation();
        self.clear_waiting_item();
    }

    /// Lets go of the item the pass was waiting on and says why. Every way
    /// an item leaves the pass without arriving goes through here.
    pub(crate) fn abandon_waiting_item(&mut self, reason: &str) {
        if self.waiting_item != 0 {
            let line = format!("LootPickup: abandoned {}: {reason}", self.waiting_name);
            self.write_log(&line);
        }
        self.release_waiting_reservation();
        self.clear_waiting_item();
    }

    fn release_waiting_reservation(&mut self) {
        if self.waiting_action != LootAction::KeepUpTo {
            return;
        }
        let Some(&pending) = self.pending_by_name.get(self.waiting_name.as_str()) else {
            return;
        };
        if pending <= self.waiting_quantity {
            self.pending_by_name.remove(self.waiting_name.as_str());
        } else {
            self.pending_by_name
                .insert(self.waiting_name.clone(), pending - self.waiting_quantity);
        }
    }

    fn clear_waiting_item(&mut self) {
        self.waiting_item = 0;
        self.waiting_name.clear();
        self.waiting_action = LootAction::NoLoot;
        self.waiting_quantity = 0;
        self.waiting_item_snapshot = InventoryItem::default();
        self.waiting_classifier_id.clear();
        self.waiting_pickup_accepted = false;
        self.waiting_pickup_completion_revision = 0;
    }

    /// The first owned item classified with `action`, in the order the
    /// classifications were made.
    fn first_classified_owned(&self, action: LootAction, owned: &[InventoryItem]) -> u32 {
        self.classified_owned_items
            .iter()
            .filter(|(_, value)| **value == action)
            .map(|(id, _)| *id)
            .find(|id| owned.iter().any(|item| item.object_id == *id))
            .unwrap_or(0)
    }

    fn forget_stale_classified(
        &mut self,
        host: &mut dyn LootHost,
        action: LootAction,
        owned: &[InventoryItem],
    ) {
        let stale: Vec<u32> = self
            .classified_owned_items
            .iter()
            .filter(|(id, value)| {
                **value == action && !owned.iter().any(|item| item.object_id == **id)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            self.remove_classified_owned(host, id);
        }
    }

    fn continue_salvage(&mut self, host: &mut dyn LootHost, can_act: bool) -> bool {
        let owned = host.owned_items_shared();
        if self.salvage_pending_item != 0 {
            let pending = self.salvage_pending_item;
            if !owned.iter().any(|item| item.object_id == pending) {
                self.remove_classified_owned(host, pending);
                self.status = format!("Salvaged {}.", self.salvage_pending_name);
                self.salvage_pending_item = 0;
                self.salvage_pending_name.clear();
                self.salvage_attempts = 0;
                self.state_age = 0.0;
                return true;
            }
            if self.state_age < PICKUP_TIMEOUT_SECONDS {
                self.status = format!("Salvaging {}…", self.salvage_pending_name);
                return true;
            }
            self.salvage_pending_item = 0;
            self.salvage_pending_name.clear();
            self.state_age = 0.0;
            self.salvage_attempts += 1;
        }

        self.forget_stale_classified(host, LootAction::Salvage, &owned);
        let source_id = self.first_classified_owned(LootAction::Salvage, &owned);
        if source_id == 0 {
            return false;
        }
        if !can_act || !host.items_available() || host.items_busy() {
            return true;
        }

        let Some(tool) = owned
            .iter()
            .find(|item| item.item_type & TINKERING_TOOL != 0)
            .filter(|tool| tool.object_id != 0)
        else {
            // The rest of the loot work carries on; the items stay marked
            // for salvage and go the moment a tool is in the pack. Said
            // once, and again only after a tool has come and gone.
            "Salvage action is waiting for a salvage tool.".clone_into(&mut self.status);
            if !self.warned_no_salvage_tool {
                self.warned_no_salvage_tool = true;
                if let Some(warning) = self.warning.as_mut() {
                    warning(
                        "No Ust in your inventory: items marked for salvage are kept until you carry one.",
                    );
                }
            }
            return false;
        };
        self.warned_no_salvage_tool = false;

        let source_name = owned
            .iter()
            .find(|item| item.object_id == source_id)
            .map(|item| item.name.clone())
            .unwrap_or_default();
        let result = host.salvage(tool.object_id, &[source_id]);
        if !result.accepted() {
            self.status = if result.status == ItemStatus::Busy {
                "Waiting to salvage…".to_owned()
            } else {
                format!("Could not salvage {source_name}.")
            };
            return result.status == ItemStatus::Busy;
        }
        self.salvage_pending_item = source_id;
        self.salvage_pending_name.clone_from(&source_name);
        self.state_age = 0.0;
        self.status = format!("Salvaging {source_name}…");
        true
    }

    fn continue_sell(&mut self, host: &mut dyn LootHost, can_act: bool) -> bool {
        let owned = host.owned_items_shared();
        if self.sell_pending_item != 0 {
            let pending = self.sell_pending_item;
            if !owned.iter().any(|item| item.object_id == pending) {
                self.remove_classified_owned(host, pending);
                self.status = format!("Sold {}.", self.sell_pending_name);
                self.sell_pending_item = 0;
                self.sell_pending_name.clear();
                self.state_age = 0.0;
                return true;
            }
            if self.state_age < PICKUP_TIMEOUT_SECONDS {
                self.status = format!("Selling {}…", self.sell_pending_name);
                return true;
            }
            self.sell_pending_item = 0;
            self.sell_pending_name.clear();
            self.state_age = 0.0;
        }

        self.forget_stale_classified(host, LootAction::Sell, &owned);
        let source_id = self.first_classified_owned(LootAction::Sell, &owned);
        if source_id == 0 {
            return false;
        }
        if host.active_vendor_id() == 0 {
            "Sell loot is queued until a vendor is open.".clone_into(&mut self.status);
            return false;
        }
        if !can_act || !host.items_available() || host.items_busy() {
            return true;
        }

        let source_name = owned
            .iter()
            .find(|item| item.object_id == source_id)
            .map(|item| item.name.clone())
            .unwrap_or_default();
        let result = host.sell(source_id);
        if !result.accepted() {
            self.status = if result.status == ItemStatus::Busy {
                "Waiting to sell…".to_owned()
            } else {
                result
                    .notice
                    .clone()
                    .unwrap_or_else(|| format!("Could not sell {source_name}."))
            };
            return result.status == ItemStatus::Busy;
        }
        self.sell_pending_item = source_id;
        self.sell_pending_name.clone_from(&source_name);
        self.state_age = 0.0;
        self.status = format!("Selling {source_name}…");
        true
    }

    fn plan_mana_transfer(&self, host: &mut dyn LootHost) -> Option<ManaStoneTransferPlan> {
        let owned = host.owned_items_shared();
        let classified = &self.classified_owned_items;
        let mut checks = ControllerChecks {
            host,
            journal: &self.mana_donors,
            kinds: &self.configured_consumable_kinds,
        };
        mana::plan(
            &owned,
            &|id| classified.get(&id).copied(),
            self.settings.mana_tank_minimum_mana,
            &mut checks,
        )
    }

    fn has_mana_stone_transfer(&self, host: &mut dyn LootHost) -> bool {
        if !self.classified_contains(LootAction::ManaTank) {
            return false;
        }
        self.plan_mana_transfer(host).is_some()
    }

    fn continue_mana_stone_transfer(&mut self, host: &mut dyn LootHost, can_act: bool) -> bool {
        if let Some(pending) = self.mana_transfer.clone() {
            let completion = host.last_item_completion();
            let outcome = mana::observe_fill(
                &pending,
                &host.owned_items_shared(),
                &completion,
                self.mana_transfer_revision,
                self.state_age >= PICKUP_TIMEOUT_SECONDS,
            );
            if outcome == ManaFillOutcome::Waiting {
                self.status = format!("Waiting for {} charge confirmation…", pending.stone_name);
                return true;
            }
            if outcome == ManaFillOutcome::Confirmed {
                self.mana_donors.forget(
                    &mut HostDonorStore { host: &mut *host },
                    pending.tank_object_id,
                );
                self.remove_classified_owned(host, pending.tank_object_id);
                self.status = format!("Filled {}.", pending.stone_name);
                self.write_log(&format!(
                    "ManaDrain: filled {} from {}",
                    pending.stone_name, pending.tank_name
                ));
                host.identify(pending.stone_object_id);
            } else {
                // Keep the donor reserved, but do not repeat an uncertain
                // destructive use.
                self.status = format!(
                    "Mana fill unconfirmed; holding {} and {}.",
                    pending.stone_name, pending.tank_name
                );
                self.write_log(&format!(
                    "ManaDrain: no answer for {} on {}; leaving both alone",
                    pending.stone_name, pending.tank_name
                ));
            }
            let outcome_name = match outcome {
                ManaFillOutcome::Waiting => "Waiting",
                ManaFillOutcome::Confirmed => "Confirmed",
                ManaFillOutcome::Uncertain => "Uncertain",
            };
            let error = host.last_item_completion().weenie_error;
            host.log_info(&format!(
                "Mana stone fill: {outcome_name}, source={} (0x{:08X}), donor={} (0x{:08X}), error=0x{error:08X}",
                pending.stone_name,
                pending.stone_object_id,
                pending.tank_name,
                pending.tank_object_id
            ));
            self.release_mana_transfer_hold();
            self.mana_transfer = None;
            self.mana_transfer_revision = 0;
            self.state_age = 0.0;
            return true;
        }
        if !can_act || !host.items_available() || host.items_busy() {
            return true;
        }

        let Some(next) = self.plan_mana_transfer(host) else {
            return false;
        };
        if !self
            .mana_donors
            .begin(&mut HostDonorStore { host: &mut *host }, &next)
        {
            "Mana donor history unavailable; draining paused.".clone_into(&mut self.status);
            return false;
        }
        let result = host.apply_item(next.stone_object_id, next.tank_object_id);
        if !result.accepted() {
            if matches!(result.status, ItemStatus::Busy | ItemStatus::Unavailable) {
                self.mana_donors.not_sent(
                    &mut HostDonorStore { host: &mut *host },
                    next.tank_object_id,
                    None,
                );
                "Waiting to fill mana stone…".clone_into(&mut self.status);
                return true;
            }
            // Refused outright: nothing was sent and nothing changed, so the
            // donor remains recorded for review, but it cannot be selected
            // again.
            let reason = result
                .notice
                .clone()
                .unwrap_or_else(|| status_name(result.status).to_owned());
            self.mana_donors.not_sent(
                &mut HostDonorStore { host: &mut *host },
                next.tank_object_id,
                Some(&reason),
            );
            self.status = format!("Could not use {} on {}.", next.stone_name, next.tank_name);
            self.write_log(&format!(
                "ManaDrain: could not empty {} into {} ({}: {}); retaining it for review",
                next.tank_name,
                next.stone_name,
                status_name(result.status),
                result.notice.as_deref().unwrap_or("")
            ));
            return false;
        }
        host.log_info(&format!(
            "Mana stone fill request: source={} (0x{:08X}), donor={} (0x{:08X})",
            next.stone_name, next.stone_object_id, next.tank_name, next.tank_object_id
        ));
        self.write_log(&format!(
            "ManaDrain: emptying {} into {}",
            next.tank_name, next.stone_name
        ));
        self.status = format!("Filling {} from {}…", next.stone_name, next.tank_name);
        self.mana_transfer = Some(next);
        self.mana_transfer_revision = host.last_item_completion().revision;
        self.state_age = 0.0;
        // The drain is an item use like any other and the character's hands
        // are full until the server answers for it.
        self.arm(ActionLockKind::ItemUse, item_use_lock::TRANSACTION_SECONDS);
        self.mana_transfer_hold_until = self
            .action_locks
            .as_ref()
            .map_or(0.0, |locks| locks.borrow().now())
            + item_use_lock::TRANSACTION_SECONDS;
        true
    }

    /// Gives back the item slot the outstanding drain took, but only while
    /// the window it armed is still running.
    fn release_mana_transfer_hold(&mut self) {
        if let Some(locks) = &self.action_locks {
            let mut locks = locks.borrow_mut();
            if locks.now() < self.mana_transfer_hold_until {
                locks.release(ActionLockKind::ItemUse);
            }
        }
        self.mana_transfer_hold_until = 0.0;
    }

    fn has_salvage_bag_combine(&self, host: &mut dyn LootHost) -> bool {
        salvage::plan(
            &host.owned_items_shared(),
            Some(&self.abandoned_combine_bags),
            Some(&self.settings.salvage_combine),
        )
        .is_some()
    }

    fn continue_salvage_bag_combine(&mut self, host: &mut dyn LootHost, can_act: bool) -> bool {
        let owned = host.owned_items_shared();
        if let Some(pending) = self.combine_pending.clone() {
            let all_exist = pending
                .object_ids
                .iter()
                .all(|id| owned.iter().any(|item| item.object_id == *id));
            if !all_exist {
                for id in &pending.object_ids {
                    self.combine_attempts.remove(id);
                }
                self.combine_pending = None;
                self.state_age = 0.0;
                "Combined salvage bags.".clone_into(&mut self.status);
                return true;
            }
            if self.state_age < PICKUP_TIMEOUT_SECONDS {
                "Combining salvage bags…".clone_into(&mut self.status);
                return true;
            }

            let first = pending.first_object_id();
            let attempts = self.combine_attempts.get(&first).copied().unwrap_or(0) + 1;
            self.combine_attempts.insert(first, attempts);
            if attempts > 40 {
                self.abandoned_combine_bags.insert(first);
                self.combine_attempts.remove(&first);
                self.status = format!("Abandoned bugged salvage bag {}.", pending.first_name());
            } else {
                self.status = format!("Retrying salvage combine ({attempts}/40)…");
            }
            self.combine_pending = None;
            self.state_age = 0.0;
            return attempts <= 40;
        }
        if !can_act || !host.items_available() || host.items_busy() {
            return true;
        }

        let Some(next) = salvage::plan(
            &owned,
            Some(&self.abandoned_combine_bags),
            Some(&self.settings.salvage_combine),
        ) else {
            return false;
        };
        let Some(tool) = owned
            .iter()
            .find(|item| item.item_type & TINKERING_TOOL != 0)
            .filter(|tool| tool.object_id != 0)
        else {
            "Salvage combine is waiting for a salvage tool.".clone_into(&mut self.status);
            return false;
        };
        let result = host.salvage(tool.object_id, &next.object_ids);
        if !result.accepted() {
            self.status = if result.status == ItemStatus::Busy {
                "Waiting to combine salvage…".to_owned()
            } else {
                "Could not combine salvage bags.".to_owned()
            };
            return result.status == ItemStatus::Busy;
        }
        self.combine_pending = Some(next);
        self.state_age = 0.0;
        "Combining salvage bags…".clone_into(&mut self.status);
        true
    }

    /// A corpse item is appraised only when the answer depends on it. Three
    /// cases always appraise: an external classifier, the player's own death
    /// corpse, and a magical candidate while a spare mana stone is held.
    fn needs_identify(
        &self,
        host: &mut dyn LootHost,
        item: &InventoryItem,
        properties: &ItemProperties,
        owned: &[InventoryItem],
    ) -> bool {
        if !casing::is_white_space_only(&self.settings.external_classifier_id) {
            return true;
        }
        if self.active_corpse_is_own_death {
            return true;
        }
        if crate::requirements::is_magical(item, properties)
            && self.spare_mana_stone_count(host, owned) > 0
        {
            return true;
        }
        rules::needs_identify(
            item,
            properties,
            &self.settings.rules,
            Some(automation(host)),
        )
    }

    fn spare_mana_stone_count(&self, host: &mut dyn LootHost, owned: &[InventoryItem]) -> i32 {
        let mut stones = 0i32;
        for item in owned {
            if !mana::is_equipped(item)
                && item.effects & 1 == 0
                && is_profiled_mana_stone(&self.configured_consumable_kinds, item)
            {
                let mut store = HostDonorStore { host: &mut *host };
                if self.mana_donors.can_operate(&mut store)
                    && !self.mana_donors.stone_held(item.object_id)
                {
                    stones += 1;
                }
            }
        }
        let mut queued = 0i32;
        for (id, action) in self.classified_owned_items.iter() {
            if *action == LootAction::ManaTank
                && owned.iter().any(|item| item.object_id == *id)
                && !self
                    .mana_donors
                    .entries()
                    .any(|donor| donor.object_id == *id && donor.stone_id != 0)
                && Self::reserves_a_mana_stone(host, *id)
            {
                queued += 1;
            }
        }
        queued += self.pending_decision_count(LootAction::ManaTank);
        (stones - queued).max(0)
    }

    /// Whether a tank the character is carrying still holds a stone back.
    /// One whose description cannot be read yet keeps its reservation.
    fn reserves_a_mana_stone(host: &mut dyn LootHost, object_id: u32) -> bool {
        host.item_properties(object_id)
            .is_none_or(|written| mana::is_drainable_in_hand(&written))
    }

    fn increment_attempt(&mut self, object_id: u32) {
        *self.item_attempts.entry(object_id).or_insert(0) += 1;
    }

    fn decide_item(
        &self,
        host: &mut dyn LootHost,
        item: &InventoryItem,
        properties: &ItemProperties,
        owned: &[InventoryItem],
    ) -> (Option<LootDecision>, LootRefusal) {
        let (decision, refusal) =
            if casing::is_white_space_only(&self.settings.external_classifier_id) {
                rules::decide(
                    item,
                    properties,
                    &self.settings.rules,
                    owned,
                    Some(&self.pending_by_name),
                    Some(automation(host)),
                )
            } else {
                self.decide_with_external_classifier(host, item, properties, owned)
            };
        if decision.is_some() {
            return (decision, refusal);
        }
        if scroll::is_eligible(host, &self.settings, item, &self.pending_scroll_reads, true) {
            return (
                Some(LootDecision::new(
                    LootAction::Read,
                    0,
                    i32::MAX,
                    "Unknown Scroll",
                )),
                refusal,
            );
        }
        let decision = self
            .automatic_mana_action(host, item, owned)
            .map(|action| LootDecision::new(action, 0, i32::MAX, &format!("{action:?}")));
        (decision, refusal)
    }

    fn decide_with_external_classifier(
        &self,
        host: &mut dyn LootHost,
        item: &InventoryItem,
        properties: &ItemProperties,
        owned: &[InventoryItem],
    ) -> (Option<LootDecision>, LootRefusal) {
        let classifier_id = &self.settings.external_classifier_id;
        let context = LootContext {
            item: item.clone(),
            properties: properties.clone(),
            owned_items: owned.to_vec(),
        };
        let Some(classification) = host
            .classify(classifier_id, context)
            .filter(|classification| classification.matched)
        else {
            return (None, LootRefusal::no_rule());
        };
        let rule_name = if casing::is_white_space_only(&classification.rule_name) {
            classifier_id.clone()
        } else {
            classification
                .rule_name
                .trim_matches(casing::is_white_space)
                .to_owned()
        };
        let Some(action) = LootAction::from_i32(i32::from(classification.action.tag())) else {
            return (None, LootRefusal::no_rule());
        };
        if action == LootAction::NoLoot {
            return (
                None,
                LootRefusal {
                    kind: LootRefusalKind::RuleDeclined,
                    rule_name,
                    limit: 0,
                    held: 0,
                },
            );
        }
        if action == LootAction::KeepUpTo {
            let limit = classification.keep_count.max(0);
            let held = rules::held_count(&item.name, owned, Some(&self.pending_by_name));
            if held >= limit {
                return (
                    None,
                    LootRefusal {
                        kind: LootRefusalKind::RuleAtCap,
                        rule_name,
                        limit,
                        held,
                    },
                );
            }
        }
        (
            Some(LootDecision {
                action,
                priority: classification.priority,
                rule_index: -1,
                rule_name,
                classifier_id: classifier_id.clone(),
            }),
            LootRefusal::no_rule(),
        )
    }

    /// Marks an item already in the pack, as a looted one is marked (for
    /// tests, and for the panel's own marking).
    pub fn mark_owned(&mut self, host: &mut dyn LootHost, object_id: u32, action: LootAction) {
        self.classified_owned_items.insert(object_id, action);
        if action == LootAction::ManaTank
            && let Some(item) = host
                .owned_items_shared()
                .iter()
                .find(|item| item.object_id == object_id)
        {
            self.remember_mana_donor(host, item);
        }
    }

    pub(crate) fn remove_classified_owned(&mut self, host: &mut dyn LootHost, object_id: u32) {
        self.classified_owned_items.remove(&object_id);
        let Some(classifier_id) = self.external_classifier_by_item.remove(&object_id) else {
            return;
        };
        host.notify_item_removed(&classifier_id, object_id);
    }

    fn prune_removed_external_items(&mut self, host: &mut dyn LootHost) {
        if self.external_classifier_by_item.is_empty() {
            return;
        }
        let owned: HashSet<u32> = host
            .owned_items_shared()
            .iter()
            .map(|item| item.object_id)
            .collect();
        let removed: Vec<u32> = self
            .external_classifier_by_item
            .keys()
            .filter(|id| !owned.contains(id))
            .copied()
            .collect();
        for id in removed {
            self.remove_classified_owned(host, id);
        }
    }

    fn automatic_mana_action(
        &self,
        host: &mut dyn LootHost,
        item: &InventoryItem,
        owned: &[InventoryItem],
    ) -> Option<LootAction> {
        // Without one helper row of the stone kind the wanted count is zero
        // whatever the stone-count setting says.
        let profile_names_a_mana_stone = self
            .configured_consumable_kinds
            .values()
            .any(|kind| *kind == ConsumableCategory::ManaStone);
        let desired = if profile_names_a_mana_stone {
            self.settings.mana_stone_loot_count.clamp(0, 100)
        } else {
            0
        };
        let stones = i32::try_from(
            owned
                .iter()
                .filter(|owned_item| {
                    is_profiled_mana_stone(&self.configured_consumable_kinds, owned_item)
                })
                .count(),
        )
        .unwrap_or(i32::MAX)
            + self.pending_decision_count(LootAction::ManaStone);
        if is_profiled_mana_stone(&self.configured_consumable_kinds, item) && stones < desired {
            return Some(LootAction::ManaStone);
        }
        if self.spare_mana_stone_count(host, owned) > 0
            && mana::is_donor(item, self.settings.mana_tank_minimum_mana)
        {
            return Some(LootAction::ManaTank);
        }
        None
    }

    fn pending_decision_count(&self, action: LootAction) -> i32 {
        let count = self
            .decisions
            .values()
            .filter(|decision| {
                decision
                    .as_ref()
                    .is_some_and(|decision| decision.action == action)
            })
            .count();
        i32::try_from(count).unwrap_or(i32::MAX)
    }

    pub(crate) fn reset_transient(&mut self) {
        self.active_corpse = 0;
        self.active_corpse_saw_contents = false;
        self.active_corpse_is_own_death = false;
        self.abandoned_corpse = 0;
        self.refused_items.clear();
        self.abandon_waiting_item("the pass stopped");
        self.awaiting_appraisal = 0;
        self.awaiting_corpse_appraisal = 0;
        self.last_corpse_description_request = 0;
        self.salvage_pending_item = 0;
        self.salvage_pending_name.clear();
        self.salvage_attempts = 0;
        self.sell_pending_item = 0;
        self.sell_pending_name.clear();
        self.mana_transfer = None;
        self.mana_transfer_revision = 0;
        self.combine_pending = None;
        self.state_age = 0.0;
    }

    // --- The mana donors (`Looting.ManaDonors.cs`).

    /// Settles the donor journal against the pack and puts every donor the
    /// character still holds back among the items taken for their mana.
    pub fn observe_mana_donors(&mut self, host: &mut dyn LootHost) {
        if !host.is_available() || !host.items_available() {
            return;
        }
        let character = host.character();
        self.mana_donors.bind(
            &mut HostDonorStore { host: &mut *host },
            &character.world,
            &character.name,
        );
        if !self
            .mana_donors
            .can_operate(&mut HostDonorStore { host: &mut *host })
        {
            return;
        }
        if self.mana_donors.entries().next().is_none()
            && !self.classified_contains(LootAction::ManaTank)
        {
            return;
        }
        let owned = host.owned_items_shared();
        let complete = host.owned_inventory_complete();
        self.mana_donors
            .reconcile(&mut HostDonorStore { host: &mut *host }, &owned, complete);
        for change in self.mana_donors.take_changes() {
            self.write_log(&change);
        }
        let tanks: Vec<u32> = self
            .classified_owned_items
            .iter()
            .filter(|(_, action)| **action == LootAction::ManaTank)
            .map(|(id, _)| *id)
            .collect();
        for id in tanks {
            self.classified_owned_items.remove(&id);
        }
        let held: Vec<u32> = self
            .mana_donors
            .entries()
            .filter(|donor| {
                !donor.removed && owned.iter().any(|item| item.object_id == donor.object_id)
            })
            .map(|donor| donor.object_id)
            .collect();
        for id in held {
            self.classified_owned_items.insert(id, LootAction::ManaTank);
        }
    }

    fn remember_mana_donor(&mut self, host: &mut dyn LootHost, item: &InventoryItem) {
        let character = host.character();
        self.mana_donors.bind(
            &mut HostDonorStore { host: &mut *host },
            &character.world,
            &character.name,
        );
        if !self
            .mana_donors
            .remember(&mut HostDonorStore { host: &mut *host }, item)
        {
            "Mana donor history unavailable; draining paused.".clone_into(&mut self.status);
        }
    }
}

/// The room left in one container: its loose-item capacity less the loose
/// items in it. Packs take a pack slot of their own.
fn free_slots_in(
    container_id: u32,
    capacity: i32,
    all: &[ac_plugin_abi::world::WorldObject],
) -> i32 {
    let loose = all
        .iter()
        .filter(|held| {
            held.container_id == container_id && held.object_class != ObjectClass::Container
        })
        .count();
    (capacity - i32::try_from(loose).unwrap_or(i32::MAX)).max(0)
}

/// The looter's host seen as the rules' shared view.
pub(crate) fn automation(host: &mut dyn LootHost) -> &mut dyn mosstank_automation::Automation {
    host
}

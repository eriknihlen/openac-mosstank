//! Topping up worn gear's mana (`ItemManaRecharge`): the planner picks a
//! charge (a charged mana stone the profile knows first, else a one-shot
//! charge) and the worn item that wants it; the controller keeps the gear's
//! appraisals fresh one question at a time, spends one charge at a time on
//! the character (which spreads the mana over everything worn), and holds
//! every reading of worn gear stale once a refill lands.

use std::collections::{HashMap, HashSet};

use ac_plugin_abi::items::{InventoryItem, ItemStatus};
use ac_plugin_abi::world::ObjectClass;
use mosstank_automation::Automation;
use mosstank_automation::locks::{
    ActionLockKind, ActionLockTable, SharedActionLocks, item_use_lock,
};
use mosstank_automation::surface::{is_assessed, try_capture_properties};
use mosstank_dotnet::collections::OrdinalStringMap;
use mosstank_dotnet::text::ordinal;
use mosstank_settings::model::{CombatSettings, ConsumableCategory, InventorySettings};
use openac_plugin_compat::random::DotNetRandom;

/// The charged-mana effect bit of a stone.
const CHARGED_MANA_EFFECT: u32 = 0x0000_0001;
/// The combat use of nocked ammunition.
const AMMUNITION_COMBAT_USE: i32 = 3;
/// An item's current mana (int property).
const ITEM_CURRENT_MANA: u32 = 107;
/// An item's maximum mana (int property).
const ITEM_MAXIMUM_MANA: u32 = 108;

/// How long an unanswered use is waited out before it is given up, seconds.
const PENDING_TIMEOUT_SECONDS: f64 = 15.0;
/// The band a worn item's appraisal is believed for, seconds: drawn per
/// item so a whole kit does not come due on one pass.
const WORN_APPRAISAL_MINIMUM_SECONDS: f64 = 120.0;
const WORN_APPRAISAL_MAXIMUM_SECONDS: f64 = 360.0;
/// How long an unanswered appraisal request holds its item back, seconds.
const WORN_APPRAISAL_REPLY_SECONDS: f64 = 10.0;
/// The line the server prints when a charge is spent.
const REFILL_ANNOUNCEMENT: &str = "points of mana to the following items";
/// The status with nothing to do.
const READY_STATUS: &str = "Worn mana ready";

/// One refill to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemManaRechargePlan {
    /// The charge or stone to spend.
    pub charge_object_id: u32,
    /// The worn item that wants it most.
    pub target_object_id: u32,
    /// The charge's name.
    pub charge_name: String,
    /// The worn item's name.
    pub target_name: String,
    /// The charge's own mana.
    pub charge_current_mana: i32,
    /// The worn item's mana now.
    pub current_mana: i32,
    /// The worn item's mana at full.
    pub maximum_mana: i32,
}

/// What the planner asks of its caller about single items.
pub trait ManaPlanHooks {
    /// Whether a charge is ready to spend.
    fn is_charge_ready(&mut self, object_id: u32) -> bool;
    /// Whether a worn item's mana numbers can be trusted.
    fn is_target_ready(&mut self, object_id: u32) -> bool;
    /// A worn item has run low.
    fn report_low(&mut self, item: &InventoryItem, current_mana: i32, maximum_mana: i32);
}

fn is_equipped(item: &InventoryItem) -> bool {
    item.equipped_location != 0
}

/// Gear the refill can top up: equipped, and not nocked ammunition.
fn is_worn_target(item: &InventoryItem) -> bool {
    is_equipped(item) && item.combat_use != AMMUNITION_COMBAT_USE
}

/// What to spend on worn gear that has run low (`ItemManaRechargePlanner
/// .Plan`), and whether something wanted mana with nothing to give it. The
/// gear is looked at first: a source is only worth finding when something
/// wants it.
pub fn plan(
    inventory: &[InventoryItem],
    consumable_kinds: &OrdinalStringMap<ConsumableCategory>,
    threshold_percent: i32,
    wield_order: Option<&[u32]>,
    mut hooks: Option<&mut dyn ManaPlanHooks>,
) -> (Option<ItemManaRechargePlan>, bool) {
    let threshold = i64::from(threshold_percent.clamp(0, 99));

    let mut needs_charge: Vec<&InventoryItem> = Vec::new();
    for item in inventory {
        if is_equipped(item)
            && item.combat_use != AMMUNITION_COMBAT_USE
            && hooks
                .as_deref_mut()
                .is_none_or(|hooks| hooks.is_target_ready(item.object_id))
            && item.item_maximum_mana > 0
            && 100 * i64::from(item.item_current_mana.max(0)) / i64::from(item.item_maximum_mana)
                < threshold
        {
            needs_charge.push(item);
        }
    }
    if needs_charge.is_empty() {
        return (None, false);
    }
    if let Some(hooks) = hooks.as_deref_mut() {
        for low in &needs_charge {
            hooks.report_low(low, low.item_current_mana.max(0), low.item_maximum_mana);
        }
    }

    // A charged stone the profile knows by name comes first (it is the
    // reusable one); only when there is none is a one-shot charge spent. A
    // charge is whatever the profile files under that kind.
    let kind = |item: &InventoryItem| {
        consumable_kinds
            .get(item.name.as_str())
            .copied()
            .unwrap_or(ConsumableCategory::Other)
    };
    let mut ready = |item: &InventoryItem| {
        !is_equipped(item)
            && hooks
                .as_deref_mut()
                .is_none_or(|hooks| hooks.is_charge_ready(item.object_id))
    };
    let mut stones: Vec<&InventoryItem> = Vec::new();
    for item in inventory {
        if item.object_class == ObjectClass::ManaStone
            && kind(item) == ConsumableCategory::ManaStone
            && item.effects & CHARGED_MANA_EFFECT != 0
            && ready(item)
        {
            stones.push(item);
        }
    }
    let mut charge = first_by_name_then_id(stones);
    if charge.is_none_or(|c| c.object_id == 0) {
        let mut sources: Vec<&InventoryItem> = Vec::new();
        for item in inventory {
            if kind(item) == ConsumableCategory::ManaSource && ready(item) {
                sources.push(item);
            }
        }
        charge = first_by_name_then_id(sources);
    }
    let Some(charge) = charge.filter(|c| c.object_id != 0) else {
        return (None, true);
    };

    let target = match wield_order {
        None => needs_charge.iter().copied().min_by(|a, b| {
            let fraction = |item: &InventoryItem| {
                100.0 * f64::from(item.item_current_mana.max(0)) / f64::from(item.item_maximum_mana)
            };
            fraction(a)
                .total_cmp(&fraction(b))
                .then(a.object_id.cmp(&b.object_id))
        }),
        // Otherwise the oldest still-queued worn item.
        Some(order) => needs_charge.iter().copied().min_by(|a, b| {
            let position = |item: &InventoryItem| {
                order
                    .iter()
                    .position(|&id| id == item.object_id)
                    .unwrap_or(usize::MAX)
            };
            position(a)
                .cmp(&position(b))
                .then(a.object_id.cmp(&b.object_id))
        }),
    }
    .expect("at least one item needs a charge");
    (
        Some(ItemManaRechargePlan {
            charge_object_id: charge.object_id,
            target_object_id: target.object_id,
            charge_name: charge.name.clone(),
            target_name: target.name.clone(),
            charge_current_mana: charge.item_current_mana,
            current_mana: target.item_current_mana,
            maximum_mana: target.item_maximum_mana,
        }),
        false,
    )
}

/// The first by name (ordinal), then by id.
fn first_by_name_then_id(mut items: Vec<&InventoryItem>) -> Option<&InventoryItem> {
    items.sort_by(|a, b| ordinal::compare(&a.name, &b.name).then(a.object_id.cmp(&b.object_id)));
    items.into_iter().next()
}

/// A charge this owner spent, as it stood when it was spent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UsedChargeSnapshot {
    assessment_version: i32,
    current_mana: i32,
    observed_empty: bool,
}

/// What one call works with: the client and the live settings.
#[allow(missing_debug_implementations)]
pub struct ManaTurn<'a> {
    /// The client.
    pub automation: &'a mut dyn Automation,
    /// The inventory settings (the refill switch and percentage).
    pub settings: &'a InventorySettings,
    /// The combat settings (what each consumable is).
    pub profiles: &'a CombatSettings,
}

/// The worn-gear refill rule.
#[derive(Debug)]
pub struct ItemManaRechargeController {
    pending: Option<ItemManaRechargePlan>,
    pending_source_assessment_version: i32,
    pending_recipient_object_id: u32,
    observed_completion: i64,
    pending_age: f64,
    frame_observed_seconds: f64,
    action_locks: SharedActionLocks,
    pending_hold_until: f64,
    wield_order: Vec<u32>,
    used_charges: HashMap<u32, UsedChargeSnapshot>,
    worn_appraisal_spread: DotNetRandom,
    posted_warnings: HashSet<String>,
    reported_low_items: HashMap<u32, String>,
    worn_appraised_until: HashMap<u32, f64>,
    /// The worn items asked about and not yet answered: the stamp the
    /// client held when asked, and when. At most one is out at a time.
    worn_appraisal_asked: Vec<(u32, i32, f64)>,
    /// Items whose outstanding question a refill overtook: the question
    /// stays out (it is the one-at-a-time latch) but its answer is not kept.
    worn_appraisal_overtaken: HashSet<u32>,
    worn_clock: f64,
    chat_sequence: u64,
    last_worn_appraisal_asked: u32,
    status: String,
}

impl ItemManaRechargeController {
    /// An idle controller; `seed` seeds the spread of the worn appraisals'
    /// lifetimes (the plugin seeds it from the clock).
    #[must_use]
    pub fn new(seed: i32) -> Self {
        Self {
            pending: None,
            pending_source_assessment_version: 0,
            pending_recipient_object_id: 0,
            observed_completion: 0,
            pending_age: 0.0,
            frame_observed_seconds: 0.0,
            action_locks: ActionLockTable::shared(),
            pending_hold_until: 0.0,
            wield_order: Vec::new(),
            used_charges: HashMap::new(),
            worn_appraisal_spread: DotNetRandom::new(seed),
            posted_warnings: HashSet::new(),
            reported_low_items: HashMap::new(),
            worn_appraised_until: HashMap::new(),
            worn_appraisal_asked: Vec::new(),
            worn_appraisal_overtaken: HashSet::new(),
            worn_clock: 0.0,
            chat_sequence: 0,
            last_worn_appraisal_asked: 0,
            status: READY_STATUS.to_owned(),
        }
    }

    /// Shares the macro's cooldown table: a charge is in the character's
    /// hands for as long as the macro waits on it.
    pub fn bind_action_locks(&mut self, locks: SharedActionLocks) {
        self.action_locks = locks;
    }

    /// What the rule is doing, for the panel.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Reads the server's answer to the use this owner issued, on the host
    /// frame (an unanswered use holds the pass, so the pass cannot be what
    /// ends the wait). Nothing is started here.
    pub fn observe_pending_receipt(&mut self, turn: &mut ManaTurn<'_>, elapsed_seconds: f64) {
        if self.pending.is_none() || !turn.automation.is_available() {
            return;
        }
        let elapsed = elapsed_seconds.max(0.0);
        self.frame_observed_seconds += elapsed;
        self.observe_completion(turn.automation);
        self.age_pending(turn.automation, elapsed);
    }

    /// Charges the wait the seconds that have passed and gives it up once
    /// it has waited long enough; true when it was given up.
    fn age_pending(&mut self, automation: &mut dyn Automation, elapsed_seconds: f64) -> bool {
        let Some(waiting) = &self.pending else {
            return false;
        };
        self.pending_age += elapsed_seconds.max(0.0);
        if self.pending_age < PENDING_TIMEOUT_SECONDS {
            return false;
        }
        self.status = format!("Mana refill unconfirmed; holding {}.", waiting.charge_name);
        let line = format!(
            "{} source=0x{:08X}, recipient=0x{:08X}",
            self.status, waiting.charge_object_id, self.pending_recipient_object_id
        );
        automation.log_info(&line);
        self.clear_pending();
        true
    }

    fn clear_pending(&mut self) {
        self.pending = None;
        self.pending_age = 0.0;
        self.frame_observed_seconds = 0.0;
        // Only while the window this owner armed is still running: past it
        // the slot may belong to somebody else.
        let mut locks = self.action_locks.borrow_mut();
        if locks.now() < self.pending_hold_until {
            locks.release(ActionLockKind::ItemUse);
        }
        self.pending_hold_until = 0.0;
    }

    /// Whether the outstanding use still holds the character: for the
    /// window it armed, not the whole watchdog.
    fn pending_still_holds_the_pass(&self) -> bool {
        self.pending.is_some() && self.action_locks.borrow().now() < self.pending_hold_until
    }

    /// One turn of the rule: true when it claims the pass. `can_act` is
    /// whether it has the turn (it still watches what it started).
    pub fn tick(&mut self, turn: &mut ManaTurn<'_>, can_act: bool, elapsed_seconds: f64) -> bool {
        self.worn_clock += elapsed_seconds.max(0.0);
        self.observe_refill_announcement(turn.automation);
        for item in turn.automation.owned_items_shared().iter() {
            if item.effects & 1 == 0 && self.used_charges.contains_key(&item.object_id) {
                let version = assessment_version(turn.automation, item.object_id);
                if let Some(used) = self.used_charges.get_mut(&item.object_id) {
                    used.observed_empty = true;
                    used.assessment_version = version;
                }
            }
        }
        self.observe_completion(turn.automation);
        // What the frame already watched off this transaction is not
        // counted a second time.
        let pending_elapsed = (elapsed_seconds.max(0.0) - self.frame_observed_seconds).max(0.0);
        self.frame_observed_seconds = 0.0;
        if self.age_pending(turn.automation, pending_elapsed) {
            return false;
        }
        if self.pending.is_some() {
            return self.pending_still_holds_the_pass();
        }
        if !can_act
            || !turn.settings.refill_worn_mana
            || !turn.automation.is_available()
            || !turn.automation.items_available()
            || turn.automation.items_busy()
        {
            return false;
        }

        let owned = turn.automation.owned_items_shared();
        self.observe_wield_order(&owned);
        // Worn gear is looked at before anything is decided about it.
        self.observe_worn_appraisals(turn.automation, &owned);
        let (planned, source_missing) = {
            let mut hooks = Hooks {
                automation: turn.automation,
                used_charges: &mut self.used_charges,
                worn_appraised_until: &self.worn_appraised_until,
                reported_low_items: &mut self.reported_low_items,
            };
            plan(
                &owned,
                &turn.profiles.consumable_categories,
                turn.settings.refill_worn_mana_percent,
                Some(self.wield_order.as_slice()),
                Some(&mut hooks),
            )
        };
        let Some(next) = planned else {
            // Only now, with nothing to spend this pass: the client answers
            // one description at a time.
            self.request_worn_appraisals(turn.automation, &owned);
            if source_missing {
                self.warn_once(
                    turn.automation,
                    "Warning: No mana charges/stones available, but equipped items need mana.",
                );
                "No mana charge or stone to spend".clone_into(&mut self.status);
                return false;
            }
            self.reported_low_items.clear();
            let status = if self.has_pending_assessment(turn, &owned) {
                "Waiting for item assessment"
            } else {
                READY_STATUS
            };
            status.clone_into(&mut self.status);
            return false;
        };
        let recipient = turn.automation.character().id;
        if recipient == 0 {
            "Waiting for player identity".clone_into(&mut self.status);
            return false;
        }
        let source_version = assessment_version(turn.automation, next.charge_object_id);
        let result = turn.automation.apply_item(next.charge_object_id, recipient);
        let line = format!(
            "Worn mana refill request: source={} (0x{:08X}), recipient=player (0x{:08X}), thresholdItem={} (0x{:08X}), threshold={}%, mana={}/{}, result={:?}",
            next.charge_name,
            next.charge_object_id,
            recipient,
            next.target_name,
            next.target_object_id,
            turn.settings.refill_worn_mana_percent.clamp(0, 99),
            next.current_mana,
            next.maximum_mana,
            result.status
        );
        turn.automation.log_info(&line);
        if !result.accepted() {
            self.status = format!("Mana refill waiting: {:?}", result.status);
            return result.status == ItemStatus::Busy;
        }
        self.pending_age = 0.0;
        self.frame_observed_seconds = 0.0;
        // The charge is in the character's hands until the server answers
        // for it; every other rule that consumes an item waits that out.
        {
            let mut locks = self.action_locks.borrow_mut();
            locks.arm(ActionLockKind::ItemUse, item_use_lock::TRANSACTION_SECONDS);
            self.pending_hold_until = locks.now() + item_use_lock::TRANSACTION_SECONDS;
        }
        self.pending_source_assessment_version = source_version;
        self.pending_recipient_object_id = recipient;
        self.used_charges.insert(
            next.charge_object_id,
            UsedChargeSnapshot {
                assessment_version: source_version,
                current_mana: next.charge_current_mana,
                observed_empty: false,
            },
        );
        self.status = format!(
            "Refilling {} ({}/{})",
            next.target_name, next.current_mana, next.maximum_mana
        );
        self.pending = Some(next);
        true
    }

    fn observe_wield_order(&mut self, owned: &[InventoryItem]) {
        self.wield_order.retain(|&queued| {
            owned
                .iter()
                .any(|item| item.object_id == queued && is_equipped(item))
        });
        for item in owned {
            if !is_worn_target(item) {
                continue;
            }
            if !self.wield_order.contains(&item.object_id) {
                self.wield_order.push(item.object_id);
            }
        }
    }

    fn has_pending_assessment(&self, turn: &mut ManaTurn<'_>, owned: &[InventoryItem]) -> bool {
        for item in owned {
            let configured_charge = !is_equipped(item)
                && turn.profiles.consumable_categories.get(item.name.as_str())
                    == Some(&ConsumableCategory::ManaStone);
            if configured_charge && !is_assessed(turn.automation, item.object_id) {
                return true;
            }
            if is_worn_target(item) && !self.worn_appraised_until.contains_key(&item.object_id) {
                return true;
            }
        }
        false
    }

    /// Takes in the appraisals that have landed and forgets the gear no
    /// longer worn; a question nobody answered is let go after a while.
    fn observe_worn_appraisals(
        &mut self,
        automation: &mut dyn Automation,
        owned: &[InventoryItem],
    ) {
        self.worn_appraised_until
            .retain(|&tracked, _| is_still_worn(owned, tracked));
        let asked = std::mem::take(&mut self.worn_appraisal_asked);
        for (object_id, version, asked_at) in asked {
            if !is_still_worn(owned, object_id) {
                self.worn_appraisal_overtaken.remove(&object_id);
                continue;
            }
            let current = assessment_version(automation, object_id);
            if current != 0 && current != version {
                if self.worn_appraisal_overtaken.remove(&object_id) {
                    // The answer to a question a refill overtook says
                    // nothing about what the item holds now.
                    self.worn_appraised_until.remove(&object_id);
                } else {
                    let until = self.worn_clock + self.next_worn_appraisal_wait();
                    self.worn_appraised_until.insert(object_id, until);
                }
            } else if self.worn_clock - asked_at >= WORN_APPRAISAL_REPLY_SECONDS {
                self.worn_appraisal_overtaken.remove(&object_id);
            } else {
                self.worn_appraisal_asked
                    .push((object_id, version, asked_at));
            }
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    fn next_worn_appraisal_wait(&mut self) -> f64 {
        f64::from(self.worn_appraisal_spread.next_range(
            (WORN_APPRAISAL_MINIMUM_SECONDS * 1000.0) as i32,
            (WORN_APPRAISAL_MAXIMUM_SECONDS * 1000.0) as i32,
        )) / 1000.0
    }

    /// Asks about one piece of worn gear that is due (never appraised, or
    /// appraised too long ago): one question at a time, the gear in turn.
    fn request_worn_appraisals(
        &mut self,
        automation: &mut dyn Automation,
        owned: &[InventoryItem],
    ) {
        if !self.worn_appraisal_asked.is_empty() {
            return;
        }
        let mut due: Vec<u32> = Vec::new();
        for item in owned {
            if !is_worn_target(item) {
                continue;
            }
            let is_due = assessment_version(automation, item.object_id) == 0
                || self
                    .worn_appraised_until
                    .get(&item.object_id)
                    .is_none_or(|&until| until <= self.worn_clock);
            if is_due {
                due.push(item.object_id);
            }
        }
        if due.is_empty() {
            return;
        }
        let next = match due
            .iter()
            .position(|&id| id == self.last_worn_appraisal_asked)
        {
            Some(start) => due[(start + 1) % due.len()],
            None => due[0],
        };
        if !automation.identify(next).accepted() {
            return;
        }
        let version = assessment_version(automation, next);
        self.worn_appraisal_asked
            .push((next, version, self.worn_clock));
        self.last_worn_appraisal_asked = next;
    }

    /// Said once a run, to the log and the chat window.
    fn warn_once(&mut self, automation: &mut dyn Automation, text: &str) {
        if !self.posted_warnings.insert(text.to_owned()) {
            return;
        }
        automation.log_info(text);
        automation.post_system_message(&format!("[MossTank] {text}"));
    }

    /// Lets the once-a-run warnings be said again.
    pub fn reset_once_per_run_warnings(&mut self) {
        self.posted_warnings.clear();
    }

    /// Forgets everything.
    pub fn reset(&mut self) {
        self.wield_order.clear();
        self.used_charges.clear();
        self.worn_appraised_until.clear();
        self.worn_appraisal_asked.clear();
        self.last_worn_appraisal_asked = 0;
        self.reported_low_items.clear();
        self.posted_warnings.clear();
        self.worn_clock = 0.0;
        self.chat_sequence = 0;
        self.clear_pending();
        self.pending_source_assessment_version = 0;
        self.pending_recipient_object_id = 0;
        READY_STATUS.clone_into(&mut self.status);
    }

    /// The refill line names every item that took mana; the server sends no
    /// property update for them, so this line (and the receipt) is the only
    /// word that every reading of worn gear is out of date.
    fn observe_refill_announcement(&mut self, automation: &mut dyn Automation) {
        for message in automation.chat_messages(self.chat_sequence) {
            self.chat_sequence = self.chat_sequence.max(message.sequence);
            if message.text.contains(REFILL_ANNOUNCEMENT) {
                self.forget_worn_readings();
            }
        }
    }

    /// Gives up every reading of worn gear; a question already out stays
    /// out (it is the one-at-a-time latch) and is marked so its answer is
    /// not kept.
    fn forget_worn_readings(&mut self) {
        self.worn_appraised_until.clear();
        self.reported_low_items.clear();
        for &(asked, _, _) in &self.worn_appraisal_asked {
            self.worn_appraisal_overtaken.insert(asked);
        }
    }

    fn observe_completion(&mut self, automation: &mut dyn Automation) {
        let completion = automation.last_item_completion();
        if completion.revision == 0 || completion.revision == self.observed_completion {
            return;
        }
        self.observed_completion = completion.revision;
        let Some(pending) = self.pending.clone() else {
            return;
        };
        if completion.source_id != pending.charge_object_id
            || completion.target_id != self.pending_recipient_object_id
        {
            return;
        }
        let success = completion.weenie_error == 0;
        automation.log_info(&format!(
            "Worn mana refill completion: source={} (0x{:08X}), recipient=player (expected=0x{:08X}, actual=0x{:08X}), thresholdItem={} (0x{:08X}), success={}, error=0x{:08X}",
            pending.charge_name,
            pending.charge_object_id,
            self.pending_recipient_object_id,
            completion.target_id,
            pending.target_name,
            pending.target_object_id,
            if success { "True" } else { "False" },
            completion.weenie_error
        ));
        if success {
            let assessment = automation.identify(pending.charge_object_id);
            automation.log_info(&format!(
                "Worn mana refill source assessment: source={} (0x{:08X}), priorVersion={}, result={:?}",
                pending.charge_name,
                pending.charge_object_id,
                self.pending_source_assessment_version,
                assessment.status
            ));
            self.forget_worn_readings();
        }
        self.status = if success {
            format!("Refilled {}", pending.target_name)
        } else {
            format!("Mana refill failed (0x{:X})", completion.weenie_error)
        };
        self.clear_pending();
        self.pending_source_assessment_version = 0;
        self.pending_recipient_object_id = 0;
    }
}

fn is_still_worn(owned: &[InventoryItem], object_id: u32) -> bool {
    owned
        .iter()
        .find(|item| item.object_id == object_id)
        .is_some_and(is_worn_target)
}

/// The client's appraisal stamp of an object (0 when it has none).
fn assessment_version(automation: &mut dyn Automation, object_id: u32) -> i32 {
    automation
        .object(object_id)
        .map_or(0, |object| object.last_id_time)
}

fn int_property(properties: &ac_plugin_abi::world::ItemProperties, key: u32) -> Option<i32> {
    properties
        .ints
        .iter()
        .find(|(k, _)| *k == key)
        .map(|&(_, value)| value)
}

/// The controller's answers to the planner.
struct Hooks<'a> {
    automation: &'a mut dyn Automation,
    used_charges: &'a mut HashMap<u32, UsedChargeSnapshot>,
    worn_appraised_until: &'a HashMap<u32, f64>,
    reported_low_items: &'a mut HashMap<u32, String>,
}

impl ManaPlanHooks for Hooks<'_> {
    /// A charge this owner never spent is ready as it stands; one it just
    /// spent has to prove it changed, by a new appraisal whose mana moved
    /// (or that follows the charge being seen empty).
    fn is_charge_ready(&mut self, object_id: u32) -> bool {
        let Some(&used) = self.used_charges.get(&object_id) else {
            return true;
        };
        let Some(current_mana) = try_capture_properties(self.automation, object_id)
            .and_then(|properties| int_property(&properties, ITEM_CURRENT_MANA))
        else {
            return false;
        };
        let version = assessment_version(self.automation, object_id);
        if version == 0
            || version == used.assessment_version
            || (!used.observed_empty && current_mana == used.current_mana)
        {
            return false;
        }
        self.used_charges.remove(&object_id);
        true
    }

    /// Only an appraisal this owner asked for and saw answered dates the
    /// mana numbers.
    fn is_target_ready(&mut self, object_id: u32) -> bool {
        self.worn_appraised_until.contains_key(&object_id)
            && try_capture_properties(self.automation, object_id).is_some_and(|properties| {
                int_property(&properties, ITEM_CURRENT_MANA).is_some()
                    && int_property(&properties, ITEM_MAXIMUM_MANA).is_some()
            })
    }

    /// Said once per item and per level.
    fn report_low(&mut self, item: &InventoryItem, current_mana: i32, maximum_mana: i32) {
        let percent = if maximum_mana == 0 {
            0
        } else {
            i32::try_from(100 * i64::from(current_mana) / i64::from(maximum_mana))
                .unwrap_or(i32::MAX)
        };
        let line = format!(
            "Item {} low on mana, {percent}%, {current_mana}/{maximum_mana}",
            item.name
        );
        if self.reported_low_items.get(&item.object_id) == Some(&line) {
            return;
        }
        self.reported_low_items.insert(item.object_id, line.clone());
        self.automation.log_info(&line);
    }
}

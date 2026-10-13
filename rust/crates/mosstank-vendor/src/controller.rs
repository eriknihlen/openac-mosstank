//! Buying and selling at an open vendor by a loot profile
//! (`VendorTradeController`). A run owns the character for the whole
//! visit: a stack pass first, then rounds of buying and selling planned by
//! [`crate::planner`], each committed through the staged lists and waited
//! on until the server answers, until a round has nothing left to do. It
//! also answers the `/ub vendor` commands, which stage and commit by hand
//! and open a vendor by name.
//!
//! The vendor's events reach the controller from the plugin
//! ([`VendorTradeController::on_vendor_opened`] and friends); they are
//! drained on the next tick, as the C# drains what its handlers queued.

use std::collections::{HashSet, VecDeque};

use ac_plugin_abi::items::{InventoryItem, ItemStatus};
use ac_plugin_abi::trade::{
    NO_VALUE_LIMIT, VendorItem, VendorProfile, VendorResult, VendorState, VendorStatus,
    VendorTransaction,
};
use ac_plugin_abi::world::{ItemProperties, ObjectClass};
use mosstank_automation::Automation;
use mosstank_automation::locks::{ActionLockKind, SharedActionLocks};
use mosstank_formats::utl::{self, LootAction, LootRule};
use mosstank_loot::rules::{self, LootDecision};
use mosstank_stores::storage::StorageError;
use openac_plugin_compat::number::{NumberStyles, format_f32, format_i64, format_u32, parse_i32};
use openac_plugin_compat::text::{casing, ordinal};

use crate::client::VendorAutomation;
use crate::object_search::{SearchView, try_find_nearest};
use crate::planner::{
    self, DEFAULT_COIN_STACK_SIZE, VendorBuyCandidate, VendorPurse, VendorSellCandidate,
    VendorSplitRequest, VendorTradePlan,
};
use crate::sell_guard::is_safe_to_sell;
use crate::ub_chat::{self, tools};

/// A run with no progress for this long gives up, seconds.
const BAIL_SECONDS: f64 = 60.0;

/// How long each tick of a running visit holds navigation and item use. The
/// window only has to outlive the gap between ticks; it is re-armed every
/// tick and released the moment the run ends.
const LOCK_SECONDS: f64 = 30.0;

/// How long a round waits for the coin and the goods to catch up with the
/// server's answer before it plans on what it can see.
const SETTLE_TIMEOUT_SECONDS: f64 = 15.0;

/// How long a split is given to produce its stack.
const SPLIT_TIMEOUT_SECONDS: f64 = 10.0;

/// Coin the settle does not wait for: a vendor rounds its payouts.
const COIN_TOLERANCE: i64 = 50;

const COIN_WEENIE_CLASS_ID: u32 = 273;
const PROFILE_FOLDER: &str = "autovendor";
const PROFILE_EXTENSION: &str = ".utl";

/// The number the reference's chat links carry to say which plugin they
/// belong to; a test-mode sell line writes it as the reference does.
pub const REFERENCE_CHAT_LINK_ID: i32 = 438_347_936;

const SHOP_CATEGORIES: [(u32, &str); 24] = [
    (0x0000_0001, "Weapons"),
    (0x0000_0002, "Armor"),
    (0x0000_0004, "Clothing"),
    (0x0000_0008, "Jewelry"),
    (0x0000_0010, "Miscellaneous"),
    (0x0000_0020, "Food"),
    (0x0000_0080, "Miscellaneous"),
    (0x0000_0100, "Weapons"),
    (0x0000_0200, "Containers"),
    (0x0000_0400, "Miscellaneous"),
    (0x0000_0800, "Gems"),
    (0x0000_1000, "Spell Components"),
    (0x0000_2000, "Books, Paper"),
    (0x0000_4000, "Keys, Tools"),
    (0x0000_8000, "Magic Items"),
    (0x0004_0000, "Trade Notes"),
    (0x0008_0000, "Mana Stones"),
    (0x0010_0000, "Services"),
    (0x0040_0000, "Cooking Items"),
    (0x0080_0000, "Alchemical Items"),
    (0x0100_0000, "Fletching Items"),
    (0x0400_0000, "Alchemical Items"),
    (0x0800_0000, "Fletching Items"),
    (0x2000_0000, "Keys, Tools"),
];

/// The switches a vendor run reads, as they stand for this call (each a
/// live read in the C#, so an edit applies to the next tick).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VendorTradeSettings {
    /// `AutoVendor.Enabled`.
    pub enabled: bool,
    /// `AutoVendor.EnableBuying`.
    pub enable_buying: bool,
    /// `AutoVendor.EnableSelling`.
    pub enable_selling: bool,
    /// `AutoVendor.TestMode`.
    pub test_mode: bool,
    /// `AutoVendor.Think`.
    pub think: bool,
    /// `AutoVendor.ShowMerchantInfo`.
    pub show_merchant_info: bool,
    /// `AutoVendor.OnlyFromMainPack`.
    pub only_from_main_pack: bool,
    /// `AutoVendor.Tries`.
    pub tries: i32,
    /// `AutoVendor.TriesTime`, milliseconds.
    pub tries_time_milliseconds: i32,
}

impl Default for VendorTradeSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            enable_buying: true,
            enable_selling: true,
            test_mode: false,
            think: false,
            show_merchant_info: true,
            only_from_main_pack: false,
            tries: 4,
            tries_time_milliseconds: 5000,
        }
    }
}

/// What the run asks of the plugin's state for the call.
pub trait VendorContext {
    /// The client.
    fn automation(&mut self) -> &mut dyn VendorAutomation;

    /// The switches as they stand now.
    fn settings(&mut self) -> VendorTradeSettings;

    /// Where a profile file is, looked for the tiered way
    /// (`UbSettingStore.ResolveProfileKey`).
    ///
    /// # Errors
    /// A candidate exists but cannot be read.
    fn resolve_profile_key(
        &mut self,
        folder: &str,
        file_name: &str,
    ) -> Result<String, StorageError>;

    /// A profile file's text, `None` when it is not there or the profile
    /// storage is not (`VtankProfiles.ReadText`).
    ///
    /// # Errors
    /// The file exists but cannot be read.
    fn read_profile(&mut self, key: &str) -> Result<Option<String>, StorageError>;

    /// The stack-and-cram pass a run asks for before it plans: called every
    /// tick with the elapsed time and whether this is the first call of the
    /// pass, answering true while it still has work in flight.
    fn stack_cram_pass(&mut self, elapsed_seconds: f64, first: bool) -> bool {
        let _ = (elapsed_seconds, first);
        false
    }

    /// Whether the route owns opening vendors right now, in which case the
    /// open-by-name command stands down rather than fight it.
    fn route_owns_vendor_open(&mut self) -> bool {
        false
    }

    /// The items the macro profile uses, which are never sold.
    fn protected_items(&mut self) -> HashSet<u32> {
        HashSet::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    StackCram,
    Plan,
    AwaitTransaction,
    Settle,
    AwaitSplit,
}

/// The vendor run and the `/ub vendor` commands.
#[derive(Debug)]
pub struct VendorTradeController {
    rules: Vec<LootRule>,
    pending_sell: HashSet<u32>,
    completed: VecDeque<VendorTransaction>,
    locks: Option<SharedActionLocks>,
    pending_open: u32,
    pending_close: bool,
    phase: Phase,
    first_stack_pass: bool,
    since_progress: f64,
    since_completion: f64,
    expected_coin: i64,
    /// How far the server's charge may run past the planned cost; the
    /// settle accepts that much more.
    coin_slack: i64,
    /// What the outstanding buy or sell asked for, said with a failure.
    attempt: String,
    coin_before: i64,
    split_revision: i64,
    split_object_id: u32,
    split_waited: f64,
    vendor_name: String,
    vendor_object_id: u32,
    reported_vendor: u32,
    open_target: u32,
    open_name: String,
    open_tries: i32,
    since_open_try: f64,
    stage_split_object_id: u32,
    stage_split_name: String,
    stage_split_needed: i32,
    stage_split_asked: i32,
    stage_split_revision: i64,
    running: bool,
    status: String,
}

impl Default for VendorTradeController {
    fn default() -> Self {
        Self::new()
    }
}

fn empty_properties() -> ItemProperties {
    ItemProperties::default()
}

fn trim(text: &str) -> &str {
    text.trim_matches(casing::is_white_space)
}

fn n0(value: i64) -> String {
    format_i64(value, "n0").expect("a supported format")
}

fn notice(notice: Option<&str>) -> String {
    match notice {
        Some(text) if !casing::is_white_space_only(text) => format!(", {text}"),
        _ => String::new(),
    }
}

fn name_matches(name: &str, wanted: &str, partial: bool) -> bool {
    if partial {
        ordinal::contains_ignore_case(name, wanted)
    } else {
        ordinal::equals_ignore_case(name, wanted)
    }
}

/// Nothing when the client took the command, which the reference sends
/// without a word; the client's refusal, as the tool's error, when not.
fn refusal(what: &str, result: &VendorResult) -> Vec<String> {
    if result.status == VendorStatus::Sent {
        Vec::new()
    } else {
        vec![ub_chat::tool_error(
            tools::AUTO_VENDOR,
            &format!(
                "{what}: {:?}{}",
                result.status,
                notice(result.notice.as_deref())
            ),
        )]
    }
}

/// The count and the name, read as the reference reads them: a leading
/// whole number is the count, anything else leaves the count at one and
/// the whole text the name. `None` when that leaves no name, or a count
/// below one.
fn try_split_count(arguments: &str) -> Option<(i32, String)> {
    let text = trim(arguments);
    let space = text.find(' ');
    let head = space.map_or(text, |space| &text[..space]);
    let (count, name) = match parse_i32(head, NumberStyles::INTEGER) {
        Some(count) => (
            count,
            space.map_or(String::new(), |space| trim(&text[space + 1..]).to_owned()),
        ),
        None => (1, text.to_owned()),
    };
    (!name.is_empty() && count >= 1).then_some((count, name))
}

/// A vendor listing in the shape the rules read, so one rule engine
/// decides both what to buy and what to sell.
fn facade(
    automation: &mut dyn VendorAutomation,
    listing: &VendorItem,
) -> (InventoryItem, ItemProperties) {
    let properties = automation
        .vendor_item_properties(listing.template_id)
        .unwrap_or_else(empty_properties);
    let stack = listing.stack_size.max(1);
    let unit_value = properties
        .int(19)
        .map_or(listing.unit_price, |listed| listed / stack);
    let item = InventoryItem {
        object_id: listing.template_id,
        weenie_class_id: listing.weenie_class_id,
        name: listing.name.clone(),
        item_type: listing.item_type,
        stack_size: stack,
        value: unit_value.wrapping_mul(stack),
        maximum_stack_size: listing.max_stack_size.max(1),
        object_class: listing.object_class,
        ..InventoryItem::default()
    };
    (item, properties)
}

fn coin_on_hand(owned: &[InventoryItem], profile: &VendorProfile) -> i64 {
    let currency = if profile.alternate_currency_weenie_class_id != 0 {
        profile.alternate_currency_weenie_class_id
    } else {
        COIN_WEENIE_CLASS_ID
    };
    // Only the currency itself pays: pyreal coins, or the vendor's own
    // alternate currency. Peas and trade notes are money-class items the
    // server does not count as coin.
    owned
        .iter()
        .filter(|item| item.weenie_class_id == currency)
        .map(|item| i64::from(item.stack_size.max(1)))
        .sum()
}

impl VendorTradeController {
    /// An idle controller.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            pending_sell: HashSet::new(),
            completed: VecDeque::new(),
            locks: None,
            pending_open: 0,
            pending_close: false,
            phase: Phase::StackCram,
            first_stack_pass: false,
            since_progress: 0.0,
            since_completion: 0.0,
            expected_coin: 0,
            coin_slack: 0,
            attempt: String::new(),
            coin_before: 0,
            split_revision: 0,
            split_object_id: 0,
            split_waited: 0.0,
            vendor_name: String::new(),
            vendor_object_id: 0,
            reported_vendor: 0,
            open_target: 0,
            open_name: String::new(),
            open_tries: 0,
            since_open_try: 0.0,
            stage_split_object_id: 0,
            stage_split_name: String::new(),
            stage_split_needed: 0,
            stage_split_asked: 0,
            stage_split_revision: 0,
            running: false,
            status: "Vendor run idle.".to_owned(),
        }
    }

    /// Whether a run owns the character.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// The run's status line.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// The shared lock table a running visit holds navigation and item use
    /// on, so the route's mover does not walk the character off mid-visit.
    pub fn bind_action_locks(&mut self, locks: SharedActionLocks) {
        self.locks = Some(locks);
    }

    /// A vendor window opened.
    pub fn on_vendor_opened(&mut self, vendor_object_id: u32) {
        self.pending_open = vendor_object_id;
        self.pending_close = false;
    }

    /// The vendor window closed.
    pub fn on_vendor_closed(&mut self) {
        self.pending_close = true;
        self.pending_open = 0;
    }

    /// A purchase or sale ended.
    pub fn on_transaction_completed(&mut self, transaction: VendorTransaction) {
        self.completed.push_back(transaction);
    }

    fn arm_locks(&self) {
        if let Some(locks) = &self.locks {
            let mut locks = locks.borrow_mut();
            locks.arm(ActionLockKind::Navigation, LOCK_SECONDS);
            locks.arm(ActionLockKind::ItemUse, LOCK_SECONDS);
        }
    }

    fn release_locks(&self) {
        if let Some(locks) = &self.locks {
            let mut locks = locks.borrow_mut();
            locks.release(ActionLockKind::Navigation);
            locks.release(ActionLockKind::ItemUse);
        }
    }

    /// Starts a run against the open vendor. The profile is the vendor's
    /// own name unless one is given, looked for the tiered way; in test
    /// mode the run only says what it would do.
    pub fn try_start(&mut self, cx: &mut dyn VendorContext, profile_name: Option<&str>) -> bool {
        if !cx.automation().is_available() {
            "Vendor run needs a live session.".clone_into(&mut self.status);
            let status = self.status.clone();
            Self::say_error(cx, &status);
            return false;
        }
        let vendor = cx.automation().vendor_state();
        if !vendor.is_open || vendor.vendor_id == 0 {
            "Vendor run cannot start: no vendor open.".clone_into(&mut self.status);
            self.think_or_say(cx, "AutoVendor Fatal - no vendor, cannot start");
            return false;
        }
        if self.running {
            self.stop(cx, None, true);
        }

        self.vendor_object_id = vendor.vendor_id;
        self.vendor_name = vendor.vendor_name;
        let mut requested = trim(profile_name.unwrap_or("")).to_owned();
        if ordinal::ends_with_ignore_case(&requested, PROFILE_EXTENSION) {
            requested.truncate(requested.len() - PROFILE_EXTENSION.len());
        }
        let file_name = format!(
            "{}{PROFILE_EXTENSION}",
            if requested.is_empty() {
                self.vendor_name.as_str()
            } else {
                requested.as_str()
            }
        );
        let found = cx
            .resolve_profile_key(PROFILE_FOLDER, &file_name)
            .map_err(|_| file_name.clone())
            .and_then(|key| match cx.read_profile(&key) {
                Ok(text) => Ok((key, text)),
                Err(_) => Err(key),
            });
        let (key, text) = match found {
            Ok(found) => found,
            Err(key) => {
                // A profile storage that fails is a profile that cannot be
                // read.
                let reason = format!("Vendor profile could not be read: {key}");
                Self::say_error(cx, &reason);
                self.stop(cx, Some(reason), false);
                return false;
            }
        };
        // A profile that cannot be had ends the run the way every other end
        // does, with the finished line after the error.
        let Some(text) = text else {
            let reason = format!("No vendor profile exists: {key}");
            Self::say_error(cx, &reason);
            self.stop(cx, Some(reason), false);
            return false;
        };
        let Some(rules) = utl::try_parse_rules(Some(&text)) else {
            let reason = format!("Vendor profile could not be read: {key}");
            Self::say_error(cx, &reason);
            self.stop(cx, Some(reason), false);
            return false;
        };
        self.rules = rules;

        if cx.settings().test_mode {
            self.report_test_mode(cx);
            self.status = format!("Vendor run test mode reported for {}.", self.vendor_name);
            return true;
        }

        self.phase = Phase::StackCram;
        self.first_stack_pass = true;
        self.since_progress = 0.0;
        self.pending_sell.clear();
        self.completed.clear();
        self.expected_coin = 0;
        self.running = true;
        self.status = format!(
            "Vendor run started at {} with {} rule(s).",
            self.vendor_name,
            self.rules.len()
        );
        true
    }

    /// Stops a run on the player's say-so; false when none was running. The
    /// finished line is said either way, as the reference's stop says it.
    pub fn stop_requested(&mut self, cx: &mut dyn VendorContext) -> bool {
        if !self.running {
            "Vendor run is not running.".clone_into(&mut self.status);
            self.say_finished(cx);
            return false;
        }
        let reason = format!("Vendor run cancelled: {}", self.vendor_name);
        self.stop(cx, Some(reason), false);
        true
    }

    /// Advances the run and the open-by-name command. True while either
    /// owns the character.
    pub fn tick(
        &mut self,
        cx: &mut dyn VendorContext,
        elapsed_seconds: f64,
        can_act: bool,
    ) -> bool {
        let step = elapsed_seconds.max(0.0);
        self.drain_events(cx);
        self.observe_stage_split(cx);
        let opening = self.tick_open(cx, step, can_act);
        if !self.running {
            return opening;
        }

        if !cx.automation().is_available() || !cx.automation().vendor_state().is_open {
            self.stop(
                cx,
                Some("Vendor run stopped: the vendor closed.".to_owned()),
                false,
            );
            return opening;
        }
        if !cx.settings().enabled {
            self.stop(cx, Some("Vendor run stopped: disabled.".to_owned()), false);
            return opening;
        }
        // Re-armed every tick, so a long visit is held for as long as it
        // lives and not just for the first window.
        self.arm_locks();
        self.since_progress += step;
        if self.since_progress > BAIL_SECONDS {
            Self::say(
                cx,
                &ub_chat::tool(tools::AUTO_VENDOR, "bail, Timeout expired"),
            );
            self.stop(
                cx,
                Some("Vendor run bailed: timeout expired.".to_owned()),
                false,
            );
            return opening;
        }
        if !can_act {
            return true;
        }

        match self.phase {
            Phase::StackCram => {
                let busy = cx.stack_cram_pass(step, self.first_stack_pass);
                self.first_stack_pass = false;
                if !busy {
                    self.phase = Phase::Plan;
                    self.since_progress = 0.0;
                }
                true
            }
            Phase::Plan => {
                self.plan_round(cx);
                self.running
            }
            Phase::AwaitTransaction => true,
            Phase::Settle => {
                self.tick_settle(cx, step);
                true
            }
            Phase::AwaitSplit => {
                self.tick_split(cx, step);
                self.running
            }
        }
    }

    /// The macro started: the route owns vendor opening from now on, so an
    /// open-by-name begun before it is dropped rather than sent beside it.
    pub fn on_macro_started(&mut self) {
        if self.open_target == 0 {
            return;
        }
        self.open_target = 0;
        self.open_tries = 0;
        self.status = format!(
            "Vendor open dropped: the macro started and the route owns opening {}.",
            self.open_name
        );
    }

    /// Back to idle.
    pub fn reset(&mut self) {
        if self.running {
            self.release_locks();
        }
        self.running = false;
        self.phase = Phase::StackCram;
        self.rules.clear();
        self.pending_sell.clear();
        self.completed.clear();
        self.pending_open = 0;
        self.pending_close = false;
        self.open_target = 0;
        self.open_tries = 0;
        self.stage_split_object_id = 0;
        self.reported_vendor = 0;
        "Vendor run idle.".clone_into(&mut self.status);
    }

    /// The `/ub vendor` family: `open[p] <name>`, `opencancel`, `buyall`,
    /// `sellall`, `clearbuy`, `clearsell`, `addbuy[p] [n] <item>` and
    /// `addsell[p] [n] <item>`. Returns the lines to say back, each in the
    /// reference's shape; `None` when the line does not parse (the caller
    /// answers that with the command's help).
    pub fn vendor_command(
        &mut self,
        cx: &mut dyn VendorContext,
        arguments: &str,
    ) -> Option<Vec<String>> {
        let text = trim(arguments);
        let space = text.find(' ');
        let verb = casing::to_lower_invariant(space.map_or(text, |space| &text[..space]));
        let rest = space.map_or(String::new(), |space| trim(&text[space + 1..]).to_owned());
        let partial =
            verb.ends_with('p') && matches!(verb.as_str(), "openp" | "addbuyp" | "addsellp");

        match verb.as_str() {
            "buy" | "buyall" => Some(refusal("Buy", &cx.automation().vendor_buy_all())),
            "sell" | "sellall" => Some(refusal("Sell", &cx.automation().vendor_sell_all())),
            "clearbuy" => Some(refusal(
                "Clear buy list",
                &cx.automation().vendor_clear_buy_list(),
            )),
            "clearsell" => Some(refusal(
                "Clear sell list",
                &cx.automation().vendor_clear_sell_list(),
            )),
            "open" | "openp" => Some(self.begin_open(cx, &rest, partial)),
            "opencancel" => {
                self.open_target = 0;
                self.open_tries = 0;
                Some(Vec::new())
            }
            // A line with no item name, or a count below one, is the usage:
            // an empty name is part of every name, and a count below one
            // would ask the server to split off billions or stage nothing.
            "addbuy" | "addbuyp" => {
                let (count, name) = try_split_count(&rest)?;
                Some(Self::add_buy_by_name(cx, &rest, count, &name, partial))
            }
            "addsell" | "addsellp" => {
                let (count, name) = try_split_count(&rest)?;
                let line = self.add_sell_by_name(cx, count, &name, partial);
                Some(if line.is_empty() {
                    Vec::new()
                } else {
                    vec![line]
                })
            }
            _ => None,
        }
    }

    fn drain_events(&mut self, cx: &mut dyn VendorContext) {
        if self.pending_close {
            self.pending_close = false;
            if self.running {
                self.stop(
                    cx,
                    Some("Vendor run stopped: the vendor closed.".to_owned()),
                    false,
                );
            }
        }
        if self.pending_open != 0 {
            let opened = self.pending_open;
            self.pending_open = 0;
            if self.open_target != 0 && opened == self.open_target {
                self.open_target = 0;
                self.open_tries = 0;
                self.status = format!("Vendor opened: {}.", self.open_name);
            }
            self.report_merchant(cx, opened);
            if !self.running && cx.settings().enabled && cx.automation().is_available() {
                self.try_start(cx, None);
            }
        }
        while let Some(transaction) = self.completed.pop_front() {
            if !self.running || self.phase != Phase::AwaitTransaction {
                continue;
            }
            if !transaction.success {
                let mut reason = format!("Vendor run stopped: {:?} failed", transaction.kind);
                if let Some(text) = transaction
                    .notice
                    .as_deref()
                    .filter(|text| !casing::is_white_space_only(text))
                {
                    reason.push_str(&format!(": {text}"));
                }
                if !self.attempt.is_empty() {
                    reason.push_str(&format!(" ({})", self.attempt));
                }
                self.stop_with_error(cx, reason);
                continue;
            }
            self.phase = Phase::Settle;
            self.since_completion = 0.0;
            self.since_progress = 0.0;
        }
    }

    fn report_merchant(&mut self, cx: &mut dyn VendorContext, vendor_object_id: u32) {
        let settings = cx.settings();
        if !settings.show_merchant_info || self.reported_vendor == vendor_object_id {
            return;
        }
        self.reported_vendor = vendor_object_id;
        let vendor = cx.automation().vendor_state();
        let profile = &vendor.profile;
        let mut categories: Vec<&str> = Vec::new();
        for (bit, name) in SHOP_CATEGORIES {
            if profile.deals_in_item_types & bit != 0 && !categories.contains(&name) {
                categories.push(name);
            }
        }
        let categories = categories.join(", ");
        let ceiling = if profile.maximum_value == NO_VALUE_LIMIT {
            "no limit".to_owned()
        } else {
            format_u32(profile.maximum_value, "n0").expect("a supported format")
        };
        let enabled = settings.enabled;
        let line = format!(
            "{}[0x{vendor_object_id:08X}]: pays {}% of value, deals in {}, max value {ceiling}, Buy: {} Sell: {}",
            vendor.vendor_name,
            format_f32(profile.buy_rate * 100.0, "0").expect("a supported format"),
            if categories.is_empty() {
                "nothing"
            } else {
                categories.as_str()
            },
            if enabled && settings.enable_buying {
                "Enabled"
            } else {
                "Disabled"
            },
            if enabled && settings.enable_selling {
                "Enabled"
            } else {
                "Disabled"
            },
        );
        Self::say(cx, &line);
    }

    fn plan_round(&mut self, cx: &mut dyn VendorContext) {
        let vendor = cx.automation().vendor_state();
        if vendor.is_busy || cx.automation().items_busy() {
            return;
        }
        let owned = cx.automation().owned_items();
        let profile = vendor.profile.clone();
        let purse = Self::purse(cx, &owned, &profile);
        let buys = self.buy_candidates(cx, &vendor, &owned);
        let sells = self.sell_candidates(cx, &owned, &profile);
        let plan = planner::plan(&buys, &sells, &profile, &purse, &owned);

        let _ = cx.automation().vendor_clear_buy_list();
        let _ = cx.automation().vendor_clear_sell_list();
        self.pending_sell.clear();

        if let Some(fatal) = &plan.fatal {
            self.think_or_say(cx, &format!("AutoVendor Fatal - {fatal}"));
            self.stop(cx, Some(format!("Vendor run stopped: {fatal}")), false);
            return;
        }
        if let Some(split) = &plan.split {
            self.begin_split(cx, split);
            return;
        }
        if !plan.buys.is_empty() {
            self.commit_buys(cx, &plan, &purse);
            return;
        }
        if !plan.sells.is_empty() {
            self.commit_sells(cx, &plan, &purse);
            return;
        }
        let reason = format!("Vendor run finished: {}", self.vendor_name);
        self.stop(cx, Some(reason), false);
    }

    fn commit_buys(
        &mut self,
        cx: &mut dyn VendorContext,
        plan: &VendorTradePlan,
        purse: &VendorPurse,
    ) {
        for line in &plan.buys {
            let _ = cx
                .automation()
                .vendor_add_to_buy_list(line.template_object_id, line.count);
        }
        let result = cx.automation().vendor_buy_all();
        if result.status != VendorStatus::Sent {
            if result.status == VendorStatus::Busy {
                return;
            }
            self.stop_with_error(
                cx,
                format!(
                    "Vendor run stopped: buy refused ({:?}{}).",
                    result.status,
                    notice(result.notice.as_deref())
                ),
            );
            return;
        }
        self.coin_before = purse.coin_on_hand;
        self.expected_coin = -plan.buy_cost;
        self.coin_slack = plan.buy_slack;
        self.phase = Phase::AwaitTransaction;
        self.since_progress = 0.0;
        let lines = plan
            .buys
            .iter()
            .map(|line| format!("{} {}", line.count, line.name))
            .collect::<Vec<_>>()
            .join(", ");
        self.status = format!("Buying {lines} for {}.", n0(plan.buy_cost));
        self.attempt = format!(
            "tried {lines} for {} with {} coin counted",
            n0(plan.buy_cost),
            n0(purse.coin_on_hand)
        );
    }

    fn commit_sells(
        &mut self,
        cx: &mut dyn VendorContext,
        plan: &VendorTradePlan,
        purse: &VendorPurse,
    ) {
        for line in &plan.sells {
            let _ = cx.automation().vendor_add_to_sell_list(line.object_id);
            self.pending_sell.insert(line.object_id);
        }
        let result = cx.automation().vendor_sell_all();
        if result.status != VendorStatus::Sent {
            self.pending_sell.clear();
            if result.status == VendorStatus::Busy {
                return;
            }
            self.stop_with_error(
                cx,
                format!(
                    "Vendor run stopped: sell refused ({:?}{}).",
                    result.status,
                    notice(result.notice.as_deref())
                ),
            );
            return;
        }
        self.coin_before = purse.coin_on_hand;
        self.expected_coin = plan.sell_proceeds;
        self.coin_slack = 0;
        self.phase = Phase::AwaitTransaction;
        self.since_progress = 0.0;
        self.status = format!(
            "Selling {} item(s) for {}.",
            plan.sells.len(),
            n0(plan.sell_proceeds)
        );
        self.attempt = format!(
            "tried selling {} item(s) for {}",
            plan.sells.len(),
            n0(plan.sell_proceeds)
        );
    }

    fn tick_settle(&mut self, cx: &mut dyn VendorContext, step: f64) {
        self.since_completion += step;
        let owned = cx.automation().owned_items();
        let profile = cx.automation().vendor_state().profile;
        let coin_now = coin_on_hand(&owned, &profile);
        let remaining = self.expected_coin - (coin_now - self.coin_before);
        let sold_gone = !owned
            .iter()
            .any(|item| self.pending_sell.contains(&item.object_id));
        if (remaining.abs() < COIN_TOLERANCE + self.coin_slack && sold_gone)
            || self.since_completion >= SETTLE_TIMEOUT_SECONDS
        {
            self.pending_sell.clear();
            self.expected_coin = 0;
            self.phase = Phase::Plan;
            self.since_progress = 0.0;
        }
    }

    #[allow(clippy::cast_sign_loss)]
    fn begin_split(&mut self, cx: &mut dyn VendorContext, split: &VendorSplitRequest) {
        self.split_revision = cx.automation().last_inventory_completion().revision;
        let player = cx.automation().character().id;
        let result = cx
            .automation()
            .split_item(split.object_id, player, split.amount as u32, 0);
        if !result.accepted() {
            if result.status == ItemStatus::Busy {
                return;
            }
            self.stop_with_error(
                cx,
                format!(
                    "Vendor run stopped: could not split {} ({:?}{}).",
                    split.name,
                    result.status,
                    notice(result.notice.as_deref())
                ),
            );
            return;
        }
        self.split_object_id = split.object_id;
        self.split_waited = 0.0;
        self.phase = Phase::AwaitSplit;
        self.status = format!("Splitting {} off {} to sell.", split.amount, split.name);
    }

    fn tick_split(&mut self, cx: &mut dyn VendorContext, step: f64) {
        self.split_waited += step;
        let completion = cx.automation().last_inventory_completion();
        if completion.revision > self.split_revision && completion.source_id == self.split_object_id
        {
            if !(completion.revision != 0 && completion.weenie_error == 0) {
                self.stop_with_error(
                    cx,
                    format!(
                        "Vendor run stopped: the split failed (0x{:X}).",
                        completion.weenie_error
                    ),
                );
                return;
            }
            self.phase = Phase::Plan;
            self.since_progress = 0.0;
            return;
        }
        if self.split_waited >= SPLIT_TIMEOUT_SECONDS {
            self.stop_with_error(
                cx,
                "Vendor run stopped: the split was not answered.".to_owned(),
            );
        }
    }

    /// The loot rules' decision on an item (`LootRuleEngine.Decide`), which
    /// decides both what to buy and what to sell.
    fn decide(
        cx: &mut dyn VendorContext,
        item: &InventoryItem,
        properties: &ItemProperties,
        rules: &[LootRule],
        owned: &[InventoryItem],
    ) -> Option<LootDecision> {
        let host: &mut dyn Automation = cx.automation();
        rules::decide(item, properties, rules, owned, None, Some(host)).0
    }

    fn buy_candidates(
        &self,
        cx: &mut dyn VendorContext,
        vendor: &VendorState,
        owned: &[InventoryItem],
    ) -> Vec<VendorBuyCandidate> {
        if !cx.settings().enable_buying {
            return Vec::new();
        }
        let mut candidates = Vec::new();
        let mut buying_containers = false;
        for listing in &vendor.items {
            let (facade, properties) = facade(cx.automation(), listing);
            let Some(decided) = Self::decide(cx, &facade, &properties, &self.rules, owned) else {
                continue;
            };
            match decided.action {
                LootAction::KeepUpTo => {
                    let limit = self.rules[usize::try_from(decided.rule_index).unwrap_or_default()]
                        .keep_count
                        .max(0);
                    if listing.object_class == ObjectClass::Container {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                        let packs = owned
                            .iter()
                            .filter(|item| item.object_class == ObjectClass::Container)
                            .count() as i32;
                        if buying_containers || limit <= packs {
                            continue;
                        }
                        buying_containers = true;
                        candidates.push(VendorBuyCandidate {
                            item: listing.clone(),
                            wanted: limit - packs,
                            rule_name: decided.rule_name,
                        });
                        continue;
                    }
                    let held: i32 = owned
                        .iter()
                        .filter(|item| ordinal::equals_ignore_case(&item.name, &listing.name))
                        .map(|item| item.stack_size.max(1))
                        .sum();
                    if limit > held {
                        candidates.push(VendorBuyCandidate {
                            item: listing.clone(),
                            wanted: limit - held,
                            rule_name: decided.rule_name,
                        });
                    }
                }
                LootAction::Keep => candidates.push(VendorBuyCandidate {
                    item: listing.clone(),
                    wanted: i32::MAX,
                    rule_name: decided.rule_name,
                }),
                _ => {}
            }
        }
        planner::order_buys(candidates)
    }

    fn sell_candidates(
        &self,
        cx: &mut dyn VendorContext,
        owned: &[InventoryItem],
        profile: &VendorProfile,
    ) -> Vec<VendorSellCandidate> {
        let settings = cx.settings();
        if !settings.enable_selling || profile.alternate_currency_weenie_class_id != 0 {
            return Vec::new();
        }
        let player = cx.automation().character().id;
        let protected = cx.protected_items();
        let mut candidates = Vec::new();
        for item in owned {
            let properties = cx
                .automation()
                .item_properties(item.object_id)
                .unwrap_or_else(empty_properties);
            if !is_safe_to_sell(
                item,
                &properties,
                player,
                settings.only_from_main_pack,
                &protected,
            ) {
                continue;
            }
            let decision = Self::decide(cx, item, &properties, &self.rules, owned);
            let Some(decided) = decision.filter(|decided| decided.action == LootAction::Sell)
            else {
                continue;
            };
            if !planner::vendor_buys(item, profile) {
                continue;
            }
            candidates.push(VendorSellCandidate {
                item: item.clone(),
                rule_name: decided.rule_name,
            });
        }
        planner::order_sells(candidates, profile)
    }

    fn purse(
        cx: &mut dyn VendorContext,
        owned: &[InventoryItem],
        profile: &VendorProfile,
    ) -> VendorPurse {
        let coin_stack = owned
            .iter()
            .filter(|item| item.weenie_class_id == COIN_WEENIE_CLASS_ID)
            .map(|item| item.maximum_stack_size)
            .find(|size| *size > 1)
            .unwrap_or(0);
        VendorPurse {
            coin_on_hand: coin_on_hand(owned, profile),
            coin_stack_size: if coin_stack > 1 {
                coin_stack
            } else {
                DEFAULT_COIN_STACK_SIZE
            },
            free_slots: cx.automation().character().main_pack_free_slots.max(0),
        }
    }

    /// Where a carried item is, in the reference's words: "Main Pack" for
    /// the main pack, a side pack by its name and its place among the packs
    /// counted from one.
    fn item_location(item: &InventoryItem, owned: &[InventoryItem], player: u32) -> String {
        let container = item.container_id;
        if container == 0 || container == player {
            return "Main Pack".to_owned();
        }
        owned
            .iter()
            .find(|pack| pack.object_id == container)
            .map_or_else(
                || "Does Not Exist".to_owned(),
                |pack| format!("{} #{}", pack.name, 1 + pack.container_slot),
            )
    }

    fn report_test_mode(&mut self, cx: &mut dyn VendorContext) {
        let owned = cx.automation().owned_items();
        let vendor = cx.automation().vendor_state();
        let profile = vendor.profile.clone();
        // Three messages, as the reference prints them: the heading, then
        // the buy list and the sell list each under a tag of its own.
        Self::say(cx, "AutoVendor TEST MODE");
        let mut report = String::from("Buy Items:\n");
        let mut lines = 0;
        for buy in self.buy_candidates(cx, &vendor, &owned) {
            let category = SHOP_CATEGORIES
                .iter()
                .find(|(bit, _)| buy.item.item_type & bit != 0)
                .map_or_else(
                    || format!("Unknown Category 0x{:08X}", buy.item.item_type),
                    |(_, name)| (*name).to_owned(),
                );
            let count = if buy.wanted == i32::MAX {
                "\u{221E}".to_owned()
            } else {
                buy.wanted.to_string()
            };
            report.push_str(&format!(
                "  {category} -> {} * {count} - {}\n",
                buy.item.name, buy.rule_name
            ));
            lines += 1;
        }
        if lines == 0 {
            report.push_str("  (Nothing)\n");
        }
        Self::say(cx, report.trim_end());
        let mut report = String::from("Sell Items:\n");
        lines = 0;
        let player = cx.automation().character().id;
        let mut sells = self.sell_candidates(cx, &owned, &profile);
        sells.sort_by_key(|sell| sell.item.container_slot);
        for sell in sells {
            // Where the item is, then its name as a link that selects it, the
            // id written as the reference writes it: a signed number.
            #[allow(clippy::cast_possible_wrap)]
            let signed = sell.item.object_id as i32;
            report.push_str(&format!(
                "  {}: <Tell:IIDString:{REFERENCE_CHAT_LINK_ID}:select|{signed}>{}</Tell> - {}\n",
                Self::item_location(&sell.item, &owned, player),
                sell.item.name,
                sell.rule_name
            ));
            lines += 1;
        }
        if lines == 0 {
            report.push_str("  (Nothing)\n");
        }
        Self::say(cx, report.trim_end());
    }

    /// Starts opening a vendor by name. Nothing is said when the open
    /// starts; a vendor that cannot be found is the reference's "failed to
    /// open" line.
    fn begin_open(&mut self, cx: &mut dyn VendorContext, name: &str, partial: bool) -> Vec<String> {
        if !cx.automation().is_available() {
            return vec![ub_chat::tool_error(
                tools::AUTO_VENDOR,
                "Vendor open needs a live session.",
            )];
        }
        if cx.route_owns_vendor_open() {
            return vec![ub_chat::tool_error(
                tools::AUTO_VENDOR,
                "The route owns vendor opening while the macro runs; use an open-vendor waypoint or stop the macro first.",
            )];
        }
        let wanted = trim(name).to_owned();
        // The reference's object search: an id, a hex id or "selected"
        // first, then the nearest vendor the name answers to, and with no
        // name at all, the nearest vendor.
        let automation = cx.automation();
        let objects = automation.world_objects();
        let player = automation.navigation_snapshot();
        let view = SearchView {
            objects: &objects,
            self_id: automation.character().id,
            player: &player,
            selected: automation.selected_object(),
            open_container: automation.open_container_id(),
        };
        let Some(vendor) = try_find_nearest(&view, Some(&wanted), partial, &[ObjectClass::Vendor])
        else {
            self.status = format!("Vendor not found: {wanted}.");
            self.think_or_say(cx, "AutoVendor failed to open vendor");
            return Vec::new();
        };
        if cx.automation().active_vendor_id() == vendor.object_id {
            self.status = format!("Vendor is already open: {}.", vendor.name);
            return Vec::new();
        }
        self.open_target = vendor.object_id;
        self.open_name.clone_from(&vendor.name);
        self.open_tries = 0;
        // The first try goes out on the next tick, not after a full wait.
        self.since_open_try = f64::MAX;
        self.status = format!("Opening vendor: {}.", vendor.name);
        Vec::new()
    }

    fn tick_open(&mut self, cx: &mut dyn VendorContext, step: f64, can_act: bool) -> bool {
        if self.open_target == 0 {
            return false;
        }
        let vendor = cx.automation().vendor_state();
        if vendor.is_open && vendor.vendor_id == self.open_target {
            self.open_target = 0;
            self.open_tries = 0;
            self.status = format!("Vendor opened: {}.", self.open_name);
            return false;
        }
        if !can_act {
            return true;
        }
        if self.since_open_try < f64::MAX {
            self.since_open_try += step;
        }
        let settings = cx.settings();
        let wait = f64::from(settings.tries_time_milliseconds.max(0)) / 1000.0;
        if self.since_open_try < wait {
            return true;
        }
        if self.open_tries >= settings.tries.max(1) {
            self.status = format!(
                "Vendor open failed: {} did not open after {} tries.",
                self.open_name, self.open_tries
            );
            self.think_or_say(cx, "AutoVendor failed to open vendor");
            self.open_target = 0;
            self.open_tries = 0;
            return false;
        }
        self.open_tries += 1;
        self.since_open_try = 0.0;
        let result = cx.automation().use_item(self.open_target);
        self.status = if result.accepted() {
            format!("Using vendor {} (try {}).", self.open_name, self.open_tries)
        } else {
            format!(
                "Waiting to use vendor {}: {:?}.",
                self.open_name, result.status
            )
        };
        true
    }

    /// Adds a vendor's listing to the buy list, in the reference's words:
    /// first the line naming what was read from the command, said before
    /// anything is checked, then the item added or the error. Profiles wait
    /// on both lines before they buy.
    fn add_buy_by_name(
        cx: &mut dyn VendorContext,
        arguments: &str,
        count: i32,
        name: &str,
        partial: bool,
    ) -> Vec<String> {
        let heard = ub_chat::line(&format!("Name: {name} count: {count} param:{arguments}"));
        let vendor = cx.automation().vendor_state();
        if !vendor.is_open {
            return vec![heard, ub_chat::error("addbuy: No vendor open")];
        }
        for listing in &vendor.items {
            if !name_matches(&listing.name, name, partial) {
                continue;
            }
            let result = cx
                .automation()
                .vendor_add_to_buy_list(listing.template_id, count);
            return if result.status == VendorStatus::Sent {
                vec![
                    heard,
                    ub_chat::line(&format!(
                        "Added item to buy list: {} * {count}",
                        listing.name
                    )),
                ]
            } else {
                vec![
                    heard,
                    ub_chat::error(&format!(
                        "addbuy: {:?}{}",
                        result.status,
                        notice(result.notice.as_deref())
                    )),
                ]
            };
        }
        vec![
            heard,
            ub_chat::error(&format!(
                "addbuy: Unable to find item {}named '{name}' in vendor sell list",
                if partial { "partially " } else { "" }
            )),
        ]
    }

    /// Stages whole stacks until the count is met; when the count runs out
    /// part way through the only stack left, that stack is split first and
    /// the piece is staged once it exists.
    #[allow(clippy::cast_sign_loss)]
    fn add_sell_by_name(
        &mut self,
        cx: &mut dyn VendorContext,
        count: i32,
        name: &str,
        partial: bool,
    ) -> String {
        if !cx.automation().vendor_state().is_open {
            return ub_chat::error("addsell: No vendor open");
        }
        let player = cx.automation().character().id;
        let protected = cx.protected_items();
        let mut matches = Vec::new();
        for item in cx.automation().owned_items() {
            if !name_matches(&item.name, name, partial) {
                continue;
            }
            let properties = cx
                .automation()
                .item_properties(item.object_id)
                .unwrap_or_else(empty_properties);
            if is_safe_to_sell(&item, &properties, player, false, &protected) {
                matches.push(item);
            }
        }
        if matches.is_empty() {
            return ub_chat::error(&format!(
                "addsell: Unable to find item {}named '{name}' in inventory",
                if partial { "partially " } else { "" }
            ));
        }

        let mut needed = count;
        let mut oversized: Option<&InventoryItem> = None;
        for item in &matches {
            let stack = item.stack_size.max(1);
            if stack <= needed {
                let _ = cx.automation().vendor_add_to_sell_list(item.object_id);
                needed -= stack;
            } else if oversized.is_none_or(|current| current.stack_size.max(1) > stack) {
                oversized = Some(item);
            }
            if needed == 0 {
                break;
            }
        }
        let shown = &matches[0].name;
        if needed == 0 {
            return ub_chat::line(&format!("Added item to sell list: {shown} * {count}"));
        }
        // The reference's words, down to the stray "22" it ends this one on.
        let Some(source) = oversized else {
            return ub_chat::line(&format!(
                "Added item to sell list: {shown} * {count}, but was missing {needed} items 22"
            ));
        };

        let revision = cx.automation().last_inventory_completion().revision;
        let split = cx
            .automation()
            .split_item(source.object_id, player, needed as u32, 0);
        if !split.accepted() {
            return ub_chat::error(&format!(
                "addsell: could not split {}: {:?}{}",
                source.name,
                split.status,
                notice(split.notice.as_deref())
            ));
        }
        self.stage_split_object_id = source.object_id;
        self.stage_split_name.clone_from(&source.name);
        self.stage_split_needed = needed;
        self.stage_split_asked = count;
        self.stage_split_revision = revision;
        // Nothing is said until the split lands and the piece is staged.
        String::new()
    }

    fn observe_stage_split(&mut self, cx: &mut dyn VendorContext) {
        if self.stage_split_object_id == 0 {
            return;
        }
        let completion = cx.automation().last_inventory_completion();
        if completion.revision <= self.stage_split_revision
            || completion.source_id != self.stage_split_object_id
        {
            return;
        }
        self.stage_split_object_id = 0;
        if !(completion.revision != 0 && completion.weenie_error == 0) {
            let line = ub_chat::error(&format!(
                "addsell: the split of {} failed (0x{:X}).",
                self.stage_split_name, completion.weenie_error
            ));
            Self::say(cx, &line);
            return;
        }
        let piece = cx.automation().owned_items().into_iter().find(|item| {
            item.stack_size == self.stage_split_needed
                && ordinal::equals_ignore_case(&item.name, &self.stage_split_name)
        });
        match piece.filter(|piece| piece.object_id != 0) {
            None => {
                let line = format!(
                    "Added item to sell list: {} * {}, but was missing {} items",
                    self.stage_split_name,
                    self.stage_split_asked - self.stage_split_needed,
                    self.stage_split_needed
                );
                Self::say(cx, &line);
            }
            Some(piece) => {
                let _ = cx.automation().vendor_add_to_sell_list(piece.object_id);
                let line = format!(
                    "Added item to sell list: {} * {}",
                    self.stage_split_name, self.stage_split_asked
                );
                Self::say(cx, &line);
            }
        }
    }

    /// Ends the run. Every end the reference has (finished, cancelled,
    /// bailed, the vendor gone) says "AutoVendor finished: <vendor>", and
    /// profiles wait on that line to walk on; only a restart, which is
    /// quiet, does not.
    fn stop(&mut self, cx: &mut dyn VendorContext, status: Option<String>, quiet: bool) {
        self.running = false;
        self.pending_sell.clear();
        self.completed.clear();
        self.phase = Phase::StackCram;
        self.release_locks();
        if let Some(status) = status {
            self.status = status;
        }
        if !quiet {
            self.say_finished(cx);
        }
    }

    /// A stop this client has a reason for that the reference has no words
    /// of its own for: the reason as the tool's error, then the end.
    fn stop_with_error(&mut self, cx: &mut dyn VendorContext, status: String) {
        Self::say_error(cx, &status);
        self.stop(cx, Some(status), false);
    }

    fn say_finished(&self, cx: &mut dyn VendorContext) {
        let line = format!("AutoVendor finished: {}", self.vendor_name);
        self.think_or_say(cx, &line);
    }

    /// A line said as a think when `AutoVendor.Think` is on (what a meta's
    /// chat trigger reads), and as a tagged line otherwise.
    fn think_or_say(&self, cx: &mut dyn VendorContext, text: &str) {
        let think = cx.settings().think;
        let automation = cx.automation();
        if think {
            let _ = ub_chat::think(automation, text);
        } else {
            ub_chat::post(automation, &ub_chat::line(text));
        }
    }

    /// A plain line under the reference's tag.
    fn say(cx: &mut dyn VendorContext, text: &str) {
        ub_chat::post(cx.automation(), &ub_chat::line(text));
    }

    /// An error the tool reports for itself.
    fn say_error(cx: &mut dyn VendorContext, text: &str) {
        ub_chat::post(
            cx.automation(),
            &ub_chat::tool_error(tools::AUTO_VENDOR, text),
        );
    }
}

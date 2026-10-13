//! The money and slot planner (`VendorTradePlanner`): it reads the
//! vendor's terms (what it pays, what it deals in, its value limits)
//! against the coin and slots on hand and decides what one round can buy
//! or sell without overfilling the pack or overspending the purse.

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::trade::{NO_VALUE_LIMIT, VendorItem, VendorProfile};
use ac_plugin_abi::world::ObjectClass;
use mosstank_dotnet::text::ordinal;
use openac_plugin_compat::number::{format_f64, format_i32};

/// One of the vendor's listings a rule wants, and how many.
#[derive(Debug, Clone, PartialEq)]
pub struct VendorBuyCandidate {
    /// The listing.
    pub item: VendorItem,
    /// How many more the rule wants, or `i32::MAX` for a rule that keeps
    /// every one.
    pub wanted: i32,
    /// The rule that wanted it, for the report.
    pub rule_name: String,
}

/// One owned item a rule says to sell.
#[derive(Debug, Clone, PartialEq)]
pub struct VendorSellCandidate {
    /// The item.
    pub item: InventoryItem,
    /// The rule that said so.
    pub rule_name: String,
}

/// One line of a buy: a listing, a count, and what it costs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VendorBuyLine {
    /// The listing.
    pub template_object_id: u32,
    /// Its name.
    pub name: String,
    /// How many.
    pub count: i32,
    /// What they cost at the listed price.
    pub cost: i64,
}

/// One line of a sell: an owned stack and what the vendor pays for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VendorSellLine {
    /// The stack.
    pub object_id: u32,
    /// Its name.
    pub name: String,
    /// How many it holds.
    pub stack_size: i32,
    /// What the vendor pays.
    pub proceeds: i64,
}

/// A stack to split before the next round, because only part of it can be
/// sold this visit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VendorSplitRequest {
    /// The stack.
    pub object_id: u32,
    /// Its name.
    pub name: String,
    /// How many to split off.
    pub amount: i32,
}

/// What the character has to shop with: coin, how much of it one pack slot
/// holds, and the slots free to take coin or goods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VendorPurse {
    /// The coin carried.
    pub coin_on_hand: i64,
    /// How much coin one slot holds.
    pub coin_stack_size: i32,
    /// The free main-pack slots.
    pub free_slots: i32,
}

/// One round of a vendor visit. A round either buys or sells, never both:
/// buying first keeps the goods the rules said to sell available as the
/// coin that buys with, and the sell round after it sees the slots the buy
/// took.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VendorTradePlan {
    /// What to buy.
    pub buys: Vec<VendorBuyLine>,
    /// What the buys cost at the listed prices.
    pub buy_cost: i64,
    /// What to sell.
    pub sells: Vec<VendorSellLine>,
    /// What the sells fetch.
    pub sell_proceeds: i64,
    /// A stack to split first.
    pub split: Option<VendorSplitRequest>,
    /// Why the run cannot go on.
    pub fatal: Option<String>,
    /// How much more than [`Self::buy_cost`] the server may charge: it
    /// prices each stack as a whole and rounds once, where the listing
    /// rounds the single item.
    pub buy_slack: i64,
}

impl VendorTradePlan {
    /// Nothing to do.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buys.is_empty()
            && self.sells.is_empty()
            && self.split.is_none()
            && self.fatal.is_none()
    }

    fn fatal(reason: String) -> Self {
        Self {
            fatal: Some(reason),
            ..Self::default()
        }
    }

    fn split(request: VendorSplitRequest) -> Self {
        Self {
            split: Some(request),
            ..Self::default()
        }
    }
}

/// The most of one listing bought in a single round.
pub const MAXIMUM_BUY_COUNT: i32 = 5000;

/// The most items one sell transaction carries.
pub const MAXIMUM_SELL_LINES: usize = 99;

/// How much coin one pack slot holds when the character carries none to
/// read it from.
pub const DEFAULT_COIN_STACK_SIZE: i32 = 25000;

/// Pack slots kept free on top of what a buy or a sell needs, so the coin
/// change and the odd extra stack always have somewhere to go.
const SPARE_SLOTS: i32 = 2;

#[allow(clippy::cast_possible_truncation)]
fn stacks_for(count: i32, max_stack: i32) -> i32 {
    (f64::from(count) / f64::from(max_stack)).ceil() as i32
}

/// The most `count` items listed at `unit_price` can cost. The listing
/// rounds one item's price to the nearest pyreal (up, from a tenth over);
/// the server prices each stack it creates as a whole and rounds once, so a
/// stack can cost up to a tenth of a pyreal more per item, plus one for the
/// rounding of each stack.
#[must_use]
pub fn worst_case_cost(unit_price: i64, count: i32, max_stack: i32) -> i64 {
    unit_price * i64::from(count)
        + i64::from((count + 9) / 10)
        + i64::from(stacks_for(count, max_stack.max(1)))
}

/// The most items that can be bought with `coin` in the worst case.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn affordable_count(coin: i64, unit_price: i64, max_stack: i32) -> i32 {
    if coin <= 0 || unit_price <= 0 {
        return 0;
    }
    let guess = i64::from(i32::MAX).min(coin * 10 / (unit_price * 10 + 1));
    let mut count = guess as i32;
    while count > 0 && worst_case_cost(unit_price, count, max_stack) > coin {
        count -= 1;
    }
    while count < i32::MAX && worst_case_cost(unit_price, count + 1, max_stack) <= coin {
        count += 1;
    }
    count
}

/// The value of one unit: the stack's value over its size.
#[must_use]
pub fn unit_value(item: &InventoryItem) -> i64 {
    i64::from(item.value / item.stack_size.max(1))
}

/// What the vendor pays for one unit: its rate on the unit value, rounded
/// down and never below one coin, or the face value for a trade note.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
pub fn payout_per_unit(item: &InventoryItem, profile: &VendorProfile) -> i64 {
    let unit = unit_value(item);
    if item.object_class == ObjectClass::TradeNote {
        return unit;
    }
    if unit <= 0 {
        return 0;
    }
    let paid = (unit as f64 * f64::from(profile.buy_rate)).floor() as i64;
    paid.max(1)
}

/// Whether this vendor takes the item at all: a trade note always, and
/// anything else only in a category it deals in, worth something, and
/// within its per-unit value limits.
#[must_use]
pub fn vendor_buys(item: &InventoryItem, profile: &VendorProfile) -> bool {
    if item.object_class == ObjectClass::TradeNote {
        return true;
    }
    if profile.deals_in_item_types & item.item_type == 0 {
        return false;
    }
    let unit = unit_value(item);
    if unit <= 0 {
        return false;
    }
    if profile.minimum_value != NO_VALUE_LIMIT && unit < i64::from(profile.minimum_value) {
        return false;
    }
    profile.maximum_value == NO_VALUE_LIMIT || unit <= i64::from(profile.maximum_value)
}

fn note_last(class: ObjectClass) -> u8 {
    u8::from(class == ObjectClass::TradeNote)
}

/// Cheapest first, trade notes last.
#[must_use]
pub fn order_buys(mut candidates: Vec<VendorBuyCandidate>) -> Vec<VendorBuyCandidate> {
    candidates.sort_by_key(|candidate| {
        (
            note_last(candidate.item.object_class),
            candidate.item.unit_price,
        )
    });
    candidates
}

/// Lowest proceeds first, trade notes last.
#[must_use]
pub fn order_sells(
    mut candidates: Vec<VendorSellCandidate>,
    profile: &VendorProfile,
) -> Vec<VendorSellCandidate> {
    candidates.sort_by_key(|candidate| {
        (
            note_last(candidate.item.object_class),
            payout_per_unit(&candidate.item, profile) * i64::from(candidate.item.stack_size.max(1)),
        )
    });
    candidates
}

/// Whether that much coin fits in the pack with one slot still free: coin
/// arrives as stacks of its own and needs slots like anything else.
fn coin_fits(coin: i64, purse: &VendorPurse) -> bool {
    let stack_size = i64::from(purse.coin_stack_size.max(1));
    let stacks = (coin + stack_size - 1) / stack_size;
    i64::from(purse.free_slots) - stacks > 0
}

/// Plans one round. Buying wins the round when anything can be bought;
/// otherwise the round sells, and sells a single trade note only when a
/// buy is waiting on the coin. `buys` and `sells` come already ordered;
/// `owned` is everything carried, searched for a lone trade note when one
/// has to be sold.
#[must_use]
#[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
pub fn plan(
    buys: &[VendorBuyCandidate],
    sells: &[VendorSellCandidate],
    profile: &VendorProfile,
    purse: &VendorPurse,
    owned: &[InventoryItem],
) -> VendorTradePlan {
    let mut buy_lines = Vec::new();
    let mut buy_cost = 0_i64;
    let mut buy_slack = 0_i64;
    let mut buy_slots = 0_i32;
    for candidate in buys {
        let item = &candidate.item;
        let price = i64::from(item.unit_price);
        if price <= 0 {
            // The reference's words: the run prints them after its
            // "AutoVendor Fatal - " heading.
            #[allow(clippy::cast_precision_loss)]
            return VendorTradePlan::fatal(format!(
                "No vendor price found while adding {}x {}[0x{:08X}] Value: {}",
                format_i32(candidate.wanted, "n0").expect("a supported format"),
                item.name,
                item.template_id,
                format_f64(price as f64, "n").expect("a supported format")
            ));
        }
        let max_stack = item.max_stack_size.max(1);
        let note = item.object_class == ObjectClass::TradeNote;
        if worst_case_cost(price, 1, max_stack) <= purse.coin_on_hand - buy_cost - buy_slack
            && purse.free_slots - buy_slots > i32::from(!note)
        {
            let affordable = i64::from(affordable_count(
                purse.coin_on_hand - buy_cost - buy_slack,
                price,
                max_stack,
            ));
            let mut count = affordable.min(i64::from(candidate.wanted)) as i32;
            count = count.min(MAXIMUM_BUY_COUNT);
            let stacks = stacks_for(count, max_stack);
            if purse.free_slots < stacks + buy_slots {
                count = max_stack * ((purse.free_slots - SPARE_SLOTS) - buy_slots);
            }
            if count <= 0 {
                break;
            }

            buy_lines.push(VendorBuyLine {
                template_object_id: item.template_id,
                name: item.name.clone(),
                count,
                cost: price * i64::from(count),
            });
            buy_slots += stacks_for(count, max_stack);
            buy_cost += price * i64::from(count);
            buy_slack += worst_case_cost(price, count, max_stack) - price * i64::from(count);
            if candidate.wanted > count {
                break;
            }
        } else if !buy_lines.is_empty() {
            break;
        }
    }
    if !buy_lines.is_empty() {
        return VendorTradePlan {
            buys: buy_lines,
            buy_cost,
            buy_slack,
            ..VendorTradePlan::default()
        };
    }

    let next_buy = buys.first().map(|candidate| &candidate.item);
    let next_buy_is_note = next_buy.is_some_and(|item| item.object_class == ObjectClass::TradeNote);
    let mut sell_lines: Vec<VendorSellLine> = Vec::new();
    let mut proceeds = 0_i64;
    for candidate in sells {
        if sell_lines.len() >= MAXIMUM_SELL_LINES {
            break;
        }
        let item = &candidate.item;
        let value = payout_per_unit(item, profile);
        let stack = item.stack_size.max(1);

        if item.object_class == ObjectClass::TradeNote {
            // A note is coin in another shape: it is sold only to fund a buy
            // of something that is not itself a note, one at a time, and only
            // once the ordinary goods have gone first.
            if next_buy.is_none() || next_buy_is_note || !sell_lines.is_empty() {
                break;
            }
            if !coin_fits(value, purse) {
                return VendorTradePlan::fatal(format!("No inventory room to sell {}", item.name));
            }
            let lone = owned.iter().find(|other| {
                other.object_class == ObjectClass::TradeNote
                    && other.stack_size.max(1) == 1
                    && ordinal::equals_ignore_case(&other.name, &item.name)
            });
            if let Some(lone) = lone.filter(|lone| lone.object_id != 0) {
                sell_lines.push(VendorSellLine {
                    object_id: lone.object_id,
                    name: lone.name.clone(),
                    stack_size: 1,
                    proceeds: payout_per_unit(lone, profile),
                });
                proceeds += payout_per_unit(lone, profile);
                break;
            }
            return VendorTradePlan::split(VendorSplitRequest {
                object_id: item.object_id,
                name: item.name.clone(),
                amount: 1,
            });
        }

        if !coin_fits(proceeds + value * i64::from(stack), purse) {
            if !sell_lines.is_empty() {
                break;
            }
            let mut amount = if value > 0 {
                i64::from(purse.free_slots - SPARE_SLOTS) * i64::from(purse.coin_stack_size) / value
            } else {
                0
            };
            amount = amount.min(i64::from(stack - 1));
            if amount > 0 {
                return VendorTradePlan::split(VendorSplitRequest {
                    object_id: item.object_id,
                    name: item.name.clone(),
                    amount: amount as i32,
                });
            }
            return VendorTradePlan::fatal(format!("No inventory room to sell {}", item.name));
        }

        sell_lines.push(VendorSellLine {
            object_id: item.object_id,
            name: item.name.clone(),
            stack_size: stack,
            proceeds: value * i64::from(stack),
        });
        proceeds += value * i64::from(stack);
    }

    if sell_lines.is_empty() {
        VendorTradePlan::default()
    } else {
        VendorTradePlan {
            sells: sell_lines,
            sell_proceeds: proceeds,
            ..VendorTradePlan::default()
        }
    }
}

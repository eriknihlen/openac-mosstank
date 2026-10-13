//! Emptying an item's mana into a stone (`ManaStoneTransferPlanner`,
//! `ManaStoneFillConfirmation`): which stone, which donor, and whether the
//! drain that went out is confirmed.

use ac_plugin_abi::items::{InventoryItem, ItemUseCompletion};
use ac_plugin_abi::world::ItemProperties;
use mosstank_formats::utl::LootAction;

const MAGICAL_EFFECT: u32 = 0x0000_0001;
const INSCRIPTION_PROPERTY: u32 = 7;
const SCRIBE_PROPERTY: u32 = 8;
const TINKER_COUNT_PROPERTY: u32 = 171;

/// One drain: this stone, filled from that item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManaStoneTransferPlan {
    /// The stone.
    pub stone_object_id: u32,
    /// The item emptied into it.
    pub tank_object_id: u32,
    /// The stone's name.
    pub stone_name: String,
    /// The item's name.
    pub tank_name: String,
}

/// Whether an item is worn or wielded.
#[must_use]
pub fn is_equipped(item: &InventoryItem) -> bool {
    item.equipped_location != 0
}

/// An item worth emptying into a stone: something of the character's own
/// making that still holds mana. Four things decide it -- it carries its
/// own charge, it holds at least the floor the profile set, it has a
/// workmanship of its own, and nobody has tinkered it.
#[must_use]
pub fn is_donor(item: &InventoryItem, minimum_tank_mana: i32) -> bool {
    !is_equipped(item)
        && item.wielder_id == 0
        && item.effects & MAGICAL_EFFECT != 0
        && item.item_current_mana >= minimum_tank_mana
        && item.workmanship > 0.0
        && item.num_times_tinkered <= 0
}

fn has_text(properties: &ItemProperties, key: u32) -> bool {
    properties
        .strings
        .iter()
        .any(|(k, text)| *k == key && !text.is_empty())
}

/// Whether nobody has written on the item: an inscribed item carries both
/// the text and the name of whoever wrote it, and either one says somebody
/// meant to keep it.
#[must_use]
pub fn is_uninscribed(properties: &ItemProperties) -> bool {
    !has_text(properties, INSCRIPTION_PROPERTY) && !has_text(properties, SCRIBE_PROPERTY)
}

/// The second look, taken once the character holds the item: nobody has
/// written on it and nobody has tinkered it.
#[must_use]
pub fn is_drainable_in_hand(properties: &ItemProperties) -> bool {
    is_uninscribed(properties)
        && properties
            .int(TINKER_COUNT_PROPERTY)
            .is_none_or(|tinkers| tinkers <= 0)
}

/// What the planner asks about each candidate.
pub trait TransferChecks {
    /// A mana stone the profile names as one.
    fn is_profiled_mana_stone(&mut self, item: &InventoryItem) -> bool;

    /// Whether a stone may be spent on this pass.
    fn can_use_stone(&mut self, object_id: u32) -> bool {
        let _ = object_id;
        true
    }

    /// Whether an item may be emptied.
    fn can_drain_tank(&mut self, object_id: u32) -> bool {
        let _ = object_id;
        true
    }

    /// Whether the planner reads the writing on a donor at all.
    fn reads_writing(&self) -> bool {
        false
    }

    /// The donor's properties, `None` when the client cannot produce them.
    fn capture(&mut self, object_id: u32) -> Option<ItemProperties> {
        let _ = object_id;
        None
    }
}

/// The next drain: the lowest-id usable empty stone, and the classified
/// donor holding the most mana (lowest id on a tie). `classified` is what
/// the looter took each owned item as.
pub fn plan(
    owned: &[InventoryItem],
    classified: &dyn Fn(u32) -> Option<LootAction>,
    minimum_tank_mana: i32,
    checks: &mut dyn TransferChecks,
) -> Option<ManaStoneTransferPlan> {
    let mut stone: Option<&InventoryItem> = None;
    for item in owned {
        if !is_equipped(item)
            && item.wielder_id == 0
            && checks.can_use_stone(item.object_id)
            && item.effects & MAGICAL_EFFECT == 0
            && checks.is_profiled_mana_stone(item)
            && stone.is_none_or(|best| item.object_id < best.object_id)
        {
            stone = Some(item);
        }
    }
    let stone = stone.filter(|stone| stone.object_id != 0)?;

    let mut tanks: Vec<&InventoryItem> = Vec::new();
    for item in owned {
        if classified(item.object_id) == Some(LootAction::ManaTank)
            && is_donor(item, minimum_tank_mana)
            && checks.can_drain_tank(item.object_id)
            && (!checks.reads_writing()
                || checks
                    .capture(item.object_id)
                    .is_some_and(|written| is_uninscribed(&written)))
        {
            tanks.push(item);
        }
    }
    tanks.sort_by(|left, right| {
        right
            .item_current_mana
            .cmp(&left.item_current_mana)
            .then_with(|| left.object_id.cmp(&right.object_id))
    });
    let tank = tanks.first().filter(|tank| tank.object_id != 0)?;
    Some(ManaStoneTransferPlan {
        stone_object_id: stone.object_id,
        tank_object_id: tank.object_id,
        stone_name: stone.name.clone(),
        tank_name: tank.name.clone(),
    })
}

/// How a drain that went out stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManaFillOutcome {
    /// No answer yet.
    Waiting,
    /// The server answered for this pair, the donor is gone and the stone
    /// is charged (or gone).
    Confirmed,
    /// The server refused it, or nothing came in time.
    Uncertain,
}

/// Judges a drain that went out against the pack and the newest use the
/// server answered.
#[must_use]
pub fn observe_fill(
    plan: &ManaStoneTransferPlan,
    owned: &[InventoryItem],
    completion: &ItemUseCompletion,
    previous_revision: i64,
    timed_out: bool,
) -> ManaFillOutcome {
    let matches = completion.revision > previous_revision
        && completion.source_id == plan.stone_object_id
        && completion.target_id == plan.tank_object_id;
    let is_success = completion.revision != 0 && completion.weenie_error == 0;
    if matches && !is_success {
        return ManaFillOutcome::Uncertain;
    }
    let donor_gone = !owned
        .iter()
        .any(|item| item.object_id == plan.tank_object_id);
    let charge_changed = owned
        .iter()
        .find(|item| item.object_id == plan.stone_object_id)
        .is_none_or(|stone| stone.effects & MAGICAL_EFFECT != 0);
    if matches && donor_gone && charge_changed {
        return ManaFillOutcome::Confirmed;
    }
    if timed_out {
        ManaFillOutcome::Uncertain
    } else {
        ManaFillOutcome::Waiting
    }
}

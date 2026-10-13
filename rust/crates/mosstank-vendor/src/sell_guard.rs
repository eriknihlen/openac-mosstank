//! What is never sold whatever a rule says (`VendorSellGuard`). Coin is
//! not goods; what is worn or wielded is in use; a pack or a foci is
//! furniture; an inscribed, tinkered, attuned or retained item is one
//! somebody meant to keep; and anything the macro profile itself names (a
//! wand, a weapon) is what the character fights with.

use std::collections::HashSet;

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::{ItemProperties, ObjectClass};
use openac_plugin_compat::text::casing;

const INSCRIPTION_PROPERTY: u32 = 7;
const SCRIBE_PROPERTY: u32 = 8;
const RETAINED_PROPERTY: u32 = 88;
const ATTUNED_PROPERTY: u32 = 114;
const TINKER_COUNT_PROPERTY: u32 = 171;
const IMBUED_EFFECT_PROPERTY: u32 = 179;
const ICON_UNDERLAY_PROPERTY: u32 = 52;
const RARE_UNDERLAY: u32 = 0x0600_5B0C;

fn lookup<T: Copy>(values: &[(u32, T)], key: u32) -> Option<T> {
    values
        .iter()
        .find(|(found, _)| *found == key)
        .map(|(_, value)| *value)
}

fn has_text(properties: &ItemProperties, key: u32) -> bool {
    properties
        .strings
        .iter()
        .find(|(found, _)| *found == key)
        .is_some_and(|(_, text)| !casing::is_white_space_only(text))
}

/// Whether an item may be sold.
#[must_use]
pub fn is_safe_to_sell(
    item: &InventoryItem,
    properties: &ItemProperties,
    player_object_id: u32,
    only_from_main_pack: bool,
    protected_object_ids: &HashSet<u32>,
) -> bool {
    if item.object_id == 0 || item.value <= 0 {
        return false;
    }
    if matches!(
        item.object_class,
        ObjectClass::Money | ObjectClass::Container | ObjectClass::Foci
    ) {
        return false;
    }
    if item.equipped_location != 0 || item.wielder_id != 0 {
        return false;
    }
    if protected_object_ids.contains(&item.object_id) {
        return false;
    }
    if item.num_times_tinkered > 0 {
        return false;
    }
    if only_from_main_pack && item.container_id != player_object_id {
        return false;
    }
    if has_text(properties, INSCRIPTION_PROPERTY) || has_text(properties, SCRIBE_PROPERTY) {
        return false;
    }
    if lookup(&properties.ints, ATTUNED_PROPERTY).is_some_and(|attuned| attuned > 0)
        || lookup(&properties.ints, TINKER_COUNT_PROPERTY).is_some_and(|tinkers| tinkers > 0)
        || lookup(&properties.ints, IMBUED_EFFECT_PROPERTY).is_some_and(|imbued| imbued != 0)
    {
        return false;
    }
    if lookup(&properties.bools, RETAINED_PROPERTY) == Some(true) {
        return false;
    }
    lookup(&properties.data_ids, ICON_UNDERLAY_PROPERTY) != Some(RARE_UNDERLAY)
}

//! The spell list segment (`ItemInfo.cs:145-252`): the item's spells by
//! id, highest first, a loot-generated item's lesser ones left out.

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::magic::SpellInfo;

use std::fmt::Write as _;

use super::{Values, key, put};

/// The appraisal's mark on a spell active on the item.
const ACTIVE_ON_ITEM: u32 = 0x8000_0000;

/// The item's spells as Decal split the appraisal's list: `spells` (the
/// entries without the active mark, `wo.Spell(i)`) and `active` (those
/// with it, unmarked, `wo.ActiveSpell(i)`), each in the appraisal's order.
#[derive(Debug, Clone, Default)]
pub(crate) struct SpellLists {
    pub(crate) spells: Vec<u32>,
    pub(crate) active: Vec<u32>,
}

impl SpellLists {
    pub(crate) fn of(item: &InventoryItem) -> Self {
        let mut lists = Self::default();
        for &id in &item.appraised_spell_ids {
            if id & ACTIVE_ON_ITEM != 0 {
                lists.active.push(id & !ACTIVE_ON_ITEM);
            } else {
                lists.spells.push(id);
            }
        }
        lists
    }
}

/// The spell shows in the line (the filter of `ItemInfo.cs:161-246`).
fn shown(v: &Values<'_>, spell: &SpellInfo) -> bool {
    let name = spell.name.as_str();

    // If the item is not loot generated, show all spells.
    if !v.has_long(key::MATERIAL) {
        return true;
    }

    // Always show Minor/Major/Epic/Legendary Impen.
    if name.contains("Minor Impenetrability")
        || name.contains("Major Impenetrability")
        || name.contains("Epic Impenetrability")
        || name.contains("Legendary Impenetrability")
    {
        return true;
    }

    // Always show trinket spells.
    if name.contains("Augmented") {
        return true;
    }

    // Show banes and impen on unenchantable equipment, hide them on
    // enchantable equipment.
    if name.contains(" Bane") || name.contains("Impen") || name.starts_with("Brogard") {
        return v.long(key::UNENCHANTABLE) != 0;
    }

    let family = spell.family;
    if (152..=158).contains(&family) || family == 195 || family == 325 {
        // A weapon buff: level 6 hidden, 7 and 8+ shown.
        return match spell.difficulty {
            250 => false,
            300 => true,
            d => d >= 400,
        };
    }

    // Not a weapon buff: levels 1-6 by name, 7 by difficulty and 8 by name
    // hidden.
    let tiered = [" I", " II", " III", " IV", " V", " VI"]
        .iter()
        .any(|suffix| name.ends_with(suffix));
    !(tiered || spell.difficulty == 300 || name.contains("Incantation"))
}

/// Append `", " + name` for each spell shown; an id the spell table does
/// not know prints as `Unknown spell <id>`.
pub(crate) fn append(
    sb: &mut String,
    v: &Values<'_>,
    lists: &SpellLists,
    lookup: &mut dyn FnMut(u32) -> Option<SpellInfo>,
) {
    let mut ids = lists.spells.clone();
    ids.sort_unstable();
    ids.reverse();
    for id in ids {
        match lookup(id) {
            Some(spell) => {
                if shown(v, &spell) {
                    sb.push_str(", ");
                    sb.push_str(&spell.name);
                }
            }
            None => {
                put!(sb, ", Unknown spell {id}");
            }
        }
    }
}

//! The Inventory filters window's bindings: its six tabs over the
//! Inventory search's filter ([`mosstank_mag::inventory_search::InventorySearch::filter`])
//! and the window's own state ([`mosstank_mag::inventory_filter::window::FilterWindow`]).
//!
//! The per-item controls are named by their item: `MagFilterSlot<Slot>`,
//! `MagFilterWeapon<WeaponKind>`, `MagFilterRating<Rating>` and
//! `MagFilterReq<Req>` (each enum's variant name), each with its
//! `Set...` handler.

use ac_plugin_abi::ui::UiValue;
use mosstank_mag::inventory_filter::window::{FilterTab, FilterWindow, Req};
use mosstank_mag::inventory_filter::{InventoryFilter, ItemType, Rating, Slot, WeaponKind};

use super::{Arg, b, i, t, tl};
use crate::tank::TankCx;

/// The filters window's id.
pub(crate) const FILTERS_WINDOW: &str = "inventory-filters";

/// The tabs, with the names their bindings are made of.
const TABS: [(FilterTab, &str); 6] = [
    (FilterTab::Item, "Item"),
    (FilterTab::Cantrips, "Cantrips"),
    (FilterTab::Ratings, "Ratings"),
    (FilterTab::Set, "Set"),
    (FilterTab::Reqs, "Reqs"),
    (FilterTab::Characters, "Characters"),
];

/// The item `name` names after `prefix`, among `all` by variant name.
fn named<T: Copy + std::fmt::Debug>(name: &str, prefix: &str, all: &[T]) -> Option<T> {
    let key = name.strip_prefix(prefix)?;
    all.iter().copied().find(|item| format!("{item:?}") == key)
}

/// The tab a `MagFilters<Tab><suffix>` binding names.
fn tab_of(name: &str, suffix: &str) -> Option<FilterTab> {
    let key = name.strip_prefix("MagFilters")?.strip_suffix(suffix)?;
    TABS.iter()
        .find(|(_, tab)| *tab == key)
        .map(|(tab, _)| *tab)
}

pub(super) fn value(cx: &mut TankCx<'_>, name: &str) -> Option<UiValue> {
    if name == "MagFiltersShown" {
        return b(cx_shown(cx));
    }
    if let Some(tab) = tab_of(name, "Selected") {
        return b(cx.mag.filter_window.tab == tab);
    }
    if let Some(tab) = tab_of(name, "Visible") {
        let current = cx.mag.filter_window.tab;
        return b(current == tab && cx_shown(cx));
    }
    let window = &cx.mag.filter_window;
    let search = &cx.mag.inventory_search;
    let filter = search.filter();
    if let Some(slot) = named(name, "MagFilterSlot", &Slot::ALL) {
        return b(filter.slots.contains(&slot));
    }
    if let Some(kind) = named(name, "MagFilterWeapon", &WeaponKind::ALL) {
        return b(filter.weapon_kinds.contains(&kind));
    }
    if let Some(rating) = named(name, "MagFilterRating", &Rating::ALL) {
        return t(window.rating_text(rating));
    }
    if let Some(req) = named(name, "MagFilterReq", &Req::ALL) {
        return t(window.req_text(req));
    }
    match name {
        // Item.
        "MagFilterEquipped" => i(FilterWindow::equipped_index(filter)),
        "MagFilterBonded" => b(filter.bonded),
        "MagFilterAttuned" => b(filter.attuned),
        "MagFilterRare" => b(filter.rare),
        "MagFilterType" => i(FilterWindow::item_type_index(filter)),
        "MagFilterWeaponKindsVisible" => b(filter.item_type == ItemType::Weapons),
        // Cantrips (the list pushed only while its tab is shown).
        "MagFilterCantripFind" => t(window.cantrip_find()),
        "MagFilterSpellContains" => t(filter.spell_contains.as_str()),
        "MagFilterCantripChecks" => Some(UiValue::BoolList(window.cantrip_checks(filter))),
        "MagFilterCantripNames" => tl(window.cantrip_names(filter)),
        "MagFilterCantripCheckedText" => t(window.cantrip_checked_text(filter)),
        "MagFilterCantripCheckedOnly" => b(window.checked_only()),
        "MagFilterCantripTiers" => tl(window.cantrip_tiers(filter)),
        // Ratings: the allowed crit damage tiers.
        "MagFilterCd0" => b(filter.allowed_cd[0]),
        "MagFilterCd1" => b(filter.allowed_cd[1]),
        "MagFilterCd2" => b(filter.allowed_cd[2]),
        // Set (pushed only while its tab is shown).
        "MagFilterSetFind" => t(window.set_find()),
        "MagFilterSetNames" => tl(window.set_names(search.set_counts())),
        "MagFilterSetCounts" => tl(window.set_counts(search.set_counts())),
        "SelectedMagFilterSetRow" => i(window.selected_set_row(search.set_counts(), filter)),
        // Characters (pushed only while its tab is shown).
        "MagFilterCharacterNames" => tl(search.character_names()),
        "MagFilterCharacterChecks" => Some(UiValue::BoolList(
            window.character_checks(&search.character_names(), filter),
        )),
        // The lists select nothing: their boxes and the tier are the
        // controls.
        "SelectedMagFilterCantripRow" | "SelectedMagFilterCharacterRow" => i(-1),

        // The Inventory window's filter line.
        "MagInventoryFilterSummary" => {
            let summary = search.filter_summary();
            t(if summary.is_empty() {
                "No filters".to_owned()
            } else {
                summary
            })
        }
        _ => None,
    }
}

/// The filters window is shown (a dock button may not toggle it, but the
/// client is asked, as for the Inventory window).
fn cx_shown(cx: &mut TankCx<'_>) -> bool {
    cx.client.mag().ui_view_visible(FILTERS_WINDOW)
}

/// Edits the filter as last set.
fn edit(cx: &mut TankCx<'_>, f: impl FnOnce(&mut InventoryFilter)) {
    cx.mag.inventory_search.edit_filter(f);
}

pub(super) fn act(cx: &mut TankCx<'_>, name: &str, arg: &Arg) -> bool {
    let on = arg.int() != 0;
    if let Some(tab) = name.strip_prefix("Show").and_then(|rest| tab_of(rest, "")) {
        cx.mag.filter_window.tab = tab;
        return true;
    }
    if let Some(slot) = named(name, "SetMagFilterSlot", &Slot::ALL) {
        edit(cx, |filter| {
            if on {
                filter.slots.insert(slot);
            } else {
                filter.slots.remove(&slot);
            }
        });
        return true;
    }
    if let Some(kind) = named(name, "SetMagFilterWeapon", &WeaponKind::ALL) {
        edit(cx, |filter| {
            if on {
                filter.weapon_kinds.insert(kind);
            } else {
                filter.weapon_kinds.remove(&kind);
            }
        });
        return true;
    }
    let mag = &mut *cx.mag;
    let (window, search) = (&mut mag.filter_window, &mut mag.inventory_search);
    if let Some(rating) = named(name, "SetMagFilterRating", &Rating::ALL) {
        window.set_rating_text(search, rating, arg.text());
        return true;
    }
    if let Some(req) = named(name, "SetMagFilterReq", &Req::ALL) {
        window.set_req_text(search, req, arg.text());
        return true;
    }
    match name {
        // The Inventory window's Filters... button: open, or close when
        // shown; the window's Close button. The filters stay either way.
        "ToggleMagInventoryFilters" => {
            let shown = cx_shown(cx);
            cx.client.show_window(FILTERS_WINDOW, !shown);
        }
        "CloseMagInventoryFilters" => {
            cx.client.show_window(FILTERS_WINDOW, false);
        }
        "ClearMagInventoryFilters" => window.clear(search),
        // A tab's Reset (only the shown tab's button can be clicked).
        "ResetMagFiltersTab" => {
            let tab = window.tab;
            window.reset_tab(search, tab);
        }
        // Item.
        "SetMagFilterEquipped" => window.set_equipped(search, arg.int()),
        "SetMagFilterBonded" => search.edit_filter(|filter| filter.bonded = on),
        "SetMagFilterAttuned" => search.edit_filter(|filter| filter.attuned = on),
        "SetMagFilterRare" => search.edit_filter(|filter| filter.rare = on),
        "SetMagFilterType" => window.set_item_type(search, arg.int()),
        // Cantrips.
        "SetMagFilterCantripFind" => window.set_cantrip_find(arg.text()),
        "SetMagFilterSpellContains" => window.set_spell_contains(search, arg.text()),
        "ToggleMagFilterCantrip" => window.toggle_cantrip(search, arg.int()),
        "SetMagFilterCantripCheckedOnly" => window.set_checked_only(on),
        "CycleMagFilterCantripTier" => window.cycle_cantrip_tier(search, arg.int()),
        // Ratings.
        "SetMagFilterCd0" | "SetMagFilterCd1" | "SetMagFilterCd2" => {
            let tier = usize::from(name.as_bytes()[name.len() - 1] - b'0');
            search.edit_filter(|filter| filter.allowed_cd[tier] = on);
        }
        // Set.
        "SetMagFilterSetFind" => window.set_set_find(arg.text()),
        "SelectMagFilterSetRow" => {
            let counts = search.set_counts().to_vec();
            window.choose_set(search, &counts, arg.int());
        }
        // Characters.
        "ToggleMagFilterCharacter" => {
            let names = search.character_names();
            window.toggle_character(search, &names, arg.int());
        }
        "SelectAllMagFilterCharacters" => window.all_characters(search),
        "SelectNoMagFilterCharacters" => window.no_characters(search),
        // A click on a list's row: its box and its tier are the controls.
        "SelectMagFilterCantripRow" | "SelectMagFilterCharacterRow" => {}
        _ => return false,
    }
    true
}

//! The Inventory filters window: what each tab shows of the filter and
//! what its controls make of it.
//!
//! Source: the spec's "The windows" (`2026-11-09-mosstank-inventory-filters.md`):
//! six tabs; the Cantrips tab's per-cantrip tier, Legendary by default,
//! "tier or better"; the Set tab's "Any set" and counts; the Characters
//! tab's checklist with All and None. The filter is applied by the search
//! after its delay; the window reads the filter as last set.

use std::collections::BTreeSet;

use mosstank_mag::inventory_filter::window::{FilterTab, FilterWindow, Req};
use mosstank_mag::inventory_filter::{
    CantripTier, Equipped, InventoryFilter, ItemType, Rating, cantrip_index,
};
use mosstank_mag::inventory_search::{InventorySearch, SetCount};

fn rig() -> (FilterWindow, InventorySearch) {
    (FilterWindow::default(), InventorySearch::default())
}

fn strength() -> usize {
    cantrip_index("Strength").unwrap()
}

#[test]
fn the_window_opens_on_the_item_tab() {
    assert_eq!(FilterWindow::default().tab, FilterTab::Item);
}

// --- Cantrips ---

#[test]
fn every_cantrip_is_listed_with_its_group_on_the_groups_first_row() {
    let (window, search) = rig();
    let names = window.cantrip_names(search.filter());
    assert_eq!(names.len(), 47);
    assert_eq!(names[0], "Strength");
    let groups = window.cantrip_groups(search.filter());
    assert_eq!(groups[0], "Attributes");
    assert_eq!(groups[1], "");
    assert_eq!(groups[6], "Weapon skills");
    assert_eq!(window.cantrip_checks(search.filter()), vec![false; 47]);
    assert_eq!(window.cantrip_tiers(search.filter()), vec!["Legendary"; 47]);
}

#[test]
fn the_find_box_narrows_the_cantrips_by_name_ignoring_case() {
    let (mut window, search) = rig();
    window.set_cantrip_find("WARD");
    assert_eq!(
        window.cantrip_names(search.filter()),
        [
            "Flame Ward",
            "Frost Ward",
            "Acid Ward",
            "Storm Ward",
            "Slashing Ward",
            "Piercing Ward",
            "Bludgeoning Ward"
        ]
    );
    // The group heads the first row shown.
    assert_eq!(window.cantrip_groups(search.filter())[0], "Wards & Banes");
    window.set_cantrip_find("");
    assert_eq!(window.cantrip_names(search.filter()).len(), 47);
}

#[test]
fn a_checked_cantrip_is_filtered_at_its_rows_tier() {
    let (mut window, mut search) = rig();
    // Strength's tier set to Epic, then checked.
    window.cycle_cantrip_tier(&mut search, 0);
    assert_eq!(window.cantrip_tiers(search.filter())[0], "Minor");
    window.cycle_cantrip_tier(&mut search, 0);
    window.cycle_cantrip_tier(&mut search, 0);
    assert_eq!(window.cantrip_tiers(search.filter())[0], "Epic");
    assert!(
        search.filter().cantrips.is_empty(),
        "not checked: no filter"
    );
    window.toggle_cantrip(&mut search, 0);
    assert_eq!(search.filter().cantrips, [(strength(), CantripTier::Epic)]);
    assert!(window.cantrip_checks(search.filter())[0]);
    // A checked cantrip's tier changes the filter.
    window.cycle_cantrip_tier(&mut search, 0);
    assert_eq!(
        search.filter().cantrips,
        [(strength(), CantripTier::Legendary)]
    );
    // Unchecked: gone from the filter, its tier kept.
    window.toggle_cantrip(&mut search, 0);
    assert!(search.filter().cantrips.is_empty());
    assert_eq!(window.cantrip_tiers(search.filter())[0], "Legendary");
}

#[test]
fn a_row_of_the_narrowed_list_is_its_own_cantrip() {
    let (mut window, mut search) = rig();
    window.set_cantrip_find("invuln");
    window.toggle_cantrip(&mut search, 0);
    assert_eq!(
        search.filter().cantrips,
        [(
            cantrip_index("Invulnerability").unwrap(),
            CantripTier::Legendary
        )]
    );
    // A row past the list does nothing.
    window.toggle_cantrip(&mut search, 5);
    window.cycle_cantrip_tier(&mut search, -1);
    assert_eq!(search.filter().cantrips.len(), 1);
}

#[test]
fn the_spell_text_is_the_filters() {
    let (window, mut search) = rig();
    window.set_spell_contains(&mut search, "Fire");
    assert_eq!(search.filter().spell_contains, "Fire");
}

#[test]
fn the_cantrips_tab_counts_the_checked_and_can_list_only_them() {
    let (mut window, mut search) = rig();
    assert_eq!(window.cantrip_checked_text(search.filter()), "None checked");
    window.toggle_cantrip(&mut search, 0);
    window.toggle_cantrip(&mut search, 2);
    assert_eq!(window.cantrip_checked_text(search.filter()), "2 checked");
    assert!(!window.checked_only());
    window.set_checked_only(true);
    assert_eq!(
        window.cantrip_names(search.filter()),
        ["Strength", "Quickness"]
    );
    // A row of the short list is its own cantrip: unchecking it leaves it.
    window.toggle_cantrip(&mut search, 1);
    assert_eq!(window.cantrip_names(search.filter()), ["Strength"]);
    assert_eq!(window.cantrip_checked_text(search.filter()), "1 checked");
    // With the find box too.
    window.set_cantrip_find("quick");
    assert!(window.cantrip_names(search.filter()).is_empty());
    window.set_checked_only(false);
    assert_eq!(window.cantrip_names(search.filter()), ["Quickness"]);
}

// --- Item ---

#[test]
fn the_item_tabs_choices_are_indices_into_their_lists() {
    let (window, mut search) = rig();
    window.set_equipped(&mut search, 2);
    assert_eq!(search.filter().equipped, Equipped::No);
    assert_eq!(FilterWindow::equipped_index(search.filter()), 2);
    window.set_item_type(&mut search, 3);
    assert_eq!(search.filter().item_type, ItemType::Weapons);
    assert_eq!(FilterWindow::item_type_index(search.filter()), 3);
    // Out of range: unchanged.
    window.set_item_type(&mut search, 9);
    assert_eq!(search.filter().item_type, ItemType::Weapons);
}

// --- Ratings and requirements ---

#[test]
fn a_number_field_keeps_its_text_and_sets_its_filter_when_it_reads() {
    let (mut window, mut search) = rig();
    window.set_rating_text(&mut search, Rating::CritDamage, " 2 ");
    assert_eq!(window.rating_text(Rating::CritDamage), " 2 ");
    assert_eq!(search.filter().rating(Rating::CritDamage), Some(2));
    // Text that is not a number sets none, and stays as typed.
    window.set_rating_text(&mut search, Rating::CritDamage, "2x");
    assert_eq!(window.rating_text(Rating::CritDamage), "2x");
    assert_eq!(search.filter().rating(Rating::CritDamage), None);
    // Grouped digits read.
    window.set_req_text(&mut search, Req::MinValue, "25,000");
    assert_eq!(search.filter().min_value, Some(25_000));
    window.set_req_text(&mut search, Req::MaxWield, "180");
    window.set_req_text(&mut search, Req::MinWorkmanship, "6");
    window.set_req_text(&mut search, Req::MaxBurden, "500");
    let filter = search.filter();
    assert_eq!(
        (filter.max_wield, filter.min_workmanship, filter.max_burden),
        (Some(180), Some(6), Some(500))
    );
    assert_eq!(window.req_text(Req::MaxBurden), "500");
    window.set_req_text(&mut search, Req::MaxBurden, "");
    assert_eq!(search.filter().max_burden, None);
}

// --- Set ---

fn counts() -> Vec<SetCount> {
    vec![
        SetCount {
            id: 13,
            name: "Soldier's Set".to_owned(),
            count: 3,
        },
        SetCount {
            id: 14,
            name: "Adept's Set".to_owned(),
            count: 1,
        },
    ]
}

#[test]
fn the_set_tab_lists_any_set_then_each_set_with_its_count() {
    let (mut window, mut search) = rig();
    assert_eq!(
        window.set_names(&counts()),
        ["Any set", "Soldier's Set", "Adept's Set"]
    );
    assert_eq!(window.set_counts(&counts()), ["", "3", "1"]);
    assert_eq!(window.selected_set_row(&counts(), search.filter()), 0);
    window.choose_set(&mut search, &counts(), 2);
    assert_eq!(search.filter().set, Some(14));
    assert_eq!(window.selected_set_row(&counts(), search.filter()), 2);
    // Found by name; the chosen set's row follows.
    window.set_set_find("adept");
    assert_eq!(window.set_names(&counts()), ["Any set", "Adept's Set"]);
    assert_eq!(window.selected_set_row(&counts(), search.filter()), 1);
    window.choose_set(&mut search, &counts(), 0);
    assert_eq!(search.filter().set, None);
}

// --- Characters ---

fn names() -> Vec<String> {
    vec!["Me".to_owned(), "Alt One".to_owned(), "Alt Two".to_owned()]
}

#[test]
fn the_characters_tab_checks_every_character_until_one_is_unchecked() {
    let (window, mut search) = rig();
    assert_eq!(
        window.character_checks(&names(), search.filter()),
        [true, true, true]
    );
    window.toggle_character(&mut search, &names(), 1);
    assert_eq!(
        search.filter().characters,
        Some(BTreeSet::from(["Me".to_owned(), "Alt Two".to_owned()]))
    );
    assert_eq!(
        window.character_checks(&names(), search.filter()),
        [true, false, true]
    );
    // Every one checked again: no filter.
    window.toggle_character(&mut search, &names(), 1);
    assert_eq!(search.filter().characters, None);
    window.no_characters(&mut search);
    assert_eq!(search.filter().characters, Some(BTreeSet::new()));
    assert_eq!(
        window.character_checks(&names(), search.filter()),
        [false, false, false]
    );
    window.all_characters(&mut search);
    assert_eq!(search.filter().characters, None);
}

// --- Reset, one tab at a time ---

/// A filter with something set on every tab.
fn every_tab_set(window: &mut FilterWindow, search: &mut InventorySearch) {
    window.set_equipped(search, 1);
    window.set_item_type(search, 3);
    search.edit_filter(|filter| {
        filter.bonded = true;
        filter
            .slots
            .insert(mosstank_mag::inventory_filter::Slot::Ring);
        filter.allowed_cd = [true, false, false];
        filter.set = Some(13);
        filter.characters = Some(BTreeSet::from(["Me".to_owned()]));
    });
    window.cycle_cantrip_tier(search, 0);
    window.toggle_cantrip(search, 0);
    window.set_spell_contains(search, "Fire");
    window.set_checked_only(true);
    window.set_rating_text(search, Rating::Tinks, "3");
    window.set_req_text(search, Req::MinValue, "100");
}

#[test]
fn reset_clears_only_the_tabs_own_filters() {
    let (mut window, mut search) = rig();
    every_tab_set(&mut window, &mut search);
    let before = search.filter().clone();
    for tab in [
        FilterTab::Item,
        FilterTab::Cantrips,
        FilterTab::Ratings,
        FilterTab::Set,
        FilterTab::Reqs,
        FilterTab::Characters,
    ] {
        let (mut window, mut search) = rig();
        every_tab_set(&mut window, &mut search);
        window.reset_tab(&mut search, tab);
        let after = search.filter().clone();
        let mut expected = before.clone();
        let fresh = InventoryFilter::default();
        match tab {
            FilterTab::Item => {
                expected.equipped = fresh.equipped;
                expected.bonded = false;
                expected.item_type = fresh.item_type;
                expected.slots.clear();
            }
            FilterTab::Cantrips => {
                expected.cantrips.clear();
                expected.spell_contains.clear();
                assert!(!window.checked_only());
                assert_eq!(window.cantrip_tiers(&after)[0], "Legendary");
            }
            FilterTab::Ratings => {
                expected.ratings = fresh.ratings;
                expected.allowed_cd = fresh.allowed_cd;
                assert_eq!(window.rating_text(Rating::Tinks), "");
            }
            FilterTab::Set => expected.set = None,
            FilterTab::Reqs => {
                expected.min_value = None;
                assert_eq!(window.req_text(Req::MinValue), "");
            }
            FilterTab::Characters => expected.characters = None,
        }
        assert_eq!(after, expected, "{tab:?}");
        // The other tabs' fields keep their text.
        if tab != FilterTab::Ratings {
            assert_eq!(window.rating_text(Rating::Tinks), "3", "{tab:?}");
        }
    }
}

// --- Clear ---

#[test]
fn clear_empties_the_filter_and_the_number_fields() {
    let (mut window, mut search) = rig();
    window.set_rating_text(&mut search, Rating::DamageRating, "3");
    window.toggle_cantrip(&mut search, 0);
    window.clear(&mut search);
    assert_eq!(*search.filter(), InventoryFilter::default());
    assert_eq!(window.rating_text(Rating::DamageRating), "");
}

//! The Inventory filters window ([`FilterWindow`]): what each of its six
//! tabs shows of the filter, and what its controls make of it. The filter
//! itself is the search's ([`InventorySearch::filter`], as last set); the
//! window keeps only what is its own: the tab shown, the find boxes, each
//! cantrip's tier while it is unchecked, and the number fields' text as
//! typed.
//!
//! - Cantrips: a heading row per group, then one row per cantrip the find
//!   box leaves (its name containing the text, ignoring case), only the
//!   checked ones when "Checked only" is on; a group with none left has no
//!   heading. A cantrip's row has a checkbox, the name, and the tier, a
//!   click on which steps it Minor, Major, Epic, Legendary and round again
//!   (Legendary to start); a heading's is blank and does nothing. A checked
//!   cantrip is filtered at its tier or better. A line counts the checked.
//! - Ratings and Reqs & value: a field each, its text kept as typed; a
//!   whole number (digits grouped by commas or not) sets the filter, any
//!   other text sets none.
//! - Set: "Any set", then each set of the set table the find box leaves,
//!   with its count (0 when nobody searched holds it).
//! - Characters: every character checked until one is unchecked; All and
//!   None.
//!
//! Each tab's Reset clears that tab's filters only ([`FilterWindow::reset_tab`]);
//! the Inventory window's Clear clears them all ([`FilterWindow::clear`]).

use super::{CantripTier, Equipped, GROUPS, InventoryFilter, ItemType, Rating};
use crate::inventory_search::{InventorySearch, SetCount};

/// The window's tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilterTab {
    /// Worn, bonded, type, slots.
    #[default]
    Item,
    /// The cantrips and the spell text.
    Cantrips,
    /// The ratings' minimums and the allowed crit damage.
    Ratings,
    /// The equipment set.
    Set,
    /// Wield level, value, workmanship, burden.
    Reqs,
    /// The characters searched.
    Characters,
}

/// A field of the Reqs & value tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Req {
    /// The highest wield level.
    MaxWield,
    /// The lowest value.
    MinValue,
    /// The lowest workmanship.
    MinWorkmanship,
    /// The highest burden.
    MaxBurden,
}

impl Req {
    /// Every field, in the tab's order.
    pub const ALL: [Self; 4] = [
        Self::MaxWield,
        Self::MinValue,
        Self::MinWorkmanship,
        Self::MaxBurden,
    ];

    /// Its label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::MaxWield => "Wield level ≤",
            Self::MinValue => "Value ≥",
            Self::MinWorkmanship => "Workmanship ≥",
            Self::MaxBurden => "Burden ≤",
        }
    }

    fn slot(self, filter: &mut InventoryFilter) -> &mut Option<i32> {
        match self {
            Self::MaxWield => &mut filter.max_wield,
            Self::MinValue => &mut filter.min_value,
            Self::MinWorkmanship => &mut filter.min_workmanship,
            Self::MaxBurden => &mut filter.max_burden,
        }
    }
}

/// A row of the Cantrips list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CantripRow {
    /// A group's heading (by its place in [`GROUPS`]): no box ticked, no
    /// tier, a click does nothing.
    Heading(usize),
    /// A cantrip (by its index).
    Cantrip(usize),
}

/// The label of the Set tab's first row.
pub const ANY_SET: &str = "Any set";

/// What the filters window keeps of its own.
#[derive(Debug, Clone)]
pub struct FilterWindow {
    /// The tab shown.
    pub tab: FilterTab,
    cantrip_find: String,
    /// Only the checked cantrips are listed.
    checked_only: bool,
    /// Each cantrip's tier while it is unchecked (by its index).
    tiers: Vec<CantripTier>,
    set_find: String,
    rating_texts: [String; Rating::ALL.len()],
    req_texts: [String; Req::ALL.len()],
}

impl Default for FilterWindow {
    fn default() -> Self {
        Self {
            tab: FilterTab::Item,
            cantrip_find: String::new(),
            checked_only: false,
            tiers: vec![CantripTier::Legendary; super::cantrips::names().count()],
            set_find: String::new(),
            rating_texts: Default::default(),
            req_texts: Default::default(),
        }
    }
}

/// `text` contains `needle` (lowercase), ignoring case.
fn contains(text: &str, needle: &str) -> bool {
    needle.is_empty() || text.to_lowercase().contains(needle)
}

/// A field's whole number: digits, a sign, commas between them; blank or
/// anything else is none.
fn read_number(text: &str) -> Option<i32> {
    let digits: String = text.trim().chars().filter(|c| *c != ',').collect();
    digits.parse().ok()
}

/// `row` of `rows`, when it is one.
fn row_of<T: Copy>(rows: &[T], row: i32) -> Option<T> {
    usize::try_from(row)
        .ok()
        .and_then(|row| rows.get(row).copied())
}

impl FilterWindow {
    // --- Cantrips ---

    /// The Cantrips tab's find box.
    #[must_use]
    pub fn cantrip_find(&self) -> &str {
        &self.cantrip_find
    }

    /// The find box typed in.
    pub fn set_cantrip_find(&mut self, text: &str) {
        text.clone_into(&mut self.cantrip_find);
    }

    /// Only the checked cantrips are listed.
    #[must_use]
    pub fn checked_only(&self) -> bool {
        self.checked_only
    }

    /// The "Checked only" switch.
    pub fn set_checked_only(&mut self, on: bool) {
        self.checked_only = on;
    }

    /// How many cantrips are checked: `None checked`, `N checked`.
    #[must_use]
    pub fn cantrip_checked_text(&self, filter: &InventoryFilter) -> String {
        match filter.cantrips.len() {
            0 => "None checked".to_owned(),
            n => format!("{n} checked"),
        }
    }

    /// The Cantrips list's rows: each group's heading, then its cantrips
    /// the find box (and "Checked only") leave; a group with none left has
    /// no heading.
    fn cantrip_rows(&self, filter: &InventoryFilter) -> Vec<CantripRow> {
        let needle = self.cantrip_find.trim().to_lowercase();
        let checked = |index: usize| filter.cantrips.iter().any(|(found, _)| *found == index);
        let mut rows = Vec::new();
        let mut index = 0;
        for (group_index, group) in GROUPS.iter().enumerate() {
            let mut headed = false;
            for name in group.cantrips {
                if contains(name, &needle) && (!self.checked_only || checked(index)) {
                    if !headed {
                        rows.push(CantripRow::Heading(group_index));
                        headed = true;
                    }
                    rows.push(CantripRow::Cantrip(index));
                }
                index += 1;
            }
        }
        rows
    }

    /// The cantrip a row is (none for a heading or a row past the list).
    fn cantrip_at(&self, filter: &InventoryFilter, row: i32) -> Option<usize> {
        match row_of(&self.cantrip_rows(filter), row)? {
            CantripRow::Cantrip(index) => Some(index),
            CantripRow::Heading(_) => None,
        }
    }

    /// The name column: a heading's `— Group —`, a cantrip's name.
    #[must_use]
    pub fn cantrip_names(&self, filter: &InventoryFilter) -> Vec<String> {
        self.cantrip_rows(filter)
            .into_iter()
            .map(|row| match row {
                CantripRow::Heading(group) => format!("— {} —", GROUPS[group].name),
                CantripRow::Cantrip(index) => {
                    super::cantrip_name(index).unwrap_or_default().to_owned()
                }
            })
            .collect()
    }

    /// The checkbox column: the cantrip is in the filter (a heading's box
    /// is never ticked).
    #[must_use]
    pub fn cantrip_checks(&self, filter: &InventoryFilter) -> Vec<bool> {
        self.cantrip_rows(filter)
            .into_iter()
            .map(|row| match row {
                CantripRow::Heading(_) => false,
                CantripRow::Cantrip(index) => {
                    filter.cantrips.iter().any(|(found, _)| *found == index)
                }
            })
            .collect()
    }

    /// The tier a cantrip shows: the filter's while it is checked, else
    /// the row's own.
    fn tier_of(&self, filter: &InventoryFilter, index: usize) -> CantripTier {
        filter
            .cantrips
            .iter()
            .find(|(found, _)| *found == index)
            .map_or(self.tiers[index], |(_, tier)| *tier)
    }

    /// The Tier column: blank on a heading (an empty caption draws no
    /// button).
    #[must_use]
    pub fn cantrip_tiers(&self, filter: &InventoryFilter) -> Vec<String> {
        self.cantrip_rows(filter)
            .into_iter()
            .map(|row| match row {
                CantripRow::Heading(_) => String::new(),
                CantripRow::Cantrip(index) => self.tier_of(filter, index).name().to_owned(),
            })
            .collect()
    }

    /// A row's checkbox (or name) clicked: the cantrip is added to the filter at its
    /// tier, or taken out of it.
    pub fn toggle_cantrip(&mut self, search: &mut InventorySearch, row: i32) {
        let Some(index) = self.cantrip_at(search.filter(), row) else {
            return;
        };
        let tier = self.tier_of(search.filter(), index);
        self.tiers[index] = tier;
        search.edit_filter(|filter| {
            if let Some(at) = filter
                .cantrips
                .iter()
                .position(|(found, _)| *found == index)
            {
                filter.cantrips.remove(at);
            } else {
                filter.cantrips.push((index, tier));
            }
        });
    }

    /// A row's tier clicked: the next tier (after Legendary, Minor), in the
    /// filter too when the cantrip is checked.
    pub fn cycle_cantrip_tier(&mut self, search: &mut InventorySearch, row: i32) {
        let Some(index) = self.cantrip_at(search.filter(), row) else {
            return;
        };
        let current = self.tier_of(search.filter(), index);
        let at = CantripTier::ALL
            .iter()
            .position(|tier| *tier == current)
            .unwrap_or(0);
        let next = CantripTier::ALL[(at + 1) % CantripTier::ALL.len()];
        self.tiers[index] = next;
        if search
            .filter()
            .cantrips
            .iter()
            .any(|(found, _)| *found == index)
        {
            search.edit_filter(|filter| {
                for (found, tier) in &mut filter.cantrips {
                    if *found == index {
                        *tier = next;
                    }
                }
            });
        }
    }

    /// The "spell name contains" box typed in.
    pub fn set_spell_contains(&self, search: &mut InventorySearch, text: &str) {
        search.edit_filter(|filter| text.clone_into(&mut filter.spell_contains));
    }

    // --- Item ---

    /// The Worn choice's place in Any, Equipped, Not equipped.
    #[must_use]
    pub fn equipped_index(filter: &InventoryFilter) -> i32 {
        position(&Equipped::ALL, filter.equipped)
    }

    /// The Worn choice made.
    pub fn set_equipped(&self, search: &mut InventorySearch, index: i32) {
        if let Some(equipped) = row_of(&Equipped::ALL, index) {
            search.edit_filter(|filter| filter.equipped = equipped);
        }
    }

    /// The Type choice's place in [`ItemType::ALL`].
    #[must_use]
    pub fn item_type_index(filter: &InventoryFilter) -> i32 {
        position(&ItemType::ALL, filter.item_type)
    }

    /// The Type choice made.
    pub fn set_item_type(&self, search: &mut InventorySearch, index: i32) {
        if let Some(item_type) = row_of(&ItemType::ALL, index) {
            search.edit_filter(|filter| filter.item_type = item_type);
        }
    }

    // --- Ratings, Reqs & value ---

    /// A rating's field, as typed.
    #[must_use]
    pub fn rating_text(&self, rating: Rating) -> &str {
        &self.rating_texts[rating.index()]
    }

    /// A rating's field typed in.
    pub fn set_rating_text(&mut self, search: &mut InventorySearch, rating: Rating, text: &str) {
        text.clone_into(&mut self.rating_texts[rating.index()]);
        let minimum = read_number(text);
        search.edit_filter(|filter| filter.set_rating(rating, minimum));
    }

    /// A requirement's field, as typed.
    #[must_use]
    pub fn req_text(&self, req: Req) -> &str {
        &self.req_texts[req as usize]
    }

    /// A requirement's field typed in.
    pub fn set_req_text(&mut self, search: &mut InventorySearch, req: Req, text: &str) {
        text.clone_into(&mut self.req_texts[req as usize]);
        let value = read_number(text);
        search.edit_filter(|filter| *req.slot(filter) = value);
    }

    // --- Set ---

    /// The Set tab's find box.
    #[must_use]
    pub fn set_find(&self) -> &str {
        &self.set_find
    }

    /// The find box typed in.
    pub fn set_set_find(&mut self, text: &str) {
        text.clone_into(&mut self.set_find);
    }

    /// The sets listed: none (Any set), then those the find box leaves.
    fn set_rows<'a>(&self, counts: &'a [SetCount]) -> Vec<Option<&'a SetCount>> {
        let needle = self.set_find.trim().to_lowercase();
        std::iter::once(None)
            .chain(
                counts
                    .iter()
                    .filter(|count| contains(&count.name, &needle))
                    .map(Some),
            )
            .collect()
    }

    /// The Set column.
    #[must_use]
    pub fn set_names(&self, counts: &[SetCount]) -> Vec<String> {
        self.set_rows(counts)
            .into_iter()
            .map(|row| row.map_or(ANY_SET, |count| count.name.as_str()).to_owned())
            .collect()
    }

    /// The Items column: how many items of the characters searched are of
    /// the set.
    #[must_use]
    pub fn set_counts(&self, counts: &[SetCount]) -> Vec<String> {
        self.set_rows(counts)
            .into_iter()
            .map(|row| row.map_or_else(String::new, |count| count.count.to_string()))
            .collect()
    }

    /// The chosen set's row; -1 when it is not listed.
    #[must_use]
    pub fn selected_set_row(&self, counts: &[SetCount], filter: &InventoryFilter) -> i32 {
        self.set_rows(counts)
            .iter()
            .position(|row| row.map(|count| count.id) == filter.set)
            .and_then(|row| i32::try_from(row).ok())
            .unwrap_or(-1)
    }

    /// A set's row clicked.
    pub fn choose_set(&self, search: &mut InventorySearch, counts: &[SetCount], row: i32) {
        let Some(chosen) = row_of(&self.set_rows(counts), row) else {
            return;
        };
        let set = chosen.map(|count| count.id);
        search.edit_filter(|filter| filter.set = set);
    }

    // --- Characters ---

    /// The checkbox column: the character is searched.
    #[must_use]
    pub fn character_checks(&self, names: &[String], filter: &InventoryFilter) -> Vec<bool> {
        names
            .iter()
            .map(|name| {
                filter
                    .characters
                    .as_ref()
                    .is_none_or(|chosen| chosen.contains(name))
            })
            .collect()
    }

    /// A character's checkbox clicked; every character checked is no
    /// filter.
    pub fn toggle_character(&self, search: &mut InventorySearch, names: &[String], row: i32) {
        let Some(name) = usize::try_from(row).ok().and_then(|row| names.get(row)) else {
            return;
        };
        search.edit_filter(|filter| {
            let mut chosen = filter
                .characters
                .take()
                .unwrap_or_else(|| names.iter().cloned().collect());
            if !chosen.remove(name) {
                chosen.insert(name.clone());
            }
            let every = names.iter().all(|name| chosen.contains(name));
            filter.characters = (!every).then_some(chosen);
        });
    }

    /// All: every character.
    pub fn all_characters(&self, search: &mut InventorySearch) {
        search.edit_filter(|filter| filter.characters = None);
    }

    /// None: no character.
    pub fn no_characters(&self, search: &mut InventorySearch) {
        search.edit_filter(|filter| filter.characters = Some(Default::default()));
    }

    // --- Reset, Clear ---

    /// One tab's Reset: its filters back to none (and its own state: the
    /// Cantrips tab's tiers and switch, the number fields' text); the
    /// other tabs' stay.
    pub fn reset_tab(&mut self, search: &mut InventorySearch, tab: FilterTab) {
        let fresh = InventoryFilter::default();
        match tab {
            FilterTab::Item => search.edit_filter(|filter| {
                filter.equipped = fresh.equipped;
                filter.bonded = fresh.bonded;
                filter.attuned = fresh.attuned;
                filter.rare = fresh.rare;
                filter.item_type = fresh.item_type;
                filter.weapon_kinds = fresh.weapon_kinds;
                filter.slots = fresh.slots;
            }),
            FilterTab::Cantrips => {
                self.checked_only = false;
                self.tiers.fill(CantripTier::Legendary);
                search.edit_filter(|filter| {
                    filter.cantrips = fresh.cantrips;
                    filter.spell_contains = fresh.spell_contains;
                });
            }
            FilterTab::Ratings => {
                self.rating_texts = Default::default();
                search.edit_filter(|filter| {
                    filter.ratings = fresh.ratings;
                    filter.allowed_cd = fresh.allowed_cd;
                });
            }
            FilterTab::Set => search.edit_filter(|filter| filter.set = fresh.set),
            FilterTab::Reqs => {
                self.req_texts = Default::default();
                search.edit_filter(|filter| {
                    for req in Req::ALL {
                        *req.slot(filter) = None;
                    }
                });
            }
            FilterTab::Characters => {
                search.edit_filter(|filter| filter.characters = fresh.characters);
            }
        }
    }

    /// Every filter cleared, and the number fields with them.
    pub fn clear(&mut self, search: &mut InventorySearch) {
        search.clear_filter();
        self.checked_only = false;
        self.rating_texts = Default::default();
        self.req_texts = Default::default();
    }
}

/// `value`'s place in `all` (0 when absent).
fn position<T: PartialEq>(all: &[T], value: T) -> i32 {
    all.iter()
        .position(|item| *item == value)
        .and_then(|at| i32::try_from(at).ok())
        .unwrap_or(0)
}

//! The combat tracker's two lists as rows of strings (Mag-Tools'
//! `CombatTrackerGUI` and `CombatTrackerGUIInfo`): the monster list (a
//! header, the "All" row, a row a name the character fought) and the
//! detail list of the selected row (damage received by attack type and
//! element; attacks, evades, resists, surges, averages and crits given).
//!
//! As Mag-Tools drew them: a monster row's cell is written only when its
//! value is not zero (it is emptied only by a clear), and the detail
//! list's averages and crits are written only when their divisor is not
//! zero (they keep their text until the next click or clear). The fixes:
//! F7, the rows sort by ordinal name and the selection follows its name;
//! F8, each Typeless cell shows its own total; F9, a percentage with a
//! zero divisor prints `(0%)` / `(0.0%)`.
//!
//! The lists follow the tracker's updates ([`CombatGui::follow`], cheap:
//! the names added as Mag-Tools added them), and their numbers are
//! worked out when asked ([`CombatGui::refresh`], while the page is
//! shown). Between two refreshes the tracker only adds to its counts, so
//! the last value written is the one Mag-Tools would show; the plugin
//! refreshes before a file replaces the counts.

use std::collections::HashSet;

use mosstank_dotnet::collections::{FnvBuildHasher, LookupMap};

use mosstank_dotnet::text::ordinal;

use super::CombatTracker;
use super::TrackerUpdate;
use super::info::{AttackType, DamageElement};
use crate::fmt::{f0, f1, grouped, number_formatter, round};

/// The monster list's column widths, pixels.
pub const MONSTER_WIDTHS: [u32; 5] = [5, 111, 37, 55, 77];
/// Which of the monster list's columns are right-aligned.
pub const MONSTER_ALIGN_RIGHT: [bool; 5] = [false, false, true, true, true];
/// The detail list's column widths, pixels.
pub const DETAIL_WIDTHS: [u32; 5] = [40, 52, 52, 45, 96];
/// Which of the detail list's columns are right-aligned.
pub const DETAIL_ALIGN_RIGHT: [bool; 5] = [false, true, true, false, true];
/// The detail list's rows.
pub const DETAIL_ROWS: usize = 11;
/// A file's rows worked out a refresh of the page shown (the 3,000 of a
/// long life's stats over three ticks).
pub const ROWS_PER_REFRESH: usize = 1000;

/// The selection marker.
const MARKER: &str = "*";
/// The "All" row.
const ALL_ROW: usize = 1;
/// The first name's row.
const FIRST_NAME_ROW: usize = 2;

/// The Combat page's sub-pages (Mag-Tools' three tabs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CombatPage {
    /// The current session's stats, the first.
    #[default]
    Current,
    /// The stats kept across sessions.
    Persistent,
    /// The buttons and options.
    Options,
}

/// Which tracker's lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsPage {
    /// The current session's.
    Current,
    /// The persistent ones.
    Persistent,
}

/// The detail list's labels: (row, column, text).
const DETAIL_LABELS: [(usize, usize, &str); 20] = [
    (0, 1, "Mel/Msl"),
    (0, 2, "Magic"),
    (0, 3, "Attacks"),
    (1, 0, "Typeless"),
    (1, 3, "Evades"),
    (2, 0, "Slash"),
    (2, 3, "Resists"),
    (3, 0, "Pierce"),
    (3, 3, "A.Surges"),
    (4, 0, "Bludge"),
    (4, 3, "C.Surges"),
    (5, 0, "Fire"),
    (6, 0, "Cold"),
    (6, 3, "Av/Mx"),
    (7, 0, "Acid"),
    (7, 3, "Crits"),
    (8, 0, "Electric"),
    (8, 3, "Av/Mx"),
    (10, 0, "Total"),
    (10, 3, "Total"),
];

/// The received block's elements, by row (rows 1-8).
const ELEMENT_ROWS: [DamageElement; 8] = [
    DamageElement::Typeless,
    DamageElement::Slash,
    DamageElement::Pierce,
    DamageElement::Bludge,
    DamageElement::Fire,
    DamageElement::Cold,
    DamageElement::Acid,
    DamageElement::Electric,
];

/// The right block's value cells: (row, column 4).
const ATTACKS: usize = 0;
const EVADES: usize = 1;
const RESISTS: usize = 2;
const AETHERIA_SURGES: usize = 3;
const CLOAK_SURGES: usize = 4;
const AVERAGE_MAX: usize = 6;
const CRITS: usize = 7;
const CRITS_AVERAGE_MAX: usize = 8;
const TOTAL: usize = 10;

/// One tracker's two lists.
#[derive(Debug, Clone)]
pub struct CombatGui {
    /// The monster list by column: the header, "All", then the names.
    columns: [Vec<String>; 5],
    /// The names' rows.
    row_of: LookupMap<String, usize>,
    selected: usize,
    detail: [[String; 5]; DETAIL_ROWS],
    /// The names whose rows wait for their numbers, in the order changed.
    pending: Vec<String>,
    pending_set: HashSet<String, FnvBuildHasher>,
    /// Something changed since the numbers were worked out: the "All" row
    /// and the detail list wait too.
    changed: bool,
    /// Every name row from this one on waits for its numbers (a file read
    /// in).
    all_rows: Option<usize>,
    revision: u64,
}

impl Default for CombatGui {
    fn default() -> Self {
        let header = ["", "", "KB's", "Dmg Rcvd", "Dmg Givn"];
        let all = [MARKER, "All", "", "", ""];
        let columns =
            std::array::from_fn(|column| vec![header[column].to_owned(), all[column].to_owned()]);
        let mut detail: [[String; 5]; DETAIL_ROWS] = Default::default();
        for (row, column, text) in DETAIL_LABELS {
            detail[row][column] = text.to_owned();
        }
        Self {
            columns,
            row_of: LookupMap::default(),
            selected: ALL_ROW,
            detail,
            pending: Vec::new(),
            pending_set: HashSet::default(),
            changed: false,
            all_rows: None,
            revision: 0,
        }
    }
}

/// `value == 0 ? "" : value.ToString("#,##0")`.
fn amount(value: i64) -> String {
    if value == 0 {
        String::new()
    } else {
        grouped(value)
    }
}

/// `(numerator / (float)divisor) * 100`, F9: 0 for a zero divisor
/// (Mag-Tools: infinity).
#[allow(clippy::cast_precision_loss)]
fn percent(numerator: i32, divisor: i32) -> f32 {
    if divisor == 0 {
        return 0.0;
    }
    (numerator as f32 / divisor as f32) * 100.0
}

/// `SetAttacks` / `SetEvades` / `SetResists`: `10 (70%)`.
fn count_whole_percent(total: i32, percent: f32) -> String {
    if total == 0 {
        return String::new();
    }
    format!(
        "{} ({}%)",
        number_formatter(i64::from(total), 999_999),
        f0(round(f64::from(percent), 0))
    )
}

/// `SetAetheriaSurges` / `SetCrits`: `2 (28.6%)`.
fn count_tenth_percent(total: i32, percent: f32) -> String {
    if total == 0 {
        return String::new();
    }
    format!(
        "{} ({}%)",
        number_formatter(i64::from(total), 999_999),
        f1(round(f64::from(percent), 1))
    )
}

/// `SetCloakSurges`: grouped, not shortened.
fn cloak_count(total: i32, percent: f32) -> String {
    if total == 0 {
        return String::new();
    }
    format!(
        "{} ({}%)",
        grouped(i64::from(total)),
        f1(round(f64::from(percent), 1))
    )
}

/// `SetAvgMax` / `SetCritsAvgMax`: `23 / 40`.
fn average_max(average: i32, max: i32) -> String {
    if average == 0 {
        return String::new();
    }
    format!(
        "{} / {}",
        grouped(i64::from(average)),
        grouped(i64::from(max))
    )
}

/// `(int)(total / count)`: the long division, truncated.
#[allow(clippy::cast_possible_truncation)]
fn int_average(total: i64, count: i32) -> i32 {
    (total / i64::from(count)) as i32
}

impl CombatGui {
    /// The monster list's rows: the header, "All", then the names.
    #[must_use]
    pub fn rows(&self) -> Vec<[String; 5]> {
        (0..self.columns[0].len())
            .map(|row| std::array::from_fn(|column| self.columns[column][row].clone()))
            .collect()
    }

    /// One column of the monster list, a cell a row.
    #[must_use]
    pub fn column(&self, column: usize) -> &[String] {
        &self.columns[column]
    }

    /// The detail list's rows.
    #[must_use]
    pub fn detail_rows(&self) -> Vec<[String; 5]> {
        self.detail.to_vec()
    }

    /// One column of the detail list, a cell a row.
    #[must_use]
    pub fn detail_column(&self, column: usize) -> Vec<String> {
        self.detail.iter().map(|row| row[column].clone()).collect()
    }

    /// The row selected (1: "All").
    #[must_use]
    pub fn selected_row(&self) -> usize {
        self.selected
    }

    /// The name of the row selected ("All" for the "All" row).
    #[must_use]
    pub fn selected_name(&self) -> &str {
        &self.columns[1][self.selected]
    }

    /// The tracker changed since the numbers were worked out.
    #[must_use]
    pub fn needs_refresh(&self) -> bool {
        self.changed || self.all_rows.is_some() || !self.pending.is_empty()
    }

    /// Counts up whenever a cell may have changed.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Follows the tracker's updates and works the numbers out.
    pub fn update(&mut self, tracker: &mut CombatTracker, me: &str, sort: bool) {
        self.follow(tracker, me, sort);
        self.refresh(tracker, me);
    }

    /// Follows the tracker's updates: the names added as Mag-Tools added
    /// them (and sorted after when `sort`, F7), a clear done; the numbers
    /// wait for [`Self::refresh`].
    pub fn follow(&mut self, tracker: &mut CombatTracker, me: &str, sort: bool) {
        let updates = tracker.take_updates();
        if updates.is_empty() {
            return;
        }
        // Sorting after the batch is sorting after each new row: the names
        // are unique and the selection follows its name.
        let mut sort_now = false;
        for update in updates {
            match update {
                TrackerUpdate::Combat { source, target }
                | TrackerUpdate::Aetheria { source, target }
                | TrackerUpdate::Cloak { source, target } => {
                    self.changed = true;
                    sort_now |= self.add_names(&source, &target, me) && sort;
                    self.mark(&source, me);
                    self.mark(&target, me);
                }
                TrackerUpdate::Cleared => {
                    self.info_cleared();
                    sort_now = false;
                }
                // `StatsLoaded`: every info's names, not sorted one by one,
                // then once.
                TrackerUpdate::Loaded => {
                    self.changed = true;
                    // Every row is worked out again (a row the file does not
                    // name has zeros, which write nothing).
                    self.all_rows = Some(FIRST_NAME_ROW);
                    for info in tracker.combat_infos() {
                        self.add_names(&info.source, &info.target, me);
                    }
                    sort_now |= sort;
                }
                // More changed than was kept: every name the tracker has.
                TrackerUpdate::Overflowed => {
                    self.changed = true;
                    self.all_rows = Some(FIRST_NAME_ROW);
                    let pairs = tracker
                        .combat_infos()
                        .iter()
                        .map(|info| (&info.source, &info.target))
                        .chain(
                            tracker
                                .aetheria_infos()
                                .iter()
                                .chain(tracker.cloak_infos())
                                .map(|info| (&info.source, &info.target)),
                        );
                    for (source, target) in pairs {
                        sort_now |= self.add_names(source, target, me) && sort;
                    }
                }
            }
        }
        if sort_now {
            self.sort();
        }
        self.revision += 1;
    }

    /// Works out the numbers that changed: the "All" row, the changed
    /// names' rows, the detail list.
    pub fn refresh(&mut self, tracker: &CombatTracker, me: &str) {
        self.refresh_some(tracker, me, usize::MAX);
    }

    /// [`Self::refresh`] with at most `rows` of a file's rows worked out
    /// (the rest, and the "All" row and the detail list, at a later call;
    /// the counts only grow meanwhile, so the numbers are the same).
    pub fn refresh_some(&mut self, tracker: &CombatTracker, me: &str, rows: usize) {
        if !self.needs_refresh() {
            return;
        }
        if let Some(first) = self.all_rows.take() {
            let end = self.columns[1].len().min(first.saturating_add(rows));
            for row in first..end {
                let values = row_values(tracker, &self.columns[1][row], me);
                self.write_row(row, values);
            }
            if end < self.columns[1].len() {
                self.all_rows = Some(end);
                self.revision += 1;
                return;
            }
        }
        let all = row_values(tracker, me, me);
        self.write_row(ALL_ROW, all);
        for name in std::mem::take(&mut self.pending) {
            if let Some(&row) = self.row_of.get(&name) {
                let values = row_values(tracker, &name, me);
                self.write_row(row, values);
            }
        }
        self.pending_set.clear();
        self.update_detail(tracker, me);
        self.changed = false;
        self.revision += 1;
    }

    /// A click on the monster list's `row` (`monsterList_Click`): the
    /// header selects "All"; the detail list is cleared and worked out for
    /// the row. A row past the list is no row.
    pub fn click(&mut self, tracker: &mut CombatTracker, me: &str, sort: bool, row: usize) {
        self.update(tracker, me, sort);
        if row >= self.columns[0].len() || row == self.selected {
            return;
        }
        let row = row.max(ALL_ROW);
        self.columns[0][self.selected].clear();
        self.selected = row;
        MARKER.clone_into(&mut self.columns[0][row]);
        self.clear_detail();
        self.update_detail(tracker, me);
        self.revision += 1;
    }

    /// `AddNamesToList`: a row for the one of `source` and `target` that is
    /// not us, unless either has one; true when a row was added.
    fn add_names(&mut self, source: &str, target: &str, me: &str) -> bool {
        if source.is_empty() && target.is_empty() {
            return false;
        }
        // We don't add ourself to the list.
        if (source == me && (target == me || target.is_empty()))
            || (source.is_empty() && target == me)
        {
            return false;
        }
        // No row has our name (none is ever added for it): not looked up.
        let has_row = |name: &str| !name.is_empty() && name != me && self.row_of.contains_key(name);
        if has_row(source) || has_row(target) {
            return false;
        }
        let name = if source.is_empty() || source == me {
            target
        } else {
            source
        };
        let row = self.columns[0].len();
        for column in &mut self.columns {
            column.push(String::new());
        }
        name.clone_into(&mut self.columns[1][row]);
        self.row_of.insert(name.to_owned(), row);
        true
    }

    /// `name`'s row waits for its numbers (`updateMonsterListRow` is
    /// called for a name that is not empty and not us).
    fn mark(&mut self, name: &str, me: &str) {
        if !name.is_empty() && name != me && !self.pending_set.contains(name) {
            self.pending_set.insert(name.to_owned());
            self.pending.push(name.to_owned());
        }
    }

    /// F7: the name rows in ordinal order, each row's cells with it; the
    /// selection stays on its name.
    fn sort(&mut self) {
        let rows = self.columns[0].len();
        let mut order: Vec<usize> = (FIRST_NAME_ROW..rows).collect();
        let names = &self.columns[1];
        order.sort_by(|&a, &b| ordinal::compare(&names[a], &names[b]));
        if order.iter().copied().eq(FIRST_NAME_ROW..rows) {
            return;
        }
        for column in &mut self.columns {
            let mut old: Vec<String> = column.drain(FIRST_NAME_ROW..).collect();
            column.extend(
                order
                    .iter()
                    .map(|&row| std::mem::take(&mut old[row - FIRST_NAME_ROW])),
            );
        }
        if self.selected >= FIRST_NAME_ROW {
            let was = self.selected;
            self.selected = order
                .iter()
                .position(|&row| row == was)
                .map_or(ALL_ROW, |at| at + FIRST_NAME_ROW);
        }
        self.row_of.clear();
        for (row, name) in self.columns[1].iter().enumerate().skip(FIRST_NAME_ROW) {
            self.row_of.insert(name.clone(), row);
        }
    }

    /// `InfoCleared`: the detail list cleared, the name rows removed, "All"
    /// selected and its numbers emptied.
    fn info_cleared(&mut self) {
        self.clear_detail();
        for column in &mut self.columns {
            column.truncate(FIRST_NAME_ROW);
        }
        self.row_of.clear();
        self.selected = ALL_ROW;
        MARKER.clone_into(&mut self.columns[0][ALL_ROW]);
        for column in &mut self.columns[2..] {
            column[ALL_ROW].clear();
        }
        self.pending.clear();
        self.pending_set.clear();
        self.changed = false;
        self.all_rows = None;
    }

    /// Writes a row's numbers that are not zero.
    fn write_row(&mut self, row: usize, (kills, received, given): (i32, i32, i64)) {
        if kills != 0 {
            self.columns[2][row] = number_formatter(i64::from(kills), 99_999);
        }
        if received != 0 {
            self.columns[3][row] = number_formatter(i64::from(received), 9_999_999);
        }
        if given != 0 {
            self.columns[4][row] = number_formatter(given, 99_999_999);
        }
    }

    /// `CombatTrackerGUIInfo.Clear`: every value cell empty.
    fn clear_detail(&mut self) {
        // Row 0's first two cells are the headers.
        for (row, cells) in self.detail.iter_mut().enumerate() {
            if row != 0 {
                cells[1].clear();
                cells[2].clear();
            }
            cells[4].clear();
        }
    }

    /// `updateCombatTrackerGUIInfo`: the detail list of the row selected.
    #[allow(clippy::too_many_lines)]
    fn update_detail(&mut self, tracker: &CombatTracker, me: &str) {
        let name = if self.selected == ALL_ROW {
            me.to_owned()
        } else {
            self.columns[1][self.selected].clone()
        };

        // The damage received, by attack type and element.
        let mut melee = [0_i32; ELEMENT_ROWS.len()];
        let mut magic = [0_i32; ELEMENT_ROWS.len()];
        let (mut total_melee, mut total_magic) = (0_i32, 0_i32);
        for info in tracker.combat_infos_of(&name) {
            if info.target != me {
                continue;
            }
            for (kind, by_type) in info.by_attack_type.iter() {
                let (cells, total) = match kind {
                    AttackType::MeleeMissle => (&mut melee, &mut total_melee),
                    AttackType::Magic => (&mut magic, &mut total_magic),
                    AttackType::Unknown => continue,
                };
                for (element, bucket) in by_type.by_element.iter() {
                    let damage = bucket.damage();
                    if let Some(at) = ELEMENT_ROWS.iter().position(|e| *e == element) {
                        cells[at] = cells[at].wrapping_add(damage);
                    }
                    *total = total.wrapping_add(damage);
                }
            }
        }
        // F8: each Typeless cell its own total.
        for at in 0..ELEMENT_ROWS.len() {
            self.detail[at + 1][1] = amount(i64::from(melee[at]));
            self.detail[at + 1][2] = amount(i64::from(magic[at]));
        }
        self.detail[TOTAL][1] = amount(i64::from(total_melee));
        self.detail[TOTAL][2] = amount(i64::from(total_magic));

        // Attacks, evades, resists.
        let (mut attacks, mut failed) = (0_i32, 0_i32);
        let (mut melee_defends, mut evades) = (0_i32, 0_i32);
        let (mut magic_defends, mut resists) = (0_i32, 0_i32);
        // Normal and critical hits given.
        let (mut normal_damage, mut max_normal) = (0_i64, 0_i32);
        let (mut crits, mut crit_damage, mut max_crit) = (0_i32, 0_i64, 0_i32);
        let (mut kills, mut given) = (0_i32, 0_i64);
        for info in tracker.combat_infos_of(&name) {
            if info.source == me {
                let totals = info.totals();
                attacks = attacks.wrapping_add(totals.total_attacks);
                failed = failed.wrapping_add(totals.failed_attacks);
                normal_damage += i64::from(totals.total_normal_damage);
                max_normal = max_normal.max(totals.max_normal_damage);
                crits = crits.wrapping_add(totals.crits);
                crit_damage += i64::from(totals.total_crit_damage);
                max_crit = max_crit.max(totals.max_crit_damage);
                kills = kills.wrapping_add(info.killing_blows);
                given += i64::from(totals.damage());
            } else if info.target == me {
                for (kind, by_type) in info.by_attack_type.iter() {
                    let totals = by_type.totals();
                    match kind {
                        AttackType::MeleeMissle => {
                            melee_defends = melee_defends.wrapping_add(totals.total_attacks);
                            evades = evades.wrapping_add(totals.failed_attacks);
                        }
                        AttackType::Magic => {
                            magic_defends = magic_defends.wrapping_add(totals.total_attacks);
                            resists = resists.wrapping_add(totals.failed_attacks);
                        }
                        AttackType::Unknown => {}
                    }
                }
            }
        }
        self.detail[ATTACKS][4] =
            count_whole_percent(attacks, percent(attacks.wrapping_sub(failed), attacks));
        self.detail[EVADES][4] = count_whole_percent(melee_defends, percent(evades, melee_defends));
        self.detail[RESISTS][4] =
            count_whole_percent(magic_defends, percent(resists, magic_defends));

        // Aetheria surges against every attack made (hit or miss); cloak
        // surges against every hit received.
        let aetheria = tracker
            .aetheria_infos()
            .iter()
            .filter(|info| info.source == name || info.target == name)
            .filter(|info| info.source == me)
            .fold(0_i32, |sum, info| sum.wrapping_add(info.total_surges));
        self.detail[AETHERIA_SURGES][4] = count_tenth_percent(aetheria, percent(aetheria, attacks));
        let cloak = tracker
            .cloak_infos()
            .iter()
            .filter(|info| info.source == name || info.target == name)
            .filter(|info| info.source == me)
            .fold(0_i32, |sum, info| sum.wrapping_add(info.total_surges));
        let hits_received = melee_defends
            .wrapping_sub(evades)
            .wrapping_add(magic_defends.wrapping_sub(resists));
        self.detail[CLOAK_SURGES][4] = cloak_count(cloak, percent(cloak, hits_received));

        // Written only when their divisor is not zero: else the text stays.
        let normal_hits = attacks
            .wrapping_sub(failed)
            .wrapping_sub(crits)
            .wrapping_sub(kills);
        if normal_hits != 0 {
            self.detail[AVERAGE_MAX][4] =
                average_max(int_average(normal_damage, normal_hits), max_normal);
        }
        let hits = attacks.wrapping_sub(failed).wrapping_sub(kills);
        if hits != 0 {
            self.detail[CRITS][4] = count_tenth_percent(crits, percent(crits, hits));
        }
        if crits != 0 {
            self.detail[CRITS_AVERAGE_MAX][4] =
                average_max(int_average(crit_damage, crits), max_crit);
        }
        self.detail[TOTAL][4] = amount(given);
    }
}

/// A row's kills and damage given (infos `me` is the source of) and the
/// damage received (`else`: the infos `me` is the target of).
fn row_values(tracker: &CombatTracker, name: &str, me: &str) -> (i32, i32, i64) {
    let (mut kills, mut received, mut given) = (0_i32, 0_i32, 0_i64);
    for info in tracker.combat_infos_of(name) {
        if info.source == me {
            kills = kills.wrapping_add(info.killing_blows);
            given += i64::from(info.damage());
        } else if info.target == me {
            received = received.wrapping_add(info.damage());
        }
    }
    (kills, received, given)
}

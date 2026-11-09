//! The Inv. Items page: the consumables the character carries, how fast
//! they go and when they run out, and above them the profit block (the
//! value of comps, salvage, notes and money gained, in MMD an hour), from
//! Mag-Tools' `Trackers/Inventory/InventoryTracker.cs`,
//! `TrackedInventory.cs`, `Trackers/ProfitLoss/ProfitLossTracker.cs`,
//! `Trackers/SnapShotGroup.cs`, `ValueSnapShotGroup.cs` and
//! `Views/InventoryTrackerGUI.cs` (43ee2ec).
//!
//! - **Tracked:** tapers and scarabs (spell components), mana stones and
//!   healing kits by their own names; trade notes, salvage and (Lesser)
//!   Corrupted Essence each as one row (`Trade Note`, `Salvage`,
//!   `Corrupted Essence`, counting every owned item of the class whose name
//!   holds it). A row is added the first time a tracked thing is seen,
//!   owned or not (a corpse's or the ground's: count 0), with that object's
//!   icon and unit value (its value over its stack; for an object not owned
//!   the open container's record says, else 0). Rows are never removed.
//! - **Order:** a new row goes before the first row of a later class
//!   (components, mana stones, healing kits, trade notes, salvage, the
//!   rest) or of its class with a lower unit value; it never moves after.
//! - **Snapshots:** the login and every change of a tracked object (any
//!   object, any change, the count changed or not) add a snapshot of the
//!   count (the profit rows: of the summed value). A rate is the change
//!   since the snapshot closest to 5 min or 1 h ago, per hour; the hours
//!   left are the count over the hour's rate of use.
//! - **Refresh:** a row is redrawn when its snapshot is added, and every
//!   500 ms a pass redraws the rows in the order they were tracked, but
//!   stops (Mag-Tools' `return`) at the first one drawn less than a second
//!   ago: about once a second, unless the first rows keep changing.
//! - Nothing is saved: the history starts at each login.
//!
//! Changes: F13: a snapshot at the same moment as the last replaces it
//! (Mag-Tools added the two up), and snapshots more than two hours old are
//! dropped, the newest always kept (Mag-Tools kept every one; none older
//! can be the closest to an hour ago). The client's clock is the frame's
//! monotonic one, so a frame's changes make one snapshot each with the
//! count at the end of the frame. The count is taken on the frame after the
//! change (Mag-Tools counted inside the change), so a burst of changes
//! costs one look at the inventory a frame. The login waits for the owned
//! list to be complete; changes before the login are not followed (as
//! Mag-Tools subscribed at the login).

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::{ObjectChange, ObjectChangeKind, ObjectClass};

use crate::fmt::n1;
use crate::host::MagHost;

/// One MMD trade note's value: the profit rows' unit.
pub const MMD: f64 = 250_000.0;
/// Snapshots older than this are dropped (F13), ms.
const KEEP_MS: f64 = 2.0 * 3_600_000.0;
/// The refresh pass's period, ms.
const PASS_MS: f64 = 500.0;
/// A row drawn more recently than this stops the refresh pass, ms.
const RECENT_MS: f64 = 1_000.0;
/// The short window.
const FIVE_MINUTES: Duration = Duration::from_secs(300);
/// The long window, and the rates' unit.
const ONE_HOUR: Duration = Duration::from_secs(3600);
/// .NET `TimeSpan` ticks a millisecond.
const TICKS_PER_MS: f64 = 10_000.0;
/// `TimeSpan.TotalMinutes`' factor.
const MINUTES_PER_TICK: f64 = 1.0 / 600_000_000.0;
/// `TimeSpan.TotalSeconds`' factor.
const SECONDS_PER_TICK: f64 = 1.0 / 10_000_000.0;
/// `TimeSpan.TotalHours`' factor.
const HOURS_PER_TICK: f64 = 1.0 / 36_000_000_000.0;

/// The profit rows, in the order Mag-Tools tracked them.
const PROFIT_NAMES: [&str; 4] = ["Peas", "Comps", "Salvage", "Net Profit"];
/// The profit rows' bits.
const PEAS: u8 = 1;
const COMPS: u8 = 2;
const SALVAGE: u8 = 4;
const NET: u8 = 8;
/// The fixed rows above the consumables.
const FIXED_ROWS: usize = 7;

#[allow(clippy::cast_precision_loss)]
fn duration_ms(duration: Duration) -> f64 {
    duration.as_millis() as f64
}

/// `TimeSpan.TotalMinutes` of `ms`.
fn minutes(ms: f64) -> f64 {
    ms * TICKS_PER_MS * MINUTES_PER_TICK
}

/// One count (or value) and when it was seen.
#[derive(Debug, Clone, Copy)]
struct Snapshot {
    at_ms: f64,
    value: i32,
}

/// The snapshots of one row (`ValueSnapShotGroup`), oldest first.
#[derive(Debug, Default, Clone)]
pub struct SnapshotGroup {
    snapshots: VecDeque<Snapshot>,
}

impl SnapshotGroup {
    /// Adds `value` seen at `at_ms` (the client's monotonic clock): one at
    /// the last one's moment replaces it (F13); then the snapshots more
    /// than two hours older than it are dropped, the newest kept.
    pub fn add(&mut self, at_ms: f64, value: i32) {
        if let Some(last) = self.snapshots.back_mut()
            && at_ms <= last.at_ms
        {
            // The clock never goes back: the same moment.
            last.value = value;
            return;
        }
        self.snapshots.push_back(Snapshot { at_ms, value });
        let cutoff = at_ms - KEEP_MS;
        let old = self
            .snapshots
            .partition_point(|snapshot| snapshot.at_ms < cutoff)
            .min(self.snapshots.len() - 1);
        self.snapshots.drain(..old);
    }

    /// The newest value (`LastKnownValue`), 0 with none.
    #[must_use]
    pub fn last_value(&self) -> i32 {
        self.snapshots.back().map_or(0, |snapshot| snapshot.value)
    }

    /// How many snapshots are kept.
    #[must_use]
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    /// No snapshot yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    /// The snapshot closest to `at_ms`, the newer on a tie
    /// (`GetSnapShotClosestToTime`).
    fn closest(&self, at_ms: f64) -> Option<Snapshot> {
        let after = self
            .snapshots
            .partition_point(|snapshot| snapshot.at_ms < at_ms);
        if after == 0 {
            return self.snapshots.front().copied();
        }
        let older = self.snapshots[after - 1];
        let Some(&newer) = self.snapshots.get(after) else {
            return Some(older);
        };
        Some(if at_ms - older.at_ms < newer.at_ms - at_ms {
            older
        } else {
            newer
        })
    }

    /// The change since the snapshot closest to `history` ago, per `usage`
    /// (`GetValueDifference`): negative when it goes down; 0 with one
    /// snapshot; NaN when that snapshot is the newest, taken at `now_ms`.
    #[must_use]
    pub fn value_difference(&self, history: Duration, usage: Duration, now_ms: f64) -> f64 {
        if self.snapshots.len() <= 1 {
            return 0.0;
        }
        let Some(closest) = self.closest(now_ms - duration_ms(history)) else {
            return 0.0;
        };
        f64::from(self.last_value().wrapping_sub(closest.value))
            * (minutes(duration_ms(usage)) / minutes(now_ms - closest.at_ms))
    }

    /// How long the newest value lasts at the rate since the snapshot
    /// closest to `period` ago (`GetTimeToDepletion`), to the millisecond;
    /// `None` with one snapshot, at 0, or when it does not go down
    /// (Mag-Tools' `TimeSpan.Zero` and `TimeSpan.MaxValue`).
    #[must_use]
    pub fn time_to_depletion(&self, period: Duration, now_ms: f64) -> Option<Duration> {
        let last = self.last_value();
        if self.snapshots.len() <= 1 || last == 0 {
            return None;
        }
        let closest = self.closest(now_ms - duration_ms(period))?;
        if last >= closest.value {
            return None;
        }
        let elapsed_seconds = (now_ms - closest.at_ms) * TICKS_PER_MS * SECONDS_PER_TICK;
        let seconds =
            f64::from(last) / (f64::from(closest.value.wrapping_sub(last)) / elapsed_seconds);
        // `TimeSpan.FromSeconds`: rounded to the millisecond.
        let ms = (seconds * 1000.0 + 0.5).trunc();
        // Also NaN (a value that was not a number).
        if ms.is_nan() || ms < 1.0 {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(Duration::from_millis(ms.min(9.2e14) as u64))
    }
}

/// `TimeSpan.TotalHours`.
#[allow(clippy::cast_precision_loss)]
fn total_hours(duration: Duration) -> f64 {
    duration.as_millis() as f64 * TICKS_PER_MS * HOURS_PER_TICK
}

/// The row a thing of `class` named `name` counts in, and whether that row
/// counts every name holding it (a group) or only its own name
/// (`InventoryTracker.ProcessObject`).
#[must_use]
pub fn tracked_name(class: ObjectClass, name: &str) -> Option<(&str, bool)> {
    match class {
        ObjectClass::SpellComponent if name.contains("Taper") || name.contains("Scarab") => {
            Some((name, false))
        }
        ObjectClass::ManaStone | ObjectClass::HealingKit => Some((name, false)),
        ObjectClass::TradeNote => Some(("Trade Note", true)),
        ObjectClass::Salvage => Some(("Salvage", true)),
        ObjectClass::Misc if name == "Corrupted Essence" || name == "Lesser Corrupted Essence" => {
            Some(("Corrupted Essence", true))
        }
        _ => None,
    }
}

/// The profit rows a thing of `class` named `name` adds its value to
/// (`ProfitLossTracker.ProcessObject` and its sums).
fn profit_rows(class: ObjectClass, name: &str) -> u8 {
    match class {
        ObjectClass::SpellComponent if name.contains("Pea") => PEAS | NET,
        ObjectClass::SpellComponent => COMPS | NET,
        ObjectClass::Salvage => SALVAGE | NET,
        ObjectClass::TradeNote | ObjectClass::Money => NET,
        _ => 0,
    }
}

/// The order of the classes' rows (`GetZIndex`).
fn class_order(class: ObjectClass) -> u32 {
    match class {
        ObjectClass::SpellComponent => 0,
        ObjectClass::ManaStone => 1,
        ObjectClass::HealingKit => 2,
        ObjectClass::TradeNote => 3,
        ObjectClass::Salvage => 4,
        _ => u32::MAX,
    }
}

/// An item's unit value: its value over its stack.
fn unit_value(item: &InventoryItem) -> i32 {
    if item.stack_size == 0 {
        item.value
    } else {
        item.value.wrapping_div(item.stack_size)
    }
}

/// `n1(v)`, empty for 0.
fn rate_text(value: f64) -> String {
    if value == 0.0 {
        String::new()
    } else {
        n1(value)
    }
}

/// Sets `cell` to `text`; true when it changed.
fn set(cell: &mut String, text: String) -> bool {
    if *cell == text {
        return false;
    }
    *cell = text;
    true
}

/// A profit row.
#[derive(Debug, Default)]
struct ProfitRow {
    group: SnapshotGroup,
    raised_ms: f64,
    dirty: bool,
    five_minutes: String,
    hour: String,
}

impl ProfitRow {
    /// Redraws its cells at `now_ms` (`UpdateProfitLossItem`); true when
    /// one changed.
    fn raise(&mut self, now_ms: f64) -> bool {
        self.raised_ms = now_ms;
        let five = self.group.value_difference(FIVE_MINUTES, ONE_HOUR, now_ms);
        let hour = self.group.value_difference(ONE_HOUR, ONE_HOUR, now_ms);
        let five = if five == 0.0 {
            String::new()
        } else {
            n1(five / MMD)
        };
        let hour = if hour == 0.0 {
            String::new()
        } else {
            n1(hour / MMD)
        };
        set(&mut self.five_minutes, five) | set(&mut self.hour, hour)
    }
}

/// A consumable row.
#[derive(Debug)]
struct Consumable {
    name: String,
    class: ObjectClass,
    /// Counts every name of its class holding `name`.
    group_name: bool,
    icon: u32,
    unit_value: i32,
    group: SnapshotGroup,
    raised_ms: f64,
    dirty: bool,
    count: String,
    five_minutes: String,
    hour: String,
    hours: String,
}

impl Consumable {
    /// An owned item counts in it.
    fn counts(&self, item: &InventoryItem) -> bool {
        item.object_class == self.class
            && if self.group_name {
                item.name.contains(self.name.as_str())
            } else {
                item.name == self.name
            }
    }

    /// Redraws its cells at `now_ms` (`UpdateInventoryItem`); true when one
    /// changed.
    fn raise(&mut self, now_ms: f64) -> bool {
        self.raised_ms = now_ms;
        let last = self.group.last_value();
        let (five, hour, hours) = if last == 0 {
            (String::new(), String::new(), String::new())
        } else {
            let five = self.group.value_difference(FIVE_MINUTES, ONE_HOUR, now_ms);
            let hour = self.group.value_difference(ONE_HOUR, ONE_HOUR, now_ms);
            let hours = self
                .group
                .time_to_depletion(ONE_HOUR, now_ms)
                .map_or_else(String::new, |left| n1(total_hours(left)));
            (rate_text(five), rate_text(hour), hours)
        };
        set(&mut self.count, last.to_string())
            | set(&mut self.five_minutes, five)
            | set(&mut self.hour, hour)
            | set(&mut self.hours, hours)
    }
}

/// What was last seen of an object of a tracked class, for its release.
#[derive(Debug)]
struct Seen {
    class: ObjectClass,
    name: String,
}

/// The page's columns, one entry a row: the seven fixed rows, then the
/// consumables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InvItemsView {
    /// The icons (portal.dat ids; 0 for none).
    pub icons: Vec<u32>,
    /// The names.
    pub names: Vec<String>,
    /// The counts.
    pub counts: Vec<String>,
    /// The rates over five minutes, an hour.
    pub five_minutes: Vec<String>,
    /// The rates over the hour.
    pub hour: Vec<String>,
    /// The hours left.
    pub hours: Vec<String>,
}

impl InvItemsView {
    fn push(&mut self, icon: u32, cells: [&str; 5]) {
        let [name, count, five, hour, hours] = cells;
        self.icons.push(icon);
        self.names.push(name.to_owned());
        self.counts.push(count.to_owned());
        self.five_minutes.push(five.to_owned());
        self.hour.push(hour.to_owned());
        self.hours.push(hours.to_owned());
    }
}

/// The Inv. Items tracker.
#[derive(Debug, Default)]
pub struct InvItemsTracker {
    /// The login was processed: changes are followed.
    active: bool,
    /// The login waits for the owned list to be complete.
    login_waiting: bool,
    /// The profit rows (after the login).
    profit: Vec<ProfitRow>,
    /// The consumables, in the order they were tracked.
    items: Vec<Consumable>,
    /// The consumables' rows, as indices into `items`.
    order: Vec<usize>,
    /// A row waits for its count.
    dirty: bool,
    /// The objects of the tracked and profit classes seen, by id.
    seen: HashMap<u32, Seen>,
    /// The next refresh pass, ms.
    next_pass_ms: f64,
    view: InvItemsView,
    /// The view is the rows' (built since the last change).
    view_current: bool,
}

impl InvItemsTracker {
    /// Forgets everything (the session ended).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The login completed: the history starts afresh with every owned
    /// thing, once the owned list is complete (on a later [`Self::tick`]
    /// when it is not yet).
    pub fn on_login(&mut self, host: &mut dyn MagHost) {
        self.reset();
        if host.owned_inventory_complete() {
            self.start(host);
        } else {
            self.login_waiting = true;
        }
    }

    /// The character logged off: changes are no longer followed (the rows
    /// stay).
    pub fn on_logoff(&mut self) {
        self.active = false;
        self.login_waiting = false;
    }

    /// The login's walk of the owned list (`CharacterFilter_LoginComplete`).
    fn start(&mut self, host: &mut dyn MagHost) {
        self.active = true;
        let now = host.now_ms();
        self.profit = (0..PROFIT_NAMES.len())
            .map(|_| ProfitRow {
                dirty: true,
                ..ProfitRow::default()
            })
            .collect();
        let owned = host.owned_items_shared();
        for item in owned.iter() {
            let tracked = tracked_name(item.object_class, &item.name);
            if tracked.is_none() && profit_rows(item.object_class, &item.name) == 0 {
                continue;
            }
            self.seen.insert(
                item.object_id,
                Seen {
                    class: item.object_class,
                    name: item.name.clone(),
                },
            );
            if let Some((name, group_name)) = tracked {
                self.mark(name, item.object_class, group_name, || {
                    (item.icon_id, unit_value(item))
                });
            }
        }
        self.recount(&owned, now);
        self.next_pass_ms = now + PASS_MS;
    }

    /// Marks the row `name` for a count, adding it (with the icon and unit
    /// value `first`) when it is new.
    fn mark(
        &mut self,
        name: &str,
        class: ObjectClass,
        group_name: bool,
        first: impl FnOnce() -> (u32, i32),
    ) {
        self.dirty = true;
        if let Some(item) = self.items.iter_mut().find(|item| item.name == name) {
            item.dirty = true;
            return;
        }
        let (icon, unit_value) = first();
        let index = self.items.len();
        let order = class_order(class);
        let at = self
            .order
            .iter()
            .position(|&row| {
                let row = &self.items[row];
                (row.class == class && row.unit_value < unit_value)
                    || class_order(row.class) > order
            })
            .unwrap_or(self.order.len());
        self.order.insert(at, index);
        self.items.push(Consumable {
            name: name.to_owned(),
            class,
            group_name,
            icon,
            unit_value,
            group: SnapshotGroup::default(),
            raised_ms: 0.0,
            dirty: true,
            count: String::new(),
            five_minutes: String::new(),
            hour: String::new(),
            hours: String::new(),
        });
        self.view_current = false;
    }

    /// An object changed: one of a tracked class marks its row (added when
    /// new) and the profit rows it counts in for a count on the next
    /// [`Self::tick`]. Only a new row asks the client anything (its unit
    /// value).
    pub fn on_object_changed(&mut self, host: &mut dyn MagHost, change: &ObjectChange) {
        if !self.active {
            return;
        }
        let released = change.kind == ObjectChangeKind::Released;
        if let Some(object) = &change.current {
            let (class, name) = (object.object_class, object.name.as_str());
            if self.touch(host, change.object_id, class, name, object.icon_id) {
                if released {
                    self.seen.remove(&change.object_id);
                } else if self
                    .seen
                    .get(&change.object_id)
                    .is_none_or(|seen| seen.class != class || seen.name != name)
                {
                    self.seen.insert(
                        change.object_id,
                        Seen {
                            class,
                            name: name.to_owned(),
                        },
                    );
                }
            }
        } else if released && let Some(seen) = self.seen.remove(&change.object_id) {
            self.touch(host, change.object_id, seen.class, &seen.name, 0);
        }
    }

    /// Marks what a change of the object touches; false for an object of
    /// no tracked or profit class.
    fn touch(
        &mut self,
        host: &mut dyn MagHost,
        object_id: u32,
        class: ObjectClass,
        name: &str,
        icon: u32,
    ) -> bool {
        let profit = profit_rows(class, name);
        let tracked = tracked_name(class, name);
        if profit == 0 && tracked.is_none() {
            return false;
        }
        if profit != 0 {
            self.dirty = true;
            for (bit, row) in self.profit.iter_mut().enumerate() {
                if profit & (1 << bit) != 0 {
                    row.dirty = true;
                }
            }
        }
        if let Some((tracked, group_name)) = tracked {
            self.mark(tracked, class, group_name, || {
                (icon, first_unit_value(host, object_id))
            });
        }
        true
    }

    /// One frame: the login's walk once the owned list is complete; the
    /// marked rows counted (one look at the owned list); the refresh pass
    /// every 500 ms.
    pub fn tick(&mut self, host: &mut dyn MagHost) {
        if self.login_waiting {
            if !host.owned_inventory_complete() {
                return;
            }
            self.login_waiting = false;
            self.start(host);
        }
        if !self.active {
            return;
        }
        let now = host.now_ms();
        if self.dirty {
            let owned = host.owned_items_shared();
            self.recount(&owned, now);
        }
        if now >= self.next_pass_ms {
            let behind = ((now - self.next_pass_ms) / PASS_MS).floor() + 1.0;
            self.next_pass_ms += behind * PASS_MS;
            self.pass(now);
        }
    }

    /// The marked rows' snapshots, from one walk of the owned list, each
    /// row redrawn.
    fn recount(&mut self, owned: &[InventoryItem], now: f64) {
        self.dirty = false;
        let profit_dirty = self.profit.iter().any(|row| row.dirty);
        let marked: Vec<usize> = (0..self.items.len())
            .filter(|&index| self.items[index].dirty)
            .collect();
        let mut sums = [0i32; PROFIT_NAMES.len()];
        let mut counts = vec![0i32; marked.len()];
        for item in owned {
            if profit_dirty {
                let rows = profit_rows(item.object_class, &item.name);
                for (bit, sum) in sums.iter_mut().enumerate() {
                    if rows & (1 << bit) != 0 {
                        *sum = sum.wrapping_add(item.value);
                    }
                }
            }
            for (slot, &index) in marked.iter().enumerate() {
                if self.items[index].counts(item) {
                    let stack = if item.stack_size == 0 {
                        1
                    } else {
                        item.stack_size
                    };
                    counts[slot] = counts[slot].wrapping_add(stack);
                }
            }
        }
        let mut changed = false;
        for (row, sum) in self.profit.iter_mut().zip(sums) {
            if std::mem::take(&mut row.dirty) {
                row.group.add(now, sum);
                changed |= row.raise(now);
            }
        }
        for (&index, count) in marked.iter().zip(counts) {
            let item = &mut self.items[index];
            item.dirty = false;
            item.group.add(now, count);
            changed |= item.raise(now);
        }
        if changed {
            self.view_current = false;
        }
    }

    /// The refresh pass of each tracker (`timer_Tick`): the rows in the
    /// order tracked, stopping at the first drawn less than a second ago.
    fn pass(&mut self, now: f64) {
        let mut changed = false;
        for row in &mut self.profit {
            if now - row.raised_ms < RECENT_MS {
                break;
            }
            changed |= row.raise(now);
        }
        for item in &mut self.items {
            if now - item.raised_ms < RECENT_MS {
                break;
            }
            changed |= item.raise(now);
        }
        if changed {
            self.view_current = false;
        }
    }

    /// Builds the view, unless nothing changed since it was built.
    pub fn refresh(&mut self) {
        if self.view_current {
            return;
        }
        let rows = FIXED_ROWS + self.order.len();
        let mut view = InvItemsView {
            icons: Vec::with_capacity(rows),
            names: Vec::with_capacity(rows),
            counts: Vec::with_capacity(rows),
            five_minutes: Vec::with_capacity(rows),
            hour: Vec::with_capacity(rows),
            hours: Vec::with_capacity(rows),
        };
        view.push(0, ["", "", "MMD/h ~5m", "MMD/h ~1h", ""]);
        for (index, name) in PROFIT_NAMES.iter().enumerate() {
            let (five, hour) = self.profit.get(index).map_or(("", ""), |row| {
                (row.five_minutes.as_str(), row.hour.as_str())
            });
            view.push(0, [name, "", five, hour, ""]);
        }
        view.push(0, ["", "", "", "", ""]);
        view.push(0, ["", "Count", "Avg/h ~5m", "Avg/h ~1h", "(Hrs)"]);
        for &index in &self.order {
            let item = &self.items[index];
            view.push(
                item.icon,
                [
                    &item.name,
                    &item.count,
                    &item.five_minutes,
                    &item.hour,
                    &item.hours,
                ],
            );
        }
        self.view = view;
        self.view_current = true;
    }

    /// The view last built ([`Self::refresh`]).
    #[must_use]
    pub fn view(&self) -> &InvItemsView {
        &self.view
    }
}

/// A new row's unit value, from the object that added it: the owned
/// item's, else the open container's record, else 0 (the client gives no
/// value for an object it has no record of).
fn first_unit_value(host: &mut dyn MagHost, object_id: u32) -> i32 {
    if let Some(item) = host
        .owned_items_shared()
        .iter()
        .find(|item| item.object_id == object_id)
    {
        return unit_value(item);
    }
    host.loot_contents()
        .iter()
        .find(|item| item.object_id == object_id)
        .map_or(0, unit_value)
}

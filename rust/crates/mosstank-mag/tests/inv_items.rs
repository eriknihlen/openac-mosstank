//! The Inv. Items page: the consumables counted and how fast they go, and
//! the profit block in MMD an hour.
//!
//! Source: Mag-Tools `Mag-Tools/Trackers/Inventory/InventoryTracker.cs`,
//! `TrackedInventory.cs`, `Trackers/ProfitLoss/ProfitLossTracker.cs`,
//! `Trackers/ValueSnapShotGroup.cs`, `Trackers/SnapShotGroup.cs` and
//! `Views/InventoryTrackerGUI.cs` at 43ee2ec.

use std::time::Duration;

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::{ObjectChange, ObjectChangeKind, ObjectClass, WorldObject};
use mosstank_mag::fmt::n1;
use mosstank_mag::inv_items::{InvItemsTracker, SnapshotGroup};
use mosstank_mag::testing::FakeMagHost;
use mosstank_meta::host::MetaHost;

const HOUR: Duration = Duration::from_secs(3600);
const MINUTE_MS: f64 = 60_000.0;
const HOUR_MS: f64 = 3_600_000.0;
/// The clock the snapshot tests read at.
const NOW: f64 = 10.0 * HOUR_MS;

/// A group of snapshots at `NOW` plus each offset (ms).
fn group(snapshots: &[(f64, i32)]) -> SnapshotGroup {
    let mut group = SnapshotGroup::default();
    for &(offset, value) in snapshots {
        group.add(NOW + offset, value);
    }
    group
}

#[test]
fn value_difference_and_depletion() {
    // ValueSnapShotGroup.GetValueDifference / GetTimeToDepletion: 10 an
    // hour ago, 4 now.
    let g = group(&[(-3_600_000.0, 10), (0.0, 4)]);
    assert_eq!(g.value_difference(HOUR, HOUR, NOW), -6.0);
    assert_eq!(n1(g.value_difference(HOUR, HOUR, NOW)), "-6.0");
    assert_eq!(
        n1(g.time_to_depletion(HOUR, NOW).unwrap().as_secs_f64() / 3600.0),
        "0.7"
    );
    // One snapshot: no difference, no depletion.
    let one = group(&[(0.0, 4)]);
    assert_eq!(one.value_difference(HOUR, HOUR, NOW), 0.0);
    assert_eq!(one.time_to_depletion(HOUR, NOW), None);
    // Not going down: never runs out.
    let up = group(&[(-HOUR_MS, 4), (0.0, 10)]);
    assert_eq!(up.time_to_depletion(HOUR, NOW), None);
    assert_eq!(up.value_difference(HOUR, HOUR, NOW), 6.0);
}

#[test]
fn the_closest_snapshot_wins_and_a_tie_goes_to_the_newer() {
    // GetSnapShotClosestToTime: newest to oldest, strictly closer.
    let g = group(&[(-10.0 * MINUTE_MS, 20), (-5.0 * MINUTE_MS, 14), (0.0, 12)]);
    // Five minutes back is the middle one: (12 - 14) * 60 / 5.
    assert_eq!(
        g.value_difference(Duration::from_secs(300), HOUR, NOW),
        -24.0
    );
    // Seven and a half minutes back is as far from both: the newer.
    assert_eq!(
        g.value_difference(Duration::from_secs(450), HOUR, NOW),
        -24.0
    );
}

#[test]
fn the_newest_snapshot_closest_at_its_own_moment_is_not_a_number() {
    // (Last - Last) * (60 / 0): Mag-Tools printed "NaN" there.
    let g = group(&[(-30.0 * MINUTE_MS, 10), (0.0, 4)]);
    let v = g.value_difference(Duration::from_secs(300), HOUR, NOW);
    assert!(v.is_nan());
    assert_eq!(n1(v), "NaN");
    // A moment later it is 0.
    assert_eq!(
        g.value_difference(Duration::from_secs(300), HOUR, NOW + 15.0),
        0.0
    );
}

#[test]
fn the_same_timestamp_replaces() {
    // F13: Mag-Tools added the two values up.
    let mut g = SnapshotGroup::default();
    g.add(NOW, 4);
    g.add(NOW, 5);
    assert_eq!(g.last_value(), 5);
    assert_eq!(g.len(), 1);
}

#[test]
fn snapshots_older_than_two_hours_are_dropped_but_the_newest_stays() {
    // F13: Mag-Tools never pruned.
    let mut g = SnapshotGroup::default();
    g.add(NOW, 10);
    g.add(NOW + HOUR_MS, 8);
    g.add(NOW + 2.0 * HOUR_MS, 7);
    assert_eq!(g.len(), 3, "two hours old is not older than two hours");
    g.add(NOW + 2.0 * HOUR_MS + 1.0, 6);
    assert_eq!(g.len(), 3, "the first is gone");
    g.add(NOW + 5.0 * HOUR_MS, 5);
    assert_eq!(g.len(), 1, "only the one just added is that recent");
    assert_eq!(g.last_value(), 5);
    // A lone old snapshot is kept: it is the newest.
    let mut old = SnapshotGroup::default();
    old.add(NOW, 3);
    assert_eq!(old.last_value(), 3);
    assert_eq!(old.value_difference(HOUR, HOUR, NOW + 10.0 * HOUR_MS), 0.0);
}

/// The client and the tracker.
struct Rig {
    host: FakeMagHost,
    tracker: InvItemsTracker,
    revision: i64,
}

impl Rig {
    fn new() -> Self {
        Self {
            host: FakeMagHost::new(),
            tracker: InvItemsTracker::default(),
            revision: 0,
        }
    }

    /// The character owns `stack` of `name` worth `value` all told.
    fn own(&mut self, id: u32, name: &str, class: ObjectClass, stack: i32, value: i32) {
        self.host.meta.owned_items.push(InventoryItem {
            object_id: id,
            name: name.to_owned(),
            object_class: class,
            stack_size: stack,
            value,
            icon_id: 0x0600_0000 + id,
            ..InventoryItem::default()
        });
        self.host
            .meta
            .objects
            .push(object(id, name, class, stack, true));
    }

    /// An owned stack is now `stack` worth `value`.
    fn restack(&mut self, id: u32, stack: i32, value: i32) {
        let item = self
            .host
            .meta
            .owned_items
            .iter_mut()
            .find(|item| item.object_id == id)
            .expect("owned");
        item.stack_size = stack;
        item.value = value;
        let object = self
            .host
            .meta
            .objects
            .iter_mut()
            .find(|object| object.object_id == id)
            .expect("known");
        object.stack_size = stack;
        self.change(id, ObjectChangeKind::Updated);
    }

    /// An owned item is used up.
    fn release(&mut self, id: u32) {
        self.host
            .meta
            .owned_items
            .retain(|item| item.object_id != id);
        self.host
            .meta
            .objects
            .retain(|object| object.object_id != id);
        self.change(id, ObjectChangeKind::Released);
    }

    fn change(&mut self, id: u32, kind: ObjectChangeKind) {
        self.revision += 1;
        let current = self.host.meta.object(id);
        let change = ObjectChange {
            object_id: id,
            kind,
            revision: self.revision,
            changed_fields: kind.fields(),
            current,
        };
        self.tracker.on_object_changed(&mut self.host, &change);
    }

    fn login(&mut self) {
        self.tracker.on_login(&mut self.host);
        self.tick();
    }

    fn tick(&mut self) {
        self.tracker.tick(&mut self.host);
    }

    /// The clock moves to `ms`, the frames between at 15 ms.
    fn run_to(&mut self, ms: f64) {
        while self.host.now_ms + 15.0 < ms {
            self.host.now_ms += 15.0;
            self.tick();
        }
        self.host.now_ms = ms;
        self.tick();
    }

    /// The clock jumps to `ms` (one frame).
    fn at(&mut self, ms: f64) {
        self.host.now_ms = ms;
        self.tick();
    }

    fn names(&mut self) -> Vec<String> {
        self.tracker.refresh();
        self.tracker.view().names.clone()
    }

    /// The consumable rows (row 7 on): name, count, ~5m, ~1h, hours.
    fn consumables(&mut self) -> Vec<[String; 5]> {
        self.tracker.refresh();
        let view = self.tracker.view();
        (7..view.names.len())
            .map(|row| {
                [
                    view.names[row].clone(),
                    view.counts[row].clone(),
                    view.five_minutes[row].clone(),
                    view.hour[row].clone(),
                    view.hours[row].clone(),
                ]
            })
            .collect()
    }

    fn consumable(&mut self, name: &str) -> [String; 5] {
        self.consumables()
            .into_iter()
            .find(|row| row[0] == name)
            .unwrap_or_else(|| panic!("no row {name}"))
    }

    fn consumable_names(&mut self) -> Vec<String> {
        self.consumables()
            .into_iter()
            .map(|row| row[0].clone())
            .collect()
    }

    /// A profit row (1 Peas, 2 Comps, 3 Salvage, 4 Net Profit): ~5m, ~1h.
    fn profit(&mut self, row: usize) -> [String; 2] {
        self.tracker.refresh();
        let view = self.tracker.view();
        [view.five_minutes[row].clone(), view.hour[row].clone()]
    }
}

fn object(id: u32, name: &str, class: ObjectClass, stack: i32, owned: bool) -> WorldObject {
    WorldObject {
        object_id: id,
        name: name.to_owned(),
        object_class: class,
        stack_size: stack,
        is_owned: owned,
        icon_id: 0x0600_0000 + id,
        ..WorldObject::default()
    }
}

fn strings(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|text| (*text).to_owned()).collect()
}

#[test]
fn the_fixed_rows_are_there_before_the_login() {
    // InventoryTrackerGUI's constructor: rows 0-6.
    let mut rig = Rig::new();
    rig.tracker.refresh();
    let view = rig.tracker.view();
    assert_eq!(
        view.names,
        strings(&["", "Peas", "Comps", "Salvage", "Net Profit", "", ""])
    );
    assert_eq!(view.counts, strings(&["", "", "", "", "", "", "Count"]));
    assert_eq!(
        view.five_minutes,
        strings(&["MMD/h ~5m", "", "", "", "", "", "Avg/h ~5m"])
    );
    assert_eq!(
        view.hour,
        strings(&["MMD/h ~1h", "", "", "", "", "", "Avg/h ~1h"])
    );
    assert_eq!(view.hours, strings(&["", "", "", "", "", "", "(Hrs)"]));
    assert_eq!(view.icons, vec![0; 7]);
}

#[test]
fn tapers_and_scarabs_tracked_other_comps_not() {
    let mut rig = Rig::new();
    rig.own(
        10,
        "Prismatic Taper",
        ObjectClass::SpellComponent,
        250,
        2500,
    );
    rig.own(11, "Lead Scarab", ObjectClass::SpellComponent, 10, 500);
    rig.own(12, "Hyssop", ObjectClass::SpellComponent, 50, 50);
    rig.own(13, "Mana Stone", ObjectClass::ManaStone, 1, 100);
    rig.own(14, "Peerless Healing Kit", ObjectClass::HealingKit, 1, 300);
    rig.own(15, "Prismatic Taper", ObjectClass::SpellComponent, 50, 500);
    // Named like one, of another class: not counted.
    rig.own(16, "Prismatic Taper", ObjectClass::Misc, 7, 0);
    rig.login();
    assert_eq!(
        rig.consumable_names(),
        [
            "Lead Scarab",
            "Prismatic Taper",
            "Mana Stone",
            "Peerless Healing Kit"
        ]
    );
    assert_eq!(rig.consumable("Prismatic Taper")[1], "300");
    assert_eq!(rig.consumable("Lead Scarab")[1], "10");
    // The row's icon is the first one seen's.
    rig.tracker.refresh();
    assert_eq!(rig.tracker.view().icons[8], 0x0600_0000 + 10);
}

#[test]
fn trade_notes_salvage_and_corrupted_essence_use_group_names() {
    let mut rig = Rig::new();
    rig.own(
        20,
        "Trade Note (250,000)",
        ObjectClass::TradeNote,
        4,
        1_000_000,
    );
    rig.own(
        21,
        "Trade Note (100,000)",
        ObjectClass::TradeNote,
        3,
        300_000,
    );
    rig.own(22, "Salvaged Iron", ObjectClass::Salvage, 1, 400);
    rig.own(23, "Salvaged Steel", ObjectClass::Salvage, 0, 300);
    rig.own(24, "Lesser Corrupted Essence", ObjectClass::Misc, 5, 0);
    rig.own(25, "Corrupted Essence", ObjectClass::Misc, 2, 0);
    rig.own(26, "Pyreal Mote", ObjectClass::Misc, 2, 0);
    rig.login();
    assert_eq!(
        rig.consumable_names(),
        ["Trade Note", "Salvage", "Corrupted Essence"]
    );
    assert_eq!(rig.consumable("Trade Note")[1], "7");
    // A stack of 0 counts as one.
    assert_eq!(rig.consumable("Salvage")[1], "2");
    assert_eq!(rig.consumable("Corrupted Essence")[1], "7");
}

#[test]
fn a_tracked_item_seen_in_a_corpse_adds_a_count_zero_row() {
    let mut rig = Rig::new();
    rig.own(30, "Mana Stone", ObjectClass::ManaStone, 1, 100);
    rig.login();
    // A corpse opened: a stone the character has none of, worth 900.
    rig.host.loot.push(InventoryItem {
        object_id: 31,
        name: "Major Mana Stone".to_owned(),
        object_class: ObjectClass::ManaStone,
        stack_size: 1,
        value: 900,
        ..InventoryItem::default()
    });
    rig.host.meta.objects.push(object(
        31,
        "Major Mana Stone",
        ObjectClass::ManaStone,
        1,
        false,
    ));
    rig.change(31, ObjectChangeKind::Created);
    // On the ground, nothing known of its value: last of its class.
    rig.host.meta.objects.push(object(
        32,
        "Minor Mana Stone",
        ObjectClass::ManaStone,
        1,
        false,
    ));
    rig.change(32, ObjectChangeKind::Created);
    rig.tick();
    // Worth more than the one owned: before it.
    assert_eq!(
        rig.consumable_names(),
        ["Major Mana Stone", "Mana Stone", "Minor Mana Stone"]
    );
    assert_eq!(
        rig.consumable("Major Mana Stone"),
        ["Major Mana Stone", "0", "", "", ""]
    );
    // An object of an untracked class adds nothing.
    rig.host
        .meta
        .objects
        .push(object(33, "Hyssop", ObjectClass::SpellComponent, 1, false));
    rig.change(33, ObjectChangeKind::Created);
    rig.tick();
    assert_eq!(rig.consumables().len(), 3);
}

#[test]
fn rows_group_by_class_then_higher_unit_value_first() {
    // InventoryTrackerGUI.consumableTracker_ItemsAdded, GetZIndex.
    let mut rig = Rig::new();
    rig.own(40, "Salvaged Iron", ObjectClass::Salvage, 1, 400);
    rig.own(41, "Lesser Corrupted Essence", ObjectClass::Misc, 1, 0);
    rig.own(42, "Healing Kit", ObjectClass::HealingKit, 1, 100);
    rig.own(43, "Mana Stone", ObjectClass::ManaStone, 1, 50);
    // Unit values 10 and 50 (the stack's value over its size).
    rig.own(44, "Prismatic Taper", ObjectClass::SpellComponent, 25, 250);
    rig.own(45, "Mana Scarab", ObjectClass::SpellComponent, 10, 500);
    rig.own(46, "Peerless Healing Kit", ObjectClass::HealingKit, 1, 300);
    rig.own(
        47,
        "Trade Note (250,000)",
        ObjectClass::TradeNote,
        1,
        250_000,
    );
    // As much as the taper: after it, seen later.
    rig.own(48, "Lead Scarab", ObjectClass::SpellComponent, 1, 10);
    rig.login();
    assert_eq!(
        rig.consumable_names(),
        [
            "Mana Scarab",
            "Prismatic Taper",
            "Lead Scarab",
            "Mana Stone",
            "Peerless Healing Kit",
            "Healing Kit",
            "Trade Note",
            "Salvage",
            "Corrupted Essence",
        ]
    );
    // A row stays where it was put: its value is the first one seen's.
    rig.restack(44, 1, 1000);
    rig.tick();
    assert_eq!(rig.consumable_names()[1], "Prismatic Taper");
}

#[test]
fn profit_rows_show_mmd_per_hour() {
    // ProfitLossTracker: Net Profit is the comps', salvage's, notes' and
    // money's value; UpdateProfitLossItem: v / 250,000, "N1".
    let mut rig = Rig::new();
    rig.own(50, "Pyreal", ObjectClass::Money, 1000, 1000);
    rig.own(
        51,
        "Prismatic Taper",
        ObjectClass::SpellComponent,
        100,
        1000,
    );
    rig.own(52, "Copper Pea", ObjectClass::SpellComponent, 1, 50);
    rig.login();
    assert_eq!(rig.profit(4), ["", ""]);
    // An hour on, 500,000 more money.
    rig.run_to(HOUR_MS);
    rig.restack(50, 501_000, 501_000);
    rig.tick();
    // The hour's column at once; the five minutes' is the new snapshot
    // itself at its own moment (Mag-Tools: "NaN").
    assert_eq!(rig.profit(4), ["NaN", "2.0"]);
    // Money is not comps, peas or salvage.
    assert_eq!(rig.profit(1), ["", ""]);
    assert_eq!(rig.profit(2), ["", ""]);
    // A refresh pass that reaches it (the rows before it, drawn on another
    // beat, can stop a pass a second more): the five minutes' is 0.
    rig.run_to(HOUR_MS + 3000.0);
    assert_eq!(rig.profit(4)[0], "");
    assert_eq!(rig.profit(4)[1], "2.0");
    // Peas spent: Peas and Net Profit move, Comps not.
    rig.run_to(HOUR_MS + 2.0 * MINUTE_MS);
    rig.release(52);
    rig.tick();
    rig.run_to(HOUR_MS + 2.0 * MINUTE_MS + 3000.0);
    // 50 lost over 1 h 2 min: -50 * 60 / 62 / 250,000, no negative zero.
    assert_eq!(rig.profit(1)[1], "0.0");
    assert_eq!(rig.profit(2)[1], "");
}

#[test]
fn a_zero_count_row_has_empty_rate_cells() {
    let mut rig = Rig::new();
    rig.own(60, "Healing Kit", ObjectClass::HealingKit, 1, 100);
    rig.own(61, "Healing Kit", ObjectClass::HealingKit, 1, 100);
    rig.login();
    rig.run_to(10.0 * MINUTE_MS);
    rig.release(60);
    rig.tick();
    rig.run_to(10.0 * MINUTE_MS + 1100.0);
    // 2 to 1 over ten minutes: -6 an hour, one left lasts ten minutes.
    assert_eq!(
        rig.consumable("Healing Kit"),
        ["Healing Kit", "1", "", "-6.0", "0.2"]
    );
    // The last one used: the row stays, its count 0, its rates empty.
    rig.release(61);
    rig.tick();
    assert_eq!(
        rig.consumable("Healing Kit"),
        ["Healing Kit", "0", "", "", ""]
    );
}

#[test]
fn every_change_of_a_tracked_object_is_a_snapshot_even_with_the_count_unchanged() {
    let mut rig = Rig::new();
    rig.own(70, "Mana Scarab", ObjectClass::SpellComponent, 10, 100);
    rig.login();
    rig.run_to(4.0 * MINUTE_MS);
    rig.restack(70, 4, 40);
    rig.tick();
    rig.run_to(8.0 * MINUTE_MS);
    rig.change(70, ObjectChangeKind::Moved);
    rig.tick();
    rig.run_to(8.0 * MINUTE_MS + 1100.0);
    // Five minutes back (3 min) is closest to the 4 min snapshot, which is
    // the same count: no use over the five minutes.
    assert_eq!(rig.consumable("Mana Scarab")[2], "");
    // Over the hour: -6 in 8 min 1.1 s.
    assert_eq!(rig.consumable("Mana Scarab")[3], "-44.9");
}

#[test]
fn the_refresh_pass_stops_at_a_row_changed_within_the_second() {
    // InventoryTracker.timer_Tick: `return` at the first entry raised less
    // than a second ago, so a busy first entry keeps the others' cells.
    let mut rig = Rig::new();
    rig.own(80, "Mana Scarab", ObjectClass::SpellComponent, 10, 100);
    rig.own(81, "Healing Kit", ObjectClass::HealingKit, 1, 100);
    rig.own(82, "Healing Kit", ObjectClass::HealingKit, 1, 100);
    rig.login();
    rig.at(MINUTE_MS);
    rig.release(81);
    rig.tick();
    // (1 - 2) * 60 / 1.
    assert_eq!(rig.consumable("Healing Kit")[2], "-60.0");
    // The scarabs (first tracked) change every 400 ms for half a minute.
    let mut at = MINUTE_MS;
    while at < 1.5 * MINUTE_MS {
        at += 400.0;
        rig.run_to(at);
        rig.change(80, ObjectChangeKind::Moved);
        rig.tick();
    }
    assert_eq!(rig.consumable("Healing Kit")[2], "-60.0", "kept");
    // Quiet again: the pass a second after the last change reaches the
    // kits.
    let now = at + 1000.0;
    rig.at(now);
    assert_eq!(
        rig.consumable("Healing Kit")[2],
        n1(-60.0 / (now / MINUTE_MS))
    );
}

#[test]
fn the_cells_refresh_about_once_a_second() {
    let mut rig = Rig::new();
    rig.own(90, "Healing Kit", ObjectClass::HealingKit, 1, 100);
    rig.own(91, "Healing Kit", ObjectClass::HealingKit, 1, 100);
    rig.login();
    rig.at(MINUTE_MS);
    rig.release(90);
    rig.tick();
    assert_eq!(rig.consumable("Healing Kit")[2], "-60.0");
    // Half a second: not yet.
    rig.run_to(MINUTE_MS + 600.0);
    assert_eq!(rig.consumable("Healing Kit")[2], "-60.0");
    // A second on: (1 - 2) * 60 / 1.0167.
    rig.run_to(MINUTE_MS + 1100.0);
    assert_ne!(rig.consumable("Healing Kit")[2], "-60.0");
}

#[test]
fn a_released_object_is_known_by_what_was_seen_of_it() {
    let mut rig = Rig::new();
    rig.own(100, "Mana Stone", ObjectClass::ManaStone, 1, 100);
    rig.own(101, "Mana Stone", ObjectClass::ManaStone, 1, 100);
    rig.login();
    assert_eq!(rig.consumable("Mana Stone")[1], "2");
    rig.release(100);
    rig.tick();
    assert_eq!(rig.consumable("Mana Stone")[1], "1");
}

#[test]
fn nothing_is_tracked_before_the_login_and_the_login_waits_for_the_owned_list() {
    let mut rig = Rig::new();
    rig.own(110, "Mana Stone", ObjectClass::ManaStone, 1, 100);
    rig.change(110, ObjectChangeKind::Created);
    rig.tick();
    assert!(rig.consumables().is_empty());
    rig.host.complete = false;
    rig.login();
    assert!(rig.consumables().is_empty());
    rig.host.complete = true;
    rig.tick();
    assert_eq!(rig.consumable_names(), ["Mana Stone"]);
    // A new login starts afresh.
    rig.host.meta.owned_items.clear();
    rig.host.meta.objects.clear();
    rig.login();
    assert!(rig.consumables().is_empty());
    assert_eq!(rig.names().len(), 7);
}

#[test]
fn an_event_is_classified_without_asking_the_client() {
    // The per-event work stays small: a tracked name already known asks
    // the client nothing until the frame recounts it.
    let mut rig = Rig::new();
    rig.own(120, "Mana Stone", ObjectClass::ManaStone, 1, 100);
    rig.own(121, "Roast Beef", ObjectClass::Food, 1, 10);
    rig.login();
    let listings = rig.host.owned_listings;
    for _ in 0..50 {
        rig.change(120, ObjectChangeKind::Moved);
        rig.change(121, ObjectChangeKind::Moved);
    }
    assert_eq!(rig.host.owned_listings, listings);
    rig.tick();
    assert_eq!(rig.host.owned_listings, listings + 1);
    // An untracked object's change asks nothing at the frame either.
    rig.change(121, ObjectChangeKind::Moved);
    rig.tick();
    assert_eq!(rig.host.owned_listings, listings + 1);
}

//! The identify chat line and its two triggers: an item the player
//! identifies, and the items of an opened corpse or chest.
//!
//! Source: Mag-Tools `Mag-Tools/ItemInfo/ItemInfoPrinter.cs`,
//! `DetectItemsIdentifiedByUser.cs`, `DetectItemsInContainers.cs`,
//! `ItemInfoIdentArgs.cs` and `VirindiTankLootRuleProcessor.cs` at 43ee2ec.
//! The expected lines are worked out from those (without the `<Tell>`
//! wrapper, which a plugin's chat line cannot carry) and from
//! `ItemInfo.cs` for the info text.

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::{ItemProperties, ObjectClass, WorldObject};
use mosstank_mag::host::Verdict;
use mosstank_mag::identify::IdentifyQueue;
use mosstank_mag::printer::{PrintSettings, Printer, Split, TEXT_TYPE, line};
use mosstank_mag::testing::{FakeMagHost, FakeVerdict};

const RING: u32 = 0x5000_0001;
const CORPSE: u32 = 0x7000_0001;

fn verdict(keep: bool, rule_name: &str, salvage: bool) -> Verdict {
    Verdict {
        keep,
        rule_name: rule_name.to_owned(),
        salvage,
    }
}

fn settings() -> PrintSettings {
    PrintSettings {
        enabled: true,
        show_buffed_values: true,
        show_value_and_burden: false,
    }
}

fn object(id: u32, name: &str, class: ObjectClass) -> WorldObject {
    WorldObject {
        object_id: id,
        name: name.to_owned(),
        object_class: class,
        has_appraisal_data: true,
        ..WorldObject::default()
    }
}

fn item(id: u32, name: &str, class: ObjectClass, container_id: u32) -> InventoryItem {
    InventoryItem {
        object_id: id,
        name: name.to_owned(),
        object_class: class,
        container_id,
        ..InventoryItem::default()
    }
}

/// The parts one test drives.
#[derive(Default)]
struct Rig {
    host: FakeMagHost,
    verdict: FakeVerdict,
    queue: IdentifyQueue,
    printer: Printer,
}

impl Rig {
    fn new() -> Self {
        Self::default()
    }

    /// The character owns `items`, each known to the world as appraised.
    fn own(&mut self, items: &[InventoryItem]) {
        for item in items {
            self.host
                .meta
                .objects
                .push(object(item.object_id, &item.name, item.object_class));
            self.host
                .meta
                .properties
                .insert(item.object_id, ItemProperties::default());
        }
        self.host.meta.owned_items.extend_from_slice(items);
    }

    fn select(&mut self, id: u32, at_ms: f64) {
        self.printer.on_selection(Some(id), at_ms, true);
    }

    fn ident_with(&mut self, object: &WorldObject, at_ms: f64, settings: PrintSettings) {
        self.host.now_ms = at_ms;
        let mut call = Split {
            host: &mut self.host,
            verdict: &mut self.verdict,
        };
        self.printer
            .on_ident(&mut call, &mut self.queue, object, at_ms, settings);
    }

    fn ident(&mut self, object: &WorldObject, at_ms: f64) {
        self.ident_with(object, at_ms, settings());
    }

    /// Opens the container `id` named `name`, holding `items`.
    fn open(&mut self, id: u32, name: &str, items: &[InventoryItem], at_ms: f64) {
        self.host
            .meta
            .objects
            .push(object(id, name, ObjectClass::Corpse));
        for item in items {
            let mut seen = object(item.object_id, &item.name, item.object_class);
            seen.container_id = id;
            self.host.meta.objects.push(seen);
            self.host
                .meta
                .properties
                .insert(item.object_id, ItemProperties::default());
        }
        self.host.loot = items.to_vec();
        self.host.meta.open_container = id;
        self.printer.on_container_opened(name, at_ms, true);
    }

    fn close(&mut self) {
        self.host.meta.open_container = 0;
        self.host.loot.clear();
    }

    fn tick(&mut self, at_ms: f64) {
        self.host.now_ms = at_ms;
        let mut call = Split {
            host: &mut self.host,
            verdict: &mut self.verdict,
        };
        self.printer
            .tick(&mut call, &mut self.queue, at_ms, settings());
    }

    /// Ticks every 50 ms from `from_ms` to `to_ms`, both included.
    fn ticks(&mut self, from_ms: f64, to_ms: f64) {
        let mut at = from_ms;
        while at <= to_ms {
            self.tick(at);
            at += 50.0;
        }
    }

    fn posted(&self) -> Vec<(String, u32)> {
        self.host.meta.posted.clone()
    }

    fn posted_text(&self) -> Vec<String> {
        self.host.posted_text()
    }
}

// ---- The line ----

#[test]
fn the_line_shapes() {
    // Source: ItemInfoPrinter.cs:121-137 (doProcessItemInfoCallback), 85-89
    // (no loot engine: the info alone).
    let keep = verdict(true, "Rares", false);
    assert_eq!(line(Some(&keep), "Ring"), "+(Rares) Ring");
    let declined = verdict(false, "Junk", false);
    assert_eq!(line(Some(&declined), "Ring"), "-(Junk) Ring");
    let no_rule = verdict(false, "", false);
    assert_eq!(line(Some(&no_rule), "Ring"), "- Ring");
    let kept_unnamed = verdict(true, "", false);
    assert_eq!(line(Some(&kept_unnamed), "Ring"), "+ Ring");
    assert_eq!(line(None, "Ring"), "Ring");
}

// ---- The player's identifies ----

#[test]
fn a_user_identify_within_ten_seconds_prints_once_in_colour_14() {
    // Source: DetectItemsIdentifiedByUser.cs:85-110 (ItemSelected),
    // 113-162 (IdentReceived: the entry is removed, so it prints once);
    // ItemInfoPrinter.cs:65-68, 140.
    let mut rig = Rig::new();
    let ring = item(RING, "Test Ring", ObjectClass::Jewelry, 1);
    rig.own(std::slice::from_ref(&ring));
    rig.verdict.0.insert(RING, verdict(true, "Rares", false));
    rig.select(RING, 0.0);
    let seen = object(RING, "Test Ring", ObjectClass::Jewelry);
    rig.ident(&seen, 2_000.0);
    rig.ident(&seen, 3_000.0);
    assert_eq!(rig.posted(), vec![("+(Rares) Test Ring".to_string(), 14)]);
    assert_eq!(TEXT_TYPE, 14);
}

#[test]
fn an_identify_eleven_seconds_after_selecting_prints_nothing() {
    // Source: DetectItemsIdentifiedByUser.cs:124-141: entries older than
    // 10 s are dropped first.
    let mut rig = Rig::new();
    rig.own(&[item(RING, "Test Ring", ObjectClass::Jewelry, 1)]);
    rig.select(RING, 0.0);
    rig.ident(&object(RING, "Test Ring", ObjectClass::Jewelry), 11_000.0);
    assert!(rig.posted().is_empty());
}

#[test]
fn exactly_ten_seconds_still_prints_and_selecting_again_starts_the_window_over() {
    // Source: DetectItemsIdentifiedByUser.cs:132 (`selected + 10 s < now`
    // drops it, so 10 s itself is kept), 95-98 (a new selection replaces
    // the time).
    let mut rig = Rig::new();
    rig.own(&[item(RING, "Test Ring", ObjectClass::Jewelry, 1)]);
    let seen = object(RING, "Test Ring", ObjectClass::Jewelry);
    rig.select(RING, 0.0);
    rig.ident(&seen, 10_000.0);
    assert_eq!(rig.posted_text(), vec!["Test Ring"]);
    rig.select(RING, 20_000.0);
    rig.select(RING, 25_000.0);
    rig.ident(&seen, 34_000.0);
    assert_eq!(rig.posted_text(), vec!["Test Ring", "Test Ring"]);
}

#[test]
fn every_identify_prunes_the_old_selections() {
    // Source: DetectItemsIdentifiedByUser.cs:124-141: the pruning runs on
    // any appraisal, before the object is looked up.
    let mut rig = Rig::new();
    let other = 0x5000_0002;
    rig.own(&[
        item(RING, "Test Ring", ObjectClass::Jewelry, 1),
        item(other, "Other", ObjectClass::Misc, 1),
    ]);
    rig.select(RING, 0.0);
    rig.select(other, 0.0);
    assert_eq!(rig.printer.selected_count(), 2);
    rig.ident(&object(0x5000_0003, "Nothing", ObjectClass::Misc), 10_500.0);
    assert_eq!(rig.printer.selected_count(), 0);
}

#[test]
fn doors_portals_npcs_vendors_corpses_lifestones_foci_housing_are_skipped() {
    // Source: DetectItemsIdentifiedByUser.cs:148-156.
    let skipped = [
        ObjectClass::Corpse,
        ObjectClass::Door,
        ObjectClass::Foci,
        ObjectClass::Housing,
        ObjectClass::Lifestone,
        ObjectClass::Npc,
        ObjectClass::Portal,
        ObjectClass::Vendor,
    ];
    for class in skipped {
        let mut rig = Rig::new();
        rig.select(RING, 0.0);
        rig.ident(&object(RING, "Thing", class), 1_000.0);
        assert!(rig.posted().is_empty(), "{class:?}");
        // Selected and appraised, so it is gone from the list.
        assert_eq!(rig.printer.selected_count(), 0, "{class:?}");
    }
    // A monster or a player prints.
    for class in [ObjectClass::Monster, ObjectClass::Player] {
        let mut rig = Rig::new();
        rig.select(RING, 0.0);
        rig.ident(&object(RING, "Drudge Skulker", class), 1_000.0);
        assert_eq!(rig.posted_text(), vec!["Drudge Skulker"], "{class:?}");
    }
}

#[test]
fn an_unselected_identify_prints_nothing() {
    // Source: DetectItemsIdentifiedByUser.cs:143-144.
    let mut rig = Rig::new();
    rig.own(&[item(RING, "Test Ring", ObjectClass::Jewelry, 1)]);
    rig.ident(&object(RING, "Test Ring", ObjectClass::Jewelry), 1_000.0);
    assert!(rig.posted().is_empty());
}

#[test]
fn without_a_profile_the_player_sees_the_info_alone() {
    // Source: ItemInfoPrinter.cs:83-89: no loot engine and the user path
    // (DontShowIfItemHasNoRule false): the info, no prefix.
    let mut rig = Rig::new();
    rig.own(&[item(RING, "Test Ring", ObjectClass::Jewelry, 1)]);
    rig.select(RING, 0.0);
    rig.ident(&object(RING, "Test Ring", ObjectClass::Jewelry), 1_000.0);
    assert_eq!(rig.posted(), vec![("Test Ring".to_string(), 14)]);
}

#[test]
fn a_user_identify_with_no_rule_shows_a_minus() {
    // Source: ItemInfoPrinter.cs:116-127: the user path shows an item no
    // rule matched.
    let mut rig = Rig::new();
    rig.own(&[item(RING, "Test Ring", ObjectClass::Jewelry, 1)]);
    rig.verdict.0.insert(RING, verdict(false, "", false));
    rig.select(RING, 0.0);
    rig.ident(&object(RING, "Test Ring", ObjectClass::Jewelry), 1_000.0);
    assert_eq!(rig.posted_text(), vec!["- Test Ring"]);
}

#[test]
fn turned_off_nothing_is_remembered_or_printed() {
    // Source: DetectItemsIdentifiedByUser.cs:89-90, 117-118.
    let mut rig = Rig::new();
    rig.own(&[item(RING, "Test Ring", ObjectClass::Jewelry, 1)]);
    rig.printer.on_selection(Some(RING), 0.0, false);
    assert_eq!(rig.printer.selected_count(), 0);
    rig.select(RING, 0.0);
    let off = PrintSettings {
        enabled: false,
        ..settings()
    };
    let seen = object(RING, "Test Ring", ObjectClass::Jewelry);
    rig.ident_with(&seen, 1_000.0, off);
    assert!(rig.posted().is_empty());
    // Not consumed while off: on again, it prints.
    rig.ident(&seen, 2_000.0);
    assert_eq!(rig.posted_text(), vec!["Test Ring"]);
}

#[test]
fn the_settings_reach_the_info() {
    // Source: ItemInfo.cs:327-334 (value and burden).
    let mut rig = Rig::new();
    let mut coin = item(RING, "Coin", ObjectClass::Misc, 1);
    coin.value = 12_345;
    coin.burden = 3;
    rig.own(std::slice::from_ref(&coin));
    rig.select(RING, 0.0);
    let with_value = PrintSettings {
        show_value_and_burden: true,
        ..settings()
    };
    let mut seen = object(RING, "Coin", ObjectClass::Misc);
    seen.is_owned = true;
    rig.ident_with(&seen, 1_000.0, with_value);
    assert_eq!(rig.posted_text(), vec!["Coin, Value 12,345, BU 3"]);
}

#[test]
fn an_object_not_owned_reads_as_the_world_knows_it() {
    // A monster on the landscape: no item record, the world object's name.
    let mut rig = Rig::new();
    rig.select(RING, 0.0);
    rig.verdict.0.insert(RING, verdict(false, "", false));
    rig.ident(
        &object(RING, "Drudge Skulker", ObjectClass::Monster),
        1_000.0,
    );
    assert_eq!(rig.posted_text(), vec!["- Drudge Skulker"]);
}

// ---- The items of an opened container ----

#[test]
fn container_items_print_only_with_a_rule_and_chests_skip_salvage() {
    // Source: DetectItemsInContainers.cs:122-150 (each item, chests and
    // vaults set DontShowIfIsSalvageRule), ItemInfoPrinter.cs:116-120
    // (no rule or a skipped salvage rule: nothing).
    let mut rig = Rig::new();
    let a = item(0x5100_0001, "Ring A", ObjectClass::Jewelry, CORPSE);
    let b = item(0x5100_0002, "Ring B", ObjectClass::Jewelry, CORPSE);
    let c = item(0x5100_0003, "Salvaged Iron", ObjectClass::Salvage, CORPSE);
    rig.verdict
        .0
        .insert(a.object_id, verdict(true, "Rares", false));
    rig.verdict.0.insert(b.object_id, verdict(false, "", false));
    rig.verdict
        .0
        .insert(c.object_id, verdict(true, "Salvage", true));
    rig.open(CORPSE, "Corpse of Drudge", &[a, b, c], 0.0);
    rig.ticks(0.0, 1_000.0);
    assert_eq!(
        rig.posted(),
        vec![
            ("+(Rares) Ring A".to_string(), 14),
            ("+(Salvage) Salvaged Iron".to_string(), 14),
        ]
    );

    for (id, name) in [
        (0x7000_0002, "Chest"),
        (0x7000_0003, "Locked Vault"),
        (0x7000_0004, "Ancient Reliquary"),
    ] {
        let mut rig = Rig::new();
        let iron = item(id + 0x100, "Salvaged Iron", ObjectClass::Salvage, id);
        let declined = item(id + 0x200, "Junk", ObjectClass::Misc, id);
        rig.verdict
            .0
            .insert(iron.object_id, verdict(true, "Salvage", true));
        rig.verdict
            .0
            .insert(declined.object_id, verdict(false, "Junk", false));
        rig.open(id, name, &[iron, declined], 0.0);
        rig.ticks(0.0, 1_000.0);
        // A declining rule names itself, so it shows.
        assert_eq!(rig.posted_text(), vec!["-(Junk) Junk"], "{name}");
    }
}

#[test]
fn without_a_profile_container_items_print_nothing() {
    // Source: ItemInfoPrinter.cs:83 (`!e.DontShowIfItemHasNoRule`).
    let mut rig = Rig::new();
    let a = item(0x5100_0001, "Ring A", ObjectClass::Jewelry, CORPSE);
    rig.open(CORPSE, "Corpse of Drudge", &[a], 0.0);
    rig.ticks(0.0, 6_000.0);
    assert!(rig.posted().is_empty());
}

#[test]
fn a_housing_storage_chest_is_not_scanned() {
    // Source: DetectItemsInContainers.cs:66-67.
    let mut rig = Rig::new();
    let a = item(0x5100_0001, "Ring A", ObjectClass::Jewelry, CORPSE);
    rig.verdict
        .0
        .insert(a.object_id, verdict(true, "Rares", false));
    rig.open(CORPSE, "Storage", &[a], 0.0);
    rig.ticks(0.0, 1_000.0);
    assert!(rig.posted().is_empty());
    assert!(rig.printer.is_idle());
}

#[test]
fn turned_off_an_opened_container_is_not_scanned() {
    // Source: DetectItemsInContainers.cs:56-57.
    let mut rig = Rig::new();
    rig.printer
        .on_container_opened("Corpse of Drudge", 0.0, false);
    assert!(rig.printer.is_idle());
}

#[test]
fn the_container_scan_stops_after_five_seconds() {
    // Source: DetectItemsInContainers.cs:152-156.
    let mut rig = Rig::new();
    let a = item(0x5100_0001, "Ring A", ObjectClass::Jewelry, CORPSE);
    let late = item(0x5100_0002, "Ring B", ObjectClass::Jewelry, CORPSE);
    rig.verdict
        .0
        .insert(a.object_id, verdict(true, "Rares", false));
    rig.verdict
        .0
        .insert(late.object_id, verdict(true, "Rares", false));
    rig.open(CORPSE, "Corpse of Drudge", std::slice::from_ref(&a), 0.0);
    rig.ticks(0.0, 4_900.0);
    assert!(!rig.printer.is_idle());
    rig.ticks(4_950.0, 5_100.0);
    assert!(rig.printer.is_idle());
    // An item listed after the scan stopped is not seen.
    rig.host.loot.push(late);
    rig.ticks(5_150.0, 8_000.0);
    assert_eq!(rig.posted_text(), vec!["+(Rares) Ring A"]);
}

#[test]
fn an_empty_container_is_waited_on_without_a_timeout() {
    // Source: DetectItemsInContainers.cs:118-120 (no items yet: wait,
    // before the five-second check).
    let mut rig = Rig::new();
    rig.open(CORPSE, "Corpse of Drudge", &[], 0.0);
    rig.ticks(0.0, 20_000.0);
    assert!(!rig.printer.is_idle());
    let a = item(0x5100_0001, "Ring A", ObjectClass::Jewelry, CORPSE);
    rig.verdict
        .0
        .insert(a.object_id, verdict(true, "Rares", false));
    rig.host.loot.push(a);
    rig.ticks(20_050.0, 20_500.0);
    assert_eq!(rig.posted_text(), vec!["+(Rares) Ring A"]);
    // Every listed item processed and past five seconds: done.
    assert!(rig.printer.is_idle());
}

#[test]
fn closing_the_container_stops_the_scan() {
    // Source: DetectItemsInContainers.cs:110-114.
    let mut rig = Rig::new();
    rig.open(CORPSE, "Corpse of Drudge", &[], 0.0);
    rig.tick(0.0);
    rig.close();
    rig.tick(200.0);
    assert!(rig.printer.is_idle());
}

#[test]
fn the_scan_thinks_every_hundred_milliseconds() {
    // Source: DetectItemsInContainers.cs:97-101.
    let mut rig = Rig::new();
    rig.open(CORPSE, "Corpse of Drudge", &[], 0.0);
    rig.tick(0.0);
    let a = item(0x5100_0001, "Ring A", ObjectClass::Jewelry, CORPSE);
    rig.verdict
        .0
        .insert(a.object_id, verdict(true, "Rares", false));
    rig.host.loot.push(a);
    rig.tick(50.0);
    assert!(rig.posted().is_empty());
    rig.tick(100.0);
    assert_eq!(rig.posted_text(), vec!["+(Rares) Ring A"]);
}

#[test]
fn a_second_container_does_not_restart_the_clock() {
    // Source: DetectItemsInContainers.cs:78-82 (Start does nothing while
    // started).
    let mut rig = Rig::new();
    rig.open(CORPSE, "Corpse of Drudge", &[], 0.0);
    rig.ticks(0.0, 3_000.0);
    let other = 0x7000_0009;
    let a = item(0x5100_0001, "Ring A", ObjectClass::Jewelry, other);
    rig.verdict
        .0
        .insert(a.object_id, verdict(true, "Rares", false));
    rig.open(other, "Corpse of Banderling", &[a], 3_000.0);
    rig.ticks(3_050.0, 5_100.0);
    assert!(rig.printer.is_idle());
    assert_eq!(rig.posted_text(), vec!["+(Rares) Ring A"]);
}

#[test]
fn each_container_item_is_processed_once_a_session() {
    // Source: DetectItemsInContainers.cs:84, 138-141 (itemsProcessed is
    // never cleared).
    let mut rig = Rig::new();
    let a = item(0x5100_0001, "Ring A", ObjectClass::Jewelry, CORPSE);
    rig.verdict
        .0
        .insert(a.object_id, verdict(true, "Rares", false));
    rig.open(CORPSE, "Corpse of Drudge", std::slice::from_ref(&a), 0.0);
    rig.ticks(0.0, 6_000.0);
    rig.close();
    rig.tick(6_100.0);
    rig.open(
        CORPSE,
        "Corpse of Drudge",
        std::slice::from_ref(&a),
        7_000.0,
    );
    rig.ticks(7_000.0, 13_000.0);
    assert_eq!(rig.posted_text(), vec!["+(Rares) Ring A"]);
    // A new session forgets them.
    rig.printer.reset();
    rig.close();
    rig.open(CORPSE, "Corpse of Drudge", &[a], 14_000.0);
    rig.ticks(14_000.0, 15_000.0);
    assert_eq!(
        rig.posted_text(),
        vec!["+(Rares) Ring A", "+(Rares) Ring A"]
    );
}

#[test]
fn only_the_open_containers_own_items_are_scanned() {
    // Source: DetectItemsInContainers.cs:136 (`GetByContainer`: the items
    // directly in it, not those in a pack inside).
    let mut rig = Rig::new();
    let pack = 0x5100_0010;
    let a = item(0x5100_0001, "Ring A", ObjectClass::Jewelry, CORPSE);
    let inner = item(0x5100_0002, "Ring B", ObjectClass::Jewelry, pack);
    rig.verdict
        .0
        .insert(a.object_id, verdict(true, "Rares", false));
    rig.verdict
        .0
        .insert(inner.object_id, verdict(true, "Rares", false));
    rig.open(CORPSE, "Chest", &[a, inner], 0.0);
    rig.ticks(0.0, 6_000.0);
    assert_eq!(rig.posted_text(), vec!["+(Rares) Ring A"]);
}

#[test]
fn a_container_item_the_profile_must_appraise_is_queued_and_printed_when_it_arrives() {
    // Source: VirindiTankLootRuleProcessor.cs:40-50 (QueryNeedsID: queue
    // the item and print in the callback), 63-85 (the callback).
    let mut rig = Rig::new();
    let a = item(0x5100_0001, "Ring A", ObjectClass::Jewelry, CORPSE);
    rig.verdict
        .0
        .insert(a.object_id, verdict(true, "Rares", false));
    rig.verdict.1.insert(a.object_id);
    rig.open(CORPSE, "Corpse of Drudge", std::slice::from_ref(&a), 0.0);
    // Not appraised yet.
    for seen in &mut rig.host.meta.objects {
        if seen.object_id == a.object_id {
            seen.has_appraisal_data = false;
        }
    }
    rig.ticks(0.0, 1_000.0);
    assert!(rig.posted().is_empty());
    assert!(rig.queue.contains(a.object_id));
    // The appraisal arrives (whoever asked for it).
    let mut seen = object(a.object_id, "Ring A", ObjectClass::Jewelry);
    seen.container_id = CORPSE;
    rig.ident(&seen, 1_500.0);
    assert_eq!(rig.posted(), vec![("+(Rares) Ring A".to_string(), 14)]);
    assert!(!rig.queue.contains(a.object_id));
    // Once only.
    rig.ident(&seen, 2_000.0);
    assert_eq!(rig.posted().len(), 1);
}

#[test]
fn an_appraised_container_item_is_decided_at_once() {
    // Source: VirindiTankLootRuleProcessor.cs:51-56 (no appraisal needed:
    // classified at once).
    let mut rig = Rig::new();
    let a = item(0x5100_0001, "Ring A", ObjectClass::Jewelry, CORPSE);
    rig.verdict
        .0
        .insert(a.object_id, verdict(true, "Rares", false));
    rig.verdict.1.insert(a.object_id);
    rig.open(CORPSE, "Corpse of Drudge", &[a], 0.0);
    rig.ticks(0.0, 200.0);
    assert_eq!(rig.posted_text(), vec!["+(Rares) Ring A"]);
    assert!(rig.queue.is_empty());
}

#[test]
fn a_waited_appraisal_after_the_container_closed_prints_nothing() {
    let mut rig = Rig::new();
    let a = item(0x5100_0001, "Ring A", ObjectClass::Jewelry, CORPSE);
    rig.verdict
        .0
        .insert(a.object_id, verdict(true, "Rares", false));
    rig.verdict.1.insert(a.object_id);
    rig.open(CORPSE, "Corpse of Drudge", std::slice::from_ref(&a), 0.0);
    for seen in &mut rig.host.meta.objects {
        seen.has_appraisal_data = false;
    }
    rig.ticks(0.0, 300.0);
    assert!(rig.queue.contains(a.object_id));
    rig.close();
    rig.ticks(350.0, 500.0);
    // Forgotten, and no longer asked for.
    assert!(!rig.queue.contains(a.object_id));
    let mut seen = object(a.object_id, "Ring A", ObjectClass::Jewelry);
    seen.container_id = CORPSE;
    rig.ident(&seen, 1_000.0);
    assert!(rig.posted().is_empty());
    assert!(rig.printer.is_idle());
}

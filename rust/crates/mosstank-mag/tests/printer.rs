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
use ac_plugin_abi::world::{ItemProperties, ObjectClass, WorldObject, selection_source};
use mosstank_mag::host::Verdict;
use mosstank_mag::identify::IdentifyQueue;
use mosstank_mag::printer::{IdentCall, PrintSettings, Printer, Split, TEXT_TYPE, line};
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
        left_click_ident: true,
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

    /// The player selected `id` with a key.
    fn select(&mut self, id: u32, at_ms: f64) {
        self.select_by(id, selection_source::KEYBOARD, at_ms, settings());
    }

    fn select_by(&mut self, id: u32, source: u8, at_ms: f64, settings: PrintSettings) {
        self.printer
            .on_selection(&mut self.queue, Some(id), source, at_ms, settings);
    }

    fn ident_with(&mut self, object: &WorldObject, at_ms: f64, settings: PrintSettings) {
        self.host.now_ms = at_ms;
        let mut call = Split::new(&mut self.host, &mut self.verdict);
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
        let mut call = Split::new(&mut self.host, &mut self.verdict);
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
fn a_mouse_selection_asks_for_its_appraisal_ahead_of_everything_waiting() {
    // Source: DetectItemsIdentifiedByUser.cs:86-90 (LeftClickIdent's
    // RequestId), with the client saying which selections the mouse made.
    let mut rig = Rig::new();
    rig.queue.push(0x5000_0007);
    rig.queue.push_urgent(0x5000_0008);
    rig.select_by(RING, selection_source::MOUSE, 0.0, settings());
    assert_eq!(rig.queue.front(), Some(RING));
    assert_eq!(rig.queue.len(), 3);
    assert_eq!(rig.printer.selected_count(), 1);
}

#[test]
fn only_a_mouse_selection_with_the_left_click_identify_on_asks() {
    let mut rig = Rig::new();
    for source in [
        selection_source::KEYBOARD,
        selection_source::SYSTEM,
        selection_source::PLUGIN,
    ] {
        rig.select_by(RING, source, 0.0, settings());
        assert!(rig.queue.is_empty(), "source {source}");
    }
    let off = PrintSettings {
        left_click_ident: false,
        ..settings()
    };
    rig.select_by(RING, selection_source::MOUSE, 0.0, off);
    assert!(rig.queue.is_empty());
    // Each still waits for the player's own appraisal.
    assert_eq!(rig.printer.selected_count(), 1);
}

#[test]
fn the_selected_objects_appraisal_ends_the_clicks_wait_and_prints_once() {
    let mut rig = Rig::new();
    rig.own(&[item(RING, "Test Ring", ObjectClass::Jewelry, 1)]);
    rig.select_by(RING, selection_source::MOUSE, 0.0, settings());
    assert!(rig.queue.contains(RING));
    let seen = object(RING, "Test Ring", ObjectClass::Jewelry);
    rig.ident(&seen, 9_000.0);
    assert!(!rig.queue.contains(RING));
    rig.ident(&seen, 9_500.0);
    assert_eq!(rig.posted_text(), vec!["Test Ring"]);
}

#[test]
fn turned_off_nothing_is_remembered_or_printed() {
    // Source: DetectItemsIdentifiedByUser.cs:89-90, 117-118.
    let mut rig = Rig::new();
    rig.own(&[item(RING, "Test Ring", ObjectClass::Jewelry, 1)]);
    let off = PrintSettings {
        enabled: false,
        ..settings()
    };
    // Nor is a click's appraisal asked for.
    rig.select_by(RING, selection_source::MOUSE, 0.0, off);
    assert_eq!(rig.printer.selected_count(), 0);
    assert!(rig.queue.is_empty());
    rig.select(RING, 0.0);
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

/// A container `CORPSE` with `count` kept rings, opened at 0 s.
fn rings(rig: &mut Rig, count: u32) -> Vec<InventoryItem> {
    let items: Vec<InventoryItem> = (0..count)
        .map(|n| {
            item(
                0x5100_0001 + n,
                &format!("Ring {n}"),
                ObjectClass::Jewelry,
                CORPSE,
            )
        })
        .collect();
    for ring in &items {
        rig.verdict
            .0
            .insert(ring.object_id, verdict(true, "Rares", false));
    }
    rig.open(CORPSE, "Corpse of Drudge", &items, 0.0);
    items
}

/// Ticks every `step_ms` from `from_ms` to `to_ms`, both included.
fn ticks_every(rig: &mut Rig, from_ms: f64, to_ms: f64, step_ms: f64) {
    let mut at = from_ms;
    while at <= to_ms {
        rig.tick(at);
        at += step_ms;
    }
}

#[test]
fn every_item_listed_at_the_open_prints_even_past_five_seconds() {
    // Source: DetectItemsInContainers.cs:136-150: each think raises every
    // new item at once; here they are listed at once and weighed one a
    // frame, so a full corpse goes on past the five seconds.
    let mut rig = Rig::new();
    rings(&mut rig, 80);
    ticks_every(&mut rig, 0.0, 9_000.0, 100.0);
    let posted = rig.posted_text();
    assert_eq!(posted.len(), 80);
    assert_eq!(posted[0], "+(Rares) Ring 0");
    assert_eq!(posted[79], "+(Rares) Ring 79");
    assert!(rig.printer.is_idle());
}

#[test]
fn an_item_listed_after_five_seconds_does_not_print_while_the_backlog_runs() {
    // Source: DetectItemsInContainers.cs:152-156: the scan stops at the
    // first think at or after five seconds.
    let mut rig = Rig::new();
    rings(&mut rig, 80);
    ticks_every(&mut rig, 0.0, 5_100.0, 100.0);
    let late = item(0x5200_0001, "Late Ring", ObjectClass::Jewelry, CORPSE);
    rig.verdict
        .0
        .insert(late.object_id, verdict(true, "Rares", false));
    rig.host.loot.push(late);
    ticks_every(&mut rig, 5_200.0, 10_000.0, 100.0);
    let posted = rig.posted_text();
    assert_eq!(posted.len(), 80);
    assert!(!posted.iter().any(|line| line.contains("Late Ring")));
}

#[test]
fn an_item_gone_from_the_container_before_its_turn_still_prints() {
    // Source: DetectItemsInContainers.cs:136-150: Mag-Tools raised every
    // listed item in the think that listed it, so the looter taking one
    // did not hide its line.
    let mut rig = Rig::new();
    let items = rings(&mut rig, 3);
    rig.tick(0.0);
    rig.host.loot.retain(|i| i.object_id != items[2].object_id);
    rig.close();
    ticks_every(&mut rig, 50.0, 1_000.0, 50.0);
    assert_eq!(
        rig.posted_text(),
        vec!["+(Rares) Ring 0", "+(Rares) Ring 1", "+(Rares) Ring 2"]
    );
}

/// A container whose one item the profile must appraise first, weighed.
fn awaiting(rig: &mut Rig) -> WorldObject {
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
    let mut seen = object(a.object_id, "Ring A", ObjectClass::Jewelry);
    seen.container_id = CORPSE;
    seen
}

#[test]
fn a_waited_appraisal_after_the_container_closed_still_prints() {
    // Source: VirindiTankLootRuleProcessor.cs:63-85: the callbacks waiting
    // were never cleared. Closing only stops asking for the appraisal.
    let mut rig = Rig::new();
    let seen = awaiting(&mut rig);
    rig.close();
    rig.ticks(350.0, 500.0);
    assert!(!rig.queue.contains(seen.object_id));
    rig.ident(&seen, 1_000.0);
    assert_eq!(rig.posted(), vec![("+(Rares) Ring A".to_string(), 14)]);
    assert!(rig.printer.is_idle());
}

#[test]
fn a_waited_appraisal_after_the_item_left_the_container_still_prints() {
    let mut rig = Rig::new();
    let seen = awaiting(&mut rig);
    rig.host.loot.clear();
    rig.ident(&seen, 1_000.0);
    assert_eq!(rig.posted_text(), vec!["+(Rares) Ring A"]);
}

#[test]
fn an_appraisal_waited_on_for_thirty_seconds_is_given_up() {
    // Bounded: an appraisal that never comes is forgotten after 30 s.
    let mut rig = Rig::new();
    let seen = awaiting(&mut rig);
    rig.ticks(350.0, 30_400.0);
    assert!(rig.printer.is_idle());
    assert!(!rig.queue.contains(seen.object_id));
    rig.ident(&seen, 31_000.0);
    assert!(rig.posted().is_empty());
}

/// A call whose verdict reads the owned items too, as MossTank's does for
/// the keep-up-to counts.
struct OwnedReading<'a>(Split<'a>);

impl IdentCall for OwnedReading<'_> {
    fn host(&mut self) -> &mut dyn mosstank_mag::host::MagHost {
        self.0.host()
    }

    fn owned(&mut self) -> &[InventoryItem] {
        self.0.owned()
    }

    fn verdict(&mut self, item: &InventoryItem, properties: &ItemProperties) -> Option<Verdict> {
        let _held = self.owned().len();
        self.0.verdict(item, properties)
    }

    fn needs_identify(&mut self, item: &InventoryItem, properties: &ItemProperties) -> bool {
        self.0.needs_identify(item, properties)
    }
}

#[test]
fn an_owned_identify_lists_the_owned_items_once() {
    let mut rig = Rig::new();
    rig.own(&[item(RING, "Test Ring", ObjectClass::Jewelry, 1)]);
    rig.verdict.0.insert(RING, verdict(true, "Rares", false));
    rig.select(RING, 0.0);
    let mut seen = object(RING, "Test Ring", ObjectClass::Jewelry);
    seen.is_owned = true;
    let mut call = OwnedReading(Split::new(&mut rig.host, &mut rig.verdict));
    rig.printer
        .on_ident(&mut call, &mut rig.queue, &seen, 1_000.0, settings());
    assert_eq!(rig.host.owned_listings, 1);
    assert_eq!(rig.posted_text(), vec!["+(Rares) Test Ring"]);
}

#[test]
fn a_corpse_item_goes_before_the_inventory_loggers_backlog() {
    // A first-run logger has 300 items waiting; the corpse item the line
    // waits on is sent next.
    let mut rig = Rig::new();
    for id in 0x6000_0000..0x6000_0000 + 300 {
        rig.queue.push(id);
    }
    let a = item(0x5100_0001, "Ring A", ObjectClass::Jewelry, CORPSE);
    rig.verdict
        .0
        .insert(a.object_id, verdict(true, "Rares", false));
    rig.verdict.1.insert(a.object_id);
    rig.open(CORPSE, "Corpse of Drudge", std::slice::from_ref(&a), 0.0);
    for seen in &mut rig.host.meta.objects {
        if seen.object_id == a.object_id {
            seen.has_appraisal_data = false;
        }
    }
    rig.ticks(0.0, 1_000.0);
    assert_eq!(rig.queue.len(), 301);
    rig.queue.tick(&mut rig.host, false);
    assert_eq!(rig.host.identified, vec![a.object_id]);
}

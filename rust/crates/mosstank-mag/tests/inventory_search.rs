//! The Inventory window's search: Mag-Tools' Tools, Inventory page,
//! widened to every character the inventory logger saved on this server.
//!
//! Source: Mag-Tools `Mag-Tools/Views/InventoryToolsView.cs` at 43ee2ec
//! (`InventorySearch_Change`: a .NET `Regex` with `IgnoreCase` over each
//! item's `ItemInfo.ToString()`, an invalid pattern lists nothing;
//! `InventoryList_Click`: `UseItem(wo.Container)` when the item is not
//! directly on the character, then `SelectItem`, then the item's text).

use std::collections::BTreeSet;

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::magic::SpellInfo;
use ac_plugin_abi::world::{
    ItemProperties, ObjectChange, ObjectChangeKind, ObjectClass, WorldObject,
};
use mosstank_mag::inventory_filter::{
    CantripTier, InventoryFilter, ItemType, Rating, Slot, cantrip_index,
};
use mosstank_mag::inventory_log::{ItemRecord, LogHeader, VERSION, storage_key, write};
use mosstank_mag::inventory_search::{
    ALL, FILTER_DELAY_MS, InfoSettings, InventorySearch, MATCHES_PER_CALL, SetCount,
};
use mosstank_mag::testing::FakeMagHost;

/// The fake's character (`FakeHost::new`).
const PLAYER: u32 = 1;
const ME: &str = "Meta Tester";
const WORLD: &str = "Coldeve";

const SIDE_PACK: u32 = 0x8000_0900;
const RING: u32 = 0x8000_0101;
const SWORD: u32 = 0x8000_0102;
const GEM: u32 = 0x8000_0103;

fn settings() -> InfoSettings {
    InfoSettings {
        show_buffed_values: true,
        show_value_and_burden: false,
    }
}

fn item(id: u32, name: &str, class: ObjectClass, container_id: u32) -> InventoryItem {
    InventoryItem {
        object_id: id,
        name: name.to_owned(),
        object_class: class,
        container_id,
        stack_size: 1,
        icon_id: 0x0600_1000 + (id & 0xFF),
        ..InventoryItem::default()
    }
}

/// A saved log of `character` holding `items`.
fn save(host: &FakeMagHost, character: &str, items: &[InventoryItem]) {
    let fragments: Vec<String> = items
        .iter()
        .map(|item| ItemRecord::from_live(item, None, &ItemProperties::default(), 0).to_json())
        .collect();
    let header = LogHeader {
        version: VERSION,
        character: character.to_owned(),
        world: WORLD.to_owned(),
        saved_unix_ms: 1_762_560_000_000,
    };
    let text = write(&header, fragments.iter().map(String::as_str));
    host.meta
        .storage
        .insert(&storage_key(WORLD, character), &text);
}

struct Rig {
    host: FakeMagHost,
    search: InventorySearch,
    shown: bool,
}

impl Rig {
    fn new() -> Self {
        Self {
            host: FakeMagHost::new(),
            search: InventorySearch::default(),
            shown: true,
        }
    }

    /// The logged-in character owns `item`.
    fn own(&mut self, item: InventoryItem) {
        self.host
            .meta
            .objects
            .push(ac_plugin_abi::world::WorldObject {
                object_id: item.object_id,
                name: item.name.clone(),
                object_class: item.object_class,
                container_id: item.container_id,
                is_owned: true,
                ..Default::default()
            });
        self.host
            .meta
            .properties
            .insert(item.object_id, ItemProperties::default());
        self.host.meta.owned_items.push(item);
    }

    fn tick(&mut self) -> bool {
        self.host.now_ms += 16.0;
        self.search.tick(&mut self.host, self.shown, settings())
    }

    /// Ticks until the search has nothing left to do.
    fn settle(&mut self) {
        for _ in 0..1000 {
            self.tick();
            if self.search.is_settled() {
                return;
            }
        }
        panic!("the search never settled: {}", self.search.status());
    }

    fn search(&mut self, pattern: &str) {
        self.search.set_pattern(pattern);
        self.settle();
    }

    fn names(&mut self) -> Vec<String> {
        self.search.row_names()
    }

    fn characters(&self) -> Vec<String> {
        self.search.row_characters()
    }
}

/// Every name in `rows` once, sorted, to compare without the order.
fn sorted(mut rows: Vec<String>) -> Vec<String> {
    rows.sort();
    rows
}

#[test]
fn a_pattern_matches_the_item_text_ignoring_case() {
    let mut rig = Rig::new();
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    rig.own(item(SWORD, "Sword", ObjectClass::MeleeWeapon, PLAYER));
    rig.search("ring");
    assert_eq!(rig.names(), ["Test Ring"]);
    rig.search("RING");
    assert_eq!(rig.names(), ["Test Ring"]);
}

/// The name column shows the item's material first, as its info line does
/// (the game sends "Acid Katar"; the material is a property of its own).
#[test]
fn a_name_shows_its_material_live_and_saved() {
    let mut katar = item(SWORD, "Acid Katar", ObjectClass::MeleeWeapon, PLAYER);
    katar.material_type = 58; // Bronze
    let mut rig = Rig::new();
    save(&rig.host, "Horan", std::slice::from_ref(&katar));
    rig.own(katar);
    rig.search("katar");
    assert_eq!(rig.names(), ["Bronze Acid Katar", "Bronze Acid Katar"]);
}

#[test]
fn an_empty_pattern_matches_everything() {
    let mut rig = Rig::new();
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    rig.own(item(SWORD, "Sword", ObjectClass::MeleeWeapon, PLAYER));
    rig.search("");
    assert_eq!(sorted(rig.names()), ["Sword", "Test Ring"]);
    assert_eq!(rig.search.status(), "2 items");
}

#[test]
fn a_slash_is_an_ordinary_character() {
    // A .NET pattern: no slashes around it, and `/` matches itself.
    let mut rig = Rig::new();
    rig.own(item(RING, "Ring of A/B", ObjectClass::Jewelry, PLAYER));
    rig.own(item(SWORD, "Sword", ObjectClass::MeleeWeapon, PLAYER));
    rig.search("a/b");
    assert_eq!(rig.names(), ["Ring of A/B"]);
    rig.search("/ring/");
    assert!(rig.names().is_empty());
}

#[test]
fn a_bad_pattern_lists_nothing_and_says_so() {
    let mut rig = Rig::new();
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    rig.search("(");
    assert!(rig.names().is_empty());
    assert_eq!(rig.search.status(), "bad pattern");
    // A good one again lists again.
    rig.search("Ring");
    assert_eq!(rig.names(), ["Test Ring"]);
}

#[test]
fn every_saved_character_is_searched_with_its_name() {
    let mut rig = Rig::new();
    save(
        &rig.host,
        "Alt One",
        &[item(0x100, "Alt Ring", ObjectClass::Jewelry, 0x50)],
    );
    save(
        &rig.host,
        "Alt Two",
        &[
            item(0x200, "Two Ring", ObjectClass::Jewelry, 0x60),
            item(0x201, "Two Gem", ObjectClass::Gem, 0x60),
        ],
    );
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    rig.search("ring");
    let rows: Vec<(String, String)> = rig.characters().into_iter().zip(rig.names()).collect();
    assert_eq!(
        sorted(rows.iter().map(|(c, n)| format!("{c}: {n}")).collect()),
        [
            "Alt One: Alt Ring",
            "Alt Two: Two Ring",
            "Meta Tester: Test Ring"
        ]
    );
    assert_eq!(
        rig.search.character_names(),
        [ME, "Alt One", "Alt Two"],
        "the logged-in character first, then the saved ones"
    );
}

#[test]
fn the_character_filter_keeps_one_characters_rows() {
    let mut rig = Rig::new();
    save(
        &rig.host,
        "Alt One",
        &[item(0x100, "Alt Ring", ObjectClass::Jewelry, 0x50)],
    );
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    rig.search("");
    rig.filter(InventoryFilter {
        characters: Some(BTreeSet::from(["Alt One".to_owned()])),
        ..InventoryFilter::default()
    });
    assert_eq!(rig.names(), ["Alt Ring"]);
    rig.filter(InventoryFilter::default());
    assert_eq!(rig.names().len(), 2);
}

#[test]
fn the_class_filter_keeps_one_kind_of_object() {
    let mut rig = Rig::new();
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    rig.own(item(SWORD, "Sword", ObjectClass::MeleeWeapon, PLAYER));
    rig.own(item(GEM, "Ruby", ObjectClass::Gem, PLAYER));
    rig.search("");
    assert_eq!(
        rig.search.classes(),
        [ALL, "Gem", "Jewelry", "MeleeWeapon"],
        "the classes present, by name"
    );
    rig.search.set_class_filter("MeleeWeapon");
    assert_eq!(rig.names(), ["Sword"]);
    // The pattern and the filter together.
    rig.search.set_class_filter(ALL);
    rig.search("r");
    assert_eq!(sorted(rig.names()), ["Ruby", "Sword", "Test Ring"]);
    rig.search.set_class_filter("Gem");
    assert_eq!(rig.names(), ["Ruby"]);
}

#[test]
fn the_where_column_names_the_pack_side_pack_or_worn() {
    let mut rig = Rig::new();
    let mut pack = item(SIDE_PACK, "Pack", ObjectClass::Container, PLAYER);
    pack.container_slot = 2;
    rig.own(pack);
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, SIDE_PACK));
    let mut sword = item(SWORD, "Sword", ObjectClass::MeleeWeapon, 0);
    sword.wielder_id = PLAYER;
    sword.equipped_location = 0x0010_0000;
    rig.own(sword);
    rig.own(item(GEM, "Ruby", ObjectClass::Gem, PLAYER));
    rig.search("");
    let places: Vec<String> = rig
        .names()
        .into_iter()
        .zip(rig.search.row_places())
        .map(|(name, place)| format!("{name}: {place}"))
        .collect();
    assert_eq!(
        sorted(places),
        [
            "Pack: Pack",
            "Ruby: Pack",
            "Sword: Worn",
            "Test Ring: Side pack 3"
        ]
    );
}

#[test]
fn the_count_column_is_the_stack_size() {
    let mut rig = Rig::new();
    let mut gems = item(GEM, "Ruby", ObjectClass::Gem, PLAYER);
    gems.stack_size = 25;
    rig.own(gems);
    rig.search("");
    assert_eq!(rig.search.row_counts(), ["25"]);
    assert_eq!(rig.search.row_icons(), [0x0600_1003]);
}

#[test]
fn clicking_a_side_pack_item_of_the_live_character_opens_the_pack_and_selects_it() {
    let mut rig = Rig::new();
    rig.own(item(SIDE_PACK, "Pack", ObjectClass::Container, PLAYER));
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, SIDE_PACK));
    rig.search("ring");
    assert_eq!(rig.names(), ["Test Ring"]);
    rig.search.click(&mut rig.host, 0, settings());
    assert_eq!(rig.host.meta.used_object, SIDE_PACK, "the side pack opened");
    assert_eq!(rig.host.meta.selection.selected, Some(RING));
    assert!(
        rig.search.info().contains("Test Ring"),
        "{}",
        rig.search.info()
    );
    assert_eq!(rig.search.selected_row(), 0);
}

#[test]
fn clicking_an_item_on_the_character_selects_it_without_using_anything() {
    let mut rig = Rig::new();
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    let mut sword = item(SWORD, "Sword", ObjectClass::MeleeWeapon, 0);
    sword.wielder_id = PLAYER;
    sword.equipped_location = 0x0010_0000;
    rig.own(sword);
    rig.search("");
    for row in 0..2 {
        rig.search.click(&mut rig.host, row, settings());
        assert_eq!(rig.host.meta.used_object, 0);
    }
    assert!(rig.host.meta.selection.selected.is_some());
}

#[test]
fn clicking_another_characters_row_shows_its_saved_text_and_touches_nothing() {
    let mut rig = Rig::new();
    save(
        &rig.host,
        "Alt One",
        &[item(0x100, "Alt Ring", ObjectClass::Jewelry, 0x50)],
    );
    rig.search("Alt");
    assert_eq!(rig.names(), ["Alt Ring"]);
    rig.search.click(&mut rig.host, 0, settings());
    assert_eq!(rig.search.info(), "Alt Ring");
    assert_eq!(rig.host.meta.used_object, 0);
    assert_eq!(rig.host.meta.selection.selected, None);
}

#[test]
fn the_live_characters_own_file_gives_way_to_its_live_rows() {
    let mut rig = Rig::new();
    save(
        &rig.host,
        ME,
        &[
            item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER),
            item(0x999, "Sold Long Ago", ObjectClass::Misc, PLAYER),
        ],
    );
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    rig.own(item(SWORD, "Sword", ObjectClass::MeleeWeapon, PLAYER));
    rig.search("");
    assert_eq!(sorted(rig.names()), ["Sword", "Test Ring"]);
    assert_eq!(rig.search.character_names(), [ME]);
}

#[test]
fn a_live_item_that_changes_is_shown_as_it_is_now() {
    let mut rig = Rig::new();
    rig.own(item(GEM, "Ruby", ObjectClass::Gem, PLAYER));
    rig.search("");
    assert_eq!(rig.search.row_counts(), ["1"]);
    rig.host.meta.owned_items[0].stack_size = 7;
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    for id in [GEM, RING] {
        rig.search.on_object_changed(&ObjectChange {
            object_id: id,
            kind: ObjectChangeKind::Updated,
            revision: 2,
            changed_fields: 0,
            current: rig
                .host
                .meta
                .objects
                .iter()
                .find(|o| o.object_id == id)
                .cloned(),
        });
    }
    rig.settle();
    let rows: Vec<String> = rig
        .names()
        .into_iter()
        .zip(rig.search.row_counts())
        .map(|(n, c)| format!("{n} {c}"))
        .collect();
    assert_eq!(sorted(rows), ["Ruby 7", "Test Ring 1"]);
    // Gone from the inventory: gone from the rows.
    rig.host.meta.owned_items.remove(0);
    rig.search.on_object_changed(&ObjectChange {
        object_id: GEM,
        kind: ObjectChangeKind::Released,
        revision: 3,
        changed_fields: 0,
        current: None,
    });
    rig.settle();
    assert_eq!(rig.names(), ["Test Ring"]);
}

#[test]
fn nothing_is_read_while_the_window_is_hidden() {
    let mut rig = Rig::new();
    save(
        &rig.host,
        "Alt One",
        &[item(0x100, "Alt Ring", ObjectClass::Jewelry, 0x50)],
    );
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    rig.shown = false;
    for _ in 0..20 {
        assert!(!rig.tick(), "no work while hidden");
    }
    assert!(rig.names().is_empty());
    rig.shown = true;
    rig.settle();
    assert_eq!(rig.names().len(), 2);
}

#[test]
fn opening_the_window_again_reads_the_files_again() {
    let mut rig = Rig::new();
    save(
        &rig.host,
        "Alt One",
        &[item(0x100, "Alt Ring", ObjectClass::Jewelry, 0x50)],
    );
    rig.search("");
    assert_eq!(rig.names(), ["Alt Ring"]);
    // Alt One logged in elsewhere and saved again; a new alt too.
    save(
        &rig.host,
        "Alt One",
        &[item(0x101, "Alt Sword", ObjectClass::MeleeWeapon, 0x50)],
    );
    save(
        &rig.host,
        "Alt Two",
        &[item(0x200, "Two Gem", ObjectClass::Gem, 0x60)],
    );
    rig.settle();
    assert_eq!(rig.names(), ["Alt Ring"], "not while it stays open");
    rig.shown = false;
    rig.tick();
    rig.shown = true;
    rig.settle();
    assert_eq!(sorted(rig.names()), ["Alt Sword", "Two Gem"]);
}

#[test]
fn a_large_file_is_read_over_several_ticks() {
    let mut rig = Rig::new();
    let items: Vec<InventoryItem> = (0..1500)
        .map(|n| item(0x1000 + n, "Pyreal", ObjectClass::Money, 0x50))
        .collect();
    save(&rig.host, "Alt One", &items);
    let mut ticks = 0;
    while !rig.search.is_settled() {
        rig.tick();
        ticks += 1;
        assert!(ticks < 1000);
    }
    assert!(ticks > 5, "read in {ticks} ticks");
    assert_eq!(rig.names().len(), 1500);
}

#[test]
fn a_new_pattern_over_many_items_is_matched_over_several_calls_and_says_so() {
    let mut rig = Rig::new();
    let items: Vec<InventoryItem> = (0..u32::try_from(MATCHES_PER_CALL * 4).unwrap())
        .map(|n| item(0x1000 + n, "Pyreal", ObjectClass::Money, 0x50))
        .collect();
    save(&rig.host, "Alt One", &items);
    rig.search("");
    assert_eq!(rig.names().len(), MATCHES_PER_CALL * 4);
    rig.search.set_pattern("pyr");
    assert!(!rig.search.is_settled());
    assert!(
        rig.search.status().starts_with("Searching"),
        "{}",
        rig.search.status()
    );
    rig.settle();
    assert_eq!(rig.names().len(), MATCHES_PER_CALL * 4);
    assert_eq!(
        rig.search.status(),
        format!("{} items", MATCHES_PER_CALL * 4)
    );
}

#[test]
fn the_same_pattern_again_changes_nothing() {
    let mut rig = Rig::new();
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    rig.search("ring");
    let revision = rig.search.revision();
    rig.search.set_pattern("ring");
    assert!(rig.search.is_settled());
    assert_eq!(rig.search.revision(), revision);
}

#[test]
fn the_selection_follows_its_item_when_the_rows_change() {
    let mut rig = Rig::new();
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    rig.own(item(SWORD, "Sword", ObjectClass::MeleeWeapon, PLAYER));
    rig.search("");
    let sword = rig.names().iter().position(|n| n == "Sword").unwrap();
    rig.search
        .click(&mut rig.host, i32::try_from(sword).unwrap(), settings());
    rig.search("sword");
    assert_eq!(rig.search.selected_row(), 0);
    rig.search("ring");
    assert_eq!(rig.search.selected_row(), -1, "not listed: no row selected");
    assert!(rig.search.info().contains("Sword"), "the text stays");
}

#[test]
fn an_empty_list_shows_no_items_text() {
    // Live: a bad pattern emptied the list and the clicked row's text stayed
    // under it.
    let mut rig = Rig::new();
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    for pattern in ["(", "nothing like it"] {
        rig.search("ring");
        assert_eq!(rig.names(), ["Test Ring"]);
        rig.search.click(&mut rig.host, 0, settings());
        assert!(rig.search.info().contains("Test Ring"));
        let revision = rig.search.revision();
        rig.search(pattern);
        assert!(rig.names().is_empty());
        assert_eq!(rig.search.info(), "", "{pattern}");
        assert_eq!(rig.search.selected_row(), -1);
        assert_ne!(rig.search.revision(), revision);
        // Found again: not selected, no text until it is clicked.
        rig.search("ring");
        assert_eq!(rig.search.selected_row(), -1);
        assert_eq!(rig.search.info(), "");
    }
}

#[test]
fn the_session_ending_forgets_everything() {
    let mut rig = Rig::new();
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    rig.search("ring");
    rig.search.reset();
    assert!(rig.names().is_empty());
    assert_eq!(rig.search.pattern(), "ring", "the pattern typed stays");
}

#[test]
fn a_pattern_that_backtracks_is_known_by_its_syntax() {
    use mosstank_mag::inventory_search::backtracks;
    for pattern in [
        "(?<!x)Pack",
        "(?=Ring)",
        "(?!a)b",
        "(?<=a)b",
        "(?>a+)",
        r"(a)\1",
        r"(?<n>a)\k<n>",
    ] {
        assert!(backtracks(pattern), "{pattern}");
    }
    for pattern in [
        "Ring",
        "Pack|Sack",
        "a(b|c)*d",
        r"\d+",
        "(?i)ring",
        "(?<name>a)",
        "(?:a)",
    ] {
        assert!(!backtracks(pattern), "{pattern}");
    }
}

#[test]
fn a_text_too_costly_to_match_is_counted_not_dropped_silently() {
    // A pattern that backtracks without end over a long text is stopped at
    // the step limit: the item is not listed, and the status says so.
    let mut rig = Rig::new();
    let words = "word ".repeat(40);
    rig.own(item(RING, words.trim(), ObjectClass::Jewelry, PLAYER));
    rig.own(item(SWORD, "Sword", ObjectClass::MeleeWeapon, PLAYER));
    rig.search(r"(?:\w+\s?)+Q(?!z)");
    assert!(rig.names().is_empty());
    assert_eq!(rig.search.status(), "0 items (1 too costly to search)");
    rig.search("Sword");
    assert_eq!(rig.search.status(), "1 item");
}

#[test]
fn an_item_gone_before_its_first_reading_is_not_listed() {
    let mut rig = Rig::new();
    for n in 0..200 {
        rig.own(item(0x9000 + n, "Pyreal", ObjectClass::Money, PLAYER));
    }
    // The first tick lists the items to read; the last one leaves before
    // it is read.
    rig.tick();
    let gone = rig.host.meta.owned_items.pop().unwrap().object_id;
    rig.search.on_object_changed(&ObjectChange {
        object_id: gone,
        kind: ObjectChangeKind::Released,
        revision: 2,
        changed_fields: 0,
        current: None,
    });
    rig.settle();
    assert_eq!(rig.names().len(), 199);
}

#[test]
fn a_click_hits_the_row_the_window_showed() {
    // The rows shown change between the push and the click: the click is
    // on the row the player saw.
    let mut rig = Rig::new();
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    rig.own(item(SWORD, "Sword", ObjectClass::MeleeWeapon, PLAYER));
    rig.search("");
    assert_eq!(rig.names(), ["Test Ring", "Sword"], "pushed");
    // The ring leaves; the rows are worked out again, not yet pushed.
    rig.host.meta.owned_items.remove(0);
    rig.search.on_object_changed(&ObjectChange {
        object_id: RING,
        kind: ObjectChangeKind::Released,
        revision: 2,
        changed_fields: 0,
        current: None,
    });
    rig.settle();
    rig.search.click(&mut rig.host, 0, settings());
    assert_ne!(
        rig.host.meta.selection.selected,
        Some(SWORD),
        "row 0 the player saw is the ring"
    );
}

#[test]
fn a_pattern_too_large_to_compile_lists_nothing_and_says_so() {
    // A short pattern that compiles huge is refused, distinct from a bad
    // one.
    let mut rig = Rig::new();
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    for pattern in [r"\w{1000}", r"\w{0,50}Ring", r"(?<!\w{0,400})Ring"] {
        rig.search(pattern);
        assert!(rig.names().is_empty(), "{pattern}");
        assert_eq!(rig.search.status(), "pattern too large", "{pattern}");
    }
    rig.search("Ring");
    assert_eq!(rig.names(), ["Test Ring"]);
}

// --- The filters (the filter window) ---

const STRENGTH: u32 = 6107; // Legendary Strength
const EPIC_STRENGTH: u32 = 4000;
const PROWESS: u32 = 2391;

fn spell(id: u32, name: &str) -> SpellInfo {
    SpellInfo {
        spell_id: id,
        name: name.to_owned(),
        ..SpellInfo::default()
    }
}

fn with_spells(host: &mut FakeMagHost) {
    for (id, name) in [
        (STRENGTH, "Legendary Strength"),
        (EPIC_STRENGTH, "Epic Strength"),
        (PROWESS, "Prowess"),
    ] {
        host.meta.spells.insert(id, spell(id, name));
    }
}

fn ints(ints: &[(u32, i32)]) -> ItemProperties {
    ItemProperties {
        ints: ints.to_vec(),
        ..ItemProperties::default()
    }
}

/// A saved log of `character` holding these appraised items.
fn save_appraised(host: &FakeMagHost, character: &str, items: &[(InventoryItem, ItemProperties)]) {
    let fragments: Vec<String> = items
        .iter()
        .map(|(item, properties)| {
            let seen = WorldObject {
                object_id: item.object_id,
                has_appraisal_data: true,
                ..WorldObject::default()
            };
            ItemRecord::from_live(item, Some(&seen), properties, 1).to_json()
        })
        .collect();
    let header = LogHeader {
        version: VERSION,
        character: character.to_owned(),
        world: WORLD.to_owned(),
        saved_unix_ms: 1_762_560_000_000,
    };
    let text = write(&header, fragments.iter().map(String::as_str));
    host.meta
        .storage
        .insert(&storage_key(WORLD, character), &text);
}

impl Rig {
    /// The logged-in character owns `item`, appraised, with `properties`.
    fn own_appraised(&mut self, item: InventoryItem, properties: ItemProperties) {
        let id = item.object_id;
        self.own(item);
        self.host.meta.properties.insert(id, properties);
        if let Some(object) = self
            .host
            .meta
            .objects
            .iter_mut()
            .find(|o| o.object_id == id)
        {
            object.has_appraisal_data = true;
        }
    }

    /// A filter set, and the search settled after its wait.
    fn filter(&mut self, filter: InventoryFilter) {
        self.search.set_filter(filter);
        self.settle();
    }
}

fn jewelry() -> InventoryFilter {
    InventoryFilter {
        item_type: ItemType::Jewelry,
        ..InventoryFilter::default()
    }
}

#[test]
fn the_filter_and_the_pattern_are_both_needed() {
    let mut rig = Rig::new();
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    rig.own_appraised(
        item(GEM, "Ring Mail", ObjectClass::Armor, PLAYER),
        ints(&[(28, 300)]),
    );
    rig.own(item(SWORD, "Sword", ObjectClass::MeleeWeapon, PLAYER));
    rig.search("ring");
    assert_eq!(sorted(rig.names()), ["Ring Mail", "Test Ring"]);
    rig.filter(jewelry());
    assert_eq!(rig.names(), ["Test Ring"]);
    assert_eq!(rig.search.filter_summary(), "Jewelry");
    rig.search("sword");
    assert!(
        rig.names().is_empty(),
        "the pattern and the filter together"
    );
    rig.search.clear_filter();
    rig.settle();
    assert_eq!(rig.names(), ["Sword"]);
    assert_eq!(rig.search.filter_summary(), "");
}

#[test]
fn a_filter_applies_0_4_s_after_its_last_change() {
    let mut rig = Rig::new();
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    rig.own(item(SWORD, "Sword", ObjectClass::MeleeWeapon, PLAYER));
    rig.search("");
    rig.search.set_filter(jewelry());
    // The window shows the filter as set at once.
    assert_eq!(rig.search.filter().item_type, ItemType::Jewelry);
    assert_eq!(rig.search.filter_summary(), "Jewelry");
    // 16 ms a tick: 20 ticks is 320 ms.
    for _ in 0..20 {
        rig.tick();
    }
    assert_eq!(rig.names().len(), 2, "not yet");
    assert!(rig.search.status().starts_with("Searching"));
    // Another change starts the wait again.
    let weapons = InventoryFilter {
        item_type: ItemType::Weapons,
        ..InventoryFilter::default()
    };
    rig.search.set_filter(weapons);
    for _ in 0..20 {
        rig.tick();
    }
    assert_eq!(rig.names().len(), 2, "320 ms after the last change");
    let mut ticks = 0;
    while !rig.search.is_settled() {
        rig.tick();
        ticks += 1;
        assert!(ticks < 100);
    }
    assert_eq!(rig.names(), ["Sword"]);
    assert!(f64::from(20 + ticks) * 16.0 >= FILTER_DELAY_MS);
    assert_eq!(rig.search.status(), "1 item");
}

#[test]
fn cantrips_and_slots_read_live_and_saved_items_alike() {
    let mut rig = Rig::new();
    with_spells(&mut rig.host);
    let mut ring = item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER);
    ring.valid_locations = 0x000C_0000;
    ring.appraised_spell_ids = vec![STRENGTH];
    let mut alt_ring = item(0x100, "Alt Ring", ObjectClass::Jewelry, 0x50);
    alt_ring.valid_locations = 0x000C_0000;
    alt_ring.appraised_spell_ids = vec![0x8000_0000 | EPIC_STRENGTH];
    let mut alt_amulet = item(0x101, "Alt Amulet", ObjectClass::Jewelry, 0x50);
    alt_amulet.valid_locations = 0x8000;
    alt_amulet.appraised_spell_ids = vec![PROWESS];
    save_appraised(
        &rig.host,
        "Alt One",
        &[
            (alt_ring, ItemProperties::default()),
            (alt_amulet, ItemProperties::default()),
        ],
    );
    rig.own_appraised(ring, ItemProperties::default());
    rig.search("");
    let strength = cantrip_index("Strength").unwrap();
    rig.filter(InventoryFilter {
        cantrips: vec![(strength, CantripTier::Epic)],
        ..InventoryFilter::default()
    });
    assert_eq!(sorted(rig.names()), ["Alt Ring", "Test Ring"]);
    rig.filter(InventoryFilter {
        cantrips: vec![(strength, CantripTier::Legendary)],
        ..InventoryFilter::default()
    });
    assert_eq!(rig.names(), ["Test Ring"]);
    rig.filter(InventoryFilter {
        slots: BTreeSet::from([Slot::Ring]),
        ..InventoryFilter::default()
    });
    assert_eq!(sorted(rig.names()), ["Alt Ring", "Test Ring"]);
    rig.filter(InventoryFilter {
        spell_contains: "prow".to_owned(),
        ..InventoryFilter::default()
    });
    assert_eq!(rig.names(), ["Alt Amulet"]);
}

#[test]
fn each_spell_is_looked_up_once_whatever_the_rows_holding_it() {
    let mut rig = Rig::new();
    with_spells(&mut rig.host);
    for n in 0..5 {
        let mut ring = item(RING + 0x10 * n, "Ring", ObjectClass::Jewelry, PLAYER);
        ring.appraised_spell_ids = vec![STRENGTH, PROWESS];
        rig.own_appraised(ring, ItemProperties::default());
    }
    rig.search("");
    assert_eq!(rig.host.spell_lookups, 2);
    assert_eq!(rig.names().len(), 5);
}

#[test]
fn items_never_appraised_left_out_by_an_appraisal_filter_are_counted() {
    let mut rig = Rig::new();
    rig.own(item(RING, "Plain Ring", ObjectClass::Jewelry, PLAYER));
    rig.own(item(SWORD, "Sword", ObjectClass::MeleeWeapon, PLAYER));
    rig.own_appraised(
        item(GEM, "Crit Ring", ObjectClass::Jewelry, PLAYER),
        ints(&[(374, 2)]),
    );
    rig.search("");
    let mut filter = InventoryFilter::default();
    filter.set_rating(Rating::CritDamage, Some(1));
    rig.filter(filter.clone());
    assert_eq!(rig.names(), ["Crit Ring"]);
    assert_eq!(rig.search.status(), "1 item (2 not identified)");
    // Left out by another filter first: not counted.
    filter.item_type = ItemType::Jewelry;
    rig.filter(filter);
    assert_eq!(rig.search.status(), "1 item (1 not identified)");
}

#[test]
fn the_sets_are_counted_over_the_characters_searched() {
    let mut rig = Rig::new();
    save_appraised(
        &rig.host,
        "Alt One",
        &[(
            item(0x100, "Alt Coat", ObjectClass::Armor, 0x50),
            ints(&[(265, 13)]),
        )],
    );
    rig.own_appraised(
        item(RING, "Coat", ObjectClass::Armor, PLAYER),
        ints(&[(265, 13)]),
    );
    rig.own_appraised(
        item(SWORD, "Helm", ObjectClass::Armor, PLAYER),
        ints(&[(265, 13)]),
    );
    rig.own_appraised(
        item(GEM, "Robe", ObjectClass::Armor, PLAYER),
        ints(&[(265, 14)]),
    );
    rig.search("");
    let count = |id: i32, name: &str, count: usize| SetCount {
        id,
        name: name.to_owned(),
        count,
    };
    assert_eq!(
        rig.search.set_counts(),
        [count(13, "Soldier's Set", 3), count(14, "Adept's Set", 1)]
    );
    rig.filter(InventoryFilter {
        characters: Some(BTreeSet::from([ME.to_owned()])),
        ..InventoryFilter::default()
    });
    assert_eq!(
        rig.search.set_counts(),
        [count(13, "Soldier's Set", 2), count(14, "Adept's Set", 1)]
    );
    // The counts ignore the other filters (as the website's).
    rig.filter(InventoryFilter {
        set: Some(14),
        ..InventoryFilter::default()
    });
    assert_eq!(rig.names(), ["Robe"]);
    assert_eq!(rig.search.set_counts().len(), 2);
}

#[test]
fn the_characters_are_chosen_in_the_filter() {
    let mut rig = Rig::new();
    save(
        &rig.host,
        "Alt One",
        &[item(0x100, "Alt Ring", ObjectClass::Jewelry, 0x50)],
    );
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    rig.search("");
    assert_eq!(rig.search.character_names(), [ME, "Alt One"]);
    rig.filter(InventoryFilter {
        characters: Some(BTreeSet::from(["Alt One".to_owned()])),
        ..InventoryFilter::default()
    });
    assert_eq!(rig.names(), ["Alt Ring"]);
    rig.filter(InventoryFilter {
        characters: Some(BTreeSet::new()),
        ..InventoryFilter::default()
    });
    assert!(rig.names().is_empty(), "none chosen: none shown");
    rig.filter(InventoryFilter::default());
    assert_eq!(rig.names().len(), 2);
    assert_eq!(rig.search.filter().characters, None);
}

#[test]
fn the_slot_wield_and_value_columns_and_their_sort_keys() {
    let mut rig = Rig::new();
    let mut ring = item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER);
    ring.valid_locations = 0x000C_0000;
    rig.own_appraised(ring, ints(&[(158, 7), (159, 1), (160, 180), (19, 7_500)]));
    rig.own(item(GEM, "Ruby", ObjectClass::Gem, PLAYER));
    rig.search("ring");
    assert_eq!(rig.search.row_slots(), ["Ring"]);
    assert_eq!(rig.search.row_wields(), ["180"]);
    assert_eq!(rig.search.row_values(), ["7,500"]);
    assert_eq!(rig.search.row_wield_keys(), [180.0]);
    assert_eq!(rig.search.row_value_keys(), [7_500.0]);
    rig.search("ruby");
    assert_eq!(rig.search.row_slots(), ["-"]);
    assert_eq!(rig.search.row_wields(), [""]);
    assert_eq!(rig.search.row_values(), [""]);
    assert_eq!(rig.search.row_wield_keys(), [-1.0]);
    assert_eq!(rig.search.row_value_keys(), [-1.0]);
}

#[test]
fn the_filter_stays_when_the_session_ends() {
    let mut rig = Rig::new();
    rig.own(item(RING, "Test Ring", ObjectClass::Jewelry, PLAYER));
    rig.search("");
    rig.filter(jewelry());
    rig.search.reset();
    assert_eq!(rig.search.filter(), &jewelry());
}

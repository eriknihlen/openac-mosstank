//! The Inventory window's search: Mag-Tools' Tools, Inventory page,
//! widened to every character the inventory logger saved on this server.
//!
//! Source: Mag-Tools `Mag-Tools/Views/InventoryToolsView.cs` at 43ee2ec
//! (`InventorySearch_Change`: a .NET `Regex` with `IgnoreCase` over each
//! item's `ItemInfo.ToString()`, an invalid pattern lists nothing;
//! `InventoryList_Click`: `UseItem(wo.Container)` when the item is not
//! directly on the character, then `SelectItem`, then the item's text).

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::{ItemProperties, ObjectChange, ObjectChangeKind, ObjectClass};
use mosstank_mag::inventory_log::{ItemRecord, LogHeader, VERSION, storage_key, write};
use mosstank_mag::inventory_search::{ALL, InfoSettings, InventorySearch, MATCHES_PER_CALL};
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

    fn names(&self) -> Vec<String> {
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
        rig.search.characters(),
        [ALL, ME, "Alt One", "Alt Two"],
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
    rig.search.set_character_filter("Alt One");
    assert_eq!(rig.names(), ["Alt Ring"]);
    rig.search.set_character_filter(ALL);
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
    assert_eq!(rig.search.characters(), [ALL, ME]);
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

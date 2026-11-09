//! The Mag tab's view of the client ([`MagHost`]) over the SDK host: each
//! call the one SDK call it stands for.

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::character::ActiveEnchantment;
use ac_plugin_abi::world::ItemProperties;
use common::{World, item};
use mosstank::automation::HostAutomation;
use mosstank_mag::host::MagHost;

fn mag<R>(world: &mut World, f: impl FnOnce(&mut dyn MagHost) -> R) -> R {
    world.with_host(|host| f(&mut HostAutomation::new(host)))
}

#[test]
fn the_owned_inventory_is_complete_when_the_client_says_so() {
    let mut world = World::with_character("Mag");
    world.set_reply::<calls::ItemsInventoryComplete>(&true);
    assert!(mag(&mut world, |host| host.owned_inventory_complete()));
    world.set_reply::<calls::ItemsInventoryComplete>(&false);
    assert!(!mag(&mut world, |host| host.owned_inventory_complete()));
}

#[test]
fn an_owned_items_properties_are_the_clients() {
    let mut world = World::with_character("Mag");
    let mut properties = ItemProperties::default();
    properties.ints.push((131, 7));
    world.set_reply::<calls::ItemsProperties>(&Some(properties.clone()));
    assert_eq!(
        mag(&mut world, |host| host.item_properties(0x5000_0001)),
        Some(properties)
    );
    assert_eq!(
        world.requests::<calls::ItemsProperties>(),
        vec![0x5000_0001]
    );
}

#[test]
fn the_open_containers_contents_are_the_clients() {
    let mut world = World::with_character("Mag");
    let ring = item(0x5000_0002, "Ring", 0x8);
    world.set_reply::<calls::LootContents>(&vec![ring.clone()]);
    assert_eq!(mag(&mut world, |host| host.loot_contents()), vec![ring]);
}

#[test]
fn every_enchantment_is_the_clients_every_enchantment_call() {
    // Plugin API 1.10: every enchantment on the character, surpassed ones
    // too, not the ones in effect.
    let mut world = World::with_character("Mag");
    let buff = ActiveEnchantment {
        spell_id: 2,
        family: 5,
        tier: 7,
        seconds_remaining: 3600.0,
    };
    world.set_reply::<calls::CharacterAllEnchantments>(&vec![buff.clone()]);
    assert_eq!(mag(&mut world, |host| host.all_enchantments()), vec![buff]);
    assert_eq!(world.requests::<calls::CharacterAllEnchantments>().len(), 1);
    assert!(world.requests::<calls::CharacterEnchantments>().is_empty());
}

#[test]
fn the_clock_is_the_sessions_monotonic_clock() {
    let mut world = World::with_character("Mag");
    world.fake.clock.monotonic_ms = 12_345;
    assert!((mag(&mut world, |host| host.now_ms()) - 12_345.0).abs() < f64::EPSILON);
}

#[test]
fn the_identify_queue_is_forgotten_when_the_session_ends() {
    let mut world = World::with_character("Mag");
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    world.with_tank(&mut plugin, |tank, _| tank.mag().identify.push(7));
    world.tick(&mut plugin);
    assert!(world.with_tank(&mut plugin, |tank, _| tank.mag().identify.contains(7)));
    world.fake.character = None;
    world.tick(&mut plugin);
    assert!(world.with_tank(&mut plugin, |tank, _| tank.mag().identify.is_empty()));
}

/// A loot profile `Mag` with one rule keeping everything, loaded.
fn keep_everything(world: &mut World, plugin: &mut mosstank::MossTank) {
    use mosstank_formats::utl::{self, LootAction, LootRule};
    let mut rule = LootRule::named("Rings", "*");
    rule.action = LootAction::Keep;
    world
        .profiles
        .insert("mosstank/loot/Mag.utl", &utl::serialize_rules(&[rule]));
    world.vt(plugin, "loot load Mag");
    world.ticks(plugin, 5);
    assert_eq!(plugin.tank().unwrap().live_loot_rules().len(), 1);
}

/// An appraisal of an owned object arrived.
fn ident_received(object_id: u32, name: &str) -> ac_plugin_abi::Event {
    use ac_plugin_abi::world::{ObjectChange, ObjectChangeKind, WorldObject};
    ac_plugin_abi::Event::ObjectChanged(ObjectChange {
        object_id,
        kind: ObjectChangeKind::IdentReceived,
        revision: 2,
        changed_fields: 0,
        current: Some(WorldObject {
            object_id,
            name: name.to_owned(),
            is_owned: true,
            has_appraisal_data: true,
            ..WorldObject::default()
        }),
    })
}

#[test]
fn an_owned_item_the_player_selects_and_identifies_prints_its_info_line() {
    // Source: Mag-Tools DetectItemsIdentifiedByUser.cs and
    // ItemInfoPrinter.cs at 43ee2ec: selected, then appraised, the line
    // shows the profile's verdict, once, in text type 14.
    use ac_plugin_abi::world::SelectionChange;
    let ring = 0x5000_0001;
    let mut world = World::with_character("Mag");
    world.own_items(vec![item(ring, "Test Ring", 0x8)]);
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    keep_everything(&mut world, &mut plugin);
    world.clear_chat();

    world.send(
        &mut plugin,
        ac_plugin_abi::Event::SelectionChanged(SelectionChange {
            previous: None,
            selected: Some(ring),
        }),
    );
    world.send(&mut plugin, ident_received(ring, "Test Ring"));
    world.send(&mut plugin, ident_received(ring, "Test Ring"));

    let lines: Vec<(String, u32)> = world
        .fake
        .posted
        .iter()
        .map(|post| (post.text.clone(), post.log_text_type))
        .collect();
    assert_eq!(lines, vec![("+(Rings) Test Ring".to_owned(), 14)]);
}

fn selected(object_id: u32) -> ac_plugin_abi::Event {
    ac_plugin_abi::Event::SelectionChanged(ac_plugin_abi::world::SelectionChange {
        previous: None,
        selected: Some(object_id),
    })
}

#[test]
fn an_identify_of_another_object_than_the_one_selected_prints_nothing() {
    let ring = 0x5000_0001;
    let other = 0x5000_0002;
    let mut world = World::with_character("Mag");
    world.own_items(vec![
        item(ring, "Test Ring", 0x8),
        item(other, "Other", 0x8),
    ]);
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    world.clear_chat();
    world.send(&mut plugin, selected(other));
    world.send(&mut plugin, ident_received(ring, "Test Ring"));
    assert!(world.chat().is_empty(), "{:?}", world.chat());
    // The selected one still prints (the empty default profile keeps
    // nothing).
    world.send(&mut plugin, ident_received(other, "Other"));
    assert_eq!(world.chat(), vec!["- Other"]);
}

const CORPSE: u32 = 0x7000_0001;
const KEPT: u32 = 0x5100_0001;
const UNAPPRAISED: u32 = 0x5100_0002;

/// A profile whose first rule (keep, `value >= 0`) stays open until an
/// item is appraised, before a catch-all that keeps nothing: an
/// unappraised item needs its appraisal first.
fn rings_then_junk(world: &mut World, plugin: &mut mosstank::MossTank) {
    use mosstank_formats::utl::{self, LootAction, LootRule};
    let mut rings = LootRule::named("Rings", "value >= 0");
    rings.action = LootAction::Keep;
    let mut junk = LootRule::named("Junk", "*");
    junk.action = LootAction::NoLoot;
    world.profiles.insert(
        "mosstank/loot/MagRings.utl",
        &utl::serialize_rules(&[rings, junk]),
    );
    world.vt(plugin, "loot load MagRings");
    world.ticks(plugin, 5);
    assert_eq!(plugin.tank().unwrap().live_loot_rules().len(), 2);
}

/// `name` open, holding one appraised ring and one not.
fn open_container(world: &mut World, plugin: &mut mosstank::MossTank, name: &str) {
    use ac_plugin_abi::world::{ObjectClass, WorldObject};
    let seen = |object_id, name: &str, appraised| WorldObject {
        object_id,
        name: name.to_owned(),
        container_id: CORPSE,
        has_appraisal_data: appraised,
        ..WorldObject::default()
    };
    world.objects = Some(vec![
        WorldObject {
            object_id: CORPSE,
            name: name.to_owned(),
            object_class: ObjectClass::Corpse,
            ..WorldObject::default()
        },
        seen(KEPT, "Ring A", true),
        seen(UNAPPRAISED, "Ring B", false),
    ]);
    let contents: Vec<_> = [(KEPT, "Ring A"), (UNAPPRAISED, "Ring B")]
        .into_iter()
        .map(|(id, ring)| {
            let mut ring = item(id, ring, 0x8);
            ring.container_id = CORPSE;
            ring
        })
        .collect();
    world.set_reply::<calls::LootContents>(&contents);
    world.set_reply::<calls::WorldOpenContainer>(&CORPSE);
    // The appraisal slot stays taken, so the queue keeps the item.
    world.set_reply::<calls::WorldIdentify>(&ac_plugin_abi::items::ItemResult::of(
        ac_plugin_abi::items::ItemStatus::Busy,
    ));
    world.send(plugin, ac_plugin_abi::Event::ContainerOpened(CORPSE));
    for _ in 0..7 {
        world.fake.clock.monotonic_ms += 100;
        world.tick(plugin);
    }
}

fn lines(world: &World) -> Vec<(String, u32)> {
    world
        .fake
        .posted
        .iter()
        .map(|post| (post.text.clone(), post.log_text_type))
        .collect()
}

#[test]
fn an_opened_corpse_prints_its_kept_items_and_waits_for_the_appraisals_it_needs() {
    // Source: Mag-Tools DetectItemsInContainers.cs, ItemInfoPrinter.cs and
    // VirindiTankLootRuleProcessor.cs at 43ee2ec.
    let mut world = World::with_character("Mag");
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    rings_then_junk(&mut world, &mut plugin);
    world.clear_chat();

    open_container(&mut world, &mut plugin, "Corpse of Drudge");
    assert_eq!(lines(&world), vec![("+(Rings) Ring A".to_owned(), 14)]);
    assert!(world.with_tank(&mut plugin, |tank, _| {
        tank.mag().identify.contains(UNAPPRAISED)
    }));
    assert!(
        world
            .requests::<calls::WorldIdentify>()
            .contains(&UNAPPRAISED)
    );

    world.send(&mut plugin, ident_received(UNAPPRAISED, "Ring B"));
    assert_eq!(
        lines(&world),
        vec![
            ("+(Rings) Ring A".to_owned(), 14),
            ("+(Rings) Ring B".to_owned(), 14),
        ]
    );
}

#[test]
fn an_opened_housing_storage_prints_nothing() {
    let mut world = World::with_character("Mag");
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    rings_then_junk(&mut world, &mut plugin);
    world.clear_chat();
    open_container(&mut world, &mut plugin, "Storage");
    assert!(world.chat().is_empty(), "{:?}", world.chat());
    assert!(world.with_tank(&mut plugin, |tank, _| tank.mag().identify.is_empty()));
}

#[test]
fn the_inventory_is_logged_after_the_login_and_again_at_logoff() {
    let mut world = World::with_character("Mag");
    world.set_reply::<calls::ItemsInventoryComplete>(&true);
    let ring = item(0x5000_0002, "Ring", 0x8);
    world.set_reply::<calls::ItemsOwned>(&vec![ring]);
    let mut plugin = world.enable();
    // The shell's own caches are built first (the Mag work waits for the
    // ticks they leave light).
    world.ticks(&mut plugin, 60);
    let key = "Mag-Tools/Coldeve/Mag.inventory.json";
    assert!(world.plugin_data.get(key).is_none());
    world.send(&mut plugin, ac_plugin_abi::Event::LoginComplete);
    world.tick(&mut plugin);
    let text = world.plugin_data.get(key).expect("the log").to_owned();
    let (header, records) = mosstank_mag::inventory_log::read(&text).expect("it reads");
    assert_eq!(
        (header.character.as_str(), header.world.as_str()),
        ("Mag", "Coldeve")
    );
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].name, "Ring");
    // Logging off writes it again, with the time of the logoff.
    world.fake.clock.utc_unix_ms += 1000;
    world.send(&mut plugin, ac_plugin_abi::Event::Logoff);
    let text = world.plugin_data.get(key).expect("the log").to_owned();
    let (logged_off, _) = mosstank_mag::inventory_log::read(&text).expect("it reads");
    assert_eq!(logged_off.saved_unix_ms, header.saved_unix_ms + 1000);
}

/// A saved inventory log of `character` on Coldeve holding `items`.
fn save_log(world: &mut World, character: &str, items: &[ac_plugin_abi::items::InventoryItem]) {
    use mosstank_mag::inventory_log::{ItemRecord, LogHeader, VERSION, storage_key, write};
    let fragments: Vec<String> = items
        .iter()
        .map(|item| ItemRecord::from_live(item, None, &ItemProperties::default(), 0).to_json())
        .collect();
    let header = LogHeader {
        version: VERSION,
        character: character.to_owned(),
        world: "Coldeve".to_owned(),
        saved_unix_ms: 1_762_560_000_000,
    };
    world.plugin_data.insert(
        &storage_key("Coldeve", character),
        &write(&header, fragments.iter().map(String::as_str)),
    );
}

#[test]
fn the_inventory_window_searches_the_live_character_and_the_saved_ones() {
    let mut world = World::with_character("Mag");
    world.set_reply::<calls::ItemsInventoryComplete>(&true);
    world.own_items(vec![item(0x5000_0002, "Test Ring", 0x8)]);
    save_log(&mut world, "Alt", &[item(0x5000_0100, "Alt Sword", 0x1)]);
    let mut plugin = world.enable();
    // The shell's own caches are built and the combat tracker's patterns
    // compiled first (the search waits for the ticks they leave light).
    world.ticks(&mut plugin, 60);
    // Closed: nothing read.
    assert!(!world.ui_bool(&mut plugin, "MagInventoryShown"));
    assert!(
        world
            .ui_list(&mut plugin, "MagInventoryRowNames")
            .is_empty()
    );

    world.act(&mut plugin, "OpenMagInventory");
    world.ticks(&mut plugin, 10);
    assert!(world.ui_bool(&mut plugin, "MagInventoryShown"));
    let mut names = world.ui_list(&mut plugin, "MagInventoryRowNames");
    names.sort();
    assert_eq!(names, ["Alt Sword", "Test Ring"]);
    assert_eq!(
        world.ui_list(&mut plugin, "MagFilterCharacterNames"),
        ["Mag", "Alt"]
    );
    assert_eq!(world.ui_text(&mut plugin, "MagInventoryStatus"), "2 items");

    // Typed: compiled at once, matched on the next ticks.
    world.act_text(&mut plugin, "SetMagInventorySearchText", "SWORD");
    assert!(
        world
            .ui_list(&mut plugin, "MagInventoryRowNames")
            .is_empty()
    );
    world.ticks(&mut plugin, 3);
    assert_eq!(
        world.ui_list(&mut plugin, "MagInventoryRowNames"),
        ["Alt Sword"]
    );
    assert_eq!(
        world.ui_list(&mut plugin, "MagInventoryRowCharacters"),
        ["Alt"]
    );
    world.act_int(&mut plugin, "SelectMagInventoryRow", 0);
    let info = world.ui_text(&mut plugin, "MagInventoryInfo");
    assert!(info.starts_with("Alt Sword"), "{info}");

    world.act_text(&mut plugin, "SetMagInventorySearchText", "(");
    assert_eq!(
        world.ui_text(&mut plugin, "MagInventoryStatus"),
        "bad pattern"
    );
    assert!(
        world
            .ui_list(&mut plugin, "MagInventoryRowNames")
            .is_empty()
    );
}

// The Inventory window's Filters... button opens the filters window; a
// cantrip checked at Epic on its Cantrips tab narrows the rows 0.4 s
// later to the items with that cantrip at Epic or better; Clear empties
// the filter again.
#[test]
fn the_filters_window_narrows_the_inventory_and_clear_empties_it() {
    let mut world = World::with_character("Mag");
    world.set_reply::<calls::ItemsInventoryComplete>(&true);
    let mut ring = item(0x5000_0002, "Strength Ring", 0x8);
    ring.appraised_spell_ids = vec![6107];
    world.own_items(vec![ring, item(0x5000_0003, "Plain Ring", 0x8)]);
    world.set_reply::<calls::SpellsGet>(&Some(ac_plugin_abi::magic::SpellInfo {
        spell_id: 6107,
        name: "Legendary Strength".to_owned(),
        ..ac_plugin_abi::magic::SpellInfo::default()
    }));
    let mut plugin = world.enable();
    world.ticks(&mut plugin, 60);
    world.act(&mut plugin, "OpenMagInventory");
    world.ticks(&mut plugin, 10);
    let names = |world: &mut World, plugin: &mut mosstank::MossTank| {
        let mut names = world.ui_list(plugin, "MagInventoryRowNames");
        names.sort();
        names
    };
    assert_eq!(
        names(&mut world, &mut plugin),
        ["Plain Ring", "Strength Ring"]
    );
    assert_eq!(
        world.ui_text(&mut plugin, "MagInventoryFilterSummary"),
        "No filters"
    );
    // The number columns' sort keys, as text the list parses: neither ring
    // needs a level or has a value (-1, below every one).
    assert_eq!(
        world.ui_list(&mut plugin, "MagInventoryRowWieldKeys"),
        ["-1", "-1"]
    );
    assert_eq!(
        world.ui_list(&mut plugin, "MagInventoryRowWields"),
        ["", ""]
    );

    assert!(!world.ui_bool(&mut plugin, "MagFiltersShown"));
    world.act(&mut plugin, "OpenMagInventoryFilters");
    assert!(world.ui_bool(&mut plugin, "MagFiltersShown"));
    assert!(world.ui_bool(&mut plugin, "MagFiltersItemVisible"));
    world.act(&mut plugin, "ShowMagFiltersCantrips");
    assert!(world.ui_bool(&mut plugin, "MagFiltersCantripsVisible"));
    assert!(!world.ui_bool(&mut plugin, "MagFiltersItemVisible"));
    assert_eq!(
        world.ui_list(&mut plugin, "MagFilterCantripNames")[0],
        "Strength"
    );
    // Strength's tier stepped from Legendary to Epic, then checked.
    for _ in 0..3 {
        world.act_int(&mut plugin, "CycleMagFilterCantripTier", 0);
    }
    assert_eq!(
        world.ui_list(&mut plugin, "MagFilterCantripTiers")[0],
        "Epic"
    );
    world.act_int(&mut plugin, "ToggleMagFilterCantrip", 0);
    assert!(world.ui_bools(&mut plugin, "MagFilterCantripChecks")[0]);
    assert_eq!(
        world.ui_text(&mut plugin, "MagInventoryFilterSummary"),
        "Epic Strength"
    );
    // The rows follow 0.4 s after the last change (frames 0.1 s apart).
    let frames = |world: &mut World, plugin: &mut mosstank::MossTank, count: usize| {
        for _ in 0..count {
            world.fake.clock.monotonic_ms += 100;
            world.tick(plugin);
        }
    };
    frames(&mut world, &mut plugin, 3);
    assert_eq!(names(&mut world, &mut plugin).len(), 2);
    frames(&mut world, &mut plugin, 10);
    assert_eq!(names(&mut world, &mut plugin), ["Strength Ring"]);

    world.act(&mut plugin, "ClearMagInventoryFilters");
    assert_eq!(
        world.ui_text(&mut plugin, "MagInventoryFilterSummary"),
        "No filters"
    );
    assert!(!world.ui_bools(&mut plugin, "MagFilterCantripChecks")[0]);
    frames(&mut world, &mut plugin, 13);
    assert_eq!(
        names(&mut world, &mut plugin),
        ["Plain Ring", "Strength Ring"]
    );
}

#[test]
fn the_mana_page_lists_a_worn_item_once_appraised() {
    // Source: Mag-Tools EquipmentTrackedItem.cs and ManaTrackerGUI.cs at
    // 43ee2ec: an appraised worn ring with its spell up, 100 of 200 mana,
    // one burned every 40 s.
    use ac_plugin_abi::character::ActiveEnchantment;
    use ac_plugin_abi::magic::SpellInfo;
    let ring = 0x5000_0002;
    let mut world = World::with_character("Mag");
    world.set_reply::<calls::ItemsInventoryComplete>(&true);
    world.own_items(vec![ac_plugin_abi::items::InventoryItem {
        wielder_id: 0x5000_0001,
        valid_locations: 0x0004_0000,
        equipped_location: 0x0004_0000,
        item_current_mana: 100,
        item_maximum_mana: 200,
        ..item(ring, "Test Ring", 0x8)
    }]);
    world.set_reply::<calls::WorldObject>(&Some(ac_plugin_abi::world::WorldObject {
        object_id: ring,
        name: "Test Ring".to_owned(),
        wielder_id: 0x5000_0001,
        is_owned: true,
        has_appraisal_data: true,
        spell_ids: vec![100],
        ..ac_plugin_abi::world::WorldObject::default()
    }));
    let mut properties = ItemProperties::default();
    properties.floats.push((5, -0.025));
    world.set_reply::<calls::ItemsProperties>(&Some(properties));
    world.set_reply::<calls::SpellsGet>(&Some(SpellInfo {
        spell_id: 100,
        family: 5,
        difficulty: 100,
        is_beneficial: true,
        ..SpellInfo::default()
    }));
    world.set_reply::<calls::CharacterAllEnchantments>(&vec![ActiveEnchantment {
        spell_id: 100,
        family: 5,
        tier: 1,
        seconds_remaining: f64::INFINITY,
    }]);
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    world.send(&mut plugin, ac_plugin_abi::Event::LoginComplete);
    world.ticks(&mut plugin, 3);
    // Tracked at the login: its appraisal asked for.
    assert!(world.requests::<calls::WorldIdentify>().contains(&ring));
    world.fake.clock.monotonic_ms = 1_000;
    world.send(&mut plugin, ident_received(ring, "Test Ring"));

    world.act(&mut plugin, "ShowMag");
    world.act(&mut plugin, "ShowMagMana");
    assert!(world.ui_bool(&mut plugin, "MagManaVisible"));
    // The page is built once a frame: reading its values asks the client
    // nothing.
    let clock_reads = world.requests::<calls::HostClock>().len();
    for name in ["MagManaIcons", "MagManaStates"] {
        world.ui_uints(&mut plugin, name);
    }
    for name in ["MagManaNames", "MagManaAmounts", "MagManaTimes"] {
        world.ui_list(&mut plugin, name);
    }
    for name in ["MagManaNeeded", "MagManaUnretained"] {
        world.ui_text(&mut plugin, name);
    }
    assert_eq!(world.requests::<calls::HostClock>().len(), clock_reads);
    assert_eq!(world.ui_uints(&mut plugin, "MagManaIcons"), [ring]);
    assert_eq!(world.ui_list(&mut plugin, "MagManaNames"), ["Test Ring"]);
    assert_eq!(
        world.ui_uints(&mut plugin, "MagManaStates"),
        [mosstank_mag::mana::ICON_ACTIVE]
    );
    assert_eq!(world.ui_list(&mut plugin, "MagManaAmounts"), ["100 / 200"]);
    assert_eq!(world.ui_list(&mut plugin, "MagManaTimes"), ["1h06m"]);
    assert_eq!(
        world.ui_text(&mut plugin, "MagManaNeeded"),
        "Mana needed: 100"
    );
    assert_eq!(
        world.ui_text(&mut plugin, "MagManaUnretained"),
        "Unretained Items: 1"
    );
    // 40 s on, one mana burned.
    world.fake.clock.monotonic_ms = 41_000;
    world.tick(&mut plugin);
    assert_eq!(world.ui_list(&mut plugin, "MagManaAmounts"), ["99 / 200"]);

    // Taken off: the row goes.
    world.send(
        &mut plugin,
        ac_plugin_abi::Event::EquipmentPlacement(ac_plugin_abi::items::EquipmentObservation {
            object_id: ring,
            equipped_location: 0x0004_0000,
            is_removal: true,
            is_initial_placement: false,
        }),
    );
    world.tick(&mut plugin);
    assert!(world.ui_list(&mut plugin, "MagManaNames").is_empty());
}

#[test]
fn the_inv_items_page_counts_the_consumables_and_their_use() {
    // Source: Mag-Tools InventoryTracker.cs and InventoryTrackerGUI.cs at
    // 43ee2ec: two healing kits at the login, one used a minute later.
    use ac_plugin_abi::items::InventoryItem;
    use ac_plugin_abi::world::{ObjectChange, ObjectChangeKind, ObjectClass};
    let kit = |object_id| InventoryItem {
        object_class: ObjectClass::HealingKit,
        value: 100,
        icon_id: 0x0600_1234,
        ..item(object_id, "Healing Kit", 0x80)
    };
    let mut world = World::with_character("Mag");
    world.set_reply::<calls::ItemsInventoryComplete>(&true);
    world.own_items(vec![
        kit(0x5000_0002),
        kit(0x5000_0003),
        InventoryItem {
            object_class: ObjectClass::Money,
            value: 250_000,
            stack_size: 250_000,
            ..item(0x5000_0004, "Pyreal", 0x40)
        },
    ]);
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    world.send(&mut plugin, ac_plugin_abi::Event::LoginComplete);
    world.ticks(&mut plugin, 2);

    world.act(&mut plugin, "ShowMag");
    world.act(&mut plugin, "ShowMagInvItems");
    assert!(world.ui_bool(&mut plugin, "MagInvItemsVisible"));
    assert_eq!(
        world.ui_list(&mut plugin, "MagInvItemsNames"),
        [
            "",
            "Peas",
            "Comps",
            "Salvage",
            "Net Profit",
            "",
            "",
            "Healing Kit"
        ]
    );
    assert_eq!(
        world.ui_list(&mut plugin, "MagInvItemsCounts"),
        ["", "", "", "", "", "", "Count", "2"]
    );
    assert_eq!(
        world.ui_uints(&mut plugin, "MagInvItemsIcons"),
        [0, 0, 0, 0, 0, 0, 0, 0x0600_1234]
    );
    assert_eq!(
        world.ui_list(&mut plugin, "MagInvItemsFiveMinutes")[0],
        "MMD/h ~5m"
    );
    assert_eq!(world.ui_list(&mut plugin, "MagInvItemsHours")[6], "(Hrs)");
    // The page is built once a frame: reading its values asks the client
    // nothing.
    let clock_reads = world.requests::<calls::HostClock>().len();
    for name in [
        "MagInvItemsNames",
        "MagInvItemsCounts",
        "MagInvItemsFiveMinutes",
        "MagInvItemsHour",
        "MagInvItemsHours",
    ] {
        world.ui_list(&mut plugin, name);
    }
    world.ui_uints(&mut plugin, "MagInvItemsIcons");
    assert_eq!(world.requests::<calls::HostClock>().len(), clock_reads);

    // A minute on, a kit is used up.
    world.fake.clock.monotonic_ms += 60_000;
    world.own_items(vec![
        kit(0x5000_0003),
        InventoryItem {
            object_class: ObjectClass::Money,
            value: 250_000,
            stack_size: 250_000,
            ..item(0x5000_0004, "Pyreal", 0x40)
        },
    ]);
    world.send(
        &mut plugin,
        ac_plugin_abi::Event::ObjectChanged(ObjectChange {
            object_id: 0x5000_0002,
            kind: ObjectChangeKind::Released,
            revision: 2,
            changed_fields: ObjectChangeKind::Released.fields(),
            current: None,
        }),
    );
    world.tick(&mut plugin);
    assert_eq!(world.ui_list(&mut plugin, "MagInvItemsCounts")[7], "1");
    // (1 - 2) * 60 / 1 an hour; one left lasts a minute.
    assert_eq!(
        world.ui_list(&mut plugin, "MagInvItemsFiveMinutes")[7],
        "-60.0"
    );
    assert_eq!(world.ui_list(&mut plugin, "MagInvItemsHour")[7], "-60.0");
    assert_eq!(world.ui_list(&mut plugin, "MagInvItemsHours")[7], "0.0");
    // No money changed: Net Profit has its login snapshot only.
    assert_eq!(world.ui_list(&mut plugin, "MagInvItemsHour")[4], "");
}

fn combat_line(text: &str) -> ac_plugin_abi::Event {
    ac_plugin_abi::Event::ChatReceived(ac_plugin_abi::ChatLine {
        kind: 8,
        combat_kind: 1,
        text: text.to_owned(),
        display_text: text.to_owned(),
        ..ac_plugin_abi::ChatLine::default()
    })
}

#[test]
fn the_combat_tracker_counts_the_chat_and_keeps_its_stats_across_sessions() {
    let mut world = World::with_character("Mag");
    world.set_reply::<calls::ItemsInventoryComplete>(&true);
    let mut plugin = world.enable();
    // The patterns compile a few a tick; a line heard meanwhile waits.
    world.tick(&mut plugin);
    world.send(
        &mut plugin,
        combat_line("You hit Drudge Skulker for 25 points of slashing damage!"),
    );
    world.ticks(&mut plugin, 60);
    world.send(&mut plugin, ac_plugin_abi::Event::LoginComplete);
    world.ticks(&mut plugin, 10);
    world.send(&mut plugin, combat_line("You obliterate Drudge Skulker!"));
    world.send(&mut plugin, combat_line("Drudge Skulker died!"));
    let (current, persistent) = world.with_tank(&mut plugin, |tank, _| {
        let combat = &tank.mag().combat;
        (
            combat.current.combat_infos().to_vec(),
            combat.persistent.combat_infos().to_vec(),
        )
    });
    assert_eq!(current.len(), 1);
    assert_eq!(
        (current[0].source.as_str(), current[0].killing_blows),
        ("Mag", 2)
    );
    assert_eq!(persistent, current);
    // Logging off writes the persistent stats (kept by default).
    let key = "Mag-Tools/Coldeve/Mag.CombatTracker.xml";
    assert!(world.plugin_data.get(key).is_none());
    world.send(&mut plugin, ac_plugin_abi::Event::Logoff);
    let written = world.plugin_data.get(key).expect("the stats").to_owned();
    assert!(written.starts_with("<CombatTracker>\r\n"), "{written}");
    assert!(written.contains("KillingBlows=\"2\""), "{written}");
    world.disable(&mut plugin);

    // The next session reads them in.
    let mut plugin = world.enable();
    world.ticks(&mut plugin, 60);
    world.send(&mut plugin, ac_plugin_abi::Event::LoginComplete);
    world.ticks(&mut plugin, 10);
    let persistent = world.with_tank(&mut plugin, |tank, _| {
        tank.mag().combat.persistent.combat_infos().to_vec()
    });
    assert_eq!(persistent, current);
}

/// The current page's monster list and detail list, as rows.
fn combat_rows(
    world: &mut World,
    plugin: &mut mosstank::MossTank,
    names: [&str; 5],
) -> Vec<[String; 5]> {
    let columns: Vec<Vec<String>> = names
        .iter()
        .map(|name| world.ui_list(plugin, name))
        .collect();
    (0..columns[0].len())
        .map(|row| std::array::from_fn(|column| columns[column][row].clone()))
        .collect()
}

const MONSTER_COLUMNS: [&str; 5] = [
    "MagCombatMarks",
    "MagCombatNames",
    "MagCombatKills",
    "MagCombatReceived",
    "MagCombatGiven",
];
const DETAIL_COLUMNS: [&str; 5] = [
    "MagCombatDetailNames",
    "MagCombatDetailMelee",
    "MagCombatDetailMagic",
    "MagCombatDetailLabels",
    "MagCombatDetailValues",
];

fn cells(cells: [&str; 5]) -> [String; 5] {
    cells.map(str::to_owned)
}

#[test]
fn the_combat_page_lists_the_fight_and_its_stats_come_back_next_session() {
    let mut world = World::with_character("Mag");
    world.set_reply::<calls::ItemsInventoryComplete>(&true);
    let mut plugin = world.enable();
    world.ticks(&mut plugin, 60);
    world.send(&mut plugin, ac_plugin_abi::Event::LoginComplete);
    world.ticks(&mut plugin, 10);
    for line in [
        "You hit Drudge Skulker for 25 points of slashing damage!",
        "Drudge Skulker hits your chest for 10 points of slashing damage!",
        "You obliterate Drudge Skulker!",
    ] {
        world.send(&mut plugin, combat_line(line));
    }
    world.tick(&mut plugin);

    world.act(&mut plugin, "ShowMag");
    world.act(&mut plugin, "ShowMagCombat");
    assert!(world.ui_bool(&mut plugin, "MagCombatCurrentSelected"));
    assert!(world.ui_bool(&mut plugin, "MagCombatStatsVisible"));
    assert!(!world.ui_bool(&mut plugin, "MagCombatOptionsVisible"));
    let monsters = vec![
        cells(["", "", "KB's", "Dmg Rcvd", "Dmg Givn"]),
        cells(["*", "All", "1", "10", "25"]),
        cells(["", "Drudge Skulker", "1", "10", "25"]),
    ];
    assert_eq!(
        combat_rows(&mut world, &mut plugin, MONSTER_COLUMNS),
        monsters
    );
    let detail = combat_rows(&mut world, &mut plugin, DETAIL_COLUMNS);
    assert_eq!(
        detail[0],
        cells(["", "Mel/Msl", "Magic", "Attacks", "2 (100%)"])
    );
    assert_eq!(detail[2], cells(["Slash", "10", "", "Resists", ""]));
    assert_eq!(detail[1][4], "1 (0%)");
    assert_eq!(detail[6][4], "25 / 25");
    assert_eq!(detail[10], cells(["Total", "10", "", "Total", "25"]));

    // A click on the monster's row shows its stats.
    world.act_int(&mut plugin, "SelectMagCombatRow", 2);
    assert_eq!(world.ui_list(&mut plugin, "MagCombatMarks"), ["", "", "*"]);

    // The options are the settings.
    world.act(&mut plugin, "ShowMagCombatOptions");
    assert!(world.ui_bool(&mut plugin, "MagCombatOptionsVisible"));
    assert!(!world.ui_bool(&mut plugin, "MagCombatStatsVisible"));
    assert!(world.ui_bool(&mut plugin, "MagCombatPersistent"));
    assert!(!world.ui_bool(&mut plugin, "MagCombatExportOnLogOff"));
    world.act_with(
        &mut plugin,
        "SetMagCombatExportOnLogOff",
        ac_plugin_abi::ui::ActionValue::Int(1),
    );
    assert!(world.ui_bool(&mut plugin, "MagCombatExportOnLogOff"));

    // The export button writes the current stats and says where.
    let key = "Mag-Tools/Coldeve/Mag.CombatTracker.xml";
    world.act(&mut plugin, "ExportMagCombatCurrent");
    let exports = |world: &World| {
        world
            .plugin_data
            .keys()
            .filter(|name| name.starts_with("Mag-Tools/Coldeve/Mag.CombatTracker.") && *name != key)
            .count()
    };
    assert_eq!(
        exports(&world),
        1,
        "{:?}",
        world.plugin_data.keys().collect::<Vec<_>>()
    );
    assert!(
        world
            .chat()
            .iter()
            .any(|text| text.starts_with("<{Mag-Tools}>: Stats exported to: ")),
        "{:?}",
        world.chat()
    );

    // Logging off writes the persistent stats.
    world.send(&mut plugin, ac_plugin_abi::Event::Logoff);
    let written = world.plugin_data.get(key).expect("the stats").to_owned();
    assert!(
        written.contains("TargetName=\"Drudge Skulker\""),
        "{written}"
    );
    world.disable(&mut plugin);

    // The next session reads them in: the persistent page lists them, the
    // current page is empty.
    let mut plugin = world.enable();
    world.ticks(&mut plugin, 60);
    world.send(&mut plugin, ac_plugin_abi::Event::LoginComplete);
    world.ticks(&mut plugin, 10);
    world.act(&mut plugin, "ShowMag");
    world.act(&mut plugin, "ShowMagCombat");
    assert_eq!(
        combat_rows(&mut world, &mut plugin, MONSTER_COLUMNS).len(),
        2
    );
    world.act(&mut plugin, "ShowMagCombatPersistent");
    assert!(world.ui_bool(&mut plugin, "MagCombatPersistentSelected"));
    assert_eq!(
        combat_rows(&mut world, &mut plugin, MONSTER_COLUMNS),
        monsters
    );

    // Clear Persistent Stats empties the page and deletes the file.
    world.act(&mut plugin, "ShowMagCombatOptions");
    world.act(&mut plugin, "ClearMagCombatPersistent");
    assert!(world.plugin_data.get(key).is_none());
    assert!(
        world
            .chat()
            .iter()
            .any(|text| text.starts_with("<{Mag-Tools}>: File deleted: ")),
        "{:?}",
        world.chat()
    );
    world.act(&mut plugin, "ShowMagCombatPersistent");
    assert_eq!(
        combat_rows(&mut world, &mut plugin, MONSTER_COLUMNS).len(),
        2
    );
}

/// A long life's persistent combat stats: `monsters` monsters fought.
fn persistent_combat_stats(me: &str, monsters: usize) -> String {
    let mut out = String::from("<CombatTracker>\r\n  <CombatInfos>");
    for index in 0..monsters {
        out.push_str(&format!(
            "\r\n    <CombatInfo SourceName=\"{me}\" TargetName=\"Drudge {index}\" KillingBlows=\"{kills}\">\r\n      <MeleeMissle>\r\n        <Slash TotalAttacks=\"{attacks}\" TotalNormalDamage=\"{damage}\" MaxNormalDamage=\"412\" />\r\n      </MeleeMissle>\r\n    </CombatInfo>\r\n    <CombatInfo SourceName=\"Drudge {index}\" TargetName=\"{me}\">\r\n      <MeleeMissle>\r\n        <Bludge TotalAttacks=\"{attacks}\" TotalNormalDamage=\"{damage}\" MaxNormalDamage=\"96\" />\r\n      </MeleeMissle>\r\n    </CombatInfo>",
            kills = 10 + index % 90,
            attacks = 1000 + index,
            damage = 112_000 + index * 7,
        ));
    }
    out.push_str("\r\n  </CombatInfos>\r\n</CombatTracker>");
    out
}

/// The character `Mag` logs in with a profile set that loads over several
/// ticks and a command that waits for it; with `mag_work`, a 1,000-monster
/// persistent combat file to read in and the Inventory window open on
/// three alts' 1,500-item logs. The ticks until the load is whole,
/// until the command ran, and whether Mag work was still under way.
fn ticks_until_the_login_settles(mag_work: bool) -> (usize, usize, bool) {
    use mosstank_formats::route::RouteWaypoint;

    let mut world = World::new();
    world.set_reply::<calls::ItemsInventoryComplete>(&true);
    world.own_items(vec![item(0x5000_0002, "Test Ring", 0x8)]);
    // The character's own route, large enough that its set loads over
    // several ticks.
    let route = mosstank_formats::route::NavigationSettings {
        enabled: true,
        waypoints: (0..4000)
            .map(|index| RouteWaypoint {
                position: ac_plugin_abi::world::Position {
                    east_west: f64::from(index) / 1000.0,
                    ..ac_plugin_abi::world::Position::default()
                },
                ..RouteWaypoint::default()
            })
            .collect(),
        ..mosstank_formats::route::NavigationSettings::default()
    };
    let key = format!(
        "{}/{}",
        mosstank_stores::directory::NAV_FOLDER,
        mosstank_stores::directory::auto_character_file_name("Mag", "Coldeve", "af")
    );
    world
        .profiles
        .insert(&key, &mosstank_formats::af::save_nav(&route).unwrap());
    if mag_work {
        world.plugin_data.insert(
            "Mag-Tools/Coldeve/Mag.CombatTracker.xml",
            &persistent_combat_stats("Mag", 1000),
        );
        for alt in 1..=3u32 {
            let items: Vec<_> = (0..1500u32)
                .map(|n| item(0x5100_0000 + alt * 0x1_0000 + n, &format!("Sword {n}"), 0x1))
                .collect();
            save_log(&mut world, &format!("Alt {alt}"), &items);
        }
    }
    let mut plugin = world.enable();
    if mag_work {
        world.act(&mut plugin, "OpenMagInventory");
    }
    world.ticks(&mut plugin, 3);

    world.fake.character = Some(common::character("Mag"));
    world.send(&mut plugin, ac_plugin_abi::Event::LoginComplete);
    world.tick(&mut plugin);
    assert!(
        plugin.tank().unwrap().is_loading_profiles(),
        "the set loads over more than one tick"
    );
    world.vt(&mut plugin, "opt set enablelooting false");
    assert!(plugin.tank().unwrap().has_held_commands());
    let mut loaded = None;
    let mut ran = None;
    for count in 1..=400 {
        world.tick(&mut plugin);
        let tank = plugin.tank().unwrap();
        if loaded.is_none() && !tank.is_loading_profiles() {
            loaded = Some(count);
        }
        if ran.is_none() && !tank.has_held_commands() {
            ran = Some(count);
            break;
        }
    }
    // The Mag work still under way then: it waited.
    let (importing, settled) = world.with_tank(&mut plugin, |tank, _| {
        let mag = tank.mag();
        (mag.combat.is_importing(), mag.inventory_search.is_settled())
    });
    (
        loaded.expect("the set loaded"),
        ran.expect("the command ran"),
        importing || !settled,
    )
}

#[test]
fn the_mag_features_background_work_waits_for_the_login_and_its_commands() {
    let (load, command, _) = ticks_until_the_login_settles(false);
    let (load_with, command_with, waited) = ticks_until_the_login_settles(true);
    assert_eq!(
        (load_with, command_with),
        (load, command),
        "(ticks to load, ticks to the command)"
    );
    assert!(command_with <= 10, "{command_with} ticks");
    assert!(waited, "the Mag work was under way");
}

#[test]
fn disabling_the_plugin_in_the_world_writes_what_logging_off_writes() {
    let mut world = World::with_character("Mag");
    world.set_reply::<calls::ItemsInventoryComplete>(&true);
    world.own_items(vec![item(0x5000_0002, "Ring", 0x8)]);
    let mut plugin = world.enable();
    world.command(
        &mut plugin,
        "ub",
        "opt set Mag.CombatTracker.ExportOnLogOff true",
    );
    world.ticks(&mut plugin, 60);
    world.send(&mut plugin, ac_plugin_abi::Event::LoginComplete);
    world.ticks(&mut plugin, 10);
    let log_key = "Mag-Tools/Coldeve/Mag.inventory.json";
    let logged = world.plugin_data.get(log_key).expect("the log").to_owned();
    world.send(&mut plugin, combat_line("You obliterate Drudge Skulker!"));
    world.send(&mut plugin, combat_line("Drudge Skulker died!"));
    let stats_key = "Mag-Tools/Coldeve/Mag.CombatTracker.xml";
    assert!(world.plugin_data.get(stats_key).is_none());

    // The ring goes into a side pack in the tick the plugin stops.
    let mut moved = item(0x5000_0002, "Ring", 0x8);
    moved.container_id = 0x8000_0900;
    world.own_items(vec![moved]);
    world.send(
        &mut plugin,
        ac_plugin_abi::Event::ObjectChanged(ac_plugin_abi::world::ObjectChange {
            object_id: 0x5000_0002,
            kind: ac_plugin_abi::world::ObjectChangeKind::Moved,
            revision: 3,
            changed_fields: 0,
            current: Some(ac_plugin_abi::world::WorldObject {
                object_id: 0x5000_0002,
                is_owned: true,
                ..ac_plugin_abi::world::WorldObject::default()
            }),
        }),
    );
    world.fake.clock.utc_unix_ms += 1000;
    world.disable(&mut plugin);
    let relogged = world.plugin_data.get(log_key).expect("the log").to_owned();
    let (before, _) = mosstank_mag::inventory_log::read(&logged).expect("it reads");
    let (after, records) = mosstank_mag::inventory_log::read(&relogged).expect("it reads");
    assert_eq!(after.saved_unix_ms, before.saved_unix_ms + 1000);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].container_id, 0x8000_0900, "the move recorded");
    let stats = world.plugin_data.get(stats_key).expect("the stats");
    assert!(stats.contains("KillingBlows=\"2\""), "{stats}");
    let exports = world
        .plugin_data
        .keys()
        .filter(|name| {
            name.starts_with("Mag-Tools/Coldeve/Mag.CombatTracker.") && *name != stats_key
        })
        .count();
    assert_eq!(exports, 1);
}

#[test]
fn disabling_the_plugin_after_logging_off_writes_nothing_again() {
    let mut world = World::with_character("Mag");
    world.set_reply::<calls::ItemsInventoryComplete>(&true);
    world.own_items(vec![item(0x5000_0002, "Ring", 0x8)]);
    let mut plugin = world.enable();
    world.ticks(&mut plugin, 60);
    world.send(&mut plugin, ac_plugin_abi::Event::LoginComplete);
    world.ticks(&mut plugin, 10);
    world.send(&mut plugin, combat_line("You obliterate Drudge Skulker!"));
    world.send(&mut plugin, ac_plugin_abi::Event::Logoff);
    let log_key = "Mag-Tools/Coldeve/Mag.inventory.json";
    let stats_key = "Mag-Tools/Coldeve/Mag.CombatTracker.xml";
    let logged = world.plugin_data.get(log_key).map(str::to_owned);
    world.plugin_data.insert(stats_key, "kept");
    world.fake.clock.utc_unix_ms += 1000;
    world.disable(&mut plugin);
    assert_eq!(world.plugin_data.get(log_key).map(str::to_owned), logged);
    assert_eq!(world.plugin_data.get(stats_key), Some("kept"));
}

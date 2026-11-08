//! The inventory logger: its record of one item, the merge with the last
//! save, the file, and when it saves.
//!
//! Source: Mag-Tools `Mag-Tools/Inventory/InventoryLogger.cs` and
//! `Shared/MyWorldObjectCreator.cs`, `Shared/MyWorldObject.cs` at 43ee2ec
//! (`Combine`, `AddTo`, `DumpInventoryToFile`). The file is our own JSON
//! (owner, 2026-11-08), not Mag-Tools' XML.

use ac_plugin_abi::items::{InventoryItem, ObjectHeader};
use ac_plugin_abi::world::{ArmorProfile, ItemProperties, ObjectClass, WeaponProfile, WorldObject};
use mosstank_mag::inventory_log::{
    ItemRecord, LogHeader, LogReader, ReadError, VERSION, combine, merge, read, storage_key, write,
};
use mosstank_mag::item_info::{ItemInfoInput, item_info};

const RING: u32 = 0x8000_0101;
const SWORD: u32 = 0x8000_0102;
const PACK: u32 = 0x8000_0103;
const ME: u32 = 0x5000_000A;

fn ring_item() -> InventoryItem {
    InventoryItem {
        object_id: RING,
        weenie_class_id: 621,
        name: "Test Ring".to_owned(),
        item_type: 0x8,
        container_id: ME,
        container_slot: 3,
        stack_size: 1,
        object_class: ObjectClass::Jewelry,
        icon_id: 0x0600_1000,
        value: 7500,
        burden: 30,
        structure: 0,
        material_type: 21,
        workmanship: 6.5,
        appraised_spell_ids: vec![2101, 0x8000_0000 | 3094, 1550],
        header: Some(ObjectHeader {
            weenie_header_flags: Some(0x8000_0000),
            ..ObjectHeader::default()
        }),
        ..InventoryItem::default()
    }
}

fn ring_properties() -> ItemProperties {
    ItemProperties {
        ints: vec![(19, 7500), (131, 21), (105, 6)],
        int64s: vec![(4, 1_234_567_890_123)],
        bools: vec![(69, true)],
        floats: vec![(5, -0.025), (144, 0.03)],
        strings: vec![
            (1, "Test Ring".to_owned()),
            (16, "Ünïcode \"quoted\" <b>".to_owned()),
        ],
        data_ids: vec![(1, 0x0200_00DE)],
        instance_ids: vec![(2, ME)],
        weapon: Some(WeaponProfile {
            damage_type: 4,
            weapon_time: 35,
            weapon_skill: 45,
            damage: 62,
            damage_variance: 0.45,
            damage_mod: 1.2,
            weapon_length: 1.0,
            max_velocity: 0.0,
            weapon_offense: 1.08,
            max_velocity_estimated: 0,
        }),
        armor: Some(ArmorProfile {
            armor_level: 200,
            slash_mod: 0.4,
            pierce_mod: 0.6,
            bludgeon_mod: 1.0,
            cold_mod: 0.8,
            fire_mod: 1.2,
            acid_mod: 0.2,
            nether_mod: 0.5,
            electric_mod: 1.4,
        }),
    }
}

fn seen(item: &InventoryItem, appraised: bool) -> WorldObject {
    WorldObject {
        object_id: item.object_id,
        name: item.name.clone(),
        object_class: item.object_class,
        is_owned: true,
        has_appraisal_data: appraised,
        ..WorldObject::default()
    }
}

fn ring_record() -> ItemRecord {
    let item = ring_item();
    ItemRecord::from_live(
        &item,
        Some(&seen(&item, true)),
        &ring_properties(),
        1_762_560_000_000,
    )
}

fn header() -> LogHeader {
    LogHeader {
        version: VERSION,
        character: "Acdream".to_owned(),
        world: "ACE".to_owned(),
        saved_unix_ms: 1_762_560_000_000,
    }
}

fn document(records: &[ItemRecord]) -> String {
    let fragments: Vec<String> = records.iter().map(ItemRecord::to_json).collect();
    write(&header(), fragments.iter().map(String::as_str))
}

#[test]
fn a_record_keeps_what_the_item_and_its_properties_hold() {
    let record = ring_record();
    assert_eq!(record.id, RING);
    assert_eq!(record.name, "Test Ring");
    assert_eq!(record.object_class, ObjectClass::Jewelry);
    assert_eq!(record.icon, 0x0600_1000);
    assert_eq!(record.container_id, ME);
    assert_eq!(record.container_slot, 3);
    assert!(record.has_id_data);
    assert_eq!(record.last_id_time, 1_762_560_000_000);
    assert_eq!(record.properties, ring_properties());
    // The appraisal's list split by the mark of a spell active on the item.
    assert_eq!(record.spells, vec![2101, 1550]);
    assert_eq!(record.active_spells, vec![3094]);
}

#[test]
fn an_item_never_appraised_has_no_id_time() {
    let item = ring_item();
    let record = ItemRecord::from_live(
        &item,
        Some(&seen(&item, false)),
        &ItemProperties::default(),
        99,
    );
    assert!(!record.has_id_data);
    assert_eq!(record.last_id_time, 0);
    // No object known: not appraised either.
    let record = ItemRecord::from_live(&item, None, &ItemProperties::default(), 99);
    assert!(!record.has_id_data);
}

#[test]
fn the_file_reads_back_as_written() {
    let mut sword = ring_record();
    sword.id = SWORD;
    sword.name = "Sword".to_owned();
    sword.object_class = ObjectClass::MeleeWeapon;
    sword.properties = ItemProperties::default();
    sword.spells.clear();
    sword.active_spells.clear();
    sword.weenie_header_flags = None;
    let records = vec![ring_record(), sword];
    let text = document(&records);
    let (read_header, read_records) = read(&text).expect("the file reads");
    assert_eq!(read_header, header());
    assert_eq!(read_records, records);
}

#[test]
fn the_file_has_the_planned_shape() {
    let text = document(&[ring_record()]);
    let value = openac_plugin_compat::json::parse(&text).expect("JSON");
    assert_eq!(value.get("version").and_then(|v| v.as_i64()), Some(1));
    assert_eq!(
        value.get("character").and_then(|v| v.as_str()),
        Some("Acdream")
    );
    assert_eq!(value.get("world").and_then(|v| v.as_str()), Some("ACE"));
    assert_eq!(
        value.get("saved_unix_ms").and_then(|v| v.as_i64()),
        Some(1_762_560_000_000)
    );
    let items = value
        .get("items")
        .and_then(|v| v.as_array())
        .expect("items");
    let ring = &items[0];
    assert_eq!(
        ring.get("id").and_then(|v| v.as_i64()),
        Some(i64::from(RING))
    );
    assert_eq!(ring.get("object_class").and_then(|v| v.as_i64()), Some(4));
    assert_eq!(
        ring.get("ints")
            .and_then(|m| m.get("19"))
            .and_then(|v| v.as_i64()),
        Some(7500)
    );
    assert_eq!(
        ring.get("floats")
            .and_then(|m| m.get("5"))
            .and_then(|v| v.as_f64()),
        Some(-0.025)
    );
    assert_eq!(
        ring.get("strings")
            .and_then(|m| m.get("1"))
            .and_then(|v| v.as_str()),
        Some("Test Ring")
    );
    assert_eq!(
        ring.get("spells")
            .and_then(|v| v.as_array())
            .map(<[_]>::len),
        Some(2)
    );
    assert!(ring.get("weapon").is_some_and(|v| !v.is_null()));
}

#[test]
fn the_file_reads_in_stages() {
    let records: Vec<ItemRecord> = (0..250)
        .map(|n| {
            let mut record = ring_record();
            record.id = RING + n;
            record
        })
        .collect();
    let text = document(&records);
    let mut reader = LogReader::new(text).expect("the header");
    assert_eq!(reader.header(), &header());
    assert_eq!(reader.remaining(), 250);
    let mut read_back = Vec::new();
    while reader.remaining() > 0 {
        read_back.extend(reader.read_items(100).expect("items"));
    }
    assert_eq!(read_back, records);
}

#[test]
fn a_file_in_any_layout_reads() {
    // Hand-edited or reformatted: the members in another order, spaces
    // and line breaks anywhere.
    let fragment = ring_record().to_json();
    let text = format!(
        "\n {{ \"items\" : [ \n {fragment} \n ] , \"world\":\"ACE\",\n\"character\" : \"Acdream\", \"saved_unix_ms\": 1762560000000, \"version\" : 1 }}\n"
    );
    let (read_header, records) = read(&text).expect("the file reads");
    assert_eq!(read_header, header());
    assert_eq!(records, vec![ring_record()]);
}

#[test]
fn an_unknown_version_is_refused() {
    let text = document(&[ring_record()]).replacen("\"version\":1", "\"version\":2", 1);
    assert_eq!(read(&text), Err(ReadError::UnknownVersion(Some(2))));
    let text = document(&[]).replacen("\"version\":1,", "", 1);
    assert_eq!(read(&text), Err(ReadError::UnknownVersion(None)));
}

#[test]
fn a_broken_file_is_corrupt() {
    let text = document(&[ring_record()]);
    assert_eq!(read(&text[..text.len() / 2]), Err(ReadError::Corrupt));
    assert_eq!(read("not json"), Err(ReadError::Corrupt));
    // An item without its id.
    let text = text.replacen(&format!("\"id\":{RING},"), "", 1);
    assert_eq!(read(&text), Err(ReadError::Corrupt));
}

#[test]
fn the_kept_record_rebuilds_the_items_info_text() {
    let item = ring_item();
    let properties = ring_properties();
    let record = ring_record();
    let (_, records) = read(&document(&[record])).expect("reads");
    let kept_item = records[0].item();
    let input = |item, properties| ItemInfoInput {
        item,
        properties,
        show_buffed_values: true,
        show_value_and_burden: true,
    };
    let mut spells = |_id: u32| None;
    let live = item_info(&input(&item, &properties), &mut spells);
    let kept = item_info(&input(&kept_item, records[0].properties()), &mut spells);
    assert!(!live.is_empty());
    assert_eq!(kept, live);
}

// --- The merge (`Combine`) ---

fn record(id: u32, class: ObjectClass, appraised: bool, ints: &[(u32, i32)]) -> ItemRecord {
    let item = InventoryItem {
        object_id: id,
        name: format!("Item {id}"),
        object_class: class,
        container_id: ME,
        appraised_spell_ids: if appraised { vec![2101] } else { Vec::new() },
        ..InventoryItem::default()
    };
    let properties = ItemProperties {
        ints: ints.to_vec(),
        ..ItemProperties::default()
    };
    let mut object = seen(&item, appraised);
    object.object_class = class;
    ItemRecord::from_live(
        &item,
        Some(&object),
        &properties,
        if appraised { 100 } else { 0 },
    )
}

#[test]
fn a_fresh_appraisal_replaces_the_record() {
    let older = record(RING, ObjectClass::Jewelry, true, &[(19, 1), (105, 7)]);
    let newer = record(RING, ObjectClass::Jewelry, true, &[(19, 2)]);
    assert_eq!(combine(older, newer.clone()), newer);
}

#[test]
fn an_unappraised_older_record_is_replaced() {
    let older = record(RING, ObjectClass::Jewelry, false, &[(19, 1), (105, 7)]);
    let newer = record(RING, ObjectClass::Jewelry, false, &[(19, 2)]);
    assert_eq!(combine(older, newer.clone()), newer);
}

#[test]
fn an_older_appraisal_keeps_its_keys_and_the_live_keys_overwrite() {
    let mut older = record(RING, ObjectClass::Jewelry, true, &[(19, 1), (105, 7)]);
    older.properties.weapon = Some(WeaponProfile {
        damage: 62,
        ..WeaponProfile::default()
    });
    older.last_id_time = 555;
    let mut newer = record(RING, ObjectClass::Jewelry, false, &[(19, 2), (5, 30)]);
    newer.container_id = PACK;
    newer.container_slot = 9;
    newer.stack_size = 4;
    let merged = combine(older.clone(), newer);
    // The live keys overwrite or are added; the appraisal's stay.
    assert_eq!(merged.properties.ints, vec![(19, 2), (105, 7), (5, 30)]);
    assert_eq!(merged.properties.weapon, older.properties.weapon);
    // Where it is now, from the live item.
    assert_eq!(merged.container_id, PACK);
    assert_eq!(merged.container_slot, 9);
    assert_eq!(merged.stack_size, 4);
    // The appraisal's own fields stay the older record's.
    assert!(merged.has_id_data);
    assert_eq!(merged.last_id_time, 555);
    assert_eq!(merged.spells, older.spells);
    assert_eq!(merged.object_class, older.object_class);
}

#[test]
fn the_merge_keeps_only_what_is_owned_now() {
    let previous = vec![
        record(RING, ObjectClass::Jewelry, true, &[(105, 7)]),
        record(SWORD, ObjectClass::MeleeWeapon, true, &[(105, 8)]),
        // Sold since.
        record(PACK, ObjectClass::Container, true, &[]),
    ];
    let live = vec![
        record(SWORD, ObjectClass::MeleeWeapon, false, &[(19, 2)]),
        // The same id as another class: not the same item.
        record(RING, ObjectClass::Armor, false, &[(19, 3)]),
    ];
    let merged = merge(&previous, live.clone());
    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].id, SWORD);
    assert!(merged[0].has_id_data);
    assert_eq!(merged[0].properties.ints, vec![(105, 8), (19, 2)]);
    assert_eq!(merged[1], live[1]);
}

// --- The file's key ---

#[test]
fn the_key_is_per_world_and_character() {
    assert_eq!(
        storage_key("ACE", "Acdream"),
        "Mag-Tools/ACE/Acdream.inventory.json"
    );
}

#[test]
fn a_name_ending_in_a_dot_never_makes_a_double_dot() {
    assert_eq!(
        storage_key("ACE", "Mr."),
        "Mag-Tools/ACE/Mr_.inventory.json"
    );
    for (world, name) in [
        ("ACE", "Mr.."),
        ("..", "x"),
        ("Cold.", "A.b."),
        ("a:b", "c\\d"),
        ("w", "a/../b"),
        ("", ""),
    ] {
        let key = storage_key(world, name);
        assert!(!key.contains(".."), "{key}");
        assert!(!key.contains(':'), "{key}");
        assert!(!key.contains('\\'), "{key}");
        assert_eq!(key.matches('/').count(), 2, "{key}");
        assert!(key.ends_with(".inventory.json"), "{key}");
    }
}

//! The settings profile serializer against the C# tests' cases and the
//! VTank fixtures (read in place from the C# test folder).

use mosstank_formats::fixtures;
use mosstank_formats::usd::{VtankCell, VtankDatabase, VtankRow, VtankTable};
use mosstank_settings::items::item_use_specifiers;
use mosstank_settings::model::{AllSettings, GemFoodItem, TargetSelectionMethod};
use mosstank_settings::options::{self, VtankSettingValueType};
use mosstank_settings::{defaults, profile};
use openac_plugin_compat::text::ordinal;

const USD_FIXTURES: [&str; 4] = ["defaultsettings", "owner-a", "owner-b", "owner-c"];

fn usd(name: &str) -> String {
    fixtures::read_vtank(&format!("{name}.usd"))
}

fn find_table<'a>(database: &'a VtankDatabase, name: &str) -> &'a VtankTable {
    database.find(name).unwrap()
}

fn table_mut<'a>(database: &'a mut VtankDatabase, name: &str) -> &'a mut VtankTable {
    database.find_mut(name).unwrap()
}

fn column(table: &VtankTable, name: &str) -> usize {
    table.column_index(name).unwrap()
}

fn buffed_item(object_id: i32, spell_id: i32, custom: Option<&str>) -> VtankRow {
    let mut cells = vec![VtankCell::int(object_id), VtankCell::int(spell_id)];
    if let Some(custom) = custom {
        cells.push(VtankCell::string(custom));
    }
    VtankRow { cells }
}

fn add_extension_column(table: &mut VtankTable, name: &str) {
    table.column_names.push(name.to_owned());
    table.index_flags.push(false);
}

fn gem_food_row(table: &VtankTable, name: &str, spell: i32, extension: &str) -> VtankRow {
    let mut row = VtankRow {
        cells: vec![VtankCell::int(0); table.column_names.len()],
    };
    row.cells[column(table, "Name")] = VtankCell::string(name);
    row.cells[column(table, "Spell")] = VtankCell::int(spell);
    row.cells[column(table, "Extension")] = VtankCell::string(extension);
    row
}

fn exemplar_row(table: &VtankTable, exemplar_id: i32, extension: Option<&str>) -> VtankRow {
    let mut row = VtankRow {
        cells: vec![VtankCell::int(0); table.column_names.len()],
    };
    row.cells[column(table, "ExemplarId")] = VtankCell::int(exemplar_id);
    if let Some(extension) = extension {
        row.cells[column(table, "Extension")] = VtankCell::string(extension);
    }
    row
}

fn raw_int_cell(text: &str) -> VtankCell {
    VtankCell {
        tag: "i".to_owned(),
        scalar_text: Some(text.to_owned()),
        table: None,
        blob_text: None,
    }
}

fn single_setting_usd(name: &str, value_tag: &str, value_text: &str) -> String {
    let lines = [
        "1",
        "Settings",
        "4",
        "Setting",
        "Value",
        "Description",
        "SettingType",
        "y",
        "n",
        "n",
        "n",
        "1",
        "s",
        name,
        value_tag,
        value_text,
        "s",
        "",
        "i",
        "1",
    ];
    lines.join("\r\n") + "\r\n"
}

fn load(text: &str, settings: &mut AllSettings) -> VtankDatabase {
    profile::load(text, settings, None).unwrap()
}

fn save(document: &mut VtankDatabase, settings: &mut AllSettings) -> String {
    profile::save(document, settings).unwrap()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: BuffedItemsSupplyDistinctOrderedIdsAndKeepSpellRows
#[test]
fn buffed_items_supply_distinct_ordered_ids_and_keep_spell_rows() {
    let mut document = defaults::create();
    let rows = &mut table_mut(&mut document, "BuffedItems").rows;
    for (object, spell) in [
        (802, 17),
        (801, 18),
        (802, 19),
        (802, -1),
        (900, 20),
        (0, 22),
        (-1, 21),
    ] {
        rows.push(buffed_item(object, spell, None));
    }
    let original = document.render();
    let mut settings = AllSettings {
        player_object_id: 900,
        ..AllSettings::default()
    };

    let mut loaded = load(&original, &mut settings);

    assert_eq!(settings.combat.combat_item_order_ids, [802, 801, 0]);
    let mut ids: Vec<u32> = settings
        .combat
        .combat_item_object_ids
        .iter()
        .copied()
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, [0, 801, 802]);
    let rows = &settings.buffs.item_enchant_rows;
    assert!(
        rows.iter()
            .any(|row| row.object_id == Some(802) && row.spell_id == Some(17))
    );
    assert!(
        rows.iter()
            .any(|row| row.object_id == Some(u32::MAX) && row.spell_id == Some(21))
    );
    assert!(
        rows.iter()
            .any(|row| row.object_id == Some(802) && row.casts_nothing())
    );
    assert_eq!(save(&mut loaded, &mut settings), original);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: BuffedItemsKeepMalformedDuplicateAndCustomRowsOnRoundTrip
#[test]
fn buffed_items_keep_malformed_duplicate_and_custom_rows_on_round_trip() {
    let mut document = defaults::create();
    let table = table_mut(&mut document, "BuffedItems");
    add_extension_column(table, "Custom");
    table.rows.push(buffed_item(801, 17, Some("first")));
    table.rows.push(buffed_item(801, 18, Some("second")));
    table.rows.push(VtankRow {
        cells: vec![
            VtankCell::string("not-an-id"),
            VtankCell::int(19),
            VtankCell::string("raw"),
        ],
    });
    table.rows.push(VtankRow {
        cells: vec![
            VtankCell::uint(u32::MAX),
            VtankCell::int(20),
            VtankCell::string("overflow"),
        ],
    });
    let original = document.render();
    let mut settings = AllSettings::default();

    let mut loaded = load(&original, &mut settings);

    assert_eq!(settings.combat.combat_item_order_ids, [801]);
    assert_eq!(save(&mut loaded, &mut settings), original);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: ItemUseSpecifiersRetainSignedIdsAndClearOnProfileSwitch
#[test]
fn item_use_specifiers_retain_signed_ids_and_clear_on_profile_switch() {
    const SIGNED: u32 = 0x8000_DF43;
    let mut first = defaults::create();
    let rows = &mut table_mut(&mut first, "ItemUseSpecifiers").rows;
    for (object, uses) in [(SIGNED.cast_signed(), 0), (2, 1), (3, 2), (4, 3)] {
        rows.push(VtankRow {
            cells: vec![VtankCell::int(object), VtankCell::int(uses)],
        });
    }
    let mut second = defaults::create();
    second
        .tables
        .retain(|(name, _)| name != "ItemUseSpecifiers");
    let mut settings = AllSettings::default();

    load(&first.render(), &mut settings);
    assert_eq!(item_use_specifiers::uses_for(&settings.combat, SIGNED), 0);
    assert_eq!(item_use_specifiers::uses_for(&settings.combat, 2), 1);
    assert_eq!(item_use_specifiers::uses_for(&settings.combat, 3), 2);
    assert_eq!(item_use_specifiers::uses_for(&settings.combat, 4), 3);
    assert_eq!(item_use_specifiers::uses_for(&settings.combat, 99), 3);

    load(&second.render(), &mut settings);
    assert!(settings.combat.item_use_specifiers.is_empty());
}

/// The Hands choices are written back: a changed value where its row
/// stands, a dropped item's row removed, a new item's row added, a row
/// that does not read kept; and what is written reads back the same.
#[test]
fn item_use_specifiers_are_saved_and_read_back() {
    let mut document = defaults::create();
    let rows = &mut table_mut(&mut document, "ItemUseSpecifiers").rows;
    for (object, uses) in [(10, 1), (11, 2), (12, 3)] {
        rows.push(VtankRow {
            cells: vec![VtankCell::int(object), VtankCell::int(uses)],
        });
    }
    rows.push(VtankRow {
        cells: vec![VtankCell::string("not an id"), VtankCell::int(1)],
    });
    let mut settings = AllSettings::default();
    let mut document = profile::load(&document.render(), &mut settings, None).unwrap();

    let uses = &mut settings.combat.item_use_specifiers;
    uses.insert(10, 0);
    uses.remove(&11);
    uses.insert(0x8000_0001, 2);
    let text = profile::save(&mut document, &mut settings).unwrap();

    let saved = VtankDatabase::parse(&text).unwrap();
    let table = find_table(&saved, "ItemUseSpecifiers");
    let cells: Vec<(String, String)> = table
        .rows
        .iter()
        .map(|row| {
            (
                row.cells[0].as_string().to_owned(),
                row.cells[1].as_string().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        cells,
        [
            ("10".to_owned(), "0".to_owned()),
            ("12".to_owned(), "3".to_owned()),
            ("not an id".to_owned(), "1".to_owned()),
            ("-2147483647".to_owned(), "2".to_owned()),
        ]
    );
    let mut reread = AllSettings::default();
    load(&text, &mut reread);
    assert_eq!(item_use_specifiers::uses_for(&reread.combat, 10), 0);
    assert_eq!(item_use_specifiers::uses_for(&reread.combat, 11), 3);
    assert_eq!(item_use_specifiers::uses_for(&reread.combat, 12), 3);
    assert_eq!(
        item_use_specifiers::uses_for(&reread.combat, 0x8000_0001),
        2
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: LoadingAProfileWithoutBuffedItemsClearsEarlierProfiledRows
#[test]
fn loading_a_profile_without_buffed_items_clears_earlier_profiled_rows() {
    let mut first = defaults::create();
    table_mut(&mut first, "BuffedItems")
        .rows
        .push(buffed_item(801, 17, None));
    let mut second = defaults::create();
    second.tables.retain(|(name, _)| name != "BuffedItems");
    let mut settings = AllSettings::default();

    load(&first.render(), &mut settings);
    load(&second.render(), &mut settings);

    assert!(settings.combat.combat_item_order_ids.is_empty());
    assert!(
        !settings
            .buffs
            .item_enchant_rows
            .iter()
            .any(|row| row.is_profiled_item_row())
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: EveryUsdFixtureLoads
#[test]
fn every_usd_fixture_loads() {
    for name in USD_FIXTURES {
        let mut settings = AllSettings::default();
        let document = load(&usd(name), &mut settings);
        assert!(document.find("Settings").is_some(), "{name}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: UntouchedRoundTripIsByteIdentical
#[test]
fn untouched_round_trip_is_byte_identical() {
    for name in USD_FIXTURES {
        let original = usd(name);
        let mut settings = AllSettings::default();
        let mut document = load(&original, &mut settings);
        assert!(save(&mut document, &mut settings) == original, "{name}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: DefaultSettingsUsdMatchesEveryCatalogDefault
#[test]
fn default_settings_usd_matches_every_catalog_default() {
    let mut s = AllSettings::default();
    load(&usd("defaultsettings"), &mut s);

    assert_eq!(s.combat.hunt_skill_excess_over_difficulty, 25);
    assert!(s.combat.enabled);
    assert!(s.buffs.enabled);
    assert!(!s.inventory.loot.enabled);
    assert!(!s.navigation.enabled);
    assert!((s.vitals.normal_health - 0.75).abs() < 1e-6);
    assert!((s.vitals.normal_stamina - 0.50).abs() < 1e-6);
    assert!((s.vitals.normal_mana - 0.50).abs() < 1e-6);
    assert!((s.combat.maximum_range - 5.0).abs() < 1e-3);
    assert_eq!(s.combat.attack_height.0, 2);
    assert_eq!(s.combat.selection_method, TargetSelectionMethod::BOTH);
    assert!((s.vitals.stamina_to_health_multiplier - 1.9).abs() < 1e-6);
    assert!((s.vitals.mana_to_health_multiplier - 2.8).abs() < 1e-6);
    assert_eq!(s.buffs.protection_elements, "ALFCBPS");
    assert_eq!(s.buffs.protection_profile_mode, 2);
    assert!((s.buffs.buff_cast_recast_seconds - 30.0).abs() < 1e-6);
    assert_eq!(s.buffs.buff_with_untrained_item_skill, 80);
    assert_eq!(s.navigation.door_lockpick_excess_threshold, -50);
    assert_eq!(s.vitals.drop_to_peace_mode_retry_count, 34);

    assert_eq!(s.vitals.recharge_handler_rows.len(), 26);
    let first = &s.vitals.recharge_handler_rows[0];
    assert_eq!(first.vital, 1); // health
    assert_eq!(first.handler_string, "Stamina to Health");
    assert_eq!(first.min_percent, 0);
    assert_eq!(first.max_percent, 15);
    assert_eq!(first.stance, 1); // magic
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: Every137CatalogNameMapsToExactlyOneField
#[test]
fn every_137_catalog_name_maps_to_exactly_one_field() {
    let settings = AllSettings::default();
    let unmapped: Vec<&str> = options::NAMES
        .iter()
        .copied()
        .filter(|name| profile::capture(name, &settings).is_none())
        .collect();
    // The recharge plan is the only one with no write path; the reference
    // client has none for it either.
    assert_eq!(unmapped, ["RechargeHandlerSet"]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: CaptureMatchesDeclaredSettingTypeAndValue
#[test]
fn capture_matches_declared_setting_type_and_value() {
    let mut settings = AllSettings::default();
    let document = load(&usd("defaultsettings"), &mut settings);
    let table = find_table(&document, "Settings");
    let name_column = column(table, "Setting");
    let value_column = column(table, "Value");
    for name in options::NAMES.iter().copied() {
        if name == "RechargeHandlerSet" {
            continue; // no live write path (see the test above)
        }
        let rows: Vec<&VtankRow> = table
            .rows
            .iter()
            .filter(|row| row.cells[name_column].as_string() == name)
            .collect();
        assert_eq!(rows.len(), 1, "{name}");
        let fixture_cell = &rows[0].cells[value_column];
        let captured = profile::capture(name, &settings).unwrap();
        assert_eq!(fixture_cell.tag, captured.tag, "{name}");
        match fixture_cell.tag.as_str() {
            "b" => assert_eq!(fixture_cell.as_bool(), captured.as_bool(), "{name}"),
            "s" => assert_eq!(fixture_cell.as_string(), captured.as_string(), "{name}"),
            "i" => assert_eq!(fixture_cell.as_int(), captured.as_int(), "{name}"),
            "u" => assert_eq!(fixture_cell.as_uint(), captured.as_uint(), "{name}"),
            "d" | "f" => assert_eq!(fixture_cell.as_double(), captured.as_double(), "{name}"),
            other => panic!("{name}: unexpected tag '{other}'."),
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: RechargeHandlerSetRoundTripsFromOwnerFixture
#[test]
fn recharge_handler_set_round_trips_from_owner_fixture() {
    let mut settings = AllSettings::default();
    load(&usd("owner-a"), &mut settings);
    assert!(!settings.vitals.recharge_handler_rows.is_empty());
    assert!(
        settings
            .vitals
            .recharge_handler_rows
            .iter()
            .any(|row| row.handler_string == "Kit Recharge")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: CreateNewHasTheSameTableSetAsTheDefaultFixture
#[test]
fn create_new_has_the_same_table_set_as_the_default_fixture() {
    let fixture = VtankDatabase::parse(&usd("defaultsettings")).unwrap();
    let created = profile::create_new(&mut AllSettings::default()).unwrap();

    let names = |database: &VtankDatabase| {
        let mut names: Vec<String> = database.tables.iter().map(|(n, _)| n.clone()).collect();
        names.sort_by(|a, b| ordinal::compare(a, b));
        names
    };
    let expected = names(&fixture);
    assert_eq!(expected, names(&created));
    assert!(
        expected.len() > 1,
        "the fixture itself must carry more than one table."
    );

    let settings = find_table(&created, "Settings");
    let description = column(settings, "Description");
    let kind = column(settings, "SettingType");
    assert!(
        settings
            .rows
            .iter()
            .any(|row| !row.cells[description].as_string().is_empty())
    );
    assert!(
        settings
            .rows
            .iter()
            .any(|row| row.cells[kind].as_int().unwrap() != VtankSettingValueType::Bool as i32)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: UnknownSettingsRowIsPreservedVerbatim
#[test]
fn unknown_settings_row_is_preserved_verbatim() {
    let text = single_setting_usd("SomeFutureSetting", "s", "future-value");
    let mut settings = AllSettings::default();
    let mut document = load(&text, &mut settings);
    assert_eq!(save(&mut document, &mut settings), text);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: GemFoodRowsLoadSaveAndReplaceThePriorProfileRows
#[test]
fn gem_food_rows_load_save_and_replace_the_prior_profile_rows() {
    let mut settings = AllSettings::default();
    let mut document = load(&usd("owner-c"), &mut settings);
    assert!(
        settings
            .buffs
            .gem_food_items
            .iter()
            .any(|entry| entry.name == "Asheron's Benediction" && entry.spell_id == 3810)
    );
    settings.buffs.gem_food_items = vec![GemFoodItem {
        name: "Unknown Gem".to_owned(),
        spell_id: 999_999,
    }];
    let saved = save(&mut document, &mut settings);

    let mut reloaded = AllSettings::default();
    load(&saved, &mut reloaded);
    assert_eq!(
        reloaded.buffs.gem_food_items,
        [GemFoodItem {
            name: "Unknown Gem".to_owned(),
            spell_id: 999_999,
        }]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: GemFoodWritesKeepCustomCellsInvalidRowsAndUnchangedRowsVerbatim
#[test]
fn gem_food_writes_keep_custom_cells_invalid_rows_and_unchanged_rows_verbatim() {
    let mut seed = defaults::create();
    let table = table_mut(&mut seed, "GemFoodItems");
    add_extension_column(table, "Extension");
    table.rows.clear();
    let keep = gem_food_row(table, "Blackmoor's Favor", 3811, "keep-this");
    let broken = gem_food_row(table, "Broken entry", 0, "invalid-row");
    table.rows.push(keep);
    table.rows.push(broken);
    let original = seed.render();

    let mut settings = AllSettings::default();
    let mut document = load(&original, &mut settings);
    assert_eq!(save(&mut document, &mut settings), original);

    settings.buffs.gem_food_items = vec![GemFoodItem {
        name: "Blackmoor's Favor".to_owned(),
        spell_id: 3812,
    }];
    let edited = save(&mut document, &mut settings);
    let reparsed = VtankDatabase::parse(&edited).unwrap();
    let rewritten = find_table(&reparsed, "GemFoodItems");
    let extension = column(rewritten, "Extension");
    let spell = column(rewritten, "Spell");
    assert!(
        rewritten
            .rows
            .iter()
            .any(|row| row.cells[extension].as_string() == "keep-this")
    );
    assert!(
        rewritten
            .rows
            .iter()
            .any(|row| row.cells[extension].as_string() == "invalid-row")
    );
    assert!(
        rewritten
            .rows
            .iter()
            .any(|row| row.cells[spell].as_int() == Ok(3812))
    );

    settings.buffs.gem_food_items.clear();
    save(&mut document, &mut settings);
    let after_removal = find_table(&document, "GemFoodItems");
    assert_eq!(after_removal.rows.len(), 1);
    assert_eq!(
        after_removal.rows[0].cells[column(after_removal, "Name")].as_string(),
        "Broken entry"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: GemFoodInvalidNumericCellsStayInertAndRoundTripVerbatim
#[test]
fn gem_food_invalid_numeric_cells_stay_inert_and_round_trip_verbatim() {
    let mut seed = defaults::create();
    let table = table_mut(&mut seed, "GemFoodItems");
    add_extension_column(table, "Extension");
    table.rows.clear();
    let spell = column(table, "Spell");
    let mut non_numeric = gem_food_row(table, "Broken text", 1, "");
    non_numeric.cells[spell] = raw_int_cell("not-an-id");
    let mut overflow = gem_food_row(table, "Broken overflow", 1, "");
    overflow.cells[spell] = raw_int_cell("999999999999999999999");
    table.rows.push(non_numeric);
    table.rows.push(overflow);
    let original = seed.render();

    let mut settings = AllSettings::default();
    let mut document = load(&original, &mut settings);

    assert!(settings.buffs.gem_food_items.is_empty());
    assert_eq!(save(&mut document, &mut settings), original);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: GemFoodRemovalKeepsTheExactSameNameSourceRow
#[test]
fn gem_food_removal_keeps_the_exact_same_name_source_row() {
    let mut seed = defaults::create();
    let table = table_mut(&mut seed, "GemFoodItems");
    add_extension_column(table, "Extension");
    table.rows.clear();
    let a = gem_food_row(table, "Gem", 1, "custom-a");
    let b = gem_food_row(table, "Gem", 2, "custom-b");
    table.rows.push(a);
    table.rows.push(b);
    let mut settings = AllSettings::default();
    let mut document = load(&seed.render(), &mut settings);
    settings.buffs.gem_food_items = vec![GemFoodItem {
        name: "Gem".to_owned(),
        spell_id: 1,
    }];

    let saved = VtankDatabase::parse(&save(&mut document, &mut settings)).unwrap();
    let saved = find_table(&saved, "GemFoodItems");

    assert_eq!(saved.rows.len(), 1);
    let row = &saved.rows[0];
    assert_eq!(row.cells[column(saved, "Spell")].as_int(), Ok(1));
    assert_eq!(
        row.cells[column(saved, "Extension")].as_string(),
        "custom-a"
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: GemFoodRowsClearWhenTheNextProfileHasNoTable
#[test]
fn gem_food_rows_clear_when_the_next_profile_has_no_table() {
    let mut settings = AllSettings::default();
    load(&usd("owner-c"), &mut settings);
    assert!(!settings.buffs.gem_food_items.is_empty());

    load(
        &single_setting_usd("EnableBuffing", "b", "True"),
        &mut settings,
    );

    assert!(settings.buffs.gem_food_items.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: ExtraBuffExemplarsLoadRoundTripAndPreserveUnknownCells
#[test]
fn extra_buff_exemplars_load_round_trip_and_preserve_unknown_cells() {
    let mut seed = defaults::create();
    let extra = table_mut(&mut seed, "ExtraBuffSpells");
    add_extension_column(extra, "Extension");
    let exemplar = column(extra, "ExemplarId");
    let rows = [
        exemplar_row(extra, 999_999, Some("keep-extra")),
        exemplar_row(extra, 999_999, Some("duplicate-extra")),
        exemplar_row(extra, 0, Some("invalid-extra")),
    ];
    let mut overflow = exemplar_row(extra, 1, Some("overflow-extra"));
    overflow.cells[exemplar] = raw_int_cell("999999999999999999999");
    extra.rows.extend(rows);
    extra.rows.push(overflow);
    let anti = table_mut(&mut seed, "AntiExtraBuffSpells");
    let anti_row = exemplar_row(anti, 999_998, None);
    anti.rows.push(anti_row);
    let original = seed.render();

    let mut settings = AllSettings::default();
    let mut document = load(&original, &mut settings);
    assert_eq!(settings.buffs.extra_buff_spell_ids, [999_999]);
    assert_eq!(settings.buffs.anti_extra_buff_spell_ids, [999_998]);
    assert_eq!(save(&mut document, &mut settings), original);

    settings.buffs.extra_buff_spell_ids = vec![999_997];
    let saved = VtankDatabase::parse(&save(&mut document, &mut settings)).unwrap();
    let rewritten = find_table(&saved, "ExtraBuffSpells");
    let extension = column(rewritten, "Extension");
    assert!(
        rewritten
            .rows
            .iter()
            .any(|row| row.cells[extension].as_string() == "invalid-extra")
    );
    assert!(
        rewritten
            .rows
            .iter()
            .any(|row| row.cells[extension].as_string() == "overflow-extra")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankSettingsProfileSerializerTests.cs :: ExtraBuffExemplarsClearWhenTheNextProfileOmitsTheirTables
#[test]
fn extra_buff_exemplars_clear_when_the_next_profile_omits_their_tables() {
    let mut settings = AllSettings::default();
    load(&usd("owner-c"), &mut settings);
    settings.buffs.extra_buff_spell_ids.push(100);
    settings.buffs.anti_extra_buff_spell_ids.push(200);

    let mut without_tables = defaults::create();
    without_tables
        .tables
        .retain(|(name, _)| name != "ExtraBuffSpells" && name != "AntiExtraBuffSpells");
    load(&without_tables.render(), &mut settings);

    assert!(settings.buffs.extra_buff_spell_ids.is_empty());
    assert!(settings.buffs.anti_extra_buff_spell_ids.is_empty());
}

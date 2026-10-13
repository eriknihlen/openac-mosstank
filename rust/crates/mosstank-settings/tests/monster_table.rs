//! The `MyMonsters` table against the C# tests' cases.

use mosstank_data::damage::MonsterDamageType;
use mosstank_formats::usd::{VtankCell, VtankDatabase, VtankRow, VtankTable};
use mosstank_settings::model::AllSettings;
use mosstank_settings::monster::{
    MonsterActionFlags, MonsterExpressionContext, MonsterRule, MonsterRuleActions, secondary_equip,
};
use mosstank_settings::{defaults, monster_table, profile};

fn monsters(database: &mut VtankDatabase) -> &mut VtankTable {
    database.find_mut(monster_table::TABLE_NAME).unwrap()
}

fn read(database: &VtankDatabase) -> Vec<MonsterRule> {
    monster_table::try_read(database, None).unwrap().unwrap()
}

fn write(database: &mut VtankDatabase, rules: &[MonsterRule]) {
    monster_table::write(database, rules).unwrap();
}

fn rule(expression: &str, actions: MonsterRuleActions) -> MonsterRule {
    MonsterRule::new(expression, actions).unwrap()
}

fn with_flags(flags: MonsterActionFlags) -> MonsterRuleActions {
    MonsterRuleActions {
        flags,
        ..MonsterRuleActions::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankMonsterRuleTableTests.cs :: ShippedDefaultRowParsesToTheShippedValues
#[test]
fn shipped_default_row_parses_to_the_shipped_values() {
    let rules = read(&defaults::create());
    assert_eq!(rules.len(), 1);
    let rule = &rules[0];
    assert!(rule.is_default());
    assert_eq!(rule.expression(), monster_table::DEFAULT_ROW_NAME);
    assert_eq!(rule.priority(), 1);
    assert_eq!(rule.actions().damage_type, MonsterDamageType::Auto);
    assert_eq!(
        rule.actions().flags,
        MonsterActionFlags::ATTACK | MonsterActionFlags::STREAK
    );
    assert_eq!(rule.actions().extra_vulnerability, MonsterDamageType::None);
    assert_eq!(
        rule.actions().pet_damage_type,
        MonsterDamageType::PlayerAuto
    );
    assert_eq!(rule.actions().weapon_object_id, 0);
    assert_eq!(rule.actions().offhand_object_id, 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankMonsterRuleTableTests.cs :: TheExtraVulnerabilityColumnRoundTripsItsFileCodes
#[test]
fn the_extra_vulnerability_column_round_trips_its_file_codes() {
    const EXTRA_VULNERABILITY_COLUMN: usize = 18;
    for (code, expected) in [
        (98, MonsterDamageType::None),
        (8, MonsterDamageType::Auto),
        (3, MonsterDamageType::Acid),
    ] {
        let mut seed = defaults::create();
        monsters(&mut seed).rows[0].cells[EXTRA_VULNERABILITY_COLUMN] = VtankCell::int(code);
        let original = seed.render();

        let mut database = VtankDatabase::parse(&original).unwrap();
        let rules = read(&database);
        assert_eq!(rules[0].actions().extra_vulnerability, expected);

        write(&mut database, &rules);
        assert_eq!(database.render(), original);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankMonsterRuleTableTests.cs :: AFreshRowWritesTheExtraVulnerabilityColumnOff
#[test]
fn a_fresh_row_writes_the_extra_vulnerability_column_off() {
    let mut database = defaults::create();
    write(
        &mut database,
        &[rule("DEFAULT", MonsterRuleActions::default())],
    );
    assert_eq!(monsters(&mut database).rows[0].cells[18].as_int(), Ok(98));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankMonsterRuleTableTests.cs :: UntouchedRowsRoundTripByteForByte
#[test]
fn untouched_rows_round_trip_byte_for_byte() {
    let original = defaults::create().render();
    let mut database = VtankDatabase::parse(&original).unwrap();
    let rules = read(&database);
    write(&mut database, &rules);
    assert_eq!(database.render(), original);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankMonsterRuleTableTests.cs :: PriorityWeaponAndSecondaryEquipShapesRoundTripByteForByte
#[test]
fn priority_weapon_and_secondary_equip_shapes_round_trip_byte_for_byte() {
    let mut seed = defaults::create();
    let row = &mut monsters(&mut seed).rows[0];
    row.cells[1] = VtankCell::int(7);
    row.cells[3] = VtankCell::int(0);
    row.cells[19] = VtankCell::int(secondary_equip::AUTO_SHIELD);
    let original = seed.render();

    let mut database = VtankDatabase::parse(&original).unwrap();
    let rules = read(&database);
    assert_eq!(rules[0].priority(), 7);

    write(&mut database, &rules);
    assert_eq!(database.render(), original);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankMonsterRuleTableTests.cs :: SignedDirectEquipmentIdsSurviveParseAndRoundTrip
#[test]
fn signed_direct_equipment_ids_survive_parse_and_round_trip() {
    const PRIMARY: u32 = 0x8000_DEB2;
    const SECONDARY: u32 = 0x8001_AC87;
    let mut database = defaults::create();
    let table = monsters(&mut database);
    table.rows[0].cells[3] = VtankCell::int(PRIMARY.cast_signed());
    table.rows[0].cells[19] = VtankCell::int(SECONDARY.cast_signed());

    let rules = read(&database);
    assert_eq!(rules.len(), 1);
    let actions = rules[0].actions().clone();
    assert_eq!(actions.weapon_object_id, PRIMARY);
    assert_eq!(actions.offhand_object_id, SECONDARY);

    write(&mut database, &[rule("DEFAULT", actions)]);
    let table = monsters(&mut database);
    assert_eq!(table.rows[0].cells[3].as_int(), Ok(PRIMARY.cast_signed()));
    assert_eq!(
        table.rows[0].cells[19].as_int(),
        Ok(SECONDARY.cast_signed())
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankMonsterRuleTableTests.cs :: EverySecondaryEquipModeSurvivesASave
#[test]
fn every_secondary_equip_mode_survives_a_save() {
    for mode in 0..=3 {
        let mut database = defaults::create();
        monsters(&mut database).rows[0].cells[19] = VtankCell::int(mode);
        let rules = read(&database);
        write(&mut database, &rules);
        assert_eq!(monsters(&mut database).rows[0].cells[19].as_int(), Ok(mode));
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankMonsterRuleTableTests.cs :: WeaponToUseKeepsTheSpellingTheFileUsed
#[test]
fn weapon_to_use_keeps_the_spelling_the_file_used() {
    for (stored, expected) in [(0, 0), (-1, -1), (0x4000_0011, 0x4000_0011)] {
        let mut database = defaults::create();
        monsters(&mut database).rows[0].cells[3] = VtankCell::int(stored);
        let rules = read(&database);
        write(&mut database, &rules);
        assert_eq!(
            monsters(&mut database).rows[0].cells[3].as_int(),
            Ok(expected)
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankMonsterRuleTableTests.cs :: AnUnparseableSpecIsLoggedAndIgnoredInsteadOfLosingTheProfile
#[test]
fn an_unparseable_spec_is_logged_and_ignored_instead_of_losing_the_profile() {
    let mut database = defaults::create();
    monsters(&mut database).rows[0].cells[0] = VtankCell::string("(((");
    let mut warnings: Vec<String> = Vec::new();
    let mut warn = |warning: &str| warnings.push(warning.to_owned());

    let rules = monster_table::try_read(&database, Some(&mut warn))
        .unwrap()
        .unwrap();

    assert_eq!(rules.len(), 1);
    let rule = &rules[0];
    assert_eq!(rule.expression(), "(((");
    assert!(rule.is_ignored_spec());
    let context = MonsterExpressionContext {
        name: "Drudge",
        type_id: 1,
        species: "Drudge",
        maximum_health: 100,
        range: 5.0,
        has_shield: false,
        meta_state: "",
        setting: None,
    };
    assert_eq!(rule.matches(&context), Ok(false));
    assert!(
        warnings
            .iter()
            .any(|w| w.starts_with("Parse error in monster spec: \"(((\", ignoring entry ("))
    );

    // And the row is still the row: it round-trips like any other.
    write(&mut database, &rules);
    assert_eq!(monsters(&mut database).rows[0].cells[0].as_string(), "(((");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankMonsterRuleTableTests.cs :: LoadKeepsEverySettingWhenOneMonsterSpecIsUnparseable
#[test]
fn load_keeps_every_setting_when_one_monster_spec_is_unparseable() {
    let mut seed = defaults::create();
    monsters(&mut seed).rows[0].cells[0] = VtankCell::string("(((");
    let mut settings = AllSettings::default();
    let mut warnings: Vec<String> = Vec::new();
    let mut warn = |warning: &str| warnings.push(warning.to_owned());

    profile::load(&seed.render(), &mut settings, Some(&mut warn)).unwrap();

    assert!(!warnings.is_empty());
    assert_eq!(settings.combat.rules.len(), 1);
    assert_eq!(settings.combat.rules[0].expression(), "(((");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankMonsterRuleTableTests.cs :: EveryColumnSurvivesAWriteThenReadCycle
#[test]
fn every_column_survives_a_write_then_read_cycle() {
    let all_ticks = MonsterActionFlags::IMPERIL
        | MonsterActionFlags::VULNERABILITY
        | MonsterActionFlags::YIELD
        | MonsterActionFlags::GRAVITY_WELL
        | MonsterActionFlags::RING
        | MonsterActionFlags::BROADSIDE
        | MonsterActionFlags::FESTER
        | MonsterActionFlags::WEAKENING_CURSE
        | MonsterActionFlags::FESTERING_CURSE
        | MonsterActionFlags::CORRUPTION
        | MonsterActionFlags::DESTRUCTIVE_CURSE
        | MonsterActionFlags::CORROSION
        | MonsterActionFlags::STREAK;
    let rules = vec![
        rule(
            "DEFAULT",
            MonsterRuleActions {
                flags: MonsterActionFlags::ATTACK,
                priority: 1,
                ..MonsterRuleActions::default()
            },
        ),
        rule(
            "species==drudge",
            MonsterRuleActions {
                flags: all_ticks,
                priority: 4,
                damage_type: MonsterDamageType::Electric,
                extra_vulnerability: MonsterDamageType::Acid,
                pet_damage_type: MonsterDamageType::Cold,
                weapon_object_id: 0x4000_0011,
                offhand_object_id: 0x4000_0022,
                ..MonsterRuleActions::default()
            },
        ),
        rule(
            "name#^Olthoi",
            MonsterRuleActions {
                flags: MonsterActionFlags::IMPERIL,
                priority: -1,
                damage_type: MonsterDamageType::VoidBasic,
                ..MonsterRuleActions::default()
            },
        ),
    ];
    let mut database = defaults::create();

    write(&mut database, &rules);
    let read = read(&database);

    assert_eq!(read.len(), 3);
    for (written, read) in rules.iter().zip(&read) {
        let (a, b) = (written.actions(), read.actions());
        assert_eq!(a.flags, b.flags);
        assert_eq!(a.priority, b.priority);
        assert_eq!(a.damage_type, b.damage_type);
        assert_eq!(a.extra_vulnerability, b.extra_vulnerability);
        assert_eq!(a.pet_damage_type, b.pet_damage_type);
        assert_eq!(a.weapon_object_id, b.weapon_object_id);
        assert_eq!(a.offhand_object_id, b.offhand_object_id);
    }
    assert_eq!(read[0].expression(), monster_table::DEFAULT_ROW_NAME);
    assert_eq!(read[1].expression(), "species==drudge");
    assert_eq!(read[2].expression(), "name#^Olthoi");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankMonsterRuleTableTests.cs :: TheAttackColumnIsStoredTheSameWayTheTickBoxShowsIt
#[test]
fn the_attack_column_is_stored_the_same_way_the_tick_box_shows_it() {
    let mut database = defaults::create();
    write(
        &mut database,
        &[
            rule("DEFAULT", with_flags(MonsterActionFlags::ATTACK)),
            rule("nope", with_flags(MonsterActionFlags::NONE)),
        ],
    );
    let table = monsters(&mut database);
    assert!(table.rows[0].cells[8].as_bool());
    assert!(!table.rows[1].cells[8].as_bool());

    let read = read(&database);
    assert!(read[0].actions().uses_primary_attack());
    assert!(!read[1].actions().uses_primary_attack());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankMonsterRuleTableTests.cs :: ARangeKeyedRowThatOnlyDebuffsDoesNotAttack
#[test]
fn a_range_keyed_row_that_only_debuffs_does_not_attack() {
    let mut database = defaults::create();
    let table = monsters(&mut database);
    let mut distant: VtankRow = table.rows[0].clone();
    distant.cells[0] = VtankCell::string("range>5");
    distant.cells[4] = VtankCell::bool(true);
    distant.cells[8] = VtankCell::bool(false);
    table.rows.push(distant);

    let rules = read(&database);

    assert!(rules[0].is_default());
    assert!(rules[0].actions().uses_primary_attack());
    assert_eq!(rules[1].expression(), "range>5");
    assert!(!rules[1].actions().uses_primary_attack());
    assert_eq!(
        rules[1].actions().flags & MonsterActionFlags::IMPERIL,
        MonsterActionFlags::IMPERIL
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankMonsterRuleTableTests.cs :: AFreshMonsterRowIsWrittenColumnForColumn
#[test]
fn a_fresh_monster_row_is_written_column_for_column() {
    let mut database = defaults::create();
    write(&mut database, &[MonsterRule::fresh("DEFAULT").unwrap()]);

    let row = &monsters(&mut database).rows[0];
    assert_eq!(row.cells[0].as_string(), monster_table::DEFAULT_ROW_NAME);
    assert_eq!(row.cells[1].as_int(), Ok(1));
    assert_eq!(row.cells[2].as_int(), Ok(8));
    assert_eq!(row.cells[3].as_int(), Ok(-1));
    assert!(row.cells[8].as_bool());
    assert!(row.cells[17].as_bool());
    for column in [4, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 16] {
        assert!(!row.cells[column].as_bool(), "column {column}");
    }
    assert_eq!(row.cells[18].as_int(), Ok(98));
    assert_eq!(row.cells[19].as_int(), Ok(0));
    assert_eq!(row.cells[20].as_int(), Ok(101));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankMonsterRuleTableTests.cs :: AnUntouchedFreshRowDoesNotFabricateAMonsterTable
#[test]
fn an_untouched_fresh_row_does_not_fabricate_a_monster_table() {
    let mut database = VtankDatabase::default();
    write(&mut database, &[MonsterRule::fresh("DEFAULT").unwrap()]);
    assert!(database.find(monster_table::TABLE_NAME).is_none());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankMonsterRuleTableTests.cs :: AnEditedLoneDefaultRowIsWrittenEvenWithoutAnExistingTable
#[test]
fn an_edited_lone_default_row_is_written_even_without_an_existing_table() {
    let mut database = VtankDatabase::default();
    write(
        &mut database,
        &[rule(
            "DEFAULT",
            MonsterRuleActions {
                damage_type: MonsterDamageType::Fire,
                ..MonsterRuleActions::fresh_row()
            },
        )],
    );
    assert_eq!(monsters(&mut database).rows[0].cells[2].as_int(), Ok(6));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankMonsterRuleTableTests.cs :: ADropInVtankProfilesRulesBecomeLive
#[test]
fn a_drop_in_vtank_profiles_rules_become_live() {
    let mut source = defaults::create();
    write(
        &mut source,
        &[
            rule("DEFAULT", with_flags(MonsterActionFlags::ATTACK)),
            rule(
                "name#^Olthoi",
                MonsterRuleActions {
                    flags: MonsterActionFlags::ATTACK | MonsterActionFlags::IMPERIL,
                    priority: 4,
                    damage_type: MonsterDamageType::Acid,
                    ..MonsterRuleActions::default()
                },
            ),
        ],
    );
    let mut settings = AllSettings::default();

    profile::load(&source.render(), &mut settings, None).unwrap();

    let rules = &settings.combat.rules;
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[1].expression(), "name#^Olthoi");
    assert_eq!(rules[1].priority(), 4);
    assert_eq!(rules[1].actions().damage_type, MonsterDamageType::Acid);
}

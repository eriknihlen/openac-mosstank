//! The game database's damage tables and every table shape, against the
//! C# tests' cases and the database excerpt fixture.

use std::cell::RefCell;

use mosstank_data::damage::MonsterDamageType;
use mosstank_formats::fixtures;
use mosstank_formats::gameinfo::{ProfileText, VtankGameInfoDatabase, VtankSpeciesMember};

fn excerpt_text() -> String {
    fixtures::read_vtank("gameinfodb-excerpt.ugd")
}

fn fixture() -> VtankGameInfoDatabase {
    VtankGameInfoDatabase::parse(&excerpt_text()).unwrap()
}

struct EmptyStorage;

impl ProfileText for EmptyStorage {
    fn read_text(&self, _key: &str) -> Option<String> {
        None
    }
}

struct TextStorage {
    text: String,
    last_key: RefCell<Option<String>>,
}

impl TextStorage {
    fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            last_key: RefCell::new(None),
        }
    }
}

impl ProfileText for TextStorage {
    fn read_text(&self, key: &str) -> Option<String> {
        *self.last_key.borrow_mut() = Some(key.to_owned());
        Some(self.text.clone())
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankDamageDatabaseTests.cs :: ExactMonsterOverrideWinsOverSpecies
#[test]
fn exact_monster_override_wins_over_species() {
    assert_eq!(
        fixture().damage_preferences("Magma Golem"),
        [
            MonsterDamageType::Cold,
            MonsterDamageType::Slash,
            MonsterDamageType::Fire
        ]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankDamageDatabaseTests.cs :: SpeciesMembersResolveThroughSpeciesDamages
#[test]
fn species_members_resolve_through_species_damages() {
    let database = fixture();
    assert_eq!(
        database.damage_preferences("Olthoi Slasher"),
        [
            MonsterDamageType::Bludgeon,
            MonsterDamageType::Slash,
            MonsterDamageType::Pierce,
            MonsterDamageType::Fire,
            MonsterDamageType::Cold,
            MonsterDamageType::Acid,
            MonsterDamageType::Electric,
        ]
    );
    assert_eq!(
        database.damage_preferences("Olthoi Slasher"),
        database.damage_preferences("olthoi slasher")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankDamageDatabaseTests.cs :: UnknownMonsterHasNoAutoElementAtAll
#[test]
fn unknown_monster_has_no_auto_element_at_all() {
    assert!(
        fixture()
            .damage_preferences("New Server Creature")
            .is_empty()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankDamageDatabaseTests.cs :: NoFileMeansTheShippedDefault
#[test]
fn no_file_means_the_shipped_default() {
    // The reference reads the database it ships inside itself when the
    // profile directory has none; every table it ships is empty.
    let database = VtankGameInfoDatabase::load(&EmptyStorage);
    assert!(database.is_loaded);
    assert!(database.damage_preferences("Magma Golem").is_empty());
    assert_eq!(database.monster_damage_override_count(), 0);
    assert_eq!(database.species_member_count(), 0);
    assert!(database.species_damages.is_empty());
    assert!(database.ammunition_options.is_empty());
    assert!(database.heal_kits.is_empty());
    assert!(database.grenade_options.is_empty());
    assert!(database.drain_spell_options.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankDamageDatabaseTests.cs :: AFileTheGrammarRejectsFallsBackToTheShippedDefault
#[test]
fn a_file_the_grammar_rejects_falls_back_to_the_shipped_default() {
    let database = VtankGameInfoDatabase::load(&TextStorage::new("this is not a ugd"));
    assert!(database.is_loaded);
    assert!(database.damage_preferences("Magma Golem").is_empty());
    assert!(database.ammunition_options.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankDamageDatabaseTests.cs :: TheFileIsReadFromTheProfileDirectoryByItsAuthenticName
#[test]
fn the_file_is_read_from_the_profile_directory_by_its_authentic_name() {
    let storage = TextStorage::new(&excerpt_text());
    let database = VtankGameInfoDatabase::load(&storage);
    assert_eq!(storage.last_key.borrow().as_deref(), Some("gameinfodb.ugd"));
    assert!(database.is_loaded);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/VtankDamageDatabaseTests.cs :: EveryTableShapeParses
#[test]
#[allow(clippy::float_cmp)]
fn every_table_shape_parses() {
    let database = fixture();

    let ammo = &database.ammunition_options[0];
    assert_eq!(ammo.name, "Fixture Quarrel");
    assert_eq!(ammo.launcher_type, 6);
    assert_eq!(ammo.wield_requirement, 0);
    assert_eq!(ammo.element, 0);
    assert_eq!(ammo.quality, 5);

    assert_eq!(
        database.species_member("Olthoi Slasher"),
        Some(VtankSpeciesMember {
            species: 1,
            maximum_health: 2000
        })
    );

    let kit = &database.heal_kits[0];
    assert_eq!(kit.name, "Fixture Healing Kit");
    assert_eq!(kit.restore_bonus, 1.25);
    assert_eq!(kit.skill_bonus, 40);
    assert_eq!(kit.vital, 1);

    let grenade = &database.grenade_options[0];
    assert_eq!(grenade.name, "Iron Phial of Imperil");
    assert_eq!(grenade.wield_requirement_type, 2);
    assert_eq!(grenade.wield_requirement_attribute, 38);
    assert_eq!(grenade.wield_requirement_value, 60);
    assert_eq!(grenade.spell_id, 1323);
    assert_eq!(grenade.spellcraft, 110);

    let drain = &database.drain_spell_options[0];
    assert_eq!(drain.spell_id, 1237);
    assert_eq!(drain.cast_time_milliseconds, 400);
    assert_eq!(drain.enemy_drain_factor, 0.2);
    assert_eq!(drain.enemy_drain_maximum_points, 25);
    assert_eq!(drain.result_multiplier, 2.0);

    let martyr = &database.martyr_spell_options[0];
    assert_ne!(martyr.spell_id, 0);
    assert!(martyr.cast_time_milliseconds > 0);
}

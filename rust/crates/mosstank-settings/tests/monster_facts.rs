//! The monster facts a rule reads from the game database, against the C#
//! tests' cases and the database excerpt fixture.

use ac_plugin_abi::combat::CombatTarget;
use mosstank_formats::fixtures;
use mosstank_formats::gameinfo::VtankGameInfoDatabase;
use mosstank_settings::model::CombatSettings;
use mosstank_settings::monster::{MonsterFactTable, MonsterRule};

fn fixture() -> VtankGameInfoDatabase {
    VtankGameInfoDatabase::parse(&fixtures::read_vtank("gameinfodb-excerpt.ugd")).unwrap()
}

fn target(name: &str, species_id: i32, species_word: &str) -> CombatTarget {
    CombatTarget {
        object_id: 0x5000_0001,
        name: name.to_owned(),
        weenie_class_id: 42,
        distance: 3.0,
        relative_angle_degrees: 0.0,
        is_health_known: true,
        health_fraction: 1.0,
        species_id,
        species_name: species_word.to_owned(),
        ..CombatTarget::default()
    }
}

fn settings_with(facts: MonsterFactTable, rules: &[(&str, i32)]) -> CombatSettings {
    CombatSettings {
        monster_facts: facts,
        rules: rules
            .iter()
            .map(|&(expression, priority)| {
                MonsterRule::with_priority(expression, priority).unwrap()
            })
            .collect(),
        ..CombatSettings::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MonsterFactTableTests.cs :: MaximumHealthComesFromTheDatabaseNotTheLiveObject
#[test]
fn maximum_health_comes_from_the_database_not_the_live_object() {
    let settings = settings_with(
        MonsterFactTable::new(fixture()),
        &[("maxhp>500", 4), ("DEFAULT", 1)],
    );
    let priority = |name, species, word| {
        settings
            .resolve_rule(&target(name, species, word))
            .priority()
    };
    assert_eq!(priority("Olthoi Slasher", 1, "Olthoi"), 4);
    assert_eq!(priority("Black Rabbit", 25, "Rabbit"), 1);
    assert_eq!(priority("Drudge Prowler", 4, "Drudge"), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MonsterFactTableTests.cs :: AnUnlistedMonsterHasNoMaximumHealthAtAll
#[test]
fn an_unlisted_monster_has_no_maximum_health_at_all() {
    let settings = settings_with(
        MonsterFactTable::new(fixture()),
        &[("maxhp<0", 4), ("DEFAULT", 1)],
    );
    // An unlisted row answers -1, which the host figure never can.
    assert_eq!(
        settings
            .resolve_rule(&target("Drudge Prowler", 4, "Drudge"))
            .priority(),
        4
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MonsterFactTableTests.cs :: SpeciesComesFromTheDatabaseNotTheLiveProperty
#[test]
fn species_comes_from_the_database_not_the_live_property() {
    let mut settings = settings_with(
        MonsterFactTable::new(fixture()),
        &[("species==Olthoi", 4), ("DEFAULT", 1)],
    );
    // The client's word for species 1 is learned from a live monster.
    settings.monster_facts.learn(1, Some("Olthoi"));
    assert_eq!(
        settings
            .resolve_rule(&target("Olthoi Slasher", 1, "Olthoi"))
            .priority(),
        4
    );
    // Same live species, but the database does not list this monster.
    assert_eq!(
        settings
            .resolve_rule(&target("Olthoi Eviscerator", 1, "Olthoi"))
            .priority(),
        1
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MonsterFactTableTests.cs :: WithoutADatabaseNothingHasASpeciesOrAMaximumHealth
#[test]
fn without_a_database_nothing_has_a_species_or_a_maximum_health() {
    let settings = settings_with(
        MonsterFactTable::default(),
        &[("species==Olthoi", 4), ("maxhp>=0", 3), ("DEFAULT", 1)],
    );
    assert_eq!(
        settings
            .resolve_rule(&target("Olthoi Slasher", 1, "Olthoi"))
            .priority(),
        1
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MonsterFactTableTests.cs :: TheImmunityMaskIsRead
#[test]
fn the_immunity_mask_is_read() {
    let facts = MonsterFactTable::new(fixture());
    assert!(facts.is_immune_to_magic("Olthoi Slasher"));
    assert!(!facts.is_immune_to_magic("Black Rabbit"));
    assert!(!MonsterFactTable::default().is_immune_to_magic("Olthoi Slasher"));
}

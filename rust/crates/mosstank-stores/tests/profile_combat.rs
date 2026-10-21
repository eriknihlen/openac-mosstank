//! The profile store's combat-section cases that the C# keeps among the
//! fight's tests (`CombatControllerTests`): the arc default on load, the
//! imported tables a new profile drops, and the Items page's ids across a
//! reload.

use mosstank_dotnet::collections::{IgnoreCaseStringSet, OrdinalStringSet};
use mosstank_settings::model::{
    AllSettings, AssistItem, CombatSettings, ConsumableCategory, UseArcsMode,
};
use mosstank_stores::profile_store::{BY_CHARACTER, ProfileLoad, ProfileStore};
use mosstank_stores::storage::{MemoryHost, PluginStorage};

fn host() -> MemoryHost {
    MemoryHost::on("Fixture")
}

fn bound(host: &mut MemoryHost) -> ProfileStore {
    let mut store = ProfileStore::new(host);
    store.bind_character(host, Some("Acdream")).unwrap();
    store
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: AProfileWithNoArcSettingLoadsWithArcsOff
#[test]
fn a_profile_with_no_arc_setting_loads_with_arcs_off() {
    let mut host = host();
    host.plugin
        .write_text("profile.json", r#"{ "combat": { "maximumRange": 5.0 } }"#)
        .unwrap();
    let mut store = bound(&mut host);
    let mut settings = AllSettings::default();

    store
        .load_current(
            &mut host,
            &mut settings,
            &mut OrdinalStringSet::new(),
            &mut IgnoreCaseStringSet::new(),
        )
        .unwrap();

    assert_eq!(settings.combat.use_arcs, UseArcsMode::NO);
    assert!((settings.combat.arc_range - 5.0).abs() < 1e-6);

    // The other road: nothing combat-shaped is stored at all, so the load
    // falls back on the settings object's own declared default.
    let mut bare_host = host_with_bare_profile();
    let mut bare_store = bound(&mut bare_host);
    let mut bare = AllSettings {
        combat: CombatSettings {
            use_arcs: UseArcsMode::YES,
            ..CombatSettings::default()
        },
        ..AllSettings::default()
    };

    bare_store
        .load_current(
            &mut bare_host,
            &mut bare,
            &mut OrdinalStringSet::new(),
            &mut IgnoreCaseStringSet::new(),
        )
        .unwrap();

    assert_eq!(bare.combat.use_arcs, UseArcsMode::NO);
}

fn host_with_bare_profile() -> MemoryHost {
    let mut host = host();
    host.plugin.write_text("profile.json", "{ }").unwrap();
    host
}

fn stale_imported_tables(combat: &mut CombatSettings) {
    combat.item_use_specifiers.insert(901, 1);
    combat.imported_assist_items.push(AssistItem {
        name: "Stale Pea".to_owned(),
        category: ConsumableCategory::Pea,
    });
    combat.combat_item_object_ids.insert(901);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: SeedingFromTheDefaultsDropsThePreviousImportedTables
#[test]
fn seeding_from_the_defaults_drops_the_previous_imported_tables() {
    let mut host = host();
    let mut store = bound(&mut host);
    let mut settings = AllSettings::default();
    let mut no_buffs = OrdinalStringSet::new();
    let mut logs = IgnoreCaseStringSet::new();

    // No file for this character yet: the load seeds from the defaults.
    stale_imported_tables(&mut settings.combat);
    assert_eq!(
        store
            .load_current(&mut host, &mut settings, &mut no_buffs, &mut logs)
            .unwrap(),
        ProfileLoad::Missing
    );
    assert!(settings.combat.item_use_specifiers.is_empty());
    assert!(settings.combat.imported_assist_items.is_empty());
    assert!(settings.combat.combat_item_object_ids.is_empty());

    // And a brand new profile, which seeds from the same defaults.
    stale_imported_tables(&mut settings.combat);
    let (created, _) = store
        .create(
            &mut host,
            Some("Empty"),
            false,
            &mut settings,
            &mut no_buffs,
            &mut logs,
        )
        .unwrap();
    assert!(created);
    assert!(settings.combat.item_use_specifiers.is_empty());
    assert!(settings.combat.imported_assist_items.is_empty());
    assert!(settings.combat.combat_item_object_ids.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatControllerTests.cs :: ProfileReloadKeepsUsdItemIdsAcrossSidecarAndSwitches
#[test]
fn profile_reload_keeps_usd_item_ids_across_sidecar_and_switches() {
    let mut host = host();
    let mut store = bound(&mut host);
    let mut settings = AllSettings::default();
    let mut no_buffs = OrdinalStringSet::new();
    let mut logs = IgnoreCaseStringSet::new();
    settings
        .combat
        .combat_item_names
        .insert("Same Wand".to_owned());
    settings
        .combat
        .combat_item_order
        .push("Same Wand".to_owned());
    settings.combat.combat_item_object_ids.insert(801);
    settings.combat.combat_item_object_ids.insert(802);
    settings.combat.combat_item_order_ids.push(802);
    settings.combat.combat_item_order_ids.push(801);
    store
        .save_current(&mut host, &mut settings, &no_buffs, &logs)
        .unwrap();

    settings.combat.combat_item_object_ids.clear();
    settings.combat.combat_item_order_ids.clear();
    store
        .load_current(&mut host, &mut settings, &mut no_buffs, &mut logs)
        .unwrap();
    assert_eq!(settings.combat.combat_item_order_ids, [802, 801]);
    assert_eq!(settings.combat.combat_item_order, ["Same Wand"]);

    let (created, _) = store
        .create(
            &mut host,
            Some("Empty"),
            false,
            &mut settings,
            &mut no_buffs,
            &mut logs,
        )
        .unwrap();
    assert!(created);
    assert!(settings.combat.combat_item_order_ids.is_empty());
    assert!(store.select(&mut host, Some(BY_CHARACTER)).unwrap());
    store
        .load_current(&mut host, &mut settings, &mut no_buffs, &mut logs)
        .unwrap();
    assert_eq!(settings.combat.combat_item_order_ids, [802, 801]);
    assert!(store.select(&mut host, Some("Empty")).unwrap());
    store
        .load_current(&mut host, &mut settings, &mut no_buffs, &mut logs)
        .unwrap();
    assert!(settings.combat.combat_item_order_ids.is_empty());
}

/// An old profile has no hand choices: migrating one leaves none of the
/// previously loaded profile's behind, as it leaves none of its item ids.
#[test]
fn migrating_an_old_profile_drops_the_previous_hand_choices() {
    let mut settings = AllSettings::default();
    stale_imported_tables(&mut settings.combat);
    let old = mosstank_stores::legacy::LegacyProfileDocument::read("{}")
        .unwrap()
        .unwrap();
    old.apply(
        &mut settings.combat,
        &mut settings.buffs,
        &mut settings.vitals,
        &mut settings.inventory,
        &mut OrdinalStringSet::new(),
        &mut |_| {},
    );
    assert!(settings.combat.combat_item_object_ids.is_empty());
    assert!(settings.combat.item_use_specifiers.is_empty());
}

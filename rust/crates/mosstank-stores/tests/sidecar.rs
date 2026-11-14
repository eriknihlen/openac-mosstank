//! The side-car document: no property shadows a VTank setting, and a
//! hand-edited side-car the C# would trip over reads as the register says.

use mosstank_dotnet::collections::{IgnoreCaseStringSet, OrdinalStringSet};
use mosstank_settings::model::AllSettings;
use mosstank_settings::options;
use mosstank_stores::sidecar::SideCarDocument;
use openac_plugin_compat::json::JsonValue;
use openac_plugin_compat::text::ordinal;

fn property_names() -> Vec<String> {
    let JsonValue::Object(members) = SideCarDocument::create_defaults().to_json() else {
        panic!("a side-car is an object");
    };
    let mut names: Vec<String> = members.into_iter().map(|(name, _)| name).collect();
    // Left out of the file while null, but a property all the same.
    names.push("NavigationWalkLegsWithClient".to_owned());
    names
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankProfileStoreSideCarTests.cs :: SideCarDocumentHasNoFieldNamedForARealVtankSetting
#[test]
fn side_car_document_has_no_field_named_for_a_real_vtank_setting() {
    let known = |name: &str| {
        options::NAMES
            .iter()
            .any(|option| ordinal::equals_ignore_case(option, name))
    };
    let mut overlaps = Vec::new();
    for name in property_names() {
        if known(&name) {
            overlaps.push(name);
            continue;
        }
        for prefix in ["Combat", "Buff", "Vitals", "Inventory"] {
            if name.len() > prefix.len() && name.starts_with(prefix) && known(&name[prefix.len()..])
            {
                overlaps.push(name.clone());
                break;
            }
        }
    }
    assert!(overlaps.is_empty(), "{overlaps:?}");
}

/// Null lists and null entries, where the C# would throw out of the load,
/// read as empty and are skipped (register row STORE-5).
#[test]
fn null_lists_and_entries_read_as_nothing() {
    let document = SideCarDocument::read(
        "{\"ItemNames\":null,\"ItemNamesById\":null,\"BuffItemEnchantRows\":[null,{\"ItemName\":\"Bow\",\"SpellName\":null}],\"InventoryLootRules\":[null],\"CombatDynamicSettings\":{\"X\":null}}",
    )
    .unwrap()
    .unwrap();
    let mut settings = AllSettings::default();
    let mut no_buff = OrdinalStringSet::new();
    let mut channels = IgnoreCaseStringSet::new();
    document.apply(&mut settings, &mut no_buff, &mut channels, &mut |_| {});

    assert!(settings.combat.combat_item_names.is_empty());
    assert_eq!(settings.buffs.item_enchant_rows.len(), 1);
    assert_eq!(settings.buffs.item_enchant_rows[0].spell_name, "");
    assert!(settings.inventory.loot.rules.is_empty());
    assert!(settings.combat.dynamic_settings.is_empty());
}

/// An enum number this build has no member for refuses the document, so
/// the load falls back to the defaults (register row STORE-5); a value of
/// the wrong JSON kind refuses it as System.Text.Json does.
#[test]
fn an_unknown_enum_number_or_a_wrong_kind_refuses_the_document() {
    assert!(SideCarDocument::read("{\"ConsumableCategories\":{\"Pea\":99}}").is_err());
    assert!(SideCarDocument::read("{\"InventoryLootRules\":[{\"Action\":42}]}").is_err());
    assert!(SideCarDocument::read("{\"ShowNavLines\":\"yes\"}").is_err());
    assert!(SideCarDocument::read("{\"Version\":1.5}").is_err());
    assert!(SideCarDocument::read("null").unwrap().is_none());
}

/// Property names match ignoring case, a later duplicate wins, and a
/// property the document lacks keeps its default.
#[test]
fn properties_match_ignoring_case_and_the_last_duplicate_wins() {
    let document = SideCarDocument::read(
        "{\"combatattackpower\":0.25,\"CombatAttackPower\":0.75,\"navigationclientpathing\":\"never\"}",
    )
    .unwrap()
    .unwrap();
    assert!((document.combat_attack_power - 0.75).abs() < f32::EPSILON);
    assert!(document.buff_attributes);
    let mut settings = AllSettings::default();
    document.apply(
        &mut settings,
        &mut OrdinalStringSet::new(),
        &mut IgnoreCaseStringSet::new(),
        &mut |_| {},
    );
    assert_eq!(
        settings.navigation.client_pathing,
        mosstank_formats::route::ClientPathing::Never
    );
}

/// The own death corpse switch (openac-mosstank#28) is written to the
/// side-car beside the rare walk, read back, and on when the file predates
/// it, as the C#'s `InventoryLootOwnDeathCorpse` property is.
#[test]
fn the_own_death_corpse_switch_round_trips_and_is_on_when_absent() {
    let apply = |text: &str| {
        let mut settings = AllSettings::default();
        SideCarDocument::read(text).unwrap().unwrap().apply(
            &mut settings,
            &mut OrdinalStringSet::new(),
            &mut IgnoreCaseStringSet::new(),
            &mut |_| {},
        );
        settings.inventory.loot.loot_own_death_corpse
    };
    let mut settings = AllSettings::default();
    settings.inventory.loot.loot_own_death_corpse = false;
    let text = SideCarDocument::capture_and_write(
        &settings,
        &OrdinalStringSet::new(),
        &IgnoreCaseStringSet::new(),
    )
    .unwrap();

    assert!(text.contains("\"InventoryLootOwnDeathCorpse\": false"));
    assert!(!apply(&text));
    assert!(apply("{}"));
}

fn applied(text: &str) -> AllSettings {
    let mut settings = AllSettings::default();
    SideCarDocument::read(text).unwrap().unwrap().apply(
        &mut settings,
        &mut OrdinalStringSet::new(),
        &mut IgnoreCaseStringSet::new(),
        &mut |_| {},
    );
    settings
}

/// New: helping the network peers is on by default, and a side-car written
/// before it could be set (version 1, which always says off) reads as on,
/// once: the next save writes version 2, and an off written then stays off.
#[test]
fn a_side_car_older_than_version_2_helps_the_network_peers() {
    assert!(
        applied("{\"Version\":1,\"VitalsHelpNetworkPeers\":false}")
            .vitals
            .help_network_peers
    );
    assert!(
        applied("{\"VitalsHelpNetworkPeers\":false}")
            .vitals
            .help_network_peers
    );
    assert!(applied("{}").vitals.help_network_peers);

    let mut settings = AllSettings::default();
    settings.vitals.help_network_peers = false;
    let text = SideCarDocument::capture_and_write(
        &settings,
        &OrdinalStringSet::new(),
        &IgnoreCaseStringSet::new(),
    )
    .unwrap();
    assert!(text.contains("\"Version\": 2"));
    assert!(!applied(&text).vitals.help_network_peers);
}

/// New: the focus and overlay switches are kept in the side-car; focus is
/// on and the overlay off when a file predates them.
#[test]
fn the_team_focus_and_overlay_switches_round_trip() {
    let mut settings = AllSettings::default();
    settings.combat.team_focus_debuffed = false;
    settings.vitals.team_overlay = true;
    let text = SideCarDocument::capture_and_write(
        &settings,
        &OrdinalStringSet::new(),
        &IgnoreCaseStringSet::new(),
    )
    .unwrap();

    let read = applied(&text);
    assert!(!read.combat.team_focus_debuffed);
    assert!(read.vitals.team_overlay);
    let fresh = applied("{}");
    assert!(fresh.combat.team_focus_debuffed);
    assert!(!fresh.vitals.team_overlay);
}

//! UtilityBelt's ItemInfo: selecting an object writes its name, the loot
//! rule it matches and its stats in chat.

mod common;

use ac_plugin_abi::items::{InventoryItem, ItemStatus};
use ac_plugin_abi::magic::SpellInfo;
use ac_plugin_abi::world::{
    ArmorProfile, ItemProperties, ObjectClass, WeaponProfile, WorldObject, selection_source,
};
use common::Rig;
use mosstank_settings::ub::UbSettingValue;
use mosstank_ub::catalog::UbCatalog;
use mosstank_ub::tools::item_info::{self, APPRAISAL_TIMEOUT_SECONDS};

const SILVER: u32 = 63;

fn switch_on(rig: &mut Rig, name: &str) {
    set_switch(rig, name, true);
}

fn set_switch(rig: &mut Rig, name: &str, on: bool) {
    let row = UbCatalog::require(&rig.catalog, name);
    rig.catalog.set(&row, UbSettingValue::from_bool(on));
}

fn enabled() -> Rig {
    let mut rig = Rig::new();
    switch_on(&mut rig, item_info::ENABLED_SETTING);
    switch_on(&mut rig, item_info::DESCRIBE_ON_SELECT_SETTING);
    rig
}

fn spell(id: u32, name: &str, family: u32, difficulty: i32) -> (u32, SpellInfo) {
    (
        id,
        SpellInfo {
            spell_id: id,
            name: name.to_owned(),
            family,
            difficulty,
            ..SpellInfo::default()
        },
    )
}

/// Puts an object the character carries in the world, with its appraisal.
fn carry(rig: &mut Rig, object: WorldObject, item: InventoryItem, properties: ItemProperties) {
    rig.host
        .meta
        .properties
        .insert(object.object_id, properties);
    rig.host.meta.objects.push(object);
    rig.host.meta.owned_items.push(item);
}

fn breastplate(appraised: bool) -> (WorldObject, InventoryItem, ItemProperties) {
    let object = WorldObject {
        object_id: 0x8000_0010,
        name: "Breastplate".to_owned(),
        object_class: ObjectClass::Armor,
        item_type: 2,
        is_owned: true,
        container_id: 1,
        has_appraisal_data: appraised,
        stack_size: 1,
        spell_ids: vec![2592, 1486, 4407, 5000],
        ..WorldObject::default()
    };
    let item = InventoryItem {
        object_id: object.object_id,
        name: object.name.clone(),
        item_type: 2,
        container_id: 1,
        stack_size: 1,
        material_type: SILVER,
        appraised_spell_ids: object.spell_ids.clone(),
        ..InventoryItem::default()
    };
    let properties = ItemProperties {
        // AL, imbue (armor rending), tinks, workmanship, set (Soldier's),
        // wield level 150, damage rating 2.
        ints: vec![
            (28, 512),
            (179, 4),
            (171, 2),
            (105, 7),
            (265, 13),
            (158, 7),
            (159, 1),
            (160, 150),
            (370, 2),
            (131, SILVER as i32),
        ],
        armor: Some(ArmorProfile {
            armor_level: 512,
            ..ArmorProfile::default()
        }),
        ..ItemProperties::default()
    };
    (object, item, properties)
}

fn breastplate_spells(rig: &mut Rig) {
    rig.host.meta.spells.extend([
        spell(2592, "Major Impenetrability", 160, 0),
        spell(1486, "Impenetrability VI", 160, 250),
        spell(4407, "Incantation of Impenetrability", 160, 400),
        spell(5000, "Epic Strength", 1, 0),
    ]);
}

/// A selection made with a key.
fn select(rig: &mut Rig, object_id: u32) {
    select_by(rig, object_id, selection_source::KEYBOARD);
}

fn select_by(rig: &mut Rig, object_id: u32, source: u8) {
    rig.with_call(|call, commands| {
        commands
            .tools_mut()
            .item_info
            .on_selection_changed(call, Some(object_id), source);
    });
}

fn tick(rig: &mut Rig, seconds: f64) {
    rig.with_call(|call, commands| commands.tools_mut().item_info.tick(call, seconds));
}

#[test]
fn nothing_is_said_until_it_is_switched_on() {
    let mut rig = Rig::new();
    let (object, item, properties) = breastplate(true);
    let id = object.object_id;
    carry(&mut rig, object, item, properties);

    select(&mut rig, id);

    assert!(rig.messages().is_empty(), "{:?}", rig.messages());
}

#[test]
fn selecting_an_appraised_item_names_the_rule_it_matches_and_its_stats() {
    let mut rig = enabled();
    let (object, item, properties) = breastplate(true);
    let id = object.object_id;
    carry(&mut rig, object, item, properties);
    breastplate_spells(&mut rig);
    "Keep armor".clone_into(&mut rig.services.loot_rule_name);

    select(&mut rig, id);

    assert_eq!(
        rig.single_message(),
        "[UB] ItemInfo: Keep armor - Silver Breastplate, Soldier's Set, AL 512, AR, Tinks 2, \
         Epic Strength, Major Impenetrability, Wield Lvl 150, Craft 7, [D 2] "
    );
    assert!(rig.host.identified.is_empty());
    // The looter was asked about the item described.
    assert_eq!(rig.services.loot_rule_asked, [id]);
}

#[test]
fn an_item_no_rule_names_is_named_alone() {
    let mut rig = enabled();
    let (object, item, properties) = breastplate(true);
    let id = object.object_id;
    carry(&mut rig, object, item, properties);

    select(&mut rig, id);

    // The client shows a plugin's tags as written, so the name is plain
    // text, not UtilityBelt's select link.
    assert!(
        rig.single_message()
            .starts_with("[UB] ItemInfo: Silver Breastplate, "),
        "{}",
        rig.single_message()
    );
}

#[test]
fn an_item_not_yet_appraised_is_appraised_and_described_when_the_answer_comes() {
    let mut rig = enabled();
    let (object, item, properties) = breastplate(false);
    let id = object.object_id;
    carry(&mut rig, object, item, properties);

    select(&mut rig, id);
    assert_eq!(rig.host.identified, [id]);
    assert!(rig.messages().is_empty());

    tick(&mut rig, 0.1);
    assert!(rig.messages().is_empty());

    rig.host.meta.objects[0].has_appraisal_data = true;
    tick(&mut rig, 0.1);
    assert_eq!(rig.messages().len(), 1);
    assert!(rig.messages()[0].contains("Silver Breastplate, "));

    tick(&mut rig, 0.1);
    assert_eq!(rig.messages().len(), 1, "said once");
}

#[test]
fn an_appraisal_that_never_comes_describes_what_is_known() {
    let mut rig = enabled();
    let (object, item, properties) = breastplate(false);
    let id = object.object_id;
    carry(&mut rig, object, item, properties);

    select(&mut rig, id);
    tick(&mut rig, APPRAISAL_TIMEOUT_SECONDS - 0.5);
    assert!(rig.messages().is_empty());
    tick(&mut rig, 1.0);

    assert_eq!(rig.messages().len(), 1);
}

#[test]
fn a_new_selection_replaces_the_one_waiting() {
    let mut rig = enabled();
    let (object, item, properties) = breastplate(false);
    let id = object.object_id;
    carry(&mut rig, object, item, properties);

    select(&mut rig, id);
    rig.with_call(|call, commands| {
        commands
            .tools_mut()
            .item_info
            .on_selection_changed(call, None, selection_source::KEYBOARD);
    });
    rig.host.meta.objects[0].has_appraisal_data = true;
    tick(&mut rig, 0.1);

    assert!(rig.messages().is_empty());
}

#[test]
fn switching_the_tool_or_its_trigger_off_drops_the_selection_waiting() {
    // UtilityBelt stops waiting when one of its switches changes.
    for setting in [
        item_info::ENABLED_SETTING,
        item_info::DESCRIBE_ON_SELECT_SETTING,
    ] {
        let mut rig = enabled();
        let (object, item, properties) = breastplate(false);
        let id = object.object_id;
        carry(&mut rig, object, item, properties);
        select(&mut rig, id);
        assert_eq!(rig.host.identified, [id]);

        set_switch(&mut rig, setting, false);
        tick(&mut rig, 0.1);
        assert_eq!(
            rig.with_call(|_, commands| commands.tools_mut().item_info.pending()),
            None,
            "{setting}"
        );
        rig.host.meta.objects[0].has_appraisal_data = true;
        set_switch(&mut rig, setting, true);
        tick(&mut rig, 0.1);
        assert!(rig.messages().is_empty(), "{setting}: {:?}", rig.messages());
    }
}

/// The tool on with only `trigger` to start it.
fn triggered_by(trigger: &str) -> Rig {
    let mut rig = Rig::new();
    switch_on(&mut rig, item_info::ENABLED_SETTING);
    switch_on(&mut rig, trigger);
    rig
}

#[test]
fn a_click_switch_describes_a_selection_the_mouse_made_and_no_other() {
    // The client says the mouse made a selection, not with which button:
    // either click switch describes what the mouse selects.
    for trigger in [
        item_info::DESCRIBE_ON_LEFT_CLICK_SETTING,
        item_info::DESCRIBE_ON_RIGHT_CLICK_SETTING,
    ] {
        let mut rig = triggered_by(trigger);
        let (object, item, properties) = breastplate(true);
        let id = object.object_id;
        carry(&mut rig, object, item, properties);

        for source in [
            selection_source::SYSTEM,
            selection_source::KEYBOARD,
            selection_source::PLUGIN,
        ] {
            select_by(&mut rig, id, source);
        }
        assert!(rig.messages().is_empty(), "{trigger}: {:?}", rig.messages());

        select_by(&mut rig, id, selection_source::MOUSE);
        assert_eq!(rig.messages().len(), 1, "{trigger}");
    }
}

#[test]
fn describe_on_select_describes_a_selection_whatever_made_it() {
    let mut rig = triggered_by(item_info::DESCRIBE_ON_SELECT_SETTING);
    let (object, item, properties) = breastplate(true);
    let id = object.object_id;
    carry(&mut rig, object, item, properties);

    for source in [
        selection_source::SYSTEM,
        selection_source::KEYBOARD,
        selection_source::MOUSE,
        selection_source::PLUGIN,
    ] {
        select_by(&mut rig, id, source);
    }

    assert_eq!(rig.messages().len(), 4);
}

#[test]
fn a_click_switch_changing_drops_the_selection_waiting() {
    let mut rig = triggered_by(item_info::DESCRIBE_ON_LEFT_CLICK_SETTING);
    let (object, item, properties) = breastplate(false);
    let id = object.object_id;
    carry(&mut rig, object, item, properties);
    select_by(&mut rig, id, selection_source::MOUSE);
    assert_eq!(rig.host.identified, [id]);

    // Switching the other click on changes the switches too.
    switch_on(&mut rig, item_info::DESCRIBE_ON_RIGHT_CLICK_SETTING);
    rig.host.meta.objects[0].has_appraisal_data = true;
    tick(&mut rig, 0.1);

    assert!(rig.messages().is_empty(), "{:?}", rig.messages());
}

#[test]
fn the_looter_keeps_the_appraisal_slot() {
    let mut rig = enabled();
    let (object, item, properties) = breastplate(false);
    let id = object.object_id;
    carry(&mut rig, object, item, properties);
    rig.services.corpse_identifier_busy = true;

    select(&mut rig, id);
    assert!(rig.host.identified.is_empty());

    rig.services.corpse_identifier_busy = false;
    tick(&mut rig, 0.1);
    assert_eq!(rig.host.identified, [id]);
}

#[test]
fn doors_and_npcs_are_described_only_when_asked_for() {
    let mut rig = enabled();
    let door = WorldObject {
        object_id: 50,
        name: "Door".to_owned(),
        object_class: ObjectClass::Door,
        has_appraisal_data: true,
        ..WorldObject::default()
    };
    rig.host.meta.objects.push(door);

    select(&mut rig, 50);
    assert!(rig.messages().is_empty());

    switch_on(&mut rig, item_info::SHOW_DOOR_INFO_SETTING);
    select(&mut rig, 50);
    assert_eq!(rig.single_message(), "[UB] ItemInfo: Door ");
}

#[test]
fn a_weapon_shows_its_damage_type_mastery_damage_and_bonuses() {
    let mut rig = enabled();
    let object = WorldObject {
        object_id: 60,
        name: "Long Sword".to_owned(),
        object_class: ObjectClass::MeleeWeapon,
        item_type: 1,
        is_owned: true,
        container_id: 1,
        has_appraisal_data: true,
        spell_ids: vec![2591, 2096, 1616],
        ..WorldObject::default()
    };
    let item = InventoryItem {
        object_id: 60,
        name: "Long Sword".to_owned(),
        item_type: 1,
        container_id: 1,
        damage: 50,
        damage_variance: 0.45,
        damage_type: 1,
        material_type: 64,
        value: 12_345,
        burden: 350,
        appraised_spell_ids: object.spell_ids.clone(),
        ..InventoryItem::default()
    };
    let properties = ItemProperties {
        // Slashing, sword mastery, elemental bonus.
        ints: vec![(45, 1), (353, 2), (204, 3), (131, 64)],
        floats: vec![(29, 1.15), (152, 1.0), (144, 0.0)],
        weapon: Some(WeaponProfile {
            damage: 50,
            damage_variance: 0.45,
            damage_mod: 1.0,
            weapon_offense: 1.08,
            ..WeaponProfile::default()
        }),
        ..ItemProperties::default()
    };
    carry(&mut rig, object, item, properties);
    rig.host.meta.spells.extend([
        spell(2591, "Major Heart Thirst", 999, 0),
        spell(2096, "Infected Caress", 154, 300),
        spell(1616, "Blood Drinker VI", 154, 250),
    ]);
    switch_on(&mut rig, item_info::SHOW_VALUE_AND_BURDEN_SETTING);

    select(&mut rig, 60);

    assert_eq!(
        rig.single_message(),
        "[UB] ItemInfo: Steel Long Sword (Slashing Sword), 27.50-50, 0.45v, +3, \
         8%a (13), 15%md, Major Heart Thirst, Infected Caress, Value 12,345, BU 350 "
    );
}

/// A summoning essence the character carries: its level in its name, its
/// ratings, and the summoning backdrop under its icon unless `plain`.
fn essence(rig: &mut Rig, object_id: u32, name: &str, ratings: Vec<(u32, i32)>, plain: bool) {
    let object = WorldObject {
        object_id,
        name: name.to_owned(),
        object_class: ObjectClass::Misc,
        is_owned: true,
        container_id: 1,
        has_appraisal_data: true,
        ..WorldObject::default()
    };
    let item = InventoryItem {
        object_id,
        name: name.to_owned(),
        container_id: 1,
        icon_underlay_id: if plain { 0 } else { 0x0600_7420 },
        ..InventoryItem::default()
    };
    let properties = ItemProperties {
        ints: ratings,
        ..ItemProperties::default()
    };
    carry(rig, object, item, properties);
}

#[test]
fn a_summoning_essence_shows_its_approximate_damage_per_second() {
    // UtilityBelt's summon estimate: the level in the name picks the base
    // (50 here), damage rating 10 adds 10%, crit 5 makes 15% of hits
    // critical, crit damage 20 makes a critical hit 2 x 1.2 times as hard:
    // 55 x 0.85 + 132 x 0.15 = 66.55.
    let mut rig = enabled();
    essence(
        &mut rig,
        70,
        "Mudstone Essence (50)",
        vec![(370, 10), (372, 5), (374, 20)],
        false,
    );

    select(&mut rig, 70);

    assert_eq!(
        rig.single_message(),
        "[UB] ItemInfo: Mudstone Essence (50), [D 10, C 5, CD 20], Approx DPS: 66.55 "
    );
}

#[test]
fn an_essence_without_a_level_in_its_name_is_reckoned_at_level_200() {
    // Level 200's base is 750: 750 x 0.9 + 1500 x 0.1 = 825.
    let mut rig = enabled();
    essence(&mut rig, 71, "Essence", Vec::new(), false);
    // Without the summoning backdrop there is no estimate.
    essence(&mut rig, 72, "Pebble (50)", Vec::new(), true);

    select(&mut rig, 71);
    select(&mut rig, 72);

    assert_eq!(
        rig.messages(),
        [
            "[UB] ItemInfo: Essence, Approx DPS: 825 ",
            "[UB] ItemInfo: Pebble (50) ",
        ]
    );
}

#[test]
fn a_players_pk_ratings_alone_show_no_ratings() {
    // UtilityBelt adds up a player's ratings without the PK ones before it
    // shows any; once one counts, the PK ones are listed too.
    let mut rig = enabled();
    let player = |object_id, name: &str| WorldObject {
        object_id,
        name: name.to_owned(),
        object_class: ObjectClass::Player,
        has_appraisal_data: true,
        ..WorldObject::default()
    };
    rig.host.meta.objects.push(player(80, "Pker"));
    rig.host.meta.properties.insert(
        80,
        ItemProperties {
            ints: vec![(381, 5), (382, 3)],
            ..ItemProperties::default()
        },
    );
    rig.host.meta.objects.push(player(81, "Hitter"));
    rig.host.meta.properties.insert(
        81,
        ItemProperties {
            ints: vec![(307, 2), (381, 5)],
            ..ItemProperties::default()
        },
    );

    select(&mut rig, 80);
    select(&mut rig, 81);

    assert_eq!(
        rig.messages(),
        [
            "[UB] ItemInfo: Pker ",
            "[UB] ItemInfo: Hitter, [D 2, PKD 5] "
        ]
    );
}

#[test]
fn an_appraisal_the_client_gives_up_on_describes_what_is_known_at_once() {
    let mut rig = enabled();
    let (object, item, properties) = breastplate(false);
    let id = object.object_id;
    carry(&mut rig, object, item, properties);
    select(&mut rig, id);
    tick(&mut rig, 0.1);
    assert!(rig.messages().is_empty());

    // Long before the five seconds are up.
    rig.host.appraisal.last_abandoned_id = id;
    tick(&mut rig, 0.1);

    assert_eq!(rig.messages().len(), 1);
    assert!(rig.messages()[0].contains("Silver Breastplate"));
    tick(&mut rig, APPRAISAL_TIMEOUT_SECONDS);
    assert_eq!(rig.messages().len(), 1, "said once");
}

#[test]
fn a_busy_appraisal_slot_is_asked_again_the_next_frame() {
    let mut rig = enabled();
    let (object, item, properties) = breastplate(false);
    let id = object.object_id;
    carry(&mut rig, object, item, properties);
    rig.host.identify_status = ItemStatus::Busy;

    select(&mut rig, id);
    tick(&mut rig, 0.1);
    assert_eq!(rig.host.identified, [id, id], "asked again while busy");

    rig.host.identify_status = ItemStatus::Started;
    tick(&mut rig, 0.1);
    tick(&mut rig, 0.1);
    assert_eq!(rig.host.identified, [id, id, id], "asked until it was sent");

    rig.host.meta.objects[0].has_appraisal_data = true;
    tick(&mut rig, 0.1);
    assert_eq!(rig.messages().len(), 1);
}

#[test]
fn an_appraisal_someone_else_asked_for_describes_the_object_waiting() {
    // The slot already waits on the object (the player or another tool
    // asked): nothing more is asked, and its answer describes it.
    let mut rig = enabled();
    let (object, item, properties) = breastplate(false);
    let id = object.object_id;
    carry(&mut rig, object, item, properties);
    rig.host.appraisal.awaiting_id = id;

    select(&mut rig, id);
    tick(&mut rig, 0.1);
    assert!(rig.host.identified.is_empty(), "{:?}", rig.host.identified);
    assert!(rig.messages().is_empty());

    rig.host.appraisal.awaiting_id = 0;
    rig.host.meta.objects[0].has_appraisal_data = true;
    tick(&mut rig, 0.1);

    assert_eq!(rig.messages().len(), 1);
    assert!(rig.host.identified.is_empty(), "{:?}", rig.host.identified);
}

#[test]
fn an_appraisal_someone_else_asked_for_that_the_client_gives_up_on_ends_the_wait() {
    // That request counts as the one asked for: when the client gives up
    // on it, what is known is described and nothing more is asked.
    let mut rig = enabled();
    let (object, item, properties) = breastplate(false);
    let id = object.object_id;
    carry(&mut rig, object, item, properties);
    rig.host.appraisal.awaiting_id = id;
    select(&mut rig, id);
    tick(&mut rig, 0.1);

    rig.host.appraisal.awaiting_id = 0;
    rig.host.appraisal.last_abandoned_id = id;
    tick(&mut rig, 0.1);

    assert_eq!(rig.messages().len(), 1);
    assert!(rig.host.identified.is_empty(), "{:?}", rig.host.identified);
}

#[test]
fn a_slot_waiting_on_another_object_is_left_to_it() {
    let mut rig = enabled();
    let (object, item, properties) = breastplate(false);
    let id = object.object_id;
    carry(&mut rig, object, item, properties);
    rig.host.appraisal.awaiting_id = 0x8000_0099;

    select(&mut rig, id);
    tick(&mut rig, 0.1);
    assert!(rig.host.identified.is_empty());

    rig.host.appraisal.awaiting_id = 0;
    tick(&mut rig, 0.1);
    assert_eq!(rig.host.identified, [id]);
}

#[test]
fn an_npc_is_described_with_show_npc_info_on() {
    let mut rig = enabled();
    rig.host.meta.objects.push(WorldObject {
        object_id: 90,
        name: "Town Crier".to_owned(),
        object_class: ObjectClass::Npc,
        has_appraisal_data: true,
        ..WorldObject::default()
    });

    select(&mut rig, 90);
    assert!(rig.messages().is_empty());

    switch_on(&mut rig, item_info::SHOW_NPC_INFO_SETTING);
    select(&mut rig, 90);
    assert_eq!(rig.single_message(), "[UB] ItemInfo: Town Crier ");
}

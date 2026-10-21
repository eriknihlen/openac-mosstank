//! The Items and Consumables pages as VTank shows them: one Items row per
//! profiled item, named from the world or `<INVALID id>`, " (NO BUFFS)" in
//! red for an item kept without buffs, a name click that takes the whole
//! item out, a Hands column read from and written to ItemUseSpecifiers, the
//! names refreshed when a listed weapon arrives, and the gems and food
//! listed ahead of the other consumables.

mod common;

use ac_plugin_abi::Event;
use ac_plugin_abi::world::{ObjectChange, ObjectChangeKind, ObjectClass, WorldObject};
use common::{World, item};
use mosstank::MossTank;
use mosstank_formats::usd::{VtankCell, VtankDatabase, VtankRow};
use mosstank_settings::items::item_use_specifiers;

const NAME: &str = "Tabs";
const KEY: &str = "mosstank/profiles/Tabs.usd";
/// The character's own id (`common::character`).
const CHARACTER: i32 = 0x5000_0001;
const NAME_COLOR: u32 = 0x00E8_DEC3;
const RED: u32 = 0x00FF_0000;

/// A profile with these BuffedItems and ItemUseSpecifiers rows, its gems
/// and food and its assist items.
#[derive(Default)]
struct Profile<'a> {
    buffed: &'a [(i32, i32)],
    uses: &'a [(i32, i32)],
    gem_food: &'a [(&'a str, i32)],
    assist: &'a [(&'a str, i32)],
}

impl Profile<'_> {
    fn database(&self) -> VtankDatabase {
        let mut database = mosstank_settings::defaults::create();
        let int_rows = |rows: &[(i32, i32)]| -> Vec<VtankRow> {
            rows.iter()
                .map(|&(a, b)| VtankRow {
                    cells: vec![VtankCell::int(a), VtankCell::int(b)],
                })
                .collect()
        };
        let named_rows = |rows: &[(&str, i32)]| -> Vec<VtankRow> {
            rows.iter()
                .map(|&(name, value)| VtankRow {
                    cells: vec![VtankCell::string(name), VtankCell::int(value)],
                })
                .collect()
        };
        database.find_mut("BuffedItems").unwrap().rows = int_rows(self.buffed);
        database.find_mut("ItemUseSpecifiers").unwrap().rows = int_rows(self.uses);
        database.find_mut("GemFoodItems").unwrap().rows = named_rows(self.gem_food);
        database.find_mut("AssistItems").unwrap().rows = named_rows(self.assist);
        database
    }
}

/// A character owning `owned`, with `profile` loaded.
fn load(
    profile: &Profile<'_>,
    owned: Vec<ac_plugin_abi::items::InventoryItem>,
) -> (World, MossTank) {
    let mut world = World::panel_character("Item Tester");
    world.own_items(owned);
    world.profiles.insert(KEY, &profile.database().render());
    let mut plugin = world.enable();
    world.vt(&mut plugin, &format!("settings load {NAME}"));
    (world, plugin)
}

fn names(world: &mut World, plugin: &mut MossTank) -> Vec<String> {
    world.ui_list(plugin, "ItemNameColumn")
}

fn hands(world: &mut World, plugin: &mut MossTank) -> Vec<String> {
    world.ui_list(plugin, "ItemHandsColumn")
}

/// The saved profile's rows of an int table.
fn saved_ints(world: &World, table: &str) -> Vec<(i32, i32)> {
    let database = VtankDatabase::parse(world.profiles.get(KEY).unwrap()).unwrap();
    database
        .find(table)
        .unwrap()
        .rows
        .iter()
        .map(|row| {
            (
                row.cells[0].as_int().unwrap(),
                row.cells[1].as_int().unwrap(),
            )
        })
        .collect()
}

/// The saved profile's names in a name table.
fn saved_names(world: &World, table: &str) -> Vec<String> {
    let database = VtankDatabase::parse(world.profiles.get(KEY).unwrap()).unwrap();
    database
        .find(table)
        .unwrap()
        .rows
        .iter()
        .map(|row| row.cells[0].as_string().to_owned())
        .collect()
}

/// A wand as the panel tests make one, in the wand slot.
fn wand(object_id: u32, name: &str) -> ac_plugin_abi::items::InventoryItem {
    item(object_id, name, 0x8000)
}

/// A sword as the panel tests make one, in the melee slot.
fn sword(object_id: u32, name: &str) -> ac_plugin_abi::items::InventoryItem {
    item(object_id, name, 1)
}

#[test]
fn the_items_page_is_one_row_per_item_without_the_any_item_entry_or_the_character() {
    let (mut world, mut plugin) = load(
        &Profile {
            buffed: &[
                (10, 101),
                (10, 102),
                (-1, 103),
                (CHARACTER, 104),
                (11, 105),
                (10, 106),
                (11, 107),
            ],
            ..Profile::default()
        },
        vec![sword(10, "Fire Sword"), wand(11, "Ice Wand")],
    );
    let p = &mut plugin;
    assert_eq!(names(&mut world, p), ["Fire Sword", "Ice Wand"]);
    assert_eq!(world.ui_list(p, "ItemRows"), ["Fire Sword", "Ice Wand"]);
}

#[test]
fn an_item_whose_first_spell_is_none_reads_no_buffs_in_red() {
    let (mut world, mut plugin) = load(
        &Profile {
            buffed: &[(11, -1), (11, 105), (12, 106), (12, -1)],
            ..Profile::default()
        },
        vec![wand(11, "Ice Wand"), sword(12, "Fire Sword")],
    );
    let p = &mut plugin;
    assert_eq!(names(&mut world, p), ["Ice Wand (NO BUFFS)", "Fire Sword"]);
    assert_eq!(world.ui_uints(p, "ItemNameColors"), [RED, NAME_COLOR]);
}

#[test]
fn an_item_the_client_does_not_know_reads_invalid_with_its_signed_id() {
    let unknown = 0x8000_1234_u32.cast_signed();
    let (mut world, mut plugin) = load(
        &Profile {
            buffed: &[(unknown, 101), (12, -1)],
            ..Profile::default()
        },
        Vec::new(),
    );
    let p = &mut plugin;
    assert_eq!(
        names(&mut world, p),
        ["<INVALID -2147478988>", "<INVALID 12> (NO BUFFS)"]
    );
    // An object the client does not know always has a Hands cell.
    assert_eq!(hands(&mut world, p), ["BothHands", "BothHands"]);
}

#[test]
fn clicking_a_name_removes_the_whole_item_and_its_hands_row_and_saves() {
    let (mut world, mut plugin) = load(
        &Profile {
            buffed: &[(10, 101), (11, 103), (10, 102)],
            uses: &[(10, 1), (11, 2)],
            ..Profile::default()
        },
        vec![sword(10, "Fire Sword"), sword(11, "Ice Sword")],
    );
    let p = &mut plugin;
    assert_eq!(names(&mut world, p), ["Fire Sword", "Ice Sword"]);
    world.act_int(p, "DeleteItemRowAt", 0);
    assert_eq!(names(&mut world, p), ["Ice Sword"]);
    assert_eq!(saved_ints(&world, "BuffedItems"), [(11, 103)]);
    assert_eq!(saved_ints(&world, "ItemUseSpecifiers"), [(11, 2)]);
}

#[test]
fn the_hands_column_reads_and_cycles_item_use_specifiers_and_survives_a_reload() {
    let (mut world, mut plugin) = load(
        &Profile {
            buffed: &[(10, -1), (11, -1), (12, 101)],
            uses: &[(12, 7)],
            ..Profile::default()
        },
        vec![
            sword(10, "Fire Sword"),
            wand(11, "Ice Wand"),
            sword(12, "Odd Sword"),
        ],
    );
    let p = &mut plugin;
    // A wand has no Hands cell; a value that names no use shows none.
    assert_eq!(hands(&mut world, p), ["BothHands", "", ""]);
    for expected in ["DONOTUSE", "RightHand", "LeftHand", "BothHands", "DONOTUSE"] {
        world.act_int(p, "CycleItemHandsAt", 0);
        assert_eq!(hands(&mut world, p)[0], expected);
    }
    world.act_int(p, "CycleItemHandsAt", 1);
    world.act_int(p, "CycleItemHandsAt", 2);
    assert_eq!(hands(&mut world, p), ["DONOTUSE", "", ""]);
    // Combat reads the same value.
    let combat = &p.tank().unwrap().settings().combat;
    assert_eq!(item_use_specifiers::uses_for(combat, 10), 0);
    assert_eq!(item_use_specifiers::uses_for(combat, 12), 7);
    world.tick(p);
    assert_eq!(saved_ints(&world, "ItemUseSpecifiers"), [(12, 7), (10, 0)]);

    world.disable(p);
    let mut reloaded = world.enable();
    world.vt(&mut reloaded, &format!("settings load {NAME}"));
    assert_eq!(hands(&mut world, &mut reloaded), ["DONOTUSE", "", ""]);
}

#[test]
fn a_listed_item_arriving_names_its_row_on_the_next_tick_and_a_move_does_not() {
    let (mut world, mut plugin) = load(
        &Profile {
            buffed: &[(10, 101)],
            ..Profile::default()
        },
        Vec::new(),
    );
    let p = &mut plugin;
    assert_eq!(names(&mut world, p), ["<INVALID 10>"]);
    world.own_items(vec![sword(10, "Fire Sword")]);
    let change = |kind| {
        Event::ObjectChanged(ObjectChange {
            object_id: 10,
            kind,
            revision: 1,
            changed_fields: 0,
            current: Some(WorldObject {
                object_id: 10,
                name: "Fire Sword".to_owned(),
                object_class: ObjectClass::MeleeWeapon,
                ..WorldObject::default()
            }),
        })
    };
    world.send(p, change(ObjectChangeKind::Moved));
    world.tick(p);
    assert_eq!(names(&mut world, p), ["<INVALID 10>"]);
    world.send(p, change(ObjectChangeKind::Created));
    // A burst of changes costs one rebuild, on the next tick.
    assert_eq!(names(&mut world, p), ["<INVALID 10>"]);
    world.tick(p);
    assert_eq!(names(&mut world, p), ["Fire Sword"]);
}

#[test]
fn entering_the_world_names_the_items_owned_since_the_profile_loaded() {
    let (mut world, mut plugin) = load(
        &Profile {
            buffed: &[(10, 101)],
            ..Profile::default()
        },
        Vec::new(),
    );
    let p = &mut plugin;
    assert_eq!(names(&mut world, p), ["<INVALID 10>"]);
    world.own_items(vec![sword(10, "Fire Sword")]);
    world.send(p, Event::LoginComplete);
    world.tick(p);
    assert_eq!(names(&mut world, p), ["Fire Sword"]);
}

/// A self aura the character knows, by name.
fn aura(spell_id: u32, name: &str) -> ac_plugin_abi::magic::SpellInfo {
    ac_plugin_abi::magic::SpellInfo {
        name: name.to_owned(),
        ..common::spell(spell_id, spell_id, "")
    }
}

#[test]
fn an_item_added_with_buffs_is_saved_with_its_spells_and_reads_back_with_them() {
    let mut world = World::panel_character("Item Tester");
    world.own_items(vec![sword(10, "Fire Sword")]);
    // Two of the sword's four default auras are known by name; the others
    // stay kept by name beside the profile.
    world.know_spells(vec![
        aura(501, "Aura of Defender Self I"),
        aura(502, "Aura of Blood Drinker Self I"),
    ]);
    world
        .profiles
        .insert(KEY, &Profile::default().database().render());
    let mut plugin = world.enable();
    world.vt(&mut plugin, &format!("settings load {NAME}"));
    world.select_object(10);
    world.act(&mut plugin, "AddSelectedItem");
    assert_eq!(names(&mut world, &mut plugin), ["Fire Sword"]);
    assert_eq!(saved_ints(&world, "BuffedItems"), [(10, 501), (10, 502)]);

    world.disable(&mut plugin);
    let mut reloaded = world.enable();
    world.vt(&mut reloaded, &format!("settings load {NAME}"));
    assert_eq!(names(&mut world, &mut reloaded), ["Fire Sword"]);
}

#[test]
fn an_item_added_without_buffs_is_saved_as_the_item_with_no_spell() {
    let mut world = World::panel_character("Item Tester");
    world.own_items(vec![sword(10, "Fire Sword")]);
    world.know_spells(vec![aura(501, "Aura of Defender Self I")]);
    world
        .profiles
        .insert(KEY, &Profile::default().database().render());
    let mut plugin = world.enable();
    world.vt(&mut plugin, &format!("settings load {NAME}"));
    world.select_object(10);
    world.act(&mut plugin, "AddSelectedItemNoBuffs");
    assert_eq!(names(&mut world, &mut plugin), ["Fire Sword (NO BUFFS)"]);
    assert_eq!(saved_ints(&world, "BuffedItems"), [(10, -1)]);
}

#[test]
fn the_consumables_page_lists_gems_and_food_first_and_a_click_removes_from_the_right_table() {
    let (mut world, mut plugin) = load(
        &Profile {
            gem_food: &[("Gem B", 2001), ("Dual", 2003), ("Food A", 2002)],
            assist: &[("Kit Z", 0), ("Dual", 7), ("Phial", 7)],
            ..Profile::default()
        },
        Vec::new(),
    );
    let p = &mut plugin;
    assert_eq!(
        world.ui_list(p, "ConsumableRows"),
        ["Gem B", "Dual", "Food A", "Kit Z", "Dual", "Phial"]
    );
    // The second "Dual" is the assist item's, but the name is a gem's
    // first, so it leaves the gems and food.
    world.act_int(p, "SelectConsumableRow", 4);
    assert_eq!(
        world.ui_list(p, "ConsumableRows"),
        ["Gem B", "Food A", "Kit Z", "Dual", "Phial"]
    );
    world.act_int(p, "SelectConsumableRow", 2);
    assert_eq!(
        world.ui_list(p, "ConsumableRows"),
        ["Gem B", "Food A", "Dual", "Phial"]
    );
    world.tick(p);
    assert_eq!(saved_names(&world, "GemFoodItems"), ["Gem B", "Food A"]);
    assert_eq!(saved_names(&world, "AssistItems"), ["Dual", "Phial"]);
}

#[test]
fn an_item_whose_buffs_the_client_cannot_name_reads_no_buffs_at_once_and_after_a_restart() {
    let mut world = World::panel_character("Item Tester");
    world.own_items(vec![sword(10, "Fire Sword")]);
    world
        .profiles
        .insert(KEY, &Profile::default().database().render());
    let mut plugin = world.enable();
    world.vt(&mut plugin, &format!("settings load {NAME}"));
    world.select_object(10);
    world.act(&mut plugin, "AddSelectedItem");
    // No default buff resolves to a spell id, so the item is kept, and
    // saved, as the item with no spell.
    assert_eq!(names(&mut world, &mut plugin), ["Fire Sword (NO BUFFS)"]);
    assert_eq!(world.ui_uints(&mut plugin, "ItemNameColors"), [RED]);
    assert_eq!(saved_ints(&world, "BuffedItems"), [(10, -1)]);

    world.disable(&mut plugin);
    let mut reloaded = world.enable();
    world.vt(&mut reloaded, &format!("settings load {NAME}"));
    assert_eq!(names(&mut world, &mut reloaded), ["Fire Sword (NO BUFFS)"]);
}

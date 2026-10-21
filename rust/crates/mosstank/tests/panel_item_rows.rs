//! The Items page's rows as object identities: what an add writes, what a
//! removal takes away, and how a row reads when its object is not to hand
//! (ported from the C#'s `MossTankPanelTests`).

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::items::{InventoryItem, ItemResult};
use ac_plugin_abi::world::WorldObject;
use common::{World, item};
use mosstank::MossTank;
use mosstank_formats::usd::{VtankCell, VtankDatabase, VtankRow};

fn add(world: &mut World, plugin: &mut MossTank, object_id: u32) {
    world.select_object(object_id);
    world.act(plugin, "AddSelectedItem");
}

fn rows(world: &mut World, plugin: &mut MossTank) -> Vec<String> {
    world.ui_list(plugin, "ItemRows")
}

fn usd_key(world: &World) -> String {
    let keys: Vec<String> = world
        .profiles
        .keys()
        .filter(|key| key.ends_with(".usd"))
        .map(str::to_owned)
        .collect();
    assert_eq!(keys.len(), 1, "{keys:?}");
    keys[0].clone()
}

fn buffed_items(world: &World) -> Vec<VtankRow> {
    let document = VtankDatabase::parse(world.profiles.get(&usd_key(world)).unwrap()).unwrap();
    document.find("BuffedItems").unwrap().rows.clone()
}

fn object_column(world: &World) -> usize {
    let document = VtankDatabase::parse(world.profiles.get(&usd_key(world)).unwrap()).unwrap();
    document
        .find("BuffedItems")
        .unwrap()
        .column_index("Object")
        .unwrap()
}

fn wielded(mut item: InventoryItem) -> InventoryItem {
    item.equipped_location = item.valid_locations;
    item.wielder_id = 1;
    item
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnItemsAddThatIsRefusedSaysWhy
/// The C# fake has no profile folder, so the add is saved only in part; the
/// Rust world takes the folder away the same way.
#[test]
fn an_items_add_that_is_refused_says_why_in_chat() {
    let mut world = World::panel_character("Refused");
    world.profiles.available = false;
    world.own_items(vec![item(10, "War Wand", 0x8000)]);
    let mut plugin = world.enable();
    let p = &mut plugin;
    world.clear_chat();
    world.act(p, "AddSelectedItem");
    assert_eq!(
        world.ui_text(p, "ProfileNotice"),
        "Select an owned inventory item first."
    );
    world.act(p, "RemoveSelectedItem");
    assert_eq!(
        world.ui_text(p, "ProfileNotice"),
        "The Items profile is empty."
    );
    add(&mut world, p, 10);
    assert!(
        world
            .ui_list(p, "ItemNameColumn")
            .iter()
            .any(|n| n == "War Wand (NO BUFFS)")
    );
    world.act(p, "AddSelectedItem");
    assert!(
        world
            .ui_text(p, "ProfileNotice")
            .contains("already in this list")
    );
    let chat = world.chat();
    assert_eq!(chat.len(), 4, "{chat:?}");
    assert!(chat[0].contains("Select an owned inventory item first."));
    assert!(chat[1].contains("The Items profile is empty."));
    assert!(
        chat[2].contains("Added War Wand, but the changes could not be fully saved:"),
        "{}",
        chat[2]
    );
    assert!(chat[3].contains("already in this list"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AddingSelectedItemPersistsItsObjectIdentityInUsd
#[test]
fn adding_the_selected_item_writes_its_object_identity() {
    let mut world = World::panel_character("Identity");
    world.own_items(vec![item(10, "War Wand", 0x8000)]);
    let mut plugin = world.enable();
    add(&mut world, &mut plugin, 10);
    let rows = buffed_items(&world);
    assert_eq!(rows.len(), 1);
    let document = VtankDatabase::parse(world.profiles.get(&usd_key(&world)).unwrap()).unwrap();
    let table = document.find("BuffedItems").unwrap();
    assert_eq!(
        rows[0].cells[table.column_index("Object").unwrap()]
            .as_int()
            .unwrap(),
        10
    );
    assert_eq!(
        rows[0].cells[table.column_index("Spell").unwrap()]
            .as_int()
            .unwrap(),
        -1
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: RemovingSelectedOwnedItemRemovesItsUsdIdentity
#[test]
fn removing_the_selected_item_takes_its_identity_out() {
    let mut world = World::panel_character("Identity");
    world.own_items(vec![item(10, "War Wand", 0x8000)]);
    let mut plugin = world.enable();
    add(&mut world, &mut plugin, 10);
    world.act(&mut plugin, "RemoveSelectedItem");
    assert!(buffed_items(&world).is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AListedItemIsOneRowUnderItsNameWhileThereIsNoInventory
// An item the client does not know now reads `<INVALID id>`, and an item
// the profile lists with no spell reads as kept without buffs.
#[test]
fn a_listed_item_the_client_does_not_know_reads_invalid_by_its_id() {
    let mut world = World::panel_character("Lister");
    world.own_items(vec![
        item(10, "Decapitator's Blade", 1),
        item(11, "Wings of Rakhil", 1),
    ]);
    let mut first = world.enable();
    add(&mut world, &mut first, 10);
    add(&mut world, &mut first, 11);
    world.disable(&mut first);

    world.own_items(Vec::new());
    let mut logged_out = world.enable();
    assert_eq!(
        rows(&mut world, &mut logged_out),
        ["<INVALID 10> (NO BUFFS)", "<INVALID 11> (NO BUFFS)"]
    );
    world.disable(&mut logged_out);

    world.own_items(vec![item(11, "Wings of Rakhil", 1)]);
    let mut without_the_blade = world.enable();
    assert_eq!(
        rows(&mut world, &mut without_the_blade),
        ["<INVALID 10> (NO BUFFS)", "Wings of Rakhil (NO BUFFS)"]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ItemRowsDeleteTheSelectedIdentityEvenWhenMissingOrNamesMatch
#[test]
fn item_rows_delete_their_own_identity_even_when_missing_or_named_alike() {
    let mut world = World::panel_character("Twins");
    world.own_items(vec![item(10, "Twin Sword", 1), item(11, "Twin Sword", 1)]);
    let mut first = world.enable();
    add(&mut world, &mut first, 10);
    add(&mut world, &mut first, 11);
    world.disable(&mut first);

    let key = usd_key(&world);
    let mut document = VtankDatabase::parse(world.profiles.get(&key).unwrap()).unwrap();
    let table = document.find_mut("BuffedItems").unwrap();
    table.rows.push(VtankRow {
        cells: vec![VtankCell::int(10), VtankCell::int(17)],
    });
    table.rows.push(VtankRow {
        cells: vec![VtankCell::string("not-an-object-id"), VtankCell::int(99)],
    });
    let text = document.render();
    world.profiles.insert(&key, &text);

    world.own_items(vec![item(11, "Twin Sword", 1)]);
    let mut second = world.enable();
    let p = &mut second;
    assert_eq!(
        rows(&mut world, p),
        ["<INVALID 10> (NO BUFFS)", "Twin Sword (NO BUFFS)"]
    );
    world.act_int(p, "DeleteItemRowAt", 0);
    assert_eq!(rows(&mut world, p), ["Twin Sword (NO BUFFS)"]);
    let column = object_column(&world);
    let saved = buffed_items(&world);
    // Every row of the removed item went, its spell row too.
    assert!(
        !saved
            .iter()
            .any(|row| row.cells[column].as_int().ok() == Some(10))
    );
    assert!(
        saved
            .iter()
            .any(|row| row.cells[column].as_int().ok() == Some(11))
    );
    assert!(
        saved
            .iter()
            .any(|row| row.cells[column].as_string() == "not-an-object-id")
    );
    world.act_int(p, "DeleteItemRowAt", 0);
    assert!(rows(&mut world, p).is_empty());
    let saved = buffed_items(&world);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].cells[column].as_string(), "not-an-object-id");
}

fn unassessed(world: &mut World) {
    world.set_reply::<calls::WorldObject>(&Some(WorldObject {
        is_owned: true,
        last_id_time: 0,
        ..WorldObject::default()
    }));
    world.set_reply::<calls::WorldIdentify>(&ItemResult::of(
        ac_plugin_abi::items::ItemStatus::Started,
    ));
}

fn assessed(world: &mut World) {
    world.set_reply::<calls::WorldObject>(&Some(WorldObject {
        is_owned: true,
        has_appraisal_data: true,
        last_id_time: 1,
        ..WorldObject::default()
    }));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AWieldedWeaponAddedToTheItemsListSurvivesARestart
#[test]
fn a_wielded_weapon_added_to_the_list_survives_a_restart() {
    let mut world = World::panel_character("Prover");
    let blade = wielded(item(10, "Decapitator's Blade", 1));
    world.own_items(vec![blade.clone()]);
    unassessed(&mut world);
    let mut first = world.enable();
    let p = &mut first;
    add(&mut world, p, 10);
    // The client names none of its buffs here, so the item is kept as
    // carrying no spell, the same before and after a restart.
    // The client answers the appraisal the add asked for.
    assessed(&mut world);
    for _ in 0..20 {
        if rows(&mut world, p)
            .iter()
            .any(|r| r == "Decapitator's Blade (NO BUFFS)")
        {
            break;
        }
        world.with_tank(p, |tank, client| tank.on_tick(client, 0.3));
    }
    assert!(
        rows(&mut world, p)
            .iter()
            .any(|r| r == "Decapitator's Blade (NO BUFFS)")
    );
    world.disable(p);
    let mut second = world.enable();
    assert!(
        rows(&mut world, &mut second)
            .iter()
            .any(|r| r == "Decapitator's Blade (NO BUFFS)")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: RemovingARowForAMissingObjectKeepsTheRestOfTheList
#[test]
fn removing_a_row_for_a_missing_object_keeps_the_rest() {
    let mut world = World::panel_character("Prover");
    let blade = wielded(item(10, "Decapitator's Blade", 1));
    let wand = item(11, "War Wand", 0x8000);
    world.own_items(vec![blade.clone(), wand]);
    let mut plugin = world.enable();
    let p = &mut plugin;
    add(&mut world, p, 10);
    add(&mut world, p, 11);
    let listed = rows(&mut world, p);
    assert!(listed.iter().any(|r| r == "Decapitator's Blade (NO BUFFS)"));
    assert!(listed.iter().any(|r| r == "War Wand (NO BUFFS)"));

    world.own_items(vec![blade.clone()]);
    world.act_int(p, "CycleItemHandsAt", 0);
    world.act_int(p, "CycleItemHandsAt", 0);
    let listed = rows(&mut world, p);
    let invalid = listed
        .iter()
        .position(|r| r.starts_with("<INVALID"))
        .expect("an unresolved row");
    world.act_int(p, "DeleteItemRowAt", i32::try_from(invalid).unwrap());
    let listed = rows(&mut world, p);
    assert!(listed.iter().any(|r| r == "Decapitator's Blade (NO BUFFS)"));
    assert!(!listed.iter().any(|r| r.starts_with("<INVALID")));
    world.disable(p);
    let mut reloaded = world.enable();
    assert!(
        rows(&mut world, &mut reloaded)
            .iter()
            .any(|r| r == "Decapitator's Blade (NO BUFFS)")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AnAddThatReachedNoProfileIsReportedAsUnsavedNotAsAdded
#[test]
fn an_add_that_reached_no_profile_is_reported_as_unsaved() {
    let mut world = World::panel_character("");
    let blade = wielded(item(10, "Decapitator's Blade", 1));
    world.own_items(vec![blade]);
    let mut plugin = world.enable();
    let p = &mut plugin;
    add(&mut world, p, 10);
    assert!(
        world
            .ui_text(p, "ProfileNotice")
            .contains("nothing was saved")
    );
    let written = |world: &World| {
        world
            .profiles
            .keys()
            .chain(world.plugin_data.keys())
            .any(|key| {
                world
                    .profiles
                    .get(key)
                    .or_else(|| world.plugin_data.get(key))
                    .is_some_and(|text| text.contains("Decapitator"))
            })
    };
    assert!(!written(&world));

    world.fake.character.as_mut().unwrap().name = "Prover".to_owned();
    world.with_tank(p, |tank, client| tank.on_tick(client, 0.3));
    add(&mut world, p, 10);
    assert_eq!(
        world.ui_text(p, "ProfileNotice"),
        "Added Decapitator's Blade."
    );
    assert!(
        rows(&mut world, p)
            .iter()
            .any(|r| r == "Decapitator's Blade (NO BUFFS)")
    );
    assert!(written(&world));
}

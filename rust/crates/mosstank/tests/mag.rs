//! The Mag tab's view of the client ([`MagHost`]) over the SDK host: each
//! call the one SDK call it stands for.

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::character::ActiveEnchantment;
use ac_plugin_abi::world::ItemProperties;
use common::{World, item};
use mosstank::automation::HostAutomation;
use mosstank_mag::host::MagHost;

fn mag<R>(world: &mut World, f: impl FnOnce(&mut dyn MagHost) -> R) -> R {
    world.with_host(|host| f(&mut HostAutomation::new(host)))
}

#[test]
fn the_owned_inventory_is_complete_when_the_client_says_so() {
    let mut world = World::with_character("Mag");
    world.set_reply::<calls::ItemsInventoryComplete>(&true);
    assert!(mag(&mut world, |host| host.owned_inventory_complete()));
    world.set_reply::<calls::ItemsInventoryComplete>(&false);
    assert!(!mag(&mut world, |host| host.owned_inventory_complete()));
}

#[test]
fn an_owned_items_properties_are_the_clients() {
    let mut world = World::with_character("Mag");
    let mut properties = ItemProperties::default();
    properties.ints.push((131, 7));
    world.set_reply::<calls::ItemsProperties>(&Some(properties.clone()));
    assert_eq!(
        mag(&mut world, |host| host.item_properties(0x5000_0001)),
        Some(properties)
    );
    assert_eq!(
        world.requests::<calls::ItemsProperties>(),
        vec![0x5000_0001]
    );
}

#[test]
fn the_open_containers_contents_are_the_clients() {
    let mut world = World::with_character("Mag");
    let ring = item(0x5000_0002, "Ring", 0x8);
    world.set_reply::<calls::LootContents>(&vec![ring.clone()]);
    assert_eq!(mag(&mut world, |host| host.loot_contents()), vec![ring]);
}

#[test]
fn every_enchantment_is_for_now_the_ones_in_effect() {
    // Task 6.0: plugin API 1.10's every-enchantment call replaces this.
    let mut world = World::with_character("Mag");
    let buff = ActiveEnchantment {
        spell_id: 2,
        family: 5,
        tier: 7,
        seconds_remaining: 3600.0,
    };
    world.set_reply::<calls::CharacterEnchantments>(&vec![buff.clone()]);
    assert_eq!(mag(&mut world, |host| host.all_enchantments()), vec![buff]);
    assert_eq!(
        world.requests::<calls::CharacterEnchantments>(),
        vec![false]
    );
}

#[test]
fn the_clock_is_the_sessions_monotonic_clock() {
    let mut world = World::with_character("Mag");
    world.fake.clock.monotonic_ms = 12_345;
    assert!((mag(&mut world, |host| host.now_ms()) - 12_345.0).abs() < f64::EPSILON);
}

#[test]
fn the_identify_queue_is_forgotten_when_the_session_ends() {
    let mut world = World::with_character("Mag");
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    world.with_tank(&mut plugin, |tank, _| tank.mag().identify.push(7));
    world.tick(&mut plugin);
    assert!(world.with_tank(&mut plugin, |tank, _| tank.mag().identify.contains(7)));
    world.fake.character = None;
    world.tick(&mut plugin);
    assert!(world.with_tank(&mut plugin, |tank, _| tank.mag().identify.is_empty()));
}

/// A loot profile `Mag` with one rule keeping everything, loaded.
fn keep_everything(world: &mut World, plugin: &mut mosstank::MossTank) {
    use mosstank_formats::utl::{self, LootAction, LootRule};
    let mut rule = LootRule::named("Rings", "*");
    rule.action = LootAction::Keep;
    world
        .profiles
        .insert("mosstank/loot/Mag.utl", &utl::serialize_rules(&[rule]));
    world.vt(plugin, "loot load Mag");
    world.ticks(plugin, 5);
    assert_eq!(plugin.tank().unwrap().live_loot_rules().len(), 1);
}

/// An appraisal of an owned object arrived.
fn ident_received(object_id: u32, name: &str) -> ac_plugin_abi::Event {
    use ac_plugin_abi::world::{ObjectChange, ObjectChangeKind, WorldObject};
    ac_plugin_abi::Event::ObjectChanged(ObjectChange {
        object_id,
        kind: ObjectChangeKind::IdentReceived,
        revision: 2,
        changed_fields: 0,
        current: Some(WorldObject {
            object_id,
            name: name.to_owned(),
            is_owned: true,
            has_appraisal_data: true,
            ..WorldObject::default()
        }),
    })
}

#[test]
fn an_owned_item_the_player_selects_and_identifies_prints_its_info_line() {
    // Source: Mag-Tools DetectItemsIdentifiedByUser.cs and
    // ItemInfoPrinter.cs at 43ee2ec: selected, then appraised, the line
    // shows the profile's verdict, once, in text type 14.
    use ac_plugin_abi::world::SelectionChange;
    let ring = 0x5000_0001;
    let mut world = World::with_character("Mag");
    world.own_items(vec![item(ring, "Test Ring", 0x8)]);
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    keep_everything(&mut world, &mut plugin);
    world.clear_chat();

    world.send(
        &mut plugin,
        ac_plugin_abi::Event::SelectionChanged(SelectionChange {
            previous: None,
            selected: Some(ring),
        }),
    );
    world.send(&mut plugin, ident_received(ring, "Test Ring"));
    world.send(&mut plugin, ident_received(ring, "Test Ring"));

    let lines: Vec<(String, u32)> = world
        .fake
        .posted
        .iter()
        .map(|post| (post.text.clone(), post.log_text_type))
        .collect();
    assert_eq!(lines, vec![("+(Rings) Test Ring".to_owned(), 14)]);
}

fn selected(object_id: u32) -> ac_plugin_abi::Event {
    ac_plugin_abi::Event::SelectionChanged(ac_plugin_abi::world::SelectionChange {
        previous: None,
        selected: Some(object_id),
    })
}

#[test]
fn an_identify_of_another_object_than_the_one_selected_prints_nothing() {
    let ring = 0x5000_0001;
    let other = 0x5000_0002;
    let mut world = World::with_character("Mag");
    world.own_items(vec![
        item(ring, "Test Ring", 0x8),
        item(other, "Other", 0x8),
    ]);
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    world.clear_chat();
    world.send(&mut plugin, selected(other));
    world.send(&mut plugin, ident_received(ring, "Test Ring"));
    assert!(world.chat().is_empty(), "{:?}", world.chat());
    // The selected one still prints (the empty default profile keeps
    // nothing).
    world.send(&mut plugin, ident_received(other, "Other"));
    assert_eq!(world.chat(), vec!["- Other"]);
}

const CORPSE: u32 = 0x7000_0001;
const KEPT: u32 = 0x5100_0001;
const UNAPPRAISED: u32 = 0x5100_0002;

/// A profile whose first rule (keep, `value >= 0`) stays open until an
/// item is appraised, before a catch-all that keeps nothing: an
/// unappraised item needs its appraisal first.
fn rings_then_junk(world: &mut World, plugin: &mut mosstank::MossTank) {
    use mosstank_formats::utl::{self, LootAction, LootRule};
    let mut rings = LootRule::named("Rings", "value >= 0");
    rings.action = LootAction::Keep;
    let mut junk = LootRule::named("Junk", "*");
    junk.action = LootAction::NoLoot;
    world.profiles.insert(
        "mosstank/loot/MagRings.utl",
        &utl::serialize_rules(&[rings, junk]),
    );
    world.vt(plugin, "loot load MagRings");
    world.ticks(plugin, 5);
    assert_eq!(plugin.tank().unwrap().live_loot_rules().len(), 2);
}

/// `name` open, holding one appraised ring and one not.
fn open_container(world: &mut World, plugin: &mut mosstank::MossTank, name: &str) {
    use ac_plugin_abi::world::{ObjectClass, WorldObject};
    let seen = |object_id, name: &str, appraised| WorldObject {
        object_id,
        name: name.to_owned(),
        container_id: CORPSE,
        has_appraisal_data: appraised,
        ..WorldObject::default()
    };
    world.objects = Some(vec![
        WorldObject {
            object_id: CORPSE,
            name: name.to_owned(),
            object_class: ObjectClass::Corpse,
            ..WorldObject::default()
        },
        seen(KEPT, "Ring A", true),
        seen(UNAPPRAISED, "Ring B", false),
    ]);
    let contents: Vec<_> = [(KEPT, "Ring A"), (UNAPPRAISED, "Ring B")]
        .into_iter()
        .map(|(id, ring)| {
            let mut ring = item(id, ring, 0x8);
            ring.container_id = CORPSE;
            ring
        })
        .collect();
    world.set_reply::<calls::LootContents>(&contents);
    world.set_reply::<calls::WorldOpenContainer>(&CORPSE);
    // The appraisal slot stays taken, so the queue keeps the item.
    world.set_reply::<calls::WorldIdentify>(&ac_plugin_abi::items::ItemResult::of(
        ac_plugin_abi::items::ItemStatus::Busy,
    ));
    world.send(plugin, ac_plugin_abi::Event::ContainerOpened(CORPSE));
    for _ in 0..7 {
        world.fake.clock.monotonic_ms += 100;
        world.tick(plugin);
    }
}

fn lines(world: &World) -> Vec<(String, u32)> {
    world
        .fake
        .posted
        .iter()
        .map(|post| (post.text.clone(), post.log_text_type))
        .collect()
}

#[test]
fn an_opened_corpse_prints_its_kept_items_and_waits_for_the_appraisals_it_needs() {
    // Source: Mag-Tools DetectItemsInContainers.cs, ItemInfoPrinter.cs and
    // VirindiTankLootRuleProcessor.cs at 43ee2ec.
    let mut world = World::with_character("Mag");
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    rings_then_junk(&mut world, &mut plugin);
    world.clear_chat();

    open_container(&mut world, &mut plugin, "Corpse of Drudge");
    assert_eq!(lines(&world), vec![("+(Rings) Ring A".to_owned(), 14)]);
    assert!(world.with_tank(&mut plugin, |tank, _| {
        tank.mag().identify.contains(UNAPPRAISED)
    }));
    assert!(
        world
            .requests::<calls::WorldIdentify>()
            .contains(&UNAPPRAISED)
    );

    world.send(&mut plugin, ident_received(UNAPPRAISED, "Ring B"));
    assert_eq!(
        lines(&world),
        vec![
            ("+(Rings) Ring A".to_owned(), 14),
            ("+(Rings) Ring B".to_owned(), 14),
        ]
    );
}

#[test]
fn an_opened_housing_storage_prints_nothing() {
    let mut world = World::with_character("Mag");
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    rings_then_junk(&mut world, &mut plugin);
    world.clear_chat();
    open_container(&mut world, &mut plugin, "Storage");
    assert!(world.chat().is_empty(), "{:?}", world.chat());
    assert!(world.with_tank(&mut plugin, |tank, _| tank.mag().identify.is_empty()));
}

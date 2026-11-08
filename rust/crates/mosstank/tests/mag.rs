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

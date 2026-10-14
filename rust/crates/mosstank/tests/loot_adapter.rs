//! The looter's view of the client: the corpses in reach as the client
//! describes them (`PluginLootContainer`), and whether the owned inventory
//! is completely listed (`IItemAutomation.IsOwnedInventoryComplete`).

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::items::{AppraisalState, LootContainer, LootState};
use ac_plugin_abi::world::WorldObject;
use common::World;
use mosstank::automation::HostAutomation;
use mosstank_loot::host::{LootCorpse, LootHost};

const CORPSE: u32 = 0x8000_0101;

fn corpse(identified: bool, answered: bool) -> LootContainer {
    LootContainer {
        object_id: CORPSE,
        weenie_class_id: 21,
        name: "Corpse of Drudge Skulker".to_owned(),
        distance: 3.0,
        is_identified: identified,
        is_appraisal_answered: answered,
        long_description: if identified {
            "Killed by Tester.".to_owned()
        } else {
            String::new()
        },
        ..LootContainer::default()
    }
}

fn corpses(world: &mut World) -> Vec<LootCorpse> {
    world.with_host(|host| HostAutomation::new(host).loot_corpses(10.0))
}

fn answered(world: &mut World) -> bool {
    let corpses = corpses(world);
    assert_eq!(corpses.len(), 1);
    corpses[0].is_appraisal_answered
}

#[test]
fn a_corpse_the_client_has_no_answer_for_is_not_answered() {
    let mut world = World::with_character("Looter");
    world.set_reply::<calls::LootCorpses>(&vec![corpse(false, false)]);
    assert!(!answered(&mut world));
}

#[test]
fn a_corpse_the_client_says_was_answered_without_a_description_is_answered() {
    // A corpse that decayed meanwhile: answered, never described.
    let mut world = World::with_character("Looter");
    world.set_reply::<calls::LootCorpses>(&vec![corpse(false, true)]);
    assert!(answered(&mut world));
}

#[test]
fn the_clients_flag_alone_says_whether_a_corpse_was_answered() {
    // The appraisal slot's last answer and a refusal on the object are not
    // read: the client keeps the flag from the first answer on.
    let mut world = World::with_character("Looter");
    world.set_reply::<calls::LootCorpses>(&vec![corpse(false, false)]);
    world.set_reply::<calls::LootState>(&LootState {
        is_available: true,
        appraisal: AppraisalState {
            current_id: CORPSE,
            ..AppraisalState::default()
        },
        ..LootState::default()
    });
    world.set_reply::<calls::WorldObject>(&Some(WorldObject {
        object_id: CORPSE,
        last_appraisal_unsuccessful: true,
        ..WorldObject::default()
    }));
    assert!(!answered(&mut world));
}

#[test]
fn a_described_corpse_carries_the_clients_description() {
    let mut world = World::with_character("Looter");
    world.set_reply::<calls::LootCorpses>(&vec![corpse(true, true)]);
    let corpses = corpses(&mut world);
    assert_eq!(corpses.len(), 1);
    assert_eq!(corpses[0].object_id, CORPSE);
    assert_eq!(corpses[0].weenie_class_id, 21);
    assert_eq!(corpses[0].name, "Corpse of Drudge Skulker");
    assert_eq!(corpses[0].long_description, "Killed by Tester.");
    assert!(corpses[0].is_identified);
    assert!(corpses[0].is_appraisal_answered);
}

fn inventory_complete(world: &mut World) -> bool {
    world.with_host(|host| HostAutomation::new(host).owned_inventory_complete())
}

#[test]
fn the_owned_inventory_is_complete_when_the_client_says_so() {
    let mut world = World::with_character("Looter");
    world.set_reply::<calls::ItemsInventoryComplete>(&true);
    assert!(inventory_complete(&mut world));
    world.set_reply::<calls::ItemsInventoryComplete>(&false);
    assert!(!inventory_complete(&mut world));
}

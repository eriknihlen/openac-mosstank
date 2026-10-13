//! The looter's view of the client: what it reads of the corpses in reach
//! (register LOOT-3).

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::items::{AppraisalState, LootContainer, LootState};
use ac_plugin_abi::world::WorldObject;
use common::World;
use mosstank::automation::HostAutomation;
use mosstank_loot::host::LootHost;

const CORPSE: u32 = 0x8000_0101;

fn corpse(identified: bool) -> LootContainer {
    LootContainer {
        object_id: CORPSE,
        weenie_class_id: 21,
        name: "Corpse of Drudge Skulker".to_owned(),
        distance: 3.0,
        is_identified: identified,
        long_description: if identified {
            "Killed by Tester.".to_owned()
        } else {
            String::new()
        },
        ..LootContainer::default()
    }
}

fn answered(world: &mut World) -> bool {
    world.with_host(|host| {
        let mut client = HostAutomation::new(host);
        let corpses = client.loot_corpses(10.0);
        assert_eq!(corpses.len(), 1);
        corpses[0].is_appraisal_answered
    })
}

fn last_answer(world: &mut World, current_id: u32) {
    world.set_reply::<calls::LootState>(&LootState {
        is_available: true,
        appraisal: AppraisalState {
            current_id,
            ..AppraisalState::default()
        },
        ..LootState::default()
    });
}

#[test]
fn a_corpse_nobody_answered_for_is_not_answered() {
    let mut world = World::with_character("Looter");
    world.set_reply::<calls::LootCorpses>(&vec![corpse(false)]);
    last_answer(&mut world, 0);
    world.set_reply::<calls::WorldObject>(&None);
    assert!(!answered(&mut world));
}

#[test]
fn a_described_corpse_is_answered() {
    let mut world = World::with_character("Looter");
    world.set_reply::<calls::LootCorpses>(&vec![corpse(true)]);
    last_answer(&mut world, 0);
    world.set_reply::<calls::WorldObject>(&None);
    assert!(answered(&mut world));
}

#[test]
fn a_corpse_the_server_refused_to_describe_is_answered() {
    let mut world = World::with_character("Looter");
    world.set_reply::<calls::LootCorpses>(&vec![corpse(false)]);
    // Another object's answer came after: the refusal is the corpse's own.
    last_answer(&mut world, 0x8000_0999);
    world.set_reply::<calls::WorldObject>(&Some(WorldObject {
        object_id: CORPSE,
        last_appraisal_unsuccessful: true,
        ..WorldObject::default()
    }));
    assert!(answered(&mut world));
}

#[test]
fn a_corpse_whose_answer_is_the_last_one_is_answered() {
    let mut world = World::with_character("Looter");
    world.set_reply::<calls::LootCorpses>(&vec![corpse(false)]);
    last_answer(&mut world, CORPSE);
    world.set_reply::<calls::WorldObject>(&Some(WorldObject {
        object_id: CORPSE,
        ..WorldObject::default()
    }));
    assert!(answered(&mut world));
}

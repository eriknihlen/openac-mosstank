//! The inventory packer (`/ub autopack`), against Mag-Tools'
//! `InventoryPacker`: the packs a keep-up-to count names, the stack and the
//! move it picks, and a run from its first line to its last.

mod common;

use std::collections::HashSet;

use ac_plugin_abi::items::{
    AppraisalState, InventoryCommandKind, InventoryCompletion, InventoryItem,
};
use ac_plugin_abi::world::{ItemProperties, ObjectClass, WorldObject};
use common::Rig;
use mosstank_formats::utl::{LootAction, LootRule, VtankLootRequirement};
use mosstank_ub::tools::pack::{self, Misplaced};

const PLAYER: u32 = 1;
const FIRST_PACK: u32 = 10;
const SECOND_PACK: u32 = 11;

fn loose(id: u32, name: &str, container: u32) -> InventoryItem {
    InventoryItem {
        object_id: id,
        name: name.to_owned(),
        container_id: container,
        stack_size: 1,
        maximum_stack_size: 1,
        ..InventoryItem::default()
    }
}

fn stack(id: u32, name: &str, container: u32, size: i32, maximum: i32) -> InventoryItem {
    InventoryItem {
        stack_size: size,
        maximum_stack_size: maximum,
        ..loose(id, name, container)
    }
}

fn side_pack(id: u32, slot: i32, capacity: i32) -> InventoryItem {
    InventoryItem {
        items_capacity: capacity,
        container_slot: slot,
        ..loose(id, "Pack", PLAYER)
    }
}

fn keep_up_to(name: &str, expression: &str, packs: i32) -> LootRule {
    let mut rule = LootRule::default();
    name.clone_into(&mut rule.name);
    rule.action = LootAction::KeepUpTo;
    rule.keep_count = packs;
    rule.set_expression(expression);
    rule
}

/// "Tester" in the world with two side packs and `items`, every one known
/// to the client and (when `appraised`) described.
fn rig(items: Vec<InventoryItem>, appraised: bool) -> Rig {
    let mut rig = Rig::new();
    let character = rig.host.meta.character_mut();
    character.id = PLAYER;
    "Tester".clone_into(&mut character.name);
    character.main_pack_free_slots = 50;
    rig.host.meta.item_state.is_available = true;
    let mut owned = vec![side_pack(FIRST_PACK, 0, 24), side_pack(SECOND_PACK, 1, 24)];
    owned.extend(items);
    rig.host.meta.objects = owned
        .iter()
        .map(|item| WorldObject {
            object_id: item.object_id,
            name: item.name.clone(),
            object_class: ObjectClass::Misc,
            container_id: item.container_id,
            is_owned: true,
            has_appraisal_data: appraised,
            ..WorldObject::default()
        })
        .collect();
    rig.host.meta.owned_items = owned;
    rig.host.meta.submitted.clear();
    rig
}

fn save(rig: &mut Rig, name: &str, rules: Vec<LootRule>) {
    rig.services.loot_profiles.push((name.to_owned(), rules));
}

fn tick(rig: &mut Rig, seconds: f64) -> bool {
    rig.with_call(|call, commands| commands.tools_mut().pack.tick(call, seconds, true))
}

/// The client's answer to the request in flight: the item is where it was
/// sent, and the receipt says so.
fn land(rig: &mut Rig, object_id: u32, container: u32) {
    for item in &mut rig.host.meta.owned_items {
        if item.object_id == object_id {
            item.container_id = container;
        }
    }
    let state = &mut rig.host.meta.item_state;
    state.is_busy = false;
    state.last_inventory_completion = InventoryCompletion {
        revision: state.last_inventory_completion.revision + 1,
        kind: InventoryCommandKind::Move,
        source_id: object_id,
        weenie_error: 0,
    };
}

fn moves(rig: &Rig) -> Vec<(u32, u32)> {
    rig.host
        .meta
        .moves
        .iter()
        .map(|(item, pack, ..)| (*item, *pack))
        .collect()
}

#[test]
fn a_keep_up_to_count_names_its_packs_one_digit_each() {
    assert_eq!(Some(vec![0]), pack::pack_numbers(0));
    assert_eq!(Some(vec![3, 1, 2]), pack::pack_numbers(312));
    assert_eq!(None, pack::pack_numbers(-1));
}

#[test]
fn packs_are_numbered_by_slot_after_the_main_pack() {
    let focus = InventoryItem {
        public_flags: 0x0080_0000,
        container_slot: 1,
        ..loose(12, "Foci of Strife", PLAYER)
    };
    let items = [
        side_pack(SECOND_PACK, 2, 24),
        focus,
        side_pack(FIRST_PACK, 0, 24),
    ];

    assert_eq!(
        vec![PLAYER, FIRST_PACK, 0, SECOND_PACK],
        pack::numbered_packs(&items, PLAYER)
    );
}

#[test]
fn part_stacks_of_one_name_in_one_pack_are_poured_together() {
    let items = [
        stack(20, "Arrow", PLAYER, 100, 100),
        stack(21, "Arrow", PLAYER, 40, 100),
        stack(22, "Arrow", FIRST_PACK, 30, 100),
        stack(23, "Bolt", PLAYER, 30, 100),
        stack(24, "Arrow", PLAYER, 10, 100),
    ];

    assert_eq!(Some((21, 24)), pack::plan_stack(&items, &HashSet::new()));
    assert_eq!(
        None,
        pack::plan_stack(&items, &HashSet::from([21, 24])),
        "a stack left alone is not poured"
    );
}

fn misplaced(id: u32, container: u32, targets: &[u32]) -> Misplaced {
    Misplaced {
        object_id: id,
        container_id: container,
        amount: 1,
        targets: targets.to_vec(),
    }
}

#[test]
fn every_first_choice_is_tried_before_any_second() {
    let items = [
        misplaced(20, PLAYER, &[FIRST_PACK, SECOND_PACK]),
        misplaced(21, PLAYER, &[SECOND_PACK]),
    ];
    let first_is_full = |pack: u32| i32::from(pack != FIRST_PACK);

    assert_eq!(
        Some((21, SECOND_PACK, 1)),
        pack::plan_move(&items, &first_is_full)
    );
    assert_eq!(
        Some((20, SECOND_PACK, 1)),
        pack::plan_move(&items[..1], &first_is_full)
    );
}

#[test]
fn an_item_in_a_later_choice_stays_while_the_better_packs_are_full() {
    // In its second pack with the first full: it is not sent on to the
    // third. A pack that is not there (0) takes nothing.
    let items = [
        misplaced(20, SECOND_PACK, &[FIRST_PACK, SECOND_PACK, PLAYER]),
        misplaced(21, PLAYER, &[0]),
    ];
    let first_is_full = |pack: u32| i32::from(pack != FIRST_PACK);

    assert_eq!(None, pack::plan_move(&items, &first_is_full));
    assert_eq!(
        Some((20, FIRST_PACK, 1)),
        pack::plan_move(&items, &|_| 1),
        "and moves up once the first has room"
    );
}

#[test]
fn without_a_profile_the_run_does_not_start() {
    let mut rig = rig(vec![loose(20, "Trade Note", PLAYER)], true);

    rig.ub("autopack");

    assert_eq!(
        "[UB] Error: Auto Pack profile does not exist: Tester.AutoPack.utl or Default.AutoPack.utl",
        rig.single_message()
    );
    assert!(!rig.commands.tools().pack.is_running());
}

#[test]
fn a_run_moves_each_item_to_its_pack_and_says_when_it_is_done() {
    let mut rig = rig(
        vec![
            loose(20, "Trade Note", PLAYER),
            loose(21, "Healing Kit", FIRST_PACK),
            loose(22, "Pyreal Mote", PLAYER),
        ],
        true,
    );
    save(
        &mut rig,
        "Tester.AutoPack",
        vec![
            keep_up_to("Notes", "name ~= trade", 1),
            keep_up_to("Kits", "name ~= kit", 20),
        ],
    );

    rig.ub("autopack");
    assert_eq!("[UB] Auto Pack - Started.", rig.single_message());

    // One move at a time, each waiting for its receipt.
    assert!(tick(&mut rig, 0.0));
    assert!(tick(&mut rig, 0.5));
    assert_eq!(1, rig.host.meta.moves.len());
    let (first, to) = moves(&rig)[0];
    land(&mut rig, first, to);
    assert!(tick(&mut rig, 0.1));
    let (second, to) = moves(&rig)[1];
    land(&mut rig, second, to);

    assert!(!tick(&mut rig, 0.1));
    let mut made = moves(&rig);
    made.sort_unstable();
    // The mote matches no rule and stays where it is.
    assert_eq!(vec![(20, FIRST_PACK), (21, SECOND_PACK)], made);
    assert_eq!(
        (20, FIRST_PACK, 1, 0, true),
        *rig.host
            .meta
            .moves
            .iter()
            .find(|made| made.0 == 20)
            .expect("the note moved"),
        "the whole item, joining a stack already there"
    );
    assert_eq!(
        vec![
            "[UB] Auto Pack - Started.".to_owned(),
            "[UB] Auto Pack - Completed.".to_owned()
        ],
        rig.messages()
    );
    assert!(!rig.commands.tools().pack.is_running());
}

#[test]
fn the_default_profile_serves_a_character_without_its_own() {
    let mut rig = rig(vec![loose(20, "Trade Note", PLAYER)], true);
    save(
        &mut rig,
        "Default.AutoPack",
        vec![keep_up_to("Notes", "name ~= trade", 2)],
    );

    rig.ub("autopack");
    assert!(tick(&mut rig, 0.0));

    assert_eq!(vec![(20, SECOND_PACK)], moves(&rig));
}

#[test]
fn a_named_profile_is_used_and_a_stop_ends_the_run() {
    let mut rig = rig(vec![loose(20, "Trade Note", PLAYER)], true);
    save(
        &mut rig,
        "sorting",
        vec![keep_up_to("Notes", "name ~= trade", 1)],
    );

    rig.ub("autopack sorting.utl");
    rig.ub("autopack sorting");
    rig.ub("autopack stop");
    rig.ub("autopack stop");

    assert_eq!(
        vec![
            "[UB] Auto Pack - Started.".to_owned(),
            "[UB] Error: Auto Pack is already running. Use /ub autopack stop to end it.".to_owned(),
            "[UB] Auto Pack - Stopped.".to_owned(),
            "[UB] Error: Auto Pack is not running.".to_owned(),
        ],
        rig.messages()
    );
    assert!(!tick(&mut rig, 1.0));
    assert!(rig.host.meta.moves.is_empty());
}

#[test]
fn part_stacks_are_poured_before_anything_moves() {
    let mut rig = rig(
        vec![
            stack(20, "Arrow", PLAYER, 40, 100),
            stack(21, "Arrow", PLAYER, 10, 100),
        ],
        true,
    );
    save(
        &mut rig,
        "Tester.AutoPack",
        vec![keep_up_to("Arrows", "name ~= arrow", 1)],
    );

    rig.ub("autopack");
    assert!(tick(&mut rig, 0.0));

    assert_eq!(vec![(20, 21, 0)], rig.host.merges);
    assert!(rig.host.meta.moves.is_empty());
}

#[test]
fn an_item_its_rule_cannot_decide_is_appraised_first() {
    let mut rig = rig(vec![loose(20, "Hauberk", PLAYER)], false);
    // Armor level 300 or better: an appraisal-only key.
    let mut good_armor = keep_up_to("good armor", "*", 1);
    good_armor.has_imported_requirements = true;
    good_armor.vtank_requirements = vec![VtankLootRequirement {
        kind: 3,
        payload: "300\r\n28\r\n".to_owned(),
    }];
    save(&mut rig, "Tester.AutoPack", vec![good_armor]);

    rig.ub("autopack");
    assert!(tick(&mut rig, 0.0));
    assert!(tick(&mut rig, 1.0));

    // The run waits on the one request instead of finishing.
    assert_eq!(vec![20], rig.host.identified);
    assert!(rig.host.meta.moves.is_empty());
    assert!(rig.commands.tools().pack.is_running());
}

/// Armor level 300 or better: a key only an appraisal reports.
fn good_armor(packs: i32) -> LootRule {
    let mut rule = keep_up_to("good armor", "*", packs);
    rule.has_imported_requirements = true;
    rule.vtank_requirements = vec![VtankLootRequirement {
        kind: 3,
        payload: "300\r\n28\r\n".to_owned(),
    }];
    rule
}

/// Any armor, by the object class every object carries.
fn any_armor(packs: i32) -> LootRule {
    let mut rule = keep_up_to("any armor", "*", packs);
    rule.has_imported_requirements = true;
    rule.vtank_requirements = vec![VtankLootRequirement {
        kind: 7,
        payload: format!("{}\r\n", ObjectClass::Armor.tag()),
    }];
    rule
}

fn hauberk() -> InventoryItem {
    InventoryItem {
        object_class: ObjectClass::Armor,
        ..loose(20, "Hauberk", PLAYER)
    }
}

/// The appraisal of `object_id` lands, with its armor level.
fn appraise(rig: &mut Rig, object_id: u32, armor_level: i32) {
    for object in &mut rig.host.meta.objects {
        if object.object_id == object_id {
            object.has_appraisal_data = true;
        }
    }
    let mut properties = ItemProperties::default();
    properties.ints.push((28, armor_level));
    rig.host.meta.properties.insert(object_id, properties);
}

#[test]
fn an_item_a_later_rule_would_send_elsewhere_is_appraised_before_it_is_packed() {
    let mut rig = rig(vec![hauberk()], false);
    save(
        &mut rig,
        "Tester.AutoPack",
        vec![good_armor(1), any_armor(2)],
    );

    rig.ub("autopack");
    assert!(tick(&mut rig, 0.0));

    // The first rule turns on the armor level, and it would send the
    // hauberk to another pack than the second: it is appraised first.
    assert_eq!(vec![20], rig.host.identified);
    assert!(rig.host.meta.moves.is_empty());

    appraise(&mut rig, 20, 300);
    assert!(tick(&mut rig, 0.5));
    assert_eq!(vec![(20, FIRST_PACK)], moves(&rig));
}

#[test]
fn an_item_every_open_rule_would_pack_alike_is_not_appraised() {
    let mut rig = rig(vec![hauberk()], false);
    save(
        &mut rig,
        "Tester.AutoPack",
        vec![good_armor(1), any_armor(1)],
    );

    rig.ub("autopack");
    assert!(tick(&mut rig, 0.0));

    assert!(rig.host.identified.is_empty());
    assert_eq!(vec![(20, FIRST_PACK)], moves(&rig));
}

#[test]
fn an_item_whose_appraisal_slot_stays_taken_is_packed_by_what_is_known() {
    let mut rig = rig(vec![hauberk()], false);
    save(
        &mut rig,
        "Tester.AutoPack",
        vec![good_armor(1), any_armor(2)],
    );
    // Somebody else's question holds the appraisal slot for good.
    rig.host.appraisal = AppraisalState {
        revision: 1,
        awaiting_id: 99,
        ..AppraisalState::default()
    };

    rig.ub("autopack");
    let mut seconds = 0.0;
    while rig.host.meta.moves.is_empty() {
        tick(&mut rig, 0.5);
        seconds += 0.5;
        assert!(seconds < 60.0, "the run waits on the slot for good");
    }

    assert!(rig.host.identified.is_empty());
    assert!(
        seconds >= pack::APPRAISAL_SLOT_WAIT_SECONDS,
        "gave up after {seconds} s"
    );
    // Without the armor level only the second rule can match.
    assert_eq!(vec![(20, SECOND_PACK)], moves(&rig));
    assert_eq!(
        vec![
            "[UB] Auto Pack - Started.".to_owned(),
            "[UB] Auto Pack - Could not appraise item: 20, Hauberk".to_owned(),
        ],
        rig.messages()
    );
}

#[test]
fn a_run_held_by_another_owner_does_not_count_against_the_item() {
    let mut rig = rig(vec![loose(20, "Trade Note", PLAYER)], true);
    save(
        &mut rig,
        "Tester.AutoPack",
        vec![keep_up_to("Notes", "name ~= trade", 1)],
    );

    rig.ub("autopack");
    assert!(tick(&mut rig, 0.0));
    assert_eq!(1, rig.host.meta.moves.len());

    // A buff burst holds the character for fifteen seconds; the move's
    // receipt never came.
    assert!(!rig.with_call(|call, commands| commands.tools_mut().pack.tick(call, 15.0, false)));
    assert!(tick(&mut rig, 0.1));

    assert_eq!(2, rig.host.meta.moves.len(), "the note is asked for again");
    assert_eq!(vec!["[UB] Auto Pack - Started.".to_owned()], rig.messages());
}

#[test]
fn a_stack_left_alone_is_not_poured_onto_either() {
    let items = [
        stack(20, "Arrow", PLAYER, 40, 100),
        stack(21, "Arrow", PLAYER, 10, 100),
    ];

    assert_eq!(None, pack::plan_stack(&items, &HashSet::from([20])));
    assert_eq!(None, pack::plan_stack(&items, &HashSet::from([21])));
}

#[test]
fn worn_and_wielded_items_packs_and_foci_stay_where_they_are() {
    let worn = InventoryItem {
        equipped_location: 0x0000_0200,
        wielder_id: PLAYER,
        container_id: 0,
        ..loose(20, "Hauberk", 0)
    };
    let wielded = InventoryItem {
        wielder_id: PLAYER,
        container_id: 0,
        ..loose(21, "Sword", 0)
    };
    let focus = InventoryItem {
        public_flags: 0x0080_0000,
        container_slot: 2,
        ..loose(22, "Foci of Strife", PLAYER)
    };
    let mut rig = rig(
        vec![worn, wielded, focus, loose(23, "Trade Note", PLAYER)],
        true,
    );
    save(
        &mut rig,
        "Tester.AutoPack",
        vec![keep_up_to("Everything", "*", 2)],
    );

    rig.ub("autopack");
    for _ in 0..20 {
        let (item, to) = match moves(&rig).last() {
            Some(made) => *made,
            None => (0, 0),
        };
        if item != 0 {
            land(&mut rig, item, to);
        }
        if !tick(&mut rig, 0.5) {
            break;
        }
    }

    // Only the note moves: the side packs, the focus and what is worn or
    // wielded match the rule but are never packed.
    assert_eq!(vec![(23, SECOND_PACK)], moves(&rig));
    assert_eq!(
        Some(&"[UB] Auto Pack - Completed.".to_owned()),
        rig.messages().last()
    );
}

#[test]
fn a_full_inventory_is_judged_over_several_looks_before_anything_moves() {
    let count = u32::try_from(pack::JUDGEMENTS_PER_LOOK).expect("a few") + 2;
    let notes = (0..count)
        .map(|index| loose(100 + index, "Trade Note", PLAYER))
        .collect();
    let mut rig = rig(notes, true);
    save(
        &mut rig,
        "Tester.AutoPack",
        vec![keep_up_to("Notes", "name ~= trade", 1)],
    );

    rig.ub("autopack");
    // The first look judges as many as it may, and moves nothing yet.
    assert!(tick(&mut rig, 0.0));
    assert!(rig.host.meta.moves.is_empty());

    // The second judges the rest; a judged item is not judged again.
    assert!(tick(&mut rig, 0.1));
    assert_eq!(1, rig.host.meta.moves.len());
}

#[test]
fn an_item_that_will_not_move_is_left_alone_after_ten_seconds() {
    let mut rig = rig(vec![loose(20, "Trade Note", PLAYER)], true);
    save(
        &mut rig,
        "Tester.AutoPack",
        vec![keep_up_to("Notes", "name ~= trade", 1)],
    );

    rig.ub("autopack");
    // Every request goes unanswered and the note never leaves the main pack.
    let mut frames = 0;
    while rig.commands.tools().pack.is_running() {
        tick(&mut rig, 0.5);
        frames += 1;
        assert!(frames < 100, "the run never ended");
    }

    assert_eq!(
        vec![
            "[UB] Auto Pack - Started.".to_owned(),
            "[UB] Auto Pack - Skipping item: 20, Trade Note".to_owned(),
            "[UB] Auto Pack - Completed.".to_owned()
        ],
        rig.messages()
    );
    assert!(rig.host.meta.moves.iter().all(|made| made.0 == 20));
    assert!(rig.host.meta.moves.len() > 1);
}

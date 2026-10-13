//! The mana jobs the looter takes on by itself and the reasons it leaves an
//! item behind (`LootingTests.ManaDonor.cs`, `LootingTests.ImplicitMana.cs`).

mod common;

use ac_plugin_abi::items::{InventoryItem, ItemResult, ItemStatus, ItemUseCompletion};
use ac_plugin_abi::world::ObjectClass;
use common::{
    Lines, World, bind_locks, capture_log, chain_corpse, enabled_settings, item, requirement,
    strings, vtank_rule,
};
use mosstank_automation::locks::{ActionLockKind, SharedActionLocks};
use mosstank_dotnet::collections::{OrdinalStringMap, OrdinalStringSet};
use mosstank_formats::utl::LootAction;
use mosstank_loot::LootController;
use mosstank_settings::model::{ConsumableCategory, LootSettings};

const DRAIN_CORPSE: u32 = 0x7000_A001;
const DRAIN_STONE: u32 = 0x7000_A010;
const DRAIN_DONOR: u32 = 0x7000_A011;

/// An empty stone the profile knows by name.
fn empty_stone() -> InventoryItem {
    InventoryItem {
        object_class: ObjectClass::ManaStone,
        ..item(DRAIN_STONE, "Major Mana Stone", 47)
    }
}

/// An item worth emptying: it holds mana and nobody made it theirs.
fn donor_with(id: u32, name: &str, mana: i32) -> InventoryItem {
    InventoryItem {
        effects: 1,
        item_current_mana: mana,
        workmanship: 5.0,
        ..item(id, name, 48)
    }
}

fn donor() -> InventoryItem {
    donor_with(DRAIN_DONOR, "Sturdy Wand", 1500)
}

fn drain_world(owned: Vec<InventoryItem>, contents: Vec<InventoryItem>) -> World {
    World {
        owned,
        contents,
        mana_drain_consumes_donor: true,
        ..World::with_corpses(vec![chain_corpse(DRAIN_CORPSE, 3.0)])
    }
}

/// A rule file that says nothing about wands or stones.
fn leave_junk() -> Vec<mosstank_formats::utl::LootRule> {
    vec![vtank_rule(
        "Leave junk",
        LootAction::NoLoot,
        vec![requirement(1, &["Junk", "1"])],
    )]
}

fn stone_helper(
    kind: ConsumableCategory,
) -> (OrdinalStringSet, OrdinalStringMap<ConsumableCategory>) {
    let mut names = OrdinalStringSet::new();
    names.insert("Major Mana Stone".to_owned());
    let mut kinds = OrdinalStringMap::new();
    kinds.insert("Major Mana Stone".to_owned(), kind);
    (names, kinds)
}

fn mana_settings(rules: Vec<mosstank_formats::utl::LootRule>) -> LootSettings {
    LootSettings {
        mana_stone_loot_count: 4,
        mana_tank_minimum_mana: 1000,
        ..enabled_settings(rules)
    }
}

fn drain_controller(world: &mut World) -> (LootController, SharedActionLocks, Lines) {
    let _ = world;
    let (names, kinds) = stone_helper(ConsumableCategory::ManaStone);
    let mut controller =
        LootController::with_consumables(mana_settings(leave_junk()), names, kinds);
    let locks = bind_locks(&mut controller);
    let log = capture_log(&mut controller);
    (controller, locks, log)
}

/// The client answers the one description it was asked for.
fn answer_description(world: &mut World) {
    let asked = world.appraisal_state.awaiting_id;
    if asked == 0 {
        return;
    }
    world.complete_appraisal(asked, false);
}

/// One pass of everything around the looter.
fn drain_pass(world: &mut World, controller: &mut LootController, locks: &SharedActionLocks) {
    let seconds = 0.5;
    locks.borrow_mut().advance(seconds);
    controller.tick_identification(world, seconds);
    answer_description(world);
    controller.tick_identification(world, seconds);
    let before = world.picked.len();
    controller.tick(world, seconds, true);
    world.settle_mana_drains();
    for index in before..world.picked.len() {
        let pulled = world.picked[index];
        let Some(taken) = world
            .contents
            .iter()
            .find(|item| item.object_id == pulled)
            .cloned()
        else {
            continue;
        };
        world.contents.retain(|item| item.object_id != pulled);
        world.owned.push(taken);
        let revision = world.inventory_completion.revision + 1;
        world.picked_up(revision, pulled);
    }
}

/// Opens the staged corpse and works it through.
fn work_the_corpse(world: &mut World, controller: &mut LootController, locks: &SharedActionLocks) {
    assert!(controller.tick(world, 1.0, true));
    world.requested = DRAIN_CORPSE;
    world.current = DRAIN_CORPSE;
    assert!(controller.observe_corpse_opened(world));
    for _ in 0..8 {
        drain_pass(world, controller, locks);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ManaDonor.cs :: ResetDoesNotForgetAnOwnedDonorWaitingForItsStone
#[test]
fn reset_does_not_forget_an_owned_donor_waiting_for_its_stone() {
    let mut world = drain_world(vec![empty_stone(), donor()], Vec::new());
    let (mut controller, _locks, _log) = drain_controller(&mut world);
    controller.mark_owned(&mut world, DRAIN_DONOR, LootAction::ManaTank);
    controller.reset(&mut world);
    controller.tick(&mut world, 0.2, true);
    assert_eq!(world.applied, [(DRAIN_STONE, DRAIN_DONOR)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ManaDonor.cs :: AnUnavailableCommandKeepsTheDonorForWhenTheHostIsReady
#[test]
fn an_unavailable_command_keeps_the_donor_for_when_the_host_is_ready() {
    let mut world = drain_world(vec![empty_stone(), donor()], Vec::new());
    world
        .apply_results
        .push_back(ItemResult::of(ItemStatus::Unavailable));
    let (mut controller, _locks, _log) = drain_controller(&mut world);
    controller.mark_owned(&mut world, DRAIN_DONOR, LootAction::ManaTank);
    controller.tick(&mut world, 0.2, true);
    controller.tick(&mut world, 0.2, true);
    assert_eq!(world.applied.len(), 2);
    assert!(
        controller
            .classified_owned_items()
            .contains_key(&DRAIN_DONOR)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ManaDonor.cs :: LateDrainConfirmationReleasesThePairAfterTimeout
#[test]
fn late_drain_confirmation_releases_the_pair_after_timeout() {
    let mut world = drain_world(vec![empty_stone(), donor()], Vec::new());
    world.mana_drain_consumes_donor = false;
    let (mut controller, _locks, _log) = drain_controller(&mut world);
    controller.mark_owned(&mut world, DRAIN_DONOR, LootAction::ManaTank);
    controller.tick(&mut world, 0.2, true);
    controller.tick(&mut world, 5.0, true);
    world.owned = vec![InventoryItem {
        effects: 1,
        ..empty_stone()
    }];
    world.use_completion = ItemUseCompletion {
        revision: 1,
        source_id: DRAIN_STONE,
        target_id: DRAIN_DONOR,
        weenie_error: 0,
    };
    controller.tick(&mut world, 0.2, true);
    assert!(
        !controller
            .classified_owned_items()
            .contains_key(&DRAIN_DONOR)
    );
    assert_eq!(world.applied.len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ManaDonor.cs :: DiscardedDonorIsNotDrainedIfPickedUpAgain
#[test]
fn discarded_donor_is_not_drained_if_picked_up_again() {
    let mut world = drain_world(vec![empty_stone(), donor()], Vec::new());
    let (mut controller, _locks, _log) = drain_controller(&mut world);
    controller.mark_owned(&mut world, DRAIN_DONOR, LootAction::ManaTank);
    world.owned = vec![empty_stone()];
    controller.observe_mana_donors(&mut world);
    world.owned = vec![empty_stone(), donor()];
    controller.tick(&mut world, 0.2, true);
    assert!(world.applied.is_empty());
    assert!(
        !controller
            .classified_owned_items()
            .contains_key(&DRAIN_DONOR)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ManaDonor.cs :: IncompleteInventoryDoesNotForgetADonorAndMovingItBetweenPacksKeepsIt
#[test]
fn incomplete_inventory_does_not_forget_a_donor_and_moving_it_between_packs_keeps_it() {
    let mut world = drain_world(vec![empty_stone(), donor()], Vec::new());
    let (mut controller, _locks, _log) = drain_controller(&mut world);
    controller.mark_owned(&mut world, DRAIN_DONOR, LootAction::ManaTank);
    world.is_owned_inventory_complete = false;
    world.owned.clear();
    controller.observe_mana_donors(&mut world);
    world.owned = vec![
        empty_stone(),
        InventoryItem {
            container_id: 0x7000_9999,
            ..donor()
        },
    ];
    world.is_owned_inventory_complete = true;
    controller.tick(&mut world, 0.2, true);
    assert_eq!(world.applied, [(DRAIN_STONE, DRAIN_DONOR)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ManaDonor.cs :: AnUncertainStoneCannotAttractReplacementDonorLoot
#[test]
fn an_uncertain_stone_cannot_attract_replacement_donor_loot() {
    let mut world = drain_world(
        vec![empty_stone(), donor()],
        vec![donor_with(DRAIN_DONOR + 1, "Sturdy Wand", 1500)],
    );
    world.mana_drain_consumes_donor = false;
    let (mut controller, locks, _log) = drain_controller(&mut world);
    controller.mark_owned(&mut world, DRAIN_DONOR, LootAction::ManaTank);
    controller.tick(&mut world, 0.2, true);
    locks.borrow_mut().advance(5.0);
    controller.tick(&mut world, 5.0, true);
    world.owned = vec![empty_stone()];
    controller.observe_mana_donors(&mut world);
    work_the_corpse(&mut world, &mut controller, &locks);
    assert!(world.picked.is_empty());
    assert_eq!(world.applied.len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ManaDonor.cs :: ADonorIsOnlyTakenWhenAnEmptyStoneIsCarried
#[test]
fn a_donor_is_only_taken_when_an_empty_stone_is_carried() {
    for stone_is_empty in [true, false] {
        let stone = if stone_is_empty {
            empty_stone()
        } else {
            InventoryItem {
                effects: 1,
                ..empty_stone()
            }
        };
        let mut world = drain_world(vec![stone], vec![donor()]);
        let (mut controller, locks, logged) = drain_controller(&mut world);

        work_the_corpse(&mut world, &mut controller, &locks);

        if stone_is_empty {
            assert_eq!(world.picked, [DRAIN_DONOR]);
            assert!(
                logged
                    .borrow()
                    .iter()
                    .any(|line| line == "LootDecision: Sturdy Wand -> ManaTank (ManaTank)")
            );
        } else {
            assert!(world.picked.is_empty());
            assert!(world.applied.is_empty());
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ManaDonor.cs :: AStoneNobodyHasAppraisedIsStillSomewhereToPutTheMana
#[test]
fn a_stone_nobody_has_appraised_is_still_somewhere_to_put_the_mana() {
    let mut world = drain_world(vec![empty_stone()], vec![donor()]);
    world.unassessed.insert(DRAIN_STONE);
    let (mut controller, locks, _log) = drain_controller(&mut world);

    work_the_corpse(&mut world, &mut controller, &locks);

    assert_eq!(world.applied, [(DRAIN_STONE, DRAIN_DONOR)]);
    assert!(!world.owned.iter().any(|item| item.object_id == DRAIN_DONOR));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ManaDonor.cs :: TheDrainWaitsForTheItemSlotAndHoldsItWhileItRuns
#[test]
fn the_drain_waits_for_the_item_slot_and_holds_it_while_it_runs() {
    let mut world = drain_world(vec![empty_stone()], vec![donor()]);
    let (mut controller, locks, _log) = drain_controller(&mut world);

    assert!(controller.tick(&mut world, 1.0, true));
    world.requested = DRAIN_CORPSE;
    world.current = DRAIN_CORPSE;
    assert!(controller.observe_corpse_opened(&mut world));
    for _ in 0..8 {
        if controller
            .classified_owned_items()
            .contains_key(&DRAIN_DONOR)
        {
            break;
        }
        drain_pass(&mut world, &mut controller, &locks);
    }
    assert_eq!(world.picked, [DRAIN_DONOR]);

    // Somebody else is mid-use: the drain waits its turn.
    locks.borrow_mut().arm(ActionLockKind::ItemUse, 60.0);
    for _ in 0..4 {
        controller.tick(&mut world, 0.2, true);
    }
    assert!(world.applied.is_empty());

    locks.borrow_mut().release(ActionLockKind::ItemUse);
    controller.tick(&mut world, 0.2, true);

    assert_eq!(world.applied, [(DRAIN_STONE, DRAIN_DONOR)]);
    assert!(locks.borrow_mut().is_locked(ActionLockKind::ItemUse));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ManaDonor.cs :: ARefusedDrainIsNotRepeated
#[test]
fn a_refused_drain_is_not_repeated() {
    let mut world = drain_world(vec![empty_stone()], vec![donor()]);
    world
        .apply_results
        .push_back(ItemResult::of(ItemStatus::InvalidItem));
    let (mut controller, locks, logged) = drain_controller(&mut world);

    work_the_corpse(&mut world, &mut controller, &locks);
    for _ in 0..6 {
        drain_pass(&mut world, &mut controller, &locks);
    }

    assert_eq!(world.applied.len(), 1);
    assert!(
        controller
            .classified_owned_items()
            .contains_key(&DRAIN_DONOR)
    );
    assert!(
        logged
            .borrow()
            .iter()
            .any(|line| line.starts_with("ManaDrain: could not empty Sturdy Wand"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ManaDonor.cs :: ABusyClientOnlyDelaysTheDrain
#[test]
fn a_busy_client_only_delays_the_drain() {
    let mut world = drain_world(vec![empty_stone()], vec![donor()]);
    world
        .apply_results
        .push_back(ItemResult::of(ItemStatus::Busy));
    let (mut controller, locks, _log) = drain_controller(&mut world);

    work_the_corpse(&mut world, &mut controller, &locks);
    for _ in 0..4 {
        drain_pass(&mut world, &mut controller, &locks);
    }

    assert_eq!(
        world.applied,
        [(DRAIN_STONE, DRAIN_DONOR), (DRAIN_STONE, DRAIN_DONOR)]
    );
    assert!(!world.owned.iter().any(|item| item.object_id == DRAIN_DONOR));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ManaDonor.cs :: OneStoneTakesOneDonorAndTheOtherIsLeft
#[test]
fn one_stone_takes_one_donor_and_the_other_is_left() {
    const SECOND: u32 = 0x7000_A012;
    let mut world = drain_world(
        vec![empty_stone()],
        vec![donor(), donor_with(SECOND, "Heavy Wand", 1800)],
    );
    let (mut controller, locks, _log) = drain_controller(&mut world);

    work_the_corpse(&mut world, &mut controller, &locks);

    assert_eq!(world.picked.len(), 1);
    assert_eq!(world.applied, [(DRAIN_STONE, DRAIN_DONOR)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ManaDonor.cs :: TheDrainSpeaksOnTheLootChannel
#[test]
fn the_drain_speaks_on_the_loot_channel() {
    let mut world = drain_world(vec![empty_stone()], vec![donor()]);
    let (mut controller, locks, logged) = drain_controller(&mut world);

    work_the_corpse(&mut world, &mut controller, &locks);

    let lines = logged.borrow();
    assert!(
        lines
            .iter()
            .any(|line| line == "ManaDrain: emptying Sturdy Wand into Major Mana Stone")
    );
    assert!(
        lines
            .iter()
            .any(|line| line == "ManaDrain: filled Major Mana Stone from Sturdy Wand")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ImplicitMana.cs :: ThreeReasonsForLeavingAnItemBehindReadDifferently
#[test]
fn three_reasons_for_leaving_an_item_behind_read_differently() {
    const CORPSE: u32 = 0x7000_9001;
    let mut world = World {
        owned: vec![InventoryItem {
            stack_size: 7,
            ..item(0x7000_9010, "Treated Healing Kit", 44)
        }],
        ..World::with_corpses(vec![chain_corpse(CORPSE, 3.0)])
    };
    let mut kits = vtank_rule(
        "Kits",
        LootAction::KeepUpTo,
        vec![requirement(1, &["Healing Kit", "1"])],
    );
    kits.keep_count = 5;
    let mut rules = leave_junk();
    rules.push(kits);
    let mut controller = LootController::new(enabled_settings(rules));
    let logged = capture_log(&mut controller);
    let _locks = bind_locks(&mut controller);

    assert!(controller.tick(&mut world, 1.0, true));
    world.requested = CORPSE;
    world.current = CORPSE;
    world.contents = vec![
        item(0x7000_9011, "Rusty Junk", 45),
        item(0x7000_9012, "Treated Healing Kit", 44),
        item(0x7000_9013, "Mystery Trinket", 46),
    ];
    assert!(controller.observe_corpse_opened(&mut world));
    controller.tick_identification(&mut world, 0.5);

    let lines = logged.borrow();
    assert!(
        lines
            .iter()
            .any(|line| line == "LootDecision: Rusty Junk -> Leave junk says leave it")
    );
    assert!(lines.iter().any(|line| {
        line == "LootDecision: Treated Healing Kit -> Kits keeps up to 5 (holding 7)"
    }));
    assert!(
        lines
            .iter()
            .any(|line| line == "LootDecision: Mystery Trinket -> no rule matched")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ImplicitMana.cs :: OnlyAHelperRowOfTheStoneKindMakesTheLooterTakeAStone
#[test]
fn only_a_helper_row_of_the_stone_kind_makes_the_looter_take_a_stone() {
    for (filed_as_a_stone, expected) in
        [(true, "ManaStone (ManaStone)"), (false, "no rule matched")]
    {
        let kind = if filed_as_a_stone {
            ConsumableCategory::ManaStone
        } else {
            ConsumableCategory::ManaFood
        };
        const CORPSE: u32 = 0x7000_9101;
        let mut world = World::with_corpses(vec![chain_corpse(CORPSE, 3.0)]);
        let settings = LootSettings {
            mana_stone_loot_count: 4,
            ..enabled_settings(leave_junk())
        };
        let (names, kinds) = stone_helper(kind);
        let mut controller = LootController::with_consumables(settings, names, kinds);
        let logged = capture_log(&mut controller);
        let _locks = bind_locks(&mut controller);

        assert!(controller.tick(&mut world, 1.0, true));
        world.requested = CORPSE;
        world.current = CORPSE;
        world.contents = vec![InventoryItem {
            object_class: ObjectClass::ManaStone,
            ..item(0x7000_9110, "Major Mana Stone", 47)
        }];
        assert!(controller.observe_corpse_opened(&mut world));
        controller.tick_identification(&mut world, 0.5);

        let wanted = format!("LootDecision: Major Mana Stone -> {expected}");
        assert!(logged.borrow().contains(&wanted), "{expected}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ImplicitMana.cs :: ACorpseDonorIsDrainedIntoASpareStoneUnlessSomebodyWroteOnIt
#[test]
fn a_corpse_donor_is_drained_into_a_spare_stone_unless_somebody_wrote_on_it() {
    const CORPSE: u32 = 0x7000_9201;
    const STONE: u32 = 0x7000_9210;
    const DONOR: u32 = 0x7000_9211;
    for inscribed in [false, true] {
        let spare = InventoryItem {
            object_class: ObjectClass::ManaStone,
            ..item(STONE, "Major Mana Stone", 47)
        };
        let wand = donor_with(DONOR, "Sturdy Wand", 1500);
        let mut world = World {
            owned: vec![spare.clone()],
            ..World::with_corpses(vec![chain_corpse(CORPSE, 3.0)])
        };
        if inscribed {
            world
                .item_properties
                .insert(DONOR, strings(&[(7, "For Horan")]));
        }
        let (names, kinds) = stone_helper(ConsumableCategory::ManaStone);
        let mut controller =
            LootController::with_consumables(mana_settings(leave_junk()), names, kinds);

        assert!(controller.tick(&mut world, 1.0, true));
        world.requested = CORPSE;
        world.current = CORPSE;
        world.contents = vec![wand.clone()];
        controller.tick_identification(&mut world, 0.5);
        assert_eq!(world.identified, [DONOR]);

        world.set_appraisal(1, 0, DONOR);
        controller.tick_identification(&mut world, 0.5);
        assert!(controller.tick(&mut world, 0.1, true));
        assert_eq!(world.picked, [DONOR]);

        world.contents.clear();
        world.owned = vec![spare, wand];
        world.picked_up(1, DONOR);
        controller.tick_identification(&mut world, 0.5);
        assert!(controller.tick(&mut world, 0.1, true));
        if inscribed {
            assert!(!controller.classified_owned_items().contains_key(&DONOR));
        } else {
            assert_eq!(
                controller.classified_owned_items().get(&DONOR),
                Some(&LootAction::ManaTank)
            );
        }

        // The corpse is done; the drain is what is left to do.
        for _ in 0..4 {
            if !world.applied.is_empty() {
                break;
            }
            controller.tick(&mut world, 0.2, true);
        }

        let expected: Vec<(u32, u32)> = if inscribed {
            Vec::new()
        } else {
            vec![(STONE, DONOR)]
        };
        assert_eq!(world.applied, expected, "inscribed {inscribed}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ImplicitMana.cs :: ADonorWhoseDescriptionHasNotCaughtUpIsStillATank
#[test]
fn a_donor_whose_description_has_not_caught_up_is_still_a_tank() {
    const CORPSE: u32 = 0x7000_9301;
    const STONE: u32 = 0x7000_9310;
    const DONOR: u32 = 0x7000_9311;
    let spare = InventoryItem {
        object_class: ObjectClass::ManaStone,
        ..item(STONE, "Major Mana Stone", 47)
    };
    let wand = donor_with(DONOR, "Sturdy Wand", 1500);
    let mut world = World {
        owned: vec![spare.clone()],
        ..World::with_corpses(vec![chain_corpse(CORPSE, 3.0)])
    };
    let (names, kinds) = stone_helper(ConsumableCategory::ManaStone);
    let mut controller =
        LootController::with_consumables(mana_settings(leave_junk()), names, kinds);

    assert!(controller.tick(&mut world, 1.0, true));
    world.requested = CORPSE;
    world.current = CORPSE;
    world.contents = vec![wand.clone()];
    controller.tick_identification(&mut world, 0.5);
    assert_eq!(world.identified, [DONOR]);

    world.set_appraisal(1, 0, DONOR);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.picked, [DONOR]);

    // It is in the pack, and the client cannot describe it yet.
    world.unreadable_properties.insert(DONOR);
    world.contents.clear();
    world.owned = vec![spare, wand];
    world.picked_up(1, DONOR);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(
        controller.classified_owned_items().get(&DONOR),
        Some(&LootAction::ManaTank)
    );

    // The description catches up and nobody wrote on it: it is drained.
    world.unreadable_properties.remove(&DONOR);
    for _ in 0..4 {
        if !world.applied.is_empty() {
            break;
        }
        controller.tick(&mut world, 0.2, true);
    }

    assert_eq!(world.applied, [(STONE, DONOR)]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ImplicitMana.cs :: NothingTheCharacterAlreadyCarriedIsEverEmptiedIntoAStone
#[test]
fn nothing_the_character_already_carried_is_ever_emptied_into_a_stone() {
    const STONE: u32 = 0x7000_9310;
    const CORPSE: u32 = 0x7000_9301;
    let spare = InventoryItem {
        object_class: ObjectClass::ManaStone,
        ..item(STONE, "Major Mana Stone", 47)
    };
    let carried = [
        donor_with(0x7000_9311, "Sturdy Wand", 1500),
        InventoryItem {
            workmanship: 8.0,
            ..donor_with(0x7000_9312, "Metal Round Shield", 2000)
        },
        InventoryItem {
            workmanship: 7.0,
            ..donor_with(0x7000_9313, "Chainmail Girth", 1200)
        },
    ];
    let mut owned = vec![spare];
    owned.extend(carried);
    let mut world = World {
        owned,
        ..World::with_corpses(vec![chain_corpse(CORPSE, 3.0)])
    };
    let (names, kinds) = stone_helper(ConsumableCategory::ManaStone);
    let mut controller =
        LootController::with_consumables(mana_settings(leave_junk()), names, kinds);

    assert!(controller.tick(&mut world, 1.0, true));
    world.requested = CORPSE;
    world.current = CORPSE;
    world.contents = vec![item(0x7000_9314, "Rusty Junk", 51)];
    controller.tick_identification(&mut world, 0.5);
    for _ in 0..8 {
        controller.tick(&mut world, 0.2, true);
    }

    assert!(world.applied.is_empty());
    assert!(controller.classified_owned_items().is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.ImplicitMana.cs :: AnItemAKeepRuleClaimedIsNeverEmptiedIntoAStone
#[test]
fn an_item_a_keep_rule_claimed_is_never_emptied_into_a_stone() {
    const CORPSE: u32 = 0x7000_9401;
    const STONE: u32 = 0x7000_9410;
    const KEPT: u32 = 0x7000_9411;
    let spare = InventoryItem {
        object_class: ObjectClass::ManaStone,
        ..item(STONE, "Major Mana Stone", 47)
    };
    let wand = donor_with(KEPT, "Sturdy Wand", 1500);
    let mut world = World {
        owned: vec![spare.clone()],
        ..World::with_corpses(vec![chain_corpse(CORPSE, 3.0)])
    };
    let wands = vec![vtank_rule(
        "Wands",
        LootAction::Keep,
        vec![requirement(1, &["Sturdy Wand", "1"])],
    )];
    let (names, kinds) = stone_helper(ConsumableCategory::ManaStone);
    let mut controller = LootController::with_consumables(mana_settings(wands), names, kinds);

    assert!(controller.tick(&mut world, 1.0, true));
    world.requested = CORPSE;
    world.current = CORPSE;
    world.contents = vec![wand.clone()];
    controller.tick_identification(&mut world, 0.5);
    world.set_appraisal(1, 0, KEPT);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.picked, [KEPT]);

    world.contents.clear();
    world.owned = vec![spare, wand];
    world.picked_up(1, KEPT);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(
        controller.classified_owned_items().get(&KEPT),
        Some(&LootAction::Keep)
    );

    for _ in 0..6 {
        controller.tick(&mut world, 0.2, true);
    }

    assert!(world.applied.is_empty());
}

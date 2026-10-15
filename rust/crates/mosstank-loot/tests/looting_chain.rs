//! The corpse chain as the reference schedules it, pulls the client
//! refuses, and stack pickups (`LootingTests.MacroChain.cs`,
//! `LootingTests.PickupRefused.cs`, `LootingTests.StackPickup.cs`).

mod common;

use ac_plugin_abi::items::{
    InventoryCommandKind, InventoryCompletion, InventoryItem, ItemResult, ItemStatus,
    ItemUseCompletion,
};
use common::{
    Lines, PLAYER, World, bind_locks, capture_log, chain_corpse, contains_line, enabled_settings,
    item, lines_with, rule,
};
use mosstank_automation::locks::{ActionLockKind, SharedActionLocks};
use mosstank_formats::utl::LootAction;
use mosstank_loot::LootController;
use mosstank_settings::model::LootSettings;

fn chain_settings(expression: &str) -> LootSettings {
    let mut keep = rule("Rule", expression, LootAction::Keep);
    keep.priority = 1;
    enabled_settings(vec![keep])
}

/// A corpse open in front of the character with its items in it, ready to
/// pull (`PullStage`).
struct PullStage {
    world: World,
    controller: LootController,
    locks: SharedActionLocks,
    log: Lines,
}

fn stage_open_corpse(
    corpse: u32,
    items: Vec<InventoryItem>,
    settings: LootSettings,
    mut world: World,
) -> PullStage {
    let mut controller = LootController::new(settings);
    let log = capture_log(&mut controller);
    let locks = bind_locks(&mut controller);

    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.opened, [corpse]);
    world.requested = corpse;
    world.current = corpse;
    assert!(controller.observe_corpse_opened(&mut world));
    let frames = items.len().max(1);
    world.contents = items;
    // The C#'s one frame weighs every item; here one item a frame does
    // (register LOOT-7), so the stage gives each item its frame.
    controller.tick_identification(&mut world, 0.5);
    for _ in 1..frames {
        controller.tick_identification(&mut world, 0.0);
    }
    PullStage {
        world,
        controller,
        locks,
        log,
    }
}

fn stage_one(corpse: u32, single: InventoryItem, settings: LootSettings) -> PullStage {
    stage_open_corpse(
        corpse,
        vec![single],
        settings,
        World::with_corpses(vec![chain_corpse(corpse, 3.0)]),
    )
}

/// An ordinary item that takes a slot of its own.
fn loose(id: u32, name: &str, wcid: u32, slot: i32) -> InventoryItem {
    InventoryItem {
        container_slot: slot,
        ..item(id, name, wcid)
    }
}

impl PullStage {
    fn tick(&mut self, elapsed: f64) -> bool {
        self.controller.tick(&mut self.world, elapsed, true)
    }

    fn advance(&self, seconds: f64) {
        self.locks.borrow_mut().advance(seconds);
    }
}

fn refused(notice: Option<&str>) -> ItemResult {
    match notice {
        Some(notice) => ItemResult::because(ItemStatus::Refused, notice),
        None => ItemResult::of(ItemStatus::Refused),
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.MacroChain.cs :: TheRuleHoldsThePassWhileTheItemSlotIsHeld
#[test]
fn the_rule_holds_the_pass_while_the_item_slot_is_held() {
    const CORPSE: u32 = 0x7000_1101;
    const COIN: u32 = 0x7000_1102;
    let mut world = World::with_corpses(vec![chain_corpse(CORPSE, 3.0)]);
    let mut controller = LootController::new(chain_settings("*"));
    let locks = bind_locks(&mut controller);

    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.opened, [CORPSE]);
    world.requested = CORPSE;
    world.current = CORPSE;
    assert!(controller.observe_corpse_opened(&mut world));
    world.contents = vec![item(COIN, "Colosseum coin", 77)];
    controller.tick_identification(&mut world, 0.5);

    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.picked, [COIN]);
    assert!(locks.borrow_mut().is_locked(ActionLockKind::ItemUse));

    // The slot is up: the turn is held and nothing is pulled.
    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.picked, [COIN]);

    // The slot's window runs out; the coin is still in the corpse.
    locks.borrow_mut().advance(0.8);
    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.picked, [COIN, COIN]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.MacroChain.cs :: TheRulesTurnNeverAsksForACorpseDescription
#[test]
fn the_rules_turn_never_asks_for_a_corpse_description() {
    const CORPSE: u32 = 0x7000_1111;
    let mut world = World::with_corpses(vec![common::undescribed(chain_corpse(CORPSE, 3.0))]);
    let mut controller = LootController::new(chain_settings("*"));

    assert!(!controller.tick(&mut world, 0.3, true));
    assert!(world.identified.is_empty());
    assert!(world.opened.is_empty());

    controller.tick_identification(&mut world, 0.1);
    assert_eq!(world.identified, [CORPSE]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.MacroChain.cs :: AnItemIsDescribedAndDecidedOnTheFrameWithoutARuleTurn
#[test]
fn an_item_is_described_and_decided_on_the_frame_without_a_rule_turn() {
    const CORPSE: u32 = 0x7000_1121;
    const COIN: u32 = 0x7000_1122;
    let mut world = World::with_corpses(vec![chain_corpse(CORPSE, 3.0)]);
    let mut controller = LootController::new(chain_settings("name ~= coin"));
    let logged = capture_log(&mut controller);

    assert!(controller.tick(&mut world, 0.3, true));
    world.requested = CORPSE;
    world.current = CORPSE;
    world.contents = vec![item(COIN, "Colosseum coin", 77)];

    controller.tick_identification(&mut world, 0.1);
    assert_eq!(world.identified, [COIN]);
    assert!(
        !logged
            .borrow()
            .iter()
            .any(|line| line.starts_with("LootDecision: "))
    );

    world.complete_appraisal(COIN, false);
    controller.tick_identification(&mut world, 0.1);
    assert!(
        logged
            .borrow()
            .iter()
            .any(|line| line.starts_with("LootDecision: Colosseum coin -> Keep"))
    );
    assert!(world.picked.is_empty());

    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.picked, [COIN]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.MacroChain.cs :: APullIsCountedWhenItIsIssuedAndDroppedAtTheAttemptCeiling
#[test]
fn a_pull_is_counted_when_it_is_issued_and_dropped_at_the_attempt_ceiling() {
    const CORPSE: u32 = 0x7000_1131;
    const COIN: u32 = 0x7000_1132;
    let settings = LootSettings {
        corpse_loot_item_max_attempts: 2,
        ..chain_settings("*")
    };
    let mut world = World::with_corpses(vec![chain_corpse(CORPSE, 3.0)]);
    let mut controller = LootController::new(settings);

    assert!(controller.tick(&mut world, 0.3, true));
    world.requested = CORPSE;
    world.current = CORPSE;
    world.contents = vec![item(COIN, "Colosseum coin", 77)];
    controller.tick_identification(&mut world, 0.5);

    assert!(controller.tick(&mut world, 0.3, true));
    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.picked, [COIN, COIN]);

    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.picked, [COIN, COIN]);
    assert_eq!(world.closed, [CORPSE]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.MacroChain.cs :: AKillCreditedWithoutThePlusIsStillOurs
#[test]
fn a_kill_credited_without_the_plus_is_still_ours() {
    const CORPSE: u32 = 0x7000_1161;
    let mut world = World {
        name: "+Tester".to_owned(),
        ..World::with_corpses(vec![chain_corpse(CORPSE, 3.0)])
    };
    let mut controller = LootController::new(chain_settings("*"));

    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.opened, [CORPSE]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.MacroChain.cs :: ARefusedOpenPacesTheNextAttemptOnTheOpenSlot
#[test]
fn a_refused_open_paces_the_next_attempt_on_the_open_slot() {
    const CORPSE: u32 = 0x7000_1172;
    let settings = LootSettings {
        corpse_open_timeout_seconds: 1.5,
        ..chain_settings("*")
    };
    let mut world = World {
        open_result: ItemStatus::Refused,
        ..World::with_corpses(vec![chain_corpse(CORPSE, 3.0)])
    };
    let mut controller = LootController::new(settings);
    let locks = bind_locks(&mut controller);

    controller.tick(&mut world, 0.3, true);
    assert_eq!(world.opened, [CORPSE]);
    assert!(
        locks
            .borrow_mut()
            .is_locked(ActionLockKind::CorpseOpenAttempt)
    );

    // Inside the slot the rule holds the pass and attempts nothing.
    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.opened, [CORPSE]);

    locks.borrow_mut().advance(1.6);
    controller.tick(&mut world, 0.3, true);
    assert_eq!(world.opened, [CORPSE, CORPSE]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.MacroChain.cs :: AnOpenHeldUpByAnotherRuleIsNotCountedAgainstTheCorpse
#[test]
fn an_open_held_up_by_another_rule_is_not_counted_against_the_corpse() {
    const CORPSE: u32 = 0x7000_1173;
    let settings = LootSettings {
        corpse_open_timeout_seconds: 1.5,
        ..chain_settings("*")
    };
    let mut world = World::with_corpses(vec![chain_corpse(CORPSE, 3.0)]);
    let mut controller = LootController::new(settings);
    let locks = bind_locks(&mut controller);

    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.opened, [CORPSE]);
    world.requested = CORPSE;

    // Another rule uses an item of its own; its window outlives the open's.
    locks.borrow_mut().arm(ActionLockKind::ItemUse, 10.0);
    locks.borrow_mut().advance(2.0);
    assert!(
        !locks
            .borrow_mut()
            .is_locked(ActionLockKind::CorpseOpenAttempt)
    );
    assert!(locks.borrow_mut().is_locked(ActionLockKind::ItemUse));

    assert!(controller.tick(&mut world, 2.0, true));
    assert_eq!(world.opened, [CORPSE]);

    // The other rule is done; the open is looked at again, still inside its
    // own timeout.
    locks.borrow_mut().advance(9.0);
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.opened, [CORPSE]);
    assert!(!controller.status.contains("Retrying corpse"));
    assert!(!controller.status.contains("Blacklisted"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.MacroChain.cs :: AnOpenTheHostIsPacingCostsTheCorpseNothing
#[test]
fn an_open_the_host_is_pacing_costs_the_corpse_nothing() {
    const FIRST: u32 = 0x7000_1181;
    const SECOND: u32 = 0x7000_1182;
    let settings = LootSettings {
        corpse_item_appearance_timeout_seconds: 0.0,
        corpse_open_timeout_seconds: 1.5,
        ..chain_settings("*")
    };
    let mut world = World::with_corpses(vec![chain_corpse(FIRST, 3.0)]);
    let mut controller = LootController::new(settings);
    let locks = bind_locks(&mut controller);

    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.opened, [FIRST]);
    world.requested = FIRST;
    world.current = FIRST;
    assert!(controller.observe_corpse_opened(&mut world));
    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.closed, [FIRST]);

    // The open of the next corpse lands inside the host's pacing window.
    world.requested = 0;
    world.current = 0;
    world.corpses = vec![chain_corpse(SECOND, 3.0)];
    world.open_result = ItemStatus::Busy;
    locks.borrow_mut().advance(0.1);
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.opened, [FIRST, SECOND]);
    assert!(!controller.status.contains("Retrying corpse"));
    assert!(!locks.borrow_mut().is_locked(ActionLockKind::ItemUse));
    assert!(
        !locks
            .borrow_mut()
            .is_locked(ActionLockKind::CorpseOpenAttempt)
    );

    // The window has passed: the same open goes out on the next turn.
    world.open_result = ItemStatus::Started;
    locks.borrow_mut().advance(0.1);
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.opened, [FIRST, SECOND, SECOND]);
    assert!(controller.status.contains("Opening"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.MacroChain.cs :: ARefusedOpenCountsTowardsTheCorpseBlacklist
#[test]
fn a_refused_open_counts_towards_the_corpse_blacklist() {
    const CORPSE: u32 = 0x7000_1171;
    let settings = LootSettings {
        blacklist_corpse_open_attempt_count: 2,
        ..chain_settings("*")
    };
    let mut world = World {
        open_result: ItemStatus::Refused,
        ..World::with_corpses(vec![chain_corpse(CORPSE, 3.0)])
    };
    let mut controller = LootController::new(settings);

    assert!(!controller.tick(&mut world, 0.3, true));
    assert!(!controller.tick(&mut world, 0.3, true));
    assert_eq!(world.opened, [CORPSE, CORPSE]);
    assert!(controller.status.contains("Blacklisted"));

    assert!(!controller.tick(&mut world, 0.3, true));
    assert_eq!(world.opened, [CORPSE, CORPSE]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.MacroChain.cs :: ASecondCorpseIsOpenedOnTheVeryNextTurnWithoutAScanWait
#[test]
fn a_second_corpse_is_opened_on_the_very_next_turn_without_a_scan_wait() {
    const FIRST: u32 = 0x7000_1141;
    const SECOND: u32 = 0x7000_1142;
    let settings = LootSettings {
        scan_interval_seconds: 5.0,
        corpse_item_appearance_timeout_seconds: 0.0,
        ..chain_settings("*")
    };
    let mut world = World::with_corpses(vec![chain_corpse(FIRST, 3.0)]);
    let mut controller = LootController::new(settings);

    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.opened, [FIRST]);
    world.requested = FIRST;
    world.current = FIRST;
    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.closed, [FIRST]);

    world.requested = 0;
    world.current = 0;
    world.corpses = vec![chain_corpse(SECOND, 3.0)];
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.opened, [FIRST, SECOND]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.MacroChain.cs :: ItemIdRequestsFollowTheReferenceCadence
#[test]
fn item_id_requests_follow_the_reference_cadence() {
    const CORPSE: u32 = 0x7000_1151;
    const FIRST: u32 = 0x7000_1152;
    const SECOND: u32 = 0x7000_1153;
    let mut world = World::with_corpses(vec![chain_corpse(CORPSE, 3.0)]);
    let mut controller = LootController::new(chain_settings("name ~= coin"));

    assert!(controller.tick(&mut world, 0.3, true));
    world.requested = CORPSE;
    world.current = CORPSE;
    world.contents = vec![
        item(FIRST, "Colosseum coin", 77),
        item(SECOND, "Colosseum coin", 77),
    ];

    controller.tick_identification(&mut world, 0.0);
    assert_eq!(world.identified, [FIRST]);

    world.complete_appraisal(FIRST, false);
    controller.tick_identification(&mut world, 0.1);
    assert_eq!(world.identified, [FIRST]);

    controller.tick_identification(&mut world, 0.4);
    assert_eq!(world.identified, [FIRST, SECOND]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.PickupRefused.cs :: ARefusedPullHoldsTheItemSlotLikeAnAcceptedOne
#[test]
fn a_refused_pull_holds_the_item_slot_like_an_accepted_one() {
    const CORPSE: u32 = 0x7000_1201;
    const COIN: u32 = 0x7000_1202;
    let mut stage = stage_one(
        CORPSE,
        item(COIN, "Colosseum coin", 77),
        chain_settings("*"),
    );
    stage
        .world
        .pickup_results
        .push_back(refused(Some("no pack the player has open has room for it")));

    assert!(stage.tick(0.3));

    assert_eq!(stage.world.picked, [COIN]);
    assert!(stage.locks.borrow_mut().is_locked(ActionLockKind::ItemUse));
    assert!(
        stage
            .locks
            .borrow_mut()
            .is_locked(ActionLockKind::Navigation)
    );

    // The very next turn takes nothing.
    assert!(stage.tick(0.1));
    assert_eq!(stage.world.picked, [COIN]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.PickupRefused.cs :: APullTheClientRefusesEndsTheCorpseWithOneLine
#[test]
fn a_pull_the_client_refuses_ends_the_corpse_with_one_line() {
    const CORPSE: u32 = 0x7000_1211;
    const COIN: u32 = 0x7000_1212;
    let settings = LootSettings {
        blacklist_corpse_open_timeout_seconds: 1.0,
        ..chain_settings("*")
    };
    let mut stage = stage_one(CORPSE, item(COIN, "Colosseum coin", 77), settings);
    stage
        .world
        .pickup_results
        .push_back(refused(Some("no pack the player has open has room for it")));

    assert!(stage.tick(0.3));
    assert_eq!(stage.world.picked, [COIN]);
    let given = lines_with(&stage.log, "giving up on");
    assert_eq!(given.len(), 1);
    assert!(given[0].contains("no pack"));

    // The corpse is closed rather than worked again.
    stage.advance(1.0);
    assert!(stage.tick(0.3));
    assert_eq!(stage.world.closed, [CORPSE]);
    assert_eq!(stage.world.picked, [COIN]);
    assert_eq!(lines_with(&stage.log, "giving up on").len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.PickupRefused.cs :: ACorpseGivenUpOnIsNotRecordedAsLooted
#[test]
fn a_corpse_given_up_on_is_not_recorded_as_looted() {
    const CORPSE: u32 = 0x7000_1221;
    const COIN: u32 = 0x7000_1222;
    let settings = LootSettings {
        blacklist_corpse_open_timeout_seconds: 1.0,
        ..chain_settings("*")
    };
    let mut stage = stage_one(CORPSE, item(COIN, "Colosseum coin", 77), settings);
    stage
        .world
        .pickup_results
        .push_back(refused(Some("no pack the player has open has room for it")));

    assert!(stage.tick(0.3));
    stage.advance(1.0);
    assert!(stage.tick(0.3));
    assert_eq!(stage.world.closed, [CORPSE]);

    // The container is shut and the skip period runs out.
    stage.world.requested = 0;
    stage.world.current = 0;
    stage.world.contents.clear();
    stage.advance(1.0);
    assert!(stage.tick(2.0));

    assert_eq!(stage.world.opened, [CORPSE, CORPSE]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.PickupRefused.cs :: ABusyPullCostsTheItemNoneOfItsAttempts
#[test]
fn a_busy_pull_costs_the_item_none_of_its_attempts() {
    const CORPSE: u32 = 0x7000_1231;
    const COIN: u32 = 0x7000_1232;
    let settings = LootSettings {
        corpse_loot_item_max_attempts: 2,
        ..chain_settings("*")
    };
    let mut stage = stage_one(CORPSE, item(COIN, "Colosseum coin", 77), settings);
    for _ in 0..3 {
        stage
            .world
            .pickup_results
            .push_back(ItemResult::of(ItemStatus::Busy));
    }

    for _ in 0..3 {
        assert!(stage.tick(0.3));
        stage.advance(1.0);
    }
    assert_eq!(stage.world.picked.len(), 3);

    // The fourth answer is "sent": the coin was still in the pass.
    assert!(stage.tick(0.3));
    assert_eq!(stage.world.picked.len(), 4);
    assert!(lines_with(&stage.log, "abandoned").is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.PickupRefused.cs :: AnItemThatSpentItsAttemptsIsReportedOnce
#[test]
fn an_item_that_spent_its_attempts_is_reported_once() {
    const CORPSE: u32 = 0x7000_1241;
    const COIN: u32 = 0x7000_1242;
    let settings = LootSettings {
        corpse_loot_item_max_attempts: 1,
        ..chain_settings("*")
    };
    let mut stage = stage_one(CORPSE, item(COIN, "Colosseum coin", 77), settings);

    // The pull goes out and the coin stays where it is.
    assert!(stage.tick(0.3));
    assert_eq!(stage.world.picked, [COIN]);
    stage.advance(1.0);

    assert!(stage.tick(0.3));
    assert_eq!(stage.world.picked, [COIN]);

    let dropped = lines_with(&stage.log, "abandoned");
    assert_eq!(dropped.len(), 1);
    assert!(dropped[0].contains("Colosseum coin"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.PickupRefused.cs :: AnItemLeftBehindWhenTheCorpseShutsIsReported
#[test]
fn an_item_left_behind_when_the_corpse_shuts_is_reported() {
    const CORPSE: u32 = 0x7000_1251;
    const COIN: u32 = 0x7000_1252;
    let mut stage = stage_one(
        CORPSE,
        item(COIN, "Colosseum coin", 77),
        chain_settings("*"),
    );

    assert!(stage.tick(0.3));
    assert_eq!(stage.world.picked, [COIN]);

    // The container shuts with the coin still in it.
    stage.world.requested = 0;
    stage.world.current = 0;
    stage.advance(1.0);
    stage.tick(0.3);

    let left = lines_with(&stage.log, "abandoned");
    assert_eq!(left.len(), 1);
    assert!(left[0].contains("Colosseum coin"));
    assert!(left[0].contains("corpse closed"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.PickupRefused.cs :: ARefusedItemIsRetiredAndTheCorpsesOtherItemsAreStillTaken
#[test]
fn a_refused_item_is_retired_and_the_corpses_other_items_are_still_taken() {
    const CORPSE: u32 = 0x7000_1261;
    const ANVIL: u32 = 0x7000_1262;
    const RING: u32 = 0x7000_1263;
    let mut stage = stage_open_corpse(
        CORPSE,
        vec![
            loose(ANVIL, "Anvil", 77, 0),
            loose(RING, "Gold Ring", 78, 1),
        ],
        chain_settings("*"),
        World::with_corpses(vec![chain_corpse(CORPSE, 3.0)]),
    );
    stage
        .world
        .pickup_results
        .push_back(refused(Some("it is too heavy for you to carry")));

    assert!(stage.tick(0.3));
    assert_eq!(stage.world.picked, [ANVIL]);
    let refusals = lines_with(&stage.log, "LootPickup: refused");
    assert_eq!(refusals.len(), 1);
    assert!(refusals[0].contains("Anvil"));
    assert!(refusals[0].contains("too heavy"));
    assert!(lines_with(&stage.log, "giving up on").is_empty());

    stage.advance(1.0);
    assert!(stage.tick(0.3));
    assert_eq!(stage.world.picked, [ANVIL, RING]);
    stage.advance(1.0);
    assert!(stage.tick(0.3));
    assert_eq!(stage.world.picked, [ANVIL, RING, RING]);
    assert_eq!(lines_with(&stage.log, "LootPickup: refused").len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.PickupRefused.cs :: ACorpseIsGivenUpOnOnlyWhenEveryItemHasBeenRefused
#[test]
fn a_corpse_is_given_up_on_only_when_every_item_has_been_refused() {
    const CORPSE: u32 = 0x7000_1271;
    const ANVIL: u32 = 0x7000_1272;
    const RING: u32 = 0x7000_1273;
    let settings = LootSettings {
        blacklist_corpse_open_timeout_seconds: 1.0,
        ..chain_settings("*")
    };
    let mut stage = stage_open_corpse(
        CORPSE,
        vec![
            loose(ANVIL, "Anvil", 77, 0),
            loose(RING, "Gold Ring", 78, 1),
        ],
        settings,
        World::with_corpses(vec![chain_corpse(CORPSE, 3.0)]),
    );
    for _ in 0..2 {
        stage
            .world
            .pickup_results
            .push_back(refused(Some("it is too heavy for you to carry")));
    }

    // The first refusal does not end the corpse; the second does.
    assert!(stage.tick(0.3));
    assert!(lines_with(&stage.log, "giving up on").is_empty());
    stage.advance(1.0);
    assert!(stage.tick(0.3));
    assert_eq!(stage.world.picked, [ANVIL, RING]);
    stage.advance(1.0);
    assert!(stage.tick(0.3));
    assert_eq!(lines_with(&stage.log, "giving up on").len(), 1);
    assert_eq!(stage.world.closed, [CORPSE]);
    assert_eq!(stage.world.picked, [ANVIL, RING]);

    // The corpse was set aside, not looted, so it is opened again.
    stage.world.requested = 0;
    stage.world.current = 0;
    stage.world.contents.clear();
    stage.advance(1.0);
    assert!(stage.tick(2.0));
    assert_eq!(stage.world.opened, [CORPSE, CORPSE]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.PickupRefused.cs :: AFullPackStillTakesAStackThatMergesIntoOneAlreadyHeld
#[test]
fn a_full_pack_still_takes_a_stack_that_merges_into_one_already_held() {
    const CORPSE: u32 = 0x7000_1281;
    const ANVIL: u32 = 0x7000_1282;
    const PYREAL: u32 = 0x7000_1283;
    const PURSE: u32 = 0x7000_1284;
    let world = World {
        owned: vec![InventoryItem {
            container_id: PLAYER,
            stack_size: 5,
            maximum_stack_size: 25,
            ..item(PURSE, "Pyreal", 60)
        }],
        ..World::with_corpses(vec![chain_corpse(CORPSE, 3.0)])
    };
    let mut stage = stage_open_corpse(
        CORPSE,
        vec![
            loose(ANVIL, "Anvil", 77, 0),
            InventoryItem {
                container_slot: 1,
                stack_size: 3,
                maximum_stack_size: 25,
                ..item(PYREAL, "Pyreal", 60)
            },
        ],
        chain_settings("*"),
        world,
    );
    stage
        .world
        .pickup_results
        .push_back(refused(Some("no pack the player has open has room for it")));

    assert!(stage.tick(0.3));
    assert_eq!(stage.world.picked, [ANVIL]);
    assert!(lines_with(&stage.log, "giving up on").is_empty());

    stage.advance(1.0);
    assert!(stage.tick(0.3));
    assert_eq!(stage.world.picked, [ANVIL, PYREAL]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.PickupRefused.cs :: AFullPackEndsTheCorpseWithoutAskingForWhatCannotFit
#[test]
fn a_full_pack_ends_the_corpse_without_asking_for_what_cannot_fit() {
    const CORPSE: u32 = 0x7000_1291;
    const ANVIL: u32 = 0x7000_1292;
    const RING: u32 = 0x7000_1293;
    const HELD: u32 = 0x7000_1294;
    let world = World {
        item_capacity: 1,
        owned: vec![InventoryItem {
            container_id: PLAYER,
            ..item(HELD, "Shield", 61)
        }],
        ..World::with_corpses(vec![chain_corpse(CORPSE, 3.0)])
    };
    let mut stage = stage_open_corpse(
        CORPSE,
        vec![
            loose(ANVIL, "Anvil", 77, 0),
            loose(RING, "Gold Ring", 78, 1),
        ],
        chain_settings("*"),
        world,
    );
    stage.world.pickup_results.push_back(refused(None));

    assert!(stage.tick(0.3));
    assert_eq!(stage.world.picked, [ANVIL]);
    assert_eq!(lines_with(&stage.log, "giving up on").len(), 1);

    // The corpse is closed, and the ring was never asked for.
    stage.advance(1.0);
    assert!(stage.tick(0.3));
    assert_eq!(stage.world.closed, [CORPSE]);
    assert_eq!(stage.world.picked, [ANVIL]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.StackPickup.cs :: ExhaustedStackPickupAdvancesToRemainingLoot
#[test]
fn exhausted_stack_pickup_advances_to_remaining_loot() {
    const CORPSE: u32 = 0x7000_1600;
    const PEA: u32 = 0x7000_1601;
    const NEXT: u32 = 0x7000_1602;
    let rows = [
        (InventoryCommandKind::Merge, 1u32),
        (InventoryCommandKind::Merge, 0),
        (InventoryCommandKind::Pickup, 1),
        (InventoryCommandKind::Unknown, 0),
    ];
    for (completion_kind, error) in rows {
        let settings = LootSettings {
            corpse_loot_item_max_attempts: 2,
            ..chain_settings("*")
        };
        let first = InventoryItem {
            maximum_stack_size: 100,
            stack_size: 1,
            ..loose(PEA, "Gold Pea", 77, 0)
        };
        let second = loose(NEXT, "Next item", 78, 1);
        let mut stage = stage_open_corpse(
            CORPSE,
            vec![first.clone(), second.clone()],
            settings,
            World::with_corpses(vec![chain_corpse(CORPSE, 3.0)]),
        );
        for attempt in 0..2i64 {
            assert!(stage.tick(0.3));
            assert_eq!(stage.world.picked.last(), Some(&PEA));
            stage.advance(1.0);
            stage.world.items_busy = true;
            assert!(stage.tick(0.3));
            assert_eq!(
                stage.world.picked.len(),
                usize::try_from(attempt + 1).unwrap()
            );
            stage.world.items_busy = false;
            stage.world.inventory_completion = if completion_kind == InventoryCommandKind::Unknown {
                InventoryCompletion::default()
            } else {
                InventoryCompletion {
                    revision: attempt + 1,
                    kind: completion_kind,
                    source_id: PEA,
                    weenie_error: error,
                }
            };
        }

        assert!(stage.tick(0.3));
        assert_eq!(
            stage.world.picked,
            [PEA, PEA, NEXT],
            "{completion_kind:?} {error}"
        );
        stage.world.contents = vec![first];
        stage.world.owned = vec![second];
        stage.advance(1.0);
        assert!(stage.tick(0.3));
        assert!(!stage.controller.classified_owned_items().contains_key(&PEA));
        assert!(
            stage
                .controller
                .classified_owned_items()
                .contains_key(&NEXT)
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.StackPickup.cs :: UnrelatedOrOldMergeFailureDoesNotInterruptPendingPickup
#[test]
fn unrelated_or_old_merge_failure_does_not_interrupt_pending_pickup() {
    const CORPSE: u32 = 0x7000_1610;
    const PEA: u32 = 0x7000_1611;
    for old in [false, true] {
        let mut stage = stage_one(CORPSE, item(PEA, "Gold Pea", 77), chain_settings("*"));
        stage.world.inventory_completion = InventoryCompletion {
            revision: 1,
            kind: InventoryCommandKind::Merge,
            source_id: PEA,
            weenie_error: 1,
        };
        assert!(stage.tick(0.3));
        stage.world.items_busy = true;
        stage.advance(1.0);
        stage.world.inventory_completion = InventoryCompletion {
            revision: if old { 1 } else { 2 },
            kind: InventoryCommandKind::Merge,
            source_id: if old { PEA } else { PEA + 1 },
            weenie_error: 1,
        };
        assert!(stage.tick(0.3));
        assert_eq!(stage.world.picked, [PEA], "old {old}");
    }
}

#[test]
fn every_line_the_staged_looter_writes_is_a_loot_line() {
    let stage = stage_one(
        0x7000_1201,
        item(0x7000_1202, "Colosseum coin", 77),
        chain_settings("*"),
    );
    assert!(contains_line(&stage.log, "LootCorpse: opening"));
}

/// An open of a corpse out of arm's reach that the client walks to first
/// (`open_walks_first`), opened once and then waited past its window while
/// the client is still busy with the walk.
fn open_and_wait_out_the_window(corpse: u32) -> (World, LootController, SharedActionLocks, Lines) {
    let settings = LootSettings {
        corpse_open_timeout_seconds: 1.5,
        ..chain_settings("*")
    };
    let mut world = World {
        open_walks_first: true,
        ..World::with_corpses(vec![chain_corpse(corpse, 3.0)])
    };
    let mut controller = LootController::new(settings);
    let log = capture_log(&mut controller);
    let locks = bind_locks(&mut controller);

    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.opened, [corpse]);

    // Past the open window the client is still walking: the open is early,
    // not failed, and the pass is held.
    locks.borrow_mut().advance(1.6);
    assert!(controller.tick(&mut world, 1.6, true));
    assert_eq!(controller.status, "Waiting to open corpse…");
    assert_eq!(world.opened, [corpse]);
    (world, controller, locks, log)
}

// Source: MossTank register LOOT-8 (no C# counterpart: the C# opens such a
// corpse again for as long as it lasts). Seen live on a corpse behind a shop
// wall in Holtburg (openac-rust
// `docs/evidence/r10/2026-10-15-mosstank-loot-slow-ticks.md`).
#[test]
fn an_open_whose_walk_covered_no_ground_sets_the_corpse_aside() {
    const UNREACHABLE: u32 = 0x7000_1301;
    const NEXT: u32 = 0x7000_1302;
    let (mut world, mut controller, locks, log) = open_and_wait_out_the_window(UNREACHABLE);

    // The client gave the walk up where it started: the open never went out.
    world.items_busy = false;
    assert!(!controller.tick(&mut world, 0.3, true));
    assert_eq!(world.opened, [UNREACHABLE]);
    assert!(contains_line(
        &log,
        "LootCorpse: setting aside 0x70001301 for 200 seconds: the walk to it covered no ground"
    ));

    // The pass goes on to the next corpse, and the one set aside stays aside.
    world.corpses.push(chain_corpse(NEXT, 3.0));
    locks.borrow_mut().advance(0.3);
    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.opened, [UNREACHABLE, NEXT]);
}

// Source: MossTank register LOOT-8.
#[test]
fn an_open_whose_walk_covered_ground_is_opened_again() {
    const CORPSE: u32 = 0x7000_1303;
    let (mut world, mut controller, _locks, log) = open_and_wait_out_the_window(CORPSE);

    // The walk got two metres along before it was called off (the corpse
    // walk taking over): the corpse is reachable and is opened again.
    world.navigation_position.north_south += 2.0 / 240.0;
    world.items_busy = false;
    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.opened, [CORPSE, CORPSE]);
    assert!(!contains_line(&log, "setting aside"));
}

// Source: MossTank register LOOT-8.
#[test]
fn an_open_the_server_answered_is_not_set_aside() {
    const CORPSE: u32 = 0x7000_1304;
    let (mut world, mut controller, _locks, log) = open_and_wait_out_the_window(CORPSE);

    // The walk arrived and the use went out; the server answered it before
    // the looter saw the container asked for.
    world.use_completion = ItemUseCompletion {
        revision: world.use_completion.revision + 1,
        source_id: CORPSE,
        target_id: 0,
        weenie_error: 0x1D,
    };
    controller.tick_identification(&mut world, 0.0);
    world.items_busy = false;
    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.opened, [CORPSE, CORPSE]);
    assert!(!contains_line(&log, "setting aside"));
}

// Source: MossTank register LOOT-8.
#[test]
fn an_open_whose_walk_the_corpse_walk_called_off_is_opened_again() {
    const CORPSE: u32 = 0x7000_1305;
    let (mut world, mut controller, _locks, log) = open_and_wait_out_the_window(CORPSE);

    // The corpse walk took the keys before the client's walk got anywhere
    // (it was still turning): called off, not given up.
    controller.note_corpse_walk_steered();
    world.items_busy = false;
    assert!(controller.tick(&mut world, 0.3, true));
    assert_eq!(world.opened, [CORPSE, CORPSE]);
    assert!(!contains_line(&log, "setting aside"));
}

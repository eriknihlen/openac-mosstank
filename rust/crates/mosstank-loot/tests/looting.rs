//! The looter end to end through its client (`LootingTests.cs`).

mod common;

use ac_plugin_abi::items::{
    InventoryItem, ItemResult, ItemStatus, ItemUseCompletion, LootAction as PublicLootAction,
};
use ac_plugin_abi::social::FellowMember;
use ac_plugin_abi::world::ObjectClass;
use common::{
    ClassifierRegistry, World, bind_locks, capture_log, chain_corpse, chat, classification,
    contains_line, enabled_settings, expression_rule, item, place, rule, scroll, skill, spell,
    undescribed,
};
use mosstank_automation::locks::ActionLockKind;
use mosstank_formats::utl::LootAction;
use mosstank_loot::LootController;
use mosstank_loot::corpse_scan::RARE_ANNOUNCEMENT_WINDOW_SECONDS;
use mosstank_loot::host::LootCorpse;
use mosstank_loot::scroll::{ReadScrollController, is_eligible};
use mosstank_settings::model::LootSettings;

fn coins_rule() -> mosstank_formats::utl::LootRule {
    let mut coins = rule("Coins", "name ~= coin", LootAction::Keep);
    coins.priority = 3;
    coins
}

fn scroll_world(corpse: u32) -> World {
    World {
        known_spell: Some(spell(777, "Incantation of Testing", 100, 34)),
        skills: vec![skill(34, "War Magic", 90)],
        corpses: vec![chain_corpse(corpse, 3.0)],
        ..World::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: CombineSalvageRunsRealTwoBagSalvageOperation
#[test]
fn combine_salvage_runs_real_two_bag_salvage_operation() {
    let settings = LootSettings {
        combine_salvage: true,
        ..enabled_settings(vec![expression_rule("*")])
    };
    const TOOL: u32 = 0x5000_0600;
    const FIRST: u32 = 0x5000_0601;
    const SECOND: u32 = 0x5000_0602;
    let mut world = World {
        owned: vec![
            InventoryItem {
                item_type: 0x2000_0000,
                ..item(TOOL, "Salvage Tool", 1)
            },
            InventoryItem {
                material_type: 12,
                workmanship: 7.1,
                ..item(FIRST, "Salvaged Iron (7)", 2)
            },
            InventoryItem {
                material_type: 12,
                workmanship: 8.5,
                ..item(SECOND, "Salvaged Iron (8)", 2)
            },
        ],
        ..World::default()
    };
    let mut controller = LootController::new(settings);

    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.salvaged, [(TOOL, FIRST), (TOOL, SECOND)]);
    world.owned.truncate(2);
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(controller.status, "Combined salvage bags.");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ControllerOpensClassifiesAndPicksThroughCanonicalLootSurface
#[test]
fn controller_opens_classifies_and_picks_through_canonical_loot_surface() {
    const CORPSE: u32 = 0x7000_0001;
    const COIN: u32 = 0x7000_0002;
    let mut world = World::with_corpses(vec![chain_corpse(CORPSE, 3.0)]);
    let mut controller = LootController::new(enabled_settings(vec![coins_rule()]));

    assert!(controller.tick(&mut world, 1.0, true));
    assert_eq!(world.opened, [CORPSE]);

    world.requested = CORPSE;
    world.current = CORPSE;
    world.contents = vec![item(COIN, "Colosseum coin", 77)];
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));
    assert_eq!(world.identified, [COIN]);

    world.set_appraisal(1, 0, COIN);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.picked, [COIN]);

    world.contents.clear();
    world.picked_up(1, COIN);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(
        controller.classified_owned_items().get(&COIN),
        Some(&LootAction::Keep)
    );

    // The pass the corpse finishes on is the pass that closes it.
    assert!(controller.tick(&mut world, 0.2, true));
    assert_eq!(controller.status, "Corpse complete.");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ALootingPassReportsTheCorpseEachDecisionAndEachPickup
#[test]
fn a_looting_pass_reports_the_corpse_each_decision_and_each_pickup() {
    const CORPSE: u32 = 0x7000_0021;
    const COIN: u32 = 0x7000_0022;
    let mut world = World::with_corpses(vec![LootCorpse {
        name: "Drudge Corpse".to_owned(),
        ..chain_corpse(CORPSE, 3.0)
    }]);
    let mut controller = LootController::new(enabled_settings(vec![coins_rule()]));
    // Every line the looter writes goes to the Loot channel.
    let logged = capture_log(&mut controller);

    assert!(controller.tick(&mut world, 1.0, true));
    world.requested = CORPSE;
    world.current = CORPSE;
    world.contents = vec![item(COIN, "Colosseum coin", 77)];
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));
    world.set_appraisal(1, 0, COIN);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));
    world.contents.clear();
    world.picked_up(1, COIN);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));

    assert!(contains_line(&logged, "opening Drudge Corpse"));
    assert!(contains_line(&logged, "Colosseum coin -> Keep (Coins)"));
    assert!(contains_line(&logged, "taking Colosseum coin"));
    assert!(contains_line(&logged, "took Colosseum coin"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ACorpseWithinReachIsDescribedOnTheFrameBeforeTheLootRuleGetsATurn
#[test]
fn a_corpse_within_reach_is_described_on_the_frame_before_the_loot_rule_gets_a_turn() {
    const CORPSE: u32 = 0x7000_0402;
    let settings = LootSettings {
        scan_interval_seconds: 0.05,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let unidentified = undescribed(chain_corpse(CORPSE, 3.0));
    let mut world = World::with_corpses(vec![unidentified.clone()]);
    let mut controller = LootController::new(settings);

    // The rule never had the pass; the frame asked anyway.
    controller.tick_identification(&mut world, 0.1);
    assert_eq!(world.identified, [CORPSE]);

    // Nothing is asked twice while the answer is outstanding.
    controller.tick_identification(&mut world, 0.1);
    assert_eq!(world.identified, [CORPSE]);

    world.complete_appraisal(CORPSE, false);
    world.corpses = vec![LootCorpse {
        is_identified: true,
        ..unidentified
    }];
    controller.tick_identification(&mut world, 0.1);
    assert_eq!(world.identified, [CORPSE]);

    // The rule's first turn opens the corpse straight away.
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.opened, [CORPSE]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: UnidentifiedCorpseIsOpenedOnceItsAutomationIdentifyCompletes_EvenWithoutPresentation
#[test]
fn unidentified_corpse_is_opened_once_its_automation_identify_completes_even_without_presentation()
{
    const CORPSE: u32 = 0x7000_0401;
    let settings = LootSettings {
        scan_interval_seconds: 0.05,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let unidentified = undescribed(chain_corpse(CORPSE, 3.0));
    let mut world = World::with_corpses(vec![unidentified.clone()]);
    let mut controller = LootController::new(settings);

    controller.tick_identification(&mut world, 0.1);
    assert_eq!(world.identified, [CORPSE]);
    assert!(world.opened.is_empty());

    world.complete_appraisal(CORPSE, false);
    world.corpses = vec![LootCorpse {
        is_identified: true,
        ..unidentified
    }];
    assert_eq!(world.presented_object_id, 0);

    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.opened, [CORPSE]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ControllerUsesSelectedExternalClassifierAndPreservesItsAction
#[test]
fn controller_uses_selected_external_classifier_and_preserves_its_action() {
    const CORPSE: u32 = 0x7000_0011;
    const ITEM: u32 = 0x7000_0012;
    let settings = LootSettings {
        external_classifier_id: "loot-plugin/main".to_owned(),
        ..enabled_settings(vec![rule("Rule", "*", LootAction::NoLoot)])
    };
    let mut world = World {
        classifier: Some(ClassifierRegistry::new(
            "loot-plugin/main",
            classification(PublicLootAction::User3, "External user action", 17, 0),
        )),
        ..World::with_corpses(vec![chain_corpse(CORPSE, 3.0)])
    };
    let mut controller = LootController::new(settings);

    assert!(controller.tick(&mut world, 1.0, true));
    world.current = CORPSE;
    world.contents = vec![item(ITEM, "External prize", 88)];
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));
    world.set_appraisal(1, 0, ITEM);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));

    assert_eq!(world.picked, [ITEM]);
    let registry = world.classifier.as_ref().expect("registry");
    assert_eq!(registry.classification_count, 1);
    assert!(controller.status.contains("External user action"));

    world.owned = vec![item(ITEM, "External prize", 88)];
    world.contents.clear();
    world.picked_up(1, ITEM);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));
    let registry = world.classifier.as_ref().expect("registry");
    assert_eq!(registry.looted.len(), 1);
    assert_eq!(registry.looted[0].item.object_id, ITEM);
    assert_eq!(registry.looted[0].action, PublicLootAction::User3);

    // The pass the corpse finishes on is the pass that closes it, and the
    // corpse stays this controller's business until the container shuts.
    assert!(controller.tick(&mut world, 0.2, true));
    world.current = 0;
    world.owned.clear();
    assert!(!controller.tick(&mut world, 0.2, true));
    assert_eq!(world.classifier.as_ref().expect("registry").removed, [ITEM]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ExternalKeepUpToAndUnavailableClassifierNeverFallBackToInternalRules
#[test]
fn external_keep_up_to_and_unavailable_classifier_never_fall_back_to_internal_rules() {
    const CORPSE: u32 = 0x7000_0021;
    const ITEM: u32 = 0x7000_0022;
    let mut world = World {
        owned: vec![InventoryItem {
            stack_size: 2,
            ..item(0x7000_0023, "Limited prize", 99)
        }],
        classifier: Some(ClassifierRegistry::new(
            "loot-plugin/main",
            classification(PublicLootAction::KeepUpTo, "", 0, 2),
        )),
        ..World::with_corpses(vec![chain_corpse(CORPSE, 3.0)])
    };
    let settings = LootSettings {
        external_classifier_id: "loot-plugin/main".to_owned(),
        ..enabled_settings(vec![expression_rule("*")])
    };
    let mut controller = LootController::new(settings);

    assert!(controller.tick(&mut world, 1.0, true));
    world.current = CORPSE;
    world.contents = vec![item(ITEM, "Limited prize", 99)];
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));
    world.set_appraisal(1, 0, ITEM);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));
    assert!(world.picked.is_empty());

    controller.reset(&mut world);
    world.requested = 0;
    world.current = 0;
    world.opened.clear();
    world.identified.clear();
    "missing/classifier".clone_into(&mut controller.settings.external_classifier_id);
    assert!(controller.tick(&mut world, 1.0, true));
    world.current = CORPSE;
    // The pass the corpse finishes on is the pass that closes it.
    assert!(controller.tick(&mut world, 0.2, true));
    assert!(world.picked.is_empty());
}

fn rare_only_looter(corpses: &[u32]) -> (LootController, World) {
    let settings = LootSettings {
        loot_only_rare_corpses: true,
        ..enabled_settings(vec![rule("Rule", "*", LootAction::NoLoot)])
    };
    let world = World::with_corpses(
        corpses
            .iter()
            .map(|id| LootCorpse::new(*id, "Corpse of Drudge", 3.0))
            .collect(),
    );
    (LootController::new(settings), world)
}

fn announce(world: &mut World, sequence: u64, text: &str) {
    world.chat_messages.push(chat(sequence, text));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: RareOnlyLootingDescribesNoCorpseUntilTheServerAnnouncesARare
#[test]
fn rare_only_looting_describes_no_corpse_until_the_server_announces_a_rare() {
    let (mut controller, mut world) = rare_only_looter(&[0x7000_1001, 0x7000_1002]);
    for _ in 0..10 {
        controller.tick(&mut world, 0.5, true);
        controller.tick_identification(&mut world, 0.5);
    }
    assert!(world.identified.is_empty());
    assert!(!controller.has_corpse_awaiting_description_within(&mut world, 50.0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: RareOnlyLootingWithoutAnAnnouncementListsTheCorpsesOnlyEveryTwoSeconds
#[test]
fn rare_only_looting_without_an_announcement_lists_the_corpses_only_every_two_seconds() {
    let (mut controller, mut world) = rare_only_looter(&[0x7000_1001, 0x7000_1002]);
    let before = world.corpse_capture_count;
    for _ in 0..10 {
        controller.tick_identification(&mut world, 0.5);
    }
    assert!(!controller.has_corpse_awaiting_description_within(&mut world, 50.0));
    let listed = world.corpse_capture_count - before;
    assert!((1..=3).contains(&listed), "{listed}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ARareAnnouncementDescribesOnlyTheNewCorpseWhileTheLootPassCannotAct
#[test]
fn a_rare_announcement_describes_only_the_new_corpse_while_the_loot_pass_cannot_act() {
    let (mut controller, mut world) = rare_only_looter(&[0x7000_1001]);
    for _ in 0..40 {
        controller.tick(&mut world, 0.5, false);
        controller.tick_identification(&mut world, 0.5);
    }
    world
        .corpses
        .push(LootCorpse::new(0x7000_1002, "Corpse of Drudge", 3.0));
    for _ in 0..4 {
        controller.tick(&mut world, 0.5, false);
        controller.tick_identification(&mut world, 0.5);
    }
    announce(
        &mut world,
        1,
        "Tester has discovered the Pearl of Blood Drinking!",
    );
    controller.tick(&mut world, 0.5, false);
    controller.tick_identification(&mut world, 0.5);

    assert_eq!(world.identified, [0x7000_1002]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: RareOnlyLootingOpensTheCorpseTheServerSaysGeneratedARare
#[test]
fn rare_only_looting_opens_the_corpse_the_server_says_generated_a_rare() {
    let (mut controller, mut world) = rare_only_looter(&[]);
    world.corpses = vec![LootCorpse {
        name: "Corpse of Assailer".to_owned(),
        long_description: "Killed by Tester. This corpse generated a rare item!".to_owned(),
        ..chain_corpse(0x7000_1003, 3.0)
    }];
    assert!(controller.tick(&mut world, 0.5, true));
    assert_eq!(world.opened, [0x7000_1003]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: RareOnlyLootingLeavesAnOrdinaryKillOfItsOwnAlone
#[test]
fn rare_only_looting_leaves_an_ordinary_kill_of_its_own_alone() {
    let (mut controller, mut world) = rare_only_looter(&[]);
    world.corpses = vec![LootCorpse {
        name: "Corpse of Assailer".to_owned(),
        ..chain_corpse(0x7000_1004, 3.0)
    }];
    controller.tick(&mut world, 0.5, true);
    assert!(world.opened.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ThisCharactersRareAnnouncementDescribesTheCorpseThatJustAppeared
#[test]
fn this_characters_rare_announcement_describes_the_corpse_that_just_appeared() {
    let (mut controller, mut world) = rare_only_looter(&[0x7000_1001]);
    controller.tick(&mut world, 0.5, true);
    controller.tick(&mut world, 20.0, true);
    controller.tick_identification(&mut world, 0.5);

    world
        .corpses
        .push(LootCorpse::new(0x7000_1002, "Corpse of Drudge", 3.0));
    controller.tick(&mut world, 0.5, true);
    announce(
        &mut world,
        1,
        "Tester has discovered the Pearl of Blood Drinking!",
    );
    controller.tick(&mut world, 0.5, true);
    controller.tick_identification(&mut world, 0.5);

    // The corpse from twenty seconds earlier is not the rare's.
    assert_eq!(world.identified, [0x7000_1002]);
    assert!(controller.has_corpse_awaiting_description_within(&mut world, 50.0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: AnotherPlayersRareAnnouncementDescribesNothing
#[test]
fn another_players_rare_announcement_describes_nothing() {
    let (mut controller, mut world) = rare_only_looter(&[0x7000_1001]);
    announce(
        &mut world,
        1,
        "Someone Else has discovered the Pearl of Blood Drinking!",
    );
    announce(
        &mut world,
        2,
        "Tester says, \"Tester has discovered the Pearl of Blood Drinking!\"",
    );
    controller.tick(&mut world, 0.5, true);
    controller.tick_identification(&mut world, 0.5);
    assert!(world.identified.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ARareAnnouncementStopsDescribingNewCorpsesAfterItsWindow
#[test]
fn a_rare_announcement_stops_describing_new_corpses_after_its_window() {
    let (mut controller, mut world) = rare_only_looter(&[0x7000_1001]);
    announce(
        &mut world,
        1,
        "+Tester has discovered the Pearl of Blood Drinking!",
    );
    controller.tick(&mut world, 0.5, true);
    controller.tick_identification(&mut world, 0.5);
    assert_eq!(world.identified, [0x7000_1001]);

    controller.tick(&mut world, RARE_ANNOUNCEMENT_WINDOW_SECONDS + 1.0, true);
    world
        .corpses
        .push(LootCorpse::new(0x7000_1002, "Corpse of Drudge", 3.0));
    world.appraisal_state.awaiting_id = 0;
    controller.tick(&mut world, 0.5, true);
    controller.tick_identification(&mut world, 5.0);

    assert!(!world.identified.contains(&0x7000_1002));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: FellowshipAndRareCorpsePolicyMatchesVtankProtectionRules
#[test]
fn fellowship_and_rare_corpse_policy_matches_vtank_protection_rules() {
    let settings = LootSettings {
        loot_fellow_corpses: true,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let mut world = World {
        members: vec![FellowMember {
            object_id: 0x5000_0002,
            name: "Fellow".to_owned(),
            current_health: 1,
            max_health: 1,
            current_stamina: 1,
            max_stamina: 1,
            current_mana: 1,
            max_mana: 1,
            distance: 2.0,
            share_loot: true,
            ..FellowMember::default()
        }],
        corpses: vec![LootCorpse {
            long_description: "Killed by Fellow.".to_owned(),
            ..chain_corpse(0x7000_0100, 3.0)
        }],
        ..World::default()
    };
    let mut controller = LootController::new(settings);

    assert!(controller.tick(&mut world, 0.25, true));
    assert_eq!(world.opened, [0x7000_0100]);

    controller.reset(&mut world);
    world.requested = 0;
    world.opened.clear();
    world.members[0].share_loot = false;
    assert!(!controller.tick(&mut world, 0.25, true));
    assert!(controller.tick(&mut world, 100.0, true));
    assert_eq!(world.opened.len(), 1);

    controller.reset(&mut world);
    world.requested = 0;
    world.opened.clear();
    world.corpses[0].is_generated_rare = true;
    assert!(!controller.tick(&mut world, 101.0, true));
    assert!(world.opened.is_empty());

    controller.settings.loot_only_rare_corpses = true;
    world.corpses[0] = LootCorpse {
        is_generated_rare: true,
        long_description: "Killed by Tester. Generated rare treasure.".to_owned(),
        ..world.corpses[0].clone()
    };
    assert!(controller.tick(&mut world, 0.25, true));
    assert_eq!(world.opened.len(), 1);
}

/// Reads one queued scroll the way the reading rule does.
fn read_once(
    reader: &mut ReadScrollController,
    world: &mut World,
    controller: &mut LootController,
) -> bool {
    reader.tick(world, controller, 0.1, true)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: AcceptedReadPickupKeepsItsReceiptAcrossBusyRetryUntilTransfer
#[test]
fn accepted_read_pickup_keeps_its_receipt_across_busy_retry_until_transfer() {
    const CORPSE: u32 = 0x7000_0280;
    const SCROLL: u32 = 0x7000_0281;
    let settings = LootSettings {
        external_classifier_id: "classifier/read".to_owned(),
        ..enabled_settings(vec![rule("Rule", "*", LootAction::NoLoot)])
    };
    let scroll_item = scroll(SCROLL, "Incantation of Testing", 777);
    let mut world = World {
        classifier: Some(ClassifierRegistry::new(
            "classifier/read",
            classification(PublicLootAction::Read, "Read it", 0, 0),
        )),
        ..scroll_world(CORPSE)
    };
    let mut controller = LootController::new(settings);
    let locks = bind_locks(&mut controller);
    let open_timeout = controller.settings.corpse_open_timeout_seconds;

    assert!(controller.tick(&mut world, 0.25, true));
    world.current = CORPSE;
    world.contents = vec![scroll_item.clone()];
    locks.borrow_mut().advance(open_timeout + 0.1);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));
    world.complete_appraisal(SCROLL, false);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.picked, [SCROLL]);

    locks.borrow_mut().advance(1.0);
    world
        .pickup_results
        .push_back(ItemResult::of(ItemStatus::Busy));
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.picked, [SCROLL, SCROLL]);
    assert!(
        world
            .classifier
            .as_ref()
            .expect("registry")
            .looted
            .is_empty()
    );

    world.contents.clear();
    world.owned = vec![scroll_item];
    world.picked_up(1, SCROLL);
    // A busy pull holds the item slot exactly as a sent one does.
    locks.borrow_mut().advance(1.0);
    assert!(controller.tick(&mut world, 0.1, true));

    assert_eq!(
        controller.classified_owned_items().get(&SCROLL),
        Some(&LootAction::Read)
    );
    assert_eq!(controller.pending_scroll_reads().get(&777), Some(&SCROLL));
    let registry = world.classifier.as_ref().expect("registry");
    assert_eq!(registry.looted.len(), 1);
    assert_eq!(registry.looted[0].item.object_id, SCROLL);
    assert_eq!(registry.looted[0].action, PublicLootAction::Read);

    let mut reader = ReadScrollController::new();
    assert!(read_once(&mut reader, &mut world, &mut controller));
    assert_eq!(world.used, [SCROLL]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: KeepUpToReceiptRemainsReservedAcrossBusyRetry
#[test]
fn keep_up_to_receipt_remains_reserved_across_busy_retry() {
    const CORPSE: u32 = 0x7000_0290;
    const FIRST: u32 = 0x7000_0291;
    const SECOND: u32 = 0x7000_0292;
    let mut keep_one = rule("Rule", "*", LootAction::KeepUpTo);
    keep_one.keep_count = 1;
    let first = item(FIRST, "Limited prize", 99);
    let second = item(SECOND, "Limited prize", 99);
    let mut world = World::with_corpses(vec![chain_corpse(CORPSE, 3.0)]);
    let mut controller = LootController::new(enabled_settings(vec![keep_one]));
    let locks = bind_locks(&mut controller);
    let open_timeout = controller.settings.corpse_open_timeout_seconds;

    assert!(controller.tick(&mut world, 0.25, true));
    world.current = CORPSE;
    world.contents = vec![first.clone()];
    locks.borrow_mut().advance(open_timeout + 0.1);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));
    assert_eq!(world.picked, [FIRST]);

    locks.borrow_mut().advance(1.0);
    world.contents = vec![first.clone(), second.clone()];
    world
        .pickup_results
        .push_back(ItemResult::of(ItemStatus::Busy));
    assert!(controller.tick(&mut world, 0.1, true));
    controller.tick_identification(&mut world, 0.5);

    world.contents = vec![second];
    world.owned = vec![first];
    assert!(controller.tick(&mut world, 0.1, true));

    assert_eq!(world.picked, [FIRST, FIRST]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ExhaustedPickupReceiptDoesNotStealTheNextItemsTransfer
#[test]
fn exhausted_pickup_receipt_does_not_steal_the_next_items_transfer() {
    const CORPSE: u32 = 0x7000_02A0;
    const FIRST: u32 = 0x7000_02A1;
    const SECOND: u32 = 0x7000_02A2;
    const SPELL: u32 = 778;
    let settings = LootSettings {
        external_classifier_id: "classifier/read".to_owned(),
        corpse_loot_item_max_attempts: 1,
        ..enabled_settings(vec![rule("Rule", "*", LootAction::NoLoot)])
    };
    let first_item = scroll(FIRST, "First scroll", SPELL);
    let second_item = scroll(SECOND, "Second scroll", SPELL);
    let mut world = World {
        known_spell: Some(spell(SPELL, "Second scroll", 100, 34)),
        skills: vec![skill(34, "War Magic", 90)],
        corpses: vec![chain_corpse(CORPSE, 3.0)],
        classifier: Some(ClassifierRegistry::new(
            "classifier/read",
            classification(PublicLootAction::Read, "Read it", 0, 0),
        )),
        ..World::default()
    };
    let mut controller = LootController::new(settings);
    let locks = bind_locks(&mut controller);
    let open_timeout = controller.settings.corpse_open_timeout_seconds;

    assert!(controller.tick(&mut world, 0.25, true));
    world.current = CORPSE;
    world.contents = vec![first_item.clone(), second_item.clone()];
    locks.borrow_mut().advance(open_timeout + 0.1);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));
    world.complete_appraisal(FIRST, false);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.picked, [FIRST]);

    locks.borrow_mut().advance(1.0);
    world.items_busy = true;
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.picked, [FIRST]);
    world.items_busy = false;
    world.inventory_completion = ac_plugin_abi::items::InventoryCompletion {
        revision: 1,
        kind: ac_plugin_abi::items::InventoryCommandKind::Pickup,
        source_id: FIRST,
        weenie_error: 1,
    };
    assert!(controller.tick(&mut world, 0.1, true));
    world.complete_appraisal(SECOND, false);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.picked.last(), Some(&SECOND));

    locks.borrow_mut().advance(1.0);
    world.contents = vec![first_item];
    world.owned = vec![second_item];
    assert!(controller.tick(&mut world, 0.1, true));

    assert!(!controller.classified_owned_items().contains_key(&FIRST));
    assert_eq!(
        controller.classified_owned_items().get(&SECOND),
        Some(&LootAction::Read)
    );
    assert_eq!(controller.pending_scroll_reads().get(&SPELL), Some(&SECOND));
    let registry = world.classifier.as_ref().expect("registry");
    assert_eq!(registry.looted.len(), 1);
    assert_eq!(registry.looted[0].item.object_id, SECOND);
    let mut reader = ReadScrollController::new();
    assert!(read_once(&mut reader, &mut world, &mut controller));
    assert_eq!(world.used, [SECOND]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ClosedCorpseWithOwnedReceiptClassifiesAndQueuesExactlyOnce
#[test]
fn closed_corpse_with_owned_receipt_classifies_and_queues_exactly_once() {
    const CORPSE: u32 = 0x7000_02B0;
    const SCROLL: u32 = 0x7000_02B1;
    const SPELL: u32 = 779;
    let settings = LootSettings {
        external_classifier_id: "classifier/read".to_owned(),
        ..enabled_settings(vec![rule("Rule", "*", LootAction::NoLoot)])
    };
    let scroll_item = scroll(SCROLL, "Transferred scroll", SPELL);
    let mut world = World {
        known_spell: Some(spell(SPELL, "Transferred scroll", 100, 34)),
        skills: vec![skill(34, "War Magic", 90)],
        corpses: vec![chain_corpse(CORPSE, 3.0)],
        classifier: Some(ClassifierRegistry::new(
            "classifier/read",
            classification(PublicLootAction::Read, "Read it", 0, 0),
        )),
        ..World::default()
    };
    let mut controller = LootController::new(settings);
    let locks = bind_locks(&mut controller);
    let open_timeout = controller.settings.corpse_open_timeout_seconds;

    assert!(controller.tick(&mut world, 0.25, true));
    world.current = CORPSE;
    world.contents = vec![scroll_item.clone()];
    locks.borrow_mut().advance(open_timeout + 0.1);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));
    world.complete_appraisal(SCROLL, false);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.picked, [SCROLL]);

    locks.borrow_mut().advance(1.0);
    world.current = 0;
    world.owned = vec![scroll_item];
    assert!(controller.tick(&mut world, 0.1, true));

    assert_eq!(
        controller.classified_owned_items().get(&SCROLL),
        Some(&LootAction::Read)
    );
    assert_eq!(controller.pending_scroll_reads().get(&SPELL), Some(&SCROLL));
    assert_eq!(world.classifier.as_ref().expect("registry").looted.len(), 1);
    let mut reader = ReadScrollController::new();
    assert!(read_once(&mut reader, &mut world, &mut controller));
    assert_eq!(world.used, [SCROLL]);

    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.classifier.as_ref().expect("registry").looted.len(), 1);
    assert_eq!(world.used, [SCROLL]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ClosedCorpseWithoutOwnedReceiptAbandonsItWithoutClassification
#[test]
fn closed_corpse_without_owned_receipt_abandons_it_without_classification() {
    const CORPSE: u32 = 0x7000_02C0;
    const SCROLL: u32 = 0x7000_02C1;
    const SPELL: u32 = 780;
    let settings = LootSettings {
        external_classifier_id: "classifier/read".to_owned(),
        ..enabled_settings(vec![rule("Rule", "*", LootAction::NoLoot)])
    };
    let scroll_item = scroll(SCROLL, "Absent scroll", SPELL);
    let mut world = World {
        known_spell: Some(spell(SPELL, "Absent scroll", 100, 34)),
        skills: vec![skill(34, "War Magic", 90)],
        corpses: vec![chain_corpse(CORPSE, 3.0)],
        classifier: Some(ClassifierRegistry::new(
            "classifier/read",
            classification(PublicLootAction::Read, "Read it", 0, 0),
        )),
        ..World::default()
    };
    let mut controller = LootController::new(settings);
    let locks = bind_locks(&mut controller);
    let open_timeout = controller.settings.corpse_open_timeout_seconds;

    assert!(controller.tick(&mut world, 0.25, true));
    world.current = CORPSE;
    world.contents = vec![scroll_item];
    locks.borrow_mut().advance(open_timeout + 0.1);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));
    world.complete_appraisal(SCROLL, false);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));

    locks.borrow_mut().advance(1.0);
    world.current = 0;
    assert!(controller.tick(&mut world, 0.1, true));

    assert!(!controller.classified_owned_items().contains_key(&SCROLL));
    assert!(controller.pending_scroll_reads().is_empty());
    assert!(
        world
            .classifier
            .as_ref()
            .expect("registry")
            .looted
            .is_empty()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: UnknownReadableScrollOverridesNoLootAndIsPickedForReading
#[test]
fn unknown_readable_scroll_overrides_no_loot_and_is_picked_for_reading() {
    const CORPSE: u32 = 0x7000_0200;
    const SCROLL: u32 = 0x7000_0201;
    let mut world = scroll_world(CORPSE);
    let mut controller = LootController::new(enabled_settings(vec![rule(
        "Rule",
        "*",
        LootAction::NoLoot,
    )]));
    // The pull holds the item slot; without the slot a second turn is a
    // second pull of the same scroll.
    let _locks = bind_locks(&mut controller);

    assert!(controller.tick(&mut world, 0.25, true));
    world.current = CORPSE;
    assert!(controller.observe_corpse_opened(&mut world));
    world.contents = vec![scroll(SCROLL, "Incantation of Testing", 777)];
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));
    world.set_appraisal(1, 0, SCROLL);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));

    assert_eq!(world.picked, [SCROLL]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: UnopenableCorpseUsesOfficialAttemptBlacklist
#[test]
fn unopenable_corpse_uses_official_attempt_blacklist() {
    let settings = LootSettings {
        corpse_open_timeout_seconds: 0.25,
        blacklist_corpse_open_attempt_count: 2,
        blacklist_corpse_open_timeout_seconds: 200.0,
        scan_interval_seconds: 0.05,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let mut world = World::with_corpses(vec![chain_corpse(0x7000_0300, 3.0)]);
    let mut controller = LootController::new(settings);

    assert!(controller.tick(&mut world, 0.1, true));
    assert!(controller.tick(&mut world, 0.3, true));
    assert!(!controller.tick(&mut world, 0.3, true));
    assert_eq!(world.opened.len(), 2);
    assert!(controller.status.contains("Blacklisted"));
    assert!(!controller.tick(&mut world, 199.0, true));
    assert_eq!(world.opened.len(), 2);
    assert!(controller.tick(&mut world, 2.0, true));
    assert_eq!(world.opened.len(), 3);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: SalvageRuleRunsRealAuthenticSalvageAfterCorpseIsComplete
#[test]
fn salvage_rule_runs_real_authentic_salvage_after_corpse_is_complete() {
    const CORPSE: u32 = 0x7000_0400;
    const SOURCE: u32 = 0x7000_0401;
    const TOOL: u32 = 0x5000_0402;
    let tool = InventoryItem {
        item_type: 0x2000_0000,
        ..item(TOOL, "Salvage Tool", 99)
    };
    let mut world = World {
        owned: vec![tool.clone()],
        ..World::with_corpses(vec![chain_corpse(CORPSE, 3.0)])
    };
    let mut controller = LootController::new(enabled_settings(vec![rule(
        "Rule",
        "*",
        LootAction::Salvage,
    )]));

    assert!(controller.tick(&mut world, 0.25, true));
    world.current = CORPSE;
    world.contents = vec![InventoryItem {
        material_type: 12,
        ..item(SOURCE, "Iron Sword", 100)
    }];
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));
    world.set_appraisal(1, 0, SOURCE);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));
    world.contents.clear();
    world.owned = vec![tool.clone(), item(SOURCE, "Iron Sword", 100)];
    world.picked_up(1, SOURCE);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));
    // The pass the corpse finishes on is the pass that closes it.
    assert!(controller.tick(&mut world, 0.2, true));
    world.current = 0;

    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.salvaged, [(TOOL, SOURCE)]);
    world.owned = vec![tool];
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(controller.status, "Salvaged Iron Sword.");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: AMissingSalvageToolWarnsOnceAndLootingCarriesOn
#[test]
fn a_missing_salvage_tool_warns_once_and_looting_carries_on() {
    const SOURCE: u32 = 0x7000_0411;
    const TOOL: u32 = 0x5000_0412;
    const NEXT_CORPSE: u32 = 0x7000_0413;
    let mut world = World {
        owned: vec![item(SOURCE, "Iron Sword", 100)],
        ..World::default()
    };
    let mut controller = LootController::new(enabled_settings(vec![rule(
        "Rule",
        "*",
        LootAction::Salvage,
    )]));
    let warnings: common::Lines = std::rc::Rc::default();
    let sink = std::rc::Rc::clone(&warnings);
    controller.warning = Some(Box::new(move |line: &str| {
        sink.borrow_mut().push(line.to_owned())
    }));
    controller.mark_owned(&mut world, SOURCE, LootAction::Salvage);

    controller.tick(&mut world, 0.1, true);
    controller.tick(&mut world, 0.1, true);
    world.corpses = vec![chain_corpse(NEXT_CORPSE, 3.0)];
    controller.tick(&mut world, 0.3, true);

    assert!(world.salvaged.is_empty());
    assert_eq!(warnings.borrow().len(), 1);
    assert!(warnings.borrow()[0].contains("Ust"));
    assert!(world.opened.contains(&NEXT_CORPSE));

    world.corpses.clear();
    world.current = 0;
    world.owned.push(InventoryItem {
        item_type: 0x2000_0000,
        ..item(TOOL, "Ust", 99)
    });
    for _ in 0..20 {
        if !world.salvaged.is_empty() {
            break;
        }
        controller.tick(&mut world, 0.3, true);
    }

    assert_eq!(world.salvaged, [(TOOL, SOURCE)]);
    assert_eq!(warnings.borrow().len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: SellRuleQueuesUntilVendorThenRunsAuthoritativeSale
#[test]
fn sell_rule_queues_until_vendor_then_runs_authoritative_sale() {
    const CORPSE: u32 = 0x7000_0500;
    const SOURCE: u32 = 0x7000_0501;
    let mut world = World::with_corpses(vec![chain_corpse(CORPSE, 3.0)]);
    let mut controller =
        LootController::new(enabled_settings(vec![rule("Rule", "*", LootAction::Sell)]));

    assert!(controller.tick(&mut world, 0.25, true));
    world.current = CORPSE;
    world.contents = vec![item(SOURCE, "Ruby", 101)];
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));
    world.set_appraisal(1, 0, SOURCE);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));
    world.contents.clear();
    world.owned = vec![item(SOURCE, "Ruby", 101)];
    world.picked_up(1, SOURCE);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));
    // The pass the corpse finishes on is the pass that closes it.
    assert!(controller.tick(&mut world, 0.2, true));
    world.current = 0;

    assert!(!controller.tick(&mut world, 0.1, true));
    assert!(controller.status.contains("queued"));
    assert!(world.sold.is_empty());
    world.vendor_id = 0x7000_0510;
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.sold, [SOURCE]);
    world.owned.clear();
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(controller.status, "Sold Ruby.");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ACatchAllProfileNeverAppraisesACorpseItem
#[test]
fn a_catch_all_profile_never_appraises_a_corpse_item() {
    const CORPSE: u32 = 0x7000_0600;
    const LOOSE: u32 = 0x7000_0601;
    let mut world = World::with_corpses(vec![LootCorpse {
        name: "Corpse of a Drudge".to_owned(),
        ..chain_corpse(CORPSE, 3.0)
    }]);
    let mut controller = LootController::new(enabled_settings(vec![rule(
        "Keep everything",
        "*",
        LootAction::Keep,
    )]));

    assert!(controller.tick(&mut world, 0.25, true));
    world.current = CORPSE;
    world.contents = vec![item(LOOSE, "Pyreal", 273)];
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));

    assert!(world.identified.is_empty());
    assert_eq!(world.picked, [LOOSE]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: EverythingOnYourOwnDeathCorpseIsAppraised
#[test]
fn everything_on_your_own_death_corpse_is_appraised() {
    const CORPSE: u32 = 0x7000_0700;
    const LOOSE: u32 = 0x7000_0701;
    let mut world = World::with_corpses(vec![LootCorpse {
        name: "Corpse of Tester".to_owned(),
        ..chain_corpse(CORPSE, 3.0)
    }]);
    let mut controller = LootController::new(enabled_settings(vec![rule(
        "Keep everything",
        "*",
        LootAction::Keep,
    )]));

    assert!(controller.tick(&mut world, 0.25, true));
    world.current = CORPSE;
    world.contents = vec![item(LOOSE, "Pyreal", 273)];
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));

    assert_eq!(world.identified, [LOOSE]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: AFartherRareCorpseIsOpenedBeforeANearerMundaneOne
#[test]
fn a_farther_rare_corpse_is_opened_before_a_nearer_mundane_one() {
    const MUNDANE: u32 = 0x7000_0800;
    const RARE: u32 = 0x7000_0801;
    let mut world = World::with_corpses(vec![
        chain_corpse(MUNDANE, 2.0),
        LootCorpse {
            is_generated_rare: true,
            long_description: "Killed by Tester. Generated rare treasure.".to_owned(),
            ..chain_corpse(RARE, 4.5)
        },
    ]);
    let mut controller = LootController::new(enabled_settings(vec![expression_rule("*")]));

    assert!(controller.tick(&mut world, 0.25, true));
    assert_eq!(world.opened, [RARE]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: TheCorpseInReachTheCharacterIsFacingIsOpenedFirst
#[test]
fn the_corpse_in_reach_the_character_is_facing_is_opened_first() {
    const BEHIND: u32 = 0x7000_0F00;
    const AHEAD: u32 = 0x7000_0F01;
    let mut world = World {
        navigation_position: place(0.0, 0.0, 0.0),
        corpses: vec![
            LootCorpse {
                position: Some(place(0.0, -2.0, 0.0)),
                ..chain_corpse(BEHIND, 2.0)
            },
            LootCorpse {
                position: Some(place(0.0, 4.0, 0.0)),
                ..chain_corpse(AHEAD, 4.0)
            },
        ],
        ..World::default()
    };
    let mut controller = LootController::new(enabled_settings(vec![expression_rule("*")]));

    assert!(controller.tick(&mut world, 0.25, true));
    assert_eq!(world.opened, [AHEAD]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ACorpseForgottenAfterTheCacheTimeoutStartsItsAgeClockAgain
#[test]
fn a_corpse_forgotten_after_the_cache_timeout_starts_its_age_clock_again() {
    const CORPSE: u32 = 0x7000_1200;
    let settings = LootSettings {
        loot_all_corpses: true,
        corpse_cache_timeout_minutes: 1.0,
        scan_interval_seconds: 0.05,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let container = LootCorpse {
        long_description: "Killed by Someone Else.".to_owned(),
        ..chain_corpse(CORPSE, 3.0)
    };
    let mut world = World::with_corpses(vec![container.clone()]);
    let mut controller = LootController::new(settings);

    assert!(!controller.tick(&mut world, 0.25, true));

    // Gone from the client for longer than the cache timeout: forgotten.
    world.corpses.clear();
    assert!(!controller.tick(&mut world, 200.0, true));

    // Back again, and only just seen for the first time.
    world.corpses = vec![container];
    assert!(!controller.tick(&mut world, 200.0, true));
    assert!(world.opened.is_empty());

    assert!(controller.tick(&mut world, 200.0, true));
    assert_eq!(world.opened, [CORPSE]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: TheOpenStepDoesNotReachBeyondArmsReach
#[test]
fn the_open_step_does_not_reach_beyond_arms_reach() {
    const NEAR: u32 = 0x7000_1000;
    const FAR: u32 = 0x7000_1001;
    let mut world = World::with_corpses(vec![chain_corpse(FAR, 30.0), chain_corpse(NEAR, 12.0)]);
    let mut controller = LootController::new(enabled_settings(vec![expression_rule("*")]));

    assert!(!controller.tick(&mut world, 0.25, true));
    assert!(world.opened.is_empty());

    // Walked to: the same corpse, now inside the open's own reach.
    world.corpses[1].distance = 3.0;
    assert!(controller.tick(&mut world, 0.25, true));
    assert_eq!(world.opened, [NEAR]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ACorpseWhoseDescriptionNamesNoKillerIsNeverLooted
#[test]
fn a_corpse_whose_description_names_no_killer_is_never_looted() {
    let settings = LootSettings {
        loot_all_corpses: true,
        scan_interval_seconds: 0.05,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let mut world = World::with_corpses(vec![LootCorpse {
        long_description: "Generated treasure".to_owned(),
        ..chain_corpse(0x7000_1100, 3.0)
    }]);
    let mut controller = LootController::new(settings);

    assert!(!controller.tick(&mut world, 200.0, true));
    assert!(!controller.tick(&mut world, 200.0, true));
    assert!(world.opened.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ARefusedCorpseIsSkippedForTenSecondsWithoutRetrying
#[test]
fn a_refused_corpse_is_skipped_for_ten_seconds_without_retrying() {
    let settings = LootSettings {
        corpse_open_timeout_seconds: 0.25,
        scan_interval_seconds: 0.05,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let mut world = World::with_corpses(vec![chain_corpse(0x7000_0900, 3.0)]);
    let mut controller = LootController::new(settings);

    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.opened.len(), 1);
    // The server's refusal is a status-line notice, never a chat line; ACE
    // names whoever has the corpse open.
    controller
        .observe_status_text("The Corpse of a Drudge Slinker is already in use by Unkle Leo!");

    assert!(!controller.tick(&mut world, 0.3, true));
    assert!(!controller.tick(&mut world, 9.0, true));
    assert_eq!(world.opened.len(), 1);

    assert!(controller.tick(&mut world, 1.0, true));
    assert_eq!(world.opened.len(), 2);
}

// C# source: LootingTests.cs :: ACorpseWeHaveNoRightToLootIsSkippedForTenSeconds
#[test]
fn a_corpse_we_have_no_right_to_loot_is_skipped_for_ten_seconds() {
    let settings = LootSettings {
        corpse_open_timeout_seconds: 0.25,
        scan_interval_seconds: 0.05,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let mut world = World::with_corpses(vec![chain_corpse(0x7000_0901, 3.0)]);
    let mut controller = LootController::new(settings);

    assert!(controller.tick(&mut world, 0.1, true));
    controller.observe_status_text(
        "You do not yet have the right to loot the Corpse of a Drudge Slinker.",
    );

    assert!(!controller.tick(&mut world, 0.3, true));
    assert!(!controller.tick(&mut world, 9.0, true));
    assert_eq!(world.opened.len(), 1);
}

// C# source: LootingTests.cs :: ARefusalSaidInChatDoesNotSkipTheCorpse
#[test]
fn a_refusal_said_in_chat_does_not_skip_the_corpse() {
    let settings = LootSettings {
        corpse_open_timeout_seconds: 0.25,
        scan_interval_seconds: 0.05,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let mut world = World::with_corpses(vec![chain_corpse(0x7000_0902, 3.0)]);
    let mut controller = LootController::new(settings);

    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.opened.len(), 1);
    // Anyone can type the server's words; only the server's notice counts.
    world.chat_messages.push(chat(
        1,
        "The Corpse of a Drudge Slinker is already in use by someone else!",
    ));

    controller.tick(&mut world, 0.3, true);
    controller.tick(&mut world, 0.3, true);
    controller.tick(&mut world, 0.3, true);
    assert!(world.opened.len() > 1, "the open was not tried again");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ACorpseSeenOutOfRangeIsAlreadyOldEnoughWhenItComesIntoRange
#[test]
fn a_corpse_seen_out_of_range_is_already_old_enough_when_it_comes_into_range() {
    const CORPSE: u32 = 0x7000_0A00;
    let settings = LootSettings {
        loot_all_corpses: true,
        corpse_approach_range: 20.0,
        scan_interval_seconds: 0.05,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let far = LootCorpse {
        long_description: "Killed by Stranger.".to_owned(),
        ..chain_corpse(CORPSE, 80.0)
    };
    let mut world = World::with_corpses(vec![far.clone()]);
    let mut controller = LootController::new(settings);

    assert!(!controller.tick(&mut world, 0.1, true));
    assert!(!controller.tick(&mut world, 100.0, true));
    assert!(world.opened.is_empty());

    world.corpses = vec![LootCorpse {
        distance: 3.0,
        ..far
    }];
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.opened, [CORPSE]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: TheOpenStepReachesACorpseTheApproachRangeDoesNot
#[test]
fn the_open_step_reaches_a_corpse_the_approach_range_does_not() {
    const CORPSE: u32 = 0x7000_0C10;
    let settings = LootSettings {
        corpse_approach_range: 0.0,
        scan_interval_seconds: 0.05,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let mut world = World::with_corpses(vec![chain_corpse(CORPSE, 3.0)]);
    let mut controller = LootController::new(settings);

    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.opened, [CORPSE]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: OpeningACorpseHoldsTheSlotsTheWalkNeeds
#[test]
fn opening_a_corpse_holds_the_slots_the_walk_needs() {
    const CORPSE: u32 = 0x7000_0C11;
    let settings = LootSettings {
        scan_interval_seconds: 0.05,
        corpse_open_timeout_seconds: 1.5,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let mut world = World::with_corpses(vec![chain_corpse(CORPSE, 3.0)]);
    let mut controller = LootController::new(settings);
    let locks = bind_locks(&mut controller);

    assert!(!locks.borrow_mut().is_locked(ActionLockKind::Navigation));
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.opened, [CORPSE]);

    assert!(locks.borrow_mut().is_locked(ActionLockKind::Navigation));
    assert!(locks.borrow_mut().is_locked(ActionLockKind::ItemUse));
    assert!(
        locks
            .borrow_mut()
            .is_locked(ActionLockKind::CorpseOpenAttempt)
    );

    locks.borrow_mut().advance(1.6);
    assert!(!locks.borrow_mut().is_locked(ActionLockKind::Navigation));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: TheSlotsGoBackTheMomentTheCorpseIsOpen
#[test]
fn the_slots_go_back_the_moment_the_corpse_is_open() {
    const CORPSE: u32 = 0x7000_0C12;
    let settings = LootSettings {
        scan_interval_seconds: 0.05,
        corpse_open_timeout_seconds: 30.0,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let mut world = World::with_corpses(vec![chain_corpse(CORPSE, 3.0)]);
    let mut controller = LootController::new(settings);
    let locks = bind_locks(&mut controller);

    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.opened, [CORPSE]);
    // Nothing has opened yet, so nothing goes back.
    assert!(!controller.observe_corpse_opened(&mut world));
    assert!(locks.borrow_mut().is_locked(ActionLockKind::ItemUse));

    world.requested = CORPSE;
    world.current = CORPSE;

    assert!(controller.observe_corpse_opened(&mut world));
    assert!(
        !locks
            .borrow_mut()
            .is_locked(ActionLockKind::CorpseOpenAttempt)
    );
    assert!(!locks.borrow_mut().is_locked(ActionLockKind::Navigation));
    assert!(!locks.borrow_mut().is_locked(ActionLockKind::ItemUse));
    assert!(!controller.observe_corpse_opened(&mut world));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: AfterTheCorpseOpensTheLooterJudgesOnTheNextPass
#[test]
fn after_the_corpse_opens_the_looter_judges_on_the_next_pass() {
    const CORPSE: u32 = 0x7000_0C13;
    const PRIZE: u32 = 0x7000_0C14;
    let settings = LootSettings {
        scan_interval_seconds: 0.05,
        corpse_open_timeout_seconds: 30.0,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let mut world = World::with_corpses(vec![chain_corpse(CORPSE, 3.0)]);
    let mut controller = LootController::new(settings);
    let locks = bind_locks(&mut controller);
    let judged = capture_log(&mut controller);

    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.opened, [CORPSE]);

    world.requested = CORPSE;
    world.current = CORPSE;
    world.contents = vec![item(PRIZE, "Plain Lockpick", 1)];
    assert!(controller.observe_corpse_opened(&mut world));

    assert!(!locks.borrow_mut().is_locked(ActionLockKind::ItemUse));
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));
    assert!(
        judged
            .borrow()
            .iter()
            .any(|line| line.starts_with("LootDecision: "))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ACorpseBeyondTheApproachRangeIsStillAskedForItsDescription
#[test]
fn a_corpse_beyond_the_approach_range_is_still_asked_for_its_description() {
    const CORPSE: u32 = 0x7000_0C20;
    let settings = LootSettings {
        corpse_approach_range: 0.0,
        scan_interval_seconds: 0.05,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let mut world = World::with_corpses(vec![LootCorpse::new(CORPSE, "Corpse", 30.0)]);
    let mut controller = LootController::new(settings);

    // The rule's turn declines: nothing within reach.
    assert!(!controller.tick(&mut world, 0.1, true));
    assert!(world.identified.is_empty());

    controller.tick_identification(&mut world, 0.1);
    assert_eq!(world.identified, [CORPSE]);
    assert!(world.opened.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: AFinishedCorpseIsClosedWithASecondUse
#[test]
fn a_finished_corpse_is_closed_with_a_second_use() {
    const CORPSE: u32 = 0x7000_0B00;
    let mut world = World::with_corpses(vec![chain_corpse(CORPSE, 3.0)]);
    let mut controller = LootController::new(enabled_settings(vec![rule(
        "Rule",
        "*",
        LootAction::NoLoot,
    )]));

    assert!(controller.tick(&mut world, 0.25, true));
    world.current = CORPSE;
    world.contents = vec![item(0x7000_0B01, "Rock", 273)];
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));

    assert_eq!(controller.status, "Corpse complete.");
    assert_eq!(world.closed, [CORPSE]);
    assert!(world.used.is_empty());
}

/// Opens one corpse holding nothing worth taking.
fn finished_corpse_scenario(items_busy: bool) -> (LootController, World, u32) {
    const CORPSE: u32 = 0x7000_0E00;
    let mut world = World::with_corpses(vec![chain_corpse(CORPSE, 3.0)]);
    let mut controller = LootController::new(enabled_settings(vec![rule(
        "Rule",
        "*",
        LootAction::NoLoot,
    )]));

    assert!(controller.tick(&mut world, 0.25, true));
    world.current = CORPSE;
    world.contents = vec![item(0x7000_0E01, "Rock", 273)];
    world.items_busy = items_busy;
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));
    (controller, world, CORPSE)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: AFinishedCorpseIsNotClosedUntilTheItemChannelIsFree
#[test]
fn a_finished_corpse_is_not_closed_until_the_item_channel_is_free() {
    let (mut controller, mut world, corpse) = finished_corpse_scenario(true);

    assert!(world.closed.is_empty());
    assert_eq!(controller.status, "Waiting to close corpse…");

    // A pass the controller may not act on holds it back just as much.
    world.items_busy = false;
    assert!(controller.tick(&mut world, 0.1, false));
    assert!(world.closed.is_empty());

    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.closed, [corpse]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ARefusedCloseIsTriedAgainUntilTheContainerActuallyShuts
#[test]
fn a_refused_close_is_tried_again_until_the_container_actually_shuts() {
    let (mut controller, mut world, corpse) = finished_corpse_scenario(false);

    assert_eq!(world.closed, [corpse]);

    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.closed, [corpse, corpse]);
    assert_eq!(world.opened.len(), 1);

    world.current = 0;
    assert!(!controller.tick(&mut world, 0.1, true));
    assert_eq!(world.closed, [corpse, corpse]);
}

/// Loots one unknown scroll off a corpse and stops there.
fn scroll_scenario() -> (LootController, ReadScrollController, World, u32) {
    const CORPSE: u32 = 0x7000_0C00;
    const SCROLL: u32 = 0x7000_0C01;
    let mut world = scroll_world(CORPSE);
    let mut controller = LootController::new(enabled_settings(vec![rule(
        "Rule",
        "*",
        LootAction::NoLoot,
    )]));
    let scroll_item = scroll(SCROLL, "Incantation of Testing", 777);

    assert!(controller.tick(&mut world, 0.25, true));
    world.current = CORPSE;
    world.contents = vec![scroll_item.clone()];
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));
    assert_eq!(world.picked, [SCROLL]);

    world.contents.clear();
    world.owned = vec![scroll_item];
    world.picked_up(1, SCROLL);
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.1, true));

    (controller, ReadScrollController::new(), world, SCROLL)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: AFailedScrollReadIsTriedAgainInsteadOfDroppingTheScroll
#[test]
fn a_failed_scroll_read_is_tried_again_instead_of_dropping_the_scroll() {
    let (mut controller, mut reader, mut world, scroll) = scroll_scenario();

    assert!(read_once(&mut reader, &mut world, &mut controller));
    assert_eq!(world.used, [scroll]);

    // The server said no.
    world.use_completion = ItemUseCompletion {
        revision: 1,
        source_id: scroll,
        target_id: 0,
        weenie_error: 7,
    };
    assert!(read_once(&mut reader, &mut world, &mut controller));
    assert_eq!(controller.pending_scroll_reads().len(), 1);
    assert_eq!(controller.pending_scroll_reads().get(&777), Some(&scroll));

    assert!(read_once(&mut reader, &mut world, &mut controller));
    assert_eq!(world.used, [scroll, scroll]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: AQueuedScrollTheCharacterNoLongerHoldsLeavesTheQueue
#[test]
fn a_queued_scroll_the_character_no_longer_holds_leaves_the_queue() {
    let (mut controller, mut reader, mut world, scroll) = scroll_scenario();

    assert!(read_once(&mut reader, &mut world, &mut controller));
    world.use_completion = ItemUseCompletion {
        revision: 1,
        source_id: scroll,
        target_id: 0,
        weenie_error: 0,
    };
    world.owned.clear();
    assert!(read_once(&mut reader, &mut world, &mut controller));

    assert!(!read_once(&mut reader, &mut world, &mut controller));
    assert!(controller.pending_scroll_reads().is_empty());
    assert_eq!(world.used, [scroll]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: AQueuedScrollIsStillReadAfterLootingIsSwitchedOff
#[test]
fn a_queued_scroll_is_still_read_after_looting_is_switched_off() {
    let (mut controller, mut reader, mut world, scroll) = scroll_scenario();
    controller.settings.enabled = false;

    assert!(read_once(&mut reader, &mut world, &mut controller));
    assert_eq!(world.used, [scroll]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: APickedUpScrollIsQueuedForTheReadingRuleInsteadOfReadInline
#[test]
fn a_picked_up_scroll_is_queued_for_the_reading_rule_instead_of_read_inline() {
    let (mut controller, mut reader, mut world, scroll) = scroll_scenario();

    assert!(world.used.is_empty());
    assert_eq!(controller.pending_scroll_reads().len(), 1);
    assert_eq!(controller.pending_scroll_reads().get(&777), Some(&scroll));

    assert!(read_once(&mut reader, &mut world, &mut controller));
    assert_eq!(world.used, [scroll]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: AMiscItemThatMerelyLooksLikeAScrollIsNotRead
#[test]
fn a_misc_item_that_merely_looks_like_a_scroll_is_not_read() {
    const CORPSE: u32 = 0x7000_0D00;
    const FAKE: u32 = 0x7000_0D01;
    let mut world = scroll_world(CORPSE);
    let mut controller = LootController::new(enabled_settings(vec![rule(
        "Rule",
        "*",
        LootAction::NoLoot,
    )]));

    assert!(controller.tick(&mut world, 0.25, true));
    world.current = CORPSE;
    world.contents = vec![InventoryItem {
        item_type: 0x0000_0080,
        spell_id: 777,
        object_class: ObjectClass::Misc,
        ..item(FAKE, "Counterfeit Scroll", 88)
    }];
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));

    assert!(world.picked.is_empty());
    assert!(controller.pending_scroll_reads().is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: AQueuedScrollIsDroppedOnceItsSpellIsKnown
#[test]
fn a_queued_scroll_is_dropped_once_its_spell_is_known() {
    let (mut controller, mut reader, mut world, _) = scroll_scenario();
    world.spell_learned = true;

    assert!(!read_once(&mut reader, &mut world, &mut controller));
    assert!(world.used.is_empty());
    assert!(controller.pending_scroll_reads().is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.cs :: ReadingUnknownScrollsIsAProfileChoice
#[test]
fn reading_unknown_scrolls_is_a_profile_choice() {
    for reads in [true, false] {
        let mut world = World {
            known_spell: Some(spell(777, "Incantation of Testing", 100, 34)),
            skills: vec![skill(34, "War Magic", 90)],
            ..World::default()
        };
        let settings = LootSettings {
            enabled: true,
            read_unknown_scrolls: reads,
            ..LootSettings::default()
        };
        let queued = mosstank_dotnet::collections::DotNetDictionary::new();
        assert_eq!(
            is_eligible(
                &mut world,
                &settings,
                &scroll(0x7000_0C01, "Incantation of Testing", 777),
                &queued,
                true
            ),
            reads
        );
    }
}

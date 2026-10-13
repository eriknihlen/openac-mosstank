//! Corpse descriptions, the closing use, a reset, and the looter's side of
//! the corpse walk (`LootingTests.AppraisalAnswered.cs`,
//! `.CorpseDescriptionGiveUp.cs`, `.CorpseDescriptionRetry.cs`,
//! `.CorpseClose.cs`, `.Reset.cs`, `.CorpseApproach.cs`).

mod common;

use common::{
    World, approach_corpse, bind_locks, capture_log, chat, contains_line, enabled_settings,
    expression_rule, item, own_rare_corpse, rule, undescribed,
};
use mosstank_automation::locks::ActionLockKind;
use mosstank_formats::utl::LootAction;
use mosstank_loot::LootController;
use mosstank_loot::host::LootCorpse;
use mosstank_settings::model::LootSettings;

fn description_settings() -> LootSettings {
    LootSettings {
        scan_interval_seconds: 0.05,
        corpse_open_timeout_seconds: 1.5,
        ..enabled_settings(vec![expression_rule("*")])
    }
}

fn approach_world(corpses: Vec<LootCorpse>) -> World {
    World {
        navigation_position: ac_plugin_abi::world::Position {
            cell_id: 0x7F7F_0001,
            is_outdoor: true,
            ..ac_plugin_abi::world::Position::default()
        },
        ..World::with_corpses(corpses)
    }
}

fn approach_settings(range: f64, walk_to_own_rares: bool) -> LootSettings {
    LootSettings {
        corpse_approach_range: range,
        scan_interval_seconds: 0.05,
        walk_to_own_rare_corpses: walk_to_own_rares,
        ..enabled_settings(vec![expression_rule("*")])
    }
}

fn next_appraisal(world: &mut World, current: u32) {
    world.set_appraisal(world.appraisal_state.revision + 1, 0, current);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.AppraisalAnswered.cs :: NavigationOnlyWaitsForDescriptionsWhenLootingCanProcessThem
#[test]
fn navigation_only_waits_for_descriptions_when_looting_can_process_them() {
    let rows = [
        (false, true, true, false),
        (true, false, true, false),
        (true, true, false, false),
        (true, true, false, true),
        (true, true, true, false),
    ];
    for (active, enabled, rules, external) in rows {
        let mut settings = description_settings();
        settings.profile_active = active;
        settings.enabled = enabled;
        if !rules {
            settings.rules.clear();
        }
        settings.external_classifier_id = if external {
            "utility/loot".to_owned()
        } else {
            String::new()
        };
        let mut world = approach_world(vec![undescribed(approach_corpse(0x7000_F199, 3.0))]);
        let mut looter = LootController::new(settings);
        let eligible = active && enabled && (rules || external);

        for _ in 0..4 {
            looter.tick(&mut world, 0.25, true);
            looter.tick_identification(&mut world, 0.25);
            assert_eq!(
                looter.has_corpse_awaiting_description_within(&mut world, 15.0),
                eligible,
                "{active} {enabled} {rules} {external}"
            );
        }
        assert_eq!(!world.identified.is_empty(), eligible);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.AppraisalAnswered.cs :: ACorpseAnsweredWithoutADescriptionIsNeitherWaitedOnNorAskedAgain
#[test]
fn a_corpse_answered_without_a_description_is_neither_waited_on_nor_asked_again() {
    const DECAYED: u32 = 0x7000_F101;
    let mut world = approach_world(vec![LootCorpse {
        is_identified: false,
        is_appraisal_answered: true,
        ..approach_corpse(DECAYED, 3.0)
    }]);
    let mut looter = LootController::new(description_settings());

    assert!(!looter.has_corpse_awaiting_description_within(&mut world, 15.0));
    looter.tick_identification(&mut world, 0.25);
    looter.tick_identification(&mut world, 4.0);

    assert!(world.identified.is_empty());
    assert!(!looter.has_corpse_awaiting_description_within(&mut world, 15.0));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.AppraisalAnswered.cs :: ACorpseAnsweredWithoutADescriptionIsPassedOver
#[test]
fn a_corpse_answered_without_a_description_is_passed_over() {
    const DECAYED: u32 = 0x7000_F111;
    let settings = LootSettings {
        loot_all_corpses: true,
        loot_fellow_corpses: true,
        scan_interval_seconds: 0.05,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let mut world = World::with_corpses(vec![LootCorpse {
        is_appraisal_answered: true,
        ..LootCorpse::new(DECAYED, "Corpse", 3.0)
    }]);
    let mut controller = LootController::new(settings);

    for elapsed in [0.1, 100.5, 0.3, 600.0] {
        assert!(!controller.tick(&mut world, elapsed, true));
        controller.tick_identification(&mut world, elapsed);
        assert!(!controller.has_corpse_awaiting_description_within(&mut world, 15.0));
    }

    assert!(world.opened.is_empty());
    assert!(world.identified.is_empty());
    assert_eq!(controller.status, "No nearby corpses.");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseClose.cs :: FinishedCorpseClosesThroughTheLootSurface
#[test]
fn finished_corpse_closes_through_the_loot_surface() {
    const CORPSE: u32 = 0x7000_C001;
    let settings = LootSettings {
        corpse_item_appearance_timeout_seconds: 0.0,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let mut world = approach_world(vec![approach_corpse(CORPSE, 3.0)]);
    let mut looter = LootController::new(settings);

    assert!(looter.tick(&mut world, 0.25, true));
    world.requested = CORPSE;
    world.current = CORPSE;
    assert!(looter.tick(&mut world, 0.25, true));

    assert_eq!(world.closed, [CORPSE]);
    assert!(world.used.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseDescriptionGiveUp.cs :: ACorpseThatNeverAnswersStopsHoldingTheWalksOff
#[test]
fn a_corpse_that_never_answers_stops_holding_the_walks_off() {
    const SILENT: u32 = 0x7000_F001;
    let mut world = approach_world(vec![undescribed(approach_corpse(SILENT, 3.0))]);
    world.one_description_at_a_time = true;
    world.never_answers.insert(SILENT);
    let mut looter = LootController::new(description_settings());
    let log = capture_log(&mut looter);

    // Three questions, three answers that never came; only the third gives
    // the corpse up.
    for _ in 0..3 {
        looter.tick_identification(&mut world, 0.25);
        assert!(looter.has_corpse_awaiting_description_within(&mut world, 15.0));
        world.release_stalled_description();
        looter.tick_identification(&mut world, 4.0);
    }

    assert_eq!(world.identified.len(), 3);
    assert!(!looter.has_corpse_awaiting_description_within(&mut world, 15.0));
    assert!(contains_line(&log, "Skipping corpse 0x7000F001"));

    // And it is not asked again while it is skipped.
    looter.tick_identification(&mut world, 4.0);
    assert_eq!(world.identified.len(), 3);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseDescriptionGiveUp.cs :: ACorpseWhoseDescriptionIsOnItsWayStillHoldsTheWalksOff
#[test]
fn a_corpse_whose_description_is_on_its_way_still_holds_the_walks_off() {
    const CORPSE: u32 = 0x7000_F011;
    let mut world = approach_world(vec![undescribed(approach_corpse(CORPSE, 3.0))]);
    world.one_description_at_a_time = true;
    world.never_answers.insert(CORPSE);
    let mut looter = LootController::new(description_settings());

    looter.tick_identification(&mut world, 0.25);
    assert!(looter.has_corpse_awaiting_description_within(&mut world, 15.0));
    looter.tick_identification(&mut world, 0.25);
    assert!(looter.has_corpse_awaiting_description_within(&mut world, 15.0));
    assert_eq!(world.identified.len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseDescriptionGiveUp.cs :: ASilentCorpseDoesNotStarveTheCorpsesBehindIt
#[test]
fn a_silent_corpse_does_not_starve_the_corpses_behind_it() {
    const SILENT: u32 = 0x7000_F021;
    const NEXT: u32 = 0x7000_F022;
    let mut world = approach_world(vec![
        undescribed(approach_corpse(SILENT, 3.0)),
        undescribed(approach_corpse(NEXT, 3.0)),
    ]);
    world.one_description_at_a_time = true;
    world.never_answers.insert(SILENT);
    let mut looter = LootController::new(description_settings());

    looter.tick_identification(&mut world, 0.25);
    assert_eq!(world.identified, [SILENT]);

    world.release_stalled_description();
    looter.tick_identification(&mut world, 4.0);

    assert_eq!(world.identified, [SILENT, NEXT]);
    assert_eq!(world.appraisal_state.current_id, NEXT);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseDescriptionGiveUp.cs :: AnItemDescriptionOutstandingWhenTheCorpseShutsDoesNotStopTheNextCorpse
#[test]
fn an_item_description_outstanding_when_the_corpse_shuts_does_not_stop_the_next_corpse() {
    const CORPSE: u32 = 0x7000_F031;
    const ITEM: u32 = 0x7000_F032;
    const LATER: u32 = 0x7000_F033;
    let mut world = approach_world(vec![approach_corpse(CORPSE, 3.0)]);
    world.one_description_at_a_time = true;
    world.never_answers.insert(ITEM);
    let mut coins = rule("Coins", "name ~= coin", LootAction::Keep);
    coins.priority = 3;
    let mut looter = LootController::new(enabled_settings(vec![coins]));

    assert!(looter.tick(&mut world, 0.25, true));
    assert_eq!(world.opened, [CORPSE]);
    world.requested = CORPSE;
    world.current = CORPSE;
    world.contents = vec![item(ITEM, "Colosseum coin", 77)];
    looter.tick_identification(&mut world, 0.25);
    assert_eq!(world.identified, [ITEM]);

    // The corpse shuts with the question still outstanding.
    world.current = 0;
    world.contents.clear();
    world.release_stalled_description();
    world.corpses = vec![
        approach_corpse(CORPSE, 3.0),
        undescribed(approach_corpse(LATER, 3.0)),
    ];

    looter.tick_identification(&mut world, 0.25);
    assert_eq!(world.identified, [ITEM, LATER]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseDescriptionRetry.cs :: DescribedCorpseOpensWhileAnotherDescriptionIsPending
#[test]
fn described_corpse_opens_while_another_description_is_pending() {
    const UNANSWERED: u32 = 0x7000_D001;
    const READY: u32 = 0x7000_D002;
    const SOMETHING_ELSE: u32 = 0x7000_D003;
    let mut world = approach_world(vec![undescribed(approach_corpse(UNANSWERED, 3.0))]);
    let mut looter = LootController::new(description_settings());

    looter.tick_identification(&mut world, 0.25);
    assert_eq!(world.identified, [UNANSWERED]);

    world.corpses = vec![
        undescribed(approach_corpse(UNANSWERED, 3.0)),
        approach_corpse(READY, 3.0),
    ];
    assert!(looter.tick(&mut world, 0.25, true));
    assert_eq!(world.opened, [READY]);

    world.requested = 0;
    world.current = 0;
    world.corpses = vec![undescribed(approach_corpse(UNANSWERED, 3.0))];
    next_appraisal(&mut world, SOMETHING_ELSE);
    looter.tick_identification(&mut world, 4.0);
    assert_eq!(world.identified, [UNANSWERED, UNANSWERED]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseDescriptionRetry.cs :: UnansweredCorpseDescriptionsRotateFairly
#[test]
fn unanswered_corpse_descriptions_rotate_fairly() {
    const FIRST: u32 = 0x7000_D011;
    const SECOND: u32 = 0x7000_D012;
    const SOMETHING_ELSE: u32 = 0x7000_D013;
    let mut world = approach_world(vec![
        undescribed(approach_corpse(FIRST, 3.0)),
        undescribed(approach_corpse(SECOND, 3.0)),
    ]);
    let mut looter = LootController::new(description_settings());

    looter.tick_identification(&mut world, 0.25);
    assert_eq!(world.identified, [FIRST]);

    next_appraisal(&mut world, SOMETHING_ELSE);
    looter.tick_identification(&mut world, 4.0);
    assert_eq!(world.identified, [FIRST, SECOND]);

    next_appraisal(&mut world, SOMETHING_ELSE);
    looter.tick_identification(&mut world, 4.0);
    assert_eq!(world.identified, [FIRST, SECOND, FIRST]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseDescriptionRetry.cs :: ACorpseWhoseDescriptionIsOvertakenIsAskedAgainNotWrittenOff
#[test]
fn a_corpse_whose_description_is_overtaken_is_asked_again_not_written_off() {
    const CORPSE: u32 = 0x7000_E001;
    const SOMETHING_ELSE: u32 = 0x7000_E002;
    let mut world = approach_world(vec![undescribed(approach_corpse(CORPSE, 3.0))]);
    let mut looter = LootController::new(description_settings());

    looter.tick_identification(&mut world, 0.25);
    assert_eq!(world.identified, [CORPSE]);

    next_appraisal(&mut world, SOMETHING_ELSE);
    looter.tick_identification(&mut world, 4.0);
    looter.tick_identification(&mut world, 0.25);
    assert_eq!(world.identified, [CORPSE, CORPSE]);

    world.corpses = vec![approach_corpse(CORPSE, 3.0)];
    next_appraisal(&mut world, CORPSE);
    assert!(looter.tick(&mut world, 0.25, true));
    assert_eq!(world.opened, [CORPSE]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.Reset.cs :: AResetDoesNotReadTheChatAlreadyWrittenAgain
#[test]
fn a_reset_does_not_read_the_chat_already_written_again() {
    let settings = LootSettings {
        loot_only_rare_corpses: true,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let mut world = World::default();
    let mut controller = LootController::new(settings);
    let logged = capture_log(&mut controller);
    let announced = |logged: &common::Lines| {
        logged
            .borrow()
            .iter()
            .filter(|line| line.contains("rare announced"))
            .count()
    };
    world
        .chat_messages
        .push(chat(5, "Tester has discovered the Warrior's Crystal!"));
    controller.tick_identification(&mut world, 0.1);
    assert_eq!(announced(&logged), 1);

    controller.reset(&mut world);
    controller.tick_identification(&mut world, 0.1);

    assert_eq!(announced(&logged), 1);
    world.chat_messages.push(chat(
        6,
        "Tester has discovered the Pearl of Blood Drinking!",
    ));
    controller.tick_identification(&mut world, 0.1);
    assert_eq!(announced(&logged), 2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.Reset.cs :: AnEmptiedOwnRareCorpseStaysEmptiedAcrossAResetWithTheOptionOn
#[test]
fn an_emptied_own_rare_corpse_stays_emptied_across_a_reset_with_the_option_on() {
    const RARE: u32 = 0x7000_2050;
    for walk_to_own_rares in [true, false] {
        let mut world = approach_world(vec![own_rare_corpse(RARE, 3.0)]);
        let mut controller = LootController::new(approach_settings(30.0, walk_to_own_rares));

        assert!(controller.tick(&mut world, 1.0, true));
        assert_eq!(world.opened, [RARE]);
        world.requested = RARE;
        world.current = RARE;
        world.contents.clear();
        for _ in 0..60 {
            if controller.status == "Corpse complete." {
                break;
            }
            controller.tick_identification(&mut world, 0.25);
            controller.tick(&mut world, 0.25, true);
        }
        assert_eq!(controller.status, "Corpse complete.");
        world.current = 0;
        assert!(
            controller
                .try_select_approach_corpse(&mut world, 30.0)
                .is_none()
        );

        controller.reset(&mut world);

        assert_eq!(
            controller
                .try_select_approach_corpse(&mut world, 30.0)
                .is_some(),
            !walk_to_own_rares
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: APullHoldsNavigationSoTheWalkCannotTakeTheCharacterAway
#[test]
fn a_pull_holds_navigation_so_the_walk_cannot_take_the_character_away() {
    const CORPSE: u32 = 0x7000_2100;
    const COIN: u32 = 0x7000_2101;
    let settings = LootSettings {
        scan_interval_seconds: 0.05,
        ..enabled_settings(vec![expression_rule("*")])
    };
    let mut world = approach_world(vec![approach_corpse(CORPSE, 3.0)]);
    let mut controller = LootController::new(settings);
    let locks = bind_locks(&mut controller);

    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.opened, [CORPSE]);

    // The container is open, so the open's own three slots go back.
    world.requested = CORPSE;
    world.current = CORPSE;
    world.contents = vec![item(COIN, "Colosseum coin", 77)];
    assert!(controller.observe_corpse_opened(&mut world));
    assert!(!locks.borrow_mut().is_locked(ActionLockKind::Navigation));

    // Read the contents, decide, pull.
    controller.tick_identification(&mut world, 0.5);
    assert!(controller.tick(&mut world, 0.2, true));
    assert!(controller.tick(&mut world, 0.1, true));
    assert_eq!(world.picked, [COIN]);

    assert!(locks.borrow_mut().is_locked(ActionLockKind::Navigation));
    assert!(locks.borrow_mut().is_locked(ActionLockKind::ItemUse));
    locks.borrow_mut().advance(0.8);
    assert!(!locks.borrow_mut().is_locked(ActionLockKind::Navigation));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/LootingTests.CorpseApproach.cs :: ThisCharactersRareHoldsTheRouteOnlyWhereTheWalkOrTheOpenReachesIt
#[test]
fn this_characters_rare_holds_the_route_only_where_the_walk_or_the_open_reaches_it() {
    for (distance, holds) in [(12.0f32, true), (6.0, false), (3.0, true)] {
        let mut settings = approach_settings(15.0, true);
        settings.corpse_minimum_approach_range = 8.0;
        let mut world = approach_world(vec![own_rare_corpse(0x7000_2036, distance)]);
        let mut loot = LootController::new(settings);
        let range = loot.settings.corpse_approach_range;

        assert_eq!(
            loot.has_own_rare_corpse_to_fetch(&mut world, range),
            holds,
            "{distance}"
        );
    }
}

/// The looter's half of `WithTheOptionOffThisCharactersRarePastTheRangeIsLeft`:
/// with the option off the rare past the range is no corpse to fetch, and
/// not the walk's pick.
#[test]
fn with_the_option_off_this_characters_rare_past_the_range_is_no_corpse_to_fetch() {
    let settings = approach_settings(15.0, false);
    assert!(!settings.walk_to_own_rare_corpses);
    let mut world = approach_world(vec![own_rare_corpse(0x7000_2032, 30.0)]);
    let mut loot = LootController::new(settings);

    assert!(loot.try_select_approach_corpse(&mut world, 15.0).is_none());
    assert!(!loot.has_own_rare_corpse_to_fetch(&mut world, 15.0));
}

/// The looter's half of `ThisCharactersRareIsWalkedToOutToTheRareReach`:
/// with the option on the walk's pick reaches this character's rare out to
/// the rare reach and no further.
#[test]
fn with_the_option_on_the_walks_pick_reaches_this_characters_rare_out_to_the_rare_reach() {
    for (distance, picked) in [(30.0f32, true), (99.0, true), (101.0, false)] {
        let mut world = approach_world(vec![own_rare_corpse(0x7000_2030, distance)]);
        let mut loot = LootController::new(approach_settings(15.0, true));
        assert_eq!(
            loot.try_select_approach_corpse(&mut world, 15.0).is_some(),
            picked,
            "{distance}"
        );
    }
}

/// The looter's half of `AStuckWalkToThisCharactersRareIsGivenUp`: a
/// given-up walk sets the rare aside, so it stops holding the route.
#[test]
fn a_given_up_walk_to_this_characters_rare_releases_the_route() {
    let mut world = approach_world(vec![own_rare_corpse(0x7000_2034, 30.0)]);
    let mut loot = LootController::new(approach_settings(15.0, true));
    let log = capture_log(&mut loot);
    assert!(loot.has_own_rare_corpse_to_fetch(&mut world, 15.0));

    loot.give_up_own_rare_walk(0x7000_2034, "no ground covered for 3 seconds");

    assert!(!loot.has_own_rare_corpse_to_fetch(&mut world, 15.0));
    assert!(contains_line(
        &log,
        "giving up the walk to this character's rare 0x70002034"
    ));
}

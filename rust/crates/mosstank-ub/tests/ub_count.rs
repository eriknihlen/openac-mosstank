//! The counter, against `InventoryCountControllerTests`. The C# fake keeps
//! the lines shown and the lines submitted (the thinks) in one list; here
//! they are read as the two they are, each in its own order.

mod common;

use std::cell::RefMut;

use ac_plugin_abi::items::{AppraisalState, InventoryItem, ItemStatus};
use ac_plugin_abi::world::{ObjectClass, Position, WorldObject};
use common::Rig;
use mosstank_formats::utl::{LootAction, LootRule};
use mosstank_ub::tools::count::InventoryCountController;

fn item(id: u32, name: &str, stack_size: i32, equipped: u32, wielder: u32) -> InventoryItem {
    InventoryItem {
        object_id: id,
        name: name.to_owned(),
        container_id: 1,
        wielder_id: wielder,
        equipped_location: equipped,
        stack_size,
        ..InventoryItem::default()
    }
}

fn plain(id: u32, name: &str) -> InventoryItem {
    item(id, name, 1, 0, 0)
}

fn known(id: u32, name: &str, appraised: bool) -> WorldObject {
    WorldObject {
        object_id: id,
        name: name.to_owned(),
        object_class: ObjectClass::Misc,
        container_id: 1,
        is_owned: true,
        has_appraisal_data: appraised,
        ..WorldObject::default()
    }
}

fn at(north_south: f64, elevation: f64) -> Option<Position> {
    Some(Position {
        cell_id: 0x0001_0001,
        north_south,
        elevation,
        is_outdoor: true,
        ..Position::default()
    })
}

fn landscape_player(id: u32, name: &str, north_south: f64, elevation: f64) -> WorldObject {
    WorldObject {
        object_id: id,
        name: name.to_owned(),
        object_class: ObjectClass::Player,
        is_landscape: true,
        position: at(north_south, elevation),
        ..WorldObject::default()
    }
}

/// "Tester" (id 1) at the origin.
fn rig() -> Rig {
    let mut rig = Rig::new().at_heading(0.0);
    let character = rig.host.meta.character_mut();
    character.id = 1;
    "Tester".clone_into(&mut character.name);
    // The expression runtime asks for the quest list as it starts.
    rig.host.meta.submitted.clear();
    rig
}

/// Marks every known object as described, and makes a described record for
/// every owned item the table lacks (`AppraiseAll`).
fn appraise_all(rig: &mut Rig) {
    for object in &mut rig.host.meta.objects {
        object.has_appraisal_data = true;
    }
    let owned = rig.host.meta.owned_items.clone();
    for item in owned {
        if rig
            .host
            .meta
            .objects
            .iter()
            .any(|object| object.object_id == item.object_id)
        {
            continue;
        }
        rig.host
            .meta
            .objects
            .push(known(item.object_id, &item.name, true));
    }
}

fn rule(name: &str, expression: &str, action: LootAction) -> LootRule {
    let mut rule = LootRule::default();
    name.clone_into(&mut rule.name);
    rule.action = action;
    rule.set_expression(expression);
    rule
}

/// One rule that keeps anything called "trade ...", and a catch-all that
/// keeps nothing.
fn save_trade_profile(rig: &mut Rig) {
    rig.services.loot_profiles.push((
        "Counted".to_owned(),
        vec![
            rule("Trade", "name ~= trade", LootAction::Keep),
            rule("Rest", "*", LootAction::NoLoot),
        ],
    ));
}

fn counter(rig: &mut Rig) -> RefMut<'_, InventoryCountController> {
    rig.commands.tools_mut().count.borrow_mut()
}

fn start(rig: &mut Rig) -> bool {
    rig.with_call(|call, commands| {
        commands
            .tools_mut()
            .count
            .borrow_mut()
            .try_start_profile(call, "Counted", true)
    })
}

fn tick(rig: &mut Rig, seconds: f64, can_act: bool) -> bool {
    rig.with_call(|call, commands| {
        commands
            .tools_mut()
            .count
            .borrow_mut()
            .tick(call, seconds, can_act)
    })
}

fn profile_count(rig: &mut Rig, name: &str) -> f64 {
    rig.with_call(|call, commands| {
        commands
            .tools_mut()
            .count
            .borrow_mut()
            .profile_count(call, name)
    })
}

fn total(rig: &mut Rig) -> Option<i32> {
    counter(rig).last_profile_tally().map(|tally| tally.total)
}

fn strings(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| (*line).to_owned()).collect()
}

/// One waiting item, "Trade Pyreal" x5, not yet appraised.
fn one_waiting() -> Rig {
    let mut rig = rig();
    rig.host.meta.owned_items = vec![item(10, "Trade Pyreal", 5, 0, 0)];
    rig.host.meta.objects = vec![known(10, "Trade Pyreal", false)];
    save_trade_profile(&mut rig);
    rig
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryCountControllerTests.cs :: NameCountSumsStackSizesPerDistinctNameAndReportsATotal
#[test]
fn name_count_sums_stack_sizes_per_distinct_name_and_reports_a_total() {
    let mut rig = rig();
    rig.host.meta.owned_items = vec![
        item(10, "Prismatic Taper", 50, 0, 0),
        item(11, "Prismatic Taper", 25, 0, 0),
        plain(12, "Pyreal Mote"),
        // Worn gear still belongs to the character.
        item(13, "Prismatic Taper Pouch", 1, 8, 0),
    ];

    rig.ub("count item prismatic taper");

    assert_eq!(
        strings(&[
            "/t Tester, Counter: Item Count: Prismatic Taper - 75",
            "/t Tester, Counter: Item Count: Prismatic Taper Pouch - 1",
            "/t Tester, Counter: Total Item Count: 76",
        ]),
        rig.submitted()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryCountControllerTests.cs :: NameCountReportsNothingFoundAgainstThePatternThatWasAsked
#[test]
fn name_count_reports_nothing_found_against_the_pattern_that_was_asked() {
    let mut rig = rig();
    rig.host.meta.owned_items = vec![plain(10, "Pyreal Mote")];

    rig.ub("count item taper");

    assert_eq!(
        strings(&[
            "/t Tester, Counter: Item Count: taper - 0",
            "/t Tester, Counter: Total Item Count: 0"
        ]),
        rig.submitted()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryCountControllerTests.cs :: NameCountRefusesAnUnusablePattern
#[test]
fn name_count_refuses_an_unusable_pattern() {
    let mut rig = rig();
    let error = InventoryCountController::try_count_by_name(&mut rig.host, "(unclosed")
        .expect_err("the pattern does not compile");
    assert!(error.starts_with("bad name pattern:"), "{error}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryCountControllerTests.cs :: PlayerCountTakesOnlyLandscapePlayersInsideTheHorizontalRange
#[test]
fn player_count_takes_only_landscape_players_inside_the_horizontal_range() {
    let mut rig = rig();
    rig.host.meta.objects = vec![
        // 24 m north: inside.
        landscape_player(20, "Near", 0.1, 0.0),
        // 24 m north and far above: inside, the climb is not measured.
        landscape_player(21, "Upstairs", 0.1, 5.0),
        // 240 m north: outside.
        landscape_player(22, "Far", 1.0, 0.0),
        // The character itself is one of the players standing there.
        landscape_player(1, "Tester", 0.0, 0.0),
        WorldObject {
            object_id: 23,
            name: "Drudge".to_owned(),
            object_class: ObjectClass::Monster,
            is_landscape: true,
            position: at(0.1, 0.0),
            ..WorldObject::default()
        },
        // Carried, so not on the landscape at all.
        WorldObject {
            object_id: 24,
            name: "Mule".to_owned(),
            object_class: ObjectClass::Player,
            container_id: 1,
            position: at(0.1, 0.0),
            ..WorldObject::default()
        },
    ];

    rig.ub("count player 100");

    assert_eq!(
        strings(&["/t Tester, Counter: Player Count: 3"]),
        rig.submitted()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryCountControllerTests.cs :: ProfileCountSkipsWornAndWieldedGearAndTalliesByRuleAndByName
#[test]
fn profile_count_skips_worn_and_wielded_gear_and_tallies_by_rule_and_by_name() {
    let mut rig = rig();
    rig.host.meta.owned_items = vec![
        item(10, "Trade Pyreal", 5, 0, 0),
        item(11, "Trade Pyreal", 3, 0, 0),
        plain(12, "Trade Note"),
        item(13, "Trade Shield", 1, 8, 0),
        item(14, "Trade Sword", 1, 0, 1),
        plain(15, "Personal Note"),
    ];
    appraise_all(&mut rig);
    save_trade_profile(&mut rig);

    assert!(start(&mut rig));

    assert!(!counter(&mut rig).is_running());
    assert_eq!(
        strings(&["[UB] Counter: Finished IDing Items"]),
        rig.messages()
    );
    assert_eq!(
        strings(&[
            "/t Tester, Counter: Rule Count: Trade - 9",
            "/t Tester, Counter: Item Count: Trade Note - 1",
            "/t Tester, Counter: Item Count: Trade Pyreal - 8",
            "/t Tester, Counter: Total Item Count: 9",
        ]),
        rig.submitted()
    );
    assert_eq!(Some(9), total(&mut rig));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryCountControllerTests.cs :: ProfileCountWaitsForAppraisalsAndOwnsTheCharacterWhileItDoes
#[test]
fn profile_count_waits_for_appraisals_and_owns_the_character_while_it_does() {
    let mut rig = one_waiting();

    assert!(start(&mut rig));
    assert!(counter(&mut rig).is_running());
    assert_eq!(
        strings(&["[UB] Counter: Items remaining to ID: 1"]),
        rig.messages()
    );

    // Waiting on an appraisal, the count owns the character.
    assert!(tick(&mut rig, 0.1, true));
    assert_eq!(vec![10], rig.host.identified);

    appraise_all(&mut rig);
    assert!(!tick(&mut rig, 0.1, true));
    assert!(!counter(&mut rig).is_running());
    assert_eq!(
        strings(&[
            "[UB] Counter: Items remaining to ID: 1",
            "[UB] Counter: Finished IDing Items",
        ]),
        rig.messages()
    );
    assert_eq!(
        strings(&[
            "/t Tester, Counter: Rule Count: Trade - 5",
            "/t Tester, Counter: Item Count: Trade Pyreal - 5",
            "/t Tester, Counter: Total Item Count: 5",
        ]),
        rig.submitted()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryCountControllerTests.cs :: ProfileCountStandsDownWhileTheLooterHoldsTheAppraisalSlot
#[test]
fn profile_count_stands_down_while_the_looter_holds_the_appraisal_slot() {
    let mut rig = one_waiting();
    rig.services.corpse_identifier_busy = true;
    assert!(start(&mut rig));

    assert!(tick(&mut rig, 0.1, true));
    assert!(rig.host.identified.is_empty());

    // The looter lets go, but the slot still carries somebody's question.
    rig.services.corpse_identifier_busy = false;
    rig.host.appraisal = AppraisalState {
        revision: 1,
        awaiting_id: 99,
        current_id: 0,
        last_abandoned_id: 0,
        ..AppraisalState::default()
    };
    assert!(tick(&mut rig, 0.1, true));
    assert!(rig.host.identified.is_empty());

    rig.host.appraisal = AppraisalState::default();
    assert!(tick(&mut rig, 0.1, true));
    assert_eq!(vec![10], rig.host.identified);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryCountControllerTests.cs :: ProfileCountAskedForByAMacroNeverOwnsTheCharacterNorWrites
#[test]
fn profile_count_asked_for_by_a_macro_never_owns_the_character_nor_writes() {
    let mut rig = one_waiting();

    assert!((profile_count(&mut rig, "Counted") - -1.0).abs() < f64::EPSILON);
    assert!(counter(&mut rig).is_running());
    assert!(!tick(&mut rig, 0.1, true));
    assert_eq!(vec![10], rig.host.identified);
    assert!(rig.messages().is_empty());

    appraise_all(&mut rig);
    assert!(!tick(&mut rig, 0.1, true));
    assert!(rig.messages().is_empty());
    assert!(rig.submitted().is_empty());
    assert!((profile_count(&mut rig, "Counted") - 5.0).abs() < f64::EPSILON);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryCountControllerTests.cs :: ProfileCountSaysAProfileItCannotReadOnceRatherThanEveryCall
#[test]
fn profile_count_says_a_profile_it_cannot_read_once_rather_than_every_call() {
    let mut rig = rig();

    assert!((profile_count(&mut rig, "Missing") - -1.0).abs() < f64::EPSILON);
    assert!((profile_count(&mut rig, "Missing") - -1.0).abs() < f64::EPSILON);

    let messages = rig.messages();
    assert_eq!(1, messages.len(), "{messages:?}");
    assert!(messages[0].contains("Missing"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryCountControllerTests.cs :: AWaitingProfileCountSaysHowMuchIsLeftEveryTenSeconds
#[test]
fn a_waiting_profile_count_says_how_much_is_left_every_ten_seconds() {
    let mut rig = one_waiting();
    assert!(start(&mut rig));

    assert!(tick(&mut rig, 9.5, false));
    assert_eq!(1, rig.messages().len());

    assert!(tick(&mut rig, 1.0, false));
    assert_eq!(
        strings(&[
            "[UB] Counter: Items remaining to ID: 1",
            "[UB] Counter: Items remaining to ID: 1"
        ]),
        rig.messages()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryCountControllerTests.cs :: ASecondProfileCountIsRefusedWhileOneIsStillWaiting
#[test]
fn a_second_profile_count_is_refused_while_one_is_still_waiting() {
    let mut rig = one_waiting();

    assert!(start(&mut rig));
    assert!(!start(&mut rig));
    assert!(counter(&mut rig).status().contains("already running"));

    counter(&mut rig).reset();
    assert!(!counter(&mut rig).is_running());
    assert!(start(&mut rig));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryCountControllerTests.cs :: AProfileCountPassesOverAnItemTheClientHasNoRecordOf
#[test]
fn a_profile_count_passes_over_an_item_the_client_has_no_record_of() {
    let mut rig = rig();
    rig.host.meta.owned_items = vec![item(10, "Trade Pyreal", 5, 0, 0), plain(11, "Trade Note")];
    rig.host.meta.objects = vec![known(10, "Trade Pyreal", true)];
    save_trade_profile(&mut rig);

    assert!(start(&mut rig));

    assert!(!counter(&mut rig).is_running());
    assert!(rig.host.identified.is_empty());
    assert_eq!(Some(5), total(&mut rig));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryCountControllerTests.cs :: ARefusedRequestDoesNotStopTheCountAskingAboutTheRest
#[test]
fn a_refused_request_does_not_stop_the_count_asking_about_the_rest() {
    let mut rig = rig();
    rig.host.meta.owned_items = vec![item(10, "Trade Pyreal", 5, 0, 0), plain(11, "Trade Note")];
    rig.host.meta.objects = vec![
        known(10, "Trade Pyreal", false),
        known(11, "Trade Note", false),
    ];
    rig.host.identify_refused.insert(10);
    save_trade_profile(&mut rig);
    assert!(start(&mut rig));

    assert!(tick(&mut rig, 0.1, true));

    assert_eq!(vec![10, 11], rig.host.identified);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryCountControllerTests.cs :: AnItemTheClientKeepsRefusingIsGivenUpOnAfterThreeAttempts
#[test]
fn an_item_the_client_keeps_refusing_is_given_up_on_after_three_attempts() {
    let mut rig = one_waiting();
    rig.host.identify_status = ItemStatus::Refused;
    assert!(start(&mut rig));

    assert!(tick(&mut rig, 0.6, true));
    assert!(tick(&mut rig, 0.6, true));
    assert!(tick(&mut rig, 0.6, true));
    assert!(!tick(&mut rig, 0.6, true));

    assert!(!counter(&mut rig).is_running());
    assert_eq!(vec![10, 10, 10], rig.host.identified);
    assert_eq!(Some(5), total(&mut rig));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryCountControllerTests.cs :: AnItemThatIsNeverAnsweredIsGivenUpOnAfterThreeTimeouts
#[test]
fn an_item_that_is_never_answered_is_given_up_on_after_three_timeouts() {
    let mut rig = one_waiting();
    assert!(start(&mut rig));

    // Each pass sends one request and waits out the ten-second timeout.
    assert!(tick(&mut rig, 10.5, true));
    assert!(tick(&mut rig, 10.5, true));
    assert!(tick(&mut rig, 10.5, true));
    assert!(!tick(&mut rig, 10.5, true));

    assert!(!counter(&mut rig).is_running());
    assert_eq!(vec![10, 10, 10], rig.host.identified);
    assert_eq!(Some(5), total(&mut rig));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryCountControllerTests.cs :: AnAbandonedRequestSpendsItsAttemptWithoutWaitingOutTheTimeout
#[test]
fn an_abandoned_request_spends_its_attempt_without_waiting_out_the_timeout() {
    let mut rig = one_waiting();
    assert!(start(&mut rig));

    // The client gives up on every question it is handed.
    rig.host.appraisal = AppraisalState {
        revision: 1,
        awaiting_id: 0,
        current_id: 0,
        last_abandoned_id: 10,
        ..AppraisalState::default()
    };
    assert!(tick(&mut rig, 0.6, true));
    assert!(tick(&mut rig, 0.6, true));
    assert!(tick(&mut rig, 0.6, true));
    assert!(!tick(&mut rig, 0.6, true));

    assert!(!counter(&mut rig).is_running());
    assert_eq!(vec![10, 10, 10], rig.host.identified);
    assert_eq!(Some(5), total(&mut rig));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryCountControllerTests.cs :: AForegroundCountGivesTheCharacterBackWhenItRunsPastItsDeadline
#[test]
fn a_foreground_count_gives_the_character_back_when_it_runs_past_its_deadline() {
    let mut rig = one_waiting();
    // The looter never lets the slot go, so nothing ever spends an attempt.
    rig.services.corpse_identifier_busy = true;
    assert!(start(&mut rig));

    assert!(tick(&mut rig, 59.0, true));
    assert!(tick(&mut rig, 59.0, true));
    assert!(tick(&mut rig, 59.0, true));
    assert!(!tick(&mut rig, 59.0, true));

    assert!(!counter(&mut rig).is_running());
    assert!(rig.host.identified.is_empty());
    assert!(counter(&mut rig).status().contains("gave up"));
    assert!(rig.messages().iter().any(|line| line.contains("gave up")));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/InventoryCountControllerTests.cs :: CancelStopsAWaitingCountAndSaysWhatItStopped
#[test]
fn cancel_stops_a_waiting_count_and_says_what_it_stopped() {
    let mut rig = one_waiting();
    assert_eq!("Item counter is not running.", counter(&mut rig).cancel());

    assert!(start(&mut rig));
    assert!(counter(&mut rig).is_running());

    assert_eq!("Item counter stopped: Counted.", counter(&mut rig).cancel());
    assert!(!counter(&mut rig).is_running());
    assert!(!tick(&mut rig, 0.1, true));
}

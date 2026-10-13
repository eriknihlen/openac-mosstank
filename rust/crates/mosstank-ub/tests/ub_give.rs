//! The item giver, against `ProfileGiveControllerTests` and the panel's
//! `/ub ig` tests.

mod common;

use ac_plugin_abi::items::{InventoryCommandKind, InventoryCompletion, InventoryItem, ItemStatus};
use ac_plugin_abi::world::{ObjectClass, Position, WorldObject};
use common::Rig;
use mosstank_formats::utl::{LootAction, LootRule};
use mosstank_ub::commands::GiveNameMatch;
use mosstank_ub::tools::give::ProfileGiveController;

const MULE: u32 = 100;

fn standing(name: &str, object_id: u32, east_west: f64, elevation: f64) -> WorldObject {
    WorldObject {
        object_id,
        name: name.to_owned(),
        object_class: ObjectClass::Player,
        position: Some(Position {
            cell_id: 0x0001_0001,
            east_west,
            elevation,
            is_outdoor: true,
            ..Position::default()
        }),
        ..WorldObject::default()
    }
}

fn item(id: u32, name: &str) -> InventoryItem {
    InventoryItem {
        object_id: id,
        name: name.to_owned(),
        container_id: 1,
        stack_size: 1,
        ..InventoryItem::default()
    }
}

fn stack(id: u32, name: &str, size: i32) -> InventoryItem {
    InventoryItem {
        stack_size: size,
        ..item(id, name)
    }
}

fn rule(name: &str, expression: &str, action: LootAction) -> LootRule {
    let mut rule = LootRule::default();
    name.clone_into(&mut rule.name);
    rule.action = action;
    rule.set_expression(expression);
    rule
}

/// "Tester" (id 1) at the origin, with one target of that name so many
/// metres east (`Nearby`); the inventory settings at their defaults.
fn nearby(name: &str, meters: f64) -> Rig {
    let mut rig = Rig::new().at_heading(0.0);
    {
        let character = rig.host.meta.character_mut();
        character.id = 1;
        "Tester".clone_into(&mut character.name);
    }
    rig.live.give_delay.set(0.0);
    rig.host.meta.objects = vec![standing(name, MULE, meters / 240.0, 0.0)];
    rig
}

fn owned(rig: &mut Rig, items: Vec<InventoryItem>) {
    rig.host.meta.owned_items = items;
}

fn give(rig: &mut Rig) -> &mut ProfileGiveController {
    &mut rig.commands.tools_mut().give
}

fn by_name(
    rig: &mut Rig,
    pattern: &str,
    matching: GiveNameMatch,
    count: i32,
    target: &str,
    partial: bool,
) -> bool {
    rig.with_call(|call, commands| {
        commands
            .tools_mut()
            .give
            .try_start_by_name(call, pattern, matching, count, target, partial)
    })
}

fn by_profile(rig: &mut Rig, profile: &str, target: &str, partial: bool) -> bool {
    rig.with_call(|call, commands| {
        commands
            .tools_mut()
            .give
            .try_start(call, profile, target, partial)
    })
}

fn tick(rig: &mut Rig, seconds: f64) -> bool {
    rig.with_call(|call, commands| commands.tools_mut().give.tick(call, seconds, true))
}

fn gives(rig: &Rig) -> Vec<u32> {
    rig.host.gives.iter().map(|(item, ..)| *item).collect()
}

fn complete(rig: &mut Rig, revision: i64, source_id: u32, weenie_error: u32) {
    rig.host.meta.item_state.last_inventory_completion = InventoryCompletion {
        revision,
        kind: InventoryCommandKind::Give,
        source_id,
        weenie_error,
    };
}

/// The ids a start queued, read out by running the queue dry with every
/// give answered at once.
fn queued(rig: &mut Rig) -> Vec<u32> {
    rig.host.gives.clear();
    let owned_before = rig.host.meta.owned_items.clone();
    for _ in 0..20 {
        if !give(rig).is_running() {
            break;
        }
        tick(rig, 0.1);
        if let Some(&(last, ..)) = rig.host.gives.last() {
            rig.host
                .meta
                .owned_items
                .retain(|item| item.object_id != last);
        }
    }
    rig.host.meta.owned_items = owned_before;
    gives(rig)
}

fn trade_profile() -> Vec<LootRule> {
    vec![
        rule("Trade", "name ~= trade", LootAction::Keep),
        rule("Rest", "*", LootAction::NoLoot),
    ]
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ProfileGiveControllerTests.cs :: NamedProfileGivesOnlyKeepMatchesAndWaitsForCompletion
#[test]
fn named_profile_gives_only_keep_matches_and_waits_for_completion() {
    let mut rig = nearby("Mule", 1.0);
    owned(
        &mut rig,
        vec![item(10, "Trade Pyreal"), item(11, "Personal Note")],
    );
    rig.services
        .loot_profiles
        .push(("Mule Items".to_owned(), trade_profile()));

    assert!(by_profile(&mut rig, "Mule Items.utl", "Mule", false));
    assert!(tick(&mut rig, 0.1));
    assert_eq!(vec![(10, MULE, 0)], rig.host.gives);

    assert!(tick(&mut rig, 0.1));
    assert_eq!(1, rig.host.gives.len());
    complete(&mut rig, 1, 10, 0);
    // The answer and the end of the run arrive on the same step.
    assert!(!tick(&mut rig, 0.1));
    assert!(!give(&mut rig).is_running());
    assert!(give(&mut rig).status().contains("1 item(s)"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ProfileGiveControllerTests.cs :: StartRejectsBusyMissingProfileAndMissingTarget
#[test]
fn start_rejects_busy_missing_profile_and_missing_target() {
    let mut rig = nearby("Mule", 1.0);
    rig.host.meta.objects[0].object_class = ObjectClass::Npc;

    assert!(!by_profile(&mut rig, "Missing", "Mule", false));
    rig.services
        .loot_profiles
        .push(("Empty".to_owned(), Vec::new()));
    assert!(!by_profile(&mut rig, "Empty", "Missing", false));
    assert!(by_profile(&mut rig, "Empty", "Mule", false));
    assert!(!by_profile(&mut rig, "Empty", "Mule", false));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ProfileGiveControllerTests.cs :: NameGiveTakesTheWholeNameAPartOfItOrAPattern
#[test]
fn name_give_takes_the_whole_name_a_part_of_it_or_a_pattern() {
    let mut rig = nearby("Mule", 1.0);
    owned(
        &mut rig,
        vec![
            item(10, "Personal Note"),
            item(11, "Personal Note, Sealed"),
            item(12, "Prismatic Taper"),
        ],
    );

    assert!(by_name(
        &mut rig,
        "personal note",
        GiveNameMatch::Exact,
        0,
        "Mule",
        false
    ));
    assert_eq!(vec![10], queued(&mut rig));

    assert!(by_name(
        &mut rig,
        "personal",
        GiveNameMatch::Partial,
        0,
        "Mule",
        false
    ));
    assert_eq!(vec![10, 11], queued(&mut rig));

    assert!(by_name(
        &mut rig,
        "^pri.*taper$",
        GiveNameMatch::Pattern,
        0,
        "Mule",
        false
    ));
    assert_eq!(vec![12], queued(&mut rig));

    assert!(!by_name(
        &mut rig,
        "(unclosed",
        GiveNameMatch::Pattern,
        0,
        "Mule",
        false
    ));
    assert!(give(&mut rig).status().to_lowercase().contains("pattern"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ProfileGiveControllerTests.cs :: NameGiveSplitsTheStackThatTheCountRunsOutIn
#[test]
fn name_give_splits_the_stack_that_the_count_runs_out_in() {
    let mut rig = nearby("Mule", 1.0);
    owned(
        &mut rig,
        vec![
            stack(10, "Prismatic Taper", 10),
            stack(11, "Prismatic Taper", 10),
        ],
    );

    assert!(by_name(
        &mut rig,
        "Prismatic Taper",
        GiveNameMatch::Exact,
        14,
        "Mule",
        false
    ));
    assert!(tick(&mut rig, 0.1));
    assert_eq!((10, MULE, 0), rig.host.gives[0]);
    rig.host.meta.owned_items.remove(0);
    assert!(tick(&mut rig, 0.1));
    assert!(tick(&mut rig, 0.1));
    assert_eq!((11, MULE, 4), rig.host.gives[1]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ProfileGiveControllerTests.cs :: NameGiveRefusesATargetBeyondTheRangeMeasuredFlat
#[test]
fn name_give_refuses_a_target_beyond_the_range_measured_flat() {
    let mut rig = nearby("Mule", 20.0);
    owned(&mut rig, vec![item(10, "Trade Pyreal")]);

    assert!(!by_name(
        &mut rig,
        "Trade Pyreal",
        GiveNameMatch::Exact,
        0,
        "Mule",
        false
    ));
    assert!(give(&mut rig).status().to_lowercase().contains("range"));

    // One metre off but fifty overhead is in range: height is not measured.
    rig.host.meta.objects = vec![standing("Mule", MULE, 1.0 / 240.0, 50.0)];
    assert!(by_name(
        &mut rig,
        "Trade Pyreal",
        GiveNameMatch::Exact,
        0,
        "Mule",
        false
    ));
}

fn rares(rig: &mut Rig) {
    rig.services.item_giver_profiles.push((
        "rares".to_owned(),
        vec![rule("Rule", "*", LootAction::Keep)],
    ));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ProfileGiveControllerTests.cs :: BothHandOversRefuseATargetOutOfRangeInTheReferencesWords
#[test]
fn both_hand_overs_refuse_a_target_out_of_range_in_the_references_words() {
    let mut rig = nearby("Mule", 20.0);
    owned(&mut rig, vec![item(10, "Trade Pyreal")]);
    rares(&mut rig);

    assert!(!by_profile(&mut rig, "rares", "mul", true));
    assert_eq!(
        "ItemGiver mul is 20.00 meters away. IGRange is set to 15",
        give(&mut rig).refusal()
    );

    assert!(!by_name(
        &mut rig,
        "Trade Pyreal",
        GiveNameMatch::Exact,
        0,
        "mul",
        true
    ));
    assert_eq!(
        "mul is 20.00 meters away, IGRange is set to 15. bailing.",
        give(&mut rig).refusal()
    );
    assert!(rig.host.gives.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ProfileGiveControllerTests.cs :: TheFinishedLineNamesTheRunAsTheReferenceDoes
#[test]
fn the_finished_line_names_the_run_as_the_reference_does() {
    let mut rig = nearby("Mule", 1.0);
    rares(&mut rig);

    assert!(by_profile(&mut rig, "rares", "mul", true));
    give(&mut rig).stop_requested();
    assert_eq!(
        "ItemGiver finished: rares.utl to mul. took 0s to give 0 item(s). 0",
        give(&mut rig).finished_line()
    );

    assert!(by_name(
        &mut rig,
        "Trade PYREAL",
        GiveNameMatch::Partial,
        0,
        "Mule",
        false
    ));
    give(&mut rig).stop_requested();
    assert_eq!(
        "ItemGiver finished: trade pyreal to Mule. took 0s to give 0 item(s). 0",
        give(&mut rig).finished_line()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ProfileGiveControllerTests.cs :: TheFinishedLineCountsTheAsksThatGaveNothing
#[test]
fn the_finished_line_counts_the_asks_that_gave_nothing() {
    let mut rig = nearby("Mule", 1.0);
    owned(&mut rig, vec![item(10, "Trade Pyreal")]);
    rig.host.give_status = ItemStatus::Busy;

    assert!(by_name(
        &mut rig,
        "Trade Pyreal",
        GiveNameMatch::Exact,
        0,
        "Mule",
        false
    ));
    for _ in 0..3 {
        tick(&mut rig, 0.1);
    }
    rig.host.give_status = ItemStatus::Started;
    tick(&mut rig, 0.1);
    rig.host.meta.owned_items.clear();
    for _ in 0..3 {
        if !give(&mut rig).is_running() {
            break;
        }
        tick(&mut rig, 0.1);
    }

    assert!(!give(&mut rig).is_running());
    assert_eq!(4, rig.host.gives.len());
    assert!(
        give(&mut rig)
            .finished_line()
            .ends_with("to give 1 item(s). 3")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ProfileGiveControllerTests.cs :: AnItemIsAskedForOnceMoreThanTheBusyCountThenWrittenOff
#[test]
fn an_item_is_asked_for_once_more_than_the_busy_count_then_written_off() {
    let mut rig = nearby("Mule", 1.0);
    owned(
        &mut rig,
        (10..15).map(|id| item(id, "Trade Pyreal")).collect(),
    );
    rig.host.give_status = ItemStatus::Refused;
    rig.live.give_busy_retry_limit.set(2);
    rig.live.give_failure_limit.set(1);

    assert!(by_name(
        &mut rig,
        "Trade Pyreal",
        GiveNameMatch::Exact,
        0,
        "Mule",
        false
    ));
    for _ in 0..20 {
        if !give(&mut rig).is_running() {
            break;
        }
        tick(&mut rig, 0.1);
    }

    assert!(!give(&mut rig).is_running());
    assert_eq!(vec![10, 10, 10, 11, 11, 11], gives(&rig));
    assert!(give(&mut rig).status().to_lowercase().contains("gave up"));
    assert!(
        give(&mut rig)
            .finished_line()
            .ends_with("to give 0 item(s). 6")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ProfileGiveControllerTests.cs :: AGiveTheServerRefusesIsAskedForAgain
#[test]
fn a_give_the_server_refuses_is_asked_for_again() {
    let mut rig = nearby("Mule", 1.0);
    owned(&mut rig, vec![item(10, "Trade Pyreal")]);

    assert!(by_name(
        &mut rig,
        "Trade Pyreal",
        GiveNameMatch::Exact,
        0,
        "Mule",
        false
    ));
    tick(&mut rig, 0.1);
    complete(&mut rig, 1, 10, 0x2B);
    tick(&mut rig, 0.1);
    tick(&mut rig, 0.1);

    assert!(give(&mut rig).is_running());
    assert_eq!(vec![10, 10], gives(&rig));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ProfileGiveControllerTests.cs :: AGiveDelayLongerThanTheBailClockDoesNotBail
#[test]
fn a_give_delay_longer_than_the_bail_clock_does_not_bail() {
    let mut rig = nearby("Mule", 1.0);
    owned(
        &mut rig,
        vec![item(10, "Trade Pyreal"), item(11, "Trade Pyreal")],
    );
    rig.live.give_delay.set(15.0);

    assert!(by_name(
        &mut rig,
        "Trade Pyreal",
        GiveNameMatch::Exact,
        0,
        "Mule",
        false
    ));
    tick(&mut rig, 0.1);
    rig.host.meta.owned_items.remove(0);
    for _ in 0..20 {
        if rig.host.gives.len() >= 2 {
            break;
        }
        tick(&mut rig, 1.0);
    }

    assert!(give(&mut rig).is_running());
    assert_eq!("", give(&mut rig).end_error());
    assert_eq!(vec![10, 11], gives(&rig));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ProfileGiveControllerTests.cs :: StopRequestedEndsARunAndReportsWhenThereIsNoneToEnd
#[test]
fn stop_requested_ends_a_run_and_reports_when_there_is_none_to_end() {
    let mut rig = nearby("Mule", 1.0);
    owned(&mut rig, vec![item(10, "Trade Pyreal")]);

    assert!(!give(&mut rig).stop_requested());
    assert!(by_name(
        &mut rig,
        "Trade Pyreal",
        GiveNameMatch::Exact,
        0,
        "Mule",
        false
    ));
    assert!(give(&mut rig).stop_requested());
    assert!(!give(&mut rig).is_running());
    assert!(!give(&mut rig).stop_requested());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ProfileGiveControllerTests.cs :: AGiveDelayWaitsBetweenHandOversAndNotBeforeTheFirst
#[test]
fn a_give_delay_waits_between_hand_overs_and_not_before_the_first() {
    let mut rig = nearby("Mule", 1.0);
    owned(
        &mut rig,
        vec![item(10, "Trade Pyreal"), item(11, "Trade Pyreal")],
    );
    rig.live.give_delay.set(1.0);

    assert!(by_name(
        &mut rig,
        "Trade Pyreal",
        GiveNameMatch::Exact,
        0,
        "Mule",
        false
    ));
    assert!(tick(&mut rig, 0.1));
    assert_eq!(1, rig.host.gives.len());

    rig.host.meta.owned_items.remove(0);
    assert!(tick(&mut rig, 0.1));
    assert!(tick(&mut rig, 0.5));
    assert_eq!(1, rig.host.gives.len());
    assert!(tick(&mut rig, 0.6));
    assert_eq!(2, rig.host.gives.len());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ProfileGiveControllerTests.cs :: AGiveWithNoTargetNamedIsRefusedRatherThanMatchingEverybody
#[test]
fn a_give_with_no_target_named_is_refused_rather_than_matching_everybody() {
    let mut rig = nearby("Mule", 1.0);
    owned(&mut rig, vec![item(10, "Prismatic Taper")]);
    rig.services.loot_profiles.push((
        "Mule Items".to_owned(),
        vec![rule("Rule", "*", LootAction::Keep)],
    ));

    assert!(!by_profile(&mut rig, "Mule Items.utl", "", true));
    assert!(give(&mut rig).status().contains("Syntax:"));
    assert!(give(&mut rig).status().contains("/ub ig"));
    assert!(!give(&mut rig).is_running());

    assert!(!by_name(
        &mut rig,
        "Prismatic Taper",
        GiveNameMatch::Exact,
        0,
        "   ",
        false
    ));
    assert!(give(&mut rig).status().contains("Syntax:"));
    assert!(give(&mut rig).status().contains("/ub give"));
    assert!(!give(&mut rig).is_running());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ProfileGiveControllerTests.cs :: AProfileInTheItemGiverFolderIsFoundByName
#[test]
fn a_profile_in_the_item_giver_folder_is_found_by_name() {
    let mut rig = nearby("Town Crier", 1.0);
    owned(&mut rig, vec![item(10, "Prismatic Taper")]);
    rares(&mut rig);

    assert!(
        by_profile(&mut rig, "rares", "Town Crier", false),
        "{}",
        give(&mut rig).status()
    );
    assert!(give(&mut rig).is_running());
    give(&mut rig).stop_requested();
    assert!(
        by_profile(&mut rig, "rares.utl", "Town Crier", false),
        "{}",
        give(&mut rig).status()
    );
    assert_eq!(
        "mosstank/ub/itemgiver/rares.utl",
        mosstank_stores::layout::item_giver_profile_key("rares.utl")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ProfileGiveControllerTests.cs :: TheTargetIsFoundByIdHexIdSelectedOrNearestName
#[test]
fn the_target_is_found_by_id_hex_id_selected_or_nearest_name() {
    for (typed, expected) in [
        ("100", 100),
        ("0x64", 100),
        ("64", 100),
        ("selected", 100),
        ("Mule", 90),
    ] {
        let mut rig = nearby("Mule", 5.0);
        rig.host.meta.objects = vec![
            standing("Mule", 100, 5.0 / 240.0, 0.0),
            standing("Mule", 90, 2.0 / 240.0, 0.0),
            standing("Tester", 1, 0.0, 0.0),
        ];
        owned(&mut rig, vec![item(10, "Prismatic Taper")]);
        rig.host.meta.selection.selected = Some(100);

        assert!(
            by_name(
                &mut rig,
                "Prismatic Taper",
                GiveNameMatch::Exact,
                0,
                typed,
                false
            ),
            "{typed}: {}",
            give(&mut rig).refusal()
        );
        tick(&mut rig, 0.1);
        assert_eq!(1, rig.host.gives.len(), "{typed}");
        assert_eq!(expected, rig.host.gives[0].1, "{typed}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ProfileGiveControllerTests.cs :: GivingToYourselfIsRefusedInTheReferencesWords
#[test]
fn giving_to_yourself_is_refused_in_the_references_words() {
    let mut rig = nearby("Mule", 5.0);
    rig.host.meta.objects.push(standing("Tester", 1, 0.0, 0.0));
    owned(&mut rig, vec![item(10, "Prismatic Taper")]);
    rig.host.meta.selection.selected = Some(1);

    assert!(!by_name(
        &mut rig,
        "Prismatic Taper",
        GiveNameMatch::Exact,
        0,
        "selected",
        false
    ));
    assert_eq!("You can't give to yourself", give(&mut rig).refusal());
    assert!(rig.host.gives.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ProfileGiveControllerTests.cs :: ARefusedStartNeverReportsTheReasonBeforeIt
#[test]
fn a_refused_start_never_reports_the_reason_before_it() {
    let mut rig = nearby("Mule", 1.0);
    owned(&mut rig, vec![item(10, "Prismatic Taper")]);

    assert!(!by_name(
        &mut rig,
        "Prismatic Taper",
        GiveNameMatch::Exact,
        0,
        "Nobody",
        false
    ));
    assert_eq!("player Nobody not found", give(&mut rig).refusal());

    rig.host.meta.character = None;
    assert!(!by_name(
        &mut rig,
        "Prismatic Taper",
        GiveNameMatch::Exact,
        0,
        "Mule",
        false
    ));
    assert_eq!(
        "Item giver refused: not in the world.",
        give(&mut rig).refusal()
    );
    assert!(!by_profile(&mut rig, "rares", "Mule", false));
    assert_eq!(
        "Item giver refused: not in the world.",
        give(&mut rig).refusal()
    );
}

// ---- MossTankPanelTests.UbCommands: /ub ig -------------------------------------

fn zero_cool() -> Rig {
    let mut rig = Rig::new().at_heading(0.0);
    rig.host.meta.objects = vec![common::player(40, "Zero Cool")];
    rig
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TheItemGiverFindsItsTargetByTheWholeName
#[test]
fn the_item_giver_finds_its_target_by_the_whole_name() {
    let mut rig = zero_cool();
    rig.ub("ig muledItems to Zero Cool");
    // The target is resolved before the profile is read, so a missing
    // profile is proof the target was found.
    assert!(rig.messages().contains(
        &"[UB] Error: InventoryManager: ItemGiver Profile does not exist: muledItems".to_owned()
    ));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TheItemGiversPartialFlagMatchesPartOfTheTargetName
#[test]
fn the_item_givers_partial_flag_matches_part_of_the_target_name() {
    let mut rig = zero_cool();
    rig.ub("igp muledItems to Zero");
    assert!(rig.messages().contains(
        &"[UB] Error: InventoryManager: ItemGiver Profile does not exist: muledItems".to_owned()
    ));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TheItemGiverWithoutThePartialFlagRefusesAHalfName
#[test]
fn the_item_giver_without_the_partial_flag_refuses_a_half_name() {
    let mut rig = zero_cool();
    rig.ub("ig muledItems to Zero");
    assert!(
        rig.messages()
            .contains(&"[UB] Error: InventoryManager: player Zero not found".to_owned())
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbCommands.cs :: TheItemGiverWithoutATargetPrintsItsUsage
#[test]
fn the_item_giver_without_a_target_prints_its_usage() {
    let mut rig = Rig::new();
    rig.ub("ig muledItems");
    assert_eq!(
        vec![
            "[UB] Error: Bad command syntax".to_owned(),
            "[UB] Usage: /ub ig[p] <lootProfile> to <target>".to_owned()
        ],
        rig.messages()[..2].to_vec()
    );
}

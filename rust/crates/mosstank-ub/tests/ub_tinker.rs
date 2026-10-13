//! Tinkering, against `TinkerJobManagerTests` and the logic tests of
//! `MossTankPanelTests.Tinker` (the page's state and the three commands;
//! the two markup tests are the panel's own).

mod common;

use ac_plugin_abi::character::{Skill, Training};
use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::types::ChatLine;
use ac_plugin_abi::world::{Confirmation, ObjectClass};
use common::Rig;
use mosstank_data::material::{self, Material};
use mosstank_data::tinker as calc;
use mosstank_ub::catalog::UbCatalog;
use mosstank_ub::commands::UbCall;
use mosstank_ub::tools::tinker::{
    self, GRANITE_IRON, TinkerJobManager, can_be_imbued, can_be_tinkered,
};
use mosstank_ub::tools::tinker_page::TinkerPage;

const WEAPON_TINKERING: u32 = 28;

fn weapon(id: u32, workmanship: f32, times_tinkered: i32) -> InventoryItem {
    InventoryItem {
        object_id: id,
        name: "Iron Long Sword".to_owned(),
        container_id: 1,
        stack_size: 1,
        damage_type: 1,
        damage: 10,
        damage_variance: 0.25,
        object_class: ObjectClass::MeleeWeapon,
        material_type: Material::Iron.id().cast_unsigned(),
        workmanship,
        salvage_workmanship: f64::from(workmanship),
        num_times_tinkered: times_tinkered,
        max_damage: 20,
        ..InventoryItem::default()
    }
}

fn bag(id: u32, kind: Material, workmanship: f64) -> InventoryItem {
    InventoryItem {
        object_id: id,
        name: format!("{} Salvage (100)", material::name(kind.id())),
        container_id: 1,
        stack_size: 1,
        structure: 100,
        maximum_structure: 100,
        object_class: ObjectClass::Salvage,
        material_type: kind.id().cast_unsigned(),
        salvage_workmanship: workmanship,
        ..InventoryItem::default()
    }
}

/// "Acdream" with Weapon Tinkering at `skill`.
fn tinker_rig(skill: u32) -> Rig {
    let mut rig = Rig::new();
    "Acdream".clone_into(&mut rig.host.meta.character_mut().name);
    set_skill(&mut rig, skill);
    rig
}

fn set_skill(rig: &mut Rig, current: u32) {
    rig.host.meta.skills = vec![Skill {
        skill_id: WEAPON_TINKERING,
        name: "Weapon Tinkering".to_owned(),
        training: Training::Trained,
        current,
        ..Skill::default()
    }];
}

fn items(rig: &mut Rig, items: Vec<InventoryItem>) {
    rig.host.meta.owned_items = items;
}

fn post(rig: &mut Rig, text: &str) {
    rig.host.meta.add_chat(ChatLine {
        text: text.to_owned(),
        ..ChatLine::default()
    });
}

fn manager(rig: &mut Rig) -> &mut TinkerJobManager {
    rig.commands.tools_mut().tinker.manager_mut()
}

fn with<R>(rig: &mut Rig, f: impl FnOnce(&mut TinkerJobManager, &mut UbCall<'_>) -> R) -> R {
    rig.with_call(|call, commands| f(commands.tools_mut().tinker.manager_mut(), call))
}

fn populate(rig: &mut Rig, item: u32, choice: &str, maximum: i32, minimum: f64) {
    with(rig, |tinker, call| {
        tinker.populate_tinker_list(call, item, choice, maximum, minimum);
    });
}

fn start(rig: &mut Rig) {
    with(rig, |tinker, call| tinker.start(call));
}

fn tick(rig: &mut Rig, seconds: f64) {
    with(rig, |tinker, call| tinker.on_tick(call, seconds));
}

fn confirm(rig: &mut Rig, context_id: u32, kind: i32, text: &str) {
    let confirmation = Confirmation {
        context_id,
        kind,
        text: text.to_owned(),
    };
    with(rig, |tinker, call| {
        tinker.on_confirmation(call, &confirmation)
    });
}

fn salvage_ids(rig: &mut Rig) -> Vec<u32> {
    manager(rig)
        .tinker_rows()
        .iter()
        .map(|row| row.salvage_object_id)
        .collect()
}

fn line(text: &str) -> String {
    format!("[UB] {text}")
}

/// A sword and the iron bags, with the run started on a plan of `maximum`.
fn running(bags: &[(u32, f64)], maximum: i32) -> Rig {
    let mut rig = tinker_rig(500);
    let mut owned = vec![weapon(0x100, 4.0, 0)];
    owned.extend(bags.iter().map(|(id, ws)| bag(*id, Material::Iron, *ws)));
    items(&mut rig, owned);
    populate(&mut rig, 0x100, "Iron", maximum, 0.0);
    start(&mut rig);
    rig
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: ThePlanSpendsTheCheapestBagsFirstAndNeverOneTwice
#[test]
fn the_plan_spends_the_cheapest_bags_first_and_never_one_twice() {
    let mut rig = tinker_rig(400);
    items(
        &mut rig,
        vec![
            weapon(0x100, 4.0, 0),
            bag(0x201, Material::Iron, 9.4),
            bag(0x202, Material::Iron, 2.1),
            bag(0x203, Material::Iron, 5.5),
        ],
    );

    populate(&mut rig, 0x100, "Iron", 3, 0.0);

    assert_eq!(vec![0x202, 0x203, 0x201], salvage_ids(&mut rig));
    let numbers: Vec<i32> = manager(&mut rig)
        .tinker_rows()
        .iter()
        .map(|row| row.number)
        .collect();
    assert_eq!(vec![1, 2, 3], numbers);
    assert_eq!(1, manager(&mut rig).jobs().len());
    assert_eq!(
        vec![0x202, 0x203, 0x201],
        manager(&mut rig).jobs()[0].salvage_to_apply
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: ABagThatDoesNotClearTheMinimumPercentIsNotQueued
#[test]
fn a_bag_that_does_not_clear_the_minimum_percent_is_not_queued() {
    let mut rig = tinker_rig(100);
    items(
        &mut rig,
        vec![weapon(0x100, 9.0, 0), bag(0x201, Material::Iron, 2.0)],
    );

    populate(&mut rig, 0x100, "Iron", 10, 99.5);

    assert!(manager(&mut rig).tinker_rows().is_empty());
    assert!(manager(&mut rig).jobs().is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: TheStopAtCountIsWhereTheRunStopsNotHowManyAttemptsItMakes
#[test]
fn the_stop_at_count_is_where_the_run_stops_not_how_many_attempts_it_makes() {
    let mut rig = tinker_rig(500);
    let mut owned = vec![weapon(0x100, 4.0, 1)];
    owned.extend((0x201..=0x210).map(|id| bag(id, Material::Iron, 2.0)));
    items(&mut rig, owned);

    populate(&mut rig, 0x100, "Iron", 4, 0.0);

    let numbers: Vec<i32> = manager(&mut rig)
        .tinker_rows()
        .iter()
        .map(|row| row.number)
        .collect();
    assert_eq!(vec![2, 3, 4], numbers);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: StartingTheRunSelectsTheFirstBagAndAppliesItToTheItem
#[test]
fn starting_the_run_selects_the_first_bag_and_applies_it_to_the_item() {
    let mut rig = running(&[(0x201, 2.0)], 1);

    assert_eq!(vec![(0x201, 0x100)], rig.host.applied);
    assert_eq!(Some(0x201), rig.host.meta.selection.selected);
    assert!(manager(&mut rig).is_running());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: TheCraftPromptIsAnsweredYesOnlyWhileAnAttemptIsInFlight
#[test]
fn the_craft_prompt_is_answered_yes_only_while_an_attempt_is_in_flight() {
    let mut rig = tinker_rig(500);
    items(
        &mut rig,
        vec![weapon(0x100, 4.0, 0), bag(0x201, Material::Iron, 2.0)],
    );

    confirm(&mut rig, 7, 5, "before");
    assert!(rig.host.answered.is_empty());

    populate(&mut rig, 0x100, "Iron", 1, 0.0);
    start(&mut rig);
    confirm(&mut rig, 9, 5, "You have a 98% chance of success.");

    assert_eq!(vec![(9, true)], rig.host.answered);
    assert!(
        rig.messages()
            .iter()
            .any(|text| text.starts_with("[UB] AutoTinker: Clicking Yes on"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: APromptThatIsNotTheCraftingOneIsLeftAlone
#[test]
fn a_prompt_that_is_not_the_crafting_one_is_left_alone() {
    let mut rig = running(&[(0x201, 2.0)], 1);
    confirm(&mut rig, 11, 2, "Swear allegiance?");
    assert!(rig.host.answered.is_empty());
}

const SUCCESS_2: &str = "Acdream successfully applies the Iron Salvage (100) (workmanship 2.00) to the Iron Long Sword.";

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: TheSuccessLineMarksTheRowAndMovesOnToTheNextBag
#[test]
fn the_success_line_marks_the_row_and_moves_on_to_the_next_bag() {
    let mut rig = running(&[(0x201, 2.0), (0x202, 3.0)], 2);

    post(&mut rig, SUCCESS_2);
    tick(&mut rig, 0.1);

    assert_eq!(vec![(0x201, 0x100), (0x202, 0x100)], rig.host.applied);
    assert_eq!(Some(true), manager(&mut rig).tinker_rows()[0].succeeded);
    assert_eq!(None, manager(&mut rig).tinker_rows()[1].succeeded);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: AFailureEndsTheJobOnThatItemRatherThanSpendingTheNextBag
#[test]
fn a_failure_ends_the_job_on_that_item_rather_than_spending_the_next_bag() {
    let mut rig = running(&[(0x201, 2.0), (0x202, 3.0)], 2);

    post(
        &mut rig,
        "Acdream fails to apply the Iron Salvage (100) (workmanship 2.00) to the Iron Long Sword. The salvage is destroyed.",
    );
    tick(&mut rig, 0.1);

    assert_eq!(vec![(0x201, 0x100)], rig.host.applied);
    assert_eq!(Some(false), manager(&mut rig).tinker_rows()[0].succeeded);
    assert!(!manager(&mut rig).is_running());
    assert!(rig.messages().contains(&line("Done tinkering")));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: ALineAboutSomebodyElsesTinkeringIsNotThisRunsAnswer
#[test]
fn a_line_about_somebody_elses_tinkering_is_not_this_runs_answer() {
    let mut rig = running(&[(0x201, 2.0), (0x202, 3.0)], 2);

    post(
        &mut rig,
        "Horan successfully applies the Iron Salvage (100) (workmanship 2.00) to the Iron Long Sword.",
    );
    post(
        &mut rig,
        "Acdream successfully applies the Iron Salvage (100) (workmanship 7.00) to the Iron Long Sword.",
    );
    tick(&mut rig, 0.1);

    assert_eq!(vec![(0x201, 0x100)], rig.host.applied);
    assert_eq!(None, manager(&mut rig).tinker_rows()[0].succeeded);
    assert!(manager(&mut rig).is_running());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: ASecondCopyOfTheSameLineDoesNotAnswerTheAttemptItJustStarted
#[test]
fn a_second_copy_of_the_same_line_does_not_answer_the_attempt_it_just_started() {
    let mut rig = running(&[(0x201, 2.0), (0x202, 2.0), (0x203, 2.0)], 3);

    post(&mut rig, SUCCESS_2);
    post(&mut rig, SUCCESS_2);
    tick(&mut rig, 0.1);

    assert_eq!(vec![(0x201, 0x100), (0x202, 0x100)], rig.host.applied);
    assert_eq!(Some(true), manager(&mut rig).tinker_rows()[0].succeeded);
    assert_eq!(None, manager(&mut rig).tinker_rows()[1].succeeded);
    assert!(manager(&mut rig).is_running());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: AnAttemptNobodyAnswersEndsTheRunRatherThanStrandingIt
#[test]
fn an_attempt_nobody_answers_ends_the_run_rather_than_stranding_it() {
    let mut rig = running(&[(0x201, 2.0)], 1);

    for _ in 0..30 {
        tick(&mut rig, 1.0);
    }

    assert!(!manager(&mut rig).is_running());
    assert!(
        rig.messages()
            .iter()
            .any(|text| text.contains("no word back"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: ASkillThatHasDroppedSinceThePlanStopsTheRun
#[test]
fn a_skill_that_has_dropped_since_the_plan_stops_the_run() {
    let mut rig = running(&[(0x201, 2.0), (0x202, 3.0)], 2);

    set_skill(&mut rig, 200);
    post(&mut rig, SUCCESS_2);
    tick(&mut rig, 0.1);

    assert_eq!(vec![(0x201, 0x100)], rig.host.applied);
    assert!(
        rig.messages()
            .contains(&line("tinkering skill decreased... stopping tinkering."))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: OnlyTheRightKindOfThingWithAttemptsLeftCanBeTinkered
#[test]
fn only_the_right_kind_of_thing_with_attempts_left_can_be_tinkered() {
    assert!(can_be_tinkered(&weapon(1, 5.0, 0)));
    assert!(!can_be_tinkered(&weapon(2, 5.0, 10)));
    assert!(!can_be_tinkered(&weapon(3, 0.0, 0)));
    assert!(!can_be_tinkered(&InventoryItem {
        object_class: ObjectClass::Food,
        ..weapon(4, 5.0, 0)
    }));

    let cap = InventoryItem {
        object_class: ObjectClass::Clothing,
        valid_locations: 1,
        armor_level: 40,
        ..weapon(5, 5.0, 0)
    };
    assert!(can_be_tinkered(&cap));
    assert!(!can_be_tinkered(&InventoryItem {
        armor_level: 0,
        ..cap.clone()
    }));
    assert!(!can_be_tinkered(&InventoryItem {
        valid_locations: 4,
        ..cap
    }));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: AnAlreadyImbuedItemCannotBeImbuedAgain
#[test]
fn an_already_imbued_item_cannot_be_imbued_again() {
    let sword = weapon(1, 5.0, 0);
    assert!(can_be_imbued(&sword));
    assert!(!can_be_imbued(&InventoryItem {
        imbued_effect: 1,
        ..sword.clone()
    }));
    assert!(!can_be_imbued(&InventoryItem {
        num_times_tinkered: 10,
        ..sword.clone()
    }));
    assert!(!can_be_imbued(&InventoryItem {
        workmanship: 0.0,
        ..sword
    }));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: ARendIsPlannedOnlyForWeaponsOfTheChosenDamageType
#[test]
fn a_rend_is_planned_only_for_weapons_of_the_chosen_damage_type() {
    let mut rig = tinker_rig(500);
    items(
        &mut rig,
        vec![
            weapon(0x100, 5.0, 0),
            InventoryItem {
                name: "Iron Dagger".to_owned(),
                damage_type: 8,
                ..weapon(0x101, 5.0, 0)
            },
            bag(0x201, Material::ImperialTopaz, 6.0),
            bag(0x202, Material::ImperialTopaz, 7.0),
        ],
    );

    with(&mut rig, |tinker, call| {
        tinker.populate_imbue_list(call, "Slashing", "Imperial Topaz");
    });

    let rows = manager(&mut rig).imbue_rows().to_vec();
    assert_eq!(1, rows.len());
    assert_eq!(0x100, rows[0].item_object_id);
    assert_eq!(0x201, rows[0].salvage_object_id);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: ARendIsPlannedForAWeaponThatHasAlreadyBeenTinkeredNineTimes
#[test]
fn a_rend_is_planned_for_a_weapon_that_has_already_been_tinkered_nine_times() {
    let mut rig = tinker_rig(500);
    items(
        &mut rig,
        vec![
            weapon(0x100, 5.0, 9),
            bag(0x201, Material::ImperialTopaz, 6.0),
        ],
    );

    with(&mut rig, |tinker, call| {
        tinker.populate_imbue_list(call, "Slashing", "Imperial Topaz");
    });

    let rows = manager(&mut rig).imbue_rows().to_vec();
    assert_eq!(1, rows.len());
    assert_eq!(0x100, rows[0].item_object_id);
    assert_eq!(0x201, rows[0].salvage_object_id);
    assert_eq!(1, manager(&mut rig).jobs().len());
    assert_eq!(vec![0x201], manager(&mut rig).jobs()[0].salvage_to_apply);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: RendAllMatchesEachWeaponsElementAndSkipsTheHopelessOnes
#[test]
fn rend_all_matches_each_weapons_element_and_skips_the_hopeless_ones() {
    let mut rig = tinker_rig(560);
    items(
        &mut rig,
        vec![
            InventoryItem {
                damage_type: 16,
                ..weapon(0x100, 5.0, 0)
            },
            InventoryItem {
                name: "Iron Dagger".to_owned(),
                damage_type: 8,
                ..weapon(0x101, 10.0, 0)
            },
            bag(0x201, Material::RedGarnet, 6.0),
            bag(0x202, Material::Aquamarine, 1.0),
        ],
    );

    with(&mut rig, |tinker, call| tinker.populate_rend_all(call));

    let rows = manager(&mut rig).imbue_rows().to_vec();
    assert_eq!(1, rows.len());
    assert_eq!(0x100, rows[0].item_object_id);
    assert_eq!(0x201, rows[0].salvage_object_id);
    assert!((0.30..=1.0).contains(&rows[0].success_chance));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: TheQueueReadsBackAsOneLinePerItemAndOnePerBagWaitingOnIt
#[test]
fn the_queue_reads_back_as_one_line_per_item_and_one_per_bag_waiting_on_it() {
    let mut rig = tinker_rig(500);
    items(
        &mut rig,
        vec![weapon(0x100, 4.0, 0), bag(0x201, Material::Iron, 2.0)],
    );

    assert_eq!(
        vec!["i'm out of jobs".to_owned()],
        manager(&mut rig).describe_jobs()
    );

    populate(&mut rig, 0x100, "Iron", 1, 0.0);

    assert_eq!(
        vec![
            "Target item: Iron Long Sword".to_owned(),
            "salvage: Iron Salvage (100) 513".to_owned()
        ],
        manager(&mut rig).describe_jobs()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: GraniteAndIronAreWeighedAfreshForEveryAttempt
#[test]
fn granite_and_iron_are_weighed_afresh_for_every_attempt() {
    let sword = InventoryItem {
        max_damage: 20,
        damage_variance: 0.5,
        ..weapon(0x100, 5.0, 0)
    };
    let (granite, iron, final_damage) = tinker::best_granite_iron(&sword);

    assert_eq!(10, granite + iron);
    assert!(granite > 0);
    assert!(iron > 0);
    assert!(final_damage > calc::damage_per_second(44.0, 0.5));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/TinkerJobManagerTests.cs :: TheDamageEstimateCountsTheBloodThirstCantripsAndTheAllowance
#[test]
fn the_damage_estimate_counts_the_blood_thirst_cantrips_and_the_allowance() {
    let plain = InventoryItem {
        max_damage: 50,
        ..weapon(1, 5.0, 0)
    };
    assert!((tinker::damage_ceiling(&plain) - 74.0).abs() < f64::EPSILON);

    let enchanted = InventoryItem {
        appraised_spell_ids: vec![6089, 2598],
        ..plain
    };
    assert!((tinker::damage_ceiling(&enchanted) - 86.0).abs() < f64::EPSILON);
}

// ---- MossTankPanelTests.Tinker ---------------------------------------------------

fn page<R>(rig: &mut Rig, f: impl FnOnce(&mut TinkerPage, &mut UbCall<'_>) -> R) -> R {
    rig.with_call(|call, commands| f(&mut commands.tools_mut().tinker, call))
}

fn ring(id: u32) -> InventoryItem {
    InventoryItem {
        object_id: id,
        name: "Gold Ring".to_owned(),
        container_id: 1,
        stack_size: 1,
        damage_variance: 0.25,
        object_class: ObjectClass::Jewelry,
        material_type: Material::Gold.id().cast_unsigned(),
        workmanship: 4.0,
        salvage_workmanship: 4.0,
        ..InventoryItem::default()
    }
}

/// The panel tests' sword: no damage type, variance a quarter.
fn panel_weapon(id: u32) -> InventoryItem {
    InventoryItem {
        damage_type: 0,
        damage: 0,
        ..weapon(id, 4.0, 0)
    }
}

/// The sword selected and added, iron chosen.
fn sword_on_the_page(rig: &mut Rig) {
    rig.host.meta.selection.selected = Some(0x100);
    page(rig, |page, call| {
        page.add_selected_item(call);
        page.select_salvage("Iron");
    });
}

fn the_tinker_page(rig: &mut Rig) -> &mut TinkerPage {
    &mut rig.commands.tools_mut().tinker
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Tinker.cs :: AddSelectedItemTakesTheSelectionAndNarrowsTheSalvageChoice
#[test]
fn add_selected_item_takes_the_selection_and_narrows_the_salvage_choice() {
    let mut rig = tinker_rig(0);
    items(&mut rig, vec![panel_weapon(0x100), ring(0x101)]);
    rig.host.meta.selection.selected = Some(0x100);

    page(&mut rig, TinkerPage::add_selected_item);

    assert_eq!("Iron Long Sword", the_tinker_page(&mut rig).item_name());
    let names = the_tinker_page(&mut rig).salvage_names().to_vec();
    assert!(names.contains(&GRANITE_IRON.to_owned()));
    assert!(names.contains(&"Iron".to_owned()));

    rig.host.meta.selection.selected = Some(0x101);
    page(&mut rig, TinkerPage::add_selected_item);

    assert_eq!("Gold Ring", the_tinker_page(&mut rig).item_name());
    assert_eq!(
        vec!["Gold", "Moonstone", "Linen", "Pine"],
        the_tinker_page(&mut rig).salvage_names()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Tinker.cs :: AnItemThatCannotBeTinkeredIsRefused
#[test]
fn an_item_that_cannot_be_tinkered_is_refused() {
    let mut rig = tinker_rig(0);
    items(
        &mut rig,
        vec![InventoryItem {
            object_class: ObjectClass::Food,
            name: "Bread".to_owned(),
            ..panel_weapon(0x100)
        }],
    );
    rig.host.meta.selection.selected = Some(0x100);

    page(&mut rig, TinkerPage::add_selected_item);

    assert_eq!("[None]", the_tinker_page(&mut rig).item_name());
    assert_eq!(
        "Item cannot be tinkered",
        the_tinker_page(&mut rig).notice()
    );
}

fn two_bags() -> Rig {
    let mut rig = tinker_rig(500);
    "Acdream".clone_into(&mut rig.host.meta.character_mut().name);
    items(
        &mut rig,
        vec![
            panel_weapon(0x100),
            bag(0x201, Material::Iron, 2.0),
            bag(0x202, Material::Iron, 3.0),
        ],
    );
    rig
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Tinker.cs :: PopulateFillsTheListAndStartAppliesItsFirstBag
#[test]
fn populate_fills_the_list_and_start_applies_its_first_bag() {
    let mut rig = two_bags();
    sword_on_the_page(&mut rig);
    page(&mut rig, |page, call| {
        page.select_max_tinks(call, "2");
        page.populate_tinker_list(call);
    });

    let tinker = the_tinker_page(&mut rig);
    assert_eq!(vec!["1", "2"], tinker.tinker_row_numbers());
    assert_eq!(
        vec!["Iron Long Sword", "Iron Long Sword"],
        tinker.tinker_row_items()
    );
    assert_eq!(vec!["Iron(2) ", "Iron(3) "], tinker.tinker_row_salvage());
    assert!(
        tinker
            .tinker_row_chances()
            .iter()
            .all(|text| text.trim().ends_with('%'))
    );

    page(&mut rig, TinkerPage::start_tinker);

    assert_eq!(vec![(0x201, 0x100)], rig.host.applied);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Tinker.cs :: TheStopAtChoiceWritesTheSettingAndDropsThePlanBuiltAgainstTheOldOne
#[test]
fn the_stop_at_choice_writes_the_setting_and_drops_the_plan_built_against_the_old_one() {
    let mut rig = two_bags();
    sword_on_the_page(&mut rig);
    page(&mut rig, |page, call| {
        page.select_max_tinks(call, "2");
        page.populate_tinker_list(call);
    });
    assert_eq!(2, the_tinker_page(&mut rig).tinker_row_numbers().len());

    page(&mut rig, |page, call| page.select_max_tinks(call, "1"));

    assert_eq!("1", TinkerPage::selected_max_tinks(&rig.catalog));
    assert_eq!(1, rig.catalog.value("AutoTinker.MaxTinks").as_int32());

    page(&mut rig, TinkerPage::populate_tinker_list);
    assert_eq!(1, the_tinker_page(&mut rig).tinker_row_numbers().len());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Tinker.cs :: TheMinimumPercentFieldWritesTheSettingInTheInvariantForm
#[test]
fn the_minimum_percent_field_writes_the_setting_in_the_invariant_form() {
    let mut rig = Rig::new();
    let text = rig
        .commands
        .tools_mut()
        .tinker
        .minimum_percent_text(&rig.catalog);
    assert_eq!("99.5", text);

    page(&mut rig, |page, call| {
        page.set_minimum_percent_text(call, "42.5")
    });

    let row = UbCatalog::require(&rig.catalog, "AutoTinker.MinPercentage");
    assert_eq!("42.5", UbCatalog::display(&rig.catalog, &row));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Tinker.cs :: ChoosingADamageTypeMovesTheSalvageChoiceToTheOneThatRendsIt
#[test]
fn choosing_a_damage_type_moves_the_salvage_choice_to_the_one_that_rends_it() {
    let mut rig = Rig::new();

    page(&mut rig, |page, call| {
        page.select_imbue_damage_type(call, "Fire")
    });
    assert_eq!(
        "Red Garnet",
        the_tinker_page(&mut rig).selected_imbue_salvage()
    );

    page(&mut rig, |page, call| {
        page.select_imbue_damage_type(call, "Electric")
    });
    assert_eq!("Jet", the_tinker_page(&mut rig).selected_imbue_salvage());

    let names = the_tinker_page(&mut rig).imbue_salvage_names().to_vec();
    assert_eq!(10, names.len());
    assert!(!names.contains(&"Iron".to_owned()));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Tinker.cs :: RendAllPlansOneRendPerWeaponMatchedToItsOwnElement
#[test]
fn rend_all_plans_one_rend_per_weapon_matched_to_its_own_element() {
    let mut rig = tinker_rig(600);
    items(
        &mut rig,
        vec![
            InventoryItem {
                damage_type: 16,
                ..panel_weapon(0x100)
            },
            InventoryItem {
                name: "Iron Dagger".to_owned(),
                damage_type: 8,
                ..panel_weapon(0x101)
            },
            bag(0x201, Material::RedGarnet, 5.0),
            bag(0x202, Material::Aquamarine, 5.0),
        ],
    );

    page(&mut rig, TinkerPage::rend_all);

    let tinker = the_tinker_page(&mut rig);
    assert_eq!(
        vec!["Iron Long Sword", "Iron Dagger"],
        tinker.imbue_row_items()
    );
    assert_eq!(
        vec!["Red Garnet(5) ", "Aquamarine(5) "],
        tinker.imbue_row_salvage()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Tinker.cs :: ARowsColourFollowsWhetherTheAttemptCameOff
#[test]
fn a_rows_colour_follows_whether_the_attempt_came_off() {
    let mut rig = two_bags();
    sword_on_the_page(&mut rig);
    page(&mut rig, |page, call| {
        page.populate_tinker_list(call);
        page.start_tinker(call);
    });

    assert_eq!(vec![(0x201, 0x100)], rig.host.applied);
    let before = the_tinker_page(&mut rig).tinker_row_colors();
    assert_eq!(before[0], before[1]);

    post(
        &mut rig,
        "Acdream fails to apply the Iron Salvage (100) (workmanship 2.00) to the Iron Long Sword. It is destroyed.",
    );
    tick(&mut rig, 0.1);

    let after = the_tinker_page(&mut rig).tinker_row_colors();
    assert_ne!(before[0], after[0]);
    assert_eq!(before[1], after[1]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Tinker.cs :: ClickingARowSelectsItsItemInTheClient
#[test]
fn clicking_a_row_selects_its_item_in_the_client() {
    let mut rig = tinker_rig(500);
    items(
        &mut rig,
        vec![panel_weapon(0x100), bag(0x201, Material::Iron, 2.0)],
    );
    sword_on_the_page(&mut rig);
    page(&mut rig, TinkerPage::populate_tinker_list);
    rig.host.meta.selection.selected = None;

    page(&mut rig, |page, call| page.select_tinker_row(call, 0));

    assert_eq!(Some(0x100), rig.host.meta.selection.selected);
    assert_eq!(0, the_tinker_page(&mut rig).selected_tinker_row_index());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Tinker.cs :: GetJobPrintsTheQueueAndSaysSoWhenThereIsNone
#[test]
fn get_job_prints_the_queue_and_says_so_when_there_is_none() {
    let mut rig = tinker_rig(500);
    items(
        &mut rig,
        vec![panel_weapon(0x100), bag(0x201, Material::Iron, 2.0)],
    );

    rig.ub("getjob");
    assert_eq!(vec![line("i'm out of jobs")], rig.messages());

    rig.clear_messages();
    sword_on_the_page(&mut rig);
    page(&mut rig, TinkerPage::populate_tinker_list);
    rig.clear_messages();
    rig.ub("getjob");

    let messages = rig.messages();
    assert_eq!(line("Target item: Iron Long Sword"), messages[0]);
    assert!(messages[1].starts_with("[UB] salvage: Iron Salvage (100)"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Tinker.cs :: AutoTinkerStartsThePlannedRunAndSaysSoWhenThereIsNothingToRun
#[test]
fn auto_tinker_starts_the_planned_run_and_says_so_when_there_is_nothing_to_run() {
    let mut rig = tinker_rig(500);
    items(
        &mut rig,
        vec![panel_weapon(0x100), bag(0x201, Material::Iron, 2.0)],
    );

    rig.ub("autotinker");
    assert_eq!(vec![line("i'm out of jobs")], rig.messages());
    assert!(rig.host.applied.is_empty());

    sword_on_the_page(&mut rig);
    page(&mut rig, TinkerPage::populate_tinker_list);
    rig.ub("autotinker");

    assert_eq!(vec![(0x201, 0x100)], rig.host.applied);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Tinker.cs :: TinkCalcPrintsTheOddsAndTheGraniteIronSplit
#[test]
fn tink_calc_prints_the_odds_and_the_granite_iron_split() {
    let mut rig = tinker_rig(500);
    items(
        &mut rig,
        vec![panel_weapon(0x100), bag(0x201, Material::Iron, 2.0)],
    );
    sword_on_the_page(&mut rig);
    rig.clear_messages();

    rig.ub("tinkcalc");

    let messages = rig.messages();
    assert!(
        messages[0].starts_with("[UB] 1: Applying ws 2 Iron"),
        "{messages:?}"
    );
    assert!(messages[0].contains("with a successChance of"));
    assert!(
        messages
            .iter()
            .any(|text| text.contains("Final max damage:")
                && text.contains("granite")
                && text.contains("iron"))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Tinker.cs :: TinkCalcWithNothingSelectedSaysSo
#[test]
fn tink_calc_with_nothing_selected_says_so() {
    let mut rig = Rig::new();
    rig.ub("tinkcalc");
    assert_eq!(vec![line("Nothing selected")], rig.messages());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Tinker.cs :: TheThreeTinkeringCommandsCarryTheirUsageLines
#[test]
fn the_three_tinkering_commands_carry_their_usage_lines() {
    let mut rig = Rig::new();
    for verb in ["autotinker", "getjob", "tinkcalc"] {
        rig.clear_messages();
        rig.ub(&format!("help {verb}"));
        assert_eq!(line(&format!("Usage: /ub {verb}")), rig.messages()[0]);
    }
}

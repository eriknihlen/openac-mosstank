//! The drain arm's planner.

use mosstank_combat::drain::select_spell;
use mosstank_formats::fixtures;
use mosstank_formats::gameinfo::VtankGameInfoDatabase;

const DRAIN_SPELLS: [u32; 3] = [1237, 1238, 1239];
const MARTYR_SPELLS: [u32; 3] = [2760, 2761, 2762];

fn select(
    health: i32,
    maximum_health: i32,
    health_floor: i32,
    target_health: i32,
    can_drain: bool,
    ring: bool,
    castable: &mut dyn FnMut(u32) -> bool,
) -> u32 {
    let game_info =
        VtankGameInfoDatabase::parse(&fixtures::read_vtank("gameinfodb-excerpt.ugd")).unwrap();
    select_spell(
        health,
        maximum_health,
        health_floor,
        target_health,
        can_drain,
        ring,
        &game_info.drain_spell_options,
        &game_info.martyr_spell_options,
        castable,
    )
}

fn any(_: u32) -> bool {
    true
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CorpseDrainPlanTests.cs :: TheGreedyStepIsTheMostHealthTakenOffPerMillisecondSpent
#[test]
fn the_greedy_step_is_the_most_health_taken_off_per_millisecond_spent() {
    // Every martyr would drop the caster under the floor, so the drains
    // compete on rate: 25/1200, 45/1800 and 70/2300. The last wins.
    assert_eq!(select(900, 1000, 749, 400, true, false, &mut any), 1239);
    // Take the winner away and the next-best rate takes it.
    assert_eq!(
        select(900, 1000, 749, 400, true, false, &mut |id| id != 1239),
        1238
    );
    // With the floor at 600 the martyrs join, and the fastest one wins.
    assert_eq!(select(900, 1000, 600, 400, true, false, &mut any), 2760);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CorpseDrainPlanTests.cs :: AMonsterNothingMagicalCanTouchIsNeverDrained
#[test]
fn a_monster_nothing_magical_can_touch_is_never_drained() {
    let chosen = select(900, 1000, 749, 400, false, false, &mut any);
    assert!(!DRAIN_SPELLS.contains(&chosen));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CorpseDrainPlanTests.cs :: AtFullHealthThereIsNothingToDrainIntoAndNothingToHeal
#[test]
fn at_full_health_there_is_nothing_to_drain_into_and_nothing_to_heal() {
    let chosen = select(1000, 1000, 749, 400, true, false, &mut any);
    assert!(MARTYR_SPELLS.contains(&chosen));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CorpseDrainPlanTests.cs :: ARingPlanMayOnlyUseTheRingMartyr
#[test]
fn a_ring_plan_may_only_use_the_ring_martyr() {
    assert_eq!(select(1000, 1000, 749, 400, true, true, &mut any), 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CorpseDrainPlanTests.cs :: APlanWhoseFirstStepIsAHealMeansDoNotDrain
#[test]
fn a_plan_whose_first_step_is_a_heal_means_do_not_drain() {
    assert_eq!(select(900, 1000, 749, 400, true, false, &mut |_| false), 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CorpseDrainPlanTests.cs :: ANearlyDeadMonsterGetsTheSearchAndAFinishingPlan
#[test]
fn a_nearly_dead_monster_gets_the_search_and_a_finishing_plan() {
    let chosen = select(900, 1000, 0, 20, true, false, &mut any);
    assert!(MARTYR_SPELLS.contains(&chosen));
}

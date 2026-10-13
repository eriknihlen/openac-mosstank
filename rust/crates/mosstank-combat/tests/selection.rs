//! Choosing the monster to fight.

use ac_plugin_abi::combat::CombatTarget;
use mosstank_combat::selection::{CombatTargetCandidate, select};
use mosstank_settings::model::{DebuffEachFirst, TargetSelectionMethod};
use mosstank_settings::monster::{MonsterRule, ResolvedMonsterRule};

#[derive(Default, Clone, Copy)]
struct C {
    priority: i32,
    distance: f64,
    angle: f64,
    urgency: i32,
    needs_debuff: bool,
    lock_selection: bool,
    last_target: bool,
    weapon: u32,
    offhand: u32,
}

#[allow(clippy::cast_possible_truncation)]
fn candidate(object_id: u32, c: C) -> CombatTargetCandidate {
    CombatTargetCandidate {
        target: CombatTarget {
            object_id,
            name: format!("Monster {object_id}"),
            weenie_class_id: object_id + 1000,
            distance: c.distance as f32,
            relative_angle_degrees: c.angle as f32,
            is_health_known: true,
            health_fraction: 1.0,
            seconds_since_health_update: f64::INFINITY,
            ..CombatTarget::default()
        },
        rule: ResolvedMonsterRule {
            rule: MonsterRule::with_priority("DEFAULT", c.priority).unwrap(),
            index: None,
            evaluation_error: None,
        },
        priority: c.priority,
        distance: c.distance,
        angle_delta: c.angle,
        urgency: c.urgency,
        needs_debuff: c.needs_debuff,
        is_lock_selection: c.lock_selection,
        is_last_target: c.last_target,
        planned_weapon: c.weapon,
        planned_offhand: c.offhand,
    }
}

fn pick(candidates: &[CombatTargetCandidate], method: TargetSelectionMethod) -> u32 {
    pick_with(candidates, method, DebuffEachFirst::ONE, 5.0, 0, 0)
}

fn pick_with(
    candidates: &[CombatTargetCandidate],
    method: TargetSelectionMethod,
    debuff_each_first: DebuffEachFirst,
    angle_range: f64,
    wielded_weapon: u32,
    wielded_offhand: u32,
) -> u32 {
    select(
        candidates,
        debuff_each_first,
        method,
        angle_range,
        wielded_weapon,
        wielded_offhand,
    )
    .object_id()
}

fn p(priority: i32, distance: f64) -> C {
    C {
        priority,
        distance,
        ..C::default()
    }
}

fn pa(priority: i32, distance: f64, angle: f64) -> C {
    C {
        angle,
        ..p(priority, distance)
    }
}

const RANGE: TargetSelectionMethod = TargetSelectionMethod::RANGE;

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatTargetSelectorTests.cs :: FirstValidCandidateBeatsTheEmptyBestSoFar
#[test]
fn first_valid_candidate_beats_the_empty_best_so_far() {
    assert_eq!(pick(&[candidate(10, pa(0, 12.0, 40.0))], RANGE), 10);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatTargetSelectorTests.cs :: PriorityWinsBeforeDistance
#[test]
fn priority_wins_before_distance() {
    let chosen = pick(
        &[candidate(10, p(1, 2.0)), candidate(20, p(4, 12.0))],
        RANGE,
    );
    assert_eq!(chosen, 20);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatTargetSelectorTests.cs :: DebuffEachFirstAllLetsDebuffNeedBeatAHigherPriority
#[test]
fn debuff_each_first_all_lets_debuff_need_beat_a_higher_priority() {
    let candidates = [
        candidate(10, p(4, 2.0)),
        candidate(
            20,
            C {
                needs_debuff: true,
                ..p(1, 12.0)
            },
        ),
    ];
    assert_eq!(
        pick_with(&candidates, RANGE, DebuffEachFirst::ALL, 5.0, 0, 0),
        20
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatTargetSelectorTests.cs :: DebuffEachFirstPriorityNeverCrossesAPriorityDifference
#[test]
fn debuff_each_first_priority_never_crosses_a_priority_difference() {
    let candidates = [
        candidate(10, p(4, 2.0)),
        candidate(
            20,
            C {
                needs_debuff: true,
                ..p(1, 12.0)
            },
        ),
    ];
    assert_eq!(
        pick_with(&candidates, RANGE, DebuffEachFirst::PRIORITY, 5.0, 0, 0),
        10
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatTargetSelectorTests.cs :: DebuffEachFirstPriorityBreaksATieOnDebuffNeed
#[test]
fn debuff_each_first_priority_breaks_a_tie_on_debuff_need() {
    let candidates = [
        candidate(10, p(2, 2.0)),
        candidate(
            20,
            C {
                needs_debuff: true,
                ..p(2, 12.0)
            },
        ),
    ];
    assert_eq!(
        pick_with(&candidates, RANGE, DebuffEachFirst::PRIORITY, 5.0, 0, 0),
        20
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatTargetSelectorTests.cs :: DebuffEachFirstOneSkipsTheDebuffNeedStepEntirely
#[test]
fn debuff_each_first_one_skips_the_debuff_need_step_entirely() {
    let candidates = [
        candidate(10, p(2, 2.0)),
        candidate(
            20,
            C {
                needs_debuff: true,
                ..p(2, 12.0)
            },
        ),
    ];
    assert_eq!(
        pick_with(&candidates, RANGE, DebuffEachFirst::ONE, 5.0, 0, 0),
        10
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatTargetSelectorTests.cs :: HigherUrgencyScoreWinsOutright
#[test]
fn higher_urgency_score_wins_outright() {
    let candidates = [
        candidate(10, p(2, 2.0)),
        candidate(
            20,
            C {
                urgency: 3,
                ..p(2, 12.0)
            },
        ),
    ];
    assert_eq!(pick(&candidates, RANGE), 20);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatTargetSelectorTests.cs :: TargetLockOnlyBreaksATieAndNeverBeatsPriorityOrUrgency
#[test]
fn target_lock_only_breaks_a_tie_and_never_beats_priority_or_urgency() {
    let locked = |c: C| C {
        lock_selection: true,
        ..c
    };
    assert_eq!(
        pick(
            &[candidate(10, locked(p(1, 2.0))), candidate(20, p(4, 12.0))],
            RANGE
        ),
        20
    );
    assert_eq!(
        pick(
            &[
                candidate(10, locked(p(2, 2.0))),
                candidate(
                    20,
                    C {
                        urgency: 1,
                        ..p(2, 12.0)
                    }
                ),
            ],
            RANGE
        ),
        20
    );
    assert_eq!(
        pick(
            &[candidate(10, p(2, 2.0)), candidate(20, locked(p(2, 12.0)))],
            RANGE
        ),
        20
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatTargetSelectorTests.cs :: WieldMatchPrefersFewerRewieldsButOnlyUnderTargetSelectMethodBoth
#[test]
fn wield_match_prefers_fewer_rewields_but_only_under_target_select_method_both() {
    let candidates = [
        candidate(
            10,
            C {
                weapon: 7,
                offhand: 8,
                ..pa(2, 2.0, 1.0)
            },
        ),
        candidate(
            20,
            C {
                weapon: 5,
                offhand: 6,
                ..pa(2, 3.0, 9.0)
            },
        ),
    ];
    assert_eq!(
        pick_with(
            &candidates,
            TargetSelectionMethod::BOTH,
            DebuffEachFirst::ONE,
            10.0,
            5,
            6
        ),
        20
    );
    assert_eq!(
        pick_with(&candidates, RANGE, DebuffEachFirst::ONE, 10.0, 5, 6),
        10
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatTargetSelectorTests.cs :: StickyPreviousTargetHoldsAgainstAnEqualNewcomer
#[test]
fn sticky_previous_target_holds_against_an_equal_newcomer() {
    let candidates = [
        candidate(10, p(2, 2.0)),
        candidate(
            20,
            C {
                last_target: true,
                ..p(2, 12.0)
            },
        ),
    ];
    assert_eq!(pick(&candidates, RANGE), 20);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatTargetSelectorTests.cs :: StickyLastTargetStillLosesToAHigherPriority
#[test]
fn sticky_last_target_still_loses_to_a_higher_priority() {
    let candidates = [
        candidate(10, p(4, 12.0)),
        candidate(
            20,
            C {
                last_target: true,
                ..p(2, 2.0)
            },
        ),
    ];
    assert_eq!(pick(&candidates, RANGE), 10);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatTargetSelectorTests.cs :: SelectionMethodDistancePrefersNearerThenAngle
#[test]
fn selection_method_distance_prefers_nearer_then_angle() {
    assert_eq!(
        pick(
            &[
                candidate(10, pa(2, 12.0, 1.0)),
                candidate(20, pa(2, 2.0, 40.0))
            ],
            RANGE
        ),
        20
    );
    assert_eq!(
        pick(
            &[
                candidate(10, pa(2, 5.0, 40.0)),
                candidate(20, pa(2, 5.0, 1.0))
            ],
            RANGE
        ),
        20
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatTargetSelectorTests.cs :: SelectionMethodAnglePrefersStraighterThenDistance
#[test]
fn selection_method_angle_prefers_straighter_then_distance() {
    let angle = TargetSelectionMethod::ANGLE;
    assert_eq!(
        pick(
            &[
                candidate(10, pa(2, 2.0, 40.0)),
                candidate(20, pa(2, 12.0, 1.0))
            ],
            angle
        ),
        20
    );
    assert_eq!(
        pick(
            &[
                candidate(10, pa(2, 12.0, 5.0)),
                candidate(20, pa(2, 2.0, 5.0))
            ],
            angle
        ),
        20
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatTargetSelectorTests.cs :: SelectionMethodBothPrefersInsideTheCutoffRegardlessOfAngle
#[test]
fn selection_method_both_prefers_inside_the_cutoff_regardless_of_angle() {
    let both = |candidates: &[CombatTargetCandidate]| {
        pick_with(
            candidates,
            TargetSelectionMethod::BOTH,
            DebuffEachFirst::ONE,
            10.0,
            0,
            0,
        )
    };
    assert_eq!(
        both(&[
            candidate(10, pa(2, 40.0, 1.0)),
            candidate(20, pa(2, 3.0, 80.0))
        ]),
        20
    );
    assert_eq!(
        both(&[
            candidate(10, pa(2, 3.0, 80.0)),
            candidate(20, pa(2, 8.0, 5.0))
        ]),
        20
    );
    assert_eq!(
        both(&[
            candidate(10, pa(2, 40.0, 1.0)),
            candidate(20, pa(2, 20.0, 70.0))
        ]),
        20
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatTargetSelectorTests.cs :: AnExactTieKeepsTheIncumbent
#[test]
fn an_exact_tie_keeps_the_incumbent() {
    assert_eq!(
        pick(
            &[
                candidate(10, pa(2, 5.0, 5.0)),
                candidate(20, pa(2, 5.0, 5.0))
            ],
            RANGE
        ),
        10
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatTargetSelectorTests.cs :: NoCandidatesYieldsTheEmptyBest
#[test]
fn no_candidates_yields_the_empty_best() {
    assert_eq!(pick(&[], RANGE), 0);
}

//! Which monster to fight (`CombatTargetSelector`): the best candidate by
//! priority, the debuff-first mode, urgency, the locked selection, the
//! fewest re-wields, the previous target, then the selection method.

use ac_plugin_abi::combat::CombatTarget;
use mosstank_settings::model::{DebuffEachFirst, TargetSelectionMethod};
use mosstank_settings::monster::{MonsterRule, ResolvedMonsterRule};

/// One monster the pass could fight.
#[derive(Debug, Clone)]
pub struct CombatTargetCandidate {
    /// The monster.
    pub target: CombatTarget,
    /// Its rule.
    pub rule: ResolvedMonsterRule,
    /// Its priority.
    pub priority: i32,
    /// How far, metres.
    pub distance: f64,
    /// How far off the character's facing, degrees.
    pub angle_delta: f64,
    /// How badly it needs debuffing.
    pub urgency: i32,
    /// Whether a debuff is owed.
    pub needs_debuff: bool,
    /// Whether it is the locked selection.
    pub is_lock_selection: bool,
    /// Whether it was the previous pass's target.
    pub is_last_target: bool,
    /// The weapon the fight would wield for it.
    pub planned_weapon: u32,
    /// The off-hand item the fight would wield for it.
    pub planned_offhand: u32,
}

impl CombatTargetCandidate {
    /// The monster's id.
    #[must_use]
    pub fn object_id(&self) -> u32 {
        self.target.object_id
    }

    /// The empty best: priority -1, nothing near.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            target: CombatTarget::default(),
            rule: ResolvedMonsterRule {
                rule: MonsterRule::authentic_default(),
                index: None,
                evaluation_error: None,
            },
            priority: -1,
            distance: 999_999.0,
            angle_delta: f64::MAX,
            urgency: 0,
            needs_debuff: false,
            is_lock_selection: false,
            is_last_target: false,
            planned_weapon: 0,
            planned_offhand: 0,
        }
    }
}

/// The best candidate (the empty one when there is none).
#[must_use]
pub fn select(
    candidates: &[CombatTargetCandidate],
    debuff_each_first: DebuffEachFirst,
    method: TargetSelectionMethod,
    target_select_angle_range: f64,
    wielded_weapon: u32,
    wielded_offhand: u32,
) -> CombatTargetCandidate {
    // One mode asks "needs a debuff" as a tiebreak, the other lets it
    // outrank priority.
    let flag =
        debuff_each_first == DebuffEachFirst::ALL || debuff_each_first == DebuffEachFirst::PRIORITY;
    let flag2 = debuff_each_first == DebuffEachFirst::ALL;
    let num4 = if method == TargetSelectionMethod::BOTH {
        target_select_angle_range
    } else {
        0.0
    };
    let mut best: Option<&CombatTargetCandidate> = None;
    let empty = CombatTargetCandidate::empty();
    for candidate in candidates {
        let incumbent = best.unwrap_or(&empty);
        if accepts(
            candidate,
            incumbent,
            flag,
            flag2,
            method,
            num4,
            wielded_weapon,
            wielded_offhand,
        ) {
            best = Some(candidate);
        }
    }
    best.cloned().unwrap_or(empty)
}

#[allow(clippy::too_many_arguments)]
fn accepts(
    f10: &CombatTargetCandidate,
    best: &CombatTargetCandidate,
    flag: bool,
    flag2: bool,
    method: TargetSelectionMethod,
    num4: f64,
    wielded_weapon: u32,
    wielded_offhand: u32,
) -> bool {
    if !flag2 {
        if f10.priority <= best.priority {
            if f10.priority < best.priority {
                return false;
            }
            return tie_break(
                f10,
                best,
                flag,
                method,
                num4,
                wielded_weapon,
                wielded_offhand,
            );
        }
        return true;
    }
    // "All": needing a debuff outranks priority itself.
    if !f10.needs_debuff || best.needs_debuff {
        if !f10.needs_debuff && best.needs_debuff {
            return false;
        }
        if f10.priority <= best.priority {
            if f10.priority < best.priority {
                return false;
            }
            return tie_break(
                f10,
                best,
                flag,
                method,
                num4,
                wielded_weapon,
                wielded_offhand,
            );
        }
        return true;
    }
    true
}

#[allow(clippy::too_many_arguments, clippy::float_cmp)]
fn tie_break(
    f10: &CombatTargetCandidate,
    best: &CombatTargetCandidate,
    flag: bool,
    method: TargetSelectionMethod,
    num4: f64,
    wielded_weapon: u32,
    wielded_offhand: u32,
) -> bool {
    if flag {
        if f10.needs_debuff && !best.needs_debuff {
            return true;
        }
        if !f10.needs_debuff && best.needs_debuff {
            return false;
        }
    }

    if f10.urgency > best.urgency {
        return true;
    }
    if f10.urgency < best.urgency {
        return false;
    }

    if f10.is_lock_selection && !best.is_lock_selection {
        return true;
    }
    if !f10.is_lock_selection && best.is_lock_selection {
        return false;
    }

    if f10.distance < num4 && best.distance < num4 {
        let rewields = |candidate: &CombatTargetCandidate| {
            i32::from(candidate.planned_weapon != wielded_weapon)
                + i32::from(candidate.planned_offhand != wielded_offhand)
        };
        let num5 = rewields(f10);
        let num6 = rewields(best);
        if num5 < num6 {
            return true;
        }
        if num5 > num6 {
            return false;
        }
    }

    if f10.is_last_target && !best.is_last_target {
        return true;
    }
    if !f10.is_last_target && best.is_last_target {
        return false;
    }

    if method == TargetSelectionMethod::BOTH {
        if f10.distance < num4 && best.distance > num4 {
            return true;
        }
        if f10.distance > num4 && best.distance < num4 {
            return false;
        }
        if f10.distance < num4 {
            if f10.angle_delta < best.angle_delta {
                return true;
            }
            if f10.angle_delta > best.angle_delta {
                return false;
            }
            return f10.distance < best.distance;
        }
        if f10.distance < best.distance {
            return true;
        }
        if f10.distance > best.distance {
            return false;
        }
        f10.angle_delta < best.angle_delta
    } else if method == TargetSelectionMethod::RANGE {
        if f10.distance < best.distance {
            return true;
        }
        if f10.distance > best.distance {
            return false;
        }
        f10.angle_delta < best.angle_delta
    } else if method == TargetSelectionMethod::ANGLE {
        if f10.angle_delta < best.angle_delta {
            return true;
        }
        if f10.angle_delta > best.angle_delta {
            return false;
        }
        f10.distance < best.distance
    } else {
        // An unknown method keeps the incumbent.
        false
    }
}

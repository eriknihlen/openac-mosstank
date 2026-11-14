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

/// Why the target choice kept or left what it was on, as the team's
/// debuffs had it (`TeamFocusDebuffed`), for `/vt team`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusDecision {
    /// Focus is off: the usual order chose.
    Off,
    /// Nothing to fight.
    NoTarget,
    /// The player's locked selection, which focus does not override.
    Locked(String),
    /// Stayed on a monster whose team debuff still holds.
    Kept(String),
    /// The usual order chose a monster the team debuffed.
    Took(String),
    /// Left a monster the team had not debuffed for one it had.
    Switched {
        /// The monster the usual order chose.
        from: String,
        /// The debuffed monster taken instead.
        to: String,
    },
    /// Nothing in range carries a team debuff: the usual order chose.
    NoneDebuffed(String),
}

impl std::fmt::Display for FocusDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => f.write_str("focus on debuffed monsters is off"),
            Self::NoTarget => f.write_str("nothing to fight"),
            Self::Locked(name) => write!(f, "{name}, the locked selection"),
            Self::Kept(name) => write!(f, "staying on {name}, whose team debuff holds"),
            Self::Took(name) => write!(f, "{name}, debuffed by the team"),
            Self::Switched { from, to } => {
                write!(
                    f,
                    "left {from} (not debuffed) for {to}, debuffed by the team"
                )
            }
            Self::NoneDebuffed(name) => write!(
                f,
                "nothing in range debuffed by the team; {name} by the usual order"
            ),
        }
    }
}

/// The focus on what the team debuffed, over the usual choice `chosen`
/// among `candidates` (`order` is the usual order; `debuffed` says whether
/// a team debuff is in force on a monster; `current` is the monster being
/// fought).
///
/// Only one move is ever made: from a monster with no team debuff to one
/// with. A current target that is still a candidate and still carries a
/// team debuff is kept, whatever else appears (a higher priority, a nearer
/// or a more debuffed monster), so the fight never flip-flops. The usual
/// order picks among several debuffed monsters. The player's locked
/// selection is never overridden.
pub fn focus_debuffed(
    candidates: &[CombatTargetCandidate],
    chosen: CombatTargetCandidate,
    current: u32,
    debuffed: impl Fn(u32) -> bool,
    order: impl Fn(&[CombatTargetCandidate]) -> CombatTargetCandidate,
) -> (CombatTargetCandidate, FocusDecision) {
    if chosen.object_id() == 0 {
        return (chosen, FocusDecision::NoTarget);
    }
    if chosen.is_lock_selection {
        let name = chosen.target.name.clone();
        return (chosen, FocusDecision::Locked(name));
    }
    if current != 0
        && debuffed(current)
        && let Some(kept) = candidates
            .iter()
            .find(|candidate| candidate.object_id() == current)
    {
        return (kept.clone(), FocusDecision::Kept(kept.target.name.clone()));
    }
    if debuffed(chosen.object_id()) {
        let name = chosen.target.name.clone();
        return (chosen, FocusDecision::Took(name));
    }
    let marked: Vec<CombatTargetCandidate> = candidates
        .iter()
        .filter(|candidate| debuffed(candidate.object_id()))
        .cloned()
        .collect();
    if marked.is_empty() {
        let name = chosen.target.name.clone();
        return (chosen, FocusDecision::NoneDebuffed(name));
    }
    let taken = order(&marked);
    let decision = FocusDecision::Switched {
        from: chosen.target.name.clone(),
        to: taken.target.name.clone(),
    };
    (taken, decision)
}

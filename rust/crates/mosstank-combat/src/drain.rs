//! The drain arm's planner (`CorpseDrainPlan`): the first step of the
//! cheapest sequence of drains, martyrs and self-heals that finishes the
//! monster without dropping the caster below a health floor. A bounded
//! depth-first search under a widening time allowance, else a greedy pick.

use mosstank_formats::gameinfo::{VtankDrainSpellOption, VtankMartyrSpellOption};

const CAST_OVERHEAD_MILLISECONDS: i32 = 800;
const HEAL_CAST_MILLISECONDS: i32 = 2450;
const HEAL_POINTS: i32 = 115;
const RING_MARTYR_SPELL_ID: u32 = 3818;
const DRAIN_FLOOR_POINTS: i32 = 25;
const BUDGET_STEP_MILLISECONDS: i32 = 600;
const NODE_BUDGET: i32 = 2000;
const SEARCH_TARGET_HEALTH_CEILING: i32 = 300;
const MARTYR_BIAS: f64 = 180.0;
const MARTYR_BIAS_COST: f64 = 3200.0;

#[derive(Debug, Clone, Copy)]
enum Kind {
    Drain {
        enemy_factor: f64,
        result_multiplier: f64,
        maximum_points: i32,
    },
    Martyr {
        self_factor: f64,
        result_multiplier: f64,
    },
    Heal,
}

#[derive(Debug, Clone, Copy)]
struct Step {
    spell_id: u32,
    cost_milliseconds: i32,
    kind: Kind,
}

impl Step {
    fn heal() -> Self {
        Self {
            spell_id: 0,
            cost_milliseconds: HEAL_CAST_MILLISECONDS + CAST_OVERHEAD_MILLISECONDS,
            kind: Kind::Heal,
        }
    }

    fn is_drain(&self) -> bool {
        matches!(self.kind, Kind::Drain { .. })
    }

    fn is_heal(&self) -> bool {
        matches!(self.kind, Kind::Heal)
    }

    fn is_martyr(&self) -> bool {
        matches!(self.kind, Kind::Martyr { .. })
    }

    #[allow(clippy::cast_possible_truncation)]
    fn apply(&self, state: State) -> State {
        let mut result = state;
        match self.kind {
            Kind::Drain {
                enemy_factor,
                result_multiplier,
                maximum_points,
            } => {
                let mut drained =
                    (f64::from(state.target_health) * enemy_factor).round_ties_even() as i32;
                drained = drained.min(state.target_health).min(maximum_points);
                let mut gained = (f64::from(drained) * result_multiplier).round_ties_even() as i32;
                if gained > state.deficit() {
                    // Overhealing is wasted: the drain is trimmed back to what
                    // the caster can still take.
                    drained -= (f64::from(gained - state.deficit()) / result_multiplier)
                        .round_ties_even() as i32;
                    if drained < 0 {
                        drained = 0;
                    }
                    gained = state.deficit();
                }
                result.health += gained;
                result.target_health -= drained;
            }
            Kind::Martyr {
                self_factor,
                result_multiplier,
            } => {
                let spent = (f64::from(result.health) * self_factor).round_ties_even() as i32;
                result.health -= spent;
                result.target_health -=
                    (f64::from(spent) * result_multiplier).round_ties_even() as i32;
                if result.target_health < 0 {
                    result.target_health = 0;
                }
            }
            Kind::Heal => {
                result.health = (result.health + HEAL_POINTS).min(result.maximum_health);
            }
        }
        result.cost_milliseconds += self.cost_milliseconds;
        result
    }
}

#[derive(Debug, Clone, Copy)]
struct State {
    target_health: i32,
    health: i32,
    maximum_health: i32,
    cost_milliseconds: i32,
}

impl State {
    fn deficit(&self) -> i32 {
        self.maximum_health - self.health
    }

    fn done(&self) -> bool {
        self.target_health <= 0
    }
}

#[derive(Debug, Clone)]
struct Node {
    steps: Vec<Step>,
    state: State,
}

impl Node {
    fn apply(&self, step: &Step) -> Self {
        let mut steps = Vec::with_capacity(self.steps.len() + 1);
        steps.extend_from_slice(&self.steps);
        steps.push(*step);
        Self {
            steps,
            state: step.apply(self.state),
        }
    }

    fn cost(&self) -> i32 {
        i32::try_from(self.steps.len()).unwrap_or(i32::MAX) + self.state.cost_milliseconds
    }

    fn score(&self) -> i32 {
        self.state.cost_milliseconds + (1000 * self.state.target_health / 106)
    }
}

/// The spell the drain arm casts now, 0 for "heal instead" or nothing.
#[allow(clippy::too_many_arguments)]
pub fn select_spell(
    health: i32,
    maximum_health: i32,
    health_floor: i32,
    target_health: i32,
    can_drain: bool,
    ring: bool,
    drains: &[VtankDrainSpellOption],
    martyrs: &[VtankMartyrSpellOption],
    castable: &mut dyn FnMut(u32) -> bool,
) -> u32 {
    let mut steps = Vec::new();
    if can_drain {
        for option in drains {
            if castable(option.spell_id) {
                steps.push(Step {
                    spell_id: option.spell_id,
                    cost_milliseconds: option.cast_time_milliseconds + CAST_OVERHEAD_MILLISECONDS,
                    kind: Kind::Drain {
                        enemy_factor: option.enemy_drain_factor,
                        result_multiplier: option.result_multiplier,
                        maximum_points: option.enemy_drain_maximum_points,
                    },
                });
            }
        }
    }
    for option in martyrs {
        let is_ring_martyr = option.spell_id == RING_MARTYR_SPELL_ID;
        if ring != is_ring_martyr || !castable(option.spell_id) {
            continue;
        }
        steps.push(Step {
            spell_id: option.spell_id,
            cost_milliseconds: option.cast_time_milliseconds + CAST_OVERHEAD_MILLISECONDS,
            kind: Kind::Martyr {
                self_factor: option.self_drain_factor,
                result_multiplier: option.result_multiplier,
            },
        });
    }
    steps.push(Step::heal());

    let start = Node {
        steps: Vec::new(),
        state: State {
            target_health,
            health,
            maximum_health,
            cost_milliseconds: 0,
        },
    };
    let best = search(&start, health_floor, &steps);
    match best.steps.first() {
        None => 0,
        Some(first) if first.is_heal() => 0,
        Some(first) => first.spell_id,
    }
}

fn search(start: &Node, health_floor: i32, steps: &[Step]) -> Node {
    let mut best: Option<Node> = None;
    let mut budget = 0;
    let mut visited = 0;
    if start.state.target_health < SEARCH_TARGET_HEALTH_CEILING {
        loop {
            // Widen the allowance and search again, so the first complete
            // plan found is also a cheap one.
            budget += BUDGET_STEP_MILLISECONDS;
            let mut stack = vec![Node {
                steps: Vec::new(),
                state: State {
                    cost_milliseconds: 0,
                    ..start.state
                },
            }];
            while let Some(node) = stack.pop() {
                visited += 1;
                if visited >= NODE_BUDGET {
                    return best.unwrap_or_else(|| greedy(start, health_floor, steps));
                }
                let mut any_within_floor = false;
                for step in steps {
                    if skip(&node.state, step) {
                        continue;
                    }
                    let next = node.apply(step);
                    if next.score() > budget || next.state.health < health_floor {
                        continue;
                    }
                    any_within_floor = true;
                    consider(next, &mut best, &mut stack);
                }
                if any_within_floor {
                    continue;
                }
                // Nothing keeps the caster above the floor from here: the
                // floor is dropped for this node.
                for step in steps {
                    if skip(&node.state, step) {
                        continue;
                    }
                    let next = node.apply(step);
                    if next.score() > budget {
                        continue;
                    }
                    consider(next, &mut best, &mut stack);
                }
            }
            if best.is_some() {
                break;
            }
        }
    }
    best.unwrap_or_else(|| greedy(start, health_floor, steps))
}

fn consider(next: Node, best: &mut Option<Node>, stack: &mut Vec<Node>) {
    if !next.state.done() {
        stack.push(next);
        return;
    }
    if best
        .as_ref()
        .is_none_or(|current| next.cost() < current.cost())
    {
        *best = Some(next);
    }
}

fn greedy(start: &Node, health_floor: i32, steps: &[Step]) -> Node {
    let mut chosen: Option<Node> = None;
    let mut best_rate = f64::MIN;
    let mut any_within_floor = false;
    for step in steps {
        if skip(&start.state, step) {
            continue;
        }
        let next = start.apply(step);
        if next.state.health < health_floor {
            continue;
        }
        any_within_floor = true;
        rank(start, next, step, &mut chosen, &mut best_rate);
    }
    if !any_within_floor {
        for step in steps {
            if skip(&start.state, step) {
                continue;
            }
            rank(start, start.apply(step), step, &mut chosen, &mut best_rate);
        }
    }
    chosen.unwrap_or_else(|| start.apply(&Step::heal()))
}

fn rank(start: &Node, next: Node, step: &Step, chosen: &mut Option<Node>, best_rate: &mut f64) {
    let Some(current) = chosen.as_ref() else {
        *chosen = Some(next);
        *best_rate = 0.0;
        return;
    };
    if !current.state.done() {
        if next.state.done() {
            *chosen = Some(next);
            return;
        }
        let (bias, bias_cost) = if step.is_martyr() {
            (MARTYR_BIAS, MARTYR_BIAS_COST)
        } else {
            (0.0, 0.0)
        };
        let rate = (bias + f64::from(start.state.target_health)
            - f64::from(next.state.target_health))
            / (bias_cost + f64::from(next.state.cost_milliseconds)
                - f64::from(start.state.cost_milliseconds));
        if rate > *best_rate {
            *best_rate = rate;
            *chosen = Some(next);
        }
        return;
    }
    if next.state.done() && next.state.cost_milliseconds < current.state.cost_milliseconds {
        *chosen = Some(next);
    }
}

fn skip(state: &State, step: &Step) -> bool {
    (state.health == state.maximum_health && (step.is_drain() || step.is_heal()))
        || (step.is_drain() && state.target_health < DRAIN_FLOOR_POINTS)
}

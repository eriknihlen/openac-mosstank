//! Spending banked experience by a weight policy: the forty-seven targets,
//! the four cost tables, the policy's text forms, and the planner.

use std::sync::OnceLock;

use ac_plugin_abi::character::{AdvancementKind, Training};

/// Which pool a target belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExperienceTargetKind {
    /// An attribute.
    Attribute,
    /// A vital.
    Vital,
    /// A skill.
    Skill,
}

/// One thing experience can be spent on. `stat_id` is the number a spend
/// names it by: the attribute's number, the vital's, or the skill's id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExperienceTarget {
    /// The name a policy uses.
    pub name: &'static str,
    /// Its pool.
    pub kind: ExperienceTargetKind,
    /// Its number.
    pub stat_id: u32,
}

impl ExperienceTarget {
    /// The kind of advancement a spend on it is.
    #[must_use]
    pub fn advancement_kind(&self) -> AdvancementKind {
        match self.kind {
            ExperienceTargetKind::Attribute => AdvancementKind::Attribute,
            ExperienceTargetKind::Vital => AdvancementKind::Vital,
            ExperienceTargetKind::Skill => AdvancementKind::Skill,
        }
    }
}

const fn target(name: &'static str, kind: ExperienceTargetKind, stat_id: u32) -> ExperienceTarget {
    ExperienceTarget {
        name,
        kind,
        stat_id,
    }
}

use ExperienceTargetKind::{Attribute, Skill, Vital};

/// The forty-seven targets: six attributes, three vitals, thirty-eight
/// skills.
pub const TARGETS: [ExperienceTarget; 47] = [
    target("Strength", Attribute, 1),
    target("Endurance", Attribute, 2),
    target("Quickness", Attribute, 3),
    target("Coordination", Attribute, 4),
    target("Focus", Attribute, 5),
    target("Self", Attribute, 6),
    target("Health", Vital, 1),
    target("Stamina", Vital, 3),
    target("Mana", Vital, 5),
    target("MeleeDefense", Skill, 6),
    target("MissileDefense", Skill, 7),
    target("ArcaneLore", Skill, 14),
    target("MagicDefense", Skill, 15),
    target("ManaConversion", Skill, 16),
    target("ItemTinkering", Skill, 18),
    target("AssessPerson", Skill, 19),
    target("Deception", Skill, 20),
    target("Healing", Skill, 21),
    target("Jump", Skill, 22),
    target("Lockpick", Skill, 23),
    target("Run", Skill, 24),
    target("AssessCreature", Skill, 27),
    target("WeaponTinkering", Skill, 28),
    target("ArmorTinkering", Skill, 29),
    target("MagicItemTinkering", Skill, 30),
    target("CreatureEnchantment", Skill, 31),
    target("ItemEnchantment", Skill, 32),
    target("LifeMagic", Skill, 33),
    target("WarMagic", Skill, 34),
    target("Leadership", Skill, 35),
    target("Loyalty", Skill, 36),
    target("Fletching", Skill, 37),
    target("Alchemy", Skill, 38),
    target("Cooking", Skill, 39),
    target("Salvaging", Skill, 40),
    target("TwoHandedCombat", Skill, 41),
    target("VoidMagic", Skill, 43),
    target("HeavyWeapons", Skill, 44),
    target("LightWeapons", Skill, 45),
    target("FinesseWeapons", Skill, 46),
    target("MissileWeapons", Skill, 47),
    target("Shield", Skill, 48),
    target("DualWield", Skill, 49),
    target("Recklessness", Skill, 50),
    target("SneakAttack", Skill, 51),
    target("DirtyFighting", Skill, 52),
    target("Summoning", Skill, 54),
];

/// The target with this name, ignoring case and surrounding space.
#[must_use]
pub fn target_named(name: &str) -> Option<ExperienceTarget> {
    let name = name.trim();
    TARGETS
        .iter()
        .find(|target| target.name.eq_ignore_ascii_case(name))
        .copied()
}

/// Where one target stands: how many times it has been raised and, for a
/// skill, how far it is trained (which picks the cost table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExperienceTargetState {
    /// The target.
    pub target: ExperienceTarget,
    /// Ranks bought so far.
    pub ranks: i32,
    /// The skill's training (ignored for attributes and vitals).
    pub training: Training,
}

/// The four cost tables: what the next rank costs, row by rank.
#[derive(Debug)]
pub struct ExperienceCostTables {
    /// Attributes.
    pub attribute: Vec<i64>,
    /// Vitals.
    pub vital: Vec<i64>,
    /// Trained skills.
    pub trained: Vec<i64>,
    /// Specialized skills.
    pub specialized: Vec<i64>,
}

/// The tables as the plugin ships them (the C# plugin's embedded file).
const COSTS: &str = include_str!("../../../../src/AcDream.Plugins.MossTank/ExperienceCosts.tsv");

fn read_tables(text: &str) -> Result<ExperienceCostTables, String> {
    let mut tables = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let mut cells = line.split('\t');
        let name = cells.next().unwrap_or_default();
        let costs = cells
            .filter(|cell| !cell.is_empty())
            .map(|cell| {
                cell.parse::<i64>()
                    .map_err(|error| format!("{name}: {cell}: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        tables.insert(name.to_owned(), costs);
    }
    let mut take = |name: &str| {
        tables
            .remove(name)
            .ok_or_else(|| format!("no {name} table"))
    };
    Ok(ExperienceCostTables {
        attribute: take("attribute")?,
        vital: take("vital")?,
        trained: take("trained")?,
        specialized: take("specialized")?,
    })
}

/// The shipped cost tables.
///
/// # Panics
/// Never: the shipped file is checked by this module's tests.
#[must_use]
pub fn cost_tables() -> &'static ExperienceCostTables {
    static TABLES: OnceLock<ExperienceCostTables> = OnceLock::new();
    TABLES.get_or_init(|| read_tables(COSTS).expect("the shipped cost tables read"))
}

/// What the next rank costs, or `None` when there is none to buy: an
/// untrained skill, or a target within `stop_before_max` rows of the end of
/// its table (at least one row is always kept back). `offset` counts ranks
/// the plan has already taken.
#[must_use]
pub fn cost_to_level(
    state: &ExperienceTargetState,
    stop_before_max: i32,
    offset: i32,
) -> Option<i64> {
    let tables = cost_tables();
    let halt = i64::from(stop_before_max.max(1));
    let level = i64::from(state.ranks) + i64::from(offset);
    let table = match state.target.kind {
        ExperienceTargetKind::Attribute => &tables.attribute,
        ExperienceTargetKind::Vital => &tables.vital,
        ExperienceTargetKind::Skill => match state.training {
            Training::Specialized => &tables.specialized,
            Training::Trained => &tables.trained,
            _ => return None,
        },
    };
    let rows = i64::try_from(table.len()).unwrap_or(i64::MAX);
    if level >= 0 && rows > level + halt {
        usize::try_from(level).ok().map(|row| table[row])
    } else {
        None
    }
}

/// One spend of the plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExperienceStep {
    /// What it raises.
    pub target: ExperienceTarget,
    /// What it costs.
    pub cost: i64,
}

/// A target's share of the plan: the rank costs it takes, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExperienceTargetPlan {
    /// The target.
    pub target: ExperienceTarget,
    /// The ranks it takes.
    pub costs: Vec<i64>,
}

/// A plan: per target for the report, flat and sorted for the spend.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExperiencePlan {
    /// Per target, in policy order.
    pub by_target: Vec<ExperienceTargetPlan>,
    /// The spends, cheapest first.
    pub steps: Vec<ExperienceStep>,
}

impl ExperiencePlan {
    /// What the whole plan costs.
    #[must_use]
    pub fn total_cost(&self) -> i64 {
        self.steps.iter().map(|step| step.cost).sum()
    }
}

/// The step limit the planner uses unless told otherwise.
pub const DEFAULT_MAX_STEPS: i32 = 9999;

/// Plans a spend: each target's next rank is priced and divided by its
/// weight; the lowest weighted cost is taken (a tie to the target named
/// first), the target repriced, and so on until the next pick is
/// unaffordable or has no rank left. In batch mode each target's total is
/// cut into chunks of at most `max_chunk`. Steps are sorted by cost, ties
/// kept in plan order.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn plan(
    policy: &[(String, f64)],
    states: &[ExperienceTargetState],
    unassigned: i64,
    stop_before_max: i32,
    batch: bool,
    max_chunk: i64,
    max_steps: i32,
) -> ExperiencePlan {
    let mut candidates: Vec<(ExperienceTargetState, f64, Vec<i64>)> = Vec::new();
    let mut weighted: Vec<f64> = Vec::new();
    for (name, weight) in policy {
        let name = name.as_str();
        if *weight <= 0.0 {
            continue;
        }
        let Some(state) = states
            .iter()
            .find(|state| state.target.name.eq_ignore_ascii_case(name))
        else {
            continue;
        };
        match cost_to_level(state, stop_before_max, 0) {
            Some(first) if first > 0 => {
                candidates.push((*state, *weight, Vec::new()));
                weighted.push(first as f64 / weight);
            }
            _ => {}
        }
    }
    if candidates.is_empty() {
        return ExperiencePlan::default();
    }

    let mut remaining = unassigned;
    for _ in 0..max_steps {
        let mut next = 0;
        for index in 1..weighted.len() {
            if weighted[index] < weighted[next] {
                next = index;
            }
        }
        let (state, weight, costs) = &mut candidates[next];
        let taken = i32::try_from(costs.len()).unwrap_or(i32::MAX);
        let Some(price) = cost_to_level(state, stop_before_max, taken) else {
            break;
        };
        if remaining < price {
            break;
        }
        costs.push(price);
        remaining -= price;
        weighted[next] = cost_to_level(state, stop_before_max, taken + 1)
            .map_or(f64::INFINITY, |after| after as f64 / *weight);
    }

    let mut by_target = Vec::new();
    let mut steps = Vec::new();
    for (state, _, costs) in candidates {
        if batch {
            let mut total: i64 = costs.iter().sum();
            let chunk = max_chunk.max(1);
            while total > chunk {
                steps.push(ExperienceStep {
                    target: state.target,
                    cost: chunk,
                });
                total -= chunk;
            }
            if total > 0 {
                steps.push(ExperienceStep {
                    target: state.target,
                    cost: total,
                });
            }
        } else {
            steps.extend(costs.iter().map(|cost| ExperienceStep {
                target: state.target,
                cost: *cost,
            }));
        }
        by_target.push(ExperienceTargetPlan {
            target: state.target,
            costs,
        });
    }
    steps.sort_by_key(|step| step.cost);
    ExperiencePlan { by_target, steps }
}

/// The policy's text forms: the settings row of "Target = weight" lines,
/// and the one-line "Target=weight;..." that is exported and imported.
pub mod policy {
    use super::{ExperienceTarget, target_named};

    fn parse_pair(text: &str) -> Result<(ExperienceTarget, f64), String> {
        let Some((name, weight)) = text.split_once('=') else {
            return Err(format!("Unable to parse {}", text.trim()));
        };
        let name = name.trim();
        let Some(target) = target_named(name) else {
            return Err(format!("Unable to parse XpTarget {name}"));
        };
        let weight = weight.trim();
        mosstank_dotnet::try_parse_double(weight)
            .map(|value| (target, value))
            .ok_or_else(|| format!("Unable to parse {weight} for {}", target.name))
    }

    fn line(name: &str, weight: f64) -> String {
        format!("{name} = {}", mosstank_dotnet::double_to_string(weight))
    }

    /// The weights, in the row's order, dropping lines that name nothing.
    #[must_use]
    pub fn parse(lines: &[String]) -> Vec<(String, f64)> {
        lines
            .iter()
            .filter_map(|line| parse_pair(line).ok())
            .map(|(target, weight)| (target.name.to_owned(), weight))
            .collect()
    }

    /// "Target=weight;...".
    #[must_use]
    pub fn export(policy: &[(String, f64)]) -> String {
        policy
            .iter()
            .map(|(name, weight)| format!("{name}={}", mosstank_dotnet::double_to_string(*weight)))
            .collect::<Vec<_>>()
            .join(";")
    }

    /// Applies "Target=weight;..." over the row: a named target's line is
    /// rewritten in place, a new one is added at the end, and what cannot
    /// be read is reported in `notices`.
    #[must_use]
    pub fn import(lines: &[String], text: &str, notices: &mut Vec<String>) -> Vec<String> {
        let mut result = lines.to_vec();
        for pair in text.split(';') {
            if pair.trim().is_empty() {
                continue;
            }
            let (target, weight) = match parse_pair(pair) {
                Ok(parsed) => parsed,
                Err(problem) => {
                    notices.push(problem);
                    continue;
                }
            };
            let replacement = line(target.name, weight);
            match result.iter().position(|existing| {
                parse_pair(existing).is_ok_and(|(named, _)| named.name == target.name)
            }) {
                Some(index) => result[index] = replacement,
                None => result.push(replacement),
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn require(name: &str) -> ExperienceTarget {
        target_named(name).unwrap()
    }

    fn state(name: &str, ranks: i32) -> ExperienceTargetState {
        trained(name, ranks, Training::Unknown)
    }

    fn trained(name: &str, ranks: i32, training: Training) -> ExperienceTargetState {
        ExperienceTargetState {
            target: require(name),
            ranks,
            training,
        }
    }

    fn policy_of(entries: &[(&str, f64)]) -> Vec<(String, f64)> {
        entries
            .iter()
            .map(|(name, weight)| ((*name).to_owned(), *weight))
            .collect()
    }

    fn steps(plan: &ExperiencePlan) -> Vec<(&'static str, i64)> {
        plan.steps
            .iter()
            .map(|step| (step.target.name, step.cost))
            .collect()
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/ExperiencePlannerTests.cs :: CostTablesLoadWithTheirPublishedEnds
    #[test]
    fn cost_tables_load_with_their_published_ends() {
        let tables = cost_tables();
        assert_eq!(tables.attribute.len(), 190);
        assert_eq!(tables.vital.len(), 196);
        assert_eq!(tables.trained.len(), 208);
        assert_eq!(tables.specialized.len(), 226);
        assert_eq!(tables.attribute[0], 110);
        assert_eq!(tables.attribute.last(), Some(&308_765_680));
        assert_eq!(tables.vital[0], 73);
        assert_eq!(tables.vital.last(), Some(&329_220_194));
        assert_eq!(tables.trained[0], 58);
        assert_eq!(tables.trained.last(), Some(&306_860_483));
        assert_eq!(tables.specialized[0], 23);
        assert_eq!(tables.specialized.last(), Some(&350_046_134));
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/ExperiencePlannerTests.cs :: CostToLevelReadsTheRowAtTheRankAndStopsShortOfTheEnd
    #[test]
    fn cost_to_level_reads_the_row_at_the_rank_and_stops_short_of_the_end() {
        let attribute = &cost_tables().attribute;
        assert_eq!(cost_to_level(&state("Strength", 0), 10, 0), Some(110));
        assert_eq!(cost_to_level(&state("Strength", 3), 10, 0), Some(283));
        assert_eq!(cost_to_level(&state("Health", 2), 10, 0), Some(148));
        assert_eq!(
            cost_to_level(&trained("WarMagic", 0, Training::Trained), 10, 0),
            Some(58)
        );
        assert_eq!(
            cost_to_level(&trained("WarMagic", 2, Training::Specialized), 10, 0),
            Some(41)
        );
        assert_eq!(
            cost_to_level(&trained("WarMagic", 0, Training::Untrained), 10, 0),
            None
        );
        assert_eq!(
            cost_to_level(&trained("WarMagic", 0, Training::Unknown), 10, 0),
            None
        );

        assert_eq!(cost_to_level(&state("Strength", 180), 10, 0), None);
        assert_eq!(
            cost_to_level(&state("Strength", 179), 10, 0),
            Some(attribute[179])
        );
        assert_eq!(
            cost_to_level(&state("Strength", 188), 0, 0),
            Some(attribute[188])
        );
        assert_eq!(cost_to_level(&state("Strength", 189), 0, 0), None);
        assert_eq!(cost_to_level(&state("Strength", 0), 10, 1), Some(167));
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/ExperiencePlannerTests.cs :: TakesTheLowestWeightedCostAndRepricesUntilUnaffordable
    #[test]
    fn takes_the_lowest_weighted_cost_and_reprices_until_unaffordable() {
        let plan = plan(
            &policy_of(&[("Strength", 1.0), ("Health", 1.4), ("WarMagic", 10.0)]),
            &[
                state("Strength", 0),
                state("Health", 0),
                trained("WarMagic", 0, Training::Specialized),
            ],
            300,
            10,
            false,
            1_000_000_000,
            DEFAULT_MAX_STEPS,
        );
        assert_eq!(
            steps(&plan),
            [
                ("WarMagic", 23),
                ("WarMagic", 33),
                ("WarMagic", 41),
                ("WarMagic", 52),
                ("WarMagic", 62),
                ("WarMagic", 71)
            ]
        );
        assert_eq!(plan.total_cost(), 282);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/ExperiencePlannerTests.cs :: EqualWeightsInterleaveAndTheFlatPlanIsSortedByCost
    #[test]
    fn equal_weights_interleave_and_the_flat_plan_is_sorted_by_cost() {
        let states = [state("Strength", 0), state("Health", 0)];
        let policy = policy_of(&[("Strength", 1.0), ("Health", 1.0)]);
        let full = plan(
            &policy,
            &states,
            500,
            10,
            false,
            1_000_000_000,
            DEFAULT_MAX_STEPS,
        );
        assert_eq!(
            steps(&full),
            [
                ("Health", 73),
                ("Strength", 110),
                ("Health", 110),
                ("Health", 148)
            ]
        );
        let by_target: Vec<_> = full
            .by_target
            .iter()
            .map(|entry| {
                (
                    entry.target.name,
                    entry.costs.len(),
                    entry.costs.iter().sum::<i64>(),
                )
            })
            .collect();
        assert_eq!(by_target, [("Strength", 1, 110), ("Health", 3, 331)]);

        let on_the_tie = plan(
            &policy,
            &states,
            183,
            10,
            false,
            1_000_000_000,
            DEFAULT_MAX_STEPS,
        );
        assert_eq!(steps(&on_the_tie), [("Health", 73), ("Strength", 110)]);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/ExperiencePlannerTests.cs :: BatchModeChunksEachTargetsTotal
    #[test]
    fn batch_mode_chunks_each_targets_total() {
        let plan = plan(
            &policy_of(&[("Health", 1.0)]),
            &[state("Health", 0)],
            1000,
            10,
            true,
            200,
            DEFAULT_MAX_STEPS,
        );
        assert_eq!(
            steps(&plan),
            [
                ("Health", 143),
                ("Health", 200),
                ("Health", 200),
                ("Health", 200)
            ]
        );
        assert_eq!(plan.total_cost(), 743);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/ExperiencePlannerTests.cs :: SkipsZeroWeightsUntrainedSkillsAndExhaustedTargets
    #[test]
    fn skips_zero_weights_untrained_skills_and_exhausted_targets() {
        let first = plan(
            &policy_of(&[
                ("Strength", 1.0),
                ("Endurance", 0.0),
                ("WarMagic", 10.0),
                ("Alchemy", 5.0),
            ]),
            &[
                state("Strength", 0),
                state("Endurance", 0),
                trained("WarMagic", 0, Training::Untrained),
                trained("Alchemy", 0, Training::Trained),
            ],
            10_000_000_000,
            10,
            false,
            1_000_000_000,
            3,
        );
        assert_eq!(
            steps(&first),
            [("Alchemy", 58), ("Alchemy", 80), ("Alchemy", 105)]
        );

        let strength = policy_of(&[("Strength", 1.0)]);
        let last_step = plan(
            &strength,
            &[state("Strength", 179)],
            10_000_000_000,
            10,
            false,
            1_000_000_000,
            3,
        );
        assert_eq!(
            steps(&last_step),
            [("Strength", cost_tables().attribute[179])]
        );

        let nothing = plan(
            &strength,
            &[state("Strength", 180)],
            10_000_000_000,
            10,
            false,
            1_000_000_000,
            DEFAULT_MAX_STEPS,
        );
        assert!(nothing.steps.is_empty());
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/ExperiencePlannerTests.cs :: PolicyTextRoundTripsAndImportReportsWhatItCannotRead
    #[test]
    fn policy_text_round_trips_and_import_reports_what_it_cannot_read() {
        let lines: Vec<String> = ["Strength = 1", "Health = 1.4", "Jump = 0.02", "Cooking = 0"]
            .map(str::to_owned)
            .to_vec();
        let parsed = policy::parse(&lines);
        assert_eq!(
            parsed,
            policy_of(&[
                ("Strength", 1.0),
                ("Health", 1.4),
                ("Jump", 0.02),
                ("Cooking", 0.0)
            ])
        );
        assert_eq!(
            policy::export(&parsed),
            "Strength=1;Health=1.4;Jump=0.02;Cooking=0"
        );

        let mut notices = Vec::new();
        let imported = policy::import(
            &lines,
            "Cooking=2;Alchemy=3;Bogus=4;Run=x;;Health=0.5",
            &mut notices,
        );
        assert_eq!(
            imported,
            [
                "Strength = 1",
                "Health = 0.5",
                "Jump = 0.02",
                "Cooking = 2",
                "Alchemy = 3"
            ]
        );
        assert_eq!(notices.len(), 2);
        assert!(notices[0].contains("Bogus"));
        assert!(notices[1].contains("Run"));
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/ExperiencePlannerTests.cs :: CatalogueNamesEveryTargetWithItsId
    #[test]
    fn catalogue_names_every_target_with_its_id() {
        let count = |kind| TARGETS.iter().filter(|target| target.kind == kind).count();
        assert_eq!(TARGETS.len(), 47);
        assert_eq!(count(Attribute), 6);
        assert_eq!(count(Vital), 3);
        assert_eq!(count(Skill), 38);
        assert_eq!(require("MeleeDefense").stat_id, 6);
        assert_eq!(require("Summoning").stat_id, 54);
        assert_eq!(require("Strength").stat_id, 1);
        assert_eq!(require("Mana").stat_id, 5);
        assert!(target_named("Bogus").is_none());
    }
}

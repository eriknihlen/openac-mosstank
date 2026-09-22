using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The experience planner, pure: a weight policy, the four cost tables, each
/// target's ranks and training, the banked experience, and out comes the
/// ordered list of spends. Every golden value below is read off the tables
/// by hand: attributes start 110, 167, 224, 283, 341, 402; vitals 73, 110,
/// 148, 186, 226, 265; trained skills 58, 80, 105; specialized 23, 33, 41,
/// 52, 62, 71, 82.
/// </summary>
public sealed class ExperiencePlannerTests
{
    /// <summary>
    /// The four tables are data, and their first and last rows are the ones
    /// the source publishes. Mutation: a table read off by one row prices
    /// the first rank at the second row's cost.
    /// </summary>
    [Fact]
    public void CostTablesLoadWithTheirPublishedEnds()
    {
        Assert.Equal(190, ExperienceCostTables.Attribute.Count);
        Assert.Equal(196, ExperienceCostTables.Vital.Count);
        Assert.Equal(208, ExperienceCostTables.Trained.Count);
        Assert.Equal(226, ExperienceCostTables.Specialized.Count);
        Assert.Equal(110, ExperienceCostTables.Attribute[0]);
        Assert.Equal(308765680, ExperienceCostTables.Attribute[^1]);
        Assert.Equal(73, ExperienceCostTables.Vital[0]);
        Assert.Equal(329220194, ExperienceCostTables.Vital[^1]);
        Assert.Equal(58, ExperienceCostTables.Trained[0]);
        Assert.Equal(306860483, ExperienceCostTables.Trained[^1]);
        Assert.Equal(23, ExperienceCostTables.Specialized[0]);
        Assert.Equal(350046134, ExperienceCostTables.Specialized[^1]);
    }

    /// <summary>
    /// The cost of the next rank is the table row at the current rank, and
    /// the last rows are kept back: with a stop of 10, an attribute at rank
    /// 180 has nowhere to go, at 179 it has one step. A stop of zero still
    /// keeps one row back. An untrained skill has no cost at all.
    /// </summary>
    [Fact]
    public void CostToLevelReadsTheRowAtTheRankAndStopsShortOfTheEnd()
    {
        ExperienceTarget strength = ExperienceTargets.Require("Strength");
        ExperienceTarget health = ExperienceTargets.Require("Health");
        ExperienceTarget war = ExperienceTargets.Require("WarMagic");

        Assert.Equal(110, ExperienceCostTables.CostToLevel(State(strength, 0), 10));
        Assert.Equal(283, ExperienceCostTables.CostToLevel(State(strength, 3), 10));
        Assert.Equal(148, ExperienceCostTables.CostToLevel(State(health, 2), 10));
        Assert.Equal(58, ExperienceCostTables.CostToLevel(State(war, 0, PluginSkillTraining.Trained), 10));
        Assert.Equal(41, ExperienceCostTables.CostToLevel(State(war, 2, PluginSkillTraining.Specialized), 10));
        Assert.Null(ExperienceCostTables.CostToLevel(State(war, 0, PluginSkillTraining.Untrained), 10));
        Assert.Null(ExperienceCostTables.CostToLevel(State(war, 0, PluginSkillTraining.Unknown), 10));

        Assert.Null(ExperienceCostTables.CostToLevel(State(strength, 180), 10));
        Assert.Equal(ExperienceCostTables.Attribute[179], ExperienceCostTables.CostToLevel(State(strength, 179), 10));
        Assert.Equal(ExperienceCostTables.Attribute[188], ExperienceCostTables.CostToLevel(State(strength, 188), 0));
        Assert.Null(ExperienceCostTables.CostToLevel(State(strength, 189), 0));
        // The offset prices a rank the plan has already taken in simulation.
        Assert.Equal(167, ExperienceCostTables.CostToLevel(State(strength, 0), 10, offset: 1));
    }

    /// <summary>
    /// The cheapest weighted cost is taken each step and repriced after. A
    /// heavy weight on a cheap specialized skill takes every step until the
    /// bank runs dry: 23+33+41+52+62+71 = 282 of 300, and 82 is unaffordable.
    /// </summary>
    [Fact]
    public void TakesTheLowestWeightedCostAndRepricesUntilUnaffordable()
    {
        ExperiencePlan plan = ExperiencePlanner.Plan(
            Policy(("Strength", 1d), ("Health", 1.4d), ("WarMagic", 10d)),
            States(
                State(ExperienceTargets.Require("Strength"), 0),
                State(ExperienceTargets.Require("Health"), 0),
                State(ExperienceTargets.Require("WarMagic"), 0, PluginSkillTraining.Specialized)),
            unassigned: 300,
            stopBeforeMax: 10,
            batch: false,
            maxChunk: 1_000_000_000);

        Assert.Equal(
            [("WarMagic", 23L), ("WarMagic", 33L), ("WarMagic", 41L), ("WarMagic", 52L), ("WarMagic", 62L), ("WarMagic", 71L)],
            plan.Steps.Select(s => (s.Target.Name, s.Cost)).ToArray());
        Assert.Equal(282, plan.TotalCost);
    }

    /// <summary>
    /// Equal weights interleave by cost, a tie going to the target named
    /// first in the policy, and the flat plan is sorted ascending by cost.
    /// By hand with 500: health 73 (427 left), strength 110 on the tie
    /// (317), health 110 (207), health 148 (59), strength 167 unaffordable.
    /// With exactly 183 the bank runs out on the tie, so which side wins it
    /// shows: strength. Mutation: giving a tie to the later target, or not
    /// repricing a target after a step, changes both plans.
    /// </summary>
    [Fact]
    public void EqualWeightsInterleaveAndTheFlatPlanIsSortedByCost()
    {
        ExperiencePlan plan = ExperiencePlanner.Plan(
            Policy(("Strength", 1d), ("Health", 1d)),
            States(
                State(ExperienceTargets.Require("Strength"), 0),
                State(ExperienceTargets.Require("Health"), 0)),
            unassigned: 500,
            stopBeforeMax: 10,
            batch: false,
            maxChunk: 1_000_000_000);

        Assert.Equal(
            [("Health", 73L), ("Strength", 110L), ("Health", 110L), ("Health", 148L)],
            plan.Steps.Select(s => (s.Target.Name, s.Cost)).ToArray());
        Assert.Equal(
            [("Strength", 1, 110L), ("Health", 3, 331L)],
            plan.ByTarget.Select(t => (t.Target.Name, t.Costs.Count, t.Costs.Sum())).ToArray());

        ExperiencePlan onTheTie = ExperiencePlanner.Plan(
            Policy(("Strength", 1d), ("Health", 1d)),
            States(
                State(ExperienceTargets.Require("Strength"), 0),
                State(ExperienceTargets.Require("Health"), 0)),
            unassigned: 183,
            stopBeforeMax: 10,
            batch: false,
            maxChunk: 1_000_000_000);
        Assert.Equal(
            [("Health", 73L), ("Strength", 110L)],
            onTheTie.Steps.Select(s => (s.Target.Name, s.Cost)).ToArray());
    }

    /// <summary>
    /// Batch mode adds a target's steps up and hands them over in chunks of
    /// at most the chunk size, sorted ascending. Health with 1000 banked:
    /// 73+110+148+186+226 = 743 (265 is unaffordable), in 200s: 200, 200,
    /// 200, 143. Mutation: chunking the whole plan instead of each target
    /// mixes two targets into one request.
    /// </summary>
    [Fact]
    public void BatchModeChunksEachTargetsTotal()
    {
        ExperiencePlan plan = ExperiencePlanner.Plan(
            Policy(("Health", 1d)),
            States(State(ExperienceTargets.Require("Health"), 0)),
            unassigned: 1000,
            stopBeforeMax: 10,
            batch: true,
            maxChunk: 200);

        Assert.Equal(
            [("Health", 143L), ("Health", 200L), ("Health", 200L), ("Health", 200L)],
            plan.Steps.Select(s => (s.Target.Name, s.Cost)).ToArray());
        Assert.Equal(743, plan.TotalCost);
    }

    /// <summary>
    /// Zero weights, untrained skills and targets with no rows left are not
    /// candidates, and a target that runs out of rows mid-plan stops being
    /// picked. Three steps of a trained skill at weight 5 (58, 80, 105 at
    /// 11.6, 16, 21 weighted) all beat strength's 110; strength at rank 179
    /// on its own has exactly one step left.
    /// </summary>
    [Fact]
    public void SkipsZeroWeightsUntrainedSkillsAndExhaustedTargets()
    {
        ExperiencePlan plan = ExperiencePlanner.Plan(
            Policy(("Strength", 1d), ("Endurance", 0d), ("WarMagic", 10d), ("Alchemy", 5d)),
            States(
                State(ExperienceTargets.Require("Strength"), 0),
                State(ExperienceTargets.Require("Endurance"), 0),
                State(ExperienceTargets.Require("WarMagic"), 0, PluginSkillTraining.Untrained),
                State(ExperienceTargets.Require("Alchemy"), 0, PluginSkillTraining.Trained)),
            unassigned: 10_000_000_000,
            stopBeforeMax: 10,
            batch: false,
            maxChunk: 1_000_000_000,
            maxSteps: 3);
        Assert.Equal(
            [("Alchemy", 58L), ("Alchemy", 80L), ("Alchemy", 105L)],
            plan.Steps.Select(s => (s.Target.Name, s.Cost)).ToArray());

        ExperiencePlan lastStep = ExperiencePlanner.Plan(
            Policy(("Strength", 1d)),
            States(State(ExperienceTargets.Require("Strength"), 179)),
            unassigned: 10_000_000_000,
            stopBeforeMax: 10,
            batch: false,
            maxChunk: 1_000_000_000,
            maxSteps: 3);
        Assert.Equal(
            [("Strength", ExperienceCostTables.Attribute[179])],
            lastStep.Steps.Select(s => (s.Target.Name, s.Cost)).ToArray());

        ExperiencePlan nothing = ExperiencePlanner.Plan(
            Policy(("Strength", 1d)),
            States(State(ExperienceTargets.Require("Strength"), 180)),
            unassigned: 10_000_000_000,
            stopBeforeMax: 10,
            batch: false,
            maxChunk: 1_000_000_000);
        Assert.Empty(nothing.Steps);
    }

    /// <summary>
    /// The policy text: lines of "Target = weight" in, the export form
    /// "Target=weight;..." out, and an import that overrides what it names,
    /// adds what is missing, and reports what it cannot read. Mutation:
    /// dropping unknown names silently hides a typo in a pasted policy.
    /// </summary>
    [Fact]
    public void PolicyTextRoundTripsAndImportReportsWhatItCannotRead()
    {
        string[] lines = ["Strength = 1", "Health = 1.4", "Jump = 0.02", "Cooking = 0"];
        IReadOnlyList<(string Name, double Weight)> policy = ExperiencePolicy.Parse(lines);
        Assert.Equal(
            [("Strength", 1d), ("Health", 1.4d), ("Jump", 0.02d), ("Cooking", 0d)],
            policy.ToArray());
        Assert.Equal("Strength=1;Health=1.4;Jump=0.02;Cooking=0", ExperiencePolicy.Export(policy));

        var notices = new List<string>();
        IReadOnlyList<string> imported = ExperiencePolicy.Import(
            lines, "Cooking=2;Alchemy=3;Bogus=4;Run=x;;Health=0.5", notices);
        Assert.Equal(
            ["Strength = 1", "Health = 0.5", "Jump = 0.02", "Cooking = 2", "Alchemy = 3"],
            imported.ToArray());
        Assert.Equal(2, notices.Count);
        Assert.Contains("Bogus", notices[0], StringComparison.Ordinal);
        Assert.Contains("Run", notices[1], StringComparison.Ordinal);
    }

    /// <summary>
    /// The catalogue: six attributes, three vitals and thirty-eight skills,
    /// each with the id a spend names it by.
    /// </summary>
    [Fact]
    public void CatalogueNamesEveryTargetWithItsId()
    {
        Assert.Equal(47, ExperienceTargets.All.Count);
        Assert.Equal(6, ExperienceTargets.All.Count(t => t.Kind == ExperienceTargetKind.Attribute));
        Assert.Equal(3, ExperienceTargets.All.Count(t => t.Kind == ExperienceTargetKind.Vital));
        Assert.Equal(38, ExperienceTargets.All.Count(t => t.Kind == ExperienceTargetKind.Skill));
        Assert.Equal(6u, ExperienceTargets.Require("MeleeDefense").StatId);
        Assert.Equal(54u, ExperienceTargets.Require("Summoning").StatId);
        Assert.Equal(1u, ExperienceTargets.Require("Strength").StatId);
        Assert.Equal(5u, ExperienceTargets.Require("Mana").StatId);
        Assert.False(ExperienceTargets.TryGet("Bogus", out _));
    }

    private static IReadOnlyList<(string Name, double Weight)> Policy(
        params (string Name, double Weight)[] entries) => entries;

    private static IReadOnlyList<ExperienceTargetState> States(
        params ExperienceTargetState[] states) => states;

    private static ExperienceTargetState State(
        ExperienceTarget target,
        int ranks,
        PluginSkillTraining training = PluginSkillTraining.Unknown) =>
        new(target, ranks, training);
}

using System.Globalization;
using System.Reflection;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>Which of the three pools of things experience is spent on.</summary>
internal enum ExperienceTargetKind
{
    Attribute,
    Vital,
    Skill,
}

/// <summary>
/// One thing experience can be spent on. <paramref name="StatId"/> is the
/// number a spend names it by when the host's own record carries none: the
/// attribute's number, the pool's number, or the skill's id.
/// </summary>
internal readonly record struct ExperienceTarget(
    string Name,
    ExperienceTargetKind Kind,
    uint StatId)
{
    public PluginAdvancementKind AdvancementKind => Kind switch
    {
        ExperienceTargetKind.Attribute => PluginAdvancementKind.Attribute,
        ExperienceTargetKind.Vital => PluginAdvancementKind.Vital,
        _ => PluginAdvancementKind.Skill,
    };
}

/// <summary>
/// Where one target stands: how many times it has been raised, and, for a
/// skill, how far it is trained -- which decides which cost table prices it.
/// </summary>
internal readonly record struct ExperienceTargetState(
    ExperienceTarget Target,
    int Ranks,
    PluginSkillTraining Training);

/// <summary>One request of the plan: this much on this target.</summary>
internal readonly record struct ExperienceStep(ExperienceTarget Target, long Cost);

/// <summary>A target's share of a plan: the rank costs it takes, in order.</summary>
internal sealed record ExperienceTargetPlan(ExperienceTarget Target, IReadOnlyList<long> Costs);

/// <summary>
/// The plan: per target for the report, and flat and sorted for the spend.
/// </summary>
internal sealed record ExperiencePlan(
    IReadOnlyList<ExperienceTargetPlan> ByTarget,
    IReadOnlyList<ExperienceStep> Steps)
{
    public static ExperiencePlan Empty { get; } = new([], []);

    public long TotalCost => Steps.Sum(static step => step.Cost);
}

/// <summary>
/// The forty-seven targets: six attributes, three vitals and thirty-eight
/// skills, under the names a policy uses for them.
/// </summary>
internal static class ExperienceTargets
{
    public static IReadOnlyList<ExperienceTarget> All { get; } =
    [
        new("Strength", ExperienceTargetKind.Attribute, 1u),
        new("Endurance", ExperienceTargetKind.Attribute, 2u),
        new("Quickness", ExperienceTargetKind.Attribute, 3u),
        new("Coordination", ExperienceTargetKind.Attribute, 4u),
        new("Focus", ExperienceTargetKind.Attribute, 5u),
        new("Self", ExperienceTargetKind.Attribute, 6u),
        new("Health", ExperienceTargetKind.Vital, 1u),
        new("Stamina", ExperienceTargetKind.Vital, 3u),
        new("Mana", ExperienceTargetKind.Vital, 5u),
        new("MeleeDefense", ExperienceTargetKind.Skill, 6u),
        new("MissileDefense", ExperienceTargetKind.Skill, 7u),
        new("ArcaneLore", ExperienceTargetKind.Skill, 14u),
        new("MagicDefense", ExperienceTargetKind.Skill, 15u),
        new("ManaConversion", ExperienceTargetKind.Skill, 16u),
        new("ItemTinkering", ExperienceTargetKind.Skill, 18u),
        new("AssessPerson", ExperienceTargetKind.Skill, 19u),
        new("Deception", ExperienceTargetKind.Skill, 20u),
        new("Healing", ExperienceTargetKind.Skill, 21u),
        new("Jump", ExperienceTargetKind.Skill, 22u),
        new("Lockpick", ExperienceTargetKind.Skill, 23u),
        new("Run", ExperienceTargetKind.Skill, 24u),
        new("AssessCreature", ExperienceTargetKind.Skill, 27u),
        new("WeaponTinkering", ExperienceTargetKind.Skill, 28u),
        new("ArmorTinkering", ExperienceTargetKind.Skill, 29u),
        new("MagicItemTinkering", ExperienceTargetKind.Skill, 30u),
        new("CreatureEnchantment", ExperienceTargetKind.Skill, 31u),
        new("ItemEnchantment", ExperienceTargetKind.Skill, 32u),
        new("LifeMagic", ExperienceTargetKind.Skill, 33u),
        new("WarMagic", ExperienceTargetKind.Skill, 34u),
        new("Leadership", ExperienceTargetKind.Skill, 35u),
        new("Loyalty", ExperienceTargetKind.Skill, 36u),
        new("Fletching", ExperienceTargetKind.Skill, 37u),
        new("Alchemy", ExperienceTargetKind.Skill, 38u),
        new("Cooking", ExperienceTargetKind.Skill, 39u),
        new("Salvaging", ExperienceTargetKind.Skill, 40u),
        new("TwoHandedCombat", ExperienceTargetKind.Skill, 41u),
        new("VoidMagic", ExperienceTargetKind.Skill, 43u),
        new("HeavyWeapons", ExperienceTargetKind.Skill, 44u),
        new("LightWeapons", ExperienceTargetKind.Skill, 45u),
        new("FinesseWeapons", ExperienceTargetKind.Skill, 46u),
        new("MissileWeapons", ExperienceTargetKind.Skill, 47u),
        new("Shield", ExperienceTargetKind.Skill, 48u),
        new("DualWield", ExperienceTargetKind.Skill, 49u),
        new("Recklessness", ExperienceTargetKind.Skill, 50u),
        new("SneakAttack", ExperienceTargetKind.Skill, 51u),
        new("DirtyFighting", ExperienceTargetKind.Skill, 52u),
        new("Summoning", ExperienceTargetKind.Skill, 54u),
    ];

    private static readonly Dictionary<string, ExperienceTarget> ByName =
        All.ToDictionary(static t => t.Name, StringComparer.OrdinalIgnoreCase);

    public static bool TryGet(string name, out ExperienceTarget target) =>
        ByName.TryGetValue(name.Trim(), out target);

    public static ExperienceTarget Require(string name) => TryGet(name, out ExperienceTarget target)
        ? target
        : throw new KeyNotFoundException($"No experience target is called {name}.");
}

/// <summary>
/// The four cost tables, shipped as data beside the assembly: what the next
/// rank of an attribute, a vital, a trained skill and a specialized skill
/// costs, row by rank.
/// </summary>
internal static class ExperienceCostTables
{
    private const string ResourceName = "AcDream.Plugins.MossTank.ExperienceCosts.tsv";

    private static readonly Dictionary<string, long[]> Tables = Load();

    public static IReadOnlyList<long> Attribute => Tables["attribute"];
    public static IReadOnlyList<long> Vital => Tables["vital"];
    public static IReadOnlyList<long> Trained => Tables["trained"];
    public static IReadOnlyList<long> Specialized => Tables["specialized"];

    /// <summary>
    /// What the next rank costs, or null when there is no rank to buy: an
    /// untrained skill, or a target within <paramref name="stopBeforeMax"/>
    /// rows of the end of its table. At least one row is always kept back,
    /// because the last rank can overshoot what the character has.
    /// </summary>
    /// <param name="state">The target and where it stands.</param>
    /// <param name="stopBeforeMax">How many rows short of the end to stop.</param>
    /// <param name="offset">Ranks the plan has already taken in simulation.</param>
    public static long? CostToLevel(in ExperienceTargetState state, int stopBeforeMax, int offset = 0)
    {
        int halt = Math.Max(stopBeforeMax, 1);
        int level = state.Ranks + offset;
        IReadOnlyList<long>? table = state.Target.Kind switch
        {
            ExperienceTargetKind.Attribute => Attribute,
            ExperienceTargetKind.Vital => Vital,
            _ => state.Training switch
            {
                PluginSkillTraining.Specialized => Specialized,
                PluginSkillTraining.Trained => Trained,
                _ => null,
            },
        };
        return table is not null && level >= 0 && table.Count > level + halt
            ? table[level]
            : null;
    }

    private static Dictionary<string, long[]> Load()
    {
        using Stream stream = typeof(ExperienceCostTables).Assembly
            .GetManifestResourceStream(ResourceName)
            ?? throw new InvalidOperationException($"Missing resource {ResourceName}.");
        using var reader = new StreamReader(stream);
        var tables = new Dictionary<string, long[]>(StringComparer.Ordinal);
        while (reader.ReadLine() is { } line)
        {
            if (line.Length == 0)
                continue;
            string[] cells = line.Split('\t');
            tables[cells[0]] = cells
                .Skip(1)
                .Where(static cell => cell.Length > 0)
                .Select(static cell => long.Parse(cell, NumberStyles.Integer, CultureInfo.InvariantCulture))
                .ToArray();
        }
        return tables;
    }
}

/// <summary>
/// The policy's text forms: the settings row of "Target = weight" lines, and
/// the one-line "Target=weight;..." that is exported and imported, so a
/// policy written for the tool this comes from pastes straight in.
/// </summary>
internal static class ExperiencePolicy
{
    /// <summary>The weights, in the row's order, dropping lines that name nothing.</summary>
    public static IReadOnlyList<(string Name, double Weight)> Parse(IReadOnlyList<string> lines)
    {
        ArgumentNullException.ThrowIfNull(lines);
        var policy = new List<(string, double)>();
        foreach (string line in lines)
        {
            if (TryParsePair(line, out ExperienceTarget target, out double weight, out _))
                policy.Add((target.Name, weight));
        }
        return policy;
    }

    public static string Export(IReadOnlyList<(string Name, double Weight)> policy) => string.Join(
        ";",
        policy.Select(static entry =>
            $"{entry.Name}={entry.Weight.ToString(CultureInfo.InvariantCulture)}"));

    /// <summary>
    /// Applies "Target=weight;..." over the row: a named target's line is
    /// rewritten in place, a target not yet in the row is added at the end,
    /// and what cannot be read is reported rather than dropped.
    /// </summary>
    public static IReadOnlyList<string> Import(
        IReadOnlyList<string> lines,
        string text,
        List<string> notices)
    {
        ArgumentNullException.ThrowIfNull(lines);
        ArgumentNullException.ThrowIfNull(notices);
        var result = new List<string>(lines);
        foreach (string pair in (text ?? string.Empty).Split(';'))
        {
            if (pair.Trim().Length == 0)
                continue;
            if (!TryParsePair(pair, out ExperienceTarget target, out double weight, out string? problem))
            {
                notices.Add(problem ?? $"Unable to parse {pair.Trim()}");
                continue;
            }
            string line = Line(target.Name, weight);
            int index = result.FindIndex(existing =>
                TryParsePair(existing, out ExperienceTarget named, out _, out _)
                && named.Name == target.Name);
            if (index >= 0)
                result[index] = line;
            else
                result.Add(line);
        }
        return result;
    }

    private static string Line(string name, double weight) =>
        $"{name} = {weight.ToString(CultureInfo.InvariantCulture)}";

    private static bool TryParsePair(
        string text,
        out ExperienceTarget target,
        out double weight,
        out string? problem)
    {
        target = default;
        weight = 0d;
        problem = null;
        string[] halves = (text ?? string.Empty).Split('=', 2);
        if (halves.Length != 2)
        {
            problem = $"Unable to parse {text?.Trim()}";
            return false;
        }
        string name = halves[0].Trim();
        if (!ExperienceTargets.TryGet(name, out target))
        {
            problem = $"Unable to parse experience target {name}";
            return false;
        }
        if (!double.TryParse(halves[1].Trim(), NumberStyles.Float, CultureInfo.InvariantCulture, out weight))
        {
            problem = $"Unable to parse {halves[1].Trim()} for {target.Name}";
            return false;
        }
        return true;
    }
}

/// <summary>
/// Plans a spend of banked experience by a weight policy. Each target's
/// next rank is priced from its table and divided by its weight; the lowest
/// weighted cost is taken, the target repriced at its next rank, and so on
/// until the next pick is unaffordable or has no rank left. The result is
/// handed over one rank at a time, or, in batch mode, as each target's total
/// cut into chunks; either way sorted cheapest first.
/// </summary>
internal static class ExperiencePlanner
{
    public const int DefaultMaxSteps = 9999;

    public static ExperiencePlan Plan(
        IReadOnlyList<(string Name, double Weight)> policy,
        IReadOnlyList<ExperienceTargetState> states,
        long unassigned,
        int stopBeforeMax,
        bool batch,
        long maxChunk,
        int maxSteps = DefaultMaxSteps)
    {
        ArgumentNullException.ThrowIfNull(policy);
        ArgumentNullException.ThrowIfNull(states);

        Dictionary<string, ExperienceTargetState> byName = states.ToDictionary(
            static state => state.Target.Name,
            StringComparer.OrdinalIgnoreCase);
        var candidates = new List<(ExperienceTargetState State, double Weight, List<long> Costs)>();
        var weighted = new List<double>();
        foreach ((string name, double weight) in policy)
        {
            if (weight <= 0d || !byName.TryGetValue(name, out ExperienceTargetState state))
                continue;
            long? cost = ExperienceCostTables.CostToLevel(state, stopBeforeMax);
            if (cost is not { } first || first <= 0L)
                continue;
            candidates.Add((state, weight, []));
            weighted.Add(first / weight);
        }
        if (candidates.Count == 0)
            return ExperiencePlan.Empty;

        long remaining = unassigned;
        for (int step = 0; step < maxSteps; step++)
        {
            int next = 0;
            for (int index = 1; index < weighted.Count; index++)
            {
                if (weighted[index] < weighted[next])
                    next = index;
            }
            (ExperienceTargetState state, double weight, List<long> costs) = candidates[next];
            long? cost = ExperienceCostTables.CostToLevel(state, stopBeforeMax, costs.Count);
            if (cost is not { } price || remaining < price)
                break;
            costs.Add(price);
            remaining -= price;
            long? following = ExperienceCostTables.CostToLevel(state, stopBeforeMax, costs.Count);
            weighted[next] = following is { } after ? after / weight : double.PositiveInfinity;
        }

        var byTarget = new List<ExperienceTargetPlan>();
        var steps = new List<ExperienceStep>();
        foreach ((ExperienceTargetState state, _, List<long> costs) in candidates)
        {
            byTarget.Add(new ExperienceTargetPlan(state.Target, costs));
            if (batch)
            {
                long total = costs.Sum();
                long chunk = Math.Max(1L, maxChunk);
                while (total > chunk)
                {
                    steps.Add(new ExperienceStep(state.Target, chunk));
                    total -= chunk;
                }
                if (total > 0L)
                    steps.Add(new ExperienceStep(state.Target, total));
            }
            else
            {
                foreach (long cost in costs)
                    steps.Add(new ExperienceStep(state.Target, cost));
            }
        }
        return new ExperiencePlan(byTarget, steps.OrderBy(static step => step.Cost).ToArray());
    }
}

namespace AcDream.Plugins.MossTank;

/// <summary>
/// One step a drain plan can take: a spell, and what casting it would do to
/// the two pools the plan cares about — the caster's own health and the
/// monster's remaining health.
/// </summary>
internal abstract class CorpseDrainStep
{
    /// <summary>
    /// The turnaround for one cast: the spell's own cast time plus the fixed
    /// overhead every cast carries.
    /// </summary>
    private const int CastOverheadMilliseconds = 800;

    protected CorpseDrainStep(uint spellId, int castMilliseconds)
    {
        SpellId = spellId;
        CostMilliseconds = castMilliseconds + CastOverheadMilliseconds;
    }

    /// <summary>Zero for the heal-yourself step, which names no spell here.</summary>
    public uint SpellId { get; }

    public int CostMilliseconds { get; }

    public abstract CorpseDrainState Apply(CorpseDrainState state);
}

/// <summary>
/// Where a plan stands: how much health the caster has of how much, how much
/// the monster has left, and how long the plan has cost so far.
/// </summary>
internal struct CorpseDrainState(int health, int maximumHealth, int targetHealth)
{
    public int TargetHealth = targetHealth;
    public int Health = health;
    public readonly int MaximumHealth = maximumHealth;
    public int CostMilliseconds = 0;

    public readonly int Deficit => MaximumHealth - Health;

    /// <summary>The monster is down: the plan has done its job.</summary>
    public readonly bool Done => TargetHealth <= 0;
}

/// <summary>Takes health off the monster and puts some of it on the caster.</summary>
internal sealed class CorpseDrainDrainStep(
    uint spellId,
    int castMilliseconds,
    double enemyFactor,
    double resultMultiplier,
    int maximumPoints)
    : CorpseDrainStep(spellId, castMilliseconds)
{
    public override CorpseDrainState Apply(CorpseDrainState state)
    {
        CorpseDrainState result = state;
        int drained = (int)Math.Round(state.TargetHealth * enemyFactor);
        if (drained > state.TargetHealth)
            drained = state.TargetHealth;
        if (drained > maximumPoints)
            drained = maximumPoints;
        int gained = (int)Math.Round(drained * resultMultiplier);
        if (gained > state.Deficit)
        {
            // Overhealing is wasted, so the drain is trimmed back to exactly
            // what the caster can still take.
            drained -= (int)Math.Round((gained - state.Deficit) / resultMultiplier);
            if (drained < 0)
                drained = 0;
            gained = state.Deficit;
        }
        result.Health += gained;
        result.TargetHealth -= drained;
        result.CostMilliseconds += CostMilliseconds;
        return result;
    }
}

/// <summary>Spends the caster's own health to hurt the monster.</summary>
internal sealed class CorpseDrainMartyrStep(
    uint spellId,
    int castMilliseconds,
    double selfFactor,
    double resultMultiplier)
    : CorpseDrainStep(spellId, castMilliseconds)
{
    public override CorpseDrainState Apply(CorpseDrainState state)
    {
        CorpseDrainState result = state;
        int spent = (int)Math.Round(result.Health * selfFactor);
        result.Health -= spent;
        result.TargetHealth -= (int)Math.Round(spent * resultMultiplier);
        if (result.TargetHealth < 0)
            result.TargetHealth = 0;
        result.CostMilliseconds += CostMilliseconds;
        return result;
    }
}

/// <summary>Heals the caster and does nothing to the monster.</summary>
internal sealed class CorpseDrainHealSelfStep()
    : CorpseDrainStep(0u, HealCastMilliseconds)
{
    private const int HealCastMilliseconds = 2450;
    private const int HealPoints = 115;

    public override CorpseDrainState Apply(CorpseDrainState state)
    {
        CorpseDrainState result = state;
        result.Health += HealPoints;
        if (result.Health > result.MaximumHealth)
            result.Health = result.MaximumHealth;
        result.CostMilliseconds += CostMilliseconds;
        return result;
    }
}

/// <summary>
/// Picks the next spell in the cheapest sequence of drains, martyrs and
/// self-heals that finishes the monster off without letting the caster's own
/// health fall below the floor the recharge settings set.
///
/// Only the FIRST step of the winning plan is played: the plan is remade from
/// the new state after every cast, so a monster that behaves differently from
/// the model is re-planned around rather than followed off a cliff. A plan
/// whose first step is a self-heal means "do not drain at all", and the caller
/// falls back to an ordinary recharge.
/// </summary>
internal static class CorpseDrainPlan
{
    /// <summary>The martyr spell that strikes everything around the caster.</summary>
    private const uint RingMartyrSpellId = 3818u;

    /// <summary>Below this the monster is not worth draining any further.</summary>
    private const int DrainFloorPoints = 25;

    /// <summary>How much the search budget grows each round.</summary>
    private const int BudgetStepMilliseconds = 600;

    /// <summary>How many plans the search may look at before giving up.</summary>
    private const int NodeBudget = 2000;

    /// <summary>
    /// The search is only worth running against a monster that is nearly
    /// down; above this the greedy one-step choice decides.
    /// </summary>
    private const int SearchTargetHealthCeiling = 300;

    /// <summary>What a martyr step is worth to the greedy chooser, and what it
    /// is charged for it.</summary>
    private const double MartyrBias = 180d;
    private const double MartyrBiasCost = 3200d;

    /// <summary>
    /// The spell to cast now, or zero for "nothing here is better than an
    /// ordinary heal".
    /// </summary>
    public static uint SelectSpell(
        int health,
        int maximumHealth,
        int healthFloor,
        int targetHealth,
        bool canDrain,
        bool ring,
        IReadOnlyList<VtankDrainSpellOption> drains,
        IReadOnlyList<VtankMartyrSpellOption> martyrs,
        Func<uint, bool> castable)
    {
        ArgumentNullException.ThrowIfNull(drains);
        ArgumentNullException.ThrowIfNull(martyrs);
        ArgumentNullException.ThrowIfNull(castable);

        var steps = new List<CorpseDrainStep>();
        if (canDrain)
        {
            foreach (VtankDrainSpellOption option in drains)
            {
                if (castable(option.SpellId))
                {
                    steps.Add(new CorpseDrainDrainStep(
                        option.SpellId,
                        option.CastTimeMilliseconds,
                        option.EnemyDrainFactor,
                        option.ResultMultiplier,
                        option.EnemyDrainMaximumPoints));
                }
            }
        }
        foreach (VtankMartyrSpellOption option in martyrs)
        {
            bool isRingMartyr = option.SpellId == RingMartyrSpellId;
            if (ring != isRingMartyr || !castable(option.SpellId))
                continue;
            steps.Add(new CorpseDrainMartyrStep(
                option.SpellId,
                option.CastTimeMilliseconds,
                option.SelfDrainFactor,
                option.ResultMultiplier));
        }
        steps.Add(new CorpseDrainHealSelfStep());

        var start = new Node(new CorpseDrainState(
            health,
            maximumHealth,
            targetHealth));
        Node? best = Search(start, healthFloor, steps);
        if (best is null || best.Steps.Count == 0)
            return 0u;
        return best.Steps[0] is CorpseDrainHealSelfStep
            ? 0u
            : best.Steps[0].SpellId;
    }

    private static Node? Search(
        Node start,
        int healthFloor,
        List<CorpseDrainStep> steps)
    {
        Node? best = null;
        int budget = 0;
        int visited = 0;
        if (start.State.TargetHealth < SearchTargetHealthCeiling)
        {
            do
            {
                // Widen the allowance and search again, so the first complete
                // plan found is also a cheap one.
                budget += BudgetStepMilliseconds;
                var stack = new Stack<Node>();
                stack.Push(new Node(new CorpseDrainState(
                    start.State.Health,
                    start.State.MaximumHealth,
                    start.State.TargetHealth)));
                while (stack.Count > 0)
                {
                    Node node = stack.Pop();
                    visited++;
                    if (visited >= NodeBudget)
                        return best ?? Greedy(start, healthFloor, steps);
                    bool anyWithinFloor = false;
                    foreach (CorpseDrainStep step in steps)
                    {
                        if (Skip(node.State, step))
                            continue;
                        Node next = node.Apply(step);
                        if (next.Score > budget
                            || next.State.Health < healthFloor)
                        {
                            continue;
                        }
                        anyWithinFloor = true;
                        Consider(next, ref best, stack);
                    }
                    if (anyWithinFloor)
                        continue;
                    // Nothing keeps the caster above the floor from here, so
                    // the floor is dropped for this node rather than the
                    // branch being abandoned.
                    foreach (CorpseDrainStep step in steps)
                    {
                        if (Skip(node.State, step))
                            continue;
                        Node next = node.Apply(step);
                        if (next.Score > budget)
                            continue;
                        Consider(next, ref best, stack);
                    }
                }
            }
            while (best is null);
        }
        return best ?? Greedy(start, healthFloor, steps);
    }

    private static void Consider(Node next, ref Node? best, Stack<Node> stack)
    {
        if (!next.State.Done)
        {
            stack.Push(next);
            return;
        }
        if (best is null || next.Cost < best.Cost)
            best = next;
    }

    /// <summary>
    /// One step, chosen for the most monster health taken off per millisecond
    /// spent. A step that finishes the monster always wins, and a martyr is
    /// given a head start and charged for it, because its value is not in the
    /// damage of the one cast.
    /// </summary>
    private static Node Greedy(
        Node start,
        int healthFloor,
        List<CorpseDrainStep> steps)
    {
        Node? chosen = null;
        double bestRate = double.MinValue;
        bool anyWithinFloor = false;
        foreach (CorpseDrainStep step in steps)
        {
            if (Skip(start.State, step))
                continue;
            Node next = start.Apply(step);
            if (next.State.Health < healthFloor)
                continue;
            anyWithinFloor = true;
            Rank(start, next, step, ref chosen, ref bestRate);
        }
        if (!anyWithinFloor)
        {
            foreach (CorpseDrainStep step in steps)
            {
                if (Skip(start.State, step))
                    continue;
                Rank(
                    start,
                    start.Apply(step),
                    step,
                    ref chosen,
                    ref bestRate);
            }
        }
        return chosen ?? start.Apply(new CorpseDrainHealSelfStep());
    }

    private static void Rank(
        Node start,
        Node next,
        CorpseDrainStep step,
        ref Node? chosen,
        ref double bestRate)
    {
        if (chosen is null)
        {
            chosen = next;
            bestRate = 0d;
            return;
        }
        if (!chosen.State.Done)
        {
            if (next.State.Done)
            {
                chosen = next;
                return;
            }
            double bias = step is CorpseDrainMartyrStep ? MartyrBias : 0d;
            double biasCost = step is CorpseDrainMartyrStep ? MartyrBiasCost : 0d;
            double rate =
                (bias + start.State.TargetHealth - next.State.TargetHealth)
                / (biasCost
                    + next.State.CostMilliseconds
                    - start.State.CostMilliseconds);
            if (rate > bestRate)
            {
                bestRate = rate;
                chosen = next;
            }
            return;
        }
        if (next.State.Done
            && next.State.CostMilliseconds < chosen.State.CostMilliseconds)
        {
            chosen = next;
        }
    }

    /// <summary>
    /// At full health there is nothing to drain into and nothing to heal, and
    /// a monster with almost nothing left is not worth a drain.
    /// </summary>
    private static bool Skip(in CorpseDrainState state, CorpseDrainStep step) =>
        (state.Health == state.MaximumHealth
            && step is CorpseDrainDrainStep or CorpseDrainHealSelfStep)
        || (step is CorpseDrainDrainStep
            && state.TargetHealth < DrainFloorPoints);

    private sealed class Node
    {
        public Node(CorpseDrainState state) => State = state;

        private Node(List<CorpseDrainStep> steps, CorpseDrainState state)
        {
            Steps = steps;
            State = state;
        }

        public List<CorpseDrainStep> Steps { get; } = [];

        public CorpseDrainState State { get; }

        public Node Apply(CorpseDrainStep step)
        {
            var steps = new List<CorpseDrainStep>(Steps.Count + 1);
            steps.AddRange(Steps);
            steps.Add(step);
            return new Node(steps, step.Apply(State));
        }

        /// <summary>Fewer casts and less time both count.</summary>
        public int Cost => Steps.Count + State.CostMilliseconds;

        /// <summary>
        /// What this plan is still worth pursuing: the time already spent plus
        /// a charge for the monster health still standing.
        /// </summary>
        public int Score => State.CostMilliseconds + (1000 * State.TargetHealth / 106);
    }
}

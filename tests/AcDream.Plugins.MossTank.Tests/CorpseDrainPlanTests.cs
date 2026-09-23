namespace AcDream.Plugins.MossTank.Tests;

public sealed class CorpseDrainPlanTests
{
    private static readonly VtankGameInfoDatabase GameInfo =
        VtankGameInfoDatabase.Parse(File.ReadAllText(Path.Combine(
            AppContext.BaseDirectory,
            "Fixtures",
            "vtank",
            "gameinfodb-excerpt.ugd")));

    private static readonly uint[] DrainSpells = [1237u, 1238u, 1239u];
    private static readonly uint[] MartyrSpells = [2760u, 2761u, 2762u];

    private static uint Select(
        int health,
        int maximumHealth,
        int healthFloor,
        int targetHealth,
        bool canDrain = true,
        bool ring = false,
        Func<uint, bool>? castable = null) =>
        CorpseDrainPlan.SelectSpell(
            health,
            maximumHealth,
            healthFloor,
            targetHealth,
            canDrain,
            ring,
            GameInfo.DrainSpellOptions,
            GameInfo.MartyrSpellOptions,
            castable ?? (static _ => true));

    /// <summary>
    /// Mutation: rank the steps by health taken off alone (divide by 1 instead
    /// of by the milliseconds the step costs) and the last assertion answers
    /// 2762 — the three drains rank the same way under both orderings, so it
    /// takes the martyrs, where they disagree, to tell the two apart.
    /// </summary>
    [Fact]
    public void TheGreedyStepIsTheMostHealthTakenOffPerMillisecondSpent()
    {
        // 900 of 1000 health, 400 left on the monster, floor at 749. Every
        // martyr would drop the caster under the floor, so the three drains
        // compete on rate: 25/1200, 45/1800 and 70/2300. The last wins.
        Assert.Equal(1239u, Select(900, 1000, 749, 400));

        // Take the winner away and the next-best rate takes it.
        Assert.Equal(
            1238u,
            Select(900, 1000, 749, 400, castable: id => id != 1239u));

        // Drop the floor to 600 and the three martyrs join the race. Now the
        // two orderings disagree: the slowest martyr takes the most health off
        // (189 points against 135), but the fastest one takes off more per
        // millisecond, and it is the fastest one the planner picks.
        Assert.Equal(2760u, Select(900, 1000, 600, 400));
    }

    /// <summary>
    /// Mutation: ignore the drainable flag and a drain is planned against a
    /// monster no drain can touch, so the cast is thrown away.
    /// </summary>
    [Fact]
    public void AMonsterNothingMagicalCanTouchIsNeverDrained()
    {
        uint chosen = Select(900, 1000, 749, 400, canDrain: false);

        Assert.DoesNotContain(chosen, DrainSpells);
    }

    /// <summary>
    /// Mutation: let a drain be planned at full health and the caster throws
    /// health it cannot receive, instead of spending its own on a martyr.
    /// </summary>
    [Fact]
    public void AtFullHealthThereIsNothingToDrainIntoAndNothingToHeal()
    {
        uint chosen = Select(1000, 1000, 749, 400);

        Assert.Contains(chosen, MartyrSpells);
    }

    /// <summary>
    /// Mutation: drop the ring filter and a single-target martyr is planned
    /// for a ring, which would hit one monster where the profile wanted all.
    /// </summary>
    [Fact]
    public void ARingPlanMayOnlyUseTheRingMartyr()
    {
        // The database lists no ring martyr, so a ring plan at full health has
        // nothing at all it may cast.
        Assert.Equal(0u, Select(1000, 1000, 749, 400, ring: true));
    }

    /// <summary>
    /// Mutation: drop <c>castable(...)</c> from the drain and martyr
    /// admissions and this answers 1239 — a spell the caster cannot cast at
    /// all. The "a heal means zero" arm itself has no distinguishing mutation:
    /// the heal step carries spell id 0, so deleting the arm answers zero
    /// anyway. The arm is there to say what the zero means, and the
    /// assertion below pins the answer, not the arm.
    /// </summary>
    [Fact]
    public void APlanWhoseFirstStepIsAHealMeansDoNotDrain()
    {
        // Nothing is castable, so the only step left is the self-heal, and
        // that is the planner's way of saying "recharge instead".
        Assert.Equal(
            0u,
            Select(900, 1000, 749, 400, castable: static _ => false));
    }

    /// <summary>
    /// Mutation: skip the bounded search under the health threshold and the
    /// nearly-dead monster is left to a drain that cannot finish it.
    /// </summary>
    [Fact]
    public void ANearlyDeadMonsterGetsTheSearchAndAFinishingPlan()
    {
        // Under 300 the bounded search runs. A monster with 20 left is below
        // the drain floor, so only a martyr can finish it, and one does.
        uint chosen = Select(900, 1000, 0, 20);

        Assert.Contains(chosen, MartyrSpells);
    }
}

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
    /// Mutation: rank the steps by health taken off alone, ignoring the time
    /// each takes, and the first assertion picks the wrong drain.
    /// </summary>
    [Fact]
    public void TheGreedyStepIsTheMostHealthTakenOffPerMillisecondSpent()
    {
        // 900 of 1000 health, 400 left on the monster, floor at 749. Every
        // martyr would drop the caster under the floor, so the three drains
        // compete on rate: 30/1300, 50/1900 and 75/2350. The last wins.
        Assert.Equal(1239u, Select(900, 1000, 749, 400));

        // Take the winner away and the next-best rate takes it.
        Assert.Equal(
            1238u,
            Select(900, 1000, 749, 400, castable: id => id != 1239u));
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
    /// Mutation: answer the first castable spell rather than zero when the
    /// plan opens with a self-heal, and the caster drains itself under the
    /// floor instead of recharging.
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

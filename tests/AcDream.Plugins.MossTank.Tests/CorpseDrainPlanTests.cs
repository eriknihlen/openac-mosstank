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

    [Fact]
    public void AMonsterNothingMagicalCanTouchIsNeverDrained()
    {
        uint chosen = Select(900, 1000, 749, 400, canDrain: false);

        Assert.DoesNotContain(chosen, DrainSpells);
    }

    [Fact]
    public void AtFullHealthThereIsNothingToDrainIntoAndNothingToHeal()
    {
        uint chosen = Select(1000, 1000, 749, 400);

        Assert.Contains(chosen, MartyrSpells);
    }

    [Fact]
    public void ARingPlanMayOnlyUseTheRingMartyr()
    {
        // The database lists no ring martyr, so a ring plan at full health has
        // nothing at all it may cast.
        Assert.Equal(0u, Select(1000, 1000, 749, 400, ring: true));
    }

    [Fact]
    public void APlanWhoseFirstStepIsAHealMeansDoNotDrain()
    {
        // Nothing is castable, so the only step left is the self-heal, and
        // that is the planner's way of saying "recharge instead".
        Assert.Equal(
            0u,
            Select(900, 1000, 749, 400, castable: static _ => false));
    }

    [Fact]
    public void ANearlyDeadMonsterGetsTheSearchAndAFinishingPlan()
    {
        // Under 300 the bounded search runs. A monster with 20 left is below
        // the drain floor, so only a martyr can finish it, and one does.
        uint chosen = Select(900, 1000, 0, 20);

        Assert.Contains(chosen, MartyrSpells);
    }
}

using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank.Tinkering;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The tinkering arithmetic, against numbers worked out by hand from the
/// formula rather than read back off the code. Each one states its own
/// working, so a reader can check the expectation without running anything.
/// </summary>
public sealed class TinkerCalcTests
{
    /// <summary>
    /// Iron on a workmanship-6 item with a workmanship-7.4 bag, first
    /// attempt. Iron costs 12. The bag is better than the item, so its
    /// discount counts double.
    /// <code>
    /// (12 x 5) + (6 x 12 x 2) - (7.4 x 2 x 12 / 5) = 60 + 144 - 35.52
    ///                                             = 168.48, floored to 168
    /// </code>
    /// </summary>
    [Fact]
    public void TheDifficultyOfAFirstIronTinkIsTheMaterialCostPlusTheItemLessTheBag()
    {
        Assert.Equal(
            168,
            TinkerCalc.Difficulty(
                (int)TinkerMaterial.Iron,
                salvageWorkmanship: 7.4d,
                itemWorkmanship: 6d,
                attemptsAlreadyTaken: 0));
    }

    /// <summary>
    /// The same attempt at an effective weapon tinkering of 300.
    /// <code>
    /// 1 - 1 / (1 + e^(0.03 x (300 - 168))) = 1 - 1 / (1 + e^3.96)
    ///                                      = 1 - 1 / 53.4573
    ///                                      = 0.98129
    /// </code>
    /// </summary>
    [Fact]
    public void ASkillOfThreeHundredAgainstThatDifficultyIsANinetyEightPercentChance()
    {
        Assert.Equal(
            0.98129d,
            TinkerCalc.SuccessChance(
                (int)TinkerMaterial.Iron,
                salvageWorkmanship: 7.4d,
                itemWorkmanship: 6d,
                attemptsAlreadyTaken: 0,
                skill: 300),
            5);
    }

    /// <summary>
    /// A bag no better than the item takes the single discount, not the
    /// double one. Black garnet costs 20; the item is workmanship 10 and the
    /// bag 9.5, so the discount is 9.5 x 1 x 20 / 5 = 38.
    /// <code>
    /// (20 x 5) + (10 x 20 x 2) - 38 = 100 + 400 - 38 = 462
    /// </code>
    /// </summary>
    [Fact]
    public void ABagNoBetterThanTheItemTakesTheSingleWorkmanshipDiscount()
    {
        Assert.Equal(
            462,
            TinkerCalc.Difficulty(
                (int)TinkerMaterial.BlackGarnet,
                salvageWorkmanship: 9.5d,
                itemWorkmanship: 10d,
                attemptsAlreadyTaken: 0));
    }

    /// <summary>
    /// An imbue rolls at a third of the ordinary chance, and Charmed Smith
    /// adds five points on top.
    /// <code>
    /// 1 - 1 / (1 + e^(0.03 x (500 - 462))) = 1 - 1 / (1 + e^1.14)
    ///                                      = 1 - 1 / 4.126768 = 0.757680
    /// 0.757680 / 3            = 0.252560
    /// 0.252560 + 0.05         = 0.302560
    /// </code>
    /// </summary>
    [Fact]
    public void AnImbueIsAThirdOfTheOrdinaryChanceAndCharmedSmithAddsFivePoints()
    {
        Assert.Equal(
            0.25256d,
            TinkerCalc.SuccessChance(
                (int)TinkerMaterial.BlackGarnet,
                salvageWorkmanship: 9.5d,
                itemWorkmanship: 10d,
                attemptsAlreadyTaken: 0,
                skill: 500),
            5);
        Assert.Equal(
            0.30256d,
            TinkerCalc.SuccessChance(
                (int)TinkerMaterial.BlackGarnet,
                salvageWorkmanship: 9.5d,
                itemWorkmanship: 10d,
                attemptsAlreadyTaken: 0,
                skill: 500,
                charmedSmith: true),
            5);
    }

    /// <summary>
    /// The sixth attempt on an item is two and a half times as hard as the
    /// first: 462 x 2.5 = 1155.
    /// </summary>
    [Fact]
    public void TheSixthAttemptIsTwoAndAHalfTimesTheFirst()
    {
        Assert.Equal(2.5d, TinkerCalc.AttemptModifier(5));
        Assert.Equal(
            1155,
            TinkerCalc.Difficulty(
                (int)TinkerMaterial.BlackGarnet,
                salvageWorkmanship: 9.5d,
                itemWorkmanship: 10d,
                attemptsAlreadyTaken: 5));
    }

    /// <summary>
    /// The ten rows, in the order the reference calculator has them, and a
    /// clamp at either end so an item the server says has been tinkered more
    /// than ten times cannot index past the table.
    /// </summary>
    [Fact]
    public void TheAttemptTableIsTheTenRowsItWasPortedFrom()
    {
        Assert.Equal(
            [1.0d, 1.1d, 1.3d, 1.6d, 2.0d, 2.5d, 3.0d, 3.5d, 4.0d, 4.5d],
            TinkerCalc.AttemptModifiers);
        Assert.Equal(1.0d, TinkerCalc.AttemptModifier(-3));
        Assert.Equal(4.5d, TinkerCalc.AttemptModifier(99));
    }

    /// <summary>
    /// Equal skill and difficulty is an even chance, and the curve is
    /// clamped rather than allowed to run past certainty either way.
    /// </summary>
    [Fact]
    public void EqualSkillAndDifficultyIsAnEvenChanceAndTheCurveIsClamped()
    {
        Assert.Equal(0.5d, TinkerCalc.SkillChance(400, 400), 10);
        Assert.InRange(TinkerCalc.SkillChance(4000, 0), 0d, 1d);
        Assert.InRange(TinkerCalc.SkillChance(0, 4000), 0d, 1d);
    }

    /// <summary>
    /// On a weapon with a ceiling of 40 and a quarter spread, iron is worth
    /// more than granite:
    /// <code>
    /// iron:    max 41, var 0.25 -> min 30.75, avg 35.875, crit 82
    ///          0.9 x 35.875 + 0.1 x 82 = 40.4875
    /// granite: max 40, var 0.20 -> min 32,    avg 36,     crit 80
    ///          0.9 x 36     + 0.1 x 80 = 40.4
    /// </code>
    /// </summary>
    [Fact]
    public void IronBeatsGraniteOnAFortyDamageQuarterSpreadWeapon()
    {
        Assert.Equal(40.4875d, TinkerCalc.DamagePerSecond(41d, 0.25d), 6);
        Assert.Equal(40.4d, TinkerCalc.DamagePerSecond(40d, 0.2d), 6);
        Assert.True(TinkerCalc.IronBeatsGranite(40d, 0.25d));
    }

    /// <summary>
    /// A wide spread is worth narrowing: at half spread granite wins.
    /// <code>
    /// iron:    max 21, var 0.5  -> min 10.5, avg 15.75, crit 42
    ///          0.9 x 15.75 + 0.1 x 42 = 18.375
    /// granite: max 20, var 0.4  -> min 12,   avg 16,    crit 40
    ///          0.9 x 16    + 0.1 x 40 = 18.4
    /// </code>
    /// </summary>
    [Fact]
    public void GraniteBeatsIronWhenTheSpreadIsWideEnoughToBeWorthNarrowing()
    {
        Assert.Equal(18.375d, TinkerCalc.DamagePerSecond(21d, 0.5d), 6);
        Assert.Equal(18.4d, TinkerCalc.DamagePerSecond(20d, 0.4d), 6);
        Assert.False(TinkerCalc.IronBeatsGranite(20d, 0.5d));
    }

    /// <summary>
    /// Each material rolls against one of the four tinkering skills, and
    /// anything that is not salvage rolls against none.
    /// </summary>
    [Fact]
    public void EachMaterialNamesTheTinkeringSkillItRollsAgainst()
    {
        Assert.Equal(28u, TinkerType.SkillId((int)TinkerMaterial.Iron));
        Assert.Equal(29u, TinkerType.SkillId((int)TinkerMaterial.Steel));
        Assert.Equal(30u, TinkerType.SkillId((int)TinkerMaterial.Opal));
        Assert.Equal(18u, TinkerType.SkillId((int)TinkerMaterial.Gold));
        Assert.Equal(0u, TinkerType.SkillId((int)TinkerMaterial.Leather));
    }

    /// <summary>
    /// The four difficulty bands, at their edges: gold and oak alone are ten,
    /// the eight spell-power gems are twenty-five, and everything the tables
    /// do not name is twenty.
    /// </summary>
    [Fact]
    public void TheMaterialCostBandsAreTheFourTheyWerePortedFrom()
    {
        Assert.Equal(10d, TinkerType.MaterialModifier((int)TinkerMaterial.Gold));
        Assert.Equal(10d, TinkerType.MaterialModifier((int)TinkerMaterial.Oak));
        Assert.Equal(11d, TinkerType.MaterialModifier((int)TinkerMaterial.Linen));
        Assert.Equal(12d, TinkerType.MaterialModifier((int)TinkerMaterial.Iron));
        Assert.Equal(25d, TinkerType.MaterialModifier((int)TinkerMaterial.RedJade));
        Assert.Equal(20d, TinkerType.MaterialModifier((int)TinkerMaterial.Diamond));
    }

    /// <summary>
    /// The ten imbue salvages, and the one of them a casting implement
    /// cannot take: sunstone raises a critical chance, which a wand has no
    /// use for.
    /// </summary>
    [Fact]
    public void OnlyTheTenImbueSalvagesImbueAndSunstoneAloneLeavesWandsOut()
    {
        int[] imbues = TinkerMaterials.Named
            .Where(static material =>
                TinkerType.SalvageKind(material) == TinkerType.ImbueSalvage)
            .ToArray();
        Assert.Equal(10, imbues.Length);
        Assert.Contains((int)TinkerMaterial.BlackOpal, imbues);
        Assert.Contains((int)TinkerMaterial.Sunstone, imbues);
        Assert.DoesNotContain((int)TinkerMaterial.Iron, imbues);

        Assert.Contains(
            PluginObjectClass.WandStaffOrb,
            TinkerType.ImbueTargets((int)TinkerMaterial.BlackOpal));
        Assert.DoesNotContain(
            PluginObjectClass.WandStaffOrb,
            TinkerType.ImbueTargets((int)TinkerMaterial.Sunstone));
        Assert.Empty(TinkerType.ImbueTargets((int)TinkerMaterial.Iron));
    }

    /// <summary>
    /// Material names go both ways, and the five group headings name nothing,
    /// so an item made of "metal" gets no material in front of its name and
    /// no salvage choice of its own.
    /// </summary>
    [Fact]
    public void MaterialNamesResolveBothWaysAndTheGroupHeadingsNameNothing()
    {
        Assert.Equal("Iron", TinkerMaterials.Name((int)TinkerMaterial.Iron));
        Assert.Equal((int)TinkerMaterial.Iron, TinkerMaterials.Id("iron"));
        Assert.Equal((int)TinkerMaterial.GreenGarnet, TinkerMaterials.Id("Green Garnet"));
        Assert.Equal(string.Empty, TinkerMaterials.Name((int)TinkerMaterial.Metal));
        Assert.Equal(0, TinkerMaterials.Id("Metal"));
        Assert.DoesNotContain((int)TinkerMaterial.Metal, TinkerMaterials.Named);
    }

    /// <summary>
    /// Every damage type a rend can be chosen for names the salvage that
    /// rends it, and the bit the server uses for it.
    /// </summary>
    [Fact]
    public void EachDamageTypeNamesTheSalvageThatRendsIt()
    {
        Assert.Equal("Emerald", TinkerJobManager.DefaultSalvageName("Acid"));
        Assert.Equal("White Sapphire", TinkerJobManager.DefaultSalvageName("Bludgeoning"));
        Assert.Equal("Aquamarine", TinkerJobManager.DefaultSalvageName("Cold"));
        Assert.Equal("Jet", TinkerJobManager.DefaultSalvageName("Electric"));
        Assert.Equal("Red Garnet", TinkerJobManager.DefaultSalvageName("Fire"));
        Assert.Equal("Black Opal", TinkerJobManager.DefaultSalvageName("Nether"));
        Assert.Equal("Black Garnet", TinkerJobManager.DefaultSalvageName("Piercing"));
        Assert.Equal("Imperial Topaz", TinkerJobManager.DefaultSalvageName("Slashing"));
        Assert.Equal("Imperial Topaz", TinkerJobManager.DefaultSalvageName("SlashPierce"));
        Assert.Equal("Black Opal", TinkerJobManager.DefaultSalvageName("Normal"));
        Assert.Equal(string.Empty, TinkerJobManager.DefaultSalvageName("Bogus"));

        Assert.Equal(1, TinkerJobManager.DamageBit("Slashing"));
        Assert.Equal(1024, TinkerJobManager.DamageBit("Nether"));
        Assert.Equal(-1, TinkerJobManager.DamageBit("Bogus"));
    }
}

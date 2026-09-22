namespace AcDream.Plugins.MossTank.Tinkering;

/// <summary>
/// The odds of one tinkering attempt. Everything here is a plain function of
/// four numbers -- what the salvage is made of, how good the salvage is, how
/// good the item is, and how many attempts the item has already taken -- plus
/// the character's skill, so it can be worked out for an item nobody is
/// holding and checked against a calculator by hand.
/// </summary>
internal static class TinkerCalc
{
    /// <summary>
    /// How much harder each successive attempt on the same item is. The first
    /// attempt is the plain difficulty; the tenth is four and a half times
    /// it. Indexed by how many attempts the item has already taken, so an
    /// untouched item reads the first row.
    /// </summary>
    internal static IReadOnlyList<double> AttemptModifiers { get; } =
    [
        1.0d,   // 1st attempt
        1.1d,   // 2nd
        1.3d,   // 3rd
        1.6d,   // 4th
        2.0d,   // 5th
        2.5d,   // 6th
        3.0d,   // 7th
        3.5d,   // 8th
        4.0d,   // 9th
        4.5d,   // 10th
    ];

    /// <summary>The most attempts any one item will take.</summary>
    internal const int MaximumAttempts = 10;

    /// <summary>
    /// How much of a chance a skill of this level has against a difficulty of
    /// that level: an S-curve through a half chance at equal numbers, a
    /// little over 95% thirty-three points above it and a little under 5%
    /// thirty-three below. Clamped to 0..1.
    /// </summary>
    /// <param name="skill">The character's effective skill.</param>
    /// <param name="difficulty">What the attempt is rated at.</param>
    /// <param name="factor">How sharp the curve is.</param>
    internal static double SkillChance(int skill, int difficulty, double factor = 0.03d)
    {
        double chance = 1d - (1d / (1d + Math.Exp(factor * (skill - difficulty))));
        return Math.Min(1d, Math.Max(0d, chance));
    }

    /// <summary>
    /// The attempt's difficulty rating: what the material costs, plus twice
    /// that for every point of the item's workmanship, less a fifth of it for
    /// every point of the salvage's -- and salvage at least as good as the
    /// item counts double there. The whole thing is then multiplied by how
    /// many attempts the item has already taken, and rounded down.
    /// </summary>
    /// <param name="salvageMaterial">The material id of the bag.</param>
    /// <param name="salvageWorkmanship">The bag's average workmanship.</param>
    /// <param name="itemWorkmanship">The item's workmanship.</param>
    /// <param name="attemptsAlreadyTaken">
    /// How many tinkers the item has already taken, 0 through 9.
    /// </param>
    internal static int Difficulty(
        int salvageMaterial,
        double salvageWorkmanship,
        double itemWorkmanship,
        int attemptsAlreadyTaken)
    {
        double materialModifier = TinkerType.MaterialModifier(salvageMaterial);
        double attemptModifier = AttemptModifier(attemptsAlreadyTaken);
        double workmanshipModifier = salvageWorkmanship >= itemWorkmanship ? 2d : 1d;
        double rating =
            ((materialModifier * 5d)
                + (itemWorkmanship * materialModifier * 2d)
                - (salvageWorkmanship * workmanshipModifier * materialModifier / 5d))
            * attemptModifier;
        return (int)Math.Floor(rating);
    }

    /// <summary>
    /// The share of attempts like this one that succeed, 0 through 1. An
    /// imbue is a third of the ordinary chance, and the Charmed Smith
    /// augmentation adds five points to an imbue on top of that.
    /// </summary>
    /// <param name="salvageMaterial">The material id of the bag.</param>
    /// <param name="salvageWorkmanship">The bag's average workmanship.</param>
    /// <param name="itemWorkmanship">The item's workmanship.</param>
    /// <param name="attemptsAlreadyTaken">
    /// How many tinkers the item has already taken, 0 through 9.
    /// </param>
    /// <param name="skill">
    /// The character's effective level in the skill
    /// <see cref="TinkerType.SkillId"/> names for this material.
    /// </param>
    /// <param name="charmedSmith">
    /// True when the character carries the Charmed Smith augmentation.
    /// </param>
    internal static double SuccessChance(
        int salvageMaterial,
        double salvageWorkmanship,
        double itemWorkmanship,
        int attemptsAlreadyTaken,
        int skill,
        bool charmedSmith = false)
    {
        int difficulty = Difficulty(
            salvageMaterial,
            salvageWorkmanship,
            itemWorkmanship,
            attemptsAlreadyTaken);
        double chance = SkillChance(skill, difficulty);
        if (TinkerType.SalvageKind(salvageMaterial) != TinkerType.ImbueSalvage)
            return chance;

        chance /= 3d;
        if (charmedSmith)
            chance += 0.05d;
        return chance;
    }

    /// <summary>
    /// Damage per second as the salvage chooser weighs it: nine swings in ten
    /// land somewhere between the weapon's floor and its ceiling, and the
    /// tenth crits for twice the ceiling.
    /// </summary>
    /// <param name="maximumDamage">The top of the weapon's damage roll.</param>
    /// <param name="variance">
    /// How far below that a swing can land, as a share of it.
    /// </param>
    internal static double DamagePerSecond(double maximumDamage, double variance)
    {
        double minimumDamage = maximumDamage * (1d - variance);
        double criticalDamage = maximumDamage * 2d;
        double averageDamage = (maximumDamage + minimumDamage) / 2d;
        return (0.9d * averageDamage) + (0.1d * criticalDamage);
    }

    /// <summary>
    /// Which of iron and granite is worth more on this weapon right now: iron
    /// adds a point to the ceiling, granite takes a fifth off the spread. Ties
    /// go to iron, as the reference calculator does.
    /// </summary>
    internal static bool IronBeatsGranite(double maximumDamage, double variance) =>
        DamagePerSecond(maximumDamage + 1d, variance)
        >= DamagePerSecond(maximumDamage, variance * 0.8d);

    /// <summary>
    /// The multiplier for an item that has already taken this many attempts,
    /// clamped to the ten rows the table has.
    /// </summary>
    internal static double AttemptModifier(int attemptsAlreadyTaken)
    {
        int row = Math.Clamp(attemptsAlreadyTaken, 0, AttemptModifiers.Count - 1);
        return AttemptModifiers[row];
    }
}

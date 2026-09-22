namespace AcDream.Plugins.MossTank;

internal readonly record struct CombatDebuffStep(
    DebuffIdentity Identity,
    bool ZeroTolerance);

internal static class CombatDebuffChain
{
    private static readonly (MonsterActionFlags Flag, bool ZeroTolerance)[] Order =
    [
        (MonsterActionFlags.Yield, false),            //  1
        (MonsterActionFlags.WeakeningCurse, false),   //  2
        (MonsterActionFlags.FesteringCurse, false),   //  3
        (MonsterActionFlags.Corruption, true),        //  4
        (MonsterActionFlags.DestructiveCurse, true),  //  5
        (MonsterActionFlags.Corrosion, true),         //  6
        (MonsterActionFlags.Imperil, false),          //  7
        (MonsterActionFlags.Vulnerability, false),    //  8  attack element
        (MonsterActionFlags.Vulnerability, false),    //  9  Ex. Vuln
        (MonsterActionFlags.GravityWell, false),      // 10
        (MonsterActionFlags.Broadside, false),        // 11
        (MonsterActionFlags.Fester, false),           // 12
    ];

    public static int OrderOf(MonsterActionFlags flag)
    {
        for (int i = 0; i < Order.Length; i++)
        {
            if (Order[i].Flag == flag)
                return i;
        }
        return int.MaxValue;
    }

    private const int NaturalVulnerabilityStep = 7;
    private const int ExtraVulnerabilityStep = 8;

    /// <summary>
    /// Whether step 9, the extra vulnerability, is part of the chain at all.
    /// It stands on its own: unlike step 8 it is NOT gated on the
    /// vulnerability column, and it is present exactly when its column names
    /// an element. "Automatic" is not an element until it has been resolved
    /// against the monster, and a column that resolved to nothing - the
    /// monster has no listed weakness, or the column names something that is
    /// not an element at all - contributes no step.
    /// This is the one rule: every caller asks here rather than spelling the
    /// test out again.
    /// </summary>
    public static bool HasExtraVulnerability(MonsterDamageType element) =>
        element is not (MonsterDamageType.None or MonsterDamageType.Auto);

    public static IReadOnlyList<CombatDebuffStep> Build(
        MonsterRuleActions actions,
        MonsterDamageType attackElement,
        MonsterDamageType extraVulnerability)
    {
        ArgumentNullException.ThrowIfNull(actions);
        var steps = new List<CombatDebuffStep>(Order.Length);
        for (int i = 0; i < Order.Length; i++)
        {
            (MonsterActionFlags flag, bool zeroTolerance) = Order[i];
            MonsterDamageType element = MonsterDamageType.Auto;
            if (i == NaturalVulnerabilityStep)
            {
                if ((actions.Flags & MonsterActionFlags.Vulnerability) == 0
                    || attackElement is MonsterDamageType.None
                        or MonsterDamageType.Auto)
                {
                    continue;
                }
                element = attackElement;
            }
            else if (i == ExtraVulnerabilityStep)
            {
                if (!HasExtraVulnerability(extraVulnerability))
                    continue;
                element = extraVulnerability;
            }
            else if ((actions.Flags & flag) == 0)
            {
                continue;
            }
            steps.Add(new CombatDebuffStep(
                new DebuffIdentity(flag, element),
                zeroTolerance));
        }
        return steps;
    }

    public static CombatDebuffStep? Choose(
        IReadOnlyList<CombatDebuffStep> steps,
        Func<CombatDebuffStep, bool> isDue)
    {
        ArgumentNullException.ThrowIfNull(steps);
        ArgumentNullException.ThrowIfNull(isDue);
        foreach (CombatDebuffStep step in steps)
        {
            if (isDue(step))
                return step;
        }
        return null;
    }

    public static bool NeedsDebuff(
        IReadOnlyList<CombatDebuffStep> steps,
        Func<CombatDebuffStep, bool> isDue) =>
        Choose(steps, isDue) is not null;
}

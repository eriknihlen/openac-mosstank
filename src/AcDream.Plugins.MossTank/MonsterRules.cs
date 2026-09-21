namespace AcDream.Plugins.MossTank;

[Flags]
internal enum MonsterActionFlags
{
    None = 0,
    Fester = 1 << 0,
    Broadside = 1 << 1,
    GravityWell = 1 << 2,
    Imperil = 1 << 3,
    Yield = 1 << 4,
    Vulnerability = 1 << 5,
    Attack = 1 << 6,
    Ring = 1 << 7,
    Streak = 1 << 8,
    WeakeningCurse = 1 << 9,
    FesteringCurse = 1 << 10,
    Corruption = 1 << 11,
    DestructiveCurse = 1 << 12,
    Corrosion = 1 << 13,
}

internal enum MonsterDamageType
{
    Auto = 0,
    Slash,
    Pierce,
    Bludgeon,
    Cold,
    Fire,
    Acid,
    Electric,
    Nether,
    VoidBasic,
    DrainAuto,
    Harm,
    None,
    PlayerAuto,
    Prismatic,
    Random,
    Fists,
    Physical,
}

internal sealed record MonsterRuleActions
{
    public MonsterActionFlags Flags { get; init; } = MonsterActionFlags.Attack;
    public int Priority { get; init; }
    public MonsterDamageType DamageType { get; init; } = MonsterDamageType.Auto;

    /// <summary>
    /// The extra vulnerability column, which is OFF unless the row asks for
    /// it. The attack element defaults to automatic because a rule that says
    /// nothing still has to pick something to hit with; the extra
    /// vulnerability is a second element debuff stacked on top of the chain,
    /// so a rule that says nothing must ask for nothing. Automatic here would
    /// resolve to the monster's first listed weakness and quietly debuff
    /// almost every monster, which is not what a fresh row means.
    /// </summary>
    public MonsterDamageType ExtraVulnerability { get; init; } =
        MonsterDamageType.None;
    public uint WeaponObjectId { get; init; }
    public uint OffhandObjectId { get; init; }

    public int WeaponToUseRaw { get; init; } = -1;

    public int SecondaryEquipRaw { get; init; }
    public string WeaponName { get; init; } = string.Empty;
    public string OffhandName { get; init; } = string.Empty;
    public MonsterDamageType PetDamageType { get; init; } =
        MonsterDamageType.PlayerAuto;

    /// <summary>
    /// The columns a brand-new monster row starts with: priority one, attack
    /// and streak ticked, automatic attack element, no extra vulnerability,
    /// automatic weapon and off-hand, and an automatic pet element. Both the
    /// DEFAULT row a profile is born with and every row added to it later
    /// start here, so an untouched row behaves the same way whichever editor
    /// wrote the profile, and sorts beside authored rules instead of below
    /// all of them.
    /// </summary>
    public static MonsterRuleActions FreshRow => new()
    {
        Priority = 1,
        Flags = MonsterActionFlags.Attack | MonsterActionFlags.Streak,
        DamageType = MonsterDamageType.Auto,
        ExtraVulnerability = MonsterDamageType.None,
        WeaponToUseRaw = -1,
        SecondaryEquipRaw = (int)VtankSecondaryEquip.Auto,
        PetDamageType = MonsterDamageType.PlayerAuto,
    };

    public int BoundedPriority => Math.Clamp(Priority, -1, 4);
    public bool Attacks => (Flags
        & (MonsterActionFlags.Attack | MonsterActionFlags.Ring)) != 0;
    public bool UsesPrimaryAttack => (Flags & MonsterActionFlags.Attack) != 0;
    public bool UsesRing => (Flags & MonsterActionFlags.Ring) != 0;
    public bool UsesStreak => (Flags & MonsterActionFlags.Streak) != 0;
}

internal sealed class MonsterRule
{
    private readonly MonsterExpression? _compiled;

    public MonsterRule(string expression, int priority)
        : this(expression, new MonsterRuleActions { Priority = priority })
    {
    }

    public MonsterRule(string expression, MonsterRuleActions actions)
    {
        Expression = string.IsNullOrWhiteSpace(expression)
            ? "DEFAULT"
            : expression.Trim();
        Actions = actions ?? throw new ArgumentNullException(nameof(actions));
        if (!IsDefault)
            _compiled = MonsterExpression.Compile(Expression);
    }

    private MonsterRule(MonsterRule source, MonsterRuleActions actions)
    {
        Expression = source.Expression;
        Actions = actions;
        IsIgnoredSpec = source.IsIgnoredSpec;
        _compiled = source._compiled;
    }

    /// <summary>
    /// The same row with different action columns, for the copy a single pass
    /// is allowed to scribble on when it discovers one of them cannot be
    /// carried out against this monster right now. The match expression is
    /// shared, not recompiled.
    /// </summary>
    public MonsterRule WithActions(MonsterRuleActions actions) =>
        new(this, actions ?? throw new ArgumentNullException(nameof(actions)));

    private MonsterRule(string expression, MonsterRuleActions actions, bool ignored)
    {
        Expression = string.IsNullOrWhiteSpace(expression)
            ? "DEFAULT"
            : expression.Trim();
        Actions = actions;
        IsIgnoredSpec = ignored;
    }

    public static MonsterRule Compile(
        string expression,
        MonsterRuleActions actions,
        out string? parseError)
    {
        ArgumentNullException.ThrowIfNull(actions);
        try
        {
            parseError = null;
            return new MonsterRule(expression, actions);
        }
        catch (FormatException error)
        {
            parseError = "Parse error in monster spec: \"" + expression
                + "\", ignoring entry (" + error.Message + ").";
            return new MonsterRule(expression, actions, ignored: true);
        }
    }

    public const string RetailDefaultName = "<DEFAULT>";

    /// <summary>
    /// The fallback row a profile with no DEFAULT row of its own falls back
    /// to, which is the fresh row: attack at priority one, finishing with a
    /// streak.
    /// </summary>
    public static MonsterRule RetailDefault() => Fresh("DEFAULT");

    public static MonsterRule Fresh(string expression) =>
        new(expression, MonsterRuleActions.FreshRow);

    public string Expression { get; }
    public MonsterRuleActions Actions { get; }
    public int Priority => Actions.Priority;
    public bool IsDefault => IsDefaultName(Expression);

    /// <summary>Either spelling of the fallback row's name.</summary>
    public static bool IsDefaultName(string? expression) =>
        expression is not null
        && (expression.Equals("DEFAULT", StringComparison.OrdinalIgnoreCase)
            || expression.Equals(
                RetailDefaultName,
                StringComparison.OrdinalIgnoreCase));
    public bool IsDynamic => _compiled?.IsDynamic == true;

    public bool IsIgnoredSpec { get; }

    public bool Matches(
        in MonsterExpressionContext context,
        out string? error)
    {
        if (_compiled is null)
        {
            error = null;
            return IsDefault && !IsIgnoredSpec;
        }
        return _compiled.IsMatch(context, out error);
    }
}

internal readonly record struct ResolvedMonsterRule(
    MonsterRule Rule,
    string? EvaluationError)
{
    public MonsterRuleActions Actions => Rule.Actions;
    public int Priority => Rule.Priority;
}

internal static class MonsterRuleResolver
{
    internal static ResolvedMonsterRule Resolve(
        IEnumerable<MonsterRule> rules,
        in MonsterExpressionContext context)
    {
        ArgumentNullException.ThrowIfNull(rules);
        MonsterRule? fallback = null;
        string? firstError = null;
        foreach (MonsterRule rule in rules)
        {
            if (rule.IsDefault)
            {
                fallback ??= rule;
                continue;
            }
            if (rule.Matches(context, out string? error))
                return new ResolvedMonsterRule(rule, firstError);
            firstError ??= error;
        }

        fallback ??= MonsterRule.RetailDefault();
        return new ResolvedMonsterRule(fallback, firstError);
    }
}

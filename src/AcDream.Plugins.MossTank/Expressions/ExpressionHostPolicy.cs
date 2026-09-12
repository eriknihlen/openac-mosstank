namespace AcDream.Plugins.MossTank.Expressions;

/// <summary>
/// The two profile-owned facts a handful of built-in expression functions need
/// but the plugin host cannot supply: how much skill headroom over a spell's
/// difficulty the profile insists on, and whether the combat controller still
/// considers a monster worth looking at. Both are optional; an unset hook
/// leaves the function at its neutral answer.
/// </summary>
internal sealed class ExpressionHostPolicy
{
    /// <summary>
    /// Skill required above a spell's difficulty before it counts as castable.
    /// The argument is true for the hunting margin, false for the buff margin.
    /// </summary>
    public Func<bool, int>? SkillMargin { get; set; }

    /// <summary>
    /// False when a monster is tracked-but-blacklisted, so the nearest-monster
    /// lookup skips it the way the combat pass does.
    /// </summary>
    public Func<uint, bool>? MonsterEligibility { get; set; }

    public int Margin(bool hunting) => SkillMargin?.Invoke(hunting) ?? 0;

    public bool IsEligibleMonster(uint objectId) =>
        MonsterEligibility?.Invoke(objectId) ?? true;
}

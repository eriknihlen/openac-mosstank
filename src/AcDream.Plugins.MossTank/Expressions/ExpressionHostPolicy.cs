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
    /// True only when the combat pass is TRACKING this monster and has not
    /// blacklisted it — both halves, not just the blacklist. A monster the
    /// pass has never seen is not eligible, so a hook that answers the
    /// blacklist alone reproduces half the rule and keeps untracked monsters
    /// in the nearest-monster answer.
    /// </summary>
    public Func<uint, bool>? MonsterEligibility { get; set; }

    public int Margin(bool hunting) => SkillMargin?.Invoke(hunting) ?? 0;

    /// <summary>
    /// With no hook set there is no combat pass to ask, so every monster
    /// stays eligible. That default is a deliberate widening of the rule
    /// above, not the rule itself.
    /// </summary>
    public bool IsEligibleMonster(uint objectId) =>
        MonsterEligibility?.Invoke(objectId) ?? true;
}

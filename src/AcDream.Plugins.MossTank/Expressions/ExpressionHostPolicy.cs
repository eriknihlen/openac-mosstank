namespace AcDream.Plugins.MossTank.Expressions;

/// <summary>
/// The profile-owned facts and macro services a handful of built-in
/// expression functions need but the plugin host cannot supply: how much
/// skill headroom over a spell's difficulty the profile insists on, the views
/// metas created, and the macro's own spell caster. All are optional; an
/// unset hook leaves the function at its neutral answer.
/// </summary>
internal sealed class ExpressionHostPolicy
{
    /// <summary>
    /// Skill required above a spell's difficulty before it counts as castable.
    /// The argument is true for the hunting margin, false for the buff margin.
    /// </summary>
    public Func<bool, int>? SkillMargin { get; set; }

    /// <summary>
    /// The views metas created. The ui* functions ask them first and fall
    /// back to the host's own windows for any other name.
    /// </summary>
    public IMetaViewControls? MetaViews { get; set; }

    /// <summary>
    /// The macro's own spell caster: the spell id and, for the on-target form,
    /// the target. The reference sends an expression's cast through the same
    /// cast tracker its buffs and attacks use, and that tracker raises the
    /// macro's busy count, so no rule runs while the cast is in flight. A cast
    /// sent past it leaves the peace-when-idle rule free to change stance in
    /// the middle of the windup, which the server answers with a fizzle.
    /// Unset, the cast goes straight to the host.
    /// </summary>
    public Action<uint, uint?>? BeginCast { get; set; }

    /// <summary>
    /// Whether the UtilityBelt debug setting is on; with it on an error is
    /// reported as the whole exception rather than its message. Unset, off.
    /// </summary>
    public Func<bool>? Debug { get; set; }

    public int Margin(bool hunting) => SkillMargin?.Invoke(hunting) ?? 0;
}

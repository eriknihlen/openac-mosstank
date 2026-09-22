namespace AcDream.Plugins.MossTank;

/// <summary>
/// The kinds of thing a name is hung over, each with its own display group
/// on the UB page. A player moves from <see cref="Player"/> to
/// <see cref="AllegiancePlayer"/> once an appraisal shows a shared monarch,
/// and a monster to <see cref="Pet"/> once it is seen to have an owner.
/// </summary>
internal enum NametagGroup
{
    Player,
    Pet,
    AllegiancePlayer,
    Portal,
    Npc,
    Vendor,
    Monster,
}

/// <summary>
/// One display group's five rows: whether it draws, and the colour and size
/// of the name and of the second line under it.
/// </summary>
/// <remarks>
/// The sizes are kept so the page and a shared profile carry them, but the
/// label surface draws every label at one screen size; they have no effect
/// here until it offers a size.
/// </remarks>
internal readonly record struct NametagGroupSettings(
    bool Enabled,
    uint TagColor,
    float TagSize,
    uint TickerColor,
    float TickerSize);

/// <summary>
/// Everything the name tags read from the UB page, taken as one value so a
/// change anywhere in it is one comparison rather than thirty-seven.
/// </summary>
internal sealed record NametagSettings(
    bool Enabled,
    float MaxRange,
    NametagGroupSettings Player,
    NametagGroupSettings Pet,
    NametagGroupSettings AllegiancePlayer,
    NametagGroupSettings Portal,
    NametagGroupSettings Npc,
    NametagGroupSettings Vendor,
    NametagGroupSettings Monster)
{
    /// <summary>The five rows for one group.</summary>
    public NametagGroupSettings For(NametagGroup group) => group switch
    {
        NametagGroup.Player => Player,
        NametagGroup.Pet => Pet,
        NametagGroup.AllegiancePlayer => AllegiancePlayer,
        NametagGroup.Portal => Portal,
        NametagGroup.Npc => Npc,
        NametagGroup.Vendor => Vendor,
        _ => Monster,
    };

    /// <summary>The catalogue's own defaults, for a session with no page.</summary>
    public static NametagSettings Default { get; } = new(
        Enabled: true,
        MaxRange: 35f,
        Player: new(true, 0xFF00FFFFu, 0.15f, 0xFF00FFFFu, 0.1f),
        Pet: new(true, 0xFF00FFFFu, 0.15f, 0xFF00FFFFu, 0.1f),
        AllegiancePlayer: new(true, 0xFF00FF00u, 0.15f, 0xFF00FF00u, 0.1f),
        Portal: new(true, 0xFF00FF00u, 0.15f, 0xFF00FF00u, 0.1f),
        Npc: new(true, 0xFFFFFF00u, 0.15f, 0xFFFFFF00u, 0.1f),
        Vendor: new(true, 0xFFFF00FFu, 0.15f, 0xFFFF00FFu, 0.1f),
        Monster: new(true, 0xFFFF0000u, 0.15f, 0xFFFF0000u, 0.1f));
}

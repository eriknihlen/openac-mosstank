using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal readonly record struct BuffItemEnchantRow(
    string ItemName,
    string SpellName,
    uint? ObjectId = null,
    uint? SpellId = null)
{
    /// <summary>An authored row that deliberately casts nothing.</summary>
    public bool CastsNothing => SpellId == uint.MaxValue
        || (!SpellId.HasValue && SpellName.Length == 0);

    public bool IsProfiledItemRow => ObjectId.HasValue && SpellId.HasValue;
}

internal static class ItemEnchantDefaults
{
    public const uint MeleeWeapon = 0x00100000u;
    public const uint Shield = 0x00200000u;
    public const uint MissileWeapon = 0x00400000u;
    public const uint Wand = 0x01000000u;
    public const uint TwoHanded = 0x02000000u;

    /// <summary>The defaults a melee weapon gets when it is added.</summary>
    private static readonly string[] MeleeAuras =
    [
        "Aura of Defender Self I",
        "Aura of Blood Drinker Self I",
        "Aura of Swift Killer Self I",
        "Aura of Heart Seeker Self I",
    ];

    private static readonly string[] MissileAuras =
    [
        "Aura of Defender Self I",
        "Aura of Blood Drinker Self I",
        "Aura of Swift Killer Self I",
    ];

    /// <summary>The defaults a wand gets when it is added.</summary>
    private static readonly string[] WandAuras =
    [
        "Aura of Defender Self I",
        "Aura of Hermetic Link Self I",
        "Aura of Spirit Drinker Self I",
    ];

    private static readonly string[] ShieldBanes =
    [
        "Piercing Bane I",      // eDamageElement.Pierce = 0
        "Bludgeon Bane I",      // Bludgeon = 1
        "Blade Bane I",         // Slash = 2
        "Acid Bane I",          // Acid = 3
        "Lightning Bane I",     // Lightning = 4
        "Frost Bane I",
        "Flame Bane I",         // Fire = 6
        "Impenetrability I",    // Physical, appended after the loop
    ];

    public static bool IsProfileEligible(in PluginInventoryItem item) =>
        item.IsPetDevice || DefaultsFor(item.ValidLocations).Length > 0;

    public static IReadOnlyList<string> Rows(
        in PluginInventoryItem item,
        bool noBuffs)
    {
        // A pet device is always a single casts-nothing row.
        if (item.IsPetDevice)
            return [];
        if (noBuffs)
            return [];
        return DefaultsFor(item.ValidLocations);
    }

    private static string[] DefaultsFor(uint validLocations) => validLocations switch
    {
        MeleeWeapon or TwoHanded => MeleeAuras,
        MissileWeapon => MissileAuras,
        Wand => WandAuras,
        Shield => ShieldBanes,
        _ => [],
    };
}

using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tinkering;

/// <summary>
/// What a material does when it is applied to something: which tinkering
/// skill the attempt is rolled against, how hard the material itself makes
/// the attempt, and whether it is an imbue rather than an ordinary tinker.
/// </summary>
/// <remarks>
/// The four tables here are the ones a tinkering calculator needs, and they
/// are the whole of the material's behaviour: everything else in a success
/// roll comes from the item, the bag and the character.
/// </remarks>
internal static class TinkerType
{
    /// <summary>The skill id of Magic Item Tinkering.</summary>
    internal const uint MagicItemTinkeringSkill = 30u;

    /// <summary>The skill id of Armor Tinkering.</summary>
    internal const uint ArmorTinkeringSkill = 29u;

    /// <summary>The skill id of Weapon Tinkering.</summary>
    internal const uint WeaponTinkeringSkill = 28u;

    /// <summary>The skill id of Item Tinkering.</summary>
    internal const uint ItemTinkeringSkill = 18u;

    /// <summary>An ordinary tinker: one of the ten attempts, no imbue.</summary>
    internal const int OrdinarySalvage = 1;

    /// <summary>
    /// An imbue: a rend or a critical bonus burned into the item, allowed
    /// once and only on a weapon.
    /// </summary>
    internal const int ImbueSalvage = 2;

    /// <summary>
    /// Which tinkering skill an attempt with this material is rolled
    /// against; zero when the material is not salvage at all.
    /// </summary>
    internal static uint SkillId(int material) => (TinkerMaterial)material switch
    {
        TinkerMaterial.Agate or TinkerMaterial.Azurite or TinkerMaterial.BlackOpal
            or TinkerMaterial.Bloodstone or TinkerMaterial.Carnelian
            or TinkerMaterial.Citrine or TinkerMaterial.FireOpal
            or TinkerMaterial.GreenGarnet or TinkerMaterial.Hematite
            or TinkerMaterial.LapisLazuli or TinkerMaterial.LavenderJade
            or TinkerMaterial.Malachite or TinkerMaterial.Opal
            or TinkerMaterial.RedJade or TinkerMaterial.RoseQuartz
            or TinkerMaterial.SmokeyQuartz or TinkerMaterial.Sunstone =>
            MagicItemTinkeringSkill,

        TinkerMaterial.Alabaster or TinkerMaterial.ArmoredilloHide
            or TinkerMaterial.Bronze or TinkerMaterial.Ceramic
            or TinkerMaterial.Marble or TinkerMaterial.Peridot
            or TinkerMaterial.ReedSharkHide or TinkerMaterial.Steel
            or TinkerMaterial.Wool or TinkerMaterial.YellowTopaz
            or TinkerMaterial.Zircon =>
            ArmorTinkeringSkill,

        TinkerMaterial.Aquamarine or TinkerMaterial.BlackGarnet
            or TinkerMaterial.Brass or TinkerMaterial.Emerald
            or TinkerMaterial.Granite or TinkerMaterial.ImperialTopaz
            or TinkerMaterial.Iron or TinkerMaterial.Jet
            or TinkerMaterial.Mahogany or TinkerMaterial.Oak
            or TinkerMaterial.RedGarnet or TinkerMaterial.Velvet
            or TinkerMaterial.WhiteSapphire =>
            WeaponTinkeringSkill,

        TinkerMaterial.Amber or TinkerMaterial.Copper or TinkerMaterial.Diamond
            or TinkerMaterial.Ebony or TinkerMaterial.Gold
            or TinkerMaterial.GromnieHide or TinkerMaterial.Linen
            or TinkerMaterial.Moonstone or TinkerMaterial.Pine
            or TinkerMaterial.Porcelain or TinkerMaterial.Pyreal
            or TinkerMaterial.Ruby or TinkerMaterial.Sapphire
            or TinkerMaterial.Satin or TinkerMaterial.Silver
            or TinkerMaterial.Teak =>
            ItemTinkeringSkill,

        _ => 0u,
    };

    /// <summary>
    /// How much the material itself adds to the difficulty. Gold and oak are
    /// the kindest at ten; the eight magic-item gems that raise a spell's
    /// power are the harshest at twenty-five.
    /// </summary>
    internal static double MaterialModifier(int material) => (TinkerMaterial)material switch
    {
        TinkerMaterial.Gold or TinkerMaterial.Oak => 10d,

        TinkerMaterial.Alabaster or TinkerMaterial.ArmoredilloHide
            or TinkerMaterial.Brass or TinkerMaterial.Bronze
            or TinkerMaterial.Ceramic or TinkerMaterial.Granite
            or TinkerMaterial.Linen or TinkerMaterial.Marble
            or TinkerMaterial.Moonstone or TinkerMaterial.Opal
            or TinkerMaterial.Pine or TinkerMaterial.ReedSharkHide
            or TinkerMaterial.Velvet or TinkerMaterial.Wool => 11d,

        TinkerMaterial.Ebony or TinkerMaterial.GreenGarnet
            or TinkerMaterial.Iron or TinkerMaterial.Mahogany
            or TinkerMaterial.Porcelain or TinkerMaterial.Satin
            or TinkerMaterial.Steel or TinkerMaterial.Teak => 12d,

        TinkerMaterial.Bloodstone or TinkerMaterial.Carnelian
            or TinkerMaterial.Citrine or TinkerMaterial.Hematite
            or TinkerMaterial.LavenderJade or TinkerMaterial.Malachite
            or TinkerMaterial.RedJade or TinkerMaterial.RoseQuartz => 25d,

        _ => 20d,
    };

    /// <summary>
    /// Whether the material imbues (<see cref="ImbueSalvage"/>) or tinkers
    /// (<see cref="OrdinarySalvage"/>). An imbue rolls at a third of the
    /// ordinary chance.
    /// </summary>
    internal static int SalvageKind(int material) => (TinkerMaterial)material switch
    {
        TinkerMaterial.BlackOpal or TinkerMaterial.FireOpal
            or TinkerMaterial.Sunstone or TinkerMaterial.Aquamarine
            or TinkerMaterial.BlackGarnet or TinkerMaterial.Emerald
            or TinkerMaterial.ImperialTopaz or TinkerMaterial.Jet
            or TinkerMaterial.RedGarnet or TinkerMaterial.WhiteSapphire =>
            ImbueSalvage,

        _ => OrdinarySalvage,
    };

    /// <summary>
    /// The kinds of item this material may be imbued into; empty for a
    /// material that imbues nothing. Sunstone raises a critical chance, which
    /// a casting implement has no use for, so it alone leaves wands out.
    /// </summary>
    internal static IReadOnlyList<PluginObjectClass> ImbueTargets(int material) =>
        (TinkerMaterial)material switch
        {
            TinkerMaterial.BlackOpal or TinkerMaterial.FireOpal
                or TinkerMaterial.Aquamarine or TinkerMaterial.BlackGarnet
                or TinkerMaterial.Emerald or TinkerMaterial.ImperialTopaz
                or TinkerMaterial.Jet or TinkerMaterial.RedGarnet
                or TinkerMaterial.WhiteSapphire =>
            [
                PluginObjectClass.WandStaffOrb,
                PluginObjectClass.MeleeWeapon,
                PluginObjectClass.MissileWeapon,
            ],

            TinkerMaterial.Sunstone =>
            [
                PluginObjectClass.MeleeWeapon,
                PluginObjectClass.MissileWeapon,
            ],

            _ => [],
        };
}

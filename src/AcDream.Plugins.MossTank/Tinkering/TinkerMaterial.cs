using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank.Tinkering;

/// <summary>
/// Every material an item or a bag of salvage can be made of, by the id the
/// server sends. Five of the ids -- cloth, gem, metal, stone and wood -- are
/// group headings rather than materials of their own; they are listed so the
/// numbering matches the server's, and no tinkering rule matches on them.
/// </summary>
internal enum TinkerMaterial
{
    /// <summary>Ceramic.</summary>
    Ceramic = 1,

    /// <summary>Porcelain.</summary>
    Porcelain = 2,

    /// <summary>The cloth heading; no item is made of it.</summary>
    Cloth = 3,

    /// <summary>Linen.</summary>
    Linen = 4,

    /// <summary>Satin.</summary>
    Satin = 5,

    /// <summary>Silk.</summary>
    Silk = 6,

    /// <summary>Velvet.</summary>
    Velvet = 7,

    /// <summary>Wool.</summary>
    Wool = 8,

    /// <summary>The gem heading; no item is made of it.</summary>
    Gem = 9,

    /// <summary>Agate.</summary>
    Agate = 10,

    /// <summary>Amber.</summary>
    Amber = 11,

    /// <summary>Amethyst.</summary>
    Amethyst = 12,

    /// <summary>Aquamarine.</summary>
    Aquamarine = 13,

    /// <summary>Azurite.</summary>
    Azurite = 14,

    /// <summary>Black garnet.</summary>
    BlackGarnet = 15,

    /// <summary>Black opal.</summary>
    BlackOpal = 16,

    /// <summary>Bloodstone.</summary>
    Bloodstone = 17,

    /// <summary>Carnelian.</summary>
    Carnelian = 18,

    /// <summary>Citrine.</summary>
    Citrine = 19,

    /// <summary>Diamond.</summary>
    Diamond = 20,

    /// <summary>Emerald.</summary>
    Emerald = 21,

    /// <summary>Fire opal.</summary>
    FireOpal = 22,

    /// <summary>Green garnet.</summary>
    GreenGarnet = 23,

    /// <summary>Green jade.</summary>
    GreenJade = 24,

    /// <summary>Hematite.</summary>
    Hematite = 25,

    /// <summary>Imperial topaz.</summary>
    ImperialTopaz = 26,

    /// <summary>Jet.</summary>
    Jet = 27,

    /// <summary>Lapis lazuli.</summary>
    LapisLazuli = 28,

    /// <summary>Lavender jade.</summary>
    LavenderJade = 29,

    /// <summary>Malachite.</summary>
    Malachite = 30,

    /// <summary>Moonstone.</summary>
    Moonstone = 31,

    /// <summary>Onyx.</summary>
    Onyx = 32,

    /// <summary>Opal.</summary>
    Opal = 33,

    /// <summary>Peridot.</summary>
    Peridot = 34,

    /// <summary>Red garnet.</summary>
    RedGarnet = 35,

    /// <summary>Red jade.</summary>
    RedJade = 36,

    /// <summary>Rose quartz.</summary>
    RoseQuartz = 37,

    /// <summary>Ruby.</summary>
    Ruby = 38,

    /// <summary>Sapphire.</summary>
    Sapphire = 39,

    /// <summary>Smokey quartz.</summary>
    SmokeyQuartz = 40,

    /// <summary>Sunstone.</summary>
    Sunstone = 41,

    /// <summary>Tiger eye.</summary>
    TigerEye = 42,

    /// <summary>Tourmaline.</summary>
    Tourmaline = 43,

    /// <summary>Turquoise.</summary>
    Turquoise = 44,

    /// <summary>White jade.</summary>
    WhiteJade = 45,

    /// <summary>White quartz.</summary>
    WhiteQuartz = 46,

    /// <summary>White sapphire.</summary>
    WhiteSapphire = 47,

    /// <summary>Yellow garnet.</summary>
    YellowGarnet = 48,

    /// <summary>Yellow topaz.</summary>
    YellowTopaz = 49,

    /// <summary>Zircon.</summary>
    Zircon = 50,

    /// <summary>Ivory.</summary>
    Ivory = 51,

    /// <summary>Leather.</summary>
    Leather = 52,

    /// <summary>Armoredillo hide.</summary>
    ArmoredilloHide = 53,

    /// <summary>Gromnie hide.</summary>
    GromnieHide = 54,

    /// <summary>Reed shark hide.</summary>
    ReedSharkHide = 55,

    /// <summary>The metal heading; no item is made of it.</summary>
    Metal = 56,

    /// <summary>Brass.</summary>
    Brass = 57,

    /// <summary>Bronze.</summary>
    Bronze = 58,

    /// <summary>Copper.</summary>
    Copper = 59,

    /// <summary>Gold.</summary>
    Gold = 60,

    /// <summary>Iron.</summary>
    Iron = 61,

    /// <summary>Pyreal.</summary>
    Pyreal = 62,

    /// <summary>Silver.</summary>
    Silver = 63,

    /// <summary>Steel.</summary>
    Steel = 64,

    /// <summary>The stone heading; no item is made of it.</summary>
    Stone = 65,

    /// <summary>Alabaster.</summary>
    Alabaster = 66,

    /// <summary>Granite.</summary>
    Granite = 67,

    /// <summary>Marble.</summary>
    Marble = 68,

    /// <summary>Obsidian.</summary>
    Obsidian = 69,

    /// <summary>Sandstone.</summary>
    Sandstone = 70,

    /// <summary>Serpentine.</summary>
    Serpentine = 71,

    /// <summary>The wood heading; no item is made of it.</summary>
    Wood = 72,

    /// <summary>Ebony.</summary>
    Ebony = 73,

    /// <summary>Mahogany.</summary>
    Mahogany = 74,

    /// <summary>Oak.</summary>
    Oak = 75,

    /// <summary>Pine.</summary>
    Pine = 76,

    /// <summary>Teak.</summary>
    Teak = 77,
}

/// <summary>
/// Turning a material id into the name a player types or reads, and back.
/// The spelling is the one the item names already use, so "Reed Shark Hide"
/// on this page reads the same as it does on a salvage bag.
/// </summary>
internal static class TinkerMaterials
{
    private static readonly Dictionary<string, int> IdsByName = BuildIdsByName();

    /// <summary>
    /// Every material that names something, lowest id first. The five group
    /// headings are left out: nothing can be made of them.
    /// </summary>
    internal static IReadOnlyList<int> Named { get; } = BuildNamed();

    /// <summary>
    /// The material's name, or an empty string when the id names none.
    /// </summary>
    internal static string Name(int material) =>
        MaterialNames.Name(material) ?? string.Empty;

    /// <summary>
    /// The id of the material with this name, ignoring case and surrounding
    /// space; zero when no material carries it.
    /// </summary>
    internal static int Id(string name) =>
        IdsByName.TryGetValue(name.Trim(), out int id) ? id : 0;

    private static int[] BuildNamed()
    {
        var named = new List<int>();
        for (int id = (int)TinkerMaterial.Ceramic; id <= (int)TinkerMaterial.Teak; id++)
        {
            if (MaterialNames.Name(id) is not null)
                named.Add(id);
        }
        return [.. named];
    }

    private static Dictionary<string, int> BuildIdsByName()
    {
        var byName = new Dictionary<string, int>(StringComparer.OrdinalIgnoreCase);
        for (int id = (int)TinkerMaterial.Ceramic; id <= (int)TinkerMaterial.Teak; id++)
        {
            if (MaterialNames.Name(id) is { } name)
                byName[name] = id;
        }
        return byName;
    }
}

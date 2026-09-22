using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The eleven kinds of mark the dungeon map draws, each with its own row
/// of settings under <c>DungeonMaps.Display.Markers</c>.
/// </summary>
public enum DungeonMarkerKind
{
    You,
    Others,
    Items,
    Monsters,
    NPCs,
    MyCorpse,
    OtherCorpses,
    Portals,
    Containers,
    Doors,
    EverythingElse,
}

/// <summary>
/// How one kind of mark is drawn, as the settings rows say.
/// </summary>
/// <param name="Enabled">Whether the kind is drawn at all.</param>
/// <param name="UseIcon">The object's icon rather than a dot.</param>
/// <param name="ShowLabel">The object's name beside the mark.</param>
/// <param name="Color">The dot's and the label's colour, as #AARRGGBB.</param>
/// <param name="Size">How many pixels across the mark is.</param>
internal readonly record struct DungeonMarkerStyle(
    bool Enabled,
    bool UseIcon,
    bool ShowLabel,
    uint Color,
    int Size);

/// <summary>
/// Sorts world objects into marker kinds and names them for the map.
/// </summary>
internal static class DungeonMarkers
{
    /// <summary>Every kind, in the order the settings rows list them.</summary>
    public static IReadOnlyList<DungeonMarkerKind> Kinds { get; } = Enum.GetValues<DungeonMarkerKind>();

    /// <summary>The kind's name as the settings rows spell it.</summary>
    public static string GroupName(DungeonMarkerKind kind) => kind.ToString();

    /// <summary>
    /// Which kind of mark an object gets. A player is you or somebody else
    /// by id; a corpse is yours by its name; shopkeepers count as NPCs. A
    /// thing that can be picked up is an item; anything the list does not
    /// name is drawn, if at all, under the catch-all kind.
    /// </summary>
    public static DungeonMarkerKind Classify(in PluginWorldObject subject, uint selfId, string selfName) =>
        subject.ObjectClass switch
        {
            PluginObjectClass.Player => subject.ObjectId == selfId ? DungeonMarkerKind.You : DungeonMarkerKind.Others,
            PluginObjectClass.Monster => DungeonMarkerKind.Monsters,
            PluginObjectClass.Npc or PluginObjectClass.Vendor => DungeonMarkerKind.NPCs,
            PluginObjectClass.Portal => DungeonMarkerKind.Portals,
            PluginObjectClass.Corpse =>
                string.Equals(subject.Name, $"Corpse of {selfName}", StringComparison.Ordinal)
                    ? DungeonMarkerKind.MyCorpse
                    : DungeonMarkerKind.OtherCorpses,
            PluginObjectClass.Door => DungeonMarkerKind.Doors,
            PluginObjectClass.Container => DungeonMarkerKind.Containers,
            PluginObjectClass.MeleeWeapon or PluginObjectClass.Armor or PluginObjectClass.Clothing
                or PluginObjectClass.Jewelry or PluginObjectClass.Food or PluginObjectClass.Money
                or PluginObjectClass.Misc or PluginObjectClass.MissileWeapon or PluginObjectClass.Gem
                or PluginObjectClass.SpellComponent or PluginObjectClass.Key or PluginObjectClass.TradeNote
                or PluginObjectClass.ManaStone or PluginObjectClass.Plant or PluginObjectClass.BaseCooking
                or PluginObjectClass.BaseAlchemy or PluginObjectClass.BaseFletching
                or PluginObjectClass.CraftedCooking or PluginObjectClass.CraftedAlchemy
                or PluginObjectClass.CraftedFletching or PluginObjectClass.HealingKit
                or PluginObjectClass.Lockpick or PluginObjectClass.WandStaffOrb or PluginObjectClass.Bundle
                or PluginObjectClass.Book or PluginObjectClass.Journal or PluginObjectClass.Foci
                or PluginObjectClass.Salvage or PluginObjectClass.Ust or PluginObjectClass.Scroll
                => DungeonMarkerKind.Items,
            _ => DungeonMarkerKind.EverythingElse,
        };

    /// <summary>
    /// Whether an object of this class moves about, and so is placed again
    /// as it goes; everything else keeps the place it was first seen at.
    /// </summary>
    public static bool IsMover(PluginObjectClass objectClass) =>
        objectClass is PluginObjectClass.Player or PluginObjectClass.Monster or PluginObjectClass.CombatPet;

    /// <summary>
    /// The name written beside the mark: a portal or an NPC loses the words
    /// "Portal to" and "Portal", so the label is the place.
    /// </summary>
    public static string LabelOf(in PluginWorldObject subject)
    {
        string name = subject.Name ?? string.Empty;
        if (subject.ObjectClass is PluginObjectClass.Portal or PluginObjectClass.Npc)
        {
            name = name.Replace("Portal to ", string.Empty, StringComparison.Ordinal)
                .Replace(" Portal", string.Empty, StringComparison.Ordinal);
        }
        return name;
    }
}

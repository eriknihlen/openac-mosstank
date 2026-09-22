namespace AcDream.Plugins.MossTank;

/// <summary>
/// Every setting the UB tab offers, as authored.
/// </summary>
/// <remarks>
/// <para>
/// The display tools nest their settings three deep: a tool holds a group,
/// the group holds a group, and only the innermost one holds a value. That
/// is a tree, and a tree does not fit a page this size, so the names are
/// flattened to dotted paths and the page's filter field does the work the
/// tree did. The repeated shapes -- a nametag, a map colour, a map marker --
/// are expanded from one helper each rather than written out, so a shape
/// cannot drift between two of its uses.
/// </para>
/// <para>
/// Window positions and sizes are deliberately absent: the host already
/// keeps where a window sits, and a second copy of that would fight it.
/// </para>
/// </remarks>
internal static class UbSettingDefinitions
{
    /// <summary>
    /// How a spend of experience is shared out until somebody says
    /// otherwise: a weight for all six attributes, all three vitals and
    /// all thirty-eight skills. A weight is a ratio, not an amount -- zero
    /// means never raise this, and ten against one means ten times as much
    /// goes here. What ships is a fighter's: the weapon skills and the two
    /// combat defences carry it, health is worth a little more than an
    /// attribute, and the trade skills are off.
    /// </summary>
    /// <remarks>
    /// This has to stay above <see cref="All"/>: static fields are filled
    /// in the order they are written, and <see cref="Build"/> reads it.
    /// </remarks>
    private static readonly string[] DefaultXpPolicy =
    [
        "Strength = 1",
        "Endurance = 1",
        "Coordination = 1",
        "Quickness = 1",
        "Focus = 1",
        "Self = 1",
        "Health = 1.4",
        "Stamina = 0.1",
        "Mana = 0.1",
        "Alchemy = 0",
        "ArcaneLore = 0.1",
        "ArmorTinkering = 0",
        "AssessCreature = 0",
        "AssessPerson = 0",
        "Cooking = 0",
        "CreatureEnchantment = 0.2",
        "Deception = 0.1",
        "DirtyFighting = 0.1",
        "DualWield = 0.1",
        "FinesseWeapons = 10",
        "Fletching = 0.1",
        "Healing = 0.1",
        "HeavyWeapons = 10",
        "ItemEnchantment = 0.2",
        "ItemTinkering = 0",
        "Jump = 0.02",
        "Leadership = 0.1",
        "LifeMagic = 1",
        "LightWeapons = 10",
        "Lockpick = 0",
        "Loyalty = 0.1",
        "MagicDefense = 0.1",
        "MagicItemTinkering = 0",
        "ManaConversion = 0.1",
        "MeleeDefense = 5",
        "MissileDefense = 5",
        "MissileWeapons = 10",
        "Recklessness = 0.1",
        "Run = 0.1",
        "Salvaging = 0.1",
        "Shield = 0",
        "SneakAttack = 0.1",
        "Summoning = 1",
        "TwoHandedCombat = 10",
        "VoidMagic = 10",
        "WarMagic = 10",
        "WeaponTinkering = 0",
    ];

    /// <summary>Every setting, grouped by tool and in page order.</summary>
    internal static IReadOnlyList<UbSettingDefinition> All { get; } = Build();

    private static readonly IReadOnlyDictionary<string, UbSettingKind> KindsByName =
        All.ToDictionary(
            static definition => definition.Name,
            static definition => definition.Kind,
            StringComparer.OrdinalIgnoreCase);

    /// <summary>
    /// The shape one name's value has, so a store holding text can read it
    /// back. A name this version has never heard of reads as text, which is
    /// what it was written as.
    /// </summary>
    internal static UbSettingKind KindOf(string name) =>
        KindsByName.TryGetValue(name, out UbSettingKind kind)
            ? kind
            : UbSettingKind.String;

    private static UbSettingDefinition[] Build()
    {
        var rows = new List<UbSettingDefinition>();
        AddNametags(rows);
        AddDungeonMaps(rows);
        AddLandscapeMaps(rows);
        AddNetworkUi(rows);
        AddBehaviourTools(rows);
        return [.. rows];
    }

    private static void AddNametags(List<UbSettingDefinition> rows)
    {
        rows.Add(Switch(
            "Nametags.Enabled",
            "Draw a name above the creatures and objects around you.",
            true,
            UbSettingScope.Global));
        rows.Add(Decimal32(
            "Nametags.MaxRange",
            "How far away, in meters, a name is still drawn.",
            35f));

        // Every kind of thing that can carry a name gets the same five
        // controls, so they are expanded from one shape.
        AddNametag(rows, "Player", "another player", 0xFF00FFFFu);
        AddNametag(rows, "Pet", "a summoned pet", 0xFF00FFFFu);
        AddNametag(rows, "AllegiancePlayer", "a player in your allegiance", 0xFF00FF00u);
        AddNametag(rows, "Portal", "a portal", 0xFF00FF00u);
        AddNametag(rows, "Npc", "a non-player character", 0xFFFFFF00u);
        AddNametag(rows, "Vendor", "a vendor", 0xFFFF00FFu);
        AddNametag(rows, "Monster", "a monster", 0xFFFF0000u);
    }

    private static void AddNametag(
        List<UbSettingDefinition> rows,
        string group,
        string subject,
        uint color)
    {
        string prefix = $"Nametags.{group}.";
        rows.Add(Switch(prefix + "Enabled", $"Draw a name over {subject}.", true));
        rows.Add(Colour(prefix + "TagColor", $"The colour of the name over {subject}.", color));
        rows.Add(Decimal32(
            prefix + "TagSize", $"The height of the name over {subject}, in meters.", 0.15f));
        rows.Add(Colour(
            prefix + "TickerColor", $"The colour of the second line under {subject}'s name.", color));
        rows.Add(Decimal32(
            prefix + "TickerSize",
            $"The height of the second line under {subject}'s name, in meters.",
            0.1f));
    }

    private static void AddDungeonMaps(List<UbSettingDefinition> rows)
    {
        rows.Add(Switch(
            "DungeonMaps.Enabled",
            "Draw a floor plan of the dungeon you are standing in.",
            true,
            UbSettingScope.Global));
        rows.Add(Switch(
            "DungeonMaps.Debug",
            "Report what the floor plan is drawing while it draws it.",
            false,
            UbSettingScope.Global));
        rows.Add(Switch(
            "DungeonMaps.DrawWhenClosed",
            "Keep drawing the floor plan while its own window is shut.",
            true));
        rows.Add(Switch(
            "DungeonMaps.ShowVisitedTiles",
            "Shade the parts of the floor you have already walked.",
            true));
        rows.Add(Colour(
            "DungeonMaps.VisitedTilesColor",
            "The tint laid over a part of the floor you have walked.",
            0xFFFF96FFu));
        rows.Add(Switch("DungeonMaps.ShowCompass", "Draw a compass on the floor plan.", true));
        rows.Add(Whole(
            "DungeonMaps.Opacity",
            "How solid the floor plan is drawn, from 0 to 255.",
            16));
        rows.Add(Decimal32("DungeonMaps.MapZoom", "How far the floor plan is zoomed in.", 4.2f));
        rows.Add(Whole("DungeonMaps.LabelFontSize", "The height of a floor-plan label.", 10));

        // Eight pieces of dungeon geometry, each with a switch and a colour.
        AddMapColour(rows, "DungeonName", "the dungeon's name across the top", 0xFFFFFFFFu);
        AddMapColour(rows, "Walls", "an outer wall", 0xFF00007Fu);
        AddMapColour(rows, "InnerWalls", "a wall inside a room", 0xFF7FBFFFu);
        AddMapColour(rows, "RampedWalls", "a sloped wall", 0xFF4EA6FFu);
        AddMapColour(rows, "Stairs", "a staircase", 0xFF003F7Fu);
        AddMapColour(rows, "Floors", "the floor", 0xFF007FBFu);
        AddMapColour(rows, "VisualNavStickyPoint", "a route's anchor point", 0xFFADFF2Fu);
        AddMapColour(rows, "VisualNavLines", "the line between two route points", 0xFFFF00FFu);

        // Eleven kinds of thing that can appear as a marker, each with the
        // same five controls.
        AddMapMarker(rows, "You", "you", true, false, false, 0xFFFF0000u);
        AddMapMarker(rows, "Others", "another player", true, true, true, 0xFFFFFFFFu);
        AddMapMarker(rows, "Items", "an item on the ground", true, true, true, 0xFFFFFFFFu);
        AddMapMarker(rows, "Monsters", "a monster", true, true, false, 0xFFFFA500u);
        AddMapMarker(rows, "NPCs", "a non-player character", true, true, false, 0xFFFFFF00u);
        AddMapMarker(rows, "MyCorpse", "your own corpse", true, true, true, 0xFFFF0000u);
        AddMapMarker(rows, "OtherCorpses", "somebody else's corpse", false, true, false, 0xFFF5F5F5u);
        AddMapMarker(rows, "Portals", "a portal", true, true, true, 0xFFFFF0FFu);
        AddMapMarker(rows, "Containers", "a chest or other container", true, true, false, 0xFFF4A460u);
        AddMapMarker(rows, "Doors", "a door", true, false, false, 0xFFA52A2Au);
        AddMapMarker(rows, "EverythingElse", "anything with no marker of its own", false, true, false, 0xFFF5F5F5u);
    }

    private static void AddMapColour(
        List<UbSettingDefinition> rows,
        string group,
        string subject,
        uint color)
    {
        string prefix = $"DungeonMaps.Display.{group}.";
        rows.Add(Switch(prefix + "Enabled", $"Draw {subject} on the floor plan.", true));
        rows.Add(Colour(prefix + "Color", $"The colour {subject} is drawn in.", color));
    }

    private static void AddMapMarker(
        List<UbSettingDefinition> rows,
        string group,
        string subject,
        bool enabled,
        bool useIcon,
        bool showLabel,
        uint color)
    {
        string prefix = $"DungeonMaps.Display.Markers.{group}.";
        rows.Add(Switch(prefix + "Enabled", $"Mark {subject} on the floor plan.", enabled));
        rows.Add(Switch(prefix + "ShowLabel", $"Write the name beside the mark for {subject}.", showLabel));
        rows.Add(Switch(prefix + "UseIcon", $"Use the icon rather than a dot for {subject}.", useIcon));
        rows.Add(Colour(prefix + "Color", $"The colour the mark for {subject} is drawn in.", color));
        rows.Add(Whole(prefix + "Size", $"How many pixels across the mark for {subject} is.", 3));
    }

    private static void AddLandscapeMaps(List<UbSettingDefinition> rows)
    {
        rows.Add(Switch(
            "LandscapeMaps.Enabled",
            "Draw a map of the countryside you are standing in.",
            true,
            UbSettingScope.Global));
        rows.Add(Whole(
            "LandscapeMaps.Opacity",
            "How solid the countryside map is drawn, from 0 to 255.",
            16));
    }

    private static void AddNetworkUi(List<UbSettingDefinition> rows)
    {
        rows.Add(Switch(
            "NetworkUI.Enabled",
            "Show the panel listing your other characters on this machine.",
            false,
            UbSettingScope.Global));
        rows.Add(Switch(
            "NetworkUI.ShowHudWhenClosed",
            "Keep the readout on screen while the main window is shut.",
            true,
            UbSettingScope.Global));
        rows.Add(Lines(
            "NetworkUI.TrackedItems",
            "Items whose count is shown for every character, one name a line.",
            UbSettingScope.Profile));
        rows.Add(Line(
            "NetworkUI.SelectedTag",
            "Which tag's characters the panel lists; All lists every one.",
            "All",
            UbSettingScope.Character));
        rows.Add(Lines(
            "Networking.Tags",
            "Names for this character that a broadcast can be aimed at, one a line.",
            UbSettingScope.Character));
    }

    private static void AddBehaviourTools(List<UbSettingDefinition> rows)
    {
        rows.Add(Switch("Aliases.Enabled", "Rewrite what you type using your alias list.", true));
        rows.Add(Line(
            "Aliases.Profile",
            "Which alias set this character uses; [character] keeps a private one.",
            "[character]",
            UbSettingScope.Character));
        rows.Add(Lines(
            "Aliases.DefinedAliases",
            "The aliases themselves, as 'what you type = what is sent', one a line."));

        rows.Add(Switch("AutoVendor.Enabled", "Buy and sell at a vendor without being asked.", true));
        rows.Add(Switch("AutoVendor.EnableBuying", "Let a vendor run buy what the profile wants.", true));
        rows.Add(Switch("AutoVendor.EnableSelling", "Let a vendor run sell what the profile allows.", true));
        rows.Add(Switch(
            "AutoVendor.TestMode",
            "Say what would be bought and sold instead of doing it.",
            false));
        rows.Add(Switch("AutoVendor.Think", "Think to yourself when a vendor run finishes.", false));
        rows.Add(Switch(
            "AutoVendor.ShowMerchantInfo",
            "Report what a vendor deals in when you walk up to one.",
            true));
        rows.Add(Switch(
            "AutoVendor.OnlyFromMainPack",
            "Sell only what is in the main pack, leaving side packs alone.",
            false));
        rows.Add(Whole("AutoVendor.Tries", "How many times opening a vendor is attempted.", 4));
        rows.Add(Whole(
            "AutoVendor.TriesTime",
            "How long to wait between attempts to open a vendor, in milliseconds.",
            5000));

        rows.Add(Whole(
            "AutoXp.StopBeforeMax",
            "How many levels short of the maximum to stop raising something.",
            10));
        rows.Add(Whole(
            "AutoXp.TriesTime",
            "How long to wait between one spend and the next, in milliseconds.",
            300));
        rows.Add(Whole(
            "AutoXp.MaxXpChunk",
            "The most experience to spend in a single request.",
            1000000000));
        rows.Add(Lines(
            "AutoXp.Policy",
            "How experience is shared out, as 'target = weight', one a line.",
            DefaultXpPolicy));

        rows.Add(Switch(
            "EquipmentManager.Think",
            "Think to yourself when an equipment set has finished going on.",
            false));

        rows.Add(Switch("GameEvents.Enabled", "Run your own handlers when something happens.", true));
        rows.Add(Line(
            "GameEvents.Profile",
            "Which handler set this character uses; [character] keeps a private one.",
            "[character]",
            UbSettingScope.Character));
        rows.Add(Lines(
            "GameEvents.Handlers",
            "The handlers themselves, as 'event = what to run', one a line. "
            + "They run only while the macro runs with meta enabled."));

        rows.Add(Decimal64(
            "ItemGiver.Range",
            "How far off someone may stand and still be handed items, in meters.",
            15d));
        rows.Add(Decimal64(
            "ItemGiver.Delay",
            "How long to wait between one hand-over and the next, in seconds.",
            0d));
        rows.Add(Whole(
            "ItemGiver.BusyRetryLimit",
            "How many times one item is offered again before it is written off.",
            10));
        rows.Add(Whole(
            "ItemGiver.FailureLimit",
            "How many written-off items stop the whole hand-over.",
            3));
        rows.Add(Switch(
            "ItemGiver.Think",
            "Think to yourself when a hand-over has finished.",
            false));

        rows.Add(Decimal32(
            "AutoTinker.MinPercentage",
            "Minimum percentage required to perform tinker.",
            99.5f));
        rows.Add(Whole(
            "AutoTinker.MaxTinks",
            "Max number of tinker attempts.",
            10));
        // The client cannot read which augmentations a character carries, and
        // Charmed Smith lifts an imbue's odds by five points. Ticking it here
        // is the only way the planned percentage can match what the server
        // will actually roll.
        rows.Add(Switch(
            "AutoTinker.CharmedSmith",
            "You carry the Charmed Smith augmentation, which adds five points "
            + "to every imbue's chance.",
            false,
            UbSettingScope.Character));

        rows.Add(Switch("Jumper.PauseNav", "Hold the route still while a jump is in the air.", true));
        rows.Add(Switch("Jumper.ThinkComplete", "Think to yourself when a jump lands.", false));
        rows.Add(Switch("Jumper.ThinkFail", "Think to yourself when a jump does not happen.", false));
        rows.Add(Whole("Jumper.Attempts", "How many times a jump is tried before giving up.", 3));

        // One switch for both directions of sharing, as the original has it:
        // what this character tells the others and what it takes in from them.
        rows.Add(Switch(
            "Sharing.Vitals",
            "Tell your other characters on this machine how you are doing and "
            + "what you have cast, and take in what they cast.",
            true,
            UbSettingScope.Global));
        rows.Add(Line(
            "Sharing.CastTag",
            "Only take in casts from characters whose client carries this tag; "
            + "blank takes them all.",
            string.Empty,
            UbSettingScope.Character));
    }

    private static UbSettingDefinition Switch(
        string name,
        string summary,
        bool value,
        UbSettingScope scope = UbSettingScope.Profile) =>
        new(name, summary, UbSettingKind.Bool, UbSettingValue.FromBool(value), scope);

    private static UbSettingDefinition Whole(
        string name,
        string summary,
        int value,
        UbSettingScope scope = UbSettingScope.Profile) =>
        new(name, summary, UbSettingKind.Int, UbSettingValue.FromInt(value), scope);

    private static UbSettingDefinition Decimal32(
        string name,
        string summary,
        float value,
        UbSettingScope scope = UbSettingScope.Profile) =>
        new(name, summary, UbSettingKind.Single, UbSettingValue.FromSingle(value), scope);

    private static UbSettingDefinition Decimal64(
        string name,
        string summary,
        double value,
        UbSettingScope scope = UbSettingScope.Profile) =>
        new(name, summary, UbSettingKind.Double, UbSettingValue.FromDouble(value), scope);

    private static UbSettingDefinition Colour(
        string name,
        string summary,
        uint argb,
        UbSettingScope scope = UbSettingScope.Profile) =>
        new(name, summary, UbSettingKind.Color, UbSettingValue.FromColor(argb), scope);

    private static UbSettingDefinition Line(
        string name,
        string summary,
        string value,
        UbSettingScope scope = UbSettingScope.Profile) =>
        new(name, summary, UbSettingKind.String, UbSettingValue.FromText(value), scope);

    private static UbSettingDefinition Lines(
        string name,
        string summary,
        UbSettingScope scope = UbSettingScope.Profile) =>
        Lines(name, summary, [], scope);

    private static UbSettingDefinition Lines(
        string name,
        string summary,
        IEnumerable<string> value,
        UbSettingScope scope = UbSettingScope.Profile) =>
        new(name, summary, UbSettingKind.Collection, UbSettingValue.FromCollection(value), scope);
}

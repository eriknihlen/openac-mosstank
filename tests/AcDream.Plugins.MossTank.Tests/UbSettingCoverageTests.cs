using AcDream.Plugins.MossTank;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The owner's requirement for the UB tab is that every setting the tools
/// being brought over declare is reachable from it. This file writes that
/// inventory down as data and reconciles the catalogue against it in both
/// directions, so a setting cannot go missing and a row cannot appear from
/// nowhere.
/// </summary>
/// <remarks>
/// <para>
/// <b>Where the inventory came from.</b> Every row below is one leaf value
/// declared by one of the in-scope tools in the source this tab is being
/// ported from, read field by field off those declarations. The
/// <c>Tool</c> and <c>Field</c> columns are spelled the way they are spelled
/// there, not the way they are spelled here, so a reader with that source
/// can check the list against it line by line. A tool that nests its
/// settings contributes one row per leaf: a nametag group is five, a map
/// colour two, a map marker five, and those three shapes are expanded from
/// their own member lists rather than typed out seventy times.
/// </para>
/// <para>
/// <b>Why it is written down twice.</b> Counting the catalogue against a
/// number taken from the catalogue proves nothing: delete a setting, update
/// the number, and the test still passes. Nothing in this file reads
/// <see cref="UbSettingDefinitions"/> to decide what ought to exist.
/// </para>
/// <para>
/// <b>Exclusions are named.</b> A setting that is deliberately not on the
/// tab has a row in <see cref="Excluded"/> carrying the reason, so a
/// considered omission reads differently from an oversight and a future
/// deletion fails this test rather than quietly passing it.
/// </para>
/// </remarks>
public sealed class UbSettingCoverageTests
{
    /// <summary>
    /// The flat settings: one row per leaf value that is not part of one of
    /// the three repeated group shapes.
    /// </summary>
    private static readonly (string Tool, string Field)[] FlatSettings =
    [
        ("Nametags", "Enabled"),
        ("Nametags", "MaxRange"),

        ("DungeonMaps", "Enabled"),
        ("DungeonMaps", "Debug"),
        ("DungeonMaps", "DrawWhenClosed"),
        ("DungeonMaps", "ShowVisitedTiles"),
        ("DungeonMaps", "VisitedTilesColor"),
        ("DungeonMaps", "ShowCompass"),
        ("DungeonMaps", "Opacity"),
        ("DungeonMaps", "MapZoom"),
        ("DungeonMaps", "LabelFontSize"),
        ("DungeonMaps", "MapWindowX"),
        ("DungeonMaps", "MapWindowY"),
        ("DungeonMaps", "MapWindowWidth"),
        ("DungeonMaps", "MapWindowHeight"),

        ("LandscapeMaps", "Enabled"),
        ("LandscapeMaps", "Opacity"),
        ("LandscapeMaps", "MapWindowX"),
        ("LandscapeMaps", "MapWindowY"),
        ("LandscapeMaps", "MapWindowWidth"),
        ("LandscapeMaps", "MapWindowHeight"),

        ("NetworkUI", "Enabled"),
        ("NetworkUI", "ShowHudWhenClosed"),
        ("NetworkUI", "TrackedItems"),
        ("NetworkUI", "SelectedTag"),
        ("NetworkUI", "WindowPositionX"),
        ("NetworkUI", "WindowPositionY"),

        ("Networking", "Tags"),

        ("AutoVendor", "Enabled"),
        ("AutoVendor", "EnableBuying"),
        ("AutoVendor", "EnableSelling"),
        ("AutoVendor", "TestMode"),
        ("AutoVendor", "Think"),
        ("AutoVendor", "ShowMerchantInfo"),
        ("AutoVendor", "OnlyFromMainPack"),
        ("AutoVendor", "Tries"),
        ("AutoVendor", "TriesTime"),

        ("AutoXp", "StopBeforeMax"),
        ("AutoXp", "TriesTime"),
        ("AutoXp", "MaxXpChunk"),
        ("AutoXp", "Policy"),

        ("Aliases", "Enabled"),
        ("Aliases", "Profile"),
        ("Aliases", "DefinedAliases"),

        ("EquipmentManager", "Think"),

        ("GameEvents", "Enabled"),
        ("GameEvents", "Profile"),
        ("GameEvents", "GameEventHandlers"),

        ("AutoTinker", "MinPercentage"),
        ("AutoTinker", "MaxTinks"),

        ("Jumper", "PauseNav"),
        ("Jumper", "ThinkComplete"),
        ("Jumper", "ThinkFail"),
        ("Jumper", "Attempts"),

        ("InventoryManager", "AutoCram"),
        ("InventoryManager", "AutoStack"),
        ("InventoryManager", "IGThink"),
        ("InventoryManager", "IGFailure"),
        ("InventoryManager", "IGBusyCount"),
        ("InventoryManager", "IGRange"),
        ("InventoryManager", "IGUIEnabled"),
        ("InventoryManager", "IGWindowX"),
        ("InventoryManager", "IGWindowY"),
        ("InventoryManager", "TreatStackAsSingleItem"),
        ("InventoryManager", "WatchLootProfile"),

        ("VTankControl", "VitalSharing"),
        ("VTankControl", "PatchExpressionEngine"),
        ("VTankControl", "MetaTickRateOverride"),
        ("VTankControl", "FixPortalLoops"),
        ("VTankControl", "PortalLoopCount"),
    ];

    /// <summary>The five fields every nametag group declares.</summary>
    private static readonly string[] NametagFields =
        ["Enabled", "TagColor", "TagSize", "TickerColor", "TickerSize"];

    /// <summary>The nametag groups.</summary>
    private static readonly string[] NametagGroups =
    [
        "Player", "Pet", "AllegiancePlayer", "Portal", "Npc", "Vendor", "Monster",
    ];

    /// <summary>The two fields every map colour group declares.</summary>
    private static readonly string[] MapColourFields = ["Enabled", "Color"];

    /// <summary>The map colour groups, under the map's display group.</summary>
    private static readonly string[] MapColourGroups =
    [
        "DungeonName", "Walls", "InnerWalls", "RampedWalls", "Stairs", "Floors",
        "VisualNavStickyPoint", "VisualNavLines",
    ];

    /// <summary>The five fields every map marker group declares.</summary>
    private static readonly string[] MapMarkerFields =
        ["Enabled", "ShowLabel", "UseIcon", "Color", "Size"];

    /// <summary>The map marker groups, under the map's marker group.</summary>
    private static readonly string[] MapMarkerGroups =
    [
        "You", "Others", "Items", "Monsters", "NPCs", "MyCorpse", "OtherCorpses",
        "Portals", "Containers", "Doors", "EverythingElse",
    ];

    /// <summary>
    /// What each setting is called here, where that differs from the name it
    /// is declared under. Everything not listed keeps its own name.
    /// </summary>
    private static readonly IReadOnlyDictionary<string, string> NamedHere =
        new Dictionary<string, string>(StringComparer.Ordinal)
        {
            ["GameEvents.GameEventHandlers"] = "GameEvents.Handlers",
            ["InventoryManager.IGThink"] = "ItemGiver.Think",
            ["InventoryManager.IGFailure"] = "ItemGiver.FailureLimit",
            ["InventoryManager.IGBusyCount"] = "ItemGiver.BusyRetryLimit",
            ["InventoryManager.IGRange"] = "ItemGiver.Range",
            ["VTankControl.VitalSharing"] = "Sharing.Vitals",
        };

    /// <summary>
    /// The settings deliberately not on the tab, each with the reason. Every
    /// key here must be one the inventory declares, so an exclusion cannot
    /// outlive the setting it excuses.
    /// </summary>
    private static readonly IReadOnlyDictionary<string, string> Excluded =
        new Dictionary<string, string>(StringComparer.Ordinal)
        {
            // Window placement. The host keeps where a panel sits and how
            // big it is, for every panel this plugin owns; a second copy of
            // that in a settings file would fight it.
            ["DungeonMaps.MapWindowX"] = "The host keeps where a window sits.",
            ["DungeonMaps.MapWindowY"] = "The host keeps where a window sits.",
            ["DungeonMaps.MapWindowWidth"] = "The host keeps how big a window is.",
            ["DungeonMaps.MapWindowHeight"] = "The host keeps how big a window is.",
            ["LandscapeMaps.MapWindowX"] = "The host keeps where a window sits.",
            ["LandscapeMaps.MapWindowY"] = "The host keeps where a window sits.",
            ["LandscapeMaps.MapWindowWidth"] = "The host keeps how big a window is.",
            ["LandscapeMaps.MapWindowHeight"] = "The host keeps how big a window is.",
            ["NetworkUI.WindowPositionX"] = "The host keeps where a window sits.",
            ["NetworkUI.WindowPositionY"] = "The host keeps where a window sits.",
            ["InventoryManager.IGWindowX"] = "The host keeps where a window sits.",
            ["InventoryManager.IGWindowY"] = "The host keeps where a window sits.",

            // Already editable, elsewhere, against the same value.
            ["InventoryManager.AutoCram"] =
                "The same switch is already on the Options page and in the "
                + "option catalogue; a second row would be a second copy of "
                + "one value.",
            ["InventoryManager.AutoStack"] =
                "The same switch is already on the Options page and in the "
                + "option catalogue; a second row would be a second copy of "
                + "one value.",

            // Nothing here for them to control.
            ["InventoryManager.IGUIEnabled"] =
                "The host decides whether a plugin panel is shown, and there "
                + "is no separate hand-over window here to show.",
            ["InventoryManager.TreatStackAsSingleItem"] =
                "A hand-over count here is always in units and splits the "
                + "last stack, so there is no second behaviour to pick.",
            ["InventoryManager.WatchLootProfile"] =
                "Plugin storage gives no file-change notification; a loot "
                + "profile is re-read when it is selected.",

            // Switches that exist only to reach into the separate macro
            // plugin the original runs alongside. Everything they patch is
            // this plugin's own code here, so there is nothing for them to
            // turn on or off.
            ["VTankControl.PatchExpressionEngine"] =
                "It swaps the macro plugin's expression evaluator for the "
                + "original's at run time. There is one expression engine "
                + "here, this plugin's own, and it is always the one used, "
                + "so there is no foreign evaluator to replace.",
            ["VTankControl.MetaTickRateOverride"] =
                "It pokes the macro plugin's meta loop from a frame handler "
                + "to make it tick faster than the rate built into it. The "
                + "meta engine here is this plugin's own and runs on its own "
                + "schedule, so there is no foreign loop to poke.",
            ["VTankControl.FixPortalLoops"] =
                "It counts portal exits into the same landcell and then "
                + "calls a repair that the original leaves unwritten -- the "
                + "method is a comment and a bare return -- so the switch "
                + "turns on a counter and nothing else.",
            ["VTankControl.PortalLoopCount"] =
                "It is how many exits that counter waits for, and it goes "
                + "with the switch above it.",
        };

    /// <summary>
    /// Rows on the tab that the inventory does not declare, each with the
    /// reason, so an accidental addition still fails the reconciliation.
    /// </summary>
    private static readonly IReadOnlyDictionary<string, string> AddedHere =
        new Dictionary<string, string>(StringComparer.Ordinal)
        {
            ["ItemGiver.Delay"] =
                "A pause between one hand-over and the next, which the "
                + "original has no setting for; it was added here so a run "
                + "can be slowed to what the server accepts.",
            ["AutoTinker.CharmedSmith"] =
                "Whether the character carries the Charmed Smith "
                + "augmentation, which adds five points to an imbue's "
                + "chance. The original reads the augmentation off the "
                + "character; this client exposes no augmentations, so the "
                + "only way the planned percentage can match what the server "
                + "rolls is for the player to tick it.",
            ["Sharing.CastTag"] =
                "Which of the other clients' casts to take in, by the tag "
                + "their client was started with. The original takes every "
                + "connected client's casts; the tags already exist on the "
                + "peer record here, so a character can listen to its own "
                + "group alone.",
        };

    /// <summary>
    /// What every row that is not part of a repeated group shape ships as,
    /// written from the declarations rather than from the catalogue, in the
    /// text the value shape reads back.
    /// </summary>
    private static readonly (string Name, string Default)[] FlatDefaults =
    [
        ("Nametags.Enabled", "True"),
        ("Nametags.MaxRange", "35"),

        ("DungeonMaps.Enabled", "True"),
        ("DungeonMaps.Debug", "False"),
        ("DungeonMaps.DrawWhenClosed", "True"),
        ("DungeonMaps.ShowVisitedTiles", "True"),
        ("DungeonMaps.VisitedTilesColor", "#FFFF96FF"),
        ("DungeonMaps.ShowCompass", "True"),
        ("DungeonMaps.Opacity", "16"),
        ("DungeonMaps.MapZoom", "4.2"),
        ("DungeonMaps.LabelFontSize", "10"),

        ("LandscapeMaps.Enabled", "True"),
        ("LandscapeMaps.Opacity", "16"),

        ("NetworkUI.Enabled", "False"),
        ("NetworkUI.ShowHudWhenClosed", "True"),
        ("NetworkUI.SelectedTag", "All"),

        ("Aliases.Enabled", "True"),
        ("Aliases.Profile", "[character]"),

        ("AutoVendor.Enabled", "True"),
        ("AutoVendor.EnableBuying", "True"),
        ("AutoVendor.EnableSelling", "True"),
        ("AutoVendor.TestMode", "False"),
        ("AutoVendor.Think", "False"),
        ("AutoVendor.ShowMerchantInfo", "True"),
        ("AutoVendor.OnlyFromMainPack", "False"),
        ("AutoVendor.Tries", "4"),
        ("AutoVendor.TriesTime", "5000"),

        ("AutoXp.StopBeforeMax", "10"),
        ("AutoXp.TriesTime", "300"),
        ("AutoXp.MaxXpChunk", "1000000000"),

        ("EquipmentManager.Think", "False"),

        ("GameEvents.Enabled", "True"),
        ("GameEvents.Profile", "[character]"),

        ("ItemGiver.Range", "15"),
        ("ItemGiver.Delay", "0"),
        ("ItemGiver.BusyRetryLimit", "10"),
        ("ItemGiver.FailureLimit", "3"),
        ("ItemGiver.Think", "False"),

        ("AutoTinker.MinPercentage", "99.5"),
        ("AutoTinker.MaxTinks", "10"),
        ("AutoTinker.CharmedSmith", "False"),

        ("Jumper.PauseNav", "True"),
        ("Jumper.ThinkComplete", "False"),
        ("Jumper.ThinkFail", "False"),
        ("Jumper.Attempts", "3"),

        ("Sharing.Vitals", "True"),
        ("Sharing.CastTag", ""),
    ];

    /// <summary>
    /// The colour each nametag group ships in. Every group ships enabled,
    /// with the same colour on both lines, a tag 0.15m high and a ticker
    /// 0.1m high.
    /// </summary>
    private static readonly (string Group, string Color)[] NametagDefaults =
    [
        ("Player", "#FF00FFFF"),
        ("Pet", "#FF00FFFF"),
        ("AllegiancePlayer", "#FF00FF00"),
        ("Portal", "#FF00FF00"),
        ("Npc", "#FFFFFF00"),
        ("Vendor", "#FFFF00FF"),
        ("Monster", "#FFFF0000"),
    ];

    /// <summary>
    /// The colour each piece of dungeon geometry is drawn in. All eight
    /// ship enabled.
    /// </summary>
    private static readonly (string Group, string Color)[] MapColourDefaults =
    [
        ("DungeonName", "#FFFFFFFF"),
        ("Walls", "#FF00007F"),
        ("InnerWalls", "#FF7FBFFF"),
        ("RampedWalls", "#FF4EA6FF"),
        ("Stairs", "#FF003F7F"),
        ("Floors", "#FF007FBF"),
        ("VisualNavStickyPoint", "#FFADFF2F"),
        ("VisualNavLines", "#FFFF00FF"),
    ];

    /// <summary>
    /// What each map marker ships as. The three switches are spelled out
    /// per group rather than shared, because they are what a mistake in the
    /// order of the group's own parameters would show up as -- a monster is
    /// drawn as its icon with no label, which is the opposite of a corpse.
    /// Every marker ships three pixels across.
    /// </summary>
    private static readonly (string Group, bool Enabled, bool UseIcon, bool ShowLabel, string Color)[]
        MapMarkerDefaults =
    [
        ("You", true, false, false, "#FFFF0000"),
        ("Others", true, true, true, "#FFFFFFFF"),
        ("Items", true, true, true, "#FFFFFFFF"),
        ("Monsters", true, true, false, "#FFFFA500"),
        ("NPCs", true, true, false, "#FFFFFF00"),
        ("MyCorpse", true, true, true, "#FFFF0000"),
        ("OtherCorpses", false, true, false, "#FFF5F5F5"),
        ("Portals", true, true, true, "#FFFFF0FF"),
        ("Containers", true, true, false, "#FFF4A460"),
        ("Doors", true, false, false, "#FFA52A2A"),
        ("EverythingElse", false, true, false, "#FFF5F5F5"),
    ];

    /// <summary>
    /// The list settings and how many lines each ships with. A list's text
    /// is its lines run together, so it is pinned by its lines and not by a
    /// cell in the table above.
    /// </summary>
    private static readonly (string Name, int Lines)[] ListDefaults =
    [
        ("NetworkUI.TrackedItems", 0),
        ("Networking.Tags", 0),
        ("Aliases.DefinedAliases", 0),
        ("GameEvents.Handlers", 0),
    ];

    /// <summary>
    /// Rows whose default is pinned by a test of its own, with the reason.
    /// </summary>
    private static readonly IReadOnlyDictionary<string, string> DefaultPinnedElsewhere =
        new Dictionary<string, string>(StringComparer.Ordinal)
        {
            ["AutoXp.Policy"] =
                "Its default is a weighting table with one line per target, "
                + "which is neither a one-line value nor a count.",
        };

    private static IReadOnlyList<(string Name, string Default)> ShippedDefaults()
    {
        var rows = new List<(string Name, string Default)>(FlatDefaults);
        foreach ((string group, string color) in NametagDefaults)
        {
            rows.Add(($"Nametags.{group}.Enabled", "True"));
            rows.Add(($"Nametags.{group}.TagColor", color));
            rows.Add(($"Nametags.{group}.TagSize", "0.15"));
            rows.Add(($"Nametags.{group}.TickerColor", color));
            rows.Add(($"Nametags.{group}.TickerSize", "0.1"));
        }
        foreach ((string group, string color) in MapColourDefaults)
        {
            rows.Add(($"DungeonMaps.Display.{group}.Enabled", "True"));
            rows.Add(($"DungeonMaps.Display.{group}.Color", color));
        }
        foreach ((string group, bool enabled, bool icon, bool label, string color) in MapMarkerDefaults)
        {
            string prefix = $"DungeonMaps.Display.Markers.{group}.";
            rows.Add((prefix + "Enabled", enabled ? "True" : "False"));
            rows.Add((prefix + "ShowLabel", label ? "True" : "False"));
            rows.Add((prefix + "UseIcon", icon ? "True" : "False"));
            rows.Add((prefix + "Color", color));
            rows.Add((prefix + "Size", "3"));
        }
        return rows;
    }

    private static IReadOnlyList<(string Tool, string Field)> Inventory()
    {
        var rows = new List<(string Tool, string Field)>(FlatSettings);
        foreach (string group in NametagGroups)
            foreach (string field in NametagFields)
                rows.Add(("Nametags", $"{group}.{field}"));
        foreach (string group in MapColourGroups)
            foreach (string field in MapColourFields)
                rows.Add(("DungeonMaps", $"Display.{group}.{field}"));
        foreach (string group in MapMarkerGroups)
            foreach (string field in MapMarkerFields)
                rows.Add(("DungeonMaps", $"Display.Markers.{group}.{field}"));
        return rows;
    }

    private static IReadOnlyList<string> InventoryKeys() =>
        Inventory().Select(static row => $"{row.Tool}.{row.Field}").ToArray();

    /// <summary>
    /// The inventory's own size, so an accidental edit to the tables above
    /// is noticed rather than quietly changing what coverage means. One
    /// hundred and seventy-six leaf values across the in-scope tools.
    /// </summary>
    [Fact]
    public void TheInventoryIsTheOneHundredAndSeventySixLeafValuesTheToolsDeclare()
    {
        IReadOnlyList<string> keys = InventoryKeys();

        Assert.Equal(176, keys.Count);
        Assert.Equal(keys.Count, keys.Distinct(StringComparer.Ordinal).Count());
        Assert.All(keys, static key => Assert.Contains('.', key));
    }

    /// <summary>
    /// The reconciliation, in both directions: every setting the tools
    /// declare is either on the tab or excluded with a reason, and every row
    /// on the tab is either one of those settings or declared as an addition
    /// of ours. Delete a setting from the catalogue and it fails here.
    /// </summary>
    [Fact]
    public void EverySettingTheInScopeToolsDeclareIsOnTheTabOrExcludedWithAReason()
    {
        var onTheTab = new HashSet<string>(
            UbSettingDefinitions.All.Select(static definition => definition.Name),
            StringComparer.Ordinal);

        var shouldBeOnTheTab = new Dictionary<string, string>(StringComparer.Ordinal);
        foreach ((string tool, string field) in Inventory())
        {
            string declared = $"{tool}.{field}";
            if (Excluded.ContainsKey(declared))
                continue;
            shouldBeOnTheTab[
                NamedHere.TryGetValue(declared, out string? here) ? here : declared] = declared;
        }

        string[] missing = shouldBeOnTheTab
            .Where(pair => !onTheTab.Contains(pair.Key))
            .Select(pair => $"{pair.Value} (looked for as {pair.Key})")
            .OrderBy(static text => text, StringComparer.Ordinal)
            .ToArray();
        Assert.True(
            missing.Length == 0,
            "These settings are declared by an in-scope tool and are neither "
            + "on the UB tab nor excluded with a reason:\n  "
            + string.Join("\n  ", missing));

        string[] unaccounted = onTheTab
            .Where(name => !shouldBeOnTheTab.ContainsKey(name) && !AddedHere.ContainsKey(name))
            .OrderBy(static name => name, StringComparer.Ordinal)
            .ToArray();
        Assert.True(
            unaccounted.Length == 0,
            "These rows are on the UB tab but are not settings the in-scope "
            + "tools declare. Add each to AddedHere with the reason it "
            + "exists:\n  " + string.Join("\n  ", unaccounted));

        // 176 declared, 21 excluded, three of our own: the arithmetic is
        // stated so a silent change to any of the three tables shows up.
        Assert.Equal(21, Excluded.Count);
        Assert.Equal(155, shouldBeOnTheTab.Count);
        Assert.Equal(shouldBeOnTheTab.Count + AddedHere.Count, onTheTab.Count);
    }

    [Fact]
    public void EveryExclusionNamesASettingThatExistsAndGivesAReason()
    {
        var declared = new HashSet<string>(InventoryKeys(), StringComparer.Ordinal);

        foreach ((string key, string reason) in Excluded)
        {
            Assert.True(
                declared.Contains(key),
                $"{key} is excluded but no in-scope tool declares it; the "
                + "exclusion has outlived its setting.");
            Assert.False(string.IsNullOrWhiteSpace(reason));
            Assert.EndsWith(".", reason.Trim(), StringComparison.Ordinal);
        }

        foreach ((string key, string reason) in AddedHere)
        {
            Assert.False(
                declared.Contains(key),
                $"{key} is listed as an addition of ours, but it is declared "
                + "by an in-scope tool; it belongs in the inventory instead.");
            Assert.False(string.IsNullOrWhiteSpace(reason));
        }
    }

    [Fact]
    public void EveryRenameNamesASettingThatExistsAndARowThatExists()
    {
        var declared = new HashSet<string>(InventoryKeys(), StringComparer.Ordinal);
        var onTheTab = new HashSet<string>(
            UbSettingDefinitions.All.Select(static definition => definition.Name),
            StringComparer.Ordinal);

        foreach ((string there, string here) in NamedHere)
        {
            Assert.Contains(there, declared);
            Assert.Contains(here, onTheTab);
            Assert.NotEqual(there, here);
        }
    }

    /// <summary>
    /// Every shipped default, against the values written down above rather
    /// than against the catalogue that produced them. Coverage says a row
    /// is on the page; this says the row is the row it is meant to be --
    /// without it, changing a dungeon-map colour to an arbitrary value
    /// passes everything, because only two of the thirty-four colours were
    /// asserted anywhere.
    /// </summary>
    [Fact]
    public void EveryShippedDefaultIsTheValueItWasPortedFrom()
    {
        var catalog = new UbSettingCatalog(new UbSettingValueBag());

        foreach ((string name, string expected) in ShippedDefaults())
            Assert.Equal(expected, catalog.Require(name).Definition.Default.ToStorageString());

        foreach ((string name, int lines) in ListDefaults)
            Assert.Equal(lines, catalog.Require(name).Definition.Default.Items.Count);

        // A third of the catalogue is colours, and all of them are in the
        // tables: 14 on nametags, 16 on the map's geometry, 11 markers and
        // the visited-tile tint.
        Assert.Equal(
            34,
            ShippedDefaults().Count(static row =>
                row.Default.StartsWith('#')));
    }

    /// <summary>
    /// The experience policy is how a spend is shared out, and it ships
    /// with a weight for all six attributes, all three vitals and all
    /// thirty-eight skills. Shipping it empty means nothing would be
    /// raised at all until somebody filled the list in by hand.
    /// </summary>
    [Fact]
    public void TheExperiencePolicyShipsWithTheWeightsItWasPortedFrom()
    {
        var catalog = new UbSettingCatalog(new UbSettingValueBag());
        IReadOnlyList<string> policy = catalog.Require("AutoXp.Policy").Get().Items;

        Assert.Equal(47, policy.Count);
        Assert.Equal("Strength = 1", policy[0]);
        Assert.Equal("Self = 1", policy[5]);
        Assert.Equal("Health = 1.4", policy[6]);
        Assert.Equal("Stamina = 0.1", policy[7]);
        Assert.Equal("Mana = 0.1", policy[8]);
        Assert.Equal("Alchemy = 0", policy[9]);
        Assert.Equal("Jump = 0.02", policy[25]);
        Assert.Equal("MeleeDefense = 5", policy[34]);
        Assert.Equal("WeaponTinkering = 0", policy[46]);

        // Seven targets carry the heaviest weight and ten are switched
        // off, which is what makes the shipped policy a fighter's.
        Assert.Equal(
            7,
            policy.Count(static line => line.EndsWith(" = 10", StringComparison.Ordinal)));
        Assert.Equal(
            10,
            policy.Count(static line => line.EndsWith(" = 0", StringComparison.Ordinal)));

        // One target and one weight a line, with a decimal point rather
        // than whatever this machine calls one.
        Assert.All(
            policy,
            static line => Assert.Matches(@"^[A-Za-z]+ = (0|[1-9][0-9]*)(\.[0-9]+)?$", line));
    }

    /// <summary>
    /// The tables above have to cover the catalogue, or a row could be
    /// added, or its default changed, without anything written down
    /// disagreeing.
    /// </summary>
    [Fact]
    public void EveryRowOnTheTabHasItsDefaultWrittenDownSomewhere()
    {
        var pinned = new HashSet<string>(
            ShippedDefaults().Select(static row => row.Name),
            StringComparer.Ordinal);
        pinned.UnionWith(ListDefaults.Select(static row => row.Name));
        pinned.UnionWith(DefaultPinnedElsewhere.Keys);

        string[] unpinned = UbSettingDefinitions.All
            .Select(static definition => definition.Name)
            .Where(name => !pinned.Contains(name))
            .OrderBy(static name => name, StringComparer.Ordinal)
            .ToArray();
        Assert.True(
            unpinned.Length == 0,
            "These rows are on the tab with no default written down, so a "
            + "change to them would go unnoticed:\n  "
            + string.Join("\n  ", unpinned));

        string[] stale = pinned
            .Where(name => !UbSettingDefinitions.All.Any(
                definition => definition.Name == name))
            .OrderBy(static name => name, StringComparer.Ordinal)
            .ToArray();
        Assert.True(
            stale.Length == 0,
            "These defaults are written down for rows that no longer "
            + "exist:\n  " + string.Join("\n  ", stale));

        foreach ((string _, string reason) in DefaultPinnedElsewhere)
            Assert.EndsWith(".", reason.Trim(), StringComparison.Ordinal);
    }

    /// <summary>
    /// A sanity check on the shapes, not a coverage check: roughly a third
    /// of the catalogue is colours, and five settings are editable lists.
    /// </summary>
    [Fact]
    public void TheShapeOfTheCatalogueIsWhatTheDisplayToolsImply()
    {
        Assert.Equal(
            34,
            UbSettingDefinitions.All.Count(static definition =>
                definition.Kind == UbSettingKind.Color));
        Assert.Equal(
            5,
            UbSettingDefinitions.All.Count(static definition =>
                definition.Kind == UbSettingKind.Collection));
        Assert.DoesNotContain(
            UbSettingDefinitions.All,
            static definition =>
                definition.Name.Contains("Window", StringComparison.OrdinalIgnoreCase));
    }
}

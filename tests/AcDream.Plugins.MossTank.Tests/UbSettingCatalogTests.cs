using System.Globalization;
using AcDream.Plugins.MossTank;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The descriptor layer the whole UB tab hangs off: one row per setting,
/// carrying the dotted name, a line of prose, the value shape, the default,
/// which of the three files it is written to, and a get/set pair.
/// </summary>
public sealed class UbSettingCatalogTests
{
    [Fact]
    public void EveryDefinitionHasADottedNameASummaryAndACategory()
    {
        Assert.NotEmpty(UbSettingDefinitions.All);
        foreach (UbSettingDefinition definition in UbSettingDefinitions.All)
        {
            Assert.Contains('.', definition.Name);
            Assert.False(
                string.IsNullOrWhiteSpace(definition.Summary),
                $"{definition.Name} has no summary, so its row on the page "
                + "would say only its own name back.");
            Assert.EndsWith(".", definition.Summary, StringComparison.Ordinal);
            Assert.False(string.IsNullOrWhiteSpace(definition.Category));
            Assert.Equal(
                definition.Name[..definition.Name.IndexOf('.', StringComparison.Ordinal)],
                definition.Category);
        }
    }

    [Fact]
    public void NoTwoDefinitionsShareAName()
    {
        string[] names = UbSettingDefinitions.All
            .Select(static definition => definition.Name)
            .ToArray();
        Assert.Equal(names.Length, names.Distinct(StringComparer.Ordinal).Count());
    }

    [Fact]
    public void EveryDefaultMatchesTheDeclaredValueShape()
    {
        foreach (UbSettingDefinition definition in UbSettingDefinitions.All)
            Assert.Equal(definition.Kind, definition.Default.Kind);
    }

    [Fact]
    public void EveryEnumSettingOffersItsChoicesAndEveryOtherSettingOffersNone()
    {
        foreach (UbSettingDefinition definition in UbSettingDefinitions.All)
        {
            if (definition.Kind == UbSettingKind.Enum)
                Assert.NotEmpty(definition.Choices);
            else
                Assert.Empty(definition.Choices);
        }
    }

    /// <summary>
    /// The three tiers have to each carry real weight or the tiering is
    /// decoration. How this client looks is one setting for the installation;
    /// how the character behaves is shareable in a named profile; who the
    /// character is belongs to that character alone.
    /// </summary>
    [Fact]
    public void AllThreeScopeTiersAreUsed()
    {
        UbSettingScope[] scopes = UbSettingDefinitions.All
            .Select(static definition => definition.Scope)
            .Distinct()
            .ToArray();
        Assert.Contains(UbSettingScope.Global, scopes);
        Assert.Contains(UbSettingScope.Profile, scopes);
        Assert.Contains(UbSettingScope.Character, scopes);
    }

    [Fact]
    public void TheCharacterTierHoldsOnlyWhoTheCharacterIs()
    {
        string[] characterScoped = UbSettingDefinitions.All
            .Where(static definition => definition.Scope == UbSettingScope.Character)
            .Select(static definition => definition.Name)
            .OrderBy(static name => name, StringComparer.Ordinal)
            .ToArray();
        Assert.Equal(
            [
                "Aliases.Profile", "AutoTinker.CharmedSmith",
                "GameEvents.Profile", "NetworkUI.SelectedTag",
                "Networking.Tags", "Sharing.CastTag",
            ],
            characterScoped);
    }

    /// <summary>
    /// The three-level nesting of the display tools is flattened to dotted
    /// names, because the page has one filter field where the original had a
    /// tree. A nametag group is five rows, a map colour two, a map marker
    /// five.
    /// </summary>
    [Fact]
    public void TheNestedDisplayGroupsAreFlattenedToDottedNames()
    {
        Assert.Contains(
            UbSettingDefinitions.All,
            static definition => definition.Name == "Nametags.Monster.TagColor");
        Assert.Contains(
            UbSettingDefinitions.All,
            static definition =>
                definition.Name == "DungeonMaps.Display.Markers.Portals.Color");
        Assert.Equal(
            5,
            UbSettingDefinitions.All.Count(static definition =>
                definition.Name.StartsWith("Nametags.Player.", StringComparison.Ordinal)));
        Assert.Equal(
            5,
            UbSettingDefinitions.All.Count(static definition =>
                definition.Name.StartsWith(
                    "DungeonMaps.Display.Markers.Doors.", StringComparison.Ordinal)));
        Assert.Equal(
            2,
            UbSettingDefinitions.All.Count(static definition =>
                definition.Name.StartsWith(
                    "DungeonMaps.Display.Walls.", StringComparison.Ordinal)));
    }

    [Fact]
    public void EveryCategoryIsOneOfTheInScopeTools()
    {
        string[] categories = UbSettingDefinitions.All
            .Select(static definition => definition.Category)
            .Distinct(StringComparer.Ordinal)
            .OrderBy(static name => name, StringComparer.Ordinal)
            .ToArray();
        Assert.Equal(
            [
                "Aliases", "AutoTinker", "AutoVendor", "AutoXp", "DungeonMaps",
                "EquipmentManager", "GameEvents", "ItemGiver", "Jumper",
                "LandscapeMaps", "Nametags", "NetworkUI", "Networking",
                "Sharing",
            ],
            categories);
    }

    [Fact]
    public void ABoundRowReadsAndWritesThroughItsGetSetPair()
    {
        var values = new UbSettingValueBag();
        var catalog = new UbSettingCatalog(values);

        UbSetting row = catalog.Require("AutoVendor.Tries");
        Assert.Equal(4, row.Get().AsInt32());

        row.Set(UbSettingValue.FromInt(9));
        Assert.Equal(9, catalog.Require("AutoVendor.Tries").Get().AsInt32());
        Assert.Equal(9, values.Read(UbSettingScope.Profile, "AutoVendor.Tries")!.Value.AsInt32());
    }

    /// <summary>
    /// Reading walks all three tiers, so a value written to the wrong one
    /// still reads back and every symptom of a misdirected write is
    /// invisible from the page. Only the write target says whether the
    /// tiering means anything, so every row's is pinned: one row's value
    /// must appear in the tier it declares and in neither of the others.
    /// </summary>
    [Fact]
    public void EveryRowIsWrittenToTheTierItsScopeDeclaresAndToNoOther()
    {
        var values = new UbSettingValueBag();
        var catalog = new UbSettingCatalog(values);

        foreach (UbSetting row in catalog.Settings)
        {
            row.Set(row.Get());
            foreach (UbSettingScope scope in Enum.GetValues<UbSettingScope>())
            {
                UbSettingValue? written = values.Read(scope, row.Name);
                if (scope == row.Scope)
                    Assert.True(
                        written is not null,
                        $"{row.Name} declares {row.Scope} but nothing was "
                        + $"written there.");
                else
                    Assert.True(
                        written is null,
                        $"{row.Name} declares {row.Scope} but its value also "
                        + $"reached {scope}.");
            }
        }

        // All three targets are exercised by the sweep above, and each by
        // more than a handful of rows.
        Assert.Equal(7, catalog.Settings.Count(static row => row.Scope == UbSettingScope.Global));
        Assert.Equal(145, catalog.Settings.Count(static row => row.Scope == UbSettingScope.Profile));
        Assert.Equal(6, catalog.Settings.Count(static row => row.Scope == UbSettingScope.Character));
    }

    [Fact]
    public void AnUnwrittenRowReadsItsDefault()
    {
        var catalog = new UbSettingCatalog(new UbSettingValueBag());
        Assert.True(catalog.Require("Jumper.PauseNav").Get().Boolean);
        Assert.Equal(
            0xFF00FFFFu,
            catalog.Require("Nametags.Player.TagColor").Get().AsColor());
    }

    /// <summary>
    /// A row whose value has a live owner elsewhere in the plugin reads and
    /// writes that owner, not the value bag: two copies of one number is the
    /// failure this layer exists to prevent.
    /// </summary>
    [Fact]
    public void AnOverriddenRowReachesItsLiveOwnerInsteadOfTheBag()
    {
        var values = new UbSettingValueBag();
        int live = 3;
        var catalog = new UbSettingCatalog(
            values,
            new Dictionary<string, UbSettingBinding>(StringComparer.OrdinalIgnoreCase)
            {
                ["Jumper.Attempts"] = new UbSettingBinding(
                    () => UbSettingValue.FromInt(live),
                    value => live = value.AsInt32()),
            });

        catalog.Require("Jumper.Attempts").Set(UbSettingValue.FromInt(7));

        Assert.Equal(7, live);
        Assert.Equal(7, catalog.Require("Jumper.Attempts").Get().AsInt32());
        Assert.Null(values.Read(UbSettingScope.Profile, "Jumper.Attempts"));
    }

    /// <summary>
    /// What a person sees on the page before touching anything is the
    /// shipped default, so every default has to read as the number it is.
    /// Fifteen of these are single-precision and showed their binary
    /// expansion instead: 0.15 as 0.150000005960464.
    /// </summary>
    [Fact]
    public void EveryShippedDecimalDefaultReadsAsTheNumberItIs()
    {
        var catalog = new UbSettingCatalog(new UbSettingValueBag());

        Assert.Equal("0.15", catalog.Require("Nametags.Player.TagSize").Display());
        Assert.Equal("0.1", catalog.Require("Nametags.Player.TickerSize").Display());
        Assert.Equal("0.15", catalog.Require("Nametags.Monster.TagSize").Display());
        Assert.Equal("0.1", catalog.Require("Nametags.Monster.TickerSize").Display());
        Assert.Equal("4.2", catalog.Require("DungeonMaps.MapZoom").Display());
        Assert.Equal("35", catalog.Require("Nametags.MaxRange").Display());
        Assert.Equal("15", catalog.Require("ItemGiver.Range").Display());
        Assert.Equal("0", catalog.Require("ItemGiver.Delay").Display());

        // All seventeen single-precision rows, not only the ones named above
        // -- fifteen of them showed noise, the sixteenth is a whole number
        // and was right by luck. A decimal row's text is the shortest one
        // that reads back as the same number, which is what it stops being
        // the moment it is widened.
        UbSettingDefinition[] singles = [.. UbSettingDefinitions.All
            .Where(static definition => definition.Kind == UbSettingKind.Single)];
        Assert.Equal(17, singles.Length);
        foreach (UbSettingDefinition definition in singles)
            Assert.Equal(
                definition.Default.AsSingle().ToString(CultureInfo.InvariantCulture),
                definition.Default.ToStorageString());
    }

    [Fact]
    public void RequireRefusesAnUnknownName()
    {
        var catalog = new UbSettingCatalog(new UbSettingValueBag());
        Assert.False(catalog.TryGet("Nope.Missing", out _));
        Assert.Throws<KeyNotFoundException>(() => catalog.Require("Nope.Missing"));
    }

    [Fact]
    public void CategoriesComeBackInOrderWithTheirRows()
    {
        var catalog = new UbSettingCatalog(new UbSettingValueBag());
        Assert.Equal(14, catalog.Categories.Count);
        Assert.Equal("Aliases", catalog.Categories[0]);
        Assert.Equal(158, catalog.Settings.Count);
        Assert.All(
            catalog.Settings,
            static row => Assert.NotNull(row.Definition));
    }
}

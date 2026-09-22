using System.Xml.Linq;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The UB page: the front page of tool switches, which is a view of the
/// same catalogue rows the UB settings page lists.
/// </summary>
public sealed partial class MossTankPanelTests
{
    /// <summary>
    /// Every button on the page, as the pair that matters: what pressing it
    /// does, and which catalogue row it is a view of.
    /// </summary>
    public static TheoryData<string, string> UbMainSwitches() => new()
    {
        { "ToggleUbAutoVendor", "AutoVendor.Enabled" },
        { "ToggleUbAutoVendorTestMode", "AutoVendor.TestMode" },
        { "ToggleUbDungeonMaps", "DungeonMaps.Enabled" },
        { "ToggleUbLandscapeMaps", "LandscapeMaps.Enabled" },
        { "ToggleUbNametags", "Nametags.Enabled" },
        { "ToggleUbVitalSharing", "Sharing.Vitals" },
        { "ToggleUbNetworkUi", "NetworkUI.Enabled" },
        { "ToggleUbAliases", "Aliases.Enabled" },
        { "ToggleUbGameEvents", "GameEvents.Enabled" },
    };

    /// <summary>What the settings page shows in the value column for one row.</summary>
    private static string UbRowValue(MossTankPanel panel, string name)
    {
        int at = panel.UbSettingNames
            .Select(static (rowName, index) => (rowName, index))
            .First(row => string.Equals(row.rowName, name, StringComparison.Ordinal))
            .index;
        return panel.UbSettingValues[at];
    }

    private static void ClickUbRow(MossTankPanel panel, string name)
    {
        int at = panel.UbSettingNames
            .Select(static (rowName, index) => (rowName, index))
            .First(row => string.Equals(row.rowName, name, StringComparison.Ordinal))
            .index;
        panel.ClickUbSettingValue(at);
    }

    private static Action Press(MossTankPanel panel, string actionName) =>
        (Action)typeof(MossTankPanel)
            .GetProperty(actionName)!
            .GetValue(panel)!;

    [Fact]
    public void TheUbTabOpensAheadOfTheSettingsCatalogue()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());

        Assert.True(panel.UbTabEnabled);
        Assert.False(panel.UbSelected);

        panel.ShowUb();

        Assert.True(panel.UbSelected);
        Assert.True(panel.UbVisible);
        Assert.False(panel.UbSettingsSelected);
        Assert.False(panel.UbSettingsVisible);
    }

    /// <summary>
    /// A button is a view of one catalogue row, so pressing it has to move
    /// the very row the settings page lists -- not a copy of it kept on this
    /// page.
    /// Mutation: give the page its own bool field and flip that instead, and
    /// the settings page's value column never changes.
    /// </summary>
    [Theory]
    [MemberData(nameof(UbMainSwitches))]
    public void PressingAToolButtonFlipsTheRowTheSettingsPageLists(
        string actionName,
        string rowName)
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());
        panel.ShowUb();

        string before = UbRowValue(panel, rowName);
        Assert.Contains(before, new[] { "True", "False" });

        Press(panel, actionName)();

        string after = UbRowValue(panel, rowName);
        Assert.NotEqual(before, after);
        Assert.Contains(after, new[] { "True", "False" });

        Press(panel, actionName)();

        Assert.Equal(before, UbRowValue(panel, rowName));
    }

    /// <summary>
    /// The other direction of the same seam: an edit made on the settings
    /// page shows on the button, because the button reads the row rather
    /// than remembering what it last wrote.
    /// Mutation: cache the caption in a field set by the toggle action, and
    /// a flip made from the settings page leaves the button lying.
    /// </summary>
    [Fact]
    public void ACaptionFollowsTheRowWhenTheSettingsPageChangesIt()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());
        panel.ShowUb();

        string before = panel.UbNametagsButtonText;
        bool wasOn = before.EndsWith(": On", StringComparison.Ordinal);
        Assert.Equal(wasOn ? "Name Tags: On" : "Name Tags: Off", before);

        ClickUbRow(panel, "Nametags.Enabled");

        Assert.Equal(wasOn ? "Name Tags: Off" : "Name Tags: On", panel.UbNametagsButtonText);
    }

    /// <summary>
    /// The caption and its colour say the same thing, since the colour is
    /// what carries the state at a glance on a page of nine buttons.
    /// Mutation: return one constant colour and the off state reads green.
    /// </summary>
    [Fact]
    public void TheColourOfAButtonAgreesWithItsCaption()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());
        panel.ShowUb();

        (string caption, uint colour)[] pairs =
        [
            (panel.UbAutoVendorButtonText, panel.UbAutoVendorButtonColor),
            (panel.UbAutoVendorTestButtonText, panel.UbAutoVendorTestButtonColor),
            (panel.UbDungeonMapsButtonText, panel.UbDungeonMapsButtonColor),
            (panel.UbLandscapeMapsButtonText, panel.UbLandscapeMapsButtonColor),
            (panel.UbNametagsButtonText, panel.UbNametagsButtonColor),
            (panel.UbVitalSharingButtonText, panel.UbVitalSharingButtonColor),
            (panel.UbNetworkUiButtonText, panel.UbNetworkUiButtonColor),
            (panel.UbAliasesButtonText, panel.UbAliasesButtonColor),
            (panel.UbGameEventsButtonText, panel.UbGameEventsButtonColor),
        ];

        uint on = 0xFF7CCB6Bu;
        uint off = 0xFFB0A184u;
        Assert.Contains(pairs, static pair => pair.caption.EndsWith("On", StringComparison.Ordinal));
        Assert.Contains(pairs, static pair => pair.caption.EndsWith("Off", StringComparison.Ordinal));
        foreach ((string caption, uint colour) in pairs)
        {
            bool isOn = caption.EndsWith("On", StringComparison.Ordinal);
            Assert.Equal(isOn ? on : off, colour);
        }
    }

    /// <summary>
    /// Flipping a switch from this page reports where the edit landed, the
    /// same way flipping it from the settings page does, because it is the
    /// same write: an unwritable tier is reported rather than silently lost.
    /// Mutation: write straight to the store instead of through the row and
    /// the notice never appears.
    /// </summary>
    [Fact]
    public void FlippingASwitchHereReportsItLikeTheSettingsPageDoes()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());
        panel.ShowUb();

        panel.ToggleUbAliases();

        Assert.StartsWith("Aliases.Enabled is now ", panel.UbSettingNotice, StringComparison.Ordinal);
    }

    /// <summary>
    /// The two buttons at the right of the page are the page's exits: the
    /// catalogue and the tinkering page.
    /// Mutation: bind Settings to ShowUb and the button does nothing at all.
    /// </summary>
    [Fact]
    public void TheSettingsAndTinkerButtonsLeaveForTheirOwnTabs()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());

        panel.ShowUb();
        panel.ShowUbSettings();
        Assert.True(panel.UbSettingsSelected);
        Assert.False(panel.UbSelected);
        Assert.False(panel.TinkerSelected);

        panel.ShowUb();
        panel.ShowTinker();
        Assert.True(panel.TinkerSelected);
        Assert.True(panel.TinkerVisible);
        Assert.True(panel.TinkerTabEnabled);
        Assert.False(panel.UbSelected);
        Assert.False(panel.UbSettingsSelected);
    }

    /// <summary>
    /// The page as authored: nine tool buttons whose caption and colour are
    /// both bound, and the two exits, which are not.
    /// Mutation: drop the colour binding from one button and it draws white
    /// whatever its row says.
    /// </summary>
    [Fact]
    public void EveryToolButtonOnTheUbPageBindsBothItsCaptionAndItsColour()
    {
        XElement group = UbMainGroup();
        XElement[] buttons = group.Elements("button").ToArray();

        Assert.Equal(11, buttons.Length);

        XElement[] tools = buttons
            .Where(static button => ((string?)button.Attribute("text"))?.StartsWith('{') == true)
            .ToArray();
        Assert.Equal(9, tools.Length);
        foreach (XElement button in tools)
        {
            Assert.StartsWith("{", (string?)button.Attribute("color"), StringComparison.Ordinal);
            Assert.StartsWith("{Toggle", (string?)button.Attribute("onclick"), StringComparison.Ordinal);
            Assert.False(string.IsNullOrWhiteSpace((string?)button.Attribute("tooltip")));
        }

        Assert.Single(
            buttons,
            static button => (string?)button.Attribute("onclick") == "{ShowUbSettings}");
        Assert.Single(
            buttons,
            static button => (string?)button.Attribute("onclick") == "{ShowTinker}");
    }

    /// <summary>
    /// The tinkering tab is authored empty on purpose, so the branch that
    /// builds that page adds to this file rather than moving what is here.
    /// Mutation: leave the tab out and the tab row has nowhere for it to go.
    /// </summary>
    [Fact]
    public void TheTinkerTabExistsWithAPageOfItsOwn()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);

        Assert.Single(
            root.Elements("tab"),
            static tab => (string?)tab.Attribute("text") == "Tinker");
        _ = Assert.Single(
            root.Elements("group"),
            static group => (string?)group.Attribute("visible") == "{TinkerVisible}");
    }

    private static XElement UbMainGroup()
    {
        XDocument document = XDocument.Load(
            Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);
        return Assert.Single(
            root.Elements("group"),
            static group => (string?)group.Attribute("visible") == "{UbVisible}");
    }
}

using System.Reflection;
using System.Xml.Linq;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The UB settings page: a filtered list over the whole catalogue, one shared edit
/// field, and the two sub-pages for the shapes one field cannot hold.
/// </summary>
public sealed partial class MossTankPanelTests
{
    private static MossTankPanel UbPanel(MemoryStorage storage)
    {
        var automation = new FakeAutomation { Name = "Acdream", WorldName = "Coldeve" };
        return new MossTankPanel(new FakeHost(automation, storage));
    }

    /// <summary>
    /// The other clients on this computer, reduced to what the two sharing
    /// rows must reach: how often the casts were asked for, and how often
    /// the client records were, which only a tag filter has any reason to
    /// read.
    /// </summary>
    private sealed class PeerNetworkProbe : INetworkAutomation
    {
        public bool IsAvailable => true;
        public List<PluginPeerCast> Casts { get; } = [];
        public int CastCaptures { get; private set; }
        public int ClientCaptures { get; private set; }

        public IReadOnlyList<PluginNetworkClient> CaptureClients()
        {
            ClientCaptures++;
            return [];
        }

        public IReadOnlyList<PluginPeerCast> CaptureCasts(long afterSequence)
        {
            CastCaptures++;
            return Casts.Where(cast => cast.Sequence > afterSequence).ToArray();
        }
    }

    /// <summary>
    /// The sharing switch on the UB page is the one the combat controller
    /// asks before it reads the other clients: off means nothing is read,
    /// and flipping it back on resumes on the next frame. This is the seam
    /// between the page and the controller; the page and the controller
    /// each pass on their own without it.
    /// Mutation: bind the controller to a constant instead of the row and
    /// the count keeps rising with the switch off.
    /// </summary>
    [Fact]
    public void TheSharingSwitchOnThePageGatesWhatIsReadFromTheOtherClients()
    {
        var peers = new PeerNetworkProbe();
        var automation = new FakeAutomation
        {
            Name = "Acdream",
            WorldName = "Coldeve",
            PeerNetwork = peers,
        };
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        panel.OnTick(0.1d);
        Assert.Equal(1, peers.CastCaptures);

        panel.SetUbFilterText("Sharing.Vitals");
        Assert.Equal("True", panel.UbSettingValues[0]);
        panel.ClickUbSettingValue(0);
        Assert.Equal("False", panel.UbSettingValues[0]);
        panel.OnTick(0.1d);
        panel.OnTick(0.1d);
        Assert.Equal(1, peers.CastCaptures);

        panel.ClickUbSettingValue(0);
        panel.OnTick(0.1d);
        Assert.Equal(2, peers.CastCaptures);
    }

    /// <summary>
    /// The tag typed on the UB page is the one the controller filters peers
    /// by. With no tag the client records are never read; with one, a look
    /// that finds casts reads them to learn which peers carry it.
    /// Mutation: bind the tag to a constant and the client records are
    /// never read.
    /// </summary>
    [Fact]
    public void TheCastTagOnThePageReachesTheControllerThatFiltersThePeers()
    {
        var peers = new PeerNetworkProbe();
        peers.Casts.Add(new PluginPeerCast(1, 7u, 0x50000002u, 10u, 70u, 300, 45d, true));
        var automation = new FakeAutomation
        {
            Name = "Acdream",
            WorldName = "Coldeve",
            PeerNetwork = peers,
        };
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        panel.OnTick(0.1d);
        Assert.Equal(1, peers.CastCaptures);
        Assert.Equal(0, peers.ClientCaptures);

        panel.SetUbFilterText("Sharing.CastTag");
        panel.SelectUbSetting(0);
        Assert.Equal(string.Empty, panel.UbSettingValueDraft);
        panel.SubmitUbSetting("debuffs");
        Assert.Equal("debuffs", panel.UbSettingValues[0]);

        peers.Casts.Add(new PluginPeerCast(2, 7u, 0x50000002u, 10u, 70u, 300, 40d, true));
        panel.OnTick(0.1d);
        Assert.Equal(2, peers.CastCaptures);
        Assert.Equal(1, peers.ClientCaptures);
    }

    private sealed class WorldLabelProbe : IWorldLabelAutomation
    {
        public List<PluginWorldLabel[]> Pushes { get; } = [];

        public bool ShowLabels(IReadOnlyList<PluginWorldLabel> labels)
        {
            Pushes.Add([.. labels]);
            return true;
        }
    }

    /// <summary>
    /// The name-tag switch on the UB page is the one the tags read: off
    /// takes every label back from the host, and on hands them over again.
    /// This is the seam between the page and the controller; each passes on
    /// its own without it. Mutation: bind the controller to the catalogue's
    /// defaults instead of the rows and the labels stay up with the switch off.
    /// </summary>
    [Fact]
    public void TheNametagSwitchOnThePageTakesTheLabelsBackFromTheHost()
    {
        var labels = new WorldLabelProbe();
        var automation = new FakeAutomation
        {
            Name = "Acdream",
            WorldName = "Coldeve",
            ObjectId = 0x50000001u,
            WorldLabels = labels,
            NavigationSnapshot = new PluginNavigationSnapshot(
                true, false, 0x50000001u,
                new PluginNavigationPosition(0xA9B40001u, 0d, 0d, 0d, 0f, true),
                false, false),
            WorldObjects =
            [
                new PluginWorldObject(0x80000001u, 1u, "Drudge Slinker", PluginObjectClass.Monster, 0u, 0u, 0u)
                {
                    IsLandscape = true,
                    HasPosition = true,
                    Position = new PluginNavigationPosition(0xA9B40001u, 0.01d, 0d, 0d, 0f, true),
                },
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        panel.OnTick(0.1d);
        Assert.Equal("Drudge Slinker", Assert.Single(Assert.Single(labels.Pushes)).Text);

        panel.SetUbFilterText("Nametags.Enabled");
        Assert.Equal("True", panel.UbSettingValues[0]);
        panel.ClickUbSettingValue(0);
        Assert.Equal("False", panel.UbSettingValues[0]);
        panel.OnTick(0.1d);
        panel.OnTick(0.1d);
        panel.OnTick(0.1d);
        Assert.Equal(2, labels.Pushes.Count);
        Assert.Empty(labels.Pushes[^1]);

        panel.ClickUbSettingValue(0);
        panel.OnTick(0.1d);
        panel.OnTick(0.1d);
        panel.OnTick(0.1d);
        Assert.Equal(3, labels.Pushes.Count);
        Assert.Equal("Drudge Slinker", Assert.Single(labels.Pushes[^1]).Text);
    }

    [Fact]
    public void TheUbSettingsPageOpensWithEverySettingAndEveryToolListed()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());

        Assert.True(panel.UbSettingsTabEnabled);
        Assert.False(panel.UbSettingsSelected);
        panel.ShowUbSettings();
        Assert.True(panel.UbSettingsSelected);
        Assert.True(panel.UbSettingsVisible);

        Assert.Equal(167, panel.UbSettingNames.Count);
        Assert.Equal(panel.UbSettingNames.Count, panel.UbSettingValues.Count);
        Assert.Equal("(all)", panel.UbCategoryNames[0]);
        Assert.Equal(16, panel.UbCategoryNames.Count);
    }

    [Fact]
    public void TheCategoryListNarrowsTheSettingList()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());
        int jumper = panel.UbCategoryNames
            .Select(static (name, index) => (name, index))
            .First(static row => row.name == "Jumper").index;

        panel.SelectUbCategory(jumper);

        Assert.Equal(4, panel.UbSettingNames.Count);
        Assert.All(
            panel.UbSettingNames,
            static name => Assert.StartsWith("Jumper.", name, StringComparison.Ordinal));

        panel.SelectUbCategory(0);
        Assert.Equal(167, panel.UbSettingNames.Count);
    }

    /// <summary>
    /// The filter is what replaces the tree the original had, so it has to
    /// reach both the dotted name and the line of prose under the list.
    /// </summary>
    [Fact]
    public void TheFilterFieldReachesBothTheNameAndTheDescription()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());

        panel.SetUbFilterText("markers.doors");
        Assert.Equal(5, panel.UbSettingNames.Count);

        panel.SetUbFilterText("staircase");
        Assert.Equal(
            ["DungeonMaps.Display.Stairs.Enabled", "DungeonMaps.Display.Stairs.Color"],
            panel.UbSettingNames.OrderByDescending(static name => name).ToArray());

        panel.SetUbFilterText(string.Empty);
        Assert.Equal(167, panel.UbSettingNames.Count);
    }

    [Fact]
    public void ASwitchFlipsWhereItStandsAndASavedValueSurvivesTheSession()
    {
        var storage = new MemoryStorage();
        MossTankPanel panel = UbPanel(storage);
        panel.SetUbFilterText("Jumper.PauseNav");

        Assert.Equal("True", panel.UbSettingValues[0]);
        panel.ClickUbSettingValue(0);
        Assert.Equal("False", panel.UbSettingValues[0]);
        Assert.Contains("Jumper.PauseNav", panel.UbSettingNotice, StringComparison.Ordinal);

        MossTankPanel next = UbPanel(storage);
        next.SetUbFilterText("Jumper.PauseNav");
        Assert.Equal("False", next.UbSettingValues[0]);
    }

    [Fact]
    public void ANumberIsTypedIntoTheSharedFieldAndCommittedOnSubmit()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());
        panel.SetUbFilterText("AutoVendor.Tries");
        panel.SelectUbSetting(0);

        Assert.Equal("4", panel.UbSettingValueDraft);
        panel.SubmitUbSetting("9");

        Assert.Equal("9", panel.UbSettingValues[0]);
    }

    /// <summary>
    /// A typing slip must leave the old value standing and say so. Silently
    /// writing zero is the failure this refusal exists to prevent.
    /// </summary>
    [Fact]
    public void ARefusedValueLeavesTheSettingAloneAndSaysWhy()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());
        panel.SetUbFilterText("AutoVendor.Tries");
        panel.SelectUbSetting(0);

        panel.SubmitUbSetting("nine");

        Assert.Equal("4", panel.UbSettingValues[0]);
        Assert.Contains("unchanged", panel.UbSettingNotice, StringComparison.Ordinal);
    }

    [Fact]
    public void PuttingARowBackToItsDefaultTakesTheSavedValueOutOfItsFile()
    {
        var storage = new MemoryStorage();
        MossTankPanel panel = UbPanel(storage);
        panel.SetUbFilterText("AutoVendor.Tries");
        panel.SelectUbSetting(0);
        panel.SubmitUbSetting("9");
        Assert.Contains(
            "\"AutoVendor.Tries\"",
            storage.Text["mosstank/ub/profiles/default.settings.json"],
            StringComparison.Ordinal);

        panel.ResetUbSettingToDefault();

        Assert.Equal("4", panel.UbSettingValues[0]);
        Assert.DoesNotContain(
            "\"AutoVendor.Tries\"",
            storage.Text["mosstank/ub/profiles/default.settings.json"],
            StringComparison.Ordinal);
    }

    [Fact]
    public void TheDescriptionAndTheFileAnEditLandsInAreBothOnThePage()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());

        panel.SetUbFilterText("Nametags.Enabled");
        panel.SelectUbSetting(0);
        Assert.StartsWith("Nametags.Enabled:", panel.UbSettingDescription, StringComparison.Ordinal);
        Assert.Equal("Saved for this installation.", panel.UbSettingScopeText);

        panel.SetUbFilterText("Networking.Tags");
        panel.SelectUbSetting(0);
        Assert.Equal("Saved for this character only.", panel.UbSettingScopeText);

        panel.SetUbFilterText("AutoVendor.Tries");
        panel.SelectUbSetting(0);
        Assert.Equal("Saved in profile 'default'.", panel.UbSettingScopeText);
    }

    /// <summary>
    /// The sentence under the description says which file the edit is
    /// about to land in, and reading walks all three files, so a write
    /// that went to the wrong one would look right on the page and read
    /// back correctly while the page's own sentence was a lie. Each of the
    /// three targets is pinned here as a file on disk, through the page.
    /// </summary>
    [Theory]
    [InlineData("Nametags.Enabled", "False", "mosstank/ub/settings.json")]
    [InlineData("AutoVendor.Tries", "9", "mosstank/ub/profiles/default.settings.json")]
    [InlineData("NetworkUI.SelectedTag", "raid", "mosstank/ub/Coldeve/Acdream/settings.json")]
    public void AnEditLandsInTheOneFileTheScopeSentenceNames(
        string name,
        string typed,
        string file)
    {
        var storage = new MemoryStorage();
        MossTankPanel panel = UbPanel(storage);
        panel.SetUbFilterText(name);
        panel.SelectUbSetting(0);

        panel.SubmitUbSetting(typed);

        Assert.Contains($"\"{name}\"", storage.Text[file], StringComparison.Ordinal);
        foreach ((string key, string content) in storage.Text)
        {
            if (key == file || !key.StartsWith("mosstank/ub/", StringComparison.Ordinal))
                continue;
            Assert.DoesNotContain($"\"{name}\"", content, StringComparison.Ordinal);
        }
    }

    /// <summary>
    /// A settings file that did not read is left where it is, so a person
    /// can repair it instead of losing it. That is right, but the page
    /// went on reporting every edit as done: the warning came once at
    /// load, and after it every switch, colour, list and typed value said
    /// it had been saved while nothing was written. The edit still applies
    /// for this session; the page has to say which of the two happened.
    /// </summary>
    [Fact]
    public void AnEditToAFileThatDidNotReadAppliesButSaysItIsNotSaved()
    {
        const string damaged = "{ this is not json";
        var storage = new MemoryStorage();
        storage.Text["mosstank/ub/profiles/default.settings.json"] = damaged;
        MossTankPanel panel = UbPanel(storage);
        panel.SetUbFilterText("AutoVendor.Tries");
        panel.SelectUbSetting(0);

        panel.SubmitUbSetting("9");

        Assert.Equal("9", panel.UbSettingValues[0]);
        Assert.Contains(
            "this session only", panel.UbSettingNotice, StringComparison.Ordinal);
        Assert.Contains("repaired", panel.UbSettingNotice, StringComparison.Ordinal);
        Assert.Equal(damaged, storage.Text["mosstank/ub/profiles/default.settings.json"]);

        // Every other way of changing a row says it too, not just the
        // typed field.
        panel.SetUbFilterText("Jumper.PauseNav");
        panel.ClickUbSettingValue(0);
        Assert.Contains(
            "this session only", panel.UbSettingNotice, StringComparison.Ordinal);

        panel.SetUbFilterText("NetworkUI.TrackedItems");
        panel.ClickUbSettingValue(0);
        panel.AddUbListEntry("Prismatic Taper");
        Assert.Contains(
            "this session only", panel.UbSettingNotice, StringComparison.Ordinal);
        panel.HideUbListEditor();

        panel.SetUbFilterText("Nametags.Monster.TagColor");
        panel.ClickUbSettingValue(0);
        panel.SetUbColorGreen(128f);
        panel.ApplyUbColor();
        Assert.Contains(
            "this session only", panel.UbSettingNotice, StringComparison.Ordinal);
    }

    [Fact]
    public void AnEditToAFileThatReadSaysNothingAboutSessions()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());
        panel.SetUbFilterText("AutoVendor.Tries");
        panel.SelectUbSetting(0);

        panel.SubmitUbSetting("9");

        Assert.Equal("AutoVendor.Tries is now 9.", panel.UbSettingNotice);
    }

    // ---- the colour sub-page -------------------------------------------

    [Fact]
    public void ClickingAColourOpensTheSubPageLoadedWithThatColour()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());
        panel.SetUbFilterText("Nametags.Monster.TagColor");

        Assert.False(panel.UbColorVisible);
        panel.ClickUbSettingValue(0);

        Assert.True(panel.UbColorVisible);
        Assert.Equal("#FFFF0000", panel.UbColorHex);
        Assert.Equal(255f, panel.UbColorAlpha);
        Assert.Equal(255f, panel.UbColorRed);
        Assert.Equal(0f, panel.UbColorGreen);
        Assert.Contains("Nametags.Monster.TagColor", panel.UbColorTarget, StringComparison.Ordinal);
    }

    [Fact]
    public void TheSlidersAndTheHexFieldEachDriveTheOther()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());
        panel.SetUbFilterText("Nametags.Monster.TagColor");
        panel.ClickUbSettingValue(0);

        panel.SetUbColorGreen(128f);
        Assert.Equal("#FFFF8000", panel.UbColorHex);
        Assert.Equal("128", panel.UbColorGreenText);

        panel.SubmitUbColorHex("#8000FF00");
        Assert.Equal(128f, panel.UbColorAlpha);
        Assert.Equal(0f, panel.UbColorRed);
        Assert.Equal(255f, panel.UbColorGreen);
    }

    /// <summary>
    /// The swatch on the colour page is bound to the draft, not to the
    /// saved value: it follows every slider and the hex field, and goes
    /// back with Default. Mutation: bind it to the saved row and it stays
    /// red while the draft turns green.
    /// </summary>
    [Fact]
    public void TheSwatchShowsTheDraftColourAsItIsEdited()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());
        panel.SetUbFilterText("Nametags.Monster.TagColor");
        panel.ClickUbSettingValue(0);
        Assert.Equal(0xFFFF0000u, panel.UbColorSwatch);

        panel.SetUbColorGreen(128f);
        Assert.Equal(0xFFFF8000u, panel.UbColorSwatch);
        Assert.Equal("#FFFF0000", panel.UbSettingValues[0]);

        panel.SubmitUbColorHex("#8000FF00");
        Assert.Equal(0x8000FF00u, panel.UbColorSwatch);

        panel.ResetUbColorToDefault();
        Assert.Equal(0xFFFF0000u, panel.UbColorSwatch);
    }

    [Fact]
    public void AColourIsOnlySavedWhenTheSubPageIsSaved()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());
        panel.SetUbFilterText("Nametags.Monster.TagColor");
        panel.ClickUbSettingValue(0);
        panel.SetUbColorGreen(128f);

        panel.HideUbColorEditor();
        Assert.False(panel.UbColorVisible);
        Assert.Equal("#FFFF0000", panel.UbSettingValues[0]);

        panel.ClickUbSettingValue(0);
        panel.SetUbColorGreen(128f);
        panel.ApplyUbColor();

        Assert.False(panel.UbColorVisible);
        Assert.Equal("#FFFF8000", panel.UbSettingValues[0]);
    }

    [Fact]
    public void TheColourButtonSaysSoWhenTheRowIsNotAColour()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());
        panel.SetUbFilterText("Jumper.Attempts");
        panel.SelectUbSetting(0);

        panel.ShowUbColorEditor();

        Assert.False(panel.UbColorVisible);
        Assert.Contains("not a colour", panel.UbSettingNotice, StringComparison.Ordinal);
    }

    // ---- the list sub-page ---------------------------------------------

    [Fact]
    public void OneListSubPageServesEveryListSetting()
    {
        var storage = new MemoryStorage();
        MossTankPanel panel = UbPanel(storage);

        foreach (string name in new[]
        {
            "AutoXp.Policy", "NetworkUI.TrackedItems", "Networking.Tags",
            "Aliases.DefinedAliases", "GameEvents.Handlers",
        })
        {
            panel.SetUbFilterText(name);
            panel.ClickUbSettingValue(0);
            Assert.True(panel.UbListVisible, $"{name} did not open the list page.");
            Assert.StartsWith(name, panel.UbListTitle, StringComparison.Ordinal);
            panel.HideUbListEditor();
        }
    }

    [Fact]
    public void ListEntriesAreAddedReplacedAndRemovedAndSavedAsTheyGo()
    {
        var storage = new MemoryStorage();
        MossTankPanel panel = UbPanel(storage);
        panel.SetUbFilterText("Networking.Tags");
        panel.ClickUbSettingValue(0);

        panel.AddUbListEntry("tank");
        panel.SetUbListDraft("healer");
        panel.AddUbListEntryFromDraft();
        Assert.Equal(["tank", "healer"], panel.UbListRows);
        Assert.Equal("tank, healer", panel.UbSettingValues[0]);

        panel.SelectUbListRow(0);
        panel.SetUbListDraft("main tank");
        panel.ReplaceUbListEntry();
        Assert.Equal(["main tank", "healer"], panel.UbListRows);

        panel.SelectUbListRow(1);
        panel.RemoveUbListEntry();
        Assert.Equal(["main tank"], panel.UbListRows);

        MossTankPanel next = UbPanel(storage);
        next.SetUbFilterText("Networking.Tags");
        Assert.Equal("main tank", next.UbSettingValues[0]);
    }

    [Fact]
    public void AnEmptyListReadsAsEmptyRatherThanAsNothing()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());
        panel.SetUbFilterText("Networking.Tags");

        Assert.Equal("(empty)", panel.UbSettingValues[0]);
    }

    [Fact]
    public void TheListButtonSaysSoWhenTheRowIsNotAList()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());
        panel.SetUbFilterText("Jumper.Attempts");
        panel.SelectUbSetting(0);

        panel.ShowUbListEditor();

        Assert.False(panel.UbListVisible);
        Assert.Contains("not a list", panel.UbSettingNotice, StringComparison.Ordinal);
    }

    // ---- profiles and characters ---------------------------------------

    /// <summary>
    /// A menu can only ever offer a profile that already exists, so without
    /// a way to make one the profile tier -- where all but eleven of the
    /// rows are written, and the whole reason the tier is described as
    /// something you can hand to a friend -- is stuck on 'default' for
    /// good. This walks the path a person actually has: type a name into
    /// the field, press the button, and the new profile is open.
    /// </summary>
    [Fact]
    public void ANewProfileIsMadeOnThePageAndIsWhereTheNextEditLands()
    {
        var storage = new MemoryStorage();
        MossTankPanel panel = UbPanel(storage);
        Assert.Equal(["default"], panel.UbProfileNames);

        panel.SetUbProfileNameDraft("raid");
        panel.CreateUbProfile();

        Assert.Equal("raid", panel.SelectedUbProfile);
        Assert.Equal(["default", "raid"], panel.UbProfileNames);

        panel.SetUbFilterText("AutoVendor.Tries");
        panel.SelectUbSetting(0);
        panel.SubmitUbSetting("9");

        Assert.Contains(
            "\"AutoVendor.Tries\"",
            storage.Text["mosstank/ub/profiles/raid.settings.json"],
            StringComparison.Ordinal);
        Assert.Contains("raid", panel.UbSettingScopeText, StringComparison.Ordinal);

        // The next session on this character opens it again, and the menu
        // still offers it.
        MossTankPanel next = UbPanel(storage);
        Assert.Equal("raid", next.SelectedUbProfile);
        Assert.Equal(["default", "raid"], next.UbProfileNames);
    }

    /// <summary>
    /// The name typed into the field is also taken by pressing Enter in it,
    /// the way every other profile name field on this window works.
    /// </summary>
    [Fact]
    public void TheProfileNameFieldMakesTheProfileOnEnter()
    {
        var storage = new MemoryStorage();
        MossTankPanel panel = UbPanel(storage);

        panel.CreateNamedUbProfile("raid");

        Assert.Equal("raid", panel.SelectedUbProfile);
        Assert.Equal(["default", "raid"], panel.UbProfileNames);
    }

    /// <summary>
    /// Copying is what makes the tier shareable in practice: set a client
    /// up, then hand a copy on under its own name without disturbing the
    /// one you are using.
    /// </summary>
    [Fact]
    public void CopyingMakesANewProfileHoldingWhatTheOpenOneHolds()
    {
        var storage = new MemoryStorage();
        MossTankPanel panel = UbPanel(storage);
        panel.SetUbFilterText("AutoVendor.Tries");
        panel.SelectUbSetting(0);
        panel.SubmitUbSetting("9");

        panel.SetUbProfileNameDraft("raid");
        panel.CopyUbProfile();

        Assert.Equal("raid", panel.SelectedUbProfile);
        Assert.Equal("9", panel.UbSettingValues[0]);
        Assert.Contains(
            "\"AutoVendor.Tries\"",
            storage.Text["mosstank/ub/profiles/raid.settings.json"],
            StringComparison.Ordinal);

        // The one it was copied from is left exactly as it was.
        panel.SubmitUbSetting("11");
        Assert.Contains(
            "\"11\"",
            storage.Text["mosstank/ub/profiles/raid.settings.json"],
            StringComparison.Ordinal);
        Assert.Contains(
            "\"9\"",
            storage.Text["mosstank/ub/profiles/default.settings.json"],
            StringComparison.Ordinal);
    }

    [Fact]
    public void ANewProfileNeedsANameAndWillNotTakeOneAlreadyInUse()
    {
        var storage = new MemoryStorage();
        MossTankPanel panel = UbPanel(storage);

        panel.CreateUbProfile();
        Assert.Equal("default", panel.SelectedUbProfile);
        Assert.Contains("name", panel.UbSettingNotice, StringComparison.OrdinalIgnoreCase);

        panel.SetUbProfileNameDraft("default");
        panel.CreateUbProfile();
        Assert.Equal("default", panel.SelectedUbProfile);
        Assert.Contains("already", panel.UbSettingNotice, StringComparison.Ordinal);
        Assert.Equal(["default"], panel.UbProfileNames);
    }

    [Fact]
    public void AProfileChosenFromTheMenuIsWhereTheNextEditLands()
    {
        var storage = new MemoryStorage();
        MossTankPanel panel = UbPanel(storage);
        panel.SetUbProfileNameDraft("raid");
        panel.CreateUbProfile();

        // Both names are in the menu now, so both are choices a person has.
        panel.SelectUbProfile("default");
        Assert.Equal("default", panel.SelectedUbProfile);
        panel.SelectUbProfile("raid");
        Assert.Equal("raid", panel.SelectedUbProfile);

        panel.SetUbFilterText("AutoVendor.Tries");
        panel.SelectUbSetting(0);
        panel.SubmitUbSetting("9");

        Assert.Contains("mosstank/ub/profiles/raid.settings.json", storage.Text.Keys);
        Assert.Contains("raid", panel.UbSettingScopeText, StringComparison.Ordinal);
    }

    [Fact]
    public void ChangingTheSubPageShutsTheOtherOne()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());

        panel.SetUbFilterText("Nametags.Monster.TagColor");
        panel.ClickUbSettingValue(0);
        Assert.True(panel.UbColorVisible);

        panel.SetUbFilterText("Networking.Tags");
        panel.ClickUbSettingValue(0);

        Assert.True(panel.UbListVisible);
        Assert.False(panel.UbColorVisible);
    }

    // ---- the settings that shipped without an editor --------------------

    /// <summary>
    /// Five settings shipped in this campaign with nowhere to change them:
    /// the four numbers that pace handing items over, and how many times a
    /// jump is tried. They are on the page now, and their rows reach the
    /// running settings rather than a second copy in the settings files.
    /// </summary>
    [Theory]
    [InlineData("ItemGiver.Range", "15", "22", "InventoryGiveRangeMeters")]
    [InlineData("ItemGiver.Delay", "0", "1.5", "InventoryGiveDelaySeconds")]
    [InlineData("ItemGiver.BusyRetryLimit", "10", "7", "InventoryGiveBusyRetryLimit")]
    [InlineData("ItemGiver.FailureLimit", "3", "5", "InventoryGiveFailureLimit")]
    [InlineData("Jumper.Attempts", "3", "8", "NavigationJumpAttempts")]
    public void ASettingWithNoEditorIsNowEditedAndComesBackAfterAReload(
        string name,
        string shipped,
        string typed,
        string storedAs)
    {
        var storage = new MemoryStorage();
        MossTankPanel panel = UbPanel(storage);
        panel.SetUbFilterText(name);
        panel.SelectUbSetting(0);
        Assert.Equal(shipped, panel.UbSettingValues[0]);

        panel.SubmitUbSetting(typed);
        Assert.Equal(typed, panel.UbSettingValues[0]);

        // It went to the profile the macro actually reads, not to a second
        // copy of the number in the new settings tree.
        string sideCar = Assert.Single(
            storage.Text,
            static pair => pair.Key.Contains("sidecar", StringComparison.Ordinal)).Value;
        Assert.Contains(storedAs, sideCar, StringComparison.Ordinal);
        Assert.DoesNotContain(
            storage.Text.Keys,
            key => key.StartsWith("mosstank/ub/", StringComparison.Ordinal)
                && storage.Text[key].Contains(name, StringComparison.Ordinal));

        MossTankPanel reloaded = UbPanel(storage);
        reloaded.SetUbFilterText(name);
        Assert.Equal(typed, reloaded.UbSettingValues[0]);
    }

    /// <summary>
    /// The running settings have bounds of their own, and the page shows the
    /// value that will actually be used rather than the one that was typed.
    /// </summary>
    [Theory]
    [InlineData("Jumper.Attempts", "999", "20")]
    [InlineData("Jumper.Attempts", "0", "1")]
    [InlineData("ItemGiver.Range", "5000", "1000")]
    [InlineData("ItemGiver.Delay", "-4", "0")]
    [InlineData("ItemGiver.BusyRetryLimit", "0", "1")]
    [InlineData("ItemGiver.FailureLimit", "900", "100")]
    public void AnOutOfRangeValueIsHeldToTheBoundTheMacroUses(
        string name,
        string typed,
        string held)
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());
        panel.SetUbFilterText(name);
        panel.SelectUbSetting(0);

        panel.SubmitUbSetting(typed);

        Assert.Equal(held, panel.UbSettingValues[0]);
    }

    [Fact]
    public void PuttingALiveOwnedRowBackToItsDefaultWritesTheDefaultIntoIt()
    {
        var storage = new MemoryStorage();
        MossTankPanel panel = UbPanel(storage);
        panel.SetUbFilterText("Jumper.Attempts");
        panel.SelectUbSetting(0);
        panel.SubmitUbSetting("8");

        panel.ResetUbSettingToDefault();

        Assert.Equal("3", panel.UbSettingValues[0]);
        MossTankPanel reloaded = UbPanel(storage);
        reloaded.SetUbFilterText("Jumper.Attempts");
        Assert.Equal("3", reloaded.UbSettingValues[0]);
    }

    /// <summary>
    /// Saving every row in turn and then reading the new settings files
    /// back: the rows that are missing from them are exactly the ones
    /// whose value belongs to something running: the five running numbers,
    /// the two event rows that are the meta profile itself, and the alias
    /// list, which lives in the file its profile row names. Any other row
    /// missing here would be a setting that quietly saves nowhere.
    /// </summary>
    [Fact]
    public void ExactlyTheSettingsWithALiveOwnerStayOutOfTheNewFiles()
    {
        var storage = new MemoryStorage();
        MossTankPanel panel = UbPanel(storage);
        string[] names = [.. panel.UbSettingNames];
        Assert.Equal(167, names.Length);

        for (int row = 0; row < names.Length; row++)
        {
            panel.SelectUbSetting(row);
            panel.SubmitUbSettingDraft();
        }

        string files = string.Join(
            '\n',
            storage.Text
                .Where(static pair => pair.Key.StartsWith("mosstank/ub/", StringComparison.Ordinal))
                .Select(static pair => pair.Value));
        string[] absent = names
            .Where(name => !files.Contains($"\"{name}\"", StringComparison.Ordinal))
            .OrderBy(static name => name, StringComparer.Ordinal)
            .ToArray();

        Assert.Equal(
            [
                "Aliases.DefinedAliases",
                "GameEvents.Handlers", "GameEvents.Profile",
                "ItemGiver.BusyRetryLimit", "ItemGiver.Delay",
                "ItemGiver.FailureLimit", "ItemGiver.Range", "Jumper.Attempts",
            ],
            absent);
    }

    [Fact]
    public void LeavingTheTabShutsBothSubPages()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());
        panel.ShowUbSettings();
        panel.SetUbFilterText("Nametags.Monster.TagColor");
        panel.ClickUbSettingValue(0);

        panel.ShowOptions();

        Assert.False(panel.UbColorVisible);
        Assert.False(panel.UbListVisible);
        Assert.False(panel.UbSettingsVisible);
    }

    /// <summary>
    /// The vendor commands reach the vendor run through the router, each in
    /// the reference's words: with no shop open a run cannot start, a
    /// staging command names what it read and then says no vendor is open,
    /// an open is called off without a word, and a stop says the run is
    /// finished whether or not one was going. Mutation: dropping the two
    /// cases from the router turns every one of these into the help text.
    /// </summary>
    [Fact]
    public void VendorCommandsAreRoutedToTheVendorRun()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Acdream", WorldName = "Coldeve" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        automation.Messages.Clear();

        panel.ExecuteUbCommand(new PluginCommand("ub", "autovendor", "/ub autovendor"));
        panel.ExecuteUbCommand(new PluginCommand("ub", "vendor addbuy 5 Ration", "/ub vendor addbuy 5 Ration"));
        panel.ExecuteUbCommand(new PluginCommand("ub", "vendor opencancel", "/ub vendor opencancel"));
        panel.ExecuteUbCommand(new PluginCommand("ub", "autovendor stop", "/ub autovendor stop"));

        Assert.Equal(
            [
                "[UB] AutoVendor Fatal - no vendor, cannot start",
                "[UB] Name: Ration count: 5 param:5 Ration",
                "[UB] Error: addbuy: No vendor open",
                "[UB] AutoVendor finished: ",
            ],
            automation.Messages);
    }

    /// <summary>
    /// The experience commands reach the spender through the router and the
    /// spender reads and writes the policy row on the page: an import shows
    /// in the next export, and in the row itself. Mutation: binding the
    /// spender to a copy of the row leaves the export at the shipped policy.
    /// </summary>
    [Fact]
    public void ExperienceCommandsReadAndWriteThePolicyRowOnThePage()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Acdream", WorldName = "Coldeve" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        automation.Messages.Clear();

        panel.ExecuteUbCommand(new PluginCommand("ub", "xp import Alchemy=7;Bogus=1", "/ub xp import Alchemy=7;Bogus=1"));
        panel.ExecuteUbCommand(new PluginCommand("ub", "xp export", "/ub xp export"));

        Assert.Contains(automation.Messages, m => m.Contains("Bogus", StringComparison.Ordinal));
        string export = automation.Messages.Last(m => m.StartsWith("[UB] Strength=", StringComparison.Ordinal));
        Assert.Contains("Alchemy=7", export, StringComparison.Ordinal);
        Assert.StartsWith("[UB] Strength=1;Endurance=1;", export, StringComparison.Ordinal);
    }

    /// <summary>
    /// The handlers row is an editor over the meta profile, not a list of
    /// its own: a line added on the settings page is a rule on the meta tab,
    /// a handler-shaped rule added on the meta tab is a line on the settings
    /// page, and both survive a reopen through the one meta profile store.
    /// Mutation: storing the row in the settings file shows two copies that
    /// drift apart.
    /// </summary>
    [Fact]
    public void HandlerLinesAreMetaRulesAndMetaRulesAreHandlerLines()
    {
        var storage = new MemoryStorage();
        MossTankPanel panel = UbPanel(storage);

        panel.SetUbFilterText("GameEvents.Handlers");
        panel.ClickUbSettingValue(0);
        panel.AddUbListEntry("LoginComplete = /framerate");

        Assert.Equal(["LoginComplete = /framerate"], panel.UbListRows);
        Assert.Single(panel.MetaRows);
        Assert.Equal(MetaEngine.DefaultState, panel.MetaStateColumn[0]);
        Assert.Contains("LoginComplete", panel.MetaConditionColumn[0], StringComparison.Ordinal);
        Assert.Contains("/framerate", panel.MetaActionColumn[0], StringComparison.Ordinal);

        panel.HideUbListEditor();
        panel.SetMetaStateDraft(MetaEngine.DefaultState);
        panel.SelectMetaCondition(nameof(MetaConditionKind.CharacterDeath));
        panel.SelectMetaAction(nameof(MetaActionKind.ChatCommand));
        panel.SetMetaActionTextDraft("/f I died");
        panel.AddMetaRule();
        Assert.Equal("LoginComplete = /framerate, CharacterDeath = /f I died", panel.UbSettingValues[0]);

        MossTankPanel next = UbPanel(storage);
        next.SetUbFilterText("GameEvents.Handlers");
        Assert.Equal("LoginComplete = /framerate, CharacterDeath = /f I died", next.UbSettingValues[0]);
        Assert.Equal(2, next.MetaRows.Count);
        Assert.DoesNotContain(
            storage.Text,
            entry => entry.Value.Contains("GameEvents.Handlers", StringComparison.Ordinal));
    }

    /// <summary>
    /// The equip verbs reach the profile controller through the command
    /// router. A missing profile is said only in the reference's debug
    /// output: nothing with Plugin.Debug off, a debug line (Abuse, 14) with
    /// it on. Mutation: dropping the router case answers with the help text
    /// instead; printing the missing profile as an ordinary line shows it
    /// with debug off.
    /// </summary>
    [Fact]
    public void EquipCommandsRouteThroughThePanelToChat()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Acdream", WorldName = "Coldeve" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        automation.Messages.Clear();

        panel.ExecuteUbCommand(new PluginCommand("ub", "equip list", "/ub equip list"));
        panel.ExecuteUbCommand(new PluginCommand("ub", "equip load nothing", "/ub equip load nothing"));

        Assert.Contains(("[UB] Equip Profiles:", UbChat.GenericChatType), automation.Posted);
        Assert.DoesNotContain(automation.Messages, m => m.Contains("No equip profile exists", StringComparison.Ordinal));

        panel.ExecuteUbCommand(new PluginCommand("ub", "opt set Plugin.Debug true", "/ub opt set Plugin.Debug true"));
        automation.Posted.Clear();
        panel.ExecuteUbCommand(new PluginCommand("ub", "equip load nothing", "/ub equip load nothing"));

        (string text, int kind) = Assert.Single(automation.Posted);
        Assert.StartsWith("[UB] EquipmentManager: No equip profile exists: ", text, StringComparison.Ordinal);
        Assert.Contains("nothing.utl", text, StringComparison.Ordinal);
        Assert.Equal(UbChat.DebugChatType, kind);
    }
}

/// <summary>
/// The aliases on the UB page: the list row is the alias file the profile
/// row names, and the one interceptor the panel installs rewrites what is
/// typed by that list.
/// </summary>
public sealed partial class MossTankPanelTests
{
    private static (MossTankPanel Panel, FakeAutomation Automation) UbAliasPanel(MemoryStorage storage)
    {
        var automation = new FakeAutomation { Name = "Acdream", WorldName = "Coldeve" };
        return (new MossTankPanel(new FakeHost(automation, storage)), automation);
    }

    /// <summary>
    /// The panel installs one interceptor, and what it does is decided by
    /// the list as edited on the page. Mutation: bind the controller to a
    /// copy of the list taken at start and the rewrite never happens.
    /// </summary>
    [Fact]
    public void TheAliasListOnThePageRewritesWhatIsTyped()
    {
        (MossTankPanel panel, FakeAutomation automation) = UbAliasPanel(new MemoryStorage());
        Func<string, PluginChatInputDecision> intercept = Assert.Single(automation.Interceptors);

        Assert.Equal(PluginChatInputAction.Pass, intercept("/ubpd").Action);

        panel.SetUbFilterText("Aliases.DefinedAliases");
        panel.ClickUbSettingValue(0);
        panel.AddUbListEntry("^/ubpd$ = /ub propertydump");

        PluginChatInputDecision decision = intercept("/ubpd");
        Assert.Equal(PluginChatInputAction.Rewrite, decision.Action);
        Assert.Equal("/ub propertydump", decision.Text);

        panel.SetUbFilterText("Aliases.Enabled");
        panel.ClickUbSettingValue(0);
        Assert.Equal(PluginChatInputAction.Pass, intercept("/ubpd").Action);

        panel.Dispose();
        Assert.Empty(automation.Interceptors);
    }

    /// <summary>
    /// The list row reads and writes the alias file the profile row names:
    /// the character's own until the profile is set, then the named one,
    /// which another character pointing at the same name shares.
    /// </summary>
    [Fact]
    public void TheAliasListIsTheFileTheProfileRowNames()
    {
        var storage = new MemoryStorage();
        (MossTankPanel panel, _) = UbAliasPanel(storage);
        panel.SetUbFilterText("Aliases.DefinedAliases");
        panel.ClickUbSettingValue(0);
        panel.AddUbListEntry("^/mine$ = /say mine");
        Assert.Equal("^/mine$ = /say mine\n", storage.Text["mosstank/ub/Coldeve/Acdream/aliases.txt"]);
        Assert.DoesNotContain(
            "Aliases.DefinedAliases",
            string.Join(
                '\n',
                storage.Text
                    .Where(static pair => pair.Key.EndsWith(".json", StringComparison.Ordinal))
                    .Select(static pair => pair.Value)),
            StringComparison.Ordinal);
        panel.HideUbListEditor();

        storage.Text["mosstank/ub/profiles/shared.aliases.txt"] = "^/ours$ = /say ours\n";
        panel.SetUbFilterText("Aliases.Profile");
        panel.SelectUbSetting(0);
        panel.SubmitUbSetting("shared");

        panel.SetUbFilterText("Aliases.DefinedAliases");
        Assert.Equal("^/ours$ = /say ours", panel.UbSettingValues[0]);
        panel.ClickUbSettingValue(0);
        Assert.Equal(["^/ours$ = /say ours"], panel.UbListRows);
        panel.AddUbListEntry("^/more$ = /say more");
        Assert.Equal(
            "^/ours$ = /say ours\n^/more$ = /say more\n",
            storage.Text["mosstank/ub/profiles/shared.aliases.txt"]);
        Assert.Equal("^/mine$ = /say mine\n", storage.Text["mosstank/ub/Coldeve/Acdream/aliases.txt"]);

        // Another character on the same computer, pointed at the same name.
        var other = new FakeAutomation { Name = "Horan", WorldName = "Coldeve" };
        var otherPanel = new MossTankPanel(new FakeHost(other, storage));
        otherPanel.SetUbFilterText("Aliases.Profile");
        otherPanel.SelectUbSetting(0);
        otherPanel.SubmitUbSetting("shared");
        otherPanel.SetUbFilterText("Aliases.DefinedAliases");
        Assert.Equal(
            "^/ours$ = /say ours, ^/more$ = /say more",
            otherPanel.UbSettingValues[0]);
    }
}

/// <summary>
/// Editing the alias list on the list page: a line goes in and comes back
/// out the same, and a line that is not an alias is refused with the
/// reason on the page rather than dropped without a word.
/// </summary>
public sealed partial class MossTankPanelTests
{
    private static MossTankPanel OpenAliasList(MemoryStorage storage)
    {
        MossTankPanel panel = UbPanel(storage);
        panel.SetUbFilterText("Aliases.DefinedAliases");
        panel.ClickUbSettingValue(0);
        Assert.True(panel.UbListVisible);
        return panel;
    }

    [Fact]
    public void AnAliasLineRoundTripsThroughThePage()
    {
        const string line = "[eat] ^/lsr$ -> actiontrycastbyid[1635]";
        var storage = new MemoryStorage();
        MossTankPanel panel = OpenAliasList(storage);

        panel.AddUbListEntry(line);
        Assert.Equal([line], panel.UbListRows);
        Assert.Equal(string.Empty, panel.UbListNotice);
        Assert.Equal(line + "\n", storage.Text["mosstank/ub/Coldeve/Acdream/aliases.txt"]);

        panel.HideUbListEditor();
        panel.ClickUbSettingValue(0);
        Assert.Equal([line], panel.UbListRows);
    }

    /// <summary>
    /// Mutation: take the check out of the add path and the bad line lands
    /// in the rows and the file with the notice empty.
    /// </summary>
    [Fact]
    public void AMalformedAliasIsRefusedWithTheReasonOnThePage()
    {
        var storage = new MemoryStorage();
        MossTankPanel panel = OpenAliasList(storage);

        panel.AddUbListEntry("^(/x$ = /y");
        Assert.Empty(panel.UbListRows);
        Assert.Contains("regular expression", panel.UbListNotice, StringComparison.Ordinal);
        Assert.False(storage.Text.ContainsKey("mosstank/ub/Coldeve/Acdream/aliases.txt"));

        panel.AddUbListEntry("just words");
        Assert.Empty(panel.UbListRows);
        Assert.Contains("pattern = command", panel.UbListNotice, StringComparison.Ordinal);

        panel.AddUbListEntry("^/ok$ = /say ok");
        Assert.Equal(["^/ok$ = /say ok"], panel.UbListRows);
        Assert.Equal(string.Empty, panel.UbListNotice);

        panel.SelectUbListRow(0);
        panel.SetUbListDraft("^(/x$ = /y");
        panel.ReplaceUbListEntry();
        Assert.Equal(["^/ok$ = /say ok"], panel.UbListRows);
        Assert.Contains("regular expression", panel.UbListNotice, StringComparison.Ordinal);
        Assert.Equal("^/ok$ = /say ok\n", storage.Text["mosstank/ub/Coldeve/Acdream/aliases.txt"]);
    }

    /// <summary>
    /// A line already in the file that is not an alias, from a hand edit
    /// or an older format, is shown in its place and named on the page, so
    /// it can be repaired rather than lost.
    /// </summary>
    [Fact]
    public void ABadLineAlreadyInTheFileIsShownAndNamed()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/ub/Coldeve/Acdream/aliases.txt"] = "^/ok$ = /say ok\nnot an alias\n";
        MossTankPanel panel = OpenAliasList(storage);

        Assert.Equal(["^/ok$ = /say ok", "not an alias"], panel.UbListRows);
        Assert.Contains("Line 2", panel.UbListNotice, StringComparison.Ordinal);

        panel.SelectUbListRow(1);
        panel.SetUbListDraft("^/fixed$ = /say fixed");
        panel.ReplaceUbListEntry();
        Assert.Equal(string.Empty, panel.UbListNotice);
    }

    /// <summary>The other list rows take any line; only the aliases are checked.</summary>
    [Fact]
    public void OtherListsAreNotHeldToTheAliasFormat()
    {
        MossTankPanel panel = UbPanel(new MemoryStorage());
        panel.SetUbFilterText("NetworkUI.TrackedItems");
        panel.ClickUbSettingValue(0);
        panel.AddUbListEntry("just words");
        Assert.Equal(["just words"], panel.UbListRows);
        Assert.Equal(string.Empty, panel.UbListNotice);
    }

    /// <summary>
    /// Every setting's description fits the labels beside the list: the
    /// markup's description labels are read, each row is selected in turn,
    /// and what each label would draw is measured in the font the client
    /// draws it with; nothing may be wider than the label, and nothing may
    /// be lost. The longest is pinned by name so a change to it is seen.
    /// Mutation: bind the whole sentence to one label and the longest
    /// summaries run off the window, as they did.
    /// </summary>
    [Fact]
    public void EverySettingDescriptionFitsTheLabelsBesideTheList()
    {
        XDocument document = XDocument.Load(Path.Combine(AppContext.BaseDirectory, "mosstank.xml"));
        XElement root = Assert.IsType<XElement>(document.Root);
        XElement page = Assert.Single(
            root.Elements("group"),
            static candidate => (string?)candidate.Attribute("visible") == "{UbSettingsVisible}");
        XElement[] labels = page.Elements("label")
            .Where(static label =>
                ((string?)label.Attribute("text") ?? string.Empty)
                    .StartsWith("{UbSettingDescription", StringComparison.Ordinal))
            .ToArray();
        Assert.NotEmpty(labels);
        double width = labels.Min(static label => (double)(float)label.Attribute("w")!);
        PropertyInfo[] bound = labels
            .Select(static label => ((string?)label.Attribute("text"))![1..^1])
            .Select(static name => typeof(MossTankPanel).GetProperty(name)
                ?? throw new InvalidOperationException($"{name} is not bound."))
            .ToArray();

        var panel = UbPanel(new MemoryStorage());
        string longest = string.Empty;
        double longestWidth = 0d;
        int longestLines = 0;
        foreach (UbSettingDefinition definition in UbSettingDefinitions.All)
        {
            panel.SetUbFilterText(definition.Name);
            int index = panel.UbSettingNames.ToList().IndexOf(definition.Name);
            Assert.True(index >= 0, definition.Name);
            panel.SelectUbSetting(index);

            string[] lines = bound
                .Select(property => (string)property.GetValue(panel)!)
                .ToArray();
            string whole = $"{definition.Name}: {definition.Summary}";
            Assert.Equal(whole, string.Join(' ', lines.Where(static line => line.Length != 0)));
            foreach (string line in lines)
                Assert.True(
                    InterfaceFontMetrics.Measure(line) <= width,
                    $"{definition.Name}: '{line}' is {InterfaceFontMetrics.Measure(line)} px in a {width} px label.");

            double wholeWidth = InterfaceFontMetrics.Measure(whole);
            if (wholeWidth > longestWidth)
                (longest, longestWidth, longestLines) =
                    (definition.Name, wholeWidth, lines.Count(static line => line.Length != 0));
        }
        // The longest sentence in the catalogue, and the lines it takes of
        // the four; a fifth would be lost, and the equality above says so.
        Assert.Equal("GameEvents.Handlers", longest);
        Assert.Equal(3, longestLines);
        Assert.True(longestLines <= labels.Length);
    }
    /// <summary>
    /// The UB tree belongs to the person, not to the plugin: settings,
    /// aliases, markers, dungeon records and the profile folders are all
    /// files a user edits, copies between installations and shares, so they
    /// live in the shared profile directory. The plugin's own storage keeps
    /// only state the plugin keeps for itself, and nothing UB-shaped may
    /// ever land there again.
    /// Mutation: point the settings store, the marker catalogue or the
    /// equip controller back at the plugin's storage, or set
    /// UbSettingStore.Root back to "ub/", and a key or a folder appears.
    /// </summary>
    [Fact]
    public void NoUbFileOrFolderEverLandsInThePluginsOwnStorage()
    {
        var plugin = new MemoryStorage();
        var vtank = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Acdream", WorldName = "Coldeve" };
        var panel = new MossTankPanel(new FakeHost(automation, plugin, null, vtank));

        // A login's folder tree, a settings write, the marker file the map
        // lays down when its folder is empty, and an equip profile save.
        panel.OnTick(0.1d);
        panel.SetUbFilterText("Sharing.Vitals");
        panel.ClickUbSettingValue(0);
        panel.ShowUbMap();
        UbCommand(panel, "equip create pin");

        Assert.DoesNotContain(
            plugin.Text.Keys,
            key => key.StartsWith(LegacyUbRoot, StringComparison.OrdinalIgnoreCase));
        Assert.DoesNotContain(
            plugin.Directories,
            folder => folder.StartsWith(LegacyUbRoot, StringComparison.OrdinalIgnoreCase));

        // And the same writes did land, so the assertions above are not
        // green because nothing happened at all.
        Assert.Contains(
            vtank.Text.Keys,
            key => key.StartsWith(UbSettingStore.Root, StringComparison.Ordinal));
        Assert.Contains(
            UbSettingStore.Root + "maps/markers.csv",
            (IEnumerable<string>)vtank.Text.Keys);
        Assert.Contains(
            vtank.Text.Keys,
            key => key.StartsWith(UbSettingStore.Root, StringComparison.Ordinal)
                && key.EndsWith("/equip/pin.utl", StringComparison.Ordinal));
    }

    /// <summary>
    /// The files an earlier version left in the plugin's own storage come
    /// across on the first start that finds them, and the person is told
    /// once, in one line, where they went.
    /// Mutation: leave the copy out of the start and a person who upgrades
    /// opens the map to no markers and the settings page to the defaults.
    /// </summary>
    [Fact]
    public void TheUbTreeLeftInThePluginsOwnStorageComesAcrossOnce()
    {
        var plugin = new MemoryStorage();
        plugin.Text["ub/maps/markers.csv"] = "Town, Holtburg, 42.1, 33.6";
        plugin.Text["ub/Coldeve/Acdream/settings.json"] = "{}";
        var vtank = new MemoryStorage { RootPath = Path.Combine("data", "vtank") };
        var automation = new FakeAutomation { Name = "Acdream", WorldName = "Coldeve" };

        var host = new FakeHost(automation, plugin, null, vtank);
        var panel = new MossTankPanel(host);
        panel.OnTick(0.1d);

        Assert.Equal(
            "Town, Holtburg, 42.1, 33.6",
            vtank.ReadText(UbSettingStore.Root + "maps/markers.csv"));
        Assert.Equal("{}", vtank.ReadText(UbSettingStore.Root + "Coldeve/Acdream/settings.json"));
        string line = Assert.Single(
            host.Logger.Infos,
            message => message.Contains("UtilityBelt file(s)", StringComparison.Ordinal));
        Assert.Contains("copied 2 UtilityBelt file(s)", line, StringComparison.Ordinal);
    }

    /// <summary>The prefix the tree used to sit under; nothing writes it now.</summary>
    private const string LegacyUbRoot = "ub/";
}

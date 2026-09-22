using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The marker files the user drops under the UB tree, the zoom bands each
/// kind shows in, and where a name goes when its neighbours are in the way.
/// </summary>
public sealed class LandscapeMarkerCatalogTests
{
    private sealed class MemoryStorage : IPluginStorage
    {
        public Dictionary<string, string> Text { get; } = new(StringComparer.Ordinal);
        public bool IsAvailable => true;
        public string? RootPath => null;
        public string? ReadText(string key) => Text.TryGetValue(key, out string? text) ? text : null;
        public IReadOnlyList<string> List(string prefix) =>
            Text.Keys.Where(key => key.StartsWith(prefix, StringComparison.Ordinal)).ToArray();
        public void WriteText(string key, string content) => Text[key] = content;
        public bool Delete(string key) => Text.Remove(key);
        public IPluginStorage OpenScope(PluginStorageScope scope) => this;
    }

    /// <summary>
    /// Mutation: skip the write when the folder is empty and a fresh
    /// installation has nowhere that shows the format.
    /// </summary>
    [Fact]
    public void AnEmptyFolderGetsTheShippedFileWithTheFormatWrittenInIt()
    {
        var storage = new MemoryStorage();
        var catalog = new LandscapeMarkerCatalog();

        Assert.False(catalog.Load(storage));
        string shipped = Assert.IsType<string>(storage.ReadText(LandscapeMarkerCatalog.ShippedFile));
        Assert.Contains("kind, name, north-south, east-west", shipped);
        Assert.Empty(catalog.Markers);
        Assert.Equal(0, catalog.Revision);
    }

    /// <summary>
    /// Mutation: read only the shipped file and the second file's markers
    /// never show; forget the revision and the map never repaints for them.
    /// </summary>
    [Fact]
    public void EveryCsvUnderTheFolderIsReadAndAChangeBumpsTheRevision()
    {
        var storage = new MemoryStorage();
        storage.WriteText(LandscapeMarkerCatalog.Folder + "towns.csv",
            "# towns\nTown, Holtburg, 42.1, 33.6\n");
        storage.WriteText(LandscapeMarkerCatalog.Folder + "portals.csv",
            "portal, \"Holtburg, Town Portal\", 42.5, 33.9\n");
        storage.WriteText(LandscapeMarkerCatalog.Folder + "notes.txt", "Town, Nope, 1, 1");
        var catalog = new LandscapeMarkerCatalog();

        Assert.True(catalog.Load(storage));
        Assert.Equal(1, catalog.Revision);
        Assert.Equal(
            [
                new LandscapeMarker(LandscapeMarkerKind.Portal, "Holtburg, Town Portal", 42.5d, 33.9d),
                new LandscapeMarker(LandscapeMarkerKind.Town, "Holtburg", 42.1d, 33.6d),
            ],
            catalog.Markers);

        Assert.False(catalog.Load(storage));
        Assert.Equal(1, catalog.Revision);

        storage.WriteText(LandscapeMarkerCatalog.Folder + "towns.csv",
            "Town, Holtburg, 42.1, 33.6\nLifestone, Holtburg Lifestone, 42.3, 33.7\n");
        Assert.True(catalog.Load(storage));
        Assert.Equal(2, catalog.Revision);
        Assert.Equal(3, catalog.Markers.Count);
    }

    /// <summary>
    /// A line with the wrong field count, an unknown kind, a coordinate off
    /// the map or a name of nothing is skipped, and the rest of the file
    /// survives it. Mutation: throw on a bad line and one typo loses a file.
    /// </summary>
    [Theory]
    [InlineData("Town, Holtburg, 42.1", false)]
    [InlineData("Castle, Holtburg, 42.1, 33.6", false)]
    [InlineData("Unknown, Holtburg, 42.1, 33.6", false)]
    [InlineData("Town, , 42.1, 33.6", false)]
    [InlineData("Town, Holtburg, 142.1, 33.6", false)]
    [InlineData("Town, Holtburg, forty, 33.6", false)]
    [InlineData("VENDOR, Aun Tanua, -31.1, 13.7", true)]
    [InlineData("Dungeon,\"Halls, of Metos\",-3.5,88.2", true)]
    public void ALineIsAMarkerOnlyWhenAllFourFieldsParse(string line, bool parses) =>
        Assert.Equal(parses, LandscapeMarkerCatalog.TryParseLine(line, out _));

    [Fact]
    public void ParsingAFileSkipsCommentsBlanksAndBadLines()
    {
        IReadOnlyList<LandscapeMarker> markers = LandscapeMarkerCatalog.Parse(
            "# header\n\nTown, Holtburg, 42.1, 33.6\r\nnonsense\nNpc, Ulgrim, 42.0, 33.5\n");
        Assert.Equal(2, markers.Count);
        Assert.Equal("Ulgrim", markers[1].Name);
        Assert.Equal(LandscapeMarkerKind.Npc, markers[1].Kind);
    }

    /// <summary>
    /// Mutation: swap a kind's two bands and a vendor's name shows before
    /// its icon does.
    /// </summary>
    [Theory]
    [InlineData("Town", 0.0d, true, true)]
    [InlineData("Portal", 0.05d, false, false)]
    [InlineData("Portal", 0.2d, true, false)]
    [InlineData("Portal", 0.32d, true, true)]
    [InlineData("Lifestone", 0.3d, true, false)]
    [InlineData("Vendor", 0.5d, true, false)]
    [InlineData("Vendor", 0.8d, true, true)]
    public void EachKindShowsItsIconAndThenItsNameFromItsOwnZoom(
        string kind, double zoom, bool icon, bool label)
    {
        LandscapeMarkerDisplay display = LandscapeMarkerCatalog.DisplayOf(
            Enum.Parse<LandscapeMarkerKind>(kind));
        Assert.Equal(icon, display.ShowsMarkerAt(zoom));
        Assert.Equal(label, display.ShowsLabelAt(zoom));
    }

    /// <summary>
    /// The eight places around a marker, in the order tried. Mutation:
    /// reorder two and the placement moves for every crowded label.
    /// </summary>
    [Fact]
    public void ANameHasEightCandidatePlacesWalkedClockwiseFromAbove()
    {
        PluginRect[] candidates = MapLabelPlacer.Candidates(100d, 100d, 40d, 12d);
        Assert.Equal(8, candidates.Length);
        Assert.Equal(new PluginRect(80d, 78d, 40d, 12d), candidates[0]);
        Assert.Equal(new PluginRect(104d, 80d, 40d, 12d), candidates[1]);
        Assert.Equal(new PluginRect(108d, 93d, 40d, 12d), candidates[2]);
        Assert.Equal(new PluginRect(108d, 104d, 40d, 12d), candidates[3]);
        Assert.Equal(new PluginRect(80d, 106d, 40d, 12d), candidates[4]);
        Assert.Equal(new PluginRect(50d, 102d, 40d, 12d), candidates[5]);
        Assert.Equal(new PluginRect(50d, 93d, 40d, 12d), candidates[6]);
        Assert.Equal(new PluginRect(54d, 82d, 40d, 12d), candidates[7]);
    }

    /// <summary>
    /// Names piled on one point take the first free place each: above,
    /// then right (top right overlaps above), then below, then left, and
    /// the fifth finds nothing. Mutation: skip the intersection test and
    /// every name lands above its marker.
    /// </summary>
    [Fact]
    public void ANameStepsAroundWhatIsAlreadyPlacedAndGivesUpWhenNothingIsFree()
    {
        var placer = new MapLabelPlacer();
        Assert.True(placer.TryPlace(100d, 100d, 40d, 12d, out PluginRect first));
        Assert.Equal(new PluginRect(80d, 78d, 40d, 12d), first);
        Assert.True(placer.TryPlace(100d, 100d, 40d, 12d, out PluginRect second));
        Assert.Equal(new PluginRect(108d, 93d, 40d, 12d), second);
        Assert.True(placer.TryPlace(100d, 100d, 40d, 12d, out PluginRect third));
        Assert.Equal(new PluginRect(80d, 106d, 40d, 12d), third);
        Assert.True(placer.TryPlace(100d, 100d, 40d, 12d, out PluginRect fourth));
        Assert.Equal(new PluginRect(50d, 93d, 40d, 12d), fourth);
        Assert.False(placer.TryPlace(100d, 100d, 40d, 12d, out _));

        placer.Reset();
        Assert.Empty(placer.Occupied);
        Assert.True(placer.TryPlace(100d, 100d, 40d, 12d, out PluginRect again));
        Assert.Equal(first, again);
    }

    /// <summary>
    /// An icon claimed before the names pushes a name off it. Mutation:
    /// ignore Occupy and the name sits on the icon.
    /// </summary>
    [Fact]
    public void AClaimedIconPushesANameToTheNextFreePlace()
    {
        var placer = new MapLabelPlacer();
        placer.Occupy(new PluginRect(80d, 78d, 40d, 12d));
        Assert.True(placer.TryPlace(100d, 100d, 40d, 12d, out PluginRect placed));
        Assert.Equal(new PluginRect(108d, 93d, 40d, 12d), placed);
    }

    [Fact]
    public void TouchingEdgesDoNotIntersect()
    {
        Assert.False(MapLabelPlacer.Intersects(new PluginRect(0d, 0d, 10d, 10d), new PluginRect(10d, 0d, 10d, 10d)));
        Assert.True(MapLabelPlacer.Intersects(new PluginRect(0d, 0d, 10d, 10d), new PluginRect(9d, 9d, 10d, 10d)));
    }
}

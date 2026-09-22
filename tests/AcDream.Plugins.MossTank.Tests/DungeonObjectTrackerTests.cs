using System.Numerics;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The things drawn on the dungeon map besides the floor: the world
/// objects placed on their storey by marker group, and the cells the
/// player has walked, which are kept between sessions.
/// </summary>
public sealed class DungeonObjectTrackerTests
{
    private const uint Landblock = 0x01A90000u;
    private const uint SelfId = 0x50000001u;
    private const string SelfName = "Acdream";

    /// <summary>
    /// A navigation position that lands on the given landblock-local metres:
    /// the inverse of the plan's own conversion, which the first test pins.
    /// </summary>
    internal static PluginNavigationPosition At(uint landblock, float x, float y, float z, float heading = 0f)
    {
        double lbx = (landblock >> 24) & 0xFFu;
        double lby = (landblock >> 16) & 0xFFu;
        return new PluginNavigationPosition(
            landblock | 0x100u,
            (lbx * 192d + x - 24468d) / 240d,
            (lby * 192d + y - 24468d) / 240d,
            z / 240d,
            heading,
            false);
    }

    private static PluginWorldObject Object(
        uint id,
        string name,
        PluginObjectClass objectClass,
        PluginNavigationPosition position,
        uint wielder = 0u,
        uint container = 0u,
        bool hasPosition = true) =>
        new(id, 0u, name, objectClass, 0u, container, wielder)
        {
            HasPosition = hasPosition,
            Position = position,
            IconId = 0x06001000u + id,
        };

    private static DungeonObjectTracker Tracker() => new();

    private static PluginDungeonFloorplan Plan() => DungeonMapPictureTests.TwoRooms();

    /// <summary>
    /// The test helper is the inverse of the plan's conversion; if the host
    /// ever moves its origin the helper and every test on it says so here.
    /// </summary>
    [Fact]
    public void TheTestPositionHelperRoundTripsThroughThePlan()
    {
        PluginNavigationPosition position = At(Landblock, 15f, 5f, 2f);
        Vector3 local = PluginDungeonFloorplan.ToLandblockLocal(in position);
        Assert.Equal(15f, local.X, 3);
        Assert.Equal(5f, local.Y, 3);
        Assert.Equal(2f, local.Z, 3);
    }

    /// <summary>
    /// Every world object falls into one of the reference's eleven marker
    /// groups, each with its own row of settings. Mutation: sending vendors
    /// to EverythingElse hides every shopkeeper under the default-off group.
    /// </summary>
    [Theory]
    [InlineData(PluginObjectClass.Player, SelfId, "Acdream", DungeonMarkerKind.You)]
    [InlineData(PluginObjectClass.Player, 0x50000002u, "Horan", DungeonMarkerKind.Others)]
    [InlineData(PluginObjectClass.Monster, 0x80000001u, "Drudge", DungeonMarkerKind.Monsters)]
    [InlineData(PluginObjectClass.Npc, 0x80000002u, "Guard", DungeonMarkerKind.NPCs)]
    [InlineData(PluginObjectClass.Vendor, 0x80000003u, "Smith", DungeonMarkerKind.NPCs)]
    [InlineData(PluginObjectClass.Portal, 0x80000004u, "Portal to Holtburg", DungeonMarkerKind.Portals)]
    [InlineData(PluginObjectClass.Corpse, 0x80000005u, "Corpse of Acdream", DungeonMarkerKind.MyCorpse)]
    [InlineData(PluginObjectClass.Corpse, 0x80000006u, "Corpse of Drudge", DungeonMarkerKind.OtherCorpses)]
    [InlineData(PluginObjectClass.Door, 0x80000007u, "Door", DungeonMarkerKind.Doors)]
    [InlineData(PluginObjectClass.Container, 0x80000008u, "Chest", DungeonMarkerKind.Containers)]
    [InlineData(PluginObjectClass.Armor, 0x80000009u, "Breastplate", DungeonMarkerKind.Items)]
    [InlineData(PluginObjectClass.Misc, 0x8000000Au, "Pyreal Mote", DungeonMarkerKind.Items)]
    [InlineData(PluginObjectClass.Lifestone, 0x8000000Bu, "Lifestone", DungeonMarkerKind.EverythingElse)]
    [InlineData(PluginObjectClass.Unknown, 0x8000000Cu, "Thing", DungeonMarkerKind.EverythingElse)]
    public void ObjectsFallIntoTheMarkerGroups(
        PluginObjectClass objectClass,
        uint id,
        string name,
        DungeonMarkerKind expected)
    {
        PluginWorldObject subject = Object(id, name, objectClass, At(Landblock, 5f, 5f, 0f));

        Assert.Equal(expected, DungeonMarkers.Classify(in subject, SelfId, SelfName));
    }

    /// <summary>The group names are the settings rows' own spelling.</summary>
    [Fact]
    public void GroupNamesMatchTheSettingsRows()
    {
        Assert.Equal("NPCs", DungeonMarkers.GroupName(DungeonMarkerKind.NPCs));
        Assert.Equal("MyCorpse", DungeonMarkers.GroupName(DungeonMarkerKind.MyCorpse));
        Assert.Equal("OtherCorpses", DungeonMarkers.GroupName(DungeonMarkerKind.OtherCorpses));
        Assert.Equal("EverythingElse", DungeonMarkers.GroupName(DungeonMarkerKind.EverythingElse));
        Assert.Equal("You", DungeonMarkers.GroupName(DungeonMarkerKind.You));
    }

    /// <summary>
    /// A portal's label is the place, not "Portal to" the place, and an
    /// NPC's label loses the same words; anything else keeps its name.
    /// </summary>
    [Theory]
    [InlineData(PluginObjectClass.Portal, "Portal to Holtburg", "Holtburg")]
    [InlineData(PluginObjectClass.Portal, "Holtburg Portal", "Holtburg")]
    [InlineData(PluginObjectClass.Npc, "Portal to Somewhere", "Somewhere")]
    [InlineData(PluginObjectClass.Monster, "Portal to Somewhere", "Portal to Somewhere")]
    public void PortalAndNpcLabelsLoseThePortalWords(PluginObjectClass objectClass, string name, string label)
    {
        PluginWorldObject subject = Object(1u, name, objectClass, At(Landblock, 5f, 5f, 0f));

        Assert.Equal(label, DungeonMarkers.LabelOf(in subject));
    }

    /// <summary>
    /// The player is placed from the navigation snapshot, everything else
    /// from the capture, each on the storey its height falls in. A capture
    /// that changes nothing reports no change, so the map is not repainted
    /// for it. Mutation: placing by the raw height instead of its band puts
    /// a monster at five metres on the ground storey.
    /// </summary>
    [Fact]
    public void ObjectsArePlacedOnTheStoreyTheirHeightFallsIn()
    {
        DungeonObjectTracker tracker = Tracker();
        PluginDungeonFloorplan plan = Plan();
        PluginWorldObject drudge = Object(0x80000001u, "Drudge", PluginObjectClass.Monster, At(Landblock, 5f, 15f, 5f));
        PluginWorldObject chest = Object(0x80000002u, "Chest", PluginObjectClass.Container, At(Landblock, 15f, 5f, 2f));

        bool changed = tracker.Update([drudge, chest], plan, At(Landblock, 5f, 5f, 0f, 90f), SelfId, SelfName);

        Assert.True(changed);
        Assert.Equal(3, tracker.Objects.Count);
        DungeonTrackedObject you = Assert.Single(tracker.Objects, o => o.Kind == DungeonMarkerKind.You);
        Assert.Equal(SelfId, you.Id);
        Assert.Equal(new Vector3(5f, 5f, 0f), you.Position);
        Assert.Equal(0f, you.LayerZ);
        Assert.True(you.IsMover);
        DungeonTrackedObject monster = Assert.Single(tracker.Objects, o => o.Id == 0x80000001u);
        Assert.Equal(6f, monster.LayerZ);
        Assert.True(monster.IsMover);
        Assert.Equal(0x06001000u + 0x80000001u, monster.IconId);
        DungeonTrackedObject container = Assert.Single(tracker.Objects, o => o.Id == 0x80000002u);
        Assert.Equal(0f, container.LayerZ);
        Assert.False(container.IsMover);

        Assert.False(tracker.Update([drudge, chest], plan, At(Landblock, 5f, 5f, 0f, 90f), SelfId, SelfName));
    }

    /// <summary>
    /// A mover is placed again only once it has gone further than the
    /// reference's threshold, which its distance helper makes just over a
    /// tenth of a metre; a thing that does not move keeps its first place
    /// however far the capture says it drifted. Mutation: re-placing on any
    /// difference repaints the map on every server position tick.
    /// </summary>
    [Fact]
    public void MoversRePlacePastTheThresholdAndStaticsKeepTheirFirstPlace()
    {
        DungeonObjectTracker tracker = Tracker();
        PluginDungeonFloorplan plan = Plan();
        PluginNavigationPosition self = At(Landblock, 5f, 5f, 0f);
        PluginWorldObject drudge = Object(0x80000001u, "Drudge", PluginObjectClass.Monster, At(Landblock, 15f, 5f, 0f));
        PluginWorldObject chest = Object(0x80000002u, "Chest", PluginObjectClass.Container, At(Landblock, 15f, 8f, 0f));
        tracker.Update([drudge, chest], plan, self, SelfId, SelfName);

        PluginWorldObject nudged = drudge with { Position = At(Landblock, 15.05f, 5f, 0f) };
        Assert.False(tracker.Update([nudged, chest], plan, self, SelfId, SelfName));
        Assert.Equal(15f, Assert.Single(tracker.Objects, o => o.Id == drudge.ObjectId).Position.X, 3);

        PluginWorldObject walked = drudge with { Position = At(Landblock, 15.2f, 5f, 0f) };
        Assert.True(tracker.Update([walked, chest], plan, self, SelfId, SelfName));
        Assert.Equal(15.2f, Assert.Single(tracker.Objects, o => o.Id == drudge.ObjectId).Position.X, 3);

        PluginWorldObject drifted = chest with { Position = At(Landblock, 5f, 15f, 6f) };
        Assert.False(tracker.Update([walked, drifted], plan, self, SelfId, SelfName));
        Assert.Equal(new Vector3(15f, 8f, 0f), Assert.Single(tracker.Objects, o => o.Id == chest.ObjectId).Position);

        Assert.True(tracker.Update([walked, drifted], plan, At(Landblock, 5f, 5.2f, 0f), SelfId, SelfName));
        Assert.Equal(0.104d, DungeonObjectTracker.UpdateDistanceMeters, 3);
    }

    /// <summary>
    /// What is not on this map is not tracked: things carried or wielded,
    /// things with no position, things in another landblock, and the
    /// player's own object when the capture carries it (the snapshot places
    /// the player). What leaves the capture leaves the map, and that is a
    /// change.
    /// </summary>
    [Fact]
    public void CarriedUnplacedAndForeignObjectsAreNotTrackedAndDeparturesAreAChange()
    {
        DungeonObjectTracker tracker = Tracker();
        PluginDungeonFloorplan plan = Plan();
        PluginNavigationPosition self = At(Landblock, 5f, 5f, 0f);
        PluginWorldObject wielded = Object(1u, "Sword", PluginObjectClass.MeleeWeapon, At(Landblock, 5f, 5f, 0f), wielder: SelfId);
        PluginWorldObject packed = Object(2u, "Gem", PluginObjectClass.Gem, At(Landblock, 5f, 5f, 0f), container: SelfId);
        PluginWorldObject nowhere = Object(3u, "Ghost", PluginObjectClass.Monster, At(Landblock, 5f, 5f, 0f), hasPosition: false);
        PluginWorldObject abroad = Object(4u, "Drudge", PluginObjectClass.Monster, At(0x01AA0000u, 5f, 5f, 0f));
        PluginWorldObject me = Object(SelfId, SelfName, PluginObjectClass.Player, At(Landblock, 5f, 5f, 0f));
        PluginWorldObject here = Object(5u, "Chest", PluginObjectClass.Container, At(Landblock, 15f, 5f, 0f));

        tracker.Update([wielded, packed, nowhere, abroad, me, here], plan, self, SelfId, SelfName);

        Assert.Equal([SelfId, 5u], tracker.Objects.Select(o => o.Id).OrderDescending().ToArray());
        Assert.True(tracker.Update([], plan, self, SelfId, SelfName));
        Assert.Equal([SelfId], tracker.Objects.Select(o => o.Id).ToArray());
    }

    /// <summary>
    /// The cells the player has stood in are kept per landblock in the
    /// settings tree, so the next session in the same dungeon starts with
    /// them shaded; the reference forgot them on every load. Marking a
    /// cell twice is one change. Mutation: keying the file by cell rather
    /// than landblock loses every other cell.
    /// </summary>
    [Fact]
    public void VisitedCellsAreKeptPerLandblockAcrossSessions()
    {
        var storage = new MemoryStorage();
        var visited = new DungeonVisitedCells(storage);
        visited.Bind(Landblock);

        Assert.True(visited.Mark(Landblock | 0x100u));
        Assert.False(visited.Mark(Landblock | 0x100u));
        Assert.True(visited.Mark(Landblock | 0x101u));
        Assert.True(visited.Contains(Landblock | 0x101u));
        Assert.False(visited.Contains(Landblock | 0x102u));
        Assert.Equal(2, visited.Count);
        Assert.Contains(storage.Keys, key => key.StartsWith("mosstank/ub/", StringComparison.Ordinal) && key.Contains("01A9", StringComparison.Ordinal));

        var again = new DungeonVisitedCells(storage);
        again.Bind(Landblock);
        Assert.True(again.Contains(Landblock | 0x100u));
        Assert.True(again.Contains(Landblock | 0x101u));
        Assert.Equal(2, again.Count);

        again.Bind(0x01AA0000u);
        Assert.Equal(0, again.Count);
        Assert.False(again.Mark(Landblock | 0x100u));
        Assert.Equal(0, again.Count);
    }

    /// <summary>
    /// A file that does not read is treated as empty, and marking still
    /// works for the session.
    /// </summary>
    [Fact]
    public void ADamagedVisitedFileReadsAsEmpty()
    {
        var storage = new MemoryStorage();
        storage.WriteText("mosstank/ub/dungeonmaps/visited/01A9.txt", "not hex\n\n01A90100\n");
        var visited = new DungeonVisitedCells(storage);
        visited.Bind(Landblock);

        Assert.Equal(1, visited.Count);
        Assert.True(visited.Contains(Landblock | 0x100u));
        Assert.True(visited.Mark(Landblock | 0x101u));
    }

    internal sealed class MemoryStorage : IPluginStorage
    {
        private readonly Dictionary<string, string> _text = new(StringComparer.Ordinal);
        public bool IsAvailable => true;
        public IEnumerable<string> Keys => _text.Keys;
        public string? ReadText(string key) => _text.TryGetValue(key, out string? value) ? value : null;
        public void WriteText(string key, string content) => _text[key] = content;
        public bool Delete(string key) => _text.Remove(key);
    }
}

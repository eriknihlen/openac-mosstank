using System.Numerics;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The name tags: one label set handed to the host, rebuilt from one tag per
/// object and re-handed only when it differs; text refreshed once a second;
/// appraisals asked for on the backoff.
/// </summary>
public sealed class NametagControllerTests
{
    private const uint SelfId = 0x50000001u;
    private const uint LevelProperty = 25u;
    private const uint MonarchProperty = 26u;
    private const uint PetOwnerProperty = 44u;
    private const uint PortalDestinationProperty = 38u;
    private const uint AllegianceNameProperty = 47u;

    private static PluginNavigationPosition At(double eastWest, double northSouth) =>
        new(0x00000000u, eastWest, northSouth, 0d, 0f, true);

    private static PluginWorldObject World(
        uint id,
        string name,
        PluginObjectClass objectClass,
        double eastWest = 0.01d,
        double northSouth = 0d,
        bool appraised = false) =>
        new(id, 1u, name, objectClass, 0u, 0u, 0u)
        {
            IsLandscape = true,
            HasPosition = true,
            Position = At(eastWest, northSouth),
            HasAppraisalData = appraised,
        };

    private static PluginItemProperties Properties(
        Dictionary<uint, int>? ints = null,
        Dictionary<uint, string>? strings = null,
        Dictionary<uint, uint>? instanceIds = null) =>
        new(
            ints ?? [],
            new Dictionary<uint, long>(),
            new Dictionary<uint, bool>(),
            new Dictionary<uint, double>(),
            strings ?? [],
            new Dictionary<uint, uint>(),
            instanceIds ?? []);

    private sealed class FakeObjects : IWorldObjectAutomation
    {
        public Dictionary<uint, PluginWorldObject> Objects { get; } = [];
        public Dictionary<uint, PluginItemProperties> Properties { get; } = [];
        public List<uint> Identified { get; } = [];

        public bool IsAvailable => true;

        public IReadOnlyList<PluginWorldObject> CaptureObjects() => [.. Objects.Values];

        public bool TryGet(uint objectId, out PluginWorldObject value) =>
            Objects.TryGetValue(objectId, out value!);

        public bool TryCaptureProperties(uint objectId, out PluginItemProperties properties)
        {
            if (!Objects.ContainsKey(objectId))
            {
                properties = default;
                return false;
            }
            properties = Properties.TryGetValue(objectId, out PluginItemProperties held)
                ? held
                : Properties(null, null, null);
            return true;
        }

        public PluginItemCommandResult Identify(uint objectId)
        {
            Identified.Add(objectId);
            return default;
        }
    }

    private sealed class FakeLabels : IWorldLabelAutomation
    {
        public List<PluginWorldLabel[]> Pushes { get; } = [];

        public IReadOnlyList<PluginWorldLabel> Current =>
            Pushes.Count == 0 ? [] : Pushes[^1];

        public bool ShowLabels(IReadOnlyList<PluginWorldLabel> labels)
        {
            Pushes.Add([.. labels]);
            return true;
        }
    }

    private sealed class Rig
    {
        public FakeObjects Objects { get; } = new();
        public FakeLabels Labels { get; } = new();
        public NametagSettings Settings { get; set; } = NametagSettings.Default;
        public PluginNavigationPosition? SelfPosition { get; set; } = At(0d, 0d);
        public uint SelfMonarch { get; set; }
        public NametagController Controller { get; }

        public Rig()
        {
            Controller = new NametagController(
                Objects,
                Labels,
                () => SelfId,
                () => SelfPosition,
                () => SelfMonarch,
                () => Settings);
        }

        public void Create(PluginWorldObject world)
        {
            Objects.Objects[world.ObjectId] = world;
            Controller.OnObjectChanged(
                new PluginObjectChange(world.ObjectId, PluginObjectChangeKind.Created));
        }

        public void Appraise(uint objectId, PluginItemProperties properties)
        {
            Objects.Properties[objectId] = properties;
            Objects.Objects[objectId] = Objects.Objects[objectId] with { HasAppraisalData = true };
            Controller.OnObjectChanged(
                new PluginObjectChange(objectId, PluginObjectChangeKind.IdentReceived));
        }

        public void Release(uint objectId)
        {
            Objects.Objects.Remove(objectId);
            Controller.OnObjectChanged(
                new PluginObjectChange(objectId, PluginObjectChangeKind.Released));
        }

        public void Ticks(int count, double each = 0.1d)
        {
            for (int i = 0; i < count; i++)
                Controller.OnTick(each);
        }
    }

    /// <summary>
    /// Mutation: skip the push in PushIfChanged and no set ever reaches the
    /// host; drop the Released branch and the second push still names it.
    /// </summary>
    [Fact]
    public void ACreatedMonsterIsHandedToTheHostAndAReleasedOneIsTakenBack()
    {
        var rig = new Rig();
        rig.Create(World(0x80000001u, "Drudge Slinker", PluginObjectClass.Monster));
        rig.Ticks(1);

        PluginWorldLabel label = Assert.Single(rig.Labels.Current);
        Assert.Equal(0x80000001u, label.ObjectId);
        Assert.Equal("Drudge Slinker", label.Text);
        Assert.Equal(0, label.Line);
        Assert.Equal(35f, label.MaxRange);
        Assert.True(label.Outline);
        Assert.Equal(new Vector4(1f, 0f, 0f, 1f), label.Color);

        rig.Release(0x80000001u);
        rig.Ticks(1);
        Assert.Empty(rig.Labels.Current);
        Assert.Equal(2, rig.Labels.Pushes.Count);
    }

    /// <summary>
    /// Mutation: remove the SequenceEqual check and every frame pushes.
    /// </summary>
    [Fact]
    public void AFrameInWhichNothingChangedHandsNothingOver()
    {
        var rig = new Rig();
        rig.Create(World(0x80000001u, "Drudge Slinker", PluginObjectClass.Monster));
        rig.Ticks(1);
        Assert.Single(rig.Labels.Pushes);

        rig.Ticks(30);
        Assert.Single(rig.Labels.Pushes);
    }

    /// <summary>
    /// Mutation: drop the one-second gate in UpdateData and the rename shows
    /// on the very next frame.
    /// </summary>
    [Fact]
    public void ATagsTextIsRefreshedAtMostOnceASecond()
    {
        var rig = new Rig();
        rig.Create(World(0x80000001u, "Drudge Slinker", PluginObjectClass.Monster));
        rig.Ticks(1);
        Assert.Equal("Drudge Slinker", Assert.Single(rig.Labels.Current).Text);

        rig.Objects.Objects[0x80000001u] =
            World(0x80000001u, "Drudge Prowler", PluginObjectClass.Monster);
        rig.Ticks(5);
        Assert.Equal("Drudge Slinker", Assert.Single(rig.Labels.Current).Text);

        rig.Ticks(6);
        Assert.Equal("Drudge Prowler", Assert.Single(rig.Labels.Current).Text);
    }

    /// <summary>
    /// The backoff: the first request goes at once, a second one not inside
    /// five seconds, and after that only once the object is closer than it
    /// was when last asked. Mutation: drop the range comparison and the
    /// third request goes at the same range.
    /// </summary>
    [Fact]
    public void AnAppraisalIsAskedForOnlyWhenCloserThanLastTimeAndAtMostEveryFiveSeconds()
    {
        var rig = new Rig();
        rig.Create(World(0x50000002u, "Sigrid", PluginObjectClass.Player, 0.05d));
        rig.Ticks(1);
        Assert.Equal([0x50000002u], rig.Objects.Identified);

        rig.Ticks(40);
        Assert.Single(rig.Objects.Identified);

        rig.Ticks(20);
        Assert.Single(rig.Objects.Identified);

        rig.Objects.Objects[0x50000002u] =
            World(0x50000002u, "Sigrid", PluginObjectClass.Player, 0.02d);
        rig.Ticks(11);
        Assert.Equal(2, rig.Objects.Identified.Count);
    }

    /// <summary>
    /// The five-second half of the backoff on its own: an object that comes
    /// closer twice inside five seconds of the first request is asked about
    /// once, and again only after the five seconds have passed. Mutation:
    /// drop the five-second clause and each closer step sends a request.
    /// </summary>
    [Fact]
    public void ComingCloserTwiceInsideFiveSecondsAsksOnce()
    {
        var rig = new Rig();
        rig.Create(World(0x50000002u, "Sigrid", PluginObjectClass.Player, 0.05d));
        rig.Ticks(1);
        Assert.Equal([0x50000002u], rig.Objects.Identified);

        rig.Objects.Objects[0x50000002u] =
            World(0x50000002u, "Sigrid", PluginObjectClass.Player, 0.04d);
        rig.Ticks(10);
        Assert.Single(rig.Objects.Identified);

        rig.Objects.Objects[0x50000002u] =
            World(0x50000002u, "Sigrid", PluginObjectClass.Player, 0.03d);
        rig.Ticks(10);
        Assert.Single(rig.Objects.Identified);

        // Past the five seconds, still closer than the first request: asked again.
        rig.Ticks(40);
        Assert.Equal(2, rig.Objects.Identified.Count);
    }

    /// <summary>
    /// Mutation: drop the attempt limit and the count keeps climbing.
    /// </summary>
    [Fact]
    public void AnObjectThatNeverAnswersIsGivenUpOnAfterTheTryLimit()
    {
        var rig = new Rig();
        double eastWest = 0.1d;
        rig.Create(World(0x50000002u, "Sigrid", PluginObjectClass.Player, eastWest));
        for (int attempt = 0; attempt < 30; attempt++)
        {
            eastWest -= 0.002d;
            rig.Objects.Objects[0x50000002u] =
                World(0x50000002u, "Sigrid", PluginObjectClass.Player, eastWest);
            rig.Ticks(60);
        }
        Assert.Equal(NametagController.AssessAttemptLimit + 1, rig.Objects.Identified.Count);
    }

    /// <summary>
    /// Mutation: ignore the level property and the brackets never appear;
    /// ignore the allegiance name and the second line is empty.
    /// </summary>
    [Fact]
    public void APlayersAppraisalAddsTheLevelAndHangsTheAllegianceUnderneath()
    {
        var rig = new Rig();
        rig.Create(World(0x50000002u, "Sigrid", PluginObjectClass.Player, 0.05d));
        rig.Ticks(1);
        Assert.Equal("Sigrid", Assert.Single(rig.Labels.Current).Text);

        rig.Appraise(0x50000002u, Properties(
            ints: new() { [LevelProperty] = 126 },
            strings: new() { [AllegianceNameProperty] = "The Iron Hand" },
            instanceIds: new() { [MonarchProperty] = 0x50000009u }));
        rig.Ticks(1);

        Assert.Equal(2, rig.Labels.Current.Count);
        PluginWorldLabel tag = rig.Labels.Current[0];
        PluginWorldLabel ticker = rig.Labels.Current[1];
        Assert.Equal("Sigrid [126]", tag.Text);
        Assert.Equal(1, tag.Line);
        Assert.Equal("<The Iron Hand>", ticker.Text);
        Assert.Equal(0, ticker.Line);
    }

    /// <summary>
    /// Mutation: drop the monarch comparison and a vassal of one's own
    /// monarch keeps the plain player colour.
    /// </summary>
    [Fact]
    public void APlayerUnderTheSameMonarchTakesTheAllegianceColour()
    {
        var rig = new Rig { SelfMonarch = 0x50000009u };
        rig.Create(World(0x50000002u, "Sigrid", PluginObjectClass.Player, 0.05d));
        rig.Ticks(1);
        Assert.Equal(new Vector4(0f, 1f, 1f, 1f), Assert.Single(rig.Labels.Current).Color);

        rig.Appraise(0x50000002u, Properties(
            ints: new() { [LevelProperty] = 50 },
            instanceIds: new() { [MonarchProperty] = 0x50000009u }));
        rig.Ticks(1);
        Assert.Equal(new Vector4(0f, 1f, 0f, 1f), rig.Labels.Current[0].Color);
    }

    /// <summary>
    /// Mutation: read the wrong string property and the destination is empty.
    /// </summary>
    [Fact]
    public void APortalHangsItsDestinationUnderneathOnceAppraised()
    {
        var rig = new Rig();
        rig.Create(World(0x70000001u, "Portal to Holtburg", PluginObjectClass.Portal, 0.05d));
        rig.Ticks(1);
        Assert.Equal([0x70000001u], rig.Objects.Identified);

        rig.Appraise(0x70000001u, Properties(
            strings: new() { [PortalDestinationProperty] = "Holtburg" }));
        rig.Ticks(1);
        Assert.Equal(2, rig.Labels.Current.Count);
        Assert.Equal("<Holtburg>", rig.Labels.Current[1].Text);
        Assert.Equal(new Vector4(0f, 1f, 0f, 1f), rig.Labels.Current[1].Color);
    }

    /// <summary>
    /// Mutation: ignore the owner property and a pet stays a monster in red.
    /// </summary>
    [Fact]
    public void AMonsterWithAnOwnerTakesThePetColour()
    {
        var rig = new Rig();
        rig.Objects.Properties[0x80000001u] = Properties(
            instanceIds: new() { [PetOwnerProperty] = 0x50000002u });
        rig.Create(World(0x80000001u, "Angel of Death", PluginObjectClass.Monster));
        rig.Ticks(1);
        Assert.Equal(new Vector4(0f, 1f, 1f, 1f), Assert.Single(rig.Labels.Current).Color);
        Assert.Empty(rig.Objects.Identified);
    }

    /// <summary>
    /// Mutation: stop comparing the settings snapshot and a colour edit on
    /// the page never reaches the host.
    /// </summary>
    [Fact]
    public void AChangeOnThePageIsPickedUpWithinAQuarterSecondAndReHanded()
    {
        var rig = new Rig();
        rig.Create(World(0x80000001u, "Drudge Slinker", PluginObjectClass.Monster));
        rig.Ticks(1);
        Assert.Single(rig.Labels.Pushes);

        rig.Settings = rig.Settings with
        {
            Monster = rig.Settings.Monster with { TagColor = 0xFF0000FFu },
        };
        rig.Ticks(3);
        Assert.Equal(2, rig.Labels.Pushes.Count);
        Assert.Equal(new Vector4(0f, 0f, 1f, 1f), Assert.Single(rig.Labels.Current).Color);
    }

    /// <summary>
    /// Mutation: skip the group switch in AddTag and the vendor is tagged;
    /// skip the master switch and the set is never cleared.
    /// </summary>
    [Fact]
    public void ADisabledGroupIsNotTaggedAndTheMasterSwitchClearsEverything()
    {
        var rig = new Rig();
        rig.Settings = rig.Settings with { Vendor = rig.Settings.Vendor with { Enabled = false } };
        rig.Create(World(0x60000001u, "Aun Tanua", PluginObjectClass.Vendor));
        rig.Create(World(0x80000001u, "Drudge Slinker", PluginObjectClass.Monster));
        rig.Ticks(1);
        Assert.Equal(0x80000001u, Assert.Single(rig.Labels.Current).ObjectId);

        rig.Settings = rig.Settings with { Enabled = false };
        rig.Ticks(3);
        Assert.Empty(rig.Labels.Current);

        rig.Settings = rig.Settings with { Enabled = true };
        rig.Ticks(3);
        Assert.Equal(0x80000001u, Assert.Single(rig.Labels.Current).ObjectId);
    }

    /// <summary>
    /// Mutation: drop the range gate and the far monster is handed over.
    /// </summary>
    [Fact]
    public void AnObjectPastTheMaximumRangeIsLeftOutOfTheSet()
    {
        var rig = new Rig();
        rig.Create(World(0x80000001u, "Near", PluginObjectClass.Monster, 0.05d));
        rig.Create(World(0x80000002u, "Far", PluginObjectClass.Monster, 1d));
        rig.Ticks(1);
        Assert.Equal("Near", Assert.Single(rig.Labels.Current).Text);
    }

    /// <summary>
    /// The host refuses a set past its cap whole, so the set is trimmed to
    /// the nearest objects. Mutation: drop the trim and the set is refused.
    /// </summary>
    [Fact]
    public void ASetPastTheHostsCapIsTrimmedToTheNearestObjects()
    {
        var rig = new Rig();
        for (uint i = 1; i <= 300; i++)
            rig.Create(World(0x80000000u + i, $"M{i}", PluginObjectClass.Monster, 0.0001d * i));
        rig.Ticks(1);
        Assert.Equal(IWorldLabelAutomation.MaximumLabels, rig.Labels.Current.Count);
        Assert.Equal("M1", rig.Labels.Current[0].Text);
        Assert.DoesNotContain(rig.Labels.Current, static label => label.Text == "M300");
    }

    /// <summary>
    /// An object gone from the table by the time its tag is refreshed is
    /// queued and removed after the walk, so the walk never edits the
    /// table it is reading. Mutation: remove inline and the walk throws.
    /// </summary>
    [Fact]
    public void AnObjectGoneFromTheTableIsDroppedAfterTheWalkNotDuringIt()
    {
        var rig = new Rig();
        rig.Create(World(0x80000001u, "One", PluginObjectClass.Monster));
        rig.Create(World(0x80000002u, "Two", PluginObjectClass.Monster));
        rig.Create(World(0x80000003u, "Three", PluginObjectClass.Monster));
        rig.Ticks(1);
        Assert.Equal(3, rig.Labels.Current.Count);

        rig.Objects.Objects.Remove(0x80000002u);
        rig.Ticks(11);
        Assert.Equal(2, rig.Labels.Current.Count);
        Assert.Equal([0x80000001u, 0x80000003u], rig.Controller.TaggedObjectIds.Order());
    }

    /// <summary>
    /// Objects that were in the world before the controller listened are
    /// found on the first frame. Mutation: drop the scan and the set is
    /// empty until something new arrives.
    /// </summary>
    [Fact]
    public void TheWorldIsScannedOnTheFirstFrameAndAfterASessionReset()
    {
        var rig = new Rig();
        rig.Objects.Objects[0x80000001u] = World(0x80000001u, "Already here", PluginObjectClass.Monster);
        rig.Objects.Objects[0x50000001u] = World(SelfId, "Me", PluginObjectClass.Player);
        rig.Ticks(1);
        Assert.Equal("Already here", Assert.Single(rig.Labels.Current).Text);

        rig.Controller.Reset();
        Assert.Empty(rig.Labels.Current);
        rig.Ticks(1);
        Assert.Equal("Already here", Assert.Single(rig.Labels.Current).Text);
    }
}

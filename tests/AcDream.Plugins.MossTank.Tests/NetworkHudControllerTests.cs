using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The clients readout: rows from what the other clients publish, filtered
/// by tag, rebuilt at most twenty times a second and repainted only when a
/// row changed; the arrow, tint and distance arithmetic; the tracked item
/// counts on this character's own row; selecting a row.
/// </summary>
public sealed class NetworkHudControllerTests
{
    private const uint SelfId = 0x50000001u;

    private static PluginNavigationPosition At(double eastWest, double northSouth, float heading = 0f) =>
        new(0xA9B40001u, eastWest, northSouth, 0d, heading, true);

    private static PluginNetworkClient Client(
        uint clientId,
        uint playerId,
        string name,
        PluginNavigationPosition position,
        uint health = 100u,
        uint maxHealth = 100u,
        uint stamina = 50u,
        uint mana = 25u,
        params string[] tags) =>
        new(clientId, playerId, name, "Coldeve", position, tags, health, mana, stamina, maxHealth, 100u, 100u, position.HeadingDegrees);

    private sealed class FakeNetwork : INetworkAutomation
    {
        public List<PluginNetworkClient> Clients { get; } = [];
        public int Captures { get; private set; }
        public bool Available { get; set; } = true;
        public bool IsAvailable => Available;

        public IReadOnlyList<PluginNetworkClient> CaptureClients()
        {
            Captures++;
            return [.. Clients];
        }
    }

    private sealed class FakeObjects : IWorldObjectAutomation
    {
        public List<PluginWorldObject> Objects { get; } = [];
        public bool IsAvailable => true;
        public IReadOnlyList<PluginWorldObject> CaptureObjects() => Objects;
    }

    private sealed class FakeSelection : ISelectionService
    {
        public List<uint> Selected { get; } = [];
        public uint? SelectedObjectId => Selected.Count == 0 ? null : Selected[^1];
        public uint? PreviousObjectId => null;

        public event Action<SelectionChangedEvent> Changed
        {
            add { }
            remove { }
        }

        public bool Select(uint objectId)
        {
            Selected.Add(objectId);
            return true;
        }

        public bool Clear() => true;
    }

    private sealed class Rig
    {
        public LandscapeMapControllerTests.FakeUi Ui { get; } = new();
        public FakeNetwork Network { get; } = new();
        public FakeObjects Objects { get; } = new();
        public FakeSelection Selection { get; } = new();
        public NetworkHudSettings Settings { get; set; } =
            NetworkHudSettings.Default with { Enabled = true };
        public PluginNavigationPosition? SelfPosition { get; set; } = At(0d, 0d);
        public NetworkHudSelf Self { get; set; } = new(SelfId, "Acdream", 100u, 100u, 50u, 100u, 25u, 100u);
        public NetworkHudController Controller { get; }

        public Rig()
        {
            Controller = new NetworkHudController(
                Ui, Network, Objects, Selection, () => Self, () => SelfPosition, () => Settings);
        }

        public LandscapeMapControllerTests.FakeCanvas Canvas => Assert.Single(Ui.Canvases);

        public LandscapeMapControllerTests.RecordingPainter Paint()
        {
            var painter = new LandscapeMapControllerTests.RecordingPainter(
                NetworkHudController.HudWidth, NetworkHudController.CanvasHeight);
            Canvas.PaintCallback(painter);
            Assert.Equal(0, painter.ClipDepth);
            return painter;
        }

        public void Ticks(int count, double each = 0.015d)
        {
            for (int i = 0; i < count; i++)
                Controller.OnTick(each);
        }
    }

    /// <summary>
    /// Mutation: drop the tag test and the untagged healer is listed under
    /// the tanks tag.
    /// </summary>
    [Fact]
    public void RowsAreTheClientsCarryingTheSelectedTagAndAllListsEveryOne()
    {
        var rig = new Rig();
        rig.Network.Clients.Add(Client(1u, 0x50000002u, "Sigrid", At(0.01d, 0d), tags: "tanks"));
        rig.Network.Clients.Add(Client(2u, 0x50000003u, "Ulf", At(0.02d, 0d), tags: "healers"));
        rig.Ticks(1);
        Assert.Equal(["Acdream", "Sigrid", "Ulf"], rig.Controller.Rows.Select(static row => row.Name));

        rig.Settings = rig.Settings with { SelectedTag = "tanks" };
        rig.Ticks(20);
        Assert.Equal(["Acdream", "Sigrid"], rig.Controller.Rows.Select(static row => row.Name));
    }

    /// <summary>
    /// The tag match ignores case, as the cast-sharing tag filter does, so
    /// a client tagged "Tanks" is listed under a selected "tanks". Mutation:
    /// an ordinal match lists nothing.
    /// </summary>
    [Fact]
    public void TheSelectedTagMatchesRegardlessOfCase()
    {
        var rig = new Rig();
        rig.Network.Clients.Add(Client(1u, 0x50000002u, "Sigrid", At(0.01d, 0d), tags: "Tanks"));
        rig.Settings = rig.Settings with { SelectedTag = "tanks" };
        rig.Ticks(20);
        Assert.Equal(["Acdream", "Sigrid"], rig.Controller.Rows.Select(static row => row.Name));
    }

    /// <summary>
    /// A still readout is never repainted; a vital that moved is, and two
    /// moves inside one twentieth of a second cost one repaint. Mutation:
    /// drop the row comparison and every rebuild repaints; drop the
    /// interval and every frame rebuilds.
    /// </summary>
    [Fact]
    public void TheReadoutRepaintsOnlyWhenARowChangedAndAtMostTwentyTimesASecond()
    {
        var rig = new Rig();
        rig.Network.Clients.Add(Client(1u, 0x50000002u, "Sigrid", At(0.01d, 0d)));
        rig.Ticks(1);
        Assert.Equal(1, rig.Canvas.Invalidations);
        int captures = rig.Network.Captures;

        // A second of 15 ms frames: a rebuild every fourth frame, never
        // more often than twenty a second, never so rarely as ten.
        rig.Ticks(66);
        Assert.Equal(1, rig.Canvas.Invalidations);
        Assert.InRange(rig.Network.Captures - captures, 11, 20);

        rig.Network.Clients[0] = Client(1u, 0x50000002u, "Sigrid", At(0.01d, 0d), health: 90u);
        rig.Ticks(1);
        rig.Network.Clients[0] = Client(1u, 0x50000002u, "Sigrid", At(0.01d, 0d), health: 80u);
        rig.Ticks(1);
        Assert.Equal(2, rig.Canvas.Invalidations);
        Assert.Equal("80%", rig.Controller.Rows[1].HealthText);

        rig.Ticks(4);
        Assert.Equal(2, rig.Canvas.Invalidations);
    }

    /// <summary>
    /// The arrow points at the other character relative to the way this
    /// one faces. Mutation: forget to subtract the facing and a character
    /// due east reads as ahead when facing east.
    /// </summary>
    [Theory]
    [InlineData(0f, 1d, 0d, 90d)]
    [InlineData(90f, 1d, 0d, 0d)]
    [InlineData(0f, 0d, 1d, 0d)]
    [InlineData(0f, 0d, -1d, 180d)]
    [InlineData(180f, 0d, -1d, 0d)]
    [InlineData(0f, -1d, 0d, 270d)]
    public void TheArrowIsTheBearingLessTheFacing(float facing, double dEast, double dNorth, double degrees)
    {
        double radians = NetworkHudController.ArrowRadians(At(0d, 0d, facing), At(dEast, dNorth));
        Assert.Equal(degrees, radians * 180d / Math.PI, 6);
    }

    /// <summary>Mutation: tint one per metre and a peer 100 m away is pink rather than red.</summary>
    [Theory]
    [InlineData(0d, 255)]
    [InlineData(50d, 155)]
    [InlineData(127.5d, 0)]
    [InlineData(500d, 0)]
    public void TheTintLosesTwoPerMetreAndStopsAtRed(double meters, int tint) =>
        Assert.Equal((byte)tint, NetworkHudController.DistanceTint(meters));

    [Theory]
    [InlineData(0d, "0")]
    [InlineData(999.4d, "999")]
    [InlineData(1500d, "1.5k")]
    [InlineData(12345d, "12k")]
    public void NumbersAreWholeUpToAThousandAndThousandsBeyond(double number, string text) =>
        Assert.Equal(text, NetworkHudController.FormatNumber(number));

    /// <summary>
    /// A row carries the fractions and percentages of its vitals and the
    /// distance in metres; this character's own row comes first and carries
    /// no distance, and a client the network reports under this character's
    /// id is that same row, not a second one. Mutation: divide by the
    /// current instead of the maximum and the fraction is always one.
    /// </summary>
    [Fact]
    public void ARowCarriesTheVitalsAndTheDistanceAndTheOwnRowNoDistance()
    {
        var rig = new Rig { SelfPosition = At(0d, 0d, 90f) };
        // One map unit east is 240 m.
        rig.Network.Clients.Add(Client(1u, 0x50000002u, "Sigrid", At(1d, 0d), health: 40u, maxHealth: 80u, stamina: 50u, mana: 25u));
        rig.Network.Clients.Add(Client(2u, SelfId, "Me", At(0d, 0d)));
        rig.Ticks(1);

        Assert.Equal(2, rig.Controller.Rows.Count);
        NetworkHudRow sigrid = rig.Controller.Rows[1];
        Assert.Equal(0.5d, sigrid.HealthFraction);
        Assert.Equal("50%", sigrid.HealthText);
        Assert.Equal(0.5d, sigrid.StaminaFraction);
        Assert.Equal("50%", sigrid.StaminaText);
        Assert.Equal(0.25d, sigrid.ManaFraction);
        Assert.Equal("25%", sigrid.ManaText);
        Assert.Equal("240", sigrid.DistanceText);
        Assert.Equal(0d, sigrid.ArrowRadians, 6);
        Assert.Equal((byte)0, sigrid.DistanceTint);
        Assert.False(sigrid.IsSelf);

        NetworkHudRow me = rig.Controller.Rows[0];
        Assert.True(me.IsSelf);
        Assert.Equal("Acdream", me.Name);
        Assert.Equal(string.Empty, me.DistanceText);
    }

    /// <summary>
    /// This character's own row counts the tracked items in its own
    /// inventory, summing stacks, and asks for the icon of the first one
    /// it holds. Mutation: count objects instead of stacks and forty
    /// arrows read as one.
    /// </summary>
    [Fact]
    public void TheOwnRowCountsTrackedItemsFromTheInventoryWithTheirIcons()
    {
        var rig = new Rig();
        rig.Settings = rig.Settings with { TrackedItems = "Prismatic Taper\n\nHealing Kit\n" };
        rig.Objects.Objects.Add(new PluginWorldObject(0x80000010u, 1u, "Prismatic Taper", PluginObjectClass.SpellComponent, 0u, 0u, 0u)
        {
            IsOwned = true,
            StackSize = 40,
        });
        rig.Objects.Objects.Add(new PluginWorldObject(0x80000011u, 1u, "Prismatic Taper", PluginObjectClass.SpellComponent, 0u, 0u, 0u)
        {
            IsOwned = true,
            StackSize = 12,
        });
        rig.Objects.Objects.Add(new PluginWorldObject(0x80000012u, 1u, "Prismatic Taper", PluginObjectClass.SpellComponent, 0u, 0u, 0u)
        {
            IsOwned = false,
            StackSize = 99,
        });
        rig.Ui.ImageStore.ObjectIcons[0x80000010u] = new PluginImage(7, 32, 32);
        rig.Network.Clients.Add(Client(1u, 0x50000002u, "Sigrid", At(0.01d, 0d)));
        rig.Ticks(1);

        NetworkHudRow me = rig.Controller.Rows[0];
        Assert.True(me.IsSelf);
        Assert.Equal(2, me.Tracked.Count);
        Assert.Equal("52", me.Tracked[0].Count);
        Assert.Equal(7, me.Tracked[0].Icon.Handle);
        Assert.Equal("0", me.Tracked[1].Count);
        Assert.False(me.Tracked[1].Icon.IsValid);
        Assert.Empty(rig.Controller.Rows[1].Tracked);
    }

    /// <summary>Mutation: select the client id instead of the player id.</summary>
    [Fact]
    public void SelectingARowSelectsThatCharacter()
    {
        var rig = new Rig();
        rig.Network.Clients.Add(Client(1u, 0x50000002u, "Sigrid", At(0.01d, 0d)));
        rig.Ticks(1);

        Assert.Equal("Sigrid", rig.Controller.SelectRow(1));
        Assert.Equal([0x50000002u], rig.Selection.Selected);
        Assert.Equal("Acdream", rig.Controller.SelectRow(0));
        Assert.Equal([0x50000002u, SelfId], rig.Selection.Selected);
        Assert.Null(rig.Controller.SelectRow(2));
        Assert.Equal(2, rig.Selection.Selected.Count);
    }

    /// <summary>
    /// The canvas opts into pointer input, and a shift-click on a row
    /// selects that row's character, the row being the one whose twenty
    /// pixels the press lands in: y of 25 is the second row, the first
    /// other character under this one. A plain click
    /// selects nothing, as the reference's does not; a shift-click below
    /// the last row selects nothing; none of them repaints, since nothing
    /// drawn changed. Mutation: leaving the descriptor click-through,
    /// dividing by the wrong height, or selecting without shift, fails.
    /// </summary>
    [Fact]
    public void AShiftClickOnARowSelectsThatCharacter()
    {
        var rig = new Rig();
        rig.Network.Clients.Add(Client(1u, 0x50000002u, "Sigrid", At(0.01d, 0d)));
        rig.Network.Clients.Add(Client(2u, 0x50000003u, "Ulf", At(0.02d, 0d)));
        rig.Ticks(1);
        LandscapeMapControllerTests.FakeCanvas canvas = rig.Canvas;
        Assert.True(canvas.Descriptor.AcceptsPointerInput);
        Assert.Equal(1, canvas.Invalidations);

        canvas.Down(50d, 25d, PluginKeyModifiers.Shift);
        canvas.Up(50d, 25d, PluginKeyModifiers.Shift);
        Assert.Equal([0x50000002u], rig.Selection.Selected);

        canvas.Down(50d, 5d);
        canvas.Up(50d, 5d);
        Assert.Single(rig.Selection.Selected);

        canvas.Down(50d, NetworkHudController.RowHeight * 3d + 1d, PluginKeyModifiers.Shift);
        canvas.Up(50d, NetworkHudController.RowHeight * 3d + 1d, PluginKeyModifiers.Shift);
        Assert.Single(rig.Selection.Selected);

        canvas.Down(50d, 19.9d, PluginKeyModifiers.Shift);
        Assert.Equal([0x50000002u, SelfId], rig.Selection.Selected);

        rig.Ticks(1);
        Assert.Equal(1, canvas.Invalidations);
    }

    /// <summary>
    /// The canvas shows while the readout is on and either its window is
    /// open or it may stay with the window shut. Mutation: ignore
    /// ShowHudWhenClosed and closing the window always hides it.
    /// </summary>
    [Fact]
    public void TheCanvasShowsWhileOnAndEitherOpenOrAllowedToStay()
    {
        var rig = new Rig { Settings = NetworkHudSettings.Default };
        rig.Ticks(1);
        Assert.Empty(rig.Ui.Canvases);

        rig.Settings = rig.Settings with { Enabled = true, ShowHudWhenClosed = true };
        rig.Ticks(20);
        Assert.True(rig.Canvas.IsVisible);

        rig.Settings = rig.Settings with { ShowHudWhenClosed = false };
        rig.Ticks(20);
        Assert.False(rig.Canvas.IsVisible);

        rig.Controller.Shown = true;
        rig.Ticks(1);
        Assert.True(rig.Canvas.IsVisible);

        rig.Settings = rig.Settings with { Enabled = false };
        rig.Ticks(20);
        Assert.False(rig.Canvas.IsVisible);
    }

    /// <summary>
    /// Each row paints its bars, its name and percentages, and for another
    /// character the arrow and distance; the own row has no arrow and no
    /// client number, and it is the one row that carries the tracked
    /// counts. A character not yet in the world has no row of its own.
    /// Mutation: paint the arrow on the own row too.
    /// </summary>
    [Fact]
    public void ThePaintDrawsBarsTextArrowsAndTrackedCounts()
    {
        var rig = new Rig();
        rig.Settings = rig.Settings with { TrackedItems = "Prismatic Taper" };
        rig.Objects.Objects.Add(new PluginWorldObject(0x80000010u, 1u, "Prismatic Taper", PluginObjectClass.SpellComponent, 0u, 0u, 0u)
        {
            IsOwned = true,
            StackSize = 40,
        });
        rig.Ui.ImageStore.ObjectIcons[0x80000010u] = new PluginImage(7, 32, 32);
        rig.Network.Clients.Add(Client(1u, 0x50000002u, "Sigrid", At(0.01d, 0d)));
        rig.Ticks(1);
        LandscapeMapControllerTests.RecordingPainter both = rig.Paint();
        Assert.Equal(12, both.Ops.Count(static op => op == "fill"));
        Assert.Equal(3, both.Ops.Count(static op => op.StartsWith("line", StringComparison.Ordinal)));
        Assert.Contains("Acdream", both.Texts);
        Assert.Contains("1 Sigrid", both.Texts);
        Assert.Contains("100%", both.Texts);
        Assert.Contains("50%", both.Texts);
        Assert.Contains("25%", both.Texts);
        Assert.Contains("2", both.Texts);
        Assert.Contains("40", both.Texts);
        Assert.Equal(7, Assert.Single(both.Images).Handle);

        rig.Self = rig.Self with { ObjectId = 0u, Name = string.Empty };
        rig.Ticks(4);
        LandscapeMapControllerTests.RecordingPainter peerOnly = rig.Paint();
        Assert.Equal(6, peerOnly.Ops.Count(static op => op == "fill"));
        Assert.Equal(3, peerOnly.Ops.Count(static op => op.StartsWith("line", StringComparison.Ordinal)));
        Assert.DoesNotContain("Acdream", peerOnly.Texts);
        Assert.DoesNotContain("40", peerOnly.Texts);
        Assert.Empty(peerOnly.Images);
    }

    /// <summary>
    /// Alone, the readout is this character: one row, the character's own
    /// name without a client number, its own vitals, and the tracked counts
    /// from its own inventory, whether or not the network is up. Mutation:
    /// build the rows from the network alone and the readout is empty.
    /// </summary>
    [Fact]
    public void AloneTheReadoutIsOneRowWithTheLocalName()
    {
        var rig = new Rig();
        rig.Settings = rig.Settings with { TrackedItems = "Prismatic Taper" };
        rig.Objects.Objects.Add(new PluginWorldObject(0x80000010u, 1u, "Prismatic Taper", PluginObjectClass.SpellComponent, 0u, 0u, 0u)
        {
            IsOwned = true,
            StackSize = 40,
        });
        rig.Network.Available = false;
        rig.Ticks(1);

        NetworkHudRow me = Assert.Single(rig.Controller.Rows);
        Assert.Equal("Acdream", me.Name);
        Assert.True(me.IsSelf);
        Assert.Equal(SelfId, me.PlayerId);
        Assert.Equal(1d, me.HealthFraction);
        Assert.Equal("50%", me.StaminaText);
        Assert.Equal("25%", me.ManaText);
        Assert.Equal(string.Empty, me.DistanceText);
        Assert.Equal("40", Assert.Single(me.Tracked).Count);

        LandscapeMapControllerTests.RecordingPainter painter = rig.Paint();
        Assert.Contains("Acdream", painter.Texts);
        Assert.DoesNotContain(painter.Texts, static text => text.EndsWith(" Acdream", StringComparison.Ordinal));
    }
}

using System.Numerics;
using AcDream.Plugin.Abstractions;
using static AcDream.Plugins.MossTank.Tests.LandscapeMapControllerTests;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The dungeon map on a canvas: the camera that turns the picture with the
/// heading, the controller that registers the canvas only inside a sealed
/// dungeon and asks for a repaint only when something drawn changed, and
/// the paint itself.
/// </summary>
public sealed class DungeonMapControllerTests
{
    private const uint Landblock = 0x01A90000u;
    private const uint SelfId = 0x50000001u;

    // ---- The camera ------------------------------------------------------

    /// <summary>
    /// Following, the camera sits on the player and turns so that what is
    /// ahead is up: facing north, a point to the north is above the middle
    /// and one to the east is to the right; facing east, the point to the
    /// east is above. Mutation: turning the other way puts east below when
    /// facing east.
    /// </summary>
    [Fact]
    public void FollowingCentresOnThePlayerAndTurnsWhatIsAheadToTheTop()
    {
        DungeonMapPicture picture = DungeonMapPicture.Build(DungeonMapPictureTests.TwoRooms());
        var camera = new DungeonMapCamera { Scale = 1f };

        camera.Follow(new Vector2(5f, 5f), 0f);
        Vector2 north = camera.ScreenFromLocal(picture, new Vector3(5f, 15f, 0f), 200d, 200d);
        Vector2 east = camera.ScreenFromLocal(picture, new Vector3(15f, 5f, 0f), 200d, 200d);
        Assert.Equal(100f, north.X, 3);
        Assert.Equal(50f, north.Y, 3);
        Assert.Equal(150f, east.X, 3);
        Assert.Equal(100f, east.Y, 3);

        camera.Follow(new Vector2(5f, 5f), 90f);
        east = camera.ScreenFromLocal(picture, new Vector3(15f, 5f, 0f), 200d, 200d);
        north = camera.ScreenFromLocal(picture, new Vector3(5f, 15f, 0f), 200d, 200d);
        Assert.Equal(100f, east.X, 3);
        Assert.Equal(50f, east.Y, 3);
        Assert.Equal(50f, north.X, 3);
        Assert.Equal(100f, north.Y, 3);
    }

    /// <summary>
    /// The zoom runs from 0.3 to 5 in steps of 0.2, and the settings row
    /// keeps it as five minus the scale, so the shipped 4.2 is a scale of 0.8.
    /// </summary>
    [Fact]
    public void TheScaleIsClampedAndTheSettingIsFiveMinusIt()
    {
        var camera = new DungeonMapCamera { Scale = 10f };
        Assert.Equal(5f, camera.Scale);
        camera.Scale = 0.1f;
        Assert.Equal(0.3f, camera.Scale);
        camera.Scale = 4.9f;
        camera.ZoomIn();
        Assert.Equal(5f, camera.Scale);
        camera.ZoomOut();
        Assert.Equal(4.8f, camera.Scale, 3);
        Assert.Equal(0.8f, DungeonMapCamera.ScaleFromSetting(4.2f), 3);
        Assert.Equal(4.2f, DungeonMapCamera.SettingFromScale(0.8f), 3);
        Assert.Equal(0.3f, DungeonMapCamera.ScaleFromSetting(9f), 3);

        // The shipped zoom is four screen pixels to the metre, as the
        // reference's 0.8 over its five-pixel tiles: a ten-metre cell is
        // forty pixels wide. Mutation: reading the row as the scale itself
        // makes it two hundred and ten.
        var shipped = new DungeonMapCamera();
        Vector2 west = shipped.ScreenFromMap(new Vector2(0f, 0f), new Vector2(25f, 25f), 320d, 280d);
        Vector2 east = shipped.ScreenFromMap(new Vector2(50f, 0f), new Vector2(25f, 25f), 320d, 280d);
        Assert.Equal(40f, east.X - west.X, 3);
    }

    /// <summary>
    /// A drag moves the map with the hand, so the middle of the canvas
    /// moves the other way in metres, through the rotation: facing north,
    /// dragging right shows what lies west; facing east, the same drag
    /// shows what lies north. Panning stops following; resuming goes back
    /// to the player. Mutation: forgetting the rotation pans west facing east.
    /// </summary>
    [Fact]
    public void PanningMovesTheMiddleAgainstTheDragThroughTheRotationAndStopsFollowing()
    {
        var camera = new DungeonMapCamera { Scale = 1f };
        camera.Follow(new Vector2(5f, 5f), 0f);

        camera.PanBy(50d, 0d);
        Assert.False(camera.Following);
        Assert.Equal(-5f, camera.CenterMeters.X, 3);
        Assert.Equal(5f, camera.CenterMeters.Y, 3);
        camera.PanBy(0d, 50d);
        Assert.Equal(15f, camera.CenterMeters.Y, 3);

        camera.ResumeFollowing();
        camera.Follow(new Vector2(5f, 5f), 90f);
        Assert.Equal(new Vector2(5f, 5f), camera.CenterMeters);
        camera.PanBy(50d, 0d);
        Assert.Equal(5f, camera.CenterMeters.X, 3);
        Assert.Equal(15f, camera.CenterMeters.Y, 3);
    }

    /// <summary>
    /// Turning by drag is a degree per pixel of leftward and upward drag,
    /// as the reference does it; the heading no longer drives the turn
    /// until following resumes.
    /// </summary>
    [Fact]
    public void RotatingByDragIsADegreePerPixelAndStopsFollowing()
    {
        var camera = new DungeonMapCamera();
        camera.Follow(Vector2.Zero, 30f);
        camera.RotateByDrag(-10d, -5d);
        Assert.False(camera.Following);
        Assert.Equal(45d, camera.RotationDegrees, 3);
        camera.Follow(Vector2.Zero, 90f);
        Assert.Equal(45d, camera.RotationDegrees, 3);
        camera.RotateBy(-50d);
        Assert.Equal(355d, camera.RotationDegrees, 3);
    }

    // ---- The controller --------------------------------------------------

    private sealed class FakeDungeon : IDungeonMapAutomation
    {
        public bool Sealed { get; set; } = true;
        public PluginDungeonFloorplan Plan { get; set; } = DungeonMapPictureTests.TwoRooms();
        public int Captures { get; private set; }
        public bool IsSealedDungeon(uint cellId) => Sealed;
        public uint CurrentLandblockId => Plan.LandblockId;

        public PluginDungeonFloorplan CaptureFloorplan(uint landblockId)
        {
            Captures++;
            return (landblockId & 0xFFFF0000u) == Plan.LandblockId ? Plan : PluginDungeonFloorplan.Empty;
        }
    }

    /// <summary>Records every line with its ends, colour and thickness, and every text with its place.</summary>
    private sealed class Painter(int width, int height) : IPluginPainter
    {
        public List<(PluginPoint From, PluginPoint To, PluginColor Color, float Thickness)> Lines { get; } = [];
        public List<(string Text, PluginPoint At, PluginColor Color)> Texts { get; } = [];
        public List<(PluginImage Image, PluginRect At)> Images { get; } = [];
        public int ClipDepth { get; private set; }
        public PluginColor Cleared { get; private set; }
        public int Width => width;
        public int Height => height;
        public void Clear(PluginColor color) => Cleared = color;
        public void FillRect(PluginRect rect, PluginColor color) { }
        public void StrokeRect(PluginRect rect, PluginColor color, float thickness) { }
        public void DrawLine(PluginPoint from, PluginPoint to, PluginColor color, float thickness) =>
            Lines.Add((from, to, color, thickness));
        public void DrawText(string text, PluginPoint position, PluginColor color, bool outline) =>
            Texts.Add((text, position, color));
        public PluginSize MeasureText(string text) => new(text.Length * 6d, 12d);
        public void DrawImage(PluginImage image, PluginRect destination, PluginColor tint) =>
            Images.Add((image, destination));
        public void DrawImageTransformed(PluginImage image, PluginRect destination, PluginColor tint,
            double rotationRadians, PluginPoint pivot, double scaleX, double scaleY) =>
            Images.Add((image, destination));
        public void PushClip(PluginRect rect) => ClipDepth++;
        public void PopClip() => ClipDepth--;
    }

    private sealed class Rig
    {
        public FakeUi Ui { get; } = new();
        public DungeonObjectTrackerTests.MemoryStorage Storage { get; } = new();
        public FakeDungeon Dungeon { get; } = new();
        public DungeonMapSettings Settings { get; set; } = DungeonMapSettings.Default;
        public List<float> SavedZoom { get; } = [];
        public PluginNavigationSnapshot Snapshot { get; set; } = SnapshotAt(5f, 5f, 0f, 0f);
        public List<PluginWorldObject> Objects { get; } = [];
        public DungeonMapController Controller { get; }

        public Rig()
        {
            Controller = new DungeonMapController(
                Ui,
                Storage,
                () => Settings,
                zoom =>
                {
                    SavedZoom.Add(zoom);
                    Settings = Settings with { MapZoom = zoom };
                },
                () => Snapshot,
                Dungeon,
                () => Objects,
                () => (SelfId, "Acdream"));
        }

        public static PluginNavigationSnapshot SnapshotAt(float x, float y, float z, float heading) =>
            new(true, false, SelfId, DungeonObjectTrackerTests.At(Landblock, x, y, z, heading), false, false);

        public void Move(float x, float y, float z, float heading) => Snapshot = SnapshotAt(x, y, z, heading);

        /// <summary>One whole frame: past the world-read interval, so the world is read.</summary>
        public void Tick() => Controller.OnTick(DungeonMapController.WorldReadSeconds + 0.001d);

        public FakeCanvas Canvas => Assert.Single(Ui.Canvases);

        public Painter Paint()
        {
            var painter = new Painter(DungeonMapController.CanvasWidth, DungeonMapController.CanvasHeight);
            Canvas.PaintCallback(painter);
            Assert.Equal(0, painter.ClipDepth);
            return painter;
        }
    }

    private static PluginWorldObject Object(uint id, string name, PluginObjectClass objectClass, float x, float y, float z) =>
        new(id, 0u, name, objectClass, 0u, 0u, 0u)
        {
            HasPosition = true,
            Position = DungeonObjectTrackerTests.At(Landblock, x, y, z),
            IconId = 0x06001000u + id,
        };

    private static bool Near(PluginPoint point, double x, double y) =>
        Math.Abs(point.X - x) < 0.01d && Math.Abs(point.Y - y) < 0.01d;

    /// <summary>
    /// The canvas is registered once, the first time the character stands
    /// in a sealed dungeon with the row on, and is hidden rather than
    /// disposed when they leave; outside a dungeon nothing is registered
    /// at all. The window being shut does not hide it while the
    /// draw-when-closed row is on. Mutation: registering on the first tick
    /// regardless registers on the countryside.
    /// </summary>
    [Fact]
    public void TheCanvasRegistersInsideASealedDungeonAndHidesOutside()
    {
        var rig = new Rig();
        rig.Dungeon.Sealed = false;
        rig.Tick();
        Assert.Empty(rig.Ui.Canvases);

        rig.Dungeon.Sealed = true;
        rig.Tick();
        FakeCanvas canvas = rig.Canvas;
        Assert.Equal(DungeonMapController.CanvasId, canvas.CanvasId);
        Assert.True(canvas.IsVisible);
        Assert.Equal(1, rig.Dungeon.Captures);

        rig.Dungeon.Sealed = false;
        rig.Tick();
        Assert.False(rig.Canvas.IsVisible);
        Assert.False(rig.Canvas.Disposed);

        rig.Dungeon.Sealed = true;
        rig.Settings = rig.Settings with { DrawWhenClosed = false };
        rig.Controller.OnTick(DungeonMapController.SettingsPollSeconds + 1d);
        Assert.False(rig.Canvas.IsVisible);
        rig.Controller.Shown = true;
        rig.Tick();
        Assert.True(rig.Canvas.IsVisible);
        Assert.Equal(1, rig.Dungeon.Captures);
    }

    /// <summary>
    /// A repaint is asked for only when something drawn changed: a step
    /// too small to move the map by a pixel, or a frame in which nothing
    /// moved, asks for nothing; a real step, a turn, a zoom, a mover that
    /// went past the threshold, and a newly walked cell each ask once.
    /// Mutation: repainting on every frame while following asks on every tick.
    /// </summary>
    [Fact]
    public void ARepaintIsAskedOnlyWhenSomethingDrawnChanged()
    {
        var rig = new Rig();
        rig.Objects.Add(Object(0x80000001u, "Drudge", PluginObjectClass.Monster, 15f, 5f, 0f));
        rig.Tick();
        Assert.Equal(1, rig.Canvas.Invalidations);
        rig.Tick();
        rig.Tick();
        Assert.Equal(1, rig.Canvas.Invalidations);

        rig.Move(5.05f, 5f, 0f, 0f);
        rig.Tick();
        Assert.Equal(1, rig.Canvas.Invalidations);

        rig.Move(6f, 5f, 0f, 0f);
        rig.Tick();
        Assert.Equal(2, rig.Canvas.Invalidations);

        rig.Move(6f, 5f, 0f, 45f);
        rig.Tick();
        Assert.Equal(3, rig.Canvas.Invalidations);

        rig.Controller.ZoomIn();
        rig.Tick();
        Assert.Equal(4, rig.Canvas.Invalidations);
        Assert.Equal([DungeonMapCamera.SettingFromScale(1.0f)], rig.SavedZoom);

        rig.Objects[0] = Object(0x80000001u, "Drudge", PluginObjectClass.Monster, 15.05f, 5f, 0f);
        rig.Tick();
        Assert.Equal(4, rig.Canvas.Invalidations);
        rig.Objects[0] = Object(0x80000001u, "Drudge", PluginObjectClass.Monster, 16f, 5f, 0f);
        rig.Tick();
        Assert.Equal(5, rig.Canvas.Invalidations);

        rig.Snapshot = new PluginNavigationSnapshot(
            true, false, SelfId, DungeonObjectTrackerTests.At(Landblock, 6f, 5f, 0f, 45f) with { CellId = Landblock | 0x101u }, false, false);
        rig.Tick();
        Assert.Equal(6, rig.Canvas.Invalidations);
        rig.Tick();
        Assert.Equal(6, rig.Canvas.Invalidations);
    }

    // ---- The pointer -----------------------------------------------------

    /// <summary>
    /// The canvas opts into pointer input with a handler from the moment it
    /// is registered. A drag moves the map with the hand, by its delta, as
    /// the pan buttons do: forty pixels at the shipped scale of 0.8 is ten
    /// metres, and dragging right then up shows what lies west and south.
    /// The drag takes the map off the player and asks for one repaint.
    /// Mutation: leaving the descriptor click-through, panning by the
    /// position instead of the delta, or keeping follow on, each fails.
    /// </summary>
    [Fact]
    public void ADragPansTheMapByItsDeltaAndStopsFollowing()
    {
        var rig = new Rig();
        rig.Tick();
        FakeCanvas canvas = rig.Canvas;
        Assert.True(canvas.Descriptor.AcceptsPointerInput);
        Assert.True(rig.Controller.Camera.Following);

        canvas.Down(100d, 100d);
        canvas.Move(140d, 100d);
        canvas.Move(140d, 60d);
        canvas.Up(140d, 60d);
        Assert.False(rig.Controller.Camera.Following);
        Assert.Equal("Follow: off", rig.Controller.FollowText);
        Assert.Equal(-5f, rig.Controller.Camera.CenterMeters.X, 3);
        Assert.Equal(-5f, rig.Controller.Camera.CenterMeters.Y, 3);
        Assert.Equal(0d, rig.Controller.Camera.RotationDegrees, 3);

        rig.Tick();
        Assert.Equal(2, canvas.Invalidations);

        // The player walks on; the map stays where it was dragged.
        rig.Move(8f, 5f, 0f, 0f);
        rig.Tick();
        Assert.Equal(-5f, rig.Controller.Camera.CenterMeters.X, 3);
        Assert.Equal(-5f, rig.Controller.Camera.CenterMeters.Y, 3);
    }

    /// <summary>
    /// A shift-drag turns the map a degree per pixel dragged left and up
    /// instead of panning it, and likewise lets go of the player.
    /// Mutation: panning under shift moves the middle.
    /// </summary>
    [Fact]
    public void AShiftDragTurnsTheMapADegreePerPixel()
    {
        var rig = new Rig();
        rig.Move(5f, 5f, 0f, 30f);
        rig.Tick();

        rig.Canvas.Drag(100d, 100d, 90d, 95d, PluginKeyModifiers.Shift);
        Assert.False(rig.Controller.Camera.Following);
        Assert.Equal(45d, rig.Controller.Camera.RotationDegrees, 3);
        Assert.Equal(new Vector2(5f, 5f), rig.Controller.Camera.CenterMeters);
        rig.Tick();
        Assert.Equal(2, rig.Canvas.Invalidations);
    }

    /// <summary>
    /// The wheel zooms a step a notch about the cursor: the spot under the
    /// cursor before is the spot under it after, and the zoom is saved to
    /// the page's row. Following, only the scale moves, since the middle is
    /// the player's anyway. A notch past the limit changes nothing and asks
    /// for nothing. Mutation: zooming about the middle moves the spot under
    /// the cursor; saving on every notch saves at the limit.
    /// </summary>
    [Fact]
    public void TheWheelZoomsAboutTheCursorKeepingTheSpotUnderItFixed()
    {
        var rig = new Rig();
        rig.Tick();
        FakeCanvas canvas = rig.Canvas;
        DungeonMapCamera camera = rig.Controller.Camera;
        DungeonMapPicture picture = DungeonMapPicture.Build(rig.Dungeon.Plan);
        const double width = DungeonMapController.CanvasWidth;
        const double height = DungeonMapController.CanvasHeight;

        canvas.Drag(100d, 100d, 120d, 110d);
        rig.Tick();
        Assert.Equal(2, canvas.Invalidations);

        var spot = new Vector3(12f, 8f, 0f);
        Vector2 before = camera.ScreenFromLocal(picture, spot, width, height);
        canvas.Wheel(before.X, before.Y, 1);
        Assert.Equal(1.0f, camera.Scale, 3);
        Vector2 after = camera.ScreenFromLocal(picture, spot, width, height);
        Assert.Equal(before.X, after.X, 2);
        Assert.Equal(before.Y, after.Y, 2);
        Assert.Equal([DungeonMapCamera.SettingFromScale(1.0f)], rig.SavedZoom);
        rig.Tick();
        Assert.Equal(3, canvas.Invalidations);

        canvas.Wheel(before.X, before.Y, -2);
        Assert.Equal(0.6f, camera.Scale, 3);
        after = camera.ScreenFromLocal(picture, spot, width, height);
        Assert.Equal(before.X, after.X, 2);
        Assert.Equal(before.Y, after.Y, 2);

        rig.Controller.ToggleFollow();
        rig.Tick();
        canvas.Wheel(10d, 10d, 1);
        Assert.Equal(0.8f, camera.Scale, 3);
        Assert.True(camera.Following);
        rig.Tick();
        Assert.Equal(new Vector2(5f, 5f), camera.CenterMeters);

        canvas.Wheel(10d, 10d, 30);
        Assert.Equal(DungeonMapCamera.MaxScale, camera.Scale);
        rig.Tick();
        int invalidations = canvas.Invalidations;
        int saves = rig.SavedZoom.Count;
        canvas.Wheel(10d, 10d, 1);
        rig.Tick();
        Assert.Equal(invalidations, canvas.Invalidations);
        Assert.Equal(saves, rig.SavedZoom.Count);
    }

    /// <summary>
    /// A cancelled press ends the drag: a move after it pans nothing, and
    /// the next press starts its own drag from its own spot. Mutation:
    /// leaving the drag open on cancel pans from the cancelled spot.
    /// </summary>
    [Fact]
    public void ACancelledPressEndsTheDrag()
    {
        var rig = new Rig();
        rig.Tick();
        FakeCanvas canvas = rig.Canvas;

        canvas.Down(100d, 100d);
        canvas.Cancel(100d, 100d);
        canvas.Move(140d, 100d);
        Assert.True(rig.Controller.Camera.Following);
        Assert.Equal(new Vector2(5f, 5f), rig.Controller.Camera.CenterMeters);
        rig.Tick();
        Assert.Equal(1, canvas.Invalidations);

        canvas.Down(200d, 100d);
        canvas.Move(240d, 100d);
        Assert.Equal(-5f, rig.Controller.Camera.CenterMeters.X, 3);
        Assert.Equal(5f, rig.Controller.Camera.CenterMeters.Y, 3);
    }

    /// <summary>
    /// The floor is painted as rectangles turned with the camera, each one
    /// line of the rectangle's own thickness in the floor colour, and the
    /// walls as lines in the wall colour. Facing north the west room's
    /// run is horizontal, left of the middle; facing east it stands
    /// vertical below the middle, because west is behind. The floor is
    /// as solid as the opacity row says. Mutation: dropping the turn
    /// leaves the run horizontal facing east.
    /// </summary>
    [Fact]
    public void FloorsArePaintedAsTurnedRectanglesInTheFloorColourAndWallsAsLines()
    {
        var rig = new Rig();
        rig.Settings = rig.Settings with { MapZoom = 4f };
        rig.Tick();
        Painter painter = rig.Paint();

        double cx = DungeonMapController.CanvasWidth / 2d;
        double cy = DungeonMapController.CanvasHeight / 2d;
        var floor = Assert.Single(painter.Lines, line =>
            line.Thickness == 50f && line.Color.G == 127 && Near(line.From, cx - 25d, cy) && Near(line.To, cx + 25d, cy));
        Assert.Equal((0, 127, 191, 204), (floor.Color.R, floor.Color.G, floor.Color.B, floor.Color.A));
        var wall = Assert.Single(painter.Lines, line =>
            line.Thickness == 1f && Near(line.From, cx - 25d, cy + 25d) && Near(line.To, cx - 25d, cy - 25d));
        Assert.Equal((0, 0, 127), (wall.Color.R, wall.Color.G, wall.Color.B));

        rig.Move(5f, 5f, 0f, 90f);
        rig.Tick();
        painter = rig.Paint();
        Assert.Single(painter.Lines, line =>
            line.Thickness == 50f && line.Color.G == 127 && Near(line.From, cx, cy + 25d) && Near(line.To, cx, cy - 25d));
    }

    /// <summary>
    /// The storey above is washed grey and half transparent, the current
    /// one is plain, and all-layers paints both plain.
    /// </summary>
    [Fact]
    public void TheStoreyAboveIsWashedAndAllLayersPaintsItPlain()
    {
        var rig = new Rig();
        rig.Settings = rig.Settings with { MapZoom = 4f };
        rig.Tick();
        Painter painter = rig.Paint();

        var above = Assert.Single(painter.Lines, line => line.Thickness == 50f && line.Color.A < 200);
        Assert.Equal((0, 75, 113, 97), (above.Color.R, above.Color.G, above.Color.B, above.Color.A));
        Assert.Equal(2, painter.Lines.Count(line => line.Thickness == 50f && line.Color.A == 204 && line.Color.G == 127));

        rig.Controller.ShowAllLayers = true;
        rig.Tick();
        painter = rig.Paint();
        Assert.Equal(3, painter.Lines.Count(line => line.Thickness == 50f && line.Color.A == 204 && line.Color.G == 127));
    }

    /// <summary>
    /// The Town Network as the live run saw it: the character stands in a
    /// sunken four-cell entry six metres under the great hall, at
    /// (60.03, -160.03, -5.995), zoom 4.2, opacity 16, all-layers off.
    /// The entry is the current storey and paints solid, four screen
    /// pixels to the metre, so a ten-metre cell is a forty-pixel run in
    /// the floor colour at the opacity row's alpha; the hall one storey up
    /// paints as the grey wash. Labels are written only for the current
    /// storey, as the reference does, so the hall's portals do not cloud
    /// the entry; all-layers labels every storey. Mutation: labelling every
    /// visible storey writes the hall portal's name.
    /// </summary>
    [Fact]
    public void AtTheSunkenEntryOnlyTheCurrentStoreyIsSolidAndLabelled()
    {
        IReadOnlyList<Vector2> entry = [new(60, -165), new(70, -165), new(70, -155), new(60, -155)];
        IReadOnlyList<Vector2> hall = [new(50, -150), new(90, -150), new(90, -100), new(50, -100)];
        var plan = new PluginDungeonFloorplan(
            Landblock,
            [
                new PluginDungeonLayer(-6f, [entry], [new PluginDungeonWall(new(60, -165), new(60, -155))]),
                new PluginDungeonLayer(0f, [hall], []),
            ],
            [
                new PluginDungeonCell(Landblock | 0x100u, new(65, -160, -6), -6f),
                new PluginDungeonCell(Landblock | 0x10Au, new(70, -125, 0), 0f),
            ],
            new Vector3(50, -165, -6),
            new Vector3(90, -100, 0));
        var rig = new Rig();
        rig.Dungeon.Plan = plan;
        rig.Settings = rig.Settings with { MapZoom = 4.2f, Opacity = 16 };
        rig.Objects.Add(Object(0x80000010u, "Portal to Yaraq", PluginObjectClass.Portal, 68f, -158f, -6f));
        rig.Objects.Add(Object(0x80000011u, "Greenspire", PluginObjectClass.Portal, 55f, -120f, 0f));
        rig.Move(60.03f, -160.03f, -5.995f, 0f);
        rig.Tick();
        Painter painter = rig.Paint();

        Assert.Equal(-1, (int)Math.Floor((rig.Controller.DrawZ + 3d) / 6d));
        var floor = Assert.Single(painter.Lines, line => Math.Abs(line.Thickness - 40f) < 0.01f && line.Color.G == 127 && line.Color.A == 204);
        Assert.Equal((0, 127, 191), (floor.Color.R, floor.Color.G, floor.Color.B));
        Assert.Equal(40d, floor.To.X - floor.From.X, 2);
        var above = Assert.Single(painter.Lines, line => Math.Abs(line.Thickness - 200f) < 0.01f);
        Assert.Equal((0, 75, 113, 97), (above.Color.R, above.Color.G, above.Color.B, above.Color.A));
        Assert.Single(painter.Texts, t => t.Text == "Yaraq");
        Assert.DoesNotContain(painter.Texts, t => t.Text == "Greenspire");

        rig.Controller.ShowAllLayers = true;
        rig.Tick();
        painter = rig.Paint();
        Assert.Single(painter.Texts, t => t.Text == "Yaraq");
        Assert.Single(painter.Texts, t => t.Text == "Greenspire");
    }

    /// <summary>
    /// A mark is the object's icon when its row asks for one and the client
    /// has it, otherwise a dot in the row's colour; a label is written
    /// upright beside the mark, in a pass after the floor, only for the
    /// kinds whose row says so. A kind switched off is not drawn at all.
    /// Mutation: labelling every kind writes "Drudge" for a monster.
    /// </summary>
    [Fact]
    public void MarksAreIconsOrDotsAndLabelsAreWrittenUprightAfterTheFloor()
    {
        var rig = new Rig();
        rig.Settings = rig.Settings with { MapZoom = 4f };
        rig.Objects.Add(Object(0x80000001u, "Drudge", PluginObjectClass.Monster, 15f, 5f, 0f));
        rig.Objects.Add(Object(0x80000002u, "Chest", PluginObjectClass.Container, 15f, 8f, 0f));
        rig.Objects.Add(Object(0x50000002u, "Horan", PluginObjectClass.Player, 5f, 8f, 0f));
        rig.Objects.Add(Object(0x80000003u, "Lifestone", PluginObjectClass.Lifestone, 2f, 2f, 0f));
        var drudgeIcon = new PluginImage(7, 32, 32);
        rig.Ui.ImageStore.ObjectIcons[0x80000001u] = drudgeIcon;
        rig.Tick();
        Painter painter = rig.Paint();

        double cx = DungeonMapController.CanvasWidth / 2d;
        double cy = DungeonMapController.CanvasHeight / 2d;
        (PluginImage image, PluginRect at) = Assert.Single(painter.Images);
        Assert.Equal(drudgeIcon, image);
        Assert.Equal(cx + 50d, at.X + (at.Width / 2d), 2);
        Assert.Equal(cy, at.Y + (at.Height / 2d), 2);

        Assert.Contains(painter.Lines, line =>
            line.Thickness == 1f && line.Color.R == 244 && line.Color.G == 164 && line.Color.B == 96
            && Math.Abs(line.From.Y - (cy - 15d)) < 4d && line.From.X > cx + 40d && line.To.X < cx + 60d);
        Assert.Contains(painter.Lines, line =>
            line.Thickness == 1f && line.Color.R == 255 && line.Color.G == 0 && line.Color.B == 0
            && Math.Abs(line.From.Y - cy) < 4d && Math.Abs(line.From.X - cx) < 4d);

        (string text, PluginPoint at2, PluginColor color) = Assert.Single(painter.Texts, t => t.Text == "Horan");
        Assert.Equal(cx - 15d, at2.X, 2);
        Assert.True(at2.Y > cy - 15d && at2.Y < cy);
        Assert.Equal(255, color.R);
        Assert.DoesNotContain(painter.Texts, t => t.Text == "Drudge");
        Assert.DoesNotContain(painter.Texts, t => t.Text == "Lifestone");
        Assert.DoesNotContain(painter.Lines, line => line.Color.R == 245 && line.Color.G == 245 && line.Color.B == 245);

        rig.Move(5f, 5f, 0f, 90f);
        rig.Tick();
        painter = rig.Paint();
        (_, PluginPoint turned, _) = Assert.Single(painter.Texts, t => t.Text == "Horan");
        Assert.Equal(cx - 15d - 15d, turned.X, 2);
        Assert.True(turned.Y > cy);
    }

    /// <summary>
    /// The cell the player stands in is shaded in the visited colour over
    /// the floor while the row is on, and the shading is kept in the
    /// settings tree so it is there on the next visit.
    /// </summary>
    [Fact]
    public void TheCellStoodInIsShadedInTheVisitedColourAndKept()
    {
        var rig = new Rig();
        rig.Settings = rig.Settings with { MapZoom = 4f };
        rig.Tick();
        Painter painter = rig.Paint();

        double cx = DungeonMapController.CanvasWidth / 2d;
        double cy = DungeonMapController.CanvasHeight / 2d;
        var visited = Assert.Single(painter.Lines, line =>
            line.Thickness == 50f && line.Color.R == 255 && line.Color.G == 150 && line.Color.B == 255);
        Assert.True(Near(visited.From, cx - 25d, cy));
        Assert.Contains(rig.Storage.Keys, key => key == "mosstank/ub/dungeonmaps/visited/01A9.txt");

        rig.Settings = rig.Settings with { ShowVisitedTiles = false };
        rig.Controller.OnTick(DungeonMapController.SettingsPollSeconds + 1d);
        Assert.Equal(2, rig.Canvas.Invalidations);
        painter = rig.Paint();
        Assert.DoesNotContain(painter.Lines, line => line.Color.G == 150);
    }

    /// <summary>
    /// The name across the top says which dungeon and which storey, the
    /// compass points north with an N at its tip, and each goes when its
    /// row is off.
    /// </summary>
    [Fact]
    public void TheNameAndTheCompassAreDrawnWhenTheirRowsSay()
    {
        var rig = new Rig();
        rig.Move(5f, 5f, 7f, 90f);
        rig.Tick();
        Painter painter = rig.Paint();

        Assert.Contains(painter.Texts, t => t.Text == "Dungeon 01A9 (Z:1)");
        (string _, PluginPoint tip, _) = Assert.Single(painter.Texts, t => t.Text == "N");
        Assert.True(tip.X < DungeonMapController.CanvasWidth - 18d);

        rig.Settings = rig.Settings with
        {
            ShowCompass = false,
            DungeonName = rig.Settings.DungeonName with { Enabled = false },
        };
        rig.Controller.OnTick(DungeonMapController.SettingsPollSeconds + 1d);
        painter = rig.Paint();
        Assert.Empty(painter.Texts);
    }

    /// <summary>
    /// The height slider and a pan stop the map following the player; the
    /// slider picks the storey drawn; follow brings both back. Held icons
    /// are let go when the map is disposed.
    /// </summary>
    [Fact]
    public void TheSliderAndAPanStopFollowingAndFollowBringsItBack()
    {
        var rig = new Rig();
        rig.Tick();
        Assert.True(rig.Controller.Camera.Following);
        Assert.Equal("Follow: on", rig.Controller.FollowText);

        rig.Controller.SetHeightPercent(100);
        rig.Tick();
        Assert.False(rig.Controller.Camera.Following);
        Assert.Equal(6d, rig.Controller.DrawZ, 3);
        Assert.Equal(100, rig.Controller.HeightPercent);
        Assert.Contains("Z:1", Assert.Single(rig.Paint().Texts, t => t.Text.StartsWith("Dungeon", StringComparison.Ordinal)).Text);

        rig.Controller.ToggleFollow();
        rig.Tick();
        Assert.True(rig.Controller.Camera.Following);
        Assert.Equal(0d, rig.Controller.DrawZ, 3);
        Assert.Equal(0, rig.Controller.HeightPercent);

        rig.Controller.Pan(DungeonMapController.PanStepPixels, 0d);
        Assert.False(rig.Controller.Camera.Following);
        rig.Controller.ToggleFollow();
        rig.Tick();
        Assert.Equal(new Vector2(5f, 5f), rig.Controller.Camera.CenterMeters);

        rig.Objects.Add(Object(0x80000001u, "Drudge", PluginObjectClass.Monster, 15f, 5f, 0f));
        rig.Ui.ImageStore.ObjectIcons[0x80000001u] = new PluginImage(7, 32, 32);
        rig.Tick();
        rig.Paint();
        rig.Controller.Dispose();
        Assert.Equal(1, rig.Ui.ImageStore.Released);
        Assert.True(rig.Canvas.Disposed);
    }

    /// <summary>The strip's status line says where the map is, or that it is nowhere.</summary>
    [Fact]
    public void TheStatusSaysWhichDungeonAndHowMuchOfItIsWalked()
    {
        var rig = new Rig();
        rig.Dungeon.Sealed = false;
        rig.Tick();
        Assert.Equal("Not in a dungeon.", rig.Controller.Status);
        rig.Dungeon.Sealed = true;
        rig.Tick();
        Assert.Equal("Dungeon 01A9: 2 storeys, 1 of 3 cells walked.", rig.Controller.Status);
    }
}

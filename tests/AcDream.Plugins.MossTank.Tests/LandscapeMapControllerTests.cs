using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The countryside map against a fake canvas: it registers once, shows
/// only while its window is open and its row is on, asks for a repaint only
/// when something drawn changed, and paints the map art (or a grid), the
/// markers, the rulers, the character and the readout.
/// </summary>
public sealed class LandscapeMapControllerTests
{
    internal sealed class FakeCanvas(PluginCanvasDescriptor descriptor, Action<IPluginPainter> paint) : IPluginCanvas
    {
        public PluginCanvasDescriptor Descriptor { get; } = descriptor;
        public Action<IPluginPainter> PaintCallback { get; } = paint;
        public int Invalidations { get; private set; }
        public bool Disposed { get; private set; }
        public string CanvasId => Descriptor.CanvasId;
        public int Width => Descriptor.Width;
        public int Height => Descriptor.Height;
        public bool IsAvailable => true;
        public bool IsVisible { get; set; } = descriptor.StartVisible;
        public PluginCanvasAnchor Anchor { get; set; } = descriptor.Anchor;
        public PluginPoint Offset { get; set; } = descriptor.Offset;
        public Action<PluginPointerEvent>? PointerHandler { get; set; }
        public int Releases { get; private set; }
        public void Invalidate() => Invalidations++;
        public void ReleasePointer() => Releases++;
        public void Dispose() => Disposed = true;

        // ---- Delivering pointer events, as the host would ----------------

        public void Send(PluginPointerEvent pointerEvent)
        {
            Assert.True(Descriptor.AcceptsPointerInput, "the canvas did not opt into pointer input");
            Assert.NotNull(PointerHandler);
            PointerHandler!(pointerEvent);
        }

        public void Down(double x, double y, PluginKeyModifiers modifiers = PluginKeyModifiers.None, PluginPointerButton button = PluginPointerButton.Left) =>
            Send(new PluginPointerEvent(PluginPointerEventKind.Down, new PluginPoint(x, y), button, modifiers, 0));

        public void Move(double x, double y, PluginKeyModifiers modifiers = PluginKeyModifiers.None, PluginPointerButton button = PluginPointerButton.Left) =>
            Send(new PluginPointerEvent(PluginPointerEventKind.Move, new PluginPoint(x, y), button, modifiers, 0));

        public void Up(double x, double y, PluginKeyModifiers modifiers = PluginKeyModifiers.None, PluginPointerButton button = PluginPointerButton.Left) =>
            Send(new PluginPointerEvent(PluginPointerEventKind.Up, new PluginPoint(x, y), button, modifiers, 0));

        public void Wheel(double x, double y, int notches) =>
            Send(new PluginPointerEvent(PluginPointerEventKind.Wheel, new PluginPoint(x, y), PluginPointerButton.None, PluginKeyModifiers.None, notches));

        public void Cancel(double x, double y) =>
            Send(new PluginPointerEvent(PluginPointerEventKind.Cancelled, new PluginPoint(x, y), PluginPointerButton.Left, PluginKeyModifiers.None, 0));

        /// <summary>A whole drag: press, one move, release.</summary>
        public void Drag(double fromX, double fromY, double toX, double toY, PluginKeyModifiers modifiers = PluginKeyModifiers.None)
        {
            Down(fromX, fromY, modifiers);
            Move(toX, toY, modifiers);
            Up(toX, toY, modifiers);
        }
    }

    internal sealed class FakeImages : IPluginImages
    {
        public HashSet<uint> ClientArt { get; } = [];
        public Dictionary<uint, PluginImage> ObjectIcons { get; } = [];
        public List<uint> Requested { get; } = [];
        public int Released { get; private set; }
        public bool IsAvailable => true;
        public int Count => 0;
        public int MaximumCount => 256;
        public long MaximumBytes => 32L * 1024 * 1024;
        public int MaximumDimension => 2048;

        public PluginImage FromClientArt(uint surfaceIdOrIndex)
        {
            Requested.Add(surfaceIdOrIndex);
            return ClientArt.Contains(surfaceIdOrIndex)
                ? new PluginImage((int)(surfaceIdOrIndex & 0xFFFF) + 1, 1024, 1024)
                : PluginImage.None;
        }

        public PluginImage FromSpellIcon(uint spellId) => PluginImage.None;

        public PluginImage FromObjectIcon(uint objectId) =>
            ObjectIcons.TryGetValue(objectId, out PluginImage icon) ? icon : PluginImage.None;

        public PluginImage FromStream(string name, Func<Stream> open) => PluginImage.None;

        public bool Release(PluginImage image)
        {
            Released++;
            return true;
        }
    }

    internal sealed class FakeUi : IUiRegistry
    {
        public FakeImages ImageStore { get; } = new();
        public List<FakeCanvas> Canvases { get; } = [];
        public IPluginImages Images => ImageStore;

        public IPluginCanvas RegisterCanvas(PluginCanvasDescriptor descriptor, Action<IPluginPainter> paint)
        {
            var canvas = new FakeCanvas(descriptor, paint);
            Canvases.Add(canvas);
            return canvas;
        }

        public void AddMarkupPanel(string markupPath, object binding) { }
        public void AddPanel(PluginPanelDescriptor descriptor, string markupPath, object binding) { }
        public IDisposable RegisterPanel(PluginPanelDescriptor descriptor, string markupPath, object binding) =>
            NoOpUiRegistration.Instance;
        public IDisposable RegisterPanelContent(PluginPanelDescriptor descriptor, string markupContent, object binding) =>
            NoOpUiRegistration.Instance;
    }

    /// <summary>Records every call so a paint can be read back as a list of what it drew.</summary>
    internal sealed class RecordingPainter(int width, int height) : IPluginPainter
    {
        public List<string> Ops { get; } = [];
        public List<string> Texts { get; } = [];
        public List<PluginImage> Images { get; } = [];
        public int ClipDepth { get; private set; }
        public int Width => width;
        public int Height => height;
        public void Clear(PluginColor color) => Ops.Add($"clear a={color.A}");
        public void FillRect(PluginRect rect, PluginColor color) => Ops.Add("fill");
        public void StrokeRect(PluginRect rect, PluginColor color, float thickness) => Ops.Add("stroke");
        public void DrawLine(PluginPoint from, PluginPoint to, PluginColor color, float thickness) =>
            Ops.Add($"line {color.R},{color.G},{color.B}");
        public void DrawText(string text, PluginPoint position, PluginColor color, bool outline)
        {
            Ops.Add("text");
            Texts.Add(text);
        }
        public PluginSize MeasureText(string text) => new(text.Length * 6d, 12d);
        public void DrawImage(PluginImage image, PluginRect destination, PluginColor tint)
        {
            Ops.Add("image");
            Images.Add(image);
        }
        public void DrawImageTransformed(PluginImage image, PluginRect destination, PluginColor tint,
            double rotationRadians, PluginPoint pivot, double scaleX, double scaleY)
        {
            Ops.Add("image-transformed");
            Images.Add(image);
        }
        public void PushClip(PluginRect rect) => ClipDepth++;
        public void PopClip() => ClipDepth--;
    }

    private sealed class Rig
    {
        public FakeUi Ui { get; } = new();
        public LandscapeMapSettings Settings { get; set; } = LandscapeMapSettings.Default;
        public PluginNavigationPosition? Player { get; set; } =
            new(0xA9B40001u, 33.6d, 42.1d, 0d, 90f, true);
        public LandscapeMarkerCatalog Markers { get; } = new();
        public LandscapeMapController Controller { get; }

        public Rig()
        {
            Controller = new LandscapeMapController(Ui, () => Settings, () => Player, Markers);
        }

        public FakeCanvas Canvas => Assert.Single(Ui.Canvases);

        public RecordingPainter Paint()
        {
            var painter = new RecordingPainter(LandscapeMapController.CanvasWidth, LandscapeMapController.CanvasHeight);
            Canvas.PaintCallback(painter);
            Assert.Equal(0, painter.ClipDepth);
            return painter;
        }

        public void Ticks(int count) { for (int i = 0; i < count; i++) Controller.OnTick(0.015d); }
    }

    /// <summary>
    /// Mutation: register on construction and a headless session with the
    /// map never opened still holds a canvas.
    /// </summary>
    [Fact]
    public void TheCanvasIsRegisteredOnceWhenTheWindowFirstOpensAndShownOnlyWhileItIsOpen()
    {
        var rig = new Rig();
        rig.Ticks(3);
        Assert.Empty(rig.Ui.Canvases);

        rig.Controller.Shown = true;
        rig.Ticks(1);
        FakeCanvas canvas = rig.Canvas;
        Assert.Equal(LandscapeMapController.CanvasId, canvas.CanvasId);
        Assert.True(canvas.IsVisible);
        Assert.Equal(1, canvas.Invalidations);

        rig.Controller.Shown = false;
        rig.Ticks(1);
        Assert.False(canvas.IsVisible);
        Assert.Single(rig.Ui.Canvases);

        rig.Controller.Shown = true;
        rig.Ticks(1);
        Assert.True(canvas.IsVisible);
        Assert.Single(rig.Ui.Canvases);
    }

    /// <summary>
    /// Mutation: ignore the row and the canvas stays up with the map off.
    /// </summary>
    [Fact]
    public void TheEnabledRowOnThePageHidesTheCanvas()
    {
        var rig = new Rig { Settings = new LandscapeMapSettings(false, 16) };
        rig.Controller.Shown = true;
        rig.Ticks(1);
        Assert.Empty(rig.Ui.Canvases);

        rig.Settings = new LandscapeMapSettings(true, 16);
        rig.Ticks(20);
        Assert.True(rig.Canvas.IsVisible);

        rig.Settings = new LandscapeMapSettings(false, 16);
        rig.Ticks(20);
        Assert.False(rig.Canvas.IsVisible);
    }

    /// <summary>
    /// A character standing still costs no repaint; a pan, a zoom, a turn,
    /// a step onto another canvas pixel, a marker file change or a page
    /// change each cost one. Mutation: invalidate every frame and the
    /// still count climbs.
    /// </summary>
    [Fact]
    public void ARepaintIsAskedForOnlyWhenSomethingDrawnChanged()
    {
        var rig = new Rig();
        rig.Controller.Shown = true;
        rig.Ticks(1);
        Assert.Equal(1, rig.Canvas.Invalidations);

        rig.Ticks(60);
        Assert.Equal(1, rig.Canvas.Invalidations);

        rig.Controller.Pan(10d, 0d);
        rig.Ticks(1);
        Assert.Equal(2, rig.Canvas.Invalidations);

        rig.Controller.ZoomIn();
        rig.Ticks(1);
        Assert.Equal(3, rig.Canvas.Invalidations);

        rig.Player = rig.Player!.Value with { HeadingDegrees = 91f };
        rig.Ticks(1);
        Assert.Equal(4, rig.Canvas.Invalidations);

        // A step too small to move the character's canvas pixel.
        rig.Player = rig.Player!.Value with { EastWest = 33.6001d };
        rig.Ticks(1);
        Assert.Equal(4, rig.Canvas.Invalidations);

        // A step that does, while following.
        rig.Controller.ToggleFollow();
        rig.Ticks(1);
        Assert.Equal(5, rig.Canvas.Invalidations);
        rig.Player = rig.Player!.Value with { EastWest = 34.6d };
        rig.Ticks(1);
        Assert.Equal(6, rig.Canvas.Invalidations);

        var storage = new LandscapeMarkerCatalogTests_Storage();
        storage.WriteText(LandscapeMarkerCatalog.Folder + "a.csv", "Town, Holtburg, 42.1, 33.6\n");
        rig.Markers.Load(storage);
        rig.Ticks(1);
        Assert.Equal(7, rig.Canvas.Invalidations);

        rig.Settings = new LandscapeMapSettings(true, 200);
        rig.Ticks(20);
        Assert.Equal(8, rig.Canvas.Invalidations);
    }

    // ---- The pointer -----------------------------------------------------

    /// <summary>
    /// The canvas opts into pointer input with a handler from the moment it
    /// is registered. A drag slides the map with the hand by its delta, in
    /// image pixels at the view's scale, exactly as the arrow buttons do,
    /// takes the view off the character, and asks for one repaint.
    /// Mutation: leaving the descriptor click-through, or panning by the
    /// position instead of the delta, fails the first move.
    /// </summary>
    [Fact]
    public void ADragPansTheMapByItsDeltaAndStopsFollowing()
    {
        var rig = new Rig();
        rig.Controller.Shown = true;
        rig.Controller.ToggleFollow();
        rig.Ticks(1);
        FakeCanvas canvas = rig.Canvas;
        Assert.True(canvas.Descriptor.AcceptsPointerInput);
        Assert.True(rig.Controller.View.Follow);
        double offsetX = rig.Controller.View.OffsetX;
        double offsetY = rig.Controller.View.OffsetY;
        double scale = rig.Controller.View.Scale;

        canvas.Down(100d, 100d);
        canvas.Move(140d, 100d);
        canvas.Move(140d, 110d);
        canvas.Up(140d, 110d);
        Assert.False(rig.Controller.View.Follow);
        Assert.Equal("Follow: off", rig.Controller.FollowText);
        Assert.Equal(offsetX - (40d / scale), rig.Controller.View.OffsetX, 6);
        Assert.Equal(offsetY - (10d / scale), rig.Controller.View.OffsetY, 6);

        rig.Ticks(1);
        Assert.Equal(2, canvas.Invalidations);
    }

    /// <summary>
    /// The wheel zooms a step a notch about the cursor: the position under
    /// it before is the position under it after. A notch past the limit
    /// changes nothing and asks for nothing. Mutation: zooming about the
    /// middle moves the position under the cursor; repainting on every
    /// notch repaints at the limit.
    /// </summary>
    [Fact]
    public void TheWheelZoomsAboutTheCursorKeepingThePositionUnderItFixed()
    {
        var rig = new Rig();
        rig.Controller.Shown = true;
        rig.Ticks(1);
        FakeCanvas canvas = rig.Canvas;
        LandscapeMapView view = rig.Controller.View;

        (double eastWest, double northSouth) = view.ToWorld(200d, 150d);
        canvas.Wheel(200d, 150d, 1);
        Assert.Equal(0.08d, view.Zoom, 6);
        (double eastAfter, double northAfter) = view.ToWorld(200d, 150d);
        Assert.Equal(eastWest, eastAfter, 6);
        Assert.Equal(northSouth, northAfter, 6);
        rig.Ticks(1);
        Assert.Equal(2, canvas.Invalidations);

        canvas.Wheel(200d, 150d, -2);
        Assert.Equal(0d, view.Zoom, 6);
        (eastAfter, northAfter) = view.ToWorld(200d, 150d);
        Assert.Equal(eastWest, eastAfter, 6);
        Assert.Equal(northSouth, northAfter, 6);
        rig.Ticks(1);
        Assert.Equal(3, canvas.Invalidations);

        canvas.Wheel(200d, 150d, -1);
        rig.Ticks(1);
        Assert.Equal(3, canvas.Invalidations);
    }

    /// <summary>
    /// While a button is held over the map the readout is the position
    /// under the cursor, painted so; released, it is the middle again, each
    /// change one repaint. Mutation: reading the middle throughout fails
    /// the pressed readout.
    /// </summary>
    [Fact]
    public void TheReadoutFollowsTheCursorWhileItIsPressedOverTheMap()
    {
        var rig = new Rig();
        rig.Controller.Shown = true;
        rig.Ticks(1);
        FakeCanvas canvas = rig.Canvas;
        LandscapeMapView view = rig.Controller.View;
        string middle = rig.Controller.Readout;

        canvas.Down(40d, 60d);
        (double eastWest, double northSouth) = view.ToWorld(40d, 60d);
        string under = LandscapeMapView.PositionText(eastWest, northSouth);
        Assert.NotEqual(middle, under);
        Assert.Equal(under, rig.Controller.Readout);
        rig.Ticks(1);
        Assert.Equal(2, canvas.Invalidations);
        Assert.Contains(under, rig.Paint().Texts);

        canvas.Up(40d, 60d);
        Assert.Equal(middle, rig.Controller.Readout);
        rig.Ticks(1);
        Assert.Equal(3, canvas.Invalidations);
    }

    /// <summary>
    /// A cancelled press ends the drag and the readout goes back to the
    /// middle: a move after it pans nothing. Mutation: leaving the drag
    /// open on cancel pans from the cancelled spot.
    /// </summary>
    [Fact]
    public void ACancelledPressEndsTheDrag()
    {
        var rig = new Rig();
        rig.Controller.Shown = true;
        rig.Ticks(1);
        FakeCanvas canvas = rig.Canvas;
        string middle = rig.Controller.Readout;
        double offsetX = rig.Controller.View.OffsetX;

        canvas.Down(100d, 100d);
        canvas.Cancel(100d, 100d);
        Assert.Equal(middle, rig.Controller.Readout);
        canvas.Move(140d, 100d);
        Assert.Equal(offsetX, rig.Controller.View.OffsetX);
        Assert.Equal(middle, rig.Controller.Readout);
    }

    /// <summary>
    /// With the client's map art to hand the paint draws it once, then the
    /// rulers, the character and the readout, and leaves no clip pushed.
    /// Mutation: skip the art and the grid draws in its place.
    /// </summary>
    [Fact]
    public void ThePaintDrawsTheClientsMapArtWhenTheClientHasIt()
    {
        var rig = new Rig();
        rig.Ui.ImageStore.ClientArt.Add(LandscapeMapController.MapArtSurfaceId);
        rig.Controller.Shown = true;
        rig.Ticks(1);

        RecordingPainter painter = rig.Paint();
        Assert.Equal("clear a=16", painter.Ops[0]);
        Assert.Single(painter.Images);
        Assert.Contains(LandscapeMapController.MapArtSurfaceId, rig.Ui.ImageStore.Requested);
        Assert.Equal(1024, rig.Controller.View.ImageWidth);
        Assert.Contains("line 255,0,255", painter.Ops);
        Assert.Contains(rig.Controller.Readout, painter.Texts);
        Assert.Contains(painter.Texts, static text => text.EndsWith('E'));
        Assert.Contains(painter.Texts, static text => text.EndsWith('N'));
    }

    /// <summary>
    /// Mutation: draw nothing without the art and the map is a blank box.
    /// </summary>
    [Fact]
    public void ThePaintDrawsAGridWhenTheClientHasNoMapArt()
    {
        var rig = new Rig();
        rig.Controller.Shown = true;
        rig.Ticks(1);

        RecordingPainter painter = rig.Paint();
        Assert.Empty(painter.Images);
        Assert.Contains("line 70,80,110", painter.Ops);
        Assert.Contains(rig.Controller.Readout, painter.Texts);
    }

    /// <summary>
    /// A marker shows its icon in its zoom band and its name only in the
    /// name's band, and only while on the canvas. Mutation: drop the band
    /// check and a vendor's name shows on the world view.
    /// </summary>
    [Fact]
    public void MarkersDrawTheirIconsAndNamesInTheirOwnZoomBands()
    {
        var rig = new Rig();
        rig.Ui.ImageStore.ClientArt.Add(LandscapeMarkerCatalog.DisplayOf(LandscapeMarkerKind.Vendor).IconSurfaceId);
        var storage = new LandscapeMarkerCatalogTests_Storage();
        storage.WriteText(
            LandscapeMarkerCatalog.Folder + "a.csv",
            "Vendor, Aun Tanua, 42.1, 33.6\nTown, Far Away, -90, -90\n");
        rig.Markers.Load(storage);
        rig.Controller.Shown = true;
        rig.Controller.ToggleFollow();
        rig.Ticks(1);

        RecordingPainter world = rig.Paint();
        Assert.DoesNotContain("Aun Tanua", world.Texts);
        Assert.DoesNotContain("Far Away", world.Texts);
        Assert.Empty(world.Images);

        rig.Controller.View.Zoom = 0.5d;
        RecordingPainter middle = rig.Paint();
        Assert.Single(middle.Images);
        Assert.DoesNotContain("Aun Tanua", middle.Texts);

        rig.Controller.View.Zoom = 0.85d;
        RecordingPainter close = rig.Paint();
        Assert.Single(close.Images);
        Assert.Contains("Aun Tanua", close.Texts);
    }

    /// <summary>
    /// Mutation: keep the handles over a session end and a reconnect draws
    /// with dropped images.
    /// </summary>
    [Fact]
    public void ASessionResetDropsTheImageHandlesAndAsksAgainOnTheNextPaint()
    {
        var rig = new Rig();
        rig.Ui.ImageStore.ClientArt.Add(LandscapeMapController.MapArtSurfaceId);
        rig.Controller.Shown = true;
        rig.Ticks(1);
        rig.Paint();
        rig.Paint();
        Assert.Single(rig.Ui.ImageStore.Requested);

        rig.Controller.Reset();
        rig.Paint();
        Assert.Equal(2, rig.Ui.ImageStore.Requested.Count);

        rig.Controller.Dispose();
        Assert.True(rig.Canvas.Disposed);
        Assert.Equal(1, rig.Ui.ImageStore.Released);
    }

    /// <summary>
    /// Follow centres the character and comes in to the follow zoom;
    /// a pan lets go again. Mutation: skip the zoom floor and follow leaves
    /// the whole world showing.
    /// </summary>
    [Fact]
    public void FollowCentresOnTheCharacterAndAPanLetsGo()
    {
        var rig = new Rig();
        rig.Controller.Shown = true;
        rig.Ticks(1);
        Assert.Equal("Follow: off", rig.Controller.FollowText);

        rig.Controller.ToggleFollow();
        Assert.Equal("Follow: on", rig.Controller.FollowText);
        Assert.Equal(LandscapeMapView.FollowZoom, rig.Controller.View.Zoom);
        Assert.Equal("42.10N, 33.60E", rig.Controller.Readout);

        rig.Controller.Pan(40d, 0d);
        Assert.Equal("Follow: off", rig.Controller.FollowText);
        Assert.NotEqual("42.10N, 33.60E", rig.Controller.Readout);
    }

    private sealed class LandscapeMarkerCatalogTests_Storage : IPluginStorage
    {
        private readonly Dictionary<string, string> _text = new(StringComparer.Ordinal);
        public bool IsAvailable => true;
        public string? RootPath => null;
        public string? ReadText(string key) => _text.TryGetValue(key, out string? text) ? text : null;
        public IReadOnlyList<string> List(string prefix) =>
            _text.Keys.Where(key => key.StartsWith(prefix, StringComparison.Ordinal)).ToArray();
        public void WriteText(string key, string content) => _text[key] = content;
        public bool Delete(string key) => _text.Remove(key);
        public IPluginStorage OpenScope(PluginStorageScope scope) => this;
    }
}

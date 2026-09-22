using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>The two rows on the UB page the countryside map reads.</summary>
internal readonly record struct LandscapeMapSettings(bool Enabled, int Opacity)
{
    public static LandscapeMapSettings Default => new(true, 16);
}

/// <summary>
/// The countryside map: the client's own map art on a canvas, with the
/// character on it, the points of interest around them, coordinate rulers
/// down the sides, and a readout of the coordinate at the middle.
/// </summary>
/// <remarks>
/// <para>
/// The canvas is retained: the host keeps what was last painted and
/// repaints only when asked. So the map asks only when something it draws
/// changed: the view panned or zoomed, the character moved by a whole
/// canvas pixel or turned, the marker set changed, the page changed, or
/// the map was shown. A frame in which the character stood still costs a
/// few comparisons.
/// </para>
/// <para>
/// The canvas takes the pointer: a drag slides the map and the wheel zooms
/// about the cursor, through the same view arithmetic the buttons drive
/// about the middle. The host reports the pointer only while a button is
/// held or the wheel turns, never a bare hover, so the readout is the
/// position under the cursor while a button is held and the middle
/// otherwise. A press the host cancels ends the drag where it is.
/// </para>
/// </remarks>
internal sealed class LandscapeMapController : IDisposable
{
    public const string CanvasId = "ub-map";
    public const int CanvasWidth = 320;
    public const int CanvasHeight = 280;
    public const int GutterWidth = 20;
    public const int IconSize = 16;
    public const int PanStepPixels = 40;

    /// <summary>The client's overworld map art, the one the map page draws.</summary>
    public const uint MapArtSurfaceId = 0x06000261u;

    /// <summary>How often the page's rows are re-read for a change.</summary>
    internal const double SettingsPollSeconds = 0.25d;

    /// <summary>The map art's size when the client has none to hand, so the grid has a shape.</summary>
    private const int FallbackImageSize = 2048;

    private static readonly PluginColor Background = new(29, 33, 59, 255);
    private static readonly PluginColor GutterFill = new(0, 0, 0, 140);
    private static readonly PluginColor TickColor = new(128, 128, 128, 255);
    private static readonly PluginColor TickText = new(211, 211, 211, 255);
    private static readonly PluginColor GridLine = new(70, 80, 110, 255);
    private static readonly PluginColor PlayerColor = new(255, 0, 255, 255);
    private static readonly PluginColor LabelColor = new(255, 255, 255, 255);

    private readonly IUiRegistry _ui;
    private readonly Func<LandscapeMapSettings> _readSettings;
    private readonly Func<PluginNavigationPosition?> _player;
    private readonly LandscapeMarkerCatalog _markers;
    private readonly Dictionary<uint, PluginImage> _images = [];
    private readonly MapLabelPlacer _placer = new();
    private IPluginCanvas? _canvas;
    private LandscapeMapSettings _settings;
    private double _settingsPollRemaining;
    private bool _shown;
    private bool _dirty = true;
    private int _lastPlayerX = int.MinValue;
    private int _lastPlayerY = int.MinValue;
    private float _lastHeading = float.NaN;
    private int _seenMarkerRevision = -1;
    private PluginNavigationPosition? _lastPlayer;
    private PluginPoint? _dragFrom;
    private PluginPoint? _pointer;

    public LandscapeMapController(
        IUiRegistry ui,
        Func<LandscapeMapSettings> readSettings,
        Func<PluginNavigationPosition?> player,
        LandscapeMarkerCatalog markers)
    {
        _ui = ui;
        _readSettings = readSettings;
        _player = player;
        _markers = markers;
        _settings = readSettings();
        View = new LandscapeMapView(CanvasWidth, CanvasHeight, FallbackImageSize, FallbackImageSize);
    }

    /// <summary>The view arithmetic; the buttons and the paint both go through it.</summary>
    public LandscapeMapView View { get; private set; }

    /// <summary>Whether the map's window is open. The canvas shows only while it is.</summary>
    public bool Shown
    {
        get => _shown;
        set
        {
            if (_shown == value)
                return;
            _shown = value;
            _dirty = true;
        }
    }

    /// <summary>How many times the host has been asked to repaint.</summary>
    internal int InvalidateCount { get; private set; }

    /// <summary>
    /// The coordinate under the cursor while a button is held over the map,
    /// else at the middle of the canvas, the way the in-game readout writes it.
    /// </summary>
    public string Readout
    {
        get
        {
            (double x, double y) = _pointer is { } at ? (at.X, at.Y) : (CanvasWidth / 2d, CanvasHeight / 2d);
            (double eastWest, double northSouth) = View.ToWorld(x, y);
            return LandscapeMapView.PositionText(eastWest, northSouth);
        }
    }

    public string FollowText => View.Follow ? "Follow: on" : "Follow: off";

    /// <summary>Zooms in one step about the middle of the canvas.</summary>
    public void ZoomIn() => ZoomAbout(CanvasWidth / 2d, CanvasHeight / 2d, LandscapeMapView.ZoomStep);

    /// <summary>Zooms out one step about the middle of the canvas.</summary>
    public void ZoomOut() => ZoomAbout(CanvasWidth / 2d, CanvasHeight / 2d, -LandscapeMapView.ZoomStep);

    /// <summary>Zooms about a canvas point, the way the wheel does. A step past the limit changes nothing.</summary>
    public void ZoomAbout(double x, double y, double delta)
    {
        double before = View.Zoom;
        View.ZoomAbout(x, y, delta);
        if (View.Zoom != before)
            _dirty = true;
    }

    /// <summary>Slides the map by a number of canvas pixels, the way a drag does.</summary>
    public void Pan(double dxPixels, double dyPixels)
    {
        View.PanBy(dxPixels, dyPixels);
        _dirty = true;
    }

    /// <summary>
    /// Follow on: centre on the character and come in to at least the
    /// follow zoom. Follow off: leave the view where it is.
    /// </summary>
    public void ToggleFollow()
    {
        View.Follow = !View.Follow;
        if (View.Follow)
        {
            if (View.Zoom < LandscapeMapView.FollowZoom)
                View.Zoom = LandscapeMapView.FollowZoom;
            if (_player() is { } player)
                View.CenterOn(player.EastWest, player.NorthSouth);
        }
        _dirty = true;
    }

    /// <summary>
    /// The frame: re-read the page now and then, re-centre while following,
    /// and ask for a repaint only when something drawn changed.
    /// </summary>
    public void OnTick(double elapsedSeconds)
    {
        _settingsPollRemaining -= Math.Max(0d, elapsedSeconds);
        if (_settingsPollRemaining <= 0d)
        {
            _settingsPollRemaining = SettingsPollSeconds;
            LandscapeMapSettings next = _readSettings();
            if (next != _settings)
            {
                _settings = next;
                _dirty = true;
            }
        }
        bool visible = _shown && _settings.Enabled;
        if (visible && _canvas is null)
        {
            _canvas = _ui.RegisterCanvas(
                new PluginCanvasDescriptor(CanvasId, CanvasWidth, CanvasHeight)
                {
                    Anchor = PluginCanvasAnchor.TopLeft,
                    Offset = new PluginPoint(40d, 200d),
                    StartVisible = false,
                    AcceptsPointerInput = true,
                },
                Paint);
            _canvas.PointerHandler = OnPointer;
        }
        if (_canvas is null)
            return;
        if (_canvas.IsVisible != visible)
        {
            _canvas.IsVisible = visible;
            _dirty = true;
        }
        if (!visible)
            return;

        PluginNavigationPosition? player = _player();
        _lastPlayer = player;
        if (player is { } at)
        {
            // Measured against the view as it was, before any re-centring,
            // so a step while following still reads as a change.
            (double x, double y) = View.ToCanvas(at.EastWest, at.NorthSouth);
            if (View.Follow)
                View.CenterOn(at.EastWest, at.NorthSouth);
            int px = (int)x;
            int py = (int)y;
            if (px != _lastPlayerX || py != _lastPlayerY)
            {
                _lastPlayerX = px;
                _lastPlayerY = py;
                _dirty = _dirty || View.Follow || View.Contains(px, py);
            }
            if (at.HeadingDegrees != _lastHeading)
            {
                _lastHeading = at.HeadingDegrees;
                _dirty = true;
            }
        }
        if (_markers.Revision != _seenMarkerRevision)
        {
            _seenMarkerRevision = _markers.Revision;
            _dirty = true;
        }
        if (!_dirty)
            return;
        _dirty = false;
        InvalidateCount++;
        _canvas.Invalidate();
    }

    /// <summary>
    /// The session ended: the interface's images are gone with it, so the
    /// handles are dropped and asked for again on the next paint.
    /// </summary>
    public void Reset()
    {
        _images.Clear();
        _lastPlayerX = int.MinValue;
        _lastPlayerY = int.MinValue;
        _lastHeading = float.NaN;
        _dirty = true;
    }

    public void Dispose()
    {
        foreach (PluginImage image in _images.Values)
            _ui.Images.Release(image);
        _images.Clear();
        _dragFrom = null;
        _pointer = null;
        _canvas?.Dispose();
        _canvas = null;
    }

    /// <summary>
    /// The pointer over the canvas: a left press starts a drag and puts the
    /// readout under the cursor, each held move slides the map by its delta,
    /// the release or a cancel ends the drag and returns the readout to the
    /// middle, and the wheel zooms about the cursor. Each marks the map
    /// dirty only when what it draws changed.
    /// </summary>
    private void OnPointer(PluginPointerEvent pointerEvent)
    {
        switch (pointerEvent.Kind)
        {
            case PluginPointerEventKind.Down when pointerEvent.Button == PluginPointerButton.Left:
                _dragFrom = pointerEvent.Position;
                MovePointer(pointerEvent.Position);
                break;
            case PluginPointerEventKind.Move when _dragFrom is { } from:
                double dx = pointerEvent.Position.X - from.X;
                double dy = pointerEvent.Position.Y - from.Y;
                if (dx != 0d || dy != 0d)
                    Pan(dx, dy);
                _dragFrom = pointerEvent.Position;
                MovePointer(pointerEvent.Position);
                break;
            case PluginPointerEventKind.Up:
            case PluginPointerEventKind.Cancelled:
                _dragFrom = null;
                MovePointer(null);
                break;
            case PluginPointerEventKind.Wheel:
                ZoomAbout(pointerEvent.Position.X, pointerEvent.Position.Y, pointerEvent.WheelDelta * LandscapeMapView.ZoomStep);
                break;
        }
    }

    /// <summary>Moves the readout's point; a repaint only when the text it shows changed.</summary>
    private void MovePointer(PluginPoint? at)
    {
        string before = Readout;
        _pointer = at;
        if (Readout != before)
            _dirty = true;
    }

    /// <summary>
    /// Paints the whole map: background, the map art or a grid in its
    /// place, the markers and their names, the rulers, the character, and
    /// the readout. Called by the host on its frame after an invalidate.
    /// </summary>
    internal void Paint(IPluginPainter painter)
    {
        painter.Clear(Background with { A = (byte)Math.Clamp(_settings.Opacity, 0, 255) });

        PluginImage map = Image(MapArtSurfaceId);
        if (map.IsValid && (View.ImageWidth != map.Width || View.ImageHeight != map.Height))
            AdoptImageSize(map.Width, map.Height);

        painter.PushClip(new PluginRect(0d, 0d, CanvasWidth, CanvasHeight));
        if (map.IsValid)
            painter.DrawImage(
                map,
                new PluginRect(View.CanvasLeft, View.CanvasTop, View.ImageWidth * View.Scale, View.ImageHeight * View.Scale),
                PluginColor.White);
        else
            PaintGrid(painter);
        PaintMarkers(painter);
        painter.PopClip();

        PaintRulers(painter);
        if (_lastPlayer is { } player)
            PaintPlayer(painter, player);
        string readout = Readout;
        PluginSize size = painter.MeasureText(readout);
        painter.DrawText(
            readout,
            new PluginPoint((CanvasWidth - size.Width) / 2d, CanvasHeight - GutterWidth - size.Height - 4d),
            LabelColor,
            true);
    }

    private void AdoptImageSize(int width, int height)
    {
        (double eastWest, double northSouth) = View.ToWorld(CanvasWidth / 2d, CanvasHeight / 2d);
        var next = new LandscapeMapView(CanvasWidth, CanvasHeight, width, height)
        {
            Zoom = View.Zoom,
            Follow = View.Follow,
        };
        next.CenterOn(eastWest, northSouth);
        View = next;
    }

    private PluginImage Image(uint surfaceId)
    {
        if (_images.TryGetValue(surfaceId, out PluginImage held) && held.IsValid)
            return held;
        PluginImage image = _ui.Images.FromClientArt(surfaceId);
        if (image.IsValid)
            _images[surfaceId] = image;
        return image;
    }

    /// <summary>The grid drawn in place of the map art, on the same steps as the rulers.</summary>
    private void PaintGrid(IPluginPainter painter)
    {
        double increment = View.HorizontalIncrement(painter.MeasureText);
        double minEast = View.ToWorld(0d, 0d).EastWest;
        double maxEast = View.ToWorld(CanvasWidth, 0d).EastWest;
        for (double c = Math.Floor(minEast / increment) * increment; c <= maxEast; c += increment)
        {
            double x = View.ToCanvas(c, 0d).X;
            painter.DrawLine(new PluginPoint(x, 0d), new PluginPoint(x, CanvasHeight), GridLine, 1f);
        }
        increment = View.VerticalIncrement(painter.MeasureText);
        double maxNorth = View.ToWorld(0d, 0d).NorthSouth;
        double minNorth = View.ToWorld(0d, CanvasHeight).NorthSouth;
        for (double c = Math.Floor(minNorth / increment) * increment; c <= maxNorth; c += increment)
        {
            double y = View.ToCanvas(0d, c).Y;
            painter.DrawLine(new PluginPoint(0d, y), new PluginPoint(CanvasWidth, y), GridLine, 1f);
        }
    }

    private void PaintMarkers(IPluginPainter painter)
    {
        _placer.Reset();
        double zoom = View.Zoom;
        var labelled = new List<(LandscapeMarker Marker, double X, double Y)>();
        foreach (LandscapeMarker marker in _markers.Markers)
        {
            LandscapeMarkerDisplay display = LandscapeMarkerCatalog.DisplayOf(marker.Kind);
            if (!display.ShowsMarkerAt(zoom))
                continue;
            (double x, double y) = View.ToCanvas(marker.EastWest, marker.NorthSouth);
            if (!View.Contains(x, y))
                continue;
            var iconRect = new PluginRect(x - (IconSize / 2d), y - (IconSize / 2d), IconSize, IconSize);
            PluginImage icon = Image(display.IconSurfaceId);
            if (icon.IsValid)
                painter.DrawImage(icon, iconRect, PluginColor.White);
            else
                painter.StrokeRect(iconRect, LabelColor, 1f);
            if (display.ShowsLabelAt(zoom))
            {
                _placer.Occupy(iconRect);
                labelled.Add((marker, x, y));
            }
        }
        foreach ((LandscapeMarker marker, double x, double y) in labelled)
        {
            PluginSize size = painter.MeasureText(marker.Name);
            if (_placer.TryPlace(x, y, size.Width, size.Height, out PluginRect placed))
                painter.DrawText(marker.Name, new PluginPoint(placed.X + 2d, placed.Y), LabelColor, true);
        }
    }

    private void PaintRulers(IPluginPainter painter)
    {
        painter.FillRect(new PluginRect(0d, 0d, CanvasWidth, GutterWidth), GutterFill);
        painter.FillRect(new PluginRect(0d, CanvasHeight - GutterWidth, CanvasWidth, GutterWidth), GutterFill);
        painter.FillRect(new PluginRect(0d, GutterWidth, GutterWidth, CanvasHeight - (2 * GutterWidth)), GutterFill);
        painter.FillRect(new PluginRect(CanvasWidth - GutterWidth, GutterWidth, GutterWidth, CanvasHeight - (2 * GutterWidth)), GutterFill);

        foreach (MapRulerTick tick in View.HorizontalTicks(painter.MeasureText))
        {
            painter.DrawLine(new PluginPoint(tick.Pixel, 0d), new PluginPoint(tick.Pixel, 4d), TickColor, 1f);
            painter.DrawLine(
                new PluginPoint(tick.Pixel, CanvasHeight - 4d),
                new PluginPoint(tick.Pixel, CanvasHeight),
                TickColor,
                1f);
            PluginSize size = painter.MeasureText(tick.Label);
            painter.DrawText(tick.Label, new PluginPoint(tick.Pixel - (size.Width / 2d), 4d), TickText, false);
            painter.DrawText(
                tick.Label,
                new PluginPoint(tick.Pixel - (size.Width / 2d), CanvasHeight - GutterWidth + 2d),
                TickText,
                false);
        }
        foreach (MapRulerTick tick in View.VerticalTicks(painter.MeasureText))
        {
            if (tick.Pixel < GutterWidth || tick.Pixel > CanvasHeight - GutterWidth)
                continue;
            painter.DrawLine(new PluginPoint(0d, tick.Pixel), new PluginPoint(4d, tick.Pixel), TickColor, 1f);
            painter.DrawLine(
                new PluginPoint(CanvasWidth - 4d, tick.Pixel),
                new PluginPoint(CanvasWidth, tick.Pixel),
                TickColor,
                1f);
            PluginSize size = painter.MeasureText(tick.Label);
            painter.DrawText(tick.Label, new PluginPoint(4d, tick.Pixel - (size.Height / 2d)), TickText, false);
            painter.DrawText(
                tick.Label,
                new PluginPoint(CanvasWidth - size.Width - 4d, tick.Pixel - (size.Height / 2d)),
                TickText,
                false);
        }
    }

    /// <summary>
    /// The character as an arrow pointing the way they face. Heading is
    /// degrees clockwise from north and the canvas's y runs down, so north
    /// is straight up on the canvas.
    /// </summary>
    private void PaintPlayer(IPluginPainter painter, PluginNavigationPosition player)
    {
        (double x, double y) = View.Follow
            ? (CanvasWidth / 2d, CanvasHeight / 2d)
            : View.ToCanvas(player.EastWest, player.NorthSouth);
        if (!View.Contains(x, y))
            return;
        double heading = player.HeadingDegrees * Math.PI / 180d;
        PluginPoint tip = Along(x, y, heading, 8d);
        PluginPoint left = Along(x, y, heading + (Math.PI * 0.78d), 7d);
        PluginPoint right = Along(x, y, heading - (Math.PI * 0.78d), 7d);
        painter.DrawLine(tip, left, PlayerColor, 2f);
        painter.DrawLine(tip, right, PlayerColor, 2f);
        painter.DrawLine(left, right, PlayerColor, 2f);
    }

    private static PluginPoint Along(double x, double y, double headingRadians, double distance) =>
        new(x + (Math.Sin(headingRadians) * distance), y - (Math.Cos(headingRadians) * distance));
}

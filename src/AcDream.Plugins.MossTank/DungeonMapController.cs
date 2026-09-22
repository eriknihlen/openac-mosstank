using System.Globalization;
using System.Numerics;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>One drawn part of the map: its switch and its colour, as #AARRGGBB.</summary>
internal readonly record struct DungeonMapPart(bool Enabled, uint Color);

/// <summary>The eleven marker rows, one style each.</summary>
internal readonly record struct DungeonMarkerStyles(
    DungeonMarkerStyle You,
    DungeonMarkerStyle Others,
    DungeonMarkerStyle Items,
    DungeonMarkerStyle Monsters,
    DungeonMarkerStyle NPCs,
    DungeonMarkerStyle MyCorpse,
    DungeonMarkerStyle OtherCorpses,
    DungeonMarkerStyle Portals,
    DungeonMarkerStyle Containers,
    DungeonMarkerStyle Doors,
    DungeonMarkerStyle EverythingElse)
{
    public DungeonMarkerStyle this[DungeonMarkerKind kind] => kind switch
    {
        DungeonMarkerKind.You => You,
        DungeonMarkerKind.Others => Others,
        DungeonMarkerKind.Items => Items,
        DungeonMarkerKind.Monsters => Monsters,
        DungeonMarkerKind.NPCs => NPCs,
        DungeonMarkerKind.MyCorpse => MyCorpse,
        DungeonMarkerKind.OtherCorpses => OtherCorpses,
        DungeonMarkerKind.Portals => Portals,
        DungeonMarkerKind.Containers => Containers,
        DungeonMarkerKind.Doors => Doors,
        _ => EverythingElse,
    };

    /// <summary>The shipped values of the marker rows.</summary>
    public static DungeonMarkerStyles Default => new(
        new(true, false, false, 0xFFFF0000u, 3),
        new(true, true, true, 0xFFFFFFFFu, 3),
        new(true, true, true, 0xFFFFFFFFu, 3),
        new(true, true, false, 0xFFFFA500u, 3),
        new(true, true, false, 0xFFFFFF00u, 3),
        new(true, true, true, 0xFFFF0000u, 3),
        new(false, true, false, 0xFFF5F5F5u, 3),
        new(true, true, true, 0xFFFFF0FFu, 3),
        new(true, true, false, 0xFFF4A460u, 3),
        new(true, false, false, 0xFFA52A2Au, 3),
        new(false, true, false, 0xFFF5F5F5u, 3));
}

/// <summary>
/// The rows on the UB page the dungeon map reads, as one value so a
/// change to any of them is one comparison.
/// </summary>
internal sealed record DungeonMapSettings(
    bool Enabled,
    bool DrawWhenClosed,
    bool ShowVisitedTiles,
    uint VisitedTilesColor,
    bool ShowCompass,
    int Opacity,
    float MapZoom,
    bool Debug,
    DungeonMapPart DungeonName,
    DungeonMapPart Walls,
    DungeonMapPart Floors,
    DungeonMarkerStyles Markers)
{
    /// <summary>The shipped values of the rows.</summary>
    public static DungeonMapSettings Default => new(
        true,
        true,
        true,
        0xFFFF96FFu,
        true,
        16,
        4.2f,
        false,
        new DungeonMapPart(true, 0xFFFFFFFFu),
        new DungeonMapPart(true, 0xFF00007Fu),
        new DungeonMapPart(true, 0xFF007FBFu),
        DungeonMarkerStyles.Default);
}

/// <summary>
/// The dungeon map: the floor plan of the sealed dungeon the character
/// stands in, on a canvas, turned with their heading, with the things in
/// it marked and the cells they have walked shaded.
/// </summary>
/// <remarks>
/// <para>
/// The canvas is retained, so the map asks for a repaint only when
/// something it draws changed: the landblock, which storeys are drawn and
/// how dim, the camera by at least a screen pixel or a degree, a marker
/// placed or moved past its threshold, a newly walked cell, or a row on
/// the page. The world is read thirty times a second at most, as the
/// reference did; a frame in between costs nothing.
/// </para>
/// <para>
/// The canvas takes the pointer: a drag pans, a shift-drag turns, and the
/// wheel zooms about the cursor, each through the same calls the control
/// strip makes with fixed steps. A drag lets go of the player, as the
/// reference's does; a press the host cancels ends the drag where it is.
/// </para>
/// </remarks>
internal sealed class DungeonMapController : IDisposable
{
    public const string CanvasId = "ub-dungeon";
    public const int CanvasWidth = 320;
    public const int CanvasHeight = 280;
    public const int PanStepPixels = 40;
    public const double RotateStepDegrees = 15d;

    /// <summary>How often the page's rows are re-read for a change.</summary>
    internal const double SettingsPollSeconds = 0.25d;

    /// <summary>The least time between two reads of the world: thirty a second.</summary>
    internal const double WorldReadSeconds = 1d / 30d;

    private const double CompassRadius = 12d;
    private const double CompassInset = 18d;

    private readonly IUiRegistry _ui;
    private readonly Func<DungeonMapSettings> _readSettings;
    private readonly Action<float> _saveMapZoom;
    private readonly Func<PluginNavigationSnapshot?> _player;
    private readonly IDungeonMapAutomation _dungeon;
    private readonly Func<IReadOnlyList<PluginWorldObject>> _objects;
    private readonly Func<(uint Id, string Name)> _self;
    private readonly DungeonObjectTracker _tracker = new();
    private readonly DungeonVisitedCells _visited;
    private readonly Dictionary<uint, PluginImage> _icons = [];
    private readonly List<(DungeonTrackedObject Object, Vector2 At, DungeonMarkerStyle Style, double Radius)> _labelled = [];

    private IPluginCanvas? _canvas;
    private DungeonMapSettings _settings;
    private double _settingsPollRemaining;
    private double _worldReadRemaining;
    private bool _shown;
    private bool _dirty = true;
    private bool _inDungeon;
    private PluginDungeonFloorplan _plan = PluginDungeonFloorplan.Empty;
    private DungeonMapPicture _picture = DungeonMapPicture.Empty;
    private uint _landblockId;
    private double _drawZ;
    private int _heightPercent;
    private bool _showAllLayers;
    private float _appliedMapZoom;
    private (int X, int Y, int Rotation, float Scale) _cameraSignature = (int.MinValue, int.MinValue, 0, 0f);
    private int _shadeSignature;
    private PluginPoint? _dragFrom;

    public DungeonMapController(
        IUiRegistry ui,
        IPluginStorage storage,
        Func<DungeonMapSettings> readSettings,
        Action<float> saveMapZoom,
        Func<PluginNavigationSnapshot?> player,
        IDungeonMapAutomation dungeon,
        Func<IReadOnlyList<PluginWorldObject>> objects,
        Func<(uint Id, string Name)> self)
    {
        _ui = ui ?? throw new ArgumentNullException(nameof(ui));
        _readSettings = readSettings ?? throw new ArgumentNullException(nameof(readSettings));
        _saveMapZoom = saveMapZoom ?? throw new ArgumentNullException(nameof(saveMapZoom));
        _player = player ?? throw new ArgumentNullException(nameof(player));
        _dungeon = dungeon ?? throw new ArgumentNullException(nameof(dungeon));
        _objects = objects ?? throw new ArgumentNullException(nameof(objects));
        _self = self ?? throw new ArgumentNullException(nameof(self));
        _visited = new DungeonVisitedCells(storage);
        _settings = readSettings();
        _appliedMapZoom = _settings.MapZoom;
        Camera.Scale = DungeonMapCamera.ScaleFromSetting(_settings.MapZoom);
    }

    /// <summary>Where the map looks from; the strip and the paint both go through it.</summary>
    public DungeonMapCamera Camera { get; } = new();

    /// <summary>Whether the map's own window is open. With draw-when-closed off, the canvas shows only while it is.</summary>
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

    /// <summary>Draw every storey plainly, however far from the player.</summary>
    public bool ShowAllLayers
    {
        get => _showAllLayers;
        set
        {
            if (_showAllLayers == value)
                return;
            _showAllLayers = value;
            _dirty = true;
        }
    }

    /// <summary>The height the map is drawn from, in metres: the player's while following, else the slider's.</summary>
    public double DrawZ => _drawZ;

    /// <summary>The height slider's position, 0 at the lowest storey to 100 at the highest.</summary>
    public int HeightPercent => _heightPercent;

    /// <summary>How many times the host has been asked to repaint.</summary>
    internal int InvalidateCount { get; private set; }

    public string FollowText => Camera.Following ? "Follow: on" : "Follow: off";

    public string LayersText => _showAllLayers ? "Layers: all" : "Layers: near";

    /// <summary>Which dungeon the map is of and how much of it has been walked, or that it is nowhere.</summary>
    public string Status => _inDungeon && !_picture.IsEmpty
        ? string.Create(
            CultureInfo.InvariantCulture,
            $"Dungeon {LandblockName(_landblockId)}: {_picture.Layers.Count} storeys, {_visited.Count} of {_picture.Cells.Count} cells walked.")
        : "Not in a dungeon.";

    /// <summary>One zoom step closer, saved to the page's row.</summary>
    public void ZoomIn()
    {
        Camera.ZoomIn();
        SaveZoom();
    }

    /// <summary>One zoom step further out, saved to the page's row.</summary>
    public void ZoomOut()
    {
        Camera.ZoomOut();
        SaveZoom();
    }

    /// <summary>
    /// Zooms so many steps about a canvas point, the way the wheel does,
    /// saved to the page's row. A step past the limit changes nothing.
    /// </summary>
    public void ZoomAbout(double x, double y, int steps)
    {
        float before = Camera.Scale;
        Camera.ZoomAbout(x, y, steps, CanvasWidth, CanvasHeight);
        if (Camera.Scale != before)
            SaveZoom();
    }

    /// <summary>Slides the map by a number of canvas pixels, the way a drag does.</summary>
    public void Pan(double dxPixels, double dyPixels)
    {
        Camera.PanBy(dxPixels, dyPixels);
        _dirty = true;
    }

    /// <summary>Turns the map by a drag's pixels, the way a shift-drag does.</summary>
    public void RotateByDrag(double dxPixels, double dyPixels)
    {
        Camera.RotateByDrag(dxPixels, dyPixels);
        _dirty = true;
    }

    /// <summary>Turns the map by so many degrees clockwise.</summary>
    public void Rotate(double degrees)
    {
        Camera.RotateBy(degrees);
        _dirty = true;
    }

    /// <summary>
    /// The height slider: picks the storey drawn and lets go of the player,
    /// as the reference's did.
    /// </summary>
    public void SetHeightPercent(int percent)
    {
        _heightPercent = Math.Clamp(percent, 0, 100);
        Camera.StopFollowing();
        _drawZ = DungeonLayerShade.DrawZFromSlider(_picture.MinZ, _picture.MaxZ, _heightPercent);
        _dirty = true;
    }

    /// <summary>Follow on: back onto the player and their storey. Follow off: stay put.</summary>
    public void ToggleFollow()
    {
        if (Camera.Following)
            Camera.StopFollowing();
        else
            Camera.ResumeFollowing();
        _dirty = true;
    }

    /// <summary>
    /// The frame: re-read the page now and then, read the world thirty
    /// times a second at most, and ask for a repaint only when something
    /// drawn changed.
    /// </summary>
    public void OnTick(double elapsedSeconds)
    {
        elapsedSeconds = Math.Max(0d, elapsedSeconds);
        _settingsPollRemaining -= elapsedSeconds;
        if (_settingsPollRemaining <= 0d)
        {
            _settingsPollRemaining = SettingsPollSeconds;
            DungeonMapSettings next = _readSettings();
            if (next != _settings)
            {
                _settings = next;
                _dirty = true;
            }
            if (Math.Abs(_settings.MapZoom - _appliedMapZoom) > 0.001f)
            {
                _appliedMapZoom = _settings.MapZoom;
                Camera.Scale = DungeonMapCamera.ScaleFromSetting(_settings.MapZoom);
            }
        }

        _worldReadRemaining -= elapsedSeconds;
        if (_worldReadRemaining > 0d && !_dirty)
            return;
        _worldReadRemaining = WorldReadSeconds;

        PluginNavigationSnapshot? player = _player();
        _inDungeon = _settings.Enabled
            && player is { IsAvailable: true } snapshot
            && _dungeon.IsSealedDungeon(snapshot.Position.CellId);
        if (_inDungeon)
            EnterLandblock(player!.Value.Position.CellId & 0xFFFF0000u);

        bool visible = _inDungeon && !_picture.IsEmpty && (_settings.DrawWhenClosed || _shown);
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

        PluginNavigationPosition position = player!.Value.Position;
        Vector3 local = PluginDungeonFloorplan.ToLandblockLocal(in position);
        if (Camera.Following)
        {
            Camera.Follow(new Vector2(local.X, local.Y), position.HeadingDegrees);
            _drawZ = local.Z;
            _heightPercent = DungeonLayerShade.SliderFromDrawZ(_picture.MinZ, _picture.MaxZ, _drawZ);
        }

        if (_visited.Mark(position.CellId))
            _dirty = true;

        (uint selfId, string selfName) = _self();
        if (_tracker.Update(_objects(), _plan, position, selfId, selfName))
            _dirty = true;

        int shadeSignature = ShadeSignature();
        if (shadeSignature != _shadeSignature)
        {
            _shadeSignature = shadeSignature;
            _dirty = true;
        }

        // Quantised to whole screen pixels and degrees: a step too small to
        // move the picture is not a change.
        Vector2 center = _picture.MapPixelFromLocal(Camera.CenterMeters) * Camera.Scale;
        (int, int, int, float) signature =
            ((int)Math.Round(center.X), (int)Math.Round(center.Y), (int)Math.Round(Camera.RotationDegrees), Camera.Scale);
        if (signature != _cameraSignature)
        {
            _cameraSignature = signature;
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
        _icons.Clear();
        _tracker.Clear();
        _cameraSignature = (int.MinValue, int.MinValue, 0, 0f);
        _dirty = true;
    }

    public void Dispose()
    {
        ReleaseIcons();
        _dragFrom = null;
        _canvas?.Dispose();
        _canvas = null;
    }

    /// <summary>
    /// The pointer over the canvas: a left press starts a drag, each held
    /// move pans by its delta, or turns by it with shift held, the release
    /// or a cancel ends it, and the wheel zooms about the cursor. The next
    /// tick repaints, since every one of these marks the map dirty.
    /// </summary>
    private void OnPointer(PluginPointerEvent pointerEvent)
    {
        switch (pointerEvent.Kind)
        {
            case PluginPointerEventKind.Down when pointerEvent.Button == PluginPointerButton.Left:
                _dragFrom = pointerEvent.Position;
                break;
            case PluginPointerEventKind.Move when _dragFrom is { } from:
                double dx = pointerEvent.Position.X - from.X;
                double dy = pointerEvent.Position.Y - from.Y;
                if (dx == 0d && dy == 0d)
                    break;
                if ((pointerEvent.Modifiers & PluginKeyModifiers.Shift) != 0)
                    RotateByDrag(dx, dy);
                else
                    Pan(dx, dy);
                _dragFrom = pointerEvent.Position;
                break;
            case PluginPointerEventKind.Up:
            case PluginPointerEventKind.Cancelled:
                _dragFrom = null;
                break;
            case PluginPointerEventKind.Wheel:
                ZoomAbout(pointerEvent.Position.X, pointerEvent.Position.Y, pointerEvent.WheelDelta);
                break;
        }
    }

    private void EnterLandblock(uint landblockId)
    {
        if (landblockId == _landblockId)
            return;
        _landblockId = landblockId;
        _plan = _dungeon.CaptureFloorplan(landblockId);
        _picture = DungeonMapPicture.Build(_plan);
        _tracker.Clear();
        _visited.Bind(landblockId);
        ReleaseIcons();
        _shadeSignature = 0;
        _dirty = true;
    }

    private void SaveZoom()
    {
        _appliedMapZoom = DungeonMapCamera.SettingFromScale(Camera.Scale);
        _saveMapZoom(_appliedMapZoom);
        _dirty = true;
    }

    private void ReleaseIcons()
    {
        foreach (PluginImage icon in _icons.Values)
            _ui.Images.Release(icon);
        _icons.Clear();
    }

    private int ShadeSignature()
    {
        var hash = new HashCode();
        foreach (DungeonMapLayerPicture layer in _picture.Layers)
            hash.Add(DungeonLayerShade.For(_drawZ, layer.Z, _showAllLayers));
        hash.Add((int)Math.Floor((_drawZ + 3d) / 6d));
        return hash.ToHashCode();
    }

    private static string LandblockName(uint landblockId) =>
        ((landblockId >> 16) & 0xFFFFu).ToString("X4", CultureInfo.InvariantCulture);

    // ---- Painting --------------------------------------------------------

    /// <summary>
    /// Paints the whole map: each drawn storey's floor, the shading of the
    /// cells walked, the walls and the marks, lowest storey first so the
    /// nearer ones lie on top; then every label upright, the compass and
    /// the name. Called by the host on its frame after an invalidate.
    /// </summary>
    internal void Paint(IPluginPainter painter)
    {
        painter.Clear(PluginColor.Transparent);
        if (_picture.IsEmpty)
            return;

        float opacity = Math.Clamp(_settings.Opacity / 20f, 0f, 1f);
        double width = painter.Width;
        double height = painter.Height;
        Vector2 centerMapPixel = _picture.MapPixelFromLocal(Camera.CenterMeters);
        _labelled.Clear();

        painter.PushClip(new PluginRect(0d, 0d, width, height));
        foreach (DungeonMapLayerPicture layer in _picture.Layers)
        {
            DungeonLayerShade shade = DungeonLayerShade.For(_drawZ, layer.Z, _showAllLayers);
            if (!shade.Visible)
                continue;
            PaintFloor(painter, layer, shade, opacity, centerMapPixel, width, height);
            PaintWalls(painter, layer, shade, opacity, centerMapPixel, width, height);
            PaintMarks(painter, layer, shade, opacity, width, height);
        }
        painter.PopClip();

        foreach ((DungeonTrackedObject subject, Vector2 at, DungeonMarkerStyle style, double radius) in _labelled)
        {
            PluginSize size = painter.MeasureText(subject.Name);
            painter.DrawText(
                subject.Name,
                new PluginPoint(at.X - (size.Width / 2d), at.Y + radius + 2d),
                Tint(style.Color, 1f, opacity),
                true);
        }

        if (_settings.ShowCompass)
            PaintCompass(painter, opacity, width);
        if (_settings.DungeonName.Enabled)
        {
            string name = string.Create(
                CultureInfo.InvariantCulture,
                $"Dungeon {LandblockName(_landblockId)} (Z:{(int)Math.Floor((_drawZ + 3d) / 6d)})");
            if (_settings.Debug && _player() is { IsAvailable: true } snapshot)
                name += " " + snapshot.Position.CellId.ToString("X8", CultureInfo.InvariantCulture);
            PluginSize size = painter.MeasureText(name);
            painter.DrawText(
                name,
                new PluginPoint((width - size.Width) / 2d, 2d),
                Tint(_settings.DungeonName.Color, 1f, opacity),
                true);
        }
    }

    /// <summary>
    /// Each floor run is a rectangle in map pixels; turned with the camera
    /// it is still a rectangle, which one line of the right thickness
    /// along its middle draws exactly. A run of a walked cell is drawn
    /// again in the visited colour.
    /// </summary>
    private void PaintFloor(
        IPluginPainter painter,
        DungeonMapLayerPicture layer,
        DungeonLayerShade shade,
        float opacity,
        Vector2 center,
        double width,
        double height)
    {
        if (!_settings.Floors.Enabled)
            return;
        PluginColor floor = Tint(_settings.Floors.Color, shade.Brightness, opacity * shade.Opacity);
        PluginColor visited = Tint(_settings.VisitedTilesColor, shade.Brightness, opacity * shade.Opacity);
        foreach (DungeonMapFloorRun run in layer.Floors)
        {
            DungeonMapRect rect = run.Rect;
            float midY = rect.Y + (rect.Height / 2f);
            Vector2 from = Camera.ScreenFromMap(new Vector2(rect.X, midY), center, width, height);
            Vector2 to = Camera.ScreenFromMap(new Vector2(rect.X + rect.Width, midY), center, width, height);
            float thickness = rect.Height * Camera.Scale;
            painter.DrawLine(Point(from), Point(to), floor, thickness);
            if (_settings.ShowVisitedTiles
                && run.CellIndex >= 0
                && _visited.Contains(_picture.Cells[run.CellIndex].CellId))
            {
                painter.DrawLine(Point(from), Point(to), visited, thickness);
            }
        }
    }

    private void PaintWalls(
        IPluginPainter painter,
        DungeonMapLayerPicture layer,
        DungeonLayerShade shade,
        float opacity,
        Vector2 center,
        double width,
        double height)
    {
        if (!_settings.Walls.Enabled)
            return;
        PluginColor color = Tint(_settings.Walls.Color, shade.Brightness, opacity * shade.Opacity);
        float thickness = Math.Max(1f, Camera.Scale);
        foreach (DungeonMapLine wall in layer.Walls)
        {
            Vector2 from = Camera.ScreenFromMap(new Vector2(wall.X1, wall.Y1), center, width, height);
            Vector2 to = Camera.ScreenFromMap(new Vector2(wall.X2, wall.Y2), center, width, height);
            painter.DrawLine(Point(from), Point(to), color, thickness);
        }
    }

    /// <summary>
    /// The marks on one storey: the object's icon when its row asks for one
    /// and the client has it, else a dot in the row's colour. Icons are
    /// drawn upright, so they read at any turn of the map. Labels are put
    /// aside for the pass after every storey, and only for the storey the
    /// map is drawn from unless every storey is shown: the marks on the
    /// storeys above and below are dimmed with their floors, but their
    /// names written at full strength would cloud the storey the player
    /// is on, as they did from the sunken entry of a great hall.
    /// </summary>
    private void PaintMarks(
        IPluginPainter painter,
        DungeonMapLayerPicture layer,
        DungeonLayerShade shade,
        float opacity,
        double width,
        double height)
    {
        bool labelled = _showAllLayers || layer.Z == DungeonLayerShade.BandOf((float)_drawZ);
        foreach (DungeonTrackedObject subject in _tracker.Objects)
        {
            if (!subject.IsPlaced || subject.LayerZ != layer.Z)
                continue;
            DungeonMarkerStyle style = _settings.Markers[subject.Kind];
            if (!style.Enabled)
                continue;
            Vector2 at = Camera.ScreenFromLocal(_picture, subject.Position, width, height);
            double radius = Math.Max(1, style.Size);
            PluginImage icon = style.UseIcon ? Icon(subject.Id) : PluginImage.None;
            if (icon.IsValid)
            {
                double side = style.Size * 32d / 5d;
                radius = side / 2d;
                painter.DrawImage(
                    icon,
                    new PluginRect(at.X - radius, at.Y - radius, side, side),
                    Tint(0xFFFFFFFFu, shade.Brightness, opacity * shade.Opacity));
            }
            else
            {
                PaintDot(painter, at, radius, Tint(style.Color, shade.Brightness, opacity * shade.Opacity));
            }
            if (labelled && style.ShowLabel)
                _labelled.Add((subject, at, style, radius));
        }
    }

    /// <summary>A filled circle, as the rows of a disc: one line per pixel row.</summary>
    private static void PaintDot(IPluginPainter painter, Vector2 at, double radius, PluginColor color)
    {
        int rows = (int)Math.Ceiling(radius);
        for (int row = -rows; row <= rows; row++)
        {
            double half = Math.Sqrt(Math.Max(0d, (radius * radius) - (row * row)));
            if (half <= 0d)
                continue;
            painter.DrawLine(
                new PluginPoint(at.X - half, at.Y + row),
                new PluginPoint(at.X + half, at.Y + row),
                color,
                1f);
        }
    }

    /// <summary>A needle from the top-right corner's circle to north, with an N at its tip.</summary>
    private void PaintCompass(IPluginPainter painter, float opacity, double width)
    {
        var center = new Vector2((float)(width - CompassInset), (float)CompassInset);
        Vector2 north = Camera.NorthOnScreen();
        Vector2 tip = center + (north * (float)CompassRadius);
        PluginColor color = Tint(0xFFFFFFFFu, 1f, opacity);
        painter.DrawLine(Point(center - (north * (float)CompassRadius)), Point(tip), color, 2f);
        PluginSize size = painter.MeasureText("N");
        painter.DrawText("N", new PluginPoint(tip.X - (size.Width / 2d), tip.Y - (size.Height / 2d)), color, true);
    }

    private PluginImage Icon(uint objectId)
    {
        if (_icons.TryGetValue(objectId, out PluginImage held) && held.IsValid)
            return held;
        PluginImage icon = _ui.Images.FromObjectIcon(objectId);
        if (icon.IsValid)
            _icons[objectId] = icon;
        return icon;
    }

    /// <summary>An #AARRGGBB colour multiplied by a brightness and an opacity.</summary>
    internal static PluginColor Tint(uint argb, float brightness, float opacity) => new(
        Channel((argb >> 16) & 0xFFu, brightness),
        Channel((argb >> 8) & 0xFFu, brightness),
        Channel(argb & 0xFFu, brightness),
        Channel((argb >> 24) & 0xFFu, opacity));

    private static byte Channel(uint value, float factor) =>
        (byte)Math.Clamp(Math.Round(value * factor), 0d, 255d);

    private static PluginPoint Point(Vector2 v) => new(v.X, v.Y);
}

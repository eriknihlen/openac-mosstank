using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>The rows on the UB page the clients readout reads.</summary>
/// <param name="Enabled">Whether the readout exists at all.</param>
/// <param name="ShowHudWhenClosed">Whether it stays on screen with its window shut.</param>
/// <param name="SelectedTag">Which tag's characters are listed; "All" lists every one.</param>
/// <param name="TrackedItems">The item names whose counts are shown, one a line.</param>
internal sealed record NetworkHudSettings(
    bool Enabled,
    bool ShowHudWhenClosed,
    string SelectedTag,
    string TrackedItems)
{
    public static NetworkHudSettings Default => new(false, true, "All", string.Empty);

    /// <summary>The tracked item names, trimmed, blanks dropped.</summary>
    public IReadOnlyList<string> TrackedItemNames => TrackedItems
        .Split('\n')
        .Select(static line => line.Trim())
        .Where(static line => line.Length != 0)
        .ToArray();
}

/// <summary>
/// This character as the readout needs it: its identity, its name and its
/// vitals, read from the character surface rather than the network, which
/// reports only the other clients.
/// </summary>
internal readonly record struct NetworkHudSelf(
    uint ObjectId,
    string Name,
    uint CurrentHealth,
    uint MaxHealth,
    uint CurrentStamina,
    uint MaxStamina,
    uint CurrentMana,
    uint MaxMana);

/// <summary>One tracked item on one row: its icon, if the client has one, and its count.</summary>
internal readonly record struct NetworkHudTrackedItem(PluginImage Icon, string Count);

/// <summary>
/// One row of the readout, worked out once per repaint and compared with
/// the last so an unchanged readout is never repainted.
/// </summary>
internal sealed record NetworkHudRow(
    uint ClientId,
    uint PlayerId,
    string Name,
    bool IsSelf,
    double HealthFraction,
    double StaminaFraction,
    double ManaFraction,
    string HealthText,
    string StaminaText,
    string ManaText,
    string DistanceText,
    double ArrowRadians,
    byte DistanceTint,
    IReadOnlyList<NetworkHudTrackedItem> Tracked)
{
    public bool Equals(NetworkHudRow? other) =>
        other is not null
        && ClientId == other.ClientId
        && PlayerId == other.PlayerId
        && Name == other.Name
        && IsSelf == other.IsSelf
        && HealthFraction == other.HealthFraction
        && StaminaFraction == other.StaminaFraction
        && ManaFraction == other.ManaFraction
        && HealthText == other.HealthText
        && StaminaText == other.StaminaText
        && ManaText == other.ManaText
        && DistanceText == other.DistanceText
        && ArrowRadians == other.ArrowRadians
        && DistanceTint == other.DistanceTint
        && Tracked.SequenceEqual(other.Tracked);

    public override int GetHashCode() => HashCode.Combine(ClientId, PlayerId, Name, HealthText);
}

/// <summary>
/// A readout of this character and the others played on this computer:
/// this one first, then one row each
/// with a health bar, half-bars for stamina and mana, the name and the
/// percentages, an arrow to where they stand tinted by how far, the
/// distance, and the counts of the items being tracked.
/// </summary>
/// <remarks>
/// <para>
/// The rows are rebuilt at most twenty times a second and the canvas is
/// repainted only when a row differs from the last time, so a still
/// readout costs a comparison. The canvas takes the pointer so that a
/// shift-click on a row selects that row's character, as the reference
/// does; the row is the one whose <see cref="RowHeight"/> pixels the press
/// lands in. The readout's own window lists the same characters for
/// selecting without the pointer.
/// </para>
/// <para>
/// The network reports the other clients only, so this character's row is
/// built from the character surface: its name and vitals. What another
/// client publishes about itself is its name, world, position, tags and
/// vitals; it does not publish item counts, so the tracked counts are
/// shown for this character's own row, from its own inventory, and the
/// other rows carry none.
/// </para>
/// </remarks>
internal sealed class NetworkHudController : IDisposable
{
    public const string CanvasId = "ub-clients";
    public const int RowHeight = 20;
    public const int HudWidth = 355;
    public const int NameWidth = 180;
    public const int HealthBarHeight = 11;
    public const int TrackedItemWidth = 24;
    public const int RangeWidth = 34;
    public const int Padding = 4;
    public const int MaxRows = 12;
    public const int CanvasHeight = (MaxRows * RowHeight) + 5;

    /// <summary>The least time between two rebuilds of the rows.</summary>
    internal const double RepaintIntervalSeconds = 1d / 20d;

    /// <summary>How often the page's rows are re-read for a change.</summary>
    internal const double SettingsPollSeconds = 0.25d;

    private const string AllTag = "All";

    private static readonly PluginColor HealthBack = new(60, 0, 0, 180);
    private static readonly PluginColor HealthFill = new(255, 0, 0, 180);
    private static readonly PluginColor StaminaBack = new(70, 30, 0, 180);
    private static readonly PluginColor StaminaFill = new(255, 180, 0, 180);
    private static readonly PluginColor ManaBack = new(0, 20, 55, 180);
    private static readonly PluginColor ManaFill = new(0, 80, 255, 180);
    private static readonly PluginColor Edge = new(0, 0, 0, 255);
    private static readonly PluginColor Text = new(255, 255, 255, 255);
    private static readonly PluginColor SmallText = new(0, 0, 0, 255);
    private static readonly PluginColor Unknown = new(128, 128, 128, 255);

    private readonly IUiRegistry _ui;
    private readonly INetworkAutomation _network;
    private readonly IWorldObjectAutomation _objects;
    private readonly ISelectionService _selection;
    private readonly Func<NetworkHudSelf> _self;
    private readonly Func<PluginNavigationPosition?> _selfPosition;
    private readonly Func<NetworkHudSettings> _readSettings;
    private readonly Dictionary<uint, PluginImage> _icons = [];
    private IPluginCanvas? _canvas;
    private NetworkHudSettings _settings;
    private double _settingsPollRemaining;
    private double _repaintRemaining;
    private bool _shown;
    private bool _dirty = true;
    private NetworkHudRow[] _rows = [];

    public NetworkHudController(
        IUiRegistry ui,
        INetworkAutomation network,
        IWorldObjectAutomation objects,
        ISelectionService selection,
        Func<NetworkHudSelf> self,
        Func<PluginNavigationPosition?> selfPosition,
        Func<NetworkHudSettings> readSettings)
    {
        _ui = ui;
        _network = network;
        _objects = objects;
        _selection = selection;
        _self = self;
        _selfPosition = selfPosition;
        _readSettings = readSettings;
        _settings = readSettings();
    }

    /// <summary>The rows as last built: this character, then the others in the order the network reported them.</summary>
    public IReadOnlyList<NetworkHudRow> Rows => _rows;

    /// <summary>Whether the readout's window is open.</summary>
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

    /// <summary>Whether the canvas should be on screen: on, and either open or allowed to stay.</summary>
    public bool ShouldShow => _settings.Enabled && (_shown || _settings.ShowHudWhenClosed);

    public string TagText => $"Tag: {_settings.SelectedTag}";

    /// <summary>
    /// Selects the character on one row, the way a click on the row would.
    /// Returns the name selected, or null when there is no such row.
    /// </summary>
    public string? SelectRow(int index)
    {
        if ((uint)index >= (uint)_rows.Length)
            return null;
        NetworkHudRow row = _rows[index];
        _selection.Select(row.PlayerId);
        return row.Name;
    }

    /// <summary>
    /// The frame: re-read the page now and then, rebuild the rows at most
    /// twenty times a second, and repaint only when a row changed.
    /// </summary>
    public void OnTick(double elapsedSeconds)
    {
        double elapsed = Math.Max(0d, elapsedSeconds);
        _settingsPollRemaining -= elapsed;
        if (_settingsPollRemaining <= 0d)
        {
            _settingsPollRemaining = SettingsPollSeconds;
            NetworkHudSettings next = _readSettings();
            if (next != _settings)
            {
                _settings = next;
                _dirty = true;
            }
        }
        bool visible = ShouldShow;
        if (visible && _canvas is null)
        {
            _canvas = _ui.RegisterCanvas(
                new PluginCanvasDescriptor(CanvasId, HudWidth, CanvasHeight)
                {
                    Anchor = PluginCanvasAnchor.TopLeft,
                    Offset = new PluginPoint(26d, 165d),
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

        _repaintRemaining -= elapsed;
        if (_repaintRemaining > 0d && !_dirty)
            return;
        _repaintRemaining = RepaintIntervalSeconds;
        NetworkHudRow[] rows = BuildRows();
        if (!rows.SequenceEqual(_rows))
        {
            _rows = rows;
            _dirty = true;
        }
        if (!_dirty)
            return;
        _dirty = false;
        _canvas.Invalidate();
    }

    /// <summary>The session ended: the interface's images went with it.</summary>
    public void Reset()
    {
        _icons.Clear();
        _rows = [];
        _dirty = true;
    }

    public void Dispose()
    {
        foreach (PluginImage icon in _icons.Values)
            _ui.Images.Release(icon);
        _icons.Clear();
        _canvas?.Dispose();
        _canvas = null;
    }

    /// <summary>
    /// The pointer over the canvas: a shift-click selects the character on
    /// the row the press lands in; rows are painted one under the other
    /// from the top, each <see cref="RowHeight"/> tall. Nothing drawn
    /// changes, so nothing is repainted.
    /// </summary>
    private void OnPointer(PluginPointerEvent pointerEvent)
    {
        if (pointerEvent.Kind != PluginPointerEventKind.Down
            || pointerEvent.Button != PluginPointerButton.Left
            || (pointerEvent.Modifiers & PluginKeyModifiers.Shift) == 0
            || pointerEvent.Position.Y < 0d)
        {
            return;
        }
        SelectRow((int)(pointerEvent.Position.Y / RowHeight));
    }

    /// <summary>
    /// The rows: this character first, from the character surface, then
    /// the clients the network reports, filtered by the selected tag. The
    /// network reports the other clients only, so the own row is built
    /// here; a client that did report this character's id is the same row
    /// and is not listed twice.
    /// </summary>
    internal NetworkHudRow[] BuildRows()
    {
        string tag = _settings.SelectedTag.Trim();
        NetworkHudSelf me = _self();
        uint self = me.ObjectId;
        PluginNavigationPosition? mine = _selfPosition();
        IReadOnlyList<string> tracked = _settings.TrackedItemNames;
        var rows = new List<NetworkHudRow>();
        if (self != 0u)
            rows.Add(new NetworkHudRow(
                0u,
                self,
                me.Name,
                true,
                Fraction(me.CurrentHealth, me.MaxHealth),
                Fraction(me.CurrentStamina, me.MaxStamina),
                Fraction(me.CurrentMana, me.MaxMana),
                Percent(me.CurrentHealth, me.MaxHealth),
                Percent(me.CurrentStamina, me.MaxStamina),
                Percent(me.CurrentMana, me.MaxMana),
                string.Empty,
                0d,
                255,
                CountTracked(tracked)));
        if (!_network.IsAvailable)
            return [.. rows];
        foreach (PluginNetworkClient client in _network.CaptureClients())
        {
            if (rows.Count >= MaxRows)
                break;
            if (client.PlayerId == self)
                continue;
            // Case-insensitive, as the cast-sharing tag filter is, so the two
            // read one tag list the same way.
            if (tag.Length != 0
                && !tag.Equals(AllTag, StringComparison.OrdinalIgnoreCase)
                && !client.Tags.Contains(tag, StringComparer.OrdinalIgnoreCase))
                continue;
            double distance = double.NaN;
            double arrow = 0d;
            if (mine is { } from)
            {
                distance = from.HorizontalDistanceMeters(client.Position);
                arrow = ArrowRadians(from, client.Position);
            }
            rows.Add(new NetworkHudRow(
                client.ClientId,
                client.PlayerId,
                client.Name,
                false,
                Fraction(client.CurrentHealth, client.MaxHealth),
                Fraction(client.CurrentStamina, client.MaxStamina),
                Fraction(client.CurrentMana, client.MaxMana),
                Percent(client.CurrentHealth, client.MaxHealth),
                Percent(client.CurrentStamina, client.MaxStamina),
                Percent(client.CurrentMana, client.MaxMana),
                double.IsNaN(distance) ? "??" : FormatNumber(distance),
                arrow,
                double.IsNaN(distance) ? (byte)255 : DistanceTint(distance),
                []));
        }
        return [.. rows];
    }

    /// <summary>
    /// Which way an arrow drawn on the screen points at another character:
    /// their compass bearing from this one, less the way this one faces,
    /// in radians clockwise from straight up.
    /// </summary>
    internal static double ArrowRadians(PluginNavigationPosition from, PluginNavigationPosition to)
    {
        double bearing = Math.Atan2(to.EastWest - from.EastWest, to.NorthSouth - from.NorthSouth);
        double facing = from.HeadingDegrees * Math.PI / 180d;
        double relative = bearing - facing;
        while (relative < 0d)
            relative += Math.PI * 2d;
        while (relative >= Math.PI * 2d)
            relative -= Math.PI * 2d;
        return relative;
    }

    /// <summary>White up close, redder with every metre: the green and blue drop two per metre.</summary>
    internal static byte DistanceTint(double meters) =>
        (byte)Math.Clamp(255d - (meters * 2d), 0d, 255d);

    /// <summary>A count or a distance as the readout writes it: whole up to a thousand, then thousands.</summary>
    internal static string FormatNumber(double number) => number switch
    {
        < 1000d => number.ToString("N0", CultureInfo.InvariantCulture),
        < 10000d => (number / 1000d).ToString("N1", CultureInfo.InvariantCulture) + "k",
        _ => (number / 1000d).ToString("N0", CultureInfo.InvariantCulture) + "k",
    };

    private static double Fraction(uint current, uint max) =>
        max == 0u ? 0d : Math.Clamp((double)current / max, 0d, 1d);

    private static string Percent(uint current, uint max) =>
        ((double)current / Math.Max(max, 1u) * 100d).ToString("N0", CultureInfo.InvariantCulture) + "%";

    /// <summary>
    /// This character's own count of each tracked item, by name, summing
    /// stacks, with the icon of the first such item the client holds.
    /// </summary>
    private IReadOnlyList<NetworkHudTrackedItem> CountTracked(IReadOnlyList<string> names)
    {
        if (names.Count == 0 || !_objects.IsAvailable)
            return [];
        var counts = new int[names.Count];
        var firstObject = new uint[names.Count];
        foreach (PluginWorldObject item in _objects.CaptureObjects())
        {
            if (!item.IsOwned)
                continue;
            for (int i = 0; i < names.Count; i++)
            {
                if (!string.Equals(item.Name, names[i], StringComparison.OrdinalIgnoreCase))
                    continue;
                counts[i] += Math.Max(1, item.StackSize);
                if (firstObject[i] == 0u)
                    firstObject[i] = item.ObjectId;
            }
        }
        var tracked = new NetworkHudTrackedItem[names.Count];
        for (int i = 0; i < names.Count; i++)
            tracked[i] = new NetworkHudTrackedItem(
                firstObject[i] == 0u ? PluginImage.None : Icon(firstObject[i]),
                FormatNumber(counts[i]));
        return tracked;
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

    /// <summary>Paints every row: bars, edge, name and percentages, arrow, distance, tracked items.</summary>
    internal void Paint(IPluginPainter painter)
    {
        painter.Clear(PluginColor.Transparent);
        int offset = 0;
        foreach (NetworkHudRow row in _rows)
        {
            PaintBars(painter, offset, row);
            if (!row.IsSelf)
            {
                PaintArrow(painter, offset, row);
                painter.DrawText(
                    row.DistanceText,
                    new PluginPoint(NameWidth + Padding + RowHeight - 4d, offset + 5d),
                    row.DistanceText == "??" ? Unknown : new PluginColor(255, row.DistanceTint, row.DistanceTint, 255),
                    false);
            }
            PaintTracked(painter, offset, row);
            // The own row has no client number: the network gives one to the
            // other clients only.
            painter.DrawText(
                row.IsSelf ? row.Name : $"{row.ClientId} {row.Name}",
                new PluginPoint(2d, offset + 1d),
                Text,
                false);
            PluginSize health = painter.MeasureText(row.HealthText);
            painter.DrawText(row.HealthText, new PluginPoint(NameWidth - health.Width - 2d, offset + 1d), Text, false);
            painter.DrawText(row.StaminaText, new PluginPoint(2d, offset + HealthBarHeight), SmallText, false);
            painter.DrawText(row.ManaText, new PluginPoint(2d + (NameWidth / 2d), offset + HealthBarHeight), SmallText, false);
            offset += RowHeight;
        }
    }

    private static void PaintBars(IPluginPainter painter, int offset, NetworkHudRow row)
    {
        double top = offset + 1d;
        double height = RowHeight - 2d;
        double lower = height - HealthBarHeight;
        painter.FillRect(new PluginRect(1d, top, NameWidth, HealthBarHeight), HealthBack);
        painter.FillRect(new PluginRect(1d, top, NameWidth * row.HealthFraction, HealthBarHeight), HealthFill);
        painter.FillRect(new PluginRect(1d, top + HealthBarHeight, NameWidth / 2d, lower), StaminaBack);
        painter.FillRect(new PluginRect(1d, top + HealthBarHeight, NameWidth / 2d * row.StaminaFraction, lower), StaminaFill);
        painter.FillRect(new PluginRect(1d + (NameWidth / 2d), top + HealthBarHeight, NameWidth / 2d, lower), ManaBack);
        painter.FillRect(new PluginRect(1d + (NameWidth / 2d), top + HealthBarHeight, NameWidth / 2d * row.ManaFraction, lower), ManaFill);
        painter.StrokeRect(new PluginRect(1d, top, NameWidth, height), Edge, 1f);
    }

    private static void PaintArrow(IPluginPainter painter, int offset, NetworkHudRow row)
    {
        double size = RowHeight * 0.75d;
        double cx = NameWidth + Padding + (size / 2d);
        double cy = offset + (RowHeight / 2d);
        var tint = new PluginColor(255, row.DistanceTint, row.DistanceTint, 255);
        PluginPoint tip = Along(cx, cy, row.ArrowRadians, size / 2d);
        PluginPoint left = Along(cx, cy, row.ArrowRadians + (Math.PI * 0.78d), size * 0.45d);
        PluginPoint right = Along(cx, cy, row.ArrowRadians - (Math.PI * 0.78d), size * 0.45d);
        painter.DrawLine(tip, left, tint, 2f);
        painter.DrawLine(tip, right, tint, 2f);
        painter.DrawLine(left, right, tint, 2f);
    }

    private static void PaintTracked(IPluginPainter painter, int offset, NetworkHudRow row)
    {
        double x = NameWidth + Padding + RangeWidth;
        int iconSize = RowHeight - 1;
        foreach (NetworkHudTrackedItem item in row.Tracked)
        {
            if (item.Icon.IsValid)
                painter.DrawImage(
                    item.Icon,
                    new PluginRect(x + ((TrackedItemWidth - iconSize) / 2d), offset + 1d, iconSize, iconSize),
                    PluginColor.White);
            PluginSize size = painter.MeasureText(item.Count);
            painter.DrawText(
                item.Count,
                new PluginPoint(x + ((TrackedItemWidth - size.Width) / 2d), offset + RowHeight - 10d),
                Text,
                true);
            x += TrackedItemWidth;
        }
    }

    private static PluginPoint Along(double x, double y, double radians, double distance) =>
        new(x + (Math.Sin(radians) * distance), y - (Math.Cos(radians) * distance));
}

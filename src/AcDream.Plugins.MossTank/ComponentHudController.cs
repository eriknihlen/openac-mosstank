using System.Globalization;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>One item the Comps HUD counts: the name it is counted by and the icon it was added with.</summary>
internal sealed record ComponentHudEntry(string Name, uint IconId);

/// <summary>
/// The Comps HUD, as Virindi HUDs draws it beside VTank: a narrow strip
/// with one row per item the player chose, each row the item's icon and
/// how many of it the character carries, re-counted every 600 ms.
/// </summary>
/// <remarks>
/// <para>
/// The reference is a 62-pixel window whose title bar carries a plus and a
/// minus button. Plus adds the selected object, by name and icon, unless
/// that name is listed already. Minus starts a removal: the counts flash
/// red and dark red, and the next row clicked is taken off; minus again,
/// or 30 seconds, cancels it. The list and the window's place are kept for
/// each character; whether the HUD is on is kept once for every character,
/// as the reference keeps it in its shared settings file.
/// </para>
/// <para>
/// A count is the reference's: every object the character owns, in a pack
/// or worn, whose name is the listed name exactly, adding its stack size,
/// or one for an object that has none.
/// </para>
/// <para>
/// Here the HUD is a canvas the host paints under every window, so the
/// title bar is painted too: a strip <see cref="HeaderHeight"/> tall with
/// the two buttons at its right. A left drag anywhere else moves the HUD,
/// as the window's title bar does in the reference; it moves when the
/// button comes up, as the clients readout does, for the reason given
/// there.
/// </para>
/// </remarks>
internal sealed class ComponentHudController : IDisposable
{
    public const string CanvasId = "comps-hud";
    public const int HudWidth = 62;
    public const int RowHeight = 20;
    public const int IconSize = 20;
    public const int TextX = 22;
    public const int HeaderHeight = 16;
    public const int ButtonSize = 14;

    /// <summary>The height of an empty list, as the reference sizes its window with nothing in it.</summary>
    public const int EmptyListHeight = 16;

    /// <summary>The place a character's HUD starts at, pixels from the screen's top-left.</summary>
    public const int DefaultX = 200;
    public const int DefaultY = 150;

    /// <summary>How often the counts are taken again, as the reference's 600 ms timer does.</summary>
    internal const double RefreshSeconds = 0.6d;

    /// <summary>How long a removal waits for a row to be clicked before it lapses.</summary>
    internal const double RemovalTimeoutSeconds = 30d;

    internal const string EnabledKey = "comps-hud/enabled.txt";
    internal const string CharacterPrefix = "comps-hud/characters/";

    internal const string RemovalPrompt = "Comps HUD: click a row to take it off the list (- again cancels).";
    internal const string RemovalCancelled = "Comps HUD: nothing taken off.";

    private static readonly PluginColor Back = new(0, 0, 0, 150);
    private static readonly PluginColor HeaderBack = new(110, 45, 25, 200);
    private static readonly PluginColor ButtonBack = new(0, 0, 0, 120);
    private static readonly PluginColor Edge = new(0, 0, 0, 255);
    internal static readonly PluginColor Text = new(255, 255, 255, 255);
    internal static readonly PluginColor RemovalBright = new(255, 0, 0, 255);
    internal static readonly PluginColor RemovalDark = new(139, 0, 0, 255);

    private static readonly JsonSerializerOptions JsonOptions = new() { WriteIndented = true };

    private readonly IUiRegistry _ui;
    private readonly IWorldObjectAutomation _objects;
    private readonly ISelectionService _selection;
    private readonly IPluginStorage _storage;
    private readonly Func<(string World, string Character)> _identity;
    private readonly Action<string> _notify;
    private readonly Dictionary<uint, PluginImage> _icons = [];
    private readonly List<ComponentHudEntry> _entries = [];
    private int[] _counts = [];
    private bool _enabled;
    private string? _scope;
    private string? _key;
    private int _x = DefaultX;
    private int _y = DefaultY;
    private IPluginCanvas? _canvas;
    private int _canvasHeight;
    private double _refreshRemaining;
    private bool _dirty = true;

    private bool _removing;
    private double _removalElapsed;

    /// <summary>The bright half of the removal flash; the reference starts on it.</summary>
    private bool _flashBright = true;

    /// <summary>The point of the canvas a left press took hold of, while it is held.</summary>
    private PluginPoint? _grab;

    public ComponentHudController(
        IUiRegistry ui,
        IWorldObjectAutomation objects,
        ISelectionService selection,
        IPluginStorage storage,
        Func<(string World, string Character)> identity,
        Action<string> notify)
    {
        _ui = ui;
        _objects = objects;
        _selection = selection;
        _storage = storage;
        _identity = identity;
        _notify = notify;
        _enabled = _storage.IsAvailable
            && bool.TryParse(_storage.ReadText(EnabledKey)?.Trim(), out bool enabled)
            && enabled;
    }

    /// <summary>Whether the HUD is on, for every character; kept across sessions.</summary>
    public bool Enabled
    {
        get => _enabled;
        set
        {
            if (_enabled == value)
                return;
            _enabled = value;
            _dirty = true;
            if (!value)
                CancelRemoval(quiet: true);
            if (_storage.IsAvailable)
                _storage.WriteText(EnabledKey, value ? "true" : "false");
        }
    }

    /// <summary>Whether a character is logged in for the list to be its.</summary>
    public bool IsBound => _key is not null;

    /// <summary>The listed items, in the order they were added.</summary>
    public IReadOnlyList<ComponentHudEntry> Entries => _entries;

    /// <summary>The counts as last taken, one for each entry.</summary>
    public IReadOnlyList<int> Counts => _counts;

    /// <summary>Whether a removal is waiting for a row to be clicked.</summary>
    public bool Removing => _removing;

    /// <summary>Where the HUD stands, pixels from the screen's top-left.</summary>
    public PluginPoint Position => new(_x, _y);

    /// <summary>The colour every count is written in now: the flash during a removal, white otherwise.</summary>
    public PluginColor CountColor => !_removing ? Text : _flashBright ? RemovalBright : RemovalDark;

    /// <summary>The canvas height the list needs: the title strip, then a row for each entry.</summary>
    public int CanvasHeight => HeaderHeight + (_entries.Count == 0 ? EmptyListHeight : _entries.Count * RowHeight);

    /// <summary>The plus button, in the canvas's pixels.</summary>
    public static PluginRect PlusButton => new(HudWidth - ButtonSize - 1d, 1d, ButtonSize, ButtonSize);

    /// <summary>The minus button, in the canvas's pixels.</summary>
    public static PluginRect MinusButton => new(HudWidth - (2d * ButtonSize) - 3d, 1d, ButtonSize, ButtonSize);

    /// <summary>
    /// The frame: bind the logged-in character's list, keep the canvas
    /// sized to it and shown while the HUD is on, and take the counts
    /// every 600 ms, flashing the removal on the same beat.
    /// </summary>
    public void OnTick(double elapsedSeconds)
    {
        double elapsed = Math.Max(0d, elapsedSeconds);
        Bind();
        bool visible = _enabled && _key is not null;
        if (visible && (_canvas is null || _canvasHeight != CanvasHeight))
            Register();
        if (_canvas is null)
            return;
        if (_canvas.IsVisible != visible)
        {
            _canvas.IsVisible = visible;
            _dirty = true;
        }
        if (!visible)
            return;

        _refreshRemaining -= elapsed;
        if (_removing)
            _removalElapsed += elapsed;
        if (_refreshRemaining <= 0d)
        {
            _refreshRemaining = RefreshSeconds;
            Refresh();
        }
        if (!_dirty)
            return;
        _dirty = false;
        _canvas.Invalidate();
    }

    /// <summary>
    /// Adds the selected object, by its name and icon, as the plus button
    /// does. Returns the name added, or null when nothing is selected, the
    /// client does not know the selection, or the name is listed already.
    /// </summary>
    public string? AddSelection()
    {
        if (_key is null || _selection.SelectedObjectId is not { } selected
            || !_objects.TryGet(selected, out PluginWorldObject item)
            || string.IsNullOrEmpty(item.Name)
            || _entries.Any(entry => entry.Name == item.Name))
        {
            return null;
        }
        _entries.Add(new ComponentHudEntry(item.Name, item.IconId));
        Changed();
        return item.Name;
    }

    /// <summary>Takes one entry off the list. Returns false when there is no such row.</summary>
    public bool RemoveAt(int index)
    {
        if ((uint)index >= (uint)_entries.Count)
            return false;
        _removing = false;
        _entries.RemoveAt(index);
        Changed();
        return true;
    }

    /// <summary>Takes the entry with this name off the list, ignoring case. Returns the name removed, or null.</summary>
    public string? Remove(string name)
    {
        int index = _entries.FindIndex(entry => entry.Name.Equals(name.Trim(), StringComparison.OrdinalIgnoreCase));
        if (index < 0)
            return null;
        string removed = _entries[index].Name;
        RemoveAt(index);
        return removed;
    }

    /// <summary>
    /// What the minus button does: start a removal, saying how to finish
    /// it, or cancel the one under way.
    /// </summary>
    public void ToggleRemoval()
    {
        if (_removing)
        {
            CancelRemoval(quiet: false);
            return;
        }
        _removing = true;
        _removalElapsed = 0d;
        _flashBright = true;
        _dirty = true;
        _notify(RemovalPrompt);
    }

    /// <summary>The session ended: the interface's images went with it, and the next login binds its own list.</summary>
    public void Reset()
    {
        _icons.Clear();
        _grab = null;
        _removing = false;
        _scope = null;
        _key = null;
        _entries.Clear();
        _counts = [];
        _dirty = true;
        if (_canvas is not null)
            _canvas.IsVisible = false;
    }

    public void Dispose()
    {
        foreach (PluginImage icon in _icons.Values)
            _ui.Images.Release(icon);
        _icons.Clear();
        _canvas?.Dispose();
        _canvas = null;
    }

    /// <summary>The storage key a character's list is kept under.</summary>
    internal static string KeyFor(string world, string character)
    {
        string scope = world.Trim().ToUpperInvariant() + "\0" + character.Trim().ToUpperInvariant();
        return CharacterPrefix + Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(scope))) + ".json";
    }

    /// <summary>
    /// How many of an item the character carries: every owned object of
    /// that exact name, adding its stack size, or one for an object that
    /// has none.
    /// </summary>
    internal static int CountOwned(IEnumerable<PluginWorldObject> objects, string name)
    {
        int count = 0;
        foreach (PluginWorldObject item in objects)
        {
            if (item.IsOwned && string.Equals(item.Name, name, StringComparison.Ordinal))
                count += Math.Max(1, item.StackSize);
        }
        return count;
    }

    private void Bind()
    {
        (string world, string character) = _identity();
        if (string.IsNullOrWhiteSpace(world) || string.IsNullOrWhiteSpace(character))
            return;
        string scope = world.Trim().ToUpperInvariant() + "\0" + character.Trim().ToUpperInvariant();
        if (scope == _scope)
            return;
        _scope = scope;
        _key = KeyFor(world, character);
        _entries.Clear();
        _counts = [];
        _x = DefaultX;
        _y = DefaultY;
        _removing = false;
        _dirty = true;
        _refreshRemaining = 0d;
        Load();
        if (_canvas is not null)
            _canvas.Offset = new PluginPoint(_x, _y);
    }

    private void Load()
    {
        if (!_storage.IsAvailable || _key is null)
            return;
        try
        {
            if (_storage.ReadText(_key) is not { } json)
                return;
            Document? document = JsonSerializer.Deserialize<Document>(json, JsonOptions);
            if (document is null)
                return;
            _x = document.X;
            _y = document.Y;
            foreach (ComponentHudEntry entry in document.Entries ?? [])
            {
                if (!string.IsNullOrEmpty(entry.Name) && _entries.All(held => held.Name != entry.Name))
                    _entries.Add(entry);
            }
        }
        catch (JsonException)
        {
            // A damaged file starts an empty list; the next change rewrites it.
            _entries.Clear();
        }
    }

    private void Save()
    {
        if (!_storage.IsAvailable || _key is null)
            return;
        _storage.WriteText(
            _key,
            JsonSerializer.Serialize(new Document { X = _x, Y = _y, Entries = [.. _entries] }, JsonOptions));
    }

    private void Changed()
    {
        Save();
        _refreshRemaining = 0d;
        _dirty = true;
    }

    private void CancelRemoval(bool quiet)
    {
        if (!_removing)
            return;
        _removing = false;
        _dirty = true;
        if (!quiet)
            _notify(RemovalCancelled);
    }

    /// <summary>
    /// Takes the counts again and steps the removal: it lapses after 30
    /// seconds without a click, and flips between its two reds otherwise.
    /// </summary>
    private void Refresh()
    {
        if (_removing && _removalElapsed > RemovalTimeoutSeconds)
        {
            _removing = false;
            _dirty = true;
        }
        if (_removing)
        {
            _flashBright = !_flashBright;
            _dirty = true;
        }
        IReadOnlyList<PluginWorldObject> objects = _objects.IsAvailable ? _objects.CaptureObjects() : [];
        int[] counts = new int[_entries.Count];
        for (int i = 0; i < counts.Length; i++)
            counts[i] = CountOwned(objects, _entries[i].Name);
        if (!counts.SequenceEqual(_counts))
        {
            _counts = counts;
            _dirty = true;
        }
    }

    /// <summary>
    /// Registers the canvas at the list's height; the host sizes a canvas
    /// once, so a list that grew or shrank takes a new one in its place.
    /// </summary>
    private void Register()
    {
        _canvas?.Dispose();
        _canvasHeight = CanvasHeight;
        _canvas = _ui.RegisterCanvas(
            new PluginCanvasDescriptor(CanvasId, HudWidth, _canvasHeight)
            {
                Anchor = PluginCanvasAnchor.TopLeft,
                Offset = new PluginPoint(_x, _y),
                StartVisible = false,
                AcceptsPointerInput = true,
            },
            Paint);
        _canvas.PointerHandler = OnPointer;
        _dirty = true;
    }

    /// <summary>
    /// The pointer: a press on plus or minus is that button; a press on a
    /// row during a removal takes that row off; any other left press
    /// starts a drag that moves the HUD when the button comes up.
    /// </summary>
    private void OnPointer(PluginPointerEvent pointerEvent)
    {
        if (pointerEvent.Button != PluginPointerButton.Left)
            return;
        PluginPoint at = pointerEvent.Position;
        switch (pointerEvent.Kind)
        {
            case PluginPointerEventKind.Down when Contains(PlusButton, at):
                _grab = null;
                AddSelection();
                break;
            case PluginPointerEventKind.Down when Contains(MinusButton, at):
                _grab = null;
                ToggleRemoval();
                break;
            case PluginPointerEventKind.Down when _removing && at.Y >= HeaderHeight:
                _grab = null;
                RemoveAt((int)((at.Y - HeaderHeight) / RowHeight));
                break;
            case PluginPointerEventKind.Down:
                _grab = at;
                break;
            case PluginPointerEventKind.Up when _grab is { } grab:
            {
                _grab = null;
                int dx = WholePixels(at.X - grab.X);
                int dy = WholePixels(at.Y - grab.Y);
                if ((dx != 0 || dy != 0) && _canvas is not null)
                {
                    PluginPoint stood = _canvas.Offset;
                    _x = SaturatingAdd(WholePixels(stood.X), dx);
                    _y = SaturatingAdd(WholePixels(stood.Y), dy);
                    _canvas.Offset = new PluginPoint(_x, _y);
                    Save();
                }
                break;
            }
            case PluginPointerEventKind.Cancelled:
                _grab = null;
                break;
        }
    }

    private static bool Contains(PluginRect rect, PluginPoint point) =>
        point.X >= rect.X && point.X < rect.X + rect.Width
        && point.Y >= rect.Y && point.Y < rect.Y + rect.Height;

    private static int WholePixels(double value) =>
        (int)Math.Clamp(Math.Round(value, MidpointRounding.AwayFromZero), int.MinValue, int.MaxValue);

    private static int SaturatingAdd(int value, int delta) =>
        (int)Math.Clamp((long)value + delta, int.MinValue, int.MaxValue);

    private PluginImage Icon(uint iconId)
    {
        if (iconId == 0u)
            return PluginImage.None;
        if (_icons.TryGetValue(iconId, out PluginImage held) && held.IsValid)
            return held;
        PluginImage icon = _ui.Images.FromClientArt(iconId);
        if (icon.IsValid)
            _icons[iconId] = icon;
        return icon;
    }

    /// <summary>The title strip with its two buttons, then each row's icon and count.</summary>
    internal void Paint(IPluginPainter painter)
    {
        painter.Clear(PluginColor.Transparent);
        int height = CanvasHeight;
        painter.FillRect(new PluginRect(0d, 0d, HudWidth, height), Back);
        painter.FillRect(new PluginRect(0d, 0d, HudWidth, HeaderHeight), HeaderBack);
        PaintButton(painter, MinusButton, plus: false, lit: _removing);
        PaintButton(painter, PlusButton, plus: true, lit: false);

        PluginColor color = CountColor;
        int top = HeaderHeight;
        for (int i = 0; i < _entries.Count; i++)
        {
            PluginImage icon = Icon(_entries[i].IconId);
            if (icon.IsValid)
                painter.DrawImage(icon, new PluginRect(0d, top, IconSize, IconSize), PluginColor.White);
            string count = (i < _counts.Length ? _counts[i] : 0).ToString(CultureInfo.InvariantCulture);
            PluginSize size = painter.MeasureText(count);
            painter.DrawText(count, new PluginPoint(TextX, top + ((RowHeight - size.Height) / 2d)), color, true);
            top += RowHeight;
        }
        painter.StrokeRect(new PluginRect(0d, 0d, HudWidth, height), Edge, 1f);
    }

    private static void PaintButton(IPluginPainter painter, PluginRect button, bool plus, bool lit)
    {
        painter.FillRect(button, lit ? RemovalDark : ButtonBack);
        painter.StrokeRect(button, Edge, 1f);
        double cx = button.X + (button.Width / 2d);
        double cy = button.Y + (button.Height / 2d);
        double arm = (button.Width / 2d) - 3d;
        painter.DrawLine(new PluginPoint(cx - arm, cy), new PluginPoint(cx + arm, cy), Text, 2f);
        if (plus)
            painter.DrawLine(new PluginPoint(cx, cy - arm), new PluginPoint(cx, cy + arm), Text, 2f);
    }

    private sealed class Document
    {
        public int X { get; set; } = DefaultX;
        public int Y { get; set; } = DefaultY;
        public List<ComponentHudEntry>? Entries { get; set; }
    }
}

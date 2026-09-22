using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>What a point of interest on the countryside map is.</summary>
internal enum LandscapeMarkerKind
{
    Unknown,
    Town,
    Lifestone,
    Vendor,
    Npc,
    Portal,
    Outpost,
    Dungeon,
}

/// <summary>
/// How one kind of marker shows: the zoom band its icon draws in, the
/// band its name draws in, and which piece of the client's art is its icon.
/// </summary>
/// <remarks>
/// Towns show at every zoom because they are how the reader finds their
/// way; a vendor only once the map is close enough that its name has room.
/// </remarks>
internal readonly record struct LandscapeMarkerDisplay(
    double MinMarkerZoom,
    double MinLabelZoom,
    uint IconSurfaceId)
{
    public double MaxMarkerZoom => 1d;

    public double MaxLabelZoom => 1d;

    public bool ShowsMarkerAt(double zoom) => zoom >= MinMarkerZoom && zoom <= MaxMarkerZoom;

    public bool ShowsLabelAt(double zoom) => zoom >= MinLabelZoom && zoom <= MaxLabelZoom;
}

/// <summary>One point of interest: what it is, what it is called, and where.</summary>
internal sealed record LandscapeMarker(
    LandscapeMarkerKind Kind,
    string Name,
    double NorthSouth,
    double EastWest);

/// <summary>
/// The points of interest drawn on the countryside map, read from files the
/// user drops into the UB storage tree. The plugin ships no data of its
/// own: portal, lifestone, vendor and town lists are community work that
/// changes with the server, so an empty file with the format written in it
/// is placed where the user will find it.
/// </summary>
/// <remarks>
/// <para>
/// <b>The file format.</b> Every <c>.csv</c> under <c>ub/maps/</c> is
/// read. A line is one marker, four fields separated by commas:
/// <c>kind, name, north-south, east-west</c>. The kind is one of Town,
/// Outpost, Portal, Dungeon, Lifestone, Npc or Vendor, in any case. The
/// coordinates are map units the way the in-game readout shows them, with
/// north and east positive, written as plain numbers (42.1, -33.6). A name
/// may hold a comma if the whole name is in double quotes. Blank lines and
/// lines starting with <c>#</c> are skipped, as is any line that does not
/// parse, so one bad line never loses a file.
/// </para>
/// </remarks>
internal sealed class LandscapeMarkerCatalog
{
    /// <summary>The folder under the UB tree the marker files live in.</summary>
    internal const string Folder = UbSettingStore.Root + "maps/";

    /// <summary>The file written, empty but for the format, when the folder has none.</summary>
    internal const string ShippedFile = Folder + "markers.csv";

    /// <summary>What the shipped file says, so the format travels with the installation.</summary>
    internal const string ShippedFileText =
        "# MossTank countryside map markers.\n"
        + "# One marker a line, four fields: kind, name, north-south, east-west\n"
        + "#   kind: Town, Outpost, Portal, Dungeon, Lifestone, Npc or Vendor\n"
        + "#   coordinates: map units as the in-game readout shows them, north and east positive\n"
        + "#   a name holding a comma goes in double quotes\n"
        + "# Every .csv file in this folder is read. Lines starting with # are skipped.\n"
        + "# Example:\n"
        + "#   Town, Holtburg, 42.1, 33.6\n"
        + "#   Portal, \"Holtburg, Town Portal\", 42.5, 33.9\n";

    private static readonly IReadOnlyDictionary<LandscapeMarkerKind, LandscapeMarkerDisplay> DisplayByKind =
        new Dictionary<LandscapeMarkerKind, LandscapeMarkerDisplay>
        {
            [LandscapeMarkerKind.Unknown] = new(0d, 0d, 0x06003C70u),
            [LandscapeMarkerKind.Town] = new(0d, 0d, 0x06003C7Eu),
            [LandscapeMarkerKind.Outpost] = new(0.1d, 0.25d, 0x06004D70u),
            [LandscapeMarkerKind.Portal] = new(0.1d, 0.32d, 0x06003C6Bu),
            [LandscapeMarkerKind.Dungeon] = new(0.1d, 0.32d, 0x06005B57u),
            [LandscapeMarkerKind.Lifestone] = new(0.24d, 0.4d, 0x060024E1u),
            [LandscapeMarkerKind.Npc] = new(0.4d, 0.8d, 0x06003C36u),
            [LandscapeMarkerKind.Vendor] = new(0.4d, 0.8d, 0x06004E1Fu),
        };

    private readonly List<LandscapeMarker> _markers = [];

    /// <summary>The markers as last loaded, in file order.</summary>
    public IReadOnlyList<LandscapeMarker> Markers => _markers;

    /// <summary>Rises by one on every load that changed the set.</summary>
    public int Revision { get; private set; }

    /// <summary>How a kind of marker shows.</summary>
    public static LandscapeMarkerDisplay DisplayOf(LandscapeMarkerKind kind) => DisplayByKind[kind];

    /// <summary>
    /// Reads every marker file under the folder, writing the shipped file
    /// first when the folder is empty so the format is on disk to be
    /// followed. Returns whether the set changed.
    /// </summary>
    public bool Load(IPluginStorage storage)
    {
        var loaded = new List<LandscapeMarker>();
        if (storage.IsAvailable)
        {
            List<string> files = storage.List(Folder)
                .Where(static key => key.EndsWith(".csv", StringComparison.OrdinalIgnoreCase))
                .OrderBy(static key => key, StringComparer.Ordinal)
                .ToList();
            if (files.Count == 0)
            {
                // The shipped file is written so the format is on disk to be
                // followed, but never over a file that is already there: a
                // listing that comes back empty is not proof that the folder
                // is, and the markers a person put in are not ours to lose.
                if (storage.ReadText(ShippedFile) is null)
                    storage.WriteText(ShippedFile, ShippedFileText);
                files.Add(ShippedFile);
            }
            foreach (string file in files)
                loaded.AddRange(Parse(storage.ReadText(file) ?? string.Empty));
        }
        if (loaded.SequenceEqual(_markers))
            return false;
        _markers.Clear();
        _markers.AddRange(loaded);
        Revision++;
        return true;
    }

    /// <summary>The markers in one file's text; lines that do not parse are skipped.</summary>
    public static IReadOnlyList<LandscapeMarker> Parse(string text)
    {
        var markers = new List<LandscapeMarker>();
        foreach (string raw in text.Split('\n'))
        {
            string line = raw.Trim();
            if (line.Length == 0 || line.StartsWith('#'))
                continue;
            if (TryParseLine(line, out LandscapeMarker? marker))
                markers.Add(marker);
        }
        return markers;
    }

    /// <summary>One line of the file, or false when it is not a marker.</summary>
    public static bool TryParseLine(
        string line,
        [System.Diagnostics.CodeAnalysis.NotNullWhen(true)] out LandscapeMarker? marker)
    {
        marker = null;
        List<string> fields = SplitFields(line);
        if (fields.Count != 4)
            return false;
        if (!Enum.TryParse(fields[0].Trim(), ignoreCase: true, out LandscapeMarkerKind kind)
            || kind == LandscapeMarkerKind.Unknown)
            return false;
        string name = fields[1].Trim();
        if (name.Length == 0)
            return false;
        if (!double.TryParse(fields[2].Trim(), NumberStyles.Float, CultureInfo.InvariantCulture, out double northSouth)
            || !double.TryParse(fields[3].Trim(), NumberStyles.Float, CultureInfo.InvariantCulture, out double eastWest))
            return false;
        if (Math.Abs(northSouth) > LandscapeMapView.MaxCoordinate
            || Math.Abs(eastWest) > LandscapeMapView.MaxCoordinate)
            return false;
        marker = new LandscapeMarker(kind, name, northSouth, eastWest);
        return true;
    }

    private static List<string> SplitFields(string line)
    {
        var fields = new List<string>();
        var field = new System.Text.StringBuilder();
        bool quoted = false;
        foreach (char c in line)
        {
            if (c == '"')
            {
                quoted = !quoted;
                continue;
            }
            if (c == ',' && !quoted)
            {
                fields.Add(field.ToString());
                field.Clear();
                continue;
            }
            field.Append(c);
        }
        fields.Add(field.ToString());
        return fields;
    }
}

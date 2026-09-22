using System.Numerics;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>A rectangle in map pixels, y down.</summary>
internal readonly record struct DungeonMapRect(int X, int Y, int Width, int Height);

/// <summary>A straight line in map pixels, y down.</summary>
internal readonly record struct DungeonMapLine(float X1, float Y1, float X2, float Y2);

/// <summary>
/// One run of floor: a rectangle of map pixels and the cell it belongs to,
/// as an index into <see cref="DungeonMapPicture.Cells"/>, or -1 when the
/// plan carried floor with no cell near it.
/// </summary>
internal readonly record struct DungeonMapFloorRun(DungeonMapRect Rect, int CellIndex);

/// <summary>One storey of the picture: its floor runs and its walls.</summary>
internal sealed class DungeonMapLayerPicture(
    float z,
    IReadOnlyList<DungeonMapFloorRun> floors,
    IReadOnlyList<DungeonMapLine> walls)
{
    /// <summary>The lowest height of the storey's six-metre band, in metres.</summary>
    public float Z { get; } = z;

    /// <summary>The floor, as rectangles that together cover it exactly once.</summary>
    public IReadOnlyList<DungeonMapFloorRun> Floors { get; } = floors;

    /// <summary>The walls, one line each.</summary>
    public IReadOnlyList<DungeonMapLine> Walls { get; } = walls;
}

/// <summary>
/// A floorplan turned into a picture in map pixels: five pixels to the
/// metre, north up, east to the right, the pixel origin at the north-west
/// corner of the plan's bounds. The floor of each storey is rasterised
/// once, at build, into rectangles that each know their cell, so the
/// painter fills rectangles rather than polygons and a visited cell can be
/// tinted without touching the picture again. Colours are not part of the
/// picture: they come from the settings at paint time, so a colour edited
/// on the settings page shows on the next paint.
/// </summary>
/// <remarks>
/// The reference picture mirrors x so that east reads to the left, then
/// turns the whole map half a circle when it draws it. Mirroring one axis
/// and turning by a half circle together are exactly a flip of the other
/// axis: north up. The reference's own label pass, which is not mirrored,
/// confirms it by placing labels at (east, -north). So this picture flips y
/// and nothing else, and the camera turns it by the heading alone.
/// </remarks>
internal sealed class DungeonMapPicture
{
    /// <summary>Map pixels per metre.</summary>
    public const float PixelsPerMeter = 5f;

    private readonly Dictionary<uint, int> _cellIndex;

    private DungeonMapPicture(
        uint landblockId,
        Vector2 originMeters,
        int width,
        int height,
        float minZ,
        float maxZ,
        IReadOnlyList<DungeonMapLayerPicture> layers,
        IReadOnlyList<PluginDungeonCell> cells)
    {
        LandblockId = landblockId;
        OriginMeters = originMeters;
        Width = width;
        Height = height;
        MinZ = minZ;
        MaxZ = maxZ;
        Layers = layers;
        Cells = cells;
        _cellIndex = new Dictionary<uint, int>(cells.Count);
        for (int i = 0; i < cells.Count; i++)
            _cellIndex.TryAdd(cells[i].CellId, i);
    }

    /// <summary>The landblock the picture is of, with a zero low half.</summary>
    public uint LandblockId { get; }

    /// <summary>
    /// The landblock-local point, in metres, at pixel (0, 0): the west edge
    /// and the north edge of the plan's bounds.
    /// </summary>
    public Vector2 OriginMeters { get; }

    /// <summary>The picture's width in map pixels.</summary>
    public int Width { get; }

    /// <summary>The picture's height in map pixels.</summary>
    public int Height { get; }

    /// <summary>The lowest storey's band height, in metres.</summary>
    public float MinZ { get; }

    /// <summary>The highest storey's band height, in metres.</summary>
    public float MaxZ { get; }

    /// <summary>The storeys, lowest first.</summary>
    public IReadOnlyList<DungeonMapLayerPicture> Layers { get; }

    /// <summary>The plan's cells, in the order the floor runs index them.</summary>
    public IReadOnlyList<PluginDungeonCell> Cells { get; }

    /// <summary>True when there is nothing to draw.</summary>
    public bool IsEmpty => Layers.Count == 0;

    /// <summary>The picture of nothing.</summary>
    public static DungeonMapPicture Empty { get; } =
        new(0u, Vector2.Zero, 0, 0, 0f, 0f, [], []);

    /// <summary>Where a landblock-local point falls in the picture, in map pixels.</summary>
    public Vector2 MapPixelFromLocal(Vector3 local) =>
        new((local.X - OriginMeters.X) * PixelsPerMeter, (OriginMeters.Y - local.Y) * PixelsPerMeter);

    /// <summary>Where a landblock-local point falls in the picture, in map pixels.</summary>
    public Vector2 MapPixelFromLocal(Vector2 local) =>
        new((local.X - OriginMeters.X) * PixelsPerMeter, (OriginMeters.Y - local.Y) * PixelsPerMeter);

    /// <summary>The index of a cell in <see cref="Cells"/>, by its full id.</summary>
    public bool TryFindCellIndex(uint cellId, out int index) =>
        _cellIndex.TryGetValue(cellId, out index);

    /// <summary>Rasterises a plan. The plan of nothing is the picture of nothing.</summary>
    public static DungeonMapPicture Build(PluginDungeonFloorplan plan)
    {
        ArgumentNullException.ThrowIfNull(plan);
        if (plan.IsEmpty || plan.Layers.Count == 0)
            return Empty;

        var origin = new Vector2(plan.BoundsMin.X, plan.BoundsMax.Y);
        int width = Math.Max(1, (int)MathF.Ceiling((plan.BoundsMax.X - plan.BoundsMin.X) * PixelsPerMeter));
        int height = Math.Max(1, (int)MathF.Ceiling((plan.BoundsMax.Y - plan.BoundsMin.Y) * PixelsPerMeter));

        var layers = new List<DungeonMapLayerPicture>(plan.Layers.Count);
        float minZ = float.MaxValue;
        float maxZ = float.MinValue;
        int[] owners = new int[width * height];
        foreach (PluginDungeonLayer layer in plan.Layers)
        {
            minZ = Math.Min(minZ, layer.Z);
            maxZ = Math.Max(maxZ, layer.Z);
            layers.Add(BuildLayer(layer, plan.Cells, origin, width, height, owners));
        }
        layers.Sort(static (a, b) => a.Z.CompareTo(b.Z));

        return new DungeonMapPicture(
            plan.LandblockId, origin, width, height, minZ, maxZ, layers, plan.Cells);
    }

    private static DungeonMapLayerPicture BuildLayer(
        PluginDungeonLayer layer,
        IReadOnlyList<PluginDungeonCell> cells,
        Vector2 origin,
        int width,
        int height,
        int[] owners)
    {
        Array.Fill(owners, NoFloor);
        foreach (IReadOnlyList<Vector2> polygon in layer.Floors)
            FillPolygon(polygon, NearestCell(polygon, layer.Z, cells), origin, width, height, owners);

        var walls = new DungeonMapLine[layer.Walls.Count];
        for (int i = 0; i < walls.Length; i++)
        {
            PluginDungeonWall wall = layer.Walls[i];
            Vector2 a = PixelOf(wall.Start, origin);
            Vector2 b = PixelOf(wall.End, origin);
            walls[i] = new DungeonMapLine(a.X, a.Y, b.X, b.Y);
        }

        return new DungeonMapLayerPicture(layer.Z, ExtractRuns(owners, width, height), walls);
    }

    private const int NoFloor = -1;
    private const int FloorWithoutCell = -2;

    private static Vector2 PixelOf(Vector2 local, Vector2 origin) =>
        new((local.X - origin.X) * PixelsPerMeter, (origin.Y - local.Y) * PixelsPerMeter);

    /// <summary>
    /// The cell a floor polygon belongs to: the one on the same storey whose
    /// centre is nearest the polygon's centroid. The plan does not say which
    /// polygon came from which cell, and the nearest centre on the same
    /// storey is right for every cell whose floor is one piece.
    /// </summary>
    private static int NearestCell(
        IReadOnlyList<Vector2> polygon,
        float layerZ,
        IReadOnlyList<PluginDungeonCell> cells)
    {
        if (polygon.Count == 0 || cells.Count == 0)
            return FloorWithoutCell;
        Vector2 centroid = Vector2.Zero;
        foreach (Vector2 vertex in polygon)
            centroid += vertex;
        centroid /= polygon.Count;

        int best = FloorWithoutCell;
        float bestDistance = float.MaxValue;
        for (int pass = 0; pass < 2 && best == FloorWithoutCell; pass++)
        {
            for (int i = 0; i < cells.Count; i++)
            {
                if (pass == 0 && Math.Abs(cells[i].LayerZ - layerZ) > 0.01f)
                    continue;
                float distance = Vector2.DistanceSquared(
                    centroid, new Vector2(cells[i].Center.X, cells[i].Center.Y));
                if (distance < bestDistance)
                {
                    bestDistance = distance;
                    best = i;
                }
            }
        }
        return best;
    }

    /// <summary>
    /// Scanline fill, even-odd, sampled at pixel centres, so two polygons
    /// that share an edge never both own the pixels along it.
    /// </summary>
    private static void FillPolygon(
        IReadOnlyList<Vector2> polygon,
        int owner,
        Vector2 origin,
        int width,
        int height,
        int[] owners)
    {
        int n = polygon.Count;
        if (n < 3)
            return;
        float[] xs = new float[n];
        float[] ys = new float[n];
        float minY = float.MaxValue;
        float maxY = float.MinValue;
        for (int i = 0; i < n; i++)
        {
            Vector2 p = PixelOf(polygon[i], origin);
            xs[i] = p.X;
            ys[i] = p.Y;
            minY = Math.Min(minY, p.Y);
            maxY = Math.Max(maxY, p.Y);
        }

        int rowStart = Math.Max(0, (int)MathF.Floor(minY));
        int rowEnd = Math.Min(height - 1, (int)MathF.Ceiling(maxY));
        var crossings = new List<float>(8);
        for (int row = rowStart; row <= rowEnd; row++)
        {
            float cy = row + 0.5f;
            crossings.Clear();
            for (int i = 0; i < n; i++)
            {
                int j = (i + 1) % n;
                float y0 = ys[i];
                float y1 = ys[j];
                if ((y0 <= cy) == (y1 <= cy))
                    continue;
                crossings.Add(xs[i] + (cy - y0) / (y1 - y0) * (xs[j] - xs[i]));
            }
            if (crossings.Count < 2)
                continue;
            crossings.Sort();
            for (int k = 0; k + 1 < crossings.Count; k += 2)
            {
                int first = Math.Max(0, (int)MathF.Ceiling(crossings[k] - 0.5f));
                int last = Math.Min(width - 1, (int)MathF.Floor(crossings[k + 1] - 0.5f));
                for (int col = first; col <= last; col++)
                    owners[row * width + col] = owner;
            }
        }
    }

    /// <summary>
    /// Turns the owner grid into rectangles: horizontal runs of one owner,
    /// merged downwards while the run beneath is the same width and owner.
    /// A rectangular room becomes one rectangle.
    /// </summary>
    private static IReadOnlyList<DungeonMapFloorRun> ExtractRuns(int[] owners, int width, int height)
    {
        var runs = new List<DungeonMapFloorRun>();
        // The runs that ended on the previous row, keyed by (x, width, owner),
        // as indices into runs; only those may grow by one more row.
        var open = new Dictionary<(int X, int Width, int Owner), int>();
        var next = new Dictionary<(int X, int Width, int Owner), int>();
        for (int row = 0; row < height; row++)
        {
            next.Clear();
            int col = 0;
            while (col < width)
            {
                int owner = owners[row * width + col];
                if (owner == NoFloor)
                {
                    col++;
                    continue;
                }
                int start = col;
                while (col < width && owners[row * width + col] == owner)
                    col++;
                (int, int, int) key = (start, col - start, owner);
                if (open.TryGetValue(key, out int index))
                {
                    DungeonMapFloorRun grown = runs[index];
                    runs[index] = grown with
                    {
                        Rect = grown.Rect with { Height = grown.Rect.Height + 1 },
                    };
                    next[key] = index;
                }
                else
                {
                    runs.Add(new DungeonMapFloorRun(
                        new DungeonMapRect(start, row, col - start, 1),
                        owner >= 0 ? owner : -1));
                    next[key] = runs.Count - 1;
                }
            }
            (open, next) = (next, open);
        }
        return runs;
    }
}

using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// Finds a spot for each marker's name where it covers neither another
/// name nor another marker's icon, trying eight places around the marker in
/// a fixed order and giving up when all eight are taken.
/// </summary>
/// <remarks>
/// The order is above, top right, right, bottom right, below, bottom left,
/// left, top left: the same walk clockwise from the top, so a crowded town
/// reads the same way from one frame to the next. A name that finds no free
/// place is left out rather than drawn over something else.
/// </remarks>
internal sealed class MapLabelPlacer
{
    private readonly List<PluginRect> _occupied = [];

    /// <summary>Every rectangle already claimed this frame.</summary>
    public IReadOnlyList<PluginRect> Occupied => _occupied;

    /// <summary>Starts a new frame with nothing claimed.</summary>
    public void Reset() => _occupied.Clear();

    /// <summary>Claims a rectangle that names must keep clear of, such as an icon.</summary>
    public void Occupy(PluginRect rect) => _occupied.Add(rect);

    /// <summary>
    /// The eight places a name of the given size may go around a marker at
    /// a canvas point, in the order they are tried.
    /// </summary>
    public static PluginRect[] Candidates(double x, double y, double width, double height)
    {
        double w = width;
        double h = height;
        double left = x - (w / 2d);
        double top = y - (h / 2d) - 16d;
        return
        [
            new(left, top, w, h),                          // above
            new(left + 4d + (w / 2d), top + 2d, w, h),     // top right
            new(left + 8d + (w / 2d), top + 15d, w, h),    // right
            new(left + 8d + (w / 2d), top + 26d, w, h),    // bottom right
            new(left, top + 28d, w, h),                    // below
            new(left - 10d - (w / 2d), top + 24d, w, h),   // bottom left
            new(left - 10d - (w / 2d), top + 15d, w, h),   // left
            new(left - 6d - (w / 2d), top + 4d, w, h),     // top left
        ];
    }

    /// <summary>
    /// Claims the first free candidate around a marker for a name of the
    /// given size. False when all eight are taken.
    /// </summary>
    public bool TryPlace(double x, double y, double width, double height, out PluginRect placed)
    {
        foreach (PluginRect candidate in Candidates(x, y, width, height))
        {
            bool blocked = false;
            foreach (PluginRect taken in _occupied)
            {
                if (Intersects(candidate, taken))
                {
                    blocked = true;
                    break;
                }
            }
            if (blocked)
                continue;
            _occupied.Add(candidate);
            placed = candidate;
            return true;
        }
        placed = default;
        return false;
    }

    /// <summary>Whether two rectangles share any area; touching edges do not.</summary>
    public static bool Intersects(PluginRect a, PluginRect b) =>
        a.X < b.X + b.Width && b.X < a.X + a.Width
        && a.Y < b.Y + b.Height && b.Y < a.Y + a.Height;
}

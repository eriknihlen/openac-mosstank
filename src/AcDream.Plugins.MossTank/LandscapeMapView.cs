using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// One tick on a coordinate ruler: where it sits along the gutter, in
/// canvas pixels, and what it says.
/// </summary>
internal readonly record struct MapRulerTick(double Pixel, string Label);

/// <summary>
/// The countryside map's view: which part of the map image the canvas shows
/// and how large. Pure arithmetic over the canvas size, the image size, a
/// zoom in 0..1 and the image pixel that sits at the canvas's centre, so
/// every conversion can be checked without a canvas.
/// </summary>
/// <remarks>
/// <para>
/// The image is the client's own map of the world: a position's fraction
/// across the image is its coordinate over twice <see cref="MaxCoordinate"/>,
/// with north at the top and the top row at <see cref="NorthEdge"/>, one
/// texel short of the west edge's reach, as the client's own rule has it.
/// </para>
/// <para>
/// Zooming is anchored at a point on the canvas: the position under that
/// point before the zoom is the position under it after, which is what
/// makes a zoom about the pointer, or about the middle, feel right. The
/// scale grows geometrically with the zoom so the same step feels the same
/// close up and far out.
/// </para>
/// </remarks>
internal sealed class LandscapeMapView
{
    /// <summary>
    /// How far, in map units, the client's map art reaches from the origin
    /// on each axis: the map page places a marker at a coordinate's tenths
    /// over a 2048-wide span, which is 102.4 units either way.
    /// </summary>
    public const double MaxCoordinate = 102.4d;

    /// <summary>
    /// Where the top row of the image sits, in map units north. The client's
    /// marker rule takes y as (2047 minus the scaled value) over 2048 where
    /// x is the scaled value over 2048, so the north edge is one texel short
    /// of <see cref="MaxCoordinate"/> while the span stays the same: row
    /// zero is 102.3 north and the last row is 102.5 south.
    /// </summary>
    public const double NorthEdge = 102.3d;

    /// <summary>How much one zoom step moves the zoom.</summary>
    public const double ZoomStep = 0.04d;

    /// <summary>The zoom the follow button will not go wider than.</summary>
    public const double FollowZoom = 0.24d;

    private double _zoom = 0.04d;

    public LandscapeMapView(int canvasWidth, int canvasHeight, int imageWidth, int imageHeight)
    {
        CanvasWidth = canvasWidth;
        CanvasHeight = canvasHeight;
        ImageWidth = imageWidth;
        ImageHeight = imageHeight;
        OffsetX = imageWidth / 2d;
        OffsetY = imageHeight / 2d;
    }

    public int CanvasWidth { get; }

    public int CanvasHeight { get; }

    public int ImageWidth { get; }

    public int ImageHeight { get; }

    /// <summary>The image pixel column that sits at the canvas's horizontal centre.</summary>
    public double OffsetX { get; private set; }

    /// <summary>The image pixel row that sits at the canvas's vertical centre.</summary>
    public double OffsetY { get; private set; }

    /// <summary>Whether the view re-centres on the character as they move.</summary>
    public bool Follow { get; set; }

    /// <summary>The zoom, from 0 (the whole world) to 1 (as close as it goes).</summary>
    public double Zoom
    {
        get => _zoom;
        set => _zoom = Math.Clamp(value, 0d, 1d);
    }

    /// <summary>How many canvas pixels one image pixel takes at this zoom.</summary>
    public double Scale => Math.Pow(1.5d, Zoom * 12d) - 0.8d;

    /// <summary>The canvas x of the image's left edge.</summary>
    public double CanvasLeft => (CanvasWidth / 2d) - (OffsetX * Scale);

    /// <summary>The canvas y of the image's top edge.</summary>
    public double CanvasTop => (CanvasHeight / 2d) - (OffsetY * Scale);

    /// <summary>The image pixel a position lands on.</summary>
    public (double X, double Y) ToImage(double eastWest, double northSouth) => (
        (eastWest + MaxCoordinate) / (MaxCoordinate * 2d) * ImageWidth,
        (NorthEdge - northSouth) / (MaxCoordinate * 2d) * ImageHeight);

    /// <summary>The canvas pixel a position lands on, fractional.</summary>
    public (double X, double Y) ToCanvas(double eastWest, double northSouth)
    {
        (double imageX, double imageY) = ToImage(eastWest, northSouth);
        return (CanvasLeft + (imageX * Scale), CanvasTop + (imageY * Scale));
    }

    /// <summary>The position under a canvas pixel.</summary>
    public (double EastWest, double NorthSouth) ToWorld(double x, double y)
    {
        double imageX = (x - CanvasLeft) / Scale;
        double imageY = (y - CanvasTop) / Scale;
        return (
            (imageX / ImageWidth * MaxCoordinate * 2d) - MaxCoordinate,
            NorthEdge - (imageY / ImageHeight * MaxCoordinate * 2d));
    }

    /// <summary>Whether a canvas pixel lies on the canvas.</summary>
    public bool Contains(double x, double y) =>
        x >= 0d && y >= 0d && x < CanvasWidth && y < CanvasHeight;

    /// <summary>Puts a position at the canvas's centre without touching the zoom.</summary>
    public void CenterOn(double eastWest, double northSouth) =>
        (OffsetX, OffsetY) = ToImage(eastWest, northSouth);

    /// <summary>
    /// Changes the zoom so that the position under a canvas point stays
    /// under it. While following the character the view is re-centred on
    /// them anyway, so only the zoom moves.
    /// </summary>
    public void ZoomAbout(double x, double y, double delta)
    {
        (double eastWest, double northSouth) = ToWorld(x, y);
        Zoom += delta;
        if (Follow)
            return;
        (double imageX, double imageY) = ToImage(eastWest, northSouth);
        OffsetX = imageX - ((x - (CanvasWidth / 2d)) / Scale);
        OffsetY = imageY - ((y - (CanvasHeight / 2d)) / Scale);
    }

    /// <summary>
    /// Slides the map by a number of canvas pixels, the way a drag does:
    /// dragging the map right shows what lies to its left. Panning takes
    /// the view away from the character.
    /// </summary>
    public void PanBy(double dxPixels, double dyPixels)
    {
        Follow = false;
        OffsetX -= dxPixels / Scale;
        OffsetY -= dyPixels / Scale;
    }

    /// <summary>The least room, in canvas pixels, left between two neighbouring ruler labels.</summary>
    public const double LabelGap = 6d;

    /// <summary>The ruler steps there are, in map units, widest first.</summary>
    private static readonly double[] Increments = [50d, 20d, 10d, 5d, 2d, 1d, 0.5d, 0.2d, 0.1d];

    /// <summary>
    /// How far apart the ruler ticks are, in map units, at a zoom: wide
    /// steps when the whole world shows, tenths when a town fills the
    /// canvas. This is the step the zoom asks for; each axis widens it as
    /// far as its labels need, see <see cref="HorizontalIncrement"/>.
    /// </summary>
    public static double CoordinateIncrement(double zoom) => zoom switch
    {
        < 0.01d => 50d,
        < 0.05d => 20d,
        < 0.13d => 10d,
        < 0.3d => 5d,
        < 0.5d => 2d,
        < 0.65d => 1d,
        < 0.85d => 0.5d,
        < 0.94d => 0.2d,
        _ => 0.1d,
    };

    /// <summary>The next narrower step, or the step itself at the narrowest.</summary>
    public static double NarrowerIncrement(double increment)
    {
        int at = Array.IndexOf(Increments, increment);
        return at < 0 || at == Increments.Length - 1 ? increment : Increments[at + 1];
    }

    /// <summary>The next wider step, or the step itself at the widest.</summary>
    public static double WiderIncrement(double increment)
    {
        int at = Array.IndexOf(Increments, increment);
        return at <= 0 ? increment : Increments[at - 1];
    }

    /// <summary>How many canvas pixels one map unit takes east to west at this zoom.</summary>
    public double PixelsPerUnitX => ImageWidth / (MaxCoordinate * 2d) * Scale;

    /// <summary>How many canvas pixels one map unit takes north to south at this zoom.</summary>
    public double PixelsPerUnitY => ImageHeight / (MaxCoordinate * 2d) * Scale;

    /// <summary>
    /// The east-west step: the zoom's own, widened until the widest label
    /// it would draw fits between two ticks with a gap to spare. The labels
    /// along the top and bottom gutters are written across the gutter, so
    /// it is their width that has to fit; the north-south labels stack, so
    /// <see cref="VerticalIncrement"/> fits their height instead.
    /// </summary>
    public double HorizontalIncrement(Func<string, PluginSize> measure) =>
        FittingIncrement(
            measure,
            PixelsPerUnitX,
            static (increment, c) => AxisLabel(c, "F1", "W", "E"),
            static size => size.Width,
            ToWorld(0d, 0d).EastWest,
            ToWorld(CanvasWidth, 0d).EastWest);

    /// <summary>The north-south step: the zoom's own, widened until the labels' height fits.</summary>
    public double VerticalIncrement(Func<string, PluginSize> measure) =>
        FittingIncrement(
            measure,
            PixelsPerUnitY,
            static (increment, c) => AxisLabel(c, increment < 1d ? "F1" : "F0", "S", "N"),
            static size => size.Height,
            ToWorld(0d, CanvasHeight).NorthSouth,
            ToWorld(0d, 0d).NorthSouth);

    private double FittingIncrement(
        Func<string, PluginSize> measure,
        double pixelsPerUnit,
        Func<double, double, string> label,
        Func<PluginSize, double> extent,
        double min,
        double max)
    {
        double increment = CoordinateIncrement(Zoom);
        while (true)
        {
            double widest = 0d;
            for (double c = Math.Floor(min / increment) * increment; c <= max; c += increment)
                widest = Math.Max(widest, extent(measure(label(increment, c))));
            double wider = WiderIncrement(increment);
            if (increment * pixelsPerUnit >= widest + LabelGap || wider == increment)
                return increment;
            increment = wider;
        }
    }

    /// <summary>
    /// The ticks along the top and bottom gutters, one per step across the
    /// visible east-west span, each with its coordinate and hemisphere; the
    /// step is <see cref="HorizontalIncrement"/> for the painter's measure.
    /// </summary>
    public IReadOnlyList<MapRulerTick> HorizontalTicks(Func<string, PluginSize> measure)
    {
        double increment = HorizontalIncrement(measure);
        double min = ToWorld(0d, 0d).EastWest;
        double max = ToWorld(CanvasWidth, 0d).EastWest;
        var ticks = new List<MapRulerTick>();
        for (double c = Math.Floor(min / increment) * increment; c <= max; c += increment)
        {
            double pixel = ToCanvas(c, 0d).X;
            if (pixel < 0d || pixel > CanvasWidth)
                continue;
            ticks.Add(new MapRulerTick(pixel, AxisLabel(c, "F1", "W", "E")));
        }
        return ticks;
    }

    /// <summary>
    /// The ticks along the left and right gutters, one per step across the
    /// visible north-south span, the bottom one first; the step is
    /// <see cref="VerticalIncrement"/> for the painter's measure.
    /// </summary>
    public IReadOnlyList<MapRulerTick> VerticalTicks(Func<string, PluginSize> measure)
    {
        double increment = VerticalIncrement(measure);
        double max = ToWorld(0d, 0d).NorthSouth;
        double min = ToWorld(0d, CanvasHeight).NorthSouth;
        var ticks = new List<MapRulerTick>();
        for (double c = Math.Floor(min / increment) * increment; c <= max; c += increment)
        {
            double pixel = ToCanvas(0d, c).Y;
            if (pixel < 0d || pixel > CanvasHeight)
                continue;
            ticks.Add(new MapRulerTick(pixel, AxisLabel(c, increment < 1d ? "F1" : "F0", "S", "N")));
        }
        return ticks;
    }

    /// <summary>A coordinate the way the in-game readout writes it: magnitude then hemisphere.</summary>
    public static string AxisLabel(double value, string format, string negative, string positive)
    {
        // A tick that rounds to zero would otherwise carry a hemisphere the
        // reader cannot see the reason for.
        string magnitude = Math.Abs(value).ToString(format, CultureInfo.InvariantCulture);
        string suffix = value > 0d ? positive : value < 0d ? negative : string.Empty;
        return magnitude + suffix;
    }

    /// <summary>A position the way the in-game readout writes it: north-south first, two decimals.</summary>
    public static string PositionText(double eastWest, double northSouth) =>
        AxisLabel(northSouth, "F2", "S", "N") + ", " + AxisLabel(eastWest, "F2", "W", "E");
}

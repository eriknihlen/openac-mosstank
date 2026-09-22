using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The countryside map's arithmetic: positions to canvas pixels and back,
/// zoom anchored at a point, panning, and the ruler steps.
/// </summary>
public sealed class LandscapeMapViewTests
{
    private static LandscapeMapView View() => new(320, 280, 2048, 2048);

    /// <summary>The test painter's measure: six pixels a character, twelve high.</summary>
    private static PluginSize Measure(string text) => new(text.Length * 6d, 12d);

    /// <summary>
    /// Mutation: flip the north sign in ToImage and the origin still round
    /// trips while a northern position lands in the south.
    /// </summary>
    [Fact]
    public void APositionRoundTripsThroughTheCanvasAndNorthIsUp()
    {
        LandscapeMapView view = View();
        view.Zoom = 0.5d;
        view.CenterOn(33.6d, 42.1d);

        (double x, double y) = view.ToCanvas(33.6d, 42.1d);
        Assert.Equal(160d, x, 6);
        Assert.Equal(140d, y, 6);

        (double eastWest, double northSouth) = view.ToWorld(x, y);
        Assert.Equal(33.6d, eastWest, 6);
        Assert.Equal(42.1d, northSouth, 6);

        (double _, double northY) = view.ToCanvas(33.6d, 60d);
        Assert.True(northY < y, "a position further north draws higher on the canvas");
        (double eastX, double _) = view.ToCanvas(50d, 42.1d);
        Assert.True(eastX > x, "a position further east draws further right");
    }

    /// <summary>
    /// Mutation: drop the offset correction in ZoomAbout and the position
    /// under the anchor drifts.
    /// </summary>
    [Fact]
    public void ZoomingAboutAPointKeepsThePositionUnderIt()
    {
        LandscapeMapView view = View();
        view.Zoom = 0.3d;
        view.CenterOn(10d, -20d);
        (double eastWest, double northSouth) = view.ToWorld(250d, 60d);

        view.ZoomAbout(250d, 60d, LandscapeMapView.ZoomStep);
        (double x, double y) = view.ToCanvas(eastWest, northSouth);
        Assert.Equal(250d, x, 6);
        Assert.Equal(60d, y, 6);

        view.ZoomAbout(250d, 60d, -3 * LandscapeMapView.ZoomStep);
        (x, y) = view.ToCanvas(eastWest, northSouth);
        Assert.Equal(250d, x, 6);
        Assert.Equal(60d, y, 6);
    }

    /// <summary>
    /// Mutation: let the zoom leave 0..1 and the scale goes negative at
    /// the wide end.
    /// </summary>
    [Fact]
    public void TheZoomStaysBetweenZeroAndOne()
    {
        LandscapeMapView view = View();
        view.ZoomAbout(0d, 0d, -5d);
        Assert.Equal(0d, view.Zoom);
        Assert.True(view.Scale > 0d);
        view.ZoomAbout(0d, 0d, 5d);
        Assert.Equal(1d, view.Zoom);
    }

    /// <summary>
    /// A pan of forty pixels moves what is under the middle by forty pixels
    /// and lets go of the character. Mutation: forget to divide by the
    /// scale and the map jumps by forty image pixels instead.
    /// </summary>
    [Fact]
    public void PanningSlidesTheMapByCanvasPixelsAndStopsFollowing()
    {
        LandscapeMapView view = View();
        view.Zoom = 0.5d;
        view.Follow = true;
        view.CenterOn(0d, 0d);
        (double eastWest, double northSouth) = view.ToWorld(160d, 140d);

        view.PanBy(40d, -10d);
        Assert.False(view.Follow);
        (double x, double y) = view.ToCanvas(eastWest, northSouth);
        Assert.Equal(200d, x, 6);
        Assert.Equal(130d, y, 6);
    }

    /// <summary>
    /// Mutation: swap two thresholds and a zoom lands on the wrong step.
    /// </summary>
    [Theory]
    [InlineData(0.0d, 50d)]
    [InlineData(0.04d, 20d)]
    [InlineData(0.1d, 10d)]
    [InlineData(0.24d, 5d)]
    [InlineData(0.4d, 2d)]
    [InlineData(0.6d, 1d)]
    [InlineData(0.8d, 0.5d)]
    [InlineData(0.9d, 0.2d)]
    [InlineData(1.0d, 0.1d)]
    public void TheRulerStepFollowsTheZoom(double zoom, double increment) =>
        Assert.Equal(increment, LandscapeMapView.CoordinateIncrement(zoom));

    /// <summary>
    /// Every tick sits on a multiple of the step, inside the canvas, with
    /// the hemisphere letter after the magnitude. Mutation: start the walk
    /// at the visible minimum instead of its floor and the first tick is
    /// off the grid.
    /// </summary>
    [Fact]
    public void RulerTicksSitOnMultiplesOfTheStepInsideTheCanvas()
    {
        LandscapeMapView view = View();
        view.Zoom = 0.6d;
        view.CenterOn(33.6d, 42.1d);
        double increment = view.HorizontalIncrement(Measure);
        Assert.Equal(LandscapeMapView.CoordinateIncrement(view.Zoom), increment);

        IReadOnlyList<MapRulerTick> horizontal = view.HorizontalTicks(Measure);
        Assert.NotEmpty(horizontal);
        foreach (MapRulerTick tick in horizontal)
        {
            Assert.InRange(tick.Pixel, 0d, 320d);
            double eastWest = view.ToWorld(tick.Pixel, 0d).EastWest;
            Assert.Equal(0d, Math.Abs(eastWest / increment - Math.Round(eastWest / increment)), 6);
            Assert.EndsWith("E", tick.Label);
        }

        IReadOnlyList<MapRulerTick> vertical = view.VerticalTicks(Measure);
        Assert.NotEmpty(vertical);
        foreach (MapRulerTick tick in vertical)
        {
            Assert.InRange(tick.Pixel, 0d, 280d);
            Assert.EndsWith("N", tick.Label);
        }
    }

    /// <summary>
    /// The image's y follows the client's own marker rule: the top row is
    /// 102.3 north, one texel short of the 102.4 the x axis reaches, so a
    /// position at 102.3 north lands on row zero and the south edge at
    /// 102.5 south lands on the last row. Mutation: a 102.4 north edge puts
    /// 102.3 north one texel down. The x rule is unchanged: 102.4 west is
    /// column zero.
    /// </summary>
    [Fact]
    public void RowZeroIsTheClientsNorthEdgeAtOneHundredTwoPointThree()
    {
        LandscapeMapView view = View();

        (double x, double y) = view.ToImage(-102.4d, 102.3d);
        Assert.Equal(0d, x, 6);
        Assert.Equal(0d, y, 6);

        (_, double south) = view.ToImage(0d, -102.5d);
        Assert.Equal(2048d, south, 6);

        (double _, double northSouth) = view.ToWorld(view.CanvasTop, view.CanvasTop);
        Assert.Equal(102.3d, northSouth, 6);
    }

    [Fact]
    public void TheReadoutWritesNorthSouthFirstWithTheHemisphereAfterTheNumber()
    {
        Assert.Equal("42.10N, 33.60E", LandscapeMapView.PositionText(33.6d, 42.1d));
        Assert.Equal("12.50S, 7.25W", LandscapeMapView.PositionText(-7.25d, -12.5d));
        Assert.Equal("0.00, 0.00", LandscapeMapView.PositionText(0d, 0d));
    }

    /// <summary>
    /// The step is chosen per axis from what the painter measures: the
    /// east-west labels are written across the gutter, so at a zoom where
    /// the zoom's own step puts two of them closer than the widest label
    /// plus a gap, the step widens to the next coarser one; the
    /// north-south labels stack, so the same zoom keeps the zoom's step
    /// there. Mutation: use one step for both axes, or ignore the measure,
    /// and the east-west labels overlap again.
    /// </summary>
    [Fact]
    public void TheEastWestStepWidensUntilTheLabelsFitAndTheNorthSouthStepNeedNot()
    {
        var view = new LandscapeMapView(320, 280, 1024, 1024) { Zoom = 0.06d };
        view.CenterOn(0d, 0d);
        Assert.Equal(10d, LandscapeMapView.CoordinateIncrement(view.Zoom));

        Assert.Equal(20d, view.HorizontalIncrement(Measure));
        Assert.Equal(10d, view.VerticalIncrement(Measure));

        IReadOnlyList<MapRulerTick> horizontal = view.HorizontalTicks(Measure);
        double widest = horizontal.Max(tick => Measure(tick.Label).Width);
        for (int i = 1; i < horizontal.Count; i++)
            Assert.True(
                horizontal[i].Pixel - horizontal[i - 1].Pixel >= widest + LandscapeMapView.LabelGap,
                $"ticks {horizontal[i - 1].Label} and {horizontal[i].Label} crowd");

        IReadOnlyList<MapRulerTick> vertical = view.VerticalTicks(Measure);
        for (int i = 1; i < vertical.Count; i++)
            Assert.True(vertical[i - 1].Pixel - vertical[i].Pixel >= 12d + LandscapeMapView.LabelGap);
    }

    /// <summary>
    /// At every zoom two neighbouring east-west labels never overlap, and
    /// the step never widens beyond what fitting them needs. Mutation:
    /// widen one step too many and the second assertion fails.
    /// </summary>
    [Theory]
    [InlineData(0.0d)]
    [InlineData(0.04d)]
    [InlineData(0.1d)]
    [InlineData(0.24d)]
    [InlineData(0.4d)]
    [InlineData(0.6d)]
    [InlineData(0.8d)]
    [InlineData(0.9d)]
    [InlineData(1.0d)]
    public void TheEastWestStepIsTheNarrowestWhoseLabelsDoNotOverlap(double zoom)
    {
        var view = new LandscapeMapView(320, 280, 1024, 1024) { Zoom = zoom };
        view.CenterOn(33.6d, 42.1d);
        IReadOnlyList<MapRulerTick> ticks = view.HorizontalTicks(Measure);
        double widest = ticks.Count == 0 ? 0d : ticks.Max(tick => Measure(tick.Label).Width);
        for (int i = 1; i < ticks.Count; i++)
            Assert.True(ticks[i].Pixel - ticks[i - 1].Pixel >= widest + LandscapeMapView.LabelGap);

        double chosen = view.HorizontalIncrement(Measure);
        double own = LandscapeMapView.CoordinateIncrement(zoom);
        Assert.True(chosen >= own);
        if (chosen > own)
        {
            // One step narrower would have crowded: the pixels per that step
            // are fewer than the widest label needs.
            double narrower = LandscapeMapView.NarrowerIncrement(chosen);
            Assert.True(narrower * view.PixelsPerUnitX < widest + LandscapeMapView.LabelGap);
        }
    }
}

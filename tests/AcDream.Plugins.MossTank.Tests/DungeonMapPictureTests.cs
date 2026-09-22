using System.Numerics;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The dungeon map model: a floorplan turned into a north-up picture in map
/// pixels, one storey at a time, and the rule that says which storeys are
/// drawn and how dim.
/// </summary>
public sealed class DungeonMapPictureTests
{
    private static readonly uint Landblock = 0x01A90000u;

    /// <summary>
    /// Two ten-metre rooms side by side on the ground storey, a third room
    /// north of them one storey up, and the walls around the west room.
    /// </summary>
    internal static PluginDungeonFloorplan TwoRooms()
    {
        IReadOnlyList<Vector2> west = [new(0, 0), new(10, 0), new(10, 10), new(0, 10)];
        IReadOnlyList<Vector2> east = [new(10, 0), new(20, 0), new(20, 10), new(10, 10)];
        IReadOnlyList<Vector2> upper = [new(0, 10), new(10, 10), new(10, 20), new(0, 20)];
        var ground = new PluginDungeonLayer(
            0f,
            [west, east],
            [
                new PluginDungeonWall(new(0, 0), new(0, 10)),
                new PluginDungeonWall(new(0, 10), new(10, 10)),
            ]);
        var first = new PluginDungeonLayer(6f, [upper], []);
        return new PluginDungeonFloorplan(
            Landblock,
            [ground, first],
            [
                new PluginDungeonCell(Landblock | 0x100u, new(5, 5, 0), 0f),
                new PluginDungeonCell(Landblock | 0x101u, new(15, 5, 0), 0f),
                new PluginDungeonCell(Landblock | 0x102u, new(5, 15, 6), 6f),
            ],
            new Vector3(0, 0, 0),
            new Vector3(20, 20, 6));
    }

    /// <summary>
    /// A ten-metre room is fifty map pixels across, its rows merge into one
    /// rectangle, and each rectangle knows which cell it belongs to, which
    /// is what the visited tint later keys on. Mutation: rasterising at one
    /// pixel per metre, or forgetting the vertical merge, changes the counts.
    /// </summary>
    [Fact]
    public void FloorsRasteriseToOneRectanglePerCellAtFivePixelsPerMetre()
    {
        DungeonMapPicture picture = DungeonMapPicture.Build(TwoRooms());

        Assert.False(picture.IsEmpty);
        Assert.Equal(100, picture.Width);
        Assert.Equal(100, picture.Height);
        Assert.Equal(2, picture.Layers.Count);

        DungeonMapLayerPicture ground = picture.Layers[0];
        Assert.Equal(0f, ground.Z);
        Assert.Equal(2, ground.Floors.Count);
        Assert.Contains(ground.Floors, run => run.Rect == new DungeonMapRect(0, 50, 50, 50) && run.CellIndex == 0);
        Assert.Contains(ground.Floors, run => run.Rect == new DungeonMapRect(50, 50, 50, 50) && run.CellIndex == 1);

        DungeonMapLayerPicture first = picture.Layers[1];
        Assert.Equal(6f, first.Z);
        Assert.Single(first.Floors);
        Assert.Equal(new DungeonMapRect(0, 0, 50, 50), first.Floors[0].Rect);
        Assert.Equal(2, first.Floors[0].CellIndex);
    }

    /// <summary>
    /// The picture is north-up with east to the right: a wall running north
    /// climbs the picture, so its pixel y falls as its metre y rises. The
    /// reference mirrors x and then turns the whole picture half a circle;
    /// the two cancel to exactly this, and drawing it directly leaves nothing
    /// to undo. Mutation: dropping the y flip puts north at the bottom.
    /// </summary>
    [Fact]
    public void WallsAreMappedNorthUpWithEastToTheRight()
    {
        DungeonMapPicture picture = DungeonMapPicture.Build(TwoRooms());

        DungeonMapLine north = picture.Layers[0].Walls[0];
        Assert.Equal(new DungeonMapLine(0, 100, 0, 50), north);
        DungeonMapLine east = picture.Layers[0].Walls[1];
        Assert.Equal(new DungeonMapLine(0, 50, 50, 50), east);

        Assert.Equal(new Vector2(75, 75), picture.MapPixelFromLocal(new Vector3(15, 5, 0)));
    }

    /// <summary>
    /// The pixel origin is the plan's own bounds, not the landblock square:
    /// a dungeon that runs west of its landblock still starts at pixel zero.
    /// </summary>
    [Fact]
    public void ThePixelOriginIsTheNorthWestCornerOfTheBounds()
    {
        IReadOnlyList<Vector2> room = [new(-5, 3), new(5, 3), new(5, 23), new(-5, 23)];
        var plan = new PluginDungeonFloorplan(
            Landblock,
            [new PluginDungeonLayer(0f, [room], [])],
            [new PluginDungeonCell(Landblock | 0x100u, new(0, 13, 0), 0f)],
            new Vector3(-5, 3, 0),
            new Vector3(5, 23, 0));

        DungeonMapPicture picture = DungeonMapPicture.Build(plan);

        Assert.Equal(new Vector2(0, 0), picture.MapPixelFromLocal(new Vector3(-5, 23, 0)));
        Assert.Equal(new Vector2(50, 100), picture.MapPixelFromLocal(new Vector3(5, 3, 0)));
        Assert.Equal(50, picture.Width);
        Assert.Equal(100, picture.Height);
        Assert.True(picture.TryFindCellIndex(Landblock | 0x100u, out int index));
        Assert.Equal(0, index);
        Assert.False(picture.TryFindCellIndex(Landblock | 0x101u, out _));
    }

    /// <summary>
    /// The empty plan is an empty picture with no layers, so a landblock the
    /// host does not know draws nothing rather than a one-pixel box.
    /// </summary>
    [Fact]
    public void TheEmptyPlanIsAnEmptyPicture()
    {
        DungeonMapPicture picture = DungeonMapPicture.Build(PluginDungeonFloorplan.Empty);

        Assert.True(picture.IsEmpty);
        Assert.Empty(picture.Layers);
    }

    /// <summary>
    /// Which storeys are drawn from a given height, and how: more than ten
    /// metres above or twenty-four below is skipped; the storey above is
    /// washed grey and half transparent; the current one is untouched; the
    /// ones below dim by a fifteenth per metre until they are black.
    /// Mutation: dimming by a sixth per metre blackens the storey one down.
    /// </summary>
    [Theory]
    [InlineData(12f, false, 0f, 0f)]
    [InlineData(6f, true, 151f / 255f, 121f / 255f)]
    [InlineData(0f, true, 1f, 1f)]
    [InlineData(-6f, true, 0.6f, 1f)]
    [InlineData(-15f, true, 0f, 1f)]
    [InlineData(-24f, true, 0f, 1f)]
    [InlineData(-30f, false, 0f, 0f)]
    public void StoreysAreSkippedWashedOrDimmedByHeightAboveThePlayer(
        float layerZ,
        bool visible,
        float brightness,
        float opacity)
    {
        DungeonLayerShade shade = DungeonLayerShade.For(drawZ: 0d, layerZ, showAllLayers: false);

        Assert.Equal(visible, shade.Visible);
        if (visible)
        {
            Assert.Equal(brightness, shade.Brightness, 3);
            Assert.Equal(opacity, shade.Opacity, 3);
        }
    }

    /// <summary>
    /// The player's own height is the middle of a six-metre band, so a
    /// storey is current from three metres under its floor to three over.
    /// </summary>
    [Fact]
    public void TheCurrentStoreyIsTheSixMetreBandAroundTheDrawHeight()
    {
        Assert.Equal(1f, DungeonLayerShade.For(2.9d, 0f, false).Brightness);
        Assert.Equal(1f, DungeonLayerShade.For(-2.9d, 0f, false).Brightness);
        Assert.NotEqual(1f, DungeonLayerShade.For(3.5d, 0f, false).Brightness);
        Assert.NotEqual(1f, DungeonLayerShade.For(-3.5d, 0f, false).Opacity);
        Assert.Equal(6f, DungeonLayerShade.BandOf(5f));
        Assert.Equal(0f, DungeonLayerShade.BandOf(2.9f));
        Assert.Equal(-6f, DungeonLayerShade.BandOf(-3.1f));

        // The live Town Network entry: standing at -5.995 the storey at -6
        // is current and plain, the hall at 0 is the storey above.
        Assert.Equal(-6f, DungeonLayerShade.BandOf(-5.995f));
        Assert.Equal(DungeonLayerShade.Plain, DungeonLayerShade.For(-5.995d, -6f, false));
        DungeonLayerShade hall = DungeonLayerShade.For(-5.995d, 0f, false);
        Assert.True(hall.Visible);
        Assert.Equal(151f / 255f, hall.Brightness, 3);
        Assert.Equal(121f / 255f, hall.Opacity, 3);
    }

    /// <summary>
    /// The show-all switch draws every storey untinted, however far away.
    /// </summary>
    [Fact]
    public void ShowAllLayersDrawsEveryStoreyUntinted()
    {
        foreach (float z in new[] { 40f, 6f, 0f, -30f })
        {
            DungeonLayerShade shade = DungeonLayerShade.For(0d, z, showAllLayers: true);
            Assert.True(shade.Visible);
            Assert.Equal(1f, shade.Brightness);
            Assert.Equal(1f, shade.Opacity);
        }
    }

    /// <summary>
    /// The height slider spans the dungeon's depth: nought is the lowest
    /// storey, a hundred the highest, and the position reads back from a
    /// height the same way.
    /// </summary>
    [Fact]
    public void TheHeightSliderSpansTheDungeonsDepth()
    {
        Assert.Equal(-12d, DungeonLayerShade.DrawZFromSlider(-12f, 6f, 0));
        Assert.Equal(-3d, DungeonLayerShade.DrawZFromSlider(-12f, 6f, 50));
        Assert.Equal(6d, DungeonLayerShade.DrawZFromSlider(-12f, 6f, 100));
        Assert.Equal(50, DungeonLayerShade.SliderFromDrawZ(-12f, 6f, -3d));
        Assert.Equal(0, DungeonLayerShade.SliderFromDrawZ(0f, 0f, 5d));
    }
}

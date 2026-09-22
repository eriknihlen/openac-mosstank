using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The dungeon window on the panel: its controls reach the map, and the
/// map's zoom reaches the page's row.
/// </summary>
public sealed partial class MossTankPanelTests
{
    /// <summary>
    /// The seam between the page and the map: a zoom from the window is
    /// written to the DungeonMaps.MapZoom row, so the page shows the zoom
    /// the map is at and a profile keeps it. Mutation: a controller bound
    /// to a discarding save leaves the row at its shipped 4.2.
    /// </summary>
    [Fact]
    public void TheDungeonWindowDrivesTheMapAndTheZoomReachesTheRow()
    {
        var automation = new FakeAutomation { Name = "Acdream", WorldName = "Coldeve" };
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));
        panel.OnTick(0.1d);

        Assert.False(panel.UbDungeonVisible);
        panel.ShowUbDungeon();
        Assert.True(panel.UbDungeonVisible);
        Assert.Equal("Not in a dungeon.", panel.UbDungeonStatusText);
        Assert.Equal("Follow: on", panel.UbDungeonFollowText);
        Assert.Equal("Layers: near", panel.UbDungeonLayersText);
        panel.ToggleUbDungeonLayers();
        Assert.Equal("Layers: all", panel.UbDungeonLayersText);

        panel.SetUbFilterText("DungeonMaps.MapZoom");
        Assert.Equal("4.2", panel.UbSettingValues[0]);
        panel.UbDungeonZoomIn();
        panel.SetUbFilterText("DungeonMaps.MapZoom");
        Assert.Equal("4", panel.UbSettingValues[0]);

        panel.SetUbDungeonHeight(50f);
        Assert.Equal("Follow: off", panel.UbDungeonFollowText);
        panel.ToggleUbDungeonFollow();
        Assert.Equal("Follow: on", panel.UbDungeonFollowText);
        panel.HideUbDungeon();
        Assert.False(panel.UbDungeonVisible);
        panel.Dispose();
    }
}

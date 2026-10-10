using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed partial class MossTankPanelTests
{
    private const string ReloadMeta = "STATE: {Default} ~~ {\r\n\tIF:\tAlways\r\n\t\tDO:\tDoAll\r\n\t\t\t\tEmbedNav nav0 {stipend.nav}\r\n\t\t\t\tSetState {Walk}\r\nSTATE: {Walk} ~~ {\r\n\tIF:\tNever\r\n\t\tDO:\tNone\r\nNAV: nav0 once ~~ {\r\n\tpnt 47.1 26.1 0.2\r\n";

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void ReloadingMetaDiscardsItsPreviousEmbeddedNavigation(bool dropdown)
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/metas/Reload.af"] = ReloadMeta;
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation { Name = "Barris", WorldName = "Coldeve" }, storage));
        Command(panel, "meta load Reload");
        panel.ToggleMeta();
        panel.ToggleCombat();
        panel.OnTick(0.3d);
        Assert.Single(panel.RouteRows);
        Assert.Equal("Walk", panel.MetaState);
        if (dropdown) panel.SelectMetaProfile("Reload");
        else Command(panel, "meta load Reload");
        Assert.Equal("Default", panel.MetaState);
        Assert.Empty(panel.RouteRows);
        Assert.DoesNotContain("(embedded)", panel.SelectedRouteProfile);
        panel.OnTick(0.3d);
        Assert.Single(panel.RouteRows);
        Assert.Equal("Walk", panel.MetaState);
    }

    [Fact]
    public void ReselectingEmbeddedRouteReadsTheMetaFileAgain()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/metas/Reload.af"] = ReloadMeta;
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation { Name = "Barris", WorldName = "Coldeve" }, storage));
        Command(panel, "meta load Reload");
        panel.ToggleMeta();
        panel.ToggleCombat();
        panel.OnTick(0.3d);
        storage.Text["mosstank/metas/Reload.af"] = ReloadMeta + "\tpnt 48.1 27.1 0.2\r\n";
        panel.SelectRouteProfile(panel.SelectedRouteProfile);
        Assert.Equal(2, panel.RouteRows.Count);
        Assert.Equal("Walk", panel.MetaState);
    }

    [Fact]
    public void ReselectingRouteReadsDiskWithoutSavingStaleContents()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/navs/Reload.af"] = OnePointRoute;
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation { Name = "Barris", WorldName = "Coldeve" }, storage));
        panel.SelectRouteProfile("Reload.af");
        string changed = OnePointRoute + "\tpnt 48.1 27.1 0.2\r\n";
        storage.Text["mosstank/navs/Reload.af"] = changed;
        panel.SelectRouteProfile("Reload.af");
        Assert.Equal(changed, storage.Text["mosstank/navs/Reload.af"]);
        Assert.Equal(2, panel.RouteRows.Count);
    }
    [Fact]
    public void MissingEmbeddedSourceLeavesCurrentRouteAndReportsFailure()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/metas/Reload.af"] = ReloadMeta;
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation { Name = "Barris", WorldName = "Coldeve" }, storage));
        Command(panel, "meta load Reload");
        panel.ToggleMeta();
        panel.ToggleCombat();
        panel.OnTick(0.3d);
        storage.Text.Remove("mosstank/metas/Reload.af");
        panel.SelectRouteProfile(panel.SelectedRouteProfile);
        Assert.Single(panel.RouteRows);
        Assert.Equal("Walk", panel.MetaState);
        Assert.Contains("Could not reload", panel.RouteNotice);
    }
    [Fact]
    public void EmbeddedOnceRouteStartsAtItsHeadInsteadOfTheOldCircularAnchor()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/metas/Reload.af"] = ReloadMeta;
        storage.Text["mosstank/navs/Old.af"] = "NAV: nav0 circular ~~ {\r\n\tpnt 20 20 0\r\n\tpnt 10 10 0\r\n\tpnt 0 0 0\r\n";
        var automation = new FakeAutomation { Name = "Barris", WorldName = "Coldeve", NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Command(panel, "nav load Old.af");
        Command(panel, "meta load Reload");
        panel.ToggleMeta();
        panel.ToggleCombat();
        Assert.Equal(2, panel.CurrentRouteWaypointIndexForTest);
        panel.OnTick(0.3d);
        Assert.Equal("Walk", panel.MetaState);
        Assert.Equal(0, panel.CurrentRouteWaypointIndexForTest);
        Assert.DoesNotContain(0xC03838u, panel.RouteWaypointColors);
        Assert.Contains("[MossTank] Loaded stipend.nav (embedded) (1 points).", automation.Messages);
        Assert.Contains("[MossTank] Meta state: Default -> Walk.", automation.Messages);
        Assert.Contains(automation.Messages, line => line.StartsWith("[MossTank] Loaded route Old (", StringComparison.Ordinal));
        Assert.All(automation.Messages, line => Assert.StartsWith("[MossTank]", line));
        Assert.DoesNotContain(automation.Messages, line => line.StartsWith("[MossTank] [MossTank]", StringComparison.Ordinal));
    }
}

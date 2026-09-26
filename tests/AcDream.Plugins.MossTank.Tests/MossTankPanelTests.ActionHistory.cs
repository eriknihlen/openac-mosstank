namespace AcDream.Plugins.MossTank.Tests;

public sealed partial class MossTankPanelTests
{
    [Fact]
    public void RecordsEveryTransitionWhileClosedWithoutDependingOnTicksOrReads()
    {
        using var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        Assert.False(panel.ActionHistoryVisible);
        Assert.EndsWith("Combat off", Assert.Single(panel.ActionHistoryLines));
        panel.ToggleCombat();
        string running = panel.CombatStatus;
        panel.ToggleCombat();
        Assert.False(panel.ActionHistoryVisible);
        Assert.Contains(panel.ActionHistoryLines, line => line.EndsWith(running, StringComparison.Ordinal));
        Assert.EndsWith(panel.CombatStatus, panel.ActionHistoryLines[^1]);
        Assert.True(panel.ActionHistoryLines.Count >= 3);
        IReadOnlyList<string> snapshot = panel.ActionHistoryLines;
        panel.ShowActionHistory();
        Assert.True(panel.ActionHistoryVisible);
        panel.HideActionHistory();
        Assert.False(panel.ActionHistoryVisible);
        Assert.Same(snapshot, panel.ActionHistoryLines);
        string current = panel.CombatStatus;
        panel.ClearActionHistory();
        Assert.Empty(panel.ActionHistoryLines);
        Assert.Equal(current, panel.CombatStatus);
        panel.ToggleCombat();
        Assert.NotEmpty(panel.ActionHistoryLines);
    }

    [Fact]
    public void HistoryRegistersWithBoundVisibilityAndNoShelfEntry()
    {
        var ui = new RemoteUi();
        var host = new FakeHost(new FakeAutomation(), lootClassifiers: new RemoteLootRegistry()) { Ui = ui };
        var plugin = new MossTankPlugin();
        plugin.Initialize(host);
        plugin.Enable();
        var history = Assert.Single(ui.Panels, entry => entry.Descriptor.WindowId == "action-history");
        Assert.True(history.Descriptor.StartVisible);
        Assert.False(history.Descriptor.ShowInSidePanel);
        Assert.Equal("mosstank-action-history.xml", Path.GetFileName(history.Path));
        var panel = Assert.IsType<MossTankPanel>(history.Binding);
        Assert.False(panel.ActionHistoryVisible);
        plugin.Disable();
    }
}

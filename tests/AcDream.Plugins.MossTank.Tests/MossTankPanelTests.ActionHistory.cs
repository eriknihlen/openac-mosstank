namespace AcDream.Plugins.MossTank.Tests;

public sealed partial class MossTankPanelTests
{
    [Fact]
    public void DebugButtonSavesHistoryAndStopsWritingWhenDisabled()
    {
        var storage = new MemoryStorage();
        using var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));
        panel.ToggleDebugLog();
        string key = Assert.Single(storage.Text.Keys, key => key.StartsWith("debug/"));
        Assert.Contains("Combat off", storage.Text[key]);
        Assert.Equal("Debug log: ON", panel.DebugLogButtonText);
        panel.ToggleCombat();
        panel.ToggleDebugLog();
        string saved = storage.Text[key];
        Assert.Contains("Logging stopped.", saved);
        panel.ToggleCombat();
        Assert.Equal(saved, storage.Text[key]);
        Assert.Equal("Debug log: off", panel.DebugLogButtonText);
    }

    [Fact]
    public void ActivityIncludesNavigationAndDoesNotRepeatUnchangedState()
    {
        using var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        panel.ToggleCombat();
        panel.OnTick(0.5);
        Assert.Contains(panel.ActionHistoryLines, line => line.Contains("Navigation:"));
        int count = panel.ActionHistoryLines.Count(line => line.Contains("Navigation:"));
        panel.OnTick(0.5);
        Assert.Equal(count, panel.ActionHistoryLines.Count(line => line.Contains("Navigation:")));
    }

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

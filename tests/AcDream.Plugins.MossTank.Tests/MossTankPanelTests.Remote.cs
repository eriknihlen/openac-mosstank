using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed partial class MossTankPanelTests
{
    [Fact]
    public void RemoteFollowPreservesTheLoadedRouteAndMacroSwitches()
    {
        (MossTankPanel panel, FakeHost host, MemoryStorage storage) =
            PanelWithRoute("mosstank/navs/jaw.nav", DroppedRoute());
        Command(panel, "nav load jaw.nav");
        host.Selection.Select(0x50000ABCu);
        var before = RouteFiles(storage);
        string[] originalRows = panel.RouteRows.ToArray();
        var switches = (panel.CombatMacroRunning, panel.CombatEnabled,
            panel.NavigationEnabled, panel.LootEnabled, panel.BuffingEnabled, panel.MetaEnabled);

        panel.RemoteFollowSelected();

        Assert.Equal("UBFollow", panel.SelectedRouteProfile);
        Assert.Equal("Follow target: Zero Cool", panel.RouteFollowTargetText);
        var after = RouteFiles(storage);
        Assert.True(after.Remove(FollowRouteKey));
        Assert.Equal(before, after);
        Assert.Equal(switches, (panel.CombatMacroRunning, panel.CombatEnabled,
            panel.NavigationEnabled, panel.LootEnabled, panel.BuffingEnabled, panel.MetaEnabled));
        Command(panel, "nav load jaw.nav");
        Assert.Equal(originalRows, panel.RouteRows);
    }

    [Theory]
    [InlineData(0u)]
    [InlineData(123u)]
    [InlineData(1u)]
    [InlineData(0x50000ABCu)]
    public void RemoteFollowRejectsMissingOrNonPlayerSelection(uint selected)
    {
        (MossTankPanel panel, FakeHost host, MemoryStorage storage) =
            PanelWithRoute("mosstank/navs/jaw_1.af", OnePointRoute);
        Command(panel, "nav load jaw_1");
        ((FakeAutomation)host.Automation).WorldObjects =
            [Landscape(0x50000ABCu, "Not a player", PluginObjectClass.Monster, 0d),
             Player(1u, "Self")];
        host.Selection.Select(selected);
        var before = RouteFiles(storage);

        panel.RemoteFollowSelected();

        Assert.Equal("jaw_1", panel.SelectedRouteProfile);
        Assert.Single(panel.RouteRows);
        Assert.Equal(before, RouteFiles(storage));
        Assert.Contains(((FakeAutomation)host.Automation).Messages,
            message => message.Contains("Select a live player", StringComparison.Ordinal));
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public void RemoteSwitchHidesSourceOnlyAfterDestinationOpens(bool opens)
    {
        var ui = new RemoteUi { Opens = opens };
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()) { Ui = ui });
        bool running = panel.CombatMacroRunning;
        panel.ShowRemote();
        panel.ShowMainWindow();
        Assert.Equal(opens
            ? ["show:remote", "hide:main", "show:main", "hide:remote"]
            : ["show:remote", "show:main"], ui.Calls);
        Assert.Equal(running, panel.CombatMacroRunning);
    }

    [Fact]
    public void RemoteRegistersHiddenWithoutAnotherShelfEntryAndSharesMainBindings()
    {
        var ui = new RemoteUi();
        var host = new FakeHost(new FakeAutomation(), lootClassifiers: new RemoteLootRegistry()) { Ui = ui };
        var plugin = new MossTankPlugin();
        plugin.Initialize(host);
        plugin.Enable();
        var remote = Assert.Single(ui.Panels, entry => entry.Descriptor.WindowId == "remote");
        var main = Assert.Single(ui.Panels, entry => entry.Descriptor.WindowId == "main");
        Assert.False(remote.Descriptor.StartVisible);
        Assert.False(remote.Descriptor.ShowInSidePanel);
        Assert.Equal("mosstank-remote.xml", Path.GetFileName(remote.Path));
        Assert.Same(main.Binding, remote.Binding);
        plugin.Disable();
    }

    private sealed class RemoteLootRegistry : IPluginLootClassifierRegistry, IDisposable
    {
        public IDisposable Register(string classifierId, string displayName, IPluginLootClassifier classifier) => this;
        public void Dispose() { }
    }

    private sealed class RemoteUi : IUiRegistry
    {
        public bool Opens { get; init; }
        public List<string> Calls { get; } = [];
        public List<(PluginPanelDescriptor Descriptor, string Path, object Binding)> Panels { get; } = [];
        public void AddPanel(PluginPanelDescriptor descriptor, string markupPath, object binding) =>
            Panels.Add((descriptor, markupPath, binding));
        public void AddMarkupPanel(string markupPath, object binding) { }
        public bool ShowPanel(string viewName) { Calls.Add("show:" + viewName); return Opens; }
        public bool HidePanel(string viewName) { Calls.Add("hide:" + viewName); return true; }
    }
}

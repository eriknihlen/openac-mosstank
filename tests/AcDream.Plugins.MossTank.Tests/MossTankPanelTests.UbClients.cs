using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The clients readout on the panel: its place is read from and written to
/// the character's NetworkUI.WindowPositionX and WindowPositionY rows.
/// </summary>
public sealed partial class MossTankPanelTests
{
    private const string UbGlobalSettings = "mosstank/ub/settings.json";
    private const string UbCharacterSettings = "mosstank/ub/Coldeve/Acdream/settings.json";

    private static (MossTankPanel Panel, LandscapeMapControllerTests.FakeCanvas Canvas) ClientsReadout(
        MemoryStorage storage)
    {
        var ui = new LandscapeMapControllerTests.FakeUi();
        var automation = new FakeAutomation { Name = "Acdream", WorldName = "Coldeve" };
        var panel = new MossTankPanel(new FakeHost(automation, storage) { Ui = ui });
        panel.OnTick(0.1d);
        panel.OnTick(0.3d);
        return (panel, Assert.Single(ui.Canvases, static c => c.CanvasId == NetworkHudController.CanvasId));
    }

    /// <summary>
    /// The whole path: a drag on the readout's canvas moves the canvas,
    /// writes the character's rows, and a panel started again puts the
    /// readout back there. Mutation: a panel whose save writes nowhere
    /// leaves the rows at 22 and 105 and the next panel at (26, 165).
    /// </summary>
    [Fact]
    public void TheClientsReadoutsDragReachesItsRowsAndItComesBackThere()
    {
        var storage = new MemoryStorage();
        storage.Text[UbGlobalSettings] = "{\"NetworkUI.Enabled\": \"True\"}";
        (MossTankPanel panel, LandscapeMapControllerTests.FakeCanvas canvas) = ClientsReadout(storage);
        Assert.Equal(new PluginPoint(26d, 165d), canvas.Offset);

        canvas.Down(100d, 10d);
        canvas.Up(130d, 50d);
        Assert.Equal(new PluginPoint(56d, 205d), canvas.Offset);
        panel.SetUbFilterText("NetworkUI.WindowPositionX");
        Assert.Equal("52", panel.UbSettingValues[0]);
        panel.SetUbFilterText("NetworkUI.WindowPositionY");
        Assert.Equal("145", panel.UbSettingValues[0]);
        panel.Dispose();

        (MossTankPanel next, LandscapeMapControllerTests.FakeCanvas again) = ClientsReadout(storage);
        Assert.Equal(new PluginPoint(56d, 205d), again.Offset);
        next.Dispose();
    }

    /// <summary>
    /// The readout opens at the place the character's file holds, and a
    /// drag writes the new place to that file and to no other. Mutation:
    /// read or write the rows at another tier and the readout opens at
    /// (26, 165) or the global file carries the place.
    /// </summary>
    [Fact]
    public void TheWindowPositionIsKeptInTheCharactersRows()
    {
        var storage = new MemoryStorage();
        storage.Text[UbGlobalSettings] = "{\"NetworkUI.Enabled\": \"True\"}";
        storage.Text[UbCharacterSettings] =
            "{\"NetworkUI.WindowPositionX\": \"300\", \"NetworkUI.WindowPositionY\": \"40\"}";
        (MossTankPanel panel, LandscapeMapControllerTests.FakeCanvas canvas) = ClientsReadout(storage);
        Assert.Equal(new PluginPoint(304d, 100d), canvas.Offset);

        canvas.Down(10d, 10d);
        canvas.Up(20d, 30d);
        Assert.Equal(new PluginPoint(314d, 120d), canvas.Offset);
        Assert.Contains("\"NetworkUI.WindowPositionX\": \"310\"", storage.Text[UbCharacterSettings], StringComparison.Ordinal);
        Assert.Contains("\"NetworkUI.WindowPositionY\": \"60\"", storage.Text[UbCharacterSettings], StringComparison.Ordinal);
        Assert.DoesNotContain("WindowPosition", storage.Text[UbGlobalSettings], StringComparison.Ordinal);
        panel.Dispose();
    }
}

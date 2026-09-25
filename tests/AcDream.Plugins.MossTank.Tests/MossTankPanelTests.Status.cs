using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed partial class MossTankPanelTests
{
    /// <summary>
    /// The macro's current meta state is published on the host's status
    /// board, where other plugins read it the way they read VTank's current
    /// meta state; it follows /vt setmetastate. Mutation: drop the publish
    /// from the tick and the board stays empty.
    /// </summary>
    [Fact]
    public void TheMetaStateIsPublishedForOtherPlugins()
    {
        var automation = new FakeAutomation { Name = "Barris", WorldName = "Coldeve" };
        var storage = new MemoryStorage();
        storage.Text["mosstank/metas/Bella.met"] = File.ReadAllText(Path.Combine(
            AppContext.BaseDirectory, "Fixtures", "vtank", "met", "bella.met"));
        var host = new FakeHost(automation, storage);
        var panel = new MossTankPanel(host);
        Command(panel, "meta load Bella");

        panel.OnTick(0.1d);
        Assert.Equal("Default", host.StatusBoard.Lines["metaState"]);

        Command(panel, "setmetastate mp_primary");
        panel.OnTick(0.1d);
        Assert.Equal("mp_primary", host.StatusBoard.Lines["metaState"]);
    }

    private sealed class RecordingStatusBoard : IPluginStatusBoard
    {
        public Dictionary<string, string> Lines { get; } = new(StringComparer.Ordinal);

        public bool IsAvailable => true;

        public bool Publish(string key, string? value)
        {
            if (value is null)
                Lines.Remove(key);
            else
                Lines[key] = value;
            return true;
        }
    }
}

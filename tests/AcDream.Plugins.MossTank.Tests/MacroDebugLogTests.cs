using System.Text;
using AcDream.Plugin.Abstractions;
namespace AcDream.Plugins.MossTank.Tests;
public sealed class MacroDebugLogTests
{
    [Fact]
    public void KeepsNewestWholeUtf8RecordsWithinByteLimit()
    {
        var storage = new Storage();
        var log = new MacroDebugLog(storage, "debug/test.log");
        log.Record("oldest");
        for (int i = 0; i < 3000; i++) log.Record($"{i}:" + new string('\u754c', 300));
        log.Record("newest");
        log.Flush();
        Assert.True(Encoding.UTF8.GetByteCount(storage.Text) <= MacroDebugLog.MaximumBytes);
        Assert.DoesNotContain("oldest", storage.Text);
        Assert.EndsWith("newest\n", storage.Text);
        Assert.All(storage.Text.Split('\n', StringSplitOptions.RemoveEmptyEntries), line => Assert.StartsWith("[", line));
        int writes = storage.Writes;
        log.Flush();
        Assert.Equal(writes, storage.Writes);
    }
    [Fact]
    public void OversizedRecordStaysWithinLimit()
    {
        var storage = new Storage();
        var log = new MacroDebugLog(storage, "debug/test.log");
        log.Record("before");
        log.Record(new string('\u754c', MacroDebugLog.MaximumBytes));
        log.Flush();
        Assert.Contains("before", storage.Text);
        Assert.Contains("Record omitted", storage.Text);
        Assert.True(Encoding.UTF8.GetByteCount(storage.Text) < MacroDebugLog.MaximumBytes);
    }
    private sealed class Storage : IPluginStorage
    {
        public string Text = "";
        public int Writes;
        public void WriteText(string key, string content) { Text = content; Writes++; }
    }
}

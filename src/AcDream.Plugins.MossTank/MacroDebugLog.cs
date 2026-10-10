using System.Text;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>Keeps the newest complete UTF-8 records, with periodic bounded snapshots.</summary>
internal sealed class MacroDebugLog(IPluginStorage storage, string key)
{
    internal const int MaximumBytes = 2 * 1024 * 1024;
    private readonly Queue<string> _lines = new();
    private int _bytes;
    private bool _dirty;
    public string Key => key;

    public void Record(string message)
    {
        string line = $"[{DateTimeOffset.UtcNow:O}] {message.Replace("\r", " ").Replace("\n", " ")}\n";
        int size = Encoding.UTF8.GetByteCount(line);
        if (size > MaximumBytes)
        {
            line = $"[{DateTimeOffset.UtcNow:O}] Record omitted: exceeds 2 MiB.\n";
            size = Encoding.UTF8.GetByteCount(line);
        }
        _lines.Enqueue(line);
        _bytes += size;
        while (_bytes > MaximumBytes)
            _bytes -= Encoding.UTF8.GetByteCount(_lines.Dequeue());
        _dirty = true;
    }

    public void Flush()
    {
        if (!_dirty) return;
        storage.WriteText(key, string.Concat(_lines));
        _dirty = false;
    }
}

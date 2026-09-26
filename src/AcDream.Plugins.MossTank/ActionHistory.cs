using System.Globalization;

namespace AcDream.Plugins.MossTank;

/// <summary>A bounded session history with stable entry positions across trimming and clearing.</summary>
internal sealed class ActionHistory(TimeProvider? timeProvider = null)
{
    internal const int Capacity = 1000;
    private readonly TimeProvider _clock = timeProvider ?? TimeProvider.System;
    private readonly Queue<string> _entries = new();
    private string? _lastStatus;

    public IReadOnlyList<string> Lines { get; private set; } = Array.Empty<string>();
    public int FirstIndex { get; private set; }

    public void Record(string status)
    {
        if (string.Equals(_lastStatus, status, StringComparison.Ordinal))
            return;
        _lastStatus = status;
        string timestamp = _clock.GetLocalNow().ToString("HH:mm:ss", CultureInfo.InvariantCulture);
        _entries.Enqueue($"[{timestamp}] {status}");
        if (_entries.Count > Capacity)
        {
            _entries.Dequeue();
            FirstIndex++;
        }
        Lines = _entries.ToArray();
    }

    public void Clear()
    {
        FirstIndex += _entries.Count;
        _entries.Clear();
        Lines = Array.Empty<string>();
        // Clearing does not create a new action. The next genuine change starts the new history.
    }
}

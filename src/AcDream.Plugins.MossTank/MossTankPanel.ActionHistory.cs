using System.Text.RegularExpressions;

namespace AcDream.Plugins.MossTank;

internal sealed partial class MossTankPanel
{
    private readonly ActionHistory _actionHistory = new();
    private readonly Dictionary<string, string> _activityStates = new();
    private MacroDebugLog? _debugLog;
    private double _activityElapsed;
    private double _debugFlushElapsed;
    private static readonly Regex ActivityMeasurements = new(@"\d+(?:[.,]\d+)?(?=[msd]\b)", RegexOptions.CultureInvariant);

    public bool ActionHistoryVisible { get; private set; }
    public IReadOnlyList<string> ActionHistoryLines => _actionHistory.Lines;
    public int ActionHistoryFirstIndex => _actionHistory.FirstIndex;
    public Action ShowActionHistory => () => ActionHistoryVisible = true;
    public Action HideActionHistory => () => ActionHistoryVisible = false;
    public Action ClearActionHistory => _actionHistory.Clear;
    public string DebugLogButtonText => _debugLog is null ? "Debug log: off" : "Debug log: ON";
    public string DebugLogLocation => _debugLog is { } log
        ? $"{_host.Storage.RootPath}/{log.Key}" : "Save detailed macro activity (newest 2 MiB).";
    public Action ToggleDebugLog => ToggleDebugLogging;

    private void ToggleDebugLogging()
    {
        if (_debugLog is not null)
        {
            WriteDebugLog("Logging stopped.");
            FlushDebugLog();
            _debugLog = null;
            return;
        }
        if (!_host.Storage.IsAvailable)
        {
            MossTankChat.Post(_host.Automation.Chat, "MossTank debug log: plugin storage is unavailable.");
            return;
        }
        _debugLog = new MacroDebugLog(_host.Storage,
            $"debug/macro-{DateTimeOffset.UtcNow:yyyyMMdd-HHmmss}-{Guid.NewGuid():N}.log");
        WriteDebugLog($"MossTank {typeof(MossTankPanel).Assembly.GetName().Version}; " +
            $"character={_host.Automation.Character.Name}; world={_host.Automation.Character.WorldName}");
        foreach (string line in _actionHistory.Lines) WriteDebugLog(line);
        FlushDebugLog();
        if (_debugLog is not null)
            MossTankChat.Post(_host.Automation.Chat, "MossTank debug log: " + DebugLogLocation);
    }

    private void WriteDebugLog(string message) => _debugLog?.Record(message);

    private void FlushDebugLog()
    {
        try { _debugLog?.Flush(); }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException or NotSupportedException)
        {
            _debugLog = null;
            _host.Log.Error("MossTank debug logging stopped: " + error.Message);
            MossTankChat.Post(_host.Automation.Chat, "MossTank debug logging stopped: " + error.Message);
        }
    }

    private void RecordActionHistory(string status)
    {
        _actionHistory.Record(status);
        WriteDebugLog("Combat: " + status);
    }

    private void ObserveActivity(string category, string status)
    {
        string key = ActivityMeasurements.Replace(status, "#");
        if (_activityStates.TryGetValue(category, out string? previous) && previous == key) return;
        _activityStates[category] = key;
        string line = category + ": " + status;
        _actionHistory.Record(line);
        WriteDebugLog(line);
    }

    private void ObserveMacroActivity(double elapsed)
    {
        _debugFlushElapsed += Math.Max(0, elapsed);
        if (_debugFlushElapsed >= 5)
        {
            _debugFlushElapsed = 0;
            FlushDebugLog();
        }
        _activityElapsed += Math.Max(0, elapsed);
        if (_activityElapsed < 0.5) return;
        _activityElapsed = 0;
        ObserveActivity("Macro", !_host.Automation.IsAvailable ? "Waiting for login." :
            !_combat.Enabled ? "Stopped." : _scheduler.IsSuspended ?
            "Paused: " + (_reportedSuspension ?? "waiting for an action to finish") : "Running.");
        if (!_host.Automation.IsAvailable || !_combat.Enabled) return;
        ObserveActivity("Rule", _scheduler.LastExecutedRule?.Name ?? "Waiting for work.");
        ObserveActivity("Navigation", _navigation.Status);
        ObserveActivity("Meta", _meta.Enabled ? _meta.CurrentState + " - " + _meta.Status : "Disabled.");
        ObserveActivity("Loot", _loot.Status);
        ObserveActivity("Buffs", _status);
        ObserveActivity("Vitals", VitalStatus);
        ObserveActivity("Item mana", _itemManaRecharge.Status);
    }
}

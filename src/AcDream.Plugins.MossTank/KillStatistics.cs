using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>Optional cumulative counters independent of bounded chat/log retention.</summary>
internal sealed class KillStatistics : IDisposable
{
    internal const string EnableKey = "kill-statistics/enabled.txt";
    internal const string SessionsPrefix = "kill-statistics/sessions/";
    private readonly IPluginHost _host;
    private readonly TimeProvider _clock;
    private bool _enabled;
    private bool _disposed;
    private bool _subscribed;
    private bool _awaitingLogin;
    private Snapshot? _session;
    private long _started;
    private long _flushed;
    private ulong _lastSequence;

    internal sealed record Snapshot(int SchemaVersion, string SessionId,
        string Character, string World, uint ObjectId, DateTimeOffset StartedUtc,
        DateTimeOffset UpdatedUtc, DateTimeOffset? EndedUtc, double CoveredSeconds,
        long TotalKills, bool Closed, ulong LastSequence);

    public KillStatistics(IPluginHost host, TimeProvider? clock = null)
    {
        _host = host;
        _clock = clock ?? TimeProvider.System;
        try
        {
            _enabled = host.Storage.IsAvailable &&
                string.Equals(host.Storage.ReadText(EnableKey)?.Trim(), "true", StringComparison.OrdinalIgnoreCase);
        }
        catch (Exception error)
        {
            Fail(error);
        }
        if (_enabled)
        {
            host.Automation.Chat.Received += OnReceived;
            host.Events.Logoff += OnLogoff;
            host.Events.LoginComplete += OnLoginComplete;
            _subscribed = true;
            Tick();
        }
    }

    public void Tick()
    {
        if (!_enabled || _disposed) return;
        SynchronizeSession();
        if (_enabled && _session is not null &&
            _clock.GetElapsedTime(_flushed).TotalSeconds >= 60)
            Flush(false);
    }

    private void SynchronizeSession()
    {
        var character = _host.Automation.Character;
        bool worldAvailable = _host.Automation.IsAvailable && character.IsInWorld;
        // Logoff precedes teardown. Do not reopen from the still-live character
        // until login is announced or an out-of-world transition was observed.
        if (!worldAvailable) _awaitingLogin = false;
        bool inWorld = !_awaitingLogin && worldAvailable &&
            character.ObjectId != 0 && character.Name.Length > 0;
        if (_session is not null && (!inWorld || _session.Character != character.Name ||
            _session.World != character.WorldName || _session.ObjectId != character.ObjectId))
        {
            Flush(true);
            _session = null;
        }
        if (!_enabled || !inWorld || _session is not null) return;
        var now = _clock.GetUtcNow();
        _started = _clock.GetTimestamp();
        _session = new(1, Guid.NewGuid().ToString("N"), character.Name,
            character.WorldName, character.ObjectId, now, now, null, 0, 0, false, _lastSequence);
        Flush(false);
    }

    private void OnReceived(PluginChatMessage message)
    {
        if (!_enabled || _disposed) return;
        SynchronizeSession();
        if (!_enabled || message.Sequence <= _lastSequence) return;
        _lastSequence = message.Sequence;
        if (_session is not null && message.LogTextType == CombatLogTextType.Default &&
            CombatResultText.IsKillingBlow(message.Text, out _))
            _session = _session with { TotalKills = _session.TotalKills + 1 };
    }

    private void OnLogoff()
    {
        if (!_enabled || _disposed) return;
        _awaitingLogin = true;
        Flush(true);
        _session = null;
    }

    private void OnLoginComplete()
    {
        if (!_enabled || _disposed) return;
        Flush(true);
        _session = null;
        _awaitingLogin = false;
        SynchronizeSession();
    }

    private void Flush(bool closed)
    {
        if (!_enabled || _session is null) return;
        var now = _clock.GetUtcNow();
        _session = _session with
        {
            UpdatedUtc = now, EndedUtc = closed ? now : null, Closed = closed,
            CoveredSeconds = _clock.GetElapsedTime(_started).TotalSeconds,
            LastSequence = _lastSequence,
        };
        try
        {
            _host.Storage.WriteJson(SessionsPrefix + _session.SessionId + ".json", _session);
            _flushed = _clock.GetTimestamp();
        }
        catch (Exception error)
        {
            Fail(error);
        }
    }

    private void Fail(Exception error)
    {
        _enabled = false;
        Unsubscribe();
        _host.Log.Error("Kill statistics recording stopped because storage failed. " +
            "The last saved snapshot is incomplete; later time is a measurement gap.", error);
    }

    public void Dispose()
    {
        if (_disposed) return;
        Unsubscribe();
        Flush(true);
        _disposed = true;
    }

    private void Unsubscribe()
    {
        if (!_subscribed) return;
        _host.Automation.Chat.Received -= OnReceived;
        _host.Events.Logoff -= OnLogoff;
        _host.Events.LoginComplete -= OnLoginComplete;
        _subscribed = false;
    }
}

using AcDream.Plugin.Abstractions;
using System.Text.Json;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class KillStatisticsTests
{
    [Fact]
    public void AllPersonalKillSentencesCountIncludingNonselectedAndUnnamedTargets()
    {
        var host = Enabled();
        using (var recorder = new KillStatistics(host, new Clock()))
        {
            ulong sequence = 0;
            foreach (string line in CombatResultTextTests.AuthenticKillLines)
                host.Surface.Send(++sequence, line);
        }
        Assert.Equal(36, Assert.Single(host.Store.Snapshots()).TotalKills);
    }

    [Fact]
    public void DisabledDoesNotSubscribeReadWorldOrWrite()
    {
        var host = new Host();
        using (var recorder = new KillStatistics(host)) recorder.Tick();
        Assert.Equal(0, host.Surface.Subscriptions);
        Assert.Equal(0, host.Store.Writes);
        Assert.Equal(0, host.Surface.CharacterReads);
        Assert.Equal(0, host.Notifications.Subscriptions);
    }

    [Fact]
    public void LogoffClosesBeforeTeardownAndSameIdentityReconnectNeedsNoTick()
    {
        var host = Enabled(); var clock = new Clock();
        using var recorder = new KillStatistics(host, clock);
        host.Surface.Send(1, "You killed Drudge!"); clock.Advance(10);
        host.Notifications.RaiseLogoff();
        Assert.True(Assert.Single(host.Store.Snapshots()).Closed);
        clock.Advance(100);
        // Teardown still exposes the old character and may deliver chat/ticks.
        host.Surface.Send(2, "You killed Drudge!");
        Assert.Single(host.Store.Snapshots());
        host.Notifications.RaiseLoginComplete();
        host.Surface.Send(3, "You killed Drudge!"); clock.Advance(20);
        recorder.Dispose();
        Assert.Equal(2, host.Store.Snapshots().Length);
        Assert.Equal(30, host.Store.Snapshots().Sum(x => x.CoveredSeconds));
        Assert.Equal(2, host.Store.Snapshots().Sum(x => x.TotalKills));
        Assert.Equal(0, host.Notifications.Subscriptions);
        host.Notifications.RaiseLoginComplete(); host.Notifications.RaiseLogoff();
        Assert.Equal(2, host.Store.Snapshots().Length);
    }

    [Fact]
    public void ObservedWorldTransitionCanReopenAfterLogoffWithoutLoginEvent()
    {
        var host = Enabled(); var clock = new Clock();
        using var recorder = new KillStatistics(host, clock);
        clock.Advance(10); host.Notifications.RaiseLogoff();
        recorder.Tick();
        Assert.Single(host.Store.Snapshots());
        host.Surface.IsInWorld = false; recorder.Tick(); clock.Advance(100);
        host.Surface.IsInWorld = true; recorder.Tick(); clock.Advance(20);
        recorder.Dispose();
        Assert.Equal(2, host.Store.Snapshots().Length);
        Assert.Equal(30, host.Store.Snapshots().Sum(x => x.CoveredSeconds));
    }

    [Fact]
    public void OutsideWorldHasNoCoverageOrKillsAndElapsedTimeIgnoresWallClockChanges()
    {
        var host = Enabled(); var clock = new Clock();
        host.Surface.IsInWorld = false;
        using var recorder = new KillStatistics(host, clock);
        clock.Advance(100); host.Surface.Send(1, "You killed Drudge!"); recorder.Tick();
        Assert.Empty(host.Store.Snapshots());
        host.Surface.IsInWorld = true; recorder.Tick();
        host.Surface.Send(1, "You killed Drudge!"); // Old delivery stays rejected on login.
        clock.Advance(60); clock.UtcOffset = -500; recorder.Tick();
        var saved = Assert.Single(host.Store.Snapshots());
        Assert.Equal(60, saved.CoveredSeconds);
        Assert.Equal(0, saved.TotalKills);
    }

    [Fact]
    public void InitialStorageFailureDisablesRecordingAndReportsIt()
    {
        var host = Enabled(); host.Store.Fail = true;
        using var recorder = new KillStatistics(host, new Clock());
        recorder.Tick(); host.Surface.Send(1, "You killed Drudge!");
        Assert.Single(host.Errors);
        Assert.Empty(host.Store.Snapshots());
        Assert.Equal(0, host.Surface.Subscriptions);
        Assert.Equal(1, host.Store.Writes);
    }

    [Fact]
    public void UnreadableEnableFlagReportsFailureWithoutSubscribing()
    {
        var host = Enabled(); host.Store.FailRead = true;
        using var recorder = new KillStatistics(host, new Clock());
        recorder.Tick();
        Assert.Single(host.Errors);
        Assert.Equal(0, host.Surface.Subscriptions);
        Assert.Equal(0, host.Store.Writes);
    }

    [Fact]
    public void DeliveredPersonalKillsDeduplicateAndFlushOncePerMinute()
    {
        var host = Enabled(); var clock = new Clock();
        using var recorder = new KillStatistics(host, clock);
        host.Surface.Send(1, "You killed Drudge!");
        host.Surface.Send(1, "You killed Drudge!");
        host.Surface.Send(2, "You killed Drudge!", 7);
        host.Surface.Send(3, "A nearby Drudge died.");
        host.Surface.Send(4, "You cleave Drudge in twain!");
        clock.Advance(59); recorder.Tick();
        Assert.Equal(1, host.Store.Writes);
        clock.Advance(1); recorder.Tick();
        var snapshot = Assert.Single(host.Store.Snapshots());
        Assert.Equal(2, snapshot.TotalKills);
        Assert.Equal(60, snapshot.CoveredSeconds);
        Assert.Equal(4ul, snapshot.LastSequence);
        Assert.False(snapshot.Closed);
        Assert.Equal(2, host.Store.Writes);
    }

    [Fact]
    public void LogoutAndCharacterOrWorldChangesCreateSeparateSegments()
    {
        var host = Enabled(); var clock = new Clock();
        using var recorder = new KillStatistics(host, clock);
        host.Surface.Send(1, "You killed Drudge!");
        clock.Advance(10); host.Surface.IsInWorld = false; recorder.Tick();
        clock.Advance(100); host.Surface.IsInWorld = true; recorder.Tick();
        clock.Advance(20); host.Surface.Name = "Second"; host.Surface.ObjectId = 2; recorder.Tick();
        clock.Advance(30); host.Surface.WorldName = "Other world"; recorder.Tick();
        recorder.Dispose();
        var segments = host.Store.Snapshots();
        Assert.Equal(4, segments.Length);
        Assert.All(segments, x => Assert.True(x.Closed));
        Assert.Equal(60, segments.Sum(x => x.CoveredSeconds));
        Assert.Equal(1, segments.Sum(x => x.TotalKills));
        Assert.Equal(4, segments.Select(x => x.SessionId).Distinct().Count());
        Assert.Contains(segments, x => x.Character == "Second" && x.World == "Other world");
    }

    [Fact]
    public void RestartLeavesPreviousSnapshotAndNeverCountsOfflineTime()
    {
        var host = Enabled(); var clock = new Clock();
        using (var first = new KillStatistics(host, clock))
        {
            clock.Advance(20); host.Surface.Send(10, "You killed Drudge!");
        }
        clock.Advance(500);
        using (var second = new KillStatistics(host, clock))
        {
            clock.Advance(40); host.Surface.Send(1, "You killed Drudge!");
        }
        Assert.Equal(2, host.Store.Snapshots().Length);
        Assert.Equal(60, host.Store.Snapshots().Sum(x => x.CoveredSeconds));
        Assert.Equal(2, host.Store.Snapshots().Sum(x => x.TotalKills));
        Assert.Equal(0, host.Surface.Subscriptions);
    }

    [Fact]
    public void DisposeFlushesOnceAndUnsubscribes()
    {
        var host = Enabled(); var clock = new Clock();
        var recorder = new KillStatistics(host, clock);
        host.Surface.Send(1, "You killed Drudge!"); clock.Advance(7);
        recorder.Dispose(); recorder.Dispose(); recorder.Tick();
        host.Surface.Send(2, "You killed Drudge!");
        var snapshot = Assert.Single(host.Store.Snapshots());
        Assert.Equal(1, snapshot.TotalKills);
        Assert.True(snapshot.Closed);
        Assert.Equal(clock.GetUtcNow(), snapshot.EndedUtc);
        Assert.Equal(7, snapshot.CoveredSeconds);
        Assert.Equal(2, host.Store.Writes);
        Assert.Equal(0, host.Surface.Subscriptions);
    }

    [Fact]
    public void StorageFailureIsReportedAndRecordingStopsWithoutRetryOrFalseZero()
    {
        var host = Enabled(); var clock = new Clock();
        using var recorder = new KillStatistics(host, clock);
        host.Surface.Send(1, "You killed Drudge!");
        host.Store.Fail = true; clock.Advance(60); recorder.Tick();
        clock.Advance(60); recorder.Tick(); recorder.Dispose();
        Assert.Single(host.Errors);
        Assert.Equal(0, host.Surface.Subscriptions);
        Assert.Equal(2, host.Store.Writes);
        Assert.Equal(0, host.Notifications.Subscriptions);
        var saved = Assert.Single(host.Store.Snapshots());
        Assert.False(saved.Closed);
        Assert.Equal(0, saved.CoveredSeconds);
    }

    private static Host Enabled()
    {
        var host = new Host(); host.Store.Values[KillStatistics.EnableKey] = "true"; return host;
    }
    private sealed class Clock : TimeProvider
    {
        private long _seconds;
        public long UtcOffset;
        public override long TimestampFrequency => 1;
        public override long GetTimestamp() => _seconds;
        public override DateTimeOffset GetUtcNow() => DateTimeOffset.UnixEpoch.AddSeconds(_seconds + UtcOffset);
        public void Advance(long seconds) => _seconds += seconds;
    }
    private sealed class Store : IPluginStorage
    {
        public bool IsAvailable => true;
        public Dictionary<string, string> Values { get; } = [];
        public int Writes; public bool Fail; public bool FailRead;
        public string? ReadText(string key) => FailRead
            ? throw new IOException("Test read failure") : Values.GetValueOrDefault(key);
        public void WriteText(string key, string value)
        {
            Writes++;
            if (Fail) throw new IOException("Test failure");
            Values[key] = value;
        }
        public KillStatistics.Snapshot[] Snapshots() => Values.Where(x => x.Key.StartsWith(KillStatistics.SessionsPrefix, StringComparison.Ordinal))
            .Select(x => JsonSerializer.Deserialize<KillStatistics.Snapshot>(x.Value)!).ToArray();
    }
    private sealed class Host : IPluginHost, IPluginLogger
    {
        public Store Store { get; } = new(); public Surface Surface { get; } = new();
        public Notifications Notifications { get; } = new();
        public List<string> Errors { get; } = [];
        public IPluginStorage Storage => Store;
        public IAutomationSurface Automation => Surface;
        public bool HasUi => false;
        public IPluginLogger Log => this;
        public IGameState State => throw new InvalidOperationException("Unexpected world polling");
        public IEvents Events => Notifications;
        public ISelectionService Selection => throw new InvalidOperationException("Unexpected selection polling");
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public void Info(string text) { }
        public void Warn(string text) { }
        public void Error(string text, Exception? error = null) => Errors.Add(text);
    }
    private sealed class Notifications : IEvents
    {
        private Action? _logoff;
        private Action? _login;
        public int Subscriptions => (_logoff?.GetInvocationList().Length ?? 0) +
            (_login?.GetInvocationList().Length ?? 0);
        public event Action Logoff { add => _logoff += value; remove => _logoff -= value; }
        public event Action LoginComplete { add => _login += value; remove => _login -= value; }
        public event Action<WorldEntitySnapshot> EntitySpawned { add { } remove { } }
        public event Action<double> Tick { add { } remove { } }
        public void RaiseLogoff() => _logoff?.Invoke();
        public void RaiseLoginComplete() => _login?.Invoke();
    }
    private sealed class Surface : IAutomationSurface, ICharacterInfo, IPluginChat
    {
        public bool IsAvailable => true;
        public int CharacterReads;
        public ICharacterInfo Character { get { CharacterReads++; return this; } }
        public ISpellCatalog Spells => NoOpAutomationSurface.Instance;
        public IMagicCommands Magic => NoOpAutomationSurface.Instance;
        public IPluginChat Chat => this;
        public bool IsInWorld { get; set; } = true;
        public string Name { get; set; } = "First";
        public string WorldName { get; set; } = "Coldeve";
        public uint ObjectId { get; set; } = 1;
        public uint CurrentHealth => 0; public uint MaxHealth => 0;
        public uint CurrentStamina => 0; public uint MaxStamina => 0;
        public uint CurrentMana => 0; public uint MaxMana => 0;
        public IReadOnlyList<PluginSkillInfo> Skills => [];
        public IReadOnlyList<PluginAttributeInfo> Attributes => [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments => [];
        public bool TryGetSkill(uint id, out PluginSkillInfo skill) { skill = default; return false; }
        private Action<PluginChatMessage>? _received;
        public int Subscriptions;
        public event Action<PluginChatMessage> Received
        {
            add { _received += value; Subscriptions++; }
            remove { if (_received?.GetInvocationList().Contains(value) == true) Subscriptions--; _received -= value; }
        }
        public void PostSystemMessage(string text) { }
        public void Send(ulong sequence, string text, int type = 0) =>
            _received?.Invoke(new(sequence, 0, 0, "", text, "") { LogTextType = type });
    }
}

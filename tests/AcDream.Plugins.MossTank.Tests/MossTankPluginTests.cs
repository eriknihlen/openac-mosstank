using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class MossTankPluginTests
{
    [Fact]
    public void EnableRegistersTheLootClassifierAndDisableRevokesIt()
    {
        var lootClassifiers = new RecordingLootClassifierRegistry();
        var host = new FakeHost(lootClassifiers);
        var plugin = new MossTankPlugin();

        plugin.Initialize(host);
        Assert.Empty(lootClassifiers.Registered);

        plugin.Enable();

        RecordingLootClassifierRegistry.Entry registered =
            Assert.Single(lootClassifiers.Registered);
        Assert.Equal("moss-tank", registered.ClassifierId);
        Assert.Equal("MossTank", registered.DisplayName);
        Assert.IsType<MossTankLootClassifier>(registered.Classifier);
        Assert.False(registered.Revoked);

        plugin.Disable();

        Assert.True(registered.Revoked);
    }

    /// <summary>
    /// Mutation pin: leave the panel in reset-only shutdown. Its equipment
    /// observers (the equipment tracker's and the equip-profile run's)
    /// remain attached, so loading a second plugin instance adds duplicate
    /// observers.
    /// Mutation executed: <c>_panel?.Dispose() was replaced with _panel?.Disable()</c>.
    /// </summary>
    [Fact]
    public void ReloadDetachesEquipmentObserverBeforeTheNextPanelSubscribes()
    {
        var lootClassifiers = new RecordingLootClassifierRegistry();
        var host = new FakeHost(lootClassifiers);
        var first = new MossTankPlugin();

        first.Initialize(host);
        first.Enable();
        Assert.Equal(2, host.AutomationForTests.PlacementObserverCount);

        first.Disable();
        Assert.Equal(0, host.AutomationForTests.PlacementObserverCount);

        var second = new MossTankPlugin();
        second.Initialize(host);
        second.Enable();
        Assert.Equal(2, host.AutomationForTests.PlacementObserverCount);

        second.Disable();
        Assert.Equal(0, host.AutomationForTests.PlacementObserverCount);
    }

    /// <summary>
    /// /vt and /ub are two words for one dispatcher, and both are given up
    /// when the plugin is. Mutation: dropping the /ub registration (or its
    /// disposal in Disable) fails the second or last assertion.
    /// </summary>
    [Fact]
    public void EnableRegistersVtAndUbOnOneHandlerAndDisableRevokesBoth()
    {
        var commands = new RecordingCommandRegistry();
        var host = new FakeHost(new RecordingLootClassifierRegistry(), commands);
        var plugin = new MossTankPlugin();
        plugin.Initialize(host);

        plugin.Enable();

        Assert.Equal(["vt", "ub"], commands.Registered.Select(entry => entry.Verb));
        Assert.Equal(
            commands.Registered[0].Handler.Method,
            commands.Registered[1].Handler.Method);
        Assert.Same(
            commands.Registered[0].Handler.Target,
            commands.Registered[1].Handler.Target);
        Assert.All(commands.Registered, entry => Assert.False(entry.Revoked));

        plugin.Disable();

        Assert.All(commands.Registered, entry => Assert.True(entry.Revoked));
    }

    private sealed class RecordingCommandRegistry : IPluginCommandRegistry
    {
        public List<Entry> Registered { get; } = [];

        public IDisposable Register(string verb, Action<PluginCommand> handler)
        {
            var entry = new Entry(verb, handler);
            Registered.Add(entry);
            return new Revocation(entry);
        }

        internal sealed class Entry(string verb, Action<PluginCommand> handler)
        {
            public string Verb { get; } = verb;
            public Action<PluginCommand> Handler { get; } = handler;
            public bool Revoked { get; internal set; }
        }

        private sealed class Revocation(Entry entry) : IDisposable
        {
            public void Dispose() => entry.Revoked = true;
        }
    }

    private sealed class RecordingLootClassifierRegistry : IPluginLootClassifierRegistry
    {
        public List<Entry> Registered { get; } = [];

        public IDisposable Register(
            string classifierId,
            string displayName,
            IPluginLootClassifier classifier)
        {
            var entry = new Entry(classifierId, displayName, classifier);
            Registered.Add(entry);
            return new Revocation(entry);
        }

        internal sealed class Entry(
            string classifierId,
            string displayName,
            IPluginLootClassifier classifier)
        {
            public string ClassifierId { get; } = classifierId;
            public string DisplayName { get; } = displayName;
            public IPluginLootClassifier Classifier { get; } = classifier;
            public bool Revoked { get; internal set; }
        }

        private sealed class Revocation(Entry entry) : IDisposable
        {
            public void Dispose() => entry.Revoked = true;
        }
    }

    private sealed class FakeHost(
        IPluginLootClassifierRegistry lootClassifiers,
        IPluginCommandRegistry? commands = null) : IPluginHost
    {
        public IPluginCommandRegistry Commands { get; } =
            commands ?? NoOpPluginCommandRegistry.Instance;
        public bool HasUi => false;
        public IPluginLogger Log { get; } = new NoOpLogger();
        public IGameState State { get; } = new NoOpState();
        public IEvents Events { get; } = new NoOpEvents();
        public ISelectionService Selection { get; } = new NoOpSelection();
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public MinimalAutomation AutomationForTests { get; } = new();
        public IAutomationSurface Automation => AutomationForTests;
        public IPluginLootClassifierRegistry LootClassifiers { get; } =
            lootClassifiers;
    }

    // A minimal but complete IAutomationSurface: only the four members
    // without an interface default (IsAvailable, Character, Spells, Magic,
    // Chat) plus their own required leaves. Everything else on
    // IAutomationSurface falls through to its NoOpAutomationSurface
    // default, which is fine for Enable()/Disable() -- neither touches it.
    private sealed class MinimalAutomation
        : IAutomationSurface, ICharacterInfo, ISpellCatalog, IMagicCommands,
          IPluginChat, IEquipmentAutomation
    {
        private Action<PluginEquipmentObservation>? _placementObserved;

        public bool IsAvailable => true;
        public ICharacterInfo Character => this;
        public ISpellCatalog Spells => this;
        public IMagicCommands Magic => this;
        public IPluginChat Chat => this;
        public IEquipmentAutomation Equipment => this;
        public bool IsBusy => false;
        public int PlacementObserverCount { get; private set; }
        public event Action<PluginEquipmentObservation> PlacementObserved
        {
            add
            {
                _placementObserved += value;
                PlacementObserverCount++;
            }
            remove
            {
                _placementObserved -= value;
                PlacementObserverCount--;
            }
        }

        public bool IsInWorld => true;
        public uint ObjectId => 0u;
        public uint CurrentHealth => 0u;
        public uint MaxHealth => 0u;
        public uint CurrentStamina => 0u;
        public uint MaxStamina => 0u;
        public uint CurrentMana => 0u;
        public uint MaxMana => 0u;
        public IReadOnlyList<PluginSkillInfo> Skills { get; } = [];
        public IReadOnlyList<PluginAttributeInfo> Attributes { get; } = [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments { get; } = [];
        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            skill = default;
            return false;
        }

        public IReadOnlyList<PluginSpellInfo> KnownSelfBuffs { get; } = [];
        public bool TryGet(uint spellId, out PluginSpellInfo info)
        {
            info = default;
            return false;
        }

        public bool IsCasting => false;
        public PluginCastGate EvaluateGate(uint spellId) =>
            PluginCastGate.Refused;
        public bool Cast(uint spellId) => false;

        public void PostSystemMessage(string text) { }
    }

    private sealed class NoOpLogger : IPluginLogger
    {
        public void Info(string message) { }
        public void Warn(string message) { }
        public void Error(string message, Exception? error = null) { }
    }

    private sealed class NoOpState : IGameState
    {
        public IReadOnlyList<WorldEntitySnapshot> Entities { get; } = [];
    }

    private sealed class NoOpEvents : IEvents
    {
        public event Action<WorldEntitySnapshot> EntitySpawned
        {
            add { }
            remove { }
        }
        public event Action<double> Tick
        {
            add { }
            remove { }
        }
    }

    private sealed class NoOpSelection : ISelectionService
    {
        public uint? SelectedObjectId => null;
        public uint? PreviousObjectId => null;
        public event Action<SelectionChangedEvent>? Changed;
        public bool Select(uint objectId)
        {
            Changed?.Invoke(default);
            return false;
        }
        public bool Clear() => false;
    }
}

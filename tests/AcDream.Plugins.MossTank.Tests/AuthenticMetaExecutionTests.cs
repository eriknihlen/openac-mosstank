using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class AuthenticMetaExecutionTests
{
    [Fact]
    public void ControlledNativeMetaTransitionsAndSetsThePanelOptionOwner()
    {
        string fixture = File.ReadAllText(Path.Combine(
            AppContext.BaseDirectory,
            "Fixtures",
            "vt-proof",
            "rynthify-controlled-meta.met"));
        Assert.True(VtankMetaProfileSerializer.TryLoad(
            fixture,
            out MetaProfile profile,
            out string error), error);
        Assert.Equal(2, profile.Rules.Count);

        var profiles = new MemoryStorage();
        var automation = new Automation();
        var host = new Host(automation, profiles);
        var panel = new MossTankPanel(host);
        using var expressions = new MossTankExpressionRuntime(host);
        var engine = new MetaEngine(
            host,
            expressions,
            profile,
            new MetaServices { SetOption = panel.SetMetaOption });

        ExecuteVtank(panel, "opt set LootPriorityBoost false");
        Assert.False(panel.LootPriorityBoostEnabled);
        engine.SetEnabled(true);

        engine.EvaluatePass();
        Assert.Equal("RynthReady", engine.CurrentState);
        Assert.False(panel.LootPriorityBoostEnabled);

        engine.EvaluatePass();
        Assert.True(panel.LootPriorityBoostEnabled);
    }

    [Fact]
    public void AMetaPassListsTheObjectsOnceNotOncePerCondition()
    {
        // A hunting meta checks many inventory counts every pass; each one
        // used to copy every object the client knows.
        var automation = new Automation();
        var host = new Host(automation, new MemoryStorage());
        using var expressions = new MossTankExpressionRuntime(host);
        static MetaRule Carrying(string name) => new()
        {
            Condition = new MetaCondition
            {
                Kind = MetaConditionKind.InventoryItemCountGreaterThanOrEqual,
                Text = name,
                Number = 5,
            },
        };
        var engine = new MetaEngine(
            host,
            expressions,
            new MetaProfile
            {
                Rules =
                [
                    Carrying("Mana Stone"),
                    Carrying("Pyreal"),
                    Carrying("Arrow"),
                    Carrying("Healing Kit"),
                ],
            });
        engine.SetEnabled(true);

        int before = automation.WorldCaptureCount;
        engine.EvaluatePass();
        Assert.Equal(1, automation.WorldCaptureCount - before);

        before = automation.WorldCaptureCount;
        expressions.Evaluate("getitemcountininventorybyname[`Pyreal`]");
        expressions.Evaluate("getitemcountininventorybyname[`Pyreal`]");
        Assert.Equal(2, automation.WorldCaptureCount - before);
    }

    [Fact]
    public void AnActionInAMetaPassStartsAFreshObjectList()
    {
        var automation = new Automation();
        var host = new Host(automation, new MemoryStorage());
        using var expressions = new MossTankExpressionRuntime(host);
        static MetaCondition Carrying(string name) => new()
        {
            Kind = MetaConditionKind.InventoryItemCountLessThanOrEqual,
            Text = name,
            Number = 5,
        };
        var engine = new MetaEngine(
            host,
            expressions,
            new MetaProfile
            {
                Rules =
                [
                    new MetaRule
                    {
                        Condition = Carrying("Pyreal"),
                        Action = new MetaAction
                        {
                            Kind = MetaActionKind.ExpressionAction,
                            Text = "setvar[x,1]",
                        },
                    },
                    new MetaRule { Condition = Carrying("Arrow") },
                ],
            });
        engine.SetEnabled(true);

        int before = automation.WorldCaptureCount;
        engine.EvaluatePass();

        Assert.Equal(2, automation.WorldCaptureCount - before);
    }

    [Fact]
    public void ControlledNativeMetaIsReselectedAfterIdentitySettlesAndThenRuns()
    {
        string fixture = File.ReadAllText(Path.Combine(
            AppContext.BaseDirectory,
            "Fixtures",
            "vt-proof",
            "rynthify-controlled-meta.met"));
        var profiles = new MemoryStorage();
        var automation = new Automation { Name = "Privileged Meta Tester" };
        var host = new Host(automation, profiles);
        var imports = Assert.IsType<MemoryStorage>(host.Storage);
        imports.Text["imports/rynthify-controlled-meta.met"] = fixture;
        var panel = new MossTankPanel(host);
        panel.OnTick(0.1d);

        ExecuteVtank(panel, "meta load rynthify-controlled-meta.met");
        Assert.Equal("rynthify-controlled-meta", panel.SelectedMetaProfile);

        automation.Name = "Meta Tester";
        panel.OnTick(0.1d);

        Assert.Equal(MossTankMetaProfileStore.ByCharacter, panel.SelectedMetaProfile);
        ExecuteVtank(panel, "meta load rynthify-controlled-meta.met");
        ExecuteVtank(panel, "opt set LootPriorityBoost false");
        ExecuteVtank(panel, "opt set EnableMeta true");

        panel.OnTick(MetaEngine.DecisionIntervalSeconds + 0.01d);
        panel.OnTick(MetaEngine.DecisionIntervalSeconds + 0.01d);

        Assert.False(panel.CombatMacroRunning);
        Assert.Equal(MetaEngine.DefaultState, panel.MetaState);
        Assert.False(panel.LootPriorityBoostEnabled);

        ExecuteVtank(panel, "start");
        panel.OnTick(MetaEngine.DecisionIntervalSeconds + 0.01d);
        panel.OnTick(MetaEngine.DecisionIntervalSeconds + 0.01d);

        Assert.True(panel.CombatMacroRunning);
        Assert.Equal("rynthify-controlled-meta", panel.SelectedMetaProfile);
        Assert.Equal("RynthReady", panel.MetaState);
        Assert.True(panel.LootPriorityBoostEnabled);
    }

    [Fact]
    public void FullNeftetHuntCallAndWorkerExecuteWithoutExpressionParseErrors()
    {
        string fixture = ReadNeftetFixture();
        var profiles = new MemoryStorage();
        profiles.Text["mosstank/metas/neftet.af"] = fixture;
        var automation = new Automation();
        var host = new Host(automation, profiles);
        var panel = new MossTankPanel(host);
        automation.RouteVtankCommand = arguments => panel.ExecuteVtankCommand(
            new PluginCommand("vt", arguments, "/vt " + arguments));
        panel.SelectMetaProfile("neftet");
        Assert.True(MetafSerializer.TryLoadMeta(
            fixture,
            automation.Spells,
            out MetaProfile profile,
            out string error), error);
        using var expressions = new MossTankExpressionRuntime(host);
        var engine = new MetaEngine(host, expressions, profile);

        ExecuteVtank(panel, "opt set EnableCombat false");
        ExecuteVtank(panel, "opt set IdlePeaceMode true");
        expressions.Evaluate("setvar[isLeader,1]");
        expressions.Evaluate("setvar[navRoute,`neftet_route`]");
        expressions.Evaluate("setvar[pickFlowers,0]");
        engine.SetEnabled(true);
        engine.Transition("Hunt");

        automation.AddChat("[Fellowship] Horan says, \"#toggle_flowers\"");
        engine.OnTick(MetaEngine.DecisionIntervalSeconds + 0.01d);

        Assert.True(panel.CombatEnabled);
        Assert.False(panel.IdlePeaceModeEnabled);
        Assert.True(
            engine.CurrentState == "toggle_flowers",
            $"state={engine.CurrentState}; status={engine.Status}");

        engine.OnTick(MetaEngine.DecisionIntervalSeconds + 0.01d);

        Assert.True(expressions.Evaluate("getvar[pickFlowers]").IsTruthy);
        Assert.Equal("Hunt", engine.CurrentState);
        Assert.DoesNotContain("failed", engine.Status, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void SelectedNeftetRulesRunCapturedChatVtankOptionsAndConfiguredCallReturn()
    {
        string fixture = ReadNeftetFixture();
        var profiles = new MemoryStorage();
        profiles.Text["mosstank/metas/neftet.af"] = fixture;
        var automation = new Automation();
        var host = new Host(automation, profiles);
        var panel = new MossTankPanel(host);
        automation.RouteVtankCommand = arguments => panel.ExecuteVtankCommand(
            new PluginCommand("vt", arguments, "/vt " + arguments));

        panel.SelectMetaProfile("neftet");
        Assert.Equal("neftet", panel.SelectedMetaProfile);
        Assert.Equal(300, panel.MetaRows.Count);
        Assert.True(MetafSerializer.TryLoadMeta(
            fixture,
            automation.Spells,
            out MetaProfile profile,
            out string error), error);
        MetaRule[] scenarioRules = profile.Rules.Where(static rule =>
            (rule.State == "Hunt"
                && rule.Action.Kind == MetaActionKind.ChatCommand
                && (rule.Action.Text == "/vt opt set enablecombat true"
                    || rule.Action.Text == "/vt opt set IdlePeaceMode false"))
            || (rule.Action.Kind == MetaActionKind.CallMetaState
                && rule.Action.Text == "toggle_flowers")
            || rule.State == "toggle_flowers"
            || rule.Condition.Children.Any(static condition =>
                condition.Kind == MetaConditionKind.ChatMessageCapture
                && condition.Text.Contains(
                    "has left your Fellowship",
                    StringComparison.Ordinal))).ToArray();
        Assert.Equal(5, scenarioRules.Length);
        MetaRule worker = Assert.Single(
            scenarioRules,
            static rule => rule.State == "toggle_flowers");
        MetaAction returnAction = Assert.Single(
            worker.Action.Children,
            static action => action.Kind == MetaActionKind.ReturnFromCall);
        scenarioRules[Array.IndexOf(scenarioRules, worker)] = new MetaRule
        {
            State = worker.State,
            Condition = worker.Condition,
            Action = new MetaAction
            {
                Kind = MetaActionKind.All,
                Children = [returnAction],
            },
        };
        var scenario = new MetaProfile { Rules = [.. scenarioRules] };
        using var expressions = new MossTankExpressionRuntime(host);
        var engine = new MetaEngine(host, expressions, scenario);

        ExecuteVtank(panel, "opt set EnableCombat false");
        ExecuteVtank(panel, "opt set IdlePeaceMode true");
        expressions.Evaluate("setvar[isLeader,0]");
        expressions.Evaluate("setvar[followTarget,`Horan`]");
        engine.SetEnabled(true);
        engine.Transition("Hunt");

        automation.AddChat("Horan has left your Fellowship");
        engine.OnTick(MetaEngine.DecisionIntervalSeconds + 0.01d);

        Assert.True(panel.CombatEnabled);
        Assert.False(panel.IdlePeaceModeEnabled);
        Assert.Equal(
            "Horan",
            expressions.Evaluate("getvar[`capturegroup_who`]").AsString());
        Assert.Equal("turn_in_quests", engine.CurrentState);

        expressions.Evaluate("setvar[isLeader,1]");
        Assert.Equal(1d, expressions.Evaluate("getvar[isLeader]").AsNumber());
        engine.Transition("Hunt");
        const string toggleMessage =
            "[Fellowship] Horan says, \"#toggle_flowers\"";
        MetaRule callRule = Assert.Single(scenario.Rules, static rule =>
            rule.Action.Kind == MetaActionKind.CallMetaState
            && rule.Action.Text == "toggle_flowers");
        Assert.Equal("Hunt", callRule.Action.SecondaryText);
        MetaCondition chatCondition = Assert.Single(
            callRule.Condition.Children,
            static condition => condition.Kind == MetaConditionKind.ChatMessage);
        Assert.Matches(chatCondition.Text, toggleMessage);
        automation.AddChat(toggleMessage);
        engine.OnTick(MetaEngine.DecisionIntervalSeconds + 0.01d);
        Assert.True(
            engine.CurrentState == "toggle_flowers",
            $"state={engine.CurrentState}; status={engine.Status}; fired={engine.FiredRuleCount}");

        engine.OnTick(MetaEngine.DecisionIntervalSeconds + 0.01d);
        Assert.Equal("Hunt", engine.CurrentState);
    }

    private static string ReadNeftetFixture() => File.ReadAllText(Path.Combine(
            AppContext.BaseDirectory,
            "Fixtures",
            "vtank",
            "af",
            "neftet.af"));

    private static void ExecuteVtank(MossTankPanel panel, string arguments) =>
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt",
            arguments,
            "/vt " + arguments));

    private sealed class Host(
        Automation automation,
        IPluginStorage profiles) : IPluginHost
    {
        public bool HasUi => false;
        public IPluginLogger Log { get; } = new Logger();
        public IGameState State { get; } = new State();
        public IEvents Events { get; } = new Events();
        public ISelectionService Selection { get; } = new Selection();
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IPluginStorage Storage { get; } = new MemoryStorage();
        public IAutomationSurface Automation { get; } = automation;
        public IPluginStorage VtankProfiles { get; } = profiles;
    }

    private sealed class Automation :
        IAutomationSurface,
        ICharacterInfo,
        IPluginChat,
        INavigationAutomation,
        IWorldObjectAutomation
    {
        private ulong _sequence;
        private readonly List<PluginChatMessage> _chat = [];
        private readonly PluginWorldObject _landscape = new(
            10u,
            1u,
            "Rock",
            PluginObjectClass.Misc,
            0u,
            0u,
            0u)
        {
            HasPosition = true,
            Position = new PluginNavigationPosition(
                0x00010001u,
                10d,
                10d,
                0d,
                0f,
                true),
        };

        public Action<string>? RouteVtankCommand { get; set; }
        public bool IsAvailable => true;
        public ICharacterInfo Character => this;
        public ISpellCatalog Spells => NoOpAutomationSurface.Instance;
        public IMagicCommands Magic => NoOpAutomationSurface.Instance;
        public IPluginChat Chat => this;
        public INavigationAutomation Navigation => this;
        public IWorldObjectAutomation Objects => this;
        public bool IsInWorld => true;
        public string Name { get; set; } = "Meta Tester";
        public string WorldName => "Coldeve";
        public uint ObjectId => 1u;
        public uint CurrentHealth => 100u;
        public uint MaxHealth => 100u;
        public uint CurrentStamina => 100u;
        public uint MaxStamina => 100u;
        public uint CurrentMana => 100u;
        public uint MaxMana => 100u;
        public IReadOnlyList<PluginSkillInfo> Skills => [];
        public IReadOnlyList<PluginAttributeInfo> Attributes => [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments => [];
        public PluginNavigationSnapshot Snapshot => new(
            true,
            false,
            ObjectId,
            new PluginNavigationPosition(
                0x00010001u,
                0d,
                0d,
                0d,
                0f,
                true),
            false,
            false);

        public void AddChat(string text) => _chat.Add(new PluginChatMessage(
            ++_sequence,
            2u,
            3,
            "Horan",
            text,
            "Fellowship"));

        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            skill = default;
            return false;
        }

        public IReadOnlyList<PluginChatMessage> CaptureMessages(ulong afterSequence) =>
            _chat.Where(message => message.Sequence > afterSequence).ToArray();

        public void PostSystemMessage(string text)
        {
        }

        public bool Submit(string text)
        {
            const string prefix = "/vt ";
            if (text.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))
                RouteVtankCommand?.Invoke(text[prefix.Length..]);
            return true;
        }

        public int WorldCaptureCount { get; private set; }

        IReadOnlyList<PluginWorldObject> IWorldObjectAutomation.CaptureObjects() =>
            ++WorldCaptureCount > 0 ? [_landscape] : [];

        IReadOnlyList<PluginNavigationObject> INavigationAutomation.CaptureObjects() =>
            [];

        public bool TryGet(uint objectId, out PluginWorldObject value)
        {
            value = _landscape;
            return objectId == _landscape.ObjectId;
        }

        public bool TryGetObject(
            uint objectId,
            out PluginNavigationObject value)
        {
            value = default;
            return false;
        }

        public PluginNavigationCommandStatus SetMovementIntent(
            in PluginMovementIntent intent) => PluginNavigationCommandStatus.Unavailable;

        public PluginNavigationCommandStatus ClearMovementIntent() =>
            PluginNavigationCommandStatus.Unavailable;

    }

    private sealed class MemoryStorage : IPluginStorage
    {
        public Dictionary<string, string> Text { get; } =
            new(StringComparer.Ordinal);
        public bool IsAvailable => true;
        public string? ReadText(string key) =>
            Text.TryGetValue(key, out string? value) ? value : null;
        public IReadOnlyList<string> List(string prefix) => Text.Keys
            .Where(key => prefix.Length == 0
                || key.StartsWith(prefix + "/", StringComparison.Ordinal))
            .OrderBy(static key => key, StringComparer.Ordinal)
            .ToArray();
        public void WriteText(string key, string content) => Text[key] = content;
        public bool Delete(string key) => Text.Remove(key);
    }

    private sealed class Logger : IPluginLogger
    {
        public void Info(string message)
        {
        }

        public void Warn(string message)
        {
        }

        public void Error(string message, Exception? exception = null)
        {
        }
    }

    private sealed class State : IGameState
    {
        public IReadOnlyList<WorldEntitySnapshot> Entities => [];
    }

    private sealed class Events : IEvents
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

    private sealed class Selection : ISelectionService
    {
        public uint? SelectedObjectId => null;
        public uint? PreviousObjectId => null;

        public event Action<SelectionChangedEvent> Changed
        {
            add { }
            remove { }
        }

        public bool Select(uint objectId) => true;
        public bool Clear() => true;
    }
}

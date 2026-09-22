using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The client's own notifications as meta condition edges. There is one
/// engine: an event handler is a rule in the profile whose condition is the
/// edge, and the event's fields reach expressions as <c>gevt_*</c> session
/// variables the same way a chat capture's groups do.
/// </summary>
public sealed class MetaGameEventTests
{
    /// <summary>
    /// A login-complete rule fires on the pass after the client says the
    /// player is in, carries the character's id and name, and fires AGAIN on
    /// the next login without a state change: an event edge re-arms its
    /// rule, unlike an ordinary rule that fires once per state entry.
    /// Mutation: dropping the re-arm leaves the count at one.
    /// </summary>
    [Fact]
    public void LoginCompleteFiresWithCharacterVariablesAndReArms()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var profile = new MetaProfile
        {
            Rules =
            [
                Rule(MetaConditionKind.LoginComplete, MetaActionKind.ExpressionAction,
                    "setvar[`logins`,getvar[`logins`]+1]"),
            ],
        };
        using var engine = new MetaEngine(host, expressions, profile);
        engine.SetEnabled(true);

        engine.OnTick(MetaEngine.DecisionIntervalSeconds);
        Assert.Equal(0d, expressions.Evaluate("getvar[`logins`]").AsNumber());

        host.Events.RaiseLoginComplete();
        engine.OnTick(MetaEngine.DecisionIntervalSeconds);
        Assert.Equal(1d, expressions.Evaluate("getvar[`logins`]").AsNumber());
        Assert.Equal(1d, expressions.Evaluate("getvar[`gevt_id`]").AsNumber());
        Assert.Equal("Meta Tester", expressions.Evaluate("getvar[`gevt_name`]").AsString());

        // No second firing without a second event.
        engine.OnTick(MetaEngine.DecisionIntervalSeconds);
        Assert.Equal(1d, expressions.Evaluate("getvar[`logins`]").AsNumber());

        host.Events.RaiseLoginComplete();
        engine.OnTick(MetaEngine.DecisionIntervalSeconds);
        Assert.Equal(2d, expressions.Evaluate("getvar[`logins`]").AsNumber());
    }

    /// <summary>
    /// The login-complete edge survives the new-session reset, because the
    /// client announces the login before the plugin's own session
    /// bookkeeping has run. Mutation: clearing it in ResetSession loses
    /// every login handler.
    /// </summary>
    [Fact]
    public void LoginCompleteEdgeSurvivesTheSessionReset()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var profile = new MetaProfile
        {
            Rules =
            [
                Rule(MetaConditionKind.LoginComplete, MetaActionKind.ExpressionAction,
                    "setvar[`logins`,1]"),
            ],
        };
        using var engine = new MetaEngine(host, expressions, profile);

        host.Events.RaiseLoginComplete();
        engine.ResetSession();
        engine.SetEnabled(true);
        engine.OnTick(MetaEngine.DecisionIntervalSeconds);
        Assert.Equal(1d, expressions.Evaluate("getvar[`logins`]").AsNumber());
    }

    /// <summary>
    /// Logoff is answered at once, not on the next pass: the client raises it
    /// before the session is torn down and there is no next pass in that
    /// session. Mutation: riding the tick leaves the handler never run.
    /// </summary>
    [Fact]
    public void LogoffRunsItsPassImmediately()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var profile = new MetaProfile
        {
            Rules =
            [
                Rule(MetaConditionKind.Logoff, MetaActionKind.ExpressionAction,
                    "setvar[`logoffs`,getvar[`logoffs`]+1]"),
            ],
        };
        using var engine = new MetaEngine(host, expressions, profile);
        engine.SetEnabled(true);

        host.Events.RaiseLogoff();
        Assert.Equal(1d, expressions.Evaluate("getvar[`logoffs`]").AsNumber());
        Assert.Equal("Meta Tester", expressions.Evaluate("getvar[`gevt_name`]").AsString());

        // The edge is consumed by that pass; the next tick does not repeat it.
        engine.OnTick(MetaEngine.DecisionIntervalSeconds);
        Assert.Equal(1d, expressions.Evaluate("getvar[`logoffs`]").AsNumber());
    }

    /// <summary>
    /// Containers opening and closing each carry the container's id.
    /// </summary>
    [Fact]
    public void ContainerEdgesCarryTheContainerId()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var profile = new MetaProfile
        {
            Rules =
            [
                Rule(MetaConditionKind.ContainerOpened, MetaActionKind.ExpressionAction,
                    "setvar[`opened`,getvar[`gevt_containerid`]]"),
                Rule(MetaConditionKind.ContainerClosed, MetaActionKind.ExpressionAction,
                    "setvar[`closed`,getvar[`gevt_containerid`]]"),
            ],
        };
        using var engine = new MetaEngine(host, expressions, profile);
        engine.SetEnabled(true);

        host.Events.RaiseContainerOpened(77u);
        engine.OnTick(MetaEngine.DecisionIntervalSeconds);
        Assert.Equal(77d, expressions.Evaluate("getvar[`opened`]").AsNumber());
        Assert.Equal(0d, expressions.Evaluate("getvar[`closed`]").AsNumber());

        host.Events.RaiseContainerClosed(77u);
        engine.OnTick(MetaEngine.DecisionIntervalSeconds);
        Assert.Equal(77d, expressions.Evaluate("getvar[`closed`]").AsNumber());
    }

    /// <summary>
    /// A portal transition is the arrival, not every notification along the
    /// way: only the completed report raises the edge, and it carries the
    /// destination cell.
    /// </summary>
    [Fact]
    public void PortalTransitionFiresOnCompletionOnly()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var profile = new MetaProfile
        {
            Rules =
            [
                Rule(MetaConditionKind.PortalTransition, MetaActionKind.ExpressionAction,
                    "setvar[`arrived`,getvar[`gevt_destinationcell`]]"),
            ],
        };
        using var engine = new MetaEngine(host, expressions, profile);
        engine.SetEnabled(true);

        host.Events.RaisePortalTransition(new PluginPortalTransition(
            1L, 1L, 0xA9B40012u, IsReady: true, IsMaterialized: false,
            IsCompleted: false, IsCancelled: false));
        engine.OnTick(MetaEngine.DecisionIntervalSeconds);
        Assert.Equal(0d, expressions.Evaluate("getvar[`arrived`]").AsNumber());

        host.Events.RaisePortalTransition(new PluginPortalTransition(
            2L, 1L, 0xA9B40012u, IsReady: true, IsMaterialized: true,
            IsCompleted: true, IsCancelled: false));
        engine.OnTick(MetaEngine.DecisionIntervalSeconds);
        Assert.Equal((double)0xA9B40012u, expressions.Evaluate("getvar[`arrived`]").AsNumber());
    }

    /// <summary>
    /// An item use and a confirmation request each expose their fields.
    /// </summary>
    [Fact]
    public void ItemUseAndConfirmationEdgesExposeTheirFields()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var profile = new MetaProfile
        {
            Rules =
            [
                Rule(MetaConditionKind.ItemUseCompleted, MetaActionKind.ExpressionAction,
                    "setvar[`used`,getvar[`gevt_sourceid`]*1000+getvar[`gevt_error`]]"),
                Rule(MetaConditionKind.ConfirmationRequested, MetaActionKind.ExpressionAction,
                    "setvar[`asked`,getvar[`gevt_text`]]"),
            ],
        };
        using var engine = new MetaEngine(host, expressions, profile);
        engine.SetEnabled(true);

        host.Events.RaiseItemUseCompleted(new PluginItemUseCompletion(5L, 42u, 0u, 3u));
        host.Events.RaiseConfirmationRequested(new PluginConfirmation(9u, 5, "Continue?"));
        engine.OnTick(MetaEngine.DecisionIntervalSeconds);
        Assert.Equal(42003d, expressions.Evaluate("getvar[`used`]").AsNumber());
        Assert.Equal("Continue?", expressions.Evaluate("getvar[`asked`]").AsString());
        Assert.Equal(9d, expressions.Evaluate("getvar[`gevt_contextid`]").AsNumber());
        Assert.Equal(5d, expressions.Evaluate("getvar[`gevt_confirmtype`]").AsNumber());
    }

    /// <summary>
    /// The death message comes from the client's own notification and the
    /// dropped items from the server's chat line, the two halves the
    /// reference handler reads.
    /// </summary>
    [Fact]
    public void DeathVariablesComeFromTheNotificationAndTheChatLine()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        using var engine = new MetaEngine(host, expressions, new MetaProfile());
        engine.SetEnabled(true);

        host.Events.RaiseLocalPlayerDied("You are liquified by Olthoi Slasher's attack!");
        host.Automation.Messages.Add(new PluginChatMessage(
            1u, 0u, 0, string.Empty, "You've lost 70 Pyreals, and your 4 Resister's Crystals!", string.Empty));
        engine.OnTick(MetaEngine.DecisionIntervalSeconds);
        Assert.Equal(
            "You are liquified by Olthoi Slasher's attack!",
            expressions.Evaluate("getvar[`gevt_deathmessage`]").AsString());
        Assert.Equal(
            "70 Pyreals, and your 4 Resister's Crystals!",
            expressions.Evaluate("getvar[`gevt_droppeditems`]").AsString());

        host.Automation.Messages.Add(new PluginChatMessage(
            2u, 0u, 0, string.Empty, "You have retained all your items.", string.Empty));
        engine.OnTick(MetaEngine.DecisionIntervalSeconds);
        Assert.Equal(
            string.Empty,
            expressions.Evaluate("getvar[`gevt_droppeditems`]").AsString());
    }

    /// <summary>
    /// The GameEvents switch on the settings page turns the handler rules
    /// off without touching the rest of the profile: a handler-shaped rule
    /// is skipped, an ordinary rule on the same edge is not.
    /// </summary>
    [Fact]
    public void HandlersSwitchSkipsOnlyHandlerShapedRules()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        MetaRule handler = Rule(MetaConditionKind.LoginComplete, MetaActionKind.ExpressionAction,
            "setvar[`handler`,1]");
        MetaRule ordinary = Rule(MetaConditionKind.LoginComplete, MetaActionKind.SetMetaState,
            "Next");
        var profile = new MetaProfile { Rules = [handler, ordinary] };
        using var engine = new MetaEngine(host, expressions, profile,
            new MetaServices { GameEventHandlersEnabled = static () => false });
        engine.SetEnabled(true);

        host.Events.RaiseLoginComplete();
        engine.OnTick(MetaEngine.DecisionIntervalSeconds);
        Assert.Equal(0d, expressions.Evaluate("getvar[`handler`]").AsNumber());
        Assert.Equal("Next", engine.CurrentState);
    }

    /// <summary>Disposing the engine lets go of the client's events.</summary>
    [Fact]
    public void DisposeUnsubscribesFromTheClient()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var engine = new MetaEngine(host, expressions, new MetaProfile());
        Assert.True(host.Events.HasSubscribers);
        engine.Dispose();
        Assert.False(host.Events.HasSubscribers);
    }

    private static MetaRule Rule(
        MetaConditionKind condition,
        MetaActionKind action,
        string text,
        string state = MetaEngine.DefaultState) => new()
        {
            State = state,
            Condition = new MetaCondition { Kind = condition },
            Action = new MetaAction { Kind = action, Text = text },
        };

    private sealed class Host : IPluginHost
    {
        public Host() => Automation = new Automation();
        public bool HasUi => false;
        public IPluginLogger Log { get; } = new Logger();
        public IGameState State { get; } = new State();
        public Events Events { get; } = new Events();
        IEvents IPluginHost.Events => Events;
        public ISelectionService Selection { get; } = new Selection();
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IPluginStorage Storage => NoOpPluginStorage.Instance;
        public Automation Automation { get; }
        IAutomationSurface IPluginHost.Automation => Automation;
    }

    internal sealed class Automation :
        IAutomationSurface,
        ICharacterInfo,
        IPluginChat,
        INavigationAutomation
    {
        public bool IsAvailable => true;
        public ICharacterInfo Character => this;
        public ISpellCatalog Spells => NoOpAutomationSurface.Instance;
        public IMagicCommands Magic => NoOpAutomationSurface.Instance;
        public IPluginChat Chat => this;
        public INavigationAutomation Navigation => this;
        public bool IsInWorld => true;
        public string Name => "Meta Tester";
        public string WorldName => "Coldeve";
        public string AccountName => "example-account";
        public uint ObjectId => 1;
        public uint CurrentHealth { get; set; } = 100;
        public uint MaxHealth => 100;
        public uint CurrentStamina => 100;
        public uint MaxStamina => 100;
        public uint CurrentMana => 100;
        public uint MaxMana => 100;
        public IReadOnlyList<PluginSkillInfo> Skills => [];
        public IReadOnlyList<PluginAttributeInfo> Attributes => [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments => [];
        public List<PluginChatMessage> Messages { get; } = [];
        public PluginNavigationPosition Position { get; set; } = new(
            0x7F7F0001u, 10d, 20d, 0d, 0f, true);

        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            skill = default;
            return false;
        }
        public IReadOnlyList<PluginChatMessage> CaptureMessages(ulong afterSequence) =>
            Messages.Where(message => message.Sequence > afterSequence).ToArray();
        public void PostSystemMessage(string text) { }
        public PluginNavigationSnapshot Snapshot => new(
            true, false, ObjectId, Position, false, false);
        public bool TryGetObject(uint objectId, out PluginNavigationObject value)
        {
            value = default;
            return false;
        }
        public PluginNavigationCommandStatus SetMovementIntent(
            in PluginMovementIntent intent) => PluginNavigationCommandStatus.Accepted;
        public PluginNavigationCommandStatus ClearMovementIntent() =>
            PluginNavigationCommandStatus.Accepted;
    }

    private sealed class Logger : IPluginLogger
    {
        public void Info(string message) { }
        public void Warn(string message) { }
        public void Error(string message, Exception? exception = null) { }
    }

    private sealed class State : IGameState
    {
        public IReadOnlyList<WorldEntitySnapshot> Entities => [];
    }

    /// <summary>The client's notifications, raisable by the test.</summary>
    internal sealed class Events : IEvents
    {
        private Action? _loginComplete;
        private Action? _logoff;
        private Action<string>? _died;
        private Action<PluginPortalTransition>? _portal;
        private Action<PluginItemUseCompletion>? _itemUse;
        private Action<uint>? _containerOpened;
        private Action<uint>? _containerClosed;
        private Action<PluginConfirmation>? _confirmation;

        public bool HasSubscribers =>
            _loginComplete is not null || _logoff is not null || _died is not null
            || _portal is not null || _itemUse is not null
            || _containerOpened is not null || _containerClosed is not null
            || _confirmation is not null;

        public event Action<WorldEntitySnapshot> EntitySpawned { add { } remove { } }
        public event Action<double> Tick { add { } remove { } }
        public event Action LoginComplete
        {
            add => _loginComplete += value;
            remove => _loginComplete -= value;
        }
        public event Action Logoff
        {
            add => _logoff += value;
            remove => _logoff -= value;
        }
        public event Action<string> LocalPlayerDied
        {
            add => _died += value;
            remove => _died -= value;
        }
        public event Action<PluginPortalTransition> PortalTransition
        {
            add => _portal += value;
            remove => _portal -= value;
        }
        public event Action<PluginItemUseCompletion> ItemUseCompleted
        {
            add => _itemUse += value;
            remove => _itemUse -= value;
        }
        public event Action<uint> ContainerOpened
        {
            add => _containerOpened += value;
            remove => _containerOpened -= value;
        }
        public event Action<uint> ContainerClosed
        {
            add => _containerClosed += value;
            remove => _containerClosed -= value;
        }
        public event Action<PluginConfirmation> ConfirmationRequested
        {
            add => _confirmation += value;
            remove => _confirmation -= value;
        }

        public void RaiseLoginComplete() => _loginComplete?.Invoke();
        public void RaiseLogoff() => _logoff?.Invoke();
        public void RaiseLocalPlayerDied(string message) => _died?.Invoke(message);
        public void RaisePortalTransition(PluginPortalTransition transition) =>
            _portal?.Invoke(transition);
        public void RaiseItemUseCompleted(PluginItemUseCompletion completion) =>
            _itemUse?.Invoke(completion);
        public void RaiseContainerOpened(uint id) => _containerOpened?.Invoke(id);
        public void RaiseContainerClosed(uint id) => _containerClosed?.Invoke(id);
        public void RaiseConfirmationRequested(PluginConfirmation confirmation) =>
            _confirmation?.Invoke(confirmation);
    }

    private sealed class Selection : ISelectionService
    {
        public uint? SelectedObjectId => null;
        public uint? PreviousObjectId => null;
        public event Action<SelectionChangedEvent> Changed { add { } remove { } }
        public bool Select(uint objectId) => true;
        public bool Clear() => true;
    }
}

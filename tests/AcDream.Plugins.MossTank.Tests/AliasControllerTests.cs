using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The alias controller: one interceptor on the chat surface that holds
/// every typed line against the alias list, stores the captures where the
/// expressions read them, and runs the actions through the expression
/// runtime and the chat bar.
/// </summary>
public sealed class AliasControllerTests
{
    /// <summary>
    /// A chat surface that behaves the way the host's does for the parts
    /// that matter here: it hands the interceptor every line it is asked to
    /// submit, and records what came out the other end.
    /// </summary>
    private sealed class Chat : IPluginChat
    {
        public List<Func<string, PluginChatInputDecision>> Interceptors { get; } = [];
        public List<string> Sent { get; } = [];
        public List<string> Posted { get; } = [];
        public int Revoked { get; private set; }

        public IDisposable RegisterInputInterceptor(Func<string, PluginChatInputDecision> intercept)
        {
            Interceptors.Add(intercept);
            return new Handle(() =>
            {
                Interceptors.Remove(intercept);
                Revoked++;
            });
        }

        public bool Submit(string text)
        {
            string line = text.Trim();
            if (line.Length == 0)
                return false;
            foreach (Func<string, PluginChatInputDecision> intercept in Interceptors.ToArray())
            {
                PluginChatInputDecision decision = intercept(line);
                if (decision.Action == PluginChatInputAction.Suppress)
                    return true;
                if (decision.Action == PluginChatInputAction.Rewrite)
                    return Submit(decision.Text ?? string.Empty);
            }
            Sent.Add(line);
            return true;
        }

        public void PostSystemMessage(string text) => Posted.Add(text);

        /// <summary>What the host does with a line the player typed.</summary>
        public PluginChatInputDecision Type(string line) =>
            Interceptors.Count == 1
                ? Interceptors[0](line.Trim())
                : throw new InvalidOperationException($"{Interceptors.Count} interceptors are installed.");

        private sealed class Handle(Action revoke) : IDisposable
        {
            public void Dispose() => revoke();
        }
    }

    private sealed class Automation(Chat chat) : IAutomationSurface
    {
        public bool IsAvailable => true;
        public ICharacterInfo Character => NoOpAutomationSurface.Instance;
        public IPluginChat Chat => chat;
        public ISpellCatalog Spells => NoOpAutomationSurface.Instance;
        public IMagicCommands Magic => NoOpAutomationSurface.Instance;
        public IItemAutomation Items => NoOpAutomationSurface.Instance;
        public INavigationAutomation Navigation => NoOpAutomationSurface.Instance;
        public IWorldObjectAutomation Objects => NoOpAutomationSurface.Instance;
    }

    private sealed class Logger : IPluginLogger
    {
        public List<string> Lines { get; } = [];
        public void Info(string message) => Lines.Add("info " + message);
        public void Warn(string message) => Lines.Add("warn " + message);
        public void Error(string message) => Lines.Add("error " + message);
        public void Error(string message, Exception? error) => Lines.Add("error " + message);
    }

    private sealed class Host(Chat chat) : IPluginHost
    {
        public bool HasUi => false;
        public Logger Logger { get; } = new();
        public IPluginLogger Log => Logger;
        public IGameState State { get; } = new GameState();
        public IEvents Events { get; } = new NoEvents();
        public ISelectionService Selection { get; } = new NoSelection();
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IPluginStorage Storage => NoOpPluginStorage.Instance;
        public IAutomationSurface Automation { get; } = new Automation(chat);

        private sealed class GameState : IGameState
        {
            public IReadOnlyList<WorldEntitySnapshot> Entities => [];
        }

        private sealed class NoEvents : IEvents
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
    }

    private sealed class Rig : IDisposable
    {
        public Chat Chat { get; } = new();
        public Host Host { get; }
        public MossTankExpressionRuntime Expressions { get; }
        public bool Enabled { get; set; } = true;
        public List<string> Lines { get; set; } = [];
        public AliasController Controller { get; }

        public Rig(params string[] lines)
        {
            Lines = [.. lines];
            Host = new Host(Chat);
            Expressions = new MossTankExpressionRuntime(Host);
            Controller = new AliasController(
                Host.Automation.Chat,
                Expressions,
                Host.Log,
                () => Enabled,
                () => Lines);
            Controller.Attach();
        }

        public void Dispose()
        {
            Controller.Dispose();
            Expressions.Dispose();
        }
    }

    [Fact]
    public void AttachInstallsOneInterceptorAndDisposeRevokesIt()
    {
        using var rig = new Rig();
        Assert.Single(rig.Chat.Interceptors);
        rig.Controller.Attach();
        Assert.Single(rig.Chat.Interceptors);

        rig.Controller.Dispose();
        Assert.Empty(rig.Chat.Interceptors);
        Assert.Equal(1, rig.Chat.Revoked);
    }

    [Fact]
    public void ALineNoAliasMatchesPasses()
    {
        using var rig = new Rig("^/ubpd$ = /ub propertydump");
        PluginChatInputDecision decision = rig.Chat.Type("/ub propertydump");
        Assert.Equal(PluginChatInputAction.Pass, decision.Action);
        Assert.Empty(rig.Chat.Sent);
    }

    [Fact]
    public void AChatCommandAliasRewritesTheTypedLine()
    {
        using var rig = new Rig("^/ubpd$ = /ub propertydump");
        PluginChatInputDecision decision = rig.Chat.Type("/ubpd");
        Assert.Equal(PluginChatInputAction.Rewrite, decision.Action);
        Assert.Equal("/ub propertydump", decision.Text);
        Assert.Empty(rig.Chat.Sent);
    }

    /// <summary>
    /// The switch is read on every line, so turning aliases off applies to
    /// the next thing typed. Mutation: read it once at Attach and the second
    /// half fails.
    /// </summary>
    [Fact]
    public void TheEnabledSwitchIsReadOnEveryLine()
    {
        using var rig = new Rig("^/ubpd$ = /ub propertydump");
        rig.Enabled = false;
        Assert.Equal(PluginChatInputAction.Pass, rig.Chat.Type("/ubpd").Action);
        rig.Enabled = true;
        Assert.Equal(PluginChatInputAction.Rewrite, rig.Chat.Type("/ubpd").Action);
    }

    /// <summary>
    /// The named groups go into the expression runtime's session scope
    /// under capturegroup_<name>, the same variables a meta chat capture
    /// fills, so an expression alias reads them the same way.
    /// </summary>
    [Fact]
    public void CaptureGroupsAreSessionVariablesTheExpressionReads()
    {
        using var rig = new Rig(
            @"^/tloc (?<name>.*)$ => \/tell +getvar[capturegroup_name]+\, hello");

        PluginChatInputDecision decision = rig.Chat.Type("/tloc Bob");

        Assert.Equal(PluginChatInputAction.Rewrite, decision.Action);
        Assert.Equal("/tell Bob, hello", decision.Text);
        Assert.Equal(
            "Bob",
            rig.Expressions.State.Get(ExpressionVariableScope.Session, "capturegroup_name")
                .ToDisplayString());
        Assert.Equal(
            "/tloc Bob",
            rig.Expressions.State.Get(ExpressionVariableScope.Session, "capturegroup_0")
                .ToDisplayString());
    }

    /// <summary>
    /// A marker in the middle of a sentence is replaced before the
    /// sentence goes out, and the rest of the sentence survives around it.
    /// </summary>
    [Fact]
    public void AMarkerInsideATypedLineIsRewrittenInPlace()
    {
        using var rig = new Rig(
            @"^(?<start>.*)(?<loc>\$LOC)(?<end>.*)$ => $capturegroup_start + `here` + $capturegroup_end");

        PluginChatInputDecision decision = rig.Chat.Type("meet me at $LOC please");

        Assert.Equal(PluginChatInputAction.Rewrite, decision.Action);
        Assert.Equal("meet me at here please", decision.Text);
    }

    /// <summary>
    /// An eating alias swallows the line and sends its output through the
    /// chat bar on its own. That output goes through the same interceptor
    /// on its way out, and must not be held against the aliases again: an
    /// alias whose output matches itself would otherwise call itself until
    /// the stack ran out.
    /// </summary>
    [Fact]
    public void AnEatingAliasSuppressesAndItsOutputIsNotAliasedAgain()
    {
        using var rig = new Rig("[eat] ^/echo.*$ = /echo again");

        PluginChatInputDecision decision = rig.Chat.Type("/echo");

        Assert.Equal(PluginChatInputAction.Suppress, decision.Action);
        Assert.Equal(["/echo again"], rig.Chat.Sent);
    }

    /// <summary>
    /// A plain expression runs and the line passes; nothing is sent for it.
    /// </summary>
    [Fact]
    public void APlainExpressionAliasRunsAndTheLinePasses()
    {
        using var rig = new Rig("^/setx$ -> setvar[x, 7]");
        Assert.Equal(PluginChatInputAction.Pass, rig.Chat.Type("/setx").Action);
        Assert.Equal(
            7d,
            rig.Expressions.State.Get(ExpressionVariableScope.Session, "x").AsNumber("x"));
    }

    /// <summary>
    /// A rewrite comes back through the interceptor; an alias whose rewrite
    /// matches itself would go round until the host cut it off. The
    /// controller stops it on the second lap: the line that was just
    /// produced is let through as it stands, and the player is told once.
    /// Mutation: drop the guard and the second call rewrites again.
    /// </summary>
    [Fact]
    public void ARewriteThatComesBackRoundIsLetThroughOnTheSecondLap()
    {
        using var rig = new Rig("^/x$ = /y", "^/y$ = /x");

        PluginChatInputDecision first = rig.Chat.Type("/x");
        Assert.Equal(PluginChatInputAction.Rewrite, first.Action);
        Assert.Equal("/y", first.Text);

        PluginChatInputDecision second = rig.Chat.Type("/y");
        Assert.Equal(PluginChatInputAction.Pass, second.Action);
        Assert.Single(rig.Host.Logger.Lines, line => line.Contains("^/x$", StringComparison.Ordinal));

        // The next thing typed is a new line, not the tail of that chain.
        Assert.Equal(PluginChatInputAction.Rewrite, rig.Chat.Type("/x").Action);
    }

    /// <summary>An alias that rewrites a line into itself changes nothing; the line goes as typed.</summary>
    [Fact]
    public void ARewriteIntoTheSameLinePassesAtOnce()
    {
        using var rig = new Rig("^/loop$ = /loop");
        Assert.Equal(PluginChatInputAction.Pass, rig.Chat.Type("/loop").Action);
        Assert.Single(rig.Host.Logger.Lines, line => line.Contains("^/loop$", StringComparison.Ordinal));
    }

    /// <summary>
    /// A chain of rewrites is fine as long as it goes somewhere new: one
    /// alias may rewrite into another.
    /// </summary>
    [Fact]
    public void ARewriteMayBecomeAnotherAlias()
    {
        using var rig = new Rig("^/a$ = /b", "^/b$ = /c");
        Assert.Equal("/b", rig.Chat.Type("/a").Text);
        Assert.Equal("/c", rig.Chat.Type("/b").Text);
        Assert.Equal(PluginChatInputAction.Pass, rig.Chat.Type("/c").Action);
    }

    /// <summary>
    /// When more than one alias has something to send, the first replaces
    /// the line and the rest go out through the chat bar after it.
    /// </summary>
    [Fact]
    public void ExtraOutputsGoOutThroughTheChatBar()
    {
        using var rig = new Rig("^/both$ = /first", "^/both$ = /second");
        PluginChatInputDecision decision = rig.Chat.Type("/both");
        Assert.Equal("/first", decision.Text);
        Assert.Equal(["/second"], rig.Chat.Sent);
    }

    /// <summary>
    /// The list is read live: a line added to the setting counts on the next
    /// thing typed, and a line that does not parse is reported once when the
    /// list changes rather than on every keystroke.
    /// </summary>
    [Fact]
    public void TheListIsReadLiveAndBadLinesAreReportedOncePerChange()
    {
        using var rig = new Rig();
        Assert.Equal(PluginChatInputAction.Pass, rig.Chat.Type("/ubpd").Action);

        rig.Lines = ["^/ubpd$ = /ub propertydump", "^(broken = /x"];
        Assert.Equal(PluginChatInputAction.Rewrite, rig.Chat.Type("/ubpd").Action);
        rig.Chat.Type("/ubpd");
        rig.Chat.Type("/ubpd");
        Assert.Single(rig.Host.Logger.Lines, line => line.Contains("Line 2", StringComparison.Ordinal));

        rig.Lines = ["^/ubpd$ = /ub propertydump"];
        rig.Chat.Type("/ubpd");
        Assert.Single(rig.Host.Logger.Lines, line => line.Contains("Line 2", StringComparison.Ordinal));
    }

    /// <summary>An expression that fails is reported and the line is not lost.</summary>
    [Fact]
    public void AFailingExpressionIsReportedAndTheLinePasses()
    {
        using var rig = new Rig("^/bad$ => nosuchfunction[]");
        Assert.Equal(PluginChatInputAction.Pass, rig.Chat.Type("/bad").Action);
        Assert.Contains(
            rig.Host.Logger.Lines,
            line => line.StartsWith("error", StringComparison.Ordinal)
                && line.Contains("nosuchfunction", StringComparison.Ordinal));
    }
}

file sealed class NoSelection : ISelectionService
{
    public uint? SelectedObjectId => null;
    public uint? PreviousObjectId => null;

    public event Action<SelectionChangedEvent> Changed
    {
        add { }
        remove { }
    }

    public bool Select(uint objectId) => false;

    public bool Clear() => false;
}

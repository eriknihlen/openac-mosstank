using System.Globalization;
using AcDream.Plugin.Abstractions;
using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class MetaEngineTests
{
    [Fact]
    public void RuleFiresOncePerStateEntryAndCanFireAfterReentry()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var profile = new MetaProfile
        {
            Rules =
            [
                Rule(MetaConditionKind.Always, MetaActionKind.ExpressionAction,
                    "setvar[`count`,getvar[`count`]+1]"),
            ],
        };
        var engine = new MetaEngine(host, expressions, profile);
        engine.SetEnabled(true);

        engine.EvaluatePass();
        engine.EvaluatePass();
        Assert.Equal(1d, expressions.Evaluate("getvar[`count`]").AsNumber());
        Assert.Equal(1, engine.FiredRuleCount);

        engine.Transition(MetaEngine.DefaultState);
        engine.EvaluatePass();
        Assert.Equal(2d, expressions.Evaluate("getvar[`count`]").AsNumber());
    }

    [Fact]
    public void StateTransitionStopsTheOldStatesOrderedPass()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var profile = new MetaProfile
        {
            Rules =
            [
                Rule(MetaConditionKind.Always, MetaActionKind.SetMetaState, "Next"),
                Rule(MetaConditionKind.Always, MetaActionKind.ExpressionAction,
                    "setvar[`wrong`,1]"),
                Rule(MetaConditionKind.Always, MetaActionKind.ExpressionAction,
                    "setvar[`right`,1]", "Next"),
            ],
        };
        var engine = new MetaEngine(host, expressions, profile);
        engine.SetEnabled(true);

        engine.EvaluatePass();
        Assert.Equal("Next", engine.CurrentState);
        Assert.Equal(0d, expressions.Evaluate("getvar[`wrong`]").AsNumber());
        engine.EvaluatePass();
        Assert.Equal(1d, expressions.Evaluate("getvar[`right`]").AsNumber());
    }

    [Fact]
    public void CallAndReturnUseTheVtankReturnStateStack()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var call = Rule(MetaConditionKind.Always, MetaActionKind.CallMetaState, "Worker");
        call.Action.SecondaryText = "ReturnHere";
        var profile = new MetaProfile
        {
            Rules =
            [
                call,
                Rule(MetaConditionKind.Always, MetaActionKind.ReturnFromCall, state: "Worker"),
                Rule(MetaConditionKind.Always, MetaActionKind.ExpressionAction,
                    "setvar[`returned`,1]", "ReturnHere"),
            ],
        };
        var engine = new MetaEngine(host, expressions, profile);
        engine.SetEnabled(true);

        engine.EvaluatePass();
        Assert.Equal("Worker", engine.CurrentState);
        Assert.Equal(1, engine.CallDepth);
        engine.EvaluatePass();
        Assert.Equal("ReturnHere", engine.CurrentState);
        Assert.Equal(0, engine.CallDepth);
        engine.EvaluatePass();
        Assert.Equal(1d, expressions.Evaluate("getvar[`returned`]").AsNumber());
    }

    [Fact]
    public void ChatCapturePublishesGroupsAndColorToExpressionVariables()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var engine = ChatRuleEngine(
            host,
            expressions,
            MetaConditionKind.ChatMessageCapture,
            "^(?<name>.+) tells you, \\\"(?<words>.+)\\\"$",
            "3;4");
        // The host hands a tell over worded as the chat window prints it, with
        // the chat text type. A creature's object id is outside the player
        // range, so its name is not a link.
        host.Automation.Messages.Add(Worded(
            0x8000_0020u, TellKind, "Horan", "ready", string.Empty, TellLogTextType,
            "Horan tells you, \"ready\""));

        engine.OnTick(MetaEngine.DecisionIntervalSeconds);

        Assert.Equal("Horan", expressions.Evaluate(
            "getvar[`capturegroup_name`]").AsString());
        Assert.Equal("ready", expressions.Evaluate(
            "getvar[`capturegroup_words`]").AsString());
        Assert.Equal(3d, expressions.Evaluate("getvar[`capturecolor`]").AsNumber());
        Assert.Equal(1d, expressions.Evaluate("getvar[`matched`]").AsNumber());
    }

    // The host's chat kinds, channel numbers and the chat text types the
    // lines below carry.
    private const int ChannelKind = 2;
    private const int TellKind = 3;
    private const int LocalSpeechKind = 0;
    private const int SpeechLogTextType = 0x02;
    private const int TellLogTextType = 0x03;
    private const int SocialLogTextType = 0x0A;
    private const int FellowshipLogTextType = 0x13;
    private const uint FellowshipChannel = 0x800u;
    private const uint PatronChannel = 0x1000u;
    private const uint CoVassalsChannel = 0x100_0000u;

    /// <summary>
    /// The command capture a RynCMD meta puts on every chat command: your own
    /// line opens with "You" (after an optional "[Channel] "), anyone else's
    /// carries the tagged name.
    /// </summary>
    private const string RynCmdCommandCapture =
        @"(^(\[[A-z]+?\] |)You|.*\<Tell:IIDString:.+:(?<name>[^\<]*)\>.+\<\\Tell\>) (?<saythink>.*), \""!atk\""$";
    private const string RynCmdCommandColors = "2;3;4;8;9;10;11;18;19";

    public static TheoryData<string, uint, int, string, uint, int, string, string, string> RynCmdCommandLines =>
        new()
        {
            // Another player on the fellowship channel: the numbered channel
            // carries no object id, so the link names id 0.
            { "Horan", 0u, ChannelKind, string.Empty, FellowshipChannel, FellowshipLogTextType,
                "[Fellowship] <Tell:IIDString:0:Horan>Horan<\\Tell> says, \"!atk\"",
                "Horan", "says" },
            // Our own fellowship line as a server sends it back to the sender.
            { string.Empty, 0u, ChannelKind, string.Empty, FellowshipChannel, FellowshipLogTextType,
                "[Fellowship] You say, \"!atk\"", "[Fellowship] You", "say" },
            // A player's tell to us: the link carries the sender's id in decimal.
            { "Horan", 0x5000_0002u, TellKind, string.Empty, 0u, TellLogTextType,
                "<Tell:IIDString:1342177282:Horan>Horan<\\Tell> tells you, \"!atk\"",
                "Horan", "tells you" },
            // A tell to ourselves.
            { "Meta Tester", Host.OwnObjectId, TellKind, string.Empty, 0u, TellLogTextType,
                "You think, \"!atk\"", "You", "think" },
            // Our own local speech; the host names its own speaker "You".
            { "You", Host.OwnObjectId, LocalSpeechKind, string.Empty, 0u, SpeechLogTextType,
                "You say, \"!atk\"", "You", "say" },
            // A named room (allegiance, general, ...) links the speaker with id 0.
            { "Horan", 0u, ChannelKind, "Allegiance", 7u, 0x12,
                "[Allegiance] <Tell:IIDString:0:Horan>Horan<\\Tell> says, \"!atk\"",
                "Horan", "says" },
        };

    [Theory]
    [MemberData(nameof(RynCmdCommandLines))]
    public void ChatCaptureSeesTheLineTheReferenceChatWindowShows(
        string sender,
        uint senderObjectId,
        int kind,
        string channelName,
        uint channelId,
        int logTextType,
        string expectedLine,
        string expectedWho,
        string expectedSayThink)
    {
        // The host hands the line over worded, beside its parts.
        AssertRynCmdCapture(
            Worded(senderObjectId, kind, sender, "!atk", channelName, logTextType,
                expectedLine, channelId),
            logTextType,
            expectedWho,
            expectedSayThink);
    }

    [Theory]
    [MemberData(nameof(RynCmdCommandLines))]
    public void AnOlderHostsPartsAreRebuiltIntoTheSameLine(
        string sender,
        uint senderObjectId,
        int kind,
        string channelName,
        uint channelId,
        int logTextType,
        string expectedLine,
        string expectedWho,
        string expectedSayThink)
    {
        _ = channelId;
        // An older host words nothing and carries no channel number: only the
        // parts arrive, and the line is put back together from them.
        var parts = new PluginChatMessage(1, senderObjectId, kind, sender, "!atk", channelName)
        {
            LogTextType = logTextType,
        };
        Assert.Equal(expectedLine, DecalChatLine.Compose(parts, Host.OwnObjectId, "Meta Tester"));
        AssertRynCmdCapture(parts, logTextType, expectedWho, expectedSayThink);
    }

    public static TheoryData<uint, string, string> NumberedChannelLines =>
        new()
        {
            { PatronChannel,
                "Your patron <Tell:IIDString:0:Horan>Horan<\\Tell> says to you, \"!atk\"",
                "says to you" },
            { CoVassalsChannel,
                "[Co-Vassals] <Tell:IIDString:0:Horan>Horan<\\Tell> says, \"!atk\"",
                "says" },
        };

    [Theory]
    [MemberData(nameof(NumberedChannelLines))]
    public void ChatCaptureReadsTheHostsWordingOfANumberedChannel(
        uint channelId,
        string displayText,
        string expectedSayThink)
    {
        // The patron and co-vassal channels have no name, only a number, and
        // their sentences cannot be rebuilt from the parts; the host's own
        // wording is the only way the command capture sees the speaker.
        AssertRynCmdCapture(
            Worded(0u, ChannelKind, "Horan", "!atk", string.Empty, SocialLogTextType,
                displayText, channelId),
            SocialLogTextType,
            "Horan",
            expectedSayThink);
    }

    [Fact]
    public void ChatMatchReadsTheHostsWordingOverItsParts()
    {
        // When the host words a line, that wording is the line: the parts
        // are not rebuilt over it even where they would read differently.
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var engine = ChatRuleEngine(
            host,
            expressions,
            MetaConditionKind.ChatMessage,
            "^\\[Fellowship\\] <Tell:IIDString:0:Horan>Horan<\\\\Tell> says, \"go\"$",
            string.Empty);
        host.Automation.Messages.Add(Worded(
            0u, ChannelKind, "Horan", "go", "Somewhere Else", FellowshipLogTextType,
            "[Fellowship] <Tell:IIDString:0:Horan>Horan<\\Tell> says, \"go\"",
            FellowshipChannel));

        engine.OnTick(MetaEngine.DecisionIntervalSeconds);

        Assert.Equal(1d, expressions.Evaluate("getvar[`matched`]").AsNumber());
    }

    private static void AssertRynCmdCapture(
        PluginChatMessage message,
        int logTextType,
        string expectedWho,
        string expectedSayThink)
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var engine = ChatRuleEngine(
            host,
            expressions,
            MetaConditionKind.ChatMessageCapture,
            RynCmdCommandCapture,
            RynCmdCommandColors);
        host.Automation.Messages.Add(message);

        engine.OnTick(MetaEngine.DecisionIntervalSeconds);

        Assert.Equal(1d, expressions.Evaluate("getvar[`matched`]").AsNumber());
        string who = expressions.Evaluate("testvar[`capturegroup_name`]").IsTruthy
            ? expressions.Evaluate("getvar[`capturegroup_name`]").AsString()
            : expressions.Evaluate("getvar[`capturegroup_1`]").AsString();
        Assert.Equal(expectedWho, who);
        Assert.Equal(expectedSayThink, expressions.Evaluate(
            "getvar[`capturegroup_saythink`]").AsString());
        Assert.Equal((double)logTextType, expressions.Evaluate(
            "getvar[`capturecolor`]").AsNumber());
    }

    /// <summary>
    /// A line as the host hands it over: its parts, its chat text type, its
    /// channel number and the host's own wording of the whole line.
    /// </summary>
    private static PluginChatMessage Worded(
        uint senderObjectId,
        int kind,
        string sender,
        string text,
        string channelName,
        int logTextType,
        string displayText,
        uint channelId = 0u) =>
        new(1, senderObjectId, kind, sender, text, channelName)
        {
            LogTextType = logTextType,
            ChannelId = channelId,
            DisplayText = displayText,
        };

    [Fact]
    public void ChatMatchSeesAnNpcTellUnlinkedAndATellToYourselfAsAThought()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var npc = ChatRuleEngine(
            host,
            expressions,
            MetaConditionKind.ChatMessage,
            "^Master Arbitrator tells you\\, \"Welcome to Colosseum!\"$",
            string.Empty);
        host.Automation.Messages.Add(Worded(
            0x8000_1234u, TellKind, "Master Arbitrator", "Welcome to Colosseum!", string.Empty,
            TellLogTextType, "Master Arbitrator tells you, \"Welcome to Colosseum!\""));
        npc.OnTick(MetaEngine.DecisionIntervalSeconds);
        Assert.Equal(1d, expressions.Evaluate("getvar[`matched`]").AsNumber());

        var thoughtHost = new Host();
        using var thoughtExpressions = new MossTankExpressionRuntime(thoughtHost);
        var thought = ChatRuleEngine(
            thoughtHost,
            thoughtExpressions,
            MetaConditionKind.ChatMessage,
            "^You think, \"Jumper Success\"",
            string.Empty);
        thoughtHost.Automation.Messages.Add(Worded(
            Host.OwnObjectId, TellKind, "Meta Tester", "Jumper Success", string.Empty,
            TellLogTextType, "You think, \"Jumper Success\""));
        thought.OnTick(MetaEngine.DecisionIntervalSeconds);
        Assert.Equal(1d, thoughtExpressions.Evaluate("getvar[`matched`]").AsNumber());
    }

    [Fact]
    public void ChatColorListFiltersOnTheChatTextTypeNotTheHostKind()
    {
        // A fellowship line is kind 2 (channel) on the host and text type 19
        // (fellowship) in the game client. A list naming 19 takes it; a
        // list naming only 2 (local speech) does not.
        const string line = "[Fellowship] <Tell:IIDString:0:Horan>Horan<\\Tell> says, \"!atk\"";
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var fellowshipOnly = ChatRuleEngine(
            host, expressions, MetaConditionKind.ChatMessageCapture, "!atk", "19");
        host.Automation.Messages.Add(Worded(
            0u, ChannelKind, "Horan", "!atk", string.Empty, FellowshipLogTextType, line,
            FellowshipChannel));
        fellowshipOnly.OnTick(MetaEngine.DecisionIntervalSeconds);
        Assert.Equal(1d, expressions.Evaluate("getvar[`matched`]").AsNumber());

        var speechHost = new Host();
        using var speechExpressions = new MossTankExpressionRuntime(speechHost);
        var speechOnly = ChatRuleEngine(
            speechHost, speechExpressions, MetaConditionKind.ChatMessageCapture, "!atk", "2");
        speechHost.Automation.Messages.Add(Worded(
            0u, ChannelKind, "Horan", "!atk", string.Empty, FellowshipLogTextType, line,
            FellowshipChannel));
        speechOnly.OnTick(MetaEngine.DecisionIntervalSeconds);
        Assert.Equal(0d, speechExpressions.Evaluate("getvar[`matched`]").AsNumber());
    }

    /// <summary>
    /// A chat condition's colour list is read the same on every machine: a
    /// culture whose plus sign is something else still reads "+3" as 3.
    /// Mutation: reading the list in the machine's culture refuses it, and
    /// the tell does not match.
    /// </summary>
    [Fact]
    public void AColourListReadsTheSameInEveryCulture()
    {
        CultureInfo previous = CultureInfo.CurrentCulture;
        var odd = (CultureInfo)CultureInfo.InvariantCulture.Clone();
        odd.NumberFormat.PositiveSign = "p";
        CultureInfo.CurrentCulture = odd;
        try
        {
            var host = new Host();
            using var expressions = new MossTankExpressionRuntime(host);
            var engine = ChatRuleEngine(
                host, expressions, MetaConditionKind.ChatMessage, "ready", "+3");
            host.Automation.Messages.Add(Worded(
                0x8000_0020u, TellKind, "Horan", "ready", string.Empty, TellLogTextType,
                "Horan tells you, \"ready\""));

            engine.OnTick(MetaEngine.DecisionIntervalSeconds);

            Assert.Equal(1d, expressions.Evaluate("getvar[`matched`]").AsNumber());
        }
        finally
        {
            CultureInfo.CurrentCulture = previous;
        }
    }

    [Fact]
    public void AFailingExpressionConditionIsFalseWarnsOnceAndThePassGoesOn()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var broken = Rule(MetaConditionKind.Expression, MetaActionKind.ExpressionAction,
            "setvar[`broken`,1]");
        broken.Condition.Text = "getvar[a]+";
        var after = Rule(MetaConditionKind.Always, MetaActionKind.ExpressionAction,
            "setvar[`after`,getvar[`after`]+1]");
        var engine = new MetaEngine(host, expressions, new MetaProfile { Rules = [broken, after] });
        engine.SetEnabled(true);

        engine.EvaluatePass();
        engine.Transition(MetaEngine.DefaultState);
        engine.EvaluatePass();

        Assert.Equal(0d, expressions.Evaluate("getvar[`broken`]").AsNumber());
        Assert.Equal(2d, expressions.Evaluate("getvar[`after`]").AsNumber());
        string warning = Assert.Single(host.Automation.Posted);
        Assert.StartsWith("Error in meta expression: getvar[a]+ (", warning);
    }

    /// <summary>
    /// An expression condition whose run ends in an error the runtime itself
    /// raised (a NaN under a bitwise operator, a pattern that does not parse)
    /// is false and the pass goes on to the next rule, as any failed
    /// condition is in UtilityBelt. Mutation: letting the raw exception out
    /// of the expression throws it out of the tick and the later rule never
    /// runs.
    /// </summary>
    [Theory]
    [InlineData("(0/0)&1")]
    [InlineData("`abc`#`(`")]
    public void ARuntimeErrorInAnExpressionConditionIsFalseAndThePassGoesOn(string source)
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var broken = Rule(MetaConditionKind.Expression, MetaActionKind.ExpressionAction,
            "setvar[`broken`,1]");
        broken.Condition.Text = source;
        var after = Rule(MetaConditionKind.Always, MetaActionKind.ExpressionAction,
            "setvar[`after`,1]");
        var engine = new MetaEngine(host, expressions, new MetaProfile { Rules = [broken, after] });
        engine.SetEnabled(true);

        engine.OnTick(MetaEngine.DecisionIntervalSeconds);

        Assert.Equal(0d, expressions.Evaluate("getvar[`broken`]").AsNumber());
        Assert.Equal(1d, expressions.Evaluate("getvar[`after`]").AsNumber());
        string warning = Assert.Single(host.Automation.Posted);
        Assert.StartsWith($"Error in meta expression: {source} (", warning);
    }

    /// <summary>
    /// A chat condition whose pattern takes too long on a line is false,
    /// reported once, and the pass goes on to the next rule, the way a
    /// condition that fails is false in UtilityBelt. Mutation: letting the
    /// pattern's timeout out throws it out of the tick and the later rule
    /// never runs.
    /// </summary>
    [Fact]
    public void AChatPatternThatTimesOutIsFalseAndThePassGoesOn()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var slow = Rule(MetaConditionKind.ChatMessage, MetaActionKind.ExpressionAction,
            "setvar[`slow`,1]");
        slow.Condition.Text = "^(x+x+)+y$";
        var after = Rule(MetaConditionKind.Always, MetaActionKind.ExpressionAction,
            "setvar[`after`,1]");
        var engine = new MetaEngine(host, expressions, new MetaProfile { Rules = [slow, after] });
        engine.SetEnabled(true);
        string line = new('x', 40);
        host.Automation.Messages.Add(Worded(
            0x8000_0020u, TellKind, "Horan", line, string.Empty, TellLogTextType, line));

        engine.OnTick(MetaEngine.DecisionIntervalSeconds);

        Assert.Equal(0d, expressions.Evaluate("getvar[`slow`]").AsNumber());
        Assert.Equal(1d, expressions.Evaluate("getvar[`after`]").AsNumber());
        string warning = Assert.Single(host.Automation.Posted);
        Assert.StartsWith("Error in meta condition ChatMessage: ", warning);
    }

    [Fact]
    public void AFailingExpressionActionWarnsOnceAndStillReturnsTrue()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var rule = Rule(MetaConditionKind.Always, MetaActionKind.All);
        rule.Action.Children =
        [
            new MetaAction { Kind = MetaActionKind.ExpressionAction, Text = "nosuchfunction[]" },
            new MetaAction { Kind = MetaActionKind.ChatExpression, Text = "nosuchfunction[]" },
            new MetaAction { Kind = MetaActionKind.SetMetaState, Text = "Next" },
        ];
        var engine = new MetaEngine(host, expressions, new MetaProfile { Rules = [rule] });
        engine.SetEnabled(true);

        engine.EvaluatePass();

        // Both failures are reported, the chat action says nothing, and the
        // state change after them still happens.
        Assert.Equal("Next", engine.CurrentState);
        // (The runtime's own quest refresh is the only line sent.)
        Assert.All(host.Automation.Submitted, text => Assert.Equal("/myquests", text));
        Assert.Collection(
            host.Automation.Posted,
            warning => Assert.StartsWith(
                "Error in meta expression action: nosuchfunction[] (", warning),
            warning => Assert.StartsWith(
                "Error in meta expression chat action: nosuchfunction[] (", warning));

        engine.Transition(MetaEngine.DefaultState);
        engine.EvaluatePass();
        Assert.Equal(2, host.Automation.Posted.Count);
    }

    /// <summary>
    /// A profile loaded by one of its own actions ends the pass there, as the
    /// reference's pass loop does: none of the old profile's later rules run,
    /// so a later SetState cannot land in the new profile, and the new
    /// profile starts in Default on the next pass. Mutation: carrying on
    /// through the old profile's rules leaves the new one in state X, where
    /// it has no rules.
    /// </summary>
    [Fact]
    public void AProfileLoadedByAnActionEndsThePass()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var cleanup = new MetaProfile
        {
            Rules =
            [
                Rule(MetaConditionKind.Always, MetaActionKind.ExpressionAction,
                    "setvar[`cleanupran`,1]"),
            ],
        };
        var first = new MetaProfile
        {
            Rules =
            [
                Rule(MetaConditionKind.Always, MetaActionKind.ChatCommand,
                    "/vt meta load Cleanup"),
                Rule(MetaConditionKind.Always, MetaActionKind.SetMetaState, "X"),
                Rule(MetaConditionKind.Always, MetaActionKind.ExpressionAction,
                    "setvar[`wrong`,1]", "X"),
            ],
        };
        var engine = new MetaEngine(host, expressions, first);
        host.Automation.OnSubmit = text =>
        {
            if (text == "/vt meta load Cleanup")
                engine.ReplaceProfile(cleanup);
        };
        engine.SetEnabled(true);

        engine.EvaluatePass();
        Assert.Equal(MetaEngine.DefaultState, engine.CurrentState);

        engine.EvaluatePass();
        Assert.Equal(1d, expressions.Evaluate("getvar[`cleanupran`]").AsNumber());
        Assert.Equal(0d, expressions.Evaluate("getvar[`wrong`]").AsNumber());
    }

    /// <summary>
    /// A DoAll runs every one of its actions and counts as done, as in the
    /// reference: a SetState in the middle does not stop the ones after it,
    /// and the pass then ends because the state changed. A load in the middle
    /// likewise lets the SetState after it apply, to the new profile.
    /// Mutation: stopping the DoAll at its SetState skips the DoExpr.
    /// </summary>
    [Fact]
    public void ADoAllRunsEveryActionAndThePassEndsOnTheStateChange()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var rule = Rule(MetaConditionKind.Always, MetaActionKind.All);
        rule.Action.Children =
        [
            new MetaAction { Kind = MetaActionKind.SetMetaState, Text = "Next" },
            new MetaAction { Kind = MetaActionKind.ExpressionAction, Text = "setvar[`after`,1]" },
        ];
        var engine = new MetaEngine(host, expressions, new MetaProfile
        {
            Rules =
            [
                rule,
                Rule(MetaConditionKind.Always, MetaActionKind.ExpressionAction,
                    "setvar[`wrong`,1]"),
            ],
        });
        engine.SetEnabled(true);

        engine.EvaluatePass();

        Assert.Equal("Next", engine.CurrentState);
        Assert.Equal(1d, expressions.Evaluate("getvar[`after`]").AsNumber());
        Assert.Equal(0d, expressions.Evaluate("getvar[`wrong`]").AsNumber());

        var loadingHost = new Host();
        using var loadingExpressions = new MossTankExpressionRuntime(loadingHost);
        var loading = Rule(MetaConditionKind.Always, MetaActionKind.All);
        loading.Action.Children =
        [
            new MetaAction { Kind = MetaActionKind.ChatCommand, Text = "/vt meta load Other" },
            new MetaAction { Kind = MetaActionKind.SetMetaState, Text = "Picked" },
        ];
        var loadingEngine = new MetaEngine(loadingHost, loadingExpressions, new MetaProfile { Rules = [loading] });
        loadingHost.Automation.OnSubmit = _ => loadingEngine.ReplaceProfile(new MetaProfile
        {
            Rules =
            [
                Rule(MetaConditionKind.Always, MetaActionKind.ExpressionAction,
                    "setvar[`picked`,1]", "Picked"),
            ],
        });
        loadingEngine.SetEnabled(true);

        loadingEngine.EvaluatePass();
        Assert.Equal("Picked", loadingEngine.CurrentState);
        loadingEngine.EvaluatePass();
        Assert.Equal(1d, loadingExpressions.Evaluate("getvar[`picked`]").AsNumber());
    }

    /// <summary>
    /// A condition that changes the state (vtsetmetastate inside an Expr)
    /// leaves the rest of the old state's rules unfired: the reference only
    /// fires a rule whose state is the current one. Mutation: carrying on
    /// fires the Default rule after the state has left Default.
    /// </summary>
    [Fact]
    public void AConditionThatChangesTheStateStopsTheOldStatesRules()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        MetaEngine? engine = null;
        expressions.Registry.Register("vtsetmetastate", 1, 1, (_, args) =>
        {
            engine!.Transition(args[0].AsString("vtsetmetastate"));
            return ExpressionValue.One;
        }, "vtsetmetastate[state]");
        var moves = Rule(MetaConditionKind.Expression, MetaActionKind.ExpressionAction,
            "setvar[`moved`,1]");
        moves.Condition.Text = "vtsetmetastate[`Other`]==0";
        engine = new MetaEngine(host, expressions, new MetaProfile
        {
            Rules =
            [
                moves,
                Rule(MetaConditionKind.Always, MetaActionKind.ExpressionAction,
                    "setvar[`wrong`,1]"),
            ],
        });
        engine.SetEnabled(true);

        engine.EvaluatePass();

        Assert.Equal("Other", engine.CurrentState);
        Assert.Equal(0d, expressions.Evaluate("getvar[`moved`]").AsNumber());
        Assert.Equal(0d, expressions.Evaluate("getvar[`wrong`]").AsNumber());
    }

    /// <summary>
    /// SetOpt, like the reference's, always counts as done: an expression
    /// that fails, a value of the wrong type or an option that does not
    /// exist is reported once and the rest of the DoAll still runs.
    /// Mutation: letting the failure escape skips the SetState.
    /// </summary>
    [Theory]
    [InlineData("nosuchfunction[]", "Error in Set VT Option meta expression: nosuchfunction[] (")]
    [InlineData("`text`", "SetVTOption Action: Attempted to set AttackDistance with the wrong type of value (")]
    [InlineData("1", "SetVTOption Action: Specified setting doesn't exist. Will not execute.")]
    public void AFailingSetOptWarnsOnceAndTheDoAllGoesOn(string valueSource, string warning)
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var rule = Rule(MetaConditionKind.Always, MetaActionKind.All);
        string option = valueSource == "1" ? "NoSuchOption" : "AttackDistance";
        rule.Action.Children =
        [
            new MetaAction
            {
                Kind = MetaActionKind.SetVtankOption,
                Text = option,
                SecondaryText = valueSource,
            },
            new MetaAction { Kind = MetaActionKind.SetMetaState, Text = "Next" },
        ];
        var engine = new MetaEngine(host, expressions, new MetaProfile { Rules = [rule] }, new MetaServices
        {
            SetOption = (name, value) => name switch
            {
                "AttackDistance" => value.AsNumber("AttackDistance") >= 0d,
                _ => false,
            },
        });
        engine.SetEnabled(true);

        engine.EvaluatePass();
        engine.Transition(MetaEngine.DefaultState);
        engine.EvaluatePass();

        Assert.Equal("Next", engine.CurrentState);
        string posted = Assert.Single(host.Automation.Posted);
        Assert.StartsWith(warning, posted);
    }

    /// <summary>
    /// The reference asks whether the option exists before it looks at the
    /// value: a name its settings table does not hold is refused with its
    /// own warning, the value is never evaluated, and nothing is written.
    /// Mutation: evaluating first reports the value's error instead, and
    /// handing the name to the option owner writes a setting that does not
    /// exist.
    /// </summary>
    [Fact]
    public void ASetOptOfANameTheReferenceDoesNotHaveIsRefusedBeforeItsValueIsRead()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var written = new List<string>();
        var engine = new MetaEngine(
            host,
            expressions,
            new MetaProfile
            {
                Rules =
                [
                    new MetaRule
                    {
                        Condition = new MetaCondition { Kind = MetaConditionKind.Always },
                        Action = new MetaAction
                        {
                            Kind = MetaActionKind.SetVtankOption,
                            Text = "MonsterRange",
                            SecondaryText = "nosuchfunction[]",
                        },
                    },
                ],
            },
            new MetaServices
            {
                SetOption = (name, _) =>
                {
                    written.Add(name);
                    return true;
                },
            });
        engine.SetEnabled(true);

        engine.EvaluatePass();
        engine.EvaluatePass();

        Assert.Empty(written);
        Assert.Equal(
            "SetVTOption Action: Specified setting doesn't exist. Will not execute.",
            Assert.Single(host.Automation.Posted));
    }

    /// <summary>
    /// GetOpt, as the reference runs it: an empty option or variable, or an
    /// option its settings table does not hold, is reported once and no
    /// variable is written; a real option is written to the variable the
    /// action names. Mutation: the old fallback variable name, or reading an
    /// unknown name as zero, writes a variable.
    /// </summary>
    [Theory]
    [InlineData("MonsterRange", "into", "GetVTOption Action: Specified setting doesn't exist. Will not execute.")]
    [InlineData("AttackDistance", "", "GetVTOption Action: Empty option or variable. Will not execute.")]
    [InlineData("", "into", "GetVTOption Action: Empty option or variable. Will not execute.")]
    public void AGetOptTheReferenceRefusesWritesNoVariable(
        string option,
        string variable,
        string warning)
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var read = new List<string>();
        var engine = new MetaEngine(
            host,
            expressions,
            new MetaProfile
            {
                Rules =
                [
                    new MetaRule
                    {
                        Condition = new MetaCondition { Kind = MetaConditionKind.Always },
                        Action = new MetaAction
                        {
                            Kind = MetaActionKind.GetVtankOption,
                            Text = option,
                            SecondaryText = variable,
                        },
                    },
                ],
            },
            new MetaServices
            {
                GetOption = name =>
                {
                    read.Add(name);
                    return ExpressionValue.Number(7d);
                },
            });
        engine.SetEnabled(true);

        engine.EvaluatePass();
        engine.EvaluatePass();

        Assert.Empty(read);
        Assert.Equal(warning, Assert.Single(host.Automation.Posted));
        Assert.False(expressions.Evaluate("testvar[`into`]").IsTruthy);
        Assert.False(expressions.Evaluate("testvar[`option`]").IsTruthy);
    }

    /// <summary>
    /// A GetOpt of a real option, spelled in any case, writes the variable
    /// it names. Mutation: refusing a name that differs only in case.
    /// </summary>
    [Fact]
    public void AGetOptOfARealOptionWritesTheNamedVariable()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var engine = new MetaEngine(
            host,
            expressions,
            new MetaProfile
            {
                Rules =
                [
                    new MetaRule
                    {
                        Condition = new MetaCondition { Kind = MetaConditionKind.Always },
                        Action = new MetaAction
                        {
                            Kind = MetaActionKind.GetVtankOption,
                            Text = "attackdistance",
                            SecondaryText = "into",
                        },
                    },
                ],
            },
            new MetaServices { GetOption = _ => ExpressionValue.Number(7d) });
        engine.SetEnabled(true);

        engine.EvaluatePass();

        Assert.Empty(host.Automation.Posted);
        Assert.Equal(7d, expressions.Evaluate("getvar[`into`]").AsNumber());
    }

    /// <summary>
    /// The record of repaired texts already reported is bounded: a meta that
    /// builds a new code string every pass cannot grow it for the whole
    /// session. Once it is full it starts over, so the first text is
    /// reported again. Mutation: an unbounded record never reports it again.
    /// </summary>
    [Fact]
    public void TheRecordOfReportedRepairsIsBounded()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);

        expressions.Evaluate("setvar[route,my.route]");
        for (int index = 0; index < MossTankExpressionRuntime.ReportedRepairLimit; index++)
            expressions.Evaluate($"setvar[route{index},my.route]");
        expressions.Evaluate("setvar[route,my.route]");

        Assert.Equal(MossTankExpressionRuntime.ReportedRepairLimit + 2, host.Logs.Warnings.Count);
        Assert.EndsWith("Expression: setvar[route,my.route]", host.Logs.Warnings[^1]);
    }

    /// <summary>
    /// The reader drops, skips and supplies what the reference parser does,
    /// silently in the meta; the log says so once per expression, naming
    /// what went, so an author can see why a line did not do what it says.
    /// Code strings run by a list function are covered too. Mutation: not
    /// reporting, or reporting on every evaluation.
    /// </summary>
    [Fact]
    public void AnExpressionTheReaderRepairedIsLoggedOncePerText()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);

        expressions.Evaluate("setvar[route,my.route]");
        expressions.Evaluate("setvar[route,my.route]");
        expressions.Evaluate("listfilter[listcreate[1,2],`$1!0&&1`]");
        expressions.Evaluate("setvar[plain,1]");

        Assert.Equal("my", expressions.Evaluate("getvar[route]").AsString());
        Assert.Collection(
            host.Logs.Warnings,
            warning =>
            {
                Assert.Contains("dropped '.r' at 15", warning);
                Assert.Contains("skipped 'oute' at 17", warning);
                Assert.EndsWith("Expression: setvar[route,my.route]", warning);
            },
            warning =>
            {
                Assert.Contains("dropped '!0' at 2", warning);
                Assert.EndsWith("Expression: $1!0&&1", warning);
            });
    }

    private static MetaEngine ChatRuleEngine(
        Host host,
        MossTankExpressionRuntime expressions,
        MetaConditionKind kind,
        string pattern,
        string colors)
    {
        var engine = new MetaEngine(host, expressions, new MetaProfile
        {
            Rules =
            [
                new MetaRule
                {
                    Condition = new MetaCondition
                    {
                        Kind = kind,
                        Text = pattern,
                        SecondaryText = colors,
                    },
                    Action = new MetaAction
                    {
                        Kind = MetaActionKind.ExpressionAction,
                        Text = "setvar[`matched`,1]",
                    },
                },
            ],
        });
        engine.SetEnabled(true);
        return engine;
    }

    [Fact]
    public void WatchdogCallsRecoveryStateOnlyWhenMovementStalls()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var setWatchdog = Rule(MetaConditionKind.Always, MetaActionKind.SetWatchdog);
        setWatchdog.Action.Text = "Recover";
        setWatchdog.Action.Number = 5d;
        setWatchdog.Action.SecondaryNumber = 1d;
        var profile = new MetaProfile { Rules = [setWatchdog] };
        var engine = new MetaEngine(host, expressions, profile);
        engine.SetEnabled(true);
        engine.EvaluatePass();

        for (int index = 0; index < 13; index++)
            engine.OnTick(0.1d);

        Assert.Equal("Recover", engine.CurrentState);
        Assert.Equal(1, engine.CallDepth);
    }

    [Fact]
    public void RecursiveCallOverflowDisablesMetaLikeVtank()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var call = Rule(MetaConditionKind.Always, MetaActionKind.CallMetaState, "Loop");
        call.Action.SecondaryText = "Loop";
        call.State = "Loop";
        var engine = new MetaEngine(
            host,
            expressions,
            new MetaProfile { Rules = [call] });
        engine.Transition("Loop");
        engine.SetEnabled(true);

        for (int index = 0; index <= MetaEngine.MaximumCallDepth; index++)
            engine.EvaluatePass();

        Assert.False(engine.Enabled);
        Assert.Contains("too many nested calls", engine.Status, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void ARecursiveCallOverflowAlsoClearsTheStoredEnableMetaSetting()
    {
        var host = new Host();
        using var expressions = new MossTankExpressionRuntime(host);
        var call = Rule(MetaConditionKind.Always, MetaActionKind.CallMetaState, "Loop");
        call.Action.SecondaryText = "Loop";
        call.State = "Loop";
        var written = new List<(string Name, bool Value)>();
        var engine = new MetaEngine(
            host,
            expressions,
            new MetaProfile { Rules = [call] },
            new MetaServices
            {
                SetOption = (name, value) =>
                {
                    written.Add((name, value.IsTruthy));
                    return true;
                },
            });
        engine.Transition("Loop");
        engine.SetEnabled(true);

        for (int index = 0; index <= MetaEngine.MaximumCallDepth; index++)
            engine.EvaluatePass();

        Assert.False(engine.Enabled);
        (string name, bool value) = Assert.Single(written);
        Assert.Equal("EnableMeta", name);
        Assert.False(value);
    }

    private static MetaRule Rule(
        MetaConditionKind condition,
        MetaActionKind action,
        string text = "",
        string state = MetaEngine.DefaultState) => new()
    {
        State = state,
        Condition = new MetaCondition { Kind = condition },
        Action = new MetaAction { Kind = action, Text = text },
    };

    private sealed class Host : IPluginHost
    {
        public const uint OwnObjectId = 1;
        public Host() => Automation = new Automation();
        public bool HasUi => false;
        public Logger Logs { get; } = new Logger();
        public IPluginLogger Log => Logs;
        public IGameState State { get; } = new State();
        public IEvents Events { get; } = new Events();
        public ISelectionService Selection { get; } = new Selection();
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IPluginStorage Storage => NoOpPluginStorage.Instance;
        public Automation Automation { get; }
        IAutomationSurface IPluginHost.Automation => Automation;
    }

    private sealed class Automation :
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
        public uint ObjectId => Host.OwnObjectId;
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
        public bool IsPortal { get; set; }
        public PluginNavigationPosition Position { get; set; } = new(
            0x7F7F0001u, 10d, 20d, 0d, 0f, true);

        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            skill = default;
            return false;
        }
        public IReadOnlyList<PluginChatMessage> CaptureMessages(ulong afterSequence) =>
            Messages.Where(message => message.Sequence > afterSequence).ToArray();
        public List<string> Posted { get; } = [];
        public List<string> Submitted { get; } = [];
        public void PostSystemMessage(string text) => Posted.Add(text);

        /// <summary>
        /// A command the host runs as the line is submitted, the way a chat
        /// command reaches its handler before the submit returns.
        /// </summary>
        public Action<string>? OnSubmit { get; set; }

        public bool Submit(string text)
        {
            Submitted.Add(text);
            OnSubmit?.Invoke(text);
            return true;
        }
        public PluginNavigationSnapshot Snapshot => new(
            true, IsPortal, ObjectId, Position, false, false);
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
        public List<string> Warnings { get; } = [];
        public void Info(string message) { }
        public void Warn(string message) => Warnings.Add(message);
        public void Error(string message, Exception? exception = null) { }
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

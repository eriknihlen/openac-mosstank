using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The /ub verb, and the two UtilityBelt commands metas lean on that the
/// client had no answer for: setmotion and prepclick.
/// </summary>
public sealed partial class MossTankPanelTests
{
    // ── /ub answers to the same commands as /vt ─────────────────────────

    /// <summary>
    /// A meta written for UtilityBelt types /ub; the line has to reach the
    /// same command the /vt form does, and do the same thing.
    /// Mutation: dispatching a /ub line as an unknown command (or dropping
    /// its arguments) leaves the turn, the jump and the use undone.
    /// </summary>
    [Fact]
    public void UbFaceTurnsLikeVtFace()
    {
        (FakeAutomation vt, MossTankPanel vtPanel) = FacingPanel(0f);
        (FakeAutomation ub, MossTankPanel ubPanel) = FacingPanel(0f);

        Command(vtPanel, "face 90");
        UbCommand(ubPanel, "face 90");
        vtPanel.OnTick(0.05d);
        ubPanel.OnTick(0.05d);

        Assert.True(ub.MovementIntents[^1].TurnRight);
        Assert.Equal(vt.MovementIntents, ub.MovementIntents);
        Assert.Equal(vt.Messages, ub.Messages);
    }

    [Fact]
    public void UbJumpswJumpsLikeVtJumpsw()
    {
        (FakeAutomation vt, MossTankPanel vtPanel) = FacingPanel(180f);
        (FakeAutomation ub, MossTankPanel ubPanel) = FacingPanel(180f);

        Command(vtPanel, "jumpsw 180 500");
        UbCommand(ubPanel, "jumpsw 180 500");
        vtPanel.OnTick(0.1d);
        ubPanel.OnTick(0.1d);

        PluginMovementIntent intent = ub.MovementIntents[^1];
        Assert.True(intent.Forward);
        Assert.True(intent.Run);
        Assert.True(intent.Jump);
        Assert.Equal(vt.MovementIntents, ub.MovementIntents);
    }

    [Fact]
    public void UbUseUsesLikeVtUse()
    {
        var vt = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects = [Carried(100u, "Prismatic Taper")],
        };
        var ub = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects = [Carried(100u, "Prismatic Taper")],
        };

        Command(new MossTankPanel(new FakeHost(vt)), "use Prismatic Taper");
        UbCommand(new MossTankPanel(new FakeHost(ub)), "use Prismatic Taper");

        Assert.Equal(100u, Assert.Single(ub.UsedItemIds));
        Assert.Equal(vt.UsedItemIds, ub.UsedItemIds);
        Assert.Equal(vt.Messages, ub.Messages);
    }

    /// <summary>
    /// Bare /ub is the compatibility line, as /vt ub is; bare /vt stays the
    /// help. /ub help is the same help /vt help prints.
    /// Mutation: letting a bare /ub fall through prints the help instead.
    /// </summary>
    [Fact]
    public void BareUbPrintsTheCompatibilityLineAndUbHelpPrintsTheHelp()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, string.Empty);
        Assert.Equal(
            [
                $"MossTank UB compatibility {MossTankPanel.PluginVersion}",
                "Type /vt help or /vt help <command> for help.",
            ],
            automation.Messages);

        automation.Messages.Clear();
        UbCommand(panel, "help");
        List<string> ubHelp = [.. automation.Messages];
        automation.Messages.Clear();
        Command(panel, string.Empty);
        Assert.Equal(automation.Messages, ubHelp);
        Assert.Contains(ubHelp, line => line.Contains("setmotion", StringComparison.Ordinal));
    }

    // ── setmotion ───────────────────────────────────────────────────────

    /// <summary>
    /// Each motion name, in any letter case, presses its own key and nothing
    /// else. Mutation: mapping any name to a different key fails its row.
    /// </summary>
    [Theory]
    [InlineData("forward", "Forward")]
    [InlineData("BACKWARD", "Backward")]
    [InlineData("TurnRight", "TurnRight")]
    [InlineData("turnleft", "TurnLeft")]
    [InlineData("StrafeRight", "StrafeRight")]
    [InlineData("strafeLeft", "StrafeLeft")]
    public void SetMotionPressesTheNamedKey(string typed, string key)
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, $"setmotion {typed} 1");

        PluginMovementIntent intent = Assert.Single(automation.MovementIntents);
        Assert.Equal(
            new PluginMovementIntent(
                Forward: key == "Forward",
                Backward: key == "Backward",
                StrafeLeft: key == "StrafeLeft",
                StrafeRight: key == "StrafeRight",
                TurnLeft: key == "TurnLeft",
                TurnRight: key == "TurnRight"),
            intent);
        Assert.Empty(automation.Messages);
    }

    /// <summary>
    /// Forward then its release leaves nothing held.
    /// Mutation: sending the intent again instead of clearing when the last
    /// key goes leaves the character running.
    /// </summary>
    [Fact]
    public void ForwardOnThenOffLeavesNoIntent()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "setmotion forward 1");
        UbCommand(panel, "setmotion forward 0");

        Assert.Single(automation.MovementIntents);
        Assert.Equal(1, automation.ClearMovementCount);
    }

    /// <summary>
    /// Held keys combine, and releasing one keeps the other.
    /// Mutation: sending only the newest key (the old expression's shape)
    /// drops the forward key when the turn is pressed.
    /// </summary>
    [Fact]
    public void ForwardAndTurnRightAreHeldTogether()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "setmotion forward 1");
        UbCommand(panel, "setmotion turnright 1");
        PluginMovementIntent both = automation.MovementIntents[^1];
        Assert.True(both.Forward);
        Assert.True(both.TurnRight);
        Assert.True(both.Run);

        UbCommand(panel, "setmotion turnright 0");
        PluginMovementIntent forwardOnly = automation.MovementIntents[^1];
        Assert.True(forwardOnly.Forward);
        Assert.False(forwardOnly.TurnRight);
        Assert.Equal(0, automation.ClearMovementCount);
    }

    /// <summary>
    /// Walk is the run key let go: it slows what is held and moves nothing on
    /// its own. Mutation: mapping walk to a forward key moves the character.
    /// </summary>
    [Fact]
    public void WalkTurnsRunOffForTheHeldKeysAndMovesNothingAlone()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "setmotion walk 1");
        Assert.Empty(automation.MovementIntents);

        UbCommand(panel, "setmotion forward 1");
        Assert.Equal(
            new PluginMovementIntent(Forward: true, Run: false),
            automation.MovementIntents[^1]);

        UbCommand(panel, "setmotion walk 0");
        Assert.Equal(
            new PluginMovementIntent(Forward: true, Run: true),
            automation.MovementIntents[^1]);
    }

    /// <summary>
    /// The command and the expression hold the same keys: a key the
    /// expression pressed is released by the command.
    /// Mutation: giving the command its own held-key state leaves the
    /// expression's forward key held after the command's release.
    /// </summary>
    [Fact]
    public void TheCommandReleasesAKeyTheExpressionPressed()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "mexec setmotion[`Forward`,1]");
        UbCommand(panel, "setmotion turnleft 1");
        Assert.True(automation.MovementIntents[^1].Forward);
        Assert.True(automation.MovementIntents[^1].TurnLeft);

        UbCommand(panel, "setmotion turnleft 0");
        UbCommand(panel, "setmotion forward 0");
        Assert.Equal(1, automation.ClearMovementCount);
    }

    /// <summary>
    /// Input the grammar refuses gets the reference's usage line; a name it
    /// accepts but does not know gets the list of names.
    /// Mutation: skipping the grammar check sends an intent for "forward 2".
    /// </summary>
    [Theory]
    [InlineData("setmotion")]
    [InlineData("setmotion forward")]
    [InlineData("setmotion forward 2")]
    [InlineData("setmotion forward on")]
    public void BadSetMotionInputPrintsTheUsageLine(string line)
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, line);

        Assert.Empty(automation.MovementIntents);
        Assert.Equal(
            [
                "Bad command syntax",
                "Usage: /vt setmotion <Forward|Backward|TurnRight|TurnLeft|StrafeRight|StrafeLeft|Walk> <0|1>",
            ],
            automation.Messages);
    }

    [Fact]
    public void AnUnknownMotionNameListsTheValidOnes()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "setmotion sideways 1");

        Assert.Empty(automation.MovementIntents);
        Assert.Equal(
            "Invalid option (sideways). Valid values are: Forward, Backward, "
            + "TurnRight, TurnLeft, StrafeRight, StrafeLeft, Walk",
            Assert.Single(automation.Messages));
    }

    // ── prepclick ───────────────────────────────────────────────────────

    /// <summary>
    /// Armed with yes or no, the next confirmation is answered that way.
    /// Mutation: answering accept=true regardless fails the "no" row.
    /// </summary>
    [Theory]
    [InlineData("yes", true, "Click Yes on Swear allegiance?")]
    [InlineData("no", false, "Click No on Swear allegiance?")]
    public void PrepClickAnswersTheNextConfirmationWithTheChoice(
        string choice,
        bool accept,
        string said)
    {
        (FakeAutomation automation, FakeEvents events, MossTankPanel panel) = PrepClickPanel();

        UbCommand(panel, $"prepclick {choice} 10");
        Assert.Equal(
            $"Will click {choice} on the next dialog to appear within 10 seconds",
            automation.Messages[^1]);
        events.RaiseConfirmation(new PluginConfirmation(11u, 2, "Swear allegiance?"));

        Assert.Equal([(11u, accept)], automation.Answered);
        Assert.Equal(said, automation.Messages[^1]);
    }

    /// <summary>
    /// One command, one answer. Mutation: staying armed after answering
    /// answers the second confirmation too.
    /// </summary>
    [Fact]
    public void PrepClickAnswersOnlyOneConfirmation()
    {
        (FakeAutomation automation, FakeEvents events, MossTankPanel panel) = PrepClickPanel();
        int baseline = events.ConfirmationListenerCount;

        UbCommand(panel, "prepclick yes 10");
        events.RaiseConfirmation(new PluginConfirmation(11u, 2, "first"));
        events.RaiseConfirmation(new PluginConfirmation(12u, 2, "second"));

        Assert.Equal([(11u, true)], automation.Answered);
        Assert.Equal(baseline, events.ConfirmationListenerCount);
    }

    /// <summary>
    /// The window runs out and the watch goes with it, in the reference's
    /// words. Mutation: never counting the window down answers a
    /// confirmation that arrives an hour later.
    /// </summary>
    [Fact]
    public void PrepClickExpiresAtTheEndOfItsWindow()
    {
        (FakeAutomation automation, FakeEvents events, MossTankPanel panel) = PrepClickPanel();

        UbCommand(panel, "prepclick yes 2");
        panel.OnTick(1.5d);
        Assert.DoesNotContain(automation.Messages, line => line.StartsWith("Time has expired", StringComparison.Ordinal));
        panel.OnTick(1d);
        Assert.Equal("Time has expired: 2", automation.Messages[^1]);

        events.RaiseConfirmation(new PluginConfirmation(11u, 2, "late"));
        Assert.Empty(automation.Answered);
    }

    /// <summary>
    /// stop cancels a watch and says how far into it the watch got; with no
    /// watch it says so. Mutation: stop only printing leaves the next
    /// confirmation answered.
    /// </summary>
    [Fact]
    public void PrepClickStopCancelsTheWatch()
    {
        (FakeAutomation automation, FakeEvents events, MossTankPanel panel) = PrepClickPanel();

        UbCommand(panel, "prepclick stop");
        Assert.Equal(
            "Message boxes are not currently being watched",
            automation.Messages[^1]);

        UbCommand(panel, "prepclick no 10");
        panel.OnTick(2.5d);
        UbCommand(panel, "prepclick stop");
        Assert.Equal(
            "Stopping... 2.5s passed out of expected 10s",
            automation.Messages[^1]);

        events.RaiseConfirmation(new PluginConfirmation(11u, 2, "after stop"));
        Assert.Empty(automation.Answered);
    }

    /// <summary>
    /// A confirmation raised before the command is not the one it was armed
    /// for. Mutation: listening from construction and remembering the last
    /// confirmation answers the earlier one at arm time.
    /// </summary>
    [Fact]
    public void AConfirmationBeforeTheCommandIsNotAnswered()
    {
        (FakeAutomation automation, FakeEvents events, MossTankPanel panel) = PrepClickPanel();

        events.RaiseConfirmation(new PluginConfirmation(10u, 2, "before"));
        UbCommand(panel, "prepclick yes 10");

        Assert.Empty(automation.Answered);
        events.RaiseConfirmation(new PluginConfirmation(11u, 2, "after"));
        Assert.Equal([(11u, true)], automation.Answered);
    }

    [Theory]
    [InlineData("prepclick")]
    [InlineData("prepclick maybe 10")]
    [InlineData("prepclick yes ten")]
    public void BadPrepClickInputPrintsTheUsageLine(string line)
    {
        (FakeAutomation automation, FakeEvents events, MossTankPanel panel) = PrepClickPanel();
        int baseline = events.ConfirmationListenerCount;

        UbCommand(panel, line);

        Assert.Equal(baseline, events.ConfirmationListenerCount);
        Assert.Equal(
            [
                "Bad command syntax",
                "Usage: /vt prepclick {stop|yes <secondstowatch>|no <secondstowatch>}",
            ],
            automation.Messages);
    }

    [Fact]
    public void AWindowOverAnHourIsRefused()
    {
        (FakeAutomation automation, FakeEvents events, MossTankPanel panel) = PrepClickPanel();
        int baseline = events.ConfirmationListenerCount;

        UbCommand(panel, "prepclick yes 3601");

        Assert.Equal(baseline, events.ConfirmationListenerCount);
        Assert.Equal(
            "3601 is not a valid number of seconds to wait",
            Assert.Single(automation.Messages));
    }

    [Theory]
    [InlineData("setmotion", "Syntax: /vt setmotion <Forward|Backward|TurnRight|TurnLeft|StrafeRight|StrafeLeft|Walk> <0|1>")]
    [InlineData("prepclick", "Syntax: /vt prepclick {stop|yes <secondstowatch>|no <secondstowatch>}")]
    [InlineData("clearmotion", "Syntax: /vt clearmotion")]
    public void TheNewCommandsHaveUsageLines(string command, string usage)
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "help " + command);

        Assert.Equal(usage, automation.Messages[0]);
    }

    private static (FakeAutomation, MossTankPanel) FacingPanel(float heading)
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(heading),
        };
        return (automation, new MossTankPanel(new FakeHost(automation)));
    }

    private static (FakeAutomation, FakeEvents, MossTankPanel) PrepClickPanel()
    {
        var automation = new FakeAutomation();
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        return (automation, (FakeEvents)host.Events, panel);
    }

    private static void UbCommand(MossTankPanel panel, string arguments) =>
        panel.ExecuteVtankCommand(new PluginCommand(
            "ub", arguments, arguments.Length == 0 ? "/ub" : "/ub " + arguments));
}

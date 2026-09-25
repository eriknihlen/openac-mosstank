using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The /ub verb, and the two UtilityBelt commands metas lean on that the
/// client had no answer for: setmotion and prepclick.
/// </summary>
public sealed partial class MossTankPanelTests
{
    // ── /ub reaches the UtilityBelt commands; /vt does not ──────────────

    /// <summary>
    /// A meta written for UtilityBelt types /ub; the line has to reach the
    /// command and do the thing, while the same line on /vt is an unknown
    /// command that does nothing, as it is to the macro itself.
    /// Mutation: dispatching /ub lines as unknown leaves the turn undone;
    /// letting /vt answer them turns the /vt panel too.
    /// </summary>
    [Fact]
    public void UbFaceTurnsAndVtFaceIsUnknown()
    {
        (FakeAutomation vt, MossTankPanel vtPanel) = FacingPanel(0f);
        (FakeAutomation ub, MossTankPanel ubPanel) = FacingPanel(0f);

        Command(vtPanel, "face 90");
        UbCommand(ubPanel, "face 90");
        vtPanel.OnTick(0.05d);
        ubPanel.OnTick(0.05d);

        Assert.Equal(90f, ub.FacedHeadings[^1]);
        Assert.Empty(vt.FacedHeadings);
        Assert.Empty(vt.MovementIntents);
        Assert.Equal(MossTankPanel.UnknownVtankCommand, Assert.Single(vt.Messages));
    }

    [Fact]
    public void UbJumpswJumps()
    {
        (FakeAutomation ub, MossTankPanel ubPanel) = FacingPanel(180f);

        UbCommand(ubPanel, "jumpsw 180 500");
        ubPanel.OnTick(0.1d);

        Assert.Equal(
            [HeldKey(PluginMoveDirection.Forward, PluginMovePace.Walk)],
            ub.Moves);
        Assert.Equal([0.5f], ub.Jumps);
    }

    [Fact]
    public void UbUseUsesAndVtUseIsUnknown()
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
        Assert.Empty(vt.UsedItemIds);
        Assert.Equal(MossTankPanel.UnknownVtankCommand, Assert.Single(vt.Messages));
    }

    /// <summary>
    /// Bare /ub is the compatibility line, as /vt ub is; bare /vt stays the
    /// macro's help. /ub help is UtilityBelt's own list, not the macro's.
    /// Mutation: letting a bare /ub fall through prints the help instead;
    /// pointing /ub help at the /vt help prints the macro's verb lists.
    /// </summary>
    [Fact]
    public void BareUbPrintsTheCompatibilityLineAndUbHelpIsItsOwn()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, string.Empty);
        Assert.Equal(
            [
                $"[UB] MossTank UB compatibility {MossTankPanel.PluginVersion}",
                " Type `/ub help` or `/ub help <command>` for help.",
            ],
            automation.Messages);

        automation.Messages.Clear();
        UbCommand(panel, "help");
        List<string> ubHelp = [.. automation.Messages];
        automation.Messages.Clear();
        Command(panel, string.Empty);
        List<string> vtHelp = [.. automation.Messages];

        Assert.StartsWith("[UB] All available UB commands: /ub {", ubHelp[0], StringComparison.Ordinal);
        Assert.Contains("setmotion", ubHelp[0], StringComparison.Ordinal);
        Assert.DoesNotContain("setmetastate", ubHelp[0], StringComparison.Ordinal);
        Assert.DoesNotContain(vtHelp, line => line.Contains("setmotion", StringComparison.Ordinal));
        Assert.Contains(vtHelp, line => line.Contains("setmetastate", StringComparison.Ordinal));
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
                "[UB] Error: Bad command syntax",
                "[UB] Usage: /ub setmotion <Forward|Backward|TurnRight|TurnLeft|StrafeRight|StrafeLeft|Walk> <0|1>",
            ],
            automation.Messages.Take(2));
    }

    [Fact]
    public void AnUnknownMotionNameListsTheValidOnes()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "setmotion sideways 1");

        Assert.Empty(automation.MovementIntents);
        Assert.Equal(
            "[UB] Error: Invalid option (sideways). Valid values are: Forward, Backward, "
            + "TurnRight, TurnLeft, StrafeRight, StrafeLeft, Walk",
            Assert.Single(automation.Messages));
    }

    // ── prepclick ───────────────────────────────────────────────────────

    /// <summary>
    /// Armed with yes or no, the next confirmation is answered that way.
    /// Every line the tool prints carries the reference's "[UB] PrepClick: "
    /// tag, which profiles wait on (`^\[UB\] PrepClick\: Will click yes`).
    /// Mutation: answering accept=true regardless fails the "no" row; the
    /// bare line without the tag fails both rows.
    /// </summary>
    [Theory]
    [InlineData("yes", true, "[UB] PrepClick: Click Yes on Swear allegiance?")]
    [InlineData("no", false, "[UB] PrepClick: Click No on Swear allegiance?")]
    public void PrepClickAnswersTheNextConfirmationWithTheChoice(
        string choice,
        bool accept,
        string said)
    {
        (FakeAutomation automation, FakeEvents events, MossTankPanel panel) = PrepClickPanel();

        UbCommand(panel, $"prepclick {choice} 10");
        Assert.Equal(
            $"[UB] PrepClick: Will click {choice} on the next dialog to appear within 10 seconds",
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
        Assert.DoesNotContain(automation.Messages, line => line.Contains("Time has expired", StringComparison.Ordinal));
        panel.OnTick(1d);
        Assert.Equal("[UB] PrepClick: Time has expired: 2", automation.Messages[^1]);

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
            "[UB] PrepClick: Message boxes are not currently being watched",
            automation.Messages[^1]);

        UbCommand(panel, "prepclick no 10");
        panel.OnTick(2.5d);
        UbCommand(panel, "prepclick stop");
        Assert.Equal(
            "[UB] PrepClick: Stopping... 2.5s passed out of expected 10s",
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
                "Usage: /ub prepclick {stop|yes <secondstowatch>|no <secondstowatch>}",
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
            "[UB] PrepClick: 3601 is not a valid number of seconds to wait",
            Assert.Single(automation.Messages));
    }

    [Theory]
    [InlineData("setmotion", "[UB] Usage: /ub setmotion <Forward|Backward|TurnRight|TurnLeft|StrafeRight|StrafeLeft|Walk> <0|1>")]
    [InlineData("prepclick", "[UB] Usage: /ub prepclick {stop|yes <secondstowatch>|no <secondstowatch>}")]
    [InlineData("clearmotion", "[UB] Usage: /ub clearmotion")]
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
        panel.ExecuteUbCommand(new PluginCommand(
            "ub", arguments, arguments.Length == 0 ? "/ub" : "/ub " + arguments));
}

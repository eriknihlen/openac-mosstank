using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The commands a meta author types. Each one is pinned on the grammar it
/// accepts and on what reaches the client afterwards, because a command that
/// parses and then calls nothing is exactly the failure a macro cannot see.
/// </summary>
public sealed partial class MossTankPanelTests
{
    // ── /vt ub, /vt help ────────────────────────────────────────────────

    [Fact]
    public void TheCompatibilityVersionLineNamesThePluginVersion()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "ub");

        Assert.Equal(
            $"[UB] MossTank UB compatibility {MossTankPanel.PluginVersion}",
            automation.Messages[0]);
    }

    [Fact]
    public void HelpForOneCommandPrintsItsUsageLine()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "help jump");

        Assert.Equal(
            "[UB] Usage: /ub jump[swzxc] [heading] [holdtime]",
            automation.Messages[0]);
    }

    /// <summary>
    /// A command's full help is the reference's: usage, description, then
    /// "Examples:" and each example under it, as one message -- and a line
    /// that does not parse answers with the same full help after its error.
    /// A command the reference gives no examples still prints the heading.
    /// Mutation: the old help stopped after the description, and the old
    /// bad-syntax answer after the usage.
    /// </summary>
    [Fact]
    public void FullHelpPrintsTheReferencesExamples()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "delay 1000");
        Assert.Equal(
            [
                "[UB] Error: Bad command syntax",
                "[UB] Usage: /ub delay <millisecondDelay> <command>",
                "Description: Runs a command after a delay.",
                "Examples:",
                " /ub delay 5000 /say hello",
                "  Runs \"/say hello\" after a 3000ms delay (3 seconds)",
            ],
            automation.Messages);

        automation.Messages.Clear();
        UbCommand(panel, "help vitae");
        Assert.Equal(
            [
                "[UB] Usage: /ub vitae",
                "Description: Thinks to yourself with your current vitae percentage.",
                "Examples:",
            ],
            automation.Messages);
    }

    /// <summary>
    /// <c>/ub mexecm</c> runs the expression and says nothing about it; only
    /// an error is printed, as with the reference's silent form. Mutation:
    /// without the verb it answers "Command not found" and runs nothing.
    /// </summary>
    [Fact]
    public void MexecmRunsTheExpressionSilently()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));
        automation.Messages.Clear();

        UbCommand(panel, "mexecm setvar[quiet,7]");

        Assert.Empty(automation.Messages);
        Assert.Equal(7d, panel.EvaluateExpression("getvar[quiet]").AsNumber());

        UbCommand(panel, "mexecm 1+");
        Assert.Equal(
            "[UB] Error: Error in expression: 1+",
            Assert.Single(automation.Messages, static line => line.StartsWith("[UB]", StringComparison.Ordinal)));
    }

    [Fact]
    public void HelpForAnUnknownCommandSaysSoRatherThanPrintingEverything()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "help notacommand");

        Assert.Equal(
            "No help found for command: notacommand",
            Assert.Single(automation.Messages));
    }

    /// <summary>
    /// Three commands the plugin answers to had no published usage line, so
    /// "/ub help vendor" said there was no such command while "/ub vendor"
    /// worked. The vendor lines repeat the reference's wording; netclients
    /// has a usage line and says plainly that this client cannot do it yet,
    /// which is a different answer from "no such command".
    /// Mutation: drop any of the three entries and its help is refused.
    /// </summary>
    [Theory]
    [InlineData("vendor", "/ub vendor {open[p] <vendorname,vendorid,vendorhex> | buyall | sellall | clearbuy | clearsell | opencancel | addbuy[p] <item> | addsell[p] <item>}")]
    [InlineData("autovendor", "/ub autovendor <cancel|lootProfile>")]
    [InlineData("netclients", "/ub netclients <tag>")]
    public void TheVendorAndNetworkCommandsHaveTheirOwnUsageLines(
        string command,
        string usage)
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "help " + command);

        Assert.Equal("[UB] Usage: " + usage, automation.Messages[0]);
        Assert.NotEmpty(automation.Messages[1]);
    }

    /// <summary>
    /// The network commands work now, so the help describes them rather than
    /// warning the player off. The two broadcasts need usage lines of their
    /// own: a command with none is answered as though it did not exist.
    /// Mutation: drop either entry and its help is refused; leave the old
    /// warning on netclients and the first assertion fails.
    /// </summary>
    [Fact]
    public void TheNetworkCommandsAreDescribedRatherThanWarnedAbout()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "help netclients");
        Assert.DoesNotContain(
            "not available on this client yet",
            automation.Messages[1],
            StringComparison.Ordinal);

        automation.Messages.Clear();
        UbCommand(panel, "help bc");
        Assert.Equal(
            "[UB] Usage: /ub bc [millisecondDelay] <command>",
            automation.Messages[0]);

        automation.Messages.Clear();
        UbCommand(panel, "help bct");
        Assert.Equal(
            "[UB] Usage: /ub bct <tags> [millisecondDelay] <command>",
            automation.Messages[0]);
    }

    // ── /ub ig ──────────────────────────────────────────────────────────

    [Fact]
    public void TheItemGiverFindsItsTargetByTheWholeName()
    {
        var automation = new FakeAutomation
        {
            WorldObjects = [Player(40u, "Zero Cool")],
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "ig muledItems to Zero Cool");

        // The target is resolved before the profile is read, so a missing
        // profile is proof the target was found.
        Assert.Contains(
            "[UB] Error: InventoryManager: ItemGiver Profile does not exist: muledItems",
            automation.Messages);
    }

    [Fact]
    public void TheItemGiversPartialFlagMatchesPartOfTheTargetName()
    {
        var automation = new FakeAutomation
        {
            WorldObjects = [Player(40u, "Zero Cool")],
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "igp muledItems to Zero");

        Assert.Contains(
            "[UB] Error: InventoryManager: ItemGiver Profile does not exist: muledItems",
            automation.Messages);
    }

    [Fact]
    public void TheItemGiverWithoutThePartialFlagRefusesAHalfName()
    {
        var automation = new FakeAutomation
        {
            WorldObjects = [Player(40u, "Zero Cool")],
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "ig muledItems to Zero");

        Assert.Contains("[UB] Error: InventoryManager: player Zero not found", automation.Messages);
    }

    [Fact]
    public void TheItemGiverWithoutATargetPrintsItsUsage()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "ig muledItems");

        Assert.Equal(
            ["[UB] Error: Bad command syntax", "[UB] Usage: /ub ig[p] <lootProfile> to <target>"],
            automation.Messages.Take(2));
    }

    // ── /ub face ────────────────────────────────────────────────────────

    /// <summary>
    /// <c>/ub face</c> asks the client itself to turn to the heading, which
    /// lands on it exactly, and then looks again every tenth of a second:
    /// within one degree is done, anything else asks the client again. A
    /// macro that compares the heading to three decimals depends on the
    /// exact landing. Mutation: going back to held turn keys asks the client
    /// for nothing; a four-degree tolerance calls 7.593 against 9 done.
    /// </summary>
    [Fact]
    public void UbFaceAsksTheClientForTheExactHeadingUntilWithinOneDegree()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(7.593f),
        };
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));
        UbCommand(panel, "opt set Jumper.ThinkComplete true");
        automation.Messages.Clear();

        UbCommand(panel, "face 9");
        Assert.Equal([9f], automation.FacedHeadings);

        // The first look is at once: still 1.4 degrees off, so ask again.
        panel.OnTick(0.02d);
        Assert.Equal([9f, 9f], automation.FacedHeadings);
        // Inside the tenth of a second nothing is asked.
        panel.OnTick(0.05d);
        Assert.Equal(2, automation.FacedHeadings.Count);
        panel.OnTick(0.06d);
        Assert.Equal(3, automation.FacedHeadings.Count);
        Assert.DoesNotContain("/t Test Character, Turning Success", automation.Submitted);

        automation.NavigationSnapshot = NavigationAt(9f);
        panel.OnTick(0.1d);
        Assert.Contains("/t Test Character, Turning Success", automation.Submitted);
        Assert.Equal(3, automation.FacedHeadings.Count);
        Assert.Empty(automation.MovementIntents);
        Assert.False(panel.ActionLocks.IsLocked(ActionLockKind.Navigation));
        // The reference says nothing in chat about a turn that worked.
        Assert.DoesNotContain(automation.Messages, static line =>
            line.Contains("heading", StringComparison.OrdinalIgnoreCase));

        panel.OnTick(1d);
        Assert.Equal(3, automation.FacedHeadings.Count);
    }

    /// <summary>
    /// Five seconds without getting within a degree is a failed turn: the
    /// route is let go and "Turning failed" goes out, as a think when
    /// Jumper.ThinkFail asks for it. Mutation: dropping the time limit keeps
    /// asking the client for ever.
    /// </summary>
    [Fact]
    public void UbFaceGivesUpAfterFiveSeconds()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        UbCommand(panel, "face 90");
        for (int tick = 0; tick < 49; tick++)
            panel.OnTick(0.1d);
        Assert.True(panel.ActionLocks.IsLocked(ActionLockKind.Navigation));
        Assert.DoesNotContain("[UB] Turning failed", automation.Messages);

        panel.OnTick(0.2d);
        Assert.Equal("[UB] Turning failed", automation.Messages[^1]);
        Assert.False(panel.ActionLocks.IsLocked(ActionLockKind.Navigation));
        int asked = automation.FacedHeadings.Count;
        panel.OnTick(1d);
        Assert.Equal(asked, automation.FacedHeadings.Count);

        UbCommand(panel, "opt set Jumper.ThinkFail true");
        UbCommand(panel, "face 90");
        panel.OnTick(5.1d);
        Assert.Contains("/t Test Character, Turning failed", automation.Submitted);
    }

    // ── /ub jump, /ub simplejump ────────────────────────────────────────

    /// <summary>
    /// A jump with a heading turns the way <c>/ub face</c> does, through the
    /// client's own turn and never the turn keys, and charges once it is
    /// within a degree. Mutation: the held-key turn puts turn intents on the
    /// wire and charges four degrees short.
    /// </summary>
    [Fact]
    public void UbJumpWithAHeadingTurnsThroughTheClientBeforeCharging()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(7.593f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "jumpw 9 100");
        Assert.Equal([9f], automation.FacedHeadings);
        panel.OnTick(0.02d);
        Assert.Empty(automation.Jumps);

        automation.NavigationSnapshot = NavigationAt(8.5f);
        panel.OnTick(0.1d);
        Assert.Equal([0.1f], automation.Jumps);
        Assert.Equal([HeldKey(PluginMoveDirection.Forward, PluginMovePace.Run)], automation.Moves);
        Assert.Empty(automation.MovementIntents);
        Assert.DoesNotContain(automation.Messages, static line =>
            line.Contains("heading", StringComparison.OrdinalIgnoreCase));
    }

    /// <summary>
    /// A jump without a heading does not turn at all: it charges at once,
    /// where the character stands. Mutation: turning first asks the client
    /// for a heading nobody gave.
    /// </summary>
    [Fact]
    public void UbJumpWithoutAHeadingChargesAtOnce()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(123f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "jumpsw 500");
        panel.OnTick(0.02d);

        Assert.Empty(automation.FacedHeadings);
        Assert.Equal([0.5f], automation.Jumps);
    }

    /// <summary>
    /// The reference's give-up line is the one a macro waits for: RynCMD
    /// jumps again when it thinks "You have failed to jump too many times".
    /// Mutation: the macro's own give-up wording leaves such a macro waiting.
    /// </summary>
    [Fact]
    public void UbJumpGivesUpWithTheReferenceLine()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(180f),
        };
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));
        UbCommand(panel, "opt set Jumper.ThinkFail true");

        UbCommand(panel, "jumpw 100");
        panel.OnTick(0.05d);
        for (int attempt = 2; attempt <= 3; attempt++)
        {
            ExpireCommandJumpAttempt(panel);
            panel.OnTick(0.05d);
        }
        ExpireCommandJumpAttempt(panel);

        Assert.Equal([0.1f, 0.1f, 0.1f], automation.Jumps);
        // Each charge's key is let go before the next, and after the last.
        Assert.Equal(
            [PluginMoveChannel.Travel, PluginMoveChannel.Travel, PluginMoveChannel.Travel],
            automation.StoppedMoves);
        Assert.Contains(
            "/t Test Character, You have failed to jump too many times.",
            automation.Submitted);
    }

    /// <summary>
    /// A jump leaves with exactly the power its hold time asks for, a tenth
    /// of a full charge per hundred milliseconds, while the client holds its
    /// keys for the charge. Mutation: holding the jump key for the hold time
    /// and letting go on a later tick leaves with whatever the frames in
    /// between added up to, which is more than was asked.
    /// </summary>
    [Fact]
    public void AWalkingForwardJumpLeavesWithExactlyThePowerAsked()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(180f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "jumpsw 300");
        panel.OnTick(0.02d);

        Assert.Equal([0.3f], automation.Jumps);
        Assert.Equal(
            [(PluginMoveDirection.Forward, PluginMovePace.Walk, 0f, PluginMoveUnit.MetersOrDegrees)],
            automation.Moves);
        Assert.DoesNotContain(automation.MovementIntents, static intent => intent.Jump);
    }

    /// <summary>
    /// Two direction letters hold two keys through the charge, each on its
    /// own channel, and without the shift letter the keys run. Mutation:
    /// holding one channel drops the strafe; reading no letter as shift walks.
    /// </summary>
    [Fact]
    public void ADiagonalJumpHoldsForwardAndStrafeLeftThroughTheCharge()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "jumpwz 100");
        panel.OnTick(0.02d);

        Assert.Equal([0.1f], automation.Jumps);
        Assert.Equal(
            [
                (PluginMoveDirection.Forward, PluginMovePace.Run, 0f, PluginMoveUnit.MetersOrDegrees),
                (PluginMoveDirection.StrafeLeft, PluginMovePace.Run, 0f, PluginMoveUnit.MetersOrDegrees),
            ],
            automation.Moves);
    }

    /// <summary>
    /// A hold of nothing is the reference's tap: the least charge the client
    /// takes, since it takes no jump of no power at all. Mutation: passing
    /// the zero on is refused and the jump never leaves.
    /// </summary>
    [Fact]
    public void AZeroHoldJumpsWithTheLeastPowerTheClientTakes()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "jump");
        panel.OnTick(0.02d);

        Assert.Equal([float.Epsilon], automation.Jumps);
        Assert.Empty(automation.Moves);
    }

    /// <summary>
    /// Once the jump is over the keys it held are let go, channel by
    /// channel, and nothing else the client is moving is stopped. Mutation:
    /// leaving them held walks the character on after it lands.
    /// </summary>
    [Fact]
    public void AJumpLetsGoOfItsKeysOnceItIsOver()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "jumpwz 100");
        panel.OnTick(0.02d);
        Assert.Empty(automation.StoppedMoves);

        automation.MoveReport = automation.MoveReport with { JumpSequence = 1 };
        panel.OnTick(0.1d);
        panel.OnTick(0.25d);

        Assert.Equal(
            [PluginMoveChannel.Travel, PluginMoveChannel.Strafe],
            automation.StoppedMoves);
        Assert.False(panel.ActionLocks.IsLocked(ActionLockKind.Navigation));
    }

    /// <summary>
    /// Forward and backward together are two keys on one channel, which a
    /// client-driven move cannot hold, so that jump stays on the held keys.
    /// Mutation: sending it as moves drops one of the two keys.
    /// </summary>
    [Fact]
    public void ForwardAndBackwardTogetherStayOnTheHeldKeys()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "jumpwx 100");
        panel.OnTick(0.02d);

        Assert.Empty(automation.Jumps);
        PluginMovementIntent intent = automation.MovementIntents[^1];
        Assert.True(intent.Jump);
        Assert.True(intent.Forward);
        Assert.True(intent.Backward);
        Assert.True(intent.Run);
    }

    /// <summary>
    /// Shift is the game's walk key, so the shift letter holds the keys at a
    /// walk on the held-keys path too, and no letter runs. Mutation: reading
    /// the letter as run inverts both.
    /// </summary>
    [Theory]
    [InlineData("jumpwx 100", true)]
    [InlineData("jumpswx 100", false)]
    public void TheShiftLetterWalksTheHeldKeys(string command, bool run)
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, command);
        panel.OnTick(0.02d);

        Assert.Equal(run, automation.MovementIntents[^1].Run);
    }

    [Fact]
    public void AShiftForwardJumpWalksForward()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(180f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "jumpsw 180 500");
        panel.OnTick(0.1d);

        Assert.Equal(
            [HeldKey(PluginMoveDirection.Forward, PluginMovePace.Walk)],
            automation.Moves);
        Assert.Equal([0.5f], automation.Jumps);
    }

    [Fact]
    public void ABackwardJumpPressesBackwardAndNotForward()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "jumpsx 300");
        panel.OnTick(0.1d);

        Assert.Equal(
            [HeldKey(PluginMoveDirection.Backward, PluginMovePace.Walk)],
            automation.Moves);
    }

    [Fact]
    public void TheStrafeFlagsPressTheirOwnKeys()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var left = new MossTankPanel(new FakeHost(automation));

        UbCommand(left, "jumpz 100");
        left.OnTick(0.1d);

        var rightAutomation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var right = new MossTankPanel(new FakeHost(rightAutomation));
        UbCommand(right, "jumpc 100");
        right.OnTick(0.1d);

        Assert.Equal([HeldKey(PluginMoveDirection.StrafeLeft, PluginMovePace.Run)], automation.Moves);
        Assert.Equal([HeldKey(PluginMoveDirection.StrafeRight, PluginMovePace.Run)], rightAutomation.Moves);
    }

    /// <summary>
    /// Each direction letter holds its own key, as the reference sets them,
    /// so two letters are a diagonal. Mutation: picking one direction from
    /// the letters drops the second key.
    /// </summary>
    [Theory]
    [InlineData("jumpwz", true, false, true, false)]
    [InlineData("jumpswc", true, false, false, true)]
    [InlineData("jumpxz", false, true, true, false)]
    public void TwoDirectionLettersHoldTwoKeys(
        string verb,
        bool forward,
        bool backward,
        bool left,
        bool right)
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, verb + " 100");
        panel.OnTick(0.01d);

        Assert.Equal([0.1f], automation.Jumps);
        List<PluginMoveDirection> held = automation.Moves.ConvertAll(static move => move.Direction);
        Assert.Equal(
            (forward, backward, left, right),
            (held.Contains(PluginMoveDirection.Forward),
                held.Contains(PluginMoveDirection.Backward),
                held.Contains(PluginMoveDirection.StrafeLeft),
                held.Contains(PluginMoveDirection.StrafeRight)));
        Assert.Equal(2, held.Count);
    }

    /// <summary>
    /// A jump verb with no direction letter is a tap straight up. Pressing
    /// forward for it would walk the character off whatever it is standing
    /// on, which is the whole difference between a loot jump and a fall.
    /// </summary>
    [Fact]
    public void ABareJumpPressesNoMovementKeyAtAll()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "jump");
        panel.OnTick(0.01d);

        Assert.Single(automation.Jumps);
        Assert.Empty(automation.Moves);
        Assert.Empty(automation.MovementIntents);
    }

    /// <summary>
    /// One number on its own is the hold time, not a heading: a jump in place
    /// is the common case, and a macro that meant a heading types two.
    /// </summary>
    [Fact]
    public void OneNumberIsTheHoldTimeAndNotAHeading()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(45f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "jump 300");
        panel.OnTick(0.01d);

        Assert.Empty(automation.FacedHeadings);
        Assert.Equal([0.3f], automation.Jumps);
    }

    [Fact]
    public void AHoldTimeOverASecondIsRefused()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "jumpw 0 2000");

        Assert.Equal(
            "[UB] Error: Jumper: holdtime should be a number between 0 and 1000",
            Assert.Single(automation.Messages));
    }

    [Fact]
    public void AHeadingOutsideTheCompassIsRefused()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "jumpw 400 100");

        Assert.Equal(
            "[UB] Error: Jumper: direction should be a number between 0 and 359",
            Assert.Single(automation.Messages));
    }

    /// <summary>
    /// A heading is digits and decimal points only, as the reference's
    /// grammar reads one: "NaN" and "Infinity" are no heading at all, and a
    /// jump never starts toward one. Mutation: the float parse alone takes
    /// "NaN", which slips past the compass check and starts the jump.
    /// </summary>
    [Theory]
    [InlineData("NaN")]
    [InlineData("Infinity")]
    [InlineData("1e2")]
    public void AHeadingThatIsNotANumberIsBadSyntax(string heading)
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, $"jumpw {heading} 100");
        panel.OnTick(0.1d);

        Assert.Equal("[UB] Error: Bad command syntax", automation.Messages[0]);
        Assert.Empty(automation.FacedHeadings);
        Assert.Empty(automation.MovementIntents);
    }

    [Fact]
    public void ASecondJumpWhileOneIsInTheAirIsRefused()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "jumpw 0 100");
        automation.Messages.Clear();
        UbCommand(panel, "jumpw 0 100");

        Assert.Equal(
            "[UB] Error: Jumper: You are already jumping. try again later.",
            Assert.Single(automation.Messages));
    }

    /// <summary>
    /// The older grammar still runs: a second word of true or false cannot be
    /// a hold time, so routes and macros written against it keep working.
    /// </summary>
    [Fact]
    public void TheOlderJumpGrammarStillStrafes()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(90f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "jump 90 true 800 strafeleft");
        panel.OnTick(0.1d);

        Assert.Equal(
            [HeldKey(PluginMoveDirection.StrafeLeft, PluginMovePace.Walk)],
            automation.Moves);
        Assert.Equal([0.8f], automation.Jumps);
    }

    [Fact]
    public void SimpleJumpJumpsWhereTheCharacterAlreadyStands()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(123f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "simplejump 200");
        panel.OnTick(0.1d);

        Assert.Empty(automation.FacedHeadings);
        Assert.Equal([0.2f], automation.Jumps);
        Assert.Empty(automation.Moves);
    }

    [Fact]
    public void SimpleJumpRefusesAHoldTimeOverASecond()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "simplejump 5000");

        Assert.Equal(
            "[UB] Error: Jumper: holdtime should be a number between 0 and 1000",
            Assert.Single(automation.Messages));
    }

    // ── /ub calcdamage ──────────────────────────────────────────────────

    [Fact]
    public void CalcDamageAddsTheCantripAndTheTinkersStillAvailable()
    {
        var automation = new FakeAutomation
        {
            WorldObjects =
            [
                new PluginWorldObject(
                    70u, 0u, "Ice Bow", PluginObjectClass.MissileWeapon,
                    0u, 0u, 0u)
                {
                    HasAppraisalData = true,
                    // Major Blood Thirst: +4 to the maximum damage.
                    SpellIds = [2586u],
                },
            ],
        };
        automation.Properties[70u] = new PluginItemProperties(
            new Dictionary<uint, int>
            {
                [204u] = 6,  // elemental damage bonus
                [171u] = 2,  // times tinkered
                [179u] = 0,  // not imbued
            },
            new Dictionary<uint, long>(),
            new Dictionary<uint, bool>(),
            new Dictionary<uint, double>(),
            new Dictionary<uint, string>(),
            new Dictionary<uint, uint>(),
            new Dictionary<uint, uint>())
        {
            WeaponProfile = new PluginWeaponProfile(
                0, 20, 0u, 50, 0.5d, 2d, 1d, 1d, 1d, 0),
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(70u);

        UbCommand(panel, "calcdamage");

        // Ten tinkers less two used and one reserved for the imbue: seven,
        // each worth 0.04 on the damage modifier.
        Assert.Contains(
            "[UB] 7 mahogany salvage adds 0.28 to DamageModifier",
            automation.Messages);
        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith(
                "[UB] Calculated Formula: (50(+4 from cantrips) + 6)",
                StringComparison.Ordinal));
    }

    /// <summary>
    /// Each cantrip that lifts the maximum damage is named, in the order the
    /// item carries its spells and before the sums that use them: the total
    /// "+6 from cantrips" says nothing about which two spells made it.
    /// Mutation: drop the per-spell lines and only the total is printed.
    /// </summary>
    [Fact]
    public void CalcDamageNamesEveryCantripThatLiftsTheMaximumDamage()
    {
        var automation = new FakeAutomation
        {
            WorldObjects =
            [
                new PluginWorldObject(
                    72u, 0u, "Ice Bow", PluginObjectClass.MissileWeapon,
                    0u, 0u, 0u)
                {
                    HasAppraisalData = true,
                    // Major Blood Thirst (+4), then Minor Blood Thirst (+2).
                    SpellIds = [2586u, 1u, 2598u],
                },
            ],
        };
        automation.CatalogSpells =
        [
            NamedSpell(2586u, 1u, "Major Blood Thirst", 1u),
            NamedSpell(2598u, 1u, "Minor Blood Thirst", 1u),
        ];
        automation.Properties[72u] = new PluginItemProperties(
            new Dictionary<uint, int>
            {
                [204u] = 0,
                [171u] = 10,  // no tinkers left, so no salvage line
                [179u] = 1,
            },
            new Dictionary<uint, long>(),
            new Dictionary<uint, bool>(),
            new Dictionary<uint, double>(),
            new Dictionary<uint, string>(),
            new Dictionary<uint, uint>(),
            new Dictionary<uint, uint>())
        {
            WeaponProfile = new PluginWeaponProfile(
                0, 20, 0u, 50, 0.5d, 2d, 1d, 1d, 1d, 0),
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(72u);

        UbCommand(panel, "calcdamage");

        Assert.Equal(
            [
                "[UB] Spell Major Blood Thirst buffs MaxDamage by 4",
                "[UB] Spell Minor Blood Thirst buffs MaxDamage by 2",
            ],
            automation.Messages.Take(2));
        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith(
                "[UB] Calculated Formula: (50(+6 from cantrips) + 0)",
                StringComparison.Ordinal));
    }

    [Fact]
    public void CalcDamageRefusesAnItemThatHasNotBeenExamined()
    {
        var automation = new FakeAutomation
        {
            WorldObjects =
            [
                new PluginWorldObject(
                    71u, 0u, "Ice Bow", PluginObjectClass.MissileWeapon,
                    0u, 0u, 0u),
            ],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(71u);

        UbCommand(panel, "calcdamage");

        Assert.Equal(
            "[UB] Error: Ice Bow does not have id data, please examine it first.",
            Assert.Single(automation.Messages));
    }

    // ── /vt opt ─────────────────────────────────────────────────────────

    [Fact]
    public void AnOptionNameWithADotReachesTheUbSettings()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        UbCommand(panel, "opt set AutoVendor.Enabled true");
        automation.Messages.Clear();
        UbCommand(panel, "opt get AutoVendor.Enabled");

        Assert.Equal(
            "[UB] AutoVendor.Enabled (Profile) = True",
            Assert.Single(automation.Messages));
    }

    /// <summary>
    /// An opt value line is the reference's <c>FullName (SettingType) =
    /// value</c>: the reference's own name for a row called something else
    /// here, Profile or State rather than the value's shape, a colour as
    /// 0x and eight hex digits, a text class by its name, and a list as
    /// [a,b]. Macros match on these lines. Mutation: the old
    /// <c>Name (Kind) = display</c> shape fails every row.
    /// </summary>
    [Theory]
    [InlineData("ItemGiver.Think", "InventoryManager.IGThink (Profile) = False")]
    [InlineData("InventoryManager.IGRange", "InventoryManager.IGRange (Profile) = 15")]
    [InlineData("Nametags.Player.TagColor", "Nametags.Player.TagColor (Profile) = 0xFF00FFFF")]
    [InlineData("Nametags.MaxRange", "Nametags.MaxRange (Profile) = 35")]
    [InlineData("Plugin.ErrorMessageDisplay.Color", "Plugin.ErrorMessageDisplay.Color (Profile) = Help")]
    [InlineData("NetworkUI.SelectedTag", "NetworkUI.SelectedTag (State) = All")]
    [InlineData("Networking.Tags", "Networking.Tags (State) = []")]
    public void AnOptValueLineIsTheReferencesFullDisplayValue(string name, string line)
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));
        automation.Messages.Clear();

        UbCommand(panel, "opt get " + name);

        Assert.Equal("[UB] " + line, Assert.Single(automation.Messages));
    }

    /// <summary>
    /// A set with no value after the name is not a set the reference's
    /// grammar accepts: it answers with its bad-syntax error and the usage,
    /// and prints no value. Mutation: reading it as a get prints the value.
    /// </summary>
    [Fact]
    public void AnOptSetWithoutAValueIsBadSyntax()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));
        automation.Messages.Clear();

        UbCommand(panel, "opt set Jumper.Attempts");

        Assert.Equal("[UB] Error: Bad command syntax", automation.Messages[0]);
        Assert.StartsWith("[UB] Usage: /ub opt ", automation.Messages[1], StringComparison.Ordinal);
        Assert.DoesNotContain(
            automation.Messages,
            static line => line.Contains("Jumper.Attempts (", StringComparison.Ordinal));
    }

    /// <summary>
    /// A list setting is edited in place: add appends what is not there
    /// yet, remove takes names out, clear empties it, and anything else is
    /// refused, in the reference's words, without touching the list.
    /// Mutation: parsing the text as the new list, the old behaviour,
    /// leaves Tags = ["add RynTest"]; the old refusal wording fails the
    /// unknown-verb lines.
    /// </summary>
    [Fact]
    public void OptSetEditsAListSettingWithAddRemoveAndClear()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        UbCommand(panel, "opt set Networking.Tags add RynTest");
        UbCommand(panel, "opt set Networking.Tags add Other,RynTest,Third");
        automation.Messages.Clear();
        UbCommand(panel, "opt get Networking.Tags");
        Assert.Equal(
            "[UB] Networking.Tags (State) = [RynTest,Other,Third]",
            Assert.Single(automation.Messages));

        UbCommand(panel, "opt set Networking.Tags remove Other,Missing");
        automation.Messages.Clear();
        UbCommand(panel, "opt set Networking.Tags replace everything");
        Assert.Equal("[UB] Error: Unknown verb: everything", Assert.Single(automation.Messages));
        automation.Messages.Clear();
        UbCommand(panel, "opt set Networking.Tags replace");
        Assert.Equal("[UB] Error: Unknown verb: replace", Assert.Single(automation.Messages));
        automation.Messages.Clear();
        UbCommand(panel, "opt get Networking.Tags");
        Assert.Equal(
            "[UB] Networking.Tags (State) = [RynTest,Third]",
            Assert.Single(automation.Messages));

        automation.Messages.Clear();
        UbCommand(panel, "opt set Networking.Tags clear");
        Assert.Equal(
            "[UB] Networking.Tags (State) = []",
            Assert.Single(automation.Messages));
    }

    /// <summary>
    /// uboptget and uboptset read and write the UB settings the opt command
    /// and the UB tab show, a list setting as a list. Mutation: routing them
    /// to the macro's option bag, the old behaviour, leaves the tag list
    /// reading 0 and the vendor switch on the page still True.
    /// </summary>
    [Fact]
    public void UbOptGetAndSetReachTheUbSettings()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        UbCommand(panel, "opt set Networking.Tags add RynTest");
        Assert.Equal(
            "RynTest",
            panel.EvaluateExpression("listgetitem[uboptget[`Networking.Tags`],0]").AsString());

        Assert.True(panel.EvaluateExpression("uboptset[`AutoVendor.Enabled`,0]").IsTruthy);
        Assert.False(panel.EvaluateExpression("uboptget[`AutoVendor.Enabled`]").IsTruthy);
        automation.Messages.Clear();
        UbCommand(panel, "opt get AutoVendor.Enabled");
        Assert.Equal("[UB] AutoVendor.Enabled (Profile) = False", Assert.Single(automation.Messages));

        Assert.True(panel.EvaluateExpression(
            "uboptset[`Networking.Tags`,listcreate[`A`,`B`]]").IsTruthy);
        Assert.False(panel.EvaluateExpression("uboptset[`Networking.Tags`,`A`]").IsTruthy);
        automation.Messages.Clear();
        UbCommand(panel, "opt get Networking.Tags");
        Assert.Equal("[UB] Networking.Tags (State) = [A,B]", Assert.Single(automation.Messages));
    }

    /// <summary>
    /// The setting names macros set at startup are all accepted: the
    /// original's spelling of a renamed row reaches that row, the two
    /// choices already made here read what is in force whatever is written,
    /// the settings profile opens the named profile, and the rest are kept
    /// and read back. Mutation: without the accepted names and spellings,
    /// every one of them answers "unknown option name".
    /// </summary>
    [Fact]
    public void OptSetAcceptsTheSettingNamesMacrosUse()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        foreach (string command in (string[])
        [
            "opt set Plugin.SettingsProfile CMD",
            "opt set VTank.PatchExpressionEngine False",
            "opt set VTank.VitalSharing False",
            "opt set InventoryManager.IGThink True",
            "opt set InventoryManager.TreatStackAsSingleItem True",
            "opt set Plugin.PortalThink True",
            "opt set AutoSalvage.Think True",
            "opt set AutoTrade.Think True",
            "opt set Plugin.VideoPatch True",
            "opt set Plugin.VideoPatchFocus True",
            "opt set Looter.EnableChests True",
        ])
        {
            automation.Messages.Clear();
            UbCommand(panel, command);
            Assert.DoesNotContain(
                automation.Messages,
                static line => line.Contains("invalid option", StringComparison.OrdinalIgnoreCase));
        }

        Assert.True(panel.EvaluateExpression("uboptget[`VTank.PatchExpressionEngine`]").IsTruthy);
        Assert.False(panel.EvaluateExpression(
            "uboptget[`InventoryManager.TreatStackAsSingleItem`]").IsTruthy);
        Assert.False(panel.EvaluateExpression("uboptget[`Sharing.Vitals`]").IsTruthy);
        Assert.True(panel.EvaluateExpression("uboptget[`ItemGiver.Think`]").IsTruthy);
        Assert.Equal("CMD", panel.SelectedUbProfile);
        Assert.Equal("CMD", panel.EvaluateExpression("uboptget[`Plugin.SettingsProfile`]").AsString());
        Assert.True(panel.EvaluateExpression("uboptget[`Plugin.VideoPatch`]").IsTruthy);
        Assert.True(panel.EvaluateExpression("uboptget[`Looter.EnableChests`]").IsTruthy);
        Assert.True(panel.EvaluateExpression("uboptset[`Looter.EnableChests`,0]").IsTruthy);
        Assert.False(panel.EvaluateExpression("uboptget[`Looter.EnableChests`]").IsTruthy);

        // The portal switch is wired: the not-found line becomes a think.
        automation.Messages.Clear();
        UbCommand(panel, "portal Gateway");
        Assert.Contains("/t Test Character, Could not find a portal", automation.Submitted);
    }

    // ── /ub myquests, quit, autostack, autocram, playeroption ──────────

    /// <summary>
    /// myquests asks the server for the quest list again, even while the
    /// login refresh is still waiting, so the quest functions read current
    /// flags.
    /// Mutation: without the verb it answers "Unknown /vt command" and
    /// nothing is asked.
    /// </summary>
    [Fact]
    public void MyQuestsAsksForTheQuestListAgain()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));
        Assert.Contains("/myquests", automation.Submitted);
        automation.Submitted.Clear();
        automation.Messages.Clear();

        UbCommand(panel, "myquests");

        Assert.Equal("[UB] QuestTracker: Refreshing quests", Assert.Single(automation.Messages));
        Assert.Contains("/myquests", automation.Submitted);
    }

    /// <summary>
    /// quit closes the client by the host's own close route. Mutation:
    /// without the verb the window is never asked.
    /// </summary>
    [Fact]
    public void QuitAsksTheClientToClose()
    {
        var automation = new FakeAutomation();
        var window = new RecordingWindow();
        var panel = new MossTankPanel(new FakeHost(automation) { Window = window });

        UbCommand(panel, "quit");

        Assert.Equal(1, window.CloseRequests);
        Assert.Equal("[UB] Quitting Client", Assert.Single(automation.Messages));
    }

    /// <summary>
    /// autostack and autocram answer even with nothing to do. Mutation:
    /// without the verbs they answer "Unknown /vt command".
    /// </summary>
    [Fact]
    public void AutoStackAndAutoCramSayWhenThereIsNothingToDo()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "autostack");
        UbCommand(panel, "autocram");

        Assert.Equal(
            ["[UB] AutoStack - nothing to do", "[UB] AutoCram - nothing to do"],
            automation.Messages);
    }

    /// <summary>
    /// clearbugged starts on /ub, is driven by the panel's own tick, and a
    /// session that ends takes the run with it. Mutation: without the tick
    /// nothing is asked; without the session reset the run goes on asking
    /// after the next login.
    /// </summary>
    [Fact]
    public void ClearBuggedRunsOnThePanelTickAndStopsWithTheSession()
    {
        var automation = new FakeAutomation
        {
            ItemEntries = [BareItem(0x8000_0010u, "Phantom"), BareItem(0x8000_0011u, "Other")],
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.OnTick(0.1d);

        UbCommand(panel, "clearbugged");
        for (int frame = 0; frame < 3; frame++)
            panel.OnTick(0.1d);

        Assert.Equal(
            "[UB] InventoryManager: ClearBugged: Identifying 2 items, to check for bugged items...",
            automation.Messages[0]);
        Assert.Equal([0x8000_0010u], automation.Identified);

        automation.IsAvailable = false;
        panel.OnTick(0.1d);
        automation.IsAvailable = true;
        automation.Messages.Clear();
        for (int frame = 0; frame < 200; frame++)
            panel.OnTick(0.1d);

        Assert.Equal([0x8000_0010u], automation.Identified);
        Assert.DoesNotContain(automation.Messages, static line =>
            line.StartsWith("ClearBugged", StringComparison.Ordinal));
    }

    /// <summary>
    /// playeroption changes one of the character's own options through the
    /// host, reading on/true as on and anything else as off, and names the
    /// valid options when asked or when given a name that is not one.
    /// Mutation: without the verb nothing changes.
    /// </summary>
    [Fact]
    public void PlayerOptionChangesTheCharactersOwnOption()
    {
        var options = new RecordingCharacterOptions();
        var automation = new FakeAutomation { CharacterOptions = options };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "playeroption allowgive off");
        UbCommand(panel, "playeroption FellowshipAutoAcceptRequests true");

        Assert.Equal(
            [("allowgive", false), ("FellowshipAutoAcceptRequests", true)],
            options.Changes);
        Assert.Equal(
            ["[UB] Setting AllowGive = False", "[UB] Setting FellowshipAutoAcceptRequests = True"],
            automation.Messages);

        automation.Messages.Clear();
        UbCommand(panel, "playeroption NoSuchOption on");
        UbCommand(panel, "playeroption list");
        Assert.All(automation.Messages, static line =>
            Assert.Contains("AllowGive, FellowshipAutoAcceptRequests", line, StringComparison.Ordinal));
        Assert.Equal(2, options.Changes.Count);
    }

    [Fact]
    public void ToggleFlipsAUbSwitch()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        UbCommand(panel, "opt get DungeonMaps.Enabled");
        string before = Assert.Single(automation.Messages);
        automation.Messages.Clear();

        UbCommand(panel, "opt toggle DungeonMaps.Enabled");

        string after = Assert.Single(automation.Messages);
        Assert.NotEqual(before, after);
        Assert.Equal(
            before.EndsWith("True", StringComparison.Ordinal)
                ? "[UB] DungeonMaps.Enabled (Profile) = False"
                : "[UB] DungeonMaps.Enabled (Profile) = True",
            after);
    }

    /// <summary>
    /// Where the two groups could both answer a name, the macro's own option
    /// wins: a meta that has always meant EnableCombat must keep meaning it.
    /// </summary>
    [Fact]
    public void ToggleFlipsTheMacrosOwnOptionByItsPlainName()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));
        bool before = panel.GetMetaOptionForTest("EnableCombat");

        Command(panel, "opt toggle EnableCombat");

        Assert.Equal(!before, panel.GetMetaOptionForTest("EnableCombat"));
        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith("Set option EnableCombat", StringComparison.Ordinal));
    }

    /// <summary>
    /// A get, set or toggle of a name no setting has is the reference's
    /// invalid option: the error, then one error line per character setting
    /// naming it -- and only the character settings, as the reference lists
    /// its per-character state and nothing else. Mutation: dropping the
    /// list leaves only the first line; listing every setting names
    /// Jumper.Attempts.
    /// </summary>
    [Theory]
    [InlineData("opt get Nope.Missing")]
    [InlineData("opt set Nope.Missing 1")]
    [InlineData("opt toggle Nope.Missing")]
    public void AnInvalidOptionListsTheCharacterSettings(string command)
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        UbCommand(panel, command);

        string[] characterSettings = UbSettingDefinitions.All
            .Where(static row => row.Scope == UbSettingScope.Character)
            .Select(static row => "[UB] Error:   - " + row.Name)
            .ToArray();
        Assert.NotEmpty(characterSettings);
        Assert.Equal(
            ["[UB] Error: Invalid option: nope.missing", .. characterSettings],
            automation.Messages);
        Assert.Contains("[UB] Error:   - Networking.Tags", automation.Messages);
        Assert.DoesNotContain("[UB] Error:   - Jumper.Attempts", automation.Messages);
    }

    /// <summary>
    /// A toggle of a setting that is not a switch is refused in the
    /// reference's words: its toggle reads the value as a true/false and
    /// reports the failed read. Mutation: any other wording fails the line.
    /// </summary>
    [Fact]
    public void TogglingSomethingThatIsNotASwitchSaysSo()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        UbCommand(panel, "opt toggle DungeonMaps.Opacity");

        Assert.Equal(
            "[UB] Error: Unable to toggle setting DungeonMaps.Opacity: Specified cast is not valid.",
            Assert.Single(automation.Messages));
    }

    /// <summary>
    /// Each word lists its own group and only that: /vt opt the macro's
    /// options, /ub opt the UB settings. Mutation: listing the UB settings
    /// under /vt opt (the old shared list) fails the first half.
    /// </summary>
    [Fact]
    public void EachOptionListShowsOnlyItsOwnGroup()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        Command(panel, "opt list");

        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith("Available options:", StringComparison.Ordinal));
        Assert.DoesNotContain(
            automation.Messages,
            static line => line.Contains("DungeonMaps.Enabled", StringComparison.Ordinal));

        automation.Messages.Clear();
        UbCommand(panel, "opt list");

        Assert.Equal("[UB] All Settings:", automation.Messages[0]);
        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith("DungeonMaps.Enabled (Profile) = ", StringComparison.Ordinal));
        Assert.DoesNotContain(
            automation.Messages,
            static line => line.StartsWith("Available options:", StringComparison.Ordinal));
    }

    // ── /ub pos, /ub id, /ub vitae, /ub combatstate, /ub date ───────────

    [Fact]
    public void PosPrintsTheSelectedObjectsCoordinatesAndCell()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects =
            [
                new PluginWorldObject(
                    80u, 0u, "Lifestone", PluginObjectClass.Lifestone, 0u, 0u, 0u)
                {
                    HasPosition = true,
                    IsLandscape = true,
                    Position = new PluginNavigationPosition(
                        0x00AB0102u, 3.5d, -1.25d, 12d, 0f, IsOutdoor: true),
                },
            ],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(80u);

        UbCommand(panel, "pos");

        Assert.Equal("[UB] Id: 80 ( 0x00000050 )", automation.Messages[0]);
        Assert.Equal("[UB] Coords: 1.2S, 3.5E", automation.Messages[1]);
        Assert.Equal("[UB] Landcell: 0x00AB0102", automation.Messages[2]);
    }

    [Fact]
    public void PosWithNothingSelectedSaysSo()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "pos");

        Assert.Equal("[UB] Error: pos: No object selected", Assert.Single(automation.Messages));
    }

    [Fact]
    public void IdPrintsTheSelectedObjectsIdBothWaysRound()
    {
        var automation = new FakeAutomation
        {
            WorldObjects =
            [
                new PluginWorldObject(
                    0x50000123u, 0u, "Mule", PluginObjectClass.Player, 0u, 0u, 0u),
            ],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(0x50000123u);

        UbCommand(panel, "id");

        Assert.Equal(
            "[UB] Id: 1342177571 ( 0x50000123 )",
            Assert.Single(automation.Messages));
    }

    [Fact]
    public void VitaeIsThoughtAsAPercentage()
    {
        var automation = new FakeAutomation { ObjectId = 1u };
        automation.Properties[1u] = new PluginItemProperties(
            new Dictionary<uint, int>(),
            new Dictionary<uint, long>(),
            new Dictionary<uint, bool>(),
            new Dictionary<uint, double> { [129u] = 0.95d },
            new Dictionary<uint, string>(),
            new Dictionary<uint, uint>(),
            new Dictionary<uint, uint>());
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "vitae");

        Assert.Contains("/t Test Character, My vitae is 95%", automation.Submitted);
    }

    /// <summary>
    /// A think is a tell to yourself and nothing else: with no name to tell,
    /// nothing is sent and nothing is printed in its place, as the
    /// reference's tell goes nowhere. Mutation: the old local "You think"
    /// line printed a system line that looked like the server's answer.
    /// </summary>
    [Fact]
    public void AThinkThatCannotBeSentPrintsNothingInItsPlace()
    {
        var automation = new FakeAutomation { Name = string.Empty };
        var panel = new MossTankPanel(new FakeHost(automation));
        automation.Messages.Clear();
        automation.Submitted.Clear();

        UbCommand(panel, "vitae");

        Assert.Empty(automation.Submitted);
        Assert.DoesNotContain(
            automation.Messages,
            static line => line.StartsWith("You think", StringComparison.Ordinal));
    }

    [Fact]
    public void CombatStateAsksTheClientToChangeStance()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "combatstate melee");

        Assert.Equal(PluginCombatMode.Melee, automation.CombatSnapshot.Mode);
    }

    [Fact]
    public void AnUnknownCombatStateIsRefusedByName()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "combatstate sideways");

        Assert.Equal(
            "[UB] Error: sideways is not a valid option",
            Assert.Single(automation.Messages));
    }

    /// <summary>
    /// The date is formatted in the invariant culture, so a macro reading its
    /// own chat sees the same words on a Swedish machine as on a US one.
    /// </summary>
    [Fact]
    public void TheUtcDateIsFormattedInTheInvariantCulture()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "dateutc dddd");

        Assert.Equal(
            "[UB] Current Date: "
            + DateTime.UtcNow.ToString("dddd", CultureInfo.InvariantCulture),
            Assert.Single(automation.Messages));
    }

    [Fact]
    public void ABareDateUsesTheDefaultFormat()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "date");

        Assert.Equal(
            "[UB] Current Date: "
            + DateTime.Now.ToString(
                "dddd dd MMMM HH:mm:ss", CultureInfo.InvariantCulture),
            Assert.Single(automation.Messages));
    }

    // ── /ub delay ───────────────────────────────────────────────────────

    [Fact]
    public void ADelayedCommandRunsOnlyOnceItsWaitIsOver()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "delay 1000 /say hello");
        panel.OnTick(0.5d);
        Assert.DoesNotContain("/say hello", automation.Submitted);

        panel.OnTick(0.6d);

        Assert.Contains("/say hello", automation.Submitted);
    }

    /// <summary>
    /// The reference schedules and runs a delayed line in silence unless its
    /// debug setting is on; then it says both, as debug lines (Abuse, 14).
    /// Mutation: printing the scheduling line as an ordinary line shows it
    /// with debug off; ignoring the setting hides it with debug on.
    /// </summary>
    [Fact]
    public void ADelayedCommandIsReportedOnlyInTheDebugOutput()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "delay 1000 /say hello");
        panel.OnTick(1.1d);
        Assert.Contains("/say hello", automation.Submitted);
        Assert.Empty(automation.Messages);

        UbCommand(panel, "opt set Plugin.Debug true");
        automation.Posted.Clear();
        UbCommand(panel, "delay 250 /say again");
        panel.OnTick(0.3d);

        Assert.Equal(
            [
                ("[UB] Scheduling command `/say again` with delay of 250ms", UbChat.DebugChatType),
                ("[UB] Plugin: Executing command `/say again` (delay was 250ms)", UbChat.DebugChatType),
            ],
            automation.Posted);
    }

    /// <summary>
    /// A set leaves the reference's ordinary line while debug is off; with
    /// it on the same line comes as a debug line instead. Ordinary lines are
    /// System (5) and errors Help (15). Mutation: always echoing as an
    /// ordinary line fails the last row; the plain system class fails all.
    /// </summary>
    [Fact]
    public void UbLinesUseTheReferencesTextClasses()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "opt set Jumper.Attempts 4");
        UbCommand(panel, "opt set Nope.Missing 1");
        UbCommand(panel, "opt set Plugin.Debug true");
        UbCommand(panel, "opt set Jumper.Attempts 5");

        Assert.Equal(
            [
                ("[UB] Jumper.Attempts (Profile) = 4", UbChat.GenericChatType),
                ("[UB] Error: Invalid option: nope.missing", UbChat.ErrorChatType),
                ("[UB] Plugin.Debug (Profile) = True", UbChat.DebugChatType),
                ("[UB] Jumper.Attempts (Profile) = 5", UbChat.DebugChatType),
            ],
            // The invalid option's list of character settings is its own
            // test's; here only the error's class matters.
            automation.Posted.Where(static line =>
                !line.Text.StartsWith("[UB] Error:   - ", StringComparison.Ordinal)));
    }

    /// <summary>
    /// A UB message of several lines is one message: every line below the
    /// heading comes in the heading's class, and the kind's switch hides the
    /// whole of it. Mutation: posting the lines below the heading as plain
    /// system lines fails the classes and leaves them showing with the
    /// switch off.
    /// </summary>
    [Fact]
    public void TheLinesBelowAUbHeadingShareItsClassAndItsSwitch()
    {
        var automation = new FakeAutomation
        {
            LoginRoster = [new PluginLoginCharacter(0x50u, "Zeke", 0, false)],
        };
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));
        Command(panel, "mexec setvar[myvar,1]");
        automation.Messages.Clear();
        automation.Posted.Clear();

        foreach (string line in (string[])
            ["", "help", "help jump", "opt list", "listvars", "login list"])
        {
            UbCommand(panel, line);
        }
        Assert.Equal(automation.Messages.Count, automation.Posted.Count);
        Assert.All(
            automation.Posted,
            static line => Assert.Equal(UbChat.GenericChatType, line.Kind));
        Assert.Contains(
            automation.Posted,
            static line => line.Text == " Type `/ub help` or `/ub help <command>` for help.");

        automation.Messages.Clear();
        automation.Posted.Clear();
        UbCommand(panel, "mexec 1+");
        Assert.Equal(2, automation.Posted.Count(static line => line.Kind == UbChat.ErrorChatType));
        Assert.StartsWith("  ", automation.Posted[^1].Text, StringComparison.Ordinal);

        UbCommand(panel, "opt set Plugin.GenericMessageDisplay.Enabled false");
        UbCommand(panel, "opt set Plugin.ErrorMessageDisplay.Enabled false");
        UbCommand(panel, "opt set Plugin.ExpressionMessageDisplay.Enabled false");
        automation.Messages.Clear();
        foreach (string line in (string[])
            ["", "help", "help jump", "opt list", "listvars", "login list", "mexec 1+"])
        {
            UbCommand(panel, line);
        }
        Assert.Empty(automation.Messages);
    }

    /// <summary>
    /// A tool command that does not parse answers exactly as every other
    /// /ub command does: the error, then the command's full help as one
    /// message, examples included. Mutation: the tools' old answer stopped
    /// after the usage line.
    /// </summary>
    [Theory]
    [InlineData("vendor frobnicate", "vendor")]
    [InlineData("vendor addbuyp 5", "vendor")]
    [InlineData("equip wear profile.utl", "equip")]
    [InlineData("xp spend", "xp")]
    public void AToolLineThatDoesNotParsePrintsTheFullHelp(string line, string verb)
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));
        UbCommandUsage usage = UbCommandHelp.Require(verb);
        Assert.NotEmpty(usage.Examples);

        UbCommand(panel, line);

        Assert.Equal(
            [
                "[UB] Error: Bad command syntax",
                "[UB] Usage: " + usage.Usage,
                "Description: " + usage.Summary,
                "Examples:",
                .. usage.Examples.SelectMany(static e => new[] { " " + e.Command, "  " + e.Description }),
            ],
            automation.Messages);
    }

    [Fact]
    public void ADelayWithoutACommandPrintsItsUsage()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "delay 1000");

        Assert.Equal(
            [
                "[UB] Error: Bad command syntax",
                "[UB] Usage: /ub delay <millisecondDelay> <command>",
            ],
            automation.Messages.Take(2));
    }

    // ── /ub closestportal, /ub portal ───────────────────────────────────

    [Fact]
    public void ClosestPortalUsesTheNearestPortal()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects =
            [
                Landscape(90u, "Far Portal", PluginObjectClass.Portal, 40d),
                Landscape(91u, "Near Portal", PluginObjectClass.Portal, 5d),
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "closestportal");

        Assert.Equal(91u, Assert.Single(automation.UsedItemIds));
        Assert.Contains("[UB] Attempting to use portal: Near Portal", automation.Messages);
    }

    [Fact]
    public void PortalByPartialNameUsesTheMatchingPortal()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects =
            [
                Landscape(92u, "Gateway", PluginObjectClass.Portal, 5d),
                Landscape(93u, "Holtburg Portal", PluginObjectClass.Portal, 40d),
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "portalp Holt");

        Assert.Equal(93u, Assert.Single(automation.UsedItemIds));
    }

    [Fact]
    public void APortalNobodyCanSeeIsReported()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "portal Gateway");

        Assert.Equal("[UB] Could not find a portal", Assert.Single(automation.Messages));
    }

    // ── /ub follow ──────────────────────────────────────────────────────

    [Fact]
    public void FollowByPartialNameNamesThePlayerItLockedOn()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects = [Player(0x50000ABCu, "Zero Cool")],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "followp Zero");

        Assert.Contains("[UB] Following Zero Cool[0x50000ABC]", automation.Messages);
    }

    /// <summary>
    /// A follow that finds nobody is the reference's error line, tag and all:
    /// the command profile waits on exactly <c>\[UB\] Error\: Could not find
    /// player</c> to tell the asker to try another character. Mutation: the
    /// bare line, or a plain line without "Error: ", never fires it.
    /// </summary>
    [Fact]
    public void FollowingNobodySaysWhoWasNotFound()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "follow Zero Cool");

        string line = Assert.Single(automation.Messages);
        Assert.Equal("[UB] Error: Could not find player Zero Cool", line);
        Assert.Matches(@"\[UB\] Error\: Could not find player", line);
    }

    /// <summary>
    /// An allegiance command that finds nobody names the verb as it was
    /// typed, flags included, after the name, as the reference's error does.
    /// </summary>
    [Fact]
    public void AnAllegianceCommandThatFindsNobodyNamesTheVerb()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "swearallegiancep Zero");

        Assert.Equal(
            "[UB] Error: Could not find player Zero Command:swearallegiancep",
            Assert.Single(automation.Messages));
    }

    // ── /ub use, /ub select, /ub close ──────────────────────────────────

    [Fact]
    public void UseOnItsOwnUsesTheNamedItem()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects = [Carried(100u, "Prismatic Taper")],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "use Prismatic Taper");

        Assert.Equal(100u, Assert.Single(automation.UsedItemIds));
        Assert.Contains("[UB] using Prismatic Taper", automation.Messages);
    }

    [Fact]
    public void UseOnASecondItemAppliesTheFirstToTheSecond()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects = [Carried(101u, "Splitting Tool"), Carried(102u, "Gold Pea")],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "usep splitting on gold pea");

        Assert.Equal((101u, 102u), Assert.Single(automation.Applied));
        Assert.Contains("[UB] using Splitting Tool on Gold Pea", automation.Messages);
    }

    [Fact]
    public void UseRefusesBothPlaceFlagsAtOnce()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "useli Cake");

        Assert.Equal(
            "[UB] l and i cannot be used in the same command",
            Assert.Single(automation.Messages));
    }

    /// <summary>
    /// The inventory flag really is a filter: an item of that name standing
    /// on the landscape must not answer a command that asked the packs.
    /// </summary>
    [Fact]
    public void TheInventoryFlagWillNotReachTheLandscape()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects = [Landscape(103u, "Cake", PluginObjectClass.Food, 3d)],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "usei Cake");

        Assert.Empty(automation.UsedItemIds);
        Assert.Contains("[UB] Error: Could not find object: Cake", automation.Messages);
    }

    [Fact]
    public void SelectPicksTheNamedItem()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects = [Carried(104u, "Gold Scarab")],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        UbCommand(panel, "selectp gold");

        Assert.Equal(104u, host.Selection.SelectedObjectId);
    }

    [Fact]
    public void CloseCorpseClosesTheOpenCorpse()
    {
        var loot = new UbFakeLoot();
        var automation = new FakeAutomation
        {
            LootSurface = loot,
            OpenContainerObjectId = 110u,
            WorldObjects =
            [
                new PluginWorldObject(
                    110u, 0u, "Corpse of a Drudge", PluginObjectClass.Corpse,
                    0u, 0u, 0u),
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "close corpse");

        Assert.Equal(110u, Assert.Single(loot.Closed));
    }

    [Fact]
    public void CloseCorpseWithNothingOpenSaysSo()
    {
        var loot = new UbFakeLoot();
        var automation = new FakeAutomation { LootSurface = loot };
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "close corpse");

        Assert.Equal(
            "[UB] No container is currently open.",
            Assert.Single(automation.Messages));
        Assert.Empty(loot.Closed);
    }

    // ── /ub swearallegiance, /ub breakallegiance ────────────────────────

    /// <summary>
    /// The client's answer to the two allegiance commands, so a test can say
    /// what came back and read which object the command named.
    /// </summary>
    private sealed class FakeAllegiance(PluginAllegianceCommandResult result)
        : IAllegianceAutomation
    {
        public bool IsAvailable => true;
        public List<uint> Sworn { get; } = [];
        public List<uint> Broken { get; } = [];

        public PluginAllegianceCommandResult Swear(uint patronObjectId)
        {
            Sworn.Add(patronObjectId);
            return result;
        }

        public PluginAllegianceCommandResult Break(uint targetObjectId)
        {
            Broken.Add(targetObjectId);
            return result;
        }
    }

    private static FakeAutomation AllegianceRig(
        PluginAllegianceCommandResult result,
        out FakeAllegiance allegiance)
    {
        allegiance = new FakeAllegiance(result);
        return new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            WorldObjects = [Player(0x50000AAAu, "Yonneh")],
            AllegianceSurface = allegiance,
        };
    }

    /// <summary>
    /// The command resolves who it means and hands that object to the client.
    /// Mutation: pass the selected object instead of the resolved one and the
    /// recorded id is wrong; drop the call and nothing is recorded at all.
    /// </summary>
    [Fact]
    public void SwearingAllegianceSendsTheResolvedPlayerToTheClient()
    {
        FakeAutomation automation = AllegianceRig(
            new PluginAllegianceCommandResult(PluginAllegianceCommandStatus.Sent),
            out FakeAllegiance allegiance);
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "swearallegiancep Yon");

        Assert.Equal(0x50000AAAu, Assert.Single(allegiance.Sworn));
        Assert.Equal(
            "[UB] Swearing Allegiance to Yonneh[0x50000AAA]",
            Assert.Single(automation.Messages));
    }

    [Fact]
    public void BreakingAllegianceSendsTheResolvedPlayerToTheClient()
    {
        FakeAutomation automation = AllegianceRig(
            new PluginAllegianceCommandResult(PluginAllegianceCommandStatus.Sent),
            out FakeAllegiance allegiance);
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "breakallegiance Yonneh");

        Assert.Equal(0x50000AAAu, Assert.Single(allegiance.Broken));
        Assert.Equal(
            "[UB] Breaking Allegiance from Yonneh[0x50000AAA]",
            Assert.Single(automation.Messages));
    }

    /// <summary>
    /// Each of the four answers reads differently, because a macro author
    /// watching chat has to tell "it went out" from "it never will".
    /// Mutation: fold any two statuses onto one line and its row fails.
    /// </summary>
    [Theory]
    [InlineData(
        PluginAllegianceCommandStatus.InvalidTarget,
        null,
        "Cannot swear allegiance to Yonneh[0x50000AAA].",
        "Yonneh[0x50000AAA] is not in your allegiance.")]
    [InlineData(
        PluginAllegianceCommandStatus.Refused,
        "You are already sworn to somebody.",
        "You are already sworn to somebody.",
        "You are already sworn to somebody.")]
    [InlineData(
        PluginAllegianceCommandStatus.Refused,
        null,
        "Refused to swear allegiance to Yonneh[0x50000AAA].",
        "Refused to break allegiance from Yonneh[0x50000AAA].")]
    [InlineData(
        PluginAllegianceCommandStatus.Unavailable,
        null,
        "Cannot swear allegiance right now.",
        "Cannot break allegiance right now.")]
    public void EveryAllegianceAnswerHasItsOwnLine(
        PluginAllegianceCommandStatus status,
        string? notice,
        string swearLine,
        string breakLine)
    {
        FakeAutomation automation = AllegianceRig(
            new PluginAllegianceCommandResult(status, notice),
            out _);
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "swearallegiance Yonneh");
        Assert.Equal("[UB] Error: " + swearLine, Assert.Single(automation.Messages));

        automation.Messages.Clear();
        UbCommand(panel, "breakallegiance Yonneh");
        Assert.Equal("[UB] Error: " + breakLine, Assert.Single(automation.Messages));
    }

    // ── /ub printcolors ─────────────────────────────────────────────────

    [Fact]
    public void PrintColorsWritesOneLineInEveryTextClass()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "printcolors");

        Assert.Equal(UbChatMessageTypes.All.Count, automation.Posted.Count);
        Assert.Equal(("[PrintColors]Broadcast (0)", 0), automation.Posted[0]);
        Assert.Contains(("[PrintColors]Magic (7)", 7), automation.Posted);
    }

    // ── /ub listvars, listpvars, listgvars ──────────────────────────────

    [Fact]
    public void ListVarsPrintsTheSessionStore()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "mexec setvar[myvar,1]");
        automation.Messages.Clear();
        UbCommand(panel, "listvars");

        Assert.Equal("[UB] Defined variables:", automation.Messages[0]);
        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith("myvar (", StringComparison.Ordinal));
    }

    /// <summary>
    /// Each variable's type is named as the reference names it: a number and
    /// a true/false are both "number" (the true/false written 1 or 0), a
    /// string "string", and an object by its type name. Mutation: printing
    /// the value kind prints "Number", "Boolean" or "String".
    /// </summary>
    [Fact]
    public void ListVarsNamesEachTypeAsTheReferenceDoes()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "mexec setvar[anumber,2.5]");
        Command(panel, "mexec setvar[astring,`hi`]");
        Command(panel, "mexec setvar[aflag,1==1]");
        Command(panel, "mexec setvar[alist,listcreate[1,2]]");
        automation.Messages.Clear();
        UbCommand(panel, "listvars");

        Assert.Equal(
            [
                "[UB] Defined variables:",
                "aflag (number) = 1",
                "alist (List) = [1,2]",
                "anumber (number) = 2.5",
                "astring (string) = hi",
            ],
            automation.Messages);
    }

    /// <summary>
    /// The three stores are separate: a session variable must not appear in
    /// the character's list, or a macro would read a value it never saved.
    /// </summary>
    [Fact]
    public void ListPvarsAndListGvarsReadTheirOwnStores()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        Command(panel, "mexec setvar[sessiononly,1]");
        Command(panel, "mexec setgvar[serverwide,2]");
        automation.Messages.Clear();
        UbCommand(panel, "listpvars");
        UbCommand(panel, "listgvars");

        Assert.Equal("[UB] Defined persistent variables:", automation.Messages[0]);
        Assert.DoesNotContain(
            automation.Messages,
            static line => line.StartsWith("sessiononly ", StringComparison.Ordinal));
        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith("serverwide (", StringComparison.Ordinal));
    }

    /// <summary>
    /// One damaged variable file is the reference's bad database row: the
    /// listing stops without printing a partial list, the command answers
    /// with the failure instead of throwing, and the variable still reads as
    /// an expression error. Mutation: letting the parser's error through
    /// prints the header first and names no variable file.
    /// </summary>
    [Fact]
    public void ListGvarsWithADamagedVariableFileFailsAsOneCommand()
    {
        var automation = new FakeAutomation();
        var storage = new MemoryStorage();
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Command(panel, "mexec setgvar[damaged,1]");
        string key = storage.Text.Keys.Single(static key =>
            key.StartsWith("expressions/global/", StringComparison.Ordinal));
        Command(panel, "mexec setgvar[fine,2]");
        storage.Text[key] = "{ not json";
        automation.Messages.Clear();

        UbCommand(panel, "listgvars");

        Assert.DoesNotContain("[UB] Defined global variables:", automation.Messages);
        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith("[UB] Error: Command failed: A global variable file", StringComparison.Ordinal));
    }

    // ── /vt addnavpt ────────────────────────────────────────────────────

    /// <summary>
    /// addnavpt finds the north/south and east/west pair anywhere in its
    /// argument and ignores the rest, the way the reference's coordinate
    /// search does: the "36.50S, 28.90E, 0.24Z" form a profile builds from
    /// getplayercoordinates, a pair without the comma, and a pair with a
    /// trailing comma left by cutting the height off all add the point, at
    /// ground height. Mutation: splitting on commas and requiring a bare
    /// number for the third part refuses the first line with the syntax
    /// message and adds no point.
    /// </summary>
    [Theory]
    [InlineData("addnavpt 41.20S, 44.00E, 0.41Z")]
    [InlineData("addnavpt 41.2S 44E")]
    [InlineData("addnavpt 41.20S, 44.00E,")]
    public void AddNavPointTakesTheCoordinatePairOutOfTheWholeArgument(string line)
    {
        var vtank = new MemoryStorage();
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(
            automation, new MemoryStorage(), vtankProfiles: vtank));

        Command(panel, line);
        Assert.Equal("Added navigation point.", automation.Messages[^1]);
        Command(panel, "navaf save pair-only");

        string saved = Assert.Single(
            vtank.Text,
            static pair => pair.Key.Contains("pair-only", StringComparison.Ordinal))
            .Value;
        Assert.Contains("pnt 44 -41.2 0", saved, StringComparison.Ordinal);
    }

    // ── /ub translateroute ──────────────────────────────────────────────

    [Fact]
    public void TranslateRouteShiftsEveryPointByTheLandblockDistance()
    {
        var vtank = new MemoryStorage();
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(
            automation, new MemoryStorage(), vtankProfiles: vtank));

        Command(panel, "addnavpt 10.0N, 20.0E");
        Command(panel, "nav save eo-east");
        automation.Messages.Clear();

        // One landblock step east is 192 m, which is 0.8 coordinates. The
        // east/west block is the top byte of the landblock id.
        UbCommand(panel, "translateroute 0x00640371 eo-east.nav 0x01640371 eo-main.nav");

        // Both routes are .nav files: a route saved by a bare name is one,
        // and the translation is written in the form its name asks for.
        KeyValuePair<string, string> saved = Assert.Single(
            vtank.Text,
            static pair => pair.Key.Contains("eo-main", StringComparison.Ordinal));
        Assert.Equal("mosstank/navs/eo-main.nav", saved.Key);
        Assert.Contains("\r\n20.8\r\n10\r\n0\r\n0\r\n", saved.Value, StringComparison.Ordinal);
        // A translation that worked is said only in the reference's debug
        // output, so with Plugin.Debug off nothing is printed.
        Assert.Empty(automation.Messages);
    }

    /// <summary>
    /// With Plugin.Debug on, a translation says what it was asked and what
    /// it did, as the reference's two debug lines (Abuse, 14). Mutation:
    /// printing them as ordinary lines, or with debug off, fails.
    /// </summary>
    [Fact]
    public void TranslateRouteReportsOnlyInTheDebugOutput()
    {
        var vtank = new MemoryStorage();
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(
            automation, new MemoryStorage(), vtankProfiles: vtank));
        Command(panel, "addnavpt 10.0N, 20.0E");
        Command(panel, "nav save eo-east");
        UbCommand(panel, "opt set Plugin.Debug true");
        automation.Posted.Clear();

        UbCommand(panel, "translateroute 0x00640371 eo-east.nav 0x01640371 eo-main.nav");

        Assert.Equal(2, automation.Posted.Count);
        Assert.All(automation.Posted, static line => Assert.Equal(UbChat.DebugChatType, line.Kind));
        Assert.Equal(
            "[UB] VTank: Translating route: RouteToLoad:eo-east.nav StartLandblock:0x00640371 "
            + "EndLandblock:0x01640371 RouteToSaveAs:eo-main.nav Force:False",
            automation.Posted[0].Text);
        Assert.StartsWith(
            "[UB] VTank: Translated 1 records from 00640371 to 01640371 by adding offsets NS:0 EW:0.8\nSaved to file: ",
            automation.Posted[1].Text,
            StringComparison.Ordinal);
    }

    [Fact]
    public void TranslateRouteRefusesToOverwriteWithoutTheForceFlag()
    {
        var vtank = new MemoryStorage();
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        };
        var panel = new MossTankPanel(new FakeHost(
            automation, new MemoryStorage(), vtankProfiles: vtank));

        Command(panel, "addnavpt 10.0N, 20.0E");
        Command(panel, "nav save eo-east");
        Command(panel, "nav save eo-main");
        automation.Messages.Clear();

        UbCommand(panel, "translateroute 0x00640371 eo-east.nav 0x00650371 eo-main.nav");

        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith(
                "[UB] Error: VTank: Output path already exists!", StringComparison.Ordinal));
    }

    [Fact]
    public void TranslateRouteRefusesALandblockThatIsNotHex()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        UbCommand(panel, "translateroute zzz a.nav 0x00650371 b.nav");

        Assert.Equal(
            "[UB] Error: VTank: Could not parse hex value from StartLandblock: zzz",
            Assert.Single(automation.Messages));
    }

    // ── fixtures ────────────────────────────────────────────────────────

    private static PluginWorldObject Player(uint objectId, string name) =>
        new(objectId, 0u, name, PluginObjectClass.Player, 0u, 0u, 0u)
        {
            HasPosition = true,
            IsLandscape = true,
            Position = new PluginNavigationPosition(
                0x00010001u, 0d, 0d, 0d, 0f, IsOutdoor: true),
        };

    private static PluginWorldObject Landscape(
        uint objectId,
        string name,
        PluginObjectClass objectClass,
        double eastWest) =>
        new(objectId, 0u, name, objectClass, 0u, 0u, 0u)
        {
            HasPosition = true,
            IsLandscape = true,
            Position = new PluginNavigationPosition(
                0x00010001u, eastWest, 0d, 0d, 0f, IsOutdoor: true),
        };

    /// <summary>A loot surface that only records the opens and closes.</summary>
    private sealed class UbFakeLoot : ILootAutomation
    {
        public List<uint> Opened { get; } = [];
        public List<uint> Closed { get; } = [];

        public bool IsAvailable => true;
        public bool IsBusy => false;
        public uint RequestedContainerId => 0u;
        public uint CurrentContainerId => 0u;
        public bool CurrentContentsReady => false;
        public PluginItemUseCompletion LastItemUseCompletion => default;
        public PluginInventoryCompletion LastInventoryCompletion => default;
        public PluginAppraisalState Appraisal => default;

        public IReadOnlyList<PluginLootContainer> CaptureCorpses(
            float maximumDistance) => [];

        public IReadOnlyList<PluginInventoryItem> CaptureCurrentContents() => [];

        public bool TryCaptureProperties(
            uint objectId,
            out PluginItemProperties properties)
        {
            properties = default;
            return false;
        }

        public PluginItemCommandResult Open(uint objectId)
        {
            Opened.Add(objectId);
            return new(PluginItemCommandStatus.Started);
        }

        public PluginItemCommandResult Close(uint objectId)
        {
            Closed.Add(objectId);
            return new(PluginItemCommandStatus.Started);
        }

        public PluginItemCommandResult Identify(uint objectId) =>
            new(PluginItemCommandStatus.Started);

        public PluginItemCommandResult Pickup(uint objectId, bool toMainPack) =>
            new(PluginItemCommandStatus.Started);
    }
}

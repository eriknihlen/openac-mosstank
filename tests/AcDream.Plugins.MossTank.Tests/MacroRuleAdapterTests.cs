using AcDream.Plugins.MossTank;

namespace AcDream.Plugins.MossTank.Tests;

public class MacroRuleAdapterTests
{
    [Fact]
    public void AbsentRuleIsNeverValidAndSwallowsRunning()
    {
        var rule = new AbsentMacroRule("Slot", "not ported");
        var context = new MacroPassContext(1d, CanAct: true);

        Assert.False(rule.ValidNow(in context));
        rule.Running = true;
        Assert.False(rule.Running);
        Assert.Equal("not ported", rule.Reason);
    }

    /// <summary>
    /// The gate is the rule's first refusal, as the reference's wrapper gates
    /// and opening lock checks are: a closed gate means the body is not
    /// consulted. Mutation: tick the controller with <c>CanAct: false</c>
    /// behind a closed gate and the call count fails.
    /// </summary>
    [Fact]
    public void GateBlockedRuleDoesNotTickItsController()
    {
        int calls = 0;
        var rule = new ControllerMacroRule(
            "rule",
            _ =>
            {
                calls++;
                return true;
            },
            gate: () => false);

        Assert.False(rule.ValidNow(new MacroPassContext(1d, CanAct: true)));
        Assert.Equal(0, calls);
        Assert.Equal("the rule's own gate is closed", rule.DeclineReason);
    }

    [Fact]
    public void ARuleAskedWithoutTheTurnIsNotTickedAndDoesNotClaim()
    {
        int calls = 0;
        var rule = new ControllerMacroRule(
            "rule",
            _ =>
            {
                calls++;
                return true;
            });

        Assert.False(rule.ValidNow(new MacroPassContext(1d, CanAct: false)));
        Assert.Equal(0, calls);
        Assert.True(rule.ValidNow(new MacroPassContext(1d, CanAct: true)));
        Assert.Equal(1, calls);
    }

    [Fact]
    public void TeardownRunsOnceOnTheTrueToFalseEdge()
    {
        int teardowns = 0;
        var rule = new ControllerMacroRule(
            "rule",
            _ => true,
            onLostTurn: () => teardowns++);

        rule.Running = false;
        Assert.Equal(0, teardowns);

        rule.Running = true;
        rule.Running = false;
        Assert.Equal(1, teardowns);

        rule.Running = false;
        Assert.Equal(1, teardowns);
    }
}

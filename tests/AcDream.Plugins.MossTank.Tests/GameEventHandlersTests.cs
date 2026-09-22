using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The handler lines on the settings page are a simplified editor over the
/// meta profile: one line is one rule in the default state whose condition
/// is the chosen edge, and the profile is the only copy.
/// </summary>
public sealed class GameEventHandlersTests
{
    /// <summary>
    /// The three action shapes round-trip: a chat command is the bare text,
    /// an expression and a chat expression carry a prefix.
    /// </summary>
    [Theory]
    [InlineData("LoginComplete = /framerate", "LoginComplete", "ChatCommand", "/framerate")]
    [InlineData("CharacterDeath = chatexpr: \\/f I died", "CharacterDeath", "ChatExpression", "\\/f I died")]
    [InlineData("Logoff = expr: setvar[`x`,1]", "Logoff", "ExpressionAction", "setvar[`x`,1]")]
    [InlineData("ContainerOpened = /ub opt", "ContainerOpened", "ChatCommand", "/ub opt")]
    public void LinesRoundTripThroughRules(
        string line,
        string condition,
        string action,
        string text)
    {
        Assert.True(GameEventHandlers.TryParse(line, out MetaRule rule, out string error), error);
        Assert.Equal(MetaEngine.DefaultState, rule.State);
        Assert.Equal(Enum.Parse<MetaConditionKind>(condition), rule.Condition.Kind);
        Assert.Equal(Enum.Parse<MetaActionKind>(action), rule.Action.Kind);
        Assert.Equal(text, rule.Action.Text);
        Assert.Equal(line, GameEventHandlers.Format(rule));
    }

    /// <summary>
    /// The reference's own event names are accepted: its pre-in-world Login
    /// has no counterpart on this client and lands on LoginComplete, and its
    /// Logout is this client's Logoff.
    /// </summary>
    [Theory]
    [InlineData("Login = /x", "LoginComplete")]
    [InlineData("logout = /x", "Logoff")]
    [InlineData("death = /x", "CharacterDeath")]
    [InlineData("portaltransition = /x", "PortalTransition")]
    [InlineData("ItemUseCompleted = /x", "ItemUseCompleted")]
    [InlineData("ConfirmationRequested = /x", "ConfirmationRequested")]
    public void ReferenceEventNamesAreAccepted(string line, string expected)
    {
        Assert.True(GameEventHandlers.TryParse(line, out MetaRule rule, out _));
        Assert.Equal(Enum.Parse<MetaConditionKind>(expected), rule.Condition.Kind);
    }

    [Theory]
    [InlineData("")]
    [InlineData("no separator")]
    [InlineData("NotAnEvent = /x")]
    [InlineData("LoginComplete =")]
    [InlineData("LoginComplete = expr: setvar[")]
    public void ABadLineIsRejectedWithAReason(string line)
    {
        Assert.False(GameEventHandlers.TryParse(line, out _, out string error));
        Assert.NotEmpty(error);
    }

    /// <summary>
    /// Applying a list replaces the handler rules and only those: a rule in
    /// another state, a rule with a different action shape, and a handler
    /// somebody disabled on the meta tab all survive. Mutation: clearing the
    /// whole profile before appending loses the user's meta.
    /// </summary>
    [Fact]
    public void ApplyReplacesHandlerRulesAndLeavesTheRestAlone()
    {
        var keepState = new MetaRule
        {
            State = "Hunting",
            Condition = new MetaCondition { Kind = MetaConditionKind.LoginComplete },
            Action = new MetaAction { Kind = MetaActionKind.ChatCommand, Text = "/a" },
        };
        var keepShape = new MetaRule
        {
            Condition = new MetaCondition { Kind = MetaConditionKind.LoginComplete },
            Action = new MetaAction { Kind = MetaActionKind.SetMetaState, Text = "Next" },
        };
        var keepDisabled = new MetaRule
        {
            Enabled = false,
            Condition = new MetaCondition { Kind = MetaConditionKind.Logoff },
            Action = new MetaAction { Kind = MetaActionKind.ChatCommand, Text = "/b" },
        };
        var replaced = new MetaRule
        {
            Condition = new MetaCondition { Kind = MetaConditionKind.CharacterDeath },
            Action = new MetaAction { Kind = MetaActionKind.ChatCommand, Text = "/old" },
        };
        var profile = new MetaProfile { Rules = [keepState, replaced, keepShape, keepDisabled] };

        Assert.Equal(["CharacterDeath = /old"], GameEventHandlers.Lines(profile));

        IReadOnlyList<string> rejected = GameEventHandlers.Apply(
            profile,
            ["LoginComplete = /framerate", "garbage", "Logoff = /bye"]);

        Assert.Single(rejected);
        Assert.Contains("garbage", rejected[0], StringComparison.Ordinal);
        Assert.Equal(["LoginComplete = /framerate", "Logoff = /bye"], GameEventHandlers.Lines(profile));
        Assert.Contains(keepState, profile.Rules);
        Assert.Contains(keepShape, profile.Rules);
        Assert.Contains(keepDisabled, profile.Rules);
        Assert.DoesNotContain(replaced, profile.Rules);
    }

    /// <summary>
    /// The new edges survive the meta profile's own text format, so a
    /// handler written on the settings page is still there after the
    /// profile is saved and reopened.
    /// </summary>
    [Fact]
    public void EventEdgesRoundTripThroughTheProfileFormat()
    {
        var profile = new MetaProfile();
        GameEventHandlers.Apply(profile,
        [
            "LoginComplete = /a",
            "Logoff = /b",
            "PortalTransition = /c",
            "ItemUseCompleted = /d",
            "ContainerOpened = /e",
            "ContainerClosed = /f",
            "ConfirmationRequested = /g",
        ]);
        string text = MetafSerializer.SaveMeta(profile);
        Assert.True(MetafSerializer.TryLoadMeta(
            text, NoOpAutomationSurface.Instance, out MetaProfile reloaded, out string error), error);
        Assert.Equal(GameEventHandlers.Lines(profile), GameEventHandlers.Lines(reloaded));
    }
}

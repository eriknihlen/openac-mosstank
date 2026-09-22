using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The settings page's event handlers as a simplified editor over the meta
/// profile. There is no handler table of its own: a handler line is one
/// rule in the default meta state whose condition is the chosen edge and
/// whose action is a chat command, a chat expression or an expression, so
/// a handler written on the settings page is visible on the meta tab, runs
/// through the one engine, and is saved with the meta profile.
/// </summary>
/// <remarks>
/// A line reads <c>event = action</c>. The action is a chat command as
/// written; <c>expr:</c> in front makes it an expression, <c>chatexpr:</c>
/// a chat expression. The reference tool's own event names are accepted
/// beside this client's: its pre-in-world <c>Login</c> has no counterpart
/// here and lands on the completed login, and its <c>Logout</c> is the
/// client's logoff.
/// </remarks>
internal static class GameEventHandlers
{
    private const string ExpressionPrefix = "expr:";
    private const string ChatExpressionPrefix = "chatexpr:";

    private static readonly (string Name, MetaConditionKind Kind)[] EventNames =
    [
        ("LoginComplete", MetaConditionKind.LoginComplete),
        ("Logoff", MetaConditionKind.Logoff),
        ("CharacterDeath", MetaConditionKind.CharacterDeath),
        ("PortalTransition", MetaConditionKind.PortalTransition),
        ("ItemUseCompleted", MetaConditionKind.ItemUseCompleted),
        ("ContainerOpened", MetaConditionKind.ContainerOpened),
        ("ContainerClosed", MetaConditionKind.ContainerClosed),
        ("ConfirmationRequested", MetaConditionKind.ConfirmationRequested),
    ];

    private static readonly Dictionary<string, MetaConditionKind> Aliases =
        new(StringComparer.OrdinalIgnoreCase)
        {
            ["Login"] = MetaConditionKind.LoginComplete,
            ["Logout"] = MetaConditionKind.Logoff,
            ["Death"] = MetaConditionKind.CharacterDeath,
        };

    /// <summary>
    /// Whether a condition kind is one of the client's notifications. The
    /// engine re-arms a rule on one of these after every pass; character
    /// death is not among them because a death rule keeps its once-per-state
    /// behaviour for the profiles that already rely on it.
    /// </summary>
    public static bool IsEventEdge(MetaConditionKind kind) => kind is
        MetaConditionKind.LoginComplete
        or MetaConditionKind.Logoff
        or MetaConditionKind.PortalTransition
        or MetaConditionKind.ItemUseCompleted
        or MetaConditionKind.ContainerOpened
        or MetaConditionKind.ContainerClosed
        or MetaConditionKind.ConfirmationRequested;

    /// <summary>
    /// Whether a rule is one the settings page can show as a handler line:
    /// enabled, in the default state, conditioned on an event, and acting
    /// as one of the three shapes a line can spell.
    /// </summary>
    public static bool IsHandler(MetaRule rule)
    {
        bool defaultState = string.IsNullOrWhiteSpace(rule.State)
            || rule.State.Trim().Equals(MetaEngine.DefaultState, StringComparison.OrdinalIgnoreCase);
        bool eventCondition = IsEventEdge(rule.Condition.Kind)
            || rule.Condition.Kind == MetaConditionKind.CharacterDeath;
        bool lineShapedAction = rule.Action.Kind is MetaActionKind.ChatCommand
            or MetaActionKind.ChatExpression
            or MetaActionKind.ExpressionAction;
        return rule.Enabled
            && defaultState
            && eventCondition
            && rule.Condition.Children.Count == 0
            && lineShapedAction
            && rule.Action.Children.Count == 0;
    }

    /// <summary>The handler rules of a profile as lines, in profile order.</summary>
    public static IReadOnlyList<string> Lines(MetaProfile profile) =>
        profile.Rules.Where(IsHandler).Select(Format).ToArray();

    /// <summary>
    /// Replaces the profile's handler rules with the ones the lines spell,
    /// leaving every other rule where it is. Returns one sentence per line
    /// that could not be read; those lines are dropped, not kept.
    /// </summary>
    public static IReadOnlyList<string> Apply(MetaProfile profile, IReadOnlyList<string> lines)
    {
        var rejected = new List<string>();
        var parsed = new List<MetaRule>();
        foreach (string line in lines)
        {
            if (TryParse(line, out MetaRule rule, out string error))
                parsed.Add(rule);
            else
                rejected.Add($"Event handler '{line}' was not saved: {error}");
        }
        profile.Rules.RemoveAll(IsHandler);
        profile.Rules.AddRange(parsed);
        return rejected;
    }

    public static string Format(MetaRule rule)
    {
        string name = EventNames.First(entry => entry.Kind == rule.Condition.Kind).Name;
        string action = rule.Action.Kind switch
        {
            MetaActionKind.ExpressionAction => ExpressionPrefix + " " + rule.Action.Text,
            MetaActionKind.ChatExpression => ChatExpressionPrefix + " " + rule.Action.Text,
            _ => rule.Action.Text,
        };
        return $"{name} = {action}";
    }

    public static bool TryParse(string line, out MetaRule rule, out string error)
    {
        rule = null!;
        string text = (line ?? string.Empty).Trim();
        int separator = text.IndexOf('=');
        if (separator < 0)
        {
            error = "a handler reads 'event = action'.";
            return false;
        }
        string eventName = text[..separator].Trim();
        string actionText = text[(separator + 1)..].Trim();
        if (!TryEvent(eventName, out MetaConditionKind kind))
        {
            error = $"'{eventName}' is not an event; one of "
                + string.Join(", ", EventNames.Select(static entry => entry.Name)) + ".";
            return false;
        }
        if (actionText.Length == 0)
        {
            error = "the action is empty.";
            return false;
        }
        MetaActionKind actionKind = MetaActionKind.ChatCommand;
        if (actionText.StartsWith(ExpressionPrefix, StringComparison.OrdinalIgnoreCase))
        {
            actionKind = MetaActionKind.ExpressionAction;
            actionText = actionText[ExpressionPrefix.Length..].Trim();
        }
        else if (actionText.StartsWith(ChatExpressionPrefix, StringComparison.OrdinalIgnoreCase))
        {
            actionKind = MetaActionKind.ChatExpression;
            actionText = actionText[ChatExpressionPrefix.Length..].Trim();
        }
        if (actionKind != MetaActionKind.ChatCommand)
        {
            try
            {
                _ = ExpressionProgram.Compile(actionText);
            }
            catch (Exception failure) when (failure is FormatException or ExpressionParseException)
            {
                error = $"the expression does not compile: {failure.Message}";
                return false;
            }
            if (actionText.Length == 0)
            {
                error = "the expression is empty.";
                return false;
            }
        }
        rule = new MetaRule
        {
            State = MetaEngine.DefaultState,
            Condition = new MetaCondition { Kind = kind },
            Action = new MetaAction { Kind = actionKind, Text = actionText },
        };
        error = string.Empty;
        return true;
    }

    private static bool TryEvent(string name, out MetaConditionKind kind)
    {
        foreach ((string candidate, MetaConditionKind candidateKind) in EventNames)
        {
            if (candidate.Equals(name, StringComparison.OrdinalIgnoreCase))
            {
                kind = candidateKind;
                return true;
            }
        }
        return Aliases.TryGetValue(name, out kind);
    }
}

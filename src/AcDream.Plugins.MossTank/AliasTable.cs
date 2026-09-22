using System.Diagnostics.CodeAnalysis;
using System.Text.RegularExpressions;

namespace AcDream.Plugins.MossTank;

/// <summary>What an alias does once its pattern matches a typed line.</summary>
internal enum AliasActionKind
{
    /// <summary>The text is sent through the chat bar as it stands.</summary>
    ChatCommand,

    /// <summary>The text is evaluated as an expression and the result is sent.</summary>
    ChatExpression,

    /// <summary>The text is evaluated as an expression and the result is dropped.</summary>
    Expression,
}

/// <summary>What the table decided for one typed line.</summary>
internal enum AliasOutcome
{
    /// <summary>The line goes out as typed.</summary>
    Pass,

    /// <summary>The line is replaced by <see cref="AliasResolution.Rewrite"/>.</summary>
    Rewrite,

    /// <summary>The line is swallowed.</summary>
    Suppress,
}

/// <summary>
/// The two things an alias action needs from the plugin: somewhere to put
/// what the pattern captured, and something to evaluate an expression.
/// </summary>
internal interface IAliasActionHost
{
    /// <summary>
    /// Stores one capture group as a session variable; null clears it,
    /// for a group the pattern has but this line did not fill.
    /// </summary>
    void SetCapture(string name, string? value);

    /// <summary>Evaluates one expression and gives its text.</summary>
    string Evaluate(string expression);
}

/// <summary>One alias: a pattern, what to do when it matches, and whether the typed line survives.</summary>
internal sealed class Alias
{
    /// <summary>A pattern that runs away is cut off rather than freezing the chat bar.</summary>
    private static readonly TimeSpan MatchTimeout = TimeSpan.FromMilliseconds(100);

    internal Alias(string pattern, AliasActionKind kind, string text, bool eat)
    {
        Pattern = pattern;
        Kind = kind;
        Text = text;
        Eat = eat;
        Regex = new Regex(
            pattern,
            RegexOptions.IgnoreCase | RegexOptions.CultureInvariant,
            MatchTimeout);
    }

    /// <summary>The regular expression, as written.</summary>
    internal string Pattern { get; }

    /// <summary>What happens on a match.</summary>
    internal AliasActionKind Kind { get; }

    /// <summary>The command or the expression, as written.</summary>
    internal string Text { get; }

    /// <summary>Whether the typed line is swallowed on a match.</summary>
    internal bool Eat { get; }

    /// <summary>The compiled pattern; case does not matter.</summary>
    internal Regex Regex { get; }

    /// <summary>The line this alias is written as; parsing it gives this alias back.</summary>
    internal string ToLine() =>
        (Eat ? AliasTable.EatFlag + " " : string.Empty)
        + Pattern
        + AliasTable.SeparatorOf(Kind)
        + Text;

    public override string ToString() => ToLine();
}

/// <summary>What the table decided for one typed line, and what has to go out for it.</summary>
internal sealed class AliasResolution
{
    internal static readonly AliasResolution Passed = new(AliasOutcome.Pass, null, [], []);

    internal AliasResolution(
        AliasOutcome outcome,
        string? rewrite,
        IReadOnlyList<string> submit,
        IReadOnlyList<string> problems)
    {
        Outcome = outcome;
        Rewrite = rewrite;
        Submit = submit;
        Problems = problems;
    }

    /// <summary>What becomes of the typed line.</summary>
    internal AliasOutcome Outcome { get; }

    /// <summary>The line that goes out in place of the typed one, when <see cref="Outcome"/> says so.</summary>
    internal string? Rewrite { get; }

    /// <summary>Lines that go out on their own, after the typed line's fate is settled.</summary>
    internal IReadOnlyList<string> Submit { get; }

    /// <summary>An action that failed, one line each, for whoever wrote the alias.</summary>
    internal IReadOnlyList<string> Problems { get; }
}

/// <summary>
/// The alias list, read from and written to the list setting one line per
/// alias. Every typed line is held against every alias; each one that
/// matches stores its named groups as session variables called
/// <c>capturegroup_&lt;name&gt;</c> and runs its action.
/// </summary>
/// <remarks>
/// <para>The line format is <c>[eat] pattern SEPARATOR action</c>:</para>
/// <list type="bullet">
/// <item><c>pattern = text</c> sends the text through the chat bar.</item>
/// <item><c>pattern =&gt; expression</c> evaluates the expression and sends the result.</item>
/// <item><c>pattern -&gt; expression</c> evaluates the expression and sends nothing.</item>
/// </list>
/// <para>
/// The separators are spaced, so a pattern may hold <c>=</c> or <c>&gt;</c>
/// on its own (a lookahead does), and the earliest one on the line splits
/// it, so the action may hold any of them. A leading <c>[eat]</c> swallows
/// the typed line. Without it, an alias that has something to send sends it
/// IN PLACE of the typed line: that is what lets a marker in the middle of
/// a sentence be replaced before the sentence goes out. With it, the typed
/// line goes nowhere and what the aliases say goes out on its own.
/// </para>
/// </remarks>
internal sealed class AliasTable
{
    /// <summary>The flag that swallows the typed line, at the start of the line.</summary>
    internal const string EatFlag = "[eat]";

    private const string ChatCommandSeparator = " = ";
    private const string ChatExpressionSeparator = " => ";
    private const string ExpressionSeparator = " -> ";

    /// <summary>The name a whole-line capture is stored under; the first group is the match.</summary>
    private const string CapturePrefix = "capturegroup_";

    /// <summary>A table with nothing in it, which passes every line.</summary>
    internal static readonly AliasTable Empty = new([], []);

    private AliasTable(IReadOnlyList<Alias> aliases, IReadOnlyList<string> problems)
    {
        Aliases = aliases;
        Problems = problems;
    }

    /// <summary>The aliases that parsed, in list order.</summary>
    internal IReadOnlyList<Alias> Aliases { get; }

    /// <summary>The lines that did not parse, numbered from one, with why.</summary>
    internal IReadOnlyList<string> Problems { get; }

    /// <summary>
    /// Reads the list setting. A line that does not parse is reported by
    /// its number and left out; the lines around it still count.
    /// </summary>
    internal static AliasTable Parse(IEnumerable<string> lines)
    {
        ArgumentNullException.ThrowIfNull(lines);
        var aliases = new List<Alias>();
        var problems = new List<string>();
        int number = 0;
        foreach (string line in lines)
        {
            number++;
            if (TryParseLine(line, out Alias? alias, out string? problem))
                aliases.Add(alias);
            else
                problems.Add($"Line {number}: {problem}");
        }
        return aliases.Count == 0 && problems.Count == 0
            ? Empty
            : new AliasTable(aliases, problems);
    }

    /// <summary>Reads one line of the list; false with a reason when it is not an alias.</summary>
    internal static bool TryParseLine(
        string? line,
        [NotNullWhen(true)] out Alias? alias,
        [NotNullWhen(false)] out string? problem)
    {
        alias = null;
        string text = (line ?? string.Empty).Trim();
        bool eat = false;
        if (text.StartsWith(EatFlag, StringComparison.OrdinalIgnoreCase))
        {
            eat = true;
            text = text[EatFlag.Length..].TrimStart();
        }

        int at = -1;
        AliasActionKind kind = AliasActionKind.ChatCommand;
        string separator = string.Empty;
        foreach ((string candidate, AliasActionKind candidateKind) in new[]
        {
            (ChatCommandSeparator, AliasActionKind.ChatCommand),
            (ChatExpressionSeparator, AliasActionKind.ChatExpression),
            (ExpressionSeparator, AliasActionKind.Expression),
        })
        {
            int index = text.IndexOf(candidate, StringComparison.Ordinal);
            if (index >= 0 && (at < 0 || index < at))
            {
                at = index;
                kind = candidateKind;
                separator = candidate;
            }
        }
        if (at < 0)
        {
            // The trailing-separator case reads as a pattern with nothing
            // after it, which is the same complaint.
            problem = "An alias is 'pattern = command', 'pattern => expression' "
                + "or 'pattern -> expression', with the spaces.";
            return false;
        }

        string pattern = text[..at].Trim();
        string action = text[(at + separator.Length)..].Trim();
        if (pattern.Length == 0)
        {
            problem = "The pattern before the separator is empty.";
            return false;
        }
        if (action.Length == 0)
        {
            problem = "There is nothing after the separator to run.";
            return false;
        }
        try
        {
            alias = new Alias(pattern, kind, action, eat);
        }
        catch (ArgumentException error)
        {
            problem = $"'{pattern}' is not a regular expression: {error.Message}";
            return false;
        }
        problem = null;
        return true;
    }

    /// <summary>The list setting's lines for these aliases.</summary>
    internal static string SeparatorOf(AliasActionKind kind) => kind switch
    {
        AliasActionKind.ChatCommand => ChatCommandSeparator,
        AliasActionKind.ChatExpression => ChatExpressionSeparator,
        AliasActionKind.Expression => ExpressionSeparator,
        _ => throw new ArgumentOutOfRangeException(nameof(kind), kind, null),
    };

    /// <summary>
    /// Holds one typed line against every alias, in list order. Each match
    /// stores its groups first and runs its action second, because the
    /// action reads them. Then the line's fate: one eater swallows it for
    /// all of them; otherwise the first alias with something to send
    /// replaces it and the rest go out after; otherwise it passes.
    /// </summary>
    internal AliasResolution Resolve(string line, IAliasActionHost host)
    {
        ArgumentNullException.ThrowIfNull(line);
        ArgumentNullException.ThrowIfNull(host);
        if (Aliases.Count == 0)
            return AliasResolution.Passed;

        bool eat = false;
        List<string>? outputs = null;
        List<string>? problems = null;
        foreach (Alias alias in Aliases)
        {
            Match match;
            try
            {
                match = alias.Regex.Match(line);
            }
            catch (RegexMatchTimeoutException)
            {
                (problems ??= []).Add($"Alias '{alias.Pattern}' took too long to match and was skipped.");
                continue;
            }
            if (!match.Success)
                continue;

            eat |= alias.Eat;
            foreach (string name in alias.Regex.GetGroupNames())
            {
                Group group = match.Groups[name];
                host.SetCapture(CapturePrefix + name, group.Success ? group.Value : null);
            }

            string? output;
            try
            {
                output = alias.Kind switch
                {
                    AliasActionKind.ChatCommand => alias.Text,
                    AliasActionKind.ChatExpression => host.Evaluate(alias.Text),
                    _ => Discard(host.Evaluate(alias.Text)),
                };
            }
            catch (Exception error) when (error is not OutOfMemoryException)
            {
                (problems ??= []).Add(
                    $"Alias '{alias.Pattern}' failed to run '{alias.Text}': {error.Message}");
                continue;
            }
            if (!string.IsNullOrWhiteSpace(output))
                (outputs ??= []).Add(output);
        }

        IReadOnlyList<string> reported = problems ?? [];
        if (eat)
            return new AliasResolution(AliasOutcome.Suppress, null, outputs ?? [], reported);
        if (outputs is { Count: > 0 })
            return new AliasResolution(AliasOutcome.Rewrite, outputs[0], outputs[1..], reported);
        return reported.Count == 0
            ? AliasResolution.Passed
            : new AliasResolution(AliasOutcome.Pass, null, [], reported);
    }

    private static string? Discard(string ignored) => null;
}

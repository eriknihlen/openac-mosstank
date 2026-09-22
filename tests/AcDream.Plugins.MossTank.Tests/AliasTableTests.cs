namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The alias list: one line per alias, matched against every line the
/// player types, deciding whether the line passes, is rewritten, or is
/// swallowed, and which actions run for it.
/// </summary>
public sealed class AliasTableTests
{
    /// <summary>
    /// A host whose expressions are a lookup table, so the table's decision
    /// can be checked without an expression engine behind it.
    /// </summary>
    private sealed class ScriptedActions : IAliasActionHost
    {
        public Dictionary<string, string> Results { get; } = new(StringComparer.Ordinal);
        public List<string> Evaluated { get; } = [];
        public Dictionary<string, string?> Captures { get; } = new(StringComparer.Ordinal);
        public List<string> Order { get; } = [];

        public void SetCapture(string name, string? value)
        {
            Captures[name] = value;
            Order.Add("capture " + name);
        }

        public string Evaluate(string expression)
        {
            Evaluated.Add(expression);
            Order.Add("evaluate " + expression);
            return Results.TryGetValue(expression, out string? result) ? result : string.Empty;
        }
    }

    // ---- The line format --------------------------------------------------

    [Theory]
    [InlineData("^/ubpd$ = /ub propertydump", "^/ubpd$", AliasActionKind.ChatCommand, "/ub propertydump", false)]
    [InlineData("^/lsr$ -> actiontrycastbyid[1635]", "^/lsr$", AliasActionKind.Expression, "actiontrycastbyid[1635]", false)]
    [InlineData("^/tloc (?<name>.*)$ => \\/tell +getvar[capturegroup_name]", "^/tloc (?<name>.*)$", AliasActionKind.ChatExpression, "\\/tell +getvar[capturegroup_name]", false)]
    [InlineData("[eat] ^/ubpd$ = /ub propertydump", "^/ubpd$", AliasActionKind.ChatCommand, "/ub propertydump", true)]
    [InlineData("  [eat]   ^/x$   =   /y  ", "^/x$", AliasActionKind.ChatCommand, "/y", true)]
    internal void ALineIsPatternSeparatorAction(
        string line,
        string pattern,
        AliasActionKind kind,
        string text,
        bool eat)
    {
        Assert.True(AliasTable.TryParseLine(line, out Alias? alias, out string? problem), problem);
        Assert.Equal(pattern, alias!.Pattern);
        Assert.Equal(kind, alias.Kind);
        Assert.Equal(text, alias.Text);
        Assert.Equal(eat, alias.Eat);
    }

    /// <summary>
    /// The earliest separator wins, so a pattern may hold the characters
    /// of a later one and an action may hold any of them.
    /// </summary>
    [Fact]
    public void TheEarliestSeparatorSplitsTheLine()
    {
        Assert.True(AliasTable.TryParseLine("^a$ = b -> c => d", out Alias? alias, out _));
        Assert.Equal("^a$", alias!.Pattern);
        Assert.Equal(AliasActionKind.ChatCommand, alias.Kind);
        Assert.Equal("b -> c => d", alias.Text);

        Assert.True(AliasTable.TryParseLine("^(?=a)b$ -> x = y", out alias, out _));
        Assert.Equal("^(?=a)b$", alias!.Pattern);
        Assert.Equal(AliasActionKind.Expression, alias.Kind);
        Assert.Equal("x = y", alias.Text);
    }

    [Theory]
    [InlineData("just words")]
    [InlineData("^/x$ =")]
    [InlineData(" = /y")]
    [InlineData("[eat] = /y")]
    [InlineData("^(/x$ = /y")]
    [InlineData("")]
    public void AMalformedLineIsRefusedWithAReason(string line)
    {
        Assert.False(AliasTable.TryParseLine(line, out Alias? alias, out string? problem));
        Assert.Null(alias);
        Assert.False(string.IsNullOrWhiteSpace(problem));
    }

    [Fact]
    public void ABadRegexSaysWhatIsWrongWithIt()
    {
        Assert.False(AliasTable.TryParseLine("^(/x$ = /y", out _, out string? problem));
        Assert.Contains("^(/x$", problem, StringComparison.Ordinal);
    }

    /// <summary>
    /// What is written back is what was read, so opening the list and
    /// closing it again changes nothing on disk. Mutation: drop the eat
    /// flag or the separator from ToLine and every round trip fails.
    /// </summary>
    [Theory]
    [InlineData("^/ubpd$ = /ub propertydump")]
    [InlineData("[eat] ^/lsr$ -> actiontrycastbyid[1635]")]
    [InlineData("^(?<start>.*)(?<loc>\\$LOC)(?<end>.*)$ => $capturegroup_start + getplayercoordinates[] + $capturegroup_end")]
    public void ALineRoundTrips(string line)
    {
        Assert.True(AliasTable.TryParseLine(line, out Alias? alias, out _));
        Assert.Equal(line, alias!.ToLine());
    }

    /// <summary>
    /// A malformed line is kept out of the table and reported by number,
    /// never dropped without a word; the good lines around it still work.
    /// </summary>
    [Fact]
    public void ParsingReportsEveryBadLineAndKeepsTheGoodOnes()
    {
        var table = AliasTable.Parse(
            ["^/a$ = /b", "nonsense", "^(/c$ = /d", "^/e$ = /f"]);

        Assert.Equal(2, table.Aliases.Count);
        Assert.Equal(["^/a$", "^/e$"], table.Aliases.Select(alias => alias.Pattern));
        Assert.Equal(2, table.Problems.Count);
        Assert.StartsWith("Line 2:", table.Problems[0], StringComparison.Ordinal);
        Assert.StartsWith("Line 3:", table.Problems[1], StringComparison.Ordinal);
    }

    // ---- Matching -----------------------------------------------------------

    [Fact]
    public void ALineNoAliasMatchesPasses()
    {
        var table = AliasTable.Parse(["^/ubpd$ = /ub propertydump"]);
        AliasResolution result = table.Resolve("/ub propertydump", new ScriptedActions());
        Assert.Equal(AliasOutcome.Pass, result.Outcome);
        Assert.Null(result.Rewrite);
        Assert.Empty(result.Submit);
    }

    /// <summary>
    /// A chat command that does not eat REPLACES the typed line: the
    /// player typed the alias, and what goes out is what it stands for.
    /// Mutation: submit the command beside the original and both go out.
    /// </summary>
    [Fact]
    public void AChatCommandThatDoesNotEatRewritesTheLine()
    {
        var table = AliasTable.Parse(["^/ubpd$ = /ub propertydump"]);
        AliasResolution result = table.Resolve("/ubpd", new ScriptedActions());
        Assert.Equal(AliasOutcome.Rewrite, result.Outcome);
        Assert.Equal("/ub propertydump", result.Rewrite);
        Assert.Empty(result.Submit);
    }

    [Fact]
    public void MatchingIgnoresCase()
    {
        var table = AliasTable.Parse(["^/UBPD$ = /ub propertydump"]);
        Assert.Equal(AliasOutcome.Rewrite, table.Resolve("/ubpd", new ScriptedActions()).Outcome);
    }

    /// <summary>
    /// An eaten line is swallowed, and what the action says goes out on
    /// its own rather than in the typed line's place.
    /// </summary>
    [Fact]
    public void AnEatingAliasSuppressesTheLineAndSubmitsItsOutput()
    {
        var table = AliasTable.Parse(["[eat] ^/ubpd$ = /ub propertydump"]);
        AliasResolution result = table.Resolve("/ubpd", new ScriptedActions());
        Assert.Equal(AliasOutcome.Suppress, result.Outcome);
        Assert.Null(result.Rewrite);
        Assert.Equal(["/ub propertydump"], result.Submit);
    }

    /// <summary>
    /// A plain expression runs and says nothing, so the typed line is
    /// neither replaced nor swallowed unless the alias eats it.
    /// </summary>
    [Fact]
    public void APlainExpressionRunsAndTheLinePassesUnlessEaten()
    {
        var actions = new ScriptedActions();
        var table = AliasTable.Parse(["^/lsr$ -> actiontrycastbyid[1635]"]);
        AliasResolution result = table.Resolve("/lsr", actions);
        Assert.Equal(AliasOutcome.Pass, result.Outcome);
        Assert.Equal(["actiontrycastbyid[1635]"], actions.Evaluated);

        table = AliasTable.Parse(["[eat] ^/lsr$ -> actiontrycastbyid[1635]"]);
        result = table.Resolve("/lsr", actions);
        Assert.Equal(AliasOutcome.Suppress, result.Outcome);
        Assert.Empty(result.Submit);
    }

    /// <summary>
    /// The documented case: a marker inside a line the player is typing is
    /// rewritten into their coordinates before the line is sent. The named
    /// groups are stored before the expression is evaluated, because the
    /// expression reads them.
    /// </summary>
    [Fact]
    public void AChatExpressionRewritesTheLineFromItsCaptureGroups()
    {
        const string expression = "$capturegroup_start + getplayercoordinates[] + $capturegroup_end";
        var actions = new ScriptedActions();
        actions.Results[expression] = "meet me at 12.3N, 45.6E please";
        var table = AliasTable.Parse(
            [@"^(?<start>.*)(?<loc>\$LOC)(?<end>.*)$ => " + expression]);

        AliasResolution result = table.Resolve("meet me at $LOC please", actions);

        Assert.Equal(AliasOutcome.Rewrite, result.Outcome);
        Assert.Equal("meet me at 12.3N, 45.6E please", result.Rewrite);
        Assert.Equal("meet me at ", actions.Captures["capturegroup_start"]);
        Assert.Equal("$LOC", actions.Captures["capturegroup_loc"]);
        Assert.Equal(" please", actions.Captures["capturegroup_end"]);
        Assert.Equal("meet me at $LOC please", actions.Captures["capturegroup_0"]);
        Assert.Equal("evaluate " + expression, actions.Order[^1]);
        Assert.All(actions.Order[..^1], step => Assert.StartsWith("capture ", step, StringComparison.Ordinal));
    }

    /// <summary>A chat expression that comes out blank sends nothing.</summary>
    [Fact]
    public void ABlankChatExpressionResultSendsNothing()
    {
        var actions = new ScriptedActions();
        var table = AliasTable.Parse(["^/quiet$ => nothing[]"]);
        AliasResolution result = table.Resolve("/quiet", actions);
        Assert.Equal(AliasOutcome.Pass, result.Outcome);
        Assert.Empty(result.Submit);
    }

    /// <summary>A group the pattern has but the line did not fill is cleared.</summary>
    [Fact]
    public void AnUnfilledGroupIsCleared()
    {
        var actions = new ScriptedActions();
        var table = AliasTable.Parse(["^/go(?: (?<where>\\w+))?$ -> 1"]);
        table.Resolve("/go", actions);
        Assert.True(actions.Captures.ContainsKey("capturegroup_where"));
        Assert.Null(actions.Captures["capturegroup_where"]);
    }

    /// <summary>
    /// Every alias that matches fires, in list order. When more than one
    /// has something to send, the first replaces the typed line and the
    /// rest go out after it; one eater swallows the line for all of them.
    /// </summary>
    [Fact]
    public void EveryMatchingAliasFiresInOrder()
    {
        var actions = new ScriptedActions();
        var table = AliasTable.Parse(
        [
            "^/both$ = /first",
            "^/bo.*$ -> note[]",
            "^/both$ = /second",
        ]);

        AliasResolution result = table.Resolve("/both", actions);
        Assert.Equal(AliasOutcome.Rewrite, result.Outcome);
        Assert.Equal("/first", result.Rewrite);
        Assert.Equal(["/second"], result.Submit);
        Assert.Equal(["note[]"], actions.Evaluated);

        table = AliasTable.Parse(
        [
            "^/both$ = /first",
            "[eat] ^/both$ = /second",
        ]);
        result = table.Resolve("/both", actions);
        Assert.Equal(AliasOutcome.Suppress, result.Outcome);
        Assert.Equal(["/first", "/second"], result.Submit);
    }

    /// <summary>
    /// An action that throws is reported against its alias and the others
    /// still run; the typed line is not lost to it.
    /// </summary>
    [Fact]
    public void AFailingActionIsReportedAndTheRestStillRun()
    {
        var actions = new ThrowingActions();
        var table = AliasTable.Parse(
        [
            "^/x$ -> boom[]",
            "^/x$ = /after",
        ]);
        AliasResolution result = table.Resolve("/x", actions);
        Assert.Equal(AliasOutcome.Rewrite, result.Outcome);
        Assert.Equal("/after", result.Rewrite);
        Assert.Single(result.Problems);
        Assert.Contains("boom[]", result.Problems[0], StringComparison.Ordinal);
        Assert.Contains("no such function", result.Problems[0], StringComparison.Ordinal);
    }

    private sealed class ThrowingActions : IAliasActionHost
    {
        public void SetCapture(string name, string? value)
        {
        }

        public string Evaluate(string expression) =>
            throw new InvalidOperationException("no such function");
    }

    [Fact]
    public void AnEmptyTablePassesEverything()
    {
        AliasResolution result = AliasTable.Empty.Resolve("/anything", new ScriptedActions());
        Assert.Equal(AliasOutcome.Pass, result.Outcome);
    }
}

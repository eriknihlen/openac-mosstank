using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The conformance suite for the expression grammar a profile is written
/// against. It pins the whole operator precedence ladder, the atom forms and
/// the places where a nested expression starts over at the loosest level.
/// Anything that changes here changes the meaning of every profile, so each
/// case is a concrete expression with the value the grammar produces.
///
/// The ladder, loosest to tightest:
/// <list type="number">
/// <item><description><c>|| &amp;&amp;</c> (one level, left to right)</description></item>
/// <item><description><c>== != &lt; &lt;= &gt; &gt;=</c></description></item>
/// <item><description><c>=</c> (right to left)</description></item>
/// <item><description><c>#</c></description></item>
/// <item><description><c>+ -</c></description></item>
/// <item><description><c>* / %</c></description></item>
/// <item><description><c>&amp; ^ |</c> (one level, left to right)</description></item>
/// <item><description><c>&lt;&lt; &gt;&gt;</c></description></item>
/// <item><description><c>~</c> and a negative number literal</description></item>
/// <item><description><c>{index}</c> and <c>{slice}</c></description></item>
/// </list>
/// </summary>
public sealed class ExpressionGrammarConformanceTests
{
    /// <summary>
    /// The bitwise group binds tighter than multiplication, so the '&amp;'
    /// runs first and the product uses its result. Mutation: putting the
    /// bitwise level back under multiplication answers 2.
    /// </summary>
    [Fact]
    public void BitwiseAndBindsTighterThanMultiplication() =>
        Assert.Equal(4d, Evaluate("2 & 3 * 2").AsNumber());

    /// <summary>
    /// The shift operators bind tighter than addition. Mutation: putting the
    /// shift level above addition answers 32.
    /// </summary>
    [Fact]
    public void ShiftBindsTighterThanAddition() =>
        Assert.Equal(7d, Evaluate("1 << 2 + 3").AsNumber());

    /// <summary>
    /// '^' is an integer exclusive-or sitting in the bitwise group, not a
    /// loose logical operator and not a power. Mutation: moving '^' back to
    /// the logical level answers 4; making it a power answers 512.
    /// </summary>
    [Fact]
    public void CaretIsAnExclusiveOrInsideTheBitwiseGroup() =>
        Assert.Equal(2d, Evaluate("2 ^ 3 * 2").AsNumber());

    /// <summary>
    /// The characters an unquoted string may continue with include the space,
    /// so a trailing space belongs to the string; only the whitespace before
    /// a string starts is skipped, because the characters it may start with
    /// do not include the space. A name pattern written with a trailing space
    /// is a different pattern and has to stay one. A name is not a string
    /// though: a function name and a variable name are read without the
    /// spaces that separate them from what follows, so an expression can be
    /// written with spaces around its operators and still reach the same
    /// variable. Mutation: trim the string token and the first two rows lose
    /// their space; keep the space on a name and "$x = 1" stores under a
    /// different name than "$x + 1" reads.
    /// </summary>
    [Fact]
    public void AnUnquotedStringKeepsATrailingSpaceButANameDoesNot()
    {
        var state = new ExpressionState();

        Assert.Equal(10d, Evaluate("strlen[Blackfire ]").AsNumber());
        Assert.Equal(
            "Blackfire ",
            Evaluate("listcreate[Blackfire , Sword]{0}").AsString());

        Assert.Equal(9d, Evaluate("strlen[ Blackfire]").AsNumber());
        Assert.Equal(
            "Blackfire",
            Evaluate("listcreate[ Blackfire, Sword]{0}").AsString());

        Assert.Equal(2d, Evaluate("$x = 1;$x + 1", state).AsNumber());
        Assert.Equal(1d, state.Get(ExpressionVariableScope.Session, "x").AsNumber());
        Assert.Equal(9d, Evaluate("strlen [Blackfire]").AsNumber());
    }

    /// <summary>
    /// Every operator that works on bits converts its operands the same way:
    /// to the nearest whole number, not by throwing the fraction away. A
    /// profile that mixes them would otherwise see one operand two different
    /// ways inside a single expression. Mutation: truncate on any one row and
    /// it answers 7, or -8 for the complement.
    /// </summary>
    [Theory]
    [InlineData("7.9 & 15", 8d)]
    [InlineData("7.9 | 0", 8d)]
    [InlineData("7.9 ^ 0", 8d)]
    [InlineData("7.9 << 0", 8d)]
    [InlineData("7.9 >> 0", 8d)]
    [InlineData("~7.9", -9d)]
    public void EveryBitwiseOperatorRoundsItsOperandsRatherThanTruncating(
        string source,
        double expected) =>
        Assert.Equal(expected, Evaluate(source).AsNumber());

    /// <summary>
    /// An assignment binds tighter than a comparison: the 2 is stored and the
    /// stored value is then compared. Mutation: making assignment the loosest
    /// operator stores the comparison result (1) instead.
    /// </summary>
    [Fact]
    public void AssignmentBindsTighterThanComparison()
    {
        var state = new ExpressionState();

        ExpressionValue result = Evaluate("$x = 2 == 2", state);

        Assert.Equal(1d, result.AsNumber());
        ExpressionValue stored = state.Get(ExpressionVariableScope.Session, "x");
        Assert.Equal(ExpressionValueKind.Number, stored.Kind);
        Assert.Equal(2d, stored.AsNumber());
    }

    /// <summary>
    /// One row per operator pair: the tighter operator on each side of the
    /// looser one wherever the two orders answer differently.
    /// </summary>
    [Theory]
    // '~' and a negative literal bind tighter than every binary operator.
    [InlineData("~1 << 2", -8d)]
    [InlineData("~1 & 3", 2d)]
    [InlineData("~1 * 2", -4d)]
    [InlineData("~1 + 2", 0d)]
    [InlineData("~(1 + 2)", -4d)]
    [InlineData("-2 * 3", -6d)]
    [InlineData("-2 + 3", 1d)]
    [InlineData("3 - -2", 5d)]
    [InlineData("-2 & 3", 2d)]
    [InlineData("- 2", -2d)]
    // Shift binds tighter than the bitwise group.
    [InlineData("3 & 1 << 2", 0d)]
    [InlineData("1 << 2 & 3", 0d)]
    [InlineData("12 >> 1 | 1", 7d)]
    // The bitwise group is one level, left to right.
    [InlineData("6 ^ 3 & 1", 1d)]
    [InlineData("4 | 3 & 1", 1d)]
    [InlineData("4 ^ 3 & 2", 2d)]
    [InlineData("1 & 2 | 3 ^ 4", 7d)]
    // The bitwise group binds tighter than multiplication.
    [InlineData("2 & 3 * 2", 4d)]
    [InlineData("2 ^ 3 * 2", 2d)]
    [InlineData("12 | 3 * 2", 30d)]
    [InlineData("2 * 3 & 4", 0d)]
    [InlineData("6 / 3 & 2", 3d)]
    [InlineData("10 % 3 | 4", 3d)]
    // Multiplication binds tighter than addition.
    [InlineData("1 + 2 * 3", 7d)]
    [InlineData("2 * 3 + 1", 7d)]
    [InlineData("10 - 6 / 2", 7d)]
    // Shift binds tighter than addition, from both sides.
    [InlineData("1 + 2 << 3", 17d)]
    [InlineData("1 << 2 + 3", 7d)]
    // Addition binds tighter than the regex operator.
    [InlineData("1 + 1 # 3", 0d)]
    [InlineData("1 # 1 + 1", 0d)]
    // Comparison binds tighter than the boolean operators.
    [InlineData("0 == 1 || 2", 2d)]
    [InlineData("3 || 1 == 2", 3d)]
    // The boolean operators are one level, left to right.
    [InlineData("1 || 0 && 0", 0d)]
    // Left to right within a level.
    [InlineData("1 - 2 - 3", -4d)]
    [InlineData("64 >> 2 >> 2", 4d)]
    [InlineData("1 << 2 << 3", 32d)]
    [InlineData("1 < 2 == 1", 1d)]
    public void EveryOperatorPairBindsTheWayTheLadderSays(
        string source,
        double expected)
    {
        Assert.Equal(expected, Evaluate(source).AsNumber(), 10);
    }

    /// <summary>
    /// An assignment swallows everything tighter than a comparison and
    /// nothing looser, and it chains right to left. Mutation: moving the
    /// assignment level either way changes what the variable ends up holding.
    /// </summary>
    [Fact]
    public void AssignmentTakesEverythingTighterThanAComparison()
    {
        var state = new ExpressionState();

        // Arithmetic and the regex operator are part of the stored value.
        Assert.Equal(3d, Evaluate("$sum = 1 + 2", state).AsNumber());
        Assert.Equal(3d, state.Get(ExpressionVariableScope.Session, "sum").AsNumber());
        Assert.True(Evaluate("$hit = 5 # 5", state).IsTruthy);
        Assert.True(state.Get(ExpressionVariableScope.Session, "hit").IsTruthy);

        // A boolean operator is not: the assignment is its left operand.
        Assert.Equal(0d, Evaluate("$flag = 1 && 0", state).AsNumber());
        Assert.Equal(1d, state.Get(ExpressionVariableScope.Session, "flag").AsNumber());

        // Assignments chain right to left.
        Assert.Equal(3d, Evaluate("$a = $b = 3", state).AsNumber());
        Assert.Equal(3d, state.Get(ExpressionVariableScope.Session, "a").AsNumber());
        Assert.Equal(3d, state.Get(ExpressionVariableScope.Session, "b").AsNumber());
    }

    /// <summary>
    /// The four index forms and the slice bounds, and the fact that an index
    /// binds tighter than every operator around it. Mutation: parsing the
    /// brace body at anything but the loosest level makes the comparison
    /// index a parse error.
    /// </summary>
    [Fact]
    public void IndexAndSliceFormsBindTighterThanEveryOperator()
    {
        var state = new ExpressionState();
        Evaluate("$l = listcreate[10,20,30,40]", state);

        Assert.Equal(10d, Evaluate("$l{0}", state).AsNumber());
        Assert.Equal(40d, Evaluate("$l{-1}", state).AsNumber());
        Assert.Equal("[20,30,40]", Evaluate("$l{1:}", state).ToDisplayString());
        Assert.Equal("[10,20]", Evaluate("$l{:2}", state).ToDisplayString());
        Assert.Equal("[20,30]", Evaluate("$l{1:3}", state).ToDisplayString());
        Assert.Equal("[10,20,30,40]", Evaluate("$l{:}", state).ToDisplayString());
        Assert.Equal(20d, Evaluate("$l{1:3}{0}", state).AsNumber());

        // Tighter than the arithmetic and the bitwise group around it.
        Assert.Equal(11d, Evaluate("$l{0} + 1", state).AsNumber());
        Assert.Equal(2d, Evaluate("$l{0} & 3", state).AsNumber());
        Assert.Equal(30d, Evaluate("$l{1} + $l{0}", state).AsNumber());

        // A brace holds a whole expression.
        Assert.Equal(20d, Evaluate("$l{1 == 1}", state).AsNumber());
        Assert.Equal(20d, Evaluate("$l{0 + 1}", state).AsNumber());

        // Strings index and slice the same way.
        Assert.Equal("e", Evaluate("`hello`{1}").AsString());
        Assert.Equal("el", Evaluate("`hello`{1:3}").AsString());
    }

    /// <summary>
    /// A parenthesis, a call argument list and an index brace each start over
    /// at the loosest level, so a comparison or a boolean operator is legal
    /// inside them.
    /// </summary>
    [Fact]
    public void ParenthesesArgumentsAndBracesRestartAtTheLoosestLevel()
    {
        var state = new ExpressionState();

        Assert.Equal(9d, Evaluate("(1 + 2) * 3").AsNumber());
        Assert.Equal(1d, Evaluate("listcreate[1 == 1]{0}").AsNumber());
        Assert.Equal(2d, Evaluate("listcount[listcreate[1 == 1, 2 || 3]]").AsNumber());
        Assert.Equal(1d, Evaluate("$x = (1 == 1)", state).AsNumber());
        Assert.Equal(1d, state.Get(ExpressionVariableScope.Session, "x").AsNumber());
    }

    /// <summary>
    /// The atom forms: a hexadecimal literal, a number with an optional
    /// leading minus and an optional leading dot, a backtick string with
    /// backslash escapes, and a bare string that keeps its inner spaces.
    /// </summary>
    [Fact]
    public void AtomFormsParseAsAtoms()
    {
        Assert.Equal(255d, Evaluate("0xFF").AsNumber());
        Assert.Equal(17d, Evaluate("0x10 + 1").AsNumber());
        Assert.Equal(15d, Evaluate("0xff & 0x0f").AsNumber());
        Assert.Equal(-2d, Evaluate("-2").AsNumber());
        Assert.Equal(0.5d, Evaluate(".5").AsNumber(), 10);
        Assert.Equal(1.5d, Evaluate("1.5").AsNumber(), 10);
        Assert.Equal("hello world", Evaluate("`hello world`").AsString());
        Assert.Equal("a`b", Evaluate("`a\\`b`").AsString());
        Assert.Equal("\\", Evaluate("`\\\\`").AsString());
        Assert.Equal("Olthoi Noble", Evaluate("Olthoi Noble").AsString());
    }

    /// <summary>
    /// The backtick is the only quote. An apostrophe and a double quote are
    /// ordinary characters in an unquoted string, which is what lets an item
    /// name be written the way the game spells it. Mutation: treating either
    /// one as a quote splits "Olthoi's Claw" after "Olthoi" and then runs to
    /// the end of the input looking for a closing quote.
    /// </summary>
    [Fact]
    public void OnlyTheBacktickDelimitsAString()
    {
        // A possessive name, bare and as a call argument.
        Assert.Equal("Olthoi's Claw", Evaluate("Olthoi's Claw").AsString());
        Assert.Equal(
            "Olthoi's Claw",
            Evaluate("listcreate[Olthoi's Claw, Gharu'ndim Robe]{0}").AsString());
        Assert.Equal(
            "Gharu'ndim Robe",
            Evaluate("listcreate[Olthoi's Claw, Gharu'ndim Robe]{1}").AsString());

        // A name that starts with an apostrophe.
        Assert.Equal("'Neath Boots", Evaluate("'Neath Boots").AsString());
        Assert.Equal(12d, Evaluate("strlen['Neath Boots]").AsNumber());

        // A name containing a double quote keeps the quotes as characters.
        Assert.Equal("\"hello\"", Evaluate("\"hello\"").AsString());
        Assert.Equal("Say \"Ho\" Staff", Evaluate("Say \"Ho\" Staff").AsString());

        // The backtick still quotes and still strips.
        Assert.Equal("Olthoi's Claw", Evaluate("`Olthoi's Claw`").AsString());
        Assert.Equal("a`b", Evaluate("`a\\`b`").AsString());

        // An apostrophe is ordinary on both sides of a comparison.
        Assert.True(Evaluate("Olthoi's Claw==`Olthoi's Claw`").IsTruthy);

        // A space is an ordinary character in an unquoted string wherever it
        // sits, the last one included, so the space written before the
        // operator belongs to the name on its left. Written with spaces, the
        // name being compared is the one with the trailing space.
        Assert.True(Evaluate("Olthoi's Claw == `Olthoi's Claw `").IsTruthy);
        Assert.False(Evaluate("Olthoi's Claw == `Olthoi's Claw`").IsTruthy);
    }

    /// <summary>
    /// A hexadecimal literal is a 32-bit pattern read as a signed number, so
    /// a flags value compares equal to the literal a rule is written with.
    /// Mutation: reading it unsigned makes 0xFFFFFFFF answer 4294967295 and
    /// stop comparing equal to -1.
    /// </summary>
    [Fact]
    public void HexadecimalLiteralsAreSigned32BitPatterns()
    {
        Assert.Equal(-1d, Evaluate("0xFFFFFFFF").AsNumber());
        Assert.Equal(-2147483648d, Evaluate("0x80000000").AsNumber());
        Assert.Equal(2147483647d, Evaluate("0x7FFFFFFF").AsNumber());
        Assert.Equal(255d, Evaluate("0x00000000FF").AsNumber());
        Assert.True(Evaluate("0xFFFFFFFF == 0 - 1").IsTruthy);

        // Bit operations are unaffected: they truncate to the same 32 bits.
        Assert.Equal(255d, Evaluate("0xFFFFFFFF & 0xFF").AsNumber());
        Assert.Equal(0d, Evaluate("0x80000000 & 0x7FFFFFFF").AsNumber());

        // A pattern wider than 32 bits is a parse error, not an overflow
        // escaping from the conversion.
        ExpressionParseException error = Assert.Throws<ExpressionParseException>(
            () => Evaluate("0x100000000"));
        Assert.Contains("32 bits", error.Message, StringComparison.Ordinal);
    }

    /// <summary>
    /// The reference parser recovers from one missing or one extra token
    /// without a word, and profiles rely on it. A ';' where a ')' is due
    /// closes the group (the ';' may follow it), so a parenthesised sequence
    /// runs as two statements and the stray ')' at the end is dropped.
    /// Mutation: without the recovery the parser stops at the ';'.
    /// </summary>
    [Fact]
    public void ASemicolonInsideParenthesesRunsAsASequence()
    {
        var state = new ExpressionState();
        ExpressionValue result = Evaluate("(setvar[a,1];setvar[b,2])", state);
        Assert.Equal(2d, result.AsNumber());
        Assert.Equal(1d, Evaluate("getvar[a]", state).AsNumber());
        Assert.Equal(2d, Evaluate("getvar[b]", state).AsNumber());
    }

    /// <summary>
    /// One extra token before the end of the expression is dropped: the
    /// stray ')' or ']' an author left at the end of a line. Mutation:
    /// without the recovery both are "Unexpected token".
    /// </summary>
    [Fact]
    public void AStrayClosingTokenAtTheEndIsDropped()
    {
        Assert.Equal(3d, Evaluate("1+(4-2))").AsNumber());
        Assert.Equal(3d, Evaluate("strlen[abc]]").AsNumber());
        Assert.Equal(2d, Evaluate("1;2)").AsNumber());
    }

    /// <summary>
    /// A missing ')' is supplied when the token in its place is one that
    /// could follow it: a ']' that closes the call, or the end. Mutation:
    /// without the recovery both are "Closing ')' expected".
    /// </summary>
    [Fact]
    public void AMissingParenthesisIsSuppliedBeforeWhatMayFollowIt()
    {
        Assert.Equal(6d, Evaluate("round[2*(1+2]").AsNumber());
        Assert.Equal(9d, Evaluate("3*(1+2").AsNumber());
    }

    /// <summary>
    /// The recovery happens only where the reference parser makes it: an
    /// expression broken in any other way stays an error. A ')' is not
    /// supplied before a token that cannot follow a group, two operators in a
    /// row are not one missing operand, an operator with nothing after it is
    /// incomplete, and after a first statement only one stray token is
    /// dropped. (The full table, taken from the reference parser itself, is
    /// in ExpressionReferenceParserParityTests.) Mutation: accepting any of
    /// them.
    /// </summary>
    [Theory]
    [InlineData("istrue[getvar[a]==0&&&&getvar[b]==0]")]
    [InlineData("getvar[a]<")]
    [InlineData("1 2 3")]
    [InlineData("(1))+2")]
    [InlineData("(1 2")]
    [InlineData("(1 abc")]
    public void ABrokenExpressionStaysBroken(string source) =>
        Assert.Throws<ExpressionParseException>(() => Evaluate(source));

    private static ExpressionValue Evaluate(
        string source,
        ExpressionState? state = null)
    {
        var context = new ExpressionEvaluationContext(
            state ?? new ExpressionState(),
            CoreExpressionFunctions.CreateDefault(new Random(1234)));
        return ExpressionProgram.Compile(source).Evaluate(context);
    }
}

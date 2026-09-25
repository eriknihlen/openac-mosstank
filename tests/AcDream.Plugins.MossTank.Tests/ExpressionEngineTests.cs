using System.Globalization;
using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class ExpressionEngineTests
{
    [Theory]
    [InlineData("1+2*3", 7d)]
    [InlineData("(1+2)*3", 9d)]
    [InlineData("0x10+1", 17d)]
    [InlineData("~1", -2d)]
    [InlineData("16>>2", 4d)]
    public void ArithmeticUsesUtilityBeltPrecedence(string source, double expected)
    {
        Assert.Equal(expected, Evaluate(source).AsNumber(), 10);
    }

    /// <summary>
    /// '^' rounds both sides to the nearest integer and answers their
    /// exclusive-or, the same conversion its level-mates use: 7.9 is 8 to all
    /// of them, so "7.9^3.2" is 8 ^ 3. It belongs to the bitwise group, which
    /// binds tighter than arithmetic, so "1||0^1" is 1 || (0 ^ 1) and answers
    /// the truthy left operand, and "2^3==1" is (2 ^ 3) == 1 and answers
    /// true. '&amp;&amp;' and '||' stay on one level, left to right, which is
    /// why "1||0&amp;&amp;0" is 0.
    /// </summary>
    [Theory]
    [InlineData("0^1", 1d)]
    [InlineData("7.9^3.2", 11d)]
    [InlineData("7.9^0", 8d)]
    [InlineData("1||0^1", 1d)]
    [InlineData("1||0&&0", 0d)]
    [InlineData("2^3==1", 1d)]
    public void BitwiseXorRoundsToIntegersAndBindsAboveArithmetic(
        string source,
        double expected)
    {
        Assert.Equal(expected, Evaluate(source).AsNumber());
    }

    [Fact]
    public void StatementsAndAllVariableScopesReturnLastValue()
    {
        var state = new ExpressionState();

        ExpressionValue result = Evaluate(
            "$session=2;@persistent=3;&global=4;"
            + "setvar[dynamic,5];$session+@persistent+&global+$dynamic",
            state);

        Assert.Equal(14d, result.AsNumber());
        Assert.Equal(2d, state.Get(ExpressionVariableScope.Session, "session").AsNumber());
        Assert.Equal(3d, state.Get(ExpressionVariableScope.Persistent, "persistent").AsNumber());
        Assert.Equal(4d, state.Get(ExpressionVariableScope.Global, "global").AsNumber());
    }

    [Fact]
    public void NumericAndComputedVariableNamesAreSupported()
    {
        var state = new ExpressionState();

        ExpressionValue result = Evaluate(
            "$1=7;$name=`chosen`;$getvar[name]=9;$1+$chosen",
            state);

        Assert.Equal(16d, result.AsNumber());
    }

    [Fact]
    public void BooleanOperatorsShortCircuitAndReturnUtilityBeltValues()
    {
        ExpressionFunctionRegistry functions = CoreExpressionFunctions.CreateDefault();
        int calls = 0;
        functions.Register("boom", 0, 0, (_, _) =>
        {
            calls++;
            throw new InvalidOperationException("must not run");
        });
        var context = new ExpressionEvaluationContext(new ExpressionState(), functions);

        Assert.Equal(0d, ExpressionProgram.Compile("0&&boom[]").Evaluate(context).AsNumber());
        Assert.Equal(3d, ExpressionProgram.Compile("3||boom[]").Evaluate(context).AsNumber());
        Assert.Equal(0, calls);
    }

    /// <summary>
    /// UtilityBelt's '&amp;&amp;' answers false (0) when either side is
    /// false, and the right side itself only when both are true, so a true
    /// left and an empty string answer the number 0, not the empty string.
    /// Mutation: handing the right side back whatever it is answers "".
    /// </summary>
    [Theory]
    [InlineData("1&&``")]
    [InlineData("`a`&&0.0")]
    public void AndWithAFalseRightSideAnswersZeroAsInUtilityBelt(string source)
    {
        ExpressionValue result = Evaluate(source);
        Assert.Equal(ExpressionValueKind.Number, result.Kind);
        Assert.Equal(0d, result.AsNumber());
    }

    [Fact]
    public void AndWithBothSidesTrueAnswersTheRightSideAsInUtilityBelt()
    {
        Assert.Equal("b", Evaluate("`a`&&`b`").AsString());
    }

    /// <summary>
    /// UtilityBelt compares a string on the left of '==' or '!=' with the
    /// right side as it prints, ignoring case, whatever the right side is:
    /// a capture group (always a string) equals the number it spells. A
    /// number on the left equals only a number. Mutation: requiring both
    /// sides to be strings makes the capture comparison false.
    /// </summary>
    [Theory]
    [InlineData("`5`==5", 1d)]
    [InlineData("`5`!=5", 0d)]
    [InlineData("`2.5`==5/2", 1d)]
    [InlineData("`[1,A]`==listcreate[1,`a`]", 1d)]
    [InlineData("5==`5`", 0d)]
    [InlineData("5!=`5`", 1d)]
    public void AStringOnTheLeftComparesWithTheRightAsItPrintsAsInUtilityBelt(
        string source,
        double expected)
    {
        Assert.Equal(expected, Evaluate(source).AsNumber());
    }

    /// <summary>
    /// The case the rule exists for: a regex capture is a string, and in
    /// UtilityBelt it equals the number it spells. Mutation: requiring both
    /// sides to be strings answers 0.
    /// </summary>
    [Fact]
    public void ACaptureGroupEqualsTheNumberItSpellsAsInUtilityBelt()
    {
        var state = new ExpressionState();
        Assert.Equal(1d, Evaluate("`level 5`#`level (?<n>[0-9]+)`;$capturegroup_n==5", state).AsNumber());
    }

    /// <summary>
    /// UtilityBelt's list functions find an item with the runtime's own
    /// equality, so a string matches only exactly, case and all, and a
    /// number never matches the string that spells it. Mutation: comparing
    /// strings case-blind finds "Bob" for bob.
    /// </summary>
    [Theory]
    [InlineData("listcontains[listcreate[Bob],bob]", 0d)]
    [InlineData("listcontains[listcreate[Bob],Bob]", 1d)]
    [InlineData("listindexof[listcreate[a,A],A]", 1d)]
    [InlineData("listlastindexof[listcreate[A,a,b],A]", 0d)]
    [InlineData("listcount[listremove[listcreate[Bob],bob]]", 1d)]
    [InlineData("listcontains[listcreate[`5`],5]", 0d)]
    public void ListFunctionsFindAnItemExactlyAsInUtilityBelt(string source, double expected)
    {
        Assert.Equal(expected, Evaluate(source).AsNumber());
    }

    [Fact]
    public void StringsAreCaseInsensitiveAndPreserveDashedBareText()
    {
        Assert.True(Evaluate("`Olthoi`==`olthoi`").IsTruthy);
        Assert.Equal("Olthoi-Noble", Evaluate("Olthoi-Noble").AsString());
        Assert.Equal("Olthoi Noble", Evaluate("`Olthoi `+Noble").AsString());
    }

    /// <summary>
    /// UtilityBelt answers a string minus a string with the subtraction's
    /// source text (its tokens with nothing between them), not with the
    /// values joined by a dash: variables stay named and quotes stay.
    /// Mutation: joining the values answers "x-y" and "a-b".
    /// </summary>
    [Theory]
    [InlineData("$a=x;$b=y;$a-$b", "$a-$b")]
    [InlineData("`a`-`b`", "`a`-`b`")]
    [InlineData("$a=x;($a)-b-c", "($a)-b-c")]
    [InlineData("Olthoi-Noble", "Olthoi-Noble")]
    public void AStringMinusAStringIsItsSourceTextAsInUtilityBelt(string source, string expected)
    {
        Assert.Equal(expected, Evaluate(source).AsString());
    }

    /// <summary>
    /// An escaped delimiter is an ordinary character in an unquoted string,
    /// which is what lets a meta build a chat command out of one. The space
    /// before the '+' is part of the string as well — a space inside an
    /// unquoted string is an ordinary character wherever it sits — so the
    /// command keeps the space that separates it from its argument.
    /// </summary>
    [Theory]
    [InlineData(@"\/vt nav load +cstr[3]", "/vt nav load 3")]
    [InlineData(@"\/f Pick flowers\: +cstr[3]", "/f Pick flowers: 3")]
    public void EscapedDelimitersRemainPartOfBareStrings(
        string source,
        string expected)
    {
        Assert.Equal(expected, Evaluate(source).AsString());
    }

    [Fact]
    public void RegexOperatorPublishesCaptureGroups()
    {
        var state = new ExpressionState();

        ExpressionValue result = Evaluate(
            "`Olthoi 275`#`(?<level>[0-9]+)`;$capturegroup_level",
            state);

        Assert.Equal("275", result.AsString());
    }

    /// <summary>
    /// UtilityBelt sets the capture groups only when '#' matches, so a
    /// failed match leaves the last match's groups readable. Mutation:
    /// clearing the groups on a failed match reads 0.
    /// </summary>
    [Fact]
    public void AFailedRegexMatchLeavesTheLastCapturesAsInUtilityBelt()
    {
        var state = new ExpressionState();

        ExpressionValue result = Evaluate(
            "`Olthoi 275`#`(?<level>[0-9]+)`;`Olthoi`#`(?<level>[0-9]+)`;$capturegroup_level",
            state);

        Assert.Equal("275", result.ToDisplayString());
        Assert.Equal(0d, Evaluate("`Olthoi`#`(?<level>[0-9]+)`", state).AsNumber());
    }

    [Fact]
    public void ListsSupportMutationIndexSlicesAndHigherOrderFunctions()
    {
        var state = new ExpressionState();
        Evaluate("$0=old0;$1=old1;$2=old2", state);

        Assert.Equal(4d, Evaluate(
            "$items=listcreate[1,2,3];listadd[$items,4];listcount[$items]",
            state).AsNumber());
        Assert.Equal(4d, Evaluate("$items{-1}", state).AsNumber());
        Assert.Equal("[2,3]", Evaluate("$items{1:3}", state).ToDisplayString());
        Assert.Equal("[2,4,6,8]", Evaluate(
            "listmap[$items,`$1*2`]", state).ToDisplayString());
        Assert.Equal("[1,3]", Evaluate(
            "listfilter[$items,`$1%2==1`]", state).ToDisplayString());
        Assert.Equal(10d, Evaluate(
            "listreduce[$items,`$2+$1`]", state).AsNumber());
        Assert.Equal("[4,3,2,1]", Evaluate(
            "listsort[$items,`$2-$1`]", state).ToDisplayString());
        // As the reference leaves them: listfilter restores only after an
        // item it keeps, so its last, dropped item (index 3, the 4) stays;
        // listreduce restores what it found; listsort leaves the last pair
        // it compared.
        Assert.Equal(3d, state.Get(ExpressionVariableScope.Session, "0").AsNumber());
        Assert.Equal(2d, state.Get(ExpressionVariableScope.Session, "1").AsNumber());
        Assert.Equal(2d, state.Get(ExpressionVariableScope.Session, "2").AsNumber());
    }

    /// <summary>
    /// UtilityBelt's listfromrange counts up from the start to the end, both
    /// included, so a start past the end gives an empty list rather than a
    /// count down. Mutation: counting down answers [3,2,1].
    /// </summary>
    [Fact]
    public void ListRangeCountsUpOnlyAsInUtilityBelt()
    {
        Assert.Equal("[1,2,3]", Evaluate("listfromrange[1,3]").ToDisplayString());
        Assert.Equal("[2]", Evaluate("listfromrange[2,2]").ToDisplayString());
        Assert.Equal("[]", Evaluate("listfromrange[3,1]").ToDisplayString());
        Assert.Equal("[1,2]", Evaluate("listfromrange[1.9,2.9]").ToDisplayString());
    }

    /// <summary>
    /// listsort without an expression sorts as UtilityBelt's runtime does by
    /// default: numbers by value, strings in the culture's word order (a
    /// punctuation mark before a letter, lower case before upper case).
    /// Mutation: comparing strings ordinally and case-blind puts "a" before
    /// "_".
    /// </summary>
    [Theory]
    [InlineData("listsort[listcreate[`a`,`_`]]", "[_,a]")]
    [InlineData("listsort[listcreate[`B`,`b`,`a`]]", "[a,b,B]")]
    [InlineData("listsort[listcreate[3,1,2]]", "[1,2,3]")]
    [InlineData("listsort[listcreate[listcreate[1]]]", "[[1]]")]
    public void ListSortWithoutAnExpressionUsesTheDefaultOrderAsInUtilityBelt(
        string source,
        string expected)
    {
        Assert.Equal(expected, Evaluate(source).ToDisplayString());
    }

    /// <summary>
    /// UtilityBelt's default order cannot compare a number with a string, or
    /// two lists, so such a sort fails. Mutation: ordering by kind sorts it.
    /// </summary>
    [Theory]
    [InlineData("listsort[listcreate[1,`a`]]")]
    [InlineData("listsort[listcreate[listcreate[1],listcreate[2]]]")]
    public void ListSortWithoutAnExpressionFailsOnValuesItCannotCompareAsInUtilityBelt(
        string source)
    {
        Assert.Throws<ExpressionEvaluationException>(() => Evaluate(source));
    }

    [Fact]
    public void DictionariesSupportMutationAndShallowCopy()
    {
        var state = new ExpressionState();

        Assert.Equal(2d, Evaluate(
            "$dict=dictcreate[a,1,b,2];$dict{b}", state).AsNumber());
        Assert.False(Evaluate("dictadditem[$dict,c,3]", state).IsTruthy);
        Assert.True(Evaluate("dictadditem[$dict,c,4]", state).IsTruthy);
        Assert.Equal(3d, Evaluate("dictsize[$dict]", state).AsNumber());
        Assert.Equal("[a,b,c]", Evaluate("dictkeys[$dict]", state).ToDisplayString());
        Assert.Equal(4d, Evaluate("dictgetitem[dictcopy[$dict],c]", state).AsNumber());
        Assert.True(Evaluate("dictremovekey[$dict,b]", state).IsTruthy);
    }

    [Fact]
    public void CollectionsRejectDirectAndIndirectCycles()
    {
        var state = new ExpressionState();
        Evaluate("$first=listcreate[];$second=listcreate[$first]", state);

        ExpressionEvaluationException direct = Assert.Throws<ExpressionEvaluationException>(
            () => Evaluate("listadd[$first,$first]", state));
        ExpressionEvaluationException indirect = Assert.Throws<ExpressionEvaluationException>(
            () => Evaluate("listadd[$first,$second]", state));

        Assert.Contains("cyclic", direct.Message, StringComparison.OrdinalIgnoreCase);
        Assert.Contains("cyclic", indirect.Message, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void CoordinatesUseAcCompassScaleAndMeters()
    {
        Assert.Equal(-12.5d, Evaluate(
            "coordinategetns[coordinateparse[`12.5S, 3.0E`]]").AsNumber());
        Assert.Equal(24d, Evaluate(
            "coordinatedistanceflat[coordinateparse[`0N, 0E`],"
            + "coordinateparse[`0.1N, 0E`]]").AsNumber(), 8);
    }

    /// <summary>
    /// The reference's text form: both halves with two decimals, then the
    /// height reading with two decimals and a Z, whatever the culture.
    /// Profiles cut the last seven characters (", 0.24Z") off it to get a
    /// point they can hand to the route commands. Mutation: the old one-
    /// decimal form without a height gives "12.5S, 3E".
    /// </summary>
    [Fact]
    public void CoordinateToStringHasTwoDecimalsAndAHeight()
    {
        Assert.Equal("12.50S, 3.00E, 0.00Z", Evaluate(
            "coordinatetostring[coordinateparse[`12.5S, 3.0E`]]").AsString());
        Assert.Equal("12.55S, 3.14W, 1.00Z", Evaluate(
            "coordinatetostring[coordinateparse[`12.55S, 3.14W, 240.0Z`]]").AsString());
        Assert.Equal("0.10N, 0.00E, -0.50Z", Evaluate(
            "coordinatetostring[coordinateparse[`0.1N, 0E, -120.0Z`]]").AsString());
        var previous = CultureInfo.CurrentCulture;
        try
        {
            CultureInfo.CurrentCulture = CultureInfo.GetCultureInfo("sv-SE");
            Assert.Equal("36.50S, 28.90E, 0.24Z", Evaluate(
                "coordinatetostring[coordinateparse[`36.5S, 28.9E, 57.6Z`]]").AsString());
        }
        finally
        {
            CultureInfo.CurrentCulture = previous;
        }
    }

    /// <summary>
    /// The reference parser: a search (not a whole-string match) for
    /// "value N/S [, or spaces] value [E/W] [, value Z]". The Z part is read
    /// as a raw height, so its reading is that number over 240; the E/W
    /// letter may be missing (east); text with no coordinate in it parses as
    /// 0N, 0E instead of failing. Mutation: the old anchored pattern throws
    /// "Unable to parse coordinate" on the first line.
    /// </summary>
    [Fact]
    public void CoordinateParseReadsTheReferenceFormsIncludingZ()
    {
        Assert.Equal(-41.34d, Evaluate(
            "coordinategetns[coordinateparse[`41.34S, 44.09E, 0.48Z`]]").AsNumber());
        Assert.Equal(44.09d, Evaluate(
            "coordinategetwe[coordinateparse[`41.34S, 44.09E, 0.48Z`]]").AsNumber());
        Assert.Equal(0.48d / 240d, Evaluate(
            "coordinategetz[coordinateparse[`41.34S, 44.09E, 0.48Z`]]").AsNumber(), 12);
        Assert.Equal(-12.3d / 240d, Evaluate(
            "coordinategetz[coordinateparse[`1.2N 3.4W -12.3z`]]").AsNumber(), 12);
        Assert.Equal(-3.4d, Evaluate(
            "coordinategetwe[coordinateparse[`1.2N 3.4W -12.3z`]]").AsNumber());
        Assert.Equal(3.4d, Evaluate(
            "coordinategetwe[coordinateparse[`at 1.2n,3.4 please`]]").AsNumber());
        Assert.Equal(0d, Evaluate(
            "coordinategetns[coordinateparse[`nowhere`]]").AsNumber());
        Assert.Equal(0d, Evaluate(
            "coordinategetwe[coordinateparse[`nowhere`]]").AsNumber());
    }

    [Fact]
    public void InstructionBudgetAndCancellationBoundNestedEvaluation()
    {
        var functions = CoreExpressionFunctions.CreateDefault();
        var budgeted = new ExpressionEvaluationContext(
            new ExpressionState(), functions, instructionBudget: 25);
        Assert.Throws<ExpressionEvaluationException>(() =>
            ExpressionProgram.Compile(
                "listmap[listfromrange[1,100],`$1*2`]").Evaluate(budgeted));

        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();
        var cancelled = new ExpressionEvaluationContext(
            new ExpressionState(), functions, cancellationToken: cancellation.Token);
        Assert.Throws<OperationCanceledException>(() =>
            ExpressionProgram.Compile("1+1").Evaluate(cancelled));
    }

    [Fact]
    public void FunctionErrorsNameTheSignatureAndOffset()
    {
        ExpressionEvaluationException error = Assert.Throws<ExpressionEvaluationException>(
            () => Evaluate("sqrt[1,2]"));

        Assert.Contains("sqrt[number]", error.Message, StringComparison.Ordinal);
        Assert.Contains("offset 0", error.Message, StringComparison.Ordinal);
    }

    /// <summary>
    /// An error the runtime raises inside an operator ends the run like any
    /// other error, as every exception ends UtilityBelt's run: a NaN or a
    /// number past 32 bits under a bitwise or shift operator, and a pattern
    /// that does not parse. The caller sees the engine's evaluation
    /// error carrying the runtime's message, never the raw exception.
    /// Mutation: letting the raw exception out fails the type check.
    /// </summary>
    [Theory]
    [InlineData("(0/0)&1")]
    [InlineData("10000000000|0")]
    [InlineData("~(0/0)")]
    [InlineData("1<<(0/0)")]
    [InlineData("`abc`#`(`")]
    public void ARuntimeErrorInsideAnOperatorIsAnEvaluationErrorAsInUtilityBelt(
        string source)
    {
        var error = Assert.Throws<ExpressionEvaluationException>(() => Evaluate(source));
        Assert.NotNull(error.InnerException);
        Assert.Equal(error.InnerException!.Message, error.Reason);
    }

    /// <summary>
    /// UtilityBelt cuts an index toward zero, so the middle item of a
    /// three-item list, $l{listcount[$l]/2}, is item 1. A number with no
    /// whole 32-bit part is out of range. Mutation: rounding the index to the
    /// nearest whole number reads item 2.
    /// </summary>
    [Theory]
    [InlineData("$l=listcreate[a,b,c];$l{listcount[$l]/2}", "b")]
    [InlineData("`abc`{1.9}", "b")]
    [InlineData("`abc`{-1.5}", "c")]
    [InlineData("$l=listcreate[a,b,c];tostring[$l{0:1.5}]", "[a]")]
    public void AnIndexIsCutTowardZeroAsInUtilityBelt(string source, string expected)
    {
        Assert.Equal(expected, Evaluate(source).ToDisplayString());
    }

    [Theory]
    [InlineData("`abc`{0/0}")]
    [InlineData("`abc`{9999999999}")]
    public void AnIndexWithNoWholeThirtyTwoBitPartIsOutOfRange(string source)
    {
        Assert.Throws<ExpressionEvaluationException>(() => Evaluate(source));
    }

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

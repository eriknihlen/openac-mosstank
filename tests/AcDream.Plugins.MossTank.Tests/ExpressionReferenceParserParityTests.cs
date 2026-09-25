using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The reference expression parser's verdicts, taken from the reference
/// parser itself run over each expression: what it accepts, what it refuses,
/// and for the lines its lexer cuts, the text it actually runs. Profiles are
/// written and tested against that parser, so a meta line has to mean here
/// what it meant there.
/// </summary>
public sealed class ExpressionReferenceParserParityTests
{
    public static TheoryData<string, bool> ReferenceVerdicts => new()
    {
        // A '!' not followed by '=' is dropped with the character after it
        // (the fellowship-recruit filters write "...[24,$1]!0&&...").
        { "x[24,1]!0&&y[]==0", true },
        { "1!2", true },
        { "echo[a! b,13]", true },
        { "1!=2", true },
        { "!", false },
        { "a! =b", false },
        // A character no token starts with is dropped on its own; a '.' is
        // dropped with the character after it unless a number follows.
        { "a?b", true },
        { "a.b", true },
        { "a\\", true },
        { "1.", true },
        { "1.2.3", true },
        { ".x", false },
        { "echo[a.]", false },
        { "`abc", false },
        // Numbers and hexadecimal: '0x' needs a digit and a lower-case x.
        { "0x", true },
        { "0X1", true },
        { "5abc", true },
        { "a:b", false },
        { "x[true ,1]", true },
        // After the second or later argument, any run of stray tokens is
        // skipped up to what may follow, and missing ']' are supplied.
        { "f[1,2)x)]", true },
        { "f[1,2 ) ) ]", true },
        { "f[1,2", true },
        { "f[1,g[2,3", true },
        { "f[g[2,3", true },
        { "f[1,2 3+4]", true },
        { "f[1,2 3;4", true },
        { "f[1,2 3=4]", true },
        { "f[1,2 3 x;y", true },
        { "f[1,g[2,3;5", true },
        { "(g[2,3", true },
        { "f[(g[2,3", true },
        { "(f[1,2 3)", true },
        { "max[1,3", true },
        { "max[1,3;4]", true },
        // After a first argument only one stray token is dropped.
        { "f[1)x)]", false },
        { "f[1 2 3]", false },
        { "f[1", false },
        { "f[1,g[2 3", false },
        { "g[f[1,2 3),4]", false },
        // Statements: after the second or later one, stray tokens are
        // skipped to the next ';' or the end; after the first, only one.
        { "1;2 3 4", true },
        { "1;2 3+4", true },
        { "(1;2 3)", true },
        { "1 2", true },
        { "1 2 3", false },
        { "(1))+2", false },
        // A missing ')' is supplied before anything that may follow any
        // group, call or statement it sits in.
        { "((1;2))", true },
        { "max[(1;2),3]", true },
        { "1+(2;3)*4", true },
        { "f[(1+2;3]", true },
        { "f[(1;2)]", true },
        { "f[1,(1+2", true },
        { "(1 2", false },
        { "(1 abc", false },
        { "f[1,(2 3 4)]", false },
        { "f[1{0 2]", false },
        { "getvar[a]<", false },
        { "istrue[getvar[a]==0&&&&getvar[b]==0]", false },
        // A string value must be followed by what may end the construct it
        // sits in when the next token ends constructs at all: the reference
        // parser decides between a value and a call name by looking at it.
        { "(a;b)", false },
        { "(1;b)", false },
        { "f[(b,1)]", false },
        { "$b)", false },
        { "b]", false },
        { "f[1,b", false },
        { "f[1,b,c", false },
        { "f[b)]", false },
        { "$b=1", true },
        { "x{b:1}", true },
        { "f[1+b]", true },
        { "`s`;1", true },
        { "true)", true },
        { "5)", true },
        // Corpus shapes.
        {
            "@RynCMDDebugMode==1&&echo[\\[RynCMD\\] Restocking Mana Scarabs, Platinum Scarabs, Prismatic Tapers, and Mana Charges. Threshold is 15 Mana Scarabs, 15 Platinum Scarabs, 600 Prismatic Tapers, and 1 Titan Mana Charge.]",
            false
        },
        {
            "listcount[listfilter[$list,`wobjectfindnearestbynameandobjectclass[24,$1]!0&&listcontains[getfellownames[],$1]==0`]]!=0",
            true
        },
        { "wobjectfindnearestbynameandobjectclass[24,$1]!0&&listcontains[getfellownames[],$1]==0", true },
        {
            "iif[a==1,0.09,iif[a==2,0.07,iif[a==3,0.05,iif[a==4,`2600`],0.03,0)\")\")\")\")]]",
            true
        },
        { "@RynCMDDebugMode==1&&echo[\\[RynCMD\\] Restocking. Threshold is 5 Mana Scarabs.]", false },
        { "chatbox[\\/mt opt set Looting.AutoLootChests false]", true },
        { "echo[\\[RynCMD\\] Exiting to RynCMD. Will continue routing,13]", true },
    };

    [Theory]
    [MemberData(nameof(ReferenceVerdicts))]
    public void TheParserAcceptsWhatTheReferenceParserAccepts(string source, bool accepted)
    {
        Exception? error = Record.Exception(() => ExpressionProgram.Compile(source));
        if (accepted)
            Assert.Null(error);
        else
            Assert.IsType<ExpressionParseException>(error);
    }

    /// <summary>
    /// The text the reference runs for lines its lexer cuts: the '.' and the
    /// character after it are dropped, and the string after them is a stray
    /// token the parser deletes.
    /// </summary>
    [Theory]
    [InlineData("capture[\\/mt opt set Looting.AutoLootChests false]", "/mt opt set Looting")]
    [InlineData("capture[\\[RynCMD\\] Exiting to RynCMD. Will continue routing,13]", "[RynCMD] Exiting to RynCMD")]
    [InlineData(
        "capture[\\[RynCMD\\] Attempting to enable Vtank patch expression engine with \\/ub bc \\/ub opt set VTank.PatchExpressionEngine True,13]",
        "[RynCMD] Attempting to enable Vtank patch expression engine with /ub bc /ub opt set VTank")]
    [InlineData("capture[a? b]", "a")]
    public void ACutLineRunsTheTextTheReferenceRuns(string source, string expected)
    {
        var captured = new List<string>();
        ExpressionFunctionRegistry functions = CoreExpressionFunctions.CreateDefault(new Random(1));
        functions.Register("capture", 1, 2, (_, args) =>
        {
            captured.Add(args[0].ToDisplayString());
            return ExpressionValue.One;
        }, "capture[text,color?]");

        ExpressionProgram.Compile(source).Evaluate(
            new ExpressionEvaluationContext(new ExpressionState(), functions));

        Assert.Equal(expected, Assert.Single(captured));
    }

    /// <summary>
    /// The fellowship filter's typo reads as "exists and not in the
    /// fellowship": the '!0' is dropped, not the rest of the condition.
    /// </summary>
    [Fact]
    public void ABangWithoutEqualsDropsItselfAndTheNextCharacter()
    {
        var state = new ExpressionState();
        ExpressionFunctionRegistry functions = CoreExpressionFunctions.CreateDefault(new Random(1));
        ExpressionValue value = ExpressionProgram.Compile("5!0&&7").Evaluate(
            new ExpressionEvaluationContext(state, functions));

        Assert.Equal(7d, value.AsNumber());
    }
}

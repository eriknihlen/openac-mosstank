using AcDream.Plugins.MossTank.Expressions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// Pins the built-in expression functions to VTank's own behaviour, so a
/// profile written against VTank evaluates identically here. Every fact below
/// is read off the VTank build these functions are compatible with; the
/// research note for the expression evaluator carries the quoted bodies.
/// </summary>
public sealed class ExpressionVtankParityTests
{
    /// <summary>
    /// The internal type tag works on ANY value and answers with the token
    /// type, not with a game item type. Mutation: aliasing the name onto the
    /// world-object variant makes every one of these throw a type error.
    /// </summary>
    [Theory]
    [InlineData("getobjectinternaltype[5]", 1d)]
    [InlineData("getobjectinternaltype[1==1]", 1d)]
    [InlineData("getobjectinternaltype[`text`]", 3d)]
    [InlineData("getobjectinternaltype[stopwatchcreate[]]", 7d)]
    [InlineData("getobjectinternaltype[coordinateparse[`10.0N, 20.0E`]]", 7d)]
    [InlineData("getobjectinternaltype[listcreate[1,2]]", 7d)]
    public void ObjectInternalTypeReportsTheTokenType(string source, double expected)
    {
        Assert.Equal(expected, Evaluate(source).AsNumber());
    }

    /// <summary>
    /// Number conversion accepts group separators, the way the two-argument
    /// double.TryParse VTank calls does. Mutation: dropping AllowThousands
    /// turns "1,234" into 0.
    /// </summary>
    [Theory]
    [InlineData("cnumber[`1,234`]", 1234d)]
    [InlineData("cnumber[`1,234.5`]", 1234.5d)]
    [InlineData("cnumber[`-12`]", -12d)]
    [InlineData("cnumber[`nonsense`]", 0d)]
    public void NumberConversionAcceptsGroupSeparatorsAndFailsToZero(
        string source,
        double expected)
    {
        Assert.Equal(expected, Evaluate(source).AsNumber());
    }

    /// <summary>
    /// Elapsed seconds are quantised to whole milliseconds, because VTank
    /// divides an integer millisecond count by 1000. Mutation: reading
    /// TotalSeconds leaves a sub-millisecond remainder.
    /// </summary>
    [Fact]
    public void StopwatchElapsedSecondsAreQuantisedToWholeMilliseconds()
    {
        var stopwatch = new ExpressionStopwatch();
        stopwatch.Start();
        while (stopwatch.ElapsedSeconds <= 0d)
        {
            // Spin until the first whole millisecond lands.
        }
        stopwatch.Stop();

        double milliseconds = stopwatch.ElapsedSeconds * 1000d;
        Assert.Equal(Math.Round(milliseconds), milliseconds, 9);
    }

    /// <summary>
    /// `cstrf` renders through an unsigned integer ONLY for a whole-string
    /// hex specifier. A custom numeric format that merely contains a literal
    /// x is a format like any other and keeps the number's fractional digits.
    /// Mutation: routing any format containing an x or X into the hex branch
    /// turns "3.5 x" into "3.0 x" and "3.5 X" into "3.0 X".
    /// </summary>
    [Theory]
    [InlineData("cstrf[255,`X`]", "FF")]
    [InlineData("cstrf[255,`x`]", "ff")]
    [InlineData("cstrf[255,`X4`]", "00FF")]
    [InlineData("cstrf[3.5,`0.0 x`]", "3.5 x")]
    [InlineData("cstrf[3.5,`0.0 X`]", "3.5 X")]
    [InlineData("cstrf[3.5,`0.00`]", "3.50")]
    public void FormattedStringsOnlyGoThroughHexForAWholeHexSpecifier(
        string source,
        string expected)
    {
        Assert.Equal(expected, Evaluate(source).AsString());
    }

    private static ExpressionValue Evaluate(string source)
    {
        var context = new ExpressionEvaluationContext(
            new ExpressionState(),
            CoreExpressionFunctions.CreateDefault(new Random(1234)));
        return ExpressionProgram.Compile(source).Evaluate(context);
    }
}

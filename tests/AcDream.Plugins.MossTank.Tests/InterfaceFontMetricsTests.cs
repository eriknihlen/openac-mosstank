namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The interface font's measure and the word wrap built on it.
/// </summary>
public sealed class InterfaceFontMetricsTests
{
    /// <summary>
    /// The advances are the font's: the captions the monster table's
    /// headings were measured with come out the same. Mutation: shift the
    /// table by one and every width is off.
    /// </summary>
    [Theory]
    [InlineData("F", 6d)]
    [InlineData("WC", 19d)]
    [InlineData("DC", 17d)]
    [InlineData("Cs", 13d)]
    [InlineData("I", 3d)]
    [InlineData("", 0d)]
    [InlineData("é", 0d)]
    public void TheMeasureIsTheFontsAdvance(string text, double width) =>
        Assert.Equal(width, InterfaceFontMetrics.Measure(text));

    /// <summary>
    /// Words move whole to the next line when the next one would not fit,
    /// no line is wider than asked, and the words all survive. Mutation:
    /// compare without the space's width and a line runs one space over.
    /// </summary>
    [Fact]
    public void WordsWrapWholeAndNoLineIsWiderThanAsked()
    {
        const string text = "Draw a name above the creatures and objects around you.";
        IReadOnlyList<string> lines = InterfaceFontMetrics.Wrap(text, 120d);

        Assert.True(lines.Count > 1);
        Assert.All(lines, line => Assert.True(InterfaceFontMetrics.Measure(line) <= 120d, line));
        Assert.Equal(text, string.Join(' ', lines));
        // The first line holds every word that fits, and not the one that does not.
        Assert.True(InterfaceFontMetrics.Measure(lines[0] + " " + lines[1].Split(' ')[0]) > 120d);
    }

    /// <summary>Mutation: never break inside a word and one wider than the label runs off it.</summary>
    [Fact]
    public void AWordWiderThanTheWholeWidthIsBrokenWhereTheWidthRunsOut()
    {
        IReadOnlyList<string> lines = InterfaceFontMetrics.Wrap("ab WWWWWWWWWW cd", 40d);
        Assert.All(lines, line => Assert.True(InterfaceFontMetrics.Measure(line) <= 40d, line));
        Assert.Equal("abWWWWWWWWWWcd", string.Concat(lines).Replace(" ", string.Empty, StringComparison.Ordinal));
        Assert.Empty(InterfaceFontMetrics.Wrap("   ", 40d));
    }
}

using System.Globalization;
using AcDream.Plugins.MossTank;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class UbSettingValueTests
{
    [Fact]
    public void EachShapeKeepsItsOwnKindAndPayload()
    {
        Assert.True(UbSettingValue.FromBool(true).Boolean);
        Assert.Equal(UbSettingKind.Bool, UbSettingValue.FromBool(true).Kind);
        Assert.Equal(7, UbSettingValue.FromInt(7).AsInt32());
        Assert.Equal(1.5f, UbSettingValue.FromSingle(1.5f).AsSingle());
        Assert.Equal(2.25d, UbSettingValue.FromDouble(2.25d).AsDouble());
        Assert.Equal("hello", UbSettingValue.FromText("hello").Text);
        Assert.Equal(3, UbSettingValue.FromChoice(3).AsInt32());
        Assert.Equal(0xFF00FF00u, UbSettingValue.FromColor(0xFF00FF00u).AsColor());
        Assert.Equal(["a", "b"], UbSettingValue.FromCollection(["a", "b"]).Items);
    }

    /// <summary>
    /// Everything this layer writes is read back on someone else's machine,
    /// so no number and no colour may go through the machine's own decimal
    /// separator.
    /// </summary>
    [Theory]
    [InlineData("en-US")]
    [InlineData("sv-SE")]
    [InlineData("de-DE")]
    public void TextIsTheSameInEveryLocale(string culture)
    {
        CultureInfo previous = CultureInfo.CurrentCulture;
        try
        {
            CultureInfo.CurrentCulture = new CultureInfo(culture);
            // 0.15 has no exact binary form, so it also catches a number
            // that is right but printed with its binary expansion; 1.5
            // would have passed either way.
            Assert.Equal("0.15", UbSettingValue.FromSingle(0.15f).ToStorageString());
            Assert.Equal("0.05", UbSettingValue.FromDouble(0.05d).ToStorageString());
            Assert.Equal("#FF00FF00", UbSettingValue.FromColor(0xFF00FF00u).ToStorageString());
            Assert.True(UbSettingValue.TryParse(
                UbSettingKind.Double, "0.05", out UbSettingValue parsed));
            Assert.Equal(0.05d, parsed.AsDouble());
        }
        finally
        {
            CultureInfo.CurrentCulture = previous;
        }
    }

    /// <summary>
    /// What the text says, not only that it says the same thing twice: a
    /// value printed as its binary expansion round-trips perfectly well and
    /// is still wrong on the page, which is how fifteen rows came to show
    /// numbers like 0.150000005960464.
    /// </summary>
    [Fact]
    public void EveryShapeReadsAsTheValueItIsAndRoundTripsThroughThatText()
    {
        (UbSettingValue Value, string Text)[] values =
        [
            (UbSettingValue.FromBool(true), "True"),
            (UbSettingValue.FromBool(false), "False"),
            (UbSettingValue.FromInt(-12), "-12"),
            (UbSettingValue.FromSingle(4.2f), "4.2"),
            (UbSettingValue.FromSingle(0.15f), "0.15"),
            (UbSettingValue.FromSingle(0.1f), "0.1"),
            (UbSettingValue.FromSingle(35f), "35"),
            (UbSettingValue.FromDouble(0.0208333333333333d), "0.0208333333333333"),
            (UbSettingValue.FromDouble(15d), "15"),
            (UbSettingValue.FromText("a name with spaces"), "a name with spaces"),
            (UbSettingValue.FromChoice(2), "2"),
            (UbSettingValue.FromColor(0xFFF4A460u), "#FFF4A460"),
            (UbSettingValue.FromCollection(["one", "two", "three"]), "one\ntwo\nthree"),
            (UbSettingValue.FromCollection([]), ""),
        ];

        foreach ((UbSettingValue value, string text) in values)
        {
            Assert.Equal(text, value.ToStorageString());
            Assert.True(
                UbSettingValue.TryParse(value.Kind, text, out UbSettingValue back),
                $"{value.Kind} '{text}' did not read back.");
            Assert.Equal(value.Kind, back.Kind);
            Assert.Equal(text, back.ToStorageString());
        }
    }

    /// <summary>
    /// Colours arrive as a 32-bit value and are shown the way the rest of the
    /// window writes a colour. A value written as a signed 32-bit number by
    /// an interchange file still has to read back as the same colour.
    /// </summary>
    [Theory]
    [InlineData("#FF00FFFF", 0xFF00FFFFu)]
    [InlineData("FF00FFFF", 0xFF00FFFFu)]
    [InlineData("#00FFFF", 0xFF00FFFFu)]
    [InlineData("-16711681", 0xFF00FFFFu)]
    [InlineData("4278255615", 0xFF00FFFFu)]
    public void AColourReadsFromEverySpellingItIsWrittenIn(string text, uint expected)
    {
        Assert.True(UbSettingValue.TryParse(UbSettingKind.Color, text, out UbSettingValue value));
        Assert.Equal(expected, value.AsColor());
    }

    /// <summary>
    /// A bare number is a decimal, because that is how an interchange file
    /// writes a colour. Read as hex instead, a colour whose alpha byte is
    /// low comes back as something else entirely: white with no alpha is
    /// written 16777215, which is eight characters long and every one of
    /// them a hex digit, and it was read as 0x16777215.
    /// </summary>
    [Theory]
    [InlineData("16777215", 0x00FFFFFFu)]
    [InlineData("123456", 0x0001E240u)]
    [InlineData("#123456", 0xFF123456u)]
    [InlineData("#12345678", 0x12345678u)]
    [InlineData("FF00FFFF", 0xFF00FFFFu)]
    [InlineData("00FFFF", 0xFF00FFFFu)]
    public void ABareNumberIsADecimalAndHexNeedsAHashOrALetter(string text, uint expected)
    {
        Assert.True(UbSettingValue.TryParse(UbSettingKind.Color, text, out UbSettingValue value));
        Assert.Equal(expected, value.AsColor());
    }

    [Fact]
    public void ABadValueIsRefusedRatherThanGuessedAt()
    {
        Assert.False(UbSettingValue.TryParse(UbSettingKind.Int, "seven", out _));
        Assert.False(UbSettingValue.TryParse(UbSettingKind.Color, "#GGGGGG", out _));
        Assert.False(UbSettingValue.TryParse(UbSettingKind.Bool, "maybe", out _));
        Assert.False(UbSettingValue.TryParse(UbSettingKind.Double, "1,5", out _));
    }

    [Fact]
    public void BooleansReadBackFromEitherSpelling()
    {
        Assert.True(UbSettingValue.TryParse(UbSettingKind.Bool, "True", out UbSettingValue yes));
        Assert.True(yes.Boolean);
        Assert.True(UbSettingValue.TryParse(UbSettingKind.Bool, "false", out UbSettingValue no));
        Assert.False(no.Boolean);
    }

    [Fact]
    public void TheValueColumnShowsTheChoicesLabelRatherThanItsNumber()
    {
        var definition = new UbSettingDefinition(
            "Jumper.Direction",
            "Which way the jump is aimed.",
            UbSettingKind.Enum,
            UbSettingValue.FromChoice(1),
            UbSettingScope.Profile)
        {
            Choices = [new VtankEnumValue(0, "Forward"), new VtankEnumValue(1, "Backward")],
        };

        Assert.Equal("Backward", definition.Display(UbSettingValue.FromChoice(1)));
        Assert.Equal("2", definition.Display(UbSettingValue.FromChoice(2)));
        Assert.Equal("True", definition.Display(UbSettingValue.FromBool(true)));
        Assert.Equal(
            "one, two",
            definition.Display(UbSettingValue.FromCollection(["one", "two"])));
    }
}

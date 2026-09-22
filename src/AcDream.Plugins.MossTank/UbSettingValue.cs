using System.Globalization;

namespace AcDream.Plugins.MossTank;

/// <summary>The value shapes a UB setting can hold.</summary>
internal enum UbSettingKind
{
    /// <summary>A switch, flipped in place on its row.</summary>
    Bool,

    /// <summary>A whole number.</summary>
    Int,

    /// <summary>A decimal number kept at single precision.</summary>
    Single,

    /// <summary>A decimal number kept at double precision.</summary>
    Double,

    /// <summary>Free text.</summary>
    String,

    /// <summary>One of a fixed set of choices, cycled in place on its row.</summary>
    Enum,

    /// <summary>A colour, held as a 32-bit alpha-red-green-blue value.</summary>
    Color,

    /// <summary>An editable list of lines, edited on its own sub-page.</summary>
    Collection,
}

/// <summary>
/// One setting's value. The shape travels with the payload, so a row can be
/// shown, typed into and written to a file without asking the catalogue what
/// it was again.
/// </summary>
internal readonly record struct UbSettingValue
{
    private static readonly string[] NoItems = [];

    private UbSettingValue(
        UbSettingKind kind,
        bool boolean,
        double number,
        string? text,
        IReadOnlyList<string>? items)
    {
        Kind = kind;
        Boolean = boolean;
        Number = number;
        Text = text ?? string.Empty;
        Items = items ?? NoItems;
    }

    /// <summary>Which shape this value holds.</summary>
    public UbSettingKind Kind { get; }

    /// <summary>The switch position, for a <see cref="UbSettingKind.Bool"/>.</summary>
    public bool Boolean { get; }

    /// <summary>The numeric payload, for every numeric shape.</summary>
    public double Number { get; }

    /// <summary>The text payload, for <see cref="UbSettingKind.String"/>.</summary>
    public string Text { get; }

    /// <summary>The lines, for <see cref="UbSettingKind.Collection"/>.</summary>
    public IReadOnlyList<string> Items { get; }

    /// <summary>A switch.</summary>
    public static UbSettingValue FromBool(bool value) =>
        new(UbSettingKind.Bool, value, value ? 1d : 0d, null, null);

    /// <summary>A whole number.</summary>
    public static UbSettingValue FromInt(int value) =>
        new(UbSettingKind.Int, false, value, null, null);

    /// <summary>A decimal number at single precision.</summary>
    public static UbSettingValue FromSingle(float value) =>
        new(UbSettingKind.Single, false, value, null, null);

    /// <summary>A decimal number at double precision.</summary>
    public static UbSettingValue FromDouble(double value) =>
        new(UbSettingKind.Double, false, value, null, null);

    /// <summary>Free text.</summary>
    public static UbSettingValue FromText(string value) =>
        new(UbSettingKind.String, false, 0d, value, null);

    /// <summary>One of a fixed set of choices, by its number.</summary>
    public static UbSettingValue FromChoice(int value) =>
        new(UbSettingKind.Enum, false, value, null, null);

    /// <summary>A colour given as 32 bits of alpha, red, green and blue.</summary>
    public static UbSettingValue FromColor(uint argb) =>
        new(UbSettingKind.Color, false, argb, null, null);

    /// <summary>An editable list of lines.</summary>
    public static UbSettingValue FromCollection(IEnumerable<string> items) =>
        new(UbSettingKind.Collection, false, 0d, null, items.ToArray());

    /// <summary>The value as a whole number, rounding a decimal one.</summary>
    public int AsInt32() => (int)Math.Round(Number, MidpointRounding.AwayFromZero);

    /// <summary>The value at single precision.</summary>
    public float AsSingle() => (float)Number;

    /// <summary>The value at double precision.</summary>
    public double AsDouble() => Number;

    /// <summary>The value as 32 bits of alpha, red, green and blue.</summary>
    public uint AsColor() =>
        (uint)(long)Math.Round(Number, MidpointRounding.AwayFromZero);

    /// <summary>
    /// Exactly what goes into the file, and exactly what a person types into
    /// the edit field: the same characters on every machine, whatever that
    /// machine's own idea of a decimal point is.
    /// </summary>
    public string ToStorageString() => Kind switch
    {
        UbSettingKind.Bool => Boolean ? "True" : "False",
        UbSettingKind.Int or UbSettingKind.Enum =>
            AsInt32().ToString(CultureInfo.InvariantCulture),
        // A single-precision number is printed from the float, not from the
        // double it is carried in. Widening 0.15f to a double does not give
        // 0.15: it gives the number the float really holds, and printing
        // that to fifteen digits puts 0.150000005960464 on the page. The
        // float's own shortest form is the one that reads back as the same
        // float.
        UbSettingKind.Single => AsSingle().ToString(CultureInfo.InvariantCulture),
        UbSettingKind.Double =>
            Number.ToString("G15", CultureInfo.InvariantCulture),
        UbSettingKind.String => Text,
        UbSettingKind.Color => $"#{AsColor():X8}",
        UbSettingKind.Collection => string.Join('\n', Items),
        _ => string.Empty,
    };

    /// <summary>
    /// Reads a value of one shape back from its stored text. A value that
    /// does not read is refused rather than guessed at, so a typing slip
    /// leaves the old value alone instead of quietly becoming zero.
    /// </summary>
    public static bool TryParse(UbSettingKind kind, string text, out UbSettingValue value)
    {
        string trimmed = text.Trim();
        switch (kind)
        {
            case UbSettingKind.Bool:
                if (bool.TryParse(trimmed, out bool boolean))
                {
                    value = FromBool(boolean);
                    return true;
                }
                break;
            case UbSettingKind.Int:
                if (int.TryParse(
                    trimmed,
                    NumberStyles.Integer,
                    CultureInfo.InvariantCulture,
                    out int whole))
                {
                    value = FromInt(whole);
                    return true;
                }
                break;
            case UbSettingKind.Enum:
                if (int.TryParse(
                    trimmed,
                    NumberStyles.Integer,
                    CultureInfo.InvariantCulture,
                    out int choice))
                {
                    value = FromChoice(choice);
                    return true;
                }
                break;
            case UbSettingKind.Single:
                if (float.TryParse(
                    trimmed,
                    NumberStyles.Float,
                    CultureInfo.InvariantCulture,
                    out float single))
                {
                    value = FromSingle(single);
                    return true;
                }
                break;
            case UbSettingKind.Double:
                if (double.TryParse(
                    trimmed,
                    NumberStyles.Float,
                    CultureInfo.InvariantCulture,
                    out double real))
                {
                    value = FromDouble(real);
                    return true;
                }
                break;
            case UbSettingKind.String:
                value = FromText(text);
                return true;
            case UbSettingKind.Color:
                if (TryParseColor(trimmed, out uint argb))
                {
                    value = FromColor(argb);
                    return true;
                }
                break;
            case UbSettingKind.Collection:
                value = FromCollection(text.Length == 0
                    ? []
                    : text.Split('\n', StringSplitOptions.RemoveEmptyEntries)
                        .Select(static line => line.Trim('\r'))
                        .ToArray());
                return true;
        }

        value = default;
        return false;
    }

    /// <summary>
    /// A colour is written here the way the rest of the window writes one,
    /// <c>#AARRGGBB</c>. It is also read back from the spellings an
    /// interchange file can carry it in: without the hash, without the alpha
    /// byte, and as a signed or unsigned 32-bit number. Where the two
    /// overlap -- a bare run of digits that is both a decimal number and a
    /// hex colour -- the decimal wins, and hex needs a hash or a letter.
    /// </summary>
    private static bool TryParseColor(string text, out uint argb)
    {
        argb = 0u;
        if (text.Length == 0)
            return false;

        bool hashed = text[0] == '#';
        ReadOnlySpan<char> body = hashed ? text.AsSpan(1) : text.AsSpan();

        // A number with no hash is read as a decimal before it is read as
        // hex, because that is how an interchange file writes a colour and
        // because the two spellings overlap. A colour whose alpha byte is
        // low is six or eight digits long -- white with no alpha is
        // 16777215 -- which is exactly the length of a hex colour and made
        // entirely of hex digits, so trying hex on the length alone read it
        // as 0x16777215. Hex without a hash still reads, but only when the
        // text is not a decimal number.
        if (!hashed)
        {
            if (int.TryParse(
                text, NumberStyles.Integer, CultureInfo.InvariantCulture, out int signed))
            {
                argb = unchecked((uint)signed);
                return true;
            }
            if (uint.TryParse(
                text, NumberStyles.Integer, CultureInfo.InvariantCulture, out uint unsigned))
            {
                argb = unsigned;
                return true;
            }
        }

        if (body.Length == 6 && uint.TryParse(
            body, NumberStyles.HexNumber, CultureInfo.InvariantCulture, out uint rgb))
        {
            argb = 0xFF000000u | rgb;
            return true;
        }
        if (body.Length == 8 && uint.TryParse(
            body, NumberStyles.HexNumber, CultureInfo.InvariantCulture, out uint packed))
        {
            argb = packed;
            return true;
        }
        return false;
    }
}

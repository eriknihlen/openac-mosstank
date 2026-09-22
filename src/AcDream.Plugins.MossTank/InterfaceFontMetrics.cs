namespace AcDream.Plugins.MossTank;

/// <summary>
/// How wide the client draws text in the font its markup labels use, so a
/// line can be wrapped to a label's width before it is bound. A label
/// draws one line and neither wraps nor clips, so the wrapping has to be
/// done by whoever supplies the text.
/// </summary>
/// <remarks>
/// The advances are those of the client's default interface font, one per
/// printable ASCII character, read from the font itself: each is the
/// glyph's width plus its bearings, which is exactly what the client adds
/// up when it measures a string. A character outside that range adds
/// nothing, as the client skips a glyph the font lacks. A line is the
/// font's line height.
/// </remarks>
internal static class InterfaceFontMetrics
{
    /// <summary>The height of one line of the font, in pixels.</summary>
    public const double LineHeight = 16d;

    private static readonly byte[] Advances =
    [
        // space ! " # $ % & ' ( ) * + , - . /
        4, 3, 5, 5, 6, 9, 9, 3, 4, 4, 4, 6, 3, 3, 3, 4,
        // 0 1 2 3 4 5 6 7 8 9 : ; < = > ?
        6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 3, 3, 6, 6, 6, 5,
        // @ A B C D E F G H I J K L M N O
        7, 9, 7, 8, 9, 7, 6, 9, 9, 3, 3, 8, 7, 11, 9, 9,
        // P Q R S T U V W X Y Z [ \ ] ^ _
        7, 9, 7, 7, 7, 8, 9, 11, 7, 7, 7, 4, 7, 4, 6, 6,
        // ` a b c d e f g h i j k l m n o
        4, 5, 6, 6, 6, 6, 3, 7, 6, 3, 3, 6, 3, 9, 6, 7,
        // p q r s t u v w x y z { | } ~
        6, 6, 4, 5, 3, 6, 7, 9, 5, 7, 6, 3, 5, 3, 6,
    ];

    /// <summary>How far the pen moves after one character, in pixels.</summary>
    public static double Advance(char c) =>
        c is >= ' ' and <= '~' ? Advances[c - ' '] : 0d;

    /// <summary>How wide a line of text draws, in pixels.</summary>
    public static double Measure(string text)
    {
        double width = 0d;
        foreach (char c in text)
            width += Advance(c);
        return width;
    }

    /// <summary>
    /// Breaks text into lines no wider than the width, at spaces where it
    /// can; a word wider than the whole width is broken where the width
    /// runs out. Blank text is no lines.
    /// </summary>
    public static IReadOnlyList<string> Wrap(string text, double width)
    {
        var lines = new List<string>();
        var line = new System.Text.StringBuilder();
        double lineWidth = 0d;
        foreach (string word in text.Split(' ', StringSplitOptions.RemoveEmptyEntries))
        {
            double wordWidth = Measure(word);
            double space = line.Length == 0 ? 0d : Advance(' ');
            if (line.Length != 0 && lineWidth + space + wordWidth > width)
            {
                lines.Add(line.ToString());
                line.Clear();
                lineWidth = 0d;
                space = 0d;
            }
            if (wordWidth > width)
            {
                foreach (char c in word)
                {
                    double advance = Advance(c);
                    if (line.Length != 0 && lineWidth + advance > width)
                    {
                        lines.Add(line.ToString());
                        line.Clear();
                        lineWidth = 0d;
                    }
                    line.Append(c);
                    lineWidth += advance;
                }
                continue;
            }
            if (space > 0d)
                line.Append(' ');
            line.Append(word);
            lineWidth += space + wordWidth;
        }
        if (line.Length != 0)
            lines.Add(line.ToString());
        return lines;
    }
}

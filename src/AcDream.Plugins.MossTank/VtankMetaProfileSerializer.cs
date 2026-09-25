using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal static class VtankMetaProfileSerializer
{
    private static readonly string[] Header =
    [
        "1", "CondAct", "5", "CType", "AType", "CData", "AData",
        "State", "n", "n", "n", "n", "n",
    ];

    private static readonly string[] TablePrefix = ["TABLE", "2", "k", "v", "n", "n"];
    private static readonly string[] RecursiveTablePrefix = ["TABLE", "2", "K", "V", "n", "n"];
    private const int MaximumRules = 100_000;
    private const int MaximumNesting = 256;

    /// <summary>
    /// The most characters a serialized blob (an embedded route, a view's
    /// markup) may state. It is a length, not a number of things: a long
    /// route runs past the rule bound on its own (1,742 nodes is some
    /// 111,000 characters), so it gets a bound of its own that no real
    /// file comes near.
    /// </summary>
    private const int MaximumSerializedCharacters = 64 * 1024 * 1024;

    public static bool TryLoad(string source, out MetaProfile profile, out string error) =>
        TryLoad(source, MetafSerializer.NoOpSpells.Instance, out profile, out error);

    public static bool TryLoad(
        string source,
        ISpellCatalog spells,
        out MetaProfile profile,
        out string error)
    {
        ArgumentNullException.ThrowIfNull(spells);
        try
        {
            var reader = new LineReader(source);
            reader.Expect(Header);
            int count = reader.ReadInt();
            if (count is < 0 or > MaximumRules)
                throw reader.Error("Invalid VTank Meta rule count.");

            var parsed = new MetaProfile();
            for (int index = 0; index < count; index++)
            {
                reader.Expect("i");
                int conditionType = reader.ReadInt();
                reader.Expect("i");
                int actionType = reader.ReadInt();
                MetaCondition condition = ReadCondition(reader, conditionType, 0);
                MetaAction action = ReadAction(reader, actionType, 0, spells);
                reader.Expect("s");
                parsed.Rules.Add(new MetaRule
                {
                    State = reader.Read(),
                    Condition = condition,
                    Action = action,
                    Enabled = true,
                });
            }
            reader.ExpectEnd();
            profile = parsed;
            error = string.Empty;
            return true;
        }
        catch (Exception exception) when (exception is FormatException
            or OverflowException or ArgumentOutOfRangeException)
        {
            profile = new MetaProfile();
            error = exception.Message;
            return false;
        }
    }

    /// <summary>
    /// The meta in the older ".met" form, line for line as that form's own
    /// writer lays it out, so a meta loaded from a ".met" saves back to the
    /// same bytes and the tool that owns the form reads what is written.
    /// </summary>
    /// <exception cref="InvalidOperationException">
    /// The meta holds something the form has no way to say: a rule switched
    /// off, a condition only this plugin knows, a whole-number field holding
    /// a fraction. Nothing is written in that case rather than a file that
    /// quietly lost it.
    /// </exception>
    public static string Save(MetaProfile profile)
    {
        ArgumentNullException.ThrowIfNull(profile);
        int disabled = profile.Rules.Count(static rule => !rule.Enabled);
        if (disabled > 0)
        {
            throw new InvalidOperationException(
                $"{disabled} rule(s) are switched off, and a .met file has no switched-off rule.");
        }
        var writer = new LineWriter();
        foreach (string line in Header)
            writer.Add(line);
        writer.Add(FormatInt(profile.Rules.Count));
        foreach (MetaRule rule in profile.Rules)
        {
            writer.Add("i");
            writer.Add(FormatInt(ConditionType(rule.Condition.Kind)));
            writer.Add("i");
            writer.Add(FormatInt(ActionType(rule.Action.Kind)));
            WriteCondition(writer, rule.Condition, 0);
            WriteAction(writer, rule.Action, 0);
            writer.Add("s");
            writer.Add(SingleLine(rule.State, "state name"));
        }
        return writer.ToString();
    }

    private static void WriteCondition(LineWriter writer, MetaCondition value, int depth)
    {
        if (depth > MaximumNesting)
            throw new InvalidOperationException("The meta's conditions nest too deep for a .met file.");
        int type = ConditionType(value.Kind);
        switch (type)
        {
            case 0 or 1 or 7 or 8 or 9 or 10 or 15 or 19 or 20:
                writer.Add("i", "0");
                break;
            case 2 or 3:
                writer.Add(RecursiveTablePrefix);
                writer.Add(FormatInt(value.Children.Count));
                foreach (MetaCondition child in value.Children)
                {
                    writer.Add("i", FormatInt(ConditionType(child.Kind)));
                    WriteCondition(writer, child, depth + 1);
                }
                break;
            case 4:
                writer.Add("s", SingleLine(value.Text, "chat pattern"));
                break;
            case 5 or 6 or 17 or 18 or 22 or 24:
                writer.Add("i", FormatWhole(value.Number));
                break;
            case 11 or 12:
                writer.Add(TablePrefix);
                writer.Add("2", "s", "n", "s", SingleLine(value.Text, "item name"));
                writer.Add("s", "c", "i", FormatWhole(value.Number));
                break;
            case 13:
                writer.Add(TablePrefix);
                writer.Add("3", "s", "n", "s", SingleLine(value.Text, "monster name"));
                writer.Add("s", "c", "i", FormatWhole(value.Number));
                writer.Add("s", "r", "d", FormatDouble(value.SecondaryNumber));
                break;
            case 14:
                writer.Add(TablePrefix);
                writer.Add("3", "s", "p", "i", FormatWhole(value.TertiaryNumber));
                writer.Add("s", "c", "i", FormatWhole(value.Number));
                writer.Add("s", "r", "d", FormatDouble(value.SecondaryNumber));
                break;
            case 16:
                writer.Add(TablePrefix);
                writer.Add("1", "s", "r", "d", FormatDouble(value.Number));
                break;
            case 21:
                if (value.Children.Count != 1)
                    throw new InvalidOperationException("A .met Not holds exactly one condition.");
                writer.Add(RecursiveTablePrefix);
                writer.Add("1", "i", FormatInt(ConditionType(value.Children[0].Kind)));
                WriteCondition(writer, value.Children[0], depth + 1);
                break;
            case 23:
                writer.Add(TablePrefix);
                writer.Add("2", "s", "sid", "i", FormatWhole(value.Number));
                writer.Add("s", "sec", "i", FormatWhole(value.SecondaryNumber));
                break;
            case 25:
                writer.Add(TablePrefix);
                writer.Add("1", "s", "dist", "d", FormatDouble(value.Number));
                break;
            case 26:
                writer.Add(TablePrefix);
                writer.Add("1", "s", "e", "s", SingleLine(value.Text, "expression"));
                break;
            case 28:
                writer.Add(TablePrefix);
                writer.Add("2", "s", "p", "s", SingleLine(value.Text, "chat pattern"));
                writer.Add("s", "c", "s", SingleLine(value.SecondaryText, "colour list"));
                break;
        }
    }

    private static void WriteAction(LineWriter writer, MetaAction value, int depth)
    {
        if (depth > MaximumNesting)
            throw new InvalidOperationException("The meta's actions nest too deep for a .met file.");
        int type = ActionType(value.Kind);
        switch (type)
        {
            case 0 or 6:
                writer.Add("i", "0");
                break;
            case 1:
                writer.Add("s", SingleLine(value.Text, "state name"));
                break;
            case 2:
                writer.Add("s", SingleLine(value.Text, "chat command"));
                break;
            case 3:
                writer.Add(RecursiveTablePrefix);
                writer.Add(FormatInt(value.Children.Count));
                foreach (MetaAction child in value.Children)
                {
                    writer.Add("i", FormatInt(ActionType(child.Kind)));
                    WriteAction(writer, child, depth + 1);
                }
                break;
            case 4:
                WriteEmbeddedNavigation(writer, value);
                break;
            case 5:
                writer.Add(TablePrefix);
                writer.Add("2", "s", "st", "s", SingleLine(value.Text, "state name"));
                writer.Add("s", "ret", "s", SingleLine(value.SecondaryText, "state name"));
                break;
            case 7 or 8:
                writer.Add(TablePrefix);
                writer.Add("1", "s", "e", "s", SingleLine(value.Text, "expression"));
                break;
            case 9:
                writer.Add(TablePrefix);
                writer.Add("3", "s", "s", "s", SingleLine(value.Text, "state name"));
                writer.Add("s", "r", "d", FormatDouble(value.Number));
                writer.Add("s", "t", "d", FormatDouble(value.SecondaryNumber));
                break;
            case 10 or 15:
                writer.Add(TablePrefix);
                writer.Add("0");
                break;
            case 11 or 12:
                writer.Add(TablePrefix);
                writer.Add("2", "s", "o", "s", SingleLine(value.Text, "option name"));
                writer.Add("s", "v", "s", SingleLine(value.SecondaryText, "option value"));
                break;
            case 13:
                writer.Add(TablePrefix);
                writer.Add("2", "s", "n", "s", SingleLine(value.Text, "view name"));
                writer.Add("s", "x", "ba", FormatInt(value.SecondaryText.Length));
                // The form writes a view's markup and runs straight on into
                // whatever follows it, with no line break in between.
                writer.AddUnterminated(value.SecondaryText);
                break;
            case 14:
                writer.Add(TablePrefix);
                writer.Add("1", "s", "n", "s", SingleLine(value.Text, "view name"));
                break;
        }
    }

    /// <summary>
    /// A route carried in the meta: its name, its node count and the route
    /// in the ".nav" form, led by the characters all of that takes, every
    /// line break counted as two.
    /// </summary>
    private static void WriteEmbeddedNavigation(LineWriter writer, MetaAction value)
    {
        writer.Add("ba");
        var lines = new List<string> { SingleLine(value.SecondaryText, "route name") };
        if (value.EmbeddedRoute is { } route)
        {
            lines.Add(FormatInt(route.Mode == RouteMode.Target ? 1 : route.Waypoints.Count));
            VtankNavRouteSerializer.WriteRoute(lines, route);
        }
        else
        {
            lines.Add("0");
        }
        writer.Add(FormatInt(lines.Sum(static line => line.Length + 2)));
        foreach (string line in lines)
            writer.Add(line);
    }

    private static int ConditionType(MetaConditionKind kind) => kind switch
    {
        MetaConditionKind.Never => 0,
        MetaConditionKind.Always => 1,
        MetaConditionKind.All => 2,
        MetaConditionKind.Any => 3,
        MetaConditionKind.ChatMessage => 4,
        MetaConditionKind.PackSlotsLessThanOrEqual => 5,
        MetaConditionKind.SecondsInStateGreaterThanOrEqual => 6,
        MetaConditionKind.NavigationRouteEmpty => 7,
        MetaConditionKind.CharacterDeath => 8,
        MetaConditionKind.AnyVendorOpen => 9,
        MetaConditionKind.VendorClosed => 10,
        MetaConditionKind.InventoryItemCountLessThanOrEqual => 11,
        MetaConditionKind.InventoryItemCountGreaterThanOrEqual => 12,
        MetaConditionKind.MonsterNameCountWithinDistance => 13,
        MetaConditionKind.MonsterPriorityCountWithinDistance => 14,
        MetaConditionKind.NeedToBuff => 15,
        MetaConditionKind.NoMonstersWithinDistance => 16,
        MetaConditionKind.LandblockEquals => 17,
        MetaConditionKind.LandcellEquals => 18,
        MetaConditionKind.PortalspaceEntered => 19,
        MetaConditionKind.PortalspaceExited => 20,
        MetaConditionKind.Not => 21,
        MetaConditionKind.PersistentSecondsInStateGreaterThanOrEqual => 22,
        MetaConditionKind.TimeLeftOnSpellGreaterThanOrEqual => 23,
        MetaConditionKind.BurdenPercentGreaterThanOrEqual => 24,
        MetaConditionKind.DistanceFromAnyRoutePointGreaterThanOrEqual => 25,
        MetaConditionKind.Expression => 26,
        MetaConditionKind.ChatMessageCapture => 28,
        _ => throw new InvalidOperationException(
            $"A .met file has no condition {kind}; save this meta as .af instead."),
    };

    private static int ActionType(MetaActionKind kind) => kind switch
    {
        MetaActionKind.None => 0,
        MetaActionKind.SetMetaState => 1,
        MetaActionKind.ChatCommand => 2,
        MetaActionKind.All => 3,
        MetaActionKind.LoadEmbeddedNavigationRoute => 4,
        MetaActionKind.CallMetaState => 5,
        MetaActionKind.ReturnFromCall => 6,
        MetaActionKind.ExpressionAction => 7,
        MetaActionKind.ChatExpression => 8,
        MetaActionKind.SetWatchdog => 9,
        MetaActionKind.ClearWatchdog => 10,
        MetaActionKind.GetVtankOption => 11,
        MetaActionKind.SetVtankOption => 12,
        MetaActionKind.CreateView => 13,
        MetaActionKind.DestroyView => 14,
        MetaActionKind.DestroyAllViews => 15,
        _ => throw new InvalidOperationException(
            $"A .met file has no action {kind}; save this meta as .af instead."),
    };

    private static string FormatInt(int value) =>
        value.ToString(CultureInfo.InvariantCulture);

    /// <summary>A whole-number field, refused when it holds a fraction.</summary>
    private static string FormatWhole(double value)
    {
        if (!double.IsFinite(value)
            || value != Math.Truncate(value)
            || value is < int.MinValue or > int.MaxValue)
        {
            throw new InvalidOperationException(
                $"A .met file holds only whole numbers in this field, not {value.ToString(CultureInfo.InvariantCulture)}.");
        }
        return FormatInt((int)value);
    }

    private static string FormatDouble(double value) =>
        VtankNavRouteSerializer.FormatDouble(value);

    private static string SingleLine(string value, string what)
    {
        string text = value ?? string.Empty;
        if (text.Contains('\n', StringComparison.Ordinal)
            || text.Contains('\r', StringComparison.Ordinal))
        {
            throw new InvalidOperationException(
                $"A .met file cannot hold a {what} that runs over more than one line.");
        }
        return text;
    }

    /// <summary>
    /// The form's lines, each ended by a line break except where a view's
    /// markup runs on into the next line.
    /// </summary>
    private sealed class LineWriter
    {
        private readonly System.Text.StringBuilder _text = new();

        public void Add(params string[] lines)
        {
            foreach (string line in lines)
                _text.Append(line).Append("\r\n");
        }

        public void AddUnterminated(string text) => _text.Append(text);

        public override string ToString() => _text.ToString();
    }

    private static MetaCondition ReadCondition(LineReader reader, int type, int depth)
    {
        CheckDepth(reader, depth);
        var value = new MetaCondition { Kind = ConditionKind(type) };
        switch (type)
        {
            case 0 or 1 or 7 or 8 or 9 or 10 or 15 or 19 or 20:
                reader.Expect("i", "0");
                break;
            case 2 or 3:
                reader.Expect(RecursiveTablePrefix);
                ReadConditions(reader, value, reader.ReadCount(), depth);
                break;
            case 4:
                reader.Expect("s");
                value.Text = reader.Read();
                break;
            case 5 or 6 or 17 or 18 or 22 or 24:
                reader.Expect("i");
                value.Number = reader.ReadInt();
                break;
            case 11 or 12:
                reader.Expect(TablePrefix, "2", "s", "n", "s");
                value.Text = reader.Read();
                reader.Expect("s", "c", "i");
                value.Number = reader.ReadInt();
                break;
            case 13:
                reader.Expect(TablePrefix, "3", "s", "n", "s");
                value.Text = reader.Read();
                reader.Expect("s", "c", "i");
                value.Number = reader.ReadInt();
                reader.Expect("s", "r", "d");
                value.SecondaryNumber = reader.ReadDouble();
                break;
            case 14:
                reader.Expect(TablePrefix, "3", "s", "p", "i");
                value.TertiaryNumber = reader.ReadInt();
                reader.Expect("s", "c", "i");
                value.Number = reader.ReadInt();
                reader.Expect("s", "r", "d");
                value.SecondaryNumber = reader.ReadDouble();
                break;
            case 16:
                reader.Expect(TablePrefix, "1", "s", "r", "d");
                value.Number = reader.ReadDouble();
                break;
            case 21:
                reader.Expect(RecursiveTablePrefix);
                if (reader.ReadCount() != 1)
                    throw reader.Error("VTank Meta Not requires exactly one condition.");
                reader.Expect("i");
                value.Children.Add(ReadCondition(reader, reader.ReadInt(), depth + 1));
                break;
            case 23:
                reader.Expect(TablePrefix, "2", "s", "sid", "i");
                value.Number = reader.ReadInt();
                reader.Expect("s", "sec", "i");
                value.SecondaryNumber = reader.ReadInt();
                break;
            case 25:
                reader.Expect(TablePrefix, "1", "s", "dist", "d");
                value.Number = reader.ReadDouble();
                break;
            case 26:
                reader.Expect(TablePrefix, "1", "s", "e", "s");
                value.Text = reader.Read();
                break;
            case 28:
                reader.Expect(TablePrefix, "2", "s", "p", "s");
                value.Text = reader.Read();
                reader.Expect("s", "c", "s");
                value.SecondaryText = reader.Read();
                break;
            default:
                throw reader.Error($"Unknown VTank Meta condition type {type}.");
        }
        return value;
    }

    private static void ReadConditions(
        LineReader reader,
        MetaCondition target,
        int count,
        int depth)
    {
        for (int index = 0; index < count; index++)
        {
            reader.Expect("i");
            target.Children.Add(ReadCondition(reader, reader.ReadInt(), depth + 1));
        }
    }

    private static MetaAction ReadAction(LineReader reader, int type, int depth, ISpellCatalog spells)
    {
        CheckDepth(reader, depth);
        var value = new MetaAction { Kind = ActionKind(type) };
        switch (type)
        {
            case 0 or 6:
                reader.Expect("i", "0");
                break;
            case 1 or 2:
                reader.Expect("s");
                value.Text = reader.Read();
                break;
            case 3:
                reader.Expect(RecursiveTablePrefix);
                int count = reader.ReadCount();
                for (int index = 0; index < count; index++)
                {
                    reader.Expect("i");
                    value.Children.Add(ReadAction(reader, reader.ReadInt(), depth + 1, spells));
                }
                break;
            case 4:
                ReadEmbeddedNavigation(reader, value, spells);
                break;
            case 5:
                reader.Expect(TablePrefix, "2", "s", "st", "s");
                value.Text = reader.Read();
                reader.Expect("s", "ret", "s");
                value.SecondaryText = reader.Read();
                break;
            case 7 or 8:
                reader.Expect(TablePrefix, "1", "s", "e", "s");
                value.Text = reader.Read();
                break;
            case 9:
                reader.Expect(TablePrefix, "3", "s", "s", "s");
                value.Text = reader.Read();
                reader.Expect("s", "r", "d");
                value.Number = reader.ReadDouble();
                reader.Expect("s", "t", "d");
                value.SecondaryNumber = reader.ReadDouble();
                break;
            case 10 or 15:
                reader.Expect(TablePrefix, "0");
                break;
            case 11:
                reader.Expect(TablePrefix, "2", "s", "o", "s");
                value.Text = reader.Read();
                reader.Expect("s", "v", "s");
                value.SecondaryText = reader.Read();
                break;
            case 12:
                reader.Expect(TablePrefix, "2", "s", "o", "s");
                value.Text = reader.Read();
                reader.Expect("s", "v", "s");
                value.SecondaryText = reader.Read();
                break;
            case 13:
                reader.Expect(TablePrefix, "2", "s", "n", "s");
                value.Text = reader.Read();
                reader.Expect("s", "x", "ba");
                int length = reader.ReadLength();
                value.SecondaryText = reader.ReadByteArray(length);
                break;
            case 14:
                reader.Expect(TablePrefix, "1", "s", "n", "s");
                value.Text = reader.Read();
                break;
            default:
                throw reader.Error($"Unknown VTank Meta action type {type}.");
        }
        return value;
    }

    private static void ReadEmbeddedNavigation(LineReader reader, MetaAction target, ISpellCatalog spells)
    {
        reader.Expect("ba");
        int serializedCharacters = reader.ReadLength();
        target.SecondaryText = reader.Read();
        int statedNodeCount = reader.ReadCount();
        if (serializedCharacters <= 5)
        {
            target.EmbeddedRoute = new NavigationSettings();
            return;
        }

        var lines = new List<string> { reader.ReadExpected("uTank2 NAV 1.2") };
        int mode = reader.ReadInt(out string modeLine);
        lines.Add(modeLine);
        int actualNodeCount;
        if (mode == 3)
        {
            lines.Add(reader.Read());
            lines.Add(reader.Read());
            actualNodeCount = 1;
        }
        else if (mode is 1 or 2 or 4)
        {
            actualNodeCount = reader.ReadCount(out string countLine);
            lines.Add(countLine);
            for (int index = 0; index < actualNodeCount; index++)
                ReadNavigationNode(reader, lines);
        }
        else
        {
            throw reader.Error($"Unknown embedded VTank navigation type {mode}.");
        }
        if (actualNodeCount != statedNodeCount)
            throw reader.Error("Embedded VTank navigation node counts do not match.");

        string blob = string.Join("\r\n", lines) + "\r\n";
        var route = new NavigationSettings();
        if (!VtankNavRouteSerializer.TryLoad(blob, route, spells, out string navError))
            throw reader.Error($"Embedded VTank navigation route is invalid: {navError}");
        target.EmbeddedRoute = route;
    }

    private static void ReadNavigationNode(LineReader reader, List<string> lines)
    {
        int type = reader.ReadInt(out string typeLine);
        lines.Add(typeLine);
        for (int index = 0; index < 4; index++)
            lines.Add(reader.Read());
        int extra = type switch
        {
            0 or 8 => 0,
            1 or 2 or 3 or 4 => 1,
            5 => 2,
            6 or 7 => 6,
            9 => 3,
            _ => throw reader.Error($"Unknown embedded VTank waypoint type {type}."),
        };
        for (int index = 0; index < extra; index++)
            lines.Add(reader.Read());
    }

    private static string NormalizeNewlines(string value) => value
        .Replace("\r\n", "\n", StringComparison.Ordinal)
        .Replace('\r', '\n');

    private static MetaConditionKind ConditionKind(int type) => type switch
    {
        0 => MetaConditionKind.Never,
        1 => MetaConditionKind.Always,
        2 => MetaConditionKind.All,
        3 => MetaConditionKind.Any,
        4 => MetaConditionKind.ChatMessage,
        5 => MetaConditionKind.PackSlotsLessThanOrEqual,
        6 => MetaConditionKind.SecondsInStateGreaterThanOrEqual,
        7 => MetaConditionKind.NavigationRouteEmpty,
        8 => MetaConditionKind.CharacterDeath,
        9 => MetaConditionKind.AnyVendorOpen,
        10 => MetaConditionKind.VendorClosed,
        11 => MetaConditionKind.InventoryItemCountLessThanOrEqual,
        12 => MetaConditionKind.InventoryItemCountGreaterThanOrEqual,
        13 => MetaConditionKind.MonsterNameCountWithinDistance,
        14 => MetaConditionKind.MonsterPriorityCountWithinDistance,
        15 => MetaConditionKind.NeedToBuff,
        16 => MetaConditionKind.NoMonstersWithinDistance,
        17 => MetaConditionKind.LandblockEquals,
        18 => MetaConditionKind.LandcellEquals,
        19 => MetaConditionKind.PortalspaceEntered,
        20 => MetaConditionKind.PortalspaceExited,
        21 => MetaConditionKind.Not,
        22 => MetaConditionKind.PersistentSecondsInStateGreaterThanOrEqual,
        23 => MetaConditionKind.TimeLeftOnSpellGreaterThanOrEqual,
        24 => MetaConditionKind.BurdenPercentGreaterThanOrEqual,
        25 => MetaConditionKind.DistanceFromAnyRoutePointGreaterThanOrEqual,
        26 => MetaConditionKind.Expression,
        28 => MetaConditionKind.ChatMessageCapture,
        _ => throw new FormatException($"Unknown VTank Meta condition type {type}."),
    };

    private static MetaActionKind ActionKind(int type) => type switch
    {
        0 => MetaActionKind.None,
        1 => MetaActionKind.SetMetaState,
        2 => MetaActionKind.ChatCommand,
        3 => MetaActionKind.All,
        4 => MetaActionKind.LoadEmbeddedNavigationRoute,
        5 => MetaActionKind.CallMetaState,
        6 => MetaActionKind.ReturnFromCall,
        7 => MetaActionKind.ExpressionAction,
        8 => MetaActionKind.ChatExpression,
        9 => MetaActionKind.SetWatchdog,
        10 => MetaActionKind.ClearWatchdog,
        11 => MetaActionKind.GetVtankOption,
        12 => MetaActionKind.SetVtankOption,
        13 => MetaActionKind.CreateView,
        14 => MetaActionKind.DestroyView,
        15 => MetaActionKind.DestroyAllViews,
        _ => throw new FormatException($"Unknown VTank Meta action type {type}."),
    };

    private static void CheckDepth(LineReader reader, int depth)
    {
        if (depth > MaximumNesting)
            throw reader.Error("VTank Meta nesting is too deep.");
    }

    private sealed class LineReader
    {
        private readonly List<string> _lines;
        private int _index;

        public LineReader(string source)
        {
            string normalized = NormalizeNewlines(source ?? string.Empty);
            _lines = normalized.Split('\n', StringSplitOptions.None).ToList();
            if (_lines.Count != 0 && _lines[^1].Length == 0)
                _lines.RemoveAt(_lines.Count - 1);
        }

        public string Read()
        {
            if (_index >= _lines.Count)
                throw Error("Unexpected end of VTank Meta data.");
            return _lines[_index++];
        }

        public string ReadExpected(string expected)
        {
            string actual = Read();
            if (!actual.Equals(expected, StringComparison.Ordinal))
                throw Error($"Expected '{expected}', found '{actual}'.");
            return actual;
        }

        public void Expect(params string[] expected)
        {
            foreach (string value in expected)
                ReadExpected(value);
        }

        public void Expect(string[] first, params string[] rest)
        {
            Expect(first);
            Expect(rest);
        }

        public int ReadInt() => int.Parse(
            Read(), NumberStyles.Integer, CultureInfo.InvariantCulture);

        public int ReadInt(out string line)
        {
            line = Read();
            return int.Parse(line, NumberStyles.Integer, CultureInfo.InvariantCulture);
        }

        public int ReadCount()
        {
            int value = ReadInt();
            if (value is < 0 or > MaximumRules)
                throw Error("Invalid VTank Meta collection count.");
            return value;
        }

        /// <summary>A serialized blob's stated character count.</summary>
        public int ReadLength()
        {
            int value = ReadInt();
            if (value is < 0 or > MaximumSerializedCharacters)
                throw Error("Invalid VTank Meta serialized length.");
            return value;
        }

        public int ReadCount(out string line)
        {
            int value = ReadInt(out line);
            if (value is < 0 or > MaximumRules)
                throw Error("Invalid VTank Meta collection count.");
            return value;
        }

        public double ReadDouble() => double.Parse(
            Read(), NumberStyles.Float, CultureInfo.InvariantCulture);

        public string ReadByteArray(int length)
        {
            if (length < 0)
                throw Error("Invalid VTank Meta byte-array length.");
            string first = Read();
            if (first.Length >= length)
            {
                string value = first[..length];
                string remainder = first[length..];
                if (remainder.Length != 0)
                    _lines.Insert(_index, remainder);
                return value;
            }

            var valueBuilder = new System.Text.StringBuilder(first);
            while (valueBuilder.Length < length && _index < _lines.Count)
            {
                valueBuilder.Append("\r\n");
                valueBuilder.Append(Read());
            }
            if (valueBuilder.Length < length)
                throw Error("Truncated VTank Meta byte array.");
            string combined = valueBuilder.ToString();
            string result = combined[..length];
            string remaining = combined[length..];
            if (remaining.Length != 0)
                _lines.Insert(_index, remaining.TrimStart('\r', '\n'));
            return result;
        }

        public void ExpectEnd()
        {
            while (_index < _lines.Count && _lines[_index].Length == 0)
                _index++;
            if (_index != _lines.Count)
                throw Error($"Unexpected trailing VTank Meta data '{_lines[_index]}'.");
        }

        public FormatException Error(string message) =>
            new($"VTank Meta line {Math.Min(_index + 1, _lines.Count + 1)}: {message}");
    }
}

using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;

namespace AcDream.Plugins.MossTank.Expressions;

internal sealed class ExpressionProgram
{
    private readonly Node[] _statements;

    private ExpressionProgram(Node[] statements, IReadOnlyList<string> repairs, string text)
    {
        _statements = statements;
        Repairs = repairs;
        Text = text;
    }

    /// <summary>
    /// The expression as the reference's parser gives it back when it names
    /// it in an error: every token it read, as written, with nothing between
    /// them (so no spaces outside a token), a "&lt;missing ']'&gt;" where it
    /// supplied a closer, and "&lt;EOF&gt;" at the end.
    /// </summary>
    public string Text { get; }

    /// <summary>
    /// What the reader dropped, skipped or supplied to make sense of the text,
    /// the way the reference parser does it without a word; empty when the
    /// text was read as written.
    /// </summary>
    public IReadOnlyList<string> Repairs { get; }

    public static ExpressionProgram Compile(string source)
    {
        if (string.IsNullOrWhiteSpace(source))
            throw new ExpressionParseException("Expression is empty", 0);
        var repairs = new List<string>();
        var parser = new Parser(source, repairs);
        Node[] statements = parser.ParseProgram();
        return new ExpressionProgram(statements, repairs, parser.Text);
    }

    /// <summary>
    /// Runs the expression. Any error while it runs is an
    /// <see cref="ExpressionEvaluationException"/>: one the runtime raises
    /// itself (a number too large for a 32-bit operator, a pattern that does
    /// not parse or takes too long) carries the runtime's message, as every
    /// error ends the reference's run whatever raised it. Only cancellation
    /// passes through as it is.
    /// </summary>
    public ExpressionValue Evaluate(ExpressionEvaluationContext context)
    {
        ArgumentNullException.ThrowIfNull(context);
        ExpressionValue result = ExpressionValue.Zero;
        try
        {
            foreach (Node statement in _statements)
                result = statement.Evaluate(context);
        }
        catch (Exception error) when (error is not (
            ExpressionEvaluationException
            or ExpressionParseException
            or OperationCanceledException))
        {
            throw new ExpressionEvaluationException(error);
        }
        return result;
    }

    private abstract class Node(int offset)
    {
        protected int Offset { get; } = offset;
        /// <summary>
        /// What the node evaluates to, with a true/false turned into the
        /// number 1 or 0: the reference does that to every value an
        /// expression produces (a literal, an operator's answer, a
        /// function's answer, a variable read), so a boolean never reaches
        /// the expression. A true/false inside a list a function built stays
        /// as it is until it is read out of the list.
        /// </summary>
        internal ExpressionValue Evaluate(ExpressionEvaluationContext context)
        {
            ExpressionValue value = EvaluateCore(context);
            return value.Kind == ExpressionValueKind.Boolean
                ? ExpressionValue.Number(value.IsTruthy ? 1d : 0d)
                : value;
        }

        protected abstract ExpressionValue EvaluateCore(ExpressionEvaluationContext context);
    }

    private sealed class LiteralNode(ExpressionValue value, int offset) : Node(offset)
    {
        protected override ExpressionValue EvaluateCore(ExpressionEvaluationContext context)
        {
            context.Step(Offset);
            return value;
        }
    }

    private sealed class VariableNode(
        ExpressionVariableScope scope,
        Node name,
        int offset) : Node(offset)
    {
        public ExpressionVariableScope Scope { get; } = scope;

        protected override ExpressionValue EvaluateCore(ExpressionEvaluationContext context)
        {
            context.Step(Offset);
            return context.State.Get(Scope, ResolveName(context));
        }

        public ExpressionValue Set(
            ExpressionEvaluationContext context,
            ExpressionValue value) =>
            context.State.Set(Scope, ResolveName(context), value);

        private string ResolveName(ExpressionEvaluationContext context) =>
            name.Evaluate(context).ToDisplayString();
    }

    private sealed class AssignmentNode(
        VariableNode variable,
        Node value,
        int offset) : Node(offset)
    {
        protected override ExpressionValue EvaluateCore(ExpressionEvaluationContext context)
        {
            context.Step(Offset);
            ExpressionValue result = value.Evaluate(context);
            return variable.Set(context, result);
        }
    }

    private sealed class FunctionNode(
        string name,
        Node[] arguments,
        int offset) : Node(offset)
    {
        protected override ExpressionValue EvaluateCore(ExpressionEvaluationContext context)
        {
            var values = new ExpressionValue[arguments.Length];
            for (int index = 0; index < arguments.Length; index++)
                values[index] = arguments[index].Evaluate(context);
            return context.Invoke(name, values, Offset);
        }
    }

    private sealed class UnaryNode(TokenKind operation, Node operand, int offset)
        : Node(offset)
    {
        protected override ExpressionValue EvaluateCore(ExpressionEvaluationContext context)
        {
            context.Step(Offset);
            ExpressionValue value = operand.Evaluate(context);
            return operation switch
            {
                TokenKind.Minus => ExpressionValue.Number(
                    -value.AsNumber("unary '-'")),
                TokenKind.Tilde => ExpressionValue.Number(
                    ~value.AsInt32("bitwise complement")),
                _ => throw new ExpressionEvaluationException(
                    $"Unsupported unary operator {operation}", Offset),
            };
        }
    }

    /// <param name="source">
    /// For a subtraction, the text it was read from as the reference's parser
    /// gives it back (see <see cref="ExpressionProgram.Text"/>); null otherwise.
    /// </param>
    private sealed class BinaryNode(
        TokenKind operation,
        Node left,
        Node right,
        int offset,
        string? source = null) : Node(offset)
    {
        protected override ExpressionValue EvaluateCore(ExpressionEvaluationContext context)
        {
            context.Step(Offset);
            ExpressionValue lhs = left.Evaluate(context);
            if (operation == TokenKind.AndAnd)
            {
                // Either side false answers false (0), as the reference
                // answers it; only a true right side is handed back itself.
                if (!lhs.IsTruthy)
                    return ExpressionValue.Zero;
                ExpressionValue last = right.Evaluate(context);
                return last.IsTruthy ? last : ExpressionValue.Zero;
            }
            if (operation == TokenKind.OrOr)
                return lhs.IsTruthy ? lhs : right.Evaluate(context);

            ExpressionValue rhs = right.Evaluate(context);
            return operation switch
            {
                TokenKind.Plus => Add(lhs, rhs),
                TokenKind.Minus => Subtract(lhs, rhs, source),
                TokenKind.Star => ExpressionValue.Number(
                    lhs.AsNumber("multiplication") * rhs.AsNumber("multiplication")),
                TokenKind.Slash => ExpressionValue.Number(
                    lhs.AsNumber("division") / rhs.AsNumber("division")),
                TokenKind.Percent => ExpressionValue.Number(
                    lhs.AsNumber("modulo") % rhs.AsNumber("modulo")),
                TokenKind.Caret => ExpressionValue.Number(
                    lhs.AsInt32("bitwise xor") ^ rhs.AsInt32("bitwise xor")),
                TokenKind.ShiftLeft => ExpressionValue.Number(
                    lhs.AsInt32("left shift") << rhs.AsInt32("left shift")),
                TokenKind.ShiftRight => ExpressionValue.Number(
                    lhs.AsInt32("right shift") >> rhs.AsInt32("right shift")),
                TokenKind.Ampersand => ExpressionValue.Number(
                    lhs.AsInt32("bitwise and") & rhs.AsInt32("bitwise and")),
                TokenKind.Pipe => ExpressionValue.Number(
                    lhs.AsInt32("bitwise or") | rhs.AsInt32("bitwise or")),
                TokenKind.Hash => RegexMatch(context, lhs, rhs),
                TokenKind.EqualEqual => ExpressionValue.Boolean(AreEqual(lhs, rhs)),
                TokenKind.BangEqual => ExpressionValue.Boolean(!AreEqual(lhs, rhs)),
                TokenKind.Less => Compare(lhs, rhs, static comparison => comparison < 0),
                TokenKind.LessEqual => Compare(lhs, rhs, static comparison => comparison <= 0),
                TokenKind.Greater => Compare(lhs, rhs, static comparison => comparison > 0),
                TokenKind.GreaterEqual => Compare(lhs, rhs, static comparison => comparison >= 0),
                _ => throw new ExpressionEvaluationException(
                    $"Unsupported binary operator {operation}", Offset),
            };
        }

        private static ExpressionValue Add(
            in ExpressionValue left,
            in ExpressionValue right)
        {
            if (left.Kind == ExpressionValueKind.Number
                || left.Kind == ExpressionValueKind.Boolean)
            {
                return ExpressionValue.Number(
                    left.AsNumber("addition") + right.AsNumber("addition"));
            }
            if (left.Kind == ExpressionValueKind.String)
            {
                return ExpressionValue.String(
                    left.AsString("concatenation") + right.ToDisplayString());
            }
            throw new ExpressionEvaluationException(
                $"Unable to add {left.Kind} to {right.Kind}.");
        }

        private static ExpressionValue Subtract(
            in ExpressionValue left,
            in ExpressionValue right,
            string? source)
        {
            if (left.Kind is ExpressionValueKind.Number
                    or ExpressionValueKind.Boolean
                && right.Kind is ExpressionValueKind.Number
                    or ExpressionValueKind.Boolean)
            {
                return ExpressionValue.Number(
                    left.AsNumber("subtraction") - right.AsNumber("subtraction"));
            }
            // The reference answers a string minus a string with the
            // subtraction's own source text, the way a dash inside an unquoted
            // name reads back: "$a-$b" is the text "$a-$b", not the values.
            if (left.Kind == ExpressionValueKind.String
                && right.Kind == ExpressionValueKind.String)
            {
                return ExpressionValue.String(
                    source ?? left.AsString() + "-" + right.AsString());
            }
            throw new ExpressionEvaluationException(
                $"Unable to subtract {right.Kind} from {left.Kind}.");
        }

        /// <summary>
        /// '==' as the reference decides it. A string on the left is
        /// compared, ignoring case, with the right side as it prints, so a
        /// capture group "5" equals the number 5. Anything else on the left
        /// is equal only to a value of its own kind that is equal to it: a
        /// number never equals a string.
        /// </summary>
        private static bool AreEqual(in ExpressionValue left, in ExpressionValue right) =>
            left.Kind == ExpressionValueKind.String
                ? string.Equals(
                    left.AsString().ToLowerInvariant(),
                    right.ToDisplayString().ToLowerInvariant(),
                    StringComparison.Ordinal)
                : left.Equals(right);

        private static ExpressionValue Compare(
            in ExpressionValue left,
            in ExpressionValue right,
            Func<int, bool> predicate)
        {
            double lhs = left.AsNumber("comparison");
            double rhs = right.AsNumber("comparison");
            return ExpressionValue.Boolean(predicate(lhs.CompareTo(rhs)));
        }

        private static ExpressionValue RegexMatch(
            ExpressionEvaluationContext context,
            in ExpressionValue left,
            in ExpressionValue right)
        {
            var regex = new Regex(
                right.ToDisplayString(),
                RegexOptions.IgnoreCase | RegexOptions.CultureInvariant,
                TimeSpan.FromMilliseconds(100));
            Match match = regex.Match(left.ToDisplayString());
            // Only a match sets the capture groups, as the reference sets
            // them; a failed one leaves the last match's groups readable.
            if (!match.Success)
                return ExpressionValue.Boolean(false);
            foreach (string groupName in regex.GetGroupNames())
            {
                string variableName = "capturegroup_" + groupName;
                Group group = match.Groups[groupName];
                if (group.Success)
                {
                    context.State.Set(
                        ExpressionVariableScope.Session,
                        variableName,
                        ExpressionValue.String(group.Value));
                }
                else
                {
                    context.State.Clear(
                        ExpressionVariableScope.Session,
                        variableName);
                }
            }
            return ExpressionValue.Boolean(match.Success);
        }
    }

    private sealed class IndexNode(
        Node source,
        Node? start,
        Node? end,
        bool isSlice,
        int offset) : Node(offset)
    {
        protected override ExpressionValue EvaluateCore(ExpressionEvaluationContext context)
        {
            context.Step(Offset);
            ExpressionValue value = source.Evaluate(context);
            if (value.Kind == ExpressionValueKind.Dictionary)
            {
                if (isSlice)
                {
                    throw new ExpressionEvaluationException(
                        "Range indices are not supported with dictionaries",
                        Offset);
                }
                string key = (start?.Evaluate(context) ?? ExpressionValue.Zero)
                    .AsString("dictionary index");
                return value.AsDictionary().Items.TryGetValue(
                    key,
                    out ExpressionValue found)
                        ? found
                        : ExpressionValue.Zero;
            }

            int length = value.Kind switch
            {
                ExpressionValueKind.List => value.AsList().Items.Count,
                ExpressionValueKind.String => value.AsString().Length,
                _ => throw new ExpressionEvaluationException(
                    $"{value.Kind} does not support index access",
                    Offset),
            };
            int first = ResolveIndex(context, start, length, 0, allowEnd: isSlice);
            if (!isSlice)
            {
                return value.Kind == ExpressionValueKind.List
                    ? value.AsList().Items[first]
                    : ExpressionValue.String(value.AsString().Substring(first, 1));
            }
            int last = ResolveIndex(context, end, length, length, allowEnd: true);
            int count = Math.Max(0, last - first);
            return value.Kind == ExpressionValueKind.List
                ? ExpressionValue.List(new ExpressionList(
                    value.AsList().Items.Skip(first).Take(count)))
                : ExpressionValue.String(value.AsString().Substring(first, count));
        }

        private int ResolveIndex(
            ExpressionEvaluationContext context,
            Node? expression,
            int length,
            int defaultValue,
            bool allowEnd)
        {
            int index = expression is null
                ? defaultValue
                : Truncate(expression.Evaluate(context).AsNumber("index"));
            if (index < 0)
                index += length;
            int maximum = allowEnd ? length : length - 1;
            if (index < 0 || index > maximum)
            {
                throw new ExpressionEvaluationException(
                    $"Index {index} is outside 0..{maximum}",
                    Offset);
            }
            return index;
        }

        /// <summary>
        /// An index as the reference reads it: the number cut toward zero,
        /// so 1.9 is 1 and -1.5 is -1. A number with no 32-bit whole part
        /// (NaN, or past the range either way) reads as the lowest 32-bit
        /// number, which no list or string reaches, so it is out of range.
        /// </summary>
        private static int Truncate(double number) =>
            number is > -2147483649d and < 2147483648d ? (int)number : int.MinValue;
    }

    private enum TokenKind
    {
        End,
        Number,
        HexNumber,
        String,
        True,
        False,
        LeftParen,
        RightParen,
        LeftBracket,
        RightBracket,
        LeftBrace,
        RightBrace,
        Comma,
        Semicolon,
        Colon,
        Dollar,
        At,
        Ampersand,
        Pipe,
        Tilde,
        Plus,
        Minus,
        Star,
        Slash,
        Percent,
        Caret,
        Hash,
        Equal,
        EqualEqual,
        BangEqual,
        Less,
        LessEqual,
        Greater,
        GreaterEqual,
        ShiftLeft,
        ShiftRight,
        AndAnd,
        OrOr,
    }

    /// <param name="Kind">Which token this is.</param>
    /// <param name="Text">
    /// The token as a name: a function name, a variable name or a keyword,
    /// with the run of spaces that separates it from what follows taken off.
    /// </param>
    /// <param name="Offset">Where it starts in the source.</param>
    /// <param name="Literal">
    /// An unquoted string exactly as it was written, trailing spaces and all,
    /// for when the token is a string rather than a name. The grammar's
    /// continuation class for an unquoted string includes the space, so
    /// "Blackfire " is a ten-character name and not the nine-character one;
    /// a name pattern written with a trailing space has to stay a different
    /// pattern. Null for every other token.
    /// </param>
    private readonly record struct Token(
        TokenKind Kind,
        string Text,
        int Offset,
        string? Literal = null)
    {
        /// <summary>The token exactly as the source spells it.</summary>
        public string Raw { get; init; } = string.Empty;
    }

    /// <summary>
    /// The reference lexer, rule for rule: at each position every token rule
    /// is tried, the longest match wins and a tie goes to the rule listed
    /// first (the operators, then true/false, '-', a number, a hexadecimal
    /// number, a string). A position no rule matches is not an error: the
    /// characters the rules had taken before they all failed are dropped,
    /// together with the one they failed on, and lexing goes on. So "!0"
    /// (a '!' only ever starts "!=") and ".x" (a '.' only ever starts a
    /// number) vanish, and so does a lone "?". The rules:
    /// <list type="bullet">
    /// <item><description>a number is digits, an optional '.', then at least
    /// one digit;</description></item>
    /// <item><description>a hexadecimal number is "0x" and at least one hex
    /// digit;</description></item>
    /// <item><description>a quoted string runs from a backtick to the next
    /// backtick not escaped with a backslash;</description></item>
    /// <item><description>an unquoted string starts with a letter, '_', an
    /// apostrophe, a double quote or a backslash and any character, and goes
    /// on through letters, digits, '_', apostrophes, double quotes, spaces and
    /// backslash pairs.</description></item>
    /// </list>
    /// </summary>
    private sealed class Lexer(string source, List<string> repairs)
    {
        private static readonly string[] Operators =
        [
            ">>", "<<", ">=", "<=", "==", "!=", "&&", "||",
            ";", "(", ")", "$", "@", "&", "{", ":", "}", "~", "^", "|",
            "*", "/", "%", "+", "#", "=", ">", "<", "[", "]", ",",
        ];

        private static readonly string[] Booleans = ["true", "false"];

        private int _offset;

        public Token Next()
        {
            while (true)
            {
                while (_offset < source.Length && char.IsWhiteSpace(source[_offset]))
                    _offset++;
                if (_offset >= source.Length)
                    return new Token(TokenKind.End, string.Empty, _offset);

                int start = _offset;
                int viable = 0;
                int best = 0;
                TokenKind kind = TokenKind.End;
                bool quoted = source[start] == '`';

                (int length, int reach, TokenKind candidate) = MatchOperator(start);
                Consider(length, reach, candidate);
                (length, reach, bool truth) = MatchBoolean(start);
                Consider(length, reach, truth ? TokenKind.True : TokenKind.False);
                int minus = source[start] == '-' ? 1 : 0;
                Consider(minus, minus, TokenKind.Minus);
                (length, reach) = MatchNumber(start);
                Consider(length, reach, TokenKind.Number);
                (length, reach) = MatchHexNumber(start);
                Consider(length, reach, TokenKind.HexNumber);
                (length, reach) = quoted ? MatchQuoted(start) : MatchUnquoted(start);
                Consider(length, reach, TokenKind.String);

                if (best == 0)
                {
                    // Nothing matched: drop what the rules had taken, and the
                    // character they failed on.
                    int failedAt = start + viable;
                    _offset = failedAt < source.Length ? failedAt + 1 : failedAt;
                    repairs.Add($"dropped '{source[start.._offset]}' at {start}");
                    continue;
                }

                _offset = start + best;
                string raw = source[start.._offset];
                Token token = kind switch
                {
                    TokenKind.String when quoted =>
                        new Token(TokenKind.String, Unescape(raw[1..^1]), start),
                    // A run of spaces does not end an unquoted string, so the
                    // token can carry the spaces that separate it from the
                    // next one. As a string those spaces are part of it; as a
                    // name they are not, so the token carries both readings
                    // and the parser picks.
                    TokenKind.String => new Token(
                        TokenKind.String,
                        Unescape(raw.TrimEnd()),
                        start,
                        Unescape(raw)),
                    TokenKind.HexNumber => new Token(TokenKind.HexNumber, raw[2..], start),
                    _ => new Token(kind, raw, start),
                };
                return token with { Raw = raw };

                void Consider(int accepted, int reached, TokenKind rule)
                {
                    viable = Math.Max(viable, reached);
                    if (accepted > best)
                    {
                        best = accepted;
                        kind = rule;
                    }
                }
            }
        }

        private (int Length, int Reach, TokenKind Kind) MatchOperator(int start)
        {
            int reach = 0;
            foreach (string candidate in Operators)
            {
                int matched = Prefix(start, candidate, ignoreCase: false);
                if (matched == candidate.Length)
                    return (matched, matched, OperatorKind(candidate));
                reach = Math.Max(reach, matched);
            }
            return (0, reach, TokenKind.End);
        }

        private (int Length, int Reach, bool Value) MatchBoolean(int start)
        {
            int reach = 0;
            foreach (string word in Booleans)
            {
                int matched = Prefix(start, word, ignoreCase: true);
                if (matched == word.Length)
                    return (matched, matched, word == "true");
                reach = Math.Max(reach, matched);
            }
            return (0, reach, false);
        }

        private int Prefix(int start, string word, bool ignoreCase)
        {
            int matched = 0;
            while (matched < word.Length
                && start + matched < source.Length
                && (ignoreCase
                    ? char.ToLowerInvariant(source[start + matched])
                    : source[start + matched]) == word[matched])
            {
                matched++;
            }
            return matched;
        }

        private static TokenKind OperatorKind(string text) => text switch
        {
            ">>" => TokenKind.ShiftRight,
            "<<" => TokenKind.ShiftLeft,
            ">=" => TokenKind.GreaterEqual,
            "<=" => TokenKind.LessEqual,
            "==" => TokenKind.EqualEqual,
            "!=" => TokenKind.BangEqual,
            "&&" => TokenKind.AndAnd,
            "||" => TokenKind.OrOr,
            ";" => TokenKind.Semicolon,
            "(" => TokenKind.LeftParen,
            ")" => TokenKind.RightParen,
            "$" => TokenKind.Dollar,
            "@" => TokenKind.At,
            "&" => TokenKind.Ampersand,
            "{" => TokenKind.LeftBrace,
            ":" => TokenKind.Colon,
            "}" => TokenKind.RightBrace,
            "~" => TokenKind.Tilde,
            "^" => TokenKind.Caret,
            "|" => TokenKind.Pipe,
            "*" => TokenKind.Star,
            "/" => TokenKind.Slash,
            "%" => TokenKind.Percent,
            "+" => TokenKind.Plus,
            "#" => TokenKind.Hash,
            "=" => TokenKind.Equal,
            ">" => TokenKind.Greater,
            "<" => TokenKind.Less,
            "[" => TokenKind.LeftBracket,
            "]" => TokenKind.RightBracket,
            "," => TokenKind.Comma,
            _ => throw new InvalidOperationException(),
        };

        /// <summary>Digits, an optional '.', and at least one digit after it.</summary>
        private (int Length, int Reach) MatchNumber(int start)
        {
            int index = start;
            while (index < source.Length && IsDigit(source[index]))
                index++;
            int whole = index - start;
            if (index >= source.Length || source[index] != '.')
                return (whole, whole);
            int fraction = index + 1;
            while (fraction < source.Length && IsDigit(source[fraction]))
                fraction++;
            return fraction > index + 1
                ? (fraction - start, fraction - start)
                : (whole, index + 1 - start);
        }

        /// <summary>"0x" (lower case) and at least one hexadecimal digit.</summary>
        private (int Length, int Reach) MatchHexNumber(int start)
        {
            if (source[start] != '0')
                return (0, 0);
            if (start + 1 >= source.Length || source[start + 1] != 'x')
                return (0, 1);
            int index = start + 2;
            while (index < source.Length && Uri.IsHexDigit(source[index]))
                index++;
            return (index > start + 2 ? index - start : 0, index - start);
        }

        /// <summary>
        /// A backtick string. A backslash before a backtick may escape it or
        /// be a character of its own before the closing backtick; the longer
        /// reading wins, as it does for every token.
        /// </summary>
        private (int Length, int Reach) MatchQuoted(int start)
        {
            int accepted = 0;
            int index = start + 1;
            while (index < source.Length)
            {
                if (source[index] == '`')
                    return (index + 1 - start, index + 1 - start);
                if (source[index] == '\\'
                    && index + 1 < source.Length
                    && source[index + 1] == '`')
                {
                    accepted = index + 2 - start;
                    index += 2;
                    continue;
                }
                index++;
            }
            return (accepted, source.Length - start);
        }

        private (int Length, int Reach) MatchUnquoted(int start)
        {
            int index = start;
            int accepted = 0;
            while (index < source.Length)
            {
                char current = source[index];
                if (current == '\\')
                {
                    if (index + 1 >= source.Length)
                        return (accepted, index + 1 - start);
                    index += 2;
                    accepted = index - start;
                    continue;
                }
                bool continues = index == start
                    ? IsStringStart(current)
                    : IsStringStart(current) || IsDigit(current) || current == ' ';
                if (!continues)
                    break;
                index++;
                accepted = index - start;
            }
            return (accepted, index - start);
        }

        private static bool IsDigit(char value) => value is >= '0' and <= '9';

        private static bool IsStringStart(char value) =>
            value is >= 'a' and <= 'z' or >= 'A' and <= 'Z' or '_' or '\'' or '"';

        private static string Unescape(string value)
        {
            if (!value.Contains('\\', StringComparison.Ordinal))
                return value;
            var built = new StringBuilder(value.Length);
            for (int index = 0; index < value.Length; index++)
            {
                char current = value[index];
                if (current == '\\' && index + 1 < value.Length)
                    current = value[++index];
                built.Append(current);
            }
            return built.ToString();
        }
    }

    private sealed class Parser
    {
        // The reference parser recovers from missing and extra tokens without
        // reporting it, and profiles are written against that. Its recovery,
        // as it plays out in this grammar:
        //  - where a closer (a group's ')', a call's ']') is due, one extra
        //    token before it is dropped; failing that, the closer is taken as
        //    read when the token in its place may follow a closed construct
        //    anywhere up the nesting (an operator, or what ends any enclosing
        //    statement, group, argument or index). So "(a;b)" closes before
        //    the ';' and runs as "a" then "b", and "f[(a+b]" closes before
        //    the ']'.
        //  - after the first statement, or the first argument of a call, one
        //    extra token is dropped when the token after it ends the
        //    statement or argument; anything else is an error.
        //  - after the second or any later statement, every token up to the
        //    next ';' or the end is skipped; after the second or any later
        //    argument, every token up to one that may follow there (',', ']',
        //    an operator, or what ends any enclosing construct).
        //  - once it has skipped or supplied a token it is "recovering" until
        //    it next reads a token it expected, and while recovering it skips
        //    nothing more, and supplies every closer it has to.
        // A call's ']' is therefore only ever supplied while recovering, and
        // an index's '}' never is.
        private static readonly TokenKind[] StatementEnds =
            [TokenKind.Semicolon, TokenKind.End];
        private static readonly TokenKind[] ArgumentEnds =
            [TokenKind.Comma, TokenKind.RightBracket];
        private static readonly TokenKind[] GroupEnds = [TokenKind.RightParen];
        private static readonly TokenKind[] IndexEnds =
            [TokenKind.Colon, TokenKind.RightBrace];

        /// <summary>
        /// Every token that may directly follow a complete operand wherever it
        /// sits: a binary operator or the '{' of an index. The assignment '='
        /// is not one: it follows only a variable being set.
        /// </summary>
        private static readonly TokenKind[] OperandFollowers =
        [
            TokenKind.OrOr, TokenKind.AndAnd,
            TokenKind.EqualEqual, TokenKind.BangEqual,
            TokenKind.Less, TokenKind.LessEqual,
            TokenKind.Greater, TokenKind.GreaterEqual,
            TokenKind.Hash,
            TokenKind.Plus, TokenKind.Minus,
            TokenKind.Star, TokenKind.Slash, TokenKind.Percent,
            TokenKind.Ampersand, TokenKind.Caret, TokenKind.Pipe,
            TokenKind.ShiftLeft, TokenKind.ShiftRight,
            TokenKind.LeftBrace,
        ];

        private readonly Lexer _lexer;
        private Token _current;
        private Token? _next;
        private bool _recovering;

        /// <summary>
        /// What ends each construct being read, innermost on top: the
        /// statement, a group, a call's arguments, an index.
        /// </summary>
        private readonly Stack<TokenKind[]> _enclosing = new([StatementEnds]);

        private readonly List<string> _repairs;
        private readonly StringBuilder _text = new();

        /// <summary>See <see cref="ExpressionProgram.Text"/>.</summary>
        public string Text => _text.ToString() + "<EOF>";

        public Parser(string source, List<string> repairs)
        {
            _repairs = repairs;
            _lexer = new Lexer(source, repairs);
            _current = _lexer.Next();
        }

        public Node[] ParseProgram()
        {
            var statements = new List<Node>();
            statements.Add(ParseExpression());
            SyncAfterItem(StatementEnds, laterItem: false, StatementEnds);
            while (_current.Kind == TokenKind.Semicolon)
            {
                Advance();
                if (_current.Kind == TokenKind.End)
                    break;
                statements.Add(ParseExpression());
                SyncAfterItem(StatementEnds, laterItem: true, StatementEnds);
            }
            if (_current.Kind != TokenKind.End)
            {
                if (Peek().Kind != TokenKind.End)
                {
                    throw new ExpressionParseException(
                        $"Unexpected token '{_current.Text}'",
                        _current.Offset);
                }
                Skip();
            }
            return statements.ToArray();
        }

        // Operator precedence, loosest first, one line per level:
        //     || &&
        //     == != < <= > >=
        //     =
        //     #
        //     + -
        //     * / %
        //     & ^ |
        //     << >>
        //     ~ and a negative number
        //     {index} and {slice}
        // Three of those levels surprise a reader used to C, and a profile
        // depends on all three: the bitwise operators share ONE level that
        // binds tighter than multiplication, '^' is an integer exclusive-or
        // and never a power, and an assignment binds tighter than a
        // comparison, so "$x = 1 == 1" stores 1 and compares what it stored.

        /// <summary>
        /// The loosest level. A statement, a parenthesised group, an index and
        /// a call argument each start over here, so every operator is usable
        /// inside them.
        /// </summary>
        private Node ParseExpression() => ParseLogical();

        private Node ParseLogical() => ParseLeft(
            ParseComparison,
            TokenKind.AndAnd,
            TokenKind.OrOr);
        private Node ParseComparison() => ParseLeft(
            ParseAssignment,
            TokenKind.EqualEqual,
            TokenKind.BangEqual,
            TokenKind.Less,
            TokenKind.LessEqual,
            TokenKind.Greater,
            TokenKind.GreaterEqual);

        /// <summary>
        /// Assignment is right-associative and takes everything tighter than a
        /// comparison, so "$a = $b = 1" chains and "$x = 1 + 2" stores 3.
        /// </summary>
        private Node ParseAssignment()
        {
            Node left = ParseRegex();
            if (_current.Kind != TokenKind.Equal)
                return left;
            Token operation = _current;
            Advance();
            if (left is not VariableNode variable)
            {
                throw new ExpressionParseException(
                    "Only variables may appear on the left of '='",
                    operation.Offset);
            }
            return new AssignmentNode(variable, ParseAssignment(), operation.Offset);
        }

        private Node ParseRegex() => ParseLeft(ParseAdditive, TokenKind.Hash);
        private Node ParseAdditive() => ParseLeft(
            ParseMultiplicative,
            TokenKind.Plus,
            TokenKind.Minus);
        private Node ParseMultiplicative() => ParseLeft(
            ParseBitwise,
            TokenKind.Star,
            TokenKind.Slash,
            TokenKind.Percent);

        /// <summary>
        /// One level for all three bitwise operators, left to right, binding
        /// tighter than multiplication: "2 &amp; 3 * 2" is (2 &amp; 3) * 2.
        /// </summary>
        private Node ParseBitwise() => ParseLeft(
            ParseShift,
            TokenKind.Ampersand,
            TokenKind.Caret,
            TokenKind.Pipe);
        private Node ParseShift() => ParseLeft(
            ParseUnary,
            TokenKind.ShiftLeft,
            TokenKind.ShiftRight);

        private Node ParseUnary()
        {
            if (_current.Kind is not (TokenKind.Minus or TokenKind.Tilde))
                return ParsePostfix();
            Token operation = _current;
            Advance();
            return new UnaryNode(operation.Kind, ParseUnary(), operation.Offset);
        }

        private Node ParsePostfix()
        {
            Node source = ParsePrimary();
            while (_current.Kind == TokenKind.LeftBrace)
            {
                Token opening = _current;
                Advance();
                Node? start = null;
                Node? end = null;
                bool slice = false;
                _enclosing.Push(IndexEnds);
                if (_current.Kind != TokenKind.Colon
                    && _current.Kind != TokenKind.RightBrace)
                {
                    start = ParseExpression();
                }
                if (_current.Kind == TokenKind.Colon)
                {
                    slice = true;
                    Advance();
                    if (_current.Kind != TokenKind.RightBrace)
                        end = ParseExpression();
                }
                _enclosing.Pop();
                Require(TokenKind.RightBrace, "Closing '}' expected");
                source = new IndexNode(source, start, end, slice, opening.Offset);
            }
            return source;
        }

        /// <param name="asName">
        /// True when what is being read names something — a variable — rather
        /// than being a value in its own right, so the spaces that separate
        /// it from the next token are not part of it.
        /// </param>
        private Node ParsePrimary(bool asName = false)
        {
            Token token = _current;
            switch (token.Kind)
            {
                case TokenKind.Number:
                    Advance();
                    return new LiteralNode(
                        ExpressionValue.Number(double.Parse(
                            token.Text,
                            NumberStyles.Float,
                            CultureInfo.InvariantCulture)),
                        token.Offset);
                case TokenKind.HexNumber:
                    Advance();
                    return new LiteralNode(
                        ExpressionValue.Number(ReadHexLiteral(token)),
                        token.Offset);
                case TokenKind.True:
                case TokenKind.False:
                    Advance();
                    return new LiteralNode(
                        ExpressionValue.Boolean(token.Kind == TokenKind.True),
                        token.Offset);
                case TokenKind.String:
                    Advance();
                    if (_current.Kind != TokenKind.LeftBracket)
                    {
                        RequireStringFollower(asName);
                        // Read as a string, so it keeps its trailing spaces.
                        // A bracket makes it a function name instead, and a
                        // name -- a function's or a variable's -- is read
                        // without them, so "$x = 1" and "$x + 1" name the
                        // same variable.
                        return new LiteralNode(
                            ExpressionValue.String(
                                asName ? token.Text : token.Literal ?? token.Text),
                            token.Offset);
                    }
                    return ParseFunction(token);
                case TokenKind.Dollar:
                case TokenKind.At:
                case TokenKind.Ampersand:
                    return ParseVariable();
                case TokenKind.LeftParen:
                    Advance();
                    _enclosing.Push(GroupEnds);
                    Node nested = ParseExpression();
                    _enclosing.Pop();
                    RequireCloser(TokenKind.RightParen, "Closing ')' expected");
                    return nested;
                default:
                    throw new ExpressionParseException(
                        $"Expression expected; found '{token.Text}'",
                        token.Offset);
            }
        }

        /// <summary>
        /// A hexadecimal literal is a 32-bit pattern read as a signed number:
        /// 0x80000000 is -2147483648 and 0xFFFFFFFF is -1. Bit operations
        /// truncate to the same 32 bits either way, so this only shows in a
        /// comparison or in printing, which is exactly where a flags value
        /// has to match its literal. A pattern too wide for 32 bits is a
        /// literal this language cannot hold, and says so at parse time.
        /// </summary>
        private static double ReadHexLiteral(Token token)
        {
            try
            {
                return unchecked((int)Convert.ToUInt32(token.Text, 16));
            }
            catch (OverflowException)
            {
                throw new ExpressionParseException(
                    "Hexadecimal literal does not fit in 32 bits",
                    token.Offset);
            }
        }

        private Node ParseFunction(Token name)
        {
            Require(TokenKind.LeftBracket, "Opening '[' expected");
            var arguments = new List<Node>();
            if (_current.Kind != TokenKind.RightBracket)
            {
                _enclosing.Push(ArgumentEnds);
                arguments.Add(ParseExpression());
                SyncAfterItem(ArgumentEnds, laterItem: false, ArgumentLaterStops());
                while (_current.Kind == TokenKind.Comma)
                {
                    Advance();
                    arguments.Add(ParseExpression());
                    SyncAfterItem(ArgumentEnds, laterItem: true, ArgumentLaterStops());
                }
                _enclosing.Pop();
            }
            RequireCloser(TokenKind.RightBracket, "Closing ']' expected");
            return new FunctionNode(name.Text, arguments.ToArray(), name.Offset);
        }

        private Node ParseVariable()
        {
            Token prefix = _current;
            Advance();
            if (_current.Kind is TokenKind.End
                or TokenKind.Comma
                or TokenKind.Semicolon
                or TokenKind.RightBracket
                or TokenKind.RightBrace
                or TokenKind.RightParen)
            {
                throw new ExpressionParseException(
                    "Variable name expected",
                    _current.Offset);
            }
            Node name = ParsePrimary(asName: true);
            ExpressionVariableScope scope = prefix.Kind switch
            {
                TokenKind.Dollar => ExpressionVariableScope.Session,
                TokenKind.At => ExpressionVariableScope.Persistent,
                TokenKind.Ampersand => ExpressionVariableScope.Global,
                _ => throw new InvalidOperationException(),
            };
            return new VariableNode(scope, name, prefix.Offset);
        }

        private Node ParseLeft(
            Func<Node> operand,
            params TokenKind[] operations)
        {
            int start = _text.Length;
            Node left = operand();
            while (operations.Contains(_current.Kind))
            {
                Token operation = _current;
                Advance();
                Node right = operand();
                // A subtraction of two strings answers its own source text,
                // so it keeps what it was read from.
                string? source = operation.Kind == TokenKind.Minus
                    ? _text.ToString(start, _text.Length - start)
                    : null;
                left = new BinaryNode(
                    operation.Kind,
                    left,
                    right,
                    operation.Offset,
                    source);
            }
            return left;
        }

        private void Require(TokenKind expected, string message)
        {
            if (_current.Kind != expected)
                throw new ExpressionParseException(message, _current.Offset);
            Advance();
        }

        /// <summary>
        /// A group's ')' or a call's ']'. One extra token before it is
        /// dropped; failing that, a missing closer is taken as read when the
        /// token in its place may follow a closed construct here.
        /// </summary>
        private void RequireCloser(TokenKind closer, string message)
        {
            if (_current.Kind == closer)
            {
                Advance();
                return;
            }
            if (Peek().Kind == closer)
            {
                Skip();
                Advance();
                return;
            }
            if (MayFollowAClosedConstruct(_current.Kind))
            {
                string missing = closer == TokenKind.RightParen ? ")" : "]";
                _repairs.Add($"supplied a missing '{missing}' at {_current.Offset}");
                _text.Append("<missing '").Append(missing).Append("'>");
                _recovering = true;
                return;
            }
            throw new ExpressionParseException(message, _current.Offset);
        }

        /// <summary>
        /// A string could be a value or the name of a call, and the reference
        /// parser tells the two apart by looking at what comes next, in the
        /// construct the string sits in. So after a string value, a token
        /// that ends constructs (the end, ';', ')', ']', ',', ':', '}', or
        /// '=') has to end this one ('=' only after a variable's name), or
        /// the expression is refused; no recovery applies. Any other token is
        /// left to the usual recovery.
        /// </summary>
        private void RequireStringFollower(bool asName)
        {
            TokenKind next = _current.Kind;
            if (next is not (TokenKind.End or TokenKind.Semicolon or TokenKind.RightParen
                or TokenKind.RightBracket or TokenKind.Comma or TokenKind.Colon
                or TokenKind.RightBrace or TokenKind.Equal))
            {
                return;
            }
            if (_enclosing.Peek().Contains(next) || (asName && next == TokenKind.Equal))
                return;
            throw new ExpressionParseException(
                next == TokenKind.End
                    ? "Unexpected end of expression"
                    : $"Unexpected token '{_current.Text}'",
                _current.Offset);
        }

        private bool MayFollowAClosedConstruct(TokenKind kind) =>
            OperandFollowers.Contains(kind)
            || _enclosing.Any(ends => ends.Contains(kind));

        /// <summary>
        /// After a statement or a call argument: the token must end it.
        /// After the first one, one extra token before an end is dropped;
        /// after a later one, every token up to one of <paramref name="stops"/>
        /// is skipped. While recovering nothing is checked.
        /// </summary>
        private void SyncAfterItem(TokenKind[] ends, bool laterItem, TokenKind[] stops)
        {
            if (_recovering || ends.Contains(_current.Kind))
                return;
            if (!laterItem)
            {
                if (!ends.Contains(Peek().Kind))
                {
                    throw new ExpressionParseException(
                        ends == ArgumentEnds
                            ? "Closing ']' expected"
                            : $"Unexpected token '{_current.Text}'",
                        _current.Offset);
                }
                Skip();
                return;
            }
            _recovering = true;
            while (_current.Kind != TokenKind.End && !stops.Contains(_current.Kind))
                Skip();
        }

        /// <summary>
        /// Where skipping stops after a later call argument: its ',' or ']',
        /// an operator, or what ends any construct it sits in.
        /// </summary>
        private TokenKind[] ArgumentLaterStops() =>
            [.. ArgumentEnds, .. OperandFollowers, .. _enclosing.SelectMany(static ends => ends)];

        private Token Peek() => _next ??= _lexer.Next();

        /// <summary>Reads the token the parser expected; any recovery is over.</summary>
        private void Advance()
        {
            _recovering = false;
            MoveNext();
        }

        /// <summary>Moves past a stray token without treating it as expected.</summary>
        private void Skip()
        {
            _repairs.Add(_current.Kind == TokenKind.End
                ? $"skipped the end at {_current.Offset}"
                : $"skipped '{_current.Literal ?? _current.Text}' at {_current.Offset}");
            MoveNext();
        }

        private void MoveNext()
        {
            _text.Append(_current.Raw);
            if (_next is Token next)
            {
                _current = next;
                _next = null;
            }
            else
            {
                _current = _lexer.Next();
            }
        }
    }
}

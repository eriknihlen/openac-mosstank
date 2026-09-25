namespace AcDream.Plugins.MossTank.Expressions;

/// <summary>
/// The reference's check of a function's arguments, made before the function
/// runs and worded exactly as it words it. It walks the function's own
/// parameters in order: the expression state it is handed takes a place in
/// the argument list, a "rest" parameter takes every remaining argument, a
/// missing optional parameter gets its default, a parameter that takes
/// anything is not checked, a number handed to a world-object parameter is an
/// object id the client must know, and any other argument must be exactly
/// the declared type. Then the count must match.
/// </summary>
internal static partial class ReferenceArguments
{
    /// <summary>Whether the reference declares this function.</summary>
    public static bool Declares(string name) => Declared.ContainsKey(name);

    /// <summary>
    /// The count of arguments the reference would accept for the function,
    /// from the fewest to the most; null for no upper bound.
    /// </summary>
    public static (int Minimum, int? Maximum) Accepted(string name)
    {
        (string types, string parameters) = Declared[name];
        if (types.Length > 0 && types[^1] == 'P')
            return (0, null);
        int own = parameters.Count(static parameter => parameter != 's');
        int required = parameters.Count(static parameter => parameter == 'r');
        return (required, own);
    }

    /// <summary>
    /// Checks the arguments of a declared function; throws the reference's
    /// error when it would. <paramref name="isKnownObject"/> tells whether
    /// the client knows an object id; null takes every id as known. The
    /// error's <see cref="ExpressionEvaluationException.Reason"/> is the
    /// reference's text; its message adds <paramref name="offset"/>, where
    /// the call is.
    /// </summary>
    public static void Check(
        string name,
        IReadOnlyList<ExpressionValue> values,
        Func<uint, bool>? isKnownObject,
        int offset = -1)
    {
        if (!Declared.TryGetValue(name, out (string Types, string Parameters) declared))
            return;
        string types = declared.Types;
        string parameters = declared.Parameters;
        bool rest = types.Length > 0 && types[^1] == 'P';

        // The argument list as the reference builds it: the values, with the
        // state and defaults put in. A null entry is one it put in itself.
        var arguments = new List<ExpressionValue?>(values.Count + 1);
        foreach (ExpressionValue value in values)
            arguments.Add(value);

        for (int index = 0; index < parameters.Length; index++)
        {
            if (parameters[index] == 's')
            {
                if (index > arguments.Count)
                    throw Error(offset, "Index must be within the bounds of the List.\r\nParameter name: index");
                arguments.Insert(index, null);
                continue;
            }
            if (index >= types.Length)
                throw Error(offset, "Index was outside the bounds of the array.");
            char type = types[index];
            if (type == 'P')
            {
                arguments.RemoveRange(index, arguments.Count - index);
                arguments.Add(null);
                break;
            }
            if (parameters[index] == 'o' && index > arguments.Count - 1)
            {
                arguments.Add(null);
                continue;
            }
            if (type == 'O')
                continue;
            if (type == 'W')
            {
                if (index >= arguments.Count)
                {
                    throw Error(offset, "Index was out of range. Must be non-negative and less than the "
                        + "size of the collection.\r\nParameter name: index");
                }
                if (arguments[index] is { Kind: ExpressionValueKind.Number } id)
                {
                    uint objectId = ExpressionValue.SignedObjectId(id.AsNumber());
                    if (isKnownObject is null || isKnownObject(objectId))
                        continue;
                    throw Error(offset, $"{Signature(name, types)} expects argument #{index + 1}/{types.Length} "
                        + $"to be a {Friendly(type)} but an invalid (number)id was passed instead. "
                        + $"Passed value: {id.ToDisplayString()}");
                }
            }
            if (index < arguments.Count
                && arguments[index] is { } argument
                && TypeOf(argument) != type)
            {
                throw Error(offset, $"{Signature(name, types)} expects argument #{index + 1}/{types.Length} "
                    + $"to be a {Friendly(type)} but a {Friendly(TypeOf(argument))} was passed "
                    + $"instead. Passed value: {argument.ToDisplayString()}");
            }
        }

        int count = arguments.Count == 0 && rest ? 1 : arguments.Count;
        if (count != parameters.Length && !rest)
        {
            throw Error(offset, $"{Signature(name, types)} expects {types.Length} arguments. "
                + $"{count} arguments were passed.");
        }
    }

    /// <summary>
    /// The function as the reference names it in an error: its name and its
    /// declared parameter types, e.g. "wobjectgetintprop[WorldObject, number]".
    /// </summary>
    public static string Signature(string name, string types) =>
        $"{name}[{string.Join(", ", types.Select(Friendly))}]";

    /// <summary>
    /// A value's type as the reference names it to the user: "number" for a
    /// number or a true/false, "string", or the object's type name such as
    /// "List" or "UIControl".
    /// </summary>
    public static string FriendlyTypeName(in ExpressionValue value) => Friendly(TypeOf(value));

    private static char TypeOf(in ExpressionValue value) => value.Kind switch
    {
        ExpressionValueKind.Number or ExpressionValueKind.Boolean => 'N',
        ExpressionValueKind.String => 'S',
        ExpressionValueKind.List => 'L',
        ExpressionValueKind.Dictionary => 'D',
        ExpressionValueKind.WorldObject => 'W',
        ExpressionValueKind.Coordinates => 'C',
        ExpressionValueKind.Stopwatch => 'T',
        ExpressionValueKind.UiControl => 'U',
        _ => '?',
    };

    /// <summary>A type as the reference names it to the user.</summary>
    private static string Friendly(char type) => type switch
    {
        'N' => "number",
        'S' => "string",
        'O' => "Object",
        'L' => "List",
        'D' => "Dictionary",
        'W' => "WorldObject",
        'C' => "Coordinates",
        'T' => "Stopwatch",
        'U' => "UIControl",
        'P' => "...items",
        _ => "Object",
    };

    private static ExpressionEvaluationException Error(int offset, string message) =>
        new(message, offset) { IsArgumentError = true };
}

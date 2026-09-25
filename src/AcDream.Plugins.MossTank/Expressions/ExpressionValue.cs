using System.Globalization;

namespace AcDream.Plugins.MossTank.Expressions;

internal enum ExpressionValueKind
{
    Number,
    String,
    Boolean,
    List,
    Dictionary,
    Coordinates,
    WorldObject,
    Stopwatch,
    UiControl,
}

internal readonly record struct ExpressionCoordinates(
    double EastWest,
    double NorthSouth,
    double Elevation = 0d)
{
    /// <summary>
    /// The form an expression reads and writes: north/south then east/west,
    /// each with two decimals, then the height reading with two decimals and
    /// a Z ("36.50S, 28.90E, 0.24Z"), in the same digits for every culture.
    /// </summary>
    public override string ToString()
    {
        string ns = NorthSouth >= 0d ? "N" : "S";
        string ew = EastWest >= 0d ? "E" : "W";
        return Math.Abs(NorthSouth).ToString("F2", CultureInfo.InvariantCulture) + ns
            + ", " + Math.Abs(EastWest).ToString("F2", CultureInfo.InvariantCulture) + ew
            + ", " + Elevation.ToString("F2", CultureInfo.InvariantCulture) + "Z";
    }

    /// <summary>
    /// The short compass form the position report prints: each half rounded
    /// to one decimal, trailing ".0" dropped, no height.
    /// </summary>
    public string ToCompassText()
    {
        string ns = NorthSouth < 0d ? "S" : "N";
        string ew = EastWest < 0d ? "W" : "E";
        return Round(NorthSouth) + ns + ", " + Round(EastWest) + ew;
    }

    private static string Round(double value) =>
        Math.Round(Math.Abs(value), 1).ToString(CultureInfo.InvariantCulture);
}

/// <summary>
/// Counts every in-place change of any list or dictionary, so a saver can
/// tell cheaply whether anything it holds may have changed since it last
/// saved without comparing the values themselves.
/// </summary>
internal static class ExpressionCollectionChanges
{
    private static long _count;

    public static long Count => Interlocked.Read(ref _count);

    public static void Record() => Interlocked.Increment(ref _count);
}

internal sealed class ExpressionList
{
    public List<ExpressionValue> Items { get; } = [];

    public ExpressionList()
    {
    }

    public ExpressionList(IEnumerable<ExpressionValue> values) =>
        Items.AddRange(values);

    /// <summary>
    /// Set once an expression function adds, inserts, removes or clears an
    /// item of this list itself (a change inside a nested list is that
    /// list's own). A global variable's list that has it when the
    /// expression ends is saved again, whatever its contents are then.
    /// Clearing counts even when the list was already empty; removing an
    /// item that is not there does not count.
    /// </summary>
    public bool HasChanges { get; private set; }

    public void MarkChanged()
    {
        HasChanges = true;
        ExpressionCollectionChanges.Record();
    }

    public override string ToString() =>
        $"[{string.Join(",", Items.Select(static item => item.ToDisplayString()))}]";
}

internal sealed class ExpressionDictionary
{
    public Dictionary<string, ExpressionValue> Items { get; } =
        new(StringComparer.Ordinal);

    /// <summary>
    /// Set once an expression function adds, replaces or removes a key of
    /// this dictionary itself; see <see cref="ExpressionList.HasChanges"/>.
    /// Removing a key that is not there does not count.
    /// </summary>
    public bool HasChanges { get; private set; }

    public void MarkChanged()
    {
        HasChanges = true;
        ExpressionCollectionChanges.Record();
    }

    public override string ToString() =>
        $"[{string.Join(",", Items.Select(static pair =>
            pair.Key + "=>" + pair.Value.ToDisplayString()))}]";
}

internal sealed class ExpressionStopwatch
{
    private readonly System.Diagnostics.Stopwatch _clock = new();

    public bool IsRunning => _clock.IsRunning;

    /// <summary>
    /// Whole milliseconds over 1000, so an expression sees the same quantised
    /// figure a profile was written against.
    /// </summary>
    public double ElapsedSeconds => _clock.ElapsedMilliseconds / 1000d;
    public void Start() => _clock.Start();
    public void Stop() => _clock.Stop();
    public void Reset() => _clock.Reset();
    public void Restart() => _clock.Restart();
    public override string ToString() =>
        ElapsedSeconds.ToString("0.###", CultureInfo.InvariantCulture);
}

internal readonly record struct ExpressionWorldObject(uint ObjectId);
internal readonly record struct ExpressionUiControl(string View, string Control);

internal readonly struct ExpressionValue : IEquatable<ExpressionValue>
{
    private readonly double _number;
    private readonly object? _reference;

    private ExpressionValue(
        ExpressionValueKind kind,
        double number,
        object? reference)
    {
        Kind = kind;
        _number = number;
        _reference = reference;
    }

    public ExpressionValueKind Kind { get; }

    public static ExpressionValue Zero => Number(0d);
    public static ExpressionValue One => Number(1d);
    public static ExpressionValue Number(double value) =>
        new(ExpressionValueKind.Number, value, null);
    public static ExpressionValue String(string? value) =>
        new(ExpressionValueKind.String, 0d, value ?? string.Empty);
    public static ExpressionValue Boolean(bool value) =>
        new(ExpressionValueKind.Boolean, value ? 1d : 0d, null);
    public static ExpressionValue List(ExpressionList value) =>
        new(ExpressionValueKind.List, 0d, value);
    public static ExpressionValue Dictionary(ExpressionDictionary value) =>
        new(ExpressionValueKind.Dictionary, 0d, value);
    public static ExpressionValue Coordinates(ExpressionCoordinates value) =>
        new(ExpressionValueKind.Coordinates, 0d, value);
    /// <param name="objectId">The object's id.</param>
    /// <param name="names">
    /// The name the client knows the object by now, or null once it has
    /// lost it; used when the object is printed (see
    /// <see cref="ToDisplayString"/>). Null prints every object as lost.
    /// </param>
    public static ExpressionValue WorldObject(
        uint objectId,
        Func<uint, string?>? names = null) =>
        new(ExpressionValueKind.WorldObject, objectId, names);
    public static ExpressionValue Stopwatch(ExpressionStopwatch value) =>
        new(ExpressionValueKind.Stopwatch, 0d, value);
    public static ExpressionValue UiControl(ExpressionUiControl value) =>
        new(ExpressionValueKind.UiControl, 0d, value);

    public double AsNumber(string? operation = null)
    {
        if (Kind is ExpressionValueKind.Number or ExpressionValueKind.Boolean)
            return _number;
        throw TypeError(operation ?? "operation", "number");
    }

    public int AsInt32(string? operation = null) =>
        Convert.ToInt32(AsNumber(operation), CultureInfo.InvariantCulture);

    public string AsString(string? operation = null)
    {
        if (Kind == ExpressionValueKind.String)
            return (string)_reference!;
        throw TypeError(operation ?? "operation", "string");
    }

    public ExpressionList AsList(string? operation = null) =>
        Kind == ExpressionValueKind.List
            ? (ExpressionList)_reference!
            : throw TypeError(operation ?? "operation", "list");

    public ExpressionDictionary AsDictionary(string? operation = null) =>
        Kind == ExpressionValueKind.Dictionary
            ? (ExpressionDictionary)_reference!
            : throw TypeError(operation ?? "operation", "dictionary");

    public ExpressionCoordinates AsCoordinates(string? operation = null) =>
        Kind == ExpressionValueKind.Coordinates
            ? (ExpressionCoordinates)_reference!
            : throw TypeError(operation ?? "operation", "coordinates");

    public ExpressionStopwatch AsStopwatch(string? operation = null) =>
        Kind == ExpressionValueKind.Stopwatch
            ? (ExpressionStopwatch)_reference!
            : throw TypeError(operation ?? "operation", "stopwatch");

    public ExpressionUiControl AsUiControl(string? operation = null) =>
        Kind == ExpressionValueKind.UiControl
            ? (ExpressionUiControl)_reference!
            : throw TypeError(operation ?? "operation", "UI control");

    /// <summary>
    /// The object id a world object or a number names. A number is an id in
    /// the signed 32-bit form expressions use (0x800008B5 is -2147481419),
    /// rounded to the nearest whole number. The unsigned form earlier
    /// versions saved (2147485877) is read too; anything outside both
    /// throws.
    /// </summary>
    public uint AsObjectId(string? operation = null) => Kind switch
    {
        ExpressionValueKind.WorldObject => checked((uint)_number),
        ExpressionValueKind.Number => SignedObjectId(_number),
        _ => throw TypeError(operation ?? "operation", "world object"),
    };

    /// <summary>
    /// Reads a number as an object id: rounded to the nearest whole number
    /// (halves to even), then the same 32 bits. The signed form is the one
    /// ids are handed out in. A whole number above the signed range but
    /// within 32 unsigned bits is also taken as the id it names, because
    /// MossTank 0.4.0 and earlier handed ids out unsigned and profiles saved them
    /// that way in persistent and global variables; those saved values keep
    /// naming their objects after an upgrade.
    /// </summary>
    public static uint SignedObjectId(double number)
    {
        double whole = Math.Round(number, MidpointRounding.ToEven);
        if (whole > int.MaxValue && whole <= uint.MaxValue)
            return (uint)whole;
        return unchecked((uint)Convert.ToInt32(number));
    }

    /// <summary>
    /// An object id as a number, in the signed 32-bit form
    /// <c>wobjectgetid</c> hands out (0x800008B5 is -2147481419), so ids
    /// from every source compare equal.
    /// </summary>
    public static ExpressionValue ObjectIdNumber(uint objectId) =>
        Number(unchecked((int)objectId));

    public bool IsTruthy => Kind switch
    {
        ExpressionValueKind.Number or ExpressionValueKind.Boolean =>
            _number != 0d,
        ExpressionValueKind.String => ((string)_reference!).Length != 0,
        _ => true,
    };

    public string ToDisplayString() => Kind switch
    {
        ExpressionValueKind.Number =>
            _number.ToString("G15", CultureInfo.InvariantCulture),
        ExpressionValueKind.Boolean => _number != 0d ? "True" : "False",
        ExpressionValueKind.String => (string)_reference!,
        ExpressionValueKind.List => _reference!.ToString()!,
        ExpressionValueKind.Dictionary => _reference!.ToString()!,
        ExpressionValueKind.Coordinates => _reference!.ToString()!,
        ExpressionValueKind.WorldObject => WorldObjectText(),
        ExpressionValueKind.Stopwatch => _reference!.ToString()!,
        ExpressionValueKind.UiControl => _reference!.ToString()!,
        _ => string.Empty,
    };

    /// <summary>
    /// A world object as the reference prints it: "0x", the id as eight
    /// upper-case hex digits, then ": " and the name the client knows it by
    /// at the moment of printing, or " (Invalid)" once the client has lost
    /// it.
    /// </summary>
    private string WorldObjectText()
    {
        uint id = checked((uint)_number);
        string hex = "0x" + id.ToString("X8", CultureInfo.InvariantCulture);
        return _reference is Func<uint, string?> names && names(id) is { } name
            ? hex + ": " + name
            : hex + " (Invalid)";
    }

    /// <summary>
    /// Whether two values are the same value, the way the reference's
    /// list functions (listcontains, listindexof, listremove) find an item:
    /// only a value of the same kind can be equal, a string exactly (case
    /// counts), a number by value, a world object by id, and a list,
    /// dictionary, stopwatch or coordinates only to itself. The '=='
    /// operator has its own rule; see the expression engine.
    /// </summary>
    public bool Equals(ExpressionValue other)
    {
        if (Kind != other.Kind)
            return false;
        return Kind switch
        {
            ExpressionValueKind.String => string.Equals(
                (string)_reference!,
                (string)other._reference!,
                StringComparison.Ordinal),
            ExpressionValueKind.Number
                or ExpressionValueKind.Boolean
                or ExpressionValueKind.WorldObject => _number.Equals(other._number),
            _ => ReferenceEquals(_reference, other._reference),
        };
    }

    public override bool Equals(object? obj) =>
        obj is ExpressionValue other && Equals(other);

    public override int GetHashCode() => Kind switch
    {
        ExpressionValueKind.String => StringComparer.Ordinal.GetHashCode(
            (string)_reference!),
        ExpressionValueKind.Number or ExpressionValueKind.Boolean =>
            _number.GetHashCode(),
        ExpressionValueKind.WorldObject => HashCode.Combine(Kind, _number),
        _ => HashCode.Combine(Kind, _reference),
    };

    public override string ToString() => ToDisplayString();

    private ExpressionEvaluationException TypeError(
        string operation,
        string expected) => new(
        $"{operation} expects {expected}, but received {Kind}.")
    {
        IsArgumentError = true,
    };
}

internal sealed class ExpressionParseException : Exception
{
    public ExpressionParseException(string message, int offset)
        : base($"{message} at offset {offset}.") => Offset = offset;

    public int Offset { get; }
}

internal sealed class ExpressionEvaluationException : Exception
{
    public ExpressionEvaluationException(string message, int offset = -1)
        : base(offset < 0 ? message : $"{message} at offset {offset}.")
    {
        Offset = offset;
        Reason = message;
    }

    /// <summary>
    /// An error the runtime itself raised while the expression ran (a number
    /// too large for a 32-bit operator, a pattern that does not parse), with
    /// the runtime's own message, as the reference reports it.
    /// </summary>
    public ExpressionEvaluationException(Exception inner)
        : base(inner.Message, inner)
    {
        Offset = -1;
        Reason = inner.Message;
    }

    public int Offset { get; }

    /// <summary>The error as the reference words it, without the offset.</summary>
    public string Reason { get; }

    /// <summary>
    /// The error was raised while a function ran, not while its arguments
    /// were being checked. The reference calls a function through
    /// reflection, so such an error reaches it wrapped, and with debugging
    /// off it reports only the wrapper's message.
    /// </summary>
    public bool RaisedInFunction { get; init; }

    /// <summary>
    /// The error is an argument of the wrong kind, which the reference
    /// finds before it calls the function.
    /// </summary>
    public bool IsArgumentError { get; init; }
}

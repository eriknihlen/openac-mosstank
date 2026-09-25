using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Expressions;

/// <summary>
/// Where the global (<c>&amp;</c>, <c>getgvar</c>) variables live. Every
/// call reaches the shared store, so a value another client wrote a moment
/// ago is the value the next read returns.
/// </summary>
internal interface IGlobalVariableStore
{
    bool TryGet(string name, out ExpressionValue value);
    bool Contains(string name);
    void Set(string name, ExpressionValue value);
    bool Remove(string name);
    void Clear();
    IReadOnlyDictionary<string, ExpressionValue> Capture();
}

/// <summary>
/// Global variables shared by every character on one server, the way the
/// server-wide variables of the macro language behave: all clients on the
/// same server read and write the same set, live. Each variable is its own
/// file, so two clients writing different variables never touch the same
/// file, and a write is one atomic replace of one variable. Every read and
/// write holds a lock that other processes and other sessions in the same
/// process honour, because a file being replaced while another client has
/// it open fails on Windows.
/// </summary>
internal sealed class StorageGlobalVariableStore : IGlobalVariableStore, IDisposable
{
    private const string Root = "expressions/global/";
    private static readonly object ProcessGate = new();

    private readonly IPluginStorage _storage;
    private readonly string _folder;
    private readonly Mutex? _mutex;
    private readonly Func<uint, string?>? _objectNames;

    /// <param name="storage">Where the variables are kept.</param>
    /// <param name="server">The server whose variables these are.</param>
    /// <param name="objectNames">
    /// The client's object names, for a saved world object to print by
    /// (see <see cref="ExpressionValue.WorldObject"/>).
    /// </param>
    public StorageGlobalVariableStore(
        IPluginStorage storage,
        string server,
        Func<uint, string?>? objectNames = null)
    {
        _storage = storage ?? throw new ArgumentNullException(nameof(storage));
        _objectNames = objectNames;
        _folder = Root + Hash(server ?? string.Empty);
        // Storage backed by a folder is shared with every other client that
        // points at the same folder, in this process or another, so the lock
        // is named after that folder. Storage without a folder is only ever
        // this process's own.
        if (storage.RootPath is { } rootPath)
            _mutex = new Mutex(false, MutexName(rootPath, _folder));
    }

    internal string Folder => _folder;

    /// <summary>
    /// Reads one variable. A variable whose file cannot be read back is an
    /// <see cref="ExpressionEvaluationException"/>: the expression reading it
    /// fails, as reading a bad row does in the reference, and nothing else.
    /// </summary>
    public bool TryGet(string name, out ExpressionValue value)
    {
        ExpressionValue found = ExpressionValue.Zero;
        string damage = string.Empty;
        Row row = Locked(() => Read(name, out found, out damage));
        value = found;
        return row switch
        {
            Row.Found => true,
            Row.Damaged => throw new ExpressionEvaluationException(
                $"Global variable {name} could not be read: {damage}"),
            _ => false,
        };
    }

    /// <summary>
    /// A damaged variable still exists, as a bad row still exists in the
    /// reference: the test sees it, and a clear or a set replaces it.
    /// </summary>
    public bool Contains(string name) => Locked(() => Read(name, out _, out _)) != Row.Absent;

    public void Set(string name, ExpressionValue value)
    {
        ArgumentNullException.ThrowIfNull(name);
        string json = JsonSerializer.Serialize(
            new StoredVariable
            {
                Name = name,
                Value = ExpressionValueJson.Store(value),
            },
            ExpressionValueJson.Options);
        Locked(() =>
        {
            _storage.WriteText(KeyFor(name), json);
            // The variable now lives under its own key; a copy saved under
            // the old case-blind key would come back after a clear.
            if (ReadKey(OldKeyFor(name), name, out _, out _) == Row.Found)
                _storage.Delete(OldKeyFor(name));
            return true;
        });
    }

    public bool Remove(string name) => Locked(() =>
    {
        bool removed = false;
        foreach (string key in new[] { KeyFor(name), OldKeyFor(name) })
        {
            if (ReadKey(key, name, out _, out _) != Row.Absent)
                removed |= _storage.Delete(key);
        }
        return removed;
    });

    public void Clear() => Locked(() =>
    {
        foreach (string key in VariableKeys())
            _storage.Delete(key);
        return true;
    });

    /// <summary>
    /// Every variable on this server. One damaged file fails the whole
    /// capture, as one bad row fails the reference's listing.
    /// </summary>
    public IReadOnlyDictionary<string, ExpressionValue> Capture() => Locked(() =>
    {
        var result = new Dictionary<string, ExpressionValue>(StringComparer.Ordinal);
        var ownKeys = new HashSet<string>(StringComparer.Ordinal);
        foreach (string key in VariableKeys())
        {
            string? json = _storage.ReadText(key);
            if (json is null)
                continue;
            if (!TryParse(json, out StoredVariable? stored, out ExpressionValue value, out string damage))
            {
                throw new ExpressionEvaluationException(
                    $"A global variable file could not be read ({Path.GetFileName(key)}): {damage}");
            }
            // A variable under its own key wins over an old case-blind copy.
            bool own = key.Equals(KeyFor(stored.Name), StringComparison.Ordinal);
            if (own)
                ownKeys.Add(stored.Name);
            else if (ownKeys.Contains(stored.Name))
                continue;
            result[stored.Name] = value;
        }
        return (IReadOnlyDictionary<string, ExpressionValue>)result;
    });

    public void Dispose() => _mutex?.Dispose();

    private enum Row
    {
        Absent,
        Found,
        Damaged,
    }

    /// <summary>
    /// Reads one variable by its exact, case-sensitive name: from its own
    /// key, or failing that from the case-blind key earlier versions
    /// saved every variable under.
    /// </summary>
    private Row Read(string name, out ExpressionValue value, out string damage)
    {
        ArgumentNullException.ThrowIfNull(name);
        Row row = ReadKey(KeyFor(name), name, out value, out damage);
        return row != Row.Absent
            ? row
            : ReadKey(OldKeyFor(name), name, out value, out damage);
    }

    private Row ReadKey(string key, string name, out ExpressionValue value, out string damage)
    {
        value = ExpressionValue.Zero;
        damage = string.Empty;
        string? json = _storage.ReadText(key);
        if (json is null)
            return Row.Absent;
        if (!TryParse(json, out StoredVariable? stored, out ExpressionValue restored, out damage))
            return Row.Damaged;
        // The file is keyed by a hash of the name; another name that shares
        // the hash, or the same name in another case, is not this variable.
        if (!stored.Name.Equals(name, StringComparison.Ordinal))
            return Row.Absent;
        value = restored;
        return Row.Found;
    }

    /// <summary>
    /// Reads one variable file back. A file that is not a whole variable
    /// (not JSON, empty, missing its name or value, or holding a value that
    /// cannot be rebuilt) is damaged, and <paramref name="damage"/> says why.
    /// </summary>
    private bool TryParse(
        string json,
        [System.Diagnostics.CodeAnalysis.NotNullWhen(true)] out StoredVariable? stored,
        out ExpressionValue value,
        out string damage) =>
        TryParse(json, _objectNames, out stored, out value, out damage);

    private static bool TryParse(
        string json,
        Func<uint, string?>? objectNames,
        [System.Diagnostics.CodeAnalysis.NotNullWhen(true)] out StoredVariable? stored,
        out ExpressionValue value,
        out string damage)
    {
        value = ExpressionValue.Zero;
        stored = null;
        damage = string.Empty;
        try
        {
            StoredVariable? parsed = JsonSerializer.Deserialize<StoredVariable>(
                json,
                ExpressionValueJson.Options);
            if (string.IsNullOrEmpty(parsed?.Name) || parsed.Value is null)
                throw new JsonException("it holds no variable");
            value = ExpressionValueJson.Restore(parsed.Value, objectNames);
            stored = parsed;
            return true;
        }
        catch (Exception error) when (error is JsonException or OverflowException)
        {
            damage = error.Message;
            return false;
        }
    }

    private IEnumerable<string> VariableKeys() => _storage.List(_folder)
        .Select(key => key.Replace('\\', '/'))
        .Select(key => key.StartsWith(_folder + "/", StringComparison.Ordinal)
            ? key
            : _folder + "/" + key)
        .Where(static key => key.EndsWith(".json", StringComparison.OrdinalIgnoreCase)
            && !Path.GetFileName(key).StartsWith('.'))
        .ToArray();

    /// <summary>
    /// A variable's own file, keyed by its exact name. The "name:" in front
    /// keeps these keys apart from the old ones, which hash the upper-cased
    /// name and so never start with a lower-case letter.
    /// </summary>
    private string KeyFor(string name) =>
        $"{_folder}/{Hash("name:" + name)}.json";

    /// <summary>
    /// Where earlier versions, whose names ignored case, saved a variable.
    /// </summary>
    private string OldKeyFor(string name) =>
        $"{_folder}/{Hash(name.ToUpperInvariant())}.json";

    /// <summary>
    /// Runs one store operation under the shared lock. A storage failure (a
    /// file another program holds, a folder the client may not write)
    /// becomes an <see cref="ExpressionEvaluationException"/>, so it fails
    /// the one expression or command that met it, as a failing database
    /// call does in the reference.
    /// </summary>
    private T Locked<T>(Func<T> action)
    {
        try
        {
            return LockedCore(action);
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            throw new ExpressionEvaluationException(
                $"The global variables could not be read or written: {error.Message}");
        }
    }

    private T LockedCore<T>(Func<T> action)
    {
        if (_mutex is null)
        {
            lock (ProcessGate)
                return action();
        }
        try
        {
            _mutex.WaitOne();
        }
        catch (AbandonedMutexException)
        {
            // The wait still succeeded: the previous holder exited while it
            // held the lock. Every write is one atomic file replace, so what
            // it left behind is whole.
        }
        try
        {
            return action();
        }
        finally
        {
            _mutex.ReleaseMutex();
        }
    }

    private static string MutexName(string rootPath, string folder)
    {
        string path = Path.GetFullPath(Path.Combine(
            rootPath,
            folder.Replace('/', Path.DirectorySeparatorChar)));
        if (OperatingSystem.IsWindows())
            path = path.ToUpperInvariant();
        // "Global" makes the name machine-wide, so clients started from
        // different logins or terminals still meet on the same lock.
        return @"Global\MossTank.GlobalVariables." + Hash(path);
    }

    private static string Hash(string text) => Convert.ToHexString(
        SHA256.HashData(Encoding.UTF8.GetBytes(text)).AsSpan(0, 12))
        .ToLowerInvariant();

    private sealed class StoredVariable
    {
        public string Name { get; set; } = string.Empty;
        public ExpressionValueJson.StoredValue Value { get; set; } = new();
    }
}

/// <summary>
/// The JSON shape expression values are saved in, shared by the
/// character's and the server's variable stores.
/// </summary>
internal static class ExpressionValueJson
{
    /// <remarks>
    /// A number that is not finite (0/0, 1/0) is a value like any other and
    /// is saved as the reference saves it, so it is written and read by name
    /// ("NaN", "Infinity") rather than refused.
    /// </remarks>
    internal static readonly JsonSerializerOptions Options = new()
    {
        WriteIndented = true,
        PropertyNameCaseInsensitive = true,
        NumberHandling = System.Text.Json.Serialization.JsonNumberHandling
            .AllowNamedFloatingPointLiterals,
    };

    /// <summary>
    /// Refuses a value that cannot be saved, worded as the reference refuses
    /// it: a stopwatch or a UI control, or a list or dictionary holding one.
    /// </summary>
    internal static void RequireSavable(in ExpressionValue value) => Store(value);

    internal static string Serialize(in ExpressionValue value) =>
        JsonSerializer.Serialize(Store(value), Options);

    internal static StoredValue Store(in ExpressionValue value) => value.Kind switch
    {
        ExpressionValueKind.Number => new StoredValue
        {
            Kind = "number",
            Number = value.AsNumber(),
        },
        ExpressionValueKind.Boolean => new StoredValue
        {
            Kind = "boolean",
            Number = value.AsNumber(),
        },
        ExpressionValueKind.String => new StoredValue
        {
            Kind = "string",
            Text = value.AsString(),
        },
        ExpressionValueKind.List => new StoredValue
        {
            Kind = "list",
            List = value.AsList().Items.Select(static item => Store(item)).ToList(),
        },
        ExpressionValueKind.Dictionary => new StoredValue
        {
            Kind = "dictionary",
            Dictionary = value.AsDictionary().Items.ToDictionary(
                static pair => pair.Key,
                static pair => Store(pair.Value),
                StringComparer.Ordinal),
        },
        ExpressionValueKind.Coordinates => StoreCoordinates(value.AsCoordinates()),
        ExpressionValueKind.WorldObject => new StoredValue
        {
            Kind = "worldobject",
            Number = value.AsObjectId(),
        },
        _ => throw new ExpressionEvaluationException(
            "There is currently no support for serializing "
            + (value.Kind == ExpressionValueKind.UiControl ? "UIControl" : value.Kind.ToString())
            + " types"),
    };

    /// <summary>
    /// Rebuilds a saved value. A part the JSON left null that a saved value
    /// always has is a damaged file: <see cref="JsonException"/>. So is a
    /// world object id out of range (<see cref="OverflowException"/>).
    /// </summary>
    internal static ExpressionValue Restore(
        StoredValue? value,
        Func<uint, string?>? objectNames = null) =>
        (value?.Kind ?? throw new JsonException("a saved value has no kind"))
            .ToLowerInvariant() switch
        {
            "number" => ExpressionValue.Number(value.Number),
            "boolean" => ExpressionValue.Boolean(value.Number != 0d),
            "string" => ExpressionValue.String(
                value.Text ?? throw new JsonException("a saved string has no text")),
            "list" => ExpressionValue.List(new ExpressionList(
                (value.List ?? []).Select(item => Restore(item, objectNames)))),
            "dictionary" => RestoreDictionary(value.Dictionary, objectNames),
            "coordinates" => RestoreCoordinates(value.Coordinates),
            "worldobject" => ExpressionValue.WorldObject(
                checked((uint)value.Number),
                objectNames),
            _ => ExpressionValue.Zero,
        };

    private static StoredValue StoreCoordinates(in ExpressionCoordinates value) => new()
    {
        Kind = "coordinates",
        Coordinates =
        [
            value.EastWest,
            value.NorthSouth,
            value.Elevation,
        ],
    };

    private static ExpressionValue RestoreDictionary(
        Dictionary<string, StoredValue>? values,
        Func<uint, string?>? objectNames)
    {
        var result = new ExpressionDictionary();
        if (values is not null)
        {
            foreach ((string key, StoredValue? value) in values)
                result.Items[key] = Restore(value, objectNames);
        }
        return ExpressionValue.Dictionary(result);
    }

    private static ExpressionValue RestoreCoordinates(double[]? values) =>
        values is { Length: >= 2 }
            ? ExpressionValue.Coordinates(new ExpressionCoordinates(
                values[0],
                values[1],
                values.Length >= 3 ? values[2] : 0d))
            : ExpressionValue.Zero;

    internal sealed class StoredValue
    {
        public string Kind { get; set; } = "number";
        public double Number { get; set; }
        public string Text { get; set; } = string.Empty;
        public List<StoredValue>? List { get; set; }
        public Dictionary<string, StoredValue>? Dictionary { get; set; }
        public double[]? Coordinates { get; set; }
    }
}

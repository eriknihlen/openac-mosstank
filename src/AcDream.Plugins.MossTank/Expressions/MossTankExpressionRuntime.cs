using System.Globalization;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Expressions;

internal sealed class MossTankExpressionRuntime : IDisposable
{
    private const int DefaultInstructionBudget = 10_000;

    /// <summary>
    /// How many repaired texts are remembered as already reported. A meta
    /// that builds its code strings as it runs makes a new text each time,
    /// so the record is emptied when it is full, and a text met again after
    /// that is reported again.
    /// </summary>
    internal const int ReportedRepairLimit = 256;

    private readonly IPluginHost _host;
    private readonly ExpressionState _state = new();
    private readonly HashSet<string> _reportedRepairs = new(StringComparer.Ordinal);
    private readonly ExpressionFunctionRegistry _functions;
    private readonly ExpressionHostPolicy _policy = new();
    private readonly ExperienceMeter _experience;
    private readonly QuestTracker _quests;
    private readonly SalvageStagingManager _salvage;
    private readonly StatusHudManager _statusHud;
    private readonly List<DelayedExpression> _delayed = [];
    private int _nextDelayId = 1;
    private string _identity = string.Empty;
    private string? _persistentJson;
    // The persistent-variable writes and collection changes the last save saw.
    private long _savedWrites = -1;
    private long _savedCollectionChanges = -1;
    // The persistent variables as last saved, one by one.
    private Dictionary<string, ExpressionValueJson.StoredValue> _savedDocument =
        new(StringComparer.Ordinal);
    private StorageGlobalVariableStore? _globalStore;
    private bool _disposed;

    public MossTankExpressionRuntime(IPluginHost host, Random? random = null)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _experience = new ExperienceMeter(host);
        _quests = new QuestTracker(host);
        _salvage = new SalvageStagingManager(host);
        _statusHud = new StatusHudManager(host);
        _functions = CoreExpressionFunctions.CreateDefault(
            random,
            id => host.Automation.Objects.TryGet(id, out _),
            line => UbChat.Post(host.Automation.Chat, UbChat.Line(line)));
        HeldMotions = new HeldMotions(host);
        HostExpressionFunctions.Register(_functions, host, _policy, HeldMotions);
        RegisterExperienceFunctions();
        RegisterQuestFunctions();
        RegisterSalvageFunctions();
        RegisterStatusHudFunctions();
        RegisterExecutionFunctions();
        BindIdentity(force: true);
    }

    public ExpressionState State => _state;

    /// <summary>
    /// Profile-owned hooks the built-ins consult; the owner wires them once.
    /// </summary>
    internal ExpressionHostPolicy Policy => _policy;

    /// <summary>
    /// The movement keys a macro holds. The setmotion command and the
    /// setmotion[] expression share it, so either can release what the other
    /// pressed.
    /// </summary>
    internal HeldMotions HeldMotions { get; }

    internal ExpressionFunctionRegistry Registry => _functions;
    public IReadOnlyCollection<ExpressionFunction> Functions => _functions.Functions;
    public int PendingExecutionCount => _delayed.Count;

    public ExpressionValue Evaluate(
        string source,
        int instructionBudget = DefaultInstructionBudget,
        CancellationToken cancellationToken = default)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        BindIdentity(force: false);
        ExpressionProgram program = ExpressionProgram.Compile(source);
        if (program.Repairs.Count != 0)
            ReportRepairs(source, program.Repairs);
        var context = new ExpressionEvaluationContext(
            _state,
            _functions,
            instructionBudget,
            cancellationToken)
        {
            RepairsObserved = ReportRepairs,
            RunFailed = ReportFailedRun,
            IsKnownObject = id => _host.Automation.Objects.TryGet(id, out _),
        };
        ExpressionValue result;
        bool completed = false;
        _state.BeginEvaluation();
        try
        {
            result = program.Evaluate(context);
            completed = true;
        }
        finally
        {
            _state.EndEvaluation(completed);
            // What the expression set before an error stands, and the
            // reference saves a persistent variable as it is set, so the
            // save happens whether or not the expression completed.
            FlushVariables();
        }
        return result;
    }

    /// <summary>
    /// Logs, once per distinct expression, what the reader dropped, skipped
    /// or supplied to read it. The reference parser makes these repairs
    /// without a word, and this reader makes the same ones, so a meta means
    /// what it meant there; the log line is so that an author whose text
    /// does not do what it says can see why.
    /// </summary>
    private void ReportRepairs(string source, IReadOnlyList<string> repairs)
    {
        if (_reportedRepairs.Contains(source))
            return;
        if (_reportedRepairs.Count >= ReportedRepairLimit)
            _reportedRepairs.Clear();
        _reportedRepairs.Add(source);
        _host.Log.Warn(
            $"Expression read the way UtilityBelt reads it: {string.Join("; ", repairs)}. Expression: {source}");
    }

    /// <summary>
    /// A code string's run failed and answered 0 (see
    /// <see cref="ExpressionEvaluationContext.RunSeparately"/>). The
    /// reference reports it every time as two UtilityBelt error lines, which
    /// its log always gets and chat gets through the error display:
    /// the code as its parser gives it back, then the reason. With the debug
    /// setting on the reason is the whole exception. With it off it is the
    /// message, and an error raised inside a function reaches the reference
    /// wrapped by the reflection call, so it is the wrapper's message.
    /// </summary>
    private void ReportFailedRun(string text, Exception error)
    {
        string reason = _policy.Debug?.Invoke() == true
            ? error.ToString()
            : error switch
            {
                ExpressionEvaluationException { RaisedInFunction: true } =>
                    "Exception has been thrown by the target of an invocation.",
                ExpressionEvaluationException evaluation => evaluation.Reason,
                _ => "Exception has been thrown by the target of an invocation.",
            };
        foreach (string line in new[] { $"Error running string expression: {text}", reason })
        {
            string message = UbChat.Error(line);
            UbChat.Post(_host.Automation.Chat, message);
            _host.Log.Info(message);
        }
    }

    public void OnTick(double elapsedSeconds)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        BindIdentity(force: false);
        if (elapsedSeconds < 0d || !double.IsFinite(elapsedSeconds))
            throw new ArgumentOutOfRangeException(nameof(elapsedSeconds));
        _experience.OnTick(elapsedSeconds);
        _quests.OnTick(elapsedSeconds);
        if (_delayed.Count == 0)
            return;

        double elapsedMilliseconds = elapsedSeconds * 1000d;
        for (int index = 0; index < _delayed.Count; index++)
            _delayed[index] = _delayed[index] with
            {
                RemainingMilliseconds =
                    _delayed[index].RemainingMilliseconds - elapsedMilliseconds,
            };

        DelayedExpression[] ready = _delayed
            .Where(static delayed => delayed.RemainingMilliseconds <= 0d)
            .OrderBy(static delayed => delayed.Id)
            .ToArray();
        if (ready.Length == 0)
            return;
        _delayed.RemoveAll(static delayed => delayed.RemainingMilliseconds <= 0d);
        foreach (DelayedExpression delayed in ready)
        {
            try
            {
                Evaluate(delayed.Source);
            }
            catch (Exception error)
            {
                _host.Log.Error(
                    $"Delayed expression {delayed.Id} failed: {error.Message}");
            }
        }
    }

    public void ClearSession()
    {
        _state.Clear(ExpressionVariableScope.Session);
        _delayed.Clear();
    }

    public void DestroyAuxiliaryViews() => _statusHud.Destroy();

    /// <summary>
    /// Asks the server for the character's quest list again, so the quest
    /// functions read what is true now rather than what was true at login.
    /// A refresh already under way starts over.
    /// </summary>
    public void RefreshQuests() => _quests.Refresh(restart: true);

    public void Dispose()
    {
        if (_disposed)
            return;
        FlushVariables();
        _delayed.Clear();
        _statusHud.Destroy();
        _state.BindGlobalStore(null);
        _globalStore?.Dispose();
        _globalStore = null;
        _quests.Dispose();
        _disposed = true;
    }

    private void RegisterExecutionFunctions()
    {
        _functions.Register("exec", 1, 1, (context, args) =>
        {
            string source = args[0].AsString("exec");
            return context.RunSeparately(context.Compile(source), out _);
        }, "exec[expression]");
        _functions.Register("delayexec", 2, 2, (_, args) =>
        {
            double delay = Math.Max(0d, args[0].AsNumber("delayexec"));
            string source = args[1].AsString("delayexec");
            int id = NextDelayId();
            _delayed.Add(new DelayedExpression(id, delay, source));
            return ExpressionValue.Number(id);
        }, "delayexec[milliseconds,expression]");
        _functions.Register("clearexec", 1, 1, (_, args) =>
        {
            int id = args[0].AsInt32("clearexec");
            return ExpressionValue.Boolean(
                _delayed.RemoveAll(delayed => delayed.Id == id) != 0);
        }, "clearexec[id]");
    }

    private void RegisterExperienceFunctions()
    {
        _functions.Register("xpreset", 0, 0, (_, _) =>
        {
            _experience.Reset();
            return ExpressionValue.One;
        }, "xpreset[]");
        _functions.Register("xpmeter", 0, 0, (_, _) =>
            ExpressionValue.String(_experience.Format()), "xpmeter[]");
        _functions.Register("xpduration", 0, 0, (_, _) =>
            ExpressionValue.Number(_experience.DurationSeconds), "xpduration[]");
        _functions.Register("xptotal", 0, 0, (_, _) =>
            ExpressionValue.Number(_experience.Experience), "xptotal[]");
        _functions.Register("lumtotal", 0, 0, (_, _) =>
            ExpressionValue.Number(_experience.Luminance), "lumtotal[]");
        _functions.Register("xpavg", 0, 0, (_, _) =>
            ExpressionValue.Number(_experience.ExperiencePerHour), "xpavg[]");
        _functions.Register("lumavg", 0, 0, (_, _) =>
            ExpressionValue.Number(_experience.LuminancePerHour), "lumavg[]");
    }

    private void RegisterQuestFunctions()
    {
        _functions.Register("testquestflag", 1, 1, (_, args) =>
            ExpressionValue.Boolean(_quests.HasCompleted(
                args[0].AsString("testquestflag"))), "testquestflag[questflag]");
        _functions.Register("getqueststatus", 1, 1, (_, args) =>
            ExpressionValue.Boolean(_quests.IsReady(
                args[0].AsString("getqueststatus"))), "getqueststatus[questflag]");
        _functions.Register("getquestktprogress", 1, 1, (_, args) =>
            ExpressionValue.Number(_quests.Progress(
                args[0].AsString("getquestktprogress"))),
            "getquestktprogress[questflag]");
        _functions.Register("getquestktrequired", 1, 1, (_, args) =>
            ExpressionValue.Number(_quests.Required(
                args[0].AsString("getquestktrequired"))),
            "getquestktrequired[questflag]");
        _functions.Register("isrefreshingquests", 0, 0, (_, _) =>
            ExpressionValue.Boolean(_quests.IsRefreshing), "isrefreshingquests[]");
    }

    private void RegisterSalvageFunctions()
    {
        _functions.Register("ustadd", 1, 1, (_, args) =>
            ExpressionValue.Boolean(_salvage.Add(HostExpressionFunctions.ObjectArgument(
                _host.Automation.Objects, args, 0, "ustadd[WorldObject]"))), "ustadd[object]");
        _functions.Register("ustopen", 0, 0, (_, _) =>
            ExpressionValue.Boolean(_salvage.Open()), "ustopen[]");
        _functions.Register("ustsalvage", 0, 0, (_, _) =>
            ExpressionValue.Boolean(_salvage.Salvage()), "ustsalvage[]");
    }

    private void RegisterStatusHudFunctions()
    {
        _functions.Register("statushud", 2, 2, (_, args) =>
            ExpressionValue.Boolean(_statusHud.Update(
                args[0].AsString("statushud"),
                args[1].ToDisplayString())),
            "statushud[key,value]");
        // The colour is taken as a 32-bit pattern, so a profile that writes
        // its colour as a negative number gets the colour it meant rather
        // than an error.
        _functions.Register("statushudcolored", 3, 3, (_, args) =>
            ExpressionValue.Boolean(_statusHud.Update(
                args[0].AsString("statushudcolored"),
                args[1].ToDisplayString(),
                unchecked((uint)(long)args[2].AsNumber("statushudcolored")))),
            "statushudcolored[key,value,rgb]");
    }

    private int NextDelayId()
    {
        int initial = _nextDelayId;
        do
        {
            int candidate = _nextDelayId++;
            if (_nextDelayId <= 0)
                _nextDelayId = 1;
            if (_delayed.All(delayed => delayed.Id != candidate))
                return candidate;
        }
        while (_nextDelayId != initial);
        throw new ExpressionEvaluationException("No delayed-expression ids remain");
    }

    private void BindIdentity(bool force)
    {
        ICharacterInfo character = _host.Automation.Character;
        string identity = string.Join(
            '\n',
            character.WorldName,
            character.AccountName,
            character.Name);
        if (!force && identity.Equals(_identity, StringComparison.Ordinal))
            return;
        if (_identity.Length != 0)
            FlushVariables();
        _identity = identity;
        _quests.BindIdentity(identity);
        _salvage.Clear();
        _state.Clear(ExpressionVariableScope.Session);
        _delayed.Clear();
        _experience.Reset();
        _persistentJson = LoadScope(ExpressionVariableScope.Persistent);
        MarkPersistentSaved();
        BindGlobalStore(character);
    }

    /// <summary>
    /// Global variables belong to the server, not to one account: every
    /// client on the same server reads and writes one shared set, and each
    /// read reaches the shared store so another client's write shows at once.
    /// </summary>
    private void BindGlobalStore(ICharacterInfo character)
    {
        _state.BindGlobalStore(null);
        _state.Clear(ExpressionVariableScope.Global);
        _globalStore?.Dispose();
        _globalStore = null;
        if (!_host.Storage.IsAvailable)
            return;
        try
        {
            _globalStore = new StorageGlobalVariableStore(
                _host.Storage,
                character.WorldName,
                HostExpressionFunctions.ObjectNames(_host));
        }
        catch (Exception error)
        {
            _host.Log.Error($"Unable to open the global expression variables: {error.Message}");
            return;
        }
        _state.BindGlobalStore(_globalStore);
        ImportAccountGlobals();
    }

    /// <summary>
    /// Earlier versions kept the global variables in one file per server and
    /// account. Their values join the shared set without replacing a value it
    /// already has, and the old file goes.
    /// </summary>
    private void ImportAccountGlobals()
    {
        string key = StorageKey(ExpressionVariableScope.Global);
        try
        {
            string? json = _host.Storage.ReadText(key);
            if (json is null)
                return;
            Dictionary<string, ExpressionValueJson.StoredValue>? document =
                string.IsNullOrWhiteSpace(json)
                    ? null
                    : JsonSerializer.Deserialize<
                        Dictionary<string, ExpressionValueJson.StoredValue>>(
                        json,
                        ExpressionValueJson.Options);
            foreach ((string name, ExpressionValueJson.StoredValue value)
                in document ?? [])
            {
                if (!_globalStore!.Contains(name))
                    _globalStore.Set(name, ExpressionValueJson.Restore(value, HostExpressionFunctions.ObjectNames(_host)));
            }
            _host.Storage.Delete(key);
        }
        catch (Exception error)
        {
            _host.Log.Error($"Unable to move the old global expression variables: {error.Message}");
        }
    }

    private string? LoadScope(ExpressionVariableScope scope)
    {
        _state.Clear(scope);
        _savedDocument = new Dictionary<string, ExpressionValueJson.StoredValue>(
            StringComparer.Ordinal);
        if (!_host.Storage.IsAvailable || _identity.Length == 0)
            return null;
        try
        {
            string? json = _host.Storage.ReadText(StorageKey(scope));
            if (string.IsNullOrWhiteSpace(json))
                return null;
            Dictionary<string, ExpressionValueJson.StoredValue>? document =
                JsonSerializer.Deserialize<
                    Dictionary<string, ExpressionValueJson.StoredValue>>(
                    json,
                    ExpressionValueJson.Options);
            if (document is not null)
            {
                _savedDocument = new Dictionary<string, ExpressionValueJson.StoredValue>(
                    document, StringComparer.Ordinal);
                _state.Replace(scope, document.Select(pair =>
                    new KeyValuePair<string, ExpressionValue>(
                        pair.Key,
                        ExpressionValueJson.Restore(pair.Value, HostExpressionFunctions.ObjectNames(_host)))));
            }
            return json;
        }
        catch (Exception error)
        {
            _host.Log.Error($"Unable to load {scope} expression variables: {error.Message}");
            return null;
        }
    }

    /// <summary>
    /// Saves the persistent variables when something may have changed since
    /// the last save: a set or clear of one, or an in-place change of a list
    /// or dictionary while one of them holds a list or dictionary. An
    /// evaluation that changed nothing costs no save.
    /// </summary>
    private void FlushVariables()
    {
        if (!_host.Storage.IsAvailable || _identity.Length == 0)
            return;
        if (_state.PersistentWrites == _savedWrites
            && (ExpressionCollectionChanges.Count == _savedCollectionChanges
                || !_state.HoldsCollection(ExpressionVariableScope.Persistent)))
        {
            return;
        }
        long writes = _state.PersistentWrites;
        long changes = ExpressionCollectionChanges.Count;
        if (FlushScope(ExpressionVariableScope.Persistent, _persistentJson, out string json))
        {
            _persistentJson = json;
            _savedWrites = writes;
            _savedCollectionChanges = changes;
        }
    }

    private void MarkPersistentSaved()
    {
        _savedWrites = _state.PersistentWrites;
        _savedCollectionChanges = ExpressionCollectionChanges.Count;
    }

    private bool FlushScope(ExpressionVariableScope scope, string? previous, out string json)
    {
        try
        {
            var document = new Dictionary<string, ExpressionValueJson.StoredValue>(
                StringComparer.Ordinal);
            foreach ((string name, ExpressionValue value) in _state.Capture(scope))
            {
                try
                {
                    document[name] = ExpressionValueJson.Store(value);
                }
                catch (ExpressionEvaluationException error)
                {
                    // A set refuses what cannot be saved, but a list or
                    // dictionary changed in place can come to hold it. That
                    // one variable keeps the form it was last saved in; the
                    // others are saved all the same.
                    if (_savedDocument.TryGetValue(name, out ExpressionValueJson.StoredValue? saved))
                        document[name] = saved;
                    _host.Log.Error(
                        $"Unable to save {scope} expression variable {name}: {error.Message}");
                }
            }
            json = JsonSerializer.Serialize(document, ExpressionValueJson.Options);
            if (!json.Equals(previous, StringComparison.Ordinal))
                _host.Storage.WriteText(StorageKey(scope), json);
            _savedDocument = document;
            return true;
        }
        catch (Exception error)
        {
            _host.Log.Error($"Unable to save {scope} expression variables: {error.Message}");
            json = previous ?? string.Empty;
            return false;
        }
    }

    /// <summary>
    /// The file for the persistent variables of the character they were
    /// loaded for, which is the one they are saved to even once the client
    /// has moved on to another character; for the global scope, the
    /// per-account file earlier versions kept.
    /// </summary>
    private string StorageKey(ExpressionVariableScope scope)
    {
        ICharacterInfo character = _host.Automation.Character;
        string owner = scope == ExpressionVariableScope.Persistent
            ? _identity
            : string.Join('\n', character.WorldName, character.AccountName);
        byte[] hash = SHA256.HashData(Encoding.UTF8.GetBytes(owner));
        return $"expressions/{scope.ToString().ToLowerInvariant()}/"
            + $"{Convert.ToHexString(hash.AsSpan(0, 12)).ToLowerInvariant()}.json";
    }

    private readonly record struct DelayedExpression(
        int Id,
        double RemainingMilliseconds,
        string Source);
}

namespace AcDream.Plugins.MossTank.Expressions;

internal enum ExpressionVariableScope
{
    Session,
    Persistent,
    Global,
}

internal sealed class ExpressionState
{
    // Variable names of every scope are case-sensitive, as the reference's
    // are: `R` and `r` are two variables.
    private readonly Dictionary<string, ExpressionValue> _session =
        new(StringComparer.Ordinal);
    private readonly Dictionary<string, ExpressionValue> _persistent =
        new(StringComparer.Ordinal);
    private readonly Dictionary<string, ExpressionValue> _global =
        new(StringComparer.Ordinal);
    private readonly Stack<Dictionary<string, ExpressionValue>> _globalReads = new();
    private IGlobalVariableStore? _globalStore;

    /// <summary>
    /// Sends the global scope to a shared store instead of this state's own
    /// table; null goes back to the table.
    /// </summary>
    internal void BindGlobalStore(IGlobalVariableStore? store)
    {
        _globalStore = store;
        foreach (Dictionary<string, ExpressionValue> reads in _globalReads)
            reads.Clear();
    }

    /// <summary>
    /// Opens one top-level evaluation. Within it a global variable is read
    /// from the shared store once, by the shorthand or by getgvar alike, and
    /// the same value is handed back after, so a list the expression changes
    /// in place is the list it reads again. A set, a clear or a clear-all
    /// writes the store and leaves that read alone: a later read in the same
    /// expression still returns it.
    /// </summary>
    internal void BeginEvaluation() =>
        _globalReads.Push(new Dictionary<string, ExpressionValue>(
            StringComparer.Ordinal));

    /// <summary>
    /// Closes the evaluation <see cref="BeginEvaluation"/> opened. When it
    /// completed, every list or dictionary it read from the shared store and
    /// then changed (<see cref="ExpressionList.HasChanges"/>) is written
    /// back. This runs after everything the expression did, so it overwrites
    /// a set or a clear of the same variable made after the change, exactly
    /// as the end-of-run write-back of the reference does.
    /// </summary>
    internal void EndEvaluation(bool completed)
    {
        Dictionary<string, ExpressionValue> reads = _globalReads.Pop();
        if (!completed || _globalStore is not { } store)
            return;
        foreach ((string name, ExpressionValue value) in reads)
        {
            bool changed = value.Kind switch
            {
                ExpressionValueKind.List => value.AsList().HasChanges,
                ExpressionValueKind.Dictionary => value.AsDictionary().HasChanges,
                _ => false,
            };
            if (changed)
                store.Set(name, value);
        }
    }

    public ExpressionValue Get(ExpressionVariableScope scope, string name)
    {
        if (scope == ExpressionVariableScope.Global && _globalStore is { } store)
            return GetGlobal(store, name);
        return Table(scope).TryGetValue(name, out ExpressionValue value)
            ? value
            : ExpressionValue.Zero;
    }

    public bool Contains(ExpressionVariableScope scope, string name) =>
        scope == ExpressionVariableScope.Global && _globalStore is { } store
            ? store.Contains(name)
            : Table(scope).ContainsKey(name);

    public ExpressionValue Set(
        ExpressionVariableScope scope,
        string name,
        ExpressionValue value)
    {
        if (scope == ExpressionVariableScope.Global && _globalStore is { } store)
        {
            store.Set(name, value);
            return value;
        }
        // A persistent variable is saved as it is set, so a value that
        // cannot be saved is refused here and nothing is stored, as the
        // reference refuses it.
        if (scope == ExpressionVariableScope.Persistent)
            ExpressionValueJson.RequireSavable(value);
        Table(scope)[name] = value;
        Written(scope);
        return value;
    }

    public bool Clear(ExpressionVariableScope scope, string name)
    {
        if (scope == ExpressionVariableScope.Global && _globalStore is { } store)
            return store.Remove(name);
        Written(scope);
        return Table(scope).Remove(name);
    }

    public void Clear(ExpressionVariableScope scope)
    {
        if (scope == ExpressionVariableScope.Global && _globalStore is { } store)
        {
            store.Clear();
            return;
        }
        Table(scope).Clear();
        Written(scope);
    }

    public IReadOnlyDictionary<string, ExpressionValue> Capture(
        ExpressionVariableScope scope) =>
        scope == ExpressionVariableScope.Global && _globalStore is { } store
            ? store.Capture()
            : new Dictionary<string, ExpressionValue>(Table(scope),
                Table(scope).Comparer);

    public void Replace(
        ExpressionVariableScope scope,
        IEnumerable<KeyValuePair<string, ExpressionValue>> values)
    {
        if (scope == ExpressionVariableScope.Global && _globalStore is { } store)
        {
            store.Clear();
            foreach ((string name, ExpressionValue value) in values)
                store.Set(name, value);
            return;
        }
        Dictionary<string, ExpressionValue> target = Table(scope);
        target.Clear();
        foreach ((string name, ExpressionValue value) in values)
            target[name] = value;
        Written(scope);
    }

    /// <summary>
    /// Counts the sets and clears of the persistent variables, so their
    /// saver can tell whether anything was set or cleared since it saved.
    /// A change inside a list or dictionary one of them holds is counted by
    /// <see cref="ExpressionCollectionChanges"/> instead.
    /// </summary>
    internal long PersistentWrites { get; private set; }

    /// <summary>Whether a variable of the scope holds a list or a dictionary.</summary>
    internal bool HoldsCollection(ExpressionVariableScope scope) =>
        Table(scope).Values.Any(static value =>
            value.Kind is ExpressionValueKind.List or ExpressionValueKind.Dictionary);

    private void Written(ExpressionVariableScope scope)
    {
        if (scope == ExpressionVariableScope.Persistent)
            PersistentWrites++;
    }

    private ExpressionValue GetGlobal(IGlobalVariableStore store, string name)
    {
        if (_globalReads.Count != 0
            && _globalReads.Peek().TryGetValue(name, out ExpressionValue cached))
        {
            return cached;
        }
        store.TryGet(name, out ExpressionValue value);
        if (_globalReads.Count != 0)
            _globalReads.Peek()[name] = value;
        return value;
    }

    private Dictionary<string, ExpressionValue> Table(
        ExpressionVariableScope scope) => scope switch
    {
        ExpressionVariableScope.Session => _session,
        ExpressionVariableScope.Persistent => _persistent,
        ExpressionVariableScope.Global => _global,
        _ => throw new ArgumentOutOfRangeException(nameof(scope)),
    };
}

internal delegate ExpressionValue ExpressionFunctionHandler(
    ExpressionEvaluationContext context,
    IReadOnlyList<ExpressionValue> arguments);

internal sealed record ExpressionFunction(
    string Name,
    int MinimumArguments,
    int MaximumArguments,
    ExpressionFunctionHandler Handler,
    string Signature,
    string Description);

internal sealed class ExpressionFunctionRegistry
{
    // Function names are case-sensitive, as the reference's are.
    private readonly Dictionary<string, ExpressionFunction> _functions =
        new(StringComparer.Ordinal);

    public IReadOnlyCollection<ExpressionFunction> Functions =>
        _functions.Values;

    public void Register(
        string name,
        int minimumArguments,
        int maximumArguments,
        ExpressionFunctionHandler handler,
        string? signature = null,
        string description = "")
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(name);
        ArgumentNullException.ThrowIfNull(handler);
        if (minimumArguments < 0 || maximumArguments < minimumArguments)
            throw new ArgumentOutOfRangeException(nameof(minimumArguments));
        var function = new ExpressionFunction(
            name,
            minimumArguments,
            maximumArguments,
            handler,
            signature ?? name + "[...]",
            description);
        if (!_functions.TryAdd(name, function))
        {
            throw new InvalidOperationException(
                $"Expression function '{name}' is already registered.");
        }
    }

    public void Alias(string alias, string existing)
    {
        if (!_functions.TryGetValue(existing, out ExpressionFunction? function))
            throw new InvalidOperationException(
                $"Expression function '{existing}' is not registered.");
        Register(
            alias,
            function.MinimumArguments,
            function.MaximumArguments,
            function.Handler,
            function.Signature.Replace(existing, alias, StringComparison.Ordinal),
            function.Description);
    }

    public ExpressionFunction Resolve(string name, int offset)
    {
        if (_functions.TryGetValue(name, out ExpressionFunction? function))
            return function;
        throw new ExpressionEvaluationException(
            $"Unknown expression method: {name}",
            offset);
    }
}

internal sealed class ExpressionEvaluationContext
{
    private int _remainingInstructions;

    public ExpressionEvaluationContext(
        ExpressionState state,
        ExpressionFunctionRegistry functions,
        int instructionBudget = 10_000,
        CancellationToken cancellationToken = default)
    {
        State = state ?? throw new ArgumentNullException(nameof(state));
        Functions = functions ?? throw new ArgumentNullException(nameof(functions));
        if (instructionBudget <= 0)
            throw new ArgumentOutOfRangeException(nameof(instructionBudget));
        _remainingInstructions = instructionBudget;
        CancellationToken = cancellationToken;
    }

    public ExpressionState State { get; }
    public ExpressionFunctionRegistry Functions { get; }

    /// <summary>
    /// Told about an expression the reader had to repair (see
    /// <see cref="ExpressionProgram.Repairs"/>), with what it did.
    /// </summary>
    public Action<string, IReadOnlyList<string>>? RepairsObserved { get; init; }

    /// <summary>
    /// Compiles an expression met while evaluating (a code string handed to
    /// ifthen, exec or a list function), reporting any repair.
    /// </summary>
    public ExpressionProgram Compile(string source)
    {
        ExpressionProgram program = ExpressionProgram.Compile(source);
        if (program.Repairs.Count != 0)
            RepairsObserved?.Invoke(source, program.Repairs);
        return program;
    }
    public CancellationToken CancellationToken { get; }
    public int RemainingInstructions => _remainingInstructions;

    /// <summary>
    /// Told about a code string whose run failed (see
    /// <see cref="RunSeparately"/>): the code as the reference's parser
    /// gives it back (<see cref="ExpressionProgram.Text"/>) and the error
    /// that ended it.
    /// </summary>
    public Action<string, Exception>? RunFailed { get; init; }

    /// <summary>
    /// Whether the client knows an object id, for the reference's argument
    /// check (see <see cref="ReferenceArguments"/>); null takes every id as
    /// known.
    /// </summary>
    public Func<uint, bool>? IsKnownObject { get; init; }

    /// <summary>
    /// Set once the instruction budget ran out. From then on every error is
    /// the budget's, and no run swallows it.
    /// </summary>
    private bool _budgetExhausted;

    public void Step(int offset)
    {
        CancellationToken.ThrowIfCancellationRequested();
        if (--_remainingInstructions < 0)
        {
            _budgetExhausted = true;
            throw new ExpressionEvaluationException(
                "Expression instruction budget exceeded",
                offset);
        }
    }

    /// <summary>
    /// Runs a code string handed to exec, ifthen or a list function the way
    /// the reference does: as a run of its own, not as part of the
    /// expression that holds it. The run keeps its own record of the global
    /// variables it reads, so it reads them fresh rather than the holding
    /// expression's copies, and when it ends it writes back the lists and
    /// dictionaries it changed. An error ends only this run: it is reported,
    /// what the run did before it stands, and the run answers 0 with
    /// <paramref name="failed"/> set, for the caller to treat as the
    /// reference treats its failed-run answer. Only running out of the
    /// instruction budget, or being cancelled, ends the holding expression.
    /// </summary>
    public ExpressionValue RunSeparately(
        ExpressionProgram program,
        out bool failed)
    {
        ArgumentNullException.ThrowIfNull(program);
        failed = false;
        State.BeginEvaluation();
        bool open = true;
        try
        {
            ExpressionValue result = program.Evaluate(this);
            open = false;
            State.EndEvaluation(completed: true);
            return result;
        }
        catch (Exception error) when (!_budgetExhausted
            && error is not OperationCanceledException)
        {
            if (open)
            {
                open = false;
                State.EndEvaluation(completed: false);
            }
            failed = true;
            RunFailed?.Invoke(program.Text, error);
            return ExpressionValue.Zero;
        }
        finally
        {
            if (open)
                State.EndEvaluation(completed: false);
        }
    }

    public ExpressionValue Invoke(
        string name,
        IReadOnlyList<ExpressionValue> arguments,
        int offset)
    {
        Step(offset);
        ExpressionFunction function = Functions.Resolve(name, offset);
        // A function the reference has gets the reference's argument check
        // first, worded as it words it.
        ReferenceArguments.Check(name, arguments, IsKnownObject, offset);
        if (arguments.Count < function.MinimumArguments
            || arguments.Count > function.MaximumArguments)
        {
            string expected = function.MinimumArguments == function.MaximumArguments
                ? function.MinimumArguments.ToString(
                    System.Globalization.CultureInfo.InvariantCulture)
                : $"{function.MinimumArguments}..{function.MaximumArguments}";
            throw new ExpressionEvaluationException(
                $"{function.Signature} expects {expected} arguments; "
                + $"{arguments.Count} were passed",
                offset);
        }
        try
        {
            return function.Handler(this, arguments);
        }
        catch (ExpressionEvaluationException error)
            when (error.IsArgumentError || error.RaisedInFunction)
        {
            throw;
        }
        catch (ExpressionEvaluationException error)
        {
            throw new ExpressionEvaluationException(error.Reason, error.Offset)
            {
                RaisedInFunction = true,
            };
        }
        catch (Exception error) when (error is not OperationCanceledException)
        {
            throw new ExpressionEvaluationException(
                $"{function.Signature} failed: {error.Message}",
                offset)
            {
                RaisedInFunction = true,
            };
        }
    }
}

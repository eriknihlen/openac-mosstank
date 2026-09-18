namespace AcDream.Plugins.MossTank;

/// <summary>
/// What a rule is handed when it is asked whether it is valid.
/// <paramref name="ElapsedSeconds"/> is the time since this rule was last
/// asked, not the time since the last pass. <paramref name="CanAct"/> is
/// always true from the scheduler: a rule is asked only while it could still
/// win the pass. The field remains for a wrapper's fallback evaluation and
/// for tests that drive a rule directly.
/// </summary>
internal readonly record struct MacroPassContext(
    double ElapsedSeconds,
    bool CanAct);

internal interface IMacroRule
{
    /// <summary>VTank's <c>FriendlyName</c>; the scheduler's status/log name.</summary>
    string Name { get; }

    bool ValidNow(in MacroPassContext context);

    bool Running { get; set; }

    /// <summary>
    /// What the rule appends to its own "Running" line. The oracle writes
    /// this per rule rather than centrally: the navigation rule names the
    /// goal it is steering at, and most rules say nothing.
    /// </summary>
    string? RunningDetail => null;

    /// <summary>
    /// Why <see cref="ValidNow"/> last answered false. Null for a rule that
    /// has nothing to say. A rule that declines every pass forever is the
    /// hardest thing to diagnose in this scheduler — the log shows only that
    /// nothing ran — so a rule that knows its own reason states it once, on
    /// the RuleInfo channel, and again whenever the reason changes.
    /// </summary>
    string? DeclineReason => null;
}

internal sealed class MacroRuleSentinel : IMacroRule
{
    public MacroRuleSentinel(string marker)
    {
        Marker = marker ?? throw new ArgumentNullException(nameof(marker));
        Name = "Sentinel " + marker;
    }

    public string Marker { get; }

    public string Name { get; }

    public bool ValidNow(in MacroPassContext context) => false;

    public bool Running
    {
        get => false;
        set { }
    }
}

internal sealed class MacroRulePreChain : IMacroRule
{
    private readonly IMacroRule _primary;
    private readonly IReadOnlyList<IMacroRule> _fallbacks;
    private readonly IReadOnlyList<Func<bool>> _gates;

    public MacroRulePreChain(
        IMacroRule primary,
        IReadOnlyList<Func<bool>>? gates = null,
        IReadOnlyList<IMacroRule>? fallbacks = null)
    {
        _primary = primary ?? throw new ArgumentNullException(nameof(primary));
        _gates = gates ?? Array.Empty<Func<bool>>();
        _fallbacks = fallbacks ?? Array.Empty<IMacroRule>();
    }

    public string Name => _primary.Name;

    public string? RunningDetail => _primary.RunningDetail;

    public string? DeclineReason => _primary.DeclineReason;

    public bool ValidNow(in MacroPassContext context)
    {
        foreach (Func<bool> gate in _gates)
        {
            if (!gate())
                return false;
        }
        return _primary.ValidNow(in context);
    }

    public bool Running
    {
        get
        {
            if (_primary.Running)
                return true;
            foreach (IMacroRule fallback in _fallbacks)
            {
                if (fallback.Running)
                    return true;
            }
            return false;
        }
        set
        {
            if (!value)
            {
                foreach (IMacroRule fallback in _fallbacks)
                    fallback.Running = false;
                _primary.Running = false;
                return;
            }

            var context = new MacroPassContext(0d, CanAct: true);
            foreach (IMacroRule fallback in _fallbacks)
            {
                if (!fallback.ValidNow(in context))
                    continue;
                fallback.Running = true;
                return;
            }
            _primary.Running = true;
        }
    }
}

internal sealed class MacroScheduler
{
    public const double HeartbeatSeconds = 0.293d;

    private readonly List<IMacroRule> _main;
    private readonly List<IMacroRule> _independent;

    private readonly Dictionary<string, string> _reportedDeclines =
        new(StringComparer.Ordinal);

    private double _untilPass;
    private double _sincePass;
    private bool _poked;
    private int _suspension;

    /// <summary>
    /// The pass clock, in seconds since the scheduler started, and the value
    /// it had when each main rule was last asked. The reference reads wall
    /// time inside every rule; here a rule is handed the time since its own
    /// last evaluation, so a rule that was not asked for a while sees the
    /// whole gap at once, exactly as a wall clock would show it.
    /// </summary>
    private double _passClock;
    private readonly double[] _lastEvaluatedAt;

    public MacroScheduler(
        IReadOnlyList<IMacroRule> mainRules,
        IReadOnlyList<IMacroRule>? independentRules = null)
    {
        ArgumentNullException.ThrowIfNull(mainRules);
        _main = [.. mainRules];
        _independent = independentRules is null
            ? []
            : [.. independentRules];
        _lastEvaluatedAt = new double[_main.Count];
    }

    /// <summary>VTank's <c>dz.o.c</c> — the single "the macro is running" flag.</summary>
    public bool IsRunning { get; private set; }

    public bool IsSuspended => _suspension > 0 || ExternalSuspension;

    public bool ExternalSuspension { get; set; }

    public IMacroRule? LastExecutedRule { get; private set; }

    public IReadOnlyList<IMacroRule> MainRules => _main;

    public IReadOnlyList<IMacroRule> IndependentRules => _independent;

    public Action<double>? MetaPass { get; set; }

    public long PassCount { get; private set; }

    private double _suspendedMetaSeconds;

    public Action<MacroLogChannel, string>? Log { get; set; }

    public Func<string>? LockStateSuffix { get; set; }

    /// <summary>Hold the pass while a blocking action is in flight.</summary>
    public void Suspend() => _suspension++;

    public void Resume()
    {
        _suspension--;
        if (_suspension > 0)
            return;
        _suspension = 0;
        Poke();
    }

    public void Poke()
    {
        if (IsRunning)
            _poked = true;
    }

    public void Start()
    {
        IsRunning = true;
        _suspension = 0;
        _untilPass = 0d;
        _poked = true;
        LastExecutedRule = null;
        _suspendedMetaSeconds = 0d;
        _passClock = 0d;
        Array.Clear(_lastEvaluatedAt);
    }

    public void Stop()
    {
        foreach (IMacroRule rule in _main)
            rule.Running = false;
        foreach (IMacroRule rule in _independent)
            rule.Running = false;
        IsRunning = false;
        LastExecutedRule = null;
        _poked = false;
        _untilPass = HeartbeatSeconds;
    }

    public bool Advance(double elapsedSeconds)
    {
        double elapsed = Math.Max(0d, elapsedSeconds);
        _sincePass += elapsed;
        if (!IsRunning)
        {
            _sincePass = 0d;
            return false;
        }

        _untilPass -= elapsed;
        if (_poked)
        {
            _poked = false;
            _untilPass = 0d;
        }
        else if (_untilPass > 0d)
        {
            return false;
        }

        _untilPass = HeartbeatSeconds;
        double passElapsed = _sincePass;
        _sincePass = 0d;
        RunPass(passElapsed);
        return true;
    }

    public void RunPass(double elapsedSeconds)
    {
        double elapsed = Math.Max(0d, elapsedSeconds);

        if (IsSuspended)
        {
            _suspendedMetaSeconds += elapsed;
        }
        else
        {
            MetaPass?.Invoke(elapsed + _suspendedMetaSeconds);
            _suspendedMetaSeconds = 0d;
        }

        _passClock += elapsed;
        var independentContext = new MacroPassContext(elapsed, CanAct: true);
        foreach (IMacroRule rule in _independent)
            rule.Running = rule.ValidNow(in independentContext);

        if (IsSuspended)
            return;

        Log?.Invoke(
            MacroLogChannel.ActiveRule,
            $"----------- Primary logic loop started ({_main.Count} rules) -----------");

        // First valid rule wins, in list order, and the scan STOPS there: the
        // rules after the winner are not asked anything this pass. A rule's
        // validity is a question, not a turn, so a rule is only ever asked
        // while it could still win. Every rule that is asked is handed the
        // time since it was last asked.
        IMacroRule? winner = null;
        int winnerIndex = -1;
        int evaluated = 0;
        for (int index = 0; index < _main.Count; index++)
        {
            IMacroRule rule = _main[index];
            double sinceAsked = _passClock - _lastEvaluatedAt[index];
            _lastEvaluatedAt[index] = _passClock;
            evaluated = index + 1;
            var context = new MacroPassContext(sinceAsked, CanAct: true);
            if (!rule.ValidNow(in context))
                continue;
            winner = rule;
            winnerIndex = index;
            break;
        }

        string suffix = LockStateSuffix?.Invoke() ?? string.Empty;
        Log?.Invoke(
            MacroLogChannel.ActiveRule,
            winner is not null
                ? $"Picked {winner.Name} P: {winnerIndex}{suffix}"
                : $"All rules inactive.{suffix}");

        foreach (IMacroRule rule in _main)
        {
            if (!ReferenceEquals(rule, winner))
                rule.Running = false;
        }
        if (winner is not null)
        {
            winner.Running = true;

            string detail = winner.RunningDetail is { Length: > 0 } text
                ? " " + text
                : string.Empty;
            Log?.Invoke(
                MacroLogChannel.RuleInfo,
                $"({winner.Name}) Running{detail}");
        }

        ReportDeclines(winner, evaluated);

        LastExecutedRule = winner;
        PassCount++;
    }

    /// <summary>
    /// Say once, per rule, why a rule that could have run did not. Repeating
    /// it every 0.293 s would bury the channel, so a reason is printed when
    /// it appears and again only when it changes; a rule that wins the pass
    /// forgets its last reason, so the next decline is printed again. Only
    /// the rules that were asked this pass have a reason worth printing.
    /// </summary>
    private void ReportDeclines(IMacroRule? winner, int evaluated)
    {
        if (Log is not { } log)
            return;
        for (int index = 0; index < evaluated; index++)
        {
            IMacroRule rule = _main[index];
            if (ReferenceEquals(rule, winner))
            {
                _reportedDeclines.Remove(rule.Name);
                continue;
            }
            if (rule.DeclineReason is not { Length: > 0 } reason)
            {
                _reportedDeclines.Remove(rule.Name);
                continue;
            }
            if (_reportedDeclines.TryGetValue(rule.Name, out string? already)
                && string.Equals(already, reason, StringComparison.Ordinal))
            {
                continue;
            }
            _reportedDeclines[rule.Name] = reason;
            log(MacroLogChannel.RuleInfo, $"({rule.Name}) declined: {reason}");
        }
    }
}

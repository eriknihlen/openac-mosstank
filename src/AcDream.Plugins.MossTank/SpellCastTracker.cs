using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal enum SpellCastTrackerState
{
    /// <summary><c>gj.b.a</c> — nothing in flight.</summary>
    Idle,

    AwaitingLaunch,

    AwaitingResult,
}

internal enum SpellCastOutcome
{
    None,

    /// <summary><c>gj.cs:442-453</c> — a success line matched.</summary>
    Success,

    /// <summary><c>gj.cs:395-398</c> — a kill line matched.</summary>
    Kill,

    /// <summary><c>gj.cs:419-422</c> — a fizzle/resist line matched.</summary>
    Fail,

    /// <summary><c>gj.cs:409-413</c> — a permanent-fail line matched.</summary>
    PermanentFail,

    Rejected,

    /// <summary>
    /// <c>gj.cs:324-327</c> — the 5.000 s attempt budget expired with no
    /// acknowledgement.
    /// </summary>
    LaunchTimeout,

    ResultTimeout,
}

internal readonly record struct SpellCastOutcomeInfo(
    SpellCastOutcome Outcome,
    uint SpellId,
    string SpellName,
    uint TargetObjectId,
    string TargetName,
    bool HitsMultipleTargets,
    uint WeenieError,
    string Text);

internal sealed class SpellCastTracker
{
    public const double LaunchTimeoutSeconds = 5.0d;

    /// <summary><c>gj.cs:254</c> — <c>j.a(907)</c>, the result timer's tick.</summary>
    public const double ResultTickSeconds = 0.907d;

    /// <summary>
    /// <c>gj.cs:255</c> — <c>q = 4500 / j.e()</c>. <c>ey.e()</c> returns the
    /// timer INTERVAL (<c>ey.cs:219-222</c>), so this is an integer tick
    /// budget: <c>4500 / 907 = 4</c>.
    /// </summary>
    public const int ResultTickBudget = 4;

    public const double ResultTimeoutSeconds = ResultTickBudget * ResultTickSeconds;

    public const double WorstCaseBusySeconds =
        LaunchTimeoutSeconds + ResultTimeoutSeconds;

    internal const int LocalSpeechChatKind = 0;

    /// <summary>Item enchantment: its result names the item, not a creature.</summary>
    internal const uint ItemEnchantmentSchool = 32u;

    /// <summary>Skill ids the two attack schools are keyed by.</summary>
    internal const uint WarMagicSchool = 34u;
    internal const uint VoidMagicSchool = 43u;

    /// <summary>
    /// How long finishing one attack school holds the other off. A void cast
    /// locks war out and a war cast locks void out.
    /// </summary>
    public const double CrossSchoolLockoutSeconds = 5.5d;

    /// <summary>The gap between re-issues of a cast the server has not
    /// acknowledged, and the shorter gap used when mana is nearly out.</summary>
    public const double ReissueIntervalSeconds = 0.2d;

    public const double LowManaReissueIntervalSeconds = 0.1d;

    /// <summary>Mana below which the shorter gap is used.</summary>
    public const int LowManaThreshold = 10;

    private SpellCastTrackerState _state;
    private uint _spellId;
    private string _spellName = string.Empty;
    private uint _targetObjectId;

    private string _targetName = string.Empty;

    private bool _hitsMultipleTargets;
    private string _saying = string.Empty;
    private long _issueRevision;
    private long _observedCompletionRevision;
    private ulong _observedChatSequence;
    private double _launchElapsed;
    private double _resultElapsed;
    private SpellCastOutcomeInfo _outcome;
    private bool _hasOutcome;
    private uint _school;
    private bool _canKill;
    private double _reissueInterval = ReissueIntervalSeconds;
    private double _nextReissueAt = ReissueIntervalSeconds;
    private ActionLockTable? _actionLocks;

    /// <summary>
    /// Re-sends a cast the server has not acknowledged. Left unbound the
    /// tracker simply waits out the attempt budget.
    /// </summary>
    public Func<uint, uint, bool>? ReissueCast { get; set; }

    /// <summary>
    /// Raised for each re-issue: one more cast sent at a target that has not
    /// answered yet. This is the only signal a client gets that a monster may
    /// not really be there.
    /// </summary>
    public Action<uint>? SpellAttempted { get; set; }

    /// <summary>
    /// Raised when the target finally answers and the cast moves on to wait
    /// for its result. The unanswered-attempt count starts over here.
    /// </summary>
    public Action<uint>? SpellAnswered { get; set; }

    /// <summary>
    /// The shared cooldown table the cross-school lockout lives in. Unbound,
    /// the lockout is not armed and nothing is refused for it.
    /// </summary>
    public void BindActionLocks(ActionLockTable locks) =>
        _actionLocks = locks ?? throw new ArgumentNullException(nameof(locks));

    /// <summary>
    /// True while a spell of this school may not be issued, because a cast of
    /// the other attack school has just finished.
    /// </summary>
    public bool IsSchoolLockedOut(uint school)
    {
        if (_actionLocks is not { } locks)
            return false;
        return school switch
        {
            VoidMagicSchool => locks.IsLocked(ActionLockKind.VoidSpellLockedOut),
            WarMagicSchool => locks.IsLocked(ActionLockKind.WarSpellLockedOut),
            _ => false,
        };
    }

    /// <summary>
    /// <c>MySpell.CanKill</c>: only these spells' casts are allowed to claim a
    /// killing blow, so a fellow's or a pet's kill line arriving mid-debuff is
    /// not credited to the debuff's target.
    /// </summary>
    public static bool CanKillFor(in PluginSpellInfo spell) =>
        spell.School == WarMagicSchool
        || spell.Family is 640u or 639u
        || spell.Saying is "feazhzhapaj" or "equinzhapaj" or "tugakquati";

    /// <summary>
    /// True for the one short window after a cast is confirmed in flight
    /// during which the macro nudges itself. It closes on its own.
    /// </summary>
    public bool JiggleWindowOpen =>
        _state == SpellCastTrackerState.AwaitingResult
        && _resultElapsed < ResultTickSeconds;

    public event Action<SpellCastOutcomeInfo>? Completed;

    public SpellCastTrackerState State => _state;

    public bool IsBusy => _state != SpellCastTrackerState.Idle;

    public uint SpellId => _spellId;

    public uint TargetObjectId => _targetObjectId;

    public bool HitsMultipleTargets => _hitsMultipleTargets;

    public static bool HitsMultipleTargetsFor(in PluginSpellInfo spell) =>
        (spell.FormulaComponentIds.Count > 0
            && spell.FormulaComponentIds[0] == 110u)
        || string.Equals(spell.Saying, "tugakquati", StringComparison.Ordinal)
        || spell.Family == 638u;

    public void Begin(
        uint spellId,
        string spellName,
        uint targetObjectId,
        string targetName,
        bool hitsMultipleTargets,
        long issueRevision,
        string saying = "",
        uint school = 0u,
        bool canKill = false,
        int currentMana = int.MaxValue)
    {
        _state = SpellCastTrackerState.AwaitingLaunch;
        _saying = Normalize(saying);
        _spellId = spellId;
        _spellName = spellName ?? string.Empty;
        _targetObjectId = targetObjectId;
        // An item enchantment's success line names the ITEM, not the creature
        // it is worn by, so there is no target name to check it against.
        _targetName = school == ItemEnchantmentSchool
            ? string.Empty
            : targetName ?? string.Empty;
        _hitsMultipleTargets = hitsMultipleTargets;
        _issueRevision = issueRevision;
        _observedCompletionRevision = issueRevision;
        _launchElapsed = 0d;
        _resultElapsed = 0d;
        _hasOutcome = false;
        _outcome = default;
        _school = school;
        _canKill = canKill;
        // Nearly out of mana, the client is re-poked twice as often.
        _reissueInterval = currentMana < LowManaThreshold
            ? LowManaReissueIntervalSeconds
            : ReissueIntervalSeconds;
        _nextReissueAt = _reissueInterval;
    }

    public void Reset()
    {
        ArmCrossSchoolLockout();
        _state = SpellCastTrackerState.Idle;
        _spellId = 0u;
        _spellName = string.Empty;
        _targetObjectId = 0u;
        _targetName = string.Empty;
        _hitsMultipleTargets = false;
        _saying = string.Empty;
        _issueRevision = 0;
        _observedCompletionRevision = 0;
        _observedChatSequence = 0uL;
        _launchElapsed = 0d;
        _resultElapsed = 0d;
        _hasOutcome = false;
        _outcome = default;
        _school = 0u;
        _canKill = false;
        _reissueInterval = ReissueIntervalSeconds;
        _nextReissueAt = ReissueIntervalSeconds;
    }

    /// <summary>
    /// Leaving a finished cast holds the OTHER attack school off for a few
    /// seconds. Only a cast that got as far as waiting for its result counts:
    /// one that never left the ground arms nothing.
    /// </summary>
    private void ArmCrossSchoolLockout()
    {
        if (_state != SpellCastTrackerState.AwaitingResult
            || _actionLocks is not { } locks)
        {
            return;
        }
        if (_school == VoidMagicSchool)
        {
            locks.Arm(
                ActionLockKind.WarSpellLockedOut,
                CrossSchoolLockoutSeconds);
        }
        else if (_school == WarMagicSchool)
        {
            locks.Arm(
                ActionLockKind.VoidSpellLockedOut,
                CrossSchoolLockoutSeconds);
        }
    }

    /// <summary>
    /// <c>gj.cs:267</c> — a deleted object ends the wait only when it is the
    /// awaited target.
    /// </summary>
    public void ResetForTarget(uint objectId)
    {
        if (IsBusy && objectId != 0u && objectId == _targetObjectId)
            Reset();
    }

    public void ObserveCompletion(in PluginCastCompletion completion)
    {
        if (!IsBusy)
            return;
        if (completion.Revision == _observedCompletionRevision)
            return;
        if (completion.Revision == _issueRevision || completion.SpellId != _spellId)
            return;
        _observedCompletionRevision = completion.Revision;
        if (completion.WeenieError != 0u)
        {
            Complete(SpellCastOutcome.Rejected, completion.WeenieError, string.Empty);
            return;
        }
        if (_state != SpellCastTrackerState.AwaitingLaunch)
            return;

        // gj.cs:227-228 — the b -> c edge resets r and starts the result
        // timer. The busy latch is NOT re-raised (m_d is already true).
        _state = SpellCastTrackerState.AwaitingResult;
        _resultElapsed = 0d;
        SpellAnswered?.Invoke(_targetObjectId);
    }

    /// <param name="logTextType">
    /// Which of the client's logs the line came from. The kill sentence is a
    /// plain line and every spell result is a magic one, so this is what
    /// keeps a player typing "You killed Drudge!" in local chat from ending
    /// the wait.
    /// </param>
    public void ObserveChat(
        ulong sequence,
        string text,
        bool ownSpeech = false,
        uint logTextType = CombatLogTextType.Default)
    {
        if (!IsBusy || string.IsNullOrEmpty(text))
            return;
        if (sequence != 0uL && sequence <= _observedChatSequence)
            return;
        if (sequence != 0uL)
            _observedChatSequence = sequence;

        if (_state == SpellCastTrackerState.AwaitingLaunch
            && _saying.Length > 0
            && ownSpeech)
        {
            if (string.Equals(Normalize(text), _saying, StringComparison.Ordinal))
            {
                // gj.cs:355 — a(gj.b.c). The busy latch is NOT re-raised.
                _state = SpellCastTrackerState.AwaitingResult;
                _resultElapsed = 0d;
                SpellAnswered?.Invoke(_targetObjectId);
            }
            else
            {
                // gj.cs:359 — a(gj.b.a).
                Reset();
            }
            return;
        }

        CombatResultTextClass result = CombatResultText.Classify(
            text,
            out string spellName,
            out string targetName);

        switch (result)
        {
            case CombatResultTextClass.Kill:
                // Only a spell that can actually kill claims a killing blow.
                // A kill line arriving while a DEBUFF is in flight belongs to
                // someone else's attack and must not end the debuff's target.
                if (!_canKill || logTextType != CombatLogTextType.Default)
                    return;
                Complete(SpellCastOutcome.Kill, 0u, text);
                return;
            case CombatResultTextClass.PermanentFail
                when logTextType != CombatLogTextType.Magic:
            case CombatResultTextClass.Fail
                when logTextType != CombatLogTextType.Magic:
            case CombatResultTextClass.Success
                when logTextType != CombatLogTextType.Magic:
                // A spell's own result is logged as magic; anything else
                // wearing those words is somebody talking.
                return;
            case CombatResultTextClass.PermanentFail:
                // The failure classes match on the sentence alone: a resist
                // whose name did not parse still ends the wait instead of
                // leaving the macro busy for the full result timeout.
                if (_hitsMultipleTargets)
                    return;
                Complete(SpellCastOutcome.PermanentFail, 0u, text);
                return;
            case CombatResultTextClass.Fail:
                Complete(SpellCastOutcome.Fail, 0u, text);
                return;
            case CombatResultTextClass.Success:
                // Success is the one class that names the spell and the
                // target, so it is the one class checked against them.
                if (spellName.Length > 0
                    && _spellName.Length > 0
                    && !spellName.Equals(_spellName, StringComparison.Ordinal))
                {
                    return;
                }
                if (targetName.Length > 0
                    && _targetName.Length > 0
                    && !targetName.Equals(
                        _targetName,
                        StringComparison.OrdinalIgnoreCase))
                {
                    return;
                }
                Complete(SpellCastOutcome.Success, 0u, text);
                return;
        }
    }

    public void Advance(double elapsedSeconds)
    {
        double elapsed = Math.Max(0d, elapsedSeconds);
        switch (_state)
        {
            case SpellCastTrackerState.AwaitingLaunch:
                _launchElapsed += elapsed;
                // A cast the server never acknowledged is sent again every
                // interval until the budget runs out, rather than costing a
                // silent five seconds of standing still.
                while (_launchElapsed >= _nextReissueAt)
                {
                    if (_nextReissueAt >= LaunchTimeoutSeconds)
                    {
                        Complete(
                            SpellCastOutcome.LaunchTimeout,
                            0u,
                            string.Empty);
                        return;
                    }
                    _nextReissueAt += _reissueInterval;
                    ReissueCast?.Invoke(_spellId, _targetObjectId);
                    SpellAttempted?.Invoke(_targetObjectId);
                }
                if (_launchElapsed >= LaunchTimeoutSeconds)
                    Complete(SpellCastOutcome.LaunchTimeout, 0u, string.Empty);
                return;
            case SpellCastTrackerState.AwaitingResult:
                _resultElapsed += elapsed;
                if (_resultElapsed >= ResultTimeoutSeconds)
                    Complete(SpellCastOutcome.ResultTimeout, 0u, string.Empty);
                return;
        }
    }

    public bool TryConsumeOutcome(out SpellCastOutcomeInfo outcome)
    {
        outcome = _outcome;
        bool had = _hasOutcome;
        _hasOutcome = false;
        _outcome = default;
        return had;
    }

    /// <summary><c>gj.cs:352</c> — <c>ToLowerInvariant().Replace(" ", "")</c>.</summary>
    private static string Normalize(string text) =>
        string.IsNullOrEmpty(text)
            ? string.Empty
            : text.Replace(" ", string.Empty, StringComparison.Ordinal)
                .ToLowerInvariant();

    private void Complete(SpellCastOutcome outcome, uint weenieError, string text)
    {
        ArmCrossSchoolLockout();
        var info = new SpellCastOutcomeInfo(
            outcome,
            _spellId,
            _spellName,
            _targetObjectId,
            _targetName,
            _hitsMultipleTargets,
            weenieError,
            text);

        _state = SpellCastTrackerState.Idle;
        _launchElapsed = 0d;
        _resultElapsed = 0d;
        _outcome = info;
        _hasOutcome = true;
        Completed?.Invoke(info);
    }
}

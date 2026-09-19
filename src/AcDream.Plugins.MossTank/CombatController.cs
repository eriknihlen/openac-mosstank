using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal sealed class CombatController
{
    private const float PowerReleaseEpsilon = 0.005f;

    private readonly IPluginHost _host;
    private readonly CombatSettings _settings;
    private readonly VitalSettings _vitalSettings;
    private readonly DebuffTracker _debuffs = new();
    private readonly CombatFailureTracker _failures = new();
    private readonly MonsterHealthTracker _health;
    private readonly PetAutomation _pets = new();
    private IReadOnlyList<PluginCombatTarget> _targets =
        Array.Empty<PluginCombatTarget>();
    private IReadOnlyList<PluginSpellInfo>? _combatSpellSnapshot;
    private IReadOnlyList<PluginSpellInfo>? _attackSpellSnapshot;
    private AttackSpellCatalog _attackCatalog =
        AttackSpellCatalog.Build(Array.Empty<PluginSpellInfo>());
    private double _now;

    private double _lastElapsedSeconds;

    private uint _plannedWeapon;

    private double _untilScan;
    private double _acquisitionRange;
    private uint _targetId;
    private ResolvedMonsterRule _targetRule;
    private string _targetName = string.Empty;
    private float _targetDistance;
    private string _targetText = "Target  —";
    private string _modeText = "Mode  Unknown";
    private PluginCombatMode _lastMode = PluginCombatMode.Unknown;
    private bool _paused;
    private long _observedPhysicalCompletion;
    private long _observedAttackCastCompletion;
    private uint _pendingPhysicalTarget;
    private uint _pendingAttackSpell;

    private readonly Dictionary<(MonsterRuleActions Actions, uint Target), MonsterDamageType>
        _passElements = [];
    private readonly Dictionary<
        (DebuffIdentity Identity, uint Target),
        IReadOnlyList<CombatDebuffSource>> _passDebuffSources = [];

    /// <summary>
    /// Every flight already tested this pass, and whether it was clear. A
    /// shape not in here has not been tried, and counts as clear until it is:
    /// that is what lets the debuff fallback drop a source the pass has
    /// already found unreachable while leaving untested sources alone.
    /// </summary>
    private readonly Dictionary<(uint Target, PluginProjectilePathKind Kind), PluginProjectilePathResult>
        _passClearance = [];
    private readonly Dictionary<DebuffIdentity, PluginSpellInfo?> _passDebuffSpells = [];
    private readonly Dictionary<(MonsterDamageType Element, uint Target), bool>
        _passDeliverable = [];

    /// <summary>
    /// Whether the pack holds what each spell's formula asks for. Answered
    /// once per pass per spell, the way the tier walk memoises it per frame.
    /// </summary>
    private readonly Dictionary<uint, bool> _passComponents = [];

    /// <summary>
    /// Monsters this pass has already found nothing to do about. They are out
    /// of the running until the next pass rebuilds the picture.
    /// </summary>
    private readonly HashSet<uint> _passInvalidTargets = [];

    /// <summary>
    /// Set while one decision turns a column off but leaves the monster worth
    /// coming back to. Without it, "nothing to cast" and "this one thing
    /// cannot be delivered" would both drop the monster.
    /// </summary>
    private bool _planKeptTheMonsterInPlay;

    /// <summary>
    /// Action columns this pass has turned off per monster, because the thing
    /// that column asks for turned out to be undeliverable against it. A
    /// monster whose remaining columns still offer something stays in the
    /// running.
    /// </summary>
    private readonly Dictionary<uint, MonsterActionFlags> _passClearedActions = [];

    /// <summary>
    /// Candidates already built this pass, so re-choosing does not re-evaluate
    /// the whole rule table per monster per attempt. Discarded when the range
    /// being asked about changes, and per monster when its columns change.
    /// </summary>
    private readonly Dictionary<uint, CombatTargetCandidate?> _passCandidates = [];
    private double _passCandidateRange = double.NaN;

    private void InvalidateForPass(uint objectId)
    {
        if (objectId == 0u)
            return;
        _passInvalidTargets.Add(objectId);
        _passCandidates.Remove(objectId);
    }

    /// <summary>
    /// The rule with whatever THIS pass has learned taken off it. What the
    /// pass learns — that a shot cannot reach, that a column is undeliverable
    /// — belongs to the pass and to nothing else, so it is applied where a
    /// decision reads the rule rather than written into the stored one.
    /// </summary>
    private ResolvedMonsterRule WithPassClearedActions(
        uint objectId,
        ResolvedMonsterRule rule) =>
        rule.Rule is not null
        && _passClearedActions.TryGetValue(
            objectId,
            out MonsterActionFlags cleared)
            ? rule with
            {
                Rule = rule.Rule.WithActions(
                    rule.Actions with
                    {
                        Flags = rule.Actions.Flags & ~cleared,
                    }),
            }
            : rule;

    private void ClearActionsForPass(uint objectId, MonsterActionFlags flags)
    {
        if (objectId == 0u)
            return;
        _passClearedActions[objectId] =
            (_passClearedActions.TryGetValue(objectId, out MonsterActionFlags held)
                ? held
                : MonsterActionFlags.None)
            | flags;
        _passCandidates.Remove(objectId);
    }
    private IReadOnlyList<PluginEquipmentItem>? _passEquipment;
    private IReadOnlyList<PluginInventoryItem>? _passInventory;
    private uint _pendingAttackTarget;
    private uint _lastTargetId;
    private PendingItemDebuff? _pendingItemDebuff;
    private ulong _observedChatSequence;
    private ulong _itemTransactionChatSequence;
    private long _observedItemCompletion;
    private bool _combatPolicySuspended;
    private bool _approachMovementOwned;
    private bool _breakableTurnOwned;

    private double _approachClock;
    private double _approachFaceHeadingStamp =
        NavigationController.NoFaceHeadingStamp;

    /// <summary>The same stamp for the breakable turn-to.</summary>
    private double _breakableTurnFaceHeadingStamp =
        NavigationController.NoFaceHeadingStamp;

    /// <summary>
    /// The reference client's breakable turn-to entry tolerance: two degrees,
    /// with a one-second budget.
    /// </summary>
    private const float BreakableTurnToleranceDegrees = 2f;
    private Func<string, int, bool>? _requestAmmunitionCraft;
    private Func<string, int, bool>? _canCraftAmmunition;
    private int _randomDamageIndex;
    private long _observedJiggleCastCompletion;
    private bool _selectionJiggleActive;
    private bool _selectionJigglePreviousPlayer;
    private double _nextSelectionJiggleAt;

    /// <summary>
    /// When the current nudge window closes. The nudge is one short window
    /// per cast, not something that runs between casts.
    /// </summary>
    private double _selectionJiggleUntil;

    private static readonly MonsterDamageType[] RandomDamageCycle =
    [
        MonsterDamageType.Pierce,
        MonsterDamageType.Bludgeon,
        MonsterDamageType.Slash,
        MonsterDamageType.Acid,
        MonsterDamageType.Electric,
        MonsterDamageType.Cold,
        MonsterDamageType.Fire,
    ];

    public CombatController(
        IPluginHost host,
        CombatSettings settings,
        VitalSettings? vitalSettings = null,
        VtankGameInfoDatabase? gameInfo = null,
        SpellCastTracker? castTracker = null)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _settings = settings ?? throw new ArgumentNullException(nameof(settings));
        _vitalSettings = vitalSettings ?? new VitalSettings();
        _gameInfo = gameInfo ?? VtankGameInfoDatabase.Empty;
        _health = new MonsterHealthTracker(
            () => _settings.MonsterFacts,
            objectId => !_host.Automation.Objects.IsAvailable
                || _host.Automation.Objects.TryGet(objectId, out _));
        _castTracker = castTracker ?? new SpellCastTracker();
        _castTracker.Completed += OnCastTrackerOutcome;
        // A request the server never answered is sent again rather than
        // costing a silent five seconds mid-fight.
        _castTracker.ReissueCast = (spellId, targetObjectId) =>
            targetObjectId == 0u
                ? _host.Automation.Magic.Cast(spellId)
                : _host.Automation.Magic.Cast(spellId, targetObjectId);
        // Every re-send at a silent target is one more reason to suspect the
        // monster is not really there; the answer clears the suspicion.
        _castTracker.SpellAttempted = objectId =>
        {
            if (_failures.RecordSpellAttempt(objectId, _settings))
                DeleteGhostMonster(objectId, _settings.DeleteGhostMonsters);
        };
        _castTracker.SpellAnswered = _failures.ResetSpellAttempts;
    }

    internal SpellCastTracker CastTracker => _castTracker;

    /// <summary>
    /// Is this monster one the combat pass is following and has not given up
    /// on? A monster the pass has never looked at answers false, exactly as
    /// one it has blacklisted or seen die does: both are "not something to
    /// point at right now". This is what a profile's own monster-finding
    /// expressions ask before handing back a target.
    /// </summary>
    internal bool IsTrackedAndNotBlacklisted(uint objectId) =>
        objectId != 0u
        && _failures.IsKnown(objectId)
        && _failures.Reason(objectId, _now) == CombatSuppressionReason.None;

    /// <summary>
    /// The shared cooldown table. A kill holds navigation off for three
    /// seconds so the corpse can be found and looted before the bot moves on.
    /// </summary>
    private ActionLockTable _actionLocks = new();
    private Func<bool> _lootingEnabled = static () => false;

    /// <summary>Seconds navigation is held after a kill.</summary>
    private const double PostKillNavigationLockSeconds = 3d;

    /// <summary>
    /// How long after the server closes an attack sequence the macro still
    /// treats result text as belonging to that attack.
    /// </summary>
    private const double PhysicalResultTextTailSeconds = 2d;

    private bool _physicalResultArmed;
    private uint _physicalResultTargetId;
    private string _physicalResultTargetName = string.Empty;
    private ushort _physicalResultIncarnation;
    private double _physicalCompletedAt = double.NegativeInfinity;

    private Action _suspendPass = static () => { };
    private Action _resumePass = static () => { };
    private bool _turnHoldsPass;
    private uint _breakableTurnTargetId;

    /// <summary>
    /// Lets the controller freeze the whole rule pass while the character is
    /// turning. Unbound (a controller-only rig) the hold is a no-op.
    /// </summary>
    internal void BindPassSuspension(Action suspend, Action resume)
    {
        _suspendPass = suspend ?? throw new ArgumentNullException(nameof(suspend));
        _resumePass = resume ?? throw new ArgumentNullException(nameof(resume));
    }

    internal void BindActionLocks(ActionLockTable locks, Func<bool> lootingEnabled)
    {
        _actionLocks = locks ?? throw new ArgumentNullException(nameof(locks));
        _lootingEnabled = lootingEnabled
            ?? throw new ArgumentNullException(nameof(lootingEnabled));
        _castTracker.BindActionLocks(_actionLocks);
    }

    /// <summary>
    /// A killing blow lands: hold navigation off for the looting window. The
    /// hold is conditional on looting being on, so a bot that never loots keeps
    /// moving.
    /// </summary>
    private void ArmPostKillNavigationLock()
    {
        if (!_lootingEnabled())
            return;
        _actionLocks.Arm(
            ActionLockKind.Navigation,
            PostKillNavigationLockSeconds);
    }

    private readonly SpellCastTracker _castTracker;

    private readonly VtankGameInfoDatabase _gameInfo;

    public bool Enabled { get; private set; }
    private IDisposable? _combatControl;
    public string Status { get; private set; } = "Combat off";
    public string TargetText => _targetText;
    public string ModeText => _modeText;
    public bool HasTarget => _targetId != 0u;

    internal bool HasPendingItemDebuff => _pendingItemDebuff is not null;

    /// <summary>
    /// True while a cast from a HELD ITEM is outstanding. The reference's wand
    /// cast tracker raises the global busy count for the life of such a cast,
    /// so the pass runs no rule until it resolves.
    /// </summary>
    internal bool HeldItemCastInFlight =>
        _pendingItemDebuff is { Source.Kind: CombatDebuffSourceKind.CasterItem };

    /// <summary>
    /// True while a debuff cast from a LEARNED SPELL is still unanswered.
    /// The reference makes no distinction between a wand cast and a spell
    /// cast for its busy count: both hold the whole pass until they
    /// resolve, so a walk can never start under either.
    /// </summary>
    internal bool LearnedDebuffCastInFlight => _debuffs.HasPending;

    /// <summary>
    /// Set when the attack's own turn has already driven the held item's cast
    /// this frame, so the frame driver does not drive it a second time.
    /// </summary>
    private bool _heldItemDrivenByTurn;

    public string ButtonText => Enabled ? "Stop Macro" : "Run Macro";

    public void BindAmmunitionCraftRequest(
        Func<string, int, bool> canCraft,
        Func<string, int, bool> request)
    {
        _canCraftAmmunition = canCraft
            ?? throw new ArgumentNullException(nameof(canCraft));
        _requestAmmunitionCraft = request
            ?? throw new ArgumentNullException(nameof(request));
    }

    public Action<MacroLogChannel, string>? Log { get; set; }

    internal CombatModeGate Gate =>
        _gate ??= BindCombatModeGate(new CombatModeGate(
            _host,
            _settings,
            _vitalSettings,
            notice => Disable(notice)));

    private CombatModeGate? _gate;

    private CombatModeGate BindAmmunition(CombatModeGate gate)
    {
        gate.AmmunitionStale = (weapon, element) =>
        {
            IEquipmentAutomation equipment = _host.Automation.Equipment;
            if (!equipment.IsAvailable || weapon == 0u)
                return false;
            return ResolveAmmunitionPlan(
                PassEquipment(),
                weapon,
                element).Kind
                != AmmunitionPlanKind.Satisfied;
        };
        gate.WieldAmmunition = element =>
        {
            IEquipmentAutomation equipment = _host.Automation.Equipment;
            if (!equipment.IsAvailable)
                return false;
            IReadOnlyList<PluginEquipmentItem> items = PassEquipment();
            return TickAmmunition(items, _plannedWeapon, element);
        };
        return gate;
    }

    internal CombatModeGate BindCombatModeGate(CombatModeGate gate)
    {
        ArgumentNullException.ThrowIfNull(gate);
        _gate = BindAmmunition(gate);
        return _gate;
    }

    public void ClearActionLocks()
    {
        _host.Automation.Combat.AbortPhysicalAttack();
        StopApproachMovement();
        StopBreakableTurnMovement();
        StopSelectionJiggle();
        _pendingPhysicalTarget = 0u;
        _pendingAttackSpell = 0u;
        _pendingAttackTarget = 0u;
        _lastTargetId = 0u;
        ClearPendingItemDebuff();
        _debuffs.ClearPending();
        Gate.Reset();
        _untilScan = 0d;
        if (Enabled)
            Status = "Action locks cleared";
    }

    public bool RecordFakeImperil(uint targetObjectId)
    {
        if (targetObjectId == 0u)
            return false;
        _debuffs.RecordFakeImperil(targetObjectId, _now);
        return true;
    }

    public void Toggle()
    {
        if (Enabled)
        {
            Disable("Macro stopped");
            _host.Automation.Chat.PostSystemMessage("[MossTank] Macro stopped.");
            return;
        }

        if (!_host.Automation.IsAvailable)
        {
            Status = "Not in world";
            return;
        }

        _combatControl = _host.Automation.Combat.AcquireCombatControl();
        Enabled = true;
        _paused = false;
        _combatPolicySuspended = !_settings.Enabled;
        _untilScan = 0d;
        Status = _settings.Enabled ? "Scanning for targets" : "Combat disabled";
        _host.Automation.Chat.PostSystemMessage("[MossTank] Macro started.");
    }

    public void SetPaused(bool paused)
    {
        if (_paused == paused)
            return;
        _paused = paused;
        if (paused && Enabled)
        {
            _host.Automation.Combat.AbortPhysicalAttack();
            DisarmPhysicalResultText();
            StopApproachMovement();
            StopBreakableTurnMovement();
            Status = "Paused for buffing";
        }
        else if (Enabled)
        {
            Status = "Scanning for targets";
            _untilScan = 0d;
        }
    }

    public bool EquipOneStepForMonster(string monsterName)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(monsterName);
        var target = new PluginCombatTarget(
            0u,
            monsterName.Trim(),
            0u,
            0f,
            0f,
            false,
            1f);
        _targetName = target.Name;
        _targetRule = _settings.ResolveRule(target);
        return !TickEquipment();
    }

    public void OnTick(double elapsedSeconds)
    {
        if (!Enabled)
            return;
        if (!_host.Automation.IsAvailable)
        {
            Disable("Session ended");
            return;
        }
        if (_paused)
            return;
        if (!_settings.Enabled)
        {
            if (_combatPolicySuspended)
                return;
            _combatPolicySuspended = true;
            if (_targetId != 0u || _pendingPhysicalTarget != 0u)
                _host.Automation.Combat.AbortPhysicalAttack();
            _pendingPhysicalTarget = 0u;
            _pendingAttackSpell = 0u;
            _pendingAttackTarget = 0u;
            ClearPendingItemDebuff();
            _debuffs.Reset();
            _failures.Reset();
            ClearTarget();
            Status = "Combat disabled";
            return;
        }
        if (_combatPolicySuspended)
        {
            _combatPolicySuspended = false;
            _untilScan = 0d;
            Status = "Scanning for targets";
        }

        ClearPassMemos();

        // The clock is wall time as this controller sees it: the turn hands
        // over the time since the rule was last asked, and whatever a frame
        // driver already added while the turn was away is not added twice.
        _lastElapsedSeconds = Math.Max(0d, elapsedSeconds);
        _now += Math.Max(0d, _lastElapsedSeconds - _frameAdvancedSinceTick);
        _frameAdvancedSinceTick = 0d;
        PluginCastCompletion castCompletion =
            _host.Automation.Magic.LastCompletion;
        // The receipt moves the tracker from "did the request land" to "what
        // did it do" (gj's b -> c edge). Idempotent by revision: the panel
        // hands it the same snapshot every host frame.
        _castTracker.ObserveCompletion(castCompletion);
        ObserveSelectionJiggle(castCompletion);
        TickSelectionJiggle();
        DebuffCompletion completion = _debuffs.Observe(
            castCompletion,
            _now);
        if (completion.Completed && !completion.Succeeded)
        {
            Status = $"{completion.SpellName} failed (0x{completion.WeenieError:X})";
        }
        _debuffs.ExpirePending(_now);

        PluginCombatSnapshot current = _host.Automation.Combat.Snapshot;
        ObserveItemDebuffReceipts();
        ObserveItemTransaction();
        ObserveAttackReceipts(current, castCompletion);
        if (current.Mode != _lastMode)
        {
            _lastMode = current.Mode;
            _modeText = $"Mode  {current.Mode}";
        }

        _untilGhostSweep -= Math.Max(0d, elapsedSeconds);
        if (_untilGhostSweep <= 0d)
        {
            _untilGhostSweep = GhostSweepIntervalSeconds;
            CheckStalledHealthGhost();
        }

        _untilScan -= Math.Max(0d, elapsedSeconds);
        if (_untilScan <= 0d)
        {
            // The attack's candidate pool is what the character can HIT. A
            // monster it would have to walk to is not a candidate here at all
            // — walking to one is a separate, much lower-priority job, so an
            // unreachable monster must not starve everything below the attack.
            _acquisitionRange = _settings.MaximumRange;
            _targets = _host.Automation.Combat.CaptureHostileTargets(
                (float)_acquisitionRange);
            Log?.Invoke(
                MacroLogChannel.Timers,
                $"Combat scan: {_targets.Count} hostile(s) within {_acquisitionRange:0.0}m");
            _failures.ObserveTargets(_targets, _now, _settings);
            foreach (PluginCombatTarget scanned in _targets)
            {
                _health.Observe(scanned, _now);
                // A live monster is where the species word for its name comes
                // from when the database does not list it.
                _settings.MonsterFacts.Learn(
                    scanned.SpeciesId,
                    scanned.SpeciesName);
            }
            _debuffs.RetainTargets(
                _targets.Select(static target => target.ObjectId).ToHashSet());
            _untilScan = Math.Max(0.05d, _settings.ScanIntervalSeconds);
            RefreshTarget();
        }

        if (_pendingItemDebuff is not null)
        {
            Status = $"Waiting on {_pendingItemDebuff.Source.Kind} debuff";
            _heldItemDrivenByTurn =
                _pendingItemDebuff.Source.Kind == CombatDebuffSourceKind.CasterItem;
            TickPendingItemDebuff(current);
            return;
        }

        // One monster the character cannot act against must not cost the whole
        // pass. When a decision turns out to be undeliverable, the monster's
        // offending action column is turned off (or the monster is dropped
        // outright) for the rest of THIS pass and the choice is made again
        // from what is left, until something can be carried out or nothing is
        // left to try.
        // With the projectile awareness off there is nothing to learn from an
        // undeliverable decision, so the pass gets one attempt: this coupling
        // is the reference's, not a convenience.
        int budget = _settings.UseProjectileAwareness
            ? Math.Max(1, _settings.MaximumCollisionChecksPerTick)
            : 1;
        try
        {
            RunAttackLoop(budget);
        }
        finally
        {
            // "The monster I picked last time" is a per-PASS memory, set once
            // when the pass is over. Inside the loop the pass has no opinion
            // yet, and the tie-break must not lose the term after the first
            // attempt clears the target.
            _lastTargetId = _targetId;
        }
    }

    /// <summary>
    /// Everything a pass learns and forgets again: what the character is
    /// carrying, which flights are blocked, which action columns this pass
    /// turned off, and the candidate pool built for one range. Both passes
    /// that pick a monster start from an empty memory of all of it.
    /// </summary>
    private void ClearPassMemos()
    {
        _passElements.Clear();
        _passDebuffSources.Clear();
        _passDebuffSpells.Clear();
        _passDeliverable.Clear();
        _passComponents.Clear();
        _passClearance.Clear();
        _passAmmunitionAvailability = null;
        _passInvalidTargets.Clear();
        _passClearedActions.Clear();
        _passCandidates.Clear();
        _passCandidateRange = double.NaN;
        _passEquipment = null;
        _passInventory = null;
        _passSelectionOrder = null;
    }

    private void RunAttackLoop(int budget)
    {
        for (int attempt = 0; attempt < budget; attempt++)
        {
            // The debuff choice is remade from scratch every time the pass
            // chooses again; what carries over between attempts is what the
            // pass LEARNED — which flights are blocked and which columns are
            // off.
            _passDebuffSources.Clear();
            if (attempt > 0)
                RefreshTarget();

            if (_targetId == 0u)
            {
                StopApproachMovement();
                Status = "Waiting for a target";
                return;
            }
            if (RunAttackAttempt() != AttackPassOutcome.Retry)
            {
                Log?.Invoke(
                    MacroLogChannel.Timers,
                    $"Attack evaluation complete. Loop iterations: {attempt + 1}");
                return;
            }
            Log?.Invoke(
                MacroLogChannel.RuleInfo,
                $"Attack: {_targetName} yielded nothing this pass, choosing again");
            ClearTarget();
        }
        Log?.Invoke(
            MacroLogChannel.Timers,
            $"Attack evaluation complete. Loop iterations: {budget}");
    }

    /// <summary>
    /// What one turn of the decision did with the pass.
    /// </summary>
    private enum AttackPassOutcome
    {
        /// <summary>Something was issued, or is being waited on.</summary>
        Claimed,

        /// <summary>
        /// Nothing can be carried out against this monster; the pass should
        /// choose again from what is left.
        /// </summary>
        Retry,
    }

    /// <summary>
    /// The rule columns this one decision works from. A rolled element is
    /// rolled ONCE per decision, before the debuff chain, so the vulnerability
    /// the chain asks for and the spell the attack throws are the same element.
    /// </summary>
    private MonsterRuleActions DecisionActions =>
        _decisionActions ?? PassActions;

    /// <summary>
    /// The target's rule as THIS pass sees it: the stored rule minus whatever
    /// the pass has learned cannot be carried out.
    /// </summary>
    private MonsterRuleActions PassActions =>
        WithPassClearedActions(_targetId, _targetRule).Actions;

    private MonsterRuleActions? _decisionActions;

    private AttackPassOutcome RunAttackAttempt()
    {
        StopApproachMovement();
        _decisionActions = ResolveRandomDamage(PassActions);

        if (_pets.Tick(
                _host.Automation.Items,
                _host.Automation.Character,
                _targets,
                _settings,
                _now,
                out string petStatus,
                readyToRefillInPeace: ReadyToActInPeace,
                captured: PassInventory()))
        {
            Status = petStatus;
            return AttackPassOutcome.Claimed;
        }

        PluginCombatSnapshot combat = _host.Automation.Combat.Snapshot;
        switch (TickDebuffs(combat))
        {
            case DebuffArmOutcome.Claimed:
                return AttackPassOutcome.Claimed;
            case DebuffArmOutcome.Retry:
                return AttackPassOutcome.Retry;
        }

        if (TickEquipment())
            return AttackPassOutcome.Claimed;

        if (!DecisionActions.Attacks && !DecisionActions.UsesStreak)
        {
            Status = $"Debuffs complete for {_targetName}";
            InvalidateForPass(_targetId);
            return AttackPassOutcome.Retry;
        }

        if (!TryPrepareAttack())
            return AttackPassOutcome.Claimed;

        combat = _host.Automation.Combat.Snapshot;

        if (combat.Mode == PluginCombatMode.Magic)
            return TickMagic();

        if (combat.Mode is not (PluginCombatMode.Melee or PluginCombatMode.Missile))
        {
            Status = $"Unsupported mode: {combat.Mode}";
            return AttackPassOutcome.Claimed;
        }

        return TickPhysical(combat);
    }

    private AttackPassOutcome TickPhysical(PluginCombatSnapshot combat)
    {
        if (combat.ServerResponsePending || combat.RepeatAttackInProgress)
        {
            Status = $"Attacking {_targetName}";
            return AttackPassOutcome.Claimed;
        }

        if (combat.RequestInProgress)
        {
            if (combat.BuildInProgress
                && combat.PowerBarLevel + PowerReleaseEpsilon
                    >= combat.DesiredPower)
            {
                PluginCombatCommandResult release =
                    _host.Automation.Combat.ReleasePhysicalAttack();
                Status = release.Status == PluginCombatCommandStatus.Released
                    ? $"Attacking {_targetName}"
                    : $"Attack release: {release.Status}";
            }
            else
            {
                Status = $"Charging {combat.PowerBarLevel * 100f:0}%";
            }
            return AttackPassOutcome.Claimed;
        }

        IReadOnlyList<PluginInventoryItem> inventory =
            PassInventory();
        PluginCombatTarget physicalTarget = FindTarget(_targetId);
        MonsterRuleActions physicalActions = ResolvePhysicalActions(
            DecisionActions,
            physicalTarget,
            inventory);
        if (combat.Mode == PluginCombatMode.Missile
            && !ProjectilePathIsClear(
                _targetId,
                PluginProjectilePathKind.Missile,
                _settings.AttackHeight,
                out PluginProjectilePathResult missilePath))
        {
            Status = ProjectileStatus(missilePath, _targetName);
            // The shot cannot reach: this monster is not attackable this
            // pass, so the choice is made again from what is left.
            ClearActionsForPass(
                _targetId,
                MonsterActionFlags.Attack | MonsterActionFlags.Streak);
            return AttackPassOutcome.Retry;
        }
        // The power table reads the element the attack actually resolved to,
        // so the bar and the wield plan cannot disagree. With the automatic
        // power off nothing is written to the bar at all: the player's own
        // setting stands.
        float desiredPower = AutoAttackPower.Resolve(
                physicalActions,
                ResolveAttackElement(physicalActions, physicalTarget),
                _settings,
                _host.Automation.Character,
                inventory)
            ?? combat.DesiredPower;
        // Every arm tears the turn down before it issues: a swing and a turn
        // both want the character, and the swing wins once it is armed.
        StopBreakableTurnMovement();
        PluginCombatCommandResult begin =
            _host.Automation.Combat.BeginPhysicalAttack(
                _targetId,
                _settings.AttackHeight,
                desiredPower);
        Status = begin.Status switch
        {
            PluginCombatCommandStatus.Started => $"Charging {_targetName}",
            PluginCombatCommandStatus.Busy => $"Waiting on {_targetName}",
            PluginCombatCommandStatus.InvalidTarget => "Target disappeared",
            PluginCombatCommandStatus.WrongMode => "Waiting for combat mode",
            _ => $"Attack refused: {begin.Status}",
        };
        Log?.Invoke(
            MacroLogChannel.CastInfo,
            $"Swing: {begin.Status} at {_targetName} (0x{_targetId:X8})");
        if (begin.Status == PluginCombatCommandStatus.InvalidTarget)
        {
            _host.Automation.Combat.AbortPhysicalAttack();
            ClearTarget();
        }
        else if (begin.Status == PluginCombatCommandStatus.Started)
        {
            _pendingPhysicalTarget = _targetId;
            ArmPhysicalResultText(_targetId, _targetName);
        }
        return AttackPassOutcome.Claimed;
    }

    private AttackPassOutcome TickMagic()
    {
        IMagicCommands magic = _host.Automation.Magic;
        if (magic.IsCasting)
        {
            Status = $"Casting at {_targetName}";
            return AttackPassOutcome.Claimed;
        }

        RefreshSpellCatalogs();
        _planKeptTheMonsterInPlay = false;
        PluginCombatTarget target = FindTarget(_targetId);
        MonsterRuleActions actions = DecisionActions;
        MonsterDamageType element = ResolveAttackElement(actions, target);

        if (actions.DamageType == MonsterDamageType.Fists
            && _attackCatalog.ResolveTuskerFists() is { } fists
            && IsUsableAttackSpell(target)(fists))
        {
            if (fists.IsProjectile && !ProjectilePathIsClear(_targetId,
                PluginProjectilePathKind.Straight, PluginAttackHeight.Medium,
                out PluginProjectilePathResult fistsPath))
            {
                Status = ProjectileStatus(fistsPath, _targetName);
                InvalidateForPass(_targetId);
                return AttackPassOutcome.Retry;
            }
            // Fists is the one arm that turns whatever the turning option
            // says, and it aims a shade off the monster's bearing.
            if (!FaceForFists(_targetId))
                return AttackPassOutcome.Claimed;
            CastAttackSpell(
                new AttackSpellChoice(
                    fists,
                    VtankCombatSpellType.War,
                    MonsterDamageType.Fists,
                    CastWithoutTarget: false),
                target);
            return AttackPassOutcome.Claimed;
        }

        bool flag3 = actions.UsesPrimaryAttack;                 // !a10.t
        bool flag4 = actions.UsesRing;                          // a10.j
        bool flag5 = actions.UsesStreak;                        // a10.s
        int ringCount = CountNearbyRingTargets();               // dz.p.c
        AttackSpellChoice? plan;

        if ((flag4 && ringCount >= _settings.MinimumRingTargets)
            || (flag4 && !flag3 && !flag5 && ringCount > 0))
        {
            plan = element == MonsterDamageType.DrainAuto
                ? PlanDrain(target, ring: true)
                : PlanRing(element, target) ?? PlanBoltOrArc(element, target);
        }
        else if ((flag3 && !flag5) || (!flag3 && !flag5 && flag4))
        {
            plan = element == MonsterDamageType.DrainAuto
                ? PlanDrain(target, ring: false)
                // A rolled element names its war spell outright instead of
                // walking the tiers, and what it names is the first rung.
                : _targetRule.Actions.DamageType == MonsterDamageType.Random
                    ? PlanRolledWar(element)
                    : PlanBoltOrArc(element, target);
        }
        else if (!flag3 && flag5)
        {
            plan = element == MonsterDamageType.DrainAuto
                ? PlanDrain(target, ring: false)
                : PlanStreak(element, target)
                    ?? WarnNoStreak(element, target);
        }
        else
        {
            if (!flag3 || !flag5)
            {
                Status = $"No attack configured for {_targetName}";
                // No action was decided at all: this monster is out of the
                // running for the rest of the pass.
                InvalidateForPass(_targetId);
                return AttackPassOutcome.Retry;
            }
            if (element == MonsterDamageType.DrainAuto)
            {
                plan = PlanDrain(target, ring: false);
            }
            else
            {
                AttackSpellChoice? streak = PlanStreak(element, target);
                plan = IsFinishingBlow(target, streak)
                    ? streak ?? WarnNoStreak(element, target)
                    : PlanBoltOrArc(element, target);
            }
        }

        if (plan is not { } chosen)
        {
            Status ??= "No usable attack spell";
            // No action could be decided at all, so the monster is out of the
            // running for the rest of the pass — unless a planner turned a
            // column off and left something else it might still be owed.
            if (!_planKeptTheMonsterInPlay)
                InvalidateForPass(_targetId);
            return AttackPassOutcome.Retry;
        }

        // A streak flies the way a bolt flies, so it is tested the way a bolt
        // is tested. Without this a streak is cast into a wall over and over
        // and the pass never learns the monster is unreachable.
        if (chosen.Type == VtankCombatSpellType.Streak
            && !ProjectilePathIsClear(
                _targetId,
                PluginProjectilePathKind.Straight,
                PluginAttackHeight.Medium,
                out PluginProjectilePathResult streakPath))
        {
            Status = ProjectileStatus(streakPath, _targetName);
            MonsterActionFlags off = MonsterActionFlags.Streak;
            // Nothing straight can reach it, and the arc could not either:
            // the attack column goes off with the streak column.
            if (!KnownClear(_targetId, PluginProjectilePathKind.Arc))
                off |= MonsterActionFlags.Attack;
            ClearActionsForPass(_targetId, off);
            return AttackPassOutcome.Retry;
        }

        CastAttackSpell(chosen, target);
        return AttackPassOutcome.Claimed;
    }

    /// <summary>
    /// The war spell a rolled element throws: the family's first rung, named
    /// outright rather than walked for the best the character can cast.
    /// </summary>
    private AttackSpellChoice? PlanRolledWar(MonsterDamageType element) =>
        _attackCatalog.ResolveBaseTier(element, VtankCombatSpellType.War)
            is { } spell
            ? new AttackSpellChoice(
                spell,
                VtankCombatSpellType.War,
                element,
                CastWithoutTarget: false)
            : null;

    /// <summary>
    /// The ring arm. Unlike every other arm it is admitted on ONE question —
    /// are the components for the family's first rung in the pack — with no
    /// skill or castability test; the rung actually thrown is the best the
    /// character can cast, and when that is nothing the cast is refused where
    /// every other refusal is reported.
    /// </summary>
    private AttackSpellChoice? PlanRing(
        MonsterDamageType element,
        in PluginCombatTarget target)
    {
        PluginSpellInfo? baseTier = _attackCatalog.ResolveBaseTier(
            element,
            VtankCombatSpellType.Ring);
        bool voidRing = element is MonsterDamageType.Nether
            or MonsterDamageType.VoidBasic;
        if (baseTier is null
            && voidRing
            && _host.Automation.Spells.TryGet(
                AttackSpellCatalog.VoidRingSpellId,
                out PluginSpellInfo catalogBase))
        {
            baseTier = catalogBase;
        }
        if (baseTier is not { } family
            || !HasCastingComponents(family.SpellId))
        {
            return null;
        }
        Func<PluginSpellInfo, bool> usable = IsUsableAttackSpell(target);
        PluginSpellInfo spell = (voidRing
            ? _attackCatalog.ResolveFamilyOf(family, usable)
            : _attackCatalog.Resolve(
                element,
                VtankCombatSpellType.Ring,
                usable)) ?? family;
        return new AttackSpellChoice(
            spell,
            VtankCombatSpellType.Ring,
            element,
            CastWithoutTarget: true);
    }

    /// <summary>
    /// The quality-walked streak line for this element.
    /// </summary>
    private AttackSpellChoice? PlanStreak(
        MonsterDamageType element,
        in PluginCombatTarget target)
    {
        PluginSpellInfo? streak = _attackCatalog.Resolve(
            element,
            VtankCombatSpellType.Streak,
            IsUsableAttackSpell(target));
        return streak is { } spell
            ? new AttackSpellChoice(
                spell,
                VtankCombatSpellType.Streak,
                element,
                CastWithoutTarget: false)
            : null;
    }

    /// <summary>
    /// The reference client's own warning text, then bolt/arc.
    /// </summary>
    private AttackSpellChoice? WarnNoStreak(
        MonsterDamageType element,
        in PluginCombatTarget target)
    {
        PostAttackWarning(
            $"No streak spell usable for element '{ElementName(element)}', "
            + "using bolt/arc instead.");
        return PlanBoltOrArc(element, target);
    }

    /// <summary>
    /// Is the monster hurt enough for the streak to be the finishing move?
    /// The bar is set by the size of the LAST blow, not by the streak's own
    /// difficulty — a character hitting for 200 finishes far earlier than one
    /// hitting for 20 — and the difficulty is only the stand-in until a real
    /// blow has been seen.
    /// </summary>
    private bool IsFinishingBlow(
        in PluginCombatTarget target,
        AttackSpellChoice? streak)
    {
        if (streak is not { } choice)
            return false;
        int threshold = _health.LastDamage <= 0
            ? choice.Spell.Difficulty / 7
            : _health.LastDamage / 7;
        int remaining = _health.RemainingHealth;
        return _health.TargetObjectId == target.ObjectId
            && remaining > 0
            && remaining < threshold;
    }

    private AttackSpellChoice? PlanBoltOrArc(
        MonsterDamageType element,
        in PluginCombatTarget target)
    {
        // These two are the decision's own record of which shape it has just
        // found blocked. They deliberately do NOT start from what the pass
        // learned earlier: a shape is offered once per decision and ruled out
        // by its own test, exactly as the reference macro does it.
        bool boltBlocked = false;
        bool arcBlocked = false;
        string? projectileRefusal = null;
        Func<PluginSpellInfo, bool> usable = IsUsableAttackSpell(target);
        for (int attempt = 0; attempt < 3; attempt++)
        {
            PluginSpellInfo? bolt = boltBlocked
                ? null
                : _attackCatalog.Resolve(element, VtankCombatSpellType.War, usable);
            PluginSpellInfo? arc = arcBlocked
                ? null
                : _attackCatalog.Resolve(element, VtankCombatSpellType.Arc, usable);

            // Neither line exists for this element: warn and give up.
            if (bolt is null && arc is null)
            {
                PostAttackWarning(
                    "Warning: no usable attack spell detected for element \""
                    + ElementName(element) + "\"");
                Status = projectileRefusal ?? "No usable attack spell";
                return null;
            }

            VtankCombatSpellType type;
            PluginSpellInfo spell;
            if (bolt is null)
            {
                type = VtankCombatSpellType.Arc;              // only an arc
                spell = arc!.Value;
            }
            else if (arc is null)
            {
                type = VtankCombatSpellType.War;              // only a bolt
                spell = bolt.Value;
            }
            else if (bolt.Value.Quality > arc.Value.Quality)
            {
                type = VtankCombatSpellType.War;              // better bolt
                spell = bolt.Value;
            }
            else if (arc.Value.Quality > bolt.Value.Quality)
            {
                type = VtankCombatSpellType.Arc;              // better arc
                spell = arc.Value;
            }
            else
            {
                // The setting decides ONLY on an exact quality tie.
                switch (_settings.UseArcs)
                {
                    case UseArcsMode.AtRange:
                        if (target.Distance >= _settings.ArcRange)
                        {
                            type = VtankCombatSpellType.Arc;
                            spell = arc.Value;
                        }
                        else
                        {
                            type = VtankCombatSpellType.War;
                            spell = bolt.Value;
                        }
                        break;
                    case UseArcsMode.Yes:
                        type = VtankCombatSpellType.Arc;
                        spell = arc.Value;
                        break;
                    default: // UseArcsMode.No, and the reference's own default
                        type = VtankCombatSpellType.War;
                        spell = bolt.Value;
                        break;
                }
            }

            bool arcShape = type == VtankCombatSpellType.Arc;
            PluginProjectilePathKind shape = arcShape
                ? PluginProjectilePathKind.Arc
                : PluginProjectilePathKind.Straight;
            if (spell.IsProjectile
                && !ProjectilePathIsClear(
                    _targetId,
                    shape,
                    HeightForShape(shape),
                    out PluginProjectilePathResult path))
            {
                projectileRefusal = ProjectileStatus(path, _targetName);
                Status = projectileRefusal;
                if (arcShape)
                {
                    arcBlocked = true;
                }
                else
                {
                    boltBlocked = true;
                    // A streak flies the same way a bolt does, so a bolt that
                    // cannot reach settles the streak too.
                    ClearActionsForPass(_targetId, MonsterActionFlags.Streak);
                }
                if (arcBlocked && boltBlocked)
                {
                    // Neither shape can reach: the attack column is off for
                    // this monster for the rest of the pass. The monster is
                    // NOT out of the running — a debuff or a ring may still be
                    // owed against it.
                    ClearActionsForPass(_targetId, MonsterActionFlags.Attack);
                    _planKeptTheMonsterInPlay = true;
                    return null;
                }
                continue;
            }
            return new AttackSpellChoice(
                spell,
                type,
                element,
                CastWithoutTarget: false);
        }
        return null;
    }

    /// <summary>
    /// A void caster's drain arm. The pick is not a preference list: it is the
    /// first step of the cheapest sequence of drains, martyrs and self-heals
    /// that finishes this monster off without dropping the caster below the
    /// health the recharge settings call normal.
    /// </summary>
    private AttackSpellChoice? PlanDrain(
        in PluginCombatTarget target,
        bool ring)
    {
        if (_health.TargetObjectId != target.ObjectId)
            return null;
        ICharacterInfo character = _host.Automation.Character;
        int health = (int)Math.Min(int.MaxValue, character.CurrentHealth);
        int maximumHealth = (int)Math.Min(int.MaxValue, character.MaxHealth);
        int targetHealth = _health.RemainingHealth;
        if (health == 0 || maximumHealth == 0 || targetHealth == 0)
            return null;
        // One point below the health the recharge rule calls normal, so a plan
        // that lands exactly on the threshold still counts as safe.
        int floor = (int)Math.Ceiling(
            maximumHealth * Math.Clamp(_vitalSettings.NormalHealth, 0d, 1d)) - 1;
        // A monster nothing magical can touch cannot be drained, and neither
        // can one whose health is not a knowable number.
        bool canDrain = !_settings.MonsterFacts.IsImmuneToMagic(target.Name)
            && targetHealth != int.MaxValue;

        PluginCombatTarget planTarget = target;
        Func<PluginSpellInfo, bool> usable = IsUsableAttackSpell(planTarget);
        uint spellId = CorpseDrainPlan.SelectSpell(
            health,
            maximumHealth,
            floor,
            targetHealth,
            canDrain,
            ring,
            _gameInfo.DrainSpellOptions,
            _gameInfo.MartyrSpellOptions,
            candidate => FindKnownSpell(candidate) is { } spell
                && usable(spell));
        if (spellId == 0u || FindKnownSpell(spellId) is not { } chosen)
        {
            Log?.Invoke(
                MacroLogChannel.DebuffChoice,
                $"Drain: nothing better than a heal against {target.Name}");
            return null;
        }
        Log?.Invoke(
            MacroLogChannel.DebuffChoice,
            $"Drain: {chosen.Name} ({targetHealth} left, floor {floor})");
        return new AttackSpellChoice(
            chosen,
            ring ? VtankCombatSpellType.Ring : VtankCombatSpellType.War,
            MonsterDamageType.DrainAuto,
            CastWithoutTarget: false);
    }

    /// <summary>A spell of the character's own, by id.</summary>
    private PluginSpellInfo? FindKnownSpell(uint spellId)
    {
        if (spellId == 0u)
            return null;
        foreach (PluginSpellInfo spell in _host.Automation.Spells.KnownCombatSpells)
        {
            if (spell.SpellId == spellId)
                return spell;
        }
        foreach (PluginSpellInfo spell in _host.Automation.Spells.KnownAttackSpells)
        {
            if (spell.SpellId == spellId)
                return spell;
        }
        return null;
    }

    private Func<PluginSpellInfo, bool> IsUsableAttackSpell(
        PluginCombatTarget target) => spell =>
            !SpellComponentPolicy.UsesBlacklistedComponent(
                _host.Automation.Spells,
                spell,
                _settings.BlacklistedSpellComponents)
            && HasCastingComponents(spell.SpellId)
            && CanCastHuntSpell(spell);

    /// <summary>
    /// A tier the pack cannot pay for is not a candidate. Without this the
    /// pick lands on the best spell known, the client refuses the cast, and
    /// the next pass picks the same spell again.
    /// </summary>
    private bool HasCastingComponents(uint spellId)
    {
        if (_passComponents.TryGetValue(spellId, out bool cached))
            return cached;
        bool answer = _host.Automation.Magic.HasComponents(spellId);
        _passComponents[spellId] = answer;
        return answer;
    }

    /// <summary>VTank's own element word in its warning text (<c>f3.a</c>).</summary>
    private static string ElementName(MonsterDamageType element) => element switch
    {
        MonsterDamageType.Electric => "Lightning",
        MonsterDamageType.VoidBasic or MonsterDamageType.Nether => "Void",
        _ => element.ToString(),
    };

    private void PostAttackWarning(string text)
    {
        if (!_postedAttackWarnings.Add(text))
            return;
        _host.Automation.Chat.PostSystemMessage("[MossTank] " + text);
    }

    private readonly HashSet<string> _postedAttackWarnings =
        new(StringComparer.Ordinal);

    private void CastAttackSpell(
        AttackSpellChoice choice,
        in PluginCombatTarget target)
    {
        IMagicCommands magic = _host.Automation.Magic;
        if (!choice.CastWithoutTarget && choice.Spell.IsProjectile
            && !ProjectilePathIsClear(_targetId,
                choice.Type == VtankCombatSpellType.Arc
                    ? PluginProjectilePathKind.Arc : PluginProjectilePathKind.Straight,
                HeightForShape(choice.Type == VtankCombatSpellType.Arc
                    ? PluginProjectilePathKind.Arc : PluginProjectilePathKind.Straight),
                out PluginProjectilePathResult path))
        {
            Status = ProjectileStatus(path, _targetName);
            return;
        }
        // Finishing a cast of one attack school holds the other off for a few
        // seconds; a hybrid that fires inside that window is simply refused.
        if (_castTracker.IsSchoolLockedOut(choice.Spell.School))
        {
            Status = $"Waiting to cast at {_targetName}";
            return;
        }
        if (!choice.CastWithoutTarget
            && !ReadyForBreakableTurn(choice.Spell, _targetId))
        {
            return;
        }
        PluginCastGate gate = choice.CastWithoutTarget
            ? magic.EvaluateGate(choice.Spell.SpellId)
            : magic.EvaluateGate(choice.Spell.SpellId, _targetId);
        if (gate != PluginCastGate.Ready)
        {
            Status = gate == PluginCastGate.Busy
                ? $"Waiting to cast at {_targetName}"
                : $"Cannot cast {choice.Spell.Name}";
            return;
        }
        // A swing and a cast both want the character. Every magic arm tears
        // the swing loop down before it issues, so a leftover physical attack
        // cannot keep running underneath the cast.
        if (_physicalResultArmed || _pendingPhysicalTarget != 0u)
        {
            _host.Automation.Combat.AbortPhysicalAttack();
            DisarmPhysicalResultText();
            _pendingPhysicalTarget = 0u;
        }
        long issueRevision = magic.LastCompletion.Revision;
        bool dispatched = choice.CastWithoutTarget
            ? magic.Cast(choice.Spell.SpellId)
            : magic.Cast(choice.Spell.SpellId, _targetId);
        if (!dispatched)
        {
            Status = $"Could not start {choice.Spell.Name}";
            return;
        }

        // The reference client's own SpellCast log line.
        Log?.Invoke(
            MacroLogChannel.SpellCast,
            $"Casting: {choice.Spell.Name} on {_targetId} ({_targetName})");
        Status = choice.Type == VtankCombatSpellType.Ring
            ? $"{choice.Spell.Name} around {_targetName}"
            : $"{choice.Spell.Name} → {_targetName}";

        bool selfCast = _targetId != 0u
            && _targetId == _host.Automation.Character.ObjectId;
        _castTracker.Begin(
            choice.Spell.SpellId,
            choice.Spell.Name,
            choice.CastWithoutTarget ? 0u : _targetId,
            choice.CastWithoutTarget
                ? string.Empty
                : selfCast ? "yourself" : _targetName,
            HitsMultipleTargets(choice.Spell),
            issueRevision,
            choice.Spell.Saying,
            choice.Spell.School,
            SpellCastTracker.CanKillFor(choice.Spell),
            checked((int)Math.Min(
                int.MaxValue,
                _host.Automation.Character.CurrentMana)),
            targetIncarnation: choice.CastWithoutTarget ? (ushort)0 : FindTarget(_targetId).Incarnation);
        Log?.Invoke(MacroLogChannel.CastInfo, "SpellCaster: Begin");
        if (!choice.CastWithoutTarget)
        {
            _pendingAttackSpell = choice.Spell.SpellId;
            _pendingAttackTarget = _targetId;
        }
    }

    private MonsterRuleActions ResolveRandomDamage(MonsterRuleActions actions)
    {
        if (actions.DamageType != MonsterDamageType.Random)
            return actions;

        MonsterDamageType damage = RandomDamageCycle[_randomDamageIndex];
        _randomDamageIndex = (_randomDamageIndex + 1) % RandomDamageCycle.Length;
        return actions with { DamageType = damage };
    }

    /// <summary>
    /// The skill margin the attack-spell tier walk asks for. Range is
    /// deliberately NOT part of this: the attack pick never asks how far a
    /// spell reaches — only the debuff choice does.
    /// </summary>
    private bool CanCastHuntSpell(in PluginSpellInfo spell)
    {
        if (spell.School == 0u
            || !_host.Automation.Character.TryGetSkill(
                spell.School,
                out PluginSkillInfo skill))
        {
            return true;
        }
        return skill.Current >= spell.Difficulty
            + _settings.HuntSkillExcessOverDifficulty;
    }

    /// <summary>
    /// How many monsters a ring would actually catch. Only monsters the pass
    /// has accepted as candidates are counted — one that is blacklisted, too
    /// near, ignored, or refusing to be attacked before it is debuffed is not
    /// going to be hit and must not push the tally over the threshold — and
    /// the ring boundary itself is outside the ring.
    /// </summary>
    private int _ringCandidateCount;

    private int CountNearbyRingTargets() => _ringCandidateCount;

    private bool TickEquipment()
    {
        _plannedWeapon = 0u;

        MonsterRuleActions actions = _targetRule.Actions;
        bool primaryRequiresWeapon = actions.UsesPrimaryAttack
            || actions.UsesRing;
        if (!primaryRequiresWeapon && !_settings.SwitchWandsToDebuff)
            return false;
        IEquipmentAutomation equipment = _host.Automation.Equipment;
        if (!equipment.IsAvailable)
        {
            return false;
        }
        if (equipment.IsBusy)
        {
            Status = "Switching equipment";
            return true;
        }

        IReadOnlyList<PluginEquipmentItem> items = PassEquipment();
        uint desiredWeapon;
        if (actions.WeaponToUseRaw == 0)
        {
            // A weapon column spelled as zero means "no weapon": the rule
            // wants a wand, and nothing is auto-selected for it.
            desiredWeapon = 0u;
        }
        else
        {
            desiredWeapon = ResolveEquipmentObjectId(
                actions.WeaponObjectId,
                actions.WeaponName,
                items);
            if (desiredWeapon == 0u)
            {
                desiredWeapon = SelectAutomaticWeapon(
                    items,
                    actions,
                    FindTarget(_targetId));
            }
        }
        _plannedWeapon = desiredWeapon;

        if (TryEquipIfNeeded(equipment, items, desiredWeapon, "weapon"))
            return true;
        if (TryEquipIfNeeded(
            equipment,
            items,
            ResolveEquipmentObjectId(
                actions.OffhandObjectId,
                actions.OffhandName,
                items),
            "offhand"))
        {
            return true;
        }
        return false;
    }

    private enum AmmunitionPlanKind
    {
        /// <summary>Not a launcher, or the right stack is already wielded.</summary>
        Satisfied,
        Wield,
        Craft,
        Unavailable,
    }

    private readonly record struct AmmunitionPlan(
        AmmunitionPlanKind Kind,
        uint ObjectId,
        string Name,
        string Notice)
    {
        public static AmmunitionPlan Satisfied { get; } = new(
            AmmunitionPlanKind.Satisfied,
            0u,
            string.Empty,
            string.Empty);
    }

    private AmmunitionPlan ResolveAmmunitionPlan(
        IReadOnlyList<PluginEquipmentItem> equipmentItems,
        uint desiredWeapon,
        MonsterDamageType configuredDamage)
    {
        PluginEquipmentItem launcher = equipmentItems.FirstOrDefault(
            item => item.ObjectId == desiredWeapon);
        int launcherType = VtankAmmunitionDatabase.LauncherType(launcher.AmmoType);
        if (launcherType == 0)
            return AmmunitionPlan.Satisfied;

        MonsterDamageType damage = configuredDamage;
        VtankPrismaticAmmoPolicy prismatic = VtankPrismaticAmmoPolicy.NoPrismatic;
        if (damage == MonsterDamageType.Auto)
        {
            damage = ResolveAttackElement(
                _targetRule.Actions,
                FindTarget(_targetId));
            prismatic = VtankPrismaticAmmoPolicy.Any;
        }
        else if (damage == MonsterDamageType.Prismatic)
        {
            prismatic = VtankPrismaticAmmoPolicy.ForcePrismatic;
        }
        if (damage is MonsterDamageType.None
            or MonsterDamageType.VoidBasic
            or MonsterDamageType.DrainAuto
            or MonsterDamageType.Harm
            or MonsterDamageType.Nether)
        {
            return AmmunitionPlan.Satisfied;
        }

        IReadOnlyList<PluginInventoryItem> inventory =
            PassInventory();
        var counts = inventory
            .GroupBy(static item => item.Name, StringComparer.OrdinalIgnoreCase)
            .ToDictionary(
                static group => group.Key,
                static group => group.Sum(item => Math.Max(1, item.StackSize)),
                StringComparer.OrdinalIgnoreCase);
        var craftable = new Dictionary<string, bool>(StringComparer.OrdinalIgnoreCase);
        bool IsAvailable(string name)
        {
            if (counts.GetValueOrDefault(name) >= 1)
                return true;
            if (craftable.TryGetValue(name, out bool cached))
                return cached;
            bool value = _canCraftAmmunition?.Invoke(name, 1) == true;
            craftable[name] = value;
            return value;
        }

        // The owner's own gameinfodb.ugd wins over the bundled table when it
        // is there — one AmmunitionOptions table, read the way e0 reads it.
        VtankAmmunitionOption? selected = VtankAmmunitionDatabase.Select(
            _gameInfo.AmmunitionOptions.Count > 0
                ? _gameInfo.AmmunitionOptions
                : VtankAmmunitionDatabase.Options,
            launcherType,
            damage,
            prismatic,
            _settings.UseSpecialAmmo,
            _host.Automation.Character,
            IsAvailable);
        if (selected is not { } option)
        {
            return new AmmunitionPlan(
                AmmunitionPlanKind.Unavailable,
                0u,
                string.Empty,
                $"No {damage} ammunition is available");
        }

        // The wielded stack already IS the winning row.
        // The stack already in the quiver only satisfies the row while it
        // still holds something: an empty quiver of the right name is not
        // ammunition.
        PluginEquipmentItem currentAmmo = equipmentItems.FirstOrDefault(
            static item => item.CombatUse == 3 && item.IsEquipped);
        if (currentAmmo.StackSize > 0
            && string.Equals(currentAmmo.Name, option.Name, StringComparison.Ordinal))
        {
            return AmmunitionPlan.Satisfied;
        }

        PluginEquipmentItem desiredAmmo = equipmentItems.FirstOrDefault(
            item => item.Name.Equals(option.Name, StringComparison.Ordinal)
                && item.StackSize > 0);
        return desiredAmmo.ObjectId != 0u
            ? new AmmunitionPlan(
                AmmunitionPlanKind.Wield,
                desiredAmmo.ObjectId,
                option.Name,
                string.Empty)
            : new AmmunitionPlan(
                AmmunitionPlanKind.Craft,
                0u,
                option.Name,
                string.Empty);
    }

    private bool ExecuteAmmunitionPlan(
        IReadOnlyList<PluginEquipmentItem> equipmentItems,
        in AmmunitionPlan plan)
    {
        switch (plan.Kind)
        {
            case AmmunitionPlanKind.Satisfied:
                return false;
            case AmmunitionPlanKind.Unavailable:
                Status = plan.Notice;
                return true;
        }

        if (!Gate.TryDropToPeace(equipmentItems, plan.Name))
            return true;

        if (plan.Kind == AmmunitionPlanKind.Wield)
        {
            PluginEquipmentCommandResult equip =
                _host.Automation.Equipment.Equip(plan.ObjectId);
            Status = equip.Status == PluginEquipmentCommandStatus.Refused
                ? equip.Notice ?? $"Cannot equip {plan.Name}."
                : $"Equipping {plan.Name}";
            return true;
        }

        if (_requestAmmunitionCraft?.Invoke(plan.Name, 1) == true)
        {
            Status = "Crafting " + plan.Name;
            return true;
        }
        Status = $"Waiting to craft {plan.Name}";
        return true;
    }

    private bool TickAmmunition(
        IReadOnlyList<PluginEquipmentItem> equipmentItems,
        uint desiredWeapon,
        MonsterDamageType configuredDamage) => ExecuteAmmunitionPlan(
            equipmentItems,
            ResolveAmmunitionPlan(
                equipmentItems,
                desiredWeapon,
                configuredDamage));

    private MonsterRuleActions ResolvePhysicalActions(
        MonsterRuleActions actions,
        in PluginCombatTarget target,
        IReadOnlyList<PluginInventoryItem> inventory)
    {
        if (actions.DamageType != MonsterDamageType.Auto)
            return actions;

        IReadOnlyList<MonsterDamageType> preferences =
            _gameInfo.DamagePreferences(target.Name);
        foreach (MonsterDamageType damage in preferences)
        {
            int mask = RawDamageType(damage);
            if (mask == 0)
                continue;
            foreach (PluginInventoryItem item in inventory)
            {
                if (item.IsEquipped && (item.DamageType & mask) != 0)
                    return actions with { DamageType = damage };
            }
        }
        return preferences.Count == 0
            ? actions
            : actions with { DamageType = preferences[0] };
    }

    internal bool ReadyToActInPeace() => Gate.TryDropToPeace(
        _host.Automation.Equipment.IsAvailable
            ? _host.Automation.Equipment.CaptureOwnedEquipment()
            : [],
        "the combat pet");

    private bool TryPrepareAttack()
    {
        // The plan is Magic until an owned item backs it. Without an
        // equipment projection nothing can back it, but the combat mode is
        // still a hard requirement: answering "ready" here would let the pass
        // try to swing or cast out of peace mode and quietly do nothing.
        IEquipmentAutomation equipment = _host.Automation.Equipment;
        PluginCombatMode wanted = PluginCombatMode.Magic;
        uint plannedWeapon = 0u;
        if (_plannedWeapon != 0u && equipment.IsAvailable)
        {
            foreach (PluginEquipmentItem item in PassEquipment())
            {
                if (item.ObjectId != _plannedWeapon)
                    continue;
                wanted = CombatModeGate.ModeFor(in item);
                plannedWeapon = _plannedWeapon;
                break;
            }
        }

        if (Gate.TryPrepare(
                wanted,
                overrideItemId: plannedWeapon,
                autoSelect: plannedWeapon == 0u,
                element: _targetRule.Rule is null
                    ? MonsterDamageType.None
                    : _targetRule.Actions.DamageType,
                captured: PassEquipment()))
        {
            return true;
        }
        Status = Gate.Status;
        return false;
    }

    private bool TryEquipIfNeeded(
        IEquipmentAutomation equipment,
        IReadOnlyList<PluginEquipmentItem> items,
        uint objectId,
        string role)
    {
        if (objectId == 0u)
            return false;

        PluginEquipmentItem? desired = null;
        foreach (PluginEquipmentItem item in items)
        {
            if (item.ObjectId == objectId)
            {
                desired = item;
                break;
            }
        }
        if (desired is not { } selected || selected.IsEquipped)
            return false;

        if (!Gate.TryDropToPeace(items, selected.Name))
        {
            Status = Gate.Status;
            return true;
        }

        PluginEquipmentCommandResult result = equipment.Equip(objectId);
        if (result.Status is PluginEquipmentCommandStatus.Started
            or PluginEquipmentCommandStatus.Busy)
        {
            Status = $"Equipping {selected.Name}";
            return true;
        }
        if (result.Status == PluginEquipmentCommandStatus.Refused)
            Status = $"Cannot equip {role}: {selected.Name}";
        return false;
    }

    /// <summary>
    /// The weapon automatic selection reaches for against this monster.
    /// </summary>
    private uint SelectAutomaticWeapon(
        IReadOnlyList<PluginEquipmentItem> items,
        MonsterRuleActions actions,
        in PluginCombatTarget target)
    {
        // An automatic rule wants whatever the monster is weak to; a rule that
        // spells its element out wants only that one.
        IReadOnlyList<MonsterDamageType> wanted =
            actions.DamageType is MonsterDamageType.Auto
                or MonsterDamageType.Prismatic
                ? _gameInfo.DamagePreferences(target.Name)
                : [actions.DamageType];
        PluginCombatTarget subject = target;
        ICharacterInfo character = _host.Automation.Character;
        return VtankWeaponLadder.Select(
            InProfileOrder(items),
            item => (_settings.CombatItemObjectIds.Contains(item.ObjectId)
                || _settings.CombatItemNames.Contains(item.Name))
                && ConfiguredSupplyReadiness.IsAssessed(_host.Automation, item.ObjectId),
            wanted,
            SpeciesOf(in subject),
            (item, element) => CanWeaponDeliver(in item, element),
            element => IsAlreadyVulnerable(in subject, element),
            warTrained: IsTrained(character, WarMagicSkill),
            voidTrained: IsTrained(character, VoidMagicSkill),
            excludeObjectId: actions.OffhandObjectId,
            onLastResort: () => PostAttackWarning(
                "Warning: no weapons found that can be autoselected for current "
                + "target. Add some weapons to the items list!"));
    }

    /// <summary>
    /// The owned items in the order the Items page fixes: what the page names
    /// first, in page order, then whatever it does not name, and the object id
    /// inside a group the page cannot tell apart.
    ///
    /// The walk that picks a weapon keeps the LAST candidate it saw at every
    /// rung, so its answer is only as steady as the order it walks. What the
    /// host hands out is sorted with the held items first, and that order
    /// turns over the instant something is wielded: two equally rated items of
    /// the same name pass the pick back and forth, and the character swaps
    /// weapons for ever instead of fighting. The page's order is the one thing
    /// in this list that a fight cannot move.
    /// </summary>
    private IReadOnlyList<PluginEquipmentItem> InProfileOrder(
        IReadOnlyList<PluginEquipmentItem> items)
    {
        if (_passSelectionOrder is { } memo && ReferenceEquals(memo.Source, items))
            return memo.Ordered;
        if (items.Count < 2)
            return items;

        IList<string> page = _settings.CombatItemOrder;
        int PageIndex(string name)
        {
            for (int index = 0; index < page.Count; index++)
            {
                if (string.Equals(page[index], name, StringComparison.Ordinal))
                    return index;
            }
            return int.MaxValue;
        }

        var ordered = new List<PluginEquipmentItem>(items);
        ordered.Sort((left, right) =>
        {
            int placed = PageIndex(left.Name).CompareTo(PageIndex(right.Name));
            if (placed != 0)
                return placed;
            int name = string.CompareOrdinal(left.Name, right.Name);
            return name != 0
                ? name
                : left.ObjectId.CompareTo(right.ObjectId);
        });
        _passSelectionOrder = (items, ordered);
        return ordered;
    }

    private (IReadOnlyList<PluginEquipmentItem> Source,
        IReadOnlyList<PluginEquipmentItem> Ordered)? _passSelectionOrder;

    /// <summary>
    /// The monster's species, or -1 when the game-info database does not name
    /// it — which no weapon's slayer type can match.
    /// </summary>
    private int SpeciesOf(in PluginCombatTarget target)
    {
        if (target.SpeciesId > 0)
            return target.SpeciesId;
        return _gameInfo.SpeciesMembers.TryGetValue(
            target.Name ?? string.Empty,
            out VtankSpeciesMember member)
            ? member.Species
            : -1;
    }

    private bool IsAlreadyVulnerable(
        in PluginCombatTarget target,
        MonsterDamageType element)
    {
        var identity = new DebuffIdentity(
            MonsterActionFlags.Vulnerability,
            element);
        if (FindDebuffSpell(identity) is not { } vulnerability)
            return false;
        // "Still up in half a second's time", which is the same question as
        // "not due within half a second".
        return !_debuffs.IsDue(
            target.ObjectId,
            identity,
            vulnerability,
            _now,
            0.5d);
    }

    private bool CanWeaponDeliver(
        in PluginEquipmentItem item,
        MonsterDamageType element)
    {
        int launcherType = VtankAmmunitionDatabase.LauncherType(item.AmmoType);
        if (launcherType == 0)
            return true;
        (MonsterDamageType, uint) key = (element, item.ObjectId);
        if (_passDeliverable.TryGetValue(key, out bool cached))
            return cached;
        bool deliverable = VtankAmmunitionDatabase.Select(
            _gameInfo.AmmunitionOptions.Count > 0
                ? _gameInfo.AmmunitionOptions
                : VtankAmmunitionDatabase.Options,
            launcherType,
            element,
            VtankPrismaticAmmoPolicy.Any,
            _settings.UseSpecialAmmo,
            _host.Automation.Character,
            AmmunitionAvailability()) is not null;
        _passDeliverable[key] = deliverable;
        return deliverable;
    }

    private static int RawDamageType(MonsterDamageType damageType) =>
        damageType switch
        {
            MonsterDamageType.Slash => 0x0001,
            MonsterDamageType.Pierce => 0x0002,
            MonsterDamageType.Bludgeon => 0x0004,
            MonsterDamageType.Cold => 0x0008,
            MonsterDamageType.Fire => 0x0010,
            MonsterDamageType.Acid => 0x0020,
            MonsterDamageType.Electric => 0x0040,
            MonsterDamageType.Nether => 0x0400,
            _ => 0,
        };

    /// <summary>
    /// The debuff arm of one decision.
    /// </summary>
    /// <returns>
    /// What the arm did with the pass: nothing (fall through to the attack),
    /// claimed it, or turned a debuff column off — in which case the whole
    /// choice is made again, because a monster whose chain just lost a step
    /// may no longer be the one worth acting on.
    /// </returns>
    private DebuffArmOutcome TickDebuffs(PluginCombatSnapshot combat)
    {
        if (_debuffs.HasPending)
        {
            Status = _host.Automation.Magic.IsCasting
                ? $"Casting {_debuffs.PendingName}"
                : $"Waiting for {_debuffs.PendingName}";
            return DebuffArmOutcome.Claimed;
        }

        RefreshSpellCatalogs();
        IReadOnlyList<PluginInventoryItem> items =
            PassInventory();
        PluginCombatTarget target = FindTarget(_targetId);
        if (target.ObjectId == 0u)
            return DebuffArmOutcome.Idle;

        MonsterRuleActions actions = DecisionActions;
        IReadOnlyList<CombatDebuffStep> steps = CombatDebuffChain.Build(
            actions,
            ResolveAttackElement(actions, target),
            ResolveExtraVulnerability(actions, target));

        if (CombatDebuffChain.Choose(
                steps,
                step => IsDebuffStepDue(step, in target, items))
            is not { } due)
        {
            return DebuffArmOutcome.Idle;
        }
        DebuffPassResult result = TickDebuffStep(
            due,
            actions,
            target,
            combat,
            items);
        if (result != DebuffPassResult.ColumnDisabled)
        {
            return result == DebuffPassResult.Claimed
                ? DebuffArmOutcome.Claimed
                : DebuffArmOutcome.Idle;
        }

        // The step cannot be delivered: its column goes off for the rest of
        // this pass and the whole choice is made again, rather than walking
        // on to the next step against a monster that may no longer be the one
        // worth acting on.
        ClearActionsForPass(_targetId, due.Identity.Flag);
        return DebuffArmOutcome.Retry;
    }

    /// <summary>What the debuff arm did with the pass.</summary>
    private enum DebuffArmOutcome
    {
        Idle,
        Claimed,
        Retry,
    }

    /// <summary>
    /// What one turn of <c>hi</c>'s debuff arm did with the pass.
    /// </summary>
    private enum DebuffPassResult
    {
        Idle,

        /// <summary>Something was issued (or is being waited on).</summary>
        Claimed,

        ColumnDisabled,
    }

    private DebuffPassResult TickDebuffStep(
        CombatDebuffStep due,
        MonsterRuleActions actions,
        PluginCombatTarget target,
        PluginCombatSnapshot combat,
        IReadOnlyList<PluginInventoryItem> items)
    {
        CombatDebuffSource choice;
        while (true)
        {
            if (ChooseDebuffSource(
                    due.Identity,
                    in target,
                    items,
                    message => Log?.Invoke(MacroLogChannel.DebuffChoice, message))
                is not { } winner)
            {
                return DebuffPassResult.Idle;
            }
            if (winner.PathKind is not { } shape
                || ProjectilePathIsClear(
                    target.ObjectId,
                    shape,
                    HeightForShape(shape),
                    out PluginProjectilePathResult debuffPath))
            {
                choice = winner;
                break;
            }
            Status = ProjectileStatus(debuffPath, target.Name);
            // Without the fallback the winner stands and its column goes off
            // for the pass. With it, the shape is now a KNOWN-blocked one, so
            // the choice made again lands on the next-best source.
            if (!_settings.AllowDebuffFallback)
                return DebuffPassResult.ColumnDisabled;
        }

        if (SpellComponentPolicy.UsesBlacklistedComponent(
                _host.Automation.Spells,
                choice.Spell,
                _settings.BlacklistedSpellComponents))
        {
            return DebuffPassResult.Idle;
        }
        if (!ReadyForBreakableTurn(choice.Spell, target.ObjectId))
            return DebuffPassResult.Claimed;
        if (choice.Kind != CombatDebuffSourceKind.LearnedSpell)
        {
            DebuffStartResult itemResult = TryStartItemDebuff(
                choice,
                target,
                combat,
                items,
                ResolveInventoryObjectId(
                    actions.OffhandObjectId,
                    actions.OffhandName,
                    items));
            return itemResult == DebuffStartResult.Handled
                ? DebuffPassResult.Claimed
                : DebuffPassResult.Idle;
        }

        // A learned debuff is cast from a wand, not from whatever the last
        // swing left in hand: the full wield gate runs first, so the wand's
        // own spellcraft and mana are what pay for the debuff.
        if (!PrepareForLearnedDebuff(actions, in target, items))
            return DebuffPassResult.Claimed;

        PluginCastGate gate = _host.Automation.Magic.EvaluateGate(
            choice.Spell.SpellId,
            target.ObjectId);
        if (gate == PluginCastGate.Busy)
        {
            Status = "Waiting to debuff";
            return DebuffPassResult.Claimed;
        }
        if (gate != PluginCastGate.Ready
            || !_host.Automation.Magic.Cast(
                choice.Spell.SpellId,
                target.ObjectId))
        {
            return DebuffPassResult.Idle;
        }

        _debuffs.Begin(
            target.ObjectId,
            choice.Identity,
            choice.Spell,
            _now,
            _host.Automation.Magic.LastCompletion.Revision);
        string targetName = string.IsNullOrWhiteSpace(target.Name)
            ? $"0x{target.ObjectId:X8}"
            : target.Name;
        Log?.Invoke(
            MacroLogChannel.SpellCast,
            $"Casting: {choice.Spell.Name} on {target.ObjectId} ({targetName})");
        Status = $"{choice.Spell.Name} → {targetName}";
        return DebuffPassResult.Claimed;
    }

    /// <summary>
    /// The wield gate a learned-spell debuff runs through. With
    /// <c>SwitchWandsToDebuff</c> on and the attack weapon already a caster,
    /// that weapon is kept; every other case falls through to the first
    /// profiled wand.
    /// </summary>
    /// <returns>True once the character is holding what it needs.</returns>
    private bool PrepareForLearnedDebuff(
        MonsterRuleActions actions,
        in PluginCombatTarget target,
        IReadOnlyList<PluginInventoryItem> items)
    {
        IReadOnlyList<PluginEquipmentItem> equipment = PassEquipment();
        uint attackWeapon = 0u;
        if (_settings.SwitchWandsToDebuff)
        {
            (uint weapon, _, _) = ResolveWieldPlan(
                actions,
                in target,
                items,
                equipment);
            foreach (PluginEquipmentItem item in equipment)
            {
                if (weapon == 0u || item.ObjectId != weapon)
                    continue;
                if (CombatModeGate.ModeFor(in item) == PluginCombatMode.Magic)
                    attackWeapon = weapon;
                break;
            }
        }

        if (Gate.TryPrepare(
                PluginCombatMode.Magic,
                overrideItemId: attackWeapon,
                autoSelect: attackWeapon == 0u,
                captured: equipment))
        {
            return true;
        }
        Status = Gate.Status;
        return false;
    }

    /// <summary>
    /// The height the way to the monster is tested at. It belongs to the
    /// SHAPE of the flight, not to the swing: an arc is thrown high, a bolt
    /// goes out level, and anything else — a shot, a thrown weapon — is tested
    /// at the height the profile swings at.
    /// </summary>
    private PluginAttackHeight HeightForShape(PluginProjectilePathKind kind) =>
        kind switch
        {
            PluginProjectilePathKind.Arc => PluginAttackHeight.High,
            PluginProjectilePathKind.Straight => PluginAttackHeight.Medium,
            _ => _settings.AttackHeight,
        };

    private bool ProjectilePathIsClear(
        uint targetObjectId,
        PluginProjectilePathKind kind,
        PluginAttackHeight height,
        out PluginProjectilePathResult result)
    {
        if (!_settings.UseProjectileAwareness)
        {
            result = new(PluginProjectilePathStatus.Clear);
            return true;
        }
        if (_passClearance.TryGetValue((targetObjectId, kind), out PluginProjectilePathResult memo))
        {
            result = memo;
            return memo.IsClear;
        }
        result = _settings.ShowCollisionDebug
            ? _host.Automation.Projectiles.EvaluatePathWithDiagnostics(
                targetObjectId,
                kind,
                height,
                (float)_settings.CollisionProjectileRadius,
                (float)_settings.CollisionStepDistance,
                _settings.CollisionSampleBudget)
            : _host.Automation.Projectiles.EvaluatePath(
                targetObjectId,
                kind,
                height,
                (float)_settings.CollisionProjectileRadius,
                (float)_settings.CollisionStepDistance,
                _settings.CollisionSampleBudget);
        if (_settings.ShowCollisionDebug && result.DebugSamples.Count > 0)
        {
            _host.Automation.Projectiles.ShowDebugSamples(result.DebugSamples);
            _host.Log.Info(
                $"MossTank collision {kind}: {result.Status}, "
                + $"{result.DebugSamples.Count} marker(s), "
                + $"{result.CollisionChecks} check(s)");
        }
        _passClearance[(targetObjectId, kind)] = result;
        return result.IsClear;
    }

    /// <summary>
    /// What the pass already knows about a flight, without testing it. An
    /// untested shape reads as clear.
    /// </summary>
    private bool KnownClear(uint targetObjectId, PluginProjectilePathKind? kind) =>
        kind is not { } shape
        || !_passClearance.TryGetValue((targetObjectId, shape), out PluginProjectilePathResult result)
        || result.IsClear;

    private static string ProjectileStatus(
        in PluginProjectilePathResult result,
        string targetName)
    {
        string target = string.IsNullOrWhiteSpace(targetName)
            ? "target"
            : targetName;
        return result.Status switch
        {
            PluginProjectilePathStatus.Blocked when result.BlockingObjectId != 0u =>
                $"Projectile path to {target} blocked by 0x{result.BlockingObjectId:X8}",
            PluginProjectilePathStatus.Blocked =>
                $"Projectile path to {target} is blocked",
            PluginProjectilePathStatus.Unavailable =>
                "Projectile collision data is unavailable",
            PluginProjectilePathStatus.BudgetExceeded =>
                "Projectile collision-check budget exhausted",
            PluginProjectilePathStatus.InvalidTarget =>
                $"Cannot resolve projectile path to {target}",
            PluginProjectilePathStatus.Error =>
                result.Notice ?? "Projectile collision check failed",
            _ => $"Cannot fire at {target}",
        };
    }

    /// <summary>The two attack schools, by skill id.</summary>
    private const uint WarMagicSkill = 34u;
    private const uint VoidMagicSkill = 43u;

    private static bool IsTrained(ICharacterInfo character, uint skillId) =>
        character.TryGetSkill(skillId, out PluginSkillInfo skill)
        && skill.Training is PluginSkillTraining.Trained
            or PluginSkillTraining.Specialized;

    private DebuffStartResult TryStartItemDebuff(
        CombatDebuffSource source,
        PluginCombatTarget target,
        PluginCombatSnapshot combat,
        IReadOnlyList<PluginInventoryItem> inventory,
        uint desiredOffhand)
    {
        IEquipmentAutomation equipment = _host.Automation.Equipment;
        if (!equipment.IsAvailable)
            return DebuffStartResult.Skipped;
        if (equipment.IsBusy)
        {
            Status = $"Equipping {ItemName(source.ItemObjectId, inventory)}";
            return DebuffStartResult.Handled;
        }

        PluginInventoryItem item = default;
        bool found = false;
        foreach (PluginInventoryItem candidate in inventory)
        {
            if (candidate.ObjectId == source.ItemObjectId)
            {
                item = candidate;
                found = true;
                break;
            }
        }
        if (!found)
            return DebuffStartResult.Skipped;

        if (source.Kind == CombatDebuffSourceKind.Grenade
            && desiredOffhand != 0u)
        {
            IReadOnlyList<PluginEquipmentItem> equipmentItems = PassEquipment();
            PluginEquipmentItem? offhand = null;
            foreach (PluginEquipmentItem candidate in equipmentItems)
            {
                if (candidate.ObjectId == desiredOffhand)
                {
                    offhand = candidate;
                    break;
                }
            }
            if (offhand is { IsEquipped: false } selectedOffhand)
            {
                PluginEquipmentCommandResult offhandResult =
                    equipment.Equip(selectedOffhand.ObjectId);
                if (offhandResult.Accepted
                    || offhandResult.Status == PluginEquipmentCommandStatus.Busy)
                {
                    Status = $"Equipping {selectedOffhand.Name}";
                    return DebuffStartResult.Handled;
                }
                return DebuffStartResult.Skipped;
            }
        }

        if (!item.IsEquipped)
        {
            PluginEquipmentCommandResult equip = equipment.Equip(item.ObjectId);
            if (equip.Status is PluginEquipmentCommandStatus.Started
                or PluginEquipmentCommandStatus.Busy)
            {
                Status = $"Equipping {item.Name}";
                return DebuffStartResult.Handled;
            }
            return DebuffStartResult.Skipped;
        }

        PluginCombatMode desiredMode = source.Kind switch
        {
            CombatDebuffSourceKind.CasterItem => PluginCombatMode.Magic,
            CombatDebuffSourceKind.Grenade => PluginCombatMode.Missile,
            _ when (item.ItemType & 0x00000100u) != 0u =>
                PluginCombatMode.Missile,
            _ => PluginCombatMode.Melee,
        };
        if (combat.Mode != desiredMode)
        {
            EnterDebuffMode(desiredMode);
            return DebuffStartResult.Handled;
        }

        string targetName = string.IsNullOrWhiteSpace(target.Name)
            ? $"0x{target.ObjectId:X8}"
            : target.Name;
        if (source.Kind == CombatDebuffSourceKind.CasterItem)
        {
            IItemAutomation itemCommands = _host.Automation.Items;
            if (!itemCommands.IsAvailable || itemCommands.IsBusy)
            {
                Status = $"Waiting to use {item.Name}";
                return DebuffStartResult.Handled;
            }
            PluginItemCommandResult apply = itemCommands.Apply(
                item.ObjectId,
                target.ObjectId);
            if (!apply.Accepted)
                return DebuffStartResult.Skipped;
            // The wand now owns the character until its cast is over: nothing
            // else may use an item, and the attack may not swing, inside that
            // window.
            _actionLocks.Arm(
                ActionLockKind.ItemUse,
                ItemUseLock.HeldItemCastSeconds);
            _pendingItemDebuff = new PendingItemDebuff(
                source,
                target.ObjectId,
                targetName,
                item.Name,
                _now,
                itemCommands.LastCompletion.Revision,
                combat.CompletionRevision,
                desiredMode,
                0f);
            Status = $"{source.Spell.Name} via {item.Name} → {targetName}";
            return DebuffStartResult.Handled;
        }

        if (combat.RequestInProgress
            || combat.ServerResponsePending
            || combat.RepeatAttackInProgress)
        {
            Status = $"Waiting to fire {item.Name}";
            return DebuffStartResult.Handled;
        }
        float power = desiredMode == PluginCombatMode.Missile ? 1f : 0f;
        PluginCombatCommandResult begin =
            _host.Automation.Combat.BeginPhysicalAttack(
                target.ObjectId,
                PluginAttackHeight.Medium,
                power);
        if (begin.Status != PluginCombatCommandStatus.Started)
            return begin.Status == PluginCombatCommandStatus.Busy
                ? DebuffStartResult.Handled
                : DebuffStartResult.Skipped;
        _pendingItemDebuff = new PendingItemDebuff(
            source,
            target.ObjectId,
            targetName,
            item.Name,
            _now,
            _host.Automation.Items.LastCompletion.Revision,
            combat.CompletionRevision,
            desiredMode,
            power);
        Status = $"Charging {item.Name} for {targetName}";
        return DebuffStartResult.Handled;
    }

    private static uint ResolveEquipmentObjectId(
        uint sessionObjectId,
        string durableName,
        IReadOnlyList<PluginEquipmentItem> items)
    {
        if (sessionObjectId != 0u
            && items.Any(item => item.ObjectId == sessionObjectId))
        {
            return sessionObjectId;
        }
        if (string.IsNullOrWhiteSpace(durableName))
            return 0u;
        foreach (PluginEquipmentItem item in items)
        {
            if (item.Name.Equals(durableName, StringComparison.Ordinal))
                return item.ObjectId;
        }
        return 0u;
    }

    private static uint ResolveInventoryObjectId(
        uint sessionObjectId,
        string durableName,
        IReadOnlyList<PluginInventoryItem> items)
    {
        if (sessionObjectId != 0u
            && items.Any(item => item.ObjectId == sessionObjectId))
        {
            return sessionObjectId;
        }
        if (string.IsNullOrWhiteSpace(durableName))
            return 0u;
        foreach (PluginInventoryItem item in items)
        {
            if (item.Name.Equals(durableName, StringComparison.Ordinal))
                return item.ObjectId;
        }
        return 0u;
    }

    private void EnterDebuffMode(PluginCombatMode mode)
    {
        PluginCombatCommandResult result =
            _host.Automation.Combat.EnterMode(mode);
        Status = result.Status == PluginCombatCommandStatus.Refused
            ? result.Notice ?? $"Cannot enter {mode} mode"
            : $"Entering {mode} mode";
    }

    private void ClearPendingItemDebuff()
    {
        // The item is finished with, so the slot goes down early rather than
        // costing the rest of its window.
        if (_pendingItemDebuff?.Source.Kind == CombatDebuffSourceKind.CasterItem)
            _actionLocks.Release(ActionLockKind.ItemUse);
        _pendingItemDebuff = null;
    }

    private void TickPendingItemDebuff(PluginCombatSnapshot combat)
    {
        if (_pendingItemDebuff is not { } pending)
            return;
        if (_now - pending.DispatchedAt >= 15d)
        {
            _host.Automation.Combat.AbortPhysicalAttack();
            Status = $"{pending.Source.Spell.Name} timed out";
            ClearPendingItemDebuff();
            return;
        }
        if (pending.Source.Kind == CombatDebuffSourceKind.CasterItem)
        {
            TickWandCastRecovery(pending);
            Status = $"Waiting for {pending.Source.Spell.Name}";
            return;
        }

        if (combat.RequestInProgress)
        {
            if (combat.BuildInProgress
                && combat.PowerBarLevel + PowerReleaseEpsilon
                    >= pending.Power)
            {
                PluginCombatCommandResult release =
                    _host.Automation.Combat.ReleasePhysicalAttack();
                Status = release.Status == PluginCombatCommandStatus.Released
                    ? $"Firing {pending.ItemName}"
                    : $"Attack release: {release.Status}";
            }
            else
            {
                Status = $"Charging {pending.ItemName}";
            }
            return;
        }
        if (combat.ServerResponsePending || combat.RepeatAttackInProgress)
        {
            Status = $"Waiting for {pending.Source.Spell.Name}";
            return;
        }
        if (combat.CompletionRevision > pending.PhysicalCompletionRevision)
        {
            pending.PhysicalCompletionRevision = combat.CompletionRevision;
            pending.AttackCompletedAt ??= _now;
            if (combat.CompletionWeenieError != 0u)
            {
                Status = $"{pending.ItemName} failed (0x{combat.CompletionWeenieError:X})";
                ClearPendingItemDebuff();
                return;
            }
        }
        if (pending.AttackCompletedAt is { } completed
            && _now - completed >= 1d)
        {
            ClearPendingItemDebuff();
            Status = $"Retrying {pending.Source.Spell.Name}";
            return;
        }
        Status = $"Waiting for {pending.Source.Spell.Name}";
    }

    private void TickWandCastRecovery(PendingItemDebuff pending)
    {
        double age = _now - pending.DispatchedAt;
        INavigationAutomation movement = _host.Automation.Navigation;
        if (_settings.JumpOutWandCasting
            && !pending.RecoverySent
            && age >= 0.2d)
        {
            _ = movement.SetMovementIntent(new PluginMovementIntent(Jump: true));
            _ = movement.ClearMovementIntent();
            pending.RecoverySent = true;
            return;
        }
        if (!_settings.DoJiggle || _settings.JumpOutWandCasting)
            return;
    }

    /// <summary>
    /// A physical attack is running at <paramref name="targetObjectId"/>, so
    /// its result text is ours to read.
    /// </summary>
    private void ArmPhysicalResultText(uint targetObjectId, string targetName)
    {
        _physicalResultArmed = true;
        _physicalResultTargetId = targetObjectId;
        _physicalResultTargetName = targetName ?? string.Empty;
        _physicalResultIncarnation = FindTarget(targetObjectId).Incarnation;
    }

    private void DisarmPhysicalResultText()
    {
        if (!_physicalResultArmed)
            return;
        _physicalResultArmed = false;
        _physicalCompletedAt = _now;
    }

    /// <summary>
    /// The melee/missile half of result reading. A swing produces no cast
    /// receipt, so the outcome of a physical attack is only ever visible in
    /// chat: this is what tells the macro the monster is dead, that a shot
    /// flew into the scenery, or that a swing landed.
    /// </summary>
    private void ObservePhysicalResultText(
        in PluginChatMessage message,
        in PluginCombatSnapshot combat)
    {
        // Read only while a swing is armed at our own target, or for a brief
        // moment after the sequence ended — the last swing's outcome line can
        // still arrive after the server has closed the attack.
        if (_physicalResultArmed)
        {
            if (combat.SelectedObjectId != _physicalResultTargetId)
                return;
        }
        else if (_now - _physicalCompletedAt > PhysicalResultTextTailSeconds)
        {
            return;
        }
        if (_physicalResultTargetId == 0u)
            return;
        PluginCombatTarget currentPhysicalTarget = FindTarget(_physicalResultTargetId);
        if (currentPhysicalTarget.ObjectId != 0u
            && currentPhysicalTarget.Incarnation != _physicalResultIncarnation)
            return;

        // Which log the line came from decides which of these arms may read
        // it at all: the miss notice and the kill sentence are plain lines,
        // and the damage report is the character's own combat log. A player
        // typing any of those sentences in chat carries a different type and
        // is ignored.
        string text = message.Text ?? string.Empty;
        if (message.LogTextType == CombatLogTextType.Default
            && string.Equals(
                text.Trim(),
                CombatResultText.MissileHitEnvironment,
                StringComparison.Ordinal))
        {
            AnnounceBlacklist(
                _failures.RecordMiss(_physicalResultTargetId, _now, _settings),
                _physicalResultTargetId,
                _physicalResultTargetName);
        }
        else if (message.LogTextType == CombatLogTextType.OwnCombat
            && CombatResultText.IsDamageReport(text))
        {
            _failures.ResetAttempts(_physicalResultTargetId);
        }

        if (message.LogTextType != CombatLogTextType.Default
            || !CombatResultText.IsKillingBlow(text, out string slain))
        {
            return;
        }

        // The looting hold goes up on the killing blow itself, before the
        // sentence is matched against our own target's name.
        ArmPostKillNavigationLock();

        // The sentence has to name OUR monster, letter for letter. An unnamed
        // stored target still refuses a sentence that names someone else.
        if (slain.Length > 0
            && !slain.Equals(
                _physicalResultTargetName,
                StringComparison.Ordinal))
        {
            return;
        }
        if (WieldingCleavingWeapon())
        {
            // A cleaving weapon can kill something other than the creature the
            // swing was aimed at, so the sentence does not identify our target.
            return;
        }

        Log?.Invoke(
            MacroLogChannel.CastInfo,
            $"AttackExecutor: Kill blow ({text})");
        _failures.ResetAttempts(_physicalResultTargetId);
        uint slainObjectId = _physicalResultTargetId;
        _host.Log.Info($"Target death attribution: physical target=0x{slainObjectId:X8}, name={_physicalResultTargetName}, message={text}");
        DisarmPhysicalResultText();
        EndKilledTarget(slainObjectId);
    }

    /// <summary>
    /// True when the wielded weapon (or a melee off-hand) cleaves, i.e. one
    /// swing can strike more than the creature it was aimed at.
    /// </summary>
    private bool WieldingCleavingWeapon()
    {
        IEquipmentAutomation equipment = _host.Automation.Equipment;
        if (!equipment.IsAvailable)
            return false;
        // The question is about the WIELDED weapon, not about everything worn:
        // a cleaving belt buckle does not make a kill sentence ambiguous. (The
        // shield slot has an arm of its own in the reference, but it re-reads
        // this same weapon's count, so it cannot change the answer.)
        IReadOnlyList<PluginEquipmentItem> items = PassEquipment();
        (uint weapon, _) = WieldedPair(items);
        if (weapon == 0u)
            return false;
        foreach (PluginEquipmentItem item in items)
        {
            if (item.ObjectId == weapon)
                return item.Cleaving > 1;
        }
        return false;
    }

    private void ObserveItemDebuffReceipts()
    {
        PluginCombatSnapshot combat = _host.Automation.Combat.Snapshot;
        foreach (PluginChatMessage message in
            _host.Automation.Chat.CaptureMessages(_observedChatSequence))
        {
            _observedChatSequence = Math.Max(
                _observedChatSequence,
                message.Sequence);
            ObservePhysicalResultText(in message, in combat);
            _castTracker.ObserveChat(
                message.Sequence,
                message.Text,
                // LOCAL speech only; the same test as
                // MossTankPanel.ObserveCastTrackerChat.
                ownSpeech: message.Kind == SpellCastTracker.LocalSpeechChatKind
                    && message.SenderObjectId != 0u
                    && message.SenderObjectId
                        == _host.Automation.Character.ObjectId,
                logTextType: (uint)message.LogTextType);
        }
    }

    /// <summary>
    /// The in-flight item transaction's own watcher. It reads the log and the
    /// item receipt on a reading position of its own, because the transaction
    /// outlives the passes the attack wins: it has to be able to see its own
    /// confirmation on a pass where the attack has no turn at all.
    /// </summary>
    private void ObserveItemTransaction()
    {
        foreach (PluginChatMessage message in
            _host.Automation.Chat.CaptureMessages(_itemTransactionChatSequence))
        {
            _itemTransactionChatSequence = Math.Max(
                _itemTransactionChatSequence,
                message.Sequence);
            // The wand's own confirmation is a magic-log line like any other
            // spell result.
            if (_pendingItemDebuff is not { } pending
                || message.LogTextType != CombatLogTextType.Magic
                || !IsMatchingCastLine(message.Text, pending.Source.Spell.Name))
            {
                continue;
            }
            _debuffs.RecordApplied(
                pending.TargetObjectId,
                pending.Source.Identity,
                pending.Source.Spell,
                _now);
            _failures.ResetAttempts(pending.TargetObjectId);
            Status = $"{pending.Source.Spell.Name} applied to {pending.TargetName}";
            if (!_settings.JumpOutWandCasting)
                StartSelectionJiggle(pending.Source.Spell);
            ClearPendingItemDebuff();
        }

        PluginItemUseCompletion itemCompletion =
            _host.Automation.Items.LastCompletion;
        if (itemCompletion.Revision <= _observedItemCompletion)
            return;
        _observedItemCompletion = itemCompletion.Revision;
        if (_pendingItemDebuff is not { } itemPending
            || itemPending.Source.Kind != CombatDebuffSourceKind.CasterItem
            || itemCompletion.SourceObjectId != itemPending.Source.ItemObjectId
            || itemCompletion.TargetObjectId != itemPending.TargetObjectId
            || itemCompletion.IsSuccess)
        {
            return;
        }
        Status = $"{itemPending.ItemName} failed (0x{itemCompletion.WeenieError:X})";
        ClearPendingItemDebuff();
    }

    private void OnCastTrackerOutcome(SpellCastOutcomeInfo info)
    {
        uint objectId = info.TargetObjectId;
        PluginCombatTarget currentTarget = FindTarget(objectId);
        if (objectId != 0u && currentTarget.ObjectId != 0u
            && currentTarget.Incarnation != info.TargetIncarnation)
            return;
        switch (info.Outcome)
        {
            case SpellCastOutcome.Kill:
                Log?.Invoke(
                    MacroLogChannel.CastInfo,
                    $"SpellCaster: Spell kill reset ({info.Text})");
                ArmPostKillNavigationLock();
                if (objectId == 0u)
                    return;
                // The health tracker lets the monster go the moment a killing
                // blow is credited to it.
                if (!info.HitsMultipleTargets
                    && objectId == _health.TargetObjectId)
                {
                    _health.Clear(_now);
                }
                _failures.ResetAttempts(objectId);
                // A spell that strikes several creatures cannot say WHICH one
                // the sentence is about, so the blow is recorded but the
                // target is not ended.
                if (!info.HitsMultipleTargets)
                {
                    _host.Log.Info($"Target death attribution: spell target=0x{objectId:X8}, message={info.Text}");
                    EndKilledTarget(objectId);
                }
                return;

            case SpellCastOutcome.PermanentFail:
                // The multiple-targets gate is applied by the tracker, which
                // owns that flag; reaching here means it passed.
                Log?.Invoke(
                    MacroLogChannel.CastInfo,
                    $"SpellCaster: Spell permanent fail reset ({info.Text})");
                if (objectId != 0u)
                    _failures.ForceBlacklist(objectId, _now, _settings);
                return;

            case SpellCastOutcome.Fail:
                Log?.Invoke(
                    MacroLogChannel.CastInfo,
                    $"SpellCaster: Spell fail reset ({info.Text})");
                return;

            case SpellCastOutcome.Success:
                Log?.Invoke(
                    MacroLogChannel.CastInfo,
                    $"SpellCaster: Spell success reset ({info.Text})");
                if (objectId == 0u)
                    return;
                // How big the blow was, taken off the running estimate. A
                // spell that strikes several creatures cannot say which one
                // the figure belongs to.
                if (!info.HitsMultipleTargets
                    && CombatResultText.TryReadSpellDamage(
                        info.Text,
                        out int points))
                {
                    _health.RecordDamage(objectId, points);
                }
                _failures.ResetAttempts(objectId);
                return;

            case SpellCastOutcome.ResultTimeout:
                Log?.Invoke(
                    MacroLogChannel.CastInfo,
                    "SpellCaster: Cast result timeout");
                if (objectId != 0u && !info.HitsMultipleTargets)
                {
                    AnnounceBlacklist(
                        _failures.RecordMiss(objectId, _now, _settings),
                        objectId,
                        info.TargetName);
                }
                return;

            case SpellCastOutcome.LaunchTimeout:
                Log?.Invoke(
                    MacroLogChannel.CastInfo,
                    "SpellCaster: Attempt timeout");
                return;

            case SpellCastOutcome.Rejected:
                Log?.Invoke(
                    MacroLogChannel.CastInfo,
                    $"SpellCaster: Cast refused (0x{info.WeenieError:X4})");
                return;
        }
    }

    private void EndKilledTarget(uint objectId)
    {
        PluginCombatTarget observed = FindTarget(objectId);
        _host.Log.Info($"Target marked dead: 0x{objectId:X8}, name={observed.Name}, healthKnown={observed.IsHealthKnown}, health={observed.HealthFraction}, healthRevision={observed.HealthRevision}, incarnation={observed.Incarnation}");
        _failures.ReportDeath(objectId, _now, observed.HealthRevision);
        if (_pendingAttackTarget == objectId)
        {
            _pendingAttackSpell = 0u;
            _pendingAttackTarget = 0u;
        }
        if (_targetId != objectId)
            return;
        _host.Automation.Combat.AbortPhysicalAttack();
        ClearTarget();
        _untilScan = 0d;
        Status = "Waiting for a target";
    }

    /// <summary>
    /// <c>gj</c>'s <c>this.m_g.HitsMultipleTargets</c>, which now lives on the
    /// tracker that owns <c>m_g</c>.
    /// </summary>
    private static bool HitsMultipleTargets(in PluginSpellInfo spell) =>
        SpellCastTracker.HitsMultipleTargetsFor(spell);

    private static bool IsMatchingCastLine(string text, string spellName) =>
        text.StartsWith($"You cast {spellName} on ", StringComparison.Ordinal);

    private void ObserveSelectionJiggle(in PluginCastCompletion completion)
    {
        if (_host.Automation.Magic.IsCasting)
        {
            StopSelectionJiggle();
            return;
        }
        if (completion.Revision <= _observedJiggleCastCompletion)
            return;
        _observedJiggleCastCompletion = completion.Revision;
        if (completion.IsSuccess
            && _host.Automation.Spells.TryGet(
                completion.SpellId,
                out PluginSpellInfo spell))
        {
            StartSelectionJiggle(spell);
        }
    }

    private void StartSelectionJiggle(in PluginSpellInfo spell)
    {
        if (!_settings.DoJiggle
            || (IsVtankInstantCast(spell)
                && spell.School is 34u or 43u))
        {
            return;
        }
        ISelectionAutomation selection = _host.Automation.Selection;
        if (!selection.Execute(PluginSelectionAction.PreviousSelection))
            return;
        _selectionJiggleActive = true;
        _selectionJigglePreviousPlayer = false;
        _nextSelectionJiggleAt = _now;
        _selectionJiggleUntil = _now + SpellCastTracker.ResultTickSeconds;
    }

    private void TickSelectionJiggle()
    {
        if (!_selectionJiggleActive)
            return;
        if (_now >= _selectionJiggleUntil)
        {
            StopSelectionJiggle();
            return;
        }
        if (_now < _nextSelectionJiggleAt)
            return;
        ISelectionAutomation selection = _host.Automation.Selection;
        int pulses = 0;
        do
        {
            PluginSelectionAction action = _selectionJigglePreviousPlayer
                ? PluginSelectionAction.PreviousPlayer
                : PluginSelectionAction.NextPlayer;
            if (!selection.Execute(action))
            {
                StopSelectionJiggle();
                return;
            }
            _selectionJigglePreviousPlayer = !_selectionJigglePreviousPlayer;
            _nextSelectionJiggleAt += 0.131d;
        }
        while (_now >= _nextSelectionJiggleAt && ++pulses < 8);
    }

    private void StopSelectionJiggle()
    {
        _selectionJiggleActive = false;
        _selectionJigglePreviousPlayer = false;
        _nextSelectionJiggleAt = 0d;
        _selectionJiggleUntil = 0d;
    }

    private static bool IsVtankInstantCast(in PluginSpellInfo spell)
    {
        if (spell.Difficulty < 50)
            return true;
        if (spell.IsUntargeted
            && !spell.IsFellowship
            && spell.DurationSeconds >= 60f
            && spell.School is 31u or 33u)
        {
            return true;
        }
        return spell.Family is >= 243u and <= 249u or 639u;
    }

    private static string ItemName(
        uint objectId,
        IReadOnlyList<PluginInventoryItem> inventory)
    {
        foreach (PluginInventoryItem item in inventory)
        {
            if (item.ObjectId == objectId)
                return item.Name;
        }
        return $"0x{objectId:X8}";
    }

    private void RefreshSpellCatalogs()
    {
        ISpellCatalog catalog = _host.Automation.Spells;
        IReadOnlyList<PluginSpellInfo> spells = catalog.KnownCombatSpells;
        IReadOnlyList<PluginSpellInfo> attacks = catalog.KnownAttackSpells;
        if (ReferenceEquals(spells, _combatSpellSnapshot)
            && ReferenceEquals(attacks, _attackSpellSnapshot))
        {
            return;
        }
        _combatSpellSnapshot = spells;
        _attackSpellSnapshot = attacks;

        if (attacks.Count == 0)
        {
            _attackCatalog = AttackSpellCatalog.Build(spells);
            return;
        }
        var union = new List<PluginSpellInfo>(spells.Count + attacks.Count);
        var seen = new HashSet<uint>();
        foreach (PluginSpellInfo spell in spells)
        {
            if (seen.Add(spell.SpellId))
                union.Add(spell);
        }
        foreach (PluginSpellInfo spell in attacks)
        {
            if (seen.Add(spell.SpellId))
                union.Add(spell);
        }
        _attackCatalog = AttackSpellCatalog.Build(union);
    }

    private PluginCombatTarget FindTarget(uint objectId)
    {
        foreach (PluginCombatTarget target in _targets)
        {
            if (target.ObjectId == objectId)
                return target;
        }
        return default;
    }

    private double _nextTargetDiagnostic;

    private void TraceTargetSelection(IReadOnlyList<CombatTargetCandidate> candidates,
        uint chosen)
    {
        if (chosen != 0u || _now < _nextTargetDiagnostic)
            return;
        _nextTargetDiagnostic = _now + 5d;
        IReadOnlyList<PluginCombatTarget> visible =
            _host.Automation.Combat.CaptureHostileTargets(float.MaxValue);
        Log?.Invoke(MacroLogChannel.RuleInfo, $"Target scan: no selection, hostiles={visible.Count}, candidates={candidates.Count}, range={_settings.MinimumRange:F2}..{_acquisitionRange:F2}, selected=0x{(_host.Selection.SelectedObjectId ?? 0u):X8}");
        foreach (PluginCombatTarget target in visible)
        {
            ResolvedMonsterRule rule = _settings.ResolveRule(target);
            CombatSuppressionReason suppression = _failures.Reason(target.ObjectId, _now);
            string reason = target.Distance > _acquisitionRange ? "outside maximum range"
                : target.Distance < _settings.MinimumRange ? "inside minimum range"
                : suppression != CombatSuppressionReason.None ? suppression.ToString()
                : _passInvalidTargets.Contains(target.ObjectId) ? "invalidated during attack pass"
                : rule.Priority < 0 ? "negative rule priority"
                : _passCandidates.TryGetValue(target.ObjectId, out CombatTargetCandidate? candidate)
                    ? candidate is null ? "no attack or due debuff after pass filtering" : "eligible candidate"
                : "absent from current acquisition snapshot";
            Log?.Invoke(MacroLogChannel.RuleInfo, $"Target check: {target.Name} (0x{target.ObjectId:X8}), distance={target.Distance:F2}, angle={target.RelativeAngleDegrees:F1}, reason={reason}, rule={rule.Rule.Expression}, priority={rule.Priority}, attacks={rule.Actions.Attacks}, streak={rule.Actions.UsesStreak}");
        }
    }

    private void RefreshTarget()
    {
        if (_targetId != 0u
            && _failures.Reason(_targetId, _now)
                != CombatSuppressionReason.None)
        {
            _host.Automation.Combat.AbortPhysicalAttack();
            ClearTarget();
        }

        PluginCombatSnapshot combat = _host.Automation.Combat.Snapshot;

        // Selection is NOT frozen for the life of an engagement. The only
        // things that stop the macro re-picking are the pass hold (a cast or a
        // turn in flight) and the item-use cooldown, both of which sit above
        // this rule; a swing loop runs beside the pass and never blocks it. A
        // higher-priority monster arriving mid-fight has to be able to win.
        RefreshSpellCatalogs();
        IReadOnlyList<PluginInventoryItem> inventory =
            PassInventory();
        IReadOnlyList<PluginEquipmentItem> equipment = PassEquipment();
        (uint wieldedWeapon, uint wieldedOffhand) = WieldedPair(equipment);

        uint lastTarget = _lastTargetId;
        var candidates = new List<CombatTargetCandidate>();
        _ringCandidateCount = 0;
        foreach (PluginCombatTarget target in _targets)
        {
            if (!TryBuildCandidate(
                    target,
                    combat,
                    lastTarget,
                    inventory,
                    equipment,
                    _acquisitionRange,
                    out CombatTargetCandidate candidate))
            {
                continue;
            }
            candidates.Add(candidate);
            if (candidate.Distance < _settings.RingDistance
                && candidate.Rule.Actions.UsesRing)
            {
                _ringCandidateCount++;
            }
        }

        CombatTargetCandidate chosen = CombatTargetSelector.Select(
            candidates,
            _settings.DebuffEachFirst,
            _settings.SelectionMethod,
            _settings.TargetSelectAngleRange,
            wieldedWeapon,
            wieldedOffhand);

        // No target chosen: drop whatever the pass was holding.
        TraceTargetSelection(candidates, chosen.ObjectId);
        if (chosen.ObjectId == 0u)
        {
            if (_targetId != 0u)
                _host.Automation.Combat.AbortPhysicalAttack();
            ClearTarget();
            return;
        }
        // The STORED rule is the authored one: what this pass learned about
        // the monster is applied where the decision reads it, so it cannot
        // outlive the pass and hold a column off between scans.
        SetTarget(chosen.Target, _settings.ResolveRule(chosen.Target));
    }

    private static (uint Weapon, uint Offhand) WieldedPair(
        IReadOnlyList<PluginEquipmentItem> equipment)
    {
        const uint weaponReadyMask = 0x03500000u;
        const uint shieldMask = 0x00000200u;
        uint weapon = 0u;
        uint offhand = 0u;
        foreach (PluginEquipmentItem item in equipment)
        {
            if (!item.IsEquipped)
                continue;
            if (weapon == 0u && (item.ValidLocations & weaponReadyMask) != 0u)
                weapon = item.ObjectId;
            else if (offhand == 0u && (item.ValidLocations & shieldMask) != 0u)
                offhand = item.ObjectId;
        }
        return (weapon, offhand);
    }

    /// <summary>
    /// The six ordered rejection gates, then the fill.
    /// </summary>
    private bool TryBuildCandidate(
        in PluginCombatTarget target,
        in PluginCombatSnapshot combat,
        uint lastTarget,
        IReadOnlyList<PluginInventoryItem> inventory,
        IReadOnlyList<PluginEquipmentItem> equipment,
        double maximumRange,
        out CombatTargetCandidate candidate)
    {
        candidate = default;

        if (_passCandidateRange != maximumRange)
        {
            _passCandidateRange = maximumRange;
            _passCandidates.Clear();
        }
        if (_passCandidates.TryGetValue(
                target.ObjectId,
                out CombatTargetCandidate? memo))
        {
            if (memo is not { } built)
                return false;
            candidate = built;
            return true;
        }
        if (_passInvalidTargets.Contains(target.ObjectId))
            return false;

        if (_failures.Reason(target.ObjectId, _now)
            != CombatSuppressionReason.None)
        {
            _passCandidates[target.ObjectId] = null;
            return false;
        }

        // Gate 3: the monster's rule must want it attacked at all.
        ResolvedMonsterRule rule = WithPassClearedActions(
            target.ObjectId,
            _settings.ResolveRule(target));
        if (rule.Priority < 0)
        {
            _passCandidates[target.ObjectId] = null;
            return false;
        }

        if (target.Distance > maximumRange
            || target.Distance < _settings.MinimumRange)
        {
            // Range is the one gate that depends on which range was asked
            // about, so it is not memoised.
            return false;
        }

        MonsterRuleActions actions = rule.Actions;
        (uint weapon, uint offhand, MonsterDamageType element) = ResolveWieldPlan(
            actions,
            target,
            inventory,
            equipment);
        IReadOnlyList<CombatDebuffStep> steps = CombatDebuffChain.Build(
            actions,
            element,
            ResolveExtraVulnerability(actions, target));
        PluginCombatTarget candidateTarget = target;
        bool needsDebuff = CombatDebuffChain.NeedsDebuff(
            steps,
            step => IsDebuffStepDue(step, in candidateTarget, inventory));

        if (!needsDebuff && !actions.Attacks && !actions.UsesStreak)
        {
            _passCandidates[target.ObjectId] = null;
            return false;
        }

        candidate = new CombatTargetCandidate(
            target,
            rule,
            rule.Priority,
            target.Distance,
            Math.Abs(target.RelativeAngleDegrees),
            DebuffUrgency(target, element, weapon, equipment),
            needsDebuff,
            _settings.TargetLock
                && combat.SelectedObjectId != 0u
                && combat.SelectedObjectId == target.ObjectId,
            lastTarget != 0u && target.ObjectId == lastTarget,
            weapon,
            offhand);
        _passCandidates[target.ObjectId] = candidate;
        return true;
    }

    private (uint Weapon, uint Offhand, MonsterDamageType Element) ResolveWieldPlan(
        MonsterRuleActions actions,
        in PluginCombatTarget target,
        IReadOnlyList<PluginInventoryItem> inventory,
        IReadOnlyList<PluginEquipmentItem> equipment)
    {
        MonsterDamageType element = ResolveAttackElement(actions, target);
        uint weapon = actions.WeaponToUseRaw == 0
            ? 0u
            : ResolveEquipmentObjectId(
                actions.WeaponObjectId,
                actions.WeaponName,
                equipment);
        if (weapon == 0u && actions.WeaponToUseRaw != 0)
            weapon = SelectAutomaticWeapon(equipment, actions, target);
        uint offhand = ResolveEquipmentObjectId(
            actions.OffhandObjectId,
            actions.OffhandName,
            equipment);
        if (offhand == 0u)
        {
            offhand = ResolveInventoryObjectId(
                actions.OffhandObjectId,
                actions.OffhandName,
                inventory);
        }
        return (weapon, offhand, element);
    }

    private MonsterDamageType ResolveAttackElement(
        MonsterRuleActions actions,
        in PluginCombatTarget target)
    {
        (MonsterRuleActions, uint) key = (actions, target.ObjectId);
        if (_passElements.TryGetValue(key, out MonsterDamageType cached))
            return cached;
        MonsterDamageType resolved = ResolveAttackElementCore(actions, target);
        _passElements[key] = resolved;
        return resolved;
    }

    private MonsterDamageType ResolveAttackElementCore(
        MonsterRuleActions actions,
        in PluginCombatTarget target)
    {
        MonsterDamageType requested = actions.DamageType;
        if (requested == MonsterDamageType.Fists)
            return MonsterDamageType.Bludgeon;
        // Auto and Prismatic both resolve the element; Prismatic differs only
        // in which ammunition it will accept.
        if (requested is not (MonsterDamageType.Auto or MonsterDamageType.Prismatic))
            return requested;

        IReadOnlyList<PluginEquipmentItem> owned = PassEquipment();
        uint weapon = PlannedWeaponFor(actions, in target, owned);
        PluginCombatMode kind = WeaponStance(actions, weapon, owned);

        // The weapon's OWN element: what its imbue rends, then what it cleaves,
        // then the damage it plainly deals.
        MonsterDamageType element = WeaponElement(weapon, owned);

        // Only a wand's element falls back to the caster's training. A melee
        // or missile build with no war magic must not be handed Void or drain.
        if (kind == PluginCombatMode.Magic)
        {
            MonsterDamageType cascade =
                AttackSpellCatalog.ResolveMagicDamageMode(
                    MonsterDamageType.Auto,
                    _host.Automation.Character);
            if (cascade == MonsterDamageType.VoidBasic)
                return cascade;
            if (cascade == MonsterDamageType.DrainAuto)
            {
                PostAttackWarning(
                    "Warning: autoselecting drain as damage type. If you are "
                    + "not a martyr mage, you probably need to add your "
                    + "weapons to the items tab.");
                return cascade;
            }
        }
        if (element != MonsterDamageType.None)
            return element;

        IReadOnlyList<MonsterDamageType> preferences =
            _gameInfo.DamagePreferences(target.Name);
        foreach (MonsterDamageType preference in preferences)
        {
            if (preference != MonsterDamageType.None
                && CanDeliverElement(preference, weapon, owned))
            {
                return preference;
            }
        }

        foreach (MonsterDamageType unlisted in VtankDamageDatabase.UnlistedElementOrder)
        {
            if (Contains(preferences, unlisted)
                || !CanDeliverElement(unlisted, weapon, owned))
            {
                continue;
            }
            PostAttackWarning(
                "Warning: no ammunition available for any of target's possible "
                + "damage types! Using unlisted damage type: "
                + ElementName(unlisted));
            return unlisted;
        }
        PostAttackWarning("Warning: no ammunition available!!!");
        return MonsterDamageType.None;
    }

    /// <summary>
    /// The weapon this rule will fight with: the one it names, else the one
    /// automatic selection would reach for. A rule that spells the weapon
    /// column as zero means "no weapon, use a wand" and names none.
    /// </summary>
    private uint PlannedWeaponFor(
        MonsterRuleActions actions,
        in PluginCombatTarget target,
        IReadOnlyList<PluginEquipmentItem> owned)
    {
        if (actions.WeaponToUseRaw == 0)
            return 0u;
        uint named = ResolveEquipmentObjectId(
            actions.WeaponObjectId,
            actions.WeaponName,
            owned);
        if (named != 0u)
            return named;
        uint automatic = SelectAutomaticWeapon(owned, actions, in target);
        if (automatic != 0u)
            return automatic;
        // Nothing was named and nothing could be picked for the element the
        // rule asked for, so the weapon already in hand is what the fight will
        // be had with.
        return CombatModeGate.FindWielded(owned)?.ObjectId ?? 0u;
    }

    /// <summary>
    /// The stance the planned weapon implies. With no weapon at all the rule
    /// means a wand, so the stance is Magic.
    /// </summary>
    private static PluginCombatMode WeaponStance(
        MonsterRuleActions actions,
        uint weapon,
        IReadOnlyList<PluginEquipmentItem> owned)
    {
        if (weapon == 0u)
            return PluginCombatMode.Magic;
        foreach (PluginEquipmentItem item in owned)
        {
            if (item.ObjectId == weapon)
                return CombatModeGate.ModeFor(in item);
        }
        return actions.WeaponToUseRaw == 0
            ? PluginCombatMode.Magic
            : PluginCombatMode.Melee;
    }

    private static MonsterDamageType WeaponElement(
        uint weapon,
        IReadOnlyList<PluginEquipmentItem> owned)
    {
        if (weapon == 0u)
            return MonsterDamageType.None;
        foreach (PluginEquipmentItem item in owned)
        {
            if (item.ObjectId == weapon)
            {
                return VtankWeaponElement.Resolve(
                    item.ImbuedEffect,
                    item.ResistanceCleaving,
                    item.DamageType);
            }
        }
        return MonsterDamageType.None;
    }

    /// <summary>
    /// The character's equipment as this pass sees it. The host builds that
    /// projection by walking every object it knows and sorting the result, so
    /// it is read once per pass and shared: a pass that has to choose again
    /// ten times over unreachable monsters must not walk the world ten times.
    /// Anything that changes what is worn ends the pass, so the pass can
    /// never act on a stale answer.
    /// </summary>
    private IReadOnlyList<PluginEquipmentItem> PassEquipment()
    {
        if (_passEquipment is not null)
            return _passEquipment;
        IEquipmentAutomation equipment = _host.Automation.Equipment;
        _passEquipment = equipment.IsAvailable
            ? equipment.CaptureOwnedEquipment()
            : Array.Empty<PluginEquipmentItem>();
        return _passEquipment;
    }

    /// <summary>
    /// The character's carried items as this pass sees it, on the same terms
    /// as <see cref="PassEquipment"/>.
    /// </summary>
    private IReadOnlyList<PluginInventoryItem> PassInventory() =>
        _passInventory ??= _host.Automation.Items.CaptureOwnedItems();

    private static bool Contains(
        IReadOnlyList<MonsterDamageType> elements,
        MonsterDamageType element)
    {
        for (int i = 0; i < elements.Count; i++)
        {
            if (elements[i] == element)
                return true;
        }
        return false;
    }

    private MonsterDamageType? RuleWeaponElement(MonsterRuleActions actions)
    {
        IReadOnlyList<PluginEquipmentItem> owned = PassEquipment();
        if (owned.Count == 0)
            return null;
        uint weapon = ResolveEquipmentObjectId(
            actions.WeaponObjectId,
            actions.WeaponName,
            owned);
        if (weapon == 0u)
            return null;
        foreach (PluginEquipmentItem item in owned)
        {
            if (item.ObjectId == weapon)
                return ProtocolDamageElement(item.DamageType);
        }
        return null;
    }

    private static MonsterDamageType? ProtocolDamageElement(int damageType) =>
        (damageType & 0x0020) != 0 ? MonsterDamageType.Acid
        : (damageType & 0x0004) != 0 ? MonsterDamageType.Bludgeon
        : (damageType & 0x0008) != 0 ? MonsterDamageType.Cold
        : (damageType & 0x0010) != 0 ? MonsterDamageType.Fire
        : (damageType & 0x0040) != 0 ? MonsterDamageType.Electric
        : (damageType & 0x0001) != 0 ? MonsterDamageType.Slash
        : (damageType & 0x0002) != 0 ? MonsterDamageType.Pierce
        : (damageType & 0x0400) != 0 ? MonsterDamageType.VoidBasic
        : null;

    /// <summary>
    /// Whether the weapon in hand can actually put this element on a monster.
    /// Only a launcher can fail: it needs ammunition of that element. Every
    /// other weapon, and a wand, can always deliver.
    /// </summary>
    private bool CanDeliverElement(
        MonsterDamageType element,
        uint weapon,
        IReadOnlyList<PluginEquipmentItem> owned)
    {
        (MonsterDamageType, uint) key = (element, weapon);
        if (_passDeliverable.TryGetValue(key, out bool cached))
            return cached;
        bool deliverable = CanDeliverElementCore(element, weapon, owned);
        _passDeliverable[key] = deliverable;
        if (!deliverable)
        {
            PostAttackWarning(
                "Warning: bow with element " + ElementName(element)
                + " ignored because ammunition is not available.");
        }
        return deliverable;
    }

    private bool CanDeliverElementCore(
        MonsterDamageType element,
        uint weapon,
        IReadOnlyList<PluginEquipmentItem> owned)
    {
        int launcherType = 0;
        foreach (PluginEquipmentItem item in owned)
        {
            if (item.ObjectId != weapon)
                continue;
            launcherType = VtankAmmunitionDatabase.LauncherType(item.AmmoType);
            break;
        }
        if (launcherType == 0)
            return true;

        return VtankAmmunitionDatabase.Select(
            _gameInfo.AmmunitionOptions.Count > 0
                ? _gameInfo.AmmunitionOptions
                : VtankAmmunitionDatabase.Options,
            launcherType,
            element,
            VtankPrismaticAmmoPolicy.Any,
            _settings.UseSpecialAmmo,
            _host.Automation.Character,
            AmmunitionAvailability()) is not null;
    }

    private Func<string, bool>? _passAmmunitionAvailability;

    /// <summary>
    /// Whether a named stack of ammunition is in the pack, or could be made.
    /// Answered once per pass per name.
    /// </summary>
    private Func<string, bool> AmmunitionAvailability()
    {
        if (_passAmmunitionAvailability is not null)
            return _passAmmunitionAvailability;

        Dictionary<string, int>? counts = null;
        var answers = new Dictionary<string, bool>(StringComparer.OrdinalIgnoreCase);
        _passAmmunitionAvailability = name =>
        {
            if (answers.TryGetValue(name, out bool cached))
                return cached;
            counts ??= PassInventory()
                .GroupBy(static item => item.Name, StringComparer.OrdinalIgnoreCase)
                .ToDictionary(
                    static group => group.Key,
                    static group => group.Sum(item => Math.Max(1, item.StackSize)),
                    StringComparer.OrdinalIgnoreCase);
            bool answer = counts.GetValueOrDefault(name) >= 1
                || _canCraftAmmunition?.Invoke(name, 1) == true;
            answers[name] = answer;
            return answer;
        };
        return _passAmmunitionAvailability;
    }

    private MonsterDamageType ResolveExtraVulnerability(
        MonsterRuleActions actions,
        in PluginCombatTarget target)
    {
        switch (actions.ExtraVulnerability)
        {
            case MonsterDamageType.Pierce:
            case MonsterDamageType.Bludgeon:
            case MonsterDamageType.Slash:
            case MonsterDamageType.Acid:
            case MonsterDamageType.Electric:
            case MonsterDamageType.Cold:
            case MonsterDamageType.Fire:
                return actions.ExtraVulnerability;
            case MonsterDamageType.Auto:
                IReadOnlyList<MonsterDamageType> preferences =
                    _gameInfo.DamagePreferences(target.Name);
                return preferences.Count > 0
                    ? preferences[0]
                    : MonsterDamageType.None;
            default:
                return MonsterDamageType.None;
        }
    }

    private int DebuffUrgency(
        in PluginCombatTarget target,
        MonsterDamageType element,
        uint plannedWeapon,
        IReadOnlyList<PluginEquipmentItem> equipment)
    {
        const uint meleeWeapon = 0x00000001u;
        const uint missileWeapon = 0x00000100u;
        int score = 0;
        var vulnerability = new DebuffIdentity(
            MonsterActionFlags.Vulnerability,
            element);
        if (_debuffs.IsApplied(
                target.ObjectId,
                vulnerability,
                FindDebuffSpell(vulnerability),
                _now))
        {
            score++;
        }

        bool physical = false;
        if (plannedWeapon != 0u)
        {
            foreach (PluginEquipmentItem item in equipment)
            {
                if (item.ObjectId != plannedWeapon)
                    continue;
                physical = (item.ItemType & (meleeWeapon | missileWeapon)) != 0u;
                break;
            }
        }
        var identity = new DebuffIdentity(
            physical ? MonsterActionFlags.Imperil : MonsterActionFlags.Yield,
            MonsterDamageType.Auto);
        if (_debuffs.IsApplied(
                target.ObjectId,
                identity,
                FindDebuffSpell(identity),
                _now))
        {
            score += 2;
        }
        return score;
    }

    private PluginSpellInfo? FindDebuffSpell(DebuffIdentity identity)
    {
        if (_passDebuffSpells.TryGetValue(identity, out PluginSpellInfo? cached))
            return cached;
        PluginSpellInfo? found = FindDebuffSpellCore(identity);
        _passDebuffSpells[identity] = found;
        return found;
    }

    private PluginSpellInfo? FindDebuffSpellCore(DebuffIdentity identity)
    {
        PluginSpellInfo? best = null;
        foreach (PluginSpellInfo spell in _host.Automation.Spells.KnownCombatSpells)
        {
            if (!DebuffSpellCatalog.TryClassify(spell, out DebuffIdentity found, out _)
                || found != identity)
            {
                continue;
            }
            if (best is not { } current || spell.Tier > current.Tier)
                best = spell;
        }
        return best;
    }

    private IReadOnlyList<CombatDebuffSource> DebuffSources(
        DebuffIdentity identity,
        in PluginCombatTarget target,
        IReadOnlyList<PluginInventoryItem> inventory,
        Action<string>? log)
    {
        (DebuffIdentity, uint) key = (identity, target.ObjectId);
        if (_passDebuffSources.TryGetValue(
                key,
                out IReadOnlyList<CombatDebuffSource>? cached))
        {
            return cached;
        }
        IReadOnlyList<CombatDebuffSource> sources = CombatItemDebuffPlanner.Sources(
            identity,
            _settings,
            _host.Automation.Character,
            _host.Automation.Spells,
            inventory,
            target.Distance,
            log);
        _passDebuffSources[key] = sources;
        return sources;
    }

    /// <summary>
    /// The one source this step will be applied from, or null when there is
    /// none. Exactly one wins: a source the character cannot use is not
    /// quietly replaced by the next-best one inside a single decision.
    /// </summary>
    private CombatDebuffSource? ChooseDebuffSource(
        DebuffIdentity identity,
        in PluginCombatTarget target,
        IReadOnlyList<PluginInventoryItem> inventory,
        Action<string>? log = null)
    {
        IReadOnlyList<CombatDebuffSource> sources = DebuffSources(
            identity,
            in target,
            inventory,
            log);
        foreach (CombatDebuffSource source in sources)
        {
            // With the fallback allowed, a source whose flight this pass has
            // ALREADY found blocked steps aside for the next-best one. With it
            // off there is no stepping aside: the winner stands and its column
            // is turned off when its flight turns out to be blocked.
            if (_settings.AllowDebuffFallback
                && !KnownClear(target.ObjectId, source.PathKind))
            {
                continue;
            }
            return source;
        }
        return null;
    }

    private bool IsDebuffStepDue(
        CombatDebuffStep step,
        in PluginCombatTarget target,
        IReadOnlyList<PluginInventoryItem> inventory)
    {
        IReadOnlyList<CombatDebuffSource> sources = DebuffSources(
            step.Identity,
            in target,
            inventory,
            log: null);
        if (sources.Count == 0)
            return false;
        return _debuffs.IsDue(
            target.ObjectId,
            step.Identity,
            sources[0].Spell,
            _now,
            step.ZeroTolerance ? 0d : _settings.DebuffPrecastSeconds);
    }

    private bool TryFind(uint objectId, out PluginCombatTarget found)
    {
        foreach (PluginCombatTarget target in _targets)
        {
            if (target.ObjectId == objectId)
            {
                found = target;
                return true;
            }
        }
        found = default;
        return false;
    }

    private void SetTarget(
        PluginCombatTarget target,
        ResolvedMonsterRule resolved)
    {
        _targetId = target.ObjectId;
        _targetRule = resolved;
        _targetName = string.IsNullOrWhiteSpace(target.Name)
            ? $"0x{target.ObjectId:X8}"
            : target.Name;
        _targetDistance = target.Distance;
        _targetText = string.Create(
            CultureInfo.InvariantCulture, $"Target  {_targetName}  {_targetDistance:0.0}m");
        _health.SetTarget(_targetId, target.Name, _now);
        // Whatever the host already knows about this monster's health counts
        // as the first report, so the fight does not start a scan behind.
        _health.Observe(target, _now);
    }

    private void ClearTarget()
    {
        _health.Clear(_now);
        DisarmPhysicalResultText();
        StopApproachMovement();
        StopBreakableTurnMovement();
        StopSelectionJiggle();
        _targetId = 0u;
        _targetRule = default;
        _targetName = string.Empty;
        _targetDistance = 0f;
        _targetText = "Target  —";
    }

    private void Disable(string status)
    {
        _host.Automation.Combat.AbortPhysicalAttack();
        StopApproachMovement();
        StopBreakableTurnMovement();
        Enabled = false;
        _paused = false;
        _combatPolicySuspended = false;
        _targets = Array.Empty<PluginCombatTarget>();
        _combatSpellSnapshot = null;
        _attackSpellSnapshot = null;
        _attackCatalog = AttackSpellCatalog.Build(Array.Empty<PluginSpellInfo>());
        _debuffs.Reset();
        _failures.Reset();
        _health.Reset();
        _observedPhysicalCompletion = 0;
        _observedAttackCastCompletion = 0;
        _pendingPhysicalTarget = 0u;
        _pendingAttackSpell = 0u;
        _pendingAttackTarget = 0u;
        ClearPendingItemDebuff();
        _observedChatSequence = 0u;
        _itemTransactionChatSequence = 0u;
        _observedItemCompletion = 0;
        // The tracker's own reset: a stopped macro must not leave the busy
        // latch up.
        _castTracker.Reset();
        _plannedWeapon = 0u;
        Gate.Reset();
        _randomDamageIndex = 0;
        _observedJiggleCastCompletion = 0;
        ClearTarget();
        Status = status;
        _combatControl?.Dispose();
        _combatControl = null;
    }

    /// <summary>
    /// Walking to a monster the character cannot yet hit. This is its OWN job,
    /// twenty positions below the attack, with its own candidate pick at the
    /// approach range: the attack must not claim the pass for a monster it
    /// would have to walk to, or nothing below the attack ever runs.
    /// </summary>
    /// <returns>True while there is a monster worth walking to.</returns>
    /// <summary>
    /// The reference's gate on the monster approach's idle-peace fallback:
    /// true while the monster worth walking to is further than the creep
    /// distance, or while there is none.
    /// </summary>
    internal bool IsApproachOutsideCreepDistance()
    {
        if (!Enabled || !_settings.Enabled || !_host.Automation.IsAvailable)
            return true;
        ClearPassMemos();
        return SelectApproachTarget() is not { } approach
            || approach.Distance >= NavigationMover.CreepDistanceMeters;
    }

    internal bool TickMonsterApproach(double elapsedSeconds, bool canAct)
    {
        if (!Enabled
            || !_settings.Enabled
            || !_host.Automation.IsAvailable
            || !canAct
            || _settings.ApproachDistance <= _settings.MaximumRange)
        {
            StopApproachMovement();
            return false;
        }

        // This rule's own clock: the attack's turn and this one are asked at
        // different times, so the re-face throttle below is paced against
        // the time THIS rule has been handed, not the attack's clock.
        _approachClock += Math.Max(0d, elapsedSeconds);
        // This rule is its own pass: the attack's may not have run at all (its
        // gate can refuse for seconds at a time), so everything the attack
        // pass learns and forgets per pass is taken fresh here rather than
        // inherited stale — a column another pass turned off must not silently
        // narrow the walk's choice of monster.
        ClearPassMemos();
        if (SelectApproachTarget() is not { } approach)
        {
            StopApproachMovement();
            return false;
        }
        // The walk ends where the attack begins.
        if (approach.Distance <= _settings.MaximumRange)
        {
            StopApproachMovement();
            return false;
        }
        return TickApproachTo(
            approach.ObjectId,
            approach.Target.Name,
            approach.Distance);
    }

    /// <summary>
    /// The same comparison chain the attack runs, over the monsters inside the
    /// approach range rather than the ones inside weapon range.
    /// </summary>
    private CombatTargetCandidate? SelectApproachTarget()
    {
        IReadOnlyList<PluginCombatTarget> reachable =
            _host.Automation.Combat.CaptureHostileTargets(
                (float)_settings.ApproachDistance);
        if (reachable.Count == 0)
            return null;

        PluginCombatSnapshot combat = _host.Automation.Combat.Snapshot;
        RefreshSpellCatalogs();
        IReadOnlyList<PluginInventoryItem> inventory =
            PassInventory();
        IReadOnlyList<PluginEquipmentItem> equipment = PassEquipment();
        (uint wieldedWeapon, uint wieldedOffhand) = WieldedPair(equipment);

        var candidates = new List<CombatTargetCandidate>();
        foreach (PluginCombatTarget target in reachable)
        {
            if (TryBuildCandidate(
                    target,
                    combat,
                    _lastTargetId,
                    inventory,
                    equipment,
                    _settings.ApproachDistance,
                    out CombatTargetCandidate candidate))
            {
                candidates.Add(candidate);
            }
        }
        if (candidates.Count == 0)
            return null;

        CombatTargetCandidate chosen = CombatTargetSelector.Select(
            candidates,
            _settings.DebuffEachFirst,
            _settings.SelectionMethod,
            _settings.TargetSelectAngleRange,
            wieldedWeapon,
            wieldedOffhand);
        return chosen.ObjectId == 0u ? null : chosen;
    }

    private bool TickApproachTo(uint objectId, string name, double distance)
    {
        INavigationAutomation navigation = _host.Automation.Navigation;
        PluginNavigationSnapshot self = navigation.Snapshot;
        if (!self.IsAvailable || self.IsPortalSpace
            || !navigation.TryGetObject(
                objectId,
                out PluginNavigationObject target))
        {
            StopApproachMovement();
            return false;
        }

        float desired = NavigationController.DesiredHeading(
            self.Position,
            target.Position);
        float delta = NavigationController.SignedHeadingDelta(
            self.Position.HeadingDegrees,
            desired);

        if (NavigationController.SteerTowards(
                navigation,
                delta,
                desired,
                _approachClock,
                ref _approachFaceHeadingStamp,
                run: true)
            != PluginNavigationCommandStatus.Accepted)
        {
            _approachMovementOwned = false;
            return false;
        }

        _approachMovementOwned = true;
        string label = string.IsNullOrWhiteSpace(name)
            ? $"0x{objectId:X8}"
            : name;
        Status = MathF.Abs(delta) > NavigationController.HeadingToleranceDegrees
            ? $"Turning to {label} ({delta:+0.0;-0.0}°)"
            : $"Approaching {label} ({distance:0.0}m)";
        return true;
    }

    private bool ReadyForBreakableTurn(
        in PluginSpellInfo spell,
        uint targetObjectId)
    {
        if (!_settings.UseBreakableTurnTo
            || !spell.RequiresTurnTo
            || targetObjectId == 0u
            || targetObjectId == _host.Automation.Character.ObjectId)
        {
            StopBreakableTurnMovement();
            return true;
        }

        if (!DriveBreakableTurn(targetObjectId))
        {
            StopBreakableTurnMovement();
            return true;
        }
        HoldPassForTurn(targetObjectId);
        return false;
    }

    /// <summary>
    /// The unconditional turn the fists arm makes. It is not the breakable
    /// turn and does not read that option: the character faces the monster,
    /// a shade to one side of dead-on, before the spell goes out.
    /// </summary>
    /// <returns>True once the character is facing where it needs to.</returns>
    private bool FaceForFists(uint targetObjectId)
    {
        const float LeadDegrees = 180f / 50f;
        INavigationAutomation navigation = _host.Automation.Navigation;
        PluginNavigationSnapshot self = navigation.Snapshot;
        if (!self.IsAvailable
            || self.IsPortalSpace
            || !navigation.TryGetObject(
                targetObjectId,
                out PluginNavigationObject target))
        {
            return true;
        }

        float desired = NavigationController.DesiredHeading(
            self.Position,
            target.Position) - LeadDegrees;
        float delta = NavigationController.SignedHeadingDelta(
            self.Position.HeadingDegrees,
            desired);
        if (MathF.Abs(delta) <= BreakableTurnToleranceDegrees)
            return true;

        if (navigation.ClearMovementIntent()
            != PluginNavigationCommandStatus.Accepted)
        {
            return true;
        }
        if (_now - _breakableTurnFaceHeadingStamp
            >= NavigationController.FaceHeadingReissueSeconds)
        {
            _breakableTurnFaceHeadingStamp = _now;
            if (navigation.FaceHeading(desired)
                != PluginNavigationCommandStatus.Accepted)
            {
                return true;
            }
        }
        _breakableTurnOwned = true;
        Status = $"Turning to {_targetName} ({delta:+0.0;-0.0}°)";
        return false;
    }

    /// <summary>
    /// One step of a turn already in flight. Returns true while the character
    /// still has turning left to do.
    /// </summary>
    private bool DriveBreakableTurn(uint targetObjectId)
    {
        INavigationAutomation navigation = _host.Automation.Navigation;
        PluginNavigationSnapshot self = navigation.Snapshot;
        if (!self.IsAvailable
            || self.IsPortalSpace
            || !navigation.TryGetObject(
                targetObjectId,
                out PluginNavigationObject target))
        {
            return false;
        }

        float desired = NavigationController.DesiredHeading(
            self.Position,
            target.Position);
        float delta = NavigationController.SignedHeadingDelta(
            self.Position.HeadingDegrees,
            desired);
        if (MathF.Abs(delta) <= BreakableTurnToleranceDegrees)
            return false;

        if (navigation.ClearMovementIntent()
            != PluginNavigationCommandStatus.Accepted)
        {
            return false;
        }
        if (_now - _breakableTurnFaceHeadingStamp
            >= NavigationController.FaceHeadingReissueSeconds)
        {
            _breakableTurnFaceHeadingStamp = _now;
            if (navigation.FaceHeading(desired)
                != PluginNavigationCommandStatus.Accepted)
            {
                return false;
            }
        }
        _breakableTurnOwned = true;
        Status = $"Turning to {_targetName} ({delta:+0.0;-0.0}°)";
        return true;
    }

    /// <summary>
    /// Turning owns the character, so it owns the rule pass too: nothing else
    /// may claim a turn while the character is still swinging round to face
    /// its target. Raised once for the life of one turn.
    /// </summary>
    private void HoldPassForTurn(uint targetObjectId)
    {
        _breakableTurnTargetId = targetObjectId;
        if (_turnHoldsPass)
            return;
        _turnHoldsPass = true;
        _suspendPass();
    }

    /// <summary>True while a turn to face the target holds the rule pass.</summary>
    internal bool TurnHoldsPass => _turnHoldsPass;

    /// <summary>
    /// Seconds a frame driver has added to the clock since the attack's turn
    /// last ran; the next turn subtracts them, so one wall clock is kept
    /// between the turn and the frame.
    /// </summary>
    private double _frameAdvancedSinceTick;

    private void AdvanceClockFromFrame(double elapsedSeconds)
    {
        double elapsed = Math.Max(0d, elapsedSeconds);
        _now += elapsed;
        _frameAdvancedSinceTick += elapsed;
    }

    /// <summary>The controller's clock, in seconds; a probe for tests.</summary>
    internal double ClockSeconds => _now;

    /// <summary>
    /// A HELD ITEM's cast is a transaction of its own: it began on a turn the
    /// attack owned, it holds the item slot, and it finishes on its own clock
    /// whoever owns the pass meanwhile. While the attack is not being asked
    /// (it lost the turn), the host frame watches that cast to its end here,
    /// including putting the item slot down early. Only the held item is such
    /// a transaction: a weapon proc or a thrown one rides a physical swing,
    /// and losing the turn has just aborted that swing.
    /// </summary>
    internal void ObserveHeldItemCast(double elapsedSeconds)
    {
        if (_heldItemDrivenByTurn)
        {
            _heldItemDrivenByTurn = false;
            return;
        }
        if (_pendingItemDebuff is not
                { Source.Kind: CombatDebuffSourceKind.CasterItem })
        {
            return;
        }
        AdvanceClockFromFrame(elapsedSeconds);
        ObserveItemTransaction();
        TickPendingItemDebuff(_host.Automation.Combat.Snapshot);
    }

    /// <summary>
    /// Steps a turn that is holding the pass. The pass itself is frozen while
    /// the hold is up, so the turn needs a driver outside it — the host frame.
    /// </summary>
    internal void AdvanceHeldTurn(double elapsedSeconds)
    {
        if (!_turnHoldsPass)
            return;
        AdvanceClockFromFrame(elapsedSeconds);
        if (_breakableTurnTargetId == 0u
            || !DriveBreakableTurn(_breakableTurnTargetId))
        {
            StopBreakableTurnMovement();
        }
    }

    private void StopBreakableTurnMovement()
    {
        _breakableTurnFaceHeadingStamp =
            NavigationController.NoFaceHeadingStamp;
        _breakableTurnTargetId = 0u;
        if (_turnHoldsPass)
        {
            _turnHoldsPass = false;
            _resumePass();
        }
        if (!_breakableTurnOwned)
            return;
        _host.Automation.Navigation.ClearMovementIntent();
        _breakableTurnOwned = false;
    }

    /// <summary>
    /// The monster-approach rule losing its turn. The reference writes
    /// Running=false to every loser on every pass, and a navigate rule
    /// told that releases the keys it was holding -- otherwise the walk it
    /// started carries on unsupervised under whichever rule won, and two
    /// movement owners steer at once.
    /// </summary>
    internal void StopMonsterApproachForLostTurn() => StopApproachMovement();

    private void StopApproachMovement()
    {
        _approachFaceHeadingStamp =
            NavigationController.NoFaceHeadingStamp;
        if (!_approachMovementOwned)
            return;
        _ = _host.Automation.Navigation.ClearMovementIntent();
        _approachMovementOwned = false;
    }

    private readonly record struct RuleCandidate(
        PluginCombatTarget Target,
        ResolvedMonsterRule Rule);

    private enum DebuffStartResult
    {
        Skipped,
        Handled,
    }

    private sealed class PendingItemDebuff(
        CombatDebuffSource source,
        uint targetObjectId,
        string targetName,
        string itemName,
        double dispatchedAt,
        long itemCompletionRevision,
        long physicalCompletionRevision,
        PluginCombatMode mode,
        float power)
    {
        public CombatDebuffSource Source { get; } = source;
        public uint TargetObjectId { get; } = targetObjectId;
        public string TargetName { get; } = targetName;
        public string ItemName { get; } = itemName;
        public double DispatchedAt { get; } = dispatchedAt;
        public long ItemCompletionRevision { get; } = itemCompletionRevision;
        public long PhysicalCompletionRevision { get; set; } =
            physicalCompletionRevision;
        public PluginCombatMode Mode { get; } = mode;
        public float Power { get; } = power;
        public double? AttackCompletedAt { get; set; }
        public bool RecoverySent { get; set; }
        public int RecoveryStage { get; set; }
    }

    private void ObserveAttackReceipts(
        PluginCombatSnapshot combat,
        PluginCastCompletion cast)
    {
        if (combat.CompletionRevision > _observedPhysicalCompletion)
        {
            _observedPhysicalCompletion = combat.CompletionRevision;
            // The server says the attack sequence finished. Retail keeps
            // reading result text for two seconds past this point, because
            // the last swing's outcome line can still be in flight.
            _physicalCompletedAt = _now;
            _pendingPhysicalTarget = 0u;
        }

        if (cast.Revision <= _observedAttackCastCompletion)
            return;
        _observedAttackCastCompletion = cast.Revision;

        if (_pendingAttackSpell == cast.SpellId)
        {
            _pendingAttackSpell = 0u;
            _pendingAttackTarget = 0u;
        }
    }

    /// <summary>
    /// The line the reference client prints when it gives a monster up as
    /// unhittable, so a watching player knows why the bot walked away.
    /// </summary>
    private void AnnounceBlacklist(bool tripped, uint objectId, string name)
    {
        if (!tripped)
            return;
        string shown = string.IsNullOrWhiteSpace(name)
            ? FindTarget(objectId).Name
            : name;
        if (string.IsNullOrWhiteSpace(shown))
            shown = "???";
        _host.Automation.Chat.PostSystemMessage(
            "Blacklisting unhittable target "
            + shown
            + " ("
            + objectId.ToString(CultureInfo.InvariantCulture)
            + ") for "
            + ((int)_settings.BlacklistMonsterTimeoutSeconds)
                .ToString(CultureInfo.InvariantCulture)
            + " seconds.");
    }

    /// <summary>How often the stalled-health check runs.</summary>
    private const double GhostSweepIntervalSeconds = 6.271d;

    private double _untilGhostSweep = GhostSweepIntervalSeconds;

    /// <summary>
    /// A monster that has been engaged for a while and whose health has not
    /// moved once in all that time is very likely not there any more: the
    /// server has dropped it and the client is still drawing it. Only a
    /// monster the profile's database gives a health ceiling for can be judged
    /// this way — without a ceiling the client is never told the health in the
    /// first place, so "the health has not moved" says nothing.
    /// </summary>
    private void CheckStalledHealthGhost()
    {
        if (!Enabled
            || !_settings.DeleteGhostMonstersByHealthTracker
            || _health.TargetObjectId == 0u
            || !_settings.MonsterFacts.IsListed(_health.TargetName)
            || _settings.MonsterFacts.MaximumHealth(_health.TargetName) <= 0)
        {
            return;
        }
        double stale = Math.Max(0d, _settings.GhostDeleteHealthTrackerSeconds);
        // Health that has never moved counts as having last moved before the
        // fight started, so the acquisition age alone decides.
        double sinceChange = _health.LastHealthChangeAt is double changed
            ? _now - changed
            : double.PositiveInfinity;
        if (_now - _health.AcquiredAt < stale || sinceChange < stale)
            return;
        DeleteGhostMonster(
            _health.TargetObjectId,
            allowed: true,
            "due to HP tracker notification");
    }

    /// <summary>
    /// Asks the client to forget an object it is still drawing. Nothing else
    /// happens: there is no per-monster "give up on this one" flag, so a
    /// deletion the client refuses leaves the monster exactly as targetable as
    /// it was.
    /// </summary>
    private void DeleteGhostMonster(
        uint objectId,
        bool allowed,
        string reason = "")
    {
        if (objectId == 0u || !allowed)
            return;
        // A ghost is looked up in the whole world, not in the range-limited
        // scan: the monster that stopped answering is often the one that has
        // just dropped out of it, and the health tracker still has its name.
        PluginCombatTarget scanned = FindTarget(objectId);
        string name = scanned.ObjectId == objectId && scanned.Name.Length > 0
            ? scanned.Name
            : _health.TargetObjectId == objectId
                ? _health.TargetName
                : string.Empty;
        if (name.Length == 0)
            return;
        PluginCombatCommandResult result =
            _host.Automation.Combat.DismissGhostTarget(objectId);
        if (!result.Accepted)
            return;
        _host.Automation.Chat.PostSystemMessage(
            string.IsNullOrEmpty(reason)
                ? $"Deleting ghost monster {name} ({objectId})"
                : $"Deleting ghost monster {name} ({objectId}) {reason}.");
        // Losing the awaited target drops the tracker to idle; deleting a
        // ghost is our own version of that event.
        _castTracker.ResetForTarget(objectId);
        if (_targetId == objectId)
        {
            _host.Automation.Combat.AbortPhysicalAttack();
            ClearTarget();
        }
    }
}

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
    private readonly Dictionary<(uint Target, PluginProjectilePathKind Kind), bool>
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
    private uint _pendingAttackTarget;
    private PendingItemDebuff? _pendingItemDebuff;
    private ulong _observedChatSequence;
    private long _observedItemCompletion;
    private bool _combatPolicySuspended;
    private bool _approachMovementOwned;
    private bool _breakableTurnOwned;

    private double _approachFaceHeadingStamp =
        NavigationController.NoFaceHeadingStamp;

    /// <summary>The same stamp for the breakable turn-to.</summary>
    private double _breakableTurnFaceHeadingStamp =
        NavigationController.NoFaceHeadingStamp;

    /// <summary>
    /// VTank's breakable turn-to entry tolerance: the <c>2.0</c> degrees
    /// <c>gj.cs:505</c> passes to <c>w.a(heading, 2.0, 1000.0, true)</c>.
    /// </summary>
    private const float BreakableTurnToleranceDegrees = 2f;
    private Func<string, int, bool>? _requestAmmunitionCraft;
    private Func<string, int, bool>? _canCraftAmmunition;
    private int _randomDamageIndex;
    private long _observedJiggleCastCompletion;
    private bool _selectionJiggleActive;
    private bool _selectionJigglePreviousPlayer;
    private double _nextSelectionJiggleAt;

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
        _castTracker = castTracker ?? new SpellCastTracker();
        _castTracker.Completed += OnCastTrackerOutcome;
    }

    internal SpellCastTracker CastTracker => _castTracker;

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
    public string Status { get; private set; } = "Combat off";
    public string TargetText => _targetText;
    public string ModeText => _modeText;
    public bool HasTarget => _targetId != 0u;

    internal bool HasPendingItemDebuff => _pendingItemDebuff is not null;

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
                equipment.CaptureOwnedEquipment(),
                weapon,
                element).Kind
                != AmmunitionPlanKind.Satisfied;
        };
        gate.WieldAmmunition = element =>
        {
            IEquipmentAutomation equipment = _host.Automation.Equipment;
            if (!equipment.IsAvailable)
                return false;
            IReadOnlyList<PluginEquipmentItem> items =
                equipment.CaptureOwnedEquipment();
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

        _passElements.Clear();
        _passDebuffSources.Clear();
        _passDebuffSpells.Clear();
        _passDeliverable.Clear();
        _passComponents.Clear();
        _passClearance.Clear();
        _passInvalidTargets.Clear();
        _passClearedActions.Clear();
        _passCandidates.Clear();
        _passCandidateRange = double.NaN;
        _passEquipment = null;

        _lastElapsedSeconds = Math.Max(0d, elapsedSeconds);
        _now += _lastElapsedSeconds;
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
        ObserveAttackReceipts(current, castCompletion);
        if (current.Mode != _lastMode)
        {
            _lastMode = current.Mode;
            _modeText = $"Mode  {current.Mode}";
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
            foreach (uint ghost in _failures.ObserveTargets(
                _targets,
                _now,
                _settings))
            {
                DismissGhost(ghost);
            }
            _debuffs.RetainTargets(
                _targets.Select(static target => target.ObjectId).ToHashSet());
            _untilScan = Math.Max(0.05d, _settings.ScanIntervalSeconds);
            RefreshTarget();
        }

        if (_pendingItemDebuff is not null)
        {
            TickPendingItemDebuff(current);
            return;
        }

        // One monster the character cannot act against must not cost the whole
        // pass. When a decision turns out to be undeliverable, the monster's
        // offending action column is turned off (or the monster is dropped
        // outright) for the rest of THIS pass and the choice is made again
        // from what is left, until something can be carried out or nothing is
        // left to try.
        int budget = _settings.UseProjectileAwareness
            ? Math.Max(1, _settings.MaximumCollisionChecksPerTick)
            : 1;
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

    private AttackPassOutcome RunAttackAttempt()
    {
        StopApproachMovement();

        if (_pets.Tick(
                _host.Automation.Items,
                _host.Automation.Character,
                _targets,
                _settings,
                _now,
                out string petStatus,
                readyToRefillInPeace: ReadyToActInPeace))
        {
            Status = petStatus;
            return AttackPassOutcome.Claimed;
        }

        PluginCombatSnapshot combat = _host.Automation.Combat.Snapshot;
        if (TickDebuffs(combat))
            return AttackPassOutcome.Claimed;

        if (TickEquipment())
            return AttackPassOutcome.Claimed;

        if (!_targetRule.Actions.Attacks && !_targetRule.Actions.UsesStreak)
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
            _host.Automation.Items.CaptureOwnedItems();
        MonsterRuleActions physicalActions = ResolvePhysicalActions(
            _targetRule.Actions,
            FindTarget(_targetId),
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
        float desiredPower = AutoAttackPower.Resolve(
            physicalActions,
            _settings,
            _host.Automation.Character,
            inventory);
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
        MonsterRuleActions actions = ResolveRandomDamage(_targetRule.Actions);
        MonsterDamageType element = ResolveAttackElement(actions, target);

        if (actions.DamageType == MonsterDamageType.Fists
            && _attackCatalog.ResolveTuskerFists() is { } fists
            && IsUsableAttackSpell(target)(fists))
        {
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
            // hi.cs:241-257 — the ordinary attack arm.
            plan = element == MonsterDamageType.DrainAuto
                ? PlanDrain(target, ring: false)
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

    private AttackSpellChoice? PlanRing(
        MonsterDamageType element,
        in PluginCombatTarget target)
    {
        PluginSpellInfo? ring = _attackCatalog.Resolve(
            element,
            VtankCombatSpellType.Ring,
            IsUsableAttackSpell(target));
        if (ring is not { } spell)
            return null;
        return _host.Automation.Magic.EvaluateGate(spell.SpellId)
            is PluginCastGate.Ready or PluginCastGate.Busy
            ? new AttackSpellChoice(
                spell,
                VtankCombatSpellType.Ring,
                element,
                CastWithoutTarget: true)
            : null;
    }

    /// <summary>
    /// <c>hi.cs:266,284</c> — <c>dz.i.c(dz.i.a(element, Streak))</c>, the
    /// quality-walked streak line.
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
    /// <c>hi.cs:274,305</c> — VTank's own warning text, then bolt/arc.
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

    private static bool IsFinishingBlow(
        in PluginCombatTarget target,
        AttackSpellChoice? streak)
    {
        if (streak is not { } choice)
            return false;
        if (!target.IsHealthKnown || target.MaximumHealth <= 0)
            return false;
        int remaining = (int)Math.Round(
            target.HealthFraction * target.MaximumHealth);
        int threshold = choice.Spell.Difficulty / 7;
        return remaining > 0 && remaining < threshold;
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

            // hi.cs:483-488
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
                type = VtankCombatSpellType.Arc;              // hi.cs:489-494
                spell = arc!.Value;
            }
            else if (arc is null)
            {
                type = VtankCombatSpellType.War;              // hi.cs:495-500
                spell = bolt.Value;
            }
            else if (bolt.Value.Quality > arc.Value.Quality)
            {
                type = VtankCombatSpellType.War;              // hi.cs:503-508
                spell = bolt.Value;
            }
            else if (arc.Value.Quality > bolt.Value.Quality)
            {
                type = VtankCombatSpellType.Arc;              // hi.cs:509-514
                spell = arc.Value;
            }
            else
            {
                // hi.cs:515-541 — ONLY reached on an exact quality tie.
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
                    default: // UseArcsMode.No and hi.cs:537's own default arm
                        type = VtankCombatSpellType.War;
                        spell = bolt.Value;
                        break;
                }
            }

            bool arcShape = type == VtankCombatSpellType.Arc;
            if (spell.IsProjectile
                && !ProjectilePathIsClear(
                    _targetId,
                    arcShape
                        ? PluginProjectilePathKind.Arc
                        : PluginProjectilePathKind.Straight,
                    // The ray's height belongs to the SHAPE: an arc is thrown
                    // high, a bolt goes out level. The configured melee height
                    // is for the swing, not for these.
                    arcShape
                        ? PluginAttackHeight.High
                        : PluginAttackHeight.Medium,
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

    private AttackSpellChoice? PlanDrain(
        in PluginCombatTarget target,
        bool ring)
    {
        Func<PluginSpellInfo, bool> usable = IsUsableAttackSpell(target);
        ICharacterInfo character = _host.Automation.Character;
        bool needsHealth = character.MaxHealth != 0u
            && character.CurrentHealth / (double)character.MaxHealth < 0.75d;
        string[] order = needsHealth
            ? ["Drain Health Other", "Martyr's Hecatomb", "Harm Other"]
            : ["Martyr's Hecatomb", "Drain Health Other", "Harm Other"];
        foreach (string family in order)
        {
            if (_attackCatalog.ResolveFamily(family, usable) is not { } spell)
                continue;
            return new AttackSpellChoice(
                spell,
                ring ? VtankCombatSpellType.Ring : VtankCombatSpellType.War,
                MonsterDamageType.DrainAuto,
                CastWithoutTarget: false);
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
        long issueRevision = magic.LastCompletion.Revision;
        bool dispatched = choice.CastWithoutTarget
            ? magic.Cast(choice.Spell.SpellId)
            : magic.Cast(choice.Spell.SpellId, _targetId);
        if (!dispatched)
        {
            if (_failures.RecordSpellDidNotStart(_targetId, _settings))
                DismissGhost(_targetId);
            Status = $"Could not start {choice.Spell.Name}";
            return;
        }

        // gj.cs:543 — VTank's own SpellCast log line.
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
            choice.Spell.Saying);
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

    private int CountNearbyRingTargets()
    {
        int count = 0;
        foreach (PluginCombatTarget target in _targets)
        {
            if (target.Distance > _settings.RingDistance)
                continue;
            ResolvedMonsterRule resolved = _settings.ResolveRule(target);
            if (resolved.Priority >= 0 && resolved.Actions.UsesRing)
                count++;
        }
        return count;
    }

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

        IReadOnlyList<PluginEquipmentItem> items =
            equipment.CaptureOwnedEquipment();
        uint desiredWeapon = ResolveEquipmentObjectId(
            actions.WeaponObjectId,
            actions.WeaponName,
            items);
        if (desiredWeapon == 0u)
        {
            desiredWeapon = SelectAutomaticWeapon(
                items,
                ResolveAttackElement(actions, FindTarget(_targetId)),
                _settings);
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
            _host.Automation.Items.CaptureOwnedItems();
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

        // bv.cs:156-162 — the wielded stack already IS the winning row.
        PluginEquipmentItem currentAmmo = equipmentItems.FirstOrDefault(
            static item => item.CombatUse == 3 && item.IsEquipped);
        if (string.Equals(currentAmmo.Name, option.Name, StringComparison.Ordinal))
            return AmmunitionPlan.Satisfied;

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
        IEquipmentAutomation equipment = _host.Automation.Equipment;
        if (!equipment.IsAvailable)
        {
            return true;
        }

        // hi.cs:579-583. The plan is Magic until an owned item backs it.
        PluginCombatMode wanted = PluginCombatMode.Magic;
        uint plannedWeapon = 0u;
        if (_plannedWeapon != 0u)
        {
            foreach (PluginEquipmentItem item in equipment.CaptureOwnedEquipment())
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
                    : _targetRule.Actions.DamageType))
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

    private static uint SelectAutomaticWeapon(
        IReadOnlyList<PluginEquipmentItem> items,
        MonsterDamageType damageType,
        CombatSettings settings)
    {
        const uint weaponReadyMask = 0x03500000u;
        int rawDamage = RawDamageType(damageType);
        PluginEquipmentItem? best = null;
        foreach (PluginEquipmentItem item in items)
        {
            if (!settings.CombatItemObjectIds.Contains(item.ObjectId)
                && !settings.CombatItemNames.Contains(item.Name))
            {
                continue;
            }
            if ((item.ValidLocations & weaponReadyMask) == 0u
                || rawDamage == 0
                || (item.DamageType & rawDamage) == 0)
            {
                continue;
            }
            if (best is null
                || item.Damage > best.Value.Damage
                || (item.Damage == best.Value.Damage
                    && item.IsEquipped
                    && !best.Value.IsEquipped))
            {
                best = item;
            }
        }
        return best?.ObjectId ?? 0u;
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

    private bool TickDebuffs(PluginCombatSnapshot combat)
    {
        if (_debuffs.HasPending)
        {
            Status = _host.Automation.Magic.IsCasting
                ? $"Casting {_debuffs.PendingName}"
                : $"Waiting for {_debuffs.PendingName}";
            return true;
        }

        RefreshSpellCatalogs();
        IReadOnlyList<PluginInventoryItem> items =
            _host.Automation.Items.CaptureOwnedItems();
        PluginCombatTarget target = FindTarget(_targetId);
        if (target.ObjectId == 0u)
            return false;

        MonsterRuleActions actions = _targetRule.Actions;
        IReadOnlyList<CombatDebuffStep> steps = CombatDebuffChain.Build(
            actions,
            ResolveAttackElement(actions, target),
            ResolveExtraVulnerability(actions, target));

        var suppressed = new List<DebuffIdentity>();
        for (int attempt = 0; attempt <= steps.Count; attempt++)
        {
            if (CombatDebuffChain.Choose(
                    steps,
                    step => !suppressed.Contains(step.Identity)
                        && IsDebuffStepDue(step, in target, items))
                is not { } due)
            {
                return false;
            }
            DebuffPassResult result = TickDebuffStep(
                due,
                actions,
                target,
                combat,
                items);
            if (result == DebuffPassResult.ColumnDisabled)
            {
                suppressed.Add(due.Identity);
                continue;
            }
            return result == DebuffPassResult.Claimed;
        }
        return false;
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
                    PluginAttackHeight.Medium,
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
                autoSelect: attackWeapon == 0u))
        {
            return true;
        }
        Status = Gate.Status;
        return false;
    }

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
        if (_passClearance.TryGetValue((targetObjectId, kind), out bool memo))
        {
            result = new(
                memo
                    ? PluginProjectilePathStatus.Clear
                    : PluginProjectilePathStatus.Blocked);
            return memo;
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
        _passClearance[(targetObjectId, kind)] = result.IsClear;
        return result.IsClear;
    }

    /// <summary>
    /// What the pass already knows about a flight, without testing it. An
    /// untested shape reads as clear.
    /// </summary>
    private bool KnownClear(uint targetObjectId, PluginProjectilePathKind? kind) =>
        kind is not { } shape
        || !_passClearance.TryGetValue((targetObjectId, shape), out bool clear)
        || clear;

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
            IReadOnlyList<PluginEquipmentItem> equipmentItems =
                equipment.CaptureOwnedEquipment();
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

        string text = message.Text ?? string.Empty;
        if (string.Equals(
                text.Trim(),
                CombatResultText.MissileHitEnvironment,
                StringComparison.Ordinal))
        {
            _failures.RecordMiss(_physicalResultTargetId, _now, _settings);
        }
        else if (CombatResultText.IsDamageReport(text))
        {
            _failures.ResetAttempts(_physicalResultTargetId);
        }

        if (!CombatResultText.IsKillingBlow(text, out string slain))
            return;

        // The looting hold goes up on the killing blow itself, before the
        // sentence is matched against our own target's name.
        ArmPostKillNavigationLock();

        if (slain.Length > 0
            && _physicalResultTargetName.Length > 0
            && !slain.Equals(
                _physicalResultTargetName,
                StringComparison.OrdinalIgnoreCase))
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
        foreach (PluginEquipmentItem item in equipment.CaptureOwnedEquipment())
        {
            if (item.IsEquipped && item.Cleaving > 1)
                return true;
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
                // gj.cs:348 — LOCAL speech only; the same test as
                // MossTankPanel.ObserveCastTrackerChat (finding R4S-12).
                ownSpeech: message.Kind == SpellCastTracker.LocalSpeechChatKind
                    && message.SenderObjectId != 0u
                    && message.SenderObjectId
                        == _host.Automation.Character.ObjectId);
            if (_pendingItemDebuff is not { } pending
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
        switch (info.Outcome)
        {
            case SpellCastOutcome.Kill:
                Log?.Invoke(
                    MacroLogChannel.CastInfo,
                    $"SpellCaster: Spell kill reset ({info.Text})");
                ArmPostKillNavigationLock();
                if (objectId == 0u)
                    return;
                _failures.ResetAttempts(objectId);
                EndKilledTarget(objectId);
                return;

            case SpellCastOutcome.PermanentFail:
                // The `!HitsMultipleTargets` gate (gj.cs:403) is applied by the
                // tracker, which owns `m_g`; reaching here means it passed.
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
                if (objectId != 0u)
                    _failures.ResetAttempts(objectId);
                return;

            case SpellCastOutcome.ResultTimeout:
                Log?.Invoke(
                    MacroLogChannel.CastInfo,
                    "SpellCaster: Cast result timeout");
                if (objectId != 0u && !info.HitsMultipleTargets)
                    _failures.RecordMiss(objectId, _now, _settings);
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
        _failures.MarkDead(objectId);
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
    }

    private void TickSelectionJiggle()
    {
        if (!_selectionJiggleActive || _now < _nextSelectionJiggleAt)
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
            _host.Automation.Items.CaptureOwnedItems();
        IReadOnlyList<PluginEquipmentItem> equipment =
            _host.Automation.Equipment.IsAvailable
                ? _host.Automation.Equipment.CaptureOwnedEquipment()
                : Array.Empty<PluginEquipmentItem>();
        (uint wieldedWeapon, uint wieldedOffhand) = WieldedPair(equipment);

        uint lastTarget = _targetId;
        var candidates = new List<CombatTargetCandidate>();
        foreach (PluginCombatTarget target in _targets)
        {
            if (TryBuildCandidate(
                    target,
                    combat,
                    lastTarget,
                    inventory,
                    equipment,
                    _acquisitionRange,
                    out CombatTargetCandidate candidate))
            {
                candidates.Add(candidate);
            }
        }

        CombatTargetCandidate chosen = CombatTargetSelector.Select(
            candidates,
            _settings.DebuffEachFirst,
            _settings.SelectionMethod,
            _settings.TargetSelectAngleRange,
            wieldedWeapon,
            wieldedOffhand);

        // dz.cs:925 — `if (this.a.b != 0) return true;`
        if (chosen.ObjectId == 0u)
        {
            if (_targetId != 0u)
                _host.Automation.Combat.AbortPhysicalAttack();
            ClearTarget();
            return;
        }
        SetTarget(chosen.Target, chosen.Rule);
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
    /// <c>f7.a(fu, maxDist, minDist, targetLock)</c> (<c>f7.cs:247-297</c>) —
    /// the six ordered rejection gates, then the fill.
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

        // Gate 3 (f7.cs:265-270).
        ResolvedMonsterRule rule = _settings.ResolveRule(target);
        if (_passClearedActions.TryGetValue(
                target.ObjectId,
                out MonsterActionFlags cleared))
        {
            rule = rule with
            {
                Rule = rule.Rule.WithActions(
                    rule.Actions with { Flags = rule.Actions.Flags & ~cleared }),
            };
        }
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
        uint weapon = ResolveEquipmentObjectId(
            actions.WeaponObjectId,
            actions.WeaponName,
            equipment);
        if (weapon == 0u)
            weapon = SelectAutomaticWeapon(equipment, element, _settings);
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
        MonsterDamageType requested = AttackSpellCatalog.ResolveMagicDamageMode(
            actions.DamageType,
            _host.Automation.Character);
        if (requested != MonsterDamageType.Auto)
            return requested;

        if (RuleWeaponElement(actions) is { } weaponElement)
            return weaponElement;

        IReadOnlyList<MonsterDamageType> preferences =
            _gameInfo.DamagePreferences(target.Name);
        foreach (MonsterDamageType preference in preferences)
        {
            if (preference != MonsterDamageType.None
                && CanDeliverElement(preference, target))
            {
                return preference;
            }
        }

        foreach (MonsterDamageType element in VtankDamageDatabase.UnlistedElementOrder)
        {
            if (Contains(preferences, element) || !CanDeliverElement(element, target))
                continue;
            PostAttackWarning(
                "Warning: no ammunition available for any of target's possible "
                + "damage types! Using unlisted damage type: "
                + ElementName(element));
            return element;
        }
        PostAttackWarning("Warning: no ammunition available!!!");
        return MonsterDamageType.None;
    }

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

    private bool CanDeliverElement(
        MonsterDamageType element,
        in PluginCombatTarget target)
    {
        (MonsterDamageType, uint) key = (element, target.ObjectId);
        if (_passDeliverable.TryGetValue(key, out bool cached))
            return cached;
        bool deliverable = CanDeliverElementCore(element, target);
        _passDeliverable[key] = deliverable;
        return deliverable;
    }

    private bool CanDeliverElementCore(
        MonsterDamageType element,
        in PluginCombatTarget target)
    {
        if (PassEquipment() is { Count: > 0 } owned
            && SelectAutomaticWeapon(owned, element, _settings) != 0u)
        {
            return true;
        }
        RefreshSpellCatalogs();
        Func<PluginSpellInfo, bool> usable = IsUsableAttackSpell(target);
        return _attackCatalog.Resolve(element, VtankCombatSpellType.War, usable)
                is not null
            || _attackCatalog.Resolve(element, VtankCombatSpellType.Arc, usable)
                is not null;
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
        _failures.BeginEngagement(_targetId, _now);
    }

    private void ClearTarget()
    {
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
        _observedPhysicalCompletion = 0;
        _observedAttackCastCompletion = 0;
        _pendingPhysicalTarget = 0u;
        _pendingAttackSpell = 0u;
        _pendingAttackTarget = 0u;
        ClearPendingItemDebuff();
        _observedChatSequence = 0u;
        _observedItemCompletion = 0;
        // gj.cs:271 — d(), the tracker's own reset. A stopped macro must not
        // leave the busy latch up.
        _castTracker.Reset();
        _plannedWeapon = 0u;
        Gate.Reset();
        _randomDamageIndex = 0;
        _observedJiggleCastCompletion = 0;
        ClearTarget();
        Status = status;
    }

    /// <summary>
    /// Walking to a monster the character cannot yet hit. This is its OWN job,
    /// twenty positions below the attack, with its own candidate pick at the
    /// approach range: the attack must not claim the pass for a monster it
    /// would have to walk to, or nothing below the attack ever runs.
    /// </summary>
    /// <returns>True while there is a monster worth walking to.</returns>
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

        _now += Math.Max(0d, elapsedSeconds);
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
            _host.Automation.Items.CaptureOwnedItems();
        IReadOnlyList<PluginEquipmentItem> equipment =
            _host.Automation.Equipment.IsAvailable
                ? _host.Automation.Equipment.CaptureOwnedEquipment()
                : Array.Empty<PluginEquipmentItem>();
        (uint wieldedWeapon, uint wieldedOffhand) = WieldedPair(equipment);

        var candidates = new List<CombatTargetCandidate>();
        foreach (PluginCombatTarget target in reachable)
        {
            if (TryBuildCandidate(
                    target,
                    combat,
                    _targetId,
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
                _now,
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

    /// <summary>
    /// Steps a turn that is holding the pass. The pass itself is frozen while
    /// the hold is up, so the turn needs a driver outside it — the host frame.
    /// </summary>
    internal void AdvanceHeldTurn(double elapsedSeconds)
    {
        if (!_turnHoldsPass)
            return;
        _now += Math.Max(0d, elapsedSeconds);
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

    private void DismissGhost(uint objectId)
    {
        PluginCombatCommandResult result =
            _host.Automation.Combat.DismissGhostTarget(objectId);
        string suffix = result.Accepted ? "deleted" : "ignored";
        _host.Automation.Chat.PostSystemMessage(
            $"[MossTank] Ghost target 0x{objectId:X8} {suffix}.");
        // gj.cs:263-278 — ReleaseObject on the awaited target drops the
        // tracker to idle; deleting a ghost is our own version of that event.
        _castTracker.ResetForTarget(objectId);
        if (_targetId == objectId)
        {
            _host.Automation.Combat.AbortPhysicalAttack();
            ClearTarget();
        }
    }
}

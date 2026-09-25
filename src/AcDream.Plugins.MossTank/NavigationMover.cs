using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The close-in mover: the piece that turns "the goal is over there" into held
/// keys, and the clock those keys are paced against. It is deliberately NOT the
/// rule and NOT the goal — the reference builds one of these per navigation
/// rule and hands each its own goal, so the route, the corpse walk and (later)
/// the monster walk all steer the same way while owning their own armed state,
/// their own re-face throttle and their own idea of what they are walking at.
/// <para>
/// Sharing one instance between two rules would be wrong rather than thrifty:
/// the rule that loses a pass tears its mover down, so two rules on one mover
/// means the loser's teardown disarms the winner.
/// </para>
/// </summary>
internal sealed class NavigationMover
{
    internal const float HeadingToleranceDegrees = 4f;

    /// <summary>
    /// How often the mover may steer once the rule has armed it. The mover is
    /// not the rule: the rule's turn only arms it, and it then runs on the
    /// client's own frame, no faster than this. That distinction is what makes
    /// the alignment band above reachable — one rule pass of held turn is
    /// tens of degrees, several times the band, so a mover stepped once per
    /// pass overshoots on every turn and hunts around the bearing forever.
    /// </summary>
    internal const double MoverIntervalSeconds = 0.047d;

    /// <summary>
    /// The character's walking pace in metres per second; the mover walks
    /// inside the creep band.
    /// </summary>
    internal const double WalkSpeedMetersPerSecond = 3.12d;

    /// <summary>
    /// The narrowest arrival radius the mover can honour: the ground one
    /// steering step covers at a walk, about 0.15 m. The mover only sees
    /// where the character is once a step, so a circle crossed in less than
    /// a step can be walked straight through between two looks; a radius of
    /// a whole step (a diameter of two) is caught even on a late frame.
    /// </summary>
    internal const double MinimumArrivalRadiusMeters =
        WalkSpeedMetersPerSecond * MoverIntervalSeconds;

    internal const double FaceHeadingReissueSeconds = 0.7d;

    /// <summary>
    /// How long the mover may hold the forward key without the character
    /// covering <see cref="StuckProgressMeters"/> before it reports itself
    /// stuck. Longer than any turn in place, shorter than a player's patience
    /// at a wall. The clock runs only while a forward key is held: a fight,
    /// a corpse walk or a lost pass drops the key and the clock with it.
    /// </summary>
    internal const double StuckSeconds = 3d;

    /// <summary>Ground the character must cover inside <see cref="StuckSeconds"/> to count as moving.</summary>
    internal const double StuckProgressMeters = 0.75d;

    internal const double NoFaceHeadingStamp = double.NegativeInfinity;

    /// <summary>The near/far split the heading relaxation switches on.</summary>
    private const double NearTargetMeters = 3d;

    /// <summary>
    /// Inside this the mover walks instead of running. A waypoint whose arrival
    /// radius is wider than this is never approached at a walk, which is what
    /// the low-minimum-distance warning is about.
    /// </summary>
    internal const double CreepDistanceMeters = 240d / 160d;

    private const float FarHeadingRelaxationDegrees = 45f;
    private const float NearHeadingRelaxationDegrees = 15f;

    private readonly IPluginHost _host;
    private CombatModeGate? _combatModeGate;
    private CombatSettings? _combatSettings;
    private bool _hadMovementIntent;
    private bool _armed;
    private double _pendingSeconds;
    private double _now;
    private double _faceHeadingStamp = NoFaceHeadingStamp;
    private bool _forwardHeld;
    private bool _stuckAnchored;
    private PluginNavigationPosition _stuckAnchor;
    private double _stuckAnchorAt;

    internal NavigationMover(IPluginHost host) =>
        _host = host ?? throw new ArgumentNullException(nameof(host));

    /// <summary>
    /// Where the owner puts what the mover has to say — the one status line the
    /// creep push writes, and nothing else.
    /// </summary>
    internal Action<string>? Status { get; set; }

    /// <summary>
    /// Said once per run when a goal this tight is being walked at in peace
    /// mode. The owner keeps the once-per-run bookkeeping; the mover only says
    /// when the case arises.
    /// </summary>
    internal Action? WarnLowStopDistance { get; set; }

    /// <summary>The mover's own clock, in seconds since the plugin started.</summary>
    internal double Now => _now;

    /// <summary>Whether the last rule pass claimed the turn.</summary>
    internal bool IsArmed => _armed;

    /// <summary>Whether the character is currently held moving by this mover.</summary>
    internal bool HasMovementIntent => _hadMovementIntent;

    internal void BindCombatModeGate(CombatModeGate gate, CombatSettings settings)
    {
        _combatModeGate = gate ?? throw new ArgumentNullException(nameof(gate));
        _combatSettings = settings ?? throw new ArgumentNullException(nameof(settings));
    }

    /// <summary>
    /// Moves the clock on by one frame. This is the ONE place it moves: the
    /// clock stands for wall time, and a turn or a pass is not a unit of it. It
    /// used to be advanced from both the route's turn and the door's, so on the
    /// ordinary pass where both were consulted every interval measured against
    /// it — the re-face throttles — ran at roughly double speed, and at an
    /// uneven rate besides.
    /// </summary>
    internal void AdvanceClock(double elapsedSeconds)
    {
        if (!double.IsFinite(elapsedSeconds) || elapsedSeconds <= 0d)
            return;
        _now += elapsedSeconds;
    }

    /// <summary>Arms or disarms the mover on the rule pass's own answer.</summary>
    internal void Arm(bool claimed) => _armed = claimed;

    internal double TakePendingSeconds()
    {
        double due = _pendingSeconds;
        _pendingSeconds = 0d;
        return due;
    }

    /// <summary>
    /// One host frame of the armed mover's bookkeeping. It answers whether this
    /// frame is a steering frame, and how much time that steer is for. What the
    /// mover makes of a frame never disarms it: only the rule that armed it can
    /// take the turn back, which is the whole point of the two being separate.
    /// </summary>
    internal bool TryTakeMoverFrame(double elapsedSeconds, out double dueSeconds)
    {
        dueSeconds = 0d;
        double elapsed = double.IsFinite(elapsedSeconds) && elapsedSeconds > 0d
            ? elapsedSeconds
            : 0d;
        AdvanceClock(elapsed);
        if (!_armed)
        {
            // A disarmed mover holds no time: the pass that arms it starts
            // from this frame, not from however long it was idle.
            _pendingSeconds = elapsed;
            return false;
        }
        _pendingSeconds += elapsed;
        if (_pendingSeconds < MoverIntervalSeconds)
            return false;
        dueSeconds = TakePendingSeconds();
        return true;
    }

    /// <summary>
    /// The rule's teardown on the losing-the-turn edge: the reference client
    /// releases the held movement keys, and ours is the same thing in acdream's
    /// terms — drop the movement intent.
    /// </summary>
    internal void StopForLostTurn()
    {
        _armed = false;
        _pendingSeconds = 0d;
        StopMovement();
    }

    /// <summary>Forgets the re-face throttle, so the next departure re-faces at once.</summary>
    internal void ClearFaceHeadingStamp() => _faceHeadingStamp = NoFaceHeadingStamp;

    /// <summary>
    /// The straight walk has held the forward key for <see cref="StuckSeconds"/>
    /// without covering ground. Stays set until <see cref="ResetStuckClock"/>,
    /// so the rule that reads it decides what to do exactly once.
    /// </summary>
    internal bool IsStuck { get; private set; }

    /// <summary>Forgets the stuck clock's anchor and its verdict; the next held frame starts it over.</summary>
    internal void ResetStuckClock()
    {
        _stuckAnchored = false;
        IsStuck = false;
    }

    /// <summary>
    /// One steering frame's worth of the stuck clock. Anchored where the
    /// character stood when the forward key was first held, moved every time
    /// it covers the progress distance, and read against the mover's clock.
    /// </summary>
    private void TrackProgress(in PluginNavigationPosition current)
    {
        if (!_forwardHeld)
        {
            _stuckAnchored = false;
            return;
        }
        if (!_stuckAnchored
            || current.HorizontalDistanceMeters(_stuckAnchor) >= StuckProgressMeters)
        {
            _stuckAnchored = true;
            _stuckAnchor = current;
            _stuckAnchorAt = _now;
            return;
        }
        if (_now - _stuckAnchorAt >= StuckSeconds)
            IsStuck = true;
    }

    /// <summary>
    /// Records that something other than the mover's own steer set or cleared
    /// the movement intent, so a later stop knows whether it has anything to
    /// clear. The jump waypoint is the one caller.
    /// </summary>
    internal void NoteMovementIntent(bool held) => _hadMovementIntent = held;

    internal void StopMovement()
    {
        _forwardHeld = false;
        ResetStuckClock();
        if (!_hadMovementIntent)
            return;
        _ = _host.Automation.Navigation.ClearMovementIntent();
        _hadMovementIntent = false;
    }

    /// <summary>
    /// Steers at a goal. Outside the alignment band the mover holds a turn key
    /// and keeps walking, so the character curves onto the bearing; the two
    /// relaxation tiers decide only whether it moves while it turns. The
    /// absolute re-face is the typing branch, where a held key would go into
    /// the chat entry.
    /// </summary>
    internal bool Steer(
        INavigationAutomation navigation,
        in PluginNavigationPosition current,
        in PluginNavigationPosition target,
        double distanceMeters)
    {
        bool claimed = SteerCore(navigation, in current, in target, distanceMeters);
        TrackProgress(in current);
        return claimed;
    }

    private bool SteerCore(
        INavigationAutomation navigation,
        in PluginNavigationPosition current,
        in PluginNavigationPosition target,
        double distanceMeters)
    {
        float desired = NavigationController.DesiredHeading(current, target);
        float delta = NavigationController.SignedHeadingDelta(
            current.HeadingDegrees,
            desired);
        float offset = Math.Abs(delta);

        if (_host.Automation.Chat.IsInputActive)
        {
            // A held turn key would go into the chat entry, so this branch
            // stops and re-faces the goal instead, at most once per re-face
            // interval. Inside the band it makes the SAME stop decision as
            // every other branch — the creep band and the forced magic-mode
            // push are not skipped just because somebody is typing.
            if (offset > HeadingToleranceDegrees)
            {
                bool claimed = ResolveStopDecision(
                    navigation,
                    false,
                    0d,
                    TurnHold.None);
                if (_now - _faceHeadingStamp >= FaceHeadingReissueSeconds)
                {
                    _faceHeadingStamp = _now;
                    _ = navigation.FaceHeading(desired);
                }
                return claimed;
            }
            _faceHeadingStamp = NoFaceHeadingStamp;
            return ResolveStopDecision(
                navigation,
                true,
                distanceMeters,
                TurnHold.None);
        }

        if (offset <= HeadingToleranceDegrees)
            return ResolveStopDecision(navigation, true, distanceMeters, TurnHold.None);

        TurnHold turn = NavigationController.PrefersLeftTurn(
            current.HeadingDegrees,
            desired)
            ? TurnHold.Left
            : TurnHold.Right;
        float relaxation = distanceMeters > NearTargetMeters
            ? FarHeadingRelaxationDegrees
            : NearHeadingRelaxationDegrees;
        return offset > relaxation
            ? ResolveStopDecision(navigation, false, 0d, turn)
            : ResolveStopDecision(navigation, true, distanceMeters, turn);
    }

    /// <summary>Which way the mover holds the turn.</summary>
    private enum TurnHold
    {
        None,
        Left,
        Right,
    }

    /// <summary>
    /// Turns "should I be moving, and how far away is the goal" into the one
    /// movement intent this host takes. Inside the creep band the mover walks
    /// rather than runs, and while it is walking in peace mode it keeps asking
    /// for magic mode: a goal that tight is meant to be stood on, and peace
    /// mode there would leave the character unable to act on arrival.
    /// </summary>
    private bool ResolveStopDecision(
        INavigationAutomation navigation,
        bool shouldMove,
        double distanceMeters,
        TurnHold turn)
    {
        bool creep = shouldMove && distanceMeters < CreepDistanceMeters;
        bool far = shouldMove && distanceMeters >= CreepDistanceMeters;
        // The run key stays on for everything but the creep band, turning in
        // place included: a turn is played at its fast rate only while the
        // run key counts as held, and a route must never turn as slowly as a
        // walk. Only the last creep to a tight goal gives it up.
        bool run = !creep;
        if (creep && !TryPrepareCreepCombatMode())
            creep = false;

        bool forward = creep || far;
        if (!forward && turn == TurnHold.None)
        {
            StopMovement();
            return true;
        }

        _hadMovementIntent = navigation.SetMovementIntent(
            new PluginMovementIntent(
                Forward: forward,
                TurnLeft: turn == TurnHold.Left,
                TurnRight: turn == TurnHold.Right,
                Run: run))
            == PluginNavigationCommandStatus.Accepted;
        _forwardHeld = forward && _hadMovementIntent;
        return _hadMovementIntent;
    }

    /// <summary>
    /// The forced magic-mode push, retried on every tick that wants to creep.
    /// </summary>
    private bool TryPrepareCreepCombatMode()
    {
        if (_combatModeGate is null || _combatSettings is null)
            return true;
        if (_host.Automation.Combat.Snapshot.Mode != PluginCombatMode.Peace)
            return true;

        if (_combatSettings.IdlePeaceMode)
            WarnLowStopDistance?.Invoke();

        if (_combatModeGate.TryPrepare(PluginCombatMode.Magic))
            return true;

        Status?.Invoke("Switching to magic mode at the waypoint.");
        return false;
    }
}

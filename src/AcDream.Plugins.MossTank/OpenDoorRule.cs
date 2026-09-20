namespace AcDream.Plugins.MossTank;

/// <summary>
/// Opening a door in the way, as its own turn.
/// </summary>
/// <remarks>
/// <para>
/// A closed door is a wall, and a wall in front of a bot that is trying to walk
/// somewhere stops everything downstream of it. That is why it is claimed
/// before looting a corpse, before approaching one, and before attacking:
/// whatever those rules want to do, they will want to do it on the other side.
/// The rule sat in the right place in the order already, but it had no body —
/// the door was opened from inside a navigate turn, which for the ordinary
/// non-priority route only comes around after all three of those.
/// </para>
/// <para>
/// The door logic itself stays on the navigation controller, which owns the
/// mover and the door ranges; this class is the turn it takes.
/// </para>
/// </remarks>
internal sealed class OpenDoorRule : IMacroRule
{
    private readonly NavigationController _navigation;
    private readonly Func<bool> _enabled;
    private readonly Func<bool> _isLocked;
    private readonly Action _arm;
    private bool _gateClosed;
    private bool _lockHeld;

    internal OpenDoorRule(
        NavigationController navigation,
        Func<bool> enabled,
        Func<bool>? isLocked = null,
        Action? arm = null)
    {
        _navigation = navigation
            ?? throw new ArgumentNullException(nameof(navigation));
        _enabled = enabled ?? throw new ArgumentNullException(nameof(enabled));
        _isLocked = isLocked ?? (static () => false);
        _arm = arm ?? (static () => { });
    }

    public string Name => "OpenDoor";

    /// <summary>
    /// Losing the turn changes nothing here. The door is not moving the
    /// character — the route rules are — so a door rule that stopped the
    /// mover on its way out would be stopping a turn it never owned: the
    /// pass right after a door finishes is one where the route rule arms
    /// the mover and this rule is marked not running in the same breath.
    /// </summary>
    public bool Running { get; set; }

    /// <summary>
    /// What the door is doing on the pass it wins. It says the same sentences
    /// the decline does, because the door's state is the door's state either
    /// way, and it is the only place that work is visible now that the door
    /// no longer writes over the route's line.
    /// </summary>
    public string? RunningDetail => _navigation.DoorStatus;

    /// <summary>
    /// Why this rule declined, in its own words. It used to fall through to
    /// the route's status line — a waypoint sentence with a live distance in
    /// it — which is not a door reason at all and, because it changed on
    /// every pass, could never be suppressed as a repeat.
    /// </summary>
    public string? DeclineReason => _gateClosed
        ? "the rule's own gate is closed"
        : _lockHeld
            ? "another rule holds a lock this one waits on"
            : _navigation.DoorStatus;

    /// <summary>
    /// Whether something else currently holds a lock this rule must respect.
    /// </summary>
    /// <remarks>
    /// MERGE SEAM. The named action-lock table is being built on the combat
    /// branch; until it lands nobody supplies this and nothing is held. When
    /// it lands the caller passes a reader for the four locks the door rule
    /// waits on — navigation, item use, door opening and the spread-lock
    /// target request — and an <see cref="Arm"/> that takes the ones it holds
    /// while it acts. The exact replacements are recorded in the slice-6
    /// research note.
    /// </remarks>
    internal bool IsLocked => _isLocked();

    /// <summary>
    /// Takes the locks this rule holds while it is opening a door.
    /// </summary>
    /// <remarks>MERGE SEAM — see <see cref="IsLocked"/>.</remarks>
    internal void Arm() => _arm();

    public bool ValidNow(in MacroPassContext context)
    {
        bool gateOpen = _enabled();
        _gateClosed = !gateOpen;
        _lockHeld = gateOpen && IsLocked;
        if (!gateOpen || _lockHeld)
            return false;

        bool allowed = context.CanAct;
        bool claimed = _navigation.TickDoorRule(
            context.ElapsedSeconds,
            allowed);
        if (allowed && claimed)
            Arm();
        return allowed && claimed;
    }
}

namespace AcDream.Plugins.MossTank;

/// <summary>
/// The named cooldown slots the rule list shares. Each one is a
/// "nobody may do this kind of thing until T" marker rather than a queue: the
/// rule that starts a slow operation arms the slot, and every rule whose work
/// would collide with it refuses while the slot is up.
/// </summary>
internal enum ActionLockKind
{
    Navigation,
    ItemUse,
    Salvage,
    CastDrainRing,
    TryingPortal,
    MeleeAttackShot,
    MeleeStopTimeout,
    SpreadLockTargetRequested,
    RandomHelperBuffLock,
    ManaStoneUse,
    RechargeLevelBoostHealth,
    RechargeLevelBoostStamina,
    RechargeLevelBoostMana,
    DoorOpening,
    FellowRecruit,
    WarSpellLockedOut,
    VoidSpellLockedOut,
    BuffCastRecast,
    CorpseOpenAttempt,
}

/// <summary>
/// One shared deadline table for <see cref="ActionLockKind"/>.
/// </summary>
/// <remarks>
/// The clock is the macro's own elapsed-seconds clock, advanced once per host
/// frame, so a lock behaves identically in a test and in a session.
/// </remarks>
internal sealed class ActionLockTable
{
    private readonly Dictionary<ActionLockKind, double> _deadlines = [];
    private double _now;

    /// <summary>Seconds since the table was created.</summary>
    public double Now => _now;

    public void Advance(double elapsedSeconds) =>
        _now += Math.Max(0d, elapsedSeconds);

    /// <summary>
    /// Arms a slot for <paramref name="seconds"/>. An arm never SHORTENS a
    /// lock that is already up: the later of the two deadlines wins, so a
    /// short window cannot cancel a long one that is still running.
    /// </summary>
    public void Arm(ActionLockKind kind, double seconds)
    {
        double deadline = _now + Math.Max(0d, seconds);
        if (_deadlines.TryGetValue(kind, out double existing))
        {
            if (deadline > existing)
                _deadlines[kind] = deadline;
            return;
        }
        _deadlines[kind] = deadline;
    }

    /// <summary>Drops a slot early, before its deadline.</summary>
    public void Release(ActionLockKind kind) => _deadlines.Remove(kind);

    /// <summary>
    /// True while the slot is up. A deadline that falls exactly on the current
    /// instant still counts as locked; an expired one is pruned as it is read.
    /// </summary>
    public bool IsLocked(ActionLockKind kind)
    {
        if (!_deadlines.TryGetValue(kind, out double deadline))
            return false;
        if (deadline >= _now)
            return true;
        _deadlines.Remove(kind);
        return false;
    }

    public void ClearAll() => _deadlines.Clear();
}

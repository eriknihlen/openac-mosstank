using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// What the macro believes about the ONE monster it is currently fighting:
/// when it picked it, when it last saw its health move, how much health is
/// left in points, and how much the last blow took off.
///
/// The server never sends a monster's health in points, only the fraction
/// remaining, so the absolute figure only exists for monsters the profile's
/// database gives a ceiling for. Everything downstream of that — the
/// finishing-move threshold, the drain plan and the stalled-health ghost
/// check — is therefore silent about monsters the database does not list,
/// which is exactly how the reference client behaves without one.
/// </summary>
internal sealed class MonsterHealthTracker
{
    /// <summary>
    /// The estimate before anything has been observed. Distinct from a real
    /// zero, which would mean the monster is dead.
    /// </summary>
    private const int NothingObserved = -1;

    private readonly Func<MonsterFactTable> _facts;
    private readonly Func<uint, bool> _isTracked;
    private int _estimate = NothingObserved;
    private long _observedRevision;

    /// <summary>
    /// The fact table is read afresh on every use: a profile load can swap it
    /// out under a tracker that is already following a monster.
    /// </summary>
    /// <param name="isTracked">
    /// Whether the client still knows the object. A ceiling for a monster the
    /// client has forgotten is a stale number, and it would otherwise drive
    /// the finishing-move threshold and the drain plan.
    /// </param>
    public MonsterHealthTracker(
        Func<MonsterFactTable> facts,
        Func<uint, bool>? isTracked = null)
    {
        _facts = facts ?? throw new ArgumentNullException(nameof(facts));
        _isTracked = isTracked ?? (static _ => true);
    }

    public uint TargetObjectId { get; private set; }

    public string TargetName { get; private set; } = string.Empty;

    /// <summary>When the current monster was picked.</summary>
    public double AcquiredAt { get; private set; }

    /// <summary>
    /// When this monster's health was last seen to move, or null while it
    /// never has been.
    /// </summary>
    public double? LastHealthChangeAt { get; private set; }

    /// <summary>The size of the last blow read out of combat text.</summary>
    public int LastDamage { get; private set; }

    /// <summary>
    /// How much health the monster has left, in points, or zero when there is
    /// no monster or no ceiling to scale the fraction against.
    /// <see cref="int.MaxValue"/> stands for "listed, but with no ceiling" —
    /// the reference client's own marker for a bottomless pool.
    /// </summary>
    public int RemainingHealth
    {
        get
        {
            if (TargetObjectId == 0u || !_isTracked(TargetObjectId))
                return 0;
            if (_estimate != NothingObserved)
                return _estimate;
            return _facts().IsListed(TargetName)
                ? _facts().MaximumHealth(TargetName)
                : 0;
        }
    }

    /// <summary>
    /// Points the current monster to <paramref name="objectId"/>. Re-pointing
    /// it at the monster it already holds changes nothing — that is what makes
    /// the acquisition time meaningful across a long fight.
    /// </summary>
    public void SetTarget(uint objectId, string name, double now)
    {
        if (objectId == TargetObjectId)
            return;
        TargetObjectId = objectId;
        TargetName = name ?? string.Empty;
        _estimate = NothingObserved;
        _observedRevision = 0L;
        LastDamage = 0;
        AcquiredAt = now;
        LastHealthChangeAt = null;
    }

    /// <summary>
    /// A fresh health report for the tracked monster. Only a report about a
    /// monster the database gives a ceiling for counts: without one there is
    /// no way to turn the fraction into points, and the reference client
    /// leaves both the estimate and the last-change time alone.
    /// </summary>
    public void Observe(in PluginCombatTarget target, double now)
    {
        if (TargetObjectId == 0u
            || target.ObjectId != TargetObjectId
            || !target.IsHealthKnown
            || target.HealthRevision == 0L
            || target.HealthRevision == _observedRevision)
        {
            return;
        }
        _observedRevision = target.HealthRevision;
        if (!_facts().IsListed(TargetName))
            return;
        int ceiling = _facts().MaximumHealth(TargetName);
        _estimate = ceiling < 0
            ? int.MaxValue
            : (int)Math.Round(target.HealthFraction * ceiling);
        LastHealthChangeAt = now;
    }

    /// <summary>One blow, read out of combat text, taken off the estimate.</summary>
    public void RecordDamage(uint objectId, int points)
    {
        if (objectId == 0u || objectId != TargetObjectId || points <= 0)
            return;
        LastDamage = points;
        if (_estimate != NothingObserved && _estimate != int.MaxValue)
            _estimate -= points;
    }

    public void Clear(double now) => SetTarget(0u, string.Empty, now);

    public void Reset()
    {
        TargetObjectId = 0u;
        TargetName = string.Empty;
        _estimate = NothingObserved;
        _observedRevision = 0L;
        LastDamage = 0;
        AcquiredAt = 0d;
        LastHealthChangeAt = null;
    }
}

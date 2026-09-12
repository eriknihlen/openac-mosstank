using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal enum CombatSuppressionReason
{
    None,
    Blacklisted,
    Ghost,

    Dead,
}

internal sealed class CombatFailureTracker
{
    private readonly Dictionary<uint, Entry> _entries = [];

    public IReadOnlyList<uint> ObserveTargets(
        IReadOnlyList<PluginCombatTarget> targets,
        double now,
        CombatSettings settings)
    {
        var live = new HashSet<uint>();
        List<uint>? newlyGhosted = null;
        foreach (PluginCombatTarget target in targets)
        {
            live.Add(target.ObjectId);
            if (!_entries.TryGetValue(target.ObjectId, out Entry? entry)
                || entry.Incarnation != target.Incarnation)
            {
                _entries[target.ObjectId] = entry = new Entry
                {
                    Incarnation = target.Incarnation,
                };
            }
            entry.LastSeenAt = now;
            if (target.HealthRevision != 0
                && target.HealthRevision != entry.HealthRevision)
            {
                entry.HealthRevision = target.HealthRevision;
                entry.Attempts = 0;
                entry.SpellStartFailures = 0;
            }

            if (entry.BlacklistedUntil <= now)
                entry.BlacklistedUntil = 0d;

            if (settings.DeleteGhostMonstersByHealthTracker
                && entry.EngagedAt is double engagedAt
                && now - engagedAt
                    >= Math.Max(0d, settings.GhostDeleteHealthTrackerSeconds)
                && target.IsHealthKnown
                && target.SecondsSinceHealthUpdate
                    >= Math.Max(0d, settings.GhostDeleteHealthTrackerSeconds))
            {
                if (!entry.IsGhost)
                {
                    entry.IsGhost = true;
                    (newlyGhosted ??= []).Add(target.ObjectId);
                }
            }
        }

        foreach (uint objectId in _entries.Keys.ToArray())
        {
            Entry entry = _entries[objectId];
            if (!live.Contains(objectId)
                && now - entry.LastSeenAt > Math.Max(
                    300d,
                    settings.BlacklistMonsterTimeoutSeconds))
            {
                _entries.Remove(objectId);
            }
        }
        return newlyGhosted ?? (IReadOnlyList<uint>)Array.Empty<uint>();
    }

    public void BeginEngagement(uint objectId, double now)
    {
        if (objectId == 0u)
            return;
        Entry entry = Get(objectId);
        entry.EngagedAt ??= now;
    }

    public bool RecordSpellDidNotStart(
        uint objectId,
        CombatSettings settings)
    {
        if (objectId == 0u || !settings.DeleteGhostMonsters)
            return false;
        Entry entry = Get(objectId);
        entry.SpellStartFailures++;
        if (entry.SpellStartFailures
            >= Math.Max(1, settings.GhostMonsterSpellAttemptCount))
        {
            if (!entry.IsGhost)
            {
                entry.IsGhost = true;
                return true;
            }
        }
        return false;
    }

    /// <summary>
    /// One recorded attempt that provably did not reach the monster. The count
    /// trips on the attempt AFTER the configured allowance, not on it.
    /// </summary>
    public void RecordMiss(
        uint objectId,
        double now,
        CombatSettings settings)
    {
        ArgumentNullException.ThrowIfNull(settings);
        if (objectId == 0u)
            return;
        Entry entry = Get(objectId);
        entry.Attempts++;
        if (entry.Attempts <= settings.BlacklistMonsterAttemptCount)
            return;
        ExtendBlacklist(entry, now, settings);
        entry.Attempts = 0;
    }

    /// <summary>The monster was reached: the attempt count starts over.</summary>
    public void ResetAttempts(uint objectId)
    {
        if (objectId == 0u || !_entries.TryGetValue(objectId, out Entry? entry))
            return;
        entry.Attempts = 0;
    }

    public void ForceBlacklist(
        uint objectId,
        double now,
        CombatSettings settings)
    {
        ArgumentNullException.ThrowIfNull(settings);
        if (objectId == 0u)
            return;
        Entry entry = Get(objectId);
        ExtendBlacklist(entry, now, settings);
        entry.Attempts = 0;
    }

    /// <summary>
    /// Pushes the deadline out, never in: a later trip under a shortened
    /// timeout must not cut an existing blacklist short.
    /// </summary>
    private static void ExtendBlacklist(
        Entry entry,
        double now,
        CombatSettings settings)
    {
        double deadline = now + Math.Max(
            0d,
            settings.BlacklistMonsterTimeoutSeconds);
        if (deadline > entry.BlacklistedUntil)
            entry.BlacklistedUntil = deadline;
    }

    public void MarkDead(uint objectId)
    {
        if (objectId == 0u)
            return;
        Get(objectId).IsDead = true;
    }

    public CombatSuppressionReason Reason(uint objectId, double now)
    {
        if (!_entries.TryGetValue(objectId, out Entry? entry))
            return CombatSuppressionReason.None;
        if (entry.IsDead)
            return CombatSuppressionReason.Dead;
        if (entry.IsGhost)
            return CombatSuppressionReason.Ghost;
        return entry.BlacklistedUntil > now
            ? CombatSuppressionReason.Blacklisted
            : CombatSuppressionReason.None;
    }

    public void Reset() => _entries.Clear();

    private Entry Get(uint objectId)
    {
        if (!_entries.TryGetValue(objectId, out Entry? entry))
            _entries[objectId] = entry = new Entry();
        return entry;
    }

    private sealed class Entry
    {
        public ushort Incarnation;
        public double LastSeenAt;
        public long HealthRevision;
        public int Attempts;
        public int SpellStartFailures;
        public double? EngagedAt;
        public double BlacklistedUntil;
        public bool IsGhost;
        public bool IsDead;
    }
}

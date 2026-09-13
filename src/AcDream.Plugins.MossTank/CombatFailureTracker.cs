using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal enum CombatSuppressionReason
{
    None,
    Blacklisted,

    Dead,
}

internal sealed class CombatFailureTracker
{
    private readonly Dictionary<uint, Entry> _entries = [];

    public void ObserveTargets(
        IReadOnlyList<PluginCombatTarget> targets,
        double now,
        CombatSettings settings)
    {
        var live = new HashSet<uint>();
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
            // The attempt count is NOT cleared by the monster's health
            // moving. Only our own damage line clears it — a fellow's blow,
            // the monster's own regeneration or a heal are somebody else
            // reaching it, and an unhittable monster in a crowd would never
            // be given up on.
            if (entry.BlacklistedUntil <= now)
                entry.BlacklistedUntil = 0d;
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
    }

    /// <summary>
    /// One more cast sent at a monster that has still not answered. Answers
    /// true on the attempt AFTER the configured allowance, and the count then
    /// starts over whether or not the caller does anything about it — the
    /// ceiling is on consecutive silence, not a permanent verdict.
    /// </summary>
    public bool RecordSpellAttempt(
        uint objectId,
        CombatSettings settings)
    {
        ArgumentNullException.ThrowIfNull(settings);
        if (objectId == 0u)
            return false;
        Entry entry = Get(objectId);
        entry.SpellAttempts++;
        if (entry.SpellAttempts <= settings.GhostMonsterSpellAttemptCount)
            return false;
        entry.SpellAttempts = 0;
        return true;
    }

    /// <summary>The monster answered: the silence count starts over.</summary>
    public void ResetSpellAttempts(uint objectId)
    {
        if (objectId != 0u && _entries.TryGetValue(objectId, out Entry? entry))
            entry.SpellAttempts = 0;
    }

    /// <summary>
    /// One recorded attempt that provably did not reach the monster. The count
    /// trips on the attempt AFTER the configured allowance, not on it. Answers
    /// true on the attempt that tripped it, so the caller can say so.
    /// </summary>
    public bool RecordMiss(
        uint objectId,
        double now,
        CombatSettings settings)
    {
        ArgumentNullException.ThrowIfNull(settings);
        if (objectId == 0u)
            return false;
        Entry entry = Get(objectId);
        entry.Attempts++;
        if (entry.Attempts <= settings.BlacklistMonsterAttemptCount)
            return false;
        ExtendBlacklist(entry, now, settings);
        entry.Attempts = 0;
        return true;
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
        return entry.BlacklistedUntil > now
            ? CombatSuppressionReason.Blacklisted
            : CombatSuppressionReason.None;
    }

    /// <summary>
    /// Has the combat pass ever seen this monster? A monster it has never
    /// looked at is not one it has an opinion about.
    /// </summary>
    public bool IsKnown(uint objectId) => _entries.ContainsKey(objectId);

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
        public int Attempts;
        public int SpellAttempts;
        public double BlacklistedUntil;
        public bool IsDead;
    }
}

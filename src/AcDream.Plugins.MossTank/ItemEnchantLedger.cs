using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal sealed class ItemEnchantLedger
{
    private sealed class Entry
    {
        public double A;
        public double B;
        public int Quality;
        public uint WeenieClassId;
        public string ItemName = string.Empty;
        public DateTimeOffset RecordedAtUtc;
        public DateTimeOffset ExpiresAtUtc;
    }

    /// <summary>
    /// The reference client's three-level table (target, then family, then
    /// spell), flattened to one key because nothing here iterates a level on
    /// its own.
    /// </summary>
    private readonly Dictionary<(uint Item, uint Family, uint SpellId), Entry>
        _entries = [];

    private readonly List<(uint Item, uint Family, uint SpellId)> _scratch = [];

    private readonly TimeProvider _timeProvider;
    private ItemEnchantPersistenceStore? _persistence;
    private ItemEnchantPersistenceScope _scope;
    private DateTimeOffset? _lastUtcObservation;

    public ItemEnchantLedger(TimeProvider? timeProvider = null)
    {
        _timeProvider = timeProvider ?? TimeProvider.System;
    }

    /// <summary>Whether any entry's due stamp is still zeroed by a force.</summary>
    public bool HasForcedEntries
    {
        get
        {
            foreach (Entry entry in _entries.Values)
            {
                if (entry.A < entry.B)
                    return true;
            }
            return false;
        }
    }

    public int Count => _entries.Count;

    /// <summary>
    /// Selects the durable server/character scope and restores its unexpired
    /// item timers into this session's monotonic clock.
    /// </summary>
    public ItemEnchantPersistenceLoadResult BindPersistence(
        IPluginStorage storage,
        string? serverName,
        string? characterName,
        double nowSeconds,
        Action<string>? log = null)
    {
        ArgumentNullException.ThrowIfNull(storage);

        _entries.Clear();
        _scratch.Clear();
        _scope = ItemEnchantPersistenceScope.Create(serverName, characterName);
        _persistence = _scope.IsValid
            ? new ItemEnchantPersistenceStore(storage)
            : null;
        if (_persistence is null)
        {
            return new ItemEnchantPersistenceLoadResult(
                ItemEnchantPersistenceLoadStatus.Unavailable, [], false);
        }

        DateTimeOffset utcNow = _timeProvider.GetUtcNow();
        _lastUtcObservation = utcNow;
        ItemEnchantPersistenceLoadResult result = _persistence.Load(_scope, utcNow);
        foreach (ItemEnchantPersistenceRecord record in result.Records)
        {
            double remaining = (record.ExpiresAtUtc - utcNow).TotalSeconds;
            if (remaining <= 0d)
                continue;
            double expiry = nowSeconds + remaining;
            _entries[(record.ItemObjectId, record.Family, record.SpellId)] =
                new Entry
                {
                    A = expiry,
                    B = expiry,
                    Quality = record.Quality,
                    WeenieClassId = record.WeenieClassId,
                    ItemName = record.ItemName,
                    RecordedAtUtc = record.RecordedAtUtc,
                    ExpiresAtUtc = record.ExpiresAtUtc,
                };
        }

        if (result.RequiresRewrite)
            Persist(utcNow);

        if (result.Status == ItemEnchantPersistenceLoadStatus.ClockRollback)
            log?.Invoke("Stored item spell timers were reset after a clock rollback.");
        else if (result.Status == ItemEnchantPersistenceLoadStatus.Invalid)
            log?.Invoke("Stored item spell timers were invalid and have been reset.");
        else if (_entries.Count > 0)
            log?.Invoke($"Restored {_entries.Count} item spell timer(s).");
        return result;
    }

    public void NoteCast(
        uint itemObjectId,
        uint family,
        uint spellId,
        int quality,
        double durationSeconds,
        double nowSeconds,
        string spellName,
        Action<string>? log = null)
    {
        NoteCastCore(
            itemObjectId,
            0u,
            string.Empty,
            family,
            spellId,
            quality,
            durationSeconds,
            nowSeconds,
            spellName,
            log);
    }

    public void NoteCast(
        in PluginInventoryItem item,
        uint family,
        uint spellId,
        int quality,
        double durationSeconds,
        double nowSeconds,
        string spellName,
        Action<string>? log = null)
    {
        NoteCastCore(
            item.ObjectId,
            item.WeenieClassId,
            item.Name,
            family,
            spellId,
            quality,
            durationSeconds,
            nowSeconds,
            spellName,
            log);
    }

    private void NoteCastCore(
        uint itemObjectId,
        uint weenieClassId,
        string? itemName,
        uint family,
        uint spellId,
        int quality,
        double durationSeconds,
        double nowSeconds,
        string spellName,
        Action<string>? log)
    {
        // A spell with no duration is not tracked at all.
        if (!double.IsFinite(durationSeconds)
            || durationSeconds <= 0d
            || !double.IsFinite(nowSeconds))
            return;

        DateTimeOffset utcNow = ObserveUtcNow();
        double expiry = nowSeconds + durationSeconds;
        if (!double.IsFinite(expiry))
            return;
        DateTimeOffset expiresAtUtc;
        try
        {
            expiresAtUtc = utcNow.AddSeconds(durationSeconds);
        }
        catch (ArgumentOutOfRangeException)
        {
            return;
        }
        (uint, uint, uint) key = (itemObjectId, family, spellId);
        bool hasEntry = _entries.TryGetValue(key, out Entry? entry);
        if (hasEntry
            && weenieClassId != 0u
            && (entry!.WeenieClassId != 0u
                && (entry.WeenieClassId != weenieClassId
                    || !string.Equals(
                        entry.ItemName,
                        itemName,
                        StringComparison.OrdinalIgnoreCase))))
        {
            _entries.Remove(key);
            hasEntry = false;
            entry = null;
        }
        bool identityEnriched = hasEntry
            && weenieClassId != 0u
            && entry!.WeenieClassId == 0u
            && !string.IsNullOrWhiteSpace(itemName);
        if (identityEnriched)
        {
            entry!.WeenieClassId = weenieClassId;
            entry.ItemName = itemName!.Trim();
        }
        if (!hasEntry)
        {
            entry = new Entry { A = double.MinValue, B = double.MinValue };
            _entries[key] = entry;
        }
        Entry current = entry!;

        if (current.B < expiry)
        {
            log?.Invoke(
                $"Cast {spellName} on {itemObjectId} ending at "
                    + $"{expiry:0.###}, OVERRIDDEN");
            current.A = expiry;
            current.B = expiry;
            current.Quality = quality;
            current.WeenieClassId = weenieClassId;
            current.ItemName = itemName?.Trim() ?? string.Empty;
            current.RecordedAtUtc = utcNow;
            current.ExpiresAtUtc = expiresAtUtc;
            Persist(utcNow);
            return;
        }

        log?.Invoke(
            $"Cast {spellName} on {itemObjectId} ending at "
                + $"{expiry:0.###}, newer already present");
        if (identityEnriched)
            Persist(utcNow);
    }

    public double RemainingSeconds(
        uint itemObjectId, uint family, int quality, double nowSeconds)
    {
        double longest = 0d;
        foreach (KeyValuePair<(uint Item, uint Family, uint SpellId), Entry> pair
            in _entries)
        {
            if (pair.Key.Item != itemObjectId || pair.Key.Family != family)
                continue;
            if (pair.Value.Quality < quality)
                continue;
            double remaining = pair.Value.A - nowSeconds;
            if (remaining > longest)
                longest = remaining;
        }
        return longest;
    }

    public double RemainingSeconds(
        in PluginInventoryItem item,
        uint family,
        int quality,
        double nowSeconds)
    {
        if (!MatchesPersistedIdentity(item))
        {
            InvalidateItem(item.ObjectId);
            return 0d;
        }
        return RemainingSeconds(item.ObjectId, family, quality, nowSeconds);
    }

    public void ForceAll(double nowSeconds)
    {
        foreach (Entry entry in _entries.Values)
            entry.A = nowSeconds;
    }

    /// <summary>
    /// The restore a finished or cancelled force pass ends on: every entry's
    /// forced stamp goes back to its real expiry, putting the real remaining
    /// time back.
    /// </summary>
    public void CancelForce()
    {
        foreach (Entry entry in _entries.Values)
            entry.A = entry.B;
    }

    /// <summary>
    /// The sweep that drops an entry once its real expiry is in the past.
    /// Without it the table would only ever grow.
    /// </summary>
    public void Expire(double nowSeconds)
    {
        DateTimeOffset utcNow = ObserveUtcNow();
        _scratch.Clear();
        foreach (KeyValuePair<(uint Item, uint Family, uint SpellId), Entry> pair
            in _entries)
        {
            // Compared against the real expiry, never the forced stamp.
            if (pair.Value.B < nowSeconds)
                _scratch.Add(pair.Key);
        }
        bool changed = _scratch.Count > 0;
        foreach ((uint, uint, uint) key in _scratch)
            _entries.Remove(key);
        _scratch.Clear();
        if (changed)
            Persist(utcNow);
    }

    /// <summary>Invalidates all timers for one item after authoritative removal.</summary>
    public void InvalidateItem(uint itemObjectId)
    {
        if (itemObjectId == 0u)
            return;
        RemoveWhere(key => key.Item == itemObjectId);
    }

    /// <summary>Invalidates one item's enchantment family after authoritative removal.</summary>
    public void Invalidate(uint itemObjectId, uint family)
    {
        if (itemObjectId == 0u || family == 0u)
            return;
        RemoveWhere(key => key.Item == itemObjectId && key.Family == family);
    }

    /// <summary>
    /// Clears both memory and the current durable scope after death, dispel, or
    /// another authoritative event that strips item enchantments.
    /// </summary>
    public void InvalidateAll()
    {
        _entries.Clear();
        _scratch.Clear();
        _persistence?.Delete(_scope);
    }

    public void Reset()
    {
        _entries.Clear();
        _scratch.Clear();
        _persistence = null;
        _scope = default;
        _lastUtcObservation = null;
    }

    private void RemoveWhere(
        Func<(uint Item, uint Family, uint SpellId), bool> predicate)
    {
        _scratch.Clear();
        foreach ((uint Item, uint Family, uint SpellId) key in _entries.Keys)
        {
            if (predicate(key))
                _scratch.Add(key);
        }
        bool changed = _scratch.Count > 0;
        foreach ((uint, uint, uint) key in _scratch)
            _entries.Remove(key);
        _scratch.Clear();
        if (changed)
            Persist(ObserveUtcNow());
    }

    private DateTimeOffset ObserveUtcNow()
    {
        DateTimeOffset utcNow = _timeProvider.GetUtcNow();
        if (_lastUtcObservation is { } previous && utcNow < previous)
        {
            _entries.Clear();
            _scratch.Clear();
            _persistence?.Delete(_scope);
        }
        _lastUtcObservation = utcNow;
        return utcNow;
    }

    private void Persist(DateTimeOffset utcNow)
    {
        if (_persistence is null || !_scope.IsValid)
            return;

        var records = new List<ItemEnchantPersistenceRecord>(_entries.Count);
        foreach (KeyValuePair<(uint Item, uint Family, uint SpellId), Entry> pair
            in _entries)
        {
            Entry entry = pair.Value;
            // Character-target rows and older callers have no durable item
            // identity. They remain useful for this session but are not saved.
            if (entry.WeenieClassId == 0u || entry.ItemName.Length == 0)
                continue;
            records.Add(new ItemEnchantPersistenceRecord(
                pair.Key.Item,
                entry.WeenieClassId,
                entry.ItemName,
                pair.Key.Family,
                pair.Key.SpellId,
                entry.Quality,
                entry.RecordedAtUtc,
                entry.ExpiresAtUtc));
        }
        try
        {
            _persistence.Save(_scope, records, utcNow);
        }
        catch (Exception)
        {
            // Persistence is advisory; a storage failure cannot stop macroing.
        }
    }

    private bool MatchesPersistedIdentity(in PluginInventoryItem item)
    {
        foreach (KeyValuePair<(uint Item, uint Family, uint SpellId), Entry> pair
            in _entries)
        {
            if (pair.Key.Item != item.ObjectId)
                continue;
            Entry entry = pair.Value;
            // Session-only rows intentionally carry no stamp.
            if (entry.WeenieClassId == 0u && entry.ItemName.Length == 0)
                continue;
            if (entry.WeenieClassId != item.WeenieClassId
                || !string.Equals(
                    entry.ItemName,
                    item.Name?.Trim() ?? string.Empty,
                    StringComparison.OrdinalIgnoreCase))
            {
                return false;
            }
        }
        return true;
    }
}

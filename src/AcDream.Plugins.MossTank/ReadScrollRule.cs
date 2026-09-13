using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// Whether a scroll is worth reading. The looter asks with
/// <paramref name="commit"/> set, so it will not queue two scrolls teaching
/// the same spell; the reading rule asks with it clear, because by then the
/// scroll it is looking at is the queued one.
/// </summary>
internal static class ScrollReading
{
    public static bool IsEligible(
        IPluginHost host,
        LootSettings settings,
        in PluginInventoryItem item,
        IReadOnlyDictionary<uint, uint> queued,
        bool commit)
    {
        ArgumentNullException.ThrowIfNull(host);
        ArgumentNullException.ThrowIfNull(settings);
        ArgumentNullException.ThrowIfNull(queued);

        // The object class is the whole test: a scroll is a writable item that
        // carries the spell it teaches, and the surface classifies it as one.
        if (item.ObjectClass != PluginObjectClass.Scroll
            || !settings.ReadUnknownScrolls
            || item.SpellId == 0u)
        {
            return false;
        }
        if (host.Automation.Spells.IsKnown(item.SpellId))
            return false;
        if (!host.Automation.Spells.TryGet(
            item.SpellId,
            out PluginSpellInfo spell))
        {
            return false;
        }
        if (!host.Automation.Character.TryGetSkill(
                spell.School,
                out PluginSkillInfo skill)
            || spell.Difficulty - 15 > skill.Current)
        {
            return false;
        }
        return !commit || !queued.ContainsKey(item.SpellId);
    }
}

/// <summary>
/// Reads the scrolls the looter picked up for reading. Retail does not read a
/// scroll as a continuation of its own pickup: the looter only records
/// "this spell is waiting on that item", and a separate rule in the loot stage
/// does the reading whenever it next wins a pass, which can be well after the
/// fight the scroll came out of.
/// </summary>
internal sealed class ReadScrollController
{
    private const double UseTimeoutSeconds = 4d;

    private readonly IPluginHost _host;
    private readonly LootSettings _settings;
    private readonly LootController _loot;
    private uint _pendingItem;
    private long _pendingRevision;
    private double _pendingAge;

    public ReadScrollController(
        IPluginHost host,
        LootSettings settings,
        LootController loot)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _settings = settings ?? throw new ArgumentNullException(nameof(settings));
        _loot = loot ?? throw new ArgumentNullException(nameof(loot));
    }

    public Action<MacroLogChannel, string>? Log { get; set; }

    public bool Tick(double elapsedSeconds, bool canAct)
    {
        // No looting gate here on purpose: the queue is filled while looting
        // is on, and the scrolls already in it are read whether it stays on
        // or not. The rule this stands in for has no such requirement either.
        IItemAutomation items = _host.Automation.Items;
        if (!_host.Automation.IsAvailable || !items.IsAvailable)
        {
            Clear();
            return false;
        }

        // The queue is the looter's, so anything that clears it — a session
        // reset included — drops an in-flight read with it.
        if (_pendingItem != 0u
            && !_loot.PendingScrollReads.Values.Contains(_pendingItem))
        {
            Clear();
        }
        if (_pendingItem != 0u)
            return ContinueRead(items, elapsedSeconds);
        if (_loot.PendingScrollReads.Count == 0)
            return false;
        if (!canAct || items.IsBusy)
            return false;

        if (Choose(items) is not { } scroll)
            return false;
        PluginItemCommandResult use = items.Use(scroll.ObjectId);
        if (!use.Accepted)
            return use.Status == PluginItemCommandStatus.Busy;
        _pendingItem = scroll.ObjectId;
        _pendingRevision = items.LastCompletion.Revision;
        _pendingAge = 0d;
        Log?.Invoke(
            MacroLogChannel.Loot,
            $"ReadScroll: reading {scroll.Name}");
        return true;
    }

    /// <summary>
    /// Waits out the read in flight. A read that fails or never answers
    /// changes nothing: the scroll is still held, so it is still queued and
    /// comes round again. Only the spell entering the spellbook, or the item
    /// leaving the character's hands, takes it off the queue.
    /// </summary>
    private bool ContinueRead(IItemAutomation items, double elapsedSeconds)
    {
        _pendingAge += Math.Max(0d, elapsedSeconds);
        PluginItemUseCompletion completion = items.LastCompletion;
        if ((completion.Revision <= _pendingRevision
                || completion.SourceObjectId != _pendingItem)
            && _pendingAge < UseTimeoutSeconds)
        {
            return true;
        }
        Clear();
        return true;
    }

    /// <summary>
    /// Drops the queued spells the character has since learned and the queued
    /// scrolls it no longer holds, then takes the first one left that is still
    /// worth reading.
    /// </summary>
    private PluginInventoryItem? Choose(IItemAutomation items)
    {
        _loot.ForgetKnownScrollReads();
        if (_loot.PendingScrollReads.Count == 0)
            return null;
        IReadOnlyList<PluginInventoryItem> owned = items.CaptureOwnedItems();
        _loot.ForgetUnownedScrollReads(owned);
        foreach (uint itemId in _loot.PendingScrollReads.Values)
        {
            foreach (PluginInventoryItem candidate in owned)
            {
                if (candidate.ObjectId != itemId)
                    continue;
                if (ScrollReading.IsEligible(
                    _host,
                    _settings,
                    candidate,
                    _loot.PendingScrollReads,
                    commit: false))
                {
                    return candidate;
                }
                break;
            }
        }
        return null;
    }

    private void Clear()
    {
        _pendingItem = 0u;
        _pendingRevision = 0L;
        _pendingAge = 0d;
    }
}

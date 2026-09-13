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
    private const uint MiscItemType = 0x00000080u;

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

        if (!settings.ReadUnknownScrolls || item.SpellId == 0u)
            return false;
        // Retail's client classifies objects into a "scroll" class of its own
        // that has no counterpart on the wire, so the shape of the item has to
        // stand in for it: see the research note on scroll reading.
        if ((item.ItemType & MiscItemType) == 0u
            || !item.Name.EndsWith(" Scroll", StringComparison.OrdinalIgnoreCase))
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
    private string _pendingName = string.Empty;
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

    public string Status { get; private set; } = "No scrolls to read.";

    public Action<MacroLogChannel, string>? Log { get; set; }

    public bool Tick(double elapsedSeconds, bool canAct)
    {
        IItemAutomation items = _host.Automation.Items;
        if (!_settings.Enabled || !_host.Automation.IsAvailable
            || !items.IsAvailable)
        {
            Clear();
            Status = "No scrolls to read.";
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
        {
            Status = "No scrolls to read.";
            return false;
        }
        if (!canAct || items.IsBusy)
            return false;

        if (Choose(items) is not { } scroll)
        {
            Status = "No scrolls to read.";
            return false;
        }
        PluginItemCommandResult use = items.Use(scroll.ObjectId);
        if (!use.Accepted)
        {
            Status = use.Status == PluginItemCommandStatus.Busy
                ? "Waiting to read a scroll…"
                : $"Could not read {scroll.Name}.";
            return use.Status == PluginItemCommandStatus.Busy;
        }
        _pendingItem = scroll.ObjectId;
        _pendingName = scroll.Name;
        _pendingRevision = items.LastCompletion.Revision;
        _pendingAge = 0d;
        Status = $"Reading {scroll.Name}…";
        Log?.Invoke(
            MacroLogChannel.Loot,
            $"ReadScroll: reading {scroll.Name}");
        return true;
    }

    public void Reset()
    {
        Clear();
        Status = "No scrolls to read.";
    }

    private bool ContinueRead(IItemAutomation items, double elapsedSeconds)
    {
        _pendingAge += Math.Max(0d, elapsedSeconds);
        PluginItemUseCompletion completion = items.LastCompletion;
        if (completion.Revision <= _pendingRevision
            || completion.SourceObjectId != _pendingItem)
        {
            if (_pendingAge < UseTimeoutSeconds)
            {
                Status = $"Reading {_pendingName}…";
                return true;
            }
            Status = $"Read timed out: {_pendingName}.";
        }
        else
        {
            Status = completion.IsSuccess
                ? $"Read {_pendingName}."
                : $"Could not read {_pendingName}.";
        }
        _loot.ForgetScrollRead(_pendingItem);
        Clear();
        return true;
    }

    /// <summary>
    /// Drops the queued spells the character has since learned, then takes the
    /// first queued scroll still in inventory and still worth reading.
    /// </summary>
    private PluginInventoryItem? Choose(IItemAutomation items)
    {
        _loot.ForgetKnownScrollReads();
        if (_loot.PendingScrollReads.Count == 0)
            return null;
        IReadOnlyList<PluginInventoryItem> owned = items.CaptureOwnedItems();
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
        _pendingName = string.Empty;
        _pendingRevision = 0L;
        _pendingAge = 0d;
    }
}

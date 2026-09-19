using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal readonly record struct ItemManaRechargePlan(
    uint ChargeObjectId,
    uint TargetObjectId,
    string ChargeName,
    string TargetName,
    int ChargeCurrentMana,
    int CurrentMana,
    int MaximumMana);

internal static class ItemManaRechargePlanner
{
    private const uint ManaStoneItemType = 0x00080000u;
    private const uint ChargedManaEffect = 0x00000001u;

    public static ItemManaRechargePlan? Plan(
        IReadOnlyList<PluginInventoryItem> inventory,
        ISet<string> consumableNames,
        int thresholdPercent,
        IReadOnlyList<uint>? wieldOrder = null,
        Func<uint, bool>? isChargeReady = null,
        Func<uint, bool>? isTargetReady = null)
    {
        ArgumentNullException.ThrowIfNull(inventory);
        ArgumentNullException.ThrowIfNull(consumableNames);
        int threshold = Math.Clamp(thresholdPercent, 0, 99);
        PluginInventoryItem charge = inventory
            .Where(item => (item.ItemType & ManaStoneItemType) != 0u
                && (item.Effects & ChargedManaEffect) != 0u
                && consumableNames.Contains(item.Name)
                && !item.IsEquipped
                && (isChargeReady?.Invoke(item.ObjectId) ?? true))
            .Where(static item => item.ItemCurrentMana > 0)
            .OrderBy(static item => item.Name, StringComparer.Ordinal)
            .ThenBy(static item => item.ObjectId)
            .FirstOrDefault();
        if (charge.ObjectId == 0u)
            return null;

        IEnumerable<PluginInventoryItem> needsCharge = inventory
            .Where(item => item.IsEquipped
                && item.CombatUse != 3
                && (isTargetReady?.Invoke(item.ObjectId) ?? true)
                && item.ItemMaximumMana > 0
                && 100L * Math.Max(0, item.ItemCurrentMana)
                    / item.ItemMaximumMana < threshold);
        PluginInventoryItem target = wieldOrder is null
            ? needsCharge
                .OrderBy(item => 100d * Math.Max(0, item.ItemCurrentMana)
                    / item.ItemMaximumMana)
                .ThenBy(static item => item.ObjectId)
                .FirstOrDefault()
            // Otherwise the oldest still-queued worn item.
            : needsCharge
                .OrderBy(item =>
                {
                    int position = IndexOf(wieldOrder, item.ObjectId);
                    return position < 0 ? int.MaxValue : position;
                })
                .ThenBy(static item => item.ObjectId)
                .FirstOrDefault();
        return target.ObjectId == 0u
            ? null
            : new ItemManaRechargePlan(
                charge.ObjectId,
                target.ObjectId,
                charge.Name,
                target.Name,
                charge.ItemCurrentMana,
                target.ItemCurrentMana,
                target.ItemMaximumMana);
    }

    private static int IndexOf(IReadOnlyList<uint> order, uint objectId)
    {
        for (int i = 0; i < order.Count; i++)
        {
            if (order[i] == objectId)
                return i;
        }
        return -1;
    }
}

internal sealed class ItemManaRechargeController
{
    private readonly record struct UsedChargeSnapshot(
        int AssessmentVersion,
        int CurrentMana,
        bool ObservedEmpty = false);

    private const uint ManaStoneItemType = 0x00080000u;
    private readonly IPluginHost _host;
    private readonly InventorySettings _settings;
    private readonly CombatSettings _profiles;
    private ItemManaRechargePlan? _pending;
    private int _pendingSourceAssessmentVersion;
    private uint _pendingRecipientObjectId;
    private long _observedCompletion;
    private double _pendingAge;

    /// <summary>
    /// How long a worn item's appraisal is believed, in seconds. Gear spends
    /// its own mana as it is used, so the numbers an appraisal gave go stale
    /// and the item has to be looked at again. The wait is drawn fresh per
    /// item inside this band so a whole kit does not come due on the same
    /// pass and queue up behind the one appraisal the client sends at a time.
    /// </summary>
    private const double WornAppraisalMinimumSeconds = 120d;
    private const double WornAppraisalMaximumSeconds = 360d;

    /// <summary>
    /// How long an unanswered appraisal request holds its item back. The
    /// answer is what normally ends the wait; this only bounds a lost one.
    /// </summary>
    private const double WornAppraisalReplySeconds = 10d;

    private readonly List<uint> _wieldOrder = [];
    private readonly Dictionary<uint, UsedChargeSnapshot> _usedCharges = [];
    private readonly Random _wornAppraisalSpread = new();

    /// <summary>When each worn item's appraisal stops being believed.</summary>
    private readonly Dictionary<uint, double> _wornAppraisedUntil = [];

    /// <summary>
    /// The worn items an appraisal has been asked for and not yet answered:
    /// the stamp the client held when it was asked, which is how a fresh
    /// answer is told from the old one, and when the asking happened.
    /// </summary>
    private readonly Dictionary<uint, (int Version, double AskedAt)>
        _wornAppraisalAsked = [];

    private double _wornClock;

    public ItemManaRechargeController(
        IPluginHost host,
        InventorySettings settings,
        CombatSettings profiles)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _settings = settings ?? throw new ArgumentNullException(nameof(settings));
        _profiles = profiles ?? throw new ArgumentNullException(nameof(profiles));
    }

    public string Status { get; private set; } = "Worn mana ready";

    public bool Tick(bool canAct, double elapsedSeconds = 0d)
    {
        IItemAutomation items = _host.Automation.Items;
        _wornClock += Math.Max(0d, elapsedSeconds);
        foreach (PluginInventoryItem item in items.CaptureOwnedItems())
        {
            if ((item.Effects & 1u) == 0u
                && _usedCharges.TryGetValue(item.ObjectId, out UsedChargeSnapshot used))
                _usedCharges[item.ObjectId] = used with
                {
                    ObservedEmpty = true,
                    AssessmentVersion = AssessmentVersion(item.ObjectId),
                };
        }
        ObserveCompletion(items);
        if (_pending is { } waiting)
        {
            _pendingAge += Math.Max(0d, elapsedSeconds);
            if (_pendingAge >= 15d)
            {
                Status = $"Mana refill unconfirmed; holding {waiting.ChargeName}.";
                _host.Log.Info($"{Status} source=0x{waiting.ChargeObjectId:X8}, recipient=0x{_pendingRecipientObjectId:X8}");
                _pending = null;
                return false;
            }
        }
        if (_pending is not null)
            return true;
        if (!canAct
            || !_settings.RefillWornMana
            || !_host.Automation.IsAvailable
            || !items.IsAvailable
            || items.IsBusy)
        {
            return false;
        }

        IReadOnlyList<PluginInventoryItem> owned = items.CaptureOwnedItems();
        ObserveWieldOrder(owned);
        // Worn gear is looked at before anything is decided about it: what
        // the client holds about an item that has never been appraised, or
        // was appraised long enough ago to have spent mana since, cannot say
        // whether the item needs any.
        ObserveWornAppraisals(owned);
        RequestWornAppraisals(owned);
        ItemManaRechargePlan? plan = ItemManaRechargePlanner.Plan(
            owned,
            _profiles.ConsumableNames,
            _settings.RefillWornManaPercent,
            _wieldOrder,
            ChargeManaKnown,
            TargetManaKnown);
        if (plan is not { } next)
        {
            Status = HasPendingAssessment(owned)
                ? "Waiting for item assessment"
                : "Worn mana ready";
            return false;
        }
        uint recipientObjectId = _host.Automation.Character.ObjectId;
        if (recipientObjectId == 0u)
        {
            Status = "Waiting for player identity";
            return false;
        }
        int sourceAssessmentVersion = AssessmentVersion(next.ChargeObjectId);
        PluginItemCommandResult result = items.Apply(
            next.ChargeObjectId,
            recipientObjectId);
        _host.Log.Info(
            $"Worn mana refill request: source={next.ChargeName} " +
            $"(0x{next.ChargeObjectId:X8}), recipient=player " +
            $"(0x{recipientObjectId:X8}), thresholdItem={next.TargetName} " +
            $"(0x{next.TargetObjectId:X8}), threshold=" +
            $"{Math.Clamp(_settings.RefillWornManaPercent, 0, 99)}%, mana=" +
            $"{next.CurrentMana}/{next.MaximumMana}, result={result.Status}");
        if (!result.Accepted)
        {
            Status = $"Mana refill waiting: {result.Status}";
            return result.Status == PluginItemCommandStatus.Busy;
        }
        _pending = next;
        _pendingAge = 0d;
        _pendingSourceAssessmentVersion = sourceAssessmentVersion;
        _pendingRecipientObjectId = recipientObjectId;
        _usedCharges[next.ChargeObjectId] = new UsedChargeSnapshot(
            sourceAssessmentVersion,
            next.ChargeCurrentMana);
        Status = $"Refilling {next.TargetName} ({next.CurrentMana}/{next.MaximumMana})";
        return true;
    }

    private void ObserveWieldOrder(IReadOnlyList<PluginInventoryItem> owned)
    {
        for (int i = _wieldOrder.Count - 1; i >= 0; i--)
        {
            uint queued = _wieldOrder[i];
            bool stillWorn = false;
            foreach (PluginInventoryItem item in owned)
            {
                if (item.ObjectId == queued && item.IsEquipped)
                {
                    stillWorn = true;
                    break;
                }
            }
            if (!stillWorn)
                _wieldOrder.RemoveAt(i);
        }
        foreach (PluginInventoryItem item in owned)
        {
            if (!item.IsEquipped || item.CombatUse == 3)
                continue;
            if (!_wieldOrder.Contains(item.ObjectId))
                _wieldOrder.Add(item.ObjectId);
        }
    }

    private bool HasPendingAssessment(
        IReadOnlyList<PluginInventoryItem> owned)
    {
        foreach (PluginInventoryItem item in owned)
        {
            bool configuredCharge =
                (item.ItemType & ManaStoneItemType) != 0u
                && _profiles.ConsumableNames.Contains(item.Name)
                && !item.IsEquipped;
            if (configuredCharge
                && !ConfiguredSupplyReadiness.IsAssessed(
                    _host.Automation,
                    item.ObjectId))
            {
                return true;
            }
            if (IsWornTarget(item)
                && !_wornAppraisedUntil.ContainsKey(item.ObjectId))
            {
                return true;
            }
        }
        return false;
    }

    /// <summary>
    /// Gear the refill can top up: something in an equipment slot that is not
    /// the ammunition the character has nocked. Ammunition sits in a slot and
    /// holds no mana worth spending a charge on.
    /// </summary>
    private static bool IsWornTarget(in PluginInventoryItem item) =>
        item.IsEquipped && item.CombatUse != 3;

    /// <summary>
    /// Take in the appraisals that have landed and forget the gear that is no
    /// longer worn. An answer is a stamp newer than the one held when the
    /// question was asked; it buys the item its band of quiet. A question
    /// nobody answered is let go after a while so the item can be asked
    /// about again.
    /// </summary>
    private void ObserveWornAppraisals(IReadOnlyList<PluginInventoryItem> owned)
    {
        foreach (uint tracked in _wornAppraisedUntil.Keys.ToArray())
        {
            if (!IsStillWorn(owned, tracked))
                _wornAppraisedUntil.Remove(tracked);
        }
        foreach (uint asked in _wornAppraisalAsked.Keys.ToArray())
        {
            if (!IsStillWorn(owned, asked))
            {
                _wornAppraisalAsked.Remove(asked);
                continue;
            }
            (int version, double askedAt) = _wornAppraisalAsked[asked];
            int current = AssessmentVersion(asked);
            if (current != 0 && current != version)
            {
                _wornAppraisalAsked.Remove(asked);
                _wornAppraisedUntil[asked] = _wornClock + NextWornAppraisalWait();
            }
            else if (_wornClock - askedAt >= WornAppraisalReplySeconds)
            {
                _wornAppraisalAsked.Remove(asked);
            }
        }
    }

    private static bool IsStillWorn(
        IReadOnlyList<PluginInventoryItem> owned,
        uint objectId)
    {
        foreach (PluginInventoryItem item in owned)
        {
            if (item.ObjectId == objectId)
                return IsWornTarget(item);
        }
        return false;
    }

    private double NextWornAppraisalWait() => _wornAppraisalSpread.Next(
        (int)(WornAppraisalMinimumSeconds * 1000d),
        (int)(WornAppraisalMaximumSeconds * 1000d)) / 1000d;

    /// <summary>
    /// Ask about the worn gear that is due: never appraised, or appraised
    /// long enough ago that its numbers are no longer believed. A question
    /// the client refuses costs nothing and is simply asked again next pass.
    /// </summary>
    private void RequestWornAppraisals(IReadOnlyList<PluginInventoryItem> owned)
    {
        foreach (PluginInventoryItem item in owned)
        {
            if (!IsWornTarget(item)
                || _wornAppraisalAsked.ContainsKey(item.ObjectId))
            {
                continue;
            }
            int version = AssessmentVersion(item.ObjectId);
            bool due = version == 0
                || !_wornAppraisedUntil.TryGetValue(
                    item.ObjectId,
                    out double until)
                || until <= _wornClock;
            if (!due)
                continue;
            PluginItemCommandResult asked =
                _host.Automation.Objects.Identify(item.ObjectId);
            if (asked.Accepted)
                _wornAppraisalAsked[item.ObjectId] = (version, _wornClock);
        }
    }

    private bool ChargeManaKnown(uint objectId)
    {
        if (!ConfiguredSupplyReadiness.TryCaptureProperties(
                _host.Automation,
                objectId,
                out PluginItemProperties properties)
            || !properties.Ints.TryGetValue(107u, out int currentMana))
        {
            return false;
        }
        if (!_usedCharges.TryGetValue(objectId, out UsedChargeSnapshot used))
            return true;

        int assessmentVersion = AssessmentVersion(objectId);
        if (assessmentVersion == 0
            || assessmentVersion == used.AssessmentVersion
            || (!used.ObservedEmpty && currentMana == used.CurrentMana))
        {
            return false;
        }

        _usedCharges.Remove(objectId);
        return true;
    }

    private bool TargetManaKnown(uint objectId) =>
        // Only an appraisal this owner asked for and saw answered counts: it
        // is what dates the mana numbers, and a number nobody can date is
        // not worth spending a charge on.
        _wornAppraisedUntil.ContainsKey(objectId)
        && ConfiguredSupplyReadiness.TryCaptureProperties(
            _host.Automation,
            objectId,
            out PluginItemProperties properties)
        && properties.Ints.ContainsKey(107u)
        && properties.Ints.ContainsKey(108u);

    public void Reset()
    {
        _wieldOrder.Clear();
        _usedCharges.Clear();
        _wornAppraisedUntil.Clear();
        _wornAppraisalAsked.Clear();
        _wornClock = 0d;
        _pending = null;
        _pendingSourceAssessmentVersion = 0;
        _pendingRecipientObjectId = 0u;
        Status = "Worn mana ready";
    }

    private int AssessmentVersion(uint objectId) =>
        _host.Automation.Objects.TryGet(
            objectId,
            out PluginWorldObject item)
            ? item.LastIdTime
            : 0;

    private void ObserveCompletion(IItemAutomation items)
    {
        PluginItemUseCompletion completion = items.LastCompletion;
        if (completion.Revision == 0 || completion.Revision == _observedCompletion)
            return;
        _observedCompletion = completion.Revision;
        if (_pending is not { } pending
            || completion.SourceObjectId != pending.ChargeObjectId
            || completion.TargetObjectId != _pendingRecipientObjectId)
        {
            return;
        }
        _host.Log.Info(
            $"Worn mana refill completion: source={pending.ChargeName} " +
            $"(0x{pending.ChargeObjectId:X8}), recipient=player " +
            $"(expected=0x{_pendingRecipientObjectId:X8}, " +
            $"actual=0x{completion.TargetObjectId:X8}), thresholdItem=" +
            $"{pending.TargetName} (0x{pending.TargetObjectId:X8}), " +
            $"success={completion.IsSuccess}, " +
            $"error=0x{completion.WeenieError:X8}");
        if (completion.IsSuccess)
        {
            PluginItemCommandResult assessment =
                _host.Automation.Objects.Identify(pending.ChargeObjectId);
            _host.Log.Info(
                $"Worn mana refill source assessment: source=" +
                $"{pending.ChargeName} (0x{pending.ChargeObjectId:X8}), " +
                $"priorVersion={_pendingSourceAssessmentVersion}, " +
                $"result={assessment.Status}");
        }
        Status = completion.IsSuccess
            ? $"Refilled {pending.TargetName}"
            : $"Mana refill failed (0x{completion.WeenieError:X})";
        _pending = null;
        _pendingSourceAssessmentVersion = 0;
        _pendingRecipientObjectId = 0u;
    }
}

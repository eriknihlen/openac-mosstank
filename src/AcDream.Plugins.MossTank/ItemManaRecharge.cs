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
    private const uint ChargedManaEffect = 0x00000001u;

    /// <summary>
    /// Pick what to spend on worn gear that has run low. The gear is looked
    /// at first, because a source is only worth finding when something wants
    /// it; <paramref name="sourceMissing"/> comes back true exactly when
    /// something wanted mana and there was nothing to give it.
    /// </summary>
    /// <param name="reportLow">Told about each worn item that has run low.</param>
    public static ItemManaRechargePlan? Plan(
        IReadOnlyList<PluginInventoryItem> inventory,
        IReadOnlyDictionary<string, ConsumableCategory> consumableKinds,
        int thresholdPercent,
        out bool sourceMissing,
        IReadOnlyList<uint>? wieldOrder = null,
        Func<uint, bool>? isChargeReady = null,
        Func<uint, bool>? isTargetReady = null,
        Action<PluginInventoryItem, int, int>? reportLow = null)
    {
        ArgumentNullException.ThrowIfNull(inventory);
        ArgumentNullException.ThrowIfNull(consumableKinds);
        sourceMissing = false;
        int threshold = Math.Clamp(thresholdPercent, 0, 99);

        List<PluginInventoryItem> needsCharge = inventory
            .Where(item => item.IsEquipped
                && item.CombatUse != 3
                && (isTargetReady?.Invoke(item.ObjectId) ?? true)
                && item.ItemMaximumMana > 0
                && 100L * Math.Max(0, item.ItemCurrentMana)
                    / item.ItemMaximumMana < threshold)
            .ToList();
        if (needsCharge.Count == 0)
            return null;
        if (reportLow is not null)
        {
            foreach (PluginInventoryItem low in needsCharge)
            {
                reportLow(
                    low,
                    Math.Max(0, low.ItemCurrentMana),
                    low.ItemMaximumMana);
            }
        }

        // A charged stone the profile knows by name comes first, because it
        // is the reusable one; only when there is none does a one-shot charge
        // get spent. A charge is whatever the profile files under that kind,
        // whatever the client calls the item.
        bool Ready(PluginInventoryItem item) =>
            !item.IsEquipped && (isChargeReady?.Invoke(item.ObjectId) ?? true);
        ConsumableCategory Kind(PluginInventoryItem item) =>
            consumableKinds.TryGetValue(item.Name, out ConsumableCategory kind)
                ? kind
                : ConsumableCategory.Other;
        PluginInventoryItem charge = inventory
            .Where(item => item.ObjectClass == PluginObjectClass.ManaStone
                && Kind(item) == ConsumableCategory.ManaStone
                && (item.Effects & ChargedManaEffect) != 0u
                && Ready(item))
            .OrderBy(static item => item.Name, StringComparer.Ordinal)
            .ThenBy(static item => item.ObjectId)
            .FirstOrDefault();
        if (charge.ObjectId == 0u)
        {
            charge = inventory
                .Where(item => Kind(item) == ConsumableCategory.ManaSource
                    && Ready(item))
                .OrderBy(static item => item.Name, StringComparer.Ordinal)
                .ThenBy(static item => item.ObjectId)
                .FirstOrDefault();
        }
        if (charge.ObjectId == 0u)
        {
            sourceMissing = true;
            return null;
        }

        PluginInventoryItem target = wieldOrder is null
            ? needsCharge
                .OrderBy(item => 100d * Math.Max(0, item.ItemCurrentMana)
                    / item.ItemMaximumMana)
                .ThenBy(static item => item.ObjectId)
                .First()
            // Otherwise the oldest still-queued worn item.
            : needsCharge
                .OrderBy(item =>
                {
                    int position = IndexOf(wieldOrder, item.ObjectId);
                    return position < 0 ? int.MaxValue : position;
                })
                .ThenBy(static item => item.ObjectId)
                .First();
        return new ItemManaRechargePlan(
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

    private readonly IPluginHost _host;
    private readonly InventorySettings _settings;
    private readonly CombatSettings _profiles;
    private ItemManaRechargePlan? _pending;
    private int _pendingSourceAssessmentVersion;
    private uint _pendingRecipientObjectId;
    private long _observedCompletion;
    private double _pendingAge;
    private double _frameObservedSeconds;
    private ActionLockTable _actionLocks = new();

    /// <summary>How long an unanswered use is waited out before it is given up.</summary>
    private const double PendingTimeoutSeconds = 15d;

    /// <summary>
    /// Shares the macro's cooldown table. A charge is in the character's
    /// hands for as long as the macro waits on it, so nothing else -- the
    /// attack included -- acts inside that window.
    /// </summary>
    internal void BindActionLocks(ActionLockTable locks) =>
        _actionLocks = locks ?? throw new ArgumentNullException(nameof(locks));

    /// <summary>
    /// Reads the server's answer to the use this owner issued, on the host
    /// frame rather than on the macro pass. An unanswered use holds the pass,
    /// so the pass cannot be what ends the wait -- it would be waiting on
    /// itself, and the hold could then only end on the watchdog, seconds
    /// after the server had already answered. Nothing is issued here.
    /// </summary>
    internal void ObservePendingReceipt(double elapsedSeconds)
    {
        if (_pending is null || !_host.Automation.IsAvailable)
            return;
        double elapsed = Math.Max(0d, elapsedSeconds);
        _frameObservedSeconds += elapsed;
        ObserveCompletion(_host.Automation.Items);
        AgePending(elapsed);
    }

    /// <summary>
    /// Charge the wait the seconds that have passed, and give it up once it
    /// has waited long enough. True when it was given up.
    /// </summary>
    private bool AgePending(double elapsedSeconds)
    {
        if (_pending is not { } waiting)
            return false;
        _pendingAge += Math.Max(0d, elapsedSeconds);
        if (_pendingAge < PendingTimeoutSeconds)
            return false;
        Status = $"Mana refill unconfirmed; holding {waiting.ChargeName}.";
        _host.Log.Info($"{Status} source=0x{waiting.ChargeObjectId:X8}, recipient=0x{_pendingRecipientObjectId:X8}");
        ClearPending();
        return true;
    }

    private void ClearPending()
    {
        _pending = null;
        _pendingAge = 0d;
        _frameObservedSeconds = 0d;
        _actionLocks.Release(ActionLockKind.ItemUse);
    }

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
    private readonly HashSet<string> _postedWarnings = new(StringComparer.Ordinal);
    private readonly Dictionary<uint, string> _reportedLowItems = [];

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
    private ulong _chatSequence;

    /// <summary>The worn item asked about last, so the next turn is somebody else's.</summary>
    private uint _lastWornAppraisalAsked;

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
        ObserveRefillAnnouncement();
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
        // Whatever the frame driver already watched off this transaction is
        // not counted a second time here: one wall clock between the two.
        double pendingElapsed = Math.Max(
            0d,
            Math.Max(0d, elapsedSeconds) - _frameObservedSeconds);
        _frameObservedSeconds = 0d;
        if (AgePending(pendingElapsed))
            return false;
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
            _profiles.ConsumableCategories.AsReadOnly(),
            _settings.RefillWornManaPercent,
            out bool sourceMissing,
            _wieldOrder,
            ChargeManaKnown,
            TargetManaKnown,
            ReportLowItem);
        if (plan is not { } next)
        {
            if (sourceMissing)
            {
                WarnOnce("Warning: No mana charges/stones available, but "
                    + "equipped items need mana.");
                Status = "No mana charge or stone to spend";
                return false;
            }
            _reportedLowItems.Clear();
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
        _frameObservedSeconds = 0d;
        // The charge is in the character's hands until the server answers
        // for it; every other rule that consumes an item waits that out.
        _actionLocks.Arm(ActionLockKind.ItemUse, ItemUseLock.TransactionSeconds);
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
            bool configuredCharge = !item.IsEquipped
                && _profiles.ConsumableCategories.TryGetValue(
                    item.Name,
                    out ConsumableCategory kind)
                && kind == ConsumableCategory.ManaStone;
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
    /// Ask about one piece of worn gear that is due: never appraised, or
    /// appraised long enough ago that its numbers are no longer believed.
    ///
    /// One question at a time, and the gear takes its turn. The client
    /// answers appraisals one at a time and the looter asks for its own
    /// through the same slot, so a whole kit asked on one pass simply queued
    /// up behind itself; and because a question nobody answers is let go
    /// rather than stamped, asking in list order would let one such item be
    /// asked again for ever while the rest were never asked at all.
    /// </summary>
    private void RequestWornAppraisals(IReadOnlyList<PluginInventoryItem> owned)
    {
        if (_wornAppraisalAsked.Count != 0)
            return;
        var due = new List<uint>();
        foreach (PluginInventoryItem item in owned)
        {
            if (!IsWornTarget(item))
                continue;
            bool isDue = AssessmentVersion(item.ObjectId) == 0
                || !_wornAppraisedUntil.TryGetValue(
                    item.ObjectId,
                    out double until)
                || until <= _wornClock;
            if (isDue)
                due.Add(item.ObjectId);
        }
        if (due.Count == 0)
            return;
        int start = due.IndexOf(_lastWornAppraisalAsked);
        uint next = due[start < 0 ? 0 : (start + 1) % due.Count];
        PluginItemCommandResult asked =
            _host.Automation.Objects.Identify(next);
        if (!asked.Accepted)
            return;
        _wornAppraisalAsked[next] = (AssessmentVersion(next), _wornClock);
        _lastWornAppraisalAsked = next;
    }

    /// <summary>
    /// Say, once per item and per level, that a worn item has run low. The
    /// same item saying the same thing three times a second would bury the
    /// log; it speaks again when its level moves or when it stops being low.
    /// </summary>
    private void ReportLowItem(
        PluginInventoryItem item,
        int currentMana,
        int maximumMana)
    {
        int percent = maximumMana == 0 ? 0 : (int)(100L * currentMana / maximumMana);
        string line = $"Item {item.Name} low on mana, {percent}%, "
            + $"{currentMana}/{maximumMana}";
        if (_reportedLowItems.TryGetValue(item.ObjectId, out string? already)
            && string.Equals(already, line, StringComparison.Ordinal))
        {
            return;
        }
        _reportedLowItems[item.ObjectId] = line;
        _host.Log.Info(line);
    }

    /// <summary>
    /// Said once a run. A character wearing mana-hungry gear with nothing to
    /// feed it would otherwise repeat this every pass for as long as the
    /// macro runs.
    /// </summary>
    private void WarnOnce(string text)
    {
        if (!_postedWarnings.Add(text))
            return;
        _host.Log.Info(text);
        _host.Automation.Chat.PostSystemMessage("[MossTank] " + text);
    }

    public void ResetOncePerRunWarnings() => _postedWarnings.Clear();

    private bool ChargeManaKnown(uint objectId)
    {
        // An item this owner has never spent is ready as it stands. Only one
        // it has just spent has to prove it changed, and that proof is the
        // mana an appraisal reports -- which a one-shot charge does not
        // carry at all, so demanding it up front ruled every charge out.
        if (!_usedCharges.TryGetValue(objectId, out UsedChargeSnapshot used))
            return true;
        if (!ConfiguredSupplyReadiness.TryCaptureProperties(
                _host.Automation,
                objectId,
                out PluginItemProperties properties)
            || !properties.Ints.TryGetValue(107u, out int currentMana))
        {
            return false;
        }

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
        _lastWornAppraisalAsked = 0u;
        _reportedLowItems.Clear();
        _postedWarnings.Clear();
        _wornClock = 0d;
        _chatSequence = 0u;
        ClearPending();
        _pendingSourceAssessmentVersion = 0;
        _pendingRecipientObjectId = 0u;
        Status = "Worn mana ready";
    }

    /// <summary>
    /// The line the server prints when a charge is spent, which names every
    /// item that took mana from it. The server sends no property update for
    /// those items, so this line and the receipt are the only two ways of
    /// knowing that every reading of worn gear is now out of date.
    /// </summary>
    private const string RefillAnnouncement =
        "points of mana to the following items";

    private void ObserveRefillAnnouncement()
    {
        foreach (PluginChatMessage message in
            _host.Automation.Chat.CaptureMessages(_chatSequence))
        {
            _chatSequence = Math.Max(_chatSequence, message.Sequence);
            if (message.Text is { } text
                && text.Contains(RefillAnnouncement, StringComparison.Ordinal))
            {
                ForgetWornReadings();
            }
        }
    }

    /// <summary>
    /// Give up every reading of worn gear. A refill moves mana into items the
    /// server then says nothing more about, so the numbers held for them are
    /// worth no more than a guess: the gear is asked about again, and nothing
    /// is spent on it until fresh answers arrive. Without this the same gear
    /// read low for the next two to six minutes and every charge carried was
    /// poured into kit that was already full.
    /// </summary>
    private void ForgetWornReadings()
    {
        _wornAppraisedUntil.Clear();
        _wornAppraisalAsked.Clear();
        _reportedLowItems.Clear();
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
            ForgetWornReadings();
        }
        Status = completion.IsSuccess
            ? $"Refilled {pending.TargetName}"
            : $"Mana refill failed (0x{completion.WeenieError:X})";
        ClearPending();
        _pendingSourceAssessmentVersion = 0;
        _pendingRecipientObjectId = 0u;
    }
}

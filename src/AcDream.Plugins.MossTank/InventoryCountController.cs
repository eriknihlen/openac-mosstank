using System.Text.RegularExpressions;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>One counted name and how many the count found under it.</summary>
internal readonly record struct InventoryCountEntry(string Name, int Count);

/// <summary>
/// What a finished count found: one entry per distinct item name, one per
/// rule that decided one of those items, and the sum.
/// </summary>
/// <remarks>
/// Both tallies sum stack sizes rather than objects, so the rule lines and
/// the item lines add up to the same total. A name count has no rules and
/// leaves <see cref="ByRule"/> empty.
/// </remarks>
internal sealed record InventoryCountTally(
    IReadOnlyList<InventoryCountEntry> ByName,
    IReadOnlyList<InventoryCountEntry> ByRule,
    int Total)
{
    public static InventoryCountTally Empty { get; } = new([], [], 0);
}

/// <summary>
/// Counts what the character carries and who stands nearby. Three questions,
/// each independent of the others:
/// <list type="bullet">
/// <item>a name pattern over the packs, summed per distinct name;</item>
/// <item>a loot profile over the packs, tallied by rule and by name, which
/// has to wait whenever an item's decision still turns on an appraisal;</item>
/// <item>the players standing within a distance of the character.</item>
/// </list>
/// Only the profile question can take time, and only it ever touches the
/// character.
/// </summary>
internal sealed class InventoryCountController
{
    private const string IdleStatus = "Item counter idle.";

    /// <summary>How often a waiting count says how much is left.</summary>
    private const double ProgressIntervalSeconds = 10d;

    /// <summary>
    /// One appraisal request every 499 ms — the same cadence the corpse
    /// identifier keeps, so the two together never ask faster than one.
    /// </summary>
    private const double IdentifyRequestIntervalSeconds = 0.499d;

    /// <summary>How long one request waits before it counts as unanswered.</summary>
    private const double IdentifyTimeoutSeconds = 10d;

    /// <summary>
    /// How many failed requests one item gets before the count gives up on it
    /// and decides it from what the client already holds. A request fails
    /// when it goes out and nothing answers it, and when the client refuses
    /// to send it at all. Without a ceiling one silent or unaskable item
    /// would hold the whole count for ever.
    /// </summary>
    private const int MaximumIdentifyAttemptsPerItem = 3;

    /// <summary>
    /// The outside limit on one count, three minutes. Every known way for an
    /// item to stall now ends in the attempt ceiling, so reaching this means
    /// something unforeseen is holding the outstanding set; a foreground
    /// count owns the character while it waits, so it has to let go anyway.
    /// A full set of packs asked at the request cadence takes under two
    /// minutes, so an honest count never comes near this.
    /// </summary>
    private const double CountDeadlineSeconds = 180d;

    private static readonly TimeSpan NamePatternTimeout =
        TimeSpan.FromMilliseconds(250d);

    private static readonly PluginItemProperties EmptyProperties = new(
        new Dictionary<uint, int>(),
        new Dictionary<uint, long>(),
        new Dictionary<uint, bool>(),
        new Dictionary<uint, double>(),
        new Dictionary<uint, string>(),
        new Dictionary<uint, uint>(),
        new Dictionary<uint, uint>());

    private readonly IPluginHost _host;
    private readonly MossTankLootProfileStore _profiles;
    private readonly Func<bool> _corpseIdentifierBusy;
    private readonly List<LootRule> _rules = [];
    private readonly Dictionary<uint, CountedItem> _counted = [];
    private readonly HashSet<uint> _outstanding = [];
    private readonly Dictionary<uint, int> _identifyAttempts = [];
    private string _profileName = string.Empty;
    private string? _reportedMissingProfile;
    private bool _foreground;
    private bool _deferred;
    private uint _awaitingObjectId;
    private double _awaitingSeconds;
    private double _sinceRequest;
    private double _sinceProgress;
    private double _sinceStart;

    /// <param name="host">The plugin host every read and request goes through.</param>
    /// <param name="profiles">Where a named loot profile is read from.</param>
    /// <param name="corpseIdentifierBusy">
    /// Asks whether the looter is waiting on the appraisal slot right now.
    /// Read at every decision point rather than kept, because the answer
    /// changes from frame to frame.
    /// </param>
    public InventoryCountController(
        IPluginHost host,
        MossTankLootProfileStore profiles,
        Func<bool> corpseIdentifierBusy)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _profiles = profiles ?? throw new ArgumentNullException(nameof(profiles));
        _corpseIdentifierBusy = corpseIdentifierBusy
            ?? throw new ArgumentNullException(nameof(corpseIdentifierBusy));
    }

    /// <summary>True while a profile count is still waiting on appraisals.</summary>
    public bool IsRunning { get; private set; }

    /// <summary>One line describing what the counter is doing or last did.</summary>
    public string Status { get; private set; } = IdleStatus;

    /// <summary>The answer the last profile count arrived at, or null.</summary>
    public InventoryCountTally? LastProfileTally { get; private set; }

    // ── a name pattern over the packs ─────────────────────────────────────

    /// <summary>
    /// Sums every owned item whose name matches <paramref name="pattern"/>,
    /// per distinct name. Stack sizes count, so one stack of ten tapers is
    /// ten. Worn and wielded gear is included: the question is what the
    /// character has, not what is loose.
    /// </summary>
    /// <param name="pattern">A regular expression matched against the name.</param>
    /// <param name="tally">The counts, empty when the pattern is unusable.</param>
    /// <param name="error">Why the pattern was refused, else empty.</param>
    /// <returns>True when the pattern was usable.</returns>
    public bool TryCountByName(
        string? pattern,
        out InventoryCountTally tally,
        out string error)
    {
        tally = InventoryCountTally.Empty;
        string text = pattern?.Trim() ?? string.Empty;
        if (text.Length == 0)
        {
            error = "no name pattern given.";
            return false;
        }

        Regex matcher;
        try
        {
            matcher = new Regex(
                text,
                RegexOptions.IgnoreCase | RegexOptions.CultureInvariant,
                NamePatternTimeout);
        }
        catch (ArgumentException failure)
        {
            error = $"bad name pattern: {failure.Message}";
            return false;
        }

        var byName = new Dictionary<string, int>(StringComparer.OrdinalIgnoreCase);
        int total = 0;
        foreach (PluginInventoryItem item in
            _host.Automation.Items.CaptureOwnedItems())
        {
            if (!matcher.IsMatch(item.Name))
                continue;
            int size = Math.Max(1, item.StackSize);
            Add(byName, item.Name, size);
            total += size;
        }

        tally = new InventoryCountTally(Sorted(byName), [], total);
        error = string.Empty;
        return true;
    }

    /// <summary>Counts by name and writes the answer to chat.</summary>
    public void ReportNameCount(string? pattern)
    {
        if (!TryCountByName(
            pattern,
            out InventoryCountTally tally,
            out string error))
        {
            Write("Item Count: " + error);
            return;
        }
        ReportTally(tally, pattern?.Trim() ?? string.Empty);
    }

    // ── the players standing nearby ───────────────────────────────────────

    /// <summary>
    /// How many players stand within <paramref name="rangeMeters"/> of the
    /// character, measured flat along the ground — a player one floor up a
    /// tower and a player beside it are the same distance away.
    /// </summary>
    /// <remarks>
    /// The character counts as one of them. The question is asked over the
    /// same set of standing-on-the-landscape objects that the profile this
    /// plugin follows asks it over, and the character's own object is in that
    /// set at a distance of zero, so an empty field answers one and a meta
    /// written against that profile reads the figure it expects. Excluding
    /// the character would answer zero and shift every threshold by one.
    /// </remarks>
    public int CountPlayersWithin(double rangeMeters)
    {
        if (!double.IsFinite(rangeMeters) || rangeMeters <= 0d)
            return 0;
        PluginNavigationSnapshot player = _host.Automation.Navigation.Snapshot;
        if (!player.IsAvailable)
            return 0;
        int count = 0;
        foreach (PluginWorldObject candidate in
            _host.Automation.Objects.CaptureObjects())
        {
            if (candidate.ObjectClass != PluginObjectClass.Player
                || !candidate.IsLandscape
                || !candidate.HasPosition)
            {
                continue;
            }
            if (candidate.Position.HorizontalDistanceMeters(player.Position)
                < rangeMeters)
            {
                count++;
            }
        }
        return count;
    }

    /// <summary>Counts players in range and writes the answer to chat.</summary>
    public void ReportPlayerCount(double rangeMeters) =>
        Write($"Player Count: {CountPlayersWithin(rangeMeters)}");

    // ── a loot profile over the packs ─────────────────────────────────────

    /// <summary>
    /// Begins a profile count. It finishes inside this call when every item
    /// can already be decided; otherwise it stays running until the
    /// outstanding appraisals land.
    /// </summary>
    /// <param name="profileName">The loot profile to count against.</param>
    /// <param name="foreground">
    /// True for a count the player asked for by command: it owns the
    /// character while it waits, and it prints its progress and its answer.
    /// A count a macro asked for does neither — holding the character would
    /// stop the very macro that has to read the answer.
    /// </param>
    /// <returns>True when the count started (or finished) here.</returns>
    public bool TryStartProfile(string? profileName, bool foreground)
    {
        if (IsRunning)
        {
            Status = "Item counter already running.";
            return false;
        }
        string requested = profileName?.Trim() ?? string.Empty;
        _rules.Clear();
        if (!_profiles.TryLoadNamed(requested, _rules))
        {
            Status = $"Item counter profile not found: {requested}.";
            return false;
        }

        _counted.Clear();
        _outstanding.Clear();
        _identifyAttempts.Clear();
        _awaitingObjectId = 0u;
        _awaitingSeconds = 0d;
        // Armed, so the first request can go out on the first frame rather
        // than half a second into the count.
        _sinceRequest = IdentifyRequestIntervalSeconds;
        _sinceProgress = 0d;
        _sinceStart = 0d;
        _deferred = false;
        _reportedMissingProfile = null;
        _profileName = requested;
        _foreground = foreground;
        LastProfileTally = null;
        IsRunning = true;

        Scan();
        if (_outstanding.Count == 0)
        {
            Complete();
            return true;
        }
        Status =
            $"Counting {_profileName}: {_outstanding.Count} item(s) to identify.";
        if (_foreground)
            Write($"Items remaining to identify: {_outstanding.Count}");
        return true;
    }

    /// <summary>
    /// The number an expression reads: the profile's total once every
    /// appraisal has landed, and -1 while there is no answer yet — the count
    /// is still waiting, a count of another profile is in flight, or the
    /// profile could not be read.
    /// </summary>
    public double ProfileCount(string? profileName)
    {
        string requested = profileName?.Trim() ?? string.Empty;
        // A count already under way answers when it finishes, whoever asked
        // for it; asking again neither restarts it nor jumps the queue.
        if (IsRunning)
            return -1d;
        if (!TryStartProfile(requested, foreground: false))
        {
            ReportMissingProfileOnce(requested);
            return -1d;
        }
        return IsRunning || LastProfileTally is null
            ? -1d
            : LastProfileTally.Total;
    }

    /// <summary>
    /// Drives a profile count that is waiting on appraisals: one request at a
    /// time, never while the looter is using the same appraisal slot, and a
    /// progress line every ten seconds.
    /// </summary>
    /// <param name="elapsedSeconds">Seconds since the last frame.</param>
    /// <param name="canAct">
    /// False when another owner holds the character this frame.
    /// </param>
    /// <returns>True while a foreground count owns the character.</returns>
    public bool Tick(double elapsedSeconds, bool canAct)
    {
        if (!IsRunning)
            return false;
        if (!_host.Automation.IsAvailable)
        {
            Stop("Item counter stopped: the session went away.");
            return false;
        }

        double elapsed = Math.Max(0d, elapsedSeconds);
        _sinceRequest += elapsed;
        _sinceProgress += elapsed;
        _sinceStart += elapsed;
        ObserveAppraisal(elapsed);
        Scan();
        if (_outstanding.Count == 0)
        {
            Complete();
            return false;
        }
        if (_sinceStart >= CountDeadlineSeconds)
        {
            GiveUp();
            return false;
        }
        Status =
            $"Counting {_profileName}: {_outstanding.Count} item(s) to identify.";
        ReportProgress();
        if (canAct)
            RequestNextAppraisal();
        return _foreground;
    }

    /// <summary>Puts down whatever a count had in flight.</summary>
    public void Reset() => Stop(IdleStatus);

    /// <summary>
    /// Cancels a count on request and says what was cancelled. A foreground
    /// count holds the character while it waits, so a person who asked for
    /// one against the wrong profile, or one that is waiting on something
    /// that will not answer, needs a way to take the character back that is
    /// not logging out.
    /// </summary>
    /// <returns>The line to show whoever asked.</returns>
    public string Cancel()
    {
        if (!IsRunning)
        {
            Stop(IdleStatus);
            return "Item counter is not running.";
        }
        string cancelled = _profileName;
        Stop($"Item counter stopped: {cancelled}.");
        return Status;
    }

    // ── the count itself ──────────────────────────────────────────────────

    private readonly record struct CountedItem(
        string RuleName,
        string ItemName,
        int Count);

    /// <summary>
    /// One pass over the packs. Every item that can be decided now is decided
    /// and counted once; everything whose decision still turns on an
    /// appraisal is left outstanding for <see cref="Tick"/> to ask about.
    /// </summary>
    private void Scan()
    {
        _outstanding.Clear();
        IItemAutomation items = _host.Automation.Items;
        foreach (PluginInventoryItem item in items.CaptureOwnedItems())
        {
            // Worn and wielded gear is what the character is using, not what
            // it is carrying, so a profile count leaves it out.
            if (item.IsEquipped || item.WielderObjectId != 0u)
                continue;
            if (_counted.ContainsKey(item.ObjectId))
                continue;
            // An item the pack list names but the client's object table has
            // no record of can neither be appraised nor decided: an appraisal
            // request for it is refused, and there is nothing to read a rule
            // against. The count passes over it rather than queueing a
            // question that nothing will ever answer. It happens in ordinary
            // play whenever an item's world record is replaced under the
            // same id.
            if (!_host.Automation.Objects.TryGet(
                item.ObjectId,
                out PluginWorldObject known))
            {
                continue;
            }
            PluginItemProperties properties = items.TryCaptureProperties(
                item.ObjectId,
                out PluginItemProperties value)
                    ? value
                    : EmptyProperties;
            if (NeedsAppraisal(item, properties, known.HasAppraisalData))
            {
                _outstanding.Add(item.ObjectId);
                continue;
            }
            if (!TryDecideKeep(item, properties, out string ruleName))
                continue;
            _counted[item.ObjectId] = new CountedItem(
                ruleName,
                item.Name,
                Math.Max(1, item.StackSize));
        }
    }

    /// <summary>
    /// True when an appraisal could still change which rule decides the item
    /// and the client does not already hold one for it. That second half is
    /// the stop: a rule written in the panel's own expression language can
    /// never declare itself decided, so without it such an item would be
    /// asked about for ever. The attempt ceiling is the other stop, for an
    /// item whose appraisal is never answered or never sent.
    /// </summary>
    /// <param name="item">The owned item being decided.</param>
    /// <param name="properties">What the client holds about it.</param>
    /// <param name="hasAppraisalData">
    /// Whether the client's record for it already carries an appraisal.
    /// </param>
    private bool NeedsAppraisal(
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        bool hasAppraisalData)
    {
        if (!LootRuleEngine.NeedsIdentify(item, properties, _rules, _host))
            return false;
        if (AttemptsSpent(item.ObjectId) >= MaximumIdentifyAttemptsPerItem)
            return false;
        return !hasAppraisalData;
    }

    /// <summary>
    /// The first rule that matches decides the item, and the count keeps the
    /// item when that rule says keep or keep-up-to.
    /// </summary>
    /// <remarks>
    /// A keep-up-to ceiling is deliberately not applied. The ceiling is
    /// measured against what is already held, which is the very inventory
    /// being counted, so applying it would cap the answer at the ceiling
    /// instead of reporting how much matches.
    /// </remarks>
    private bool TryDecideKeep(
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        out string ruleName)
    {
        for (int index = 0; index < _rules.Count; index++)
        {
            LootRule rule = _rules[index];
            if (!rule.IsMatch(item, properties, _host, out _))
                continue;
            ruleName = string.IsNullOrWhiteSpace(rule.Name)
                ? $"Rule {index + 1}"
                : rule.Name;
            return rule.Action is LootAction.Keep or LootAction.KeepUpTo;
        }
        ruleName = string.Empty;
        return false;
    }

    /// <summary>
    /// Watches the one request in flight. An answer frees the slot for the
    /// next item; silence past the timeout, or a question the client has
    /// given up on, spends one of the item's attempts instead of holding the
    /// count still.
    /// </summary>
    private void ObserveAppraisal(double elapsedSeconds)
    {
        if (_awaitingObjectId == 0u)
            return;
        _awaitingSeconds += elapsedSeconds;
        if (_host.Automation.Objects.TryGet(
                _awaitingObjectId,
                out PluginWorldObject known)
            && known.HasAppraisalData)
        {
            _awaitingObjectId = 0u;
            _awaitingSeconds = 0d;
            return;
        }
        bool abandoned = _host.Automation.Loot.Appraisal.LastAbandonedObjectId
            == _awaitingObjectId;
        if (!abandoned && _awaitingSeconds < IdentifyTimeoutSeconds)
            return;
        SpendAttempt(_awaitingObjectId);
        _awaitingObjectId = 0u;
        _awaitingSeconds = 0d;
    }

    /// <summary>How many failed requests one item has already used up.</summary>
    private int AttemptsSpent(uint objectId) =>
        _identifyAttempts.TryGetValue(objectId, out int attempts) ? attempts : 0;

    /// <summary>Charges one failed request to an item.</summary>
    private void SpendAttempt(uint objectId) =>
        _identifyAttempts[objectId] = AttemptsSpent(objectId) + 1;

    /// <summary>
    /// Asks about the next outstanding item, when the appraisal slot is free
    /// to ask with.
    /// </summary>
    private void RequestNextAppraisal()
    {
        if (_awaitingObjectId != 0u)
            return;
        // The looter and this count read the same appraisal slot, and the
        // answer that lands in it belongs to whoever asked last. A count that
        // asked over the looter would take the answer the looter is waiting
        // for, so it stands down and asks once the slot is its own.
        if (_corpseIdentifierBusy()
            || _host.Automation.Loot.Appraisal.AwaitingObjectId != 0u)
        {
            _deferred = true;
            return;
        }
        _deferred = false;
        if (_sinceRequest < IdentifyRequestIntervalSeconds)
            return;
        foreach (uint objectId in _outstanding.Order())
        {
            // An item that used up its attempts this frame is still in the
            // outstanding set until the next scan rebuilds it; there is no
            // point asking about it again.
            if (AttemptsSpent(objectId) >= MaximumIdentifyAttemptsPerItem)
                continue;
            PluginItemCommandResult result =
                _host.Automation.Objects.Identify(objectId);
            if (result.Accepted)
            {
                _awaitingObjectId = objectId;
                _awaitingSeconds = 0d;
                _sinceRequest = 0d;
                return;
            }
            // Busy is the client's own backpressure and says nothing about
            // this item, so the pass stops and asks again on a later frame.
            // Anything else is a refusal of THIS item: it costs the item one
            // of its attempts and the pass moves on to the next outstanding
            // item, so one item the client will never send a request for can
            // neither stop the others from being asked about nor hold the
            // count open for ever.
            if (result.Status == PluginItemCommandStatus.Busy)
                return;
            SpendAttempt(objectId);
        }
    }

    private void ReportProgress()
    {
        if (!_foreground || _sinceProgress < ProgressIntervalSeconds)
            return;
        _sinceProgress = 0d;
        Write(_deferred
            ? $"Items remaining to identify: {_outstanding.Count}"
                + " (waiting for the looter)"
            : $"Items remaining to identify: {_outstanding.Count}");
    }

    private void Complete()
    {
        InventoryCountTally tally = BuildTally();
        LastProfileTally = tally;
        IsRunning = false;
        _awaitingObjectId = 0u;
        _awaitingSeconds = 0d;
        _deferred = false;
        Status = $"Counted {_profileName}: {tally.Total}.";
        if (_foreground)
            ReportTally(tally, _profileName);
        _foreground = false;
    }

    /// <summary>
    /// Ends a count that ran past its deadline. Whatever is holding the
    /// outstanding set open, the character comes back and the counter is
    /// free for the next question; the count reports no answer rather than a
    /// total it knows is short.
    /// </summary>
    private void GiveUp()
    {
        int remaining = _outstanding.Count;
        string name = _profileName;
        bool foreground = _foreground;
        Stop($"Item counter gave up on {name}: "
            + $"{remaining} item(s) never identified.");
        if (foreground)
            Write(Status);
    }

    private InventoryCountTally BuildTally()
    {
        var byName = new Dictionary<string, int>(StringComparer.OrdinalIgnoreCase);
        var byRule = new Dictionary<string, int>(StringComparer.OrdinalIgnoreCase);
        int total = 0;
        foreach (CountedItem counted in _counted.Values)
        {
            Add(byName, counted.ItemName, counted.Count);
            Add(byRule, counted.RuleName, counted.Count);
            total += counted.Count;
        }
        return new InventoryCountTally(Sorted(byName), Sorted(byRule), total);
    }

    /// <summary>
    /// The answer as chat lines: what each rule accounted for, what each name
    /// accounted for, and the sum. A count that found nothing still says so,
    /// against whatever was asked for.
    /// </summary>
    private void ReportTally(InventoryCountTally tally, string subject)
    {
        foreach (InventoryCountEntry entry in tally.ByRule)
            Write($"Rule Count: {entry.Name} - {entry.Count}");
        if (tally.ByName.Count == 0)
            Write($"Item Count: {subject} - 0");
        foreach (InventoryCountEntry entry in tally.ByName)
            Write($"Item Count: {entry.Name} - {entry.Count}");
        Write($"Total Item Count: {tally.Total}");
    }

    /// <summary>
    /// A profile an expression names but cannot be read is said once per
    /// name: an expression asking every pass would otherwise fill the chat
    /// window with the same line.
    /// </summary>
    private void ReportMissingProfileOnce(string profileName)
    {
        if (string.Equals(
            _reportedMissingProfile,
            profileName,
            StringComparison.OrdinalIgnoreCase))
        {
            return;
        }
        _reportedMissingProfile = profileName;
        Write(Status);
    }

    private void Stop(string status)
    {
        _rules.Clear();
        _counted.Clear();
        _outstanding.Clear();
        _identifyAttempts.Clear();
        _profileName = string.Empty;
        _reportedMissingProfile = null;
        _foreground = false;
        _deferred = false;
        _awaitingObjectId = 0u;
        _awaitingSeconds = 0d;
        _sinceRequest = 0d;
        _sinceProgress = 0d;
        _sinceStart = 0d;
        IsRunning = false;
        LastProfileTally = null;
        Status = status;
    }

    private void Write(string text) =>
        _host.Automation.Chat.PostSystemMessage(text);

    private static void Add(Dictionary<string, int> target, string key, int count) =>
        target[key] = (target.TryGetValue(key, out int held) ? held : 0) + count;

    private static IReadOnlyList<InventoryCountEntry> Sorted(
        Dictionary<string, int> source) =>
        [.. source
            .OrderBy(static pair => pair.Key, StringComparer.OrdinalIgnoreCase)
            .Select(static pair => new InventoryCountEntry(pair.Key, pair.Value))];
}

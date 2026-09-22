using System.Globalization;
using System.Text.RegularExpressions;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>How an item name given on the command line is matched.</summary>
internal enum GiveNameMatch
{
    /// <summary>The whole name, ignoring case.</summary>
    Exact,

    /// <summary>Any part of the name, ignoring case.</summary>
    Partial,

    /// <summary>A regular expression over the name.</summary>
    Pattern,
}

/// <summary>One item queued to be given, and how much of it.</summary>
/// <param name="ObjectId">The item to hand over.</param>
/// <param name="Amount">
/// How many out of the stack, or zero for the whole item — which is what the
/// give call means by zero.
/// </param>
internal readonly record struct PendingGive(uint ObjectId, uint Amount);

/// <summary>
/// Hands items to another character, one at a time, waiting for each to leave
/// the packs before asking for the next. What goes in the queue is decided two
/// ways -- by a loot profile, or by a name typed on the command line -- and
/// everything after that decision is the same for both: the same queue, the
/// same wait on the client's answer, the same retry ladder, and the same
/// ceilings.
/// </summary>
internal sealed class ProfileGiveController
{
    private const double GiveTimeoutSeconds = 10d;

    /// <summary>The line printed when a give is missing its target.</summary>
    private const string GiveUsage =
        "/vt give[p|P|r] [itemCount] <itemName> to <target>";

    /// <summary>The line printed when a profile give is missing its target.</summary>
    private const string ProfileGiveUsage =
        "/vt ig[p] <lootProfile> to <target>";

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
    private readonly InventorySettings _settings;
    private readonly Queue<PendingGive> _pending = new();
    private uint _targetObjectId;
    private uint _waitingObjectId;
    private long _completionRevision;
    private double _waitingSeconds;
    private double _sinceGive = double.MaxValue;
    private int _attempts;
    private int _given;
    private int _failed;
    private string _subject = string.Empty;
    private string _targetName = string.Empty;

    public ProfileGiveController(
        IPluginHost host,
        MossTankLootProfileStore profiles,
        InventorySettings settings)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _profiles = profiles ?? throw new ArgumentNullException(nameof(profiles));
        _settings = settings ?? throw new ArgumentNullException(nameof(settings));
    }

    public bool IsRunning { get; private set; }
    public string Status { get; private set; } = "Item giver idle.";

    /// <summary>
    /// Queues every item a loot profile says to keep. The profile decides
    /// what goes; whole items go, because a profile counts items and not
    /// pieces of a stack.
    /// </summary>
    /// <param name="partialTargetMatch">
    /// True when part of the target's name is enough, which is what the
    /// command's <c>p</c> flag asks for.
    /// </param>
    public bool TryStart(
        string? profileName,
        string? targetName,
        bool partialTargetMatch = false)
    {
        if (IsRunning || !_host.Automation.IsAvailable)
            return false;

        string requestedProfile = profileName?.Trim() ?? string.Empty;
        if (!TryResolveTarget(
            targetName,
            checkRange: false,
            out PluginWorldObject target,
            partialTargetMatch,
            ProfileGiveUsage))
        {
            return false;
        }

        var rules = new List<LootRule>();
        // The shared itemgiver folder first, as the reference does; a loot
        // profile by that name still answers, for the expression that gives
        // by loot profile.
        if (!_profiles.TryLoadItemGiver(requestedProfile, rules)
            && !_profiles.TryLoadNamed(requestedProfile, rules))
        {
            Status = $"Item giver profile not found: {requestedProfile}.";
            return false;
        }

        IReadOnlyList<PluginInventoryItem> owned =
            _host.Automation.Items.CaptureOwnedItems();
        _pending.Clear();
        foreach (PluginInventoryItem item in Givable(owned))
        {
            PluginItemProperties properties = _host.Automation.Items
                .TryCaptureProperties(item.ObjectId, out PluginItemProperties value)
                    ? value
                    : EmptyProperties;
            if (MatchesGiveProfile(item, properties, rules))
                _pending.Enqueue(new PendingGive(item.ObjectId, 0u));
        }

        Begin(target, requestedProfile);
        return true;
    }

    /// <summary>
    /// Queues every item whose name answers to <paramref name="pattern"/>.
    /// </summary>
    /// <param name="pattern">The name, part of a name, or a regular expression.</param>
    /// <param name="match">Which of those three it is.</param>
    /// <param name="count">
    /// How many units to give in all, counting a stack as its size, or zero
    /// for everything that matches. The last item queued is split when the
    /// count runs out part way through its stack.
    /// </param>
    /// <param name="targetName">Who to give them to.</param>
    /// <param name="partialTargetMatch">
    /// True when part of the target's name is enough, which is what the
    /// command's capital <c>P</c> flag asks for.
    /// </param>
    /// <returns>True when the run started.</returns>
    public bool TryStartByName(
        string? pattern,
        GiveNameMatch match,
        int count,
        string? targetName,
        bool partialTargetMatch = false)
    {
        if (IsRunning || !_host.Automation.IsAvailable)
            return false;

        string text = pattern?.Trim() ?? string.Empty;
        if (text.Length == 0)
        {
            Status = "Item giver was given no item name.";
            return false;
        }

        Regex? matcher = null;
        if (match == GiveNameMatch.Pattern)
        {
            try
            {
                matcher = new Regex(
                    text,
                    RegexOptions.IgnoreCase | RegexOptions.CultureInvariant,
                    NamePatternTimeout);
            }
            catch (ArgumentException failure)
            {
                Status = $"Item giver pattern refused: {failure.Message}";
                return false;
            }
        }

        // The range is checked here and not per give: a target that walks off
        // mid-run is caught by the tick losing sight of it.
        if (!TryResolveTarget(
            targetName,
            checkRange: true,
            out PluginWorldObject target,
            partialTargetMatch))
        {
            return false;
        }

        int remaining = count > 0 ? count : int.MaxValue;
        _pending.Clear();
        foreach (PluginInventoryItem item in
            Givable(_host.Automation.Items.CaptureOwnedItems()))
        {
            if (remaining <= 0)
                break;
            if (!NameMatches(item.Name, text, match, matcher))
                continue;
            int stack = Math.Max(1, item.StackSize);
            if (stack <= remaining)
            {
                // The whole item goes, so the give says nothing about amount.
                _pending.Enqueue(new PendingGive(item.ObjectId, 0u));
                remaining -= stack;
                continue;
            }
            _pending.Enqueue(new PendingGive(item.ObjectId, (uint)remaining));
            remaining = 0;
        }

        Begin(target, text);
        return true;
    }

    /// <summary>
    /// Stops a run on the player's say-so. False when nothing was running.
    /// </summary>
    public bool StopRequested()
    {
        if (!IsRunning)
        {
            Status = "Item giver is not running.";
            return false;
        }
        Stop($"Item giver stopped: {_given} item(s) given to {_targetName}.");
        return true;
    }

    public bool Tick(double elapsedSeconds, bool canAct)
    {
        if (!IsRunning)
            return false;
        if (!_host.Automation.IsAvailable
            || !_host.Automation.Objects.TryGet(
                _targetObjectId,
                out PluginWorldObject target)
            || target.ObjectClass is not (PluginObjectClass.Player
                or PluginObjectClass.Npc))
        {
            Stop("Item giver stopped: target vanished.");
            return false;
        }

        double step = Math.Max(0d, elapsedSeconds);
        if (_sinceGive < 1e9d)
            _sinceGive += step;

        if (_waitingObjectId != 0u)
        {
            _waitingSeconds += step;
            PluginInventoryCompletion completion =
                _host.Automation.Items.LastInventoryCompletion;
            bool itemStillOwned = _host.Automation.Items.CaptureOwnedItems()
                .Any(item => item.ObjectId == _waitingObjectId);
            if (!itemStillOwned
                || (completion.Revision > _completionRevision
                    && completion.Kind == PluginInventoryCommandKind.Give
                    && completion.SourceObjectId == _waitingObjectId))
            {
                if (!itemStillOwned || completion.IsSuccess)
                    _given++;
                _pending.Dequeue();
                _waitingObjectId = 0u;
                _attempts = 0;
                _waitingSeconds = 0d;
                _sinceGive = 0d;
            }
            else if (_waitingSeconds >= GiveTimeoutSeconds)
            {
                if (_attempts >= BusyRetryLimit)
                {
                    _waitingObjectId = 0u;
                    _waitingSeconds = 0d;
                    Fail();
                }
                else
                {
                    _waitingObjectId = 0u;
                    _waitingSeconds = 0d;
                }
            }
            return true;
        }

        if (_pending.Count == 0)
        {
            Stop($"Item giver finished: {_given} item(s) given to {_targetName}.");
            return false;
        }
        if (!canAct || _host.Automation.Items.IsBusy)
            return true;
        // A pause between gives, for a server that dislikes being asked in a
        // burst. Zero, the default, is no pause at all.
        if (_sinceGive < Math.Max(0d, _settings.GiveDelaySeconds))
            return true;

        PendingGive next = _pending.Peek();
        if (!_host.Automation.Items.CaptureOwnedItems()
            .Any(item => item.ObjectId == next.ObjectId))
        {
            _pending.Dequeue();
            return true;
        }

        long baselineRevision =
            _host.Automation.Items.LastInventoryCompletion.Revision;
        PluginItemCommandResult result = _host.Automation.Items.Give(
            next.ObjectId,
            _targetObjectId,
            next.Amount);
        if (result.Accepted)
        {
            _waitingObjectId = next.ObjectId;
            _completionRevision = baselineRevision;
            _waitingSeconds = 0d;
            _attempts++;
            Status = $"Giving item {_given + 1} to {_targetName}…";
        }
        else if (result.Status is PluginItemCommandStatus.InvalidItem
            or PluginItemCommandStatus.InvalidTarget
            or PluginItemCommandStatus.Refused
            or PluginItemCommandStatus.Unavailable)
        {
            Fail();
        }
        return true;
    }

    public void Reset()
    {
        _pending.Clear();
        _targetObjectId = 0u;
        _waitingObjectId = 0u;
        _completionRevision = 0;
        _waitingSeconds = 0d;
        _sinceGive = double.MaxValue;
        _attempts = 0;
        _given = 0;
        _failed = 0;
        IsRunning = false;
        Status = "Item giver idle.";
    }

    /// <summary>
    /// How many times one item is asked for before it is written off. A give
    /// that lands on a busy client is simply not answered, so the ladder is
    /// the only thing that tells a slow answer from a lost one.
    /// </summary>
    private int BusyRetryLimit => Math.Max(1, _settings.GiveBusyRetryLimit);

    /// <summary>
    /// Writes off the item at the head of the queue, and stops the whole run
    /// once too many have been written off -- a run that cannot give anything
    /// should say so rather than work through the packs failing.
    /// </summary>
    private void Fail()
    {
        _pending.Dequeue();
        _attempts = 0;
        _sinceGive = 0d;
        _failed++;
        if (_failed > Math.Max(0, _settings.GiveFailureLimit))
        {
            Stop(
                $"Item giver gave up after {_failed} failure(s); "
                + $"{_given} item(s) went to {_targetName}.");
        }
    }

    private void Begin(in PluginWorldObject target, string subject)
    {
        _targetObjectId = target.ObjectId;
        _subject = subject;
        _targetName = target.Name;
        _waitingObjectId = 0u;
        _attempts = 0;
        _given = 0;
        _failed = 0;
        _waitingSeconds = 0d;
        _sinceGive = double.MaxValue;
        IsRunning = true;
        Status = _pending.Count == 0
            ? $"No items match {_subject}."
            : $"Giving {_pending.Count} item(s) to {_targetName}.";
    }

    /// <summary>
    /// Items that can actually be handed over: what the character is wearing
    /// or wielding is not among them.
    /// </summary>
    private static IEnumerable<PluginInventoryItem> Givable(
        IReadOnlyList<PluginInventoryItem> owned) => owned
            .Where(static item => !item.IsEquipped && item.WielderObjectId == 0u)
            .OrderBy(static item => item.ObjectId);

    private static bool NameMatches(
        string name,
        string text,
        GiveNameMatch match,
        Regex? matcher) => match switch
        {
            GiveNameMatch.Pattern => matcher?.IsMatch(name) ?? false,
            GiveNameMatch.Partial =>
                name.Contains(text, StringComparison.OrdinalIgnoreCase),
            _ => name.Equals(text, StringComparison.OrdinalIgnoreCase),
        };

    /// <summary>
    /// Finds the player or non-player character of that name, nearest first,
    /// and -- when asked -- refuses one standing further off than the give
    /// range allows.
    /// </summary>
    private bool TryResolveTarget(
        string? targetName,
        bool checkRange,
        out PluginWorldObject target,
        bool partialTargetMatch = false,
        string usage = GiveUsage)
    {
        target = default;
        string requested = targetName?.Trim() ?? string.Empty;

        // Part of a name is enough for the partial flag, and the empty string
        // is part of every name: without this, a line that lost its target
        // would hand the packs to whoever stood nearest.
        if (requested.Length == 0)
        {
            Status = $"Syntax: {usage}";
            return false;
        }

        target = _host.Automation.Objects.CaptureObjects()
            .Where(obj => obj.ObjectClass is PluginObjectClass.Player
                or PluginObjectClass.Npc)
            .Where(obj => partialTargetMatch
                ? obj.Name.Contains(requested, StringComparison.OrdinalIgnoreCase)
                : obj.Name.Equals(requested, StringComparison.OrdinalIgnoreCase))
            .Where(obj => obj.ObjectId != _host.Automation.Character.ObjectId)
            .OrderBy(obj => DistanceFromPlayer(obj))
            .ThenBy(static obj => obj.ObjectId)
            .FirstOrDefault();
        if (target.ObjectId == 0u)
        {
            Status = $"Item giver target not found: {requested}.";
            return false;
        }
        if (!checkRange)
            return true;

        double range = Math.Max(0d, _settings.GiveRangeMeters);
        double distance = DistanceFromPlayer(target);
        if (distance > range)
        {
            Status = string.Create(
                CultureInfo.InvariantCulture,
                $"{target.Name} is {distance:0.##} m away; the give range is {range:0.##} m.");
            target = default;
            return false;
        }
        return true;
    }

    private bool MatchesGiveProfile(
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        IReadOnlyList<LootRule> rules)
    {
        foreach (LootRule rule in rules)
        {
            if (!rule.IsMatch(item, properties, _host, out _))
                continue;
            return rule.Action is LootAction.Keep or LootAction.KeepUpTo;
        }
        return false;
    }

    /// <summary>
    /// Flat ground distance in meters: a target one floor up a stairwell and a
    /// target beside it are the same distance away, which is the measure the
    /// whole plugin ranges by.
    /// </summary>
    private double DistanceFromPlayer(in PluginWorldObject target)
    {
        PluginNavigationSnapshot player = _host.Automation.Navigation.Snapshot;
        return !player.IsAvailable || !target.HasPosition
            ? double.MaxValue
            : player.Position.HorizontalDistanceMeters(target.Position);
    }

    private void Stop(string status)
    {
        _pending.Clear();
        _targetObjectId = 0u;
        _waitingObjectId = 0u;
        _completionRevision = 0;
        _waitingSeconds = 0d;
        _sinceGive = double.MaxValue;
        _attempts = 0;
        IsRunning = false;
        Status = status;
    }
}

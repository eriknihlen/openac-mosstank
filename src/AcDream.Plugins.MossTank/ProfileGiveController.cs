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
    /// <summary>
    /// How long a run may go without moving on to its next item before it
    /// bails, as the reference's item giver does: the whole run, not one
    /// give, is what the clock watches. It restarts when a new item is first
    /// asked for.
    /// </summary>
    internal const double BailSeconds = 10d;

    /// <summary>The line printed when a give is missing its target.</summary>
    private const string GiveUsage =
        "/ub give[p|P|r] [itemCount] <itemName> to <target>";

    /// <summary>The line printed when a profile give is missing its target.</summary>
    private const string ProfileGiveUsage =
        "/ub ig[p] <lootProfile> to <target>";

    /// <summary>Who a hand-over may go to.</summary>
    private static readonly PluginObjectClass[] TargetClasses =
        [PluginObjectClass.Player, PluginObjectClass.Npc];

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
    private double _sinceGive = double.MaxValue;
    private int _attempts;
    private int _given;
    private int _failed;
    private string _subject = string.Empty;
    private string _targetName = string.Empty;
    private string _typedTarget = string.Empty;
    private int _giveCalls;
    private double _sinceProgress;
    private uint _currentObjectId;

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
    /// Why the last start was refused, in the reference's words where it
    /// has them ("player Bob not found"), for the command to print as the
    /// tool's error.
    /// </summary>
    public string Refusal { get; private set; } = string.Empty;

    /// <summary>
    /// The line the last run ended on, as the reference says it: "ItemGiver
    /// finished: &lt;what&gt; to &lt;who&gt;. took &lt;time&gt; to give
    /// &lt;n&gt; item(s). &lt;failures&gt;". Every end of a run says it.
    /// </summary>
    public string FinishedLine { get; private set; } = string.Empty;

    /// <summary>
    /// The error the last run ended on, printed before its finished line, or
    /// empty when it ended without one: "ItemGiver bail, Timeout expired"
    /// for a run that stopped moving.
    /// </summary>
    public string EndError { get; private set; } = string.Empty;

    private double _runSeconds;

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
        if (!CanStart())
            return false;

        string requestedProfile = profileName?.Trim() ?? string.Empty;
        if (!TryResolveTarget(
            targetName,
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
            Refusal = $"ItemGiver Profile does not exist: {requestedProfile}";
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

        // The reference names a profile hand-over by the file it loaded, so
        // the end line keeps the extension.
        string file = requestedProfile.EndsWith(".utl", StringComparison.OrdinalIgnoreCase)
            ? requestedProfile
            : requestedProfile + ".utl";
        Begin(target, file, targetName);
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
        if (!CanStart())
            return false;

        string text = pattern?.Trim() ?? string.Empty;
        if (text.Length == 0)
        {
            Status = "Item giver was given no item name.";
            Refusal = Status;
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
                Refusal = Status;
                return false;
            }
        }

        // The range is checked here and not per give: a target that walks off
        // mid-run is caught by the tick losing sight of it.
        if (!TryResolveTarget(
            targetName,
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

        // The reference matches a name in lower case and names the run by it;
        // a pattern is kept as it was written.
        Begin(target, match == GiveNameMatch.Pattern ? text : text.ToLowerInvariant(), targetName);
        return true;
    }

    /// <summary>
    /// Clears the last refusal, then says whether a start may go ahead at
    /// all: not while a run is going, and not outside the world -- each with
    /// its own refusal, so a start refused here never reports the reason an
    /// earlier start was refused for.
    /// </summary>
    private bool CanStart()
    {
        Refusal = string.Empty;
        if (IsRunning)
        {
            Refusal = "Already running.";
            return false;
        }
        if (!_host.Automation.IsAvailable)
        {
            Status = "Item giver refused: not in the world.";
            Refusal = Status;
            return false;
        }
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
        double step = Math.Max(0d, elapsedSeconds);
        _sinceProgress += step;
        bool owns = TickRun(step, canAct, out bool checkBail);
        // The reference looks at its clock only on a step that waited: not
        // inside the pause between asks, and not on a step that asked. A run
        // that has not moved on to a new item for ten seconds then bails,
        // however many asks of the current one are still allowed.
        if (!IsRunning || !checkBail || _sinceProgress <= BailSeconds)
            return owns;
        EndError = "ItemGiver bail, Timeout expired";
        Stop($"Item giver bailed: no progress for {BailSeconds:0} seconds.");
        return false;
    }

    /// <summary>
    /// One step of a run, in the reference's order: the pause since the last
    /// ask, then the wait on that ask's answer, then everything that has left
    /// the packs counted as given, then the next ask.
    /// </summary>
    /// <param name="step">Seconds since the last step.</param>
    /// <param name="canAct">False while something else holds the action.</param>
    /// <param name="checkBail">
    /// True when the step waited, which is when the run's clock is looked at.
    /// </param>
    private bool TickRun(double step, bool canAct, out bool checkBail)
    {
        checkBail = false;
        _runSeconds += step;
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

        if (_sinceGive < 1e9d)
            _sinceGive += step;
        // A pause between asks, for a server that dislikes being asked in a
        // burst, counted from the last ask. Zero, the default, is no pause.
        if (_sinceGive < Math.Max(0d, _settings.GiveDelaySeconds))
            return true;

        if (_waitingObjectId != 0u)
        {
            PluginInventoryCompletion completion =
                _host.Automation.Items.LastInventoryCompletion;
            bool itemStillOwned = _host.Automation.Items.CaptureOwnedItems()
                .Any(item => item.ObjectId == _waitingObjectId);
            if (itemStillOwned
                && !(completion.Revision > _completionRevision
                    && completion.Kind == PluginInventoryCommandKind.Give
                    && completion.SourceObjectId == _waitingObjectId))
            {
                checkBail = true;
                return true;
            }
            _waitingObjectId = 0u;
            if (!itemStillOwned || completion.IsSuccess)
            {
                _pending.Dequeue();
                _given++;
            }
            // An error leaves the item in the packs and at the head of the
            // queue, to be asked for again.
        }

        CountItemsThatLeft();
        if (_pending.Count == 0)
        {
            Stop($"Item giver finished: {_given} item(s) given to {_targetName}.");
            return false;
        }
        if (!canAct || _host.Automation.Items.IsBusy)
        {
            checkBail = true;
            return true;
        }

        Ask(_pending.Peek());
        return IsRunning;
    }

    /// <summary>
    /// Counts every queued item that is no longer in the packs as given and
    /// drops it, as the reference does before each ask: it counts an item
    /// that has left as given, however it left.
    /// </summary>
    private void CountItemsThatLeft()
    {
        if (_pending.Count == 0)
            return;
        var owned = new HashSet<uint>(_host.Automation.Items.CaptureOwnedItems()
            .Select(static item => item.ObjectId));
        int before = _pending.Count;
        PendingGive[] kept = [.. _pending.Where(give => owned.Contains(give.ObjectId))];
        if (kept.Length == before)
            return;
        _given += before - kept.Length;
        _pending.Clear();
        foreach (PendingGive give in kept)
            _pending.Enqueue(give);
    }

    /// <summary>
    /// Asks for one item, as the reference does: the first ask of a new item
    /// restarts the run's clock and its count of asks; every ask counts,
    /// whether or not it went out; and the ask that takes an item past the
    /// busy count writes it off at once, without waiting for its answer.
    /// </summary>
    private void Ask(in PendingGive next)
    {
        if (next.ObjectId != _currentObjectId)
        {
            _attempts = 0;
            _sinceProgress = 0d;
        }
        _currentObjectId = next.ObjectId;
        _attempts++;
        _giveCalls++;

        long baselineRevision =
            _host.Automation.Items.LastInventoryCompletion.Revision;
        PluginItemCommandResult result = _host.Automation.Items.Give(
            next.ObjectId,
            _targetObjectId,
            next.Amount);
        _sinceGive = 0d;
        if (_attempts > BusyRetryLimit)
        {
            WriteOff();
            return;
        }
        if (result.Accepted)
        {
            _waitingObjectId = next.ObjectId;
            _completionRevision = baselineRevision;
            Status = $"Giving item {_given + 1} to {_targetName}…";
        }
    }

    public void Reset()
    {
        _pending.Clear();
        _targetObjectId = 0u;
        _waitingObjectId = 0u;
        _currentObjectId = 0u;
        _completionRevision = 0;
        _sinceGive = double.MaxValue;
        _attempts = 0;
        _given = 0;
        _failed = 0;
        IsRunning = false;
        Status = "Item giver idle.";
    }

    /// <summary>
    /// The busy count: an item is asked for once more than this, then
    /// written off. A give that lands on a busy client is simply not
    /// answered, so the count is the only thing that tells a slow answer from
    /// a lost one.
    /// </summary>
    private int BusyRetryLimit => Math.Max(1, _settings.GiveBusyRetryLimit);

    /// <summary>
    /// Writes off the item at the head of the queue, and stops the whole run
    /// once more have been written off than the failure count allows -- a run
    /// that cannot give anything should say so rather than work through the
    /// packs failing.
    /// </summary>
    private void WriteOff()
    {
        _pending.Dequeue();
        _failed++;
        if (_failed > Math.Max(0, _settings.GiveFailureLimit))
        {
            Stop(
                $"Item giver gave up after {_failed} failure(s); "
                + $"{_given} item(s) went to {_targetName}.");
        }
    }

    private void Begin(in PluginWorldObject target, string subject, string? typedTarget)
    {
        _typedTarget = typedTarget?.Trim() ?? string.Empty;
        _giveCalls = 0;
        _targetObjectId = target.ObjectId;
        _subject = subject;
        _targetName = target.Name;
        _waitingObjectId = 0u;
        _currentObjectId = 0u;
        _attempts = 0;
        _given = 0;
        _failed = 0;
        _sinceGive = double.MaxValue;
        _runSeconds = 0d;
        _sinceProgress = 0d;
        EndError = string.Empty;
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
    /// and refuses one standing further off than the give range allows --
    /// for a profile hand-over as for a give by name, each refusal in the
    /// reference's words for that command, naming the target as typed.
    /// </summary>
    private bool TryResolveTarget(
        string? targetName,
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
            Refusal = Status;
            return false;
        }

        // The reference's object search: an id, a hex id or "selected"
        // first, then the nearest player or non-player character the name
        // answers to. The first three can name the character itself, which
        // the reference refuses in words of its own.
        if (!UbObjectSearch.TryFindNearest(
            _host,
            requested,
            partialTargetMatch,
            TargetClasses,
            out target))
        {
            Status = $"Item giver target not found: {requested}.";
            Refusal = $"player {requested} not found";
            return false;
        }
        if (target.ObjectId == _host.Automation.Character.ObjectId)
        {
            Status = "Item giver refused: the target is yourself.";
            Refusal = "You can't give to yourself";
            target = default;
            return false;
        }
        double range = Math.Max(0d, _settings.GiveRangeMeters);
        double distance = DistanceFromPlayer(target);
        if (distance > range)
        {
            Status = string.Create(
                CultureInfo.InvariantCulture,
                $"{target.Name} is {distance:0.##} m away; the give range is {range:0.##} m.");
            Refusal = usage == ProfileGiveUsage
                ? string.Create(
                    CultureInfo.InvariantCulture,
                    $"ItemGiver {requested} is {distance:n2} meters away. IGRange is set to {range}")
                : string.Create(
                    CultureInfo.InvariantCulture,
                    $"{requested} is {distance:n2} meters away, IGRange is set to {range}. bailing.");
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
        FinishedLine = string.Create(
            CultureInfo.InvariantCulture,
            $"ItemGiver finished: {_subject} to {_typedTarget}. took {FriendlyTime(_runSeconds)} to give {_given} item(s). {_giveCalls - _given}");
        _pending.Clear();
        _targetObjectId = 0u;
        _waitingObjectId = 0u;
        _completionRevision = 0;
        _sinceGive = double.MaxValue;
        _attempts = 0;
        IsRunning = false;
        Status = status;
    }

    /// <summary>
    /// A span as the reference writes one: days, hours, minutes and seconds
    /// that are not zero ("1m 5s"), or "0s".
    /// </summary>
    internal static string FriendlyTime(double seconds)
    {
        var span = TimeSpan.FromSeconds(Math.Max(0d, Math.Floor(seconds)));
        var parts = new List<string>(4);
        if (span.Days > 0)
            parts.Add(span.Days.ToString(CultureInfo.InvariantCulture) + "d");
        if (span.Hours > 0)
            parts.Add(span.Hours.ToString(CultureInfo.InvariantCulture) + "h");
        if (span.Minutes > 0)
            parts.Add(span.Minutes.ToString(CultureInfo.InvariantCulture) + "m");
        if (span.Seconds > 0)
            parts.Add(span.Seconds.ToString(CultureInfo.InvariantCulture) + "s");
        return parts.Count == 0 ? "0s" : string.Join(' ', parts);
    }
}

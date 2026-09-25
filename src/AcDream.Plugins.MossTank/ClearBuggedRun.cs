using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>
/// <c>/ub clearbugged</c>: asks the server to describe every item the
/// character carries and lets go of the ones the server no longer has, the
/// "ghost" items a client keeps listing after the server has lost them.
/// </summary>
/// <remarks>
/// <para>
/// The reference drops an item the moment the server refuses to describe it.
/// A refusal alone proves little: the server also refuses an item made to
/// resist appraisal, and a repeat request for the same item sent within five
/// seconds of an unsuccessful one. So an item counts as a ghost only when it
/// is refused twice, the second request going out at least
/// <see cref="RecheckDelaySeconds"/> after the first refusal came back; an
/// item that answers the second time is kept. An item made to resist
/// appraisal is refused both times and is let go of like a ghost -- the
/// client cannot tell the two apart -- and the server lists it again the next
/// time it describes the inventory.
/// </para>
/// <para>
/// Worn and wielded items are left alone, and so is a pack that still lists
/// contents. Requests go out one at a time, at most one every
/// <see cref="IdentifyIntervalSeconds"/>, the reference's identify cadence.
/// </para>
/// </remarks>
internal sealed class ClearBuggedRun
{
    /// <summary>The reference's identify queue sends one request every 499 ms.</summary>
    internal const double IdentifyIntervalSeconds = 0.499d;

    /// <summary>
    /// How long after the first refusal the item is asked about again. The
    /// server refuses a repeat request inside this window without looking.
    /// </summary>
    internal const double RecheckDelaySeconds = 5d;

    /// <summary>
    /// How long a run may go without any request going out, any answer
    /// coming back or any item being let go of before it ends. Every item
    /// gives up on its own well inside this, so only a run that is stuck --
    /// the slot held by someone else, or the session out of the world --
    /// reaches it.
    /// </summary>
    internal const double NoProgressTimeoutSeconds = 30d;

    /// <summary>
    /// How long one request may go unanswered before it is given up on and
    /// the item left alone: twice the client's own bound on the wait.
    /// </summary>
    internal const double AnswerTimeoutSeconds = 10d;

    /// <summary>
    /// How often the run says how many items it is still waiting on: on its
    /// first turn, then every ten seconds while any are left, as the
    /// reference's appraiser does.
    /// </summary>
    internal const double WaitingLineIntervalSeconds = 10d;

    private const string Name = "ClearBugged";

    private readonly IPluginHost _host;
    private readonly Action<string> _write;
    private readonly Queue<uint> _firstChecks = new();
    private readonly List<(uint ObjectId, double DueAt)> _rechecks = [];
    private readonly List<uint> _toForget = [];
    private readonly Dictionary<uint, string> _names = [];
    private bool _running;
    private double _clock;
    private double _sinceRequest;
    private uint _awaiting;
    private bool _awaitingRecheck;
    private double _awaitingAge;
    private long _awaitingRevision;
    private double _sinceProgress;
    private int _carried;
    private double _untilWaitingLine;

    public ClearBuggedRun(IPluginHost host, Action<string> write)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _write = write ?? throw new ArgumentNullException(nameof(write));
    }

    /// <summary>Whether a run is under way.</summary>
    public bool IsRunning => _running;

    /// <summary>
    /// Starts a run over everything the character carries; refused while a
    /// run is already under way.
    /// </summary>
    public void Start()
    {
        if (_running)
        {
            _write(UbChat.ToolError(UbChat.Tools.InventoryManager, $"{Name}: already running."));
            return;
        }
        if (!_host.Automation.IsAvailable)
        {
            _write(UbChat.ToolError(UbChat.Tools.InventoryManager, $"{Name}: not in the world."));
            return;
        }
        Reset();
        IReadOnlyList<PluginInventoryItem> owned = _host.Automation.Items.CaptureOwnedItems();
        // The count is everything carried, worn and packs included, as the
        // reference counts it; only the items that can safely be let go of
        // are asked about.
        int carried = owned.Count(item => item.ObjectId != 0u && item.ObjectId != OwnObjectId);
        foreach (PluginInventoryItem item in owned)
        {
            if (!IsCandidate(item, owned))
                continue;
            _firstChecks.Enqueue(item.ObjectId);
            _names[item.ObjectId] = item.Name ?? string.Empty;
        }
        _running = true;
        _carried = carried;
        _sinceRequest = IdentifyIntervalSeconds;
        _write(UbChat.Tool(UbChat.Tools.InventoryManager, string.Create(
            CultureInfo.InvariantCulture,
            $"{Name}: Identifying {carried} items, to check for bugged items...")));
        // The reference's appraiser announces the job it was handed, every
        // item carried, before the first request goes out.
        _write(UbChat.Line(string.Create(
            CultureInfo.InvariantCulture,
            $"new Assessor.Job initialized with {carried} items...")));
        FinishWhenDone();
    }

    /// <summary>Drives the run; a no-op while none is under way.</summary>
    public void Tick(double elapsedSeconds)
    {
        if (!_running)
            return;
        _sinceProgress += elapsedSeconds;
        if (_sinceProgress >= NoProgressTimeoutSeconds)
        {
            Finish();
            return;
        }
        if (!_host.Automation.IsAvailable)
        {
            SayWaitingWhenDue(elapsedSeconds);
            return;
        }
        _clock += elapsedSeconds;
        _sinceRequest += elapsedSeconds;
        ForgetConfirmedGhosts();
        if (_awaiting != 0u)
        {
            _awaitingAge += elapsedSeconds;
            PollAnswer();
        }
        if (_awaiting == 0u && _sinceRequest >= IdentifyIntervalSeconds)
            RequestNext();
        FinishWhenDone();
        SayWaitingWhenDue(elapsedSeconds);
    }

    /// <summary>Drops a run without a word, as a session that ends does.</summary>
    public void Reset()
    {
        _running = false;
        _firstChecks.Clear();
        _rechecks.Clear();
        _toForget.Clear();
        _names.Clear();
        _clock = 0d;
        _sinceRequest = 0d;
        _awaiting = 0u;
        _awaitingRecheck = false;
        _awaitingAge = 0d;
        _awaitingRevision = 0L;
        _sinceProgress = 0d;
        _carried = 0;
        _untilWaitingLine = 0d;
    }

    /// <summary>
    /// The items still to be settled: waiting for a first or second request,
    /// asked and not yet answered, or refused twice and not yet let go of.
    /// Each is in exactly one of those places.
    /// </summary>
    private int Pending =>
        _firstChecks.Count + _rechecks.Count + _toForget.Count + (_awaiting != 0u ? 1 : 0);

    /// <summary>
    /// The reference's appraiser's progress line, on the run's first turn and
    /// every <see cref="WaitingLineIntervalSeconds"/> after while the run
    /// lasts: the items still pending out of every item carried, and the time
    /// they take at one request per <see cref="IdentifyIntervalSeconds"/>.
    /// </summary>
    private void SayWaitingWhenDue(double elapsedSeconds)
    {
        if (!_running)
            return;
        // Due once the interval has passed, not on the mark itself.
        _untilWaitingLine -= elapsedSeconds;
        if (_untilWaitingLine >= 0d)
            return;
        _untilWaitingLine = WaitingLineIntervalSeconds;
        int pending = Pending;
        _write(UbChat.Line(string.Create(
            CultureInfo.InvariantCulture,
            $"Assessor waiting to ID {pending} of {_carried} items. This will take about {pending * IdentifyIntervalSeconds:n2} seconds.")));
    }

    private uint OwnObjectId => _host.Automation.Character.ObjectId;

    /// <summary>
    /// An item the run may ask about and let go of: carried, not the
    /// character, not worn or wielded, and not a pack that still lists
    /// contents.
    /// </summary>
    private bool IsCandidate(
        in PluginInventoryItem item,
        IReadOnlyList<PluginInventoryItem> owned)
    {
        if (item.ObjectId == 0u || item.ObjectId == OwnObjectId)
            return false;
        if (item.IsEquipped || item.WielderObjectId != 0u)
            return false;
        foreach (PluginInventoryItem other in owned)
        {
            if (other.ContainerObjectId == item.ObjectId)
                return false;
        }
        return true;
    }

    private bool IsStillCandidate(uint objectId)
    {
        IReadOnlyList<PluginInventoryItem> owned = _host.Automation.Items.CaptureOwnedItems();
        foreach (PluginInventoryItem item in owned)
        {
            if (item.ObjectId == objectId)
                return IsCandidate(item, owned);
        }
        return false;
    }

    private void RequestNext()
    {
        int due = -1;
        for (int index = 0; index < _rechecks.Count; index++)
        {
            if (_rechecks[index].DueAt <= _clock
                && (due < 0 || _rechecks[index].DueAt < _rechecks[due].DueAt))
            {
                due = index;
            }
        }
        uint objectId;
        bool recheck = due >= 0;
        if (recheck)
            objectId = _rechecks[due].ObjectId;
        else if (_firstChecks.Count != 0)
            objectId = _firstChecks.Peek();
        else
            return;

        // An item moved out of reach since the run began -- dropped, given,
        // worn, or a pack that has been filled -- is no longer asked about.
        if (!IsStillCandidate(objectId))
        {
            Consume(recheck, due);
            return;
        }

        PluginItemCommandResult asked = _host.Automation.Objects.Identify(objectId);
        _sinceRequest = 0d;
        if (asked.Status == PluginItemCommandStatus.Busy)
            return; // Another request holds the slot; asked again next turn.
        Consume(recheck, due);
        _sinceProgress = 0d;
        if (!asked.Accepted)
            return;
        _awaiting = objectId;
        _awaitingRecheck = recheck;
        _awaitingAge = 0d;
        _awaitingRevision = _host.Automation.Loot.Appraisal.Revision;
    }

    private void Consume(bool recheck, int due)
    {
        if (recheck)
            _rechecks.RemoveAt(due);
        else
            _firstChecks.Dequeue();
    }

    /// <summary>
    /// Reads the client's appraisal slot for the answer to the request in
    /// flight. The answer is a change to the slot that completes this item;
    /// a slot that merely stops reporting the wait (it ran out) has not
    /// changed, and an item the slot already showed as complete before the
    /// request is not taken for a fresh answer.
    /// </summary>
    private void PollAnswer()
    {
        PluginAppraisalState appraisal = _host.Automation.Loot.Appraisal;
        if (appraisal.LastAbandonedObjectId == _awaiting)
        {
            GiveUpOnAnswer();
            return;
        }
        if (appraisal.AwaitingObjectId == _awaiting)
        {
            _awaitingRevision = appraisal.Revision;
        }
        else if (appraisal.CurrentObjectId == _awaiting
            && appraisal.Revision != _awaitingRevision)
        {
            Answered(appraisal.CurrentObjectUnsuccessful);
            return;
        }
        if (_awaitingAge >= AnswerTimeoutSeconds)
            GiveUpOnAnswer();
    }

    private void Answered(bool refused)
    {
        uint objectId = _awaiting;
        bool recheck = _awaitingRecheck;
        _awaiting = 0u;
        _awaitingRecheck = false;
        _sinceProgress = 0d;
        if (!refused)
            return;
        if (recheck)
            _toForget.Add(objectId);
        else
            _rechecks.Add((objectId, _clock + RecheckDelaySeconds));
    }

    private void GiveUpOnAnswer()
    {
        _awaiting = 0u;
        _awaitingRecheck = false;
        _sinceProgress = 0d;
    }

    private void ForgetConfirmedGhosts()
    {
        for (int index = 0; index < _toForget.Count;)
        {
            if (_host.Automation.Items.IsBusy)
                return;
            uint objectId = _toForget[index];
            string name = _names.TryGetValue(objectId, out string? known) ? known : string.Empty;
            PluginItemCommandResult result = _host.Automation.Items.ForgetStaleItem(objectId);
            if (result.Status == PluginItemCommandStatus.Busy)
                return;
            _toForget.RemoveAt(index);
            _sinceProgress = 0d;
            if (result.Status == PluginItemCommandStatus.Completed)
            {
                // The reference's appraiser reports each one as its error.
                _write(UbChat.ToolError(UbChat.Tools.Assessor, string.Create(
                    CultureInfo.InvariantCulture,
                    $"Bugged Item: 0x{objectId:X8} {name}")));
            }
            else
            {
                string why = string.IsNullOrWhiteSpace(result.Notice)
                    ? result.Status.ToString()
                    : result.Notice;
                _write(UbChat.ToolError(UbChat.Tools.InventoryManager, string.Create(
                    CultureInfo.InvariantCulture,
                    $"{Name}: could not remove 0x{objectId:X8} {name}: {why}")));
            }
        }
    }

    private void FinishWhenDone()
    {
        if (!_running
            || _awaiting != 0u
            || _firstChecks.Count != 0
            || _rechecks.Count != 0
            || _toForget.Count != 0)
        {
            return;
        }
        Finish();
    }

    /// <summary>
    /// Ends the run with the reference's own last line. What was let go of
    /// has already been named one line each; an item that was never
    /// answered is simply left alone, as the reference leaves it.
    /// </summary>
    private void Finish()
    {
        Reset();
        _write(UbChat.Tool(UbChat.Tools.InventoryManager, $"{Name}: Done!"));
    }
}

using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// <c>/ub clearbugged</c> asks the server about every carried item and lets
/// go only of an item refused twice, the second request going out at least
/// five seconds after the first refusal: the server also refuses an item made
/// to resist appraisal, and a repeat request inside five seconds, without
/// either meaning the item is gone.
/// </summary>
public sealed class ClearBuggedRunTests
{
    private const double Frame = 0.1d;

    /// <summary>
    /// A ghost is refused on both requests, five seconds apart, and let go of.
    /// Mutation: forgetting on the first refusal (no second request) fails the
    /// request count; no forget call leaves the item and fails the report.
    /// </summary>
    [Fact]
    public void AGhostRefusedTwiceFiveSecondsApartIsLetGoOf()
    {
        var automation = new FakeAutomation();
        automation.Add(Item(10u, "Phantom Pack"), ServerAnswer.Gone);
        var run = new ClearBuggedRun(new FakeHost(automation), automation.Messages.Add);

        run.Start();
        Drive(run, automation, seconds: 20d);

        Assert.Equal([10u, 10u], automation.Requests.Select(static request => request.ObjectId));
        Assert.True(automation.Requests[1].At - automation.Answers[0].At
            >= ClearBuggedRun.RecheckDelaySeconds - 1e-9);
        Assert.Equal([10u], automation.Forgotten);
        Assert.Empty(automation.Owned);
        Assert.Equal(
            [
                "[UB] InventoryManager: ClearBugged: Identifying 1 items, to check for bugged items...",
                "[UB] new Assessor.Job initialized with 1 items...",
                "[UB] Assessor waiting to ID 1 of 1 items. This will take about 0.50 seconds.",
                "[UB] Error: Assessor: Bugged Item: 0x0000000A Phantom Pack",
                "[UB] InventoryManager: ClearBugged: Done!",
            ],
            automation.Messages);
        Assert.False(run.IsRunning);
    }

    /// <summary>
    /// The appraiser's own lines: the job it was handed as the run starts,
    /// then how many items are still pending out of all carried, on the first
    /// turn and every ten seconds after, until the run is done. Mutation:
    /// dropping the progress line, or printing it every turn, fails the
    /// count and the spacing.
    /// </summary>
    [Fact]
    public void TheAppraiserSaysWhatIsLeftEveryTenSeconds()
    {
        var automation = new FakeAutomation();
        for (uint id = 100u; id < 150u; id++)
            automation.Add(Item(id, "Gem"), ServerAnswer.Described);
        var said = new List<(double At, string Line)>();
        var run = new ClearBuggedRun(new FakeHost(automation), line => said.Add((automation.Now, line)));

        run.Start();
        Drive(run, automation, seconds: 60d);

        Assert.Equal("[UB] new Assessor.Job initialized with 50 items...", said[1].Line);
        List<(double At, string Line)> waiting =
            [.. said.Where(static s => s.Line.StartsWith("[UB] Assessor waiting to ID ", StringComparison.Ordinal))];
        Assert.Equal(
            "[UB] Assessor waiting to ID 50 of 50 items. This will take about 24.95 seconds.",
            waiting[0].Line);
        Assert.Equal(3, waiting.Count);
        // The next line is due once more than ten seconds have passed: on
        // the first turn past the mark.
        for (int index = 1; index < waiting.Count; index++)
        {
            double gap = waiting[index].At - waiting[index - 1].At;
            Assert.InRange(
                gap,
                ClearBuggedRun.WaitingLineIntervalSeconds - 1e-9,
                ClearBuggedRun.WaitingLineIntervalSeconds + Frame + 1e-9);
        }
        Assert.Matches(@"^\[UB\] Assessor waiting to ID \d+ of 50 items\. This will take about \d+\.\d\d seconds\.$", waiting[^1].Line);
        Assert.Equal("[UB] InventoryManager: ClearBugged: Done!", said[^1].Line);
        Assert.False(run.IsRunning);
    }

    /// <summary>
    /// An item the server describes is kept, and asked about once.
    /// Mutation: letting go of every item asked about forgets it.
    /// </summary>
    [Fact]
    public void AnItemTheServerDescribesIsKept()
    {
        var automation = new FakeAutomation();
        automation.Add(Item(11u, "Pyreal"), ServerAnswer.Described);
        var run = new ClearBuggedRun(new FakeHost(automation), automation.Messages.Add);

        run.Start();
        Drive(run, automation, seconds: 20d);

        Assert.Equal([11u], automation.Requests.Select(static request => request.ObjectId));
        Assert.Empty(automation.Forgotten);
        Assert.Equal("[UB] InventoryManager: ClearBugged: Done!", automation.Messages[^1]);
    }

    /// <summary>
    /// An item refused once and described on the second request -- the
    /// server's repeat-request refusal, or a refusal that did not hold -- is
    /// real, and kept. Mutation: letting go after the first refusal forgets it.
    /// </summary>
    [Fact]
    public void AnItemRefusedOnceAndDescribedTheSecondTimeIsKept()
    {
        var automation = new FakeAutomation();
        automation.Add(Item(12u, "Stubborn Gem"), ServerAnswer.RefusedOnce);
        var run = new ClearBuggedRun(new FakeHost(automation), automation.Messages.Add);

        run.Start();
        Drive(run, automation, seconds: 20d);

        Assert.Equal([12u, 12u], automation.Requests.Select(static request => request.ObjectId));
        Assert.Empty(automation.Forgotten);
        Assert.Single(automation.Owned);
        Assert.Equal("[UB] InventoryManager: ClearBugged: Done!", automation.Messages[^1]);
    }

    /// <summary>
    /// An item made to resist appraisal is refused every time and the client
    /// cannot tell it from a ghost: it is let go of, and the server lists it
    /// again the next time it describes the inventory. Pinned so the choice is
    /// a decision, not an accident.
    /// </summary>
    [Fact]
    public void AnItemThatResistsEveryTimeIsLetGoOfLikeAGhost()
    {
        var automation = new FakeAutomation();
        automation.Add(Item(13u, "Sealed Chest"), ServerAnswer.Resists);
        var run = new ClearBuggedRun(new FakeHost(automation), automation.Messages.Add);

        run.Start();
        Drive(run, automation, seconds: 20d);

        Assert.Equal([13u], automation.Forgotten);
    }

    /// <summary>
    /// Worn and wielded items are never asked about, nor a pack that still
    /// lists contents; an empty pack and the contents of a full one are.
    /// The count on the first line is everything carried, as the reference
    /// counts it. Mutation: dropping either skip asks about (and lets go of)
    /// them; counting only the items asked about prints 2.
    /// </summary>
    [Fact]
    public void WornWieldedAndFilledPacksAreLeftAlone()
    {
        var automation = new FakeAutomation();
        automation.Add(Item(20u, "Worn Robe", equippedLocation: 0x200u), ServerAnswer.Gone);
        automation.Add(Item(21u, "Wielded Sword", wielder: FakeAutomation.OwnId), ServerAnswer.Gone);
        automation.Add(Item(22u, "Full Pack"), ServerAnswer.Gone);
        automation.Add(Item(23u, "Pack Content", container: 22u), ServerAnswer.Gone);
        automation.Add(Item(24u, "Empty Pack"), ServerAnswer.Gone);
        var run = new ClearBuggedRun(new FakeHost(automation), automation.Messages.Add);

        run.Start();
        Drive(run, automation, seconds: 30d);

        Assert.Equal(
            "[UB] InventoryManager: ClearBugged: Identifying 5 items, to check for bugged items...",
            automation.Messages[0]);
        Assert.DoesNotContain(automation.Requests, static request => request.ObjectId is 20u or 21u or 22u);
        Assert.Equal([23u, 24u], automation.Forgotten.Order());
    }

    /// <summary>
    /// The second request never goes out before five seconds have passed since
    /// the first refusal came back. Mutation: a shorter delay sends it early.
    /// </summary>
    [Fact]
    public void TheSecondRequestWaitsFiveSecondsAfterTheFirstRefusal()
    {
        var automation = new FakeAutomation();
        automation.Add(Item(30u, "Phantom"), ServerAnswer.Gone);
        var run = new ClearBuggedRun(new FakeHost(automation), automation.Messages.Add);

        run.Start();
        Drive(run, automation, seconds: 1d);
        double refusedAt = Assert.Single(automation.Answers).At;
        Drive(run, automation, seconds: refusedAt + 4.9d - automation.Now);
        Assert.Single(automation.Requests);
        Assert.Empty(automation.Forgotten);

        Drive(run, automation, seconds: 1d);
        Assert.Equal(2, automation.Requests.Count);
        Assert.Equal([30u], automation.Forgotten);
    }

    /// <summary>
    /// Requests go out one at a time, at most one every 499 ms, the
    /// reference's identify cadence. Mutation: no interval floods the server
    /// with one request a frame.
    /// </summary>
    [Fact]
    public void RequestsArePacedOnTheIdentifyCadence()
    {
        var automation = new FakeAutomation();
        for (uint id = 40u; id < 46u; id++)
            automation.Add(Item(id, "Real"), ServerAnswer.Described);
        var run = new ClearBuggedRun(new FakeHost(automation), automation.Messages.Add);

        run.Start();
        Drive(run, automation, seconds: 10d);

        Assert.Equal(6, automation.Requests.Count);
        for (int index = 1; index < automation.Requests.Count; index++)
        {
            Assert.True(automation.Requests[index].At - automation.Requests[index - 1].At
                >= ClearBuggedRun.IdentifyIntervalSeconds - 1e-9);
        }
    }

    /// <summary>
    /// A request whose wait runs out leaves the slot showing the item's
    /// earlier refusal. That stale answer is not the second refusal: the item
    /// is left alone, and the run ends on the reference's own last line.
    /// Mutation: taking any slot
    /// that shows the item complete as the answer forgets it.
    /// </summary>
    [Fact]
    public void AnUnansweredSecondRequestIsNotTakenForASecondRefusal()
    {
        var automation = new FakeAutomation();
        automation.Add(Item(50u, "Phantom"), ServerAnswer.Gone);
        var run = new ClearBuggedRun(new FakeHost(automation), automation.Messages.Add);

        run.Start();
        Drive(run, automation, seconds: 1d);
        automation.AnswerNothing = true;
        Drive(run, automation, seconds: 6d);
        Assert.Equal(2, automation.Requests.Count);
        // The client's own bound on the wait ran out: it stops reporting the
        // wait, and nothing else about the slot changes.
        automation.ExpireWait();
        Drive(run, automation, seconds: 15d);

        Assert.Empty(automation.Forgotten);
        Assert.Equal(
            "[UB] InventoryManager: ClearBugged: Done!",
            automation.Messages[^1]);
    }

    /// <summary>
    /// A slot busy with someone else's question is asked again later, not
    /// skipped. Mutation: consuming the item on Busy never asks about it.
    /// </summary>
    [Fact]
    public void ABusySlotIsAskedAgainLater()
    {
        var automation = new FakeAutomation { SlotBusy = true };
        automation.Add(Item(60u, "Phantom"), ServerAnswer.Gone);
        var run = new ClearBuggedRun(new FakeHost(automation), automation.Messages.Add);

        run.Start();
        Drive(run, automation, seconds: 2d);
        Assert.Empty(automation.Requests);
        automation.SlotBusy = false;
        Drive(run, automation, seconds: 20d);

        Assert.Equal([60u], automation.Forgotten);
    }

    /// <summary>
    /// A second start while a run is under way is refused and changes nothing.
    /// Mutation: restarting re-queues everything and asks twice.
    /// </summary>
    [Fact]
    public void ASecondStartWhileRunningIsRefused()
    {
        var automation = new FakeAutomation();
        automation.Add(Item(70u, "Real"), ServerAnswer.Described);
        automation.Add(Item(71u, "Real"), ServerAnswer.Described);
        var run = new ClearBuggedRun(new FakeHost(automation), automation.Messages.Add);

        run.Start();
        Drive(run, automation, seconds: 0.2d);
        run.Start();
        Drive(run, automation, seconds: 10d);

        Assert.Equal("[UB] Error: InventoryManager: ClearBugged: already running.", automation.Messages[3]);
        Assert.Equal([70u, 71u], automation.Requests.Select(static request => request.ObjectId));
    }

    /// <summary>
    /// A run dropped with the session sends nothing more and says nothing.
    /// </summary>
    [Fact]
    public void AResetRunStopsWithoutAWord()
    {
        var automation = new FakeAutomation();
        automation.Add(Item(80u, "Phantom"), ServerAnswer.Gone);
        var run = new ClearBuggedRun(new FakeHost(automation), automation.Messages.Add);

        run.Start();
        Drive(run, automation, seconds: 1d);
        run.Reset();
        int said = automation.Messages.Count;
        Drive(run, automation, seconds: 20d);

        Assert.Single(automation.Requests);
        Assert.Empty(automation.Forgotten);
        Assert.Equal(said, automation.Messages.Count);
        Assert.False(run.IsRunning);
    }

    /// <summary>
    /// The client lets go only when it is free; a busy client is asked again.
    /// Mutation: dropping the Busy retry never forgets the ghost.
    /// </summary>
    [Fact]
    public void ForgettingWaitsForAFreeClient()
    {
        var automation = new FakeAutomation();
        automation.Add(Item(90u, "Phantom"), ServerAnswer.Gone);
        var run = new ClearBuggedRun(new FakeHost(automation), automation.Messages.Add);

        run.Start();
        Drive(run, automation, seconds: 1d);
        automation.ItemsBusy = true;
        Drive(run, automation, seconds: 8d);
        Assert.Empty(automation.Forgotten);
        Assert.True(run.IsRunning);
        automation.ItemsBusy = false;
        Drive(run, automation, seconds: 1d);

        Assert.Equal([90u], automation.Forgotten);
    }

    /// <summary>
    /// A run that makes no progress at all -- the appraisal slot held by
    /// someone else for good, or the session out of the world -- ends on its
    /// own after half a minute with the reference's last line, rather than
    /// holding on until the session ends. Mutation: without the run-level
    /// clock the run is still going after a minute.
    /// </summary>
    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public void ARunThatMakesNoProgressEndsOnItsOwn(bool slotBusy)
    {
        var automation = new FakeAutomation { SlotBusy = slotBusy };
        automation.Add(Item(100u, "Phantom"), ServerAnswer.Gone);
        var run = new ClearBuggedRun(new FakeHost(automation), automation.Messages.Add);

        run.Start();
        automation.Available = slotBusy;
        Drive(run, automation, seconds: ClearBuggedRun.NoProgressTimeoutSeconds - 1d);
        Assert.True(run.IsRunning);
        Drive(run, automation, seconds: 30d);

        Assert.False(run.IsRunning);
        Assert.Empty(automation.Forgotten);
        Assert.Equal("[UB] InventoryManager: ClearBugged: Done!", automation.Messages[^1]);
    }

    private static void Drive(ClearBuggedRun run, FakeAutomation automation, double seconds)
    {
        int frames = (int)Math.Round(seconds / Frame);
        for (int frame = 0; frame < frames; frame++)
        {
            automation.Now += Frame;
            run.Tick(Frame);
            automation.Deliver();
        }
    }

    private static PluginInventoryItem Item(
        uint objectId,
        string name,
        uint container = FakeAutomation.OwnId,
        uint wielder = 0u,
        uint equippedLocation = 0u) => new(
            objectId, 0u, name, 0u, container, wielder, 0u, equippedLocation,
            0u, 0u, 0u, 1, 0, 0, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0d,
            0, 0, 0);

    private enum ServerAnswer
    {
        /// <summary>The item exists and is described every time.</summary>
        Described,

        /// <summary>The server has no such item and refuses every time.</summary>
        Gone,

        /// <summary>The item resists appraisal and is refused every time.</summary>
        Resists,

        /// <summary>Refused on the first request, described on every later one.</summary>
        RefusedOnce,
    }

    /// <summary>
    /// A client and a server in one: requests take the single appraisal slot,
    /// and <see cref="Deliver"/> brings back the server's answer the way the
    /// client reports it, bumping the slot's revision.
    /// </summary>
    private sealed class FakeAutomation
        : IAutomationSurface, ICharacterInfo, IItemAutomation,
          IWorldObjectAutomation, INavigationAutomation, ILootAutomation,
          IPluginChat
    {
        public const uint OwnId = 1u;

        private readonly Dictionary<uint, ServerAnswer> _answers = [];
        private readonly Dictionary<uint, int> _asked = [];
        private readonly HashSet<uint> _lastRefused = [];
        private PluginAppraisalState _slot;

        public double Now { get; set; }
        public List<PluginInventoryItem> Owned { get; } = [];
        public List<(uint ObjectId, double At)> Requests { get; } = [];
        public List<(uint ObjectId, double At)> Answers { get; } = [];
        public List<uint> Forgotten { get; } = [];
        public List<string> Messages { get; } = [];
        public bool SlotBusy { get; set; }
        public bool ItemsBusy { get; set; }
        public bool AnswerNothing { get; set; }

        public void Add(PluginInventoryItem item, ServerAnswer answer)
        {
            Owned.Add(item);
            _answers[item.ObjectId] = answer;
        }

        /// <summary>The client stops reporting a wait that ran out; nothing else changes.</summary>
        public void ExpireWait() => _slot = _slot with { AwaitingObjectId = 0u };

        public void Deliver()
        {
            uint id = _slot.AwaitingObjectId;
            if (id == 0u || AnswerNothing)
                return;
            int asked = _asked[id];
            bool described = _answers[id] switch
            {
                ServerAnswer.Described => true,
                ServerAnswer.RefusedOnce => asked > 1,
                _ => false,
            };
            if (described)
                _lastRefused.Remove(id);
            else
                _lastRefused.Add(id);
            _slot = new PluginAppraisalState(_slot.Revision + 1, 0u, id, 0u)
            {
                CurrentObjectUnsuccessful = !described,
            };
            Answers.Add((id, Now));
        }

        public bool Available { get; set; } = true;
        public bool IsAvailable => Available;
        public ICharacterInfo Character => this;
        public ISpellCatalog Spells => NoOpAutomationSurface.Instance;
        public IMagicCommands Magic => NoOpAutomationSurface.Instance;
        public IPluginChat Chat => this;
        public IItemAutomation Items => this;
        public IWorldObjectAutomation Objects => this;
        public INavigationAutomation Navigation => this;
        public ILootAutomation Loot => this;

        public bool IsInWorld => true;
        public string Name => "Tester";
        public uint ObjectId => OwnId;
        public uint CurrentHealth => 0u;
        public uint MaxHealth => 0u;
        public uint CurrentStamina => 0u;
        public uint MaxStamina => 0u;
        public uint CurrentMana => 0u;
        public uint MaxMana => 0u;
        public IReadOnlyList<PluginSkillInfo> Skills => [];
        public IReadOnlyList<PluginAttributeInfo> Attributes => [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments => [];
        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            skill = default;
            return false;
        }

        bool IItemAutomation.IsBusy => ItemsBusy;
        public IReadOnlyList<PluginInventoryItem> CaptureOwnedItems() => Owned.ToArray();

        public PluginItemCommandResult ForgetStaleItem(uint objectId)
        {
            if (ItemsBusy)
                return new(PluginItemCommandStatus.Busy);
            int index = Owned.FindIndex(item => item.ObjectId == objectId);
            if (index < 0)
                return new(PluginItemCommandStatus.InvalidItem);
            if (Owned[index].IsEquipped || !_lastRefused.Contains(objectId))
                return new(PluginItemCommandStatus.Refused, "not refused");
            Owned.RemoveAt(index);
            Forgotten.Add(objectId);
            return new(PluginItemCommandStatus.Completed);
        }

        public PluginItemCommandResult Identify(uint objectId)
        {
            if (SlotBusy)
                return new(PluginItemCommandStatus.Busy);
            Requests.Add((objectId, Now));
            _asked[objectId] = _asked.GetValueOrDefault(objectId) + 1;
            _slot = _slot with { Revision = _slot.Revision + 1, AwaitingObjectId = objectId };
            return new(PluginItemCommandStatus.Started);
        }

        PluginAppraisalState ILootAutomation.Appraisal => _slot;

        public bool TryGetObject(uint objectId, out PluginNavigationObject value)
        {
            value = default;
            return false;
        }

        public PluginNavigationCommandStatus SetMovementIntent(in PluginMovementIntent intent) =>
            PluginNavigationCommandStatus.Unavailable;

        public PluginNavigationCommandStatus ClearMovementIntent() =>
            PluginNavigationCommandStatus.Unavailable;

        public PluginNavigationSnapshot Snapshot => default;

        public void PostSystemMessage(string text) => Messages.Add(text);
    }

    private sealed class FakeHost(FakeAutomation automation) : IPluginHost
    {
        public bool HasUi => false;
        public IPluginLogger Log { get; } = new NullLog();
        public IGameState State { get; } = new NoState();
        public IEvents Events { get; } = new NoEvents();
        public ISelectionService Selection { get; } = new NoSelection();
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IPluginStorage Storage { get; } = new NoStorage();
        public IAutomationSurface Automation => automation;
        public IPluginStorage VtankProfiles => Storage;
    }

    private sealed class NoStorage : IPluginStorage
    {
        public bool IsAvailable => false;
        public string? ReadText(string key) => null;
        public void WriteText(string key, string content) { }
        public bool Delete(string key) => false;
    }

    private sealed class NullLog : IPluginLogger
    {
        public void Info(string message) { }
        public void Warn(string message) { }
        public void Error(string message, Exception? exception = null) { }
    }

    private sealed class NoState : IGameState
    {
        public IReadOnlyList<WorldEntitySnapshot> Entities => [];
    }

    private sealed class NoEvents : IEvents
    {
        public event Action<WorldEntitySnapshot> EntitySpawned
        {
            add { }
            remove { }
        }
        public event Action<double> Tick
        {
            add { }
            remove { }
        }
    }

    private sealed class NoSelection : ISelectionService
    {
        public uint? SelectedObjectId => null;
        public uint? PreviousObjectId => null;
        public event Action<SelectionChangedEvent> Changed
        {
            add { }
            remove { }
        }
        public bool Select(uint objectId) => false;
        public bool Clear() => false;
    }
}

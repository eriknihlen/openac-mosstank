using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class InventoryCountControllerTests
{
    [Fact]
    public void NameCountSumsStackSizesPerDistinctNameAndReportsATotal()
    {
        var automation = new FakeAutomation
        {
            ItemsValue =
            [
                Item(10u, "Prismatic Taper", stackSize: 50),
                Item(11u, "Prismatic Taper", stackSize: 25),
                Item(12u, "Pyreal Mote"),
                // Worn gear still belongs to the character, so a name count
                // reports it.
                Item(13u, "Prismatic Taper Pouch", equippedLocation: 8u),
            ],
        };
        InventoryCountController controller = Controller(automation);

        controller.ReportNameCount("prismatic taper");

        Assert.Equal(
            [
                "/t Tester, Counter: Item Count: Prismatic Taper - 75",
                "/t Tester, Counter: Item Count: Prismatic Taper Pouch - 1",
                "/t Tester, Counter: Total Item Count: 76",
            ],
            automation.Messages);
    }

    [Fact]
    public void NameCountReportsNothingFoundAgainstThePatternThatWasAsked()
    {
        var automation = new FakeAutomation
        {
            ItemsValue = [Item(10u, "Pyreal Mote")],
        };
        InventoryCountController controller = Controller(automation);

        controller.ReportNameCount("taper");

        Assert.Equal(
            ["/t Tester, Counter: Item Count: taper - 0", "/t Tester, Counter: Total Item Count: 0"],
            automation.Messages);
    }

    [Fact]
    public void NameCountRefusesAnUnusablePattern()
    {
        var automation = new FakeAutomation();
        InventoryCountController controller = Controller(automation);

        Assert.False(controller.TryCountByName(
            "(unclosed",
            out InventoryCountTally tally,
            out string error));

        Assert.Equal(0, tally.Total);
        Assert.StartsWith("bad name pattern:", error, StringComparison.Ordinal);
    }

    /// <summary>
    /// The character counts as one of the players standing there: the object
    /// table holds the character's own object on the landscape at a distance
    /// of zero, and the profile this plugin follows counts it. Mutation: skip
    /// the character's own object and an empty field answers zero, which
    /// shifts every threshold a meta is written against by one.
    /// </summary>
    [Fact]
    public void PlayerCountTakesOnlyLandscapePlayersInsideTheHorizontalRange()
    {
        var automation = new FakeAutomation
        {
            ObjectsValue =
            [
                // 24 m north: inside.
                LandscapePlayer(20u, "Near", northSouth: 0.1d),
                // 24 m north and far above: horizontal distance ignores the
                // climb, so this one is inside as well.
                LandscapePlayer(21u, "Upstairs", northSouth: 0.1d, elevation: 5d),
                // 240 m north: outside.
                LandscapePlayer(22u, "Far", northSouth: 1d),
                // The character itself stands in the object table too, and
                // is one of the players standing there.
                LandscapePlayer(1u, "Tester", northSouth: 0d),
                new PluginWorldObject(
                    23u, 0u, "Drudge", PluginObjectClass.Monster, 0u, 0u, 0u)
                {
                    IsLandscape = true,
                    HasPosition = true,
                    Position = PositionAt(0.1d, 0d),
                },
                // Carried, so not on the landscape at all.
                new PluginWorldObject(
                    24u, 0u, "Mule", PluginObjectClass.Player, 0u, 1u, 0u)
                {
                    HasPosition = true,
                    Position = PositionAt(0.1d, 0d),
                },
            ],
        };
        InventoryCountController controller = Controller(automation);

        controller.ReportPlayerCount(100d);

        Assert.Equal(["/t Tester, Counter: Player Count: 3"], automation.Messages);
    }

    [Fact]
    public void ProfileCountSkipsWornAndWieldedGearAndTalliesByRuleAndByName()
    {
        var automation = new FakeAutomation
        {
            ItemsValue =
            [
                Item(10u, "Trade Pyreal", stackSize: 5),
                Item(11u, "Trade Pyreal", stackSize: 3),
                Item(12u, "Trade Note"),
                Item(13u, "Trade Shield", equippedLocation: 8u),
                Item(14u, "Trade Sword", wielderObjectId: 1u),
                Item(15u, "Personal Note"),
            ],
        };
        // Everything is already appraised, so every rule can decide at once.
        automation.AppraiseAll();
        InventoryCountController controller = Controller(automation, out var profiles);
        SaveTradeProfile(profiles);

        Assert.True(controller.TryStartProfile("Counted", foreground: true));

        Assert.False(controller.IsRunning);
        Assert.Equal(
            [
                "[UB] Counter: Finished IDing Items",
                "/t Tester, Counter: Rule Count: Trade - 9",
                "/t Tester, Counter: Item Count: Trade Note - 1",
                "/t Tester, Counter: Item Count: Trade Pyreal - 8",
                "/t Tester, Counter: Total Item Count: 9",
            ],
            automation.Messages);
        Assert.Equal(9, controller.LastProfileTally?.Total);
    }

    [Fact]
    public void ProfileCountWaitsForAppraisalsAndOwnsTheCharacterWhileItDoes()
    {
        var automation = new FakeAutomation
        {
            ItemsValue = [Item(10u, "Trade Pyreal", stackSize: 5)],
            ObjectsValue = [Unappraised(10u, "Trade Pyreal")],
        };
        InventoryCountController controller = Controller(automation, out var profiles);
        SaveTradeProfile(profiles);

        Assert.True(controller.TryStartProfile("Counted", foreground: true));
        Assert.True(controller.IsRunning);
        Assert.Equal(["[UB] Counter: Items remaining to ID: 1"], automation.Messages);

        // Waiting on an appraisal, the count owns the character.
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([10u], automation.Identified);

        automation.AppraiseAll();
        Assert.False(controller.Tick(0.1d, canAct: true));
        Assert.False(controller.IsRunning);
        Assert.Equal(
            [
                "[UB] Counter: Items remaining to ID: 1",
                "[UB] Counter: Finished IDing Items",
                "/t Tester, Counter: Rule Count: Trade - 5",
                "/t Tester, Counter: Item Count: Trade Pyreal - 5",
                "/t Tester, Counter: Total Item Count: 5",
            ],
            automation.Messages);
    }

    [Fact]
    public void ProfileCountStandsDownWhileTheLooterHoldsTheAppraisalSlot()
    {
        var automation = new FakeAutomation
        {
            ItemsValue = [Item(10u, "Trade Pyreal", stackSize: 5)],
            ObjectsValue = [Unappraised(10u, "Trade Pyreal")],
        };
        var looterBusy = true;
        InventoryCountController controller = Controller(
            automation,
            out var profiles,
            () => looterBusy);
        SaveTradeProfile(profiles);
        Assert.True(controller.TryStartProfile("Counted", foreground: true));

        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Empty(automation.Identified);

        // The looter lets go, but the slot still carries somebody's question.
        looterBusy = false;
        automation.AppraisalState = new PluginAppraisalState(1L, 99u, 0u, 0u);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Empty(automation.Identified);

        automation.AppraisalState = default;
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([10u], automation.Identified);
    }

    [Fact]
    public void ProfileCountAskedForByAMacroNeverOwnsTheCharacterNorWrites()
    {
        var automation = new FakeAutomation
        {
            ItemsValue = [Item(10u, "Trade Pyreal", stackSize: 5)],
            ObjectsValue = [Unappraised(10u, "Trade Pyreal")],
        };
        InventoryCountController controller = Controller(automation, out var profiles);
        SaveTradeProfile(profiles);

        Assert.Equal(-1d, controller.ProfileCount("Counted"));
        Assert.True(controller.IsRunning);
        Assert.False(controller.Tick(0.1d, canAct: true));
        Assert.Equal([10u], automation.Identified);
        Assert.Empty(automation.Messages);

        automation.AppraiseAll();
        Assert.False(controller.Tick(0.1d, canAct: true));
        Assert.Empty(automation.Messages);
        Assert.Equal(5d, controller.ProfileCount("Counted"));
    }

    [Fact]
    public void ProfileCountSaysAProfileItCannotReadOnceRatherThanEveryCall()
    {
        var automation = new FakeAutomation();
        InventoryCountController controller = Controller(automation);

        Assert.Equal(-1d, controller.ProfileCount("Missing"));
        Assert.Equal(-1d, controller.ProfileCount("Missing"));

        string line = Assert.Single(automation.Messages);
        Assert.Contains("Missing", line, StringComparison.Ordinal);
    }

    [Fact]
    public void AWaitingProfileCountSaysHowMuchIsLeftEveryTenSeconds()
    {
        var automation = new FakeAutomation
        {
            ItemsValue = [Item(10u, "Trade Pyreal", stackSize: 5)],
            ObjectsValue = [Unappraised(10u, "Trade Pyreal")],
        };
        InventoryCountController controller = Controller(automation, out var profiles);
        SaveTradeProfile(profiles);
        Assert.True(controller.TryStartProfile("Counted", foreground: true));

        Assert.True(controller.Tick(9.5d, canAct: false));
        Assert.Single(automation.Messages);

        Assert.True(controller.Tick(1d, canAct: false));
        Assert.Equal(
            ["[UB] Counter: Items remaining to ID: 1", "[UB] Counter: Items remaining to ID: 1"],
            automation.Messages);
    }

    [Fact]
    public void ASecondProfileCountIsRefusedWhileOneIsStillWaiting()
    {
        var automation = new FakeAutomation
        {
            ItemsValue = [Item(10u, "Trade Pyreal", stackSize: 5)],
            ObjectsValue = [Unappraised(10u, "Trade Pyreal")],
        };
        InventoryCountController controller = Controller(automation, out var profiles);
        SaveTradeProfile(profiles);

        Assert.True(controller.TryStartProfile("Counted", foreground: true));
        Assert.False(controller.TryStartProfile("Counted", foreground: true));
        Assert.Contains(
            "already running",
            controller.Status,
            StringComparison.Ordinal);

        controller.Reset();
        Assert.False(controller.IsRunning);
        Assert.True(controller.TryStartProfile("Counted", foreground: true));
    }

    // ── nothing holds a count open for ever ───────────────────────────────

    /// <summary>
    /// An item the pack list names but the client's object table has no
    /// record of is passed over, not queued: nothing can appraise it and no
    /// rule can read it, so a queued one would never come off the list.
    /// Mutation: queue it instead, and the count below never finishes.
    /// </summary>
    [Fact]
    public void AProfileCountPassesOverAnItemTheClientHasNoRecordOf()
    {
        var automation = new FakeAutomation
        {
            ItemsValue =
            [
                Item(10u, "Trade Pyreal", stackSize: 5),
                // Owned, but the object table has never heard of it.
                Item(11u, "Trade Note"),
            ],
            ObjectsValue = [Appraised(10u, "Trade Pyreal")],
        };
        InventoryCountController controller = Controller(automation, out var profiles);
        SaveTradeProfile(profiles);

        Assert.True(controller.TryStartProfile("Counted", foreground: true));

        Assert.False(controller.IsRunning);
        Assert.Empty(automation.Identified);
        Assert.Equal(5, controller.LastProfileTally?.Total);
    }

    /// <summary>
    /// A request the client will not send says nothing about the items behind
    /// it, so the pass moves on and asks about the next one. Mutation: give
    /// up on the whole pass after the first answer, and the second item is
    /// never asked about at all.
    /// </summary>
    [Fact]
    public void ARefusedRequestDoesNotStopTheCountAskingAboutTheRest()
    {
        var automation = new FakeAutomation
        {
            ItemsValue =
            [
                Item(10u, "Trade Pyreal", stackSize: 5),
                Item(11u, "Trade Note"),
            ],
            ObjectsValue =
            [
                Unappraised(10u, "Trade Pyreal"),
                Unappraised(11u, "Trade Note"),
            ],
        };
        automation.IdentifyStatuses[10u] = PluginItemCommandStatus.Refused;
        InventoryCountController controller = Controller(automation, out var profiles);
        SaveTradeProfile(profiles);
        Assert.True(controller.TryStartProfile("Counted", foreground: true));

        Assert.True(controller.Tick(0.1d, canAct: true));

        Assert.Equal([10u, 11u], automation.Identified);
    }

    /// <summary>
    /// A refusal costs the item one of its attempts, the same as a request
    /// that went out and was never answered, so an item the client will never
    /// send a request for is given up on and decided from what the client
    /// already holds. Mutation: leave the refusal path uncounted, and the
    /// count asks about the same item for ever.
    /// </summary>
    [Fact]
    public void AnItemTheClientKeepsRefusingIsGivenUpOnAfterThreeAttempts()
    {
        var automation = new FakeAutomation
        {
            ItemsValue = [Item(10u, "Trade Pyreal", stackSize: 5)],
            ObjectsValue = [Unappraised(10u, "Trade Pyreal")],
            DefaultIdentifyStatus = PluginItemCommandStatus.Refused,
        };
        InventoryCountController controller = Controller(automation, out var profiles);
        SaveTradeProfile(profiles);
        Assert.True(controller.TryStartProfile("Counted", foreground: true));

        Assert.True(controller.Tick(0.6d, canAct: true));
        Assert.True(controller.Tick(0.6d, canAct: true));
        Assert.True(controller.Tick(0.6d, canAct: true));
        Assert.False(controller.Tick(0.6d, canAct: true));

        Assert.False(controller.IsRunning);
        Assert.Equal([10u, 10u, 10u], automation.Identified);
        Assert.Equal(5, controller.LastProfileTally?.Total);
    }

    /// <summary>
    /// A request that goes out and is never answered costs the item one of
    /// its attempts after the timeout, and three of those give up on it.
    /// Mutation: drop the timeout, or the attempt counter, and the count
    /// waits on the same silent item for ever.
    /// </summary>
    [Fact]
    public void AnItemThatIsNeverAnsweredIsGivenUpOnAfterThreeTimeouts()
    {
        var automation = new FakeAutomation
        {
            ItemsValue = [Item(10u, "Trade Pyreal", stackSize: 5)],
            ObjectsValue = [Unappraised(10u, "Trade Pyreal")],
        };
        InventoryCountController controller = Controller(automation, out var profiles);
        SaveTradeProfile(profiles);
        Assert.True(controller.TryStartProfile("Counted", foreground: true));

        // Each pass sends one request and waits out the ten-second timeout.
        Assert.True(controller.Tick(10.5d, canAct: true));
        Assert.True(controller.Tick(10.5d, canAct: true));
        Assert.True(controller.Tick(10.5d, canAct: true));
        Assert.False(controller.Tick(10.5d, canAct: true));

        Assert.False(controller.IsRunning);
        Assert.Equal([10u, 10u, 10u], automation.Identified);
        Assert.Equal(5, controller.LastProfileTally?.Total);
    }

    /// <summary>
    /// The client saying it has abandoned the question spends the attempt at
    /// once, so the count does not sit out a ten-second timeout for an answer
    /// that is already known not to be coming. Mutation: ignore the abandoned
    /// record, and the three attempts take thirty seconds instead of two.
    /// </summary>
    [Fact]
    public void AnAbandonedRequestSpendsItsAttemptWithoutWaitingOutTheTimeout()
    {
        var automation = new FakeAutomation
        {
            ItemsValue = [Item(10u, "Trade Pyreal", stackSize: 5)],
            ObjectsValue = [Unappraised(10u, "Trade Pyreal")],
        };
        InventoryCountController controller = Controller(automation, out var profiles);
        SaveTradeProfile(profiles);
        Assert.True(controller.TryStartProfile("Counted", foreground: true));

        // The client gives up on every question it is handed.
        automation.AppraisalState = new PluginAppraisalState(1L, 0u, 0u, 10u);
        Assert.True(controller.Tick(0.6d, canAct: true));
        Assert.True(controller.Tick(0.6d, canAct: true));
        Assert.True(controller.Tick(0.6d, canAct: true));
        Assert.False(controller.Tick(0.6d, canAct: true));

        Assert.False(controller.IsRunning);
        Assert.Equal([10u, 10u, 10u], automation.Identified);
        Assert.Equal(5, controller.LastProfileTally?.Total);
    }

    /// <summary>
    /// The outside limit. Every known stall now ends in the attempt ceiling,
    /// so this one is about the unforeseen: whatever keeps the outstanding
    /// set non-empty, a foreground count hands the character back and the
    /// counter is free for the next question. Mutation: drop the deadline,
    /// and the count below owns the character until the session ends.
    /// </summary>
    [Fact]
    public void AForegroundCountGivesTheCharacterBackWhenItRunsPastItsDeadline()
    {
        var automation = new FakeAutomation
        {
            ItemsValue = [Item(10u, "Trade Pyreal", stackSize: 5)],
            ObjectsValue = [Unappraised(10u, "Trade Pyreal")],
        };
        // The looter never lets the appraisal slot go, so the count never
        // gets to ask and nothing ever spends an attempt.
        InventoryCountController controller = Controller(
            automation,
            out var profiles,
            static () => true);
        SaveTradeProfile(profiles);
        Assert.True(controller.TryStartProfile("Counted", foreground: true));

        Assert.True(controller.Tick(59d, canAct: true));
        Assert.True(controller.Tick(59d, canAct: true));
        Assert.True(controller.Tick(59d, canAct: true));
        Assert.False(controller.Tick(59d, canAct: true));

        Assert.False(controller.IsRunning);
        Assert.Empty(automation.Identified);
        Assert.Contains("gave up", controller.Status, StringComparison.Ordinal);
        Assert.Contains(
            automation.Messages,
            line => line.Contains("gave up", StringComparison.Ordinal));
    }

    /// <summary>
    /// Cancelling names what was cancelled and leaves the counter idle, and
    /// cancelling nothing says so rather than pretending it stopped
    /// something.
    /// </summary>
    [Fact]
    public void CancelStopsAWaitingCountAndSaysWhatItStopped()
    {
        var automation = new FakeAutomation
        {
            ItemsValue = [Item(10u, "Trade Pyreal", stackSize: 5)],
            ObjectsValue = [Unappraised(10u, "Trade Pyreal")],
        };
        InventoryCountController controller = Controller(automation, out var profiles);
        SaveTradeProfile(profiles);
        Assert.Equal("Item counter is not running.", controller.Cancel());

        Assert.True(controller.TryStartProfile("Counted", foreground: true));
        Assert.True(controller.IsRunning);

        Assert.Equal("Item counter stopped: Counted.", controller.Cancel());
        Assert.False(controller.IsRunning);
        Assert.False(controller.Tick(0.1d, canAct: true));
    }

    // ── fixture ───────────────────────────────────────────────────────────

    private static InventoryCountController Controller(FakeAutomation automation) =>
        Controller(automation, out _);

    private static InventoryCountController Controller(
        FakeAutomation automation,
        out MossTankLootProfileStore profiles,
        Func<bool>? looterBusy = null)
    {
        var host = new FakeHost(automation, new MemoryStorage());
        profiles = new MossTankLootProfileStore(host);
        profiles.BindCharacter(automation.Name);
        return new InventoryCountController(
            host,
            profiles,
            looterBusy ?? (static () => false));
    }

    /// <summary>
    /// One named rule that keeps anything called "trade …", and a catch-all
    /// that keeps nothing, so a count has both a match and a miss to make.
    /// </summary>
    private static void SaveTradeProfile(MossTankLootProfileStore profiles)
    {
        Assert.True(profiles.Create("Counted", false, [], out _));
        profiles.SaveCurrent(
        [
            new LootRule
            {
                Name = "Trade",
                Expression = "name ~= trade",
                Action = LootAction.Keep,
            },
            new LootRule
            {
                Name = "Rest",
                Expression = "*",
                Action = LootAction.NoLoot,
            },
        ]);
    }

    private static PluginNavigationPosition PositionAt(
        double northSouth,
        double elevation = 0d) =>
        new(0x00010001u, 0d, northSouth, elevation, 0f, IsOutdoor: true);

    private static PluginWorldObject LandscapePlayer(
        uint objectId,
        string name,
        double northSouth,
        double elevation = 0d) =>
        new(objectId, 0u, name, PluginObjectClass.Player, 0u, 0u, 0u)
        {
            IsLandscape = true,
            HasPosition = true,
            Position = PositionAt(northSouth, elevation),
        };

    private static PluginWorldObject Unappraised(uint objectId, string name) =>
        new(objectId, 0u, name, PluginObjectClass.Misc, 0u, 1u, 0u)
        {
            IsOwned = true,
            HasAppraisalData = false,
        };

    private static PluginWorldObject Appraised(uint objectId, string name) =>
        new(objectId, 0u, name, PluginObjectClass.Misc, 0u, 1u, 0u)
        {
            IsOwned = true,
            HasAppraisalData = true,
        };

    private static PluginInventoryItem Item(
        uint objectId,
        string name,
        int stackSize = 1,
        uint equippedLocation = 0u,
        uint wielderObjectId = 0u) => new(
            objectId, 0u, name, 0u, 1u, wielderObjectId, 0u, equippedLocation,
            0u, 0u, 0u, stackSize, 0, 0, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0d,
            0, 0, 0);

    private sealed class FakeAutomation
        : IAutomationSurface, ICharacterInfo, IItemAutomation,
          IWorldObjectAutomation, INavigationAutomation, ILootAutomation,
          IPluginChat
    {
        public bool IsAvailable { get; set; } = true;
        public ICharacterInfo Character => this;
        public ISpellCatalog Spells => NoOpAutomationSurface.Instance;
        public IMagicCommands Magic => NoOpAutomationSurface.Instance;
        public IPluginChat Chat => this;
        public IItemAutomation Items => this;
        public IWorldObjectAutomation Objects => this;
        public INavigationAutomation Navigation => this;
        public ILootAutomation Loot => this;

        public bool IsInWorld => IsAvailable;
        public string Name => "Tester";
        public uint ObjectId => 1u;
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

        public IReadOnlyList<PluginInventoryItem> ItemsValue { get; set; } = [];
        public List<PluginWorldObject> ObjectsValue { get; set; } = [];
        public List<string> Messages { get; } = [];
        public List<uint> Identified { get; } = [];
        public PluginAppraisalState AppraisalState { get; set; }

        /// <summary>What the client answers a request about one item with.</summary>
        public Dictionary<uint, PluginItemCommandStatus> IdentifyStatuses { get; } = [];

        /// <summary>What it answers about every other item.</summary>
        public PluginItemCommandStatus DefaultIdentifyStatus { get; set; } =
            PluginItemCommandStatus.Started;

        /// <summary>Marks every known object as already described.</summary>
        public void AppraiseAll()
        {
            for (int index = 0; index < ObjectsValue.Count; index++)
                ObjectsValue[index] = ObjectsValue[index] with { HasAppraisalData = true };
            foreach (PluginInventoryItem item in ItemsValue)
            {
                if (ObjectsValue.Exists(known => known.ObjectId == item.ObjectId))
                    continue;
                ObjectsValue.Add(new PluginWorldObject(
                    item.ObjectId, 0u, item.Name, PluginObjectClass.Misc,
                    0u, 1u, 0u)
                {
                    IsOwned = true,
                    HasAppraisalData = true,
                });
            }
        }

        bool IItemAutomation.IsAvailable => true;
        bool IItemAutomation.IsBusy => false;
        bool IWorldObjectAutomation.IsAvailable => true;
        bool ILootAutomation.IsAvailable => true;

        public bool TryGetObject(uint objectId, out PluginNavigationObject value)
        {
            value = default;
            return false;
        }

        public PluginNavigationCommandStatus SetMovementIntent(
            in PluginMovementIntent intent) =>
            PluginNavigationCommandStatus.Unavailable;

        public PluginNavigationCommandStatus ClearMovementIntent() =>
            PluginNavigationCommandStatus.Unavailable;

        public PluginNavigationSnapshot Snapshot => new(
            IsAvailable: true,
            IsPortalSpace: false,
            LocalObjectId: 1u,
            Position: PositionAt(0d),
            IsMoving: false,
            IsAirborne: false);

        PluginAppraisalState ILootAutomation.Appraisal => AppraisalState;

        public IReadOnlyList<PluginInventoryItem> CaptureOwnedItems() => ItemsValue;
        public IReadOnlyList<PluginWorldObject> CaptureObjects() => ObjectsValue;

        public bool TryGet(uint objectId, out PluginWorldObject value)
        {
            foreach (PluginWorldObject candidate in ObjectsValue)
            {
                if (candidate.ObjectId != objectId)
                    continue;
                value = candidate;
                return true;
            }
            value = default;
            return false;
        }

        public bool TryCaptureProperties(
            uint objectId,
            out PluginItemProperties properties)
        {
            properties = new PluginItemProperties(
                new Dictionary<uint, int>(),
                new Dictionary<uint, long>(),
                new Dictionary<uint, bool>(),
                new Dictionary<uint, double>(),
                new Dictionary<uint, string>(),
                new Dictionary<uint, uint>(),
                new Dictionary<uint, uint>());
            return true;
        }

        public PluginItemCommandResult Identify(uint objectId)
        {
            Identified.Add(objectId);
            return new PluginItemCommandResult(
                IdentifyStatuses.TryGetValue(
                    objectId,
                    out PluginItemCommandStatus status)
                    ? status
                    : DefaultIdentifyStatus);
        }

        public void PostSystemMessage(string text) => Messages.Add(text);

        /// <summary>A line handed to the chat bar, kept beside the printed ones.</summary>
        public bool Submit(string text)
        {
            Messages.Add(text);
            return true;
        }
    }

    private sealed class FakeHost(
        FakeAutomation automation,
        IPluginStorage storage) : IPluginHost
    {
        public bool HasUi => false;
        public IPluginLogger Log { get; } = new FakeLogger();
        public IGameState State { get; } = new FakeState();
        public IEvents Events { get; } = new FakeEvents();
        public ISelectionService Selection { get; } = new FakeSelection();
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IPluginStorage Storage => storage;
        public IAutomationSurface Automation => automation;
        public IPluginStorage VtankProfiles => storage;
    }

    private sealed class MemoryStorage : IPluginStorage
    {
        private readonly Dictionary<string, string> _text =
            new(StringComparer.Ordinal);
        public bool IsAvailable => true;
        public string? ReadText(string key) =>
            _text.TryGetValue(key, out string? value) ? value : null;
        public void WriteText(string key, string content) => _text[key] = content;
        public bool Delete(string key) => _text.Remove(key);
    }

    private sealed class FakeLogger : IPluginLogger
    {
        public void Info(string message) { }
        public void Warn(string message) { }
        public void Error(string message, Exception? exception = null) { }
    }

    private sealed class FakeState : IGameState
    {
        public IReadOnlyList<WorldEntitySnapshot> Entities => [];
    }

    private sealed class FakeEvents : IEvents
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

    private sealed class FakeSelection : ISelectionService
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

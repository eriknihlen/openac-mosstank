using System.Reflection;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed partial class MossTankPanelTests
{
    [Fact]
    public void CorruptProfileIsPreservedAndReportedBeforeDefaultsLoad()
    {
        var storage = new MemoryStorage();
        storage.Text["profiles/index.json"] = "{ this is not json";

        var panel = new MossTankPanel(
            new FakeHost(new FakeAutomation(), storage));

        Assert.Contains(
            "Raw data was preserved",
            panel.ProfileLifecycleNotice,
            StringComparison.Ordinal);
        KeyValuePair<string, string> backup = Assert.Single(
            storage.Text,
            static pair => pair.Key.StartsWith(
                "recovery/macro/",
                StringComparison.Ordinal));
        Assert.Contains("profiles/index.json", backup.Value, StringComparison.Ordinal);
        Assert.Contains("{ this is not json", backup.Value, StringComparison.Ordinal);
    }

    [Fact]
    public void PanelFillsEveryPositionOfVtanksRuleList()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        IReadOnlyList<IMacroRule> rules = panel.MacroRules;

        Assert.Equal(66, rules.Count);
        Assert.All(rules, static rule => Assert.NotNull(rule));
        Assert.Equal(
            MacroRuleTable.Entries.Count(static entry => entry.Slot is null),
            rules.Count(static rule => rule is MacroRuleSentinel));
        Assert.Equal("IdlePeace", rules[^1].Name);
        Assert.Equal("Sentinel END", rules[^2].Name);
        Assert.All(
            rules.OfType<AbsentMacroRule>(),
            static rule => Assert.NotEmpty(rule.Reason));
    }

    /// <summary>
    /// The door rule has a body. It sat in the right place in the order with
    /// nothing behind it, so the door was only ever opened once a navigate turn
    /// came around — which, on the ordinary route, is after attacking and after
    /// both corpse rules.
    /// </summary>
    [Fact]
    public void TheDoorSlotHoldsALiveRule()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        int door = panel.MacroRules
            .ToList()
            .FindIndex(static rule => rule.Name == "OpenDoor");

        Assert.True(door >= 0, "the door slot is not filled at all.");
        Assert.IsType<OpenDoorRule>(panel.MacroRules[door]);
    }

    [Fact]
    public void WieldedManaRefillOutranksBuffSelf()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        IReadOnlyList<IMacroRule> rules = panel.MacroRules;

        int mana = rules.ToList().FindIndex(
            static rule => rule.Name == "RefillWieldedMana");
        int buff = rules.ToList().FindIndex(static rule => rule.Name == "BuffSelf");

        Assert.True(mana >= 0 && buff >= 0);
        Assert.True(
            mana < buff,
            $"RefillWieldedMana at {mana} must outrank BuffSelf at {buff}.");
    }

    [Fact]
    public void HelperRechargeSitsBelowBuffSelfAndSelfRechargeAboveIt()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        List<IMacroRule> rules = [.. panel.MacroRules];

        int self = rules.FindIndex(static rule => rule.Name == "RechargeSelfNormal");
        int buff = rules.FindIndex(static rule => rule.Name == "BuffSelf");
        int helper = rules.FindIndex(static rule => rule.Name == "UseHealersHeart");

        Assert.True(self >= 0 && buff >= 0 && helper >= 0);
        Assert.True(
            self < buff,
            $"RechargeSelfNormal at {self} must outrank BuffSelf at {buff}.");
        Assert.True(
            buff < helper,
            $"BuffSelf at {buff} must outrank UseHealersHeart at {helper}.");
    }

    [Fact]
    public void AStagedSelfRechargeIsNotWipedByTheHelperRowsLosingTick()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 10,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            // BoosterVital 2 = VitalKind.Health.
            ItemEntries = [Item(60, "Bread", 0x20) with { BoosterVital = 2 }],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(60u);
        panel.AddSelectedConsumable();

        panel.ToggleCombat();
        for (int tick = 0; tick < 6; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([60u], automation.UsedItemIds);
    }

    /// <summary>
    /// The item channel going free is a receipt, and the pass waiting behind
    /// it runs on that frame rather than at the next heartbeat. The loot
    /// rules hold the pass whenever an item request cannot go out — an open,
    /// a pull, a close — and the route stands still behind them, so a third
    /// of a second of that wait is paid on every corpse.
    ///
    /// Mutation: drop the item-channel flag from the frame's poke watch and
    /// the small frame below runs no pass.
    /// </summary>
    [Fact]
    public void TheFramePassPokesWhenTheItemChannelGoesFree()
    {
        var automation = new FakeAutomation { ItemsBusy = true };
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ToggleCombat();
        for (int tick = 0; tick < 4; tick++)
            panel.OnTick(0.3d);
        long settled = panel.MacroPassCount;

        // A frame well inside the heartbeat, on its own, is not a pass.
        panel.OnTick(0.01d);
        Assert.Equal(settled, panel.MacroPassCount);

        automation.ItemsBusy = false;
        panel.OnTick(0.01d);

        Assert.Equal(settled + 1, panel.MacroPassCount);
    }

    /// <summary>
    /// The slots a corpse open takes are given back on the host's frame, not
    /// on a rule pass — a rule pass cannot release the item slot, because that
    /// slot is what stops the loot rules running at all. So the frame has to
    /// carry the call, and this is the only test that says so.
    ///
    /// Mutation: delete `if (_loot.ObserveCorpseOpened()) _scheduler.Poke();`
    /// from the frame pass and the slot is still held after the container has
    /// opened.
    /// </summary>
    [Fact]
    public void TheFramePassGivesBackTheSlotsACorpseOpenTook()
    {
        var loot = new FrameLootSurface();
        var automation = new FakeAutomation { LootSurface = loot };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.AddLootRule();
        if (!panel.LootEnabled)
            panel.ToggleLooting();
        loot.Corpses =
        [
            new PluginLootContainer(
                FrameLootSurface.CorpseId,
                1u,
                "Corpse",
                3f,
                false,
                false,
                false)
            {
                IsIdentified = true,
                LongDescription = $"Killed by {automation.Name}.",
            },
        ];

        panel.ToggleCombat();
        for (int tick = 0; tick < 12 && loot.Opened == 0u; tick++)
            panel.OnTick(0.3d);

        Assert.Equal(FrameLootSurface.CorpseId, loot.Opened);
        Assert.True(panel.ActionLocks.IsLocked(ActionLockKind.ItemUse));
        Assert.True(panel.ActionLocks.IsLocked(ActionLockKind.Navigation));

        // The container is open now, which is the moment the slots go back.
        loot.Current = FrameLootSurface.CorpseId;
        panel.OnTick(0.05d);

        Assert.False(panel.ActionLocks.IsLocked(ActionLockKind.ItemUse));
        Assert.False(panel.ActionLocks.IsLocked(ActionLockKind.Navigation));
        Assert.False(
            panel.ActionLocks.IsLocked(ActionLockKind.CorpseOpenAttempt));
    }

    /// <summary>
    /// The vendor waypoint's own job ends the moment the window is up, so
    /// what keeps the route standing at the counter while the shop is worked
    /// is the vendor run's navigation lock, re-armed on every frame pass of
    /// the panel and let go when the run stops. Without it the route walks
    /// off to the next waypoint mid-visit.
    /// Mutation: drop `_locks?.Arm(ActionLockKind.Navigation, LockSeconds)`
    /// from VendorTradeController.Tick and the lock is clear on the pass
    /// that started the run; drop the release in Stop and it is still held
    /// after the counter closes.
    /// </summary>
    [Fact]
    public void ARunningVendorRunHoldsTheNavigationLockThroughTheFramePass()
    {
        var vendor = new FrameVendorSurface();
        var automation = new FakeAutomation { VendorSurface = vendor };
        var storage = new MemoryStorage();
        storage.WriteText(
            "mosstank/ub/autovendor/Shopkeeper.utl",
            MossTankLootProfileStore.SerializeRules(
            [
                new LootRule
                {
                    Name = "rest",
                    Expression = "*",
                    Action = LootAction.NoLoot,
                },
            ]));
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Command(panel, "opt set AutoVendor.Enabled true");

        vendor.Open(FrameVendorSurface.VendorId);
        panel.OnTick(0.05d);

        // The run took the character on the frame pass that started it, so
        // the route stands still instead of walking to the next waypoint.
        Assert.True(panel.ActionLocks.IsLocked(ActionLockKind.Navigation));

        vendor.Close();
        panel.OnTick(0.05d);

        Assert.False(panel.ActionLocks.IsLocked(ActionLockKind.Navigation));
    }


    /// <summary>
    /// The whole macro's navigation holds from the moment a vendor run starts
    /// until it stops. Live, on the route vnd -> pau 2s -> rcl, it did not:
    /// the window opened, the run reported "pays 90%", bought once and sold
    /// once, and MEANWHILE the route completed the vendor waypoint, ran the
    /// two-second pause out and cast the recall, after which the run's next
    /// buy failed with "Vendor transaction failed."
    ///
    /// The observed sequence, frame by frame: the route's rule claims the
    /// pass and ARMS the mover while it walks to the vendor; the window
    /// opens; the vendor run takes the character, which suspends the macro's
    /// rule pass for as long as it owns it; a suspended pass asks no rule
    /// anything and therefore takes nobody's turn away, so the route's rule
    /// stays "running" and its mover stays armed; and the panel's own frame
    /// driver keeps stepping that armed mover — a path that never looks at
    /// the navigation lock the run arms on every one of its ticks.
    ///
    /// Mutation: step the armed mover without asking whether the navigation
    /// slots are clear and the route reaches the recall inside the eight
    /// seconds below, exactly as it did live.
    /// </summary>
    [Fact]
    public void ARunningVendorRunHoldsTheRouteAtTheCounterUntilItStops()
    {
        var vendor = new FrameVendorSurface
        {
            Listings =
            [
                new PluginVendorItem(
                    9001u, 5000u, "Ration", PluginObjectClass.Food, 5, 1)
                {
                    MaxStackSize = 25,
                    ItemType = 0x20u,
                },
            ],
        };
        var automation = new FakeAutomation
        {
            VendorSurface = vendor,
            NavigationSnapshot = NavigationAt(0f),
            CurrentHealth = 100,
            MaxHealth = 100,
            ItemEntries = [ShopCoin(1000)],
        };
        automation.NavigationObjects.Add(new PluginNavigationObject(
            FrameVendorSurface.VendorId,
            "Shopkeeper",
            automation.NavigationSnapshot.Position));
        var storage = new MemoryStorage();
        storage.WriteText(
            "mosstank/ub/autovendor/Shopkeeper.utl",
            MossTankLootProfileStore.SerializeRules(
            [
                new LootRule
                {
                    Name = "rations",
                    Expression = "name ~= ration",
                    Action = LootAction.KeepUpTo,
                    KeepCount = 10,
                },
                new LootRule
                {
                    Name = "rest",
                    Expression = "*",
                    Action = LootAction.NoLoot,
                },
            ]));
        storage.Text["mosstank/navs/Shop.af"] = MetafSerializer.SaveNav(
            ShopRoute(automation.NavigationSnapshot.Position));

        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Command(panel, "opt set AutoVendor.Enabled true");
        Command(panel, "nav load Shop");
        panel.SetMetaOption("EnableNav", Truthy(true));
        panel.ToggleCombat();

        // The route is walking its vendor waypoint: its rule claims the pass
        // and arms the mover, which is the state the live route was in when
        // the window came up.
        for (int frame = 0; frame < 4; frame++)
            panel.OnTick(0.05d);
        Assert.Equal(0, panel.RouteWaypointIndexForTest);

        vendor.Open(FrameVendorSurface.VendorId);

        // Eight seconds of ordinary frames: four times the two-second pause,
        // so a route that is free to run reaches the recall many times over.
        for (int frame = 0; frame < 160; frame++)
        {
            panel.OnTick(0.05d);
            Assert.True(
                panel.ActionLocks.IsLocked(ActionLockKind.Navigation),
                "the run holds navigation for every frame it is running");
            Assert.True(
                panel.RouteWaypointIndexForTest <= 1,
                $"the route passed the pause at frame {frame}");
            Assert.Empty(automation.CastSpellIds);
        }

        // The buy is staged and waiting on the counter's answer, which is
        // where the live run was when the recall went out under it.
        Assert.NotEmpty(vendor.BuyAllCalls);

        // The counter answers; the run has nothing left to do and stops.
        automation.ItemEntries =
        [
            ShopCoin(950),
            ShopRation(40u, 10),
        ];
        vendor.Complete(PluginVendorTransactionKind.Buy, success: true);
        for (int frame = 0; frame < 20; frame++)
            panel.OnTick(0.05d);

        Assert.False(panel.ActionLocks.IsLocked(ActionLockKind.Navigation));

        // With the counter done the route picks up where it left off: the
        // pause runs out and the recall is cast.
        for (int frame = 0; frame < 120; frame++)
            panel.OnTick(0.05d);

        Assert.Contains(1635u, automation.CastSpellIds);
    }

    /// <summary>The live route: the vendor, a two-second pause, the recall.</summary>
    private static NavigationSettings ShopRoute(PluginNavigationPosition at)
    {
        var route = new NavigationSettings
        {
            Enabled = true,
            Mode = RouteMode.Circular,
        };
        route.Waypoints.Add(new RouteWaypoint
        {
            Type = RouteWaypointType.OpenVendor,
            Position = at,
            ObjectId = FrameVendorSurface.VendorId,
            ObjectName = "Shopkeeper",
        });
        route.Waypoints.Add(new RouteWaypoint
        {
            Type = RouteWaypointType.Pause,
            Position = at,
            DurationMilliseconds = 2000,
        });
        route.Waypoints.Add(new RouteWaypoint
        {
            Type = RouteWaypointType.Recall,
            Position = at,
            Recall = RouteRecallKind.LifestoneRecall,
            RecallSpellId = 1635u,
            RecallSpellName = "Lifestone Recall",
        });
        return route;
    }

    private static PluginInventoryItem ShopCoin(int amount) =>
        new PluginInventoryItem(
            30u, 2030u, "Pyreal", 0x10u, 1u, 0u, 0u, 0u, 0u, 0u, 0u,
            amount, 0, 0, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0d, 0, 0, 0)
        {
            Value = amount,
            WeenieClassId = 273u,
            ObjectClass = PluginObjectClass.Money,
            MaximumStackSize = 25000,
        };

    private static PluginInventoryItem ShopRation(uint id, int stack) =>
        new PluginInventoryItem(
            id, id + 2000u, "Ration", 0x20u, 1u, 0u, 0u, 0u, 0u, 0u, 0u,
            stack, 0, 0, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0d, 0, 0, 0)
        {
            Value = 5 * stack,
            MaximumStackSize = 25,
            ObjectClass = PluginObjectClass.Food,
        };
    /// <summary>A shop counter the frame pass can open and close.</summary>
    private sealed class FrameVendorSurface : IVendorAutomation
    {
        internal const uint VendorId = 0x7C95B01Cu;

        private uint _vendorObjectId;

        public bool IsAvailable => true;
        public bool IsOpen => _vendorObjectId != 0u;
        public uint VendorObjectId => _vendorObjectId;
        public string VendorName => IsOpen ? "Shopkeeper" : string.Empty;
        public List<PluginVendorItem> Listings { get; init; } = [];
        public IReadOnlyList<PluginVendorItem> Items => Listings;
        public bool IsBusy => false;
        public PluginVendorProfile Profile => new(
            0.75f,
            0x1u | 0x20u,
            PluginVendorProfile.NoValueLimit,
            PluginVendorProfile.NoValueLimit,
            true,
            0u,
            0u,
            null!);
        public IReadOnlyList<(uint, int)> BuyList => [];
        public IReadOnlyList<uint> SellList => [];

        public event Action<uint>? Opened;
        public event Action? Closed;
        public event Action<PluginVendorTransaction>? TransactionCompleted;

        public bool TryCaptureProperties(
            uint templateObjectId,
            out PluginItemProperties properties)
        {
            properties = default;
            return false;
        }

        public List<(uint TemplateObjectId, int Count)> Staged { get; } = [];
        public List<IReadOnlyList<(uint, int)>> BuyAllCalls { get; } = [];

        public PluginVendorCommandResult AddToBuyList(uint templateObjectId, int count)
        {
            Staged.Add((templateObjectId, count));
            return new(PluginVendorCommandStatus.Sent);
        }

        public PluginVendorCommandResult AddToSellList(uint itemObjectId) =>
            new(PluginVendorCommandStatus.Sent);
        public PluginVendorCommandResult RemoveFromBuyList(uint templateObjectId) =>
            new(PluginVendorCommandStatus.Sent);
        public PluginVendorCommandResult RemoveFromSellList(uint itemObjectId) =>
            new(PluginVendorCommandStatus.Sent);
        public PluginVendorCommandResult ClearBuyList()
        {
            Staged.Clear();
            return new(PluginVendorCommandStatus.Sent);
        }

        public PluginVendorCommandResult ClearSellList() =>
            new(PluginVendorCommandStatus.Sent);
        public PluginVendorCommandResult BuyAll()
        {
            if (Staged.Count == 0)
                return new(PluginVendorCommandStatus.InvalidItem);
            BuyAllCalls.Add([.. Staged]);
            Staged.Clear();
            return new(PluginVendorCommandStatus.Sent);
        }

        public PluginVendorCommandResult SellAll() =>
            new(PluginVendorCommandStatus.InvalidItem);

        internal void Open(uint vendorObjectId)
        {
            _vendorObjectId = vendorObjectId;
            Opened?.Invoke(vendorObjectId);
        }

        internal void Close()
        {
            _vendorObjectId = 0u;
            Closed?.Invoke();
        }

        /// <summary>The counter's answer to a staged buy or sell.</summary>
        internal void Complete(PluginVendorTransactionKind kind, bool success) =>
            TransactionCompleted?.Invoke(
                new PluginVendorTransaction(kind, success, null!));
    }

    /// <summary>
    /// The reference's corpse-id latch: with looting on, a corpse whose
    /// description has not arrived, within the approach range (five metres
    /// at least) plus ten, holds every walk off for the pass — the route
    /// included — and the loot channel says so. Once the corpse is
    /// described and turns out not to be ours, the walks are free again.
    /// Mutation: drop <c>!_navigationWaitsOnCorpseId</c> from
    /// <c>NavigationLocksAreClear</c> and the route wins the first pass.
    /// </summary>
    [Fact]
    public void AnUndescribedCorpseWithinReachHoldsEveryWalkOffForThePass()
    {
        var loot = new FrameLootSurface();
        var automation = new FakeAutomation
        {
            LootSurface = loot,
            NavigationSnapshot = new PluginNavigationSnapshot(
                true,
                false,
                1u,
                new PluginNavigationPosition(0x00010001u, 0d, 0d, 0d, 0f, IsOutdoor: true),
                false,
                false),
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.AddLootRule();
        if (!panel.LootEnabled)
            panel.ToggleLooting();
        // One waypoint the character is nowhere near, so the route rule has
        // something to do on every pass.
        automation.NavigationSnapshot = automation.NavigationSnapshot with
        {
            Position = new PluginNavigationPosition(
                0x00010001u, 0d, 1d, 0d, 0f, IsOutdoor: true),
        };
        panel.AddRoutePoint();
        automation.NavigationSnapshot = automation.NavigationSnapshot with
        {
            Position = new PluginNavigationPosition(
                0x00010001u, 0d, 0d, 0d, 0f, IsOutdoor: true),
        };
        panel.ToggleNavigation();
        loot.Corpses =
        [
            new PluginLootContainer(
                FrameLootSurface.CorpseId,
                1u,
                "Corpse",
                8f,
                false,
                false,
                false),
        ];
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "log ActiveRule on", "/vt log ActiveRule on"));
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "log Loot on", "/vt log Loot on"));
        automation.Messages.Clear();
        panel.ToggleCombat();

        panel.OnTick(0.3d);
        panel.OnTick(0.3d);

        Assert.Contains(
            "[MossTank] Holding this tick until the corpse id arrives.",
            automation.Messages);
        Assert.DoesNotContain(
            automation.Messages,
            line => line.Contains("Picked NavigateRouteIdle", StringComparison.Ordinal));
        Assert.DoesNotContain(
            automation.Messages,
            line => line.Contains("Picked NavigateCorpseIdle", StringComparison.Ordinal));

        // Described now, and somebody else's kill: nothing to loot, nothing
        // to wait for, and the route walks on.
        loot.Corpses =
        [
            loot.Corpses[0] with
            {
                IsIdentified = true,
                LongDescription = "Killed by Stranger.",
            },
        ];
        automation.Messages.Clear();
        panel.OnTick(0.3d);
        panel.OnTick(0.3d);

        Assert.Contains(
            automation.Messages,
            line => line.Contains("Picked NavigateRouteIdle", StringComparison.Ordinal));
        Assert.DoesNotContain(
            "[MossTank] Holding this tick until the corpse id arrives.",
            automation.Messages);
    }

    /// <summary>
    /// The reference's open rule is VALID while the item slot is held: the
    /// pass after the open is still the open rule's, with the slot up, and
    /// nothing below it runs. Mutation: put <c>ItemSlotIsFree()</c> back in
    /// front of the idle open row's gate and the second pass line is not the
    /// open rule's.
    /// </summary>
    [Fact]
    public void TheOpenRuleHoldsThePassWhileItsOwnSlotIsUp()
    {
        var loot = new FrameLootSurface();
        var automation = new FakeAutomation { LootSurface = loot };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.AddLootRule();
        if (!panel.LootEnabled)
            panel.ToggleLooting();
        loot.Corpses =
        [
            new PluginLootContainer(
                FrameLootSurface.CorpseId,
                1u,
                "Corpse",
                3f,
                false,
                false,
                false)
            {
                IsIdentified = true,
                LongDescription = $"Killed by {automation.Name}.",
            },
        ];
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "log ActiveRule on", "/vt log ActiveRule on"));
        panel.ToggleCombat();
        for (int tick = 0; tick < 12 && loot.Opened == 0u; tick++)
            panel.OnTick(0.3d);
        Assert.Equal(FrameLootSurface.CorpseId, loot.Opened);
        Assert.True(panel.ActionLocks.IsLocked(ActionLockKind.ItemUse));

        // The container has not opened; the slot is still up.
        automation.Messages.Clear();
        panel.OnTick(0.3d);

        Assert.Contains(
            automation.Messages,
            line => line.Contains("Picked OpenCorpseIdle", StringComparison.Ordinal)
                && line.Contains("I=True", StringComparison.Ordinal));
    }

    /// <summary>
    /// Emptying a corpse is a chain of small waits, and a pass that ends in
    /// "still waiting" costs the rest of a heartbeat unless the answer it
    /// was waiting for wakes the pass itself. The container opening and
    /// shutting, and its contents arriving, are three such answers: each
    /// runs the next pass on the frame it lands. Mutation: drop the
    /// container/contents fields from the poke watch and each of these ticks
    /// runs no pass at all.
    /// </summary>
    [Fact]
    public void ACorpseReceiptRunsTheNextPassOnItsOwnFrame()
    {
        var loot = new FrameLootSurface();
        var automation = new FakeAutomation { LootSurface = loot };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.AddLootRule();
        if (!panel.LootEnabled)
            panel.ToggleLooting();
        panel.ToggleCombat();
        // Settle: nothing is changing, so nothing pokes and the pass runs on
        // the heartbeat alone.
        for (int tick = 0; tick < 4; tick++)
            panel.OnTick(0.3d);
        long settled = panel.MacroPassCount;
        panel.OnTick(0.01d);
        Assert.Equal(settled, panel.MacroPassCount);

        // The container opened.
        loot.Current = FrameLootSurface.CorpseId;
        panel.OnTick(0.01d);
        Assert.Equal(settled + 1, panel.MacroPassCount);

        // Its contents arrived.
        loot.ContentsReady = false;
        panel.OnTick(0.3d);
        long beforeContents = panel.MacroPassCount;
        loot.ContentsReady = true;
        panel.OnTick(0.01d);
        Assert.Equal(beforeContents + 1, panel.MacroPassCount);

        // The container shut.
        panel.OnTick(0.3d);
        long beforeClose = panel.MacroPassCount;
        loot.Current = 0u;
        panel.OnTick(0.01d);
        Assert.Equal(beforeClose + 1, panel.MacroPassCount);
    }

    private sealed class FrameLootSurface : ILootAutomation
    {
        internal const uint CorpseId = 0x70000D01u;

        public uint Opened { get; private set; }
        public uint Current { get; set; }
        public bool ContentsReady { get; set; } = true;
        public IReadOnlyList<PluginLootContainer> Corpses { get; set; } = [];

        public bool IsAvailable => true;
        public bool IsBusy => false;
        public uint RequestedContainerId => Opened;
        public uint CurrentContainerId => Current;
        public bool CurrentContentsReady => ContentsReady;

        public IReadOnlyList<PluginLootContainer> CaptureCorpses(
            float maximumDistance) => Corpses;

        public PluginItemCommandResult Open(uint containerObjectId)
        {
            Opened = containerObjectId;
            return new(PluginItemCommandStatus.Started);
        }
    }

    [Fact]
    public void CorruptSideCarMonsterRuleIsLoggedNotSilentlySwallowed()
    {
        var storage = new MemoryStorage();
        string usdKey = SettingsKey(
            VtankProfileDirectory.AutoCharacterFileName("Barris", string.Empty, "usd"));
        storage.Text[usdKey] = VtankDefaultSettingsDatabase.Create().Render();
        storage.Text["profiles/macro/sidecar/--Barris_.usd.json"] = """
            {
              "CombatRules": [
                { "Expression": "(((" },
                { "Expression": "DEFAULT" }
              ]
            }
            """;

        var host = new FakeHost(new FakeAutomation { Name = "Barris" }, storage);
        _ = new MossTankPanel(host);

        Assert.Contains(host.Logger.Warnings, message =>
            message.Contains("monster rule", StringComparison.OrdinalIgnoreCase));
    }

    private static string LegacyByCharacterProfileKey(string characterName)
    {
        string identity = "char:" + characterName.Trim().ToUpperInvariant();
        string hash = Convert.ToHexString(
            System.Security.Cryptography.SHA256.HashData(
                System.Text.Encoding.UTF8.GetBytes(identity)));
        return $"profiles/macro/{hash}.json";
    }

    /// <summary>
    /// Following the route holds a walk the client plans, except the
    /// route's own hand-off: holding that one left the character standing
    /// until the meta's watchdog restarted the route. Mutation: ignore the
    /// route's own walk and the route holds it again.
    /// </summary>
    [Theory]
    [InlineData(true, false, true)]
    [InlineData(true, true, false)]
    [InlineData(false, false, false)]
    public void TheRouteNeverHoldsItsOwnClientWalk(bool routeEnabled, bool routeOwnsWalk, bool holds) =>
        Assert.Equal(holds, MossTankPanel.RouteHoldsClientWalks(routeEnabled, routeOwnsWalk));

    [Theory]
    [InlineData(false, "Attack", false, false, false, 99d, null)]
    [InlineData(true, null, false, false, false, 99d, null)]
    [InlineData(true, "IdlePeace", false, false, false, 99d, null)]
    [InlineData(true, "RandomHelper", false, false, false, 99d, null)]
    [InlineData(true, "NavigateRouteIdle", false, false, false, 99d, null)]
    [InlineData(true, "Attack", false, false, false, 99d, "MossTank is running Attack")]
    [InlineData(true, "BuffSelf", false, false, false, 99d, "MossTank is running BuffSelf")]
    [InlineData(true, "NavigateRouteIdle", false, true, false, 99d, "MossTank is following its route")]
    [InlineData(true, "NavigateRouteIdle", false, false, true, 1d, "MossTank is waiting for a corpse to loot")]
    [InlineData(true, "NavigateRouteIdle", false, false, false, 1d, null)]
    [InlineData(true, "NavigateRouteIdle", false, false, true, 3d, null)]
    [InlineData(false, null, true, false, false, 99d, "MossTank is buffing")]
    public void AWalkTheClientPlansWaitsWhileTheMacroHasSomethingToDo(
        bool running,
        string? lastRule,
        bool buffing,
        bool routeNavigation,
        bool looting,
        double secondsSinceAttack,
        string? expected)
    {
        Assert.Equal(
            expected,
            MossTankPanel.WalkPauseReasonFor(running, lastRule, buffing, routeNavigation, looting, secondsSinceAttack));
    }

    /// <summary>
    /// The Client pathing choice lives in the macro profile's side-car and
    /// comes back on reload; a side-car written by the older "walk legs with
    /// client pathing" checkbox reads as Always, and one that says nothing
    /// reads as the default, When stuck.
    /// </summary>
    [Fact]
    public void ClientPathingIsSavedWithTheProfileAndTheOldCheckboxReadsAsAlways()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Barris" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Assert.Equal("When stuck", panel.SelectedClientPathing);

        panel.SelectClientPathing("Always");

        Assert.Equal("Always", panel.SelectedClientPathing);
        var reloaded = new MossTankPanel(new FakeHost(new FakeAutomation { Name = "Barris" }, storage));
        Assert.Equal("Always", reloaded.SelectedClientPathing);

        string key = Assert.Single(storage.Text.Keys, static k => k.Contains("sidecar", StringComparison.Ordinal));
        string written = storage.Text[key];
        Assert.Contains("NavigationClientPathing", written, StringComparison.Ordinal);

        storage.Text[key] = System.Text.RegularExpressions.Regex.Replace(
            written,
            "\"NavigationClientPathing\"\\s*:\\s*\"Always\"",
            "\"NavigationWalkLegsWithClient\": true");
        var legacy = new MossTankPanel(new FakeHost(new FakeAutomation { Name = "Barris" }, storage));
        Assert.Equal("Always", legacy.SelectedClientPathing);

        storage.Text[key] = System.Text.RegularExpressions.Regex.Replace(
            written,
            "\"NavigationClientPathing\"\\s*:\\s*\"Always\",?",
            string.Empty);
        var silent = new MossTankPanel(new FakeHost(new FakeAutomation { Name = "Barris" }, storage));
        Assert.Equal("When stuck", silent.SelectedClientPathing);
    }

    [Fact]
    public void FirstLoadMigratesLegacyJsonMacroProfileToUsdAndDeletesTheJsonKey()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Barris" };
        string legacyKey = LegacyByCharacterProfileKey("Barris");
        storage.Text[legacyKey] = """
            {
              "Combat": { "MaximumRange": 48.0 },
              "ItemNames": ["Wand of Testing"]
            }
            """;

        var panel = new MossTankPanel(new FakeHost(automation, storage));

        Assert.False(storage.Text.ContainsKey(legacyKey));
        string usdKey = SettingsKey(
            VtankProfileDirectory.AutoCharacterFileName("Barris", string.Empty, "usd"));
        Assert.True(storage.Text.ContainsKey(usdKey));
        Assert.Equal(0.2d, panel.EvaluateExpression("uboptget[`AttackDistance`]").AsNumber(), precision: 7);
        Assert.Contains("Wand of Testing", panel.ItemProfileText, StringComparison.Ordinal);

        var reloaded = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Barris" }, storage));
        Assert.Equal(0.2d, reloaded.EvaluateExpression("uboptget[`AttackDistance`]").AsNumber(), precision: 7);
        Assert.Contains("Wand of Testing", reloaded.ItemProfileText, StringComparison.Ordinal);
    }

    [Fact]
    public void ExistingUsdCounterpartLeavesLegacyJsonUntouchedAndUnread()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Barris" };
        string legacyKey = LegacyByCharacterProfileKey("Barris");
        storage.Text[legacyKey] = """{ "Combat": { "MaximumRange": 240.0 } }""";
        string usdKey = SettingsKey(
            VtankProfileDirectory.AutoCharacterFileName("Barris", string.Empty, "usd"));
        var seedCombat = new CombatSettings { MaximumRange = 120d };
        VtankDatabase seedDatabase = VtankSettingsProfileSerializer.CreateNew(
            new VtankSettingsProfileSerializer.AllSettings
            {
                Combat = seedCombat,
                Buffs = new BuffSettings(),
                Vitals = new VitalSettings(),
                Inventory = new InventorySettings(),
                Navigation = new NavigationSettings(),
            });
        storage.Text[usdKey] = seedDatabase.Render();

        var panel = new MossTankPanel(new FakeHost(automation, storage));

        Assert.True(storage.Text.ContainsKey(legacyKey));
        Assert.Equal(0.5d, panel.EvaluateExpression("uboptget[`AttackDistance`]").AsNumber(), precision: 7);
    }

    [Fact]
    public void DropInUsdWithLootingEnabledAndNoSideCarLoadsLootingEnabled()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Barris" };
        string usdKey = SettingsKey(
            VtankProfileDirectory.AutoCharacterFileName("Barris", string.Empty, "usd"));

        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        VtankTable settingsTable = database.Find("Settings")!;
        int nameColumn = settingsTable.ColumnIndex("Setting");
        int valueColumn = settingsTable.ColumnIndex("Value");
        VtankRow row = settingsTable.Rows.First(candidate =>
            candidate.Cells[nameColumn].AsString().Equals(
                "EnableLooting", StringComparison.OrdinalIgnoreCase));
        row.Cells[valueColumn] = VtankCell.Bool(true);
        storage.Text[usdKey] = database.Render();

        // No side-car key at all — a real drop-in, not something MossTank
        // itself ever saved.
        var panel = new MossTankPanel(new FakeHost(automation, storage));

        Assert.True(panel.LootEnabled);
    }

    private static string LegacyNamedProfileKey(string name)
    {
        string identity = "named:" + name.Trim().ToUpperInvariant();
        string hash = Convert.ToHexString(
            System.Security.Cryptography.SHA256.HashData(
                System.Text.Encoding.UTF8.GetBytes(identity)));
        return $"profiles/macro/{hash}.json";
    }

    [Fact]
    public void SettingsRosterSweepConvertsEveryNamedLegacyProfileOnce()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Barris" };
        storage.Text["profiles/index.json"] = """
            {
              "Version": 1,
              "MineOnly": false,
              "Profiles": [
                { "Name": "Farming", "Owner": "Barris" },
                { "Name": "Buffing", "Owner": "Barris" }
              ],
              "SelectedByCharacter": {}
            }
            """;
        storage.Text[LegacyNamedProfileKey("Farming")] =
            """{ "Combat": { "MaximumRange": 42.0 } }""";
        storage.Text[LegacyNamedProfileKey("Buffing")] =
            """{ "Combat": { "MaximumRange": 24.0 } }""";

        var panel = new MossTankPanel(new FakeHost(automation, storage));

        // MineOnly recovered from the old roster's own shape...
        Assert.False(panel.MineOnlyEnabled);
        string farmingUsd = SettingsKey(
            VtankProfileDirectory.SubProfilePrefix("Barris", string.Empty)
            + "Farming.usd");
        string buffingUsd = SettingsKey(
            VtankProfileDirectory.SubProfilePrefix("Barris", string.Empty)
            + "Buffing.usd");
        Assert.Equal(42d, RangeOf(storage, farmingUsd));
        Assert.Equal(24d, RangeOf(storage, buffingUsd));

        // Both legacy JSON keys and the whole (now fully-swept) roster key
        // are gone; MineOnly now lives at its own dedicated key.
        Assert.False(storage.Text.ContainsKey(LegacyNamedProfileKey("Farming")));
        Assert.False(storage.Text.ContainsKey(LegacyNamedProfileKey("Buffing")));
        Assert.False(storage.Text.ContainsKey("profiles/index.json"));
        Assert.True(storage.Text.ContainsKey("profiles/macro/preferences.json"));

        // Idempotent: a fresh panel against the same storage sweeps nothing
        // more (there is no roster key left to read) and keeps both values.
        var reloaded = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Barris" }, storage));
        Assert.False(reloaded.MineOnlyEnabled);
        Assert.Equal(42d, RangeOf(storage, farmingUsd));
        Assert.Equal(24d, RangeOf(storage, buffingUsd));
    }

    private static double RangeOf(MemoryStorage storage, string usdKey)
    {
        string text = Assert.Contains(usdKey, (IDictionary<string, string>)storage.Text);
        var settings = new VtankSettingsProfileSerializer.AllSettings
        {
            Combat = new CombatSettings(),
            Buffs = new BuffSettings(),
            Vitals = new VitalSettings(),
            Inventory = new InventorySettings(),
            Navigation = new NavigationSettings(),
        };
        VtankSettingsProfileSerializer.Load(text, settings);
        return settings.Combat.MaximumRange;
    }

    private static string LegacyMetaByCharacterKey(string characterName)
    {
        byte[] hash = System.Security.Cryptography.SHA256.HashData(
            System.Text.Encoding.UTF8.GetBytes(characterName.ToLowerInvariant()));
        return $"profiles/meta/by-character/{Convert.ToHexString(hash.AsSpan(0, 12)).ToLowerInvariant()}.json";
    }

    [Fact]
    public void MetaStoreMigratesLegacyJsonProfileToAfAndDeletesTheJsonKey()
    {
        var storage = new MemoryStorage();
        string legacyKey = LegacyMetaByCharacterKey("Barris");
        var legacyProfile = new MetaProfile
        {
            Rules =
            [
                new MetaRule
                {
                    State = "Default",
                    Condition = MetaCondition.Always(),
                    Action = new MetaAction { Kind = MetaActionKind.ChatCommand, Text = "/say hi" },
                    Enabled = true,
                },
            ],
        };
        storage.Text[legacyKey] = System.Text.Json.JsonSerializer.Serialize(
            legacyProfile,
            new System.Text.Json.JsonSerializerOptions { WriteIndented = true });

        var store = new MossTankMetaProfileStore(
            new FakeHost(new FakeAutomation { Name = "Barris" }, storage));
        store.BindCharacter("Barris");
        MetaProfile loaded = store.LoadCurrent();

        Assert.False(storage.Text.ContainsKey(legacyKey));
        MetaRule rule = Assert.Single(loaded.Rules);
        Assert.Equal("/say hi", rule.Action.Text);

        // Idempotent second run: nothing left to migrate, loads straight
        // from the now-real .af file.
        var reopened = new MossTankMetaProfileStore(
            new FakeHost(new FakeAutomation { Name = "Barris" }, storage));
        reopened.BindCharacter("Barris");
        MetaProfile reloaded = reopened.LoadCurrent();
        Assert.Single(reloaded.Rules);
    }

    [Fact]
    public void MetaStoreLeavesLegacyJsonUntouchedWhenAfCounterpartExists()
    {
        var storage = new MemoryStorage();
        string legacyKey = LegacyMetaByCharacterKey("Barris");
        storage.Text[legacyKey] = System.Text.Json.JsonSerializer.Serialize(new MetaProfile
        {
            Rules = [new MetaRule { Action = new MetaAction { Kind = MetaActionKind.ChatCommand, Text = "/say stale" } }],
        });
        string realKey = "mosstank/metas/" + VtankProfileDirectory.AutoCharacterFileName("Barris", string.Empty, "af");
        storage.Text[realKey] = MetafSerializer.SaveMeta(new MetaProfile
        {
            Rules = [new MetaRule { Action = new MetaAction { Kind = MetaActionKind.ChatCommand, Text = "/say real" } }],
        });

        var store = new MossTankMetaProfileStore(
            new FakeHost(new FakeAutomation { Name = "Barris" }, storage));
        store.BindCharacter("Barris");
        MetaProfile loaded = store.LoadCurrent();

        Assert.True(storage.Text.ContainsKey(legacyKey));
        Assert.Equal("/say real", Assert.Single(loaded.Rules).Action.Text);
    }



    [Fact]
    public void EveryProfileLoadSaysWhichFileAndWhichFullPathItCameFrom()
    {
        var storage = new MemoryStorage { RootPath = Path.Combine("data", "vtank") };
        storage.Text["mosstank/navs/Hunt.af"] =
            MetafSerializer.SaveNav(new NavigationSettings());
        storage.Text[SettingsKey("Shared.usd")] =
            VtankDefaultSettingsDatabase.Create().Render();
        var automation = new FakeAutomation { Name = "Barris", WorldName = "Coldeve" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        panel.OnTick(0.1d);
        automation.Messages.Clear();

        Command(panel, "nav load Hunt");
        panel.OnTick(0.1d);

        string expectedPath = Path.Combine(
            "data",
            "vtank",
            "mosstank",
            "navs",
            "Hunt.af");
        Assert.Contains(
            automation.Messages,
            message => message.Contains("navigation profile Hunt.af", StringComparison.Ordinal)
                && message.Contains(expectedPath, StringComparison.Ordinal));

        automation.Messages.Clear();
        Command(panel, "settings load Shared");
        panel.OnTick(0.1d);

        Assert.Contains(
            automation.Messages,
            message => message.Contains("settings profile Shared.usd", StringComparison.Ordinal)
                && message.Contains(
                    Path.Combine("data", "vtank", "mosstank", "profiles", "Shared.usd"),
                    StringComparison.Ordinal));
    }

    [Theory]
    [InlineData("nav load Dropped")]
    [InlineData("nav load Dropped.nav")]
    public void ADroppedRouteLoadsByCommandWithOrWithoutItsExtension(string arguments)
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/navs/Dropped.nav"] = File.ReadAllText(Path.Combine(
            AppContext.BaseDirectory, "Fixtures", "vtank", "nav", "nav_ab.nav"));
        var automation = new FakeAutomation { Name = "Barris", WorldName = "Coldeve" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));

        Command(panel, arguments);

        Assert.Equal("Dropped.nav", panel.SelectedRouteProfile);
        Assert.NotEmpty(panel.RouteRows);
    }

    /// <summary>
    /// A route a meta carries inside itself shows by its name in the route
    /// selector and is never written to the route file that was selected
    /// before it; loading that file again gets the author's route back.
    /// Mutation: save the embedded route to the selected profile and the
    /// circuit comes back as the one-point embedded route.
    /// </summary>
    [Fact]
    public void AnEmbeddedRouteShowsByNameAndNeverOverwritesTheSelectedRouteFile()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/navs/Circuit.nav"] = File.ReadAllText(Path.Combine(
            AppContext.BaseDirectory, "Fixtures", "vtank", "nav", "nav_ab.nav"));
        storage.Text["mosstank/metas/Stipend.af"] =
            "STATE: {Default} ~~ {\r\n"
            + "\tIF:\tAlways\r\n"
            + "\t\tDO:\tDoAll\r\n"
            + "\t\t\t\tEmbedNav nav0__stipend_nav {stipend.nav}\r\n"
            + "\t\t\t\tSetState {Walk}\r\n"
            + "STATE: {Walk} ~~ {\r\n"
            + "\tIF:\tNever\r\n"
            + "\t\tDO:\tNone\r\n"
            + "NAV: nav0__stipend_nav once ~~ {\r\n"
            + "\tpnt 47.1 26.1 0.2\r\n";
        var panel = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Barris", WorldName = "Coldeve" }, storage));
        Command(panel, "nav load Circuit");
        int circuitPoints = panel.RouteRows.Count;
        string circuitName = panel.SelectedRouteProfile;
        Assert.True(circuitPoints > 1);

        Command(panel, "meta load Stipend");
        panel.ToggleMeta();
        panel.ToggleCombat();
        var before = new Dictionary<string, string>(storage.Text);
        panel.OnTick(0.3d);

        Assert.Equal("Walk", panel.MetaState);
        Assert.Equal("stipend.nav (embedded)", panel.SelectedRouteProfile);
        Assert.Contains("stipend.nav (embedded)", panel.RouteProfileNames);
        Assert.Single(panel.RouteRows);
        foreach ((string key, string text) in storage.Text)
        {
            if (key.StartsWith("mosstank/navs/", StringComparison.Ordinal))
                Assert.Equal(before.GetValueOrDefault(key), text);
        }

        Command(panel, "nav load Circuit");

        Assert.Equal(circuitName, panel.SelectedRouteProfile);
        Assert.DoesNotContain("stipend.nav (embedded)", panel.RouteProfileNames);
        Assert.Equal(circuitPoints, panel.RouteRows.Count);
    }

    /// <summary>
    /// Setting an option writes the settings profile and nothing else: a meta
    /// and a route edited on disk while they are loaded keep the edit. The
    /// meta here sets an option itself, as metas do all the time. Nor is a
    /// meta written by any other settings change, or on switching to another.
    /// Mutation: save the whole profile set on an option change, save the
    /// meta with the other settings, or save it before a switch, and the
    /// edit is written over.
    /// </summary>
    [Fact]
    public void SettingAnOptionLeavesTheLoadedMetaAndRouteFilesAlone()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/navs/Circuit.nav"] = File.ReadAllText(Path.Combine(
            AppContext.BaseDirectory, "Fixtures", "vtank", "nav", "nav_ab.nav"));
        storage.Text["mosstank/metas/Edited.af"] =
            "STATE: {Default} ~~ {\r\n"
            + "\tIF:\tAlways\r\n"
            + "\t\tDO:\tChat {/vt opt set enablebuffing true}\r\n";
        var panel = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Barris", WorldName = "Coldeve" }, storage));
        Command(panel, "nav load Circuit");
        Command(panel, "meta load Edited");
        const string editedMeta =
            "STATE: {Default} ~~ {\r\n"
            + "\tIF:\tAlways\r\n"
            + "\t\tDO:\tChat {/vt opt set enablelooting true}\r\n";
        const string editedRoute = "edited on disk";
        storage.Text["mosstank/metas/Edited.af"] = editedMeta;
        storage.Text["mosstank/navs/Circuit.nav"] = editedRoute;

        Command(panel, "opt set enablebuffing false");
        Command(panel, "opt toggle enablelooting");

        Assert.Equal(editedMeta, storage.Text["mosstank/metas/Edited.af"]);
        Assert.Equal(editedRoute, storage.Text["mosstank/navs/Circuit.nav"]);

        Command(panel, "setattackbar 0.5");
        storage.Text["mosstank/metas/Other.af"] =
            "STATE: {Default} ~~ {\r\n"
            + "\tIF:\tNever\r\n"
            + "\t\tDO:\tNone\r\n";
        Command(panel, "meta load Other");

        Assert.Equal(editedMeta, storage.Text["mosstank/metas/Edited.af"]);
    }

    /// <summary>
    /// The meta interval is MossTank's own preference: the command sets it,
    /// the engine uses it at once, and a new session reads it back. It is not
    /// a VTank option, so it never lands in a settings profile.
    /// Mutation: skip saving the preference and the second panel is back at
    /// the reference pace.
    /// </summary>
    [Fact]
    public void TheMetaIntervalIsSavedAndReadBackAsMossTanksOwnPreference()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Barris", WorldName = "Coldeve" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Assert.Equal(293, panel.MetaIntervalMillisecondsForTest);

        Command(panel, "metainterval 100");

        Assert.Equal(100, panel.MetaIntervalMillisecondsForTest);
        var again = new MossTankPanel(new FakeHost(automation, storage));
        Assert.Equal(100, again.MetaIntervalMillisecondsForTest);
        Assert.DoesNotContain(
            storage.Text,
            pair => pair.Key.EndsWith(".usd", StringComparison.OrdinalIgnoreCase)
                && pair.Value.Contains("MetaInterval", StringComparison.OrdinalIgnoreCase));
    }

    /// <summary>
    /// /vt nextwp skips one waypoint, /vt nextwp 2 skips two, and anything
    /// else is refused with the syntax; the Route tab's button skips one.
    /// Mutation: ignore the count and the second command moves one point.
    /// </summary>
    [Fact]
    public void NextWaypointSkipsOneByDefaultOrTheCountGiven()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/navs/Circuit.nav"] = File.ReadAllText(Path.Combine(
            AppContext.BaseDirectory, "Fixtures", "vtank", "nav", "nav_ab.nav"));
        var automation = new FakeAutomation { Name = "Barris", WorldName = "Coldeve" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Command(panel, "nav load Circuit");
        int count = panel.RouteRows.Count;
        Assert.True(count > 3);
        int start = panel.CurrentRouteWaypointIndexForTest;

        Command(panel, "nextwp");
        Assert.Equal((start + 1) % count, panel.CurrentRouteWaypointIndexForTest);
        Command(panel, "nextwp 2");
        Assert.Equal((start + 3) % count, panel.CurrentRouteWaypointIndexForTest);
        Command(panel, "nextwp zero");
        Assert.Equal((start + 3) % count, panel.CurrentRouteWaypointIndexForTest);
        Assert.Contains(automation.Messages, message => message.Contains("Syntax: /vt nextwp", StringComparison.Ordinal));
        panel.SkipRouteWaypoint();
        Assert.Equal((start + 4) % count, panel.CurrentRouteWaypointIndexForTest);
    }

    [Theory]
    [InlineData("meta load Dropped")]
    [InlineData("meta load Dropped.met")]
    public void ADroppedMetaLoadsByCommandWithOrWithoutItsExtension(string arguments)
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/metas/Dropped.met"] = File.ReadAllText(Path.Combine(
            AppContext.BaseDirectory, "Fixtures", "vtank", "met", "bella.met"));
        var automation = new FakeAutomation { Name = "Barris", WorldName = "Coldeve" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));

        Command(panel, arguments);

        Assert.Equal("Dropped.met", panel.SelectedMetaProfile);
    }

    [Fact]
    public void AFailedLoadNamesTheFullPathItTried()
    {
        var storage = new MemoryStorage { RootPath = Path.Combine("data", "vtank") };
        storage.Text["mosstank/navs/Broken.af"] = "not a route at all";
        var automation = new FakeAutomation { Name = "Barris", WorldName = "Coldeve" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        panel.OnTick(0.1d);
        automation.Messages.Clear();

        Command(panel, "nav load Broken");
        panel.OnTick(0.1d);

        Assert.Contains(
            automation.Messages,
            message => message.Contains("Could not load", StringComparison.Ordinal)
                && message.Contains(
                    Path.Combine("data", "vtank", "mosstank", "navs", "Broken.af"),
                    StringComparison.Ordinal));
    }

    [Fact]
    public void AMetDroppedIntoTheMetasFolderLoadsAndSavesBesideItself()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/metas/Dropped.met"] = File.ReadAllText(Path.Combine(
            AppContext.BaseDirectory, "Fixtures", "vtank", "met", "bella.met"));
        var store = new MossTankMetaProfileStore(
            new FakeHost(new FakeAutomation { Name = "Barris" }, storage));
        store.BindCharacter("Barris");

        Assert.Contains("Dropped.met", store.AvailableNames);
        Assert.True(store.Select("Dropped.met"));

        MetaProfile loaded = store.LoadCurrent();
        Assert.NotEmpty(loaded.Rules);

        string dropped = storage.Text["mosstank/metas/Dropped.met"];
        Assert.True(store.SaveCurrent(loaded));

        Assert.Equal(dropped, storage.Text["mosstank/metas/Dropped.met"]);
        Assert.True(storage.Text.ContainsKey("mosstank/metas/Dropped.af"));
    }

    [Fact]
    public void MetaStoreRefusesToLoadANavOnlyFileWithNoticeNamingNavsFolder()
    {
        string navOnlyContent = File.ReadAllText(
            Path.Combine(AppContext.BaseDirectory, "Fixtures", "vtank", "af", "nav_ab.af"));
        var storage = new MemoryStorage();
        storage.Text["mosstank/metas/Misplaced.af"] = navOnlyContent;

        var host = new FakeHost(new FakeAutomation { Name = "Barris" }, storage);
        var store = new MossTankMetaProfileStore(host);
        store.BindCharacter("Barris");
        Assert.True(store.Select("Misplaced"));

        MetaProfile loaded = store.LoadCurrent();

        Assert.Empty(loaded.Rules);
        Assert.NotNull(store.RecoveryNotice);
        Assert.Contains("mosstank/navs/", store.RecoveryNotice, StringComparison.Ordinal);
    }

    private static string LegacyMetaNamedKey(string name)
    {
        byte[] hash = System.Security.Cryptography.SHA256.HashData(
            System.Text.Encoding.UTF8.GetBytes(name.ToLowerInvariant()));
        return $"profiles/meta/named/{Convert.ToHexString(hash.AsSpan(0, 12)).ToLowerInvariant()}.json";
    }

    [Fact]
    public void MetaRosterSweepConvertsEveryNamedLegacyProfileOnce()
    {
        var storage = new MemoryStorage();
        storage.Text["profiles/meta/index.json"] = """{ "Names": ["Farming", "Buffing"] }""";
        storage.Text[LegacyMetaNamedKey("Farming")] = System.Text.Json.JsonSerializer.Serialize(
            new MetaProfile
            {
                Rules = [new MetaRule { Action = new MetaAction { Kind = MetaActionKind.ChatCommand, Text = "/say farming" } }],
            });
        storage.Text[LegacyMetaNamedKey("Buffing")] = System.Text.Json.JsonSerializer.Serialize(
            new MetaProfile
            {
                Rules = [new MetaRule { Action = new MetaAction { Kind = MetaActionKind.ChatCommand, Text = "/say buffing" } }],
            });

        var store = new MossTankMetaProfileStore(
            new FakeHost(new FakeAutomation { Name = "Barris" }, storage));
        store.BindCharacter("Barris");
        store.LoadCurrent();

        Assert.False(storage.Text.ContainsKey(LegacyMetaNamedKey("Farming")));
        Assert.False(storage.Text.ContainsKey(LegacyMetaNamedKey("Buffing")));
        Assert.False(storage.Text.ContainsKey("profiles/meta/index.json"));
        Assert.True(MetafSerializer.TryLoadMeta(
            storage.Text["mosstank/metas/Farming.af"], NoOpSpellCatalogForExport.Instance, out MetaProfile farming, out _));
        Assert.Equal("/say farming", Assert.Single(farming.Rules).Action.Text);
        Assert.True(MetafSerializer.TryLoadMeta(
            storage.Text["mosstank/metas/Buffing.af"], NoOpSpellCatalogForExport.Instance, out MetaProfile buffing, out _));
        Assert.Equal("/say buffing", Assert.Single(buffing.Rules).Action.Text);

        // Idempotent: a fresh store against the same storage sweeps nothing
        // more (there is no roster key left to read).
        var reopened = new MossTankMetaProfileStore(
            new FakeHost(new FakeAutomation { Name = "Barris" }, storage));
        reopened.BindCharacter("Barris");
        reopened.LoadCurrent();
        Assert.True(storage.Text.ContainsKey("mosstank/metas/Farming.af"));
        Assert.True(storage.Text.ContainsKey("mosstank/metas/Buffing.af"));
    }

    [Fact]
    public void MetaStoreRefusesToSaveADisabledRuleAndKeepsThePriorAfContent()
    {
        var storage = new MemoryStorage();
        var store = new MossTankMetaProfileStore(
            new FakeHost(new FakeAutomation { Name = "Barris" }, storage));
        store.BindCharacter("Barris");
        var enabledOnly = new MetaProfile
        {
            Rules = [new MetaRule { Action = new MetaAction { Kind = MetaActionKind.ChatCommand, Text = "/say good" } }],
        };
        Assert.True(store.SaveCurrent(enabledOnly));
        string key = "mosstank/metas/" + VtankProfileDirectory.AutoCharacterFileName("Barris", string.Empty, "af");
        string goodContent = storage.Text[key];

        var withDisabledRule = new MetaProfile
        {
            Rules =
            [
                new MetaRule { Action = new MetaAction { Kind = MetaActionKind.ChatCommand, Text = "/say good" } },
                new MetaRule { Action = new MetaAction { Kind = MetaActionKind.ChatCommand, Text = "/say off" }, Enabled = false },
            ],
        };

        bool saved = store.SaveCurrent(withDisabledRule);

        Assert.False(saved);
        Assert.NotNull(store.SaveNotice);
        Assert.Contains("disabled", store.SaveNotice, StringComparison.OrdinalIgnoreCase);
        Assert.Equal(goodContent, storage.Text[key]);
    }

    [Fact]
    public void FirstRunGuidanceExplainsProfilesImportsAndPersistentShelfOnce()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, storage));

        panel.OnTick(0d);
        panel.OnTick(0d);

        string message = Assert.Single(
            automation.Messages,
            static value => value.Contains("First run:", StringComparison.Ordinal));
        Assert.Contains("plugin shelf", message, StringComparison.Ordinal);
        Assert.Contains(".nav/.utl/.met", message, StringComparison.Ordinal);
        Assert.Equal("shown", storage.Text["onboarding/v1.txt"]);
    }

    [Fact]
    public void RunningMacroRebuffsNormallyAndUsesWiderIdleTopoffOnlyWhenEnabled()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(
                    1,
                    "Life Magic",
                    PluginSkillTraining.Trained,
                    300),
            ],
            KnownSelfBuffs =
            [
                Spell(
                    1,
                    10,
                    "Increases the caster's Life Magic skill by 10 points."),
            ],
            ActiveEnchantments = [new PluginActiveEnchantment(1, 10, 1, 600)],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ToggleCombat();
        panel.OnTick(0d);
        Assert.Empty(automation.CastSpellIds);

        panel.ToggleIdleBuffTopoff();
        panel.OnTick(1d);

        Assert.Equal([1u], automation.CastSpellIds);
        Assert.StartsWith("Buffing", panel.BuffStatus, StringComparison.Ordinal);
    }

    /// <summary>
    /// The idle top-off has a window of its own, wider than the ordinary
    /// rebuff one, and it is the profile's number. A buff with ten minutes
    /// left is due under a twenty-minute window and comfortably fresh under a
    /// five-minute one.
    ///
    /// Mutation: read the ordinary rebuff window for the idle pass, or a
    /// constant, and both rows answer the same way.
    /// </summary>
    [Theory]
    [InlineData(1200d, true)]
    [InlineData(300d, false)]
    public void TheIdleTopoffWindowIsItsOwnProfileNumber(
        double idleWindowSeconds,
        bool casts)
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(
                    1,
                    "Life Magic",
                    PluginSkillTraining.Trained,
                    300),
            ],
            KnownSelfBuffs =
            [
                Spell(
                    1,
                    10,
                    "Increases the caster's Life Magic skill by 10 points."),
            ],
            ActiveEnchantments = [new PluginActiveEnchantment(1, 10, 1, 600)],
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.SetMetaOption(
            "IdleBuffTopoffTimeSeconds",
            AcDream.Plugins.MossTank.Expressions.ExpressionValue.Number(
                idleWindowSeconds));

        // Ten minutes left is outside the ordinary rebuff window either way,
        // so nothing is cast until the idle pass is switched on.
        panel.ToggleCombat();
        panel.OnTick(0d);
        Assert.Empty(automation.CastSpellIds);

        panel.ToggleIdleBuffTopoff();
        panel.OnTick(1d);

        Assert.Equal(casts, automation.CastSpellIds.Count != 0);
    }

    [Fact]
    public void StoppingTheMacroEndsAnAutomaticBuffPassInProgress()
    {
        var automation = BuffPassAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ToggleCombat();
        panel.OnTick(0d);
        Assert.NotEmpty(automation.CastSpellIds);
        Assert.StartsWith("Buffing", panel.BuffStatus, StringComparison.Ordinal);

        int castsWhenStopped = automation.CastSpellIds.Count;
        Assert.True(
            castsWhenStopped < 3,
            "the pass must still be mid-queue for this pin to mean anything");

        panel.ToggleCombat();

        for (int i = 0; i < 10; i++)
            panel.OnTick(1d);

        Assert.Equal(castsWhenStopped, automation.CastSpellIds.Count);
        Assert.DoesNotContain(
            "Buffing", panel.BuffStatus, StringComparison.Ordinal);
    }

    /// <summary>
    /// With the macro stopped, keeping worn gear charged is the only job
    /// left, and the character's own switch is the only thing gating it. The
    /// running list's copy of the same rule carries no such switch and no
    /// combat gate: it sits between the self-recharge and the buffing and
    /// runs whenever the item slot is free.
    ///
    /// Mutation: put the switch back on the running list's copy, or add a
    /// second rule to the stopped list, and this fails.
    /// </summary>
    [Fact]
    public void TheStoppedMacroListIsTheWornManaRuleUnderItsOwnSwitch()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));
        var context = new MacroPassContext(0.3d, CanAct: true);

        IMacroRule whenOff = Assert.Single(panel.MacroDisabledRules);
        Assert.Equal("RefillWieldedMana", whenOff.Name);

        // Worn gear the rule would want to look at, so that whether the rule
        // was asked at all is visible from outside.
        automation.ItemEntries =
        [
            Item(0x50005001u, "Low Wand", 0x00000001u) with
            {
                EquippedLocation = 0x00000002u,
                ItemCurrentMana = 10,
                ItemMaximumMana = 100,
            },
        ];
        Command(panel, "opt set RefillWornMana true");
        Command(panel, "opt set ManaChargesWhenOff false");

        Assert.False(whenOff.ValidNow(in context));
        Assert.DoesNotContain(0x50005001u, automation.Identified);

        Command(panel, "opt set ManaChargesWhenOff true");
        Assert.False(whenOff.ValidNow(in context));
        Assert.Contains(0x50005001u, automation.Identified);

        IMacroRule inMacro = panel.MacroRules.First(
            static rule => rule.Name == "RefillWieldedMana");
        int refill = panel.MacroRules.ToList().FindIndex(
            static rule => rule.Name == "RefillWieldedMana");
        int recharge = panel.MacroRules.ToList().FindIndex(
            static rule => rule.Name == "RechargeSelfNormal");
        int buff = panel.MacroRules.ToList().FindIndex(
            static rule => rule.Name == "BuffSelf");
        Assert.True(recharge < refill && refill < buff);
        Assert.NotSame(whenOff, inMacro);
    }

    /// <summary>
    /// Stopping the macro in the middle of a cast or a use must not cost the
    /// pass its hold. The hold is a count, and the panel gives back exactly
    /// the one it raised: dropping its own flag alone left a count nobody
    /// could give back, and every later pass -- the stopped list included, so
    /// worn gear was never charged again -- returned at the hold for the rest
    /// of the session.
    ///
    /// Mutation: clear the flag without resuming, or reset the count inside
    /// the start, and the worn item is never looked at again.
    /// </summary>
    [Fact]
    public void StoppingTheMacroInsideACastStillLeavesTheStoppedListRunning()
    {
        FakeAutomation automation = BuffPassAutomation();
        automation.SuppressCastCompletion = true;
        automation.ItemEntries =
        [
            Item(0x50005001u, "Low Wand", 0x00000001u) with
            {
                EquippedLocation = 0x00000002u,
                ItemCurrentMana = 10,
                ItemMaximumMana = 100,
            },
        ];
        var panel = new MossTankPanel(new FakeHost(automation));
        Command(panel, "opt set RefillWornMana true");
        Command(panel, "opt set ManaChargesWhenOff true");

        panel.ToggleCombat();
        panel.OnTick(0d);
        Assert.Single(automation.CastSpellIds);
        // One more frame, which is where the hold on the pass is taken: it is
        // taken beside the pass, on the frame after the cast went out.
        panel.OnTick(0.1d);

        // Stopped with the cast still unanswered and the hold in place.
        panel.ToggleCombat();
        automation.Identified.Clear();

        for (int tick = 0; tick < 40; tick++)
            panel.OnTick(0.3d);

        Assert.Contains(0x50005001u, automation.Identified);
    }

    /// <summary>
    /// With the macro off, keeping worn gear charged keeps the pass alive --
    /// and that is the only job it keeps alive. A buff pass left half done
    /// ends with the macro, and the stopped list casts nothing of its own.
    /// </summary>
    [Fact]
    public void TheStoppedMacroListKeepsThePassAliveWithoutBuffing()
    {
        var automation = BuffPassAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "opt set ManaChargesWhenOff true");
        panel.ToggleCombat();
        panel.OnTick(0d);
        int castsWhenStopped = automation.CastSpellIds.Count;
        Assert.NotEmpty(automation.CastSpellIds);

        panel.ToggleCombat();

        for (int i = 0; i < 10; i++)
            panel.OnTick(1d);

        Assert.Equal(castsWhenStopped, automation.CastSpellIds.Count);
        Assert.DoesNotContain(
            "Buffing", panel.BuffStatus, StringComparison.Ordinal);
    }

    [Fact]
    public void ForceBuffWithTheMacroOffCastsNothingUntilRunMacro()
    {
        var automation = BuffPassAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ForceBuff();
        for (int i = 0; i < 20; i++)
            panel.OnTick(0.3d);

        Assert.Empty(automation.CastSpellIds);

        panel.ToggleCombat();
        for (int i = 0; i < 20; i++)
            panel.OnTick(0.3d);

        Assert.NotEmpty(automation.CastSpellIds);
    }

    [Fact]
    public void TogglingBuffingWithTheMacroOffCastsNothing()
    {
        var automation = BuffPassAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ForceBuff();
        panel.SetMetaOption("EnableBuffing", Truthy(false));
        for (int i = 0; i < 10; i++)
            panel.OnTick(0.3d);
        panel.SetMetaOption("EnableBuffing", Truthy(true));
        for (int i = 0; i < 20; i++)
            panel.OnTick(0.3d);

        Assert.Empty(automation.CastSpellIds);
    }

    [Fact]
    public void AServerRejectedCastRePicksTheSameSpellInsteadOfAdvancing()
    {
        var automation = BuffPassAutomation();
        automation.NextCastWeenieError = 0x1Du;
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ToggleCombat();
        for (int i = 0; i < 8; i++)
            panel.OnTick(0.3d);

        Assert.NotEmpty(automation.CastSpellIds);
        Assert.All(automation.CastSpellIds, id => Assert.Equal(1u, id));
        Assert.Contains("Spell 1", panel.BuffStatus, StringComparison.Ordinal);
    }

    [Fact]
    public void AnAcknowledgedCastWithNoResultTextKeepsThePassSuspended()
    {
        var automation = BuffPassAutomation();
        automation.SuppressCastResultText = true;
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ToggleCombat();
        panel.OnTick(0d);
        Assert.Single(automation.CastSpellIds);

        // Three seconds is inside the 4 x 907 ms result budget.
        for (int i = 0; i < 10; i++)
            panel.OnTick(0.3d);

        Assert.False(automation.IsCasting);
        Assert.Single(automation.CastSpellIds);
    }

    [Fact]
    public void AnAcceptedCastAdvancesTheQueueOnItsSuccessfulResult()
    {
        var automation = BuffPassAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ToggleCombat();
        for (int i = 0; i < 8; i++)
            panel.OnTick(0.3d);

        // The plain fake never marks the enchantment active, so a later scan
        // re-queues the same three; only the first pass is under test.
        Assert.Equal([1u, 2u, 3u], automation.CastSpellIds.Take(3));
    }

    [Fact]
    public void APermanentlyRefusedSpellIsPickedAgainForeverBecauseAuthenticNeverGivesUp()
    {
        var automation = BuffPassAutomation();
        automation.NextCastWeenieError = 0x1Du;
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ToggleCombat();
        for (int i = 0; i < 40; i++)
            panel.OnTick(1d);

        Assert.True(
            automation.CastSpellIds.Count > 5,
            "the refused spell was never re-issued");
        Assert.All(automation.CastSpellIds, id => Assert.Equal(1u, id));
    }

    [Fact]
    public void TheCastSuspensionHoldsForTheResultThoughIsCastingNeverLatched()
    {
        var automation = BuffPassAutomation();
        automation.SuppressCastCompletion = true;
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ToggleCombat();
        panel.OnTick(0d);
        Assert.Single(automation.CastSpellIds);

        for (int i = 0; i < 10; i++)
            panel.OnTick(0.3d);

        Assert.False(automation.IsCasting);
        Assert.Single(automation.CastSpellIds);

        // Past the 5000 ms attempt watchdog the tracker drops
        // to idle and re-issues the SAME spell; it never walks the queue.
        for (int i = 0; i < 12; i++)
            panel.OnTick(0.3d);

        Assert.True(automation.CastSpellIds.Count > 1);
        Assert.All(automation.CastSpellIds, id => Assert.Equal(1u, id));
    }

    [Fact]
    public void AMasteryBuffThatLandsUnlocksTheHigherTierOfTheNextPick()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 100),
            ],
            KnownSelfBuffs =
            [
                Spell(1, 10, "Increases the caster's Life Magic skill by 10 points."),
                // The second family: tier 2 needs skill 205, tier 1 needs 15.
                Spell(2, 20, "Increases the caster's Strength by 10 points.")
                    with { Tier = 2, Difficulty = 200 },
                Spell(3, 20, "Increases the caster's Strength by 10 points.")
                    with { Tier = 1, Difficulty = 10 },
            ],
            Attributes = [new PluginAttributeInfo(0, "Strength", 100)],
            RaiseSkillOnCast = (1u, 1u, 300u),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ToggleCombat();
        for (int tick = 0; tick < 6; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([1u, 2u], automation.CastSpellIds);
    }

    [Fact]
    public void TheIdleTopoffWindowBelongsToRow53NotToTheRebuffRule()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
            ],
            KnownSelfBuffs =
            [
                Spell(1, 10, "Increases the caster's Life Magic skill by 10 points."),
            ],
            // 600 s left: over the 300 s rebuff window, under the 1200 s idle one.
            ActiveEnchantments = [new PluginActiveEnchantment(1u, 10u, 1, 600d)],
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.SetMetaOption("IdleBuffTopoff", Truthy(true));
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "log ActiveRule on", "/vt log ActiveRule on"));
        automation.Messages.Clear();

        panel.ToggleCombat();
        panel.OnTick(0.3d);

        Assert.Contains(
            automation.Messages,
            static message => message.StartsWith(
                "[MossTank] Picked BuffSelfIdle", StringComparison.Ordinal));
        Assert.DoesNotContain(
            automation.Messages,
            static message => message.StartsWith(
                "[MossTank] Picked BuffSelf ", StringComparison.Ordinal));
    }

    [Fact]
    public void WithIdleTopoffOffTheWiderWindowIsNotConsideredAtAll()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
            ],
            KnownSelfBuffs =
            [
                Spell(1, 10, "Increases the caster's Life Magic skill by 10 points."),
            ],
            ActiveEnchantments = [new PluginActiveEnchantment(1u, 10u, 1, 600d)],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ToggleCombat();
        for (int tick = 0; tick < 6; tick++)
            panel.OnTick(0.3d);

        Assert.Empty(automation.CastSpellIds);
    }

    [Fact]
    public void AFamilyWhoseTopTierHasNoScarabsFallsToTheCastableLowerTier()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
            ],
            KnownSelfBuffs =
            [
                Spell(2, 20, "Increases the caster's Strength by 10 points.")
                    with { Tier = 2, Difficulty = 200, FormulaComponentIds = [7u] },
                Spell(3, 20, "Increases the caster's Strength by 10 points.")
                    with { Tier = 1, Difficulty = 10, FormulaComponentIds = [8u] },
            ],
            Attributes = [new PluginAttributeInfo(0, "Strength", 100)],
            ItemEntries = [Item(50, "Pyreal Scarab", 1)],
        };
        automation.Components[7u] = Component(7u, "Lead Scarab");
        automation.Components[8u] = Component(8u, "Pyreal Scarab");
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ToggleCombat();
        for (int tick = 0; tick < 6; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([3u], automation.CastSpellIds);
        Assert.Contains(
            "[MossTank] Out of component \"Lead Scarab\" — spells that "
                + "need it are off.",
            automation.Messages);
    }

    [Fact]
    public void AFamilyWithNoCastableTierIsSkippedAndTheNextFamilyIsCast()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
            ],
            KnownSelfBuffs =
            [
                Spell(2, 20, "Increases the caster's Strength by 10 points.")
                    with { FormulaComponentIds = [7u] },
                Spell(3, 21, "Increases the caster's Endurance by 10 points.")
                    with { FormulaComponentIds = [8u] },
            ],
            Attributes =
            [
                new PluginAttributeInfo(0, "Strength", 100),
                new PluginAttributeInfo(1, "Endurance", 100),
            ],
            ItemEntries = [Item(50, "Pyreal Scarab", 1)],
        };
        automation.Components[7u] = Component(7u, "Lead Scarab");
        automation.Components[8u] = Component(8u, "Pyreal Scarab");
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ToggleCombat();
        for (int tick = 0; tick < 6; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([3u], automation.CastSpellIds);
        Assert.Contains(
            "[MossTank] Buff skipped: no known spell in the class of Spell 2.",
            automation.Messages);
    }

    [Fact]
    public void ARefusedCastSaysWhichSpellAndWhyOnceOnTheCastInfoChannel()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
            ],
            KnownSelfBuffs =
            [
                Spell(1, 20, "Increases the caster's Strength by 10 points."),
            ],
            Attributes = [new PluginAttributeInfo(0, "Strength", 100)],
        };
        automation.CastRefusals[1u] = PluginCastRequestResult.MissingComponents;
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "log CastInfo on", "/vt log CastInfo on"));
        automation.Messages.Clear();

        panel.ToggleCombat();
        for (int tick = 0; tick < 8; tick++)
            panel.OnTick(0.3d);

        Assert.Empty(automation.CastSpellIds);
        Assert.Single(
            automation.Messages,
            message => message.Contains("not issued", StringComparison.Ordinal));
        Assert.Contains(
            "[MossTank] SpellCaster: Spell 1 not issued — MissingComponents",
            automation.Messages);
    }

    [Fact]
    public void ATierTheHostHasNoComponentsForIsNotACandidateAndTheWalkDropsToTheNext()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
            ],
            KnownSelfBuffs =
            [
                Spell(1, 20, "Increases the caster's Strength by 10 points."),
                Spell(2, 20, "Increases the caster's Strength by 10 points.")
                    with { Tier = 6, Difficulty = 250 },
            ],
            Attributes = [new PluginAttributeInfo(0, "Strength", 100)],
        };
        automation.MissingComponentSpellIds.Add(2u);

        var panel = new MossTankPanel(new FakeHost(automation));
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "log CastInfo on", "/vt log CastInfo on"));
        automation.Messages.Clear();

        panel.ToggleCombat();
        for (int tick = 0; tick < 8; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([1u], automation.CastSpellIds);
        Assert.DoesNotContain(
            automation.Messages,
            message => message.Contains("not issued", StringComparison.Ordinal));
    }

    [Fact]
    public void TimersTraceNamesEveryHigherTierAndTheFirstFailingTermWithNumbers()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills = [new PluginSkillInfo(1, "Skill", PluginSkillTraining.Trained, 288)],
            Attributes = [new PluginAttributeInfo(0, "Strength", 100)],
            KnownSelfBuffs =
            [
                // Lowest Difficulty, self-targeted -> BuffLine.Reference (the
                // family's own anchor), and the tier the walk should land on:
                // needed = 250 + 5 = 255 <= 288.
                Spell(6, 90, "Increases the caster's Strength by 10 points.")
                    with { Tier = 6, Difficulty = 250 },
                // needed = 296 + 5 = 301 > 288 -- skill-short.
                Spell(7, 90, "Increases the caster's Strength by 10 points.")
                    with { Tier = 7, Difficulty = 296 },
                Spell(8, 90, "Increases the caster's Strength by 10 points.")
                    with { Tier = 8, Difficulty = 340, IsFellowship = true },
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        Command(panel, "log Timers on");
        automation.Messages.Clear();

        panel.ToggleCombat();
        panel.OnTick(0.3d);

        Assert.Contains(
            "[MossTank] Buffing: Spell 6 — picked Spell 6 (gen 6); "
                + "rejected: Spell 8 unknown, Spell 7 skill 288 < 301",
            automation.Messages);
    }

    [Fact]
    public void NoTimersLineWhenTheHighestKnownTierIsPicked()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills = [new PluginSkillInfo(1, "Skill", PluginSkillTraining.Trained, 300)],
            Attributes = [new PluginAttributeInfo(0, "Strength", 100)],
            KnownSelfBuffs =
            [
                Spell(1, 91, "Increases the caster's Strength by 10 points.")
                    with { Tier = 1, Difficulty = 10 },
                Spell(2, 91, "Increases the caster's Strength by 10 points.")
                    with { Tier = 2, Difficulty = 20 },
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        Command(panel, "log Timers on");
        automation.Messages.Clear();

        panel.ToggleCombat();
        for (int tick = 0; tick < 6; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([2u], automation.CastSpellIds);
        Assert.DoesNotContain(
            automation.Messages,
            message => message.Contains("Buffing:", StringComparison.Ordinal));
    }

    [Fact]
    public void TimersTraceIsNotReemittedAcrossRepeatedIdenticalPasses()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills = [new PluginSkillInfo(1, "Skill", PluginSkillTraining.Trained, 288)],
            Attributes = [new PluginAttributeInfo(0, "Strength", 100)],
            KnownSelfBuffs =
            [
                Spell(6, 90, "Increases the caster's Strength by 10 points.")
                    with { Tier = 6, Difficulty = 250 },
                Spell(7, 90, "Increases the caster's Strength by 10 points.")
                    with { Tier = 7, Difficulty = 296 },
                Spell(8, 90, "Increases the caster's Strength by 10 points.")
                    with { Tier = 8, Difficulty = 340, IsFellowship = true },
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        Command(panel, "log Timers on");
        automation.Messages.Clear();

        panel.ToggleCombat();
        panel.OnTick(0.3d);
        int afterFirstPass = automation.Messages.Count(
            message => message.StartsWith(
                "[MossTank] Buffing: Spell 6", StringComparison.Ordinal));
        Assert.Equal(1, afterFirstPass);

        for (int tick = 0; tick < 5; tick++)
            panel.OnTick(0.3d);
        int afterFiveMorePasses = automation.Messages.Count(
            message => message.StartsWith(
                "[MossTank] Buffing: Spell 6", StringComparison.Ordinal));
        Assert.Equal(1, afterFiveMorePasses);
    }

    [Fact]
    public void TheBuffCastRecastLockWidensTheDueWindowRatherThanBlockingPicks()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
            ],
            KnownSelfBuffs =
            [
                Spell(1, 10, "Increases the caster's Life Magic skill by 10 points."),
                Spell(2, 20, "Increases the caster's Strength by 10 points."),
            ],
            Attributes = [new PluginAttributeInfo(0, "Strength", 100)],
            ActiveEnchantments = [new PluginActiveEnchantment(2, 20, 1, 310)],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ToggleCombat();
        for (int tick = 0; tick < 6; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([1u, 2u], automation.CastSpellIds);
    }

    [Fact]
    public void ForceBuffZeroesTheDueStampsAndCancelPutsThemBack()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
            ],
            KnownSelfBuffs =
            [
                Spell(1, 10, "Increases the caster's Life Magic skill by 10 points."),
            ],
            ActiveEnchantments = [new PluginActiveEnchantment(1, 10, 1, 1800)],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        // Nothing is due: 1800 s left against a 300 s threshold.
        panel.ToggleCombat();
        panel.OnTick(0.3d);
        Assert.Empty(automation.CastSpellIds);

        // Force-buff - everything now reads as about to expire...
        panel.ForceBuff();
        // ...and cancelling it puts it back before the next heartbeat can act.
        panel.CancelForceBuff();
        for (int tick = 0; tick < 6; tick++)
            panel.OnTick(0.3d);
        Assert.Empty(automation.CastSpellIds);

        panel.ForceBuff();
        for (int tick = 0; tick < 6; tick++)
            panel.OnTick(0.3d);
        Assert.Equal([1u], automation.CastSpellIds);
    }

    [Fact]
    public void AForcedEntryStopsBeingForcedOnceItIsRecast()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
            ],
            KnownSelfBuffs =
            [
                Spell(1, 10, "Increases the caster's Life Magic skill by 10 points."),
            ],
            ActiveEnchantments = [new PluginActiveEnchantment(1, 10, 1, 1800)],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ToggleCombat();
        panel.ForceBuff();
        for (int tick = 0; tick < 20; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([1u], automation.CastSpellIds);
    }

    [Fact]
    public void TheBuffCastEmitsVtanksSpellCastLogLine()
    {
        var automation = BuffPassAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "log SpellCast on");
        panel.ToggleCombat();
        panel.OnTick(0.3d);

        Assert.Contains(
            automation.Messages,
            static value => value.Contains(
                "Casting: Spell 1 on ", StringComparison.Ordinal));
    }

    [Fact]
    public void AFizzledBuffIsSimplyStillDueOnTheNextHeartbeat()
    {
        var automation = BuffPassAutomation();
        automation.CastResultText = "Your spell fizzled.";
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ToggleCombat();
        for (int tick = 0; tick < 10; tick++)
            panel.OnTick(0.3d);

        Assert.True(automation.CastSpellIds.Count > 2);
        Assert.All(automation.CastSpellIds, id => Assert.Equal(1u, id));
    }

    [Fact]
    public void APermanentFailIsNotADropBecauseEqHasNoSuchBlacklist()
    {
        var automation = BuffPassAutomation();
        automation.CastResultText = "Target is out of range";
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ToggleCombat();
        for (int tick = 0; tick < 10; tick++)
            panel.OnTick(0.3d);

        Assert.True(automation.CastSpellIds.Count > 2);
        Assert.All(automation.CastSpellIds, id => Assert.Equal(1u, id));
    }

    /// <summary>
    /// The Items page has no notice line of its own, so every refusal and
    /// every success answers in chat. A refused add that reports nothing is
    /// indistinguishable from a page with no Add button.
    /// Mutation: set the notice without announcing it and chat stays silent.
    /// </summary>
    [Fact]
    public void AnItemsAddThatIsRefusedSaysWhy()
    {
        var automation = ItemEnchantAutomation();
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        panel.AddSelectedItem(); // nothing selected
        Assert.Equal("Select an owned inventory item first.", panel.ProfileNotice);

        panel.RemoveSelectedItem(); // nothing in the list yet
        Assert.Equal("The Items profile is empty.", panel.ProfileNotice);

        host.Selection.Select(10);
        panel.AddSelectedItem();
        Assert.Contains("War Wand", panel.ItemNameColumn);

        panel.AddSelectedItem(); // the same item twice
        Assert.Contains("already in this list", panel.ProfileNotice, StringComparison.Ordinal);

        // Each of those four outcomes reached the player, in chat.
        Assert.Collection(
            automation.Messages,
            message => Assert.Contains(
                "Select an owned inventory item first.", message, StringComparison.Ordinal),
            message => Assert.Contains(
                "The Items profile is empty.", message, StringComparison.Ordinal),
            message => Assert.Contains(
                "Added War Wand.", message, StringComparison.Ordinal),
            message => Assert.Contains(
                "already in this list", message, StringComparison.Ordinal));
    }

    [Fact]
    public void AddingAWandPopulatesItsThreeDefaultAurasAndCastsThemAtTheItem()
    {
        var automation = ItemEnchantAutomation();
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        host.Selection.Select(10);
        panel.AddSelectedItem();
        panel.ToggleCombat();
        for (int tick = 0; tick < 12; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([101u, 102u, 103u], automation.CastSpellIds);
        Assert.Equal(10u, host.Selection.SelectedObjectId);
    }

    /// <summary>
    /// Mutation pin: omit the ordered-ID append when admitting an item;
    /// the saved BuffedItems table has no selected-object row.
    /// </summary>
    [Fact]
    public void AddingSelectedItemPersistsItsObjectIdentityInUsd()
    {
        var storage = new MemoryStorage();
        var automation = ItemEnchantAutomation();
        var host = new FakeHost(automation, storage);
        var panel = new MossTankPanel(host);
        host.Selection.Select(10u);

        panel.AddSelectedItem();

        string usd = Assert.Single(storage.Text,
            entry => entry.Key.EndsWith(".usd", StringComparison.Ordinal)).Value;
        VtankTable table = VtankDatabase.Parse(usd).Find("BuffedItems")!;
        VtankRow row = Assert.Single(table.Rows);
        Assert.Equal(10, row.Cells[table.ColumnIndex("Object")].AsInt());
        Assert.Equal(-1, row.Cells[table.ColumnIndex("Spell")].AsInt());
    }

    /// <summary>
    /// Mutation <c>IgnoreProfiledItemSpellRows</c>: omit the numeric
    /// BuffedItems row adapter and no cast is selected for object 11.
    /// </summary>
    [Fact]
    public void ImportedBuffedItemSpellTargetsItsExactSameNameObject()
    {
        var storage = new MemoryStorage();
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        database.Find("BuffedItems")!.Rows.Add(new VtankRow
        {
            Cells =
            {
                VtankCell.Int(11),
                VtankCell.Int(101),
            },
        });
        storage.Text["mosstank/profiles/Imported.usd"] = database.Render();
        FakeAutomation automation = ItemEnchantAutomation();
        automation.ItemEntries =
        [
            Item(10, "War Wand", 0x8000u, validLocations: 0x01000000u),
            Item(11, "War Wand", 0x8000u, validLocations: 0x01000000u),
        ];
        var host = new FakeHost(automation, storage);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        Command(panel, "settings load Imported");
        host.Selection.Select(10u);
        panel.ToggleCombat();
        for (int tick = 0; tick < 6; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([101u], automation.CastSpellIds);
        Assert.Equal([11u], automation.CastSelectionIds);
    }

    [Fact]
    public void ImportedBuffedItemWeaponSentinelUsesTheEquippedWeapon()
    {
        var storage = new MemoryStorage();
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        database.Find("BuffedItems")!.Rows.Add(new VtankRow
        {
            Cells = { VtankCell.Int(-1), VtankCell.Int(101) },
        });
        storage.Text["mosstank/profiles/Imported.usd"] = database.Render();
        FakeAutomation automation = ItemEnchantAutomation();
        automation.ItemEntries =
        [
            Item(10, "Worn Coat", 2u) with { EquippedLocation = 0x00000001u },
            Item(11, "War Wand", 0x8000u) with { EquippedLocation = 0x01000000u },
        ];
        var host = new FakeHost(automation, storage);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        Command(panel, "settings load Imported");
        panel.ToggleCombat();
        for (int tick = 0; tick < 6; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([101u], automation.CastSpellIds);
        Assert.Equal([11u], automation.CastSelectionIds);
    }

    [Fact]
    public void ImportedUntargetedBuffedItemSpellJoinsTheNormalBuffPlan()
    {
        var storage = new MemoryStorage();
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        database.Find("BuffedItems")!.Rows.Add(new VtankRow
        {
            Cells = { VtankCell.Int(10), VtankCell.Int(101) },
        });
        storage.Text["mosstank/profiles/Imported.usd"] = database.Render();
        FakeAutomation automation = ItemEnchantAutomation();
        automation.Skills = [new PluginSkillInfo(32, "Item Enchantment", PluginSkillTraining.Trained, 300)];
        automation.KnownSelfBuffs =
        [
            NamedSpell(101, 201, "Untargeted Aura I", ItemEnchantmentSchoolId)
                with { IsUntargeted = true, IsSelfTargeted = true },
        ];
        var host = new FakeHost(automation, storage);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        Command(panel, "settings load Imported");
        panel.ToggleCombat();
        for (int tick = 0; tick < 6; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([101u], automation.CastSpellIds);
        Assert.Equal([0u], automation.CastSelectionIds);
    }

    [Fact]
    public void UnknownAndEmptyImportedBuffedItemSpellsStayInert()
    {
        var storage = new MemoryStorage();
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        VtankTable table = database.Find("BuffedItems")!;
        table.Rows.Add(new VtankRow { Cells = { VtankCell.Int(10), VtankCell.Int(999) } });
        table.Rows.Add(new VtankRow { Cells = { VtankCell.Int(10), VtankCell.Int(-1) } });
        storage.Text["mosstank/profiles/Imported.usd"] = database.Render();
        FakeAutomation automation = ItemEnchantAutomation();
        var host = new FakeHost(automation, storage);
        var panel = new MossTankPanel(host);

        Command(panel, "settings load Imported");
        panel.ToggleCombat();
        for (int tick = 0; tick < 6; tick++)
            panel.OnTick(0.3d);

        Assert.Empty(automation.CastSpellIds);
    }

    [Fact]
    public void RemovingSelectedOwnedItemRemovesItsUsdIdentity()
    {
        var storage = new MemoryStorage();
        var automation = ItemEnchantAutomation();
        var host = new FakeHost(automation, storage);
        var panel = new MossTankPanel(host);
        host.Selection.Select(10u);
        panel.AddSelectedItem();

        panel.RemoveSelectedItem();

        string usd = Assert.Single(storage.Text,
            entry => entry.Key.EndsWith(".usd", StringComparison.Ordinal)).Value;
        Assert.Empty(VtankDatabase.Parse(usd).Find("BuffedItems")!.Rows);
    }

    /// <summary>
    /// Mutation <c>SkipExactBuffedItemRemoval</c>: omit the selected stored ID
    /// from removal. The missing row remains while the same-name item and raw rows
    /// must stay intact.
    /// </summary>
    /// <summary>
    /// An item is listed by its object id, and an id cannot be looked up
    /// while there is no inventory: logged out, or just logged in. The page
    /// then showed every item twice -- once as an INVALID id and once by its
    /// name. Each item is one row, under the name it was last seen with, and
    /// nothing is called invalid that is merely not looked up yet. With an
    /// inventory that really does not hold the item, the row says so by name.
    /// </summary>
    [Fact]
    public void AListedItemIsOneRowUnderItsNameWhileThereIsNoInventory()
    {
        var storage = new MemoryStorage();
        var firstHost = new FakeHost(new FakeAutomation
        {
            ItemEntries =
            [
                Item(10, "Decapitator's Blade", 1),
                Item(11, "Wings of Rakhil", 1),
            ],
        }, storage);
        var first = new MossTankPanel(firstHost);
        firstHost.Selection.Select(10u);
        first.AddSelectedItem();
        firstHost.Selection.Select(11u);
        first.AddSelectedItem();

        var loggedOut = new MossTankPanel(
            new FakeHost(new FakeAutomation { ItemEntries = [] }, storage));

        Assert.Equal(["Decapitator's Blade", "Wings of Rakhil"], loggedOut.ItemRows);

        var withoutTheBlade = new MossTankPanel(new FakeHost(new FakeAutomation
        {
            ItemEntries = [Item(11, "Wings of Rakhil", 1)],
        }, storage));

        Assert.Equal(
            ["<INVALID Decapitator's Blade>", "Wings of Rakhil"],
            withoutTheBlade.ItemRows);
    }

    [Fact]
    public void ItemRowsDeleteTheSelectedIdentityEvenWhenMissingOrNamesMatch()
    {
        var storage = new MemoryStorage();
        var originalItems = new FakeAutomation
        {
            ItemEntries =
            [
                Item(10, "Twin Sword", 1),
                Item(11, "Twin Sword", 1),
            ],
        };
        var firstHost = new FakeHost(originalItems, storage);
        var first = new MossTankPanel(firstHost);
        firstHost.Selection.Select(10u);
        first.AddSelectedItem();
        firstHost.Selection.Select(11u);
        first.AddSelectedItem();
        string usdKey = Assert.Single(storage.Text.Keys,
            key => key.EndsWith(".usd", StringComparison.Ordinal));
        VtankDatabase document = VtankDatabase.Parse(storage.Text[usdKey]);
        VtankTable table = document.Find("BuffedItems")!;
        var spellRow = new VtankRow();
        spellRow.Cells.Add(VtankCell.Int(10));
        spellRow.Cells.Add(VtankCell.Int(17));
        table.Rows.Add(spellRow);
        table.Rows.Add(new VtankRow
        {
            Cells = { VtankCell.String("not-an-object-id"), VtankCell.Int(99) },
        });
        storage.Text[usdKey] = document.Render();

        var second = new MossTankPanel(new FakeHost(new FakeAutomation
        {
            ItemEntries = [Item(11, "Twin Sword", 1)],
        }, storage));
        Assert.Equal(
            ["<INVALID Twin Sword>", "Twin Sword", "<INVALID Twin Sword> — Spell 0x00000011"],
            second.ItemRows);

        second.DeleteItemRowAt(0);

        Assert.Equal(["Twin Sword", "<INVALID Twin Sword> — Spell 0x00000011"], second.ItemRows);
        table = VtankDatabase.Parse(storage.Text[usdKey]).Find("BuffedItems")!;
        Assert.Contains(table.Rows, row => row.Cells[table.ColumnIndex("Object")]
            .AsInt() == 11);
        Assert.Contains(table.Rows, row => row.Cells[table.ColumnIndex("Object")]
            .AsString() == "not-an-object-id");

        second.DeleteItemRowAt(1);
        Assert.Equal(["Twin Sword"], second.ItemRows);

        second.DeleteItemRowAt(0);

        Assert.Empty(second.ItemRows);
        table = VtankDatabase.Parse(storage.Text[usdKey]).Find("BuffedItems")!;
        Assert.Single(table.Rows);
        Assert.Equal("not-an-object-id", table.Rows[0].Cells[
            table.ColumnIndex("Object")].AsString());
    }

    /// <summary>
    /// Imported rows identify a single Object/Spell pair. Their removal must
    /// update the live plan as well as the saved table, leaving other spells
    /// and the equipment row for the same object intact.
    /// Mutation: omit the profiled-row branch in
    /// <c>DeleteItemRowAtCore</c>; the imported spell stays in the UI and
    /// the post-delete row assertion fails.
    /// </summary>
    [Fact]
    public void ImportedBuffedItemUiDeletesOneSpellPairAndKeepsItsSiblings()
    {
        var storage = new MemoryStorage();
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        VtankTable table = database.Find("BuffedItems")!;
        table.Rows.Add(new VtankRow { Cells = { VtankCell.Int(10), VtankCell.Int(-1) } });
        table.Rows.Add(new VtankRow { Cells = { VtankCell.Int(10), VtankCell.Int(101) } });
        table.Rows.Add(new VtankRow { Cells = { VtankCell.Int(10), VtankCell.Int(102) } });
        storage.Text["mosstank/profiles/Imported.usd"] = database.Render();
        FakeAutomation automation = ItemEnchantAutomation();
        var host = new FakeHost(automation, storage);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        Command(panel, "settings load Imported");
        Assert.Equal(
            ["War Wand", "War Wand — Aura of Defender Self I", "War Wand — Aura of Hermetic Link Self I"],
            panel.ItemRows);

        panel.DeleteItemRowAt(1);

        Assert.Equal(
            ["War Wand", "War Wand — Aura of Hermetic Link Self I"],
            panel.ItemRows);
        panel.ToggleCombat();
        for (int tick = 0; tick < 8; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([102u], automation.CastSpellIds);
        table = VtankDatabase.Parse(storage.Text["mosstank/profiles/Imported.usd"]).Find("BuffedItems")!;
        Assert.Contains(table.Rows, row => row.Cells[table.ColumnIndex("Object")].AsInt() == 10
            && row.Cells[table.ColumnIndex("Spell")].AsInt() == -1);
        Assert.DoesNotContain(table.Rows, row => row.Cells[table.ColumnIndex("Object")].AsInt() == 10
            && row.Cells[table.ColumnIndex("Spell")].AsInt() == 101);
        Assert.Contains(table.Rows, row => row.Cells[table.ColumnIndex("Object")].AsInt() == 10
            && row.Cells[table.ColumnIndex("Spell")].AsInt() == 102);
    }

    /// <summary>
    /// A numeric profile spell supplies a family even when its old display
    /// name does not match a known higher tier's name.
    /// Mutation: resolve the numeric row through <c>exemplar.Name</c> rather
    /// than the exemplar itself; the renamed known tier is not cast.
    /// </summary>
    [Fact]
    public void ImportedBuffedItemNumericExemplarResolvesKnownTierByFamily()
    {
        var storage = new MemoryStorage();
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        database.Find("BuffedItems")!.Rows.Add(new VtankRow
        {
            Cells = { VtankCell.Int(10), VtankCell.Int(100) },
        });
        storage.Text["mosstank/profiles/Imported.usd"] = database.Render();
        FakeAutomation automation = ItemEnchantAutomation();
        automation.Skills =
        [
            new PluginSkillInfo(
                ItemEnchantmentSchoolId,
                "Item Enchantment",
                PluginSkillTraining.Trained,
                300),
        ];
        automation.KnownSelfBuffs =
        [
            NamedSpell(101, 201, "Renamed Higher Aura VI", ItemEnchantmentSchoolId)
                with { Tier = 6, IsSelfTargeted = true },
        ];
        automation.CatalogSpells =
        [
            NamedSpell(100, 201, "Old Exemplar I", ItemEnchantmentSchoolId)
                with { IsSelfTargeted = true },
        ];
        var host = new FakeHost(automation, storage);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        Command(panel, "settings load Imported");
        Assert.Equal(["War Wand — Old Exemplar I"], panel.ItemRows);
        panel.ToggleCombat();
        for (int tick = 0; tick < 6; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([101u], automation.CastSpellIds);
        Assert.Equal([10u], automation.CastSelectionIds);
    }

    [Fact]
    public void AddingAShieldPopulatesTheSevenBanesAndImpenetrability()
    {
        var automation = ItemEnchantAutomation();
        automation.ItemEntries =
        [
            Item(20, "Tower Shield", 2, validLocations: 0x00200000u),
        ];
        automation.KnownSelfBuffs =
        [
            NamedSpell(201, 301, "Blade Bane I", ItemEnchantmentSchoolId),
            NamedSpell(202, 302, "Impenetrability I", ItemEnchantmentSchoolId),
        ];
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        host.Selection.Select(20);
        panel.AddSelectedItem();
        panel.ToggleCombat();
        for (int tick = 0; tick < 12; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([202u, 201u, 201u, 202u], automation.CastSpellIds);
    }

    [Fact]
    public void ShieldBaneRowsFollowTheDamageElementEnumOrder()
    {
        FakeAutomation automation = ItemEnchantAutomation();
        automation.ItemEntries =
        [
            Item(20, "Tower Shield", 2, validLocations: 0x00200000u),
        ];
        automation.KnownSelfBuffs =
        [
            NamedSpell(210, 310, "Piercing Bane I", ItemEnchantmentSchoolId),
            NamedSpell(211, 311, "Acid Bane I", ItemEnchantmentSchoolId),
            NamedSpell(212, 312, "Flame Bane I", ItemEnchantmentSchoolId),
        ];
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        host.Selection.Select(20);
        panel.AddSelectedItem();
        panel.ToggleCombat();
        for (int tick = 0; tick < 12; tick++)
            panel.OnTick(0.3d);

        Assert.Equal(
            [211u, 212u, 210u, 210u, 211u, 212u], automation.CastSpellIds);
    }

    [Fact]
    public void UntrainedItemEnchantmentOverTheLevelThresholdBuildsNoItemRows()
    {
        FakeAutomation automation = ItemEnchantAutomation();
        // No Item Enchantment skill at all, and well past
        // BuffWithUntrained-Item's default 80.
        automation.Level = 100;
        var withBanes = new List<PluginSpellInfo>(automation.KnownSelfBuffs)
        {
            Bane(341, 441, "Impenetrability I"),
            Bane(342, 442, "Acid Bane I"),
        };
        automation.KnownSelfBuffs = withBanes;
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        host.Selection.Select(10);
        panel.AddSelectedItem();
        panel.ToggleCombat();
        for (int tick = 0; tick < 12; tick++)
            panel.OnTick(0.3d);

        Assert.Empty(automation.CastSpellIds);
    }

    [Fact]
    public void UntrainedItemEnchantmentUnderTheLevelThresholdStillBuildsRows()
    {
        FakeAutomation automation = ItemEnchantAutomation();
        automation.Level = 80;
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        host.Selection.Select(10);
        panel.AddSelectedItem();
        panel.ToggleCombat();
        for (int tick = 0; tick < 12; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([101u, 102u, 103u], automation.CastSpellIds);
    }

    [Fact]
    public void AWandAddedWithNoBuffsCastsNothingAtIt()
    {
        var automation = ItemEnchantAutomation();
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        host.Selection.Select(10);
        panel.AddSelectedItemNoBuffs();
        panel.ToggleCombat();
        for (int tick = 0; tick < 12; tick++)
            panel.OnTick(0.3d);

        Assert.Empty(automation.CastSpellIds);
    }

    [Fact]
    public void AnItemAlreadyEnchantedForLongerThanTheThresholdIsNotDue()
    {
        var automation = ItemEnchantAutomation();
        // The auras are self-targeted: the character carries their timers.
        automation.ActiveEnchantments =
        [
            new PluginActiveEnchantment(101u, 201u, 1, 1800d),
            new PluginActiveEnchantment(102u, 202u, 1, 1800d),
        ];
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        host.Selection.Select(10);
        panel.AddSelectedItem();
        panel.ToggleCombat();
        for (int tick = 0; tick < 12; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([103u], automation.CastSpellIds);
    }

    [Fact]
    public void ForceBuffAlsoForcesTheItemEnchantRowsBecauseEqIEndsOnDmD()
    {
        FakeAutomation automation = ItemEnchantAutomation();
        automation.ActiveEnchantments =
        [
            new PluginActiveEnchantment(101u, 201u, 1, 1800d),
            new PluginActiveEnchantment(102u, 202u, 1, 1800d),
            new PluginActiveEnchantment(103u, 203u, 1, 1800d),
        ];
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        host.Selection.Select(10);
        panel.AddSelectedItem();
        panel.ToggleCombat();
        for (int tick = 0; tick < 4; tick++)
            panel.OnTick(0.3d);
        Assert.Empty(automation.CastSpellIds);

        panel.ForceBuff();
        for (int tick = 0; tick < 12; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([101u, 102u, 103u], automation.CastSpellIds);
    }

    /// <summary>
    /// The reference buff pick walks the SELF list to exhaustion before it
    /// ever reaches the item list. A self buff that is due therefore always
    /// outranks
    /// every item enchantment.
    /// Mutation: try the item rows first in TryPickBuff and this fails.
    /// </summary>
    [Fact]
    public void EverySelfBuffIsCastBeforeAnyItemEnchantment()
    {
        var automation = ItemEnchantAutomation();
        automation.Skills =
        [
            new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
        ];
        automation.KnownSelfBuffs =
        [
            .. automation.KnownSelfBuffs,
            Spell(1, 10, "Increases the caster's Life Magic skill by 10 points."),
        ];
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        host.Selection.Select(10);
        panel.AddSelectedItem();
        panel.ToggleCombat();
        for (int tick = 0; tick < 12; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([1u, 101u, 102u, 103u], automation.CastSpellIds);
    }


    private static FakeAutomation BaneAutomation(
        params PluginSpellInfo[] known) => new()
    {
        CurrentHealth = 100,
        MaxHealth = 100,
        CurrentStamina = 100,
        MaxStamina = 100,
        CurrentMana = 100,
        MaxMana = 100,
        ObjectId = 0x50000001u,
        Skills =
        [
            new PluginSkillInfo(
                ItemEnchantmentSchoolId,
                "Item Enchantment",
                PluginSkillTraining.Trained,
                300),
        ],
        KnownSelfBuffs = known,
    };

    private static PluginSpellInfo Bane(uint id, uint family, string name) =>
        new(
            id,
            name,
            family,
            Tier: 1,
            Difficulty: 10,
            ManaCost: 5,
            DurationSeconds: 1800f,
            School: ItemEnchantmentSchoolId,
            "Increases a shield or piece of armor's resistance by 10%. "
                + "Target yourself to cast this spell on all of your equipped armor.",
            IsSelfTargeted: false,
            IsBeneficial: true);

    [Fact]
    public void ACharacterTargetedBaneIsCastOnceAndThenCoveredByTheLedger()
    {
        FakeAutomation automation = BaneAutomation(
            Bane(301, 401, "Blade Bane I"));
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        panel.ToggleCombat();
        for (int tick = 0; tick < 30; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([301u], automation.CastSpellIds);

        Assert.Empty(automation.ActiveEnchantments);
        Assert.False(
            automation.ItemEnchantments.TryGetValue(
                automation.ObjectId, out var onCharacter)
                && onCharacter.Count > 0,
            "a bane never lands on the character itself");
    }

    [Fact]
    public void ImpenetrabilityIsTheFirstCharacterRow()
    {
        FakeAutomation automation = BaneAutomation(
            Bane(311, 411, "Acid Bane I"),
            Bane(312, 412, "Impenetrability I"));
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        panel.ToggleCombat();
        for (int tick = 0; tick < 30; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([312u, 311u], automation.CastSpellIds);
    }

    [Fact]
    public void BaneRowsFollowTheProfileLettersInReadingOrder()
    {
        FakeAutomation automation = BaneAutomation(
            Bane(321, 421, "Bludgeon Bane I"),
            Bane(322, 422, "Lightning Bane I"));
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);
        panel.SetMetaOption(
            "BuffProfile_Banes",
            AcDream.Plugins.MossTank.Expressions.ExpressionValue.Number(1));
        panel.SetMetaOption(
            "BuffProfile-Banes",
            AcDream.Plugins.MossTank.Expressions.ExpressionValue.String("LB"));

        panel.ToggleCombat();
        for (int tick = 0; tick < 30; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([322u, 321u], automation.CastSpellIds);
    }

    [Fact]
    public void CharacterRowsClimbTheFamilyPastTheRenamedTiers()
    {
        PluginSpellInfo tierOne = Bane(331, 431, "Acid Bane I");
        FakeAutomation automation = BaneAutomation(
            tierOne,
            tierOne with { SpellId = 0x082Cu, Name = "Olthoi's Bane", Tier = 7, Difficulty = 300 },
            tierOne with { SpellId = 0x1127u, Name = "Incantation of Acid Bane", Tier = 8, Difficulty = 400 });
        automation.Skills =
        [
            new PluginSkillInfo(
                ItemEnchantmentSchoolId,
                "Item Enchantment",
                PluginSkillTraining.Trained,
                410),
        ];
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);
        panel.SetMetaOption(
            "BuffProfile_Banes",
            AcDream.Plugins.MossTank.Expressions.ExpressionValue.Number(8));

        panel.ToggleCombat();
        for (int tick = 0; tick < 30; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([0x1127u], automation.CastSpellIds);
    }

    [Fact]
    public void CharacterRowsAreAcceptedAgainstTheirOwnTierOneSpell()
    {
        PluginSpellInfo tierOne = Bane(331, 431, "Acid Bane I") with
        {
            ComponentSet = new PluginSpellComponentSet(7, 34, 42, 57),
        };
        PluginSpellInfo decoy = tierOne with
        {
            SpellId = 900u,
            Name = "Acid Ward Self I",
            IsSelfTargeted = true,
            Difficulty = 5,
            ComponentSet = new PluginSpellComponentSet(7, 34, 42, 61),
        };
        FakeAutomation automation = BaneAutomation(
            tierOne,
            decoy,
            tierOne with { SpellId = 0x1127u, Name = "Incantation of Acid Bane", Tier = 8, Difficulty = 400 });
        automation.Skills =
        [
            new PluginSkillInfo(
                ItemEnchantmentSchoolId,
                "Item Enchantment",
                PluginSkillTraining.Trained,
                410),
        ];
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);
        panel.SetMetaOption(
            "BuffProfile_Banes",
            AcDream.Plugins.MossTank.Expressions.ExpressionValue.Number(8));

        panel.ToggleCombat();
        for (int tick = 0; tick < 30; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([0x1127u], automation.CastSpellIds);
    }

    [Fact]
    public void TheBanePresetsAreTheBaneEnumsNotTheProtectionEnums()
    {
        FakeAutomation automation = BaneAutomation(
            Bane(331, 431, "Acid Bane I"),
            Bane(332, 432, "Blade Bane I"));
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);
        panel.SetMetaOption(
            "BuffProfile_Banes",
            AcDream.Plugins.MossTank.Expressions.ExpressionValue.Number(8));

        panel.ToggleCombat();
        for (int tick = 0; tick < 30; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([331u], automation.CastSpellIds);
    }

    [Fact]
    public void ASelfCastBaneLandsOnArmorAndStampsTheLedgerAtTheCharacter()
    {
        FakeAutomation automation = BaneAutomation(
            Bane(351, 451, "Blade Bane I"));
        automation.CastResultText =
            "You cast Blade Bane I on Alduressa Boots, refreshing Blade Bane I";
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "log Misc on", "/vt log Misc on"));

        panel.ToggleCombat();
        for (int tick = 0; tick < 30; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([351u], automation.CastSpellIds);

        Assert.Contains(
            automation.Messages,
            message => message.Contains(
                $"Cast Blade Bane I on {automation.ObjectId} ending at",
                StringComparison.Ordinal));

        Assert.Contains(
            automation.Messages,
            message => message.Contains(
                $"Buffing: character row {automation.Name} "
                    + $"({automation.ObjectId}) → Blade Bane I",
                StringComparison.Ordinal));
    }

    /// <summary>AC's Item Enchantment skill id, the aura school.</summary>
    private const uint ItemEnchantmentSchoolId = 32u;

    private static FakeAutomation ItemEnchantAutomation() => new()
    {
        CurrentHealth = 100,
        MaxHealth = 100,
        CurrentStamina = 100,
        MaxStamina = 100,
        CurrentMana = 100,
        MaxMana = 100,
        ObjectId = 0x50000001u,
        Skills = [],
        KnownSelfBuffs =
        [
            NamedSpell(101, 201, "Aura of Defender Self I", ItemEnchantmentSchoolId)
                with { IsSelfTargeted = true },
            NamedSpell(102, 202, "Aura of Hermetic Link Self I", ItemEnchantmentSchoolId)
                with { IsSelfTargeted = true },
            NamedSpell(103, 203, "Aura of Spirit Drinker Self I", ItemEnchantmentSchoolId)
                with { IsSelfTargeted = true },
        ],
        ItemEntries = [Item(10, "War Wand", 0x8000u, validLocations: 0x01000000u)],
    };

    private static PluginSpellInfo NamedSpell(
        uint id,
        uint family,
        string name,
        uint school) => new(
            id,
            name,
            family,
            Tier: 1,
            Difficulty: 10,
            ManaCost: 5,
            DurationSeconds: 1800f,
            School: school,
            "Increases a weapon's damage value by 2 points.",
            IsSelfTargeted: false,
            IsBeneficial: true);

    /// <summary>
    /// Running the fellowship -- answering tells, handling votes -- is a
    /// profile choice, and with it off the manager does nothing and says so
    /// rather than quietly watching chat.
    ///
    /// Mutation: run the manager whatever the profile says and the second row
    /// reports the in-world state instead.
    /// </summary>
    [Theory]
    [InlineData(true, "Not in a fellowship")]
    [InlineData(false, "Fellow manager disabled")]
    public void ManagingTheFellowshipIsAProfileChoice(bool manages, string status)
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        panel.SetMetaOption("AutoFellowManagement", Truthy(manages));

        panel.ToggleCombat();
        for (int tick = 0; tick < 4; tick++)
            panel.OnTick(0.3d);

        Assert.Equal(status, panel.FellowshipManagerStatus);
    }

    [Fact]
    public void RandomHelperBuffsGoToANearbyFellowWithTheSettingOn()
    {
        var automation = new CombatCapableFakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            ItemEntries = [Item(10, "War Wand", itemType: 0x00008000u)],
            EquipmentItems =
            [
                EquipmentItem(10, "War Wand", itemType: 0x00008000u),
            ],
            Skills =
            [
                new PluginSkillInfo(33, "Life Magic", PluginSkillTraining.Trained, 300),
            ],
            KnownSelfBuffs = [NamedSpell(500, 600, "Armor Other I", 33u)],
        };
        automation.CombatSnapshot = automation.CombatSnapshot with
        {
            Mode = PluginCombatMode.Magic,
        };
        automation.WorldObjects.Add(
            new PluginWorldObject(
                0x50000009u, 123u, "Fellow A", PluginObjectClass.Player, 0u, 0u, 0u)
            {
                HasPosition = true,
                Position = NavigationAt(0f).Position,
            });
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(10);
        panel.AddSelectedItem();
        panel.SetMetaOption("RandomHelperBuffs", Truthy(true));
        panel.ToggleCombat();

        for (int tick = 0; tick < 200; tick++)
            panel.OnTick(0.3d);

        Assert.Contains(0x50000009u, automation.CastTargets);
    }

    private static FakeAutomation GemFoodAutomation(string itemName = "Blackmoor's Favor") => new()
    {
        CurrentHealth = 100,
        MaxHealth = 100,
        CurrentStamina = 100,
        MaxStamina = 100,
        CurrentMana = 100,
        MaxMana = 100,
        KnownSelfBuffs =
        [
            Spell(3810, 518, "An appraised effect.") with { DurationSeconds = 600f },
            Spell(3811, 519, "A configured effect."),
        ],
        ItemEntries =
        [
            Item(101, itemName, 0x800u)
                with { AppraisedSpellIds = [3810u] },
        ],
    };

    private static string SettingsWithGemFood(params (string Name, uint SpellId)[] entries)
    {
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        VtankTable table = database.Find("GemFoodItems")!;
        int name = table.ColumnIndex("Name");
        int spell = table.ColumnIndex("Spell");
        table.Rows.Clear();
        foreach ((string itemName, uint spellId) in entries)
        {
            var row = new VtankRow();
            for (int column = 0; column < table.ColumnNames.Count; column++)
                row.Cells.Add(VtankCell.Int(0));
            row.Cells[name] = VtankCell.String(itemName);
            row.Cells[spell] = VtankCell.Int(unchecked((int)spellId));
            table.Rows.Add(row);
        }
        return database.Render();
    }

    private static string SettingsWithoutGemFood()
    {
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        database.Tables.RemoveAll(static entry => entry.Name == "GemFoodItems");
        return database.Render();
    }

    private static VtankRow SettingRow(VtankTable table, string name, VtankCell value)
    {
        var row = new VtankRow();
        for (int column = 0; column < table.ColumnNames.Count; column++)
            row.Cells.Add(VtankCell.Int(0));
        row.Cells[table.ColumnIndex("Setting")] = VtankCell.String(name);
        row.Cells[table.ColumnIndex("Value")] = value;
        return row;
    }

    private static VtankRow ExemplarRow(int spellId)
    {
        var row = new VtankRow();
        row.Cells.Add(VtankCell.Int(spellId));
        return row;
    }

    private static FakeAutomation BuffPassAutomation() => new()
    {
        CurrentHealth = 100,
        MaxHealth = 100,
        CurrentStamina = 100,
        MaxStamina = 100,
        CurrentMana = 100,
        MaxMana = 100,
        Skills =
        [
            new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
            new PluginSkillInfo(2, "War Magic", PluginSkillTraining.Trained, 300),
            new PluginSkillInfo(3, "Item Tinkering", PluginSkillTraining.Trained, 300),
        ],
        KnownSelfBuffs =
        [
            Spell(1, 10, "Increases the caster's Life Magic skill by 10 points."),
            Spell(2, 20, "Increases the caster's War Magic skill by 10 points."),
            Spell(3, 30, "Increases the caster's Item Tinkering skill by 10 points."),
        ],
    };

    [Fact]
    public void MacroWieldsCasterEntersMagicBuffsThenWieldsWeaponFightsThenIdlePeace()
    {
        PluginSpellInfo buff = Spell(
            1, 10, "Increases the caster's Life Magic skill by 10 points.");
        var automation = new CombatCapableFakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills = [new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300)],
            KnownSelfBuffs = [buff],
            ItemEntries =
            [
                Item(10, "War Wand", itemType: 0x00008000u),
                Item(20, "Battle Axe", itemType: 1),
            ],
            EquipmentItems =
            [
                EquipmentItem(10, "War Wand", itemType: 0x00008000u),
                EquipmentItem(20, "Battle Axe", itemType: 1),
            ],
            Targets = [new PluginCombatTarget(30, "Drudge", 700, 2f, 0f, true, 1f)],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        host.Selection.Select(10);
        panel.AddSelectedItem(); // Items profile: the wand
        host.Selection.Select(20);
        panel.AddSelectedItem(); // Items profile: the axe
        panel.CycleMonsterWeaponAt(0);
        panel.ToggleIdlePeaceMode();

        panel.ToggleCombat(); // Run Macro, starting in Peace

        bool attacked = false;
        for (int tick = 0; tick < 60 && !attacked; tick++)
        {
            panel.OnTick(0.7);
            attacked = automation.BeginCount > 0;
        }
        Assert.True(
            attacked,
            "Never attacked. CallLog: " + string.Join(" | ", automation.CallLog));

        int equipWand = automation.CallLog.IndexOf("Equip:0000000A");
        int enterMagic = automation.CallLog.IndexOf("EnterMode:Magic");
        int cast = automation.CallLog.IndexOf("Cast:1");
        int equipWeapon = automation.CallLog.IndexOf("Equip:00000014");
        int defaultMode = automation.CallLog.FindIndex(
            entry => entry == "EnterMode:Melee");
        int attack = automation.CallLog.IndexOf("Attack:0000001E");

        // Peace(already) -> Equip wand: no separate peace request was needed
        // for the wand because the macro started in Peace already.
        Assert.True(equipWand >= 0, "wand was never equipped");
        Assert.DoesNotContain(
            "EnterMode:Peace",
            automation.CallLog.Take(equipWand));
        Assert.True(enterMagic > equipWand, "Magic requested before the wand was wielded");
        Assert.True(cast > enterMagic, "cast happened before Magic mode was entered");
        Assert.True(
            equipWeapon > cast,
            "the weapon was wielded before the buff was cast: "
                + string.Join(" | ", automation.CallLog));
        Assert.True(
            defaultMode > equipWeapon,
            "combat mode entered before the weapon was equipped: "
                + string.Join(" | ", automation.CallLog));
        Assert.DoesNotContain("EnterDefaultMode:Melee", automation.CallLog);
        Assert.True(attack > defaultMode, "attack began before the default mode was entered");

        // With the hostile gone and Peace Mode When Idle on, the macro
        // returns to peace by itself.
        automation.Targets = [];
        for (int tick = 0;
             tick < 60 && automation.CombatSnapshot.Mode != PluginCombatMode.Peace;
             tick++)
        {
            panel.OnTick(0.5);
        }
        Assert.Equal(PluginCombatMode.Peace, automation.CombatSnapshot.Mode);
    }

    [Fact]
    public void ACastInFlightFreezesTheWholeMainTrackNotJustTheCastingRules()
    {
        var automation = new CombatCapableFakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
        };
        automation.CombatSnapshot = automation.CombatSnapshot with
        {
            Mode = PluginCombatMode.Melee,
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.ToggleIdlePeaceMode();
        panel.ToggleCombat();
        // A cast this macro issued and is still waiting on.
        SpellCastTracker tracker = ((IBuffRuleHost)panel).CastTracker;
        tracker.Begin(1u, "Strength Self", 0u, string.Empty, false, issueRevision: 0L);

        for (int tick = 0; tick < 5; tick++)
            panel.OnTick(0.3d);

        Assert.DoesNotContain("EnterMode:Peace", automation.CallLog);
        Assert.Equal(PluginCombatMode.Melee, automation.CombatSnapshot.Mode);

        tracker.Reset();
        panel.OnTick(0.01d);
        panel.OnTick(0.01d);

        Assert.Contains("EnterMode:Peace", automation.CallLog);
    }

    /// <summary>
    /// The host's casting flag is its inventory busy count under another
    /// name: every open, pickup and appraisal raises it. The reference's
    /// global busy count is raised by none of those, so the flag alone must
    /// not hold the pass. Mutation: put <c>Magic.IsCasting</c> back into the
    /// suspension predicate and the pass never reaches the peace rule.
    /// </summary>
    [Fact]
    public void TheHostsInventoryBusyFlagAloneDoesNotSuspendThePass()
    {
        var automation = new CombatCapableFakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            IsCasting = true,
        };
        automation.CombatSnapshot = automation.CombatSnapshot with
        {
            Mode = PluginCombatMode.Melee,
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.ToggleIdlePeaceMode();
        panel.ToggleCombat();

        for (int tick = 0; tick < 5; tick++)
            panel.OnTick(0.3d);

        Assert.Contains("EnterMode:Peace", automation.CallLog);
    }

    /// <summary>
    /// The reference has two self-recharge rows: the normal thresholds at
    /// row 4 and the no-target thresholds at row 59, below loot, the buff
    /// top-off and the monster approach. Both are live rules here.
    /// </summary>
    [Fact]
    public void BothSelfRechargeRowsAreLiveRules()
    {
        var panel = new MossTankPanel(new FakeHost(new CombatCapableFakeAutomation()));
        IMacroRule normal = panel.MacroRules.First(static rule => rule.Name == "RechargeSelfNormal");
        IMacroRule idle = panel.MacroRules.First(static rule => rule.Name == "RechargeSelfNoTarget");
        Assert.IsType<ControllerMacroRule>(normal);
        Assert.IsType<ControllerMacroRule>(idle);
    }

    /// <summary>
    /// The reference gates the route rule's idle-peace fallback on normal
    /// movement being allowed: inside the creep band of the waypoint the walk
    /// itself runs (and pushes into magic mode); idle peace is not asked for
    /// there. Mutation: leave the fallback ungated and peace is requested.
    /// </summary>
    [Fact]
    public void InsideTheCreepBandTheRouteWalksInsteadOfAskingForPeace()
    {
        var automation = new CombatCapableFakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
        };
        automation.CombatSnapshot = automation.CombatSnapshot with
        {
            Mode = PluginCombatMode.Melee,
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        // A waypoint one metre north: inside the creep band.
        automation.NavigationSnapshot = automation.NavigationSnapshot with
        {
            Position = new PluginNavigationPosition(
                0x00010001u, 0d, 1d / 240d, 0d, 0f, IsOutdoor: true),
        };
        panel.AddRoutePoint();
        automation.NavigationSnapshot = automation.NavigationSnapshot with
        {
            Position = new PluginNavigationPosition(
                0x00010001u, 0d, 0d, 0d, 0f, IsOutdoor: true),
        };
        panel.ToggleNavigation();
        panel.ToggleIdlePeaceMode();
        panel.ToggleCombat();

        for (int tick = 0; tick < 4; tick++)
            panel.OnTick(0.3d);

        Assert.DoesNotContain("EnterMode:Peace", automation.CallLog);
        IMacroRule route = panel.MacroRules.First(
            static rule => rule.Name == "NavigateRouteIdle");
        Assert.True(route.Running);
    }

    /// <summary>
    /// The reference wires an idle-peace fallback on the monster approach,
    /// gated on the monster being outside the creep band.
    /// </summary>
    [Fact]
    public void TheMonsterApproachCarriesTheIdlePeaceFallback()
    {
        var panel = new MossTankPanel(new FakeHost(new CombatCapableFakeAutomation()));
        IMacroRule approach = panel.MacroRules.First(
            static rule => rule.Name == "NavigateMonster");
        Assert.IsType<MacroRulePreChain>(approach);
    }

    [Fact]
    public void VtLogActiveRuleOnPostsThePickedLineNamingTheWinner()
    {
        var automation = new CombatCapableFakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
        };
        automation.CombatSnapshot = automation.CombatSnapshot with
        {
            Mode = PluginCombatMode.Melee,
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.ToggleIdlePeaceMode();
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "log ActiveRule on", "/vt log ActiveRule on"));
        automation.Messages.Clear(); // drop "/vt log"'s own "Set ActiveRule" echo
        panel.ToggleCombat();

        panel.OnTick(0.1d);

        Assert.Contains(
            "[MossTank] Picked IdlePeace P: 65   I=False, N=False, S=False",
            automation.Messages);
    }

    /// <summary>
    /// The reference's busy count is raised by a spell cast, a wand cast and
    /// a kit or craft use, never by an open, a pickup or an identify. Here
    /// the host's inventory flag is up (an identify is outstanding) and the
    /// pass still runs. Mutation: put <c>automation.Items.IsBusy</c> back
    /// into the in-flight test of <c>ObserveCastSuspension</c> and the pass
    /// line never appears.
    /// </summary>
    [Fact]
    public void AnOutstandingInventoryRequestDoesNotStopThePass()
    {
        var automation = new CombatCapableFakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            ItemsBusy = true,
        };
        automation.CombatSnapshot = automation.CombatSnapshot with
        {
            Mode = PluginCombatMode.Melee,
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.ToggleIdlePeaceMode();
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "log ActiveRule on", "/vt log ActiveRule on"));
        automation.Messages.Clear();
        panel.ToggleCombat();

        panel.OnTick(0.1d);
        panel.OnTick(0.3d);

        Assert.Contains(
            "[MossTank] Picked IdlePeace P: 65   I=False, N=False, S=False",
            automation.Messages);
    }

    /// <summary>
    /// The reference gates a buff on the item-use slot and on whether a buff
    /// is due, never on the host's inventory transaction state. The two are
    /// not the same thing here: a cast raises that transaction count on its
    /// way out, so a buff rule watching it declines for several passes after
    /// each of its OWN casts, and every one of those passes falls through to
    /// whatever wants it next -- which, standing over a corpse, is the open
    /// rule. That is the "it tries to open a corpse between every spell"
    /// report. Mutation: gate <c>BuffSelfRule</c> on
    /// <c>automation.Items.IsBusy</c> again and the rule declines instead of
    /// claiming the pass.
    /// </summary>
    [Fact]
    public void AnOutstandingInventoryRequestDoesNotStopTheBuffRule()
    {
        FakeAutomation automation = BuffPassAutomation();
        automation.ItemsBusy = true;
        automation.IsCasting = true;
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "log ActiveRule on", "/vt log ActiveRule on"));
        automation.Messages.Clear();

        panel.ToggleCombat();
        panel.OnTick(0d);

        // The rule keeps the pass AND casts. The host's flags are its
        // inventory transaction count under two names, raised by appraisals
        // and pickups as much as by casts; a request that never completes
        // leaves the count raised for good, and a rule that waits on it
        // claims every pass, casts nothing, and starves every rule below.
        Assert.Contains(
            automation.Messages,
            line => line.Contains("Picked BuffSelf", StringComparison.Ordinal));
        Assert.NotEmpty(automation.CastSpellIds);
    }

    [Fact]
    public void VtLogActiveRuleOnPostsAllRulesInactiveWhenNothingIsValid()
    {
        var automation = new CombatCapableFakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
        };
        automation.CombatSnapshot = automation.CombatSnapshot with
        {
            Mode = PluginCombatMode.Peace,
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.ToggleIdlePeaceMode(); // valid only outside Peace — stays silent
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "log ActiveRule on", "/vt log ActiveRule on"));
        automation.Messages.Clear();
        panel.ToggleCombat();

        panel.OnTick(0.1d);

        Assert.Contains(
            "[MossTank] All rules inactive.   I=False, N=False, S=False",
            automation.Messages);
    }

    /// <summary>
    /// Mutation: delete the Attack rule's
    /// <c>gate: () =&gt; !_actionLocks.IsLocked(ActionLockKind.ItemUse)</c> and
    /// the first assertion fails — the bot keeps swinging inside the item's
    /// own cooldown.
    /// </summary>
    [Fact]
    public void AnItemUseLockHoldsTheAttackRuleOffUntilItExpires()
    {
        var automation = new CombatCapableFakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            ItemEntries = [Item(20, "Battle Axe", itemType: 1)],
            EquipmentItems = [EquipmentItem(20, "Battle Axe", itemType: 1)],
            Targets = [new PluginCombatTarget(30, "Drudge", 700, 2f, 0f, true, 1f)],
        };
        automation.CombatSnapshot = automation.CombatSnapshot with
        {
            Mode = PluginCombatMode.Melee,
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(20);
        panel.AddSelectedItem();
        panel.CycleMonsterWeaponAt(0);
        panel.ToggleCombat();

        bool attacked = false;
        for (int tick = 0; tick < 60 && !attacked; tick++)
        {
            panel.OnTick(0.7d);
            attacked = automation.BeginCount > 0;
        }
        Assert.True(
            attacked,
            "the rig never attacks at all. CallLog: "
                + string.Join(" | ", automation.CallLog));

        panel.ActionLocks.Arm(ActionLockKind.ItemUse, 5d);
        int before = automation.BeginCount;
        for (int tick = 0; tick < 5; tick++)
            panel.OnTick(0.7d);

        Assert.Equal(before, automation.BeginCount);

        for (int tick = 0; tick < 5; tick++)
            panel.OnTick(0.7d);

        Assert.True(
            automation.BeginCount > before,
            "the attack never resumed after the item-use lock expired");
    }

    /// <summary>
    /// The item slot is not the attack's alone: every rule that consumes an
    /// item reads it first, which is what makes one shared release safe — with
    /// only one owner at a time, no rule can put down a window another one is
    /// holding. Here the slot is held by somebody else and the self-recharge
    /// eats nothing until it comes free.
    /// Mutation: drop <c>ItemSlotIsFree() &amp;&amp;</c> from
    /// <c>RechargeSelfNormal</c>'s gate and the first assertion fails — the
    /// bread is eaten inside another owner's window, and the recharge's own
    /// release then deletes that owner's deadline.
    /// </summary>
    [Fact]
    public void TheItemSlotHoldsTheConsumableRulesOffAsWellAsTheAttack()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            // BoosterVital 2 = VitalKind.Health.
            ItemEntries = [Item(60, "Bread", 0x20) with { BoosterVital = 2 }],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(60u);
        panel.AddSelectedConsumable();
        panel.ToggleCombat();
        // The macro clears every slot as it starts, so the window opens after
        // the first frame.
        panel.OnTick(0d);

        // Somebody else's window, and only now is health worth a bite: two
        // seconds of passes eat nothing.
        panel.ActionLocks.Arm(ActionLockKind.ItemUse, 5d);
        automation.CurrentHealth = 10;
        for (int tick = 0; tick < 6; tick++)
            panel.OnTick(0.3d);
        Assert.Empty(automation.UsedItemIds);

        panel.ActionLocks.Release(ActionLockKind.ItemUse);
        for (int tick = 0; tick < 6; tick++)
            panel.OnTick(0.3d);
        Assert.Equal([60u], automation.UsedItemIds);
    }

    /// <summary>
    /// The whole point of the hold: a bite eaten, the server's answer, and
    /// the next bite, all inside a second. The hold is what stops the pass,
    /// and the pass used to be the only place the answer was ever read — so
    /// the hold waited on the thing it had stopped and ended only on its
    /// watchdog, more than eight seconds after the server had answered, with
    /// the character standing still for all of it. Mutation: delete the
    /// <c>_vitalRecharge.ObservePendingReceipt</c> call from
    /// <c>ObservePendingTransactions</c> and the second bite never comes.
    /// </summary>
    [Fact]
    public void AnsweredConsumableUsesFollowOneAnotherInsideASecond()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            // BoosterVital 2 = VitalKind.Health.
            ItemEntries = [Item(60, "Bread", 0x20) with { BoosterVital = 2 }],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(60u);
        panel.AddSelectedConsumable();
        panel.ToggleCombat();
        panel.OnTick(0d);

        automation.CurrentHealth = 10;
        for (int tick = 0; tick < 6; tick++)
            panel.OnTick(0.1d);
        Assert.Equal([60u], automation.UsedItemIds);

        // The server answers, and one second of frames follows — a fraction of
        // the suspension watchdog.
        automation.ItemCompletion = new PluginItemUseCompletion(1L, 60u, 0u, 0u);
        for (int tick = 0; tick < 10; tick++)
            panel.OnTick(0.1d);

        Assert.Equal([60u, 60u], automation.UsedItemIds);
    }

    /// <summary>
    /// The wand's cast holds the very slot the attack's first refusal reads,
    /// so from the pass after it starts the attack has no turn at all. The
    /// cast is nobody else's business but its own: it keeps being watched
    /// across those turnless passes, sees its own confirmation in the magic
    /// log, and puts the slot down early — driven here through the real rule
    /// table, not by calling the controller.
    /// Mutation: empty <c>CombatController.ObserveHeldItemCast</c> (the frame
    /// driver that watches the held item's cast while the attack has no turn)
    /// and the last two assertions fail — the slot stays locked for its whole
    /// eleven-and-a-half seconds and the attack stays paused.
    /// </summary>
    [Fact]
    public void AWandCastIsWatchedToItsEndThoughItsOwnSlotHoldsTheAttackOff()
    {
        var imperil = new PluginSpellInfo(
            90u,
            "Imperil Other VII",
            Family: 1,
            Tier: 8,
            Difficulty: 350,
            ManaCost: 30,
            DurationSeconds: 60,
            School: 31,
            Description: string.Empty,
            IsSelfTargeted: false,
            IsBeneficial: false)
        {
            IsDebuff = true,
            IsOffensive = true,
            BaseRangeConstant = 80f,
        };
        var automation = new CombatCapableFakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentMana = 100,
            MaxMana = 100,
            ItemEntries =
            [
                Item(800, "Imperil Lens", itemType: 0x8000) with
                {
                    EquippedLocation = 0x00100000u,
                    SpellId = 90u,
                    ItemSpellcraft = 400,
                },
            ],
            EquipmentItems = [EquipmentItem(800, "Imperil Lens", itemType: 0x8000)],
            Targets = [new PluginCombatTarget(30, "Drudge", 700, 2f, 0f, true, 1f)],
        };
        automation.SpellLookup.Add(imperil);
        automation.CombatSnapshot = automation.CombatSnapshot with
        {
            Mode = PluginCombatMode.Magic,
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(800);
        panel.AddSelectedItem();
        panel.ToggleMonsterImperilAt(0);
        panel.SetMetaOption("EnableBuffing", Truthy(false));
        panel.ToggleCombat();

        for (int tick = 0; tick < 40 && automation.ApplyCount == 0; tick++)
            panel.OnTick(0.3d);
        Assert.True(
            automation.ApplyCount > 0,
            "the rig never used the wand at all. CallLog: "
                + string.Join(" | ", automation.CallLog));
        Assert.Equal((800u, 30u), automation.LastAppliedItem);
        Assert.True(panel.ActionLocks.IsLocked(ActionLockKind.ItemUse));

        // Several passes with the attack's gate shut. Two seconds in, well
        // short of the cast's own window.
        for (int tick = 0; tick < 7; tick++)
            panel.OnTick(0.3d);
        Assert.True(panel.ActionLocks.IsLocked(ActionLockKind.ItemUse));

        automation.PostChat(
            "You cast Imperil Other VII on Drudge.",
            logTextType: 0x07u);
        panel.OnTick(0.3d);

        Assert.False(panel.ActionLocks.IsLocked(ActionLockKind.ItemUse));
        // The slot came down on the frame, ahead of the pass, so the attack
        // had its turn back on that very pass and is no longer paused.
        Assert.DoesNotContain(
            "Paused",
            panel.CombatStatus,
            StringComparison.Ordinal);
    }

    /// <summary>
    /// A weapon proc's debuff rides a physical swing, and it arms no item
    /// slot of its own. So when some OTHER rule takes the item slot — a kit,
    /// a dispel item, the mode-gate's recovery — the attack loses its turn,
    /// its swing is aborted, and that is the end of it: nothing may go on to
    /// release a swing that is no longer running, least of all inside the
    /// window the other rule took the slot for.
    /// Mutation: widen the turnless branch at the top of
    /// <c>CombatController.OnTick</c> back to <c>_pendingItemDebuff is not
    /// null</c> (dropping the <c>CasterItem</c> pattern) and the last
    /// assertion fails — the charge is released on the very passes the
    /// attack has no turn on.
    /// </summary>
    [Fact]
    public void AProcChargeIsNotReleasedOnAPassTheAttackLost()
    {
        var imperil = new PluginSpellInfo(
            91u,
            "Imperil Other VII",
            Family: 1,
            Tier: 8,
            Difficulty: 350,
            ManaCost: 30,
            DurationSeconds: 60,
            School: 31,
            Description: string.Empty,
            IsSelfTargeted: false,
            IsBeneficial: false)
        {
            IsDebuff = true,
            IsOffensive = true,
            BaseRangeConstant = 80f,
        };
        var automation = new CombatCapableFakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentMana = 100,
            MaxMana = 100,
            ItemEntries =
            [
                Item(801, "Imperil Sword", itemType: 0x0001) with
                {
                    EquippedLocation = 0x00100000u,
                    ItemSpellcraft = 400,
                    AppraisedSpellIds = [91u],
                },
            ],
            EquipmentItems = [EquipmentItem(801, "Imperil Sword", itemType: 0x0001)],
            Targets = [new PluginCombatTarget(30, "Drudge", 700, 2f, 0f, true, 1f)],
        };
        automation.SpellLookup.Add(imperil);
        automation.CombatSnapshot = automation.CombatSnapshot with
        {
            Mode = PluginCombatMode.Melee,
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(801);
        panel.AddSelectedItem();
        panel.ToggleMonsterImperilAt(0);
        panel.SetMetaOption("EnableBuffing", Truthy(false));
        panel.ToggleCombat();

        for (int tick = 0; tick < 40 && automation.BeginCount == 0; tick++)
            panel.OnTick(0.3d);
        Assert.True(
            automation.BeginCount > 0,
            "the rig never began the proc's swing at all. CallLog: "
                + string.Join(" | ", automation.CallLog));
        Assert.Equal(30u, automation.LastBeginTarget);
        // The proc arms nothing: the slot is free until someone else takes it.
        Assert.False(panel.ActionLocks.IsLocked(ActionLockKind.ItemUse));
        Assert.Equal(0, automation.ReleaseCount);

        // Another rule takes the item slot for its own five-second window, and
        // the swing it interrupted is sitting at full power.
        panel.ActionLocks.Arm(ActionLockKind.ItemUse, 5d);
        automation.CombatSnapshot = automation.CombatSnapshot with
        {
            RequestInProgress = true,
            BuildInProgress = true,
            PowerBarLevel = 1f,
        };
        for (int tick = 0; tick < 8; tick++)
            panel.OnTick(0.3d);

        Assert.True(panel.ActionLocks.IsLocked(ActionLockKind.ItemUse));
        Assert.Equal(0, automation.ReleaseCount);
    }

    /// <summary>
    /// Mutation: drop <c>NavigationLocksAreClear()</c> from the two navigate
    /// gates and this fails — the route advances over the corpse the kill just
    /// made.
    /// </summary>
    [Fact]
    public void ANavigationLockHoldsTheRouteRuleOff()
    {
        var automation = new CombatCapableFakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        // One waypoint the character is nowhere near, so the route rule has
        // something to do on every pass.
        automation.NavigationSnapshot = automation.NavigationSnapshot with
        {
            Position = new PluginNavigationPosition(
                0x00010001u, 0d, 1d, 0d, 0f, IsOutdoor: true),
        };
        panel.AddRoutePoint();
        automation.NavigationSnapshot = automation.NavigationSnapshot with
        {
            Position = new PluginNavigationPosition(
                0x00010001u, 0d, 0d, 0d, 0f, IsOutdoor: true),
        };
        panel.ToggleNavigation();
        panel.ToggleCombat();
        panel.OnTick(0.3d);

        IMacroRule route = panel.MacroRules.First(
            static rule => rule.Name == "NavigateRouteIdle");
        var context = new MacroPassContext(0.3d, CanAct: true);
        Assert.True(
            route.ValidNow(in context),
            "the rig's route rule is not valid even with every lock clear");

        panel.ActionLocks.Arm(ActionLockKind.Navigation, 3d);
        Assert.False(route.ValidNow(in context));

        panel.ActionLocks.Release(ActionLockKind.Navigation);
        panel.ActionLocks.Arm(ActionLockKind.DoorOpening, 3d);
        Assert.False(route.ValidNow(in context));

        panel.ActionLocks.Release(ActionLockKind.DoorOpening);
        panel.ActionLocks.Arm(ActionLockKind.SpreadLockTargetRequested, 3d);
        Assert.False(route.ValidNow(in context));
    }

    /// <summary>
    /// Mutation: build the attack's candidates out to the approach range again
    /// and this fails — the attack claims the pass for a monster it cannot
    /// reach and every rule below it starves.
    /// </summary>
    [Fact]
    public void AMonsterOutOfWeaponRangeDoesNotStarveTheRulesBelowTheAttack()
    {
        var automation = new CombatCapableFakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            ItemEntries = [Item(20, "Battle Axe", itemType: 1)],
            EquipmentItems = [EquipmentItem(20, "Battle Axe", itemType: 1)],
            Targets = [new PluginCombatTarget(30, "Drudge", 700, 12f, 0f, true, 1f)],
        };
        automation.CombatSnapshot = automation.CombatSnapshot with
        {
            Mode = PluginCombatMode.Melee,
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(20);
        panel.AddSelectedItem();
        panel.CycleMonsterWeaponAt(0);
        panel.SetApproachRangeText("20");
        // Navigation off, so the monster-approach rule cannot claim the pass
        // either: what runs has to come from below both of them.
        panel.ToggleIdlePeaceMode();
        panel.ToggleCombat();

        for (int tick = 0; tick < 20; tick++)
            panel.OnTick(0.3d);

        Assert.Equal(0, automation.BeginCount);
        Assert.Contains("EnterMode:Peace", automation.CallLog);
    }

    [Fact]
    public void MonsterApproachSitsBelowIdleBuffAndAboveTheRoute()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        List<IMacroRule> rules = [.. panel.MacroRules];

        int idleBuff = rules.FindIndex(static rule => rule.Name == "BuffSelfIdle");
        int approach = rules.FindIndex(
            static rule => rule.Name == "NavigateMonster");
        int route = rules.FindIndex(
            static rule => rule.Name == "NavigateRouteIdle");

        Assert.True(idleBuff >= 0 && approach >= 0 && route >= 0);
        Assert.True(
            idleBuff < approach,
            $"idle buff top-off at {idleBuff} must outrank the approach at {approach}.");
        Assert.True(
            approach < route,
            $"the approach at {approach} must outrank the route at {route}.");
    }

    [Fact]
    public void VtLogActiveRuleOffPostsNothing()
    {
        var automation = new CombatCapableFakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
        };
        automation.CombatSnapshot = automation.CombatSnapshot with
        {
            Mode = PluginCombatMode.Melee,
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.ToggleIdlePeaceMode();
        panel.ToggleCombat();

        panel.OnTick(0.1d);

        Assert.DoesNotContain(automation.Messages, message => message.StartsWith(
            "[MossTank] Picked", StringComparison.Ordinal));
        Assert.DoesNotContain(automation.Messages, message => message.StartsWith(
            "[MossTank] All rules inactive", StringComparison.Ordinal));
    }

    [Fact]
    public void VtLogRuleInfoOnPostsRuleRunningLine()
    {
        var automation = new CombatCapableFakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
        };
        automation.CombatSnapshot = automation.CombatSnapshot with
        {
            Mode = PluginCombatMode.Melee,
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.ToggleIdlePeaceMode();
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "log RuleInfo on", "/vt log RuleInfo on"));
        automation.Messages.Clear();
        panel.ToggleCombat();

        panel.OnTick(0.1d);

        Assert.Contains("[MossTank] (IdlePeace) Running", automation.Messages);
    }

    /// <summary>
    /// The other half of the sink: turning a type off stops posting it
    /// again, and re-uses the exact <c>Set</c>/<c>Reset</c> echo text
    /// <c>/vt log</c> itself already had.
    /// </summary>
    [Fact]
    public void VtLogOffStopsPostingAfterHavingBeenOn()
    {
        var automation = new CombatCapableFakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
        };
        automation.CombatSnapshot = automation.CombatSnapshot with
        {
            Mode = PluginCombatMode.Melee,
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.ToggleIdlePeaceMode();
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "log ActiveRule on", "/vt log ActiveRule on"));
        panel.ToggleCombat();
        panel.OnTick(0.1d);
        Assert.Contains(automation.Messages, message => message.StartsWith(
            "[MossTank] Picked", StringComparison.Ordinal));

        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "log ActiveRule off", "/vt log ActiveRule off"));
        automation.Messages.Clear();
        panel.OnTick(0.1d);

        Assert.DoesNotContain(automation.Messages, message => message.StartsWith(
            "[MossTank] Picked", StringComparison.Ordinal));
    }

    [Fact]
    public void AStuckTransactionReleasesTheSuspensionOnVtanksTrackerWatchdog()
    {
        var automation = new CombatCapableFakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
        };
        automation.CombatSnapshot = automation.CombatSnapshot with
        {
            Mode = PluginCombatMode.Melee,
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.ToggleIdlePeaceMode();
        panel.ToggleCombat();
        // A cast the server never answers: the tracker's own budget ends it.
        ((IBuffRuleHost)panel).CastTracker.Begin(
            1u, "Strength Self", 0u, string.Empty, false, issueRevision: 0L);

        for (int tick = 0; tick < 34; tick++)
            panel.OnTick(0.3d);

        Assert.Contains("EnterMode:Peace", automation.CallLog);
    }

    /// <summary>
    /// The reference's death keeps the macro running and turns the four
    /// switches off, saved for the restore verb. Mutation: stop the macro
    /// instead and the first assertion fails.
    /// </summary>
    [Fact]
    public void DeathWithStopMacroOnDeathKeepsTheMacroAndDisablesTheFourSwitches()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.SetMetaOption("StopMacroOnDeath", Truthy(true));
        panel.SetMetaOption("EnableNav", Truthy(true));
        panel.SetMetaOption("EnableLooting", Truthy(true));
        panel.SetMetaOption("EnableBuffing", Truthy(true));
        panel.SetMetaOption("EnableCombat", Truthy(true));
        panel.ToggleCombat();
        panel.OnTick(0.1d);
        Assert.True(panel.CombatMacroRunning);

        automation.CurrentHealth = 0;
        panel.OnTick(0.1d);

        Assert.True(panel.CombatMacroRunning);
        Assert.False(panel.GetMetaOptionForTest("EnableNav"));
        Assert.False(panel.GetMetaOptionForTest("EnableLooting"));
        Assert.False(panel.GetMetaOptionForTest("EnableBuffing"));
        Assert.False(panel.GetMetaOptionForTest("EnableCombat"));
        Assert.Contains(
            automation.Messages,
            static value => value.Contains("deathrestore", StringComparison.Ordinal));

        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "deathrestore", "/vt deathrestore"));

        Assert.True(panel.GetMetaOptionForTest("EnableNav"));
        Assert.True(panel.GetMetaOptionForTest("EnableLooting"));
        Assert.True(panel.GetMetaOptionForTest("EnableBuffing"));
        Assert.True(panel.GetMetaOptionForTest("EnableCombat"));
        Assert.Contains(
            automation.Messages,
            static value => value.Contains(
                "back to their earlier values", StringComparison.Ordinal));
    }

    /// <summary>The reject branch of the same handler: nothing at all.</summary>
    [Fact]
    public void DeathWithStopMacroOnDeathOffChangesNothing()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.SetMetaOption("StopMacroOnDeath", Truthy(false));
        panel.SetMetaOption("EnableNav", Truthy(true));
        panel.ToggleCombat();
        panel.OnTick(0.1d);

        automation.CurrentHealth = 0;
        panel.OnTick(0.1d);

        Assert.True(panel.CombatMacroRunning);
        Assert.True(panel.GetMetaOptionForTest("EnableNav"));
        Assert.DoesNotContain(
            automation.Messages,
            static value => value.Contains("stopped because", StringComparison.Ordinal));
    }

    /// <summary>
    /// Stopping the macro is not forgetting the round. The reference's stop
    /// leaves the route's position alone; where the round begins again is the
    /// start's business, not the stop's.
    /// </summary>
    [Fact]
    public void StoppingTheMacroKeepsTheWaypointTheRouteWasWalkingTo()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation));
        automation.CurrentHealth = 100;
        automation.MaxHealth = 100;
        TwoPointRoute(panel, automation);
        panel.SetMetaOption("EnableNav", Truthy(true));
        panel.ToggleCombat();
        for (int pass = 0; pass < 4; pass++)
            panel.OnTick(0.3d);
        // Stand between the two points, so a route that had gone back to its
        // first one could not quietly advance past it again and look like a
        // route that had never moved.
        StandAt(automation, 25d);
        Assert.Equal(1, panel.RouteWaypointIndexForTest);

        panel.ExecuteVtankCommand(new PluginCommand("vt", "stop", "/vt stop"));

        Assert.False(panel.CombatMacroRunning);
        Assert.Equal(1, panel.RouteWaypointIndexForTest);
    }

    /// <summary>
    /// The same thing through the death path, which is the way a player
    /// actually meets it: die on the way to waypoint two, and the round is
    /// still on waypoint two afterwards.
    /// </summary>
    [Fact]
    public void DeathLeavesTheRouteOnTheWaypointItWasWalkingTo()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation));
        automation.CurrentHealth = 100;
        automation.MaxHealth = 100;
        TwoPointRoute(panel, automation);
        panel.SetMetaOption("EnableNav", Truthy(true));
        panel.ToggleCombat();
        for (int pass = 0; pass < 4; pass++)
            panel.OnTick(0.3d);
        StandAt(automation, 25d);
        Assert.Equal(1, panel.RouteWaypointIndexForTest);

        automation.CurrentHealth = 0;
        panel.OnTick(0.1d);

        Assert.True(panel.CombatMacroRunning);
        Assert.Equal(1, panel.RouteWaypointIndexForTest);
    }

    /// <summary>
    /// Starting decides where the round begins, and it is the nearest point
    /// of the route the character could walk to — not wherever the last run
    /// stopped. A character that died and woke at a lifestone, or simply
    /// walked away, picks the round up beside itself.
    /// </summary>
    [Fact]
    public void StartingTheMacroAnchorsTheRoundToTheNearestPointOfTheRoute()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation));
        automation.CurrentHealth = 100;
        automation.MaxHealth = 100;
        TwoPointRoute(panel, automation);
        panel.SetMetaOption("EnableNav", Truthy(true));

        StandAt(automation, 48d);
        panel.ExecuteVtankCommand(new PluginCommand("vt", "start", "/vt start"));
        Assert.Equal(1, panel.RouteWaypointIndexForTest);

        panel.ExecuteVtankCommand(new PluginCommand("vt", "stop", "/vt stop"));
        StandAt(automation, 2d);
        panel.ExecuteVtankCommand(new PluginCommand("vt", "start", "/vt start"));

        Assert.Equal(0, panel.RouteWaypointIndexForTest);
    }

    /// <summary>
    /// Only places are candidates. A pause is something to do, not somewhere
    /// to be, so the coordinate it happens to carry does not make it the
    /// nearest point of the route.
    /// </summary>
    [Fact]
    public void OnlyThePlacesOnARouteCanAnchorTheRound()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation));
        automation.CurrentHealth = 100;
        automation.MaxHealth = 100;
        panel.AddRoutePoint();                 // 0: a place, at 0
        StandAt(automation, 50d);
        panel.AddRoutePoint();                 // 1: a place, at 50
        StandAt(automation, 99d);
        panel.AddRoutePause();                 // 2: not a place, at 99
        panel.SetMetaOption("EnableNav", Truthy(true));

        StandAt(automation, 98d);
        panel.ExecuteVtankCommand(new PluginCommand("vt", "start", "/vt start"));

        Assert.Equal(1, panel.RouteWaypointIndexForTest);
    }

    /// <summary>
    /// Nearest is nearest in three dimensions. Two points of a dungeon route
    /// can sit one above the other, and the one on your own floor is the one
    /// you are at.
    /// </summary>
    [Fact]
    public void HeightCountsWhenPickingTheNearestPointOfTheRoute()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation));
        automation.CurrentHealth = 100;
        automation.MaxHealth = 100;
        StandAtHeight(automation, 0d);
        panel.AddRoutePoint();                 // 0: the floor below
        StandAtHeight(automation, 10d);
        panel.AddRoutePoint();                 // 1: the floor above
        panel.SetMetaOption("EnableNav", Truthy(true));

        StandAtHeight(automation, 9d);
        panel.ExecuteVtankCommand(new PluginCommand("vt", "start", "/vt start"));

        Assert.Equal(1, panel.RouteWaypointIndexForTest);
    }

    /// <summary>A once-through route starts at its head, wherever you are.</summary>
    [Fact]
    public void AOnceRouteStartsAtItsHead()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation));
        automation.CurrentHealth = 100;
        automation.MaxHealth = 100;
        TwoPointRoute(panel, automation);
        panel.SelectRouteMode("Once");
        panel.SetMetaOption("EnableNav", Truthy(true));

        StandAt(automation, 48d);
        panel.ExecuteVtankCommand(new PluginCommand("vt", "start", "/vt start"));

        Assert.Equal(0, panel.RouteWaypointIndexForTest);
    }

    /// <summary>
    /// A follow route has no round to anchor — there is one target and the
    /// index means nothing — so the start leaves it alone.
    /// </summary>
    [Fact]
    public void AFollowRouteHasNoRoundToAnchor()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation));
        automation.CurrentHealth = 100;
        automation.MaxHealth = 100;
        TwoPointRoute(panel, automation);
        panel.SetMetaOption("EnableNav", Truthy(true));
        panel.ToggleCombat();
        for (int pass = 0; pass < 4; pass++)
            panel.OnTick(0.3d);
        Assert.Equal(1, panel.RouteWaypointIndexForTest);
        panel.ExecuteVtankCommand(new PluginCommand("vt", "stop", "/vt stop"));

        // Switching the mode is a route change and resets the round, so the
        // index is put back deliberately to give the start something to leave
        // alone.
        panel.SelectRouteMode("Target");
        panel.ExecuteVtankCommand(new PluginCommand("vt", "stop", "/vt stop"));
        StandAt(automation, 48d);
        int before = panel.RouteWaypointIndexForTest;
        panel.ExecuteVtankCommand(new PluginCommand("vt", "start", "/vt start"));

        Assert.Equal(before, panel.RouteWaypointIndexForTest);
    }

    /// <summary>
    /// Flipping a setting is not a route change. Turning navigation off and
    /// on again leaves the round where it was — and the toolbar toggle has to
    /// agree with setting the same thing by name, which never reset it.
    /// </summary>
    [Fact]
    public void TurningNavigationOffAndOnAgainKeepsTheRound()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation));
        automation.CurrentHealth = 100;
        automation.MaxHealth = 100;
        TwoPointRoute(panel, automation);
        panel.SetMetaOption("EnableNav", Truthy(true));
        panel.ToggleCombat();
        for (int pass = 0; pass < 4; pass++)
            panel.OnTick(0.3d);
        StandAt(automation, 25d);
        Assert.Equal(1, panel.RouteWaypointIndexForTest);

        panel.ToggleNavigation();
        panel.ToggleNavigation();
        panel.ToggleFollowAroundCorners();
        panel.ToggleOpenDoors();

        Assert.Equal(1, panel.RouteWaypointIndexForTest);
    }

    /// <summary>
    /// The route that CHANGED under the controller is the one that goes back
    /// to its first point — the reset path a stop must not borrow.
    /// </summary>
    [Fact]
    public void LoadingARouteStillPutsTheRoundBackToItsFirstPoint()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation));
        automation.CurrentHealth = 100;
        automation.MaxHealth = 100;
        TwoPointRoute(panel, automation);
        panel.SetMetaOption("EnableNav", Truthy(true));
        panel.ToggleCombat();
        for (int pass = 0; pass < 4; pass++)
            panel.OnTick(0.3d);
        Assert.Equal(1, panel.RouteWaypointIndexForTest);

        panel.AddRoutePoint();

        Assert.Equal(0, panel.RouteWaypointIndexForTest);
    }

    /// <summary>Two points, at east-west 0 and 50, with the character back at 0.</summary>
    /// <summary>
    /// A circular route loaded while the macro runs (a meta swapping from its
    /// travel route to its hunting circuit) starts at the point nearest the
    /// character, as a macro start does. Seen live: the circuit started at
    /// its first point 190 m away, out of the client's reach, and the
    /// character stood still until the meta's watchdog restarted everything.
    /// Mutation: go to the head on a load and the index is 0.
    /// </summary>
    [Fact]
    public void ACircuitLoadedWhileRunningStartsAtItsNearestPoint()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/navs/Circle.af"] =
            "NAV: nav0 circular ~~ {\r\n"
            + "\tpnt 0 0 0\r\n"
            + "\tpnt 0.2 0 0\r\n"
            + "\tpnt 0.4 0 0\r\n";
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        automation.CurrentHealth = 100;
        automation.MaxHealth = 100;
        panel.ToggleCombat();
        Assert.True(panel.CombatEnabled);

        StandAt(automation, 0.4d);
        Command(panel, "nav load Circle");

        Assert.Equal(2, panel.RouteWaypointIndexForTest);
    }

    private static void TwoPointRoute(MossTankPanel panel, FakeAutomation automation)
    {
        panel.AddRoutePoint();
        StandAt(automation, 50d);
        panel.AddRoutePoint();
        StandAt(automation, 0d);
    }

    private static void StandAt(FakeAutomation automation, double eastWest) =>
        automation.NavigationSnapshot = automation.NavigationSnapshot with
        {
            Position = automation.NavigationSnapshot.Position with
            {
                EastWest = eastWest,
            },
        };

    private static void StandAtHeight(FakeAutomation automation, double elevation) =>
        automation.NavigationSnapshot = automation.NavigationSnapshot with
        {
            Position = automation.NavigationSnapshot.Position with
            {
                Elevation = elevation,
            },
        };

    /// <summary>
    /// The fake-death verb is the whole death, not just the meta edge: the
    /// reference's verb calls the same handler a real death does.
    /// </summary>
    [Fact]
    public void TheFakeDeathVerbRunsTheWholeDeathHandler()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.SetMetaOption("StopMacroOnDeath", Truthy(true));
        panel.ToggleCombat();
        panel.OnTick(0.1d);
        Assert.True(panel.CombatMacroRunning);

        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "fakedeath", "/vt fakedeath"));

        Assert.True(panel.CombatMacroRunning);
        Assert.False(panel.GetMetaOptionForTest("EnableCombat"));
        Assert.Contains(
            automation.Messages,
            static value => value.Contains("You died!", StringComparison.Ordinal));
    }

    /// <summary>The restore verb is listed in the help, as the reference's link is offered.</summary>
    [Fact]
    public void TheDeathRestoreVerbIsListed()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ExecuteVtankCommand(new PluginCommand("vt", "help", "/vt help"));

        Assert.Contains(
            automation.Messages,
            static value => value.Contains("deathrestore", StringComparison.Ordinal));
    }


    private static AcDream.Plugins.MossTank.Expressions.ExpressionValue Truthy(
        bool value) =>
        AcDream.Plugins.MossTank.Expressions.ExpressionValue.Boolean(value);

    [Fact]
    public void ForceBuffGoesThroughTheModeGateBecauseMEIsConsumableNotForce()
    {
        PluginSpellInfo buff = Spell(
            1, 10, "Increases the caster's Life Magic skill by 10 points.");
        var automation = new CombatCapableFakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
            ],
            KnownSelfBuffs = [buff],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        // No wand anywhere: the gate's own path here is
        // PostNoWandNoticeAndStop, exactly as the reference does.
        panel.ToggleCombat();
        panel.ForceBuff();
        for (int tick = 0; tick < 5; tick++)
            panel.OnTick(0.3d);

        Assert.Empty(automation.CastSpellIds);
        Assert.Contains(
            automation.Messages,
            static value => value.Contains(
                CombatModeGate.NoWandNotice,
                StringComparison.Ordinal));
        Assert.False(panel.CombatMacroRunning);
    }

    [Fact]
    public void TogglingBuffingOffPausesTheForceBuffInsteadOfCancellingIt()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
            ],
            KnownSelfBuffs =
            [
                Spell(1, 10, "Increases the caster's Life Magic skill by 10 points."),
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.SetMetaOption("EnableBuffing", Truthy(false));

        panel.ToggleCombat();
        panel.ForceBuff();
        for (int tick = 0; tick < 4; tick++)
            panel.OnTick(0.3d);
        Assert.Empty(automation.CastSpellIds);   // EnableBuffing holds the pass

        panel.SetMetaOption("EnableBuffing", Truthy(true));
        for (int tick = 0; tick < 4; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([1u], automation.CastSpellIds);
    }

    [Fact]
    public void AForceBuffWithTheMacroOffNeitherBuffsNorTopsUpVitals()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 10,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
            ],
            KnownSelfBuffs =
            [
                Spell(1, 10, "Increases the caster's Life Magic skill by 10 points."),
            ],
            // BoosterVital 2 = VitalKind.Health.
            ItemEntries = [Item(60, "Bread", 0x20) with { BoosterVital = 2 }],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(60u);
        panel.AddSelectedConsumable();

        // The macro stays OFF, so nothing in the main list runs at all.
        panel.ForceBuff();
        for (int tick = 0; tick < 6; tick++)
            panel.OnTick(0.3d);

        Assert.Empty(automation.CastSpellIds);
        Assert.Empty(automation.UsedItemIds);
    }

    [Fact]
    public void AnItemRowWhoseSelfAuraLandsOnTheCharacterIsCoveredNextPass()
    {
        FakeAutomation automation = ItemEnchantAutomation();
        // One resolvable row, so the loop would be unmistakable.
        automation.KnownSelfBuffs =
        [
            NamedSpell(101, 201, "Aura of Defender Self I", ItemEnchantmentSchoolId)
                with { IsSelfTargeted = true },
        ];
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        host.Selection.Select(10);
        panel.AddSelectedItem();
        panel.ToggleCombat();
        for (int tick = 0; tick < 20; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([101u], automation.CastSpellIds);

        Assert.Contains(
            automation.ActiveEnchantments,
            held => held.SpellId == 101u);
        Assert.False(
            automation.ItemEnchantments.TryGetValue(10u, out var onWand)
                && onWand.Count > 0,
            "the server never enchants the item with a Self aura");
    }

    [Fact]
    public void AnItemEnchantRowFallsToTheCastableLowerTierLikeTheSelfWalk()
    {
        FakeAutomation automation = ItemEnchantAutomation();
        automation.Skills =
        [
            new PluginSkillInfo(
                ItemEnchantmentSchoolId,
                "Item Enchantment",
                PluginSkillTraining.Trained,
                300),
        ];
        automation.KnownSelfBuffs =
        [
            NamedSpell(106, 201, "Aura of Defender Self VI", ItemEnchantmentSchoolId)
                with
            {
                IsSelfTargeted = true,
                Tier = 6,
                Difficulty = 200,
                FormulaComponentIds = [7u],
            },
            NamedSpell(101, 201, "Aura of Defender Self I", ItemEnchantmentSchoolId)
                with
            {
                IsSelfTargeted = true,
                Tier = 1,
                Difficulty = 10,
                FormulaComponentIds = [8u],
            },
        ];
        automation.Components[7u] = Component(7u, "Lead Scarab");
        automation.Components[8u] = Component(8u, "Pyreal Scarab");
        automation.ItemEntries =
        [
            Item(10, "War Wand", 0x8000u, validLocations: 0x01000000u),
            Item(50, "Pyreal Scarab", 1),
        ];

        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);
        host.Selection.Select(10);
        panel.AddSelectedItem();
        panel.ToggleCombat();
        for (int tick = 0; tick < 12; tick++)
            panel.OnTick(0.3d);

        Assert.NotEmpty(automation.CastSpellIds);
        Assert.All(automation.CastSpellIds, id => Assert.Equal(101u, id));
    }

    [Fact]
    public void AnItemRowPickAndItsLedgerWriteBothSayWhatHappenedOnMisc()
    {
        FakeAutomation automation = ItemEnchantAutomation();
        automation.KnownSelfBuffs =
        [
            NamedSpell(101, 201, "Aura of Defender Self I", ItemEnchantmentSchoolId)
                with { IsSelfTargeted = true },
        ];
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "log Misc on", "/vt log Misc on"));

        host.Selection.Select(10);
        panel.AddSelectedItem();
        automation.Messages.Clear();
        panel.ToggleCombat();
        for (int tick = 0; tick < 12; tick++)
            panel.OnTick(0.3d);

        Assert.Contains(
            automation.Messages,
            message => message.Contains(
                "Buffing: item row War Wand (10) \u2192 Aura of Defender Self I",
                StringComparison.Ordinal));
        Assert.Contains(
            automation.Messages,
            message => message.Contains(
                "Cast Aura of Defender Self I on 10 ending at",
                StringComparison.Ordinal)
                && message.Contains("OVERRIDDEN", StringComparison.Ordinal));
    }

    [Fact]
    public void ASkippedBuffFamilyWarnsOnceInChatAndMirrorsToTheHostLog()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 5),
            ],
            KnownSelfBuffs =
            [
                Spell(1, 10, "Increases the caster's Life Magic skill by 10 points."),
            ],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        Command(panel, "log Misc on");

        panel.ToggleCombat();
        for (int tick = 0; tick < 12; tick++)
            panel.OnTick(0.3d);

        Assert.Empty(automation.CastSpellIds);

        const string Warning =
            "Buff skipped: no known spell in the class of Spell 1.";
        Assert.Equal(
            1,
            automation.Messages.Count(
                message => message.Contains(Warning, StringComparison.Ordinal)));
        Assert.Equal(
            1,
            host.Logger.Infos.Count(
                line => line.Contains(Warning, StringComparison.Ordinal)));
    }

    [Fact]
    public void EnablingBuffingWithTheMacroOffCastsNothing()
    {
        var automation = BuffPassAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "opt set ManaChargesWhenOff true");
        panel.SetMetaOption("EnableBuffing", Truthy(false));
        for (int tick = 0; tick < 5; tick++)
            panel.OnTick(0.3d);

        panel.SetMetaOption("EnableBuffing", Truthy(true));
        for (int tick = 0; tick < 20; tick++)
            panel.OnTick(0.3d);

        Assert.Empty(automation.CastSpellIds);

        // And the macro still starts it, so the pin is not vacuous.
        panel.ToggleCombat();
        for (int tick = 0; tick < 4; tick++)
            panel.OnTick(0.3d);
        Assert.NotEmpty(automation.CastSpellIds);
    }

    /// <summary>
    /// The reference rule is valid whenever a vital is below its threshold,
    /// and holds the pass with a warning when nothing can answer it; what
    /// the reference never has is "nothing to use", because its kits need
    /// no assessment and a heal spell is always a handler. The assessment
    /// starvation that once left this character with nothing to use is
    /// fixed at its cause; the rule keeps the reference's hold.
    /// </summary>
    [Fact]
    public void ARechargeWithNothingToUseHoldsThePassAsTheReferenceDoes()
    {
        // Health is low and nothing can answer it: no kit, no food, no spell.
        var automation = new FakeAutomation
        {
            CurrentHealth = 10,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        panel.ToggleCombat();

        var held = new List<string>();
        for (int tick = 0; tick < 12; tick++)
        {
            panel.OnTick(0.3d);
            foreach (IMacroRule rule in panel.MacroRules)
            {
                if (rule.Running && rule.Name == "RechargeSelfNormal")
                    held.Add(rule.Name);
            }
        }

        Assert.NotEmpty(held);
    }

    [Fact]
    public void WithTheMacroOffOnlyRefillWieldedManaMayRun()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 10,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
            ],
            // …and a buff due, so BuffSelf would.
            KnownSelfBuffs =
            [
                Spell(1, 10, "Increases the caster's Life Magic skill by 10 points."),
            ],
            ItemEntries = [Item(60, "Bread", 0x20) with { BoosterVital = 2 }],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(60u);
        panel.AddSelectedConsumable();
        Command(panel, "opt set ManaChargesWhenOff true");

        var ranWithMacroOff = new HashSet<string>(StringComparer.Ordinal);
        for (int tick = 0; tick < 20; tick++)
        {
            panel.OnTick(0.3d);
            foreach (IMacroRule rule in panel.MacroRules)
            {
                if (rule.Running)
                    ranWithMacroOff.Add(rule.Name);
            }
        }

        Assert.All(
            ranWithMacroOff,
            name => Assert.Equal("RefillWieldedMana", name));
        Assert.DoesNotContain("RechargeSelfNormal", ranWithMacroOff);
        Assert.DoesNotContain("BuffSelf", ranWithMacroOff);
        Assert.Empty(automation.CastSpellIds);
        Assert.Empty(automation.UsedItemIds);

        // The same world with the macro ON runs both of them — so the
        // assertions above are about the GATE, not about an empty fixture.
        panel.ToggleCombat();
        var ranWithMacroOn = new HashSet<string>(StringComparer.Ordinal);
        for (int tick = 0; tick < 20; tick++)
        {
            panel.OnTick(0.3d);
            foreach (IMacroRule rule in panel.MacroRules)
            {
                if (rule.Running)
                    ranWithMacroOn.Add(rule.Name);
            }
        }

        Assert.Contains("RechargeSelfNormal", ranWithMacroOn);
        Assert.NotEmpty(automation.UsedItemIds);
    }

    [Fact]
    public void AForceEndsWhenTheStampsItZeroedHaveBeenRecast()
    {
        var automation = BuffPassAutomation();
        // Spells 1 and 2 are up with hours left: due ONLY under the force.
        automation.ActiveEnchantments =
        [
            new PluginActiveEnchantment(1u, 10u, 1, 1800d),
            new PluginActiveEnchantment(2u, 20u, 1, 1800d),
        ];
        // Spell 3 is genuinely due and can never land, so the pick is never
        // empty. It sorts last, so it does not starve the two forced ones.
        automation.RefusedCastSpellIds.Add(3u);

        var panel = new MossTankPanel(new FakeHost(automation));
        panel.ToggleCombat();
        panel.ForceBuff();

        for (int tick = 0; tick < 40; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([1u, 2u], automation.CastSpellIds);

        for (int tick = 0; tick < 20; tick++)
            panel.OnTick(0.3d);
        Assert.Equal([1u, 2u], automation.CastSpellIds);
    }

    [Fact]
    public void CancelForceBuffStopsTheForceOnTheCall()
    {
        var automation = BuffPassAutomation();
        automation.ActiveEnchantments =
        [
            new PluginActiveEnchantment(1u, 10u, 1, 1800d),
            new PluginActiveEnchantment(2u, 20u, 1, 1800d),
            new PluginActiveEnchantment(3u, 30u, 1, 1800d),
        ];
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.ToggleCombat();

        panel.ForceBuff();
        panel.OnTick(0.3d);
        int castsBeforeCancel = automation.CastSpellIds.Count;

        panel.CancelForceBuff();

        for (int tick = 0; tick < 20; tick++)
            panel.OnTick(0.3d);
        Assert.Equal(castsBeforeCancel, automation.CastSpellIds.Count);
    }

    [Fact]
    public void FastCastBuffsHoldsForwardOnlyUntilInstantBuffCastEnds()
    {
        var automation = new FakeAutomation
        {
            Skills =
            [
                new PluginSkillInfo(
                    1,
                    "Life Magic",
                    PluginSkillTraining.Trained,
                    300),
            ],
            KnownSelfBuffs =
            [
                Spell(
                    1,
                    10,
                    "Increases the caster's Life Magic skill by 10 points."),
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        Command(panel, "opt set FastCastBuffs true");

        panel.ToggleCombat();
        panel.ForceBuff();
        panel.OnTick(0d);

        PluginMovementIntent held = Assert.Single(automation.MovementIntents);
        Assert.True(held.Forward);
        Assert.Equal(0, automation.ClearMovementCount);

        panel.OnTick(0.25d);

        Assert.Equal(1, automation.ClearMovementCount);
    }

    [Fact]
    public void FastCastBuffsNeverAppliesForwardMovementToWarSpells()
    {
        var automation = new FakeAutomation
        {
            Skills =
            [
                new PluginSkillInfo(
                    1,
                    "Life Magic",
                    PluginSkillTraining.Trained,
                    300),
            ],
            KnownSelfBuffs =
            [
                Spell(
                    1,
                    10,
                    "Increases the caster's Life Magic skill by 10 points.")
                    with { School = 34u },
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        Command(panel, "opt set FastCastBuffs true");

        panel.ToggleCombat();
        panel.ForceBuff();
        panel.OnTick(0d);

        Assert.Empty(automation.MovementIntents);
    }

    [Fact]
    public void RetainedLabelReads_UseUpdateSideSnapshotsWithoutAllocating()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 90,
            MaxHealth = 100,
            CurrentStamina = 80,
            MaxStamina = 110,
            CurrentMana = 70,
            MaxMana = 120,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
                new PluginSkillInfo(2, "War Magic", PluginSkillTraining.Specialized, 350),
                new PluginSkillInfo(3, "Run", PluginSkillTraining.Untrained, 100),
            ],
            Attributes =
            [
                new PluginAttributeInfo(0, "Strength", 100),
                new PluginAttributeInfo(1, "Endurance", 100),
            ],
            KnownSelfBuffs =
            [
                Spell(1, 10, "Increases the caster's Life Magic skill by 10 points."),
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.OnTick(0.0);

        Assert.Equal("Health 90/100   Stam 80/110   Mana 70/120", panel.Vitals);
        Assert.Equal("2 attributes, 2 trained skills, 1 buff lines", panel.Coverage);
        string expectedVitals = panel.Vitals;
        string expectedCoverage = panel.Coverage;

        _ = panel.Vitals;
        _ = panel.Coverage;
        long before = GC.GetAllocatedBytesForCurrentThread();
        bool sameReferences = true;
        for (int i = 0; i < 10_000; i++)
        {
            sameReferences &= ReferenceEquals(expectedVitals, panel.Vitals);
            sameReferences &= ReferenceEquals(expectedCoverage, panel.Coverage);
        }
        long allocated = GC.GetAllocatedBytesForCurrentThread() - before;

        Assert.True(sameReferences);
        Assert.Equal(0, allocated);
        Assert.Equal(1, automation.KnownSelfBuffReads);
    }

    [Fact]
    public void UpdateTick_RefreshesRareCoverageAndChangedVitals()
    {
        var automation = new FakeAutomation
        {
            CurrentHealth = 90,
            MaxHealth = 100,
            CurrentStamina = 80,
            MaxStamina = 110,
            CurrentMana = 70,
            MaxMana = 120,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
            ],
            Attributes = [new PluginAttributeInfo(0, "Strength", 100)],
            KnownSelfBuffs =
            [
                Spell(1, 10, "Increases the caster's Life Magic skill by 10 points."),
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.OnTick(0.0);

        automation.CurrentHealth = 75;
        automation.Skills =
        [
            new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
            new PluginSkillInfo(2, "War Magic", PluginSkillTraining.Specialized, 350),
        ];
        panel.OnTick(0.5);

        Assert.StartsWith("Health 75/100", panel.Vitals, StringComparison.Ordinal);
        Assert.Equal("1 attributes, 1 trained skills, 1 buff lines", panel.Coverage);

        panel.OnTick(0.5);
        Assert.Equal("1 attributes, 2 trained skills, 1 buff lines", panel.Coverage);

        automation.KnownSelfBuffs =
        [
            Spell(1, 10, "Increases the caster's Life Magic skill by 10 points."),
            Spell(2, 11, "Increases the caster's War Magic skill by 10 points."),
        ];
        panel.OnTick(0.0);

        Assert.Equal("1 attributes, 2 trained skills, 2 buff lines", panel.Coverage);
    }

    [Fact]
    public void ItemsAndConsumablesTabsAddTheSelectedOwnedItemToRealProfiles()
    {
        var automation = new FakeAutomation
        {
            ItemEntries =
            [
                Item(10, "Imperil Lens", 0x8000),
                Item(11, "Iron Phial of Imperil", 0x100),
                Item(12, "Black Marrow Pea", 0x20),
            ],
        };
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);

        host.Selection.Select(10);
        panel.AddSelectedItem();
        Assert.Contains("Imperil Lens", panel.ItemProfileText, StringComparison.Ordinal);

        host.Selection.Select(11);
        panel.AddSelectedConsumable();
        panel.AddAllPeas();
        Assert.Contains(
            "Iron Phial of Imperil",
            panel.ConsumableProfileText,
            StringComparison.Ordinal);
        Assert.Contains(
            CraftingPlanner.AllPeas,
            panel.ConsumableProfileText,
            StringComparison.Ordinal);

        Assert.Contains("Imperil Lens", panel.ItemRows);
        panel.RemoveSelectedItem();
        Assert.DoesNotContain("Imperil Lens", panel.ItemRows);
        Assert.Equal(2, panel.ConsumableRows.Count);
        panel.RemoveSelectedConsumable();
        Assert.Single(panel.ConsumableRows);
    }

    [Fact]
    public void ItemsGridNameClickDeletesAndHandsClickCyclesHandedness()
    {
        var automation = new FakeAutomation
        {
            ItemEntries =
            [
                Item(10, "Fire Sword", 1),
                Item(11, "Ice Wand", 1),
            ],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(10);
        panel.AddSelectedItem();
        host.Selection.Select(11);
        panel.AddSelectedItem();
        Assert.Equal(["Fire Sword", "Ice Wand"], panel.ItemNameColumn);

        Assert.Equal("Auto", panel.ItemHandsColumn[0]);
        panel.CycleItemHandsAt(0);
        Assert.Equal("1-Handed", panel.ItemHandsColumn[0]);
        panel.CycleItemHandsAt(0);
        Assert.Equal("2-Handed", panel.ItemHandsColumn[0]);
        panel.CycleItemHandsAt(0);
        Assert.Equal("Auto", panel.ItemHandsColumn[0]);
        Assert.Equal("Auto", panel.ItemHandsColumn[1]);

        panel.DeleteItemRowAt(0); // "Fire Sword"
        Assert.Equal(["Ice Wand"], panel.ItemNameColumn);
    }

    [Fact]
    public void ItemHandsColumnDoesNotReallocateOnEveryReadAndNoBuffSuffixNeverLeaks()
    {
        var automation = new FakeAutomation
        {
            ItemEntries = [Item(10, "Fire Sword", 1)],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(10);
        panel.AddSelectedItemNoBuffs();
        Assert.Equal(["Fire Sword   [no buffs]"], panel.ItemNameColumn);

        Assert.Same(panel.ItemHandsColumn, panel.ItemHandsColumn);
        Assert.Equal("Auto", panel.ItemHandsColumn[0]);

        panel.CycleItemHandsAt(0);
        IReadOnlyList<string> afterCycle = panel.ItemHandsColumn;
        Assert.Equal("1-Handed", afterCycle[0]);
        Assert.Same(afterCycle, panel.ItemHandsColumn);
    }

    [Fact]
    public void ConsumablesLeftListRowClickRemovesTheRowDirectly()
    {
        var automation = new FakeAutomation
        {
            ItemEntries =
            [
                Item(20, "Iron Phial of Imperil", 0x100),
                Item(21, "Iron Phial of Vulnerability", 0x100),
            ],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(20);
        panel.AddSelectedConsumable();
        host.Selection.Select(21);
        panel.AddSelectedConsumable();
        Assert.Equal(2, panel.ConsumableRows.Count);

        panel.SelectConsumableRow(0);

        Assert.Single(panel.ConsumableRows);
    }

    [Fact]
    public void ExcludedComponentsGridAddsBySelectionAndDeletesByAnyCellClick()
    {
        var automation = new FakeAutomation
        {
            ItemEntries =
            [
                Item(20, "Charged Yellow Scarab", 0x20)
                    with { IconId = 0x06001234u },
            ],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        Assert.Empty(panel.ExcludedComponentRows);
        host.Selection.Select(20);
        panel.AddSelectedComponent();
        Assert.Equal(["Charged Yellow Scarab"], panel.ExcludedComponentRows);
        Assert.Equal(0x06001234u, panel.ExcludedComponentIcons[0]);

        panel.AddSelectedComponent();
        Assert.Equal(["Charged Yellow Scarab"], panel.ExcludedComponentRows);

        panel.DeleteExcludedComponentAt(0);
        Assert.Empty(panel.ExcludedComponentRows);
    }

    [Fact]
    public void ExcludedComponentIconsDoesNotScanLiveInventoryOnEveryRead()
    {
        var automation = new FakeAutomation
        {
            ItemEntries =
            [
                Item(20, "Charged Yellow Scarab", 0x20) with { IconId = 0x06001234u },
            ],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(20);
        panel.AddSelectedComponent();
        Assert.Equal(0x06001234u, panel.ExcludedComponentIcons[0]);

        int callsAfterAdd = automation.CaptureOwnedItemsCallCount;
        for (int i = 0; i < 5; i++)
            _ = panel.ExcludedComponentIcons;

        Assert.Equal(callsAfterAdd, automation.CaptureOwnedItemsCallCount);
    }

    [Fact]
    public void ExtraBuffAndBlacklistedFamilyNamesPersistAcrossSessions()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation
        {
            Name = "Persist Check",
            KnownSelfBuffs =
            [
                Spell(1, 10, "Increases the caster's Strength by 10 points."),
                Spell(2, 20, "Increases the caster's Focus by 10 points."),
            ],
        };
        var first = new MossTankPanel(new FakeHost(automation, storage));
        first.ShowExtraBuffPicker();
        first.PickBuffAt(0); // "Spell 1"
        first.ShowBlacklistedBuffPicker();
        first.PickBuffAt(1); // "Spell 2"
        Assert.Equal(["Spell 1"], first.ExtraBuffRows);
        Assert.Equal(["Spell 2"], first.BlacklistedBuffFamilyRows);

        string usd = Assert.Single(storage.Text,
            static entry => entry.Key.EndsWith(".usd", StringComparison.Ordinal)).Value;
        VtankDatabase document = VtankDatabase.Parse(usd);
        Assert.Equal(1, Assert.Single(document.Find("ExtraBuffSpells")!.Rows).Cells[0].AsInt());
        Assert.Equal(2, Assert.Single(document.Find("AntiExtraBuffSpells")!.Rows).Cells[0].AsInt());

        var second = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Persist Check" }, storage));

        Assert.Equal(["Spell 1"], second.ExtraBuffRows);
        Assert.Equal(["Spell 2"], second.BlacklistedBuffFamilyRows);
    }

    [Fact]
    public void ImportedExtraBuffExemplarIsVisibleAndRemovableFromTheBuffUi()
    {
        var storage = new MemoryStorage();
        VtankDatabase profile = VtankDefaultSettingsDatabase.Create();
        profile.Find("ExtraBuffSpells")!.Rows.Add(ExemplarRow(1));
        storage.Text[SettingsKey(VtankProfileDirectory.AutoCharacterFileName(
            "Imported Extra", string.Empty, "usd"))] = profile.Render();
        var automation = new FakeAutomation
        {
            Name = "Imported Extra",
            KnownSelfBuffs =
            [
                Spell(1, 10, "Increases the caster's Strength by 10 points."),
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation, storage));

        Assert.Equal(["Spell 1"], panel.ExtraBuffRows);
        panel.DeleteExtraBuffAt(0);

        Assert.Empty(panel.ExtraBuffRows);
        string usd = storage.Text[SettingsKey(VtankProfileDirectory.AutoCharacterFileName(
            "Imported Extra", string.Empty, "usd"))];
        Assert.Empty(VtankDatabase.Parse(usd).Find("ExtraBuffSpells")!.Rows);
    }

    /// <summary>
    /// A GemFood row owns its spell choice. Mutation: choose the first
    /// appraised spell for configured gems and this selects Spell 3810 rather
    /// than the profile's Spell 3811.
    /// </summary>
    [Fact]
    public void LoadedGemFoodUsesItsConfiguredSpellWithoutAConsumablesProfileRow()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/profiles/GemFood.usd"] = SettingsWithGemFood(
            ("Unknown Gem", 999999u),
            ("Blackmoor's Favor", 3811u));
        var automation = new FakeAutomation
        {
            Name = "Gem Tester",
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            KnownSelfBuffs =
            [
                Spell(3810, 518, "An unconfigured effect."),
                Spell(3811, 519, "A configured effect."),
            ],
            ItemEntries =
            [
                Item(100, "Unknown Gem", 0x800u)
                    with { AppraisedSpellIds = [3810u] },
                Item(101, "Blackmoor's Favor", 0x800u)
                    with { AppraisedSpellIds = [3810u] },
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation, storage));

        Command(panel, "settings load GemFood");
        panel.ToggleCombat();
        for (int tick = 0; tick < 8; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([101u], automation.UsedItemIds);
        Assert.Contains("Spell 3811", panel.BuffStatus, StringComparison.Ordinal);
    }

    /// <summary>
    /// A settings load is an activation transaction. Mutation
    /// <c>ApplyDefaultsAfterFailedSettingsLoad</c>: apply a fresh default
    /// database after the parse error; the selected profile, binding, and
    /// retained option assertions fail.
    /// </summary>
    [Fact]
    public void SettingsLoadCommandPreservesTheActiveProfileWhenTheCandidateIsMalformed()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Saver", WorldName = "Rune" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Command(panel, "settings save Safe");
        Command(panel, "opt set AttackDistance 0.02");
        string safe = panel.SelectedMacroProfile;
        string cdfKey = VtankProfileDirectory.CdfFileName("Saver", "Rune");
        string binding = storage.Text[cdfKey];
        const string malformed = "not a settings database";
        storage.Text["mosstank/profiles/Broken.usd"] = malformed;
        automation.Messages.Clear();

        Command(panel, "settings load Broken");
        Command(panel, "opt set EnableBuffing false");

        Assert.Equal(safe, panel.SelectedMacroProfile);
        Assert.Equal(0.02d, panel.EvaluateExpression(
            "uboptget[`AttackDistance`]").AsNumber(), precision: 7);
        Assert.Equal(binding, storage.Text[cdfKey]);
        Assert.Equal(malformed, storage.Text["mosstank/profiles/Broken.usd"]);
        Assert.Contains(automation.Messages, message =>
            message.Contains("could not be read", StringComparison.Ordinal));
        Assert.DoesNotContain(automation.Messages, message =>
            message.Contains("Loaded settings profile Broken", StringComparison.Ordinal));
    }

    /// <summary>
    /// Mutation <c>SkipSettingsCopyTargetValidation</c>: omit the existing
    /// target parse before copying; the malformed target becomes a new
    /// settings document.
    /// </summary>
    [Fact]
    public void SettingsSaveCommandDoesNotOverwriteAnUnreadableTarget()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Saver", WorldName = "Rune" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Command(panel, "settings save Safe");
        string selected = panel.SelectedMacroProfile;
        string cdfKey = VtankProfileDirectory.CdfFileName("Saver", "Rune");
        string binding = storage.Text[cdfKey];
        string target = SettingsKey(
            VtankProfileDirectory.SubProfilePrefix("Saver", "Rune") + "Target.usd");
        const string malformed = "not a settings database";
        storage.Text[target] = malformed;

        Command(panel, "settings save Target");

        Assert.Equal(selected, panel.SelectedMacroProfile);
        Assert.Equal(binding, storage.Text[cdfKey]);
        Assert.Equal(malformed, storage.Text[target]);
        Assert.Contains(automation.Messages, message =>
            message.Contains("Cannot overwrite unreadable settings profile", StringComparison.Ordinal));
    }

    /// <summary>
    /// A copied settings profile starts with the complete active USD document,
    /// including imported item enchant rows and tables the adapter does not
    /// interpret. Mutation: create the copy from defaults; the raw row and
    /// custom-table assertions fail.
    /// </summary>
    [Fact]
    public void SettingsCopyPreservesImportedBuffedItemsAndUnknownTablesIndependently()
    {
        var storage = new MemoryStorage();
        var automation = ItemEnchantAutomation();
        automation.ItemEntries =
        [
            Item(10, "War Wand", 0x8000u, validLocations: 0x01000000u),
            Item(11, "Equipped Wand", 0x8000u) with { EquippedLocation = 0x01000000u },
        ];
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Command(panel, "settings save Source");
        string source = SettingsKey(panel.SelectedMacroProfile);
        VtankDatabase sourceDatabase = VtankDatabase.Parse(storage.Text[source]);
        VtankTable buffed = sourceDatabase.Find("BuffedItems")!;
        buffed.Rows.Add(new VtankRow { Cells = { VtankCell.Int(10), VtankCell.Int(101) } });
        buffed.Rows.Add(new VtankRow { Cells = { VtankCell.Int(10), VtankCell.Int(102) } });
        buffed.Rows.Add(new VtankRow { Cells = { VtankCell.Int(-1), VtankCell.Int(101) } });
        var custom = new VtankTable();
        custom.ColumnNames.Add("Marker");
        custom.IndexFlags.Add(false);
        custom.Rows.Add(new VtankRow { Cells = { VtankCell.String("copy proof") } });
        sourceDatabase.Tables.Add(("CopyProof", custom));
        storage.Text[source] = sourceDatabase.Render();

        Command(panel, "settings load Source");
        Command(panel, "settings save Copy");
        string copy = SettingsKey(panel.SelectedMacroProfile);
        VtankDatabase copied = VtankDatabase.Parse(storage.Text[copy]);
        VtankTable copiedBuffed = copied.Find("BuffedItems")!;
        Assert.Contains(copiedBuffed.Rows, row =>
            row.Cells[copiedBuffed.ColumnIndex("Object")].AsInt() == 10
            && row.Cells[copiedBuffed.ColumnIndex("Spell")].AsInt() == 101);
        Assert.Contains(copiedBuffed.Rows, row =>
            row.Cells[copiedBuffed.ColumnIndex("Object")].AsInt() == 10
            && row.Cells[copiedBuffed.ColumnIndex("Spell")].AsInt() == 102);
        Assert.Contains(copiedBuffed.Rows, row =>
            row.Cells[copiedBuffed.ColumnIndex("Object")].AsInt() == -1
            && row.Cells[copiedBuffed.ColumnIndex("Spell")].AsInt() == 101);
        Assert.Equal("copy proof", copied.Find("CopyProof")!.Rows[0].Cells[0].AsString());

        Command(panel, "opt set AttackDistance 0.02");
        VtankDatabase sourceAfterCopySave = VtankDatabase.Parse(storage.Text[source]);
        Assert.Equal("copy proof", sourceAfterCopySave.Find("CopyProof")!.Rows[0].Cells[0].AsString());
        Assert.Equal(3, sourceAfterCopySave.Find("BuffedItems")!.Rows.Count);

        storage.Text[SettingsKey(panel.SelectedMacroProfile)] = "externally corrupted source";
        panel.ToggleAutoStack();
        Assert.False(panel.AutoStackEnabled);
        panel.DeleteItemRowAt(0);
        Command(panel, "settings save StaleCopy");
        VtankDatabase staleCopy = VtankDatabase.Parse(
            storage.Text[SettingsKey(panel.SelectedMacroProfile)]);
        VtankTable staleSettings = staleCopy.Find("Settings")!;
        VtankRow autoStack = Assert.Single(staleSettings.Rows, row =>
            row.Cells[staleSettings.ColumnIndex("Setting")].AsString()
                .Equals("AutoStack", StringComparison.OrdinalIgnoreCase));
        Assert.False(autoStack.Cells[staleSettings.ColumnIndex("Value")].AsBool());
        VtankTable staleBuffed = staleCopy.Find("BuffedItems")!;
        Assert.DoesNotContain(staleBuffed.Rows, row =>
            row.Cells[staleBuffed.ColumnIndex("Object")].AsInt() == 10
            && row.Cells[staleBuffed.ColumnIndex("Spell")].AsInt() == 101);
        Assert.Contains(staleBuffed.Rows, row =>
            row.Cells[staleBuffed.ColumnIndex("Object")].AsInt() == 10
            && row.Cells[staleBuffed.ColumnIndex("Spell")].AsInt() == 102);
        Assert.Equal("copy proof", staleCopy.Find("CopyProof")!.Rows[0].Cells[0].AsString());
    }

    /// <summary>
    /// Mutation <c>SkipAssistItemsApply</c>: omit the imported-table apply
    /// after loading; both known rows disappear from the planner inputs and
    /// the assertions below fail. Mutation <c>RecreateAssistRows</c>: clear
    /// and rebuild the table in Write; the unknown custom row is lost.
    /// </summary>
    [Fact]
    public void ImportedAssistItemsMapKindsAndPreserveUnknownCustomRowsOnSave()
    {
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        VtankTable table = database.Find("AssistItems")!;
        table.ColumnNames.Add("Extension");
        table.IndexFlags.Add(false);
        table.Rows.Add(new VtankRow { Cells = { VtankCell.String("Health Kit"), VtankCell.Int(0), VtankCell.String("keep-health") } });
        table.Rows.Add(new VtankRow { Cells = { VtankCell.String("Bread"), VtankCell.Int(1), VtankCell.String("keep-food") } });
        table.Rows.Add(new VtankRow { Cells = { VtankCell.String("Unknown"), VtankCell.Int(99), VtankCell.String("keep-unknown") } });
        var combat = new CombatSettings();
        var settings = new VtankSettingsProfileSerializer.AllSettings
        {
            Combat = combat, Buffs = new BuffSettings(), Vitals = new VitalSettings(),
            Inventory = new InventorySettings(), Navigation = new NavigationSettings(),
        };
        VtankSettingsProfileSerializer.Load(database.Render(), settings);
        VtankAssistItems.Apply(combat);

        Assert.Equal(ConsumableCategory.HealthKit, combat.ConsumableCategories["Health Kit"]);
        Assert.Equal(ConsumableCategory.HealthFood, combat.ConsumableCategories["Bread"]);
        Assert.DoesNotContain("Unknown", combat.ConsumableNames);

        combat.ConsumableNames.Remove("Health Kit");
        combat.ConsumableCategories.Remove("Health Kit");
        for (int i = combat.ImportedAssistItems.Count - 1; i >= 0; i--)
        {
            if (combat.ImportedAssistItems[i].Name == "Health Kit")
                combat.ImportedAssistItems.RemoveAt(i);
        }
        VtankSettingsProfileSerializer.Save(database, settings);
        VtankTable saved = database.Find("AssistItems")!;
        int objectColumn = saved.ColumnIndex("Object");
        int typeColumn = saved.ColumnIndex("Type");
        int extensionColumn = saved.ColumnIndex("Extension");
        Assert.DoesNotContain(saved.Rows, row => row.Cells[objectColumn].AsString() == "Health Kit");
        VtankRow food = Assert.Single(saved.Rows, row => row.Cells[objectColumn].AsString() == "Bread");
        Assert.Equal(1, food.Cells[typeColumn].AsInt());
        Assert.Equal("keep-food", food.Cells[extensionColumn].AsString());
        VtankRow unknown = Assert.Single(saved.Rows, row => row.Cells[objectColumn].AsString() == "Unknown");
        Assert.Equal(99, unknown.Cells[typeColumn].AsInt());
        Assert.Equal("keep-unknown", unknown.Cells[extensionColumn].AsString());
    }

    [Fact]
    public void InitialMalformedSettingsProfileStaysInactiveAndIsNeverSavedByAnOptionChange()
    {
        var storage = new MemoryStorage();
        const string character = "Saver";
        const string world = "Rune";
        string profile = SettingsKey(
            VtankProfileDirectory.AutoCharacterFileName(character, world, "usd"));
        const string malformed = "not a settings database";
        storage.Text[profile] = malformed;
        VtankProfileDirectory.WriteCharacterBinding(storage, character, world,
            new VtankProfileDirectory.VtankCharacterBinding(profile, string.Empty, string.Empty, null));
        var automation = new FakeAutomation { Name = character, WorldName = world };
        var panel = new MossTankPanel(new FakeHost(automation, storage));

        Command(panel, "opt set EnableBuffing false");
        Command(panel, "settings save Copy");
        panel.ToggleCombat();

        Assert.Equal(malformed, storage.Text[profile]);
        Assert.False(storage.Text.ContainsKey(
            SettingsKey(
                VtankProfileDirectory.SubProfilePrefix(character, world) + "Copy.usd")));
        Assert.False(panel.CombatMacroRunning);
        Assert.Contains("Raw data was preserved", panel.ProfileLifecycleNotice,
            StringComparison.Ordinal);
    }

    /// <summary>
    /// Mutation <c>SkipExistingSettingsSaveValidation</c>: omit the save-time
    /// parse of the current file; the option write replaces the malformed
    /// external edit.
    /// </summary>
    [Fact]
    public void OptionChangesDoNotOverwriteAnExternallyCorruptedActiveSettingsProfile()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Saver", WorldName = "Rune" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Command(panel, "settings save Safe");
        string profile = panel.SelectedMacroProfile;
        const string malformed = "not a settings database";
        storage.Text[profile] = malformed;

        panel.ToggleCombatEnabled();

        Assert.Equal(malformed, storage.Text[profile]);
    }

    /// <summary>
    /// Mutation <c>ApplySettingsBeforeCompleteValidation</c>: parse the
    /// candidate directly into the active settings; the valid early option
    /// survives the rejected late numeric row and is later saved into Safe.
    /// </summary>
    [Fact]
    public void LateSettingsConversionFailureDoesNotMutateTheActiveProfile()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Saver", WorldName = "Rune" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Command(panel, "settings save Safe");
        Command(panel, "opt set AttackDistance 0.02");
        string safe = panel.SelectedMacroProfile;

        VtankDatabase candidate = VtankDefaultSettingsDatabase.Create();
        VtankTable settings = candidate.Find("Settings")!;
        settings.Rows.Add(SettingRow(settings, "AttackDistance", VtankCell.Double(0.1d)));
        settings.Rows.Add(SettingRow(settings, "SpellDiffExcessThreshold-Hunt", new VtankCell
        {
            Tag = "i", ScalarText = "not-an-integer",
        }));
        string malformed = candidate.Render();
        storage.Text["mosstank/profiles/LateBroken.usd"] = malformed;

        Command(panel, "settings load LateBroken");
        Command(panel, "opt set EnableBuffing false");

        Assert.Equal(safe, panel.SelectedMacroProfile);
        Assert.Equal(0.02d, panel.EvaluateExpression(
            "uboptget[`AttackDistance`]").AsNumber(), precision: 7);
        Assert.Equal(malformed, storage.Text["mosstank/profiles/LateBroken.usd"]);
    }

    [Fact]
    public void SetInAllPreservesAnExistingEmptySettingsFile()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/profiles/Empty.usd"] = string.Empty;
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));

        Command(panel, "opt setinall AttackDistance 0.03");

        Assert.Equal(string.Empty, storage.Text["mosstank/profiles/Empty.usd"]);
    }

    [Fact]
    public void SwitchingToAProfileWithoutGemFoodClearsItsOwnedConsumables()
    {
        var storage = new MemoryStorage();
        storage.Text["mosstank/profiles/GemFood.usd"] = SettingsWithGemFood(("Blackmoor's Favor", 3811u));
        storage.Text["mosstank/profiles/NoGemFood.usd"] = SettingsWithoutGemFood();
        var automation = GemFoodAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, storage));

        Command(panel, "settings load GemFood");
        Command(panel, "settings load NoGemFood");
        panel.ToggleCombat();
        for (int tick = 0; tick < 8; tick++)
            panel.OnTick(0.3d);

        Assert.Empty(automation.UsedItemIds);
    }

    /// <summary>
    /// Mutation <c>DoNotRemoveImportedAssistSelection</c>: leave the selected
    /// imported name in the consumable collection; Save writes the removed
    /// row back and the exact-row assertion fails.
    /// </summary>
    [Fact]
    public void RemovingAnImportedConsumableDeletesOnlyItsExactAssistRow()
    {
        var storage = new MemoryStorage();
        VtankDatabase imported = VtankDefaultSettingsDatabase.Create();
        VtankTable table = imported.Find("AssistItems")!;
        table.ColumnNames.Add("Extension");
        table.IndexFlags.Add(false);
        table.Rows.Add(new VtankRow { Cells = { VtankCell.String("Bread"), VtankCell.Int(1), VtankCell.String("bread-custom") } });
        table.Rows.Add(new VtankRow { Cells = { VtankCell.String("Mana Cake"), VtankCell.Int(5), VtankCell.String("cake-custom") } });
        storage.Text["mosstank/profiles/AssistRemove.usd"] = imported.Render();
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));

        Command(panel, "settings load AssistRemove");
        Assert.Equal(["Bread", "Mana Cake"], panel.ConsumableRows);
        panel.SelectConsumableRow(0);

        VtankTable saved = VtankDatabase.Parse(storage.Text[SettingsKey(panel.SelectedMacroProfile)])
            .Find("AssistItems")!;
        int name = saved.ColumnIndex("Object");
        int extension = saved.ColumnIndex("Extension");
        Assert.DoesNotContain(saved.Rows, row => row.Cells[name].AsString() == "Bread");
        VtankRow cake = Assert.Single(saved.Rows, row => row.Cells[name].AsString() == "Mana Cake");
        Assert.Equal("cake-custom", cake.Cells[extension].AsString());
    }
    /// <summary>
    /// Mutation <c>RetainImportedAssistItemsAcrossRead</c>: omit the initial
    /// clear in the table reader; switching to the empty table leaves Bread
    /// visible in the new profile.
    /// </summary>
    [Fact]
    public void SwitchingToAnEmptyAssistItemsTableClearsImportedConsumables()
    {
        var storage = new MemoryStorage();
        VtankDatabase imported = VtankDefaultSettingsDatabase.Create();
        imported.Find("AssistItems")!.Rows.Add(new VtankRow
        {
            Cells = { VtankCell.String("Bread"), VtankCell.Int(1) },
        });
        storage.Text["mosstank/profiles/Assist.usd"] = imported.Render();
        storage.Text["mosstank/profiles/NoAssist.usd"] = VtankDefaultSettingsDatabase.Create().Render();
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));

        Command(panel, "settings load Assist");
        Assert.Contains("Bread", panel.ConsumableRows);
        Command(panel, "settings load NoAssist");

        Assert.DoesNotContain("Bread", panel.ConsumableRows);
    }

    /// <summary>
    /// The profile came in saying "[All Peas]" is an ordinary single pea, so
    /// the splitter ignores it. Pressing Add All Peas is the owner saying
    /// otherwise, and it has to take effect at once and still be there after
    /// a reload -- the name was already in the list, which is exactly why the
    /// edit used to be dropped on the floor.
    ///
    /// Mutation executed: <c>the explicit edit left the imported row alone
    /// and skipped the save when the name already existed</c> (the state
    /// before this fix). The splitter then keeps reading the imported kind
    /// and nothing is written to the profile.
    /// </summary>
    [Fact]
    public void AddingAllPeasOverAnImportedSinglePeaRowActsAndPersists()
    {
        var storage = new MemoryStorage();
        VtankDatabase imported = VtankDefaultSettingsDatabase.Create();
        VtankTable table = imported.Find("AssistItems")!;
        table.ColumnNames.Add("Extension");
        table.IndexFlags.Add(false);
        table.Rows.Add(new VtankRow
        {
            Cells =
            {
                VtankCell.String(CraftingPlanner.AllPeas),
                VtankCell.Int(9),
                VtankCell.String("keep-custom"),
            },
        });
        storage.Text["mosstank/profiles/AssistAllPeas.usd"] = imported.Render();
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));

        Command(panel, "settings load AssistAllPeas");
        Assert.DoesNotContain(CraftingPlanner.AllPeas, panel.LivePeaConsumables);

        panel.AddAllPeas();

        Assert.Contains(CraftingPlanner.AllPeas, panel.LivePeaConsumables);
        VtankTable saved = VtankDatabase.Parse(
            storage.Text[SettingsKey(panel.SelectedMacroProfile)]).Find("AssistItems")!;
        int name = saved.ColumnIndex("Object");
        VtankRow row = Assert.Single(saved.Rows,
            candidate => candidate.Cells[name].AsString() == CraftingPlanner.AllPeas);
        Assert.Equal(11, row.Cells[saved.ColumnIndex("Type")].AsInt());
        Assert.Equal("keep-custom",
            row.Cells[saved.ColumnIndex("Extension")].AsString());
    }

    /// <summary>
    /// The wildcard name and the single-pea kind are two halves of one rule,
    /// and a profile that pairs them the wrong way round means neither. The
    /// splitter must not take the name on its own or the kind on its own.
    ///
    /// Mutation executed: <c>the splitter was handed the consumable names
    /// without the imported-kind filter</c>. Both mismatched rows then
    /// authorize splitting.
    /// </summary>
    [Fact]
    public void MismatchedPeaNameAndKindPairsAuthorizeNothing()
    {
        var storage = new MemoryStorage();
        VtankDatabase imported = VtankDefaultSettingsDatabase.Create();
        VtankTable table = imported.Find("AssistItems")!;
        // The wildcard name carrying the single-pea kind, and a single pea
        // carrying the wildcard kind.
        table.Rows.Add(new VtankRow
        {
            Cells = { VtankCell.String(CraftingPlanner.AllPeas), VtankCell.Int(9) },
        });
        table.Rows.Add(new VtankRow
        {
            Cells = { VtankCell.String("Gold Pea"), VtankCell.Int(11) },
        });
        storage.Text["mosstank/profiles/AssistMismatch.usd"] = imported.Render();
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));

        Command(panel, "settings load AssistMismatch");

        Assert.Contains(CraftingPlanner.AllPeas, panel.ConsumableRows);
        Assert.Contains("Gold Pea", panel.ConsumableRows);
        Assert.Empty(panel.LivePeaConsumables);
    }

    /// <summary>
    /// Two rows for one name, each with its own custom columns, are the
    /// profile's business and not ours: saving something unrelated has to
    /// hand them back unchanged.
    ///
    /// Mutation executed: <c>a row whose kind is not the live one was dropped
    /// from the desired set on save</c>. The second Bread row and its custom
    /// cell are then lost the first time anything is saved.
    /// </summary>
    [Fact]
    public void DuplicateImportedRowsAndCustomCellsSurviveAnUnrelatedSave()
    {
        var storage = new MemoryStorage();
        VtankDatabase imported = VtankDefaultSettingsDatabase.Create();
        VtankTable table = imported.Find("AssistItems")!;
        table.ColumnNames.Add("Extension");
        table.IndexFlags.Add(false);
        table.Rows.Add(new VtankRow
        {
            Cells =
            {
                VtankCell.String("Bread"), VtankCell.Int(1),
                VtankCell.String("first-custom"),
            },
        });
        table.Rows.Add(new VtankRow
        {
            Cells =
            {
                VtankCell.String("Bread"), VtankCell.Int(5),
                VtankCell.String("second-custom"),
            },
        });
        storage.Text["mosstank/profiles/AssistDuplicate.usd"] = imported.Render();
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));

        Command(panel, "settings load AssistDuplicate");
        panel.ToggleAutoStack();

        VtankTable saved = VtankDatabase.Parse(
            storage.Text[SettingsKey(panel.SelectedMacroProfile)]).Find("AssistItems")!;
        int name = saved.ColumnIndex("Object");
        int type = saved.ColumnIndex("Type");
        int extension = saved.ColumnIndex("Extension");
        VtankRow[] bread = saved.Rows
            .Where(row => row.Cells[name].AsString() == "Bread")
            .ToArray();
        Assert.Equal(2, bread.Length);
        Assert.Equal([1, 5], bread.Select(row => row.Cells[type].AsInt()).ToArray());
        Assert.Equal(
            ["first-custom", "second-custom"],
            bread.Select(row => row.Cells[extension].AsString()).ToArray());
    }

    /// <summary>
    /// An explicit kind speaks for every row carrying that name. The second
    /// row and its custom cell are still the profile's business, so saying
    /// what a consumable is for must not be a way of deleting them.
    ///
    /// Mutation executed: <c>the explicit edit re-kinded only the first row
    /// for the name</c>. The second row then no longer matches what is wanted
    /// and is dropped, with its custom cell, on the same save.
    /// </summary>
    [Fact]
    public void AnExplicitKindReKindsEveryRowForTheNameAndKeepsThem()
    {
        var storage = new MemoryStorage();
        VtankDatabase imported = VtankDefaultSettingsDatabase.Create();
        VtankTable table = imported.Find("AssistItems")!;
        table.ColumnNames.Add("Extension");
        table.IndexFlags.Add(false);
        table.Rows.Add(new VtankRow
        {
            Cells =
            {
                VtankCell.String(CraftingPlanner.AllPeas), VtankCell.Int(9),
                VtankCell.String("first-custom"),
            },
        });
        table.Rows.Add(new VtankRow
        {
            Cells =
            {
                VtankCell.String(CraftingPlanner.AllPeas), VtankCell.Int(1),
                VtankCell.String("second-custom"),
            },
        });
        storage.Text["mosstank/profiles/AssistTwoPeaRows.usd"] = imported.Render();
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));

        Command(panel, "settings load AssistTwoPeaRows");
        panel.AddAllPeas();

        VtankTable saved = VtankDatabase.Parse(
            storage.Text[SettingsKey(panel.SelectedMacroProfile)]).Find("AssistItems")!;
        int name = saved.ColumnIndex("Object");
        VtankRow[] peas = saved.Rows
            .Where(row => row.Cells[name].AsString() == CraftingPlanner.AllPeas)
            .ToArray();
        Assert.Equal(2, peas.Length);
        Assert.Equal(
            [11, 11],
            peas.Select(row => row.Cells[saved.ColumnIndex("Type")].AsInt()).ToArray());
        Assert.Equal(
            ["first-custom", "second-custom"],
            peas.Select(row => row.Cells[saved.ColumnIndex("Extension")].AsString())
                .ToArray());
    }

    /// <summary>
    /// The profile says this bread is drunk for mana. The bag says it is
    /// food, and food restores health -- but the profile was explicit, and
    /// the periodic look through the bag must not quietly overrule it.
    ///
    /// Mutation executed: <c>the imported-row check was dropped from the
    /// periodic assessment sweep</c>. The bread is reclassified as health
    /// food on the first sweep.
    /// </summary>
    [Fact]
    public void ThePeriodicSweepDoesNotOverruleAnImportedConsumableKind()
    {
        var storage = new MemoryStorage();
        VtankDatabase imported = VtankDefaultSettingsDatabase.Create();
        imported.Find("AssistItems")!.Rows.Add(new VtankRow
        {
            Cells = { VtankCell.String("Bread"), VtankCell.Int(5) },
        });
        storage.Text["mosstank/profiles/AssistBread.usd"] = imported.Render();
        var automation = new FakeAutomation
        {
            ItemEntries =
            [
                Item(77u, "Bread", 0x00000020u) with { BoosterVital = 2 },
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation, storage));

        Command(panel, "settings load AssistBread");
        for (int tick = 0; tick < 4; tick++)
            panel.OnTick(0.6d);

        panel.ToggleAutoStack();
        VtankTable saved = VtankDatabase.Parse(
            storage.Text[SettingsKey(panel.SelectedMacroProfile)]).Find("AssistItems")!;
        int name = saved.ColumnIndex("Object");
        VtankRow row = Assert.Single(saved.Rows,
            candidate => candidate.Cells[name].AsString() == "Bread");
        Assert.Equal(5, row.Cells[saved.ColumnIndex("Type")].AsInt());
    }

    [Fact]
    public void UiAuthoredBuffConsumableStillUsesItsAppraisedSpell()
    {
        var automation = GemFoodAutomation("Ui Authored Gem");
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        host.Selection.Select(101u);
        panel.AddSelectedConsumable();
        panel.ToggleCombat();
        for (int tick = 0; tick < 8; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([101u], automation.UsedItemIds);
        Assert.Contains("Spell 3810", panel.BuffStatus, StringComparison.Ordinal);
    }

    [Fact]
    public void AutostartLogChannelsAreTheSessionsAndSurviveAProfileLoad()
    {
        var storage = new MemoryStorage();
        // An earlier session wrote the profile the run selects, with one
        // channel of the profile's own.
        var first = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Prover" }, storage));
        Command(first, "log ActiveRule on");
        Command(first, "settings save vt-proof-settings");

        var automation = new FakeAutomation { Name = "Prover" };
        var host = new FakeHost(automation, storage)
        {
            SessionSettings = new Dictionary<string, string>(StringComparer.Ordinal)
            {
                ["settingsProfile"] = "vt-proof-settings",
                ["logChannels"] = "SpellCast,RuleInfo",
            },
        };
        var panel = new MossTankPanel(host);
        panel.TickAutostart();
        Assert.Empty(host.Logger.Errors);
        Command(panel, "log");
        Assert.Equal("Log state:  ActiveRule RuleInfo SpellCast", LastLogState(automation));

        // The run reloads the same profile from outside, as the proof does.
        Command(panel, "settings load vt-proof-settings.usd");
        Command(panel, "log");
        Assert.Equal("Log state:  ActiveRule RuleInfo SpellCast", LastLogState(automation));

        // Turning a session channel off is honoured too.
        Command(panel, "log SpellCast off");
        Command(panel, "log");
        Assert.Equal("Log state:  ActiveRule RuleInfo", LastLogState(automation));
    }

    private static string LastLogState(FakeAutomation automation) =>
        automation.Messages.Last(message =>
            message.StartsWith("Log state:", StringComparison.Ordinal)
            || message == "Not currently logging.");

    [Fact]
    public void TheVtLogChannelSelectionSurvivesAReload()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Log Persist" };
        var first = new MossTankPanel(new FakeHost(automation, storage));

        Command(first, "log Misc on");
        Command(first, "log SpellCast on");
        Command(first, "log RuleInfo on");
        Command(first, "log RuleInfo off");

        var reloaded = new FakeAutomation { Name = "Log Persist" };
        var second = new MossTankPanel(new FakeHost(reloaded, storage));
        Command(second, "log");

        string state = Assert.Single(
            reloaded.Messages,
            message => message.StartsWith("Log state:", StringComparison.Ordinal));
        Assert.Equal("Log state:  Misc SpellCast", state);
    }

    [Fact]
    public void BuffPickerAddsToTheRequestedListAndAnyCellClickDeletes()
    {
        var automation = new FakeAutomation
        {
            KnownSelfBuffs =
            [
                Spell(1, 10, "Increases the caster's Strength by 10 points."),
                Spell(2, 20, "Increases the caster's Focus by 10 points."),
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Assert.False(panel.BuffPickerVisible);
        panel.ShowExtraBuffPicker();
        Assert.True(panel.BuffPickerVisible);
        Assert.Equal(2, panel.BuffPickerRows.Count);

        panel.SetBuffPickerSearchText("Spell 1");
        Assert.Equal(["Spell 1"], panel.BuffPickerRows);
        panel.PickBuffAt(0);
        Assert.False(panel.BuffPickerVisible);
        Assert.Equal(["Spell 1"], panel.ExtraBuffRows);
        Assert.Empty(panel.BlacklistedBuffFamilyRows);

        panel.ShowBlacklistedBuffPicker();
        panel.SetBuffPickerSearchText(string.Empty);
        panel.PickBuffAt(1); // "Spell 2" (sorted after "Spell 1")
        Assert.Equal(["Spell 2"], panel.BlacklistedBuffFamilyRows);

        panel.DeleteExtraBuffAt(0);
        Assert.Empty(panel.ExtraBuffRows);
        panel.DeleteBlacklistedBuffFamilyAt(0);
        Assert.Empty(panel.BlacklistedBuffFamilyRows);
    }

    [Fact]
    public void ItemProfilesPersistThroughHostScopedStorage()
    {
        var storage = new MemoryStorage();
        var firstAutomation = new FakeAutomation
        {
            ItemEntries = [Item(10, "Imperil Lens", 0x8000)],
        };
        var firstHost = new FakeHost(firstAutomation, storage);
        var first = new MossTankPanel(firstHost);
        firstHost.Selection.Select(10);
        first.AddSelectedItem();

        var second = new MossTankPanel(new FakeHost(
            new FakeAutomation(),
            storage));

        Assert.Contains("Imperil Lens", second.ItemProfileText, StringComparison.Ordinal);
        Assert.Contains(
            storage.Text.Keys,
            key => key.StartsWith("profiles/macro/", StringComparison.Ordinal));
    }

    [Fact]
    public void VitalThresholdsPersistWithTheProfile()
    {
        var storage = new MemoryStorage();
        var first = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));
        first.SetNormalHealth(0.42f);
        first.SetNoTargetMana(0.88f);
        first.ToggleHelpOthers();

        var second = new MossTankPanel(
            new FakeHost(new FakeAutomation(), storage));

        Assert.Equal(0.42f, second.NormalHealthValue, precision: 2);
        Assert.Equal(0.88f, second.NoTargetManaValue, precision: 2);
        Assert.False(second.HelpOthersEnabled);
    }

    [Fact]
    public void AutoStackAndAutoCramUseVtankDefaultsAndPersistPerProfile()
    {
        var storage = new MemoryStorage();
        var first = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));

        Assert.True(first.AutoStackEnabled);
        Assert.False(first.AutoCramEnabled);
        Assert.True(first.AutoCraftItemsEnabled);
        Assert.True(first.RefillWornManaEnabled);
        first.ToggleAutoStack();
        first.ToggleAutoCram();
        first.ToggleAutoCraftItems();
        first.ToggleRefillWornMana();
        first.SetRefillWornMana(0.44f);

        var second = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));
        Assert.False(second.AutoStackEnabled);
        Assert.True(second.AutoCramEnabled);
        Assert.False(second.AutoCraftItemsEnabled);
        Assert.False(second.RefillWornManaEnabled);
        Assert.Equal(0.44f, second.RefillWornManaValue, precision: 2);
    }

    [Fact]
    public void VitalsPercentWrappersReadAndWriteTheSameFieldAsTheZeroToOnePair()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), new MemoryStorage()));

        panel.SetNormalHealth(0.42f);
        Assert.Equal(42f, panel.NormalHealthPercent, precision: 2);

        panel.SetHelperManaPercent(65f);
        Assert.Equal(0.65f, panel.HelperManaValue, precision: 3);
        Assert.Equal(65f, panel.HelperManaPercent, precision: 2);
    }

    [Fact]
    public void FastCastProjectileAwarenessAndDebuffFallbackToggleTheirVtankDefaultsAndPersist()
    {
        var storage = new MemoryStorage();
        var first = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));

        Assert.False(first.FastCastBuffsEnabled);
        Assert.True(first.DontShootAtWallsEnabled);
        Assert.False(first.DebuffFallbackEnabled);

        first.ToggleFastCastBuffs();
        first.ToggleDontShootAtWalls();
        first.ToggleDebuffFallback();

        Assert.True(first.FastCastBuffsEnabled);
        Assert.False(first.DontShootAtWallsEnabled);
        Assert.True(first.DebuffFallbackEnabled);

        var second = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));
        Assert.True(second.FastCastBuffsEnabled);
        Assert.False(second.DontShootAtWallsEnabled);
        Assert.True(second.DebuffFallbackEnabled);
    }

    [Fact]
    public void ToggleAdvancedOptionsAndLootEditorVisibilityFlipBothWays()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), new MemoryStorage()));

        Assert.False(panel.AdvancedOptionsVisible);
        panel.ToggleAdvancedOptionsVisible();
        Assert.True(panel.AdvancedOptionsVisible);
        panel.ToggleAdvancedOptionsVisible();
        Assert.False(panel.AdvancedOptionsVisible);

        Assert.False(panel.LootEditorVisible);
        panel.ToggleLootEditorVisible();
        Assert.True(panel.LootEditorVisible);
        panel.ToggleLootEditorVisible();
        Assert.False(panel.LootEditorVisible);
    }

    [Fact]
    public void AdvancedOptionCategoryNamesShowRealNamesNotHexBitmasks()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        Assert.Equal(
            [
                "Misc", "Recharge", "MeleeCombat", "SpellCombat", "Ranges",
                "Navigation", "Buffing", "Crafting", "Looting",
            ],
            panel.AdvancedOptionCategoryNames);
        Assert.DoesNotContain(
            panel.AdvancedOptionCategoryNames, name => name.StartsWith("0x", StringComparison.Ordinal));
    }

    [Fact]
    public void AdvancedOptionCategoryFilterHidesNonMatchingSettings()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        Assert.Contains("EnableLooting", panel.AdvancedOptionNames);
        Assert.Contains("EnableNav", panel.AdvancedOptionNames);

        for (int i = 0; i < panel.AdvancedOptionCategoryEnabled.Count; i++)
            if (VtankOptionCatalog.CategoryBits[i] != 0x100)
                panel.ToggleAdvancedOptionCategoryAt(i);

        Assert.Contains("EnableLooting", panel.AdvancedOptionNames);
        Assert.DoesNotContain("EnableNav", panel.AdvancedOptionNames);

        // Re-enabling everything restores the full list.
        for (int i = 0; i < panel.AdvancedOptionCategoryEnabled.Count; i++)
            if (!panel.AdvancedOptionCategoryEnabled[i])
                panel.ToggleAdvancedOptionCategoryAt(i);
        Assert.Contains("EnableNav", panel.AdvancedOptionNames);
    }

    [Fact]
    public void AdvancedOptionListHidesTStringSettingsButKeepsTheirEnumCounterparts()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        Assert.DoesNotContain("BuffProfile-Prots", panel.AdvancedOptionNames);
        Assert.DoesNotContain("BuffProfile-Banes", panel.AdvancedOptionNames);
        Assert.DoesNotContain("BlacklistedSpellComps", panel.AdvancedOptionNames);

        Assert.Contains("BuffProfile_Prots", panel.AdvancedOptionNames);
        Assert.Contains("BuffProfile_Banes", panel.AdvancedOptionNames);
    }

    /// <summary>
    /// A setting nothing in the macro reads is not an option here at all: a
    /// row the player can change that changes nothing is worse than no row.
    /// A profile written elsewhere may still carry the name; it is ignored
    /// on load like any other name the macro does not know.
    ///
    /// Mutation: put the name back in the option catalog and the row returns.
    /// </summary>
    [Fact]
    public void ASettingNothingReadsIsNotAnOption()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        Assert.DoesNotContain("WhoYouGonnaCall", panel.AdvancedOptionNames);
        Assert.DoesNotContain("WhoYouGonnaCall", VtankOptionCatalog.Names);
    }

    /// <summary>
    /// The advanced list shows the four plain value kinds -- switch, choice,
    /// whole number and decimal -- and nothing else. A table-valued setting has
    /// no plain value to type, and an edit made against it here would be
    /// dropped without a word, so it is not offered at all; the recharge
    /// handler table is the only one of its kind.
    ///
    /// Mutation: filter only text settings out again and the table reappears
    /// in the list as an editable row.
    /// </summary>
    [Fact]
    public void AdvancedOptionListOffersOnlyThePlainValueKinds()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        Assert.DoesNotContain("RechargeHandlerSet", panel.AdvancedOptionNames);

        foreach (string name in panel.AdvancedOptionNames)
        {
            Assert.Contains(
                VtankOptionCatalog.DeclaredType(name),
                new[]
                {
                    VtankSettingValueType.Bool,
                    VtankSettingValueType.Enum,
                    VtankSettingValueType.Int,
                    VtankSettingValueType.Double,
                });
        }
    }

    [Fact]
    public void AdvancedOptionCategoryEnabledIsNotTheMutableBackingArray()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        IReadOnlyList<bool> categoryEnabled = panel.AdvancedOptionCategoryEnabled;

        Assert.Throws<InvalidCastException>(() => (bool[])categoryEnabled);
    }

    [Fact]
    public void AdvancedOptionDescriptionSurfacesRealAuthenticHelpText()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        int index = panel.AdvancedOptionNames.ToList().IndexOf("DoHelp");
        Assert.True(index >= 0);

        panel.SelectAdvancedOption(index);

        Assert.StartsWith("DoHelp:", panel.AdvancedOptionDescription, StringComparison.Ordinal);
        Assert.Contains("fellowship", panel.AdvancedOptionDescription, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void AdvancedOptionValueColumnMirrorsTheLiveSettingValue()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        int index = panel.AdvancedOptionNames.ToList().IndexOf("EnableCombat");
        Assert.True(index >= 0);
        string before = panel.AdvancedOptionValueColumn[index];
        Assert.Equal(panel.CombatEnabled ? "True" : "False", before);

        panel.ToggleCombatEnabled();
        panel.SelectAdvancedOption(index);

        string after = panel.AdvancedOptionValueColumn[index];
        Assert.NotEqual(before, after);
        Assert.Equal(panel.CombatEnabled ? "True" : "False", after);
    }

    [Fact]
    public void ShowAdvancedOptionsRefreshesValuesChangedWhileThePopupWasClosed()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        int index = panel.AdvancedOptionNames.ToList().IndexOf("EnableCombat");
        Assert.True(index >= 0);
        string before = panel.AdvancedOptionValueColumn[index];
        Assert.Equal(panel.CombatEnabled ? "True" : "False", before);

        panel.ToggleCombatEnabled();
        panel.ShowAdvancedOptions();

        string after = panel.AdvancedOptionValueColumn[index];
        Assert.NotEqual(before, after);
        Assert.Equal(panel.CombatEnabled ? "True" : "False", after);
    }

    [Fact]
    public void AdvancedOptionsPopupBindingsDoNotReallocateOnEveryRead()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        Assert.Same(panel.AdvancedOptionNames, panel.AdvancedOptionNames);
        Assert.Same(panel.AdvancedOptionValueColumn, panel.AdvancedOptionValueColumn);

        int index = panel.AdvancedOptionNames.ToList().IndexOf("EnableCombat");
        Assert.True(index >= 0);
        panel.SelectAdvancedOption(index);

        IReadOnlyList<string> namesAfterSelect = panel.AdvancedOptionNames;
        IReadOnlyList<string> valuesAfterSelect = panel.AdvancedOptionValueColumn;
        // ...but once settled, repeated reads must again share one instance.
        Assert.Same(namesAfterSelect, panel.AdvancedOptionNames);
        Assert.Same(valuesAfterSelect, panel.AdvancedOptionValueColumn);
    }

    [Fact]
    public void ClickAdvancedOptionValue_OnBoolRow_FlipsInPlace()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        int index = panel.AdvancedOptionNames.ToList().IndexOf("EnableCombat");
        Assert.True(index >= 0);
        bool before = panel.CombatEnabled;

        panel.ClickAdvancedOptionValue(index);

        Assert.Equal(!before, panel.CombatEnabled);
        Assert.Equal(panel.CombatEnabled ? "True" : "False", panel.AdvancedOptionValueColumn[index]);

        panel.ClickAdvancedOptionValue(index);
        Assert.Equal(before, panel.CombatEnabled);
    }

    [Fact]
    public void ClickAdvancedOptionValue_OnEnumRow_CyclesToTheNextLabelAndWraps()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        int index = panel.AdvancedOptionNames.ToList().IndexOf("UseArcs");
        Assert.True(index >= 0);

        panel.SelectAdvancedOption(index);
        panel.SubmitAdvancedOption("1");
        Assert.Equal("No", panel.AdvancedOptionValueColumn[index]);

        panel.ClickAdvancedOptionValue(index);
        Assert.Equal("At Range", panel.AdvancedOptionValueColumn[index]);

        panel.ClickAdvancedOptionValue(index);
        Assert.Equal("Yes", panel.AdvancedOptionValueColumn[index]);

        panel.ClickAdvancedOptionValue(index);
        Assert.Equal("No", panel.AdvancedOptionValueColumn[index]);
    }

    [Fact]
    public void ClickAdvancedOptionValue_OnNumericRow_SelectsAndLoadsTheEditField()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        int index = panel.AdvancedOptionNames.ToList().IndexOf("SpellDiffExcessThreshold-Hunt");
        Assert.True(index >= 0);

        panel.ClickAdvancedOptionValue(index);

        Assert.Equal(index, panel.SelectedAdvancedOptionIndex);
        Assert.Equal(panel.AdvancedOptionValueColumn[index], panel.AdvancedOptionValueDraft);
    }

    [Fact]
    public void CombatMacroRunningReflectsTheSameStateAsCombatButtonText()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), new MemoryStorage()));

        Assert.False(panel.CombatMacroRunning);
        Assert.Equal("Run Macro", panel.CombatButtonText);

        panel.ToggleCombat();

        Assert.True(panel.CombatMacroRunning);
        Assert.Equal("Stop Macro", panel.CombatButtonText);
    }

    [Fact]
    public void LootingUsesVtankDefaultsAndPersistsTheOrderedRuleEditor()
    {
        var storage = new MemoryStorage();
        var first = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));

        Assert.False(first.LootEnabled);
        Assert.False(first.LootPriorityBoostEnabled);
        Assert.Empty(first.LootRuleRows);
        first.ToggleLooting();
        first.ToggleLootPriorityBoost();
        first.AddLootRule();
        first.SetLootExpressionDraft("name ~= coin && value >= 10");
        first.ApplyLootRule();
        first.SelectLootAction(nameof(LootAction.KeepUpTo));
        first.LootKeepCountUp();
        first.LootPriorityUp();
        first.LootRangeDown();

        var second = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));
        Assert.True(second.LootEnabled);
        Assert.True(second.LootPriorityBoostEnabled);
        Assert.Single(second.LootRuleRows);
        Assert.Contains("KeepUpTo", second.LootRuleRows[0], StringComparison.Ordinal);
        Assert.Contains("name ~= coin", second.LootRuleRows[0], StringComparison.Ordinal);
        Assert.Equal("Keep up to 2", second.LootKeepCountText);
        Assert.Equal("Priority 1", second.LootPriorityText);
        // The shipped corpse range is zero — the walk to a corpse is off until
        // a profile asks for it — and the down button floors there.
        Assert.Equal("Corpse range 0m", second.LootRangeText);
    }

    private static string LegacyLootProfileKey(string value, bool byCharacter)
    {
        string identity = (byCharacter ? "char:" : "named:") + value.Trim().ToUpperInvariant();
        string hash = Convert.ToHexString(
            System.Security.Cryptography.SHA256.HashData(
                System.Text.Encoding.UTF8.GetBytes(identity)));
        return $"profiles/loot/{hash}.json";
    }

    [Fact]
    public void LootRosterSweepConvertsByCharacterAndEveryNamedLegacyProfileOnce()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Barris" };
        storage.Text[LegacyLootProfileKey("Barris", byCharacter: true)] = """
            {
              "Rules": [
                { "Name": "Coins", "Expression": "name ~= coin", "Action": 0, "Priority": 3 }
              ]
            }
            """;
        storage.Text["profiles/loot/index.json"] = """
            {
              "Version": 1,
              "Names": ["Farming"],
              "SelectedByCharacter": {}
            }
            """;
        storage.Text[LegacyLootProfileKey("Farming", byCharacter: false)] = """
            {
              "Rules": [
                { "Name": "Salvage", "Expression": "name ~= salvage", "Action": 0, "Priority": 1 }
              ]
            }
            """;

        var panel = new MossTankPanel(new FakeHost(automation, storage));

        string byCharacterFile = LootKey(
            VtankProfileDirectory.AutoCharacterFileName("Barris", string.Empty, "utl"));
        Assert.True(storage.Text.ContainsKey(byCharacterFile));
        Assert.True(storage.Text.ContainsKey("mosstank/loot/Farming.utl"));
        Assert.False(storage.Text.ContainsKey(LegacyLootProfileKey("Barris", byCharacter: true)));
        Assert.False(storage.Text.ContainsKey(LegacyLootProfileKey("Farming", byCharacter: false)));
        Assert.False(storage.Text.ContainsKey("profiles/loot/index.json"));

        Assert.Contains("name ~= coin", Assert.Single(panel.LootRuleRows), StringComparison.Ordinal);

        panel.SelectLootProfile("Farming");
        Assert.Contains("name ~= salvage", Assert.Single(panel.LootRuleRows), StringComparison.Ordinal);

        // Idempotent: a fresh panel against the same storage sweeps nothing
        // more (there is no roster key left to read) and keeps both values.
        var reloaded = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Barris" }, storage));
        Assert.Contains("name ~= coin", Assert.Single(reloaded.LootRuleRows), StringComparison.Ordinal);
    }

    [Fact]
    public void LootProfilesAreIndependentNamedDocuments()
    {
        var storage = new MemoryStorage();
        var panel = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Looter" },
            storage));
        panel.AddLootRule();
        panel.SetLootExpressionDraft("name ~= coin");
        panel.ApplyLootRule();
        Vt(panel, "loot save Currency"); // save copies the current rules under the new name
        panel.AddLootRule();

        Assert.Equal("Currency", panel.LootProfileName);
        Assert.Equal(2, panel.LootRuleRows.Count);
        panel.SelectLootProfile(MossTankLootProfileStore.ByCharacter);
        Assert.Single(panel.LootRuleRows);
        panel.SelectLootProfile("Currency");
        Assert.Equal(2, panel.LootRuleRows.Count);
        Assert.True(storage.Text.ContainsKey("mosstank/loot/Currency.utl"));
    }

    /// <summary>
    /// A picker change is one activation transaction: an unreadable candidate
    /// cannot become the selected label or character binding while the previous
    /// rules continue to execute.
    /// Mutation <c>PrematureSelectionCommit</c>: commit <c>_selected</c> and
    /// the CDF in <c>Select</c>, before
    /// <c>LoadCurrent</c> validates the candidate; the selected-name and CDF
    /// assertions fail.
    /// </summary>
    [Fact]
    public void LootProfilePickerRejectsMalformedCandidateWithoutChangingActiveProfileOrFiles()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation
        {
            Name = "Looter",
            WorldName = "Coldeve",
        };
        var host = new FakeHost(automation, storage);
        var panel = new MossTankPanel(host);
        Command(panel, "loot new Safe");
        panel.AddLootRule();
        string[] safeRules = panel.LootRuleRows.ToArray();
        const string malformed = "UTL\r\n1\r\n1\r\nunfinished";
        storage.Text["mosstank/loot/Loot5.utl"] = malformed;
        string cdfKey = VtankProfileDirectory.CdfFileName("Looter", "Coldeve");
        string cdfBefore = storage.Text[cdfKey];

        panel.SelectLootProfile("Loot5");

        Assert.Equal("Safe", panel.LootProfileName);
        Assert.Equal(safeRules, panel.LootRuleRows);
        Assert.Equal(cdfBefore, storage.Text[cdfKey]);
        Assert.Equal(malformed, storage.Text["mosstank/loot/Loot5.utl"]);
        Assert.Contains("Loot5", panel.LootEditorNotice, StringComparison.Ordinal);
        Assert.Contains("Active profile remains Safe", panel.LootEditorNotice, StringComparison.Ordinal);
        Assert.DoesNotContain(host.Logger.Infos, message =>
            message.Contains("Loaded loot profile Loot5", StringComparison.Ordinal));
    }

    /// <summary>
    /// Mutation <c>AllowPartialLootCopy</c>: remove the partial-source guard
    /// in <c>MossTankLootProfileStore.Create</c>; the command replaces the
    /// destination and changes the active binding.
    /// </summary>
    [Fact]
    public void LootSaveCommandRefusesToCopyAnIncompleteActiveProfile()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation
        {
            Name = "Looter",
            WorldName = "Coldeve",
        };
        var host = new FakeHost(automation, storage);
        var panel = new MossTankPanel(host);
        const string partial = "UTL\r\n1\r\n2\r\nKeep\r\n\r\n0;1\r\nunfinished";
        storage.Text["mosstank/loot/Partial.utl"] = partial;

        Command(panel, "loot load Partial");
        panel.AddLootRule();
        string[] rowsBeforeSave = panel.LootRuleRows.ToArray();
        string cdfKey = VtankProfileDirectory.CdfFileName("Looter", "Coldeve");
        string bindingBefore = storage.Text[cdfKey];

        Vt(panel, "loot save Copy");

        Assert.Equal("Partial", panel.LootProfileName);
        Assert.Equal(rowsBeforeSave, panel.LootRuleRows);
        Assert.Equal(partial, storage.Text["mosstank/loot/Partial.utl"]);
        Assert.False(storage.Text.ContainsKey("mosstank/loot/Copy.utl"));
        Assert.Equal(bindingBefore, storage.Text[cdfKey]);
        Assert.Contains(automation.Messages, message =>
            message.Contains("incomplete", StringComparison.OrdinalIgnoreCase));
    }

    /// <summary>
    /// Mutation <c>AllowInactiveLootCopy</c>: remove the no-active-profile
    /// branch in <c>MossTankLootProfileStore.Create</c>; the command creates
    /// a new document from an untrusted inactive startup profile.
    /// </summary>
    [Fact]
    public void LootSaveCommandRefusesWithAnInactiveStartupProfile()
    {
        var storage = new MemoryStorage();
        var firstAutomation = new FakeAutomation
        {
            Name = "Looter",
            WorldName = "Coldeve",
        };
        var first = new MossTankPanel(new FakeHost(firstAutomation, storage));
        Command(first, "loot new Broken");
        const string malformed = "UTL\r\n1\r\n1\r\nunfinished";
        storage.Text["mosstank/loot/Broken.utl"] = malformed;
        string cdfKey = VtankProfileDirectory.CdfFileName("Looter", "Coldeve");
        string bindingBefore = storage.Text[cdfKey];
        var automation = new FakeAutomation
        {
            Name = "Looter",
            WorldName = "Coldeve",
        };
        var panel = new MossTankPanel(new FakeHost(automation, storage));

        Vt(panel, "loot save Copy");

        Assert.Equal(MossTankLootProfileStore.NoActiveProfile, panel.LootProfileName);
        Assert.Equal(malformed, storage.Text["mosstank/loot/Broken.utl"]);
        Assert.False(storage.Text.ContainsKey("mosstank/loot/Copy.utl"));
        Assert.Equal(bindingBefore, storage.Text[cdfKey]);
        Assert.Contains(automation.Messages, message =>
            message.Contains("no complete active profile", StringComparison.OrdinalIgnoreCase));
    }
    /// <summary>
    /// The command path must surface the same rejected activation and must not
    /// append its old unconditional success line.
    /// Mutation <c>UnconditionalLootLoadedMessage</c>: emit the old success
    /// message after the rejected load; the single-message assertion fails.
    /// </summary>
    [Fact]
    public void LootLoadCommandReportsMalformedCandidateWithoutAFalseLoadedMessage()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation
        {
            Name = "Looter",
            WorldName = "Coldeve",
        };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Command(panel, "loot new Safe");
        panel.AddLootRule();
        const string malformed = "UTL\r\n1\r\n1\r\nunfinished";
        storage.Text["mosstank/loot/Loot5.utl"] = malformed;
        automation.Messages.Clear();

        Command(panel, "loot load Loot5.utl");

        Assert.Equal("Safe", panel.LootProfileName);
        Assert.Contains(
            automation.Messages,
            message => message.Contains("Loot5", StringComparison.Ordinal)
                && message.Contains("could not be read", StringComparison.Ordinal)
                && message.Contains("Active profile remains Safe", StringComparison.Ordinal));
        // The failure line names the file it tried, and nothing claims a load.
        Assert.Contains(
            automation.Messages,
            message => message.Contains(
                "Could not load loot profile Loot5.utl",
                StringComparison.Ordinal));
        Assert.DoesNotContain(
            automation.Messages,
            message => message.Contains("Loaded loot profile", StringComparison.Ordinal));
        Assert.Equal(malformed, storage.Text["mosstank/loot/Loot5.utl"]);
    }

    /// <summary>
    /// Reloading the already-active name still validates the file before any
    /// save. An external truncation must not be repaired from stale memory.
    /// Mutation <c>OverwriteMalformedCurrentOnSave</c>: continue through
    /// <c>SaveCurrent</c> after the existing file fails to parse; the raw-file
    /// assertion fails.
    /// </summary>
    [Fact]
    public void ReloadingActiveLootProfileDoesNotOverwriteExternalMalformedEdit()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation
        {
            Name = "Looter",
            WorldName = "Coldeve",
        };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Command(panel, "loot new Safe");
        panel.AddLootRule();
        const string malformed = "UTL\r\n1\r\n1\r\nunfinished";
        storage.Text["mosstank/loot/Safe.utl"] = malformed;

        Command(panel, "loot load Safe");
        panel.ToggleLootPriorityBoost();

        Assert.Equal("Safe", panel.LootProfileName);
        Assert.Equal(malformed, storage.Text["mosstank/loot/Safe.utl"]);
        Assert.Contains(automation.Messages, message =>
            message.Contains("could not be read", StringComparison.Ordinal));
    }

    /// <summary>
    /// Mutation <c>AllowMalformedCopyTarget</c>: skip the target validation
    /// in <c>MossTankLootProfileStore.Create</c>; the raw-file assertion
    /// fails.
    /// </summary>
    [Fact]
    public void LootSaveCommandDoesNotOverwriteMalformedDestination()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation
        {
            Name = "Looter",
            WorldName = "Coldeve",
        };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Command(panel, "loot new Source");
        panel.AddLootRule();
        const string malformed = "UTL\r\n1\r\n1\r\nunfinished";
        storage.Text["mosstank/loot/Target.utl"] = malformed;

        Command(panel, "loot save Target");

        Assert.Equal("Source", panel.LootProfileName);
        Assert.Equal(malformed, storage.Text["mosstank/loot/Target.utl"]);
        Assert.Contains(automation.Messages, message =>
            message.Contains("Cannot overwrite unreadable", StringComparison.Ordinal));
    }
    /// <summary>
    /// A malformed profile named by the startup CDF cannot inherit the settings
    /// sidecar's old rules or use its external classifier. It remains inactive,
    /// and ordinary UI saves cannot overwrite either source file.
    /// Mutation <c>IgnoreInactiveProfileInLootTick</c>: remove the inactive
    /// guard from <c>LootController.Tick</c>; the corpse-open assertion fails.
    /// </summary>
    [Fact]
    public void MalformedStartupLootBindingIsInactiveAndCannotBeOverwrittenByLaterSaves()
    {
        var storage = new MemoryStorage();
        var classifiers = new FakeLootClassifierRegistry(
            new PluginLootClassifierInfo("utility/loot", "Utility Loot"));
        var firstAutomation = new FakeAutomation
        {
            Name = "Looter",
            WorldName = "Coldeve",
        };
        var first = new MossTankPanel(new FakeHost(
            firstAutomation,
            storage,
            classifiers));
        Command(first, "loot new Loot5");
        first.AddLootRule();
        first.SelectLootClassifier("Utility Loot [utility/loot]");
        if (!first.LootEnabled)
            first.ToggleLooting();

        const string malformed = "UTL\r\n1\r\n1\r\nunfinished";
        storage.Text["mosstank/loot/Loot5.utl"] = malformed;
        string cdfKey = VtankProfileDirectory.CdfFileName("Looter", "Coldeve");
        string cdfBefore = storage.Text[cdfKey];
        var loot = new FrameLootSurface
        {
            Corpses =
            [
                new PluginLootContainer(
                    FrameLootSurface.CorpseId,
                    1u,
                    "Corpse",
                    3f,
                    false,
                    false,
                    false)
                {
                    IsIdentified = true,
                    LongDescription = "Killed by Looter.",
                },
            ],
        };
        var restartedAutomation = new FakeAutomation
        {
            Name = "Looter",
            WorldName = "Coldeve",
            LootSurface = loot,
        };
        var restartedHost = new FakeHost(
            restartedAutomation,
            storage,
            classifiers);

        var restarted = new MossTankPanel(restartedHost);

        Assert.Equal(MossTankLootProfileStore.NoActiveProfile, restarted.LootProfileName);
        Assert.Empty(restarted.LootRuleRows);
        Assert.Equal("Utility Loot [utility/loot]", restarted.SelectedLootClassifier);
        Assert.Contains("No loot profile is active", restarted.LootEditorNotice, StringComparison.Ordinal);
        Assert.Contains(restartedHost.Logger.Errors, message =>
            message.Contains("Loot5", StringComparison.Ordinal)
                && message.Contains("No loot profile is active", StringComparison.Ordinal));

        restarted.ToggleLootPriorityBoost();
        restarted.ToggleCombat();
        for (int tick = 0; tick < 12; tick++)
            restarted.OnTick(0.3d);

        Assert.Equal(0u, loot.Opened);
        Assert.Equal(malformed, storage.Text["mosstank/loot/Loot5.utl"]);
        Assert.Equal(cdfBefore, storage.Text[cdfKey]);

        var recovered = new VtankLootProfile
        {
            Rules = [new LootRule { Expression = "*", Action = LootAction.Keep }],
        };
        storage.Text["mosstank/loot/Recovered.utl"] = VtankLootProfileSerializer.Write(recovered);
        restarted.SelectLootProfile("Recovered");

        Assert.Equal("Recovered", restarted.LootProfileName);
        Assert.Single(restarted.LootRuleRows);
        Assert.Equal(malformed, storage.Text["mosstank/loot/Loot5.utl"]);
        Assert.Contains(
            "Recovered.utl",
            storage.Text[cdfKey],
            StringComparison.Ordinal);
        for (int tick = 0; tick < 12 && loot.Opened == 0u; tick++)
            restarted.OnTick(0.3d);
        Assert.Equal(FrameLootSurface.CorpseId, loot.Opened);
    }

    /// <summary>
    /// A named CDF target that is absent is a failed activation, not the
    /// first-run By-char case. It must not be created from inherited sidecar
    /// rules by the constructor or a later save.
    /// Mutation <c>TreatMissingNamedBindingAsFirstRun</c>: route every missing
    /// candidate through the By-char creation branch; the file appears.
    /// </summary>
    [Fact]
    public void MissingNamedStartupLootBindingIsNotCreatedFromInheritedRules()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation
        {
            Name = "Looter",
            WorldName = "Coldeve",
        };
        var first = new MossTankPanel(new FakeHost(automation, storage));
        Command(first, "loot new MissingLater");
        first.AddLootRule();
        storage.Text.Remove("mosstank/loot/MissingLater.utl");
        string cdfKey = VtankProfileDirectory.CdfFileName("Looter", "Coldeve");
        string cdfBefore = storage.Text[cdfKey];

        var restartedHost = new FakeHost(
            new FakeAutomation { Name = "Looter", WorldName = "Coldeve" },
            storage);
        var restarted = new MossTankPanel(restartedHost);
        restarted.ToggleLootPriorityBoost();

        Assert.False(storage.Text.ContainsKey("mosstank/loot/MissingLater.utl"));
        Assert.Equal(MossTankLootProfileStore.NoActiveProfile, restarted.LootProfileName);
        Assert.Empty(restarted.LootRuleRows);
        Assert.Equal(cdfBefore, storage.Text[cdfKey]);
        Assert.Contains(restartedHost.Logger.Errors, message =>
            message.Contains("MissingLater", StringComparison.Ordinal)
                && message.Contains("not found", StringComparison.Ordinal));
    }

    /// <summary>
    /// A settings-profile load may replace the in-memory rule list from its
    /// sidecar before the active loot file is checked. If that file was
    /// truncated externally, the rules that were active before the settings
    /// switch remain active and the source stays untouched.
    /// Mutation <c>DropActiveRuleSnapshotRestore</c>: disable the snapshot
    /// restore branch; the rule count falls back to the older sidecar copy.
    /// </summary>
    [Fact]
    public void SettingsSwitchRetainsActiveLootRulesWhenActiveLootFileBecameMalformed()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation
        {
            Name = "Looter",
            WorldName = "Coldeve",
        };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Command(panel, "loot new Safe");
        panel.AddLootRule();
        Command(panel, "settings save OneRule");
        Command(panel, "settings save TwoRules");
        panel.AddLootRule();
        Assert.Equal(2, panel.LootRuleRows.Count);
        const string malformed = "UTL\r\n1\r\n1\r\nunfinished";
        storage.Text["mosstank/loot/Safe.utl"] = malformed;

        Command(panel, "settings load OneRule");
        panel.ToggleLootPriorityBoost();

        Assert.Equal(2, panel.LootRuleRows.Count);
        Assert.Equal(malformed, storage.Text["mosstank/loot/Safe.utl"]);
        Assert.Contains("could not be read", panel.LootEditorNotice, StringComparison.Ordinal);
    }

    /// <summary>
    /// Character binding starts a new ownership scope. A corrupt profile for
    /// the next character cannot restore the previous character's rules after
    /// the settings sidecar has loaded.
    /// Mutation <c>RestoreRulesAcrossCharacterRebind</c>: restore the pre-bind
    /// snapshot after every failed load; the empty-row assertion exposes the
    /// old character's rules.
    /// </summary>
    [Fact]
    public void CharacterRebindToMalformedLootProfileDoesNotRetainPreviousCharactersRules()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation
        {
            Name = "First",
            WorldName = "Coldeve",
        };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Command(panel, "loot new FirstLoot");
        panel.AddLootRule();
        Assert.Single(panel.LootRuleRows);

        string secondSettings = SettingsKey(
            VtankProfileDirectory.AutoCharacterFileName("Second", "Coldeve", "usd"));
        storage.Text[secondSettings] = VtankDefaultSettingsDatabase.Create().Render();
        const string malformed = "UTL\r\n1\r\n1\r\nunfinished";
        storage.Text["mosstank/loot/SecondLoot.utl"] = malformed;
        VtankProfileDirectory.WriteCharacterBinding(
            storage,
            "Second",
            "Coldeve",
            new VtankProfileDirectory.VtankCharacterBinding(
                secondSettings,
                "SecondLoot.utl",
                string.Empty,
                null));

        automation.Name = "Second";
        panel.OnTick(0.1d);

        Assert.Equal(MossTankLootProfileStore.NoActiveProfile, panel.LootProfileName);
        Assert.Empty(panel.LootRuleRows);
        Assert.Equal(malformed, storage.Text["mosstank/loot/SecondLoot.utl"]);
        Assert.Contains("No loot profile is active", panel.LootEditorNotice, StringComparison.Ordinal);
    }

    [Fact]
    public void LootClassifierSelectionIsVisibleAndPersistsWithMacroProfile()
    {
        var storage = new MemoryStorage();
        var classifiers = new FakeLootClassifierRegistry(
            new PluginLootClassifierInfo("utility/loot", "Utility Loot"));
        var panel = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Looter" },
            storage,
            classifiers));

        Assert.Contains("Utility Loot [utility/loot]", panel.LootClassifierNames);
        panel.SelectLootClassifier("Utility Loot [utility/loot]");

        Assert.Equal(
            "Utility Loot [utility/loot]",
            panel.SelectedLootClassifier);
        var restored = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Looter" },
            storage,
            classifiers));
        Assert.Equal(
            "Utility Loot [utility/loot]",
            restored.SelectedLootClassifier);

        restored.SelectLootClassifier("VTClassic");
        Assert.Equal("VTClassic", restored.SelectedLootClassifier);
    }

    [Fact]
    public void VtankRecoveryAndFakeImperilCommandsHaveRealLocalSemantics()
    {
        var automation = new FakeAutomation
        {
            BusyReferences = 2,
            WorldObjects =
            [
                new PluginWorldObject(
                    0x50000001u,
                    123u,
                    "Drudge",
                    PluginObjectClass.Monster,
                    0u,
                    0u,
                    0u),
            ],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        Command(panel, "clearbusy");
        Assert.Equal(1, automation.BusyReferences);
        Assert.Contains(automation.Messages,
            text => text.Contains("2 -> 1", StringComparison.Ordinal));

        Command(panel, "clearlocks");
        Assert.Contains(automation.Messages,
            text => text.Contains("Action locks cleared", StringComparison.Ordinal));

        host.Selection.Select(0x50000001u);
        Command(panel, "fakeimp");
        Assert.Contains(automation.Messages,
            text => text.Contains("Fake cast complete", StringComparison.Ordinal));
    }

    /// <summary>
    /// Mutation <c>OmitLootSettingsFromCopy</c>: do not pass the active loot
    /// settings to <c>MossTankLootProfileStore.Create</c>; the copied
    /// salvage-combine settings revert to defaults.
    /// </summary>
    [Fact]
    public void LootCommandsImportAndExportExactVtclassicUtlFiles()
    {
        var storage = new MemoryStorage();
        var legacy = new VtankLootProfile
        {
            Rules =
            [
                new LootRule
                {
                    Name = "Pyreal",
                    Action = LootAction.KeepUpTo,
                    KeepCount = 100,
                    Priority = 7,
                    VtankRequirements =
                    [
                        new VtankLootRequirement
                        {
                            Type = 1,
                            Payload = "^Pyreal$\r\n1\r\n",
                        },
                    ],
                },
            ],
            SalvageCombine = new VtankSalvageCombineSettings
            {
                DefaultCombineString = "1-5, 6-10",
                MaterialCombineStrings = new Dictionary<int, string>
                {
                    [61] = "1-10",
                },
                MaterialValueModeValues = new Dictionary<int, int>
                {
                    [61] = 75_000,
                },
            },
        };
        storage.Text["imports/Legacy.utl"] =
            VtankLootProfileSerializer.Write(legacy);
        var panel = new MossTankPanel(new FakeHost(
            new FakeAutomation(),
            storage));

        Command(panel, "loot load Legacy.utl");
        Command(panel, "loot save Copy");

        Assert.Equal("Copy", panel.LootProfileName);
        Assert.Single(panel.LootRuleRows);
        Assert.Contains("KeepUpTo", panel.LootRuleRows[0], StringComparison.Ordinal);
        string exported = storage.Text["mosstank/loot/Copy.utl"];
        Assert.True(VtankLootProfileSerializer.TryRead(
            exported,
            out VtankLootProfile roundTrip,
            out string error), error);
        Assert.Equal("1-5, 6-10", roundTrip.SalvageCombine.DefaultCombineString);
        Assert.Equal(75_000, roundTrip.SalvageCombine.MaterialValueModeValues[61]);
        Assert.Equal(1, Assert.Single(roundTrip.Rules).VtankRequirements[0].Type);
    }

    [Fact]
    public void MacroSettingsRoundTripPreservesImportedLootConditions()
    {
        var host = new FakeHost(new FakeAutomation(), new MemoryStorage());
        var store = new MossTankProfileStore(host);
        // A profile belongs to a character; the store writes nothing until it
        // knows whose it is.
        store.BindCharacter("Prover");
        var settings = new VtankSettingsProfileSerializer.AllSettings
        {
            Combat = new(), Buffs = new(), Vitals = new(), Inventory = new(), Navigation = new(),
        };
        settings.Inventory.Loot.Rules.Add(new LootRule
        {
            Name = "Only pyreals", Action = LootAction.Keep,
            CustomExpression = "preserve me",
            VtankRequirements = [new() { Type = 1, Payload = "^Pyreal$\r\n1\r\n" }],
        });
        store.SaveCurrent(settings, new HashSet<string>(), new HashSet<string>());
        settings.Inventory.Loot.Rules.Clear();
        store.LoadCurrent(settings, new HashSet<string>(), new HashSet<string>());
        LootRule restored = Assert.Single(settings.Inventory.Loot.Rules);
        Assert.Equal("^Pyreal$\r\n1\r\n", Assert.Single(restored.VtankRequirements).Payload);
        Assert.Equal("preserve me", restored.CustomExpression);
    }

    [Fact]
    public void NamedProfileCopyHotLoadsWithoutMixingByCharacterSettings()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Moss Wart" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        panel.SetNormalHealth(0.42f);
        Vt(panel, "settings save Fellowship");

        const string fellowshipFile = "--Moss Wart__Fellowship.usd";
        Assert.Equal(fellowshipFile, panel.SelectedMacroProfile);
        Assert.Contains(fellowshipFile, panel.MacroProfileNames);
        panel.SetNormalHealth(0.88f);
        panel.SelectMacroProfile(MossTankProfileStore.ByCharacter);

        Assert.Equal(0.42f, panel.NormalHealthValue, precision: 2);
        panel.SelectMacroProfile("Fellowship");
        Assert.Equal(fellowshipFile, panel.SelectedMacroProfile);
        Assert.Equal(0.88f, panel.NormalHealthValue, precision: 2);
    }

    [Fact]
    public void DeleteProfileRemovesTheRealFileAndFallsBackToByCharacter()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Moss Wart" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        panel.SetNormalHealth(0.42f);
        Vt(panel, "settings save Fellowship");
        const string fellowshipFile = "--Moss Wart__Fellowship.usd";
        Assert.Equal(fellowshipFile, panel.SelectedMacroProfile);
        Assert.Contains(fellowshipFile, panel.MacroProfileNames);

        panel.DeleteProfile();

        Assert.Equal(MossTankProfileStore.ByCharacter, panel.SelectedMacroProfile);
        Assert.DoesNotContain(fellowshipFile, panel.MacroProfileNames);
        Assert.False(storage.Text.ContainsKey(fellowshipFile));
        Assert.Contains("Deleted", panel.ProfileLifecycleNotice, StringComparison.Ordinal);
    }

    [Fact]
    public void DeleteProfileRefusesToRemoveByCharacter()
    {
        var storage = new MemoryStorage();
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));

        panel.DeleteProfile();

        Assert.Equal(MossTankProfileStore.ByCharacter, panel.SelectedMacroProfile);
        Assert.Contains("cannot be deleted", panel.ProfileLifecycleNotice, StringComparison.Ordinal);
    }

    [Fact]
    public void DeleteRouteProfileRemovesTheRealFileAndFallsBackToByCharacter()
    {
        var storage = new MemoryStorage();
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));
        Vt(panel, "nav save Fellowship");
        Assert.Equal("Fellowship", panel.SelectedRouteProfile);
        Assert.Contains("Fellowship", panel.RouteProfileNames);
        Assert.True(storage.Text.ContainsKey("mosstank/navs/Fellowship.af"));

        panel.DeleteRouteProfile();

        Assert.Equal(MossTankRouteProfileStore.ByCharacter, panel.SelectedRouteProfile);
        Assert.DoesNotContain("Fellowship", panel.RouteProfileNames);
        Assert.False(storage.Text.ContainsKey("mosstank/navs/Fellowship.af"));
        Assert.Contains("Deleted", panel.RouteNotice, StringComparison.Ordinal);
    }

    /// <summary>
    /// The live loss this pins: a once-through route was walked to its end,
    /// picked again in the Route tab, and came back empty, because the run had
    /// taken the points out of the very list the profile is written from and
    /// the next save wrote that empty list over the author's file.
    ///
    /// So: the file keeps its three points while the route runs, still has
    /// them once the route has finished, and picking the profile again — which
    /// saves before it loads — gives the whole route back, on its first point.
    ///
    /// Mutation: put the point removal back in the once branch of the waypoint
    /// advance and the saved file drops to one point and then to none.
    /// </summary>
    [Fact]
    public void RunningAOnceRouteLeavesItsProfileOnDiskWhole()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            CurrentHealth = 100,
            MaxHealth = 100,
        };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        panel.SelectRouteMode("Once");
        // Three points where the character already stands, so every host tick
        // is an arrival and the route runs itself out.
        panel.AddRoutePoint();
        panel.AddRoutePoint();
        panel.AddRoutePoint();
        Vt(panel, "nav save OnceRun");
        const string file = "mosstank/navs/OnceRun.af";
        Assert.Equal(3, PointLines(storage, file));

        panel.SetMetaOption("EnableNav", Truthy(true));
        panel.ToggleCombat();
        panel.OnTick(0.3d);

        // Mid-run, any save at all — here the ordinary door toggle — must
        // still write the whole route.
        Assert.Equal(1, panel.RouteWaypointIndexForTest);
        panel.ToggleOpenDoors();
        Assert.Equal(3, PointLines(storage, file));

        for (int pass = 0; pass < 6; pass++)
            panel.OnTick(0.3d);

        Assert.Equal("Once route complete.", panel.NavigationStatus);
        Assert.Equal(3, panel.RouteRows.Count);
        Assert.Equal(3, PointLines(storage, file));

        panel.SelectRouteProfile("OnceRun");

        Assert.Equal(3, panel.RouteRows.Count);
        Assert.Equal(0, panel.RouteWaypointIndexForTest);
        Assert.Equal(3, PointLines(storage, file));
    }

    /// <summary>
    /// A once route paints the points it has already run red, straight away —
    /// before this the rows only went red once the route was reloaded, which
    /// is exactly when the cursor resets and they go back to normal again.
    /// Reloading the profile puts every row back to the ordinary colour.
    ///
    /// Mutation: colouring off anything but the cursor — a constant, or the
    /// selected row — turns this red.
    /// </summary>
    [Fact]
    public void AOnceRoutePaintsTheWaypointsItHasRunAndAReloadClearsThem()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            CurrentHealth = 100,
            MaxHealth = 100,
        };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        panel.SelectRouteMode("Once");
        panel.AddRoutePoint();
        panel.AddRoutePoint();
        panel.AddRoutePoint();
        Vt(panel, "nav save OnceColours");

        Assert.All(panel.RouteWaypointColors, static color => Assert.Equal(0xE8DEC3u, color));

        panel.SetMetaOption("EnableNav", Truthy(true));
        panel.ToggleCombat();
        panel.OnTick(0.3d);

        // One point behind the cursor: that row, and only that row, is spent.
        Assert.Equal(1, panel.RouteWaypointIndexForTest);
        Assert.Equal(
            [0xC03838u, 0xE8DEC3u, 0xE8DEC3u],
            panel.RouteWaypointColors);

        panel.OnTick(0.3d);
        panel.OnTick(0.3d);

        Assert.Equal(3, panel.RouteWaypointIndexForTest);
        Assert.Equal(
            [0xC03838u, 0xC03838u, 0xC03838u],
            panel.RouteWaypointColors);

        panel.SelectRouteProfile("OnceColours");

        Assert.Equal(
            [0xE8DEC3u, 0xE8DEC3u, 0xE8DEC3u],
            panel.RouteWaypointColors);
    }

    /// <summary>
    /// A circular route spends nothing — it comes back around — so no row of
    /// one ever goes red, however far the cursor has moved.
    /// </summary>
    [Fact]
    public void ACircularRouteNeverPaintsAWaypointAsSpent()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            CurrentHealth = 100,
            MaxHealth = 100,
        };
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));
        panel.SelectRouteMode("Circular");
        panel.AddRoutePoint();
        panel.AddRoutePoint();
        panel.AddRoutePoint();

        panel.SetMetaOption("EnableNav", Truthy(true));
        panel.ToggleCombat();
        for (int pass = 0; pass < 4; pass++)
            panel.OnTick(0.3d);

        Assert.All(panel.RouteWaypointColors, static color => Assert.Equal(0xE8DEC3u, color));
    }

    /// <summary>The number of plain route points a saved .af file holds.</summary>
    private static int PointLines(MemoryStorage storage, string key) =>
        (storage.ReadText(key) ?? string.Empty)
            .Split('\n')
            .Count(line => line.TrimStart().StartsWith("pnt", StringComparison.Ordinal));

    [Fact]
    public void DeleteRouteProfileRefusesToRemoveByCharacter()
    {
        var storage = new MemoryStorage();
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));

        panel.DeleteRouteProfile();

        Assert.Equal(MossTankRouteProfileStore.ByCharacter, panel.SelectedRouteProfile);
        Assert.Contains("cannot be deleted", panel.RouteNotice, StringComparison.Ordinal);
    }

    [Fact]
    public void DeleteMetaProfileRemovesTheRealFileAndFallsBackToByCharacter()
    {
        var storage = new MemoryStorage();
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));
        Vt(panel, "meta save Fellowship");
        Assert.Equal("Fellowship", panel.SelectedMetaProfile);
        Assert.Contains("Fellowship", panel.MetaProfileNames);
        Assert.True(storage.Text.ContainsKey("mosstank/metas/Fellowship.af"));

        panel.DeleteMetaProfile();

        Assert.Equal(MossTankMetaProfileStore.ByCharacter, panel.SelectedMetaProfile);
        Assert.DoesNotContain("Fellowship", panel.MetaProfileNames);
        Assert.False(storage.Text.ContainsKey("mosstank/metas/Fellowship.af"));
        Assert.Contains("Deleted", panel.MetaNotice, StringComparison.Ordinal);
    }

    [Fact]
    public void DeleteMetaProfileRefusesToRemoveByCharacter()
    {
        var storage = new MemoryStorage();
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));

        panel.DeleteMetaProfile();

        Assert.Equal(MossTankMetaProfileStore.ByCharacter, panel.SelectedMetaProfile);
        Assert.Contains("cannot be deleted", panel.MetaNotice, StringComparison.Ordinal);
    }

    [Fact]
    public void DeleteLootProfileRemovesTheRealFileAndFallsBackToByCharacter()
    {
        var storage = new MemoryStorage();
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));
        Vt(panel, "loot new Fellowship");
        Assert.Equal("Fellowship", panel.LootProfileName);
        Assert.Contains("Fellowship", panel.LootProfileNames);

        panel.DeleteLootProfile();

        Assert.Equal(MossTankLootProfileStore.ByCharacter, panel.LootProfileName);
        Assert.DoesNotContain("Fellowship", panel.LootProfileNames);
        Assert.False(storage.Text.ContainsKey("mosstank/loot/Fellowship.utl"));
        Assert.Contains("Deleted", panel.LootEditorNotice, StringComparison.Ordinal);
    }

    /// <summary>
    /// Deleting a valid named profile starts a fresh fallback activation. A
    /// malformed By-char destination leaves no active rules and is preserved.
    /// Mutation <c>PrematureByCharacterFallbackOnDelete</c>: mark By-char
    /// active before <c>LoadLootProfile</c>; the selected-name assertion fails.
    /// </summary>
    [Fact]
    public void DeleteNamedLootProfileDoesNotActivateOrOverwriteMalformedByCharacterFallback()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation
        {
            Name = "Looter",
            WorldName = "Coldeve",
        };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        string byCharacterFile = LootKey(
            VtankProfileDirectory.AutoCharacterFileName("Looter", "Coldeve", "utl"));
        const string malformed = "UTL\r\n1\r\n1\r\nunfinished";
        storage.Text[byCharacterFile] = malformed;
        Command(panel, "loot new Named");
        panel.AddLootRule();

        panel.DeleteLootProfile();
        panel.ToggleLootPriorityBoost();

        Assert.Equal(MossTankLootProfileStore.NoActiveProfile, panel.LootProfileName);
        Assert.Empty(panel.LootRuleRows);
        Assert.Equal(malformed, storage.Text[byCharacterFile]);
        Assert.Contains("could not be read", panel.LootEditorNotice, StringComparison.Ordinal);
    }

    [Fact]
    public void DeleteLootProfileRefusesToRemoveByCharacter()
    {
        var storage = new MemoryStorage();
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));

        panel.DeleteLootProfile();

        Assert.Equal(MossTankLootProfileStore.ByCharacter, panel.LootProfileName);
        Assert.Contains("cannot be deleted", panel.LootEditorNotice, StringComparison.Ordinal);
    }

    [Fact]
    public void ShowNavLinesDrawsTheLoadedRouteAndPersistsWithTheProfile()
    {
        var storage = new MemoryStorage();
        storage.Text["imports/Legacy.nav"] = """
            uTank2 NAV 1.2
            4
            1
            0
            12.5
            -3.25
            0
            0
            """;
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var lines = new RecordingWorldLines();
        var panel = new MossTankPanel(new FakeHost(
            automation, storage, worldLines: lines));
        Command(panel, "nav load Legacy.nav");
        panel.OnTick(0.3d);

        // Off by default: the plugin asks the host for nothing.
        Assert.False(panel.ShowNavLinesEnabled);
        Assert.Empty(lines.Layers);

        panel.ToggleShowNavLines();
        Assert.True(panel.ShowNavLinesEnabled);
        RecordingWorldLines.Layer layer = Assert.Single(lines.Layers);
        // The one point is drawn as its arrival ring.
        Assert.Equal(24, layer.Lines.Count);
        Assert.Contains(
            storage.Text,
            pair => pair.Key.StartsWith("profiles/macro/sidecar/", StringComparison.Ordinal)
                && pair.Value.Contains("\"ShowNavLines\": true", StringComparison.Ordinal));

        panel.ToggleShowNavLines();
        Assert.False(panel.ShowNavLinesEnabled);
        Assert.Empty(layer.Lines);
    }

    /// <summary>
    /// The weapon in the character's hands is added to the Items list the
    /// same way anything else is, and it is still there after a restart.
    /// </summary>
    [Fact]
    public void AWieldedWeaponAddedToTheItemsListSurvivesARestart()
    {
        var storage = new MemoryStorage();
        PluginInventoryItem blade = Wielded(Item(10, "Decapitator's Blade", 1));
        var first = new FakeAutomation
        {
            Name = "Prover",
            ItemEntries = [blade],
        };
        first.Unassessed.Add(10u);
        var firstHost = new FakeHost(first, storage);
        var firstPanel = new MossTankPanel(firstHost);
        firstHost.Selection.Select(10);
        firstPanel.AddSelectedItem();
        for (int tick = 0; tick < 20
            && !firstPanel.ItemRows.Contains("Decapitator's Blade"); tick++)
        {
            firstPanel.OnTick(0.3d);
        }

        Assert.Contains("Decapitator's Blade", firstPanel.ItemRows);

        var second = new FakeAutomation
        {
            Name = "Prover",
            ItemEntries = [blade],
        };
        var panel = new MossTankPanel(new FakeHost(second, storage));

        Assert.Contains("Decapitator's Blade", panel.ItemRows);
    }

    /// <summary>
    /// Dropping the row for an object the character no longer carries leaves
    /// every other row of the list where it was, on disk as well as on the
    /// page.
    /// </summary>
    [Fact]
    public void RemovingARowForAMissingObjectKeepsTheRestOfTheList()
    {
        var storage = new MemoryStorage();
        PluginInventoryItem blade = Wielded(Item(10, "Decapitator's Blade", 1));
        PluginInventoryItem wand = Item(11, "War Wand", 0x8000);
        var automation = new FakeAutomation
        {
            Name = "Prover",
            ItemEntries = [blade, wand],
        };
        var host = new FakeHost(automation, storage);
        var panel = new MossTankPanel(host);
        host.Selection.Select(10);
        panel.AddSelectedItem();
        host.Selection.Select(11);
        panel.AddSelectedItem();
        Assert.Contains("Decapitator's Blade", panel.ItemRows);
        Assert.Contains("War Wand", panel.ItemRows);

        // The wand is given away: its row is still listed, by the object id
        // nobody can resolve any more.
        automation.ItemEntries = [blade];
        panel.CycleItemHandsAt(0);
        panel.CycleItemHandsAt(0);
        int invalid = panel.ItemRows
            .ToList()
            .FindIndex(row => row.StartsWith("<INVALID", StringComparison.Ordinal));
        Assert.True(invalid >= 0, "expected an unresolved row");

        panel.DeleteItemRowAt(invalid);

        Assert.Contains("Decapitator's Blade", panel.ItemRows);
        Assert.DoesNotContain(
            panel.ItemRows,
            row => row.StartsWith("<INVALID", StringComparison.Ordinal));

        var reloaded = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Prover", ItemEntries = [blade] },
            storage));
        Assert.Contains("Decapitator's Blade", reloaded.ItemRows);
    }

    /// <summary>
    /// "Added" is a promise that the item is still there next session, so it
    /// is only said once the profile is on disk. Before a character is
    /// named there is nowhere real to write -- the profile file is named
    /// after the character, and a file written under a blank name is one
    /// the character never reads -- so the add is reported as unsaved and
    /// nothing is written. Mutation: report the add before saving, or let
    /// the store write a nameless profile, and the add reads as a success
    /// that vanishes at the next restart.
    /// </summary>
    [Fact]
    public void AnAddThatReachedNoProfileIsReportedAsUnsavedNotAsAdded()
    {
        var storage = new MemoryStorage();
        PluginInventoryItem blade = Wielded(Item(10, "Decapitator's Blade", 1));
        var automation = new FakeAutomation
        {
            Name = string.Empty,
            ItemEntries = [blade],
        };
        var host = new FakeHost(automation, storage);
        var panel = new MossTankPanel(host);
        host.Selection.Select(10);

        panel.AddSelectedItem();

        Assert.Contains("nothing was saved", panel.ProfileNotice, StringComparison.Ordinal);
        Assert.DoesNotContain(
            storage.Text,
            pair => pair.Value.Contains("Decapitator", StringComparison.Ordinal));

        // Named, and the same add is written and reported as an add.
        automation.Name = "Prover";
        panel.OnTick(0.3d);
        host.Selection.Select(10);
        panel.AddSelectedItem();

        Assert.Equal("Added Decapitator's Blade.", panel.ProfileNotice);
        Assert.Contains("Decapitator's Blade", panel.ItemRows);
        Assert.Contains(
            storage.Text,
            pair => pair.Value.Contains("Decapitator", StringComparison.Ordinal));
    }

    private static PluginInventoryItem Wielded(PluginInventoryItem item) =>
        item with { EquippedLocation = item.ValidLocations, WielderObjectId = 1u };

    [Fact]
    public void AProfileItemLoadedWithTheCharacterIsAssessedWithoutBeingReAdded()
    {
        var storage = new MemoryStorage();
        var first = new FakeAutomation
        {
            Name = "Prover",
            ItemEntries = [Item(10, "Fire Sword", 1)],
        };
        var firstHost = new FakeHost(first, storage);
        var firstPanel = new MossTankPanel(firstHost);
        firstHost.Selection.Select(10);
        firstPanel.AddSelectedItem();
        Assert.Contains("Fire Sword", firstPanel.ItemRows);

        // A fresh session: the same character, the same sword, not yet
        // assessed by this client. The profile names it; nobody re-adds it.
        var automation = new FakeAutomation
        {
            Name = "Prover",
            ItemEntries = [Item(10, "Fire Sword", 1)],
        };
        automation.Unassessed.Add(10u);
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Assert.Contains("Fire Sword", panel.ItemRows);

        for (int tick = 0; tick < 20; tick++)
            panel.OnTick(0.3d);

        Assert.Contains(10u, automation.Identified);
        Assert.Empty(automation.Unassessed);
    }

    [Fact]
    public void AnItemTheClientRefusesToAssessDoesNotStarveTheOthers()
    {
        var storage = new MemoryStorage();
        var first = new FakeAutomation
        {
            Name = "Prover",
            ItemEntries = [Item(10, "Fire Sword", 1), Item(11, "Ice Wand", 0x8000)],
        };
        var firstHost = new FakeHost(first, storage);
        var firstPanel = new MossTankPanel(firstHost);
        firstHost.Selection.Select(10);
        firstPanel.AddSelectedItem();
        firstHost.Selection.Select(11);
        firstPanel.AddSelectedItem();

        // A fresh session in which the client refuses the first item's
        // appraisal outright, every time.
        var automation = new FakeAutomation
        {
            Name = "Prover",
            ItemEntries = [Item(10, "Fire Sword", 1), Item(11, "Ice Wand", 0x8000)],
        };
        automation.Unassessed.Add(10u);
        automation.Unassessed.Add(11u);
        automation.IdentifyRefusals.Add(10u);
        var panel = new MossTankPanel(new FakeHost(automation, storage));

        for (int tick = 0; tick < 20; tick++)
            panel.OnTick(0.3d);

        Assert.Contains(11u, automation.Identified);
        Assert.Equal([10u], automation.Unassessed.Order());
    }

    [Fact]
    public void AnItemTheChannelIsBusyOnDoesNotStarveTheOthersAndIsComeBackTo()
    {
        var storage = new MemoryStorage();
        var seed = new FakeAutomation
        {
            Name = "Prover",
            ItemEntries = [Item(10, "Fire Sword", 1), Item(11, "Ice Wand", 0x8000)],
        };
        var seedHost = new FakeHost(seed, storage);
        var seedPanel = new MossTankPanel(seedHost);
        seedHost.Selection.Select(10);
        seedPanel.AddSelectedItem();
        seedHost.Selection.Select(11);
        seedPanel.AddSelectedItem();

        // The description channel is held by somebody else's question about
        // the first item — the shape a corpse identification in flight puts
        // this sweep in.
        var automation = new FakeAutomation
        {
            Name = "Prover",
            ItemEntries = [Item(10, "Fire Sword", 1), Item(11, "Ice Wand", 0x8000)],
        };
        automation.Unassessed.Add(10u);
        automation.Unassessed.Add(11u);
        automation.IdentifyBusy.Add(10u);
        var panel = new MossTankPanel(new FakeHost(automation, storage));

        for (int tick = 0; tick < 20; tick++)
            panel.OnTick(0.3d);

        // The second item is served meanwhile, and the busy one is still
        // being asked about rather than written off.
        Assert.Contains(11u, automation.Identified);
        Assert.DoesNotContain(11u, automation.Unassessed);
        Assert.True(automation.Identified.Count(id => id == 10u) > 1);

        automation.IdentifyBusy.Clear();
        for (int tick = 0; tick < 20; tick++)
            panel.OnTick(0.3d);

        Assert.Empty(automation.Unassessed);
    }

    [Fact]
    public void NavCommandsImportLegacyAndExportAf()
    {
        var storage = new MemoryStorage();
        storage.Text["imports/Legacy.nav"] = """
            uTank2 NAV 1.2
            4
            1
            0
            12.5
            -3.25
            0
            0
            """;
        var panel = new MossTankPanel(new FakeHost(
            new FakeAutomation(),
            storage));

        Command(panel, "nav load Legacy.nav");

        Assert.Equal("Legacy", panel.SelectedRouteProfile);
        Assert.Single(panel.RouteRows);
        Assert.Contains("12.5", panel.RouteRows[0], StringComparison.Ordinal);

        Command(panel, "nav save Exported.nav");
        Assert.Contains(
            "NAV: ",
            storage.Text["mosstank/navs/Exported.af"],
            StringComparison.Ordinal);
    }

    [Fact]
    public void NavSaveAcceptsAnAfSuffixedNameWithoutDoublingIt()
    {
        var storage = new MemoryStorage();
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));

        Command(panel, "nav save Foo.af");

        Assert.True(storage.Text.ContainsKey("mosstank/navs/Foo.af"));
        Assert.False(storage.Text.ContainsKey("mosstank/navs/Foo.af.af"));
    }

    [Fact]
    public void MetaAndRouteProfilesWithTheSameNameDoNotCollide()
    {
        var storage = new MemoryStorage();
        storage.Text["imports/Same.met"] =
            "1\r\nCondAct\r\n5\r\nCType\r\nAType\r\nCData\r\nAData\r\nState\r\n"
            + "n\r\nn\r\nn\r\nn\r\nn\r\n1\r\n"
            + "i\r\n1\r\ni\r\n2\r\ni\r\n0\r\ns\r\n/say imported\r\n"
            + "s\r\nDefault\r\n";
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));

        Command(panel, "meta load Same.met");
        Command(panel, "meta save Same.met");
        Command(panel, "nav save Same.nav");

        Assert.True(storage.Text.ContainsKey("mosstank/metas/Same.af"));
        Assert.True(storage.Text.ContainsKey("mosstank/navs/Same.af"));
        Assert.Contains(
            "STATE: ",
            storage.Text["mosstank/metas/Same.af"],
            StringComparison.Ordinal);
        Assert.Contains(
            "NAV: ",
            storage.Text["mosstank/navs/Same.af"],
            StringComparison.Ordinal);
    }

    // Renamed from
    // MetaCommandsImportAndExportExactVtankMetFiles for the same reason as
    // the nav test above.
    [Fact]
    public void MetaCommandsImportLegacyAndExportAf()
    {
        var storage = new MemoryStorage();
        storage.Text["imports/Legacy.met"] =
            "1\r\nCondAct\r\n5\r\nCType\r\nAType\r\nCData\r\nAData\r\nState\r\n"
            + "n\r\nn\r\nn\r\nn\r\nn\r\n1\r\n"
            + "i\r\n1\r\ni\r\n2\r\ni\r\n0\r\ns\r\n/say imported\r\n"
            + "s\r\nDefault\r\n";
        var panel = new MossTankPanel(new FakeHost(
            new FakeAutomation(),
            storage));

        Command(panel, "meta load Legacy.met");

        Assert.Equal("Legacy", panel.SelectedMetaProfile);
        Assert.Single(panel.MetaRows);
        Assert.Contains("/say imported", panel.MetaRows[0], StringComparison.Ordinal);

        Command(panel, "meta save Exported.met");
        Assert.Contains(
            "STATE: ",
            storage.Text["mosstank/metas/Exported.af"],
            StringComparison.Ordinal);
        Assert.True(MetafSerializer.TryLoadMeta(
            storage.Text["mosstank/metas/Exported.af"],
            NoOpSpellCatalogForExport.Instance,
            out MetaProfile exported,
            out string error), error);
        Assert.Single(exported.Rules);
    }

    [Fact]
    public void MetaSaveAcceptsAnAfSuffixedNameWithoutDoublingIt()
    {
        var storage = new MemoryStorage();
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));

        Command(panel, "meta save Foo.af");

        Assert.True(storage.Text.ContainsKey("mosstank/metas/Foo.af"));
        Assert.False(storage.Text.ContainsKey("mosstank/metas/Foo.af.af"));
    }

    private sealed class NoOpSpellCatalogForExport : ISpellCatalog
    {
        public static NoOpSpellCatalogForExport Instance { get; } = new();
        public IReadOnlyList<PluginSpellInfo> KnownSelfBuffs => [];
        public bool TryGet(uint spellId, out PluginSpellInfo info)
        {
            info = default;
            return false;
        }
    }

    [Fact]
    public void ByCharacterProfilesAreIsolatedByCharacterName()
    {
        var storage = new MemoryStorage();
        var first = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "One" },
            storage));
        first.SetNormalHealth(0.33f);

        var second = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Two" },
            storage));

        Assert.Equal(0.75f, second.NormalHealthValue, precision: 2);
    }

    [Fact]
    public void MonstersGridMutationsPersistAcrossSessions()
    {
        var storage = new MemoryStorage();
        var first = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Rule Maker" },
            storage));

        first.SetMonsterExpressionDraft("species==drudge");
        first.AddMonsterRule(); // row 1: "species==drudge"
        first.ToggleMonsterImperilAt(1);
        first.CycleMonsterDamageAt(1); // Auto -> Void Basic (MonsterDamageCycle order)
        first.CycleMonsterPriorityAt(1); // a new row starts at 1, so 1 -> 2

        Assert.Equal(["DEFAULT", "species==drudge"], first.MonsterNameColumn);
        Assert.True(first.MonsterImperilColumn[1]);
        Assert.Equal("Void Basic", first.MonsterDamageColumn[1]);
        Assert.Equal("2", first.MonsterPriorityColumn[1]);
        Assert.Equal(string.Empty, first.MonsterExpressionDraft);

        var second = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Rule Maker" },
            storage));

        Assert.Equal(
            [MonsterRule.AuthenticDefaultName, "species==drudge"],
            second.MonsterNameColumn);
        Assert.True(second.MonsterImperilColumn[1]);
        Assert.Equal("Void Basic", second.MonsterDamageColumn[1]);
        Assert.Equal("2", second.MonsterPriorityColumn[1]);
    }

    [Fact]
    public void ToggleMonsterFlagAtWritesOnlyTheTargetedRow()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        panel.SetMonsterExpressionDraft("drudge");
        panel.AddMonsterRule(); // row 1: "drudge"

        Assert.False(panel.MonsterImperilColumn[0]);
        Assert.False(panel.MonsterImperilColumn[1]);

        panel.ToggleMonsterImperilAt(1);

        Assert.False(panel.MonsterImperilColumn[0]);
        Assert.True(panel.MonsterImperilColumn[1]);

        panel.ToggleMonsterImperilAt(1);
        Assert.False(panel.MonsterImperilColumn[1]);
    }

    [Fact]
    public void CycleMonsterPriorityAtWrapsExactlyNegativeOneThroughFour()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        Assert.Equal("1", panel.MonsterPriorityColumn[0]); // a fresh row starts at 1

        foreach (string next in new[] { "2", "3", "4", "-1", "0", "1" })
        {
            panel.CycleMonsterPriorityAt(0);
            Assert.Equal(next, panel.MonsterPriorityColumn[0]);
        }
    }

    [Fact]
    public void MonsterDamageColumnsCycleInTheExactAuthenticOrder()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        AssertCyclesExactly(
            () => panel.MonsterDamageColumn[0],
            () => panel.CycleMonsterDamageAt(0),
            [
                "Pierce", "Bludgeon", "Slash", "Acid", "Lightning", "Cold",
                "Fire", "Harm", "Auto", "Void Basic", "Drain Auto",
                "Prismatic", "Random", "Fists",
            ]);
        AssertCyclesExactly(
            () => panel.MonsterExtraVulnColumn[0],
            () => panel.CycleMonsterExtraVulnerabilityAt(0),
            ["Pierce", "Bludgeon", "Slash", "Acid", "Lightning", "Cold", "Fire", "Auto", "None"]);
        AssertCyclesExactly(
            () => panel.MonsterPetDamageColumn[0],
            () => panel.CycleMonsterPetDamageAt(0),
            [
                "Pierce", "Bludgeon", "Slash", "Acid", "Lightning", "Cold",
                "Fire", "PAuto", "Auto", "None",
            ]);

        static void AssertCyclesExactly(
            Func<string> readCurrent, Action cycle, string[] expected)
        {
            int guard = 0;
            while (readCurrent() != expected[0])
            {
                cycle();
                Assert.True(++guard <= expected.Length, "cycle never reached the array's first entry");
            }
            foreach (string next in expected.Skip(1).Append(expected[0]))
            {
                cycle();
                Assert.Equal(next, readCurrent());
            }
        }
    }

    /// <summary>
    /// The DEFAULT row's extra vulnerability is editable from the grid and is
    /// written with the profile, so a profile that carries one is one the
    /// owner can also clear.
    /// Mutation: have <c>CycleMonsterExtraVulnerabilityAt</c> skip row 0, or
    /// drop the column from the profile writer, and this goes red.
    /// </summary>
    [Fact]
    public void TheDefaultRowsExtraVulnerabilityIsEditableAndSurvivesAReload()
    {
        var storage = new MemoryStorage();
        var first = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Vuln Setter" }, storage));

        Assert.Equal("DEFAULT", first.MonsterNameColumn[0]);
        Assert.Equal("None", first.MonsterExtraVulnColumn[0]);
        int guard = 0;
        while (first.MonsterExtraVulnColumn[0] != "Auto")
        {
            first.CycleMonsterExtraVulnerabilityAt(0);
            Assert.True(++guard <= 12, "the cycle never reached Auto");
        }

        var second = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Vuln Setter" }, storage));
        Assert.Equal("Auto", second.MonsterExtraVulnColumn[0]);

        guard = 0;
        while (second.MonsterExtraVulnColumn[0] != "None")
        {
            second.CycleMonsterExtraVulnerabilityAt(0);
            Assert.True(++guard <= 12, "the cycle never reached None");
        }

        var third = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Vuln Setter" }, storage));
        Assert.Equal("None", third.MonsterExtraVulnColumn[0]);
    }

    [Fact]
    public void DeleteMonsterRuleAtRemovesNonDefaultRowsButNeverDefault()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        panel.SetMonsterExpressionDraft("drudge");
        panel.AddMonsterRule();
        panel.SetMonsterExpressionDraft("mosswart");
        panel.AddMonsterRule();
        Assert.Equal(["DEFAULT", "drudge", "mosswart"], panel.MonsterNameColumn);

        panel.DeleteMonsterRuleAt(0); // DEFAULT: refused
        Assert.Equal(["DEFAULT", "drudge", "mosswart"], panel.MonsterNameColumn);

        panel.DeleteMonsterRuleAt(1); // drudge
        Assert.Equal(["DEFAULT", "mosswart"], panel.MonsterNameColumn);
    }

    [Fact]
    public void VtankRefreshCommandRepopulatesTheMonstersGridAfterCreatingTheDefaultRule()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        Type panelType = typeof(MossTankPanel);
        var combatSettings = (CombatSettings)panelType
            .GetField("_combatSettings", BindingFlags.Instance | BindingFlags.NonPublic)!
            .GetValue(panel)!;
        combatSettings.Rules.Clear();
        FieldInfo monsterNameColumnField = panelType
            .GetField("_monsterNameColumn", BindingFlags.Instance | BindingFlags.NonPublic)!;
        monsterNameColumnField.SetValue(panel, new[] { "STALE" });

        panel.ExecuteVtankCommand(new PluginCommand("vt", "refresh", "/vt refresh"));

        Assert.Equal(["DEFAULT"], panel.MonsterNameColumn);
    }

    [Fact]
    public void MoveMonsterRuleAtReordersButNeverDisplacesDefault()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        panel.SetMonsterExpressionDraft("drudge");
        panel.AddMonsterRule();
        panel.SetMonsterExpressionDraft("mosswart");
        panel.AddMonsterRule();
        Assert.Equal(["DEFAULT", "drudge", "mosswart"], panel.MonsterNameColumn);

        panel.MoveMonsterRuleDownAt(1); // drudge <-> mosswart
        Assert.Equal(["DEFAULT", "mosswart", "drudge"], panel.MonsterNameColumn);

        panel.MoveMonsterRuleUpAt(2); // back (row 2 "drudge" up to row 1)
        Assert.Equal(["DEFAULT", "drudge", "mosswart"], panel.MonsterNameColumn);

        panel.MoveMonsterRuleUpAt(1); // would displace DEFAULT from row 0: refused
        Assert.Equal(["DEFAULT", "drudge", "mosswart"], panel.MonsterNameColumn);

        Assert.Equal(3, panel.MonsterMoveUpIcons.Count);
        Assert.All(panel.MonsterMoveUpIcons, id => Assert.Equal(0x060028FCu, id));
        Assert.All(panel.MonsterMoveDownIcons, id => Assert.Equal(0x060028FDu, id));
    }

    /// <summary>
    /// Both the DEFAULT row a fresh profile is born with and a row the player
    /// adds afterwards start as the same fresh row: priority one, attack and
    /// streak ticked, nothing else. A row that started at priority zero with
    /// no streak sorted below every authored rule and fired plain bolts where
    /// its neighbours finished with a streak.
    /// Mutation: start a new row at priority zero, or without the streak tick,
    /// and this fails on that column for both rows.
    /// </summary>
    [Fact]
    public void EveryNewMonsterRowStartsAtPriorityOneWithAttackAndStreak()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        panel.SetMonsterExpressionDraft("species==drudge");
        panel.AddMonsterRule();

        Assert.Equal(["DEFAULT", "species==drudge"], panel.MonsterNameColumn);
        for (int row = 0; row < 2; row++)
        {
            Assert.Equal("1", panel.MonsterPriorityColumn[row]);
            Assert.True(panel.MonsterAttackColumn[row]);
            Assert.True(panel.MonsterStreakColumn[row]);
            Assert.False(panel.MonsterRingColumn[row]);
            Assert.False(panel.MonsterImperilColumn[row]);
            Assert.False(panel.MonsterVulnerabilityColumn[row]);
        }
    }

    [Fact]
    public void AddMonsterRuleUsesTheDraftTextAndAddSelectedMonsterUsesTheWorldTarget()
    {
        var automation = new CombatCapableFakeAutomation
        {
            Targets = [new PluginCombatTarget(30, "Drudge", 700, 2f, 0f, true, 1f)],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        panel.SetMonsterExpressionDraft("species==drudge");
        panel.AddMonsterRule();
        Assert.Equal(["DEFAULT", "species==drudge"], panel.MonsterNameColumn);
        Assert.Equal(string.Empty, panel.MonsterExpressionDraft);

        host.Selection.Select(30);
        panel.AddSelectedMonster();
        Assert.Equal(["DEFAULT", "species==drudge", "Drudge"], panel.MonsterNameColumn);
    }

    /// <summary>
    /// Add with an empty expression box takes the selected creature's name.
    /// Mutation: give the empty box a placeholder caption instead, and the
    /// row is named after the placeholder, which matches no creature.
    /// </summary>
    [Fact]
    public void AddWithAnEmptyExpressionTakesTheSelectedCreaturesName()
    {
        var automation = new CombatCapableFakeAutomation
        {
            Targets = [new PluginCombatTarget(30, "Drudge Slinker", 700, 2f, 0f, true, 1f)],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        host.Selection.Select(30);
        panel.AddMonsterRule();

        Assert.Equal(["DEFAULT", "Drudge Slinker"], panel.MonsterNameColumn);
        Assert.Contains("Drudge Slinker", panel.MonsterEditorNotice, StringComparison.Ordinal);
    }

    /// <summary>
    /// With nothing selected and nothing typed, Add says so and adds no row.
    /// Mutation: add the row anyway and the name column grows.
    /// </summary>
    [Fact]
    public void AddWithNoExpressionAndNoSelectionAddsNothingAndSaysSo()
    {
        var panel = new MossTankPanel(new FakeHost(new CombatCapableFakeAutomation()));

        panel.AddMonsterRule();

        Assert.Equal(["DEFAULT"], panel.MonsterNameColumn);
        Assert.Equal("Select a monster in the world first.", panel.MonsterEditorNotice);
    }

    /// <summary>
    /// A creature that is not hostile to us right now is not in the combat
    /// scan, but its name is in the object table, and that name is all the
    /// rule matches on.
    /// Mutation: drop the object-table fallback and the row is never added.
    /// </summary>
    [Fact]
    public void AddSelectedMonsterFallsBackToTheObjectTableForANonHostileCreature()
    {
        var automation = new FakeAutomation
        {
            WorldObjects =
            [
                new PluginWorldObject(
                    42u, 0u, "Sclavus", PluginObjectClass.Monster, 0u, 0u, 0u),
            ],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        host.Selection.Select(42);
        panel.AddSelectedMonster();

        Assert.Equal(["DEFAULT", "Sclavus"], panel.MonsterNameColumn);
    }

    [Fact]
    public void MonsterWeaponColumnCyclesTheRegisteredRosterAndPersistsByNameAcrossSessions()
    {
        var storage = new MemoryStorage();
        var firstAutomation = new FakeAutomation
        {
            Name = "Rule Maker",
            ItemEntries = [Item(10, "Fire Sword", 1)],
        };
        var firstHost = new FakeHost(firstAutomation, storage);
        var first = new MossTankPanel(firstHost);
        firstHost.Selection.Select(10);
        first.AddSelectedItem(); // registers "Fire Sword" into the weapon roster

        Assert.Equal("<AUTO>", first.MonsterWeaponColumn[0]);
        first.CycleMonsterWeaponAt(0); // AUTO -> Fire Sword (only registered item)
        Assert.Equal("Fire Sword", first.MonsterWeaponColumn[0]);
        first.CycleMonsterWeaponAt(0); // Fire Sword -> AUTO (wraps)
        Assert.Equal("<AUTO>", first.MonsterWeaponColumn[0]);
        first.CycleMonsterWeaponAt(0);
        Assert.Equal("Fire Sword", first.MonsterWeaponColumn[0]);

        var second = new MossTankPanel(new FakeHost(
            new FakeAutomation
            {
                Name = "Rule Maker",
                ItemEntries = [Item(99, "Fire Sword", 1)],
            },
            storage));

        Assert.Equal("Fire Sword", second.MonsterWeaponColumn[0]);
    }

    /// <summary>
    /// A Weapon cell holding a pick the Items page no longer carries reads
    /// INVALID, in the same words the Items page uses for one of its own rows
    /// that has gone missing. Showing the item's ordinary name would promise a
    /// weapon the fight will never wield.
    /// Mutation: print the resolved item name whatever the Items page holds,
    /// and the cell reads "Unlisted Blade".
    /// </summary>
    [Fact]
    public void AMonsterWeaponCellNamingAnUnlistedItemReadsInvalid()
    {
        var automation = new FakeAutomation
        {
            Name = "Rule Maker",
            ItemEntries =
            [
                Item(0x800013FAu, "Blade", 1),
                Item(0x800013FBu, "Sword", 1),
            ],
        };
        var host = new FakeHost(automation, new MemoryStorage());
        var panel = new MossTankPanel(host);
        host.Selection.Select(0x800013FAu);
        panel.AddSelectedItem();
        host.Selection.Select(0x800013FBu);
        panel.AddSelectedItem();

        // The rule picks the blade while the Items page still carries it.
        int guard = 0;
        while (panel.MonsterWeaponColumn[0] != "Blade")
        {
            panel.CycleMonsterWeaponAt(0);
            Assert.True(++guard <= 6, "the weapon cycle never reached Blade");
        }

        // The blade then leaves the Items page, and the rule is left holding
        // a pick nothing will honour.
        panel.SelectItemRow(
            panel.ItemNameColumn
                .Select(static (name, index) => (name, index))
                .First(static row => row.name == "Blade").index);
        panel.RemoveSelectedItem();

        Assert.Equal("<INVALID 0x800013FA>", panel.MonsterWeaponColumn[0]);
    }

    [Fact]
    public void MonsterGridColumnsDoNotScanLiveInventoryOrAllocateOnEveryRead()
    {
        var automation = new FakeAutomation
        {
            Name = "Perf Check",
            ItemEntries = [Item(10, "Fire Sword", 1)],
        };
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        host.Selection.Select(10);
        panel.AddSelectedItem();
        panel.CycleMonsterWeaponAt(0);

        int callsAfterCycle = automation.CaptureOwnedItemsCallCount;
        IReadOnlyList<string> weaponFirstRead = panel.MonsterWeaponColumn;
        for (int i = 0; i < 5; i++)
        {
            _ = panel.MonsterFesterColumn;
            _ = panel.MonsterNameColumn;
            _ = panel.MonsterWeaponColumn;
            _ = panel.MonsterOffhandColumn;
            _ = panel.MonsterMoveUpIcons;
        }

        Assert.Equal(callsAfterCycle, automation.CaptureOwnedItemsCallCount);
        Assert.Same(weaponFirstRead, panel.MonsterWeaponColumn);
    }

    /// <summary>
    /// A route recorded into a route profile with client pathing checked is loaded back with
    /// it, and while the macro runs its legs are walked by the client's navigation, one walk to
    /// each waypoint in turn, instead of being steered at.
    /// </summary>
    private static (double NorthSouth, double EastWest, double Elevation) MapPoint(PluginNavigationPosition position) =>
        (Math.Round(position.NorthSouth, 5), Math.Round(position.EastWest, 5), Math.Round(position.Elevation, 5));

    /// <summary>
    /// Records a three-point route into the route profile "hunt", with client pathing checked
    /// or not, the way a player does from the Route tab, and returns where its points stand.
    private static void Vt(MossTankPanel panel, string arguments) =>
        panel.ExecuteVtankCommand(new PluginCommand("vt", arguments, "/vt " + arguments));

    /// </summary>
    private static PluginNavigationPosition[] RecordRoute(MemoryStorage storage, bool clientPathing)
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        panel.SetRouteProfileNameDraft("hunt");
        panel.CreateRouteProfile();
        var points = new PluginNavigationPosition[3];
        for (int index = 0; index < points.Length; index++)
        {
            points[index] = automation.NavigationSnapshot.Position with
            {
                NorthSouth = (index + 1) * 20d / 240d,
            };
            automation.NavigationSnapshot = automation.NavigationSnapshot with { Position = points[index] };
            panel.AddRoutePoint();
        }
        return points;
    }

    [Fact]
    public void RouteGridDeletesByAnyCellClickAndSelectsNearestByDistance()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.AddRoutePoint(); // waypoint 0 at EastWest=0
        automation.NavigationSnapshot = automation.NavigationSnapshot with
        {
            Position = automation.NavigationSnapshot.Position with { EastWest = 50d },
        };
        panel.AddRoutePoint(); // waypoint 1 at EastWest=50
        Assert.Equal(2, panel.RouteWaypointTextColumn.Count);
        Assert.Equal(["1", "2"], panel.RouteWaypointCountColumn);

        automation.NavigationSnapshot = automation.NavigationSnapshot with
        {
            Position = automation.NavigationSnapshot.Position with { EastWest = 48d },
        };
        panel.SelectNearestRouteWaypoint();
        Assert.Equal(1, panel.SelectedRouteWaypointIndex);

        panel.DeleteRouteWaypointAt(0);
        Assert.Single(panel.RouteWaypointTextColumn);
        Assert.Equal(["1"], panel.RouteWaypointCountColumn);
    }

    /// <summary>
    /// The rule pass arms the route mover; it does not drive it. A mover that
    /// only steers on the pass has a control period several times its own
    /// heading tolerance, so it overshoots every turn and hunts around the
    /// bearing instead of settling on it.
    /// </summary>
    [Fact]
    public void TheRouteMoverSteersOnHostFramesBetweenSchedulerPasses()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.AddRoutePoint();
        automation.NavigationSnapshot = automation.NavigationSnapshot with
        {
            // Twelve metres west of the waypoint, so the mover has a real leg.
            Position = automation.NavigationSnapshot.Position with { EastWest = 0.05d },
        };
        panel.ToggleNavigation();
        Assert.True(panel.NavigationEnabled);
        panel.ToggleCombat();

        // The first frame carries the pass that arms the mover.
        panel.OnTick(0.05d);
        Assert.NotEmpty(automation.MovementIntents);
        int afterArming = automation.MovementIntents.Count;

        // Four more frames, none of which carries a pass: the scheduler's
        // heartbeat is 0.293 s and only 0.2 s of it has gone by.
        for (int frame = 0; frame < 4; frame++)
            panel.OnTick(0.05d);

        Assert.Equal(afterArming + 4, automation.MovementIntents.Count);
    }

    [Fact]
    public void RouteInsertModeControlsWhereANewWaypointLands()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation));
        Assert.Equal("Add to End", panel.SelectedRouteInsertMode);

        automation.NavigationSnapshot = automation.NavigationSnapshot with
        { Position = automation.NavigationSnapshot.Position with { EastWest = 0d } };
        panel.AddRoutePoint(); // index 0 @ 0
        automation.NavigationSnapshot = automation.NavigationSnapshot with
        { Position = automation.NavigationSnapshot.Position with { EastWest = 20d } };
        panel.AddRoutePoint(); // index 1 @ 20 (default Add to End)
        Assert.Equal(2, panel.RouteWaypointTextColumn.Count);
        Assert.Contains("20", panel.RouteWaypointTextColumn[1]);

        panel.SelectRouteInsertMode("Insert Above");
        Assert.Equal("Insert Above", panel.SelectedRouteInsertMode);
        panel.SelectRouteWaypoint(0); // the @0 waypoint
        automation.NavigationSnapshot = automation.NavigationSnapshot with
        { Position = automation.NavigationSnapshot.Position with { EastWest = 99d } };
        panel.AddRoutePoint(); // must land BEFORE index 0, not appended
        Assert.Equal(3, panel.RouteWaypointTextColumn.Count);
        Assert.Contains("99", panel.RouteWaypointTextColumn[0]);

        panel.SelectRouteInsertMode("Insert Below");
        Assert.Equal("Insert Below", panel.SelectedRouteInsertMode);
        panel.SelectRouteWaypoint(0); // the @99 waypoint
        automation.NavigationSnapshot = automation.NavigationSnapshot with
        { Position = automation.NavigationSnapshot.Position with { EastWest = 77d } };
        panel.AddRoutePoint(); // must land right AFTER index 0
        Assert.Equal(4, panel.RouteWaypointTextColumn.Count);
        Assert.Contains("77", panel.RouteWaypointTextColumn[1]);
    }

    [Fact]
    public void RouteRecallComboShowsVtanksTerseCaptionsButTheWaypointKeepsTheFullName()
    {
        var automation = new FakeAutomation { NavigationSnapshot = NavigationAt(0f) };
        var panel = new MossTankPanel(new FakeHost(automation));

        Assert.Equal(
            [
                "Primary", "Secondary", "LS", "LS Sending", "Portal", "Aphus",
                "Sanctuary", "Caul", "GW", "Aerlinthe", "Mt. Lethe", "Ulgrim's",
                "Bur", "PtOIA", "Graveyard", "Colosseum", "Fac. Hub",
                "Gear K. Camp", "Neftet", "Candeth", "Rynthid", "VR Rocks",
                "VR Tree", "Soc. CH", "Soc. RB", "Soc. EW", "Marketplace",
            ],
            panel.RouteRecallNames);

        Assert.Equal("Primary", panel.SelectedRouteRecall);

        panel.SelectRouteRecall("LS");
        Assert.Equal("LS", panel.SelectedRouteRecall);

        panel.AddRouteRecall();
        Assert.Contains("Lifestone Recall", panel.RouteNotice, StringComparison.Ordinal);
        Assert.Contains("Lifestone Recall", Assert.Single(panel.RouteWaypointTextColumn));
    }

    [Fact]
    public void RoutePauseSecondsFieldParsesAndClampsInput()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        Assert.Equal("5", panel.RoutePauseSecondsFieldText);

        panel.SetRoutePauseSecondsText("12");
        Assert.Equal("12", panel.RoutePauseSecondsFieldText);

        panel.SetRoutePauseSecondsText("99999");
        Assert.Equal("3600", panel.RoutePauseSecondsFieldText);

        panel.SetRoutePauseSecondsText("not a number");
        Assert.Equal("3600", panel.RoutePauseSecondsFieldText); // unchanged on bad input
    }

    /// <summary>
    /// Follow turns the loaded route into a follow route aimed at the selected
    /// character, with no waypoints needed, and says who is followed.
    /// Mutation: leave the route mode alone and the mode menu still reads
    /// Circular, so the route walks its own (empty) waypoint list instead.
    /// </summary>
    [Fact]
    public void FollowTurnsTheLoadedRouteIntoAFollowRouteOnTheSelectedCharacter()
    {
        var automation = new FakeAutomation();
        automation.NavigationObjects.Add(new PluginNavigationObject(
            77u, "Horan", default));
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);
        panel.ToggleNavigation(); // navigation on, as a follower would have it
        Assert.Equal("Circular", panel.SelectedRouteMode);
        Assert.Empty(panel.RouteWaypointTextColumn);

        host.Selection.Select(77u);
        panel.SetFollowTarget();

        Assert.Equal("Follow", panel.SelectedRouteMode);
        Assert.Equal("Follow target: Horan", panel.RouteFollowTargetText);
        Assert.Equal("Following Horan.", panel.RouteNotice);
        Assert.Empty(panel.RouteWaypointTextColumn);
    }

    /// <summary>
    /// A follow route set up while navigation is switched off says so, instead
    /// of leaving a button that looks as if it did nothing.
    /// </summary>
    [Fact]
    public void FollowSaysWhenNavigationIsStillSwitchedOff()
    {
        var automation = new FakeAutomation();
        automation.NavigationObjects.Add(new PluginNavigationObject(
            77u, "Horan", default));
        var host = new FakeHost(automation);
        var panel = new MossTankPanel(host);

        host.Selection.Select(77u);
        panel.SetFollowTarget();

        Assert.Contains("Enable Navigation", panel.RouteNotice, StringComparison.Ordinal);
    }

    /// <summary>
    /// With nothing selected, Follow says so and leaves the route alone.
    /// </summary>
    [Fact]
    public void FollowWithNothingSelectedLeavesTheRouteAlone()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        panel.SetFollowTarget();

        Assert.Equal("Circular", panel.SelectedRouteMode);
        Assert.Equal("Follow target: [None]", panel.RouteFollowTargetText);
        Assert.Equal("Select a live object to follow first.", panel.RouteNotice);
    }

    /// <summary>
    /// A follow route survives the round trip through the route profile the
    /// window saves it to, target and all.
    /// </summary>
    [Fact]
    public void AFollowRouteIsSavedAndReloadedWithItsTarget()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Follower" };
        automation.NavigationObjects.Add(new PluginNavigationObject(
            77u, "Horan", default));
        var host = new FakeHost(automation, storage);
        var panel = new MossTankPanel(host);

        host.Selection.Select(77u);
        panel.SetFollowTarget();

        var reloaded = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Follower" }, storage));

        Assert.Equal("Follow", reloaded.SelectedRouteMode);
        Assert.Equal("Follow target: Horan", reloaded.RouteFollowTargetText);
    }

    [Fact]
    public void MetaTabEditsAndExecutesTheLiveStateMachine()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        panel.ShowMeta();
        Assert.True(panel.MetaVisible);
        panel.SetMetaStateDraft(MetaEngine.DefaultState);
        panel.SelectMetaCondition(nameof(MetaConditionKind.Always));
        panel.SelectMetaAction(nameof(MetaActionKind.SetMetaState));
        panel.SetMetaActionTextDraft("Hunt");
        panel.AddMetaRule();

        Assert.Single(panel.MetaRows);
        Assert.Contains("Hunt", panel.MetaRows[0], StringComparison.Ordinal);
        panel.ToggleMeta();
        panel.ToggleCombat();
        panel.OnTick(0.3);

        Assert.True(panel.MetaEnabled);
        Assert.Equal("Hunt", panel.MetaState);
    }

    [Fact]
    public void MetaGridColumnsMatchTheRulesAndDeleteMoveActOnTheClickedRow()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        panel.SetMetaStateDraft("Idle");
        panel.SelectMetaAction(nameof(MetaActionKind.ChatCommand));
        panel.SetMetaActionTextDraft("/mt one");
        panel.AddMetaRule();
        panel.SetMetaStateDraft("Hunt");
        panel.SetMetaActionTextDraft("/mt two");
        panel.AddMetaRule();

        Assert.Equal(["Idle", "Hunt"], panel.MetaStateColumn);
        Assert.Contains("/mt one", panel.MetaActionColumn[0], StringComparison.Ordinal);
        Assert.Contains("/mt two", panel.MetaActionColumn[1], StringComparison.Ordinal);
        Assert.Equal(["X", "X"], panel.MetaDeleteColumn);
        Assert.All(panel.MetaMoveUpIcons, id => Assert.Equal(0x060028FCu, id));
        Assert.All(panel.MetaMoveDownIcons, id => Assert.Equal(0x060028FDu, id));

        panel.MoveMetaRuleDownAt(0); // "Idle" <-> "Hunt"
        Assert.Equal(["Hunt", "Idle"], panel.MetaStateColumn);
        panel.MoveMetaRuleUpAt(1); // back
        Assert.Equal(["Idle", "Hunt"], panel.MetaStateColumn);

        panel.DeleteMetaRuleAt(0); // "Idle"
        Assert.Equal(["Hunt"], panel.MetaStateColumn);
    }

    [Fact]
    public void MetaAndRouteGridColumnsDoNotReallocateOnEveryRead()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
        }));
        panel.SelectMetaAction(nameof(MetaActionKind.ChatCommand));
        panel.SetMetaActionTextDraft("/mt one");
        panel.AddMetaRule();
        panel.AddRoutePoint();

        Assert.Same(panel.MetaStateColumn, panel.MetaStateColumn);
        Assert.Same(panel.MetaConditionColumn, panel.MetaConditionColumn);
        Assert.Same(panel.MetaActionColumn, panel.MetaActionColumn);
        Assert.Same(panel.MetaDeleteColumn, panel.MetaDeleteColumn);
        Assert.Same(panel.MetaMoveUpIcons, panel.MetaMoveUpIcons);
        Assert.Same(panel.MetaMoveDownIcons, panel.MetaMoveDownIcons);
        Assert.Same(panel.RouteWaypointCountColumn, panel.RouteWaypointCountColumn);
    }

    [Fact]
    public void MetaEditorPopupOpensOnCellClickOrCreateAndClosesOnApplyOrCancel()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        Assert.False(panel.MetaEditorVisible);

        panel.CreateMetaRule();
        Assert.True(panel.MetaEditorVisible);
        Assert.Single(panel.MetaRows);

        panel.HideMetaEditor();
        Assert.False(panel.MetaEditorVisible);

        panel.SelectMetaRule(0);
        Assert.True(panel.MetaEditorVisible);

        panel.ApplyMetaRule();
        Assert.False(panel.MetaEditorVisible);

        panel.SelectMetaRule(0);
        Assert.True(panel.MetaEditorVisible);
        panel.HideMetaEditor();
        Assert.False(panel.MetaEditorVisible);

        panel.DeleteMetaRuleAt(0);
        Assert.False(panel.MetaEditorVisible);
    }

    [Fact]
    public void SwitchingTabsClosesTheBuffPickerAndMetaEditorPopups()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        panel.ShowExtraBuffPicker();
        Assert.True(panel.BuffPickerVisible);
        panel.ShowOptions(); // switch away from Buffs
        Assert.False(panel.BuffPickerVisible);

        panel.ShowMeta();
        panel.CreateMetaRule();
        Assert.True(panel.MetaEditorVisible);
        panel.ShowOptions(); // switch away from Meta
        Assert.False(panel.MetaEditorVisible);
    }

    [Fact]
    public void MetaCurrentStateMenuForcesTheLiveEngineIntoTheChosenState()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        panel.SetMetaStateDraft(MetaEngine.DefaultState);
        panel.SelectMetaAction(nameof(MetaActionKind.ChatCommand));
        panel.SetMetaActionTextDraft("/mt hi");
        panel.AddMetaRule();
        panel.SetMetaStateDraft("Hunt");
        panel.AddMetaRule();

        Assert.Contains("Hunt", panel.MetaCurrentStateNames);
        Assert.Equal(MetaEngine.DefaultState, panel.SelectedMetaCurrentState);

        panel.SetMetaCurrentState("Hunt");

        Assert.Equal("Hunt", panel.SelectedMetaCurrentState);
        Assert.Equal("Hunt", panel.MetaState);
    }

    [Fact]
    public void MetaProfilesAreIndependentDurableDocuments()
    {
        var storage = new MemoryStorage();
        var first = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Meta Maker" },
            storage));
        first.SelectMetaAction(nameof(MetaActionKind.ChatCommand));
        first.SetMetaActionTextDraft("/mt status");
        first.AddMetaRule();
        Vt(first, "meta save Hunting");

        Assert.Equal("Hunting", first.SelectedMetaProfile);
        Assert.Single(first.MetaRows);
        first.SelectMetaProfile(MossTankMetaProfileStore.ByCharacter);
        first.AddMetaRule();
        Assert.Equal(2, first.MetaRows.Count);

        var second = new MossTankPanel(new FakeHost(
            new FakeAutomation { Name = "Meta Maker" },
            storage));
        Assert.Equal(MossTankMetaProfileStore.ByCharacter, second.SelectedMetaProfile);
        Assert.Equal(2, second.MetaRows.Count);
        second.SelectMetaProfile("Hunting");
        Assert.Single(second.MetaRows);
        Assert.Contains("/mt status", second.MetaRows[0], StringComparison.Ordinal);
    }

    [Fact]
    public void UtilityBeltVtankExpressionsControlTheSameLiveOwners()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        panel.SelectMetaCondition(nameof(MetaConditionKind.Expression));
        panel.SetMetaConditionTextDraft("vtmacroenabled[] == 1");
        panel.SelectMetaAction(nameof(MetaActionKind.ExpressionAction));
        panel.SetMetaActionTextDraft(
            "vtsetsetting[`MonsterRange`,42] + vtsetmetastate[`Expression State`]");
        panel.AddMetaRule();
        panel.ToggleMeta();
        panel.ToggleCombat();

        panel.OnTick(0.3);

        Assert.Equal("Expression State", panel.MetaState);
        Assert.Equal("Maximum target range: 42m", panel.AttackRangeText);
        Assert.True(panel.EvaluateExpression("uboptset[`MonsterRange`,33]").IsTruthy);
        Assert.Equal(33d, panel.EvaluateExpression(
            "uboptget[`MonsterRange`]").AsNumber());
    }

    [Fact]
    public void RunMacroAndEnableCombatAreIndependentVtankStates()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));

        Assert.Equal("Run Macro", panel.CombatButtonText);
        Assert.True(panel.CombatEnabled);

        panel.ToggleCombat();
        panel.ToggleCombatEnabled();
        panel.OnTick(0d);

        Assert.Equal("Stop Macro", panel.CombatButtonText);
        Assert.False(panel.CombatEnabled);
        Assert.Equal("Combat disabled", panel.CombatStatus);
        Assert.True(panel.EvaluateExpression("vtmacroenabled[]").IsTruthy);
        Assert.False(panel.EvaluateExpression("uboptget[`EnableCombat`]").IsTruthy);
    }

    [Fact]
    public void SameCharacterReconnectClearsOnlySessionStateAndCanRestartCleanly()
    {
        var automation = new FakeAutomation { Name = "Relogger" };
        var panel = new MossTankPanel(new FakeHost(automation));
        panel.ToggleMeta();
        panel.ToggleCombat();
        panel.EvaluateExpression(
            "$session=11;@persistent=22;&global=33;"
            + "delayexec[60000,`$late=1`]");
        panel.EvaluateExpression("vtsetmetastate[`Hunt`]");

        automation.IsAvailable = false;
        panel.OnTick(0.1d);

        Assert.Equal("Run Macro", panel.CombatButtonText);
        Assert.False(panel.MetaEnabled);
        Assert.Equal(MetaEngine.DefaultState, panel.MetaState);
        Assert.Equal(0d, panel.EvaluateExpression("$session").AsNumber());
        Assert.Equal(22d, panel.EvaluateExpression("@persistent").AsNumber());
        Assert.Equal(33d, panel.EvaluateExpression("&global").AsNumber());
        Assert.Equal("Lost the session.", panel.BuffStatus);

        automation.IsAvailable = true;
        panel.OnTick(0.1d);
        panel.OnTick(61d);

        Assert.Equal(0d, panel.EvaluateExpression("$late").AsNumber());
        Assert.Equal("Idle.", panel.BuffStatus);
        panel.ToggleCombat();
        Assert.Equal("Stop Macro", panel.CombatButtonText);
    }

    [Fact]
    public void TheIdleBandRefillsACombatPetOnTheIdleThresholdNotTheNormalOne()
    {
        var automation = new FakeAutomation
        {
            ItemEntries =
            [
                Item(10, "Cold Rift", 0, petClass: 49387) with
                {
                    Structure = 3,
                    MaximumStructure = 50,
                },
                Item(11, "Encapsulated Spirit", 0) with
                {
                    WeenieClassId = PetDeviceCatalog.EncapsulatedSpiritWeenieClassId,
                },
            ],
        };
        var host = new FakeHost(automation);
        automation.CurrentSelection = () => host.Selection.SelectedObjectId ?? 0u;
        var panel = new MossTankPanel(host);
        host.Selection.Select(10u);
        panel.AddSelectedItem();
        Command(panel, "opt set petrefillcount-normal 0");
        Command(panel, "opt set petrefillcount-idle 3");
        panel.ToggleCombat();

        IMacroRule rule = ((IMacroRuleProvider)panel)
            .Create(MacroRuleSlot.RefillPetChargesIdle);

        // Three charges left is under the idle threshold and over the normal
        // one: only a rule reading its own setting stops for this.
        Assert.True(rule.ValidNow(new MacroPassContext(0.3d, true)));

        Command(panel, "opt set petrefillcount-idle 0");

        Assert.False(rule.ValidNow(new MacroPassContext(2d, true)));
    }

    [Fact]
    public void AProfileWhoseEnableMetaIsTrueLoadsWithTheMetaEngineRunning()
    {
        var storage = new MemoryStorage();
        storage.Text[MetaProfileKey] = ProfileTextWithEnableMeta(true);

        var panel = new MossTankPanel(
            new FakeHost(new FakeAutomation { Name = "Metaphile" }, storage));

        Assert.True(panel.MetaEnabled);
        Assert.True(panel.EvaluateExpression("uboptget[`EnableMeta`]").IsTruthy);
    }

    [Fact]
    public void TheMetaCheckboxIsStoredInTheProfileAndOutlivesReloadAndReconnect()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Metaphile" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        Assert.False(panel.MetaEnabled);

        panel.ToggleMeta();

        Assert.True(panel.MetaEnabled);
        Assert.True(StoredEnableMeta(storage.Text[MetaProfileKey]));

        automation.IsAvailable = false;
        panel.OnTick(0.1d);
        automation.IsAvailable = true;
        panel.OnTick(0.1d);

        Assert.True(panel.MetaEnabled);

        var reloaded = new MossTankPanel(
            new FakeHost(new FakeAutomation { Name = "Metaphile" }, storage));

        Assert.True(reloaded.MetaEnabled);
    }

    private static string MetaProfileKey => SettingsKey(
        VtankProfileDirectory.AutoCharacterFileName("Metaphile", string.Empty, "usd"));

    private static string ProfileTextWithEnableMeta(bool value)
    {
        VtankDatabase database = VtankDefaultSettingsDatabase.Create();
        VtankTable settings = database.Find("Settings")!;
        int nameColumn = settings.ColumnIndex("Setting");
        int valueColumn = settings.ColumnIndex("Value");
        VtankRow row = settings.Rows.First(candidate =>
            candidate.Cells[nameColumn].AsString().Equals(
                "EnableMeta", StringComparison.OrdinalIgnoreCase));
        row.Cells[valueColumn] = VtankCell.Bool(value);
        return database.Render();
    }

    private static bool StoredEnableMeta(string profileText)
    {
        VtankTable settings = VtankDatabase.Parse(profileText).Find("Settings")!;
        int nameColumn = settings.ColumnIndex("Setting");
        int valueColumn = settings.ColumnIndex("Value");
        return settings.Rows
            .First(candidate => candidate.Cells[nameColumn].AsString().Equals(
                "EnableMeta", StringComparison.OrdinalIgnoreCase))
            .Cells[valueColumn]
            .AsBool();
    }

    [Fact]
    public void OfficialVtankOptionDefaultsAndDynamicOverridesAreDurable()
    {
        var storage = new MemoryStorage();
        var first = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));

        // One fewer than the shipped settings file names: the one setting no
        // rule reads is not an option here.
        Assert.Equal(136, VtankOptionCatalog.Names.Length);
        Assert.Equal(10d, first.EvaluateExpression(
            "uboptget[`ArrowheadFletchDiffExcessThreshold`]").AsNumber());
        Assert.Equal(0.0833333333333333d, first.EvaluateExpression(
            "uboptget[`DoorIDRange`]").AsNumber(), precision: 14);
        Assert.True(first.EvaluateExpression(
            "uboptget[`ManaChargesWhenOff`]").IsTruthy);
        Assert.Equal(4d, first.EvaluateExpression(
            "uboptget[`SpellCompMin-Critical`]").AsNumber());
        Assert.Equal(20d, first.EvaluateExpression(
            "uboptget[`SpellCompMin-Normal`]").AsNumber());
        Assert.Equal(20d, first.EvaluateExpression(
            "uboptget[`SpellCompMin-Idle`]").AsNumber());
        Assert.Equal(2d, first.EvaluateExpression(
            "uboptget[`IdleCraftCount_HealthKits`]").AsNumber());
        Assert.Equal(15d, first.EvaluateExpression(
            "uboptget[`IdleCraftCount_ManaFood`]").AsNumber());
        Assert.True(first.EvaluateExpression(
            "uboptset[`ArrowheadFletchDiffExcessThreshold`,22]").IsTruthy);
        Assert.True(first.EvaluateExpression(
            "uboptset[`IdleCraftCount_HealthKits`,7]").IsTruthy);

        var second = new MossTankPanel(new FakeHost(new FakeAutomation(), storage));
        Assert.Equal(22d, second.EvaluateExpression(
            "uboptget[`arrowheadfletchdiffexcessthreshold`]").AsNumber());
        Assert.Equal(7d, second.EvaluateExpression(
            "uboptget[`idlecraftcount_healthkits`]").AsNumber());
    }

    [Fact]
    public void VtankSetInAllRewritesEveryKnownMacroProfile()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation, storage));

        Command(panel, "settings save First");
        Command(panel, "opt set AttackDistance 0.01");
        Command(panel, "settings save Second");
        Command(panel, "opt set AttackDistance 0.02");
        Command(panel, "opt setinall AttackDistance 0.03");
        Command(panel, "settings load First");
        Assert.Equal(0.03d, panel.EvaluateExpression(
            "uboptget[`AttackDistance`]").AsNumber(), precision: 7);
        Command(panel, "settings load Second");
        Assert.Equal(0.03d, panel.EvaluateExpression(
            "uboptget[`AttackDistance`]").AsNumber(), precision: 7);

        var reloaded = new MossTankPanel(new FakeHost(
            new FakeAutomation(), storage));
        Command(reloaded, "settings load First");
        Assert.Equal(0.03d, reloaded.EvaluateExpression(
            "uboptget[`AttackDistance`]").AsNumber(), precision: 7);
    }

    [Fact]
    public void VtankSetInAllReportsAuthenticExactMessageText()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "opt setinall AttackDistance 0.03");

        string message = Assert.Single(automation.Messages, static text =>
            text.StartsWith("Done saving setting", StringComparison.Ordinal));
        Assert.StartsWith(
            "Done saving setting AttackDistance to all profiles. (Changed ",
            message,
            StringComparison.Ordinal);
        Assert.EndsWith(" profiles)", message, StringComparison.Ordinal);
        Assert.DoesNotContain("=", message, StringComparison.Ordinal);
    }

    [Theory]
    [InlineData("set")]
    [InlineData("setinall")]
    public void VtankOptSetRejectsAWrongTypedValueWithAuthenticExactText(string operation)
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, $"opt {operation} EnableLooting notaboolean");

        Assert.Equal(
            "Option set: bad value. EnableLooting takes a System.Boolean.",
            Assert.Single(automation.Messages));
    }

    [Fact]
    public void SetOptionInAllAppendsARowWhenAFileHasNoneForThatSetting()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = "Barris" };
        const string minimalUsd =
            "1\r\nSettings\r\n4\r\nSetting\r\nValue\r\nDescription\r\nSettingType\r\n"
            + "y\r\nn\r\nn\r\nn\r\n1\r\ns\r\nEnableNav\r\nb\r\nFalse\r\ns\r\n\r\ni\r\n1\r\n";
        storage.Text["mosstank/profiles/Other.usd"] = minimalUsd;

        var store = new MossTankProfileStore(new FakeHost(automation, storage));
        var settings = new VtankSettingsProfileSerializer.AllSettings
        {
            Combat = new CombatSettings(),
            Buffs = new BuffSettings(),
            Vitals = new VitalSettings(),
            Inventory = new InventorySettings(),
            Navigation = new NavigationSettings(),
        };
        settings.Inventory.Loot.Enabled = true;

        int count = store.SetOptionInAll("EnableLooting", settings);

        VtankDatabase rewritten = VtankDatabase.Parse(storage.Text["mosstank/profiles/Other.usd"]);
        VtankTable table = rewritten.Find("Settings")!;
        int nameColumn = table.ColumnIndex("Setting");
        int valueColumn = table.ColumnIndex("Value");
        VtankRow appended = Assert.Single(table.Rows, row =>
            row.Cells[nameColumn].AsString().Equals(
                "EnableLooting", StringComparison.OrdinalIgnoreCase));
        Assert.True(appended.Cells[valueColumn].AsBool());
        Assert.Equal("0", appended.Cells[2].Tag);
        Assert.Equal("0", appended.Cells[3].Tag);
        Assert.True(count >= 1);
    }

    [Fact]
    public void VtankHelpAndExpressionsUseTheLocalCommandSurface()
    {
        var automation = new FakeAutomation();
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ExecuteVtankCommand(new PluginCommand("vt", "help", "/vt help"));
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt",
            "mexec 1 + 2 * 3",
            "/vt mexec 1 + 2 * 3"));

        // Four macro verb lists, the documented-command list, the pointer at
        // per-command help, then the two the expression wrote.
        Assert.Equal(8, automation.Messages.Count);
        Assert.StartsWith("MossTank /vt — profiles:", automation.Messages[0],
            StringComparison.Ordinal);
        Assert.Equal("MExec evaluating expression: \"1 + 2 * 3\"", automation.Messages[6]);
        Assert.Equal("Result: 7", automation.Messages[7]);
    }

    /// <summary>
    /// The count reaches the player two ways — as a command that writes its
    /// answer, and as expression functions a meta reads the figure from —
    /// and both have to be bound to the same counter to be worth anything.
    /// All three expression functions are read for their figure here, the
    /// profile one included: a registration that answered with a constant
    /// would still appear in the registered list, so the list says nothing.
    /// </summary>
    [Fact]
    public void VtankCountAnswersAsACommandAndAsAnExpression()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            ItemEntries =
            [
                Item(10u, "Prismatic Taper", 0x00001000u),
                Item(11u, "Prismatic Taper", 0x00001000u),
                Item(12u, "Pyreal Mote", 0x00001000u),
            ],
            WorldObjects =
            [
                // 24 m away, so inside a 100 m question.
                new PluginWorldObject(
                    20u, 0u, "Near", PluginObjectClass.Player, 0u, 0u, 0u)
                {
                    IsLandscape = true,
                    HasPosition = true,
                    Position = new PluginNavigationPosition(
                        0x00010001u, 0d, 0.1d, 0d, 0f, IsOutdoor: true),
                },
                // 240 m away, so outside it.
                new PluginWorldObject(
                    21u, 0u, "Far", PluginObjectClass.Player, 0u, 0u, 0u)
                {
                    IsLandscape = true,
                    HasPosition = true,
                    Position = new PluginNavigationPosition(
                        0x00010001u, 0d, 1d, 0d, 0f, IsOutdoor: true),
                },
                // The three carried items, already appraised, so a profile
                // count can decide all three without waiting on anything.
                Carried(10u, "Prismatic Taper"),
                Carried(11u, "Prismatic Taper"),
                Carried(12u, "Pyreal Mote"),
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));
        // A named loot profile for the profile count to answer against.
        panel.AddLootRule();
        panel.SetLootExpressionDraft("name ~= taper");
        panel.ApplyLootRule();
        Vt(panel, "loot save Counted");
        automation.Messages.Clear();

        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "count item taper", "/vt count item taper"));
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "count player 100", "/vt count player 100"));
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt",
            "mexec vtcountitem[taper] + vtcountplayer[100]",
            "/vt mexec vtcountitem[taper] + vtcountplayer[100]"));

        Assert.Equal(
            [
                "Item Count: Prismatic Taper - 2",
                "Total Item Count: 2",
                "Player Count: 1",
            ],
            automation.Messages.Take(3));
        Assert.Equal("Result: 3", automation.Messages[^1]);

        panel.ExecuteVtankCommand(new PluginCommand(
            "vt",
            "mexec vtcountprofile[Counted]",
            "/vt mexec vtcountprofile[Counted]"));

        // Two tapers kept by the rule, and the mote left out by it.
        Assert.Equal("Result: 2", automation.Messages[^1]);
    }

    /// <summary>
    /// A profile count holds the character while it waits on appraisals, so
    /// there has to be a way to take the character back that is not logging
    /// out. Mutation: drop the stop verb, and the command falls through to
    /// the usage line while the count carries on holding the character.
    /// </summary>
    [Fact]
    public void VtankCountStopCancelsAProfileCountThatIsStillWaiting()
    {
        var automation = new FakeAutomation
        {
            ItemEntries = [Item(10u, "Prismatic Taper", 0x00001000u)],
        };
        automation.Unassessed.Add(10u);
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));
        panel.AddLootRule();
        panel.SetLootExpressionDraft("name ~= taper");
        panel.ApplyLootRule();
        Vt(panel, "loot save Counted");

        Vt(panel, "count profile Counted");
        Assert.Contains(
            automation.Messages,
            line => line.StartsWith(
                "Items remaining to identify:",
                StringComparison.Ordinal));

        Vt(panel, "count stop");

        Assert.Contains(
            automation.Messages,
            line => line.Contains(
                "Item counter stopped: Counted",
                StringComparison.Ordinal));
    }

    /// <summary>
    /// The roster the command works on is alphabetical, and the number it
    /// prints beside a character is that alphabetical position — not the slot
    /// the account's list keeps the character in. Here the two disagree for
    /// every character, so a listing that printed the slot as the index, or
    /// an unsorted roster, shows up at once.
    /// </summary>
    [Fact]
    public void VtankLoginListsTheRosterAlphabeticallyBesideItsAccountSlots()
    {
        var automation = new FakeAutomation
        {
            Name = "Mule",
            LoginRoster =
            [
                new PluginLoginCharacter(0x50u, "Zeke", 0, false),
                new PluginLoginCharacter(0x10u, "Mule", 1, false),
                new PluginLoginCharacter(0x30u, "Aardvark", 2, false),
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "login list");

        Assert.Equal("Listing 3 logins.", automation.Messages[0]);
        Assert.Contains("Slot", automation.Messages[1], StringComparison.Ordinal);
        Assert.StartsWith("0 ", Squeeze(automation.Messages[2]), StringComparison.Ordinal);
        Assert.Contains("Aardvark", automation.Messages[2], StringComparison.Ordinal);
        Assert.EndsWith(" 2", Squeeze(automation.Messages[2]), StringComparison.Ordinal);
        // The character the client is on is marked, and its own slot (1) is
        // not its alphabetical index (1 here by coincidence of three names).
        Assert.Contains("**Mule**", automation.Messages[3], StringComparison.Ordinal);
        Assert.StartsWith("2 ", Squeeze(automation.Messages[4]), StringComparison.Ordinal);
        Assert.Contains("Zeke", automation.Messages[4], StringComparison.Ordinal);
        Assert.EndsWith(" 0", Squeeze(automation.Messages[4]), StringComparison.Ordinal);
    }

    /// <summary>
    /// The three ways to name the next character: an alphabetical index, an
    /// index counted from where the character stands now, and part of a name.
    /// The wrap is only taken when it was asked for; without it an index off
    /// the end refuses and clears the pick rather than logging in as someone
    /// unexpected.
    /// </summary>
    [Fact]
    public void VtankLoginNextTakesAnIndexARelativeIndexOrPartOfAName()
    {
        var automation = new FakeAutomation
        {
            Name = "Mule",
            LoginRoster =
            [
                new PluginLoginCharacter(0x50u, "Zeke", 0, false),
                new PluginLoginCharacter(0x10u, "Mule", 1, false),
                new PluginLoginCharacter(0x30u, "Aardvark", 2, false),
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "login next 0");
        Assert.Equal(0x30u, automation.NextLoginObjectId);

        // One before "Mule" alphabetically is "Aardvark".
        Command(panel, "login nextr -1");
        Assert.Equal(0x30u, automation.NextLoginObjectId);

        Command(panel, "login nextr 1");
        Assert.Equal(0x50u, automation.NextLoginObjectId);

        Command(panel, "login next zek");
        Assert.Equal(0x50u, automation.NextLoginObjectId);

        // Off the end without the loop flag: refused, and the pick is dropped.
        Command(panel, "login next 7");
        Assert.Equal(0u, automation.NextLoginObjectId);

        // Past the end with the loop flag wraps around instead: three steps
        // on from "Mule" in a list of three is "Mule" again.
        Command(panel, "login nextrl 3");
        Assert.Equal(0x10u, automation.NextLoginObjectId);

        Command(panel, "login next nobody");
        Assert.Equal(0u, automation.NextLoginObjectId);

        Command(panel, "login next 2");
        Assert.Equal(0x50u, automation.NextLoginObjectId);
        Command(panel, "login clear");
        Assert.Equal(0u, automation.NextLoginObjectId);
    }

    /// <summary>A padded line with its column gaps collapsed to one space.</summary>
    private static string Squeeze(string value) => string.Join(
        ' ',
        value.Split(' ', StringSplitOptions.RemoveEmptyEntries));

    [Fact]
    public void VtankJumpTurnsToTheRequestedHeadingBeforeChargingAndReleasing()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(90f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", "jump 180 false 100", "/vt jump 180 false 100"));
        panel.OnTick(0.05d);

        Assert.True(automation.MovementIntents[^1].TurnRight);
        Assert.False(automation.MovementIntents[^1].Jump);

        automation.NavigationSnapshot = NavigationAt(180f);
        panel.OnTick(0.01d);
        Assert.True(automation.MovementIntents[^1].Jump);

        panel.OnTick(0.1d);
        Assert.False(automation.MovementIntents[^1].Jump);
    }

    /// <summary>
    /// A jump can go backwards, which is the one direction the intent had no
    /// way to ask for. Mutation: leaving the backward case out of the intent
    /// makes the jump stand still, with every direction flag false.
    /// </summary>
    [Fact]
    public void VtankJumpCanGoBackwardsAndARouteJumpRemembersThat()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(180f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "jump 180 false 100 backward");
        panel.OnTick(0.05d);

        Assert.True(automation.MovementIntents[^1].Backward);
        Assert.False(automation.MovementIntents[^1].Forward);
        Assert.True(automation.MovementIntents[^1].Jump);

        Command(panel, "addnavjump 180 false 100 backward");
        Assert.Contains("Backward", panel.RouteRows[^1], StringComparison.Ordinal);
        // Saved route files have no code for a backward jump, and the player
        // is told so at the one moment it can be acted on.
        Assert.Contains(
            automation.Messages,
            static message => message.Contains(
                "backward",
                StringComparison.OrdinalIgnoreCase)
                && message.Contains("forward", StringComparison.OrdinalIgnoreCase));
    }

    /// <summary>
    /// A charge the client never turns into a jump is tried again, and after
    /// the attempt ceiling the command lets the character go rather than
    /// holding it for ever. Mutation: dropping the retry leaves one charge;
    /// dropping the ceiling leaves it charging without end.
    /// </summary>
    [Fact]
    public void VtankJumpRetriesAChargeThatNeverBecameAJumpThenGivesUp()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(180f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "jump 180 false 100");
        panel.OnTick(0.05d);
        Assert.Equal(1, automation.MovementIntents.Count(intent => intent.Jump));

        for (int attempt = 2; attempt <= 3; attempt++)
        {
            ExpireCommandJumpAttempt(panel);
            panel.OnTick(0.05d);
            Assert.Equal(
                attempt,
                automation.MovementIntents.Count(intent => intent.Jump));
        }

        ExpireCommandJumpAttempt(panel);
        Assert.Contains(
            "gave up",
            automation.Messages[^1],
            StringComparison.OrdinalIgnoreCase);

        int intents = automation.MovementIntents.Count;
        panel.OnTick(1d);
        Assert.Equal(intents, automation.MovementIntents.Count);

        // The same clock run through with the client reporting a jump: the
        // charge stands, and nothing is tried a second time.
        var reporting = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(180f),
        };
        var second = new MossTankPanel(new FakeHost(reporting));
        Command(second, "jump 180 false 100");
        second.OnTick(0.05d);
        second.OnTick(0.1d);
        reporting.MoveReport = reporting.MoveReport with { JumpSequence = 1 };
        ExpireCommandJumpAttempt(second);
        second.OnTick(0.05d);
        Assert.Equal(1, reporting.MovementIntents.Count(intent => intent.Jump));
    }

    /// <summary>
    /// Facing a heading is the jump's turn without the jump: it turns, says
    /// so, and stops. Mutation: letting it fall through to the charge puts a
    /// jump intent on the wire.
    /// </summary>
    [Fact]
    public void VtankFaceTurnsToTheHeadingAndStopsWithoutJumping()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(90f),
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "face 180");
        panel.OnTick(0.05d);
        Assert.True(automation.MovementIntents[^1].TurnRight);

        automation.NavigationSnapshot = NavigationAt(180f);
        panel.OnTick(0.05d);
        Assert.DoesNotContain(automation.MovementIntents, intent => intent.Jump);
        Assert.Contains("180", automation.Messages[^1], StringComparison.Ordinal);

        int intents = automation.MovementIntents.Count;
        panel.OnTick(1d);
        Assert.Equal(intents, automation.MovementIntents.Count);
    }

    /// <summary>
    /// The give line carries three things the command has to pull apart: how
    /// the item name is matched, how many units to hand over, and who to hand
    /// them to. Mutation: splitting on the last " to " instead of the first
    /// makes the item name swallow a target whose name contains it.
    /// </summary>
    [Fact]
    public void VtankGiveReadsTheMatchModeTheCountAndTheTarget()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            ItemEntries =
            [
                Item(10u, "Prismatic Taper", 0x00001000u) with { StackSize = 10 },
                Item(11u, "Pyreal Mote", 0x00001000u),
            ],
            WorldObjects =
            [
                new PluginWorldObject(
                    100u, 0u, "Taper Mule", PluginObjectClass.Player, 0u, 0u, 0u)
                {
                    IsLandscape = true,
                    HasPosition = true,
                    Position = NavigationAt(0f).Position,
                },
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "givep 4 prismatic to Taper Mule");
        panel.OnTick(0.1d);
        Assert.Equal([(10u, 100u, 4u)], automation.Gives);

        Command(panel, "give stop");
        automation.Gives.Clear();

        Command(panel, "giver ^pyreal.*$ to Taper Mule");
        panel.OnTick(0.1d);
        Assert.Equal([(11u, 100u, 0u)], automation.Gives);

        Command(panel, "give cancel");
        automation.Gives.Clear();

        // An exact match: the partial name alone matches nothing now.
        Command(panel, "give prismatic to Taper Mule");
        panel.OnTick(0.1d);
        Assert.Empty(automation.Gives);
    }

    /// <summary>
    /// The give flags mean different things in the two letter cases: little
    /// p matches part of the ITEM's name, big P part of the TARGET's. A macro
    /// written against the reference types "/vt giveP Prismatic Taper to Zero"
    /// and means the whole item name and half the mule's.
    /// Mutation: fold the capital onto the small one and the item name is
    /// matched partially while the target has to be named in full, which is
    /// the opposite of what was typed.
    /// </summary>
    [Fact]
    public void ACapitalPOnGiveMatchesPartOfTheTargetsNameAndNotTheItems()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            ItemEntries =
            [
                Item(10u, "Prismatic Taper", 0x00001000u),
                Item(11u, "Prismatic Taper, Sealed", 0x00001000u),
            ],
            WorldObjects =
            [
                new PluginWorldObject(
                    100u, 0u, "Taper Mule", PluginObjectClass.Player, 0u, 0u, 0u)
                {
                    IsLandscape = true,
                    HasPosition = true,
                    Position = NavigationAt(0f).Position,
                },
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        Command(panel, "giveP Prismatic Taper to Taper");
        panel.OnTick(0.1d);

        // Part of the target's name found the mule; the whole item name left
        // the sealed taper behind.
        Assert.Equal([(10u, 100u, 0u)], automation.Gives);
    }

    /// <summary>
    /// The give's own numbers ride in the profile beside every other setting
    /// the reference profile format has no room for. Mutation: leaving the
    /// give range out of the profile read leaves the default 15 m standing and
    /// the hand-over goes ahead.
    /// </summary>
    [Fact]
    public void ProfileLoadCarriesTheGiveRange()
    {
        var storage = new MemoryStorage();
        storage.Text[SettingsKey("Tight.usd")] =
            VtankDefaultSettingsDatabase.Create().Render();
        storage.Text["profiles/macro/sidecar/Tight.usd.json"] =
            """{ "InventoryGiveRangeMeters": 2 }""";
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(0f),
            ItemEntries = [Item(10u, "Prismatic Taper", 0x00001000u)],
            WorldObjects =
            [
                // Five meters off: inside the default range, outside this one.
                new PluginWorldObject(
                    100u, 0u, "Mule", PluginObjectClass.Player, 0u, 0u, 0u)
                {
                    IsLandscape = true,
                    HasPosition = true,
                    Position = new PluginNavigationPosition(
                        0x00010001u, 5d / 240d, 0d, 0d, 0f, IsOutdoor: true),
                },
            ],
        };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        panel.OnTick(0.1d);

        Command(panel, "settings load Tight");
        panel.OnTick(0.1d);
        automation.Messages.Clear();

        Command(panel, "give Prismatic Taper to Mule");
        panel.OnTick(0.1d);

        Assert.Empty(automation.Gives);
        Assert.Contains(
            automation.Messages,
            static message => message.Contains(
                "give range",
                StringComparison.OrdinalIgnoreCase));
    }

    /// <summary>
    /// Jumper.PauseNav holds the route still for as long as the jump is in
    /// the air, and lets it go the moment the jump is done. Without it the
    /// macro's own walking fights the charge for the movement keys.
    /// Mutation: stop arming the navigation slot and the route is free to
    /// walk while the character is charging a jump.
    /// </summary>
    [Fact]
    public void PauseNavHoldsTheRouteStillForAsLongAsTheJumpLasts()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(180f),
        };
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));

        Command(panel, "face 180");
        Assert.True(panel.ActionLocks.IsLocked(ActionLockKind.Navigation));

        // Already pointing the right way, so the turn is done on the first
        // tick and the hold goes with it.
        panel.OnTick(0.05d);
        Assert.False(panel.ActionLocks.IsLocked(ActionLockKind.Navigation));

        Command(panel, "opt set Jumper.PauseNav false");
        Command(panel, "face 180");
        Assert.False(panel.ActionLocks.IsLocked(ActionLockKind.Navigation));
    }

    /// <summary>
    /// Jumper.ThinkComplete says so to yourself when the turn or the jump
    /// lands, which is how a macro watching its own chat learns the command
    /// is done.
    /// Mutation: leave the setting unread and nothing is thought.
    /// </summary>
    [Fact]
    public void ThinkCompleteThinksTurningSuccessAndJumperSuccess()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(180f),
        };
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));
        Command(panel, "opt set Jumper.ThinkComplete true");

        Command(panel, "face 180");
        panel.OnTick(0.05d);
        Assert.Contains("/t Test Character, Turning Success", automation.Submitted);

        Command(panel, "jump 180 false 100");
        panel.OnTick(0.05d);
        panel.OnTick(0.1d);
        automation.MoveReport = automation.MoveReport with { JumpSequence = 1 };
        panel.OnTick(0.5d);

        Assert.Contains("/t Test Character, Jumper Success", automation.Submitted);
    }

    /// <summary>
    /// Jumper.ThinkFail sends the failure down the same road, so a macro can
    /// watch for it instead of reading the plugin's own chat channel. With it
    /// off the line is written as before.
    /// Mutation: leave the setting unread and the give-up is only written.
    /// </summary>
    [Fact]
    public void ThinkFailThinksTheJumpThatNeverHappened()
    {
        var automation = new FakeAutomation
        {
            NavigationSnapshot = NavigationAt(180f),
        };
        var panel = new MossTankPanel(new FakeHost(automation, new MemoryStorage()));
        Command(panel, "opt set Jumper.ThinkFail true");

        Command(panel, "jump 180 false 100");
        panel.OnTick(0.05d);
        for (int attempt = 2; attempt <= 3; attempt++)
        {
            ExpireCommandJumpAttempt(panel);
            panel.OnTick(0.05d);
        }
        ExpireCommandJumpAttempt(panel);

        Assert.Contains(
            automation.Submitted,
            static line => line.StartsWith("/t Test Character, Jump gave up", StringComparison.Ordinal));
    }

    /// <summary>
    /// One command jump's hold, settle and retry grace, ticked through with
    /// the client reporting no jump at all.
    /// </summary>
    private static void ExpireCommandJumpAttempt(MossTankPanel panel)
    {
        panel.OnTick(0.1d);
        panel.OnTick(0.25d);
        panel.OnTick(5d);
    }

    [Fact]
    public void RegistersEveryAuditedUtilityBeltExpressionFunction()
    {
        var panel = new MossTankPanel(new FakeHost(new FakeAutomation()));
        string[] expected =
        [
            "abs",
            "acos",
            "actiontryapplyitem",
            "actiontrycastbyid",
            "actiontrycastbyidontarget",
            "actiontrydrop",
            "actiontryequipanywand",
            "actiontrygiveitem",
            "actiontrygiveprofile",
            "actiontrymove",
            "actiontryselect",
            "actiontrysplit",
            "actiontryuseitem",
            "asin",
            "atan",
            "atan2",
            "ceiling",
            "chatbox",
            "chatboxpaste",
            "chr",
            "clearallgvars",
            "clearallpvars",
            "clearallvars",
            "clearexec",
            "cleargvar",
            "clearmotion",
            "clearnextlogin",
            "clearpvar",
            "clearvar",
            "cnumber",
            "componentdata",
            "componentname",
            "coordinatedistanceflat",
            "coordinatedistancewithz",
            "coordinategetns",
            "coordinategetwe",
            "coordinategetz",
            "coordinateparse",
            "coordinatetostring",
            "cos",
            "cosh",
            "cstr",
            "cstrf",
            "delayexec",
            "dictadditem",
            "dictclear",
            "dictcopy",
            "dictcreate",
            "dictgetitem",
            "dicthaskey",
            "dictkeys",
            "dictremovekey",
            "dictsize",
            "dictvalues",
            "echo",
            "exec",
            "floor",
            "getaccounthash",
            "getbusystate",
            "getcancastspell_buff",
            "getcancastspell_hunt",
            "getcharacterindex",
            "getcharattribute_base",
            "getcharattribute_buffed",
            "getcharboolprop",
            "getcharburden",
            "getchardoubleprop",
            "getcharintprop",
            "getcharquadprop",
            "getcharskill_base",
            "getcharskill_buffed",
            "getcharskill_traininglevel",
            "getcharstringprop",
            "getcharvital_base",
            "getcharvital_buffedmax",
            "getcharvital_current",
            "getcombatstate",
            "getcontaineritemcount",
            "getcooldownexpiration",
            "getcorpsesunopenedbyme",
            "getdatetimelocal",
            "getdatetimeutc",
            "getequippedweapontype",
            "getfellowid",
            "getfellowids",
            "getfellowname",
            "getfellownames",
            "getfellowshipcanrecruit",
            "getfellowshipcount",
            "getfellowshipisfull",
            "getfellowshipisleader",
            "getfellowshipisopen",
            "getfellowshipleaderid",
            "getfellowshiplocked",
            "getfellowshipname",
            "getfellowshipstatus",
            "getfreecontainerslots",
            "getfreeitemslots",
            "getgameday",
            "getgamehour",
            "getgamehourname",
            "getgamemonth",
            "getgamemonthname",
            "getgameticks",
            "getgameyear",
            "getgvar",
            "getheading",
            "getheadingto",
            "getinventorycountbytemplatetype",
            "getisday",
            "getisnight",
            "getisspellknown",
            "getitemcountininventorybyname",
            "getitemcountininventorybynamerx",
            "getknownspells",
            "getminutesuntilday",
            "getminutesuntilnight",
            "getmotion",
            "getobjectinternaltype",
            "getplayercoordinates",
            "getplayerlandblock",
            "getplayerlandcell",
            "getpvar",
            "getquestktprogress",
            "getquestktrequired",
            "getqueststatus",
            "getregexmatch",
            "getspellexpiration",
            "getspellexpirationbyname",
            "getunixtime",
            "getvar",
            "getworldname",
            "hascorpsebeenopenedbyme",
            "hexstr",
            "ifthen",
            "iif",
            "isfalse",
            "isportaling",
            "isrefreshingquests",
            "istrue",
            "listadd",
            "listclear",
            "listcontains",
            "listcopy",
            "listcount",
            "listcreate",
            "listfilter",
            "listfromrange",
            "listgetitem",
            "listindexof",
            "listinsert",
            "listlastindexof",
            "listmap",
            "listpop",
            "listreduce",
            "listremove",
            "listremoveat",
            "listreverse",
            "listsort",
            "lumavg",
            "lumtotal",
            "netcasts",
            "netclients",
            "ord",
            "randint",
            "round",
            "setcombatstate",
            "setgvar",
            "setmotion",
            "setnextlogin",
            "setpvar",
            "setvar",
            "sin",
            "sinh",
            "spelldata",
            "spellname",
            "sqrt",
            "statushud",
            "statushudcolored",
            "stopwatchcreate",
            "stopwatchelapsedseconds",
            "stopwatchstart",
            "stopwatchstop",
            "strlen",
            "tan",
            "tanh",
            "testgvar",
            "testpvar",
            "testquestflag",
            "testvar",
            "tostring",
            "touchgvar",
            "touchpvar",
            "touchvar",
            "uboptget",
            "uboptset",
            "uigetcontrol",
            "uisetlabel",
            "uisetvisible",
            "uiviewexists",
            "uiviewvisible",
            "ustadd",
            "ustopen",
            "ustsalvage",
            "vitae",
            "vtgetmeta",
            "vtgetmetastate",
            "vtgetsetting",
            "vtmacroenabled",
            "vtsetmetastate",
            "vtsetsetting",
            "wobjectfindall",
            "wobjectfindallbycontainer",
            "wobjectfindallbynamerx",
            "wobjectfindallbyobjectclass",
            "wobjectfindallbytemplatetype",
            "wobjectfindallinventory",
            "wobjectfindallinventorybynamerx",
            "wobjectfindallinventorybyobjectclass",
            "wobjectfindallinventorybytemplatetype",
            "wobjectfindalllandscape",
            "wobjectfindalllandscapebynamerx",
            "wobjectfindalllandscapebyobjectclass",
            "wobjectfindalllandscapebytemplatetype",
            "wobjectfindbyid",
            "wobjectfindininventorybyname",
            "wobjectfindininventorybynamerx",
            "wobjectfindininventorybytemplatetype",
            "wobjectfindnearestbynameandobjectclass",
            "wobjectfindnearestbyobjectclass",
            "wobjectfindnearestbytemplatetype",
            "wobjectfindnearestdoor",
            "wobjectfindnearestmonster",
            "wobjectgetactivespellids",
            "wobjectgetboolprop",
            "wobjectgetdoubleprop",
            "wobjectgethealth",
            "wobjectgethealthvalue",
            "wobjectgetid",
            "wobjectgetintprop",
            "wobjectgetisdooropen",
            "wobjectgetmanavalue",
            "wobjectgetname",
            "wobjectgetobjectclass",
            "wobjectgetopencontainer",
            "wobjectgetphysicscoordinates",
            "wobjectgetplayer",
            "wobjectgetselection",
            "wobjectgetspellids",
            "wobjectgetstaminavalue",
            "wobjectgetstringprop",
            "wobjectgettemplatetype",
            "wobjecthasdata",
            "wobjectisvalid",
            "wobjectlastidtime",
            "wobjectrequestdata",
            "xpavg",
            "xpduration",
            "xpmeter",
            "xpreset",
            "xptotal",
        ];

        string[] missing = expected
            .Except(panel.ExpressionFunctionNames, StringComparer.OrdinalIgnoreCase)
            .Order(StringComparer.OrdinalIgnoreCase)
            .ToArray();
        Assert.True(
            missing.Length == 0,
            "Missing UtilityBelt expression functions: " + string.Join(", ", missing));
    }

    private static PluginInventoryItem Item(
        uint id,
        string name,
        uint itemType,
        uint validLocations = 0u,
        int petClass = 0) => new PluginInventoryItem(
            id, 0, name, itemType, 1, 0,
            validLocations != 0u ? validLocations : DefaultSlotFor(itemType),
            0, 0, 0, 0,
            1, 0, 0, 0, petClass, 0, 0, false, 0, 0, 0, 0, 0, 0, 0, 0)
        {
            ObjectClass = ClassFor(itemType),
        };

    /// <summary>
    /// One owned item as the object table holds it: in the character's packs
    /// rather than on the landscape, and already appraised, so a rule can
    /// decide it without the counter waiting on anything.
    /// </summary>
    private static PluginWorldObject Carried(uint objectId, string name) =>
        new(objectId, 0u, name, PluginObjectClass.SpellComponent, 0u, 1u, 0u)
        {
            IsOwned = true,
            HasAppraisalData = true,
        };

    // The panel admits an item by its class and the slot it can be wielded
    // in, the way the host reports them; a fixture carries both so a test's
    // item is judged as an owned item would be.
    private static PluginObjectClass ClassFor(uint itemType) => itemType switch
    {
        _ when (itemType & 0x00008000u) != 0u => PluginObjectClass.WandStaffOrb,
        _ when (itemType & 0x00000100u) != 0u => PluginObjectClass.MissileWeapon,
        _ when (itemType & 0x00000001u) != 0u => PluginObjectClass.MeleeWeapon,
        _ when (itemType & 0x00000020u) != 0u => PluginObjectClass.Food,
        _ when (itemType & 0x00000800u) != 0u => PluginObjectClass.Gem,
        _ when (itemType & 0x00001000u) != 0u => PluginObjectClass.SpellComponent,
        _ when (itemType & 0x00000080u) != 0u => PluginObjectClass.Misc,
        _ => PluginObjectClass.Unknown,
    };

    private static uint DefaultSlotFor(uint itemType) => itemType switch
    {
        _ when (itemType & 0x00008000u) != 0u => ItemEnchantDefaults.Wand,
        _ when (itemType & 0x00000100u) != 0u => ItemEnchantDefaults.MissileWeapon,
        _ when (itemType & 0x00000001u) != 0u => ItemEnchantDefaults.MeleeWeapon,
        _ => 0u,
    };

    private static PluginEquipmentItem EquipmentItem(
        uint id,
        string name,
        uint itemType) => new(
            id,
            name,
            ItemType: itemType,
            ValidLocations: 0x00100000,
            EquippedLocation: 0,
            ContainerObjectId: 1,
            WielderObjectId: 0,
            CombatUse: 1,
            DamageType: 0,
            WeaponSkill: 44,
            Damage: 20,
            DamageVariance: 0.25)
        {
            ObjectClass = ClassFor(itemType),
        };

    private static PluginSpellInfo Spell(uint id, uint family, string description) => new(
        id,
        $"Spell {id}",
        family,
        Tier: 1,
        Difficulty: 10,
        ManaCost: 5,
        DurationSeconds: 60f,
        School: 1,
        description,
        IsSelfTargeted: true,
        IsBeneficial: true);

    [Fact]
    public void ADepartedIdIsRetiredEvenWhenTheEntryCountIsUnchanged()
    {
        var tracker = new BuffDueTracker();
        tracker.Observe(
        [
            new PluginActiveEnchantment(1u, 10u, 1, 600d),
            new PluginActiveEnchantment(2u, 20u, 1, 600d),
        ]);
        tracker.ForceAll();
        Assert.Equal([1u, 2u], tracker.ForcedSpellIds.Order());

        // Spell 1 drops; spell 2 gains a second layer. Two entries either way.
        tracker.Observe(
        [
            new PluginActiveEnchantment(2u, 20u, 1, 600d),
            new PluginActiveEnchantment(2u, 20u, 2, 900d),
        ]);

        Assert.Equal([2u], tracker.ForcedSpellIds.Order());
    }

    [Fact]
    public void AnOwnTellDuringTheLaunchWaitDoesNotDropTheCastLatch()
    {
        var automation = new FakeAutomation
        {
            ObjectId = 0x50000001u,
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
            ],
            KnownSelfBuffs =
            [
                Spell(1, 10, "Increases the caster's Life Magic skill by 10 points.")
                    with { Saying = "abracadabra" },
            ],
            // The server has taken the request and said nothing yet, so the
            // tracker sits in AwaitingLaunch with a Saying to match.
            SuppressCastCompletion = true,
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ToggleCombat();
        for (int tick = 0; tick < 3; tick++)
            panel.OnTick(0.3d);
        Assert.Equal([1u], automation.CastSpellIds);

        automation.PostChatFrom(0x50000001u, 3, "meet me at the portal");
        for (int tick = 0; tick < 3; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([1u], automation.CastSpellIds);
    }

    /// <summary>
    /// Only a gesture ends the launch wait. The character's own words in
    /// local chat look identical, so the log they came from is what tells the
    /// two apart.
    /// Mutation: drop the log-type half of the launch arm's test in
    /// <c>SpellCastTracker.ObserveChat</c> and the middle assertion fails —
    /// typing in local chat cancels the cast in flight.
    /// </summary>
    [Fact]
    public void OnlyAGestureEndsTheLaunchWaitNotTypedLocalSpeech()
    {
        var automation = new FakeAutomation
        {
            ObjectId = 0x50000001u,
            CurrentHealth = 100,
            MaxHealth = 100,
            CurrentStamina = 100,
            MaxStamina = 100,
            CurrentMana = 100,
            MaxMana = 100,
            Skills =
            [
                new PluginSkillInfo(1, "Life Magic", PluginSkillTraining.Trained, 300),
            ],
            KnownSelfBuffs =
            [
                Spell(1, 10, "Increases the caster's Life Magic skill by 10 points.")
                    with { Saying = "abracadabra" },
            ],
            SuppressCastCompletion = true,
        };
        var panel = new MossTankPanel(new FakeHost(automation));

        panel.ToggleCombat();
        for (int tick = 0; tick < 3; tick++)
            panel.OnTick(0.3d);
        Assert.Equal([1u], automation.CastSpellIds);

        // The same words typed into local chat are not a gesture: they carry
        // the plain log type, and the wait goes on.
        automation.PostChatFrom(0x50000001u, 0, "hocus pocus");
        for (int tick = 0; tick < 3; tick++)
            panel.OnTick(0.3d);
        Assert.Equal([1u], automation.CastSpellIds);

        // A DIFFERENT spell's words, gestured by us: this wait is over, the
        // latch drops, and the pass re-derives the same pick.
        automation.PostChatFrom(0x50000001u, 0, "hocus pocus", logTextType: 0x11u);
        for (int tick = 0; tick < 3; tick++)
            panel.OnTick(0.3d);

        Assert.Equal([1u, 1u], automation.CastSpellIds);
    }


    [Fact]
    public void LoginBindsExactlyOneSettingsFileAndOneBindingFile()
    {
        // The world reports the character's name without the server's display
        // marker until the character's own object has arrived, and with it
        // afterwards. Both spellings have to file under one profile.
        var storage = new MemoryStorage();
        var automation = new FakeAutomation
        {
            Name = "Acdream",
            WorldName = "sawato",
        };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        panel.SetNormalHealth(0.33f);

        automation.Name = "+Acdream";
        panel.OnTick(0.1d);
        panel.SetNormalHealth(0.44f);

        string[] settingsFiles = [.. storage.Text.Keys
            .Where(static key => key.EndsWith(".usd", StringComparison.Ordinal))];
        string[] bindingFiles = [.. storage.Text.Keys
            .Where(static key => key.EndsWith(".cdf", StringComparison.Ordinal))];

        Assert.Single(settingsFiles);
        Assert.Single(bindingFiles);
        Assert.DoesNotContain(
            settingsFiles,
            static key => key.Contains('+', StringComparison.Ordinal));
        Assert.DoesNotContain(
            bindingFiles,
            static key => key.Contains('+', StringComparison.Ordinal));
    }

    /// <summary>
    /// A session's whole life as the client reports it: no name while the
    /// plugin loads, the plain name from the character list, the marked name
    /// once the character is in the world, no name again while it logs out
    /// and back in, then the same two spellings again. All of it is one
    /// character, so it is one profile: nothing is filed under an empty name,
    /// and what was set before the relog is still in force after it.
    /// </summary>
    [Fact]
    public void ARelogKeepsTheCharactersOneProfile()
    {
        var storage = new MemoryStorage();
        var automation = new FakeAutomation { Name = string.Empty, WorldName = "sawato" };
        var panel = new MossTankPanel(new FakeHost(automation, storage));
        panel.OnTick(0.1d);

        automation.Name = "Mossy";
        panel.OnTick(0.1d);
        automation.Name = "+Mossy";
        panel.OnTick(0.1d);
        panel.SetNormalHealth(0.44f);

        // The relog: the name goes away and comes back.
        automation.Name = string.Empty;
        panel.OnTick(0.1d);
        // Nobody is named, so nobody else's settings take over meanwhile.
        Assert.Equal(0.44f, panel.NormalHealthValue, 3);
        automation.Name = "Mossy";
        panel.OnTick(0.1d);
        automation.Name = "+Mossy";
        panel.OnTick(0.1d);

        string[] settingsFiles = [.. storage.Text.Keys
            .Where(static key => key.EndsWith(".usd", StringComparison.Ordinal))];
        string[] bindingFiles = [.. storage.Text.Keys
            .Where(static key => key.EndsWith(".cdf", StringComparison.Ordinal))];

        Assert.Equal([SettingsKey("--Mossy_sawato.usd")], settingsFiles);
        Assert.Single(bindingFiles);
        Assert.DoesNotContain(
            storage.Text.Keys,
            static key => key.Contains("--_", StringComparison.Ordinal));
        Assert.Equal(0.44f, panel.NormalHealthValue, 3);
    }

    private static PluginSpellComponentInfo Component(uint id, string name) => new(
        id, id, name, 1d, 0u, 1d, 0u, id, "Scarab", string.Empty);

    private static PluginNavigationSnapshot NavigationAt(float heading) => new(
        IsAvailable: true,
        IsPortalSpace: false,
        LocalObjectId: 1u,
        Position: new PluginNavigationPosition(
            0x00010001u, 0d, 0d, 0d, heading, IsOutdoor: true),
        IsMoving: false,
        IsAirborne: false);

    private static string SettingsKey(string bareFileName) =>
        $"{VtankProfileDirectory.SettingsFolder}/{bareFileName}";

    private static string LootKey(string bareFileName) =>
        $"{VtankProfileDirectory.LootFolder}/{bareFileName}";

    private static void Command(MossTankPanel panel, string arguments) =>
        panel.ExecuteVtankCommand(new PluginCommand(
            "vt", arguments, "/vt " + arguments));

    private sealed class FakeHost(
        IAutomationSurface automation,
        IPluginStorage? storage = null,
        IPluginLootClassifierRegistry? lootClassifiers = null,
        IPluginStorage? vtankProfiles = null,
        IPluginWorldLines? worldLines = null) : IPluginHost
    {
        public bool HasUi => false;
        public IPluginWorldLines WorldLines { get; } =
            worldLines ?? NoOpPluginWorldLines.Instance;
        public IReadOnlyDictionary<string, string> SessionSettings { get; set; } =
            new Dictionary<string, string>(StringComparer.Ordinal);
        public FakeLogger Logger { get; } = new();
        public IPluginLogger Log => Logger;
        public IGameState State { get; } = new FakeState();
        public IEvents Events { get; } = new FakeEvents();
        public ISelectionService Selection { get; } = new FakeSelection();
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IPluginStorage Storage { get; } =
            storage ?? NoOpPluginStorage.Instance;
        public IAutomationSurface Automation { get; } = automation;
        public IPluginLootClassifierRegistry LootClassifiers { get; } =
            lootClassifiers ?? NoOpPluginLootClassifierRegistry.Instance;
        public IPluginStorage VtankProfiles { get; } =
            vtankProfiles ?? storage ?? NoOpPluginStorage.Instance;
    }

    private sealed class RecordingWorldLines : IPluginWorldLines
    {
        public List<Layer> Layers { get; } = [];

        public IPluginWorldLineLayer? CreateLayer()
        {
            var layer = new Layer();
            Layers.Add(layer);
            return layer;
        }

        public sealed class Layer : IPluginWorldLineLayer
        {
            public IReadOnlyList<PluginWorldLine> Lines { get; private set; } = [];
            public bool Disposed { get; private set; }
            public void SetLines(IReadOnlyList<PluginWorldLine> lines) => Lines = lines;
            public void Dispose() => Disposed = true;
        }
    }

    private sealed class FakeLootClassifierRegistry(
        params PluginLootClassifierInfo[] available)
        : IPluginLootClassifierRegistry
    {
        public IReadOnlyList<PluginLootClassifierInfo> Available { get; } =
            available;
    }

    private sealed class FakeAutomation
        : IAutomationSurface, ICharacterInfo, ISpellCatalog, IMagicCommands,
          IPluginChat, IItemAutomation, INavigationAutomation,
          IWorldObjectAutomation, IRecoveryAutomation, IEnchantmentAutomation,
          ICombatAutomation, ILoginAutomation, IDialogAutomation
    {
        public ILoginAutomation Login => this;

        public IDialogAutomation Dialogs => this;

        /// <summary>The confirmations answered, in order, and how.</summary>
        public List<(uint ContextId, bool Accept)> Answered { get; } = [];

        bool IDialogAutomation.Answer(uint contextId, bool accept)
        {
            Answered.Add((contextId, accept));
            return true;
        }

        /// <summary>
        /// The account's characters as the client last heard them, in the
        /// order the list arrived rather than any sorted order.
        /// </summary>
        public IReadOnlyList<PluginLoginCharacter> LoginRoster { get; set; } = [];

        public uint NextLoginObjectId { get; private set; }

        bool ILoginAutomation.IsAvailable => IsAvailable;

        IReadOnlyList<PluginLoginCharacter> ILoginAutomation.CaptureRoster() =>
            LoginRoster;

        bool ILoginAutomation.SetNextLogin(uint characterObjectId)
        {
            foreach (PluginLoginCharacter candidate in LoginRoster)
            {
                if (candidate.ObjectId != characterObjectId)
                    continue;
                NextLoginObjectId = characterObjectId;
                return true;
            }
            return false;
        }

        bool ILoginAutomation.ClearNextLogin()
        {
            NextLoginObjectId = 0u;
            return true;
        }

        /// <summary>
        /// Per-object tracked enchantments — what an item-enchant row's due
        /// test reads.
        /// </summary>
        public Dictionary<uint, List<PluginTrackedEnchantment>> ItemEnchantments
        { get; } = [];

        public IEnchantmentAutomation Enchantments => this;

        /// <summary>
        /// The other clients on this computer, when a test stages them. Left
        /// unset the surface is the inert one, which reads nobody.
        /// </summary>
        public INetworkAutomation? PeerNetwork { get; set; }

        INetworkAutomation IAutomationSurface.Network =>
            PeerNetwork ?? NoOpAutomationSurface.Instance;

        /// <summary>
        /// The shop counter, when a test stages one. Left unset the surface
        /// is the inert one, which never has a vendor open.
        /// </summary>
        public IVendorAutomation? VendorSurface { get; set; }

        IVendorAutomation IAutomationSurface.Vendor =>
            VendorSurface ?? NoOpAutomationSurface.Instance;

        /// <summary>
        /// The client's answer to the two allegiance commands, when a test
        /// stages one. Left unset the surface is the inert one, which reports
        /// every command unavailable.
        /// </summary>
        public IAllegianceAutomation? AllegianceSurface { get; set; }

        IAllegianceAutomation IAutomationSurface.Allegiance =>
            AllegianceSurface ?? NoOpAutomationSurface.Instance;

        /// <summary>The fellowship a test stages; unset, the inert one.</summary>
        public IFellowshipAutomation? FellowshipSurface { get; set; }

        IFellowshipAutomation IAutomationSurface.Fellowship =>
            FellowshipSurface ?? NoOpAutomationSurface.Instance;

        /// <summary>
        /// The labels hung over the world, when a test records them. Left
        /// unset the surface is the inert one, which shows nothing.
        /// </summary>
        public IWorldLabelAutomation? WorldLabels { get; set; }

        IWorldLabelAutomation IAutomationSurface.Labels =>
            WorldLabels ?? NoOpAutomationSurface.Instance;

        /// <summary>
        /// The corpses and open container this run reports, when a test needs
        /// them. Left unset the surface is the inert one every other test
        /// here has always had.
        /// </summary>
        public ILootAutomation? LootSurface { get; set; }

        ILootAutomation IAutomationSurface.Loot =>
            LootSurface ?? NoOpAutomationSurface.Instance;

        public IReadOnlyList<PluginTrackedEnchantment> Capture(uint targetObjectId) =>
            ItemEnchantments.TryGetValue(targetObjectId, out var held)
                ? held
                : [];
        private IReadOnlyList<PluginSpellInfo> _knownSelfBuffs = [];
        public IReadOnlyList<PluginSpellInfo> CatalogSpells { get; set; } = [];

        public bool IsAvailable { get; set; } = true;
        public ICharacterInfo Character => this;
        public ISpellCatalog Spells => this;
        public IMagicCommands Magic => this;
        public IPluginChat Chat => this;
        public ICombatAutomation Combat => this;
        public PluginCombatSnapshot CombatSnapshot { get; set; } = new()
        {
            Mode = PluginCombatMode.Magic,
        };
        PluginCombatSnapshot ICombatAutomation.Snapshot => CombatSnapshot;
        PluginCombatCommandResult ICombatAutomation.EnterMode(
            PluginCombatMode mode)
        {
            CombatSnapshot = CombatSnapshot with { Mode = mode };
            return new(PluginCombatCommandStatus.ModeChangeSent);
        }
        IReadOnlyList<PluginCombatTarget>
            ICombatAutomation.CaptureHostileTargets(float maximumDistance) => [];
        PluginCombatCommandResult ICombatAutomation.EnterDefaultMode() =>
            new(PluginCombatCommandStatus.Unavailable);
        PluginCombatCommandResult ICombatAutomation.BeginPhysicalAttack(
            uint targetObjectId, PluginAttackHeight height, float power) =>
            new(PluginCombatCommandStatus.Unavailable);
        PluginCombatCommandResult ICombatAutomation.ReleasePhysicalAttack() =>
            new(PluginCombatCommandStatus.Unavailable);
        PluginCombatCommandResult ICombatAutomation.AbortPhysicalAttack() =>
            new(PluginCombatCommandStatus.Stopped);
        public IItemAutomation Items => this;
        public INavigationAutomation Navigation => this;
        public IWorldObjectAutomation Objects => this;
        // An assessed item answers a property capture. The bag carries what
        // the item's own projection already knows, its mana above all, so a
        // reader that keys on the appraised properties sees the same numbers.
        public Dictionary<uint, PluginItemProperties> Properties { get; } = [];

        public bool TryCaptureProperties(uint objectId, out PluginItemProperties properties)
        {
            if (Properties.TryGetValue(objectId, out properties))
                return true;
            foreach (PluginInventoryItem item in ItemEntries)
            {
                if (item.ObjectId != objectId)
                    continue;
                var ints = new Dictionary<uint, int>();
                if (item.ItemMaximumMana > 0)
                {
                    ints[107u] = item.ItemCurrentMana;
                    ints[108u] = item.ItemMaximumMana;
                }
                properties = new PluginItemProperties(
                    ints,
                    new Dictionary<uint, long>(),
                    new Dictionary<uint, bool>(),
                    new Dictionary<uint, double>(),
                    new Dictionary<uint, string>(),
                    new Dictionary<uint, uint>(),
                    new Dictionary<uint, uint>());
                return true;
            }
            if (((IWorldObjectAutomation)this).TryGet(objectId, out _))
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
            properties = default;
            return false;
        }
        public IRecoveryAutomation Recovery => this;
        public bool IsInWorld => IsAvailable;
        public string Name { get; set; } = "Test Character";
        public string WorldName { get; set; } = string.Empty;

        public int Level { get; set; }
        public uint ObjectId { get; set; } = 1;
        public uint CurrentHealth { get; set; }
        public uint MaxHealth { get; set; }
        public int MainPackFreeSlots { get; set; } = 10;
        public uint CurrentStamina { get; set; }
        public uint MaxStamina { get; set; }
        public uint CurrentMana { get; set; }
        public uint MaxMana { get; set; }
        public IReadOnlyList<PluginSkillInfo> Skills { get; set; } = [];
        public IReadOnlyList<PluginAttributeInfo> Attributes { get; set; } = [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments { get; set; } = [];
        public IReadOnlyList<PluginInventoryItem> ItemEntries { get; set; } = [];
        public List<string> Messages { get; } = [];
        public List<uint> CastSpellIds { get; } = [];
        public List<uint> CastSelectionIds { get; } = [];
        public List<PluginMovementIntent> MovementIntents { get; } = [];
        public int ClearMovementCount { get; private set; }
        public IReadOnlyList<PluginWorldObject> WorldObjects { get; set; } = [];
        public int BusyReferences { get; set; }
        public PluginNavigationSnapshot NavigationSnapshot { get; set; }
        public PluginNavigationSnapshot Snapshot => NavigationSnapshot;

        /// <summary>
        /// What the client says about the moves and the jumps it has driven.
        /// Left alone its jump counter never moves, which is a client that
        /// took no jump.
        /// </summary>
        public PluginMoveReport MoveReport { get; set; }
        public int CaptureOwnedItemsCallCount { get; private set; }
        public IReadOnlyList<PluginInventoryItem> CaptureOwnedItems()
        {
            CaptureOwnedItemsCallCount++;
            return ItemEntries;
        }
        public IReadOnlyList<PluginWorldObject> CaptureObjects() => WorldObjects;

        public bool ItemsBusy { get; set; }
        // Items this client has not assessed yet; an identify request the
        // fake accepts assesses them. Empty, every item counts as assessed.
        public HashSet<uint> Unassessed { get; } = [];
        public List<uint> Identified { get; } = [];
        public HashSet<uint> IdentifyRefusals { get; } = [];

        /// <summary>
        /// Items the one description channel is busy on — somebody else's
        /// question is outstanding, so this one is not taken. The sweep has
        /// to leave such an item and come back to it rather than stop.
        /// </summary>
        public HashSet<uint> IdentifyBusy { get; } = [];
        public PluginItemCommandStatus IdentifyStatus { get; set; } =
            PluginItemCommandStatus.Started;

        public PluginItemCommandResult Identify(uint objectId)
        {
            Identified.Add(objectId);
            if (IdentifyBusy.Contains(objectId))
                return new(PluginItemCommandStatus.Busy);
            if (IdentifyRefusals.Contains(objectId))
                return new(PluginItemCommandStatus.Refused);
            if (IdentifyStatus == PluginItemCommandStatus.Started)
                Unassessed.Remove(objectId);
            return new(IdentifyStatus);
        }

        bool IItemAutomation.IsBusy => ItemsBusy;

        /// <summary>
        /// Object ids handed to <c>Items.Use</c>, in order. The request is
        /// accepted and <c>Items.LastCompletion</c> never moves — a server
        /// that took the use and has not answered yet, which is the window a
        /// staged recharge has to survive.
        /// </summary>
        public List<uint> UsedItemIds { get; } = [];

        public PluginItemCommandResult Use(uint objectId)
        {
            UsedItemIds.Add(objectId);
            return new PluginItemCommandResult(PluginItemCommandStatus.Started);
        }

        /// <summary>
        /// Item pairs handed to <c>Items.Apply</c>, in order: the tool and
        /// what it was used on.
        /// </summary>
        public List<(uint One, uint Two)> Applied { get; } = [];

        public PluginItemCommandResult Apply(uint objectId, uint targetObjectId)
        {
            Applied.Add((objectId, targetObjectId));
            return new PluginItemCommandResult(PluginItemCommandStatus.Started);
        }

        /// <summary>Hand-overs asked for, with the amount each carried.</summary>
        public List<(uint Item, uint Target, uint Amount)> Gives { get; } = [];

        public PluginItemCommandResult Give(
            uint objectId,
            uint targetObjectId,
            uint amount = 0u)
        {
            Gives.Add((objectId, targetObjectId, amount));
            return new PluginItemCommandResult(PluginItemCommandStatus.Started);
        }

        /// <summary>
        /// The server's answer to a use, as the host publishes it: the latest
        /// receipt, read by polling, with a revision that only ever moves
        /// forward. Left alone it never moves, which is the unanswered case.
        /// </summary>
        public PluginItemUseCompletion ItemCompletion { get; set; }

        PluginItemUseCompletion IItemAutomation.LastCompletion => ItemCompletion;

        bool IWorldObjectAutomation.TryGet(
            uint objectId,
            out PluginWorldObject value)
        {
            foreach (PluginWorldObject candidate in WorldObjects)
            {
                if (candidate.ObjectId != objectId)
                    continue;
                value = candidate;
                return true;
            }
            // Owned items are in the object table too, already assessed: the
            // state a profiled item is in before it may be used.
            foreach (PluginInventoryItem item in ItemEntries)
            {
                if (item.ObjectId != objectId)
                    continue;
                value = new PluginWorldObject(
                    item.ObjectId, item.WeenieClassId, item.Name, PluginObjectClass.Unknown,
                    item.ItemType, item.ContainerObjectId, item.WielderObjectId)
                {
                    LastIdTime = Unassessed.Contains(item.ObjectId) ? 0 : 1,
                };
                return true;
            }
            value = default;
            return false;
        }

        public PluginRecoveryResult ClearOneBusyReference()
        {
            int before = BusyReferences;
            BusyReferences = Math.Max(0, BusyReferences - 1);
            return new(true, before, BusyReferences);
        }

        public int KnownSelfBuffReads { get; private set; }

        public IReadOnlyList<PluginSpellInfo> KnownSelfBuffs
        {
            get
            {
                KnownSelfBuffReads++;
                return _knownSelfBuffs;
            }
            set => _knownSelfBuffs = value;
        }

        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            foreach (PluginSkillInfo candidate in Skills)
            {
                if (candidate.SkillId == skillId)
                {
                    skill = candidate;
                    return true;
                }
            }
            skill = default;
            return false;
        }

        public bool TryGet(uint spellId, out PluginSpellInfo info)
        {
            foreach (PluginSpellInfo candidate in _knownSelfBuffs)
            {
                if (candidate.SpellId == spellId)
                {
                    info = candidate;
                    return true;
                }
            }
            foreach (PluginSpellInfo candidate in CatalogSpells)
            {
                if (candidate.SpellId == spellId)
                {
                    info = candidate;
                    return true;
                }
            }
            info = default;
            return false;
        }

        public bool IsCasting { get; set; }

        public uint NextCastWeenieError { get; set; }
        public bool SuppressCastCompletion { get; set; }

        public bool SuppressCastResultText { get; set; }

        public string? CastResultText { get; set; }

        public List<PluginChatMessage> ChatLines { get; } = [];
        private ulong _chatSequence;

        public IReadOnlyList<PluginChatMessage> CaptureMessages(ulong afterSequence)
        {
            var result = new List<PluginChatMessage>();
            foreach (PluginChatMessage message in ChatLines)
            {
                if (message.Sequence > afterSequence)
                    result.Add(message);
            }
            return result;
        }

        /// <summary>
        /// A line the client logged. <paramref name="logTextType"/> is the log
        /// it came from: 0x07 for a spell result, 0 for a plain line.
        /// </summary>
        public void PostChat(string text, uint logTextType = 0u) =>
            ChatLines.Add(new PluginChatMessage(
                ++_chatSequence, 0u, 0, string.Empty, text, string.Empty)
            {
                LogTextType = (int)logTextType,
            });
        public void PostChatFrom(
            uint senderObjectId,
            int kind,
            string text,
            uint logTextType = 0u) =>
            ChatLines.Add(new PluginChatMessage(
                ++_chatSequence, senderObjectId, kind, string.Empty, text,
                string.Empty)
            {
                LogTextType = (int)logTextType,
            });


        private PluginCastCompletion _lastCompletion;
        public PluginCastCompletion LastCompletion => _lastCompletion;

        public Dictionary<uint, PluginSpellComponentInfo> Components { get; } = [];

        public bool TryGetComponent(
            uint componentId, out PluginSpellComponentInfo info) =>
            Components.TryGetValue(componentId, out info);

        public HashSet<uint> RefusedCastSpellIds { get; } = [];

        /// <summary>
        /// Spell ids the host answers <c>HasComponents(false)</c> for — the
        /// authoritative formula test <c>BuffCastability</c> now runs INSIDE
        /// the tier accept, so these are never picked at all.
        /// </summary>
        public HashSet<uint> MissingComponentSpellIds { get; } = [];

        public Dictionary<uint, PluginCastRequestResult> CastRefusals { get; } = [];

        public bool HasComponents(uint spellId) =>
            !MissingComponentSpellIds.Contains(spellId);

        public PluginCastRequestResult RequestCast(uint spellId)
        {
            if (CastRefusals.TryGetValue(
                    spellId, out PluginCastRequestResult refusal))
                return refusal;
            return Cast(spellId)
                ? PluginCastRequestResult.Sent
                : PluginCastRequestResult.Unavailable;
        }

        public PluginCastGate EvaluateGate(uint spellId) => PluginCastGate.Ready;
        public bool Cast(uint spellId)
        {
            if (RefusedCastSpellIds.Contains(spellId))
                return false;
            CastSpellIds.Add(spellId);
            CastSelectionIds.Add(CurrentSelection?.Invoke() ?? 0u);
            if (!SuppressCastCompletion)
            {
                _lastCompletion = new PluginCastCompletion(
                    _lastCompletion.Revision + 1,
                    spellId,
                    0u,
                    NextCastWeenieError);
            }
            if (NextCastWeenieError == 0u
                && !SuppressCastCompletion
                && !SuppressCastResultText)
            {
                string name = TryGet(spellId, out PluginSpellInfo spell)
                    ? spell.Name
                    : $"Spell {spellId}";
                PostChat(
                    CastResultText ?? $"You cast {name} on yourself",
                    logTextType: 0x07u);
            }

            if (NextCastWeenieError == 0u
                && !SuppressCastCompletion
                && CastResultText is null)
            {
                LandEnchantment(spellId);
                RaiseSkill(spellId);
            }
            return true;
        }

        public (uint SpellId, uint SkillId, uint Level)? RaiseSkillOnCast { get; set; }

        private void RaiseSkill(uint spellId)
        {
            if (RaiseSkillOnCast is not { } raise || raise.SpellId != spellId)
                return;
            var next = new List<PluginSkillInfo>();
            foreach (PluginSkillInfo skill in Skills)
            {
                next.Add(skill.SkillId == raise.SkillId
                    ? skill with { Current = raise.Level }
                    : skill);
            }
            Skills = next;
        }

        public Func<uint>? CurrentSelection { get; set; }

        /// <summary>Fold a landed enchantment into the tracked table.</summary>
        private void LandEnchantment(uint spellId)
        {
            foreach (PluginSpellInfo spell in KnownSelfBuffs)
            {
                if (spell.SpellId != spellId)
                    continue;
                uint target = CurrentSelection?.Invoke() ?? 0u;

                if (spell.School == 32u
                    && !spell.IsSelfTargeted
                    && target == ObjectId)
                {
                    return;
                }

                if (spell.School == 32u
                    && !spell.IsSelfTargeted
                    && target != 0u
                    && target != ObjectId)
                {
                    if (!ItemEnchantments.TryGetValue(target, out var onItem))
                    {
                        onItem = [];
                        ItemEnchantments[target] = onItem;
                    }
                    onItem.RemoveAll(held => held.SpellId == spellId);
                    onItem.Add(new PluginTrackedEnchantment(
                        target,
                        spellId,
                        spell.Family,
                        spell.Tier,
                        spell.IsUntargeted,
                        EnchantmentDurationSeconds));
                    return;
                }
                var next = new List<PluginActiveEnchantment>();
                foreach (PluginActiveEnchantment held in ActiveEnchantments)
                {
                    if (held.SpellId != spellId)
                        next.Add(held);
                }
                next.Add(new PluginActiveEnchantment(
                    spellId,
                    spell.Family,
                    spell.Tier,
                    EnchantmentDurationSeconds));
                ActiveEnchantments = next;
                return;
            }
        }

        /// <summary>How long a landed fake buff runs for.</summary>
        public double EnchantmentDurationSeconds { get; set; } = 1800d;
        public void PostSystemMessage(string text) => Messages.Add(text);

        /// <summary>
        /// Lines handed back to the client's chat bar, in order: what a
        /// delayed command or a think actually does.
        /// </summary>
        public List<string> Submitted { get; } = [];

        public bool Submit(string text)
        {
            Submitted.Add(text);
            return true;
        }

        /// <summary>Lines written in one of the client's own text classes.</summary>
        public List<(string Text, int Kind)> Posted { get; } = [];

        public void PostMessage(string text, int kind) => Posted.Add((text, kind));

        /// <summary>The container the client currently has open, if any.</summary>
        public uint OpenContainerObjectId { get; set; }

        /// <summary>The typed-line interceptors installed, in order; a handle disposed removes its own.</summary>
        public List<Func<string, PluginChatInputDecision>> Interceptors { get; } = [];

        public IDisposable RegisterInputInterceptor(Func<string, PluginChatInputDecision> intercept)
        {
            Interceptors.Add(intercept);
            return new InterceptorHandle(() => Interceptors.Remove(intercept));
        }

        private sealed class InterceptorHandle(Action revoke) : IDisposable
        {
            public void Dispose() => revoke();
        }

        /// <summary>Objects the navigation surface can be asked about by id.</summary>
        public List<PluginNavigationObject> NavigationObjects { get; } = [];

        public bool TryGetObject(uint objectId, out PluginNavigationObject value)
        {
            foreach (PluginNavigationObject candidate in NavigationObjects)
            {
                if (candidate.ObjectId != objectId)
                    continue;
                value = candidate;
                return true;
            }
            value = default;
            return false;
        }
        public PluginNavigationCommandStatus SetMovementIntent(
            in PluginMovementIntent intent)
        {
            MovementIntents.Add(intent);
            return PluginNavigationCommandStatus.Accepted;
        }
        public PluginNavigationCommandStatus ClearMovementIntent()
        {
            ClearMovementCount++;
            return PluginNavigationCommandStatus.Accepted;
        }

        /// <summary>The walks asked of the client's navigation, and the latest walk's report.</summary>
        public List<PluginNavigationPosition> Walks { get; } = [];
        public PluginGoToReport GoToReport { get; set; }

        public PluginNavigationCommandStatus GoTo(PluginNavigationPosition position, float arrivalMeters)
        {
            Walks.Add(position);
            GoToReport = new PluginGoToReport(GoToReport.Sequence + 1, PluginGoToState.Planning, 0u, float.NaN, 0, "planning");
            return PluginNavigationCommandStatus.Accepted;
        }

        public PluginNavigationCommandStatus StopGoTo()
        {
            GoToReport = GoToReport with { State = PluginGoToState.Stopped, Reason = "stopped" };
            return PluginNavigationCommandStatus.Accepted;
        }
    }

    private sealed class CombatCapableFakeAutomation :
        IAutomationSurface, ICharacterInfo, ISpellCatalog, IMagicCommands,
        IPluginChat, ICombatAutomation, IEquipmentAutomation, IItemAutomation,
        INavigationAutomation, IWorldObjectAutomation
    {
        public INavigationAutomation Navigation => this;
        public IWorldObjectAutomation Objects => this;
        // An assessed item answers a property capture. The bag carries what
        // the item's own projection already knows, its mana above all, so a
        // reader that keys on the appraised properties sees the same numbers.
        public Dictionary<uint, PluginItemProperties> Properties { get; } = [];

        public bool TryCaptureProperties(uint objectId, out PluginItemProperties properties)
        {
            if (Properties.TryGetValue(objectId, out properties))
                return true;
            foreach (PluginInventoryItem item in ItemEntries)
            {
                if (item.ObjectId != objectId)
                    continue;
                var ints = new Dictionary<uint, int>();
                if (item.ItemMaximumMana > 0)
                {
                    ints[107u] = item.ItemCurrentMana;
                    ints[108u] = item.ItemMaximumMana;
                }
                properties = new PluginItemProperties(
                    ints,
                    new Dictionary<uint, long>(),
                    new Dictionary<uint, bool>(),
                    new Dictionary<uint, double>(),
                    new Dictionary<uint, string>(),
                    new Dictionary<uint, uint>(),
                    new Dictionary<uint, uint>());
                return true;
            }
            if (((IWorldObjectAutomation)this).TryGet(objectId, out _))
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
            properties = default;
            return false;
        }
        // Owned equipment and items are in the object table, already assessed:
        // the state a profiled item is in before the macro may use it.
        bool IWorldObjectAutomation.TryGet(uint objectId, out PluginWorldObject value)
        {
            foreach (PluginEquipmentItem item in EquipmentItems)
            {
                if (item.ObjectId != objectId)
                    continue;
                value = new PluginWorldObject(
                    item.ObjectId, 0u, item.Name, item.ObjectClass,
                    item.ItemType, item.ContainerObjectId, item.WielderObjectId)
                {
                    LastIdTime = 1,
                    IsOwned = true,
                };
                return true;
            }
            foreach (PluginInventoryItem item in ItemEntries)
            {
                if (item.ObjectId != objectId)
                    continue;
                value = new PluginWorldObject(
                    item.ObjectId, item.WeenieClassId, item.Name, PluginObjectClass.Unknown,
                    item.ItemType, item.ContainerObjectId, item.WielderObjectId)
                {
                    LastIdTime = 1,
                };
                return true;
            }
            // The monsters the tests stage are in the world too: the controller
            // drops a target the object table cannot find.
            foreach (PluginCombatTarget target in Targets)
            {
                if (target.ObjectId != objectId)
                    continue;
                value = new PluginWorldObject(
                    target.ObjectId, target.WeenieClassId, target.Name,
                    PluginObjectClass.Unknown, 0u, 0u, 0u);
                return true;
            }
            value = default;
            return false;
        }
        public PluginNavigationSnapshot NavigationSnapshot { get; set; } =
            NavigationAt(0f);
        public PluginNavigationSnapshot Snapshot => NavigationSnapshot;
        public List<PluginWorldObject> WorldObjects { get; } = [];
        public IReadOnlyList<PluginWorldObject> CaptureObjects()
        {
            var objects = new List<PluginWorldObject>(WorldObjects);
            foreach (PluginEquipmentItem item in EquipmentItems)
            {
                objects.Add(new PluginWorldObject(
                    item.ObjectId, 0u, item.Name, item.ObjectClass,
                    item.ItemType, item.ContainerObjectId, item.WielderObjectId)
                {
                    LastIdTime = 1,
                    IsOwned = true,
                });
            }
            return objects;
        }
        public bool TryGetObject(uint objectId, out PluginNavigationObject value)
        {
            value = default;
            return false;
        }
        public PluginNavigationCommandStatus SetMovementIntent(
            in PluginMovementIntent intent) =>
            PluginNavigationCommandStatus.Accepted;
        public PluginNavigationCommandStatus ClearMovementIntent() =>
            PluginNavigationCommandStatus.Accepted;

        public bool IsAvailable { get; set; } = true;
        public ICharacterInfo Character => this;
        public ISpellCatalog Spells => this;
        public IMagicCommands Magic => this;
        public IPluginChat Chat => this;
        public ICombatAutomation Combat => this;
        public IEquipmentAutomation Equipment => this;
        public IItemAutomation Items => this;

        public List<string> CallLog { get; } = [];

        public bool IsInWorld => IsAvailable;
        public uint ObjectId { get; set; } = 1;
        public uint CurrentHealth { get; set; }
        public uint MaxHealth { get; set; }
        public uint CurrentStamina { get; set; }
        public uint MaxStamina { get; set; }
        public uint CurrentMana { get; set; }
        public uint MaxMana { get; set; }
        public int SummoningMastery => 0;
        public IReadOnlyList<PluginSkillInfo> Skills { get; set; } = [];
        public IReadOnlyList<PluginAttributeInfo> Attributes { get; set; } = [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments { get; set; } = [];
        public IReadOnlyList<PluginSpellInfo> KnownSelfBuffs { get; set; } = [];
        public IReadOnlyList<PluginSpellInfo> KnownAttackSpells { get; set; } = [];
        public IReadOnlyList<PluginSpellInfo> KnownCombatSpells { get; set; } = [];
        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            foreach (PluginSkillInfo candidate in Skills)
            {
                if (candidate.SkillId == skillId)
                {
                    skill = candidate;
                    return true;
                }
            }
            skill = default;
            return false;
        }
        /// <summary>Spells the catalog knows that are not self buffs.</summary>
        public List<PluginSpellInfo> SpellLookup { get; } = [];

        public bool TryGet(uint spellId, out PluginSpellInfo info)
        {
            foreach (PluginSpellInfo candidate in KnownSelfBuffs)
            {
                if (candidate.SpellId == spellId)
                {
                    info = candidate;
                    return true;
                }
            }
            foreach (PluginSpellInfo candidate in SpellLookup)
            {
                if (candidate.SpellId == spellId)
                {
                    info = candidate;
                    return true;
                }
            }
            info = default;
            return false;
        }

        // ── magic ─────────────────────────────────────────────────────
        public bool IsCasting { get; set; }
        public List<uint> CastSpellIds { get; } = [];

        public uint NextCastWeenieError { get; set; }
        private PluginCastCompletion _lastCompletion;
        public PluginCastCompletion LastCompletion => _lastCompletion;

        public PluginCastGate EvaluateGate(uint spellId) => PluginCastGate.Ready;
        public List<PluginChatMessage> ChatLines { get; } = [];
        private ulong _chatSequence;

        public IReadOnlyList<PluginChatMessage> CaptureMessages(ulong afterSequence)
        {
            var result = new List<PluginChatMessage>();
            foreach (PluginChatMessage message in ChatLines)
            {
                if (message.Sequence > afterSequence)
                    result.Add(message);
            }
            return result;
        }

        /// <summary>
        /// A line the client logged. <paramref name="logTextType"/> is the log
        /// it came from: 0x07 for a spell result, 0 for a plain line.
        /// </summary>
        public void PostChat(string text, uint logTextType = 0u) =>
            ChatLines.Add(new PluginChatMessage(
                ++_chatSequence, 0u, 0, string.Empty, text, string.Empty)
            {
                LogTextType = (int)logTextType,
            });

        public List<uint> CastTargets { get; } = [];

        public PluginCastGate EvaluateGate(uint spellId, uint targetObjectId) =>
            PluginCastGate.Ready;

        public bool Cast(uint spellId, uint targetObjectId)
        {
            CastTargets.Add(targetObjectId);
            return Cast(spellId);
        }

        public bool Cast(uint spellId)
        {
            CastSpellIds.Add(spellId);
            CallLog.Add($"Cast:{spellId}");
            _lastCompletion = new PluginCastCompletion(
                _lastCompletion.Revision + 1,
                spellId,
                0u,
                NextCastWeenieError);
            if (NextCastWeenieError == 0u)
            {
                string castName = TryGet(spellId, out PluginSpellInfo cast)
                    ? cast.Name
                    : $"Spell {spellId}";
                PostChat($"You cast {castName} on yourself", logTextType: 0x07u);
            }
            foreach (PluginSpellInfo spell in KnownSelfBuffs)
            {
                if (spell.SpellId != spellId)
                    continue;
                ActiveEnchantments =
                [
                    .. ActiveEnchantments,
                    new PluginActiveEnchantment(
                        spellId,
                        spell.Family,
                        spell.Tier,
                        600d),
                ];
                break;
            }
            return true;
        }

        public List<string> Messages { get; } = [];
        public void PostSystemMessage(string text) => Messages.Add(text);

        bool IItemAutomation.IsAvailable => true;
        public bool ItemsBusy { get; set; }
        bool IItemAutomation.IsBusy => ItemsBusy;
        public IReadOnlyList<PluginInventoryItem> ItemEntries { get; set; } = [];
        public IReadOnlyList<PluginInventoryItem> CaptureOwnedItems() => ItemEntries;
        public int ApplyCount { get; private set; }
        public (uint Item, uint Target) LastAppliedItem { get; private set; }
        public PluginItemUseCompletion ItemCompletion { get; set; }
        PluginItemUseCompletion IItemAutomation.LastCompletion => ItemCompletion;
        PluginItemCommandResult IItemAutomation.Apply(
            uint objectId,
            uint targetObjectId)
        {
            ApplyCount++;
            LastAppliedItem = (objectId, targetObjectId);
            CallLog.Add($"Apply:{objectId:X8}->{targetObjectId:X8}");
            return new(PluginItemCommandStatus.Started);
        }

        public PluginCombatSnapshot CombatSnapshot { get; set; } = new(
            SelectedObjectId: 0,
            PluginCombatMode.Peace,
            PluginAttackHeight.Medium,
            DesiredPower: 0.5f,
            PowerBarLevel: 0f,
            BuildInProgress: false,
            RequestInProgress: false,
            ServerResponsePending: false,
            RepeatAttackInProgress: false);
        PluginCombatSnapshot ICombatAutomation.Snapshot => CombatSnapshot;
        public IReadOnlyList<PluginCombatTarget> Targets { get; set; } = [];
        public IReadOnlyList<PluginCombatTarget> CaptureHostileTargets(
            float maximumDistance) => Targets;
        public int ModeChangeRequests { get; private set; }
        public PluginCombatCommandResult EnterMode(PluginCombatMode mode)
        {
            ModeChangeRequests++;
            CombatSnapshot = CombatSnapshot with { Mode = mode };
            CallLog.Add($"EnterMode:{mode}");
            return new(PluginCombatCommandStatus.ModeChangeSent);
        }
        public PluginCombatCommandResult EnterDefaultMode()
        {
            PluginCombatMode mode = PluginCombatMode.Peace;
            foreach (PluginEquipmentItem item in EquipmentItems)
            {
                if (!item.IsEquipped)
                    continue;
                mode = (item.ItemType & 0x00008000u) != 0u
                    ? PluginCombatMode.Magic
                    : PluginCombatMode.Melee;
                break;
            }
            CombatSnapshot = CombatSnapshot with { Mode = mode };
            CallLog.Add($"EnterDefaultMode:{mode}");
            return new(PluginCombatCommandStatus.ModeChangeSent);
        }
        public int BeginCount { get; private set; }
        public uint LastBeginTarget { get; private set; }
        public PluginCombatCommandResult BeginPhysicalAttack(
            uint targetObjectId, PluginAttackHeight height, float power)
        {
            LastBeginTarget = targetObjectId;
            BeginCount++;
            CallLog.Add($"Attack:{targetObjectId:X8}");
            return new(PluginCombatCommandStatus.Started);
        }
        public int ReleaseCount { get; private set; }
        public PluginCombatCommandResult ReleasePhysicalAttack()
        {
            ReleaseCount++;
            CallLog.Add("Release");
            return new(PluginCombatCommandStatus.Released);
        }
        public PluginCombatCommandResult AbortPhysicalAttack() =>
            new(PluginCombatCommandStatus.Stopped);

        // ── equipment ─────────────────────────────────────────────────
        bool IEquipmentAutomation.IsAvailable => true;
        bool IEquipmentAutomation.IsBusy => false;
        public IReadOnlyList<PluginEquipmentItem> EquipmentItems { get; set; } = [];
        public event Action<PluginEquipmentObservation>? PlacementObserved;
        public IReadOnlyList<PluginEquipmentPlacement>
            CaptureWorldPlacementsInOrder() =>
            EquipmentItems.Select(static item => new PluginEquipmentPlacement(
                item.ObjectId, item.EquippedLocation)).ToArray();
        public IReadOnlyList<PluginEquipmentItem> CaptureOwnedEquipment() =>
            EquipmentItems;
        public PluginEquipmentCommandResult Equip(
            uint objectId,
            uint requestedLocation = 0u)
        {
            CallLog.Add($"Equip:{objectId:X8}");
            IReadOnlyList<PluginEquipmentItem> before = EquipmentItems;
            EquipmentItems = EquipmentItems
                .Select(item => item.ObjectId == objectId
                    ? item with { EquippedLocation = 0x00100000u }
                    : item with { EquippedLocation = 0u })
                .ToArray();
            foreach (PluginEquipmentItem old in before)
            {
                if (old.ObjectId != objectId && old.EquippedLocation != 0u)
                    PlacementObserved?.Invoke(new PluginEquipmentObservation(
                        old.ObjectId, 0u, true));
            }
            PlacementObserved?.Invoke(new PluginEquipmentObservation(
                objectId, 0x00100000u, false));
            return new(PluginEquipmentCommandStatus.Started);
        }
    }

    private sealed class FakeLogger : IPluginLogger
    {
        public List<string> Warnings { get; } = [];

        public List<string> Infos { get; } = [];
        public List<string> Errors { get; } = [];
        public void Info(string message) => Infos.Add(message);
        public void Warn(string message) => Warnings.Add(message);
        public void Error(string message, Exception? exception = null) =>
            Errors.Add(message);
    }

    private sealed class MemoryStorage : IPluginStorage
    {
        public Dictionary<string, string> Text { get; } =
            new(StringComparer.Ordinal);
        public bool IsAvailable => true;
        public string? RootPath { get; set; }
        public string? ReadText(string key) =>
            Text.TryGetValue(key, out string? value) ? value : null;
        public IReadOnlyList<string> List(string prefix) => Text.Keys
            .Where(key => prefix.Length == 0
                || key.StartsWith(prefix + "/", StringComparison.Ordinal))
            .OrderBy(static key => key, StringComparer.Ordinal)
            .ToArray();
        /// <summary>Every folder the plugin asked for, in order.</summary>
        public List<string> Directories { get; } = [];
        public bool EnsureDirectory(string prefix)
        {
            Directories.Add(prefix);
            return true;
        }
        public void WriteText(string key, string content) => Text[key] = content;
        public bool Delete(string key) => Text.Remove(key);
    }

    private sealed class FakeState : IGameState
    {
        public IReadOnlyList<WorldEntitySnapshot> Entities => [];
    }

    private sealed class FakeEvents : IEvents
    {
        private Action<PluginConfirmation>? _confirmation;

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

        public event Action<PluginConfirmation> ConfirmationRequested
        {
            add => _confirmation += value;
            remove => _confirmation -= value;
        }

        /// <summary>How many handlers are listening for confirmations.</summary>
        public int ConfirmationListenerCount =>
            _confirmation?.GetInvocationList().Length ?? 0;

        public void RaiseConfirmation(PluginConfirmation confirmation) =>
            _confirmation?.Invoke(confirmation);
    }

    private sealed class FakeSelection : ISelectionService
    {
        public uint? SelectedObjectId { get; private set; }
        public uint? PreviousObjectId { get; private set; }

        public event Action<SelectionChangedEvent> Changed
        {
            add { }
            remove { }
        }

        public bool Select(uint objectId)
        {
            PreviousObjectId = SelectedObjectId;
            SelectedObjectId = objectId;
            return true;
        }

        public bool Clear()
        {
            PreviousObjectId = SelectedObjectId;
            SelectedObjectId = null;
            return true;
        }
    }
}

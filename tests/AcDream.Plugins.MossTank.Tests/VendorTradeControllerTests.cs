using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The vendor run as an owner of the character: it wakes when a shop opens,
/// finds its profile the tiered way, stacks first, then buys and sells round
/// by round through the staged lists and the two commit calls, waiting on
/// the server's answer between rounds.
/// </summary>
public sealed class VendorTradeControllerTests
{
    private const uint Player = 1u;
    private const uint Shopkeeper = 500u;
    private const uint RationListing = 9001u;
    private const uint Sword = 20u;
    private const uint Coins = 30u;

    /// <summary>
    /// Only pyreal coins pay for a buy. Peas are money-class items but the
    /// server does not count them as coin, so counting their stacks as
    /// pyreals plans a buy the server refuses outright.
    /// Mutation: count every money-class stack as coin and the buy asks for
    /// 9 rations it cannot pay for.
    /// </summary>
    [Fact]
    public void PeasAreNotCountedAsCoinWhenSizingABuy()
    {
        FakeAutomation automation = ShopWithRations();
        automation.Owned =
        [
            Coin(30),
            Owned(41u, "Gold Pea", value: 22500, stack: 20, itemType: 0x40u) with
            {
                WeenieClassId = 8331u,
                ObjectClass = PluginObjectClass.Money,
                MaximumStackSize = 100,
            },
        ];
        (VendorTradeController controller, MemoryStorage storage) = Controller(automation);
        WriteProfile(storage, "mosstank/ub/autovendor/Shopkeeper.utl");

        automation.Open(Shopkeeper);
        for (int i = 0; i < 6 && automation.BuyAllCalls.Count == 0; i++)
            controller.Tick(0.1d, canAct: true);

        // 30 pyreals at 5 a ration buy 5 once the server's rounding is
        // allowed for, not the 9 that 30 + 20 peas would.
        Assert.Equal([(RationListing, 5)], automation.BuyAllCalls.Single());
    }

    /// <summary>
    /// The whole visit, round by round: the run starts on the open, the
    /// stack pass goes first, the buy round stages and commits, the sell
    /// round follows the server's answer, and the run ends when a round has
    /// nothing left to do. Mutation: replanning before the server answers
    /// commits the same sell twice.
    /// </summary>
    [Fact]
    public void AVisitBuysThenSellsThenFinishes()
    {
        FakeAutomation automation = ShopWithRations();
        automation.Owned =
        [
            Owned(Sword, "Sword", value: 40, stack: 1, itemType: 0x1u),
            Coin(100),
        ];
        (VendorTradeController controller, MemoryStorage storage) = Controller(automation);
        WriteProfile(storage, "mosstank/ub/autovendor/Shopkeeper.utl");
        int stackPasses = 0;
        controller.BindStackCramPass((_, _) => ++stackPasses < 3);

        automation.Open(Shopkeeper);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.True(controller.IsRunning);
        Assert.Empty(automation.BuyAllCalls);
        // Two more ticks of stacking before the planner runs.
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(3, stackPasses);

        Assert.True(controller.Tick(0.1d, canAct: true));
        // 100 coin at 5 a ration buys 20, and the rule wants 10.
        Assert.Equal([(RationListing, 10)], automation.BuyAllCalls.Single());
        Assert.Empty(automation.SellAllCalls);

        // Nothing moves until the server answers.
        Assert.True(controller.Tick(1d, canAct: true));
        Assert.Single(automation.BuyAllCalls);

        automation.Owned = [automation.Owned[0], Coin(50), Owned(40u, "Ration", 50, 10, itemType: 0x20u)];
        automation.Complete(PluginVendorTransactionKind.Buy, true);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([Sword], automation.SellAllCalls.Single());
        Assert.Single(automation.BuyAllCalls);

        automation.Owned = [Coin(80), automation.Owned[2]];
        automation.Complete(PluginVendorTransactionKind.Sell, true);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.False(controller.Tick(0.1d, canAct: true));
        Assert.False(controller.IsRunning);
        Assert.Contains("finished", controller.Status, StringComparison.OrdinalIgnoreCase);
    }

    /// <summary>
    /// Test mode says what a run would do and touches nothing.
    /// </summary>
    [Fact]
    public void TestModeReportsAndDoesNotTransact()
    {
        FakeAutomation automation = ShopWithRations();
        automation.Owned = [Owned(Sword, "Sword", 40, 1, itemType: 0x1u), Coin(100)];
        (VendorTradeController controller, MemoryStorage storage) = Controller(
            automation,
            new VendorTradeSettings { TestMode = () => true });
        WriteProfile(storage, "mosstank/ub/autovendor/Shopkeeper.utl");

        automation.Open(Shopkeeper);
        Assert.False(controller.Tick(0.1d, canAct: true));
        Assert.False(controller.IsRunning);
        Assert.Empty(automation.BuyAllCalls);
        Assert.Empty(automation.SellAllCalls);
        Assert.Empty(automation.Staged);
        Assert.Contains(automation.Messages, m => m.Contains("Ration", StringComparison.Ordinal));
        Assert.Contains(automation.Messages, m => m.Contains("Sword", StringComparison.Ordinal));
    }

    /// <summary>
    /// The profile lookup is tiered: the character's own folder wins over the
    /// server's, which wins over the plugin's, and a vendor with no file of
    /// its own falls back to default.utl the same three ways. With no file
    /// at all the run does not start and says which file it looked for.
    /// </summary>
    [Fact]
    public void ProfileIsResolvedByTierAndAMissingOneRefusesToStart()
    {
        FakeAutomation automation = ShopWithRations();
        automation.Owned = [Coin(100)];
        (VendorTradeController controller, MemoryStorage storage) = Controller(automation);

        automation.Open(Shopkeeper);
        Assert.False(controller.Tick(0.1d, canAct: true));
        Assert.False(controller.IsRunning);
        Assert.Contains("Shopkeeper.utl", controller.Status, StringComparison.Ordinal);

        // A default in the character's folder is found for any vendor.
        WriteProfile(storage, "mosstank/ub/Coldeve/Acdream/autovendor/default.utl");
        Assert.True(controller.TryStart(null));
        Assert.True(controller.IsRunning);
        controller.Reset();

        // A named profile on the command line resolves the same way.
        WriteProfile(storage, "mosstank/ub/autovendor/recomp.utl");
        Assert.True(controller.TryStart("recomp"));
        Assert.True(controller.IsRunning);
    }

    /// <summary>
    /// What is never sold whatever a rule says: coin, what is worn or
    /// wielded, a pack, an inscribed or tinkered item, an attuned or retained
    /// one, and anything the macro profile itself uses. Mutation: dropping
    /// the profile-item check sells the character's own wand.
    /// </summary>
    [Fact]
    public void ProtectedItemsAreNeverStagedForSale()
    {
        FakeAutomation automation = ShopWithRations(listRations: false);
        PluginInventoryItem wand = Owned(21u, "Sword Wand", 40, 1, itemType: 0x1u);
        PluginInventoryItem worn = Owned(22u, "Sword Worn", 40, 1, itemType: 0x1u) with
        {
            EquippedLocation = 1u,
        };
        PluginInventoryItem inscribed = Owned(23u, "Sword Inscribed", 40, 1, itemType: 0x1u);
        PluginInventoryItem tinkered = Owned(24u, "Sword Tinkered", 40, 1, itemType: 0x1u) with
        {
            NumTimesTinkered = 1,
        };
        PluginInventoryItem attuned = Owned(25u, "Sword Attuned", 40, 1, itemType: 0x1u);
        PluginInventoryItem retained = Owned(26u, "Sword Retained", 40, 1, itemType: 0x1u);
        PluginInventoryItem plain = Owned(27u, "Sword Plain", 40, 1, itemType: 0x1u);
        automation.Owned = [wand, worn, inscribed, tinkered, attuned, retained, plain, Coin(10)];
        automation.Properties[inscribed.ObjectId] = Props(strings: new() { [7u] = "keep" });
        automation.Properties[attuned.ObjectId] = Props(ints: new() { [114u] = 1 });
        automation.Properties[retained.ObjectId] = Props(bools: new() { [88u] = true });
        (VendorTradeController controller, MemoryStorage storage) = Controller(automation);
        controller.BindProtectedItems(() => new HashSet<uint> { wand.ObjectId });
        WriteProfile(storage, "mosstank/ub/autovendor/Shopkeeper.utl");

        automation.Open(Shopkeeper);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([plain.ObjectId], automation.SellAllCalls.Single());
    }

    /// <summary>
    /// The vendor commands: staging by name, whole or partial, with a count;
    /// the two commits; the two clears; and an open that stands down while
    /// the route owns vendor opening.
    /// </summary>
    [Fact]
    public void VendorCommandsStageCommitClearAndOpen()
    {
        FakeAutomation automation = ShopWithRations();
        automation.Owned = [Owned(Sword, "Sword", 40, 1, itemType: 0x1u), Coin(10)];
        (VendorTradeController controller, _) = Controller(automation);
        bool routeOwns = false;
        controller.BindRouteOwnsVendorOpen(() => routeOwns);

        Assert.Contains("No vendor open", controller.VendorCommand("addbuy 5 Ration")[0], StringComparison.Ordinal);

        automation.Open(Shopkeeper);
        controller.VendorCommand("addbuy 5 Ration");
        Assert.Equal([(RationListing, 5)], automation.Staged);
        controller.VendorCommand("addbuyp 2 rat");
        Assert.Equal([(RationListing, 7)], automation.Staged);
        Assert.Contains("Unable to find", controller.VendorCommand("addbuy Rock")[0], StringComparison.Ordinal);

        controller.VendorCommand("addsellp swo");
        Assert.Equal([Sword], automation.StagedSells);
        controller.VendorCommand("clearsell");
        Assert.Empty(automation.StagedSells);
        controller.VendorCommand("clearbuy");
        Assert.Empty(automation.Staged);

        controller.VendorCommand("addsell Sword");
        controller.VendorCommand("sellall");
        Assert.Single(automation.SellAllCalls);
        controller.VendorCommand("addbuy Ration");
        controller.VendorCommand("buyall");
        Assert.Single(automation.BuyAllCalls);

        automation.Close();
        automation.WorldObjects =
        [
            new PluginWorldObject(Shopkeeper, 0u, "Shopkeeper", PluginObjectClass.Vendor, 0u, 0u, 0u),
        ];
        routeOwns = true;
        Assert.Contains("route", controller.VendorCommand("open Shopkeeper")[0], StringComparison.OrdinalIgnoreCase);
        Assert.Empty(automation.Uses);

        routeOwns = false;
        controller.VendorCommand("openp shop");
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([Shopkeeper], automation.Uses);
        // No retry until the tries interval has passed.
        Assert.True(controller.Tick(1d, canAct: true));
        Assert.Single(automation.Uses);
        Assert.True(controller.Tick(5d, canAct: true));
        Assert.Equal(2, automation.Uses.Count);
        controller.VendorCommand("opencancel");
        Assert.False(controller.Tick(5d, canAct: true));
        Assert.Equal(2, automation.Uses.Count);
    }

    /// <summary>
    /// An open-by-name begun before the macro starts is dropped when it
    /// does: the route owns vendor opening from then on, and the refusal at
    /// begin alone would leave the earlier use going out beside the route.
    /// Mutation: skip the clear and the second use is sent after the start.
    /// </summary>
    [Fact]
    public void AMacroStartDropsAnOpenBegunBeforeIt()
    {
        FakeAutomation automation = ShopWithRations();
        automation.Owned = [Coin(10)];
        (VendorTradeController controller, _) = Controller(automation);
        automation.WorldObjects =
        [
            new PluginWorldObject(Shopkeeper, 0u, "Shopkeeper", PluginObjectClass.Vendor, 0u, 0u, 0u),
        ];

        controller.VendorCommand("open Shopkeeper");
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([Shopkeeper], automation.Uses);

        controller.OnMacroStarted();
        Assert.False(controller.Tick(5d, canAct: true));
        Assert.False(controller.Tick(5d, canAct: true));
        Assert.Single(automation.Uses);
    }

    /// <summary>
    /// A run that gets no answer for a minute gives up, and a shop that
    /// closes under it ends it at once.
    /// </summary>
    [Fact]
    public void ARunBailsOnSilenceAndStopsWhenTheShopCloses()
    {
        FakeAutomation automation = ShopWithRations();
        automation.Owned = [Coin(100)];
        (VendorTradeController controller, MemoryStorage storage) = Controller(automation);
        WriteProfile(storage, "mosstank/ub/autovendor/Shopkeeper.utl");

        automation.Open(Shopkeeper);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Single(automation.BuyAllCalls);
        Assert.True(controller.Tick(30d, canAct: true));
        Assert.False(controller.Tick(31d, canAct: true));
        Assert.False(controller.IsRunning);
        Assert.Contains("timeout", controller.Status, StringComparison.OrdinalIgnoreCase);

        automation.Close();
        automation.Open(Shopkeeper);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.True(controller.IsRunning);
        automation.Close();
        Assert.False(controller.Tick(0.1d, canAct: true));
        Assert.False(controller.IsRunning);
    }

    /// <summary>
    /// A running visit holds the navigation and item-use locks for as long
    /// as it runs, re-armed every tick, and lets go when it ends. Without
    /// that, only the waypoint's own hold parks the character, and its
    /// thirty-second cap walks a visit that needs several settle rounds off
    /// mid-visit. Mutation: arming once at start instead of every tick
    /// expires under a long visit; forgetting the release leaves the route
    /// parked after the shop closes.
    /// </summary>
    [Fact]
    public void ARunHoldsTheNavigationLockUntilItEnds()
    {
        FakeAutomation automation = ShopWithRations();
        automation.Owned = [Coin(100)];
        var locks = new ActionLockTable();
        (VendorTradeController controller, MemoryStorage storage) = Controller(automation, locks: locks);
        WriteProfile(storage, "mosstank/ub/autovendor/Shopkeeper.utl");

        automation.Open(Shopkeeper);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.True(controller.IsRunning);
        Assert.True(locks.IsLocked(ActionLockKind.Navigation));
        Assert.True(locks.IsLocked(ActionLockKind.ItemUse));

        // Longer than one arming: the run re-arms while it lives.
        for (int i = 0; i < 4; i++)
        {
            locks.AdvanceTo(10d * (i + 1));
            Assert.True(controller.Tick(10d, canAct: false));
            Assert.True(locks.IsLocked(ActionLockKind.Navigation));
        }

        automation.Close();
        Assert.False(controller.Tick(0.1d, canAct: true));
        Assert.False(controller.IsRunning);
        Assert.False(locks.IsLocked(ActionLockKind.Navigation));
        Assert.False(locks.IsLocked(ActionLockKind.ItemUse));
    }

    private static (VendorTradeController, MemoryStorage) Controller(
        FakeAutomation automation,
        VendorTradeSettings? settings = null,
        ActionLockTable? locks = null)
    {
        var storage = new MemoryStorage();
        var host = new FakeHost(automation, storage);
        var store = new UbSettingStore(storage, NullUbSettingLog.Instance)
            .Bind("Coldeve", "Acdream");
        var controller = new VendorTradeController(host, store);
        controller.BindSettings(settings ?? new VendorTradeSettings());
        controller.BindStackCramPass((_, _) => false);
        if (locks is not null)
            controller.BindActionLocks(locks);
        return (controller, storage);
    }

    /// <summary>Keep up to ten rations; sell every sword; leave the rest.</summary>
    private static void WriteProfile(MemoryStorage storage, string key) =>
        storage.WriteText(key, MossTankLootProfileStore.SerializeRules(
        [
            new LootRule { Name = "rations", Expression = "name ~= ration", Action = LootAction.KeepUpTo, KeepCount = 10 },
            new LootRule { Name = "swords", Expression = "name ~= sword", Action = LootAction.Sell },
            new LootRule { Name = "rest", Expression = "*", Action = LootAction.NoLoot },
        ]));

    private static FakeAutomation ShopWithRations(bool listRations = true) => new()
    {
        Listings = listRations
            ? [new PluginVendorItem(RationListing, 5000u, "Ration", PluginObjectClass.Food, 5, 1) { MaxStackSize = 25, ItemType = 0x20u }]
            : [],
        Profile = new PluginVendorProfile(
            0.75f, 0x1u | 0x20u, PluginVendorProfile.NoValueLimit,
            PluginVendorProfile.NoValueLimit, true, 0u, 0u, null!),
    };

    private static PluginInventoryItem Coin(int amount) =>
        Owned(Coins, "Pyreal", amount, amount, itemType: 0x10u) with
        {
            WeenieClassId = 273u,
            ObjectClass = PluginObjectClass.Money,
            MaximumStackSize = 25000,
        };

    private static PluginInventoryItem Owned(
        uint id,
        string name,
        int value,
        int stack,
        uint itemType) =>
        new(id, id + 2000u, name, itemType, Player, 0u, 0u, 0u, 0u, 0u, 0u,
            stack, 0, 0, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0d, 0, 0, 0)
        {
            Value = value,
            MaximumStackSize = Math.Max(1, stack),
            ObjectClass = PluginObjectClass.MeleeWeapon,
        };

    private static PluginItemProperties Props(
        Dictionary<uint, int>? ints = null,
        Dictionary<uint, bool>? bools = null,
        Dictionary<uint, string>? strings = null) => new(
            ints ?? [],
            new Dictionary<uint, long>(),
            bools ?? [],
            new Dictionary<uint, double>(),
            strings ?? [],
            new Dictionary<uint, uint>(),
            new Dictionary<uint, uint>());

    private sealed class FakeAutomation
        : IAutomationSurface, ICharacterInfo, IItemAutomation,
          IWorldObjectAutomation, IVendorAutomation, IPluginChat
    {
        public bool IsAvailable => true;
        public ICharacterInfo Character => this;
        public IItemAutomation Items => this;
        public IWorldObjectAutomation Objects => this;
        public IVendorAutomation Vendor => this;
        public IPluginChat Chat => this;

        public List<string> Messages { get; } = [];
        public IReadOnlyList<PluginInventoryItem> Owned { get; set; } = [];
        public IReadOnlyList<PluginWorldObject> WorldObjects { get; set; } = [];
        public ISpellCatalog Spells => NoOpAutomationSurface.Instance;
        public IMagicCommands Magic => NoOpAutomationSurface.Instance;
        public Dictionary<uint, PluginItemProperties> Properties { get; } = [];
        public IReadOnlyList<PluginVendorItem> Listings { get; set; } = [];
        public PluginVendorProfile ShopProfile { get; set; }
        public List<(uint Template, int Count)> Staged { get; } = [];
        public List<uint> StagedSells { get; } = [];
        public List<(uint Template, int Count)[]> BuyAllCalls { get; } = [];
        public List<uint[]> SellAllCalls { get; } = [];
        public List<uint> Uses { get; } = [];

        public PluginVendorProfile Profile
        {
            get => IsOpen ? ShopProfile : PluginVendorProfile.Unset;
            set => ShopProfile = value;
        }

        // ICharacterInfo
        public bool IsInWorld => true;
        public string Name => "Acdream";
        public string WorldName => "Coldeve";
        public uint ObjectId => Player;
        public int MainPackFreeSlots => 10;
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

        // IItemAutomation
        bool IItemAutomation.IsAvailable => true;
        bool IItemAutomation.IsBusy => false;
        public uint ActiveVendorObjectId { get; private set; }
        public PluginInventoryCompletion LastInventoryCompletion { get; set; }
        public IReadOnlyList<PluginInventoryItem> CaptureOwnedItems() => Owned;
        public bool TryCaptureProperties(uint objectId, out PluginItemProperties properties)
        {
            if (Properties.TryGetValue(objectId, out PluginItemProperties found))
            {
                properties = found;
                return true;
            }
            properties = Props();
            return true;
        }
        public PluginItemCommandResult Use(uint objectId)
        {
            Uses.Add(objectId);
            return new PluginItemCommandResult(PluginItemCommandStatus.Started);
        }

        // IWorldObjectAutomation
        bool IWorldObjectAutomation.IsAvailable => true;
        public IReadOnlyList<PluginWorldObject> CaptureObjects() => WorldObjects;
        public bool TryGet(uint objectId, out PluginWorldObject value)
        {
            foreach (PluginWorldObject candidate in WorldObjects)
            {
                if (candidate.ObjectId == objectId)
                {
                    value = candidate;
                    return true;
                }
            }
            value = default;
            return false;
        }

        // IVendorAutomation
        bool IVendorAutomation.IsAvailable => true;
        public bool IsOpen => ActiveVendorObjectId != 0u;
        public uint VendorObjectId => ActiveVendorObjectId;
        public string VendorName => IsOpen ? "Shopkeeper" : string.Empty;
        IReadOnlyList<PluginVendorItem> IVendorAutomation.Items => IsOpen ? Listings : [];
        bool IVendorAutomation.IsBusy => false;
        public IReadOnlyList<(uint, int)> BuyList => Staged;
        public IReadOnlyList<uint> SellList => StagedSells;
        public event Action<uint>? Opened;
        public event Action? Closed;
        public event Action<PluginVendorTransaction>? TransactionCompleted;

        bool IVendorAutomation.TryCaptureProperties(uint templateObjectId, out PluginItemProperties properties) =>
            TryCaptureProperties(templateObjectId, out properties);

        public PluginVendorCommandResult AddToBuyList(uint templateObjectId, int count)
        {
            int index = Staged.FindIndex(s => s.Template == templateObjectId);
            if (index >= 0)
                Staged[index] = (templateObjectId, Staged[index].Count + count);
            else
                Staged.Add((templateObjectId, count));
            return new PluginVendorCommandResult(PluginVendorCommandStatus.Sent);
        }
        public PluginVendorCommandResult AddToSellList(uint itemObjectId)
        {
            StagedSells.Add(itemObjectId);
            return new PluginVendorCommandResult(PluginVendorCommandStatus.Sent);
        }
        public PluginVendorCommandResult RemoveFromBuyList(uint templateObjectId)
        {
            Staged.RemoveAll(s => s.Template == templateObjectId);
            return new PluginVendorCommandResult(PluginVendorCommandStatus.Sent);
        }
        public PluginVendorCommandResult RemoveFromSellList(uint itemObjectId)
        {
            StagedSells.Remove(itemObjectId);
            return new PluginVendorCommandResult(PluginVendorCommandStatus.Sent);
        }
        public PluginVendorCommandResult ClearBuyList()
        {
            Staged.Clear();
            return new PluginVendorCommandResult(PluginVendorCommandStatus.Sent);
        }
        public PluginVendorCommandResult ClearSellList()
        {
            StagedSells.Clear();
            return new PluginVendorCommandResult(PluginVendorCommandStatus.Sent);
        }
        public PluginVendorCommandResult BuyAll()
        {
            if (Staged.Count == 0)
                return new PluginVendorCommandResult(PluginVendorCommandStatus.InvalidItem);
            BuyAllCalls.Add([.. Staged]);
            Staged.Clear();
            return new PluginVendorCommandResult(PluginVendorCommandStatus.Sent);
        }
        public PluginVendorCommandResult SellAll()
        {
            if (StagedSells.Count == 0)
                return new PluginVendorCommandResult(PluginVendorCommandStatus.InvalidItem);
            SellAllCalls.Add([.. StagedSells]);
            StagedSells.Clear();
            return new PluginVendorCommandResult(PluginVendorCommandStatus.Sent);
        }

        public void Open(uint vendorObjectId)
        {
            ActiveVendorObjectId = vendorObjectId;
            Opened?.Invoke(vendorObjectId);
        }

        public void Close()
        {
            ActiveVendorObjectId = 0u;
            Staged.Clear();
            StagedSells.Clear();
            Closed?.Invoke();
        }

        public void Complete(PluginVendorTransactionKind kind, bool success) =>
            TransactionCompleted?.Invoke(new PluginVendorTransaction(kind, success, null!));

        // IPluginChat
        public void PostSystemMessage(string text) => Messages.Add(text);
        public bool Submit(string text)
        {
            Messages.Add(text);
            return true;
        }
    }

    private sealed class FakeHost(FakeAutomation automation, IPluginStorage storage) : IPluginHost
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
        private readonly Dictionary<string, string> _text = new(StringComparer.Ordinal);
        public bool IsAvailable => true;
        public string? ReadText(string key) => _text.TryGetValue(key, out string? value) ? value : null;
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
        public event Action<WorldEntitySnapshot> EntitySpawned { add { } remove { } }
        public event Action<double> Tick { add { } remove { } }
    }

    private sealed class FakeSelection : ISelectionService
    {
        public uint? SelectedObjectId => null;
        public uint? PreviousObjectId => null;
        public event Action<SelectionChangedEvent> Changed { add { } remove { } }
        public bool Select(uint objectId) => false;
        public bool Clear() => false;
    }
}

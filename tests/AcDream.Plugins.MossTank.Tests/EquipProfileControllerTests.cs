using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// Equipment profiles: a loot profile under <c>ub/equip/</c> names a set of
/// gear, and the controller takes everything else off and puts that set on,
/// lore-gated pieces last. It is a prologue owner: it refuses while the
/// combat controller has a monster to dress for, and it holds the item-use
/// lock for its whole run.
/// </summary>
public sealed class EquipProfileControllerTests
{
    private const uint Player = 1u;
    private const uint Sword = 10u;
    private const uint Bracelet = 11u;
    private const uint Hauberk = 12u;
    private const uint Ration = 13u;

    private const uint BraceletSlot = 0x00040000u;
    private const uint HauberkSlot = 0x00000004u;
    private const uint WeaponSlot = 0x01000000u;
    private const uint LoreRequirementProperty = 109u;

    /// <summary>
    /// Two owners for equipment placement is a known failure class here, so
    /// a load is refused outright while the combat controller has a target,
    /// and it says why. Mutation: dropping the check starts the run.
    /// </summary>
    [Fact]
    public void LoadIsRefusedWhileTheCombatControllerHasATarget()
    {
        var automation = new FakeAutomation();
        automation.Owned = [Wearable(Bracelet, "Set A Bracelet", BraceletSlot)];
        (EquipProfileController controller, MemoryStorage storage, _) =
            Controller(automation, combatHasTarget: () => true);
        WriteSetAProfile(storage, "mosstank/ub/equip/suit.utl");

        IReadOnlyList<string> lines = controller.Command("load suit")!;

        Assert.False(controller.IsRunning);
        Assert.Contains("monster", lines[0], StringComparison.OrdinalIgnoreCase);
        Assert.False(controller.Tick(0.1d, canAct: true));
        Assert.Empty(automation.EquipCalls);
        Assert.Empty(automation.MoveCalls);
    }

    /// <summary>
    /// With EquipmentManager.Think on, the finished line is a real tell to
    /// the character's own name, which the server answers with its own
    /// "You think" line; nothing is printed locally in its place.
    /// Mutation: printing a local "You think" system line sends no tell.
    /// </summary>
    [Fact]
    public void TheFinishedLineIsThoughtAsARealTell()
    {
        var automation = new FakeAutomation();
        (EquipProfileController controller, MemoryStorage storage, _) = Controller(automation);
        controller.BindSettings(new EquipProfileSettings { Think = static () => true });
        WriteSetAProfile(storage, "mosstank/ub/equip/suit.utl");

        Assert.Empty(controller.Command("load suit")!);
        for (int frame = 0; frame < 10 && controller.IsRunning; frame++)
            controller.Tick(0.1d, canAct: true);

        Assert.False(controller.IsRunning);
        Assert.Contains(
            automation.Messages,
            static line => line.StartsWith(
                "/t Acdream, Equipment Manager: Finished equipping items in ",
                StringComparison.Ordinal));
        Assert.DoesNotContain(
            automation.Messages,
            static line => line.StartsWith("You think", StringComparison.Ordinal));
    }

    /// <summary>
    /// How a refused start is said is decided where it is refused, not read
    /// back from the status wording: a missing profile only in the debug
    /// output, an unreadable one as the tool's error. Mutation: saying the
    /// missing profile as an error, or the unreadable one only in debug,
    /// fails one of the two.
    /// </summary>
    [Fact]
    public void EachRefusalIsSaidTheWayItsCauseIs()
    {
        var automation = new FakeAutomation();
        (EquipProfileController controller, MemoryStorage storage, _) = Controller(automation);

        Assert.Empty(controller.Command("load suit")!);

        storage.WriteText("mosstank/ub/equip/broken.utl", "not a loot profile");
        IReadOnlyList<string> lines = controller.Command("load broken")!;
        Assert.Equal(
            "[UB] Error: EquipmentManager: Equip profile could not be read: mosstank/ub/equip/broken.utl",
            Assert.Single(lines));
    }

    /// <summary>
    /// The finished line writes its seconds as the reference writes a
    /// number, with no fixed decimal places: a two-second run is "2s".
    /// Mutation: the old one-decimal format writes "2.0s".
    /// </summary>
    [Fact]
    public void TheFinishedLineWritesItsSecondsAsTheReferenceDoes()
    {
        var automation = new FakeAutomation();
        (EquipProfileController controller, MemoryStorage storage, _) = Controller(automation);
        WriteSetAProfile(storage, "mosstank/ub/equip/suit.utl");

        Assert.Empty(controller.Command("load suit")!);
        for (int frame = 0; frame < 10 && controller.IsRunning; frame++)
            controller.Tick(1d, canAct: true);

        string line = Assert.Single(
            automation.Messages,
            static text => text.Contains("Finished equipping", StringComparison.Ordinal));
        Assert.Matches(@"^\[UB\] Equipment Manager: Finished equipping items in [1-9]\d*s$", line);
    }

    /// <summary>
    /// The whole load: the piece that is not in the profile comes off by a
    /// move to the player's own pack, then the profile's pieces go on in
    /// lore order with the placement receipt gating each step, and the
    /// item-use lock is held throughout and let go at the end. The bracelet
    /// is the lore-gated piece and sorts first by name, so a name sort puts
    /// it on first and a lore sort puts it on last. Mutation: sorting by
    /// name; not waiting for the receipt sends the second equip while the
    /// first is in flight.
    /// </summary>
    [Fact]
    public void LoadDequipsThenEquipsInLoreOrderUnderTheItemLock()
    {
        var automation = new FakeAutomation();
        automation.Owned =
        [
            Wearable(Sword, "Sword", WeaponSlot, equipped: WeaponSlot),
            Wearable(Hauberk, "Set A Hauberk", HauberkSlot),
            Wearable(Bracelet, "Set A Bracelet", BraceletSlot),
            Consumable(Ration, "Set A Ration"),
        ];
        automation.Properties[Hauberk] = Props(new() { [LoreRequirementProperty] = 0 });
        automation.Properties[Bracelet] = Props(new() { [LoreRequirementProperty] = 50 });
        automation.Appraised.UnionWith([Sword, Hauberk, Bracelet]);
        (EquipProfileController controller, MemoryStorage storage, ActionLockTable locks) =
            Controller(automation);
        WriteSetAProfile(storage, "mosstank/ub/equip/suit.utl");

        Assert.Empty(controller.Command("load suit")!);
        Assert.True(controller.IsRunning);

        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.True(locks.IsLocked(ActionLockKind.ItemUse));
        Assert.Equal([(Sword, Player)], automation.MoveCalls);
        Assert.Empty(automation.EquipCalls);

        // Nothing more until the sword is seen leaving its slot.
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Single(automation.MoveCalls);
        automation.ObserveRemoval(Sword);

        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([(Hauberk, 0u)], automation.EquipCalls);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Single(automation.EquipCalls);
        automation.ObservePlacement(Hauberk, HauberkSlot);

        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([(Hauberk, 0u), (Bracelet, 0u)], automation.EquipCalls);
        automation.ObservePlacement(Bracelet, BraceletSlot);

        Assert.False(controller.Tick(0.1d, canAct: true));
        Assert.False(controller.IsRunning);
        Assert.False(locks.IsLocked(ActionLockKind.ItemUse));
        Assert.Contains(automation.Messages, m => m.Contains("Finished equipping", StringComparison.Ordinal));
        // The ration matched by name but cannot be worn; it is never sent.
        Assert.DoesNotContain(automation.EquipCalls, call => call.Item == Ration);
    }

    /// <summary>
    /// A rule that needs appraisal data asks for it first, one item at a
    /// time, and decides only once the answer is in. There is no batch
    /// call, so the controller tracks its own outstanding request.
    /// Mutation: deciding on unappraised data skips the hauberk.
    /// </summary>
    [Fact]
    public void ItemsTheRulesCannotDecideAreIdentifiedFirst()
    {
        var automation = new FakeAutomation();
        automation.Owned = [Wearable(Hauberk, "Hauberk", HauberkSlot)];
        (EquipProfileController controller, MemoryStorage storage, _) = Controller(automation);
        // Armor level 300 or better: an appraisal-only key.
        storage.WriteText("mosstank/ub/equip/suit.utl", MossTankLootProfileStore.SerializeRules(
        [
            new LootRule
            {
                Name = "good armor",
                Action = LootAction.Keep,
                HasImportedRequirements = true,
                VtankRequirements = [new VtankLootRequirement { Type = 3, Payload = "300\r\n28\r\n" }],
            },
        ]));

        controller.Command("load suit");
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([Hauberk], automation.IdentifyCalls);
        Assert.Empty(automation.EquipCalls);

        Assert.True(controller.Tick(1d, canAct: true));
        Assert.Single(automation.IdentifyCalls);

        automation.Properties[Hauberk] = Props(new() { [28u] = 320 });
        automation.Appraised.Add(Hauberk);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([(Hauberk, 0u)], automation.EquipCalls);
    }

    /// <summary>
    /// Two rings in one profile: each equip asks for no particular slot and
    /// lets the client choose, so the second ring takes the free finger.
    /// A non-zero mask is forwarded verbatim and the server widens it only
    /// for armour and clothing, so asking for the ring's whole mask claims
    /// both fingers on the first ring and the second is refused. Mutation:
    /// passing the item's valid-locations mask.
    /// </summary>
    [Fact]
    public void TwoRingsAreEquippedWithNoSlotRequestSoTheClientPicksTheFreeFinger()
    {
        const uint LeftRing = 14u;
        const uint RightRing = 15u;
        const uint BothFingers = 0x000C0000u;
        var automation = new FakeAutomation();
        automation.Owned =
        [
            Wearable(LeftRing, "Set A Ring of Dawn", BothFingers),
            Wearable(RightRing, "Set A Ring of Dusk", BothFingers),
        ];
        automation.Appraised.UnionWith([LeftRing, RightRing]);
        (EquipProfileController controller, MemoryStorage storage, _) = Controller(automation);
        WriteSetAProfile(storage, "mosstank/ub/equip/rings.utl");

        Assert.Empty(controller.Command("load rings")!);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Single(automation.EquipCalls);
        Assert.Equal(0u, automation.EquipCalls[0].Location);
        automation.ObservePlacement(automation.EquipCalls[0].Item, 0x00040000u);

        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(2, automation.EquipCalls.Count);
        Assert.Equal(0u, automation.EquipCalls[1].Location);
        Assert.NotEqual(automation.EquipCalls[0].Item, automation.EquipCalls[1].Item);
        automation.ObservePlacement(automation.EquipCalls[1].Item, 0x00080000u);

        Assert.False(controller.Tick(0.1d, canAct: true));
        Assert.False(controller.IsRunning);
    }

    /// <summary>Test mode says the order and touches nothing.</summary>
    [Fact]
    public void TestReportsTheOrderAndTouchesNothing()
    {
        var automation = new FakeAutomation();
        automation.Owned =
        [
            Wearable(Sword, "Sword", WeaponSlot, equipped: WeaponSlot),
            Wearable(Hauberk, "Set A Hauberk", HauberkSlot),
            Wearable(Bracelet, "Set A Bracelet", BraceletSlot),
        ];
        automation.Properties[Bracelet] = Props(new() { [LoreRequirementProperty] = 50 });
        automation.Appraised.UnionWith([Sword, Hauberk, Bracelet]);
        (EquipProfileController controller, MemoryStorage storage, ActionLockTable locks) =
            Controller(automation);
        WriteSetAProfile(storage, "mosstank/ub/equip/suit.utl");

        controller.Command("test suit");
        Assert.False(controller.Tick(0.1d, canAct: true));
        Assert.False(controller.IsRunning);
        Assert.Empty(automation.EquipCalls);
        Assert.Empty(automation.MoveCalls);
        Assert.False(locks.IsLocked(ActionLockKind.ItemUse));
        int bracelet = automation.Messages.FindIndex(m => m.Contains("Set A Bracelet", StringComparison.Ordinal));
        int hauberk = automation.Messages.FindIndex(m => m.Contains("Set A Hauberk", StringComparison.Ordinal));
        Assert.True(hauberk >= 0 && bracelet > hauberk, string.Join(Environment.NewLine, automation.Messages));
    }

    /// <summary>
    /// Create writes what is worn as a loot profile in the character's own
    /// folder, and never over an existing file without being asked twice:
    /// a live profile was overwritten once. The written profile reads back
    /// and matches the worn pieces by name and slot. Mutation: dropping the
    /// confirmation overwrites on the second call.
    /// </summary>
    [Fact]
    public void CreateWritesTheWornSetAndConfirmsBeforeOverwriting()
    {
        var automation = new FakeAutomation();
        automation.Owned =
        [
            Wearable(Sword, "Sword", WeaponSlot, equipped: WeaponSlot),
            Wearable(Bracelet, "Set A Bracelet", BraceletSlot, equipped: BraceletSlot),
            Wearable(Hauberk, "Set A Hauberk", HauberkSlot),
        ];
        (EquipProfileController controller, MemoryStorage storage, _) = Controller(automation);
        const string key = "mosstank/ub/Coldeve/Acdream/equip/mine.utl";

        Assert.Contains("created", controller.Command("create mine")![0], StringComparison.OrdinalIgnoreCase);
        string first = Assert.IsType<string>(storage.ReadText(key));
        var rules = new List<LootRule>();
        Assert.True(MossTankLootProfileStore.TryParseRules(first, rules));
        Assert.Equal(2, rules.Count);
        Assert.NotNull(LootRuleEngine.Decide(automation.Owned[0], Props(), rules, automation.Owned));
        Assert.NotNull(LootRuleEngine.Decide(automation.Owned[1], Props(), rules, automation.Owned));
        Assert.Null(LootRuleEngine.Decide(automation.Owned[2], Props(), rules, automation.Owned));

        // Now the hauberk is worn instead; the file must not change yet.
        automation.Owned =
        [
            Wearable(Hauberk, "Set A Hauberk", HauberkSlot, equipped: HauberkSlot),
        ];
        IReadOnlyList<string> second = controller.Command("create mine")!;
        Assert.Contains("already exists", second[0], StringComparison.OrdinalIgnoreCase);
        Assert.Equal(first, storage.ReadText(key));

        controller.Tick(1d, canAct: true);
        Assert.Contains("created", controller.Command("create mine")![0], StringComparison.OrdinalIgnoreCase);
        Assert.NotEqual(first, storage.ReadText(key));
        rules.Clear();
        Assert.True(MossTankLootProfileStore.TryParseRules(storage.ReadText(key), rules));
        Assert.Single(rules);
    }

    /// <summary>
    /// The confirmation to overwrite is short-lived: after half a minute a
    /// create is a fresh question again.
    /// </summary>
    [Fact]
    public void AnOverwriteConfirmationExpires()
    {
        var automation = new FakeAutomation();
        automation.Owned = [Wearable(Sword, "Sword", WeaponSlot, equipped: WeaponSlot)];
        (EquipProfileController controller, MemoryStorage storage, _) = Controller(automation);
        const string key = "mosstank/ub/Coldeve/Acdream/equip/mine.utl";

        controller.Command("create mine");
        string first = storage.ReadText(key)!;
        controller.Command("create mine");
        controller.Tick(31d, canAct: true);
        Assert.Contains("already exists", controller.Command("create mine")![0], StringComparison.OrdinalIgnoreCase);
        Assert.Equal(first, storage.ReadText(key));
    }

    /// <summary>
    /// The profile lookup is the six-step one: the character's folder,
    /// the server's, the installation's, then default.utl in each; and the
    /// list shows every file at every depth with where it was found.
    /// </summary>
    [Fact]
    public void ProfilesResolveByTierAndListShowsEveryDepth()
    {
        var automation = new FakeAutomation();
        automation.Owned = [Wearable(Bracelet, "Set A Bracelet", BraceletSlot)];
        automation.Appraised.Add(Bracelet);
        (EquipProfileController controller, MemoryStorage storage, _) = Controller(automation);

        // A missing profile is said only in the debug output, as the
        // reference says it; the refusal itself is the status.
        Assert.Empty(controller.Command("load suit")!);
        Assert.StartsWith("No equip profile", controller.Status, StringComparison.Ordinal);
        Assert.False(controller.IsRunning);

        WriteSetAProfile(storage, "mosstank/ub/Coldeve/equip/default.utl");
        Assert.Empty(controller.Command("test suit")!);
        Assert.True(controller.IsRunning);
        controller.Reset();

        WriteSetAProfile(storage, "mosstank/ub/Coldeve/Acdream/equip/suit.utl");
        WriteSetAProfile(storage, "mosstank/ub/equip/other.utl");
        IReadOnlyList<string> listing = controller.Command("list")!;
        Assert.Contains(listing, line => line.Contains("suit.utl", StringComparison.Ordinal)
            && line.Contains("Coldeve/Acdream", StringComparison.Ordinal));
        Assert.Contains(listing, line => line.Contains("default.utl", StringComparison.Ordinal));
        Assert.Contains(listing, line => line.Contains("other.utl", StringComparison.Ordinal));
    }

    /// <summary>A run with no progress for ten seconds gives up and lets go of the lock.</summary>
    [Fact]
    public void ARunWithNoProgressBailsAndReleasesTheLock()
    {
        var automation = new FakeAutomation();
        automation.Owned = [Wearable(Bracelet, "Set A Bracelet", BraceletSlot)];
        automation.Appraised.Add(Bracelet);
        (EquipProfileController controller, MemoryStorage storage, ActionLockTable locks) =
            Controller(automation);
        WriteSetAProfile(storage, "mosstank/ub/equip/suit.utl");

        controller.Command("load suit");
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Single(automation.EquipCalls);
        Assert.True(locks.IsLocked(ActionLockKind.ItemUse));

        Assert.True(controller.Tick(5d, canAct: true));
        Assert.False(controller.Tick(6d, canAct: true));
        Assert.False(controller.IsRunning);
        Assert.False(locks.IsLocked(ActionLockKind.ItemUse));
        Assert.Contains(automation.Messages, m => m.Contains("timeout", StringComparison.OrdinalIgnoreCase));
    }

    private static (EquipProfileController, MemoryStorage, ActionLockTable) Controller(
        FakeAutomation automation,
        Func<bool>? combatHasTarget = null)
    {
        var storage = new MemoryStorage();
        var host = new FakeHost(automation, storage);
        var store = new UbSettingStore(storage, NullUbSettingLog.Instance)
            .Bind("Coldeve", "Acdream");
        var locks = new ActionLockTable();
        var controller = new EquipProfileController(host, store, combatHasTarget ?? (static () => false));
        controller.BindActionLocks(locks);
        return (controller, storage, locks);
    }

    /// <summary>Keep everything named "Set A ..."; leave the rest.</summary>
    private static void WriteSetAProfile(MemoryStorage storage, string key) =>
        storage.WriteText(key, MossTankLootProfileStore.SerializeRules(
        [
            new LootRule { Name = "set a", Expression = "name ~= Set A", Action = LootAction.Keep },
            new LootRule { Name = "rest", Expression = "*", Action = LootAction.NoLoot },
        ]));

    private static PluginInventoryItem Wearable(
        uint id,
        string name,
        uint validLocations,
        uint equipped = 0u) =>
        new(id, id + 2000u, name, 0x2u, Player, equipped == 0u ? 0u : Player, validLocations,
            equipped, 0u, 0u, 0u, 1, 0, 0, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0d, 0, 0, 0)
        {
            Value = 100,
            MaximumStackSize = 1,
            ObjectClass = PluginObjectClass.Armor,
        };

    private static PluginInventoryItem Consumable(uint id, string name) =>
        new(id, id + 2000u, name, 0x20u, Player, 0u, 0u, 0u, 0u, 0u, 0u,
            1, 0, 0, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0d, 0, 0, 0)
        {
            Value = 5,
            MaximumStackSize = 25,
            ObjectClass = PluginObjectClass.Food,
        };

    private static PluginItemProperties Props(Dictionary<uint, int>? ints = null) => new(
        ints ?? [],
        new Dictionary<uint, long>(),
        new Dictionary<uint, bool>(),
        new Dictionary<uint, double>(),
        new Dictionary<uint, string>(),
        new Dictionary<uint, uint>(),
        new Dictionary<uint, uint>());

    private sealed class FakeAutomation
        : IAutomationSurface, ICharacterInfo, IItemAutomation, IEquipmentAutomation,
          IWorldObjectAutomation, IPluginChat
    {
        private Action<PluginEquipmentObservation>? _placementObserved;

        public bool IsAvailable => true;
        public ICharacterInfo Character => this;
        public IItemAutomation Items => this;
        public IEquipmentAutomation Equipment => this;
        public IWorldObjectAutomation Objects => this;
        public IPluginChat Chat => this;
        public ISpellCatalog Spells => NoOpAutomationSurface.Instance;
        public IMagicCommands Magic => NoOpAutomationSurface.Instance;

        public List<string> Messages { get; } = [];
        public List<PluginInventoryItem> Owned { get; set; } = [];
        public Dictionary<uint, PluginItemProperties> Properties { get; } = [];
        public HashSet<uint> Appraised { get; } = [];
        public List<(uint Item, uint Location)> EquipCalls { get; } = [];
        public List<(uint Item, uint Container)> MoveCalls { get; } = [];
        public List<uint> IdentifyCalls { get; } = [];

        // ICharacterInfo
        public bool IsInWorld => true;
        public string Name => "Acdream";
        public string WorldName => "Coldeve";
        public uint ObjectId => Player;
        public int MainPackFreeSlots => 10;
        public uint CurrentHealth => 100u;
        public uint MaxHealth => 100u;
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
        public IReadOnlyList<PluginInventoryItem> CaptureOwnedItems() => Owned;
        bool IItemAutomation.TryCaptureProperties(uint objectId, out PluginItemProperties properties) =>
            TryCaptureProperties(objectId, out properties);
        public PluginItemCommandResult MoveToContainer(
            uint objectId, uint containerObjectId, uint amount = 0u, int placement = 0)
        {
            MoveCalls.Add((objectId, containerObjectId));
            return new PluginItemCommandResult(PluginItemCommandStatus.Started);
        }

        // IEquipmentAutomation
        bool IEquipmentAutomation.IsAvailable => true;
        bool IEquipmentAutomation.IsBusy => false;
        public IReadOnlyList<PluginEquipmentItem> CaptureOwnedEquipment() => Owned
            .Where(static item => item.ValidLocations != 0u)
            .Select(static item => new PluginEquipmentItem(
                item.ObjectId, item.Name, item.ItemType, item.ValidLocations,
                item.EquippedLocation, item.ContainerObjectId, item.WielderObjectId,
                0, 0, 0, 0, 0d))
            .ToArray();
        public event Action<PluginEquipmentObservation> PlacementObserved
        {
            add => _placementObserved += value;
            remove => _placementObserved -= value;
        }
        public PluginEquipmentCommandResult Equip(uint objectId, uint requestedLocation = 0u)
        {
            EquipCalls.Add((objectId, requestedLocation));
            return new PluginEquipmentCommandResult(PluginEquipmentCommandStatus.Started);
        }

        // IWorldObjectAutomation
        bool IWorldObjectAutomation.IsAvailable => true;
        public bool TryGet(uint objectId, out PluginWorldObject value)
        {
            foreach (PluginInventoryItem item in Owned)
            {
                if (item.ObjectId == objectId)
                {
                    value = new PluginWorldObject(
                        item.ObjectId, item.WeenieClassId, item.Name, item.ObjectClass,
                        item.ItemType, item.ContainerObjectId, item.WielderObjectId)
                    {
                        HasAppraisalData = Appraised.Contains(objectId),
                        IsOwned = true,
                    };
                    return true;
                }
            }
            value = default;
            return false;
        }
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
        public PluginItemCommandResult Identify(uint objectId)
        {
            IdentifyCalls.Add(objectId);
            return new PluginItemCommandResult(PluginItemCommandStatus.Started);
        }

        // IPluginChat
        public void PostSystemMessage(string text) => Messages.Add(text);
        public bool Submit(string text)
        {
            Messages.Add(text);
            return true;
        }

        public void ObservePlacement(uint objectId, uint location)
        {
            Replace(objectId, location);
            _placementObserved?.Invoke(new PluginEquipmentObservation(objectId, location, false));
        }

        public void ObserveRemoval(uint objectId)
        {
            Replace(objectId, 0u);
            _placementObserved?.Invoke(new PluginEquipmentObservation(objectId, 0u, true));
        }

        private void Replace(uint objectId, uint location)
        {
            for (int i = 0; i < Owned.Count; i++)
            {
                if (Owned[i].ObjectId == objectId)
                {
                    Owned[i] = Owned[i] with
                    {
                        EquippedLocation = location,
                        WielderObjectId = location == 0u ? 0u : Player,
                    };
                }
            }
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
        public IReadOnlyList<string> List(string prefix) => _text.Keys
            .Where(key => key.StartsWith(prefix, StringComparison.Ordinal))
            .OrderBy(static key => key, StringComparer.Ordinal)
            .ToArray();
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

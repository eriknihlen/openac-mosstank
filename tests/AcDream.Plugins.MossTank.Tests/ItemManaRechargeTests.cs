using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class ItemManaRechargeTests
{
    [Fact]
    public void PlannerUsesProfiledManaChargeOnMostDepletedWornItem()
    {
        var names = new HashSet<string>(StringComparer.Ordinal)
        {
            "Mana Charge",
        };
        PluginInventoryItem planTarget = Item(20, "Low Wand") with
        {
            EquippedLocation = 0x01000000u,
            ItemCurrentMana = 10,
            ItemMaximumMana = 100,
        };
        ItemManaRechargePlan plan = Assert.IsType<ItemManaRechargePlan>(
            ItemManaRechargePlanner.Plan(
                [
                    Item(10, "Mana Charge", 0x00080000u) with
                    {
                        ItemCurrentMana = 100,
                        Effects = 0x00000001u,
                    },
                    planTarget,
                    Item(21, "Other Wand") with
                    {
                        EquippedLocation = 0x01000000u,
                        ItemCurrentMana = 20,
                        ItemMaximumMana = 100,
                    },
                ],
                names,
                thresholdPercent: 33));

        Assert.Equal(10u, plan.ChargeObjectId);
        Assert.Equal(20u, plan.TargetObjectId);
        Assert.Equal(10, plan.CurrentMana);
    }

    [Fact]
    public void TheOldestQueuedWornItemIsChargedFirstNotTheMostDepleted()
    {
        var names = new HashSet<string>(StringComparer.Ordinal) { "Mana Charge" };
        PluginInventoryItem[] inventory =
        [
            Item(10, "Mana Charge", 0x00080000u) with
            {
                ItemCurrentMana = 100,
                Effects = 0x00000001u,
            },
            Item(20, "Low Wand") with
            {
                EquippedLocation = 0x01000000u,
                ItemCurrentMana = 10,
                ItemMaximumMana = 100,
            },
            Item(21, "Older Wand") with
            {
                EquippedLocation = 0x02000000u,
                ItemCurrentMana = 20,
                ItemMaximumMana = 100,
            },
        ];

        ItemManaRechargePlan queued = Assert.IsType<ItemManaRechargePlan>(
            ItemManaRechargePlanner.Plan(
                inventory, names, thresholdPercent: 33, wieldOrder: [21u, 20u]));
        Assert.Equal(21u, queued.TargetObjectId);

        ItemManaRechargePlan depleted = Assert.IsType<ItemManaRechargePlan>(
            ItemManaRechargePlanner.Plan(inventory, names, thresholdPercent: 33));
        Assert.Equal(20u, depleted.TargetObjectId);
    }

    [Fact]
    public void PlannerRequiresProfileMembershipAndBelowThreshold()
    {
        PluginInventoryItem charge = Item(10, "Mana Charge", 0x00080000u) with
        {
            ItemCurrentMana = 100,
            Effects = 0x00000001u,
        };
        PluginInventoryItem wand = Item(20, "Wand") with
        {
            EquippedLocation = 0x01000000u,
            ItemCurrentMana = 34,
            ItemMaximumMana = 100,
        };

        Assert.Null(ItemManaRechargePlanner.Plan(
            [charge, wand],
            new HashSet<string>(StringComparer.Ordinal),
            33));
        Assert.Null(ItemManaRechargePlanner.Plan(
            [charge, wand],
            new HashSet<string>(StringComparer.Ordinal) { "Mana Charge" },
            33));
    }

    /// <summary>
    /// A worn-mana refill applies the charge to the player, because that is the
    /// command that distributes mana across equipped items. After a successful
    /// receipt, the old positive source appraisal must not schedule another
    /// action, even when a new appraisal stamp arrives while its omitted
    /// current-mana property remains merged in the local object.
    /// Mutation: target the selected threshold item again, or remove either the
    /// used-charge quarantine or the current-mana-change check, and this test
    /// observes an armor target or a second application.
    /// </summary>
    [Fact]
    public void SuccessfulPlayerRefillCannotReuseStalePositiveCharge()
    {
        PluginInventoryItem charge = Item(10, "Mana Charge", 0x00080000u) with
        {
            ItemCurrentMana = 100,
            Effects = 0x00000001u,
        };
        PluginInventoryItem missingCurrentMana = Item(19, "Unready Helm") with
        {
            EquippedLocation = 0x00000001u,
            ItemCurrentMana = 0,
            ItemMaximumMana = 100,
        };
        PluginInventoryItem thresholdItem = Item(20, "Low Gloves") with
        {
            EquippedLocation = 0x00000002u,
            ItemCurrentMana = 10,
            ItemMaximumMana = 100,
        };
        var surface = new Surface
        {
            Inventory = [charge, missingCurrentMana, thresholdItem],
        };
        surface.Assess(charge, 100, (107u, 100));
        surface.Assess(missingCurrentMana, 100, (108u, 100));
        surface.Assess(thresholdItem, 100, (107u, 10), (108u, 100));
        var profiles = new CombatSettings();
        profiles.ConsumableNames.Add(charge.Name);
        var host = new Host(surface);
        var controller = new ItemManaRechargeController(
            host,
            new InventorySettings
            {
                RefillWornMana = true,
                RefillWornManaPercent = 33,
            },
            profiles);

        Assert.True(controller.Tick(canAct: true));

        Assert.Equal([(10u, 1u)], surface.ApplyCalls);
        Assert.Contains(host.Logger.Infos, line =>
            line.Contains("thresholdItem=Low Gloves (0x00000014)",
                StringComparison.Ordinal));
        Assert.Contains(19u, surface.PropertyCaptures);
        Assert.Contains(20u, surface.PropertyCaptures);

        Assert.True(controller.Tick(canAct: true));
        Assert.Equal([(10u, 1u)], surface.ApplyCalls);

        surface.LastItemCompletion = new PluginItemUseCompletion(
            1L,
            999u,
            surface.ObjectId,
            0u);
        Assert.True(controller.Tick(canAct: true));
        Assert.Equal([(10u, 1u)], surface.ApplyCalls);

        surface.LastItemCompletion = new PluginItemUseCompletion(
            2L,
            charge.ObjectId,
            surface.ObjectId,
            0u);
        // Completion can arrive before the live charged-effect update. The
        // accepted source must remain blocked even while every cached value
        // still looks like the previous charged stone.
        Assert.False(controller.Tick(canAct: true));
        Assert.Equal([(10u, 1u)], surface.ApplyCalls);
        surface.ReplaceAssessmentVersion(charge.ObjectId, 101);
        Assert.False(controller.Tick(canAct: true));
        Assert.Equal([(10u, 1u)], surface.ApplyCalls);
        surface.Inventory =
        [
            charge with { Effects = 0u },
            missingCurrentMana,
            thresholdItem,
        ];

        Assert.False(controller.Tick(canAct: true));
        Assert.Equal([10u], surface.IdentifyRequests);
        Assert.Equal([(10u, 1u)], surface.ApplyCalls);

        // A newer stamp alone is insufficient because appraisal application
        // merges keys and an empty stone's omitted 107 can remain stale.
        surface.ReplaceAssessmentVersion(charge.ObjectId, 101);

        Assert.False(controller.Tick(canAct: true));
        Assert.Equal([(10u, 1u)], surface.ApplyCalls);
        Assert.Contains(host.Logger.Infos, line =>
            line.Contains("success=True", StringComparison.Ordinal)
                && line.Contains("expected=0x00000001, actual=0x00000001",
                    StringComparison.Ordinal));

        // A real empty-to-charged transition may refill to exactly the old amount.
        surface.Inventory = [charge, missingCurrentMana, thresholdItem];
        Assert.False(controller.Tick(canAct: true)); // Still the appraisal from before charging.
        surface.ReplaceAssessmentVersion(charge.ObjectId, 102);
        Assert.True(controller.Tick(canAct: true));
        Assert.Equal([(10u, 1u), (10u, 1u)], surface.ApplyCalls);
        surface.LastItemCompletion = new PluginItemUseCompletion(3, 10, 999, 0);
        Assert.True(controller.Tick(canAct: true));
        Assert.False(controller.Tick(canAct: true, elapsedSeconds: 15d));
        Assert.Contains("unconfirmed", controller.Status);
        Assert.False(controller.Tick(canAct: true));
        Assert.Equal([(10u, 1u), (10u, 1u)], surface.ApplyCalls);
    }

    [Fact]
    public void EmptyStoneWithStalePositiveManaIsNeverARefillCandidate()
    {
        PluginInventoryItem emptyStone = Item(10, "Mana Stone", 0x00080000u) with
        {
            ItemCurrentMana = 100,
            Effects = 0u,
        };
        PluginInventoryItem armor = Item(20, "Worn Armor") with
        {
            EquippedLocation = 1u,
            ItemCurrentMana = 10,
            ItemMaximumMana = 100,
        };
        Assert.Null(ItemManaRechargePlanner.Plan(
            [emptyStone, armor],
            new HashSet<string>(StringComparer.Ordinal) { emptyStone.Name },
            33));
    }

    private static PluginInventoryItem Item(
        uint id,
        string name,
        uint itemType = 0x80u) => new(
            id, 0u, name, itemType, 1u, 0u, 0u, 0u, 0u, 0u, 0u,
            1, 0, 0, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0d, 0, 0, 0);

    private sealed class Host(Surface surface) : IPluginHost
    {
        public bool HasUi => false;
        public TestLogger Logger { get; } = new();
        public IPluginLogger Log => Logger;
        public IGameState State => null!;
        public IEvents Events => null!;
        public ISelectionService Selection => null!;
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IAutomationSurface Automation => surface;
    }

    private sealed class TestLogger : IPluginLogger
    {
        public List<string> Infos { get; } = [];
        public void Info(string message) => Infos.Add(message);
        public void Warn(string message) { }
        public void Error(string message, Exception? exception = null) { }
    }

    private sealed class Surface :
        IAutomationSurface,
        ICharacterInfo,
        IItemAutomation,
        IWorldObjectAutomation
    {
        private readonly Dictionary<uint, PluginWorldObject> _objects = [];
        private readonly Dictionary<uint, PluginItemProperties> _properties = [];

        public bool IsAvailable => true;
        public ICharacterInfo Character => this;
        public ISpellCatalog Spells => NoOpAutomationSurface.Instance;
        public IMagicCommands Magic => NoOpAutomationSurface.Instance;
        public IPluginChat Chat => NoOpAutomationSurface.Instance;
        public IItemAutomation Items => this;
        public IWorldObjectAutomation Objects => this;
        public bool IsInWorld => true;
        public uint ObjectId => 1u;
        public uint CurrentHealth => 100u;
        public uint MaxHealth => 100u;
        public uint CurrentStamina => 100u;
        public uint MaxStamina => 100u;
        public uint CurrentMana => 100u;
        public uint MaxMana => 100u;
        public IReadOnlyList<PluginSkillInfo> Skills => [];
        public IReadOnlyList<PluginAttributeInfo> Attributes => [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments => [];
        public IReadOnlyList<PluginInventoryItem> Inventory { get; set; } = [];
        public PluginItemUseCompletion LastItemCompletion { get; set; }
        PluginItemUseCompletion IItemAutomation.LastCompletion =>
            LastItemCompletion;
        public List<(uint Source, uint Target)> ApplyCalls { get; } = [];
        public List<uint> IdentifyRequests { get; } = [];
        public List<uint> PropertyCaptures { get; } = [];

        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            skill = default;
            return false;
        }

        public IReadOnlyList<PluginInventoryItem> CaptureOwnedItems() =>
            Inventory;

        public bool TryCaptureProperties(
            uint objectId,
            out PluginItemProperties properties)
        {
            PropertyCaptures.Add(objectId);
            return _properties.TryGetValue(objectId, out properties);
        }

        public PluginItemCommandResult Apply(
            uint objectId,
            uint targetObjectId)
        {
            ApplyCalls.Add((objectId, targetObjectId));
            return new(PluginItemCommandStatus.Started);
        }

        public bool TryGet(uint objectId, out PluginWorldObject value) =>
            _objects.TryGetValue(objectId, out value);

        public PluginItemCommandResult Identify(uint objectId)
        {
            IdentifyRequests.Add(objectId);
            return new(PluginItemCommandStatus.Started);
        }

        public void Assess(
            PluginInventoryItem item,
            int version,
            params (uint Key, int Value)[] ints)
        {
            _objects[item.ObjectId] = new PluginWorldObject(
                item.ObjectId,
                item.WeenieClassId,
                item.Name,
                item.ObjectClass,
                item.ItemType,
                item.ContainerObjectId,
                item.WielderObjectId)
            {
                IsOwned = true,
                HasAppraisalData = true,
                LastIdTime = version,
            };
            _properties[item.ObjectId] = new PluginItemProperties(
                ints.ToDictionary(static pair => pair.Key, static pair => pair.Value),
                new Dictionary<uint, long>(),
                new Dictionary<uint, bool>(),
                new Dictionary<uint, double>(),
                new Dictionary<uint, string>(),
                new Dictionary<uint, uint>(),
                new Dictionary<uint, uint>());
        }

        public void ReplaceAssessmentVersion(uint objectId, int version)
        {
            _objects[objectId] = _objects[objectId] with
            {
                LastIdTime = version,
            };
        }
    }
}

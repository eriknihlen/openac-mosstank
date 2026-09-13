using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class LootingTests
{
    private const uint Player = 0x50000001u;
    private static readonly PluginItemProperties EmptyProperties = new(
        new Dictionary<uint, int>(),
        new Dictionary<uint, long>(),
        new Dictionary<uint, bool>(),
        new Dictionary<uint, double>(),
        new Dictionary<uint, string>(),
        new Dictionary<uint, uint>(),
        new Dictionary<uint, uint>());

    [Theory]
    [InlineData("Killed by Tester.", "Tester")]
    [InlineData("Killed by Moss Wart's War Magic.", "Moss Wart's War Magic")]
    [InlineData("Generated treasure", "")]
    public void CorpseKillerParsingMatchesVtankLongDescription(
        string description,
        string expected) =>
        Assert.Equal(expected, LootController.KillerName(description));

    [Theory]
    [InlineData("name ~= Pyreal && value >= 100", true)]
    [InlineData("name == 'Heavy Pyreal' || wcid == 55", true)]
    [InlineData("workmanship > 8 && material == 12", true)]
    [InlineData("int[105] == 7", true)]
    [InlineData("stack > 10", false)]
    public void LootExpressionMatchesProjectedAndRawProperties(
        string source,
        bool expected)
    {
        PluginInventoryItem item = Item(10, "Heavy Pyreal", 44) with
        {
            Value = 100,
            Workmanship = 9.5f,
            MaterialType = 12,
            StackSize = 5,
        };
        PluginItemProperties properties = EmptyProperties with
        {
            Ints = new Dictionary<uint, int> { [105] = 7 },
        };

        Assert.Equal(
            expected,
            LootRuleExpression.Compile(source).IsMatch(item, properties));
    }

    [Fact]
    public void RuleEngineUsesFirstMatchAndHonorsKeepUpToCounts()
    {
        PluginInventoryItem corpseItem = Item(10, "Health Elixir", 44) with
        {
            StackSize = 2,
        };
        PluginInventoryItem held = Item(20, "Health Elixir", 44) with
        {
            StackSize = 8,
        };
        LootRule[] rules =
        [
            new()
            {
                Name = "Elixirs",
                Expression = "name ~= elixir",
                Action = LootAction.KeepUpTo,
                KeepCount = 10,
            },
            new() { Name = "All", Expression = "*", Action = LootAction.Keep },
        ];

        Assert.Null(LootRuleEngine.Decide(
            corpseItem,
            EmptyProperties,
            rules,
            [held],
            new Dictionary<string, int> { ["Health Elixir"] = 2 }));

        LootDecision decision = Assert.IsType<LootDecision>(
            LootRuleEngine.Decide(
                corpseItem,
                EmptyProperties,
                rules,
                [],
                null));
        Assert.Equal(LootAction.KeepUpTo, decision.Action);
        Assert.Equal("Elixirs", decision.RuleName);
    }

    [Fact]
    public void ImportedVtankRequirementsExecuteAsExactAndSet()
    {
        PluginInventoryItem item = Item(10u, "Epic Sword", 44u) with
        {
            Value = 25_000,
            ObjectClass = PluginObjectClass.MeleeWeapon,
            AppraisedSpellIds = [777u],
            Palettes =
            [
                new PluginPaletteInfo(0x0400ABCDu, 0, 1, 255, 0, 0),
            ],
            GearDamage = 3,
            GearCriticalChance = 2,
        };
        var automation = new Automation
        {
            CharacterLevel = 275,
            MainPackSlots = 101,
            KnownSpell = new PluginSpellInfo(
                777u, "Legendary Blood Thirst", 1u, 1, 1, 1, 1f,
                32u, string.Empty, false, true),
            SkillsValue =
            [
                new PluginSkillInfo(
                    44u,
                    "Heavy Weapons",
                    PluginSkillTraining.Specialized,
                    500u)
                {
                    Base = 450u,
                },
            ],
        };
        var rule = new LootRule
        {
            Name = "VTClassic",
            Action = LootAction.Keep,
            VtankRequirements =
            [
                Requirement(1, "^Epic.*", "1"),
                Requirement(3, "25000", "19"),
                Requirement(7, "1"),
                Requirement(9, "Legendary.*", string.Empty, "1"),
                Requirement(14, "255", "0", "0", "0", "0"),
                Requirement(
                    15,
                    "255", "0", "0", "0", "0",
                    "Amuli Coat (Chest)"),
                Requirement(16, "255", "0", "0", "0", "0", "0"),
                Requirement(17, "0", "43981"),
                Requirement(1000, "500", "44"),
                Requirement(1001, "101"),
                Requirement(1002, "275"),
                Requirement(1004, "44", "400", "460"),
                Requirement(2007, "5"),
                Requirement(9999, "false"),
            ],
        };

        Assert.NotNull(LootRuleEngine.Decide(
            item,
            EmptyProperties,
            [rule],
            [],
            host: new Host(automation)));

        rule.VtankRequirements[1].Payload = "25001\r\n19\r\n";
        Assert.Null(LootRuleEngine.Decide(
            item,
            EmptyProperties,
            [rule],
            [],
            host: new Host(automation)));
    }

    [Fact]
    public void ManaStonePlannerUsesStoneOnQualifiedHighestManaTank()
    {
        PluginInventoryItem stone = Item(1u, "Mana Stone", 1u) with
        {
            ItemType = 0x00080000u,
        };
        PluginInventoryItem lowTank = Item(2u, "Low Tank", 2u) with
        {
            ItemCurrentMana = 1200,
            Value = 1,
        };
        PluginInventoryItem highTank = Item(3u, "High Tank", 3u) with
        {
            ItemCurrentMana = 3000,
            Value = 1,
        };
        var classified = new Dictionary<uint, LootAction>
        {
            [stone.ObjectId] = LootAction.ManaStone,
            [lowTank.ObjectId] = LootAction.ManaTank,
            [highTank.ObjectId] = LootAction.ManaTank,
        };

        ManaStoneTransferPlan plan = Assert.IsType<ManaStoneTransferPlan>(
            ManaStoneTransferPlanner.Plan(
                [stone, lowTank, highTank],
                classified,
                minimumTankMana: 1000));

        Assert.Equal(stone.ObjectId, plan.StoneObjectId);
        Assert.Equal(highTank.ObjectId, plan.TankObjectId);
    }

    [Theory]
    [InlineData(6.9f, 6.0f, true)]
    [InlineData(6.9f, 7.0f, false)]
    [InlineData(7.0f, 8.99f, true)]
    [InlineData(8.99f, 9.0f, false)]
    [InlineData(9.0f, 9.99f, true)]
    [InlineData(9.99f, 10.0f, false)]
    [InlineData(10.0f, 10.0f, true)]
    public void SalvageCombineUsesVtanksExactWorkmanshipBands(
        float left,
        float right,
        bool expected) =>
        Assert.Equal(
            expected,
            SalvageBagCombinePlanner.SameVtankWorkmanshipBand(left, right));

    [Fact]
    public void ImportedSalvageRangesAndValueModeDriveThePlanner()
    {
        Assert.False(SalvageBagCombinePlanner.SameCombineBand(
            5f,
            6f,
            "1-5, 6-10"));
        var settings = new VtankSalvageCombineSettings
        {
            DefaultCombineString = "1-5, 6-10",
            MaterialCombineStrings = [],
            MaterialValueModeValues = new Dictionary<int, int>
            {
                [12] = 100,
            },
        };
        PluginInventoryItem[] bags =
        [
            Item(1u, "Salvaged Iron (4)", 1u) with
            {
                MaterialType = 12u, Workmanship = 4f, Value = 60, Structure = 30,
            },
            Item(2u, "Salvaged Iron (5)", 1u) with
            {
                MaterialType = 12u, Workmanship = 5f, Value = 50, Structure = 30,
            },
            Item(3u, "Salvaged Iron (6)", 1u) with
            {
                MaterialType = 12u, Workmanship = 6f, Value = 500, Structure = 30,
            },
        ];

        SalvageBagCombinePlan plan = Assert.IsType<SalvageBagCombinePlan>(
            SalvageBagCombinePlanner.Plan(bags, settings: settings));

        Assert.Equal(new uint[] { 1u, 2u }, plan.ObjectIds);
    }

    [Fact]
    public void CombineSalvageRunsRealTwoBagSalvageOperation()
    {
        var settings = new LootSettings
        {
            Enabled = true,
            CombineSalvage = true,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        const uint tool = 0x50000600u;
        const uint first = 0x50000601u;
        const uint second = 0x50000602u;
        var automation = new Automation
        {
            Owned =
            [
                Item(tool, "Salvage Tool", 1u) with
                {
                    ItemType = 0x20000000u,
                },
                Item(first, "Salvaged Iron (7)", 2u) with
                {
                    MaterialType = 12u,
                    Workmanship = 7.1f,
                },
                Item(second, "Salvaged Iron (8)", 2u) with
                {
                    MaterialType = 12u,
                    Workmanship = 8.5f,
                },
            ],
        };
        var controller = new LootController(new Host(automation), settings);

        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { (tool, first), (tool, second) }, automation.Salvaged);
        automation.Owned = [automation.Owned[0], automation.Owned[1]];
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal("Combined salvage bags.", controller.Status);
    }

    [Fact]
    public void ControllerOpensClassifiesAndPicksThroughCanonicalLootSurface()
    {
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(new LootRule
        {
            Name = "Coins",
            Expression = "name ~= coin",
            Action = LootAction.Keep,
            Priority = 3,
        });
        var automation = new Automation();
        var controller = new LootController(new Host(automation), settings);
        const uint corpse = 0x70000001u;
        const uint coin = 0x70000002u;
        automation.Corpses =
        [
            new PluginLootContainer(
                corpse, 1u, "Corpse", 3f, false, false, false)
            {
                IsIdentified = true,
                LongDescription = "Killed by Tester.",
            },
        ];

        Assert.True(controller.Tick(1d, canAct: true));
        Assert.Equal(new[] { corpse }, automation.Opened);

        automation.Requested = corpse;
        automation.Current = corpse;
        automation.Contents = [Item(coin, "Colosseum coin", 77)];
        Assert.True(controller.Tick(0.2d, canAct: true));
        Assert.Equal(new[] { coin }, automation.Identified);

        automation.AppraisalState = new PluginAppraisalState(1, 0u, coin);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { coin }, automation.Picked);

        automation.Contents = [];
        automation.InventoryCompletion = new PluginInventoryCompletion(
            1,
            PluginInventoryCommandKind.Pickup,
            coin,
            0u);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(LootAction.Keep, controller.ClassifiedOwnedItems[coin]);

        Assert.False(controller.Tick(0.2d, canAct: true));
        Assert.Equal("Corpse complete.", controller.Status);
    }

    /// <summary>
    /// A looting pass has to be followable from outside the panel: what it
    /// opened, what it decided about each item, and what it actually took.
    /// Without those lines a run leaves no record of a loot pass at all.
    /// </summary>
    [Fact]
    public void ALootingPassReportsTheCorpseEachDecisionAndEachPickup()
    {
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(new LootRule
        {
            Name = "Coins",
            Expression = "name ~= coin",
            Action = LootAction.Keep,
            Priority = 3,
        });
        var automation = new Automation();
        var logged = new List<(MacroLogChannel Channel, string Message)>();
        var controller = new LootController(new Host(automation), settings)
        {
            Log = (channel, message) => logged.Add((channel, message)),
        };
        const uint corpse = 0x70000021u;
        const uint coin = 0x70000022u;
        automation.Corpses =
        [
            new PluginLootContainer(
                corpse, 1u, "Drudge Corpse", 3f, false, false, false)
            {
                IsIdentified = true,
                LongDescription = "Killed by Tester.",
            },
        ];

        Assert.True(controller.Tick(1d, canAct: true));
        automation.Requested = corpse;
        automation.Current = corpse;
        automation.Contents = [Item(coin, "Colosseum coin", 77)];
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.AppraisalState = new PluginAppraisalState(1, 0u, coin);
        Assert.True(controller.Tick(0.1d, canAct: true));
        automation.Contents = [];
        automation.InventoryCompletion = new PluginInventoryCompletion(
            1,
            PluginInventoryCommandKind.Pickup,
            coin,
            0u);
        Assert.True(controller.Tick(0.1d, canAct: true));

        Assert.All(logged, entry =>
            Assert.Equal(MacroLogChannel.Loot, entry.Channel));
        string[] messages = logged.Select(static entry => entry.Message).ToArray();
        Assert.Contains(
            messages,
            message => message.Contains("opening Drudge Corpse", StringComparison.Ordinal));
        Assert.Contains(
            messages,
            message => message.Contains("Colosseum coin -> Keep (Coins)", StringComparison.Ordinal));
        Assert.Contains(
            messages,
            message => message.Contains("taking Colosseum coin", StringComparison.Ordinal));
        Assert.Contains(
            messages,
            message => message.Contains("took Colosseum coin", StringComparison.Ordinal));
    }

    [Fact]
    public void ControllerUsesSelectedExternalClassifierAndPreservesItsAction()
    {
        var settings = new LootSettings
        {
            Enabled = true,
            ExternalClassifierId = "loot-plugin/main",
        };
        settings.Rules.Add(new LootRule
        {
            Expression = "*",
            Action = LootAction.NoLoot,
        });
        var automation = new Automation();
        var classifier = new ClassifierRegistry(
            "loot-plugin/main",
            new PluginLootClassification(
                Matched: true,
                PluginLootAction.User3,
                "External user action",
                Priority: 17));
        var controller = new LootController(
            new Host(automation, classifier),
            settings);
        const uint corpse = 0x70000011u;
        const uint item = 0x70000012u;
        automation.Corpses =
        [
            new PluginLootContainer(
                corpse, 1u, "Corpse", 3f, false, false, false)
            {
                IsIdentified = true,
                LongDescription = "Killed by Tester.",
            },
        ];

        Assert.True(controller.Tick(1d, canAct: true));
        automation.Current = corpse;
        automation.Contents = [Item(item, "External prize", 88u)];
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.AppraisalState = new PluginAppraisalState(1, 0u, item);
        Assert.True(controller.Tick(0.1d, canAct: true));

        Assert.Equal(new[] { item }, automation.Picked);
        Assert.Equal(1, classifier.ClassificationCount);
        Assert.Contains("External user action", controller.Status);

        PluginInventoryItem ownedItem = Item(item, "External prize", 88u);
        automation.Owned = [ownedItem];
        automation.Contents = [];
        automation.InventoryCompletion = new PluginInventoryCompletion(
            1,
            PluginInventoryCommandKind.Pickup,
            item,
            0u);
        Assert.True(controller.Tick(0.1d, canAct: true));
        PluginLootedItem looted = Assert.Single(classifier.Looted);
        Assert.Equal(item, looted.Item.ObjectId);
        Assert.Equal(PluginLootAction.User3, looted.Action);

        Assert.False(controller.Tick(0.2d, canAct: true));
        automation.Owned = [];
        Assert.False(controller.Tick(0.2d, canAct: true));
        Assert.Equal(new[] { item }, classifier.Removed);
    }

    [Fact]
    public void ExternalKeepUpToAndUnavailableClassifierNeverFallBackToInternalRules()
    {
        const uint corpse = 0x70000021u;
        const uint item = 0x70000022u;
        var automation = new Automation
        {
            Corpses =
            [
                new PluginLootContainer(
                    corpse, 1u, "Corpse", 3f, false, false, false)
                {
                    IsIdentified = true,
                    LongDescription = "Killed by Tester.",
                },
            ],
            Owned = [Item(0x70000023u, "Limited prize", 99u) with
            {
                StackSize = 2,
            }],
        };
        var settings = new LootSettings
        {
            Enabled = true,
            ExternalClassifierId = "loot-plugin/main",
        };
        settings.Rules.Add(new LootRule
        {
            Expression = "*",
            Action = LootAction.Keep,
        });
        var classifier = new ClassifierRegistry(
            "loot-plugin/main",
            new PluginLootClassification(
                Matched: true,
                PluginLootAction.KeepUpTo,
                KeepCount: 2));
        var controller = new LootController(
            new Host(automation, classifier),
            settings);

        Assert.True(controller.Tick(1d, canAct: true));
        automation.Current = corpse;
        automation.Contents = [Item(item, "Limited prize", 99u)];
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.AppraisalState = new PluginAppraisalState(1, 0u, item);
        Assert.False(controller.Tick(0.1d, canAct: true));
        Assert.Empty(automation.Picked);

        controller.Reset();
        automation.Requested = 0u;
        automation.Current = 0u;
        automation.Opened.Clear();
        automation.Identified.Clear();
        settings.ExternalClassifierId = "missing/classifier";
        Assert.True(controller.Tick(1d, canAct: true));
        automation.Current = corpse;
        Assert.False(controller.Tick(0.2d, canAct: true));
        Assert.Empty(automation.Picked);
    }

    [Fact]
    public void FellowshipAndRareCorpsePolicyMatchesVtankProtectionRules()
    {
        var settings = new LootSettings
        {
            Enabled = true,
            LootFellowCorpses = true,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        var automation = new Automation
        {
            Members =
            [
                new PluginFellowMember(
                    0x50000002u,
                    "Fellow",
                    1, 1, 1, 1, 1, 1, 2f)
                {
                    ShareLoot = true,
                },
            ],
            Corpses =
            [
                new PluginLootContainer(
                    0x70000100u, 1u, "Corpse", 3f, false, false, false)
                {
                    IsIdentified = true,
                    LongDescription = "Killed by Fellow.",
                },
            ],
        };
        var controller = new LootController(new Host(automation), settings);

        Assert.True(controller.Tick(0.25d, canAct: true));
        Assert.Equal(new[] { 0x70000100u }, automation.Opened);

        controller.Reset();
        automation.Requested = 0u;
        automation.Opened.Clear();
        automation.Members =
        [
            automation.Members[0] with { ShareLoot = false },
        ];
        Assert.False(controller.Tick(0.25d, canAct: true));
        Assert.True(controller.Tick(100d, canAct: true));
        Assert.Single(automation.Opened);

        controller.Reset();
        automation.Requested = 0u;
        automation.Opened.Clear();
        automation.Corpses =
        [
            automation.Corpses[0] with { IsGeneratedRare = true },
        ];
        Assert.False(controller.Tick(101d, canAct: true));
        Assert.Empty(automation.Opened);

        settings.LootOnlyRareCorpses = true;
        automation.Corpses =
        [
            automation.Corpses[0] with
            {
                IsGeneratedRare = true,
                LongDescription = "Killed by Tester. Generated rare treasure.",
            },
        ];
        Assert.True(controller.Tick(0.25d, canAct: true));
        Assert.Single(automation.Opened);
    }

    [Fact]
    public void UnknownReadableScrollOverridesNoLootAndIsPickedForReading()
    {
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(new LootRule
        {
            Expression = "*",
            Action = LootAction.NoLoot,
        });
        var automation = new Automation
        {
            KnownSpell = new PluginSpellInfo(
                777u,
                "Incantation of Testing",
                1u,
                1,
                100,
                10,
                0f,
                34u,
                string.Empty,
                false,
                false),
            SkillsValue =
            [
                new PluginSkillInfo(
                    34u,
                    "War Magic",
                    PluginSkillTraining.Trained,
                    90u),
            ],
            Corpses =
            [
                new PluginLootContainer(
                    0x70000200u, 1u, "Corpse", 3f, false, false, false)
                {
                    IsIdentified = true,
                    LongDescription = "Killed by Tester.",
                },
            ],
        };
        var controller = new LootController(new Host(automation), settings);
        const uint scroll = 0x70000201u;

        Assert.True(controller.Tick(0.25d, canAct: true));
        automation.Current = 0x70000200u;
        automation.Contents =
        [
            Item(scroll, "Incantation of Testing Scroll", 88u) with
            {
                ItemType = 0x80u,
                SpellId = 777u,
            },
        ];
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.AppraisalState = new PluginAppraisalState(1, 0u, scroll);
        Assert.True(controller.Tick(0.1d, canAct: true));

        Assert.Equal(new[] { scroll }, automation.Picked);
    }

    [Fact]
    public void UnopenableCorpseUsesOfficialAttemptBlacklist()
    {
        var settings = new LootSettings
        {
            Enabled = true,
            CorpseOpenTimeoutSeconds = 0.25d,
            BlacklistCorpseOpenAttemptCount = 2,
            BlacklistCorpseOpenTimeoutSeconds = 200d,
            ScanIntervalSeconds = 0.05d,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        var automation = new Automation
        {
            Corpses =
            [
                new PluginLootContainer(
                    0x70000300u, 1u, "Corpse", 3f, false, false, false)
                {
                    IsIdentified = true,
                    LongDescription = "Killed by Tester.",
                },
            ],
        };
        var controller = new LootController(new Host(automation), settings);

        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.True(controller.Tick(0.3d, canAct: true));
        Assert.False(controller.Tick(0.3d, canAct: true));
        Assert.Equal(2, automation.Opened.Count);
        Assert.Contains("Blacklisted", controller.Status);
        Assert.False(controller.Tick(199d, canAct: true));
        Assert.Equal(2, automation.Opened.Count);
        Assert.True(controller.Tick(2d, canAct: true));
        Assert.Equal(3, automation.Opened.Count);
    }

    [Fact]
    public void SalvageRuleRunsRealRetailSalvageAfterCorpseIsComplete()
    {
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(new LootRule
        {
            Expression = "*",
            Action = LootAction.Salvage,
        });
        const uint corpse = 0x70000400u;
        const uint source = 0x70000401u;
        const uint tool = 0x50000402u;
        var automation = new Automation
        {
            Corpses =
            [
                new PluginLootContainer(
                    corpse, 1u, "Corpse", 3f, false, false, false)
                {
                    IsIdentified = true,
                    LongDescription = "Killed by Tester.",
                },
            ],
            Owned =
            [
                Item(tool, "Salvage Tool", 99u) with
                {
                    ItemType = 0x20000000u,
                },
            ],
        };
        var controller = new LootController(new Host(automation), settings);

        Assert.True(controller.Tick(0.25d, canAct: true));
        automation.Current = corpse;
        automation.Contents =
        [
            Item(source, "Iron Sword", 100u) with { MaterialType = 12u },
        ];
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.AppraisalState = new PluginAppraisalState(1, 0u, source);
        Assert.True(controller.Tick(0.1d, canAct: true));
        automation.Contents = [];
        automation.Owned = [automation.Owned[0], Item(source, "Iron Sword", 100u)];
        automation.InventoryCompletion = new PluginInventoryCompletion(
            1,
            PluginInventoryCommandKind.Pickup,
            source,
            0u);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.False(controller.Tick(0.2d, canAct: true));

        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { (tool, source) }, automation.Salvaged);
        automation.Owned = [automation.Owned[0]];
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal("Salvaged Iron Sword.", controller.Status);
    }

    [Fact]
    public void SellRuleQueuesUntilVendorThenRunsAuthoritativeSale()
    {
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(new LootRule
        {
            Expression = "*",
            Action = LootAction.Sell,
        });
        const uint corpse = 0x70000500u;
        const uint source = 0x70000501u;
        var automation = new Automation
        {
            Corpses =
            [
                new PluginLootContainer(
                    corpse, 1u, "Corpse", 3f, false, false, false)
                {
                    IsIdentified = true,
                    LongDescription = "Killed by Tester.",
                },
            ],
        };
        var controller = new LootController(new Host(automation), settings);

        Assert.True(controller.Tick(0.25d, canAct: true));
        automation.Current = corpse;
        automation.Contents = [Item(source, "Ruby", 101u)];
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.AppraisalState = new PluginAppraisalState(1, 0u, source);
        Assert.True(controller.Tick(0.1d, canAct: true));
        automation.Contents = [];
        automation.Owned = [Item(source, "Ruby", 101u)];
        automation.InventoryCompletion = new PluginInventoryCompletion(
            1,
            PluginInventoryCommandKind.Pickup,
            source,
            0u);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.False(controller.Tick(0.2d, canAct: true));

        Assert.False(controller.Tick(0.1d, canAct: true));
        Assert.Contains("queued", controller.Status);
        Assert.Empty(automation.Sold);
        automation.VendorId = 0x70000510u;
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { source }, automation.Sold);
        automation.Owned = [];
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal("Sold Ruby.", controller.Status);
    }

    [Fact]
    public void ADecidedRuleMatchMeansTheItemNeedsNoAppraisal()
    {
        PluginInventoryItem money = Item(10u, "Pyreal", 273u) with
        {
            ObjectClass = PluginObjectClass.Money,
        };
        LootRule[] rules =
        [
            VtankRule("Money", LootAction.Keep, Requirement(7, "7")),
            VtankRule(
                "Good weapons",
                LootAction.Salvage,
                Requirement(2003, "100", "218103842")),
        ];

        Assert.False(LootRuleEngine.NeedsIdentify(
            money,
            EmptyProperties,
            rules));
    }

    [Fact]
    public void AnOpenRuleIsMootWhenALaterDecidedRuleCarriesTheSameAction()
    {
        PluginInventoryItem money = Item(10u, "Pyreal", 273u) with
        {
            ObjectClass = PluginObjectClass.Money,
        };
        LootRule[] sameAction =
        [
            VtankRule(
                "Good weapons",
                LootAction.Keep,
                Requirement(2003, "100", "218103842")),
            VtankRule("Money", LootAction.Keep, Requirement(7, "7")),
        ];
        LootRule[] differentAction =
        [
            VtankRule(
                "Good weapons",
                LootAction.Keep,
                Requirement(2003, "100", "218103842")),
            VtankRule("Money", LootAction.Salvage, Requirement(7, "7")),
        ];

        Assert.False(LootRuleEngine.NeedsIdentify(
            money,
            EmptyProperties,
            sameAction));
        Assert.True(LootRuleEngine.NeedsIdentify(
            money,
            EmptyProperties,
            differentAction));
    }

    [Fact]
    public void SpellRequirementsStayOpenOnlyWhileTheItemClaimsToBeMagical()
    {
        LootRule[] rules =
        [
            VtankRule("Enchanted", LootAction.Keep, Requirement(8, "1")),
            VtankRule("Everything else", LootAction.NoLoot),
        ];
        PluginInventoryItem plain = Item(10u, "Rock", 273u);
        PluginInventoryItem magical = plain with { Effects = 1u };

        Assert.False(LootRuleEngine.NeedsIdentify(
            plain,
            EmptyProperties,
            rules));
        Assert.True(LootRuleEngine.NeedsIdentify(
            magical,
            EmptyProperties,
            rules));
    }

    [Fact]
    public void ACatchAllProfileNeverAppraisesACorpseItem()
    {
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(new LootRule
        {
            Name = "Keep everything",
            Expression = "*",
            Action = LootAction.Keep,
        });
        const uint corpse = 0x70000600u;
        const uint loose = 0x70000601u;
        var automation = new Automation
        {
            Corpses =
            [
                new PluginLootContainer(
                    corpse, 1u, "Corpse of a Drudge", 3f, false, false, false)
                {
                    IsIdentified = true,
                    LongDescription = "Killed by Tester.",
                },
            ],
        };
        var controller = new LootController(new Host(automation), settings);

        Assert.True(controller.Tick(0.25d, canAct: true));
        automation.Current = corpse;
        automation.Contents = [Item(loose, "Pyreal", 273u)];
        Assert.True(controller.Tick(0.2d, canAct: true));

        Assert.Empty(automation.Identified);
        Assert.Equal(new[] { loose }, automation.Picked);
    }

    [Fact]
    public void EverythingOnYourOwnDeathCorpseIsAppraised()
    {
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(new LootRule
        {
            Name = "Keep everything",
            Expression = "*",
            Action = LootAction.Keep,
        });
        const uint corpse = 0x70000700u;
        const uint loose = 0x70000701u;
        var automation = new Automation
        {
            Corpses =
            [
                new PluginLootContainer(
                    corpse, 1u, "Corpse of Tester", 3f, false, false, false)
                {
                    IsIdentified = true,
                    LongDescription = "Killed by Tester.",
                },
            ],
        };
        var controller = new LootController(new Host(automation), settings);

        Assert.True(controller.Tick(0.25d, canAct: true));
        automation.Current = corpse;
        automation.Contents = [Item(loose, "Pyreal", 273u)];
        Assert.True(controller.Tick(0.2d, canAct: true));

        Assert.Equal(new[] { loose }, automation.Identified);
    }

    [Fact]
    public void ABuffedKeyOnlyGainsItsSpellBonusWhenTheItemCarriesTheBaseKey()
    {
        // Spell 2598 is Blood Drinker: +2 to the item's damage key.
        PluginInventoryItem noDamage = Item(10u, "Ruby", 273u) with
        {
            AppraisedSpellIds = [2598u],
        };
        PluginInventoryItem weapon = noDamage with { Damage = 5 };
        VtankLootRequirement[] atLeastOne =
            [Requirement(2003, "1", "218103842")];

        Assert.False(VtankLootRequirementEvaluator.IsMatch(
            atLeastOne,
            noDamage,
            EmptyProperties,
            null,
            out _));
        Assert.True(VtankLootRequirementEvaluator.IsMatch(
            atLeastOne,
            weapon,
            EmptyProperties,
            null,
            out _));
    }

    [Theory]
    // Spell 3199 authors Change 1.10, which truncates to 1 and multiplies.
    [InlineData(3199u, 144u, 1d, 1.10d, true)]
    [InlineData(3199u, 144u, 1d, 1.11d, false)]
    // Spell 2588 authors Change 0.05, which truncates to 0 and adds.
    [InlineData(2588u, 29u, 0.10d, 0.15d, true)]
    [InlineData(2588u, 29u, 0.10d, 0.16d, false)]
    public void TheBuffedDoubleOperationComesFromTheAuthoredChangeField(
        uint spellId,
        uint key,
        double baseValue,
        double threshold,
        bool expected)
    {
        PluginInventoryItem item = Item(10u, "Sword", 273u) with
        {
            AppraisedSpellIds = [spellId],
        };
        PluginItemProperties properties = EmptyProperties with
        {
            Floats = new Dictionary<uint, double> { [key] = baseValue },
        };

        Assert.Equal(
            expected,
            VtankLootRequirementEvaluator.IsMatch(
                [Requirement(
                    2005,
                    threshold.ToString(CultureInfo.InvariantCulture),
                    key.ToString(CultureInfo.InvariantCulture))],
                item,
                properties,
                null,
                out _));
    }

    [Fact]
    public void AFartherRareCorpseIsOpenedBeforeANearerMundaneOne()
    {
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(new LootRule { Expression = "*" });
        const uint mundane = 0x70000800u;
        const uint rare = 0x70000801u;
        var automation = new Automation
        {
            Corpses =
            [
                new PluginLootContainer(
                    mundane, 1u, "Corpse", 3f, false, false, false)
                {
                    IsIdentified = true,
                    LongDescription = "Killed by Tester.",
                },
                new PluginLootContainer(
                    rare, 1u, "Corpse", 25f, false, false, false)
                {
                    IsIdentified = true,
                    IsGeneratedRare = true,
                    LongDescription =
                        "Killed by Tester. Generated rare treasure.",
                },
            ],
        };
        var controller = new LootController(new Host(automation), settings);

        Assert.True(controller.Tick(0.25d, canAct: true));
        Assert.Equal(new[] { rare }, automation.Opened);
    }

    [Fact]
    public void ARefusedCorpseIsSkippedForTenSecondsWithoutRetrying()
    {
        var settings = new LootSettings
        {
            Enabled = true,
            CorpseOpenTimeoutSeconds = 0.25d,
            ScanIntervalSeconds = 0.05d,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        const uint corpse = 0x70000900u;
        var automation = new Automation
        {
            Corpses =
            [
                new PluginLootContainer(
                    corpse, 1u, "Corpse", 3f, false, false, false)
                {
                    IsIdentified = true,
                    LongDescription = "Killed by Tester.",
                },
            ],
        };
        var controller = new LootController(new Host(automation), settings);

        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Single(automation.Opened);
        automation.ChatMessages.Add(new PluginChatMessage(
            1uL,
            0u,
            0,
            string.Empty,
            "The Corpse of a Drudge Slinker is already in use by someone else!",
            string.Empty));

        Assert.False(controller.Tick(0.3d, canAct: true));
        Assert.False(controller.Tick(9d, canAct: true));
        Assert.Single(automation.Opened);

        Assert.True(controller.Tick(1d, canAct: true));
        Assert.Equal(2, automation.Opened.Count);
    }

    [Fact]
    public void ACorpseSeenOutOfRangeIsAlreadyOldEnoughWhenItComesIntoRange()
    {
        var settings = new LootSettings
        {
            Enabled = true,
            LootAllCorpses = true,
            CorpseApproachRange = 20d,
            ScanIntervalSeconds = 0.05d,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        const uint corpse = 0x70000A00u;
        var far = new PluginLootContainer(
            corpse, 1u, "Corpse", 80f, false, false, false)
        {
            IsIdentified = true,
            LongDescription = "Killed by Stranger.",
        };
        var automation = new Automation { Corpses = [far] };
        var controller = new LootController(new Host(automation), settings);

        Assert.False(controller.Tick(0.1d, canAct: true));
        Assert.False(controller.Tick(100d, canAct: true));
        Assert.Empty(automation.Opened);

        automation.Corpses = [far with { Distance = 3f }];
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { corpse }, automation.Opened);
    }

    private static LootRule VtankRule(
        string name,
        LootAction action,
        params VtankLootRequirement[] requirements) => new()
    {
        Name = name,
        Expression = "*",
        Action = action,
        VtankRequirements = [.. requirements],
    };

    private static PluginInventoryItem Item(uint id, string name, uint wcid) =>
        new(
            id, wcid, name, 0u, 0u, 0u, 0u, 0u, 0u, 0u, 0u,
            1, 0, 0, 0u, 0, 0, 0u, false, 0d, 0, 0, 0, 0d, 0, 0, 0);

    private static VtankLootRequirement Requirement(
        int type,
        params string[] lines) => new()
    {
        Type = type,
        Payload = string.Join("\r\n", lines) + "\r\n",
    };

    private sealed class Automation
        : IAutomationSurface, ICharacterInfo, ISpellCatalog, IItemAutomation,
          ILootAutomation, IFellowshipAutomation, IPluginChat
    {
        public bool IsAvailable => true;
        public ICharacterInfo Character => this;
        public ISpellCatalog Spells => this;
        public IMagicCommands Magic => NoOpAutomationSurface.Instance;
        public IPluginChat Chat => this;
        public List<PluginChatMessage> ChatMessages { get; } = [];
        public IReadOnlyList<PluginChatMessage> CaptureMessages(
            ulong afterSequence) => ChatMessages
                .Where(message => message.Sequence > afterSequence)
                .ToArray();
        public void PostSystemMessage(string text) { }
        public IItemAutomation Items => this;
        public ILootAutomation Loot => this;
        public IFellowshipAutomation Fellowship => this;
        public bool IsInWorld => true;
        public uint ObjectId => Player;
        public string Name => "Tester";
        public int CharacterLevel { get; set; }
        public int Level => CharacterLevel;
        public int MainPackSlots { get; set; }
        public int MainPackFreeSlots => MainPackSlots;
        public uint CurrentHealth => 0;
        public uint MaxHealth => 0;
        public uint CurrentStamina => 0;
        public uint MaxStamina => 0;
        public uint CurrentMana => 0;
        public uint MaxMana => 0;
        public IReadOnlyList<PluginSkillInfo> Skills => SkillsValue;
        public IReadOnlyList<PluginAttributeInfo> Attributes => [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments => [];
        public uint Requested { get; set; }
        public uint Current { get; set; }
        public uint RequestedContainerId => Requested;
        public uint CurrentContainerId => Current;
        public IReadOnlyList<PluginLootContainer> Corpses { get; set; } = [];
        public IReadOnlyList<PluginInventoryItem> Contents { get; set; } = [];
        public IReadOnlyList<PluginInventoryItem> Owned { get; set; } = [];
        public PluginInventoryCompletion InventoryCompletion { get; set; }
        public PluginAppraisalState AppraisalState { get; set; }
        public PluginAppraisalState Appraisal => AppraisalState;
        public IReadOnlyList<PluginSkillInfo> SkillsValue { get; set; } = [];
        public PluginSpellInfo? KnownSpell { get; set; }
        public bool SpellLearned { get; set; }
        public IReadOnlyList<PluginFellowMember> Members { get; set; } = [];
        public bool IsInFellowship => Members.Count != 0;
        public IReadOnlyList<PluginFellowMember> CaptureMembers() => Members;
        public IReadOnlyList<PluginSpellInfo> KnownSelfBuffs => [];
        public bool IsKnown(uint spellId) => SpellLearned
            && KnownSpell?.SpellId == spellId;
        public bool TryGet(uint spellId, out PluginSpellInfo info)
        {
            if (KnownSpell is { } spell && spell.SpellId == spellId)
            {
                info = spell;
                return true;
            }
            info = default;
            return false;
        }
        PluginInventoryCompletion ILootAutomation.LastInventoryCompletion =>
            InventoryCompletion;
        public List<uint> Opened { get; } = [];
        public List<uint> Picked { get; } = [];
        public List<uint> Identified { get; } = [];
        public List<(uint Tool, uint Item)> Salvaged { get; } = [];
        public List<uint> Sold { get; } = [];
        public uint VendorId { get; set; }
        public uint ActiveVendorObjectId => VendorId;
        public IReadOnlyList<PluginInventoryItem> CaptureOwnedItems() => Owned;
        public IReadOnlyList<PluginLootContainer> CaptureCorpses(
            float maximumDistance) => Corpses
                .Where(corpse => corpse.Distance <= maximumDistance)
                .ToArray();
        public IReadOnlyList<PluginInventoryItem> CaptureCurrentContents() =>
            Contents;
        public bool TryCaptureProperties(
            uint objectId,
            out PluginItemProperties properties)
        {
            properties = EmptyProperties;
            return true;
        }
        public PluginItemCommandResult Open(uint containerObjectId)
        {
            Opened.Add(containerObjectId);
            Requested = containerObjectId;
            return new(PluginItemCommandStatus.Started);
        }
        public PluginItemCommandResult Identify(uint objectId)
        {
            Identified.Add(objectId);
            AppraisalState = AppraisalState with
            {
                Revision = AppraisalState.Revision + 1,
                AwaitingObjectId = objectId,
            };
            return new(PluginItemCommandStatus.Started);
        }
        public PluginItemCommandResult Pickup(
            uint objectId,
            bool mainPack = false)
        {
            Picked.Add(objectId);
            return new(PluginItemCommandStatus.Started);
        }
        public PluginItemCommandResult Salvage(
            uint toolObjectId,
            IReadOnlyList<uint> itemObjectIds)
        {
            foreach (uint itemObjectId in itemObjectIds)
                Salvaged.Add((toolObjectId, itemObjectId));
            return new(PluginItemCommandStatus.Started);
        }
        public PluginItemCommandResult Sell(uint objectId, uint amount = 0u)
        {
            Sold.Add(objectId);
            return new(PluginItemCommandStatus.Started);
        }
        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            foreach (PluginSkillInfo candidate in SkillsValue)
            {
                if (candidate.SkillId != skillId)
                    continue;
                skill = candidate;
                return true;
            }
            skill = default;
            return false;
        }
    }

    private sealed class Host(
        IAutomationSurface automation,
        IPluginLootClassifierRegistry? lootClassifiers = null) : IPluginHost
    {
        public bool HasUi => false;
        public IPluginLogger Log => Logger.Instance;
        public IGameState State => EmptyState.Instance;
        public IEvents Events => EmptyEvents.Instance;
        public ISelectionService Selection => EmptySelection.Instance;
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IAutomationSurface Automation { get; } = automation;
        public IPluginLootClassifierRegistry LootClassifiers { get; } =
            lootClassifiers ?? NoOpPluginLootClassifierRegistry.Instance;
    }

    private sealed class ClassifierRegistry(
        string id,
        PluginLootClassification result) : IPluginLootClassifierRegistry
    {
        public int ClassificationCount { get; private set; }
        public List<PluginLootedItem> Looted { get; } = [];
        public List<uint> Removed { get; } = [];
        public IReadOnlyList<PluginLootClassifierInfo> Available =>
            [new(id, "Test classifier")];

        public bool TryClassify(
            string classifierId,
            in PluginLootClassificationContext context,
            out PluginLootClassification classification)
        {
            if (!string.Equals(classifierId, id, StringComparison.OrdinalIgnoreCase))
            {
                classification = default;
                return false;
            }
            ClassificationCount++;
            classification = result;
            return true;
        }

        public bool TryNotifyLooted(
            string classifierId,
            in PluginLootedItem item)
        {
            if (!string.Equals(classifierId, id, StringComparison.OrdinalIgnoreCase))
                return false;
            Looted.Add(item);
            return true;
        }

        public bool TryNotifyItemRemoved(string classifierId, uint objectId)
        {
            if (!string.Equals(classifierId, id, StringComparison.OrdinalIgnoreCase))
                return false;
            Removed.Add(objectId);
            return true;
        }
    }

    private sealed class Logger : IPluginLogger
    {
        public static Logger Instance { get; } = new();
        public void Info(string message) { }
        public void Warn(string message) { }
        public void Error(string message, Exception? exception = null) { }
    }

    private sealed class EmptyState : IGameState
    {
        public static EmptyState Instance { get; } = new();
        public IReadOnlyList<WorldEntitySnapshot> Entities => [];
    }

    private sealed class EmptyEvents : IEvents
    {
        public static EmptyEvents Instance { get; } = new();
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

    private sealed class EmptySelection : ISelectionService
    {
        public static EmptySelection Instance { get; } = new();
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

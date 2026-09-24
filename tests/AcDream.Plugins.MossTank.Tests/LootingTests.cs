using System.Globalization;
using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed partial class LootingTests
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
            ObjectClass = PluginObjectClass.ManaStone,
        };
        PluginInventoryItem lowTank = Item(2u, "Low Tank", 2u) with
        {
            ItemCurrentMana = 1200,
            Value = 1,
            Workmanship = 5,
            Effects = 1,
        };
        PluginInventoryItem highTank = Item(3u, "High Tank", 3u) with
        {
            ItemCurrentMana = 3000,
            Value = 1,
            Workmanship = 5,
            Effects = 1,
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
                minimumTankMana: 1000,
                isProfiledManaStone: static candidate =>
                    candidate.ObjectClass == PluginObjectClass.ManaStone
                    && candidate.Name == "Mana Stone"));

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
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.2d, canAct: true));
        Assert.Equal(new[] { coin }, automation.Identified);

        automation.AppraisalState = new PluginAppraisalState(1, 0u, coin);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { coin }, automation.Picked);

        automation.Contents = [];
        automation.InventoryCompletion = new PluginInventoryCompletion(
            1,
            PluginInventoryCommandKind.Pickup,
            coin,
            0u);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(LootAction.Keep, controller.ClassifiedOwnedItems[coin]);

        // The pass the corpse finishes on is the pass that closes it.
        Assert.True(controller.Tick(0.2d, canAct: true));
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
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.AppraisalState = new PluginAppraisalState(1, 0u, coin);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.1d, canAct: true));
        automation.Contents = [];
        automation.InventoryCompletion = new PluginInventoryCompletion(
            1,
            PluginInventoryCommandKind.Pickup,
            coin,
            0u);
        controller.TickIdentification(0.5d);
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
    public void ACorpseWithinReachIsDescribedOnTheFrameBeforeTheLootRuleGetsATurn()
    {
        var settings = new LootSettings
        {
            Enabled = true,
            ScanIntervalSeconds = 0.05d,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        var automation = new Automation();
        var controller = new LootController(new Host(automation), settings);
        const uint corpse = 0x70000402u;
        var unidentified = new PluginLootContainer(
            corpse, 1u, "Corpse", 3f, false, false, false)
        {
            LongDescription = "Killed by Tester.",
        };
        automation.Corpses = [unidentified];

        // The rule never had the pass; the frame asked anyway.
        controller.TickIdentification(0.1d);
        Assert.Equal(new[] { corpse }, automation.Identified);

        // Nothing is asked twice while the answer is outstanding.
        controller.TickIdentification(0.1d);
        Assert.Equal(new[] { corpse }, automation.Identified);

        automation.CompleteAppraisal(corpse, presentInUi: false);
        automation.Corpses = [unidentified with { IsIdentified = true }];
        controller.TickIdentification(0.1d);
        Assert.Equal(new[] { corpse }, automation.Identified);

        // The rule's first turn opens the corpse straight away.
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { corpse }, automation.Opened);
    }

    [Fact]
    public void UnidentifiedCorpseIsOpenedOnceItsAutomationIdentifyCompletes_EvenWithoutPresentation()
    {
        // Regression guard for the corpse-looting stall. A corpse identify is always
        // Automation-origin and, on the real host, only ever advances the
        // examination window's presentation target if that window already
        // happens to be showing the corpse -- which it normally is not.
        // Looting's corpse-appraisal wait polls Appraisal.CurrentObjectId
        // as a pure completion signal (via CompleteAppraisal, which mirrors
        // AppAutomationSurface's real LastCompletedAppraisalId mapping,
        // not a value the test hands over directly); if that signal were
        // gated on presentation instead, corpse looting would stall
        // forever waiting on a window that never opens.
        var settings = new LootSettings
        {
            Enabled = true,
            ScanIntervalSeconds = 0.05d,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        var automation = new Automation();
        var controller = new LootController(new Host(automation), settings);
        const uint corpse = 0x70000401u;
        var unidentified = new PluginLootContainer(
            corpse, 1u, "Corpse", 3f, false, false, false)
        {
            LongDescription = "Killed by Tester.",
        };
        automation.Corpses = [unidentified];

        controller.TickIdentification(0.1d);
        Assert.Equal(new[] { corpse }, automation.Identified);
        Assert.Empty(automation.Opened);

        automation.CompleteAppraisal(corpse, presentInUi: false);
        automation.Corpses = [unidentified with { IsIdentified = true }];

        // The completion signal advanced -- but presentation, which is a
        // different field entirely on the real host, never did. If
        // CompleteAppraisal's presentInUi parameter were decorative (as it
        // was before this assertion existed), this would not prove
        // anything about the corpse identify being genuinely unpresented.
        Assert.Equal(0u, automation.PresentedObjectId);

        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { corpse }, automation.Opened);
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
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.AppraisalState = new PluginAppraisalState(1, 0u, item);
        controller.TickIdentification(0.5d);
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
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.1d, canAct: true));
        PluginLootedItem looted = Assert.Single(classifier.Looted);
        Assert.Equal(item, looted.Item.ObjectId);
        Assert.Equal(PluginLootAction.User3, looted.Action);

        // The pass the corpse finishes on is the pass that closes it, and the
        // corpse stays this controller's business until the container shuts.
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.Current = 0u;
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
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.AppraisalState = new PluginAppraisalState(1, 0u, item);
        // The pass the corpse finishes on is the pass that closes it.
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Empty(automation.Picked);

        controller.Reset();
        automation.Requested = 0u;
        automation.Current = 0u;
        automation.Opened.Clear();
        automation.Identified.Clear();
        settings.ExternalClassifierId = "missing/classifier";
        Assert.True(controller.Tick(1d, canAct: true));
        automation.Current = corpse;
        // The pass the corpse finishes on is the pass that closes it.
        Assert.True(controller.Tick(0.2d, canAct: true));
        Assert.Empty(automation.Picked);
    }

    // Rare-only looting describes no ordinary kill: the server announces a
    // rare find by name, and only corpses that appeared around this
    // character's own announcement are described.

    private static (LootController Controller, Automation Automation) RareOnlyLooter(
        params uint[] corpses)
    {
        var settings = new LootSettings { Enabled = true, LootOnlyRareCorpses = true };
        settings.Rules.Add(new LootRule { Expression = "*", Action = LootAction.NoLoot });
        var automation = new Automation
        {
            Corpses = [.. corpses.Select(static id =>
                new PluginLootContainer(id, 1u, "Corpse of Drudge", 3f, false, false, false))],
        };
        return (new LootController(new Host(automation), settings), automation);
    }

    private static void Announce(Automation automation, ulong sequence, string text) =>
        automation.ChatMessages.Add(new PluginChatMessage(
            sequence, 0u, 0, string.Empty, text, string.Empty));

    [Fact]
    public void RareOnlyLootingDescribesNoCorpseUntilTheServerAnnouncesARare()
    {
        (LootController controller, Automation automation) =
            RareOnlyLooter(0x70001001u, 0x70001002u);

        for (int pass = 0; pass < 10; pass++)
        {
            controller.Tick(0.5d, canAct: true);
            controller.TickIdentification(0.5d);
        }

        Assert.Empty(automation.Identified);
        Assert.False(controller.HasCorpseAwaitingDescriptionWithin(50d));
    }

    [Fact]
    public void ThisCharactersRareAnnouncementDescribesTheCorpseThatJustAppeared()
    {
        (LootController controller, Automation automation) =
            RareOnlyLooter(0x70001001u);
        controller.Tick(0.5d, canAct: true);
        controller.Tick(20d, canAct: true);
        controller.TickIdentification(0.5d);

        automation.Corpses =
        [
            .. automation.Corpses,
            new PluginLootContainer(0x70001002u, 1u, "Corpse of Drudge", 3f, false, false, false),
        ];
        controller.Tick(0.5d, canAct: true);
        Announce(automation, 1uL, "Tester has discovered the Pearl of Blood Drinking!");
        controller.Tick(0.5d, canAct: true);
        controller.TickIdentification(0.5d);

        // The corpse from twenty seconds earlier is not the rare's.
        Assert.Equal([0x70001002u], automation.Identified);
        Assert.True(controller.HasCorpseAwaitingDescriptionWithin(50d));
    }

    [Fact]
    public void AnotherPlayersRareAnnouncementDescribesNothing()
    {
        (LootController controller, Automation automation) =
            RareOnlyLooter(0x70001001u);
        Announce(automation, 1uL, "Someone Else has discovered the Pearl of Blood Drinking!");
        Announce(automation, 2uL, "Tester says, \"Tester has discovered the Pearl of Blood Drinking!\"");

        controller.Tick(0.5d, canAct: true);
        controller.TickIdentification(0.5d);

        Assert.Empty(automation.Identified);
    }

    [Fact]
    public void ARareAnnouncementStopsDescribingNewCorpsesAfterItsWindow()
    {
        (LootController controller, Automation automation) =
            RareOnlyLooter(0x70001001u);
        Announce(automation, 1uL, "+Tester has discovered the Pearl of Blood Drinking!");
        controller.Tick(0.5d, canAct: true);
        controller.TickIdentification(0.5d);
        Assert.Equal([0x70001001u], automation.Identified);

        controller.Tick(LootController.RareAnnouncementWindowSeconds + 1d, canAct: true);
        automation.Corpses =
        [
            .. automation.Corpses,
            new PluginLootContainer(0x70001002u, 1u, "Corpse of Drudge", 3f, false, false, false),
        ];
        automation.AppraisalState = automation.AppraisalState with { AwaitingObjectId = 0u };
        controller.Tick(0.5d, canAct: true);
        controller.TickIdentification(5d);

        Assert.DoesNotContain(0x70001002u, automation.Identified);
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

    /// <summary>
    /// Mutation <c>DropWaitingPickupReceipt</c>: clear the receipt when the
    /// item remains in the corpse. The busy retry then loses the Read action,
    /// classifier notification and reader queue after the original pickup lands.
    /// </summary>
    [Fact]
    public void AcceptedReadPickupKeepsItsReceiptAcrossBusyRetryUntilTransfer()
    {
        const uint corpse = 0x70000280u;
        const uint scroll = 0x70000281u;
        var settings = new LootSettings
        {
            Enabled = true,
            ExternalClassifierId = "classifier/read",
        };
        settings.Rules.Add(new LootRule { Expression = "*", Action = LootAction.NoLoot });
        PluginInventoryItem scrollItem = Scroll(scroll, "Incantation of Testing", 777u);
        var automation = new Automation
        {
            KnownSpell = new PluginSpellInfo(
                777u, "Incantation of Testing", 1u, 1, 100, 10, 0f,
                34u, string.Empty, false, false),
            SkillsValue =
            [
                new PluginSkillInfo(
                    34u, "War Magic", PluginSkillTraining.Trained, 90u),
            ],
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
        var classifier = new ClassifierRegistry(
            "classifier/read",
            new PluginLootClassification(true, PluginLootAction.Read, "Read it"));
        var host = new Host(automation, classifier);
        var controller = new LootController(host, settings);
        var locks = new ActionLockTable();
        controller.BindActionLocks(locks);

        Assert.True(controller.Tick(0.25d, canAct: true));
        automation.Current = corpse;
        automation.Contents = [scrollItem];
        locks.Advance(settings.CorpseOpenTimeoutSeconds + 0.1d);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.CompleteAppraisal(scroll, presentInUi: false);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([scroll], automation.Picked);

        locks.Advance(1d);
        automation.PickupResults.Enqueue(new(PluginItemCommandStatus.Busy));
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([scroll, scroll], automation.Picked);
        Assert.Empty(classifier.Looted);

        automation.Contents = [];
        automation.Owned = [scrollItem];
        automation.InventoryCompletion = new PluginInventoryCompletion(
            1, PluginInventoryCommandKind.Pickup, scroll, 0u);
        // A busy pull holds the item slot exactly as a sent one does, so the
        // turn that reads the transfer is the one after the hold runs out.
        locks.Advance(1d);
        Assert.True(controller.Tick(0.1d, canAct: true));

        Assert.Equal(LootAction.Read, controller.ClassifiedOwnedItems[scroll]);
        Assert.Equal(scroll, controller.PendingScrollReads[777u]);
        PluginLootedItem looted = Assert.Single(classifier.Looted);
        Assert.Equal(scroll, looted.Item.ObjectId);
        Assert.Equal(PluginLootAction.Read, looted.Action);

        var reader = new ReadScrollController(host, settings, controller);
        Assert.True(reader.Tick(0.1d, canAct: true));
        Assert.Equal([scroll], automation.Used);
    }

    /// <summary>Mutation <c>DropWaitingPickupReceipt</c>: release the reservation before the transfer arrives.</summary>
    [Fact]
    public void KeepUpToReceiptRemainsReservedAcrossBusyRetry()
    {
        const uint corpse = 0x70000290u;
        const uint first = 0x70000291u;
        const uint second = 0x70000292u;
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(new LootRule
        {
            Expression = "*",
            Action = LootAction.KeepUpTo,
            KeepCount = 1,
        });
        PluginInventoryItem firstItem = Item(first, "Limited prize", 99u);
        PluginInventoryItem secondItem = Item(second, "Limited prize", 99u);
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
        var locks = new ActionLockTable();
        controller.BindActionLocks(locks);

        Assert.True(controller.Tick(0.25d, canAct: true));
        automation.Current = corpse;
        automation.Contents = [firstItem];
        locks.Advance(settings.CorpseOpenTimeoutSeconds + 0.1d);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.2d, canAct: true));
        Assert.Equal([first], automation.Picked);

        locks.Advance(1d);
        automation.Contents = [firstItem, secondItem];
        automation.PickupResults.Enqueue(new(PluginItemCommandStatus.Busy));
        Assert.True(controller.Tick(0.1d, canAct: true));
        controller.TickIdentification(0.5d);

        automation.Contents = [secondItem];
        automation.Owned = [firstItem];
        Assert.True(controller.Tick(0.1d, canAct: true));

        Assert.Equal([first, first], automation.Picked);
    }

    /// <summary>
    /// A receipt at its retry ceiling is abandoned before another item is
    /// selected, so the next transfer keeps its own classifier and reader
    /// work.
    /// Mutation: remove the exhausted-receipt reconciliation before candidate
    /// selection in <c>ContinueCurrentCorpse</c>; the second pickup has no
    /// receipt and the reader queue assertion fails.
    /// </summary>
    [Fact]
    public void ExhaustedPickupReceiptDoesNotStealTheNextItemsTransfer()
    {
        const uint corpse = 0x700002A0u;
        const uint first = 0x700002A1u;
        const uint second = 0x700002A2u;
        const uint spell = 778u;
        var settings = new LootSettings
        {
            Enabled = true,
            ExternalClassifierId = "classifier/read",
            CorpseLootItemMaxAttempts = 1,
        };
        settings.Rules.Add(new LootRule { Expression = "*", Action = LootAction.NoLoot });
        PluginInventoryItem firstItem = Scroll(first, "First scroll", spell);
        PluginInventoryItem secondItem = Scroll(second, "Second scroll", spell);
        var automation = new Automation
        {
            KnownSpell = new PluginSpellInfo(
                spell, "Second scroll", 1u, 1, 100, 10, 0f, 34u,
                string.Empty, false, false),
            SkillsValue = [new PluginSkillInfo(34u, "War Magic", PluginSkillTraining.Trained, 90u)],
            Corpses = [new PluginLootContainer(corpse, 1u, "Corpse", 3f, false, false, false)
            {
                IsIdentified = true,
                LongDescription = "Killed by Tester.",
            }],
        };
        var classifier = new ClassifierRegistry(
            "classifier/read",
            new PluginLootClassification(true, PluginLootAction.Read, "Read it"));
        var host = new Host(automation, classifier);
        var controller = new LootController(host, settings);
        var locks = new ActionLockTable();
        controller.BindActionLocks(locks);

        Assert.True(controller.Tick(0.25d, canAct: true));
        automation.Current = corpse;
        automation.Contents = [firstItem, secondItem];
        locks.Advance(settings.CorpseOpenTimeoutSeconds + 0.1d);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.CompleteAppraisal(first, presentInUi: false);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([first], automation.Picked);

        locks.Advance(1d);
        automation.ItemsBusy = true;
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([first], automation.Picked);
        automation.ItemsBusy = false;
        automation.InventoryCompletion = new PluginInventoryCompletion(
            1, PluginInventoryCommandKind.Pickup, first, 1u);
        Assert.True(controller.Tick(0.1d, canAct: true));
        automation.CompleteAppraisal(second, presentInUi: false);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(second, automation.Picked[^1]);

        locks.Advance(1d);
        automation.Contents = [firstItem];
        automation.Owned = [secondItem];
        Assert.True(controller.Tick(0.1d, canAct: true));

        Assert.DoesNotContain(first, controller.ClassifiedOwnedItems.Keys);
        Assert.Equal(LootAction.Read, controller.ClassifiedOwnedItems[second]);
        Assert.Equal(second, controller.PendingScrollReads[spell]);
        Assert.Equal(second, Assert.Single(classifier.Looted).Item.ObjectId);
        var reader = new ReadScrollController(host, settings, controller);
        Assert.True(reader.Tick(0.1d, canAct: true));
        Assert.Equal([second], automation.Used);
    }

    /// <summary>
    /// Mutation <c>IgnoreOwnedReceiptOnContainerChange</c>: replace the owned
    /// receipt check in <c>LootController.Tick</c> with <c>false</c>; this
    /// test then drops the transferred scroll instead of classifying and
    /// queuing it exactly once.
    /// </summary>
    [Fact]
    public void ClosedCorpseWithOwnedReceiptClassifiesAndQueuesExactlyOnce()
    {
        const uint corpse = 0x700002B0u;
        const uint scroll = 0x700002B1u;
        const uint spell = 779u;
        var settings = new LootSettings { Enabled = true, ExternalClassifierId = "classifier/read" };
        settings.Rules.Add(new LootRule { Expression = "*", Action = LootAction.NoLoot });
        PluginInventoryItem scrollItem = Scroll(scroll, "Transferred scroll", spell);
        var automation = new Automation
        {
            KnownSpell = new PluginSpellInfo(spell, "Transferred scroll", 1u, 1, 100, 10, 0f, 34u, string.Empty, false, false),
            SkillsValue = [new PluginSkillInfo(34u, "War Magic", PluginSkillTraining.Trained, 90u)],
            Corpses = [new PluginLootContainer(corpse, 1u, "Corpse", 3f, false, false, false) { IsIdentified = true, LongDescription = "Killed by Tester." }],
        };
        var classifier = new ClassifierRegistry("classifier/read", new PluginLootClassification(true, PluginLootAction.Read, "Read it"));
        var host = new Host(automation, classifier);
        var controller = new LootController(host, settings);
        var locks = new ActionLockTable();
        controller.BindActionLocks(locks);

        Assert.True(controller.Tick(0.25d, canAct: true));
        automation.Current = corpse;
        automation.Contents = [scrollItem];
        locks.Advance(settings.CorpseOpenTimeoutSeconds + 0.1d);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.CompleteAppraisal(scroll, presentInUi: false);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal([scroll], automation.Picked);

        locks.Advance(1d);
        automation.Current = 0u;
        automation.Owned = [scrollItem];
        Assert.True(controller.Tick(0.1d, canAct: true));

        Assert.Equal(LootAction.Read, controller.ClassifiedOwnedItems[scroll]);
        Assert.Equal(scroll, controller.PendingScrollReads[spell]);
        Assert.Equal(scroll, Assert.Single(classifier.Looted).Item.ObjectId);
        var reader = new ReadScrollController(host, settings, controller);
        Assert.True(reader.Tick(0.1d, canAct: true));
        Assert.Equal([scroll], automation.Used);

        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Single(classifier.Looted);
        Assert.Equal([scroll], automation.Used);
    }
    [Fact]
    public void ClosedCorpseWithoutOwnedReceiptAbandonsItWithoutClassification()
    {
        const uint corpse = 0x700002C0u;
        const uint scroll = 0x700002C1u;
        const uint spell = 780u;
        var settings = new LootSettings
        {
            Enabled = true,
            ExternalClassifierId = "classifier/read",
        };
        settings.Rules.Add(new LootRule { Expression = "*", Action = LootAction.NoLoot });
        PluginInventoryItem scrollItem = Scroll(scroll, "Absent scroll", spell);
        var automation = new Automation
        {
            KnownSpell = new PluginSpellInfo(
                spell, "Absent scroll", 1u, 1, 100, 10, 0f, 34u,
                string.Empty, false, false),
            SkillsValue = [new PluginSkillInfo(34u, "War Magic", PluginSkillTraining.Trained, 90u)],
            Corpses = [new PluginLootContainer(corpse, 1u, "Corpse", 3f, false, false, false)
            {
                IsIdentified = true,
                LongDescription = "Killed by Tester.",
            }],
        };
        var classifier = new ClassifierRegistry(
            "classifier/read",
            new PluginLootClassification(true, PluginLootAction.Read, "Read it"));
        var controller = new LootController(new Host(automation, classifier), settings);
        var locks = new ActionLockTable();
        controller.BindActionLocks(locks);

        Assert.True(controller.Tick(0.25d, canAct: true));
        automation.Current = corpse;
        automation.Contents = [scrollItem];
        locks.Advance(settings.CorpseOpenTimeoutSeconds + 0.1d);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.CompleteAppraisal(scroll, presentInUi: false);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.1d, canAct: true));

        locks.Advance(1d);
        automation.Current = 0u;
        Assert.True(controller.Tick(0.1d, canAct: true));

        Assert.DoesNotContain(scroll, controller.ClassifiedOwnedItems.Keys);
        Assert.Empty(controller.PendingScrollReads);
        Assert.Empty(classifier.Looted);
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
        // The pull holds the item slot; without the slot a second turn is a
        // second pull of the same scroll, as the reference would make it.
        controller.BindActionLocks(new ActionLockTable());
        const uint scroll = 0x70000201u;

        Assert.True(controller.Tick(0.25d, canAct: true));
        automation.Current = 0x70000200u;
        Assert.True(controller.ObserveCorpseOpened());
        automation.Contents =
        [
            Scroll(scroll, "Incantation of Testing", 777u),
        ];
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.AppraisalState = new PluginAppraisalState(1, 0u, scroll);
        controller.TickIdentification(0.5d);
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
    public void SalvageRuleRunsRealAuthenticSalvageAfterCorpseIsComplete()
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
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.AppraisalState = new PluginAppraisalState(1, 0u, source);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.1d, canAct: true));
        automation.Contents = [];
        automation.Owned = [automation.Owned[0], Item(source, "Iron Sword", 100u)];
        automation.InventoryCompletion = new PluginInventoryCompletion(
            1,
            PluginInventoryCommandKind.Pickup,
            source,
            0u);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.1d, canAct: true));
        // The pass the corpse finishes on is the pass that closes it, and the
        // corpse stays this controller's business until the container shuts.
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.Current = 0u;

        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { (tool, source) }, automation.Salvaged);
        automation.Owned = [automation.Owned[0]];
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal("Salvaged Iron Sword.", controller.Status);
    }

    /// <summary>
    /// Without a salvage tool the loot work goes on: the item stays marked,
    /// the player is told once in chat, and the item is salvaged the moment a
    /// tool is carried. Mutation: stop the loot pass while the tool is missing
    /// and the next corpse is never opened; drop the once-only flag and the
    /// warning repeats every pass.
    /// </summary>
    [Fact]
    public void AMissingSalvageToolWarnsOnceAndLootingCarriesOn()
    {
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(new LootRule { Expression = "*", Action = LootAction.Salvage });
        const uint source = 0x70000411u;
        const uint tool = 0x50000412u;
        const uint nextCorpse = 0x70000413u;
        var automation = new Automation { Owned = [Item(source, "Iron Sword", 100u)] };
        var controller = new LootController(new Host(automation), settings);
        var warnings = new List<string>();
        controller.Warning = warnings.Add;
        controller.MarkOwnedForTest(source, LootAction.Salvage);

        controller.Tick(0.1d, canAct: true);
        controller.Tick(0.1d, canAct: true);
        automation.Corpses =
        [
            new PluginLootContainer(nextCorpse, 1u, "Corpse", 3f, false, false, false)
            {
                IsIdentified = true,
                LongDescription = "Killed by Tester.",
            },
        ];
        controller.Tick(0.3d, canAct: true);

        Assert.Empty(automation.Salvaged);
        Assert.Contains("Ust", Assert.Single(warnings));
        Assert.Contains(nextCorpse, automation.Opened);

        automation.Corpses = [];
        automation.Current = 0u;
        automation.Owned =
        [
            automation.Owned[0],
            Item(tool, "Ust", 99u) with { ItemType = 0x20000000u },
        ];
        for (int i = 0; i < 20 && automation.Salvaged.Count == 0; i++)
            controller.Tick(0.3d, canAct: true);

        Assert.Equal(new[] { (tool, source) }, automation.Salvaged);
        Assert.Single(warnings);
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
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.AppraisalState = new PluginAppraisalState(1, 0u, source);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.1d, canAct: true));
        automation.Contents = [];
        automation.Owned = [Item(source, "Ruby", 101u)];
        automation.InventoryCompletion = new PluginInventoryCompletion(
            1,
            PluginInventoryCommandKind.Pickup,
            source,
            0u);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.1d, canAct: true));
        // The pass the corpse finishes on is the pass that closes it, and the
        // corpse stays this controller's business until the container shuts.
        Assert.True(controller.Tick(0.2d, canAct: true));
        automation.Current = 0u;

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
        controller.TickIdentification(0.5d);
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
        controller.TickIdentification(0.5d);
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

    /// <summary>
    /// Whether a spell's amount multiplies or adds is decided by truncating
    /// the authored operation to a whole number and asking whether it is one.
    /// Every row of the table authors the same number for the operation as for
    /// the amount, so this pins the choice, not where it is read from.
    /// Mutation: swap the two branches.
    /// </summary>
    [Theory]
    // Spell 3199 authors 1.10, which truncates to 1 and multiplies.
    [InlineData(3199u, 144u, 1d, 1.10d, true)]
    [InlineData(3199u, 144u, 1d, 1.11d, false)]
    // Spell 2588 authors 0.05, which truncates to 0 and adds.
    [InlineData(2588u, 29u, 0.10d, 0.15d, true)]
    [InlineData(2588u, 29u, 0.10d, 0.16d, false)]
    public void ABuffedDoubleMultipliesOnlyWhenTheAuthoredOperationIsOne(
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
                    mundane, 1u, "Corpse", 2f, false, false, false)
                {
                    IsIdentified = true,
                    LongDescription = "Killed by Tester.",
                },
                new PluginLootContainer(
                    rare, 1u, "Corpse", 4.5f, false, false, false)
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

    /// <summary>
    /// Mutation: score the open pick by distance instead of by how far round
    /// the character would have to turn.
    /// </summary>
    [Fact]
    public void TheCorpseInReachTheCharacterIsFacingIsOpenedFirst()
    {
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(new LootRule { Expression = "*" });
        const uint behind = 0x70000F00u;
        const uint ahead = 0x70000F01u;
        var automation = new Automation
        {
            NavigationSnapshot = new PluginNavigationSnapshot(
                true,
                false,
                Player,
                Place(0d, 0d, headingDegrees: 0f),
                false,
                false),
            Corpses =
            [
                new PluginLootContainer(
                    behind, 1u, "Corpse", 2f, false, false, false)
                {
                    IsIdentified = true,
                    LongDescription = "Killed by Tester.",
                    HasPosition = true,
                    Position = Place(0d, -2d),
                },
                new PluginLootContainer(
                    ahead, 1u, "Corpse", 4f, false, false, false)
                {
                    IsIdentified = true,
                    LongDescription = "Killed by Tester.",
                    HasPosition = true,
                    Position = Place(0d, 4d),
                },
            ],
        };
        var controller = new LootController(new Host(automation), settings);

        Assert.True(controller.Tick(0.25d, canAct: true));
        Assert.Equal(new[] { ahead }, automation.Opened);
    }

    /// <summary>
    /// Mutation: keep every corpse record for the life of the session instead
    /// of dropping the ones the client has stopped reporting.
    /// </summary>
    [Fact]
    public void ACorpseForgottenAfterTheCacheTimeoutStartsItsAgeClockAgain()
    {
        var settings = new LootSettings
        {
            Enabled = true,
            LootAllCorpses = true,
            CorpseCacheTimeoutMinutes = 1d,
            ScanIntervalSeconds = 0.05d,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        const uint corpse = 0x70001200u;
        PluginLootContainer container = new(
            corpse, 1u, "Corpse", 3f, false, false, false)
        {
            IsIdentified = true,
            LongDescription = "Killed by Someone Else.",
        };
        var automation = new Automation { Corpses = [container] };
        var controller = new LootController(new Host(automation), settings);

        Assert.False(controller.Tick(0.25d, canAct: true));

        // Gone from the client for longer than the cache timeout: forgotten.
        automation.Corpses = [];
        Assert.False(controller.Tick(200d, canAct: true));

        // Back again, and as far as the ownership timer is concerned it has
        // only just been seen for the first time.
        automation.Corpses = [container];
        Assert.False(controller.Tick(200d, canAct: true));
        Assert.Empty(automation.Opened);

        Assert.True(controller.Tick(200d, canAct: true));
        Assert.Equal(new[] { corpse }, automation.Opened);
    }

    /// <summary>
    /// The open step reaches exactly as far as arm's reach and no further. A
    /// corpse twelve metres off is not opened from where the character stands,
    /// however long it waits: that corpse belongs to the walk, which is its own
    /// rule one position above this one.
    ///
    /// Mutation: give the open a second pick at the approach range when nothing
    /// is in reach, and the far corpse is opened from across the field.
    /// </summary>
    [Fact]
    public void TheOpenStepDoesNotReachBeyondArmsReach()
    {
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(new LootRule { Expression = "*" });
        const uint near = 0x70001000u;
        const uint far = 0x70001001u;
        var automation = new Automation
        {
            Corpses =
            [
                new PluginLootContainer(
                    far, 1u, "Corpse", 30f, false, false, false)
                {
                    IsIdentified = true,
                    LongDescription = "Killed by Tester.",
                },
                new PluginLootContainer(
                    near, 1u, "Corpse", 12f, false, false, false)
                {
                    IsIdentified = true,
                    LongDescription = "Killed by Tester.",
                },
            ],
        };
        var controller = new LootController(new Host(automation), settings);

        Assert.False(controller.Tick(0.25d, canAct: true));
        Assert.Empty(automation.Opened);

        // Walked to: the same corpse, now inside the open's own reach.
        automation.Corpses =
        [
            automation.Corpses[0],
            automation.Corpses[1] with { Distance = 3f },
        ];
        Assert.True(controller.Tick(0.25d, canAct: true));
        Assert.Equal(new[] { near }, automation.Opened);
    }

    /// <summary>
    /// Mutation: read the rare flag off the wire property alone, ignoring a
    /// description that is not a kill description.
    /// </summary>
    [Fact]
    public void ACorpseWhoseDescriptionNamesNoKillerIsTreatedAsRare()
    {
        var settings = new LootSettings
        {
            Enabled = true,
            LootAllCorpses = true,
            ScanIntervalSeconds = 0.05d,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        const uint unattributed = 0x70001100u;
        var automation = new Automation
        {
            Corpses =
            [
                new PluginLootContainer(
                    unattributed, 1u, "Corpse", 3f, false, false, false)
                {
                    IsIdentified = true,
                    LongDescription = "Generated treasure",
                },
            ],
        };
        var controller = new LootController(new Host(automation), settings);

        // Rare, so it sorts first; nobody killed it, so it is never looted —
        // not even once it is old enough for the loot-anything timer.
        Assert.False(controller.Tick(200d, canAct: true));
        Assert.False(controller.Tick(200d, canAct: true));
        Assert.Empty(automation.Opened);
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

    /// <summary>
    /// The open step carries its own reach and does not borrow the approach
    /// range. The approach range ships at zero, which is the setting a fresh
    /// profile has, and a corpse three metres away is still opened.
    ///
    /// Mutation: narrow the scan to the approach range before selecting, and
    /// the corpse is never opened.
    /// </summary>
    [Fact]
    public void TheOpenStepReachesACorpseTheApproachRangeDoesNot()
    {
        var settings = new LootSettings
        {
            Enabled = true,
            CorpseApproachRange = 0d,
            ScanIntervalSeconds = 0.05d,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        const uint corpse = 0x70000C10u;
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
        Assert.Equal(new[] { corpse }, automation.Opened);
    }

    /// <summary>
    /// Opening a corpse holds navigation as well as the item slot. A corpse
    /// out of the server's own reach is opened by walking to it first, and
    /// that walk is the client's — a route rule steering at the same time
    /// cancels it, and the open then answers nothing at all.
    ///
    /// Mutation: drop the navigation arm and the slot is free the moment the
    /// open is issued.
    /// </summary>
    [Fact]
    public void OpeningACorpseHoldsTheSlotsTheWalkNeeds()
    {
        var settings = new LootSettings
        {
            Enabled = true,
            ScanIntervalSeconds = 0.05d,
            CorpseOpenTimeoutSeconds = 1.5d,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        const uint corpse = 0x70000C11u;
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
        var locks = new ActionLockTable();
        var controller = new LootController(new Host(automation), settings);
        controller.BindActionLocks(locks);

        Assert.False(locks.IsLocked(ActionLockKind.Navigation));
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { corpse }, automation.Opened);

        Assert.True(locks.IsLocked(ActionLockKind.Navigation));
        Assert.True(locks.IsLocked(ActionLockKind.ItemUse));
        Assert.True(locks.IsLocked(ActionLockKind.CorpseOpenAttempt));

        locks.Advance(1.6d);
        Assert.False(locks.IsLocked(ActionLockKind.Navigation));
    }

    /// <summary>
    /// The three slots go back the instant the container is open, not when the
    /// open's own window runs out. The corpse-open slot is the marker that
    /// says the three belong to an open, so it is both the test and the first
    /// released, and a second look releases nothing.
    ///
    /// Mutation: drop any one of the three releases and that slot is still
    /// held; drop the whole observer and all three are.
    /// </summary>
    [Fact]
    public void TheSlotsGoBackTheMomentTheCorpseIsOpen()
    {
        var settings = new LootSettings
        {
            Enabled = true,
            ScanIntervalSeconds = 0.05d,
            CorpseOpenTimeoutSeconds = 30d,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        const uint corpse = 0x70000C12u;
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
        var locks = new ActionLockTable();
        var controller = new LootController(new Host(automation), settings);
        controller.BindActionLocks(locks);

        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { corpse }, automation.Opened);
        // Nothing has opened yet, so nothing goes back.
        Assert.False(controller.ObserveCorpseOpened());
        Assert.True(locks.IsLocked(ActionLockKind.ItemUse));

        automation.Requested = corpse;
        automation.Current = corpse;

        Assert.True(controller.ObserveCorpseOpened());
        Assert.False(locks.IsLocked(ActionLockKind.CorpseOpenAttempt));
        Assert.False(locks.IsLocked(ActionLockKind.Navigation));
        Assert.False(locks.IsLocked(ActionLockKind.ItemUse));
        Assert.False(controller.ObserveCorpseOpened());
    }

    /// <summary>
    /// What the release is for: with a long open window, the pass that follows
    /// the container opening reads the corpse and judges it. Both rules that
    /// carry this controller are gated on the item slot being free, so a slot
    /// still held here is not a pause — it is the looter unable to look at the
    /// corpse it just opened, which is the reported symptom exactly.
    ///
    /// Mutation: remove the observer call and the item slot is still held
    /// thirty seconds later, so the gate this asserts is closed.
    /// </summary>
    [Fact]
    public void AfterTheCorpseOpensTheLooterJudgesOnTheNextPass()
    {
        var settings = new LootSettings
        {
            Enabled = true,
            ScanIntervalSeconds = 0.05d,
            CorpseOpenTimeoutSeconds = 30d,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        const uint corpse = 0x70000C13u;
        const uint prize = 0x70000C14u;
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
        var locks = new ActionLockTable();
        var controller = new LootController(new Host(automation), settings);
        controller.BindActionLocks(locks);
        var judged = new List<string>();
        controller.Log = (_, line) => judged.Add(line);

        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { corpse }, automation.Opened);

        automation.Requested = corpse;
        automation.Current = corpse;
        automation.Contents = [Item(prize, "Plain Lockpick", 1)];
        Assert.True(controller.ObserveCorpseOpened());

        // The gate both loot rules are held by is what the release opens.
        Assert.False(locks.IsLocked(ActionLockKind.ItemUse));
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.2d, canAct: true));
        Assert.Contains(
            judged,
            line => line.StartsWith("LootDecision: ", StringComparison.Ordinal));
    }

    /// <summary>
    /// The description is asked of every corpse the client is reporting, not
    /// only of the ones already in reach: a corpse whose description never
    /// arrives can never be judged, and by the time the character walks up to
    /// it there is nothing to walk up for. The frame asks; the rule's own
    /// turn reaches only as far as the character would walk, so a corpse a
    /// field away never costs the route a pass.
    ///
    /// Mutation: narrow the frame scan to the approach range before asking,
    /// and the far corpse is never identified.
    /// </summary>
    [Fact]
    public void ACorpseBeyondTheApproachRangeIsStillAskedForItsDescription()
    {
        var settings = new LootSettings
        {
            Enabled = true,
            CorpseApproachRange = 0d,
            ScanIntervalSeconds = 0.05d,
        };
        settings.Rules.Add(new LootRule { Expression = "*" });
        const uint corpse = 0x70000C20u;
        var automation = new Automation
        {
            Corpses =
            [
                new PluginLootContainer(
                    corpse, 1u, "Corpse", 30f, false, false, false),
            ],
        };
        var controller = new LootController(new Host(automation), settings);

        // The rule's turn declines: nothing within reach.
        Assert.False(controller.Tick(0.1d, canAct: true));
        Assert.Empty(automation.Identified);

        controller.TickIdentification(0.1d);
        Assert.Equal(new[] { corpse }, automation.Identified);
        Assert.Empty(automation.Opened);
    }

    [Fact]
    public void AFinishedCorpseIsClosedWithASecondUse()
    {
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(new LootRule
        {
            Expression = "*",
            Action = LootAction.NoLoot,
        });
        const uint corpse = 0x70000B00u;
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
        automation.Contents = [Item(0x70000B01u, "Rock", 273u)];
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.2d, canAct: true));

        Assert.Equal("Corpse complete.", controller.Status);
        Assert.Equal(new[] { corpse }, automation.Closed);
        Assert.Empty(automation.Used);
    }

    /// <summary>
    /// Mutation: drop the <c>canAct</c> / availability / busy gate from
    /// <c>CloseFinishedCorpse</c>.
    /// </summary>
    [Fact]
    public void AFinishedCorpseIsNotClosedUntilTheItemChannelIsFree()
    {
        (LootController controller, Automation automation, uint corpse) =
            FinishedCorpseScenario(itemsBusy: true);

        Assert.Empty(automation.Closed);
        Assert.Equal("Waiting to close corpse…", controller.Status);

        // A pass the controller may not act on holds it back just as much.
        automation.ItemsBusy = false;
        Assert.True(controller.Tick(0.1d, canAct: false));
        Assert.Empty(automation.Closed);

        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { corpse }, automation.Closed);
    }

    /// <summary>
    /// Mutation: clear the corpse and answer false when the closing use is
    /// refused, instead of keeping it and trying again.
    /// </summary>
    [Fact]
    public void ARefusedCloseIsTriedAgainUntilTheContainerActuallyShuts()
    {
        (LootController controller, Automation automation, uint corpse) =
            FinishedCorpseScenario(itemsBusy: false);

        Assert.Equal(new[] { corpse }, automation.Closed);

        // The server did not shut the container, so the corpse is still this
        // controller's business and the use comes round again.
        Assert.True(controller.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { corpse, corpse }, automation.Closed);
        Assert.Empty(automation.Opened.Skip(1));

        automation.Current = 0u;
        Assert.False(controller.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { corpse, corpse }, automation.Closed);
    }

    /// <summary>Opens one corpse holding nothing worth taking.</summary>
    private static (LootController Controller, Automation Automation,
        uint Corpse) FinishedCorpseScenario(bool itemsBusy)
    {
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(new LootRule
        {
            Expression = "*",
            Action = LootAction.NoLoot,
        });
        const uint corpse = 0x70000E00u;
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
        automation.Contents = [Item(0x70000E01u, "Rock", 273u)];
        automation.ItemsBusy = itemsBusy;
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.2d, canAct: true));
        return (controller, automation, corpse);
    }

    /// <summary>
    /// Mutation: take the scroll off the queue when the read fails or never
    /// answers, instead of only when the item is gone.
    /// </summary>
    [Fact]
    public void AFailedScrollReadIsTriedAgainInsteadOfDroppingTheScroll()
    {
        (LootController controller, ReadScrollController reader,
            Automation automation, uint scroll, _) = ScrollScenario();

        Assert.True(reader.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { scroll }, automation.Used);

        // The server said no.
        automation.UseCompletion =
            new PluginItemUseCompletion(1L, scroll, 0u, 7u);
        Assert.True(reader.Tick(0.1d, canAct: true));
        Assert.Equal(
            new Dictionary<uint, uint> { [777u] = scroll },
            controller.PendingScrollReads);

        Assert.True(reader.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { scroll, scroll }, automation.Used);
    }

    /// <summary>
    /// Mutation: drop the call that sheds queued scrolls the character no
    /// longer holds.
    /// </summary>
    [Fact]
    public void AQueuedScrollTheCharacterNoLongerHoldsLeavesTheQueue()
    {
        (LootController controller, ReadScrollController reader,
            Automation automation, uint scroll, _) = ScrollScenario();

        Assert.True(reader.Tick(0.1d, canAct: true));
        automation.UseCompletion =
            new PluginItemUseCompletion(1L, scroll, 0u, 0u);
        automation.Owned = [];
        Assert.True(reader.Tick(0.1d, canAct: true));

        Assert.False(reader.Tick(0.1d, canAct: true));
        Assert.Empty(controller.PendingScrollReads);
        Assert.Equal(new[] { scroll }, automation.Used);
    }

    /// <summary>
    /// Mutation: gate the reading rule on the looting switch.
    /// </summary>
    [Fact]
    public void AQueuedScrollIsStillReadAfterLootingIsSwitchedOff()
    {
        (_, ReadScrollController reader, Automation automation, uint scroll,
            LootSettings settings) = ScrollScenario();
        settings.Enabled = false;

        Assert.True(reader.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { scroll }, automation.Used);
    }

    [Fact]
    public void APickedUpScrollIsQueuedForTheReadingRuleInsteadOfReadInline()
    {
        (LootController controller, ReadScrollController reader,
            Automation automation, uint scroll, _) = ScrollScenario();

        Assert.Empty(automation.Used);
        Assert.Equal(
            new Dictionary<uint, uint> { [777u] = scroll },
            controller.PendingScrollReads);

        Assert.True(reader.Tick(0.1d, canAct: true));
        Assert.Equal(new[] { scroll }, automation.Used);
    }

    /// <summary>
    /// Mutation: test the item's misc type flag plus a name ending in
    /// " Scroll" instead of the object class.
    /// </summary>
    [Fact]
    public void AMiscItemThatMerelyLooksLikeAScrollIsNotRead()
    {
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(new LootRule
        {
            Expression = "*",
            Action = LootAction.NoLoot,
        });
        const uint corpse = 0x70000D00u;
        const uint fake = 0x70000D01u;
        var automation = new Automation
        {
            KnownSpell = new PluginSpellInfo(
                777u, "Incantation of Testing", 1u, 1, 100, 10, 0f,
                34u, string.Empty, false, false),
            SkillsValue =
            [
                new PluginSkillInfo(
                    34u, "War Magic", PluginSkillTraining.Trained, 90u),
            ],
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
        automation.Contents =
        [
            Item(fake, "Counterfeit Scroll", 88u) with
            {
                ItemType = 0x00000080u,
                SpellId = 777u,
                ObjectClass = PluginObjectClass.Misc,
            },
        ];
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.2d, canAct: true));

        Assert.Empty(automation.Picked);
        Assert.Empty(controller.PendingScrollReads);
    }

    [Fact]
    public void AQueuedScrollIsDroppedOnceItsSpellIsKnown()
    {
        (LootController controller, ReadScrollController reader,
            Automation automation, _, _) = ScrollScenario();
        automation.SpellLearned = true;

        Assert.False(reader.Tick(0.1d, canAct: true));
        Assert.Empty(automation.Used);
        Assert.Empty(controller.PendingScrollReads);
    }

    /// <summary>Loots one unknown scroll off a corpse and stops there.</summary>
    private static (LootController Controller, ReadScrollController Reader,
        Automation Automation, uint Scroll, LootSettings Settings)
        ScrollScenario()
    {
        var settings = new LootSettings { Enabled = true };
        settings.Rules.Add(new LootRule
        {
            Expression = "*",
            Action = LootAction.NoLoot,
        });
        const uint corpse = 0x70000C00u;
        const uint scroll = 0x70000C01u;
        var automation = new Automation
        {
            KnownSpell = new PluginSpellInfo(
                777u, "Incantation of Testing", 1u, 1, 100, 10, 0f,
                34u, string.Empty, false, false),
            SkillsValue =
            [
                new PluginSkillInfo(
                    34u, "War Magic", PluginSkillTraining.Trained, 90u),
            ],
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
        var host = new Host(automation);
        var controller = new LootController(host, settings);
        PluginInventoryItem item = Scroll(scroll, "Incantation of Testing", 777u);

        Assert.True(controller.Tick(0.25d, canAct: true));
        automation.Current = corpse;
        automation.Contents = [item];
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.2d, canAct: true));
        Assert.Equal(new[] { scroll }, automation.Picked);

        automation.Contents = [];
        automation.Owned = [item];
        automation.InventoryCompletion = new PluginInventoryCompletion(
            1, PluginInventoryCommandKind.Pickup, scroll, 0u);
        controller.TickIdentification(0.5d);
        Assert.True(controller.Tick(0.1d, canAct: true));

        return (
            controller,
            new ReadScrollController(host, settings, controller),
            automation,
            scroll,
            settings);
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

    /// <summary>A spot in the world, in metres east and north of nowhere.</summary>
    private static PluginNavigationPosition Place(
        double eastMeters,
        double northMeters,
        float headingDegrees = 0f) =>
        new(1u, eastMeters / 240d, northMeters / 240d, 0d, headingDegrees, true);

    /// <summary>
    /// A real scroll: a writable item carrying the spell it teaches, which is
    /// what makes the client classify it as a scroll. The name deliberately
    /// does NOT end in " Scroll" — many do not, and the shape is what decides.
    /// </summary>
    /// <summary>
    /// Reading scrolls the character cannot yet cast is a profile choice. With
    /// it off, a scroll that passes every other test -- an unknown spell, a
    /// school the character is skilled enough in -- is still not worth reading,
    /// so the looter never queues it and the reading rule never gets one.
    ///
    /// Mutation: drop the option from the eligibility test and the scroll is
    /// read whatever the profile says.
    /// </summary>
    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public void ReadingUnknownScrollsIsAProfileChoice(bool reads)
    {
        var automation = new Automation
        {
            KnownSpell = new PluginSpellInfo(
                777u, "Incantation of Testing", 1u, 1, 100, 10, 0f,
                34u, string.Empty, false, false),
            SkillsValue =
            [
                new PluginSkillInfo(
                    34u, "War Magic", PluginSkillTraining.Trained, 90u),
            ],
        };
        var settings = new LootSettings
        {
            Enabled = true,
            ReadUnknownScrolls = reads,
        };

        Assert.Equal(
            reads,
            ScrollReading.IsEligible(
                new Host(automation),
                settings,
                Scroll(0x70000C01u, "Incantation of Testing", 777u),
                new Dictionary<uint, uint>(),
                commit: true));
    }

    private static PluginInventoryItem Scroll(
        uint id,
        string spellName,
        uint spellId) =>
        Item(id, spellName, 88u) with
        {
            ItemType = 0x00002000u,
            SpellId = spellId,
            ObjectClass = PluginObjectClass.Scroll,
        };

    private static VtankLootRequirement Requirement(
        int type,
        params string[] lines) => new()
    {
        Type = type,
        Payload = string.Join("\r\n", lines) + "\r\n",
    };

    private sealed class Automation
        : IAutomationSurface, ICharacterInfo, ISpellCatalog, IItemAutomation,
          ILootAutomation, IFellowshipAutomation, IPluginChat,
          INavigationAutomation, IWorldObjectAutomation
    {
        public bool IsAvailable => true;
        public INavigationAutomation Navigation => this;
        public PluginNavigationSnapshot NavigationSnapshot { get; set; }
        public PluginNavigationSnapshot Snapshot => NavigationSnapshot;
        public bool TryGetObject(uint objectId, out PluginNavigationObject value)
        {
            value = default;
            return false;
        }
        public List<PluginMovementIntent> Intents { get; } = [];
        public int MovementClears { get; private set; }
        public List<float> FacedHeadings { get; } = [];
        public PluginNavigationCommandStatus SetMovementIntent(
            in PluginMovementIntent intent)
        {
            Intents.Add(intent);
            return PluginNavigationCommandStatus.Accepted;
        }
        public PluginNavigationCommandStatus ClearMovementIntent()
        {
            MovementClears++;
            return PluginNavigationCommandStatus.Accepted;
        }
        public PluginNavigationCommandStatus FaceHeading(float headingDegrees)
        {
            FacedHeadings.Add(headingDegrees);
            return PluginNavigationCommandStatus.Accepted;
        }
        public bool ItemsBusy { get; set; }
        bool IItemAutomation.IsBusy => ItemsBusy;
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
        public string Name { get; set; } = "Tester";
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
        public List<uint> Closed { get; } = [];
        public List<uint> Used { get; } = [];
        public PluginItemUseCompletion UseCompletion { get; set; }
        public PluginItemUseCompletion LastCompletion => UseCompletion;
        public PluginItemCommandResult Use(uint objectId)
        {
            Used.Add(objectId);
            return new(PluginItemCommandStatus.Started);
        }
        /// <summary>
        /// Every "use this on that" the looter issued. The mana drain is one
        /// of these, and a fake without it left the whole drain step
        /// unreachable from any test.
        /// </summary>
        public List<(uint Source, uint Target)> Applied { get; } = [];

        /// <summary>
        /// What the client answers the next "use this on that" with, oldest
        /// first; anything asked after the queue runs dry is accepted.
        /// </summary>
        public Queue<PluginItemCommandResult> ApplyResults { get; } = [];

        /// <summary>
        /// Models what a drain does to the pack: the stone the mana went into
        /// comes back charged and the item it came out of is gone, destroyed
        /// by the drain. Off by default so the tests that only care about
        /// what was asked stay as they were.
        /// </summary>
        public bool ManaDrainConsumesDonor { get; set; }

        private readonly List<(uint Stone, uint Donor)> _unsettledDrains = [];

        public PluginItemCommandResult Apply(uint objectId, uint targetObjectId)
        {
            Applied.Add((objectId, targetObjectId));
            PluginItemCommandResult answer = ApplyResults.Count > 0
                ? ApplyResults.Dequeue()
                : new(PluginItemCommandStatus.Started);
            if (answer.Accepted && ManaDrainConsumesDonor)
                _unsettledDrains.Add((objectId, targetObjectId));
            return answer;
        }

        /// <summary>
        /// The server answers the drains that went out. It is a separate step
        /// because it is a separate moment: a use is accepted first and
        /// answered afterwards, and a fake that answers inside the call leaves
        /// no version of the pack for the macro to have asked about.
        /// </summary>
        public void SettleManaDrains()
        {
            foreach ((uint stone, uint donor) in _unsettledDrains)
            {
                var remaining = new List<PluginInventoryItem>();
                foreach (PluginInventoryItem item in Owned)
                {
                    if (item.ObjectId == donor)
                        continue;
                    remaining.Add(item.ObjectId == stone
                        ? item with { Effects = item.Effects | 1u }
                        : item);
                }
                Owned = remaining;
                UseCompletion = new PluginItemUseCompletion(
                    UseCompletion.Revision + 1, stone, donor, 0u);
            }
            _unsettledDrains.Clear();
        }
        public List<uint> Picked { get; } = [];
        public Queue<PluginItemCommandResult> PickupResults { get; } = [];
        public List<uint> Identified { get; } = [];
        public List<(uint Tool, uint Item)> Salvaged { get; } = [];
        public List<uint> Sold { get; } = [];
        public uint VendorId { get; set; }
        public uint ActiveVendorObjectId => VendorId;
        public IReadOnlyList<PluginInventoryItem> CaptureOwnedItems() => Owned;
        // The host's object table, as far as these tests need it: every owned
        // item is there and already assessed, the state a kit or stone is in
        // before the macro may use it -- bar the ones a test names as never
        // having been appraised, which is what a stone bought or looted and
        // never looked at reads like.
        public HashSet<uint> Unassessed { get; } = [];

        /// <summary>
        /// How many loose items the character's own pack holds. Zero is what
        /// a client that has not described the character yet reports, and
        /// that is the state every test bar the pack-full ones wants.
        /// </summary>
        public int ItemCapacity { get; set; }
        public IWorldObjectAutomation Objects => this;
        bool IWorldObjectAutomation.IsAvailable => true;
        IReadOnlyList<PluginWorldObject> IWorldObjectAutomation.CaptureObjects() =>
            Owned
                .Select(item => new PluginWorldObject(
                    item.ObjectId, item.WeenieClassId, item.Name,
                    item.ObjectClass, item.ItemType, item.ContainerObjectId,
                    item.WielderObjectId))
                .ToArray();
        bool IWorldObjectAutomation.TryGet(uint objectId, out PluginWorldObject value)
        {
            if (objectId == Player)
            {
                value = new PluginWorldObject(
                    Player, 1u, Name, PluginObjectClass.Player, 0u, 0u, 0u)
                {
                    ItemsCapacity = ItemCapacity,
                };
                return true;
            }
            foreach (PluginInventoryItem item in Owned)
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
            // The corpses the tests stage are in the world too.
            foreach (PluginLootContainer corpse in Corpses)
            {
                if (corpse.ObjectId != objectId)
                    continue;
                value = new PluginWorldObject(
                    corpse.ObjectId, corpse.WeenieClassId, corpse.Name,
                    PluginObjectClass.Unknown, 0u, 0u, 0u)
                {
                    LastIdTime = corpse.IsIdentified ? 1 : 0,
                };
                return true;
            }
            value = default;
            return false;
        }
        public IReadOnlyList<PluginLootContainer> CaptureCorpses(
            float maximumDistance) => Corpses
                .Where(corpse => corpse.Distance <= maximumDistance)
                .ToArray();
        public IReadOnlyList<PluginInventoryItem> CaptureCurrentContents() =>
            Contents;
        /// <summary>
        /// What an appraisal delivered, per object. The real host answers
        /// this per object and an item nobody has appraised carries none of
        /// the mana, workmanship or scribe values the mana rules read, so a
        /// fake that hands every object the same empty table cannot tell a
        /// donor from anything else.
        /// </summary>
        public Dictionary<uint, PluginItemProperties> ItemProperties { get; } = [];

        /// <summary>
        /// Items the host cannot describe yet — an item that has left the
        /// corpse but whose description has not caught up.
        /// </summary>
        public HashSet<uint> UnreadableProperties { get; } = [];

        public bool TryCaptureProperties(
            uint objectId,
            out PluginItemProperties properties)
        {
            if (UnreadableProperties.Contains(objectId))
            {
                properties = EmptyProperties;
                return false;
            }
            properties = ItemProperties.TryGetValue(
                objectId, out PluginItemProperties stored)
                ? stored
                : EmptyProperties;
            return true;
        }
        public PluginItemCommandStatus OpenResult { get; set; } = PluginItemCommandStatus.Started;
        public PluginItemCommandResult Open(uint containerObjectId)
        {
            Opened.Add(containerObjectId);
            if (OpenResult != PluginItemCommandStatus.Started)
                return new(OpenResult);
            Requested = containerObjectId;
            return new(PluginItemCommandStatus.Started);
        }
        public PluginItemCommandResult Close(uint containerObjectId)
        {
            Closed.Add(containerObjectId);
            return new(PluginItemCommandStatus.Started);
        }
        /// <summary>
        /// Models the host's one-question-at-a-time description channel:
        /// while an answer is outstanding every other Identify comes back
        /// Busy, as the real surface's gate does. Off by default so the
        /// tests that only care about what was asked stay as they were.
        /// </summary>
        public bool OneDescriptionAtATime { get; set; }

        /// <summary>
        /// The bound the host puts on one unanswered question, after which
        /// it gives up and says so. Counted in Identify calls made while the
        /// silent object holds the channel, which is what
        /// <see cref="ReleaseStalledDescription"/> spends.
        /// </summary>
        public HashSet<uint> NeverAnswers { get; } = [];

        public PluginItemCommandResult Identify(uint objectId)
        {
            if (OneDescriptionAtATime && AppraisalState.AwaitingObjectId != 0u)
                return new(PluginItemCommandStatus.Busy);
            Identified.Add(objectId);
            AppraisalState = AppraisalState with
            {
                Revision = AppraisalState.Revision + 1,
                AwaitingObjectId = objectId,
                LastAbandonedObjectId =
                    AppraisalState.LastAbandonedObjectId == objectId
                        ? 0u
                        : AppraisalState.LastAbandonedObjectId,
            };
            // Only the channel-modelling fake answers by itself; the rest of
            // the tests stage the answer they want by hand.
            if (OneDescriptionAtATime && !NeverAnswers.Contains(objectId))
                CompleteAppraisal(objectId, presentInUi: false);
            return new(PluginItemCommandStatus.Started);
        }

        /// <summary>
        /// The host giving up on a question the server never answered: the
        /// channel is freed and the asker is told, exactly as the runtime
        /// owner's own bound does it.
        /// </summary>
        public void ReleaseStalledDescription()
        {
            uint awaiting = AppraisalState.AwaitingObjectId;
            if (awaiting == 0u)
                return;
            AppraisalState = AppraisalState with
            {
                Revision = AppraisalState.Revision + 1,
                AwaitingObjectId = 0u,
                LastAbandonedObjectId = awaiting,
            };
        }

        /// <summary>
        /// The examination window's presentation target, separate from the
        /// completion signal (AppraisalState.CurrentObjectId) -- mirrors
        /// RuntimeInteractionTransactionState.CurrentAppraisalId. Only
        /// CompleteAppraisal(..., presentInUi: true) ever moves this;
        /// tests assert against it directly to prove a "not presented"
        /// completion really did not touch presentation.
        /// </summary>
        public uint PresentedObjectId { get; private set; }

        /// <summary>
        /// Simulates an appraisal response landing, through the same split
        /// the real host uses (AppAutomationSurface.ILootAutomation.
        /// Appraisal maps CurrentObjectId to the completion signal --
        /// RuntimeInteractionTransactionState.LastCompletedAppraisalId --
        /// never to the examination window's presentation target,
        /// PresentedObjectId here). A test that instead pokes
        /// AppraisalState.CurrentObjectId directly cannot tell the two
        /// apart and would not have caught the corpse-looting
        /// stall: the corpse identify is Automation-origin and normally
        /// never presents (presentInUi: false here), yet the completion
        /// signal must still advance so looting proceeds.
        /// </summary>
        public void CompleteAppraisal(uint objectId, bool presentInUi)
        {
            AppraisalState = AppraisalState with
            {
                Revision = AppraisalState.Revision + 1,
                AwaitingObjectId = 0u,
                CurrentObjectId = objectId,
            };
            if (presentInUi)
                PresentedObjectId = objectId;
        }
        public PluginItemCommandResult Pickup(
            uint objectId,
            bool mainPack = false)
        {
            Picked.Add(objectId);
            return PickupResults.Count == 0
                ? new(PluginItemCommandStatus.Started)
                : PickupResults.Dequeue();
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

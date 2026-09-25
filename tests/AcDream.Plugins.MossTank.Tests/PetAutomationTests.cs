using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class PetAutomationTests
{
    // Essences as the catalog knows them.
    private const uint Cold = 49387u;
    private const uint Fire = 49380u;
    private const uint Acid = 49366u;
    private const uint Electric = 49373u;

    /// <summary>The spirit a refill tops an essence up with; the refill knows it by name.</summary>
    internal const uint EncapsulatedSpiritClass = 49485u;

    [Fact]
    public void AMonsterTakesThePickOnlyByBeingNearerAndRankedHigher()
    {
        // Equal rank: the nearer monster behind the first one does not take
        // the pick, because it does not outrank it.
        var settings = Settings(MonsterDamageType.Auto);
        var items = Essences(Device(1, Cold));

        PetSummonChoice choice = Select(
            items,
            [Target(10, "First", 6), Target(11, "Nearer", 2)],
            settings);

        Assert.Equal(10u, choice.Target.ObjectId);

        settings.Rules.Insert(0, new MonsterRule(
            "name#Nearer",
            new MonsterRuleActions { Priority = 5, PetDamageType = MonsterDamageType.Auto }));
        choice = Select(
            items,
            [Target(10, "First", 6), Target(11, "Nearer", 2)],
            settings);

        Assert.Equal(11u, choice.Target.ObjectId);
    }

    [Fact]
    public void TheDensityCountsOnlyMonstersInRangeThatWantAPet()
    {
        var settings = Settings(MonsterDamageType.Auto);
        settings.PetMonsterDensity = 2;
        settings.PetRangeMode = PetRangeMode.Custom;
        settings.PetCustomRange = 10f;
        settings.Rules.Insert(0, new MonsterRule(
            "name#NoPet",
            new MonsterRuleActions { PetDamageType = MonsterDamageType.None }));
        var items = Essences(Device(1, Cold));

        Assert.True(Select(
            items,
            [Target(10, "Target", 4), Target(11, "NoPet", 3), Target(12, "Far", 20)],
            settings).IsNone);
        Assert.False(Select(
            items,
            [Target(10, "Target", 4), Target(12, "Other", 9)],
            settings).IsNone);
    }

    [Fact]
    public void AnEssenceOutOfReachIsPassedOverForOneThatIsNot()
    {
        var settings = Settings(MonsterDamageType.Cold);
        var items = Essences(
            // Every one of these suits the monster better than the last.
            Device(1, Cold, requiredLevel: 200),
            Device(2, Cold, requiredSkill: 400),
            Device(3, Cold, structure: 0),
            Device(4, Cold, mastery: 2),
            Device(5, Fire));
        var warnings = new List<string>();

        PetSummonChoice choice = PetAutomation.SelectPet(
            items,
            items.Items,
            [Target(10, "Target", 4)],
            new Character(mastery: 3, summoning: 300, level: 150),
            settings,
            _ => MonsterDamageType.None,
            _ => [],
            warnings.Add);

        Assert.Equal(5u, choice.Device.ObjectId);
        Assert.Contains(warnings, text => text.Contains("wrong summoning mastery", StringComparison.Ordinal));
    }

    [Fact]
    public void ANamedElementIsFirstChoiceButNeverTheOnlyOne()
    {
        var items = Essences(Device(1, Acid), Device(2, Fire), Device(3, Cold));

        Assert.Equal(3u, Select(items, [Target(10, "Target", 4)],
            Settings(MonsterDamageType.Cold), attack: MonsterDamageType.Fire).Device.ObjectId);
        // No cold essence: the attack's own element comes next ...
        items = Essences(Device(1, Acid), Device(2, Fire));
        Assert.Equal(2u, Select(items, [Target(10, "Target", 4)],
            Settings(MonsterDamageType.Cold), attack: MonsterDamageType.Fire).Device.ObjectId);
        // ... then the monster's listed weaknesses, in their order ...
        Assert.Equal(1u, Select(items, [Target(10, "Target", 4)],
            Settings(MonsterDamageType.Cold),
            attack: MonsterDamageType.Slash,
            preferences: [MonsterDamageType.Electric, MonsterDamageType.Acid, MonsterDamageType.Fire]).Device.ObjectId);
        // ... and with nothing to go by, the first usable one still summons.
        Assert.Equal(1u, Select(items, [Target(10, "Target", 4)],
            Settings(MonsterDamageType.Cold), attack: MonsterDamageType.Slash).Device.ObjectId);
    }

    [Fact]
    public void PlayerAutoFollowsTheAttackThenTheWeaknessList()
    {
        var items = Essences(Device(1, Acid), Device(2, Electric), Device(3, Fire));
        var settings = Settings(MonsterDamageType.PlayerAuto);

        Assert.Equal(3u, Select(items, [Target(10, "Target", 4)], settings,
            attack: MonsterDamageType.Fire,
            preferences: [MonsterDamageType.Electric]).Device.ObjectId);
        Assert.Equal(2u, Select(items, [Target(10, "Target", 4)], settings,
            attack: MonsterDamageType.Cold,
            preferences: [MonsterDamageType.Electric, MonsterDamageType.Acid]).Device.ObjectId);
    }

    [Fact]
    public void AutoGoesByTheWeaknessListAlone()
    {
        var items = Essences(Device(1, Acid), Device(2, Fire));

        Assert.Equal(2u, Select(items, [Target(10, "Target", 4)],
            Settings(MonsterDamageType.Auto),
            attack: MonsterDamageType.Acid,
            preferences: [MonsterDamageType.Fire, MonsterDamageType.Acid]).Device.ObjectId);
    }

    [Fact]
    public void AnEqualRankGoesToTheHigherLevelRequirementThenTheItemsPageOrder()
    {
        var settings = Settings(MonsterDamageType.Auto);
        settings.CombatItemOrderIds.Clear();
        foreach (uint id in new uint[] { 3, 1, 2 })
            settings.CombatItemOrderIds.Add(id);
        var items = Essences(
            Device(1, Cold, requiredLevel: 50),
            Device(2, Cold, requiredLevel: 80),
            Device(3, Fire, requiredLevel: 50));

        Assert.Equal(2u, Select(items, [Target(10, "Target", 4)], settings).Device.ObjectId);

        items = Essences(Device(1, Cold, requiredLevel: 50), Device(3, Fire, requiredLevel: 50));
        Assert.Equal(3u, Select(items, [Target(10, "Target", 4)], settings).Device.ObjectId);
    }

    [Fact]
    public void AnEssenceTheTableDoesNotKnowStrikesAsABludgeon()
    {
        var items = Essences(Device(1, Cold), Device(2, 99999u));

        Assert.Equal(2u, Select(items, [Target(10, "Target", 4)],
            Settings(MonsterDamageType.Bludgeon)).Device.ObjectId);
    }

    [Fact]
    public void OnlyItemsOnTheItemsPageAreConsidered()
    {
        var settings = Settings(MonsterDamageType.Auto);
        settings.CombatItemObjectIds.Clear();
        settings.CombatItemOrderIds.Clear();
        var items = Essences(Device(1, Cold));

        Assert.True(Select(items, [Target(10, "Target", 4)], settings).IsNone);
    }

    [Fact]
    public void TheRefillTakesTheFirstLowEssenceAndTheSmallestSpiritStack()
    {
        var settings = Settings(MonsterDamageType.Cold);
        // Nothing about the refill depends on summoning being wanted.
        settings.SummonPets = false;
        var items = Essences(
            // Under the threshold but already full: nothing to add.
            Device(1, Cold, structure: 2, maximum: 2),
            Device(2, Fire, structure: 3),
            Device(3, Electric, structure: 1),
            Spirit(4, stack: 12),
            Spirit(5, stack: 3));

        Assert.True(PetAutomation.TrySelectRefill(
            items, items.Items, settings, 3, out PluginInventoryItem device, out PluginInventoryItem spirit));
        Assert.Equal(2u, device.ObjectId);
        Assert.Equal(5u, spirit.ObjectId);

        Assert.True(PetAutomation.TrySelectRefill(
            items, items.Items, settings, 1, out device, out _));
        Assert.Equal(3u, device.ObjectId);
    }

    /// <summary>
    /// An essence not yet appraised this session is known by the shared
    /// cooldown the server sends with the item, for the refill as for the
    /// summon; an unappraised item without it is not an essence. Mutation:
    /// reading the cooldown only from the appraisal finds nothing to refill.
    /// </summary>
    [Fact]
    public void TheRefillKnowsAnUnappraisedEssenceByTheCooldownSentWithIt()
    {
        var settings = Settings(MonsterDamageType.Cold);
        var items = Essences(
            Device(1, Cold, structure: 1) with { SharedCooldownId = 0u },
            Device(2, Fire, structure: 1) with
            {
                SharedCooldownId = PluginInventoryItem.SummoningCooldownId,
            },
            Spirit(3));
        items.Unappraised.Add(1u);
        items.Unappraised.Add(2u);

        Assert.True(PetAutomation.TrySelectRefill(
            items, items.Items, settings, 3, out PluginInventoryItem device, out _));
        Assert.Equal(2u, device.ObjectId);
    }

    [Fact]
    public void TheRefillNeedsALowEssenceAndASpirit()
    {
        var settings = Settings(MonsterDamageType.Cold);
        var items = Essences(Device(1, Cold, structure: 4), Spirit(2));
        Assert.False(PetAutomation.TrySelectRefill(items, items.Items, settings, 3, out _, out _));

        items = Essences(Device(1, Cold, structure: 1));
        Assert.False(PetAutomation.TrySelectRefill(items, items.Items, settings, 3, out _, out _));
    }

    /// <summary>
    /// A refill the client refuses outright gives the turn up, so the rules
    /// below it run, and is not asked for again until the retry clock has
    /// run out.
    /// Mutation: keep the turn on every refusal, or drop the retry clock, and
    /// the refused refill either holds the pass or is re-sent on the next one.
    /// </summary>
    [Fact]
    public void ARefusedRefillGivesUpTheTurnAndWaitsBeforeAskingAgain()
    {
        (PetRefillRule rule, SummonPetRuleTests.Automation automation) = RefillRule();
        automation.ApplyStatus = PluginItemCommandStatus.Refused;

        Assert.False(rule.Tick(new MacroPassContext(0.3d, CanAct: true)));
        Assert.Single(automation.Applies);
        Assert.False(rule.Tick(new MacroPassContext(0.3d, CanAct: true)));
        Assert.Single(automation.Applies);

        automation.ApplyStatus = PluginItemCommandStatus.Started;
        Assert.True(rule.Tick(new MacroPassContext(PetRefillRule.RefusalRetrySeconds, CanAct: true)));
        Assert.Equal(2, automation.Applies.Count);
    }

    /// <summary>
    /// A refill refused because the character is busy is still the refill
    /// that is wanted: it keeps the turn, but it too waits out the retry
    /// clock rather than being re-sent on every pass.
    /// Mutation: re-send a busy refusal on the next pass and the second pass
    /// asks the client again.
    /// </summary>
    [Fact]
    public void ABusyRefillKeepsTheTurnButWaitsBeforeAskingAgain()
    {
        (PetRefillRule rule, SummonPetRuleTests.Automation automation) = RefillRule();
        automation.ApplyStatus = PluginItemCommandStatus.Busy;

        Assert.True(rule.Tick(new MacroPassContext(0.3d, CanAct: true)));
        Assert.Single(automation.Applies);
        Assert.True(rule.Tick(new MacroPassContext(0.3d, CanAct: true)));
        Assert.Single(automation.Applies);

        automation.ApplyStatus = PluginItemCommandStatus.Started;
        Assert.True(rule.Tick(new MacroPassContext(PetRefillRule.RefusalRetrySeconds, CanAct: true)));
        Assert.Equal(2, automation.Applies.Count);
    }

    private static (PetRefillRule Rule, SummonPetRuleTests.Automation Automation) RefillRule()
    {
        var automation = new SummonPetRuleTests.Automation
        {
            ItemEntries = [Device(1, Cold, structure: 1), Spirit(2)],
        };
        var rule = new PetRefillRule(
            new SummonPetRuleTests.FakeHost(automation),
            Settings(MonsterDamageType.Cold),
            threshold: () => 3,
            readyToRefillInPeace: () => true);
        return (rule, automation);
    }

    [Theory]
    [InlineData(48886u, (int)MonsterDamageType.Bludgeon)]
    [InlineData(49366u, (int)MonsterDamageType.Acid)]
    [InlineData(49380u, (int)MonsterDamageType.Fire)]
    [InlineData(49387u, (int)MonsterDamageType.Cold)]
    [InlineData(49373u, (int)MonsterDamageType.Electric)]
    public void Catalog_MapsAuthenticDeviceWcids(
        uint wcid,
        int expected)
        => Assert.Equal(
            (MonsterDamageType)expected,
            PetDeviceCatalog.DamageType(wcid));

    private static PetSummonChoice Select(
        ItemAutomation items,
        IReadOnlyList<PluginCombatTarget> targets,
        CombatSettings settings,
        MonsterDamageType attack = MonsterDamageType.None,
        IReadOnlyList<MonsterDamageType>? preferences = null) =>
        PetAutomation.SelectPet(
            items,
            items.Items,
            targets,
            new Character(mastery: 3, summoning: 300, level: 275),
            settings,
            _ => attack,
            _ => preferences ?? []);

    private static CombatSettings Settings(MonsterDamageType pet)
    {
        var settings = new CombatSettings { SummonPets = true, MaximumRange = 10f };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                DamageType = MonsterDamageType.Auto,
                PetDamageType = pet,
            }));
        for (uint id = 1; id <= 20; id++)
        {
            settings.CombatItemObjectIds.Add(id);
            settings.CombatItemOrderIds.Add(id);
        }
        return settings;
    }

    private static ItemAutomation Essences(params PluginInventoryItem[] items) =>
        new() { Items = items };

    private static PluginCombatTarget Target(uint id, string name, float distance) =>
        new(id, name, 1, distance, 0, true, 1f);

    private static PluginInventoryItem Spirit(uint id, int stack = 1) =>
        Item(id, EncapsulatedSpiritClass) with
        {
            Name = "Encapsulated Spirit",
            StackSize = stack,
        };

    /// <summary>
    /// An essence as a client sees it: no pet class, the shared summoning
    /// cooldown the host reports with the item, and (in the fake's property
    /// table) the level it asks for.
    /// </summary>
    private static PluginInventoryItem Device(
        uint id,
        uint wcid,
        int mastery = 0,
        int requiredSkill = 100,
        int requiredLevel = 0,
        int structure = 50,
        int maximum = 50) =>
        Item(id, wcid) with
        {
            Name = $"Essence {id}",
            SharedCooldownId = PluginInventoryItem.SummoningCooldownId,
            SummoningMastery = mastery,
            UseRequiresSkill = 54,
            UseRequiresSkillLevel = requiredSkill,
            Structure = structure,
            MaximumStructure = maximum,
            // The level requirement rides in the spell id slot for the fake
            // to pick up; nothing reads SpellId for an essence.
            SpellId = (uint)requiredLevel,
        };

    private static PluginInventoryItem Item(uint id, uint wcid) => new(
        ObjectId: id,
        WeenieClassId: wcid,
        Name: $"Item {id}",
        ItemType: 0,
        ContainerObjectId: 1,
        WielderObjectId: 0,
        ValidLocations: 0,
        EquippedLocation: 0,
        Useability: 0,
        TargetType: 0,
        PublicFlags: 0,
        StackSize: 1,
        Structure: 0,
        MaximumStructure: 0,
        SpellId: 0,
        PetClass: 0,
        SummoningMastery: 0,
        ProcSpellId: 0,
        ProcSpellSelfTargeted: false,
        ProcSpellRate: 0,
        WeaponSkill: 0,
        DamageType: 0,
        Damage: 0,
        DamageVariance: 0,
        UseRequiresSkill: 0,
        UseRequiresSkillLevel: 0,
        UseRequiresSkillSpecialized: 0);

    private sealed class Character(int mastery, uint summoning, int level) : ICharacterInfo
    {
        public bool IsInWorld => true;
        public int Level => level;
        public uint ObjectId => 1;
        public uint CurrentHealth => 100;
        public uint MaxHealth => 100;
        public uint CurrentStamina => 100;
        public uint MaxStamina => 100;
        public uint CurrentMana => 100;
        public uint MaxMana => 100;
        public int SummoningMastery => mastery;
        public IReadOnlyList<PluginSkillInfo> Skills =>
            [new(54, "Summoning", PluginSkillTraining.Trained, summoning)];
        public IReadOnlyList<PluginAttributeInfo> Attributes => [];
        public IReadOnlyList<PluginActiveEnchantment> ActiveEnchantments => [];

        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            if (skillId == 54)
            {
                skill = Skills[0];
                return true;
            }
            skill = default;
            return false;
        }
    }

    private sealed class ItemAutomation : IItemAutomation
    {
        public bool IsAvailable => true;
        public bool IsBusy { get; set; }
        public PluginItemUseCompletion LastCompletion => default;
        public IReadOnlyList<PluginInventoryItem> Items { get; set; } = [];
        public IReadOnlyList<PluginInventoryItem> CaptureOwnedItems() => Items;

        /// <summary>Items this session has not appraised: no properties yet.</summary>
        public HashSet<uint> Unappraised { get; } = [];

        public PluginItemCommandResult Use(uint objectId) =>
            new(PluginItemCommandStatus.Started);

        public PluginItemCommandResult Apply(uint objectId, uint targetObjectId) =>
            new(PluginItemCommandStatus.Started);

        /// <summary>
        /// An essence's assessment as the server sends it: the shared
        /// summoning cooldown, its charges and the level it asks for, and no
        /// pet class. Anything with a structure ceiling is an essence here.
        /// </summary>
        public bool TryCaptureProperties(uint objectId, out PluginItemProperties properties)
        {
            properties = default;
            foreach (PluginInventoryItem item in Items)
            {
                if (item.ObjectId != objectId)
                    continue;
                var ints = new Dictionary<uint, int>();
                if (item.MaximumStructure > 0 && !Unappraised.Contains(objectId))
                {
                    ints[280u] = 213;
                    ints[92u] = item.Structure;
                    ints[91u] = item.MaximumStructure;
                    if (item.SpellId != 0u)
                        ints[369u] = (int)item.SpellId;
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
            return false;
        }
    }
}

using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank.Tests;

public sealed class SummonPetRuleTests
{
    private static MacroPassContext Pass(double elapsed = 0.3d) =>
        new(elapsed, CanAct: true);

    private static SummonPetRule Rule(
        Automation automation,
        CombatSettings settings,
        bool combatOwnsTarget = false) =>
        new(new FakeHost(automation),
            settings,
            _ => MonsterDamageType.None,
            _ => []);

    private static CombatSettings Settings()
    {
        var settings = new CombatSettings
        {
            Enabled = true,
            SummonPets = true,
            MaximumRange = 10d,
        };
        settings.Rules.Clear();
        settings.Rules.Add(new MonsterRule(
            "DEFAULT",
            new MonsterRuleActions
            {
                DamageType = MonsterDamageType.Auto,
                PetDamageType = MonsterDamageType.Cold,
            }));
        for (uint id = 1; id <= 20; id++)
            settings.CombatItemObjectIds.Add(id);
        return settings;
    }

    private static Automation Ready() => new()
    {
        Targets = [new PluginCombatTarget(10, "Drudge", 1, 4f, 0f, true, 1f)],
        ItemEntries = [Device(1u, 49387u)],
    };

    [Fact]
    public void SummonsWithMonstersInRangeAndNoSelectedTarget()
    {
        Automation automation = Ready();
        SummonPetRule rule = Rule(automation, Settings());

        // The predicate picks and issues nothing; the turn uses the device.
        Assert.True(rule.ValidNow(Pass()));
        Assert.Empty(automation.Uses);
        rule.Running = true;
        Assert.Equal([1u], automation.Uses);
    }

    /// <summary>
    /// Whether an item is a pet device is the host's answer: the summoning
    /// cooldown it shares, which the server's assessment carries, or a pet
    /// class, if the server ever names one. An item with neither is not a
    /// pet device.
    /// Mutation: keep a predicate of MossTank's own that ignores the pet
    /// class and the pet-class row is invalid.
    /// </summary>
    [Fact]
    public void AnEssenceIsKnownByTheHostsPetDeviceFlag()
    {
        Automation automation = Ready();
        SummonPetRule rule = Rule(automation, Settings());
        Assert.True(rule.ValidNow(Pass()));

        automation.Ints[1u] = new() { [280u] = 1 };
        Assert.False(Rule(automation, Settings()).ValidNow(Pass()));

        automation.ItemEntries = [automation.ItemEntries[0] with { PetClass = 49388 }];
        Assert.True(Rule(automation, Settings()).ValidNow(Pass()));
    }

    /// <summary>
    /// The server sends an item's shared cooldown with the item itself, so an
    /// essence this session has not appraised is still known by it: the
    /// first pass after a relog summons with it. Mutation: reading the
    /// cooldown only from the appraisal leaves the rule invalid.
    /// </summary>
    [Fact]
    public void AnUnappraisedEssenceIsKnownByTheCooldownSentWithIt()
    {
        Automation automation = Ready();
        automation.Ints[1u] = [];
        automation.ItemEntries =
        [
            automation.ItemEntries[0] with
            {
                SharedCooldownId = PluginInventoryItem.SummoningCooldownId,
            },
        ];
        SummonPetRule rule = Rule(automation, Settings());

        Assert.True(rule.ValidNow(Pass()));
        rule.Running = true;
        Assert.Equal([1u], automation.Uses);

        automation.ItemEntries =
        [
            automation.ItemEntries[0] with { SharedCooldownId = 5u },
        ];
        Assert.False(Rule(automation, Settings()).ValidNow(Pass()));
    }

    /// <summary>
    /// The shared cooldown is what stands between two summons that went
    /// through. The rule does not ask whether a pet of the character's is
    /// still about: once the cooldown has run out, the next pass summons
    /// again.
    /// </summary>
    [Fact]
    public void TheNextSummonWaitsForTheCooldownAlone()
    {
        Automation automation = Ready();
        automation.ActivePets = 1;
        SummonPetRule rule = Rule(automation, Settings());

        Assert.True(rule.ValidNow(Pass()));
        rule.Running = true;
        Assert.Equal([1u], automation.Uses);
        rule.Running = false;

        automation.SpellCatalog.Cooldowns[213u] = 45d;
        Assert.False(rule.ValidNow(Pass(20d)));

        automation.SpellCatalog.Cooldowns[213u] = 0d;
        Assert.True(rule.ValidNow(Pass()));
        rule.Running = true;
        Assert.Equal([1u, 1u], automation.Uses);
    }

    /// <summary>
    /// A summon the client refuses is not tried again on the next pass, only
    /// once the retry clock has run.
    /// Mutation: drop the retry clock and the second pass re-sends the use.
    /// </summary>
    [Fact]
    public void ARefusedSummonWaitsBeforeItIsTriedAgain()
    {
        Automation automation = Ready();
        automation.UseStatus = PluginItemCommandStatus.Refused;
        SummonPetRule rule = Rule(automation, Settings());

        Assert.True(rule.ValidNow(Pass()));
        rule.Running = true;
        Assert.Equal([1u], automation.Uses);
        rule.Running = false;

        Assert.False(rule.ValidNow(Pass()));
        rule.Running = true;
        Assert.Equal([1u], automation.Uses);
        rule.Running = false;

        automation.UseStatus = PluginItemCommandStatus.Started;
        Assert.True(rule.ValidNow(Pass(SummonPetRule.RefusalRetrySeconds)));
        rule.Running = true;
        Assert.Equal([1u, 1u], automation.Uses);
    }

    /// <summary>
    /// A summon that went out waits for the server's answer. An answer with
    /// an error — which starts no cooldown — is tried again only after the
    /// retry clock, not on the next pass.
    /// Mutation: re-send while the answer is outstanding, or at once after a
    /// failed answer, and a use goes out on a pass where none should.
    /// </summary>
    [Fact]
    public void ASummonTheServerFailsWaitsForItsAnswerAndThenTheRetryClock()
    {
        Automation automation = Ready();
        SummonPetRule rule = Rule(automation, Settings());

        Assert.True(rule.ValidNow(Pass()));
        rule.Running = true;
        rule.Running = false;
        Assert.False(rule.ValidNow(Pass()));

        // The server answers the summon with an error and starts no cooldown.
        automation.Completion = new PluginItemUseCompletion(1L, 1u, 0u, 0x0402u);
        Assert.False(rule.ValidNow(Pass()));
        Assert.False(rule.ValidNow(Pass()));
        Assert.True(rule.ValidNow(Pass(SummonPetRule.RefusalRetrySeconds)));
        rule.Running = true;
        Assert.Equal([1u, 1u], automation.Uses);
    }

    /// <summary>
    /// A summon that went through ends the wait at once; the cooldown it
    /// started is what holds the next one. An answer about some other item
    /// does not end the wait.
    /// Mutation: end the wait on any answer and the other item's answer lets
    /// the rule summon again.
    /// </summary>
    [Fact]
    public void ASummonThatWentThroughEndsTheWaitOnItsOwnAnswer()
    {
        Automation automation = Ready();
        SummonPetRule rule = Rule(automation, Settings());
        Assert.True(rule.ValidNow(Pass()));
        rule.Running = true;
        rule.Running = false;

        automation.Completion = new PluginItemUseCompletion(1L, 7u, 0u, 0u);
        Assert.False(rule.ValidNow(Pass()));

        automation.Completion = new PluginItemUseCompletion(2L, 1u, 0u, 0u);
        Assert.True(rule.ValidNow(Pass()));
    }

    /// <summary>
    /// An answer that never comes does not stop summoning for good: the wait
    /// ends after its own few seconds.
    /// Mutation: wait for the answer without a limit and the rule never
    /// becomes valid again.
    /// </summary>
    [Fact]
    public void ASummonWhoseAnswerNeverComesStopsWaitingForIt()
    {
        Automation automation = Ready();
        SummonPetRule rule = Rule(automation, Settings());
        Assert.True(rule.ValidNow(Pass()));
        rule.Running = true;
        rule.Running = false;

        Assert.False(rule.ValidNow(Pass(SummonPetRule.AnswerWaitSeconds - 1d)));
        Assert.True(rule.ValidNow(Pass(1d)));
    }

    /// <summary>
    /// A pet needs room to appear: the rule asks, last and only once it has
    /// an essence and a monster, whether a body fits three metres straight
    /// ahead, and does nothing while the answer is Blocked. A client that
    /// cannot tell (Unknown) counts as room, the way the reference treats a
    /// failed check. Mutation: dropping the check summons into the wall.
    /// </summary>
    [Fact]
    public void NoRoomAheadForThePetIsInvalid()
    {
        Automation automation = Ready();
        automation.Room.Status = PluginRoomAheadStatus.Blocked;
        SummonPetRule rule = Rule(automation, Settings());

        Assert.False(rule.ValidNow(Pass()));
        Assert.Equal([3f], automation.Room.Asked);
        rule.Running = true;
        Assert.Empty(automation.Uses);

        automation.Room.Status = PluginRoomAheadStatus.Unknown;
        Assert.True(rule.ValidNow(Pass()));

        automation.Room.Status = PluginRoomAheadStatus.Clear;
        Assert.True(rule.ValidNow(Pass()));
        rule.Running = true;
        Assert.Equal([1u], automation.Uses);
    }

    /// <summary>
    /// The room check is the rule's last question: with no essence to use
    /// or no monster in range it is never asked.
    /// </summary>
    [Fact]
    public void TheRoomCheckComesAfterThePetIsChosen()
    {
        Automation automation = Ready();
        automation.Targets = [];

        Assert.False(Rule(automation, Settings()).ValidNow(Pass()));
        Assert.Empty(automation.Room.Asked);
    }

    /// <summary>EnableCombat gates the rule.</summary>
    [Fact]
    public void CombatDisabledIsInvalid()
    {
        Automation automation = Ready();
        CombatSettings settings = Settings();
        settings.Enabled = false;

        Assert.False(Rule(automation, settings).ValidNow(Pass()));
        Assert.Empty(automation.Uses);
    }

    /// <summary>SummonPets gates the rule.</summary>
    [Fact]
    public void SummonPetsOffIsInvalid()
    {
        Automation automation = Ready();
        CombatSettings settings = Settings();
        settings.SummonPets = false;

        Assert.False(Rule(automation, settings).ValidNow(Pass()));
        Assert.Empty(automation.CaptureCalls);
        Assert.Empty(automation.Uses);
    }

    /// <summary>The Summoning skill must be trained.</summary>
    [Fact]
    public void UntrainedSummoningIsInvalid()
    {
        Automation automation = Ready();
        automation.SummoningTraining = PluginSkillTraining.Untrained;

        Assert.False(Rule(automation, Settings()).ValidNow(Pass()));
        Assert.Empty(automation.Uses);
    }

    /// <summary>
    /// The summoning cooldown every essence shares gates the rule. The server
    /// keeps it among the cooldowns, not the enchantments in force, so it is
    /// read from the host's cooldown clock.
    /// </summary>
    [Fact]
    public void TheSharedSummoningCooldownIsInvalid()
    {
        Automation automation = Ready();
        automation.SpellCatalog.Cooldowns[213u] = 30d;

        Assert.False(Rule(automation, Settings()).ValidNow(Pass()));
        Assert.Empty(automation.Uses);

        automation.SpellCatalog.Cooldowns[213u] = 0d;
        Assert.True(Rule(automation, Settings()).ValidNow(Pass()));
    }

    [Fact]
    public void NoMonstersInRangeIsInvalid()
    {
        Automation automation = Ready();
        automation.Targets = [];

        Assert.False(Rule(automation, Settings()).ValidNow(Pass()));
        Assert.Empty(automation.Uses);
    }

    [Fact]
    public void MonsterDensityBelowTheSettingIsInvalid()
    {
        Automation automation = Ready();
        CombatSettings settings = Settings();
        settings.PetMonsterDensity = 2;

        Assert.False(Rule(automation, settings).ValidNow(Pass()));
        Assert.Empty(automation.Uses);
    }

    [Fact]
    public void ASelectedTargetDoesNotRefuseTheSummon()
    {
        // The reference rule asks nothing about a selected target: it fires
        // off the in-range monster census alone.
        Automation automation = Ready();

        Assert.True(
            Rule(automation, Settings(), combatOwnsTarget: true).ValidNow(Pass()));
    }

    [Fact]
    public void IndependentTrackNeverRefills()
    {
        Automation automation = Ready();
        automation.ItemEntries =
        [
            Device(1u, 49387u, structure: 0),
            Item(2u, PetAutomationTests.EncapsulatedSpiritClass) with
            {
                Name = "Encapsulated Spirit",
            },
        ];

        Assert.False(Rule(automation, Settings()).ValidNow(Pass()));
        Assert.Empty(automation.Applies);
        Assert.Empty(automation.Uses);
    }

    /// <summary>
    /// An essence as a client sees it: the server never sends the pet class,
    /// only the summoning cooldown the essence shares (see the fake's property
    /// table).
    /// </summary>
    private static PluginInventoryItem Device(
        uint id,
        uint wcid,
        int structure = 50) => Item(id, wcid) with
        {
            PetClass = 0,
            SummoningMastery = 0,
            UseRequiresSkill = 54,
            UseRequiresSkillLevel = 100,
            Structure = structure,
            MaximumStructure = 50,
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
        Structure: 1,
        MaximumStructure: 1,
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

    internal sealed class FakeHost(Automation automation) : IPluginHost
    {
        public bool HasUi => false;
        public IPluginLogger Log { get; } = new StubLogger();
        public IGameState State { get; } = new StubState();
        public IEvents Events { get; } = new StubEvents();
        public ISelectionService Selection { get; } = new StubSelection();
        public IUiRegistry Ui => NoOpUiRegistry.Instance;
        public IAutomationSurface Automation { get; } = automation;
    }

    private sealed class StubLogger : IPluginLogger
    {
        public void Info(string message) { }
        public void Warn(string message) { }
        public void Error(string message, Exception? exception = null) { }
    }

    private sealed class StubState : IGameState
    {
        public IReadOnlyList<WorldEntitySnapshot> Entities => [];
    }

    private sealed class StubEvents : IEvents
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

    private sealed class StubSelection : ISelectionService
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

    internal sealed class FakeSpells : ISpellCatalog
    {
        public Dictionary<uint, double> Cooldowns { get; } = [];
        public IReadOnlyList<PluginSpellInfo> KnownSelfBuffs => [];

        public bool TryGet(uint spellId, out PluginSpellInfo info)
        {
            info = default;
            return false;
        }

        public double GetCooldownRemaining(uint cooldownId) =>
            Cooldowns.GetValueOrDefault(cooldownId);
    }

    internal sealed class Automation
        : IAutomationSurface, ICharacterInfo, IItemAutomation, ICombatAutomation
    {
        public bool IsAvailable => true;
        public RoomCheck Room { get; } = new();
        INavigationAutomation IAutomationSurface.Navigation => Room;
        public ICharacterInfo Character => this;
        public IItemAutomation Items => this;
        public ICombatAutomation Combat => this;
        public FakeSpells SpellCatalog { get; } = new();
        public ISpellCatalog Spells => SpellCatalog;
        public IMagicCommands Magic => NoOpAutomationSurface.Instance;
        public IPluginChat Chat => NoOpAutomationSurface.Instance;

        public PluginSkillTraining SummoningTraining { get; set; } =
            PluginSkillTraining.Trained;
        public IReadOnlyList<PluginActiveEnchantment> Enchantments { get; set; } = [];
        public IReadOnlyList<PluginCombatTarget> Targets { get; set; } = [];
        public IReadOnlyList<PluginInventoryItem> ItemEntries { get; set; } = [];
        public List<uint> Uses { get; } = [];
        public List<(uint Source, uint Target)> Applies { get; } = [];

        public bool IsInWorld => true;
        public uint ObjectId => 1;
        public uint CurrentHealth => 100;
        public uint MaxHealth => 100;
        public uint CurrentStamina => 100;
        public uint MaxStamina => 100;
        public uint CurrentMana => 100;
        public uint MaxMana => 100;
        public int SummoningMastery => 0;
        public IReadOnlyList<PluginSkillInfo> Skills =>
            [new(54, "Summoning", SummoningTraining, 300)];
        public IReadOnlyList<PluginAttributeInfo> Attributes => [];
        IReadOnlyList<PluginActiveEnchantment> ICharacterInfo.ActiveEnchantments =>
            Enchantments;

        public bool TryGetSkill(uint skillId, out PluginSkillInfo skill)
        {
            if (skillId == 54u)
            {
                skill = Skills[0];
                return true;
            }
            skill = default;
            return false;
        }

        bool IItemAutomation.IsAvailable => true;
        bool IItemAutomation.IsBusy => false;
        public int ActivePets { get; set; }
        int IItemAutomation.ActiveOwnedPetCount => ActivePets;
        /// <summary>The last use the server answered; nothing answered, left alone.</summary>
        public PluginItemUseCompletion Completion { get; set; }
        PluginItemUseCompletion IItemAutomation.LastCompletion => Completion;

        /// <summary>What the client answers a use or an apply with.</summary>
        public PluginItemCommandStatus UseStatus { get; set; } = PluginItemCommandStatus.Started;
        public PluginItemCommandStatus ApplyStatus { get; set; } = PluginItemCommandStatus.Started;
        /// <summary>
        /// The host folds the shared cooldown an appraisal carries into the
        /// item it reports, as it does the one sent with the item itself.
        /// </summary>
        public IReadOnlyList<PluginInventoryItem> CaptureOwnedItems() =>
            ItemEntries.Select(item =>
                item.SharedCooldownId == 0u
                && ((IItemAutomation)this).TryCaptureProperties(
                    item.ObjectId, out PluginItemProperties properties)
                && properties.Ints.TryGetValue(280u, out int cooldown)
                    ? item with { SharedCooldownId = (uint)cooldown }
                    : item).ToArray();

        public PluginItemCommandResult Use(uint objectId)
        {
            Uses.Add(objectId);
            return new(UseStatus);
        }

        public PluginItemCommandResult Apply(uint objectId, uint targetObjectId)
        {
            Applies.Add((objectId, targetObjectId));
            return new(ApplyStatus);
        }

        /// <summary>
        /// Property tables by object id. An item with none gets the shared
        /// summoning cooldown when it has a structure ceiling of 50, the way
        /// an essence's assessment reads.
        /// </summary>
        public Dictionary<uint, Dictionary<uint, int>> Ints { get; } = [];

        public bool TryCaptureProperties(uint objectId, out PluginItemProperties properties)
        {
            properties = default;
            Dictionary<uint, int>? ints = null;
            if (!Ints.TryGetValue(objectId, out ints))
            {
                foreach (PluginInventoryItem item in ItemEntries)
                {
                    if (item.ObjectId == objectId && item.MaximumStructure == 50)
                        ints = new() { [280u] = 213 };
                    else if (item.ObjectId == objectId)
                        ints = [];
                }
            }
            if (ints is null)
                return false;
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

        PluginCombatSnapshot ICombatAutomation.Snapshot => default;
        public List<float> CaptureCalls { get; } = [];
        IReadOnlyList<PluginCombatTarget> ICombatAutomation.CaptureHostileTargets(
            float maximumDistance)
        {
            CaptureCalls.Add(maximumDistance);
            return Targets;
        }
        PluginCombatCommandResult ICombatAutomation.EnterDefaultMode() =>
            new(PluginCombatCommandStatus.Unavailable);
        PluginCombatCommandResult ICombatAutomation.BeginPhysicalAttack(
            uint targetObjectId,
            PluginAttackHeight height,
            float power) => new(PluginCombatCommandStatus.Unavailable);
        PluginCombatCommandResult ICombatAutomation.ReleasePhysicalAttack() =>
            new(PluginCombatCommandStatus.Unavailable);
        PluginCombatCommandResult ICombatAutomation.AbortPhysicalAttack() =>
            new(PluginCombatCommandStatus.Unavailable);
    }

    /// <summary>
    /// The room-ahead check alone: every question is recorded, and the
    /// answer is whatever the test stages.
    /// </summary>
    internal sealed class RoomCheck : INavigationAutomation
    {
        public PluginRoomAheadStatus Status { get; set; } = PluginRoomAheadStatus.Clear;
        public List<float> Asked { get; } = [];

        public PluginRoomAhead CheckRoomAhead(float distanceMeters)
        {
            Asked.Add(distanceMeters);
            return new PluginRoomAhead(Status, default);
        }

        public PluginNavigationSnapshot Snapshot => default;

        public bool TryGetObject(uint objectId, out PluginNavigationObject value)
        {
            value = default;
            return false;
        }

        public PluginNavigationCommandStatus SetMovementIntent(in PluginMovementIntent intent) =>
            PluginNavigationCommandStatus.Unavailable;

        public PluginNavigationCommandStatus ClearMovementIntent() =>
            PluginNavigationCommandStatus.Unavailable;
    }
}

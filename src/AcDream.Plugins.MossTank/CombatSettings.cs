using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal enum TargetSelectionMethod
{
    Range,
    Angle,
    Both,
}

internal enum DebuffEachFirst
{
    One = 1,
    Priority = 2,
    All = 3,
}

internal enum DebuffSelectionMethod
{
    SpellLevel = 1,
    Skill = 2,
}

internal enum UseArcsMode
{
    No = 1,
    AtRange = 2,
    Yes = 3,
}

internal enum PetRangeMode
{
    AttackDistance = 0,
    Custom = 1,
}

internal enum ConsumableCategory
{
    Other,
    HealthKit,
    HealthFood,
    StaminaKit,
    StaminaFood,
    ManaKit,
    ManaFood,
    Pea,
    AllPeas,
    Lockpick,
    ManaStone,
    ManaSource,
    Grenade,
    SplitComponent,
    BuffConsumable,
}

internal sealed class CombatSettings
{
    public bool Enabled { get; set; } = true;
    public int HuntSkillExcessOverDifficulty { get; set; } = 25;
    public double MaximumRange { get; set; } = 5d;
    /// <summary>
    /// Monsters nearer than this are not valid attack targets. VTank applies
    /// this before priority and angle/range ranking.
    /// </summary>
    public double MinimumRange { get; set; }
    public double ApproachDistance { get; set; }
    public bool IdlePeaceMode { get; set; }
    public bool StopMacroOnDeath { get; set; } = true;
    public bool JumpOutWandCasting { get; set; }
    public bool DoJiggle { get; set; }
    public TargetSelectionMethod SelectionMethod { get; set; } =
        TargetSelectionMethod.Both;
    public double TargetSelectAngleRange { get; set; } = 5d;
    public bool TargetLock { get; set; }
    public PluginAttackHeight AttackHeight { get; set; } =
        PluginAttackHeight.Medium;
    public float AttackPower { get; set; } = 0.5f;
    public bool AutoAttackPower { get; set; } = true;
    public bool UseRecklessness { get; set; } = true;
    public double ScanIntervalSeconds { get; set; } = 0.25;
    public DebuffEachFirst DebuffEachFirst { get; set; } = DebuffEachFirst.One;
    public DebuffSelectionMethod DebuffSelectionMethod { get; set; } =
        DebuffSelectionMethod.Skill;
    public double DebuffPrecastSeconds { get; set; } = 5d;
    public bool SwitchWandsToDebuff { get; set; }
    /// <summary>
    /// Stock profiles never arc: on an exact quality tie the bolt wins. The
    /// option only decides that tie.
    /// </summary>
    public UseArcsMode UseArcs { get; set; } = UseArcsMode.No;
    public double SpellRangeFudge { get; set; } = 1d;
    public bool UseBreakableTurnTo { get; set; } = true;
    public bool UseProjectileAwareness { get; set; } = true;
    public double CollisionProjectileRadius { get; set; } = 0.4d;
    public double CollisionStepDistance { get; set; } = 0.7d;
    public bool ShowCollisionDebug { get; set; }

    /// <summary>
    /// How many times ONE pass may choose again after finding it cannot carry
    /// out its decision against the monster it picked.
    /// </summary>
    public int MaximumCollisionChecksPerTick { get; set; } = 500;

    /// <summary>
    /// The cap on samples taken along a single flight path. The reference
    /// client bounds this by geometry — the step distance against the length
    /// of the flight — and has no setting for it; the host's path check wants
    /// a number, so this is ours and is not the per-pass retry budget.
    /// </summary>
    public int CollisionSampleBudget { get; set; } = 500;
    public double ArcRange { get; set; } = 5d;
    public double RingDistance { get; set; } = 5d;
    public int MinimumRingTargets { get; set; } = 4;
    public bool DeleteGhostMonsters { get; set; } = true;
    public int GhostMonsterSpellAttemptCount { get; set; } = 200;
    public int BlacklistMonsterAttemptCount { get; set; } = 4;
    public double BlacklistMonsterTimeoutSeconds { get; set; } = 120d;
    public bool DeleteGhostMonstersByHealthTracker { get; set; } = true;
    public double GhostDeleteHealthTrackerSeconds { get; set; } = 30d;
    public bool SummonPets { get; set; } = true;
    public PetRangeMode PetRangeMode { get; set; } = PetRangeMode.AttackDistance;
    public double PetCustomRange { get; set; } = 5d;
    public int PetMonsterDensity { get; set; } = 1;
    public int PetRefillCountIdle { get; set; } = 3;
    public int PetRefillCountNormal { get; set; } = 1;
    public bool AllowDebuffFallback { get; set; }
    public int UseSpecialAmmo { get; set; }
    public bool WhoYouGonnaCall { get; set; } = true;
    public bool AutoFellowManagement { get; set; } = true;
    public string BlacklistedSpellComponents { get; set; } = string.Empty;
    public ISet<uint> CombatItemObjectIds { get; } = new HashSet<uint>();
    public IList<uint> CombatItemOrderIds { get; } = new List<uint>();
    internal ISet<uint> RemovedCombatItemObjectIds { get; } = new HashSet<uint>();
    internal IList<BuffedItemKey> RemovedBuffedItemRows { get; } =
        new List<BuffedItemKey>();
    public ISet<string> CombatItemNames { get; } =
        new HashSet<string>(StringComparer.Ordinal);
    public IList<string> CombatItemOrder { get; } = new List<string>();
    public ISet<string> ConsumableNames { get; } =
        new HashSet<string>(StringComparer.Ordinal);
    /// <summary>
    /// The reference's heal-kit table from its game-info database, by item
    /// name: the restore and skill bonuses a kit is used with, read without
    /// ever appraising the kit. Set by the host from the loaded database.
    /// </summary>
    public IReadOnlyDictionary<string, VtankHealKit> HealKits { get; set; } =
        new Dictionary<string, VtankHealKit>(StringComparer.OrdinalIgnoreCase);

    public IDictionary<string, ConsumableCategory> ConsumableCategories { get; } =
        new Dictionary<string, ConsumableCategory>(StringComparer.Ordinal);
    internal IList<AssistItem> ImportedAssistItems { get; } = new List<AssistItem>();
    internal IDictionary<uint, int> ItemUseSpecifiers { get; } = new Dictionary<uint, int>();
    public IList<MonsterRule> Rules { get; } =
        new List<MonsterRule> { new("DEFAULT", 0) };

    /// <summary>
    /// Where a rule's <c>species</c> and <c>maxhp</c> come from. Both are
    /// database facts, not live properties; without a database they answer
    /// the way an unlisted monster answers.
    /// </summary>
    public MonsterFactTable MonsterFacts { get; set; } = new();

    /// <summary>
    /// Which rule this monster matches. A pure lookup: the fact table is
    /// taught what a live monster says about its species by the scan, not by
    /// this.
    /// </summary>
    public ResolvedMonsterRule ResolveRule(PluginCombatTarget target)
    {
        var context = new MonsterExpressionContext(
            target.Name,
            target.WeenieClassId,
            MonsterFacts.Species(target.Name),
            MonsterFacts.MaximumHealth(target.Name),
            target.Distance,
            target.HasShield,
            MetaState,
            ResolveSetting);
        return MonsterRuleResolver.Resolve(Rules, context);
    }

    public string MetaState { get; set; } = "Default";
    public IDictionary<string, MonsterValue> DynamicSettings { get; } =
        new Dictionary<string, MonsterValue>(StringComparer.OrdinalIgnoreCase);

    private MonsterValue? ResolveSetting(string name) =>
        DynamicSettings.TryGetValue(name, out MonsterValue value)
            ? value
            : null;
}

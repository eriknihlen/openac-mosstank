using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

/// <summary>The essence the summon rule will use and the monster it chose it for.</summary>
internal readonly record struct PetSummonChoice(
    PluginInventoryItem Device,
    PluginCombatTarget Target)
{
    public static PetSummonChoice None => default;

    public bool IsNone => Device.ObjectId == 0u;
}

/// <summary>
/// The two questions the pet rules ask: which essence to summon with, for
/// which monster, and which essence to top up with which spirit.
/// </summary>
internal static class PetAutomation
{
    /// <summary>The character level an item asks for before it can be used.</summary>
    public const uint UseRequiresLevelProperty = 369u;

    /// <summary>An item's remaining uses.</summary>
    public const uint StructureProperty = 92u;

    public const string EncapsulatedSpiritName = "Encapsulated Spirit";

    private const uint SummoningSkillId = 54u;

    /// <summary>
    /// The summon rule's choice. The monster is the nearest one that also
    /// outranks every nearer one; the count of monsters that want a pet has to
    /// reach the density setting; then every usable essence on the Items page
    /// is ranked by how well its element suits that monster, and the one with
    /// the higher level requirement wins a tie. Any usable essence can win:
    /// the ranking orders them, it never rules one out.
    /// </summary>
    /// <param name="attackElement">
    /// The element the attack itself would strike this monster with.
    /// </param>
    /// <param name="damagePreferences">
    /// The elements the game-info database lists for this monster, best first.
    /// </param>
    /// <param name="warn">Where a once-per-cause warning goes.</param>
    public static PetSummonChoice SelectPet(
        IItemAutomation automation,
        IReadOnlyList<PluginInventoryItem> owned,
        IReadOnlyList<PluginCombatTarget> targets,
        ICharacterInfo character,
        CombatSettings settings,
        Func<PluginCombatTarget, MonsterDamageType> attackElement,
        Func<string, IReadOnlyList<MonsterDamageType>> damagePreferences,
        Action<string>? warn = null)
    {
        ArgumentNullException.ThrowIfNull(automation);
        ArgumentNullException.ThrowIfNull(owned);
        ArgumentNullException.ThrowIfNull(targets);
        ArgumentNullException.ThrowIfNull(character);
        ArgumentNullException.ThrowIfNull(settings);
        ArgumentNullException.ThrowIfNull(attackElement);
        ArgumentNullException.ThrowIfNull(damagePreferences);

        float range = PetRange(settings);
        PluginCombatTarget chosen = default;
        bool found = false;
        double nearest = double.MaxValue;
        int highest = -1;
        MonsterDamageType wanted = MonsterDamageType.None;
        int wanting = 0;
        foreach (PluginCombatTarget target in targets)
        {
            if (target.Distance > range)
                continue;
            ResolvedMonsterRule rule = settings.ResolveRule(target);
            if (rule.Priority < 0)
                continue;
            if (rule.Actions.PetDamageType == MonsterDamageType.None)
                continue;
            wanting++;
            // Both at once, as the reference asks it: a monster takes the pick
            // only by being nearer AND ranked higher than the one holding it.
            if (target.Distance < nearest && rule.Priority > highest)
            {
                highest = rule.Priority;
                chosen = target;
                found = true;
                nearest = target.Distance;
                wanted = rule.Actions.PetDamageType;
            }
        }
        if (!found)
            return PetSummonChoice.None;
        if (wanting < settings.PetMonsterDensity)
            return PetSummonChoice.None;

        MonsterDamageType attack = attackElement(chosen);
        IReadOnlyList<MonsterDamageType> preferences = damagePreferences(chosen.Name);

        PluginInventoryItem? best = null;
        int bestRank = int.MaxValue;
        int bestLevel = 0;
        foreach (PluginInventoryItem item in InItemsPageOrder(owned, settings))
        {
            if (!automation.TryCaptureProperties(item.ObjectId, out PluginItemProperties properties)
                || !item.IsPetDevice)
            {
                continue;
            }
            if (!IsUsable(in item, in properties, character, warn))
                continue;
            MonsterDamageType element = DeviceElement(item.WeenieClassId);
            int level = properties.Ints.GetValueOrDefault(UseRequiresLevelProperty);
            int rank = Rank(wanted, element, attack, preferences);
            if (best is null || rank < bestRank || (level > bestLevel && rank == bestRank))
            {
                bestLevel = level;
                bestRank = rank;
                best = item;
            }
        }
        return best is { } device
            ? new PetSummonChoice(device, chosen)
            : PetSummonChoice.None;
    }

    /// <summary>
    /// The refill rule's choice: the first essence on the Items page that is
    /// down to the threshold and short of full, and the smallest stack of
    /// spirits to top it up with. Monsters and the summon setting play no
    /// part.
    /// </summary>
    public static bool TrySelectRefill(
        IItemAutomation automation,
        IReadOnlyList<PluginInventoryItem> owned,
        CombatSettings settings,
        int threshold,
        out PluginInventoryItem device,
        out PluginInventoryItem spirit)
    {
        ArgumentNullException.ThrowIfNull(automation);
        ArgumentNullException.ThrowIfNull(owned);
        ArgumentNullException.ThrowIfNull(settings);
        device = default;
        spirit = default;
        bool found = false;
        foreach (PluginInventoryItem item in InItemsPageOrder(owned, settings))
        {
            if (!automation.TryCaptureProperties(item.ObjectId, out PluginItemProperties properties)
                || !item.IsPetDevice)
            {
                continue;
            }
            int structure = Structure(in item, in properties, unknown: 99999);
            if (structure <= threshold && structure < item.MaximumStructure)
            {
                device = item;
                found = true;
                break;
            }
        }
        if (!found)
            return false;
        if (SmallestStack(owned, EncapsulatedSpiritName) is not { } smallest)
        {
            device = default;
            return false;
        }
        spirit = smallest;
        return true;
    }

    public static float PetRange(CombatSettings settings) =>
        (float)(settings.PetRangeMode == PetRangeMode.Custom
            ? settings.PetCustomRange
            : settings.MaximumRange);

    /// <summary>
    /// The owned items the Items page lists, in the page's order: the rows
    /// named by object id first, as the page holds them, then anything the
    /// page names only by name.
    /// </summary>
    internal static IReadOnlyList<PluginInventoryItem> InItemsPageOrder(
        IReadOnlyList<PluginInventoryItem> owned,
        CombatSettings settings)
    {
        var ordered = new List<PluginInventoryItem>();
        var placed = new HashSet<uint>();
        foreach (uint id in settings.CombatItemOrderIds)
        {
            foreach (PluginInventoryItem item in owned)
            {
                if (item.ObjectId == id && placed.Add(id))
                {
                    ordered.Add(item);
                    break;
                }
            }
        }
        foreach (PluginInventoryItem item in owned)
        {
            if (placed.Contains(item.ObjectId))
                continue;
            if (settings.CombatItemObjectIds.Contains(item.ObjectId)
                || settings.CombatItemNames.Contains(item.Name))
            {
                placed.Add(item.ObjectId);
                ordered.Add(item);
            }
        }
        return ordered;
    }

    /// <summary>
    /// How well an essence suits the monster; lower is better. An element the
    /// monster rule names outright comes first, then the element the attack
    /// strikes with, then the monster's listed weaknesses in their order.
    /// Anything else ranks last but still counts.
    /// </summary>
    private static int Rank(
        MonsterDamageType wanted,
        MonsterDamageType element,
        MonsterDamageType attack,
        IReadOnlyList<MonsterDamageType> preferences)
    {
        switch (wanted)
        {
            case MonsterDamageType.Pierce:
            case MonsterDamageType.Bludgeon:
            case MonsterDamageType.Slash:
            case MonsterDamageType.Acid:
            case MonsterDamageType.Electric:
            case MonsterDamageType.Cold:
            case MonsterDamageType.Fire:
                if (wanted == element)
                    return 0;
                if (attack == element)
                    return 1;
                return PreferenceRank(element, preferences);
            case MonsterDamageType.PlayerAuto:
                if (attack == element)
                    return 0;
                return PreferenceRank(element, preferences);
            case MonsterDamageType.Auto:
                return PreferenceRank(element, preferences);
            default:
                return int.MaxValue;
        }
    }

    private static int PreferenceRank(
        MonsterDamageType element,
        IReadOnlyList<MonsterDamageType> preferences)
    {
        for (int index = 0; index < preferences.Count; index++)
        {
            if (preferences[index] == element)
                return index + 10;
        }
        return int.MaxValue;
    }

    /// <summary>
    /// An essence this character can use now: its level and summoning-skill
    /// requirements met, charges left, and no other summoning mastery's.
    /// </summary>
    private static bool IsUsable(
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        ICharacterInfo character,
        Action<string>? warn)
    {
        if (properties.Ints.GetValueOrDefault(UseRequiresLevelProperty) > character.Level)
            return false;
        uint summoning = character.TryGetSkill(SummoningSkillId, out PluginSkillInfo skill)
            ? skill.Current
            : 0u;
        if (item.UseRequiresSkillLevel > summoning)
            return false;
        if (Structure(in item, in properties, unknown: 9999) == 0)
            return false;
        if (item.SummoningMastery != 0
            && item.SummoningMastery != character.SummoningMastery)
        {
            warn?.Invoke("Warning: ignoring pet " + item.Name
                + " because you have the wrong summoning mastery!");
            return false;
        }
        return true;
    }

    /// <summary>
    /// The essence's charges: the assessed number when there is one, else the
    /// item's own, else the given stand-in for "not known".
    /// </summary>
    private static int Structure(
        in PluginInventoryItem item,
        in PluginItemProperties properties,
        int unknown)
    {
        if (properties.Ints.TryGetValue(StructureProperty, out int assessed))
            return assessed;
        return item.MaximumStructure > 0 ? item.Structure : unknown;
    }

    /// <summary>
    /// The element an essence's pet strikes with. An essence the table does
    /// not know strikes as a bludgeon, as the golems do.
    /// </summary>
    private static MonsterDamageType DeviceElement(uint weenieClassId)
    {
        MonsterDamageType element = PetDeviceCatalog.DamageType(weenieClassId);
        return element == MonsterDamageType.Auto
            ? MonsterDamageType.Bludgeon
            : element;
    }

    private static PluginInventoryItem? SmallestStack(
        IReadOnlyList<PluginInventoryItem> owned,
        string name)
    {
        PluginInventoryItem? smallest = null;
        int fewest = int.MaxValue;
        foreach (PluginInventoryItem item in owned)
        {
            if (!string.Equals(item.Name, name, StringComparison.OrdinalIgnoreCase))
                continue;
            int count = Math.Max(1, item.StackSize);
            if (count < fewest)
            {
                fewest = count;
                smallest = item;
            }
        }
        return smallest;
    }
}

/// <summary>
/// Tops a pet essence's charges back up. The rule stands twice in the pass:
/// high up with the tight "normal" threshold, and in the idle band with the
/// looser one, because how empty an essence has to be before it is worth
/// stopping for depends on whether there is anything else to do.
/// </summary>
internal sealed class PetRefillRule
{
    private readonly IPluginHost _host;
    private readonly CombatSettings _settings;
    private readonly Func<int> _threshold;
    private readonly Func<bool> _readyToRefillInPeace;

    public PetRefillRule(
        IPluginHost host,
        CombatSettings settings,
        Func<int> threshold,
        Func<bool> readyToRefillInPeace)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _settings = settings ?? throw new ArgumentNullException(nameof(settings));
        _threshold = threshold ?? throw new ArgumentNullException(nameof(threshold));
        _readyToRefillInPeace = readyToRefillInPeace
            ?? throw new ArgumentNullException(nameof(readyToRefillInPeace));
    }

    public string Status { get; private set; } = string.Empty;

    /// <summary>
    /// How long a refused refill waits before it is asked for again, so a
    /// refusal is not re-sent on every pass.
    /// </summary>
    internal const double RefusalRetrySeconds = 1d;

    private double _now;
    private double _retryAt = double.NegativeInfinity;
    private bool _retryHoldsTurn;

    /// <summary>
    /// Valid when an essence on the Items page is low and a spirit is at
    /// hand; its turn drops to peace first, then uses the spirit on the
    /// essence. A refused use is not asked for again until the retry clock
    /// has run: while it runs, a refusal because the character was busy
    /// keeps the turn (the refill is still what is wanted, the host is just
    /// not ready), and any other refusal gives the turn up so the rules
    /// below it get their go.
    /// </summary>
    public bool Tick(MacroPassContext context)
    {
        _now += Math.Max(0d, context.ElapsedSeconds);
        IAutomationSurface automation = _host.Automation;
        Status = string.Empty;
        if (!automation.IsAvailable || !automation.Items.IsAvailable)
            return false;
        if (!PetAutomation.TrySelectRefill(
                automation.Items,
                automation.Items.CaptureOwnedItems(),
                _settings,
                _threshold(),
                out PluginInventoryItem device,
                out PluginInventoryItem spirit))
        {
            return false;
        }
        if (_now < _retryAt)
        {
            Status = $"Waiting to refill {device.Name}";
            return _retryHoldsTurn;
        }
        if (!context.CanAct)
            return true;
        if (!_readyToRefillInPeace())
        {
            Status = "Entering peace mode to refill the combat pet";
            return true;
        }
        PluginItemCommandResult result = automation.Items.Apply(
            spirit.ObjectId,
            device.ObjectId);
        if (result.Status == PluginItemCommandStatus.Started)
        {
            Status = $"Refilling {device.Name}";
            return true;
        }
        _retryAt = _now + RefusalRetrySeconds;
        _retryHoldsTurn = result.Status == PluginItemCommandStatus.Busy;
        Status = result.Notice ?? $"Combat pet refill refused: {result.Status}";
        return _retryHoldsTurn;
    }

    public void Reset()
    {
        _now = 0d;
        _retryAt = double.NegativeInfinity;
        _retryHoldsTurn = false;
        Status = string.Empty;
    }
}

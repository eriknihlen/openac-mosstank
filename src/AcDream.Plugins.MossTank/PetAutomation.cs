using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal enum PetAutomationActionKind
{
    None,
    Refill,
    Summon,
}

internal readonly record struct PetAutomationChoice(
    PetAutomationActionKind Kind,
    PluginInventoryItem Device,
    PluginInventoryItem Tool,
    PluginCombatTarget Target,
    MonsterDamageType DamageType)
{
    public static PetAutomationChoice None => default;
}

internal sealed class PetAutomation
{
    private const double RetailPetCooldownSeconds = 45d;
    private const double RefusalRetrySeconds = 1d;

    private long _observedCompletionRevision;
    private uint _pendingSourceId;
    private PetAutomationActionKind _pendingKind;
    private double _nextSummonAt;
    private double _nextRefillAt;

    public bool Tick(
        IItemAutomation automation,
        ICharacterInfo character,
        IReadOnlyList<PluginCombatTarget> targets,
        CombatSettings settings,
        double now,
        out string status,
        bool allowRefill = true,
        bool allowSummon = true,
        Func<bool>? readyToRefillInPeace = null,
        IReadOnlyList<PluginInventoryItem>? captured = null)
    {
        ArgumentNullException.ThrowIfNull(automation);
        ArgumentNullException.ThrowIfNull(character);
        ArgumentNullException.ThrowIfNull(targets);
        ArgumentNullException.ThrowIfNull(settings);

        ObserveCompletion(automation.LastCompletion, now, out string? completion);
        if (completion is not null)
            status = completion;
        else
            status = string.Empty;

        if (_pendingSourceId != 0u)
        {
            status = _pendingKind == PetAutomationActionKind.Refill
                ? "Refilling combat pet"
                : "Summoning combat pet";
            return true;
        }
        if (!settings.SummonPets || !automation.IsAvailable)
            return false;
        if (automation.IsBusy)
        {
            status = "Waiting to use combat pet";
            return true;
        }

        // The caller's own per-pass projection when it has one: building it
        // walks every object the client knows.
        IReadOnlyList<PluginInventoryItem> items =
            captured ?? automation.CaptureOwnedItems();
        PetAutomationChoice choice = Select(
            items,
            targets,
            character,
            settings,
            automation.ActiveOwnedPetCount,
            allowRefill && now >= _nextRefillAt,
            allowSummon && now >= _nextSummonAt);
        if (choice.Kind == PetAutomationActionKind.None)
            return false;

        if (choice.Kind == PetAutomationActionKind.Refill
            && readyToRefillInPeace is not null
            && !readyToRefillInPeace())
        {
            status = "Entering peace mode to refill the combat pet";
            return true;
        }

        PluginItemCommandResult result = choice.Kind == PetAutomationActionKind.Refill
            ? automation.Apply(choice.Tool.ObjectId, choice.Device.ObjectId)
            : automation.Use(choice.Device.ObjectId);
        if (result.Status == PluginItemCommandStatus.Started)
        {
            _pendingSourceId = choice.Kind == PetAutomationActionKind.Refill
                ? choice.Tool.ObjectId
                : choice.Device.ObjectId;
            _pendingKind = choice.Kind;
            status = choice.Kind == PetAutomationActionKind.Refill
                ? $"Refilling {choice.Device.Name}"
                : $"Summoning {choice.Device.Name} for {choice.Target.Name}";
            return true;
        }

        if (choice.Kind == PetAutomationActionKind.Refill)
            _nextRefillAt = now + RefusalRetrySeconds;
        else
            _nextSummonAt = now + RefusalRetrySeconds;
        status = result.Notice
            ?? $"Combat pet action refused: {result.Status}";
        return true;
    }

    /// <summary>Folds the last item completion into the pending state without acting.</summary>
    public void Observe(IItemAutomation automation, double now)
    {
        ArgumentNullException.ThrowIfNull(automation);
        ObserveCompletion(automation.LastCompletion, now, out _);
    }

    /// <summary>
    /// The summon rule's predicate: the device to use and the monster it is
    /// for, chosen without issuing anything. False while a use of ours is
    /// still unanswered, while the summon is on its retry clock, or when
    /// nothing qualifies.
    /// </summary>
    public bool TrySelectSummon(
        IItemAutomation automation,
        ICharacterInfo character,
        IReadOnlyList<PluginCombatTarget> targets,
        CombatSettings settings,
        double now,
        out PetAutomationChoice choice)
    {
        ArgumentNullException.ThrowIfNull(automation);
        ArgumentNullException.ThrowIfNull(character);
        ArgumentNullException.ThrowIfNull(targets);
        ArgumentNullException.ThrowIfNull(settings);
        choice = PetAutomationChoice.None;
        if (_pendingSourceId != 0u || !settings.SummonPets || !automation.IsAvailable)
            return false;
        if (now < _nextSummonAt)
            return false;
        choice = Select(
            automation.CaptureOwnedItems(),
            targets,
            character,
            settings,
            automation.ActiveOwnedPetCount,
            allowRefill: false,
            allowSummon: true);
        return choice.Kind == PetAutomationActionKind.Summon;
    }

    /// <summary>The summon rule's turn: uses the device the predicate chose.</summary>
    public string IssueSummon(IItemAutomation automation, PetAutomationChoice choice, double now)
    {
        ArgumentNullException.ThrowIfNull(automation);
        if (choice.Kind != PetAutomationActionKind.Summon || _pendingSourceId != 0u)
            return string.Empty;
        PluginItemCommandResult result = automation.Use(choice.Device.ObjectId);
        if (result.Status == PluginItemCommandStatus.Started)
        {
            _pendingSourceId = choice.Device.ObjectId;
            _pendingKind = PetAutomationActionKind.Summon;
            return $"Summoning {choice.Device.Name} for {choice.Target.Name}";
        }
        _nextSummonAt = now + RefusalRetrySeconds;
        return result.Notice ?? $"Combat pet action refused: {result.Status}";
    }

    /// <summary>
    /// The refill on its own, with its own charge threshold and no interest
    /// in monsters: a combat-item pet device of mine is below the threshold
    /// and short of full, and I am carrying a spirit to top it up with. The
    /// reference rule reads the combat-items list and those two structure
    /// numbers and nothing else, which is why it can hold a position far
    /// from the attack and still be the same rule.
    /// </summary>
    public bool TickRefill(
        IItemAutomation automation,
        CombatSettings settings,
        int refillThreshold,
        double now,
        out string status,
        Func<bool>? readyToRefillInPeace = null,
        bool canAct = true)
    {
        ArgumentNullException.ThrowIfNull(automation);
        ArgumentNullException.ThrowIfNull(settings);

        ObserveCompletion(automation.LastCompletion, now, out string? completion);
        status = completion ?? string.Empty;

        if (_pendingSourceId != 0u)
        {
            status = "Refilling combat pet";
            return true;
        }
        // A blocked pass still watches for the completion above; it must not
        // start anything.
        if (!canAct)
            return false;
        if (!automation.IsAvailable)
            return false;
        // The reference rule asks nothing about the host being busy: its
        // predicate is a low device and a spirit to use on it. A pass is
        // never held for a use that has not been issued.
        if (automation.IsBusy)
            return false;
        if (now < _nextRefillAt)
            return false;

        IReadOnlyList<PluginInventoryItem> items = automation.CaptureOwnedItems();
        if (SelectRefillDevice(items, settings, refillThreshold) is not { } device)
            return false;
        if (FindSpirit(items) is not { } spirit)
            return false;
        if (readyToRefillInPeace is not null && !readyToRefillInPeace())
        {
            status = "Entering peace mode to refill the combat pet";
            return true;
        }

        PluginItemCommandResult result = automation.Apply(
            spirit.ObjectId,
            device.ObjectId);
        if (result.Status == PluginItemCommandStatus.Started)
        {
            _pendingSourceId = spirit.ObjectId;
            _pendingKind = PetAutomationActionKind.Refill;
            status = $"Refilling {device.Name}";
            return true;
        }

        _nextRefillAt = now + RefusalRetrySeconds;
        status = result.Notice ?? $"Combat pet action refused: {result.Status}";
        return false;
    }

    /// <summary>
    /// First match in list order, not the best one: the reference rule stops
    /// at the first low device it walks past.
    /// </summary>
    private static PluginInventoryItem? SelectRefillDevice(
        IReadOnlyList<PluginInventoryItem> items,
        CombatSettings settings,
        int refillThreshold)
    {
        int threshold = Math.Max(0, refillThreshold);
        foreach (PluginInventoryItem item in items)
        {
            if (!settings.CombatItemObjectIds.Contains(item.ObjectId)
                && !settings.CombatItemNames.Contains(item.Name))
            {
                continue;
            }
            if (!item.IsPetDevice)
                continue;
            if (item.Structure <= threshold && item.Structure < item.MaximumStructure)
                return item;
        }
        return null;
    }

    internal static PetAutomationChoice Select(
        IReadOnlyList<PluginInventoryItem> items,
        IReadOnlyList<PluginCombatTarget> targets,
        ICharacterInfo character,
        CombatSettings settings,
        int activeOwnedPetCount,
        bool allowRefill,
        bool allowSummon)
    {
        if (!settings.SummonPets || activeOwnedPetCount > 0)
            return PetAutomationChoice.None;

        float range = (float)(settings.PetRangeMode == PetRangeMode.Custom
            ? settings.PetCustomRange
            : settings.MaximumRange);
        int density = Math.Max(1, settings.PetMonsterDensity);
        var eligible = new List<(PluginCombatTarget Target, ResolvedMonsterRule Rule)>();
        foreach (PluginCombatTarget target in targets)
        {
            if (target.Distance > range)
                continue;
            ResolvedMonsterRule rule = settings.ResolveRule(target);
            if (rule.Priority < 0
                || rule.Actions.PetDamageType == MonsterDamageType.None)
            {
                continue;
            }
            eligible.Add((target, rule));
        }
        if (eligible.Count < density)
            return PetAutomationChoice.None;

        eligible.Sort(static (left, right) =>
        {
            int priority = right.Rule.Priority.CompareTo(left.Rule.Priority);
            return priority != 0
                ? priority
                : left.Target.Distance.CompareTo(right.Target.Distance);
        });
        (PluginCombatTarget selectedTarget, ResolvedMonsterRule targetRule) = eligible[0];
        MonsterDamageType desired = ResolveDesiredDamage(targetRule.Actions);

        PluginInventoryItem? device = SelectDevice(
            items,
            character,
            desired,
            settings,
            allowFallback: targetRule.Actions.PetDamageType
                == MonsterDamageType.PlayerAuto);
        if (device is not { } selected)
            return PetAutomationChoice.None;

        int refillThreshold = Math.Max(0, settings.PetRefillCountNormal);
        if (allowRefill
            && selected.MaximumStructure > 0
            && selected.Structure <= refillThreshold
            && selected.Structure < selected.MaximumStructure
            && FindSpirit(items) is { } spirit)
        {
            return new PetAutomationChoice(
                PetAutomationActionKind.Refill,
                selected,
                spirit,
                selectedTarget,
                desired);
        }
        if (!allowSummon || selected.Structure <= 0)
            return PetAutomationChoice.None;
        return new PetAutomationChoice(
            PetAutomationActionKind.Summon,
            selected,
            default,
            selectedTarget,
            desired);
    }

    private static MonsterDamageType ResolveDesiredDamage(
        MonsterRuleActions actions)
    {
        if (actions.PetDamageType != MonsterDamageType.PlayerAuto)
            return actions.PetDamageType;
        return actions.DamageType is
            MonsterDamageType.Bludgeon or MonsterDamageType.Acid
            or MonsterDamageType.Fire or MonsterDamageType.Cold
            or MonsterDamageType.Electric
            ? actions.DamageType
            : MonsterDamageType.Auto;
    }

    private static PluginInventoryItem? SelectDevice(
        IReadOnlyList<PluginInventoryItem> items,
        ICharacterInfo character,
        MonsterDamageType desired,
        CombatSettings settings,
        bool allowFallback)
    {
        PluginInventoryItem? exact = null;
        PluginInventoryItem? fallback = null;
        foreach (PluginInventoryItem item in items)
        {
            if (!settings.CombatItemObjectIds.Contains(item.ObjectId)
                && !settings.CombatItemNames.Contains(item.Name))
            {
                continue;
            }
            if (!item.IsPetDevice || !CanUse(item, character))
                continue;
            MonsterDamageType damage = PetDeviceCatalog.DamageType(
                item.WeenieClassId);
            if (fallback is null || Better(item, fallback.Value))
                fallback = item;
            if (desired != MonsterDamageType.Auto && damage != desired)
                continue;
            if (exact is null || Better(item, exact.Value))
                exact = item;
        }
        if (exact is not null)
            return exact;
        return desired == MonsterDamageType.Auto || allowFallback
            ? fallback
            : null;
    }

    private static bool CanUse(
        in PluginInventoryItem item,
        ICharacterInfo character)
    {
        if (item.SummoningMastery != 0
            && item.SummoningMastery != character.SummoningMastery)
        {
            return false;
        }
        if (item.UseRequiresSkill == 0)
            return true;
        if (!character.TryGetSkill((uint)item.UseRequiresSkill, out PluginSkillInfo skill)
            || skill.Current < item.UseRequiresSkillLevel)
        {
            return false;
        }
        return item.UseRequiresSkillSpecialized == 0
            || skill.Training == PluginSkillTraining.Specialized;
    }

    private static bool Better(
        in PluginInventoryItem candidate,
        in PluginInventoryItem incumbent)
    {
        int candidateRating = candidate.GearDamage
            + candidate.GearCriticalChance
            + candidate.GearCriticalDamage;
        int incumbentRating = incumbent.GearDamage
            + incumbent.GearCriticalChance
            + incumbent.GearCriticalDamage;
        if (candidate.UseRequiresSkillLevel != incumbent.UseRequiresSkillLevel)
            return candidate.UseRequiresSkillLevel > incumbent.UseRequiresSkillLevel;
        if (candidateRating != incumbentRating)
            return candidateRating > incumbentRating;
        if (candidate.Structure != incumbent.Structure)
            return candidate.Structure > incumbent.Structure;
        return candidate.ObjectId < incumbent.ObjectId;
    }

    private static PluginInventoryItem? FindSpirit(
        IReadOnlyList<PluginInventoryItem> items)
    {
        foreach (PluginInventoryItem item in items)
        {
            if (item.WeenieClassId == PetDeviceCatalog.EncapsulatedSpiritWeenieClassId
                && item.StackSize > 0)
            {
                return item;
            }
        }
        return null;
    }

    private void ObserveCompletion(
        PluginItemUseCompletion completion,
        double now,
        out string? status)
    {
        status = null;
        if (completion.Revision == 0
            || completion.Revision == _observedCompletionRevision)
        {
            return;
        }
        _observedCompletionRevision = completion.Revision;
        if (_pendingSourceId == 0u
            || completion.SourceObjectId != _pendingSourceId)
        {
            return;
        }

        PetAutomationActionKind completed = _pendingKind;
        _pendingSourceId = 0u;
        _pendingKind = PetAutomationActionKind.None;
        if (completed == PetAutomationActionKind.Summon)
            _nextSummonAt = now + RetailPetCooldownSeconds;
        else
            _nextRefillAt = now + RefusalRetrySeconds;
        status = completion.IsSuccess
            ? completed == PetAutomationActionKind.Summon
                ? "Combat pet summoned"
                : "Combat pet refilled"
            : $"Combat pet failed (0x{completion.WeenieError:X})";
    }
}

/// <summary>
/// A pass position that does nothing but top a combat pet's charges back
/// up. The rule exists twice in the reference pass — once high up, with the
/// tight "normal" threshold, and once in the idle band with the looser one
/// — because how empty a device has to be before it is worth stopping for
/// depends on whether there is anything else to do. Only the second one
/// stands here: the first is part of the attack.
/// </summary>
internal sealed class PetRefillRule
{
    private readonly IPluginHost _host;
    private readonly CombatSettings _settings;
    private readonly Func<int> _threshold;
    private readonly Func<bool> _readyToRefillInPeace;
    private readonly PetAutomation _pets = new();
    private double _now;

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

    public bool Tick(MacroPassContext context)
    {
        _now += Math.Max(0d, context.ElapsedSeconds);
        IAutomationSurface automation = _host.Automation;
        if (!automation.IsAvailable)
            return false;
        bool claimed = _pets.TickRefill(
            automation.Items,
            _settings,
            _threshold(),
            _now,
            out string status,
            _readyToRefillInPeace,
            context.CanAct);
        Status = status;
        return claimed;
    }

    public void Reset()
    {
        _now = 0d;
        Status = string.Empty;
    }
}

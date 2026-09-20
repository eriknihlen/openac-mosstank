using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal enum VitalRechargeSourceKind
{
    LearnedSpell,
    CasterItem,
    Kit,
    Food,
}

internal readonly record struct VitalRechargeChoice(
    VitalKind Vital,
    VitalRechargeSourceKind SourceKind,
    string Name,
    uint SpellId,
    uint ItemObjectId,
    PluginCombatMode? RequiredMode)
{
    public bool UsesItem => SourceKind != VitalRechargeSourceKind.LearnedSpell;
    public uint TargetObjectId { get; init; }
}

internal enum VitalRechargeMethod
{
    RegularSpell,
    StaminaToHealth,
    ManaToHealth,
    HealthToStamina,
    HealthToMana,
    Kit,
    Food,
}

public readonly record struct RechargeHandlerRow(
    int Vital,
    string HandlerString,
    int MinPercent,
    int MaxPercent,
    int Stance)
{
    internal static VitalKind? ToVitalKind(int vital) => vital switch
    {
        1 => VitalKind.Health,
        2 => VitalKind.Stamina,
        3 => VitalKind.Mana,
        _ => null,
    };

    internal static int FromVitalKind(VitalKind vital) => vital switch
    {
        VitalKind.Health => 1,
        VitalKind.Stamina => 2,
        VitalKind.Mana => 3,
        _ => 0,
    };
}

internal static class VitalRechargePlanner
{
    private const uint HealingSkill = 21u;
    private const uint HealingKitPublicFlag = 0x00010000u;
    private const uint CasterItemType = 0x00008000u;

    public static bool TryPlan(
        VitalKind vital,
        IAutomationSurface automation,
        VitalSettings settings,
        CombatSettings combatSettings,
        out VitalRechargeChoice choice,
        Action<string>? trace = null)
    {
        ArgumentNullException.ThrowIfNull(automation);
        ArgumentNullException.ThrowIfNull(settings);
        ArgumentNullException.ThrowIfNull(combatSettings);

        PluginCombatMode mode = automation.Combat.Snapshot.Mode;
        int percent = VitalPlan.Percent(automation.Character, vital);
        IReadOnlyList<VitalRechargeMethod> handlers = Handlers(
            vital,
            mode == PluginCombatMode.Magic,
            percent,
            settings.RechargeHandlerSet,
            settings.RechargeHandlerRows.Count != 0
                ? settings.RechargeHandlerRows
                : null);
        IReadOnlyList<PluginInventoryItem> items =
            automation.Items.CaptureOwnedItems();

        if (trace is not null)
        {
            ICharacterInfo character = automation.Character;
            bool hasHealing = character.TryGetSkill(HealingSkill, out PluginSkillInfo healing);
            trace($"Recharge check: vital={vital}, percent={percent}, mode={mode}, handlers={string.Join(",", handlers)}, health={character.CurrentHealth}/{character.MaxHealth}, stamina={character.CurrentStamina}/{character.MaxStamina}, Healing={healing.Current}/{healing.Training}, skillPresent={hasHealing}, kitsInMagic={settings.UseKitsInMagicMode}, minimumKitChance={settings.MinimumHealKitSuccessChance}, healthThresholds={settings.NormalHealth}/{settings.NoTargetHealth}");
            foreach (PluginInventoryItem item in items)
            {
                bool configured = combatSettings.ConsumableNames.Contains(item.Name);
                if (!configured && (item.PublicFlags & HealingKitPublicFlag) == 0u
                    && !item.Name.Contains("kit", StringComparison.OrdinalIgnoreCase))
                    continue;
                double chance = HealKitChance(healing.Current, item.BoostValue, character, vital, mode) * 100d;
                string rejection = !configured ? "not configured"
                    : !ConfiguredSupplyReadiness.IsAssessed(automation, item.ObjectId) ? "awaiting assessment"
                    : mode == PluginCombatMode.Magic && !settings.UseKitsInMagicMode ? "kits disabled in magic"
                    : vital != VitalKind.Stamina && character.CurrentStamina < 15u ? "stamina below 15"
                    : !hasHealing || healing.Training is not (PluginSkillTraining.Trained or PluginSkillTraining.Specialized) ? "Healing not trained"
                    : item.BoosterVital != (int)vital ? "vital does not match"
                    : (item.PublicFlags & HealingKitPublicFlag) == 0u ? "missing healer flag"
                    : item.UseRequiresSkillLevel > healing.Current ? "skill level too low"
                    : item.UseRequiresSkillSpecialized != 0 && healing.Training != PluginSkillTraining.Specialized ? "specialization required"
                    : chance < settings.MinimumHealKitSuccessChance ? "success chance below minimum"
                    : "eligible";
                trace($"Kit check: {item.Name} (0x{item.ObjectId:X8}), result={rejection}, configured={configured}, flags=0x{item.PublicFlags:X8}, boosterVital={item.BoosterVital}, bonus={item.BoostValue}, modifier={item.HealKitModifier}, uses={item.Structure}, requiredSkill={item.UseRequiresSkill}, requiredLevel={item.UseRequiresSkillLevel}, requiredSpec={item.UseRequiresSkillSpecialized}, chance={chance:F2}%");
            }
        }

        foreach (VitalRechargeMethod handler in handlers)
        {
            if (TryHandler(
                    handler,
                    vital,
                    automation,
                    settings,
                    combatSettings,
                    items,
                    out choice))
            {
                trace?.Invoke($"Recharge selected: handler={handler}, source={choice.SourceKind}, name={choice.Name}");
                return true;
            }
        }
        choice = default;
        return false;
    }

    public static bool TryPlanHelper(
        IAutomationSurface automation,
        VitalSettings settings,
        out VitalRechargeChoice choice) => TryPlanHelper(
            automation,
            settings,
            new CombatSettings(),
            out choice);

    public static bool TryPlanHelper(
        IAutomationSurface automation,
        VitalSettings settings,
        CombatSettings combatSettings,
        out VitalRechargeChoice choice) => TryPlanHelper(
            automation,
            settings,
            combatSettings,
            trace: null,
            out choice);

    /// <summary>
    /// <paramref name="trace"/> receives the helper spell walk's pick and
    /// rejections, in the buff pass's own wording, so a connected run can
    /// show why a tier was skipped or nothing was cast at all.
    /// </summary>
    public static bool TryPlanHelper(
        IAutomationSurface automation,
        VitalSettings settings,
        CombatSettings combatSettings,
        Action<string>? trace,
        out VitalRechargeChoice choice)
    {
        ArgumentNullException.ThrowIfNull(automation);
        ArgumentNullException.ThrowIfNull(settings);
        if (!settings.HelpOthers || !automation.Fellowship.IsInFellowship)
        {
            choice = default;
            return false;
        }

        IReadOnlyList<PluginFellowMember> members =
            automation.Fellowship.CaptureMembers();
        foreach ((VitalKind vital, double threshold, double distance, uint baseSpell)
            in new[]
            {
                (VitalKind.Health, settings.HelperHealth,
                    settings.HelperHealthDistance, (uint)SpellId.AdjaSGift),
                (VitalKind.Stamina, settings.HelperStamina,
                    settings.HelperStaminaDistance, (uint)SpellId.Replenish),
                (VitalKind.Mana, settings.HelperMana,
                    settings.HelperManaDistance, (uint)SpellId.GiftOfEssence),
            })
        {
            PluginFellowMember? target = Lowest(members, vital, threshold, (float)distance);
            if (target is not { } fellow)
                continue;
            if (vital == VitalKind.Health
                && TryHealersHeart(
                    automation,
                    settings,
                    combatSettings,
                    fellow,
                    out choice))
            {
                return true;
            }
            if (!automation.Spells.TryGet(baseSpell, out PluginSpellInfo basis)
                || !TryResolveHelperSpell(
                    automation,
                    basis,
                    combatSettings,
                    trace,
                    out PluginSpellInfo spell))
            {
                continue;
            }

            choice = new VitalRechargeChoice(
                vital,
                VitalRechargeSourceKind.LearnedSpell,
                spell.Name,
                spell.SpellId,
                0u,
                PluginCombatMode.Magic)
            {
                TargetObjectId = fellow.ObjectId,
            };
            return true;
        }
        choice = default;
        return false;
    }

    private static bool TryHealersHeart(
        IAutomationSurface automation,
        VitalSettings settings,
        CombatSettings combatSettings,
        in PluginFellowMember target,
        out VitalRechargeChoice choice)
    {
        choice = default;
        // The item-use slot is the caller's gate, as the reference gates
        // this row: never the host's inventory transaction state, which a
        // cast of its own raises.
        if (!settings.UseHealersHeart)
            return false;
        if (!automation.Character.TryGetSkill(33u, out PluginSkillInfo life)
            || life.Current < 245u
            || !automation.Character.TryGetSkill(14u, out PluginSkillInfo arcaneLore)
            || arcaneLore.Current < 105u)
        {
            return false;
        }

        PluginInventoryItem selected = default;
        int rank = 0;
        foreach (PluginInventoryItem item in automation.Items.CaptureOwnedItems())
        {
            if (!combatSettings.CombatItemNames.Contains(item.Name))
                continue;
            if (item.ContainerObjectId != automation.Character.ObjectId
                && item.WielderObjectId != automation.Character.ObjectId)
            {
                continue;
            }
            int candidateRank = item.Name switch
            {
                "Legendary Seed of Mornings" => 2,
                "The Healer's Heart" => 1,
                _ => 0,
            };
            if (candidateRank <= rank)
                continue;
            selected = item;
            rank = candidateRank;
        }
        if (selected.ObjectId == 0u)
            return false;

        choice = new VitalRechargeChoice(
            VitalKind.Health,
            VitalRechargeSourceKind.CasterItem,
            selected.Name,
            0u,
            selected.ObjectId,
            null)
        {
            TargetObjectId = target.ObjectId,
        };
        return true;
    }

    internal static IReadOnlyList<VitalRechargeMethod> Handlers(
        VitalKind vital,
        bool magicMode,
        int currentPercent,
        string? handlerSet = null,
        IReadOnlyList<RechargeHandlerRow>? handlerRows = null)
    {
        if (handlerRows is { Count: > 0 }
            && TryHandlersFromRows(
                vital,
                magicMode,
                currentPercent,
                handlerRows,
                out VitalRechargeMethod[] fromRows))
        {
            return fromRows;
        }

        IReadOnlyList<VitalRechargeMethod> defaults;
        if (magicMode)
        {
            defaults = vital switch
            {
                VitalKind.Health when currentPercent <= 15 =>
                [
                    VitalRechargeMethod.StaminaToHealth,
                    VitalRechargeMethod.ManaToHealth,
                    VitalRechargeMethod.RegularSpell,
                    VitalRechargeMethod.Food,
                    VitalRechargeMethod.Kit,
                ],
                VitalKind.Health =>
                [
                    VitalRechargeMethod.Kit,
                    VitalRechargeMethod.StaminaToHealth,
                    VitalRechargeMethod.ManaToHealth,
                    VitalRechargeMethod.RegularSpell,
                    VitalRechargeMethod.Food,
                ],
                VitalKind.Stamina =>
                [
                    VitalRechargeMethod.Kit,
                    VitalRechargeMethod.RegularSpell,
                    VitalRechargeMethod.Food,
                ],
                VitalKind.Mana =>
                [
                    VitalRechargeMethod.Kit,
                    VitalRechargeMethod.Food,
                    VitalRechargeMethod.RegularSpell,
                ],
                _ => [],
            };
        }
        else
        {
            defaults = vital switch
            {
                VitalKind.Health when currentPercent <= 10 =>
                [
                    VitalRechargeMethod.Food,
                    VitalRechargeMethod.Kit,
                    VitalRechargeMethod.StaminaToHealth,
                    VitalRechargeMethod.RegularSpell,
                ],
                VitalKind.Health when currentPercent <= 15 =>
                [
                    VitalRechargeMethod.Food,
                    VitalRechargeMethod.Kit,
                    VitalRechargeMethod.RegularSpell,
                ],
                VitalKind.Health =>
                [
                    VitalRechargeMethod.Kit,
                    VitalRechargeMethod.Food,
                    VitalRechargeMethod.RegularSpell,
                ],
                VitalKind.Stamina or VitalKind.Mana =>
                [
                    VitalRechargeMethod.Kit,
                    VitalRechargeMethod.Food,
                    VitalRechargeMethod.RegularSpell,
                ],
                _ => [],
            };
        }

        return TryParseHandlerSet(
            handlerSet,
            HandlerContext(vital, magicMode, currentPercent),
            out VitalRechargeMethod[] custom)
                ? custom
                : defaults;
    }

    private static bool TryHandlersFromRows(
        VitalKind vital,
        bool magicMode,
        int currentPercent,
        IReadOnlyList<RechargeHandlerRow> rows,
        out VitalRechargeMethod[] handlers)
    {
        int vitalCode = RechargeHandlerRow.FromVitalKind(vital);
        int stance = magicMode ? 1 : 2;
        var matched = new List<VitalRechargeMethod>();
        foreach (RechargeHandlerRow row in rows)
        {
            if (row.Vital != vitalCode
                || row.Stance != stance
                || currentPercent < row.MinPercent
                || currentPercent > row.MaxPercent)
            {
                continue;
            }
            if (TryParseHandlerToken(row.HandlerString, out VitalRechargeMethod method))
                matched.Add(method);
        }
        handlers = [.. matched];
        return handlers.Length != 0;
    }

    private static bool TryParseHandlerToken(string source, out VitalRechargeMethod method)
    {
        string normalized = source.Replace(" ", string.Empty, StringComparison.Ordinal)
            .Replace("-", string.Empty, StringComparison.Ordinal)
            .ToLowerInvariant();
        switch (normalized)
        {
            case "regularspell":
                method = VitalRechargeMethod.RegularSpell;
                return true;
            case "staminatohealth":
                method = VitalRechargeMethod.StaminaToHealth;
                return true;
            case "manatohealth":
                method = VitalRechargeMethod.ManaToHealth;
                return true;
            case "healthtostamina":
                method = VitalRechargeMethod.HealthToStamina;
                return true;
            case "healthtomana":
                method = VitalRechargeMethod.HealthToMana;
                return true;
            case "kit":
            case "kitrecharge":
                method = VitalRechargeMethod.Kit;
                return true;
            case "food":
            case "rechargewithfood":
                method = VitalRechargeMethod.Food;
                return true;
            default:
                method = default;
                return false;
        }
    }

    private static string HandlerContext(
        VitalKind vital,
        bool magicMode,
        int currentPercent)
    {
        string stance = magicMode ? "magic" : "combat";
        string vitalName = vital.ToString().ToLowerInvariant();
        string band = vital == VitalKind.Health
            ? magicMode
                ? currentPercent <= 15 ? "low" : "normal"
                : currentPercent <= 10
                    ? "critical"
                    : currentPercent <= 15 ? "low" : "normal"
            : "normal";
        return $"{stance}-{vitalName}-{band}";
    }

    private static bool TryParseHandlerSet(
        string? source,
        string context,
        out VitalRechargeMethod[] handlers)
    {
        handlers = [];
        if (string.IsNullOrWhiteSpace(source)
            || source.Equals("RechargeHandlerSet", StringComparison.OrdinalIgnoreCase))
        {
            return false;
        }

        string selected = source;
        if (source.Contains('='))
        {
            selected = string.Empty;
            string fallbackContext = context.EndsWith(
                    "-critical",
                    StringComparison.Ordinal)
                || context.EndsWith("-low", StringComparison.Ordinal)
                    ? context[..context.LastIndexOf('-')] + "-normal"
                    : string.Empty;
            foreach (string segment in source.Split(
                ';',
                StringSplitOptions.RemoveEmptyEntries
                    | StringSplitOptions.TrimEntries))
            {
                int equals = segment.IndexOf('=');
                if (equals <= 0)
                    continue;
                string key = segment[..equals].Trim();
                if (key.Equals(context, StringComparison.OrdinalIgnoreCase))
                {
                    selected = segment[(equals + 1)..];
                    break;
                }
                if (selected.Length == 0
                    && fallbackContext.Length != 0
                    && key.Equals(
                        fallbackContext,
                        StringComparison.OrdinalIgnoreCase))
                {
                    selected = segment[(equals + 1)..];
                }
            }
        }
        if (string.IsNullOrWhiteSpace(selected))
            return false;

        var parsed = new List<VitalRechargeMethod>();
        foreach (string token in selected.Split(
            [',', '>', '|'],
            StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries))
        {
            string normalized = token.Replace(" ", string.Empty)
                .Replace("-", string.Empty)
                .ToLowerInvariant();
            VitalRechargeMethod? value = normalized switch
            {
                "regularspell" => VitalRechargeMethod.RegularSpell,
                "staminatohealth" => VitalRechargeMethod.StaminaToHealth,
                "manatohealth" => VitalRechargeMethod.ManaToHealth,
                "healthtostamina" => VitalRechargeMethod.HealthToStamina,
                "healthtomana" => VitalRechargeMethod.HealthToMana,
                "kit" or "kitrecharge" => VitalRechargeMethod.Kit,
                "food" or "rechargewithfood" => VitalRechargeMethod.Food,
                _ => null,
            };
            if (value is { } method)
                parsed.Add(method);
        }
        if (parsed.Count == 0)
            return false;
        handlers = [.. parsed];
        return true;
    }

    private static bool TryHandler(
        VitalRechargeMethod method,
        VitalKind vital,
        IAutomationSurface automation,
        VitalSettings settings,
        CombatSettings combatSettings,
        IReadOnlyList<PluginInventoryItem> items,
        out VitalRechargeChoice choice)
    {
        switch (method)
        {
            case VitalRechargeMethod.Kit:
                return TryKit(
                    vital,
                    automation,
                    settings,
                    combatSettings,
                    items,
                    out choice);
            case VitalRechargeMethod.Food:
                return TryFood(
                    vital,
                    automation,
                    combatSettings,
                    items,
                    out choice);
        }

        (string stem, VitalKind? sourceVital) = SpellStem(method, vital);
        if (stem.Length == 0
            || sourceVital is { } source
                && vital != VitalKind.Health
                && VitalPlan.IsBelowNormal(automation.Character, settings, source))
        {
            choice = default;
            return false;
        }
        if (!TrySpell(
            vital,
            stem,
            automation,
            combatSettings,
            items,
            out choice))
        {
            return false;
        }
        if (method is VitalRechargeMethod.StaminaToHealth
                or VitalRechargeMethod.ManaToHealth
            && !ConversionWorthwhile(
                method,
                choice.SpellId,
                automation,
                settings))
        {
            choice = default;
            return false;
        }
        return true;
    }

    private static (string Stem, VitalKind? SourceVital) SpellStem(
        VitalRechargeMethod method,
        VitalKind vital) => method switch
    {
        VitalRechargeMethod.RegularSpell => vital switch
        {
            VitalKind.Health => (VitalPlan.HealSelfStem, null),
            VitalKind.Stamina => (VitalPlan.RevitalizeStem, null),
            VitalKind.Mana => (VitalPlan.StaminaToManaStem, VitalKind.Stamina),
            _ => (string.Empty, null),
        },
        VitalRechargeMethod.StaminaToHealth =>
            ("Stamina to Health", VitalKind.Stamina),
        VitalRechargeMethod.ManaToHealth =>
            ("Mana to Health", VitalKind.Mana),
        VitalRechargeMethod.HealthToStamina =>
            ("Health to Stamina", VitalKind.Health),
        VitalRechargeMethod.HealthToMana =>
            ("Health to Mana", VitalKind.Health),
        _ => (string.Empty, null),
    };

    private static bool TrySpell(
        VitalKind vital,
        string stem,
        IAutomationSurface automation,
        CombatSettings settings,
        IReadOnlyList<PluginInventoryItem> items,
        out VitalRechargeChoice choice)
    {
        bool found = false;
        PluginSpellInfo bestSpell = default;
        uint bestItem = 0u;

        foreach (PluginSpellInfo spell in automation.Spells.KnownSelfBuffs)
        {
            if (!Matches(spell.Name, stem))
                continue;
            if (SpellComponentPolicy.UsesBlacklistedComponent(
                    automation.Spells,
                    spell,
                    settings.BlacklistedSpellComponents))
                continue;
            if (!CanCast(automation.Character, spell))
                continue;
            if (!found || Better(spell, 0u, bestSpell, bestItem))
            {
                found = true;
                bestSpell = spell;
                bestItem = 0u;
            }
        }

        foreach (PluginInventoryItem item in items)
        {
            if (!settings.CombatItemObjectIds.Contains(item.ObjectId)
                && !settings.CombatItemNames.Contains(item.Name))
            {
                continue;
            }
            if ((item.ItemType & CasterItemType) == 0u)
                continue;
            if (!ConfiguredSupplyReadiness.IsAssessed(
                    automation,
                    item.ObjectId))
            {
                continue;
            }

            foreach (uint spellId in ItemSpellIds(item))
            {
                if (!automation.Spells.TryGet(spellId, out PluginSpellInfo spell)
                    || !Matches(spell.Name, stem))
                {
                    continue;
                }
                if (SpellComponentPolicy.UsesBlacklistedComponent(
                        automation.Spells,
                        spell,
                        settings.BlacklistedSpellComponents))
                {
                    continue;
                }
                if (!found || Better(spell, item.ObjectId, bestSpell, bestItem))
                {
                    found = true;
                    bestSpell = spell;
                    bestItem = item.ObjectId;
                }
            }
        }

        if (!found)
        {
            choice = default;
            return false;
        }
        choice = new VitalRechargeChoice(
            vital,
            bestItem == 0u
                ? VitalRechargeSourceKind.LearnedSpell
                : VitalRechargeSourceKind.CasterItem,
            bestSpell.Name,
            bestSpell.SpellId,
            bestItem,
            PluginCombatMode.Magic);
        return true;
    }

    private static bool TryKit(
        VitalKind vital,
        IAutomationSurface automation,
        VitalSettings settings,
        CombatSettings combatSettings,
        IReadOnlyList<PluginInventoryItem> items,
        out VitalRechargeChoice choice)
    {
        PluginCombatMode mode = automation.Combat.Snapshot.Mode;
        if (mode == PluginCombatMode.Magic && !settings.UseKitsInMagicMode)
        {
            choice = default;
            return false;
        }
        if (vital != VitalKind.Stamina && automation.Character.CurrentStamina < 15u)
        {
            choice = default;
            return false;
        }
        if (!automation.Character.TryGetSkill(HealingSkill, out PluginSkillInfo healing)
            || healing.Training is not (PluginSkillTraining.Trained
                or PluginSkillTraining.Specialized))
        {
            choice = default;
            return false;
        }

        bool found = false;
        PluginInventoryItem best = default;
        double bestRestore = double.NegativeInfinity;
        foreach (PluginInventoryItem item in items)
        {
            if (!combatSettings.ConsumableNames.Contains(item.Name))
                continue;
            // The reference never appraises a kit: its kind comes from the
            // profile and its bonuses from the game-info table, by name. An
            // appraised kit is judged by its own properties; an unappraised
            // one by the profile's kind and the table, as the reference does.
            int skillBonus;
            double restoreBonus;
            if (ConfiguredSupplyReadiness.IsAssessed(automation, item.ObjectId))
            {
                if (item.BoosterVital != (int)vital
                    || (item.PublicFlags & HealingKitPublicFlag) == 0u
                    || item.UseRequiresSkillLevel > healing.Current
                    || item.UseRequiresSkillSpecialized != 0
                        && healing.Training != PluginSkillTraining.Specialized)
                {
                    continue;
                }
                skillBonus = item.BoostValue;
                restoreBonus = item.HealKitModifier;
            }
            else
            {
                if (!combatSettings.ConsumableCategories.TryGetValue(
                        item.Name,
                        out ConsumableCategory category)
                    || category != KitCategoryFor(vital))
                {
                    continue;
                }
                bool listed = combatSettings.HealKits.TryGetValue(
                    item.Name,
                    out VtankHealKit tableRow);
                skillBonus = listed ? tableRow.SkillBonus : 0;
                restoreBonus = listed ? tableRow.RestoreBonus : 1d;
            }
            if (HealKitChance(
                    healing.Current,
                    skillBonus,
                    automation.Character,
                    vital,
                    mode) * 100d < settings.MinimumHealKitSuccessChance)
            {
                continue;
            }
            if (!found
                || restoreBonus > bestRestore
                || restoreBonus == bestRestore
                    && item.Structure < best.Structure)
            {
                best = item;
                bestRestore = restoreBonus;
                found = true;
            }
        }
        if (!found)
        {
            choice = default;
            return false;
        }

        choice = new VitalRechargeChoice(
            vital,
            VitalRechargeSourceKind.Kit,
            best.Name,
            0u,
            best.ObjectId,
            settings.GoToPeaceModeToUseKits
                ? PluginCombatMode.Peace
                : null);
        return true;
    }

    private static ConsumableCategory KitCategoryFor(VitalKind vital) => vital switch
    {
        VitalKind.Stamina => ConsumableCategory.StaminaKit,
        VitalKind.Mana => ConsumableCategory.ManaKit,
        _ => ConsumableCategory.HealthKit,
    };

    private static ConsumableCategory FoodCategoryFor(VitalKind vital) => vital switch
    {
        VitalKind.Stamina => ConsumableCategory.StaminaFood,
        VitalKind.Mana => ConsumableCategory.ManaFood,
        _ => ConsumableCategory.HealthFood,
    };

    private static bool TryFood(
        VitalKind vital,
        IAutomationSurface automation,
        CombatSettings settings,
        IReadOnlyList<PluginInventoryItem> items,
        out VitalRechargeChoice choice)
    {
        foreach (PluginInventoryItem item in items)
        {
            if (!settings.ConsumableNames.Contains(item.Name))
                continue;
            // As with kits: an appraised item by its properties, an unappraised
            // one by the kind the profile gives it. The reference appraises
            // neither.
            bool eligible = ConfiguredSupplyReadiness.IsAssessed(automation, item.ObjectId)
                ? item.BoosterVital == (int)vital
                    && (item.PublicFlags & HealingKitPublicFlag) == 0u
                    && item.UseRequiresSkill != (int)HealingSkill
                : settings.ConsumableCategories.TryGetValue(
                        item.Name,
                        out ConsumableCategory category)
                    && category == FoodCategoryFor(vital);
            if (eligible)
            {
                choice = new VitalRechargeChoice(
                    vital,
                    VitalRechargeSourceKind.Food,
                    item.Name,
                    0u,
                    item.ObjectId,
                    null);
                return true;
            }
        }
        choice = default;
        return false;
    }

    internal static double HealKitChance(
        uint healingSkill,
        int skillBonus,
        ICharacterInfo character,
        VitalKind vital,
        PluginCombatMode mode)
    {
        (uint current, uint maximum) = vital switch
        {
            VitalKind.Health => (character.CurrentHealth, character.MaxHealth),
            VitalKind.Stamina => (character.CurrentStamina, character.MaxStamina),
            VitalKind.Mana => (character.CurrentMana, character.MaxMana),
            _ => (0u, 0u),
        };
        double multiplier = mode == PluginCombatMode.Peace ? 2d : 2.2d;
        double missing = Math.Max(0d, (double)maximum - current);
        double difficulty = Math.Ceiling(multiplier * missing);
        return 1d - 1d / (1d + Math.Exp(
            0.03d * (healingSkill + skillBonus - difficulty)));
    }

    private static bool ConversionWorthwhile(
        VitalRechargeMethod method,
        uint conversionSpellId,
        IAutomationSurface automation,
        VitalSettings settings)
    {
        if (!automation.Spells.TryGet(
                conversionSpellId,
                out PluginSpellInfo conversion))
        {
            return false;
        }

        int ordinaryHeal = 0;
        foreach (PluginSpellInfo spell in automation.Spells.KnownSelfBuffs)
        {
            if (!Matches(spell.Name, VitalPlan.HealSelfStem)
                || !CanCast(automation.Character, spell))
            {
                continue;
            }
            ordinaryHeal = Math.Max(ordinaryHeal, EstimatedOrdinaryHeal(spell.Name));
        }

        int missing = checked((int)Math.Max(
            0L,
            (long)automation.Character.MaxHealth
                - automation.Character.CurrentHealth));
        if (ordinaryHeal > missing)
            return false;

        VitalKind source = method == VitalRechargeMethod.StaminaToHealth
            ? VitalKind.Stamina
            : VitalKind.Mana;
        int sourceCurrent = source == VitalKind.Stamina
            ? checked((int)automation.Character.CurrentStamina)
            : checked((int)automation.Character.CurrentMana) - 30;
        sourceCurrent = Math.Max(0, sourceCurrent);
        int converted = Math.Min(
            missing,
            EstimatedTransfer(conversion.Name, sourceCurrent));
        double multiplier = source == VitalKind.Stamina
            ? settings.StaminaToHealthMultiplier
            : settings.ManaToHealthMultiplier;
        return ordinaryHeal * multiplier < missing
            && ordinaryHeal * multiplier < converted;
    }

    internal static int EstimatedOrdinaryHeal(string spellName) => spellName switch
    {
        "Heal Self I" => 17,
        "Heal Self II" => 25,
        "Heal Self III" => 32,
        "Heal Self IV" => 45,
        "Heal Self V" => 67,
        "Heal Self VI" => 87,
        "Adja's Intervention" => 115,
        "Incantation of Heal Self" => 135,
        _ => 10,
    };

    internal static int EstimatedTransfer(string spellName, int sourceCurrent)
    {
        (double multiplier, int cap) = spellName switch
        {
            _ when spellName.EndsWith(" I", StringComparison.Ordinal) =>
                (0.9, 50),
            _ when spellName.EndsWith(" II", StringComparison.Ordinal) =>
                (1.0, 100),
            _ when spellName.EndsWith(" III", StringComparison.Ordinal) =>
                (1.1, 150),
            _ when spellName.EndsWith(" IV", StringComparison.Ordinal) =>
                (1.2, 200),
            _ when spellName.EndsWith(" V", StringComparison.Ordinal) =>
                (1.35, int.MaxValue),
            _ when spellName.EndsWith(" VI", StringComparison.Ordinal) =>
                (1.5, int.MaxValue),
            _ => (1.75, int.MaxValue),
        };
        return Math.Min(
            cap,
            checked((int)Math.Floor(sourceCurrent * multiplier)));
    }

    /// <summary>
    /// A fellow's vitals are trusted only this long after the server last
    /// streamed them; older samples (or none at all) mean "unknown", never
    /// "still low".
    /// </summary>
    internal const double FellowVitalsTrustSeconds = 10d;

    private static PluginFellowMember? Lowest(
        IReadOnlyList<PluginFellowMember> members,
        VitalKind vital,
        double threshold,
        float maximumDistance)
    {
        PluginFellowMember? best = null;
        double bestFraction = double.PositiveInfinity;
        foreach (PluginFellowMember member in members)
        {
            if (member.VitalsAgeSeconds is not { } age
                || age >= FellowVitalsTrustSeconds
                || member.Distance > maximumDistance)
            {
                continue;
            }
            (uint current, uint maximum) = vital switch
            {
                VitalKind.Health => (member.CurrentHealth, member.MaxHealth),
                VitalKind.Stamina => (member.CurrentStamina, member.MaxStamina),
                VitalKind.Mana => (member.CurrentMana, member.MaxMana),
                _ => (0u, 0u),
            };
            if (maximum == 0u)
                continue;
            double fraction = (double)current / maximum;
            if (fraction < threshold && fraction < bestFraction)
            {
                best = member;
                bestFraction = fraction;
            }
        }
        return best;
    }

    /// <summary>
    /// The helper spell is the best castable tier of the reference's family
    /// that stays on the reference's own line — a family holds both the Self
    /// and the Other line, and only the component set tells them apart. Same
    /// accept as the buff walk, with the hunting skill margin.
    /// </summary>
    private static bool TryResolveHelperSpell(
        IAutomationSurface automation,
        in PluginSpellInfo reference,
        CombatSettings combatSettings,
        Action<string>? trace,
        out PluginSpellInfo spell)
    {
        var tiers = new List<PluginSpellInfo>();
        foreach (PluginSpellInfo candidate in automation.Spells.KnownSelfBuffs)
        {
            if (candidate.Family == reference.Family)
                tiers.Add(candidate);
        }
        tiers.Sort(static (a, b) =>
        {
            if (a.Tier != b.Tier)
                return b.Tier.CompareTo(a.Tier);
            if (a.Quality != b.Quality)
                return b.Quality.CompareTo(a.Quality);
            return a.SpellId.CompareTo(b.SpellId);
        });

        var skillLevels = new Dictionary<uint, uint>();
        foreach (PluginSkillInfo skill in automation.Character.Skills)
            skillLevels[skill.SkillId] = skill.Current;

        var castability = new BuffCastability(
            automation.Spells,
            automation.Magic,
            automation.Items.IsAvailable
                ? automation.Items.CaptureOwnedItems()
                : [],
            combatSettings.BlacklistedSpellComponents,
            text => trace?.Invoke(text),
            (walked, pick, rejections) =>
                trace?.Invoke(FormatHelperTrace(walked, pick, rejections)));
        var line = new BuffLine(
            reference.Family, BuffTargetKind.Other, reference.Name, tiers)
        {
            ReferenceOverride = reference,
        };
        return BuffPlan.TryPickTier(
            line,
            skillLevels,
            combatSettings.HuntSkillExcessOverDifficulty,
            castability,
            out spell);
    }

    /// <summary>
    /// The buff pass's tier-trace wording: the pick, then every rejected
    /// tier above it with its reason (all of them when nothing was picked).
    /// </summary>
    private static string FormatHelperTrace(
        BuffLine line,
        PluginSpellInfo? pick,
        IReadOnlyList<BuffTierRejection> rejections)
    {
        int floor = pick?.Tier ?? int.MinValue;
        var higher = new List<string>();
        foreach (BuffTierRejection rejection in rejections)
        {
            if (rejection.Spell.Tier > floor)
                higher.Add($"{rejection.Spell.Name} {rejection.Reason}");
        }
        string pickedText = pick is { } spell
            ? $"picked {spell.Name} (gen {spell.Tier})"
            : "picked nothing";
        string rejectedText = higher.Count == 0 ? "none" : string.Join(", ", higher);
        return $"Helping: {line.Reference.Name} \u2014 {pickedText}; rejected: {rejectedText}";
    }

    private static bool CanCast(ICharacterInfo character, PluginSpellInfo spell) =>
        spell.School == 0u
        || !character.TryGetSkill(spell.School, out PluginSkillInfo skill)
        || skill.Current >= spell.Difficulty;

    private static bool Better(
        PluginSpellInfo candidate,
        uint candidateItem,
        PluginSpellInfo current,
        uint currentItem)
    {
        int quality = candidate.Quality.CompareTo(current.Quality);
        if (quality != 0)
            return quality > 0;
        // dz.cs/m.cs: a direct learned spell wins the final source tie.
        if ((candidateItem == 0u) != (currentItem == 0u))
            return candidateItem == 0u;
        return candidateItem < currentItem;
    }

    private static bool Matches(string name, string stem)
    {
        if (name.IndexOf(stem, StringComparison.OrdinalIgnoreCase) >= 0)
            return true;
        return stem switch
        {
            VitalPlan.HealSelfStem => name.Equals(
                "Adja's Intervention",
                StringComparison.OrdinalIgnoreCase),
            VitalPlan.RevitalizeStem => name.Equals(
                "Robustification",
                StringComparison.OrdinalIgnoreCase),
            VitalPlan.StaminaToManaStem => name.Equals(
                "Meditative Trance",
                StringComparison.OrdinalIgnoreCase),
            _ => false,
        };
    }

    private static IEnumerable<uint> ItemSpellIds(PluginInventoryItem item)
    {
        if (item.SpellId != 0u)
            yield return item.SpellId;
        foreach (uint spellId in item.AppraisedSpellIds)
        {
            if (spellId != 0u && spellId != item.SpellId)
                yield return spellId;
        }
    }
}

/// <summary>One server-receipt-driven self-recharge state machine.</summary>
internal sealed class VitalRechargeController
{
    private CombatModeGate? _combatModeGate;

    internal void BindCombatModeGate(CombatModeGate gate) =>
        _combatModeGate = gate ?? throw new ArgumentNullException(nameof(gate));

    private readonly IPluginHost _host;
    private readonly VitalSettings _settings;
    private readonly CombatSettings _combatSettings;
    private Pending? _pending;

    /// <summary>
    /// True while a kit, a food item or a caster item this controller used
    /// is still unanswered. The reference's kit sequencer and wand cast
    /// tracker both raise the global busy count for that whole wait.
    /// </summary>
    internal bool ItemUseInFlight => _pending is { Choice.UsesItem: true };

    /// <summary>
    /// True while a recharge cast from a LEARNED SPELL is still unanswered.
    /// The reference raises the global busy count for the life of any cast
    /// it issues, so the pass runs no rule until it resolves.
    /// </summary>
    internal bool CastInFlight => _pending is { Choice.UsesItem: false };

    private double _retryDelay;
    private double _rechargeTraceDelay;
    private double _pendingSeconds;
    private double _healthBoostRemaining;
    private double _staminaBoostRemaining;
    private double _manaBoostRemaining;
    private bool _vitalsRequested;
    private readonly HashSet<string> _helperTraceLines = new(StringComparer.Ordinal);

    public VitalRechargeController(
        IPluginHost host,
        VitalSettings settings,
        CombatSettings combatSettings)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _settings = settings ?? throw new ArgumentNullException(nameof(settings));
        _combatSettings = combatSettings
            ?? throw new ArgumentNullException(nameof(combatSettings));
    }

    internal const string IdleStatus = "Vitals idle";

    private void TraceRecharge(string message)
    {
        _rechargeTraceDelay = 5d;
        _host.Log.Info(message);
    }

    public string Status { get; private set; } = IdleStatus;

    private ActionLockTable _actionLocks = new();

    /// <summary>
    /// Shares the macro's cooldown table. A kit, a stone or a bite of food
    /// holds the item slot for as long as the macro waits on it, so nothing
    /// else — the attack included — acts inside that window.
    /// </summary>
    internal void BindActionLocks(ActionLockTable locks) =>
        _actionLocks = locks ?? throw new ArgumentNullException(nameof(locks));

    private void ReleaseItemUse()
    {
        if (_pending is { } pending
            && pending.Choice.SourceKind != VitalRechargeSourceKind.LearnedSpell
            // Only while the window this owner armed is still running: past
            // it the slot may belong to somebody else, and releasing it then
            // would pull the floor out from under whoever holds it.
            && _actionLocks.Now < _pendingHoldUntil)
        {
            _actionLocks.Release(ActionLockKind.ItemUse);
        }
        _pendingHoldUntil = 0d;
    }

    /// <summary>
    /// When the item-slot window this owner armed for the outstanding use
    /// runs out. Past it the use is still watched to its end, but it no
    /// longer holds the character.
    /// </summary>
    private double _pendingHoldUntil;

    /// <summary>
    /// Whether the outstanding use still holds the pass. A cast does for as
    /// long as it is in flight; an item use does for the window it armed and
    /// no longer, because waiting out an answer that is not coming is this
    /// owner's business, not everybody else's.
    /// </summary>
    private bool PendingStillHoldsThePass =>
        _pending is { } waiting
        && (waiting.Choice.SourceKind == VitalRechargeSourceKind.LearnedSpell
            || _actionLocks.Now < _pendingHoldUntil);

    public bool Tick(
        double elapsedSeconds,
        bool enabled,
        bool noTarget,
        bool helpers = true)
    {
        IAutomationSurface automation = _host.Automation;
        double elapsed = Math.Max(0d, elapsedSeconds);
        // Whatever the frame driver already watched off this transaction is
        // not counted a second time here: one wall clock between the two.
        double pendingElapsed = Math.Max(0d, elapsed - _frameObservedSeconds);
        _frameObservedSeconds = 0d;
        _retryDelay = Math.Max(0d, _retryDelay - elapsed);
        _rechargeTraceDelay = Math.Max(0d, _rechargeTraceDelay - elapsed);
        _healthBoostRemaining = Math.Max(0d, _healthBoostRemaining - elapsed);
        _staminaBoostRemaining = Math.Max(0d, _staminaBoostRemaining - elapsed);
        _manaBoostRemaining = Math.Max(0d, _manaBoostRemaining - elapsed);
        if (!_settings.Enabled || !automation.IsAvailable)
        {
            ReleaseItemUse();
            _pending = null;
            _frameObservedSeconds = 0d;
            ClearBoosts();
            Status = IdleStatus;
            SyncVitalsRequest(automation, wanted: false);
            return false;
        }
        ObservePending(pendingElapsed);
        if (_pending is { } pending)
        {
            Status = $"Recharging {pending.Choice.Vital}: {pending.Choice.Name}";
            return PendingStillHoldsThePass;
        }

        // No turn this pass, so nothing new is begun.
        if (!enabled)
        {
            ClearBoosts();
            Status = IdleStatus;
            SyncVitalsRequest(automation, wanted: false);
            return false;
        }
        if (helpers)
        {
            SyncVitalsRequest(
                automation,
                wanted: _settings.HelpOthers && automation.Fellowship.IsInFellowship);
        }


        VitalKind? need = VitalPlan.DecideNeed(
            automation.Character,
            _settings,
            noTarget,
            _healthBoostRemaining > 0d ? _settings.RechargeBoostAmount : 0,
            _staminaBoostRemaining > 0d ? _settings.RechargeBoostAmount : 0,
            _manaBoostRemaining > 0d ? _settings.RechargeBoostAmount : 0);
        if (!helpers && need is null)
        {
            Status = "Vitals ready";
            return false;
        }
        VitalRechargeChoice helper = default;
        if (need is not null && !helpers)
        {
        }
        // The reference gates both helper rows on the item-use slot, the
        // same slot a kit or a heart holds while its animation runs.
        else if (need is null
            && (_actionLocks.IsLocked(ActionLockKind.ItemUse)
                || !VitalRechargePlanner.TryPlanHelper(
                    automation,
                    _settings,
                    _combatSettings,
                    TraceHelper,
                    out helper)))
        {
            Status = "Vitals ready";
            return false;
        }
        if (helpers && need is not null)
        {
            // Rows 11/12 (fb/gu) are the HELPER rules; a self need belongs to
            // row 4 and was already offered there this pass.
            Status = "Vitals ready";
            return false;
        }
        // The reference rule is valid whenever a vital is below its threshold,
        // whatever it then finds to use: a vital it has no answer for holds
        // the pass with a warning, not a yield. The retry delay only paces
        // how often a plan is looked for again.
        if (_retryDelay > 0d)
        {
            Status = "Waiting to retry";
            return true;
        }

        VitalRechargeChoice choice;
        if (need is null)
        {
            choice = helper;
        }
        else if (!VitalRechargePlanner.TryPlan(
                     need.Value,
                     automation,
                     _settings,
                     _combatSettings,
                     out choice,
                     _rechargeTraceDelay <= 0d ? TraceRecharge : null))
        {
            Status = $"No {need.Value} recharge available";
            _retryDelay = 1d;
            return true;
        }

        if (choice.RequiredMode == PluginCombatMode.Magic
            && _combatModeGate is { } preparation)
        {
            bool itemSpell = choice.SourceKind == VitalRechargeSourceKind.CasterItem;
            if (!preparation.TryPrepare(
                    PluginCombatMode.Magic,
                    overrideItemId: itemSpell ? choice.ItemObjectId : 0u,
                    autoSelect: !itemSpell))
            {
                ArmBoost(choice.Vital);
                Status = preparation.Status;
                return true;
            }
        }

        if (choice.RequiredMode is { } required
            && automation.Combat.Snapshot.Mode != required)
        {
            if (required == PluginCombatMode.Magic)
                ArmBoost(choice.Vital);
            PluginCombatCommandResult mode = automation.Combat.EnterMode(required);
            Status = mode.Accepted
                ? $"Switching to {required} for {choice.Name}"
                : $"Waiting for {required}: {choice.Name}";
            return true;
        }

        long revision = choice.SourceKind == VitalRechargeSourceKind.LearnedSpell
            ? automation.Magic.LastCompletion.Revision
            : automation.Items.LastCompletion.Revision;
        bool started = Start(automation, choice);
        if (!started)
        {
            Status = $"Waiting to use {choice.Name}";
            _retryDelay = 0.25d;
            return true;
        }
        _pending = new Pending(choice, revision);
        if (choice.UsesItem)
        {
            // An item is in the character's hands until the server answers
            // for it; the attack and every other item rule wait that out.
            _actionLocks.Arm(
                ActionLockKind.ItemUse,
                ItemUseLock.TransactionSeconds);
            _pendingHoldUntil =
                _actionLocks.Now + ItemUseLock.TransactionSeconds;
        }
        _pendingSeconds = 0d;
        Status = $"Recharging {choice.Vital}: {choice.Name}";
        _host.Log.Info(choice.TargetObjectId == 0u
            ? $"Vitals: {choice.Vital} \u2192 {choice.Name} on self"
            : $"Vitals: {choice.Vital} \u2192 {choice.Name} at fellow 0x{choice.TargetObjectId:X8}");
        return true;
    }

    public void Reset()
    {
        ReleaseItemUse();
        _pending = null;
        _pendingSeconds = 0d;
        _frameObservedSeconds = 0d;
        _retryDelay = 0d;
        _rechargeTraceDelay = 0d;
        ClearBoosts();
        // The host drops its subscription with the session; only the
        // plugin-side memory of it is stale here.
        _vitalsRequested = false;
        _helperTraceLines.Clear();
        Status = IdleStatus;
    }

    /// <summary>
    /// Each distinct helper walk line reaches the launch log once per session:
    /// the walk result and its missing-component warnings interleave, so a
    /// last-line memory would repeat both on every tick.
    /// </summary>
    private void TraceHelper(string line)
    {
        if (!_helperTraceLines.Add(line))
            return;
        _host.Log.Info(line);
    }

    private void SyncVitalsRequest(IAutomationSurface automation, bool wanted)
    {
        if (wanted == _vitalsRequested)
            return;
        if (!automation.IsAvailable)
        {
            _vitalsRequested = false;
            return;
        }
        if (automation.Fellowship.RequestVitals(wanted).Accepted)
            _vitalsRequested = wanted;
    }

    private void ArmBoost(VitalKind vital)
    {
        double duration = Math.Max(0d, _settings.RechargeBoostTimeSeconds);
        switch (vital)
        {
            case VitalKind.Health:
                _healthBoostRemaining = duration;
                break;
            case VitalKind.Stamina:
                _staminaBoostRemaining = duration;
                break;
            case VitalKind.Mana:
                _manaBoostRemaining = duration;
                break;
        }
    }

    private void ClearBoost(VitalKind vital)
    {
        switch (vital)
        {
            case VitalKind.Health:
                _healthBoostRemaining = 0d;
                break;
            case VitalKind.Stamina:
                _staminaBoostRemaining = 0d;
                break;
            case VitalKind.Mana:
                _manaBoostRemaining = 0d;
                break;
        }
    }

    private void ClearBoosts()
    {
        _healthBoostRemaining = 0d;
        _staminaBoostRemaining = 0d;
        _manaBoostRemaining = 0d;
    }

    private static bool IsLevelBoostSpell(in VitalRechargeChoice choice) =>
        choice.Vital switch
        {
            VitalKind.Health => choice.Name.Equals(
                "Adja's Intervention",
                StringComparison.OrdinalIgnoreCase),
            VitalKind.Stamina => choice.Name.Equals(
                "Robustification",
                StringComparison.OrdinalIgnoreCase),
            VitalKind.Mana => choice.Name.Equals(
                "Meditative Trance",
                StringComparison.OrdinalIgnoreCase),
            _ => false,
        };

    private static bool Start(
        IAutomationSurface automation,
        VitalRechargeChoice choice)
    {
        if (choice.SourceKind == VitalRechargeSourceKind.LearnedSpell)
        {
            PluginCastGate gate = choice.TargetObjectId == 0u
                ? automation.Magic.EvaluateGate(choice.SpellId)
                : automation.Magic.EvaluateGate(
                    choice.SpellId,
                    choice.TargetObjectId);
            return gate == PluginCastGate.Ready
                && (choice.TargetObjectId == 0u
                    ? automation.Magic.Cast(choice.SpellId)
                    : automation.Magic.Cast(
                        choice.SpellId,
                        choice.TargetObjectId));
        }

        PluginItemCommandResult result = choice.SourceKind switch
        {
            VitalRechargeSourceKind.Food =>
                automation.Items.Use(choice.ItemObjectId),
            _ => automation.Items.Apply(
                choice.ItemObjectId,
                choice.TargetObjectId == 0u
                    ? automation.Character.ObjectId
                    : choice.TargetObjectId),
        };
        return result.Accepted;
    }

    /// <summary>
    /// Seconds the frame driver has already watched off the transaction since
    /// the rule was last asked; the next turn subtracts them.
    /// </summary>
    private double _frameObservedSeconds;

    /// <summary>
    /// Reads the server's answer to a use or a cast this controller issued,
    /// on the host frame rather than on the macro pass. An unanswered use
    /// holds the pass, so the pass cannot be what ends the wait — it would be
    /// waiting on itself, and the hold could then only end on its watchdog,
    /// seconds after the server had already answered. Nothing is issued here:
    /// this only watches, and starting the next use stays with the turn.
    /// </summary>
    internal void ObservePendingReceipt(double elapsedSeconds)
    {
        if (_pending is null || !_host.Automation.IsAvailable)
            return;
        double elapsed = Math.Max(0d, elapsedSeconds);
        _frameObservedSeconds += elapsed;
        ObservePending(elapsed);
    }

    /// <summary>
    /// An item the server has yet to answer for is a transaction of its own:
    /// it holds the shared slot and it is watched to its end whoever owns the
    /// pass meanwhile, because a losing tick is not a reason to abandon it —
    /// and abandoning it is what would drop the slot early under another
    /// owner's feet.
    /// </summary>
    private void ObservePending(double elapsedSeconds)
    {
        if (_pending is not { } pending)
            return;
        _pendingSeconds += Math.Max(0d, elapsedSeconds);
        if (TryComplete(_host.Automation, pending))
        {
            if (_settings.ClearLevelBoostFlagOnCast
                && pending.Choice.SourceKind
                    == VitalRechargeSourceKind.LearnedSpell
                && IsLevelBoostSpell(pending.Choice))
            {
                ClearBoost(pending.Choice.Vital);
            }
            ReleaseItemUse();
            _pending = null;
            _pendingSeconds = 0d;
            _retryDelay = 0.25d;
            return;
        }
        if (_pendingSeconds < PendingTimeoutSeconds)
            return;
        Status = $"Timed out: {pending.Choice.Name}";
        ReleaseItemUse();
        _pending = null;
        _pendingSeconds = 0d;
        _retryDelay = 1d;
    }

    /// <summary>How long an unanswered use is waited out before it is given up.</summary>
    private const double PendingTimeoutSeconds = 15d;

    /// <summary>
    /// Whether the server has answered for THIS use or cast. Receipts are
    /// read on every frame now, beside every other owner of one, so a receipt
    /// alone says nothing: a door opened by the walk or an item used by the
    /// combat-mode gate raises the same stamp. A receipt that is not this
    /// one is stepped over -- its stamp is taken as the new floor, so it can
    /// never be read twice -- and the wait goes on.
    /// </summary>
    private static bool TryComplete(IAutomationSurface automation, Pending pending)
    {
        if (pending.Choice.SourceKind == VitalRechargeSourceKind.LearnedSpell)
        {
            PluginCastCompletion cast = automation.Magic.LastCompletion;
            if (cast.Revision <= pending.Revision)
                return false;
            pending.Revision = cast.Revision;
            return cast.SpellId == pending.Choice.SpellId;
        }

        PluginItemUseCompletion use = automation.Items.LastCompletion;
        if (use.Revision <= pending.Revision)
            return false;
        pending.Revision = use.Revision;
        return use.SourceObjectId == pending.Choice.ItemObjectId;
    }

    private sealed class Pending(VitalRechargeChoice choice, long revision)
    {
        public VitalRechargeChoice Choice { get; } = choice;

        /// <summary>
        /// The newest receipt stamp this wait has already looked at.
        /// </summary>
        public long Revision { get; set; } = revision;
    }
}

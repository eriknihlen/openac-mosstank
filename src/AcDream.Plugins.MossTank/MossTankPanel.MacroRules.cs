using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal sealed partial class MossTankPanel : IMacroRuleProvider
{
    private bool _prologueOwnsAction;

    internal IReadOnlyList<IMacroRule> MacroRules => _scheduler.MainRules;

    internal ActionLockTable ActionLocks => _actionLocks;

    /// <summary>
    /// The three cooldown slots that hold every navigation rule off: the one a
    /// kill or a portal arms, the one a shared-target request arms, and the one
    /// a door arms while it opens.
    /// </summary>
    private bool NavigationLocksAreClear() =>
        !_actionLocks.IsLocked(ActionLockKind.Navigation)
        && !_actionLocks.IsLocked(ActionLockKind.SpreadLockTargetRequested)
        && !_actionLocks.IsLocked(ActionLockKind.DoorOpening);

    private void StopMacroFromGate(string notice)
    {
        if (_buffRule.IsBursting)
            Stop(notice);
        _status = notice;
        SetMacroRunning(false);
    }

    IMacroRule IMacroRuleProvider.Create(MacroRuleSlot slot) => slot switch
    {
        MacroRuleSlot.SplitPeasCritical => new ControllerMacroRule(
            "SplitPeasCritical",
            context => _crafting.TickCritical(
                context.ElapsedSeconds,
                context.CanAct),
            gate: () => _combat.Enabled && !_buffRule.IsBursting),
        MacroRuleSlot.CraftFoodCritical => new AbsentMacroRule(
            "CraftFoodCritical",
            "fused into SplitPeasCritical — CraftingController.TickCritical "
                + "runs the pea split and the category craft in one call."),

        MacroRuleSlot.RechargeSelfNormal => new ControllerMacroRule(
            "RechargeSelfNormal",
            context => _vitalRecharge.Tick(
                context.ElapsedSeconds,
                context.CanAct,
                noTarget: !_combat.HasTarget,
                helpers: false),
            gate: () => _combat.Enabled),

        MacroRuleSlot.RefillWieldedMana => new ControllerMacroRule(
            "RefillWieldedMana",
            context => _itemManaRecharge.Tick(context.CanAct),
            gate: () => _combat.Enabled
                || _inventorySettings.ManaChargesWhenOff),

        MacroRuleSlot.BuffSelfNormal => new ControllerMacroRule(
            "BuffSelf",
            context => _buffRule.Tick(context, idle: false)),
        MacroRuleSlot.SplitPeasNormal => new AbsentMacroRule(
            "SplitPeasNormal",
            "fused into CraftFood — CraftingController.Tick runs the pea "
                + "split before the category craft."),

        MacroRuleSlot.DispelSelf => new ControllerMacroRule(
            "DispelSelf",
            context => _dispel.Tick(context.ElapsedSeconds, context.CanAct),
            gate: () => _combat.Enabled && !_buffRule.IsBursting),
        MacroRuleSlot.UseDispelItem => new AbsentMacroRule(
            "UseDispelItem",
            "fused into DispelSelf — DispelController.Tick tries the self "
                + "dispel, then a dispel item, then the ally drum."),
        MacroRuleSlot.DispelAllies => new AbsentMacroRule(
            "DispelAllies",
            "fused into DispelSelf — see UseDispelItem."),

        MacroRuleSlot.UseHealersHeart => new ControllerMacroRule(
            "UseHealersHeart",
            context => _vitalHelperRecharge.Tick(
                context.ElapsedSeconds,
                context.CanAct,
                noTarget: !_combat.HasTarget,
                helpers: true),
            gate: () => _combat.Enabled),
        MacroRuleSlot.RechargeOther => new AbsentMacroRule(
            "RechargeOther",
            "fused into UseHealersHeart - VitalRechargeController's helper "
                + "half tries the heart first and then gu's Adja's Gift / "
                + "Replenish / Gift of Essence chain, in that order."),

        MacroRuleSlot.CraftFood => new ControllerMacroRule(
            "CraftFood",
            context => _crafting.Tick(context.ElapsedSeconds, context.CanAct),
            gate: () => _combat.Enabled && !_buffRule.IsBursting),
        MacroRuleSlot.RefillPetChargesNormal => new AbsentMacroRule(
            "RefillPetChargesNormal",
            "fused into Attack — PetAutomation's refill branch runs inside "
                + "CombatController.OnTick."),

        MacroRuleSlot.FellowshipManager => new ControllerMacroRule(
            "FellowshipManager",
            context =>
            {
                _fellowshipManager.Tick(
                    context.ElapsedSeconds,
                    context.CanAct && _combat.Enabled
                        && AutoFellowManagementEnabled);
                return false;
            }),

        MacroRuleSlot.OpenDoor => new AbsentMacroRule(
            "OpenDoor",
            "fused into the Navigate rules — NavigationController.Tick runs "
                + "TickDoor itself."),

        MacroRuleSlot.ReadScrollPriority => new AbsentMacroRule(
            "ReadScrollPriority",
            "no priority tier: MossTank's inventory maintenance is idle-only "
                + "(the rule can run while a target is active)."),
        MacroRuleSlot.StackCramPriority => new AbsentMacroRule(
            "StackCramPriority",
            "fused into ReadScrollIdle."),
        MacroRuleSlot.SalvageItemsPriority => new AbsentMacroRule(
            "SalvageItemsPriority",
            "fused into ReadScrollIdle."),
        MacroRuleSlot.ReadScrollIdle => new ControllerMacroRule(
            "ReadScrollIdle",
            context => _inventoryMaintenance.Tick(
                context.ElapsedSeconds,
                context.CanAct),
            gate: () => _combat.Enabled && !_buffRule.IsBursting),
        MacroRuleSlot.StackCramIdle => new AbsentMacroRule(
            "StackCramIdle",
            "fused into ReadScrollIdle — InventoryMaintenanceController.Tick "
                + "covers scrolls, stack cramming and salvage in one call."),
        MacroRuleSlot.SalvageItemsIdle => new AbsentMacroRule(
            "SalvageItemsIdle",
            "fused into ReadScrollIdle."),

        MacroRuleSlot.NavigateCorpsePriority => new ControllerMacroRule(
            "LootCorpsePriority",
            context => _loot.Tick(context.ElapsedSeconds, context.CanAct),
            gate: () => _combat.Enabled && !_buffRule.IsBursting
                && _inventorySettings.Loot.PriorityBoost,
            bookkeepWhenBlocked: false),
        MacroRuleSlot.NavigateCorpseIdle => new ControllerMacroRule(
            "LootCorpseIdle",
            context => _loot.Tick(context.ElapsedSeconds, context.CanAct),
            gate: () => !_inventorySettings.Loot.PriorityBoost
                && _combat.Enabled && !_buffRule.IsBursting,
            bookkeepWhenBlocked: false),
        MacroRuleSlot.OpenCorpsePriority => new AbsentMacroRule(
            "OpenCorpsePriority",
            "fused into LootCorpsePriority."),
        MacroRuleSlot.LootCorpsePriority => new AbsentMacroRule(
            "LootCorpsePriority (loot step)",
            "fused into LootCorpsePriority."),
        MacroRuleSlot.CorpseWaitPriority => new AbsentMacroRule(
            "CorpseWaitPriority",
            "fused into LootCorpsePriority."),
        MacroRuleSlot.OpenCorpseIdle => new AbsentMacroRule(
            "OpenCorpseIdle",
            "fused into LootCorpseIdle."),
        MacroRuleSlot.LootCorpseIdle => new AbsentMacroRule(
            "LootCorpseIdle (loot step)",
            "fused into LootCorpseIdle."),
        MacroRuleSlot.CorpseWaitIdle => new AbsentMacroRule(
            "CorpseWaitIdle",
            "fused into LootCorpseIdle."),

        MacroRuleSlot.NavigateRoutePriority => new ControllerMacroRule(
            "NavigateRoutePriority",
            context => _navigation.Tick(
                context.ElapsedSeconds,
                context.CanAct),
            gate: () => _combat.Enabled && !_buffRule.IsBursting
                && _navigationSettings.Priority
                && NavigationLocksAreClear(),
            onLostTurn: _navigation.StopForLostTurn,
            bookkeepWhenBlocked: false),
        MacroRuleSlot.NavigateRouteIdle => new ControllerMacroRule(
            "NavigateRouteIdle",
            context => _navigation.Tick(
                context.ElapsedSeconds,
                context.CanAct),
            gate: () => !_navigationSettings.Priority
                && _combat.Enabled && !_buffRule.IsBursting
                && NavigationLocksAreClear(),
            onLostTurn: _navigation.StopForLostTurn,
            bookkeepWhenBlocked: false),

        MacroRuleSlot.Attack => new ControllerMacroRule(
            "Attack",
            TickCombatRule,
            // The attack's first refusal: an item that was just used owns the
            // character for the rest of its cooldown, and attacking inside
            // that window only eats the item's own animation.
            gate: () => !_actionLocks.IsLocked(ActionLockKind.ItemUse),
            onLostTurn: () => _combat.SetPaused(true)),

        // Rows 38-40.
        MacroRuleSlot.SplitPeasIdle => new AbsentMacroRule(
            "SplitPeasIdle",
            "fused into CraftFoodIdle — CraftingController.TickIdle runs the "
                + "pea split before the idle-count category craft."),
        MacroRuleSlot.CraftFoodIdle => new ControllerMacroRule(
            "CraftFoodIdle",
            context => _crafting.TickIdle(
                context.ElapsedSeconds,
                context.CanAct),
            gate: () => _combat.Enabled && !_buffRule.IsBursting),
        MacroRuleSlot.RefillPetChargesIdle => new AbsentMacroRule(
            "RefillPetChargesIdle",
            "fused into Attack — see RefillPetChargesNormal."),

        MacroRuleSlot.BuffSelfIdle => new ControllerMacroRule(
            "BuffSelfIdle",
            context => _buffRule.Tick(context, idle: true),
            gate: () => _buffSettings.IdleBuffTopoff,
            bookkeepWhenBlocked: false),

        // Walking to a monster is a navigation job that sits twenty positions
        // below the attack, not part of the attack: idle looting, idle buff
        // top-off and the route all outrank it.
        MacroRuleSlot.NavigateMonster => new ControllerMacroRule(
            "NavigateMonster",
            context => _combat.TickMonsterApproach(
                context.ElapsedSeconds,
                context.CanAct),
            gate: () => _combat.Enabled && !_buffRule.IsBursting
                && _navigationSettings.Enabled
                && NavigationLocksAreClear(),
            bookkeepWhenBlocked: false),

        MacroRuleSlot.RechargeSelfNoTarget => new AbsentMacroRule(
            "RechargeSelfNoTarget",
            "fused into RechargeSelfNormal — VitalPlan.Threshold merges the "
                + "Normal and NoTarget settings (KB 02 §5.5)."),

        MacroRuleSlot.RandomHelper => new AbsentMacroRule(
            "RandomHelper",
            "not run — the shipped rule cannot cast (its draw loop only wins "
                + "on an uncastable pick), so the position is held inert; the "
                + "RandomHelper settings still load and save."),

        MacroRuleSlot.IdlePeace => new MacroRulePreChain(
            _idlePeace,
            [() => _combat.Enabled]),

        _ => throw new ArgumentOutOfRangeException(nameof(slot), slot, null),
    };

    IMacroRule IMacroRuleProvider.Create(MacroIndependentSlot slot) => slot switch
    {
        MacroIndependentSlot.SummonPet => _summonPet,
        _ => throw new ArgumentOutOfRangeException(nameof(slot), slot, null),
    };

    private bool TickCombatRule(MacroPassContext context)
    {
        _combat.SetPaused(!context.CanAct);
        _combat.OnTick(context.ElapsedSeconds);
        return context.CanAct
            && (_combat.HasTarget || _combat.HasPendingItemDebuff);
    }
}

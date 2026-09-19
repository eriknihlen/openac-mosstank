using AcDream.Plugin.Abstractions;

namespace AcDream.Plugins.MossTank;

internal sealed partial class MossTankPanel : IMacroRuleProvider
{
    private bool _prologueOwnsAction;

    internal IReadOnlyList<IMacroRule> MacroRules => _scheduler.MainRules;

    internal ActionLockTable ActionLocks => _actionLocks;

    /// <summary>
    /// The cooldown slot an item in use holds. It is the first refusal of
    /// every rule that consumes an item — the attack included, because a swing
    /// inside an item's own animation only eats the item — and it is what
    /// makes the shared release safe: one owner at a time, so nobody can drop
    /// somebody else's window.
    /// </summary>
    private bool ItemSlotIsFree() =>
        !_actionLocks.IsLocked(ActionLockKind.ItemUse);

    /// <summary>
    /// The three cooldown slots that hold every navigation rule off: the one a
    /// kill or a portal arms, the one a shared-target request arms, and the one
    /// a door arms while it opens.
    /// </summary>
    private bool NavigationLocksAreClear() =>
        !_navigationWaitsOnCorpseId
        && !_actionLocks.IsLocked(ActionLockKind.Navigation)
        && !_actionLocks.IsLocked(ActionLockKind.SpreadLockTargetRequested)
        && !_actionLocks.IsLocked(ActionLockKind.DoorOpening);

    /// <summary>
    /// The reference's two per-pass latches: whether the corpse-id question
    /// has been asked this pass, and its answer — an undescribed corpse
    /// within the loot reach holds every walk off for the pass. Cleared at
    /// the top of each pass.
    /// </summary>
    private bool _corpseIdWaitDecided;
    private bool _navigationWaitsOnCorpseId;

    private void ClearPassLatches()
    {
        _corpseIdWaitDecided = false;
        _navigationWaitsOnCorpseId = false;
    }

    /// <summary>
    /// The reference's "set waiting on corpse id" gate. It never refuses the
    /// row it sits on; its job is the latch. Asked once per pass, by the
    /// first of the five rows whose earlier gates passed: with looting on,
    /// a corpse whose description has not arrived, within the approach
    /// range (five metres at least) plus ten, holds every walk off this
    /// pass.
    /// </summary>
    private bool WaitOnCorpseId()
    {
        if (_corpseIdWaitDecided)
            return true;
        _corpseIdWaitDecided = true;
        if (!_inventorySettings.Loot.Enabled)
            return true;
        double reach = Math.Max(
            LootController.CorpseOpenRangeMeters,
            _inventorySettings.Loot.CorpseApproachRange)
            + LootController.CorpseIdWaitMarginMeters;
        if (!_loot.HasCorpseAwaitingDescriptionWithin(reach))
            return true;
        _navigationWaitsOnCorpseId = true;
        EmitMacroLog(MacroLogChannel.Loot, "Waiting this tick on corpse ID.");
        return true;
    }

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
            gate: () => ItemSlotIsFree() && _combat.Enabled
               ),
        MacroRuleSlot.CraftFoodCritical => new AbsentMacroRule(
            "CraftFoodCritical",
            "fused into SplitPeasCritical — CraftingController.TickCritical "
                + "runs the pea split and the category craft in one call."),

        MacroRuleSlot.RechargeSelfNormal => new ControllerMacroRule(
            "RechargeSelfNormal",
            context => _vitalRecharge.Tick(
                context.ElapsedSeconds,
                context.CanAct,
                noTarget: false,
                helpers: false),
            gate: () => ItemSlotIsFree() && _combat.Enabled,
            runningDetail: () => _vitalRecharge.Status),

        MacroRuleSlot.RefillWieldedMana => new ControllerMacroRule(
            "RefillWieldedMana",
            context => _itemManaRecharge.Tick(context.CanAct, context.ElapsedSeconds),
            gate: () => ItemSlotIsFree()
                && (_combat.Enabled
                    || _inventorySettings.ManaChargesWhenOff)),

        MacroRuleSlot.BuffSelfNormal => new ControllerMacroRule(
            "BuffSelf",
            context => _buffRule.Tick(context, idle: false),
            gate: ItemSlotIsFree,
            declineReason: () => _buffRule.DeclineReason),
        MacroRuleSlot.SplitPeasNormal => new AbsentMacroRule(
            "SplitPeasNormal",
            "fused into CraftFood — CraftingController.Tick runs the pea "
                + "split before the category craft."),

        MacroRuleSlot.DispelSelf => new ControllerMacroRule(
            "DispelSelf",
            context => _dispel.Tick(context.ElapsedSeconds, context.CanAct),
            gate: () => ItemSlotIsFree() && _combat.Enabled
               ),
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
            gate: () => ItemSlotIsFree() && _combat.Enabled),
        MacroRuleSlot.RechargeOther => new AbsentMacroRule(
            "RechargeOther",
            "fused into UseHealersHeart - VitalRechargeController's helper "
                + "half tries the heart first and then gu's Adja's Gift / "
                + "Replenish / Gift of Essence chain, in that order."),

        MacroRuleSlot.CraftFood => new ControllerMacroRule(
            "CraftFood",
            context => _crafting.Tick(context.ElapsedSeconds, context.CanAct),
            gate: () => ItemSlotIsFree() && _combat.Enabled
               ),
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

        MacroRuleSlot.OpenDoor => new OpenDoorRule(
            _navigation,
            () => _combat.Enabled,
            // The door waits on four slots: the route's three and the item
            // slot, because opening is an item use.
            isLocked: () => !NavigationLocksAreClear()
                || _actionLocks.IsLocked(ActionLockKind.ItemUse)),

        MacroRuleSlot.ReadScrollPriority => new ControllerMacroRule(
            "ReadScrollPriority",
            context => _readScroll.Tick(
                context.ElapsedSeconds,
                context.CanAct),
            gate: () => ItemSlotIsFree() && _combat.Enabled
                && _inventorySettings.Loot.PriorityBoost),
        MacroRuleSlot.StackCramPriority => new ControllerMacroRule(
            "StackCramPriority",
            context => _inventoryMaintenance.Tick(
                context.ElapsedSeconds,
                context.CanAct),
            gate: () => ItemSlotIsFree() && _combat.Enabled
                && _inventorySettings.Loot.PriorityBoost),
        MacroRuleSlot.SalvageItemsPriority => new AbsentMacroRule(
            "SalvageItemsPriority",
            "fused into LootCorpsePriority — the loot controller runs the "
                + "salvage and salvage-combine steps itself whenever no corpse "
                + "is open, which is the same gate the separate rule uses."),
        MacroRuleSlot.ReadScrollIdle => new MacroRulePreChain(
            new ControllerMacroRule(
                "ReadScrollIdle",
                context => _readScroll.Tick(
                    context.ElapsedSeconds,
                    context.CanAct),
                gate: () => ItemSlotIsFree() && !_inventorySettings.Loot.PriorityBoost
                    && _combat.Enabled),
            fallbacks: [_idlePeace]),
        MacroRuleSlot.StackCramIdle => new MacroRulePreChain(
            new ControllerMacroRule(
                "StackCramIdle",
                context => _inventoryMaintenance.Tick(
                    context.ElapsedSeconds,
                    context.CanAct),
                gate: () => ItemSlotIsFree() && !_inventorySettings.Loot.PriorityBoost
                    && _combat.Enabled),
            fallbacks: [_idlePeace]),
        MacroRuleSlot.SalvageItemsIdle => new AbsentMacroRule(
            "SalvageItemsIdle",
            "fused into LootCorpseIdle — see SalvageItemsPriority."),

        // The walk to a corpse is its own rule and it outranks the open, so a
        // corpse out of arm's reach is walked to first and opened on arrival.
        // It reads the navigation switch, not just the looting one: the walk
        // is navigation, and a profile with navigation off does not get walked
        // anywhere by the looter either.
        MacroRuleSlot.NavigateCorpsePriority => new ControllerMacroRule(
            "NavigateCorpsePriority",
            context => _corpseApproach.ClaimFromRulePass(context.CanAct),
            gate: () => _combat.Enabled
                && _inventorySettings.Loot.Enabled
                && _inventorySettings.Loot.PriorityBoost
                && WaitOnCorpseId()
                && _navigationSettings.Enabled
                && NavigationLocksAreClear(),
            onLostTurn: _corpseApproach.StopForLostTurn,
            runningDetail: () => _corpseApproach.RunningDetail,
            declineReason: () => _corpseApproach.Status),
        MacroRuleSlot.NavigateCorpseIdle => new MacroRulePreChain(
            new ControllerMacroRule(
                "NavigateCorpseIdle",
                context => _corpseApproach.ClaimFromRulePass(context.CanAct),
                gate: () => _combat.Enabled
                    && _inventorySettings.Loot.Enabled
                    && !_inventorySettings.Loot.PriorityBoost
                    && WaitOnCorpseId()
                    && _navigationSettings.Enabled
                    && NavigationLocksAreClear(),
                onLostTurn: _corpseApproach.StopForLostTurn,
                runningDetail: () => _corpseApproach.RunningDetail,
                declineReason: () => _corpseApproach.Status),
            fallbacks:
            [
                new MacroRulePreChain(
                    _idlePeace,
                    [() => _corpseApproach.IsOutsideCreepDistance()]),
            ]),
        MacroRuleSlot.OpenCorpsePriority => new ControllerMacroRule(
            "OpenCorpsePriority",
            TickLootRule,
            // No item-slot gate: the reference's open rule is VALID while the
            // slot is held and holds the pass through the open and each pull.
            gate: () => _combat.Enabled
                && _inventorySettings.Loot.PriorityBoost
                && WaitOnCorpseId(),
            runningDetail: () => _loot.Status),
        MacroRuleSlot.OpenCorpseIdle => new MacroRulePreChain(
            new ControllerMacroRule(
                "OpenCorpseIdle",
                TickLootRule,
                gate: () => !_inventorySettings.Loot.PriorityBoost
                    && _combat.Enabled
                    && _inventorySettings.Loot.Enabled
                    && WaitOnCorpseId(),
                runningDetail: () => _loot.Status),
            fallbacks: [_idlePeace]),
        MacroRuleSlot.LootCorpsePriority => new AbsentMacroRule(
            "LootCorpsePriority (loot step)",
            "fused into OpenCorpsePriority — the loot controller runs select, "
                + "open, pick up and close as one state machine, and the three "
                + "rules it replaces sit next to each other in the list, so "
                + "nothing else can win a pass between them."),
        MacroRuleSlot.CorpseWaitPriority => new AbsentMacroRule(
            "CorpseWaitPriority",
            "fused into OpenCorpsePriority — the controller issues the "
                + "closing use itself once the corpse is fully processed."),
        MacroRuleSlot.LootCorpseIdle => new AbsentMacroRule(
            "LootCorpseIdle (loot step)",
            "fused into OpenCorpseIdle — see LootCorpsePriority."),
        MacroRuleSlot.CorpseWaitIdle => new AbsentMacroRule(
            "CorpseWaitIdle",
            "fused into OpenCorpseIdle — see CorpseWaitPriority."),

        MacroRuleSlot.NavigateRoutePriority => new ControllerMacroRule(
            "NavigateRoutePriority",
            context => _navigation.ClaimFromRulePass(context.CanAct),
            gate: () => _combat.Enabled
                && _navigationSettings.Priority
                && WaitOnCorpseId()
                && NavigationLocksAreClear(),
            onLostTurn: _navigation.StopForLostTurn,
            runningDetail: () => _navigation.RunningDetail,
            declineReason: () => _navigation.Status),
        MacroRuleSlot.NavigateRouteIdle => new MacroRulePreChain(
            new ControllerMacroRule(
                "NavigateRouteIdle",
                context => _navigation.ClaimFromRulePass(context.CanAct),
                gate: () => !_navigationSettings.Priority
                    && _combat.Enabled
                    && NavigationLocksAreClear(),
                onLostTurn: _navigation.StopForLostTurn,
                runningDetail: () => _navigation.RunningDetail,
                declineReason: () => _navigation.Status),
            // Idle peace while walking the route; inside the creep band and
            // at a recall the walk itself runs and pushes into magic mode.
            fallbacks:
            [
                new MacroRulePreChain(
                    _idlePeace,
                    [() => _navigation.AllowsNormalMovement()]),
            ]),

        MacroRuleSlot.Attack => new ControllerMacroRule(
            "Attack",
            TickCombatRule,
            // The attack's first refusal: an item that was just used owns the
            // character for the rest of its cooldown, and attacking inside
            // that window only eats the item's own animation.
            gate: ItemSlotIsFree,
            onLostTurn: () => _combat.SetPaused(true),
            declineReason: () => _combat.Status),

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
            gate: () => ItemSlotIsFree() && _combat.Enabled
               ),
        // The idle band's own refill, with its own (higher, so more eager)
        // charge threshold: down here nothing is being fought, so a device
        // gets topped up long before the attack band would bother. It asks
        // nothing about monsters, which is why it can stand this far from
        // the attack; the Normal threshold still governs the refill that
        // runs inside the attack itself.
        MacroRuleSlot.RefillPetChargesIdle => new ControllerMacroRule(
            "RefillPetChargesIdle",
            _idlePetRefill.Tick,
            gate: () => ItemSlotIsFree() && _combat.Enabled
                && _combatSettings.Enabled,
            runningDetail: () => _idlePetRefill.Status),

        MacroRuleSlot.BuffSelfIdle => new ControllerMacroRule(
            "BuffSelfIdle",
            context => _buffRule.Tick(context, idle: true),
            gate: () => ItemSlotIsFree() && _buffSettings.IdleBuffTopoff,
            declineReason: () => _buffRule.DeclineReason),

        // Walking to a monster is a navigation job that sits twenty positions
        // below the attack, not part of the attack: idle looting, idle buff
        // top-off and the route all outrank it.
        MacroRuleSlot.NavigateMonster => new MacroRulePreChain(
            new ControllerMacroRule(
                "NavigateMonster",
                context => _combat.TickMonsterApproach(
                    context.ElapsedSeconds,
                    context.CanAct),
                gate: () => _combat.Enabled
                    && _navigationSettings.Enabled
                    && NavigationLocksAreClear(),
                onLostTurn: _combat.StopMonsterApproachForLostTurn),
            fallbacks:
            [
                new MacroRulePreChain(
                    _idlePeace,
                    [() => _combat.IsApproachOutsideCreepDistance()]),
            ]),

        // Row 59: the idle recharge, below loot, the buff top-off and the
        // monster approach, against the no-target thresholds. Row 4 above
        // reads the normal thresholds alone.
        MacroRuleSlot.RechargeSelfNoTarget => new ControllerMacroRule(
            "RechargeSelfNoTarget",
            context => _vitalRecharge.Tick(
                context.ElapsedSeconds,
                context.CanAct,
                noTarget: true,
                helpers: false),
            gate: () => ItemSlotIsFree() && _combat.Enabled,
            runningDetail: () => _vitalRecharge.Status),

        MacroRuleSlot.RandomHelper => new MacroRulePreChain(
            _randomHelper,
            [() => _combat.Enabled]),

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

    /// <summary>
    /// The attack's turn. Being asked at all means the rule may win, so the
    /// pause the last lost turn put up comes down first; losing the turn is
    /// the rule's <c>onLostTurn</c>, which puts it back up.
    /// </summary>
    private bool TickCombatRule(MacroPassContext context)
    {
        _combat.SetPaused(false);
        _combat.OnTick(context.ElapsedSeconds);
        return _combat.HasTarget || _combat.HasPendingItemDebuff;
    }

    private bool TickLootRule(MacroPassContext context)
    {
        bool claimed = _loot.Tick(context.ElapsedSeconds, context.CanAct);
        return claimed
            || (context.CanAct && _loot.HasEligibleCorpseInOpenRange());
    }
}

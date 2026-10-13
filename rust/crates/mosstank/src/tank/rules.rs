//! The macro's rules (`MossTankPanel.MacroRules`): the call's state the
//! scheduler and its rules work with ([`TankCx`]), and the rule behind each
//! slot of VTank's list, with the C#'s gates, fallbacks and edges.
//!
//! A slot whose controller is not converted yet holds an absent rule that
//! never wins (crafting, the stack-and-cram pass, the pet refills, the
//! random helper buffs and the pet summoner): the macro runs without that
//! job until its slice lands.

use std::cell::RefCell;
use std::rc::Rc;

use mosstank_automation::locks::ActionLockKind;
use mosstank_buffs::buff_rule::BuffTurn;
use mosstank_buffs::mana::ManaTurn;
use mosstank_buffs::recharge_controller::VitalTurn;
use mosstank_combat::controller::Ctx;
use mosstank_combat::dispel::DispelTurn;
use mosstank_combat::mover::CreepSettings;
use mosstank_loot::controller::LootController;
use mosstank_loot::corpse_approach::ApproachTurn;
use mosstank_loot::corpse_scan::{CORPSE_ID_WAIT_MARGIN_METERS, CORPSE_OPEN_RANGE_METERS};
use mosstank_meta::engine::MetaEngine;
use mosstank_meta::host::MetaHost;
use mosstank_meta::scheduler::{
    AbsentMacroRule, ControllerMacroRule, ControllerTick, IdlePeaceContext, IdlePeaceRule,
    MacroContext, MacroFamily, MacroGate, MacroHook, MacroIndependentSlot, MacroLogChannel,
    MacroPassContext, MacroRule, MacroRulePreChain, MacroRuleProvider, MacroRuleSlot,
    MacroScheduler, MacroText, build,
};
use mosstank_nav::OpenDoorRule;
use mosstank_nav::RouteTurn;
use mosstank_nav::door_rule::DoorRuleHost;
use mosstank_settings::model::LootSettings;

use super::Core;
use crate::client::TankClient;

/// The call's state: the client, the shell's parts and the meta engine,
/// all for the length of one call.
#[allow(missing_debug_implementations)]
pub struct TankCx<'a> {
    /// The client.
    pub client: &'a mut dyn TankClient,
    /// The shell's parts.
    pub core: &'a mut Core,
    /// The meta engine.
    pub meta: &'a mut MetaEngine,
    /// The UB tab.
    pub ub: &'a mut super::ub::UbParts,
}

/// The family of [`TankCx`] states.
#[derive(Debug, Clone, Copy)]
pub struct TankFamily;

impl MacroFamily for TankFamily {
    type Cx<'a> = TankCx<'a>;
}

/// The looter with the profile's loot settings lent to it for one call: the
/// C#'s panel and looter share one settings object, the Rust looter keeps
/// its own, so the panel's is moved in for the call and back out after.
pub(crate) struct LentLoot<'a> {
    pub(crate) loot: &'a mut LootController,
    home: &'a mut LootSettings,
}

impl<'a> LentLoot<'a> {
    pub(crate) fn new(loot: &'a mut LootController, home: &'a mut LootSettings) -> Self {
        std::mem::swap(&mut loot.settings, home);
        Self { loot, home }
    }
}

impl Drop for LentLoot<'_> {
    fn drop(&mut self) {
        std::mem::swap(&mut self.loot.settings, self.home);
    }
}

/// The looter, lent its settings, from a [`Core`].
macro_rules! lent_loot {
    ($core:expr) => {
        $crate::tank::rules::LentLoot::new(&mut $core.loot, &mut $core.settings.inventory.loot)
    };
}
pub(crate) use lent_loot;

/// The route's turn over a call's state.
macro_rules! route_turn {
    ($client:expr, $core:expr) => {
        mosstank_nav::RouteTurn {
            automation: $client.route(),
            settings: &mut $core.settings.navigation,
            creep: Some(mosstank_combat::mover::CreepSettings {
                combat: &$core.settings.combat,
                vitals: &$core.settings.vitals,
            }),
        }
    };
}
pub(crate) use route_turn;

/// The fight's turn over a call's state.
macro_rules! combat_turn {
    ($client:expr, $core:expr) => {
        mosstank_combat::controller::CombatTurn {
            automation: $client.rules(),
            settings: &mut $core.settings.combat,
            vitals: &$core.settings.vitals,
            crafting: None,
        }
    };
}
pub(crate) use combat_turn;

/// The fight's read-only view over a call's state.
macro_rules! combat_ctx {
    ($client:expr, $core:expr) => {
        mosstank_combat::controller::Ctx {
            automation: $client.rules(),
            settings: &$core.settings.combat,
            vitals: &$core.settings.vitals,
            crafting: None,
        }
    };
}
pub(crate) use combat_ctx;

/// The buff rule's owner over a call's state.
macro_rules! buff_owner {
    ($core:expr) => {
        $crate::tank::BuffOwner::new(
            $core.combat.enabled(),
            $core.settings.buffs.fast_cast_buffs,
            &mut $core.status,
            &$core.lines,
            &mut $core.selection_before_pass,
            &mut $core.fast_cast,
        )
    };
}
pub(crate) use buff_owner;

impl TankCx<'_> {
    /// The cooldown slot an item in use holds is free (`ItemSlotIsFree`).
    pub(crate) fn item_slot_is_free(&mut self) -> bool {
        !self
            .core
            .locks
            .borrow_mut()
            .is_locked(ActionLockKind::ItemUse)
    }

    /// The three slots that hold every navigation rule off, with the
    /// per-pass corpse-id latch (`NavigationLocksAreClear`).
    pub(crate) fn navigation_locks_are_clear(&mut self) -> bool {
        !self.core.navigation_waits_on_corpse_id && self.core.navigation_slots_are_clear()
    }

    /// The reference's "set waiting on corpse id" gate: never refuses, its
    /// job is the latch (`WaitOnCorpseId`).
    pub(crate) fn wait_on_corpse_id(&mut self) -> bool {
        if self.core.corpse_id_wait_decided {
            return true;
        }
        self.core.corpse_id_wait_decided = true;
        if !self.core.settings.inventory.loot.enabled {
            return true;
        }
        let reach = CORPSE_OPEN_RANGE_METERS
            .max(self.core.settings.inventory.loot.corpse_approach_range)
            + CORPSE_ID_WAIT_MARGIN_METERS;
        let waiting = {
            let lent = lent_loot!(self.core);
            lent.loot
                .has_corpse_awaiting_description_within(self.client.looter(), reach)
        };
        if !waiting {
            return true;
        }
        self.core.navigation_waits_on_corpse_id = true;
        self.core.queue_log(
            MacroLogChannel::Loot,
            "Holding this tick until the corpse id arrives.",
        );
        true
    }

    /// MossTank's own hold of the route for this character's rare corpse
    /// (`RareCorpseHoldsRoute`).
    pub(crate) fn rare_corpse_holds_route(&mut self) -> bool {
        let range = self.core.settings.inventory.loot.corpse_approach_range;
        let holds = self.core.settings.inventory.loot.enabled && {
            let lent = lent_loot!(self.core);
            lent.loot
                .has_own_rare_corpse_to_fetch(self.client.looter(), range)
        };
        if holds && !self.core.route_held_for_rare {
            self.core.queue_log(
                MacroLogChannel::Loot,
                "Holding the route until this character's rare corpse is looted.",
            );
        }
        self.core.route_held_for_rare = holds;
        holds
    }

    /// The attack's turn (`TickCombatRule`): the pause the last lost turn
    /// put up comes down first.
    fn tick_combat_rule(&mut self, context: MacroPassContext) -> bool {
        let core = &mut *self.core;
        core.sync_route_after_kill();
        core.combat.set_paused(self.client.rules(), false);
        let mut turn = combat_turn!(self.client, core);
        core.combat.on_tick(&mut turn, context.elapsed_seconds);
        core.forward_cast_outcomes();
        core.combat.has_target() || core.combat.has_pending_item_debuff()
    }

    /// The looter's turn (`TickLootRule`).
    fn tick_loot_rule(&mut self, context: MacroPassContext) -> bool {
        let lent = lent_loot!(self.core);
        let claimed = lent.loot.tick(
            self.client.looter(),
            context.elapsed_seconds,
            context.can_act,
        );
        claimed
            || (context.can_act
                && lent
                    .loot
                    .has_eligible_corpse_in_open_range(self.client.looter()))
    }

    /// The buff rule's turn, then the stop it asked for, if any.
    fn tick_buff_rule(&mut self, context: MacroPassContext, idle: bool) -> bool {
        let core = &mut *self.core;
        let mut owner = buff_owner!(core);
        let claimed = {
            let mut turn = BuffTurn {
                automation: self.client.rules(),
                settings: &core.settings.buffs,
                combat: &core.settings.combat,
                vitals: &core.settings.vitals,
                host: &mut owner,
            };
            core.buff_rule.tick(&mut turn, context.can_act, idle)
        };
        let stop = owner.stop_request.take();
        if let Some(status) = stop {
            core.stop(self.client, &status);
        }
        claimed
    }

    /// The self or helper recharge's turn.
    fn tick_recharge(&mut self, context: MacroPassContext, no_target: bool, helpers: bool) -> bool {
        let core = &mut *self.core;
        let mut turn = VitalTurn {
            automation: self.client.rules(),
            settings: &core.settings.vitals,
            combat: &core.settings.combat,
        };
        let controller = if helpers {
            &mut core.vital_helper_recharge
        } else {
            &mut core.vital_recharge
        };
        controller.tick(
            &mut turn,
            context.elapsed_seconds,
            context.can_act,
            no_target,
            helpers,
        )
    }

    /// The worn-gear refill's turn.
    fn tick_item_mana(&mut self, context: MacroPassContext) -> bool {
        let core = &mut *self.core;
        let mut turn = ManaTurn {
            automation: self.client.rules(),
            settings: &core.settings.inventory,
            profiles: &core.settings.combat,
        };
        core.item_mana
            .tick(&mut turn, context.can_act, context.elapsed_seconds)
    }

    /// The dispel's turn.
    fn tick_dispel(&mut self, context: MacroPassContext) -> bool {
        let core = &mut *self.core;
        let mut turn = DispelTurn {
            automation: self.client.rules(),
            settings: &core.settings.vitals,
            combat: &core.settings.combat,
        };
        core.dispel
            .tick(&mut turn, context.elapsed_seconds, context.can_act)
    }

    /// The macro's stack-and-cram turn.
    fn tick_stack_cram(&mut self, context: MacroPassContext) -> bool {
        let core = &mut *self.core;
        core.inventory_maintenance.tick(
            self.client.ub(),
            &core.settings.inventory,
            context.elapsed_seconds,
            context.can_act,
        )
    }

    /// The scroll reader's turn.
    fn tick_read_scroll(&mut self, context: MacroPassContext) -> bool {
        let core = &mut *self.core;
        let lent = LentLoot::new(&mut core.loot, &mut core.settings.inventory.loot);
        core.read_scroll.tick(
            self.client.looter(),
            &mut *lent.loot,
            context.elapsed_seconds,
            context.can_act,
        )
    }

    /// The corpse walk's turn.
    fn claim_corpse_walk(&mut self, context: MacroPassContext) -> bool {
        let core = &mut *self.core;
        let lent = LentLoot::new(&mut core.loot, &mut core.settings.inventory.loot);
        let mut turn = ApproachTurn {
            host: self.client.looter(),
            loot: &mut *lent.loot,
            creep: Some(CreepSettings {
                combat: &core.settings.combat,
                vitals: &core.settings.vitals,
            }),
        };
        core.corpse_approach
            .claim_from_rule_pass(&mut turn, context.can_act)
    }

    /// Whether the corpse walk is outside its creep band.
    fn corpse_walk_outside_creep_distance(&mut self) -> bool {
        let core = &mut *self.core;
        let lent = LentLoot::new(&mut core.loot, &mut core.settings.inventory.loot);
        let mut turn = ApproachTurn {
            host: self.client.looter(),
            loot: &mut *lent.loot,
            creep: Some(CreepSettings {
                combat: &core.settings.combat,
                vitals: &core.settings.vitals,
            }),
        };
        core.corpse_approach.is_outside_creep_distance(&mut turn)
    }

    /// The route's turn.
    fn claim_route(&mut self, context: MacroPassContext) -> bool {
        let core = &mut *self.core;
        let mut turn: RouteTurn<'_> = route_turn!(self.client, core);
        core.navigation
            .claim_from_rule_pass(&mut turn, context.can_act)
    }
}

impl Core {
    /// The three navigation slots alone (`NavigationSlotsAreClear`).
    pub(crate) fn navigation_slots_are_clear(&self) -> bool {
        let mut locks = self.locks.borrow_mut();
        !locks.is_locked(ActionLockKind::Navigation)
            && !locks.is_locked(ActionLockKind::SpreadLockTargetRequested)
            && !locks.is_locked(ActionLockKind::DoorOpening)
    }

    /// The per-pass latches clear (`ClearPassLatches`).
    pub(crate) fn clear_pass_latches(&mut self) {
        self.corpse_id_wait_decided = false;
        self.navigation_waits_on_corpse_id = false;
    }
}

impl MacroContext for TankCx<'_> {
    fn has_meta_pass(&self) -> bool {
        true
    }

    /// The meta is looked at only while the macro runs, and waits for the
    /// other clients its functions need (`MetaPeersReady`).
    fn meta_pass(&mut self, elapsed_seconds: f64) {
        if !self.core.combat.enabled() || !self.core.meta_peers_ready(self.client) {
            return;
        }
        self.with_engine(|engine, host, services| engine.on_tick(host, services, elapsed_seconds));
        self.core.write_out(self.client);
    }

    fn pass_starting(&mut self) {
        self.core.clear_pass_latches();
    }

    fn has_log(&self) -> bool {
        true
    }

    fn log(&mut self, channel: MacroLogChannel, message: &str) {
        self.core
            .emit_macro_log(self.client, channel, message, true);
    }

    /// The three columns are cooldown-slot states.
    fn lock_state_suffix(&mut self) -> String {
        let mut locks = self.core.locks.borrow_mut();
        format!(
            "   I={}, N={}, S={}",
            pascal(locks.is_locked(ActionLockKind::ItemUse)),
            pascal(locks.is_locked(ActionLockKind::Navigation)),
            pascal(locks.is_locked(ActionLockKind::Salvage))
        )
    }
}

/// .NET's `bool.ToString()`.
fn pascal(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

impl IdlePeaceContext for TankCx<'_> {
    fn meta_host(&mut self) -> &mut dyn MetaHost {
        self.client.meta()
    }

    fn idle_peace_mode(&mut self) -> bool {
        self.core.settings.combat.idle_peace_mode
    }
}

impl DoorRuleHost for TankCx<'_> {
    fn door_rule_enabled(&mut self) -> bool {
        self.core.combat.enabled()
    }

    /// The door waits on four slots: the route's three and the item slot,
    /// because opening is an item use.
    fn door_rule_locked(&mut self) -> bool {
        !self.navigation_locks_are_clear()
            || self
                .core
                .locks
                .borrow_mut()
                .is_locked(ActionLockKind::ItemUse)
    }

    fn tick_door_rule(&mut self, elapsed_seconds: f64, can_act: bool) -> bool {
        let core = &mut *self.core;
        let mut turn: RouteTurn<'_> = route_turn!(self.client, core);
        core.navigation
            .tick_door_rule(&mut turn, elapsed_seconds, can_act)
    }

    fn door_status(&mut self) -> String {
        self.core.navigation.door_status().to_owned()
    }
}

/// The peace-when-idle rule the C# shares between several slots: one rule,
/// whose running flag and status every slot sees.
pub(crate) type SharedIdlePeace = Rc<RefCell<IdlePeaceRule>>;

/// A slot's view of the shared peace-when-idle rule.
struct IdlePeaceSlot(SharedIdlePeace);

impl MacroRule<TankFamily> for IdlePeaceSlot {
    fn name(&self) -> String {
        MacroRule::<TankFamily>::name(&*self.0.borrow())
    }

    fn shape(&self) -> mosstank_meta::scheduler::RuleShape {
        MacroRule::<TankFamily>::shape(&*self.0.borrow())
    }

    fn valid_now(&mut self, cx: &mut TankCx<'_>, context: MacroPassContext) -> bool {
        MacroRule::<TankFamily>::valid_now(&mut *self.0.borrow_mut(), cx, context)
    }

    fn running(&self) -> bool {
        MacroRule::<TankFamily>::running(&*self.0.borrow())
    }

    fn set_running(&mut self, cx: &mut TankCx<'_>, running: bool) {
        MacroRule::<TankFamily>::set_running(&mut *self.0.borrow_mut(), cx, running);
    }
}

/// A rule that writes out the lines its controller queued, and runs the
/// gate's stops, right after each of its calls.
struct WritingOut(Box<dyn MacroRule<TankFamily>>);

impl MacroRule<TankFamily> for WritingOut {
    fn name(&self) -> String {
        self.0.name()
    }

    fn shape(&self) -> mosstank_meta::scheduler::RuleShape {
        self.0.shape()
    }

    fn valid_now(&mut self, cx: &mut TankCx<'_>, context: MacroPassContext) -> bool {
        let valid = self.0.valid_now(cx, context);
        cx.core.write_out(cx.client);
        valid
    }

    fn running(&self) -> bool {
        self.0.running()
    }

    fn set_running(&mut self, cx: &mut TankCx<'_>, running: bool) {
        self.0.set_running(cx, running);
        cx.core.write_out(cx.client);
    }

    fn running_detail(&mut self, cx: &mut TankCx<'_>) -> Option<String> {
        self.0.running_detail(cx)
    }

    fn decline_reason(&mut self, cx: &mut TankCx<'_>) -> Option<String> {
        self.0.decline_reason(cx)
    }
}

fn tick(
    tick: impl for<'a, 'b> FnMut(&'b mut TankCx<'a>, MacroPassContext) -> bool + 'static,
) -> ControllerTick<TankFamily> {
    Box::new(tick)
}

fn gate(
    gate: impl for<'a, 'b> FnMut(&'b mut TankCx<'a>) -> bool + 'static,
) -> MacroGate<TankFamily> {
    Box::new(gate)
}

fn hook(hook: impl for<'a, 'b> FnMut(&'b mut TankCx<'a>) + 'static) -> MacroHook<TankFamily> {
    Box::new(hook)
}

fn text(
    text: impl for<'a, 'b> FnMut(&'b mut TankCx<'a>) -> Option<String> + 'static,
) -> MacroText<TankFamily> {
    Box::new(text)
}

fn absent(name: &str, reason: &str) -> Box<dyn MacroRule<TankFamily>> {
    Box::new(AbsentMacroRule::new(name, reason))
}

/// Not converted yet: the slot never wins until its controller lands.
const NOT_CONVERTED: &str = "not converted to the Rust plugin yet";

/// The rule behind each slot (`IMacroRuleProvider`).
pub(crate) struct Provider {
    pub(crate) idle_peace: SharedIdlePeace,
}

impl Provider {
    fn idle_peace(&self) -> Box<dyn MacroRule<TankFamily>> {
        Box::new(IdlePeaceSlot(self.idle_peace.clone()))
    }

    #[allow(clippy::too_many_lines)]
    fn rule(&self, slot: MacroRuleSlot) -> Box<dyn MacroRule<TankFamily>> {
        use MacroRuleSlot as S;
        match slot {
            S::SplitPeasCritical => absent("SplitPeasCritical", NOT_CONVERTED),
            S::CraftFoodCritical => absent(
                "CraftFoodCritical",
                "fused into SplitPeasCritical \u{2014} CraftingController.TickCritical runs the pea split and the category craft in one call.",
            ),
            S::RechargeSelfNormal => Box::new(
                ControllerMacroRule::new(
                    "RechargeSelfNormal",
                    tick(|cx, context| cx.tick_recharge(context, false, false)),
                )
                .with_gate(gate(|cx| {
                    cx.item_slot_is_free() && cx.core.combat.enabled()
                }))
                .with_running_detail(text(|cx| Some(cx.core.vital_recharge.status().to_owned()))),
            ),
            // No combat gate: keeping worn gear charged is not part of
            // fighting.
            S::RefillWieldedMana => Box::new(
                ControllerMacroRule::new(
                    "RefillWieldedMana",
                    tick(|cx, context| cx.tick_item_mana(context)),
                )
                .with_gate(gate(|cx| cx.item_slot_is_free())),
            ),
            S::RefillWieldedManaWhenOff => Box::new(
                ControllerMacroRule::new(
                    "RefillWieldedMana",
                    tick(|cx, context| cx.tick_item_mana(context)),
                )
                .with_gate(gate(|cx| {
                    cx.item_slot_is_free() && cx.core.settings.inventory.mana_charges_when_off
                })),
            ),
            S::BuffSelfNormal => Box::new(
                ControllerMacroRule::new(
                    "BuffSelf",
                    tick(|cx, context| cx.tick_buff_rule(context, false)),
                )
                .with_gate(gate(|cx| cx.item_slot_is_free()))
                .with_decline_reason(text(|cx| {
                    Some(cx.core.buff_rule.decline_reason().to_owned())
                })),
            ),
            S::SplitPeasNormal => absent(
                "SplitPeasNormal",
                "fused into CraftFood \u{2014} CraftingController.Tick runs the pea split before the category craft.",
            ),
            S::DispelSelf => Box::new(
                ControllerMacroRule::new("DispelSelf", tick(|cx, context| cx.tick_dispel(context)))
                    .with_gate(gate(|cx| {
                        cx.item_slot_is_free() && cx.core.combat.enabled()
                    })),
            ),
            S::UseDispelItem => absent(
                "UseDispelItem",
                "fused into DispelSelf \u{2014} DispelController.Tick tries the self dispel, then a dispel item, then the ally drum.",
            ),
            S::DispelAllies => absent(
                "DispelAllies",
                "fused into DispelSelf \u{2014} see UseDispelItem.",
            ),
            S::UseHealersHeart => Box::new(
                ControllerMacroRule::new(
                    "UseHealersHeart",
                    tick(|cx, context| {
                        let no_target = !cx.core.combat.has_target();
                        cx.tick_recharge(context, no_target, true)
                    }),
                )
                .with_gate(gate(|cx| {
                    cx.item_slot_is_free() && cx.core.combat.enabled()
                })),
            ),
            S::RechargeOther => absent(
                "RechargeOther",
                "fused into UseHealersHeart - VitalRechargeController's helper half tries the heart first and then gu's Adja's Gift / Replenish / Gift of Essence chain, in that order.",
            ),
            S::CraftFood => absent("CraftFood", NOT_CONVERTED),
            S::RefillPetChargesNormal => absent("RefillPetChargesNormal", NOT_CONVERTED),
            S::FellowshipManager => Box::new(ControllerMacroRule::new(
                "FellowshipManager",
                tick(|cx, context| {
                    let enabled = context.can_act
                        && cx.core.combat.enabled()
                        && cx.core.settings.combat.auto_fellow_management;
                    cx.core
                        .fellowship
                        .tick(cx.client.fellows(), context.elapsed_seconds, enabled);
                    false
                }),
            )),
            S::OpenDoor => Box::new(OpenDoorRule::new()),
            S::ReadScrollPriority => Box::new(
                ControllerMacroRule::new(
                    "ReadScrollPriority",
                    tick(|cx, context| cx.tick_read_scroll(context)),
                )
                .with_gate(gate(|cx| {
                    cx.item_slot_is_free()
                        && cx.core.combat.enabled()
                        && cx.core.settings.inventory.loot.priority_boost
                })),
            ),
            S::StackCramPriority => Box::new(
                ControllerMacroRule::new(
                    "StackCramPriority",
                    tick(|cx, context| cx.tick_stack_cram(context)),
                )
                .with_gate(gate(|cx| {
                    cx.item_slot_is_free()
                        && cx.core.combat.enabled()
                        && cx.core.settings.inventory.loot.priority_boost
                })),
            ),
            S::SalvageItemsPriority => absent(
                "SalvageItemsPriority",
                "fused into LootCorpsePriority \u{2014} the loot controller runs the salvage and salvage-combine steps itself whenever no corpse is open, which is the same gate the separate rule uses.",
            ),
            S::ReadScrollIdle => Box::new(MacroRulePreChain::new(
                Box::new(
                    ControllerMacroRule::new(
                        "ReadScrollIdle",
                        tick(|cx, context| cx.tick_read_scroll(context)),
                    )
                    .with_gate(gate(|cx| {
                        cx.item_slot_is_free()
                            && !cx.core.settings.inventory.loot.priority_boost
                            && cx.core.combat.enabled()
                    })),
                ),
                Vec::new(),
                vec![self.idle_peace()],
            )),
            S::StackCramIdle => Box::new(MacroRulePreChain::new(
                Box::new(
                    ControllerMacroRule::new(
                        "StackCramIdle",
                        tick(|cx, context| cx.tick_stack_cram(context)),
                    )
                    .with_gate(gate(|cx| {
                        cx.item_slot_is_free()
                            && !cx.core.settings.inventory.loot.priority_boost
                            && cx.core.combat.enabled()
                    })),
                ),
                Vec::new(),
                vec![self.idle_peace()],
            )),
            S::SalvageItemsIdle => absent(
                "SalvageItemsIdle",
                "fused into LootCorpseIdle \u{2014} see SalvageItemsPriority.",
            ),
            // The walk to a corpse reads the navigation switch, not just the
            // looting one.
            S::NavigateCorpsePriority => Box::new(corpse_walk("NavigateCorpsePriority", true)),
            S::NavigateCorpseIdle => Box::new(MacroRulePreChain::new(
                Box::new(corpse_walk("NavigateCorpseIdle", false)),
                Vec::new(),
                vec![Box::new(MacroRulePreChain::new(
                    self.idle_peace(),
                    vec![gate(|cx| cx.corpse_walk_outside_creep_distance())],
                    Vec::new(),
                ))],
            )),
            // No item-slot gate: the open rule is valid while the slot is
            // held and holds the pass through the open and each pull.
            S::OpenCorpsePriority => Box::new(
                ControllerMacroRule::new(
                    "OpenCorpsePriority",
                    tick(|cx, context| cx.tick_loot_rule(context)),
                )
                .with_gate(gate(|cx| {
                    cx.core.combat.enabled()
                        && cx.core.settings.inventory.loot.priority_boost
                        && cx.wait_on_corpse_id()
                }))
                .with_running_detail(text(|cx| Some(cx.core.loot.status.clone()))),
            ),
            S::OpenCorpseIdle => Box::new(MacroRulePreChain::new(
                Box::new(
                    ControllerMacroRule::new(
                        "OpenCorpseIdle",
                        tick(|cx, context| cx.tick_loot_rule(context)),
                    )
                    .with_gate(gate(|cx| {
                        !cx.core.settings.inventory.loot.priority_boost
                            && cx.core.combat.enabled()
                            && cx.core.settings.inventory.loot.enabled
                            && cx.wait_on_corpse_id()
                    }))
                    .with_running_detail(text(|cx| Some(cx.core.loot.status.clone()))),
                ),
                Vec::new(),
                vec![self.idle_peace()],
            )),
            S::LootCorpsePriority => absent(
                "LootCorpsePriority (loot step)",
                "fused into OpenCorpsePriority \u{2014} the loot controller runs select, open, pick up and close as one state machine, and the three rules it replaces sit next to each other in the list, so nothing else can win a pass between them.",
            ),
            S::CorpseWaitPriority => absent(
                "CorpseWaitPriority",
                "fused into OpenCorpsePriority \u{2014} the controller issues the closing use itself once the corpse is fully processed.",
            ),
            S::LootCorpseIdle => absent(
                "LootCorpseIdle (loot step)",
                "fused into OpenCorpseIdle \u{2014} see LootCorpsePriority.",
            ),
            S::CorpseWaitIdle => absent(
                "CorpseWaitIdle",
                "fused into OpenCorpseIdle \u{2014} see CorpseWaitPriority.",
            ),
            S::NavigateRoutePriority => {
                Box::new(route_rule("NavigateRoutePriority").with_gate(gate(|cx| {
                    cx.core.combat.enabled()
                        && cx.core.settings.navigation.priority
                        && cx.wait_on_corpse_id()
                        && cx.navigation_locks_are_clear()
                        && !cx.rare_corpse_holds_route()
                })))
            }
            S::NavigateRouteIdle => Box::new(MacroRulePreChain::new(
                Box::new(route_rule("NavigateRouteIdle").with_gate(gate(|cx| {
                    !cx.core.settings.navigation.priority
                        && cx.core.combat.enabled()
                        && cx.navigation_locks_are_clear()
                        && !cx.rare_corpse_holds_route()
                }))),
                Vec::new(),
                // Idle peace while walking the route; inside the creep band
                // and at a recall the walk itself runs.
                vec![Box::new(MacroRulePreChain::new(
                    self.idle_peace(),
                    vec![gate(|cx| {
                        let core = &mut *cx.core;
                        let mut turn: RouteTurn<'_> = route_turn!(cx.client, core);
                        core.navigation.allows_normal_movement(&mut turn)
                    })],
                    Vec::new(),
                ))],
            )),
            // The attack's first refusal: an item that was just used owns
            // the character for the rest of its cooldown.
            S::Attack => Box::new(
                ControllerMacroRule::new(
                    "Attack",
                    tick(|cx, context| cx.tick_combat_rule(context)),
                )
                .with_gate(gate(|cx| cx.item_slot_is_free()))
                .with_lost_turn(hook(|cx| {
                    cx.core.combat.set_paused(cx.client.rules(), true);
                }))
                .with_decline_reason(text(|cx| Some(cx.core.combat.status().to_owned())))
                .with_running_detail(text(|cx| Some(cx.core.combat.running_detail()))),
            ),
            S::SplitPeasIdle => absent(
                "SplitPeasIdle",
                "fused into CraftFoodIdle \u{2014} CraftingController.TickIdle runs the pea split before the idle-count category craft.",
            ),
            S::CraftFoodIdle => absent("CraftFoodIdle", NOT_CONVERTED),
            S::RefillPetChargesIdle => absent("RefillPetChargesIdle", NOT_CONVERTED),
            S::BuffSelfIdle => Box::new(
                ControllerMacroRule::new(
                    "BuffSelfIdle",
                    tick(|cx, context| cx.tick_buff_rule(context, true)),
                )
                .with_gate(gate(|cx| {
                    cx.item_slot_is_free() && cx.core.settings.buffs.idle_buff_topoff
                }))
                .with_decline_reason(text(|cx| {
                    Some(cx.core.buff_rule.decline_reason().to_owned())
                })),
            ),
            // Walking to a monster is a navigation job below the attack.
            S::NavigateMonster => Box::new(MacroRulePreChain::new(
                Box::new(
                    ControllerMacroRule::new(
                        "NavigateMonster",
                        tick(|cx, context| {
                            let core = &mut *cx.core;
                            let mut ctx: Ctx<'_> = combat_ctx!(cx.client, core);
                            core.combat
                                .claim_monster_approach_from_rule_pass(&mut ctx, context.can_act)
                        }),
                    )
                    .with_gate(gate(|cx| {
                        cx.core.combat.enabled()
                            && cx.core.settings.navigation.enabled
                            && cx.navigation_locks_are_clear()
                    }))
                    .with_lost_turn(hook(|cx| {
                        cx.core
                            .combat
                            .stop_monster_approach_for_lost_turn(cx.client.rules());
                    })),
                ),
                Vec::new(),
                vec![Box::new(MacroRulePreChain::new(
                    self.idle_peace(),
                    vec![gate(|cx| {
                        let core = &mut *cx.core;
                        let mut ctx: Ctx<'_> = combat_ctx!(cx.client, core);
                        core.combat.is_approach_outside_creep_distance(&mut ctx)
                    })],
                    Vec::new(),
                ))],
            )),
            S::RechargeSelfNoTarget => Box::new(
                ControllerMacroRule::new(
                    "RechargeSelfNoTarget",
                    tick(|cx, context| cx.tick_recharge(context, true, false)),
                )
                .with_gate(gate(|cx| {
                    cx.item_slot_is_free() && cx.core.combat.enabled()
                }))
                .with_running_detail(text(|cx| Some(cx.core.vital_recharge.status().to_owned()))),
            ),
            S::RandomHelper => absent("RandomHelper", NOT_CONVERTED),
            S::IdlePeace => Box::new(MacroRulePreChain::new(
                self.idle_peace(),
                vec![gate(|cx| cx.core.combat.enabled())],
                Vec::new(),
            )),
        }
    }
}

/// The walk to a corpse, priority or idle.
fn corpse_walk(name: &str, priority: bool) -> ControllerMacroRule<TankFamily> {
    ControllerMacroRule::new(name, tick(|cx, context| cx.claim_corpse_walk(context)))
        .with_gate(gate(move |cx| {
            let loot = &cx.core.settings.inventory.loot;
            cx.core.combat.enabled()
                && loot.enabled
                && loot.priority_boost == priority
                && cx.wait_on_corpse_id()
                && cx.core.settings.navigation.enabled
                && cx.navigation_locks_are_clear()
        }))
        .with_lost_turn(hook(|cx| {
            cx.core
                .corpse_approach
                .stop_for_lost_turn(cx.client.rules());
        }))
        .with_running_detail(text(|cx| Some(cx.core.corpse_approach.running_detail())))
        .with_decline_reason(text(|cx| Some(cx.core.corpse_approach.status().to_owned())))
}

/// The route's rule, before its gate.
fn route_rule(name: &str) -> ControllerMacroRule<TankFamily> {
    ControllerMacroRule::new(name, tick(|cx, context| cx.claim_route(context)))
        .with_lost_turn(hook(|cx| {
            let core = &mut *cx.core;
            let mut turn: RouteTurn<'_> = route_turn!(cx.client, core);
            core.navigation.stop_for_lost_turn(&mut turn);
        }))
        .with_running_detail(text(|cx| {
            let core = &mut *cx.core;
            let mut turn: RouteTurn<'_> = route_turn!(cx.client, core);
            Some(core.navigation.running_detail(&mut turn))
        }))
        .with_decline_reason(text(|cx| Some(cx.core.navigation.status().to_owned())))
}

impl MacroRuleProvider<TankFamily> for Provider {
    fn create(&mut self, slot: MacroRuleSlot) -> Box<dyn MacroRule<TankFamily>> {
        Box::new(WritingOut(self.rule(slot)))
    }

    fn create_independent(&mut self, slot: MacroIndependentSlot) -> Box<dyn MacroRule<TankFamily>> {
        match slot {
            MacroIndependentSlot::SummonPet => absent("SummonPet", NOT_CONVERTED),
        }
    }
}

/// The scheduler over VTank's table, with the shared peace-when-idle rule.
pub(crate) fn build_scheduler(idle_peace: &SharedIdlePeace) -> MacroScheduler<TankFamily> {
    build(&mut Provider {
        idle_peace: idle_peace.clone(),
    })
}

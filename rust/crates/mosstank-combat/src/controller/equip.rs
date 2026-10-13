//! What the fight wields: the weapon the rule names or the automatic walk
//! picks, the off-hand item, the ammunition a launcher needs, and the
//! element the attack resolves to.

use std::cell::RefCell;
use std::collections::HashMap;

use ac_plugin_abi::combat::{CombatMode, CombatTarget};
use ac_plugin_abi::items::{EquipmentItem, EquipmentStatus, InventoryItem};
use ac_plugin_abi::world::ObjectClass;
use mosstank_automation::Automation;
use mosstank_automation::surface::is_assessed;
use mosstank_data::attack_spells::resolve_magic_damage_mode;
use mosstank_data::damage::{MonsterDamageType, UNLISTED_ELEMENT_ORDER};
use mosstank_data::weapon::{element as weapon_element, ladder, launcher_type};
use mosstank_settings::ammunition::{self, VtankPrismaticAmmoPolicy};
use mosstank_settings::items::item_use_specifiers;
use mosstank_settings::model::{CombatSettings, VitalSettings};
use mosstank_settings::monster::{MonsterActionFlags, MonsterRuleActions, secondary_equip};

use super::{
    AmmunitionCrafting, CombatController, Ctx, VOID_MAGIC_SKILL, WAR_MAGIC_SKILL, element_name,
    is_trained, reborrow_crafting,
};
use crate::debuff::DebuffIdentity;
use crate::gate::{AmmunitionHooks, CombatModeGate, GateTurn, Prepare, find_wielded};

/// The off-hand slots (shield or a one-handed melee weapon).
const OFF_HAND_LOCATIONS: u32 = 0x0020_0000 | 0x0010_0000;
/// Two-handed.
const TWO_HANDED: u32 = 0x0200_0000;

/// What the ammunition choice decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AmmunitionPlanKind {
    /// Not a launcher, or the right stack is wielded.
    Satisfied,
    /// Wield this stack.
    Wield,
    /// Make some.
    Craft,
    /// None can be had.
    Unavailable,
}

/// The ammunition choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmmunitionPlan {
    /// What to do.
    pub kind: AmmunitionPlanKind,
    /// The stack to wield.
    pub object_id: u32,
    /// The ammunition's name.
    pub name: String,
    /// Why none can be had.
    pub notice: String,
    /// The crafting plan.
    pub craft: Option<u32>,
}

impl AmmunitionPlan {
    fn satisfied() -> Self {
        Self {
            kind: AmmunitionPlanKind::Satisfied,
            object_id: 0,
            name: String::new(),
            notice: String::new(),
            craft: None,
        }
    }
}

/// The C# `RawDamageType`: the damage bits of an element.
fn raw_damage_type(damage: MonsterDamageType) -> i32 {
    match damage {
        MonsterDamageType::Slash => 0x0001,
        MonsterDamageType::Pierce => 0x0002,
        MonsterDamageType::Bludgeon => 0x0004,
        MonsterDamageType::Cold => 0x0008,
        MonsterDamageType::Fire => 0x0010,
        MonsterDamageType::Acid => 0x0020,
        MonsterDamageType::Electric => 0x0040,
        MonsterDamageType::Nether => 0x0400,
        _ => 0,
    }
}

/// Whether an owned object is one of the profile's items
/// (`CombatProfileItems.IsProfiled`): by id when the profile lists ids.
#[must_use]
pub fn is_profiled(settings: &CombatSettings, object_id: u32, name: &str) -> bool {
    if settings.combat_item_object_ids.contains(&object_id) {
        return true;
    }
    settings.combat_item_object_ids.is_empty() && settings.combat_item_names.contains(name)
}

/// The weapon and the shield held now.
pub(super) fn wielded_pair(equipment: &[EquipmentItem]) -> (u32, u32) {
    let mut weapon = 0;
    let mut offhand = 0;
    for item in equipment {
        if weapon == 0 && mosstank_automation::equipment::is_weapon_slot(item.equipped_location) {
            weapon = item.object_id;
        } else if offhand == 0 && item.equipped_location == 0x0020_0000 {
            offhand = item.object_id;
        }
    }
    (weapon, offhand)
}

fn is_profiled_caster(items: &[EquipmentItem], object_id: u32) -> bool {
    items
        .iter()
        .find(|item| item.object_id == object_id)
        .is_some_and(CombatModeGate::is_caster)
}

/// The owned items in the Items page's order, then by name and id.
fn in_profile_order(settings: &CombatSettings, items: &[EquipmentItem]) -> Vec<EquipmentItem> {
    if items.len() < 2 {
        return items.to_vec();
    }
    let page = &settings.combat_item_order;
    let page_index = |name: &str| {
        page.iter()
            .position(|entry| entry == name)
            .unwrap_or(usize::MAX)
    };
    let mut ordered = items.to_vec();
    ordered.sort_by(|left, right| {
        page_index(&left.name)
            .cmp(&page_index(&right.name))
            .then_with(|| mosstank_dotnet::text::ordinal::compare(&left.name, &right.name))
            .then(left.object_id.cmp(&right.object_id))
    });
    ordered
}

fn resolve_equipment_object_id(
    session_object_id: u32,
    durable_name: &str,
    items: &[EquipmentItem],
) -> u32 {
    if session_object_id != 0 && items.iter().any(|item| item.object_id == session_object_id) {
        return session_object_id;
    }
    if durable_name.trim().is_empty() {
        return 0;
    }
    items
        .iter()
        .find(|item| item.name == durable_name)
        .map_or(0, |item| item.object_id)
}

/// Whether an item could be held in the off hand at all.
fn can_fill_off_hand(item: &InventoryItem) -> bool {
    item.valid_locations & OFF_HAND_LOCATIONS != 0 && item.valid_locations & TWO_HANDED == 0
}

/// The pack item a rule's named off hand means.
pub(super) fn resolve_inventory_offhand_object_id(
    actions: &MonsterRuleActions,
    items: &[InventoryItem],
) -> u32 {
    if actions.offhand_object_id != 0
        && items
            .iter()
            .any(|item| item.object_id == actions.offhand_object_id)
    {
        return actions.offhand_object_id;
    }
    if actions.offhand_name.trim().is_empty() {
        return 0;
    }
    items
        .iter()
        .find(|item| item.name == actions.offhand_name && can_fill_off_hand(item))
        .map_or(0, |item| item.object_id)
}

fn needs_inventory_offhand(actions: &MonsterRuleActions, selected: Option<u32>) -> bool {
    selected == Some(0) && (actions.offhand_object_id != 0 || !actions.offhand_name.is_empty())
}

fn weapon_stance(actions: &MonsterRuleActions, weapon: u32, owned: &[EquipmentItem]) -> CombatMode {
    if weapon == 0 {
        return CombatMode::Magic;
    }
    if let Some(item) = owned.iter().find(|item| item.object_id == weapon) {
        return CombatModeGate::mode_for(item);
    }
    if actions.weapon_to_use_raw == 0 {
        CombatMode::Magic
    } else {
        CombatMode::Melee
    }
}

fn weapon_element_of(weapon: u32, owned: &[EquipmentItem]) -> MonsterDamageType {
    if weapon == 0 {
        return MonsterDamageType::None;
    }
    owned
        .iter()
        .find(|item| item.object_id == weapon)
        .map_or(MonsterDamageType::None, |item| {
            weapon_element::resolve(
                item.imbued_effect,
                item.resistance_cleaving,
                item.damage_type,
            )
        })
}

/// The gate's ammunition question, answered by the fight.
struct AmmoHooks<'c, 's> {
    controller: &'c mut CombatController,
    settings: &'s CombatSettings,
    vitals: &'s VitalSettings,
    crafting: Option<&'s mut dyn AmmunitionCrafting>,
}

impl AmmunitionHooks for AmmoHooks<'_, '_> {
    fn ammunition_stale(
        &mut self,
        automation: &mut dyn Automation,
        weapon: u32,
        element: MonsterDamageType,
    ) -> bool {
        let mut cx = Ctx {
            automation,
            settings: self.settings,
            vitals: self.vitals,
            crafting: reborrow_crafting(&mut self.crafting),
        };
        self.controller.ammunition_stale(&mut cx, weapon, element)
    }
}

impl CombatController {
    /// Plans the weapon and off hand for the target; never claims the pass
    /// itself.
    pub(super) fn tick_equipment(&mut self, cx: &mut Ctx<'_>) -> bool {
        self.planned_weapon = 0;
        self.planned_offhand = None;
        self.planned_weapon_search_found_nothing = false;

        let actions = self.target_actions();
        let primary_requires_weapon = actions.uses_primary_attack() || actions.uses_ring();
        if !primary_requires_weapon && !cx.settings.switch_wands_to_debuff {
            return false;
        }
        if !cx.automation.equipment_available() {
            return false;
        }
        let items = self.pass_equipment(cx.automation);
        let desired_weapon = if actions.weapon_to_use_raw == 0 {
            // "No weapon": a wand, and nothing is picked for it.
            0
        } else {
            let (mut desired, unlisted) =
                self.resolve_rule_weapon_object_id(cx.settings, &actions, &items);
            if unlisted {
                self.post_attack_warning(
                    cx.automation,
                    "Warning: this rule's Weapon column names an item that is not in the Items \
                     list, so it will not be wielded. Add it to the Items list, or set the column \
                     back to <AUTO>.",
                );
            }
            if desired == 0 {
                let target = self.find_target(self.target_id);
                desired = self.select_automatic_weapon(cx, &items, &actions, &target);
                self.planned_weapon_search_found_nothing =
                    self.nothing_listed_can_fight(cx.automation, &items, desired);
            }
            desired
        };
        self.planned_weapon = desired_weapon;
        self.planned_offhand =
            self.planned_secondary_for_lazy(cx, &actions, &items, desired_weapon);
        false
    }

    fn resolve_ammunition_plan(
        &mut self,
        cx: &mut Ctx<'_>,
        equipment_items: &[EquipmentItem],
        desired_weapon: u32,
        configured_damage: MonsterDamageType,
    ) -> AmmunitionPlan {
        let launcher = equipment_items
            .iter()
            .find(|item| item.object_id == desired_weapon)
            .cloned()
            .unwrap_or_default();
        let launcher_kind = launcher_type(&launcher);
        if launcher_kind == 0 {
            return AmmunitionPlan::satisfied();
        }
        if !self.game_info.is_loaded {
            return AmmunitionPlan {
                kind: AmmunitionPlanKind::Unavailable,
                notice: "Ammunition database is unavailable".to_owned(),
                ..AmmunitionPlan::satisfied()
            };
        }

        let mut damage = configured_damage;
        let mut prismatic = VtankPrismaticAmmoPolicy::NoPrismatic;
        if damage == MonsterDamageType::Auto {
            let target = self.find_target(self.target_id);
            let actions = self.target_actions();
            damage = self.resolve_attack_element(cx, &actions, &target);
            prismatic = VtankPrismaticAmmoPolicy::Any;
        } else if damage == MonsterDamageType::Prismatic {
            prismatic = VtankPrismaticAmmoPolicy::ForcePrismatic;
        }
        let inventory = self.pass_inventory(cx.automation);
        let mut counts: HashMap<String, i32> = HashMap::new();
        for item in &inventory {
            *counts.entry(item.name.clone()).or_default() += item.stack_size;
        }
        let craftable: RefCell<HashMap<String, Option<u32>>> = RefCell::new(HashMap::new());
        let skills = cx.automation.skills();
        let settings = cx.settings;
        let crafting = RefCell::new((cx.crafting.as_deref_mut(), &mut *cx.automation));
        let is_available = |name: &str| -> bool {
            if counts.get(name).copied().unwrap_or(0) >= 1 {
                return true;
            }
            let mut cache = craftable.borrow_mut();
            if let Some(cached) = cache.get(name) {
                return cached.is_some();
            }
            let resolved = {
                let mut pair = crafting.borrow_mut();
                let (hook, automation) = &mut *pair;
                hook.as_deref_mut()
                    .and_then(|hook| hook.resolve(&mut **automation, settings, name, 1))
            };
            cache.insert(name.to_owned(), resolved);
            resolved.is_some()
        };
        let options = self.game_info.ammunition_options.clone();
        let selected = ammunition::select(
            &options,
            launcher_kind,
            damage,
            prismatic,
            cx.settings.use_special_ammo,
            &skills,
            &is_available,
        );
        let Some(option) = selected else {
            return AmmunitionPlan {
                kind: AmmunitionPlanKind::Unavailable,
                notice: format!("No {damage:?} ammunition is available"),
                ..AmmunitionPlan::satisfied()
            };
        };

        // The stack in the quiver satisfies the row while it holds some.
        if let Some(current) = equipment_items
            .iter()
            .find(|item| item.equipped_location == 0x0080_0000)
            && current.stack_size > 0
            && current.name == option.name
        {
            return AmmunitionPlan::satisfied();
        }

        let mut desired: Option<&EquipmentItem> = None;
        for item in equipment_items {
            if item.name == option.name
                && item.stack_size > desired.map_or(0, |current| current.stack_size)
            {
                desired = Some(item);
            }
        }
        match desired {
            Some(stack) if stack.object_id != 0 => AmmunitionPlan {
                kind: AmmunitionPlanKind::Wield,
                object_id: stack.object_id,
                name: option.name.clone(),
                notice: String::new(),
                craft: None,
            },
            _ => AmmunitionPlan {
                kind: AmmunitionPlanKind::Craft,
                object_id: 0,
                name: option.name.clone(),
                notice: String::new(),
                craft: craftable.borrow().get(&option.name).copied().flatten(),
            },
        }
    }

    /// The gate's `AmmunitionStale`: whether the weapon about to be readied
    /// needs other ammunition (the plan is kept for the wield).
    pub fn ammunition_stale(
        &mut self,
        cx: &mut Ctx<'_>,
        weapon: u32,
        element: MonsterDamageType,
    ) -> bool {
        self.gate_ammunition_plan = None;
        if !cx.automation.equipment_available() || weapon == 0 {
            return false;
        }
        let equipment = self.pass_equipment(cx.automation);
        let plan = self.resolve_ammunition_plan(cx, &equipment, weapon, element);
        if plan.kind == AmmunitionPlanKind::Unavailable {
            // Said, not swallowed.
            self.set_status(plan.notice.clone());
            self.post_attack_warning(cx.automation, &format!("Warning: {}.", plan.notice));
            return false;
        }
        if !matches!(
            plan.kind,
            AmmunitionPlanKind::Wield | AmmunitionPlanKind::Craft
        ) {
            return false;
        }
        self.gate_ammunition_plan = Some(plan);
        true
    }

    /// The gate's `WieldAmmunition`: wields what the last question chose.
    pub fn wield_ammunition(&mut self, cx: &mut Ctx<'_>, element: MonsterDamageType) -> bool {
        let _ = element;
        if !cx.automation.equipment_available() {
            return false;
        }
        let Some(plan) = self.gate_ammunition_plan.take() else {
            return false;
        };
        let equipment = self.pass_equipment(cx.automation);
        self.execute_ammunition_plan(cx, &equipment, &plan)
    }

    fn execute_ammunition_plan(
        &mut self,
        cx: &mut Ctx<'_>,
        equipment: &[EquipmentItem],
        plan: &AmmunitionPlan,
    ) -> bool {
        match plan.kind {
            AmmunitionPlanKind::Satisfied => return false,
            AmmunitionPlanKind::Unavailable => {
                self.set_status(plan.notice.clone());
                return true;
            }
            _ => {}
        }
        let dropped = self.gate.borrow_mut().try_drop_to_peace(
            cx.automation,
            cx.settings,
            cx.vitals,
            equipment,
            &plan.name,
        );
        self.drain_gate_stops(cx.automation);
        if !dropped {
            return true;
        }
        if plan.kind == AmmunitionPlanKind::Wield {
            let armed = self
                .gate
                .borrow_mut()
                .try_arm_ammunition_swap(cx.automation, plan.object_id);
            if !armed {
                self.set_status("Waiting for equipment");
                return true;
            }
            let equip = cx.automation.equip(plan.object_id);
            self.set_status(if equip.status == EquipmentStatus::Refused {
                equip
                    .notice
                    .unwrap_or_else(|| format!("Cannot equip {}.", plan.name))
            } else {
                format!("Equipping {}", plan.name)
            });
            return true;
        }
        if let Some(craft) = plan.craft
            && cx
                .crafting
                .as_deref_mut()
                .is_some_and(|crafting| crafting.request(&mut *cx.automation, cx.settings, craft))
        {
            self.set_status(format!("Crafting {}", plan.name));
            return true;
        }
        self.set_status(format!("Waiting to craft {}", plan.name));
        true
    }

    pub(super) fn resolve_physical_actions(
        &self,
        actions: MonsterRuleActions,
        target: &CombatTarget,
        inventory: &[InventoryItem],
    ) -> MonsterRuleActions {
        if actions.damage_type != MonsterDamageType::Auto {
            return actions;
        }
        let preferences = self.game_info.damage_preferences(&target.name).to_vec();
        for &damage in &preferences {
            let mask = raw_damage_type(damage);
            if mask == 0 {
                continue;
            }
            if inventory
                .iter()
                .any(|item| item.equipped_location != 0 && item.damage_type & mask != 0)
            {
                return MonsterRuleActions {
                    damage_type: damage,
                    ..actions
                };
            }
        }
        match preferences.first() {
            None => actions,
            Some(&first) => MonsterRuleActions {
                damage_type: first,
                ..actions
            },
        }
    }

    /// The element the attack would strike this monster with (the summon
    /// rule ranks essences by it).
    pub fn attack_element_for(
        &mut self,
        cx: &mut Ctx<'_>,
        target: &CombatTarget,
    ) -> MonsterDamageType {
        let actions = cx.settings.resolve_rule(target).actions().clone();
        self.resolve_attack_element(cx, &actions, target)
    }

    /// Drops to peace for the pet (`ReadyToActInPeace`).
    pub fn ready_to_act_in_peace(&mut self, cx: &mut Ctx<'_>) -> bool {
        let equipment = if cx.automation.equipment_available() {
            cx.automation.owned_equipment()
        } else {
            Vec::new()
        };
        let ready = self.gate.borrow_mut().try_drop_to_peace(
            cx.automation,
            cx.settings,
            cx.vitals,
            &equipment,
            "the combat pet",
        );
        self.drain_gate_stops(cx.automation);
        ready
    }

    pub(super) fn try_prepare_attack(&mut self, cx: &mut Ctx<'_>) -> bool {
        self.attack_has_no_usable_weapon = false;
        let mut wanted = CombatMode::Magic;
        let mut planned_weapon = 0;
        if self.planned_weapon != 0 && cx.automation.equipment_available() {
            let equipment = self.pass_equipment(cx.automation);
            if let Some(item) = equipment
                .iter()
                .find(|item| item.object_id == self.planned_weapon)
            {
                wanted = CombatModeGate::mode_for(item);
                planned_weapon = self.planned_weapon;
            }
        }

        // Nothing listed can fight this monster: said once, and the monster
        // is left to the rules below the attack.
        if self.planned_weapon_search_found_nothing {
            const NOTICE: &str = "No weapon in the profile's item list can be used against this \
                                  monster. Add the weapon you fight with to the Items list.";
            self.post_attack_warning(cx.automation, &format!("Warning: {NOTICE}"));
            self.set_status(NOTICE);
            self.attack_has_no_usable_weapon = true;
            self.note_target_reason(self.target_id, "no usable listed weapon");
            self.clear_actions_for_pass(
                self.target_id,
                MonsterActionFlags::ATTACK | MonsterActionFlags::STREAK | MonsterActionFlags::RING,
            );
            return false;
        }

        let element = self
            .target_rule
            .as_ref()
            .map_or(MonsterDamageType::None, |rule| rule.actions().damage_type);
        let captured = self.pass_equipment(cx.automation);
        let request = Prepare {
            wanted,
            override_item_id: planned_weapon,
            auto_select: planned_weapon == 0,
            element,
            captured: Some(&captured),
            secondary_item_id: self.planned_offhand,
        };
        let gate = self.gate.clone();
        let ready = {
            let mut gate = gate.borrow_mut();
            let settings: &CombatSettings = cx.settings;
            let mut hooks = AmmoHooks {
                controller: self,
                settings,
                vitals: cx.vitals,
                crafting: reborrow_crafting(&mut cx.crafting),
            };
            let mut turn = GateTurn {
                automation: &mut *cx.automation,
                combat: settings,
                vitals: cx.vitals,
            };
            gate.try_prepare(&mut turn, request, Some(&mut hooks))
        };
        let wield = gate.borrow_mut().take_ammunition_wield();
        if let Some(element) = wield {
            self.wield_ammunition(cx, element);
        }
        self.drain_gate_stops(cx.automation);
        if ready {
            return true;
        }
        let status = gate.borrow().status().to_owned();
        self.set_status(status);
        false
    }

    /// The weapon the automatic walk reaches for against this monster.
    pub(super) fn select_automatic_weapon(
        &mut self,
        cx: &mut Ctx<'_>,
        items: &[EquipmentItem],
        actions: &MonsterRuleActions,
        target: &CombatTarget,
    ) -> u32 {
        let wanted: Vec<MonsterDamageType> = if matches!(
            actions.damage_type,
            MonsterDamageType::Auto | MonsterDamageType::Prismatic
        ) {
            self.game_info.damage_preferences(&target.name).to_vec()
        } else {
            vec![actions.damage_type]
        };
        let species = self.species_of(target);
        let war_trained = is_trained(cx.automation, WAR_MAGIC_SKILL);
        let void_trained = is_trained(cx.automation, VOID_MAGIC_SKILL);
        let ordered = in_profile_order(cx.settings, items);

        // What the walk reads: assessment, delivery and vulnerability.
        let mut assessed: HashMap<u32, bool> = HashMap::new();
        for item in &ordered {
            assessed.insert(item.object_id, is_assessed(cx.automation, item.object_id));
        }
        let settings = cx.settings;
        let deliverable: RefCell<HashMap<(MonsterDamageType, u32), bool>> =
            RefCell::new(HashMap::new());
        let mut vulnerable: HashMap<MonsterDamageType, bool> = HashMap::new();
        for &element in &[
            MonsterDamageType::Slash,
            MonsterDamageType::Pierce,
            MonsterDamageType::Bludgeon,
            MonsterDamageType::Cold,
            MonsterDamageType::Fire,
            MonsterDamageType::Acid,
            MonsterDamageType::Electric,
            MonsterDamageType::Nether,
        ] {
            let already = self.is_already_vulnerable(cx.automation, target, element);
            vulnerable.insert(element, already);
        }
        // The answers the walk may ask for; only the ones it does ask are
        // remembered for the pass, as the C# memoises them as asked.
        for item in &ordered {
            if launcher_type(item) == 0 {
                continue;
            }
            for &element in &wanted {
                let key = (element, item.object_id);
                let answer = match self.pass_deliverable.get(&key) {
                    Some(&cached) => cached,
                    None => self.can_weapon_deliver_core(cx, item, element),
                };
                deliverable.borrow_mut().insert(key, answer);
            }
        }
        let consulted: RefCell<Vec<(MonsterDamageType, u32)>> = RefCell::new(Vec::new());
        let selectable = |item: &EquipmentItem| {
            is_profiled(settings, item.object_id, &item.name)
                && item_use_specifiers::uses_for(settings, item.object_id) & 1 != 0
                && assessed.get(&item.object_id).copied().unwrap_or(false)
        };
        let can_deliver = |item: &EquipmentItem, element: MonsterDamageType| {
            if launcher_type(item) == 0 {
                return true;
            }
            consulted.borrow_mut().push((element, item.object_id));
            deliverable
                .borrow()
                .get(&(element, item.object_id))
                .copied()
                .unwrap_or(false)
        };
        let already_vulnerable =
            |element: MonsterDamageType| vulnerable.get(&element).copied().unwrap_or(false);
        let walk = |profiled: &dyn Fn(&EquipmentItem) -> bool| {
            ladder::select(&ladder::Request {
                items: &ordered,
                is_profiled: profiled,
                wanted: &wanted,
                species,
                can_deliver: &can_deliver,
                already_vulnerable: &already_vulnerable,
                war_trained,
                void_trained,
                exclude_object_id: actions.offhand_object_id,
            })
        };

        let first = walk(&selectable);
        if first.last_resort {
            self.post_attack_warning(
                cx.automation,
                "Warning: no weapons found that can be autoselected for current target. Add \
                 some weapons to the items list!",
            );
        }
        let mut chosen = first.object_id;
        // A rule that swings keeps something that can strike over a wand.
        if chosen != 0 && is_profiled_caster(items, chosen) {
            let striking =
                walk(&|item: &EquipmentItem| selectable(item) && !CombatModeGate::is_caster(item));
            if striking.object_id != 0 {
                chosen = striking.object_id;
            }
        }
        for key in consulted.into_inner() {
            if let Some(&answer) = deliverable.borrow().get(&key) {
                self.pass_deliverable.entry(key).or_insert(answer);
            }
        }
        chosen
    }

    /// Whether the automatic choice has left nothing to fight with.
    fn nothing_listed_can_fight(
        &self,
        automation: &mut dyn Automation,
        items: &[EquipmentItem],
        chosen: u32,
    ) -> bool {
        if chosen != 0 && !is_profiled_caster(items, chosen) {
            return false;
        }
        if is_trained(automation, WAR_MAGIC_SKILL) || is_trained(automation, VOID_MAGIC_SKILL) {
            return false;
        }
        find_wielded(items).is_some_and(|worn| !CombatModeGate::is_caster(worn))
    }

    /// The monster's species, or -1 when nothing names it.
    fn species_of(&self, target: &CombatTarget) -> i32 {
        if target.species_id > 0 {
            return target.species_id;
        }
        self.game_info
            .species_member(&target.name)
            .map_or(-1, |member| member.species)
    }

    fn is_already_vulnerable(
        &mut self,
        automation: &mut dyn Automation,
        target: &CombatTarget,
        element: MonsterDamageType,
    ) -> bool {
        let identity = DebuffIdentity::new(MonsterActionFlags::VULNERABILITY, element);
        let Some(vulnerability) = self.find_debuff_spell(automation, identity) else {
            return false;
        };
        // "Still up in half a second's time".
        !self
            .debuffs
            .is_due(target.object_id, identity, &vulnerability, self.now, 0.5)
    }

    fn can_weapon_deliver_core(
        &mut self,
        cx: &mut Ctx<'_>,
        item: &EquipmentItem,
        element: MonsterDamageType,
    ) -> bool {
        let launcher_kind = launcher_type(item);
        if launcher_kind == 0 {
            return true;
        }
        if !self.game_info.is_loaded {
            return false;
        }
        self.select_ammunition(cx, launcher_kind, element)
    }

    fn select_ammunition(
        &mut self,
        cx: &mut Ctx<'_>,
        launcher_kind: i32,
        element: MonsterDamageType,
    ) -> bool {
        let skills = cx.automation.skills();
        let options = self.game_info.ammunition_options.clone();
        if self.pass_ammunition_counts.is_none() {
            let mut counts: HashMap<String, i32> = HashMap::new();
            for item in self.pass_inventory(cx.automation) {
                *counts.entry(item.name.clone()).or_default() += item.stack_size;
            }
            self.pass_ammunition_counts = Some(counts);
        }
        let counts = self.pass_ammunition_counts.clone().unwrap_or_default();
        let answers = RefCell::new(std::mem::take(&mut self.pass_ammunition_answers));
        let settings = cx.settings;
        let crafting = RefCell::new((cx.crafting.as_deref_mut(), &mut *cx.automation));
        let is_available = |name: &str| -> bool {
            if let Some(&cached) = answers.borrow().get(name) {
                return cached;
            }
            let answer = counts.get(name).copied().unwrap_or(0) >= 1 || {
                let mut pair = crafting.borrow_mut();
                let (hook, automation) = &mut *pair;
                hook.as_deref_mut().is_some_and(|hook| {
                    hook.resolve(&mut **automation, settings, name, 1).is_some()
                })
            };
            answers.borrow_mut().insert(name.to_owned(), answer);
            answer
        };
        let found = ammunition::select(
            &options,
            launcher_kind,
            element,
            VtankPrismaticAmmoPolicy::Any,
            cx.settings.use_special_ammo,
            &skills,
            &is_available,
        )
        .is_some();
        self.pass_ammunition_answers = answers.into_inner();
        found
    }

    /// The weapon a rule names, honoured only while it is a profiled item;
    /// the second answer says it was named but unlisted.
    pub(super) fn resolve_rule_weapon_object_id(
        &self,
        settings: &CombatSettings,
        actions: &MonsterRuleActions,
        items: &[EquipmentItem],
    ) -> (u32, bool) {
        let resolved =
            resolve_equipment_object_id(actions.weapon_object_id, &actions.weapon_name, items);
        if resolved == 0 {
            return (0, false);
        }
        match items.iter().find(|item| item.object_id == resolved) {
            Some(item) if is_profiled(settings, item.object_id, &item.name) => (resolved, false),
            Some(_) => (0, true),
            None => (0, false),
        }
    }

    /// The off hand the rule will ask for (`PlannedSecondaryFor`).
    pub(super) fn planned_secondary_for(
        &mut self,
        cx: &mut Ctx<'_>,
        actions: &MonsterRuleActions,
        items: &[EquipmentItem],
        primary: u32,
        inventory: &[InventoryItem],
    ) -> Option<u32> {
        let selected = self.resolve_secondary_equipment(cx, actions, items, primary);
        if needs_inventory_offhand(actions, selected) {
            Some(resolve_inventory_offhand_object_id(actions, inventory))
        } else {
            selected
        }
    }

    /// The same, reading the pack only when needed.
    fn planned_secondary_for_lazy(
        &mut self,
        cx: &mut Ctx<'_>,
        actions: &MonsterRuleActions,
        items: &[EquipmentItem],
        primary: u32,
    ) -> Option<u32> {
        let selected = self.resolve_secondary_equipment(cx, actions, items, primary);
        if needs_inventory_offhand(actions, selected) {
            let inventory = self.pass_inventory(cx.automation);
            Some(resolve_inventory_offhand_object_id(actions, &inventory))
        } else {
            selected
        }
    }

    fn resolve_secondary_equipment(
        &mut self,
        cx: &mut Ctx<'_>,
        actions: &MonsterRuleActions,
        items: &[EquipmentItem],
        primary: u32,
    ) -> Option<u32> {
        if actions.offhand_object_id != 0 || !actions.offhand_name.is_empty() {
            return Some(resolve_equipment_object_id(
                actions.offhand_object_id,
                &actions.offhand_name,
                items,
            ));
        }
        match actions.secondary_equip_raw {
            secondary_equip::NONE => Some(0),
            secondary_equip::AUTO_WEAPON => {
                Some(self.select_automatic_secondary_weapon(cx, items, primary))
            }
            secondary_equip::AUTO_SHIELD => None,
            secondary_equip::AUTO => Some(self.resolve_automatic_secondary(cx, items, primary)),
            _ => Some(0),
        }
    }

    fn resolve_automatic_secondary(
        &mut self,
        cx: &mut Ctx<'_>,
        items: &[EquipmentItem],
        primary: u32,
    ) -> u32 {
        let shield = is_trained(cx.automation, 48);
        let dual_wield = is_trained(cx.automation, 49);
        let shield_item = select_automatic_shield(cx.settings, items);
        if shield && !dual_wield {
            return shield_item;
        }
        let weapon = self.select_automatic_secondary_weapon(cx, items, primary);
        if !shield && dual_wield {
            return weapon;
        }
        if shield_item != 0 {
            shield_item
        } else {
            weapon
        }
    }

    fn select_automatic_secondary_weapon(
        &mut self,
        cx: &mut Ctx<'_>,
        items: &[EquipmentItem],
        primary: u32,
    ) -> u32 {
        for item in in_profile_order(cx.settings, items) {
            if item.object_id == primary
                || item.object_class != ObjectClass::MeleeWeapon
                || item.valid_locations & TWO_HANDED != 0
                || item_use_specifiers::uses_for(cx.settings, item.object_id) & 2 == 0
                || !is_profiled(cx.settings, item.object_id, &item.name)
                || !is_assessed(cx.automation, item.object_id)
            {
                continue;
            }
            return item.object_id;
        }
        0
    }

    /// The element the attack resolves to for a rule and monster (once per
    /// pass).
    pub(super) fn resolve_attack_element(
        &mut self,
        cx: &mut Ctx<'_>,
        actions: &MonsterRuleActions,
        target: &CombatTarget,
    ) -> MonsterDamageType {
        if let Some((_, _, cached)) = self
            .pass_elements
            .iter()
            .find(|(memo, id, _)| *id == target.object_id && memo == actions)
        {
            return *cached;
        }
        let resolved = self.resolve_attack_element_core(cx, actions, target);
        self.pass_elements
            .push((actions.clone(), target.object_id, resolved));
        resolved
    }

    fn resolve_attack_element_core(
        &mut self,
        cx: &mut Ctx<'_>,
        actions: &MonsterRuleActions,
        target: &CombatTarget,
    ) -> MonsterDamageType {
        let requested = actions.damage_type;
        if requested == MonsterDamageType::Fists {
            return MonsterDamageType::Bludgeon;
        }
        if !matches!(
            requested,
            MonsterDamageType::Auto | MonsterDamageType::Prismatic
        ) {
            return requested;
        }

        let owned = self.pass_equipment(cx.automation);
        let weapon = self.planned_weapon_for(cx, actions, target, &owned);
        let kind = weapon_stance(actions, weapon, &owned);
        // The weapon's own element.
        let element = weapon_element_of(weapon, &owned);

        // Only a wand's element falls back to the caster's training.
        if kind == CombatMode::Magic {
            let skills = cx.automation.skills();
            let cascade = resolve_magic_damage_mode(MonsterDamageType::Auto, &skills);
            if cascade == MonsterDamageType::VoidBasic {
                return cascade;
            }
            if cascade == MonsterDamageType::DrainAuto {
                self.post_attack_warning(
                    cx.automation,
                    "Warning: autoselecting drain as damage type. If you are not a martyr mage, \
                     you probably need to add your weapons to the items tab.",
                );
                return cascade;
            }
        }
        if element != MonsterDamageType::None {
            return element;
        }

        let preferences = self.game_info.damage_preferences(&target.name).to_vec();
        for &preference in &preferences {
            if preference != MonsterDamageType::None
                && self.can_deliver_element(cx, preference, weapon, &owned)
            {
                return preference;
            }
        }
        for unlisted in UNLISTED_ELEMENT_ORDER {
            if preferences.contains(&unlisted)
                || !self.can_deliver_element(cx, unlisted, weapon, &owned)
            {
                continue;
            }
            self.post_attack_warning(
                cx.automation,
                &format!(
                    "Warning: no ammunition available for any of target's possible damage \
                     types! Using unlisted damage type: {}",
                    element_name(unlisted)
                ),
            );
            return unlisted;
        }
        self.post_attack_warning(cx.automation, "No ammunition available at all!");
        MonsterDamageType::None
    }

    /// The weapon the rule fights with: named, else automatic, else the one
    /// in hand.
    fn planned_weapon_for(
        &mut self,
        cx: &mut Ctx<'_>,
        actions: &MonsterRuleActions,
        target: &CombatTarget,
        owned: &[EquipmentItem],
    ) -> u32 {
        if actions.weapon_to_use_raw == 0 {
            return 0;
        }
        let (named, _) = self.resolve_rule_weapon_object_id(cx.settings, actions, owned);
        if named != 0 {
            return named;
        }
        let automatic = self.select_automatic_weapon(cx, owned, actions, target);
        if automatic != 0 {
            return automatic;
        }
        match find_wielded(owned) {
            Some(current)
                if item_use_specifiers::uses_for(cx.settings, current.object_id) & 1 != 0 =>
            {
                current.object_id
            }
            _ => 0,
        }
    }

    /// Whether the weapon can put this element on a monster (a launcher
    /// needs ammunition of it).
    fn can_deliver_element(
        &mut self,
        cx: &mut Ctx<'_>,
        element: MonsterDamageType,
        weapon: u32,
        owned: &[EquipmentItem],
    ) -> bool {
        if let Some(&cached) = self.pass_deliverable.get(&(element, weapon)) {
            return cached;
        }
        let launcher_kind = owned
            .iter()
            .find(|item| item.object_id == weapon)
            .map_or(0, launcher_type);
        let deliverable = if launcher_kind == 0 {
            true
        } else if !self.game_info.is_loaded {
            false
        } else {
            self.select_ammunition(cx, launcher_kind, element)
        };
        self.pass_deliverable.insert((element, weapon), deliverable);
        if !deliverable {
            self.post_attack_warning(
                cx.automation,
                &format!(
                    "Skipping the bow element {}: no ammunition for it.",
                    element_name(element)
                ),
            );
        }
        deliverable
    }

    pub(super) fn resolve_extra_vulnerability(
        &self,
        actions: &MonsterRuleActions,
        target: &CombatTarget,
    ) -> MonsterDamageType {
        match actions.extra_vulnerability {
            MonsterDamageType::Pierce
            | MonsterDamageType::Bludgeon
            | MonsterDamageType::Slash
            | MonsterDamageType::Acid
            | MonsterDamageType::Electric
            | MonsterDamageType::Cold
            | MonsterDamageType::Fire => actions.extra_vulnerability,
            MonsterDamageType::Auto => self
                .game_info
                .damage_preferences(&target.name)
                .first()
                .copied()
                .unwrap_or(MonsterDamageType::None),
            _ => MonsterDamageType::None,
        }
    }

    /// The weapon, off hand and element a monster would be fought with.
    pub(super) fn resolve_wield_plan(
        &mut self,
        cx: &mut Ctx<'_>,
        actions: &MonsterRuleActions,
        target: &CombatTarget,
        inventory: &[InventoryItem],
        equipment: &[EquipmentItem],
    ) -> (u32, u32, MonsterDamageType) {
        let element = self.resolve_attack_element(cx, actions, target);
        let mut weapon = if actions.weapon_to_use_raw == 0 {
            0
        } else {
            self.resolve_rule_weapon_object_id(cx.settings, actions, equipment)
                .0
        };
        if weapon == 0 && actions.weapon_to_use_raw != 0 {
            weapon = self.select_automatic_weapon(cx, equipment, actions, target);
        }
        let offhand = self
            .planned_secondary_for(cx, actions, equipment, weapon, inventory)
            .unwrap_or(0);
        (weapon, offhand, element)
    }
}

fn select_automatic_shield(settings: &CombatSettings, items: &[EquipmentItem]) -> u32 {
    for item in in_profile_order(settings, items) {
        if item.valid_locations == 0x0020_0000
            && !matches!(
                item.object_class,
                ObjectClass::MeleeWeapon | ObjectClass::MissileWeapon | ObjectClass::WandStaffOrb
            )
            && is_profiled(settings, item.object_id, &item.name)
        {
            return item.object_id;
        }
    }
    0
}

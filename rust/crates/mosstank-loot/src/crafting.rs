//! Crafting (`Crafting.cs`): what a consumable is ([`classify`]), which
//! recipe step makes a wanted item from what the packs hold
//! ([`CraftingPlanner`]), and the controller that splits peas, restocks the
//! profile's kits and food, and crafts ammunition the fight asks for
//! ([`CraftingController`]).

use std::collections::HashMap;

use ac_plugin_abi::character::{Skill, Training};
use ac_plugin_abi::items::{InventoryItem, ItemStatus};
use mosstank_automation::Automation;
use mosstank_dotnet::collections::{OrdinalStringSet, fold_ignore_case};
use mosstank_dotnet::text::{casing, ordinal};
use mosstank_formats::gameinfo::{VtankCraftDatabase, VtankCraftRecipe, VtankGameInfoDatabase};
use mosstank_settings::model::{
    CombatSettings, ConsumableCategory, InventorySettings, VitalSettings,
};

use crate::host::status_name;

/// The consumables profile's entry that splits every pea
/// (`CraftingPlanner.AllPeas`).
pub const ALL_PEAS: &str = "[All Peas]";

/// The public flag a healing kit carries.
const HEALING_KIT_PUBLIC_FLAG: u32 = 0x0001_0000;
/// The public flag a lockpick carries.
const LOCKPICK_PUBLIC_FLAG: u32 = 0x0002_0000;
/// The skill whose recipes (arrowheads) ask for headroom over their
/// difficulty: Fletching.
const FLETCHING_SKILL: u32 = 37;
/// How long a split ahead of a craft may stay unanswered, seconds.
const SPLIT_TIMEOUT_SECONDS: f64 = 10.0;
/// How many plans handed to the fight are kept.
const HANDED_OUT_PLANS: usize = 64;

/// What kind of consumable an owned item is (`ConsumableClassifier.Classify`).
#[must_use]
pub fn classify(item: &InventoryItem) -> ConsumableCategory {
    if item.name == ALL_PEAS {
        return ConsumableCategory::AllPeas;
    }
    if item.name.ends_with(" Pea") {
        return ConsumableCategory::Pea;
    }
    if item.public_flags & LOCKPICK_PUBLIC_FLAG != 0 {
        return ConsumableCategory::Lockpick;
    }
    if item.public_flags & HEALING_KIT_PUBLIC_FLAG != 0 {
        return kit_category(&item.name);
    }
    match item.booster_vital {
        2 => ConsumableCategory::HealthFood,
        4 => ConsumableCategory::StaminaFood,
        6 => ConsumableCategory::ManaFood,
        _ => classify_name(&item.name),
    }
}

/// What kind of consumable a name alone says (`ClassifyName`).
#[must_use]
pub fn classify_name(name: &str) -> ConsumableCategory {
    if name == ALL_PEAS {
        return ConsumableCategory::AllPeas;
    }
    if name.ends_with(" Pea") {
        return ConsumableCategory::Pea;
    }
    if name.ends_with(" Kit") {
        kit_category(name)
    } else {
        ConsumableCategory::Other
    }
}

fn kit_category(name: &str) -> ConsumableCategory {
    match name {
        "Medicated Stamina Kit"
        | "Eternal Stamina Kit"
        | "Greater Stamina Kit"
        | "Lesser Stamina Kit" => ConsumableCategory::StaminaKit,
        "Medicated Mana Kit" | "Eternal Mana Kit" | "Greater Mana Kit" | "Lesser Mana Kit" => {
            ConsumableCategory::ManaKit
        }
        _ => ConsumableCategory::HealthKit,
    }
}

/// One recipe step to take (`CraftingPlan`): the recipe, the two items to
/// use one on the other, and what the whole chain is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CraftingPlan {
    /// The recipe.
    pub recipe: VtankCraftRecipe,
    /// The item used.
    pub first_object_id: u32,
    /// The item it is used on (0 while a split has to make it).
    pub second_object_id: u32,
    /// What the chain makes in the end.
    pub desired_result: String,
    /// One unit has to be split off the first stack first (a recipe that
    /// combines two of the same item).
    pub requires_split_first_stack: bool,
    /// Where the split unit goes.
    pub split_container_object_id: u32,
}

impl CraftingPlan {
    fn new(recipe: &VtankCraftRecipe, first: u32, second: u32, desired: &str) -> Self {
        Self {
            recipe: recipe.clone(),
            first_object_id: first,
            second_object_id: second,
            desired_result: desired.to_owned(),
            requires_split_first_stack: false,
            split_container_object_id: 0,
        }
    }
}

/// The recipe planner (`CraftingPlanner`).
#[derive(Debug, Clone, Copy)]
pub struct CraftingPlanner;

/// Stack counts by name, any case (`GroupBy(Name, OrdinalIgnoreCase)`).
fn counts(inventory: &[InventoryItem]) -> HashMap<String, i32> {
    let mut counts: HashMap<String, i32> = HashMap::new();
    for item in inventory {
        *counts.entry(fold_ignore_case(&item.name)).or_default() += item.stack_size.max(1);
    }
    counts
}

fn count_of(counts: &HashMap<String, i32>, name: &str) -> i32 {
    counts.get(&fold_ignore_case(name)).copied().unwrap_or(0)
}

/// The first held stack of `name` (any case), other than `excluded`.
fn find<'i>(
    inventory: &'i [InventoryItem],
    name: &str,
    excluded: u32,
) -> Option<&'i InventoryItem> {
    inventory.iter().find(|item| {
        item.object_id != excluded
            && item.stack_size > 0
            && ordinal::equals_ignore_case(&item.name, name)
    })
}

fn has_required_skill(
    skills: &[Skill],
    required_skill: u32,
    difficulty: i32,
    arrowhead_fletch_difficulty_excess: i32,
) -> bool {
    if required_skill == 0 {
        return true;
    }
    let Some(skill) = skills.iter().find(|skill| skill.skill_id == required_skill) else {
        return false;
    };
    if !matches!(skill.training, Training::Trained | Training::Specialized) {
        return false;
    }
    required_skill != FLETCHING_SKILL
        || i64::from(skill.current)
            >= i64::from(difficulty.max(0)) + i64::from(arrowhead_fletch_difficulty_excess)
}

impl CraftingPlanner {
    /// The next step towards any of `desired_results` the packs hold fewer
    /// than `desired_count` of, by name (any case, alphabetical), or none.
    #[must_use]
    pub fn plan(
        crafts: &VtankCraftDatabase,
        inventory: &[InventoryItem],
        desired_results: &[String],
        skills: &[Skill],
        desired_count: i32,
        arrowhead_fletch_difficulty_excess: i32,
    ) -> Option<CraftingPlan> {
        let counts = counts(inventory);
        let mut wanted: Vec<&String> = Vec::new();
        for name in desired_results {
            if casing::is_white_space_only(name)
                || wanted
                    .iter()
                    .any(|seen| ordinal::equals_ignore_case(seen, name))
            {
                continue;
            }
            wanted.push(name);
        }
        wanted.sort_by(|a, b| ordinal::compare_ignore_case(a, b));
        for desired in wanted {
            if count_of(&counts, desired) >= desired_count.max(1) {
                continue;
            }
            let mut visiting: Vec<String> = Vec::new();
            if let Some(plan) = find_step(
                crafts,
                desired,
                desired,
                inventory,
                skills,
                &mut visiting,
                arrowhead_fletch_difficulty_excess,
            ) {
                return Some(plan);
            }
        }
        None
    }

    /// Splitting a pea the profile names into the component it makes, when
    /// the packs hold fewer than `minimum_component_count` of that
    /// component (`PlanPeaSplit`).
    #[must_use]
    pub fn plan_pea_split(
        crafts: &VtankCraftDatabase,
        inventory: &[InventoryItem],
        consumable_profile: &dyn Fn(&str) -> bool,
        minimum_component_count: i32,
    ) -> Option<CraftingPlan> {
        let minimum = minimum_component_count.max(0);
        if minimum == 0 {
            return None;
        }
        let tool = find(inventory, "Splitting Tool", 0)?;
        let all_peas = consumable_profile(ALL_PEAS);
        let counts = counts(inventory);
        for recipe in crafts.recipes() {
            if recipe.first_item != "Splitting Tool"
                || !recipe.second_item.ends_with(" Pea")
                || (!all_peas && !consumable_profile(&recipe.second_item))
                || count_of(&counts, &recipe.result_item) >= minimum
            {
                continue;
            }
            let Some(pea) = find(inventory, &recipe.second_item, 0) else {
                continue;
            };
            return Some(CraftingPlan::new(
                recipe,
                tool.object_id,
                pea.object_id,
                &recipe.result_item,
            ));
        }
        None
    }
}

/// The step that makes `result` now, or the first step towards one of its
/// ingredients (`FindStep`).
fn find_step(
    crafts: &VtankCraftDatabase,
    result: &str,
    desired_result: &str,
    inventory: &[InventoryItem],
    skills: &[Skill],
    visiting: &mut Vec<String>,
    arrowhead_fletch_difficulty_excess: i32,
) -> Option<CraftingPlan> {
    let key = fold_ignore_case(result);
    if visiting.contains(&key) {
        return None;
    }
    visiting.push(key.clone());
    let found = (|| {
        for recipe in crafts.for_result(result) {
            if !has_required_skill(
                skills,
                recipe.required_skill,
                recipe.difficulty,
                arrowhead_fletch_difficulty_excess,
            ) {
                continue;
            }
            let Some(first) = find(inventory, &recipe.first_item, 0) else {
                if let Some(prerequisite) = find_step(
                    crafts,
                    &recipe.first_item,
                    desired_result,
                    inventory,
                    skills,
                    visiting,
                    arrowhead_fletch_difficulty_excess,
                ) {
                    return Some(prerequisite);
                }
                continue;
            };
            let same_item = ordinal::equals_ignore_case(&recipe.first_item, &recipe.second_item);
            let excluded = if same_item { first.object_id } else { 0 };
            let Some(second) = find(inventory, &recipe.second_item, excluded) else {
                if same_item && first.stack_size >= 2 {
                    let mut plan = CraftingPlan::new(recipe, first.object_id, 0, desired_result);
                    plan.requires_split_first_stack = true;
                    plan.split_container_object_id = first.container_id;
                    return Some(plan);
                }
                if let Some(prerequisite) = find_step(
                    crafts,
                    &recipe.second_item,
                    desired_result,
                    inventory,
                    skills,
                    visiting,
                    arrowhead_fletch_difficulty_excess,
                ) {
                    return Some(prerequisite);
                }
                continue;
            };
            return Some(CraftingPlan::new(
                recipe,
                first.object_id,
                second.object_id,
                desired_result,
            ));
        }
        None
    })();
    visiting.retain(|seen| *seen != key);
    found
}

/// What one call of the crafting works with.
#[allow(missing_debug_implementations)]
pub struct CraftTurn<'a> {
    /// The client.
    pub automation: &'a mut dyn Automation,
    /// The inventory settings.
    pub settings: &'a InventorySettings,
    /// The profile's items and consumables.
    pub profiles: &'a CombatSettings,
    /// The vital settings (the stance gate's peace retries).
    pub vitals: &'a VitalSettings,
}

/// The line said once when the game database has no recipes
/// (`NoRecipesWarning`).
pub const NO_RECIPES_WARNING: &str =
    "AutoCraft has no recipes until the game database is downloaded; /vt gamedb update checks now.";

/// Where the one line about having no recipes goes.
pub type CraftWarning = Box<dyn FnMut(&str)>;

/// Whether the character is ready to craft in peace, dropping to peace
/// when not.
pub type PeaceGate = Box<dyn FnMut(&mut CraftTurn<'_>) -> bool>;

/// The crafting (`CraftingController`).
#[derive(Default)]
#[allow(missing_debug_implementations)]
pub struct CraftingController {
    pending: Option<CraftingPlan>,
    pending_split: Option<CraftingPlan>,
    observed_completion: i64,
    observed_inventory_completion: i64,
    until_scan: f64,
    until_critical_scan: f64,
    until_idle_scan: f64,
    split_elapsed: f64,
    split_acknowledged: bool,
    frame_observed_split_seconds: f64,
    game_info: VtankGameInfoDatabase,
    no_recipes_said: bool,
    status: String,
    /// Plans handed to the fight by handle, the newest last.
    handed_out: Vec<(u32, CraftingPlan)>,
    next_handle: u32,
    /// Where the one line about having no recipes goes (`Warning`).
    pub warning: Option<CraftWarning>,
    /// Whether the character is ready to craft in peace, dropping to peace
    /// when not (`BindPeaceGate`); unbound reads as ready.
    pub ready_to_craft_in_peace: Option<PeaceGate>,
}

impl CraftingController {
    /// A controller reading `game_info`'s recipes.
    #[must_use]
    pub fn new(game_info: VtankGameInfoDatabase) -> Self {
        Self {
            game_info,
            status: "AutoCraft idle".to_owned(),
            ..Self::default()
        }
    }

    /// What it is doing (`Status`).
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// The game database whose craft table every plan reads.
    #[must_use]
    pub fn game_info(&self) -> &VtankGameInfoDatabase {
        &self.game_info
    }

    /// A craft use or the split ahead of it is still unanswered
    /// (`UseInFlight`).
    #[must_use]
    pub fn use_in_flight(&self) -> bool {
        self.pending.is_some() || self.pending_split.is_some()
    }

    /// Takes a newer game database (`ReplaceGameInfo`).
    pub fn replace_game_info(&mut self, game_info: VtankGameInfoDatabase) {
        self.game_info = game_info;
        self.no_recipes_said = false;
    }

    /// Whether there is anything to plan with (`HasRecipes`).
    fn has_recipes(&mut self) -> bool {
        if !self.game_info.crafts.recipes().is_empty() {
            return true;
        }
        "AutoCraft has no recipes".clone_into(&mut self.status);
        if !self.no_recipes_said {
            self.no_recipes_said = true;
            if let Some(warning) = self.warning.as_mut() {
                warning(NO_RECIPES_WARNING);
            }
        }
        false
    }

    fn ready_in_peace(&mut self, turn: &mut CraftTurn<'_>) -> bool {
        match self.ready_to_craft_in_peace.as_mut() {
            Some(ready) => ready(turn),
            None => true,
        }
    }

    fn start_in_peace(&mut self, turn: &mut CraftTurn<'_>, plan: Option<CraftingPlan>) -> bool {
        let Some(next) = plan else {
            return false;
        };
        if !self.ready_in_peace(turn) {
            self.status = format!("Entering peace mode to craft {}", next.recipe.result_item);
            return true;
        }
        self.start(turn.automation, next)
    }

    /// Crafts `count` of `result_name` now (`Request`).
    pub fn request(&mut self, turn: &mut CraftTurn<'_>, result_name: &str, count: i32) -> bool {
        if casing::is_white_space_only(result_name)
            || self.pending.is_some()
            || self.pending_split.is_some()
            || !turn.automation.is_available()
        {
            return false;
        }
        if !turn.automation.items_available() || turn.automation.items_busy() || !self.has_recipes()
        {
            return false;
        }
        let inventory = turn.automation.owned_items();
        let skills = turn.automation.skills();
        let plan = CraftingPlanner::plan(
            &self.game_info.crafts,
            &inventory,
            &[result_name.to_owned()],
            &skills,
            count,
            turn.settings.arrowhead_fletch_difficulty_excess,
        );
        self.start_in_peace(turn, plan)
    }

    /// Whether `count` of `result_name` could be crafted now
    /// (`CanRequest`).
    pub fn can_request(&mut self, turn: &mut CraftTurn<'_>, result_name: &str, count: i32) -> bool {
        self.resolve_request_plan(turn, result_name, count)
            .is_some()
    }

    /// The plan that would craft `count` of `result_name` now
    /// (`ResolveRequestPlan`).
    pub fn resolve_request_plan(
        &mut self,
        turn: &mut CraftTurn<'_>,
        result_name: &str,
        count: i32,
    ) -> Option<CraftingPlan> {
        if casing::is_white_space_only(result_name)
            || !turn.automation.is_available()
            || !turn.automation.items_available()
            || !self.has_recipes()
        {
            return None;
        }
        let inventory = turn.automation.owned_items();
        let skills = turn.automation.skills();
        CraftingPlanner::plan(
            &self.game_info.crafts,
            &inventory,
            &[result_name.to_owned()],
            &skills,
            count,
            turn.settings.arrowhead_fletch_difficulty_excess,
        )
    }

    /// Starts a plan resolved earlier, as it was resolved
    /// (`RequestResolved`).
    pub fn request_resolved(&mut self, turn: &mut CraftTurn<'_>, plan: &CraftingPlan) -> bool {
        if self.pending.is_some() || self.pending_split.is_some() || !turn.automation.is_available()
        {
            return false;
        }
        if !turn.automation.items_available()
            || turn.automation.items_busy()
            || plan.first_object_id == 0
            || plan.second_object_id == 0
        {
            return false;
        }
        if turn.automation.objects_available()
            && (turn.automation.object(plan.first_object_id).is_none()
                || turn.automation.object(plan.second_object_id).is_none())
        {
            return false;
        }
        self.start_in_peace(turn, Some(plan.clone()))
    }

    /// Reads the server's answer to the craft or split this controller
    /// issued, on the host frame rather than the macro pass
    /// (`ObservePendingReceipt`). Nothing is issued here.
    pub fn observe_pending_receipt(
        &mut self,
        automation: &mut dyn Automation,
        settings: &InventorySettings,
        elapsed_seconds: f64,
    ) {
        if !automation.is_available() || (self.pending.is_none() && self.pending_split.is_none()) {
            return;
        }
        self.observe_completion(automation);
        if self.pending_split.is_none() {
            return;
        }
        let elapsed = elapsed_seconds.max(0.0);
        self.frame_observed_split_seconds += elapsed;
        self.observe_split_completion(
            automation,
            elapsed,
            false,
            true,
            settings.scan_interval_seconds,
        );
    }

    /// The critical band's turn (`TickCritical`).
    pub fn tick_critical(
        &mut self,
        turn: &mut CraftTurn<'_>,
        elapsed_seconds: f64,
        can_act: bool,
    ) -> bool {
        let split_elapsed = (elapsed_seconds.max(0.0) - self.frame_observed_split_seconds).max(0.0);
        self.frame_observed_split_seconds = 0.0;
        self.observe_completion(turn.automation);
        if self.observe_split_completion(
            turn.automation,
            split_elapsed,
            can_act,
            true,
            turn.settings.scan_interval_seconds,
        ) {
            return true;
        }
        if let Some(held) = self.held_by_pending(turn) {
            return held;
        }
        if !self.may_scan(turn, can_act) {
            return false;
        }
        self.until_critical_scan -= elapsed_seconds.max(0.0);
        if self.until_critical_scan > 0.0 {
            return false;
        }
        self.until_critical_scan = turn.settings.scan_interval_seconds.max(0.1);
        if !self.has_recipes() {
            return false;
        }
        let inventory = turn.automation.owned_items();
        let mut plan = self.pea_split(turn, &inventory, turn.settings.critical_component_minimum);
        if plan.is_none() {
            plan = self.plan_category_craft(turn, &inventory, false);
        }
        self.start_in_peace(turn, plan)
    }

    /// The normal band's turn (`Tick`).
    pub fn tick(&mut self, turn: &mut CraftTurn<'_>, elapsed_seconds: f64, can_act: bool) -> bool {
        self.observe_completion(turn.automation);
        if self.observe_split_completion(
            turn.automation,
            elapsed_seconds,
            can_act,
            false,
            turn.settings.scan_interval_seconds,
        ) {
            return true;
        }
        if let Some(held) = self.held_by_pending(turn) {
            return held;
        }
        if !self.may_scan(turn, can_act) {
            return false;
        }
        self.until_scan -= elapsed_seconds.max(0.0);
        if self.until_scan > 0.0 {
            return false;
        }
        self.until_scan = turn.settings.scan_interval_seconds.max(0.1);
        if !self.has_recipes() {
            return false;
        }
        let inventory = turn.automation.owned_items();
        let mut plan = self.pea_split(turn, &inventory, turn.settings.normal_component_minimum);
        if plan.is_none() {
            let wanted: Vec<String> = turn
                .profiles
                .consumable_names
                .iter()
                .chain(turn.profiles.combat_item_names.iter())
                .filter(|name| name.as_str() != ALL_PEAS && !name.ends_with(" Pea"))
                .cloned()
                .collect();
            let skills = turn.automation.skills();
            plan = CraftingPlanner::plan(
                &self.game_info.crafts,
                &inventory,
                &wanted,
                &skills,
                1,
                turn.settings.arrowhead_fletch_difficulty_excess,
            );
        }
        if plan.is_none() {
            "AutoCraft idle".clone_into(&mut self.status);
            return false;
        }
        self.start_in_peace(turn, plan)
    }

    /// The idle band's turn (`TickIdle`).
    pub fn tick_idle(
        &mut self,
        turn: &mut CraftTurn<'_>,
        elapsed_seconds: f64,
        can_act: bool,
    ) -> bool {
        self.observe_completion(turn.automation);
        if self.observe_split_completion(
            turn.automation,
            elapsed_seconds,
            can_act,
            false,
            turn.settings.scan_interval_seconds,
        ) {
            return true;
        }
        if let Some(held) = self.held_by_pending(turn) {
            return held;
        }
        if !self.may_scan(turn, can_act) {
            return false;
        }
        self.until_idle_scan -= elapsed_seconds.max(0.0);
        if self.until_idle_scan > 0.0 {
            return false;
        }
        self.until_idle_scan = turn.settings.scan_interval_seconds.max(0.1);
        if !self.has_recipes() {
            return false;
        }
        let inventory = turn.automation.owned_items();
        let mut plan = self.pea_split(turn, &inventory, turn.settings.idle_component_minimum);
        if plan.is_none() {
            plan = self.plan_category_craft(turn, &inventory, true);
        }
        self.start_in_peace(turn, plan)
    }

    /// A craft still in the air holds the turn while the inventory is busy,
    /// and is let go once it is not.
    fn held_by_pending(&mut self, turn: &mut CraftTurn<'_>) -> Option<bool> {
        if self.pending.is_some() {
            if turn.automation.items_busy() {
                return Some(true);
            }
            self.pending = None;
        }
        None
    }

    fn may_scan(&mut self, turn: &mut CraftTurn<'_>, can_act: bool) -> bool {
        can_act
            && turn.settings.auto_craft_items
            && turn.automation.is_available()
            && turn.automation.items_available()
            && !turn.automation.items_busy()
    }

    fn pea_split(
        &self,
        turn: &CraftTurn<'_>,
        inventory: &[InventoryItem],
        minimum: i32,
    ) -> Option<CraftingPlan> {
        if !turn.settings.split_peas {
            return None;
        }
        let peas = pea_consumable_names(turn.profiles);
        CraftingPlanner::plan_pea_split(
            &self.game_info.crafts,
            inventory,
            &|name| peas.contains(name),
            minimum,
        )
    }

    fn plan_category_craft(
        &self,
        turn: &mut CraftTurn<'_>,
        inventory: &[InventoryItem],
        idle_counts: bool,
    ) -> Option<CraftingPlan> {
        let mut names: Vec<&String> = turn.profiles.consumable_names.iter().collect();
        names.sort_by(|a, b| ordinal::compare(a, b));
        let skills = turn.automation.skills();
        for name in names {
            let category = turn
                .profiles
                .consumable_categories
                .get(name.as_str())
                .copied()
                .unwrap_or_else(|| classify_name(name));
            let desired = if idle_counts {
                idle_count(turn.settings, category)
            } else {
                match category {
                    ConsumableCategory::HealthKit
                    | ConsumableCategory::HealthFood
                    | ConsumableCategory::StaminaKit
                    | ConsumableCategory::StaminaFood
                    | ConsumableCategory::ManaKit
                    | ConsumableCategory::ManaFood => 1,
                    _ => 0,
                }
            };
            if desired <= 0 {
                continue;
            }
            if let Some(plan) = CraftingPlanner::plan(
                &self.game_info.crafts,
                inventory,
                std::slice::from_ref(name),
                &skills,
                desired,
                turn.settings.arrowhead_fletch_difficulty_excess,
            ) {
                return Some(plan);
            }
        }
        None
    }

    fn start(&mut self, automation: &mut dyn Automation, next: CraftingPlan) -> bool {
        if next.requires_split_first_stack {
            let completion_before = automation.inventory_completion().revision;
            let split =
                automation.split_stack(next.first_object_id, next.split_container_object_id, 1);
            if !split.accepted() {
                self.status = format!("AutoCraft split waiting: {}", status_name(split.status));
                return split.status == ItemStatus::Busy;
            }
            self.status = format!("Splitting {} for crafting", next.recipe.first_item);
            self.pending_split = Some(next);
            self.observed_inventory_completion = completion_before;
            self.split_elapsed = 0.0;
            self.split_acknowledged = false;
            return true;
        }
        let result = automation.apply_item(next.first_object_id, next.second_object_id);
        if !result.accepted() {
            self.status = format!("AutoCraft waiting: {}", status_name(result.status));
            return result.status == ItemStatus::Busy;
        }
        self.status = format!("Crafting {}", next.recipe.result_item);
        self.pending = Some(next);
        true
    }

    /// Keeps a resolved plan for the fight, which knows it by the handle
    /// returned (the C#'s fight holds the plan object itself).
    pub fn hand_out(&mut self, plan: CraftingPlan) -> u32 {
        self.next_handle = self.next_handle.wrapping_add(1).max(1);
        self.handed_out.push((self.next_handle, plan));
        if self.handed_out.len() > HANDED_OUT_PLANS {
            self.handed_out.remove(0);
        }
        self.next_handle
    }

    /// A plan handed out earlier.
    #[must_use]
    pub fn handed_out(&self, handle: u32) -> Option<CraftingPlan> {
        self.handed_out
            .iter()
            .find(|(kept, _)| *kept == handle)
            .map(|(_, plan)| plan.clone())
    }

    /// Forgets everything in flight (`Reset`).
    pub fn reset(&mut self) {
        self.pending = None;
        self.pending_split = None;
        self.until_scan = 0.0;
        self.until_critical_scan = 0.0;
        self.until_idle_scan = 0.0;
        self.split_elapsed = 0.0;
        self.frame_observed_split_seconds = 0.0;
        self.split_acknowledged = false;
        "AutoCraft idle".clone_into(&mut self.status);
    }

    fn observe_completion(&mut self, automation: &mut dyn Automation) {
        let completion = automation.last_item_completion();
        if completion.revision == 0 || completion.revision == self.observed_completion {
            return;
        }
        self.observed_completion = completion.revision;
        let Some(pending) = self.pending.as_ref() else {
            return;
        };
        if completion.source_id != pending.first_object_id {
            return;
        }
        self.status = if completion.weenie_error == 0 {
            format!("Crafted {}", pending.recipe.result_item)
        } else {
            format!("Craft failed (0x{:X})", completion.weenie_error)
        };
        self.pending = None;
        self.until_scan = 0.0;
        self.until_critical_scan = 0.0;
        self.until_idle_scan = 0.0;
    }

    fn observe_split_completion(
        &mut self,
        automation: &mut dyn Automation,
        elapsed_seconds: f64,
        can_act: bool,
        advance_clock: bool,
        scan_interval: f64,
    ) -> bool {
        let Some(split_plan) = self.pending_split.clone() else {
            return false;
        };
        if advance_clock {
            self.split_elapsed += elapsed_seconds.max(0.0);
        }
        let completion = automation.inventory_completion();
        if completion.revision != 0 && completion.revision != self.observed_inventory_completion {
            self.observed_inventory_completion = completion.revision;
            if completion.source_id == split_plan.first_object_id {
                if completion.weenie_error != 0 {
                    self.status =
                        format!("AutoCraft split failed (0x{:X})", completion.weenie_error);
                    self.clear_pending_split(scan_interval);
                    return true;
                }
                self.split_acknowledged = true;
            }
        }
        if can_act
            && self.split_acknowledged
            && self.try_start_after_split(automation, &split_plan, scan_interval)
        {
            return true;
        }
        if self.split_elapsed < SPLIT_TIMEOUT_SECONDS {
            self.status = if self.split_acknowledged {
                "AutoCraft waiting for split inventory".to_owned()
            } else {
                format!("Splitting {} for crafting", split_plan.recipe.first_item)
            };
            return true;
        }
        "AutoCraft split timed out".clone_into(&mut self.status);
        self.clear_pending_split(scan_interval);
        true
    }

    fn try_start_after_split(
        &mut self,
        automation: &mut dyn Automation,
        split_plan: &CraftingPlan,
        scan_interval: f64,
    ) -> bool {
        let mut inputs: Vec<InventoryItem> = automation
            .owned_items()
            .into_iter()
            .filter(|item| ordinal::equals_ignore_case(&item.name, &split_plan.recipe.first_item))
            .collect();
        inputs.sort_by_key(|item| item.object_id);
        if inputs.len() < 2 {
            return false;
        }
        let ready = CraftingPlan {
            first_object_id: inputs[0].object_id,
            second_object_id: inputs[1].object_id,
            requires_split_first_stack: false,
            ..split_plan.clone()
        };
        self.clear_pending_split(scan_interval);
        self.start(automation, ready)
    }

    fn clear_pending_split(&mut self, scan_interval: f64) {
        self.pending_split = None;
        self.split_elapsed = 0.0;
        self.split_acknowledged = false;
        let interval = scan_interval.max(0.1);
        self.until_scan = interval;
        self.until_critical_scan = interval;
        self.until_idle_scan = interval;
    }
}

/// The idle restock count for a category (`IdleCount`).
fn idle_count(settings: &InventorySettings, category: ConsumableCategory) -> i32 {
    match category {
        ConsumableCategory::HealthKit => settings.idle_health_kit_count,
        ConsumableCategory::StaminaKit => settings.idle_stamina_kit_count,
        ConsumableCategory::ManaKit => settings.idle_mana_kit_count,
        ConsumableCategory::HealthFood => settings.idle_health_food_count,
        ConsumableCategory::StaminaFood => settings.idle_stamina_food_count,
        ConsumableCategory::ManaFood => settings.idle_mana_food_count,
        _ => 0,
    }
}

/// The consumables that may be split as peas (`PeaConsumableNames`): a
/// name-only entry keeps the old split behaviour; an imported entry carries
/// its category, and only a pea category authorizes a split.
#[must_use]
pub fn pea_consumable_names(profiles: &CombatSettings) -> OrdinalStringSet {
    let mut imported: HashMap<&str, ConsumableCategory> = HashMap::new();
    for item in &profiles.imported_assist_items {
        imported.insert(item.name.as_str(), item.category);
    }
    let mut names = OrdinalStringSet::new();
    for name in profiles.consumable_names.iter() {
        let keep = match imported.get(name.as_str()) {
            None => true,
            Some(category) if name == ALL_PEAS => *category == ConsumableCategory::AllPeas,
            Some(category) => *category == ConsumableCategory::Pea,
        };
        if keep {
            names.insert(name.clone());
        }
    }
    names
}

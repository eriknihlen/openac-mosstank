//! Tinkering (`TinkerJobManager`): plans and runs it. It reads what the
//! character owns, picks the cheapest salvage that still clears the odds
//! the player asked for, queues one job per item, and feeds those bags to
//! the server one at a time, saying yes to the client's own "this may fail"
//! prompt and reading the answer back out of chat.
//!
//! A tinker is a round trip: apply, answer the prompt, wait for the line
//! that says it worked or not. Nothing here overlaps two of those, and a
//! failure ends the job on that item, because every bag still queued was
//! chosen against the old attempt count.

use std::collections::HashSet;

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::{Confirmation, ObjectClass};
use mosstank_data::material::{self, Material};
use mosstank_data::tinker::{self, IMBUE_DAMAGE_TYPES, MAXIMUM_ATTEMPTS};
use openac_plugin_compat::number::{self, MidpointRounding};
use openac_plugin_compat::regex::{self, DotNetRegex, RegexMatch, RegexOptions};

use crate::chat::{self, tools};
use crate::commands::UbCall;
use crate::host::UbHost;

/// The confirmation type the client raises for a crafting attempt that may
/// fail; the only one this answers.
const CRAFT_CONFIRMATION_TYPE: i32 = 5;
/// A full bag of salvage; a part-used bag cannot be applied.
const FULL_SALVAGE_STRUCTURE: i32 = 100;
/// How long one attempt may go unanswered before the run gives up.
const ATTEMPT_TIMEOUT_SECONDS: f64 = 20.0;
/// The lowest odds the bulk imbue queues anything at.
const BULK_IMBUE_MINIMUM_CHANCE: f64 = 0.30;
/// Bonuses to a weapon's damage ceiling the estimate counts.
const DAMAGE_CEILING_CANTRIPS: [(u32, i32); 4] = [
    (2598, 2),  // Minor Blood Thirst
    (2586, 4),  // Major Blood Thirst
    (4661, 7),  // Epic Blood Thirst
    (6089, 10), // Legendary Blood Thirst
];
/// The flat allowance added on top of the weapon's own ceiling.
const DAMAGE_CEILING_ALLOWANCE: i32 = 24;
/// The item classes that can be tinkered at all.
const TINKERABLE_CLASSES: [ObjectClass; 6] = [
    ObjectClass::Armor,
    ObjectClass::Clothing,
    ObjectClass::Jewelry,
    ObjectClass::MeleeWeapon,
    ObjectClass::MissileWeapon,
    ObjectClass::WandStaffOrb,
];
/// The only clothing slots worth tinkering: head, hands and feet.
const TINKERABLE_CLOTHING_SLOTS: [u32; 3] = [1, 32, 256];

/// The salvage choice that is a decision taken per attempt: whichever of
/// granite and iron is worth more on the weapon as it then stands.
pub const GRANITE_IRON: &str = "Granite/Iron";

/// The row that says the character carries Charmed Smith.
const CHARMED_SMITH_SETTING: &str = "AutoTinker.CharmedSmith";

/// The choice of salvage offered for an item of this kind.
#[must_use]
pub fn usable_salvage(object_class: ObjectClass) -> &'static [&'static str] {
    match object_class {
        ObjectClass::MissileWeapon => &["Mahogany", "Brass", "Gold", "Moonstone", "Linen", "Pine"],
        ObjectClass::MeleeWeapon => &[
            GRANITE_IRON,
            "Brass",
            "Granite",
            "Iron",
            "Gold",
            "Moonstone",
            "Linen",
            "Pine",
        ],
        ObjectClass::WandStaffOrb => &[
            "Brass",
            "Green Garnet",
            "Gold",
            "Moonstone",
            "Linen",
            "Pine",
            "Opal",
        ],
        ObjectClass::Armor | ObjectClass::Clothing => {
            &["Steel", "Moonstone", "Linen", "Pine", "Gold"]
        }
        ObjectClass::Jewelry => &["Gold", "Moonstone", "Linen", "Pine"],
        _ => &[],
    }
}

/// One item and the bags queued against it, soonest first (`TinkerJob`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TinkerJob {
    /// The item.
    pub item_object_id: u32,
    /// The bags still to apply; one leaves the list as it is handed over.
    pub salvage_to_apply: Vec<u32>,
    /// The bags already handed over, in order.
    pub salvage_applied: Vec<u32>,
}

/// One line of the Tinker or Imbue list (`TinkerListRow`).
#[derive(Debug, Clone, PartialEq)]
pub struct TinkerListRow {
    /// Which attempt on the item, from one.
    pub number: i32,
    /// The item.
    pub item_object_id: u32,
    /// Its name.
    pub item_name: String,
    /// The bag spent.
    pub salvage_object_id: u32,
    /// The bag's name and workmanship.
    pub salvage_name: String,
    /// The odds as a percentage.
    pub success_text: String,
    /// The odds.
    pub success_chance: f64,
    /// `None` until answered, then whether it came off.
    pub succeeded: Option<bool>,
}

/// The job a run is working on. It starts as the head of the queue and is
/// the same job there while it stays queued; a new plan (a Populate) drops
/// it from the queue, and the run carries on with it all the same until it
/// finishes.
#[derive(Debug, Clone)]
enum ActiveJob {
    /// The head of the queue, `jobs[0]`.
    Queued,
    /// No longer in the queue.
    Detached(TinkerJob),
}

/// The tinkering plan and run.
#[derive(Debug, Clone)]
pub struct TinkerJobManager {
    jobs: Vec<TinkerJob>,
    tinker_rows: Vec<TinkerListRow>,
    imbue_rows: Vec<TinkerListRow>,
    salvage_bags: Vec<InventoryItem>,
    tinkerable_items: Vec<InventoryItem>,
    salvage_used: HashSet<u32>,
    wanted_materials: Vec<i32>,
    possible_materials: Vec<i32>,
    owned: Vec<InventoryItem>,
    running: bool,
    active: Option<ActiveJob>,
    attempts_taken: i32,
    last_skill: i32,
    last_skill_id: u32,
    pending_salvage_id: u32,
    pending_salvage_name: String,
    pending_salvage_workmanship: f64,
    pending_item_name: String,
    awaiting_outcome: bool,
    wait_elapsed: f64,
    chat_sequence: u64,
}

impl Default for TinkerJobManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TinkerJobManager {
    /// Nothing planned.
    #[must_use]
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
            tinker_rows: Vec::new(),
            imbue_rows: Vec::new(),
            salvage_bags: Vec::new(),
            tinkerable_items: Vec::new(),
            salvage_used: HashSet::new(),
            wanted_materials: Vec::new(),
            possible_materials: Vec::new(),
            owned: Vec::new(),
            running: false,
            active: None,
            attempts_taken: -1,
            last_skill: 0,
            last_skill_id: 0,
            pending_salvage_id: 0,
            pending_salvage_name: String::new(),
            pending_salvage_workmanship: 0.0,
            pending_item_name: String::new(),
            awaiting_outcome: false,
            wait_elapsed: 0.0,
            chat_sequence: 0,
        }
    }

    /// The jobs queued, in the order they run.
    #[must_use]
    pub fn jobs(&self) -> &[TinkerJob] {
        &self.jobs
    }

    /// The Tinker page's list.
    #[must_use]
    pub fn tinker_rows(&self) -> &[TinkerListRow] {
        &self.tinker_rows
    }

    /// The Imbue page's list.
    #[must_use]
    pub fn imbue_rows(&self) -> &[TinkerListRow] {
        &self.imbue_rows
    }

    /// True while a queued job is being worked through.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Re-reads what the character owns: the full bags of salvage, cheapest
    /// first, and every item that could still take a tinker.
    pub fn scan_inventory(&mut self, host: &mut dyn UbHost) {
        self.owned = host.owned_items();
        self.salvage_bags.clear();
        self.tinkerable_items.clear();
        for item in &self.owned {
            if item.object_class == ObjectClass::Salvage && item.structure == FULL_SALVAGE_STRUCTURE
            {
                self.salvage_bags.push(item.clone());
            }
            if can_be_tinkered(item) {
                self.tinkerable_items.push(item.clone());
            }
        }
        self.salvage_bags
            .sort_by(|a, b| a.salvage_workmanship.total_cmp(&b.salvage_workmanship));
        self.tinkerable_items
            .sort_by(|a, b| a.salvage_workmanship.total_cmp(&b.salvage_workmanship));
    }

    /// The item, when the character owns it (as last read).
    #[must_use]
    pub fn try_owned(&self, object_id: u32) -> Option<&InventoryItem> {
        self.owned.iter().find(|item| item.object_id == object_id)
    }

    // ---- planning ----------------------------------------------------------

    /// Plans ordinary tinkers on one item: every attempt from the one it is
    /// on up to `maximum_attempts`, each spending the cheapest bag that
    /// still beats `minimum_percent`.
    pub fn populate_tinker_list(
        &mut self,
        call: &mut UbCall<'_>,
        item_object_id: u32,
        salvage_choice: &str,
        maximum_attempts: i32,
        minimum_percent: f64,
    ) {
        self.clear_plan();
        self.tinker_rows.clear();
        self.scan_inventory(call.host);
        let Some(item) = self.try_owned(item_object_id).cloned() else {
            call.write_ub(&chat::tool(tools::AUTO_TINKER, "Select an item first"));
            return;
        };
        if !can_be_tinkered(&item) {
            call.write_ub("Item cannot be tinkered");
            return;
        }
        for name in usable_salvage(item.object_class) {
            self.add_possible_material(name);
        }
        self.build_wanted_materials(&item, salvage_choice, maximum_attempts);
        let mut rows = std::mem::take(&mut self.tinker_rows);
        self.build_job(call, &item, minimum_percent, maximum_attempts, &mut rows);
        self.tinker_rows = rows;
    }

    /// Plans a rend of one damage type with one salvage across everything
    /// that can still take it: one attempt per item.
    pub fn populate_imbue_list(
        &mut self,
        call: &mut UbCall<'_>,
        damage_type_name: &str,
        salvage_name: &str,
    ) {
        self.clear_plan();
        self.imbue_rows.clear();
        self.scan_inventory(call.host);
        let salvage_material = material::id(salvage_name);
        if salvage_material == 0 {
            call.write_ub("no salvage matches...  quitting");
            return;
        }
        self.add_possible_material(salvage_name);

        let damage_bit = tinker::damage_bit(damage_type_name);
        let allowed = tinker::imbue_targets(salvage_material);
        let mut number = 0;
        for item in self.tinkerable_items.clone() {
            if !can_be_imbued(&item) || !allowed.contains(&item.object_class) {
                continue;
            }
            if item.damage_type != damage_bit && item.wand_elemental_damage_type != damage_bit {
                continue;
            }
            self.wanted_materials.clear();
            self.wanted_materials.push(salvage_material);
            // A rend is one attempt, whatever has already gone into the
            // item: it is normally the last thing put in.
            let Some((bag, chance)) =
                self.try_take_salvage(call, &item, item.num_times_tinkered, 0.0)
            else {
                continue;
            };
            self.salvage_used.insert(bag.object_id);
            self.jobs.push(TinkerJob {
                item_object_id: item.object_id,
                salvage_to_apply: vec![bag.object_id],
                salvage_applied: Vec::new(),
            });
            number += 1;
            self.imbue_rows.push(row(number, &item, &bag, chance));
        }
    }

    /// Plans one rend for every weapon that can still take one, with the
    /// salvage that matches what it already strikes with; anything below
    /// thirty percent is left out.
    pub fn populate_rend_all(&mut self, call: &mut UbCall<'_>) {
        self.clear_plan();
        self.imbue_rows.clear();
        self.scan_inventory(call.host);
        self.possible_materials
            .extend(material::named().map(Material::id));

        let mut number = 0;
        for item in self.tinkerable_items.clone() {
            if !can_be_imbued(&item) {
                continue;
            }
            let damage_bit = match item.object_class {
                ObjectClass::WandStaffOrb => item.wand_elemental_damage_type,
                ObjectClass::MeleeWeapon | ObjectClass::MissileWeapon => item.damage_type,
                _ => -1,
            };
            if damage_bit == -1 {
                continue;
            }
            let salvage_material = default_salvage(damage_bit);
            if salvage_material == -1 {
                continue;
            }
            let Some(bag) = self.try_next_bag(salvage_material) else {
                continue;
            };
            let chance = self.chance_for(call, &bag, &item, item.num_times_tinkered);
            if chance < BULK_IMBUE_MINIMUM_CHANCE {
                continue;
            }
            self.salvage_used.insert(bag.object_id);
            self.jobs.push(TinkerJob {
                item_object_id: item.object_id,
                salvage_to_apply: vec![bag.object_id],
                salvage_applied: Vec::new(),
            });
            number += 1;
            self.imbue_rows.push(row(number, &item, &bag, chance));
        }
    }

    /// The materials wanted, one per remaining attempt; granite and iron are
    /// decided attempt by attempt against a running damage estimate.
    fn build_wanted_materials(
        &mut self,
        item: &InventoryItem,
        salvage_choice: &str,
        maximum_attempts: i32,
    ) {
        self.wanted_materials.clear();
        let mut maximum_damage = damage_ceiling(item);
        let mut variance = item.damage_variance;
        let remaining = maximum_attempts - item.num_times_tinkered;
        for _ in 1..=remaining {
            if salvage_choice != GRANITE_IRON {
                self.wanted_materials.push(material::id(salvage_choice));
                continue;
            }
            if tinker::iron_beats_granite(maximum_damage, variance) {
                maximum_damage += 1.0;
                self.wanted_materials.push(Material::Iron.id());
            } else {
                variance *= 0.8;
                self.wanted_materials.push(Material::Granite.id());
            }
        }
    }

    /// Turns the wanted materials into one job on this item.
    fn build_job(
        &mut self,
        call: &mut UbCall<'_>,
        item: &InventoryItem,
        minimum_percent: f64,
        maximum_attempts: i32,
        rows: &mut Vec<TinkerListRow>,
    ) {
        let mut queued = Vec::new();
        let mut attempts_taken = item.num_times_tinkered;
        let remaining = maximum_attempts - item.num_times_tinkered;
        for _ in 1..=remaining {
            let Some((bag, chance)) =
                self.try_take_salvage(call, item, attempts_taken, minimum_percent)
            else {
                continue;
            };
            self.salvage_used.insert(bag.object_id);
            queued.push(bag.object_id);
            rows.push(row(attempts_taken + 1, item, &bag, chance));
            attempts_taken += 1;
        }
        if queued.is_empty() {
            return;
        }
        self.jobs.push(TinkerJob {
            item_object_id: item.object_id,
            salvage_to_apply: queued,
            salvage_applied: Vec::new(),
        });
    }

    /// The cheapest bag of a still-wanted material that clears the floor,
    /// and its odds; taking one strikes its material off the want list.
    fn try_take_salvage(
        &mut self,
        call: &mut UbCall<'_>,
        item: &InventoryItem,
        attempts_taken: i32,
        minimum_percent: f64,
    ) -> Option<(InventoryItem, f64)> {
        for candidate in self.salvage_bags.clone() {
            let material = candidate.material_type.cast_signed();
            if !self.wanted_materials.contains(&material)
                || self.salvage_used.contains(&candidate.object_id)
            {
                continue;
            }
            let chance = self.chance_for(call, &candidate, item, attempts_taken);
            if chance <= minimum_percent / 100.0 {
                continue;
            }
            if let Some(at) = self.wanted_materials.iter().position(|m| *m == material) {
                self.wanted_materials.remove(at);
            }
            return Some((candidate, chance));
        }
        None
    }

    /// The cheapest unspoken-for bag of this material, when the plan allows
    /// the material at all.
    fn try_next_bag(&self, material: i32) -> Option<InventoryItem> {
        self.salvage_bags
            .iter()
            .find(|candidate| {
                self.possible_materials
                    .contains(&candidate.material_type.cast_signed())
                    && !self.salvage_used.contains(&candidate.object_id)
                    && candidate.material_type == material.cast_unsigned()
            })
            .cloned()
    }

    /// The cheapest full bag of this material, as the packs were last read.
    #[must_use]
    pub fn try_cheapest_bag(&self, material: i32) -> Option<&InventoryItem> {
        self.salvage_bags
            .iter()
            .find(|candidate| candidate.material_type == material.cast_unsigned())
    }

    fn add_possible_material(&mut self, name: &str) {
        if name == GRANITE_IRON {
            self.possible_materials.push(Material::Granite.id());
            self.possible_materials.push(Material::Iron.id());
            return;
        }
        let material = material::id(name);
        if material != 0 {
            self.possible_materials.push(material);
        }
    }

    /// Clears the plan, leaving the lists on the page alone.
    pub fn clear_plan(&mut self) {
        // A run in progress keeps its job, which is no longer queued.
        if matches!(self.active, Some(ActiveJob::Queued)) && !self.jobs.is_empty() {
            self.active = Some(ActiveJob::Detached(self.jobs.remove(0)));
        }
        self.jobs.clear();
        self.salvage_used.clear();
        self.wanted_materials.clear();
        self.possible_materials.clear();
    }

    /// What one bag is worth on one item at that attempt count, with the
    /// character's skill in whichever tinkering skill the material calls
    /// for, and Charmed Smith as the page says.
    pub fn chance_for(
        &self,
        call: &mut UbCall<'_>,
        bag: &InventoryItem,
        item: &InventoryItem,
        attempts_taken: i32,
    ) -> f64 {
        let material = bag.material_type.cast_signed();
        let skill = skill(call.host, tinker::skill_id(material));
        tinker::success_chance(
            material,
            bag.salvage_workmanship,
            f64::from(item.workmanship),
            attempts_taken,
            skill,
            call.flag(CHARMED_SMITH_SETTING),
        )
    }

    // ---- running -------------------------------------------------------------

    /// Starts working through the queued jobs; says so and does nothing when
    /// the queue is empty or a run is going.
    pub fn start(&mut self, call: &mut UbCall<'_>) {
        if self.running {
            call.write_ub(&chat::tool(tools::AUTO_TINKER, "already running"));
            return;
        }
        if self.jobs.is_empty() {
            call.write_ub("i'm out of jobs");
            return;
        }
        self.running = true;
        self.chat_sequence = latest_chat_sequence(call.host);
        self.start_next_job(call);
    }

    /// Stops the run and drops the queue; the lists stay as they are.
    pub fn stop(&mut self) {
        self.running = false;
        self.active = None;
        self.awaiting_outcome = false;
        self.wait_elapsed = 0.0;
        self.attempts_taken = -1;
        self.last_skill = 0;
        self.last_skill_id = 0;
        self.pending_salvage_id = 0;
        self.pending_salvage_name.clear();
        self.pending_salvage_workmanship = 0.0;
        self.pending_item_name.clear();
        self.clear_plan();
    }

    /// One frame: read chat for the answer to the attempt in flight, or
    /// start the next attempt when there is none.
    pub fn on_tick(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64) {
        if !self.running {
            return;
        }
        if !self.awaiting_outcome {
            self.do_next_tink(call);
            return;
        }
        self.wait_elapsed += elapsed_seconds;
        self.observe_chat(call);
        if !self.awaiting_outcome || self.wait_elapsed < ATTEMPT_TIMEOUT_SECONDS {
            return;
        }
        call.write_ub("AutoTinker: no word back on the last tink... stopping tinkering.");
        self.stop();
    }

    /// Says yes to the client's craft prompt while an attempt waits on one;
    /// every other prompt is left for whoever raised it.
    pub fn on_confirmation(&mut self, call: &mut UbCall<'_>, confirmation: &Confirmation) {
        if !self.running || !self.awaiting_outcome {
            return;
        }
        if confirmation.kind != CRAFT_CONFIRMATION_TYPE {
            return;
        }
        call.write_ub(&format!(
            "AutoTinker: Clicking Yes on {}",
            confirmation.text
        ));
        call.host.answer_dialog(confirmation.context_id, true);
    }

    fn start_next_job(&mut self, call: &mut UbCall<'_>) {
        if self.jobs.is_empty() {
            call.write_ub("i'm out of jobs");
            self.stop();
            return;
        }
        self.active = Some(ActiveJob::Queued);
        self.attempts_taken = -1;
        self.do_next_tink(call);
    }

    /// The job the run is working on, queued or not.
    fn active_job(&self) -> Option<&TinkerJob> {
        match &self.active {
            Some(ActiveJob::Queued) => self.jobs.first(),
            Some(ActiveJob::Detached(job)) => Some(job),
            None => None,
        }
    }

    /// The job the run is working on, to hand a bag over from.
    fn active_job_mut(&mut self) -> Option<&mut TinkerJob> {
        match &mut self.active {
            Some(ActiveJob::Queued) => self.jobs.first_mut(),
            Some(ActiveJob::Detached(job)) => Some(job),
            None => None,
        }
    }

    /// Hands the next bag to the server: re-reads the item, checks the skill
    /// has not dropped, selects the bag as the client would and applies it.
    fn do_next_tink(&mut self, call: &mut UbCall<'_>) {
        let Some(job) = self.active_job() else {
            self.stop();
            return;
        };
        let Some(&next_salvage) = job.salvage_to_apply.first() else {
            self.finish_job(call);
            return;
        };
        let item_object_id = job.item_object_id;

        self.owned = call.host.owned_items();
        let Some(item) = self.try_owned(item_object_id).cloned() else {
            call.write_ub("AutoTinker: the item is gone... stopping tinkering.");
            self.stop();
            return;
        };
        self.attempts_taken = if self.attempts_taken == -1 {
            item.num_times_tinkered
        } else {
            self.attempts_taken + 1
        };

        let Some(bag) = self.try_owned(next_salvage).cloned() else {
            call.write_ub("AutoTinker: the salvage is gone... stopping tinkering.");
            self.stop();
            return;
        };

        let skill_id = tinker::skill_id(bag.material_type.cast_signed());
        let skill = skill(call.host, skill_id);
        if skill < self.last_skill && skill_id == self.last_skill_id {
            call.write_ub("tinkering skill decreased... stopping tinkering.");
            self.finish_job(call);
            return;
        }
        self.last_skill_id = skill_id;
        self.last_skill = skill;

        self.pending_item_name = tinkered_item_name(&item);
        self.pending_salvage_id = bag.object_id;
        self.pending_salvage_name = salvage_name(&bag);
        self.pending_salvage_workmanship = bag.salvage_workmanship;

        call.host.select(next_salvage);
        if call.host.selection().selected != Some(next_salvage) {
            call.write_ub("AutoTinker: could not select the salvage... stopping tinkering.");
            self.stop();
            return;
        }

        if let Some(job) = self.active_job_mut() {
            job.salvage_to_apply.remove(0);
            job.salvage_applied.push(next_salvage);
        }
        self.awaiting_outcome = true;
        self.wait_elapsed = 0.0;
        call.host.apply_item(next_salvage, item_object_id);
    }

    fn finish_job(&mut self, call: &mut UbCall<'_>) {
        // A job a new plan dropped from the queue is not in it to remove.
        if matches!(self.active.take(), Some(ActiveJob::Queued)) && !self.jobs.is_empty() {
            self.jobs.remove(0);
        }
        self.awaiting_outcome = false;
        if !self.jobs.is_empty() {
            self.start_next_job(call);
            return;
        }
        call.write_ub("Done tinkering");
        self.stop();
    }

    /// Reads the lines sent since the last look for the one that answers
    /// the attempt in flight. One line answers one attempt: the rest of the
    /// batch keeps its sequence and is read on the next look, so a second
    /// copy of the same words cannot answer the attempt it just started.
    fn observe_chat(&mut self, call: &mut UbCall<'_>) {
        let character = call
            .host
            .character()
            .map(|character| character.name)
            .unwrap_or_default();
        let success = success_pattern(&character);
        let failure = failure_pattern(&character);
        let foolproof = foolproof_pattern();

        for message in call.host.chat_captured(self.chat_sequence) {
            self.chat_sequence = self.chat_sequence.max(message.sequence);
            let text = message.text.trim();
            if !self.awaiting_outcome {
                continue;
            }
            if let Some(found) = matched(&success, text)
                && self.matches_pending_attempt(&found)
            {
                self.record_outcome(call, true);
                break;
            }
            if let Some(found) = matched(&foolproof, text)
                && self.matches_foolproof(&found)
            {
                self.record_outcome(call, true);
                break;
            }
            if let Some(found) = matched(&failure, text)
                && self.matches_pending_attempt(&found)
            {
                self.record_outcome(call, false);
                break;
            }
        }
    }

    fn record_outcome(&mut self, call: &mut UbCall<'_>, succeeded: bool) {
        self.mark_row(self.pending_salvage_id, succeeded);
        self.awaiting_outcome = false;
        self.wait_elapsed = 0.0;
        if !succeeded {
            // The attempt is spent either way, and every bag still queued
            // was chosen against the attempt count just used up.
            self.finish_job(call);
            return;
        }
        if self
            .active_job()
            .is_some_and(|job| !job.salvage_to_apply.is_empty())
        {
            self.do_next_tink(call);
            return;
        }
        self.finish_job(call);
    }

    fn mark_row(&mut self, salvage_object_id: u32, succeeded: bool) {
        for row in self
            .tinker_rows
            .iter_mut()
            .chain(self.imbue_rows.iter_mut())
        {
            if row.salvage_object_id == salvage_object_id {
                row.succeeded = Some(succeeded);
            }
        }
    }

    fn matches_pending_attempt(&self, found: &RegexMatch<'_, '_>) -> bool {
        let workmanship = number::format_f64(self.pending_salvage_workmanship, "0.00")
            .expect("a number format the kit prints");
        found.group("item").value == self.pending_item_name
            && found.group("workmanship").value == workmanship
            && found.group("salvage").value.replace(" Salvage", "").trim()
                == self.pending_salvage_name.trim()
    }

    fn matches_foolproof(&self, found: &RegexMatch<'_, '_>) -> bool {
        mosstank_dotnet::text::ordinal::equals_ignore_case(
            &strip_foolproof(found.group("salvage").value),
            &strip_foolproof(&self.pending_salvage_name),
        )
    }

    /// The queue in words, as `/ub getjob` prints it: a line naming each
    /// item, then one per bag waiting on it.
    #[must_use]
    pub fn describe_jobs(&self) -> Vec<String> {
        if self.jobs.is_empty() {
            return vec!["i'm out of jobs".to_owned()];
        }
        let mut lines = Vec::new();
        for job in &self.jobs {
            lines.push(format!(
                "Target item: {}",
                self.owned_name(job.item_object_id)
            ));
            for salvage in &job.salvage_to_apply {
                lines.push(format!("salvage: {} {salvage}", self.owned_name(*salvage)));
            }
        }
        lines
    }

    fn owned_name(&self, object_id: u32) -> String {
        self.try_owned(object_id)
            .map_or_else(|| object_id.to_string(), |item| item.name.clone())
    }
}

/// Whether this item can take a tinker at all: the right kind, a
/// workmanship, attempts left; clothing in one of the three armoured slots
/// with armour of its own.
#[must_use]
pub fn can_be_tinkered(item: &InventoryItem) -> bool {
    if !TINKERABLE_CLASSES.contains(&item.object_class) {
        return false;
    }
    if item.salvage_workmanship <= 0.0 {
        return false;
    }
    if item.num_times_tinkered >= MAXIMUM_ATTEMPTS {
        return false;
    }
    if item.object_class == ObjectClass::Clothing
        && (!TINKERABLE_CLOTHING_SLOTS.contains(&item.valid_locations) || item.armor_level == 0)
    {
        return false;
    }
    true
}

/// Whether this item can still take an imbue: a workmanship, no imbue
/// already, attempts left.
#[must_use]
pub fn can_be_imbued(item: &InventoryItem) -> bool {
    item.workmanship > 0.0 && item.imbued_effect < 1 && item.num_times_tinkered < MAXIMUM_ATTEMPTS
}

/// The damage ceiling the granite-against-iron comparison works from: the
/// weapon's own, plus its blood-thirst cantrips, plus the flat allowance.
#[must_use]
pub fn damage_ceiling(item: &InventoryItem) -> f64 {
    let mut ceiling = f64::from(item.max_damage);
    for (spell_id, bonus) in DAMAGE_CEILING_CANTRIPS {
        if item.appraised_spell_ids.contains(&spell_id) {
            ceiling += f64::from(bonus);
        }
    }
    ceiling + f64::from(DAMAGE_CEILING_ALLOWANCE)
}

/// How many granite and iron an untouched melee weapon should take, and its
/// damage per second at the end, each choice made against the estimate the
/// previous ones left.
#[must_use]
pub fn best_granite_iron(item: &InventoryItem) -> (i32, i32, f64) {
    let mut maximum_damage = damage_ceiling(item);
    let mut variance = item.damage_variance;
    let (mut granite, mut iron, mut final_damage) = (0, 0, 0.0);
    for _ in item.num_times_tinkered + 1..=MAXIMUM_ATTEMPTS {
        if tinker::iron_beats_granite(maximum_damage, variance) {
            final_damage = tinker::damage_per_second(maximum_damage + 1.0, variance);
            maximum_damage += 1.0;
            iron += 1;
        } else {
            final_damage = tinker::damage_per_second(maximum_damage, variance * 0.8);
            variance *= 0.8;
            granite += 1;
        }
    }
    (granite, iron, final_damage)
}

/// The salvage that rends this damage type; -1 when none does.
#[must_use]
pub fn default_salvage(damage_type: i32) -> i32 {
    IMBUE_DAMAGE_TYPES
        .iter()
        .find(|(_, bit, _)| *bit == damage_type)
        .map_or(-1, |(_, _, material)| material.id())
}

/// The item's name as the crafting line says it: the material once, in
/// front.
#[must_use]
pub fn tinkered_item_name(item: &InventoryItem) -> String {
    let material = material::name(item.material_type.cast_signed());
    if material.is_empty() {
        return item.name.clone();
    }
    let bare = if item.name.contains(material) {
        item.name.replace(material, "")
    } else {
        item.name.clone()
    };
    format!("{} {}", material.trim(), bare.trim())
}

/// The bag's name with the salvage wording stripped off.
#[must_use]
pub fn salvage_name(bag: &InventoryItem) -> String {
    bag.name
        .replace("Salvaged ", "")
        .replace("Salvage ", "")
        .replace("Salvage", "")
        .replace("(100)", "")
        .trim()
        .to_owned()
}

fn row(number: i32, item: &InventoryItem, bag: &InventoryItem, chance: f64) -> TinkerListRow {
    let rounded = number::round_digits(bag.salvage_workmanship, 2, MidpointRounding::AwayFromZero);
    TinkerListRow {
        number,
        item_object_id: item.object_id,
        item_name: item.name.clone(),
        salvage_object_id: bag.object_id,
        salvage_name: format!(
            "{}({}) ",
            salvage_name(bag),
            mosstank_dotnet::double_to_string(rounded)
        ),
        success_text: number::format_f64(chance, "P").expect("a number format the kit prints"),
        success_chance: chance,
        succeeded: None,
    }
}

fn strip_foolproof(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let mut out = String::with_capacity(name.len());
    let mut rest = name;
    let mut lower_rest = lower.as_str();
    while let Some(at) = lower_rest.find("foolproof") {
        out.push_str(&rest[..at]);
        rest = &rest[at + "foolproof".len()..];
        lower_rest = &lower_rest[at + "foolproof".len()..];
    }
    out.push_str(rest);
    out.trim().to_owned()
}

fn matched<'r, 't>(regex: &'r DotNetRegex, text: &'t str) -> Option<RegexMatch<'r, 't>> {
    // A line past the step limit is no answer to the attempt.
    regex.find(text).ok().flatten()
}

fn success_pattern(character: &str) -> DotNetRegex {
    pattern(&format!(
        r"^{} successfully applies the (?<salvage>[\w\s\-]+?)(\sSalvage.*)(\s?\(100\))?\s\(workmanship (?<workmanship>\d+\.\d+)\) to the (?<item>[\w\s'\-]+)\.$",
        regex::escape(character)
    ))
}

fn failure_pattern(character: &str) -> DotNetRegex {
    pattern(&format!(
        r"^{} fails to apply the (?<salvage>[\w\s\-]+?)(\sSalvage.*)(\s?\(100\))?\s\(workmanship (?<workmanship>\d+\.\d+)\) to the (?<item>[\w\s'\-]+)\..*$",
        regex::escape(character)
    ))
}

fn foolproof_pattern() -> DotNetRegex {
    pattern(r"^You apply the (?<salvage>.*)\.$")
}

fn pattern(source: &str) -> DotNetRegex {
    DotNetRegex::new(source, RegexOptions::CULTURE_INVARIANT).expect("the pattern compiles")
}

fn latest_chat_sequence(host: &mut dyn UbHost) -> u64 {
    host.chat_captured(0)
        .iter()
        .map(|message| message.sequence)
        .max()
        .unwrap_or(0)
}

/// The character's current level in a skill; 0 for no skill.
fn skill(host: &mut dyn UbHost, skill_id: u32) -> i32 {
    if skill_id == 0 {
        return 0;
    }
    host.skills()
        .into_iter()
        .find(|skill| skill.skill_id == skill_id)
        .map_or(0, |skill| skill.current.cast_signed())
}

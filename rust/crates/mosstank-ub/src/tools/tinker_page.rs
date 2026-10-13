//! The Tinker page (`MossTankPanel.Tinker`): an item, a salvage, a floor
//! under the odds and the run that spends one bag after another on it, with
//! the bulk rend underneath; and the three commands, `/ub autotinker`,
//! `/ub getjob` and `/ub tinkcalc`. The page owns only what is typed into
//! it: the plan and the run live in [`TinkerJobManager`], and every number
//! on the page is read back out of it.

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::ObjectClass;
use mosstank_data::material::{self, Material};
use mosstank_data::tinker::{self, IMBUE_DAMAGE_TYPES, IMBUE_SALVAGE, MAXIMUM_ATTEMPTS};
use mosstank_settings::ub::UbSettingValue;
use openac_plugin_compat::number::{self, NumberStyles};

use super::tinker::{
    GRANITE_IRON, TinkerJobManager, TinkerListRow, best_granite_iron, can_be_tinkered,
    damage_ceiling, salvage_name, usable_salvage,
};
use crate::catalog::UbCatalog;
use crate::commands::UbCall;

/// The colour an ordinary row draws in.
pub const TINKER_ROW_COLOR: u32 = 0x00E8_DEC3;
/// The colour a row that came off draws in.
pub const TINKER_SUCCESS_COLOR: u32 = 0x0038_C038;
/// The colour a row that failed draws in.
pub const TINKER_FAILURE_COLOR: u32 = 0x00C0_3838;

const MIN_PERCENTAGE: &str = "AutoTinker.MinPercentage";
const MAX_TINKS: &str = "AutoTinker.MaxTinks";

/// The page and the manager behind it.
#[derive(Debug, Clone)]
pub struct TinkerPage {
    manager: TinkerJobManager,
    item_id: u32,
    item_label: String,
    notice: String,
    salvage_names: Vec<String>,
    salvage_choice: String,
    minimum_percent_draft: Option<String>,
    tinker_row_index: i32,
    imbue_salvage_names: Vec<String>,
    imbue_salvage_choice: String,
    imbue_damage_type_choice: String,
    imbue_row_index: i32,
}

impl Default for TinkerPage {
    fn default() -> Self {
        Self::new()
    }
}

impl TinkerPage {
    /// The page as it opens: every material, then Granite/Iron, offered;
    /// the first damage type and the salvage that rends it chosen.
    #[must_use]
    pub fn new() -> Self {
        let mut salvage: Vec<String> = material::named()
            .filter_map(Material::name)
            .map(str::to_owned)
            .collect();
        salvage.sort();
        salvage.push(GRANITE_IRON.to_owned());
        let salvage_choice = salvage[0].clone();

        let mut imbue: Vec<String> = material::named()
            .filter(|material| tinker::salvage_kind(material.id()) == IMBUE_SALVAGE)
            .filter_map(Material::name)
            .map(str::to_owned)
            .collect();
        imbue.sort();

        let damage_type = IMBUE_DAMAGE_TYPES[0].0.to_owned();
        let imbue_salvage_choice = tinker::default_salvage_name(&damage_type).to_owned();
        Self {
            manager: TinkerJobManager::new(),
            item_id: 0,
            item_label: "[None]".to_owned(),
            notice: "Select an item in your inventory, then add it here.".to_owned(),
            salvage_names: salvage,
            salvage_choice,
            minimum_percent_draft: None,
            tinker_row_index: -1,
            imbue_salvage_names: imbue,
            imbue_salvage_choice,
            imbue_damage_type_choice: damage_type,
            imbue_row_index: -1,
        }
    }

    /// The plan and the run.
    #[must_use]
    pub fn manager(&self) -> &TinkerJobManager {
        &self.manager
    }

    /// The plan and the run, for the frame
    /// ([`TinkerJobManager::on_tick`]) and the client's prompts
    /// ([`TinkerJobManager::on_confirmation`]).
    pub fn manager_mut(&mut self) -> &mut TinkerJobManager {
        &mut self.manager
    }

    // ---- the Tinker half -----------------------------------------------------

    /// The item the run will tinker, or "[None]".
    #[must_use]
    pub fn item_name(&self) -> &str {
        &self.item_label
    }

    /// What the page last had to say.
    #[must_use]
    pub fn notice(&self) -> &str {
        &self.notice
    }

    /// Takes what the client has selected as the item to tinker, and narrows
    /// the salvage choice to what is worth putting on it.
    pub fn add_selected_item(&mut self, call: &mut UbCall<'_>) {
        self.manager.stop();
        self.manager.clear_plan();
        self.manager.scan_inventory(call.host);
        let selected = call.host.selection().selected.unwrap_or(0);
        let item = if selected == 0 {
            None
        } else {
            self.manager.try_owned(selected).cloned()
        };
        let Some(item) = item else {
            "Select an item first".clone_into(&mut self.notice);
            return;
        };
        if !can_be_tinkered(&item) {
            self.item_id = 0;
            "[None]".clone_into(&mut self.item_label);
            "Item cannot be tinkered".clone_into(&mut self.notice);
            return;
        }
        self.item_id = item.object_id;
        self.item_label.clone_from(&item.name);
        self.notice = format!(
            "{} has taken {} of {MAXIMUM_ATTEMPTS} tinks.",
            item.name, item.num_times_tinkered
        );

        let usable = usable_salvage(item.object_class);
        if usable.is_empty() {
            return;
        }
        self.salvage_names = usable.iter().map(|name| (*name).to_owned()).collect();
        if !self.salvage_names.contains(&self.salvage_choice) {
            self.salvage_choice = self.salvage_names[0].clone();
        }
    }

    /// The salvage the run will spend, as the choice offers them.
    #[must_use]
    pub fn salvage_names(&self) -> &[String] {
        &self.salvage_names
    }

    /// Which salvage is chosen.
    #[must_use]
    pub fn selected_salvage(&self) -> &str {
        &self.salvage_choice
    }

    /// Chooses the salvage a populate plans with.
    pub fn select_salvage(&mut self, name: &str) {
        if self.salvage_names.iter().any(|known| known == name) {
            name.clone_into(&mut self.salvage_choice);
        }
    }

    /// The floor under an attempt's odds, as typed (the setting as it read
    /// when the page first showed it, until something is typed).
    pub fn minimum_percent_text(&mut self, catalog: &dyn UbCatalog) -> String {
        self.minimum_percent_draft
            .get_or_insert_with(|| number::to_string_f32(catalog.value(MIN_PERCENTAGE).as_single()))
            .clone()
    }

    /// Takes a new floor: an empty field is no floor, as the reference has
    /// it; anything that is not a number is left alone.
    pub fn set_minimum_percent_text(&mut self, call: &mut UbCall<'_>, text: &str) {
        self.minimum_percent_draft = Some(text.to_owned());
        let percent = if text.trim().is_empty() {
            0.0
        } else {
            match number::parse_f32(text, NumberStyles::FLOAT) {
                Some(percent) => percent,
                None => return,
            }
        };
        let row = call.settings.require(MIN_PERCENTAGE);
        call.settings
            .set(&row, UbSettingValue::from_single(percent));
    }

    /// The attempt counts the run may stop at.
    #[must_use]
    pub fn max_tinks_choices() -> Vec<String> {
        (1..=MAXIMUM_ATTEMPTS)
            .map(|count| count.to_string())
            .collect()
    }

    /// Which attempt the run stops at.
    #[must_use]
    pub fn selected_max_tinks(catalog: &dyn UbCatalog) -> String {
        maximum_attempts(catalog).to_string()
    }

    /// Changes where the run stops; the plan was built against the old
    /// number, so it is dropped, as the reference drops it.
    pub fn select_max_tinks(&mut self, call: &mut UbCall<'_>, text: &str) {
        let Some(count) = number::parse_i32(text, NumberStyles::INTEGER) else {
            return;
        };
        let count = count.clamp(1, MAXIMUM_ATTEMPTS);
        let row = call.settings.require(MAX_TINKS);
        call.settings.set(&row, UbSettingValue::from_int(count));
        call.write_ub(&format!("Updated max tink value to {count}"));
        self.manager.stop();
        "Populate the list again for the new stop-at count.".clone_into(&mut self.notice);
    }

    /// Plans the run on the item the page holds.
    pub fn populate_tinker_list(&mut self, call: &mut UbCall<'_>) {
        if self.item_id == 0 {
            "select an item first".clone_into(&mut self.notice);
            return;
        }
        let maximum = maximum_attempts(call.settings);
        let minimum = f64::from(call.settings.value(MIN_PERCENTAGE).as_single());
        let choice = self.salvage_choice.clone();
        self.manager
            .populate_tinker_list(call, self.item_id, &choice, maximum, minimum);
        self.tinker_row_index = -1;
        self.notice = format!("{} tink(s) planned.", self.manager.tinker_rows().len());
    }

    /// Starts the planned run.
    pub fn start_tinker(&mut self, call: &mut UbCall<'_>) {
        if self.item_id == 0 {
            "select an item first".clone_into(&mut self.notice);
            return;
        }
        self.manager.start(call);
    }

    /// Stops the run and drops what is left of the plan.
    pub fn stop(&mut self) {
        self.manager.stop();
        "Stopped.".clone_into(&mut self.notice);
    }

    /// Which attempt each Tinker row is.
    #[must_use]
    pub fn tinker_row_numbers(&self) -> Vec<String> {
        numbers(self.manager.tinker_rows())
    }

    /// The item each Tinker row spends its bag on.
    #[must_use]
    pub fn tinker_row_items(&self) -> Vec<String> {
        items(self.manager.tinker_rows())
    }

    /// The bag each Tinker row spends, with its workmanship.
    #[must_use]
    pub fn tinker_row_salvage(&self) -> Vec<String> {
        salvage(self.manager.tinker_rows())
    }

    /// The odds each Tinker row was planned at.
    #[must_use]
    pub fn tinker_row_chances(&self) -> Vec<String> {
        chances(self.manager.tinker_rows())
    }

    /// Green for a row that came off, red for one that did not.
    #[must_use]
    pub fn tinker_row_colors(&self) -> Vec<u32> {
        self.manager.tinker_rows().iter().map(row_color).collect()
    }

    /// Which Tinker row is highlighted.
    #[must_use]
    pub fn selected_tinker_row_index(&self) -> i32 {
        self.tinker_row_index
    }

    /// Selects the row's item in the client, as the reference does.
    pub fn select_tinker_row(&mut self, call: &mut UbCall<'_>, index: i32) {
        let Some(row) = usize::try_from(index)
            .ok()
            .and_then(|at| self.manager.tinker_rows().get(at))
        else {
            return;
        };
        let item = row.item_object_id;
        self.tinker_row_index = index;
        call.host.select(item);
    }

    // ---- the Imbue half ------------------------------------------------------

    /// The damage types a rend can be planned for.
    #[must_use]
    pub fn imbue_damage_type_names() -> Vec<String> {
        IMBUE_DAMAGE_TYPES
            .iter()
            .map(|(name, ..)| (*name).to_owned())
            .collect()
    }

    /// Which damage type is chosen.
    #[must_use]
    pub fn selected_imbue_damage_type(&self) -> &str {
        &self.imbue_damage_type_choice
    }

    /// Chooses the damage type, and with it the salvage that rends it.
    pub fn select_imbue_damage_type(&mut self, call: &mut UbCall<'_>, name: &str) {
        if !IMBUE_DAMAGE_TYPES.iter().any(|(known, ..)| *known == name) {
            return;
        }
        name.clone_into(&mut self.imbue_damage_type_choice);
        let salvage = tinker::default_salvage_name(name);
        if self
            .imbue_salvage_names
            .iter()
            .any(|known| known == salvage)
        {
            salvage.clone_into(&mut self.imbue_salvage_choice);
        }
        self.refresh_imbue_plan(call);
    }

    /// The salvages that imbue.
    #[must_use]
    pub fn imbue_salvage_names(&self) -> &[String] {
        &self.imbue_salvage_names
    }

    /// Which of them is chosen.
    #[must_use]
    pub fn selected_imbue_salvage(&self) -> &str {
        &self.imbue_salvage_choice
    }

    /// Chooses the salvage a rend uses.
    pub fn select_imbue_salvage(&mut self, call: &mut UbCall<'_>, name: &str) {
        if !self.imbue_salvage_names.iter().any(|known| known == name) {
            return;
        }
        name.clone_into(&mut self.imbue_salvage_choice);
        self.refresh_imbue_plan(call);
    }

    /// Re-reads the packs and re-plans the chosen rend.
    pub fn refresh_imbue_list(&mut self, call: &mut UbCall<'_>) {
        self.refresh_imbue_plan(call);
    }

    /// Plans one rend for every weapon that can still take one.
    pub fn rend_all(&mut self, call: &mut UbCall<'_>) {
        self.manager.populate_rend_all(call);
        self.imbue_row_index = -1;
        self.notice = format!("{} rend(s) planned.", self.manager.imbue_rows().len());
    }

    /// Starts the planned rends.
    pub fn start_imbue(&mut self, call: &mut UbCall<'_>) {
        self.manager.start(call);
    }

    /// Which rend each Imbue row is.
    #[must_use]
    pub fn imbue_row_numbers(&self) -> Vec<String> {
        numbers(self.manager.imbue_rows())
    }

    /// The item each Imbue row rends.
    #[must_use]
    pub fn imbue_row_items(&self) -> Vec<String> {
        items(self.manager.imbue_rows())
    }

    /// The bag each Imbue row spends.
    #[must_use]
    pub fn imbue_row_salvage(&self) -> Vec<String> {
        salvage(self.manager.imbue_rows())
    }

    /// The odds each Imbue row was planned at.
    #[must_use]
    pub fn imbue_row_chances(&self) -> Vec<String> {
        chances(self.manager.imbue_rows())
    }

    /// Green for a rend that came off, red for one that did not.
    #[must_use]
    pub fn imbue_row_colors(&self) -> Vec<u32> {
        self.manager.imbue_rows().iter().map(row_color).collect()
    }

    /// Which Imbue row is highlighted.
    #[must_use]
    pub fn selected_imbue_row_index(&self) -> i32 {
        self.imbue_row_index
    }

    /// Selects the row's item in the client.
    pub fn select_imbue_row(&mut self, call: &mut UbCall<'_>, index: i32) {
        let Some(row) = usize::try_from(index)
            .ok()
            .and_then(|at| self.manager.imbue_rows().get(at))
        else {
            return;
        };
        let item = row.item_object_id;
        self.imbue_row_index = index;
        call.host.select(item);
    }

    fn refresh_imbue_plan(&mut self, call: &mut UbCall<'_>) {
        let damage_type = self.imbue_damage_type_choice.clone();
        let salvage = self.imbue_salvage_choice.clone();
        self.manager
            .populate_imbue_list(call, &damage_type, &salvage);
        self.imbue_row_index = -1;
        self.notice = format!("{} rend(s) planned.", self.manager.imbue_rows().len());
    }

    // ---- the commands --------------------------------------------------------

    /// `/ub autotinker`: works through whatever the page has planned.
    pub fn auto_tinker(&mut self, call: &mut UbCall<'_>) {
        self.manager.start(call);
    }

    /// `/ub getjob`: the queue as it stands.
    pub fn print_jobs(&mut self, call: &mut UbCall<'_>) {
        self.manager.scan_inventory(call.host);
        for line in self.manager.describe_jobs() {
            call.write_ub(&line);
        }
    }

    /// `/ub tinkcalc`: what the chosen salvage is worth on the item in hand,
    /// and for a melee weapon how the attempts split between granite and
    /// iron.
    pub fn print_calculation(&mut self, call: &mut UbCall<'_>) {
        self.manager.scan_inventory(call.host);
        let Some(item) = self.calculation_item(call) else {
            call.write_ub("Nothing selected");
            return;
        };

        match self.calculation_salvage(call, &item) {
            Some(bag) => {
                let chance = self
                    .manager
                    .chance_for(call, &bag, &item, item.num_times_tinkered);
                let line = format!(
                    "{}: Applying ws {} {} {} to {} {} with a successChance of {}",
                    item.num_times_tinkered + 1,
                    mosstank_dotnet::double_to_string(bag.salvage_workmanship),
                    salvage_name(&bag),
                    bag.object_id,
                    item.name,
                    item.object_id,
                    number::format_f64(chance, "P").expect("a number format the kit prints")
                );
                call.write_ub(&line);
            }
            None => call.write_ub("no salvage matches...  quitting"),
        }

        if item.object_class != ObjectClass::MeleeWeapon {
            call.write_ub_error("tinkcalc only currently works with melee weapons");
            return;
        }
        let (granite, iron, final_damage) = best_granite_iron(&item);
        call.write_ub(&format!(
            "{}: Final max damage: {}, {granite} granite, {iron} iron",
            item.name,
            number::format_f64(final_damage, "N2").expect("a number format the kit prints")
        ));
    }

    /// The item a calculation runs against: the page's, otherwise whatever
    /// is selected that is not salvage.
    fn calculation_item(&self, call: &mut UbCall<'_>) -> Option<InventoryItem> {
        if self.item_id != 0
            && let Some(item) = self.manager.try_owned(self.item_id)
        {
            return Some(item.clone());
        }
        let selected = call.host.selection().selected.unwrap_or(0);
        if selected == 0 {
            return None;
        }
        self.manager
            .try_owned(selected)
            .filter(|item| item.object_class != ObjectClass::Salvage)
            .cloned()
    }

    /// The salvage a calculation runs with: the selected bag when one is
    /// selected, else the cheapest bag of the chosen material (Granite/Iron
    /// resolving to whichever is worth more now).
    fn calculation_salvage(
        &self,
        call: &mut UbCall<'_>,
        item: &InventoryItem,
    ) -> Option<InventoryItem> {
        let selected = call.host.selection().selected.unwrap_or(0);
        if selected != 0
            && let Some(bag) = self.manager.try_owned(selected)
            && bag.object_class == ObjectClass::Salvage
        {
            return Some(bag.clone());
        }
        let material = if self.salvage_choice == GRANITE_IRON {
            if tinker::iron_beats_granite(damage_ceiling(item), item.damage_variance) {
                Material::Iron.id()
            } else {
                Material::Granite.id()
            }
        } else {
            material::id(&self.salvage_choice)
        };
        if material == 0 {
            return None;
        }
        self.manager.try_cheapest_bag(material).cloned()
    }
}

fn maximum_attempts(catalog: &dyn UbCatalog) -> i32 {
    catalog
        .value(MAX_TINKS)
        .as_int32()
        .clamp(1, MAXIMUM_ATTEMPTS)
}

fn numbers(rows: &[TinkerListRow]) -> Vec<String> {
    rows.iter().map(|row| row.number.to_string()).collect()
}

fn items(rows: &[TinkerListRow]) -> Vec<String> {
    rows.iter().map(|row| row.item_name.clone()).collect()
}

fn salvage(rows: &[TinkerListRow]) -> Vec<String> {
    rows.iter().map(|row| row.salvage_name.clone()).collect()
}

fn chances(rows: &[TinkerListRow]) -> Vec<String> {
    rows.iter().map(|row| row.success_text.clone()).collect()
}

fn row_color(row: &TinkerListRow) -> u32 {
    match row.succeeded {
        Some(true) => TINKER_SUCCESS_COLOR,
        Some(false) => TINKER_FAILURE_COLOR,
        None => TINKER_ROW_COLOR,
    }
}

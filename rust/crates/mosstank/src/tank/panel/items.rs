//! The Items, Consumables and Buffs pages and the buff picker window, and
//! the item admission behind their Add buttons (`MossTankPanel`'s item
//! editors, `MossTankPanel.ItemAdmission`, `ItemEnchantDefaults`).

use ac_plugin_abi::items::{InventoryItem, ItemResult, ItemStatus};
use ac_plugin_abi::ui::UiValue;
use ac_plugin_abi::world::{ItemProperties, ObjectClass};
use mosstank_automation::Automation;
use mosstank_combat::pets::is_pet_device;
use mosstank_loot::crafting::{ALL_PEAS, classify, classify_name};
use mosstank_settings::model::{AssistItem, BuffItemEnchantRow, BuffedItemKey, ConsumableCategory};

use super::{Arg, b, clamp_row, f, i, row, t, tl};
use mosstank_meta::scheduler::MacroLogChannel;

use crate::tank::TankCx;

/// The Hands column's cycle (`HandednessCycle`).
const HANDEDNESS_CYCLE: [&str; 3] = ["Auto", "1-Handed", "2-Handed"];
/// How the excluded scarab list is joined (`ExcludedComponentDelimiter`).
const EXCLUDED_COMPONENT_DELIMITER: &str = "; ";

/// Where an item is worn, by kind (`ItemEnchantDefaults`).
const MELEE_WEAPON: u32 = 0x0010_0000;
const SHIELD: u32 = 0x0020_0000;
const MISSILE_WEAPON: u32 = 0x0040_0000;
const WAND: u32 = 0x0100_0000;
const TWO_HANDED: u32 = 0x0200_0000;

const MELEE_AURAS: [&str; 4] = [
    "Aura of Defender Self I",
    "Aura of Blood Drinker Self I",
    "Aura of Swift Killer Self I",
    "Aura of Heart Seeker Self I",
];
const MISSILE_AURAS: [&str; 3] = [
    "Aura of Defender Self I",
    "Aura of Blood Drinker Self I",
    "Aura of Swift Killer Self I",
];
const WAND_AURAS: [&str; 3] = [
    "Aura of Defender Self I",
    "Aura of Hermetic Link Self I",
    "Aura of Spirit Drinker Self I",
];
/// The banes in the damage element order, then Impenetrability.
const SHIELD_BANES: [&str; 8] = [
    "Piercing Bane I",
    "Bludgeon Bane I",
    "Blade Bane I",
    "Acid Bane I",
    "Lightning Bane I",
    "Frost Bane I",
    "Flame Bane I",
    "Impenetrability I",
];

/// The defaults an item worn in `valid_locations` gets (`DefaultsFor`).
fn defaults_for(valid_locations: u32) -> &'static [&'static str] {
    match valid_locations {
        MELEE_WEAPON | TWO_HANDED => &MELEE_AURAS,
        MISSILE_WEAPON => &MISSILE_AURAS,
        WAND => &WAND_AURAS,
        SHIELD => &SHIELD_BANES,
        _ => &[],
    }
}

/// `ItemEnchantDefaults.IsProfileEligible`.
fn is_profile_eligible(item: &InventoryItem, pet_device: bool) -> bool {
    pet_device || !defaults_for(item.valid_locations).is_empty()
}

/// `ItemEnchantDefaults.Rows`: a pet device and a no-buffs item have none.
fn default_rows(item: &InventoryItem, no_buffs: bool, pet_device: bool) -> &'static [&'static str] {
    if pet_device || no_buffs {
        &[]
    } else {
        defaults_for(item.valid_locations)
    }
}

/// The Items and Consumables pages' rows as last built
/// (`RefreshItemEditors`).
#[derive(Debug, Clone, Default)]
pub(crate) struct ItemRows {
    pub(crate) rows: Vec<String>,
    pub(crate) base_names: Vec<String>,
    pub(crate) object_ids: Vec<Option<u32>>,
    pub(crate) enchant_rows: Vec<Option<BuffItemEnchantRow>>,
    pub(crate) hands: Vec<String>,
    pub(crate) consumables: Vec<String>,
    pub(crate) excluded_components: Vec<String>,
    pub(crate) excluded_component_icons: Vec<u32>,
}

/// An item add waiting for its appraisal (`PendingProfileAddition`).
#[derive(Debug, Clone)]
struct PendingAddition {
    object_id: u32,
    consumable: bool,
    no_buffs: bool,
    profile: String,
    elapsed: f64,
}

/// What the item admission keeps between frames
/// (`MossTankPanel.ItemAdmission`).
#[derive(Debug, Clone, Default)]
pub(crate) struct ItemAdmission {
    pending: Option<PendingAddition>,
    retries: Vec<(u32, f64)>,
    notes: Vec<(u32, String)>,
    time: f64,
    next_assessment: f64,
    next_scan: f64,
}

impl ItemAdmission {
    fn retry_at(&self, object_id: u32) -> Option<f64> {
        self.retries
            .iter()
            .find(|(id, _)| *id == object_id)
            .map(|(_, at)| *at)
    }

    fn set_retry(&mut self, object_id: u32, at: f64) {
        self.retries.retain(|(id, _)| *id != object_id);
        self.retries.push((object_id, at));
    }
}

/// `ProfileText`: a heading and up to eight names, alphabetically.
fn profile_text(heading: &str, names: &mosstank_dotnet::collections::OrdinalStringSet) -> String {
    let mut entries: Vec<&String> = names.iter().collect();
    entries.sort_by(|a, b| mosstank_dotnet::text::ordinal::compare(a, b));
    entries.truncate(8);
    if entries.is_empty() {
        format!("{heading}: [None]")
    } else {
        let joined: Vec<&str> = entries.iter().map(|name| name.as_str()).collect();
        format!("{heading}: {}", joined.join("  |  "))
    }
}

/// `ParseExcludedComponents`.
fn parse_excluded_components(setting: &str) -> Vec<String> {
    if mosstank_dotnet::text::casing::is_white_space_only(setting) {
        return Vec::new();
    }
    setting
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect()
}

fn int_property(properties: &ItemProperties, id: u32) -> Option<i32> {
    properties.int(id)
}

fn float_property(properties: &ItemProperties, id: u32) -> Option<f64> {
    properties
        .floats
        .iter()
        .find(|(key, _)| *key == id)
        .map(|(_, value)| *value)
}

/// `ProfileItemAdmission.TryConsumable`: what kind of consumable an
/// assessed item is, if it is one the Consumables page takes.
fn try_consumable(
    item: &InventoryItem,
    properties: &ItemProperties,
    automation: &mut dyn Automation,
) -> Option<ConsumableCategory> {
    let kind = item.object_class;
    if matches!(
        kind,
        ObjectClass::Food | ObjectClass::Gem | ObjectClass::HealingKit | ObjectClass::ManaStone
    ) {
        if kind == ObjectClass::ManaStone {
            #[allow(clippy::float_cmp)]
            let category = if float_property(properties, 137).unwrap_or(0.0) != 1.0 {
                ConsumableCategory::ManaStone
            } else if int_property(properties, 108).unwrap_or(-1) <= 0
                && int_property(properties, 107).unwrap_or(0) > 0
            {
                ConsumableCategory::ManaSource
            } else {
                ConsumableCategory::Other
            };
            return (category != ConsumableCategory::Other).then_some(category);
        }
        if kind == ObjectClass::HealingKit {
            return Some(classify(item));
        }
        let category = match item.booster_vital {
            2 => ConsumableCategory::HealthFood,
            4 => ConsumableCategory::StaminaFood,
            6 => ConsumableCategory::ManaFood,
            _ => ConsumableCategory::Other,
        };
        if category != ConsumableCategory::Other {
            return Some(category);
        }
        return try_buff(item, automation);
    }
    let category =
        if kind == ObjectClass::MissileWeapon && item.ammo_type == 0 && item.name.contains("Phial")
        {
            ConsumableCategory::Grenade
        } else if kind == ObjectClass::SpellComponent && !item.name.ends_with(" Pea") {
            ConsumableCategory::SplitComponent
        } else if kind == ObjectClass::Lockpick {
            ConsumableCategory::Lockpick
        } else if kind == ObjectClass::Misc {
            return try_buff(item, automation);
        } else {
            ConsumableCategory::Other
        };
    (category != ConsumableCategory::Other).then_some(category)
}

/// `TryBuff`: an item whose spell lasts five minutes or more, and is no
/// war spell, is a buff consumable.
fn try_buff(item: &InventoryItem, automation: &mut dyn Automation) -> Option<ConsumableCategory> {
    let spell_id = *item.appraised_spell_ids.first()?;
    let spell = automation.spells().try_get(spell_id)?;
    if spell.school == 32 || spell.duration_seconds < 300.0 {
        return None;
    }
    Some(ConsumableCategory::BuffConsumable)
}

pub(super) fn value(cx: &mut TankCx<'_>, name: &str) -> Option<UiValue> {
    let panel = &cx.core.panel;
    let rows = &panel.items;
    match name {
        "ItemRows" | "ItemNameColumn" => tl(rows.rows.clone()),
        "ItemHandsColumn" => tl(rows.hands.clone()),
        "SelectedItemRowIndex" => i(panel.selected_item_row),
        "ConsumableRows" => tl(rows.consumables.clone()),
        "SelectedConsumableRowIndex" => i(panel.selected_consumable_row),
        "ExcludedComponentRows" => tl(rows.excluded_components.clone()),
        "ExcludedComponentIcons" => Some(UiValue::UintList(rows.excluded_component_icons.clone())),
        "SelectedExcludedComponentIndex" => i(panel.selected_excluded_component_row),
        "ProfileNotice" => t(panel.profile_notice.clone()),
        "ItemProfileText" => t(profile_text(
            "Weapons / Wands / Shields / Pets",
            &cx.core.settings.combat.combat_item_names,
        )),
        "ConsumableProfileText" => t(profile_text(
            "Gems / Food / Kits / Potions / Charges / Grenades / Lockpicks",
            &cx.core.settings.combat.consumable_names,
        )),
        "RefillWornManaEnabled" => b(cx.core.settings.inventory.refill_worn_mana),
        #[allow(clippy::cast_precision_loss)]
        "RefillWornManaValue" => {
            f(cx.core.settings.inventory.refill_worn_mana_percent as f32 / 100.0)
        }
        "RefillWornManaText" => t(format!(
            "Refill worn mana below {}%",
            cx.core.settings.inventory.refill_worn_mana_percent
        )),
        "ExtraBuffRows" => {
            let buffs = &cx.core.settings.buffs;
            let (names, ids) = (
                buffs.extra_buff_spell_names.clone(),
                buffs.extra_buff_spell_ids.clone(),
            );
            tl(cx.buff_rows(&names, &ids))
        }
        "BlacklistedBuffFamilyRows" => {
            let buffs = &cx.core.settings.buffs;
            let (names, ids) = (
                buffs.blacklisted_buff_family_names.clone(),
                buffs.anti_extra_buff_spell_ids.clone(),
            );
            tl(cx.buff_rows(&names, &ids))
        }
        "SelectedExtraBuffIndex" => i(panel.selected_extra_buff_row),
        "SelectedBlacklistedBuffIndex" => i(panel.selected_blacklisted_buff_row),
        "BuffPickerVisible" => b(panel.buff_picker_visible),
        "BuffPickerSearchText" => t(panel.buff_picker_search_text.clone()),
        "BuffPickerRows" => tl(cx.buff_picker_rows()),
        "SelectedBuffPickerIndex" => i(panel.selected_buff_picker_row),
        _ => None,
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn act(cx: &mut TankCx<'_>, name: &str, arg: &Arg) -> bool {
    match name {
        "AddSelectedItem" => cx.begin_profile_item_addition(false, false),
        "AddSelectedItemNoBuffs" => cx.begin_profile_item_addition(false, true),
        "AddSelectedConsumable" => cx.begin_profile_item_addition(true, false),
        "AddAllPeas" => cx.add_all_peas(),
        "SelectItemRow" => {
            let count = cx.core.panel.items.rows.len();
            cx.core.panel.selected_item_row = clamp_row(arg.int(), count);
        }
        "SelectConsumableRow" => {
            let count = cx.core.panel.items.consumables.len();
            cx.core.panel.selected_consumable_row = clamp_row(arg.int(), count);
            cx.remove_selected_consumable();
        }
        "RemoveSelectedItem" => {
            let count = cx.core.panel.items.base_names.len();
            if count == 0 {
                cx.announce_profile_notice("The Items profile is empty.");
                return true;
            }
            let selected = clamp_row(cx.core.panel.selected_item_row, count);
            cx.delete_item_row_at(selected);
        }
        "RemoveSelectedConsumable" => cx.remove_selected_consumable(),
        "DeleteItemRowAt" => cx.delete_item_row_at(arg.int()),
        "CycleItemHandsAt" => {
            let Some(at) = row(arg.int(), cx.core.panel.items.base_names.len()) else {
                return true;
            };
            let name = cx.core.panel.items.base_names[at].clone();
            let next = (cx.handedness_index(&name) + 1) % HANDEDNESS_CYCLE.len();
            let handedness = &mut cx.core.panel.item_handedness;
            handedness.retain(|(item, _)| *item != name);
            handedness.push((name, next));
            cx.refresh_item_editors();
        }
        "SelectExcludedComponentRow" => {
            let count = cx.core.panel.items.excluded_components.len();
            cx.core.panel.selected_excluded_component_row = clamp_row(arg.int(), count);
        }
        "DeleteExcludedComponentAt" => {
            let mut updated = cx.core.panel.items.excluded_components.clone();
            let Some(at) = row(arg.int(), updated.len()) else {
                return true;
            };
            let removed = updated.remove(at);
            cx.set_excluded_components(&updated);
            cx.core.panel.profile_notice = format!("Removed {removed}.");
            cx.refresh_item_editors();
            cx.core.save_profile(cx.client);
        }
        "AddSelectedComponent" => {
            let Some(item) = cx.selected_inventory_item() else {
                "Select an owned spell component first."
                    .clone_into(&mut cx.core.panel.profile_notice);
                return true;
            };
            let current = cx.core.panel.items.excluded_components.clone();
            if current
                .iter()
                .any(|entry| mosstank_dotnet::text::ordinal::equals_ignore_case(entry, &item.name))
            {
                "That blacklist entry is already in the list."
                    .clone_into(&mut cx.core.panel.profile_notice);
                return true;
            }
            let mut updated = current;
            updated.push(item.name.clone());
            cx.set_excluded_components(&updated);
            cx.core.panel.profile_notice = format!("Added {}.", item.name);
            cx.refresh_item_editors();
            cx.core.save_profile(cx.client);
        }
        "DeleteExtraBuffAt" => {
            cx.core.panel.selected_extra_buff_row = arg.int();
            cx.delete_buff_row(false, arg.int());
            cx.core.save_profile(cx.client);
        }
        "DeleteBlacklistedBuffFamilyAt" => {
            cx.core.panel.selected_blacklisted_buff_row = arg.int();
            cx.delete_buff_row(true, arg.int());
            cx.core.save_profile(cx.client);
        }
        "ShowExtraBuffPicker" => cx.show_buff_picker(false),
        "ShowBlacklistedBuffPicker" => cx.show_buff_picker(true),
        "SetBuffPickerSearchText" => {
            arg.text()
                .clone_into(&mut cx.core.panel.buff_picker_search_text);
        }
        "SelectBuffPickerRow" => cx.core.panel.selected_buff_picker_row = arg.int(),
        "PickBuffAt" => cx.pick_buff_at(arg.int()),
        "HideBuffPicker" => cx.core.panel.buff_picker_visible = false,
        "ToggleRefillWornMana" => {
            let inventory = &mut cx.core.settings.inventory;
            inventory.refill_worn_mana = !inventory.refill_worn_mana;
            cx.core.item_mana.reset();
            cx.core.save_profile(cx.client);
        }
        "SetRefillWornMana" => {
            #[allow(clippy::cast_possible_truncation)]
            let percent = (arg.float() * 100.0).round() as i32;
            cx.core.settings.inventory.refill_worn_mana_percent = percent.clamp(0, 99);
            cx.core.save_profile(cx.client);
        }
        _ => return false,
    }
    true
}

impl TankCx<'_> {
    fn handedness_index(&self, name: &str) -> usize {
        self.core
            .panel
            .item_handedness
            .iter()
            .find(|(item, _)| item == name)
            .map_or(0, |(_, index)| *index)
    }

    /// `RefreshItemEditors`: the consumables' kinds, then every row of the
    /// Items and Consumables pages.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn refresh_item_editors(&mut self) {
        self.refresh_consumable_categories();
        let owned = self.client.rules().owned_items_shared();
        let inventory_unknown = owned.is_empty();
        let mut base_names: Vec<String> = Vec::new();
        let mut display_names: Vec<String> = Vec::new();
        let mut row_ids: Vec<Option<u32>> = Vec::new();
        let mut enchant_rows: Vec<Option<BuffItemEnchantRow>> = Vec::new();
        let mut resolved_names: Vec<String> = Vec::new();
        let mut profiled_ids: Vec<u32> = Vec::new();
        let enchant = self.core.settings.buffs.item_enchant_rows.clone();
        for row in enchant {
            if !row.is_profiled_item_row() {
                continue;
            }
            let object_id = row.object_id.unwrap_or(0);
            let name = if object_id == u32::MAX {
                "Equipped weapon".to_owned()
            } else if let Some(item) = owned.iter().find(|item| item.object_id == object_id) {
                self.core
                    .settings
                    .combat
                    .combat_item_names_by_id
                    .insert(object_id, item.name.clone());
                resolved_names.push(item.name.clone());
                profiled_ids.push(object_id);
                item.name.clone()
            } else {
                profiled_ids.push(object_id);
                self.unheld_item_row_name(object_id, inventory_unknown, &mut resolved_names)
            };
            display_names.push(self.profiled_item_row_text(&name, &row));
            base_names.push(name);
            row_ids.push((object_id != u32::MAX).then_some(object_id));
            enchant_rows.push(Some(row));
        }
        let order = self.core.settings.combat.combat_item_order_ids.clone();
        for id in order {
            if profiled_ids.contains(&id) {
                continue;
            }
            if let Some(item) = owned.iter().find(|item| item.object_id == id) {
                self.core
                    .settings
                    .combat
                    .combat_item_names_by_id
                    .insert(id, item.name.clone());
                base_names.push(item.name.clone());
                display_names.push(item.name.clone());
                resolved_names.push(item.name.clone());
            } else {
                let unheld = self.unheld_item_row_name(id, inventory_unknown, &mut resolved_names);
                base_names.push(unheld.clone());
                display_names.push(unheld);
            }
            row_ids.push(Some(id));
            enchant_rows.push(None);
        }
        let mut names: Vec<String> = self
            .core
            .settings
            .combat
            .combat_item_names
            .iter()
            .cloned()
            .collect();
        names.sort_by(|a, b| mosstank_dotnet::text::ordinal::compare(a, b));
        for name in names {
            if resolved_names.contains(&name) {
                continue;
            }
            base_names.push(name.clone());
            display_names.push(name);
            row_ids.push(None);
            enchant_rows.push(None);
        }
        let rows: Vec<String> = display_names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                if self
                    .core
                    .no_buff_item_names
                    .contains(base_names[index].as_str())
                {
                    format!("{name}   [no buffs]")
                } else {
                    name.clone()
                }
            })
            .collect();
        let hands: Vec<String> = base_names
            .iter()
            .map(|name| HANDEDNESS_CYCLE[self.handedness_index(name)].to_owned())
            .collect();
        let mut consumables: Vec<String> = self
            .core
            .settings
            .combat
            .consumable_names
            .iter()
            .cloned()
            .collect();
        consumables.sort_by(|a, b| mosstank_dotnet::text::ordinal::compare(a, b));
        let excluded =
            parse_excluded_components(&self.core.settings.buffs.blacklisted_spell_components);
        let icons: Vec<u32> = excluded
            .iter()
            .map(|name| {
                owned
                    .iter()
                    .find(|item| {
                        mosstank_dotnet::text::ordinal::equals_ignore_case(&item.name, name)
                    })
                    .map_or(0, |item| item.icon_id)
            })
            .collect();
        let panel = &mut self.core.panel;
        panel.selected_item_row = clamp_row(panel.selected_item_row, rows.len());
        panel.selected_consumable_row = clamp_row(panel.selected_consumable_row, consumables.len());
        panel.selected_excluded_component_row =
            clamp_row(panel.selected_excluded_component_row, excluded.len());
        panel.items = ItemRows {
            rows,
            base_names,
            object_ids: row_ids,
            enchant_rows,
            hands,
            consumables,
            excluded_components: excluded,
            excluded_component_icons: icons,
        };
        panel.items_dirty = false;
    }

    /// `ProfiledItemRowText`.
    fn profiled_item_row_text(&mut self, item_name: &str, row: &BuffItemEnchantRow) -> String {
        if row.casts_nothing() {
            return item_name.to_owned();
        }
        let spell_id = row.spell_id.unwrap_or(0);
        let spell_name = self
            .client
            .rules()
            .spells()
            .try_get(spell_id)
            .map_or_else(|| format!("Spell 0x{spell_id:08X}"), |spell| spell.name);
        format!("{item_name} \u{2014} {spell_name}")
    }

    /// `UnheldItemRowName`.
    fn unheld_item_row_name(
        &self,
        object_id: u32,
        inventory_unknown: bool,
        resolved_names: &mut Vec<String>,
    ) -> String {
        let remembered = self
            .core
            .settings
            .combat
            .combat_item_names_by_id
            .get(&object_id)
            .filter(|name| !mosstank_dotnet::text::casing::is_white_space_only(name))
            .cloned();
        let Some(remembered) = remembered else {
            return format!("<INVALID 0x{object_id:08X}>");
        };
        resolved_names.push(remembered.clone());
        if inventory_unknown {
            remembered
        } else {
            format!("<INVALID {remembered}>")
        }
    }

    /// `RefreshConsumableCategories`.
    fn refresh_consumable_categories(&mut self) {
        let combat = &mut self.core.settings.combat;
        let stale: Vec<String> = combat
            .consumable_categories
            .keys()
            .filter(|name| !combat.consumable_names.contains(name.as_str()))
            .cloned()
            .collect();
        for name in stale {
            combat.consumable_categories.remove(name.as_str());
        }
        let names: Vec<String> = combat.consumable_names.iter().cloned().collect();
        for name in names {
            if !combat.consumable_categories.contains_key(name.as_str()) {
                let category = classify_name(&name);
                combat.consumable_categories.insert(name, category);
            }
        }
        let owned = self.client.rules().owned_items_shared();
        for item in owned.iter() {
            let combat = &self.core.settings.combat;
            if !combat.consumable_names.contains(item.name.as_str())
                || combat
                    .imported_assist_items
                    .iter()
                    .any(|imported| imported.name == item.name)
            {
                continue;
            }
            let Some(properties) = self.client.rules().item_properties(item.object_id) else {
                continue;
            };
            if let Some(category) = try_consumable(item, &properties, self.client.rules()) {
                self.core
                    .settings
                    .combat
                    .consumable_categories
                    .insert(item.name.clone(), category);
            }
        }
    }

    /// `TryGetSelectedInventoryItem`.
    pub(crate) fn selected_inventory_item(&mut self) -> Option<InventoryItem> {
        let selected = self.client.rules().selected_object().unwrap_or(0);
        if selected == 0 {
            return None;
        }
        self.client
            .rules()
            .owned_items_shared()
            .iter()
            .find(|item| item.object_id == selected)
            .cloned()
    }

    /// `AnnounceProfileNotice`: the Items page has no notice line, so a
    /// refusal there is said in chat too.
    fn announce_profile_notice(&mut self, notice: &str) {
        notice.clone_into(&mut self.core.panel.profile_notice);
        crate::tank::Core::write_vtank(self.client, &format!("[MossTank] {notice}"));
    }

    /// `UnsavedProfileNotice`.
    fn unsaved_profile_notice(&self, change: &str) -> String {
        if let Some(failure) = self
            .core
            .profiles
            .save_failure_notice()
            .filter(|n| !n.is_empty())
        {
            return format!("{change}, but the changes could not be fully saved: {failure}");
        }
        if let Some(failure) = self
            .core
            .profiles
            .load_failure_notice()
            .filter(|n| !n.is_empty())
        {
            return format!("{change}, but nothing was saved: {failure}");
        }
        format!(
            "{change}, but nothing was saved: no character profile is active yet. Try again once you are in the world."
        )
    }

    /// `DeleteItemRowAtCore`.
    fn delete_item_row_at(&mut self, index: i32) {
        let items = &self.core.panel.items;
        let Some(at) = row(index, items.base_names.len()) else {
            return;
        };
        let removed = items.base_names[at].clone();
        if let Some(imported) = items.enchant_rows[at].clone() {
            self.remove_profiled_item_enchant_row(&imported, &removed);
            self.refresh_item_editors();
            self.finish_item_notice(&format!("Removed {removed}"));
            return;
        }
        if let Some(object_id) = items.object_ids[at] {
            let combat = &mut self.core.settings.combat;
            combat.combat_item_object_ids.remove(&object_id);
            combat.combat_item_order_ids.retain(|id| *id != object_id);
            combat.combat_item_names_by_id.remove(&object_id);
            combat.removed_combat_item_object_ids.insert(object_id);
        }
        let another_named_row = self
            .core
            .panel
            .items
            .base_names
            .iter()
            .enumerate()
            .any(|(other, name)| other != at && *name == removed);
        if !another_named_row {
            let combat = &mut self.core.settings.combat;
            combat.combat_item_names.remove(removed.as_str());
            combat.combat_item_order.retain(|name| *name != removed);
            self.core.no_buff_item_names.remove(removed.as_str());
            self.core
                .panel
                .item_handedness
                .retain(|(name, _)| *name != removed);
            self.clear_item_enchant_rows(&removed);
        }
        self.refresh_item_editors();
        self.finish_item_notice(&format!("Removed {removed}"));
    }

    /// Saves, then says what was done, or that it was not saved.
    fn finish_item_notice(&mut self, change: &str) {
        self.core.panel.profile_notice = if self.core.save_profile(self.client) {
            format!("{change}.")
        } else {
            self.unsaved_profile_notice(change)
        };
    }

    /// `RemoveProfiledItemEnchantRow`.
    fn remove_profiled_item_enchant_row(
        &mut self,
        removed: &BuffItemEnchantRow,
        display_name: &str,
    ) {
        let object_id = removed.object_id.unwrap_or(0);
        let spell_id = removed.spell_id.unwrap_or(0);
        let rows = &mut self.core.settings.buffs.item_enchant_rows;
        if let Some(index) = rows.iter().position(|candidate| {
            candidate.is_profiled_item_row()
                && candidate.object_id == Some(object_id)
                && candidate.spell_id == Some(spell_id)
        }) {
            rows.remove(index);
        }
        self.core
            .settings
            .combat
            .removed_buffed_item_rows
            .push(BuffedItemKey {
                object_id,
                spell_id,
            });
        if object_id == u32::MAX {
            return;
        }
        if self
            .core
            .settings
            .buffs
            .item_enchant_rows
            .iter()
            .any(|candidate| {
                candidate.is_profiled_item_row() && candidate.object_id == Some(object_id)
            })
        {
            return;
        }
        let combat = &mut self.core.settings.combat;
        combat.combat_item_object_ids.remove(&object_id);
        combat.combat_item_order_ids.retain(|id| *id != object_id);
        combat.combat_item_names_by_id.remove(&object_id);
        combat.combat_item_names.remove(display_name);
        combat.combat_item_order.retain(|name| name != display_name);
        self.core.no_buff_item_names.remove(display_name);
        self.core
            .panel
            .item_handedness
            .retain(|(name, _)| name != display_name);
    }

    /// `ClearItemEnchantRows`: every authored row for one item.
    fn clear_item_enchant_rows(&mut self, item_name: &str) {
        self.core
            .settings
            .buffs
            .item_enchant_rows
            .retain(|row| row.item_name != item_name);
    }

    /// `RemoveSelectedConsumableCore`.
    fn remove_selected_consumable(&mut self) {
        let mut names: Vec<String> = self
            .core
            .settings
            .combat
            .consumable_names
            .iter()
            .cloned()
            .collect();
        names.sort_by(|a, b| mosstank_dotnet::text::ordinal::compare(a, b));
        if names.is_empty() {
            "The Consumables profile is empty.".clone_into(&mut self.core.panel.profile_notice);
            return;
        }
        let at = usize::try_from(clamp_row(
            self.core.panel.selected_consumable_row,
            names.len(),
        ))
        .unwrap_or(0);
        let removed = names[at].clone();
        let combat = &mut self.core.settings.combat;
        combat.consumable_names.remove(removed.as_str());
        combat.consumable_categories.remove(removed.as_str());
        combat
            .imported_assist_items
            .retain(|item| item.name != removed);
        self.core.panel.profile_notice = format!("Removed {removed}.");
        self.refresh_item_editors();
        self.core.save_profile(self.client);
    }

    /// `SetConsumableCategory`: the explicit choice is also the last word
    /// on the kind the profile was loaded with; true when anything changed.
    fn set_consumable_category(&mut self, name: &str, category: ConsumableCategory) -> bool {
        let combat = &mut self.core.settings.combat;
        let mut changed = combat.consumable_categories.get(name) != Some(&category);
        combat
            .consumable_categories
            .insert(name.to_owned(), category);
        for loaded in &mut combat.imported_assist_items {
            if loaded.name != name || loaded.category == category {
                continue;
            }
            *loaded = AssistItem {
                name: loaded.name.clone(),
                category,
            };
            changed = true;
        }
        changed
    }

    /// `AddAllPeasCore`.
    fn add_all_peas(&mut self) {
        let added = self
            .core
            .settings
            .combat
            .consumable_names
            .insert(ALL_PEAS.to_owned());
        let changed = self.set_consumable_category(ALL_PEAS, ConsumableCategory::AllPeas) || added;
        self.core.panel.profile_notice = if added {
            "Added [All Peas].".to_owned()
        } else {
            "[All Peas] is already in this profile.".to_owned()
        };
        if changed {
            self.refresh_item_editors();
            self.core.save_profile(self.client);
        }
    }

    /// `CommitProfileItem`: written first, reported second.
    fn commit_profile_item(&mut self, item: &InventoryItem, no_buffs: bool) {
        let combat = &mut self.core.settings.combat;
        if combat.combat_item_object_ids.insert(item.object_id) {
            combat.combat_item_order_ids.push(item.object_id);
        }
        combat
            .combat_item_names_by_id
            .insert(item.object_id, item.name.clone());
        combat
            .removed_combat_item_object_ids
            .remove(&item.object_id);
        if combat.combat_item_names.insert(item.name.clone()) {
            combat.combat_item_order.push(item.name.clone());
        }
        if no_buffs {
            self.core.no_buff_item_names.insert(item.name.clone());
        } else {
            self.core.no_buff_item_names.remove(item.name.as_str());
        }
        self.populate_item_enchant_rows(item, no_buffs);
        self.refresh_item_editors();
        let added = if no_buffs {
            format!("Added {} (no buffs)", item.name)
        } else {
            format!("Added {}", item.name)
        };
        let notice = if self.core.save_profile(self.client) {
            format!("{added}.")
        } else {
            self.unsaved_profile_notice(&added)
        };
        self.announce_profile_notice(&notice);
    }

    /// `PopulateItemEnchantRows`.
    fn populate_item_enchant_rows(&mut self, item: &InventoryItem, no_buffs: bool) {
        self.clear_item_enchant_rows(&item.name);
        let pet_device = is_pet_device(item);
        if !is_profile_eligible(item, pet_device) {
            return;
        }
        let defaults = default_rows(item, no_buffs, pet_device);
        let rows = &mut self.core.settings.buffs.item_enchant_rows;
        if defaults.is_empty() {
            rows.push(BuffItemEnchantRow {
                item_name: item.name.clone(),
                spell_name: String::new(),
                object_id: None,
                spell_id: None,
            });
            return;
        }
        for spell_name in defaults {
            rows.push(BuffItemEnchantRow {
                item_name: item.name.clone(),
                spell_name: (*spell_name).to_owned(),
                object_id: None,
                spell_id: None,
            });
        }
    }

    /// `CommitConsumable`.
    fn commit_consumable(&mut self, item: &InventoryItem, category: ConsumableCategory) {
        self.core
            .settings
            .combat
            .consumable_names
            .insert(item.name.clone());
        self.set_consumable_category(&item.name, category);
        self.refresh_item_editors();
        self.core.panel.profile_notice = if self.core.save_profile(self.client) {
            format!("Added {}.", item.name)
        } else {
            self.unsaved_profile_notice(&format!("Added {}", item.name))
        };
    }

    /// `BeginProfileItemAddition`: the item waits for its appraisal.
    fn begin_profile_item_addition(&mut self, consumable: bool, no_buffs: bool) {
        let Some(item) = self.selected_inventory_item() else {
            self.announce_profile_notice("Select an owned inventory item first.");
            return;
        };
        let combat = &self.core.settings.combat;
        let already = if consumable {
            combat.consumable_names.contains(item.name.as_str())
        } else {
            combat.combat_item_object_ids.contains(&item.object_id)
        };
        if already {
            self.announce_profile_notice(&format!("{} is already in this list.", item.name));
            return;
        }
        self.core.panel.admission.pending = Some(PendingAddition {
            object_id: item.object_id,
            consumable,
            no_buffs,
            profile: self.core.profiles.selected(),
            elapsed: 0.0,
        });
        self.tick_profile_item_addition(0.0);
    }

    /// `EnsureItemAssessed`: true once the item has been appraised; asks for
    /// one appraisal at a time otherwise.
    fn ensure_item_assessed(&mut self, object_id: u32) -> bool {
        if self
            .client
            .rules()
            .object(object_id)
            .is_some_and(|world| world.last_id_time != 0)
        {
            self.core
                .panel
                .admission
                .retries
                .retain(|(id, _)| *id != object_id);
            return true;
        }
        let admission = &self.core.panel.admission;
        if admission
            .retry_at(object_id)
            .is_some_and(|at| admission.time < at)
        {
            return false;
        }
        if admission.time < admission.next_assessment || self.client.rules().items_busy() {
            return false;
        }
        let result = self.client.rules().identify(object_id);
        self.note_assessment_request(object_id, &result);
        let admission = &mut self.core.panel.admission;
        if result.accepted() {
            // One appraisal in flight at a time; its answer ends the wait,
            // the ten seconds only bound a lost one.
            admission.next_assessment = admission.time + 2.0;
            let at = admission.time + 10.0;
            admission.set_retry(object_id, at);
        } else {
            // A request the client did not send must not hold the others.
            let at = admission.time
                + if result.status == ItemStatus::Busy {
                    1.0
                } else {
                    10.0
                };
            admission.set_retry(object_id, at);
        }
        false
    }

    /// `NoteAssessmentRequest`: said once per item and outcome.
    fn note_assessment_request(&mut self, object_id: u32, result: &ItemResult) {
        let note = mosstank_automation::surface::item_status_name(result.status).to_owned();
        let notes = &mut self.core.panel.admission.notes;
        if notes
            .iter()
            .any(|(id, previous)| *id == object_id && *previous == note)
        {
            return;
        }
        notes.retain(|(id, _)| *id != object_id);
        notes.push((object_id, note.clone()));
        let name = self
            .client
            .rules()
            .owned_items_shared()
            .iter()
            .find(|item| item.object_id == object_id)
            .cloned()
            .map(|item| item.name)
            .unwrap_or_default();
        self.core.emit_macro_log(
            self.client,
            MacroLogChannel::Misc,
            &format!("Assessing {name} (0x{object_id:08X}): {note}"),
            true,
        );
    }

    /// `TickConfiguredItemAssessment`: every profile item is appraised once,
    /// and a consumable nobody gave a kind takes the kind it is.
    pub(crate) fn tick_configured_item_assessment(&mut self, elapsed: f64) {
        let admission = &mut self.core.panel.admission;
        admission.time += elapsed.max(0.0);
        if admission.time < admission.next_scan {
            return;
        }
        admission.next_scan = admission.time + 0.5;
        let owned = self.client.rules().owned_items_shared();
        self.core
            .panel
            .admission
            .retries
            .retain(|(id, _)| owned.iter().any(|item| item.object_id == *id));
        for item in owned.iter() {
            let combat = &self.core.settings.combat;
            if !combat.consumable_names.contains(item.name.as_str())
                && !combat.combat_item_names.contains(item.name.as_str())
                && !combat.combat_item_object_ids.contains(&item.object_id)
            {
                continue;
            }
            if !self.ensure_item_assessed(item.object_id) {
                continue;
            }
            let combat = &self.core.settings.combat;
            if !combat.consumable_names.contains(item.name.as_str())
                || combat
                    .imported_assist_items
                    .iter()
                    .any(|loaded| loaded.name == item.name)
            {
                continue;
            }
            let Some(properties) = self.client.rules().item_properties(item.object_id) else {
                continue;
            };
            if let Some(category) = try_consumable(item, &properties, self.client.rules()) {
                self.core
                    .settings
                    .combat
                    .consumable_categories
                    .insert(item.name.clone(), category);
            }
        }
    }

    /// `TickProfileItemAddition`: the waiting add, once its item is
    /// appraised.
    pub(crate) fn tick_profile_item_addition(&mut self, elapsed: f64) {
        let Some(mut pending) = self.core.panel.admission.pending.take() else {
            return;
        };
        pending.elapsed += elapsed.max(0.0);
        if pending.profile != self.core.profiles.selected() {
            return;
        }
        let item = self
            .client
            .rules()
            .owned_items_shared()
            .iter()
            .find(|value| value.object_id == pending.object_id)
            .cloned();
        let Some(item) = item.filter(|_| pending.elapsed < 30.0) else {
            let owned = self
                .client
                .rules()
                .owned_items_shared()
                .iter()
                .any(|value| value.object_id == pending.object_id);
            self.announce_profile_notice(if owned {
                "Assessment did not complete. Select the item and try again."
            } else {
                "The selected item is no longer owned."
            });
            return;
        };
        if !self.ensure_item_assessed(item.object_id) {
            self.core.panel.profile_notice =
                format!("Assessing {} before adding it\u{2026}", item.name);
            self.core.panel.admission.pending = Some(pending);
            return;
        }
        let Some(properties) = self.client.rules().item_properties(item.object_id) else {
            self.announce_profile_notice("Item properties are unavailable. Try again.");
            return;
        };
        if pending.consumable {
            match try_consumable(&item, &properties, self.client.rules()) {
                Some(category) => self.commit_consumable(&item, category),
                None => self.announce_profile_notice(&format!(
                    "{} is not a supported consumable.",
                    item.name
                )),
            }
            return;
        }
        let pet_device = is_pet_device(&item);
        if !is_profile_eligible(&item, pet_device)
            || (item.valid_locations == MISSILE_WEAPON && item.ammo_type == 0)
        {
            self.announce_profile_notice(&format!("{} is not supported in Items.", item.name));
            return;
        }
        self.commit_profile_item(
            &item,
            pending.no_buffs || pet_device || int_property(&properties, 36).is_some(),
        );
        let mastery = self.client.rules().character().summoning_mastery;
        if pet_device && item.summoning_mastery != 0 && item.summoning_mastery != mastery {
            self.core.panel.profile_notice = format!(
                "Added {} without buffs. Warning: different summoning mastery.",
                item.name
            );
        }
    }

    /// The excluded scarab list written to both owners.
    fn set_excluded_components(&mut self, entries: &[String]) {
        let joined = entries.join(EXCLUDED_COMPONENT_DELIMITER);
        self.core.settings.buffs.blacklisted_spell_components = joined.clone();
        self.core.settings.combat.blacklisted_spell_components = joined;
    }

    /// `BuffRows`: the names and the spells by id, once each,
    /// alphabetically.
    fn buff_rows(
        &mut self,
        names: &mosstank_dotnet::collections::OrdinalStringSet,
        ids: &[u32],
    ) -> Vec<String> {
        let mut rows: Vec<String> = Vec::new();
        for name in names.iter() {
            if !rows.contains(name) {
                rows.push(name.clone());
            }
        }
        for id in ids {
            let name = self.buff_row_name(*id);
            if !rows.contains(&name) {
                rows.push(name);
            }
        }
        rows.sort_by(|a, b| mosstank_dotnet::text::ordinal::compare(a, b));
        rows
    }

    /// `BuffRowName`.
    fn buff_row_name(&mut self, spell_id: u32) -> String {
        self.client
            .rules()
            .spells()
            .try_get(spell_id)
            .map_or_else(|| format!("Spell {spell_id}"), |spell| spell.name)
    }

    /// `DeleteBuffRow`.
    fn delete_buff_row(&mut self, blacklist: bool, index: i32) {
        let buffs = &self.core.settings.buffs;
        let (names, ids) = if blacklist {
            (
                buffs.blacklisted_buff_family_names.clone(),
                buffs.anti_extra_buff_spell_ids.clone(),
            )
        } else {
            (
                buffs.extra_buff_spell_names.clone(),
                buffs.extra_buff_spell_ids.clone(),
            )
        };
        let rows = self.buff_rows(&names, &ids);
        let Some(at) = row(index, rows.len()) else {
            return;
        };
        let selected = rows[at].clone();
        let mut keep: Vec<u32> = Vec::new();
        for id in ids.iter().rev() {
            if self.buff_row_name(*id) != selected {
                keep.push(*id);
            }
        }
        keep.reverse();
        let buffs = &mut self.core.settings.buffs;
        if blacklist {
            buffs
                .blacklisted_buff_family_names
                .remove(selected.as_str());
            buffs.anti_extra_buff_spell_ids = keep;
        } else {
            buffs.extra_buff_spell_names.remove(selected.as_str());
            buffs.extra_buff_spell_ids = keep;
        }
    }

    /// `ShowBuffPickerCore`.
    fn show_buff_picker(&mut self, for_blacklist: bool) {
        let panel = &mut self.core.panel;
        panel.buff_picker_for_blacklist = for_blacklist;
        panel.buff_picker_search_text.clear();
        panel.selected_buff_picker_row = 0;
        panel.buff_picker_visible = true;
    }

    /// `BuffPickerRows`: the known self buffs matching the search, once
    /// each, alphabetically.
    /// Kept with the list and search they came from: the same shared list
    /// and search give the same rows.
    fn buff_picker_rows(&mut self) -> Vec<String> {
        let known = self.client.rules().spells().known_self_buffs_shared();
        let search = &self.core.panel.buff_picker_search_text;
        if let Some((kept_known, kept_search, rows)) = &self.core.memo.buff_picker_rows
            && std::rc::Rc::ptr_eq(kept_known, &known)
            && kept_search == search
        {
            return rows.clone();
        }
        let any = mosstank_dotnet::text::casing::is_white_space_only(search);
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut rows: Vec<String> = Vec::new();
        for spell in known.iter() {
            if !any && !mosstank_dotnet::text::ordinal::contains_ignore_case(&spell.name, search) {
                continue;
            }
            if seen.insert(&spell.name) {
                rows.push(spell.name.clone());
            }
        }
        rows.sort_by(|a, b| mosstank_dotnet::text::ordinal::compare(a, b));
        self.core.memo.buff_picker_rows =
            Some((std::rc::Rc::clone(&known), search.clone(), rows.clone()));
        rows
    }

    /// `PickBuffAtCore`.
    fn pick_buff_at(&mut self, index: i32) {
        let rows = self.buff_picker_rows();
        let Some(at) = row(index, rows.len()) else {
            return;
        };
        let name = rows[at].clone();
        let spell_id = self
            .client
            .rules()
            .spells()
            .known_self_buffs()
            .into_iter()
            .find(|candidate| candidate.name == name)
            .map_or(0, |spell| spell.spell_id);
        let buffs = &mut self.core.settings.buffs;
        if self.core.panel.buff_picker_for_blacklist {
            buffs.blacklisted_buff_family_names.insert(name);
            add_exemplar(&mut buffs.anti_extra_buff_spell_ids, spell_id);
        } else {
            buffs.extra_buff_spell_names.insert(name);
            add_exemplar(&mut buffs.extra_buff_spell_ids, spell_id);
        }
        self.core.panel.buff_picker_visible = false;
        self.core.save_profile(self.client);
    }
}

/// `AddExemplar`.
fn add_exemplar(ids: &mut Vec<u32>, spell_id: u32) {
    if spell_id != 0 && !ids.contains(&spell_id) {
        ids.push(spell_id);
    }
}

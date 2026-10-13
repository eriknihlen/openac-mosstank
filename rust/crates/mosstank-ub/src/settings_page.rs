//! The UB settings page (`MossTankPanel.UbSettings`): one filtered list of
//! settings, one description, one edit field, and two sub-pages for the two
//! value shapes a single field cannot hold (a colour and a list). Every row
//! is a view of the catalogue: a change is written through the catalogue,
//! so the row's scope decides which file it lands in.
//!
//! The page's state lives here; the catalogue and the profiles come in on
//! each call ([`UbCatalog`], [`UbProfiles`]), so the page and every other
//! reader of the catalogue can never hold two different values.

use mosstank_settings::ub::{UbSetting, UbSettingKind, UbSettingScope, UbSettingValue};
use openac_plugin_compat::text::ordinal;

use crate::alias::AliasTable;
use crate::catalog::UbCatalog;
use crate::font;
use crate::live::UbProfiles;

/// The first row of the category list, which filters nothing.
pub const ALL_CATEGORIES: &str = "(all)";

/// The width of the description labels beside the list, which the
/// description is wrapped to.
pub const DESCRIPTION_WIDTH: f64 = 308.0;

/// The alias list's row.
const ALIAS_LIST: &str = "Aliases.DefinedAliases";

/// The page.
#[derive(Debug, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct UbSettingsPage {
    rows: Vec<UbSetting>,
    row_names: Vec<String>,
    row_values: Vec<String>,
    category_names: Vec<String>,
    selected_category: usize,
    selected_row: usize,
    filter: String,
    draft: String,
    notice: String,
    profile_name_draft: String,
    description_text: String,
    description_lines: Vec<String>,

    color_visible: bool,
    color_name: String,
    color_value: u32,

    list_visible: bool,
    list_name: String,
    list_entries: Vec<String>,
    selected_list_row: usize,
    list_draft: String,
    list_notice: String,
}

/// `ClampRow`: a row index held to a list of `count` rows.
fn clamp_row(index: i64, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    usize::try_from(index.max(0)).unwrap_or(0).min(count - 1)
}

fn shape_name(kind: UbSettingKind) -> &'static str {
    match kind {
        UbSettingKind::Bool => "True or False",
        UbSettingKind::Int | UbSettingKind::Enum => "whole number",
        UbSettingKind::Single | UbSettingKind::Double => "number",
        UbSettingKind::Color => "colour",
        UbSettingKind::String | UbSettingKind::Collection => "value",
    }
}

impl UbSettingsPage {
    /// The page over `catalog`, every row listed.
    pub fn new(catalog: &dyn UbCatalog) -> Self {
        let mut page = Self::default();
        page.category_names.push(ALL_CATEGORIES.to_owned());
        page.category_names
            .extend(catalog.categories().iter().cloned());
        page.refresh(catalog);
        page
    }

    // ---- what the page shows ----------------------------------------------

    /// The category list, "(all)" first.
    #[must_use]
    pub fn category_names(&self) -> &[String] {
        &self.category_names
    }

    /// The category chosen.
    #[must_use]
    pub fn selected_category(&self) -> usize {
        self.selected_category
    }

    /// The rows' names.
    #[must_use]
    pub fn setting_names(&self) -> &[String] {
        &self.row_names
    }

    /// The rows' values as the value column shows them.
    #[must_use]
    pub fn setting_values(&self) -> &[String] {
        &self.row_values
    }

    /// The row chosen.
    #[must_use]
    pub fn selected_setting(&self) -> usize {
        self.selected_row
    }

    /// The filter typed.
    #[must_use]
    pub fn filter_text(&self) -> &str {
        &self.filter
    }

    /// What the last change said.
    #[must_use]
    pub fn notice(&self) -> &str {
        &self.notice
    }

    /// The shared edit field.
    #[must_use]
    pub fn value_draft(&self) -> &str {
        &self.draft
    }

    /// The new profile's name, as typed.
    #[must_use]
    pub fn profile_name_draft(&self) -> &str {
        &self.profile_name_draft
    }

    fn selected_row(&self) -> Option<&UbSetting> {
        if self.rows.is_empty() {
            None
        } else {
            self.rows.get(self.selected_row.min(self.rows.len() - 1))
        }
    }

    /// One line of the selected setting's description (0 to 3), wrapped to
    /// the labels' width.
    pub fn description_line(&mut self, index: usize) -> String {
        let text = match self.selected_row() {
            Some(row) => format!("{}: {}", row.name(), row.definition.summary),
            None => "No setting matches the filter.".to_owned(),
        };
        if text != self.description_text {
            self.description_lines = font::wrap(&text, DESCRIPTION_WIDTH);
            self.description_text = text;
        }
        self.description_lines
            .get(index)
            .cloned()
            .unwrap_or_default()
    }

    /// Which of the three files an edit to this row lands in.
    #[must_use]
    pub fn scope_text(&self, profiles: &dyn UbProfiles) -> String {
        match self.selected_row().map(UbSetting::scope) {
            None => String::new(),
            Some(UbSettingScope::Global) => "Saved for this installation.".to_owned(),
            Some(UbSettingScope::Character) => "Saved for this character only.".to_owned(),
            Some(UbSettingScope::Profile) => {
                format!("Saved in profile '{}'.", profiles.profile_name())
            }
        }
    }

    // ---- the list -------------------------------------------------------------

    /// Picks a category.
    pub fn select_category(&mut self, catalog: &dyn UbCatalog, index: i64) {
        self.selected_category = clamp_row(index, self.category_names.len());
        self.selected_row = 0;
        self.refresh(catalog);
    }

    /// Picks a row.
    pub fn select_setting(&mut self, catalog: &dyn UbCatalog, index: i64) {
        self.selected_row = clamp_row(index, self.rows.len());
        self.refresh(catalog);
    }

    /// Types the filter.
    pub fn set_filter_text(&mut self, catalog: &dyn UbCatalog, text: &str) {
        text.clone_into(&mut self.filter);
        self.selected_row = 0;
        self.refresh(catalog);
    }

    /// A click on the value itself does the shortest thing that shape
    /// allows: a switch flips, a choice steps on, a colour and a list open
    /// the sub-page they need, anything else simply selects the row.
    pub fn click_value(&mut self, catalog: &mut dyn UbCatalog, index: i64) {
        let Some(row) = usize::try_from(index)
            .ok()
            .and_then(|at| self.rows.get(at).cloned())
        else {
            return;
        };
        self.selected_row = usize::try_from(index).unwrap_or(0);
        match row.kind() {
            UbSettingKind::Bool => {
                let flipped = !catalog.get(&row).boolean;
                catalog.set(&row, UbSettingValue::from_bool(flipped));
                let display = catalog.display(&row);
                self.note_change(catalog, &row, &format!("is now {display}"));
            }
            UbSettingKind::Enum => self.cycle_choice(catalog, &row),
            UbSettingKind::Color => self.open_color_editor(catalog, &row),
            UbSettingKind::Collection => self.open_list_editor(catalog, &row),
            _ => {}
        }
        self.refresh(catalog);
    }

    /// Types into the shared field.
    pub fn set_value_draft(&mut self, text: &str) {
        text.clone_into(&mut self.draft);
    }

    /// Types into the field and commits it.
    pub fn submit(&mut self, catalog: &mut dyn UbCatalog, text: &str) {
        text.clone_into(&mut self.draft);
        self.apply_draft(catalog);
    }

    /// Commits the field as it stands.
    pub fn submit_draft(&mut self, catalog: &mut dyn UbCatalog) {
        self.apply_draft(catalog);
    }

    /// Puts a row back to what the catalogue says, by taking this row's own
    /// value out of its file rather than writing the default into it.
    pub fn reset_to_default(&mut self, catalog: &mut dyn UbCatalog) {
        let Some(row) = self.selected_row().cloned() else {
            return;
        };
        catalog.reset_to_default(&row);
        let display = catalog.display(&row);
        self.note_change(catalog, &row, &format!("is back to {display}"));
        self.refresh(catalog);
    }

    fn apply_draft(&mut self, catalog: &mut dyn UbCatalog) {
        let Some(row) = self.selected_row().cloned() else {
            return;
        };
        let Some(value) = UbSettingValue::try_parse(row.kind(), &self.draft) else {
            self.notice = format!(
                "'{}' is not a {}; {} is unchanged.",
                self.draft,
                shape_name(row.kind()),
                row.name()
            );
            return;
        };
        catalog.set(&row, value);
        let display = catalog.display(&row);
        self.note_change(catalog, &row, &format!("is now {display}"));
        self.refresh(catalog);
    }

    /// What a change to one row reports: a row whose file did not read
    /// still changes for this session, and the sentence says so.
    fn note_change(&mut self, catalog: &dyn UbCatalog, row: &UbSetting, clause: &str) {
        self.notice = if catalog.can_save(row) {
            format!("{} {clause}.", row.name())
        } else {
            format!(
                "{} {clause} for this session only; its settings file could not be read, so nothing is saved until that file is repaired.",
                row.name()
            )
        };
    }

    /// A switch flipped from somewhere else on the tab (the UB page's
    /// buttons): the same write and the same notice.
    pub fn flip_switch(&mut self, catalog: &mut dyn UbCatalog, name: &str) {
        let row = catalog.require(name);
        let flipped = !catalog.get(&row).boolean;
        catalog.set(&row, UbSettingValue::from_bool(flipped));
        let display = catalog.display(&row);
        self.note_change(catalog, &row, &format!("is now {display}"));
        self.refresh(catalog);
    }

    fn cycle_choice(&mut self, catalog: &mut dyn UbCatalog, row: &UbSetting) {
        let choices = &row.definition.choices;
        if choices.is_empty() {
            return;
        }
        let current = catalog.get(row).as_int32();
        let at = choices
            .iter()
            .position(|choice| choice.value == current)
            .unwrap_or(0);
        catalog.set(
            row,
            UbSettingValue::from_choice(choices[(at + 1) % choices.len()].value),
        );
        let display = catalog.display(row);
        self.note_change(catalog, row, &format!("is now {display}"));
    }

    /// Re-reads the rows: the category, the filter (name or description,
    /// ignoring case), the values and the draft.
    pub fn refresh(&mut self, catalog: &dyn UbCatalog) {
        let category = if self.selected_category > 0 {
            self.category_names
                .get(self.selected_category)
                .cloned()
                .unwrap_or_default()
        } else {
            String::new()
        };
        let filter = self.filter.trim().to_owned();
        self.rows = catalog
            .settings()
            .iter()
            .filter(|row| {
                (category.is_empty() || row.definition.category() == category)
                    && (filter.is_empty()
                        || ordinal::contains_ignore_case(row.name(), &filter)
                        || ordinal::contains_ignore_case(&row.definition.summary, &filter))
            })
            .cloned()
            .collect();
        self.selected_row = clamp_row(
            i64::try_from(self.selected_row).unwrap_or(0),
            self.rows.len(),
        );
        self.row_names = self.rows.iter().map(|row| row.name().to_owned()).collect();
        self.row_values = self.rows.iter().map(|row| catalog.display(row)).collect();
        self.draft = self
            .selected_row()
            .map(|row| catalog.get(row).to_storage_string())
            .unwrap_or_default();
    }

    // ---- profiles ---------------------------------------------------------------

    /// Opens a profile chosen from the menu.
    pub fn select_profile(
        &mut self,
        catalog: &dyn UbCatalog,
        profiles: &mut dyn UbProfiles,
        name: &str,
    ) {
        profiles.select_profile(name);
        self.notice = format!("Profile '{}' is open.", profiles.profile_name());
        self.refresh(catalog);
    }

    /// Types the new profile's name.
    pub fn set_profile_name_draft(&mut self, text: &str) {
        text.clone_into(&mut self.profile_name_draft);
    }

    /// Makes a profile by a name typed and entered.
    pub fn create_named_profile(
        &mut self,
        catalog: &dyn UbCatalog,
        profiles: &mut dyn UbProfiles,
        text: &str,
    ) {
        text.clone_into(&mut self.profile_name_draft);
        self.create_profile_core(catalog, profiles, false);
    }

    /// Makes an empty profile by the name typed.
    pub fn create_profile(&mut self, catalog: &dyn UbCatalog, profiles: &mut dyn UbProfiles) {
        self.create_profile_core(catalog, profiles, false);
    }

    /// Makes a profile holding what the open one holds.
    pub fn copy_profile(&mut self, catalog: &dyn UbCatalog, profiles: &mut dyn UbProfiles) {
        self.create_profile_core(catalog, profiles, true);
    }

    fn create_profile_core(
        &mut self,
        catalog: &dyn UbCatalog,
        profiles: &mut dyn UbProfiles,
        copy_current: bool,
    ) {
        let (made, notice) = profiles.create_profile(&self.profile_name_draft, copy_current);
        self.notice = notice;
        if !made {
            return;
        }
        self.profile_name_draft.clear();
        self.refresh(catalog);
    }

    // ---- the colour sub-page ------------------------------------------------------

    /// Whether the colour sub-page is open.
    #[must_use]
    pub fn color_visible(&self) -> bool {
        self.color_visible
    }

    /// The draft as the markup's colour binding reads it: `0xAARRGGBB`.
    #[must_use]
    pub fn color_swatch(&self) -> u32 {
        self.color_value
    }

    /// Which row the colour sub-page edits, and its draft.
    #[must_use]
    pub fn color_target(&self) -> String {
        if self.color_name.is_empty() {
            String::new()
        } else {
            format!("{} = #{:08X}", self.color_name, self.color_value)
        }
    }

    /// One channel (shift 24 alpha, 16 red, 8 green, 0 blue).
    #[must_use]
    pub fn color_channel(&self, shift: u32) -> f32 {
        f32::from(u8::try_from((self.color_value >> shift) & 0xFF).unwrap_or(0))
    }

    /// One channel as the slider's caption writes it.
    #[must_use]
    pub fn color_channel_text(&self, shift: u32) -> String {
        ((self.color_value >> shift) & 0xFF).to_string()
    }

    /// The draft as `#AARRGGBB`.
    #[must_use]
    pub fn color_hex(&self) -> String {
        format!("#{:08X}", self.color_value)
    }

    /// Moves one channel's slider.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn set_color_channel(&mut self, shift: u32, value: f32) {
        let channel = (f64::from(value).round_ties_even() as i64).clamp(0, 255) as u32;
        self.color_value = (self.color_value & !(0xFF << shift)) | (channel << shift);
    }

    /// Types the hex field; a value that does not read is ignored.
    pub fn set_color_hex(&mut self, text: &str) {
        if let Some(value) = UbSettingValue::try_parse(UbSettingKind::Color, text) {
            self.color_value = value.as_color();
        }
    }

    /// Saves the draft colour into its row and shuts the sub-page.
    pub fn apply_color(&mut self, catalog: &mut dyn UbCatalog) {
        if !self.color_name.is_empty()
            && let Some(row) = catalog.try_get(&self.color_name)
        {
            catalog.set(&row, UbSettingValue::from_color(self.color_value));
            let display = catalog.display(&row);
            self.note_change(catalog, &row, &format!("is now {display}"));
        }
        self.color_visible = false;
        self.refresh(catalog);
    }

    /// Shuts the colour sub-page without saving.
    pub fn hide_color_editor(&mut self) {
        self.color_visible = false;
    }

    /// The colour button: opens the sub-page on the selected row, or says
    /// it is not a colour.
    pub fn show_color_editor(&mut self, catalog: &dyn UbCatalog) {
        match self.selected_row().cloned() {
            Some(row) if row.kind() == UbSettingKind::Color => {
                self.open_color_editor(catalog, &row);
            }
            _ => "That setting is not a colour.".clone_into(&mut self.notice),
        }
    }

    /// Puts the draft back to the row's default colour.
    pub fn reset_color_to_default(&mut self, catalog: &dyn UbCatalog) {
        if !self.color_name.is_empty()
            && let Some(row) = catalog.try_get(&self.color_name)
        {
            self.color_value = row.definition.default.as_color();
        }
    }

    fn open_color_editor(&mut self, catalog: &dyn UbCatalog, row: &UbSetting) {
        row.name().clone_into(&mut self.color_name);
        self.color_value = catalog.get(row).as_color();
        self.color_visible = true;
        self.list_visible = false;
    }

    // ---- the list sub-page --------------------------------------------------------

    /// Whether the list sub-page is open.
    #[must_use]
    pub fn list_visible(&self) -> bool {
        self.list_visible
    }

    /// The list's title: its row and how many lines it has.
    #[must_use]
    pub fn list_title(&self) -> String {
        if self.list_name.is_empty() {
            String::new()
        } else {
            format!("{} ({})", self.list_name, self.list_entries.len())
        }
    }

    /// The list's lines.
    #[must_use]
    pub fn list_rows(&self) -> &[String] {
        &self.list_entries
    }

    /// The line chosen.
    #[must_use]
    pub fn selected_list_row(&self) -> usize {
        self.selected_list_row
    }

    /// The list's edit field.
    #[must_use]
    pub fn list_draft(&self) -> &str {
        &self.list_draft
    }

    /// What is wrong with the open list, or nothing.
    #[must_use]
    pub fn list_notice(&self) -> &str {
        &self.list_notice
    }

    /// Picks a line, loading it into the field.
    pub fn select_list_row(&mut self, index: i64) {
        self.selected_list_row = clamp_row(index, self.list_entries.len());
        self.list_draft = self
            .list_entries
            .get(self.selected_list_row)
            .cloned()
            .unwrap_or_default();
    }

    /// Types the list's field.
    pub fn set_list_draft(&mut self, text: &str) {
        text.clone_into(&mut self.list_draft);
    }

    /// Types a line and adds it.
    pub fn add_list_entry(&mut self, catalog: &mut dyn UbCatalog, text: &str) {
        text.clone_into(&mut self.list_draft);
        self.add_list_entry_core(catalog);
    }

    /// Adds the field's line.
    pub fn add_list_entry_from_draft(&mut self, catalog: &mut dyn UbCatalog) {
        self.add_list_entry_core(catalog);
    }

    /// Replaces the chosen line with the field's.
    pub fn replace_list_entry(&mut self, catalog: &mut dyn UbCatalog) {
        let entry = self.list_draft.trim().to_owned();
        if self.list_entries.is_empty() || entry.is_empty() {
            return;
        }
        if !self.admit_list_entry(&entry) {
            return;
        }
        let at = self.selected_list_row.min(self.list_entries.len() - 1);
        self.list_entries[at] = entry;
        self.save_list(catalog);
    }

    /// Removes the chosen line.
    pub fn remove_list_entry(&mut self, catalog: &mut dyn UbCatalog) {
        if self.list_entries.is_empty() {
            return;
        }
        let at = self.selected_list_row.min(self.list_entries.len() - 1);
        self.list_entries.remove(at);
        self.selected_list_row = clamp_row(
            i64::try_from(self.selected_list_row).unwrap_or(0),
            self.list_entries.len(),
        );
        self.save_list(catalog);
    }

    /// Shuts the list sub-page.
    pub fn hide_list_editor(&mut self) {
        self.list_visible = false;
    }

    /// The list button: opens the sub-page on the selected row, or says it
    /// is not a list.
    pub fn show_list_editor(&mut self, catalog: &dyn UbCatalog) {
        match self.selected_row().cloned() {
            Some(row) if row.kind() == UbSettingKind::Collection => {
                self.open_list_editor(catalog, &row);
            }
            _ => "That setting is not a list.".clone_into(&mut self.notice),
        }
    }

    /// Leaving the tab shuts both sub-pages.
    pub fn leave(&mut self) {
        self.color_visible = false;
        self.list_visible = false;
    }

    fn open_list_editor(&mut self, catalog: &dyn UbCatalog, row: &UbSetting) {
        row.name().clone_into(&mut self.list_name);
        self.list_entries = catalog.get(row).items;
        self.selected_list_row = 0;
        self.list_draft.clear();
        self.list_visible = true;
        self.color_visible = false;
        self.note_list_shape();
    }

    fn add_list_entry_core(&mut self, catalog: &mut dyn UbCatalog) {
        let entry = self.list_draft.trim().to_owned();
        if entry.is_empty() || !self.admit_list_entry(&entry) {
            return;
        }
        self.list_entries.push(entry);
        self.selected_list_row = self.list_entries.len() - 1;
        self.list_draft.clear();
        self.save_list(catalog);
    }

    /// A list is written the moment it changes.
    fn save_list(&mut self, catalog: &mut dyn UbCatalog) {
        if self.list_name.is_empty() {
            return;
        }
        let Some(row) = catalog.try_get(&self.list_name) else {
            return;
        };
        catalog.set(&row, UbSettingValue::from_collection(&self.list_entries));
        let count = self.list_entries.len();
        self.note_change(catalog, &row, &format!("has {count} entries"));
        self.note_list_shape();
        self.refresh(catalog);
    }

    fn is_alias_list(&self) -> bool {
        ordinal::equals_ignore_case(&self.list_name, ALIAS_LIST)
    }

    /// Whether one typed line may go into the open list: only the alias
    /// list has a shape to hold it to.
    fn admit_list_entry(&mut self, entry: &str) -> bool {
        if !self.is_alias_list() {
            return true;
        }
        match AliasTable::try_parse_line(entry) {
            Ok(_) => true,
            Err(problem) => {
                self.list_notice = problem;
                false
            }
        }
    }

    /// Names the lines of the open list that are not what the list holds.
    fn note_list_shape(&mut self) {
        if !self.is_alias_list() {
            self.list_notice.clear();
            return;
        }
        let problems = AliasTable::parse(&self.list_entries).problems;
        self.list_notice = match problems.len() {
            0 => String::new(),
            1 => problems[0].clone(),
            more => format!("{} ({} more)", problems[0], more - 1),
        };
    }
}

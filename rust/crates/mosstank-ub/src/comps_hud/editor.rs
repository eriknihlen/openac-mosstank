//! The Edit items window's side of the HUD: what the character carries,
//! as filtered by the search and the kind; the rows as the window last
//! showed them, so a click acts on the item the player saw even when the
//! carried items were taken again since; the selected rows; and the lines
//! the window shows over its two lists.

use super::carried::{self, CarriedItem, CarriedKind};
use super::{ComponentHudController, REFRESH_SECONDS, clamp};
use crate::host::UbHost;

/// The window's state.
#[derive(Debug, Clone, Default)]
pub(super) struct Editor {
    pub(super) visible: bool,
    search: String,
    kind: CarriedKind,
    /// The carried rows' names as the window last showed them.
    shown: Vec<String>,
    /// The selected carried row, by name.
    selected_carried: Option<String>,
    /// The selected row of the HUD's list.
    pub(super) selected_entry: usize,
}

impl ComponentHudController {
    /// Whether the Edit items window is open.
    #[must_use]
    pub fn editor_visible(&self) -> bool {
        self.editor.visible
    }

    /// Opens the Edit items window, with what the character carries taken
    /// at once.
    pub fn show_editor(&mut self, host: &mut dyn UbHost) {
        self.editor.visible = true;
        let objects = host.objects();
        self.carried = carried::gather(&objects);
        self.choose_drawn_objects(&objects);
        self.refresh_remaining = REFRESH_SECONDS;
        self.note_shown_carried();
    }

    /// Closes it.
    pub fn hide_editor(&mut self) {
        self.editor.visible = false;
        self.carried.clear();
        self.editor.shown.clear();
    }

    /// What the window's search holds.
    #[must_use]
    pub fn search(&self) -> &str {
        &self.editor.search
    }

    /// Sets the search.
    pub fn set_search(&mut self, text: &str) {
        text.clone_into(&mut self.editor.search);
        self.note_shown_carried();
    }

    /// The kind the window lists.
    #[must_use]
    pub fn kind(&self) -> CarriedKind {
        self.editor.kind
    }

    /// Sets the kind it lists.
    pub fn set_kind(&mut self, kind: CarriedKind) {
        self.editor.kind = kind;
        self.note_shown_carried();
    }

    /// What the character carries, as the window filters it.
    #[must_use]
    pub fn carried_rows(&self) -> Vec<&CarriedItem> {
        carried::filter(&self.carried, &self.editor.search, self.editor.kind)
    }

    /// The carried rows the window is shown now: what a row click names
    /// from here on.
    pub fn note_shown_carried(&mut self) -> Vec<String> {
        let names: Vec<String> = self
            .carried_rows()
            .iter()
            .map(|item| item.name.clone())
            .collect();
        self.editor.shown.clone_from(&names);
        names
    }

    /// The line over what the character carries: what it is, or why it is
    /// empty.
    #[must_use]
    pub fn carried_heading(&self) -> &'static str {
        if self.carried.is_empty() {
            "You carry nothing yet"
        } else if self.carried_rows().is_empty() {
            "Nothing you carry matches"
        } else {
            "You carry"
        }
    }

    /// The line over the HUD's list: what it is, or that it is empty.
    #[must_use]
    pub fn entries_heading(&self) -> &'static str {
        if self.entries.is_empty() {
            "In the HUD: nothing yet (tick an item)"
        } else {
            "In the HUD"
        }
    }

    /// Whether a carried name is on the HUD.
    #[must_use]
    pub fn is_listed(&self, name: &str) -> bool {
        self.entries.iter().any(|entry| entry.name == name)
    }

    /// The selected carried row among those shown; none when it is not
    /// shown (or nothing is).
    #[must_use]
    pub fn selected_carried(&self) -> Option<usize> {
        let selected = self.editor.selected_carried.as_deref()?;
        self.editor.shown.iter().position(|name| name == selected)
    }

    /// Selects a carried row as shown.
    pub fn select_carried(&mut self, row: usize) {
        self.editor.selected_carried = self.editor.shown.get(row).cloned();
    }

    /// Puts the item of the carried row the player clicked (as the window
    /// showed it) on the HUD, or takes it off when it is on; whether there
    /// was such a row.
    pub fn toggle_carried(&mut self, host: &mut dyn UbHost, row: usize) -> bool {
        let Some(name) = self.editor.shown.get(row).cloned() else {
            return false;
        };
        self.editor.selected_carried = Some(name.clone());
        if let Some(index) = self.entries.iter().position(|entry| entry.name == name) {
            self.remove_at(host, index);
        } else {
            let carried = self.carried.iter().find(|item| item.name == name);
            let icon = carried.map_or(0, |item| item.icon_id);
            let object = carried.map(|item| item.object_id);
            // Refused only out of the world, where there is nothing to list.
            if self.add(host, &name, icon).is_ok()
                && let Some(object) = object
            {
                self.drawn_objects.insert(name, object);
            }
        }
        true
    }

    /// The selected row of the HUD's list; none when it is empty.
    #[must_use]
    pub fn selected_entry(&self) -> Option<usize> {
        (!self.entries.is_empty()).then_some(self.editor.selected_entry)
    }

    /// Selects a row of the HUD's list.
    pub fn select_entry(&mut self, row: usize) {
        self.editor.selected_entry = clamp(row, self.entries.len());
    }
}

//! The views metas create (`MetaViewManager`). A view is VTank view XML
//! ([`MetaView`]) or, as a MossTank extension, the host's own panel
//! markup. Every view is kept whether or not the host can draw it, so the
//! ui* functions and button actions behave the same with and without a
//! window.

use std::collections::HashSet;

use ac_plugin_abi::LogLevel;
use openac_plugin_compat::text::casing;
use sha2::{Digest, Sha256};

use crate::host::{MetaHost, PanelDescriptor};
use crate::view::{self, MetaView, MetaViewControlKind};

/// What setting a control's label came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaViewLabelResult {
    /// There is no such control.
    NotFound,
    /// The label was set.
    Set,
    /// The control takes no label.
    TakesNoLabel,
}

/// The views a meta created, as the ui* functions see them
/// (`IMetaViewControls`).
pub trait MetaViewControls {
    /// Whether a meta created a view with this name, in any form.
    fn view_exists(&self, view: &str) -> bool;
    /// Whether this view's controls are held here rather than by the host.
    fn holds_controls(&self, view: &str) -> bool;
    /// Whether the view is shown.
    fn is_view_visible(&mut self, host: &mut dyn MetaHost, view: &str) -> bool;
    /// Whether the view has the control.
    fn control_exists(&self, view: &str, control: &str) -> bool;
    /// Labels a control.
    fn set_control_label(&mut self, view: &str, control: &str, label: &str) -> MetaViewLabelResult;
    /// Shows or hides a control; false when there is no such control.
    fn set_control_visible(&mut self, view: &str, control: &str, visible: bool) -> bool;
}

/// What a hit button asks for, in order: its expression, then its state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonAction<'a> {
    /// Run this expression.
    Expression(&'a str),
    /// Move the meta to this state.
    SetState(&'a str),
}

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    view: Option<MetaView>,
    registration: Option<u32>,
}

/// The views.
#[derive(Debug, Clone, Default)]
pub struct MetaViewManager {
    views: Vec<Entry>,
    warnings: HashSet<String>,
}

/// How many views the reference allows (one more may exist; see
/// [`MetaViewManager::create`]).
const OFFICIAL_VIEW_LIMIT: usize = 5;

impl MetaViewManager {
    /// No views.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many views exist.
    #[must_use]
    pub fn count(&self) -> usize {
        self.views.len()
    }

    /// The window handle of a view, when it is drawn.
    #[must_use]
    pub fn registration(&self, name: &str) -> Option<u32> {
        self.entry(name).and_then(|entry| entry.registration)
    }

    fn entry(&self, name: &str) -> Option<&Entry> {
        self.views.iter().find(|entry| entry.name == name)
    }

    /// Creates or replaces a view. False when it was refused: no name or
    /// markup, the view limit reached, or markup that does not parse. A
    /// view with the same name is destroyed first, even when the new one
    /// is then refused.
    pub fn create(&mut self, host: &mut dyn MetaHost, name: &str, markup: &str) -> bool {
        if name.is_empty() || markup.is_empty() {
            return false;
        }
        // The limit counts before the replaced view goes, so six views may
        // exist and a seventh, or a replacement at six, is refused.
        if self.views.len() > OFFICIAL_VIEW_LIMIT {
            return false;
        }
        self.destroy(host, name);
        if view::is_panel_markup(markup) {
            let registration = register_panel_markup(host, name, markup);
            self.views.push(Entry {
                name: name.to_owned(),
                view: None,
                registration,
            });
            return true;
        }
        let parsed = match MetaView::parse(markup) {
            Ok(parsed) => parsed,
            Err(error) => {
                host.log(
                    LogLevel::Warn,
                    &format!("MossTank Meta view '{name}' is invalid: {error}"),
                );
                return false;
            }
        };
        let registration = register_view(host, name, &parsed);
        self.views.push(Entry {
            name: name.to_owned(),
            view: Some(parsed),
            registration,
        });
        true
    }

    /// Destroys a view; false when there was none.
    pub fn destroy(&mut self, host: &mut dyn MetaHost, name: &str) -> bool {
        let Some(position) = self.views.iter().position(|entry| entry.name == name) else {
            return false;
        };
        let entry = self.views.remove(position);
        if let Some(handle) = entry.registration {
            host.unregister_panel(handle);
        }
        true
    }

    /// Destroys every view.
    pub fn destroy_all(&mut self, host: &mut dyn MetaHost) {
        for entry in std::mem::take(&mut self.views) {
            if let Some(handle) = entry.registration {
                host.unregister_panel(handle);
            }
        }
    }

    /// The parsed view behind a name; `None` for none or a panel-markup
    /// view.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&MetaView> {
        self.entry(name).and_then(|entry| entry.view.as_ref())
    }

    fn find_mut(&mut self, name: &str) -> Option<&mut MetaView> {
        self.views
            .iter_mut()
            .find(|entry| entry.name == name)
            .and_then(|entry| entry.view.as_mut())
    }

    /// Hits a button the way a click on the drawn window does: its
    /// expression first and then its state, each only when it has one,
    /// through `run`. An expression that fails is reported once per
    /// distinct error and does not stop the state change. False when there
    /// is no such button.
    pub fn click(
        &mut self,
        host: &mut dyn MetaHost,
        view: &str,
        control: &str,
        run: &mut dyn FnMut(&mut dyn MetaHost, ButtonAction<'_>) -> Result<(), String>,
    ) -> bool {
        let Some(index) = self
            .find(view)
            .and_then(|parsed| parsed.find(control))
            .filter(|found| found.kind == MetaViewControlKind::Button)
            .map(|found| found.index)
        else {
            return false;
        };
        self.hit_slot(host, view, index, run);
        true
    }

    /// Hits the control a drawn window bound to `slot` (its click), when it
    /// is a button: the button it was drawn from, even when another
    /// control shares its name.
    pub fn click_slot(
        &mut self,
        host: &mut dyn MetaHost,
        view: &str,
        slot: usize,
        run: &mut dyn FnMut(&mut dyn MetaHost, ButtonAction<'_>) -> Result<(), String>,
    ) {
        let is_button = self
            .find(view)
            .and_then(|parsed| parsed.controls().get(slot))
            .is_some_and(|control| control.kind == MetaViewControlKind::Button);
        if is_button {
            self.hit_slot(host, view, slot, run);
        }
    }

    fn hit_slot(
        &mut self,
        host: &mut dyn MetaHost,
        view: &str,
        slot: usize,
        run: &mut dyn FnMut(&mut dyn MetaHost, ButtonAction<'_>) -> Result<(), String>,
    ) {
        let Some(button) = self
            .find(view)
            .and_then(|parsed| parsed.controls().get(slot))
            .cloned()
        else {
            return;
        };
        let mut failure = None;
        if !button.action_expression.is_empty()
            && let Err(error) = run(host, ButtonAction::Expression(&button.action_expression))
        {
            failure = Some(error);
        }
        if !button.set_state.is_empty() {
            let _ = run(host, ButtonAction::SetState(&button.set_state));
        }
        if let Some(failure) = failure {
            self.report_button_failure(host, &failure);
        }
    }

    /// Reports a button's failed expression, once per distinct error.
    pub fn report_button_failure(&mut self, host: &mut dyn MetaHost, failure: &str) {
        let message = format!("Error in Button meta expression: {failure}");
        if self.warnings.insert(message.clone()) {
            host.log(LogLevel::Warn, &message);
        }
    }
}

impl MetaViewControls for MetaViewManager {
    fn view_exists(&self, view: &str) -> bool {
        self.entry(view).is_some()
    }

    fn holds_controls(&self, view: &str) -> bool {
        self.find(view).is_some()
    }

    /// A view is shown as soon as it is created; a drawn one while its
    /// window is, which the player can close.
    fn is_view_visible(&mut self, host: &mut dyn MetaHost, view: &str) -> bool {
        self.holds_controls(view) && (!host.has_ui() || host.ui_view_visible(&window_id(view)))
    }

    fn control_exists(&self, view: &str, control: &str) -> bool {
        self.find(view)
            .and_then(|parsed| parsed.find(control))
            .is_some()
    }

    fn set_control_label(&mut self, view: &str, control: &str, label: &str) -> MetaViewLabelResult {
        let Some(found) = self
            .find_mut(view)
            .and_then(|parsed| parsed.find_mut(control))
        else {
            return MetaViewLabelResult::NotFound;
        };
        if found.kind != MetaViewControlKind::Button {
            return MetaViewLabelResult::TakesNoLabel;
        }
        label.clone_into(&mut found.text);
        MetaViewLabelResult::Set
    }

    fn set_control_visible(&mut self, view: &str, control: &str, visible: bool) -> bool {
        let Some(found) = self
            .find_mut(view)
            .and_then(|parsed| parsed.find_mut(control))
        else {
            return false;
        };
        found.visible = visible;
        true
    }
}

fn register_view(host: &mut dyn MetaHost, name: &str, parsed: &MetaView) -> Option<u32> {
    if !host.has_ui() {
        return None;
    }
    if parsed.controls().len() > view::CAPACITY {
        host.log(
            LogLevel::Warn,
            &format!(
                "MossTank Meta view '{name}' has {} controls; only the first {} are drawn.",
                parsed.controls().len(),
                view::CAPACITY
            ),
        );
    }
    host.register_panel(
        &PanelDescriptor {
            window_id: window_id(name),
            title: view::window_title(parsed),
            icon_text: initials(name),
            start_visible: true,
            show_in_side_panel: true,
        },
        &view::build_markup(parsed),
    )
}

fn register_panel_markup(host: &mut dyn MetaHost, name: &str, markup: &str) -> Option<u32> {
    if !host.has_ui() {
        return None;
    }
    host.register_panel(
        &PanelDescriptor {
            window_id: window_id(name),
            title: name.to_owned(),
            icon_text: initials(name),
            start_visible: true,
            show_in_side_panel: true,
        },
        markup,
    )
}

/// A view's window id: "meta-" and eight bytes of SHA-256 of its name.
#[must_use]
pub fn window_id(name: &str) -> String {
    let hash = Sha256::digest(name.as_bytes());
    let mut id = String::from("meta-");
    for byte in &hash[..8] {
        id.push_str(&format!("{byte:02x}"));
    }
    id
}

/// The shelf icon's text: the first letters of the first two words.
fn initials(name: &str) -> String {
    let letters: String = name
        .split(' ')
        .filter(|word| !word.is_empty())
        .take(2)
        .filter_map(|word| word.chars().next())
        .collect();
    if letters.is_empty() {
        "M".to_owned()
    } else {
        casing::to_upper_invariant(&letters)
    }
}

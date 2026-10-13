//! The windows the meta draws (`MetaViewManager.RegisterView` and
//! `RegisterPanelMarkup`, `StatusHudManager`'s window): each registered
//! with the client through the SDK's plugin UI with the bindings its markup
//! names, kept from the call that made it to the call that takes it down,
//! and given its values the way the panel's windows are
//! ([`crate::panel::Windows`]).
//!
//! The C# hands the client a binding object the window reads every frame:
//! a view's per-control text, visibility and click (`MetaViewBinding`), a
//! panel-markup view's `WindowAvailable` (`PanelMarkupBinding`), and the
//! status window's rows (`StatusBinding`). Here the shell works those
//! values out ([`crate::tank::Tank::meta_window_value`]) and pushes them,
//! and a click comes back as an event (register UI-1).

use std::collections::HashMap;

use ac_plugin_abi::ui::{BindingKind, UiValue};
use mosstank_meta::host::PanelDescriptor;
use openac_plugin::Host;
use openac_plugin::ui::{Panel, PanelSpec};

use crate::panel::{is_handler, schema};

/// The status window's id (`StatusHudManager`).
pub const STATUS_WINDOW_ID: &str = "vtank-meta-status";

/// The meta's windows the client holds.
#[derive(Debug, Default)]
pub struct MetaWindows {
    panels: Vec<(Panel, Vec<(String, BindingKind)>)>,
}

impl MetaWindows {
    /// Registers a window; its panel number.
    ///
    /// # Errors
    /// The markup does not read, or the client refused the window.
    pub fn register(
        &mut self,
        host: &mut Host<'_>,
        descriptor: &PanelDescriptor,
        markup: &str,
    ) -> Result<u32, String> {
        let bindings = schema::bindings(markup)?;
        let binding_type = if descriptor.window_id == STATUS_WINDOW_ID {
            "StatusBinding"
        } else {
            "MetaViewBinding"
        };
        let mut spec = PanelSpec::new(&descriptor.window_id, &descriptor.title, markup)
            .binding_type(binding_type)
            .icon_text(&descriptor.icon_text);
        if !descriptor.start_visible {
            spec = spec.start_hidden();
        }
        if !descriptor.show_in_side_panel {
            spec = spec.off_shelf();
        }
        for (name, kind) in &bindings {
            spec = spec.bind(name, *kind);
        }
        let panel = host.register_panel(&spec)?;
        let id = panel.id();
        self.panels.push((panel, bindings));
        Ok(id)
    }

    /// Takes a window down; nothing when it is not one of these.
    pub fn unregister(&mut self, host: &mut Host<'_>, panel: u32) {
        if let Some(index) = self.panels.iter().position(|(held, _)| held.id() == panel) {
            let (held, _) = self.panels.remove(index);
            held.remove(host);
        }
    }

    /// Whether a panel number is one of these windows.
    #[must_use]
    pub fn owns(&self, panel: u32) -> bool {
        self.panels.iter().any(|(held, _)| held.id() == panel)
    }

    /// The window id of one of these windows.
    #[must_use]
    pub fn window_id(&self, panel: u32) -> Option<&str> {
        self.panels
            .iter()
            .find(|(held, _)| held.id() == panel)
            .map(|(held, _)| held.window_id())
    }

    /// How many windows are held.
    #[must_use]
    pub fn count(&self) -> usize {
        self.panels.len()
    }

    /// Pushes every window's values as `value` works them out for a window
    /// id and a binding; the SDK sends only what changed.
    pub fn push(
        &mut self,
        host: &mut Host<'_>,
        value: &mut dyn FnMut(&str, &str) -> Option<UiValue>,
    ) {
        for (panel, bindings) in &mut self.panels {
            let window_id = panel.window_id().to_owned();
            let mut values: HashMap<&str, UiValue> = HashMap::new();
            for (name, kind) in bindings.iter() {
                if is_handler(*kind) {
                    continue;
                }
                if let Some(found) = value(&window_id, name) {
                    values.insert(name, found);
                }
            }
            for (name, found) in values {
                panel.set(name, found);
            }
            if let Err(error) = panel.flush(host) {
                host.warn(&format!(
                    "MossTank window '{window_id}' refused its values: {error}"
                ));
            }
        }
    }
}

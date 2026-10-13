//! MossTank's twelve windows as the client holds them (`MossTankPlugin`'s
//! `AddPanel` calls): each one's markup, registered with the bindings the
//! markup names, and the values pushed to it from the shell
//! ([`crate::tank::Tank::ui_value`]).

pub mod schema;

use std::collections::HashMap;

use ac_plugin_abi::ui::{BindingKind, UiValue};
use openac_plugin::Host;
use openac_plugin::ui::{Panel, PanelSpec};

/// One window: its id, caption, markup and how it starts.
#[derive(Debug, Clone, Copy)]
pub struct WindowSpec {
    /// The id the client knows it by (`PluginPanelDescriptor.Id`).
    pub id: &'static str,
    /// Its caption.
    pub title: &'static str,
    /// Its markup.
    pub markup: &'static str,
    /// Open when registered.
    pub start_visible: bool,
    /// On the plugin shelf.
    pub on_shelf: bool,
}

macro_rules! markup {
    ($file:literal) => {
        include_str!(concat!(
            "../../../../../src/AcDream.Plugins.MossTank/",
            $file
        ))
    };
}

/// The shelf button's art (a portal.dat render surface).
pub const ICON_SURFACE_ID: u32 = 0x0600_2C41;
/// The shelf button's initials when the art is missing.
pub const ICON_TEXT: &str = "MT";

/// The twelve windows, in the C#'s registration order.
pub const WINDOWS: [WindowSpec; 12] = [
    WindowSpec {
        id: "main",
        title: "MossTank",
        markup: markup!("mosstank.xml"),
        start_visible: false,
        on_shelf: true,
    },
    WindowSpec {
        id: "remote",
        title: "MossTank Remote",
        markup: markup!("mosstank-remote.xml"),
        start_visible: false,
        on_shelf: false,
    },
    WindowSpec {
        id: "action-history",
        title: "MossTank Action History",
        markup: markup!("mosstank-action-history.xml"),
        start_visible: true,
        on_shelf: false,
    },
    WindowSpec {
        id: "advanced-options",
        title: "MossTank Advanced Options",
        markup: markup!("mosstank-advanced.xml"),
        start_visible: true,
        on_shelf: false,
    },
    WindowSpec {
        id: "loot-editor",
        title: "MossTank Loot Editor",
        markup: markup!("mosstank-loot-editor.xml"),
        start_visible: true,
        on_shelf: false,
    },
    WindowSpec {
        id: "buff-picker",
        title: "MossTank Add Buff",
        markup: markup!("mosstank-buffpicker.xml"),
        start_visible: true,
        on_shelf: false,
    },
    WindowSpec {
        id: "meta-editor",
        title: "MossTank Meta Rule Editor",
        markup: markup!("mosstank-metaeditor.xml"),
        start_visible: true,
        on_shelf: false,
    },
    WindowSpec {
        id: "ub-color",
        title: "MossTank Colour",
        markup: markup!("mosstank-ub-color.xml"),
        start_visible: true,
        on_shelf: false,
    },
    WindowSpec {
        id: "ub-list",
        title: "MossTank List",
        markup: markup!("mosstank-ub-list.xml"),
        start_visible: true,
        on_shelf: false,
    },
    WindowSpec {
        id: "ub-map",
        title: "MossTank Map",
        markup: markup!("mosstank-ub-map.xml"),
        start_visible: true,
        on_shelf: false,
    },
    WindowSpec {
        id: "ub-clients",
        title: "MossTank Clients",
        markup: markup!("mosstank-ub-clients.xml"),
        start_visible: true,
        on_shelf: false,
    },
    WindowSpec {
        id: "ub-dungeon",
        title: "MossTank Dungeon",
        markup: markup!("mosstank-ub-dungeon.xml"),
        start_visible: true,
        on_shelf: false,
    },
];

/// The panel spec the client registers for a window.
///
/// # Errors
/// The markup does not read.
pub fn spec(window: &WindowSpec) -> Result<(PanelSpec, Vec<(String, BindingKind)>), String> {
    let bindings = schema::bindings(window.markup)?;
    let mut spec =
        PanelSpec::new(window.id, window.title, window.markup).binding_type("MossTankPanel");
    if window.id == "main" {
        spec = spec.icon_text(ICON_TEXT).icon(ICON_SURFACE_ID);
    }
    if !window.start_visible {
        spec = spec.start_hidden();
    }
    if !window.on_shelf {
        spec = spec.off_shelf();
    }
    for (name, kind) in &bindings {
        spec = spec.bind(name, *kind);
    }
    Ok((spec, bindings))
}

/// How often the windows' values are pushed between events, seconds: the
/// C# windows read their properties every frame; the values here follow
/// within a tenth of a second, and at once after anything the player does
/// (register UI-1).
pub const PUSH_INTERVAL_SECONDS: f64 = 0.1;

/// The registered windows.
#[derive(Debug, Default)]
pub struct Windows {
    panels: Vec<(Panel, Vec<(String, BindingKind)>)>,
    since_push: f64,
}

impl Windows {
    /// Registers every window; the ones the client refuses are left out and
    /// the refusal is logged.
    pub fn register(host: &mut Host<'_>) -> Self {
        let mut panels = Vec::new();
        for window in &WINDOWS {
            let registered = spec(window).and_then(|(spec, bindings)| {
                host.register_panel(&spec).map(|panel| (panel, bindings))
            });
            match registered {
                Ok(panel) => panels.push(panel),
                Err(error) => host.warn(&format!(
                    "MossTank window '{}' was not registered: {error}",
                    window.id
                )),
            }
        }
        Self {
            panels,
            since_push: 0.0,
        }
    }

    /// Whether a panel number is one of these windows.
    #[must_use]
    pub fn owns(&self, panel: u32) -> bool {
        self.panels.iter().any(|(window, _)| window.id() == panel)
    }

    /// The window registered as `window_id`.
    #[must_use]
    pub fn panel(&self, window_id: &str) -> Option<&Panel> {
        self.panels
            .iter()
            .map(|(panel, _)| panel)
            .find(|panel| panel.window_id() == window_id)
    }

    /// Whether a frame of `elapsed_seconds` brings the next push due.
    pub fn push_due(&mut self, elapsed_seconds: f64) -> bool {
        self.since_push += elapsed_seconds.max(0.0);
        self.since_push >= PUSH_INTERVAL_SECONDS
    }

    /// Every value binding the windows name, once each.
    #[must_use]
    pub fn value_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for (_, bindings) in &self.panels {
            for (name, kind) in bindings {
                if !is_handler(*kind) && !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        names
    }

    /// Pushes the values worked out for [`Self::value_names`]; the SDK
    /// sends only what changed.
    pub fn push(&mut self, host: &mut Host<'_>, values: &HashMap<String, UiValue>) {
        self.since_push = 0.0;
        for (panel, bindings) in &mut self.panels {
            for (name, kind) in bindings.iter() {
                if is_handler(*kind) {
                    continue;
                }
                if let Some(value) = values.get(name) {
                    panel.set(name, value.clone());
                }
            }
            if let Err(error) = panel.flush(host) {
                host.warn(&format!(
                    "MossTank window '{}' refused its values: {error}",
                    panel.window_id()
                ));
            }
        }
    }

    /// Removes every window.
    pub fn remove(self, host: &mut Host<'_>) {
        for (panel, _) in self.panels {
            panel.remove(host);
        }
    }
}

/// Whether a binding is a handler rather than a value.
#[must_use]
pub fn is_handler(kind: BindingKind) -> bool {
    matches!(
        kind,
        BindingKind::Action
            | BindingKind::IntAction
            | BindingKind::FloatAction
            | BindingKind::TextAction
    )
}

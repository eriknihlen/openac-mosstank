//! MossTank's twelve windows as the client holds them (`MossTankPlugin`'s
//! `AddPanel` calls): each one's markup, registered with the bindings the
//! markup names, and the values pushed to it from the shell
//! ([`crate::tank::Tank::ui_value`]).

mod bindings_table;
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
    // Read from the table made of the markup (a test holds them equal);
    // parsing every window's markup costs the start tens of millions of
    // instructions.
    let bindings = match bindings_table::of(window.id) {
        Some(table) => table
            .iter()
            .map(|(name, kind)| ((*name).to_owned(), *kind))
            .collect(),
        None => schema::bindings(window.markup)?,
    };
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

/// Values drawn only inside one shown part of a window (a page of the main
/// window, an editor window): the C# windows read a property only to draw
/// it, so these are worked out and pushed only while that part is shown
/// (and once at the start), the costly grids and pickers among them
/// (register UI-1). A test holds each to the markup: every use of the
/// value sits inside the part.
pub const SHOWN_ONLY_WITH: [(&str, &str); 23] = [
    ("MacroProfileNames", "ProfilesVisible"),
    ("RouteProfileNames", "ProfilesVisible"),
    ("SelectedRouteProfile", "ProfilesVisible"),
    ("LootProfileNames", "ProfilesVisible"),
    ("MetaProfileNames", "ProfilesVisible"),
    ("SelectedMetaProfile", "ProfilesVisible"),
    ("RouteWaypointTextColumn", "RouteVisible"),
    ("RouteWaypointCountColumn", "RouteVisible"),
    ("RouteWaypointColors", "RouteVisible"),
    ("MetaDeleteColumn", "MetaVisible"),
    ("MetaMoveUpIcons", "MetaVisible"),
    ("MetaMoveDownIcons", "MetaVisible"),
    ("MetaStateColumn", "MetaVisible"),
    ("MetaConditionColumn", "MetaVisible"),
    ("MetaActionColumn", "MetaVisible"),
    ("MetaCurrentStateNames", "MetaVisible"),
    ("UbProfileNames", "UbSettingsVisible"),
    ("UbSettingNames", "UbSettingsVisible"),
    ("UbSettingValues", "UbSettingsVisible"),
    ("LootRuleRows", "LootEditorVisible"),
    ("BuffPickerRows", "BuffPickerVisible"),
    ("AdvancedOptionNames", "AdvancedOptionsVisible"),
    ("AdvancedOptionValueColumn", "AdvancedOptionsVisible"),
];

/// The value a binding of `kind` has before anything is worked out for it:
/// false, zero, empty. `None` for a handler.
#[must_use]
pub fn empty_value(kind: BindingKind) -> Option<UiValue> {
    Some(match kind {
        BindingKind::Bool => UiValue::Bool(false),
        BindingKind::Int => UiValue::Int(0),
        BindingKind::Uint => UiValue::Uint(0),
        BindingKind::Float => UiValue::Float(0.0),
        BindingKind::Text => UiValue::Text(String::new()),
        BindingKind::TextList => UiValue::TextList(Vec::new()),
        BindingKind::UintList => UiValue::UintList(Vec::new()),
        BindingKind::IntList => UiValue::IntList(Vec::new()),
        BindingKind::BoolList => UiValue::BoolList(Vec::new()),
        _ => return None,
    })
}

/// The shown part `name` is drawn in, when it is one of
/// [`SHOWN_ONLY_WITH`].
#[must_use]
pub fn shown_only_with(name: &str) -> Option<&'static str> {
    SHOWN_ONLY_WITH
        .iter()
        .find(|(value, _)| *value == name)
        .map(|(_, shown)| *shown)
}

/// The registered windows.
#[derive(Debug, Default)]
pub struct Windows {
    panels: Vec<(Panel, Vec<(String, BindingKind)>)>,
    /// Every value binding the windows name, once each ([`Self::value_names`]).
    names: std::rc::Rc<[String]>,
    /// For each window's bindings, whether no later window names it: the
    /// value pushed is handed over there instead of copied.
    last_use: Vec<Vec<bool>>,
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
        let names = value_names(&panels).into();
        let last_use = last_uses(&panels);
        Self {
            panels,
            names,
            last_use,
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

    /// The kind a value binding is declared with.
    #[must_use]
    pub fn kind_of(&self, name: &str) -> Option<BindingKind> {
        self.panels
            .iter()
            .flat_map(|(_, bindings)| bindings)
            .find(|(binding, _)| binding == name)
            .map(|(_, kind)| *kind)
    }

    /// Every value binding the windows name, once each, in first-use order
    /// (worked out when the windows are registered).
    #[must_use]
    pub fn value_names(&self) -> std::rc::Rc<[String]> {
        std::rc::Rc::clone(&self.names)
    }

    /// Pushes the values worked out for [`Self::value_names`]; the SDK
    /// sends only what changed. A binding with no value keeps the one pushed
    /// before. Whether every window took its values.
    pub fn push(&mut self, host: &mut Host<'_>, mut values: HashMap<&str, UiValue>) -> bool {
        self.since_push = 0.0;
        let mut taken = true;
        for ((panel, bindings), last_use) in self.panels.iter_mut().zip(&self.last_use) {
            for ((name, kind), &last) in bindings.iter().zip(last_use) {
                if is_handler(*kind) {
                    continue;
                }
                let value = if last {
                    values.remove(name.as_str())
                } else {
                    values.get(name.as_str()).cloned()
                };
                if let Some(value) = value {
                    panel.set(name, value);
                }
            }
            if let Err(error) = panel.flush(host) {
                taken = false;
                host.warn(&format!(
                    "MossTank window '{}' refused its values: {error}",
                    panel.window_id()
                ));
            }
        }
        taken
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

/// Every value binding `panels` name, once each, in first-use order.
fn value_names(panels: &[(Panel, Vec<(String, BindingKind)>)]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (_, bindings) in panels {
        for (name, kind) in bindings {
            if !is_handler(*kind) && !names.contains(name) {
                names.push(name.clone());
            }
        }
    }
    names
}

/// For each of `panels`' bindings, whether no later panel names it.
fn last_uses(panels: &[(Panel, Vec<(String, BindingKind)>)]) -> Vec<Vec<bool>> {
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut last_use: Vec<Vec<bool>> = vec![Vec::new(); panels.len()];
    for (index, (_, bindings)) in panels.iter().enumerate().rev() {
        let mut flags: Vec<bool> = bindings
            .iter()
            .rev()
            .map(|(name, _)| seen.insert(name.as_str()))
            .collect();
        flags.reverse();
        last_use[index] = flags;
    }
    last_use
}

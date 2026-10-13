//! A meta view (`MetaView`, `MetaViewBinding`): the view XML a CreateView
//! action carries, parsed into its controls, and the panel markup a window
//! host draws it from. The view is its whole state, drawn or not: a host
//! without a window keeps the same controls and answers the same queries.

use mosstank_dotnet::compat::text::casing;
use openac_plugin_compat::text::ordinal;

/// What a control is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaViewControlKind {
    /// A fixed layout placing its children at pixel offsets.
    Layout,
    /// A button that runs an expression and/or changes the meta state.
    Button,
}

/// One control of a view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaViewControl {
    /// What it is.
    pub kind: MetaViewControlKind,
    /// Its name (names need not be unique).
    pub name: String,
    /// Left offset, pixels.
    pub left: i32,
    /// Top offset, pixels.
    pub top: i32,
    /// Width, pixels.
    pub width: i32,
    /// Height, pixels.
    pub height: i32,
    /// A button's expression.
    pub action_expression: String,
    /// A button's state.
    pub set_state: String,
    /// The indices (into [`MetaView::controls`]) of a layout's children.
    pub children: Vec<usize>,
    /// Its position in [`MetaView::controls`]: the drawn window binds each
    /// control's text, visibility and click by this number.
    pub index: usize,
    /// Its text (a button's label).
    pub text: String,
    /// Whether it is shown.
    pub visible: bool,
}

/// A parsed view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaView {
    /// Its title.
    pub title: String,
    /// Width, pixels.
    pub width: i32,
    /// Height, pixels.
    pub height: i32,
    /// The index of the first child of the view element, when it is a known
    /// control.
    pub root: Option<usize>,
    controls: Vec<MetaViewControl>,
    by_name: Vec<(String, usize)>,
}

impl MetaView {
    /// Every parsed control in document order, children before their
    /// layout's next sibling.
    #[must_use]
    pub fn controls(&self) -> &[MetaViewControl] {
        &self.controls
    }

    /// The root control.
    #[must_use]
    pub fn root_control(&self) -> Option<&MetaViewControl> {
        self.root.map(|index| &self.controls[index])
    }

    /// The control a name refers to: when two share one, the one parsed
    /// last, and a layout is parsed after its own children.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&MetaViewControl> {
        self.find_index(name).map(|index| &self.controls[index])
    }

    /// [`Self::find`], to change.
    pub fn find_mut(&mut self, name: &str) -> Option<&mut MetaViewControl> {
        self.find_index(name).map(|index| &mut self.controls[index])
    }

    fn find_index(&self, name: &str) -> Option<usize> {
        self.by_name
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, index)| *index)
    }

    /// Parses view XML. Malformed XML or a view element with no child node
    /// gives the reason; unknown elements and control types are skipped
    /// with everything beneath them.
    ///
    /// # Errors
    /// Why the view cannot be read.
    pub fn parse(xml: &str) -> Result<Self, String> {
        let document = roxmltree::Document::parse(xml)
            .map_err(|error| format!("Malformed XML in Meta View. {error}"))?;
        let element = document.root_element();
        let Some(first) = element.children().find(|node| is_node(*node)) else {
            return Err("The view element has no control.".to_owned());
        };
        let mut view = Self {
            title: attribute_string(element, "title"),
            width: attribute_int(element, "width"),
            height: attribute_int(element, "height"),
            root: None,
            controls: Vec::new(),
            by_name: Vec::new(),
        };
        view.root = view.parse_control(first);
        Ok(view)
    }

    fn parse_control(&mut self, node: roxmltree::Node<'_, '_>) -> Option<usize> {
        if !node.is_element() || !ordinal::equals_ignore_case(node.tag_name().name(), "control") {
            return None;
        }
        let kind = match casing::to_upper_invariant(&attribute_string(node, "type")).as_str() {
            "BUTTON" => MetaViewControlKind::Button,
            "LAYOUT" => MetaViewControlKind::Layout,
            _ => return None,
        };
        let button = kind == MetaViewControlKind::Button;
        let index = self.controls.len();
        self.controls.push(MetaViewControl {
            kind,
            name: attribute_string(node, "name"),
            left: attribute_int(node, "left"),
            top: attribute_int(node, "top"),
            width: attribute_int(node, "width"),
            height: attribute_int(node, "height"),
            action_expression: if button {
                attribute_string(node, "actionexpr")
            } else {
                String::new()
            },
            set_state: if button {
                attribute_string(node, "setstate")
            } else {
                String::new()
            },
            children: Vec::new(),
            index,
            text: if button {
                attribute_string(node, "text")
            } else {
                String::new()
            },
            visible: true,
        });
        if kind == MetaViewControlKind::Layout {
            for child in node.children() {
                if let Some(parsed) = self.parse_control(child) {
                    self.controls[index].children.push(parsed);
                }
            }
        }
        let name = self.controls[index].name.clone();
        if !name.is_empty() {
            match self
                .by_name
                .iter_mut()
                .find(|(candidate, _)| *candidate == name)
            {
                Some(entry) => entry.1 = index,
                None => self.by_name.push((name, index)),
            }
        }
        Some(index)
    }
}

/// A node `XDocument.Parse` keeps: it drops whitespace-only text.
fn is_node(node: roxmltree::Node<'_, '_>) -> bool {
    match node.node_type() {
        roxmltree::NodeType::Text => node
            .text()
            .is_some_and(|text| !text.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n'))),
        _ => true,
    }
}

fn attribute_string(node: roxmltree::Node<'_, '_>, name: &str) -> String {
    node.attribute(name).unwrap_or_default().to_owned()
}

fn attribute_int(node: roxmltree::Node<'_, '_>, name: &str) -> i32 {
    node.attribute(name)
        .and_then(mosstank_dotnet::try_parse_int)
        .unwrap_or(0)
}

/// Whether markup is the host's own panel markup (its root a `panel`).
#[must_use]
pub fn is_panel_markup(markup: &str) -> bool {
    roxmltree::Document::parse(markup).is_ok_and(|document| {
        ordinal::equals_ignore_case(document.root_element().tag_name().name(), "panel")
    })
}

/// How many controls a drawn view binds.
pub const CAPACITY: usize = 128;

/// Pixels between the window's sides and the view's content.
pub const SIDE_MARGIN: i32 = 4;

/// Pixels above the view's content, which the window's title occupies.
pub const TITLE_HEIGHT: i32 = 24;

/// The drawn window's title.
#[must_use]
pub fn window_title(view: &MetaView) -> String {
    format!("Meta - {}", view.title)
}

/// The panel markup for a view: each fixed layout a group at its offset,
/// each button a button at its own, a title strip above and a margin at
/// the sides. Controls past [`CAPACITY`] are not drawn.
#[must_use]
pub fn build_markup(view: &MetaView) -> String {
    let mut out = format!(
        "<panel x=\"320\" y=\"120\" w=\"{}\" h=\"{}\" title=\"{}\"",
        view.width + 2 * SIDE_MARGIN,
        view.height + TITLE_HEIGHT + SIDE_MARGIN,
        escape_attribute(&window_title(view))
    );
    match view.root {
        Some(root) => {
            out.push('>');
            // The root control fills the view whatever bounds it declares.
            add_control(
                &mut out,
                view,
                root,
                (SIDE_MARGIN, TITLE_HEIGHT, view.width, view.height),
            );
            out.push_str("</panel>");
        }
        None => out.push_str(" />"),
    }
    out
}

fn add_control(out: &mut String, view: &MetaView, index: usize, bounds: (i32, i32, i32, i32)) {
    let control = &view.controls[index];
    if control.index >= CAPACITY {
        return;
    }
    let tag = match control.kind {
        MetaViewControlKind::Layout => "group",
        MetaViewControlKind::Button => "button",
    };
    out.push('<');
    out.push_str(tag);
    if !control.name.is_empty() {
        out.push_str(&format!(" name=\"{}\"", escape_attribute(&control.name)));
    }
    let (x, y, w, h) = bounds;
    out.push_str(&format!(" x=\"{x}\" y=\"{y}\" w=\"{w}\" h=\"{h}\""));
    if control.kind == MetaViewControlKind::Button {
        out.push_str(&format!(
            " text=\"{{Text{0}}}\" onclick=\"{{Click{0}}}\"",
            control.index
        ));
    }
    out.push_str(&format!(" visible=\"{{Visible{}}}\"", control.index));
    let drawn: Vec<usize> = control
        .children
        .iter()
        .copied()
        .filter(|child| view.controls[*child].index < CAPACITY)
        .collect();
    if drawn.is_empty() {
        out.push_str(" />");
        return;
    }
    out.push('>');
    for child in drawn {
        let c = &view.controls[child];
        add_control(out, view, child, (c.left, c.top, c.width, c.height));
    }
    out.push_str(&format!("</{tag}>"));
}

/// An attribute value as `XElement.ToString` writes it.
fn escape_attribute(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\r' => out.push_str("&#xD;"),
            '\n' => out.push_str("&#xA;"),
            '\t' => out.push_str("&#x9;"),
            other => out.push(other),
        }
    }
    out
}

/// What a drawn view's slot `slot` shows: the control's text.
#[must_use]
pub fn slot_text(view: &MetaView, slot: usize) -> String {
    view.controls
        .get(slot)
        .map(|control| control.text.clone())
        .unwrap_or_default()
}

/// Whether a drawn view's slot `slot` is shown.
#[must_use]
pub fn slot_visible(view: &MetaView, slot: usize) -> bool {
    view.controls
        .get(slot)
        .is_some_and(|control| control.visible)
}

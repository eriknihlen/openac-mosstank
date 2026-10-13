//! The bindings a window's markup names, each with the kind the attribute
//! it appears in takes: the C# panel declared them by its properties'
//! types, and the markup and those types agree, so the kinds can be read
//! back from where each name is used.

use ac_plugin_abi::ui::BindingKind;

/// `{Name}` → `Name`.
fn binding(value: &str) -> Option<&str> {
    let inner = value.strip_prefix('{')?.strip_suffix('}')?;
    (!inner.is_empty()).then_some(inner)
}

/// The kind a value attribute's binding takes on an element.
fn value_kind(element: &str, attribute: &str, column_type: Option<&str>) -> Option<BindingKind> {
    use BindingKind as K;
    Some(match (element, attribute) {
        (_, "text" | "tooltip" | "title") => K::Text,
        (_, "color" | "background" | "border" | "icon") => K::Uint,
        (_, "visible" | "enabled" | "checked") => K::Bool,
        ("tab", "selected") => K::Bool,
        ("list", "selected") => K::Int,
        ("menu", "selected") => K::Text,
        ("slider", "value" | "min" | "max") => K::Float,
        ("meter", "fill") => K::Float,
        ("log", "firstindex") => K::Int,
        (_, "items") => K::TextList,
        (_, "colors" | "icons") => K::UintList,
        ("column", "values") => {
            if column_type == Some("check") {
                K::BoolList
            } else {
                K::UintList
            }
        }
        _ => return None,
    })
}

/// The kind a handler attribute's binding takes on an element.
fn handler_kind(element: &str, attribute: &str) -> Option<BindingKind> {
    use BindingKind as K;
    Some(match (element, attribute) {
        ("button" | "tab" | "toggle", "onclick") => K::Action,
        ("field", "onup" | "ondown") => K::Action,
        ("field", "onchange" | "onsubmit" | "onblur") => K::TextAction,
        ("menu", "onchange") => K::TextAction,
        ("slider", "onchange") => K::FloatAction,
        ("list", "onchange") => K::IntAction,
        ("column", "onclick" | "onchange") => K::IntAction,
        _ => return None,
    })
}

/// Every binding the markup names, in first-use order, with its kind.
///
/// # Errors
/// The markup is not XML, a binding sits in an attribute that takes none,
/// or one name is used as two kinds.
pub fn bindings(markup: &str) -> Result<Vec<(String, BindingKind)>, String> {
    let document = roxmltree::Document::parse(markup).map_err(|error| error.to_string())?;
    let mut found: Vec<(String, BindingKind)> = Vec::new();
    for node in document.descendants().filter(roxmltree::Node::is_element) {
        let element = node.tag_name().name();
        let column_type = node.attribute("type");
        for attribute in node.attributes() {
            let Some(name) = binding(attribute.value()) else {
                continue;
            };
            let kind = handler_kind(element, attribute.name())
                .or_else(|| value_kind(element, attribute.name(), column_type))
                .ok_or_else(|| {
                    format!(
                        "<{element} {}=\"{}\"> takes no binding",
                        attribute.name(),
                        attribute.value()
                    )
                })?;
            match found.iter().find(|(seen, _)| seen == name) {
                Some((_, seen)) if *seen != kind => {
                    return Err(format!("{{{name}}} is used as {seen:?} and as {kind:?}"));
                }
                Some(_) => {}
                None => found.push((name.to_owned(), kind)),
            }
        }
    }
    Ok(found)
}

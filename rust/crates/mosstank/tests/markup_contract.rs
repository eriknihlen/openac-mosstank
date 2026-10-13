//! The windows' markup, read as the C# tests read it: the same twelve files
//! the plugin registers, checked for the layout and binding rules the C#
//! markup contract holds them to.

use mosstank::panel::{WINDOWS, schema};
use roxmltree::{Document, Node};

use ac_plugin_abi::ui::BindingKind;

fn markup(file: &str) -> &'static str {
    let id = match file {
        "mosstank.xml" => "main",
        "mosstank-remote.xml" => "remote",
        "mosstank-action-history.xml" => "action-history",
        "mosstank-advanced.xml" => "advanced-options",
        "mosstank-loot-editor.xml" => "loot-editor",
        "mosstank-buffpicker.xml" => "buff-picker",
        "mosstank-metaeditor.xml" => "meta-editor",
        "mosstank-ub-color.xml" => "ub-color",
        "mosstank-ub-list.xml" => "ub-list",
        "mosstank-ub-map.xml" => "ub-map",
        "mosstank-ub-clients.xml" => "ub-clients",
        "mosstank-ub-dungeon.xml" => "ub-dungeon",
        other => panic!("no window reads {other}"),
    };
    WINDOWS
        .iter()
        .find(|window| window.id == id)
        .unwrap()
        .markup
}

/// Every markup file, by its file name.
const FILES: [&str; 12] = [
    "mosstank-action-history.xml",
    "mosstank-advanced.xml",
    "mosstank-buffpicker.xml",
    "mosstank-loot-editor.xml",
    "mosstank-metaeditor.xml",
    "mosstank-remote.xml",
    "mosstank-ub-clients.xml",
    "mosstank-ub-color.xml",
    "mosstank-ub-dungeon.xml",
    "mosstank-ub-list.xml",
    "mosstank-ub-map.xml",
    "mosstank.xml",
];

fn document(file: &str) -> Document<'static> {
    Document::parse(markup(file)).unwrap()
}

const INTERACTIVE_ELEMENT_NAMES: [&str; 8] = [
    "tab", "button", "toggle", "slider", "field", "menu", "list", "column",
];

fn name<'a>(node: Node<'a, '_>) -> &'a str {
    node.tag_name().name()
}

fn attr<'a>(node: Node<'a, '_>, attribute: &str) -> Option<&'a str> {
    node.attribute(attribute)
}

fn number(node: Node<'_, '_>, attribute: &str) -> f32 {
    attr(node, attribute)
        .and_then(|text| text.trim().parse::<f32>().ok())
        .unwrap_or(0.0)
}

fn elements<'a, 'i>(node: Node<'a, 'i>) -> impl Iterator<Item = Node<'a, 'i>> {
    node.children().filter(Node::is_element)
}

fn named<'a, 'i>(node: Node<'a, 'i>, tag: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    elements(node).filter(move |child| name(*child) == tag)
}

fn descendants_named<'a, 'i>(
    node: Node<'a, 'i>,
    tag: &'static str,
) -> impl Iterator<Item = Node<'a, 'i>> {
    node.descendants()
        .filter(move |child| child.is_element() && name(*child) == tag)
}

fn page<'a, 'i>(root: Node<'a, 'i>, visible: &str) -> Node<'a, 'i> {
    let binding = format!("{{{visible}}}");
    let pages: Vec<_> = named(root, "group")
        .filter(|group| attr(*group, "visible") == Some(binding.as_str()))
        .collect();
    assert_eq!(pages.len(), 1, "{visible}");
    pages[0]
}

fn text<'a>(node: Node<'a, '_>) -> &'a str {
    attr(node, "text").unwrap_or("")
}

/// The kind the plugin registers for a binding of this window.
fn kinds(file: &str) -> Vec<(String, BindingKind)> {
    schema::bindings(markup(file)).unwrap()
}

fn kind(kinds: &[(String, BindingKind)], binding: &str) -> BindingKind {
    kinds
        .iter()
        .find(|(name, _)| name == binding)
        .map(|(_, kind)| *kind)
        .unwrap_or_else(|| panic!("{binding} is not registered"))
}

fn binding_name(expression: &str) -> &str {
    assert!(
        expression.starts_with('{') && expression.ends_with('}'),
        "{expression}"
    );
    &expression[1..expression.len() - 1]
}

fn effective_height(node: Node<'_, '_>) -> f32 {
    let declared = number(node, "h");
    if declared > 0.0 {
        return declared;
    }
    match name(node) {
        "label" | "field" | "button" => 16.0,
        "toggle" => 20.0,
        _ => 0.0,
    }
}

/// The children that cross their parent's right or bottom edge.
fn outside_parent(parent: Node<'_, '_>) -> Vec<String> {
    let mut problems = Vec::new();
    let parent_width = number(parent, "w");
    let parent_height = number(parent, "h");
    for child in elements(parent) {
        let width = number(child, "w");
        let height = effective_height(child);
        if width > 0.0 && number(child, "x") + width > parent_width {
            problems.push(format!(
                "<{}> crosses the right edge of <{}>.",
                name(child),
                name(parent)
            ));
        }
        if height > 0.0 && number(child, "y") + height > parent_height {
            problems.push(format!(
                "<{}> crosses the bottom edge of <{}>.",
                name(child),
                name(parent)
            ));
        }
        problems.extend(outside_parent(child));
    }
    problems
}

fn rectangles_overlap(a: Node<'_, '_>, b: Node<'_, '_>) -> bool {
    let (aw, ah) = (number(a, "w"), number(a, "h"));
    let (bw, bh) = (number(b, "w"), number(b, "h"));
    if aw <= 0.0 || ah <= 0.0 || bw <= 0.0 || bh <= 0.0 {
        return false;
    }
    let (ax, ay) = (number(a, "x"), number(a, "y"));
    let (bx, by) = (number(b, "x"), number(b, "y"));
    ax < bx + bw && bx < ax + aw && ay < by + bh && by < ay + ah
}

/// The sibling pairs that overlap (two group pages excepted).
fn sibling_overlaps(container: Node<'_, '_>) -> Vec<String> {
    let children: Vec<_> = elements(container)
        .filter(|child| name(*child) != "column")
        .collect();
    let mut problems = Vec::new();
    for (i, a) in children.iter().enumerate() {
        for b in &children[i + 1..] {
            if name(*a) == "group" && name(*b) == "group" {
                continue;
            }
            if rectangles_overlap(*a, *b) {
                problems.push(format!(
                    "<{}> text='{}' overlaps sibling <{}> text='{}'",
                    name(*a),
                    text(*a),
                    name(*b),
                    text(*b)
                ));
            }
        }
    }
    for child in children {
        problems.extend(sibling_overlaps(child));
    }
    problems
}

/// The handler kinds the client's retained UI calls an element's handler
/// attributes with (`Action`, `Action<int>`, `Action<float>`,
/// `Action<string>`); mismatches named.
fn delegate_shape_mismatches(node: Node<'_, '_>, kinds: &[(String, BindingKind)]) -> Vec<String> {
    let mut expected: Vec<(&str, BindingKind)> = Vec::new();
    if name(node) == "column" {
        expected.push(("onchange", BindingKind::IntAction));
        expected.push(("onclick", BindingKind::IntAction));
    } else {
        expected.push(("onclick", BindingKind::Action));
        expected.push(("onsubmit", BindingKind::TextAction));
        match name(node) {
            "field" | "menu" => expected.push(("onchange", BindingKind::TextAction)),
            "slider" => expected.push(("onchange", BindingKind::FloatAction)),
            "list" => expected.push(("onchange", BindingKind::IntAction)),
            _ => {}
        }
    }
    let mut problems = Vec::new();
    for (attribute, wanted) in expected {
        let Some(expression) = attr(node, attribute) else {
            continue;
        };
        let binding = binding_name(expression);
        let actual = kind(kinds, binding);
        if actual != wanted {
            problems.push(format!(
                "<{}> {attribute}={expression}: {actual:?}, not {wanted:?}",
                name(node)
            ));
        }
    }
    problems
}

fn horizontal_anchor(node: Node<'_, '_>) -> (bool, bool) {
    let anchor = attr(node, "anchor").unwrap_or("");
    let parts: Vec<String> = anchor
        .split(' ')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    let left = parts.iter().any(|part| part == "left");
    let right = parts.iter().any(|part| part == "right");
    if !left && !right {
        (true, false)
    } else {
        (left, right)
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: InteractiveElementNames_IncludesColumn
#[test]
fn interactive_element_names_include_column() {
    assert!(INTERACTIVE_ELEMENT_NAMES.contains(&"column"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: TheExtraVulnerabilityHeadingExplainsThatItCastsWithoutTheVulnCheck
#[test]
fn the_extra_vulnerability_heading_explains_that_it_casts_without_the_vuln_check() {
    let document = document("mosstank.xml");
    let headings: Vec<_> = descendants_named(document.root_element(), "label")
        .filter(|label| text(*label) == "Ex. Vuln")
        .collect();
    assert_eq!(headings.len(), 1);
    let tooltip = attr(headings[0], "tooltip").unwrap_or("");
    assert!(tooltip.contains("None"));
    assert!(tooltip.contains("Auto"));
    assert!(tooltip.contains("does NOT need V ticked"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: VtankTabOrderAndEveryBindingResolveAgainstTheLivePanel
#[test]
fn vtank_tab_order_and_every_binding_resolves() {
    let document = document("mosstank.xml");
    let root = document.root_element();
    let tabs: Vec<&str> = named(root, "tab").map(text).collect();
    assert_eq!(
        tabs,
        [
            "Options",
            "Profiles",
            "Vitals",
            "Monsters",
            "Items",
            "Consumables",
            "Buffs",
            "Route",
            "Meta",
            "UB",
            "UB settings",
            "Tinker",
        ]
    );
    // Each binding is one whole `{Name}` and is registered; that the shell
    // answers every one is `windows.rs`.
    let kinds = kinds("mosstank.xml");
    for node in root.descendants().filter(Node::is_element) {
        for attribute in node.attributes() {
            if !attribute.value().contains('{') {
                continue;
            }
            let binding = binding_name(attribute.value());
            assert!(!binding.contains(['{', '}']), "{}", attribute.value());
            kind(&kinds, binding);
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: EveryInteractiveBindingMatchesTheRetainedUiDelegateShape
#[test]
fn every_interactive_binding_matches_the_retained_ui_delegate_shape() {
    let document = document("mosstank.xml");
    let kinds = kinds("mosstank.xml");
    let problems: Vec<String> = document
        .root_element()
        .descendants()
        .filter(Node::is_element)
        .flat_map(|node| delegate_shape_mismatches(node, &kinds))
        .collect();
    assert!(problems.is_empty(), "{problems:#?}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: Column_OnchangeAndOnclick_MustBeActionOfInt
#[test]
fn a_columns_onchange_and_onclick_are_row_actions() {
    let good = r#"<panel w="100" h="100"><list x="0" y="0" w="100" h="100" items="{Rows}"><column type="check" onchange="{RowAction}" /></list></panel>"#;
    let kinds = schema::bindings(good).unwrap();
    assert_eq!(kind(&kinds, "RowAction"), BindingKind::IntAction);
    let document = Document::parse(good).unwrap();
    let column = descendants_named(document.root_element(), "column")
        .next()
        .unwrap();
    assert!(delegate_shape_mismatches(column, &kinds).is_empty());

    // A plain action on a column is the mismatch the check names.
    let bad = r#"<panel w="100" h="100"><list x="0" y="0" w="100" h="100" items="{Rows}"><column type="icon" onclick="{PlainAction}" /></list></panel>"#;
    let document = Document::parse(bad).unwrap();
    let column = descendants_named(document.root_element(), "column")
        .next()
        .unwrap();
    let kinds = vec![("PlainAction".to_owned(), BindingKind::Action)];
    assert_eq!(delegate_shape_mismatches(column, &kinds).len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: EveryInteractiveControlDeclaresARealHandlerBinding
#[test]
fn every_interactive_control_declares_a_real_handler_binding() {
    let document = document("mosstank.xml");
    let controls: Vec<_> = document
        .root_element()
        .descendants()
        .filter(|node| node.is_element() && INTERACTIVE_ELEMENT_NAMES.contains(&name(*node)))
        .filter(|node| name(*node) != "field" || attr(*node, "editable") != Some("false"))
        .collect();
    assert!(!controls.is_empty());
    for control in controls {
        let handler = attr(control, "onclick")
            .or_else(|| attr(control, "onchange"))
            .or_else(|| attr(control, "onsubmit"));
        assert!(
            handler.is_some(),
            "<{}> text='{}'",
            name(control),
            text(control)
        );
        assert_ne!(attr(control, "enabled"), Some("false"));
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: MonstersGridHasVtanksTwentyThreeColumnsInOrderWithAuthenticHeaderTooltips
#[test]
fn the_monsters_grid_has_vtanks_columns_in_order_with_authentic_heading_tooltips() {
    let document = document("mosstank.xml");
    let group = page(document.root_element(), "MonstersVisible");
    let lists: Vec<_> = named(group, "list").collect();
    assert_eq!(lists.len(), 1);
    let types: Vec<&str> = named(lists[0], "column")
        .map(|column| attr(column, "type").unwrap_or(""))
        .collect();
    let mut expected = vec!["check"; 14];
    expected.extend(["text"; 7]);
    expected.extend(["icon", "icon", "text"]);
    assert_eq!(types, expected);

    let tooltip = |caption: &str| {
        named(group, "label")
            .filter(|label| text(*label) == caption)
            .last()
            .and_then(|label| attr(label, "tooltip"))
    };
    for (caption, expected) in [
        ("F", "Fester"),
        ("B", "Broadside of a Barn"),
        ("G", "Gravity Well"),
        ("I", "Imperil"),
        ("Y", "Yield"),
        ("V", "Vuln (Element)"),
        ("A", "Attack"),
        ("R", "Ring Spell"),
        ("S", "Streak"),
        ("WC", "Weakening Curse"),
        ("FC", "Festering Curse"),
        ("Cp", "Corruption"),
        ("DC", "Destructive Curse"),
        ("Cs", "Corrosion"),
        ("P", "Priority"),
    ] {
        assert_eq!(tooltip(caption), Some(expected), "{caption}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: ItemsTabIsVtankOnlyPlusTheAcceptedSliceOneRemoveButton
#[test]
fn the_items_tab_is_vtanks_plus_the_remove_button() {
    let document = document("mosstank.xml");
    let group = page(document.root_element(), "ItemsVisible");
    assert_eq!(named(group, "list").count(), 1);
    let buttons: Vec<&str> = named(group, "button").map(text).collect();
    assert_eq!(buttons, ["Add", "Add (no buffs)", "Remove"]);
    assert_eq!(named(group, "toggle").count(), 0);
    assert_eq!(named(group, "slider").count(), 0);
    let labels: Vec<&str> = named(group, "label").map(text).collect();
    for absent in [
        "{MonsterEquipmentText}",
        "{RefillWornManaText}",
        "{ItemManaRechargeStatus}",
        "{ProfileNotice}",
    ] {
        assert!(!labels.contains(&absent), "{absent}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: BuffsTabIsVtankOnlySixControls
#[test]
fn the_buffs_tab_is_vtanks_six_controls() {
    let document = document("mosstank.xml");
    let group = page(document.root_element(), "BuffsVisible");
    assert_eq!(named(group, "list").count(), 2);
    let buttons: Vec<&str> = named(group, "button").map(text).collect();
    assert_eq!(buttons, ["Add...", "Add..."]);
    let labels: Vec<&str> = named(group, "label").map(text).collect();
    assert_eq!(labels, ["Extra Buff Spells", "Blacklisted Buff Families"]);
    assert_eq!(named(group, "toggle").count(), 0);
    assert_eq!(named(group, "slider").count(), 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: LootEditorHasNoLeftoverBackButton
#[test]
fn the_loot_editor_has_no_leftover_back_button() {
    let document = document("mosstank-loot-editor.xml");
    assert!(!named(document.root_element(), "button").any(|button| text(button) == "Back"));
    // The panel still closes it (`CloseLootEditor`): `windows.rs`.
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: AuthoredShellFitsTheMinimumCanvasAndEverySizedChildFitsItsParent
#[test]
fn the_authored_shell_fits_and_every_sized_child_fits_its_parent() {
    let document = document("mosstank.xml");
    let root = document.root_element();
    assert_eq!(number(root, "w"), 984.0);
    assert_eq!(number(root, "h"), 271.0);
    let problems = outside_parent(root);
    assert!(problems.is_empty(), "{problems:#?}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: TheWindowResizesUpwardsFromItsAuthoredSizeAndNoSmaller
#[test]
fn the_window_resizes_upwards_from_its_authored_size_and_no_smaller() {
    let document = document("mosstank.xml");
    let root = document.root_element();
    assert_eq!(attr(root, "resizable"), Some("true"));
    assert_eq!(number(root, "w"), number(root, "minw"));
    assert_eq!(number(root, "h"), number(root, "minh"));
    assert_eq!(attr(root, "resize"), Some("both"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: EveryPageOfTheWindowHoldsAllFourEdges
#[test]
fn every_page_of_the_window_holds_all_four_edges() {
    let document = document("mosstank.xml");
    let pages: Vec<_> = named(document.root_element(), "group")
        .filter(|group| attr(*group, "visible").is_some_and(|v| v.ends_with("Visible}")))
        .collect();
    assert_eq!(pages.len(), 12);
    for page in pages {
        let anchor = attr(page, "anchor").unwrap_or("").to_ascii_lowercase();
        for edge in ["left", "top", "right", "bottom"] {
            assert!(
                anchor.contains(edge),
                "{:?} does not hold its {edge} edge",
                attr(page, "visible")
            );
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: EveryRowOnAPagesBottomEdgeIsAnchoredToThatEdge
#[test]
fn every_row_on_a_pages_bottom_edge_is_anchored_to_that_edge() {
    let document = document("mosstank.xml");
    let mut stranded = Vec::new();
    for page in named(document.root_element(), "group") {
        let page_height = number(page, "h");
        for child in elements(page) {
            if attr(child, "h").is_none() || attr(child, "y").is_none() {
                continue;
            }
            let bottom = number(child, "y") + number(child, "h");
            if bottom < page_height - 24.0 {
                continue;
            }
            let anchor = attr(child, "anchor").unwrap_or("").to_ascii_lowercase();
            if !anchor.contains("bottom") {
                stranded.push(format!(
                    "{:?} <{} text=\"{}\"> at y={}",
                    attr(page, "visible"),
                    name(child),
                    text(child),
                    number(child, "y")
                ));
            }
        }
    }
    assert!(stranded.is_empty(), "{stranded:#?}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: EveryStretchingListDeclaresARealAnchor
#[test]
fn every_stretching_list_declares_a_real_anchor() {
    let document = document("mosstank.xml");
    for (visible, expected) in [
        ("MonstersVisible", 1),
        ("ItemsVisible", 1),
        ("ConsumablesVisible", 2),
        ("BuffsVisible", 2),
        ("RouteVisible", 1),
        ("MetaVisible", 1),
        ("UbSettingsVisible", 2),
    ] {
        let group = page(document.root_element(), visible);
        let lists: Vec<_> = named(group, "list").collect();
        assert_eq!(lists.len(), expected, "{visible}");
        for list in lists {
            assert!(
                attr(list, "anchor").is_some_and(|anchor| !anchor.is_empty()),
                "A <list> in the {visible} tab has no anchor"
            );
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: TheColourPageHasASwatchBoundToAColourShapedProperty
#[test]
fn the_colour_page_has_a_swatch_bound_to_a_colour_shaped_value() {
    let document = document("mosstank-ub-color.xml");
    let swatches: Vec<_> = descendants_named(document.root_element(), "group")
        .filter(|group| attr(*group, "background").is_some_and(|b| b.starts_with('{')))
        .collect();
    assert_eq!(swatches.len(), 1);
    let binding = binding_name(attr(swatches[0], "background").unwrap());
    let kind = kind(&kinds("mosstank-ub-color.xml"), binding);
    assert!(
        matches!(
            kind,
            BindingKind::Uint | BindingKind::Int | BindingKind::Text
        ),
        "{kind:?}"
    );
    assert!(attr(swatches[0], "border").is_some_and(|border| !border.trim().is_empty()));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: TheUbProfilePickerOffersANameAndBothWaysOfMakingOne
#[test]
fn the_ub_profile_picker_offers_a_name_and_both_ways_of_making_one() {
    let document = document("mosstank.xml");
    let group = page(document.root_element(), "UbSettingsVisible");
    let count = |tag: &'static str, test: &dyn Fn(Node<'_, '_>) -> bool| {
        descendants_named(group, tag)
            .filter(|node| test(*node))
            .count()
    };
    assert_eq!(
        count("field", &|field| attr(field, "onsubmit")
            == Some("{CreateNamedUbProfile}")
            && attr(field, "text") == Some("{UbProfileNameDraft}")),
        1
    );
    assert_eq!(
        count("button", &|button| attr(button, "onclick")
            == Some("{CreateUbProfile}")),
        1
    );
    assert_eq!(
        count("button", &|button| attr(button, "onclick")
            == Some("{CopyUbProfile}")),
        1
    );
    assert_eq!(
        count("menu", &|menu| attr(menu, "onchange")
            == Some("{SelectUbProfile}")),
        1
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: AssertWithinParent_CatchesAnUnsizedLabelNearTheBottomEdge
#[test]
fn the_parent_check_catches_an_unsized_label_near_the_bottom_edge() {
    let document =
        Document::parse(r#"<group w="848" h="194"><label x="4" y="190" text="Notice" /></group>"#)
            .unwrap();
    assert!(!outside_parent(document.root_element()).is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: AuthoredControlsInTheSameContainerNeverOverlapASibling
#[test]
fn authored_controls_in_the_same_container_never_overlap_a_sibling() {
    for file in FILES {
        let document = document(file);
        let problems = sibling_overlaps(document.root_element());
        assert!(problems.is_empty(), "{file}: {problems:#?}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: AssertNoSiblingOverlap_CatchesARealOverlapAndIgnoresGroupPagesAndTouchingEdges
#[test]
fn the_overlap_check_catches_a_real_overlap_and_ignores_pages_and_touching_edges() {
    let overlapping = Document::parse(
        r#"<panel w="848" h="194"><button x="336" y="102" w="120" h="16" /><button x="338" y="108" w="26" h="20" /></panel>"#,
    )
    .unwrap();
    assert!(!sibling_overlaps(overlapping.root_element()).is_empty());
    let touching = Document::parse(
        r#"<panel w="848" h="194"><button x="0" y="0" w="100" h="20" /><button x="100" y="0" w="100" h="20" /></panel>"#,
    )
    .unwrap();
    assert!(sibling_overlaps(touching.root_element()).is_empty());
    let pages = Document::parse(
        r#"<panel w="848" h="236"><group x="8" y="42" w="848" h="194" /><group x="8" y="42" w="848" h="194" /></panel>"#,
    )
    .unwrap();
    assert!(sibling_overlaps(pages.root_element()).is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: SecondaryPopupPanelsFitTheirOwnBoundsAndEveryBindingResolves
#[test]
fn secondary_windows_fit_their_own_bounds_and_every_binding_resolves() {
    let bounds = [
        ("mosstank-remote.xml", 154.0, 174.0),
        ("mosstank-action-history.xml", 620.0, 350.0),
        ("mosstank-advanced.xml", 650.0, 560.0),
        ("mosstank-loot-editor.xml", 268.0, 300.0),
        ("mosstank-buffpicker.xml", 268.0, 236.0),
        ("mosstank-metaeditor.xml", 630.0, 174.0),
        ("mosstank-ub-color.xml", 268.0, 182.0),
        ("mosstank-ub-list.xml", 268.0, 300.0),
        ("mosstank-ub-map.xml", 320.0, 76.0),
        ("mosstank-ub-clients.xml", 268.0, 200.0),
        ("mosstank-ub-dungeon.xml", 320.0, 96.0),
    ];
    for file in FILES.iter().filter(|file| **file != "mosstank.xml") {
        let (_, width, height) = bounds
            .iter()
            .find(|(name, _, _)| name == file)
            .unwrap_or_else(|| panic!("{file} has no expected width/height entry"));
        let document = document(file);
        let root = document.root_element();
        assert_eq!(number(root, "w"), *width, "{file}");
        assert_eq!(number(root, "h"), *height, "{file}");
        let problems = outside_parent(root);
        assert!(problems.is_empty(), "{file}: {problems:#?}");
        let kinds = kinds(file);
        for node in root.descendants().filter(Node::is_element) {
            let problems = delegate_shape_mismatches(node, &kinds);
            assert!(problems.is_empty(), "{file}: {problems:#?}");
            for attribute in node.attributes() {
                if attribute.value().contains('{') {
                    kind(&kinds, binding_name(attribute.value()));
                }
            }
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: NoButtonAnywhereUsesTheUnrenderableArrowGlyphs
#[test]
fn no_button_anywhere_uses_the_unrenderable_arrow_glyphs() {
    for file in FILES {
        let document = document(file);
        for button in descendants_named(document.root_element(), "button") {
            assert!(
                !matches!(attr(button, "text"), Some("\u{2191}" | "\u{2193}")),
                "{file}"
            );
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: TextlessAndAbbreviatedControlsHaveAccessibleAuthenticTooltips
#[test]
fn textless_and_abbreviated_controls_have_tooltips() {
    const TERSE: [&str; 18] = [
        "+", "-", "\u{2191}", "\u{2193}", "F", "B", "G", "I", "Y", "V", "A", "R", "S", "W", "FC",
        "Cp", "DC", "Cs",
    ];
    let document = document("mosstank.xml");
    for element in document.root_element().descendants().filter(|node| {
        node.is_element()
            && INTERACTIVE_ELEMENT_NAMES.contains(&name(*node))
            && name(*node) != "column"
    }) {
        let caption = attr(element, "text").unwrap_or("");
        if !caption.trim().is_empty() && !TERSE.contains(&caption) {
            continue;
        }
        assert!(
            attr(element, "tooltip").is_some_and(|tooltip| !tooltip.trim().is_empty()),
            "<{}> text='{caption}' needs a tooltip.",
            name(element)
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: EveryEditableFieldSitsInABorderedWellThatMakesItLookLikeAField
#[test]
fn every_editable_field_sits_in_a_bordered_well() {
    for file in FILES {
        let document = document(file);
        for field in descendants_named(document.root_element(), "field")
            .filter(|field| attr(*field, "editable") != Some("false"))
        {
            let well = field.parent_element().unwrap();
            assert_eq!(name(well), "group", "{file}");
            assert_eq!(attr(field, "background"), Some("#00000000"), "{file}");
            assert!(
                attr(well, "background").is_some_and(|b| !b.trim().is_empty()),
                "A field well in {file} draws no background."
            );
            assert!(
                attr(well, "border").is_some_and(|b| !b.trim().is_empty()),
                "A field well in {file} draws no border."
            );
            assert_eq!(elements(well).count(), 1, "{file}");
            assert_eq!(number(field, "x"), 1.0);
            assert_eq!(number(field, "y"), 1.0);
            assert_eq!(number(field, "w"), number(well, "w") - 2.0);
            assert_eq!(number(field, "h"), number(well, "h") - 2.0);
            let Some(field_anchor) = attr(field, "anchor") else {
                continue;
            };
            let well_anchor = attr(well, "anchor").unwrap_or("").to_ascii_lowercase();
            assert!(
                well_anchor.contains("left") && well_anchor.contains("right"),
                "A field in {file} carries an anchor although its well does not widen"
            );
            assert_eq!(field_anchor, "left top right");
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: EveryPageFillsTheSameRectangleUnderTheTabRow
#[test]
fn every_page_fills_the_same_rectangle_under_the_tab_row() {
    let document = document("mosstank.xml");
    let root = document.root_element();
    assert_eq!(number(root, "w"), 984.0);
    assert_eq!(number(root, "h"), 271.0);
    for group in named(root, "group") {
        assert_eq!(number(group, "x"), 8.0);
        assert_eq!(number(group, "y"), 42.0);
        assert_eq!(number(group, "w"), 976.0);
        assert_eq!(number(group, "h"), 229.0);
    }
    assert!(outside_parent(root).is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: OnlyWindowChromeSitsInTheTitleRowsOfAnyWindow
#[test]
fn only_window_chrome_sits_in_the_title_rows_of_any_window() {
    const TITLE_ROW_BOTTOM: f32 = 20.0;
    for file in FILES {
        let document = document(file);
        let root = document.root_element();
        let width = number(root, "w");
        for child in elements(root) {
            if attr(child, "text") == Some("{WindowTitle}") {
                continue;
            }
            let onclick = attr(child, "onclick");
            let closer = name(child) == "button"
                && ((file == "mosstank-advanced.xml" && onclick == Some("{HideAdvancedOptions}"))
                    || (file == "mosstank-action-history.xml"
                        && onclick == Some("{HideActionHistory}")));
            let swapper = name(child) == "button"
                && ((file == "mosstank.xml" && onclick == Some("{ShowRemote}"))
                    || (file == "mosstank-remote.xml" && onclick == Some("{ShowMainWindow}")));
            if closer || swapper {
                let reach = if closer { 24.0 } else { 50.0 };
                let x = number(child, "x");
                assert!(x >= width - reach && x <= width, "{file}");
                assert!(x + number(child, "w") <= width, "{file}");
                assert_eq!(attr(child, "anchor"), Some("right top"), "{file}");
                continue;
            }
            assert!(
                number(child, "y") >= TITLE_ROW_BOTTOM,
                "<{}> at y={} in {file} is drawn under the window title.",
                name(child),
                number(child, "y")
            );
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: EveryItemsPageButtonSaysWhatItActsOn
#[test]
fn every_items_page_button_says_what_it_acts_on() {
    let document = document("mosstank.xml");
    let group = page(document.root_element(), "ItemsVisible");
    for action in [
        "{AddSelectedItem}",
        "{AddSelectedItemNoBuffs}",
        "{RemoveSelectedItem}",
    ] {
        let buttons: Vec<_> = named(group, "button")
            .filter(|button| attr(*button, "onclick") == Some(action))
            .collect();
        assert_eq!(buttons.len(), 1, "{action}");
        assert!(
            attr(buttons[0], "tooltip").is_some_and(|tooltip| !tooltip.trim().is_empty()),
            "{action}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: TheNavigationPageCanAimAFollowRouteAndSaysWhoIsFollowed
#[test]
fn the_navigation_page_can_aim_a_follow_route_and_says_who_is_followed() {
    let document = document("mosstank.xml");
    let group = page(document.root_element(), "RouteVisible");
    let follow: Vec<_> = named(group, "button")
        .filter(|button| attr(*button, "onclick") == Some("{SetFollowTarget}"))
        .collect();
    assert_eq!(follow.len(), 1);
    assert_eq!(text(follow[0]), "Follow");
    assert_eq!(
        named(group, "label")
            .filter(|label| text(*label) == "{RouteFollowTargetText}")
            .count(),
        1
    );
    let menus: Vec<_> = named(group, "menu")
        .filter(|menu| attr(*menu, "items") == Some("{RouteModeNames}"))
        .collect();
    assert_eq!(menus.len(), 1);
    assert!(attr(menus[0], "tooltip").unwrap().contains("Follow"));
}

/// The heading captions' glyph advances in the client's default interface
/// font (the C# test's measured table).
const CAPTION_WIDTHS: [(&str, f32); 14] = [
    ("F", 6.0),
    ("B", 7.0),
    ("G", 9.0),
    ("I", 3.0),
    ("Y", 7.0),
    ("V", 9.0),
    ("A", 9.0),
    ("R", 7.0),
    ("S", 7.0),
    ("WC", 19.0),
    ("FC", 14.0),
    ("Cp", 14.0),
    ("DC", 17.0),
    ("Cs", 13.0),
];

fn caption_width(caption: &str) -> Option<f32> {
    CAPTION_WIDTHS
        .iter()
        .find(|(name, _)| *name == caption)
        .map(|(_, width)| *width)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: EveryCheckColumnHeadingIsCentredOverTheLampItsColumnDraws
#[test]
fn every_check_column_heading_is_centred_over_its_lamp() {
    const LAMP_SIZE: f32 = 11.0;
    let document = document("mosstank.xml");
    let group = page(document.root_element(), "MonstersVisible");
    let table = named(group, "list").next().unwrap();
    let check_columns: Vec<_> = named(table, "column")
        .filter(|column| attr(*column, "type") == Some("check"))
        .collect();
    let captions: Vec<&str> = named(group, "label")
        .map(text)
        .filter(|caption| caption_width(caption).is_some())
        .collect();
    assert_eq!(check_columns.len(), captions.len());
    let mut column_left = 0.0f32;
    for (column, caption) in check_columns.iter().zip(&captions) {
        let column_width = number(*column, "width");
        assert!(column_width > 0.0, "A check column needs a fixed width.");
        let lamp_centre =
            column_left + 1.0 + (column_width - 2.0 - LAMP_SIZE).max(0.0) * 0.5 + LAMP_SIZE * 0.5;
        let headings: Vec<_> = named(group, "label")
            .filter(|label| text(*label) == *caption)
            .collect();
        assert_eq!(headings.len(), 1, "{caption}");
        let heading_centre = number(headings[0], "x") + caption_width(caption).unwrap() * 0.5;
        assert!(
            (heading_centre - lamp_centre).abs() <= 1.0,
            "Heading '{caption}' is centred at {heading_centre} but its lamp at {lamp_centre}."
        );
        column_left += column_width;
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: EveryTextColumnHeadingStartsWhereItsCellTextDoes
#[test]
fn every_text_column_heading_starts_where_its_cell_text_does() {
    const CELL_PADDING: f32 = 3.0;
    let document = document("mosstank.xml");
    let group = page(document.root_element(), "MonstersVisible");
    let table = named(group, "list").next().unwrap();
    let columns: Vec<_> = named(table, "column").collect();
    for (caption, column) in [
        ("Name", 14),
        ("P", 15),
        ("Dmg type", 16),
        ("Ex. Vuln", 17),
        ("Weapon", 18),
        ("Offhand", 19),
        ("PetDmg", 20),
    ] {
        let column_left: f32 = columns[..column]
            .iter()
            .map(|column| number(*column, "width"))
            .sum();
        let headings: Vec<_> = named(group, "label")
            .filter(|label| text(*label) == caption)
            .collect();
        assert_eq!(headings.len(), 1, "{caption}");
        assert_eq!(
            number(headings[0], "x"),
            column_left + CELL_PADDING,
            "{caption}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: EveryListHeadingIsPinnedToTheSameHorizontalEdgeAsItsList
#[test]
fn every_list_heading_is_pinned_to_the_same_horizontal_edge_as_its_list() {
    let document = document("mosstank.xml");
    for list in descendants_named(document.root_element(), "list") {
        let group = list.parent_element().unwrap();
        let (list_left, list_right) = horizontal_anchor(list);
        let x = number(list, "x");
        let right = x + number(list, "w");
        let top = number(list, "y");
        for label in named(group, "label") {
            let bottom = number(label, "y") + number(label, "h");
            let label_x = number(label, "x");
            if bottom > top || bottom <= top - 24.0 {
                continue;
            }
            if label_x < x || label_x >= right {
                continue;
            }
            let (label_left, label_right) = horizontal_anchor(label);
            assert_eq!(
                label_left,
                list_left,
                "Heading '{}' is not pinned to the same left edge as its list.",
                text(label)
            );
            assert!(
                !label_right || list_right,
                "Heading '{}' is pinned to a right edge its list is not.",
                text(label)
            );
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: NoSecondaryWindowIsAuthoredOverTheMainWindow
#[test]
fn no_secondary_window_is_authored_over_the_main_window() {
    let main = document("mosstank.xml");
    for file in FILES.iter().filter(|file| file.starts_with("mosstank-")) {
        let document = document(file);
        assert!(
            !rectangles_overlap(main.root_element(), document.root_element()),
            "{file} is authored over the main window"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Tinker.cs :: TheTinkerPageIsOneSelfContainedGroupInTheMarkup
#[test]
fn the_tinker_page_is_one_self_contained_group_in_the_markup() {
    let document = document("mosstank.xml");
    let group = page(document.root_element(), "TinkerVisible");
    assert_eq!(attr(group, "anchor"), Some("left top right bottom"));
    assert_eq!(named(group, "list").count(), 2);
    assert_eq!(named(group, "menu").count(), 4);
    assert!(named(group, "button").any(|button| text(button) == "Add Selected Item"));
    assert!(named(group, "button").any(|button| text(button) == "Rend All"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbMainTab.cs :: TheTinkerTabExistsWithAPageOfItsOwn
#[test]
fn the_tinker_tab_exists_with_a_page_of_its_own() {
    let document = document("mosstank.xml");
    let root = document.root_element();
    assert_eq!(
        named(root, "tab")
            .filter(|tab| text(*tab) == "Tinker")
            .count(),
        1
    );
    page(root, "TinkerVisible");
}

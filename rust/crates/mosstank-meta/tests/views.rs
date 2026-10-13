//! The views metas create, the status window, and the ui* functions.
//! Ported from the C#'s `MetaViewManagerTests`.

mod common;

use common::*;
use mosstank_formats::meta::{MetaAction, MetaActionKind, MetaCondition, MetaRule};
use mosstank_meta::status_hud::StatusHud;
use mosstank_meta::testing::FakeHost;
use mosstank_meta::view::{self, MetaView, MetaViewControlKind};
use mosstank_meta::view_manager::{ButtonAction, MetaViewControls, MetaViewManager};

const MARKUP: &str = "<panel w=\"120\" h=\"60\" title=\"Meta\" visible=\"{WindowAvailable}\" />";

/// Written for these tests in the shape the published multi-client metas
/// use (from the C# test).
const PARTY_VIEW: &str = "<?xml version=\"1.0\"?> <view width=\"240\" height=\"100\" title=\"PartyBoard\">\t<control type=\"layout\" name=\"Grid\" text=\"Command\">
\t\t<control type=\"button\" name=\"Member1\" left=\"0\" top=\"0\" width=\"90\" height=\"25\" text=\"\" />
\t\t<control type=\"button\" name=\"Member1State\" left=\"90\" top=\"0\" width=\"150\" height=\"25\" text=\"\" actionexpr=\"setvar[`pinged`,1]\" />
\t\t<control type=\"button\" name=\"Footer\" left=\"0\" top=\"25\" width=\"120\" height=\"25\" text=\"Reset\" setstate=\"\" actionexpr=\"setvar[`order`,`first`]\" />
\t\t<control type=\"button\" name=\"Footer\" left=\"120\" top=\"25\" width=\"120\" height=\"25\" text=\"Leave\" setstate=\"Leaving\" actionexpr=\"setvar[`order`,`expression`]\" />
\t\t<control type=\"checkbox\" name=\"Unknown\" left=\"0\" top=\"50\" width=\"20\" height=\"20\" />
\t\t<control type=\"button\" name=\"Home\" left=\"0\" top=\"75\" width=\"240\" height=\"25\" text=\"Home\" setstate=\"Default\" />
\t</control></view>";

/// The C#'s `RecordingUiRegistry`: a window host whose own window
/// "Status" has a control "Action".
fn window_host() -> FakeHost {
    let mut host = FakeHost::new();
    host.has_ui = true;
    host.ui.views = vec!["Status".to_owned()];
    host.ui.controls = vec![("Status".to_owned(), "Action".to_owned())];
    host.ui.label_accepted = true;
    host
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaViewManagerTests.cs :: CreateReplaceDestroyAndDestroyAllOwnTheRegistrationTokens
#[test]
fn create_replace_destroy_and_destroy_all_own_the_registration_tokens() {
    let mut host = window_host();
    let mut manager = MetaViewManager::new();

    assert!(manager.create(&mut host, "Status", MARKUP));
    assert_eq!(host.panels.len(), 1);
    assert_eq!(host.panels[0].descriptor.title, "Status");
    assert!(host.panels[0].descriptor.show_in_side_panel);
    assert!(manager.create(&mut host, "Status", MARKUP));
    assert!(host.panels[0].disposed);
    assert_eq!(manager.count(), 1);

    assert!(manager.destroy(&mut host, "Status"));
    assert!(host.panels.last().unwrap().disposed);
    assert!(!manager.destroy(&mut host, "Status"));

    assert!(manager.create(&mut host, "One", MARKUP));
    assert!(manager.create(&mut host, "Two", MARKUP));
    manager.destroy_all(&mut host);
    assert_eq!(manager.count(), 0);
    assert!(
        host.panels[host.panels.len() - 2..]
            .iter()
            .all(|panel| panel.disposed)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaViewManagerTests.cs :: InvalidMarkupAndTheOfficialViewBoundaryArePreserved
#[test]
fn invalid_markup_and_the_official_view_boundary_are_preserved() {
    let mut host = window_host();
    let mut manager = MetaViewManager::new();

    assert!(!manager.create(&mut host, "Bad", "not xml"));
    assert!(!manager.create(&mut host, "Bad", "<label />"));
    for index in 0..6 {
        assert!(manager.create(&mut host, &format!("View {index}"), MARKUP));
    }

    assert_eq!(manager.count(), 6);
    assert!(!manager.create(&mut host, "Seventh", MARKUP));
    assert!(!manager.create(&mut host, "View 0", MARKUP));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaViewManagerTests.cs :: StatusHudReusesOneShelfWindowAndOwnsItsLifetime
#[test]
fn status_hud_reuses_one_shelf_window_and_owns_its_lifetime() {
    let mut host = window_host();
    let mut hud = StatusHud::default();

    assert!(hud.update(&mut host, "State", "Hunting", None));
    assert_eq!(host.panels.len(), 1);
    assert_eq!(host.panels[0].descriptor.title, "VTank Meta Status");
    assert!(host.panels[0].descriptor.show_in_side_panel);
    assert_eq!(hud.rows(), ["State: Hunting"]);
    assert_eq!(hud.row_colors(), [0x00E8_DEC3]);

    assert!(hud.update(&mut host, "State", "Resting", Some(0x0000_FF00)));
    assert!(hud.update(&mut host, "Target", "Drudge", Some(0x00FF_9900)));
    assert_eq!(host.panels.len(), 1);
    assert_eq!(hud.rows(), ["State: Resting", "Target: Drudge"]);
    assert_eq!(hud.row_colors(), [0x0000_FF00, 0x00FF_9900]);

    hud.destroy(&mut host);
    assert!(host.panels[0].disposed);
    assert_eq!(hud.count(), 0);
    assert!(hud.rows().is_empty());
    assert!(hud.row_colors().is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaViewManagerTests.cs :: UtilityBeltUiExpressionsUseThePluginScopedViewRegistry
#[test]
fn utility_belt_ui_expressions_use_the_plugin_scoped_view_registry() {
    let mut host = window_host();
    let mut expressions = runtime(&mut host);

    assert!(eval_truthy(
        &mut expressions,
        &mut host,
        "uiviewexists[`Status`]"
    ));
    assert!(eval_truthy(
        &mut expressions,
        &mut host,
        "uiviewvisible[`Status`]"
    ));
    assert!(eval_truthy(
        &mut expressions,
        &mut host,
        "uisetlabel[uigetcontrol[`Status`,`Action`],`Run`]"
    ));
    assert_eq!(host.ui.label, "Run");
    // The visibility setter hands its second argument back.
    assert_eq!(
        eval_number(
            &mut expressions,
            &mut host,
            "uisetvisible[uigetcontrol[`Status`,`Action`],0]"
        ),
        0.0
    );
    assert_eq!(host.ui.last_visible, Some(false));
}

fn do_all(children: Vec<MetaAction>) -> MetaRule {
    MetaRule::new(
        "Default",
        MetaCondition::always(),
        MetaAction {
            kind: MetaActionKind::All,
            children,
            ..MetaAction::default()
        },
    )
}

fn view_action(kind: MetaActionKind, name: &str, markup: &str) -> MetaAction {
    MetaAction {
        kind,
        text: name.to_owned(),
        secondary_text: markup.to_owned(),
        ..MetaAction::default()
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaViewManagerTests.cs :: RefusedViewActionsDoNotStopTheRestOfADoAll
#[test]
fn refused_view_actions_do_not_stop_the_rest_of_a_do_all() {
    let mut host = window_host();
    let mut services = TestServices::default();
    let mut engine = engine(
        &mut host,
        profile(vec![do_all(vec![
            view_action(MetaActionKind::DestroyView, "NeverOpened", ""),
            view_action(MetaActionKind::CreateView, "Broken", "not xml"),
            action(MetaActionKind::SetMetaState, "Next"),
        ])]),
    );
    engine.set_enabled(true);

    engine.evaluate_pass(&mut host, &mut services);

    assert_eq!(engine.current_state(), "Next");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaViewManagerTests.cs :: ViewXmlParsesItsLayoutAndButtonsWithTheLastNameWinning
#[test]
fn view_xml_parses_its_layout_and_buttons_with_the_last_name_winning() {
    let parsed = MetaView::parse(PARTY_VIEW).unwrap();

    assert_eq!(parsed.title, "PartyBoard");
    assert_eq!((parsed.width, parsed.height), (240, 100));
    let root = parsed.root_control().unwrap();
    assert_eq!(root.kind, MetaViewControlKind::Layout);
    // The unknown control type is skipped; five buttons remain.
    let names: Vec<&str> = root
        .children
        .iter()
        .map(|child| parsed.controls()[*child].name.as_str())
        .collect();
    assert_eq!(
        names,
        ["Member1", "Member1State", "Footer", "Footer", "Home"]
    );
    let footer = parsed.find("Footer").unwrap();
    assert_eq!(footer.text, "Leave");
    assert_eq!(
        (footer.left, footer.top, footer.width, footer.height),
        (120, 25, 120, 25)
    );
    assert_eq!(footer.set_state, "Leaving");
    assert_eq!(
        parsed.find("Grid").map(|control| control.kind),
        Some(MetaViewControlKind::Layout)
    );
    assert!(parsed.find("Unknown").is_none());
    let error = MetaView::parse("<view title=\"x\" />").unwrap_err();
    assert!(!error.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaViewManagerTests.cs :: AHostWithoutAWindowKeepsTheViewAndItsControls
#[test]
fn a_host_without_a_window_keeps_the_view_and_its_controls() {
    let mut host = FakeHost::new();
    let mut services = TestServices::default();
    let mut engine = engine(&mut host, profile(Vec::new()));

    assert!(
        !value(
            &mut engine,
            &mut host,
            &mut services,
            "uiviewexists[`Party`]"
        )
        .is_truthy()
    );
    assert!(engine.views().create(&mut host, "Party", PARTY_VIEW));
    assert!(
        value(
            &mut engine,
            &mut host,
            &mut services,
            "uiviewexists[`Party`]"
        )
        .is_truthy()
    );
    assert!(
        value(
            &mut engine,
            &mut host,
            &mut services,
            "uiviewvisible[`Party`]"
        )
        .is_truthy()
    );
    assert_eq!(
        number(
            &mut engine,
            &mut host,
            &mut services,
            "uisetlabel[uigetcontrol[`Party`,`Member1`],`Horan`]"
        ),
        1.0
    );
    assert_eq!(
        engine
            .views()
            .find("Party")
            .unwrap()
            .find("Member1")
            .unwrap()
            .text,
        "Horan"
    );
    assert_eq!(
        number(
            &mut engine,
            &mut host,
            &mut services,
            "uisetvisible[uigetcontrol[`Party`,`Member1State`],0]"
        ),
        0.0
    );
    assert!(
        !engine
            .views()
            .find("Party")
            .unwrap()
            .find("Member1State")
            .unwrap()
            .visible
    );
    assert_eq!(
        number(
            &mut engine,
            &mut host,
            &mut services,
            "uigetcontrol[`Party`,`Missing`]"
        ),
        0.0
    );
    assert!(
        engine
            .evaluate(
                &mut host,
                &mut services,
                "uisetlabel[uigetcontrol[`Party`,`Grid`],`x`]"
            )
            .is_err()
    );

    assert!(engine.views().destroy(&mut host, "Party"));
    assert!(
        !value(
            &mut engine,
            &mut host,
            &mut services,
            "uiviewexists[`Party`]"
        )
        .is_truthy()
    );
    assert_eq!(
        number(
            &mut engine,
            &mut host,
            &mut services,
            "uigetcontrol[`Party`,`Member1`]"
        ),
        0.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaViewManagerTests.cs :: AButtonRunsItsExpressionAndThenItsState
#[test]
fn a_button_runs_its_expression_and_then_its_state() {
    let mut host = FakeHost::new();
    let mut expressions = runtime(&mut host);
    let mut manager = MetaViewManager::new();
    let mut states: Vec<(String, String)> = Vec::new();
    assert!(manager.create(&mut host, "Party", PARTY_VIEW));

    let mut run = |host: &mut dyn mosstank_meta::host::MetaHost, action: ButtonAction<'_>| {
        let mut services = TestServices::default();
        match action {
            ButtonAction::Expression(source) => expressions
                .evaluate(host, &mut services, source)
                .map(|_| ())
                .map_err(|error| error.message()),
            ButtonAction::SetState(state) => {
                let order = expressions
                    .evaluate(host, &mut services, "getvar[`order`]")
                    .unwrap();
                states.push((state.to_owned(), order.to_display_string()));
                Ok(())
            }
        }
    };
    // "Footer" is the second of the two buttons with that name.
    assert!(manager.click(&mut host, "Party", "Footer", &mut run));
    assert!(manager.click(&mut host, "Party", "Member1State", &mut run));
    assert!(manager.click(&mut host, "Party", "Member1", &mut run));
    assert!(!manager.click(&mut host, "Party", "Grid", &mut run));
    assert!(!manager.click(&mut host, "Party", "Missing", &mut run));

    assert_eq!(states, [("Leaving".to_owned(), "expression".to_owned())]);
    assert_eq!(
        eval_number(&mut expressions, &mut host, "getvar[`pinged`]"),
        1.0
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaViewManagerTests.cs :: AFailingButtonExpressionStillChangesStateAndWarnsOnce
#[test]
fn a_failing_button_expression_still_changes_state_and_warns_once() {
    const VIEW: &str = "<view width=\"100\" height=\"25\" title=\"Broken\"><control type=\"layout\">
<control type=\"button\" name=\"Go\" left=\"0\" top=\"0\" width=\"100\" height=\"25\" text=\"Go\" actionexpr=\"nosuchfunction[]\" setstate=\"Next\" />
</control></view>";
    let mut host = FakeHost::new();
    let mut expressions = runtime(&mut host);
    let mut manager = MetaViewManager::new();
    let mut states: Vec<String> = Vec::new();
    assert!(manager.create(&mut host, "Broken", VIEW));

    let mut run =
        |host: &mut dyn mosstank_meta::host::MetaHost, action: ButtonAction<'_>| match action {
            ButtonAction::Expression(source) => expressions
                .evaluate(host, &mut TestServices::default(), source)
                .map(|_| ())
                .map_err(|error| error.message()),
            ButtonAction::SetState(state) => {
                states.push(state.to_owned());
                Ok(())
            }
        };
    assert!(manager.click(&mut host, "Broken", "Go", &mut run));
    assert!(manager.click(&mut host, "Broken", "Go", &mut run));

    assert_eq!(states, ["Next", "Next"]);
    let warnings = warnings(&host);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].starts_with("Error in Button meta expression"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaViewManagerTests.cs :: AMetaCreatesAViewAndItsButtonMovesTheMetaWithoutAWindow
#[test]
fn a_meta_creates_a_view_and_its_button_moves_the_meta_without_a_window() {
    let mut host = FakeHost::new();
    let mut services = TestServices::default();
    let mut engine = engine(
        &mut host,
        profile(vec![do_all(vec![
            view_action(MetaActionKind::DestroyView, "Party", ""),
            view_action(MetaActionKind::CreateView, "Party", PARTY_VIEW),
            action(MetaActionKind::SetMetaState, "Waiting"),
        ])]),
    );
    engine.set_enabled(true);

    engine.evaluate_pass(&mut host, &mut services);
    assert_eq!(engine.current_state(), "Waiting");
    assert!(
        value(
            &mut engine,
            &mut host,
            &mut services,
            "uiviewexists[`Party`]"
        )
        .is_truthy()
    );

    assert!(engine.click_view(&mut host, &mut services, "Party", "Footer"));
    assert_eq!(engine.current_state(), "Leaving");
    let order = value(&mut engine, &mut host, &mut services, "getvar[`order`]");
    assert_eq!(order.to_display_string(), "expression");
}

fn attribute(node: roxmltree::Node<'_, '_>, name: &str) -> String {
    node.attribute(name).unwrap_or_default().to_owned()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaViewManagerTests.cs :: AWindowHostDrawsTheViewAtItsAuthoredLayout
#[test]
fn a_window_host_draws_the_view_at_its_authored_layout() {
    let mut host = window_host();
    let mut manager = MetaViewManager::new();

    assert!(manager.create(&mut host, "Party", PARTY_VIEW));

    assert_eq!(host.panels.len(), 1);
    let panel_registration = &host.panels[0];
    assert_eq!(panel_registration.descriptor.title, "Meta - PartyBoard");
    assert!(panel_registration.descriptor.start_visible);
    let document = roxmltree::Document::parse(&panel_registration.markup).unwrap();
    let panel = document.root_element();
    assert_eq!(panel.tag_name().name(), "panel");
    assert_eq!(attribute(panel, "title"), "Meta - PartyBoard");
    // The view's 240 x 100 content, a 4-pixel margin at each side and a
    // 24-pixel title strip above.
    assert_eq!(
        (attribute(panel, "w"), attribute(panel, "h")),
        ("248".to_owned(), "128".to_owned())
    );
    let elements: Vec<_> = panel
        .children()
        .filter(roxmltree::Node::is_element)
        .collect();
    assert_eq!(elements.len(), 1);
    let group = elements[0];
    assert_eq!(group.tag_name().name(), "group");
    assert_eq!(
        (
            attribute(group, "x"),
            attribute(group, "y"),
            attribute(group, "w"),
            attribute(group, "h"),
            attribute(group, "visible")
        ),
        (
            "4".to_owned(),
            "24".to_owned(),
            "240".to_owned(),
            "100".to_owned(),
            "{Visible0}".to_owned()
        )
    );
    let buttons: Vec<String> = group
        .children()
        .filter(roxmltree::Node::is_element)
        .map(|button| {
            assert_eq!(button.tag_name().name(), "button");
            format!(
                "{} {},{} {}x{} {} {} {}",
                attribute(button, "name"),
                attribute(button, "x"),
                attribute(button, "y"),
                attribute(button, "w"),
                attribute(button, "h"),
                attribute(button, "text"),
                attribute(button, "onclick"),
                attribute(button, "visible")
            )
        })
        .collect();
    assert_eq!(
        buttons,
        [
            "Member1 0,0 90x25 {Text1} {Click1} {Visible1}",
            "Member1State 90,0 150x25 {Text2} {Click2} {Visible2}",
            "Footer 0,25 120x25 {Text3} {Click3} {Visible3}",
            "Footer 120,25 120x25 {Text4} {Click4} {Visible4}",
            "Home 0,75 240x25 {Text5} {Click5} {Visible5}",
        ]
    );
}

/// The C# binds onclick, visible and text by property name on its binding
/// object; here the window reads a slot's text, visibility and click by
/// the number in the binding name, so every binding the markup holds must
/// name a slot the view answers for.
// Source: tests/AcDream.Plugins.MossTank.Tests/MetaViewManagerTests.cs :: EveryBindingOfTheDrawnViewResolvesOnItsBindingObject
#[test]
fn every_binding_of_the_drawn_view_resolves_on_its_binding_object() {
    let mut host = window_host();
    let mut manager = MetaViewManager::new();
    assert!(manager.create(&mut host, "Party", PARTY_VIEW));
    let document = roxmltree::Document::parse(&host.panels[0].markup).unwrap();
    let controls = manager.find("Party").unwrap().controls().len();

    let mut bound = 0;
    for node in document.descendants().filter(roxmltree::Node::is_element) {
        for attribute in node.attributes() {
            let value = attribute.value();
            let Some(name) = value
                .strip_prefix('{')
                .and_then(|rest| rest.strip_suffix('}'))
            else {
                continue;
            };
            let prefix = match attribute.name() {
                "onclick" => "Click",
                "visible" => "Visible",
                "text" => "Text",
                other => panic!("{other} is bound"),
            };
            let slot: usize = name
                .strip_prefix(prefix)
                .unwrap_or_else(|| panic!("{value} is not a {prefix} slot"))
                .parse()
                .unwrap();
            assert!(slot < view::CAPACITY && slot < controls, "{value}");
            bound += 1;
        }
    }
    assert_eq!(bound, 16);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaViewManagerTests.cs :: ADrawnButtonRunsTheControlItWasDrawnFromAndShowsUiChanges
#[test]
fn a_drawn_button_runs_the_control_it_was_drawn_from_and_shows_ui_changes() {
    let mut host = window_host();
    let mut services = TestServices::default();
    let mut engine = engine(&mut host, profile(Vec::new()));
    assert!(engine.views().create(&mut host, "Party", PARTY_VIEW));

    // Both "Footer" buttons are drawn; each runs its own actions.
    engine.click_view_slot(&mut host, &mut services, "Party", 3);
    assert!(services.state_changes.is_empty());
    assert_eq!(
        value(&mut engine, &mut host, &mut services, "getvar[`order`]").to_display_string(),
        "first"
    );
    engine.click_view_slot(&mut host, &mut services, "Party", 4);
    assert_eq!(engine.current_state(), "Leaving");
    assert_eq!(
        value(&mut engine, &mut host, &mut services, "getvar[`order`]").to_display_string(),
        "expression"
    );

    let party = |engine: &mut mosstank_meta::engine::MetaEngine| {
        engine.views().find("Party").unwrap().clone()
    };
    assert_eq!(view::slot_text(&party(&mut engine), 3), "Reset");
    value(
        &mut engine,
        &mut host,
        &mut services,
        "uisetlabel[uigetcontrol[`Party`,`Member1`],`Horan`]",
    );
    assert_eq!(view::slot_text(&party(&mut engine), 1), "Horan");
    assert!(view::slot_visible(&party(&mut engine), 2));
    value(
        &mut engine,
        &mut host,
        &mut services,
        "uisetvisible[uigetcontrol[`Party`,`Member1State`],0]",
    );
    assert!(!view::slot_visible(&party(&mut engine), 2));
    // The meta's view answers ahead of the host, which knows no "Party".
    assert_eq!(party(&mut engine).find("Member1").unwrap().text, "Horan");
    assert_eq!(host.ui.label, "");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MetaViewManagerTests.cs :: TheBindingHasASlotForEveryControlItCanDraw
#[test]
fn the_binding_has_a_slot_for_every_control_it_can_draw() {
    assert_eq!(view::CAPACITY, 128);
    let buttons: String = (0..=view::CAPACITY)
        .map(|index| {
            format!(
                "<control type=\"button\" name=\"B{index}\" left=\"0\" top=\"0\" width=\"10\" height=\"10\" text=\"\" />"
            )
        })
        .collect();
    let mut host = window_host();
    let mut manager = MetaViewManager::new();
    assert!(manager.create(
        &mut host,
        "Wide",
        &format!(
            "<view width=\"10\" height=\"10\" title=\"Wide\"><control type=\"layout\">{buttons}</control></view>"
        )
    ));

    let document = roxmltree::Document::parse(&host.panels[0].markup).unwrap();
    // The layout takes slot 0, so one button fewer than the capacity is drawn.
    assert_eq!(
        document
            .descendants()
            .filter(|node| node.has_tag_name("button"))
            .count(),
        view::CAPACITY - 1
    );
    assert_eq!(warnings(&host).len(), 1);
    assert!(manager.control_exists("Wide", &format!("B{}", view::CAPACITY)));
}

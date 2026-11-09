//! The fifteen windows as the client holds them: registered with the
//! descriptors `MossTankPlugin.Initialize` gives, every value binding pushed
//! with a value of its kind, and every handler answered by the shell.

mod common;

use ac_plugin_abi::Event;
use ac_plugin_abi::LogLevel;
use ac_plugin_abi::ui::{ActionValue, TypedBindingKind, UiAction, UiEvent, UiValue};
use common::World;
use mosstank::panel::{INVENTORY_ICON_SURFACE_ID, WINDOWS, is_handler, schema};

fn fresh() -> (World, mosstank::MossTank) {
    let mut world = World::with_character("Window Tester");
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    (world, plugin)
}

fn kind_of(value: &UiValue) -> TypedBindingKind {
    use UiValue as V;
    match value {
        V::Bool(_) => TypedBindingKind::Bool,
        V::Int(_) => TypedBindingKind::Int,
        V::Uint(_) => TypedBindingKind::Uint,
        V::Float(_) => TypedBindingKind::Float,
        V::Text(_) => TypedBindingKind::Text,
        V::TextList(_) => TypedBindingKind::TextList,
        V::UintList(_) => TypedBindingKind::UintList,
        V::IntList(_) => TypedBindingKind::IntList,
        V::BoolList(_) => TypedBindingKind::BoolList,
    }
}

// Source: src/AcDream.Plugins.MossTank/MossTankPlugin.cs :: Initialize (the
// twelve AddPanel calls: ids, captions, the main window hidden on the shelf
// with its icon, the remote hidden off the shelf, the rest off the shelf),
// the Comps HUD's Edit items window off the shelf beside them, the Mag
// tab's Inventory window hidden on the shelf with its own icon, and its
// filters window hidden off the shelf.
#[test]
fn every_window_registers_with_its_descriptor() {
    let (world, _plugin) = fresh();
    let warnings = world.logs(LogLevel::Warn);
    assert!(
        !warnings
            .iter()
            .any(|line| line.contains("was not registered") || line.contains("refused")),
        "{warnings:?}"
    );
    let panels: Vec<_> = world.fake.ui.panels().collect();
    assert_eq!(panels.len(), WINDOWS.len());
    for window in &WINDOWS {
        let panel = panels
            .iter()
            .find(|panel| panel.descriptor.window_id == window.id)
            .unwrap_or_else(|| panic!("window {} is missing", window.id));
        assert_eq!(panel.descriptor.title, window.title);
        assert_eq!(
            panel.descriptor.start_visible, window.start_visible,
            "{}",
            window.id
        );
        assert_eq!(
            panel.descriptor.show_in_side_panel, window.on_shelf,
            "{}",
            window.id
        );
        assert_eq!(
            panel.descriptor.icon_surface_id,
            window.icon.map_or(0, |icon| icon.0),
            "{}",
            window.id
        );
        assert_eq!(
            panel.descriptor.icon_text.as_deref(),
            window.icon.map(|icon| icon.1),
            "{}",
            window.id
        );
    }
    let main = panels
        .iter()
        .find(|panel| panel.descriptor.window_id == "main")
        .unwrap();
    assert_eq!(main.descriptor.icon_text.as_deref(), Some("MT"));
    assert_eq!(main.descriptor.icon_surface_id, 0x0600_2C41);
    assert!(!main.descriptor.start_visible);
    assert!(main.descriptor.show_in_side_panel);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: SecondaryPopupPanelsFitTheirOwnBoundsAndEveryBindingResolves (every
// binding names a panel property, here: a value the shell answers with the
// kind the markup's attribute takes).
#[test]
fn every_value_binding_is_pushed_with_a_value_of_its_kind() {
    let (world, _plugin) = fresh();
    let mut missing = Vec::new();
    for window in &WINDOWS {
        let panel = world
            .fake
            .ui
            .panels()
            .find(|panel| panel.descriptor.window_id == window.id)
            .unwrap();
        for (name, kind) in schema::bindings(window.markup).unwrap() {
            if is_handler(kind) {
                continue;
            }
            match world.fake.panel_value(panel.id, &name) {
                Some(value) => assert_eq!(kind_of(&value), kind, "{} {name}", window.id),
                None => missing.push(format!("{}:{name}", window.id)),
            }
        }
    }
    assert!(missing.is_empty(), "no value for {missing:?}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: SecondaryPopupPanelsFitTheirOwnBoundsAndEveryBindingResolves (every
// handler a markup names is one the panel has).
#[test]
fn every_handler_a_window_names_is_answered() {
    let (mut world, mut plugin) = fresh();
    let mut unanswered = Vec::new();
    for window in &WINDOWS {
        for (name, kind) in schema::bindings(window.markup).unwrap() {
            if !is_handler(kind) {
                continue;
            }
            let value = match kind {
                TypedBindingKind::IntAction => ActionValue::Int(0),
                TypedBindingKind::FloatAction => ActionValue::Float(0.5),
                TypedBindingKind::TextAction => ActionValue::Text(String::new()),
                _ => ActionValue::None,
            };
            let answered = world.with_tank(&mut plugin, |tank, client| {
                tank.ui_act(client, &name, value)
            });
            if !answered {
                unanswered.push(format!("{}:{name}", window.id));
            }
        }
    }
    assert!(unanswered.is_empty(), "unanswered: {unanswered:?}");
}

// Source: src/AcDream.Plugins.MossTank/MossTankPanel.cs :: ShowRemote,
// ShowMainWindow (the remote's button and the main window's swap them).
#[test]
fn the_remote_and_the_main_window_swap() {
    let (mut world, mut plugin) = fresh();
    let id = |world: &World, window: &str| {
        world
            .fake
            .ui
            .panels()
            .find(|panel| panel.descriptor.window_id == window)
            .unwrap()
            .id
    };
    let visible = |world: &World, window: &str| {
        world
            .fake
            .ui
            .panels()
            .find(|panel| panel.descriptor.window_id == window)
            .unwrap()
            .requested_visible
    };
    let main = id(&world, "main");
    click(&mut world, &mut plugin, main, "ShowRemote");
    assert!(visible(&world, "remote"));
    assert!(!visible(&world, "main"));
    let remote = id(&world, "remote");
    click(&mut world, &mut plugin, remote, "ShowMainWindow");
    assert!(visible(&world, "main"));
    assert!(!visible(&world, "remote"));
}

// Source: src/AcDream.Plugins.MossTank/MossTankPanel.cs :: SelectTab (a
// tab's click shows its page and hides the others).
#[test]
fn a_tab_click_shows_its_page_and_hides_the_others() {
    let (mut world, mut plugin) = fresh();
    let main = world
        .fake
        .ui
        .panels()
        .find(|panel| panel.descriptor.window_id == "main")
        .unwrap()
        .id;
    click(&mut world, &mut plugin, main, "ShowRoute");
    assert_eq!(
        world.fake.panel_value(main, "RouteVisible"),
        Some(UiValue::Bool(true))
    );
    assert_eq!(
        world.fake.panel_value(main, "MonstersVisible"),
        Some(UiValue::Bool(false))
    );
}

fn click(world: &mut World, plugin: &mut mosstank::MossTank, panel: u32, handler: &str) {
    world.send(
        plugin,
        Event::Ui(UiEvent::Action(UiAction {
            panel,
            handler: handler.to_owned(),
            value: ActionValue::None,
        })),
    );
}

fn value(world: &mut World, plugin: &mut mosstank::MossTank, name: &str) -> Option<UiValue> {
    world.with_tank(plugin, |tank, client| tank.ui_value(client, name))
}

fn act(world: &mut World, plugin: &mut mosstank::MossTank, name: &str) {
    assert!(world.with_tank(plugin, |tank, client| tank.ui_act(
        client,
        name,
        ActionValue::None
    )));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: EveryVtankTabIsBackedByALivePanelSurface
#[test]
fn every_vtank_tab_is_backed_by_a_live_page() {
    let (mut world, mut plugin) = fresh();
    for tab in [
        "OptionsTabEnabled",
        "VitalsTabEnabled",
        "MonstersTabEnabled",
        "BuffsTabEnabled",
        "ProfilesTabEnabled",
        "ItemsTabEnabled",
        "ConsumablesTabEnabled",
        "RouteTabEnabled",
        "MetaTabEnabled",
    ] {
        assert_eq!(
            value(&mut world, &mut plugin, tab),
            Some(UiValue::Bool(true)),
            "{tab}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Tinker.cs :: TheTinkerTabShowsAndHidesWithTheRest
#[test]
fn the_tinker_tab_shows_and_hides_with_the_rest() {
    let (mut world, mut plugin) = fresh();
    let is = |world: &mut World, plugin: &mut mosstank::MossTank, name: &str| {
        value(world, plugin, name) == Some(UiValue::Bool(true))
    };
    assert!(is(&mut world, &mut plugin, "TinkerTabEnabled"));
    assert!(!is(&mut world, &mut plugin, "TinkerSelected"));
    act(&mut world, &mut plugin, "ShowTinker");
    assert!(is(&mut world, &mut plugin, "TinkerSelected"));
    assert!(is(&mut world, &mut plugin, "TinkerVisible"));
    assert!(!is(&mut world, &mut plugin, "UbVisible"));
    assert!(!is(&mut world, &mut plugin, "OptionsVisible"));
    act(&mut world, &mut plugin, "ShowOptions");
    assert!(!is(&mut world, &mut plugin, "TinkerVisible"));
}

// The Mag tab: Mag-Tools' tracker pages (Views/mainView.xml: Combat, Mana,
// Inv. Items), Combat first.
#[test]
fn the_mag_tab_shows_its_page_and_its_three_sub_pages() {
    let (mut world, mut plugin) = fresh();
    let is = |world: &mut World, plugin: &mut mosstank::MossTank, name: &str| {
        value(world, plugin, name) == Some(UiValue::Bool(true))
    };
    assert!(is(&mut world, &mut plugin, "MagTabEnabled"));
    assert!(!is(&mut world, &mut plugin, "MagVisible"));
    act(&mut world, &mut plugin, "ShowMag");
    assert!(is(&mut world, &mut plugin, "MagSelected"));
    assert!(is(&mut world, &mut plugin, "MagVisible"));
    assert!(!is(&mut world, &mut plugin, "TinkerVisible"));
    assert!(
        is(&mut world, &mut plugin, "MagCombatVisible"),
        "Combat is the first sub-page"
    );
    assert!(is(&mut world, &mut plugin, "MagCombatSelected"));
    act(&mut world, &mut plugin, "ShowMagMana");
    assert!(is(&mut world, &mut plugin, "MagManaVisible"));
    assert!(is(&mut world, &mut plugin, "MagManaSelected"));
    assert!(!is(&mut world, &mut plugin, "MagCombatVisible"));
    act(&mut world, &mut plugin, "ShowMagInvItems");
    assert!(is(&mut world, &mut plugin, "MagInvItemsVisible"));
    assert!(!is(&mut world, &mut plugin, "MagManaVisible"));
    act(&mut world, &mut plugin, "ShowOptions");
    assert!(!is(&mut world, &mut plugin, "MagVisible"));
    act(&mut world, &mut plugin, "ShowMag");
    assert!(
        is(&mut world, &mut plugin, "MagInvItemsVisible"),
        "the sub-page is kept"
    );
}

// The Inventory window (the Mag tab's inventory search): on the dock with
// its own icon, the game's Pack icon, closed until opened.
#[test]
fn the_inventory_window_sits_on_the_dock_with_its_own_icon() {
    let (world, _plugin) = fresh();
    let inventory = world
        .fake
        .ui
        .panels()
        .find(|panel| panel.descriptor.window_id == "inventory")
        .expect("the inventory window registers");
    assert!(inventory.descriptor.show_in_side_panel, "on the dock");
    assert_eq!(
        inventory.descriptor.icon_surface_id,
        INVENTORY_ICON_SURFACE_ID
    );
    assert_eq!(INVENTORY_ICON_SURFACE_ID, 0x0600_1BAF);
    assert_eq!(inventory.descriptor.icon_text.as_deref(), Some("IN"));
    assert!(!inventory.descriptor.start_visible);
}

// The Mag tab's button opens it.
#[test]
fn the_mag_tabs_button_opens_the_inventory_window() {
    let (mut world, mut plugin) = fresh();
    let shown = |world: &World| {
        world
            .fake
            .ui
            .panels()
            .find(|panel| panel.descriptor.window_id == "inventory")
            .unwrap()
            .requested_visible
    };
    assert!(!shown(&world));
    act(&mut world, &mut plugin, "OpenMagInventory");
    assert!(shown(&world));
}

// The Inventory window's Filters... button opens the filters window, which
// starts hidden and has no dock button of its own.
#[test]
fn the_inventory_windows_filters_button_opens_the_filters_window() {
    let (mut world, mut plugin) = fresh();
    let filters = |world: &World| {
        world
            .fake
            .ui
            .panels()
            .find(|panel| panel.descriptor.window_id == "inventory-filters")
            .map(|panel| {
                (
                    panel.descriptor.start_visible,
                    panel.descriptor.show_in_side_panel,
                    panel.requested_visible,
                )
            })
            .expect("the filters window registers")
    };
    assert_eq!(filters(&world), (false, false, false));
    act(&mut world, &mut plugin, "OpenMagInventoryFilters");
    assert!(filters(&world).2);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankMarkupContractTests.cs :: LootEditorHasNoLeftoverBackButton
#[test]
fn the_loot_editor_closes_through_its_own_handler() {
    let (mut world, mut plugin) = fresh();
    act(&mut world, &mut plugin, "CloseLootEditor");
    assert_eq!(
        value(&mut world, &mut plugin, "LootEditorVisible"),
        Some(UiValue::Bool(false))
    );
}

fn panel_id(world: &World, window: &str) -> u32 {
    world
        .fake
        .ui
        .panels()
        .find(|panel| panel.descriptor.window_id == window)
        .unwrap()
        .id
}

fn window_switch(opens: bool) {
    let (mut world, mut plugin) = fresh();
    world.refuse_window_show = !opens;
    let running = plugin.tank().unwrap().macro_running();
    let main = panel_id(&world, "main");
    let remote = panel_id(&world, "remote");
    let before = world.requests::<ac_plugin_abi::calls::UiPanelShow>().len();
    click(&mut world, &mut plugin, main, "ShowRemote");
    click(&mut world, &mut plugin, remote, "ShowMainWindow");
    let calls: Vec<String> = world
        .requests::<ac_plugin_abi::calls::UiPanelShow>()
        .into_iter()
        .skip(before)
        .map(|show| format!("{}:{}", if show.show { "show" } else { "hide" }, show.view))
        .collect();
    let expected: &[&str] = if opens {
        &["show:remote", "hide:main", "show:main", "hide:remote"]
    } else {
        &["show:remote", "show:main"]
    };
    assert_eq!(calls, expected);
    assert_eq!(plugin.tank().unwrap().macro_running(), running);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Remote.cs :: RemoteSwitchHidesSourceOnlyAfterDestinationOpens
#[test]
fn the_remote_switch_hides_the_source_only_after_the_destination_opens() {
    window_switch(true);
    window_switch(false);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.Remote.cs :: RemoteRegistersHiddenWithoutAnotherShelfEntryAndSharesMainBindings
#[test]
fn the_remote_registers_hidden_off_the_shelf_and_shares_the_main_bindings() {
    let (world, _plugin) = fresh();
    let panels: Vec<_> = world.fake.ui.panels().collect();
    let remote = panels
        .iter()
        .find(|panel| panel.descriptor.window_id == "remote")
        .unwrap();
    let main = panels
        .iter()
        .find(|panel| panel.descriptor.window_id == "main")
        .unwrap();
    assert!(!remote.descriptor.start_visible);
    assert!(!remote.descriptor.show_in_side_panel);
    // One shell answers both: every binding the two share holds the same
    // value in each.
    let mut shared = 0;
    for (name, value) in &remote.values {
        if let Some(other) = main.values.get(name) {
            assert_eq!(value, other, "{name}");
            shared += 1;
        }
    }
    assert!(shared > 0);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.ActionHistory.cs :: HistoryRegistersWithBoundVisibilityAndNoShelfEntry
#[test]
fn the_history_registers_with_bound_visibility_and_no_shelf_entry() {
    let (mut world, mut plugin) = fresh();
    let history = world
        .fake
        .ui
        .panels()
        .find(|panel| panel.descriptor.window_id == "action-history")
        .unwrap();
    assert!(history.descriptor.start_visible);
    assert!(!history.descriptor.show_in_side_panel);
    assert_eq!(
        value(&mut world, &mut plugin, "ActionHistoryVisible"),
        Some(UiValue::Bool(false))
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbMainTab.cs :: TheUbTabOpensAheadOfTheSettingsCatalogue
#[test]
fn the_ub_tab_opens_ahead_of_the_settings_catalogue() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    assert!(world.ui_bool(p, "UbTabEnabled"));
    assert!(!world.ui_bool(p, "UbSelected"));
    world.act(p, "ShowUb");
    assert!(world.ui_bool(p, "UbSelected"));
    assert!(world.ui_bool(p, "UbVisible"));
    assert!(!world.ui_bool(p, "UbSettingsSelected"));
    assert!(!world.ui_bool(p, "UbSettingsVisible"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbMainTab.cs :: TheSettingsAndTinkerButtonsLeaveForTheirOwnTabs
#[test]
fn the_settings_and_tinker_buttons_leave_for_their_own_tabs() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    world.act(p, "ShowUb");
    world.act(p, "ShowUbSettings");
    assert!(world.ui_bool(p, "UbSettingsSelected"));
    assert!(!world.ui_bool(p, "UbSelected"));
    assert!(!world.ui_bool(p, "TinkerSelected"));
    world.act(p, "ShowUb");
    world.act(p, "ShowTinker");
    assert!(world.ui_bool(p, "TinkerSelected"));
    assert!(world.ui_bool(p, "TinkerVisible"));
    assert!(world.ui_bool(p, "TinkerTabEnabled"));
    assert!(!world.ui_bool(p, "UbSelected"));
    assert!(!world.ui_bool(p, "UbSettingsSelected"));
}

/// The bindings table the plugin registers from, as Rust source.
fn bindings_table_source() -> String {
    use std::fmt::Write as _;
    let mut source = String::from(
        "//! Generated by `tests/windows.rs`\n\
         //! (`MOSSTANK_UPDATE_BINDINGS=1 cargo test -p mosstank --test windows`): the\n\
         //! bindings each window's markup names, as [`super::schema::bindings`]\n\
         //! reads them, so the plugin's start does not parse the markup.\n\
         \n\
         use ac_plugin_abi::ui::TypedBindingKind as K;\n\
         \n\
         /// The bindings of the window with this id, in first-use order.\n\
         pub(super) fn of(window_id: &str) -> Option<&'static [(&'static str, K)]> {\n    \
         Some(match window_id {\n",
    );
    for window in &WINDOWS {
        let _ = writeln!(source, "        {:?} => &[", window.id);
        for (name, kind) in schema::bindings(window.markup).unwrap() {
            let _ = writeln!(source, "            ({name:?}, K::{kind:?}),");
        }
        source.push_str("        ],\n");
    }
    source.push_str("        _ => return None,\n    })\n}\n");
    source
}

// The table the plugin registers its windows from is what the markup names
// (`MOSSTANK_UPDATE_BINDINGS=1` writes it afresh).
#[test]
fn the_bindings_table_is_what_the_markup_names() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/panel/bindings_table.rs");
    let expected = bindings_table_source();
    if std::env::var_os("MOSSTANK_UPDATE_BINDINGS").is_some() {
        std::fs::write(&path, &expected).unwrap();
    }
    let actual = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        actual == expected,
        "src/panel/bindings_table.rs is out of date: run with MOSSTANK_UPDATE_BINDINGS=1"
    );
    for window in &WINDOWS {
        let (_, bindings) = mosstank::panel::spec(window).unwrap();
        assert_eq!(
            bindings,
            schema::bindings(window.markup).unwrap(),
            "{}",
            window.id
        );
    }
}

// The values pushed only while a part of a window is shown are drawn
// nowhere else: every use of each sits inside that part (the markup's
// `visible` bindings around it).
#[test]
fn a_value_pushed_only_while_shown_is_drawn_only_there() {
    let mut uses: Vec<(String, Vec<String>)> = Vec::new();
    for window in &WINDOWS {
        uses.extend(schema::uses_with_visibility(window.markup).unwrap());
    }
    for (name, part) in mosstank::panel::SHOWN_ONLY_WITH {
        let of_name: Vec<&Vec<String>> = uses
            .iter()
            .filter(|(used, _)| used == name)
            .map(|(_, shown)| shown)
            .collect();
        assert!(!of_name.is_empty(), "{name} is drawn nowhere");
        for shown in of_name {
            assert!(
                shown.iter().any(|condition| condition == part),
                "{name} is drawn outside {part} (inside {shown:?})"
            );
        }
    }
}

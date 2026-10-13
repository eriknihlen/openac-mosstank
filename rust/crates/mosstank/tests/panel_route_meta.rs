//! The Route and Meta pages, driven through the window bindings (ported
//! from the C#'s `MossTankPanelTests`).

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::navigation::{NavigationObject, NavigationSnapshot};
use ac_plugin_abi::world::Position;
use common::World;
use mosstank::MossTank;

/// Standing at `east_west` map units (`NavigationAt(0f)` moved east).
fn stand_at(world: &mut World, east_west: f64) {
    world.set_reply::<calls::NavigationSnapshot>(&NavigationSnapshot {
        is_available: true,
        local_object_id: 1,
        position: Position {
            cell_id: 0x7F7F_0001,
            east_west,
            is_outdoor: true,
            ..Position::default()
        },
        ..NavigationSnapshot::default()
    });
}

fn fresh() -> (World, MossTank) {
    let mut world = World::with_character("Route Tester");
    stand_at(&mut world, 0.0);
    let plugin = world.enable();
    (world, plugin)
}

fn waypoints(world: &mut World, plugin: &mut MossTank) -> Vec<String> {
    world.ui_list(plugin, "RouteWaypointTextColumn")
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: RouteGridDeletesByAnyCellClickAndSelectsNearestByDistance
#[test]
fn the_route_grid_deletes_by_any_cell_and_selects_the_nearest() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    world.act(p, "AddRoutePoint");
    stand_at(&mut world, 50.0);
    world.act(p, "AddRoutePoint");
    assert_eq!(waypoints(&mut world, p).len(), 2);
    assert_eq!(world.ui_list(p, "RouteWaypointCountColumn"), ["1", "2"]);
    stand_at(&mut world, 48.0);
    world.act(p, "SelectNearestRouteWaypoint");
    assert_eq!(world.ui_int(p, "SelectedRouteWaypointIndex"), 1);
    world.act_int(p, "DeleteRouteWaypointAt", 0);
    assert_eq!(waypoints(&mut world, p).len(), 1);
    assert_eq!(world.ui_list(p, "RouteWaypointCountColumn"), ["1"]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: RouteInsertModeControlsWhereANewWaypointLands
#[test]
fn the_insert_mode_decides_where_a_new_waypoint_lands() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    assert_eq!(world.ui_text(p, "SelectedRouteInsertMode"), "Add to End");
    world.act(p, "AddRoutePoint");
    stand_at(&mut world, 20.0);
    world.act(p, "AddRoutePoint");
    let rows = waypoints(&mut world, p);
    assert_eq!(rows.len(), 2);
    assert!(rows[1].contains("20"), "{rows:?}");

    world.act_text(p, "SelectRouteInsertMode", "Insert Above");
    assert_eq!(world.ui_text(p, "SelectedRouteInsertMode"), "Insert Above");
    world.act_int(p, "SelectRouteWaypoint", 0);
    stand_at(&mut world, 99.0);
    world.act(p, "AddRoutePoint");
    let rows = waypoints(&mut world, p);
    assert_eq!(rows.len(), 3);
    assert!(rows[0].contains("99"), "{rows:?}");

    world.act_text(p, "SelectRouteInsertMode", "Insert Below");
    assert_eq!(world.ui_text(p, "SelectedRouteInsertMode"), "Insert Below");
    world.act_int(p, "SelectRouteWaypoint", 0);
    stand_at(&mut world, 77.0);
    world.act(p, "AddRoutePoint");
    let rows = waypoints(&mut world, p);
    assert_eq!(rows.len(), 4);
    assert!(rows[1].contains("77"), "{rows:?}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: RouteRecallComboShowsVtanksTerseCaptionsButTheWaypointKeepsTheFullName
#[test]
fn the_recall_menu_shows_terse_captions_and_the_waypoint_keeps_the_full_name() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    assert_eq!(
        world.ui_list(p, "RouteRecallNames"),
        [
            "Primary",
            "Secondary",
            "LS",
            "LS Sending",
            "Portal",
            "Aphus",
            "Sanctuary",
            "Caul",
            "GW",
            "Aerlinthe",
            "Mt. Lethe",
            "Ulgrim's",
            "Bur",
            "PtOIA",
            "Graveyard",
            "Colosseum",
            "Fac. Hub",
            "Gear K. Camp",
            "Neftet",
            "Candeth",
            "Rynthid",
            "VR Rocks",
            "VR Tree",
            "Soc. CH",
            "Soc. RB",
            "Soc. EW",
            "Marketplace",
        ]
    );
    assert_eq!(world.ui_text(p, "SelectedRouteRecall"), "Primary");
    world.act_text(p, "SelectRouteRecall", "LS");
    assert_eq!(world.ui_text(p, "SelectedRouteRecall"), "LS");
    world.act(p, "AddRouteRecall");
    assert!(world.ui_text(p, "RouteNotice").contains("Lifestone Recall"));
    let rows = waypoints(&mut world, p);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].contains("Lifestone Recall"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: RoutePauseSecondsFieldParsesAndClampsInput
#[test]
fn the_pause_seconds_field_parses_and_clamps() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    assert_eq!(world.ui_text(p, "RoutePauseSecondsFieldText"), "5");
    world.act_text(p, "SetRoutePauseSecondsText", "12");
    assert_eq!(world.ui_text(p, "RoutePauseSecondsFieldText"), "12");
    world.act_text(p, "SetRoutePauseSecondsText", "99999");
    assert_eq!(world.ui_text(p, "RoutePauseSecondsFieldText"), "3600");
    world.act_text(p, "SetRoutePauseSecondsText", "not a number");
    assert_eq!(world.ui_text(p, "RoutePauseSecondsFieldText"), "3600");
}

fn horan_in_view(world: &mut World) {
    world.set_reply::<calls::NavigationObject>(&Some(NavigationObject {
        object_id: 77,
        name: "Horan".to_owned(),
        ..NavigationObject::default()
    }));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: FollowTurnsTheLoadedRouteIntoAFollowRouteOnTheSelectedCharacter
#[test]
fn follow_turns_the_route_into_a_follow_route_on_the_selection() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    horan_in_view(&mut world);
    world.act(p, "ToggleNavigation");
    assert_eq!(world.ui_text(p, "SelectedRouteMode"), "Circular");
    assert!(waypoints(&mut world, p).is_empty());
    world.select_object(77);
    world.act(p, "SetFollowTarget");
    assert_eq!(world.ui_text(p, "SelectedRouteMode"), "Follow");
    assert_eq!(
        world.ui_text(p, "RouteFollowTargetText"),
        "Follow target: Horan"
    );
    assert_eq!(world.ui_text(p, "RouteNotice"), "Following Horan.");
    assert!(waypoints(&mut world, p).is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: FollowSaysWhenNavigationIsStillSwitchedOff
#[test]
fn follow_says_when_navigation_is_still_off() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    horan_in_view(&mut world);
    world.select_object(77);
    world.act(p, "SetFollowTarget");
    assert!(
        world
            .ui_text(p, "RouteNotice")
            .contains("Enable Navigation")
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: FollowWithNothingSelectedLeavesTheRouteAlone
#[test]
fn follow_with_nothing_selected_leaves_the_route_alone() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    world.act(p, "SetFollowTarget");
    assert_eq!(world.ui_text(p, "SelectedRouteMode"), "Circular");
    assert_eq!(
        world.ui_text(p, "RouteFollowTargetText"),
        "Follow target: [None]"
    );
    assert_eq!(
        world.ui_text(p, "RouteNotice"),
        "Select a live object to follow first."
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: AFollowRouteIsSavedAndReloadedWithItsTarget
#[test]
fn a_follow_route_is_saved_and_reloaded_with_its_target() {
    let (mut world, mut plugin) = fresh();
    horan_in_view(&mut world);
    world.select_object(77);
    world.act(&mut plugin, "SetFollowTarget");
    world.disable(&mut plugin);
    let mut reloaded = world.enable();
    let p = &mut reloaded;
    assert_eq!(world.ui_text(p, "SelectedRouteMode"), "Follow");
    assert_eq!(
        world.ui_text(p, "RouteFollowTargetText"),
        "Follow target: Horan"
    );
}

fn add_meta_rule(world: &mut World, plugin: &mut MossTank, state: Option<&str>, text: &str) {
    if let Some(state) = state {
        world.act_text(plugin, "SetMetaStateDraft", state);
    }
    world.act_text(plugin, "SetMetaActionTextDraft", text);
    world.act(plugin, "AddMetaRule");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MetaTabEditsAndExecutesTheLiveStateMachine
#[test]
fn the_meta_tab_edits_and_runs_the_live_state_machine() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    world.act(p, "ShowMeta");
    assert!(world.ui_bool(p, "MetaVisible"));
    world.act_text(p, "SetMetaStateDraft", "Default");
    world.act_text(p, "SelectMetaCondition", "Always");
    world.act_text(p, "SelectMetaAction", "SetMetaState");
    add_meta_rule(&mut world, p, None, "Hunt");
    let rows = world.ui_list(p, "MetaRows");
    assert_eq!(rows.len(), 1);
    assert!(rows[0].contains("Hunt"));
    world.act(p, "ToggleMeta");
    world.act(p, "ToggleCombat");
    world.with_tank(p, |tank, client| tank.on_tick(client, 0.3));
    assert!(plugin.tank().unwrap().meta_enabled());
    assert_eq!(plugin.tank().unwrap().meta_state(), "Hunt");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MetaGridColumnsMatchTheRulesAndDeleteMoveActOnTheClickedRow
#[test]
fn the_meta_grid_columns_match_the_rules_and_act_on_the_clicked_row() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    world.act_text(p, "SelectMetaAction", "ChatCommand");
    add_meta_rule(&mut world, p, Some("Idle"), "/mt one");
    add_meta_rule(&mut world, p, Some("Hunt"), "/mt two");
    assert_eq!(world.ui_list(p, "MetaStateColumn"), ["Idle", "Hunt"]);
    let actions = world.ui_list(p, "MetaActionColumn");
    assert!(actions[0].contains("/mt one"));
    assert!(actions[1].contains("/mt two"));
    assert_eq!(world.ui_list(p, "MetaDeleteColumn"), ["X", "X"]);
    assert!(
        world
            .ui_uints(p, "MetaMoveUpIcons")
            .iter()
            .all(|id| *id == 0x0600_28FC)
    );
    assert!(
        world
            .ui_uints(p, "MetaMoveDownIcons")
            .iter()
            .all(|id| *id == 0x0600_28FD)
    );
    world.act_int(p, "MoveMetaRuleDownAt", 0);
    assert_eq!(world.ui_list(p, "MetaStateColumn"), ["Hunt", "Idle"]);
    world.act_int(p, "MoveMetaRuleUpAt", 1);
    assert_eq!(world.ui_list(p, "MetaStateColumn"), ["Idle", "Hunt"]);
    world.act_int(p, "DeleteMetaRuleAt", 0);
    assert_eq!(world.ui_list(p, "MetaStateColumn"), ["Hunt"]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MetaEditorPopupOpensOnCellClickOrCreateAndClosesOnApplyOrCancel
#[test]
fn the_meta_editor_opens_on_a_cell_or_create_and_closes_on_apply_or_cancel() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    assert!(!world.ui_bool(p, "MetaEditorVisible"));
    world.act(p, "CreateMetaRule");
    assert!(world.ui_bool(p, "MetaEditorVisible"));
    assert_eq!(world.ui_list(p, "MetaRows").len(), 1);
    world.act(p, "HideMetaEditor");
    assert!(!world.ui_bool(p, "MetaEditorVisible"));
    world.act_int(p, "SelectMetaRule", 0);
    assert!(world.ui_bool(p, "MetaEditorVisible"));
    world.act(p, "ApplyMetaRule");
    assert!(!world.ui_bool(p, "MetaEditorVisible"));
    world.act_int(p, "SelectMetaRule", 0);
    assert!(world.ui_bool(p, "MetaEditorVisible"));
    world.act(p, "HideMetaEditor");
    assert!(!world.ui_bool(p, "MetaEditorVisible"));
    world.act_int(p, "DeleteMetaRuleAt", 0);
    assert!(!world.ui_bool(p, "MetaEditorVisible"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: SwitchingTabsClosesTheBuffPickerAndMetaEditorPopups
#[test]
fn switching_tabs_closes_the_buff_picker_and_the_meta_editor() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    world.act(p, "ShowExtraBuffPicker");
    assert!(world.ui_bool(p, "BuffPickerVisible"));
    world.act(p, "ShowOptions");
    assert!(!world.ui_bool(p, "BuffPickerVisible"));
    world.act(p, "ShowMeta");
    world.act(p, "CreateMetaRule");
    assert!(world.ui_bool(p, "MetaEditorVisible"));
    world.act(p, "ShowOptions");
    assert!(!world.ui_bool(p, "MetaEditorVisible"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MetaCurrentStateMenuForcesTheLiveEngineIntoTheChosenState
#[test]
fn the_current_state_menu_forces_the_engine_into_the_chosen_state() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    world.act_text(p, "SelectMetaAction", "ChatCommand");
    add_meta_rule(&mut world, p, Some("Default"), "/mt hi");
    add_meta_rule(&mut world, p, Some("Hunt"), "/mt hi");
    assert!(
        world
            .ui_list(p, "MetaCurrentStateNames")
            .iter()
            .any(|state| state == "Hunt")
    );
    assert_eq!(world.ui_text(p, "SelectedMetaCurrentState"), "Default");
    world.act_text(p, "SetMetaCurrentState", "Hunt");
    assert_eq!(world.ui_text(p, "SelectedMetaCurrentState"), "Hunt");
    assert_eq!(plugin.tank().unwrap().meta_state(), "Hunt");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MetaProfilesAreIndependentDurableDocuments
#[test]
fn meta_profiles_are_independent_durable_documents() {
    let mut world = World::panel_character("Meta Maker");
    let mut first = world.enable();
    let p = &mut first;
    world.act_text(p, "SelectMetaAction", "ChatCommand");
    add_meta_rule(&mut world, p, None, "/mt status");
    world.vt(p, "meta save Hunting");
    assert_eq!(world.ui_text(p, "SelectedMetaProfile"), "Hunting");
    assert_eq!(world.ui_list(p, "MetaRows").len(), 1);
    world.act_text(p, "SelectMetaProfile", "By char");
    world.act(p, "AddMetaRule");
    assert_eq!(world.ui_text(p, "SelectedMetaProfile"), "By char");
    assert_eq!(world.ui_list(p, "MetaRows").len(), 2);
    world.disable(p);

    let mut second = world.enable();
    let p = &mut second;
    assert_eq!(world.ui_text(p, "SelectedMetaProfile"), "By char");
    assert_eq!(world.ui_list(p, "MetaRows").len(), 2);
    world.act_text(p, "SelectMetaProfile", "Hunting");
    let rows = world.ui_list(p, "MetaRows");
    assert_eq!(rows.len(), 1);
    assert!(rows[0].contains("/mt status"));
}

// Not applicable: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MetaAndRouteGridColumnsDoNotReallocateOnEveryRead: .NET reference identity of the columns a property returns; a window binding here is a value, worked out for each push

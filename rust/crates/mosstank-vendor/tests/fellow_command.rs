//! `/ub fellow`: each sub-command's grammar and the one call it hands the
//! client, the name forms, and the client's refusals
//! (`MossTankPanelTests.UbFellow`, the command's half; the `/ub`
//! dispatcher's help lines belong to the UB tab).

use std::collections::HashMap;

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::Skill;
use ac_plugin_abi::combat::{CombatMode, CombatResult, CombatStatus};
use ac_plugin_abi::items::{InventoryItem, ItemResult, ItemStatus};
use ac_plugin_abi::magic::{CastGate, SpellInfo};
use ac_plugin_abi::navigation::{
    MovementIntent, NavigationObject, NavigationSnapshot, NavigationStatus,
};
use ac_plugin_abi::social::{FellowMember, FellowshipState, FellowshipStatus};
use ac_plugin_abi::world::{ItemProperties, ObjectClass, Position, WorldObject};
use mosstank_automation::{Automation, SpellCatalog};
use mosstank_nav::NavigationAutomation;
use mosstank_vendor::FellowshipAutomation;
use mosstank_vendor::fellow_command::{
    self, FELLOW_USAGE, FellowCommandAutomation, FellowCommandOutcome,
};
use mosstank_vendor::ub_chat::{ERROR_CHAT_TYPE, GENERIC_CHAT_TYPE, UbChatSink};

const SELF: u32 = 1;
const YONNEH: u32 = 0x5000_0AAA;
const SAWATO: u32 = 0x5000_0BBB;

/// A fellowship a test sets up, recording each call it is sent, on a client
/// whose character is id 1 (`FakeFellowship` with the panel tests'
/// `FakeAutomation`).
#[derive(Debug)]
struct Fellows {
    is_in_fellowship: bool,
    name: String,
    leader_id: u32,
    is_open: bool,
    is_locked: bool,
    shares_experience: bool,
    splits_experience_evenly: bool,
    roster: Vec<FellowMember>,
    levels: HashMap<u32, u32>,
    calls: Vec<String>,
    result: FellowshipStatus,
    world_objects: Vec<WorldObject>,
    navigation: NavigationSnapshot,
    selected: Option<u32>,
    /// Every line posted, with its text class.
    posted: Vec<(String, u32)>,
}

impl Fellows {
    fn record(&mut self, call: String) -> FellowshipStatus {
        self.calls.push(call);
        self.result
    }

    fn messages(&self) -> Vec<&str> {
        self.posted.iter().map(|(text, _)| text.as_str()).collect()
    }
}

impl SpellCatalog for Fellows {
    fn known_self_buffs(&mut self) -> Vec<SpellInfo> {
        Vec::new()
    }

    fn try_get(&mut self, _spell_id: u32) -> Option<SpellInfo> {
        None
    }
}

impl Automation for Fellows {
    fn is_available(&mut self) -> bool {
        true
    }

    fn character(&mut self) -> CharacterInfo {
        CharacterInfo {
            id: SELF,
            ..CharacterInfo::default()
        }
    }

    fn skills(&mut self) -> Vec<Skill> {
        Vec::new()
    }

    fn spells(&mut self) -> &mut dyn SpellCatalog {
        self
    }

    fn evaluate_gate(&mut self, _spell_id: u32, _target: Option<u32>) -> CastGate {
        CastGate::Refused
    }

    fn cast(&mut self, _spell_id: u32, _target: Option<u32>) -> bool {
        false
    }

    fn combat_mode(&mut self) -> CombatMode {
        CombatMode::Unknown
    }

    fn enter_combat_mode(&mut self, _mode: CombatMode) -> CombatResult {
        CombatResult {
            status: CombatStatus::Unavailable,
            notice: None,
        }
    }

    fn items_available(&mut self) -> bool {
        false
    }

    fn owned_items(&mut self) -> Vec<InventoryItem> {
        Vec::new()
    }

    fn use_item(&mut self, _object_id: u32) -> ItemResult {
        ItemResult::of(ItemStatus::Unavailable)
    }

    fn apply_item(&mut self, _object_id: u32, _target_id: u32) -> ItemResult {
        ItemResult::of(ItemStatus::Unavailable)
    }

    fn item_properties(&mut self, _object_id: u32) -> Option<ItemProperties> {
        None
    }

    fn post_system_message(&mut self, text: &str) {
        self.posted.push((text.to_owned(), 0));
    }

    fn log_info(&mut self, _text: &str) {}

    fn log_warn(&mut self, _text: &str) {}
}

impl NavigationAutomation for Fellows {
    fn navigation(&mut self) -> NavigationSnapshot {
        self.navigation.clone()
    }

    fn navigation_object(&mut self, _object_id: u32) -> Option<NavigationObject> {
        None
    }

    fn set_movement_intent(&mut self, _intent: MovementIntent) -> NavigationStatus {
        NavigationStatus::Unavailable
    }

    fn clear_movement_intent(&mut self) -> NavigationStatus {
        NavigationStatus::Unavailable
    }
}

impl UbChatSink for Fellows {
    fn post_message(&mut self, text: &str, chat_type: u32) {
        self.posted.push((text.to_owned(), chat_type));
    }
}

impl FellowshipAutomation for Fellows {
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    fn fellowship_state(&mut self) -> FellowshipState {
        FellowshipState {
            is_in_fellowship: self.is_in_fellowship,
            name: self.name.clone(),
            leader_id: self.leader_id,
            is_open: self.is_open,
            is_locked: self.is_locked,
            member_count: self.roster.len() as i32,
        }
    }

    fn fellowship_roster(&mut self) -> Vec<FellowMember> {
        self.roster.clone()
    }

    fn fellowship_recruit(&mut self, player_id: u32) -> FellowshipStatus {
        self.record(format!("recruit {player_id:08X}"))
    }

    fn fellowship_dismiss(&mut self, player_id: u32) -> FellowshipStatus {
        self.record(format!("dismiss {player_id:08X}"))
    }

    fn fellowship_assign_leader(&mut self, player_id: u32) -> FellowshipStatus {
        self.record(format!("leader {player_id:08X}"))
    }

    fn fellowship_set_open(&mut self, open: bool) -> FellowshipStatus {
        self.record(format!("open={}", if open { "True" } else { "False" }))
    }
}

impl FellowCommandAutomation for Fellows {
    fn fellowship_create(&mut self, name: &str, share_experience: bool) -> FellowshipStatus {
        self.record(format!(
            "create {name} share={}",
            if share_experience { "True" } else { "False" }
        ))
    }

    fn fellowship_quit(&mut self, disband: bool) -> FellowshipStatus {
        self.record(format!(
            "quit disband={}",
            if disband { "True" } else { "False" }
        ))
    }

    fn fellowship_shares_experience(&mut self) -> bool {
        self.shares_experience
    }

    fn fellowship_splits_experience_evenly(&mut self) -> bool {
        self.splits_experience_evenly
    }

    fn fellow_level(&mut self, member: &FellowMember) -> u32 {
        self.levels.get(&member.object_id).copied().unwrap_or(0)
    }

    fn world_objects(&mut self) -> Vec<WorldObject> {
        self.world_objects.clone()
    }

    fn selected_object_id(&mut self) -> Option<u32> {
        self.selected
    }
}

fn fellow(id: u32, name: &str, distance: f32) -> FellowMember {
    FellowMember {
        object_id: id,
        name: name.to_owned(),
        current_health: 100,
        max_health: 120,
        current_stamina: 100,
        max_stamina: 100,
        current_mana: 100,
        max_mana: 100,
        distance,
        ..FellowMember::default()
    }
}

fn map_position(east_west: f64) -> Position {
    Position {
        cell_id: 0x0001_0001,
        east_west,
        north_south: 0.0,
        elevation: 0.0,
        heading_degrees: 0.0,
        is_outdoor: true,
    }
}

fn player_at(object_id: u32, name: &str, east_west_meters: f64) -> WorldObject {
    WorldObject {
        object_id,
        name: name.to_owned(),
        object_class: ObjectClass::Player,
        is_landscape: true,
        position: Some(map_position(east_west_meters / 240.0)),
        ..WorldObject::default()
    }
}

fn navigation_at_origin() -> NavigationSnapshot {
    NavigationSnapshot {
        is_available: true,
        is_portal_space: false,
        local_object_id: 1,
        position: map_position(0.0),
        ..NavigationSnapshot::default()
    }
}

/// `FellowRig`: in an open fellowship the character leads, with Yonneh and
/// Sawato on the roster.
fn rig(
    in_fellowship: bool,
    leader: u32,
    is_open: bool,
    roster: Option<Vec<FellowMember>>,
) -> Fellows {
    Fellows {
        is_in_fellowship: in_fellowship,
        name: "Mosswart Hunters".to_owned(),
        leader_id: leader,
        is_open,
        is_locked: false,
        shares_experience: false,
        splits_experience_evenly: false,
        roster: roster.unwrap_or_else(|| {
            vec![
                fellow(SELF, "Me", 0.0),
                fellow(YONNEH, "Yonneh", 20.0),
                fellow(SAWATO, "Sawato Rockstyle", 5.0),
            ]
        }),
        levels: HashMap::new(),
        calls: Vec::new(),
        result: FellowshipStatus::Accepted,
        world_objects: Vec::new(),
        navigation: NavigationSnapshot::default(),
        selected: None,
        posted: Vec::new(),
    }
}

fn default_rig() -> Fellows {
    rig(true, SELF, true, None)
}

/// `UbCommand(panel, line)` for a `fellow` line, debug off.
fn command(fellows: &mut Fellows, line: &str) -> FellowCommandOutcome {
    let arguments = line.strip_prefix("fellow").expect("a fellow line");
    fellow_command::run(fellows, arguments, false)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbFellow.cs :: FellowCreateAsksForTheWholeNameSharingExperience
#[test]
fn fellow_create_asks_for_the_whole_name_sharing_experience() {
    let mut fellows = rig(false, SELF, true, None);

    command(&mut fellows, "fellow create Sawato Rockstyle");

    assert_eq!(fellows.calls, ["create Sawato Rockstyle share=True"]);
    assert!(fellows.posted.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbFellow.cs :: FellowCreateInsideAFellowshipSendsNothing
#[test]
fn fellow_create_inside_a_fellowship_sends_nothing() {
    let mut fellows = default_rig();

    command(&mut fellows, "fellow create Again");

    assert!(fellows.calls.is_empty());
    assert_eq!(
        fellows.messages(),
        ["[UB] You are already in a fellowship."]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbFellow.cs :: FellowCommandsOutsideAFellowshipSayWhy
#[test]
fn fellow_commands_outside_a_fellowship_say_why() {
    for line in ["fellow quit", "fellow recruit Yonneh", "fellow status"] {
        let mut fellows = rig(false, SELF, true, None);

        command(&mut fellows, line);

        assert!(fellows.calls.is_empty(), "{line}");
        assert_eq!(
            fellows.messages(),
            ["[UB] Your are not currently in a fellowship."]
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbFellow.cs :: FellowQuitAndDisbandSendTheRightQuit
#[test]
fn fellow_quit_and_disband_send_the_right_quit() {
    for (line, call) in [
        ("fellow quit", "quit disband=False"),
        ("fellow disband", "quit disband=True"),
        ("fellow QUIT", "quit disband=False"),
    ] {
        let mut fellows = default_rig();

        command(&mut fellows, line);

        assert_eq!(fellows.calls, [call], "{line}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbFellow.cs :: FellowDisbandNeedsTheLead
#[test]
fn fellow_disband_needs_the_lead() {
    let mut fellows = rig(true, SAWATO, true, None);

    command(&mut fellows, "fellow disband");

    assert!(fellows.calls.is_empty());
    assert_eq!(
        fellows.messages(),
        ["[UB] You are not the fellowship leader!"]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbFellow.cs :: FellowOpenAndCloseAskOnlyWhenItWouldChange
#[test]
fn fellow_open_and_close_ask_only_when_it_would_change() {
    for (line, is_open, leader, call) in [
        ("fellow open", false, SELF, Some("open=True")),
        ("fellow close", true, SELF, Some("open=False")),
        ("fellow open", true, SELF, None),
        ("fellow close", false, SELF, None),
        ("fellow open", false, SAWATO, None),
    ] {
        let mut fellows = rig(true, leader, is_open, None);

        command(&mut fellows, line);

        let expected: Vec<&str> = call.into_iter().collect();
        assert_eq!(fellows.calls, expected, "{line} {is_open} {leader}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbFellow.cs :: FellowStatusPrintsTheFellowshipAndEachMember
#[test]
fn fellow_status_prints_the_fellowship_and_each_member() {
    for (shares, even, clause) in [
        (true, true, "Sharing XP"),
        (true, false, "Sharing XP, Uneven Split"),
        (false, true, "NOT Sharing XP"),
        (false, false, "NOT Sharing XP, Uneven Split"),
    ] {
        let mut fellows = default_rig();
        fellows.shares_experience = shares;
        fellows.splits_experience_evenly = even;
        fellows.levels = fellows
            .roster
            .iter()
            .zip(100..)
            .map(|(member, level)| (member.object_id, level))
            .collect();

        command(&mut fellows, "fellow status");

        assert!(fellows.calls.is_empty());
        assert_eq!(
            fellows.messages(),
            [
                format!(
                    "[UB] 00000001 00000001 Your current fellowship, \"Mosswart Hunters\", has 3 members, {clause}. Open, Not Locked."
                )
                .as_str(),
                "[UB]  Me[100] H:100/120 (Leader) ",
                "[UB]  Yonneh[101] H:100/120",
                "[UB]  Sawato Rockstyle[102] H:100/120",
            ]
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbFellow.cs :: FellowRecruitResolvesThePlayer
#[test]
fn fellow_recruit_resolves_the_player() {
    for (line, recruited) in [
        ("fellow recruit Yonneh", YONNEH),
        ("fellow recruitp yon", YONNEH),
        ("fellow recruit", SAWATO),
        ("fellow recruit selected", YONNEH),
        ("fellow recruit Yon", 0),
    ] {
        let mut fellows = rig(true, SELF, true, Some(vec![fellow(SELF, "Me", 0.0)]));
        fellows.world_objects = vec![
            player_at(YONNEH, "Yonneh", 20.0),
            player_at(SAWATO, "Sawato Rockstyle", 5.0),
        ];
        fellows.navigation = navigation_at_origin();
        fellows.selected = Some(YONNEH);

        command(&mut fellows, line);

        if recruited == 0 {
            // Silent, as the reference is without its debug output.
            assert!(fellows.calls.is_empty(), "{line}");
            assert!(fellows.posted.is_empty(), "{line}");
            continue;
        }
        assert_eq!(
            fellows.calls,
            [format!("recruit {recruited:08X}")],
            "{line}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbFellow.cs :: FellowTargetsAreNamedOnlyWithDebugOn
#[test]
fn fellow_targets_are_named_only_with_debug_on() {
    let mut fellows = default_rig();
    fellows.world_objects = vec![player_at(YONNEH, "Yonneh", 20.0)];
    fellows.navigation = navigation_at_origin();

    for line in [
        " recruit Yonneh",
        " recruit Nobody",
        " dismiss Sawato Rockstyle",
        " leader Yonneh",
        " leader Nobody",
    ] {
        fellow_command::run(&mut fellows, line, true);
    }

    assert_eq!(
        fellows.posted,
        [
            (
                "[UB] Recruiting Yonneh[0x50000AAA]".to_owned(),
                GENERIC_CHAT_TYPE
            ),
            (
                "[UB] Error: Could not find player Nobody".to_owned(),
                ERROR_CHAT_TYPE
            ),
            (
                "[UB] Dismissing Sawato Rockstyle[0x50000BBB]".to_owned(),
                GENERIC_CHAT_TYPE
            ),
            (
                "[UB] Transferring leader to Yonneh[0x50000AAA]".to_owned(),
                GENERIC_CHAT_TYPE
            ),
            (
                "[UB] Error: Could not find player Nobody".to_owned(),
                ERROR_CHAT_TYPE
            ),
        ]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbFellow.cs :: FellowRecruitSkipsMembersAndFarPlayers
#[test]
fn fellow_recruit_skips_members_and_far_players() {
    for (east_west_meters, already_member, sent) in [
        (20.0, true, false),
        (80.0, false, false),
        (20.0, false, true),
    ] {
        let mut roster = vec![fellow(SELF, "Me", 0.0)];
        if already_member {
            roster.push(fellow(YONNEH, "Yonneh", 20.0));
        }
        let mut fellows = rig(true, SELF, true, Some(roster));
        fellows.world_objects = vec![player_at(YONNEH, "Yonneh", east_west_meters)];
        fellows.navigation = navigation_at_origin();

        command(&mut fellows, "fellow recruit Yonneh");

        assert_eq!(fellows.calls.len(), usize::from(sent));
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbFellow.cs :: FellowDismissAndLeaderResolveTheMember
#[test]
fn fellow_dismiss_and_leader_resolve_the_member() {
    for (line, call) in [
        ("fellow dismiss Yonneh", "dismiss 50000AAA"),
        ("fellow dismissp rock", "dismiss 50000BBB"),
        ("fellow dismiss", "dismiss 50000BBB"),
        ("fellow dismiss 0x50000AAA", "dismiss 50000AAA"),
        ("fellow dismiss selected", "dismiss 50000AAA"),
        ("fellow leader Sawato Rockstyle", "leader 50000BBB"),
        ("fellow leaderp yon", "leader 50000AAA"),
        ("fellow leader", "leader 50000BBB"),
    ] {
        let mut fellows = default_rig();
        fellows.selected = Some(YONNEH);

        command(&mut fellows, line);

        assert_eq!(fellows.calls, [call], "{line}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbFellow.cs :: FellowDismissByPartOfANameNeedsP
#[test]
fn fellow_dismiss_by_part_of_a_name_needs_p() {
    let mut fellows = default_rig();

    command(&mut fellows, "fellow dismiss rock");

    assert!(fellows.calls.is_empty());
    assert!(fellows.posted.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbFellow.cs :: FellowDismissingYourselfQuits
#[test]
fn fellow_dismissing_yourself_quits() {
    let mut fellows = default_rig();

    command(&mut fellows, "fellow dismiss 1");

    assert_eq!(fellows.calls, ["quit disband=False"]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbFellow.cs :: FellowDismissAndLeaderNeedTheLead
#[test]
fn fellow_dismiss_and_leader_need_the_lead() {
    for line in ["fellow dismiss Yonneh", "fellow leader Yonneh"] {
        let mut fellows = rig(true, SAWATO, true, None);

        command(&mut fellows, line);

        assert!(fellows.calls.is_empty(), "{line}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbFellow.cs :: AFellowCommandTheClientDidNotSendIsReported
#[test]
fn a_fellow_command_the_client_did_not_send_is_reported() {
    for (status, said) in [
        (
            FellowshipStatus::Rejected,
            "[UB] Error: The client refused to dismiss Yonneh.",
        ),
        (
            FellowshipStatus::Unavailable,
            "[UB] Error: Cannot dismiss Yonneh right now.",
        ),
    ] {
        let mut fellows = default_rig();
        fellows.result = status;

        command(&mut fellows, "fellow dismiss Yonneh");

        assert_eq!(fellows.calls, ["dismiss 50000AAA"]);
        assert_eq!(fellows.messages().last().copied(), Some(said));
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbFellow.cs :: BadFellowInputPrintsTheUsageLine (the grammar; the usage lines are the /ub dispatcher's)
#[test]
fn bad_fellow_input_is_bad_syntax() {
    for line in [
        "fellow",
        "fellow create",
        "fellow invite Yonneh",
        "fellow quit now",
    ] {
        let mut fellows = default_rig();

        assert_eq!(
            command(&mut fellows, line),
            FellowCommandOutcome::BadSyntax,
            "{line}"
        );

        assert!(fellows.calls.is_empty());
        assert!(fellows.posted.is_empty());
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbFellow.cs :: FellowHasAUsageLine (the usage text; `help fellow` is the dispatcher's)
#[test]
fn fellow_has_a_usage_line() {
    assert_eq!(
        format!("[UB] Usage: {FELLOW_USAGE}"),
        "[UB] Usage: /ub fellow create <Name>|quit|disband|open|close|status|recruit[p][ Name]|dismiss[p][ Name]|leader[p][ Name]"
    );
}

//! The `/vt` verbs that test and dump what the macro sees (ported from the
//! C#'s `MossTankPanelTests`, and pinned to the C#'s words where it has no
//! test of its own).

mod common;

use ac_plugin_abi::character::{Skill, Training};
use ac_plugin_abi::social::RecoveryResult;
use ac_plugin_abi::world::{ObjectClass, WorldObject};
use ac_plugin_abi::{calls, magic::CastGate};
use common::{World, spell};

fn world_with(name: &str) -> (World, mosstank::MossTank) {
    let mut world = World::with_character(name);
    let plugin = world.enable();
    world.clear_chat();
    (world, plugin)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: VtankRecoveryAndFakeImperilCommandsHaveRealLocalSemantics
#[test]
fn vtank_recovery_and_fake_imperil_commands_have_real_local_semantics() {
    let mut world = World::with_character("Recoverer");
    // The client holds two busy references and lets go of one.
    world.set_reply::<calls::RecoveryClearOneBusy>(&RecoveryResult {
        accepted: true,
        previous_count: 2,
        current_count: 1,
        message: String::new(),
    });
    world.set_reply::<calls::WorldObject>(&Some(WorldObject {
        object_id: 0x5000_0001,
        weenie_class_id: 123,
        name: "Drudge".to_owned(),
        object_class: ObjectClass::Monster,
        ..WorldObject::default()
    }));
    let mut plugin = world.enable();

    world.vt(&mut plugin, "clearbusy");
    assert_eq!(world.requests::<calls::RecoveryClearOneBusy>().len(), 1);
    assert!(world.chat().iter().any(|text| text.contains("2 -> 1")));

    world.vt(&mut plugin, "clearlocks");
    assert!(
        world
            .chat()
            .iter()
            .any(|text| text.contains("Action locks cleared"))
    );

    world.select_object(0x5000_0001);
    world.vt(&mut plugin, "fakeimp");
    assert!(
        world
            .chat()
            .iter()
            .any(|text| text.contains("Fake cast complete"))
    );
}

// ---- Not ports of C# tests: the C#'s words for each verb -------------------

#[test]
fn a_refused_clear_busy_says_the_clients_reason() {
    let (mut world, mut plugin) = world_with("Recoverer");
    world.set_reply::<calls::RecoveryClearOneBusy>(&RecoveryResult {
        accepted: false,
        previous_count: 0,
        current_count: 0,
        message: "Nothing is busy.".to_owned(),
    });

    world.vt(&mut plugin, "clearbusy");

    assert_eq!(world.chat(), ["[MossTank] Nothing is busy."]);
}

#[test]
fn test_spell_reads_the_number_then_the_spell_then_says_whether_it_can_be_cast() {
    let (mut world, mut plugin) = world_with("Caster");

    world.vt(&mut plugin, "testspell lots");
    world.set_reply::<calls::SpellsGet>(&None);
    world.vt(&mut plugin, "testspell 7");
    let mut known = spell(7, 12, "A spell");
    known.difficulty = 150;
    world.set_reply::<calls::SpellsGet>(&Some(known));
    world.set_reply::<calls::SpellsIsKnown>(&true);
    world.set_reply::<calls::MagicGate>(&CastGate::NotKnown);
    world.vt(&mut plugin, "testspell 7");

    assert_eq!(
        world.chat(),
        [
            "[MossTank] Syntax: /vt testspell [spellid]",
            "[MossTank] Invalid spellid.",
            "[MossTank] ---------------------------------",
            "[MossTank] Testing ability to cast spell Spell 7, family 12, quality 150, diff 150",
            "[MossTank] Known: True, cast gate: NotKnown",
            "[MossTank] ---------------------------------",
        ]
    );
}

#[test]
fn meta_function_help_names_the_function_and_its_argument_count() {
    let (mut world, mut plugin) = world_with("Helper");

    world.vt(&mut plugin, "metafunchelp nosuchfunction");
    world.vt(&mut plugin, "metafunchelp LISTCREATE");

    let chat = world.chat();
    assert_eq!(
        chat[0],
        "[MossTank] Function not defined \"nosuchfunction\""
    );
    assert_eq!(chat[1], "[MossTank] -------------------------------");
    assert_eq!(chat[2], "[MossTank] Function: listcreate");
    assert!(chat[3].starts_with("[MossTank] Signature: "), "{}", chat[3]);
    assert!(
        chat[4].starts_with("[MossTank] Description: "),
        "{}",
        chat[4]
    );
    assert_eq!(chat[5], "[MossTank] Parameter count: 0..2147483647");
    assert_eq!(chat[6], "[MossTank] -------------------------------");
    assert_eq!(chat.len(), 7);
}

#[test]
fn list_meta_functions_lists_every_name_in_lines_of_at_most_240_characters() {
    let (mut world, mut plugin) = world_with("Lister");

    world.vt(&mut plugin, "listmetafunctions");

    let chat = world.chat();
    assert_eq!(chat[0], "[MossTank] Meta functions built in to MossTank:");
    let names: Vec<&str> = chat[1..]
        .iter()
        .flat_map(|line| {
            let body = line.strip_prefix("[MossTank] ").unwrap();
            assert!(body.len() <= 240, "{body}");
            body.split(", ")
        })
        .collect();
    assert!(names.contains(&"listcreate"));
    let mut sorted = names.clone();
    sorted.sort_by_key(|name| name.to_ascii_uppercase());
    assert_eq!(names, sorted);
}

#[test]
fn dump_skills_on_vt_prints_each_skill_by_id() {
    let (mut world, mut plugin) = world_with("Skilled");
    world.set_reply::<calls::CharacterSkills>(&vec![
        Skill {
            skill_id: 54,
            name: "Summoning".to_owned(),
            training: Training::Trained,
            base: 200,
            current: 300,
            ..Skill::default()
        },
        Skill {
            skill_id: 6,
            name: "Melee Defense".to_owned(),
            training: Training::Specialized,
            base: 100,
            current: 120,
            ..Skill::default()
        },
    ]);

    world.vt(&mut plugin, "dumpskills");

    assert_eq!(
        world.chat(),
        [
            "[MossTank] Character skills (2):",
            "[MossTank] 6\tMelee Defense\t100\t120\tSpecialized",
            "[MossTank] 54\tSummoning\t200\t300\tTrained",
        ]
    );
}

#[test]
fn property_dump_on_vt_needs_an_appraised_selection() {
    let (mut world, mut plugin) = world_with("Dumper");

    world.vt(&mut plugin, "propertydump");

    assert_eq!(
        world.chat(),
        [
            "[MossTank] Propertydump: Either no object selected or current selection object is invalid or not appraised."
        ]
    );
}

#[test]
fn equip_items_for_without_a_name_says_how_it_is_used() {
    let (mut world, mut plugin) = world_with("Dresser");

    world.vt(&mut plugin, "equipitemsfor");

    assert_eq!(
        world.chat(),
        [
            "[MossTank] Syntax: /vt equipitemsfor [monster name]",
            "[MossTank] NOTE: each use of this command invokes one equipment step; multiple calls may be required.",
        ]
    );
}

#[test]
fn the_tests_without_a_selection_ask_for_one() {
    let (mut world, mut plugin) = world_with("Tester");

    world.vt(&mut plugin, "testitem");
    world.vt(&mut plugin, "testmonster");

    assert_eq!(
        world.chat(),
        [
            "[MossTank] TestItem: pick an item first.",
            "[MossTank] TestMonster: pick a monster first.",
        ]
    );
}

#[test]
fn the_dumps_head_their_tables_with_a_count() {
    let (mut world, mut plugin) = world_with("Dumper");

    world.vt(&mut plugin, "dumpspells");
    world.vt(&mut plugin, "dumpspecies");
    world.vt(&mut plugin, "dumpmetavars");

    assert_eq!(
        world.chat(),
        [
            "[MossTank] Known spell table (0):",
            "[MossTank] Currently observed species (0):",
            "[MossTank] Assigned meta variables:",
        ]
    );
}

/// `^\[MossTank\] Pet can spawn: <answer>, test time: [0-9.]+ms$`.
fn is_room_line(line: &str, answer: &str) -> bool {
    line.strip_prefix(&format!("[MossTank] Pet can spawn: {answer}, test time: "))
        .and_then(|rest| rest.strip_suffix("ms"))
        .is_some_and(|time| {
            !time.is_empty() && time.chars().all(|c| c.is_ascii_digit() || c == '.')
        })
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: TestPetReportsTheRoomAheadThenThePetChoice
/// `/vt testpet` asks whether there is room three metres ahead for a pet to
/// appear, and how long the check took; a client that cannot tell answers
/// True, as the summon rule counts it. The pet choice follows on a line of
/// its own.
#[test]
fn test_pet_reports_the_room_ahead_then_the_pet_choice() {
    use ac_plugin_abi::navigation::RoomAheadStatus;
    let mut automation = common::automation::FakeAutomation::default();
    automation.room_ahead = RoomAheadStatus::Blocked;
    let mut world = World::with_automation(automation);
    let mut plugin = world.enable();

    world.vt(&mut plugin, "testpet");

    assert_eq!(world.automation().room_ahead_asked, [3.0]);
    let chat = world.chat();
    assert!(is_room_line(&chat[chat.len() - 2], "False"), "{chat:?}");
    assert_eq!(chat[chat.len() - 1], "[MossTank] Pet choice: none");

    world.automation().room_ahead = RoomAheadStatus::Unknown;
    world.vt(&mut plugin, "testpet");
    let chat = world.chat();
    assert!(is_room_line(&chat[chat.len() - 2], "True"), "{chat:?}");

    world.automation().room_ahead = RoomAheadStatus::Clear;
    world.vt(&mut plugin, "testpet");
    let chat = world.chat();
    assert!(is_room_line(&chat[chat.len() - 2], "True"), "{chat:?}");
}

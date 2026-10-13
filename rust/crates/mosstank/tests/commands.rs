//! `/vt` and `/ub` are two command sets (ported from the C#'s
//! `MossTankPanelTests.CommandNamespaces`): each word answers its own
//! commands and answers the other's as it answers any word it does not
//! know.

mod common;

use ac_plugin_abi::calls;
use common::World;
use mosstank_meta::chat::UB_ERROR_CHAT_TYPE;

const VT: &str = "vt";
const UB: &str = "ub";
const UNKNOWN_VT: &str = "[MossTank] Unknown /vt command. Use /vt help.";
const UNKNOWN_UB: &str = "[UB] Error: Command not found! Type \"ub help\" for a list of commands.";

fn namespace_world() -> (World, mosstank::MossTank) {
    let mut world = World::with_character("Namespace Tester");
    let plugin = world.enable();
    world.clear_chat();
    (world, plugin)
}

fn unknown_reply(word: &str) -> &'static str {
    if word == VT { UNKNOWN_VT } else { UNKNOWN_UB }
}

fn is_unknown_reply(message: &str) -> bool {
    message == UNKNOWN_VT || message == UNKNOWN_UB
}

const CASES: &[(&str, &str)] = &[
    (VT, "start"),
    (VT, "stop"),
    (VT, "forcebuff"),
    (VT, "cancelforcebuff"),
    (VT, "settings"),
    (VT, "nav"),
    (VT, "loot"),
    (VT, "meta"),
    (VT, "setmetastate"),
    (VT, "echo hello"),
    (VT, "setattackbar 0.5"),
    (VT, "tapjump"),
    (VT, "addnavjump"),
    (VT, "addnavpt"),
    (VT, "addnavcheckpoint"),
    (VT, "reverseroute"),
    (VT, "reverseroutequery"),
    (VT, "deletemonster"),
    (VT, "equipitemsfor"),
    (VT, "testitem"),
    (VT, "testmonster"),
    (VT, "testspell"),
    (VT, "testpet"),
    (VT, "listmonstervariables"),
    (VT, "dumpmetavars"),
    (VT, "listmetafunctions"),
    (VT, "metafunchelp"),
    (VT, "deathrestore"),
    (VT, "fakedeath"),
    (VT, "pscount"),
    (VT, "refresh"),
    (VT, "log"),
    (VT, "lockdump"),
    (VT, "dumptracker"),
    (VT, "clearlocks"),
    (VT, "clearbusy"),
    (VT, "fakeimp"),
    (VT, "dumpspells"),
    (VT, "dumpspecies"),
    (VT, "dumpmats"),
    (VT, "getdb"),
    (VT, "gamedb"),
    (VT, "nextwp"),
    (VT, "prevwp"),
    (VT, "metainterval"),
    (VT, "ub"),
    (UB, "count item taper"),
    (UB, "login list"),
    (UB, "autovendor"),
    (UB, "vendor buyall"),
    (UB, "xp test"),
    (UB, "equip list"),
    (UB, "simplejump"),
    (UB, "calcdamage"),
    (UB, "pos"),
    (UB, "id"),
    (UB, "vitae"),
    (UB, "combatstate peace"),
    (UB, "date"),
    (UB, "dateutc"),
    (UB, "delay 100 /say hi"),
    (UB, "bc /say hi"),
    (UB, "bct tag /say hi"),
    (UB, "netclients"),
    (UB, "closestportal"),
    (UB, "close corpse"),
    (UB, "printcolors"),
    (UB, "autotinker"),
    (UB, "getjob"),
    (UB, "tinkcalc"),
    (UB, "listvars"),
    (UB, "listpvars"),
    (UB, "listgvars"),
    (UB, "myquests"),
    (UB, "quit"),
    (UB, "autostack"),
    (UB, "autocram"),
    (UB, "clearbugged"),
    (UB, "playeroption list"),
    (UB, "translateroute"),
    (UB, "face 90"),
    (UB, "setmotion forward 0"),
    (UB, "clearmotion"),
    (UB, "prepclick stop"),
    (UB, "fellow status"),
    (UB, "jumpsw 90 100"),
    (UB, "ig muled to Bob"),
    (UB, "igp muled to Bob"),
    (UB, "portal Town"),
    (UB, "portalp Town"),
    (UB, "follow Bob"),
    (UB, "followp Bob"),
    (UB, "use Taper"),
    (UB, "usel Taper"),
    (UB, "select Taper"),
    (UB, "swearallegiance Bob"),
    (UB, "breakallegiance Bob"),
    (UB, "give Taper to Bob"),
    (UB, "givep Taper to Bob"),
    (UB, "giveP Taper to Bob"),
    (UB, "giver Taper to Bob"),
];

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.CommandNamespaces.cs :: EachCommandAnswersOnItsOwnWordOnly
#[test]
fn each_command_answers_on_its_own_word_only() {
    for (word, line) in CASES {
        let (mut own, mut own_plugin) = namespace_world();
        let (mut other, mut other_plugin) = namespace_world();
        let other_word = if *word == VT { UB } else { VT };

        own.command(&mut own_plugin, word, line);
        other.command(&mut other_plugin, other_word, line);

        let own_chat = own.chat();
        assert!(
            !own_chat.iter().any(|line| is_unknown_reply(line)),
            "/{word} {line}: {own_chat:?}"
        );
        assert!(
            !own_chat
                .iter()
                .any(|line| line.starts_with("Command failed")),
            "/{word} {line}: {own_chat:?}"
        );
        let other_chat = other.chat();
        assert!(!other_chat.is_empty(), "/{other_word} {line}");
        assert_eq!(
            other_chat[0],
            unknown_reply(other_word),
            "/{other_word} {line}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.CommandNamespaces.cs :: TheCommandsBothPluginsHaveAnswerOnBothWords
#[test]
fn the_commands_both_plugins_have_answer_on_both_words() {
    for line in [
        "help",
        "opt list",
        "mexec 1",
        "propertydump",
        "dumpskills",
        "jump 90 true 100",
        "jump 100",
        "",
    ] {
        for word in [VT, UB] {
            let (mut world, mut plugin) = namespace_world();
            world.command(&mut plugin, word, line);
            let chat = world.chat();
            assert!(
                !chat.iter().any(|line| is_unknown_reply(line)),
                "/{word} {line}: {chat:?}"
            );
        }
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.CommandNamespaces.cs :: EachWordAnswersAnUnknownWordInItsOwnWords
#[test]
fn each_word_answers_an_unknown_word_in_its_own_words() {
    let (mut vt, mut vt_plugin) = namespace_world();
    let (mut ub, mut ub_plugin) = namespace_world();

    vt.command(&mut vt_plugin, VT, "autostak");
    ub.command(&mut ub_plugin, UB, "autostak");

    assert_eq!(vt.chat(), [UNKNOWN_VT]);
    assert_eq!(
        ub.chat(),
        [
            UNKNOWN_UB,
            "[UB] Error: Did you mean one of these? autostack",
        ]
    );
    assert!(
        ub.fake
            .posted
            .iter()
            .all(|post| post.log_text_type == UB_ERROR_CHAT_TYPE)
    );

    ub.clear_chat();
    ub.command(&mut ub_plugin, UB, "zzzzzzzz");
    assert_eq!(ub.chat(), [UNKNOWN_UB]);

    ub.command(
        &mut ub_plugin,
        UB,
        "opt set Plugin.ErrorMessageDisplay.Enabled false",
    );
    ub.clear_chat();
    ub.command(&mut ub_plugin, UB, "zzzzzzzz");
    assert!(ub.chat().is_empty(), "{:?}", ub.chat());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.CommandNamespaces.cs :: TheEditCountMatchesTheReference
#[test]
fn the_edit_count_matches_the_reference() {
    for (one, two, distance) in [
        ("fcae", "face", 1),
        ("autostak", "autostack", 1),
        ("face", "face", 0),
        ("bc", "fac", 2),
    ] {
        assert_eq!(
            mosstank_ub::help::edit_distance(one, two),
            distance,
            "{one} {two}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.CommandNamespaces.cs :: EachHelpListsOnlyItsOwnCommands
#[test]
fn each_help_lists_only_its_own_commands() {
    let (mut vt, mut vt_plugin) = namespace_world();
    let (mut ub, mut ub_plugin) = namespace_world();

    vt.command(&mut vt_plugin, VT, "help");
    ub.command(&mut ub_plugin, UB, "help");

    let ub_chat = ub.chat();
    let ub_list = ub_chat[0].clone();
    assert!(ub_list.starts_with("[UB] All available UB commands: /ub {"));
    assert_eq!(
        ub_chat[1],
        "For help with a specific command, use `/ub help [command]`"
    );
    let vt_chat = vt.chat();
    let has_word = |lines: &[String], word: &str| {
        lines.iter().any(|line| {
            line.split(|c: char| !c.is_alphanumeric())
                .any(|part| part == word)
        })
    };
    for ub_only in ["autostack", "fellow", "give", "setmotion", "vitae"] {
        assert!(ub_list.contains(ub_only), "{ub_only}");
        assert!(!has_word(&vt_chat, ub_only), "{ub_only}");
    }
    for vt_only in ["setmetastate", "gamedb", "nextwp", "forcebuff"] {
        assert!(!ub_list.contains(vt_only), "{vt_only}");
        assert!(has_word(&vt_chat, vt_only), "{vt_only}");
    }

    ub.clear_chat();
    ub.command(&mut ub_plugin, UB, "help setmetastate");
    assert_eq!(ub.chat()[0], ub_list);

    ub.clear_chat();
    ub.command(&mut ub_plugin, UB, "help face");
    assert_eq!(
        ub.chat(),
        [
            "[UB] Usage: /ub face <heading>",
            "Description: Face heading commands with built in navigation pausing and retries.",
            "Examples:",
            " /ub face 180",
            "  Faces your character towards 180 degrees (south).",
        ]
    );

    vt.clear_chat();
    vt.command(&mut vt_plugin, VT, "help face");
    assert_eq!(vt.chat(), ["[MossTank] No help found for command: face"]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.CommandNamespaces.cs :: EachOptReachesOnlyItsOwnGroup
#[test]
fn each_opt_reaches_only_its_own_group() {
    let (mut world, mut plugin) = namespace_world();
    let combat = plugin.tank().unwrap().option("EnableCombat").is_truthy();

    world.command(&mut plugin, VT, "opt set DungeonMaps.Enabled false");
    assert_eq!(
        world.chat(),
        ["[MossTank] Option set: Invalid option specified."]
    );
    world.clear_chat();
    world.command(&mut plugin, UB, "mexec uboptget[`DungeonMaps.Enabled`]");
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("Result: [number] 1")),
        "{:?}",
        world.chat()
    );

    world.clear_chat();
    world.command(&mut plugin, VT, "opt get DungeonMaps.Enabled");
    assert_eq!(
        world.chat(),
        ["[MossTank] Option get: Invalid option specified."]
    );

    world.clear_chat();
    world.command(&mut plugin, VT, "opt toggle DungeonMaps.Enabled");
    assert_eq!(
        world.chat(),
        ["[MossTank] Option toggle: Invalid option specified."]
    );

    world.clear_chat();
    world.command(
        &mut plugin,
        UB,
        &format!(
            "opt set EnableCombat {}",
            if combat { "False" } else { "True" }
        ),
    );
    assert_eq!(world.chat()[0], "[UB] Error: Invalid option: enablecombat");
    assert_eq!(
        plugin.tank().unwrap().option("EnableCombat").is_truthy(),
        combat
    );

    world.clear_chat();
    world.command(&mut plugin, UB, "opt toggle EnableCombat");
    assert_eq!(world.chat()[0], "[UB] Error: Invalid option: enablecombat");

    world.clear_chat();
    world.command(&mut plugin, UB, "opt set DungeonMaps.Enabled false");
    assert_eq!(world.chat(), ["[UB] DungeonMaps.Enabled (Profile) = False"]);

    world.clear_chat();
    world.command(&mut plugin, UB, "opt frobnicate");
    assert_eq!(
        world.chat()[..2],
        [
            "[UB] Error: Bad command syntax".to_owned(),
            "[UB] Usage: /ub opt {list | get <option> | set <option> <newValue> | toggle <option>}"
                .to_owned(),
        ]
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.CommandNamespaces.cs :: EachMexecAnswersInItsOwnWords
#[test]
fn each_mexec_answers_in_its_own_words() {
    let (mut vt, mut vt_plugin) = namespace_world();
    let (mut ub, mut ub_plugin) = namespace_world();

    vt.command(&mut vt_plugin, VT, "mexec 1+2");
    ub.command(&mut ub_plugin, UB, "mexec 1+2");
    ub.command(&mut ub_plugin, UB, "mexec 1==1");
    ub.command(&mut ub_plugin, UB, "mexec `a`");

    assert_eq!(
        vt.chat(),
        [
            "[MossTank] MExec evaluating expression: \"1+2\"",
            "[MossTank] Result: 3",
        ]
    );
    let chat = ub.chat();
    assert_eq!(chat[0], "[UB] Evaluating expression: \"1+2\"");
    let result = |line: &str, kind: &str, value: &str| {
        let prefix = format!("[UB] Result: [{kind}] {value} (");
        line.starts_with(&prefix) && line.ends_with("ms)")
    };
    assert!(result(&chat[1], "number", "3"), "{}", chat[1]);
    assert!(result(&chat[3], "number", "1"), "{}", chat[3]);
    assert!(result(&chat[5], "string", "a"), "{}", chat[5]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.CommandNamespaces.cs :: MetaChatLinesReachTheWordTheyName
#[test]
fn meta_chat_lines_reach_the_word_they_name() {
    let (mut world, mut plugin) = namespace_world();
    // Standing in the world, facing north (`NavigationAt(0f)`).
    world.set_reply::<calls::NavigationSnapshot>(&ac_plugin_abi::navigation::NavigationSnapshot {
        is_available: true,
        local_object_id: 0x5000_0001,
        ..Default::default()
    });

    world.command(&mut plugin, UB, "face 90");
    world.command(&mut plugin, VT, "face 90");
    world.command(&mut plugin, VT, "reverseroutequery");
    world.tick(&mut plugin);

    let faced = world.requests::<calls::NavigationFace>();
    assert_eq!(faced.last().copied(), Some(90.0));
    let lines: Vec<String> = world
        .chat()
        .into_iter()
        .filter(|line| line == UNKNOWN_VT || line.starts_with("[MossTank] Nav "))
        .collect();
    assert_eq!(lines, [UNKNOWN_VT, "[MossTank] Nav backwards is: False"]);
}

//! The three commands that reach the other clients on this computer, and
//! the labels this one answers to, against `MossTankPanelTests.UbNetwork`.
//! The C# puts labels on the character through the settings page; here
//! through the same row by `/ub opt`, which the page tests cover on their
//! own.

mod common;

use ac_plugin_abi::peers::NetworkClient;
use ac_plugin_abi::world::Position;
use common::Rig;
use common::page::PageRig;
use mosstank_ub::catalog::UbCatalog;
use mosstank_ub::network::TagPublisher;
use mosstank_ub::testing::{FakeUbHost, SharedStorage};

fn net_client(client_id: u32, name: &str, tags: &[&str]) -> NetworkClient {
    NetworkClient {
        client_id,
        player_id: 0x5000_0000 + client_id,
        name: name.to_owned(),
        world_name: "Coldeve".to_owned(),
        position: Position {
            cell_id: 0xA9B4_0001,
            is_outdoor: true,
            ..Position::default()
        },
        tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
        current_health: 100,
        current_mana: 100,
        current_stamina: 100,
        max_health: 100,
        max_mana: 100,
        max_stamina: 100,
        heading: 0.0,
        is_remote: false,
    }
}

/// `NetworkPanel`: Acdream of Coldeve, object 0x50000001.
fn network_rig() -> Rig {
    let mut rig = Rig::new();
    let me = rig.host.meta.character_mut();
    "Acdream".clone_into(&mut me.name);
    "Coldeve".clone_into(&mut me.world);
    me.id = 0x5000_0001;
    rig
}

fn set_own_tags(rig: &mut Rig, tags: &[&str]) {
    for tag in tags {
        rig.ub(&format!("opt set Networking.Tags add {tag}"));
    }
    rig.clear_messages();
}

// ---- /ub bc ---------------------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbNetwork.cs :: ABroadcastReadsItsDelayOffTheFrontOfTheLine
#[test]
fn a_broadcast_reads_its_delay_off_the_front_of_the_line() {
    for (typed, expected, delay) in [
        ("bc 5000 /say hello", "/say hello", 5000),
        ("bc /say hello", "/say hello", 0),
        ("bc 0 /vt start", "/vt start", 0),
    ] {
        let mut rig = network_rig();
        rig.ub(typed);
        assert_eq!(
            vec![(expected.to_owned(), Vec::<String>::new(), delay)],
            rig.host.broadcasts,
            "{typed}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbNetwork.cs :: ABroadcastNamesEveryClientItReaches
#[test]
fn a_broadcast_names_every_client_it_reaches() {
    let mut rig = network_rig();
    rig.host.meta.peers = vec![net_client(2, "Horan", &[]), net_client(3, "Yonneh", &[])];
    rig.ub("bc 250 /say hello");
    assert_eq!(
        vec![
            "[UB] Broadcasting command to all clients: \"/say hello\" with delay inbetween of 250ms",
            "[UB] Sending Horan: \"/say hello\" with delay inbetween of 250ms",
            "[UB] Sending Yonneh: \"/say hello\" with delay inbetween of 250ms",
        ],
        rig.messages()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbNetwork.cs :: TheSenderRunsItsOwnBroadcastLocally
#[test]
fn the_sender_runs_its_own_broadcast_locally() {
    let mut rig = network_rig();
    rig.ub("bc 5000 /say hello");
    assert!(!rig.submitted().contains(&"/say hello".to_owned()));
    rig.tick(0.05);
    assert!(rig.submitted().contains(&"/say hello".to_owned()));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbNetwork.cs :: ADelayTooLargeToHoldIsRefused
#[test]
fn a_delay_too_large_to_hold_is_refused() {
    let mut rig = network_rig();
    rig.ub("bc 99999999999 /say hello");
    assert_eq!(
        "[UB] Error: Unable to broadcast command, invalid delay: 99999999999",
        rig.single_message()
    );
    assert!(rig.host.broadcasts.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbNetwork.cs :: ABroadcastWithoutACommandPrintsItsUsage
#[test]
fn a_broadcast_without_a_command_prints_its_usage() {
    let mut rig = network_rig();
    rig.ub("bc 500");
    assert_eq!(
        vec![
            "[UB] Error: Bad command syntax",
            "[UB] Usage: /ub bc [millisecondDelay] <command>",
        ],
        rig.messages()[..2]
    );
    assert!(rig.host.broadcasts.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbNetwork.cs :: ABroadcastTheClientRefusesSaysSo
#[test]
fn a_broadcast_the_client_refuses_says_so() {
    let mut rig = network_rig();
    rig.host.broadcast_result = false;
    rig.host.meta.peers = vec![net_client(2, "Horan", &[])];
    rig.ub("bc /say hello");
    let messages = rig.messages();
    assert_eq!(
        "[UB] Error: Unable to broadcast command to the other clients.",
        messages.last().unwrap()
    );
    assert!(
        !messages
            .iter()
            .any(|line| line.starts_with("[UB] Sending "))
    );
}

// ---- /ub bct --------------------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbNetwork.cs :: ATaggedBroadcastReadsItsTagsThenItsDelay
#[test]
fn a_tagged_broadcast_reads_its_tags_then_its_delay() {
    for (typed, line, delay, tags) in [
        (
            "bct one,two 5000 /say hello",
            "/say hello",
            5000,
            vec!["one", "two"],
        ),
        ("bct three /say hello", "/say hello", 0, vec!["three"]),
        (
            "bct \"some tag\",\"another tag\" /say hello",
            "/say hello",
            0,
            vec!["some tag", "another tag"],
        ),
    ] {
        let mut rig = network_rig();
        rig.ub(typed);
        let tags: Vec<String> = tags.into_iter().map(str::to_owned).collect();
        assert_eq!(
            vec![(line.to_owned(), tags, delay)],
            rig.host.broadcasts,
            "{typed}"
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbNetwork.cs :: ATaggedBroadcastNamesTheTagsItIsAimedAt
#[test]
fn a_tagged_broadcast_names_the_tags_it_is_aimed_at() {
    let mut rig = network_rig();
    rig.ub("bct one,two 250 /say hello");
    assert_eq!(
        "[UB] Broadcasting command to clients with tags (one,two): \"/say hello\" with delay inbetween of 250ms",
        rig.single_message()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbNetwork.cs :: ATaggedBroadcastRunsLocallyOnlyWhenOneOfTheTagsIsOurs
#[test]
fn a_tagged_broadcast_runs_locally_only_when_one_of_the_tags_is_ours() {
    let mut rig = network_rig();
    set_own_tags(&mut rig, &["tank"]);

    rig.ub("bct healer /say hello");
    rig.tick(0.05);
    assert!(!rig.submitted().contains(&"/say hello".to_owned()));

    rig.ub("bct healer,TANK /say hello");
    rig.tick(0.05);
    assert!(rig.submitted().contains(&"/say hello".to_owned()));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbNetwork.cs :: ATaggedBroadcastWithoutATagSaysSo
#[test]
fn a_tagged_broadcast_without_a_tag_says_so() {
    let mut rig = network_rig();
    rig.ub("bct , /say hello");
    assert_eq!(
        "[UB] Error: You must specify at least one tag to send the command to.",
        rig.single_message()
    );
    assert!(rig.host.broadcasts.is_empty());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbNetwork.cs :: ATaggedBroadcastWithNothingAfterTheTagsPrintsItsUsage
#[test]
fn a_tagged_broadcast_with_nothing_after_the_tags_prints_its_usage() {
    let mut rig = network_rig();
    rig.ub("bct one");
    assert_eq!(
        vec![
            "[UB] Error: Bad command syntax",
            "[UB] Usage: /ub bct <tags> [millisecondDelay] <command>",
        ],
        rig.messages()[..2]
    );
    assert!(rig.host.broadcasts.is_empty());
}

// ---- /ub netclients ---------------------------------------------------------------

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbNetwork.cs :: TheClientListCarriesThisCharacterAndTheOthers
#[test]
fn the_client_list_carries_this_character_and_the_others() {
    let mut rig = network_rig();
    rig.host.meta.peers = vec![
        net_client(3, "Yonneh", &["healer"]),
        net_client(2, "Horan", &[]),
    ];
    rig.ub("netclients");
    assert_eq!(
        vec![
            "[UB] ClientData<0, Acdream>",
            "[UB] ClientData<2, Horan>",
            "[UB] ClientData<3, Yonneh> [healer]",
        ],
        rig.messages()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbNetwork.cs :: AClientListWithATagListsOnlyThatTag
#[test]
fn a_client_list_with_a_tag_lists_only_that_tag() {
    let mut rig = network_rig();
    rig.host.meta.peers = vec![
        net_client(2, "Horan", &["tank"]),
        net_client(3, "Yonneh", &["healer"]),
    ];
    set_own_tags(&mut rig, &["healer"]);
    rig.ub("netclients healer");
    assert_eq!(
        vec![
            "[UB] ClientData<0, Acdream> [healer]",
            "[UB] ClientData<3, Yonneh> [healer]",
        ],
        rig.messages()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbNetwork.cs :: AClientListWithNothingToShowSaysSo
#[test]
fn a_client_list_with_nothing_to_show_says_so() {
    let mut rig = network_rig();
    rig.ub("netclients raid");
    assert_eq!("[UB] No net clients to show", rig.single_message());
}

// ---- Networking.Tags ------------------------------------------------------------------

fn publish(publisher: &mut TagPublisher, host: &mut FakeUbHost, rig: &PageRig, on_load: bool) {
    let catalog: &dyn UbCatalog = &rig.catalog;
    publisher.publish(host, catalog.value("Networking.Tags").items, on_load);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbNetwork.cs :: TheLabelsRowIsPushedToTheClientWhenItChanges
#[test]
fn the_labels_row_is_pushed_to_the_client_when_it_changes() {
    let mut rig = PageRig::new(&SharedStorage::new());
    let mut host = FakeUbHost::new();
    let mut publisher = TagPublisher::new();
    // The page is built, then a frame.
    publish(&mut publisher, &mut host, &rig, true);
    publish(&mut publisher, &mut host, &rig, false);
    assert!(host.peer_tag_pushes.is_empty());

    rig.filter("Networking.Tags");
    rig.click(0);
    rig.add_list_entry("tank");
    rig.page.hide_list_editor();
    publish(&mut publisher, &mut host, &rig, false);
    publish(&mut publisher, &mut host, &rig, false);

    assert_eq!(vec![vec!["tank".to_owned()]], host.peer_tag_pushes);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.UbNetwork.cs :: ALabelsRowSavedEarlierIsPushedAsThePageIsBuilt
#[test]
fn a_labels_row_saved_earlier_is_pushed_as_the_page_is_built() {
    let storage = SharedStorage::new();
    let mut first = PageRig::new(&storage);
    first.filter("Networking.Tags");
    first.click(0);
    first.add_list_entry("tank");

    let next = PageRig::new(&storage);
    let mut host = FakeUbHost::new();
    let mut publisher = TagPublisher::new();
    publish(&mut publisher, &mut host, &next, true);
    assert_eq!(vec![vec!["tank".to_owned()]], host.peer_tag_pushes);
}

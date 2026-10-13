//! The UtilityBelt line shapes, against `UbChatTests`.

use mosstank_ub::chat::{self, DefaultDisplays, tools};
use mosstank_ub::testing::FakeHost;

// Source: tests/AcDream.Plugins.MossTank.Tests/UbChatTests.cs :: EachShapeIsTheReferencesOwn
#[test]
fn each_shape_is_the_references_own() {
    assert_eq!("[UB] Quitting Client", chat::line("Quitting Client"));
    assert_eq!(
        "[UB] Error: Could not find player Bob",
        chat::error("Could not find player Bob")
    );
    assert_eq!(
        "[UB] PrepClick: Will click yes on the next dialog to appear within 5 seconds",
        chat::tool(
            tools::PREP_CLICK,
            "Will click yes on the next dialog to appear within 5 seconds"
        )
    );
    assert_eq!(
        "[UB] Error: Jumper: You are already jumping. try again later.",
        chat::tool_error(tools::JUMPER, "You are already jumping. try again later.")
    );
    // The logger leaves a line that already carries the tag alone.
    assert_eq!("[UB] Already tagged", chat::line("[UB] Already tagged"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/UbChatTests.cs :: EachKindIsPrintedInTheReferencesTextClass
#[test]
fn each_kind_is_printed_in_the_references_text_class() {
    let mut host = FakeHost::new();
    let displays = DefaultDisplays;

    chat::post(&mut host, &displays, &chat::line("Quitting Client"));
    chat::post(
        &mut host,
        &displays,
        &chat::tool_error(tools::JUMPER, "You are already jumping. try again later."),
    );
    chat::post(&mut host, &displays, "Untagged");
    chat::post_debug(
        &mut host,
        &displays,
        true,
        "Scheduling command `x` with delay of 5ms",
    );
    chat::post_tool_debug(
        &mut host,
        &displays,
        true,
        tools::PLUGIN,
        "Attempting to use portal Gate",
    );
    chat::post_debug(&mut host, &displays, false, "never printed");

    let expected: Vec<(String, u32)> = [
        ("[UB] Quitting Client", 5),
        (
            "[UB] Error: Jumper: You are already jumping. try again later.",
            15,
        ),
        ("[UB] Untagged", 5),
        ("[UB] Scheduling command `x` with delay of 5ms", 14),
        ("[UB] Plugin: Attempting to use portal Gate", 14),
    ]
    .into_iter()
    .map(|(text, kind)| (text.to_owned(), kind))
    .collect();
    assert_eq!(expected, host.posted);
}

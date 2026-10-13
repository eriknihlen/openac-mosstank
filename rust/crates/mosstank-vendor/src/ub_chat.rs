//! The shape of the lines the UtilityBelt side prints (`UbChat`), in the
//! reference's exact words: "[UB] " before the text (unless it already
//! begins with the tag), "Error: " before an error, and a tool's name and a
//! colon before a line a tool prints for itself. Macros wait on these
//! lines with chat triggers, so the tag, the colon and the space after it
//! are part of what a line means.
//!
//! Each kind of line prints in its own text class: an ordinary line in
//! System, an error in Help. The player's display settings for each kind
//! (`Plugin.*MessageDisplay`) belong to the UB tab; until it binds them,
//! every line prints in its kind's default class, as the C#'s unbound chat
//! does.

use mosstank_nav::NavigationAutomation;

/// The plugin's tag, with the space that follows it.
pub const TAG: &str = "[UB] ";

/// The text class of an ordinary line (System).
pub const GENERIC_CHAT_TYPE: u32 = 0x05;

/// The text class of a debug line (Abuse).
pub const DEBUG_CHAT_TYPE: u32 = 0x0E;

/// The text class of an error line (Help).
pub const ERROR_CHAT_TYPE: u32 = 0x0F;

const ERROR_LEAD: &str = "[UB] Error: ";

/// The tools' names, as their lines carry them.
pub mod tools {
    /// The vendor tool.
    pub const AUTO_VENDOR: &str = "AutoVendor";
}

/// A plain line: the tag, then the text.
#[must_use]
pub fn line(text: &str) -> String {
    if text.starts_with("[UB]") {
        text.to_owned()
    } else {
        format!("{TAG}{text}")
    }
}

/// An error line: the tag, "Error: ", then the text.
#[must_use]
pub fn error(text: &str) -> String {
    line(&format!("Error: {text}"))
}

/// A line a tool prints for itself: the tag, the tool, the text.
#[must_use]
pub fn tool(tool: &str, text: &str) -> String {
    line(&format!("{tool}: {text}"))
}

/// An error a tool reports for itself.
#[must_use]
pub fn tool_error(tool: &str, text: &str) -> String {
    error(&format!("{tool}: {text}"))
}

/// The class a finished line prints in: an error line or a generic one,
/// by its lead.
#[must_use]
pub fn chat_type_of(tagged: &str) -> u32 {
    if tagged.starts_with(ERROR_LEAD) {
        ERROR_CHAT_TYPE
    } else {
        GENERIC_CHAT_TYPE
    }
}

/// Where a UB line is printed: the chat window, in a text class.
pub trait UbChatSink {
    /// Prints `text` in text class `chat_type`.
    fn post_message(&mut self, text: &str, chat_type: u32);
}

/// Prints a finished line through its kind's display (`UbChat.Post`).
pub fn post(chat: &mut dyn UbChatSink, text: &str) {
    let tagged = line(text);
    let chat_type = chat_type_of(&tagged);
    chat.post_message(&tagged, chat_type);
}

/// Says something to yourself the way the reference does: a tell to the
/// character's own name, handed to the chat bar, so what reaches the chat
/// window (and every chat trigger watching it) is the server's own "You
/// think" line. False when the client took no line.
pub fn think(automation: &mut dyn NavigationAutomation, text: &str) -> bool {
    let name = automation.character().name;
    !name.is_empty() && automation.submit_chat(&format!("/t {name}, {text}"))
}

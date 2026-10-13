//! The shape of every line the UtilityBelt side prints, in the reference's
//! exact words (`UbChat`, `UbChatMessageTypes`): its logger puts `[UB] `
//! before the text (unless the text already begins with the tag), an error
//! is `Error: ` then the text, and a line a tool prints for itself carries
//! the tool's name and a colon first.
//!
//! A tool's name is the name its settings are stored under: the prepclick
//! tool prints `[UB] PrepClick: ...`. Macros wait on these lines with chat
//! triggers, so the tag, the colon and the space after it are part of what
//! the line means.
//!
//! Each kind of line has its own display (whether it is shown, and the text
//! class it is printed in): the reference's `Plugin.GenericMessageDisplay`,
//! `DebugMessageDisplay`, `ExpressionMessageDisplay` and
//! `ErrorMessageDisplay`. An ordinary line is System, an error Help, a debug
//! line Abuse by default. The display comes in as a [`UbDisplays`]: the
//! settings catalogue ([`crate::catalog::UbCatalog`]) or the shipped
//! defaults ([`DefaultDisplays`]). The shapes themselves are
//! `mosstank-vendor`'s; this module adds the displays, the several-line
//! messages, the debug lines and the thinks the UB commands print.

use mosstank_meta::host::{MetaHost, UbDisplay, UbLineKind};

pub use mosstank_meta::chat::{
    UB_DEBUG_CHAT_TYPE as DEBUG_CHAT_TYPE, UB_ERROR_CHAT_TYPE as ERROR_CHAT_TYPE,
    UB_GENERIC_CHAT_TYPE as GENERIC_CHAT_TYPE, UB_TAG as TAG,
    ub_default_display as default_display,
};

/// The setting that shows the debug lines.
pub const DEBUG_SETTING: &str = "Plugin.Debug";

const ERROR_LEAD: &str = "[UB] Error: ";

/// Where a kind of line's display comes from.
pub trait UbDisplays {
    /// Whether a kind's lines are shown, and the class they print in.
    fn display(&self, kind: UbLineKind) -> UbDisplay;
}

/// The displays as the reference ships them (a chat with no displays bound).
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultDisplays;

impl UbDisplays for DefaultDisplays {
    fn display(&self, kind: UbLineKind) -> UbDisplay {
        default_display(kind)
    }
}

/// The name a kind's display settings are held under, e.g.
/// `Plugin.ErrorMessageDisplay`.
#[must_use]
pub fn display_setting(kind: UbLineKind) -> String {
    let name = match kind {
        UbLineKind::Generic => "Generic",
        UbLineKind::Debug => "Debug",
        UbLineKind::Expression => "Expression",
        UbLineKind::Error => "Error",
    };
    format!("Plugin.{name}MessageDisplay")
}

// The line shapes are the vendor tools' own (`mosstank-vendor::ub_chat`),
// so every UB line has one spelling.
pub use mosstank_vendor::ub_chat::{error, line, tool, tool_error};

/// Prints a finished line through its kind's display: an error line or a
/// generic one, by its lead. Only the logger's error path starts a line
/// with the error lead, so the lead is the kind.
pub fn post(host: &mut dyn MetaHost, displays: &dyn UbDisplays, text: &str) {
    let tagged = line(text);
    let kind = if tagged.starts_with(ERROR_LEAD) {
        UbLineKind::Error
    } else {
        UbLineKind::Generic
    };
    post_as(host, displays, kind, &tagged);
}

/// Prints an expression line (what `/ub mexec` says) through the
/// expression display.
pub fn post_expression(host: &mut dyn MetaHost, displays: &dyn UbDisplays, text: &str) {
    post_as(host, displays, UbLineKind::Expression, &line(text));
}

/// Prints one message of several lines, as the reference's logger prints a
/// message with line breaks in it: the tag on the first line only, and
/// every line in the kind's class and behind the kind's switch, read once
/// for the whole message. An error message's first line carries the error
/// lead; a blank line is not printed.
pub fn post_message<I, S>(
    host: &mut dyn MetaHost,
    displays: &dyn UbDisplays,
    kind: UbLineKind,
    heading: &str,
    continuation: I,
) where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let display = displays.display(kind);
    if !display.show {
        return;
    }
    let first = if kind == UbLineKind::Error {
        error(heading)
    } else {
        line(heading)
    };
    host.chat_post(&first, display.chat_type);
    for text in continuation {
        let text = text.as_ref();
        if !text.is_empty() {
            host.chat_post(text, display.chat_type);
        }
    }
}

/// Says something to yourself the way the reference does: a real tell to
/// the character's own name, handed to the chat bar, so what reaches the
/// chat window is the server's own "You think" line. False when the client
/// took no line; nothing is printed in its place.
pub fn think(host: &mut dyn MetaHost, text: &str) -> bool {
    let name = host.character().map(|info| info.name).unwrap_or_default();
    !name.is_empty() && host.chat_submit(&format!("/t {name}, {text}"))
}

/// A line a tool either thinks or prints, as its think switch says.
pub fn think_or_write(host: &mut dyn MetaHost, displays: &dyn UbDisplays, text: &str, think: bool) {
    if think {
        self::think(host, text);
    } else {
        post(host, displays, &line(text));
    }
}

fn post_as(host: &mut dyn MetaHost, displays: &dyn UbDisplays, kind: UbLineKind, tagged: &str) {
    let display = displays.display(kind);
    if display.show {
        host.chat_post(tagged, display.chat_type);
    }
}

/// A debug line: printed, tagged and in the debug class, only while the
/// debug setting is on.
pub fn post_debug(host: &mut dyn MetaHost, displays: &dyn UbDisplays, debug: bool, text: &str) {
    if debug {
        post_as(host, displays, UbLineKind::Debug, &line(text));
    }
}

/// A tool's debug line: the tool's name and a colon first.
pub fn post_tool_debug(
    host: &mut dyn MetaHost,
    displays: &dyn UbDisplays,
    debug: bool,
    tool: &str,
    text: &str,
) {
    post_debug(host, displays, debug, &format!("{tool}: {text}"));
}

/// The tools' names, as their lines carry them: each is the name the tool's
/// settings are held under.
pub mod tools {
    /// The assessor.
    pub const ASSESSOR: &str = "Assessor";
    /// The tinkering run.
    pub const AUTO_TINKER: &str = "AutoTinker";
    /// The vendor run.
    pub const AUTO_VENDOR: &str = "AutoVendor";
    /// The counter.
    pub const COUNTER: &str = "Counter";
    /// Dressing by profile.
    pub const EQUIPMENT_MANAGER: &str = "EquipmentManager";
    /// The give runs.
    pub const INVENTORY_MANAGER: &str = "InventoryManager";
    /// The jumper.
    pub const JUMPER: &str = "Jumper";
    /// The plugin itself.
    pub const PLUGIN: &str = "Plugin";
    /// The dialog clicker.
    pub const PREP_CLICK: &str = "PrepClick";
    /// The quest tracker.
    pub const QUEST_TRACKER: &str = "QuestTracker";
    /// The route tools.
    pub const VTANK: &str = "VTank";
}

/// Every text class `/ub printcolors` walks, by the name a macro author
/// knows it by, in ascending order of the number the client keys them by.
pub use mosstank_settings::ub::definitions::CHAT_MESSAGE_TYPES as MESSAGE_TYPES;

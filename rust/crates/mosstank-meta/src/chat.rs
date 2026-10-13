//! The chat lines the meta reads and writes: the whole line a chat
//! condition matches (the reference chat window's wording), MossTank's own
//! tagged lines, and UtilityBelt's.

use ac_plugin_abi::ChatLine;
use ac_plugin_abi::LogLevel;

use crate::host::{MetaHost, UbDisplay, UbLineKind};

// The host's line kinds, as it numbers them on `ChatLine::kind`.
const LOCAL_SPEECH_KIND: u32 = 0;
const RANGED_SPEECH_KIND: u32 = 1;
const CHANNEL_KIND: u32 = 2;
const TELL_KIND: u32 = 3;
const EMOTE_KIND: u32 = 6;
const SOUL_EMOTE_KIND: u32 = 7;

/// The client's text type of the fellowship channel.
const FELLOWSHIP_LOG_TEXT_TYPE: u32 = 0x13;

// A speaker whose object id falls in this range is a player: the game
// client links a player's name so it can be clicked to answer, and leaves
// a creature's name plain.
const FIRST_PLAYER_OBJECT_ID: u32 = 0x5000_0001;
const LAST_PLAYER_OBJECT_ID: u32 = 0x6FFF_FFFF;

/// The line as the game's chat window prints it, without its closing line
/// break: the host's own wording when it gives one, otherwise rebuilt from
/// the parts the way the game client words each kind of line.
#[must_use]
pub fn compose_line(message: &ChatLine, own_object_id: u32, own_name: &str) -> String {
    if !message.display_text.is_empty() {
        return message.display_text.clone();
    }
    let text = message.text.as_str();
    let sender = message.sender.as_str();
    match message.kind {
        LOCAL_SPEECH_KIND => {
            if is_own(message, own_object_id) {
                format!("You say, \"{text}\"")
            } else {
                format!("{} says, \"{text}\"", speaker(sender, message.sender_id))
            }
        }
        // Shouted speech has no sentence of its own for the speaker: the game
        // client prints a shout the character made under its own name.
        RANGED_SPEECH_KIND => {
            let name = if is_own(message, own_object_id) {
                own_name
            } else {
                sender
            };
            format!("{} says, \"{text}\"", speaker(name, message.sender_id))
        }
        CHANNEL_KIND => channel_line(message, sender, text),
        TELL_KIND => tell_line(message, sender, text, own_object_id),
        // Someone else's emote is their name and the words; the character's
        // own pose line arrives as its own sentence.
        EMOTE_KIND | SOUL_EMOTE_KIND if !is_own_speaker(sender) => format!("{sender} {text}"),
        _ => text.to_owned(),
    }
}

fn channel_line(message: &ChatLine, sender: &str, text: &str) -> String {
    let own = is_own_speaker(sender);
    let mut channel = message.channel.clone();
    if channel.is_empty() && message.log_text_type == FELLOWSHIP_LOG_TEXT_TYPE {
        "Fellowship".clone_into(&mut channel);
    }
    if channel.is_empty() {
        // A numbered channel other than the fellowship is worded by its
        // number, which a host that does not word its lines does not give:
        // the words are the most that can be matched.
        return text.to_owned();
    }
    // A channel speaker is linked with id 0: the channel message carries
    // only the name.
    if own {
        format!("[{channel}] You say, \"{text}\"")
    } else {
        format!("[{channel}] {} says, \"{text}\"", link(0, sender))
    }
}

fn tell_line(message: &ChatLine, sender: &str, text: &str, own_object_id: u32) -> String {
    if message.sender_id == 0 {
        return format!("You tell {sender}, \"{text}\"");
    }
    // A tell the character sent to itself comes back from itself.
    if own_object_id != 0 && message.sender_id == own_object_id {
        return format!("You think, \"{text}\"");
    }
    format!(
        "{} tells you, \"{text}\"",
        speaker(sender, message.sender_id)
    )
}

fn speaker(name: &str, object_id: u32) -> String {
    if (FIRST_PLAYER_OBJECT_ID..=LAST_PLAYER_OBJECT_ID).contains(&object_id) {
        link(object_id, name)
    } else {
        name.to_owned()
    }
}

/// The client's tell link; the id is written in decimal.
fn link(object_id: u32, name: &str) -> String {
    format!("<Tell:IIDString:{object_id}:{name}>{name}<\\Tell>")
}

fn is_own(message: &ChatLine, own_object_id: u32) -> bool {
    is_own_speaker(&message.sender) || (own_object_id != 0 && message.sender_id == own_object_id)
}

// The host names the character's own speech "You", or leaves the speaker
// out on a line the server sends back to its sender.
fn is_own_speaker(sender: &str) -> bool {
    sender.is_empty() || sender == "You"
}

/// MossTank's tag.
const MOSSTANK_TAG: &str = "[MossTank]";

/// Shows a MossTank line in chat, tagged.
pub fn post_mosstank(host: &mut dyn MetaHost, text: &str) {
    let line = if text.starts_with(MOSSTANK_TAG) {
        text.to_owned()
    } else {
        format!("{MOSSTANK_TAG} {text}")
    };
    host.chat_post(&line, 0);
}

/// Writes a line in the log at `level`.
pub fn log(host: &mut dyn MetaHost, level: LogLevel, message: &str) {
    host.log(level, message);
}

/// UtilityBelt's tag, with the space that follows it.
pub const UB_TAG: &str = "[UB] ";

/// The text class of an ordinary UtilityBelt line (System).
pub const UB_GENERIC_CHAT_TYPE: u32 = 0x05;

/// The text class of a UtilityBelt debug line (Abuse).
pub const UB_DEBUG_CHAT_TYPE: u32 = 0x0E;

/// The text class of a UtilityBelt error line (Help).
pub const UB_ERROR_CHAT_TYPE: u32 = 0x0F;

const UB_ERROR_LEAD: &str = "[UB] Error: ";

/// A kind's display as the reference ships it.
#[must_use]
pub fn ub_default_display(kind: UbLineKind) -> UbDisplay {
    UbDisplay {
        show: true,
        chat_type: match kind {
            UbLineKind::Debug => UB_DEBUG_CHAT_TYPE,
            UbLineKind::Error => UB_ERROR_CHAT_TYPE,
            UbLineKind::Generic | UbLineKind::Expression => UB_GENERIC_CHAT_TYPE,
        },
    }
}

/// A plain UtilityBelt line: the tag, then the text.
#[must_use]
pub fn ub_line(text: &str) -> String {
    if text.starts_with("[UB]") {
        text.to_owned()
    } else {
        format!("{UB_TAG}{text}")
    }
}

/// A UtilityBelt error line: the tag, "Error: ", then the text.
#[must_use]
pub fn ub_error(text: &str) -> String {
    ub_line(&format!("Error: {text}"))
}

/// Prints a finished UtilityBelt line through its kind's display: an error
/// line or a generic one, by its lead.
pub fn ub_post(host: &mut dyn MetaHost, line: &str) {
    let tagged = ub_line(line);
    let kind = if tagged.starts_with(UB_ERROR_LEAD) {
        UbLineKind::Error
    } else {
        UbLineKind::Generic
    };
    let display = host.ub_display(kind);
    if display.show {
        host.chat_post(&tagged, display.chat_type);
    }
}

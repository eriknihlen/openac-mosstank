//! `/ub fellow`: the reference's fellowship command, one line per thing
//! the client's own social panel can do (`MossTankCommands.Fellow`). Every
//! line only asks the client; the fellowship state it reads is what the
//! server last said.
//!
//! The quiet refusals (recruiting while neither leader nor open, handing
//! the lead to someone outside the fellowship, opening an open
//! fellowship) are the reference's: it checks and sends nothing, and says
//! nothing either.

use ac_plugin_abi::social::{FellowMember, FellowshipStatus};
use ac_plugin_abi::world::{ObjectClass, WorldObject};
use openac_plugin_compat::text::casing;

use crate::client::FellowshipAutomation;
use crate::object_search::{self, SearchView};
use crate::ub_chat::{self, UbChatSink};

/// The command's usage line.
pub const FELLOW_USAGE: &str = "/ub fellow create <Name>|quit|disband|open|close|status|recruit[p][ Name]|dismiss[p][ Name]|leader[p][ Name]";

/// How far away a player can be and still be recruited, metres. The
/// reference sends nothing for anyone farther.
const FELLOW_RECRUIT_RANGE_METERS: f64 = 75.0;

/// What `/ub fellow` reads and asks beyond the fellowship manager's view.
pub trait FellowCommandAutomation: FellowshipAutomation + UbChatSink {
    /// Starts a fellowship.
    fn fellowship_create(&mut self, name: &str, share_experience: bool) -> FellowshipStatus;

    /// Leaves the fellowship, or ends it for everyone.
    fn fellowship_quit(&mut self, disband: bool) -> FellowshipStatus;

    /// Whether the fellowship shares experience.
    fn fellowship_shares_experience(&mut self) -> bool;

    /// Whether the fellowship splits experience evenly.
    fn fellowship_splits_experience_evenly(&mut self) -> bool;

    /// A member's level.
    fn fellow_level(&mut self, member: &FellowMember) -> u32;

    /// Every object the client knows.
    fn world_objects(&mut self) -> Vec<WorldObject>;

    /// The selected object, if any.
    fn selected_object_id(&mut self) -> Option<u32>;
}

/// What a `/ub fellow` line came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FellowCommandOutcome {
    /// The line was understood (and answered, or quietly refused).
    Done,
    /// The line does not parse: the dispatcher says "Bad command syntax"
    /// and the command's help.
    BadSyntax,
}

fn trim(text: &str) -> &str {
    text.trim_matches(casing::is_white_space)
}

/// The reference's grammar:
/// `^(create .+|quit|disband|open|close|status|recruit(p)?( .+)?|dismiss(p)?( .+)?|leader(p)?( .+)?)$`,
/// any case.
fn matches_grammar(line: &str) -> bool {
    if line.contains('\n') {
        return false;
    }
    let lower = casing::to_lower_invariant(line);
    if let Some(name) = lower.strip_prefix("create ") {
        return !name.is_empty();
    }
    if matches!(
        lower.as_str(),
        "quit" | "disband" | "open" | "close" | "status"
    ) {
        return true;
    }
    for verb in ["recruit", "dismiss", "leader"] {
        let Some(rest) = lower.strip_prefix(verb) else {
            continue;
        };
        let rest = rest.strip_prefix('p').unwrap_or(rest);
        if rest.is_empty() {
            return true;
        }
        if let Some(name) = rest.strip_prefix(' ') {
            return !name.is_empty();
        }
    }
    false
}

/// The head and the rest of a line (`SplitHead`): split at the first space
/// or tab, the rest trimmed.
fn split_head(value: &str) -> (&str, &str) {
    let trimmed = trim(value);
    match trimmed.find([' ', '\t']) {
        Some(separator) => (&trimmed[..separator], trim(&trimmed[separator + 1..])),
        None => (trimmed, ""),
    }
}

fn write(automation: &mut dyn FellowCommandAutomation, text: &str) {
    ub_chat::post(automation, &ub_chat::line(text));
}

fn write_error(automation: &mut dyn FellowCommandAutomation, text: &str) {
    ub_chat::post(automation, &ub_chat::error(text));
}

/// Answers one `/ub fellow` line. `debug` is UtilityBelt's
/// `Plugin.Debug`: with it on, the command names whom it recruits,
/// dismisses or hands the lead to, and whom it could not find.
pub fn run(
    automation: &mut dyn FellowCommandAutomation,
    arguments: &str,
    debug: bool,
) -> FellowCommandOutcome {
    let line = trim(arguments);
    if !matches_grammar(line) {
        return FellowCommandOutcome::BadSyntax;
    }

    let (head, name) = split_head(line);
    let verb = casing::to_lower_invariant(head);
    let state = automation.fellowship_state();
    if !state.is_in_fellowship && verb != "create" {
        write(automation, "Your are not currently in a fellowship.");
        return FellowCommandOutcome::Done;
    }

    let own = automation.character().id;
    let is_leader = state.leader_id == own;
    match verb.as_str() {
        "create" => {
            if state.is_in_fellowship {
                write(automation, "You are already in a fellowship.");
                return FellowCommandOutcome::Done;
            }
            // The reference passes the character's own share-experience
            // option; this client does not expose it, and the option is on
            // unless the player turned it off.
            let result = automation.fellowship_create(name, true);
            report(automation, result, "create a fellowship");
        }
        "quit" => {
            let result = automation.fellowship_quit(false);
            report(automation, result, "quit the fellowship");
        }
        "disband" => {
            if !is_leader {
                write(automation, "You are not the fellowship leader!");
                return FellowCommandOutcome::Done;
            }
            let result = automation.fellowship_quit(true);
            report(automation, result, "disband the fellowship");
        }
        "open" | "close" => {
            let open = verb == "open";
            if !is_leader || state.is_open == open {
                return FellowCommandOutcome::Done;
            }
            let result = automation.fellowship_set_open(open);
            report(
                automation,
                result,
                if open {
                    "open the fellowship"
                } else {
                    "close the fellowship"
                },
            );
        }
        "status" => write_status(automation, own),
        "recruit" | "recruitp" => {
            recruit(automation, is_leader, name, verb == "recruitp", debug);
        }
        "dismiss" | "dismissp" => {
            dismiss(automation, own, is_leader, name, verb == "dismissp", debug);
        }
        _ => assign_leader(automation, own, is_leader, name, verb == "leaderp", debug),
    }
    FellowCommandOutcome::Done
}

fn recruit(
    automation: &mut dyn FellowCommandAutomation,
    is_leader: bool,
    name: &str,
    partial: bool,
    debug: bool,
) {
    // The reference names whom it recruits, and whom it could not find,
    // only with its debug setting on. A "could not find player" error here
    // is read by the profiles that wait on that line from the follow
    // command, so it must not appear with debug off.
    let objects = automation.world_objects();
    let player = automation.navigation();
    let view = SearchView {
        objects: &objects,
        self_id: automation.character().id,
        player: &player,
        selected: automation.selected_object_id(),
        open_container: 0,
    };
    let Some(found) =
        object_search::try_find_nearest(&view, Some(name), partial, &[ObjectClass::Player])
    else {
        write_not_found(automation, name, debug);
        return;
    };
    if debug {
        write(
            automation,
            &format!("Recruiting {}[0x{:08X}]", found.name, found.object_id),
        );
    }
    if !is_leader && !automation.fellowship_state().is_open {
        return;
    }
    if automation
        .fellowship_roster()
        .iter()
        .any(|member| member.object_id == found.object_id)
    {
        return;
    }
    if object_search::distance(&player, found) > FELLOW_RECRUIT_RANGE_METERS {
        return;
    }
    let result = automation.fellowship_recruit(found.object_id);
    report(automation, result, &format!("recruit {}", found.name));
}

fn find_fellow(
    automation: &mut dyn FellowCommandAutomation,
    own: u32,
    name: &str,
    partial: bool,
) -> Option<FellowMember> {
    let roster = automation.fellowship_roster();
    let selected = automation.selected_object_id();
    object_search::try_find_fellow(&roster, Some(name), partial, own, selected).cloned()
}

fn dismiss(
    automation: &mut dyn FellowCommandAutomation,
    own: u32,
    is_leader: bool,
    name: &str,
    partial: bool,
    debug: bool,
) {
    let Some(member) = find_fellow(automation, own, name, partial) else {
        write_not_found(automation, name, debug);
        return;
    };
    if debug {
        write(
            automation,
            &format!("Dismissing {}[0x{:08X}]", member.name, member.object_id),
        );
    }
    // Dismissing yourself is leaving: the reference turns it into a quit,
    // the only form the server answers sensibly.
    if member.object_id == own {
        let result = automation.fellowship_quit(false);
        report(automation, result, "quit the fellowship");
        return;
    }
    if !is_leader {
        return;
    }
    let result = automation.fellowship_dismiss(member.object_id);
    report(automation, result, &format!("dismiss {}", member.name));
}

fn assign_leader(
    automation: &mut dyn FellowCommandAutomation,
    own: u32,
    is_leader: bool,
    name: &str,
    partial: bool,
    debug: bool,
) {
    let Some(member) = find_fellow(automation, own, name, partial) else {
        write_not_found(automation, name, debug);
        return;
    };
    if debug {
        write(
            automation,
            &format!(
                "Transferring leader to {}[0x{:08X}]",
                member.name, member.object_id
            ),
        );
    }
    if !is_leader || member.object_id == own {
        return;
    }
    let result = automation.fellowship_assign_leader(member.object_id);
    report(
        automation,
        result,
        &format!("give the lead to {}", member.name),
    );
}

/// The reference's status line and one line per member, each with its
/// level. The split clause follows the reference's own test: it is printed
/// whenever the split is not even, sharing or not.
fn write_status(automation: &mut dyn FellowCommandAutomation, own: u32) {
    let state = automation.fellowship_state();
    let count = state.member_count;
    let leader = state.leader_id;
    let plural = if count != 1 { "s" } else { "" };
    let open = if state.is_open { "Open" } else { "Closed" };
    let locked = if state.is_locked {
        "**LOCKED**"
    } else {
        "Not Locked"
    };
    let sharing = if automation.fellowship_shares_experience() {
        ""
    } else {
        "NOT "
    };
    let split = if automation.fellowship_splits_experience_evenly() {
        ""
    } else {
        ", Uneven Split"
    };
    write(
        automation,
        &format!(
            "{leader:08X} {own:08X} Your current fellowship, \"{}\", has {count} member{plural}, {sharing}Sharing XP{split}. {open}, {locked}.",
            state.name
        ),
    );
    for member in automation.fellowship_roster() {
        // Each member is a message of its own in the reference, so each
        // carries the tag before its leading space.
        let level = automation.fellow_level(&member);
        write(
            automation,
            &format!(
                " {}[{level}] H:{}/{}{}",
                member.name,
                member.current_health,
                member.max_health,
                if member.object_id == leader {
                    " (Leader) "
                } else {
                    ""
                }
            ),
        );
    }
}

/// The reference's error for a name it could not find, written only while
/// its debug setting is on; it names the player as typed.
fn write_not_found(automation: &mut dyn FellowCommandAutomation, name: &str, debug: bool) {
    if debug {
        write_error(automation, &format!("Could not find player {name}"));
    }
}

/// Says so when the client did not send a fellowship command.
fn report(automation: &mut dyn FellowCommandAutomation, result: FellowshipStatus, what: &str) {
    if result == FellowshipStatus::Accepted {
        return;
    }
    write_error(
        automation,
        &if result == FellowshipStatus::Rejected {
            format!("The client refused to {what}.")
        } else {
            format!("Cannot {what} right now.")
        },
    );
}

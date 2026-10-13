//! The `/ub` commands (`MossTankCommands.Ub`, `MossTankCommands.Network`
//! and the UB parts of `MossTankCommands`): every UtilityBelt command word,
//! the ones that name an object, print what the client knows, run
//! something later, or reach the other clients.
//!
//! [`UbCommands`] holds what the commands keep between calls (the delayed
//! lines, the jump in the air, the dialog watch); [`UbCall`] is what one
//! call reaches. The tools other slices own are reached through
//! [`UbServices`]; the UB tools ([`crate::tools`]) run here.

pub mod call;
pub mod jumper;
pub mod opt;

use ac_plugin_abi::navigation::MovementIntent;
use ac_plugin_abi::world::{Confirmation, ObjectClass, WorldObject};
use mosstank_expr::reference_arguments::friendly_type_name;
use mosstank_expr::{ExpressionCoordinates, ExpressionValue, ExpressionVariableScope};
use mosstank_formats::route::RouteJumpDirection;
use mosstank_meta::held_motions::{self, HeldMotion};
use mosstank_meta::host::CombatMode;
use mosstank_meta::login_roster::{self, LoginRosterFailure};
use openac_plugin_compat::datetime::DateTime;
use openac_plugin_compat::number::{self, MidpointRounding, NumberStyles};
use openac_plugin_compat::text::{casing, ordinal};

pub use call::{UbCall, UbServices};
use jumper::{CommandJump, JumpRequest, JumpTurn};

use crate::chat::{self, tools};
use crate::help;
use crate::host::AllegianceStatus;
use crate::network;
use crate::prepclick::PrepClickController;
use crate::search::{self, UbSearchScope};
use crate::tools::UbTools;

/// What `/ub` answers to a word it has no command for.
pub const UNKNOWN_UB_COMMAND: &str =
    "[UB] Error: Command not found! Type \"ub help\" for a list of commands.";

/// The default format a bare date command prints in.
const DEFAULT_DATE_FORMAT: &str = "dddd dd MMMM HH:mm:ss";

/// Cantrips that raise a weapon's maximum damage, and by how much.
const MAX_DAMAGE_CANTRIPS: [(u32, i32); 5] = [
    (2598, 2),  // Minor Blood Thirst
    (2586, 4),  // Major Blood Thirst
    (4661, 7),  // Epic Blood Thirst
    (6089, 10), // Legendary Blood Thirst
    (3688, 300),
];

/// The appraised elemental damage added to every swing.
const ELEMENTAL_DAMAGE_BONUS_PROPERTY: u32 = 204;
/// How many times a tinker has already been put into an item.
const NUMBER_TIMES_TINKERED_PROPERTY: u32 = 171;
/// Non-zero once an imbue has taken one of the ten attempts.
const IMBUED_PROPERTY: u32 = 179;
/// The vitae multiplier the server keeps on the character.
const VITAE_PROPERTY: u32 = 129;

const PORTAL_CLASSES: [ObjectClass; 2] = [ObjectClass::Portal, ObjectClass::Npc];
const PLAYER_CLASSES: [ObjectClass; 1] = [ObjectClass::Player];

/// A line waiting on its delay.
#[derive(Debug, Clone, PartialEq)]
struct DelayedCommand {
    remaining_seconds: f64,
    command: String,
    delay_milliseconds: f64,
}

/// How a give matches the item's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiveNameMatch {
    /// The whole name.
    Exact,
    /// Any part of it.
    Partial,
    /// A regular expression.
    Pattern,
}

/// What the `/ub` commands keep between calls.
#[derive(Debug, Default)]
pub struct UbCommands {
    delayed: Vec<DelayedCommand>,
    jump: CommandJump,
    prep_click: PrepClickController,
    tools: UbTools,
}

/// Splits off the first word: the trimmed text up to the first space or
/// tab, and the trimmed rest (`SplitHead`).
#[must_use]
pub fn split_head(value: &str) -> (String, String) {
    let trimmed = value.trim();
    match trimmed.find([' ', '\t']) {
        None => (trimmed.to_owned(), String::new()),
        Some(at) => (
            trimmed[..at].to_owned(),
            trimmed[at + 1..].trim().to_owned(),
        ),
    }
}

/// The name with the first of `extensions` it ends with taken off
/// (`StripExtension`).
#[must_use]
pub fn strip_extension(name: &str, extensions: &[&str]) -> String {
    let result = name.trim();
    for extension in extensions {
        if ordinal::ends_with_ignore_case(result, extension) {
            return result[..result.len() - extension.len()].to_owned();
        }
    }
    result.to_owned()
}

/// A verb's flag letters, when the verb is `name` followed only by letters
/// from `allowed` (`TryFlags`).
fn try_flags<'v>(verb: &'v str, name: &str, allowed: &str) -> Option<&'v str> {
    if !ordinal::starts_with_ignore_case(verb, name) {
        return None;
    }
    let rest = &verb[name.len()..];
    rest.chars()
        .all(|letter| allowed.contains(letter))
        .then_some(rest)
}

/// Reads a typed verb as the give verb and its flags: `p` matches part of
/// the item's name, `P` part of the target's, `r` reads the item name as a
/// regular expression (and wins over `p`). Any other letter means this was
/// not a give verb at all.
#[must_use]
pub fn read_give_flags(typed: &str) -> Option<(GiveNameMatch, bool)> {
    if !ordinal::starts_with_ignore_case(typed, "give") {
        return None;
    }
    let mut partial_item = false;
    let mut partial_target = false;
    let mut pattern = false;
    for flag in typed[4..].chars() {
        match flag {
            'p' => partial_item = true,
            'P' => partial_target = true,
            'r' | 'R' => pattern = true,
            _ => return None,
        }
    }
    let matching = if pattern {
        GiveNameMatch::Pattern
    } else if partial_item {
        GiveNameMatch::Partial
    } else {
        GiveNameMatch::Exact
    };
    Some((matching, partial_target))
}

/// The value as the reference prints it beside its type name: a true or
/// false is the number 1 or 0 to it, everything else its display text.
fn value_text(call: &mut UbCall<'_>, value: &ExpressionValue) -> String {
    match value {
        ExpressionValue::Boolean(flag) => if *flag { "1" } else { "0" }.to_owned(),
        _ => call.expressions.display(call.host, value),
    }
}

/// `double.ToString(InvariantCulture)`.
fn invariant(value: f64) -> String {
    number::to_string_f64(value)
}

/// `{value:format}` in the invariant culture, for a format the kit has.
fn formatted(value: f64, format: &str) -> String {
    number::format_f64(value, format).expect("a number format the kit reproduces")
}

/// A hexadecimal landblock with or without its `0x` (`TryParseLandblock`).
fn parse_landblock(text: &str) -> Option<u32> {
    search::parse_hex(text)
}

impl UbCommands {
    /// Nothing waiting.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The jump or turn under way.
    #[must_use]
    pub fn jump(&self) -> &CommandJump {
        &self.jump
    }

    /// The command jump, for the macro's own `/vt jump` and `/vt face`.
    pub fn jump_mut(&mut self) -> &mut CommandJump {
        &mut self.jump
    }

    /// The dialog watch.
    #[must_use]
    pub fn prep_click(&self) -> &PrepClickController {
        &self.prep_click
    }

    /// The UB tools the commands start.
    #[must_use]
    pub fn tools(&self) -> &UbTools {
        &self.tools
    }

    /// The UB tools, for the plugin's frame to drive.
    pub fn tools_mut(&mut self) -> &mut UbTools {
        &mut self.tools
    }

    /// How many lines wait on their delay.
    #[must_use]
    pub fn delayed_count(&self) -> usize {
        self.delayed.len()
    }

    /// Runs one line typed under `/ub` (`ExecuteUbCommand`).
    pub fn execute(&mut self, call: &mut UbCall<'_>, input: &str) {
        let (typed, arguments) = split_head(input);

        // The give verb carries its flags in the letters after it, and their
        // case is the whole meaning; read before the verb is folded.
        if let Some((matching, partial_target)) = read_give_flags(&typed) {
            self.tools
                .give
                .borrow_mut()
                .give_command(call, &arguments, matching, partial_target);
            return;
        }

        let verb = casing::to_lower_invariant(&typed);
        match verb.as_str() {
            "" => self.write_version(call),
            "help" => help_command(call, &arguments),
            "opt" => opt::handle_opt(call, &arguments),
            "mexec" => execute_expression(call, &arguments, false),
            "mexecm" => execute_expression(call, &arguments, true),
            "propertydump" => dump_selected_properties(call),
            "dumpskills" => dump_skills(call),
            "login" => login_command(call, &arguments),
            "autovendor" => {
                let text = arguments.trim();
                if ["stop", "cancel", "quit"]
                    .iter()
                    .any(|word| ordinal::equals_ignore_case(text, word))
                {
                    call.services.auto_vendor_stop(call.host);
                } else {
                    call.services
                        .auto_vendor_start(call.host, (!text.is_empty()).then_some(text));
                }
            }
            "vendor" => {
                let reply = call.services.vendor_command(call.host, &arguments);
                call.post_ub_reply("vendor", reply);
            }
            "xp" => {
                let reply = self.tools.xp.command(call, &arguments);
                call.post_ub_reply("xp", reply);
            }
            "equip" => {
                let reply = self.tools.equip.command(call, &arguments);
                call.post_ub_reply("equip", reply);
            }
            "simplejump" => self.simple_jump(call, &arguments),
            "calcdamage" => calculate_selected_damage(call),
            "pos" => print_selected_position(call),
            "id" => {
                if let Some(item) = selected_object(call, "Id") {
                    call.write_ub(&id_line(&item));
                }
            }
            "vitae" => think_vitae(call),
            "combatstate" => combat_state(call, &arguments),
            "date" => print_date(call, false, &arguments),
            "dateutc" => print_date(call, true, &arguments),
            "delay" => self.delay_command(call, &arguments),
            "bc" => self.broadcast(call, &arguments),
            "bct" => self.tagged_broadcast(call, &arguments),
            "netclients" => net_clients(call, &arguments),
            "closestportal" => use_portal_by_name(call, "", true),
            "close" => close_command(call, &arguments),
            "printcolors" => {
                for (name, value) in chat::MESSAGE_TYPES {
                    call.host.chat_post(
                        &format!("[PrintColors]{name} ({value})"),
                        u32::try_from(value).unwrap_or_default(),
                    );
                }
            }
            "listvars" => list_variables(call, ExpressionVariableScope::Session),
            "listpvars" => list_variables(call, ExpressionVariableScope::Persistent),
            "listgvars" => list_variables(call, ExpressionVariableScope::Global),
            "myquests" => {
                call.post_ub(&chat::tool(tools::QUEST_TRACKER, "Refreshing quests"));
                call.expressions.refresh_quests(call.host);
            }
            "quit" => {
                call.write_ub("Quitting Client");
                if !call.host.request_close() {
                    call.write_ub_error("The client could not be closed from here.");
                }
            }
            "playeroption" => player_option(call, &arguments),
            "translateroute" => translate_route(call, &arguments),
            "face" => self.face(call, &arguments),
            "setmotion" => set_motion(call, &arguments),
            "clearmotion" => {
                call.expressions.held_motions().clear(call.host);
            }
            "prepclick" => {
                let mut lines = Vec::new();
                self.prep_click
                    .command(&arguments, &mut |line| lines.push(line.to_owned()));
                for line in lines {
                    call.write_transcript_line(&line);
                }
            }
            "fellow" => {
                if !call.services.fellow_command(call.host, &arguments) {
                    call.write_ub_bad_syntax("fellow");
                }
            }
            "autostack" => self.tools.stack_cram.start(call, true),
            "autocram" => self.tools.stack_cram.start(call, false),
            "count" => self.tools.count.borrow_mut().command(call, &arguments),
            "autotinker" => self.tools.tinker.auto_tinker(call),
            "getjob" => self.tools.tinker.print_jobs(call),
            "tinkcalc" => self.tools.tinker.print_calculation(call),
            "clearbugged" => self.tools.start_clear_bugged(call),
            _ => {
                // The flagged verbs (jumpsw, igp, usepi, ...) are matched by
                // name and flag set rather than spelled out.
                if !self.try_flagged(call, &verb, &arguments) {
                    write_unknown_command(call, &verb);
                }
            }
        }
    }

    /// Runs a command whose verb carries single-letter flags; false when the
    /// verb is none of them (`TryExecuteFlaggedCommand`).
    fn try_flagged(&mut self, call: &mut UbCall<'_>, verb: &str, arguments: &str) -> bool {
        if let Some(flags) = try_flags(verb, "jump", "swzxc") {
            self.jump_command(call, flags, arguments);
        } else if let Some(flags) = try_flags(verb, "ig", "p") {
            self.tools
                .give
                .borrow_mut()
                .item_giver_command(call, arguments, flags.contains('p'));
        } else if let Some(flags) = try_flags(verb, "portal", "p") {
            use_portal_by_name(call, arguments, flags.contains('p'));
        } else if let Some(flags) = try_flags(verb, "follow", "p") {
            follow(call, arguments, flags.contains('p'));
        } else if let Some(flags) = try_flags(verb, "use", "lip") {
            use_command(call, flags, arguments);
        } else if let Some(flags) = try_flags(verb, "select", "lip") {
            select_command(call, flags, arguments);
        } else if let Some(flags) = try_flags(verb, "swearallegiance", "p") {
            allegiance(call, verb, arguments, flags.contains('p'), true);
        } else if let Some(flags) = try_flags(verb, "breakallegiance", "p") {
            allegiance(call, verb, arguments, flags.contains('p'), false);
        } else {
            return false;
        }
        true
    }

    /// Bare `/ub`: which build of the compatibility surface is loaded.
    fn write_version(&self, call: &mut UbCall<'_>) {
        let version = call.services.plugin_version();
        // The reference prints the two as one message.
        call.write_ub_message(
            &format!("MossTank UB compatibility {version}"),
            [" Type `/ub help` or `/ub help <command>` for help."],
        );
    }

    // ---- jump, simplejump, face ----------------------------------------

    /// `/ub jump[swzxc] [heading] [holdtime]`.
    fn jump_command(&mut self, call: &mut UbCall<'_>, flags: &str, arguments: &str) {
        let parts: Vec<&str> = arguments.split(' ').filter(|p| !p.is_empty()).collect();
        if parts.len() > 2 {
            call.write_ub_bad_syntax("jump");
            return;
        }
        // Two numbers are heading then hold; one number on its own is the
        // hold, because a jump in place is the common case.
        let heading_text = (parts.len() == 2).then(|| parts[0]);
        let hold_text = match parts.len() {
            2 => Some(parts[1]),
            1 => Some(parts[0]),
            _ => None,
        };
        let Some(milliseconds) = parse_hold_time(call, hold_text) else {
            return;
        };
        let mut heading = call.host.navigation().position.heading_degrees;
        if let Some(text) = heading_text {
            // Digits and points only, so "NaN", "Infinity", "1e3" and "-5"
            // never read as a heading.
            let requested = is_plain_decimal(text)
                .then(|| number::parse_f64(text, NumberStyles::ALLOW_DECIMAL_POINT))
                .flatten();
            let Some(requested) = requested else {
                call.write_ub_bad_syntax("jump");
                return;
            };
            if !(0.0..=359.0).contains(&requested) {
                call.post_ub(&chat::tool_error(
                    tools::JUMPER,
                    "direction should be a number between 0 and 359",
                ));
                return;
            }
            #[allow(clippy::cast_possible_truncation)]
            {
                heading = requested as f32;
            }
        }
        if !self.refuse_when_already_jumping(call) {
            return;
        }
        // With no heading the reference does not turn at all.
        self.jump.start(
            call,
            &JumpRequest {
                shift: flags.contains('s'),
                turn: if heading_text.is_none() {
                    JumpTurn::None
                } else {
                    JumpTurn::Client
                },
                keys: Some(MovementIntent {
                    forward: flags.contains('w'),
                    backward: flags.contains('x'),
                    strafe_left: flags.contains('z'),
                    strafe_right: flags.contains('c'),
                    turn_left: false,
                    turn_right: false,
                    run: !flags.contains('s'),
                    jump: true,
                }),
                ..JumpRequest::new(heading, milliseconds)
            },
        );
    }

    /// `/ub simplejump [holdtime]`: a jump where the character stands.
    fn simple_jump(&mut self, call: &mut UbCall<'_>, arguments: &str) {
        let parts: Vec<&str> = arguments.split(' ').filter(|p| !p.is_empty()).collect();
        if parts.len() > 1 {
            call.write_ub_bad_syntax("simplejump");
            return;
        }
        let Some(milliseconds) = parse_hold_time(call, parts.first().copied()) else {
            return;
        };
        if !self.refuse_when_already_jumping(call) {
            return;
        }
        let heading = call.host.navigation().position.heading_degrees;
        self.jump.start(
            call,
            &JumpRequest {
                direction: Some(RouteJumpDirection::Forward),
                omit_direction: true,
                turn: JumpTurn::None,
                ..JumpRequest::new(heading, milliseconds)
            },
        );
    }

    /// `/ub face <heading>`: the jump's turn on its own.
    fn face(&mut self, call: &mut UbCall<'_>, arguments: &str) {
        let heading = number::parse_f32(arguments.trim(), NumberStyles::FLOAT)
            .filter(|heading| heading.is_finite());
        let Some(heading) = heading else {
            call.write_ub_bad_syntax("face");
            return;
        };
        self.jump.start(
            call,
            &JumpRequest {
                direction: Some(RouteJumpDirection::Forward),
                face_only: true,
                turn: JumpTurn::Client,
                ..JumpRequest::new(heading, 0)
            },
        );
    }

    /// False, having said so, when a jump is already in the air.
    fn refuse_when_already_jumping(&self, call: &mut UbCall<'_>) -> bool {
        if !self.jump.is_active() {
            return true;
        }
        call.post_ub(&chat::tool_error(
            tools::JUMPER,
            "You are already jumping. try again later.",
        ));
        false
    }

    // ---- delay -----------------------------------------------------------

    /// `/ub delay <milliseconds> <command>`.
    fn delay_command(&mut self, call: &mut UbCall<'_>, arguments: &str) {
        let (head, rest) = split_head(arguments);
        let milliseconds =
            number::parse_f64(&head, NumberStyles::FLOAT).filter(|ms| ms.is_finite() && *ms > 0.0);
        match milliseconds {
            Some(milliseconds) if !rest.is_empty() => {
                self.schedule(call, &rest, milliseconds);
            }
            _ => call.write_ub_bad_syntax("delay"),
        }
    }

    /// Queues a line to run once its delay is up; said only in the debug
    /// output, and again when the line runs.
    pub fn schedule(&mut self, call: &mut UbCall<'_>, command: &str, milliseconds: f64) {
        call.write_ub_debug(&format!(
            "Scheduling command `{command}` with delay of {}ms",
            invariant(milliseconds)
        ));
        self.delayed.push(DelayedCommand {
            remaining_seconds: milliseconds / 1000.0,
            command: command.to_owned(),
            delay_milliseconds: milliseconds,
        });
        // A stable sort, as the C#'s comparison sort happens to be on these
        // short lists; lines due together keep the order they were typed.
        self.delayed
            .sort_by(|a, b| a.remaining_seconds.total_cmp(&b.remaining_seconds));
    }

    /// Runs the delayed lines whose wait is over.
    fn tick_delayed(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64) {
        if self.delayed.is_empty() {
            return;
        }
        let step = elapsed_seconds.max(0.0);
        let mut due = Vec::new();
        let mut index = self.delayed.len();
        while index > 0 {
            index -= 1;
            self.delayed[index].remaining_seconds -= step;
            if self.delayed[index].remaining_seconds > 0.0 {
                continue;
            }
            due.push(self.delayed.remove(index));
        }
        // The loop walks backwards, so the oldest entry comes out last.
        for line in due.into_iter().rev() {
            call.write_ub_tool_debug(
                tools::PLUGIN,
                &format!(
                    "Executing command `{}` (delay was {}ms)",
                    line.command,
                    invariant(line.delay_milliseconds)
                ),
            );
            call.host.chat_submit(&line.command);
        }
    }

    // ---- bc, bct ---------------------------------------------------------

    /// `/ub bc [millisecondDelay] <command>`: runs a line on every client on
    /// this computer, this one included (the client never hands a sender
    /// back its own broadcast, so the sender runs it here).
    fn broadcast(&mut self, call: &mut UbCall<'_>, arguments: &str) {
        let Some((delay, command)) = read_delay(call, arguments) else {
            return;
        };
        if command.is_empty() {
            call.write_ub_bad_syntax("bc");
            return;
        }
        call.write_ub(&format!(
            "Broadcasting command to all clients: \"{command}\" with delay inbetween of {delay}ms"
        ));
        self.schedule(call, &command, 0.0);
        if !call.host.broadcast_command(&command, &[], delay) {
            call.write_ub_error("Unable to broadcast command to the other clients.");
            return;
        }
        let mut clients = call.host.peer_clients();
        clients.sort_by_key(|client| client.client_id);
        for client in clients {
            call.write_ub(&format!(
                "Sending {}: \"{command}\" with delay inbetween of {delay}ms",
                client.name
            ));
        }
    }

    /// `/ub bct <tags> [millisecondDelay] <command>`: the same, aimed at the
    /// clients answering to one of the labels. This client runs the line
    /// itself only when one of those labels is its own.
    fn tagged_broadcast(&mut self, call: &mut UbCall<'_>, arguments: &str) {
        let Some((tags, rest)) = network::split_tag_list(arguments) else {
            call.write_ub_bad_syntax("bct");
            return;
        };
        let Some((delay, command)) = read_delay(call, &rest) else {
            return;
        };
        if tags.is_empty() {
            call.write_ub_error("You must specify at least one tag to send the command to.");
            return;
        }
        if command.is_empty() {
            call.write_ub_bad_syntax("bct");
            return;
        }
        call.write_ub(&format!(
            "Broadcasting command to clients with tags ({}): \"{command}\" with delay inbetween of {delay}ms",
            tags.join(",")
        ));
        // Ignoring case, as the client's own delivery does.
        let own = own_tags(call);
        if tags.iter().any(|tag| {
            own.iter()
                .any(|mine| ordinal::equals_ignore_case(mine, tag))
        }) {
            self.schedule(call, &command, 0.0);
        }
        if !call.host.broadcast_command(&command, &tags, delay) {
            call.write_ub_error("Unable to broadcast command to the other clients.");
        }
    }

    // ---- the frame, the session -------------------------------------------

    /// One frame of the commands: the delayed lines, the dialog watch, and
    /// the jump; true while the jump owns the character.
    pub fn on_tick(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64) -> bool {
        self.tick_early(call, elapsed_seconds);
        self.tick_command_jump(call, elapsed_seconds)
    }

    /// The frame's early part: the delayed lines and the dialog watch
    /// (`TickDelayedCommands`, `PrepClick.OnTick`).
    pub fn tick_early(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64) {
        self.tick_delayed(call, elapsed_seconds);
        let mut lines = Vec::new();
        self.prep_click
            .on_tick(elapsed_seconds, &mut |line| lines.push(line.to_owned()));
        for line in lines {
            call.write_transcript_line(&line);
        }
    }

    /// The command jump, which owns the character while it lasts
    /// (`TickCommandJump`).
    pub fn tick_command_jump(&mut self, call: &mut UbCall<'_>, elapsed_seconds: f64) -> bool {
        self.jump.tick(call, elapsed_seconds)
    }

    /// The server asks a yes/no question.
    pub fn on_confirmation(&mut self, call: &mut UbCall<'_>, confirmation: &Confirmation) {
        let mut lines = Vec::new();
        self.prep_click
            .on_confirmation(call.host, confirmation, &mut |line| {
                lines.push(line.to_owned());
            });
        for line in lines {
            call.write_transcript_line(&line);
        }
        // The tinker run says yes to the craft prompt it is waiting on.
        self.tools
            .tinker
            .manager_mut()
            .on_confirmation(call, confirmation);
    }

    /// The session ended: a line scheduled before it must not arrive after
    /// it, and whatever was held is let go (`ResetCommandSession`).
    pub fn reset_session(&mut self, call: &mut UbCall<'_>) {
        self.jump.reset(call);
        self.delayed.clear();
        self.prep_click.reset();
        call.expressions.held_motions().reset();
    }
}

/// `/ub help [command]`: a known command prints its usage and description,
/// anything else (or nothing) the whole list of commands.
fn help_command(call: &mut UbCall<'_>, arguments: &str) {
    let requested = arguments.trim();
    if !requested.is_empty()
        && let Some(usage) = help::try_get(requested)
    {
        call.write_ub_command_help(usage);
        return;
    }
    let names: Vec<&str> = help::ENTRIES.iter().map(|entry| entry.name).collect();
    call.write_ub_message(
        &format!("All available UB commands: /ub {{{}}}", names.join(", ")),
        ["For help with a specific command, use `/ub help [command]`"],
    );
}

/// The reference's answer to a word it has no command for: where to look,
/// then every command within two edits of what was typed.
fn write_unknown_command(call: &mut UbCall<'_>, verb: &str) {
    call.post_ub(UNKNOWN_UB_COMMAND);
    let near: Vec<&str> = help::ENTRIES
        .iter()
        .map(|entry| entry.name)
        .filter(|name| help::edit_distance(verb, name) <= 2)
        .collect();
    if !near.is_empty() {
        call.write_ub_error(&format!("Did you mean one of these? {}", near.join(", ")));
    }
}

/// `/ub mexec <expression>`: the expression, then its result with the
/// result's type and how long it took; `/ub mexecm` runs it and says only
/// an error.
fn execute_expression(call: &mut UbCall<'_>, source: &str, silent: bool) {
    if !silent {
        call.post_expression(&format!("Evaluating expression: \"{source}\""));
    }
    let started = call.services.monotonic_ms();
    let result = call.expressions.evaluate(call.host, source);
    let value = match result {
        Ok(value) => value,
        Err(error) => {
            // One message: the error under the tag, then the reason indented
            // on the line below it.
            call.write_ub_error_message(
                &format!("Error in expression: {source}"),
                [format!("  {}", error.message())],
            );
            return;
        }
    };
    let elapsed = call.services.monotonic_ms() - started;
    if silent {
        return;
    }
    let milliseconds = number::round_digits(elapsed, 3, MidpointRounding::ToEven);
    let kind = friendly_type_name(&value);
    let text = value_text(call, &value);
    call.post_expression(&format!(
        "Result: [{kind}] {text} ({}ms)",
        invariant(milliseconds)
    ));
}

/// `/ub setmotion <motion> <0|1>`: presses or releases one movement key and
/// leaves it that way, through the keys `setmotion[]` holds.
fn set_motion(call: &mut UbCall<'_>, arguments: &str) {
    // `^(?<motion>\w.+) (?<state>[01])$`: a name of two or more characters,
    // one space, and a 0 or a 1.
    let text = arguments.trim();
    let parsed = text.rsplit_once(' ').and_then(|(name, state)| {
        let starts_with_word = name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        (starts_with_word && name.chars().count() >= 2 && (state == "0" || state == "1"))
            .then_some((name, state == "1"))
    });
    let Some((name, on)) = parsed else {
        call.write_ub_bad_syntax("setmotion");
        return;
    };
    let Some(motion) = HeldMotion::parse(name) else {
        call.write_ub_error(&format!(
            "Invalid option ({name}). Valid values are: {}",
            held_motions::valid_names()
        ));
        return;
    };
    call.expressions.held_motions().set(call.host, motion, on);
}

/// The hold in milliseconds, 0 when none was typed; `None`, having said
/// so, when it is not a number the client can hold a jump key for.
fn parse_hold_time(call: &mut UbCall<'_>, text: Option<&str>) -> Option<i32> {
    let Some(text) = text else {
        return Some(0);
    };
    match mosstank_dotnet::try_parse_int(text) {
        Some(milliseconds) if (0..=1000).contains(&milliseconds) => Some(milliseconds),
        _ => {
            call.post_ub(&chat::tool_error(
                tools::JUMPER,
                "holdtime should be a number between 0 and 1000",
            ));
            None
        }
    }
}

/// True when the text is nothing but digits and decimal points.
fn is_plain_decimal(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c == '.' || c.is_ascii_digit())
}

/// `/ub calcdamage`: what the selected missile weapon would hit for once
/// every tinker it can still take has gone in.
#[allow(clippy::cast_precision_loss)]
fn calculate_selected_damage(call: &mut UbCall<'_>) {
    let selected = call.host.selection().selected.unwrap_or(0);
    let item = if selected == 0 {
        None
    } else {
        call.host.object(selected)
    };
    let Some(item) = item else {
        call.write_ub_error("Nothing selected");
        return;
    };
    let properties = call.host.properties(selected);
    let Some((properties, profile)) =
        properties.and_then(|p| p.weapon.clone().map(|weapon| (p, weapon)))
    else {
        call.write_ub_error(&format!(
            "{} does not have id data, please examine it first.",
            item.name
        ));
        return;
    };
    if item.object_class != ObjectClass::MissileWeapon {
        call.write_ub_error(&format!(
            "Calc Damage: {:?} is not currently supported",
            item.object_class
        ));
        return;
    }

    let max_damage = f64::from(profile.damage);
    let damage_bonus = number::round_digits(profile.damage_mod, 2, MidpointRounding::ToEven);
    let elemental_bonus = properties.int(ELEMENTAL_DAMAGE_BONUS_PROPERTY).unwrap_or(0);
    let mut cantrip_bonus = 0;
    for spell_id in &item.spell_ids {
        let bonus = MAX_DAMAGE_CANTRIPS
            .iter()
            .find(|(candidate, _)| candidate == spell_id)
            .map_or(0, |(_, bonus)| *bonus);
        if bonus == 0 {
            continue;
        }
        cantrip_bonus += bonus;
        let name = spell_name(call, *spell_id);
        call.write_ub(&format!("Spell {name} buffs MaxDamage by {bonus}"));
    }
    let times_tinkered = properties.int(NUMBER_TIMES_TINKERED_PROPERTY).unwrap_or(0);
    let imbued = properties.int(IMBUED_PROPERTY).unwrap_or(0) > 0;
    let tinkers_available = 10 - (times_tinkered + i32::from(!imbued)).min(10);
    let per_tinker = f64::from(tinkers_available) * 0.04;
    let damage = (max_damage + f64::from(cantrip_bonus) + f64::from(elemental_bonus))
        * (damage_bonus + per_tinker - 1.0);

    if tinkers_available > 0 {
        call.write_ub(&format!(
            "{tinkers_available} mahogany salvage adds {} to DamageModifier",
            invariant(per_tinker)
        ));
    }
    call.write_ub("Formula: (DamageBonus + ElementalBonus) * DamageModifier");
    call.write_ub(&format!(
        "Calculated Formula: ({}(+{cantrip_bonus} from cantrips) + {elemental_bonus}) * {}(+{} from {tinkers_available} tinkers)",
        invariant(max_damage),
        invariant(damage_bonus - 1.0),
        invariant(per_tinker)
    ));
    call.write_ub(&format!("Calculated (after tinks): {}", invariant(damage)));
}

/// The spell's name, its number when the catalog has no entry for it.
fn spell_name(call: &mut UbCall<'_>, spell_id: u32) -> String {
    call.host
        .spell(spell_id)
        .map(|spell| spell.name)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| spell_id.to_string())
}

fn id_line(item: &WorldObject) -> String {
    format!("Id: {} ( 0x{:08X} )", item.object_id, item.object_id)
}

/// The selected object, or `None` having said why (`TryGetSelectedObject`).
fn selected_object(call: &mut UbCall<'_>, verb: &str) -> Option<WorldObject> {
    let selected = call.host.selection().selected.unwrap_or(0);
    if selected == 0 {
        call.write_ub_error(&format!("{verb}: No object selected"));
        return None;
    }
    let item = call.host.object(selected);
    if item.is_none() {
        call.write_ub_error(&format!("{verb}: null object selected"));
    }
    item
}

/// `/ub pos`: where the selected object stands, in every form the client
/// can say it.
fn print_selected_position(call: &mut UbCall<'_>) {
    let Some(item) = selected_object(call, "pos") else {
        return;
    };
    let Some(position) = item.position.clone() else {
        call.write_ub(&id_line(&item));
        call.write_ub_error("pos: the client knows no position for that object.");
        return;
    };
    let coordinates =
        ExpressionCoordinates::new(position.east_west, position.north_south, position.elevation);
    call.write_ub(&id_line(&item));
    call.write_ub(&format!("Coords: {}", coordinates.to_compass_text()));
    call.write_ub(&format!("Landcell: 0x{:08X}", position.cell_id));
    call.write_ub(&format!(
        "Position: ew:{} ns:{} z:{}",
        invariant(position.east_west),
        invariant(position.north_south),
        invariant(position.elevation)
    ));
    let player = call.host.navigation();
    call.write_ub(&format!(
        "Distance: {} m",
        formatted(search::distance(&player, &item), "0.###")
    ));
}

/// `/ub propertydump`: the dump as the macro prints it, under a UB heading.
fn dump_selected_properties(call: &mut UbCall<'_>) {
    let Some(item) = selected_object(call, "propertydump") else {
        return;
    };
    call.write_ub(&format!("Property Dump for {}", item.name));
    call.write_vtank(&format!(
        "Object 0x{:08X}: {}, class={}, WCID={}",
        item.object_id, item.name, item.object_class as u32, item.weenie_class_id
    ));
    let Some(properties) = call.host.properties(item.object_id) else {
        return;
    };
    dump_table(call, "Int", &properties.ints, |v| v.to_string());
    dump_table(call, "Int64", &properties.int64s, |v| v.to_string());
    dump_table(call, "Bool", &properties.bools, |v| {
        if *v { "True" } else { "False" }.to_owned()
    });
    dump_table(call, "Float", &properties.floats, |v| invariant(*v));
    dump_table(call, "String", &properties.strings, Clone::clone);
    dump_table(call, "DataId", &properties.data_ids, |v| v.to_string());
    dump_table(call, "InstanceId", &properties.instance_ids, |v| {
        v.to_string()
    });
}

fn dump_table<T>(
    call: &mut UbCall<'_>,
    kind: &str,
    values: &[(u32, T)],
    text: impl Fn(&T) -> String,
) {
    let mut ordered: Vec<&(u32, T)> = values.iter().collect();
    ordered.sort_by_key(|(key, _)| *key);
    for (key, value) in ordered {
        call.write_vtank(&format!("{kind}[{key}] = {}", text(value)));
    }
}

/// `/ub dumpskills`: every line a message of its own under the tag.
fn dump_skills(call: &mut UbCall<'_>) {
    let mut skills = call.host.skills();
    call.write_ub(&format!("Character skills ({}):", skills.len()));
    skills.sort_by_key(|skill| skill.skill_id);
    for skill in skills {
        call.write_ub(&format!(
            "{}\t{}\t{}\t{}\t{:?}",
            skill.skill_id, skill.name, skill.base, skill.current, skill.training
        ));
    }
}

/// `/ub vitae`: says the penalty out loud, as a think.
fn think_vitae(call: &mut UbCall<'_>) {
    let own = call.host.character().map_or(0, |info| info.id);
    let multiplier = call
        .host
        .properties(own)
        .and_then(|properties| {
            properties
                .floats
                .iter()
                .find(|(key, _)| *key == VITAE_PROPERTY)
                .map(|(_, value)| *value)
        })
        .unwrap_or(1.0);
    call.think(&format!(
        "My vitae is {}%",
        invariant(number::round(multiplier * 100.0))
    ));
}

/// `/ub combatstate (peace|melee|missile|magic)`.
fn combat_state(call: &mut UbCall<'_>, arguments: &str) {
    let requested = casing::to_lower_invariant(arguments.trim());
    let mode = match requested.as_str() {
        "peace" => CombatMode::Peace,
        "melee" => CombatMode::Melee,
        "missile" => CombatMode::Missile,
        "magic" => CombatMode::Magic,
        _ => {
            call.write_ub_error(&format!("{requested} is not a valid option"));
            return;
        }
    };
    let result = call.host.enter_combat_mode(mode);
    if result.accepted() {
        call.write_ub(&format!("Combat state set to {mode:?}."));
    } else {
        call.write_ub_error(&format!(
            "Could not set combat state to {mode:?}: {:?}",
            result.status
        ));
    }
}

/// `/ub date[utc] [format]`: a .NET custom date format, in the invariant
/// culture.
fn print_date(call: &mut UbCall<'_>, utc: bool, format: &str) {
    let pattern = match format.trim() {
        "" => DEFAULT_DATE_FORMAT,
        pattern => pattern,
    };
    let now_ms = call.services.now_utc_ms();
    let now = if utc {
        DateTime::utc_from_unix_millis(now_ms)
    } else {
        let offset = call.services.local_offset_minutes();
        DateTime::local_from_unix_millis(now_ms, offset)
    };
    match now.format(pattern) {
        Ok(text) => call.write_ub(&format!("Current Date: {text}")),
        Err(error) => call.write_ub(&error.to_string()),
    }
}

/// `/ub closestportal`, `/ub portal[p] <name>`: uses a portal by name, or
/// with a blank name the nearest one.
fn use_portal_by_name(call: &mut UbCall<'_>, name: &str, partial: bool) {
    let Some(portal) = search::try_find_nearest(call.host, name, partial, &PORTAL_CLASSES) else {
        if call.flag("Plugin.PortalThink") {
            call.think("Could not find a portal");
        } else {
            call.write_ub("Could not find a portal");
        }
        return;
    };
    call.write_ub(&format!("Attempting to use portal: {}", portal.name));
    call.write_ub_tool_debug(
        tools::PLUGIN,
        &format!("Attempting to use portal {}", portal.name),
    );
    let result = call.host.use_item(portal.object_id);
    if !result.accepted() {
        call.post_ub(&chat::tool(
            tools::PLUGIN,
            &format!("Unable to use portal {}: {:?}", portal.name, result.status),
        ));
    }
}

/// `/ub follow[p] <name>`: aims the follow route at a player.
fn follow(call: &mut UbCall<'_>, name: &str, partial: bool) {
    let requested = name.trim();
    let Some(player) = search::try_find_nearest(call.host, requested, partial, &PLAYER_CLASSES)
    else {
        call.write_ub_error(&if requested.is_empty() {
            "Could not find closest player".to_owned()
        } else {
            format!("Could not find player {requested}")
        });
        return;
    };
    let who = format!("{}[0x{:08X}]", player.name, player.object_id);
    if !call
        .services
        .follow(call.host, player.object_id, &player.name)
    {
        call.write_ub_error(&format!("Failed to follow {who}"));
        return;
    }
    call.write_ub(&format!("Following {who}"));
    if !call.services.navigation_enabled() {
        call.write_ub("Turn Enable Navigation on to start.");
    }
}

/// The search scope the flag letters ask for; the two place flags are a
/// choice, not a pair.
fn read_scope(call: &mut UbCall<'_>, flags: &str) -> Option<(UbSearchScope, bool)> {
    let partial = flags.contains('p');
    if flags.contains('l') && flags.contains('i') {
        call.write_ub("l and i cannot be used in the same command");
        return None;
    }
    let scope = if flags.contains('i') {
        UbSearchScope::Inventory
    } else if flags.contains('l') {
        UbSearchScope::Landscape
    } else {
        UbSearchScope::All
    };
    Some((scope, partial))
}

/// `/ub use[li][p] [itemOne] on [itemTwo]`.
fn use_command(call: &mut UbCall<'_>, flags: &str, arguments: &str) {
    let Some((scope, partial)) = read_scope(call, flags) else {
        return;
    };
    let text = arguments.trim();
    if text.is_empty() {
        call.write_ub_bad_syntax("use");
        return;
    }
    let (first, second) = match ordinal::index_of_ignore_case(text, " on ") {
        Some(at) if at > 0 => (text[..at].trim(), Some(text[at + 4..].trim())),
        _ => (text, None),
    };
    let Some(one) = search::try_find(call.host, first, scope, partial, 0) else {
        call.write_ub_error(&format!("Could not find object: {first}"));
        return;
    };
    let Some(second) = second.filter(|second| !second.is_empty()) else {
        use_single_object(call, &one, partial);
        return;
    };
    let Some(two) = search::try_find(
        call.host,
        second,
        UbSearchScope::All,
        partial,
        one.object_id,
    ) else {
        call.write_ub(&format!("{second} is null"));
        return;
    };
    call.write_ub(&format!("using {} on {}", one.name, two.name));
    call.host.apply_item(one.object_id, two.object_id);
}

fn use_single_object(call: &mut UbCall<'_>, one: &WorldObject, partial: bool) {
    call.write_ub(&format!("using {}", one.name));
    match one.object_class {
        ObjectClass::Portal => use_portal_by_name(call, &one.name, partial),
        ObjectClass::Vendor => {
            let reply = call
                .services
                .vendor_command(call.host, &format!("open {}", one.name));
            call.post_ub_reply("vendor", reply);
        }
        ObjectClass::Container | ObjectClass::Corpse => {
            call.host.open_loot(one.object_id);
        }
        _ => {
            call.host.use_item(one.object_id);
        }
    }
}

/// `/ub select[li][p] [item]`.
fn select_command(call: &mut UbCall<'_>, flags: &str, arguments: &str) {
    let Some((scope, partial)) = read_scope(call, flags) else {
        return;
    };
    let text = arguments.trim();
    if text.is_empty() {
        call.write_ub_bad_syntax("select");
        return;
    }
    match search::try_find(call.host, text, scope, partial, 0) {
        Some(item) => {
            call.host.select(item.object_id);
        }
        None => call.write_ub_error(&format!("Could not find object: {text}")),
    }
}

/// `/ub close corpse` (or chest).
fn close_command(call: &mut UbCall<'_>, arguments: &str) {
    let requested = casing::to_lower_invariant(arguments.trim());
    if requested != "corpse" && requested != "chest" {
        call.write_ub_bad_syntax("close");
        return;
    }
    let open = call.host.open_container();
    let container = if open == 0 {
        None
    } else {
        call.host.object(open)
    };
    let Some(container) = container else {
        call.write_ub("No container is currently open.");
        return;
    };
    let matches = if requested == "corpse" {
        container.object_class == ObjectClass::Corpse
    } else {
        container.object_class == ObjectClass::Container
    };
    if !matches {
        call.write_ub_error(&format!(
            "The open container is a {:?}, not a {requested}.",
            container.object_class
        ));
        return;
    }
    call.host.close_loot(open);
}

/// `/ub swearallegiance[p]`, `/ub breakallegiance[p]`: resolves who the
/// command means and asks the client to send it. A miss names the verb as
/// typed, flags and all.
fn allegiance(call: &mut UbCall<'_>, verb: &str, name: &str, partial: bool, swear: bool) {
    let requested = name.trim();
    let Some(player) = search::try_find_nearest(call.host, requested, partial, &PLAYER_CLASSES)
    else {
        let missing = if requested.is_empty() {
            "Could not find closest player".to_owned()
        } else {
            format!("Could not find player {requested}")
        };
        call.write_ub_error(&format!("{missing} Command:{verb}"));
        return;
    };
    let who = format!("{}[0x{:08X}]", player.name, player.object_id);
    let result = if swear {
        call.host.allegiance_swear(player.object_id)
    } else {
        call.host.allegiance_break(player.object_id)
    };
    match result.status {
        AllegianceStatus::Sent => call.write_ub(&if swear {
            format!("Swearing Allegiance to {who}")
        } else {
            format!("Breaking Allegiance from {who}")
        }),
        AllegianceStatus::InvalidTarget => call.write_ub_error(&if swear {
            format!("Cannot swear allegiance to {who}.")
        } else {
            format!("{who} is not in your allegiance.")
        }),
        AllegianceStatus::Refused => {
            // A refusal is about the session, so the client's own reason is
            // what is worth printing.
            let notice = result
                .notice
                .filter(|notice| !notice.trim().is_empty())
                .unwrap_or_else(|| {
                    if swear {
                        format!("Refused to swear allegiance to {who}.")
                    } else {
                        format!("Refused to break allegiance from {who}.")
                    }
                });
            call.write_ub_error(&notice);
        }
        AllegianceStatus::Unavailable => call.write_ub_error(if swear {
            "Cannot swear allegiance right now."
        } else {
            "Cannot break allegiance right now."
        }),
    }
}

/// `/ub listvars`, `listpvars`, `listgvars`: one variable store, read whole
/// before anything is printed.
fn list_variables(call: &mut UbCall<'_>, scope: ExpressionVariableScope) {
    let captured = call.expressions.state().capture(scope);
    let mut variables = match captured {
        Ok(variables) => variables,
        Err(error) => {
            call.write_ub_error(&format!("Command failed: {}", error.message()));
            call.host.log(
                ac_plugin_abi::LogLevel::Error,
                &format!("MossTank /ub command failed. {}", error.message()),
            );
            return;
        }
    };
    variables.sort_by(|a, b| ordinal::compare_ignore_case(&a.0, &b.0));
    let heading = match scope {
        ExpressionVariableScope::Persistent => "Defined persistent variables:",
        ExpressionVariableScope::Global => "Defined global variables:",
        ExpressionVariableScope::Session => "Defined variables:",
    };
    let lines: Vec<String> = variables
        .iter()
        .map(|(name, value)| {
            format!(
                "{name} ({}) = {}",
                friendly_type_name(value),
                value_text(call, value)
            )
        })
        .collect();
    call.write_ub_message(heading, lines);
}

/// `/ub playeroption (list|<option> <on|true|off|false>)`: anything but on
/// or true turns it off, as the original reads it.
fn player_option(call: &mut UbCall<'_>, arguments: &str) {
    let text = arguments.trim();
    let options: Vec<String> = call
        .host
        .character_options()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    if ordinal::equals_ignore_case(text, "list") {
        call.write_ub(&format!("Valid values are: {}", options.join(", ")));
        return;
    }
    let parts: Vec<&str> = text.split(' ').filter(|p| !p.is_empty()).collect();
    if parts.len() != 2 {
        call.write_ub_error("Usage: /ub playeroption <option> <on/true|off/false>");
        return;
    }
    let value = casing::to_lower_invariant(parts[1]);
    let on = value == "on" || value == "true";
    let result = call.host.set_character_option(parts[0], on);
    match result.status {
        crate::host::OptionStatus::Accepted => {
            let canonical = options
                .iter()
                .find(|candidate| ordinal::equals_ignore_case(candidate, parts[0]))
                .cloned()
                .unwrap_or_else(|| parts[0].to_owned());
            call.write_ub(&format!(
                "Setting {canonical} = {}",
                if on { "True" } else { "False" }
            ));
        }
        crate::host::OptionStatus::UnknownOption => call.write_ub_error(&format!(
            "Invalid option. Valid values are: {}",
            options.join(", ")
        )),
        crate::host::OptionStatus::Unavailable => call.write_ub_error(&format!(
            "Unable to set {}: {}",
            parts[0],
            result
                .notice
                .unwrap_or_else(|| format!("{:?}", result.status))
        )),
    }
}

/// `/ub translateroute <startLandblock> <route> <endLandblock> <saveAs>
/// [force]`: copies a route onto another landblock by shifting every point
/// by the distance between the two blocks (192 m a landblock step, 240 m a
/// map unit).
fn translate_route(call: &mut UbCall<'_>, arguments: &str) {
    let parts: Vec<&str> = arguments.split(' ').filter(|p| !p.is_empty()).collect();
    if !(4..=5).contains(&parts.len())
        || (parts.len() == 5 && !ordinal::equals_ignore_case(parts[4], "force"))
    {
        call.write_ub_bad_syntax("translateroute");
        return;
    }
    let force = parts.len() == 5;
    call.write_ub_tool_debug(
        tools::VTANK,
        &format!(
            "Translating route: RouteToLoad:{} StartLandblock:{} EndLandblock:{} RouteToSaveAs:{} Force:{}",
            parts[1],
            parts[0],
            parts[2],
            parts[3],
            if force { "True" } else { "False" }
        ),
    );
    let Some(start) = parse_landblock(parts[0]) else {
        call.post_ub(&chat::tool_error(
            tools::VTANK,
            &format!(
                "Could not parse hex value from StartLandblock: {}",
                parts[0]
            ),
        ));
        return;
    };
    let Some(end) = parse_landblock(parts[2]) else {
        call.post_ub(&chat::tool_error(
            tools::VTANK,
            &format!("Could not parse hex value from EndLandblock: {}", parts[2]),
        ));
        return;
    };
    let difference = |from: u32, to: u32| (f64::from(to) - f64::from(from)) * 192.0;
    let east_west = difference(start >> 24, end >> 24) / 240.0;
    let north_south = difference((start << 8) >> 24, (end << 8) >> 24) / 240.0;
    match call.services.translate_route(
        call.host,
        parts[1],
        parts[3],
        east_west,
        north_south,
        force,
    ) {
        Err(notice) => call.post_ub(&chat::tool_error(tools::VTANK, &notice)),
        Ok((saved, records)) => call.write_ub_tool_debug(
            tools::VTANK,
            &format!(
                "Translated {records} records from {start:08X} to {end:08X} by adding offsets NS:{} EW:{}\nSaved to file: {saved}",
                invariant(north_south),
                invariant(east_west)
            ),
        ),
    }
}

/// `/ub login next[r][l] <name-or-index>`, `clear`, `list`: which character
/// the client logs in as after this one logs out. A selector that names
/// nobody clears the pick rather than leaving an earlier one standing.
fn login_command(call: &mut UbCall<'_>, arguments: &str) {
    let (head, selector) = split_head(arguments);
    let verb = casing::to_lower_invariant(&head);
    if verb == "clear" {
        clear_next_login(call);
        return;
    }
    if verb == "list" {
        print_login_roster(call);
        return;
    }
    let Some(mut flags) = verb.strip_prefix("next") else {
        call.write_ub_bad_syntax("login");
        return;
    };
    let relative = flags.starts_with('r');
    if relative {
        flags = &flags[1..];
    }
    let looping = flags.starts_with('l');
    if looping {
        flags = &flags[1..];
    }
    if !flags.is_empty() {
        call.write_ub_bad_syntax("login");
        return;
    }
    if selector.is_empty() {
        call.write_ub("Specify part of name or index: /ub login next <name|index>");
        return;
    }
    let roster = login_roster::sorted(call.host.login().roster);
    let current = call
        .host
        .character()
        .map(|info| info.name)
        .unwrap_or_default();
    match login_roster::try_resolve(&roster, &selector, &current, relative, looping) {
        Err(failure) => {
            call.write_ub(&match failure {
                LoginRosterFailure::Empty => {
                    "The account's character list has not arrived yet.".to_owned()
                }
                LoginRosterFailure::NoCurrentCharacter => {
                    "Cannot log in relative to a character that is not on the list.".to_owned()
                }
                LoginRosterFailure::OutOfRange => "Login index is out of bounds.  Use the [l]oop option to wrap around: /ub login nextl -100".to_owned(),
                LoginRosterFailure::PendingDelete => {
                    "That character is scheduled for deletion and cannot be played.".to_owned()
                }
                LoginRosterFailure::None | LoginRosterFailure::NoSuchName => format!(
                    "No character found with name {selector}.  Clearing next login."
                ),
            });
            if failure != LoginRosterFailure::Empty {
                clear_next_login(call);
            }
        }
        Ok(index) => {
            let chosen = roster[index].clone();
            if call.host.login_set_next(chosen.object_id) {
                call.write_ub(&format!(
                    "Logging in as {} next at index {index}",
                    chosen.name
                ));
            } else {
                call.write_ub_error(&format!("Could not set the next login to {}.", chosen.name));
            }
        }
    }
}

fn clear_next_login(call: &mut UbCall<'_>) {
    call.host.login_clear_next();
    call.write_ub("Next login cleared.");
}

/// The account's characters with the alphabetical index every selector
/// speaks in and the slot the account keeps each in, in the reference's
/// columns.
fn print_login_roster(call: &mut UbCall<'_>) {
    let roster = login_roster::sorted(call.host.login().roster);
    let current = call
        .host
        .character()
        .map(|info| info.name)
        .unwrap_or_default();
    let mut rows = vec![format!(
        "{:<10}{:<30}{:<20}Filter Index",
        "Index", "Name", "ID"
    )];
    for (index, character) in roster.iter().enumerate() {
        let name = if character.name == current {
            format!("**{}**", character.name)
        } else {
            character.name.clone()
        };
        rows.push(format!(
            "{index:<10}{name:<30}{:<20}{}",
            character.object_id, character.active_index
        ));
    }
    call.write_ub_message(&format!("Listing {} logins.", roster.len()), rows);
}

/// The broadcast's delay and command, or `None` having refused the delay.
fn read_delay(call: &mut UbCall<'_>, arguments: &str) -> Option<(i32, String)> {
    match network::read_delay_and_command(arguments) {
        Ok(read) => Some(read),
        Err(delay_text) => {
            call.write_ub_error(&format!(
                "Unable to broadcast command, invalid delay: {delay_text}"
            ));
            None
        }
    }
}

/// The labels this character answers to, as the settings hold them.
fn own_tags(call: &UbCall<'_>) -> Vec<String> {
    call.settings.value("Networking.Tags").items
}

/// `/ub netclients [tag]`: who is on this computer, this character included
/// (the client reports the others only).
fn net_clients(call: &mut UbCall<'_>, arguments: &str) {
    let tags = network::split_tags(arguments.trim());
    let wanted = |held: &[String]| {
        tags.is_empty()
            || tags
                .iter()
                .any(|tag| held.iter().any(|h| ordinal::equals_ignore_case(h, tag)))
    };
    let mut showed = false;
    if let Some(me) = call.host.character().filter(|info| info.id != 0) {
        let own = own_tags(call);
        if wanted(&own) {
            call.write_ub(&network::net_client_line(0, &me.name, &own));
            showed = true;
        }
    }
    let mut clients = call.host.peer_clients();
    clients.sort_by_key(|client| client.client_id);
    for client in clients {
        if !wanted(&client.tags) {
            continue;
        }
        call.write_ub(&network::net_client_line(
            client.client_id,
            &client.name,
            &client.tags,
        ));
        showed = true;
    }
    if !showed {
        call.write_ub("No net clients to show");
    }
}

//! What one `/ub` command, or one frame of the UB tools, reaches
//! ([`UbCall`]): the client, the settings, the expression runtime, the rest
//! of the plugin ([`UbServices`]) and the shared action locks; and the
//! UtilityBelt line writers every command prints through (`WriteUb`,
//! `WriteUbError`, `WriteUbMessage`, `WriteUbBadSyntax`, ...).

use mosstank_automation::locks::SharedActionLocks;
use mosstank_expr::ExpressionValue;
use mosstank_meta::host::UbLineKind;

use crate::catalog::{CatalogDisplays, UbCatalog};
use crate::chat;
use crate::expressions::UbExpressions;
use crate::help::{self, UbCommandUsage};
use crate::host::UbHost;

/// The rest of the plugin, as the UB commands reach it: what other slices
/// own (the macro's options, the vendor and fellowship tools, the route,
/// the stores) and the clock.
pub trait UbServices {
    /// The plugin's version, for the compatibility line.
    fn plugin_version(&self) -> String;
    /// Wall time, Unix milliseconds.
    fn now_utc_ms(&mut self) -> i64;
    /// The local offset from UTC, minutes.
    fn local_offset_minutes(&mut self) -> i32;
    /// A monotonic clock, milliseconds, for timing an expression.
    fn monotonic_ms(&mut self) -> f64;

    /// Whether a name is one of the macro's own options.
    fn is_vtank_option(&mut self, name: &str) -> bool;
    /// One of the macro's own options.
    fn get_meta_option(&mut self, host: &mut dyn UbHost, name: &str) -> ExpressionValue;
    /// Writes one of the macro's own options; false when refused.
    fn set_meta_option(
        &mut self,
        host: &mut dyn UbHost,
        name: &str,
        value: &ExpressionValue,
    ) -> bool;

    /// `/ub vendor <arguments>`: the lines it answers, `None` for a line it
    /// does not parse.
    fn vendor_command(&mut self, host: &mut dyn UbHost, arguments: &str) -> Option<Vec<String>>;
    /// `/ub autovendor [profile]`: starts a vendor run.
    fn auto_vendor_start(&mut self, host: &mut dyn UbHost, profile: Option<&str>);
    /// `/ub autovendor stop`.
    fn auto_vendor_stop(&mut self, host: &mut dyn UbHost);
    /// `/ub fellow <arguments>`: false for a line that does not parse (the
    /// dispatcher then says "Bad command syntax" and the help).
    fn fellow_command(&mut self, host: &mut dyn UbHost, arguments: &str) -> bool;
    /// `/ub xp <arguments>`: the lines it answers, `None` for a line it does
    /// not parse.
    fn xp_command(&mut self, host: &mut dyn UbHost, arguments: &str) -> Option<Vec<String>>;
    /// `/ub equip <arguments>`: the lines it answers, `None` for a line it
    /// does not parse.
    fn equip_command(&mut self, host: &mut dyn UbHost, arguments: &str) -> Option<Vec<String>>;

    /// A verb of a UB tool this crate does not carry yet (`give*`, `ig*`,
    /// `count`, `autostack`, `autocram`, `clearbugged`, `autotinker`,
    /// `getjob`, `tinkcalc`), with its arguments: the plugin runs it.
    fn run_tool_verb(&mut self, host: &mut dyn UbHost, verb: &str, arguments: &str);

    /// Aims the follow route at a player; false when it could not.
    fn follow(&mut self, host: &mut dyn UbHost, player_id: u32, name: &str) -> bool;
    /// Whether the route is switched on.
    fn navigation_enabled(&mut self) -> bool;
    /// How many times a jump is charged before it gives up.
    fn jump_attempts(&mut self) -> i32;
    /// Copies a route onto another landblock, shifting every point by the
    /// map units given; the saved file's name and the number of records, or
    /// why not.
    ///
    /// # Errors
    /// The route could not be read, or the output exists and `force` is
    /// off, or it could not be written.
    #[allow(clippy::too_many_arguments)]
    fn translate_route(
        &mut self,
        host: &mut dyn UbHost,
        from: &str,
        to: &str,
        east_west: f64,
        north_south: f64,
        force: bool,
    ) -> Result<(String, i32), String>;
}

/// One call into the UB tab.
pub struct UbCall<'a> {
    /// The client.
    pub host: &'a mut dyn UbHost,
    /// The UB settings.
    pub settings: &'a mut dyn UbCatalog,
    /// The expression runtime.
    pub expressions: &'a mut dyn UbExpressions,
    /// The rest of the plugin.
    pub services: &'a mut dyn UbServices,
    /// The action locks the macro's rules share, borrowed for each use: a
    /// command's expression or service may reach them too.
    pub locks: &'a SharedActionLocks,
}

impl std::fmt::Debug for UbCall<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UbCall")
    }
}

impl UbCall<'_> {
    /// Whether the UtilityBelt side prints its debug lines.
    pub fn ub_debug(&self) -> bool {
        self.settings.ub_debug()
    }

    /// A switch in the settings.
    pub fn flag(&self, name: &str) -> bool {
        self.settings.flag(name)
    }

    /// A finished UtilityBelt line, in its kind's text class (`PostUb`).
    pub fn post_ub(&mut self, line: &str) {
        chat::post(self.host, &CatalogDisplays(&*self.settings), line);
    }

    /// A plain UtilityBelt line: `[UB] ` and the text (`WriteUb`).
    pub fn write_ub(&mut self, text: &str) {
        self.post_ub(&chat::line(text));
    }

    /// A UtilityBelt error: `[UB] Error: ` and the text (`WriteUbError`).
    pub fn write_ub_error(&mut self, text: &str) {
        self.post_ub(&chat::error(text));
    }

    /// An expression line (`UbChat.PostExpression`).
    pub fn post_expression(&mut self, text: &str) {
        chat::post_expression(self.host, &CatalogDisplays(&*self.settings), text);
    }

    /// A plain message of several lines (`WriteUbMessage`).
    pub fn write_ub_message<I, S>(&mut self, heading: &str, continuation: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        chat::post_message(
            self.host,
            &CatalogDisplays(&*self.settings),
            UbLineKind::Generic,
            heading,
            continuation,
        );
    }

    /// An error of several lines (`WriteUbErrorMessage`).
    pub fn write_ub_error_message<I, S>(&mut self, heading: &str, continuation: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        chat::post_message(
            self.host,
            &CatalogDisplays(&*self.settings),
            UbLineKind::Error,
            heading,
            continuation,
        );
    }

    /// A debug line, printed only while debug is on (`WriteUbDebug`).
    pub fn write_ub_debug(&mut self, text: &str) {
        let debug = self.ub_debug();
        chat::post_debug(self.host, &CatalogDisplays(&*self.settings), debug, text);
    }

    /// A tool's debug line (`WriteUbToolDebug`).
    pub fn write_ub_tool_debug(&mut self, tool: &str, text: &str) {
        let debug = self.ub_debug();
        chat::post_tool_debug(
            self.host,
            &CatalogDisplays(&*self.settings),
            debug,
            tool,
            text,
        );
    }

    /// A line of the macro's own: `[MossTank] ` and the text (`WriteVtank`).
    pub fn write_vtank(&mut self, text: &str) {
        mosstank_meta::chat::post_mosstank(self.host, text);
    }

    /// A line from a tool that writes both kinds: one under the UtilityBelt
    /// tag in its kind's class, anything else as the macro's own line
    /// (`WriteTranscriptLine`).
    pub fn write_transcript_line(&mut self, line: &str) {
        if line.starts_with(chat::TAG) {
            self.post_ub(line);
        } else {
            self.write_vtank(line);
        }
    }

    /// Says something to yourself (`Think`).
    pub fn think(&mut self, text: &str) -> bool {
        chat::think(self.host, text)
    }

    /// What `/ub` answers when a command's arguments do not parse: the
    /// error, then the command's full help (`WriteUbBadSyntax`).
    ///
    /// # Panics
    /// The verb has no usage line: the verbs are this crate's constants.
    pub fn write_ub_bad_syntax(&mut self, verb: &str) {
        self.write_ub_error("Bad command syntax");
        self.write_ub_command_help(help::require(verb));
    }

    /// The full help for one command, as one message: the usage, the
    /// description, then "Examples:" and each example indented by one and
    /// two spaces (`WriteUbCommandHelp`).
    pub fn write_ub_command_help(&mut self, usage: &UbCommandUsage) {
        let mut lines = vec![
            format!("Description: {}", usage.summary),
            "Examples:".to_owned(),
        ];
        for example in usage.examples {
            lines.push(format!(" {}", example.command));
            lines.push(format!("  {}", example.description));
        }
        self.write_ub_message(&format!("Usage: {}", usage.usage), lines);
    }

    /// Prints what a tool's command answered, line by line; `None` is a
    /// line the tool could not parse, which gets the bad-syntax answer
    /// (`PostUbReply`).
    pub fn post_ub_reply(&mut self, verb: &str, reply: Option<Vec<String>>) {
        match reply {
            None => self.write_ub_bad_syntax(verb),
            Some(lines) => {
                for line in lines {
                    self.post_ub(&line);
                }
            }
        }
    }
}

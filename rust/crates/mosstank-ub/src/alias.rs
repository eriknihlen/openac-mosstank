//! The alias list (`AliasTable`, `AliasController`): one line per alias,
//! matched against every line the player types, deciding whether the line
//! passes, is rewritten, or is swallowed, and which actions run for it.
//!
//! The line format is `[eat] pattern SEPARATOR action`:
//!
//! - `pattern = text` sends the text through the chat bar;
//! - `pattern => expression` evaluates the expression and sends the result;
//! - `pattern -> expression` evaluates the expression and sends nothing.
//!
//! The separators are spaced, so a pattern may hold `=` or `>` on its own
//! (a lookahead does), and the earliest one on the line splits it, so the
//! action may hold any of them. A leading `[eat]` swallows the typed line.
//! Without it, an alias that has something to send sends it IN PLACE of the
//! typed line: that is what lets a marker in the middle of a sentence be
//! replaced before the sentence goes out. With it, the typed line goes
//! nowhere and what the aliases say goes out on its own.

use std::collections::HashSet;

use ac_plugin_abi::LogLevel;
use ac_plugin_abi::callback::InterceptDecision;
use mosstank_expr::{ExpressionValue, ExpressionVariableScope};
use mosstank_meta::host::MetaHost;
use openac_plugin_compat::regex::{DotNetRegex, RegexError, RegexOptions};
use openac_plugin_compat::text::ordinal;

use crate::expressions::UbExpressions;
use crate::host::UbHost;

/// What an alias does once its pattern matches a typed line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasActionKind {
    /// The text is sent through the chat bar as it stands.
    ChatCommand,
    /// The text is evaluated as an expression and the result is sent.
    ChatExpression,
    /// The text is evaluated as an expression and the result is dropped.
    Expression,
}

/// What the table decided for one typed line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasOutcome {
    /// The line goes out as typed.
    Pass,
    /// The line is replaced by [`AliasResolution::rewrite`].
    Rewrite,
    /// The line is swallowed.
    Suppress,
}

/// The two things an alias action needs from the plugin: somewhere to put
/// what the pattern captured, and something to evaluate an expression.
pub trait AliasActionHost {
    /// Stores one capture group as a session variable; `None` clears it,
    /// for a group the pattern has but this line did not fill.
    fn set_capture(&mut self, name: &str, value: Option<&str>);
    /// Evaluates one expression and gives its text.
    ///
    /// # Errors
    /// The expression failed, with the reason.
    fn evaluate(&mut self, expression: &str) -> Result<String, String>;
}

/// The flag that swallows the typed line, at the start of the line.
pub const EAT_FLAG: &str = "[eat]";

const CHAT_COMMAND_SEPARATOR: &str = " = ";
const CHAT_EXPRESSION_SEPARATOR: &str = " => ";
const EXPRESSION_SEPARATOR: &str = " -> ";

/// The name a capture is stored under, before the group's name; the group
/// named `0` is the whole match.
const CAPTURE_PREFIX: &str = "capturegroup_";

/// One alias: a pattern, what to do when it matches, and whether the typed
/// line survives.
#[derive(Debug, Clone)]
pub struct Alias {
    /// The regular expression, as written.
    pub pattern: String,
    /// What happens on a match.
    pub kind: AliasActionKind,
    /// The command or the expression, as written.
    pub text: String,
    /// Whether the typed line is swallowed on a match.
    pub eat: bool,
    regex: DotNetRegex,
}

impl Alias {
    /// The compiled pattern; case does not matter. A pattern that runs away
    /// is cut off by the kit's step limit rather than freezing the chat bar
    /// (.NET: a 100 ms match timeout; register COMPAT-5).
    fn new(
        pattern: &str,
        kind: AliasActionKind,
        text: &str,
        eat: bool,
    ) -> Result<Self, RegexError> {
        let regex = DotNetRegex::new(
            pattern,
            RegexOptions::IGNORE_CASE | RegexOptions::CULTURE_INVARIANT,
        )?;
        Ok(Self {
            pattern: pattern.to_owned(),
            kind,
            text: text.to_owned(),
            eat,
            regex,
        })
    }

    /// The line this alias is written as; parsing it gives this alias back.
    #[must_use]
    pub fn to_line(&self) -> String {
        format!(
            "{}{}{}{}",
            if self.eat {
                format!("{EAT_FLAG} ")
            } else {
                String::new()
            },
            self.pattern,
            separator_of(self.kind),
            self.text
        )
    }
}

/// The separator a kind of alias is written with.
#[must_use]
pub fn separator_of(kind: AliasActionKind) -> &'static str {
    match kind {
        AliasActionKind::ChatCommand => CHAT_COMMAND_SEPARATOR,
        AliasActionKind::ChatExpression => CHAT_EXPRESSION_SEPARATOR,
        AliasActionKind::Expression => EXPRESSION_SEPARATOR,
    }
}

/// What the table decided for one typed line, and what has to go out for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasResolution {
    /// What becomes of the typed line.
    pub outcome: AliasOutcome,
    /// The line that goes out in place of the typed one, when the outcome
    /// says so.
    pub rewrite: Option<String>,
    /// Lines that go out on their own, after the typed line's fate is
    /// settled.
    pub submit: Vec<String>,
    /// An action that failed, one line each, for whoever wrote the alias.
    pub problems: Vec<String>,
}

impl AliasResolution {
    fn passed() -> Self {
        Self {
            outcome: AliasOutcome::Pass,
            rewrite: None,
            submit: Vec::new(),
            problems: Vec::new(),
        }
    }
}

/// The alias list, read from and written to the list setting one line per
/// alias. Every typed line is held against every alias; each one that
/// matches stores its named groups as session variables called
/// `capturegroup_<name>` and runs its action.
#[derive(Debug, Clone, Default)]
pub struct AliasTable {
    /// The aliases that parsed, in list order.
    pub aliases: Vec<Alias>,
    /// The lines that did not parse, numbered from one, with why.
    pub problems: Vec<String>,
}

impl AliasTable {
    /// A table with nothing in it, which passes every line.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Reads the list setting. A line that does not parse is reported by
    /// its number and left out; the lines around it still count.
    pub fn parse<I, S>(lines: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut table = Self::default();
        for (index, line) in lines.into_iter().enumerate() {
            match Self::try_parse_line(line.as_ref()) {
                Ok(alias) => table.aliases.push(alias),
                Err(problem) => table
                    .problems
                    .push(format!("Line {}: {problem}", index + 1)),
            }
        }
        table
    }

    /// Reads one line of the list.
    ///
    /// # Errors
    /// The line is not an alias, with the reason.
    pub fn try_parse_line(line: &str) -> Result<Alias, String> {
        let mut text = line.trim();
        let mut eat = false;
        if ordinal::starts_with_ignore_case(text, EAT_FLAG) {
            eat = true;
            text = text[EAT_FLAG.len()..].trim_start();
        }

        let mut found: Option<(usize, AliasActionKind, &str)> = None;
        for (candidate, kind) in [
            (CHAT_COMMAND_SEPARATOR, AliasActionKind::ChatCommand),
            (CHAT_EXPRESSION_SEPARATOR, AliasActionKind::ChatExpression),
            (EXPRESSION_SEPARATOR, AliasActionKind::Expression),
        ] {
            if let Some(index) = text.find(candidate)
                && found.is_none_or(|(at, _, _)| index < at)
            {
                found = Some((index, kind, candidate));
            }
        }
        // The trailing-separator case reads as a pattern with nothing after
        // it, which is the same complaint.
        let Some((at, kind, separator)) = found else {
            return Err("An alias is 'pattern = command', 'pattern => expression' \
                        or 'pattern -> expression', with the spaces."
                .to_owned());
        };
        let pattern = text[..at].trim();
        let action = text[at + separator.len()..].trim();
        if pattern.is_empty() {
            return Err("The pattern before the separator is empty.".to_owned());
        }
        if action.is_empty() {
            return Err("There is nothing after the separator to run.".to_owned());
        }
        Alias::new(pattern, kind, action, eat)
            .map_err(|error| format!("'{pattern}' is not a regular expression: {error}"))
    }

    /// Holds one typed line against every alias, in list order. Each match
    /// stores its groups first and runs its action second, because the
    /// action reads them. Then the line's fate: one eater swallows it for
    /// all of them; otherwise the first alias with something to send
    /// replaces it and the rest go out after; otherwise it passes.
    pub fn resolve(&self, line: &str, host: &mut dyn AliasActionHost) -> AliasResolution {
        if self.aliases.is_empty() {
            return AliasResolution::passed();
        }
        let mut eat = false;
        let mut outputs: Vec<String> = Vec::new();
        let mut problems: Vec<String> = Vec::new();
        for alias in &self.aliases {
            let found = match alias.regex.find(line) {
                Ok(found) => found,
                Err(RegexError::StepLimitExceeded) => {
                    problems.push(format!(
                        "Alias '{}' took too long to match and was skipped.",
                        alias.pattern
                    ));
                    continue;
                }
                Err(error) => {
                    problems.push(format!(
                        "Alias '{}' could not be matched and was skipped: {error}",
                        alias.pattern
                    ));
                    continue;
                }
            };
            let Some(found) = found else {
                continue;
            };

            eat |= alias.eat;
            for name in alias.regex.group_names() {
                let group = found.group(name);
                host.set_capture(
                    &format!("{CAPTURE_PREFIX}{name}"),
                    group.success.then_some(group.value),
                );
            }

            let output = match alias.kind {
                AliasActionKind::ChatCommand => Ok(Some(alias.text.clone())),
                AliasActionKind::ChatExpression => host.evaluate(&alias.text).map(Some),
                AliasActionKind::Expression => host.evaluate(&alias.text).map(|_| None),
            };
            match output {
                Ok(Some(output)) if !output.trim().is_empty() => outputs.push(output),
                Ok(_) => {}
                Err(error) => problems.push(format!(
                    "Alias '{}' failed to run '{}': {error}",
                    alias.pattern, alias.text
                )),
            }
        }

        if eat {
            return AliasResolution {
                outcome: AliasOutcome::Suppress,
                rewrite: None,
                submit: outputs,
                problems,
            };
        }
        if !outputs.is_empty() {
            let rewrite = outputs.remove(0);
            return AliasResolution {
                outcome: AliasOutcome::Rewrite,
                rewrite: Some(rewrite),
                submit: outputs,
                problems,
            };
        }
        AliasResolution {
            problems,
            ..AliasResolution::passed()
        }
    }
}

/// The alias actions over the expression runtime: the captures are the same
/// session variables a meta chat capture fills, written the same way, so an
/// alias expression reads them as a meta does.
pub struct RuntimeAliasActions<'a> {
    /// The client.
    pub host: &'a mut dyn MetaHost,
    /// The runtime.
    pub expressions: &'a mut dyn UbExpressions,
}

impl std::fmt::Debug for RuntimeAliasActions<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RuntimeAliasActions")
    }
}

impl AliasActionHost for RuntimeAliasActions<'_> {
    fn set_capture(&mut self, name: &str, value: Option<&str>) {
        // A session variable is never stored anywhere that can refuse it.
        let state = self.expressions.state();
        let _ = match value {
            None => state
                .clear(ExpressionVariableScope::Session, name)
                .map(|_| ()),
            Some(value) => state
                .set(
                    ExpressionVariableScope::Session,
                    name,
                    ExpressionValue::string(value),
                )
                .map(|_| ()),
        };
    }

    fn evaluate(&mut self, expression: &str) -> Result<String, String> {
        let value = self
            .expressions
            .evaluate(self.host, expression)
            .map_err(|error| error.message())?;
        Ok(self.expressions.display(self.host, &value))
    }
}

/// Puts the alias list between the chat bar and the world: one interceptor
/// holds every typed line against the list, stores what the patterns
/// captured where the expressions read them, and runs the actions through
/// the expression runtime and the chat bar.
///
/// An action's output is let through untouched when it comes back past the
/// interceptor: an alias whose output matched itself would otherwise call
/// itself without end. A rewrite is different; the client feeds it back
/// through the interceptor, so one alias may rewrite into another. That
/// chain is watched here: the moment it produces a line it has produced
/// before, the line is let through as it stands and the player is told once.
///
/// The list and the switch are read live, on every line; the list is
/// parsed again only when its lines change, and a line that does not parse
/// is reported once per change.
#[derive(Debug, Default)]
pub struct AliasController {
    registration: Option<u32>,
    parsed_lines: Vec<String>,
    table: AliasTable,
    rewrite_chain: HashSet<String>,
    last_rewrite: Option<String>,
    /// What the aliases sent on their own and the client has not yet handed
    /// back past the interceptor (register UB-1).
    dispatched: Vec<String>,
    disposed: bool,
}

impl AliasController {
    /// A controller with no interceptor yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Installs the one interceptor; a second call is a no-op. The handle,
    /// or why the client refused.
    ///
    /// # Errors
    /// The client refused the interceptor.
    pub fn attach(&mut self, host: &mut dyn UbHost) -> Result<u32, String> {
        if let Some(handle) = self.registration {
            return Ok(handle);
        }
        let handle = host.chat_add_interceptor()?;
        self.registration = Some(handle);
        Ok(handle)
    }

    /// The interceptor's handle, once attached.
    #[must_use]
    pub fn registration(&self) -> Option<u32> {
        self.registration
    }

    /// Removes the interceptor; the controller passes every line after.
    pub fn dispose(&mut self, host: &mut dyn UbHost) {
        if self.disposed {
            return;
        }
        self.disposed = true;
        if let Some(handle) = self.registration.take() {
            host.chat_remove_interceptor(handle);
        }
    }

    /// The list as last parsed, for whoever wants to show it.
    pub fn table(&mut self, host: &mut dyn MetaHost, lines: &[String]) -> &AliasTable {
        self.current_table(host, lines)
    }

    /// The client asks about one typed line. `enabled` and `lines` are the
    /// switch and the list as the settings hold them now.
    pub fn intercept(
        &mut self,
        host: &mut dyn MetaHost,
        expressions: &mut dyn UbExpressions,
        enabled: bool,
        lines: &[String],
        line: &str,
    ) -> InterceptDecision {
        if let Some(index) = self.dispatched.iter().position(|sent| sent == line) {
            self.dispatched.remove(index);
            return InterceptDecision::Pass;
        }
        if self.disposed || !enabled {
            return InterceptDecision::Pass;
        }
        if self.current_table(host, lines).aliases.is_empty() {
            return InterceptDecision::Pass;
        }

        // A line that is the rewrite just handed back is the next lap of the
        // same chain; anything else starts a chain of its own.
        if self.last_rewrite.as_deref() != Some(line) {
            self.rewrite_chain.clear();
            self.rewrite_chain.insert(line.to_owned());
        }
        self.last_rewrite = None;

        let result = self.table.resolve(
            line,
            &mut RuntimeAliasActions {
                host: &mut *host,
                expressions,
            },
        );
        for problem in &result.problems {
            host.log(LogLevel::Error, problem);
        }

        match result.outcome {
            AliasOutcome::Suppress => {
                self.dispatch(host, result.submit);
                InterceptDecision::Suppress
            }
            AliasOutcome::Rewrite => {
                let rewrite = result.rewrite.unwrap_or_default();
                if !self.rewrite_chain.insert(rewrite.clone()) {
                    let culprit = self.culprit(&rewrite);
                    host.log(
                        LogLevel::Warn,
                        &format!(
                            "An alias rewrote '{line}' into '{rewrite}', which the same \
                             chain of aliases has already produced; it is sent as it \
                             stands. Check the alias list for '{culprit}'."
                        ),
                    );
                    self.dispatch(host, result.submit);
                    return InterceptDecision::Pass;
                }
                self.last_rewrite = Some(rewrite.clone());
                self.dispatch(host, result.submit);
                InterceptDecision::Rewrite(rewrite)
            }
            AliasOutcome::Pass => InterceptDecision::Pass,
        }
    }

    /// Sends the outputs that go out on their own. They come back past the
    /// interceptor after the call; they are remembered so they pass then.
    fn dispatch(&mut self, host: &mut dyn MetaHost, outputs: Vec<String>) {
        for output in outputs {
            let sent = output.trim().to_owned();
            if host.chat_submit(&output) && !sent.is_empty() {
                self.dispatched.push(sent);
            }
        }
    }

    fn current_table(&mut self, host: &mut dyn MetaHost, lines: &[String]) -> &AliasTable {
        if self.parsed_lines.as_slice() != lines {
            self.parsed_lines = lines.to_vec();
            self.table = AliasTable::parse(&self.parsed_lines);
            for problem in &self.table.problems {
                host.log(LogLevel::Warn, &format!("Alias list, {problem}"));
            }
        }
        &self.table
    }

    /// The first alias that matches the line the chain came back to.
    fn culprit(&self, line: &str) -> String {
        self.table
            .aliases
            .iter()
            .find(|alias| alias.regex.is_match(line).unwrap_or(false))
            .map_or_else(|| line.to_owned(), |alias| alias.pattern.clone())
    }
}

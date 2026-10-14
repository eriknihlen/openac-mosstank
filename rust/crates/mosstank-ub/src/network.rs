//! The other clients on this computer, as the UB tab reaches them: the
//! peer features a meta needs (`PeerFeatureSubscriptions`,
//! `MetaPeerRequirements`), and the grammar of the broadcast commands
//! (`MossTankCommands.Network`): leading digits are the delay, the rest of
//! the line is the command, and a tag list is comma separated with quotes
//! around any tag holding a space.

use std::sync::LazyLock;

use mosstank_formats::meta::{
    MetaAction, MetaActionKind, MetaCondition, MetaConditionKind, MetaProfile,
};
use openac_plugin_compat::regex::{DotNetRegex, RegexOptions};

use crate::host::UbHost;

/// What a lease asks the other clients to share (`PluginPeerCapabilities`).
pub mod capabilities {
    /// Nothing.
    pub const NONE: u32 = 0;
    /// Their state (`netclients[]`).
    pub const CLIENT_STATE: u32 = 1;
    /// Their casts (`netcasts[]`).
    pub const CASTS: u32 = 2;
    /// Their broadcast commands.
    pub const COMMANDS: u32 = 4;
}

/// The peer features this plugin has asked for, held as one lease that is
/// replaced when what is needed changes (`PeerFeatureSubscriptions`).
#[derive(Debug, Default)]
pub struct PeerFeatureSubscriptions {
    lease: Option<u32>,
    current: u32,
}

impl PeerFeatureSubscriptions {
    /// Nothing asked for.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// What is asked for now.
    #[must_use]
    pub fn current(&self) -> u32 {
        self.current
    }

    /// Asks for `requested` and nothing else: the new lease is taken before
    /// the old one is given back, so a feature both need never lapses.
    pub fn update(&mut self, host: &mut dyn UbHost, requested: u32) {
        if requested == self.current {
            return;
        }
        let next = if requested == capabilities::NONE {
            None
        } else {
            host.peer_subscribe(requested)
        };
        if let Some(old) = self.lease.take() {
            host.peer_unsubscribe(old);
        }
        self.lease = next;
        self.current = requested;
    }

    /// Whether a lease is held: the host shares over a hub (one with none
    /// hands out no lease).
    #[must_use]
    pub fn holds_lease(&self) -> bool {
        self.lease.is_some()
    }

    /// Gives the lease back.
    pub fn dispose(&mut self, host: &mut dyn UbHost) {
        if let Some(old) = self.lease.take() {
            host.peer_unsubscribe(old);
        }
        self.current = capabilities::NONE;
    }
}

static NETCLIENTS: LazyLock<DotNetRegex> = LazyLock::new(|| peer_call(r"\bnetclients\s*\["));
static NETCASTS: LazyLock<DotNetRegex> = LazyLock::new(|| peer_call(r"\bnetcasts\s*\["));

fn peer_call(pattern: &str) -> DotNetRegex {
    DotNetRegex::new(
        pattern,
        RegexOptions::IGNORE_CASE | RegexOptions::CULTURE_INVARIANT,
    )
    .expect("a fixed pattern the kit reproduces")
}

/// The peer features an enabled rule of a meta calls for
/// (`MetaPeerRequirements.Detect`): a call to `netclients[]` asks for the
/// clients' state, a call to `netcasts[]` for their casts; a word that only
/// looks like one (`mynetclients[]`) asks for nothing.
#[must_use]
pub fn meta_peer_requirements(profile: &MetaProfile) -> u32 {
    profile
        .rules
        .iter()
        .filter(|rule| rule.enabled)
        .fold(capabilities::NONE, |flags, rule| {
            flags | condition_needs(&rule.condition) | action_needs(&rule.action)
        })
}

fn text_needs(text: &str) -> u32 {
    let mut flags = capabilities::NONE;
    // Both calls begin `netc`, whose letters match only themselves in
    // either case: a text without it needs neither pattern run (a large
    // meta has thousands of expressions, and the patterns are costly).
    if !text
        .as_bytes()
        .windows(4)
        .any(|window| window.eq_ignore_ascii_case(b"netc"))
    {
        return flags;
    }
    if NETCLIENTS.is_match(text).unwrap_or(false) {
        flags |= capabilities::CLIENT_STATE;
    }
    if NETCASTS.is_match(text).unwrap_or(false) {
        flags |= capabilities::CASTS;
    }
    flags
}

fn condition_needs(condition: &MetaCondition) -> u32 {
    let own = if condition.kind == MetaConditionKind::Expression {
        text_needs(&condition.text)
    } else {
        capabilities::NONE
    };
    condition
        .children
        .iter()
        .fold(own, |flags, child| flags | condition_needs(child))
}

fn action_needs(action: &MetaAction) -> u32 {
    let own = if matches!(
        action.kind,
        MetaActionKind::ExpressionAction | MetaActionKind::ChatExpression
    ) {
        text_needs(&action.text)
    } else {
        capabilities::NONE
    };
    action
        .children
        .iter()
        .fold(own, |flags, child| flags | action_needs(child))
}

/// A broadcast's delay and command: the leading digits are the delay in
/// milliseconds and the rest of the line, less one space, is the command.
///
/// # Errors
/// The digits are not a number this client can hold; the digits, for the
/// refusal.
pub fn read_delay_and_command(arguments: &str) -> Result<(i32, String), String> {
    let digits = arguments.bytes().take_while(u8::is_ascii_digit).count();
    let delay_text = &arguments[..digits];
    let mut command = &arguments[digits..];
    if let Some(rest) = command.strip_prefix(' ') {
        command = rest;
    }
    if delay_text.is_empty() {
        return Ok((0, command.to_owned()));
    }
    match mosstank_dotnet::try_parse_int(delay_text) {
        Some(delay) => Ok((delay, command.to_owned())),
        None => Err(delay_text.to_owned()),
    }
}

/// Splits a leading comma-separated tag list off the front of a line. A tag
/// holding a space is written in quotes, so the list ends at the first space
/// outside a pair of them. `None` when the line carries no space at all.
#[must_use]
pub fn split_tag_list(arguments: &str) -> Option<(Vec<String>, String)> {
    let mut quoted = false;
    let mut end = None;
    for (index, letter) in arguments.char_indices() {
        if letter == '"' {
            quoted = !quoted;
        } else if letter == ' ' && !quoted {
            end = Some(index);
            break;
        }
    }
    let end = end?;
    Some((
        split_tags(&arguments[..end]),
        arguments[end + 1..].to_owned(),
    ))
}

/// The tags in one list, unquoted, with the empty ones dropped.
#[must_use]
pub fn split_tags(list: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let mut quoted = false;
    let mut start = 0;
    let bytes = list.as_bytes();
    for index in 0..=list.len() {
        if index < list.len() {
            let letter = bytes[index];
            if letter == b'"' {
                quoted = !quoted;
            }
            if letter != b',' || quoted {
                continue;
            }
        }
        let piece = list[start..index].trim().trim_matches('"').trim();
        start = index + 1;
        if !piece.is_empty() {
            tags.push(piece.to_owned());
        }
    }
    tags
}

/// One client as `/ub netclients` prints it: its id, its name, and its
/// tags when it has any.
#[must_use]
pub fn net_client_line(client_id: u32, name: &str, tags: &[String]) -> String {
    if tags.is_empty() {
        format!("ClientData<{client_id}, {name}>")
    } else {
        format!("ClientData<{client_id}, {name}> [{}]", tags.join(", "))
    }
}

/// Hands the client the labels this character answers to
/// (`PublishNetworkTags`), so the other clients see it under them and a
/// tagged broadcast reaches it; pushed when the row changes, not every
/// frame.
#[derive(Debug, Default)]
pub struct TagPublisher {
    published: Option<Vec<String>>,
}

impl TagPublisher {
    /// Nothing pushed yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pushes the `Networking.Tags` row when it differs from what was last
    /// pushed. `on_load` is the push made while the tab is being built: an
    /// empty row is not pushed then, since the client may have been started
    /// with labels of its own; every later push is a change, so an emptied
    /// row does reach the client.
    pub fn publish(&mut self, host: &mut dyn UbHost, tags: Vec<String>, on_load: bool) {
        if self.published.as_ref() == Some(&tags) {
            return;
        }
        let pushed = tags;
        self.published = Some(pushed.clone());
        if on_load && pushed.is_empty() {
            return;
        }
        host.set_peer_tags(&pushed);
    }
}

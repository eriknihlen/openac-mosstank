//! The `.af` text form of metas and routes (the format metaf created).
//!
//! Written from the format's documentation (metaf's reference: data types,
//! keywords, node shapes), the fixture files and the C# plugin's behaviour
//! observed as a black box (`tests/golden`), not from the C# reader or
//! writer; see `NOTICE.md`.
//!
//! A meta file is states of rules, then the routes its `EmbedNav` actions
//! name (tabs shown as four spaces):
//!
//! ```text
//! STATE: {Default} ~~ {
//!     IF:    All
//!             Expr {getvar[x]==1}
//!             Not Death
//!         DO:    DoAll
//!                 Chat {/vt nav load route}
//!                 EmbedNav nav0__route {route}
//! ~~ }
//! NAV: nav0__route once ~~ {
//!     pnt 59.5 -28.4 0.05
//! ~~ }
//! ```
//!
//! `~~` starts a comment to the end of the line, everywhere (even inside a
//! string). Strings are in braces, with braces inside doubled. Nesting is
//! by tabs: an `IF:` condition is at depth 2, a `DO:` action at depth 3,
//! and what an `All`, `Any` or `DoAll` holds sits one tab deeper on the
//! lines below; a `Not` shares its operand's line and adds no depth.

use mosstank_dotnet::compat::number::to_string_f64;
use mosstank_dotnet::{checked_double_to_int, parse_bool, round_to_int_checked};

use crate::meta::{
    MetaAction, MetaActionKind, MetaCondition, MetaConditionKind, MetaProfile, MetaRule,
};
use crate::nav::jump_direction_code;
use crate::route::{
    NavigationSettings, RouteJumpDirection, RouteMode, RouteWaypoint, RouteWaypointType,
    SpellLookup, map_position,
};

/// The header a meta file starts with: metaf's credit block and keyword
/// lists, kept as metaf writes it (bare line feeds), then a blank line.
pub const META_HEADER: &str = include_str!("af/meta_header.txt");

/// The header a route file starts with.
pub const NAV_HEADER: &str = include_str!("af/nav_header.txt");

const SEPARATOR: &str =
    "~~========================= ONLY NAVS APPEAR BELOW THIS LINE =========================~~";

// ---------------------------------------------------------------------
// Lines
// ---------------------------------------------------------------------

/// One line that holds something: its number, its leading tabs, and the
/// rest with the comment and trailing white space taken off.
#[derive(Debug, Clone, Copy)]
struct Line<'a> {
    number: usize,
    tabs: usize,
    body: &'a str,
}

impl Line<'_> {
    fn is_block_start(&self) -> bool {
        self.body.starts_with("STATE:") || self.body.starts_with("NAV:")
    }

    fn is_keyword(&self) -> bool {
        self.is_block_start() || self.body.starts_with("IF:") || self.body.starts_with("DO:")
    }
}

fn split_lines(text: &str) -> (Vec<Line<'_>>, usize) {
    let mut lines = Vec::new();
    let mut count = 0;
    for (index, raw) in text.split('\n').enumerate() {
        count = index + 1;
        let without_comment = raw.find("~~").map_or(raw, |at| &raw[..at]);
        let trimmed = without_comment.trim_end();
        let body = trimmed.trim_start_matches('\t');
        if body.is_empty() {
            continue;
        }
        lines.push(Line {
            number: index + 1,
            tabs: trimmed.len() - body.len(),
            body,
        });
    }
    (lines, count)
}

fn at(line: usize, message: &str) -> String {
    format!("line {line}: {message}")
}

// ---------------------------------------------------------------------
// Arguments
// ---------------------------------------------------------------------

/// One argument as written.
#[derive(Debug, Clone, PartialEq)]
enum Arg {
    Int(i32),
    Double(f64),
    Hex(u32),
    Text(String),
    Literal(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// `i`: an optionally signed whole number.
    Int,
    /// `d`: an optionally signed number with an optional fraction and an
    /// optional signed exponent.
    Double,
    /// A pause or a jump charge: `d` without an exponent.
    Decimal,
    /// `h`: eight upper-case hexadecimal digits.
    Hex,
    /// `s`: a braced string.
    Text,
    /// `l`: a literal (letter or underscore, then letters, digits,
    /// underscores).
    Literal,
}

/// Reads `rest` (everything after the keyword) as `shapes`, each argument
/// after white space; `None` when it does not match.
fn parse_args(rest: &str, shapes: &[Shape]) -> Option<Vec<Arg>> {
    let mut args = Vec::with_capacity(shapes.len());
    let mut rest = rest;
    for shape in shapes {
        let trimmed = rest.trim_start_matches([' ', '\t']);
        if trimmed.len() == rest.len() {
            return None;
        }
        let (arg, after) = parse_arg(trimmed, *shape)?;
        args.push(arg);
        rest = after;
    }
    rest.trim().is_empty().then_some(args)
}

fn take_token(text: &str) -> (&str, &str) {
    let end = text.find([' ', '\t']).unwrap_or(text.len());
    text.split_at(end)
}

fn parse_arg(text: &str, shape: Shape) -> Option<(Arg, &str)> {
    match shape {
        Shape::Text => parse_string(text).map(|(value, rest)| (Arg::Text(value), rest)),
        Shape::Int => {
            let (token, rest) = take_token(text);
            let digits = token.strip_prefix(['+', '-']).unwrap_or(token);
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let value = mosstank_dotnet::try_parse_int(token)?;
            Some((Arg::Int(value), rest))
        }
        Shape::Double | Shape::Decimal => {
            let (token, rest) = take_token(text);
            let mut unsigned = token.strip_prefix(['+', '-']).unwrap_or(token);
            if shape == Shape::Double
                && let Some(at) = unsigned.find(['e', 'E'])
            {
                // An exponent needs its sign: `2.08E-05`, never `1e5`.
                let exponent = &unsigned[at + 1..];
                let digits = exponent.strip_prefix(['+', '-'])?;
                if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                unsigned = &unsigned[..at];
            }
            let (whole, fraction) = match unsigned.split_once('.') {
                Some((whole, fraction)) => (whole, Some(fraction)),
                None => (unsigned, None),
            };
            let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
            // `.5` reads, `5.` does not.
            let whole_ok = digits(whole) || (whole.is_empty() && fraction.is_some());
            if !whole_ok || fraction.is_some_and(|fraction| !digits(fraction)) {
                return None;
            }
            let value = mosstank_dotnet::parse_double(token).ok()?;
            Some((Arg::Double(value), rest))
        }
        Shape::Hex => {
            let (token, rest) = take_token(text);
            if token.len() != 8
                || !token
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
            {
                return None;
            }
            let value = u32::from_str_radix(token, 16).ok()?;
            Some((Arg::Hex(value), rest))
        }
        Shape::Literal => {
            let (token, rest) = take_token(text);
            let mut chars = token.chars();
            let first = chars.next()?;
            if !(first.is_ascii_alphabetic() || first == '_')
                || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return None;
            }
            Some((Arg::Literal(token.to_owned()), rest))
        }
    }
}

/// A braced string at the start of `text`: its value (doubled braces
/// halved) and what follows; `None` when it is not one or holds a lone
/// brace.
fn parse_string(text: &str) -> Option<(String, &str)> {
    let inner = text.strip_prefix('{')?;
    let mut value = String::new();
    let mut chars = inner.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        match c {
            '{' => {
                if chars.next_if(|(_, next)| *next == '{').is_some() {
                    value.push('{');
                } else {
                    return None;
                }
            }
            '}' => {
                if chars.next_if(|(_, next)| *next == '}').is_some() {
                    value.push('}');
                } else {
                    return Some((value, &inner[index + 1..]));
                }
            }
            other => value.push(other),
        }
    }
    None
}

/// A string as written: braced, braces inside doubled.
fn quote(text: &str) -> String {
    format!("{{{}}}", text.replace('{', "{{").replace('}', "}}"))
}

fn arg_text(arg: &Arg) -> String {
    match arg {
        Arg::Text(value) | Arg::Literal(value) => value.clone(),
        _ => String::new(),
    }
}

fn arg_number(arg: &Arg) -> f64 {
    match arg {
        Arg::Int(value) => f64::from(*value),
        Arg::Double(value) => *value,
        Arg::Hex(value) => f64::from(value.cast_signed()),
        _ => 0.0,
    }
}

fn arg_hex(arg: &Arg) -> u32 {
    match arg {
        Arg::Hex(value) => *value,
        _ => 0,
    }
}

// ---------------------------------------------------------------------
// Keywords
// ---------------------------------------------------------------------

fn condition_keyword(kind: MetaConditionKind) -> &'static str {
    use MetaConditionKind as K;
    match kind {
        K::Never => "Never",
        K::Always => "Always",
        K::All => "All",
        K::Any => "Any",
        K::ChatMessage => "ChatMatch",
        K::PackSlotsLessThanOrEqual => "MainSlotsLE",
        K::SecondsInStateGreaterThanOrEqual => "SecsInStateGE",
        K::NavigationRouteEmpty => "NavEmpty",
        K::CharacterDeath => "Death",
        K::AnyVendorOpen => "VendorOpen",
        K::VendorClosed => "VendorClosed",
        K::InventoryItemCountLessThanOrEqual => "ItemCountLE",
        K::InventoryItemCountGreaterThanOrEqual => "ItemCountGE",
        K::MonsterNameCountWithinDistance => "MobsInDist_Name",
        K::MonsterPriorityCountWithinDistance => "MobsInDist_Priority",
        K::NeedToBuff => "NeedToBuff",
        K::NoMonstersWithinDistance => "NoMobsInDist",
        K::LandblockEquals => "BlockE",
        K::LandcellEquals => "CellE",
        K::PortalspaceEntered => "IntoPortal",
        K::PortalspaceExited => "ExitPortal",
        K::Not => "Not",
        K::PersistentSecondsInStateGreaterThanOrEqual => "PSecsInStateGE",
        K::TimeLeftOnSpellGreaterThanOrEqual => "SecsOnSpellGE",
        K::BurdenPercentGreaterThanOrEqual => "BuPercentGE",
        K::DistanceFromAnyRoutePointGreaterThanOrEqual => "DistToRteGE",
        K::Expression => "Expr",
        K::ChatMessageCapture => "ChatCapture",
        K::LoginComplete => "LoginComplete",
        K::Logoff => "Logoff",
        K::PortalTransition => "PortalDone",
        K::ItemUseCompleted => "ItemUsed",
        K::ContainerOpened => "ContainerOpened",
        K::ContainerClosed => "ContainerClosed",
        K::ConfirmationRequested => "ConfirmAsked",
    }
}

fn condition_shape(kind: MetaConditionKind) -> &'static [Shape] {
    use MetaConditionKind as K;
    use Shape::{Double as D, Hex as H, Int as I, Text as S};
    match kind {
        K::ChatMessage | K::Expression => &[S],
        K::PackSlotsLessThanOrEqual
        | K::SecondsInStateGreaterThanOrEqual
        | K::PersistentSecondsInStateGreaterThanOrEqual
        | K::BurdenPercentGreaterThanOrEqual => &[I],
        K::InventoryItemCountLessThanOrEqual | K::InventoryItemCountGreaterThanOrEqual => &[I, S],
        K::MonsterNameCountWithinDistance => &[I, D, S],
        K::MonsterPriorityCountWithinDistance => &[I, D, I],
        K::NoMonstersWithinDistance | K::DistanceFromAnyRoutePointGreaterThanOrEqual => &[D],
        K::LandblockEquals | K::LandcellEquals => &[H],
        K::TimeLeftOnSpellGreaterThanOrEqual => &[I, I],
        K::ChatMessageCapture => &[S, S],
        _ => &[],
    }
}

fn action_keyword(kind: MetaActionKind) -> &'static str {
    use MetaActionKind as K;
    match kind {
        K::None => "None",
        K::SetMetaState => "SetState",
        K::ChatCommand => "Chat",
        K::All => "DoAll",
        K::LoadEmbeddedNavigationRoute => "EmbedNav",
        K::CallMetaState => "CallState",
        K::ReturnFromCall => "Return",
        K::ExpressionAction => "DoExpr",
        K::ChatExpression => "ChatExpr",
        K::SetWatchdog => "SetWatchdog",
        K::ClearWatchdog => "ClearWatchdog",
        K::GetVtankOption => "GetOpt",
        K::SetVtankOption => "SetOpt",
        K::CreateView => "CreateView",
        K::DestroyView => "DestroyView",
        K::DestroyAllViews => "DestroyAllViews",
    }
}

fn action_shape(kind: MetaActionKind) -> &'static [Shape] {
    use MetaActionKind as K;
    use Shape::{Double as D, Text as S};
    match kind {
        K::SetMetaState
        | K::ChatCommand
        | K::ExpressionAction
        | K::ChatExpression
        | K::DestroyView => &[S],
        K::CallMetaState | K::GetVtankOption | K::SetVtankOption | K::CreateView => &[S, S],
        K::SetWatchdog => &[D, D, S],
        _ => &[],
    }
}

fn route_type_keyword(mode: RouteMode) -> &'static str {
    match mode {
        RouteMode::Circular => "circular",
        RouteMode::Linear => "linear",
        RouteMode::Target => "follow",
        RouteMode::Once => "once",
    }
}

// ---------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------

/// The depth (tabs) an `IF:` condition sits at.
const CONDITION_DEPTH: usize = 2;
/// The depth (tabs) a `DO:` action sits at.
const ACTION_DEPTH: usize = 3;

struct Parser<'a, 's> {
    lines: Vec<Line<'a>>,
    index: usize,
    spells: Option<&'s dyn SpellLookup>,
}

/// An action whose route is filled in once every `NAV:` is read.
struct PendingNav {
    rule: usize,
    path: Vec<usize>,
    tag: String,
}

impl<'a> Parser<'a, '_> {
    fn peek(&self) -> Option<Line<'a>> {
        self.lines.get(self.index).copied()
    }

    fn shape_error(line: Line<'_>, keyword: &str) -> String {
        // The C# reports these one line on (it has moved past the line).
        at(
            line.number + 1,
            &format!("'{keyword}' arguments do not match the expected shape."),
        )
    }

    /// `keyword rest` from a line's body (or a `Not`'s operand).
    fn split_keyword(text: &str) -> (&str, &str) {
        take_token(text)
    }

    fn condition_on(
        &mut self,
        line: Line<'a>,
        text: &str,
        depth: usize,
    ) -> Result<MetaCondition, String> {
        let (keyword, rest) = Self::split_keyword(text);
        if keyword == "Not" {
            let operand = rest.trim_start_matches([' ', '\t']);
            if operand.is_empty() {
                return Err(at(
                    line.number,
                    "'Not' must be followed by a Condition operation on the same line.",
                ));
            }
            let child = self.condition_on(line, operand, depth)?;
            return Ok(MetaCondition {
                children: vec![child],
                ..MetaCondition::of(MetaConditionKind::Not)
            });
        }
        let Some(kind) = MetaConditionKind::ALL
            .into_iter()
            .find(|kind| *kind != MetaConditionKind::Not && condition_keyword(*kind) == keyword)
        else {
            return Err(at(
                line.number,
                "expected a Condition operation after 'IF:'.",
            ));
        };
        let args = parse_args(rest, condition_shape(kind))
            .ok_or_else(|| Self::shape_error(line, keyword))?;
        let mut condition = MetaCondition::of(kind);
        use MetaConditionKind as K;
        match kind {
            K::ChatMessage | K::Expression => condition.text = arg_text(&args[0]),
            K::PackSlotsLessThanOrEqual
            | K::SecondsInStateGreaterThanOrEqual
            | K::PersistentSecondsInStateGreaterThanOrEqual
            | K::BurdenPercentGreaterThanOrEqual
            | K::NoMonstersWithinDistance
            | K::DistanceFromAnyRoutePointGreaterThanOrEqual
            | K::LandblockEquals
            | K::LandcellEquals => condition.number = arg_number(&args[0]),
            K::InventoryItemCountLessThanOrEqual | K::InventoryItemCountGreaterThanOrEqual => {
                condition.number = arg_number(&args[0]);
                condition.text = arg_text(&args[1]);
            }
            K::MonsterNameCountWithinDistance => {
                condition.number = arg_number(&args[0]);
                condition.secondary_number = arg_number(&args[1]);
                condition.text = arg_text(&args[2]);
            }
            K::MonsterPriorityCountWithinDistance => {
                condition.number = arg_number(&args[0]);
                condition.secondary_number = arg_number(&args[1]);
                condition.tertiary_number = arg_number(&args[2]);
            }
            K::TimeLeftOnSpellGreaterThanOrEqual => {
                condition.secondary_number = arg_number(&args[0]);
                condition.number = arg_number(&args[1]);
            }
            K::ChatMessageCapture => {
                condition.text = arg_text(&args[0]);
                condition.secondary_text = arg_text(&args[1]);
            }
            K::All | K::Any => {
                condition.children = self.condition_children(depth)?;
            }
            _ => {}
        }
        Ok(condition)
    }

    /// The conditions an `All` or `Any` at `depth` holds, on the lines below.
    fn condition_children(&mut self, depth: usize) -> Result<Vec<MetaCondition>, String> {
        let mut children = Vec::new();
        while let Some(line) = self.peek() {
            if line.is_keyword() {
                break;
            }
            if line.tabs > depth + 1 {
                return Err(at(
                    line.number,
                    "tabbed in too far inside an All/Any condition.",
                ));
            }
            if line.tabs <= depth {
                if depth == CONDITION_DEPTH {
                    return Err(at(
                        line.number,
                        "not tabbed in enough to be inside an All/Any condition.",
                    ));
                }
                break;
            }
            self.index += 1;
            children.push(self.condition_on(line, line.body, depth + 1)?);
        }
        Ok(children)
    }

    fn action_on(
        &mut self,
        line: Line<'a>,
        text: &str,
        depth: usize,
        pending: &mut Vec<PendingNav>,
        rule: usize,
        path: &mut Vec<usize>,
    ) -> Result<MetaAction, String> {
        let (keyword, rest) = Self::split_keyword(text);
        let Some(kind) = MetaActionKind::ALL
            .into_iter()
            .find(|kind| action_keyword(*kind) == keyword)
        else {
            return Err(at(line.number, "expected an Action operation after 'DO:'."));
        };
        let mut action = MetaAction::of(kind);
        if kind == MetaActionKind::LoadEmbeddedNavigationRoute {
            let args = parse_args(rest, &[Shape::Literal, Shape::Text])
                .or_else(|| parse_args(rest, &[Shape::Literal, Shape::Text, Shape::Text]))
                .ok_or_else(|| Self::shape_error(line, keyword))?;
            action.secondary_text = arg_text(&args[1]);
            pending.push(PendingNav {
                rule,
                path: path.clone(),
                tag: arg_text(&args[0]),
            });
            return Ok(action);
        }
        let args =
            parse_args(rest, action_shape(kind)).ok_or_else(|| Self::shape_error(line, keyword))?;
        use MetaActionKind as K;
        match kind {
            K::SetMetaState
            | K::ChatCommand
            | K::ExpressionAction
            | K::ChatExpression
            | K::DestroyView => {
                action.text = arg_text(&args[0]);
            }
            K::CallMetaState | K::GetVtankOption | K::SetVtankOption | K::CreateView => {
                action.text = arg_text(&args[0]);
                action.secondary_text = arg_text(&args[1]);
            }
            K::SetWatchdog => {
                action.number = arg_number(&args[0]);
                action.secondary_number = arg_number(&args[1]);
                action.text = arg_text(&args[2]);
            }
            K::All => {
                while let Some(child) = self.peek() {
                    if child.is_keyword() {
                        break;
                    }
                    if child.tabs > depth + 1 {
                        return Err(at(child.number, "tabbed in too far inside a DoAll action."));
                    }
                    if child.tabs <= depth {
                        if depth == ACTION_DEPTH {
                            return Err(at(
                                child.number,
                                "not tabbed in enough to be inside a DoAll action.",
                            ));
                        }
                        break;
                    }
                    self.index += 1;
                    path.push(action.children.len());
                    let nested =
                        self.action_on(child, child.body, depth + 1, pending, rule, path)?;
                    path.pop();
                    action.children.push(nested);
                }
            }
            _ => {}
        }
        Ok(action)
    }

    /// `IF:`/`DO:` with what follows the colon.
    fn after_marker<'b>(body: &'b str, marker: &str) -> Option<&'b str> {
        body.strip_prefix(marker)
            .map(|rest| rest.trim_start_matches([' ', '\t']))
    }

    fn state_header(line: Line<'_>) -> Result<String, String> {
        let invalid = || {
            at(
                line.number,
                "'STATE:' must be followed by a delimited string name.",
            )
        };
        let rest = line.body.strip_prefix("STATE:").ok_or_else(invalid)?;
        let args = parse_args(rest, &[Shape::Text]).ok_or_else(invalid)?;
        Ok(arg_text(&args[0]))
    }

    fn nav_header(line: Line<'_>) -> Result<(String, RouteMode), String> {
        let invalid = || {
            at(
                line.number,
                "'NAV:' must be followed by a tag literal and a nav type (circular/linear/once/follow).",
            )
        };
        let rest = line.body.strip_prefix("NAV:").ok_or_else(invalid)?;
        let trimmed = rest.trim_start_matches([' ', '\t']);
        if trimmed.len() == rest.len() {
            return Err(invalid());
        }
        let (tag, after) = parse_arg(trimmed, Shape::Literal).ok_or_else(invalid)?;
        let (mode, after) = take_token(after.trim_start_matches([' ', '\t']));
        if !after.trim().is_empty() {
            return Err(invalid());
        }
        let mode = match mode {
            "circular" => RouteMode::Circular,
            "linear" => RouteMode::Linear,
            "follow" => RouteMode::Target,
            "once" => RouteMode::Once,
            _ => return Err(invalid()),
        };
        Ok((arg_text(&tag), mode))
    }

    /// The nodes of a `NAV:` section, up to the next block.
    fn nav_body(&mut self, mode: RouteMode) -> Result<NavigationSettings, String> {
        let mut route = NavigationSettings::with_mode(mode);
        while let Some(line) = self.peek() {
            if line.is_block_start() {
                break;
            }
            self.index += 1;
            let (keyword, rest) = take_token(line.body);
            let node_error = || {
                at(
                    line.number,
                    "expected a nav node line (one tab, then a node keyword).",
                )
            };
            if line.tabs != 1 {
                return Err(node_error());
            }
            use Shape::{Decimal as M, Double as D, Hex as H, Int as I, Text as S};
            let shape: &[Shape] = match keyword {
                "flw" => &[H, S],
                "pnt" | "chk" => &[D, D, D],
                "prt" => &[D, D, D, H],
                "rcl" | "cht" => &[D, D, D, S],
                "pau" => &[D, D, D, M],
                "vnd" => &[D, D, D, H, S],
                "ptl" | "tlk" => &[D, D, D, D, D, D, I, S],
                "jmp" => &[D, D, D, D, S, M],
                _ => return Err(node_error()),
            };
            let args = parse_args(rest, shape).ok_or_else(|| Self::shape_error(line, keyword))?;
            if keyword == "flw" {
                if mode == RouteMode::Target {
                    route.follow_target_object_id = arg_hex(&args[0]);
                    route.follow_target_name = arg_text(&args[1]);
                }
                continue;
            }
            if mode == RouteMode::Target {
                continue;
            }
            let kind = match keyword {
                "pnt" => RouteWaypointType::Point,
                "chk" => RouteWaypointType::Checkpoint,
                "prt" => RouteWaypointType::Portal,
                "rcl" => RouteWaypointType::Recall,
                "cht" => RouteWaypointType::ChatCommand,
                "pau" => RouteWaypointType::Pause,
                "vnd" => RouteWaypointType::OpenVendor,
                "ptl" => RouteWaypointType::PortalByName,
                "tlk" => RouteWaypointType::UseNpc,
                _ => RouteWaypointType::Jump,
            };
            let mut waypoint = RouteWaypoint::at(
                kind,
                arg_number(&args[0]),
                arg_number(&args[1]),
                arg_number(&args[2]),
            );
            match kind {
                RouteWaypointType::Portal => waypoint.object_id = arg_hex(&args[3]),
                RouteWaypointType::Recall => {
                    waypoint.recall_spell_name = arg_text(&args[3]);
                    waypoint.resolve_recall_spell(self.spells);
                }
                RouteWaypointType::ChatCommand => waypoint.text = arg_text(&args[3]),
                RouteWaypointType::Pause => {
                    waypoint.duration_milliseconds = round_to_int_checked(arg_number(&args[3]))
                        .map_err(|_| Self::shape_error(line, keyword))?;
                }
                RouteWaypointType::OpenVendor => {
                    waypoint.object_id = arg_hex(&args[3]);
                    waypoint.object_name = arg_text(&args[4]);
                }
                RouteWaypointType::PortalByName | RouteWaypointType::UseNpc => {
                    waypoint.reference_position = map_position(
                        arg_number(&args[3]),
                        arg_number(&args[4]),
                        arg_number(&args[5]),
                    );
                    waypoint.legacy_object_class = match args[6] {
                        Arg::Int(value) => value,
                        _ => 0,
                    };
                    waypoint.object_name = arg_text(&args[7]);
                }
                RouteWaypointType::Jump => {
                    waypoint.jump_heading_degrees = arg_number(&args[3]);
                    // Anything but True (any case) leaves at a run.
                    waypoint.jump_hold_shift = parse_bool(&arg_text(&args[4])).unwrap_or(false);
                    let charge = arg_number(&args[5]);
                    waypoint.jump_charge_milliseconds = round_to_int_checked(charge)
                        .map_err(|_| Self::shape_error(line, keyword))?;
                    waypoint.jump_direction = jump_direction_of(&rest_token(rest, 6));
                }
                _ => {}
            }
            route.waypoints.push(waypoint);
        }
        Ok(route)
    }
}

/// The `n`th white-space separated token of `rest` (for a number's text).
fn rest_token(rest: &str, n: usize) -> String {
    // The braced argument holds no white space for `jmp` (True/False), so a
    // plain split finds the charge.
    rest.split([' ', '\t'])
        .filter(|token| !token.is_empty())
        .nth(n - 1)
        .unwrap_or("")
        .to_owned()
}

/// A jump's direction from its charge as written: digits, a point, four
/// zeros and the direction code (3 forward, 4 strafe left, 5 strafe right);
/// anything else is forward.
fn jump_direction_of(charge: &str) -> RouteJumpDirection {
    let Some((whole, fraction)) = charge.split_once('.') else {
        return RouteJumpDirection::Forward;
    };
    if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) {
        return RouteJumpDirection::Forward;
    }
    match fraction {
        "00004" => RouteJumpDirection::StrafeLeft,
        "00005" => RouteJumpDirection::StrafeRight,
        _ => RouteJumpDirection::Forward,
    }
}

/// Reads a meta file.
///
/// # Errors
/// Why the text is not a meta, naming the line; a file of routes only is
/// refused (it belongs among the routes).
pub fn try_load_meta(text: &str, spells: Option<&dyn SpellLookup>) -> Result<MetaProfile, String> {
    let (lines, line_count) = split_lines(text);
    let mut parser = Parser {
        lines,
        index: 0,
        spells,
    };
    let mut profile = MetaProfile::default();
    let mut navs: Vec<(String, NavigationSettings)> = Vec::new();
    let mut pending: Vec<PendingNav> = Vec::new();
    let mut saw_state = false;
    while let Some(line) = parser.peek() {
        parser.index += 1;
        if line.body.starts_with("STATE:") {
            saw_state = true;
            let state = Parser::state_header(line)?;
            let mut rules = 0;
            loop {
                let Some(next) = parser.peek() else {
                    if rules == 0 {
                        return Err(at(
                            line_count,
                            &format!(
                                "state '{state}' has no rules (every state needs at least one)."
                            ),
                        ));
                    }
                    break;
                };
                if next.is_block_start() {
                    if rules == 0 {
                        return Err(at(
                            next.number,
                            &format!(
                                "state '{state}' has no rules (every state needs at least one)."
                            ),
                        ));
                    }
                    break;
                }
                let Some(condition_text) = Parser::after_marker(next.body, "IF:") else {
                    if next.body.starts_with("DO:") {
                        return Err(at(
                            next.number,
                            "expected an 'IF:' line to start a new Rule.",
                        ));
                    }
                    return Err(at(next.number, "expected 'STATE:', 'IF:', or 'NAV:'."));
                };
                parser.index += 1;
                let condition = parser.condition_on(next, condition_text, CONDITION_DEPTH)?;
                // At the end of the file the C# fails with an index error;
                // here it is the same refusal as a missing DO: line.
                let Some(do_line) = parser.peek() else {
                    return Err(at(line_count, "expected a 'DO:' line."));
                };
                let Some(action_text) = Parser::after_marker(do_line.body, "DO:") else {
                    return Err(at(do_line.number, "expected a 'DO:' line."));
                };
                parser.index += 1;
                let rule_index = profile.rules.len();
                let mut path = Vec::new();
                let action = parser.action_on(
                    do_line,
                    action_text,
                    ACTION_DEPTH,
                    &mut pending,
                    rule_index,
                    &mut path,
                )?;
                profile.rules.push(MetaRule::new(&state, condition, action));
                rules += 1;
            }
        } else if line.body.starts_with("NAV:") {
            let (tag, mode) = Parser::nav_header(line)?;
            let route = parser.nav_body(mode)?;
            navs.retain(|(existing, _)| *existing != tag);
            navs.push((tag, route));
        } else {
            return Err(at(line.number, "expected 'STATE:' or 'NAV:'."));
        }
    }
    if !saw_state && !navs.is_empty() {
        return Err(at(
            line_count,
            "contains only NAV: block(s) and no STATE: block — this looks like a stand-alone route profile that belongs in the mosstank/navs/ folder, not a Meta profile.",
        ));
    }
    for embed in pending {
        let route = navs
            .iter()
            .find(|(tag, _)| *tag == embed.tag)
            .map_or_else(NavigationSettings::default, |(_, route)| route.clone());
        let mut action = &mut profile.rules[embed.rule].action;
        for index in &embed.path {
            action = &mut action.children[*index];
        }
        action.embedded_route = Some(Box::new(route));
    }
    Ok(profile)
}

/// Reads a route file into `target` (its mode, waypoints and follow
/// target). Nothing changes when the text does not read.
///
/// # Errors
/// Why the text is not a route; a meta is refused (it belongs among the
/// metas).
pub fn try_load_nav(
    text: &str,
    target: &mut NavigationSettings,
    spells: Option<&dyn SpellLookup>,
) -> Result<(), String> {
    let (lines, line_count) = split_lines(text);
    if let Some(state) = lines.iter().find(|line| line.body.starts_with("STATE:")) {
        return Err(at(
            state.number,
            "found a STATE: rule section — this is a Meta profile (possibly with an embedded nav), not a stand-alone route; load it from the mosstank/metas/ folder instead.",
        ));
    }
    let mut parser = Parser {
        lines,
        index: 0,
        spells,
    };
    let mut found: Option<NavigationSettings> = None;
    while let Some(line) = parser.peek() {
        parser.index += 1;
        if !line.body.starts_with("NAV:") {
            return Err(at(
                line.number,
                "expected 'NAV:' (or a STATE: rule section, refused for a route load).",
            ));
        }
        let (_, mode) = Parser::nav_header(line)?;
        let route = parser.nav_body(mode)?;
        if found.is_none() {
            found = Some(route);
        }
    }
    let Some(route) = found else {
        return Err(at(
            line_count,
            "no NAV: block found — this looks like a Meta profile that belongs in the mosstank/metas/ folder, not a route.",
        ));
    };
    crate::nav::apply(target, route);
    Ok(())
}

// ---------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------

fn af_number(value: f64) -> String {
    to_string_f64(value)
}

fn hex_of(value: f64) -> Result<String, String> {
    let whole = checked_double_to_int(value)?;
    Ok(format!("{:08X}", whole.cast_unsigned()))
}

struct Writer {
    out: String,
    /// Each `EmbedNav` action (by address) with its tag.
    tags: Vec<(usize, String)>,
    /// The routes to write, by tag, in the profile's order.
    navs: Vec<(String, NavigationSettings)>,
}

impl Writer {
    /// Numbers the routes in the order the profile holds them (rule by
    /// rule, depth first), which is the order the C# numbers them in, even
    /// when the file groups the rules by state.
    fn number_routes<'p>(&mut self, actions: impl Iterator<Item = &'p MetaAction>) {
        fn walk(writer: &mut Writer, action: &MetaAction) {
            if action.kind == MetaActionKind::LoadEmbeddedNavigationRoute {
                let tag = nav_tag(writer.tags.len(), &action.secondary_text);
                if let Some(route) = &action.embedded_route {
                    writer.navs.push((tag.clone(), (**route).clone()));
                }
                writer.tags.push((std::ptr::from_ref(action) as usize, tag));
            }
            for child in &action.children {
                walk(writer, child);
            }
        }
        for action in actions {
            walk(self, action);
        }
    }

    fn line(&mut self, tabs: usize, text: &str) {
        self.out.push_str(&"\t".repeat(tabs));
        self.out.push_str(text);
        self.out.push_str("\r\n");
    }

    fn condition_text(condition: &MetaCondition) -> Result<String, String> {
        use MetaConditionKind as K;
        let keyword = condition_keyword(condition.kind);
        Ok(match condition.kind {
            K::Not => match condition.children.first() {
                Some(child) => format!("Not {}", Self::condition_text(child)?),
                None => "Not".to_owned(),
            },
            K::ChatMessage | K::Expression => format!("{keyword} {}", quote(&condition.text)),
            K::PackSlotsLessThanOrEqual
            | K::SecondsInStateGreaterThanOrEqual
            | K::PersistentSecondsInStateGreaterThanOrEqual
            | K::BurdenPercentGreaterThanOrEqual
            | K::NoMonstersWithinDistance
            | K::DistanceFromAnyRoutePointGreaterThanOrEqual => {
                format!("{keyword} {}", af_number(condition.number))
            }
            K::LandblockEquals | K::LandcellEquals => {
                format!("{keyword} {}", hex_of(condition.number)?)
            }
            K::InventoryItemCountLessThanOrEqual | K::InventoryItemCountGreaterThanOrEqual => {
                format!(
                    "{keyword} {} {}",
                    af_number(condition.number),
                    quote(&condition.text)
                )
            }
            K::MonsterNameCountWithinDistance => format!(
                "{keyword} {} {} {}",
                af_number(condition.number),
                af_number(condition.secondary_number),
                quote(&condition.text)
            ),
            K::MonsterPriorityCountWithinDistance => format!(
                "{keyword} {} {} {}",
                af_number(condition.number),
                af_number(condition.secondary_number),
                af_number(condition.tertiary_number)
            ),
            K::TimeLeftOnSpellGreaterThanOrEqual => format!(
                "{keyword} {} {}",
                af_number(condition.secondary_number),
                af_number(condition.number)
            ),
            K::ChatMessageCapture => format!(
                "{keyword} {} {}",
                quote(&condition.text),
                quote(&condition.secondary_text)
            ),
            _ => keyword.to_owned(),
        })
    }

    /// The condition's own children (through a `Not`, which adds no depth).
    fn condition_children(condition: &MetaCondition) -> &[MetaCondition] {
        match condition.kind {
            MetaConditionKind::All | MetaConditionKind::Any => &condition.children,
            MetaConditionKind::Not => condition
                .children
                .first()
                .map_or(&[], Self::condition_children),
            _ => &[],
        }
    }

    fn write_condition_children(
        &mut self,
        condition: &MetaCondition,
        depth: usize,
    ) -> Result<(), String> {
        for child in Self::condition_children(condition) {
            let text = Self::condition_text(child)?;
            self.line(depth + 1, &text);
            self.write_condition_children(child, depth + 1)?;
        }
        Ok(())
    }

    fn action_text(&mut self, action: &MetaAction) -> String {
        use MetaActionKind as K;
        let keyword = action_keyword(action.kind);
        match action.kind {
            K::SetMetaState | K::ChatCommand | K::ExpressionAction | K::ChatExpression => {
                format!("{keyword} {}", quote(&action.text))
            }
            // Two spaces, as the C# writes it.
            K::DestroyView => format!("{keyword}  {}", quote(&action.text)),
            K::CallMetaState | K::GetVtankOption | K::SetVtankOption | K::CreateView => format!(
                "{keyword} {} {}",
                quote(&action.text),
                quote(&action.secondary_text)
            ),
            K::SetWatchdog => format!(
                "{keyword} {} {} {}",
                af_number(action.number),
                af_number(action.secondary_number),
                quote(&action.text)
            ),
            K::LoadEmbeddedNavigationRoute => {
                let tag = self
                    .tags
                    .iter()
                    .find(|(address, _)| *address == std::ptr::from_ref(action) as usize)
                    .map(|(_, tag)| tag.clone())
                    .unwrap_or_default();
                format!("{keyword} {tag} {}", quote(&action.secondary_text))
            }
            _ => keyword.to_owned(),
        }
    }

    fn write_action_children(&mut self, action: &MetaAction, depth: usize) {
        if action.kind != MetaActionKind::All {
            return;
        }
        for child in &action.children {
            let text = self.action_text(child);
            self.line(depth + 1, &text);
            self.write_action_children(child, depth + 1);
        }
    }

    fn write_route(&mut self, tag: &str, route: &NavigationSettings) {
        self.line(
            0,
            &format!("NAV: {tag} {} ~~ {{", route_type_keyword(route.mode)),
        );
        if route.mode == RouteMode::Target {
            self.line(
                1,
                &format!(
                    "flw {:08X} {}",
                    route.follow_target_object_id,
                    quote(&route.follow_target_name)
                ),
            );
        } else {
            for waypoint in &route.waypoints {
                self.line(1, &node_text(waypoint));
            }
        }
        self.line(0, "~~ }");
    }
}

/// A route's tag in a meta file: `nav` and its number, then two
/// underscores and its name with everything but ASCII letters, digits and
/// underscores turned into underscores.
fn nav_tag(index: usize, name: &str) -> String {
    if name.is_empty() {
        return format!("nav{index}");
    }
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("nav{index}__{cleaned}")
}

fn node_text(waypoint: &RouteWaypoint) -> String {
    let at = format!(
        "{} {} {}",
        af_number(waypoint.position.east_west),
        af_number(waypoint.position.north_south),
        af_number(waypoint.position.elevation)
    );
    match waypoint.kind {
        RouteWaypointType::Point => format!("pnt {at}"),
        RouteWaypointType::Checkpoint => format!("chk {at}"),
        RouteWaypointType::Portal => format!("prt {at} {:08X}", waypoint.object_id),
        RouteWaypointType::Recall => format!("rcl {at} {}", quote(&waypoint.recall_spell_name)),
        RouteWaypointType::Pause => format!("pau {at} {}", waypoint.duration_milliseconds),
        RouteWaypointType::ChatCommand => format!("cht {at} {}", quote(&waypoint.text)),
        RouteWaypointType::OpenVendor => format!(
            "vnd {at} {:08X} {}",
            waypoint.object_id,
            quote(&waypoint.object_name)
        ),
        RouteWaypointType::PortalByName | RouteWaypointType::UseNpc => format!(
            "{} {at} {} {} {} {} {}",
            if waypoint.kind == RouteWaypointType::PortalByName {
                "ptl"
            } else {
                "tlk"
            },
            af_number(waypoint.reference_position.east_west),
            af_number(waypoint.reference_position.north_south),
            af_number(waypoint.reference_position.elevation),
            waypoint.legacy_object_class,
            quote(&waypoint.object_name)
        ),
        RouteWaypointType::Jump => format!(
            "jmp {at} {} {} {}.0000{}",
            af_number(waypoint.jump_heading_degrees),
            if waypoint.jump_hold_shift {
                "{True}"
            } else {
                "{False}"
            },
            waypoint.jump_charge_milliseconds,
            jump_direction_code(waypoint.jump_direction)
        ),
    }
}

/// Writes a meta file. Switched-off rules have no form in it: they are
/// refused unless `drop_disabled_rules` says to leave them out.
///
/// # Errors
/// Switched-off rules (unless dropped); a landblock or landcell past the
/// 32-bit range.
pub fn save_meta(profile: &MetaProfile, drop_disabled_rules: bool) -> Result<String, String> {
    let disabled = profile.rules.iter().filter(|rule| !rule.enabled).count();
    if disabled > 0 && !drop_disabled_rules {
        return Err(format!(
            "SaveMeta: {disabled} disabled rule(s) have no metaf-compatible representation and would be silently dropped. Call SaveMeta(profile, dropDisabledRules: true) to accept that loss explicitly."
        ));
    }
    let mut states: Vec<&str> = Vec::new();
    for rule in profile.rules.iter().filter(|rule| rule.enabled) {
        if !states.contains(&rule.state.as_str()) {
            states.push(&rule.state);
        }
    }
    let mut writer = Writer {
        out: format!("{META_HEADER}\r\n"),
        tags: Vec::new(),
        navs: Vec::new(),
    };
    writer.number_routes(
        profile
            .rules
            .iter()
            .filter(|rule| rule.enabled)
            .map(|rule| &rule.action),
    );
    for state in states {
        writer.line(0, &format!("STATE: {} ~~ {{", quote(state)));
        for rule in profile
            .rules
            .iter()
            .filter(|rule| rule.enabled && rule.state == state)
        {
            let condition = Writer::condition_text(&rule.condition)?;
            writer.line(1, &format!("IF:\t{condition}"));
            writer.write_condition_children(&rule.condition, CONDITION_DEPTH)?;
            let action = writer.action_text(&rule.action);
            writer.line(2, &format!("DO:\t{action}"));
            writer.write_action_children(&rule.action, ACTION_DEPTH);
        }
        writer.line(0, "~~ }");
    }
    if !writer.navs.is_empty() {
        writer.out.push_str("\r\n");
        writer.line(0, SEPARATOR);
        writer.out.push_str("\r\n");
        let navs = std::mem::take(&mut writer.navs);
        for (tag, route) in &navs {
            writer.write_route(tag, route);
        }
    }
    Ok(writer.out)
}

/// Writes a route file.
///
/// # Errors
/// None today; kept fallible like the other writers.
pub fn save_nav(route: &NavigationSettings) -> Result<String, String> {
    let mut writer = Writer {
        out: format!("{NAV_HEADER}\r\n"),
        navs: Vec::new(),
        tags: Vec::new(),
    };
    writer.write_route("nav0", route);
    Ok(writer.out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::{NoSpells, RouteRecallKind};

    fn load_meta(text: &str) -> MetaProfile {
        try_load_meta(text, Some(&NoSpells)).unwrap_or_else(|error| panic!("{error}"))
    }

    fn load_nav(text: &str) -> NavigationSettings {
        let mut route = NavigationSettings::default();
        try_load_nav(text, &mut route, Some(&NoSpells)).unwrap_or_else(|error| panic!("{error}"));
        route
    }

    fn meta_fixtures() -> Vec<String> {
        crate::fixtures::list("vtank/af", "af")
            .into_iter()
            .filter(|path| !path.rsplit('/').next().unwrap_or("").starts_with("nav_"))
            .collect()
    }

    fn nav_fixtures() -> Vec<String> {
        crate::fixtures::list("vtank/af", "af")
            .into_iter()
            .filter(|path| path.rsplit('/').next().unwrap_or("").starts_with("nav_"))
            .collect()
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: EveryMetaAfFixtureParses
    #[test]
    fn every_meta_af_fixture_parses() {
        let fixtures = meta_fixtures();
        assert_eq!(fixtures.len(), 12);
        for path in fixtures {
            let profile = try_load_meta(&crate::fixtures::read(&path), Some(&NoSpells))
                .unwrap_or_else(|error| panic!("{path}: {error}"));
            assert!(!profile.rules.is_empty(), "{path}");
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: EveryNavOnlyAfFixtureParses
    #[test]
    fn every_nav_only_af_fixture_parses() {
        for path in nav_fixtures() {
            let mut route = NavigationSettings::default();
            try_load_nav(&crate::fixtures::read(&path), &mut route, Some(&NoSpells))
                .unwrap_or_else(|error| panic!("{path}: {error}"));
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: EveryNavOnlyAfFixtureIsRefusedByTryLoadMeta
    #[test]
    fn every_nav_only_af_fixture_is_refused_by_try_load_meta() {
        for path in nav_fixtures() {
            let error = try_load_meta(&crate::fixtures::read(&path), Some(&NoSpells)).unwrap_err();
            assert!(error.contains("navs/"), "{path}: {error}");
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: MetaOnlyContentIsRefusedByTryLoadNavWithMetasFolderNotice
    #[test]
    fn meta_only_content_is_refused_by_try_load_nav_with_metas_folder_notice() {
        let text = crate::fixtures::read("vtank/af/bella.af");
        let mut route = NavigationSettings::default();
        let error = try_load_nav(&text, &mut route, Some(&NoSpells)).unwrap_err();
        assert!(error.contains("metas/"), "{error}");
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: MetaParseWriteParseIsIdentical
    #[test]
    fn meta_parse_write_parse_is_identical() {
        for path in meta_fixtures() {
            let first = load_meta(&crate::fixtures::read(&path));
            let second = load_meta(&save_meta(&first, false).unwrap());
            assert_eq!(first, second, "{path}");
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: NavParseWriteParseIsIdentical
    #[test]
    fn nav_parse_write_parse_is_identical() {
        for path in nav_fixtures() {
            let first = load_nav(&crate::fixtures::read(&path));
            let second = load_nav(&save_nav(&first).unwrap());
            assert_eq!(first, second, "{path}");
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: BinaryImportMatchesMetafAfConversion
    // (and ExampleSortMetaBinaryImportMatchesAf, one of its pairs)
    #[test]
    fn binary_import_matches_metaf_af_conversion() {
        let mut pairs = 0;
        for met in crate::fixtures::list("vtank/met", "met") {
            let af = met.replace("/met/", "/af/").replace(".met", ".af");
            if !crate::fixtures::root().join(&af).exists() {
                continue;
            }
            pairs += 1;
            let from_met = crate::met::try_load(&crate::fixtures::read(&met), Some(&NoSpells))
                .unwrap_or_else(|error| panic!("{met}: {error}"));
            let from_af = load_meta(&crate::fixtures::read(&af));
            assert_eq!(from_met, from_af, "{met}");
        }
        assert_eq!(pairs, 9);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: BinaryNavImportMatchesMetafAfConversion
    #[test]
    fn binary_nav_import_matches_metaf_af_conversion() {
        let mut pairs = 0;
        for nav in crate::fixtures::list("vtank/nav", "nav") {
            let af = nav.replace("/nav/", "/af/").replace(".nav", ".af");
            if !crate::fixtures::root().join(&af).exists() {
                continue;
            }
            pairs += 1;
            let mut from_nav = NavigationSettings::default();
            crate::nav::try_load(&crate::fixtures::read(&nav), &mut from_nav, Some(&NoSpells))
                .unwrap();
            let from_af = load_nav(&crate::fixtures::read(&af));
            assert_eq!(from_nav, from_af, "{nav}");
        }
        assert_eq!(pairs, 5);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: WriterOutputMatchesMetafCanonicalEmission
    #[test]
    fn writer_output_matches_metaf_canonical_emission() {
        for name in [
            "bella",
            "gauntlet_leader",
            "empyrean_facility",
            "augments",
            "example_sort_meta",
        ] {
            let original = crate::fixtures::read(&format!("vtank/af/{name}.af"));
            let profile = load_meta(&original);
            assert_eq!(save_meta(&profile, false).unwrap(), original, "{name}");
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: WriterOutputMatchesMetafCanonicalEmissionNavOnly
    #[test]
    fn writer_output_matches_metaf_canonical_emission_nav_only() {
        for name in [
            "nav_ab",
            "nav_briennecarlus",
            "nav_empyrean",
            "nav_lockandkeyjaw",
        ] {
            let original = crate::fixtures::read(&format!("vtank/af/{name}.af"));
            let route = load_nav(&original);
            assert_eq!(save_nav(&route).unwrap(), original, "{name}");
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: PtlNodeKeepsBothCoordinateTriplesDistinct
    #[test]
    fn ptl_node_keeps_both_coordinate_triples_distinct() {
        let profile = load_meta(&crate::fixtures::read("vtank/af/aphus.af"));
        let waypoint = profile
            .rules
            .iter()
            .filter_map(|rule| find_waypoint(&rule.action, "Portal to Town Network"))
            .next()
            .expect("a waypoint named Portal to Town Network");
        assert!((waypoint.position.east_west - -101.597_905_190_786).abs() < 1e-6);
        assert!((waypoint.position.north_south - -96.621_609_369_913_7).abs() < 1e-6);
        assert!((waypoint.position.elevation - 2.083_331_346_511_84e-5).abs() < 1e-10);
        assert!((waypoint.reference_position.east_west - 59.393_645_858_764_7).abs() < 1e-6);
        assert!((waypoint.reference_position.north_south - -28.725_608_348_846_4).abs() < 1e-6);
        assert!((waypoint.reference_position.elevation - 0.050_825_003_534_555_4).abs() < 1e-6);
        let rewritten = save_meta(&profile, false).unwrap();
        assert!(rewritten.contains(
            "ptl -101.597905190786 -96.6216093699137 2.08333134651184E-05 59.3936458587647 -28.7256083488464 0.0508250035345554 14 {Portal to Town Network}"
        ));
    }

    fn find_waypoint(action: &MetaAction, name: &str) -> Option<RouteWaypoint> {
        if let Some(route) = &action.embedded_route
            && let Some(found) = route.waypoints.iter().find(|w| w.object_name == name)
        {
            return Some(found.clone());
        }
        action
            .children
            .iter()
            .find_map(|child| find_waypoint(child, name))
    }

    fn chat_rule(enabled: bool, text: &str) -> MetaRule {
        MetaRule {
            enabled,
            ..MetaRule::new(
                "Default",
                MetaCondition::always(),
                MetaAction {
                    text: text.to_owned(),
                    ..MetaAction::of(MetaActionKind::ChatCommand)
                },
            )
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: SaveMetaRefusesToDropADisabledRuleByDefault
    #[test]
    fn save_meta_refuses_to_drop_a_disabled_rule_by_default() {
        let profile = MetaProfile {
            rules: vec![chat_rule(false, "/say must not silently vanish")],
        };
        assert!(
            save_meta(&profile, false)
                .unwrap_err()
                .contains("1 disabled rule")
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: SaveMetaDropsDisabledRulesOnlyWhenExplicitlyToldTo
    #[test]
    fn save_meta_drops_disabled_rules_only_when_explicitly_told_to() {
        let profile = MetaProfile {
            rules: vec![chat_rule(false, "/say must not run")],
        };
        let source = save_meta(&profile, true).unwrap();
        assert!(!source.contains("must not run"));
        assert!(load_meta(&source).rules.is_empty());
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: NestedAllAnyNotParsesCorrectly
    #[test]
    fn nested_all_any_not_parses_correctly() {
        let af = [
            "STATE: {Default}",
            "\tIF:\tAll",
            "\t\t\tAlways",
            "\t\t\tNot Death",
            "\t\t\tAny",
            "\t\t\t\tVendorOpen",
            "\t\t\t\tVendorClosed",
            "\t\tDO:\tDoAll",
            "\t\t\t\tChat {hello}",
            "\t\t\t\tSetState {Next}",
        ]
        .join("\r\n")
            + "\r\n";
        let profile = load_meta(&af);
        assert_eq!(profile.rules.len(), 1);
        let rule = &profile.rules[0];
        assert_eq!(rule.condition.kind, MetaConditionKind::All);
        assert_eq!(rule.condition.children.len(), 3);
        assert_eq!(rule.condition.children[0].kind, MetaConditionKind::Always);
        assert_eq!(rule.condition.children[1].kind, MetaConditionKind::Not);
        assert_eq!(
            rule.condition.children[1].children[0].kind,
            MetaConditionKind::CharacterDeath
        );
        assert_eq!(rule.condition.children[2].kind, MetaConditionKind::Any);
        assert_eq!(rule.condition.children[2].children.len(), 2);
        assert_eq!(rule.action.kind, MetaActionKind::All);
        assert_eq!(rule.action.children.len(), 2);
        assert_eq!(rule.action.children[0].text, "hello");
        assert_eq!(rule.action.children[1].text, "Next");
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: BracesInsideStringsAreDoubledInTheFileAndSingleInTheProfile
    #[test]
    fn braces_inside_strings_are_doubled_in_the_file_and_single_in_the_profile() {
        let af = [
            "STATE: {Default}",
            "\tIF:\tExpr {$list{{1}}==2}",
            "\t\tDO:\tDoAll",
            "\t\t\t\tDoExpr {$list=listcreate[1,2];$first=$list{{0}}}",
            "\t\t\t\tEmbedNav nav0 {route}",
            "\tIF:\tChatCapture {^(?<n>\\d{{2,3}}) left$} {0}",
            "\t\tDO:\tChat {/t Horan, {{}}}",
            "",
            "NAV: nav0 once ~~ {",
            "\tcht 1 2 3 {say {{hi}}}",
            "~~ }",
        ]
        .join("\r\n")
            + "\r\n";
        let profile = load_meta(&af);
        assert_eq!(profile.rules[0].condition.text, "$list{1}==2");
        assert_eq!(
            profile.rules[0].action.children[0].text,
            "$list=listcreate[1,2];$first=$list{0}"
        );
        assert_eq!(profile.rules[1].condition.text, "^(?<n>\\d{2,3}) left$");
        assert_eq!(profile.rules[1].action.text, "/t Horan, {}");
        let route = profile.rules[0].action.children[1]
            .embedded_route
            .as_ref()
            .unwrap();
        assert_eq!(route.waypoints.len(), 1);
        assert_eq!(route.waypoints[0].text, "say {hi}");

        let rewritten = save_meta(&profile, false).unwrap();
        assert!(rewritten.contains("Expr {$list{{1}}==2}"));
        assert!(rewritten.contains("Chat {/t Horan, {{}}}"));
        assert!(rewritten.contains("cht 1 2 3 {say {{hi}}}"));
        let again = load_meta(&rewritten);
        assert_eq!(
            profile.rules[0].condition.text,
            again.rules[0].condition.text
        );
        assert_eq!(
            profile.rules[0].action.children[0].text,
            again.rules[0].action.children[0].text
        );
        assert_eq!(
            profile.rules[1].condition.text,
            again.rules[1].condition.text
        );
        assert_eq!(
            again.rules[0].action.children[1]
                .embedded_route
                .as_ref()
                .unwrap()
                .waypoints[0]
                .text,
            "say {hi}"
        );
    }

    fn shape(condition: &MetaCondition) -> String {
        if condition.kind == MetaConditionKind::Expression {
            return format!("Expr {}", condition.text);
        }
        let children: Vec<String> = condition.children.iter().map(shape).collect();
        format!("{}({})", condition.kind.name(), children.join(","))
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: ANegatedAllOrAnyKeepsItsOperandsThroughSaveAndLoad
    #[test]
    fn a_negated_all_or_any_keeps_its_operands_through_save_and_load() {
        let expr = |text: &str| MetaCondition {
            text: text.to_owned(),
            ..MetaCondition::of(MetaConditionKind::Expression)
        };
        let not = |operand: MetaCondition| MetaCondition {
            children: vec![operand],
            ..MetaCondition::of(MetaConditionKind::Not)
        };
        let group = |kind, children| MetaCondition {
            children,
            ..MetaCondition::of(kind)
        };
        let chat = |text: &str| MetaAction {
            text: text.to_owned(),
            ..MetaAction::of(MetaActionKind::ChatCommand)
        };
        let profile = MetaProfile {
            rules: vec![
                MetaRule::new(
                    "Default",
                    not(group(
                        MetaConditionKind::Any,
                        vec![expr("a==1"), expr("b==2"), expr("c==3")],
                    )),
                    chat("top"),
                ),
                MetaRule::new(
                    "Default",
                    group(
                        MetaConditionKind::All,
                        vec![
                            expr("x==1"),
                            not(group(
                                MetaConditionKind::All,
                                vec![
                                    expr("a"),
                                    not(group(MetaConditionKind::Any, vec![expr("b"), expr("c")])),
                                ],
                            )),
                            expr("y==2"),
                        ],
                    ),
                    chat("nested"),
                ),
            ],
        };
        let af = save_meta(&profile, false).unwrap();
        assert!(af.contains("\tIF:\tNot Any\r\n\t\t\tExpr {a==1}\r\n"));
        let again = load_meta(&af);
        assert_eq!(
            shape(&profile.rules[0].condition),
            shape(&again.rules[0].condition)
        );
        assert_eq!(
            shape(&profile.rules[1].condition),
            shape(&again.rules[1].condition)
        );
        assert_eq!(
            shape(&again.rules[1].condition),
            "All(Expr x==1,Not(All(Expr a,Not(Any(Expr b,Expr c)))),Expr y==2)"
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: ALoneBraceInsideAStringIsRefused
    #[test]
    fn a_lone_brace_inside_a_string_is_refused() {
        for action_line in ["\t\tDO:\tChat {a}b}", "\t\tDO:\tChat {a{b}"] {
            let af = ["STATE: {Default}", "\tIF:\tAlways", action_line].join("\r\n") + "\r\n";
            assert!(
                try_load_meta(&af, Some(&NoSpells)).is_err(),
                "{action_line}"
            );
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: MobsInDistPriorityRoundTripsAllThreeNumbersDistinctly
    #[test]
    fn mobs_in_dist_priority_round_trips_all_three_numbers_distinctly() {
        let af = [
            "STATE: {Default}",
            "\tIF:\tMobsInDist_Priority 7 12.5 3",
            "\t\tDO:\tChat {seen}",
        ]
        .join("\r\n")
            + "\r\n";
        let profile = load_meta(&af);
        let condition = &profile.rules[0].condition;
        assert_eq!(
            condition.kind,
            MetaConditionKind::MonsterPriorityCountWithinDistance
        );
        assert_eq!(
            (
                condition.number,
                condition.secondary_number,
                condition.tertiary_number
            ),
            (7.0, 12.5, 3.0)
        );
        let reloaded = load_meta(&save_meta(&profile, false).unwrap());
        let condition = &reloaded.rules[0].condition;
        assert_eq!(
            (
                condition.number,
                condition.secondary_number,
                condition.tertiary_number
            ),
            (7.0, 12.5, 3.0)
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: SynthesizedGetOptFollowAndJumpFixtureRoundTrips
    #[test]
    fn synthesized_get_opt_follow_and_jump_fixture_round_trips() {
        let af = [
            "STATE: {Default}",
            "\tIF:\tAlways",
            "\t\tDO:\tDoAll",
            "\t\t\t\tGetOpt {AttackDistance} {myvar}",
            "\t\t\t\tSetOpt {AttackDistance} {myvar}",
            "NAV: myfollow follow",
            "\tflw 00001234 {Some Monster}",
        ]
        .join("\r\n")
            + "\r\n";
        let profile = load_meta(&af);
        assert_eq!(profile.rules.len(), 1);
        let get = &profile.rules[0].action.children[0];
        assert_eq!(get.kind, MetaActionKind::GetVtankOption);
        assert_eq!(get.text, "AttackDistance");
        assert_eq!(get.secondary_text, "myvar");
        assert_eq!(
            profile.rules[0].action.children[1].kind,
            MetaActionKind::SetVtankOption
        );

        let mut route = NavigationSettings::default();
        let error = try_load_nav(&af, &mut route, Some(&NoSpells)).unwrap_err();
        assert!(error.contains("STATE:"), "{error}");
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: FollowNavNodeParsesAsANavOnlyDocument
    #[test]
    fn follow_nav_node_parses_as_a_nav_only_document() {
        let route = load_nav("NAV: myfollow follow\r\n\tflw 00001234 {Some Monster}\r\n");
        assert_eq!(route.mode, RouteMode::Target);
        assert_eq!(route.follow_target_object_id, 0x0000_1234);
        assert_eq!(route.follow_target_name, "Some Monster");
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: JumpNodeLoadPreservesAuthoredChargeMillisecondsAboveAuthenticCeiling
    #[test]
    fn jump_node_load_preserves_authored_charge_milliseconds_above_authentic_ceiling() {
        let route = load_nav("NAV: j once\n\tjmp 1 2 3 90 {True} 5000");
        let waypoint = &route.waypoints[0];
        assert_eq!(waypoint.kind, RouteWaypointType::Jump);
        assert_eq!(waypoint.jump_charge_milliseconds, 5000);
        assert!(waypoint.jump_hold_shift);
        assert_eq!(waypoint.jump_heading_degrees, 90.0);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: RecallNodeResolvesItsSpellFromTheRecallTableWithAnEmptyCatalogue
    #[test]
    fn recall_node_resolves_its_spell_from_the_recall_table_with_an_empty_catalogue() {
        let route = load_nav("NAV: r once\n\trcl 59.5 -28.4 0.05 {Lifestone Recall}");
        let waypoint = &route.waypoints[0];
        assert_eq!(waypoint.kind, RouteWaypointType::Recall);
        assert_eq!(waypoint.recall_spell_id, 1635);
        assert_eq!(waypoint.recall, RouteRecallKind::LifestoneRecall);
        assert_eq!(waypoint.recall_spell_name, "Lifestone Recall");
        assert!(save_nav(&route).unwrap().contains("{Lifestone Recall}"));
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: EveryRecallDisplayNameResolvesToItsOwnKind
    #[test]
    fn every_recall_display_name_resolves_to_its_own_kind() {
        for kind in RouteRecallKind::ALL {
            let route = load_nav(&format!(
                "NAV: r once\r\n\trcl 1 2 3 {{{}}}",
                kind.display_name()
            ));
            assert_eq!(route.waypoints[0].recall, kind);
            assert_eq!(route.waypoints[0].recall_spell_id, kind.spell_id());
        }
    }

    /// Two inputs the C# fails on with an exception (an index error, an
    /// overflow) rather than a refusal; here they are refusals (register
    /// row FMT-1 in docs/divergences.md).
    #[test]
    fn inputs_the_csharp_throws_on_are_refused() {
        let if_at_end = "STATE: {A}\r\n\tIF:\tAlways\r\n\t\tDO:\tNone\r\n\tIF:\tNever\r\n";
        assert_eq!(
            try_load_meta(if_at_end, Some(&NoSpells)).unwrap_err(),
            "line 5: expected a 'DO:' line."
        );
        let mut route = NavigationSettings::default();
        assert!(
            try_load_nav(
                "NAV: j once\r\n\tpau 1 2 3 99999999999\r\n",
                &mut route,
                Some(&NoSpells)
            )
            .is_err()
        );
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: JumpNodeBelowClampPassesThroughUnchanged
    #[test]
    fn jump_node_below_clamp_passes_through_unchanged() {
        let route = load_nav("NAV: j once\n\tjmp 1 2 3 90 {False} 500");
        assert_eq!(route.waypoints[0].jump_charge_milliseconds, 500);
        assert!(!route.waypoints[0].jump_hold_shift);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: JumpNodeSaveThenLoadRoundTripsChargeMillisecondsAboveAuthenticCeiling
    #[test]
    fn jump_node_save_then_load_round_trips_charge_milliseconds_above_authentic_ceiling() {
        let mut source = NavigationSettings::with_mode(RouteMode::Once);
        source.waypoints.push(RouteWaypoint {
            kind: RouteWaypointType::Jump,
            jump_heading_degrees: 90.0,
            jump_hold_shift: true,
            jump_charge_milliseconds: 5000,
            ..RouteWaypoint::default()
        });
        let route = load_nav(&save_nav(&source).unwrap());
        assert_eq!(route.waypoints[0].jump_charge_milliseconds, 5000);
    }

    struct Recalls(Vec<(u32, String)>);

    impl SpellLookup for Recalls {
        fn find_by_name(&self, _name: &str) -> Option<u32> {
            None
        }
        fn known_spells(&self) -> Vec<(u32, String)> {
            self.0.clone()
        }
        fn name_of(&self, spell_id: u32) -> Option<String> {
            self.0
                .iter()
                .find(|(id, _)| *id == spell_id)
                .map(|(_, name)| name.clone())
        }
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/MetafSerializerTests.cs :: RecallNodeRoundTripsByNameAndResolvesTheRealSpellIdFromTheCatalog
    #[test]
    fn recall_node_round_trips_by_name_and_resolves_the_real_spell_id_from_the_catalog() {
        let kinds = [
            RouteRecallKind::PrimaryPortalRecall,
            RouteRecallKind::MountLetheRecall,
            RouteRecallKind::EldrytchWebStrongholdRecall,
        ];
        let mut source = NavigationSettings::with_mode(RouteMode::Once);
        for kind in kinds {
            source.waypoints.push(RouteWaypoint {
                kind: RouteWaypointType::Recall,
                recall: kind,
                recall_spell_name: kind.display_name().to_owned(),
                recall_spell_id: kind.spell_id(),
                ..RouteWaypoint::default()
            });
        }
        let af = save_nav(&source).unwrap();
        let catalog = Recalls(
            kinds
                .iter()
                .map(|kind| (kind.spell_id(), kind.display_name().to_owned()))
                .collect(),
        );
        let mut target = NavigationSettings::default();
        try_load_nav(&af, &mut target, Some(&catalog)).unwrap();
        assert_eq!(target.waypoints.len(), kinds.len());
        for (waypoint, kind) in target.waypoints.iter().zip(kinds) {
            assert_eq!(waypoint.recall_spell_name, kind.display_name());
            assert_eq!(waypoint.recall_spell_id, kind.spell_id());
        }
    }
}

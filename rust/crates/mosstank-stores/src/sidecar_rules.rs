//! The side-car's loot rules (`InventoryLootRules`), written and read by
//! hand.
//!
//! The C# keeps every loot rule in the settings side-car, so a real
//! player's 1,700-rule profile makes it a 1.4 MB document: through the
//! JSON value tree that costs some 400 M instructions to write and 600 M
//! to read, several plugin calls' worth. The rules here are written
//! straight into the text, byte for byte as the document writer lays them
//! out (System.Text.Json indented, `JavaScriptEncoder.Default`), and read
//! back by a reader that takes exactly that text and nothing else: anything
//! else (another tool's spacing, a different escape, a number the writer
//! would not write) makes it give up, and the whole document is read the
//! usual way. Tests hold both to the document writer and reader.

use mosstank_formats::utl::{LootAction, LootRule};

use crate::sidecar::LootRuleDocument;

/// The line break the document writer puts between indented lines.
const NEW_LINE: &str = "\r\n";

/// The member's name as the document writer writes it, with what comes
/// before it at the top level (its line) and after it (`": "`).
pub(crate) const MEMBER: &str = "\r\n  \"InventoryLootRules\": ";

/// The depth the member's value is written at (a member of the document).
pub(crate) const DEPTH: usize = 1;

fn line(out: &mut String, depth: usize) {
    out.push_str(NEW_LINE);
    for _ in 0..depth {
        out.push_str("  ");
    }
}

/// `value` in decimal, as the writer writes a number.
fn push_int(out: &mut String, value: i32) {
    let value = i64::from(value);
    if value < 0 {
        out.push('-');
    }
    let mut digits = [0u8; 20];
    let mut rest = value.unsigned_abs();
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b'0' + u8::try_from(rest % 10).unwrap_or(0);
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    out.push_str(std::str::from_utf8(&digits[start..]).unwrap_or_default());
}

/// [`plain`] for every byte.
const PLAIN: [bool; 256] = {
    let mut table = [false; 256];
    let mut byte = 0;
    while byte < 256 {
        #[allow(clippy::cast_possible_truncation)]
        {
            table[byte] = plain(byte as u8);
        }
        byte += 1;
    }
    table
};

/// Whether the writer writes `byte` (ASCII) as itself inside a string.
const fn plain(byte: u8) -> bool {
    byte >= 0x20
        && byte <= 0x7E
        && !matches!(
            byte,
            b'"' | b'\\' | b'&' | b'\'' | b'+' | b'<' | b'>' | b'`'
        )
}

fn push_hex4(out: &mut String, unit: u16) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push_str("\\u");
    for shift in [12, 8, 4, 0] {
        out.push(char::from(HEX[usize::from((unit >> shift) & 0xF)]));
    }
}

/// One character as the writer writes it inside a string.
fn push_char(out: &mut String, c: char) {
    match c {
        '\\' => out.push_str("\\\\"),
        '\u{8}' => out.push_str("\\b"),
        '\t' => out.push_str("\\t"),
        '\n' => out.push_str("\\n"),
        '\u{c}' => out.push_str("\\f"),
        '\r' => out.push_str("\\r"),
        '"' | '&' | '\'' | '+' | '<' | '>' | '`' | '\u{7f}' => push_hex4(out, c as u16),
        c if (c as u32) < 0x20 || (c as u32) > 0x7E => {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                push_hex4(out, *unit);
            }
        }
        c => out.push(c),
    }
}

fn push_string(out: &mut String, text: &str) {
    out.push('"');
    let bytes = text.as_bytes();
    let mut start = 0;
    // Runs of characters written as they are go in one copy.
    while let Some(offset) = bytes[start..]
        .iter()
        .position(|byte| !PLAIN[usize::from(*byte)])
    {
        let index = start + offset;
        out.push_str(&text[start..index]);
        let c = text[index..].chars().next().unwrap_or_default();
        push_char(out, c);
        start = index + c.len_utf8();
    }
    out.push_str(&text[start..]);
    out.push('"');
}

fn push_string_or_null(out: &mut String, text: Option<&str>) {
    match text {
        Some(text) => push_string(out, text),
        None => out.push_str("null"),
    }
}

/// A rule's requirements, borrowed: each one's type and payload (`None`
/// for a `null` entry).
type Requirements<'a> = Box<dyn Iterator<Item = Option<(i32, Option<&'a str>)>> + 'a>;

/// One rule as the side-car holds it, borrowed: from a document's rule or
/// straight from a live loot rule (`LootRuleDocument::from_rule`).
struct RuleView<'a> {
    name: Option<&'a str>,
    expression: Option<&'a str>,
    action: i32,
    keep_count: i32,
    priority: i32,
    custom_expression: Option<&'a str>,
    /// The requirements' count and each one's type and payload.
    requirements: Option<(usize, Requirements<'a>)>,
    has_imported_requirements: bool,
}

impl<'a> RuleView<'a> {
    fn of_document(rule: &'a LootRuleDocument) -> Self {
        Self {
            name: rule.name.as_deref(),
            expression: rule.expression.as_deref(),
            action: rule.action as i32,
            keep_count: rule.keep_count,
            priority: rule.priority,
            custom_expression: rule.custom_expression.as_deref(),
            requirements: rule.requirements.as_ref().map(|items| {
                let iter: Requirements<'a> = Box::new(items.iter().map(|item| {
                    item.as_ref()
                        .map(|(kind, payload)| (*kind, payload.as_deref()))
                }));
                (items.len(), iter)
            }),
            has_imported_requirements: rule.has_imported_requirements,
        }
    }

    fn of_rule(rule: &'a LootRule) -> Self {
        let items = &rule.vtank_requirements;
        let iter: Requirements<'a> = Box::new(
            items
                .iter()
                .map(|requirement| Some((requirement.kind, Some(requirement.payload.as_str())))),
        );
        Self {
            name: Some(&rule.name),
            expression: Some(rule.expression()),
            action: rule.action as i32,
            keep_count: rule.keep_count,
            priority: rule.priority,
            custom_expression: Some(&rule.custom_expression),
            requirements: Some((items.len(), iter)),
            has_imported_requirements: rule.has_imported_requirements,
        }
    }
}

/// The member's value: the rules as the document writer writes them at
/// [`DEPTH`].
pub(crate) fn write(out: &mut String, rules: Option<&Vec<Option<LootRuleDocument>>>) {
    match rules {
        None => out.push_str("null"),
        Some(rules) => write_views(
            out,
            rules.len(),
            rules
                .iter()
                .map(|rule| rule.as_ref().map(RuleView::of_document)),
        ),
    }
}

/// [`write`] of the documents the live loot rules are captured as, written
/// from the rules themselves.
pub(crate) fn write_live(out: &mut String, rules: &[LootRule]) {
    write_views(
        out,
        rules.len(),
        rules.iter().map(|rule| Some(RuleView::of_rule(rule))),
    );
}

/// [`write_live`] a run at a time: the text grows by about the bytes each
/// [`Self::step`] is given, and is [`write_live`]'s once it is done. The
/// rules are hashed as they are written, so [`Self::finish`] can tell
/// whether they changed while it was under way.
#[derive(Debug, Clone, Default)]
pub(crate) struct LiveWrite {
    text: String,
    next: usize,
    hasher: crate::fast_hash::FastHasher,
}

impl LiveWrite {
    /// Writes on from where the last step stopped, until the text has
    /// grown by `bytes` (a rule at least) or every rule is written: true
    /// then.
    pub(crate) fn step(&mut self, rules: &[LootRule], bytes: usize) -> bool {
        use std::hash::{Hash, Hasher};
        if self.next == 0 && self.text.is_empty() {
            self.text.push('[');
            // As a slice hashes: its length, then each rule.
            self.hasher.write_usize(rules.len());
        }
        let stop = self.text.len().saturating_add(bytes);
        while let Some(rule) = rules.get(self.next) {
            if self.next > 0 {
                self.text.push(',');
            }
            line(&mut self.text, DEPTH + 1);
            write_rule(&mut self.text, RuleView::of_rule(rule), DEPTH + 1);
            rule.hash(&mut self.hasher);
            self.next += 1;
            if self.text.len() >= stop {
                break;
            }
        }
        if self.next < rules.len() {
            return false;
        }
        if !rules.is_empty() {
            line(&mut self.text, DEPTH);
        }
        self.text.push(']');
        true
    }

    /// The text written, and the hash of the rules as they were written
    /// (`fast_hash::hash` of the rules, when they did not change).
    pub(crate) fn finish(self) -> (u64, String) {
        use std::hash::Hasher;
        (self.hasher.finish(), self.text)
    }
}

fn write_views<'a>(
    out: &mut String,
    count: usize,
    rules: impl Iterator<Item = Option<RuleView<'a>>>,
) {
    out.push('[');
    for (index, rule) in rules.enumerate() {
        if index > 0 {
            out.push(',');
        }
        line(out, DEPTH + 1);
        match rule {
            Some(rule) => write_rule(out, rule, DEPTH + 1),
            None => out.push_str("null"),
        }
    }
    if count > 0 {
        line(out, DEPTH);
    }
    out.push(']');
}

fn member(out: &mut String, first: bool, depth: usize, name: &str) {
    if !first {
        out.push(',');
    }
    line(out, depth);
    out.push('"');
    out.push_str(name);
    out.push_str("\": ");
}

fn write_rule(out: &mut String, rule: RuleView<'_>, depth: usize) {
    let inner = depth + 1;
    out.push('{');
    member(out, true, inner, "Name");
    push_string_or_null(out, rule.name);
    member(out, false, inner, "Expression");
    push_string_or_null(out, rule.expression);
    member(out, false, inner, "Action");
    push_int(out, rule.action);
    member(out, false, inner, "KeepCount");
    push_int(out, rule.keep_count);
    member(out, false, inner, "Priority");
    push_int(out, rule.priority);
    member(out, false, inner, "CustomExpression");
    push_string_or_null(out, rule.custom_expression);
    member(out, false, inner, "Requirements");
    match rule.requirements {
        None => out.push_str("null"),
        Some((count, items)) => {
            out.push('[');
            for (index, item) in items.enumerate() {
                if index > 0 {
                    out.push(',');
                }
                line(out, inner + 1);
                match item {
                    None => out.push_str("null"),
                    Some((kind, payload)) => {
                        out.push('{');
                        member(out, true, inner + 2, "Type");
                        push_int(out, kind);
                        member(out, false, inner + 2, "Payload");
                        push_string_or_null(out, payload);
                        line(out, inner + 1);
                        out.push('}');
                    }
                }
            }
            if count > 0 {
                line(out, inner);
            }
            out.push(']');
        }
    }
    member(out, false, inner, "HasImportedRequirements");
    out.push_str(if rule.has_imported_requirements {
        "true"
    } else {
        "false"
    });
    line(out, depth);
    out.push('}');
}

/// Reads exactly what [`write`] writes.
struct Reader<'a> {
    text: &'a [u8],
    source: &'a str,
    at: usize,
    /// Where a number or an escape is written again to compare.
    scratch: String,
}

impl<'a> Reader<'a> {
    fn literal(&mut self, expected: &str) -> Option<()> {
        let end = self.at.checked_add(expected.len())?;
        (self.text.get(self.at..end)? == expected.as_bytes()).then(|| self.at = end)
    }

    fn try_literal(&mut self, expected: &str) -> bool {
        self.literal(expected).is_some()
    }

    fn line(&mut self, depth: usize) -> Option<()> {
        self.literal(NEW_LINE)?;
        for _ in 0..depth {
            self.literal("  ")?;
        }
        Some(())
    }

    fn member(&mut self, first: bool, depth: usize, name: &str) -> Option<()> {
        if !first {
            self.literal(",")?;
        }
        self.line(depth)?;
        self.literal("\"")?;
        self.literal(name)?;
        self.literal("\": ")
    }

    /// A number as the writer writes an `int`.
    fn int(&mut self) -> Option<i32> {
        let start = self.at;
        if self.text.get(self.at) == Some(&b'-') {
            self.at += 1;
        }
        let digits = self.at;
        while self.text.get(self.at).is_some_and(u8::is_ascii_digit) {
            self.at += 1;
        }
        let count = self.at - digits;
        // No leading zero, no "-0": the writer's own text only.
        if count == 0 || (count > 1 && self.text[digits] == b'0') {
            return None;
        }
        let value: i32 = self.source.get(start..self.at)?.parse().ok()?;
        self.scratch.clear();
        push_int(&mut self.scratch, value);
        (self.scratch.as_bytes() == &self.text[start..self.at]).then_some(value)
    }

    fn hex4(&mut self) -> Option<u16> {
        let digits = self.text.get(self.at..self.at + 4)?;
        let mut unit = 0u16;
        for digit in digits {
            let value = match digit {
                b'0'..=b'9' => digit - b'0',
                b'A'..=b'F' => digit - b'A' + 10,
                _ => return None,
            };
            unit = unit * 16 + u16::from(value);
        }
        self.at += 4;
        Some(unit)
    }

    /// A string as the writer writes one: every character in the one form
    /// the writer gives it.
    fn string(&mut self) -> Option<String> {
        self.literal("\"")?;
        let mut value = String::new();
        let mut run = self.at;
        loop {
            // The run of characters written as they are, at once.
            let rest = self.text.get(self.at..)?;
            self.at += rest
                .iter()
                .position(|byte| !PLAIN[usize::from(*byte)])
                .unwrap_or(rest.len());
            let byte = *self.text.get(self.at)?;
            value.push_str(self.source.get(run..self.at)?);
            if byte == b'"' {
                self.at += 1;
                return Some(value);
            }
            if byte != b'\\' {
                return None;
            }
            let start = self.at;
            let escape = *self.text.get(self.at + 1)?;
            self.at += 2;
            let c = match escape {
                b'\\' => '\\',
                b'b' => '\u{8}',
                b't' => '\t',
                b'n' => '\n',
                b'f' => '\u{c}',
                b'r' => '\r',
                b'u' => {
                    let unit = self.hex4()?;
                    if (0xD800..0xDC00).contains(&unit) {
                        self.literal("\\u")?;
                        let low = self.hex4()?;
                        if !(0xDC00..0xE000).contains(&low) {
                            return None;
                        }
                        let code = 0x10000
                            + ((u32::from(unit) - 0xD800) << 10)
                            + (u32::from(low) - 0xDC00);
                        char::from_u32(code)?
                    } else {
                        char::from_u32(u32::from(unit))?
                    }
                }
                _ => return None,
            };
            self.scratch.clear();
            push_char(&mut self.scratch, c);
            if self.scratch.as_bytes() != &self.text[start..self.at] {
                return None;
            }
            value.push(c);
            run = self.at;
        }
    }

    fn string_or_null(&mut self) -> Option<Option<String>> {
        if self.try_literal("null") {
            return Some(None);
        }
        self.string().map(Some)
    }

    fn rule(&mut self, depth: usize) -> Option<LootRuleDocument> {
        let inner = depth + 1;
        self.literal("{")?;
        self.member(true, inner, "Name")?;
        let name = self.string_or_null()?;
        self.member(false, inner, "Expression")?;
        let expression = self.string_or_null()?;
        self.member(false, inner, "Action")?;
        let action = LootAction::from_i32(self.int()?)?;
        self.member(false, inner, "KeepCount")?;
        let keep_count = self.int()?;
        self.member(false, inner, "Priority")?;
        let priority = self.int()?;
        self.member(false, inner, "CustomExpression")?;
        let custom_expression = self.string_or_null()?;
        self.member(false, inner, "Requirements")?;
        let requirements = if self.try_literal("null") {
            None
        } else {
            self.literal("[")?;
            let mut items = Vec::new();
            if !self.try_literal("]") {
                loop {
                    self.line(inner + 1)?;
                    if self.try_literal("null") {
                        items.push(None);
                    } else {
                        self.literal("{")?;
                        self.member(true, inner + 2, "Type")?;
                        let kind = self.int()?;
                        self.member(false, inner + 2, "Payload")?;
                        let payload = self.string_or_null()?;
                        self.line(inner + 1)?;
                        self.literal("}")?;
                        items.push(Some((kind, payload)));
                    }
                    if !self.try_literal(",") {
                        break;
                    }
                }
                self.line(inner)?;
                self.literal("]")?;
            }
            Some(items)
        };
        self.member(false, inner, "HasImportedRequirements")?;
        let has_imported_requirements = if self.try_literal("true") {
            true
        } else {
            self.literal("false")?;
            false
        };
        self.line(depth)?;
        self.literal("}")?;
        Some(LootRuleDocument {
            name,
            expression,
            action,
            keep_count,
            priority,
            custom_expression,
            requirements,
            has_imported_requirements,
        })
    }
}

/// The member's value written by [`write`] at `at` in `text`: the rules
/// and where the value ends; `None` for any text [`write`] would not have
/// written.
pub(crate) fn read(
    text: &str,
    at: usize,
) -> Option<(Option<Vec<Option<LootRuleDocument>>>, usize)> {
    let mut reader = Reader {
        text: text.as_bytes(),
        source: text,
        at,
        scratch: String::with_capacity(16),
    };
    if reader.try_literal("null") {
        return Some((None, reader.at));
    }
    reader.literal("[")?;
    let mut rules = Vec::new();
    if !reader.try_literal("]") {
        loop {
            reader.line(DEPTH + 1)?;
            if reader.try_literal("null") {
                rules.push(None);
            } else {
                rules.push(Some(reader.rule(DEPTH + 1)?));
            }
            if !reader.try_literal(",") {
                break;
            }
        }
        reader.line(DEPTH)?;
        reader.literal("]")?;
    }
    Some((Some(rules), reader.at))
}

/// A read of the member's value a run of rules at a time (a large profile
/// has thousands): the rules as the side-car's `Apply` takes them in, each
/// document made a loot rule as it is read.
#[derive(Debug, Clone)]
pub(crate) struct RulesRead {
    at: usize,
    started: bool,
    rules: Option<Vec<LootRule>>,
    end: Option<usize>,
}

/// How a step of a [`RulesRead`] went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RulesStep {
    /// Rules remain.
    More,
    /// The value is read; it ends at this byte.
    Done(usize),
    /// Not what [`write`] writes: read the document the usual way.
    GiveUp,
}

impl RulesRead {
    /// A read of the value at byte `at`.
    pub(crate) fn new(at: usize) -> Self {
        Self {
            at,
            started: false,
            rules: None,
            end: None,
        }
    }

    /// Where the read has got, in bytes of the text.
    pub(crate) fn position(&self) -> usize {
        self.at
    }

    /// The rules read (`None` for a `null` value), once done.
    pub(crate) fn take_rules(&mut self) -> Option<Vec<LootRule>> {
        self.rules.take()
    }

    /// Reads on through `text` (the same text every step) until at least
    /// `bytes` more are read and a rule ends.
    pub(crate) fn step(&mut self, text: &str, bytes: usize) -> RulesStep {
        if let Some(end) = self.end {
            return RulesStep::Done(end);
        }
        let mut reader = Reader {
            text: text.as_bytes(),
            source: text,
            at: self.at,
            scratch: String::with_capacity(16),
        };
        let outcome = self.read_on(&mut reader, bytes);
        self.at = reader.at;
        match outcome {
            None => RulesStep::GiveUp,
            Some(false) => RulesStep::More,
            Some(true) => {
                self.end = Some(self.at);
                RulesStep::Done(self.at)
            }
        }
    }

    /// Whether the value is read; `None` when it is not as written.
    fn read_on(&mut self, reader: &mut Reader<'_>, bytes: usize) -> Option<bool> {
        let start = reader.at;
        if !self.started {
            self.started = true;
            if reader.try_literal("null") {
                return Some(true);
            }
            reader.literal("[")?;
            self.rules = Some(Vec::new());
            if reader.try_literal("]") {
                return Some(true);
            }
        } else if !reader.try_literal(",") {
            reader.line(DEPTH)?;
            reader.literal("]")?;
            return Some(true);
        }
        let rules = self.rules.as_mut()?;
        loop {
            reader.line(DEPTH + 1)?;
            if !reader.try_literal("null") {
                rules.push(reader.rule(DEPTH + 1)?.into_rule());
            }
            if reader.at - start >= bytes {
                return Some(false);
            }
            if !reader.try_literal(",") {
                break;
            }
        }
        reader.line(DEPTH)?;
        reader.literal("]")?;
        Some(true)
    }
}

//! VTClassic loot profiles (`.utl`): rules with VTank requirement records,
//! the salvage-combine block, and blocks of other kinds kept as they are.
//!
//! Version 1 (`UTL`, `1`, rule count) gives every requirement a length line;
//! version 0 (no header) has fixed line counts per requirement type. Lines
//! are written with `\r\n`; lengths count UTF-16 code units of the text as
//! written.

use std::collections::BTreeMap;

use mosstank_dotnet::text::casing;
use mosstank_dotnet::try_parse_int;
use openac_plugin_compat::text::utf16;

use crate::FormatError;

const HEADER: &str = "UTL";
const CURRENT_VERSION: i32 = 1;
const SALVAGE_BLOCK: &str = "SalvageCombine";
const DISABLED_RULE_TYPE: i32 = 9999;
const NEW_LINE: &str = "\r\n";

/// The block MossTank adds to keep its own rule expressions.
pub const MOSSTANK_RULES_BLOCK_TYPE: &str = "MossTankRuleExpressions";

/// What a loot rule does with a matching item (VTank's action numbers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(i32)]
pub enum LootAction {
    /// 0
    NoLoot = 0,
    /// 1
    #[default]
    Keep = 1,
    /// 2
    Salvage = 2,
    /// 3
    Sell = 3,
    /// 4
    Read = 4,
    /// 5
    User1 = 5,
    /// 6
    User2 = 6,
    /// 7
    User3 = 7,
    /// 8
    User4 = 8,
    /// 9
    User5 = 9,
    /// 10: keep up to a count.
    KeepUpTo = 10,
    /// 11 (MossTank's own; never in a file).
    ManaStone = 11,
    /// 12 (MossTank's own; never in a file).
    ManaTank = 12,
}

impl LootAction {
    /// The action with VTank's number, or `None`.
    #[must_use]
    pub fn from_i32(value: i32) -> Option<Self> {
        Some(match value {
            0 => Self::NoLoot,
            1 => Self::Keep,
            2 => Self::Salvage,
            3 => Self::Sell,
            4 => Self::Read,
            5 => Self::User1,
            6 => Self::User2,
            7 => Self::User3,
            8 => Self::User4,
            9 => Self::User5,
            10 => Self::KeepUpTo,
            11 => Self::ManaStone,
            12 => Self::ManaTank,
            _ => return None,
        })
    }
}

/// One VTank requirement record: its type and its payload text as read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct VtankLootRequirement {
    /// The requirement type.
    pub kind: i32,
    /// The payload lines, each ended with `\r\n`.
    pub payload: String,
}

/// A loot rule, as the profile files hold it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LootRule {
    /// The rule's name.
    pub name: String,
    // The default `*` is not copied for every rule a file holds.
    expression: std::borrow::Cow<'static, str>,
    /// What the rule does.
    pub action: LootAction,
    /// How many to keep for [`LootAction::KeepUpTo`].
    pub keep_count: i32,
    /// Priority (VTank's; kept and written back).
    pub priority: i32,
    /// The VTClassic editor's custom expression line.
    pub custom_expression: String,
    /// VTank requirement records; when present they decide the rule.
    pub vtank_requirements: Vec<VtankLootRequirement>,
    /// The requirements came from a file (an empty list then means "always").
    pub has_imported_requirements: bool,
}

impl Default for LootRule {
    fn default() -> Self {
        Self {
            name: "Rule".to_owned(),
            expression: std::borrow::Cow::Borrowed("*"),
            action: LootAction::Keep,
            keep_count: 1,
            priority: 0,
            custom_expression: String::new(),
            vtank_requirements: Vec::new(),
            has_imported_requirements: false,
        }
    }
}

impl LootRule {
    /// MossTank's own match expression (`*` matches everything).
    #[must_use]
    pub fn expression(&self) -> &str {
        &self.expression
    }

    /// Sets the expression: blank becomes `*`, anything else is trimmed.
    pub fn set_expression(&mut self, value: &str) {
        self.expression = if casing::is_white_space_only(value) {
            std::borrow::Cow::Borrowed("*")
        } else {
            std::borrow::Cow::Owned(value.trim().to_owned())
        };
    }

    /// A rule with this name and expression, otherwise default.
    #[must_use]
    pub fn named(name: &str, expression: &str) -> Self {
        let mut rule = Self {
            name: name.to_owned(),
            ..Self::default()
        };
        rule.set_expression(expression);
        rule
    }
}

/// The salvage-combine block: how bags of each material combine.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VtankSalvageCombineSettings {
    /// The default combine ranges.
    pub default_combine_string: String,
    /// Combine ranges by material id.
    pub material_combine_strings: BTreeMap<i32, String>,
    /// Value-mode values by material id.
    pub material_value_mode_values: BTreeMap<i32, i32>,
}

impl Default for VtankSalvageCombineSettings {
    fn default() -> Self {
        // VTank's defaults: these materials combine "1-10".
        const MATERIALS: [i32; 25] = [
            10, 14, 16, 17, 18, 19, 22, 25, 29, 30, 36, 37, 41, 47, 35, 27, 26, 21, 15, 13, 50, 49,
            34, 52, 51,
        ];
        Self {
            default_combine_string: "1-6, 7-8, 9, 10".to_owned(),
            material_combine_strings: MATERIALS
                .iter()
                .map(|material| (*material, "1-10".to_owned()))
                .collect(),
            material_value_mode_values: BTreeMap::new(),
        }
    }
}

/// A block of a kind this reader does not interpret, kept to be written
/// back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VtankLootExtraBlock {
    /// The block's type line.
    pub kind: String,
    /// Its characters.
    pub payload: String,
}

/// A whole loot profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VtankLootProfile {
    /// The version the file was read as (0 or 1).
    pub source_version: i32,
    /// The rules, in order.
    pub rules: Vec<LootRule>,
    /// The salvage-combine settings.
    pub salvage_combine: VtankSalvageCombineSettings,
    /// Other blocks, in order.
    pub unknown_blocks: Vec<VtankLootExtraBlock>,
}

impl Default for VtankLootProfile {
    fn default() -> Self {
        Self {
            source_version: 1,
            rules: Vec::new(),
            salvage_combine: VtankSalvageCombineSettings::default(),
            unknown_blocks: Vec::new(),
        }
    }
}

/// A read that may have stopped part way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VtankLootProfileReadResult {
    /// What was read (all of it when complete).
    pub profile: VtankLootProfile,
    /// Why the read stopped; empty when complete.
    pub error: String,
    /// Whether the whole file was read.
    pub is_complete: bool,
}

/// Reads a profile, all or nothing.
///
/// # Errors
/// Why the text is not a whole profile (the C# `TryRead`'s message).
pub fn try_read(source: Option<&str>) -> Result<VtankLootProfile, String> {
    let result = read_allowing_partial(source);
    if result.is_complete {
        Ok(result.profile)
    } else {
        Err(result.error)
    }
}

/// Reads as much of a profile as is well formed.
#[must_use]
pub fn read_allowing_partial(source: Option<&str>) -> VtankLootProfileReadResult {
    let source = source.unwrap_or_default();
    let mut read = ProfileRead::new();
    loop {
        if let Some(result) = read.step(source, usize::MAX) {
            return result;
        }
    }
}

/// A read of a profile a run of its rules at a time (a large profile has
/// thousands, more than one plugin call should parse): each
/// [`Self::step`] reads on from where the last stopped, and the result is
/// the one [`read_allowing_partial`] gives for the same text.
#[derive(Debug, Clone, Default)]
pub struct ProfileRead {
    /// Where the next step reads from, in bytes.
    position: usize,
    /// The header, once read: the version and the rule count.
    header: Option<(i32, usize)>,
    /// What has been read so far.
    profile: VtankLootProfile,
    /// The read is over.
    done: bool,
}

impl ProfileRead {
    /// A read from the start.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How far the read has got, in bytes of the text.
    #[must_use]
    pub fn position(&self) -> usize {
        self.position
    }

    /// Reads on through `source` (the same text on every step) until at
    /// least `bytes` more have been read and a rule ends, or the read is
    /// over; the result once it is (`None` while rules remain). The blocks
    /// after the rules are read in the step that reads the last rule.
    pub fn step(&mut self, source: &str, bytes: usize) -> Option<VtankLootProfileReadResult> {
        if self.done {
            return None;
        }
        if source.is_empty() {
            return Some(self.finish(Err(FormatError(
                "The VTClassic loot profile is empty.".to_owned(),
            ))));
        }
        let mut reader = CharacterReader {
            source,
            position: self.position,
        };
        let outcome = self.read_on(&mut reader, bytes);
        self.position = reader.position;
        match outcome {
            Ok(false) => None,
            Ok(true) => Some(self.finish(Ok(()))),
            Err(error) => Some(self.finish(Err(error))),
        }
    }

    fn finish(&mut self, outcome: Result<(), FormatError>) -> VtankLootProfileReadResult {
        self.done = true;
        let profile = std::mem::take(&mut self.profile);
        match outcome {
            Ok(()) => VtankLootProfileReadResult {
                profile,
                error: String::new(),
                is_complete: true,
            },
            Err(FormatError(message)) => VtankLootProfileReadResult {
                profile,
                error: message,
                is_complete: false,
            },
        }
    }

    /// Whether the whole profile has been read.
    fn read_on(
        &mut self,
        reader: &mut CharacterReader<'_>,
        bytes: usize,
    ) -> Result<bool, FormatError> {
        let (version, count) = match self.header {
            Some(header) => header,
            None => {
                let header = read_header(reader)?;
                self.profile.source_version = header.0;
                self.header = Some(header);
                header
            }
        };
        let start = reader.position;
        while self.profile.rules.len() < count {
            let index = self.profile.rules.len();
            let rule = read_rule(reader, version, index)?;
            self.profile.rules.push(rule);
            if self.profile.rules.len() < count && reader.position - start >= bytes {
                return Ok(false);
            }
        }
        read_blocks(reader, &mut self.profile)?;
        Ok(true)
    }
}

/// The version and the rule count.
fn read_header(reader: &mut CharacterReader<'_>) -> Result<(i32, usize), FormatError> {
    let first = reader.read_line()?;
    if first == HEADER {
        let version = parse_int(reader.read_line()?, Field::Named("profile version"))?;
        if !(0..=CURRENT_VERSION).contains(&version) {
            return Err(FormatError(format!(
                "VTClassic loot profile version {version} is not supported."
            )));
        }
        Ok((
            version,
            parse_count(reader.read_line()?, Field::Named("rule count"), 100_000)?,
        ))
    } else {
        Ok((0, parse_count(first, Field::Named("rule count"), 100_000)?))
    }
}

/// The blocks after the rules, to the end of the text.
fn read_blocks(
    reader: &mut CharacterReader<'_>,
    profile: &mut VtankLootProfile,
) -> Result<(), FormatError> {
    while !reader.at_end() {
        let block_type = reader.read_line()?;
        if block_type.is_empty() && reader.at_end() {
            break;
        }
        let length = parse_count(
            reader.read_line()?,
            Field::Block(block_type),
            16 * 1024 * 1024,
        )?;
        if block_type == SALVAGE_BLOCK {
            profile.salvage_combine = read_salvage(reader)?;
        } else {
            let payload = reader.read_characters(length)?;
            profile.unknown_blocks.push(VtankLootExtraBlock {
                kind: block_type.to_owned(),
                payload: payload.to_owned(),
            });
        }
    }
    Ok(())
}

/// Writes a profile as version 1.
#[must_use]
pub fn write(profile: &VtankLootProfile) -> String {
    write_parts(
        &profile.rules,
        &profile.salvage_combine,
        &profile.unknown_blocks,
    )
}

/// [`write`] of a profile with these rules, salvage settings and other
/// blocks, the rules written where they are (a large profile has
/// thousands).
#[must_use]
pub fn write_parts(
    rules: &[LootRule],
    salvage: &VtankSalvageCombineSettings,
    unknown_blocks: &[VtankLootExtraBlock],
) -> String {
    let mut output = String::with_capacity(64 + rules.len() * 256);
    append_line(&mut output, HEADER);
    append_number_line(&mut output, i64::from(CURRENT_VERSION));
    append_number_line(&mut output, count(rules.len()));
    for rule in rules {
        write_rule(&mut output, rule);
    }

    write_tail(&mut output, salvage, unknown_blocks);
    output
}

/// What follows the rules: the salvage block and the other blocks.
fn write_tail(
    output: &mut String,
    salvage: &VtankSalvageCombineSettings,
    unknown_blocks: &[VtankLootExtraBlock],
) {
    write_block(output, SALVAGE_BLOCK, &write_salvage(salvage));
    for block in unknown_blocks {
        if block.kind.is_empty() || block.kind == SALVAGE_BLOCK {
            continue;
        }
        write_block(output, &block.kind, &block.payload);
    }
}

/// [`write_parts`] a run at a time: the header and the rules, about the
/// bytes each [`Self::step`] is given, then the rest at [`Self::finish`].
/// Each rule written is hashed into the caller's hasher, the count first
/// (as a slice of them hashes).
#[derive(Debug, Clone, Default)]
pub struct PartsWrite {
    text: String,
    next: usize,
}

impl PartsWrite {
    /// Writes on from where the last step stopped until the text has grown
    /// by `bytes` (a rule at least): true once every rule is written.
    pub fn step<H: std::hash::Hasher>(
        &mut self,
        rules: &[LootRule],
        bytes: usize,
        hasher: &mut H,
    ) -> bool {
        use std::hash::Hash;
        if self.next == 0 && self.text.is_empty() {
            append_line(&mut self.text, HEADER);
            append_number_line(&mut self.text, i64::from(CURRENT_VERSION));
            append_number_line(&mut self.text, count(rules.len()));
            hasher.write_usize(rules.len());
        }
        let stop = self.text.len().saturating_add(bytes);
        while let Some(rule) = rules.get(self.next) {
            write_rule(&mut self.text, rule);
            rule.hash(hasher);
            self.next += 1;
            if self.text.len() >= stop {
                break;
            }
        }
        self.next >= rules.len()
    }

    /// The whole text, as [`write_parts`] writes it, once every rule is
    /// written (and the rules did not change meanwhile).
    #[must_use]
    pub fn finish(
        mut self,
        salvage: &VtankSalvageCombineSettings,
        unknown_blocks: &[VtankLootExtraBlock],
    ) -> String {
        write_tail(&mut self.text, salvage, unknown_blocks);
        self.text
    }
}

fn read_rule(
    reader: &mut CharacterReader<'_>,
    version: i32,
    rule_index: usize,
) -> Result<LootRule, FormatError> {
    let number = rule_index + 1;
    let name = reader.read_line()?;
    let custom_expression = if version >= 1 {
        reader.read_line()?
    } else {
        ""
    };
    let mut fields = reader.read_line()?.split(';');
    let (Some(priority), Some(action_value)) = (fields.next(), fields.next()) else {
        return Err(FormatError(format!(
            "Loot rule {number} has an invalid header."
        )));
    };
    let priority = parse_int(priority, Field::Rule(number, "priority"))?;
    let action_value = parse_int(action_value, Field::Rule(number, "action"))?;
    let action = LootAction::from_i32(action_value)
        .filter(|_| (0..=10).contains(&action_value))
        .ok_or_else(|| FormatError(format!("Loot rule {number} has action {action_value}.")))?;

    let mut rule = LootRule {
        name: if casing::is_white_space_only(name) {
            format!("Rule {number}")
        } else {
            name.to_owned()
        },
        expression: std::borrow::Cow::Borrowed("*"),
        custom_expression: custom_expression.to_owned(),
        action,
        keep_count: 1,
        priority,
        vtank_requirements: Vec::with_capacity(fields.clone().count()),
        has_imported_requirements: true,
    };
    if rule.action == LootAction::KeepUpTo {
        rule.keep_count = parse_int(reader.read_line()?, Field::Rule(number, "keep count"))?.max(0);
    }

    for field in fields {
        let kind = parse_int(field, Field::Rule(number, "requirement type"))?;
        let payload = if version >= 1 {
            let length = parse_count(
                reader.read_line()?,
                Field::Rule(number, "requirement length"),
                16 * 1024 * 1024,
            )?;
            match legacy_payload_line_count(kind) {
                Some(lines) => read_known_payload(reader, lines)?,
                None => reader.read_characters(length)?.to_owned(),
            }
        } else {
            let Some(lines) = legacy_payload_line_count(kind) else {
                return Err(FormatError(format!(
                    "Version 0 loot rule {number} uses unknown requirement {kind}."
                )));
            };
            read_known_payload(reader, lines)?
        };
        rule.vtank_requirements
            .push(VtankLootRequirement { kind, payload });
    }
    Ok(rule)
}

fn write_rule(output: &mut String, rule: &LootRule) {
    append_line(output, single_line(&rule.name));
    append_line(output, single_line(&rule.custom_expression));

    let requirements = export_requirements(rule);
    let action = rule.action as i32;
    let action = if (0..=10).contains(&action) {
        action
    } else {
        LootAction::NoLoot as i32
    };
    push_decimal(output, i64::from(rule.priority));
    output.push(';');
    push_decimal(output, i64::from(action));
    for requirement in requirements.iter() {
        output.push(';');
        push_decimal(output, i64::from(requirement.kind));
    }
    output.push_str(NEW_LINE);

    if action == LootAction::KeepUpTo as i32 {
        append_number_line(output, i64::from(rule.keep_count.max(0)));
    }
    for requirement in requirements.iter() {
        let payload = normalized(&requirement.payload);
        append_number_line(output, count(utf16_len(&payload)));
        output.push_str(&payload);
    }
}

fn export_requirements(rule: &LootRule) -> std::borrow::Cow<'_, [VtankLootRequirement]> {
    if rule.has_imported_requirements || !rule.vtank_requirements.is_empty() {
        return std::borrow::Cow::Borrowed(&rule.vtank_requirements);
    }
    std::borrow::Cow::Owned(vec![VtankLootRequirement {
        kind: DISABLED_RULE_TYPE,
        payload: format!("true{NEW_LINE}"),
    }])
}

fn read_known_payload(
    reader: &mut CharacterReader<'_>,
    line_count: usize,
) -> Result<String, FormatError> {
    // Lines already ended by `\r\n` in the file are the payload as it
    // stands: one copy instead of one per line.
    let start = reader.position;
    let mut written_as_is = true;
    for _ in 0..line_count {
        let line_start = reader.position;
        let end = line_start + reader.read_line()?.len();
        written_as_is &= reader.source.get(end..reader.position) == Some(NEW_LINE);
    }
    if written_as_is {
        return Ok(reader.source[start..reader.position].to_owned());
    }
    reader.position = start;
    let mut payload = String::new();
    for _ in 0..line_count {
        append_line(&mut payload, reader.read_line()?);
    }
    Ok(payload)
}

fn read_salvage(
    reader: &mut CharacterReader<'_>,
) -> Result<VtankSalvageCombineSettings, FormatError> {
    parse_int(reader.read_line()?, Field::Named("salvage block version"))?;
    let mut result = VtankSalvageCombineSettings {
        default_combine_string: reader.read_line()?.to_owned(),
        material_combine_strings: BTreeMap::new(),
        material_value_mode_values: BTreeMap::new(),
    };
    let strings = parse_count(
        reader.read_line()?,
        Field::Named("salvage material rule count"),
        10_000,
    )?;
    for _ in 0..strings {
        let material = parse_int(reader.read_line()?, Field::Named("salvage material id"))?;
        result
            .material_combine_strings
            .insert(material, reader.read_line()?.to_owned());
    }
    if reader.at_end() {
        return Ok(result);
    }
    let values = parse_count(
        reader.read_line()?,
        Field::Named("salvage value-mode count"),
        10_000,
    )?;
    for _ in 0..values {
        let material = parse_int(
            reader.read_line()?,
            Field::Named("salvage value material id"),
        )?;
        let value = parse_int(
            reader.read_line()?,
            Field::Named("salvage value-mode value"),
        )?;
        result.material_value_mode_values.insert(material, value);
    }
    Ok(result)
}

fn write_salvage(settings: &VtankSalvageCombineSettings) -> String {
    let mut output = String::new();
    append_line(&mut output, "1");
    append_line(&mut output, single_line(&settings.default_combine_string));
    append_line(
        &mut output,
        &settings.material_combine_strings.len().to_string(),
    );
    for (material, combine) in &settings.material_combine_strings {
        append_line(&mut output, &material.to_string());
        append_line(&mut output, single_line(combine));
    }
    append_line(
        &mut output,
        &settings.material_value_mode_values.len().to_string(),
    );
    for (material, value) in &settings.material_value_mode_values {
        append_line(&mut output, &material.to_string());
        append_line(&mut output, &value.to_string());
    }
    output
}

fn write_block(output: &mut String, kind: &str, payload: &str) {
    let payload = normalized(payload);
    append_line(output, single_line(kind));
    append_number_line(output, count(utf16_len(&payload)));
    output.push_str(&payload);
}

/// [`normalize_payload`], the text itself when it is in those line endings
/// already (every carriage return before a line feed, every line feed after
/// a carriage return).
fn normalized(payload: &str) -> std::borrow::Cow<'_, str> {
    let bytes = payload.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => index += 2,
            b'\r' | b'\n' => return std::borrow::Cow::Owned(normalize_payload(payload)),
            _ => index += 1,
        }
    }
    std::borrow::Cow::Borrowed(payload)
}

/// The UTF-16 length, the byte length of ASCII text.
fn utf16_len(text: &str) -> usize {
    if text.is_ascii() {
        text.len()
    } else {
        utf16::len(text)
    }
}

/// A count as the file writes it.
fn count(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// `value` in decimal, as `{value}` writes it.
fn push_decimal(output: &mut String, value: i64) {
    if value < 0 {
        output.push('-');
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
    output.push_str(std::str::from_utf8(&digits[start..]).unwrap_or_default());
}

/// A number on a line of its own.
fn append_number_line(output: &mut String, value: i64) {
    push_decimal(output, value);
    output.push_str(NEW_LINE);
}

/// Text in the line endings a block is written with, so a length measured
/// on the result is the length a reader counts off the file.
#[must_use]
pub fn normalize_payload(payload: &str) -> String {
    payload
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', NEW_LINE)
}

/// The text up to its first line break.
fn single_line(value: &str) -> &str {
    // Line breaks are ASCII: a byte search finds the first.
    let end = value
        .bytes()
        .position(|byte| byte == b'\r' || byte == b'\n');
    end.map_or(value, |end| &value[..end])
}

/// Lines a requirement type's payload has (version 0 has no lengths).
fn legacy_payload_line_count(kind: i32) -> Option<usize> {
    Some(match kind {
        0 => 1,
        2 | 3 | 4 | 5 | 11 | 12 | 13 | 2003 | 2005 | 1 | 17 | 1000 => 2,
        6 | 7 | 8 | 10 | 1001 | 1002 | 1003 | 2000 | 2001 | 2006 | 2007 | 9999 => 1,
        9 | 1004 | 2008 => 3,
        14 => 5,
        15 | 16 => 6,
        _ => return None,
    })
}

/// The field a number is read for, named in the refusal: worded only when
/// the number does not read (a large profile reads some ten thousand).
#[derive(Debug, Clone, Copy)]
enum Field<'a> {
    /// A field of the profile, by name.
    Named(&'a str),
    /// A block's length line, by the block's type line.
    Block(&'a str),
    /// A field of the rule with this number.
    Rule(usize, &'static str),
}

impl std::fmt::Display for Field<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Named(name) => f.write_str(name),
            Self::Block(kind) => write!(f, "{kind} block length"),
            Self::Rule(number, name) => write!(f, "rule {number} {name}"),
        }
    }
}

fn parse_count(value: &str, field: Field<'_>, maximum: i32) -> Result<usize, FormatError> {
    let parsed = parse_int(value, field)?;
    if parsed < 0 || parsed > maximum {
        return Err(FormatError(format!("Invalid {field}: {value}.")));
    }
    Ok(usize::try_from(parsed).expect("a checked count"))
}

fn parse_int(value: &str, field: Field<'_>) -> Result<i32, FormatError> {
    try_parse_int(value).ok_or_else(|| FormatError(format!("Invalid {field}: {value}.")))
}

fn append_line(output: &mut String, value: &str) {
    output.push_str(value);
    output.push_str(NEW_LINE);
}

/// Reads lines ended by `\r\n`, `\r` or `\n`, and runs of characters.
#[derive(Debug)]
struct CharacterReader<'a> {
    source: &'a str,
    position: usize,
}

impl<'a> CharacterReader<'a> {
    fn at_end(&self) -> bool {
        self.position >= self.source.len()
    }

    fn read_line(&mut self) -> Result<&'a str, FormatError> {
        let mut lines = mosstank_dotnet::LineReader::new(&self.source[self.position..]);
        let Some(line) = lines.read_line() else {
            return Err(FormatError(
                "The VTClassic loot profile ended unexpectedly.".to_owned(),
            ));
        };
        self.position = self.source.len() - lines.rest().len();
        Ok(line)
    }

    fn read_characters(&mut self, count: usize) -> Result<&'a str, FormatError> {
        let rest = &self.source[self.position..];
        // ASCII text counts one UTF-16 unit a byte.
        if rest.as_bytes().get(..count).is_some_and(<[u8]>::is_ascii) {
            self.position += count;
            return Ok(&rest[..count]);
        }
        let Some((value, _)) = utf16::split_at(rest, count) else {
            return Err(FormatError(
                "A VTClassic length-delimited block is truncated.".to_owned(),
            ));
        };
        self.position += value.len();
        Ok(value)
    }
}

// ---------------------------------------------------------------------
// MossTank's own rule expressions, kept in a block of their own so VTank
// and VTClassic read the file unchanged.
// ---------------------------------------------------------------------

/// The profile text for `rules`, with MossTank's expressions attached.
#[must_use]
pub fn serialize_rules(rules: &[LootRule]) -> String {
    write_parts(
        rules,
        &VtankSalvageCombineSettings::default(),
        &[mosstank_expressions_block(rules)],
    )
}

/// The rules in the text of a profile file, expressions and all; `None`
/// when the text is not a profile.
#[must_use]
pub fn try_parse_rules(text: Option<&str>) -> Option<Vec<LootRule>> {
    let mut profile = try_read(text).ok()?;
    apply_mosstank_expressions(&mut profile);
    Some(profile.rules)
}

/// Adds (or replaces) the block holding each rule's MossTank expression.
pub fn attach_mosstank_expressions(profile: &mut VtankLootProfile) {
    profile
        .unknown_blocks
        .retain(|block| block.kind != MOSSTANK_RULES_BLOCK_TYPE);
    let block = mosstank_expressions_block(&profile.rules);
    profile.unknown_blocks.push(block);
}

/// The block holding each rule's MossTank expression
/// ([`attach_mosstank_expressions`]).
#[must_use]
pub fn mosstank_expressions_block(rules: &[LootRule]) -> VtankLootExtraBlock {
    let mut payload = String::with_capacity(8 + rules.len() * 4);
    append_number_line(&mut payload, count(rules.len()));
    for rule in rules {
        // An imported rule keeps its requirement records, even an empty
        // list; its expression is not stored. The length is measured on the
        // text as the block is written, line breaks normalized.
        if rule.has_imported_requirements || !rule.vtank_requirements.is_empty() {
            append_number_line(&mut payload, 0);
            continue;
        }
        let expression = normalized(rule.expression());
        append_number_line(&mut payload, count(utf16_len(&expression)));
        payload.push_str(&expression);
    }
    VtankLootExtraBlock {
        kind: MOSSTANK_RULES_BLOCK_TYPE.to_owned(),
        payload,
    }
}

/// The other blocks a read gives of a profile written with `blocks` (the
/// ones [`write_parts`] writes, as the reader takes them back); `None`
/// when that is not plain from the blocks themselves (a kind the reader
/// would take for the salvage block, or an empty one).
#[must_use]
pub fn blocks_as_read(blocks: &[VtankLootExtraBlock]) -> Option<Vec<VtankLootExtraBlock>> {
    blocks
        .iter()
        .filter(|block| !block.kind.is_empty() && block.kind != SALVAGE_BLOCK)
        .map(|block| {
            let kind = single_line(&block.kind);
            (!kind.is_empty() && kind != SALVAGE_BLOCK).then(|| VtankLootExtraBlock {
                kind: kind.to_owned(),
                payload: normalized(&block.payload).into_owned(),
            })
        })
        .collect()
}

/// Takes MossTank's expression block out of `profile` and puts each
/// expression back on its rule (a rule with one stops being an imported
/// VTank rule).
pub fn apply_mosstank_expressions(profile: &mut VtankLootProfile) {
    let Some(index) = profile
        .unknown_blocks
        .iter()
        .position(|candidate| candidate.kind == MOSSTANK_RULES_BLOCK_TYPE)
    else {
        return;
    };
    let block = profile.unknown_blocks.remove(index);
    let mut lines = mosstank_dotnet::LineReader::new(&block.payload);
    let Some(count) = lines.read_line().and_then(try_parse_int) else {
        return;
    };
    let limit = usize::try_from(count).unwrap_or(0).min(profile.rules.len());
    for rule in profile.rules.iter_mut().take(limit) {
        let Some(length) = lines
            .read_line()
            .and_then(try_parse_int)
            .and_then(|length| usize::try_from(length).ok())
        else {
            return;
        };
        let Some(expression) = lines.read_utf16(length) else {
            return;
        };
        if expression.is_empty() {
            continue;
        }
        rule.set_expression(expression);
        rule.vtank_requirements.clear();
        rule.has_imported_requirements = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(rule: &LootRule) -> Vec<i32> {
        rule.vtank_requirements.iter().map(|r| r.kind).collect()
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootProfileSerializerTests.cs :: ReadsAndWritesExactUtlOneRecordsAndUnknownBlocks
    #[test]
    fn reads_and_writes_exact_utl_one_records_and_unknown_blocks() {
        let source = concat!(
            "UTL\r\n1\r\n1\r\n",
            "Epic loot\r\nLegacy editor text\r\n42;10;1;3;9999\r\n7\r\n",
            "12\r\n^Epic.*\r\n1\r\n11\r\n25000\r\n19\r\n",
            "7\r\nfalse\r\n",
            "SalvageCombine\r\n36\r\n1\r\n1-6, 7-8, 9, 10\r\n1\r\n61\r\n1-10\r\n0\r\n",
            "FutureBlock\r\n7\r\nhello\r\n"
        );
        let profile = try_read(Some(source)).unwrap();
        assert_eq!(profile.rules.len(), 1);
        let rule = &profile.rules[0];
        assert_eq!(rule.name, "Epic loot");
        assert_eq!(rule.custom_expression, "Legacy editor text");
        assert_eq!(rule.action, LootAction::KeepUpTo);
        assert_eq!(rule.keep_count, 7);
        assert_eq!(rule.priority, 42);
        assert_eq!(kinds(rule), [1, 3, 9999]);
        assert_eq!(
            profile.salvage_combine.material_combine_strings[&61],
            "1-10"
        );
        assert_eq!(profile.unknown_blocks.len(), 1);
        assert_eq!(profile.unknown_blocks[0].payload, "hello\r\n");

        let canonical = write(&profile);
        let second = try_read(Some(&canonical)).unwrap();
        assert_eq!(write(&second), canonical);
    }

    fn valid_payload(kind: i32) -> String {
        let lines = match kind {
            0 => 1,
            1 | 2 | 3 | 4 | 5 | 11 | 12 | 13 | 17 | 1000 | 2003 | 2005 => 2,
            6 | 7 | 8 | 10 | 1001 | 1002 | 1003 | 2000 | 2001 | 2006 | 2007 | 9999 => 1,
            9 | 1004 | 2008 => 3,
            14 => 5,
            15 | 16 => 6,
            _ => panic!("no payload shape for {kind}"),
        };
        (1..=lines).map(|index| format!("{index}\r\n")).collect()
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootProfileSerializerTests.cs :: RoundTripsEveryVtankRequirementTypeWithoutLosingPayload
    #[test]
    fn round_trips_every_vtank_requirement_type_without_losing_payload() {
        let types = [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 1000, 1001, 1002, 1003,
            1004, 2000, 2001, 2003, 2005, 2006, 2007, 2008, 9999,
        ];
        let mut rule = LootRule {
            name: "All vocabulary".to_owned(),
            action: LootAction::Keep,
            priority: -19,
            ..LootRule::default()
        };
        for kind in types {
            rule.vtank_requirements.push(VtankLootRequirement {
                kind,
                payload: valid_payload(kind),
            });
        }
        let profile = VtankLootProfile {
            rules: vec![rule.clone()],
            ..VtankLootProfile::default()
        };
        let source = write(&profile);
        let loaded = try_read(Some(&source)).unwrap();
        let round_trip = &loaded.rules[0];
        assert_eq!(kinds(round_trip), types);
        assert_eq!(round_trip.vtank_requirements, rule.vtank_requirements);
        assert_eq!(write(&loaded), source);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootProfileSerializerTests.cs :: KnownRecordsUseTheirTypedLineShapeAndKnownBlocksIgnoreLengthMetadata
    #[test]
    fn known_records_use_their_typed_line_shape_and_known_blocks_ignore_length_metadata() {
        let source = concat!(
            "UTL\n1\n2\n",
            "First\neditor expression\n0;1;1\n999\nSword\n1\n",
            "Second\n\n0;1;7\n1\n2\n",
            "SalvageCombine\n1\n1\n1-6, 7-8, 9, 10\n1\n61\n1-10\n0\n",
            "FutureBlock\n6\nhello\n"
        );
        let profile = try_read(Some(source)).unwrap();
        assert_eq!(profile.rules.len(), 2);
        assert_eq!(
            profile.rules[0].vtank_requirements[0].payload,
            "Sword\r\n1\r\n"
        );
        assert_eq!(profile.rules[1].vtank_requirements[0].payload, "2\r\n");
        assert_eq!(
            profile.salvage_combine.material_combine_strings[&61],
            "1-10"
        );
        assert_eq!(profile.unknown_blocks[0].payload, "hello\n");
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootProfileSerializerTests.cs :: UnknownRequirementStillUsesAndValidatesItsDeclaredLength
    #[test]
    fn unknown_requirement_still_uses_and_validates_its_declared_length() {
        let valid = "UTL\r\n1\r\n1\r\nUnknown\r\n\r\n0;1;4242\r\n7\r\nhello\r\n";
        let profile = try_read(Some(valid)).unwrap();
        assert_eq!(profile.rules[0].vtank_requirements.len(), 1);
        assert_eq!(profile.rules[0].vtank_requirements[0].payload, "hello\r\n");

        let truncated = "UTL\r\n1\r\n1\r\nUnknown\r\n\r\n0;1;4242\r\n99\r\nhello\r\n";
        let error = try_read(Some(truncated)).unwrap_err();
        assert!(error.to_lowercase().contains("truncated"), "{error}");
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootProfileSerializerTests.cs :: ImportedEmptyRequirementSetRoundTripsAsAnUnconditionalKeep
    // (partial: the final LootRuleEngine.Decide check comes with the looting slice)
    #[test]
    fn imported_empty_requirement_set_round_trips_as_an_unconditional_keep() {
        let source = "UTL\r\n1\r\n1\r\nAlways keep\r\n\r\n0;1\r\n";
        let profile = try_read(Some(source)).unwrap();
        let imported = &profile.rules[0];
        assert!(imported.vtank_requirements.is_empty());
        assert!(imported.has_imported_requirements);

        let written = write(&profile);
        assert!(!written.contains(";9999\r\n"));
        let reloaded = try_read(Some(&written)).unwrap();
        assert!(reloaded.rules[0].vtank_requirements.is_empty());
        assert!(reloaded.rules[0].has_imported_requirements);
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootProfileSerializerTests.cs :: NativeExpressionsExportDisabledInsteadOfAccidentalMatchAll
    #[test]
    fn native_expressions_export_disabled_instead_of_accidental_match_all() {
        let mut rule = LootRule::named("Native only", "name ~= Sword && value > 100");
        rule.action = LootAction::Keep;
        let profile = VtankLootProfile {
            rules: vec![rule],
            ..VtankLootProfile::default()
        };
        let source = write(&profile);
        assert!(source.contains("0;1;9999\r\n"));
        assert!(source.contains("6\r\ntrue\r\n"));
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootProfileSerializerTests.cs :: ReadsLegacyVersionZeroKnownRequirements
    #[test]
    fn reads_legacy_version_zero_known_requirements() {
        let source = "1\r\nLegacy\r\n7;1;1;8\r\n^Sword$\r\n1\r\n3\r\n";
        let profile = try_read(Some(source)).unwrap();
        assert_eq!(profile.source_version, 0);
        let rule = &profile.rules[0];
        assert_eq!(rule.vtank_requirements.len(), 2);
        assert_eq!(rule.vtank_requirements[0].payload, "^Sword$\r\n1\r\n");
        assert_eq!(rule.vtank_requirements[1].payload, "3\r\n");
    }

    // Source: tests/AcDream.Plugins.MossTank.Tests/VtankLootProfileSerializerTests.cs :: RuleExpressionsWithCrlfLfAndCrLineBreaksRoundTripThroughTheFile
    #[test]
    fn rule_expressions_with_crlf_lf_and_cr_line_breaks_round_trip_through_the_file() {
        let rules = [
            LootRule::named("LF", "first\nsecond"),
            LootRule::named("CRLF and CR", "one\r\ntwo\rthree"),
            LootRule::named("Single line", "ObjectClass = 5"),
        ];
        let text = serialize_rules(&rules);
        try_read(Some(&text)).unwrap();
        let loaded = try_parse_rules(Some(&text)).unwrap();
        let expressions: Vec<&str> = loaded.iter().map(LootRule::expression).collect();
        assert_eq!(
            expressions,
            ["first\r\nsecond", "one\r\ntwo\r\nthree", "ObjectClass = 5"]
        );
        assert_eq!(serialize_rules(&loaded), text);
    }

    #[test]
    fn the_utl_fixtures_read_and_write_back_stably() {
        for name in ["loot-a.utl", "loot-b.utl", "loot-c.utl"] {
            let text = crate::fixtures::read_vtank(name);
            let profile = try_read(Some(&text)).unwrap_or_else(|error| panic!("{name}: {error}"));
            let written = write(&profile);
            let again = try_read(Some(&written)).unwrap();
            assert_eq!(write(&again), written, "{name}");
        }
        // loot-d.utl is cut short after a few of its 90 rules.
        let partial = read_allowing_partial(Some(&crate::fixtures::read_vtank("loot-d.utl")));
        assert!(!partial.is_complete);
        assert!(!partial.profile.rules.is_empty());
    }

    // The C# tests' fixtures (Fixtures/vtank/loot-*.utl), one written with
    // `\n` alone and one with mixed breaks too: read a run at a time, a
    // profile is what reading it whole gives, a cut-short one included.
    #[test]
    fn a_read_a_run_at_a_time_is_the_whole_read() {
        let mut texts: Vec<String> = ["loot-a.utl", "loot-b.utl", "loot-c.utl", "loot-d.utl"]
            .iter()
            .map(|name| crate::fixtures::read_vtank(name))
            .collect();
        texts.push(texts[2].replace("\r\n", "\n"));
        texts.push(
            texts[2]
                .replacen("\r\n", "\n", 40)
                .replacen("\r\n", "\r", 7),
        );
        texts.push(String::new());
        for text in &texts {
            let whole = read_allowing_partial(Some(text));
            for bytes in [0, 1, 200, 5000] {
                let mut read = ProfileRead::new();
                let mut steps = 0;
                let result = loop {
                    steps += 1;
                    if let Some(result) = read.step(text, bytes) {
                        break result;
                    }
                };
                assert_eq!(result, whole, "{bytes} bytes a step");
                assert!(read.step(text, bytes).is_none(), "the read is over");
                if bytes == 0 && whole.profile.rules.len() > 2 {
                    assert!(steps > 2, "a rule a step");
                }
            }
        }
    }

    // The writer's shortcuts give what the plain forms give: a payload in
    // its line endings already is written as it is, a number as `{n}`.
    #[test]
    fn the_writers_shortcuts_write_what_the_plain_forms_write() {
        for payload in [
            "",
            "a",
            "a\r\nb\r\n",
            "a\nb",
            "a\rb",
            "\r\r\n",
            "\n\r",
            "x\r",
            "\r\n\r\n",
            "é\r\nü",
        ] {
            assert_eq!(
                normalized(payload),
                normalize_payload(payload),
                "{payload:?}"
            );
            assert_eq!(utf16_len(payload), utf16::len(payload), "{payload:?}");
        }
        for value in [i64::MIN, -10, -1, 0, 7, 10, 1684, i64::MAX] {
            let mut text = String::new();
            push_decimal(&mut text, value);
            assert_eq!(text, value.to_string());
        }
        assert_eq!(single_line("first\r\nsecond"), "first");
        assert_eq!(single_line("only"), "only");
    }

    // What a store remembers of a file it wrote is what reading the file
    // gives: the other blocks, line breaks as written.
    #[test]
    fn the_blocks_a_written_profile_reads_back_are_the_blocks_written() {
        let profile = try_read(Some(&crate::fixtures::read_vtank("loot-c.utl"))).unwrap();
        let blocks = vec![
            VtankLootExtraBlock {
                kind: "Other".to_owned(),
                payload: "one\ntwo\rthree\r\n".to_owned(),
            },
            mosstank_expressions_block(&profile.rules),
        ];
        let text = write_parts(&profile.rules, &profile.salvage_combine, &blocks);
        assert_eq!(
            blocks_as_read(&blocks),
            Some(try_read(Some(&text)).unwrap().unknown_blocks)
        );
        let odd = [VtankLootExtraBlock {
            kind: "SalvageCombine\r\nx".to_owned(),
            payload: String::new(),
        }];
        assert_eq!(blocks_as_read(&odd), None);
    }

    #[test]
    fn a_profile_written_a_run_at_a_time_is_written_whole() {
        use std::hash::{BuildHasher, Hasher};
        for name in ["loot-a.utl", "loot-b.utl", "loot-c.utl"] {
            let profile = try_read(Some(&crate::fixtures::read_vtank(name))).unwrap();
            let whole = write(&profile);
            for bytes in [1, 300, usize::MAX] {
                let state = std::collections::hash_map::RandomState::new();
                let mut hasher = state.build_hasher();
                let mut write = PartsWrite::default();
                while !write.step(&profile.rules, bytes, &mut hasher) {}
                assert_eq!(
                    write.finish(&profile.salvage_combine, &profile.unknown_blocks),
                    whole,
                    "{name}"
                );
                assert_eq!(
                    hasher.finish(),
                    state.hash_one(&profile.rules[..]),
                    "{name}"
                );
            }
        }
    }
}

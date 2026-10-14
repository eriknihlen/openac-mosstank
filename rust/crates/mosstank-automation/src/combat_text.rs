//! Reading the character's own combat log (`CombatResultText`): which of a
//! spell's result sentences a line is, the killing-blow sentences, and the
//! damage reports.

use std::sync::OnceLock;

use openac_plugin_compat::number::{self, NumberStyles};
use openac_plugin_compat::regex::{DotNetRegex, RegexOptions};

/// The client's own log-text types the combat reader keys on. The same
/// sentence typed by a player in local chat carries a different one, so
/// these tell the character's own combat log from anyone else's words.
pub mod log_text_type {
    /// The plain type: the miss notice and the kill sentence.
    pub const DEFAULT: u32 = 0x00;
    /// The character's own blow: "you hit X for N points".
    pub const OWN_COMBAT: u32 = 0x16;
    /// A spell's own result line.
    pub const MAGIC: u32 = 0x07;
    /// The spoken words of a spell being cast: the server logs a caster's
    /// own gesture words under this type.
    pub const SPELLCASTING: u32 = 0x11;
}

/// Which kind of result sentence a line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CombatResultTextClass {
    /// None of them.
    #[default]
    None,
    /// A fizzle or a resist.
    Fail,
    /// A failure the same cast would meet again.
    PermanentFail,
    /// The spell landed.
    Success,
    /// A killing blow.
    Kill,
}

/// A classified line and the names it carried (empty when it named none).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CombatResult {
    /// Which sentence it is.
    pub class: CombatResultTextClass,
    /// The spell it named.
    pub spell_name: String,
    /// The creature or item it named.
    pub target_name: String,
}

/// A fixed pattern, read by hand when it has the plain shape
/// [`PlainShape`] describes, else built on its first use or before it by
/// [`warm_one`] (building one costs the plugin some ten million
/// instructions, so they are built one a tick before they are needed).
#[derive(Debug)]
struct Pattern {
    text: &'static str,
    plain: OnceLock<Option<PlainShape>>,
    built: OnceLock<DotNetRegex>,
    lead: OnceLock<&'static str>,
    required: OnceLock<Vec<String>>,
}

/// A pattern of the shape `^PREFIX(?<targetname>.*)SUFFIX$` or `^TEXT$`
/// whose literal parts have no metacharacter but `.` (any character but a
/// line feed). It matches a line that, less one final line feed, is the
/// prefix, a target with no line feed, and the suffix: the regex's greedy
/// `.*` takes all between them, and `$` matches at the end or before a
/// final line feed (the suffix never ends in a line feed).
#[derive(Debug, Clone, Copy)]
struct PlainShape {
    prefix: &'static str,
    /// The suffix, when the pattern captures a target.
    suffix: Option<&'static str>,
}

const TARGET_GROUP: &str = "(?<targetname>.*)";

impl PlainShape {
    /// The shape of `pattern`, if it is plain.
    fn of(pattern: &'static str) -> Option<Self> {
        let inner = pattern.strip_prefix('^')?.strip_suffix('$')?;
        const META: [char; 13] = [
            '\\', '^', '$', '|', '?', '*', '+', '(', ')', '[', ']', '{', '}',
        ];
        let literal = |part: &str| !part.contains(META);
        match inner.split_once(TARGET_GROUP) {
            Some((prefix, suffix)) => (literal(prefix) && literal(suffix)).then_some(Self {
                prefix,
                suffix: Some(suffix),
            }),
            None => literal(inner).then_some(Self {
                prefix: inner,
                suffix: None,
            }),
        }
    }

    /// The target the line carries ("" for none) when the line matches.
    fn read(self, line: &str) -> Option<&str> {
        let line = line.strip_suffix('\n').unwrap_or(line);
        let rest = take_literal(line, self.prefix)?;
        let Some(suffix) = self.suffix else {
            return rest.is_empty().then_some("");
        };
        let target = take_literal_from_end(rest, suffix)?;
        (!target.contains('\n')).then_some(target)
    }
}

/// Whether a literal character (`.` any but a line feed) matches `c`.
fn literal_matches(literal: char, c: char) -> bool {
    if literal == '.' {
        c != '\n'
    } else {
        literal == c
    }
}

/// `text` past `literal` when it starts with it.
fn take_literal<'t>(text: &'t str, literal: &str) -> Option<&'t str> {
    let mut chars = text.char_indices();
    for expected in literal.chars() {
        let (_, c) = chars.next()?;
        if !literal_matches(expected, c) {
            return None;
        }
    }
    Some(chars.as_str())
}

/// `text` short of `literal` when it ends with it.
fn take_literal_from_end<'t>(text: &'t str, literal: &str) -> Option<&'t str> {
    let mut chars = text.chars();
    for expected in literal.chars().rev() {
        let c = chars.next_back()?;
        if !literal_matches(expected, c) {
            return None;
        }
    }
    Some(chars.as_str())
}

/// The literal text (`.` any character but a line feed) every line an
/// anchored pattern matches starts with: what follows its `^` up to the
/// first metacharacter, less a last character a quantifier applies to;
/// empty for a pattern not anchored at the start or with an alternation.
fn leading_literal(pattern: &'static str) -> &'static str {
    const META: [char; 13] = [
        '\\', '^', '$', '|', '?', '*', '+', '(', ')', '[', ']', '{', '}',
    ];
    let Some(inner) = pattern.strip_prefix('^') else {
        return "";
    };
    if inner.contains('|') {
        return "";
    }
    let end = inner.find(META).unwrap_or(inner.len());
    let lead = &inner[..end];
    // `x*`, `x?` and `x{n,}` make their `x` no part of every match.
    match inner[end..].chars().next() {
        Some('*' | '?' | '{') => {
            let cut = lead.char_indices().last().map_or(0, |(index, _)| index);
            &lead[..cut]
        }
        _ => lead,
    }
}

/// The runs of literal text every line the pattern matches contains: the
/// literal characters outside its groups and classes, split where a
/// wildcard, group, class, anchor or optional character stands; none for a
/// pattern with an alternation outside a group.
fn required_literals(pattern: &str) -> Vec<String> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut runs = Vec::new();
    let mut run = String::new();
    let mut index = 0;
    let end_run = |run: &mut String, runs: &mut Vec<String>| {
        if !run.is_empty() {
            runs.push(std::mem::take(run));
        }
    };
    while let Some(&c) = chars.get(index) {
        // One atom: a literal character (escaped or not), or something
        // that is not one (skipped whole).
        let literal = match c {
            '|' => return Vec::new(),
            '(' | '[' => {
                index = skip_bracketed(&chars, index);
                None
            }
            '\\' => {
                let escaped = chars.get(index + 1).copied();
                index += 2;
                escaped.filter(|e| !e.is_alphanumeric())
            }
            '{' => {
                index = chars[index..]
                    .iter()
                    .position(|c| *c == '}')
                    .map_or(chars.len(), |close| index + close + 1);
                None
            }
            '.' | '^' | '$' | '*' | '+' | '?' | '}' | ')' | ']' => {
                index += 1;
                None
            }
            c => {
                index += 1;
                Some(c)
            }
        };
        // A quantifier after the atom: `*`, `?` and `{` may leave it out,
        // `+` keeps it but may repeat it.
        let quantifier = chars.get(index).copied();
        match (literal, quantifier) {
            (Some(c), Some('+')) => {
                run.push(c);
                end_run(&mut run, &mut runs);
            }
            (Some(_), Some('*' | '?' | '{')) | (None, _) => end_run(&mut run, &mut runs),
            (Some(c), _) => run.push(c),
        }
    }
    end_run(&mut run, &mut runs);
    runs
}

/// The index past the group or class that opens at `start`.
fn skip_bracketed(chars: &[char], start: usize) -> usize {
    let mut depth = 0usize;
    let mut in_class = false;
    let mut index = start;
    while let Some(&c) = chars.get(index) {
        match c {
            '\\' => index += 1,
            '[' if !in_class => in_class = true,
            ']' if in_class => {
                in_class = false;
                if depth == 0 {
                    return index + 1;
                }
            }
            '(' if !in_class => depth += 1,
            ')' if !in_class => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
        index += 1;
    }
    chars.len()
}

impl Pattern {
    const fn new(text: &'static str) -> Self {
        Self {
            text,
            plain: OnceLock::new(),
            built: OnceLock::new(),
            lead: OnceLock::new(),
            required: OnceLock::new(),
        }
    }

    /// The literal runs every line the pattern matches contains
    /// ([`required_literals`]).
    fn required(&self) -> &[String] {
        self.required.get_or_init(|| required_literals(self.text))
    }

    fn plain(&self) -> Option<PlainShape> {
        *self.plain.get_or_init(|| PlainShape::of(self.text))
    }

    /// The literal start every line the pattern matches has
    /// ([`leading_literal`]).
    fn lead(&self) -> &'static str {
        self.lead.get_or_init(|| leading_literal(self.text))
    }

    fn get(&self) -> &DotNetRegex {
        self.built.get_or_init(|| {
            DotNetRegex::new(
                self.text,
                RegexOptions::CULTURE_INVARIANT | RegexOptions::EXPLICIT_CAPTURE,
            )
            .expect("a fixed pattern")
        })
    }

    /// The spell and target names the line carries when it matches.
    fn matches(&self, text: &str) -> Option<(String, String)> {
        if let Some(shape) = self.plain() {
            return shape
                .read(text)
                .map(|target| (String::new(), target.to_owned()));
        }
        // A line that does not start as every match does, or lacks a piece
        // of text every match has, is no match: the regex is not built
        // (some millions of instructions) for it.
        take_literal(text, self.lead())?;
        if !self
            .required()
            .iter()
            .all(|run| text.contains(run.as_str()))
        {
            return None;
        }
        let Ok(Some(found)) = self.get().find(text) else {
            return None;
        };
        Some((
            found.group("spellname").value.to_owned(),
            found.group("targetname").value.to_owned(),
        ))
    }
}

static FAIL: [Pattern; 2] = [
    Pattern::new("^Your spell fizzled.$"),
    Pattern::new("^(?<targetname>.*) resists your spell$"),
];

static PERMANENT_FAIL: [Pattern; 5] = [
    Pattern::new("^Target is out of range$"),
    Pattern::new("^You fail to affect (?<targetname>.*) because you are not a player killer!$"),
    Pattern::new("^You fail to affect (?<targetname>.*) because .* is not a player killer!$"),
    Pattern::new(
        "^You fail to affect (?<targetname>.*) because beneficial spells do not affect .*!$",
    ),
    Pattern::new("^(?<targetname>.*) is an invalid target.$"),
];

static SUCCESS: [Pattern; 13] = [
    Pattern::new("^You cast (?<spellname>.*) on (?<targetname>.*), refreshing .*$"),
    Pattern::new("^You cast (?<spellname>.*) on (?<targetname>.*), surpassing .*$"),
    Pattern::new("^You cast (?<spellname>.*) on (?<targetname>.*), but it is surpassed by .*$"),
    Pattern::new("^You cast (?<spellname>.*) on (?<targetname>.*)$"),
    Pattern::new("^You .* due to casting (?<spellname>.*) on (?<targetname>.*)$"),
    Pattern::new(r"^With (?<spellname>.*) you .* from (?<targetname>.*)\.$"),
    Pattern::new(r"^With (?<spellname>.*) you .* to (?<targetname>.*)\.$"),
    Pattern::new(
        r"^(Critical hit! )?(Sneak Attack! )?You .* .* for .* points with (?<spellname>.*)\.$",
    ),
    Pattern::new(r"^You cast (?<spellname>.*) and restore .* points of your .*\.$"),
    Pattern::new(
        "^You cast (?<spellname>.*) on yourself and lose .* points of .* and also gain .* points of .*$",
    ),
    Pattern::new("^You have been teleported.$"),
    Pattern::new(r"^You cast (?<spellname>.*) on (?<targetname>.*) and dispel: .*\.$"),
    Pattern::new("^You cast (?<spellname>.*) on (?<targetname>.*), but the dispel fails.$"),
];

static KILL: [Pattern; 36] = [
    Pattern::new("^You knock (?<targetname>.*) into next Morningthaw!$"),
    Pattern::new("^You obliterate (?<targetname>.*)!$"),
    Pattern::new("^(?<targetname>.*) is utterly destroyed by your attack!$"),
    Pattern::new("^(?<targetname>.*) catches your attack, with dire consequences!$"),
    Pattern::new(
        "^The deadly force of your attack is so strong that (?<targetname>.*)'s ancestors feel it!$",
    ),
    Pattern::new("^You smite (?<targetname>.*) mightily!$"),
    Pattern::new(
        "^You slay (?<targetname>.*) viciously enough to impart death several times over!$",
    ),
    Pattern::new("^You killed (?<targetname>.*)!$"),
    Pattern::new("^(?<targetname>.*) is torn to ribbons by your assault!$"),
    Pattern::new("^You cleave (?<targetname>.*) in twain!$"),
    Pattern::new("^Your killing blow nearly turns (?<targetname>.*) inside-out!$"),
    Pattern::new("^You split (?<targetname>.*) apart!$"),
    Pattern::new(
        "^The thunder of crushing (?<targetname>.*) is followed by the deafening silence of death!$",
    ),
    Pattern::new("^You beat (?<targetname>.*) to a lifeless pulp!$"),
    Pattern::new("^(?<targetname>.*) is shattered by your assault!$"),
    Pattern::new("^You flatten (?<targetname>.*)'s body with the force of your assault!$"),
    Pattern::new("^(?<targetname>.*) is fatally punctured!$"),
    Pattern::new("^(?<targetname>.*)'s perforated corpse falls before you!$"),
    Pattern::new("^You run (?<targetname>.*) through!$"),
    Pattern::new("^(?<targetname>.*)'s death is preceded by a sharp, stabbing pain!$"),
    Pattern::new("^You bring (?<targetname>.*) to a fiery end!$"),
    Pattern::new("^(?<targetname>.*) is incinerated by your assault!$"),
    Pattern::new("^(?<targetname>.*) is reduced to cinders!$"),
    Pattern::new("^(?<targetname>.*)'s seared corpse smolders before you!$"),
    Pattern::new("^Your lightning coruscates over (?<targetname>.*)'s mortal remains!$"),
    Pattern::new("^Blistered by lightning, (?<targetname>.*) falls!$"),
    Pattern::new("^Electricity tears (?<targetname>.*) apart!$"),
    Pattern::new("^Your assault sends (?<targetname>.*) to an icy death!$"),
    Pattern::new("^Your attack stops (?<targetname>.*) cold!$"),
    Pattern::new("^(?<targetname>.*) suffers a frozen fate!$"),
    Pattern::new("^(?<targetname>.*)'s last strength dissolves before you!$"),
    Pattern::new("^(?<targetname>.*) is liquified by your attack!$"),
    Pattern::new("^You reduce (?<targetname>.*) to a sizzling, oozing mass!$"),
    Pattern::new("^You reduce (?<targetname>.*) to a drained, twisted corpse!$"),
    Pattern::new("^(?<targetname>.*) is dessicated by your attack!$"),
    Pattern::new("^(?<targetname>.*)'s last strength withers before you!$"),
];

/// The one line that says a shot never reached its target: it hit the
/// world instead.
pub const MISSILE_HIT_ENVIRONMENT: &str = "Your missile attack hit the environment.";

static DAMAGE_REPORT: [Pattern; 1] = [Pattern::new(
    r"^(Critical hit\!)?[ ]*You .* for .* point(s)? of .*\!$",
)];

static SPELL_DAMAGE: [Pattern; 1] = [Pattern::new(
    "(?:You .* for )(?<points>[0-9]+)(?: points with .*)",
)];

/// The first pattern that matches, with the spell and target it captured.
/// The fixed patterns run on one chat line, so the step limit is not
/// reached; a stopped match would count as no match.
fn try_match(patterns: &[Pattern], text: &str) -> Option<(String, String)> {
    patterns.iter().find_map(|pattern| pattern.matches(text))
}

/// Whether a line reports damage done: the swing reached the monster.
#[must_use]
pub fn is_damage_report(text: &str) -> bool {
    !text.is_empty() && try_match(&DAMAGE_REPORT, text).is_some()
}

/// How much a spell of ours just took off a monster, read out of the
/// sentence that reports it.
#[must_use]
pub fn try_read_spell_damage(text: Option<&str>) -> Option<i32> {
    let text = text.filter(|t| !t.is_empty())?;
    let Ok(Some(found)) = SPELL_DAMAGE[0].get().find(text) else {
        return None;
    };
    number::parse_i32(found.group("points").value, NumberStyles::INTEGER)
}

/// The slain creature's name (empty when the sentence names none) when the
/// line is a killing-blow sentence.
#[must_use]
pub fn is_killing_blow(text: &str) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    try_match(&KILL, text).map(|(_, target)| target)
}

/// Which result sentence a line is: kills first, then permanent failures,
/// failures and successes.
#[must_use]
pub fn classify(text: &str) -> CombatResult {
    if text.is_empty() {
        return CombatResult::default();
    }
    for (class, patterns) in [
        (CombatResultTextClass::Kill, &KILL[..]),
        (CombatResultTextClass::PermanentFail, &PERMANENT_FAIL[..]),
        (CombatResultTextClass::Fail, &FAIL[..]),
        (CombatResultTextClass::Success, &SUCCESS[..]),
    ] {
        if let Some((spell_name, target_name)) = try_match(patterns, text) {
            return CombatResult {
                class,
                spell_name,
                target_name,
            };
        }
    }
    CombatResult::default()
}

/// Builds the next of these patterns not yet built (a plain one is read by
/// hand and never built); false when all are. What a pattern answers does
/// not depend on when it was built.
pub fn warm_one() -> bool {
    let groups: [&[Pattern]; 6] = [
        &SUCCESS,
        &KILL,
        &FAIL,
        &PERMANENT_FAIL,
        &DAMAGE_REPORT,
        &SPELL_DAMAGE,
    ];
    let next = groups
        .into_iter()
        .flatten()
        .find(|pattern| pattern.plain().is_none() && pattern.built.get().is_none());
    next.map(Pattern::get).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_patterns() -> impl Iterator<Item = &'static Pattern> {
        let groups: [&'static [Pattern]; 6] = [
            &SUCCESS,
            &KILL,
            &FAIL,
            &PERMANENT_FAIL,
            &DAMAGE_REPORT,
            &SPELL_DAMAGE,
        ];
        groups.into_iter().flatten()
    }

    /// Lines shaped after every pattern, and near misses.
    fn lines() -> Vec<String> {
        let targets = [
            "Mosswart Kin",
            "",
            " ",
            "a\nb",
            "Drudge's Ravener",
            "\u{1F600} \u{e9}",
            "x!",
            "into next Morningthaw",
        ];
        let mut lines = vec![
            String::new(),
            "\n".to_owned(),
            "You killed !".to_owned(),
            "You killed!".to_owned(),
            "Your spell fizzled".to_owned(),
            "Your spell fizzled\n\n".to_owned(),
            "Your spell fizzled\u{1F600}".to_owned(),
            "Your spell fizzled\n".to_owned(),
            "Your spell fizzledX\n".to_owned(),
        ];
        for pattern in all_patterns() {
            let inner = pattern.text.trim_start_matches('^').trim_end_matches('$');
            for target in targets {
                for wildcard in [".", "x", "\u{e9}", "\n"] {
                    let line = inner
                        .replace("(?<targetname>.*)", target)
                        .replace("(?<spellname>.*)", "Flame Bolt VII")
                        .replace(".*", "some")
                        .replace('.', wildcard);
                    for variant in [
                        line.clone(),
                        format!("{line}\n"),
                        format!("{line}\n\n"),
                        format!(" {line}"),
                        format!("{line} "),
                        line.to_uppercase(),
                        line.chars().skip(1).collect(),
                        line.chars()
                            .take(line.chars().count().saturating_sub(1))
                            .collect(),
                    ] {
                        lines.push(variant);
                    }
                }
            }
        }
        lines
    }

    // A line the leading literal turns away is one the regex does not
    // match: the check only spares building the regex.
    #[test]
    fn a_line_turned_away_by_its_text_is_no_match() {
        let lines = lines();
        let mut leads = 0;
        for pattern in all_patterns().filter(|pattern| pattern.plain().is_none()) {
            if !pattern.lead().is_empty() {
                leads += 1;
            }
            for line in &lines {
                let matched = matches!(pattern.get().find(line), Ok(Some(_)));
                if matched {
                    assert!(
                        take_literal(line, pattern.lead()).is_some(),
                        "{:?} turned away {line:?}",
                        pattern.text
                    );
                    for run in pattern.required() {
                        assert!(
                            line.contains(run.as_str()),
                            "{:?} turned away {line:?} for {run:?}",
                            pattern.text
                        );
                    }
                }
            }
        }
        // Every built pattern but the critical hit's, the damage report and
        // the unanchored spell damage.
        assert_eq!(leads, 13);
        assert_eq!(leading_literal("^You .* due to"), "You ");
        assert_eq!(
            leading_literal("^Your spell fizzled.$"),
            "Your spell fizzled."
        );
        assert_eq!(leading_literal("^(Critical hit! )?You"), "");
        assert_eq!(leading_literal("^ab?c"), "a");
        assert_eq!(leading_literal("(?:You .* for )"), "");
        assert_eq!(leading_literal("^a|b"), "");
        assert_eq!(
            required_literals(r"^(Critical hit\!)?[ ]*You .* for .* point(s)? of .*\!$"),
            ["You ", " for ", " point", " of ", "!"]
        );
        assert_eq!(required_literals("^ab?c+d{2}e$"), ["a", "c", "e"]);
        assert_eq!(required_literals(r"x\dy\.z"), ["x", "y.z"]);
        assert!(required_literals("^a|b").is_empty());
        assert!(required_literals("(?:You .* for )(?<points>[0-9]+)").is_empty());
    }

    // A pattern read by hand answers every line as the regex does.
    #[test]
    fn a_plain_pattern_is_read_as_the_regex_reads_it() {
        let lines = lines();
        let mut plain = 0;
        for pattern in all_patterns() {
            let Some(shape) = pattern.plain() else {
                continue;
            };
            plain += 1;
            for line in &lines {
                let by_hand = shape.read(line).map(str::to_owned);
                let by_regex = match pattern.get().find(line) {
                    Ok(Some(found)) => Some(found.group("targetname").value.to_owned()),
                    _ => None,
                };
                assert_eq!(by_hand, by_regex, "{:?} on {line:?}", pattern.text);
            }
        }
        // Every kill sentence, both failures, three refusals and the teleport.
        assert_eq!(plain, 36 + 2 + 3 + 1);
    }
}

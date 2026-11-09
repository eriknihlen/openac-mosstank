//! The combat chat lines: every pattern of Mag-Tools' combat tracker
//! (`CombatMessages`, `Util.IsChat`), in its order, with .NET's regular
//! expression semantics ([`DotNetRegex`]: the look-ahead of the Gossamer
//! Flesh pattern, `$` before a final line break); the element words; the
//! aetheria and cloak lines; and the fixes to Mag-Tools' misses, each its
//! own function:
//!
//! - F1 [`f1_reckless_crit_rest`]: own critical hits with `Recklessness!`.
//! - F2 [`f2_spell_hit_without_prefixes`]: spell hits with `Overpower!` or
//!   `Sneak Attack!`.
//! - F3 [`f3_outgoing_drain`]: outgoing drains (`With {spell} you drain …`).
//! - F4 [`f4_killed_target`]: the kill lines `{X} died!` and the PK one.
//! - F5 [`f5_cloak_weave`]: the third-person cloak line, when it is ours.
//! - F6 [`f6_strip_incoming_prefixes`]: incoming prefixes out of the
//!   attacker's name.
//!
//! (F10, the plugin's own lines, is the tracker's: [`super::PostedLines`].)
//!
//! A pattern costs a few million instructions to compile, so they are
//! compiled a few at a time ([`Patterns::compile_some`]); a line is parsed
//! once they all are.

use ac_plugin_abi::ChatLine;
use openac_plugin_compat::regex::{DotNetRegex, RegexOptions};

use super::info::{AttackType, CombatEvent, DamageElement, SurgeEvent};

/// `Util.IsChat`'s patterns: speech and tells of every form.
const IS_CHAT: [&str; 7] = [
    r#"^<Tell:IIDString:[0-9]+:(?<name>[\w\s'-]+)>[\w\s'-]+<\\Tell> says, "(?<msg>.*)"$"#,
    r#"^\[(?<channel>.+)]+ <Tell:IIDString:[0-9]+:(?<name>[\w\s'-]+)>[\w\s'-]+<\\Tell> says, "(?<msg>.*)"$"#,
    r#"^You say, "(?<msg>.*)"$"#,
    r#"^<Tell:IIDString:[0-9]+:(?<name>[\w\s'-]+)>[\w\s'-]+<\\Tell> tells you, "(?<msg>.*)"$"#,
    r#"^You tell .+, "(?<msg>.*)"$"#,
    r#"^(?<name>[\w\s'-]+) says, "(?<msg>.*)"$"#,
    r#"^(?<name>[\w\s'-]+) tells you, "(?<msg>.*)"$"#,
];

/// `FailedAttacks`.
const FAILED: [&str; 4] = [
    r"^You evaded (?<targetname>.+)!$",
    r"^(?<targetname>.+) evaded your attack.$",
    r"^You resist the spell cast by (?<targetname>.+)$",
    r"^(?<targetname>.+) resists your spell$",
];

/// `TargetKilledByMe`.
const KILLED_BY_ME: [&str; 36] = [
    r"^You flatten (?<targetname>.+)'s body with the force of your assault!$",
    r"^You bring (?<targetname>.+) to a fiery end!$",
    r"^You beat (?<targetname>.+) to a lifeless pulp!$",
    r"^You smite (?<targetname>.+) mightily!$",
    r"^You obliterate (?<targetname>.+)!$",
    r"^You run (?<targetname>.+) through!$",
    r"^You reduce (?<targetname>.+) to a sizzling, oozing mass!$",
    r"^You knock (?<targetname>.+) into next Morningthaw!$",
    r"^You split (?<targetname>.+) apart!$",
    r"^You cleave (?<targetname>.+) in twain!$",
    r"^You slay (?<targetname>.+) viciously enough to impart death several times over!$",
    r"^You reduce (?<targetname>.+) to a drained, twisted corpse!$",
    r"^Your killing blow nearly turns (?<targetname>.+) inside-out!$",
    r"^Your attack stops (?<targetname>.+) cold!$",
    r"^Your lightning coruscates over (?<targetname>.+)'s mortal remains!$",
    r"^Your assault sends (?<targetname>.+) to an icy death!$",
    r"^You killed (?<targetname>.+)!$",
    r"^The thunder of crushing (?<targetname>.+) is followed by the deafening silence of death!$",
    r"^The deadly force of your attack is so strong that (?<targetname>.+)'s ancestors feel it!$",
    r"^(?<targetname>.+)'s seared corpse smolders before you!$",
    r"^(?<targetname>.+) is reduced to cinders!$",
    r"^(?<targetname>.+) is shattered by your assault!$",
    r"^(?<targetname>.+) catches your attack, with dire consequences!$",
    r"^(?<targetname>.+) is utterly destroyed by your attack!$",
    r"^(?<targetname>.+) suffers a frozen fate!$",
    r"^(?<targetname>.+)'s perforated corpse falls before you!$",
    r"^(?<targetname>.+) is fatally punctured!$",
    r"^(?<targetname>.+)'s death is preceded by a sharp, stabbing pain!$",
    r"^(?<targetname>.+) is torn to ribbons by your assault!$",
    r"^(?<targetname>.+) is liquified by your attack!$",
    r"^(?<targetname>.+)'s last strength dissolves before you!$",
    r"^Electricity tears (?<targetname>.+) apart!$",
    r"^Blistered by lightning, (?<targetname>.+) falls!$",
    r"^(?<targetname>.+)'s last strength withers before you!$",
    r"^(?<targetname>.+) is dessicated by your attack!$",
    r"^(?<targetname>.+) is incinerated by your assault!$",
];

/// `MeleeMissileReceivedAttacks`.
const MELEE_MISSILE_RECEIVED: [&str; 4] = [
    r"^Critical hit! Overpower! (?<targetname>.+) [\w]+ your .+ for (?<points>.*) point.* of .+ damage.*$",
    r"^Critical hit! (?<targetname>.+) [\w]+ your .+ for (?<points>.*) point.* of .+ damage.*$",
    r"^Overpower! (?<targetname>.+) [\w]+ your .+ for (?<points>.+) point.* of .+ damage.*$",
    r"^(?<targetname>.+) [\w]+ your .+ for (?<points>.+) point.* of .+ damage.*$",
];

/// `MeleeMissileGivenAttacks`.
const MELEE_MISSILE_GIVEN: [&str; 6] = [
    r"^Critical hit!  You [\w]+ (?<targetname>.*) for (?<points>.+) point.* of .+ damage.*$",
    r"^You [\w]+ (?<targetname>.*) for (?<points>.+) point.* of .+ damage.*$",
    r"^Critical hit!  Sneak Attack! You [\w]+ (?<targetname>.*) for (?<points>.+) point.* of .+ damage.*$",
    r"^Sneak Attack! You [\w]+ (?<targetname>.*) for (?<points>.+) point.* of .+ damage.*$",
    r"^Sneak Attack! Recklessness! You [\w]+ (?<targetname>.*) for (?<points>.+) point.* of .+ damage.*$",
    r"^Recklessness! You [\w]+ (?<targetname>.*) for (?<points>.+) point.* of .+ damage.*$",
];

/// `MagicReceivedAttacks`.
const MAGIC_RECEIVED: [&str; 7] = [
    r"^Critical hit! Overpower! (?<targetname>.+) [\w]+ you for (?<points>.+) point.* with .+$",
    r"^Critical hit! (?<targetname>.+) [\w]+ you for (?<points>.+) point.* with .+$",
    r"^Overpower! (?<targetname>.+) [\w]+ you for (?<points>.+) point.* with .+$",
    r"^(?<targetname>.+) [\w]+ you for (?<points>.+) point.* with .+$",
    r"^Magical energies lose (?<points>.+) point.* of health due to (?<targetname>.+) casting .+$",
    r"^You lose (?<points>.+) point.* of health due to (?<targetname>.+) casting .+$",
    r"^(?<targetname>.+) casts .+ and drains (?<points>.+) point.* .+$",
];

/// `MagicGivenAttacks`.
const MAGIC_GIVEN: [&str; 2] = [
    r"^Critical hit! You [\w]+ (?<targetname>.+) for (?<points>.+) point.* with .+$",
    r"^You [\w]+ (?<targetname>.+) for (?<points>.+) point.* with .+$",
];

/// `MagicCastAttacks`: a Gossamer Flesh cast counts as a magic attack.
const MAGIC_CAST: [&str; 2] = [
    r"^You cast Gossamer Flesh on (?<targetname>((?!, ).)+)$",
    r"^You cast Gossamer Flesh on (?<targetname>.+), .+$",
];

/// The lists in the order they are compiled; a list's patterns stay in
/// their own order.
const LISTS: [&[&str]; 8] = [
    &IS_CHAT,
    &FAILED,
    &KILLED_BY_ME,
    &MELEE_MISSILE_RECEIVED,
    &MELEE_MISSILE_GIVEN,
    &MAGIC_RECEIVED,
    &MAGIC_GIVEN,
    &MAGIC_CAST,
];

/// Every pattern as Mag-Tools wrote it, in the order compiled.
pub fn all_patterns() -> impl Iterator<Item = &'static str> {
    LISTS.iter().flat_map(|list| list.iter().copied())
}

/// Every pattern, all lists.
pub const PATTERN_COUNT: usize = 7 + 4 + 36 + 4 + 6 + 7 + 2 + 2;

/// What a pattern costs to compile, in units of about two million
/// instructions: a plain one two, and five more for each Unicode word
/// class (measured natively, compiled as [`single_end_anchor`] writes
/// them: 0.08-0.16 ms plain, 0.38 ms with one `\w`, 0.93 ms with two `\w`
/// and two `\s`; some 27 million instructions a native millisecond).
#[must_use]
pub fn compile_cost(pattern: &str) -> usize {
    2 + 5 * pattern.matches(r"\w").count()
}

/// The pattern with its final `$` written `\n?\z`. Without `Multiline`,
/// .NET's `$` is the end or the place before a final `\n`; the part each
/// of these patterns ends with matches no line break (a literal, `.`, or
/// `.*`), so the two match the same lines with the same groups. The
/// compatibility kit compiles a `$` pattern twice (for a line with and
/// one without the final break); this one once, at half the cost.
#[must_use]
pub fn single_end_anchor(pattern: &str) -> std::borrow::Cow<'_, str> {
    match pattern.strip_suffix('$') {
        Some(body) if !body.ends_with('\\') => format!("{body}\\n?\\z").into(),
        _ => pattern.into(),
    }
}

/// Mag-Tools' patterns, compiled a few at a time.
#[derive(Debug, Clone, Default)]
pub struct Patterns {
    compiled: Vec<DotNetRegex>,
}

/// The patterns of one list, once compiled.
struct Lists<'a> {
    is_chat: &'a [DotNetRegex],
    failed: &'a [DotNetRegex],
    killed_by_me: &'a [DotNetRegex],
    melee_missile_received: &'a [DotNetRegex],
    melee_missile_given: &'a [DotNetRegex],
    magic_received: &'a [DotNetRegex],
    magic_given: &'a [DotNetRegex],
    magic_cast: &'a [DotNetRegex],
}

impl Patterns {
    /// Every pattern compiled now (the tests; the plugin compiles them a
    /// few a tick).
    #[must_use]
    pub fn compiled() -> Self {
        let mut patterns = Self::default();
        while !patterns.compile_some(usize::MAX) {}
        patterns
    }

    /// Compiles more patterns, at least one, until their cost
    /// ([`compile_cost`]) reaches `budget`; true when all are.
    pub fn compile_some(&mut self, budget: usize) -> bool {
        let mut spent = 0usize;
        while let Some(text) = all_patterns().nth(self.compiled.len()) {
            let cost = compile_cost(text);
            if spent > 0 && spent.saturating_add(cost) > budget {
                break;
            }
            spent = spent.saturating_add(cost);
            // The patterns are this file's own and all compile.
            match DotNetRegex::new(&single_end_anchor(text), RegexOptions::NONE) {
                Ok(regex) => self.compiled.push(regex),
                Err(error) => unreachable!("combat pattern {text:?}: {error}"),
            }
        }
        self.is_ready()
    }

    /// Every pattern is compiled.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.compiled.len() == PATTERN_COUNT
    }

    fn lists(&self) -> Option<Lists<'_>> {
        if !self.is_ready() {
            return None;
        }
        let mut rest = self.compiled.as_slice();
        let mut take = |n: usize| {
            let (head, tail) = rest.split_at(n);
            rest = tail;
            head
        };
        Some(Lists {
            is_chat: take(IS_CHAT.len()),
            failed: take(FAILED.len()),
            killed_by_me: take(KILLED_BY_ME.len()),
            melee_missile_received: take(MELEE_MISSILE_RECEIVED.len()),
            melee_missile_given: take(MELEE_MISSILE_GIVEN.len()),
            magic_received: take(MAGIC_RECEIVED.len()),
            magic_given: take(MAGIC_GIVEN.len()),
            magic_cast: take(MAGIC_CAST.len()),
        })
    }

    /// `Util.IsChat`: the line is speech or a tell (`None` before the
    /// patterns are compiled).
    #[must_use]
    pub fn is_chat(&self, text: &str) -> Option<bool> {
        Some(
            self.lists()?
                .is_chat
                .iter()
                .any(|regex| regex.is_match(text).unwrap_or(false)),
        )
    }

    /// What a chat line tells the combat tracker of `me` (`None` before
    /// the patterns are compiled): the three parsers of Mag-Tools, each
    /// on the line's printed text (`display_text`), with the fixes.
    #[must_use]
    pub fn parse(&self, me: &str, line: &ChatLine) -> Option<ParsedLine> {
        let lists = self.lists()?;
        let text = line.display_text.as_str();
        let mut parsed = ParsedLine::default();
        if text.is_empty()
            || lists
                .is_chat
                .iter()
                .any(|regex| regex.is_match(text).unwrap_or(false))
        {
            return Some(parsed);
        }
        let (combat, warnings) = standard(&lists, me, line);
        parsed.combat = combat;
        parsed.warnings = warnings;
        parsed.aetheria = aetheria(text);
        parsed.cloak = cloak(me, text);
        Some(parsed)
    }
}

/// What one chat line told.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedLine {
    /// An attack, failure or kill (Mag-Tools' `StandardTracker`).
    pub combat: Option<CombatEvent>,
    /// An aetheria surge (`AetheriaTracker`).
    pub aetheria: Option<SurgeEvent>,
    /// A cloak surge (`CloakTracker`).
    pub cloak: Option<SurgeEvent>,
    /// The "Unable to parse …" lines Mag-Tools writes to chat (without
    /// its prefix), written once (F15; Mag-Tools wrote them once per tracker).
    pub warnings: Vec<String>,
}

/// The first of `regexes` that matches `text`.
fn first_match<'r, 't>(
    regexes: &'r [DotNetRegex],
    text: &'t str,
) -> Option<openac_plugin_compat::regex::RegexMatch<'r, 't>> {
    regexes
        .iter()
        .find_map(|regex| regex.find(text).ok().flatten())
}

/// A captured group's text, owned.
fn group(found: &openac_plugin_compat::regex::RegexMatch<'_, '_>, name: &str) -> String {
    found.group(name).value.to_owned()
}

/// `int.TryParse(points)`: 0 when it does not read.
fn points(found: &openac_plugin_compat::regex::RegexMatch<'_, '_>) -> i32 {
    mosstank_dotnet::try_parse_int(found.group("points").value).unwrap_or(0)
}

/// `StandardTracker.Current_ChatBoxMessage` after its speech test: the
/// event and the "Unable to parse" lines.
fn standard(lists: &Lists<'_>, me: &str, line: &ChatLine) -> (Option<CombatEvent>, Vec<String>) {
    let text = line.display_text.as_str();
    let mut source = String::new();
    let mut target = String::new();
    let mut attack_type = AttackType::Unknown;
    let mut element = DamageElement::Unknown;
    let mut is_failed = false;
    let is_crit = text.contains("Critical hit!");
    let mut is_killing_blow = false;
    let mut damage = 0;

    if let Some(found) = first_match(lists.failed, text) {
        is_failed = true;
        let name = group(&found, "targetname");
        if text.starts_with("You evaded ") {
            source = name;
            me.clone_into(&mut target);
            attack_type = AttackType::MeleeMissle;
        } else if text.contains(" evaded your attack") {
            me.clone_into(&mut source);
            target = name;
            attack_type = AttackType::MeleeMissle;
        } else if text.starts_with("You resist the spell cast by ") {
            source = name;
            me.clone_into(&mut target);
            attack_type = AttackType::Magic;
        } else if text.contains(" resists your spell") {
            me.clone_into(&mut source);
            target = name;
            attack_type = AttackType::Magic;
        }
    } else if let Some(found) = first_match(lists.killed_by_me, text) {
        is_killing_blow = true;
        me.clone_into(&mut source);
        target = group(&found, "targetname");
    } else if let Some(killed) = f4_killed_target(line) {
        is_killing_blow = true;
        me.clone_into(&mut source);
        killed.clone_into(&mut target);
    } else {
        let mut hit = |from: String, to: String, kind: AttackType, element_now, points_now| {
            source = from;
            target = to;
            attack_type = kind;
            element = element_now;
            damage = points_now;
        };
        if let Some(found) = first_match(lists.melee_missile_received, text) {
            let name = f6_strip_incoming_prefixes(found.group("targetname").value);
            hit(
                name.to_owned(),
                me.to_owned(),
                AttackType::MeleeMissle,
                element_from_text(text),
                points(&found),
            );
        } else if let Some(found) = first_match(lists.melee_missile_given, text).or_else(|| {
            f1_reckless_crit_rest(text)
                .and_then(|rest| lists.melee_missile_given[1].find(rest).ok().flatten())
        }) {
            hit(
                me.to_owned(),
                group(&found, "targetname"),
                AttackType::MeleeMissle,
                element_from_text(text),
                points(&found),
            );
        } else if let Some(found) = first_match(lists.magic_received, text) {
            let name = f6_strip_incoming_prefixes(found.group("targetname").value);
            hit(
                name.to_owned(),
                me.to_owned(),
                AttackType::Magic,
                element_from_text(text),
                points(&found),
            );
        } else if let Some((name, amount)) = magic_given(lists, text) {
            hit(
                me.to_owned(),
                name,
                AttackType::Magic,
                element_from_text(text),
                amount,
            );
        } else if let Some((name, amount)) = f3_outgoing_drain(text) {
            hit(
                me.to_owned(),
                name.to_owned(),
                AttackType::Magic,
                DamageElement::Typeless,
                amount,
            );
        } else if let Some(found) = first_match(lists.magic_cast, text) {
            hit(
                me.to_owned(),
                group(&found, "targetname"),
                AttackType::Magic,
                DamageElement::None,
                0,
            );
        }
    }

    if source.is_empty() && target.is_empty() {
        return (None, Vec::new());
    }
    let mut warnings = Vec::new();
    if !is_killing_blow && attack_type == AttackType::Unknown {
        warnings.push(format!("Unable to parse attack type from: {text}"));
    }
    if !is_killing_blow && !is_failed && element == DamageElement::Unknown {
        warnings.push(format!("Unable to parse damage element from: {text}"));
    }
    let event = CombatEvent {
        source,
        target,
        attack_type,
        element,
        is_failed,
        is_crit,
        is_killing_blow,
        damage,
    };
    (Some(event), warnings)
}

/// `MagicGivenAttacks` on the line, then (F2) on the line without its
/// `Overpower!` and `Sneak Attack!` prefixes: the target and the points.
fn magic_given(lists: &Lists<'_>, text: &str) -> Option<(String, i32)> {
    if let Some(found) = first_match(lists.magic_given, text) {
        return Some((group(&found, "targetname"), points(&found)));
    }
    let stripped = f2_spell_hit_without_prefixes(text)?;
    let found = first_match(lists.magic_given, &stripped)?;
    Some((group(&found, "targetname"), points(&found)))
}

/// `GetElementFromText`: case-sensitive words anywhere in the line (names
/// and spells included), the first rule that hits.
#[must_use]
pub fn element_from_text(text: &str) -> DamageElement {
    const RULES: [(&[&str], DamageElement); 9] = [
        (
            &["slash", " cut", " scratch", " mangle"],
            DamageElement::Slash,
        ),
        (
            &["pierc", " gore", " impale", " nick", " stab"],
            DamageElement::Pierce,
        ),
        (
            &["bludge", " smash", " bash", " graze", " crush"],
            DamageElement::Bludge,
        ),
        (
            &["fire", " burn", " singe", " scorch", " incinerate"],
            DamageElement::Fire,
        ),
        (
            &["cold", " frost", " chill", " numb", " freeze"],
            DamageElement::Cold,
        ),
        (
            &["acid", " blister", " sear", " corrode", " dissolve"],
            DamageElement::Acid,
        ),
        (
            &[
                "electric",
                " lightning",
                " jolt",
                " shock",
                " spark",
                " blast",
            ],
            DamageElement::Electric,
        ),
        // Nether.
        (
            &[" eradicate", " wither", " scar", " twist"],
            DamageElement::Typeless,
        ),
        // Typeless and life.
        (
            &[
                " deplete", " siphon", " exhaust", " drain", " lose ", " health ",
            ],
            DamageElement::Typeless,
        ),
    ];
    RULES
        .iter()
        .find(|(words, _)| words.iter().any(|word| text.contains(word)))
        .map_or(DamageElement::Unknown, |(_, element)| *element)
}

/// `AetheriaTracker`: an aetheria surge (its source is the character only
/// for the surges that land on their owner).
#[must_use]
pub fn aetheria(text: &str) -> Option<SurgeEvent> {
    // The cast and expiry lines of the same surge are not counted.
    if text.starts_with("You cast") || text.contains("has expired.") {
        return None;
    }
    let mut source = String::new();
    let mut target = String::new();
    if text.starts_with("Aetheria surges on ") && text.contains(" with the ") {
        let rest = text.replace("Aetheria surges on ", "");
        if let Some(at) = rest.find(" with the ") {
            rest[..at].clone_into(&mut target);
        }
        if [
            "Surge of Destruction",
            "Surge of Protection",
            "Surge of Regeneration",
        ]
        .iter()
        .any(|surge| text.contains(surge))
        {
            source.clone_from(&target);
        }
    }
    let known = [
        "Surge of Destruction",
        "Surge of Protection",
        "Surge of Regeneration",
        "Surge of Affliction",
        "Surge of Festering",
    ]
    .iter()
    .any(|surge| text.contains(surge));
    known.then_some(SurgeEvent { source, target })
}

/// `CloakTracker`: a cloak surge, with F5's third-person line.
#[must_use]
pub fn cloak(me: &str, text: &str) -> Option<SurgeEvent> {
    let mut source = String::new();
    let mut target = String::new();
    if text.starts_with("You cast ") || text.starts_with("Your cloak ") {
        if text.contains("Shroud of Darkness") && !text.contains("yourself") {
            me.clone_into(&mut source);
            if let Some(at) = text.find(" on ") {
                text[at + 4..].clone_into(&mut target);
            }
        }
        if text.contains("Cloaked in Skill") || text.contains("Your cloak reduced the damage") {
            me.clone_into(&mut source);
            me.clone_into(&mut target);
        }
    } else if let Some(surge) = f5_cloak_weave(me, text) {
        source = surge.source;
        target = surge.target;
    }
    let known = [
        "Shroud of Darkness (Melee)",
        "Shroud of Darkness (Missile)",
        "Shroud of Darkness (Magic)",
        "Cloaked in Skill",
        "Your cloak reduced the damage",
    ]
    .iter()
    .any(|surge| text.contains(surge));
    known.then_some(SurgeEvent { source, target })
}

/// F1: `Critical hit!  Recklessness! You …` and
/// `Critical hit!  Sneak Attack! Recklessness! You …` (own critical hits
/// with Recklessness, which no Mag-Tools pattern takes): the line from
/// `You` on, for the plain own-hit pattern. The line's `Critical hit!`
/// still marks the critical hit.
#[must_use]
pub fn f1_reckless_crit_rest(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("Critical hit!  ")?;
    let rest = rest.strip_prefix("Sneak Attack! ").unwrap_or(rest);
    let rest = rest.strip_prefix("Recklessness! ")?;
    rest.starts_with("You ").then_some(rest)
}

/// F2: a spell hit with `Overpower! ` and/or `Sneak Attack! ` (also after
/// `Critical hit! `), which no Mag-Tools pattern takes: the line without
/// them, `Critical hit! ` kept as the critical-hit marker. `None` when the
/// line has neither.
#[must_use]
pub fn f2_spell_hit_without_prefixes(text: &str) -> Option<String> {
    let (crit, mut rest) = match text.strip_prefix("Critical hit! ") {
        Some(rest) => ("Critical hit! ", rest),
        None => ("", text),
    };
    let mut stripped = false;
    for prefix in ["Overpower! ", "Sneak Attack! "] {
        if let Some(after) = rest.strip_prefix(prefix) {
            rest = after;
            stripped = true;
        }
    }
    stripped.then(|| format!("{crit}{rest}"))
}

/// The text without one final line break (what .NET's `$` allows).
fn without_final_newline(text: &str) -> &str {
    text.strip_suffix('\n').unwrap_or(text)
}

/// F3: an outgoing drain, `With {spell} you drain {n} points of {vital}
/// from {target}.` (which Mag-Tools does not count): the target and the
/// points, a Typeless magic hit.
#[must_use]
pub fn f3_outgoing_drain(text: &str) -> Option<(&str, i32)> {
    let text = without_final_newline(text);
    let rest = text.strip_prefix("With ")?;
    let (spell, rest) = rest.split_once(" you drain ")?;
    if spell.is_empty() {
        return None;
    }
    let (amount, rest) = rest.split_once(' ')?;
    let amount = mosstank_dotnet::try_parse_int(amount)?;
    let rest = rest
        .strip_prefix("points of ")
        .or_else(|| rest.strip_prefix("point of "))?;
    let (vital, target) = rest.split_once(" from ")?;
    let target = target.strip_suffix('.')?;
    (!vital.is_empty() && !target.is_empty()).then_some((target, amount))
}

/// F4: the killer's lines no Mag-Tools pattern takes, `{X} died!` and
/// `You send {X} to death so violently that even the lifestone flinches!`:
/// the one killed. Only from a combat line telling of our own deed (kind
/// 8, outgoing), since `{X} died!` is worded the same for anyone.
#[must_use]
pub fn f4_killed_target(line: &ChatLine) -> Option<&str> {
    const COMBAT_LINE: u32 = 8;
    const OUTGOING: u32 = 1;
    if line.kind != COMBAT_LINE || line.combat_kind != OUTGOING {
        return None;
    }
    let text = without_final_newline(&line.display_text);
    let name = text
        .strip_prefix("You send ")
        .and_then(|rest| {
            rest.strip_suffix(" to death so violently that even the lifestone flinches!")
        })
        .or_else(|| text.strip_suffix(" died!"))?;
    (!name.is_empty()).then_some(name)
}

/// F5: the cloak line the server sends, `The cloak of {me} weaves the
/// magic of {spell}!` (Mag-Tools read only first-person lines, which the
/// server never sends): a surge of ours. A Shroud of Darkness lands on the
/// attacker, whom the line does not name (target ""); Cloaked in Skill on
/// us.
#[must_use]
pub fn f5_cloak_weave(me: &str, text: &str) -> Option<SurgeEvent> {
    if me.is_empty() {
        return None;
    }
    let spell = text
        .strip_prefix("The cloak of ")?
        .strip_prefix(me)?
        .strip_prefix(" weaves the magic of ")?;
    let target = if spell.contains("Cloaked in Skill") {
        me
    } else {
        ""
    };
    Some(SurgeEvent::new(me, target))
}

/// F6: the prefixes an incoming hit's line puts before the attacker's
/// name (`Critical hit! `, `Sneak Attack! `, `Reckless! `), which
/// Mag-Tools' patterns leave in the name.
#[must_use]
pub fn f6_strip_incoming_prefixes(mut name: &str) -> &str {
    loop {
        let before = name;
        for prefix in ["Critical hit! ", "Sneak Attack! ", "Reckless! "] {
            name = name.strip_prefix(prefix).unwrap_or(name);
        }
        if name == before {
            return name;
        }
    }
}

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

/// A fixed pattern, built on its first use or before it by
/// [`warm_one`] (building one costs the plugin some ten million
/// instructions, so they are built one a tick before they are needed).
#[derive(Debug)]
struct Pattern {
    text: &'static str,
    built: OnceLock<DotNetRegex>,
}

impl Pattern {
    const fn new(text: &'static str) -> Self {
        Self {
            text,
            built: OnceLock::new(),
        }
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
    patterns.iter().find_map(|pattern| {
        let Ok(Some(found)) = pattern.get().find(text) else {
            return None;
        };
        Some((
            found.group("spellname").value.to_owned(),
            found.group("targetname").value.to_owned(),
        ))
    })
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

/// Builds the next of these patterns not yet built; false when all are.
/// What a pattern answers does not depend on when it was built.
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
        .find(|pattern| pattern.built.get().is_none());
    next.map(Pattern::get).is_some()
}

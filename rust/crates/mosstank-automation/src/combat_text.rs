//! Reading the character's own combat log (`CombatResultText`): which of a
//! spell's result sentences a line is, the killing-blow sentences, and the
//! damage reports.

use std::sync::LazyLock;

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

fn compile(patterns: &[&str]) -> Vec<DotNetRegex> {
    patterns
        .iter()
        .map(|pattern| {
            DotNetRegex::new(
                pattern,
                RegexOptions::CULTURE_INVARIANT | RegexOptions::EXPLICIT_CAPTURE,
            )
            .expect("a fixed pattern")
        })
        .collect()
}

static FAIL: LazyLock<Vec<DotNetRegex>> = LazyLock::new(|| {
    compile(&[
        "^Your spell fizzled.$",
        "^(?<targetname>.*) resists your spell$",
    ])
});

static PERMANENT_FAIL: LazyLock<Vec<DotNetRegex>> = LazyLock::new(|| {
    compile(&[
        "^Target is out of range$",
        "^You fail to affect (?<targetname>.*) because you are not a player killer!$",
        "^You fail to affect (?<targetname>.*) because .* is not a player killer!$",
        "^You fail to affect (?<targetname>.*) because beneficial spells do not affect .*!$",
        "^(?<targetname>.*) is an invalid target.$",
    ])
});

static SUCCESS: LazyLock<Vec<DotNetRegex>> = LazyLock::new(|| {
    compile(&[
        "^You cast (?<spellname>.*) on (?<targetname>.*), refreshing .*$",
        "^You cast (?<spellname>.*) on (?<targetname>.*), surpassing .*$",
        "^You cast (?<spellname>.*) on (?<targetname>.*), but it is surpassed by .*$",
        "^You cast (?<spellname>.*) on (?<targetname>.*)$",
        "^You .* due to casting (?<spellname>.*) on (?<targetname>.*)$",
        r"^With (?<spellname>.*) you .* from (?<targetname>.*)\.$",
        r"^With (?<spellname>.*) you .* to (?<targetname>.*)\.$",
        r"^(Critical hit! )?(Sneak Attack! )?You .* .* for .* points with (?<spellname>.*)\.$",
        r"^You cast (?<spellname>.*) and restore .* points of your .*\.$",
        "^You cast (?<spellname>.*) on yourself and lose .* points of .* and also gain .* points of .*$",
        "^You have been teleported.$",
        r"^You cast (?<spellname>.*) on (?<targetname>.*) and dispel: .*\.$",
        "^You cast (?<spellname>.*) on (?<targetname>.*), but the dispel fails.$",
    ])
});

static KILL: LazyLock<Vec<DotNetRegex>> = LazyLock::new(|| {
    compile(&[
        "^You knock (?<targetname>.*) into next Morningthaw!$",
        "^You obliterate (?<targetname>.*)!$",
        "^(?<targetname>.*) is utterly destroyed by your attack!$",
        "^(?<targetname>.*) catches your attack, with dire consequences!$",
        "^The deadly force of your attack is so strong that (?<targetname>.*)'s ancestors feel it!$",
        "^You smite (?<targetname>.*) mightily!$",
        "^You slay (?<targetname>.*) viciously enough to impart death several times over!$",
        "^You killed (?<targetname>.*)!$",
        "^(?<targetname>.*) is torn to ribbons by your assault!$",
        "^You cleave (?<targetname>.*) in twain!$",
        "^Your killing blow nearly turns (?<targetname>.*) inside-out!$",
        "^You split (?<targetname>.*) apart!$",
        "^The thunder of crushing (?<targetname>.*) is followed by the deafening silence of death!$",
        "^You beat (?<targetname>.*) to a lifeless pulp!$",
        "^(?<targetname>.*) is shattered by your assault!$",
        "^You flatten (?<targetname>.*)'s body with the force of your assault!$",
        "^(?<targetname>.*) is fatally punctured!$",
        "^(?<targetname>.*)'s perforated corpse falls before you!$",
        "^You run (?<targetname>.*) through!$",
        "^(?<targetname>.*)'s death is preceded by a sharp, stabbing pain!$",
        "^You bring (?<targetname>.*) to a fiery end!$",
        "^(?<targetname>.*) is incinerated by your assault!$",
        "^(?<targetname>.*) is reduced to cinders!$",
        "^(?<targetname>.*)'s seared corpse smolders before you!$",
        "^Your lightning coruscates over (?<targetname>.*)'s mortal remains!$",
        "^Blistered by lightning, (?<targetname>.*) falls!$",
        "^Electricity tears (?<targetname>.*) apart!$",
        "^Your assault sends (?<targetname>.*) to an icy death!$",
        "^Your attack stops (?<targetname>.*) cold!$",
        "^(?<targetname>.*) suffers a frozen fate!$",
        "^(?<targetname>.*)'s last strength dissolves before you!$",
        "^(?<targetname>.*) is liquified by your attack!$",
        "^You reduce (?<targetname>.*) to a sizzling, oozing mass!$",
        "^You reduce (?<targetname>.*) to a drained, twisted corpse!$",
        "^(?<targetname>.*) is dessicated by your attack!$",
        "^(?<targetname>.*)'s last strength withers before you!$",
    ])
});

/// The one line that says a shot never reached its target: it hit the
/// world instead.
pub const MISSILE_HIT_ENVIRONMENT: &str = "Your missile attack hit the environment.";

static DAMAGE_REPORT: LazyLock<Vec<DotNetRegex>> =
    LazyLock::new(|| compile(&[r"^(Critical hit\!)?[ ]*You .* for .* point(s)? of .*\!$"]));

static SPELL_DAMAGE: LazyLock<Vec<DotNetRegex>> =
    LazyLock::new(|| compile(&["(?:You .* for )(?<points>[0-9]+)(?: points with .*)"]));

/// The first pattern that matches, with the spell and target it captured.
/// The fixed patterns run on one chat line, so the step limit is not
/// reached; a stopped match would count as no match.
fn try_match(patterns: &[DotNetRegex], text: &str) -> Option<(String, String)> {
    patterns.iter().find_map(|pattern| {
        let Ok(Some(found)) = pattern.find(text) else {
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
    let Ok(Some(found)) = SPELL_DAMAGE[0].find(text) else {
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
        (CombatResultTextClass::Kill, &*KILL),
        (CombatResultTextClass::PermanentFail, &*PERMANENT_FAIL),
        (CombatResultTextClass::Fail, &*FAIL),
        (CombatResultTextClass::Success, &*SUCCESS),
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

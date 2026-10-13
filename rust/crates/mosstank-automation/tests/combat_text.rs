//! The combat log's result sentences.

use mosstank_automation::combat_text::{CombatResultTextClass, classify};

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatResultTextTests.cs :: ClassifiesAuthenticOwnResultLines
#[test]
fn classifies_authentic_own_result_lines() {
    use CombatResultTextClass::{Fail, Kill, None, PermanentFail, Success};
    let cases = [
        // Plain fail.
        ("Your spell fizzled.", Fail, ""),
        ("Drudge Slinker resists your spell", Fail, "Drudge Slinker"),
        // Permanent fail.
        ("Target is out of range", PermanentFail, ""),
        (
            "Drudge Slinker is an invalid target.",
            PermanentFail,
            "Drudge Slinker",
        ),
        (
            "You fail to affect Drudge Slinker because you are not a player killer!",
            PermanentFail,
            "Drudge Slinker",
        ),
        // Success.
        (
            "You cast Frost Bolt VII on Drudge Slinker",
            Success,
            "Drudge Slinker",
        ),
        (
            "You cast Imperil Other VII on Drudge Slinker, refreshing Imperil Other VI",
            Success,
            "Drudge Slinker",
        ),
        // Kill.
        ("You killed Drudge Slinker!", Kill, "Drudge Slinker"),
        ("You obliterate Drudge Slinker!", Kill, "Drudge Slinker"),
        (
            "Drudge Slinker's perforated corpse falls before you!",
            Kill,
            "Drudge Slinker",
        ),
        (
            "You knock Drudge Slinker into next Morningthaw!",
            Kill,
            "Drudge Slinker",
        ),
        // Anything else.
        ("You say, \"Frost Bolt VII\"", None, ""),
        (
            "Drudge Slinker hits you for 12 points of slashing damage!",
            None,
            "",
        ),
    ];
    for (text, expected, target) in cases {
        let result = classify(text);
        assert_eq!(result.class, expected, "{text}");
        assert_eq!(result.target_name, target, "{text}");
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatResultTextTests.cs :: SuccessLineCapturesTheSpellName
#[test]
fn success_line_captures_the_spell_name() {
    let result = classify("You cast Frost Bolt VII on Drudge Slinker");
    assert_eq!(result.class, CombatResultTextClass::Success);
    assert_eq!(result.spell_name, "Frost Bolt VII");
    assert_eq!(result.target_name, "Drudge Slinker");
}

const AUTHENTIC_KILL_LINES: [&str; 36] = [
    "You knock X into next Morningthaw!",
    "You obliterate X!",
    "X is utterly destroyed by your attack!",
    "X catches your attack, with dire consequences!",
    "The deadly force of your attack is so strong that X's ancestors feel it!",
    "You smite X mightily!",
    "You slay X viciously enough to impart death several times over!",
    "You killed X!",
    "X is torn to ribbons by your assault!",
    "You cleave X in twain!",
    "Your killing blow nearly turns X inside-out!",
    "You split X apart!",
    "The thunder of crushing X is followed by the deafening silence of death!",
    "You beat X to a lifeless pulp!",
    "X is shattered by your assault!",
    "You flatten X's body with the force of your assault!",
    "X is fatally punctured!",
    "X's perforated corpse falls before you!",
    "You run X through!",
    "X's death is preceded by a sharp, stabbing pain!",
    "You bring X to a fiery end!",
    "X is incinerated by your assault!",
    "X is reduced to cinders!",
    "X's seared corpse smolders before you!",
    "Your lightning coruscates over X's mortal remains!",
    "Blistered by lightning, X falls!",
    "Electricity tears X apart!",
    "Your assault sends X to an icy death!",
    "Your attack stops X cold!",
    "X suffers a frozen fate!",
    "X's last strength dissolves before you!",
    "X is liquified by your attack!",
    "You reduce X to a sizzling, oozing mass!",
    "You reduce X to a drained, twisted corpse!",
    "X is dessicated by your attack!",
    "X's last strength withers before you!",
];

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatResultTextTests.cs :: EveryAuthenticKillVerbIsCovered
#[test]
fn every_authentic_kill_verb_is_covered() {
    assert_eq!(AUTHENTIC_KILL_LINES.len(), 36);
    for line in AUTHENTIC_KILL_LINES {
        assert_eq!(classify(line).class, CombatResultTextClass::Kill, "{line}");
    }
}

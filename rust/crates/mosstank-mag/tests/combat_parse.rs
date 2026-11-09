//! The combat tracker's chat lines: every Mag-Tools pattern with a line
//! OpenAC or ACE really prints (the research note on the combat tracker,
//! its cross-check), the fixes F1-F6 and F10, and what the trackers keep.

use std::sync::LazyLock;

use ac_plugin_abi::ChatLine;
use mosstank_mag::combat::info::{
    AttackType, CombatEvent, CombatInfo, DamageByElement, DamageElement, SurgeEvent,
};
use mosstank_mag::combat::patterns::{
    self, ParsedLine, Patterns, f1_reckless_crit_rest, f2_spell_hit_without_prefixes,
    f3_outgoing_drain, f4_killed_target, f5_cloak_weave, f6_strip_incoming_prefixes,
};
use mosstank_mag::combat::{Combat, CombatSettings, CombatTracker, PostedLines, TrackerUpdate};
use mosstank_mag::testing::FakeMagHost;

const ME: &str = "Acdream";

static PATTERNS: LazyLock<Patterns> = LazyLock::new(Patterns::compiled);

/// A client-worded combat line, outgoing (`combat_kind` 1).
fn out(text: &str) -> ChatLine {
    ChatLine {
        kind: 8,
        combat_kind: 1,
        log_text_type: 0x16,
        text: text.to_owned(),
        display_text: text.to_owned(),
        ..ChatLine::default()
    }
}

/// A client-worded combat line, incoming (`combat_kind` 2).
fn combat_in(text: &str) -> ChatLine {
    ChatLine {
        combat_kind: 2,
        log_text_type: 0x15,
        ..out(text)
    }
}

/// A server text line (kind 4, class Magic).
fn system(text: &str) -> ChatLine {
    ChatLine {
        kind: 4,
        combat_kind: 0,
        log_text_type: 0x07,
        ..out(text)
    }
}

fn parse(me: &str, line: &ChatLine) -> ParsedLine {
    PATTERNS.parse(me, line).expect("compiled")
}

fn event(line: &ChatLine) -> Option<CombatEvent> {
    parse(ME, line).combat
}

fn damage(
    source: &str,
    target: &str,
    attack_type: AttackType,
    element: DamageElement,
    crit: bool,
    points: i32,
) -> Option<CombatEvent> {
    Some(CombatEvent::damage(
        source,
        target,
        attack_type,
        element,
        crit,
        points,
    ))
}

use AttackType::{Magic, MeleeMissle};
use DamageElement as E;

// FailedAttacks

#[test]
fn you_evaded_is_their_failed_melee_attack() {
    assert_eq!(
        event(&combat_in("You evaded Drudge Skulker!")),
        Some(CombatEvent::failed("Drudge Skulker", ME, MeleeMissle))
    );
}

#[test]
fn evaded_your_attack_is_our_failed_melee_attack() {
    assert_eq!(
        event(&out("Drudge Skulker evaded your attack.")),
        Some(CombatEvent::failed(ME, "Drudge Skulker", MeleeMissle))
    );
}

#[test]
fn you_resist_is_their_failed_spell() {
    assert_eq!(
        event(&system("You resist the spell cast by Drudge Skulker")),
        Some(CombatEvent::failed("Drudge Skulker", ME, Magic))
    );
}

#[test]
fn resists_your_spell_is_our_failed_spell() {
    assert_eq!(
        event(&system("Drudge Skulker resists your spell")),
        Some(CombatEvent::failed(ME, "Drudge Skulker", Magic))
    );
}

// MeleeMissileReceivedAttacks

#[test]
fn received_crit_with_overpower() {
    // Mag-Tools' own example (OpenAC prints no Overpower text).
    assert_eq!(
        event(&combat_in(
            "Critical hit! Overpower! Lugian Launcher smashes your lower leg for 108 points of bludgeoning damage!"
        )),
        damage("Lugian Launcher", ME, MeleeMissle, E::Bludge, true, 108)
    );
}

#[test]
fn received_crit() {
    assert_eq!(
        event(&combat_in(
            "Critical hit! Virindi Executor scratches your upper leg for 3 points of slashing damage!"
        )),
        damage("Virindi Executor", ME, MeleeMissle, E::Slash, true, 3)
    );
}

#[test]
fn received_overpower() {
    assert_eq!(
        event(&combat_in(
            "Overpower! Lugian Launcher bashes your foot for 43 points of bludgeoning damage!"
        )),
        damage("Lugian Launcher", ME, MeleeMissle, E::Bludge, false, 43)
    );
}

#[test]
fn received_hit_with_the_augmentation_suffix() {
    assert_eq!(
        event(&combat_in(
            "Mosswart Stalker scorches your chest for 7 points of fire damage! Your Critical Protection augmentation allows you to avoid a critical hit!"
        )),
        damage("Mosswart Stalker", ME, MeleeMissle, E::Fire, false, 7)
    );
}

#[test]
fn received_single_point() {
    assert_eq!(
        event(&combat_in(
            "Ruschk Sadist numbs your lower leg for 1 point of cold damage!"
        )),
        damage("Ruschk Sadist", ME, MeleeMissle, E::Cold, false, 1)
    );
}

#[test]
fn received_prismatic_hit_is_counted_with_a_warning() {
    let parsed = parse(
        ME,
        &combat_in("Drudge Skulker hits your chest for 5 points of prismatic damage!"),
    );
    assert_eq!(
        parsed.combat,
        damage("Drudge Skulker", ME, MeleeMissle, E::Unknown, false, 5)
    );
    assert_eq!(
        parsed.warnings,
        vec![
            "Unable to parse damage element from: Drudge Skulker hits your chest for 5 points of prismatic damage!"
                .to_owned()
        ]
    );
}

#[test]
fn received_health_hit_has_no_pattern() {
    // Nothing between "of " and " damage": no pattern, as in retail.
    assert_eq!(
        event(&combat_in(
            "Drudge Skulker drains your chest for 5 points of damage!"
        )),
        None
    );
}

// MeleeMissileGivenAttacks

#[test]
fn own_crit() {
    assert_eq!(
        event(&out(
            "Critical hit!  You scorch Annihilator for 685 points of fire damage!"
        )),
        damage(ME, "Annihilator", MeleeMissle, E::Fire, true, 685)
    );
}

#[test]
fn own_melee_hit() {
    assert_eq!(
        event(&out(
            "You mangle Mosswart Defiler for 12 points of slashing damage!"
        )),
        damage(ME, "Mosswart Defiler", MeleeMissle, E::Slash, false, 12)
    );
}

#[test]
fn own_hit_with_the_augmentation_suffix() {
    assert_eq!(
        event(&out(
            "You hit Drudge Skulker for 1 point of piercing damage! Your target's Critical Protection augmentation allows them to avoid your critical hit!"
        )),
        damage(ME, "Drudge Skulker", MeleeMissle, E::Pierce, false, 1)
    );
}

#[test]
fn own_multi_type_hit() {
    assert_eq!(
        event(&out(
            "You hit Drudge Skulker for 30 points of slashing/piercing damage!"
        )),
        damage(ME, "Drudge Skulker", MeleeMissle, E::Slash, false, 30)
    );
}

#[test]
fn own_crit_sneak_attack() {
    assert_eq!(
        event(&out(
            "Critical hit!  Sneak Attack! You chill Biaka for 402 points of cold damage!"
        )),
        damage(ME, "Biaka", MeleeMissle, E::Cold, true, 402)
    );
}

#[test]
fn own_sneak_attack() {
    assert_eq!(
        event(&out(
            "Sneak Attack! You chill Biaka for 357 points of cold damage!"
        )),
        damage(ME, "Biaka", MeleeMissle, E::Cold, false, 357)
    );
}

#[test]
fn own_sneak_attack_recklessness() {
    assert_eq!(
        event(&out(
            "Sneak Attack! Recklessness! You crush Olthoi Swarm Soldier for 234 points of bludgeoning damage!"
        )),
        damage(
            ME,
            "Olthoi Swarm Soldier",
            MeleeMissle,
            E::Bludge,
            false,
            234
        )
    );
}

#[test]
fn own_recklessness() {
    assert_eq!(
        event(&out(
            "Recklessness! You scratch Reviath for 7 points of slashing damage!"
        )),
        damage(ME, "Reviath", MeleeMissle, E::Slash, false, 7)
    );
}

#[test]
fn periodic_nether_damage_counts_as_a_typeless_melee_hit() {
    // ACE's damage-over-time line, as Mag-Tools counted it.
    assert_eq!(
        event(&system(
            "You wither Drudge Skulker for 20 points of periodic nether damage!"
        )),
        damage(ME, "Drudge Skulker", MeleeMissle, E::Typeless, false, 20)
    );
}

// MagicReceivedAttacks

#[test]
fn received_spell_crit_with_overpower() {
    assert_eq!(
        event(&system(
            "Critical hit! Overpower! Virindi Rival shocks you for 96 points with Lightning Arc VII."
        )),
        damage("Virindi Rival", ME, Magic, E::Electric, true, 96)
    );
}

#[test]
fn received_spell_crit() {
    assert_eq!(
        event(&system(
            "Critical hit! Mag-salvager smashes you for 147 points with Shock Wave Streak VII."
        )),
        damage("Mag-salvager", ME, Magic, E::Bludge, true, 147)
    );
}

#[test]
fn received_spell_overpower() {
    assert_eq!(
        event(&system(
            "Overpower! Virindi Rival shocks you for 96 points with Lightning Arc VII."
        )),
        damage("Virindi Rival", ME, Magic, E::Electric, false, 96)
    );
}

#[test]
fn received_spell_hit() {
    assert_eq!(
        event(&system(
            "Crystal Shard Sentinel scorches you for 47 points with Flame Arc VII."
        )),
        damage("Crystal Shard Sentinel", ME, Magic, E::Fire, false, 47)
    );
}

#[test]
fn received_spell_hit_with_the_augmentation_suffix() {
    assert_eq!(
        event(&system(
            "Drudge Skulker freezes you for 30 points with Frost Bolt VI. Your augmentation allows you to avoid a critical hit!"
        )),
        damage("Drudge Skulker", ME, Magic, E::Cold, false, 30)
    );
}

#[test]
fn magical_energies_lose() {
    // Mag-Tools' own example; ACE never sends it.
    assert_eq!(
        event(&system(
            "Magical energies lose 1 point of health due to Sentient Crystal Shard casting Vitality Siphon"
        )),
        damage("Sentient Crystal Shard", ME, Magic, E::Typeless, false, 1)
    );
}

#[test]
fn you_lose_health_due_to_a_drain() {
    assert_eq!(
        event(&system(
            "You lose 39 points of health due to Infernal Zefir casting Drain Health Other V on you"
        )),
        damage("Infernal Zefir", ME, Magic, E::Typeless, false, 39)
    );
}

#[test]
fn casts_and_drains() {
    assert_eq!(
        event(&system(
            "Crystal Minion casts Harm Other VI and drains 39 points of your health."
        )),
        damage("Crystal Minion", ME, Magic, E::Typeless, false, 39)
    );
}

#[test]
fn the_casters_life_transfer_line_is_not_received_damage() {
    assert_eq!(
        event(&system(
            "You lose 39 points of health due to casting Drain Health Other V on Drudge Skulker"
        )),
        None
    );
}

// MagicGivenAttacks

#[test]
fn own_spell_crit() {
    assert_eq!(
        event(&system(
            "Critical hit! You smash Drudge Skulker for 147 points with Shock Wave Streak VII."
        )),
        damage(ME, "Drudge Skulker", Magic, E::Bludge, true, 147)
    );
}

#[test]
fn own_spell_hit() {
    assert_eq!(
        event(&system(
            "You scorch Drudge Skulker for 80 points with Flame Bolt VI."
        )),
        damage(ME, "Drudge Skulker", Magic, E::Fire, false, 80)
    );
}

#[test]
fn own_spell_hit_with_the_augmentation_suffix() {
    assert_eq!(
        event(&system(
            "Critical hit! You eradicate Drudge Skulker for 300 points with Nether Bolt VI. Your critical hit was avoided with their augmentation!"
        )),
        damage(ME, "Drudge Skulker", Magic, E::Typeless, true, 300)
    );
}

// MagicCastAttacks

#[test]
fn gossamer_flesh_cast_counts_as_an_attack() {
    assert_eq!(
        event(&system("You cast Gossamer Flesh on Chicken")),
        Some(CombatEvent {
            element: E::None,
            ..CombatEvent::damage(ME, "Chicken", Magic, E::None, false, 0)
        })
    );
}

#[test]
fn gossamer_flesh_refreshed() {
    assert_eq!(
        event(&system(
            "You cast Gossamer Flesh on Chicken, refreshing Gossamer Flesh"
        )),
        damage(ME, "Chicken", Magic, E::None, false, 0)
    );
}

#[test]
fn other_casts_are_not_attacks() {
    assert_eq!(event(&system("You cast Harm Other VI on Chicken")), None);
}

// TargetKilledByMe: ACE's killer strings, each with the monster's name.

#[test]
fn every_kill_message_counts_a_killing_blow() {
    let lines = [
        "You flatten Drudge Skulker's body with the force of your assault!",
        "You bring Drudge Skulker to a fiery end!",
        "You beat Drudge Skulker to a lifeless pulp!",
        "You smite Drudge Skulker mightily!",
        "You obliterate Drudge Skulker!",
        "You run Drudge Skulker through!",
        "You reduce Drudge Skulker to a sizzling, oozing mass!",
        "You knock Drudge Skulker into next Morningthaw!",
        "You split Drudge Skulker apart!",
        "You cleave Drudge Skulker in twain!",
        "You slay Drudge Skulker viciously enough to impart death several times over!",
        "You reduce Drudge Skulker to a drained, twisted corpse!",
        "Your killing blow nearly turns Drudge Skulker inside-out!",
        "Your attack stops Drudge Skulker cold!",
        "Your lightning coruscates over Drudge Skulker's mortal remains!",
        "Your assault sends Drudge Skulker to an icy death!",
        "You killed Drudge Skulker!",
        "The thunder of crushing Drudge Skulker is followed by the deafening silence of death!",
        "The deadly force of your attack is so strong that Drudge Skulker's ancestors feel it!",
        "Drudge Skulker's seared corpse smolders before you!",
        "Drudge Skulker is reduced to cinders!",
        "Drudge Skulker is shattered by your assault!",
        "Drudge Skulker catches your attack, with dire consequences!",
        "Drudge Skulker is utterly destroyed by your attack!",
        "Drudge Skulker suffers a frozen fate!",
        "Drudge Skulker's perforated corpse falls before you!",
        "Drudge Skulker is fatally punctured!",
        "Drudge Skulker's death is preceded by a sharp, stabbing pain!",
        "Drudge Skulker is torn to ribbons by your assault!",
        "Drudge Skulker is liquified by your attack!",
        "Drudge Skulker's last strength dissolves before you!",
        "Electricity tears Drudge Skulker apart!",
        "Blistered by lightning, Drudge Skulker falls!",
        "Drudge Skulker's last strength withers before you!",
        "Drudge Skulker is dessicated by your attack!",
        "Drudge Skulker is incinerated by your assault!",
    ];
    for text in lines {
        let parsed = parse(ME, &out(text));
        assert_eq!(
            parsed.combat,
            Some(CombatEvent::killing_blow(ME, "Drudge Skulker")),
            "{text}"
        );
        assert!(parsed.warnings.is_empty(), "{text}");
    }
}

#[test]
fn the_killer_text_keeps_its_final_line_break() {
    // The server's killer text reaches the plugin untrimmed; .NET's `$`
    // allows one final line break.
    assert_eq!(
        event(&out("You obliterate Drudge Skulker!\n")),
        Some(CombatEvent::killing_blow(ME, "Drudge Skulker"))
    );
}

#[test]
fn victim_and_bystander_death_lines_are_not_our_kills() {
    for text in [
        "Drudge Skulker splits you apart!",
        "Horan is fatally punctured by Drudge Skulker!",
    ] {
        let line = ChatLine {
            combat_kind: 3,
            ..out(text)
        };
        assert_eq!(event(&line), None, "{text}");
    }
}

// Lines that must not match.

#[test]
fn lines_that_are_no_attack() {
    for text in [
        "You suffer 12 points of minor impact damage.",
        "The Lifestone's magic protects Drudge Skulker from the attack!",
        "You receive 12 points of periodic damage.",
        "Your Strength Self VIII spell has expired.",
        "Horan has logged in.",
    ] {
        assert_eq!(parse(ME, &system(text)), ParsedLine::default(), "{text}");
    }
}

// Util.IsChat

#[test]
fn speech_is_skipped() {
    let words = "You hit Drudge Skulker for 25 points of slashing damage!";
    for text in [
        format!("<Tell:IIDString:1342177282:Horan>Horan<\\Tell> says, \"{words}\""),
        format!("[General] <Tell:IIDString:1342177282:Horan>Horan<\\Tell> says, \"{words}\""),
        format!("You say, \"{words}\""),
        format!("<Tell:IIDString:1342177282:Horan>Horan<\\Tell> tells you, \"{words}\""),
        format!("You tell Horan, \"{words}\""),
        format!("Town Crier says, \"{words}\""),
        format!("Town Crier tells you, \"{words}\""),
    ] {
        let line = ChatLine {
            kind: 0,
            ..out(&text)
        };
        assert_eq!(parse(ME, &line), ParsedLine::default(), "{text}");
        assert_eq!(PATTERNS.is_chat(&text), Some(true), "{text}");
    }
    assert_eq!(PATTERNS.is_chat(words), Some(false));
}

#[test]
fn the_printed_line_is_parsed_not_the_words() {
    // A tell's words alone would read as a hit; its printed line is speech.
    let line = ChatLine {
        kind: 3,
        text: "You hit Drudge Skulker for 25 points of slashing damage!".to_owned(),
        display_text:
            "<Tell:IIDString:1:Horan>Horan<\\Tell> tells you, \"You hit Drudge Skulker for 25 points of slashing damage!\""
                .to_owned(),
        ..ChatLine::default()
    };
    assert_eq!(parse(ME, &line), ParsedLine::default());
}

// Element words

#[test]
fn elements_follow_mag_tools_order() {
    assert_eq!(patterns::element_from_text("x slashing"), E::Slash);
    // The first rule that hits wins: "slash" before "fire".
    assert_eq!(patterns::element_from_text("fire slash"), E::Slash);
    assert_eq!(patterns::element_from_text("x scorch"), E::Fire);
    assert_eq!(patterns::element_from_text("x blast"), E::Electric);
    assert_eq!(patterns::element_from_text("x twist"), E::Typeless);
    assert_eq!(patterns::element_from_text("x siphon"), E::Typeless);
    // Case-sensitive.
    assert_eq!(patterns::element_from_text("Slashing"), E::Unknown);
}

// Aetheria

#[test]
fn own_aetheria_surge_counts() {
    assert_eq!(
        parse(
            ME,
            &system("Aetheria surges on Acdream with the power of Surge of Destruction!")
        )
        .aetheria,
        Some(SurgeEvent::new(ME, ME))
    );
}

#[test]
fn a_surge_on_a_monster_has_no_source() {
    assert_eq!(
        parse(
            ME,
            &system("Aetheria surges on Drudge Skulker with the power of Surge of Festering!")
        )
        .aetheria,
        Some(SurgeEvent::new("", "Drudge Skulker"))
    );
}

#[test]
fn the_cast_line_of_a_surge_is_not_counted() {
    assert_eq!(
        parse(ME, &system("You cast Surge of Destruction on yourself")).aetheria,
        None
    );
    assert_eq!(
        parse(ME, &system("Surge of Destruction has expired.")).aetheria,
        None
    );
}

// Cloak

#[test]
fn cloak_damage_reduction_counts() {
    assert_eq!(
        parse(
            ME,
            &system("Your cloak reduced the damage from 162 down to 0!")
        )
        .cloak,
        Some(SurgeEvent::new(ME, ME))
    );
}

#[test]
fn first_person_shroud_names_its_target() {
    assert_eq!(
        parse(
            ME,
            &system("You cast Shroud of Darkness (Melee) on Invading Iron Blade Knight")
        )
        .cloak,
        Some(SurgeEvent::new(ME, "Invading Iron Blade Knight"))
    );
}

// The fixes

#[test]
fn own_crit_with_recklessness_counts() {
    // F1
    assert_eq!(
        f1_reckless_crit_rest(
            "Critical hit!  Recklessness! You hit Drudge Skulker for 50 points of slashing damage!"
        ),
        Some("You hit Drudge Skulker for 50 points of slashing damage!")
    );
    assert_eq!(
        f1_reckless_crit_rest(
            "Recklessness! You hit Drudge Skulker for 5 points of slashing damage!"
        ),
        None
    );
    assert_eq!(
        event(&out(
            "Critical hit!  Recklessness! You hit Drudge Skulker for 50 points of slashing damage!"
        )),
        damage(ME, "Drudge Skulker", MeleeMissle, E::Slash, true, 50)
    );
    assert_eq!(
        event(&out(
            "Critical hit!  Sneak Attack! Recklessness! You crush Drudge Skulker for 90 points of bludgeoning damage!"
        )),
        damage(ME, "Drudge Skulker", MeleeMissle, E::Bludge, true, 90)
    );
}

#[test]
fn sneak_attack_spell_hit_counts() {
    // F2
    assert_eq!(
        f2_spell_hit_without_prefixes(
            "Critical hit! Overpower! Sneak Attack! You smash X for 1 points with Y."
        ),
        Some("Critical hit! You smash X for 1 points with Y.".to_owned())
    );
    assert_eq!(
        f2_spell_hit_without_prefixes("You smash X for 1 points with Y."),
        None
    );
    assert_eq!(
        event(&system(
            "Sneak Attack! You scorch Drudge Skulker for 80 points with Flame Bolt VI."
        )),
        damage(ME, "Drudge Skulker", Magic, E::Fire, false, 80)
    );
    assert_eq!(
        event(&system(
            "Overpower! You scorch Drudge Skulker for 80 points with Flame Bolt VI."
        )),
        damage(ME, "Drudge Skulker", Magic, E::Fire, false, 80)
    );
    assert_eq!(
        event(&system(
            "Critical hit! Overpower! You smash Drudge Skulker for 147 points with Shock Wave Streak VII."
        )),
        damage(ME, "Drudge Skulker", Magic, E::Bludge, true, 147)
    );
    assert_eq!(
        event(&system(
            "Critical hit! Sneak Attack! You smash Drudge Skulker for 147 points with Shock Wave Streak VII."
        )),
        damage(ME, "Drudge Skulker", Magic, E::Bludge, true, 147)
    );
}

#[test]
fn outgoing_drain_counts() {
    // F3
    assert_eq!(
        f3_outgoing_drain("With Harm Other VI you drain 40 points of health from Drudge Skulker."),
        Some(("Drudge Skulker", 40))
    );
    assert_eq!(
        f3_outgoing_drain("With you drain 40 points of health from X."),
        None
    );
    assert_eq!(
        event(&system(
            "With Harm Other VI you drain 40 points of health from Drudge Skulker."
        )),
        damage(ME, "Drudge Skulker", Magic, E::Typeless, false, 40)
    );
    assert_eq!(
        event(&system(
            "With Enfeeble Other VI you drain 25 points of stamina from Drudge Skulker."
        )),
        damage(ME, "Drudge Skulker", Magic, E::Typeless, false, 25)
    );
}

#[test]
fn died_kill_line_counts_only_as_a_combat_line() {
    // F4
    assert_eq!(
        f4_killed_target(&out("Drudge Skulker died!")),
        Some("Drudge Skulker")
    );
    assert_eq!(f4_killed_target(&system("Drudge Skulker died!")), None);
    assert_eq!(
        event(&out("Drudge Skulker died!")),
        Some(CombatEvent::killing_blow(ME, "Drudge Skulker"))
    );
    assert_eq!(event(&system("Drudge Skulker died!")), None);
    let victim = ChatLine {
        combat_kind: 3,
        ..out("Drudge Skulker died!")
    };
    assert_eq!(event(&victim), None);
    assert_eq!(
        event(&out(
            "You send Horan to death so violently that even the lifestone flinches!"
        )),
        Some(CombatEvent::killing_blow(ME, "Horan"))
    );
    assert_eq!(event(&out(" died!")), None);
}

#[test]
fn cloak_weave_counts_as_a_surge() {
    // F5
    assert_eq!(
        f5_cloak_weave(
            ME,
            "The cloak of Acdream weaves the magic of Shroud of Darkness (Melee)!"
        ),
        Some(SurgeEvent::new(ME, ""))
    );
    assert_eq!(
        parse(
            ME,
            &system("The cloak of Acdream weaves the magic of Shroud of Darkness (Melee)!")
        )
        .cloak,
        Some(SurgeEvent::new(ME, ""))
    );
    assert_eq!(
        parse(
            ME,
            &system("The cloak of Acdream weaves the magic of Cloaked in Skill!")
        )
        .cloak,
        Some(SurgeEvent::new(ME, ME))
    );
    // Another player's cloak: no source, so not counted.
    assert_eq!(
        parse(
            ME,
            &system("The cloak of Horan weaves the magic of Shroud of Darkness (Magic)!")
        )
        .cloak,
        Some(SurgeEvent::new("", ""))
    );
    let mut tracker = CombatTracker::default();
    tracker.add_parsed(
        ME,
        &parse(
            ME,
            &system("The cloak of Horan weaves the magic of Shroud of Darkness (Magic)!"),
        ),
    );
    assert!(tracker.cloak_infos().is_empty());
}

#[test]
fn incoming_prefixes_are_not_part_of_the_name() {
    // F6
    assert_eq!(
        f6_strip_incoming_prefixes("Sneak Attack! Reckless! Drudge Skulker"),
        "Drudge Skulker"
    );
    assert_eq!(
        event(&combat_in(
            "Sneak Attack! Drudge Skulker hits your chest for 10 points of slashing damage!"
        )),
        damage("Drudge Skulker", ME, MeleeMissle, E::Slash, false, 10)
    );
    assert_eq!(
        event(&combat_in(
            "Critical hit! Sneak Attack! Olthoi Soldier stabs your head for 30 points of piercing damage!"
        )),
        damage("Olthoi Soldier", ME, MeleeMissle, E::Pierce, true, 30)
    );
    assert_eq!(
        event(&combat_in(
            "Reckless! Drudge Skulker hits your chest for 10 points of slashing damage!"
        )),
        damage("Drudge Skulker", ME, MeleeMissle, E::Slash, false, 10)
    );
    assert_eq!(
        event(&system(
            "Critical hit! Overpower! Sneak Attack! Drudge Skulker shocks you for 96 points with Lightning Arc VII."
        )),
        damage("Drudge Skulker", ME, Magic, E::Electric, true, 96)
    );
}

#[test]
fn posted_lines_are_forgotten_on_the_second_tick() {
    // F10's memory
    let mut posted = PostedLines::default();
    posted.note("a".to_owned());
    posted.note("a".to_owned());
    assert!(posted.take("a"));
    posted.tick();
    assert!(!posted.is_empty());
    posted.tick();
    assert!(posted.is_empty());
    assert!(!posted.take("a"));
}

// The trackers

/// A combat tracker with its patterns compiled, the character in the world.
fn ready() -> (Combat, FakeMagHost) {
    let mut host = FakeMagHost::new();
    let mut combat = Combat::default();
    while !combat.is_ready() {
        combat.tick(&mut host, CombatSettings::default());
    }
    (combat, host)
}

#[test]
fn our_own_unable_to_parse_line_is_ignored() {
    // F10
    let (mut combat, mut host) = ready();
    let me = host.meta.character.as_ref().unwrap().name.clone();
    let line = combat_in("Drudge Skulker hits your chest for 5 points of prismatic damage!");
    combat.on_chat(&mut host, &line);
    let posted = host.posted_text();
    // Mag-Tools' two trackers each wrote it.
    assert_eq!(posted.len(), 2, "{posted:?}");
    assert!(posted[0].starts_with("<{Mag-Tools}>: Unable to parse damage element from: "));
    let infos = combat.current.combat_infos().to_vec();
    // The client hands the plugin its own lines back.
    for text in &posted {
        combat.on_chat(&mut host, &system(text));
    }
    assert_eq!(host.posted_text().len(), 2);
    assert_eq!(combat.current.combat_infos(), infos.as_slice());
    assert_eq!(combat.current.combat_infos()[0].source, "Drudge Skulker");
    assert_eq!(combat.current.combat_infos()[0].target, me);
}

#[test]
fn both_trackers_count_every_line() {
    let (mut combat, mut host) = ready();
    let me = host.meta.character.as_ref().unwrap().name.clone();
    for text in [
        "You hit Drudge Skulker for 25 points of slashing damage!",
        "You hit Drudge Skulker for 25 points of slashing damage!",
        "Drudge Skulker evaded your attack.",
    ] {
        combat.on_chat(&mut host, &combat_line(text));
    }
    for tracker in [&combat.current, &combat.persistent] {
        let infos = tracker.combat_infos();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].source, me);
        let melee = infos[0].by_attack_type.get(MeleeMissle).unwrap();
        assert_eq!(
            melee.by_element.get(E::Slash),
            Some(&DamageByElement {
                total_attacks: 2,
                total_normal_damage: 50,
                max_normal_damage: 25,
                ..DamageByElement::default()
            })
        );
        assert_eq!(
            melee.by_element.get(E::Unknown),
            Some(&DamageByElement {
                total_attacks: 1,
                failed_attacks: 1,
                ..DamageByElement::default()
            })
        );
    }
}

fn combat_line(text: &str) -> ChatLine {
    out(text)
}

#[test]
fn lines_before_the_patterns_wait_for_them() {
    let mut host = FakeMagHost::new();
    let mut combat = Combat::default();
    combat.on_chat(
        &mut host,
        &out("You hit Drudge Skulker for 25 points of slashing damage!"),
    );
    assert!(combat.current.is_empty());
    while !combat.is_ready() {
        combat.tick(&mut host, CombatSettings::default());
    }
    combat.tick(&mut host, CombatSettings::default());
    assert_eq!(combat.current.combat_infos().len(), 1);
}

#[test]
fn only_events_we_take_part_in_are_kept() {
    let mut tracker = CombatTracker::default();
    tracker.add_combat(
        ME,
        &CombatEvent::damage("Horan", "Drudge", MeleeMissle, E::Slash, false, 5),
    );
    tracker.add_aetheria(ME, &SurgeEvent::new("Horan", "Horan"));
    assert!(tracker.is_empty());
    tracker.add_aetheria(ME, &SurgeEvent::new(ME, ME));
    tracker.add_aetheria(ME, &SurgeEvent::new(ME, ME));
    assert_eq!(tracker.aetheria_infos()[0].total_surges, 2);
    assert_eq!(
        tracker.take_updates(),
        vec![
            TrackerUpdate::Aetheria {
                source: ME.to_owned(),
                target: ME.to_owned()
            };
            2
        ]
    );
}

#[test]
fn a_kill_and_a_crit_land_in_their_buckets() {
    let mut info = CombatInfo::new(ME, "Drudge");
    info.add(&CombatEvent::killing_blow(ME, "Drudge"));
    info.add(&CombatEvent::damage(ME, "Drudge", Magic, E::Fire, true, 90));
    info.add(&CombatEvent::damage(ME, "Drudge", Magic, E::Fire, true, 60));
    assert_eq!(info.killing_blows, 1);
    let kill = info
        .by_attack_type
        .get(AttackType::Unknown)
        .and_then(|t| t.by_element.get(E::Unknown));
    assert_eq!(kill.map(|b| b.total_attacks), Some(1));
    let fire = info
        .by_attack_type
        .get(Magic)
        .and_then(|t| t.by_element.get(E::Fire))
        .copied()
        .unwrap();
    assert_eq!(
        (
            fire.total_attacks,
            fire.crits,
            fire.total_crit_damage,
            fire.max_crit_damage
        ),
        (2, 2, 150, 90)
    );
    assert_eq!(info.totals().total_attacks, 3);
    assert_eq!(info.damage(), 150);
    // Insertion order: Unknown first.
    let order: Vec<AttackType> = info.by_attack_type.iter().map(|(k, _)| k).collect();
    assert_eq!(order, vec![AttackType::Unknown, Magic]);
}

#[test]
fn clearing_tells_the_lists() {
    let (mut combat, mut host) = ready();
    combat.on_chat(&mut host, &out("You obliterate Drudge Skulker!"));
    combat.current.take_updates();
    combat.current.clear();
    assert!(combat.current.is_empty());
    assert!(!combat.persistent.is_empty());
    assert_eq!(combat.current.take_updates(), vec![TrackerUpdate::Cleared]);
}

#[test]
fn the_end_anchor_written_once_matches_what_mag_tools_pattern_matches() {
    use openac_plugin_compat::regex::{DotNetRegex, RegexOptions};
    // Every line of these tests, as is, with a final line break, with a
    // line break inside, and with text after it.
    let source = include_str!("combat_parse.rs");
    let mut lines: Vec<String> = Vec::new();
    for quoted in source.split('"').skip(1).step_by(2) {
        let text = quoted.replace("\\\\", "\\");
        lines.push(text.clone());
        lines.push(format!("{text}\n"));
        lines.push(format!("{text}\n\n"));
        lines.push(format!("{text}\nmore"));
    }
    lines.push("You obliterate Drudge Skulker!\n".to_owned());
    lines.push("<Tell:IIDString:1:Horan>Horan<\\Tell> says, \"hi\"\n".to_owned());
    lines.push("Town Crier says, \"hi\"\n".to_owned());
    lines.push("You cast Gossamer Flesh on Chicken\n".to_owned());
    let mut compared = 0;
    for pattern in patterns::all_patterns() {
        let original = DotNetRegex::new(pattern, RegexOptions::NONE).unwrap();
        let rewritten =
            DotNetRegex::new(&patterns::single_end_anchor(pattern), RegexOptions::NONE).unwrap();
        assert_ne!(original.pattern(), rewritten.pattern(), "{pattern}");
        for line in &lines {
            let a = original.find(line).unwrap();
            let b = rewritten.find(line).unwrap();
            assert_eq!(a.is_some(), b.is_some(), "{pattern} on {line:?}");
            if let (Some(a), Some(b)) = (a, b) {
                compared += 1;
                for group in ["targetname", "points", "name", "msg", "channel"] {
                    assert_eq!(a.group(group), b.group(group), "{pattern} on {line:?}");
                }
            }
        }
    }
    assert!(compared > 150, "{compared}");
}

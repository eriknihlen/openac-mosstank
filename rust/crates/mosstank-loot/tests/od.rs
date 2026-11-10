//! Weapon OD by the loot profile's `(OD +N)` rules, read from profiles in
//! the shape of a real one (`(M) Xbow (OD +10)`: mastery, missile class,
//! buffed missile damage, wield skill).

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::{ItemProperties, ObjectClass, WeaponProfile};
use mosstank_formats::utl::{self, LootRule};
use mosstank_loot::od::{OdRules, od_step};

/// One requirement as a version-1 profile writes it: its length (the
/// payload's characters, line breaks counted), then its lines.
fn requirement(lines: &[&str]) -> String {
    let payload: String = lines.iter().map(|line| format!("{line}\r\n")).collect();
    format!("{}\r\n{payload}", payload.len())
}

/// A rule: its name, an empty expression, `priority;action;types`, then
/// each requirement.
fn rule(name: &str, requirements: &[(i32, Vec<String>)]) -> String {
    let kinds: String = requirements
        .iter()
        .map(|(kind, _)| format!(";{kind}"))
        .collect();
    let mut text = format!("{name}\r\n\r\n0;1{kinds}\r\n");
    for (_, lines) in requirements {
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        text.push_str(&requirement(&lines));
    }
    text
}

fn line(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

/// A ladder's step: mastery, class, a damage requirement at `damage`,
/// wield skill.
fn step(name: &str, mastery: i32, class: i32, damage_kind: i32, damage: i32, skill: i32) -> String {
    rule(
        name,
        &[
            (12, line(&[&mastery.to_string(), "353"])),
            (7, line(&[&class.to_string()])),
            (damage_kind, line(&[&damage.to_string()])),
            (12, line(&[&skill.to_string(), "159"])),
        ],
    )
}

/// The crossbow ladder: `(M) Xbow (OD +N)` asks 77 + N buffed missile
/// damage (87 for +10, 78 for +1), highest first as the profile lists it.
fn xbow_ladder() -> Vec<String> {
    (1..=10)
        .rev()
        .map(|n| step(&format!("(M) Xbow (OD +{n})"), 9, 9, 2001, 77 + n, 47))
        .collect()
}

/// A heavy axe ladder: `(H) Axe (OD +N)` asks 30 + N buffed median damage.
fn axe_ladder() -> Vec<String> {
    (1..=10)
        .rev()
        .map(|n| step(&format!("(H) Axe (OD +{n})"), 3, 1, 2000, 30 + n, 44))
        .collect()
}

fn profile(rules: &[String]) -> Vec<LootRule> {
    let text = format!("UTL\r\n1\r\n{}\r\n{}", rules.len(), rules.concat());
    utl::try_read(Some(&text)).expect("the profile reads").rules
}

fn weapon(class: ObjectClass, damage: i32, variance: f64) -> (InventoryItem, WeaponProfile) {
    let item = InventoryItem {
        object_id: 1,
        name: "Weapon".to_owned(),
        object_class: class,
        damage,
        damage_variance: variance,
        ..InventoryItem::default()
    };
    let profile = WeaponProfile {
        damage,
        damage_variance: variance,
        damage_mod: 1.0,
        ..WeaponProfile::default()
    };
    (item, profile)
}

/// A crossbow (mastery 9, missile skill) whose buffed missile damage is
/// `damage` (its elemental bonus; no damage modifier).
fn crossbow(damage: i32) -> (InventoryItem, ItemProperties) {
    bow_of_mastery(9, damage)
}

fn bow_of_mastery(mastery: i32, damage: i32) -> (InventoryItem, ItemProperties) {
    let (item, profile) = weapon(ObjectClass::MissileWeapon, 0, 0.0);
    let properties = ItemProperties {
        ints: vec![(353, mastery), (159, 47), (204, damage)],
        weapon: Some(profile),
        ..ItemProperties::default()
    };
    (item, properties)
}

fn od(rules: &[LootRule], (item, properties): &(InventoryItem, ItemProperties)) -> Option<i32> {
    OdRules::new(rules).od(item, properties, &mut |_| None)
}

#[test]
fn a_rules_name_gives_its_step() {
    assert_eq!(od_step("(M) Xbow (OD +10)"), Some(10));
    assert_eq!(od_step("(2H) Cleaving (OD +3)"), Some(3));
    assert_eq!(od_step("(OD +x) then (OD +4)"), Some(4));
    for name in [
        "(M) Xbow (Max 385)",
        "(M) Xbow (OD +)",
        "(M) Xbow (OD +x)",
        "(M) Xbow (OD +10",
        "(M) Xbow (OD 10)",
        "(M) Xbow (OD +99999999999)",
    ] {
        assert_eq!(od_step(name), None, "{name}");
    }
}

#[test]
fn a_crossbows_od_is_the_highest_step_it_reaches() {
    let rules = profile(&xbow_ladder());
    assert_eq!(OdRules::new(&rules).len(), 10);
    assert_eq!(od(&rules, &crossbow(89)), Some(10));
    assert_eq!(od(&rules, &crossbow(87)), Some(10));
    assert_eq!(od(&rules, &crossbow(86)), Some(9));
    assert_eq!(od(&rules, &crossbow(80)), Some(3));
    assert_eq!(od(&rules, &crossbow(78)), Some(1));
    assert_eq!(od(&rules, &crossbow(77)), None);
}

#[test]
fn a_bow_has_no_od_by_the_crossbow_ladder() {
    let rules = profile(&xbow_ladder());
    assert_eq!(od(&rules, &bow_of_mastery(8, 89)), None);
}

#[test]
fn the_highest_matching_step_wins_whatever_the_profiles_order() {
    let mut ascending = xbow_ladder();
    ascending.reverse();
    let rules = profile(&ascending);
    assert_eq!(rules[0].name, "(M) Xbow (OD +1)");
    assert_eq!(od(&rules, &crossbow(89)), Some(10));
    assert_eq!(od(&rules, &crossbow(81)), Some(4));
}

#[test]
fn a_disabled_od_rule_is_not_counted() {
    // `(M) Xbow (OD +10)` disabled (requirement 9999 set to true, as a
    // profile writes a rule switched off), its requirements otherwise met.
    let mut ladder = xbow_ladder();
    ladder[0] = rule(
        "(M) Xbow (OD +10)",
        &[
            (12, line(&["9", "353"])),
            (7, line(&["9"])),
            (2001, line(&["87"])),
            (12, line(&["47", "159"])),
            (9999, line(&["true"])),
        ],
    );
    // A placeholder ladder step, only the switch.
    ladder.push(rule("(M) Xbow (OD +11)", &[(9999, line(&["true"]))]));
    let rules = profile(&ladder);
    assert_eq!(OdRules::new(&rules).len(), 9);
    assert_eq!(od(&rules, &crossbow(89)), Some(9));
}

#[test]
fn a_rule_without_a_step_is_not_an_od_rule() {
    // A rule matching every missile weapon, named like a ladder's but with
    // no step.
    let mut rules = xbow_ladder();
    rules.insert(
        0,
        rule(
            "(M) Xbow (Max 385)",
            &[(7, line(&["9"])), (12, line(&["9", "353"]))],
        ),
    );
    rules.insert(0, rule("(M) Xbow (OD +x)", &[(7, line(&["9"]))]));
    let rules = profile(&rules);
    assert_eq!(OdRules::new(&rules).len(), 10);
    assert_eq!(od(&rules, &crossbow(77)), None);
    assert_eq!(od(&rules, &crossbow(80)), Some(3));
}

#[test]
fn each_ladder_judges_its_own_weapons() {
    let mut both = xbow_ladder();
    both.extend(axe_ladder());
    let rules = profile(&both);
    // An axe whose median damage is 36 (40 maximum, variance 0.2): +6.
    let (axe, profile) = weapon(ObjectClass::MeleeWeapon, 40, 0.2);
    let properties = ItemProperties {
        ints: vec![(353, 3), (159, 44)],
        weapon: Some(profile),
        ..ItemProperties::default()
    };
    assert_eq!(od(&rules, &(axe.clone(), properties.clone())), Some(6));
    // The same numbers on a missile weapon: neither ladder.
    let bow = InventoryItem {
        object_class: ObjectClass::MissileWeapon,
        ..axe
    };
    assert_eq!(od(&rules, &(bow, properties)), None);
    assert_eq!(od(&rules, &crossbow(84)), Some(7));
}

#[test]
fn a_profile_without_od_rules_has_none() {
    let rules = profile(&[rule("Keep everything", &[(7, line(&["9"]))])]);
    assert!(OdRules::new(&rules).is_empty());
    assert_eq!(od(&rules, &crossbow(89)), None);
}

#[test]
fn the_fingerprint_follows_the_od_rules_only() {
    let mut text = xbow_ladder();
    text.push(rule("Keep everything", &[(7, line(&["9"]))]));
    let rules = profile(&text);
    let before = OdRules::fingerprint(&rules);
    assert_eq!(OdRules::fingerprint(&rules.clone()), before);
    // Another rule changed: the same.
    let mut other = rules.clone();
    other.last_mut().unwrap().name = "Keep nothing".to_owned();
    assert_eq!(OdRules::fingerprint(&other), before);
    // An OD rule's damage changed, or one renamed out of the ladder.
    let mut damage = rules.clone();
    damage[0].vtank_requirements[2].payload = "88\r\n".to_owned();
    assert_ne!(OdRules::fingerprint(&damage), before);
    let mut renamed = rules;
    renamed[3].name = "(M) Xbow".to_owned();
    assert_ne!(OdRules::fingerprint(&renamed), before);
}

#[test]
fn a_step_may_have_spaces_and_an_at_least_mark() {
    // As real profiles write them: a space before the `)`, and `>=` ahead
    // of the `+`.
    assert_eq!(od_step("(F) Spear (OD +10 )"), Some(10));
    assert_eq!(od_step("(H) Axe (OD >= +8)"), Some(8));
    assert_eq!(od_step("(H) Axe (OD>=+4)"), Some(4));
    assert_eq!(od_step("(H) Axe (OD  >=  +  1  )"), Some(1));
    assert_eq!(od_step("(H) Axe (OD+3)"), Some(3));
    for name in [
        "(H) Axe (OD >= +)",
        "(H) Axe (OD >= 8)",
        "(H) Axe (OD => +8)",
        "(H) Axe (OD >= +8 x)",
        "(H) Axe (OD + x)",
    ] {
        assert_eq!(od_step(name), None, "{name}");
    }
    // The spaced step is an OD rule like any other.
    let mut ladder = xbow_ladder();
    ladder[0] = step("(M) Xbow (OD +10 )", 9, 9, 2001, 87, 47);
    let rules = profile(&ladder);
    assert_eq!(od(&rules, &crossbow(89)), Some(10));
}

/// A crossbow carrying `spell` (appraised), and a spell table that knows
/// it as `name`.
fn spelled_crossbow(damage: i32, spell: u32) -> (InventoryItem, ItemProperties) {
    let (mut item, properties) = crossbow(damage);
    item.appraised_spell_ids = vec![spell];
    (item, properties)
}

#[test]
fn a_cantrip_gated_step_reads_the_spell_table() {
    // `(M) Xbow CS (OD +10)`: the ladder's step and a Legendary cantrip,
    // by spell name (requirement 0) or by count (requirement 9).
    let by_name = rule(
        "(M) Xbow CS (OD +10)",
        &[
            (7, line(&["9"])),
            (0, line(&["^Legendary Blood Thirst$"])),
            (2001, line(&["87"])),
        ],
    );
    let by_count = rule(
        "(M) Xbow Leg (OD +9)",
        &[
            (7, line(&["9"])),
            (9, line(&["^Legendary ", "Thirst", "1"])),
            (2001, line(&["86"])),
        ],
    );
    let rules = profile(&[by_name, by_count]);
    let od_rules = OdRules::new(&rules);
    let table = |name: &'static str| {
        move |id: u32| {
            (id == 6089).then(|| ac_plugin_abi::magic::SpellInfo {
                spell_id: id,
                name: name.to_owned(),
                ..ac_plugin_abi::magic::SpellInfo::default()
            })
        }
    };
    let (bow, properties) = spelled_crossbow(89, 6089);
    let mut thirst = table("Legendary Blood Thirst");
    assert_eq!(od_rules.od(&bow, &properties, &mut thirst), Some(10));
    // Another Legendary: only the count's step (Thirst excluded there).
    let mut defender = table("Legendary Defender");
    assert_eq!(od_rules.od(&bow, &properties, &mut defender), Some(9));
    // A spell the table does not know, or none on the item: no step.
    assert_eq!(od_rules.od(&bow, &properties, &mut |_| None), None);
    let (plain, properties) = crossbow(89);
    assert_eq!(od_rules.od(&plain, &properties, &mut thirst), None);
}

#[test]
fn only_what_decides_an_od_moves_the_fingerprint() {
    let rules = profile(&xbow_ladder());
    let before = OdRules::fingerprint(&rules);
    let built = OdRules::new(&rules);
    // The action, the priority, the name's other words: the same.
    let mut other = rules.clone();
    other[0].action = mosstank_formats::utl::LootAction::Salvage;
    other[1].priority = 7;
    other[2].keep_count = 5;
    other[3].name = "(M) Crossbow (OD +7)".to_owned();
    other[4].custom_expression = "anything".to_owned();
    assert_eq!(OdRules::fingerprint(&other), before);
    assert_eq!(OdRules::new(&other), built);
    // A step switched off, or its step changed: moved.
    let mut disabled = rules.clone();
    disabled[5]
        .vtank_requirements
        .push(mosstank_formats::utl::VtankLootRequirement {
            kind: 9999,
            payload: "true\r\n".to_owned(),
        });
    assert_ne!(OdRules::fingerprint(&disabled), before);
    assert_ne!(OdRules::new(&disabled), built);
    let mut stepped = rules;
    stepped[6].name = "(M) Xbow (OD +11)".to_owned();
    assert_ne!(OdRules::fingerprint(&stepped), before);
    assert_ne!(OdRules::new(&stepped), built);
}

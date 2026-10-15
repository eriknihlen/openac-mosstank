//! A buff decided with the gear it is cast in: a weapon whose own spells
//! raise the casting skill must not make a buff due that the caster, once
//! wielded, no longer reaches (register BUFF-1; a live character swapped
//! its orb and its scythe for minutes, never casting and never fighting).

mod common;

use ac_plugin_abi::magic::{SpellInfo, TrackedEnchantment};
use common::World;
use common::automation::{
    FakeAutomation, ITEM_ENCHANTMENT_SCHOOL, combat_target, equipment_item, named_spell, skill,
};
use mosstank::MossTank;
use mosstank_expr::ExpressionValue;

const CHARACTER: u32 = 0x5000_0001;
const WAND: u32 = 10;
const AXE: u32 = 20;
const BROGARDS_DEFIANCE: u32 = 2108;

/// An armor bane of the Impenetrability family at a tier.
fn impenetrability(spell_id: u32, name: &str, tier: i32, difficulty: i32) -> SpellInfo {
    let mut spell = named_spell(spell_id, 160, name, ITEM_ENCHANTMENT_SCHOOL);
    spell.tier = tier;
    spell.difficulty = difficulty;
    spell.description = "Increases a shield or piece of armor's resistance by 10%. \
                         Target yourself to cast this spell on all of your equipped armor."
        .to_owned();
    spell
}

/// Item Enchantment at 381, 426 while the axe is wielded (its own spells
/// add 45): with a 20-point margin the wand reaches tier 7 (300) and the
/// axe tier 8 (400). The character's armor holds the tier 7 for
/// `tier_seven_seconds`; a monster to fight; the wand and the axe on the
/// Items page; nothing wielded at the start.
fn open(tier_seven_seconds: f64) -> (World, MossTank) {
    let mut automation = FakeAutomation::combat_capable();
    automation.object_id = CHARACTER;
    automation.health = (100, 100);
    automation.stamina = (100, 100);
    automation.mana = (100, 100);
    automation.skills = vec![skill(ITEM_ENCHANTMENT_SCHOOL, "Item Enchantment", 381)];
    automation.wield_skill_bonus = Some((AXE, ITEM_ENCHANTMENT_SCHOOL, 45));
    automation.set_known_self_buffs(vec![
        impenetrability(51, "Impenetrability I", 1, 1),
        impenetrability(BROGARDS_DEFIANCE, "Brogard's Defiance", 7, 300),
        impenetrability(4407, "Incantation of Impenetrability", 8, 400),
    ]);
    automation.item_enchantments.insert(
        CHARACTER,
        vec![TrackedEnchantment {
            target_id: CHARACTER,
            spell_id: BROGARDS_DEFIANCE,
            family: 160,
            quality: 300,
            is_untargeted: false,
            seconds_remaining: tier_seven_seconds,
        }],
    );
    automation.item_entries = vec![
        common::item(WAND, "War Wand", 0x0000_8000),
        common::item(AXE, "Battle Axe", 1),
    ];
    automation.equipment = vec![
        equipment_item(WAND, "War Wand", 0x0000_8000),
        equipment_item(AXE, "Battle Axe", 1),
    ];
    automation.hostile_targets = vec![combat_target(30, "Drudge", 700, 2.0)];
    let mut world = World::with_automation(automation);
    let mut plugin = world.enable();
    for object_id in [WAND, AXE] {
        world.select_object(object_id);
        world.act(&mut plugin, "AddSelectedItem");
    }
    world.act_int(&mut plugin, "CycleMonsterWeaponAt", 0);
    world
        .with_tank(&mut plugin, |tank, client| {
            tank.set_meta_option(
                client,
                "SpellDiffExcessThreshold-Buff",
                &ExpressionValue::Number(20.0),
            )
        })
        .unwrap();
    (world, plugin)
}

fn equips_of(log: &[String], object_id: u32) -> usize {
    let entry = format!("Equip:{object_id:08X}");
    log.iter().filter(|line| **line == entry).count()
}

// The tier 7 on the armor covers what the wand reaches. Held, the axe lifts
// the skill to tier 8, which the tier 7 does not cover; but the wand that
// would cast it brings the skill back to tier 7, covered. Deciding with the
// axe's skill took the wand out, found nothing due, gave the axe back, and
// so on for good: never a cast, never a swing. Once the wand has shown what
// it reaches, the axe's bonus no longer makes the buff due.
#[test]
fn a_buff_due_only_through_the_weapons_own_bonus_does_not_pull_the_caster_out_again() {
    let (mut world, mut plugin) = open(3000.0);
    world.act(&mut plugin, "ToggleCombat");

    for _ in 0..80 {
        world.frame(&mut plugin, 0.5);
    }

    let log = world.automation().call_log.clone();
    let joined = log.join(" | ");
    assert!(world.cast_spell_ids().is_empty(), "{joined}");
    assert!(
        world.automation().begin_count > 0,
        "never attacked: {joined}"
    );
    assert_eq!(equips_of(&log, WAND), 1, "{joined}");
    assert_eq!(equips_of(&log, AXE), 2, "{joined}");
}

// What the wand reaches still decides: the tier 7 running under the rebuff
// window is due with the axe in hand too, and the wand comes out for it.
#[test]
fn a_buff_the_caster_reaches_still_takes_the_caster_out() {
    let (mut world, mut plugin) = open(100.0);
    world.act(&mut plugin, "ToggleCombat");

    for _ in 0..20 {
        world.frame(&mut plugin, 0.5);
    }

    let log = world.automation().call_log.clone();
    let joined = log.join(" | ");
    let equip_wand = log.iter().position(|line| line == "Equip:0000000A");
    let cast = log
        .iter()
        .position(|line| *line == format!("Cast:{BROGARDS_DEFIANCE}"));
    assert!(equip_wand.is_some(), "{joined}");
    assert!(cast > equip_wand, "{joined}");
}

//! UtilityBelt's ItemInfo through the shell: the selection event reaches
//! the tool with what made it, and the rule the line names is the one the
//! looter's own decision names.

mod common;

use ac_plugin_abi::calls;
use ac_plugin_abi::items::{ClassifierInfo, LootAction as PublicLootAction, LootClassification};
use ac_plugin_abi::world::{ObjectClass, SelectionChange, WorldObject, selection_source};
use common::{World, item};
use mosstank::MossTank;
use mosstank_formats::utl::{self, LootAction, LootRule};

const RING: u32 = 0x5000_0001;

/// A character carrying one appraised ring, ItemInfo switched on with
/// `switches`, the chat cleared.
fn ring_world(switches: &[&str]) -> (World, MossTank) {
    let mut world = World::with_character("Info");
    world.own_items(vec![item(RING, "Test Ring", 0x8)]);
    world.objects = Some(vec![WorldObject {
        object_id: RING,
        name: "Test Ring".to_owned(),
        object_class: ObjectClass::Jewelry,
        item_type: 0x8,
        container_id: 1,
        is_owned: true,
        has_appraisal_data: true,
        ..WorldObject::default()
    }]);
    world.set_reply::<calls::LootClassifiers>(&vec![ClassifierInfo {
        id: "utility/loot".to_owned(),
        display_name: "Utility Loot".to_owned(),
    }]);
    let mut plugin = world.enable();
    world.tick(&mut plugin);
    world.command(&mut plugin, "ub", "opt set ItemInfo.Enabled true");
    for switch in switches {
        world.command(&mut plugin, "ub", &format!("opt set {switch} true"));
    }
    world.clear_chat();
    (world, plugin)
}

/// Loads a loot profile of `rules` as `name`.
fn load_profile(world: &mut World, plugin: &mut MossTank, name: &str, rules: &[LootRule]) {
    world.profiles.insert(
        &format!("mosstank/loot/{name}.utl"),
        &utl::serialize_rules(rules),
    );
    world.vt(plugin, &format!("loot load {name}"));
    world.ticks(plugin, 5);
    assert_eq!(plugin.tank().unwrap().live_loot_rules().len(), rules.len());
    world.clear_chat();
}

fn keep(name: &str) -> LootRule {
    let mut rule = LootRule::named(name, "*");
    rule.action = LootAction::Keep;
    rule
}

fn select_by(world: &mut World, plugin: &mut MossTank, source: u8) {
    world.send(
        plugin,
        ac_plugin_abi::Event::SelectionChanged(SelectionChange {
            previous: None,
            selected: Some(RING),
            source,
        }),
    );
}

/// The ItemInfo lines posted.
fn info_lines(world: &World) -> Vec<String> {
    world
        .chat()
        .into_iter()
        .filter(|line| line.starts_with("[UB] ItemInfo: "))
        .collect()
}

/// A profile keeping everything under `Rings`, the looter set to ask the
/// external classifier instead, which answers a match under `rule_name`.
fn classifier_answers(world: &mut World, plugin: &mut MossTank, rule_name: &str) {
    load_profile(world, plugin, "Rings", &[keep("Rings")]);
    world.act_text(
        plugin,
        "SelectLootClassifier",
        "Utility Loot [utility/loot]",
    );
    world.set_reply::<calls::LootClassify>(&Some(LootClassification {
        matched: true,
        action: PublicLootAction::Keep,
        rule_name: rule_name.to_owned(),
        priority: 1,
        keep_count: 0,
    }));
    world.clear_chat();
}

#[test]
fn the_rule_named_is_the_one_the_looters_decision_names() {
    // The looter asks the external classifier it is set to, not the
    // profile's own rules; the line names what the looter would.
    let (mut world, mut plugin) = ring_world(&["ItemInfo.DescribeOnSelect"]);
    classifier_answers(&mut world, &mut plugin, "Their Rule");

    select_by(&mut world, &mut plugin, selection_source::KEYBOARD);

    let lines = info_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("Their Rule - Test Ring"), "{}", lines[0]);
}

#[test]
fn either_click_switch_describes_what_the_mouse_selects_and_nothing_else() {
    for switch in [
        "ItemInfo.DescribeOnLeftClick",
        "ItemInfo.DescribeOnRightClick",
    ] {
        let (mut world, mut plugin) = ring_world(&[switch]);
        for source in [
            selection_source::KEYBOARD,
            selection_source::SYSTEM,
            selection_source::PLUGIN,
        ] {
            select_by(&mut world, &mut plugin, source);
        }
        assert!(
            info_lines(&world).is_empty(),
            "{switch}: {:?}",
            world.chat()
        );

        select_by(&mut world, &mut plugin, selection_source::MOUSE);
        assert_eq!(info_lines(&world).len(), 1, "{switch}: {:?}", world.chat());
    }
}

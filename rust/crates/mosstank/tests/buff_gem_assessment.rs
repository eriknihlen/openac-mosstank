//! A looted gem the profile's GemFoodItems names, never appraised, through
//! the whole plugin: it is used when its buff is missing, and while its
//! buff is on its appraisal is asked for once.

mod common;

use ac_plugin_abi::character::ActiveEnchantment;
use ac_plugin_abi::items::InventoryItem;
use common::World;
use common::automation::FakeAutomation;
use mosstank::MossTank;
use mosstank_formats::usd::{VtankCell, VtankRow};

const GEM: u32 = 101;
const GEM_NAME: &str = "Black Luster Pearl";
const GEM_SPELL: u32 = 3811;
const GEM_FAMILY: u32 = 519;

fn tick(world: &mut World, plugin: &mut MossTank, seconds: f64) {
    world.with_tank(plugin, |tank, client| tank.on_tick(client, seconds));
}

fn run(world: &mut World, plugin: &mut MossTank, count: usize) {
    world.act(plugin, "ToggleCombat");
    for _ in 0..count {
        tick(world, plugin, 0.3);
    }
}

fn gem() -> InventoryItem {
    common::item(GEM, GEM_NAME, 0x800)
}

/// The defaults with the gem in GemFoodItems.
fn settings_with_the_gem() -> String {
    let mut database = mosstank_settings::defaults::create();
    let table = database.find_mut("GemFoodItems").unwrap();
    let name = table.column_index("Name").unwrap();
    let spell = table.column_index("Spell").unwrap();
    table.rows.clear();
    let mut row = VtankRow {
        cells: vec![VtankCell::int(0); table.column_names.len()],
    };
    row.cells[name] = VtankCell::string(GEM_NAME);
    row.cells[spell] = VtankCell::int(i32::try_from(GEM_SPELL).unwrap());
    table.rows.push(row);
    database.render()
}

/// Full vitals, the gem's spell in the table, the gem carried and never
/// appraised, and the profile loaded.
fn world_with_an_unappraised_gem(buff_on: bool) -> (World, MossTank) {
    let mut automation = FakeAutomation::with_full_vitals();
    automation.set_known_self_buffs(vec![common::spell(
        GEM_SPELL,
        GEM_FAMILY,
        "A configured effect.",
    )]);
    automation.item_entries = vec![gem()];
    automation.unassessed.insert(GEM);
    if buff_on {
        automation.active_enchantments.push(ActiveEnchantment {
            spell_id: GEM_SPELL,
            family: GEM_FAMILY,
            tier: 1,
            seconds_remaining: 3000.0,
        });
    }
    let mut world = World::with_automation(automation);
    world
        .profiles
        .insert("mosstank/profiles/GemFood.usd", &settings_with_the_gem());
    let mut plugin = world.enable();
    world.vt(&mut plugin, "settings load GemFood");
    (world, plugin)
}

#[test]
fn an_unappraised_configured_gem_is_used_when_its_buff_is_missing() {
    let (mut world, mut plugin) = world_with_an_unappraised_gem(false);

    run(&mut world, &mut plugin, 8);

    assert_eq!(world.automation().used_item_ids, [GEM]);
}

#[test]
fn an_unappraised_configured_gem_whose_buff_is_on_is_appraised_once() {
    let (mut world, mut plugin) = world_with_an_unappraised_gem(true);

    run(&mut world, &mut plugin, 8);

    assert!(world.automation().used_item_ids.is_empty());
    assert_eq!(world.automation().identified, [GEM]);
}

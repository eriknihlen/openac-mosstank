//! The loot rule engine against the C# plugin, run as a black box: the
//! cases in `tests/golden/cases.txt` (real `.utl` fixtures and MossTank's
//! own expressions, against the items described there), and what the C#
//! gave for them in `tests/golden/cases.out`, written by
//! `RustPortLootGolden.cs` in the C# tests (`MOSSTANK_RUST_GOLDEN=1`).
//! Outputs, not code, are compared.

use std::fmt::Write as _;

use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::world::{ArmorProfile, ItemProperties, ObjectClass, WeaponProfile};
use mosstank_formats::fixtures;
use mosstank_formats::utl;
use mosstank_loot::expression::LootRuleExpression;
use mosstank_loot::rules::{self, decide, needs_identify};

fn golden(name: &str) -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn uint(text: &str) -> u32 {
    match text.strip_prefix("0x") {
        Some(hex) => u32::from_str_radix(hex, 16).expect("hex"),
        None => text.parse().expect("uint"),
    }
}

#[allow(clippy::cast_possible_wrap)]
fn int(text: &str) -> i32 {
    match text.strip_prefix("0x") {
        Some(hex) => u32::from_str_radix(hex, 16).expect("hex") as i32,
        None => text.parse().expect("int"),
    }
}

fn double(text: &str) -> f64 {
    text.parse().expect("double")
}

fn float(text: &str) -> f32 {
    text.parse().expect("float")
}

/// An item as the case file describes it (`RustPortLootGolden.Item`).
fn item(spec: &str) -> (InventoryItem, ItemProperties) {
    let mut item = InventoryItem {
        stack_size: 1,
        ..InventoryItem::default()
    };
    let mut properties = ItemProperties::default();
    for part in spec.split(';') {
        let open = part.find('(');
        let equals = part.find('=');
        if let Some(open) = open
            && open > 0
            && part.ends_with(')')
            && equals.is_none_or(|equals| open < equals)
        {
            let key = &part[..open];
            let values: Vec<&str> = part[open + 1..part.len() - 1].split(',').collect();
            let pairs = || {
                values
                    .iter()
                    .map(|pair| pair.split_once(':').expect("a pair"))
            };
            match key {
                "spells" => item.appraised_spell_ids = values.iter().map(|v| uint(v)).collect(),
                "gear" => {
                    item.gear_damage = int(values[0]);
                    item.gear_damage_resistance = int(values[1]);
                    item.gear_critical_chance = int(values[2]);
                    item.gear_critical_resistance = int(values[3]);
                    item.gear_critical_damage = int(values[4]);
                    item.gear_critical_damage_resistance = int(values[5]);
                }
                "ints" => properties.ints = pairs().map(|(k, v)| (uint(k), int(v))).collect(),
                "floats" => {
                    properties.floats = pairs().map(|(k, v)| (uint(k), double(v))).collect()
                }
                "strings" => {
                    properties.strings = pairs().map(|(k, v)| (uint(k), v.to_owned())).collect();
                }
                "weapon" => {
                    properties.weapon = Some(WeaponProfile {
                        damage_type: 0,
                        weapon_time: int(values[0]),
                        weapon_skill: uint(values[1]),
                        damage: int(values[2]),
                        damage_variance: double(values[3]),
                        damage_mod: double(values[4]),
                        weapon_length: double(values[5]),
                        max_velocity: double(values[6]),
                        weapon_offense: double(values[7]),
                        max_velocity_estimated: 0,
                    });
                }
                "armor" => {
                    properties.armor = Some(ArmorProfile {
                        armor_level: int(values[0]),
                        slash_mod: float(values[1]),
                        pierce_mod: float(values[2]),
                        bludgeon_mod: float(values[3]),
                        cold_mod: float(values[4]),
                        fire_mod: float(values[5]),
                        acid_mod: float(values[6]),
                        nether_mod: float(values[7]),
                        electric_mod: float(values[8]),
                    });
                }
                _ => panic!("unknown item list {key}"),
            }
            continue;
        }
        let (field, value) = part.split_once('=').expect("a field");
        match field {
            "id" => item.object_id = uint(value),
            "wcid" => item.weenie_class_id = uint(value),
            "name" => value.clone_into(&mut item.name),
            "class" => {
                item.object_class = u8::try_from(int(value))
                    .ok()
                    .and_then(ObjectClass::from_tag)
                    .expect("a known object class");
            }
            "itemtype" => item.item_type = uint(value),
            "value" => item.value = int(value),
            "burden" => item.burden = int(value),
            "workmanship" => item.workmanship = float(value),
            "material" => item.material_type = uint(value),
            "damage" => item.damage = int(value),
            "variance" => item.damage_variance = double(value),
            "effects" => item.effects = uint(value),
            "underlay" => item.icon_underlay_id = uint(value),
            "overlay" => item.icon_overlay_id = uint(value),
            "icon" => item.icon_id = uint(value),
            "coverage" => item.coverage_mask = uint(value),
            "stack" => item.stack_size = int(value),
            "maxstack" => item.maximum_stack_size = int(value),
            "equipped" => item.equipped_location = uint(value),
            "wielder" => item.wielder_id = uint(value),
            "container" => item.container_id = uint(value),
            "structure" => item.structure = int(value),
            "maxstructure" => item.maximum_structure = int(value),
            "mana" => item.item_current_mana = int(value),
            "maxmana" => item.item_maximum_mana = int(value),
            "tinkered" => item.num_times_tinkered = int(value),
            "combatuse" => item.combat_use = int(value),
            "weaponskill" => item.weapon_skill = int(value),
            "damagetype" => item.damage_type = int(value),
            "ammo" => item.ammo_type = uint(value),
            "target" => item.target_type = uint(value),
            "publicflags" => item.public_flags = uint(value),
            "useability" => item.useability = uint(value),
            "valid" => item.valid_locations = uint(value),
            "plural" => value.clone_into(&mut item.plural_name),
            "radius" => item.use_radius = float(value),
            "slot" => item.container_slot = int(value),
            _ => panic!("unknown item field {field}"),
        }
    }
    (item, properties)
}

type Items = Vec<(String, InventoryItem, ItemProperties)>;

fn profile(out: &mut String, text: &str, items: &Items) {
    let profile = match utl::try_read(Some(text)) {
        Ok(profile) => profile,
        Err(error) => {
            writeln!(out, "!! {error}").unwrap();
            return;
        }
    };
    let rules_list = &profile.rules;
    for (name, item, properties) in items {
        write!(out, "{name}: ").unwrap();
        let (decision, refusal) = decide(item, properties, rules_list, &[], None, None);
        match decision {
            Some(found) => write!(
                out,
                "decision={}|{}|{:?}",
                found.rule_index, found.rule_name, found.action
            )
            .unwrap(),
            None => write!(out, "refused={}", refusal.describe()).unwrap(),
        }
        write!(
            out,
            "; needs={}; rules=",
            needs_identify(item, properties, rules_list, None)
        )
        .unwrap();
        for (index, rule) in rules_list.iter().enumerate() {
            let (decided, early) = rules::rule_early_match(rule, item, properties, None);
            let (matched, failure) = rules::rule_matches(rule, item, properties, None);
            if index > 0 {
                out.push(' ');
            }
            out.push(if decided { 'd' } else { 'u' });
            out.push(if early { 'm' } else { '-' });
            out.push(if matched { 'M' } else { '.' });
            out.push(if failure.is_none() { '_' } else { '!' });
        }
        out.push('\n');
    }
}

fn expression(out: &mut String, source: &str, items: &Items) {
    let Ok(compiled) = LootRuleExpression::compile(source) else {
        out.push_str("!! compile\n");
        return;
    };
    for (name, item, properties) in items {
        let verdict = match compiled.is_match(item, properties) {
            Ok(true) => "true",
            Ok(false) => "false",
            Err(_) => "error",
        };
        writeln!(out, "{name}: {verdict}").unwrap();
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/RustPortLootGolden.cs :: WriteLootGoldenData (the cases.out it writes is the expected one)
#[test]
fn the_rule_engine_answers_as_the_csharp_plugin_does() {
    let mut items: Items = Vec::new();
    let mut out = String::new();
    let cases = golden("cases.txt");
    for line in cases.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(3, " ::: ");
        let kind = parts.next().expect("a kind");
        let name = parts.next().expect("a name");
        let field = parts.next().unwrap_or("");
        match kind {
            "item" => {
                let (item, properties) = item(field);
                items.push((name.to_owned(), item, properties));
            }
            "profile" => {
                writeln!(out, "== profile {name}").unwrap();
                profile(&mut out, &fixtures::read(&field[1..]), &items);
            }
            "expr" => {
                writeln!(out, "== expr {name}").unwrap();
                expression(&mut out, field, &items);
            }
            _ => panic!("unknown case kind {kind}"),
        }
    }
    let expected = golden("cases.out").replace("\r\n", "\n");
    for (number, (actual, wanted)) in out.lines().zip(expected.lines()).enumerate() {
        assert_eq!(actual, wanted, "cases.out line {}", number + 1);
    }
    assert_eq!(out.lines().count(), expected.lines().count());
}

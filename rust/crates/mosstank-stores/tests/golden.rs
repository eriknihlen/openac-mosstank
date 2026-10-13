//! The settings profile store against the C# store's own results: each
//! scenario in `golden/cases.txt` runs here and in the C# plugin
//! (`RustPortStoresGolden.cs` in the C# tests, run by `RustPortGolden`),
//! and every file both leave behind, every log line and every answer must
//! match `golden/cases.out`, which the C# wrote. Settings documents (`.usd`)
//! are compared by length and FNV-1a hash; everything else in full. On a
//! mismatch the Rust output is written to `target/golden/stores.rust.out`.
//!
//! Source: the C# `MossTankProfileStore` (with its side-car, legacy and
//! preferences documents) as a black box over the scenarios.

use std::fmt::Write as _;
use std::path::PathBuf;

use mosstank_dotnet::collections::{IgnoreCaseStringSet, OrdinalStringSet};
use mosstank_formats::route::ClientPathing;
use mosstank_formats::utl::{LootAction, LootRule};
use mosstank_settings::model::{AllSettings, BuffItemEnchantRow, ConsumableCategory};
use mosstank_settings::monster::{MonsterRule, MonsterRuleActions, MonsterValue, MonsterValueKind};
use mosstank_stores::profile_store::{ProfileLoad, ProfileStore};
use mosstank_stores::recovery::sha256_hex;
use mosstank_stores::storage::{MemoryHost, MemoryStorage};
use openac_plugin_compat::number::{to_string_f32, to_string_f64};
use openac_plugin_compat::text::ordinal;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn show(text: &str) -> String {
    text.replace('\r', "\\r")
        .replace('\n', "\\n\n")
        .replace('\t', "\\t")
}

fn fnv64(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("fnv64={hash:016x} bytes={}\n", text.len())
}

fn bool_text(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

fn load_text(load: ProfileLoad) -> &'static str {
    match load {
        ProfileLoad::Loaded => "Loaded",
        ProfileLoad::Partial => "Partial",
        ProfileLoad::Missing => "Missing",
        ProfileLoad::Failed => "Failed",
    }
}

fn storage_dump(out: &mut String, root: &str, storage: &MemoryStorage) {
    let mut keys: Vec<&str> = storage.keys().collect();
    keys.sort_by(|a, b| ordinal::compare(a, b));
    for key in keys {
        let text = storage.get(key).unwrap_or_default();
        writeln!(out, "--- {root} {key}").unwrap();
        if ordinal::ends_with_ignore_case(key, ".usd") {
            out.push_str(&fnv64(text));
        } else {
            out.push_str(&show(text));
            out.push('\n');
        }
    }
}

fn files(out: &mut String, host: &MemoryHost) {
    storage_dump(out, "plugin", &host.plugin);
    storage_dump(out, "profiles", &host.profiles);
    out.push_str("--- log\n");
    for line in &host.log.lines {
        out.push_str(line);
        out.push('\n');
    }
}

struct Live {
    settings: AllSettings,
    no_buff: OrdinalStringSet,
    channels: IgnoreCaseStringSet,
}

impl Live {
    fn new() -> Self {
        Self {
            settings: AllSettings::default(),
            no_buff: OrdinalStringSet::new(),
            channels: IgnoreCaseStringSet::new(),
        }
    }

    fn load(&mut self, store: &mut ProfileStore, host: &mut MemoryHost) -> ProfileLoad {
        store
            .load_current(
                host,
                &mut self.settings,
                &mut self.no_buff,
                &mut self.channels,
            )
            .unwrap()
    }

    fn save(&mut self, store: &mut ProfileStore, host: &mut MemoryHost) -> bool {
        store
            .save_current(host, &mut self.settings, &self.no_buff, &self.channels)
            .unwrap()
    }
}

fn host() -> MemoryHost {
    let mut host = MemoryHost::on("Coldeve");
    host.plugin = MemoryStorage::ordinal();
    host.profiles = MemoryStorage::ordinal();
    host
}

fn joined<'a>(items: impl IntoIterator<Item = &'a String>) -> String {
    items
        .into_iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("|")
}

fn summary(out: &mut String, live: &Live) {
    let s = &live.settings;
    let c = &s.combat;
    writeln!(out, "items {}", joined(c.combat_item_names.iter())).unwrap();
    writeln!(out, "order {}", joined(c.combat_item_order.iter())).unwrap();
    let by_id: Vec<String> = c
        .combat_item_names_by_id
        .iter()
        .map(|(id, name)| format!("{id}={name}"))
        .collect();
    writeln!(out, "byid {}", by_id.join(";")).unwrap();
    writeln!(out, "consumables {}", joined(c.consumable_names.iter())).unwrap();
    let categories: Vec<String> = c
        .consumable_categories
        .iter()
        .map(|(name, category)| format!("{name}={}", *category as i32))
        .collect();
    writeln!(out, "categories {}", categories.join(";")).unwrap();
    writeln!(out, "nobuff {}", joined(live.no_buff.iter())).unwrap();
    writeln!(out, "logs {}", joined(live.channels.iter())).unwrap();
    writeln!(out, "power {}", to_string_f32(c.attack_power)).unwrap();
    writeln!(out, "meta {}", c.meta_state).unwrap();
    let dynamic: Vec<String> = c
        .dynamic_settings
        .iter()
        .map(|(name, value)| {
            let kind = match value.kind {
                MonsterValueKind::Number => 0,
                MonsterValueKind::Text => 1,
                MonsterValueKind::Boolean => 2,
            };
            format!(
                "{name}={kind}:{}:{}:{}",
                to_string_f64(value.number),
                value.text,
                bool_text(value.boolean)
            )
        })
        .collect();
    writeln!(out, "dynamic {}", dynamic.join(";")).unwrap();
    let rules: Vec<String> = c
        .rules
        .iter()
        .map(|rule| {
            let a = rule.actions();
            format!(
                "{}[{}/{}/{}/{}]",
                rule.expression(),
                a.weapon_name,
                a.offhand_name,
                a.priority,
                a.flags.0
            )
        })
        .collect();
    writeln!(out, "rules {}", rules.join(";")).unwrap();
    writeln!(
        out,
        "extra {}",
        joined(s.buffs.extra_buff_spell_names.iter())
    )
    .unwrap();
    let enchant: Vec<String> = s
        .buffs
        .item_enchant_rows
        .iter()
        .map(|row| format!("{}={}", row.item_name, row.spell_name))
        .collect();
    writeln!(out, "enchant {}", enchant.join(";")).unwrap();
    writeln!(out, "peers {}", bool_text(s.vitals.help_network_peers)).unwrap();
    writeln!(out, "give {}", to_string_f64(s.inventory.give_range_meters)).unwrap();
    writeln!(
        out,
        "classifier {}",
        s.inventory.loot.external_classifier_id
    )
    .unwrap();
    let loot: Vec<String> = s
        .inventory
        .loot
        .rules
        .iter()
        .map(|rule| {
            format!(
                "{}|{}|{}|{}",
                rule.name,
                rule.expression(),
                rule.action as i32,
                rule.keep_count
            )
        })
        .collect();
    writeln!(out, "loot {}", loot.join(";")).unwrap();
    let pathing = match s.navigation.client_pathing {
        ClientPathing::Never => "Never",
        ClientPathing::WhenStuck => "WhenStuck",
        ClientPathing::Always => "Always",
    };
    writeln!(out, "pathing {pathing}").unwrap();
    writeln!(out, "jumps {}", s.navigation.jump_attempts).unwrap();
    writeln!(out, "range {}", to_string_f64(c.maximum_range)).unwrap();
}

fn first_run(out: &mut String) {
    let mut host = host();
    let mut live = Live::new();
    let mut store = ProfileStore::new(&mut host);
    let bound = store.bind_character(&mut host, Some("+Acdream")).unwrap();
    writeln!(out, "bind {}", bool_text(bound)).unwrap();
    writeln!(out, "load {}", load_text(live.load(&mut store, &mut host))).unwrap();
    writeln!(out, "save {}", bool_text(live.save(&mut store, &mut host))).unwrap();
    writeln!(out, "selected {}", store.selected()).unwrap();
    files(out, &host);
}

fn set_values(
    s: &mut AllSettings,
    live_no_buff: &mut OrdinalStringSet,
    channels: &mut IgnoreCaseStringSet,
) {
    let c = &mut s.combat;
    c.combat_item_names.insert("Sword".to_owned());
    c.combat_item_names.insert("Bow".to_owned());
    c.combat_item_order.push("Bow".to_owned());
    c.combat_item_object_ids.insert(0x5001);
    c.combat_item_names_by_id.insert(0x5001, "Bow".to_owned());
    c.combat_item_names_by_id.insert(0x5002, "Gone".to_owned());
    c.consumable_names.insert("Mana Potion".to_owned());
    c.consumable_names.insert("Healing Kit".to_owned());
    c.consumable_categories
        .insert("Healing Kit".to_owned(), ConsumableCategory::HealthKit);
    c.consumable_categories
        .insert("Mana Potion".to_owned(), ConsumableCategory::ManaKit);
    live_no_buff.insert("Wand".to_owned());
    channels.insert("Tells".to_owned());
    channels.insert("Fellow".to_owned());
    c.attack_power = 0.75;
    c.meta_state = "Hunting".to_owned();
    c.dynamic_settings
        .insert("MaximumRange".to_owned(), MonsterValue::from_number(25.5));
    c.dynamic_settings.insert(
        "UseRecklessness".to_owned(),
        MonsterValue::from_boolean(true),
    );
    let drudge = MonsterRule::new(
        "Drudge",
        MonsterRuleActions {
            weapon_name: "Bow".to_owned(),
            ..MonsterRuleActions::default()
        },
    )
    .unwrap();
    c.rules = vec![MonsterRule::fresh_default(), drudge];
    s.buffs
        .extra_buff_spell_names
        .insert("Strength Self VII".to_owned());
    s.buffs.item_enchant_rows.push(BuffItemEnchantRow {
        item_name: "Bow".to_owned(),
        spell_name: "Blood Drinker Self VII".to_owned(),
        object_id: None,
        spell_id: None,
    });
    s.vitals.help_network_peers = true;
    s.inventory.give_range_meters = 20.5;
    s.inventory.loot.external_classifier_id = "  vtc  ".to_owned();
    let mut peas = LootRule::named("Keep peas", "Pea");
    peas.action = LootAction::Salvage;
    peas.keep_count = 3;
    s.inventory.loot.rules.push(peas);
    s.navigation.client_pathing = ClientPathing::Always;
    s.navigation.jump_attempts = 5;
}

fn side_car_values(out: &mut String) {
    let mut host = host();
    let mut live = Live::new();
    let mut store = ProfileStore::new(&mut host);
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    live.load(&mut store, &mut host);
    set_values(&mut live.settings, &mut live.no_buff, &mut live.channels);
    writeln!(out, "save {}", bool_text(live.save(&mut store, &mut host))).unwrap();
    files(out, &host);
    out.push_str("--- reload\n");
    let mut fresh = Live::new();
    let mut again = ProfileStore::new(&mut host);
    again.bind_character(&mut host, Some("Acdream")).unwrap();
    writeln!(out, "load {}", load_text(fresh.load(&mut again, &mut host))).unwrap();
    summary(out, &fresh);
}

fn create_and_delete(out: &mut String) {
    let mut host = host();
    let mut live = Live::new();
    let mut store = ProfileStore::new(&mut host);
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    live.load(&mut store, &mut host);
    live.save(&mut store, &mut host);
    let (created, notice) = store
        .create(
            &mut host,
            Some("raid"),
            false,
            &mut live.settings,
            &mut live.no_buff,
            &mut live.channels,
        )
        .unwrap();
    writeln!(out, "create {} {notice}", bool_text(created)).unwrap();
    writeln!(out, "selected {}", store.selected()).unwrap();
    writeln!(
        out,
        "exists raid {} nope {}",
        bool_text(store.exists(&mut host, Some("raid")).unwrap()),
        bool_text(store.exists(&mut host, Some("nope")).unwrap())
    )
    .unwrap();
    writeln!(
        out,
        "select bychar {}",
        bool_text(store.select(&mut host, Some("By char")).unwrap())
    )
    .unwrap();
    writeln!(
        out,
        "select raid {}",
        bool_text(
            store
                .select(&mut host, Some("--Acdream_Coldeve_raid.usd"))
                .unwrap()
        )
    )
    .unwrap();
    writeln!(out, "selected {}", store.selected()).unwrap();
    writeln!(
        out,
        "names {}",
        store.available_names(&mut host).unwrap().join("|")
    )
    .unwrap();
    let (deleted, notice) = store.delete(&mut host).unwrap();
    writeln!(out, "delete {} {notice}", bool_text(deleted)).unwrap();
    writeln!(out, "selected {}", store.selected()).unwrap();
    files(out, &host);
}

fn corrupt(out: &mut String) {
    let mut host = host();
    host.profiles
        .insert("mosstank/profiles/--Acdream_Coldeve.usd", "not a usd\r\n");
    let mut live = Live::new();
    let mut store = ProfileStore::new(&mut host);
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    writeln!(out, "load {}", load_text(live.load(&mut store, &mut host))).unwrap();
    writeln!(
        out,
        "failure {}",
        store.load_failure_notice().unwrap_or_default()
    )
    .unwrap();
    writeln!(
        out,
        "recovery {}",
        store.recovery_notice().unwrap_or_default()
    )
    .unwrap();
    writeln!(out, "save {}", bool_text(live.save(&mut store, &mut host))).unwrap();
    files(out, &host);
}

fn set_in_all(out: &mut String) {
    let mut host = host();
    let mut live = Live::new();
    let mut store = ProfileStore::new(&mut host);
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    live.load(&mut store, &mut host);
    live.save(&mut store, &mut host);
    store
        .create(
            &mut host,
            Some("raid"),
            false,
            &mut live.settings,
            &mut live.no_buff,
            &mut live.channels,
        )
        .unwrap();
    live.settings.combat.maximum_range = 42.0;
    live.settings
        .combat
        .dynamic_settings
        .insert("AttackDistance".to_owned(), MonsterValue::from_number(42.0));
    let count = store
        .set_option_in_all(&mut host, "attackdistance", &live.settings)
        .unwrap();
    writeln!(out, "setinall {count}").unwrap();
    files(out, &host);
}

fn legacy(out: &mut String) {
    let mut host = host();
    let key = format!("profiles/macro/{}.json", sha256_hex("char:ACDREAM"));
    host.plugin.insert(
        &key,
        "{\"Version\":6,\"ItemNames\":[\"Axe\"],\"ConsumableNames\":[\"Pea\"],\"ConsumableCategories\":{\"Pea\":7},\"Combat\":{\"MaximumRange\":30,\"Rules\":[{\"Expression\":\"Drudge\",\"Priority\":9}]}}",
    );
    let mut live = Live::new();
    let mut store = ProfileStore::new(&mut host);
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    writeln!(out, "load {}", load_text(live.load(&mut store, &mut host))).unwrap();
    summary(out, &live);
    files(out, &host);
}

fn roster(out: &mut String) {
    let mut host = host();
    host.plugin.insert(
        "profiles/index.json",
        "{\"Profiles\":[{\"Name\":\"Hunt\",\"Owner\":\"Acdream\"},{\"Name\":\"Other\",\"Owner\":\"Someone\"}]}",
    );
    let key = format!("profiles/macro/{}.json", sha256_hex("named:HUNT"));
    host.plugin.insert(&key, "{\"ItemNames\":[\"Bow\"]}");
    let mut live = Live::new();
    let mut store = ProfileStore::new(&mut host);
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    writeln!(out, "load {}", load_text(live.load(&mut store, &mut host))).unwrap();
    writeln!(
        out,
        "names {}",
        store.available_names(&mut host).unwrap().join("|")
    )
    .unwrap();
    files(out, &host);
}

fn preferences(out: &mut String) {
    let mut host = host();
    let mut store = ProfileStore::new(&mut host);
    store.set_meta_interval_milliseconds(&mut host, 10);
    store.set_game_db_check_interval_hours(&mut host, 500);
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    store.set_mine_only(&mut host, false).unwrap();
    writeln!(
        out,
        "meta {} db {} mine {}",
        store.meta_interval_milliseconds(),
        store.game_db_check_interval_hours(),
        bool_text(store.mine_only())
    )
    .unwrap();
    files(out, &host);
}

fn run(name: &str) -> String {
    let mut out = String::new();
    match name {
        "first-run" => first_run(&mut out),
        "side-car-values" => side_car_values(&mut out),
        "create-and-delete" => create_and_delete(&mut out),
        "corrupt" => corrupt(&mut out),
        "set-in-all" => set_in_all(&mut out),
        "legacy" => legacy(&mut out),
        "roster" => roster(&mut out),
        "preferences" => preferences(&mut out),
        other => panic!("unknown scenario {other}"),
    }
    out
}

#[test]
fn every_scenario_leaves_what_the_csharp_store_leaves() {
    let cases = std::fs::read_to_string(golden_dir().join("cases.txt")).unwrap();
    let expected = std::fs::read_to_string(golden_dir().join("cases.out"))
        .unwrap()
        .replace("\r\n", "\n");
    let mut actual = String::new();
    for line in cases.lines() {
        let name = line.trim();
        if name.is_empty() || name.starts_with('#') {
            continue;
        }
        writeln!(actual, "##### {name}").unwrap();
        actual.push_str(&run(name));
        actual.push('\n');
    }
    if actual != expected {
        let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/golden");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("stores.rust.out"), &actual).unwrap();
        let first = actual
            .lines()
            .zip(expected.lines())
            .position(|(a, e)| a != e)
            .unwrap_or(0);
        panic!(
            "the stores' results differ from the C#'s from line {}: rust {:?} / c# {:?} (full output in target/golden/stores.rust.out)",
            first + 1,
            actual.lines().nth(first),
            expected.lines().nth(first)
        );
    }
}

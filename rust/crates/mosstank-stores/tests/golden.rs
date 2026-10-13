//! The settings profile store against the C# store's own results: each
//! scenario in `golden/cases.txt` runs here and in the C# plugin
//! (`RustPortStoresGolden.cs` in the C# tests, run by `RustPortGolden`),
//! and every file both leave behind, every log line and every answer must
//! match `golden/cases.out`, which the C# wrote. Settings documents (`.usd`)
//! are compared by length and FNV-1a hash; everything else in full. On a
//! mismatch the Rust output is written to `target/golden/stores.rust.out`.
//!
//! Source: the C# `MossTankProfileStore` (with its side-car, legacy and
//! preferences documents), `MossTankLootProfileStore` and
//! `MossTankRouteProfileStore` as black boxes over the scenarios.

use std::fmt::Write as _;
use std::path::PathBuf;

use mosstank_dotnet::collections::{IgnoreCaseStringSet, OrdinalStringSet};
use mosstank_formats::nav;
use mosstank_formats::route::{
    ClientPathing, NavigationSettings, Position, RouteMode, RouteWaypoint, RouteWaypointType,
};
use mosstank_formats::utl::{self, LootAction, LootRule, VtankLootRequirement};
use mosstank_settings::model::{AllSettings, BuffItemEnchantRow, ConsumableCategory, LootSettings};
use mosstank_settings::monster::{MonsterRule, MonsterRuleActions, MonsterValue, MonsterValueKind};
use mosstank_stores::loot_store::LootProfileStore;
use mosstank_stores::profile_store::{ProfileLoad, ProfileStore};
use mosstank_stores::recovery::sha256_hex;
use mosstank_stores::route_store::RouteProfileStore;
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

// ---- The loot profile store ----------------------------------------------------

fn rules_text(rules: &[LootRule]) -> String {
    rules
        .iter()
        .map(|rule| {
            format!(
                "{}|{}|{}|{}|{}",
                rule.name,
                rule.expression(),
                rule.action as i32,
                rule.keep_count,
                rule.vtank_requirements.len()
            )
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn loot_state(out: &mut String, store: &LootProfileStore, host: &mut MemoryHost) {
    writeln!(
        out,
        "selected {} active {} partial {}",
        store.selected(),
        bool_text(store.has_active_profile()),
        bool_text(store.active_profile_is_partial())
    )
    .unwrap();
    writeln!(
        out,
        "names {}",
        store.available_names(host).unwrap().join("|")
    )
    .unwrap();
}

fn peas() -> LootRule {
    let mut rule = LootRule::named("Keep peas", "Pea");
    rule.action = LootAction::KeepUpTo;
    rule.keep_count = 25;
    rule
}

fn loot_first_run(out: &mut String) {
    let mut host = host();
    let mut store = LootProfileStore::new();
    let mut rules = Vec::new();
    let mut settings = LootSettings::default();
    let bound = store.bind_character(&mut host, Some("Acdream")).unwrap();
    writeln!(out, "bind {}", bool_text(bound)).unwrap();
    let load = store
        .load_current(&mut host, &mut rules, Some(&mut settings))
        .unwrap();
    writeln!(out, "load {}", load_text(load)).unwrap();
    rules.push(peas());
    store
        .save_current(&mut host, &rules, Some(&settings))
        .unwrap();
    loot_state(out, &store, &mut host);
    files(out, &host);
}

fn loot_named(out: &mut String) {
    let mut host = host();
    let mut store = LootProfileStore::new();
    let mut rules = Vec::new();
    let mut settings = LootSettings::default();
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    store
        .load_current(&mut host, &mut rules, Some(&mut settings))
        .unwrap();
    rules.push(peas());
    store
        .save_current(&mut host, &rules, Some(&settings))
        .unwrap();
    let (created, notice) = store
        .create(&mut host, Some("Shared"), true, &rules, Some(&mut settings))
        .unwrap();
    writeln!(out, "create {} {notice}", bool_text(created)).unwrap();
    loot_state(out, &store, &mut host);
    let selected = store.select(&mut host, Some("By char")).unwrap();
    writeln!(out, "select bychar {}", bool_text(selected)).unwrap();
    rules.clear();
    let load = store
        .load_current(&mut host, &mut rules, Some(&mut settings))
        .unwrap();
    writeln!(out, "load {} {}", load_text(load), rules_text(&rules)).unwrap();
    writeln!(
        out,
        "exists {} {}",
        bool_text(store.exists(&mut host, Some("shared")).unwrap()),
        bool_text(store.exists(&mut host, Some("nope")).unwrap())
    )
    .unwrap();
    let selected = store.select(&mut host, Some("Shared")).unwrap();
    writeln!(out, "select shared {}", bool_text(selected)).unwrap();
    rules.clear();
    let load = store
        .load_current(&mut host, &mut rules, Some(&mut settings))
        .unwrap();
    writeln!(out, "load {} {}", load_text(load), rules_text(&rules)).unwrap();
    let mut named = Vec::new();
    let found = store
        .try_load_named(&mut host, Some("By char"), &mut named)
        .unwrap();
    writeln!(out, "named {} {}", bool_text(found), rules_text(&named)).unwrap();
    let (deleted, notice) = store.delete(&mut host).unwrap();
    writeln!(out, "delete {} {notice}", bool_text(deleted)).unwrap();
    loot_state(out, &store, &mut host);
    rules.clear();
    let load = store
        .load_current(&mut host, &mut rules, Some(&mut settings))
        .unwrap();
    writeln!(out, "load {} {}", load_text(load), rules_text(&rules)).unwrap();
    loot_state(out, &store, &mut host);
    files(out, &host);
}

fn loot_partial(out: &mut String) {
    let mut host = host();
    let text = utl::serialize_rules(&[peas(), LootRule::named("Second rule", "Gem")]);
    let cut = text.find("Second").unwrap() + 3;
    host.profiles
        .insert("mosstank/loot/--Acdream_Coldeve.utl", &text[..cut]);
    let mut store = LootProfileStore::new();
    let mut rules = Vec::new();
    let mut settings = LootSettings::default();
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    let load = store
        .load_current(&mut host, &mut rules, Some(&mut settings))
        .unwrap();
    writeln!(out, "load {} {}", load_text(load), rules_text(&rules)).unwrap();
    writeln!(out, "notice {}", store.load_notice().unwrap_or_default()).unwrap();
    rules.push(LootRule::named("Added", ""));
    store
        .save_current(&mut host, &rules, Some(&settings))
        .unwrap();
    let (created, notice) = store
        .create(&mut host, Some("Copy"), true, &rules, Some(&mut settings))
        .unwrap();
    writeln!(out, "create {} {notice}", bool_text(created)).unwrap();
    loot_state(out, &store, &mut host);
    files(out, &host);
}

fn loot_import(out: &mut String) {
    let mut host = host();
    host.plugin
        .insert("imports/VtcOld.utl", &utl::serialize_rules(&[peas()]));
    host.plugin
        .insert("imports/Broken.utl", "not a loot profile");
    let mut store = LootProfileStore::new();
    let mut rules = Vec::new();
    let mut settings = LootSettings::default();
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    store
        .load_current(&mut host, &mut rules, Some(&mut settings))
        .unwrap();
    let (imported, notice) = store
        .try_import_legacy(
            &mut host,
            Some("vtcold.utl"),
            &mut rules,
            Some(&mut settings),
        )
        .unwrap();
    writeln!(
        out,
        "import {} {notice} {}",
        bool_text(imported),
        rules_text(&rules)
    )
    .unwrap();
    let (broken, notice) = store
        .try_import_legacy(&mut host, Some("Broken"), &mut rules, Some(&mut settings))
        .unwrap();
    writeln!(out, "import {} {notice}", bool_text(broken)).unwrap();
    let (missing, notice) = store
        .try_import_legacy(&mut host, Some("Nope"), &mut rules, Some(&mut settings))
        .unwrap();
    writeln!(out, "import {} {notice}", bool_text(missing)).unwrap();
    loot_state(out, &store, &mut host);
    files(out, &host);
}

fn loot_legacy(out: &mut String) {
    let mut host = host();
    host.plugin.insert(
        &format!("profiles/loot/{}.json", sha256_hex("char:ACDREAM")),
        "{\"Rules\":[{\"Name\":\"Old\",\"Expression\":\"Salvage\",\"Action\":2}],\"SalvageCombine\":{\"DefaultCombineString\":\"1-10\",\"MaterialValueModeValues\":{\"5\":3}}}",
    );
    host.plugin.insert(
        "profiles/loot/index.json",
        "{\"Names\":[\"Hunt\",\"By char\",\"Gone\"]}",
    );
    host.plugin.insert(
        &format!("profiles/loot/{}.json", sha256_hex("named:HUNT")),
        "{\"Rules\":[{\"Name\":\"Hunt rule\",\"Requirements\":[{\"Type\":1,\"Payload\":\"x\\r\\n\"}],\"HasImportedRequirements\":true}]}",
    );
    let mut store = LootProfileStore::new();
    let mut rules = Vec::new();
    let mut settings = LootSettings::default();
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    let load = store
        .load_current(&mut host, &mut rules, Some(&mut settings))
        .unwrap();
    writeln!(out, "load {} {}", load_text(load), rules_text(&rules)).unwrap();
    writeln!(
        out,
        "salvage {}",
        settings.salvage_combine.default_combine_string
    )
    .unwrap();
    loot_state(out, &store, &mut host);
    files(out, &host);
}

fn loot_unsupported(out: &mut String) {
    let mut host = host();
    let mut radar = LootRule::named("Radar", "");
    radar.vtank_requirements = vec![VtankLootRequirement {
        kind: 2,
        payload: "0\r\n95\r\n1\r\n".to_owned(),
    }];
    radar.has_imported_requirements = true;
    host.profiles.insert(
        "mosstank/loot/--Acdream_Coldeve.utl",
        &utl::serialize_rules(&[radar]),
    );
    let mut store = LootProfileStore::new();
    let mut rules = Vec::new();
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    let load = store.load_current(&mut host, &mut rules, None).unwrap();
    writeln!(out, "load {} {}", load_text(load), rules_text(&rules)).unwrap();
    files(out, &host);
}

// ---- The route store ------------------------------------------------------------

fn route_text(route: &NavigationSettings) -> String {
    let points: Vec<String> = route
        .waypoints
        .iter()
        .map(|point| {
            format!(
                "{}({},{})",
                point.kind.name(),
                to_string_f64(point.position.east_west),
                to_string_f64(point.position.north_south)
            )
        })
        .collect();
    format!("route {} {}", route.mode.name(), points.join(";"))
}

fn two_points() -> NavigationSettings {
    let point = |east_west, north_south| RouteWaypoint {
        kind: RouteWaypointType::Point,
        position: Position {
            cell_id: 0,
            east_west,
            north_south,
            elevation: 0.1,
            heading_degrees: 0.0,
            is_outdoor: true,
        },
        ..RouteWaypoint::default()
    };
    NavigationSettings {
        mode: RouteMode::Linear,
        waypoints: vec![point(1.5, -2.25), point(1.75, -2.5)],
        ..NavigationSettings::default()
    }
}

fn route_state(out: &mut String, store: &RouteProfileStore, host: &mut MemoryHost) {
    writeln!(out, "selected {}", store.selected(host).unwrap()).unwrap();
    writeln!(
        out,
        "names {}",
        store.available_names(host).unwrap().join("|")
    )
    .unwrap();
}

fn route_first_run(out: &mut String) {
    let mut host = host();
    let mut store = RouteProfileStore::new();
    let mut route = NavigationSettings::default();
    let bound = store.bind_character(&mut host, Some("Acdream")).unwrap();
    writeln!(out, "bind {}", bool_text(bound)).unwrap();
    let load = store.load_current(&mut host, &mut route, None).unwrap();
    writeln!(out, "load {}", load_text(load)).unwrap();
    let two = two_points();
    writeln!(
        out,
        "save {}",
        bool_text(store.save_current(&mut host, &two))
    )
    .unwrap();
    route_state(out, &store, &mut host);
    files(out, &host);
}

fn route_named(out: &mut String) {
    let mut host = host();
    let mut store = RouteProfileStore::new();
    let mut route = NavigationSettings::default();
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    store.load_current(&mut host, &mut route, None).unwrap();
    let two = two_points();
    store.save_current(&mut host, &two);
    let (created, notice) = store.create(&mut host, Some("Hunt"), true, &two).unwrap();
    writeln!(out, "create {} {notice}", bool_text(created)).unwrap();
    let (created, notice) = store
        .create(&mut host, Some("Hunt.af"), true, &two)
        .unwrap();
    writeln!(out, "create {} {notice}", bool_text(created)).unwrap();
    route_state(out, &store, &mut host);
    let selected = store.select(&mut host, Some("By char"), false).unwrap();
    writeln!(out, "select bychar {}", bool_text(selected)).unwrap();
    let mut loaded = NavigationSettings::default();
    let load = store.load_current(&mut host, &mut loaded, None).unwrap();
    writeln!(out, "load {} {}", load_text(load), route_text(&loaded)).unwrap();
    let selected = store.select(&mut host, Some("hunt"), false).unwrap();
    writeln!(out, "select hunt {}", bool_text(selected)).unwrap();
    route_state(out, &store, &mut host);
    let mut loaded = NavigationSettings::default();
    let load = store.load_current(&mut host, &mut loaded, None).unwrap();
    writeln!(out, "load {} {}", load_text(load), route_text(&loaded)).unwrap();
    let exact = store.select(&mut host, Some("Hunt.af"), true).unwrap();
    let nope = store.select(&mut host, Some("Nope.af"), true).unwrap();
    writeln!(out, "select exact {} {}", bool_text(exact), bool_text(nope)).unwrap();
    route_state(out, &store, &mut host);
    writeln!(
        out,
        "exists {} {}",
        bool_text(store.exists(&mut host, Some("hunt")).unwrap()),
        bool_text(store.exists(&mut host, Some("nope")).unwrap())
    )
    .unwrap();
    let (deleted, notice) = store.delete(&mut host).unwrap();
    writeln!(out, "delete {} {notice}", bool_text(deleted)).unwrap();
    route_state(out, &store, &mut host);
    files(out, &host);
}

fn route_follow(out: &mut String) {
    let mut host = host();
    let mut store = RouteProfileStore::new();
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    store.select_follow_route(&mut host).unwrap();
    writeln!(out, "follow {}", bool_text(store.follow_route_selected())).unwrap();
    route_state(out, &store, &mut host);
    let mut route = NavigationSettings::default();
    let load = store.load_current(&mut host, &mut route, None).unwrap();
    writeln!(out, "load {} {}", load_text(load), route_text(&route)).unwrap();
    route.mode = RouteMode::Target;
    route.follow_target_name = "Leader".to_owned();
    route.follow_target_object_id = 0x5001;
    writeln!(
        out,
        "save {}",
        bool_text(store.save_current(&mut host, &route))
    )
    .unwrap();
    files(out, &host);
}

fn translate(
    out: &mut String,
    store: &mut RouteProfileStore,
    host: &mut MemoryHost,
    source: &str,
    target: &str,
    force: bool,
) {
    let (done, notice, records) = store
        .try_translate(host, Some(source), Some(target), 0.5, -0.25, force, None)
        .unwrap();
    writeln!(out, "translate {} {notice} {records}", bool_text(done)).unwrap();
}

fn route_translate(out: &mut String) {
    let mut host = host();
    let mut store = RouteProfileStore::new();
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    store
        .load_current(&mut host, &mut NavigationSettings::default(), None)
        .unwrap();
    store
        .create(&mut host, Some("Hunt"), true, &two_points())
        .unwrap();
    translate(out, &mut store, &mut host, "Hunt", "Moved", false);
    translate(out, &mut store, &mut host, "Hunt", "Moved", false);
    translate(out, &mut store, &mut host, "Hunt", "Moved.af", false);
    translate(out, &mut store, &mut host, "Hunt", "Moved", true);
    translate(out, &mut store, &mut host, "Nope", "Other", false);
    translate(out, &mut store, &mut host, "", "Other", false);
    store
        .create(
            &mut host,
            Some("Empty"),
            false,
            &NavigationSettings::default(),
        )
        .unwrap();
    translate(out, &mut store, &mut host, "Empty", "Other", false);
    route_state(out, &store, &mut host);
    files(out, &host);
}

fn route_import(out: &mut String) {
    let mut host = host();
    host.plugin
        .insert("imports/Old.nav", &nav::save(&two_points()).unwrap());
    host.plugin.insert("exports/Bad.nav", "nonsense");
    let mut store = RouteProfileStore::new();
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    let mut route = NavigationSettings::default();
    store.load_current(&mut host, &mut route, None).unwrap();
    let (imported, notice) = store
        .try_import_legacy(&mut host, Some("old"), &mut route, None)
        .unwrap();
    writeln!(
        out,
        "import {} {notice} {}",
        bool_text(imported),
        route_text(&route)
    )
    .unwrap();
    let (bad, notice) = store
        .try_import_legacy(
            &mut host,
            Some("Bad"),
            &mut NavigationSettings::default(),
            None,
        )
        .unwrap();
    writeln!(out, "import {} {notice}", bool_text(bad)).unwrap();
    let (missing, notice) = store
        .try_import_legacy(
            &mut host,
            Some("Nope"),
            &mut NavigationSettings::default(),
            None,
        )
        .unwrap();
    writeln!(out, "import {} {notice}", bool_text(missing)).unwrap();
    route_state(out, &store, &mut host);
    files(out, &host);
}

fn route_corrupt(out: &mut String) {
    let mut host = host();
    host.profiles
        .insert("mosstank/navs/--Acdream_Coldeve.af", "garbage\r\n");
    let mut store = RouteProfileStore::new();
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    let mut route = NavigationSettings::default();
    let load = store.load_current(&mut host, &mut route, None).unwrap();
    writeln!(out, "load {}", load_text(load)).unwrap();
    writeln!(
        out,
        "recovery {}",
        store.recovery_notice().unwrap_or_default()
    )
    .unwrap();
    files(out, &host);
}

fn route_legacy(out: &mut String) {
    let mut host = host();
    host.plugin.insert(
        &format!("profiles/route/{}.json", sha256_hex("char:ACDREAM")),
        "{\"Mode\":1,\"Waypoints\":[{\"Type\":0,\"EastWest\":1.5,\"NorthSouth\":-2.25,\"IsOutdoor\":true},{\"Type\":2,\"Recall\":1}]}",
    );
    host.plugin
        .insert("profiles/route/index.json", "{\"Names\":[\"Old\"]}");
    host.plugin.insert(
        &format!("profiles/route/{}.json", sha256_hex("named:OLD")),
        "{\"Mode\":7,\"Waypoints\":[{\"Type\":99,\"EastWest\":3}]}",
    );
    let mut store = RouteProfileStore::new();
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    let mut route = NavigationSettings::default();
    let load = store.load_current(&mut host, &mut route, None).unwrap();
    writeln!(out, "load {} {}", load_text(load), route_text(&route)).unwrap();
    route_state(out, &store, &mut host);
    files(out, &host);
}

fn route_bare_lf(out: &mut String) {
    let mut host = host();
    host.profiles.insert(
        "mosstank/navs/Lf.nav",
        &nav::save(&two_points()).unwrap().replace("\r\n", "\n"),
    );
    let mut store = RouteProfileStore::new();
    store.bind_character(&mut host, Some("Acdream")).unwrap();
    let selected = store.select(&mut host, Some("Lf"), false).unwrap();
    writeln!(out, "select {}", bool_text(selected)).unwrap();
    let mut route = NavigationSettings::default();
    let load = store.load_current(&mut host, &mut route, None).unwrap();
    writeln!(out, "load {} {}", load_text(load), route_text(&route)).unwrap();
    route.waypoints.remove(1);
    writeln!(
        out,
        "save {}",
        bool_text(store.save_current(&mut host, &route))
    )
    .unwrap();
    store.clear_current(&mut host, &mut route);
    route_state(out, &store, &mut host);
    files(out, &host);
}

fn run(name: &str) -> String {
    let mut out = String::new();
    match name {
        "route-first-run" => route_first_run(&mut out),
        "route-named" => route_named(&mut out),
        "route-follow" => route_follow(&mut out),
        "route-translate" => route_translate(&mut out),
        "route-import" => route_import(&mut out),
        "route-corrupt" => route_corrupt(&mut out),
        "route-legacy" => route_legacy(&mut out),
        "route-bare-lf" => route_bare_lf(&mut out),
        "loot-first-run" => loot_first_run(&mut out),
        "loot-named" => loot_named(&mut out),
        "loot-partial" => loot_partial(&mut out),
        "loot-import" => loot_import(&mut out),
        "loot-legacy" => loot_legacy(&mut out),
        "loot-unsupported" => loot_unsupported(&mut out),
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

//! The fuel each call of the built `mosstank.wasm` uses, against the
//! plugin's own test client with a character sized like a live one, and
//! whether every call fits the openAC host's budgets with room to spare.
//!
//! Run by `scripts/check.sh` after the WebAssembly build (the test is
//! ignored by a plain `cargo test`, which has no module to load):
//!
//! ```text
//! cargo test -p mosstank-fuel --test budgets -- --ignored --nocapture
//! ```
//!
//! `MOSSTANK_FUEL_WASM=<path>` measures another build of the module.
//! `MOSSTANK_FUEL_GAMEDB=<path to gameinfodb.ugd>` measures with a real
//! game database; otherwise a generated one with the downloaded database's
//! tables, row counts and value kinds (2026-09-17, about 2 MB) is used.

// The plugin's own test client: the SDK's fake host, the two storages and
// the C# tests' fake automation (casts, their chat lines, landed buffs).
#[path = "../../mosstank/tests/common/mod.rs"]
mod common;

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;

use ac_plugin_abi::character::{Attribute, Training};
use ac_plugin_abi::combat::CombatTarget;
use ac_plugin_abi::magic::{SPELL_PAGE_MAX, SpellInfo, SpellList, SpellPage};
use ac_plugin_abi::{CommandCall, Event, Import, StartInfo, Wire};
use common::World;
use common::automation::FakeAutomation;
use mosstank_fuel::{CallFuel, Client, DEFAULT_MEMORY_MIB, Plugin};
use openac_plugin::Transport;

/// The share of a call's allowed fuel a measured call may use.
const MARGIN_PERCENT: f64 = 50.0;

/// The monster the fight is with.
const MONSTER_ID: u32 = 0x8000_02A7;

/// The plugin's test client, with the whole spell table paged as the
/// client pages it and the known attack spells (the fake answers only the
/// self buffs).
struct Session {
    world: World,
    table: Vec<SpellInfo>,
    attacks: Vec<SpellInfo>,
    combat: Vec<SpellInfo>,
    known: BTreeSet<u32>,
}

impl Client for Session {
    fn call(&mut self, import: Import, request: &[u8]) -> Vec<u8> {
        let in_world = self.world.automation.is_some();
        match import {
            Import::SpellsPage if in_world => {
                let (first, count) = <(u32, u32)>::from_bytes(request).expect("a page request");
                let first = first as usize;
                let count = count.min(SPELL_PAGE_MAX) as usize;
                SpellPage {
                    total: u32::try_from(self.table.len()).unwrap_or(u32::MAX),
                    spells: self.table.iter().skip(first).take(count).cloned().collect(),
                }
                .to_bytes()
            }
            Import::SpellsKnown if in_world => {
                match SpellList::from_bytes(request).expect("a list") {
                    SpellList::Attacks => self.attacks.to_bytes(),
                    SpellList::Combat => self.combat.to_bytes(),
                    SpellList::SelfBuffs => self.world.call(import, request),
                }
            }
            Import::SpellsKnownIds if in_world => {
                let ids = |spells: &[SpellInfo]| -> Vec<u32> {
                    spells.iter().map(|spell| spell.spell_id).collect()
                };
                match SpellList::from_bytes(request).expect("a list") {
                    SpellList::Attacks => ids(&self.attacks).to_bytes(),
                    SpellList::Combat => ids(&self.combat).to_bytes(),
                    SpellList::SelfBuffs => self.world.call(import, request),
                }
            }
            Import::SpellsIsKnown if in_world => {
                let id = u32::from_bytes(request).expect("a spell id");
                self.known.contains(&id).to_bytes()
            }
            _ => self.world.call(import, request),
        }
    }
}

/// A spell's rank from its name: the closing numeral, an incantation 8.
fn tier_of(name: &str) -> i32 {
    if name.starts_with("Incantation of ") {
        return 8;
    }
    let numeral = name.rsplit(' ').next().unwrap_or("");
    match numeral {
        "I" => 1,
        "II" => 2,
        "III" => 3,
        "IV" => 4,
        "V" => 5,
        "VI" => 6,
        "VII" => 7,
        "VIII" => 8,
        _ => 1,
    }
}

/// A spell's family key: its name without the rank.
fn family_key(name: &str) -> String {
    let base = name.strip_prefix("Incantation of ").unwrap_or(name);
    match base.rsplit_once(' ') {
        Some((head, "I" | "II" | "III" | "IV" | "V" | "VI" | "VII" | "VIII")) => head.to_owned(),
        _ => base.to_owned(),
    }
}

const WAR_WORDS: [&str; 7] = ["Bolt", "Streak", "Arc", "Blast", "Volley", "Ring", "Wall"];
const ITEM_WORDS: [&str; 8] = [
    "Bane",
    "Blood Drinker",
    "Defender",
    "Swift Killer",
    "Heart Seeker",
    "Spirit Drinker",
    "Hermetic",
    "Impenetrability",
];
const LIFE_WORDS: [&str; 7] = [
    "Armor",
    "Protection",
    "Regeneration",
    "Rejuvenation",
    "Mana Renewal",
    "Heal",
    "Revitalize",
];

/// The skills by id (1 to 54), as the client names them.
const SKILLS: [&str; 54] = [
    "Axe",
    "Bow",
    "Crossbow",
    "Dagger",
    "Mace",
    "Melee Defense",
    "Missile Defense",
    "Sling",
    "Spear",
    "Staff",
    "Sword",
    "Thrown Weapon",
    "Unarmed Combat",
    "Arcane Lore",
    "Magic Defense",
    "Mana Conversion",
    "Spellcraft",
    "Item Tinkering",
    "Assess Person",
    "Deception",
    "Healing",
    "Jump",
    "Lockpick",
    "Run",
    "Awareness",
    "Arms and Armor Repair",
    "Assess Creature",
    "Weapon Tinkering",
    "Armor Tinkering",
    "Magic Item Tinkering",
    "Creature Enchantment",
    "Item Enchantment",
    "Life Magic",
    "War Magic",
    "Leadership",
    "Loyalty",
    "Fletching",
    "Alchemy",
    "Cooking",
    "Salvaging",
    "Two Handed Combat",
    "Gearcraft",
    "Void Magic",
    "Heavy Weapons",
    "Light Weapons",
    "Finesse Weapons",
    "Missile Weapons",
    "Shield",
    "Dual Wield",
    "Recklessness",
    "Sneak Attack",
    "Dirty Fighting",
    "Challenge",
    "Summoning",
];

/// What a self buff's description says it raises, as the client's
/// descriptions word it, from its family's name.
fn buff_description(family: &str, tier: i32) -> String {
    let amount = tier * 10;
    let attribute = [
        "Strength",
        "Endurance",
        "Coordination",
        "Quickness",
        "Focus",
        "Willpower",
    ]
    .into_iter()
    .find(|name| family == format!("{name} Self"));
    if let Some(name) = attribute {
        let name = if name == "Willpower" { "Self" } else { name };
        return format!("Increases the caster's {name} by {amount} points.");
    }
    match family {
        "Regeneration Self" => {
            return format!("Increases the caster's natural healing rate by {amount}%.");
        }
        "Rejuvenation Self" => {
            return format!("Increases the rate at which the caster regains Stamina by {amount}%.");
        }
        "Mana Renewal Self" => {
            return format!("Increases the caster's natural mana rate by {amount}%.");
        }
        "Armor Self" => return format!("Increases the caster's natural armor by {amount} points."),
        _ => {}
    }
    if let Some(element) = family.strip_suffix(" Protection Self") {
        return format!("Reduces damage the caster takes from {element} by {amount}%.");
    }
    if let Some(skill) = SKILLS
        .iter()
        .find(|skill| family.starts_with(*skill) && family.ends_with(" Self"))
    {
        return format!("Increases the caster's {skill} skill by {amount} points.");
    }
    format!(
        "{family}: changes the target as the spell table describes, for as long as the \
         enchantment lasts."
    )
}

/// The spell table: the 6266 spells of the generated id table, with
/// families, ranks, schools and texts the size of the client's.
fn spell_table() -> Vec<SpellInfo> {
    let mut families: HashMap<String, u32> = HashMap::new();
    mosstank_data::spell_id::ALL
        .iter()
        .map(|entry| {
            let name = entry.name.unwrap_or(entry.ident).to_owned();
            let next = u32::try_from(families.len()).unwrap_or(0) + 1;
            let family = *families.entry(family_key(&name)).or_insert(next);
            let tier = tier_of(&name);
            let war = WAR_WORDS.iter().any(|word| name.contains(word));
            let school = if war {
                34
            } else if ITEM_WORDS.iter().any(|word| name.contains(word)) {
                32
            } else if LIFE_WORDS.iter().any(|word| name.contains(word)) {
                33
            } else {
                31
            };
            let self_buff = name.contains(" Self") && !war;
            SpellInfo {
                spell_id: entry.id,
                description: buff_description(&family_key(&name), tier),
                name,
                family,
                tier,
                difficulty: tier * 50,
                mana_cost: tier * 15,
                duration_seconds: if self_buff { 1800.0 * tier as f32 } else { 0.0 },
                school,
                is_self_targeted: self_buff && school != 32,
                is_beneficial: self_buff,
                is_offensive: war,
                is_projectile: war,
                requires_turn_to: war,
                formula_component_ids: vec![0x2B, 0x31, 0x40, 0x4F, 0x56],
                saying: "Zojak Malar Equin".to_owned(),
                component_set: (0x40, 0x31, 0x4F, 0x56),
                ..SpellInfo::default()
            }
        })
        .collect()
}

/// A character sized like the live check's: every self buff of the table
/// known, every skill trained, a full pack, a wand, and a monster 3 m away
/// once the fight starts.
fn character_automation(table: &[SpellInfo]) -> FakeAutomation {
    let mut automation = FakeAutomation::default();
    automation.name = "+Acdream".to_owned();
    automation.world_name = "sawato".to_owned();
    automation.level = 275;
    automation.object_id = 0x5000_000A;
    automation.health = (400, 400);
    automation.stamina = (400, 400);
    automation.mana = (600, 600);
    automation.main_pack_free_slots = 40;
    automation.enchantment_duration_seconds = 3600.0;
    automation.skills = SKILLS
        .iter()
        .zip(1..)
        .map(|(name, id)| {
            let mut skill = common::automation::skill(id, name, 450);
            skill.training = Training::Specialized;
            skill
        })
        .collect();
    automation.attributes = [
        "Strength",
        "Endurance",
        "Coordination",
        "Quickness",
        "Focus",
        "Self",
    ]
    .iter()
    .zip(1..)
    .map(|(name, kind)| Attribute {
        kind,
        name: (*name).to_owned(),
        current: 300,
        base: 300,
        ..Attribute::default()
    })
    .collect();
    // Ranks III to VIII and the incantations: some 730 self buffs, about
    // what the live check's idle ticks read (one spellbook read and a
    // few small ones in 19 million instructions).
    let known_rank =
        |spell: &&SpellInfo| !(spell.name.ends_with(" I") || spell.name.ends_with(" II"));
    let self_buffs: Vec<SpellInfo> = table
        .iter()
        .filter(|spell| spell.is_beneficial)
        .filter(known_rank)
        .cloned()
        .collect();
    automation.catalog_spells = table
        .iter()
        .filter(|spell| !spell.is_beneficial || !known_rank(spell))
        .cloned()
        .collect();
    automation.set_known_self_buffs(self_buffs);
    let mut items = Vec::new();
    let names = [
        ("Greater Amuli Shadow Coat", 0x2),
        ("Amuli Shadow Leggings", 0x2),
        ("Shadow Gauntlets", 0x2),
        ("Shadow Sollerets", 0x2),
        ("Shadow Helm", 0x2),
        ("Shadow Bracers", 0x2),
        ("Shadow Girth", 0x2),
        ("Crystal Wand", 0x8000),
        ("Mana Scarab", 0x1000),
        ("Platinum Scarab", 0x1000),
        ("Quill of Infliction", 0x1000),
        ("Ink of Formation", 0x1000),
        ("Glyph of Strength", 0x1000),
        ("Prismatic Taper", 0x1000),
        ("Hyssop", 0x1000),
        ("Powdered Agate", 0x1000),
        ("Mana Stone", 0x800),
        ("Stamina Elixir", 0x80),
        ("Health Elixir", 0x80),
        ("Mana Elixir", 0x80),
        ("Roast Beef", 0x20),
        ("Peerless Healing Kit", 0x80),
        ("Pyreal Mote", 0x80),
        ("Salvaged Iron", 0x80),
    ];
    for index in 0..120u32 {
        let (name, item_type) = names[index as usize % names.len()];
        let mut item = common::item(0x5100_0000 + index, name, item_type);
        item.stack_size = if item_type == 0x1000 { 100 } else { 1 };
        items.push(item);
    }
    automation.item_entries = items;
    automation
}

fn monster() -> CombatTarget {
    CombatTarget {
        object_id: MONSTER_ID,
        name: "Mosswart Kin".to_owned(),
        weenie_class_id: 12702,
        distance: 3.0,
        is_health_known: true,
        health_fraction: 1.0,
        species_name: "Mosswart".to_owned(),
        maximum_health: 120,
        ..CombatTarget::default()
    }
}

/// A game database with the downloaded one's tables, row counts and value
/// kinds, its names and messages as long, at the built-in version.
fn generated_game_database() -> String {
    struct Table {
        name: &'static str,
        columns: &'static [(&'static str, char)],
        index: usize,
        rows: usize,
    }
    let tables = [
        Table {
            name: "AmmunitionOptions",
            columns: &[
                ("AmmoName", 's'),
                ("LauncherType", 'i'),
                ("WieldReq", 'i'),
                ("Element", 'i'),
                ("Quality", 'i'),
                ("Special", 'i'),
                ("WieldReq2Skill", 'i'),
                ("WieldReq2Value", 'i'),
            ],
            index: 0,
            rows: 137,
        },
        Table {
            name: "CooldownIDs",
            columns: &[("Itemname", 's'), ("CooldownID", 'i')],
            index: 0,
            rows: 1084,
        },
        Table {
            name: "CraftInteractions",
            columns: &[
                ("UseItem1", 's'),
                ("UseItem2", 's'),
                ("ResultItem", 's'),
                ("ResultCount", 'i'),
                ("SuccessMsg", 's'),
                ("FailMsg", 's'),
                ("ReqSkill", 'i'),
                ("ReqDiff", 'i'),
                ("ID", 'i'),
            ],
            index: 8,
            rows: 9343,
        },
        Table {
            name: "DBLastUpdateTime",
            columns: &[("Zero", 'i'), ("Time", 'i')],
            index: usize::MAX,
            rows: 1,
        },
        Table {
            name: "DBVersion",
            columns: &[("VersionInt", 'i')],
            index: usize::MAX,
            rows: 1,
        },
        Table {
            name: "DrainSpellOptions",
            columns: &[
                ("SpellID", 'i'),
                ("CastTimeMilliseconds", 'i'),
                ("EnemyDrainFactor", 'd'),
                ("EnemyDrainMaximumPoints", 'i'),
                ("ResultMultiplier", 'd'),
            ],
            index: 0,
            rows: 8,
        },
        Table {
            name: "GrenadeOptions",
            columns: &[
                ("GrenName", 's'),
                ("WieldReqType", 'i'),
                ("WieldReqAttribute", 'i'),
                ("WieldReqValue", 'i'),
                ("Spell", 'i'),
                ("Spellcraft", 'i'),
            ],
            index: 0,
            rows: 72,
        },
        Table {
            name: "HealKits",
            columns: &[
                ("KitName", 's'),
                ("RestoreBonus", 'd'),
                ("SkillBonus", 'i'),
                ("WhichVital", 'i'),
            ],
            index: 0,
            rows: 25,
        },
        Table {
            name: "MartyrSpellOptions",
            columns: &[
                ("SpellID", 'i'),
                ("CastTimeMilliseconds", 'i'),
                ("SelfDrainFactor", 'd'),
                ("ResultMultiplier", 'd'),
            ],
            index: 0,
            rows: 9,
        },
        Table {
            name: "MonsterDamageOverrides",
            columns: &[("Monster", 's'), ("DamageString", 's')],
            index: 0,
            rows: 1495,
        },
        Table {
            name: "MonsterImmunities",
            columns: &[("Monster", 's'), ("ImmunityMask", 'i')],
            index: 0,
            rows: 55,
        },
        Table {
            name: "SpeciesDamages",
            columns: &[("Species", 'i'), ("DamageString", 's')],
            index: 0,
            rows: 94,
        },
        Table {
            name: "SpeciesMembers",
            columns: &[("Monster", 's'), ("Species", 'i'), ("MaximumHealth", 'i')],
            index: 0,
            rows: 2857,
        },
        Table {
            name: "SpellQualityOverrides",
            columns: &[
                ("SpellID", 'i'),
                ("Valid", 'i'),
                ("NewQuality", 'i'),
                ("NewFamily", 'i'),
            ],
            index: 0,
            rows: 0,
        },
    ];
    let mut text = String::new();
    let mut line = |value: &str| {
        text.push_str(value);
        text.push_str("\r\n");
    };
    line(&tables.len().to_string());
    for table in &tables {
        line(table.name);
        line(&table.columns.len().to_string());
        for (column, _) in table.columns {
            line(column);
        }
        for column in 0..table.columns.len() {
            line(if column == table.index { "y" } else { "n" });
        }
        line(&table.rows.to_string());
        for row in 0..table.rows {
            for (column, (name, kind)) in table.columns.iter().enumerate() {
                let value = match (table.name, *name, *kind) {
                    ("DBVersion", _, _) => "9".to_owned(),
                    ("DBLastUpdateTime", "Time", _) => "1758067200".to_owned(),
                    (_, "DamageString", _) => "2;6;0;1;4;3;5".to_owned(),
                    (_, "SuccessMsg", _) => format!("You make a crafted item number {row}."),
                    (_, "FailMsg", _) => format!("You fail to make a crafted item number {row}."),
                    (_, _, 's') => format!("Generated {} {row:05}", table.name),
                    (_, _, 'd') => format!("{}.25", row + column),
                    _ => format!("{}", (row * 7 + column * 13) % 500),
                };
                line(&kind.to_string());
                line(&value);
            }
        }
    }
    text
}

/// One measured case: its worst call and how many calls it took.
struct Case {
    name: &'static str,
    worst: CallFuel,
    calls: usize,
    /// The median call's share of its budget, percent.
    median: f64,
    /// How many calls used more than [`MARGIN_PERCENT`] of their budget.
    over: usize,
}

impl Case {
    fn of(name: &'static str, calls: &[CallFuel]) -> Self {
        let worst = calls
            .iter()
            .max_by(|a, b| a.percent().total_cmp(&b.percent()))
            .cloned()
            .unwrap_or_default();
        let mut shares: Vec<f64> = calls.iter().map(CallFuel::percent).collect();
        shares.sort_by(f64::total_cmp);
        Self {
            name,
            worst,
            calls: calls.len(),
            median: shares.get(shares.len() / 2).copied().unwrap_or_default(),
            over: shares
                .iter()
                .filter(|share| **share > MARGIN_PERCENT)
                .count(),
        }
    }
}

fn top_imports(fuel: &CallFuel) -> String {
    let mut imports: Vec<(&&str, &u32)> = fuel.imports.iter().collect();
    imports.sort_by(|a, b| b.1.cmp(a.1));
    imports
        .iter()
        .take(6)
        .map(|(name, count)| format!("{name} {count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The module measured: `MOSSTANK_FUEL_WASM`, else the release build.
fn wasm_path() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("MOSSTANK_FUEL_WASM") {
        return path.into();
    }
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-unknown-unknown/release/mosstank.wasm")
}

fn start_info() -> StartInfo {
    StartInfo {
        plugin_id: "acdream.mosstank".to_owned(),
        api_version: ac_plugin_abi::API_CURRENT.to_string(),
        client_version: "0.1.0".to_owned(),
    }
}

fn command(verb: &str, arguments: &str) -> Event {
    Event::Command(CommandCall {
        verb: verb.to_owned(),
        arguments: arguments.to_owned(),
        raw: format!("/{verb} {arguments}"),
    })
}

/// The clock moves one tick.
fn advance(session: &mut Session) {
    let clock = &mut session.world.fake.clock;
    clock.monotonic_ms += 15;
    clock.utc_unix_ms += 15;
    clock.utc_unix_100ns += 150_000;
}

fn tick(plugin: &mut Plugin, session: &mut Session) -> CallFuel {
    advance(session);
    plugin.tick(session).expect("a tick")
}

#[test]
#[ignore = "needs the release module: scripts/check.sh runs it after the WebAssembly build"]
fn every_call_fits_the_hosts_budgets_with_room_to_spare() {
    let wasm = std::fs::read(wasm_path()).expect("the release mosstank.wasm (build it first)");
    let game_database = match std::env::var("MOSSTANK_FUEL_GAMEDB") {
        Ok(path) => std::fs::read_to_string(&path).expect("the game database"),
        Err(_) => generated_game_database(),
    };
    let table = spell_table();
    let attacks: Vec<SpellInfo> = table
        .iter()
        .filter(|spell| spell.is_offensive)
        .cloned()
        .collect();
    let known: BTreeSet<u32> = table
        .iter()
        .filter(|spell| {
            spell.is_offensive
                || (spell.is_beneficial
                    && !(spell.name.ends_with(" I") || spell.name.ends_with(" II")))
        })
        .map(|spell| spell.spell_id)
        .collect();
    let mut session = Session {
        world: World::new(),
        combat: attacks.clone(),
        attacks,
        known,
        table,
    };
    session.world.fake.clock.utc_unix_ms = 1_760_000_000_000;
    session.world.fake.clock.utc_unix_100ns = 17_600_000_000_000_000;
    session.world.fake.clock.random_seed = 12345;
    session
        .world
        .profiles
        .insert("gameinfodb.ugd", &game_database);
    // The update was checked a moment ago: no download at start.
    session
        .world
        .plugin_data
        .insert("gamedb/last-check.txt", "2026-10-13T19:00:00.0000000Z");

    let mut cases = Vec::new();

    // Start before the world, the game database cached.
    let (mut plugin, instantiated) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("load");
    let enabled = plugin.enable(&start_info(), &mut session).expect("enable");
    cases.push(Case::of("instantiate", &[instantiated]));
    cases.push(Case::of("start (game database cached)", &[enabled]));

    // The character enters the world.
    let automation = character_automation(&session.table);
    session.world.fake.character = Some(common::character("+Acdream"));
    session.world.automation = Some(automation);
    session.world.chat_log = Some(Vec::new());
    let login = plugin
        .event(&Event::LoginComplete, &mut session)
        .expect("login");
    cases.push(Case::of("event: login complete", &[login]));
    let first = tick(&mut plugin, &mut session);
    cases.push(Case::of("first tick in the world", &[first]));
    let idle: Vec<CallFuel> = (0..100).map(|_| tick(&mut plugin, &mut session)).collect();
    cases.push(Case::of("idle tick", &idle));

    // The macro starts: the buffs.
    let started = plugin
        .event(&command("vt", "start"), &mut session)
        .expect("/vt start");
    cases.push(Case::of("event: /vt start", &[started]));
    let mut buff_ticks = Vec::new();
    let mut other_ticks = Vec::new();
    for _ in 0..1500 {
        let casts_before = session.world.cast_spell_ids().len();
        let fuel = tick(&mut plugin, &mut session);
        if session.world.cast_spell_ids().len() > casts_before {
            buff_ticks.push(fuel);
        } else {
            other_ticks.push(fuel);
        }
    }
    cases.push(Case::of("buff decision tick (a cast)", &buff_ticks));
    cases.push(Case::of("macro tick while buffing", &other_ticks));

    // A monster comes into range: the fight.
    if let Some(automation) = session.world.automation.as_mut() {
        automation.hostile_targets = vec![monster()];
    }
    let mut combat_ticks = Vec::new();
    let mut other_ticks = Vec::new();
    for _ in 0..800 {
        let casts_before = session.world.cast_spell_ids().len();
        let fuel = tick(&mut plugin, &mut session);
        let new_casts = &session.world.cast_spell_ids()[casts_before..];
        let attacked = new_casts
            .iter()
            .any(|id| session.attacks.iter().any(|spell| spell.spell_id == *id));
        if attacked {
            combat_ticks.push(fuel);
        } else {
            other_ticks.push(fuel);
        }
    }
    cases.push(Case::of("combat decision tick (an attack)", &combat_ticks));
    cases.push(Case::of("macro tick while fighting", &other_ticks));

    let stopped = plugin.disable(&mut session).expect("disable");
    cases.push(Case::of("stop", &[stopped]));

    // Started again in the world (a reload), the database cached.
    let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("reload");
    let enabled = plugin
        .enable(&start_info(), &mut session)
        .expect("enable in the world");
    cases.push(Case::of("start in the world (reload)", &[enabled]));
    let first = tick(&mut plugin, &mut session);
    cases.push(Case::of("first tick after the reload", &[first]));

    let mut report = String::new();
    let _ = writeln!(
        report,
        "{:<36} {:>6} {:>7} {:>6} {:>14} {:>14} {:>7}  imports of the worst call",
        "case", "calls", "median", ">half", "worst used", "allowed", "share"
    );
    for case in &cases {
        let _ = writeln!(
            report,
            "{:<36} {:>6} {:>6.1}% {:>6} {:>14} {:>14} {:>6.1}%  {}",
            case.name,
            case.calls,
            case.median,
            case.over,
            case.worst.used,
            case.worst.allowed(),
            case.worst.percent(),
            top_imports(&case.worst)
        );
    }
    println!("{report}");
    if std::env::var_os("MOSSTANK_FUEL_TRACE").is_some() {
        for case in &cases {
            print_profile(case);
        }
    }
    assert!(
        cases
            .iter()
            .filter(|case| case.name.starts_with("buff") || case.name.starts_with("combat"))
            .all(|case| case.calls > 0),
        "the macro made no buff or combat decision:\n{report}"
    );
    let over: Vec<&Case> = cases
        .iter()
        .filter(|case| case.worst.percent() > MARGIN_PERCENT)
        .collect();
    assert!(
        over.is_empty(),
        "calls over {MARGIN_PERCENT}% of their budget:\n{report}"
    );
}

/// The stretches of a case's worst call that ran the most instructions,
/// each between two imports (`MOSSTANK_FUEL_TRACE=1`).
fn print_profile(case: &Case) {
    let trace = &case.worst.trace;
    let traced: u64 = trace.iter().map(|(_, used)| used).sum();
    let mut stretches: Vec<(usize, u64)> = trace
        .iter()
        .enumerate()
        .map(|(index, (_, used))| (index, *used))
        .collect();
    stretches.push((trace.len(), case.worst.used.saturating_sub(traced)));
    stretches.sort_by_key(|stretch| std::cmp::Reverse(stretch.1));
    println!("{} ({} imports):", case.name, trace.len());
    if std::env::var("MOSSTANK_FUEL_TRACE").as_deref() == Ok("full") {
        let names: Vec<String> = trace
            .iter()
            .enumerate()
            .map(|(index, (name, used))| format!("#{index} {name} {used}"))
            .collect();
        println!("    {}", names.join(" | "));
    }
    for (index, used) in stretches.iter().take(8) {
        let after = index
            .checked_sub(1)
            .and_then(|before| trace.get(before))
            .map_or("<start>", |(name, _)| name);
        let next = trace.get(*index).map_or("<end>", |(name, _)| name);
        println!("    {used:>12} after #{index} {after} before {next}");
    }
}

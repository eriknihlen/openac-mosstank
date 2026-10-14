//! The fuel each call of the built `mosstank.wasm` uses, against the
//! plugin's own test client with a heavy character on a busy public server
//! ([`heavy`]), and whether every call stays under half its threshold: no
//! call may cause lag (the host bounds calls by wall time, so this is the
//! measure that does not depend on the machine; `mosstank_fuel`).
//!
//! Run by `scripts/check.sh` after the WebAssembly build (the test is
//! ignored by a plain `cargo test`, which has no module to load):
//!
//! ```text
//! cargo test -p mosstank-fuel --test budgets -- --ignored --nocapture
//! ```
//!
//! `MOSSTANK_FUEL_WASM=<path>` measures another build of the module.
//! `MOSSTANK_FUEL_GAMEDB=<path to gameinfodb.ugd>` measures with that game
//! database (`generated`: the stand-in); otherwise the published one
//! (openac-gamedata's, 2.1 MB) from a checkout beside this one, else a
//! generated one with its tables, row counts, value kinds and size. Three cases: the heavy character's session (login
//! burst, every event kind, idle, buffs, a fight, looting, a reload); a
//! fresh install's download of the game database; and a real player's VTank
//! profile folder ([`profiles`]: the owner's, read in place, or
//! `MOSSTANK_FUEL_PROFILES=<folder>`, else a stand-in with its counts and
//! sizes) with the character whose profile set is the heaviest: start, the
//! first tick in the world, the profile load, idle ticks on a busy server,
//! the chat interceptor, the editors, the Profiles page left open, a reload.
//! `MOSSTANK_FUEL_TRACE=1` profiles the worst call of every case (`full`:
//! every import of it), `MOSSTANK_FUEL_TRACE_CASE=<text>` only the cases
//! whose names hold the text. A log line the plugin writes that starts with
//! `@` names its place in a profile: a marker to profile by.

// The plugin's own test client: the SDK's fake host, the two storages and
// the C# tests' fake automation (casts, their chat lines, landed buffs).
#[path = "../../mosstank/tests/common/mod.rs"]
mod common;
mod heavy;
mod profiles;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use ac_plugin_abi::character::{Attribute, Training};
use ac_plugin_abi::combat::CombatTarget;
use ac_plugin_abi::magic::{SPELL_PAGE_MAX, SpellInfo, SpellList, SpellPage};
use ac_plugin_abi::world::{ObjectChange, ObjectChangeKind};
use ac_plugin_abi::{CommandCall, Event, Import, StartInfo, Wire, calls};
use common::World;
use common::automation::FakeAutomation;
use heavy::Created;
use mosstank_fuel::{CallFuel, Client, DEFAULT_MEMORY_MIB, Plugin};
use openac_plugin::Transport;

/// The share of a call's allowed fuel a measured call may use.
const MARGIN_PERCENT: f64 = 50.0;

/// The chat lines the client keeps for its plugins' reads (openac-rust
/// `ac_runtime::plugins::chat::RETAINED_LINES`).
const CHAT_RETAINED_LINES: usize = 512;

/// The monster the fight is with.
const MONSTER_ID: u32 = 0x8000_02A7;

/// The plugin's test client, with the whole spell table paged as the
/// client pages it, the known attack spells (the fake answers only the
/// self buffs) and an opened corpse's contents.
struct Session {
    world: World,
    table: Vec<SpellInfo>,
    attacks: Vec<SpellInfo>,
    combat: Vec<SpellInfo>,
    known: BTreeSet<u32>,
    /// How often the call under way wrote each file, and its largest write.
    written: BTreeMap<String, (u32, usize)>,
    /// The most writes of one file in one call so far: the count, the file
    /// and the call.
    most_writes: (u32, String, String),
    /// The same of the files of [`LARGE_FILE_BYTES`] or more.
    most_large_writes: (u32, String, String),
}

impl Client for Session {
    fn call(&mut self, import: Import, request: &[u8]) -> Vec<u8> {
        let in_world = self.world.automation.is_some();
        match import {
            Import::StorageWrite => {
                let write = ac_plugin_abi::storage::StorageWrite::from_bytes(request)
                    .expect("a storage write");
                let file = format!("{:?}/{}", write.place, write.key);
                let entry = self.written.entry(file).or_default();
                entry.0 += 1;
                entry.1 = entry.1.max(write.bytes.len());
                self.world.call(import, request)
            }
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
            // The client keeps its last lines only.
            Import::ChatCaptured if self.world.chat_log.is_some() => {
                let after = u64::from_bytes(request).expect("a sequence");
                let log = self.world.chat_log.as_deref().unwrap_or_default();
                let kept = &log[log.len().saturating_sub(CHAT_RETAINED_LINES)..];
                kept.iter()
                    .filter(|line| line.sequence > after)
                    .cloned()
                    .collect::<Vec<_>>()
                    .to_bytes()
            }
            Import::LootContents if self.world.loot.is_some() => {
                let current = self.world.loot().current;
                if current == 0 {
                    Vec::<ac_plugin_abi::items::InventoryItem>::new().to_bytes()
                } else {
                    heavy::corpse_contents(current).to_bytes()
                }
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

/// A heavy character: every self buff of the table known (all ranks),
/// every skill trained, [`heavy::ENCHANTMENTS`] enchantments on it,
/// standing in a town, its pack still empty (the login burst fills it).
fn character_automation(table: &[SpellInfo]) -> FakeAutomation {
    let mut automation = FakeAutomation::default();
    automation.name = "Mossy Hunter".to_owned();
    automation.world_name = "Coldeve".to_owned();
    automation.level = 275;
    automation.object_id = heavy::CHARACTER_ID;
    automation.health = (400, 400);
    automation.stamina = (400, 400);
    automation.mana = (600, 600);
    automation.main_pack_free_slots = 0;
    automation.enchantment_duration_seconds = 3600.0;
    automation.navigation = heavy::navigation();
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
    let self_buffs: Vec<SpellInfo> = table
        .iter()
        .filter(|spell| spell.is_beneficial)
        .cloned()
        .collect();
    automation.catalog_spells = table
        .iter()
        .filter(|spell| !spell.is_beneficial)
        .cloned()
        .collect();
    automation.set_known_self_buffs(self_buffs);
    automation.active_enchantments = heavy::enchantments(table);
    automation
}

/// The spellbook: every attack and self buff, then other spells of the
/// table up to [`heavy::KNOWN_SPELLS`].
fn spellbook(table: &[SpellInfo]) -> BTreeSet<u32> {
    let mut known: BTreeSet<u32> = table
        .iter()
        .filter(|spell| spell.is_offensive || spell.is_beneficial)
        .map(|spell| spell.spell_id)
        .collect();
    for spell in table {
        if known.len() >= heavy::KNOWN_SPELLS {
            break;
        }
        known.insert(spell.spell_id);
    }
    known
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
/// kinds, its strings as long in all (the published file is 2,120,593
/// bytes), at the built-in version.
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
                    // Short enough that the strings come to the published
                    // database's (1,511,965 bytes over 54,029, 2026-09-23).
                    (_, _, 's') => {
                        let short = &table.name[..table.name.len().min(6)];
                        format!("Generated {short} {row:05}")
                    }
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
    name: String,
    worst: CallFuel,
    calls: usize,
    /// The median call's share of its budget, percent.
    median: f64,
    /// How many calls used more than [`MARGIN_PERCENT`] of their budget.
    over: usize,
}

impl Case {
    fn of(name: &str, calls: &[CallFuel]) -> Self {
        let worst = calls
            .iter()
            .max_by(|a, b| a.percent().total_cmp(&b.percent()))
            .cloned()
            .unwrap_or_default();
        let mut shares: Vec<f64> = calls.iter().map(CallFuel::percent).collect();
        shares.sort_by(f64::total_cmp);
        Self {
            name: name.to_owned(),
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
    let fuel = plugin.tick(session).expect("a tick");
    session.tally_writes("tick");
    fuel
}

fn send(plugin: &mut Plugin, session: &mut Session, event: &Event) -> CallFuel {
    let fuel = plugin.event(event, session).expect("an event");
    session.tally_writes(&format!("{event:?}"));
    fuel
}

impl Session {
    /// The call just made: the file it wrote most often (and the large
    /// one), kept when no call before wrote one file as often.
    fn tally_writes(&mut self, call: &str) {
        let written = std::mem::take(&mut self.written);
        let call: String = call.chars().take(120).collect();
        let most = |files: &mut dyn Iterator<Item = (&String, &(u32, usize))>,
                    kept: &mut (u32, String, String)| {
            if let Some((file, (count, _))) = files.max_by_key(|(_, (count, _))| *count)
                && *count > kept.0
            {
                *kept = (*count, file.clone(), call.clone());
            }
        };
        most(&mut written.iter(), &mut self.most_writes);
        most(
            &mut written
                .iter()
                .filter(|(_, (_, bytes))| *bytes >= LARGE_FILE_BYTES),
            &mut self.most_large_writes,
        );
    }
}

/// A file this large is written at most once in a call; a small one (a
/// character's binding) at most twice, as the C#'s select-after-save does.
const LARGE_FILE_BYTES: usize = 64 * 1024;

/// The cases measured, by name, in the order first measured.
#[derive(Default)]
struct Cases {
    order: Vec<String>,
    calls: BTreeMap<String, Vec<CallFuel>>,
}

impl Cases {
    fn add(&mut self, name: &str, fuel: CallFuel) {
        if !self.calls.contains_key(name) {
            self.order.push(name.to_owned());
        }
        self.calls.entry(name.to_owned()).or_default().push(fuel);
    }

    fn all(&self) -> Vec<Case> {
        self.order
            .iter()
            .map(|name| Case::of(name, &self.calls[name]))
            .collect()
    }
}

/// A chat line arrives: kept for the captured-chat reads and sent as the
/// event.
fn chat(plugin: &mut Plugin, session: &mut Session, cases: &mut Cases, case: &str) {
    let log = session.world.chat_log.get_or_insert_with(Vec::new);
    let sequence = log.last().map_or(1, |line| line.sequence + 1);
    let line = heavy::chat_line(sequence, session.world.fake.clock.utc_unix_ms);
    log.push(line.clone());
    let fuel = send(plugin, session, &Event::ChatReceived(line));
    cases.add(case, fuel);
}

/// The client creates `created` (and its owned item), as the login burst
/// does: the object table and the inventory grow, the corpses list too.
fn create(session: &mut Session, created: &Created) {
    let automation = session.world.automation();
    if let Some(item) = &created.item {
        automation.item_entries.push(item.clone());
    }
    automation.world_objects.push(created.object.clone());
    if created.object.object_class == ac_plugin_abi::world::ObjectClass::Corpse {
        let corpse = heavy::corpses(std::slice::from_ref(created));
        let index = session.world.loot().corpses.len();
        let mut corpse = corpse.into_iter().next().expect("a corpse");
        corpse.distance = 3.0 + (index % 25) as f32;
        session.world.loot().corpses.push(corpse);
    }
}

fn object_change(kind: ObjectChangeKind, revision: i64, created: &Created) -> Event {
    Event::ObjectChanged(ObjectChange {
        object_id: created.object.object_id,
        kind,
        revision,
        changed_fields: kind.fields(),
        current: (kind != ObjectChangeKind::Released).then(|| created.object.clone()),
    })
}

fn spawned(created: &Created) -> Event {
    Event::EntitySpawned(ac_plugin_abi::world::EntitySnapshot {
        id: created.object.object_id,
        source_id: created.object.object_id,
        position: (96.0, 96.0, 0.0),
        rotation: (0.0, 0.0, 0.0, 1.0),
    })
}

/// The game database measured: `MOSSTANK_FUEL_GAMEDB`, else the published
/// one (openac-gamedata's `gameinfodb.ugd`, 2.1 MB) from a checkout beside
/// this one, else a generated one with its tables, row counts and value
/// kinds. The published file is AGPL-3.0, so it is read in place, never
/// copied into this repository.
fn game_database() -> String {
    if let Ok(path) = std::env::var("MOSSTANK_FUEL_GAMEDB") {
        if path == "generated" {
            return generated_game_database();
        }
        return std::fs::read_to_string(&path).expect("the game database");
    }
    let published = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../openac-gamedata/out/gameinfodb.ugd");
    std::fs::read_to_string(published).unwrap_or_else(|_| generated_game_database())
}

/// The heavy character's client, out of the world: the spell table, the
/// spellbook, the session proof's profiles (keep everything), an
/// allegiance and a fellowship; storage otherwise empty.
fn new_session() -> Session {
    let table = spell_table();
    let attacks: Vec<SpellInfo> = table
        .iter()
        .filter(|spell| spell.is_offensive)
        .cloned()
        .collect();
    let mut session = Session {
        world: World::new(),
        combat: attacks.clone(),
        attacks,
        known: spellbook(&table),
        written: BTreeMap::new(),
        most_writes: (0, String::new(), String::new()),
        most_large_writes: (0, String::new(), String::new()),
        table,
    };
    session.world.fake.clock.utc_unix_ms = 1_760_000_000_000;
    session.world.fake.clock.utc_unix_100ns = 17_600_000_000_000_000;
    session.world.fake.clock.random_seed = 12345;
    common::write_session_proof_fixture(&mut session.world);
    session
        .world
        .set_reply::<calls::AllegianceState>(&heavy::allegiance());
    let (fellowship, fellows) = heavy::fellowship();
    session
        .world
        .set_reply::<calls::FellowshipState>(&fellowship);
    session
        .world
        .set_reply::<calls::FellowshipMembers>(&fellows.clone());
    session.world.set_reply::<calls::FellowshipRoster>(&fellows);
    session
}

/// The heavy character enters the world, its pack and surroundings all
/// there at once (no burst).
fn enter_world(session: &mut Session) {
    enter_world_as(session, "Mossy Hunter");
}

/// [`enter_world`] as the character `name`.
fn enter_world_as(session: &mut Session, name: &str) {
    let inventory = heavy::inventory();
    let surroundings = heavy::surroundings();
    let mut automation = character_automation(&session.table);
    name.clone_into(&mut automation.name);
    automation.properties = heavy::player_properties(&surroundings);
    for created in inventory.iter().chain(&surroundings) {
        if let Some(item) = &created.item {
            automation.item_entries.push(item.clone());
        }
        automation.world_objects.push(created.object.clone());
    }
    session.world.fake.character = Some(common::character(name));
    session.world.automation = Some(automation);
    session.world.chat_log = Some(Vec::new());
    session.world.loot = Some(common::FrameLootSurface::default());
}

/// The table of cases, worst call first in each.
fn report(cases: &[Case]) -> String {
    let mut report = String::new();
    let _ = writeln!(
        report,
        "{:<38} {:>6} {:>7} {:>6} {:>14} {:>14} {:>7}  imports of the worst call",
        "case", "calls", "median", ">half", "worst used", "allowed", "share"
    );
    for case in cases {
        let _ = writeln!(
            report,
            "{:<38} {:>6} {:>6.1}% {:>6} {:>14} {:>14} {:>6.1}%  {}",
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
    if std::env::var_os("MOSSTANK_FUEL_TRACE").is_some() {
        let only = std::env::var("MOSSTANK_FUEL_TRACE_CASE").unwrap_or_default();
        for case in cases
            .iter()
            .filter(|case| case.name.contains(only.as_str()))
        {
            print_profile(case);
        }
    }
    report
}

/// Every case under half its budget.
fn assert_within_margin(cases: &[Case], report: &str) {
    let over: Vec<&Case> = cases
        .iter()
        .filter(|case| case.worst.percent() > MARGIN_PERCENT)
        .collect();
    assert!(
        over.is_empty(),
        "calls over {MARGIN_PERCENT}% of their budget:\n{report}"
    );
}

/// The handle the client gives the game database's download.
const DOWNLOAD_HANDLE: u32 = 77;

#[test]
#[ignore = "needs the release module: scripts/check.sh runs it after the WebAssembly build"]
fn the_game_database_download_fits_on_a_fresh_install() {
    let wasm = std::fs::read(wasm_path()).expect("the release mosstank.wasm (build it first)");
    let game_database = game_database();
    // A fresh install: no database, no check kept; the download is sent.
    let mut session = new_session();
    session
        .world
        .set_reply::<calls::NetHttpRequest>(&Ok(DOWNLOAD_HANDLE));
    let mut cases = Cases::default();
    let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("load");
    let enabled = plugin.enable(&start_info(), &mut session).expect("enable");
    cases.add("start (fresh install)", enabled);
    enter_world(&mut session);
    let login = send(&mut plugin, &mut session, &Event::LoginComplete);
    cases.add("event: login complete", login);
    for _ in 0..200 {
        let fuel = tick(&mut plugin, &mut session);
        cases.add("tick before the download's reply", fuel);
    }
    assert!(
        !session.world.requests::<calls::NetHttpRequest>().is_empty(),
        "the download was not sent"
    );
    let reply = Event::Net(ac_plugin_abi::net::NetEvent::HttpCompleted(
        ac_plugin_abi::net::HttpReply {
            handle: DOWNLOAD_HANDLE,
            status: 200,
            reason: "OK".to_owned(),
            body: game_database.clone().into_bytes(),
            ..ac_plugin_abi::net::HttpReply::default()
        },
    ));
    let fuel = send(&mut plugin, &mut session, &reply);
    cases.add("event: the download's reply", fuel);
    for _ in 0..1000 {
        let fuel = tick(&mut plugin, &mut session);
        cases.add("tick after the download's reply", fuel);
    }
    let saved = session.world.profiles.get("gameinfodb.ugd").map(str::len);
    let cases = cases.all();
    let report = report(&cases);
    println!("{report}");
    assert_eq!(
        saved,
        Some(game_database.len()),
        "the download was not saved:\n{report}\n{:?}",
        session.world.chat()
    );
    assert!(
        session
            .world
            .chat()
            .iter()
            .any(|line| line.contains("Game database updated")),
        "the update was not announced: {:?}",
        session.world.chat()
    );
    assert_within_margin(&cases, &report);
}

/// The handle the client gives the aliases' chat interceptor.
const INTERCEPTOR_HANDLE: u32 = 41;

/// A click or a pick on one of the plugin's windows (any of them: a
/// handler is found by its name).
fn ui_action(session: &Session, handler: &str, value: ac_plugin_abi::ui::ActionValue) -> Event {
    let panel = session
        .world
        .fake
        .ui
        .panels()
        .next()
        .map(|panel| panel.id)
        .expect("a registered window");
    Event::Ui(ac_plugin_abi::ui::UiEvent::Action(
        ac_plugin_abi::ui::UiAction {
            panel,
            handler: handler.to_owned(),
            value,
        },
    ))
}

/// The stand-in for the owner's login meta: on entering its first state it
/// sets the options and loads the routes the owner's meta does, by chat
/// command, all in one pass.
fn option_meta(folder: &profiles::Folder) -> String {
    use mosstank_formats::meta::{
        MetaAction, MetaActionKind, MetaCondition, MetaProfile, MetaRule,
    };
    let route = |index: usize| {
        folder
            .files
            .iter()
            .filter_map(|(key, _)| key.strip_prefix("mosstank/navs/")?.strip_suffix(".nav"))
            .filter(|name| !name.contains('/'))
            .nth(index)
            .unwrap_or_default()
            .to_owned()
    };
    let mut lines: Vec<String> = [
        "enablecombat false",
        "enablelooting true",
        "enablenav true",
        "lootonlyrarecorpses true",
        "enablebuffing true",
        "lootpriorityboost true",
        "navclosestoprange 0.008",
        "navpriorityboost false",
        "autocram true",
        "enablecombat true",
    ]
    .iter()
    .map(|option| format!("/vt opt set {option}"))
    .collect();
    lines.push(format!("/vt nav load {}", route(0)));
    lines.push(format!("/vt nav load {}", route(1)));
    let rules = lines
        .into_iter()
        .map(|line| {
            let mut action = MetaAction::of(MetaActionKind::ChatCommand);
            action.text = line;
            MetaRule::new("Default", MetaCondition::always(), action)
        })
        .collect();
    mosstank_formats::met::save(&MetaProfile { rules }).expect("the stand-in login meta")
}

/// The client with the profile folder in its profile storage, the game
/// database checked lately and the aliases' interceptor granted.
fn folder_session(folder: &profiles::Folder) -> Session {
    let mut session = new_session();
    session
        .world
        .profiles
        .insert("gameinfodb.ugd", &game_database());
    session
        .world
        .plugin_data
        .insert("gamedb/last-check.txt", "2026-10-13T19:00:00.0000000Z");
    for (key, text) in &folder.files {
        session.world.profiles.insert(key, text);
    }
    session
        .world
        .set_reply::<calls::ChatAddInterceptor>(&Ok(INTERCEPTOR_HANDLE));
    session
}

/// The meta the owner's session loads at login (`bore`), else the
/// stand-in that does what it does ([`option_meta`]), put in the folder.
fn login_meta(folder: &profiles::Folder, session: &mut Session) -> String {
    if folder
        .files
        .iter()
        .any(|(key, _)| key.eq_ignore_ascii_case("mosstank/metas/bore.met"))
    {
        return "bore".to_owned();
    }
    session
        .world
        .profiles
        .insert("mosstank/metas/loginmeta.met", &option_meta(folder));
    "loginmeta".to_owned()
}

/// The owner's session's login commands (the launcher's `loginCommands`,
/// play.coldeve.ac, 2026-10-14): UB variables, `meta` loaded by name and
/// the macro started.
fn login_commands(meta: &str) -> [(&'static str, String); 7] {
    [
        ("ub", "mexec setvar[navNumber,2]".to_owned()),
        ("ub", "mexec setvar[attackRange, 0.033333]".to_owned()),
        ("ub", "mexec setvar[augInnateSelfTarget,10]".to_owned()),
        ("ub", "mexec setvar[augResistSlashTarget,2]".to_owned()),
        (
            "ub",
            "mexec chatbox[\\/a I just logged in and starting meta bore with number +getvar[navNumber]]"
                .to_owned(),
        ),
        ("vt", format!("meta load {meta}")),
        ("vt", "start".to_owned()),
    ]
}

/// The character on the folder's world whose binding names `meta`, else
/// the folder's heaviest.
fn character_bound_to(folder: &profiles::Folder, meta: &str) -> String {
    let prefix = format!("mosstank/profiles/{}_", profiles::WORLD);
    let wanted = format!("mosstank/metas/{meta}.met");
    folder
        .files
        .iter()
        .find_map(|(key, text)| {
            let name = key.strip_prefix(&prefix)?.strip_suffix(".cdf")?;
            text.lines()
                .any(|line| line.trim().eq_ignore_ascii_case(&wanted))
                .then(|| name.to_owned())
        })
        .unwrap_or_else(|| folder.character.clone())
}

/// No call wrote a large file more than once, nor any file many times
/// (owner, 2026-10-14: a meta's option changes saved the settings profile
/// and its 1.36 MB side-car at every one, 26 writes in one tick, past the
/// host's hard limit).
fn assert_no_file_rewritten(session: &Session, report: &str) {
    for (most, limit) in [(&session.most_large_writes, 1), (&session.most_writes, 2)] {
        let (count, file, call) = most;
        assert!(
            *count <= limit,
            "{file} was written {count} times in one call ({call})\n{report}"
        );
    }
}

/// The owner's own login (play.coldeve.ac, 2026-10-14): the plugin starts
/// before the world, the character whose binding names the login meta
/// enters, and the launcher's login commands arrive at once, before the
/// first tick, with the character's profile set not yet read. Then the
/// macro runs the meta.
#[test]
#[ignore = "needs the release module: scripts/check.sh runs it after the WebAssembly build"]
fn the_owners_login_fits() {
    let wasm = std::fs::read(wasm_path()).expect("the release mosstank.wasm (build it first)");
    let folder = profiles::folder();
    let mut session = folder_session(&folder);
    let meta = login_meta(&folder, &mut session);
    let character = character_bound_to(&folder, &meta);
    let mut cases = Cases::default();

    // The commands before the first tick, and after it (the character's
    // profile set then loading, a step a tick).
    for ticks_first in [0, 1] {
        let mut session = folder_session(&folder);
        login_meta(&folder, &mut session);
        let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("load");
        let enabled = plugin.enable(&start_info(), &mut session).expect("enable");
        cases.add("start (the profile folder)", enabled);
        enter_world_as(&mut session, &character);
        let login = send(&mut plugin, &mut session, &Event::LoginComplete);
        cases.add("event: login complete", login);
        for _ in 0..ticks_first {
            let fuel = tick(&mut plugin, &mut session);
            cases.add("first tick in the world", fuel);
        }
        for (verb, arguments) in login_commands(&meta) {
            let fuel = send(&mut plugin, &mut session, &command(verb, &arguments));
            cases.add("event: a login command (at login)", fuel);
        }
        for _ in 0..600 {
            chat(&mut plugin, &mut session, &mut cases, "chat line");
            let fuel = tick(&mut plugin, &mut session);
            cases.add("tick (the login meta running)", fuel);
        }
        assert_no_file_rewritten(&session, "");
    }

    let cases = cases.all();
    let mut report = report(&cases);
    let _ = writeln!(
        report,
        "profile folder: {}; character: {character}; meta: {meta}",
        folder.source
    );
    println!("{report}");
    assert_no_file_rewritten(&session, &report);
    assert_within_margin(&cases, &report);
}

#[test]
#[ignore = "needs the release module: scripts/check.sh runs it after the WebAssembly build"]
#[allow(clippy::too_many_lines)]
fn a_real_profile_folder_fits() {
    use ac_plugin_abi::ui::ActionValue;
    let wasm = std::fs::read(wasm_path()).expect("the release mosstank.wasm (build it first)");
    let folder = profiles::folder();
    let mut session = folder_session(&folder);
    let mut cases = Cases::default();

    // Start before the world, the folder there.
    let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("load");
    let enabled = plugin.enable(&start_info(), &mut session).expect("enable");
    cases.add("start (the profile folder)", enabled);

    // The character whose profile set is the heaviest enters the world; on
    // a busy server a line of chat arrives every tick.
    enter_world_as(&mut session, &folder.character);
    let login = send(&mut plugin, &mut session, &Event::LoginComplete);
    cases.add("event: login complete", login);
    let first = tick(&mut plugin, &mut session);
    cases.add("first tick in the world", first);
    for _ in 0..300 {
        chat(&mut plugin, &mut session, &mut cases, "chat line");
        let fuel = tick(&mut plugin, &mut session);
        cases.add("ticks after login (the profile load)", fuel);
    }
    for _ in 0..400 {
        chat(&mut plugin, &mut session, &mut cases, "chat line");
        let fuel = tick(&mut plugin, &mut session);
        cases.add("idle tick (a chat line each)", fuel);
    }

    // The player types: the aliases' interceptor sees every line.
    for line in [
        "hello there",
        "/vt opt get enablelooting",
        "/ub help",
        "lfg",
    ] {
        let callback =
            ac_plugin_abi::callback::Callback::ChatIntercept((INTERCEPTOR_HANDLE, line.to_owned()));
        let fuel = plugin
            .callback(&callback, &mut session)
            .expect("a callback");
        cases.add("callback: chat intercept", fuel);
        let fuel = tick(&mut plugin, &mut session);
        cases.add("ticks between the editors", fuel);
    }

    // The editors: the profiles page, the meta and loot editors, their
    // rows picked, and the settings pages reloaded.
    let actions = [
        ("ShowProfiles", ActionValue::None),
        ("ShowMeta", ActionValue::None),
        ("SelectMetaRule", ActionValue::Int(0)),
        ("SelectMetaRule", ActionValue::Int(40)),
        ("ShowLootEditor", ActionValue::None),
        ("SelectLootRule", ActionValue::Int(0)),
        ("SelectLootRule", ActionValue::Int(1000)),
        ("ToggleLootEditorVisible", ActionValue::None),
        ("ToggleLootEditorVisible", ActionValue::None),
    ];
    for (handler, value) in actions {
        let event = ui_action(&session, handler, value);
        let fuel = send(&mut plugin, &mut session, &event);
        cases.add("event: editor refresh", fuel);
        let fuel = tick(&mut plugin, &mut session);
        cases.add("ticks between the editors", fuel);
    }
    let refreshed = send(&mut plugin, &mut session, &command("vt", "refresh"));
    cases.add("event: /vt refresh", refreshed);

    // An option changed on the panel saves the profiles, the settings
    // side-car with every loot rule among them, as each change does.
    let settle = |plugin: &mut Plugin, session: &mut Session, cases: &mut Cases| {
        for _ in 0..60 {
            chat(plugin, session, cases, "chat line");
            let fuel = tick(plugin, session);
            cases.add("ticks after a save or a pick", fuel);
        }
    };
    for handler in ["LootRangeUp", "LootRangeDown", "LootRangeUp"] {
        let event = ui_action(&session, handler, ActionValue::None);
        let fuel = send(&mut plugin, &mut session, &event);
        cases.add("event: an option changed (a save)", fuel);
        settle(&mut plugin, &mut session, &mut cases);
    }

    // The player picks profiles: the largest loot profile and meta in the
    // folder, by the pickers and by command, then the character's own.
    let largest = |kind: &str, extension: &str| {
        let prefix = format!("mosstank/{kind}/");
        let (key, _) = folder
            .files
            .iter()
            .filter(|(key, _)| {
                key.strip_prefix(&prefix)
                    .is_some_and(|name| name.ends_with(extension) && !name.contains('/'))
            })
            .max_by_key(|(_, text)| text.len())
            .expect("a profile");
        let name = key.rsplit('/').next().unwrap_or_default();
        name.strip_suffix(extension).unwrap_or(name).to_owned()
    };
    let loot = largest("loot", ".utl");
    let meta = largest("metas", ".met");
    let picks = [
        (
            "event: a loot profile picked",
            ui_action(
                &session,
                "SelectLootProfile",
                ActionValue::Text(loot.clone()),
            ),
        ),
        (
            "event: a meta picked",
            ui_action(
                &session,
                "SelectMetaProfile",
                ActionValue::Text(meta.clone()),
            ),
        ),
        (
            "event: a loot profile picked",
            command("vt", "loot load By char"),
        ),
        (
            "event: a meta picked",
            command("vt", &format!("meta load {meta}")),
        ),
        (
            "event: a meta picked",
            ui_action(
                &session,
                "SelectMetaProfile",
                ActionValue::Text("By char".to_owned()),
            ),
        ),
        (
            "event: a settings profile picked",
            ui_action(
                &session,
                "SelectMacroProfile",
                ActionValue::Text("By char".to_owned()),
            ),
        ),
    ];
    for (case, event) in &picks {
        let fuel = send(&mut plugin, &mut session, event);
        cases.add(case, fuel);
        settle(&mut plugin, &mut session, &mut cases);
    }
    let said = session.world.chat().join("\n") + &session.world.all_logs().join("\n");
    for picked in [
        format!("Loaded loot profile {loot}."),
        format!("Loaded Meta profile {meta}."),
        "Loaded settings profile ".to_owned(),
    ] {
        assert!(said.contains(&picked), "not said: {picked}");
    }

    // The Profiles page stays open: its pickers list their folders again
    // every few seconds.
    for _ in 0..400 {
        chat(&mut plugin, &mut session, &mut cases, "chat line");
        let fuel = tick(&mut plugin, &mut session);
        cases.add("idle tick (Profiles page open)", fuel);
    }

    // The owner's login commands once the set is read (the login itself is
    // `the_owners_login_fits`): the meta's states set options and load
    // routes by chat command, many in one pass.
    let login_meta = login_meta(&folder, &mut session);
    for (verb, arguments) in login_commands(&login_meta) {
        let fuel = send(&mut plugin, &mut session, &command(verb, &arguments));
        cases.add("event: a login command", fuel);
    }
    for _ in 0..300 {
        chat(&mut plugin, &mut session, &mut cases, "chat line");
        let fuel = tick(&mut plugin, &mut session);
        cases.add("tick (the login meta running)", fuel);
    }
    let stopped = send(&mut plugin, &mut session, &command("vt", "stop"));
    cases.add("event: a login command", stopped);
    for _ in 0..60 {
        chat(&mut plugin, &mut session, &mut cases, "chat line");
        let fuel = tick(&mut plugin, &mut session);
        cases.add("idle tick (Profiles page open)", fuel);
    }

    // Started again in the world (a reload): the constructor reads the
    // whole set at once, within the start's budget.
    let stopped = plugin.disable(&mut session).expect("disable");
    cases.add("stop", stopped);
    let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("reload");
    let enabled = plugin
        .enable(&start_info(), &mut session)
        .expect("enable in the world");
    cases.add("start in the world (reload)", enabled);
    for _ in 0..100 {
        chat(&mut plugin, &mut session, &mut cases, "chat line");
        let fuel = tick(&mut plugin, &mut session);
        cases.add("idle tick after the reload", fuel);
    }

    let cases = cases.all();
    let mut report = report(&cases);
    for folder_name in ["metas", "navs", "profiles", "loot", "ub"] {
        let (count, bytes) = folder.count(folder_name);
        let _ = writeln!(report, "{folder_name}: {count} files, {bytes} bytes");
    }
    let _ = writeln!(
        report,
        "profile folder: {}; character: {}",
        folder.source, folder.character
    );
    println!("{report}");
    let chat = session.world.chat();
    assert!(
        chat.iter()
            .chain(session.world.all_logs().iter())
            .any(|line| line.contains("rules)")),
        "the loot profile was not loaded:\n{report}\n{chat:?}"
    );
    assert_no_file_rewritten(&session, &report);
    assert_within_margin(&cases, &report);
}

#[test]
#[ignore = "needs the release module: scripts/check.sh runs it after the WebAssembly build"]
#[allow(clippy::too_many_lines)]
fn every_call_fits_the_hosts_budgets_with_room_to_spare() {
    let wasm = std::fs::read(wasm_path()).expect("the release mosstank.wasm (build it first)");
    let game_database = game_database();
    let mut session = new_session();
    session
        .world
        .profiles
        .insert("gameinfodb.ugd", &game_database);
    // The update was checked a moment ago: no download at start.
    session
        .world
        .plugin_data
        .insert("gamedb/last-check.txt", "2026-10-13T19:00:00.0000000Z");
    let mut cases = Cases::default();

    // Start before the world, the game database cached.
    let (mut plugin, instantiated) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("load");
    let enabled = plugin.enable(&start_info(), &mut session).expect("enable");
    cases.add("instantiate", instantiated);
    cases.add("start (game database cached)", enabled);

    // The character enters the world: the login burst creates its pack and
    // everything about it, with the town's chat, while the plugin ticks.
    let inventory = heavy::inventory();
    let surroundings = heavy::surroundings();
    let mut automation = character_automation(&session.table);
    automation.properties = heavy::player_properties(&surroundings);
    session.world.fake.character = Some(common::character("Mossy Hunter"));
    session.world.automation = Some(automation);
    session.world.chat_log = Some(Vec::new());
    session.world.loot = Some(common::FrameLootSurface::default());
    let mut revision = 0i64;
    let mut ticked = false;
    for (index, created) in inventory.iter().chain(&surroundings).enumerate() {
        create(&mut session, created);
        revision += 1;
        let fuel = send(
            &mut plugin,
            &mut session,
            &object_change(ObjectChangeKind::Created, revision, created),
        );
        if created.item.is_some() {
            cases.add("burst: object created (carried)", fuel);
        } else {
            cases.add("burst: object created (about)", fuel);
            let fuel = send(&mut plugin, &mut session, &spawned(created));
            cases.add("burst: entity spawned", fuel);
        }
        if index + 1 == heavy::INVENTORY_ITEMS {
            let fuel = send(&mut plugin, &mut session, &Event::LoginComplete);
            cases.add("event: login complete (in the burst)", fuel);
        }
        if index % (heavy::BURST_OBJECTS / heavy::BURST_CHAT_LINES) == 7 {
            chat(&mut plugin, &mut session, &mut cases, "burst: chat line");
        }
        if index % 15 == 14 {
            let fuel = tick(&mut plugin, &mut session);
            if ticked {
                cases.add("burst: tick", fuel);
            } else {
                cases.add("first tick in the world", fuel);
                ticked = true;
            }
        }
    }
    let after_burst: Vec<CallFuel> = (0..200).map(|_| tick(&mut plugin, &mut session)).collect();
    for fuel in after_burst {
        cases.add("ticks after the burst", fuel);
    }

    // Every other kind of event, at this scale.
    let carried = &inventory[heavy::SIDE_PACKS + 40];
    let player = &surroundings[3];
    let dropped = surroundings.last().expect("a dropped item");
    let kinds: Vec<(&str, Event)> = vec![
        (
            "event: object updated",
            object_change(ObjectChangeKind::Updated, revision + 1, carried),
        ),
        (
            "event: object appraised",
            object_change(ObjectChangeKind::IdentReceived, revision + 2, player),
        ),
        (
            "event: object moved",
            object_change(ObjectChangeKind::Moved, revision + 3, carried),
        ),
        (
            "event: object released",
            object_change(ObjectChangeKind::Released, revision + 4, dropped),
        ),
        ("event: window attached", Event::WindowAttached(true)),
        (
            "event: selection changed",
            Event::SelectionChanged(ac_plugin_abi::world::SelectionChange {
                previous: None,
                selected: Some(player.object.object_id),
            }),
        ),
        (
            "event: navigation snapshot",
            Event::NavigationSnapshotChanged(heavy::navigation()),
        ),
        (
            "event: experience changed",
            Event::ExperienceChanged(Some(
                ac_plugin_abi::character::CharacterExperience::default(),
            )),
        ),
        (
            "event: item use completed",
            Event::ItemUseCompleted(ac_plugin_abi::items::ItemUseCompletion::default()),
        ),
        (
            "event: equipment placement",
            Event::EquipmentPlacement(ac_plugin_abi::items::EquipmentObservation {
                object_id: carried.object.object_id,
                equipped_location: 0x0000_0E00,
                ..ac_plugin_abi::items::EquipmentObservation::default()
            }),
        ),
        (
            "event: activation completed",
            Event::ActivationCompleted(ac_plugin_abi::world::ActivationCompletion::default()),
        ),
        (
            "event: confirmation requested",
            Event::ConfirmationRequested(ac_plugin_abi::world::Confirmation {
                context_id: 7,
                kind: 5,
                text: "Brightblade 1 would like to recruit you to a fellowship.".to_owned(),
            }),
        ),
        (
            "event: portal transition",
            Event::PortalTransition(ac_plugin_abi::world::PortalTransition::default()),
        ),
        (
            "event: navigation changed",
            Event::NavigationChanged(ac_plugin_abi::navigation::GoToReport::default()),
        ),
        (
            "event: vendor opened",
            Event::Vendor(ac_plugin_abi::trade::VendorEvent::Opened(
                surroundings[NEARBY_VENDOR].object.object_id,
            )),
        ),
        (
            "event: vendor closed",
            Event::Vendor(ac_plugin_abi::trade::VendorEvent::Closed),
        ),
        (
            "event: container opened",
            Event::ContainerOpened(surroundings[NEARBY_VENDOR].object.object_id),
        ),
        (
            "event: container closed",
            Event::ContainerClosed(surroundings[NEARBY_VENDOR].object.object_id),
        ),
    ];
    for (name, event) in &kinds {
        for _ in 0..3 {
            let fuel = send(&mut plugin, &mut session, event);
            cases.add(name, fuel);
            let fuel = tick(&mut plugin, &mut session);
            cases.add("ticks between the events", fuel);
        }
    }
    let idle: Vec<CallFuel> = (0..400).map(|_| tick(&mut plugin, &mut session)).collect();
    for fuel in idle {
        cases.add("idle tick", fuel);
    }

    // The macro starts: the buffs, with the town still talking.
    let started = send(&mut plugin, &mut session, &command("vt", "start"));
    cases.add("event: /vt start", started);
    for index in 0..1500 {
        let casts_before = session.world.cast_spell_ids().len();
        let fuel = tick(&mut plugin, &mut session);
        if session.world.cast_spell_ids().len() > casts_before {
            cases.add("buff decision tick (a cast)", fuel);
        } else {
            cases.add("macro tick while buffing", fuel);
        }
        if index % 40 == 0 {
            chat(
                &mut plugin,
                &mut session,
                &mut cases,
                "chat line while buffing",
            );
        }
    }

    // The monsters come into range: the fight, the nearest first.
    let targets = heavy::hostile_targets(&surroundings, monster());
    session.world.automation().hostile_targets = targets;
    for index in 0..800 {
        let casts_before = session.world.cast_spell_ids().len();
        let fuel = tick(&mut plugin, &mut session);
        let new_casts = &session.world.cast_spell_ids()[casts_before..];
        let attacked = new_casts
            .iter()
            .any(|id| session.attacks.iter().any(|spell| spell.spell_id == *id));
        if attacked {
            cases.add("combat decision tick (an attack)", fuel);
        } else {
            cases.add("macro tick while fighting", fuel);
        }
        if index % 20 == 0 {
            chat(
                &mut plugin,
                &mut session,
                &mut cases,
                "chat line while fighting",
            );
        }
    }

    // The fight is over: the corpses are looted.
    session.world.automation().hostile_targets.clear();
    for arguments in ["loot load vt-proof-loot", "opt set enablelooting true"] {
        let fuel = send(&mut plugin, &mut session, &command("vt", arguments));
        cases.add("event: /vt loot command", fuel);
    }
    let mut opened = BTreeSet::new();
    for _ in 0..1200 {
        let fuel = tick(&mut plugin, &mut session);
        cases.add("loot tick", fuel);
        let loot = session.world.loot();
        if loot.opened != 0 && loot.current != loot.opened {
            loot.current = loot.opened;
            loot.contents_ready = true;
            opened.insert(loot.opened);
            let event = Event::ContainerOpened(loot.opened);
            let fuel = send(&mut plugin, &mut session, &event);
            cases.add("event: corpse opened", fuel);
        }
    }

    let stopped = plugin.disable(&mut session).expect("disable");
    cases.add("stop", stopped);

    // Started again in the world (a reload): it hears every object there.
    let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("reload");
    let enabled = plugin
        .enable(&start_info(), &mut session)
        .expect("enable in the world");
    cases.add("start in the world (reload)", enabled);
    for created in &surroundings {
        let fuel = send(&mut plugin, &mut session, &spawned(created));
        cases.add("reload: entity spawned", fuel);
    }
    let first = tick(&mut plugin, &mut session);
    cases.add("first tick after the reload", first);
    for _ in 0..300 {
        let fuel = tick(&mut plugin, &mut session);
        cases.add("idle tick after the reload", fuel);
    }
    let died = send(
        &mut plugin,
        &mut session,
        &Event::LocalPlayerDied("You have been slain by a Mosswart Kin!".to_owned()),
    );
    cases.add("event: died", died);
    let logoff = send(&mut plugin, &mut session, &Event::Logoff);
    cases.add("event: logoff", logoff);

    let cases = cases.all();
    let mut report = report(&cases);
    let _ = writeln!(
        report,
        "corpses opened: {}; casts: {}",
        opened.len(),
        session.world.cast_spell_ids().len()
    );
    println!("{report}");
    assert!(
        cases
            .iter()
            .filter(|case| case.name.starts_with("buff") || case.name.starts_with("combat"))
            .count()
            == 2,
        "the macro made no buff or combat decision:\n{report}"
    );
    assert!(!opened.is_empty(), "no corpse was opened:\n{report}");
    assert_within_margin(&cases, &report);
}

/// A vendor among the objects about (after the players and monsters).
const NEARBY_VENDOR: usize = heavy::NEARBY_PLAYERS + heavy::NEARBY_MONSTERS + 2;

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
            .map_or("<start>", |(name, _)| name.as_ref());
        let next = trace.get(*index).map_or("<end>", |(name, _)| name.as_ref());
        println!("    {used:>12} after #{index} {after} before {next}");
    }
}

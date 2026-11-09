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
//! generated one with its tables, row counts, value kinds and size. Three
//! cases: the heavy character's session (login
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
use mosstank_stores::storage::PluginStorage as _;
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

/// The inventory logger on the heavy character, every item fully
/// appraised: the login's records are made and written (with no log yet,
/// then again with the last session's log to read and merge), and logging
/// off writes the whole log in one call.
#[test]
#[ignore = "needs the release module: scripts/check.sh runs it after the WebAssembly build"]
fn the_inventory_log_fits() {
    let wasm = std::fs::read(wasm_path()).expect("the release mosstank.wasm (build it first)");
    let key = "Mag-Tools/Coldeve/Mossy Hunter.inventory.json";
    let mut session = new_session();
    let mut cases = Cases::default();
    for round in ["no log yet", "with the last log"] {
        let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("load");
        plugin.enable(&start_info(), &mut session).expect("enable");
        enter_world(&mut session);
        let properties = heavy::appraised_properties(&heavy::inventory());
        session.world.automation().properties.extend(properties);
        // The first session appraised everything; the next one logs in
        // with nothing appraised yet, so every record is the last log's
        // appraisal merged with the live item.
        let appraised = round == "no log yet";
        for object in &mut session.world.automation().world_objects {
            if object.is_owned {
                object.has_appraisal_data = appraised;
            }
        }
        session
            .world
            .set_reply::<calls::ItemsInventoryComplete>(&true);
        tick(&mut plugin, &mut session);
        let before = session.world.plugin_data.get(key).map(str::to_owned);
        let login = send(&mut plugin, &mut session, &Event::LoginComplete);
        cases.add("inventory log: login complete", login);
        for _ in 0..60 {
            let fuel = tick(&mut plugin, &mut session);
            cases.add(&format!("inventory log: tick ({round})"), fuel);
        }
        let written = session.world.plugin_data.get(key).map(str::to_owned);
        assert!(
            written.is_some() && written != before,
            "{round}: no log written"
        );
        if round == "with the last log" {
            // Every item changed in the tick of the logoff: all recorded
            // again before the write.
            every_owned_item_moved(&mut plugin, &mut session, &mut cases, "inventory log");
        }
        let logoff = send(&mut plugin, &mut session, &Event::Logoff);
        cases.add(&format!("inventory log: logoff ({round})"), logoff);
        plugin.disable(&mut session).expect("disable");
    }
    let log = session.world.plugin_data.get(key).expect("the log");
    let items = log.matches("\n{\"id\":").count();
    let kept_appraisals = log.matches("\"has_id_data\":true").count();
    let cases = cases.all();
    let mut report = report(&cases);
    let _ = writeln!(report, "log: {} KB, {items} items", log.len() / 1024);
    println!("{report}");
    assert_eq!(items, heavy::INVENTORY_ITEMS, "{report}");
    assert_eq!(kept_appraisals, heavy::INVENTORY_ITEMS, "{report}");
    assert_no_file_rewritten(&session, &report);
    assert_within_margin(&cases, &report);
}

/// Every owned item moves (a pack sorted), each change its own event,
/// with no tick between: the inventory log has them all to record again.
fn every_owned_item_moved(
    plugin: &mut Plugin,
    session: &mut Session,
    cases: &mut Cases,
    case: &str,
) {
    let owned: Vec<_> = session
        .world
        .automation()
        .world_objects
        .iter()
        .filter(|object| object.is_owned)
        .cloned()
        .collect();
    for (revision, object) in owned.iter().enumerate() {
        let moved = Event::ObjectChanged(ObjectChange {
            object_id: object.object_id,
            kind: ObjectChangeKind::Moved,
            revision: 50_000 + revision as i64,
            changed_fields: ObjectChangeKind::Moved.fields(),
            current: Some(object.clone()),
        });
        let fuel = send(plugin, session, &moved);
        cases.add(&format!("{case}: item moved"), fuel);
    }
}
/// The text the Inventory window shows in its binding `name` (the client's
/// own value type, read through its debug form: `Text("...")`).
fn inventory_text(session: &Session, name: &str) -> String {
    session
        .world
        .fake
        .ui
        .panels()
        .find(|panel| panel.descriptor.window_id == "inventory")
        .and_then(|panel| panel.values.get(name))
        .map(|value| {
            let debug = format!("{value:?}");
            debug
                .strip_prefix("Text(\"")
                .and_then(|text| text.strip_suffix("\")"))
                .map_or_else(|| debug.clone(), str::to_owned)
        })
        .unwrap_or_default()
}

fn inventory_status(session: &Session) -> String {
    inventory_text(session, "MagInventoryStatus")
}

/// The heavy character enters the world, every item appraised, and its
/// inventory is logged.
fn log_in_heavy(plugin: &mut Plugin, session: &mut Session) {
    enter_world(session);
    let properties = heavy::appraised_properties(&heavy::inventory());
    session.world.automation().properties.extend(properties);
    session
        .world
        .set_reply::<calls::ItemsInventoryComplete>(&true);
    tick(plugin, session);
    send(plugin, session, &Event::LoginComplete);
    for _ in 0..60 {
        tick(plugin, session);
    }
}

/// The Inventory window on the heavy character with three other characters'
/// saved logs of as many items (6,000 items in all): opened, its files read
/// and every item's text made, then a search typed a key at a time, and a
/// row clicked.
#[test]
#[ignore = "needs the release module: scripts/check.sh runs it after the WebAssembly build"]
fn the_inventory_search_fits() {
    use ac_plugin_abi::ui::ActionValue;
    let wasm = std::fs::read(wasm_path()).expect("the release mosstank.wasm (build it first)");
    let mut session = new_session();
    let mut cases = Cases::default();

    // A first session logs the heavy character; three alts' logs are copies.
    let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("load");
    plugin.enable(&start_info(), &mut session).expect("enable");
    log_in_heavy(&mut plugin, &mut session);
    send(&mut plugin, &mut session, &Event::Logoff);
    plugin.disable(&mut session).expect("disable");
    let log = session
        .world
        .plugin_data
        .get("Mag-Tools/Coldeve/Mossy Hunter.inventory.json")
        .expect("the heavy character's log")
        .to_owned();
    for alt in 1..=3 {
        let name = format!("Alt {alt}");
        let text = log.replacen(
            "\"character\":\"Mossy Hunter\"",
            &format!("\"character\":\"{name}\""),
            1,
        );
        session
            .world
            .plugin_data
            .insert(&format!("Mag-Tools/Coldeve/{name}.inventory.json"), &text);
    }

    // The next session: the window opened once the login's log is written.
    let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("load");
    plugin.enable(&start_info(), &mut session).expect("enable");
    log_in_heavy(&mut plugin, &mut session);
    let open = ui_action(&session, "OpenMagInventory", ActionValue::None);
    let fuel = send(&mut plugin, &mut session, &open);
    cases.add("inventory search: open the window", fuel);
    let total = heavy::INVENTORY_ITEMS * 4;
    let all = format!("{total} items");
    let mut ticks = 0;
    while inventory_status(&session) != all {
        let fuel = tick(&mut plugin, &mut session);
        cases.add("inventory search: tick (reading)", fuel);
        ticks += 1;
        assert!(ticks < 2000, "never read: {}", inventory_status(&session));
    }
    let read_ticks = ticks;

    // A search typed a key at a time, each key matched against every text,
    // then patterns only .NET's syntax has (look-arounds), one that is
    // quadratic in the text, one whose engine builds large tables, two that
    // backtrack without end (each text stopped at the step limit, and said
    // so), and a look-behind again; then a row clicked.
    let realistic = [
        "S",
        "Sw",
        "Swo",
        "Pack|Sack",
        "(?<!x)Pack",
        "(?=.*Sword)(?=.*Fire)",
        "e.{25}x",
    ];
    let pathological = [r"(\w+\s?)+\1", r"(?:\w+\s?)+Q(?!z)"];
    // Patterns that compile huge: refused, and quickly.
    let too_large = [r"\w{1000}", r"\w{0,50}Ring", r"(?<!\w{0,400})Ring"];
    // Refused natively (64-bit), but under the size limit in the 32-bit
    // module, where it compiles and is searched with.
    let near_the_limit = [r"(\w+) (\w+) (\w+)"];
    // A long plain pattern pasted in.
    let pasted = "Superb Gold Ring of the Frozen North ".repeat(6)[..200].to_owned();
    let pasted = [pasted.as_str()];
    let mut matching_ticks = 0;
    for pattern in realistic
        .iter()
        .chain(&pasted)
        .chain(&pathological)
        .chain(&too_large)
        .chain(&near_the_limit)
        .chain(&["(?<!x)Pack"])
    {
        let pattern = *pattern;
        let typed = ui_action(
            &session,
            "SetMagInventorySearchText",
            ActionValue::Text(pattern.to_owned()),
        );
        let fuel = send(&mut plugin, &mut session, &typed);
        cases.add(&format!("inventory search: typed {pattern}"), fuel);
        let mut ticks = 0;
        while inventory_status(&session).starts_with("Searching") || ticks < 10 {
            let fuel = tick(&mut plugin, &mut session);
            cases.add(&format!("inventory search: matching {pattern}"), fuel);
            ticks += 1;
            assert!(ticks < 1000, "{pattern}: {}", inventory_status(&session));
        }
        matching_ticks = matching_ticks.max(ticks);
        let status = inventory_status(&session);
        if too_large.contains(&pattern) {
            assert_eq!(status, "pattern too large", "{pattern}");
        } else if pathological.contains(&pattern) {
            assert!(
                status.contains("too costly to search"),
                "{pattern}: {status}"
            );
        } else {
            assert!(
                status.ends_with(" items") || status == "1 item",
                "{pattern}: {status}"
            );
        }
    }
    let click = ui_action(&session, "SelectMagInventoryRow", ActionValue::Int(0));
    let fuel = send(&mut plugin, &mut session, &click);
    cases.add("inventory search: a row clicked", fuel);
    let info = inventory_text(&session, "MagInventoryInfo");
    assert!(info.starts_with("Pack"), "{info}");

    let cases = cases.all();
    let mut report = report(&cases);
    let _ = writeln!(
        report,
        "{total} items read in {read_ticks} ticks; a key's matching took up to {matching_ticks} ticks"
    );
    println!("{report}");
    assert_no_file_rewritten(&session, &report);
    assert_within_margin(&cases, &report);
}

/// One click or entry on the plugin's windows, measured as `case`.
fn act(
    plugin: &mut Plugin,
    session: &mut Session,
    cases: &mut Cases,
    case: &str,
    handler: &str,
    value: ac_plugin_abi::ui::ActionValue,
) {
    let action = ui_action(session, handler, value);
    let fuel = send(plugin, session, &action);
    cases.add(case, fuel);
}

/// Ticks until the Inventory window's rows are worked out again for the
/// filter (its 0.4 s wait, then the rows tested a tick's worth at a time),
/// each tick measured as `case`; the ticks taken.
fn settle_inventory(
    plugin: &mut Plugin,
    session: &mut Session,
    cases: &mut Cases,
    case: &str,
) -> usize {
    let mut ticks = 0;
    while inventory_status(session).starts_with("Searching") || ticks < 40 {
        let fuel = tick(plugin, session);
        cases.add(case, fuel);
        ticks += 1;
        assert!(ticks < 1000, "{case}: {}", inventory_status(session));
    }
    ticks
}

/// The Inventory window on the heavy character with three other
/// characters' saved logs (6,000 items in all, the saved ones with a set,
/// an armor level and two ratings), the filters window shown and every
/// filter set through it (the Item tab's choices and slots, cantrips at
/// mixed tiers and a spell text, every rating and the crit damage tiers, a
/// set, the requirements, a character left out), then one filter changed.
/// A header click sorts in the client (the list's `sort` and `sortkeys`,
/// no `onsort`): it never calls the plugin, so it has no case here.
#[test]
#[ignore = "needs the release module: scripts/check.sh runs it after the WebAssembly build"]
fn the_inventory_filters_fit() {
    use ac_plugin_abi::ui::ActionValue;
    let wasm = std::fs::read(wasm_path()).expect("the release mosstank.wasm (build it first)");
    let mut session = new_session();
    let mut cases = Cases::default();

    // A first session logs the heavy character; three alts' logs are
    // copies whose appraised items carry a set and some ratings.
    let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("load");
    plugin.enable(&start_info(), &mut session).expect("enable");
    log_in_heavy(&mut plugin, &mut session);
    send(&mut plugin, &mut session, &Event::Logoff);
    plugin.disable(&mut session).expect("disable");
    let log = session
        .world
        .plugin_data
        .get("Mag-Tools/Coldeve/Mossy Hunter.inventory.json")
        .expect("the heavy character's log")
        .to_owned();
    let appraised = "\"ints\":{\"1\":";
    assert!(log.contains(appraised), "the log's appraised items");
    for alt in 1..=3 {
        let name = format!("Alt {alt}");
        let text = log
            .replacen(
                "\"character\":\"Mossy Hunter\"",
                &format!("\"character\":\"{name}\""),
                1,
            )
            .replace(
                appraised,
                &format!(
                    "\"ints\":{{\"265\":{},\"28\":300,\"374\":2,\"315\":3,\"1\":",
                    12 + alt
                ),
            );
        session
            .world
            .plugin_data
            .insert(&format!("Mag-Tools/Coldeve/{name}.inventory.json"), &text);
    }

    // The next session: the Inventory window opened and every item read.
    let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("load");
    plugin.enable(&start_info(), &mut session).expect("enable");
    log_in_heavy(&mut plugin, &mut session);
    let open = ui_action(&session, "OpenMagInventory", ActionValue::None);
    send(&mut plugin, &mut session, &open);
    let total = heavy::INVENTORY_ITEMS * 4;
    let all = format!("{total} items");
    let mut ticks = 0;
    while inventory_status(&session) != all {
        tick(&mut plugin, &mut session);
        ticks += 1;
        assert!(ticks < 2000, "never read: {}", inventory_status(&session));
    }

    // The filters window, every filter set through it.
    let p = &mut plugin;
    let s = &mut session;
    let c = &mut cases;
    let on = || ActionValue::Int(1);
    let text = |t: &str| ActionValue::Text(t.to_owned());
    act(
        p,
        s,
        c,
        "filters: open the window",
        "ToggleMagInventoryFilters",
        ActionValue::None,
    );
    let set = "filters: a filter set";
    let tab = "filters: a tab shown";
    act(p, s, c, tab, "ShowMagFiltersItem", ActionValue::None);
    act(p, s, c, set, "SetMagFilterEquipped", ActionValue::Int(2));
    act(p, s, c, set, "SetMagFilterBonded", on());
    act(p, s, c, set, "SetMagFilterAttuned", on());
    act(p, s, c, set, "SetMagFilterRare", on());
    act(p, s, c, set, "SetMagFilterType", ActionValue::Int(1));
    for slot in ["Hands", "Ring", "Neck"] {
        act(p, s, c, set, &format!("SetMagFilterSlot{slot}"), on());
    }
    act(p, s, c, tab, "ShowMagFiltersCantrips", ActionValue::None);
    act(p, s, c, set, "SetMagFilterCantripFind", text("a"));
    act(p, s, c, set, "SetMagFilterCantripFind", text(""));
    // Three cantrips (row 0 is the Attributes heading).
    for row in [1, 5, 40] {
        act(
            p,
            s,
            c,
            set,
            "ToggleMagFilterCantrip",
            ActionValue::Int(row),
        );
    }
    // Mixed tiers: one cycled once, one twice, one left Legendary.
    for row in [5, 40, 40] {
        act(
            p,
            s,
            c,
            set,
            "CycleMagFilterCantripTier",
            ActionValue::Int(row),
        );
    }
    act(p, s, c, set, "SetMagFilterSpellContains", text("strength"));
    act(p, s, c, tab, "ShowMagFiltersRatings", ActionValue::None);
    for rating in [
        "DamageRating",
        "CritDamage",
        "HealBoost",
        "Vitality",
        "ArmorLevel",
        "DamageResist",
        "CritResist",
        "CritDamageResist",
        "HealingResist",
        "NetherResist",
        "DotResist",
        "LifeResist",
        "SneakAttack",
        "Recklessness",
        "Deception",
        "PkDamage",
        "PkDamageResist",
        "GearPkDamage",
        "GearPkResist",
        "Tinks",
    ] {
        act(
            p,
            s,
            c,
            set,
            &format!("SetMagFilterRating{rating}"),
            text("1"),
        );
    }
    act(p, s, c, set, "SetMagFilterCd2", ActionValue::Int(0));
    act(p, s, c, tab, "ShowMagFiltersSet", ActionValue::None);
    act(p, s, c, set, "SetMagFilterSetFind", text("set"));
    act(p, s, c, set, "SetMagFilterSetFind", text(""));
    // Row 0 is Any set; 1 the set with the most items.
    act(p, s, c, set, "SelectMagFilterSetRow", ActionValue::Int(1));
    act(p, s, c, tab, "ShowMagFiltersReqs", ActionValue::None);
    for (req, value) in [
        ("MaxWield", "275"),
        ("MinValue", "100"),
        ("MinWorkmanship", "1"),
        ("MaxBurden", "500"),
    ] {
        act(p, s, c, set, &format!("SetMagFilterReq{req}"), text(value));
    }
    act(p, s, c, tab, "ShowMagFiltersCharacters", ActionValue::None);
    act(
        p,
        s,
        c,
        set,
        "ToggleMagFilterCharacter",
        ActionValue::Int(3),
    );
    let summary = inventory_text(s, "MagInventoryFilterSummary");
    for part in [
        "Not equipped",
        "Bonded",
        "Armor",
        "Hands",
        "Crit damage ≥ 1",
        "Wield ≤ 275",
    ] {
        assert!(summary.contains(part), "{part}: {summary}");
    }
    assert!(summary.contains("'s Set"), "a set chosen: {summary}");
    let every_ticks = settle_inventory(p, s, c, "filters: tick (every filter)");
    // Attuned and Rare cleared (no item is either), then Bonded below.
    act(p, s, c, tab, "ShowMagFiltersItem", ActionValue::None);
    act(p, s, c, set, "SetMagFilterAttuned", ActionValue::Int(0));
    act(p, s, c, set, "SetMagFilterRare", ActionValue::Int(0));
    settle_inventory(p, s, c, "filters: tick (every filter)");

    // One filter changed: the Bonded box cleared, so the armor reaches the
    // appraisal's filters; the Cantrips tab shown while the rows are
    // tested.
    act(p, s, c, tab, "ShowMagFiltersCantrips", ActionValue::None);
    act(
        p,
        s,
        c,
        "filters: one filter changed",
        "SetMagFilterBonded",
        ActionValue::Int(0),
    );
    let changed_ticks = settle_inventory(p, s, c, "filters: tick (one filter changed)");
    let status = inventory_status(s);
    assert!(
        status.ends_with(" items")
            || status.ends_with(" item")
            || status.contains("not identified"),
        "{status}"
    );

    let cases = cases.all();
    let mut report = report(&cases);
    let _ = writeln!(
        report,
        "{total} items; every filter settled in {every_ticks} ticks, one change in {changed_ticks}; {status}"
    );
    println!("{report}");
    assert_no_file_rewritten(&session, &report);
    assert_within_margin(&cases, &report);
}

/// The OD ladders the Weapon OD fuel case loads: 31 of 10 rules each,
/// shaped like real ones (`(H) Axe (OD +10)` ... `(M) Xbow (OD +1)`):
/// mastery (353), object class, a name gate on every other melee ladder,
/// the damage step (buffed median damage for melee, buffed missile damage
/// for missile), wield skill (159). 28 melee ladders (masteries 1 to 7,
/// each with the four melee skills) and 3 missile ones. The heavy
/// character's weapons are given mastery 2 and skill 45, so a melee weapon
/// reads the damage step of every mastery-2 ladder (the skill is read
/// last); only the light one is reachable (`40 + N + raise`: OD 8 with
/// `raise` 0, 7 with 1), the others ask more than any weapon has.
fn od_ladders(raise: i32) -> String {
    use mosstank_formats::utl::{self, LootAction, LootRule, VtankLootRequirement};
    fn requirement(kind: i32, lines: &[&str]) -> VtankLootRequirement {
        VtankLootRequirement {
            kind,
            payload: lines.iter().map(|line| format!("{line}\r\n")).collect(),
        }
    }
    /// One ladder: `damage` is the damage requirement and its base (step
    /// N asks base + N).
    fn ladder(
        rules: &mut Vec<LootRule>,
        name: &str,
        (mastery, class, skill): (i32, i32, i32),
        gate: Option<&str>,
        (damage_kind, base): (i32, i32),
    ) {
        for n in (1..=10).rev() {
            let mut rule = LootRule::named(&format!("{name} (OD +{n})"), "*");
            rule.action = LootAction::Keep;
            let mut requirements = vec![
                requirement(12, &[&mastery.to_string(), "353"]),
                requirement(7, &[&class.to_string()]),
            ];
            if let Some(gate) = gate {
                requirements.push(requirement(1, &[gate, "1"]));
            }
            requirements.push(requirement(damage_kind, &[&(base + n).to_string()]));
            requirements.push(requirement(12, &[&skill.to_string(), "159"]));
            rule.vtank_requirements = requirements;
            rule.has_imported_requirements = true;
            rules.push(rule);
        }
    }
    let mut rules = Vec::new();
    let skills = [("H", 44), ("L", 45), ("F", 46), ("2H", 41)];
    for mastery in 1..=7 {
        for (index, (prefix, skill)) in skills.iter().enumerate() {
            let reachable = mastery == 2 && *skill == 45;
            let base = if reachable { 40 + raise } else { 500 };
            let gate = (index % 2 == 1).then_some("^.*(Tachi|Sword|Dagger|Axe).*$");
            ladder(
                &mut rules,
                &format!("({prefix}) Mastery {mastery}"),
                (mastery, 1, *skill),
                gate,
                (2000, base),
            );
        }
    }
    for (name, mastery) in [("(M) Bow", 8), ("(M) Xbow", 9), ("(M) TW", 10)] {
        ladder(&mut rules, name, (mastery, 9, 47), None, (2001, 500));
    }
    assert_eq!(rules.len(), 310);
    utl::serialize_rules(&rules)
}

/// A list the Inventory window shows in its binding `name`, through the
/// client's debug form of the value.
fn inventory_list_debug(session: &Session, name: &str) -> String {
    session
        .world
        .fake
        .ui
        .panels()
        .find(|panel| panel.descriptor.window_id == "inventory")
        .and_then(|panel| panel.values.get(name))
        .map(|value| format!("{value:?}"))
        .unwrap_or_default()
}

/// Ticks until the Inventory window starts reading every row again (it
/// looks at the loot profile once a second), then until it has read and
/// searched them all (at least 40 ticks in all), each tick measured as
/// `case`.
fn settle_reading(plugin: &mut Plugin, session: &mut Session, cases: &mut Cases, case: &str) {
    let mut ticks = 0;
    let mut reading = false;
    loop {
        let status = inventory_status(session);
        reading |= status.starts_with("Reading");
        let busy = status.starts_with("Searching") || status.starts_with("Reading");
        if reading && !busy && ticks >= 40 {
            break;
        }
        let fuel = tick(plugin, session);
        cases.add(case, fuel);
        ticks += 1;
        assert!(reading || ticks < 200, "{case}: never read again: {status}");
        assert!(ticks < 2000, "{case}: {}", inventory_status(session));
    }
}

/// The Weapon OD: a loot profile of 310 OD rules (31 ladders of 10) loaded,
/// then the Inventory window opened on the heavy character with three
/// other characters' saved logs (6,000 items, their weapons of mastery 2
/// and skill 45): every row read, its OD worked out; the Weapon OD filter
/// set and changed; the profile's OD rules changed, so every row is read
/// again.
#[test]
#[ignore = "needs the release module: scripts/check.sh runs it after the WebAssembly build"]
fn the_weapon_od_fits() {
    use ac_plugin_abi::ui::ActionValue;
    let wasm = std::fs::read(wasm_path()).expect("the release mosstank.wasm (build it first)");
    let mut session = new_session();
    let mut cases = Cases::default();

    // A first session logs the heavy character; three alts' logs are
    // copies whose appraised items carry the mastery and skill.
    let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("load");
    plugin.enable(&start_info(), &mut session).expect("enable");
    log_in_heavy(&mut plugin, &mut session);
    send(&mut plugin, &mut session, &Event::Logoff);
    plugin.disable(&mut session).expect("disable");
    let log = session
        .world
        .plugin_data
        .get("Mag-Tools/Coldeve/Mossy Hunter.inventory.json")
        .expect("the heavy character's log")
        .to_owned();
    let appraised = "\"ints\":{\"1\":";
    assert!(log.contains(appraised), "the log's appraised items");
    for alt in 1..=3 {
        let name = format!("Alt {alt}");
        let text = log
            .replacen(
                "\"character\":\"Mossy Hunter\"",
                &format!("\"character\":\"{name}\""),
                1,
            )
            .replace(appraised, "\"ints\":{\"353\":2,\"159\":45,\"1\":");
        session
            .world
            .plugin_data
            .insert(&format!("Mag-Tools/Coldeve/{name}.inventory.json"), &text);
    }

    // The next session: the ladders loaded, the live weapons of mastery 2
    // and skill 45 too.
    let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("load");
    plugin.enable(&start_info(), &mut session).expect("enable");
    log_in_heavy(&mut plugin, &mut session);
    for properties in session.world.automation().properties.values_mut() {
        properties.ints.extend([(353, 2), (159, 45)]);
    }
    session
        .world
        .profiles
        .insert("mosstank/loot/od-ladders.utl", &od_ladders(0));
    let fuel = send(
        &mut plugin,
        &mut session,
        &command("vt", "loot load od-ladders"),
    );
    cases.add("OD: the profile loaded", fuel);
    for _ in 0..300 {
        let fuel = tick(&mut plugin, &mut session);
        cases.add("OD: tick after the profile's load", fuel);
    }

    // The window opened: every row read, its OD worked out.
    let open = ui_action(&session, "OpenMagInventory", ActionValue::None);
    let fuel = send(&mut plugin, &mut session, &open);
    cases.add("OD: open the window", fuel);
    let total = heavy::INVENTORY_ITEMS * 4;
    let all = format!("{total} items");
    let mut read_ticks = 0;
    while inventory_status(&session) != all {
        let fuel = tick(&mut plugin, &mut session);
        cases.add("OD: tick (reading, each OD worked out)", fuel);
        read_ticks += 1;
        assert!(
            read_ticks < 2000,
            "never read: {}",
            inventory_status(&session)
        );
    }
    let ods = inventory_list_debug(&session, "MagInventoryRowOds");
    assert!(ods.contains("\"8\""), "the light ladder's OD 8: {ods}");

    // The Weapon OD filter set, then changed.
    let p = &mut plugin;
    let s = &mut session;
    let c = &mut cases;
    let text = |t: &str| ActionValue::Text(t.to_owned());
    act(
        p,
        s,
        c,
        "OD: open the filters",
        "ToggleMagInventoryFilters",
        ActionValue::None,
    );
    act(
        p,
        s,
        c,
        "OD: a tab shown",
        "ShowMagFiltersRatings",
        ActionValue::None,
    );
    act(p, s, c, "OD: the filter set", "SetMagFilterOd", text("8"));
    let filter_ticks = settle_inventory(p, s, c, "OD: tick (the filter set)");
    let at_8 = inventory_status(s);
    assert!(!at_8.starts_with("0 items"), "{at_8}");
    act(
        p,
        s,
        c,
        "OD: the filter changed",
        "SetMagFilterOd",
        text("9"),
    );
    let changed_ticks = settle_inventory(p, s, c, "OD: tick (the filter changed)");
    let at_9 = inventory_status(s);
    assert!(at_9.starts_with("0 items"), "{at_9}");

    // The profile's OD rules changed (the light ladder asks one more):
    // every row read again, its OD worked out again.
    s.world
        .profiles
        .insert("mosstank/loot/od-ladders.utl", &od_ladders(1));
    let fuel = send(p, s, &command("vt", "loot load od-ladders"));
    c.add("OD: the profile changed", fuel);
    act(
        p,
        s,
        c,
        "OD: the filter changed",
        "SetMagFilterOd",
        text("7"),
    );
    settle_reading(p, s, c, "OD: tick (read again, profile changed)");
    let at_7 = inventory_status(s);
    assert_eq!(at_7, at_8, "OD 7 now where OD 8 was");

    let cases = cases.all();
    let mut report = report(&cases);
    let _ = writeln!(
        report,
        "{total} items read in {read_ticks} ticks; the filter settled in {filter_ticks} ticks, \
         its change in {changed_ticks}; at OD 8: {at_8}; after the profile's change, at OD 7: \
         {at_7}"
    );
    println!("{report}");
    assert_no_file_rewritten(&session, &report);
    assert_within_margin(&cases, &report);
}

/// The pieces of gear the heavy character wears for the Mana page.
const WORN_MANA_ITEMS: usize = 20;

/// The heavy character puts on [`WORN_MANA_ITEMS`] of its carried gear
/// (each with mana and six spells), every spell of them cast on it by the
/// item (no end) beside its timed enchantments. The worn items' ids and
/// names.
fn wear_gear(session: &mut Session) -> Vec<(u32, String)> {
    let automation = session.world.automation();
    let mut worn = Vec::new();
    for item in &mut automation.item_entries {
        if worn.len() == WORN_MANA_ITEMS {
            break;
        }
        if item.valid_locations == 0 || item.item_maximum_mana == 0 {
            continue;
        }
        item.equipped_location = item.valid_locations;
        item.wielder_id = heavy::CHARACTER_ID;
        item.container_id = 0;
        worn.push((item.object_id, item.name.clone()));
    }
    // Each burns a mana every 40 s.
    for (object_id, _) in &worn {
        let properties = automation.properties.entry(*object_id).or_default();
        properties.floats.retain(|(id, _)| *id != 5);
        properties.floats.push((5, -0.025));
    }
    let mut casts = Vec::new();
    for object in &mut automation.world_objects {
        if worn.iter().any(|(id, _)| *id == object.object_id) {
            object.wielder_id = heavy::CHARACTER_ID;
            object.container_id = 0;
            object.has_appraisal_data = true;
            casts.extend(object.spell_ids.iter().copied());
        }
    }
    let mut every = heavy::enchantments(&session.table);
    every.extend(casts.into_iter().filter_map(|spell_id| {
        session
            .table
            .iter()
            .find(|spell| spell.spell_id == spell_id)
            .map(|spell| ac_plugin_abi::character::ActiveEnchantment {
                spell_id,
                family: spell.family,
                tier: spell.tier,
                seconds_remaining: f64::INFINITY,
            })
    }));
    session
        .world
        .set_reply::<calls::CharacterAllEnchantments>(&every);
    worn
}

/// The main window's list `name` (read through its debug form).
fn main_list_len(session: &Session, name: &str) -> usize {
    session
        .world
        .fake
        .ui
        .panels()
        .find(|panel| panel.descriptor.window_id == "main")
        .and_then(|panel| panel.values.get(name))
        .map_or(0, |value| format!("{value:?}").matches('"').count() / 2)
}

/// One line of a fight (three in four) amid the town's chat, heard now.
fn fight_or_town_line(
    plugin: &mut Plugin,
    session: &mut Session,
    line: usize,
    me: &str,
) -> CallFuel {
    let log = session.world.chat_log.get_or_insert_with(Vec::new);
    let sequence = log.last().map_or(1, |line| line.sequence + 1);
    let event = if line % 4 == 3 {
        heavy::chat_line(sequence, session.world.fake.clock.utc_unix_ms)
    } else {
        heavy::fight_line(sequence, session.world.fake.clock.utc_unix_ms, me)
    };
    log.push(event.clone());
    send(plugin, session, &Event::ChatReceived(event))
}

/// The combat tracker on the heavy character with a long life's persistent
/// stats of `monsters` monsters: its patterns compiled a few a tick, the
/// stats file read in at the login, a burst of
/// [`heavy::BURST_CHAT_LINES`] fight lines in one tick amid the town's
/// chat, and the logoff that writes the persistent stats and exports the
/// current ones. `catching_up`: the login completes at once and a fight's
/// worth of lines is heard while the patterns compile, so the lines that
/// waited are read while the file is read in. `shown`: the Combat tab
/// whose handler is named is shown from before the login on (its lists
/// built as the file is read in, and drawn through the burst).
fn combat_session(monsters: usize, catching_up: bool, shown: Option<&str>) {
    use ac_plugin_abi::ui::ActionValue;
    let wasm = std::fs::read(wasm_path()).expect("the release mosstank.wasm (build it first)");
    let me = "Mossy Hunter";
    let key = "Mag-Tools/Coldeve/Mossy Hunter.CombatTracker.xml";
    let mut session = new_session();
    let saved = heavy::persistent_combat_stats(me, monsters);
    session.world.plugin_data.insert(key, &saved);
    let mut cases = Cases::default();
    let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("load");
    plugin.enable(&start_info(), &mut session).expect("enable");
    let set = command("ub", "opt set Mag.CombatTracker.ExportOnLogOff true");
    send(&mut plugin, &mut session, &set);
    enter_world(&mut session);
    session
        .world
        .set_reply::<calls::ItemsInventoryComplete>(&true);
    if let Some(tab) = shown {
        for handler in ["ShowMag", "ShowMagCombat", tab] {
            let action = ui_action(&session, handler, ActionValue::None);
            let fuel = send(&mut plugin, &mut session, &action);
            cases.add("combat: page shown", fuel);
        }
    }
    if catching_up {
        tick(&mut plugin, &mut session);
        let login = send(&mut plugin, &mut session, &Event::LoginComplete);
        cases.add("combat: login complete", login);
        for line in 0..2 * heavy::BURST_CHAT_LINES {
            let fuel = fight_or_town_line(&mut plugin, &mut session, line, me);
            cases.add("combat: chat line (patterns compiling)", fuel);
        }
        for _ in 0..300 {
            let fuel = tick(&mut plugin, &mut session);
            cases.add(
                "combat: tick (compiling, lines waiting, file read in)",
                fuel,
            );
        }
    } else {
        // The town's chat while the patterns compile: it waits for them.
        for _ in 0..20 {
            chat(
                &mut plugin,
                &mut session,
                &mut cases,
                "combat: chat line (patterns compiling)",
            );
            let fuel = tick(&mut plugin, &mut session);
            cases.add("combat: tick (patterns compiling, login)", fuel);
        }
        let login = send(&mut plugin, &mut session, &Event::LoginComplete);
        cases.add("combat: login complete", login);
        for _ in 0..200 {
            let fuel = tick(&mut plugin, &mut session);
            cases.add("combat: tick (persistent stats read in)", fuel);
        }
    }

    for line in 0..heavy::BURST_CHAT_LINES {
        let fuel = fight_or_town_line(&mut plugin, &mut session, line, me);
        cases.add("combat: chat line (burst)", fuel);
    }
    for _ in 0..5 {
        let fuel = tick(&mut plugin, &mut session);
        cases.add("combat: tick (after the burst)", fuel);
    }
    if shown.is_some() {
        // A row clicked, the next tick drawn with its details.
        let click = ui_action(&session, "SelectMagCombatRow", ActionValue::Int(2));
        let fuel = send(&mut plugin, &mut session, &click);
        cases.add("combat: monster row clicked", fuel);
        let fuel = tick(&mut plugin, &mut session);
        cases.add("combat: tick (after the burst)", fuel);
    }
    let drawn = main_list_len(&session, "MagCombatNames");
    let warnings = session
        .world
        .chat()
        .iter()
        .filter(|text| text.contains("Unable to parse damage element"))
        .count();

    let logoff = send(&mut plugin, &mut session, &Event::Logoff);
    cases.add("combat: logoff (both files written)", logoff);
    let written = session
        .world
        .plugin_data
        .get(key)
        .unwrap_or_default()
        .to_owned();
    let exports = session
        .world
        .plugin_data
        .list("Mag-Tools/Coldeve/")
        .unwrap_or_default()
        .into_iter()
        .filter(|name| name.contains(".CombatTracker.2"))
        .count();
    let cases = cases.all();
    let mut report = report(&cases);
    let _ = writeln!(
        report,
        "persistent stats: {monsters} monsters, {} KB read, {} KB written; {warnings} warnings posted",
        saved.len() / 1024,
        written.len() / 1024
    );
    println!("{report}");
    // The file read in, the fight added to it, written back.
    let last_monster = format!("TargetName=\"Drudge Skulker {}\"", monsters / 10 - 1);
    assert!(written.contains(&last_monster), "{report}");
    assert!(written.len() > saved.len(), "{report}");
    assert_eq!(exports, 1, "{report}");
    assert!(warnings > 0, "{report}");
    // The tab's list drawn: the header, All, a row a monster.
    match shown {
        Some("ShowMagCombatPersistent") => assert_eq!(drawn, monsters + 2, "{report}"),
        Some(_) => assert!(drawn > 2, "{report}"),
        None => {}
    }
    assert_no_file_rewritten(&session, &report);
    assert_within_margin(&cases, &report);
}

/// [`combat_session`] with [`heavy::PERSISTENT_MONSTERS`] monsters.
#[test]
#[ignore = "needs the release module: scripts/check.sh runs it after the WebAssembly build"]
fn the_combat_tracker_fits() {
    combat_session(heavy::PERSISTENT_MONSTERS, false, None);
}

/// [`combat_session`] with three times as many monsters: the logoff only
/// joins the stats' text and writes it.
#[test]
#[ignore = "needs the release module: scripts/check.sh runs it after the WebAssembly build"]
fn the_combat_tracker_with_three_thousand_monsters_fits() {
    combat_session(3 * heavy::PERSISTENT_MONSTERS, false, None);
}

/// [`combat_session`] catching up: the lines that waited for the patterns
/// read while the persistent file is read in.
#[test]
#[ignore = "needs the release module: scripts/check.sh runs it after the WebAssembly build"]
fn the_combat_tracker_catching_up_fits() {
    combat_session(heavy::PERSISTENT_MONSTERS, true, None);
}

/// [`combat_session`] with three thousand monsters and the Persistent
/// Stats tab shown from before the login: its lists built as the file is
/// read in, drawn through the burst, a row clicked.
#[test]
#[ignore = "needs the release module: scripts/check.sh runs it after the WebAssembly build"]
fn the_combat_page_shown_with_three_thousand_monsters_fits() {
    combat_session(
        3 * heavy::PERSISTENT_MONSTERS,
        false,
        Some("ShowMagCombatPersistent"),
    );
}

/// [`combat_session`] with the Current Session Stats tab shown through
/// the burst.
#[test]
#[ignore = "needs the release module: scripts/check.sh runs it after the WebAssembly build"]
fn the_combat_page_shown_during_the_burst_fits() {
    combat_session(
        heavy::PERSISTENT_MONSTERS,
        false,
        Some("ShowMagCombatCurrent"),
    );
}

/// The Mana page and its overlay on the heavy character wearing twenty
/// pieces of gear with mana: the login (each piece tracked and asked for),
/// the overlay drawn and painted again every tick, dragged, then beside
/// the page as the burn ticks, every piece appraised, a burst of town chat
/// with the system lines that cue an appraisal, and a piece taken off and
/// put back on.
#[test]
#[ignore = "needs the release module: scripts/check.sh runs it after the WebAssembly build"]
fn the_mana_page_fits() {
    use ac_plugin_abi::items::EquipmentObservation;
    use ac_plugin_abi::ui::ActionValue;
    let wasm = std::fs::read(wasm_path()).expect("the release mosstank.wasm (build it first)");
    let mut session = new_session();
    let mut cases = Cases::default();
    let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("load");
    plugin.enable(&start_info(), &mut session).expect("enable");
    enter_world(&mut session);
    let properties = heavy::appraised_properties(&heavy::inventory());
    session.world.automation().properties.extend(properties);
    let worn = wear_gear(&mut session);
    assert_eq!(worn.len(), WORN_MANA_ITEMS);
    session
        .world
        .set_reply::<calls::ItemsInventoryComplete>(&true);
    tick(&mut plugin, &mut session);
    let login = send(&mut plugin, &mut session, &Event::LoginComplete);
    cases.add("mana page: login complete", login);
    for _ in 0..60 {
        let fuel = tick(&mut plugin, &mut session);
        cases.add("mana page: tick (page hidden)", fuel);
    }

    // Every piece appraised once (an unappraised one has burned all its
    // mana and stays at 0), then the overlay ticked on: drawn alone, every
    // tick a burn later (each row's mana changes, so each tick paints),
    // dragged, then beside the page for the rest of the run.
    let worn_objects: Vec<_> = session
        .world
        .automation()
        .world_objects
        .iter()
        .filter(|object| worn.iter().any(|(id, _)| *id == object.object_id))
        .cloned()
        .collect();
    for (revision, object) in worn_objects.iter().enumerate() {
        let appraised = Event::ObjectChanged(ObjectChange {
            object_id: object.object_id,
            kind: ObjectChangeKind::IdentReceived,
            revision: 5_000 + revision as i64,
            changed_fields: ObjectChangeKind::IdentReceived.fields(),
            current: Some(object.clone()),
        });
        let fuel = send(&mut plugin, &mut session, &appraised);
        cases.add("mana page: worn item appraised", fuel);
    }
    let overlay_on = ui_action(&session, "SetMagManaShowOverlay", ActionValue::Int(1));
    let fuel = send(&mut plugin, &mut session, &overlay_on);
    cases.add("mana overlay: ticked on", fuel);
    for _ in 0..60 {
        session.world.fake.clock.monotonic_ms += 40_000;
        let fuel = tick(&mut plugin, &mut session);
        cases.add("mana overlay: tick (repainted)", fuel);
    }
    let overlay = |session: &Session| {
        session
            .world
            .fake
            .ui
            .canvases()
            .filter(|canvas| canvas.descriptor.canvas_id == mosstank_mag::mana_overlay::CANVAS_ID)
            .last()
            .map(|canvas| {
                (
                    canvas.id,
                    canvas.visible,
                    canvas.descriptor.height,
                    canvas.offset,
                )
            })
            .expect("the overlay's canvas")
    };
    let painted = |session: &Session| {
        session
            .world
            .fake
            .ui
            .canvases()
            .filter(|canvas| canvas.descriptor.canvas_id == mosstank_mag::mana_overlay::CANVAS_ID)
            .last()
            .map(|canvas| format!("{:?}", canvas.paint))
            .unwrap_or_default()
    };
    let before = painted(&session);
    session.world.fake.clock.monotonic_ms += 40_000;
    let fuel = tick(&mut plugin, &mut session);
    cases.add("mana overlay: tick (repainted)", fuel);
    assert_ne!(painted(&session), before, "a burn later, painted again");
    let (canvas, visible, height, from) = overlay(&session);
    assert!(visible, "the overlay is drawn");
    assert_eq!(
        height,
        mosstank_mag::mana_overlay::height_for(WORN_MANA_ITEMS),
        "a row per worn item"
    );
    let press = |kind: u8, x: f32, y: f32| {
        Event::Ui(ac_plugin_abi::ui::UiEvent::Pointer(
            ac_plugin_abi::ui::PointerEvent {
                canvas,
                kind,
                x,
                y,
                button: 1,
                modifiers: 0,
                wheel_delta: 0,
            },
        ))
    };
    use ac_plugin_abi::ui::PointerEvent as P;
    let fuel = send(&mut plugin, &mut session, &press(P::DOWN, 40.0, 5.0));
    cases.add("mana overlay: drag", fuel);
    for _ in 0..20 {
        let fuel = send(&mut plugin, &mut session, &press(P::MOVE, 43.0, 7.0));
        cases.add("mana overlay: drag", fuel);
    }
    let fuel = send(&mut plugin, &mut session, &press(P::UP, 40.0, 5.0));
    cases.add("mana overlay: drag", fuel);
    let (_, _, _, to) = overlay(&session);
    assert_eq!((to.0 - from.0, to.1 - from.1), (60.0, 40.0), "dragged");
    for _ in 0..10 {
        let fuel = tick(&mut plugin, &mut session);
        cases.add("mana overlay: tick (repainted)", fuel);
    }

    for handler in ["ShowMag", "ShowMagMana"] {
        let shown = ui_action(&session, handler, ActionValue::None);
        let fuel = send(&mut plugin, &mut session, &shown);
        cases.add("mana page: shown", fuel);
    }
    for _ in 0..60 {
        let fuel = tick(&mut plugin, &mut session);
        cases.add("mana page: tick (page shown)", fuel);
    }
    assert_eq!(
        main_list_len(&session, "MagManaNames"),
        WORN_MANA_ITEMS,
        "the rows drawn"
    );

    let objects: Vec<_> = session
        .world
        .automation()
        .world_objects
        .iter()
        .filter(|object| worn.iter().any(|(id, _)| *id == object.object_id))
        .cloned()
        .collect();
    for (revision, object) in objects.iter().enumerate() {
        let appraised = Event::ObjectChanged(ObjectChange {
            object_id: object.object_id,
            kind: ObjectChangeKind::IdentReceived,
            revision: 10_000 + revision as i64,
            changed_fields: ObjectChangeKind::IdentReceived.fields(),
            current: Some(object.clone()),
        });
        let fuel = send(&mut plugin, &mut session, &appraised);
        cases.add("mana page: worn item appraised", fuel);
        let fuel = tick(&mut plugin, &mut session);
        cases.add("mana page: tick (page shown)", fuel);
    }

    for line in 0..heavy::BURST_CHAT_LINES {
        if line % 8 == 3 {
            let name = &worn[line % worn.len()].1;
            let log = session.world.chat_log.get_or_insert_with(Vec::new);
            let sequence = log.last().map_or(1, |line| line.sequence + 1);
            let mut low = heavy::chat_line(sequence, session.world.fake.clock.utc_unix_ms);
            low.kind = 4;
            low.text = format!("Your {name} is low on Mana.");
            log.push(low.clone());
            let fuel = send(&mut plugin, &mut session, &Event::ChatReceived(low));
            cases.add("mana page: chat line", fuel);
        } else {
            chat(
                &mut plugin,
                &mut session,
                &mut cases,
                "mana page: chat line",
            );
        }
        if line % 4 == 0 {
            let fuel = tick(&mut plugin, &mut session);
            cases.add("mana page: tick (page shown)", fuel);
        }
    }

    let (piece, _) = worn[0].clone();
    for is_removal in [true, false] {
        let placed = Event::EquipmentPlacement(EquipmentObservation {
            object_id: piece,
            equipped_location: 0x0004_0000,
            is_removal,
            is_initial_placement: false,
        });
        let fuel = send(&mut plugin, &mut session, &placed);
        cases.add("mana page: placement", fuel);
        let fuel = tick(&mut plugin, &mut session);
        cases.add("mana page: tick (page shown)", fuel);
    }
    assert_eq!(main_list_len(&session, "MagManaNames"), WORN_MANA_ITEMS);

    let cases = cases.all();
    let report = report(&cases);
    println!("{report}");
    assert_no_file_rewritten(&session, &report);
    assert_within_margin(&cases, &report);
}

/// How many items the Comps HUD case lists.
const COMPS_HUD_ITEMS: i32 = 12;

/// The Comps HUD on the heavy character: shown, twelve carried items ticked
/// on in Edit items, recounted every tick (each recount takes the whole
/// inventory), the pointer over it (repainted with its buttons) and off it,
/// and a drag.
#[test]
#[ignore = "needs the release module: scripts/check.sh runs it after the WebAssembly build"]
fn the_comps_hud_fits() {
    use ac_plugin_abi::ui::{ActionValue, PointerEvent as P, UiEvent};
    let wasm = std::fs::read(wasm_path()).expect("the release mosstank.wasm (build it first)");
    let mut session = new_session();
    let mut cases = Cases::default();
    let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("load");
    plugin.enable(&start_info(), &mut session).expect("enable");
    log_in_heavy(&mut plugin, &mut session);

    let fuel = send(&mut plugin, &mut session, &command("vt", "compshud on"));
    cases.add("comps hud: shown", fuel);
    let shown = ui_action(&session, "ShowCompsHudEditor", ActionValue::None);
    let fuel = send(&mut plugin, &mut session, &shown);
    cases.add("comps hud: edit items", fuel);
    for row in 0..COMPS_HUD_ITEMS {
        let toggled = ui_action(&session, "ToggleCompsHudCarriedAt", ActionValue::Int(row));
        let fuel = send(&mut plugin, &mut session, &toggled);
        cases.add("comps hud: edit items", fuel);
        let fuel = tick(&mut plugin, &mut session);
        cases.add("comps hud: tick (recounted)", fuel);
    }
    let hidden = ui_action(&session, "HideCompsHudEditor", ActionValue::None);
    send(&mut plugin, &mut session, &hidden);
    for _ in 0..60 {
        session.world.fake.clock.monotonic_ms += 600;
        let fuel = tick(&mut plugin, &mut session);
        cases.add("comps hud: tick (recounted)", fuel);
    }
    let strip = |session: &Session| {
        session
            .world
            .fake
            .ui
            .canvases()
            .filter(|canvas| canvas.descriptor.canvas_id == "comps-hud")
            .last()
            .map(|canvas| {
                (
                    canvas.id,
                    canvas.descriptor.height,
                    canvas.offset,
                    format!("{:?}", canvas.paint),
                )
            })
            .expect("the HUD's canvas")
    };
    let (canvas, height, from, painted) = strip(&session);
    assert!(
        height >= COMPS_HUD_ITEMS * 16,
        "a row per item: {height} pixels"
    );
    assert!(!painted.contains("Line"), "no buttons yet");
    let pointer = |kind: u8, button: u8, x: f32, y: f32| {
        Event::Ui(UiEvent::Pointer(P {
            canvas,
            kind,
            x,
            y,
            button,
            modifiers: 0,
            wheel_delta: 0,
        }))
    };
    for _ in 0..10 {
        let fuel = send(&mut plugin, &mut session, &pointer(P::HOVER, 0, 10.0, 20.0));
        cases.add("comps hud: hover", fuel);
        let fuel = tick(&mut plugin, &mut session);
        cases.add("comps hud: tick (hovered)", fuel);
        assert!(strip(&session).3.contains("Line"), "the buttons show");
        let fuel = send(&mut plugin, &mut session, &pointer(P::LEAVE, 0, 0.0, 0.0));
        cases.add("comps hud: hover", fuel);
        let fuel = tick(&mut plugin, &mut session);
        cases.add("comps hud: tick (hovered)", fuel);
        assert!(!strip(&session).3.contains("Line"), "and go");
    }
    let fuel = send(&mut plugin, &mut session, &pointer(P::DOWN, 1, 10.0, 20.0));
    cases.add("comps hud: drag", fuel);
    for _ in 0..20 {
        let fuel = send(&mut plugin, &mut session, &pointer(P::MOVE, 1, 13.0, 22.0));
        cases.add("comps hud: drag", fuel);
    }
    let fuel = send(&mut plugin, &mut session, &pointer(P::UP, 1, 10.0, 20.0));
    cases.add("comps hud: drag", fuel);
    let (_, _, to, _) = strip(&session);
    assert_eq!((to.0 - from.0, to.1 - from.1), (60.0, 40.0), "dragged");
    for _ in 0..10 {
        session.world.fake.clock.monotonic_ms += 600;
        let fuel = tick(&mut plugin, &mut session);
        cases.add("comps hud: tick (recounted)", fuel);
    }

    let cases = cases.all();
    let report = report(&cases);
    println!("{report}");
    assert_within_margin(&cases, &report);
}

/// The things about a heavy character of the Inv. Items page's classes it
/// has none of (a row each, count 0): mana stones, kits, scarabs and
/// essence on the ground.
fn unowned_consumables() -> Vec<ac_plugin_abi::world::WorldObject> {
    use ac_plugin_abi::world::{ObjectClass, WorldObject};
    let kinds = [
        ("Major Mana Stone", ObjectClass::ManaStone),
        ("Minor Mana Stone", ObjectClass::ManaStone),
        ("Healing Kit", ObjectClass::HealingKit),
        ("Treated Healing Kit", ObjectClass::HealingKit),
        ("Lead Scarab", ObjectClass::SpellComponent),
        ("Corrupted Essence", ObjectClass::Misc),
    ];
    (0..60u32)
        .map(|index| {
            let (name, class) = kinds[index as usize % kinds.len()];
            WorldObject {
                object_id: 0x5100_0000 + index,
                name: name.to_owned(),
                object_class: class,
                stack_size: 1,
                icon_id: 0x0600_3000,
                ..WorldObject::default()
            }
        })
        .collect()
}

/// A created-object event for `object`.
fn created(revision: i64, object: &ac_plugin_abi::world::WorldObject) -> Event {
    Event::ObjectChanged(ObjectChange {
        object_id: object.object_id,
        kind: ObjectChangeKind::Created,
        revision,
        changed_fields: ObjectChangeKind::Created.fields(),
        current: Some(object.clone()),
    })
}

/// The Inv. Items page on the heavy character, shown through the login
/// burst: the login's walk of the owned list early in it, the rest of the
/// pack (its salvage, scarabs, tapers, kits, stones, notes and money among
/// the rest) and the town arriving after it, things on the ground of the
/// classes it tracks, a corpse opened, the 1 s refresh after the burst,
/// then stacks changing and used up.
#[test]
#[ignore = "needs the release module: scripts/check.sh runs it after the WebAssembly build"]
#[allow(clippy::too_many_lines)]
fn the_inv_items_page_fits() {
    use ac_plugin_abi::ui::ActionValue;
    use ac_plugin_abi::world::{ObjectClass, WorldObject};
    let wasm = std::fs::read(wasm_path()).expect("the release mosstank.wasm (build it first)");
    let mut session = new_session();
    let mut cases = Cases::default();
    let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("load");
    plugin.enable(&start_info(), &mut session).expect("enable");
    for handler in ["ShowMag", "ShowMagInvItems"] {
        let shown = ui_action(&session, handler, ActionValue::None);
        let fuel = send(&mut plugin, &mut session, &shown);
        cases.add("inv items: shown", fuel);
    }

    let inventory = heavy::inventory();
    let surroundings = heavy::surroundings();
    let mut automation = character_automation(&session.table);
    automation.properties = heavy::player_properties(&surroundings);
    session.world.fake.character = Some(common::character("Mossy Hunter"));
    session.world.automation = Some(automation);
    session.world.chat_log = Some(Vec::new());
    session.world.loot = Some(common::FrameLootSurface::default());
    session
        .world
        .set_reply::<calls::ItemsInventoryComplete>(&true);
    let mut revision = 0i64;
    let unowned = unowned_consumables();
    for (index, thing) in inventory.iter().chain(&surroundings).enumerate() {
        create(&mut session, thing);
        revision += 1;
        let fuel = send(
            &mut plugin,
            &mut session,
            &object_change(ObjectChangeKind::Created, revision, thing),
        );
        cases.add("inv items: burst object created", fuel);
        if index == 100 {
            let fuel = send(&mut plugin, &mut session, &Event::LoginComplete);
            cases.add("inv items: login complete", fuel);
        }
        if index % 25 == 3 {
            let object = &unowned[(index / 25) % unowned.len()];
            session
                .world
                .automation()
                .world_objects
                .push(object.clone());
            revision += 1;
            let fuel = send(&mut plugin, &mut session, &created(revision, object));
            cases.add("inv items: burst object created", fuel);
        }
        if index % (heavy::BURST_OBJECTS / heavy::BURST_CHAT_LINES) == 7 {
            chat(
                &mut plugin,
                &mut session,
                &mut cases,
                "inv items: chat line",
            );
        }
        if index % 15 == 14 {
            let fuel = tick(&mut plugin, &mut session);
            cases.add("inv items: burst tick", fuel);
        }
    }

    // A corpse opened: its contents arrive.
    let corpse = surroundings
        .iter()
        .find(|thing| thing.object.object_class == ObjectClass::Corpse)
        .expect("a corpse")
        .object
        .object_id;
    session.world.loot().current = corpse;
    for item in heavy::corpse_contents(corpse) {
        revision += 1;
        let object = WorldObject {
            object_id: item.object_id,
            name: item.name.clone(),
            object_class: item.object_class,
            container_id: corpse,
            stack_size: item.stack_size,
            ..WorldObject::default()
        };
        let fuel = send(&mut plugin, &mut session, &created(revision, &object));
        cases.add("inv items: corpse item created", fuel);
    }
    // The 1 s refresh, three seconds of it.
    for _ in 0..200 {
        let fuel = tick(&mut plugin, &mut session);
        cases.add("inv items: tick (page shown)", fuel);
    }
    // The fixed rows, the pack's seven tracked names, the ground's six.
    let rows = main_list_len(&session, "MagInvItemsNames");
    assert_eq!(rows, 7 + 7 + 6, "the rows drawn");

    // Stacks change (used from, picked up), and some are used up.
    let tracked: Vec<Created> = inventory
        .iter()
        .filter(|thing| {
            matches!(
                thing.object.object_class,
                ObjectClass::SpellComponent
                    | ObjectClass::ManaStone
                    | ObjectClass::HealingKit
                    | ObjectClass::TradeNote
                    | ObjectClass::Salvage
                    | ObjectClass::Money
            )
        })
        .cloned()
        .collect();
    assert!(tracked.len() > 300);
    for (index, thing) in tracked.iter().take(300).enumerate() {
        revision += 1;
        let fuel = send(
            &mut plugin,
            &mut session,
            &object_change(ObjectChangeKind::Updated, revision, thing),
        );
        cases.add("inv items: tracked object updated", fuel);
        if index % 3 == 2 {
            let fuel = tick(&mut plugin, &mut session);
            cases.add("inv items: tick (page shown)", fuel);
        }
    }
    for thing in tracked.iter().rev().take(30) {
        let id = thing.object.object_id;
        let automation = session.world.automation();
        automation.item_entries.retain(|item| item.object_id != id);
        automation
            .world_objects
            .retain(|object| object.object_id != id);
        revision += 1;
        let fuel = send(
            &mut plugin,
            &mut session,
            &object_change(ObjectChangeKind::Released, revision, thing),
        );
        cases.add("inv items: tracked object released", fuel);
        let fuel = tick(&mut plugin, &mut session);
        cases.add("inv items: tick (page shown)", fuel);
    }
    assert_eq!(
        main_list_len(&session, "MagInvItemsNames"),
        rows,
        "rows stay"
    );

    let cases = cases.all();
    let report = report(&cases);
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

    // A player's large loot profile is loaded, and the next kill is looted
    // with it: each of the corpse's two dozen items is weighed against a few
    // thousand rules (owner's report, 2026-10-15: 53-77 ms a tick while
    // looting, when every decision compiled every rule's pattern again and
    // a corpse was weighed in one frame). The last corpse closes first.
    let closed = {
        let loot = session.world.loot();
        let closed = loot.current;
        loot.current = 0;
        loot.opened = 0;
        loot.contents_ready = false;
        closed
    };
    if closed != 0 {
        let fuel = send(&mut plugin, &mut session, &Event::ContainerClosed(closed));
        cases.add("event: corpse closed", fuel);
    }
    let (large_source, large_profile) = profiles::largest_loot_profile();
    session
        .world
        .profiles
        .insert("mosstank/loot/vt-large-loot.utl", &large_profile);
    let fuel = send(
        &mut plugin,
        &mut session,
        &command("vt", "loot load vt-large-loot"),
    );
    cases.add("event: /vt loot command (large profile)", fuel);
    // About fifteen seconds before the next kill: the profile's patterns
    // are compiled ahead meanwhile.
    for _ in 0..900 {
        let fuel = tick(&mut plugin, &mut session);
        cases.add("tick after the large profile's load", fuel);
    }
    {
        let loot = session.world.loot();
        let mut fresh = loot.corpses.first().cloned().expect("a corpse");
        fresh.object_id = 0x8FFF_0001;
        fresh.distance = 3.0;
        loot.corpses.push(fresh);
        loot.current = 0;
        loot.opened = 0;
        loot.contents_ready = false;
    }
    let mut reopened = BTreeSet::new();
    for _ in 0..1200 {
        let fuel = tick(&mut plugin, &mut session);
        cases.add("loot tick (large profile)", fuel);
        let loot = session.world.loot();
        if loot.opened != 0 && loot.current != loot.opened {
            loot.current = loot.opened;
            loot.contents_ready = true;
            reopened.insert(loot.opened);
            let event = Event::ContainerOpened(loot.opened);
            let fuel = send(&mut plugin, &mut session, &event);
            cases.add("event: corpse opened (large profile)", fuel);
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
        "corpses opened: {}; casts: {}; large loot profile: {large_source} ({} KB)",
        opened.len(),
        session.world.cast_spell_ids().len(),
        large_profile.len() / 1024
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
    assert!(
        !reopened.is_empty(),
        "no corpse was opened with the large profile:\n{report}"
    );
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

/// The ticks the launcher's login commands may wait for the character's
/// profile set and the frames MossTank's own work takes, whatever the Mag
/// features have to do.
const LOGIN_COMMAND_TICKS: usize = 20;

/// Everything at once: the owner's heavy character logs in with a
/// 1,500-item inventory log, a 1,000-monster persistent combat file and
/// twenty worn mana items, more than two hundred objects nearby, the Inv.
/// Items page shown and the Inventory window open on three alts' 1,500-item
/// logs; the launcher's login commands wait for the profile set and start
/// the macro on the login meta; a 200-line chat burst comes; then the
/// plugin is disabled in the world (the log and the stats written). Every
/// call under half its budget, the login commands run within
/// [`LOGIN_COMMAND_TICKS`] ticks, and the Mag work done meanwhile.
#[test]
#[ignore = "needs the release module: scripts/check.sh runs it after the WebAssembly build"]
#[allow(clippy::too_many_lines)]
fn the_heavy_login_with_every_mag_feature_fits() {
    use ac_plugin_abi::ui::ActionValue;
    let wasm = std::fs::read(wasm_path()).expect("the release mosstank.wasm (build it first)");
    let folder = profiles::folder();
    let mut session = folder_session(&folder);
    let meta = login_meta(&folder, &mut session);
    let character = character_bound_to(&folder, &meta);
    let log_key = format!("Mag-Tools/Coldeve/{character}.inventory.json");
    let stats_key = format!("Mag-Tools/Coldeve/{character}.CombatTracker.xml");

    // A first session logs the character's inventory.
    {
        let mut first = new_session();
        let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("load");
        plugin.enable(&start_info(), &mut first).expect("enable");
        enter_world_as(&mut first, &character);
        let properties = heavy::appraised_properties(&heavy::inventory());
        first.world.automation().properties.extend(properties);
        first
            .world
            .set_reply::<calls::ItemsInventoryComplete>(&true);
        tick(&mut plugin, &mut first);
        send(&mut plugin, &mut first, &Event::LoginComplete);
        let mut ticks = 0;
        while first.world.plugin_data.get(&log_key).is_none() {
            tick(&mut plugin, &mut first);
            ticks += 1;
            assert!(ticks < 1000, "the first session's log never written");
        }
        send(&mut plugin, &mut first, &Event::Logoff);
        plugin.disable(&mut first).expect("disable");
        let log = first.world.plugin_data.get(&log_key).expect("the log");
        session.world.plugin_data.insert(&log_key, log);
        for alt in 1..=3 {
            let name = format!("Alt {alt}");
            let text = log.replacen(
                &format!("\"character\":\"{character}\""),
                &format!("\"character\":\"{name}\""),
                1,
            );
            session
                .world
                .plugin_data
                .insert(&format!("Mag-Tools/Coldeve/{name}.inventory.json"), &text);
        }
    }
    let saved_stats = heavy::persistent_combat_stats(&character, heavy::PERSISTENT_MONSTERS);
    session.world.plugin_data.insert(&stats_key, &saved_stats);
    let saved_log = session.world.plugin_data.get(&log_key).unwrap().to_owned();

    let mut cases = Cases::default();
    let (mut plugin, _) = Plugin::load(&wasm, DEFAULT_MEMORY_MIB).expect("load");
    let enabled = plugin.enable(&start_info(), &mut session).expect("enable");
    cases.add("all: start (the profile folder)", enabled);
    for handler in ["ShowMag", "ShowMagInvItems", "OpenMagInventory"] {
        let action = ui_action(&session, handler, ActionValue::None);
        let fuel = send(&mut plugin, &mut session, &action);
        cases.add("all: page shown, window opened", fuel);
    }
    enter_world_as(&mut session, &character);
    let properties = heavy::appraised_properties(&heavy::inventory());
    session.world.automation().properties.extend(properties);
    let worn = wear_gear(&mut session);
    assert_eq!(worn.len(), WORN_MANA_ITEMS);
    session
        .world
        .set_reply::<calls::ItemsInventoryComplete>(&true);
    let login = send(&mut plugin, &mut session, &Event::LoginComplete);
    cases.add("all: login complete", login);
    let fuel = tick(&mut plugin, &mut session);
    cases.add("all: tick (the profile set loading)", fuel);
    for (verb, arguments) in login_commands(&meta) {
        let fuel = send(&mut plugin, &mut session, &command(verb, &arguments));
        cases.add("all: a login command (the set loading)", fuel);
    }
    let ran = |session: &Session| {
        session
            .world
            .requests::<calls::ChatSubmit>()
            .iter()
            .any(|line| line.contains("I just logged in"))
    };
    let mut command_ticks = None;
    let total = heavy::INVENTORY_ITEMS * 4;
    let all = format!("{total} items");
    let mut read_ticks = None;
    for count in 0..1500 {
        if command_ticks.is_none() && ran(&session) {
            command_ticks = Some(count);
        }
        if read_ticks.is_none() && inventory_status(&session) == all {
            read_ticks = Some(count);
        }
        if count == 300 {
            for line in 0..heavy::BURST_CHAT_LINES {
                let fuel = fight_or_town_line(&mut plugin, &mut session, line, &character);
                cases.add("all: chat line (burst)", fuel);
            }
        } else if count % 4 == 0 {
            chat(&mut plugin, &mut session, &mut cases, "all: chat line");
        }
        let fuel = tick(&mut plugin, &mut session);
        cases.add("all: tick (the macro running)", fuel);
        if read_ticks.is_some() && count > 600 {
            break;
        }
    }
    let inv_items_rows = main_list_len(&session, "MagInvItemsNames");
    every_owned_item_moved(&mut plugin, &mut session, &mut cases, "all");
    let disabled = plugin.disable(&mut session).expect("disable");
    cases.add("all: disabled in the world", disabled);
    let written_log = session.world.plugin_data.get(&log_key).unwrap_or_default();
    let written_stats = session
        .world
        .plugin_data
        .get(&stats_key)
        .unwrap_or_default()
        .to_owned();
    let log_rewritten = written_log != saved_log;
    let items = written_log.matches("\n{\"id\":").count();

    let cases = cases.all();
    let mut report = report(&cases);
    let _ = writeln!(
        report,
        "profile folder: {}; character: {character}; meta: {meta}; login commands ran after {command_ticks:?} ticks; {total} items read after {read_ticks:?} ticks; Inv. Items rows {inv_items_rows}",
        folder.source
    );
    println!("{report}");
    let command_ticks = command_ticks.expect("the login commands ran");
    assert!(command_ticks <= LOGIN_COMMAND_TICKS, "{report}");
    assert!(read_ticks.is_some(), "the Inventory window read: {report}");
    assert!(inv_items_rows > 7, "{report}");
    assert!(log_rewritten, "the log written at disable: {report}");
    assert_eq!(items, heavy::INVENTORY_ITEMS, "{report}");
    assert!(
        written_stats.len() > saved_stats.len(),
        "the stats read in, the fight added, written at disable: {report}"
    );
    assert_no_file_rewritten(&session, &report);
    assert_within_margin(&cases, &report);
}

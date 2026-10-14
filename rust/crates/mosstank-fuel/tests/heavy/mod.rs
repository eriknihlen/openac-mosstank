//! A heavy character on a busy public server, as the fuel gate measures
//! it: a full inventory with side packs, foci, salvage and stacks, a large
//! spellbook, a crowded town (players, vendors, portals, monsters), many
//! enchantments, an allegiance, a fellowship, corpses about, and the login
//! burst that brings all of it in (owner, 2026-10-14: MossTank ran out of
//! fuel seven seconds after loading on play.coldeve.ac, a world far larger
//! than the local test character's).

use std::collections::BTreeMap;

use ac_plugin_abi::character::ActiveEnchantment;
use ac_plugin_abi::combat::CombatTarget;
use ac_plugin_abi::items::{InventoryItem, LootContainer};
use ac_plugin_abi::magic::SpellInfo;
use ac_plugin_abi::navigation::NavigationSnapshot;
use ac_plugin_abi::social::{AllegianceMember, AllegianceState, FellowMember, FellowshipState};
use ac_plugin_abi::types::ChatLine;
use ac_plugin_abi::world::{ItemProperties, ObjectClass, Position, WorldObject};

/// The character's object id.
pub const CHARACTER_ID: u32 = 0x5000_000A;

/// How many items the character carries, its side packs among them.
pub const INVENTORY_ITEMS: usize = 1500;

/// How many side packs.
pub const SIDE_PACKS: usize = 30;

/// How many objects stand about the character (players, monsters, vendors,
/// portals, the town's furniture).
pub const NEARBY_OBJECTS: usize = 300;

/// How many of them are players.
pub const NEARBY_PLAYERS: usize = 50;

/// How many of them are monsters.
pub const NEARBY_MONSTERS: usize = 100;

/// How many corpses lie about.
pub const CORPSES: usize = 40;

/// How many objects the login burst creates, all told.
pub const BURST_OBJECTS: usize = 2000;

/// How many chat lines arrive in the burst.
pub const BURST_CHAT_LINES: usize = 200;

/// How many enchantments are on the character.
pub const ENCHANTMENTS: usize = 60;

/// How many members the allegiance has.
pub const ALLEGIANCE_MEMBERS: u32 = 100;

/// How many spells the spellbook holds.
pub const KNOWN_SPELLS: usize = 2000;

/// The landblock cell the character stands in (a town).
const CELL: u32 = 0xA9B4_0021;

/// Where the character stands, landblock-local metres.
const HERE: [f32; 3] = [96.0, 96.0, 0.0];

/// A place `metres` off the character, at `degrees` from north.
fn near(metres: f32, degrees: f32) -> Position {
    let radians = degrees.to_radians();
    Position::from_cell(
        CELL,
        [
            HERE[0] + metres * radians.sin(),
            HERE[1] + metres * radians.cos(),
            HERE[2],
        ],
        degrees,
    )
}

/// The character's movement snapshot: in the world, standing in town.
#[must_use]
pub fn navigation() -> NavigationSnapshot {
    NavigationSnapshot {
        is_available: true,
        local_object_id: CHARACTER_ID,
        position: near(0.0, 0.0),
        confirmed_position: near(0.0, 0.0),
        revision: 1,
        confirmed_position_revision: 1,
        ..NavigationSnapshot::default()
    }
}

/// One object the burst creates, as the client's object table holds it,
/// with the owned item behind it when the character carries it.
#[derive(Debug, Clone)]
pub struct Created {
    /// The object.
    pub object: WorldObject,
    /// The inventory record, for an owned item.
    pub item: Option<InventoryItem>,
}

/// What a carried item is: its name, item type bits, class, stack and
/// worn place.
struct Kind {
    name: &'static str,
    item_type: u32,
    class: ObjectClass,
    stack: i32,
    valid_locations: u32,
}

const fn kind(
    name: &'static str,
    item_type: u32,
    class: ObjectClass,
    stack: i32,
    valid_locations: u32,
) -> Kind {
    Kind {
        name,
        item_type,
        class,
        stack,
        valid_locations,
    }
}

/// The carried things a heavy character has many of, by share.
const CARRIED: [Kind; 30] = [
    kind("Foci of Enchantment", 0x80, ObjectClass::Foci, 1, 0),
    kind(
        "Foci of Creature Enchantment",
        0x80,
        ObjectClass::Foci,
        1,
        0,
    ),
    kind("Foci of Elements", 0x80, ObjectClass::Foci, 1, 0),
    kind("Foci of Strife", 0x80, ObjectClass::Foci, 1, 0),
    kind("Foci of Verdancy", 0x80, ObjectClass::Foci, 1, 0),
    kind("Salvaged Iron", 0x4000_0000, ObjectClass::Salvage, 100, 0),
    kind("Salvaged Steel", 0x4000_0000, ObjectClass::Salvage, 100, 0),
    kind("Salvaged Granite", 0x4000_0000, ObjectClass::Salvage, 87, 0),
    kind(
        "Salvaged Mahogany",
        0x4000_0000,
        ObjectClass::Salvage,
        64,
        0,
    ),
    kind("Salvaged Brass", 0x4000_0000, ObjectClass::Salvage, 100, 0),
    kind("Mana Scarab", 0x1000, ObjectClass::SpellComponent, 100, 0),
    kind(
        "Platinum Scarab",
        0x1000,
        ObjectClass::SpellComponent,
        100,
        0,
    ),
    kind(
        "Prismatic Taper",
        0x1000,
        ObjectClass::SpellComponent,
        250,
        0,
    ),
    kind("Pyreal Mote", 0x80, ObjectClass::Misc, 25, 0),
    kind("Stamina Elixir", 0x80, ObjectClass::Misc, 10, 0),
    kind("Health Elixir", 0x80, ObjectClass::Misc, 10, 0),
    kind("Mana Elixir", 0x80, ObjectClass::Misc, 10, 0),
    kind("Roast Beef", 0x20, ObjectClass::Food, 50, 0),
    kind("Peerless Healing Kit", 0x80, ObjectClass::HealingKit, 1, 0),
    kind("Mana Stone", 0x800, ObjectClass::ManaStone, 1, 0),
    kind("Gold Piece", 0x40, ObjectClass::Money, 25000, 0),
    kind("Trade Note (250,000)", 0x40, ObjectClass::TradeNote, 20, 0),
    kind(
        "Greater Amuli Shadow Coat",
        0x2,
        ObjectClass::Armor,
        1,
        0x0000_0E00,
    ),
    kind("Shadow Gauntlets", 0x2, ObjectClass::Armor, 1, 0x0000_0020),
    kind("Ornate Ring", 0x8, ObjectClass::Jewelry, 1, 0x0004_0000),
    kind("Gold Necklace", 0x8, ObjectClass::Jewelry, 1, 0x0000_8000),
    kind(
        "Crystal Wand",
        0x8000,
        ObjectClass::WandStaffOrb,
        1,
        0x0100_0000,
    ),
    kind(
        "Assault Tachi",
        0x1,
        ObjectClass::MeleeWeapon,
        1,
        0x0010_0000,
    ),
    kind(
        "Ancient Mhoire Atlatl",
        0x100,
        ObjectClass::MissileWeapon,
        1,
        0x0040_0000,
    ),
    kind("Shard of the Herald's Crest", 0x80, ObjectClass::Misc, 1, 0),
];

/// The spells an item of the gear kinds carries (appraised).
fn gear_spells(index: usize) -> Vec<u32> {
    let base = 2000 + u32::try_from(index % 400).unwrap_or(0) * 3;
    (0..6).map(|offset| base + offset).collect()
}

/// The character's inventory: [`SIDE_PACKS`] packs on the character, then
/// [`INVENTORY_ITEMS`] in all, spread across the main pack and the packs,
/// in the order the server sends them (packs first).
#[must_use]
pub fn inventory() -> Vec<Created> {
    let mut created = Vec::with_capacity(INVENTORY_ITEMS);
    let pack_id = |pack: usize| 0x8000_1000 + u32::try_from(pack).unwrap_or(0);
    for pack in 0..SIDE_PACKS {
        let object_id = pack_id(pack);
        let name = if pack % 3 == 0 {
            "Pack"
        } else if pack % 3 == 1 {
            "Sack"
        } else {
            "Trader's Pouch"
        };
        let item = InventoryItem {
            object_id,
            weenie_class_id: 136,
            name: name.to_owned(),
            item_type: 0x200,
            container_id: CHARACTER_ID,
            stack_size: 1,
            items_capacity: 24,
            container_slot: i32::try_from(pack).unwrap_or(0),
            object_class: ObjectClass::Container,
            ..InventoryItem::default()
        };
        created.push(Created {
            object: owned_object(&item, Vec::new()),
            item: Some(item),
        });
    }
    for index in SIDE_PACKS..INVENTORY_ITEMS {
        let kind = &CARRIED[index % CARRIED.len()];
        let object_id = 0x8000_2000 + u32::try_from(index).unwrap_or(0);
        // The main pack holds the first hundred and two; the packs the rest.
        let carried = index - SIDE_PACKS;
        let container_id = if carried < 102 {
            CHARACTER_ID
        } else {
            pack_id((carried - 102) / 24 % SIDE_PACKS)
        };
        let gear = kind.valid_locations != 0;
        let spells = if gear { gear_spells(index) } else { Vec::new() };
        let item = InventoryItem {
            object_id,
            weenie_class_id: 4000 + u32::try_from(index % CARRIED.len()).unwrap_or(0),
            name: kind.name.to_owned(),
            item_type: kind.item_type,
            container_id,
            valid_locations: kind.valid_locations,
            stack_size: kind.stack,
            maximum_stack_size: kind.stack.max(1),
            container_slot: i32::try_from(carried % 24).unwrap_or(0),
            object_class: kind.class,
            appraised_spell_ids: spells.clone(),
            item_current_mana: if gear { 1200 } else { 0 },
            item_maximum_mana: if gear { 1500 } else { 0 },
            value: 5000,
            burden: 50,
            // Salvage, and every piece of gear, has a material.
            material_type: if kind.class == ObjectClass::Salvage || gear {
                u32::try_from(index % 40).unwrap_or(0) + 20
            } else {
                0
            },
            workmanship: if kind.class == ObjectClass::Salvage {
                7.5
            } else {
                0.0
            },
            ..InventoryItem::default()
        };
        created.push(Created {
            object: owned_object(&item, spells),
            item: Some(item),
        });
    }
    created
}

/// The object table's record of a carried item.
fn owned_object(item: &InventoryItem, spells: Vec<u32>) -> WorldObject {
    WorldObject {
        object_id: item.object_id,
        weenie_class_id: item.weenie_class_id,
        name: item.name.clone(),
        object_class: item.object_class,
        item_type: item.item_type,
        container_id: item.container_id,
        is_owned: true,
        has_appraisal_data: !spells.is_empty(),
        last_id_time: i32::from(!spells.is_empty()),
        stack_size: item.stack_size,
        items_capacity: item.items_capacity,
        spell_ids: spells,
        icon_id: 0x0600_1000,
        ..WorldObject::default()
    }
}

const PLAYER_NAMES: [&str; 10] = [
    "Aerfalle",
    "Brightblade",
    "Corwin",
    "Dusk Strider",
    "Elysa",
    "Fenwick",
    "Grimmauld",
    "Halcyon",
    "Ironvein",
    "Jasmine of Holtburg",
];

const MONSTER_NAMES: [&str; 5] = [
    "Mosswart Kin",
    "Drudge Skulker",
    "Banderling Raider",
    "Tusker Guard",
    "Olthoi Ripper",
];

/// The objects about the character: [`NEARBY_PLAYERS`] players,
/// [`NEARBY_MONSTERS`] monsters, vendors, townsfolk, portals, doors and the
/// town's furniture, [`NEARBY_OBJECTS`] in all; then [`CORPSES`] corpses and
/// the things lying about, up to [`BURST_OBJECTS`] with the inventory.
#[must_use]
pub fn surroundings() -> Vec<Created> {
    let mut created = Vec::new();
    let mut next_id = 0x5000_1000u32;
    let mut add = |name: String, class: ObjectClass, metres: f32| {
        let object_id = next_id;
        next_id += 1;
        let degrees = (object_id % 360) as f32;
        created.push(Created {
            object: WorldObject {
                object_id,
                weenie_class_id: 1 + object_id % 5000,
                name,
                object_class: class,
                item_type: match class {
                    ObjectClass::Player
                    | ObjectClass::Monster
                    | ObjectClass::Npc
                    | ObjectClass::Vendor => 0x10,
                    ObjectClass::Portal => 0x1_0000,
                    ObjectClass::Corpse => 0x200,
                    _ => 0x80,
                },
                capabilities: ac_plugin_abi::world::capabilities::INTERACTABLE,
                // In the world, not carried: the host's landscape.
                is_landscape: true,
                position: Some(near(metres, degrees)),
                stack_size: 1,
                icon_id: 0x0600_2000,
                ..WorldObject::default()
            },
            item: None,
        });
        object_id
    };
    for index in 0..NEARBY_PLAYERS {
        let name = format!("{} {}", PLAYER_NAMES[index % PLAYER_NAMES.len()], index);
        add(name, ObjectClass::Player, 4.0 + (index % 30) as f32);
    }
    for index in 0..NEARBY_MONSTERS {
        let name = MONSTER_NAMES[index % MONSTER_NAMES.len()].to_owned();
        add(name, ObjectClass::Monster, 8.0 + (index % 50) as f32);
    }
    for index in 0..20 {
        add(
            format!("Vendor {index}"),
            ObjectClass::Vendor,
            10.0 + index as f32,
        );
    }
    for index in 0..30 {
        add(
            format!("Townsperson {index}"),
            ObjectClass::Npc,
            12.0 + index as f32,
        );
    }
    for index in 0..10 {
        add(
            format!("Portal to Town {index}"),
            ObjectClass::Portal,
            20.0 + index as f32,
        );
    }
    for index in 0..20 {
        add(
            format!("Door {index}"),
            ObjectClass::Door,
            6.0 + index as f32,
        );
    }
    let furniture = NEARBY_OBJECTS - NEARBY_PLAYERS - NEARBY_MONSTERS - 80;
    for index in 0..furniture {
        add(
            format!("Sign {index}"),
            ObjectClass::Sign,
            5.0 + (index % 60) as f32,
        );
    }
    for index in 0..CORPSES {
        let name = format!("Corpse of {}", MONSTER_NAMES[index % MONSTER_NAMES.len()]);
        add(name, ObjectClass::Corpse, 3.0 + (index % 25) as f32);
    }
    let lying = BURST_OBJECTS - INVENTORY_ITEMS - NEARBY_OBJECTS - CORPSES;
    for index in 0..lying {
        add(
            format!("Dropped Item {index}"),
            ObjectClass::Misc,
            15.0 + (index % 40) as f32,
        );
    }
    created
}

/// The players' appraisals: a level, half of them in the character's
/// allegiance.
#[must_use]
pub fn player_properties(surroundings: &[Created]) -> BTreeMap<u32, ItemProperties> {
    surroundings
        .iter()
        .filter(|created| created.object.object_class == ObjectClass::Player)
        .enumerate()
        .map(|(index, created)| {
            let monarch = if index % 2 == 0 {
                MONARCH_ID
            } else {
                0x5000_0F00 + index as u32
            };
            (
                created.object.object_id,
                ItemProperties {
                    ints: vec![(25, 100 + i32::try_from(index).unwrap_or(0) * 4)],
                    instance_ids: vec![(26, monarch)],
                    strings: vec![(47, "Mosswart Appreciation Society".to_owned())],
                    ..ItemProperties::default()
                },
            )
        })
        .collect()
}

/// The monarch.
const MONARCH_ID: u32 = 0x5000_0B00;

/// The allegiance: [`ALLEGIANCE_MEMBERS`] members, a dozen vassals.
#[must_use]
pub fn allegiance() -> AllegianceState {
    let member = |object_id: u32, name: String, rank: u32| AllegianceMember {
        object_id,
        name,
        rank,
        level: 200,
        heritage_group: 1,
        gender: 1,
        is_online: object_id.is_multiple_of(3),
    };
    AllegianceState {
        is_available: true,
        revision: 4,
        is_known: true,
        name: "Mosswart Appreciation Society".to_owned(),
        rank: 7,
        member_count: ALLEGIANCE_MEMBERS,
        vassal_count: 12,
        monarch_id: MONARCH_ID,
        monarch: Some(member(MONARCH_ID, "Old King Moss".to_owned(), 10)),
        patron: Some(member(0x5000_0B01, "Patron Pete".to_owned(), 8)),
        vassals: (0..12)
            .map(|index| member(0x5000_0C00 + index, format!("Vassal {index}"), 5))
            .collect(),
    }
}

/// A fellowship of six.
#[must_use]
pub fn fellowship() -> (FellowshipState, Vec<FellowMember>) {
    let members: Vec<FellowMember> = (0..6)
        .map(|index| FellowMember {
            object_id: if index == 0 {
                CHARACTER_ID
            } else {
                0x5000_1000 + index
            },
            name: format!("Fellow {index}"),
            current_health: 300,
            max_health: 400,
            current_stamina: 300,
            max_stamina: 400,
            current_mana: 300,
            max_mana: 400,
            distance: index as f32 * 5.0,
            share_loot: true,
            level: 200,
            vitals_age_seconds: Some(1.0),
        })
        .collect();
    let state = FellowshipState {
        is_in_fellowship: true,
        name: "Moss Hunters".to_owned(),
        leader_id: CHARACTER_ID,
        is_open: true,
        is_locked: false,
        member_count: 6,
        shares_experience: true,
        splits_experience_evenly: true,
    };
    (state, members)
}

/// The corpses as the loot surface lists them.
#[must_use]
pub fn corpses(surroundings: &[Created]) -> Vec<LootContainer> {
    surroundings
        .iter()
        .filter(|created| created.object.object_class == ObjectClass::Corpse)
        .enumerate()
        .map(|(index, created)| LootContainer {
            object_id: created.object.object_id,
            weenie_class_id: 21,
            name: created.object.name.clone(),
            distance: 3.0 + (index % 25) as f32,
            long_description: if index % 2 == 0 {
                "Killed by Mossy Hunter.".to_owned()
            } else {
                "Killed by Brightblade 1.".to_owned()
            },
            is_identified: true,
            is_appraisal_answered: true,
            position: created.object.position.clone(),
            ..LootContainer::default()
        })
        .collect()
}

/// What an opened corpse holds: two dozen things.
#[must_use]
pub fn corpse_contents(corpse_id: u32) -> Vec<InventoryItem> {
    (0..24u32)
        .map(|index| {
            let kind = &CARRIED[index as usize % CARRIED.len()];
            InventoryItem {
                object_id: 0x8100_0000 + (corpse_id & 0xFFFF) * 32 + index,
                weenie_class_id: 4000 + index,
                name: kind.name.to_owned(),
                item_type: kind.item_type,
                container_id: corpse_id,
                valid_locations: kind.valid_locations,
                stack_size: kind.stack,
                object_class: kind.class,
                value: 2500,
                ..InventoryItem::default()
            }
        })
        .collect()
}

/// The monsters as the combat view lists them, the one fought nearest.
#[must_use]
pub fn hostile_targets(surroundings: &[Created], fought: CombatTarget) -> Vec<CombatTarget> {
    let mut targets = vec![fought];
    targets.extend(
        surroundings
            .iter()
            .filter(|created| created.object.object_class == ObjectClass::Monster)
            .enumerate()
            .map(|(index, created)| CombatTarget {
                object_id: created.object.object_id,
                name: created.object.name.clone(),
                weenie_class_id: created.object.weenie_class_id,
                distance: 8.0 + (index % 50) as f32,
                is_health_known: index % 2 == 0,
                health_fraction: 1.0,
                species_name: "Mosswart".to_owned(),
                maximum_health: 120,
                ..CombatTarget::default()
            }),
    );
    targets
}

/// [`ENCHANTMENTS`] enchantments on the character, none of them a self
/// buff the buff rule casts (the buff case still buffs).
#[must_use]
pub fn enchantments(table: &[SpellInfo]) -> Vec<ActiveEnchantment> {
    table
        .iter()
        .filter(|spell| !spell.is_beneficial && !spell.is_offensive)
        .step_by(7)
        .take(ENCHANTMENTS)
        .enumerate()
        .map(|(index, spell)| ActiveEnchantment {
            spell_id: spell.spell_id,
            family: spell.family,
            tier: spell.tier,
            seconds_remaining: 600.0 + index as f64 * 30.0,
        })
        .collect()
}

/// One line of a busy town's chat: speech, channels, tells, system lines
/// and combat, by turns of the sequence.
#[must_use]
pub fn chat_line(sequence: u64, received_unix_ms: i64) -> ChatLine {
    let index = usize::try_from(sequence).unwrap_or(0);
    let speaker = format!(
        "{} {}",
        PLAYER_NAMES[index % PLAYER_NAMES.len()],
        index % 50
    );
    let (kind, log_text_type, channel, text) = match index % 8 {
        0 => (
            0,
            0x02,
            "",
            "anyone selling platinum scarabs? paying well".to_owned(),
        ),
        1 => (
            2,
            0x00,
            "General",
            "LF fellow for Olthoi North, level 200+, bring buffs".to_owned(),
        ),
        2 => (
            2,
            0x00,
            "Trade",
            "WTS Foci of Enchantment, salvaged granite 10w, PST".to_owned(),
        ),
        3 => (
            3,
            0x03,
            "",
            "are you still at the lifestone? I have your comps".to_owned(),
        ),
        4 => (4, 0x05, "", format!("{speaker} has logged in.")),
        5 => (
            8,
            0x15,
            "",
            format!("{speaker} hits the Mosswart Kin for 34 points of slashing damage!"),
        ),
        6 => (
            4,
            0x07,
            "",
            "Your Strength Self VIII spell has expired.".to_owned(),
        ),
        _ => (
            2,
            0x00,
            "Allegiance",
            "monarch is giving out rares at the mansion".to_owned(),
        ),
    };
    let system = kind == 4;
    ChatLine {
        sequence,
        sender_id: if system {
            0
        } else {
            0x5000_1000 + u32::try_from(index % 50).unwrap_or(0)
        },
        kind,
        sender: if system { String::new() } else { speaker },
        text,
        channel: channel.to_owned(),
        log_text_type,
        combat_kind: u32::from(kind == 8),
        channel_id: 0,
        display_text: String::new(),
        received_unix_ms,
    }
}

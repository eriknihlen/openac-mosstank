//! The Slot column: where an item can be worn or wielded, by name. A port
//! of the owner's MosswartOverlord inventory search's display logic (its Go
//! service's `slotname.go`): the EquipMask names, loot armor reduced to the
//! slot its coverage says it is for, then an exact name, a common one, or
//! the bits one by one.
//!
//! Where it differs: either finger reads `Ring` and either wrist
//! `Bracelet` (the website: `Left Ring, Right Ring`, and the wrists bit by
//! bit); the aetheria slots read `Aetheria Blue`, `Yellow`, `Red`, one bit
//! or several (the website: its own aetheria names, `slotname.go:151-158`,
//! sit behind its exact lookup, which finds the enum's `SigilOne` to
//! `SigilThree` first, so it showed those, joined bit by bit when
//! combined); `0x7FFFFFFF` (any ready slot) reads `-` (the website:
//! `CanGoInReadySlot`); and armor with a material but no coverage bits
//! is named by its own slot bits (the website: `Inventory`).

/// The EquipMask bits and their names, in ascending order (the decode
/// joins them in this order).
const EQUIP_MASK: &[(u32, &str)] = &[
    (0x0000_0001, "Head"),
    (0x0000_0002, "Chest"),
    (0x0000_0004, "Abdomen"),
    (0x0000_0008, "Upper Arms"),
    (0x0000_0010, "Lower Arms"),
    (0x0000_0020, "Hands"),
    (0x0000_0040, "Upper Legs"),
    (0x0000_0080, "Lower Legs"),
    (0x0000_0100, "Feet"),
    (0x0000_0200, "Chest"),
    (0x0000_0400, "Abdomen"),
    (0x0000_0800, "Upper Arms"),
    (0x0000_1000, "Lower Arms"),
    (0x0000_2000, "Upper Legs"),
    (0x0000_4000, "Lower Legs"),
    (0x0000_8000, "Neck"),
    (0x0001_0000, "Left Wrist"),
    (0x0002_0000, "Right Wrist"),
    (0x0004_0000, "Left Ring"),
    (0x0008_0000, "Right Ring"),
    (0x0010_0000, "Melee Weapon"),
    (0x0020_0000, "Shield"),
    (0x0040_0000, "Missile Weapon"),
    (0x0080_0000, "Ammo"),
    (0x0100_0000, "Held"),
    (0x0200_0000, "Two-Handed"),
    (0x0400_0000, "Trinket"),
    (0x0800_0000, "Cloak"),
    (0x1000_0000, "Aetheria Blue"),
    (0x2000_0000, "Aetheria Yellow"),
    (0x4000_0000, "Aetheria Red"),
];

/// Values named as a whole.
const COMMON: &[(u32, &str)] = &[
    (30, "Shirt"),
    (0x0004_0000 | 0x0008_0000, "Ring"),
    (0x0001_0000 | 0x0002_0000, "Bracelet"),
];

/// The EquipMask bits of body armor and of the head, hands and feet.
const BODY_ARMOR_EQUIP: u32 = 0x0000_7F21;

/// The coverage bits of outerwear, head, hands and feet.
const BODY_ARMOR_COVERAGE: u32 = 0x0001_FF00;

/// The slot's name: `valid_locations` (EquipMask bits), reduced by
/// `coverage` (CoverageMask bits) when the item has a material and several
/// armor bits. `-` for an item worn nowhere.
#[must_use]
pub fn slot_name(valid_locations: u32, coverage: u32, has_material: bool) -> String {
    // Nothing, any ready slot (every bit below the top one), or past it.
    if valid_locations == 0 || valid_locations >= i32::MAX.cast_unsigned() {
        return "-".to_owned();
    }
    let mut names: Vec<String> = Vec::new();
    for option in slot_options(valid_locations, coverage, has_material) {
        let name = translate(option);
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    }
    if names.is_empty() {
        "-".to_owned()
    } else {
        names.join(", ")
    }
}

/// The slots an item is for (`getSophisticatedSlotOptions`).
fn slot_options(valid_locations: u32, coverage: u32, has_material: bool) -> Vec<u32> {
    const LOWER_LEG_WEAR: u32 = 128;
    const FOOT_WEAR: u32 = 256;
    if valid_locations == LOWER_LEG_WEAR | FOOT_WEAR {
        return vec![FOOT_WEAR];
    }
    // Without coverage bits, its own slot bits name it.
    if valid_locations & BODY_ARMOR_EQUIP != 0
        && valid_locations.count_ones() > 1
        && has_material
        && coverage != 0
    {
        return coverage_reduction(coverage)
            .into_iter()
            .map(coverage_to_equip_mask)
            .collect();
    }
    vec![valid_locations]
}

/// The coverage a multi-slot piece is for (`getCoverageReductionOptions`).
fn coverage_reduction(coverage: u32) -> Vec<u32> {
    const UPPER_ARMS: u32 = 4096;
    const LOWER_ARMS: u32 = 8192;
    const UPPER_LEGS: u32 = 256;
    const LOWER_LEGS: u32 = 512;
    const CHEST: u32 = 1024;
    const ABDOMEN: u32 = 2048;
    const FEET: u32 = 65_536;
    if coverage.count_ones() <= 1 || coverage & BODY_ARMOR_COVERAGE == 0 {
        return vec![coverage];
    }
    match coverage {
        c if c == UPPER_ARMS | LOWER_ARMS => vec![UPPER_ARMS, LOWER_ARMS],
        c if c == UPPER_LEGS | LOWER_LEGS => vec![UPPER_LEGS, LOWER_LEGS],
        c if c == LOWER_LEGS | FEET => vec![FEET],
        c if c == CHEST | ABDOMEN
            || c == CHEST | ABDOMEN | UPPER_ARMS
            || c == CHEST | UPPER_ARMS | LOWER_ARMS
            || c == CHEST | UPPER_ARMS
            || c == CHEST | ABDOMEN | UPPER_ARMS | LOWER_ARMS =>
        {
            vec![CHEST]
        }
        c if c == ABDOMEN | UPPER_LEGS | LOWER_LEGS => vec![ABDOMEN, UPPER_LEGS, LOWER_LEGS],
        c if c == ABDOMEN | UPPER_LEGS => vec![ABDOMEN],
        c => vec![c],
    }
}

/// A coverage bit's EquipMask bit (`coverageToEquipMask`); others as they
/// are.
fn coverage_to_equip_mask(coverage: u32) -> u32 {
    match coverage {
        16_384 => 1,
        1024 => 512,
        4096 => 2048,
        8192 => 4096,
        32_768 => 32,
        2048 => 1024,
        256 => 8192,
        512 => 16_384,
        65_536 => 256,
        other => other,
    }
}

/// One slot value's name (`translateEquipmentSlot`): exact, common, then
/// bit by bit; empty for none.
fn translate(location: u32) -> String {
    if location == 0 {
        return String::new();
    }
    let exact = |table: &[(u32, &'static str)]| {
        table
            .iter()
            .find(|(mask, _)| *mask == location)
            .map(|(_, name)| *name)
    };
    if let Some(name) = exact(EQUIP_MASK).or_else(|| exact(COMMON)) {
        return name.to_owned();
    }
    let mut joined = String::new();
    for (mask, name) in EQUIP_MASK {
        if location & mask == *mask {
            if !joined.is_empty() {
                joined.push_str(", ");
            }
            joined.push_str(name);
        }
    }
    joined
}

/// The parts of a slot's name (`Chest, Abdomen` is two).
pub(crate) fn parts(slot: &str) -> impl Iterator<Item = &str> {
    slot.split(", ")
}

/// The armor slots (`suit_solver.go`'s set): crit damage tiers apply to
/// pieces worn there.
const ARMOR_SLOTS: &[&str] = &[
    "Head",
    "Chest",
    "Upper Arms",
    "Lower Arms",
    "Hands",
    "Abdomen",
    "Upper Legs",
    "Lower Legs",
    "Feet",
];

/// The slot is an armor slot, or one of its parts is.
pub(crate) fn is_armor_slot(slot: &str) -> bool {
    parts(slot).any(|part| ARMOR_SLOTS.contains(&part))
}

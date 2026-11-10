//! Test data (native builds only): the debuff spells as the client's spell
//! table has them.
//!
//! A debuff is told by its place in the spell table (its family and its
//! line there), so a test needs spells shaped like the table's. These rows
//! were read from the game's spell table: for each debuff the first, sixth,
//! seventh and eighth level of the line a character learns, the seventh
//! level of its self-cast twin, and some of the family's spells that only
//! monsters and items cast; then the lures, item spells whose names read
//! like the vulnerabilities' and are no debuff.

use ac_plugin_abi::magic::SpellInfo;

/// One spell: id, name, family, tier, difficulty, school skill, flag bits,
/// target mask, duration (seconds), mana, cast at nothing, the herb,
/// powder, potion and talisman of its formula, and the range constant and
/// modifier.
struct Row(
    u32,
    &'static str,
    u32,
    i32,
    i32,
    u32,
    u32,
    u32,
    f32,
    i32,
    bool,
    (u32, u32, u32, u32),
    f32,
    f32,
);

const BENEFICIAL: u32 = 0x4;
const SELF_TARGETED: u32 = 0x8;
const REVERSED: u32 = 0x10;
const FELLOWSHIP: u32 = 0x2000;
const DAMAGE_OVER_TIME: u32 = 0x0001_0000;

impl Row {
    fn spell(&self) -> SpellInfo {
        let flags = self.6;
        let beneficial = flags & BENEFICIAL != 0;
        let self_targeted = flags & SELF_TARGETED != 0;
        SpellInfo {
            spell_id: self.0,
            name: self.1.to_owned(),
            family: self.2,
            tier: self.3,
            difficulty: self.4,
            school: self.5,
            raw_flags: flags,
            target_mask: self.7,
            duration_seconds: self.8,
            mana_cost: self.9,
            is_untargeted: self.10,
            component_set: self.11,
            is_self_targeted: self_targeted,
            is_beneficial: beneficial,
            is_debuff: flags & REVERSED != 0,
            is_offensive: !beneficial && !self_targeted,
            is_fellowship: flags & FELLOWSHIP != 0,
            is_damage_over_time: flags & DAMAGE_OVER_TIME != 0,
            requires_turn_to: !self.10,
            base_range_constant: self.12,
            base_range_modifier: self.13,
            saying: "testwords".to_owned(),
            ..SpellInfo::default()
        }
    }
}

#[rustfmt::skip]
const ROWS: &[Row] = &[
    // Fester Other I
    Row(171, "Fester Other I", 94, 1, 25, 33, 0x13, 0x10, 60.0, 10, false, (12, 26, 41, 52), 5.0, 1.0),
    Row(176, "Fester Other VI", 94, 6, 275, 33, 0x13, 0x10, 210.0, 60, false, (12, 26, 41, 52), 5.0, 0.25),
    Row(2372, "Price of Immortality", 94, 6, 275, 33, 0x93, 0x10, 300.0, 60, false, (0, 0, 0, 52), 5.0, 0.25),
    Row(2178, "Decrepitude's Grasp", 94, 7, 325, 33, 0x93, 0x10, 240.0, 70, false, (12, 26, 41, 52), 5.0, 0.25),
    Row(2179, "Fester Self VII", 94, 7, 325, 33, 0x9a, 0x10, 240.0, 70, false, (12, 26, 41, 61), 5.0, 1.0),
    Row(4489, "Incantation of Fester Other", 94, 8, 400, 33, 0x93, 0x10, 300.0, 80, false, (12, 26, 41, 52), 5.0, 0.25),
    Row(4088, "Withering Poison", 94, 7, 500, 33, 0x93, 0x10, 240.0, 70, false, (12, 26, 41, 52), 5.0, 0.25),
    // Broadside of a Barn
    Row(262, "Defenselessness Other I", 40, 1, 1, 31, 0x13, 0x10, 60.0, 10, false, (8, 30, 37, 50), 5.0, 1.0),
    Row(267, "Defenselessness Other VI", 40, 6, 275, 31, 0x13, 0x10, 210.0, 60, false, (8, 30, 37, 50), 5.0, 0.25),
    Row(2228, "Broadside of a Barn", 40, 7, 325, 31, 0x93, 0x10, 240.0, 70, false, (8, 30, 37, 50), 5.0, 0.25),
    Row(2229, "Defenselessness Self VII", 40, 7, 325, 31, 0x9a, 0x10, 240.0, 70, false, (8, 30, 37, 60), 0.0, 0.0),
    Row(4543, "Incantation of Defenselessness Other", 40, 8, 400, 31, 0x93, 0x10, 300.0, 80, false, (8, 30, 37, 50), 5.0, 0.25),
    // Gravity Well
    Row(15, "Vulnerability Other I", 38, 1, 1, 31, 0x13, 0x10, 60.0, 10, false, (8, 30, 44, 50), 0.0, 1.0),
    Row(234, "Vulnerability Other VI", 38, 6, 275, 31, 0x13, 0x10, 210.0, 60, false, (8, 30, 44, 50), 5.0, 0.25),
    Row(2318, "Gravity Well", 38, 7, 325, 31, 0x93, 0x10, 240.0, 70, false, (8, 30, 44, 50), 5.0, 0.25),
    Row(2319, "Vulnerability Self VII", 38, 7, 325, 31, 0x98, 0x10, 240.0, 70, false, (8, 30, 44, 60), 0.0, 0.0),
    Row(4197, "Naked to the Elements", 38, 7, 350, 31, 0x93, 0x10, 30.0, 70, false, (8, 30, 44, 50), 5.0, 0.25),
    Row(3928, "Disarmament", 38, 7, 400, 31, 0x93, 0x10, 60.0, 70, false, (8, 30, 44, 50), 5.0, 0.25),
    Row(3962, "Bound to the Law", 38, 7, 400, 31, 0x93, 0x10, 240.0, 70, false, (8, 30, 44, 50), 5.0, 0.25),
    Row(4114, "Hypnotic Suggestion", 38, 1, 400, 31, 0x93, 0x10, 30.0, 70, false, (8, 30, 44, 50), 5.0, 0.25),
    Row(4633, "Incantation of Vulnerability Other", 38, 8, 400, 31, 0x93, 0x10, 300.0, 80, false, (8, 30, 44, 50), 5.0, 0.25),
    // Imperil Other I
    Row(25, "Imperil Other I", 116, 1, 25, 33, 0x13, 0x10, 60.0, 10, false, (14, 34, 40, 54), 5.0, 1.0),
    Row(5115, "Expose Weakness I", 116, 1, 25, 33, 0x13, 0x10, 60.0, 10, false, (14, 34, 40, 54), 5.0, 1.0),
    Row(1327, "Imperil Other VI", 116, 6, 275, 33, 0x13, 0x10, 210.0, 60, false, (14, 34, 40, 54), 5.0, 0.25),
    Row(5120, "Expose Weakness VI", 116, 6, 275, 33, 0x13, 0x10, 210.0, 60, false, (14, 34, 40, 54), 5.0, 0.25),
    Row(2074, "Gossamer Flesh", 116, 7, 325, 33, 0x93, 0x10, 240.0, 70, false, (14, 34, 40, 54), 5.0, 0.25),
    Row(2075, "Imperil Self VII", 116, 7, 325, 33, 0x9a, 0x10, 240.0, 70, false, (14, 34, 40, 62), 0.0, 0.0),
    Row(4087, "Armor Breach", 116, 7, 325, 33, 0x93, 0x10, 10.0, 70, false, (14, 34, 40, 54), 5.0, 0.25),
    Row(5121, "Expose Weakness VII", 116, 7, 325, 33, 0x93, 0x10, 240.0, 70, false, (14, 34, 40, 54), 5.0, 0.25),
    Row(4312, "Incantation of Imperil Other", 116, 8, 400, 33, 0x93, 0x10, 300.0, 80, false, (14, 34, 40, 54), 5.0, 0.25),
    Row(5114, "Expose Weakness VIII", 116, 8, 400, 33, 0x93, 0x10, 300.0, 80, false, (14, 34, 40, 54), 5.0, 0.25),
    Row(3000, "Ire of the Dark Prince", 116, 6, 500, 33, 0x13, 0x10, 10.0, 60, false, (14, 34, 40, 54), 5.0, 0.25),
    Row(3916, "Flayed Flesh", 116, 7, 500, 33, 0x93, 0x10, 10.0, 70, false, (14, 34, 40, 54), 5.0, 0.25),
    // Magic Yield Other I
    Row(280, "Magic Yield Other I", 42, 1, 1, 31, 0x13, 0x10, 60.0, 10, false, (8, 30, 41, 50), 5.0, 1.0),
    Row(1846, "Curse of Black Fire", 42, 6, 50, 31, 0x92, 0x10, 1020.0, 60, false, (8, 30, 41, 50), 5.0, 0.25),
    Row(285, "Magic Yield Other VI", 42, 6, 275, 31, 0x13, 0x10, 210.0, 60, false, (8, 30, 41, 50), 5.0, 0.25),
    Row(2282, "Futility", 42, 7, 325, 31, 0x93, 0x10, 240.0, 70, false, (8, 30, 41, 50), 5.0, 0.25),
    Row(2283, "Magic Yield Self VII", 42, 7, 325, 31, 0x9a, 0x10, 240.0, 70, false, (8, 30, 41, 60), 0.0, 0.0),
    Row(4194, "Magical Void", 42, 7, 350, 31, 0x93, 0x10, 30.0, 70, false, (8, 30, 41, 50), 5.0, 0.25),
    Row(3886, "Magic Disarmament", 42, 6, 400, 31, 0x93, 0x10, 60.0, 70, false, (8, 30, 41, 50), 10.0, 0.25),
    Row(4116, "Trance", 42, 1, 400, 31, 0x93, 0x10, 30.0, 70, false, (8, 30, 41, 50), 10.0, 0.25),
    Row(4597, "Incantation of Magic Yield Other", 42, 8, 400, 31, 0x93, 0x10, 300.0, 80, false, (8, 30, 41, 50), 5.0, 0.25),
    // Weakening Curse I
    Row(5379, "Weakening Curse I", 642, 1, 25, 43, 0x13, 0x10, 15.0, 10, false, (195, 0, 198, 52), 20.0, 0.0),
    Row(5384, "Weakening Curse VI", 642, 6, 275, 43, 0x13, 0x10, 15.0, 60, false, (195, 0, 198, 52), 20.0, 0.0),
    Row(5385, "Weakening Curse VII", 642, 7, 325, 43, 0x13, 0x10, 15.0, 70, false, (195, 0, 198, 52), 20.0, 0.0),
    Row(5386, "Incantation of Weakening Curse", 642, 8, 400, 43, 0x13, 0x10, 15.0, 80, false, (195, 0, 198, 52), 20.0, 0.0),
    // Festering Curse I
    Row(5371, "Festering Curse I", 643, 1, 25, 43, 0x93, 0x10, 30.0, 10, false, (196, 0, 198, 52), 20.0, 0.0),
    Row(5376, "Festering Curse VI", 643, 6, 275, 43, 0x93, 0x10, 30.0, 60, false, (196, 0, 198, 52), 20.0, 0.0),
    Row(5377, "Festering Curse VII", 643, 7, 325, 43, 0x93, 0x10, 30.0, 70, false, (196, 0, 198, 52), 20.0, 0.0),
    Row(5378, "Incantation of Festering Curse", 643, 8, 400, 43, 0x93, 0x10, 30.0, 80, false, (196, 0, 198, 52), 20.0, 0.0),
    // Corruption I
    Row(5395, "Corruption I", 638, 1, 1, 43, 0x10003, 0x10, 0.0, 10, false, (18, 0, 197, 55), 30.0, 0.7),
    Row(5400, "Corruption VI", 638, 6, 250, 43, 0x10003, 0x10, 0.0, 60, false, (18, 0, 197, 55), 30.0, 0.2),
    Row(5401, "Corruption VII", 638, 7, 300, 43, 0x10003, 0x10, 0.0, 70, false, (18, 0, 197, 55), 30.0, 0.2),
    Row(5402, "Incantation of Corruption", 638, 8, 400, 43, 0x10003, 0x10, 0.0, 80, false, (18, 0, 197, 55), 30.0, 0.2),
    // Destructive Curse I
    Row(5339, "Destructive Curse I", 636, 1, 25, 43, 0x10093, 0x10, 30.0, 10, false, (194, 0, 197, 52), 10.0, 0.0),
    Row(5344, "Destructive Curse VI", 636, 6, 275, 43, 0x10093, 0x10, 30.0, 60, false, (194, 0, 197, 52), 10.0, 0.0),
    Row(5337, "Destructive Curse VII", 636, 7, 325, 43, 0x10093, 0x10, 30.0, 70, false, (194, 0, 197, 52), 10.0, 0.0),
    Row(5338, "Incantation of Destructive Curse", 636, 8, 400, 43, 0x10093, 0x10, 30.0, 80, false, (194, 0, 197, 52), 10.0, 0.0),
    Row(5334, "Bael'zharons Curse of Destruction", 636, 8, 700, 34, 0x93, 0x10, 30.0, 80, false, (13, 34, 40, 54), 35.0, 0.0),
    // Corrosion I
    Row(5387, "Corrosion I", 637, 1, 1, 43, 0x10003, 0x10, 0.0, 5, false, (15, 0, 197, 55), 30.0, 0.7),
    Row(5392, "Corrosion VI", 637, 6, 250, 43, 0x10003, 0x10, 0.0, 30, false, (15, 0, 197, 55), 30.0, 0.2),
    Row(5393, "Corrosion VII", 637, 7, 300, 43, 0x10003, 0x10, 0.0, 35, false, (15, 0, 197, 55), 30.0, 0.2),
    Row(5394, "Incantation of Corrosion", 637, 8, 400, 43, 0x10003, 0x10, 0.0, 40, false, (15, 0, 197, 55), 30.0, 0.2),
    // Acid Vulnerability Other I
    Row(521, "Acid Vulnerability Other I", 102, 1, 25, 33, 0x13, 0x10, 60.0, 10, false, (14, 34, 42, 54), 5.0, 1.0),
    Row(526, "Acid Vulnerability Other VI", 102, 6, 275, 33, 0x13, 0x10, 210.0, 60, false, (14, 34, 42, 54), 5.0, 0.25),
    Row(3076, "Exposed Flesh", 102, 1, 300, 33, 0x13, 0x10, 120.0, 10, false, (7, 26, 41, 54), 5.0, 1.0),
    Row(2162, "Olthoi's Gift", 102, 7, 325, 33, 0x93, 0x10, 240.0, 70, false, (14, 34, 42, 54), 5.0, 0.25),
    Row(2163, "Acid Vulnerability Self VII", 102, 7, 325, 33, 0x9a, 0x10, 240.0, 70, false, (14, 34, 42, 62), 0.0, 0.0),
    Row(3462, "Canker Flesh", 102, 7, 350, 33, 0x93, 0x10, 60.0, 10, false, (14, 34, 42, 54), 5.0, 0.25),
    Row(3875, "Acidic Curse", 102, 6, 400, 33, 0x2092, 0x10, 240.0, 70, false, (14, 34, 42, 54), 10.0, 0.25),
    Row(4473, "Incantation of Acid Vulnerability Other", 102, 8, 400, 33, 0x93, 0x10, 300.0, 80, false, (14, 34, 42, 54), 5.0, 0.25),
    // Bludgeoning Vulnerability Other I
    Row(1048, "Bludgeoning Vulnerability Other I", 104, 1, 25, 33, 0x13, 0x10, 60.0, 10, false, (14, 34, 48, 54), 5.0, 1.0),
    Row(1053, "Bludgeoning Vulnerability Other VI", 104, 6, 275, 33, 0x13, 0x10, 210.0, 60, false, (14, 34, 48, 54), 5.0, 0.25),
    Row(2166, "Tusker's Gift", 104, 7, 325, 33, 0x93, 0x10, 240.0, 70, false, (14, 34, 48, 54), 5.0, 0.25),
    Row(2167, "Bludgeoning Vulnerability Self VII", 104, 7, 325, 33, 0x9a, 0x10, 240.0, 70, false, (14, 34, 48, 62), 0.0, 0.0),
    Row(3461, "Batter Flesh", 104, 7, 350, 33, 0x93, 0x10, 60.0, 10, false, (14, 34, 48, 54), 5.0, 0.25),
    Row(4477, "Incantation of Bludgeoning Vulnerability Other", 104, 8, 400, 33, 0x93, 0x10, 300.0, 80, false, (14, 34, 48, 54), 5.0, 0.25),
    // Cold Vulnerability Other I
    Row(1060, "Cold Vulnerability Other I", 106, 1, 25, 33, 0x13, 0x10, 60.0, 10, false, (14, 34, 39, 54), 5.0, 1.0),
    Row(1065, "Cold Vulnerability Other VI", 106, 6, 275, 33, 0x13, 0x10, 210.0, 60, false, (14, 34, 39, 54), 5.0, 0.25),
    Row(2168, "Gelidite's Gift", 106, 7, 325, 33, 0x93, 0x10, 240.0, 70, false, (14, 34, 39, 54), 5.0, 0.25),
    Row(2169, "Cold Vulnerability Self VII", 106, 7, 325, 33, 0x9a, 0x10, 240.0, 70, false, (14, 34, 39, 62), 0.0, 0.0),
    Row(3464, "Numb Flesh", 106, 7, 350, 33, 0x93, 0x10, 60.0, 10, false, (14, 34, 39, 54), 5.0, 0.25),
    Row(4479, "Incantation of Cold Vulnerability Other", 106, 8, 400, 33, 0x93, 0x10, 300.0, 80, false, (14, 34, 39, 54), 5.0, 0.25),
    // Fire Vulnerability Other I
    Row(21, "Fire Vulnerability Other I", 110, 1, 25, 33, 0x13, 0x10, 60.0, 10, false, (14, 34, 45, 54), 5.0, 1.0),
    Row(1108, "Fire Vulnerability Other VI", 110, 6, 275, 33, 0x13, 0x10, 210.0, 60, false, (14, 34, 45, 54), 5.0, 0.25),
    Row(2170, "Inferno's Gift", 110, 7, 325, 33, 0x93, 0x10, 240.0, 70, false, (14, 34, 45, 54), 5.0, 0.25),
    Row(2171, "Fire Vulnerability Self VII", 110, 7, 325, 33, 0x9a, 0x10, 240.0, 70, false, (14, 34, 45, 62), 0.0, 0.0),
    Row(3463, "Char Flesh", 110, 7, 350, 33, 0x93, 0x10, 60.0, 10, false, (14, 34, 45, 54), 5.0, 0.25),
    Row(4481, "Incantation of Fire Vulnerability Other", 110, 8, 400, 33, 0x93, 0x10, 300.0, 80, false, (14, 34, 45, 54), 5.0, 0.25),
    // Lightning Vulnerability Other I
    Row(1084, "Lightning Vulnerability Other I", 108, 1, 25, 33, 0x13, 0x10, 60.0, 10, false, (14, 34, 37, 54), 5.0, 1.0),
    Row(1089, "Lightning Vulnerability Other VI", 108, 6, 275, 33, 0x13, 0x10, 210.0, 60, false, (14, 34, 37, 54), 5.0, 0.25),
    Row(2172, "Astyrrian's Gift", 108, 7, 325, 33, 0x93, 0x10, 240.0, 70, false, (14, 34, 37, 54), 5.0, 0.25),
    Row(2173, "Lightning Vulnerability Self VII", 108, 7, 325, 33, 0x9a, 0x10, 240.0, 70, false, (14, 34, 37, 62), 0.0, 0.0),
    Row(4483, "Incantation of Lightning Vulnerability Other", 108, 8, 400, 33, 0x93, 0x10, 300.0, 80, false, (14, 34, 37, 54), 5.0, 0.25),
    // Piercing Vulnerability Other I
    Row(1151, "Piercing Vulnerability Other I", 112, 1, 25, 33, 0x13, 0x10, 60.0, 10, false, (14, 34, 43, 54), 5.0, 1.0),
    Row(1156, "Piercing Vulnerability Other VI", 112, 6, 275, 33, 0x13, 0x10, 210.0, 60, false, (14, 34, 43, 54), 5.0, 0.25),
    Row(2174, "Archer's Gift", 112, 7, 325, 33, 0x93, 0x10, 240.0, 70, false, (14, 34, 43, 54), 5.0, 0.25),
    Row(2175, "Piercing Vulnerability Self VII", 112, 7, 325, 33, 0x9a, 0x10, 240.0, 70, false, (14, 34, 43, 62), 0.0, 0.0),
    Row(4485, "Incantation of Piercing Vulnerability Other", 112, 8, 400, 33, 0x93, 0x10, 300.0, 80, false, (14, 34, 43, 54), 5.0, 0.25),
    // Blade Vulnerability Other I
    Row(1127, "Blade Vulnerability Other I", 114, 1, 25, 33, 0x13, 0x10, 60.0, 10, false, (14, 34, 46, 54), 5.0, 1.0),
    Row(1132, "Blade Vulnerability Other VI", 114, 6, 275, 33, 0x13, 0x10, 210.0, 60, false, (14, 34, 46, 54), 5.0, 0.25),
    Row(2164, "Swordsman's Gift", 114, 7, 325, 33, 0x93, 0x10, 240.0, 70, false, (14, 34, 46, 54), 5.0, 0.25),
    Row(2165, "Blade Vulnerability Self VII", 114, 7, 325, 33, 0x9a, 0x10, 240.0, 70, false, (14, 34, 46, 62), 0.0, 0.0),
    Row(4475, "Incantation of Blade Vulnerability Other", 114, 8, 400, 33, 0x93, 0x10, 300.0, 80, false, (14, 34, 46, 54), 5.0, 0.25),
    // The item spells that raise a weapon's damage of one element: no debuff.
    Row(4411, "Incantation of Lure Blade", 157, 8, 400, 32, 0x91, 0x88b8f, 300.0, 80, false, (8, 36, 46, 57), 25.0, 0.0),
    Row(1504, "Acid Lure VI", 163, 6, 275, 32, 0x11, 0x88b8f, 210.0, 60, false, (8, 34, 42, 57), 25.0, 0.0),
    Row(2093, "Olthoi Bait", 163, 7, 325, 32, 0x91, 0x88b8f, 240.0, 70, false, (8, 34, 42, 57), 25.0, 0.0),
    Row(4392, "Incantation of Acid Lure", 163, 8, 400, 32, 0x91, 0x88b8f, 300.0, 80, false, (8, 34, 42, 57), 25.0, 0.0),
    Row(1510, "Bludgeon Lure VI", 165, 6, 275, 32, 0x11, 0x88b8f, 210.0, 60, false, (8, 34, 48, 57), 25.0, 0.0),
    Row(2099, "Tusker Bait", 165, 7, 325, 32, 0x91, 0x88b8f, 240.0, 70, false, (8, 34, 48, 57), 25.0, 0.0),
    Row(4398, "Incantation of Bludgeon Lure", 165, 8, 400, 32, 0x91, 0x88b8f, 300.0, 80, false, (8, 34, 48, 57), 25.0, 0.0),
    Row(1522, "Frost Lure VI", 167, 6, 275, 32, 0x11, 0x88b8f, 210.0, 60, false, (8, 34, 39, 57), 25.0, 0.0),
    Row(2105, "Gelidite Bait", 167, 7, 325, 32, 0x91, 0x88b8f, 240.0, 70, false, (8, 34, 39, 57), 25.0, 0.0),
    Row(4404, "Incantation of Frost Lure", 167, 8, 400, 32, 0x91, 0x88b8f, 300.0, 80, false, (8, 34, 39, 57), 25.0, 0.0),
    Row(1534, "Lightning Lure VI", 169, 6, 275, 32, 0x11, 0x88b8f, 210.0, 60, false, (8, 34, 37, 57), 25.0, 0.0),
    Row(2111, "Astyrrian Bait", 169, 7, 325, 32, 0x91, 0x88b8f, 240.0, 70, false, (8, 34, 37, 57), 25.0, 0.0),
    Row(4410, "Incantation of Lightning Lure", 169, 8, 400, 32, 0x91, 0x88b8f, 300.0, 80, false, (8, 34, 37, 57), 25.0, 0.0),
    Row(1546, "Flame Lure VI", 171, 6, 275, 32, 0x11, 0x88b8f, 210.0, 60, false, (8, 34, 45, 57), 25.0, 0.0),
    Row(2103, "Inferno Bait", 171, 7, 325, 32, 0x91, 0x88b8f, 240.0, 70, false, (8, 34, 45, 57), 25.0, 0.0),
    Row(4402, "Incantation of Flame Lure", 171, 8, 400, 32, 0x91, 0x88b8f, 300.0, 80, false, (8, 34, 45, 57), 25.0, 0.0),
    Row(1568, "Piercing Lure VI", 173, 6, 275, 32, 0x11, 0x88b8f, 210.0, 60, false, (8, 34, 43, 57), 25.0, 0.0),
    Row(2114, "Archer Bait", 173, 7, 325, 32, 0x91, 0x88b8f, 240.0, 70, false, (8, 34, 43, 57), 25.0, 0.0),
    Row(4413, "Incantation of Piercing Lure", 173, 8, 400, 32, 0x91, 0x88b8f, 300.0, 80, false, (8, 34, 43, 57), 25.0, 0.0),
    Row(1557, "Blade Lure VI", 175, 6, 275, 32, 0x11, 0x88b8f, 210.0, 60, false, (8, 34, 46, 57), 25.0, 0.0),
    Row(2095, "Swordsman Bait", 175, 7, 325, 32, 0x91, 0x88b8f, 240.0, 70, false, (8, 34, 46, 57), 25.0, 0.0),
    Row(4394, "Incantation of Blade Lure", 175, 8, 400, 32, 0x91, 0x88b8f, 300.0, 80, false, (8, 34, 46, 57), 25.0, 0.0),
];

/// Every spell of the fixture.
#[must_use]
pub fn debuff_spells() -> Vec<SpellInfo> {
    ROWS.iter().map(Row::spell).collect()
}

/// The fixture's spell of this name.
///
/// # Panics
/// When the fixture has no such spell.
#[must_use]
pub fn debuff_spell(name: &str) -> SpellInfo {
    ROWS.iter()
        .find(|row| row.1 == name)
        .map_or_else(|| panic!("no fixture spell named {name}"), Row::spell)
}

/// `spell` given the family, school and formula of the fixture's first
/// spell with its name less the level. The tests ported from the C# name
/// their spells by line and level ("Imperil Other VII", a name the game
/// does not have) and number families as they like; this puts such a spell
/// where the table has its line. A name the fixture has no line for is
/// returned as it is.
#[must_use]
pub fn in_debuff_line(mut spell: SpellInfo) -> SpellInfo {
    let line = mosstank_data::attack_spells::base_name(&spell.name);
    let Some(row) = ROWS
        .iter()
        .find(|row| mosstank_data::attack_spells::base_name(row.1).eq_ignore_ascii_case(&line))
    else {
        return spell;
    };
    spell.family = row.2;
    spell.school = row.5;
    spell.component_set = row.11;
    spell
}

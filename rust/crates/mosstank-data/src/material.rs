//! Materials, by the id the server sends, and their names.

macro_rules! materials {
    ($($variant:ident = $id:literal => $name:expr,)*) => {
        /// Every material an item or a bag of salvage can be made of. Five
        /// ids (cloth, gem, metal, stone and wood) are group headings rather
        /// than materials; they keep the numbering the server's and name
        /// nothing.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[allow(missing_docs)]
        pub enum Material {
            $($variant = $id,)*
        }

        impl Material {
            /// Every material, lowest id first.
            pub const ALL: &'static [Self] = &[$(Self::$variant,)*];

            /// The material with this id.
            #[must_use]
            pub fn from_id(id: i32) -> Option<Self> {
                match id {
                    $($id => Some(Self::$variant),)*
                    _ => None,
                }
            }

            /// The material's name as item names spell it ("Reed Shark
            /// Hide"); `None` for a group heading.
            #[must_use]
            pub fn name(self) -> Option<&'static str> {
                match self {
                    $(Self::$variant => $name,)*
                }
            }

            /// The id the server sends.
            #[must_use]
            pub fn id(self) -> i32 {
                self as i32
            }
        }
    };
}

materials! {
    Ceramic = 1 => Some("Ceramic"),
    Porcelain = 2 => Some("Porcelain"),
    Cloth = 3 => None,
    Linen = 4 => Some("Linen"),
    Satin = 5 => Some("Satin"),
    Silk = 6 => Some("Silk"),
    Velvet = 7 => Some("Velvet"),
    Wool = 8 => Some("Wool"),
    Gem = 9 => None,
    Agate = 10 => Some("Agate"),
    Amber = 11 => Some("Amber"),
    Amethyst = 12 => Some("Amethyst"),
    Aquamarine = 13 => Some("Aquamarine"),
    Azurite = 14 => Some("Azurite"),
    BlackGarnet = 15 => Some("Black Garnet"),
    BlackOpal = 16 => Some("Black Opal"),
    Bloodstone = 17 => Some("Bloodstone"),
    Carnelian = 18 => Some("Carnelian"),
    Citrine = 19 => Some("Citrine"),
    Diamond = 20 => Some("Diamond"),
    Emerald = 21 => Some("Emerald"),
    FireOpal = 22 => Some("Fire Opal"),
    GreenGarnet = 23 => Some("Green Garnet"),
    GreenJade = 24 => Some("Green Jade"),
    Hematite = 25 => Some("Hematite"),
    ImperialTopaz = 26 => Some("Imperial Topaz"),
    Jet = 27 => Some("Jet"),
    LapisLazuli = 28 => Some("Lapis Lazuli"),
    LavenderJade = 29 => Some("Lavender Jade"),
    Malachite = 30 => Some("Malachite"),
    Moonstone = 31 => Some("Moonstone"),
    Onyx = 32 => Some("Onyx"),
    Opal = 33 => Some("Opal"),
    Peridot = 34 => Some("Peridot"),
    RedGarnet = 35 => Some("Red Garnet"),
    RedJade = 36 => Some("Red Jade"),
    RoseQuartz = 37 => Some("Rose Quartz"),
    Ruby = 38 => Some("Ruby"),
    Sapphire = 39 => Some("Sapphire"),
    SmokeyQuartz = 40 => Some("Smokey Quartz"),
    Sunstone = 41 => Some("Sunstone"),
    TigerEye = 42 => Some("Tiger Eye"),
    Tourmaline = 43 => Some("Tourmaline"),
    Turquoise = 44 => Some("Turquoise"),
    WhiteJade = 45 => Some("White Jade"),
    WhiteQuartz = 46 => Some("White Quartz"),
    WhiteSapphire = 47 => Some("White Sapphire"),
    YellowGarnet = 48 => Some("Yellow Garnet"),
    YellowTopaz = 49 => Some("Yellow Topaz"),
    Zircon = 50 => Some("Zircon"),
    Ivory = 51 => Some("Ivory"),
    Leather = 52 => Some("Leather"),
    ArmoredilloHide = 53 => Some("Armoredillo Hide"),
    GromnieHide = 54 => Some("Gromnie Hide"),
    ReedSharkHide = 55 => Some("Reed Shark Hide"),
    Metal = 56 => None,
    Brass = 57 => Some("Brass"),
    Bronze = 58 => Some("Bronze"),
    Copper = 59 => Some("Copper"),
    Gold = 60 => Some("Gold"),
    Iron = 61 => Some("Iron"),
    Pyreal = 62 => Some("Pyreal"),
    Silver = 63 => Some("Silver"),
    Steel = 64 => Some("Steel"),
    Stone = 65 => None,
    Alabaster = 66 => Some("Alabaster"),
    Granite = 67 => Some("Granite"),
    Marble = 68 => Some("Marble"),
    Obsidian = 69 => Some("Obsidian"),
    Sandstone = 70 => Some("Sandstone"),
    Serpentine = 71 => Some("Serpentine"),
    Wood = 72 => None,
    Ebony = 73 => Some("Ebony"),
    Mahogany = 74 => Some("Mahogany"),
    Oak = 75 => Some("Oak"),
    Pine = 76 => Some("Pine"),
    Teak = 77 => Some("Teak"),
}

/// The name in front of an item's own name for a material id ("Silver" in
/// "Silver Long Sword"); `None` for a group heading or an unknown id.
#[must_use]
pub fn material_name(id: i32) -> Option<&'static str> {
    Material::from_id(id).and_then(Material::name)
}

/// The material's name, or the empty string when the id names none.
#[must_use]
pub fn name(id: i32) -> &'static str {
    material_name(id).unwrap_or("")
}

/// The id of the material with this name, ignoring case and surrounding
/// space; 0 when no material carries it.
#[must_use]
pub fn id(name: &str) -> i32 {
    let name = name.trim();
    Material::ALL
        .iter()
        .find(|material| {
            material
                .name()
                .is_some_and(|own| own.eq_ignore_ascii_case(name))
        })
        .map_or(0, |material| material.id())
}

/// Every material that names something, lowest id first.
pub fn named() -> impl Iterator<Item = Material> {
    Material::ALL
        .iter()
        .copied()
        .filter(|material| material.name().is_some())
}

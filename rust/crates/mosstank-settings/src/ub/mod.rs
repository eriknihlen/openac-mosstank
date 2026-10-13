//! The UB tab's settings: their value shapes ([`UbSettingValue`]), the
//! catalogue as authored ([`definitions`]), and the catalogue bound to where
//! each value is kept ([`UbSettingCatalog`]).
//!
//! Values live in three files: the installation's (global), a named profile
//! someone can share, and the character's own. Reading walks character,
//! profile, global, then the default; a write lands in the one tier the
//! setting declares.

pub mod definitions;

use std::collections::HashMap;
use std::sync::Arc;

use mosstank_dotnet::compat::number::{self, NumberStyles};
use mosstank_dotnet::{
    double_to_int, format_g15, parse_bool, round_away_from_zero, to_upper_invariant,
    try_parse_double, try_parse_float, try_parse_int, try_parse_uint, utf16_len,
};

use crate::defaults::VtankEnumValue;

/// The value shapes a UB setting can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UbSettingKind {
    /// A switch.
    Bool,
    /// A whole number.
    Int,
    /// A decimal number kept at single precision.
    Single,
    /// A decimal number kept at double precision.
    Double,
    /// Free text.
    String,
    /// One of a fixed set of choices.
    Enum,
    /// A colour, 32 bits of alpha, red, green and blue.
    Color,
    /// A list of lines.
    Collection,
}

/// One setting's value; the shape travels with the payload.
#[derive(Debug, Clone, PartialEq)]
pub struct UbSettingValue {
    /// Which shape this value holds.
    pub kind: UbSettingKind,
    /// The switch position, for a switch.
    pub boolean: bool,
    /// The numeric payload, for every numeric shape.
    pub number: f64,
    /// The text, for free text.
    pub text: String,
    /// The lines, for a list.
    pub items: Vec<String>,
}

impl UbSettingValue {
    fn new(kind: UbSettingKind, boolean: bool, number: f64) -> Self {
        Self {
            kind,
            boolean,
            number,
            text: String::new(),
            items: Vec::new(),
        }
    }

    /// A switch.
    #[must_use]
    pub fn from_bool(value: bool) -> Self {
        Self::new(UbSettingKind::Bool, value, if value { 1.0 } else { 0.0 })
    }

    /// A whole number.
    #[must_use]
    pub fn from_int(value: i32) -> Self {
        Self::new(UbSettingKind::Int, false, f64::from(value))
    }

    /// A decimal number at single precision.
    #[must_use]
    pub fn from_single(value: f32) -> Self {
        Self::new(UbSettingKind::Single, false, f64::from(value))
    }

    /// A decimal number at double precision.
    #[must_use]
    pub fn from_double(value: f64) -> Self {
        Self::new(UbSettingKind::Double, false, value)
    }

    /// Free text.
    #[must_use]
    pub fn from_text(value: &str) -> Self {
        Self {
            text: value.to_owned(),
            ..Self::new(UbSettingKind::String, false, 0.0)
        }
    }

    /// One of a fixed set of choices, by its number.
    #[must_use]
    pub fn from_choice(value: i32) -> Self {
        Self::new(UbSettingKind::Enum, false, f64::from(value))
    }

    /// A colour as 32 bits of alpha, red, green and blue.
    #[must_use]
    pub fn from_color(argb: u32) -> Self {
        Self::new(UbSettingKind::Color, false, f64::from(argb))
    }

    /// A list of lines.
    #[must_use]
    pub fn from_collection<S: AsRef<str>>(items: &[S]) -> Self {
        Self {
            items: items.iter().map(|item| item.as_ref().to_owned()).collect(),
            ..Self::new(UbSettingKind::Collection, false, 0.0)
        }
    }

    /// The value as a whole number, rounding a decimal one.
    #[must_use]
    pub fn as_int32(&self) -> i32 {
        double_to_int(round_away_from_zero(self.number))
    }

    /// The value at single precision.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn as_single(&self) -> f32 {
        self.number as f32
    }

    /// The value at double precision.
    #[must_use]
    pub fn as_double(&self) -> f64 {
        self.number
    }

    /// The value as a colour (`(uint)(long)Math.Round(...)`).
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn as_color(&self) -> u32 {
        (round_away_from_zero(self.number) as i64) as u32
    }

    /// Exactly what goes into the file and what a person types into the
    /// edit field, the same on every machine. A single-precision number is
    /// printed from the float (its shortest form), not from the double it
    /// is carried in.
    #[must_use]
    pub fn to_storage_string(&self) -> String {
        match self.kind {
            UbSettingKind::Bool => if self.boolean { "True" } else { "False" }.to_owned(),
            UbSettingKind::Int | UbSettingKind::Enum => self.as_int32().to_string(),
            UbSettingKind::Single => number::to_string_f32(self.as_single()),
            UbSettingKind::Double => format_g15(self.number),
            UbSettingKind::String => self.text.clone(),
            UbSettingKind::Color => format!("#{:08X}", self.as_color()),
            UbSettingKind::Collection => self.items.join("\n"),
        }
    }

    /// Reads a value of one shape back from its text; a value that does not
    /// read is refused rather than guessed at.
    #[must_use]
    pub fn try_parse(kind: UbSettingKind, text: &str) -> Option<Self> {
        let trimmed = text.trim();
        match kind {
            UbSettingKind::Bool => parse_bool(trimmed).ok().map(Self::from_bool),
            UbSettingKind::Int => try_parse_int(trimmed).map(Self::from_int),
            UbSettingKind::Enum => try_parse_int(trimmed).map(Self::from_choice),
            UbSettingKind::Single => try_parse_float(trimmed).map(Self::from_single),
            UbSettingKind::Double => try_parse_double(trimmed).map(Self::from_double),
            UbSettingKind::String => Some(Self::from_text(text)),
            UbSettingKind::Color => try_parse_color(trimmed).map(Self::from_color),
            UbSettingKind::Collection => {
                let items: Vec<&str> = if text.is_empty() {
                    Vec::new()
                } else {
                    text.split('\n')
                        .filter(|line| !line.is_empty())
                        .map(|line| line.trim_matches('\r'))
                        .collect()
                };
                Some(Self::from_collection(&items))
            }
        }
    }
}

/// A colour: `#AARRGGBB` as written, read back also without the hash,
/// without the alpha byte, and as a signed or unsigned 32-bit number. A
/// bare run of digits is a decimal; hex needs a hash or a letter.
fn try_parse_color(text: &str) -> Option<u32> {
    if text.is_empty() {
        return None;
    }
    let hashed = text.starts_with('#');
    let body = if hashed { &text[1..] } else { text };
    if !hashed {
        if let Some(signed) = try_parse_int(text) {
            return Some(signed.cast_unsigned());
        }
        if let Some(unsigned) = try_parse_uint(text) {
            return Some(unsigned);
        }
    }
    let hex = |digits: &str| number::parse_u32(digits, NumberStyles::HEX_NUMBER);
    match utf16_len(body) {
        6 => hex(body).map(|rgb| 0xFF00_0000 | rgb),
        8 => hex(body),
        _ => None,
    }
}

/// Which of the three files a setting is written to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UbSettingScope {
    /// One value for this installation, whoever is logged in.
    Global,
    /// A named, shareable set.
    Profile,
    /// One value for this character on this server.
    Character,
}

impl UbSettingScope {
    /// Every tier.
    pub const ALL: [Self; 3] = [Self::Global, Self::Profile, Self::Character];
}

/// One row of the catalogue as authored.
#[derive(Debug, Clone, PartialEq)]
pub struct UbSettingDefinition {
    /// The full dotted name, which is also the storage key.
    pub name: String,
    /// One line of prose.
    pub summary: String,
    /// The value shape.
    pub kind: UbSettingKind,
    /// The value used until someone writes one.
    pub default: UbSettingValue,
    /// Which file an edit lands in.
    pub scope: UbSettingScope,
    /// The choices, for a choice row.
    pub choices: Vec<VtankEnumValue>,
}

impl UbSettingDefinition {
    /// A definition with no choices.
    #[must_use]
    pub fn new(
        name: &str,
        summary: &str,
        kind: UbSettingKind,
        default: UbSettingValue,
        scope: UbSettingScope,
    ) -> Self {
        Self {
            name: name.to_owned(),
            summary: summary.to_owned(),
            kind,
            default,
            scope,
            choices: Vec::new(),
        }
    }

    /// The tool the setting belongs to: the dotted name's first part.
    #[must_use]
    pub fn category(&self) -> &str {
        match self.name.find('.') {
            Some(dot) if dot > 0 => &self.name[..dot],
            _ => &self.name,
        }
    }

    /// The short form shown in the value column: a choice as its label, a
    /// list as its lines run together, everything else as stored.
    #[must_use]
    pub fn display(&self, value: &UbSettingValue) -> String {
        if value.kind == UbSettingKind::Enum
            && let Some(choice) = self
                .choices
                .iter()
                .find(|choice| choice.value == value.as_int32())
        {
            return choice.label.clone();
        }
        if value.kind == UbSettingKind::Collection {
            return if value.items.is_empty() {
                "(empty)".to_owned()
            } else {
                value.items.join(", ")
            };
        }
        value.to_storage_string()
    }

    /// The value as the reference's command line writes it: a choice as
    /// its label, a colour as `0x` and eight hex digits, a list as `[a,b]`.
    #[must_use]
    pub fn command_line_display(&self, value: &UbSettingValue) -> String {
        match value.kind {
            UbSettingKind::Enum => self.display(value),
            UbSettingKind::Color => format!("0x{:08X}", value.as_color()),
            UbSettingKind::Collection => format!("[{}]", value.items.join(",")),
            _ => value.to_storage_string(),
        }
    }

    /// The reference's setting type: `State` for a character's own value,
    /// `Profile` otherwise.
    #[must_use]
    pub fn reference_setting_type(&self) -> &'static str {
        if self.scope == UbSettingScope::Character {
            "State"
        } else {
            "Profile"
        }
    }
}

/// Where a live-owned setting's value lives: the pair the page calls.
#[allow(missing_debug_implementations)]
pub struct UbSettingBinding {
    /// Reads the value in force.
    pub get: Box<dyn Fn() -> UbSettingValue>,
    /// Writes a new one.
    pub set: Box<dyn FnMut(UbSettingValue)>,
    /// Whether a write outlives the session.
    pub can_save: Box<dyn Fn() -> bool>,
}

impl UbSettingBinding {
    /// A binding whose writes always save.
    pub fn new(
        get: impl Fn() -> UbSettingValue + 'static,
        set: impl FnMut(UbSettingValue) + 'static,
    ) -> Self {
        Self {
            get: Box::new(get),
            set: Box::new(set),
            can_save: Box::new(|| true),
        }
    }
}

/// Somewhere scoped values are kept.
pub trait UbSettingValues {
    /// The value written at one tier, if any.
    fn read(&self, scope: UbSettingScope, name: &str) -> Option<UbSettingValue>;
    /// Writes one value at one tier.
    fn write(&mut self, scope: UbSettingScope, name: &str, value: UbSettingValue);
    /// Removes one value from one tier, so the wider tier shows through.
    fn clear(&mut self, scope: UbSettingScope, name: &str) -> bool;
    /// Whether a write to one tier survives the session.
    fn is_writable(&self, scope: UbSettingScope) -> bool;
}

/// The three tiers held in memory (names in any case).
#[derive(Debug, Clone, Default)]
pub struct UbSettingValueBag {
    tiers: [HashMap<String, UbSettingValue>; 3],
}

impl UbSettingValueBag {
    fn tier(scope: UbSettingScope) -> usize {
        match scope {
            UbSettingScope::Global => 0,
            UbSettingScope::Profile => 1,
            UbSettingScope::Character => 2,
        }
    }
}

impl UbSettingValues for UbSettingValueBag {
    fn read(&self, scope: UbSettingScope, name: &str) -> Option<UbSettingValue> {
        self.tiers[Self::tier(scope)]
            .get(&to_upper_invariant(name))
            .cloned()
    }

    fn write(&mut self, scope: UbSettingScope, name: &str, value: UbSettingValue) {
        self.tiers[Self::tier(scope)].insert(to_upper_invariant(name), value);
    }

    fn clear(&mut self, scope: UbSettingScope, name: &str) -> bool {
        self.tiers[Self::tier(scope)]
            .remove(&to_upper_invariant(name))
            .is_some()
    }

    /// The bag is the session: nothing here can lose an edit.
    fn is_writable(&self, _scope: UbSettingScope) -> bool {
        true
    }
}

/// One catalogue row: its definition and whether its value has a live
/// owner elsewhere in the plugin rather than the value store.
#[derive(Debug, Clone, PartialEq)]
pub struct UbSetting {
    /// What this setting is.
    pub definition: Arc<UbSettingDefinition>,
    /// Whether the value belongs to something running.
    pub has_live_owner: bool,
}

impl UbSetting {
    /// The full dotted name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.definition.name
    }

    /// The value shape.
    #[must_use]
    pub fn kind(&self) -> UbSettingKind {
        self.definition.kind
    }

    /// Which file an edit lands in.
    #[must_use]
    pub fn scope(&self) -> UbSettingScope {
        self.definition.scope
    }
}

/// No UB setting has this name (`KeyNotFoundException`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("No UB setting is named '{0}'.")]
pub struct UnknownSetting(pub String);

/// Every UB setting, bound to where its value is kept. A row whose value
/// already has a live owner reads and writes that owner, so one number
/// never exists twice.
#[allow(missing_debug_implementations)]
pub struct UbSettingCatalog<V: UbSettingValues> {
    values: V,
    settings: Vec<UbSetting>,
    by_name: HashMap<String, UbSetting>,
    live: HashMap<String, UbSettingBinding>,
    categories: Vec<String>,
}

impl<V: UbSettingValues> UbSettingCatalog<V> {
    /// The catalogue over `values`, with `live_owners` (by name, any case)
    /// taking the rows they name.
    pub fn new(values: V, live_owners: Vec<(String, UbSettingBinding)>) -> Self {
        let live: HashMap<String, UbSettingBinding> = live_owners
            .into_iter()
            .map(|(name, binding)| (to_upper_invariant(&name), binding))
            .collect();
        let bind = |definition: &UbSettingDefinition| UbSetting {
            definition: Arc::new(definition.clone()),
            has_live_owner: live.contains_key(&to_upper_invariant(&definition.name)),
        };
        let settings: Vec<UbSetting> = definitions::all().iter().map(bind).collect();
        let mut by_name: HashMap<String, UbSetting> = settings
            .iter()
            .map(|row| (to_upper_invariant(row.name()), row.clone()))
            .collect();
        // Reachable by name, but not rows on the tab.
        for definition in definitions::accepted_only() {
            by_name.insert(to_upper_invariant(&definition.name), bind(definition));
        }
        for (spelling, name) in definitions::ORIGINAL_SPELLINGS {
            let row = by_name[&to_upper_invariant(name)].clone();
            by_name.insert(to_upper_invariant(spelling), row);
        }
        let mut categories: Vec<String> = Vec::new();
        for row in &settings {
            let category = row.definition.category();
            if !categories.iter().any(|known| known == category) {
                categories.push(category.to_owned());
            }
        }
        categories.sort_by(|a, b| mosstank_dotnet::compare_ordinal(a, b));
        Self {
            values,
            settings,
            by_name,
            live,
            categories,
        }
    }

    /// The value store.
    pub fn values(&self) -> &V {
        &self.values
    }

    /// The value store, to change.
    pub fn values_mut(&mut self) -> &mut V {
        &mut self.values
    }

    /// Every row, in the order the catalogue declares them.
    #[must_use]
    pub fn settings(&self) -> &[UbSetting] {
        &self.settings
    }

    /// The tools, in name order.
    #[must_use]
    pub fn categories(&self) -> &[String] {
        &self.categories
    }

    /// The row of one name: a row on the tab, a name kept only so macros
    /// can set it, or the original's spelling of a row named otherwise here.
    #[must_use]
    pub fn try_get(&self, name: &str) -> Option<UbSetting> {
        self.by_name.get(&to_upper_invariant(name)).cloned()
    }

    /// The row of one name.
    ///
    /// # Errors
    /// The catalogue has no such row.
    pub fn require(&self, name: &str) -> Result<UbSetting, UnknownSetting> {
        self.try_get(name)
            .ok_or_else(|| UnknownSetting(name.to_owned()))
    }

    fn binding(&self, row: &UbSetting) -> Option<&UbSettingBinding> {
        if !row.has_live_owner {
            return None;
        }
        self.live.get(&to_upper_invariant(row.name()))
    }

    /// The value in force: the live owner's, else the character's, the
    /// profile's, the installation's, then the default.
    #[must_use]
    pub fn get(&self, row: &UbSetting) -> UbSettingValue {
        if let Some(binding) = self.binding(row) {
            return (binding.get)();
        }
        let name = row.name();
        self.values
            .read(UbSettingScope::Character, name)
            .or_else(|| self.values.read(UbSettingScope::Profile, name))
            .or_else(|| self.values.read(UbSettingScope::Global, name))
            .unwrap_or_else(|| row.definition.default.clone())
    }

    /// Writes a value: to the live owner, or to the tier the row declares.
    pub fn set(&mut self, row: &UbSetting, value: UbSettingValue) {
        if row.has_live_owner
            && let Some(binding) = self.live.get_mut(&to_upper_invariant(row.name()))
        {
            (binding.set)(value);
            return;
        }
        self.values.write(row.scope(), row.name(), value);
    }

    /// Whether a write to this row survives the session.
    #[must_use]
    pub fn can_save(&self, row: &UbSetting) -> bool {
        match self.binding(row) {
            Some(binding) => (binding.can_save)(),
            None => self.values.is_writable(row.scope()),
        }
    }

    /// The short form for the value column.
    #[must_use]
    pub fn display(&self, row: &UbSetting) -> String {
        row.definition.display(&self.get(row))
    }

    /// The line `/ub opt` prints: `Name (Type) = value`, in the reference's
    /// shape (macros match on it).
    #[must_use]
    pub fn full_display_value(&self, row: &UbSetting) -> String {
        format!(
            "{} ({}) = {}",
            definitions::original_name_of(row.name()),
            row.definition.reference_setting_type(),
            row.definition.command_line_display(&self.get(row))
        )
    }

    /// Puts one row back to what the catalogue says: a stored row has its
    /// own value taken out (so a wider tier shows through again); a live
    /// row is set to the default.
    pub fn reset_to_default(&mut self, row: &UbSetting) {
        if row.has_live_owner {
            let default = row.definition.default.clone();
            self.set(row, default);
        } else {
            self.values.clear(row.scope(), row.name());
        }
    }
}

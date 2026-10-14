//! The side-car written beside each settings profile, in the plugin's own
//! storage (`profiles/macro/sidecar/<profile>.json`): what MossTank keeps
//! that a VTank settings file has no column for (item names, consumable
//! kinds, log channels, monster-rule item names, loot rules, ...), as the
//! C#'s `SideCarDocument` serializes it.

use mosstank_data::damage::MonsterDamageType;
use mosstank_dotnet::collections::{IgnoreCaseStringSet, OrdinalStringSet};
use mosstank_dotnet::text::{casing, ordinal};
use mosstank_formats::route::ClientPathing;
use mosstank_formats::utl::{LootAction, LootRule, VtankLootRequirement};
use mosstank_settings::model::{
    AllSettings, BuffItemEnchantRow, CombatSettings, ConsumableCategory,
};
use mosstank_settings::monster::{
    MonsterActionFlags, MonsterRule, MonsterRuleActions, MonsterValue, MonsterValueKind,
    is_default_name,
};
use openac_plugin_compat::json::{self, JsonValue};
use openac_plugin_compat::number::{self, NumberStyles};

use crate::sidecar_rules;
use crate::stj::{self, JsonError, Object};

/// The non-blank values, once each, in ordinal order (`Sorted`).
#[must_use]
pub fn sorted<'a>(values: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for value in values {
        if casing::is_white_space_only(value) || out.contains(value) {
            continue;
        }
        out.push(value.clone());
    }
    out.sort_by(|a, b| ordinal::compare(a, b));
    out
}

/// The names in the order the Items page shows them: `order` first, then
/// the rest sorted (`InPageOrder`).
fn in_page_order(order: &[String], names: &OrdinalStringSet) -> Vec<String> {
    let mut remaining: Vec<String> = names.iter().cloned().collect();
    let mut result = Vec::new();
    for name in order {
        if let Some(at) = remaining.iter().position(|known| known == name) {
            remaining.remove(at);
            result.push(name.clone());
        }
    }
    result.extend(sorted(&remaining));
    result
}

pub(crate) fn replace_names(target: &mut OrdinalStringSet, values: Option<&[Option<String>]>) {
    target.clear();
    for value in values.unwrap_or_default().iter().flatten() {
        if !casing::is_white_space_only(value) {
            target.insert(value.clone());
        }
    }
}

fn replace_ignore_case(target: &mut IgnoreCaseStringSet, values: Option<&[Option<String>]>) {
    target.clear();
    for value in values.unwrap_or_default().iter().flatten() {
        if !casing::is_white_space_only(value) {
            target.insert(value.clone());
        }
    }
}

pub(crate) fn replace_order_names(target: &mut Vec<String>, values: Option<&[Option<String>]>) {
    target.clear();
    for value in values.unwrap_or_default().iter().flatten() {
        if !casing::is_white_space_only(value) && !target.contains(value) {
            target.push(value.clone());
        }
    }
}

fn some_strings(values: Vec<String>) -> Option<Vec<Option<String>>> {
    Some(values.into_iter().map(Some).collect())
}

fn strings_json(values: Option<&Vec<Option<String>>>) -> JsonValue {
    values.map_or(JsonValue::Null, |items| {
        JsonValue::Array(
            items
                .iter()
                .map(|item| stj::string_or_null(item.as_deref()))
                .collect(),
        )
    })
}

fn consumable_number(category: ConsumableCategory) -> i32 {
    category as i32
}

pub(crate) fn consumable_from(number: i32) -> Option<ConsumableCategory> {
    use ConsumableCategory as C;
    Some(match number {
        0 => C::Other,
        1 => C::HealthKit,
        2 => C::HealthFood,
        3 => C::StaminaKit,
        4 => C::StaminaFood,
        5 => C::ManaKit,
        6 => C::ManaFood,
        7 => C::Pea,
        8 => C::AllPeas,
        9 => C::Lockpick,
        10 => C::ManaStone,
        11 => C::ManaSource,
        12 => C::Grenade,
        13 => C::SplitComponent,
        14 => C::BuffConsumable,
        _ => return None,
    })
}

/// An enum number this build has no member for (register row STORE-5).
fn unknown(path: &str, number: i32) -> JsonError {
    format!("The JSON value could not be converted to an enum: {number} at {path}")
}

fn damage_type(
    object: &Object<'_>,
    name: &str,
    default: MonsterDamageType,
) -> Result<MonsterDamageType, JsonError> {
    let number = object.int(name, default as i32)?;
    MonsterDamageType::from_i32(number).ok_or_else(|| unknown(name, number))
}

/// Option name to its saved value, in the order written.
pub type DynamicSettings = Vec<(String, Option<DynamicSettingDocument>)>;

/// A VTank requirement record: its type and payload.
pub type RequirementDocument = (i32, Option<String>);

/// One dynamic setting (`DynamicSettingDocument`): a monster value by kind.
#[derive(Debug, Clone, PartialEq)]
pub struct DynamicSettingDocument {
    /// `MonsterValueKind` as its number (any number; not a known one reads
    /// as text).
    pub kind: i32,
    /// The number.
    pub number: f64,
    /// The text.
    pub text: Option<String>,
    /// The boolean.
    pub boolean: bool,
}

impl DynamicSettingDocument {
    /// The saved form of a value.
    #[must_use]
    pub fn from_value(value: &MonsterValue) -> Self {
        Self {
            kind: match value.kind {
                MonsterValueKind::Number => 0,
                MonsterValueKind::Text => 1,
                MonsterValueKind::Boolean => 2,
            },
            number: value.number,
            text: Some(value.text.clone()),
            boolean: value.boolean,
        }
    }

    /// The value again.
    #[must_use]
    pub fn to_value(&self) -> MonsterValue {
        match self.kind {
            0 => MonsterValue::from_number(self.number),
            2 => MonsterValue::from_boolean(self.boolean),
            _ => MonsterValue::from_text(self.text.as_deref().unwrap_or_default()),
        }
    }

    fn to_json(&self) -> JsonValue {
        JsonValue::object([
            ("Kind", stj::int(self.kind)),
            ("Number", stj::double(self.number)),
            ("Text", stj::string_or_null(self.text.as_deref())),
            ("Boolean", JsonValue::Bool(self.boolean)),
        ])
    }

    fn read(object: &Object<'_>) -> Result<Self, JsonError> {
        Ok(Self {
            kind: object.int("Kind", 0)?,
            number: object.double("Number", 0.0)?,
            text: object.string("Text", "")?,
            boolean: object.bool("Boolean", false)?,
        })
    }
}

fn dynamic_settings_json(entries: &[(String, Option<DynamicSettingDocument>)]) -> JsonValue {
    JsonValue::object(entries.iter().map(|(name, setting)| {
        (
            name.clone(),
            setting
                .as_ref()
                .map_or(JsonValue::Null, DynamicSettingDocument::to_json),
        )
    }))
}

pub(crate) fn read_dynamic_settings(
    object: &Object<'_>,
    name: &str,
) -> Result<Option<DynamicSettings>, JsonError> {
    let Some(entries) = object.map(name, "Dictionary`2[String,DynamicSettingDocument]")? else {
        return Ok(None);
    };
    let mut out = Vec::new();
    for (key, value) in entries {
        let setting = match Object::of(value, name, "DynamicSettingDocument")? {
            Some(inner) => Some(DynamicSettingDocument::read(&inner)?),
            None => None,
        };
        out.push((key.to_owned(), setting));
    }
    Ok(Some(out))
}

fn capture_dynamic_settings(
    combat: &CombatSettings,
) -> Vec<(String, Option<DynamicSettingDocument>)> {
    combat
        .dynamic_settings
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                Some(DynamicSettingDocument::from_value(value)),
            )
        })
        .collect()
}

fn apply_dynamic_settings(
    combat: &mut CombatSettings,
    entries: Option<&[(String, Option<DynamicSettingDocument>)]>,
) {
    combat.dynamic_settings.clear();
    for (name, setting) in entries.unwrap_or_default() {
        // A null entry would throw in the C# (register row STORE-5).
        if let Some(setting) = setting {
            combat
                .dynamic_settings
                .insert(name.clone(), setting.to_value());
        }
    }
}

/// One loot rule (`LootRuleDocument`).
#[derive(Debug, Clone, PartialEq)]
pub struct LootRuleDocument {
    /// The name.
    pub name: Option<String>,
    /// The expression.
    pub expression: Option<String>,
    /// The action.
    pub action: LootAction,
    /// How many to keep.
    pub keep_count: i32,
    /// The priority.
    pub priority: i32,
    /// The custom expression line.
    pub custom_expression: Option<String>,
    /// The VTank requirement records, by type and payload.
    pub requirements: Option<Vec<Option<RequirementDocument>>>,
    /// Whether the requirements came from a file.
    pub has_imported_requirements: bool,
}

impl LootRuleDocument {
    /// The saved form of a rule.
    #[must_use]
    pub fn from_rule(rule: &LootRule) -> Self {
        Self {
            name: Some(rule.name.clone()),
            expression: Some(rule.expression().to_owned()),
            action: rule.action,
            keep_count: rule.keep_count,
            priority: rule.priority,
            custom_expression: Some(rule.custom_expression.clone()),
            requirements: Some(
                rule.vtank_requirements
                    .iter()
                    .map(|requirement| Some((requirement.kind, Some(requirement.payload.clone()))))
                    .collect(),
            ),
            has_imported_requirements: rule.has_imported_requirements,
        }
    }

    /// [`Self::to_rule`], the document's texts moved into the rule.
    #[must_use]
    pub fn into_rule(self) -> LootRule {
        let name = self
            .name
            .as_deref()
            .filter(|name| !casing::is_white_space_only(name))
            .map_or_else(|| "Rule".to_owned(), |name| name.trim().to_owned());
        let mut rule = LootRule::named(&name, self.expression.as_deref().unwrap_or_default());
        rule.action = self.action;
        rule.keep_count = self.keep_count.clamp(0, 100_000);
        rule.priority = self.priority.clamp(-1000, 1000);
        rule.custom_expression = self.custom_expression.unwrap_or_default();
        rule.has_imported_requirements = self.has_imported_requirements;
        rule.vtank_requirements = self
            .requirements
            .into_iter()
            .flatten()
            .flatten()
            .map(|(kind, payload)| VtankLootRequirement {
                kind,
                payload: payload.unwrap_or_default(),
            })
            .collect();
        rule
    }

    /// The rule again, its numbers held in range.
    #[must_use]
    pub fn to_rule(&self) -> LootRule {
        let name = self
            .name
            .as_deref()
            .filter(|name| !casing::is_white_space_only(name))
            .map_or_else(|| "Rule".to_owned(), |name| name.trim().to_owned());
        let mut rule = LootRule::named(&name, self.expression.as_deref().unwrap_or_default());
        rule.action = self.action;
        rule.keep_count = self.keep_count.clamp(0, 100_000);
        rule.priority = self.priority.clamp(-1000, 1000);
        rule.custom_expression = self.custom_expression.clone().unwrap_or_default();
        rule.has_imported_requirements = self.has_imported_requirements;
        rule.vtank_requirements = self
            .requirements
            .iter()
            .flatten()
            .flatten()
            .map(|(kind, payload)| VtankLootRequirement {
                kind: *kind,
                payload: payload.clone().unwrap_or_default(),
            })
            .collect();
        rule
    }

    /// The JSON object.
    #[must_use]
    pub fn to_json(&self) -> JsonValue {
        JsonValue::object([
            ("Name", stj::string_or_null(self.name.as_deref())),
            (
                "Expression",
                stj::string_or_null(self.expression.as_deref()),
            ),
            ("Action", stj::int(self.action as i32)),
            ("KeepCount", stj::int(self.keep_count)),
            ("Priority", stj::int(self.priority)),
            (
                "CustomExpression",
                stj::string_or_null(self.custom_expression.as_deref()),
            ),
            (
                "Requirements",
                self.requirements.as_ref().map_or(JsonValue::Null, |items| {
                    JsonValue::Array(
                        items
                            .iter()
                            .map(|item| {
                                item.as_ref().map_or(JsonValue::Null, |(kind, payload)| {
                                    JsonValue::object([
                                        ("Type", stj::int(*kind)),
                                        ("Payload", stj::string_or_null(payload.as_deref())),
                                    ])
                                })
                            })
                            .collect(),
                    )
                }),
            ),
            (
                "HasImportedRequirements",
                JsonValue::Bool(self.has_imported_requirements),
            ),
        ])
    }

    /// Reads one.
    ///
    /// # Errors
    /// A value of the wrong kind, or an action this build has no member for.
    pub fn read(object: &Object<'_>) -> Result<Self, JsonError> {
        let action_number = object.int("Action", LootAction::Keep as i32)?;
        let action =
            LootAction::from_i32(action_number).ok_or_else(|| unknown("Action", action_number))?;
        let requirements =
            match object.objects("Requirements", Some(Vec::new()), "VtankLootRequirement")? {
                None => None,
                Some(items) => Some(
                    items
                        .iter()
                        .map(|item| {
                            item.map(|item| -> Result<_, JsonError> {
                                Ok((item.int("Type", 0)?, item.string("Payload", "")?))
                            })
                            .transpose()
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            };
        Ok(Self {
            name: object.string("Name", "Rule")?,
            expression: object.string("Expression", "*")?,
            action,
            keep_count: object.int("KeepCount", 1)?,
            priority: object.int("Priority", 0)?,
            custom_expression: object.string("CustomExpression", "")?,
            requirements,
            has_imported_requirements: object.bool("HasImportedRequirements", false)?,
        })
    }
}

/// The item names one monster rule uses (`MonsterRuleNameDocument`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonsterRuleNameDocument {
    /// The rule's spec.
    pub expression: Option<String>,
    /// The main-hand weapon's name.
    pub weapon_name: Option<String>,
    /// The off-hand item's name.
    pub offhand_name: Option<String>,
}

impl MonsterRuleNameDocument {
    fn from_rule(rule: &MonsterRule) -> Self {
        Self {
            expression: Some(rule.expression().to_owned()),
            weapon_name: Some(rule.actions().weapon_name.clone()),
            offhand_name: Some(rule.actions().offhand_name.clone()),
        }
    }

    fn to_json(&self) -> JsonValue {
        JsonValue::object([
            (
                "Expression",
                stj::string_or_null(self.expression.as_deref()),
            ),
            (
                "WeaponName",
                stj::string_or_null(self.weapon_name.as_deref()),
            ),
            (
                "OffhandName",
                stj::string_or_null(self.offhand_name.as_deref()),
            ),
        ])
    }

    fn read(object: &Object<'_>) -> Result<Self, JsonError> {
        Ok(Self {
            expression: object.string("Expression", "")?,
            weapon_name: object.string("WeaponName", "")?,
            offhand_name: object.string("OffhandName", "")?,
        })
    }
}

/// One whole monster rule (`MonsterRuleDocument`), as older side-cars and
/// the legacy profile kept them.
#[derive(Debug, Clone, PartialEq)]
pub struct MonsterRuleDocument {
    /// The spec.
    pub expression: Option<String>,
    /// The action flags.
    pub flags: MonsterActionFlags,
    /// The priority.
    pub priority: i32,
    /// The element.
    pub damage_type: MonsterDamageType,
    /// The extra vulnerability (off when the document never wrote it).
    pub extra_vulnerability: MonsterDamageType,
    /// The main-hand weapon's id.
    pub weapon_object_id: u32,
    /// The off-hand item's id.
    pub offhand_object_id: u32,
    /// The main-hand weapon's name.
    pub weapon_name: Option<String>,
    /// The off-hand item's name.
    pub offhand_name: Option<String>,
    /// The pet's element.
    pub pet_damage_type: MonsterDamageType,
}

impl MonsterRuleDocument {
    /// The saved form of a rule.
    #[must_use]
    pub fn from_rule(rule: &MonsterRule) -> Self {
        let actions = rule.actions();
        Self {
            expression: Some(rule.expression().to_owned()),
            flags: actions.flags,
            priority: actions.priority,
            damage_type: actions.damage_type,
            extra_vulnerability: actions.extra_vulnerability,
            weapon_object_id: actions.weapon_object_id,
            offhand_object_id: actions.offhand_object_id,
            weapon_name: Some(actions.weapon_name.clone()),
            offhand_name: Some(actions.offhand_name.clone()),
            pet_damage_type: actions.pet_damage_type,
        }
    }

    /// The rule again, its priority held in range.
    ///
    /// # Errors
    /// The spec does not compile.
    pub fn to_rule(&self) -> Result<MonsterRule, String> {
        MonsterRule::new(
            self.expression.as_deref().unwrap_or_default(),
            MonsterRuleActions {
                flags: self.flags,
                priority: self.priority.clamp(-1, 4),
                damage_type: self.damage_type,
                extra_vulnerability: self.extra_vulnerability,
                weapon_object_id: self.weapon_object_id,
                offhand_object_id: self.offhand_object_id,
                weapon_name: self.weapon_name.clone().unwrap_or_default(),
                offhand_name: self.offhand_name.clone().unwrap_or_default(),
                pet_damage_type: self.pet_damage_type,
                ..MonsterRuleActions::default()
            },
        )
    }

    /// The JSON object.
    #[must_use]
    pub fn to_json(&self) -> JsonValue {
        JsonValue::object([
            (
                "Expression",
                stj::string_or_null(self.expression.as_deref()),
            ),
            ("Flags", stj::int(self.flags.0)),
            ("Priority", stj::int(self.priority)),
            ("DamageType", stj::int(self.damage_type as i32)),
            (
                "ExtraVulnerability",
                stj::int(self.extra_vulnerability as i32),
            ),
            ("WeaponObjectId", stj::uint(self.weapon_object_id)),
            ("OffhandObjectId", stj::uint(self.offhand_object_id)),
            (
                "WeaponName",
                stj::string_or_null(self.weapon_name.as_deref()),
            ),
            (
                "OffhandName",
                stj::string_or_null(self.offhand_name.as_deref()),
            ),
            ("PetDamageType", stj::int(self.pet_damage_type as i32)),
        ])
    }

    /// Reads one.
    ///
    /// # Errors
    /// A value of the wrong kind, or an element this build has no member for.
    pub fn read(object: &Object<'_>) -> Result<Self, JsonError> {
        Ok(Self {
            expression: object.string("Expression", "DEFAULT")?,
            flags: MonsterActionFlags(object.int("Flags", MonsterActionFlags::ATTACK.0)?),
            priority: object.int("Priority", 0)?,
            damage_type: damage_type(object, "DamageType", MonsterDamageType::Auto)?,
            extra_vulnerability: damage_type(
                object,
                "ExtraVulnerability",
                MonsterDamageType::None,
            )?,
            weapon_object_id: object.uint("WeaponObjectId", 0)?,
            offhand_object_id: object.uint("OffhandObjectId", 0)?,
            weapon_name: object.string("WeaponName", "")?,
            offhand_name: object.string("OffhandName", "")?,
            pet_damage_type: damage_type(object, "PetDamageType", MonsterDamageType::PlayerAuto)?,
        })
    }
}

/// Reads an array of documents; a `null` item stays `None`.
fn read_items<T>(
    object: &Object<'_>,
    name: &str,
    missing: Option<Vec<Option<T>>>,
    target: &str,
    read: impl Fn(&Object<'_>) -> Result<T, JsonError>,
) -> Result<Option<Vec<Option<T>>>, JsonError> {
    if object.get(name).is_none() {
        return Ok(missing);
    }
    match object.objects(name, None, target)? {
        None => Ok(None),
        Some(items) => items
            .iter()
            .map(|item| item.as_ref().map(&read).transpose())
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
    }
}

fn items_json<T>(items: Option<&Vec<Option<T>>>, to_json: impl Fn(&T) -> JsonValue) -> JsonValue {
    items.map_or(JsonValue::Null, |items| {
        JsonValue::Array(
            items
                .iter()
                .map(|item| item.as_ref().map_or(JsonValue::Null, &to_json))
                .collect(),
        )
    })
}

/// An authored item-enchant row on disk (`ItemEnchantRowDocument`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemEnchantRowDocument {
    /// The item's name.
    pub item_name: Option<String>,
    /// The spell's name.
    pub spell_name: Option<String>,
}

/// What stands in the document for the loot rules until they are written
/// into its place ([`SideCarDocument::write`]).
const RULES_STAND_IN: &str = "MossTankInventoryLootRulesGoHere";

/// The side-car (`SideCarDocument`), every property in the C#'s order.
#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs, clippy::struct_excessive_bools)]
pub struct SideCarDocument {
    pub show_nav_lines: bool,
    pub version: i32,
    pub item_names: Option<Vec<Option<String>>>,
    /// Object id in hexadecimal to the name it was last seen under.
    pub item_names_by_id: Option<Vec<(String, Option<String>)>>,
    pub consumable_names: Option<Vec<Option<String>>>,
    pub consumable_categories: Option<Vec<(String, ConsumableCategory)>>,
    pub no_buff_item_names: Option<Vec<Option<String>>>,
    pub log_channels: Option<Vec<Option<String>>>,
    pub combat_attack_power: f32,
    pub combat_scan_interval_seconds: f64,
    pub combat_meta_state: Option<String>,
    pub combat_dynamic_settings: Option<DynamicSettings>,
    pub combat_rules: Option<Vec<Option<MonsterRuleDocument>>>,
    pub combat_rule_item_names: Option<Vec<Option<MonsterRuleNameDocument>>>,
    pub buff_attributes: bool,
    pub buff_protections: bool,
    pub buff_auras: bool,
    pub buff_banes: bool,
    pub buff_regeneration: bool,
    pub buff_other: bool,
    pub buff_trained_skills_only: bool,
    pub buff_extra_spell_names: Option<Vec<Option<String>>>,
    pub buff_blacklisted_family_names: Option<Vec<Option<String>>>,
    pub buff_item_enchant_rows: Option<Vec<Option<ItemEnchantRowDocument>>>,
    pub vitals_enabled: bool,
    /// Help network peers outside the fellowship (a MossTank extension).
    pub vitals_help_network_peers: bool,
    pub inventory_scan_interval_seconds: f64,
    pub inventory_give_range_meters: f64,
    pub inventory_give_delay_seconds: f64,
    pub inventory_give_busy_retry_limit: i32,
    pub inventory_give_failure_limit: i32,
    pub inventory_loot_classifier_id: Option<String>,
    pub inventory_loot_scan_interval_seconds: f64,
    pub inventory_loot_walk_to_own_rare_corpses: bool,
    pub inventory_loot_rules: Option<Vec<Option<LootRuleDocument>>>,
    /// The Client pathing choice by name; absent in older files.
    pub navigation_client_pathing: Option<String>,
    pub navigation_jump_attempts: i32,
    /// The checkbox the pathing choice replaced (true reads as Always);
    /// left out of the file when null.
    pub navigation_walk_legs_with_client: Option<bool>,
}

fn client_pathing_name(value: ClientPathing) -> &'static str {
    match value {
        ClientPathing::Never => "Never",
        ClientPathing::WhenStuck => "WhenStuck",
        ClientPathing::Always => "Always",
    }
}

/// `Enum.TryParse<ClientPathing>(text, ignoreCase: true)`: a member's name
/// or number.
fn parse_client_pathing(text: Option<&str>) -> Option<ClientPathing> {
    let text = text?.trim();
    for value in [
        ClientPathing::Never,
        ClientPathing::WhenStuck,
        ClientPathing::Always,
    ] {
        if ordinal::equals_ignore_case(text, client_pathing_name(value)) {
            return Some(value);
        }
    }
    match number::parse_i32(text, NumberStyles::INTEGER)? {
        0 => Some(ClientPathing::Never),
        1 => Some(ClientPathing::WhenStuck),
        2 => Some(ClientPathing::Always),
        // Any other number parses in .NET to a value no switch knows; the
        // route then treats it as neither Never nor Always.
        _ => Some(ClientPathing::WhenStuck),
    }
}

impl SideCarDocument {
    /// What the live settings put in a side-car (`Capture`).
    #[must_use]
    pub fn capture(
        settings: &AllSettings,
        no_buff_item_names: &OrdinalStringSet,
        log_channels: &IgnoreCaseStringSet,
    ) -> Self {
        Self::capture_with(settings, no_buff_item_names, log_channels, true)
    }

    /// [`Self::capture`] and [`Self::write`] of it, the loot rules written
    /// straight from the settings rather than copied into the document
    /// first (a large profile has thousands).
    ///
    /// # Errors
    /// As [`Self::write`].
    pub fn capture_and_write(
        settings: &AllSettings,
        no_buff_item_names: &OrdinalStringSet,
        log_channels: &IgnoreCaseStringSet,
    ) -> Result<String, String> {
        let document = Self::capture_with(settings, no_buff_item_names, log_channels, false);
        match document
            .write_with(|out| sidecar_rules::write_live(out, &settings.inventory.loot.rules))?
        {
            Some(text) => Ok(text),
            None => Self::capture(settings, no_buff_item_names, log_channels).write(),
        }
    }

    /// [`Self::capture_and_write`] with the loot rules' text kept in `kept`
    /// with what it was written from: rules as they were at the last write
    /// (or as [`KeptRules::prepare`] wrote them ahead) are not written
    /// again; `checked`: [`KeptRules::take_check`]'s answer.
    ///
    /// # Errors
    /// As [`Self::write`].
    pub fn capture_and_write_kept(
        settings: &AllSettings,
        no_buff_item_names: &OrdinalStringSet,
        log_channels: &IgnoreCaseStringSet,
        kept: &mut KeptRules,
        checked: bool,
    ) -> Result<String, String> {
        let rules = kept.text(&settings.inventory.loot.rules, checked);
        let document = Self::capture_with(settings, no_buff_item_names, log_channels, false);
        match document.write_with(|out| out.push_str(rules))? {
            Some(text) => Ok(text),
            None => Self::capture(settings, no_buff_item_names, log_channels).write(),
        }
    }

    fn capture_with(
        settings: &AllSettings,
        no_buff_item_names: &OrdinalStringSet,
        log_channels: &IgnoreCaseStringSet,
        with_rules: bool,
    ) -> Self {
        let combat = &settings.combat;
        let buffs = &settings.buffs;
        let inventory = &settings.inventory;
        Self {
            show_nav_lines: settings.navigation.show_nav_lines,
            version: 1,
            // In the order the Items page shows them: the weapon walk
            // reads that order.
            item_names: some_strings(in_page_order(
                &combat.combat_item_order,
                &combat.combat_item_names,
            )),
            item_names_by_id: Some(
                combat
                    .combat_item_names_by_id
                    .iter()
                    .filter(|(id, _)| combat.combat_item_object_ids.contains(id))
                    .map(|(id, name)| (format!("{id:08X}"), Some(name.clone())))
                    .collect(),
            ),
            consumable_names: some_strings(sorted(combat.consumable_names.iter())),
            consumable_categories: Some(
                combat
                    .consumable_categories
                    .iter()
                    .map(|(name, category)| (name.clone(), *category))
                    .collect(),
            ),
            no_buff_item_names: some_strings(sorted(no_buff_item_names.iter())),
            log_channels: some_strings(sorted(log_channels.iter())),
            combat_attack_power: combat.attack_power,
            combat_scan_interval_seconds: combat.scan_interval_seconds,
            combat_meta_state: Some(combat.meta_state.clone()),
            combat_dynamic_settings: Some(capture_dynamic_settings(combat)),
            combat_rules: None,
            combat_rule_item_names: Some(
                combat
                    .rules
                    .iter()
                    .filter(|rule| {
                        !rule.actions().weapon_name.is_empty()
                            || !rule.actions().offhand_name.is_empty()
                    })
                    .map(|rule| Some(MonsterRuleNameDocument::from_rule(rule)))
                    .collect(),
            ),
            buff_attributes: buffs.buff_attributes,
            buff_protections: buffs.buff_protections,
            buff_auras: buffs.buff_auras,
            buff_banes: buffs.buff_banes,
            buff_regeneration: buffs.buff_regeneration,
            buff_other: buffs.buff_other,
            buff_trained_skills_only: buffs.buff_trained_skills_only,
            buff_extra_spell_names: some_strings(sorted(buffs.extra_buff_spell_names.iter())),
            buff_blacklisted_family_names: some_strings(sorted(
                buffs.blacklisted_buff_family_names.iter(),
            )),
            buff_item_enchant_rows: Some(
                buffs
                    .item_enchant_rows
                    .iter()
                    .filter(|row| !row.is_profiled_item_row())
                    .map(|row| {
                        Some(ItemEnchantRowDocument {
                            item_name: Some(row.item_name.clone()),
                            spell_name: Some(row.spell_name.clone()),
                        })
                    })
                    .collect(),
            ),
            vitals_enabled: settings.vitals.enabled,
            vitals_help_network_peers: settings.vitals.help_network_peers,
            inventory_scan_interval_seconds: inventory.scan_interval_seconds,
            inventory_give_range_meters: inventory.give_range_meters,
            inventory_give_delay_seconds: inventory.give_delay_seconds,
            inventory_give_busy_retry_limit: inventory.give_busy_retry_limit,
            inventory_give_failure_limit: inventory.give_failure_limit,
            inventory_loot_classifier_id: Some(inventory.loot.external_classifier_id.clone()),
            inventory_loot_scan_interval_seconds: inventory.loot.scan_interval_seconds,
            inventory_loot_walk_to_own_rare_corpses: inventory.loot.walk_to_own_rare_corpses,
            inventory_loot_rules: with_rules.then(|| {
                inventory
                    .loot
                    .rules
                    .iter()
                    .map(|rule| Some(LootRuleDocument::from_rule(rule)))
                    .collect()
            }),
            navigation_client_pathing: Some(
                client_pathing_name(settings.navigation.client_pathing).to_owned(),
            ),
            navigation_jump_attempts: settings.navigation.jump_attempts,
            navigation_walk_legs_with_client: None,
        }
    }

    /// The side-car of fresh settings (`CreateDefaults`).
    #[must_use]
    pub fn create_defaults() -> Self {
        Self::capture(
            &AllSettings::default(),
            &OrdinalStringSet::new(),
            &IgnoreCaseStringSet::new(),
        )
    }

    /// Puts the side-car into the live settings (`Apply`); a monster rule
    /// that does not compile is logged through `warn` and left out.
    pub fn apply(
        &self,
        settings: &mut AllSettings,
        no_buff_item_names: &mut OrdinalStringSet,
        log_channels: &mut IgnoreCaseStringSet,
        warn: &mut dyn FnMut(&str),
    ) {
        let combat = &mut settings.combat;
        replace_names(&mut combat.combat_item_names, self.item_names.as_deref());
        combat.combat_item_names_by_id.clear();
        for (id, name) in self.item_names_by_id.iter().flatten() {
            if let (Some(object_id), Some(name)) = (
                number::parse_u32(id, NumberStyles::HEX_NUMBER),
                name.as_deref()
                    .filter(|name| !casing::is_white_space_only(name)),
            ) {
                combat
                    .combat_item_names_by_id
                    .insert(object_id, name.to_owned());
            }
        }
        settings.navigation.show_nav_lines = self.show_nav_lines;
        replace_order_names(&mut combat.combat_item_order, self.item_names.as_deref());
        replace_names(
            &mut combat.consumable_names,
            self.consumable_names.as_deref(),
        );
        combat.consumable_categories.clear();
        for (item_name, category) in self.consumable_categories.iter().flatten() {
            if combat.consumable_names.contains(item_name.as_str()) {
                combat
                    .consumable_categories
                    .insert(item_name.clone(), *category);
            }
        }
        replace_names(no_buff_item_names, self.no_buff_item_names.as_deref());
        replace_ignore_case(log_channels, self.log_channels.as_deref());
        combat.attack_power = self.combat_attack_power.clamp(0.0, 1.0);
        combat.scan_interval_seconds = self.combat_scan_interval_seconds.clamp(0.05, 5.0);
        combat.meta_state = match self.combat_meta_state.as_deref() {
            Some(state) if !casing::is_white_space_only(state) => state.to_owned(),
            _ => "Default".to_owned(),
        };
        apply_dynamic_settings(combat, self.combat_dynamic_settings.as_deref());
        if let Some(migrated) = self.combat_rules.as_ref().filter(|rules| !rules.is_empty())
            && combat.rules.len() == 1
            && combat.rules[0].is_default()
        {
            combat.rules.clear();
            for rule in migrated.iter().flatten() {
                match rule.to_rule() {
                    Ok(rule) => combat.rules.push(rule),
                    Err(error) => {
                        warn(&format!(
                            "MossTank could not restore a monster rule: {error}"
                        ));
                    }
                }
            }
        }
        if !combat.rules.iter().any(MonsterRule::is_default) {
            combat.rules.push(MonsterRule::fresh_default());
        }
        self.apply_rule_item_names(combat);

        let buffs = &mut settings.buffs;
        buffs.buff_attributes = self.buff_attributes;
        buffs.buff_protections = self.buff_protections;
        buffs.buff_auras = self.buff_auras;
        buffs.buff_banes = self.buff_banes;
        buffs.buff_regeneration = self.buff_regeneration;
        buffs.buff_other = self.buff_other;
        buffs.buff_trained_skills_only = self.buff_trained_skills_only;
        replace_names(
            &mut buffs.extra_buff_spell_names,
            self.buff_extra_spell_names.as_deref(),
        );
        replace_names(
            &mut buffs.blacklisted_buff_family_names,
            self.buff_blacklisted_family_names.as_deref(),
        );
        buffs
            .item_enchant_rows
            .retain(BuffItemEnchantRow::is_profiled_item_row);
        for row in self.buff_item_enchant_rows.iter().flatten().flatten() {
            let Some(item_name) = row.item_name.as_deref().filter(|name| !name.is_empty()) else {
                continue;
            };
            buffs.item_enchant_rows.push(BuffItemEnchantRow {
                item_name: item_name.to_owned(),
                spell_name: row.spell_name.clone().unwrap_or_default(),
                object_id: None,
                spell_id: None,
            });
        }

        settings.vitals.enabled = self.vitals_enabled;
        settings.vitals.help_network_peers = self.vitals_help_network_peers;

        let inventory = &mut settings.inventory;
        inventory.scan_interval_seconds = self.inventory_scan_interval_seconds.clamp(0.05, 10.0);
        inventory.give_range_meters = self.inventory_give_range_meters.clamp(0.0, 1000.0);
        inventory.give_delay_seconds = self.inventory_give_delay_seconds.clamp(0.0, 60.0);
        inventory.give_busy_retry_limit = self.inventory_give_busy_retry_limit.clamp(1, 100);
        inventory.give_failure_limit = self.inventory_give_failure_limit.clamp(0, 100);
        inventory.loot.external_classifier_id = self
            .inventory_loot_classifier_id
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .to_owned();
        inventory.loot.scan_interval_seconds =
            self.inventory_loot_scan_interval_seconds.clamp(0.05, 5.0);
        inventory.loot.walk_to_own_rare_corpses = self.inventory_loot_walk_to_own_rare_corpses;
        inventory.loot.rules.clear();
        for rule in self.inventory_loot_rules.iter().flatten().flatten() {
            inventory.loot.rules.push(rule.to_rule());
        }
        settings.navigation.client_pathing =
            match parse_client_pathing(self.navigation_client_pathing.as_deref()) {
                Some(pathing) => pathing,
                None if self.navigation_walk_legs_with_client == Some(true) => {
                    ClientPathing::Always
                }
                None => ClientPathing::WhenStuck,
            };
        settings.navigation.jump_attempts = self.navigation_jump_attempts.clamp(1, 20);
    }

    fn apply_rule_item_names(&self, combat: &mut CombatSettings) {
        let Some(names) = self
            .combat_rule_item_names
            .as_ref()
            .filter(|names| !names.is_empty())
        else {
            return;
        };
        for index in 0..combat.rules.len() {
            let rule = &combat.rules[index];
            for stored in names.iter().flatten() {
                // The fallback row is DEFAULT in MossTank's editor and
                // <DEFAULT> in the .usd, so it is matched by identity.
                let expression = stored.expression.as_deref().unwrap_or_default();
                let matches = if is_default_name(expression) {
                    rule.is_default()
                } else {
                    stored.expression.as_deref() == Some(rule.expression())
                };
                if !matches {
                    continue;
                }
                let actions = MonsterRuleActions {
                    weapon_name: stored.weapon_name.clone().unwrap_or_default(),
                    offhand_name: stored.offhand_name.clone().unwrap_or_default(),
                    ..rule.actions().clone()
                };
                combat.rules[index] = rule.with_actions(actions);
                break;
            }
        }
    }

    /// The document as the C# writes it.
    #[must_use]
    pub fn to_json(&self) -> JsonValue {
        self.to_json_with(items_json(
            self.inventory_loot_rules.as_ref(),
            LootRuleDocument::to_json,
        ))
    }

    /// [`Self::to_json`] with `loot_rules` as `InventoryLootRules`.
    fn to_json_with(&self, loot_rules: JsonValue) -> JsonValue {
        let mut members: Vec<(String, JsonValue)> =
            vec![
                ("ShowNavLines".into(), JsonValue::Bool(self.show_nav_lines)),
                ("Version".into(), stj::int(self.version)),
                ("ItemNames".into(), strings_json(self.item_names.as_ref())),
                (
                    "ItemNamesById".into(),
                    self.item_names_by_id
                        .as_ref()
                        .map_or(JsonValue::Null, |entries| {
                            JsonValue::object(entries.iter().map(|(id, name)| {
                                (id.clone(), stj::string_or_null(name.as_deref()))
                            }))
                        }),
                ),
                (
                    "ConsumableNames".into(),
                    strings_json(self.consumable_names.as_ref()),
                ),
                (
                    "ConsumableCategories".into(),
                    self.consumable_categories
                        .as_ref()
                        .map_or(JsonValue::Null, |entries| {
                            JsonValue::object(entries.iter().map(|(name, category)| {
                                (name.clone(), stj::int(consumable_number(*category)))
                            }))
                        }),
                ),
                (
                    "NoBuffItemNames".into(),
                    strings_json(self.no_buff_item_names.as_ref()),
                ),
                (
                    "LogChannels".into(),
                    strings_json(self.log_channels.as_ref()),
                ),
                (
                    "CombatAttackPower".into(),
                    stj::float(self.combat_attack_power),
                ),
                (
                    "CombatScanIntervalSeconds".into(),
                    stj::double(self.combat_scan_interval_seconds),
                ),
                (
                    "CombatMetaState".into(),
                    stj::string_or_null(self.combat_meta_state.as_deref()),
                ),
                (
                    "CombatDynamicSettings".into(),
                    self.combat_dynamic_settings
                        .as_ref()
                        .map_or(JsonValue::Null, |entries| dynamic_settings_json(entries)),
                ),
                (
                    "CombatRules".into(),
                    items_json(self.combat_rules.as_ref(), MonsterRuleDocument::to_json),
                ),
                (
                    "CombatRuleItemNames".into(),
                    items_json(
                        self.combat_rule_item_names.as_ref(),
                        MonsterRuleNameDocument::to_json,
                    ),
                ),
                (
                    "BuffAttributes".into(),
                    JsonValue::Bool(self.buff_attributes),
                ),
                (
                    "BuffProtections".into(),
                    JsonValue::Bool(self.buff_protections),
                ),
                ("BuffAuras".into(), JsonValue::Bool(self.buff_auras)),
                ("BuffBanes".into(), JsonValue::Bool(self.buff_banes)),
                (
                    "BuffRegeneration".into(),
                    JsonValue::Bool(self.buff_regeneration),
                ),
                ("BuffOther".into(), JsonValue::Bool(self.buff_other)),
                (
                    "BuffTrainedSkillsOnly".into(),
                    JsonValue::Bool(self.buff_trained_skills_only),
                ),
                (
                    "BuffExtraSpellNames".into(),
                    strings_json(self.buff_extra_spell_names.as_ref()),
                ),
                (
                    "BuffBlacklistedFamilyNames".into(),
                    strings_json(self.buff_blacklisted_family_names.as_ref()),
                ),
                (
                    "BuffItemEnchantRows".into(),
                    items_json(self.buff_item_enchant_rows.as_ref(), |row| {
                        JsonValue::object([
                            ("ItemName", stj::string_or_null(row.item_name.as_deref())),
                            ("SpellName", stj::string_or_null(row.spell_name.as_deref())),
                        ])
                    }),
                ),
                ("VitalsEnabled".into(), JsonValue::Bool(self.vitals_enabled)),
                (
                    "VitalsHelpNetworkPeers".into(),
                    JsonValue::Bool(self.vitals_help_network_peers),
                ),
                (
                    "InventoryScanIntervalSeconds".into(),
                    stj::double(self.inventory_scan_interval_seconds),
                ),
                (
                    "InventoryGiveRangeMeters".into(),
                    stj::double(self.inventory_give_range_meters),
                ),
                (
                    "InventoryGiveDelaySeconds".into(),
                    stj::double(self.inventory_give_delay_seconds),
                ),
                (
                    "InventoryGiveBusyRetryLimit".into(),
                    stj::int(self.inventory_give_busy_retry_limit),
                ),
                (
                    "InventoryGiveFailureLimit".into(),
                    stj::int(self.inventory_give_failure_limit),
                ),
                (
                    "InventoryLootClassifierId".into(),
                    stj::string_or_null(self.inventory_loot_classifier_id.as_deref()),
                ),
                (
                    "InventoryLootScanIntervalSeconds".into(),
                    stj::double(self.inventory_loot_scan_interval_seconds),
                ),
                (
                    "InventoryLootWalkToOwnRareCorpses".into(),
                    JsonValue::Bool(self.inventory_loot_walk_to_own_rare_corpses),
                ),
                ("InventoryLootRules".into(), loot_rules),
                (
                    "NavigationClientPathing".into(),
                    stj::string_or_null(self.navigation_client_pathing.as_deref()),
                ),
                (
                    "NavigationJumpAttempts".into(),
                    stj::int(self.navigation_jump_attempts),
                ),
            ];
        if let Some(walk) = self.navigation_walk_legs_with_client {
            members.push(("NavigationWalkLegsWithClient".into(), JsonValue::Bool(walk)));
        }
        JsonValue::Object(members)
    }

    /// The side-car's text, as the C# writes it.
    ///
    /// # Errors
    /// A number JSON cannot hold (the attack bar set to NaN): .NET's
    /// message.
    pub fn write(&self) -> Result<String, String> {
        let rules = self.inventory_loot_rules.as_ref();
        match self.write_with(|out| sidecar_rules::write(out, rules))? {
            Some(text) => Ok(text),
            None => stj::try_write(&self.to_json()),
        }
    }

    /// The document with its rules written by `write_rules`: the rest
    /// through the value tree, a stand-in where the rules go, the rules
    /// written straight into that place (`sidecar_rules`: thousands of them
    /// with a large profile). `None` should the stand-in not be found.
    fn write_with(&self, write_rules: impl FnOnce(&mut String)) -> Result<Option<String>, String> {
        let text =
            stj::try_write(&self.to_json_with(JsonValue::String(RULES_STAND_IN.to_owned())))?;
        let stand_in = format!("{}\"{RULES_STAND_IN}\"", sidecar_rules::MEMBER);
        let Some(at) = text.rfind(&stand_in) else {
            return Ok(None);
        };
        let value = at + sidecar_rules::MEMBER.len();
        let mut out = String::with_capacity(text.len() + 64 * 1024);
        out.push_str(&text[..value]);
        write_rules(&mut out);
        out.push_str(&text[value + RULES_STAND_IN.len() + 2..]);
        Ok(Some(out))
    }

    /// Reads a side-car; `None` for a document that is JSON `null`.
    ///
    /// # Errors
    /// Not JSON, or a value of the wrong kind (`JsonException`).
    pub fn read(text: &str) -> Result<Option<Self>, JsonError> {
        if let Some(document) = Self::read_quickly(text) {
            return Ok(Some(document));
        }
        Self::read_through_values(text)
    }

    /// [`Self::read`] through the JSON value tree, whatever the layout.
    fn read_through_values(text: &str) -> Result<Option<Self>, JsonError> {
        let document = json::parse(text).map_err(|error| error.to_string())?;
        let Some(object) = Object::of(&document, "$", "SideCarDocument")? else {
            return Ok(None);
        };
        Self::read_object(&object).map(Some)
    }

    /// [`Self::read`] of a document whose rules are as [`Self::write`]
    /// writes them: the rules read by hand (`sidecar_rules`), the rest
    /// through the value tree with `null` in their place. `None` for any
    /// other document, which is read the usual way.
    fn read_quickly(text: &str) -> Option<Self> {
        let at = text.find(sidecar_rules::MEMBER)? + sidecar_rules::MEMBER.len();
        let (rules, end) = sidecar_rules::read(text, at)?;
        let mut rest = String::with_capacity(text.len() - (end - at) + 4);
        rest.push_str(&text[..at]);
        rest.push_str("null");
        rest.push_str(&text[end..]);
        let document = json::parse(&rest).ok()?;
        let JsonValue::Object(members) = &document else {
            return None;
        };
        // The rules' member, once (a name matched ignoring case, as the
        // reader matches names), and the one just read.
        let mut named = members
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("InventoryLootRules"));
        if !matches!(
            (named.next(), named.next()),
            (Some((_, JsonValue::Null)), None)
        ) {
            return None;
        }
        let object = Object::of(&document, "$", "SideCarDocument").ok()??;
        let mut read = Self::read_object(&object).ok()?;
        read.inventory_loot_rules = rules;
        Some(read)
    }

    #[allow(clippy::too_many_lines)]
    fn read_object(o: &Object<'_>) -> Result<Self, JsonError> {
        let item_names_by_id = match o.map("ItemNamesById", "Dictionary`2[String,String]")? {
            None => None,
            Some(entries) => Some(
                entries
                    .into_iter()
                    .map(|(id, name)| {
                        Ok((id.to_owned(), stj::string_value(name, "ItemNamesById")?))
                    })
                    .collect::<Result<Vec<_>, JsonError>>()?,
            ),
        };
        let consumable_categories = match o.map(
            "ConsumableCategories",
            "Dictionary`2[String,ConsumableCategory]",
        )? {
            None => None,
            Some(entries) => Some(
                entries
                    .into_iter()
                    .map(|(name, value)| {
                        let number = stj::int_value(value, "ConsumableCategories")?;
                        let category = consumable_from(number)
                            .ok_or_else(|| unknown("ConsumableCategories", number))?;
                        Ok((name.to_owned(), category))
                    })
                    .collect::<Result<Vec<_>, JsonError>>()?,
            ),
        };
        Ok(Self {
            show_nav_lines: o.bool("ShowNavLines", false)?,
            version: o.int("Version", 1)?,
            item_names: o.strings("ItemNames")?,
            item_names_by_id,
            consumable_names: o.strings("ConsumableNames")?,
            consumable_categories,
            no_buff_item_names: o.strings("NoBuffItemNames")?,
            log_channels: o.strings("LogChannels")?,
            combat_attack_power: o.float("CombatAttackPower", 0.5)?,
            combat_scan_interval_seconds: o.double("CombatScanIntervalSeconds", 0.25)?,
            combat_meta_state: o.string("CombatMetaState", "Default")?,
            combat_dynamic_settings: read_dynamic_settings(o, "CombatDynamicSettings")?,
            combat_rules: read_items(
                o,
                "CombatRules",
                None,
                "MonsterRuleDocument",
                MonsterRuleDocument::read,
            )?,
            combat_rule_item_names: read_items(
                o,
                "CombatRuleItemNames",
                Some(Vec::new()),
                "MonsterRuleNameDocument",
                MonsterRuleNameDocument::read,
            )?,
            buff_attributes: o.bool("BuffAttributes", true)?,
            buff_protections: o.bool("BuffProtections", true)?,
            buff_auras: o.bool("BuffAuras", true)?,
            buff_banes: o.bool("BuffBanes", true)?,
            buff_regeneration: o.bool("BuffRegeneration", true)?,
            buff_other: o.bool("BuffOther", false)?,
            buff_trained_skills_only: o.bool("BuffTrainedSkillsOnly", true)?,
            buff_extra_spell_names: o.strings("BuffExtraSpellNames")?,
            buff_blacklisted_family_names: o.strings("BuffBlacklistedFamilyNames")?,
            buff_item_enchant_rows: read_items(
                o,
                "BuffItemEnchantRows",
                Some(Vec::new()),
                "ItemEnchantRowDocument",
                |row| {
                    Ok(ItemEnchantRowDocument {
                        item_name: row.string("ItemName", "")?,
                        spell_name: row.string("SpellName", "")?,
                    })
                },
            )?,
            vitals_enabled: o.bool("VitalsEnabled", true)?,
            vitals_help_network_peers: o.bool("VitalsHelpNetworkPeers", false)?,
            inventory_scan_interval_seconds: o.double("InventoryScanIntervalSeconds", 0.25)?,
            inventory_give_range_meters: o.double("InventoryGiveRangeMeters", 15.0)?,
            inventory_give_delay_seconds: o.double("InventoryGiveDelaySeconds", 0.0)?,
            inventory_give_busy_retry_limit: o.int("InventoryGiveBusyRetryLimit", 10)?,
            inventory_give_failure_limit: o.int("InventoryGiveFailureLimit", 3)?,
            inventory_loot_classifier_id: o.string("InventoryLootClassifierId", "")?,
            inventory_loot_scan_interval_seconds: o
                .double("InventoryLootScanIntervalSeconds", 0.25)?,
            inventory_loot_walk_to_own_rare_corpses: o
                .bool("InventoryLootWalkToOwnRareCorpses", false)?,
            inventory_loot_rules: read_items(
                o,
                "InventoryLootRules",
                Some(Vec::new()),
                "LootRuleDocument",
                LootRuleDocument::read,
            )?,
            navigation_client_pathing: match o.get("NavigationClientPathing") {
                None => None,
                Some(value) => stj::string_value(value, "$.NavigationClientPathing")?,
            },
            navigation_jump_attempts: o.int("NavigationJumpAttempts", 3)?,
            navigation_walk_legs_with_client: o.optional_bool("NavigationWalkLegsWithClient")?,
        })
    }
}

/// A side-car read a run of its loot rules at a time (a large profile's
/// side-car is a 1.4 MB document): the rules as [`SideCarDocument::write`]
/// writes them are read by hand and made loot rules as they are read, the
/// rest of the document at the end; a side-car laid out any other way is
/// read the usual way at the end.
#[derive(Debug, Clone)]
pub struct SideCarRead {
    text: String,
    rules: Option<sidecar_rules::RulesRead>,
    end: Option<usize>,
}

/// What a [`SideCarRead`] read.
#[derive(Debug)]
pub enum SideCarOutcome {
    /// The document, and the loot rules to take in after applying it when
    /// they were read by hand (the document then has none).
    Read(Box<SideCarDocument>, Option<Vec<LootRule>>),
    /// The document is JSON `null`.
    Null,
    /// The document will not read: why, and its text.
    Failed(JsonError, String),
}

impl SideCarRead {
    /// A read of `text`.
    #[must_use]
    pub fn new(text: String) -> Self {
        let rules = text
            .find(sidecar_rules::MEMBER)
            .map(|at| sidecar_rules::RulesRead::new(at + sidecar_rules::MEMBER.len()));
        Self {
            text,
            rules,
            end: None,
        }
    }

    /// Reads on through at least `bytes` more of the rules; whether they
    /// are all read (or will be read at the end), and the bytes read.
    pub fn step(&mut self, bytes: usize) -> (bool, usize) {
        let Some(rules) = self.rules.as_mut() else {
            return (true, 0);
        };
        if self.end.is_some() {
            return (true, 0);
        }
        let before = rules.position();
        let step = rules.step(&self.text, bytes);
        let read = rules.position() - before;
        match step {
            sidecar_rules::RulesStep::More => (false, read),
            sidecar_rules::RulesStep::Done(end) => {
                self.end = Some(end);
                (true, read)
            }
            sidecar_rules::RulesStep::GiveUp => {
                self.rules = None;
                (true, read)
            }
        }
    }

    /// The document's length, in bytes.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.text.len()
    }

    /// Whether the rules are all read (or will be read at the end).
    #[must_use]
    pub fn is_read(&self) -> bool {
        self.rules.is_none() || self.end.is_some()
    }

    /// The read's outcome, the rules read to the end first.
    #[must_use]
    pub fn finish(mut self) -> SideCarOutcome {
        while !self.step(usize::MAX).0 {}
        if let (Some(mut rules), Some(end)) = (self.rules.take(), self.end) {
            let at = rules.position();
            let start = self
                .text
                .find(sidecar_rules::MEMBER)
                .map(|at| at + sidecar_rules::MEMBER.len());
            if let Some(start) = start
                && at == end
                && let Some(document) = Self::rest(&self.text, start, end)
            {
                return SideCarOutcome::Read(
                    Box::new(document),
                    Some(rules.take_rules().unwrap_or_default()),
                );
            }
        }
        match SideCarDocument::read_through_values(&self.text) {
            Ok(Some(document)) => SideCarOutcome::Read(Box::new(document), None),
            Ok(None) => SideCarOutcome::Null,
            Err(error) => SideCarOutcome::Failed(error, self.text),
        }
    }

    /// The document with `null` for the rules between `start` and `end`,
    /// read through the value tree; `None` unless it reads and names the
    /// rules once.
    fn rest(text: &str, start: usize, end: usize) -> Option<SideCarDocument> {
        let mut rest = String::with_capacity(text.len() - (end - start) + 4);
        rest.push_str(&text[..start]);
        rest.push_str("null");
        rest.push_str(&text[end..]);
        let document = json::parse(&rest).ok()?;
        let JsonValue::Object(members) = &document else {
            return None;
        };
        // The rules' member once (a name matched ignoring case, as the
        // reader matches names), and the one read by hand.
        let mut named = members
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("InventoryLootRules"));
        if !matches!(
            (named.next(), named.next()),
            (Some((_, JsonValue::Null)), None)
        ) {
            return None;
        }
        let object = Object::of(&document, "$", "SideCarDocument").ok()??;
        SideCarDocument::read_object(&object).ok()
    }
}

/// The loot rules' text in the side-car, kept with what it was written
/// from: a large profile's is 1.4 MB, some 36 M instructions to write, so
/// a save that finds it kept only copies it, and [`Self::prepare`] writes
/// it ahead a run at a time.
#[derive(Debug, Clone, Default)]
pub struct KeptRules {
    kept: Option<(u64, String)>,
    writing: Option<sidecar_rules::LiveWrite>,
    /// [`Self::prepare`] found the kept text written from the rules it was
    /// given, and nothing has used that answer yet.
    checked: bool,
}

impl KeptRules {
    /// The text for `rules`: the kept one when written from them, else
    /// written now and kept. `checked`: the caller has
    /// [`Self::take_check`]'s word that the kept text is the rules' (the
    /// rules unchanged since), so they are not looked at again.
    pub fn text(&mut self, rules: &[LootRule], checked: bool) -> &str {
        if !(checked && self.kept.is_some()) {
            let hash = crate::fast_hash::hash(rules);
            if self.kept.as_ref().is_none_or(|(kept, _)| *kept != hash) {
                let mut text = String::new();
                sidecar_rules::write_live(&mut text, rules);
                self.kept = Some((hash, text));
                self.writing = None;
            }
        }
        self.kept.as_ref().map_or("", |(_, text)| text.as_str())
    }

    /// Whether the last [`Self::prepare`] answered true and no one has
    /// taken that answer since; taking it clears it.
    pub fn take_check(&mut self) -> bool {
        std::mem::take(&mut self.checked)
    }

    /// Writes the text for `rules` ahead, about `bytes` of it: true once it
    /// is kept, and the next save copies it. Rules that change while it is
    /// written start it again.
    pub fn prepare(&mut self, rules: &[LootRule], bytes: usize) -> bool {
        self.checked = false;
        // The whole set is hashed when a preparation starts (the text kept
        // may be theirs already) and when it ends; the runs between hash
        // only what they write.
        if self.writing.is_none() {
            let hash = crate::fast_hash::hash(rules);
            if self.kept.as_ref().is_some_and(|(kept, _)| *kept == hash) {
                self.checked = true;
                return true;
            }
            self.writing = Some(sidecar_rules::LiveWrite::default());
        }
        let Some(write) = self.writing.as_mut() else {
            return false;
        };
        if !write.step(rules, bytes) {
            return false;
        }
        let Some(write) = self.writing.take() else {
            return false;
        };
        let (written, text) = write.finish();
        if written != crate::fast_hash::hash(rules) {
            // They changed while it was written: the next run starts again.
            return false;
        }
        self.kept = Some((written, text));
        self.checked = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use mosstank_formats::utl::{LootRule, VtankLootRequirement};
    use mosstank_settings::model::AllSettings;

    use super::*;

    /// Loot rules with every shape the side-car writes: VTank requirement
    /// records, expressions, names that need escaping (quotes, HTML
    /// characters, line breaks, controls, accents, a character beyond the
    /// basic plane), a missing payload and no requirements at all.
    fn awkward_rules() -> Vec<LootRule> {
        let mut rules = Vec::new();
        let mut plain = LootRule::named("Peas", "ObjectClass = 5 && Value > 100");
        plain.priority = -3;
        rules.push(plain);
        let mut odd = LootRule::named("\"Odd\" <name> & 'more' + `tick`\\", "x\ny");
        odd.custom_expression = "tab\there\r\nbreak\u{1}\u{7f}é€😀".to_owned();
        odd.action = LootAction::KeepUpTo;
        odd.keep_count = 2_147_483_647;
        rules.push(odd);
        let mut imported = LootRule::named("Imported", "*");
        imported.has_imported_requirements = true;
        imported.vtank_requirements = vec![
            VtankLootRequirement {
                kind: 1,
                payload: "Aetheria\r\n1\r\n".to_owned(),
            },
            VtankLootRequirement {
                kind: -2,
                payload: String::new(),
            },
        ];
        rules.push(imported);
        let mut empty = LootRule::named("Empty", "*");
        empty.has_imported_requirements = true;
        rules.push(empty);
        rules
    }

    /// The owner's own 1,700-rule loot profile when it is on this machine,
    /// read in place (the fuel gate measures with it).
    fn owner_rules() -> Option<Vec<LootRule>> {
        let local = std::env::var_os("LOCALAPPDATA")?;
        let path = std::path::Path::new(&local).join("openac-rust/vtank/mosstank/loot/Loot.utl");
        let text = String::from_utf8_lossy(&std::fs::read(path).ok()?).into_owned();
        mosstank_formats::utl::try_read(Some(&text))
            .ok()
            .map(|profile| profile.rules)
    }

    fn documents() -> Vec<SideCarDocument> {
        let mut sets = vec![
            awkward_rules(),
            Vec::new(),
            mosstank_formats::utl::try_read(Some(&mosstank_formats::fixtures::read_vtank(
                "loot-c.utl",
            )))
            .unwrap()
            .rules,
        ];
        sets.extend(owner_rules());
        let mut documents: Vec<SideCarDocument> = sets
            .into_iter()
            .map(|rules| {
                let mut settings = AllSettings::default();
                settings.inventory.loot.rules = rules;
                SideCarDocument::capture(
                    &settings,
                    &OrdinalStringSet::new(),
                    &IgnoreCaseStringSet::new(),
                )
            })
            .collect();
        let mut nulls = documents[0].clone();
        if let Some(rules) = nulls.inventory_loot_rules.as_mut() {
            rules.insert(1, None);
            if let Some(Some(rule)) = rules.get_mut(0) {
                rule.name = None;
                rule.requirements = Some(vec![None, Some((5, None))]);
            }
            if let Some(Some(rule)) = rules.get_mut(2) {
                rule.requirements = None;
            }
        }
        documents.push(nulls);
        let mut none = documents[0].clone();
        none.inventory_loot_rules = None;
        documents.push(none);
        documents
    }

    // The side-car's text, rules written by hand, is the document writer's
    // to the byte.
    #[test]
    fn the_side_car_is_written_as_the_document_writer_writes_it() {
        for document in documents() {
            assert_eq!(document.write(), stj::try_write(&document.to_json()));
        }
    }

    // A side-car as the writer writes it reads by hand as it reads through
    // the value tree; any other layout of the same document reads through
    // the tree.
    #[test]
    fn the_side_car_reads_as_the_document_reader_reads_it() {
        for document in documents() {
            let text = document.write().unwrap();
            let quick = SideCarDocument::read_quickly(&text);
            assert!(quick.is_some(), "the writer's own text reads by hand");
            assert_eq!(quick, SideCarDocument::read_through_values(&text).unwrap());
            assert_eq!(
                SideCarDocument::read(&text).unwrap(),
                Some(document.clone())
            );
            let compact = json::system_text_json::to_string(
                &document.to_json(),
                &json::system_text_json::Options::COMPACT,
            )
            .unwrap();
            assert_eq!(SideCarDocument::read_quickly(&compact), None);
            assert_eq!(
                SideCarDocument::read(&compact).unwrap(),
                Some(document.clone())
            );
            // An escape the writer would not write, a number it would not
            // write, a member twice: the value tree decides.
            let escaped = format!("\"Name\": \"{}u0050eas\"", char::from(92u8));
            for (from, to) in [
                ("\"Name\": \"Peas\"", escaped.as_str()),
                ("\"Priority\": -3", "\"Priority\": -3.0"),
                (
                    "\r\n  \"Version\":",
                    "\r\n  \"inventorylootrules\": null,\r\n  \"Version\":",
                ),
            ] {
                if !text.contains(from) {
                    continue;
                }
                let changed = text.replacen(from, to, 1);
                assert_eq!(SideCarDocument::read_quickly(&changed), None, "{to}");
                assert_eq!(
                    SideCarDocument::read(&changed),
                    SideCarDocument::read_through_values(&changed),
                    "{to}"
                );
            }
        }
    }

    // Captured and written in one go, the rules straight from the
    // settings, a side-car is what capturing it and writing it gives.
    #[test]
    fn capturing_and_writing_in_one_go_is_capturing_then_writing() {
        let mut sets = vec![awkward_rules(), Vec::new()];
        sets.extend(owner_rules());
        for rules in sets {
            let mut settings = AllSettings::default();
            settings.inventory.loot.rules = rules;
            let names = OrdinalStringSet::new();
            let channels = IgnoreCaseStringSet::new();
            assert_eq!(
                SideCarDocument::capture_and_write(&settings, &names, &channels),
                stj::try_write(&SideCarDocument::capture(&settings, &names, &channels).to_json())
            );
        }
    }

    // Read a run of rules at a time, a side-car gives the document and the
    // loot rules its `Apply` takes in from reading it whole; laid out any
    // other way it is read whole.
    #[test]
    fn a_side_car_read_a_run_at_a_time_is_the_whole_read() {
        for document in documents() {
            let text = document.write().unwrap();
            let whole = SideCarDocument::read_through_values(&text)
                .unwrap()
                .unwrap();
            let rules_of = |document: &SideCarDocument| -> Vec<LootRule> {
                document
                    .inventory_loot_rules
                    .iter()
                    .flatten()
                    .flatten()
                    .map(LootRuleDocument::to_rule)
                    .collect()
            };
            for bytes in [0, 300, usize::MAX] {
                let mut read = SideCarRead::new(text.clone());
                while !read.step(bytes).0 {}
                let SideCarOutcome::Read(read, Some(rules)) = read.finish() else {
                    panic!("the writer's own text reads by hand");
                };
                assert_eq!(rules, rules_of(&whole));
                assert_eq!(
                    rules.iter().map(LootRule::clone).collect::<Vec<_>>(),
                    whole
                        .inventory_loot_rules
                        .clone()
                        .into_iter()
                        .flatten()
                        .flatten()
                        .map(LootRuleDocument::into_rule)
                        .collect::<Vec<_>>()
                );
                let mut expected = whole.clone();
                expected.inventory_loot_rules = None;
                assert_eq!(*read, expected);
            }
            let compact = json::system_text_json::to_string(
                &document.to_json(),
                &json::system_text_json::Options::COMPACT,
            )
            .unwrap();
            let SideCarOutcome::Read(read, None) = SideCarRead::new(compact).finish() else {
                panic!("another layout reads whole");
            };
            assert_eq!(*read, whole);
        }
        assert!(matches!(
            SideCarRead::new("{".to_owned()).finish(),
            SideCarOutcome::Failed(_, _)
        ));
        assert!(matches!(
            SideCarRead::new("null".to_owned()).finish(),
            SideCarOutcome::Null
        ));
    }

    // The rules' text kept from the last write gives the same side-car, and
    // rules that changed since are written again.
    #[test]
    fn a_side_car_written_with_kept_rules_is_the_same_side_car() {
        let names = OrdinalStringSet::new();
        let channels = IgnoreCaseStringSet::new();
        let mut kept = KeptRules::default();
        for rules in [
            awkward_rules(),
            awkward_rules(),
            Vec::new(),
            awkward_rules(),
        ] {
            let mut settings = AllSettings::default();
            settings.inventory.loot.rules = rules;
            assert_eq!(
                SideCarDocument::capture_and_write_kept(
                    &settings, &names, &channels, &mut kept, false
                ),
                SideCarDocument::capture_and_write(&settings, &names, &channels)
            );
        }
    }

    #[test]
    fn rules_written_ahead_a_run_at_a_time_make_the_same_side_car() {
        let names = OrdinalStringSet::new();
        let channels = IgnoreCaseStringSet::new();
        let mut sets = vec![awkward_rules(), Vec::new()];
        sets.extend(owner_rules());
        for rules in sets {
            let mut settings = AllSettings::default();
            settings.inventory.loot.rules = rules;
            for bytes in [1, 700, 64 * 1024] {
                let mut kept = KeptRules::default();
                let mut runs = 1;
                while !kept.prepare(&settings.inventory.loot.rules, bytes) {
                    runs += 1;
                }
                assert!(kept.prepare(&settings.inventory.loot.rules, bytes));
                if bytes == 1 {
                    assert_eq!(runs, settings.inventory.loot.rules.len().max(1));
                }
                let checked = kept.take_check();
                assert!(checked);
                assert_eq!(
                    SideCarDocument::capture_and_write_kept(
                        &settings, &names, &channels, &mut kept, checked
                    ),
                    SideCarDocument::capture_and_write(&settings, &names, &channels)
                );
                assert!(!kept.take_check());
            }
        }
    }

    #[test]
    fn rules_that_change_while_written_ahead_are_written_again() {
        let names = OrdinalStringSet::new();
        let channels = IgnoreCaseStringSet::new();
        let mut settings = AllSettings::default();
        settings.inventory.loot.rules = awkward_rules();
        let mut kept = KeptRules::default();
        assert!(!kept.prepare(&settings.inventory.loot.rules, 1));
        assert!(!kept.take_check());
        settings.inventory.loot.rules.pop();
        while !kept.prepare(&settings.inventory.loot.rules, 1) {}
        // A save that comes later, the rules changed again since, looks.
        settings.inventory.loot.rules.pop();
        assert_eq!(
            SideCarDocument::capture_and_write_kept(&settings, &names, &channels, &mut kept, false),
            SideCarDocument::capture_and_write(&settings, &names, &channels)
        );
    }
}

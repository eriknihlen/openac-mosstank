//! The Monsters page: VTank's twenty-three-column rule grid, the add box
//! and the add-selected button.

use ac_plugin_abi::ui::UiValue;
use ac_plugin_abi::world::ObjectClass;
use mosstank_combat::controller::is_profiled;
use mosstank_data::damage::MonsterDamageType;
use mosstank_settings::monster::{MonsterActionFlags, MonsterRule, MonsterRuleActions};

use super::meta_tab::{MOVE_DOWN_ICON, MOVE_UP_ICON};
use super::{Arg, i, t, tl};
use crate::tank::TankCx;

/// The damage cell's cycle (`MonsterDamageCycle`).
const MONSTER_DAMAGE_CYCLE: [MonsterDamageType; 14] = [
    MonsterDamageType::Pierce,
    MonsterDamageType::Bludgeon,
    MonsterDamageType::Slash,
    MonsterDamageType::Acid,
    MonsterDamageType::Electric,
    MonsterDamageType::Cold,
    MonsterDamageType::Fire,
    MonsterDamageType::Harm,
    MonsterDamageType::Auto,
    MonsterDamageType::VoidBasic,
    MonsterDamageType::DrainAuto,
    MonsterDamageType::Prismatic,
    MonsterDamageType::Random,
    MonsterDamageType::Fists,
];

/// The extra vulnerability's cycle.
const MONSTER_EXTRA_VULNERABILITY_CYCLE: [MonsterDamageType; 9] = [
    MonsterDamageType::Pierce,
    MonsterDamageType::Bludgeon,
    MonsterDamageType::Slash,
    MonsterDamageType::Acid,
    MonsterDamageType::Electric,
    MonsterDamageType::Cold,
    MonsterDamageType::Fire,
    MonsterDamageType::Auto,
    MonsterDamageType::None,
];

/// The pet element's cycle.
const MONSTER_PET_DAMAGE_CYCLE: [MonsterDamageType; 10] = [
    MonsterDamageType::Pierce,
    MonsterDamageType::Bludgeon,
    MonsterDamageType::Slash,
    MonsterDamageType::Acid,
    MonsterDamageType::Electric,
    MonsterDamageType::Cold,
    MonsterDamageType::Fire,
    MonsterDamageType::PlayerAuto,
    MonsterDamageType::Auto,
    MonsterDamageType::None,
];

/// The fourteen check columns, in the grid's order, by binding name.
const FLAG_COLUMNS: [(&str, &str, MonsterActionFlags); 14] = [
    (
        "MonsterFesterColumn",
        "ToggleMonsterFesterAt",
        MonsterActionFlags::FESTER,
    ),
    (
        "MonsterBroadsideColumn",
        "ToggleMonsterBroadsideAt",
        MonsterActionFlags::BROADSIDE,
    ),
    (
        "MonsterGravityWellColumn",
        "ToggleMonsterGravityWellAt",
        MonsterActionFlags::GRAVITY_WELL,
    ),
    (
        "MonsterImperilColumn",
        "ToggleMonsterImperilAt",
        MonsterActionFlags::IMPERIL,
    ),
    (
        "MonsterYieldColumn",
        "ToggleMonsterYieldAt",
        MonsterActionFlags::YIELD,
    ),
    (
        "MonsterVulnerabilityColumn",
        "ToggleMonsterVulnerabilityAt",
        MonsterActionFlags::VULNERABILITY,
    ),
    (
        "MonsterAttackColumn",
        "ToggleMonsterAttackAt",
        MonsterActionFlags::ATTACK,
    ),
    (
        "MonsterRingColumn",
        "ToggleMonsterRingAt",
        MonsterActionFlags::RING,
    ),
    (
        "MonsterStreakColumn",
        "ToggleMonsterStreakAt",
        MonsterActionFlags::STREAK,
    ),
    (
        "MonsterWeakeningColumn",
        "ToggleMonsterWeakeningAt",
        MonsterActionFlags::WEAKENING_CURSE,
    ),
    (
        "MonsterFesteringColumn",
        "ToggleMonsterFesteringAt",
        MonsterActionFlags::FESTERING_CURSE,
    ),
    (
        "MonsterCorruptionColumn",
        "ToggleMonsterCorruptionAt",
        MonsterActionFlags::CORRUPTION,
    ),
    (
        "MonsterDestructiveColumn",
        "ToggleMonsterDestructiveAt",
        MonsterActionFlags::DESTRUCTIVE_CURSE,
    ),
    (
        "MonsterCorrosionColumn",
        "ToggleMonsterCorrosionAt",
        MonsterActionFlags::CORROSION,
    ),
];

/// A damage type as the grid names it (`DamageTypeDisplay`).
pub(crate) fn damage_type_display(value: MonsterDamageType) -> String {
    match value {
        MonsterDamageType::Electric => "Lightning".to_owned(),
        MonsterDamageType::VoidBasic | MonsterDamageType::Nether => "Void Basic".to_owned(),
        MonsterDamageType::DrainAuto => "Drain Auto".to_owned(),
        MonsterDamageType::PlayerAuto => "PAuto".to_owned(),
        other => format!("{other:?}"),
    }
}

/// `CycleDamage`: the next value of a cycle, the first for one outside it.
fn cycle_damage(current: MonsterDamageType, cycle: &[MonsterDamageType]) -> MonsterDamageType {
    let next = cycle
        .iter()
        .position(|value| *value == current)
        .map_or(0, |index| (index + 1) % cycle.len());
    cycle[next]
}

/// `EnsureDefaultMonsterRule`: row 0 is always DEFAULT.
pub(crate) fn ensure_default_monster_rule(cx: &mut TankCx<'_>) {
    if cx.core.settings.combat.rules.is_empty() {
        cx.core
            .settings
            .combat
            .rules
            .push(MonsterRule::fresh_default());
    }
}

/// `EscapeMonsterLiteral`: a name as a literal of the monster spec.
pub(crate) fn escape_monster_literal(value: &str) -> String {
    const OPERATORS: &str = "%/*+-#><=&|()";
    let mut result = String::with_capacity(value.len() + 8);
    for character in value.chars() {
        if character.is_ascii_digit() || character == '\\' || OPERATORS.contains(character) {
            result.push('\\');
        }
        result.push(character);
    }
    result
}

pub(super) fn value(cx: &mut TankCx<'_>, name: &str) -> Option<UiValue> {
    if let Some((_, _, flag)) = FLAG_COLUMNS.iter().find(|(column, _, _)| *column == name) {
        return Some(UiValue::BoolList(
            cx.core
                .settings
                .combat
                .rules
                .iter()
                .map(|rule| rule.actions().flags.intersects(*flag))
                .collect(),
        ));
    }
    let count = cx.core.settings.combat.rules.len();
    match name {
        "MonsterNameColumn" => tl(cx
            .core
            .settings
            .combat
            .rules
            .iter()
            .map(|rule| rule.expression().to_owned())
            .collect()),
        "MonsterPriorityColumn" => tl(cx
            .core
            .settings
            .combat
            .rules
            .iter()
            .map(|rule| rule.actions().priority.to_string())
            .collect()),
        "MonsterDamageColumn" => tl(column(cx, |actions| {
            damage_type_display(actions.damage_type)
        })),
        "MonsterExtraVulnColumn" => tl(column(cx, |actions| {
            damage_type_display(actions.extra_vulnerability)
        })),
        "MonsterPetDamageColumn" => tl(column(cx, |actions| {
            damage_type_display(actions.pet_damage_type)
        })),
        "MonsterWeaponColumn" => {
            let rules: Vec<MonsterRuleActions> = actions(cx);
            tl(rules
                .iter()
                .map(|actions| {
                    cx.rule_weapon_display_name(actions.weapon_object_id, &actions.weapon_name)
                })
                .collect())
        }
        "MonsterOffhandColumn" => {
            let rules: Vec<MonsterRuleActions> = actions(cx);
            tl(rules
                .iter()
                .map(|actions| {
                    cx.item_display_name(actions.offhand_object_id, &actions.offhand_name)
                })
                .collect())
        }
        "MonsterMoveUpIcons" => Some(UiValue::UintList(vec![MOVE_UP_ICON; count])),
        "MonsterMoveDownIcons" => Some(UiValue::UintList(vec![MOVE_DOWN_ICON; count])),
        "MonsterFillerColumn" => tl(Vec::new()),
        "SelectedMonsterListRow" => i(cx.core.panel.monster_list_selected_row),
        "MonsterExpressionDraft" => t(cx.core.panel.monster_expression_draft.clone()),
        "MonsterEditorNotice" => t(cx.core.panel.monster_editor_notice.clone()),
        "MonsterEquipmentText" => {
            let default = cx
                .core
                .settings
                .combat
                .rules
                .iter()
                .find(|rule| rule.is_default())
                .map_or_else(MonsterRuleActions::default, |rule| rule.actions().clone());
            let weapon = cx.item_display_name(default.weapon_object_id, &default.weapon_name);
            let offhand = cx.item_display_name(default.offhand_object_id, &default.offhand_name);
            t(format!("Weapon {weapon}   Offhand {offhand}"))
        }
        _ => None,
    }
}

fn actions(cx: &TankCx<'_>) -> Vec<MonsterRuleActions> {
    cx.core
        .settings
        .combat
        .rules
        .iter()
        .map(|rule| rule.actions().clone())
        .collect()
}

fn column(cx: &TankCx<'_>, cell: impl Fn(&MonsterRuleActions) -> String) -> Vec<String> {
    cx.core
        .settings
        .combat
        .rules
        .iter()
        .map(|rule| cell(rule.actions()))
        .collect()
}

pub(super) fn act(cx: &mut TankCx<'_>, name: &str, arg: &Arg) -> bool {
    if let Some((_, _, flag)) = FLAG_COLUMNS.iter().find(|(_, handler, _)| *handler == name) {
        let flag = *flag;
        cx.update_monster_actions_at(arg.int(), |mut actions| {
            actions.flags = MonsterActionFlags(actions.flags.0 ^ flag.0);
            actions
        });
        return true;
    }
    match name {
        "SelectMonsterListRow" => cx.core.panel.monster_list_selected_row = arg.int(),
        "SetMonsterExpressionDraft" => {
            arg.text()
                .clone_into(&mut cx.core.panel.monster_expression_draft);
        }
        // A rule added under a blank caption would match nothing all
        // session: with no expression typed, the selected creature's name
        // is used, and with nothing selected either, nothing is added.
        "AddMonsterRule" => {
            let draft = cx.core.panel.monster_expression_draft.clone();
            if mosstank_dotnet::text::casing::is_white_space_only(&draft) {
                cx.add_selected_monster();
                return true;
            }
            cx.add_monster_rule(&draft);
            cx.core.panel.monster_expression_draft.clear();
        }
        "AddSelectedMonster" => cx.add_selected_monster(),
        "DeleteMonsterRuleAt" => cx.delete_monster_rule_at(arg.int()),
        "CycleMonsterPriorityAt" => cx.update_monster_actions_at(arg.int(), |mut actions| {
            let mut next = actions.bounded_priority() + 1;
            if next == 5 {
                next = -1;
            }
            actions.priority = next;
            actions
        }),
        "CycleMonsterDamageAt" => cx.update_monster_actions_at(arg.int(), |mut actions| {
            actions.damage_type = cycle_damage(actions.damage_type, &MONSTER_DAMAGE_CYCLE);
            actions
        }),
        "CycleMonsterExtraVulnerabilityAt" => {
            cx.update_monster_actions_at(arg.int(), |mut actions| {
                actions.extra_vulnerability = cycle_damage(
                    actions.extra_vulnerability,
                    &MONSTER_EXTRA_VULNERABILITY_CYCLE,
                );
                actions
            });
        }
        "CycleMonsterPetDamageAt" => cx.update_monster_actions_at(arg.int(), |mut actions| {
            actions.pet_damage_type =
                cycle_damage(actions.pet_damage_type, &MONSTER_PET_DAMAGE_CYCLE);
            actions
        }),
        "CycleMonsterWeaponAt" => cx.cycle_monster_equipment_at(arg.int(), false),
        "CycleMonsterOffhandAt" => cx.cycle_monster_equipment_at(arg.int(), true),
        "MoveMonsterRuleUpAt" => cx.move_monster_rule_at(arg.int(), -1),
        "MoveMonsterRuleDownAt" => cx.move_monster_rule_at(arg.int(), 1),
        "MonsterFillerClick" => {}
        _ => return false,
    }
    true
}

impl TankCx<'_> {
    /// `RuleWeaponDisplayName`: a pick the Items list no longer carries
    /// reads INVALID, as the Items page says of its own missing rows.
    pub(crate) fn rule_weapon_display_name(
        &mut self,
        object_id: u32,
        durable_name: &str,
    ) -> String {
        if object_id == 0 && mosstank_dotnet::text::casing::is_white_space_only(durable_name) {
            return "<AUTO>".to_owned();
        }
        if is_profiled(&self.core.settings.combat, object_id, durable_name) {
            return self.item_display_name(object_id, durable_name);
        }
        if object_id != 0 {
            format!("<INVALID 0x{object_id:08X}>")
        } else {
            format!("<INVALID {durable_name}>")
        }
    }

    /// `ItemDisplayName`.
    pub(crate) fn item_display_name(&mut self, object_id: u32, durable_name: &str) -> String {
        if !mosstank_dotnet::text::casing::is_white_space_only(durable_name) {
            return durable_name.to_owned();
        }
        if object_id == 0 {
            return "<AUTO>".to_owned();
        }
        self.client
            .rules()
            .owned_items()
            .into_iter()
            .find(|item| item.object_id == object_id)
            .map_or_else(|| format!("0x{object_id:08X}"), |item| item.name)
    }

    /// `AddMonsterRuleCore`.
    fn add_monster_rule(&mut self, expression: &str) {
        match MonsterRule::fresh(expression) {
            Ok(rule) => {
                self.core.settings.combat.rules.push(rule);
                self.core.panel.monster_editor_notice = format!("Added {expression}.");
                self.core.save_profile(self.client);
            }
            Err(error) => self.core.panel.monster_editor_notice = error,
        }
    }

    /// `AddSelectedMonsterCore`.
    fn add_selected_monster(&mut self) {
        let Some(name) = self.resolve_selected_monster_name() else {
            "Select a monster in the world first."
                .clone_into(&mut self.core.panel.monster_editor_notice);
            return;
        };
        self.add_monster_rule(&escape_monster_literal(&name));
    }

    /// `TryResolveSelectedMonsterName`: a hostile creature's name, else a
    /// monster's from the object table.
    fn resolve_selected_monster_name(&mut self) -> Option<String> {
        let selected = self.client.rules().selected_object().unwrap_or(0);
        if selected == 0 {
            return None;
        }
        if let Some(candidate) = self
            .client
            .rules()
            .hostile_targets(f32::MAX)
            .into_iter()
            .find(|candidate| candidate.object_id == selected)
        {
            return (!mosstank_dotnet::text::casing::is_white_space_only(&candidate.name))
                .then_some(candidate.name);
        }
        self.client
            .rules()
            .object(selected)
            .filter(|world| {
                world.object_class == ObjectClass::Monster
                    && !mosstank_dotnet::text::casing::is_white_space_only(&world.name)
            })
            .map(|world| world.name)
    }

    /// `DeleteMonsterRuleAtCore`: DEFAULT stays.
    fn delete_monster_rule_at(&mut self, row: i32) {
        let rules = &mut self.core.settings.combat.rules;
        let Some(at) = super::row(row, rules.len()) else {
            return;
        };
        if rules[at].is_default() {
            "DEFAULT cannot be removed.".clone_into(&mut self.core.panel.monster_editor_notice);
            return;
        }
        let removed = rules.remove(at);
        self.core.panel.monster_editor_notice = format!("Removed {}.", removed.expression());
        ensure_default_monster_rule(self);
        self.core.save_profile(self.client);
    }

    /// `CycleMonsterEquipmentAt`: the next of the Items page's names, then
    /// automatic.
    fn cycle_monster_equipment_at(&mut self, row: i32, offhand: bool) {
        let mut names: Vec<String> = self
            .core
            .settings
            .combat
            .combat_item_names
            .iter()
            .cloned()
            .collect();
        names.sort_by(|a, b| mosstank_dotnet::text::ordinal::compare(a, b));
        let owned = self.client.rules().owned_items();
        self.update_monster_actions_at(row, move |mut actions| {
            let current = if offhand {
                &actions.offhand_name
            } else {
                &actions.weapon_name
            };
            let current_index = if current.is_empty() {
                None
            } else {
                names.iter().position(|name| name == current)
            };
            let next = match current_index {
                None => names.first(),
                Some(index) => names.get(index + 1),
            };
            let name = next.cloned().unwrap_or_default();
            let object_id = if name.is_empty() {
                0
            } else {
                owned
                    .iter()
                    .find(|item| item.name == name)
                    .map_or(0, |item| item.object_id)
            };
            if offhand {
                actions.offhand_object_id = object_id;
                actions.offhand_name = name;
            } else {
                actions.weapon_object_id = object_id;
                actions.weapon_name = name;
            }
            actions
        });
    }

    /// `MoveMonsterRuleAtCore`: DEFAULT never moves and is never moved past.
    fn move_monster_rule_at(&mut self, row: i32, direction: i32) {
        let rules = &mut self.core.settings.combat.rules;
        let Some(at) = super::row(row, rules.len()) else {
            return;
        };
        if rules[at].is_default() {
            return;
        }
        let Some(destination) = super::row(row + direction, rules.len()) else {
            return;
        };
        if rules[destination].is_default() {
            return;
        }
        let current = rules.remove(at);
        let expression = current.expression().to_owned();
        rules.insert(destination, current);
        self.core.panel.monster_editor_notice = format!("Moved {expression}.");
        self.core.save_profile(self.client);
    }

    /// `UpdateMonsterActionsAt`: the row's spec kept, its actions replaced.
    fn update_monster_actions_at(
        &mut self,
        row: i32,
        update: impl FnOnce(MonsterRuleActions) -> MonsterRuleActions,
    ) {
        let rules = &mut self.core.settings.combat.rules;
        let Some(at) = super::row(row, rules.len()) else {
            return;
        };
        let actions = update(rules[at].actions().clone());
        rules[at] = rules[at].with_actions(actions);
        self.core.save_profile(self.client);
    }
}

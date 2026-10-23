//! The `/vt` verbs that test and dump what the macro sees
//! (`MossTankCommands`): `equipitemsfor`, `testitem`, `propertydump`,
//! `testmonster`, `testspell`, `testpet`, `dumpmetavars`,
//! `listmetafunctions`, `metafunchelp`, `clearbusy`, `dumpspells`,
//! `dumpspecies` and `dumpskills`, each in the C#'s exact words.

use std::cell::RefCell;

use ac_plugin_abi::combat::CombatTarget;
use mosstank_automation::RoomAhead;
use mosstank_combat::controller::Ctx;
use mosstank_combat::pets::{self, PET_ROOM_AHEAD_METERS};
use mosstank_dotnet::text::ordinal;
use mosstank_expr::runtime::ExpressionVariableScope;
use openac_plugin_compat::number::{self, NumberStyles};

use super::rules::combat_turn;
use super::{Core, TankCx};
use crate::client::TankClient;

/// .NET's `bool.ToString()`.
fn pascal(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

/// `EquipItemsFor`: one equipment step towards the named monster's gear.
pub(super) fn equip_items_for(cx: &mut TankCx<'_>, monster_name: &str) {
    if monster_name.is_empty() {
        Core::write_vtank(cx.client, "Syntax: /vt equipitemsfor [monster name]");
        Core::write_vtank(
            cx.client,
            "NOTE: each use of this command invokes one equipment step; multiple calls may be required.",
        );
        return;
    }
    let ready = {
        let core = &mut *cx.core;
        let mut turn = combat_turn!(cx.client, core);
        core.combat
            .equip_one_step_for_monster(&mut turn, monster_name)
    };
    cx.core.forward_cast_outcomes();
    cx.core.write_out(cx.client);
    Core::write_vtank(
        cx.client,
        &format!(
            "Changing items for monster \"{monster_name}\", ready: {}",
            pascal(ready)
        ),
    );
}

/// `TestSelectedItem`: what the loot rules decide for the selected item.
pub(super) fn test_selected_item(cx: &mut TankCx<'_>) {
    let automation = cx.client.rules();
    let selected = automation.selected_object().unwrap_or(0);
    let item = if selected == 0 {
        None
    } else {
        automation
            .owned_items_shared()
            .iter()
            .find(|item| item.object_id == selected)
            .cloned()
    };
    let Some(item) = item else {
        Core::write_vtank(cx.client, "TestItem: pick an item first.");
        return;
    };
    let Some(properties) = automation.item_properties(item.object_id) else {
        let _ = automation.identify(item.object_id);
        Core::write_vtank(cx.client, "TestItem: Waiting for appraisal data.");
        return;
    };
    let owned = automation.owned_items_shared();
    let (decision, _) = mosstank_loot::rules::decide(
        &item,
        &properties,
        &cx.core.settings.inventory.loot.rules,
        &owned,
        None,
        Some(automation),
    );
    let line = match decision {
        Some(decision) => format!(
            "TestItem: {} => {:?} ({}, priority {}).",
            item.name, decision.action, decision.rule_name, decision.priority
        ),
        None => format!("TestItem: {} => NoLoot.", item.name),
    };
    Core::write_vtank(cx.client, &line);
}

/// `DumpSelectedProperties`: every property of the selected object.
pub(super) fn dump_selected_properties(client: &mut dyn TankClient) {
    let selected = client.rules().selected_object().unwrap_or(0);
    let found = if selected == 0 {
        None
    } else {
        client.meta().object(selected).and_then(|item| {
            client
                .meta()
                .properties(selected)
                .map(|properties| (item, properties))
        })
    };
    let Some((item, properties)) = found else {
        Core::write_vtank(
            client,
            "Propertydump: Either no object selected or current selection object is invalid or not appraised.",
        );
        return;
    };
    Core::write_vtank(
        client,
        &format!(
            "Object 0x{:08X}: {}, class={}, WCID={}",
            item.object_id, item.name, item.object_class as u32, item.weenie_class_id
        ),
    );
    dump_table(client, "Int", &properties.ints, |v| v.to_string());
    dump_table(client, "Int64", &properties.int64s, |v| v.to_string());
    dump_table(client, "Bool", &properties.bools, |v| pascal(*v).to_owned());
    dump_table(client, "Float", &properties.floats, |v| {
        number::to_string_f64(*v)
    });
    dump_table(client, "String", &properties.strings, Clone::clone);
    dump_table(client, "DataId", &properties.data_ids, |v| v.to_string());
    dump_table(client, "InstanceId", &properties.instance_ids, |v| {
        v.to_string()
    });
}

/// `DumpPropertyTable`: one line per property, by key.
fn dump_table<T>(
    client: &mut dyn TankClient,
    kind: &str,
    values: &[(u32, T)],
    text: impl Fn(&T) -> String,
) {
    let mut ordered: Vec<&(u32, T)> = values.iter().collect();
    ordered.sort_by_key(|(key, _)| *key);
    for (key, value) in ordered {
        Core::write_vtank(client, &format!("{kind}[{key}] = {}", text(value)));
    }
}

/// `TestSelectedMonster`: the monster rule the selected monster matches.
pub(super) fn test_selected_monster(cx: &mut TankCx<'_>) {
    let automation = cx.client.rules();
    let selected = automation.selected_object().unwrap_or(0);
    let target = automation
        .hostile_targets(f32::MAX)
        .into_iter()
        .find(|value| value.object_id == selected)
        .filter(|target| target.object_id != 0);
    let Some(target) = target else {
        Core::write_vtank(cx.client, "TestMonster: pick a monster first.");
        return;
    };
    let resolved = cx.core.settings.combat.resolve_rule(&target);
    Core::write_vtank(
        cx.client,
        &format!(
            "TestMonster: evaluating monster rules for monster {}, type {}",
            target.name, target.weenie_class_id
        ),
    );
    Core::write_vtank(
        cx.client,
        &format!(
            "Matched '{}', priority {}, damage {:?}, attack={}.",
            resolved.rule.expression(),
            resolved.priority(),
            resolved.actions().damage_type,
            pascal(resolved.actions().uses_primary_attack())
        ),
    );
    if let Some(error) = resolved
        .evaluation_error
        .as_deref()
        .filter(|error| !error.is_empty())
    {
        Core::write_vtank(cx.client, &format!("Expression warning: {error}"));
    }
}

/// `TestSpell`: whether a spell can be cast now.
pub(super) fn test_spell(client: &mut dyn TankClient, argument: &str) {
    let Some(spell_id) = number::parse_u32(argument, NumberStyles::INTEGER) else {
        Core::write_vtank(client, "Syntax: /vt testspell [spellid]");
        return;
    };
    let Some(spell) = client.rules().spells().try_get(spell_id) else {
        Core::write_vtank(client, "Invalid spellid.");
        return;
    };
    let known = client.rules().spells().is_known(spell_id);
    let gate = client.rules().evaluate_gate(spell_id, None);
    Core::write_vtank(client, "---------------------------------");
    // The spell's quality is its difficulty: no host overrides it.
    Core::write_vtank(
        client,
        &format!(
            "Testing ability to cast spell {}, family {}, quality {}, diff {}",
            spell.name, spell.family, spell.difficulty, spell.difficulty
        ),
    );
    Core::write_vtank(
        client,
        &format!("Known: {}, cast gate: {gate:?}", pascal(known)),
    );
    Core::write_vtank(client, "---------------------------------");
}

/// `TestPet`: whether there is room ahead for a pet to appear, the summon
/// rule's last question, and how long asking took; a client that cannot
/// tell answers True, as the rule counts it. Then the essence and monster
/// the rule would choose.
pub(super) fn test_pet(cx: &mut TankCx<'_>) {
    let started = cx.client.clock().monotonic_ms;
    let room = cx.client.rules().check_room_ahead(PET_ROOM_AHEAD_METERS) != RoomAhead::Blocked;
    let finished = cx.client.clock().monotonic_ms;
    #[allow(clippy::cast_precision_loss)]
    let milliseconds = finished.saturating_sub(started) as f64;
    Core::write_vtank(
        cx.client,
        &format!(
            "Pet can spawn: {}, test time: {}ms",
            pascal(room),
            number::to_string_f64(milliseconds)
        ),
    );
    let core = &mut *cx.core;
    let combat = RefCell::new(&mut core.combat);
    let combat_settings = &core.settings.combat;
    let vitals = &core.settings.vitals;
    let game_info = &core.game_info;
    let mut attack_element = |automation: &mut dyn mosstank_automation::Automation,
                              target: &CombatTarget| {
        let mut ctx = Ctx {
            automation,
            settings: combat_settings,
            vitals,
            crafting: None,
        };
        combat.borrow_mut().attack_element_for(&mut ctx, target)
    };
    let damage_preferences = |name: &str| game_info.damage_preferences(name).to_vec();
    let automation = cx.client.rules();
    let owned = automation.owned_items_shared();
    let targets = automation.hostile_targets(pets::pet_range(combat_settings));
    let character = automation.character();
    let summoning = pets::summoning_level(automation);
    let choice = pets::select_pet(
        automation,
        &owned,
        &targets,
        &character,
        summoning,
        combat_settings,
        &mut attack_element,
        &damage_preferences,
        &mut |_, _| {},
        core.summon_pet.refused_types(),
    );
    let line = if choice.is_none() {
        "Pet choice: none".to_owned()
    } else {
        format!(
            "Pet choice: {}, for: {}",
            choice.device.name, choice.target.name
        )
    };
    Core::write_vtank(cx.client, &line);
}

/// `DumpMetaVariables`: every variable of every scope.
pub(super) fn dump_meta_variables(cx: &mut TankCx<'_>) {
    Core::write_vtank(cx.client, "Assigned meta variables:");
    for scope in [
        ExpressionVariableScope::Session,
        ExpressionVariableScope::Persistent,
        ExpressionVariableScope::Global,
    ] {
        let captured = cx.meta.expressions().capture_variables(cx.client, scope);
        let variables = match captured {
            Ok(variables) => variables,
            Err(error) => {
                super::commands::command_failed(cx.client, &error.to_string());
                return;
            }
        };
        for (name, value) in variables {
            Core::write_vtank(
                cx.client,
                &format!("{scope:?}.{name} = {}", value.to_display_string()),
            );
        }
    }
}

/// `ListMetaFunctions`: every function's name, by name.
pub(super) fn list_meta_functions(cx: &mut TankCx<'_>) {
    Core::write_vtank(cx.client, "Meta functions built in to MossTank:");
    let mut names: Vec<String> = cx
        .meta
        .expressions()
        .functions()
        .functions()
        .map(|function| function.name.clone())
        .collect();
    names.sort_by(|a, b| ordinal::compare_ignore_case(a, b));
    write_chunks(cx.client, &names);
}

/// `WriteChunks`: the values joined by ", " in lines of at most 240
/// characters.
fn write_chunks(client: &mut dyn TankClient, values: &[String]) {
    let mut line = String::new();
    let mut length = 0usize;
    for value in values {
        let width = value.encode_utf16().count();
        let extra = if length == 0 { width } else { width + 2 };
        if length != 0 && length + extra > 240 {
            Core::write_vtank(client, &line);
            line.clear();
            length = 0;
        }
        if length != 0 {
            line.push_str(", ");
            length += 2;
        }
        line.push_str(value);
        length += width;
    }
    if length != 0 {
        Core::write_vtank(client, &line);
    }
}

/// `MetaFunctionHelp`: one function's signature, description and argument
/// count.
pub(super) fn meta_function_help(cx: &mut TankCx<'_>, name: &str) {
    let found = cx
        .meta
        .expressions()
        .functions()
        .functions()
        .find(|function| ordinal::equals_ignore_case(&function.name, name))
        .map(|function| {
            (
                function.name.clone(),
                function.signature.clone(),
                function.description.clone(),
                function.minimum_arguments,
                function.maximum_arguments,
            )
        });
    let Some((function, signature, description, minimum, maximum)) = found else {
        Core::write_vtank(cx.client, &format!("Function not defined \"{name}\""));
        return;
    };
    // The C# counts arguments in an int, the unbounded ones as its largest.
    let count = |value: usize| {
        if value == usize::MAX {
            i32::MAX.to_string()
        } else {
            value.to_string()
        }
    };
    let count = if minimum == maximum {
        count(minimum)
    } else {
        format!("{}..{}", count(minimum), count(maximum))
    };
    Core::write_vtank(cx.client, "-------------------------------");
    Core::write_vtank(cx.client, &format!("Function: {function}"));
    Core::write_vtank(cx.client, &format!("Signature: {signature}"));
    Core::write_vtank(cx.client, &format!("Description: {description}"));
    Core::write_vtank(cx.client, &format!("Parameter count: {count}"));
    Core::write_vtank(cx.client, "-------------------------------");
}

/// `/vt clearbusy`: lets go of one busy reference the client holds.
pub(super) fn clear_busy(client: &mut dyn TankClient) {
    let recovery = client.clear_one_busy_reference();
    let line = if recovery.accepted {
        format!(
            "Action busy: {} -> {}.",
            recovery.previous_count, recovery.current_count
        )
    } else {
        recovery.message
    };
    Core::write_vtank(client, &line);
}

/// `DumpSpells`: the combat spells and self buffs the character knows, once
/// each, by id.
pub(super) fn dump_spells(client: &mut dyn TankClient) {
    let mut spells = client.rules().spells().known_combat_spells();
    spells.extend(client.rules().spells().known_self_buffs());
    let mut unique: Vec<ac_plugin_abi::magic::SpellInfo> = Vec::new();
    for spell in spells {
        if !unique.iter().any(|seen| seen.spell_id == spell.spell_id) {
            unique.push(spell);
        }
    }
    unique.sort_by_key(|spell| spell.spell_id);
    Core::write_vtank(client, &format!("Known spell table ({}):", unique.len()));
    for spell in unique {
        Core::write_vtank(
            client,
            &format!(
                "{}\t{}\t{}\t{}",
                spell.spell_id, spell.name, spell.family, spell.difficulty
            ),
        );
    }
}

/// `DumpSpecies`: the species of the monsters around, once each, by id.
pub(super) fn dump_species(client: &mut dyn TankClient) {
    let mut species: Vec<(i32, String)> = Vec::new();
    for target in client.rules().hostile_targets(f32::MAX) {
        if target.species_id != 0 && !species.iter().any(|(id, _)| *id == target.species_id) {
            species.push((target.species_id, target.species_name));
        }
    }
    species.sort_by_key(|(id, _)| *id);
    Core::write_vtank(
        client,
        &format!("Currently observed species ({}):", species.len()),
    );
    for (id, name) in species {
        Core::write_vtank(client, &format!("{id}\t{name}"));
    }
}

/// `DumpSkills`: the character's skills, by id.
pub(super) fn dump_skills(client: &mut dyn TankClient) {
    let mut skills = client.rules().skills();
    Core::write_vtank(client, &format!("Character skills ({}):", skills.len()));
    skills.sort_by_key(|skill| skill.skill_id);
    for skill in skills {
        Core::write_vtank(
            client,
            &format!(
                "{}\t{}\t{}\t{}\t{:?}",
                skill.skill_id, skill.name, skill.base, skill.current, skill.training
            ),
        );
    }
}

//! Where a debuff can come from.

use ac_plugin_abi::character::{Skill, Training};
use ac_plugin_abi::items::InventoryItem;
use ac_plugin_abi::magic::SpellInfo;
use mosstank_automation::StaticSpellCatalog;
use mosstank_combat::debuff::DebuffIdentity;
use mosstank_combat::item_debuff::{
    CombatDebuffSource, CombatDebuffSourceKind, DebuffPlanInput, candidates, sources,
};
use mosstank_data::damage::MonsterDamageType;
use mosstank_formats::fixtures;
use mosstank_formats::gameinfo::{VtankGameInfoDatabase, VtankGrenadeOption};
use mosstank_settings::model::{CombatSettings, DebuffSelectionMethod};
use mosstank_settings::monster::{MonsterActionFlags, MonsterRuleActions};

fn grenades() -> Vec<VtankGrenadeOption> {
    VtankGameInfoDatabase::parse(&fixtures::read_vtank("gameinfodb-excerpt.ugd"))
        .unwrap()
        .grenade_options
}

fn spell(id: u32, name: &str, quality: i32, school: u32) -> SpellInfo {
    SpellInfo {
        spell_id: id,
        name: name.to_owned(),
        family: 1,
        tier: 7,
        difficulty: quality,
        mana_cost: 10,
        duration_seconds: 60.0,
        school,
        is_debuff: true,
        is_offensive: true,
        target_mask: 0x10,
        ..SpellInfo::default()
    }
}

fn item(id: u32, name: &str, item_type: u32, spell_id: u32) -> InventoryItem {
    InventoryItem {
        object_id: id,
        name: name.to_owned(),
        item_type,
        container_id: 1,
        stack_size: 1,
        spell_id,
        ..InventoryItem::default()
    }
}

fn skills(list: &[(u32, u32)]) -> Vec<Skill> {
    list.iter()
        .map(|&(skill_id, current)| Skill {
            skill_id,
            training: Training::Trained,
            current,
            base: current,
            ..Skill::default()
        })
        .collect()
}

/// The test file's catalogue: `known` are the combat spells, `all` the
/// table.
fn catalog(known: &[SpellInfo], all: &[SpellInfo]) -> KnownCatalog {
    KnownCatalog {
        known: known.to_vec(),
        table: StaticSpellCatalog::of(all.to_vec()),
    }
}

struct KnownCatalog {
    known: Vec<SpellInfo>,
    table: StaticSpellCatalog,
}

impl mosstank_automation::SpellCatalog for KnownCatalog {
    fn known_self_buffs(&mut self) -> Vec<SpellInfo> {
        Vec::new()
    }

    fn try_get(&mut self, spell_id: u32) -> Option<SpellInfo> {
        self.table.try_get(spell_id)
    }

    fn known_combat_spells(&mut self) -> Vec<SpellInfo> {
        self.known.clone()
    }
}

fn imperil_rule() -> MonsterRuleActions {
    MonsterRuleActions {
        flags: MonsterActionFlags::IMPERIL,
        ..MonsterRuleActions::default()
    }
}

fn plan(
    actions: &MonsterRuleActions,
    settings: &CombatSettings,
    skills: &[Skill],
    catalog: &mut KnownCatalog,
    items: &[InventoryItem],
) -> Vec<CombatDebuffSource> {
    let grenades = grenades();
    let mut input = DebuffPlanInput {
        settings,
        skills,
        spells: catalog,
        items,
        grenades: &grenades,
        target_distance: 0.0,
    };
    candidates(actions, &mut input, &mut |_, _| true)
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatItemDebuffPlannerTests.cs :: SpellLevelAndSkillMethodsUseOfficialComparisonOrder
#[test]
fn spell_level_and_skill_methods_use_official_comparison_order() {
    let learned = spell(1, "Imperil Other VI", 300, 31);
    let lens_spell = spell(2, "Imperil Other VII", 350, 31);
    let mut catalog = catalog(
        std::slice::from_ref(&learned),
        &[learned.clone(), lens_spell],
    );
    let character = skills(&[(31, 500)]);
    let mut lens = item(100, "Imperil Lens", 0x8000, 2);
    lens.item_spellcraft = 100;
    let mut settings = CombatSettings {
        debuff_selection_method: DebuffSelectionMethod::SPELL_LEVEL,
        ..CombatSettings::default()
    };
    settings.combat_item_object_ids.insert(lens.object_id);
    let actions = imperil_rule();

    let by_level = plan(
        &actions,
        &settings,
        &character,
        &mut catalog,
        &[lens.clone()],
    );
    assert_eq!(by_level[0].kind, CombatDebuffSourceKind::CasterItem);

    settings.debuff_selection_method = DebuffSelectionMethod::SKILL;
    let by_skill = plan(&actions, &settings, &character, &mut catalog, &[lens]);
    assert_eq!(by_skill[0].kind, CombatDebuffSourceKind::LearnedSpell);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatItemDebuffPlannerTests.cs :: SourcesAnswersForOneKindOnlyAndEmitsAuthenticDebuffChoiceLines
#[test]
fn sources_answers_for_one_kind_only_and_emits_authentic_debuff_choice_lines() {
    let imperil = spell(1, "Imperil Other VI", 300, 31);
    let yield_spell = spell(2, "Magic Yield Other VII", 350, 31);
    let lens_spell = spell(3, "Imperil Other VII", 350, 31);
    let mut catalog = catalog(
        &[imperil.clone(), yield_spell.clone()],
        &[imperil, yield_spell, lens_spell],
    );
    let character = skills(&[(31, 500)]);
    let mut lens = item(100, "Imperil Lens", 0x8000, 3);
    lens.item_spellcraft = 900;
    let mut settings = CombatSettings {
        debuff_selection_method: DebuffSelectionMethod::SPELL_LEVEL,
        ..CombatSettings::default()
    };
    settings.combat_item_object_ids.insert(lens.object_id);
    let mut log: Vec<String> = Vec::new();
    let grenades = grenades();
    let items = [lens];
    let mut input = DebuffPlanInput {
        settings: &settings,
        skills: &character,
        spells: &mut catalog,
        items: &items,
        grenades: &grenades,
        target_distance: 0.0,
    };

    let found = sources(
        DebuffIdentity::new(MonsterActionFlags::IMPERIL, MonsterDamageType::Auto),
        &mut input,
        Some(&mut |line: &str| log.push(line.to_owned())),
    );

    assert!(
        found
            .iter()
            .all(|source| source.identity.flag == MonsterActionFlags::IMPERIL)
    );
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].kind, CombatDebuffSourceKind::CasterItem);
    assert!(log.contains(&"Find debuff choice (Imperil Lens [100]): Begin".to_owned()));
    assert!(log.contains(
        &"Find debuff choice (Imperil Lens [100]): Item set to be used, quality 350.".to_owned()
    ));
    assert!(log.contains(&"Find debuff choice (Imperil Lens [100]): Item tests done.".to_owned()));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatItemDebuffPlannerTests.cs :: SourcesLogsAuthenticWrongObjectTypeStopForAProfiledNonSource
#[test]
fn sources_logs_authentic_wrong_object_type_stop_for_a_profiled_non_source() {
    let imperil = spell(1, "Imperil Other VI", 300, 31);
    let mut catalog = catalog(
        std::slice::from_ref(&imperil),
        std::slice::from_ref(&imperil),
    );
    let armour = item(101, "Studded Leather", 0x2, 0);
    let mut settings = CombatSettings::default();
    settings.combat_item_object_ids.insert(armour.object_id);
    let mut log: Vec<String> = Vec::new();
    let grenades = grenades();
    let character = skills(&[(31, 500)]);
    let items = [armour];
    let mut input = DebuffPlanInput {
        settings: &settings,
        skills: &character,
        spells: &mut catalog,
        items: &items,
        grenades: &grenades,
        target_distance: 0.0,
    };

    sources(
        DebuffIdentity::new(MonsterActionFlags::IMPERIL, MonsterDamageType::Auto),
        &mut input,
        Some(&mut |line: &str| log.push(line.to_owned())),
    );

    assert!(log.contains(
        &"Find debuff choice (Studded Leather [101]): Stop, wrong object type".to_owned()
    ));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatItemDebuffPlannerTests.cs :: ItemAndGrenadeSourcesRequireTheirExactProfiles
#[test]
fn item_and_grenade_sources_require_their_exact_profiles() {
    let imperil = spell(1323, "Imperil Other I", 100, 31);
    let mut catalog = catalog(&[], &[imperil]);
    let character = skills(&[(38, 400)]);
    let mut grenade = item(200, "Iron Phial of Imperil", 0x100, 0);
    grenade.combat_use = 0;
    let mut settings = CombatSettings::default();

    assert!(
        plan(
            &imperil_rule(),
            &settings,
            &character,
            &mut catalog,
            &[grenade.clone()]
        )
        .is_empty()
    );

    settings
        .consumable_names
        .insert("Iron Phial of Imperil".to_owned());
    let found = plan(
        &imperil_rule(),
        &settings,
        &character,
        &mut catalog,
        &[grenade],
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, CombatDebuffSourceKind::Grenade);
    assert_eq!(found[0].item_object_id, 200);
    assert_eq!(found[0].source_skill, 110);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatItemDebuffPlannerTests.cs :: AGrenadeIsWhatItsGameDatabaseRowSays
#[test]
fn a_grenade_is_what_its_game_database_row_says() {
    let imperil = spell(1323, "Imperil Other I", 100, 31);
    // The test catalogue decides what spell 1061 is: another imperil.
    let frost = spell(1061, "Imperil Other II", 150, 31);
    let mut catalog = catalog(&[], &[imperil, frost]);
    let lower_case = item(200, "iron phial of imperil", 0x100, 0);
    let frost_phial = item(201, "Fixture Phial of Frost", 0x100, 0);
    let unknown_spell = item(202, "Fixture Phial of Nothing", 0x100, 0);
    let mut settings = CombatSettings::default();
    for name in [&lower_case.name, &frost_phial.name, &unknown_spell.name] {
        settings.consumable_names.insert(name.clone());
    }
    let items = [lower_case, frost_phial, unknown_spell];

    let unskilled = plan(
        &imperil_rule(),
        &settings,
        &skills(&[(38, 250)]),
        &mut catalog,
        &items,
    );
    let skilled = plan(
        &imperil_rule(),
        &settings,
        &skills(&[(38, 350)]),
        &mut catalog,
        &items,
    );

    let ids = |sources: &[CombatDebuffSource]| {
        let mut ids: Vec<u32> = sources.iter().map(|s| s.item_object_id).collect();
        ids.sort_unstable();
        ids
    };
    assert_eq!(
        unskilled
            .iter()
            .map(|s| s.item_object_id)
            .collect::<Vec<_>>(),
        [200]
    );
    assert_eq!(ids(&skilled), [200, 201]);
    assert!(
        skilled
            .iter()
            .all(|source| source.kind == CombatDebuffSourceKind::Grenade)
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/CombatItemDebuffPlannerTests.cs :: ProcWeaponUsesFirstQualifyingAppraisedNonWarSpell
#[test]
fn proc_weapon_uses_first_qualifying_appraised_non_war_spell() {
    let war = spell(10, "Flame Bolt VII", 400, 34);
    let imperil = spell(11, "Imperil Other VII", 350, 31);
    let mut catalog = catalog(&[], &[war, imperil]);
    let mut weapon = item(300, "Debuff Sword", 1, 0);
    weapon.item_spellcraft = 360;
    weapon.appraised_spell_ids = vec![10, 11];
    let mut settings = CombatSettings::default();
    settings.combat_item_object_ids.insert(300);

    let found = plan(&imperil_rule(), &settings, &[], &mut catalog, &[weapon]);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, CombatDebuffSourceKind::ProcWeapon);
    assert_eq!(found[0].spell.spell_id, 11);
}

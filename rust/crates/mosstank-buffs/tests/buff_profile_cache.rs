//! The buff lines cached across passes.

use std::rc::Rc;

use ac_plugin_abi::character::{ActiveEnchantment, Skill, Training};
use ac_plugin_abi::magic::SpellInfo;
use mosstank_buffs::plan::{self, BuffCastability, BuildOptions};
use mosstank_buffs::profile::{self, BuffLine, BuffProfileCache, BuffTargetKind};
use mosstank_settings::model::BuffSettings;

fn spell(id: u32, family: u32) -> SpellInfo {
    SpellInfo {
        spell_id: id,
        name: format!("Spell {id}"),
        family,
        tier: 1,
        difficulty: 50,
        mana_cost: 10,
        duration_seconds: 1800.0,
        school: 33,
        description: "Reduces damage the caster takes from Fire by 9%.".to_owned(),
        is_self_targeted: true,
        is_beneficial: true,
        ..SpellInfo::default()
    }
}

fn plan_count(
    lines: &[BuffLine],
    skills: &[Skill],
    active: &[ActiveEnchantment],
    settings: &BuffSettings,
    castability: Option<&mut dyn BuffCastability>,
) -> usize {
    plan::build(
        lines,
        skills,
        &[],
        active,
        settings,
        BuildOptions {
            castability,
            ..BuildOptions::default()
        },
    )
    .len()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffProfileCacheTests.cs :: ReusesEquivalentContentsWithoutAllocatingOnRepeatedPasses
#[test]
fn reuses_equivalent_contents_without_allocating_on_repeated_passes() {
    // The C# also counts the bytes a thousand unchanged passes allocate;
    // here an unchanged pass hands back the same shared lines.
    let mut cache = BuffProfileCache::new();
    let spells = [spell(1, 1), spell(2, 2)];
    let first = cache.capture(&spells);
    assert!(Rc::ptr_eq(&first, &cache.capture(&spells.clone())));
    for _ in 0..100 {
        assert!(Rc::ptr_eq(&first, &cache.capture(&spells)));
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffProfileCacheTests.cs :: RebuildsAfterInPlaceReplacementAdditionRemovalAndReordering
#[test]
fn rebuilds_after_in_place_replacement_addition_removal_and_reordering() {
    let mut cache = BuffProfileCache::new();
    let mut spells = vec![
        spell(1, 1),
        SpellInfo {
            tier: 2,
            ..spell(2, 1)
        },
    ];
    let original = cache.capture(&spells);
    let check = |cache: &mut BuffProfileCache, spells: &[SpellInfo]| {
        let expected = profile::build(spells);
        let actual = cache.capture(spells);
        assert_eq!(expected.as_slice(), &*actual);
    };
    spells[0] = SpellInfo {
        family: 3,
        tier: 4,
        difficulty: 100,
        description: "Increases the caster's Strength by 10 points.".to_owned(),
        ..spells[0].clone()
    };
    check(&mut cache, &spells);
    assert_eq!(original[0].family, 1);
    assert_eq!(original[0].kind, BuffTargetKind::Protection);
    spells.push(spell(3, 4));
    check(&mut cache, &spells);
    spells.reverse();
    check(&mut cache, &spells);
    spells.remove(1);
    check(&mut cache, &spells);
    spells.clear();
    check(&mut cache, &spells);
    spells.push(spell(4, 1));
    check(&mut cache, &spells);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffProfileCacheTests.cs :: CachedGroupingStillUsesCurrentBuffSettings
#[test]
fn cached_grouping_still_uses_current_buff_settings() {
    let mut cache = BuffProfileCache::new();
    let spells = [spell(1, 1)];
    let mut settings = BuffSettings::default();
    let lines = cache.capture(&spells);
    assert_eq!(plan_count(&lines, &[], &[], &settings, None), 1);
    settings.buff_protections = false;
    assert!(Rc::ptr_eq(&lines, &cache.capture(&spells)));
    assert_eq!(plan_count(&lines, &[], &[], &settings, None), 0);
    settings.buff_protections = true;
    assert_eq!(plan_count(&lines, &[], &[], &settings, None), 1);
}

/// Castable while the formula's first component is 1.
struct ComponentProbe;

impl BuffCastability for ComponentProbe {
    fn is_castable(&mut self, tier: &SpellInfo) -> (bool, Option<String>) {
        (tier.formula_component_ids.first() == Some(&1), None)
    }

    fn note_no_castable_tier(&mut self, _line: &BuffLine) {}
}

fn life_magic(level: u32) -> Skill {
    Skill {
        skill_id: 33,
        name: "Life Magic".to_owned(),
        training: Training::Trained,
        current: level,
        ..Skill::default()
    }
}

fn enchantment(seconds_remaining: f64) -> ActiveEnchantment {
    ActiveEnchantment {
        spell_id: 1,
        family: 1,
        tier: 1,
        seconds_remaining,
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/BuffProfileCacheTests.cs :: ReusedLinesStillObserveSkillsEnchantmentsAndFormulaEdits
#[test]
fn reused_lines_still_observe_skills_enchantments_and_formula_edits() {
    let mut cache = BuffProfileCache::new();
    let mut spells = [SpellInfo {
        formula_component_ids: vec![1],
        ..spell(1, 1)
    }];
    let lines = cache.capture(&spells);
    let settings = BuffSettings::default();
    let mut probe = ComponentProbe;
    let low = [life_magic(50)];
    let high = [life_magic(100)];
    assert_eq!(
        plan_count(&lines, &low, &[], &settings, Some(&mut probe)),
        0
    );
    assert_eq!(
        plan_count(&lines, &high, &[], &settings, Some(&mut probe)),
        1
    );
    assert_eq!(
        plan_count(
            &lines,
            &high,
            &[enchantment(1800.0)],
            &settings,
            Some(&mut probe)
        ),
        0
    );
    assert_eq!(
        plan_count(
            &lines,
            &high,
            &[enchantment(299.0)],
            &settings,
            Some(&mut probe)
        ),
        1
    );

    // The C# edits the formula list the cached line shares, and the reused
    // line sees it. A Rust spell owns its formula, so the edit is a new
    // value: the cache rebuilds, and the plan sees the edit just the same.
    spells[0].formula_component_ids[0] = 2;
    let edited = cache.capture(&spells);
    assert_eq!(
        plan_count(&edited, &high, &[], &settings, Some(&mut probe)),
        0
    );
    spells[0].formula_component_ids = vec![1];
    let replaced = cache.capture(&spells);
    assert!(!Rc::ptr_eq(&edited, &replaced));
    assert_eq!(
        plan_count(&replaced, &high, &[], &settings, Some(&mut probe)),
        1
    );
}

// The shared-list capture answers as the by-value capture: the same
// allocation reuses the lines, an equal copy keeps them, a change rebuilds.
#[test]
fn a_shared_list_is_captured_as_its_contents_are() {
    let mut by_value = BuffProfileCache::new();
    let mut shared = BuffProfileCache::new();
    let lists: Vec<Vec<SpellInfo>> = vec![
        vec![spell(1, 10), spell(2, 10), spell(3, 11)],
        vec![spell(1, 10), spell(2, 10), spell(3, 11)],
        vec![spell(1, 10), spell(3, 11)],
        Vec::new(),
        vec![spell(4, 12)],
    ];
    for list in lists {
        let list: Rc<[SpellInfo]> = Rc::from(list);
        for same in [Rc::clone(&list), list] {
            let expected = by_value.capture(&same);
            let lines = shared.capture_shared(&same);
            assert_eq!(*lines, *expected);
        }
    }
}

// The coverage line's count is the number of lines the profile makes.
#[test]
fn the_line_count_is_the_number_of_lines_built() {
    let mut instant = spell(9, 30);
    instant.duration_seconds = 0.0;
    let mut odd = spell(8, 31);
    odd.duration_seconds = f32::NAN;
    let lists = [
        Vec::new(),
        vec![spell(1, 10)],
        vec![spell(1, 10), spell(2, 10), spell(3, 11), instant.clone()],
        vec![instant, odd, spell(5, 12), spell(6, 13), spell(7, 12)],
    ];
    for list in lists {
        assert_eq!(profile::line_count(&list), profile::build(&list).len());
    }
}

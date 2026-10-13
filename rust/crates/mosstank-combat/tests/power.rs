//! The automatic attack power.

use ac_plugin_abi::character::{Skill, Training};
use ac_plugin_abi::items::InventoryItem;
use mosstank_combat::power::resolve;
use mosstank_data::damage::MonsterDamageType as D;
use mosstank_settings::model::CombatSettings;
use mosstank_settings::monster::MonsterRuleActions;

fn weapon(id: u32, item_type: u32, damage_type: i32, equipped_location: u32) -> InventoryItem {
    InventoryItem {
        object_id: id,
        name: "Weapon".to_owned(),
        item_type,
        container_id: 1,
        valid_locations: equipped_location,
        equipped_location,
        stack_size: 1,
        damage_type,
        ..InventoryItem::default()
    }
}

fn recklessness() -> Vec<Skill> {
    vec![Skill {
        skill_id: 50,
        name: "Recklessness".to_owned(),
        training: Training::Trained,
        current: 300,
        base: 300,
        ..Skill::default()
    }]
}

fn close(expected: f32, actual: Option<f32>) {
    let actual = actual.expect("a power");
    assert!((expected - actual).abs() < 0.005, "{expected} != {actual}");
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AutoAttackPowerTests.cs :: MeleeHybridDecisionTreeMatchesOfficialVtank
#[test]
fn melee_hybrid_decision_tree_matches_official_vtank() {
    let cases: [(D, i32, i32, bool, bool, f32); 5] = [
        (D::Slash, 1, 0, false, false, 0.5),
        (D::Pierce, 1, 0, false, false, 0.0),
        (D::Pierce, 0, 0, false, false, 0.2),
        (D::Pierce, 0, 0x40, true, false, 0.49),
        (D::Pierce, 0, 0x40, false, true, 1.0),
    ];
    for (requested, weapon_type, attack_type, dual_wield, shield, expected) in cases {
        let mut held = weapon(1, 1, 0x3, 0x0010_0000);
        held.weapon_type = weapon_type;
        held.attack_type = attack_type;
        let mut items = vec![held];
        if dual_wield {
            items.push(weapon(2, 1, 0x1, 0x0020_0000));
        }
        if shield {
            items.push(weapon(3, 0x2, 0, 0x0020_0000));
        }
        let settings = CombatSettings {
            use_recklessness: false,
            ..CombatSettings::default()
        };
        let actions = MonsterRuleActions {
            damage_type: requested,
            ..MonsterRuleActions::default()
        };
        close(
            expected,
            resolve(&actions, requested, &settings, &[], &items),
        );
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AutoAttackPowerTests.cs :: MissileAndRecklessnessUseFullThenClampToAuthenticRange
#[test]
fn missile_and_recklessness_use_full_then_clamp_to_authentic_range() {
    let bow = weapon(1, 0x100, 0x2, 0x0040_0000);
    let settings = CombatSettings {
        auto_attack_power: true,
        use_recklessness: true,
        ..CombatSettings::default()
    };
    let actions = MonsterRuleActions {
        damage_type: D::Pierce,
        ..MonsterRuleActions::default()
    };
    close(
        0.9,
        resolve(&actions, D::Pierce, &settings, &recklessness(), &[bow]),
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AutoAttackPowerTests.cs :: DisabledAutoPowerLeavesTheBarAlone
#[test]
fn disabled_auto_power_leaves_the_bar_alone() {
    let settings = CombatSettings {
        auto_attack_power: false,
        attack_power: 0.37,
        ..CombatSettings::default()
    };
    assert!(
        resolve(
            &MonsterRuleActions::default(),
            D::Slash,
            &settings,
            &[],
            &[]
        )
        .is_none()
    );
}

// Source: tests/AcDream.Plugins.MossTank.Tests/AutoAttackPowerTests.cs :: AnUnarmedWeaponSwingsAtNothingForAWrongElement
#[test]
fn an_unarmed_weapon_swings_at_nothing_for_a_wrong_element() {
    let mut fists = weapon(1, 1, 0x3, 0x0010_0000);
    fists.weapon_type = 1;
    let settings = CombatSettings {
        use_recklessness: false,
        ..CombatSettings::default()
    };
    let actions = MonsterRuleActions::default();
    close(
        0.0,
        resolve(
            &actions,
            D::Bludgeon,
            &settings,
            &[],
            std::slice::from_ref(&fists),
        ),
    );
    close(0.5, resolve(&actions, D::Slash, &settings, &[], &[fists]));
}

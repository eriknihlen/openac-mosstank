//! The Loot Own Death Corpse switch on the panel: on by default, set and
//! read through `/vt opt`, saved with the profile, and listed under Looting
//! in the advanced options (ported from the C#'s
//! `MossTankPanelTests.OwnDeathCorpse`). Those tests are not yet in this
//! branch's C# tree, so they are named in "C# source" lines the census does
//! not read; make them Source lines when the C# tree catches up.

mod common;

use common::World;
use mosstank_settings::options::{LOOT_OWN_DEATH_CORPSE, NAMES};

const ENABLED: &str = "LootOwnDeathCorpseEnabled";

// C# source: MossTankPanelTests.OwnDeathCorpse.cs :: LootOwnDeathCorpseIsOnByDefaultSetThroughCommandsAndSurvivesARestart
#[test]
fn loot_own_death_corpse_is_on_by_default_set_through_commands_and_survives_a_restart() {
    let mut world = World::with_character("Prover");
    let mut plugin = world.enable();
    let p = &mut plugin;
    assert!(world.ui_bool(p, ENABLED));

    world.vt(p, "opt set LootOwnDeathCorpse false");
    assert!(!world.ui_bool(p, ENABLED));
    world.vt(p, "opt get lootowndeathcorpse");
    assert!(
        world
            .chat()
            .iter()
            .any(|line| line.contains("Option LootOwnDeathCorpse = False"))
    );

    world.disable(p);
    let mut restarted = world.enable();
    let r = &mut restarted;
    assert!(!world.ui_bool(r, ENABLED));

    world.act(r, "ToggleLootOwnDeathCorpse");
    assert!(world.ui_bool(r, ENABLED));
}

// C# source: MossTankPanelTests.OwnDeathCorpse.cs :: LootOwnDeathCorpseAppearsInLootingAdvancedOptions
#[test]
fn loot_own_death_corpse_appears_in_looting_advanced_options() {
    let mut world = World::with_character("Prover");
    let mut plugin = world.enable();
    let p = &mut plugin;

    assert!(!NAMES.contains(&LOOT_OWN_DEATH_CORPSE));
    world.act(p, "ShowAdvancedOptions");
    world.act_text(p, "SetAdvancedOptionSearchText", LOOT_OWN_DEATH_CORPSE);
    assert_eq!(
        world.ui_list(p, "AdvancedOptionNames"),
        [LOOT_OWN_DEATH_CORPSE]
    );
    world.act_int(p, "SelectAdvancedOption", 0);
    assert!(
        world
            .ui_text(p, "AdvancedOptionDescription")
            .contains("death corpse")
    );

    assert_eq!(world.ui_list(p, "AdvancedOptionValueColumn")[0], "True");
    world.act_int(p, "ClickAdvancedOptionValue", 0);
    assert!(!world.ui_bool(p, ENABLED));
}

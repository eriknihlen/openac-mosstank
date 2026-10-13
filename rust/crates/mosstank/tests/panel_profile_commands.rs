//! The route and meta profile commands as the panel shows their results
//! (ported from the C#'s `MossTankPanelTests`).

mod common;

use common::World;
use mosstank::MossTank;

const LEGACY_NAV: &str = "uTank2 NAV 1.2\n4\n1\n0\n12.5\n-3.25\n0\n0";

const LEGACY_MET: &str = "1\r\nCondAct\r\n5\r\nCType\r\nAType\r\nCData\r\nAData\r\nState\r\nn\r\nn\r\nn\r\nn\r\nn\r\n1\r\ni\r\n1\r\ni\r\n2\r\ni\r\n0\r\ns\r\n/say imported\r\ns\r\nDefault\r\n";

fn fresh() -> (World, MossTank) {
    let mut world = World::panel_character("Commander");
    let plugin = world.enable();
    (world, plugin)
}

fn file(world: &World, key: &str) -> String {
    world
        .profiles
        .get(key)
        .unwrap_or_else(|| panic!("{key}"))
        .to_owned()
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: NavCommandsImportLegacyAndExportAf
#[test]
fn nav_commands_import_a_legacy_route_and_export_both_forms() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    world.plugin_data.insert("imports/Legacy.nav", LEGACY_NAV);
    world.vt(p, "nav load Legacy.nav");
    assert_eq!(world.ui_text(p, "SelectedRouteProfile"), "Legacy");
    let rows = world.ui_list(p, "RouteRows");
    assert_eq!(rows.len(), 1);
    assert!(rows[0].contains("12.5"), "{rows:?}");
    world.vt(p, "nav save Exported.nav");
    assert!(file(&world, "mosstank/navs/Exported.nav").starts_with("uTank2 NAV 1.2\r\n"));
    world.vt(p, "navaf save Exported");
    assert!(file(&world, "mosstank/navs/Exported.af").contains("NAV: "));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: NavSaveAcceptsAnAfSuffixedNameWithoutDoublingIt
#[test]
fn nav_save_takes_an_af_name_without_doubling_it() {
    let (mut world, mut plugin) = fresh();
    world.vt(&mut plugin, "nav save Foo.af");
    assert!(world.profiles.contains("mosstank/navs/Foo.af"));
    assert!(!world.profiles.contains("mosstank/navs/Foo.af.af"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MetaAndRouteProfilesWithTheSameNameDoNotCollide
#[test]
fn a_meta_and_a_route_of_the_same_name_do_not_collide() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    world.plugin_data.insert("imports/Same.met", LEGACY_MET);
    world.vt(p, "meta load Same.met");
    world.vt(p, "meta save Same.met");
    world.vt(p, "nav save Same.nav");
    assert!(file(&world, "mosstank/metas/Same.met").starts_with("1\r\nCondAct\r\n"));
    assert!(file(&world, "mosstank/navs/Same.nav").starts_with("uTank2 NAV 1.2\r\n"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MetaCommandsImportLegacyAndExportAf
#[test]
fn meta_commands_import_a_legacy_meta_and_export_both_forms() {
    let (mut world, mut plugin) = fresh();
    let p = &mut plugin;
    world.plugin_data.insert("imports/Legacy.met", LEGACY_MET);
    world.vt(p, "meta load Legacy.met");
    assert_eq!(world.ui_text(p, "SelectedMetaProfile"), "Legacy");
    let rows = world.ui_list(p, "MetaRows");
    assert_eq!(rows.len(), 1);
    assert!(rows[0].contains("/say imported"), "{rows:?}");

    world.vt(p, "meta save Exported.met");
    let met = mosstank_formats::met::try_load(&file(&world, "mosstank/metas/Exported.met"), None)
        .unwrap();
    assert_eq!(met.rules.len(), 1);

    world.vt(p, "metaaf save Exported");
    let af = file(&world, "mosstank/metas/Exported.af");
    assert!(af.contains("STATE: "));
    let exported = mosstank_formats::af::try_load_meta(&af, None).unwrap();
    assert_eq!(exported.rules.len(), 1);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: MetaSaveAcceptsAnAfSuffixedNameWithoutDoublingIt
#[test]
fn meta_save_takes_an_af_name_without_doubling_it() {
    let (mut world, mut plugin) = fresh();
    world.vt(&mut plugin, "meta save Foo.af");
    assert!(world.profiles.contains("mosstank/metas/Foo.af"));
    assert!(!world.profiles.contains("mosstank/metas/Foo.af.af"));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/MossTankPanelTests.cs :: ByCharacterProfilesAreIsolatedByCharacterName
#[test]
fn by_character_profiles_are_isolated_by_character_name() {
    let mut world = World::panel_character("One");
    let mut first = world.enable();
    world.act_float(&mut first, "SetNormalHealth", 0.33);
    world.disable(&mut first);
    world.fake.character.as_mut().unwrap().name = "Two".to_owned();
    let mut second = world.enable();
    assert!((world.ui_float(&mut second, "NormalHealthValue") - 0.75).abs() < 0.005);
}

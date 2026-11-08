//! The Mag settings: Mag-Tools' own names and defaults (its
//! `SettingsManager.cs`), as rows of the UB settings catalogue, Global
//! scope, under a `Mag` category.

use mosstank_mag::settings::*;
use mosstank_settings::ub::definitions;
use mosstank_settings::ub::{UbSettingKind, UbSettingScope, UbSettingValue};

#[test]
fn every_mag_setting_is_a_global_switch_with_mag_tools_default() {
    let expected = [
        (ITEM_INFO_ENABLED, true),
        (ITEM_INFO_SHOW_BUFFED, true),
        (ITEM_INFO_VALUE_BURDEN, false),
        (COMBAT_PERSISTENT, true),
        (COMBAT_EXPORT_ON_LOGOFF, false),
        (COMBAT_SORT_ALPHA, false),
        // Mag-Tools: off; on here, because the inventory search needs it.
        (INVENTORY_LOGGER, true),
    ];
    for (name, default) in expected {
        let row = definitions::all()
            .iter()
            .find(|row| row.name == name)
            .unwrap_or_else(|| panic!("{name} is not in the catalogue"));
        assert_eq!(row.kind, UbSettingKind::Bool, "{name}");
        assert_eq!(row.default, UbSettingValue::from_bool(default), "{name}");
        assert_eq!(row.scope, UbSettingScope::Global, "{name}");
        assert_eq!(row.category(), "Mag", "{name}");
    }
    let mag_rows = definitions::all()
        .iter()
        .filter(|row| row.category() == "Mag")
        .count();
    assert_eq!(mag_rows, expected.len());
}

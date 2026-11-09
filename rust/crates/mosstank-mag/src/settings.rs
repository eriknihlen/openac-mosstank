//! The Mag settings' names: Mag-Tools' own paths (`SettingsManager.cs`)
//! under a `Mag` category of the UB settings catalogue, where their rows
//! and defaults are authored (`mosstank-settings`, `ub::definitions`).

/// Print an item's info line when it is identified (default on).
pub const ITEM_INFO_ENABLED: &str = "Mag.ItemInfoOnIdent.Enabled";
/// Show the buffed values of weapons and casters in it (default on).
pub const ITEM_INFO_SHOW_BUFFED: &str = "Mag.ItemInfoOnIdent.ShowBuffedValues";
/// Show the value and burden in it (default off).
pub const ITEM_INFO_VALUE_BURDEN: &str = "Mag.ItemInfoOnIdent.ShowValueAndBurden";
/// Ask for the appraisal of what the mouse selects, so selecting is enough
/// to print its line (default on; Mag-Tools: off).
pub const ITEM_INFO_LEFT_CLICK: &str = "Mag.ItemInfoOnIdent.LeftClickIdent";
/// Keep the combat stats across sessions (default on).
pub const COMBAT_PERSISTENT: &str = "Mag.CombatTracker.Persistent";
/// Export the current combat stats on logging off (default off).
pub const COMBAT_EXPORT_ON_LOGOFF: &str = "Mag.CombatTracker.ExportOnLogOff";
/// Sort the combat tracker's monsters by name (default off).
pub const COMBAT_SORT_ALPHA: &str = "Mag.CombatTracker.SortAlphabetically";
/// Log the inventory to a file (default on; Mag-Tools: off, but the
/// inventory search reads the files).
pub const INVENTORY_LOGGER: &str = "Mag.InventoryManagement.InventoryLogger";
/// Show the mana overlay (default off; not Mag-Tools': the Mana page's
/// rows drawn on the game).
pub const MANA_SHOW_OVERLAY: &str = "Mag.ManaManagement.ShowOverlay";
/// Where the mana overlay stands, pixels from the screen's left (kept when
/// it is dragged).
pub const MANA_OVERLAY_X: &str = "Mag.ManaManagement.OverlayX";
/// Where the mana overlay stands, pixels from the screen's top.
pub const MANA_OVERLAY_Y: &str = "Mag.ManaManagement.OverlayY";

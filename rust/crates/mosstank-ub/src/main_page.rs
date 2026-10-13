//! The UB page (`MossTankPanel.UbMain`): the short front page of on/off
//! buttons for the tools themselves, with the catalogue one button away.
//! Every button is a view of a catalogue row, read afresh each frame and
//! written through the same write the settings page's value column makes,
//! so flipping one here and looking at it on the settings page can never
//! disagree. The state is said in the caption and repeated in its colour.

use crate::catalog::UbCatalog;
use crate::settings_page::UbSettingsPage;

/// A switch that is on: the green the rest of the window uses for running.
pub const SWITCH_ON_COLOR: u32 = 0xFF7C_CB6B;
/// A switch that is off: the window's dimmed label colour.
pub const SWITCH_OFF_COLOR: u32 = 0xFFB0_A184;

/// One button: its caption's label, the row it is a view of, and whether
/// its caption is the short form (the vendor's test-mode button).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UbSwitch {
    /// The handler's name in the markup.
    pub action: &'static str,
    /// The caption's label.
    pub label: &'static str,
    /// The catalogue row.
    pub row: &'static str,
    /// `T On` rather than `Label: On`.
    pub short: bool,
}

const fn switch(action: &'static str, label: &'static str, row: &'static str) -> UbSwitch {
    UbSwitch {
        action,
        label,
        row,
        short: false,
    }
}

/// The page's buttons, in the page's reading order.
pub const SWITCHES: [UbSwitch; 10] = [
    switch("ToggleUbAutoVendor", "Auto Vendor", "AutoVendor.Enabled"),
    UbSwitch {
        action: "ToggleUbAutoVendorTestMode",
        label: "T",
        row: "AutoVendor.TestMode",
        short: true,
    },
    switch("ToggleUbDungeonMaps", "Dungeon Maps", "DungeonMaps.Enabled"),
    switch(
        "ToggleUbLandscapeMaps",
        "Landscape Maps",
        "LandscapeMaps.Enabled",
    ),
    switch("ToggleUbNametags", "Name Tags", "Nametags.Enabled"),
    switch("ToggleUbVitalSharing", "Vital Sharing", "Sharing.Vitals"),
    switch(
        "ToggleUbReceiveCommands",
        "Receive broadcasts",
        "Networking.ReceiveCommands",
    ),
    switch("ToggleUbNetworkUi", "Network UI", "NetworkUI.Enabled"),
    switch("ToggleUbAliases", "Aliases", "Aliases.Enabled"),
    switch("ToggleUbGameEvents", "Game Events", "GameEvents.Enabled"),
];

/// The button whose handler is `action`.
#[must_use]
pub fn find(action: &str) -> Option<&'static UbSwitch> {
    SWITCHES.iter().find(|button| button.action == action)
}

/// Whether the button's row is on, read afresh.
#[must_use]
pub fn is_on(catalog: &dyn UbCatalog, button: &UbSwitch) -> bool {
    catalog.flag(button.row)
}

/// The button's caption: `Label: On`, or `T On` for the short one.
#[must_use]
pub fn caption(catalog: &dyn UbCatalog, button: &UbSwitch) -> String {
    let state = if is_on(catalog, button) { "On" } else { "Off" };
    if button.short {
        format!("{} {state}", button.label)
    } else {
        format!("{}: {state}", button.label)
    }
}

/// The button's colour, agreeing with its caption.
#[must_use]
pub fn color(catalog: &dyn UbCatalog, button: &UbSwitch) -> u32 {
    if is_on(catalog, button) {
        SWITCH_ON_COLOR
    } else {
        SWITCH_OFF_COLOR
    }
}

/// Presses the button: the row's own write, reported the way the settings
/// page reports it (`FlipUbSwitch`).
pub fn press(catalog: &mut dyn UbCatalog, settings_page: &mut UbSettingsPage, button: &UbSwitch) {
    settings_page.flip_switch(catalog, button.row);
}

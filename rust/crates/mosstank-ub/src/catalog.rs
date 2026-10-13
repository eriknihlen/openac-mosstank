//! The UB settings catalogue as the UB tab reaches it ([`UbCatalog`]): the
//! rows, their values, and the line `/ub opt` prints, over whichever value
//! store the plugin bound the catalogue to.
//!
//! The rows a tool reads are named by constants; a name the catalogue does
//! not have is a mistake in this crate, not in the player's input, which is
//! why [`UbCatalog::require`] panics where the C#'s `Require` throws.

use mosstank_meta::host::{UbDisplay, UbLineKind};
use mosstank_settings::ub::{
    UbSetting, UbSettingCatalog, UbSettingKind, UbSettingValue, UbSettingValues,
};

use crate::chat::{self, UbDisplays};

/// The catalogue, whatever keeps its values.
pub trait UbCatalog {
    /// Every row, in the order the catalogue declares them.
    fn settings(&self) -> &[UbSetting];
    /// The tools, in name order.
    fn categories(&self) -> &[String];
    /// The row of one name (any case, or the original's spelling).
    fn try_get(&self, name: &str) -> Option<UbSetting>;
    /// The value in force.
    fn get(&self, row: &UbSetting) -> UbSettingValue;
    /// Writes a value to the tier (or live owner) the row names.
    fn set(&mut self, row: &UbSetting, value: UbSettingValue);
    /// Whether a write to this row survives the session.
    fn can_save(&self, row: &UbSetting) -> bool;
    /// The short form for the value column.
    fn display(&self, row: &UbSetting) -> String;
    /// The line `/ub opt` prints: `Name (Type) = value`.
    fn full_display_value(&self, row: &UbSetting) -> String;
    /// Puts one row back to what the catalogue says.
    fn reset_to_default(&mut self, row: &UbSetting);

    /// The row of a name this crate reads.
    ///
    /// # Panics
    /// The catalogue has no such row: the name is a constant in this crate,
    /// so that is a mistake here (the C# throws `KeyNotFoundException`).
    fn require(&self, name: &str) -> UbSetting {
        self.try_get(name)
            .unwrap_or_else(|| panic!("No UB setting is named '{name}'."))
    }

    /// The value of a row this crate reads.
    fn value(&self, name: &str) -> UbSettingValue {
        let row = self.require(name);
        self.get(&row)
    }

    /// A switch this crate reads.
    fn flag(&self, name: &str) -> bool {
        self.value(name).boolean
    }

    /// Whether the UtilityBelt side prints its debug lines.
    fn ub_debug(&self) -> bool {
        self.flag(chat::DEBUG_SETTING)
    }
}

impl<V: UbSettingValues> UbCatalog for UbSettingCatalog<V> {
    fn settings(&self) -> &[UbSetting] {
        UbSettingCatalog::settings(self)
    }

    fn categories(&self) -> &[String] {
        UbSettingCatalog::categories(self)
    }

    fn try_get(&self, name: &str) -> Option<UbSetting> {
        UbSettingCatalog::try_get(self, name)
    }

    fn get(&self, row: &UbSetting) -> UbSettingValue {
        UbSettingCatalog::get(self, row)
    }

    fn set(&mut self, row: &UbSetting, value: UbSettingValue) {
        UbSettingCatalog::set(self, row, value);
    }

    fn can_save(&self, row: &UbSetting) -> bool {
        UbSettingCatalog::can_save(self, row)
    }

    fn display(&self, row: &UbSetting) -> String {
        UbSettingCatalog::display(self, row)
    }

    fn full_display_value(&self, row: &UbSetting) -> String {
        UbSettingCatalog::full_display_value(self, row)
    }

    fn reset_to_default(&mut self, row: &UbSetting) {
        UbSettingCatalog::reset_to_default(self, row);
    }
}

/// A kind of UB line's display, as its two settings say
/// (`MossTankPanel.UbMessageDisplay`).
#[must_use]
pub fn message_display(catalog: &dyn UbCatalog, kind: UbLineKind) -> UbDisplay {
    let display = chat::display_setting(kind);
    let color = catalog.value(&format!("{display}.Color")).as_int32();
    UbDisplay {
        show: catalog.flag(&format!("{display}.Enabled")),
        // A text class is a small number; one that does not fit is the
        // number the client is handed all the same (`(int)` to the wire).
        chat_type: u32::from_ne_bytes(color.to_ne_bytes()),
    }
}

impl UbDisplays for dyn UbCatalog + '_ {
    fn display(&self, kind: UbLineKind) -> UbDisplay {
        message_display(self, kind)
    }
}

/// The value kinds' names as .NET prints the enum (`{setting.Kind}`).
#[must_use]
pub fn kind_name(kind: UbSettingKind) -> &'static str {
    match kind {
        UbSettingKind::Bool => "Bool",
        UbSettingKind::Int => "Int",
        UbSettingKind::Single => "Single",
        UbSettingKind::Double => "Double",
        UbSettingKind::String => "String",
        UbSettingKind::Enum => "Enum",
        UbSettingKind::Color => "Color",
        UbSettingKind::Collection => "Collection",
    }
}

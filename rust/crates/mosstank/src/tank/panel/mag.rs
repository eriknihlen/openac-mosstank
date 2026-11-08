//! The Mag tab: Mag-Tools' tracker pages (Combat, Mana, Inv. Items), one
//! sub-tab each, bound to the Mag features' state in
//! [`mosstank_mag::MagParts`], and the button that opens the Inventory
//! window; and the Inventory window's own bindings.

use ac_plugin_abi::ui::UiValue;
use mosstank_mag::MagSubPage;

use super::{Arg, TankTab, b, i, t, tl};
use crate::tank::TankCx;

/// The filters' choice that keeps every row.
const ALL: &str = "All";

pub(super) fn value(cx: &mut TankCx<'_>, name: &str) -> Option<UiValue> {
    let page = cx.mag.sub_page;
    let shown = cx.core.panel.active_tab == TankTab::Mag;
    match name {
        "MagCombatSelected" => b(page == MagSubPage::Combat),
        "MagManaSelected" => b(page == MagSubPage::Mana),
        "MagInvItemsSelected" => b(page == MagSubPage::InvItems),
        "MagCombatVisible" => b(shown && page == MagSubPage::Combat),
        "MagManaVisible" => b(shown && page == MagSubPage::Mana),
        "MagInvItemsVisible" => b(shown && page == MagSubPage::InvItems),

        // The Inventory window.
        "MagInventorySearchText" => t(cx.mag.inventory_search.clone()),
        "MagInventoryCharacters" | "MagInventoryClasses" => tl(vec![ALL.to_owned()]),
        "SelectedMagInventoryCharacter" | "SelectedMagInventoryClass" => t(ALL),
        "MagInventoryRowCharacters"
        | "MagInventoryRowNames"
        | "MagInventoryRowCounts"
        | "MagInventoryRowPlaces" => tl(Vec::new()),
        "SelectedMagInventoryRow" => i(-1),
        "MagInventoryInfo" => t(""),
        _ => None,
    }
}

pub(super) fn act(cx: &mut TankCx<'_>, name: &str, arg: &Arg) -> bool {
    match name {
        "ShowMagCombat" => cx.mag.sub_page = MagSubPage::Combat,
        "ShowMagMana" => cx.mag.sub_page = MagSubPage::Mana,
        "ShowMagInvItems" => cx.mag.sub_page = MagSubPage::InvItems,
        "OpenMagInventory" => {
            cx.client.show_window("inventory", true);
        }

        // The Inventory window.
        "SetMagInventorySearchText" => arg.text().clone_into(&mut cx.mag.inventory_search),
        "SelectMagInventoryCharacter" | "SelectMagInventoryClass" | "SelectMagInventoryRow" => {}
        _ => return false,
    }
    true
}

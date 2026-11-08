//! The Mag tab: Mag-Tools' tracker pages (Combat, Mana, Inv. Items), one
//! sub-tab each, bound to the Mag features' state in
//! [`mosstank_mag::MagParts`], and the button that opens the Inventory
//! window.

use ac_plugin_abi::ui::UiValue;
use mosstank_mag::MagSubPage;

use super::{Arg, TankTab, b};
use crate::tank::TankCx;

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
        _ => None,
    }
}

pub(super) fn act(cx: &mut TankCx<'_>, name: &str, _arg: &Arg) -> bool {
    match name {
        "ShowMagCombat" => cx.mag.sub_page = MagSubPage::Combat,
        "ShowMagMana" => cx.mag.sub_page = MagSubPage::Mana,
        "ShowMagInvItems" => cx.mag.sub_page = MagSubPage::InvItems,
        "OpenMagInventory" => {
            cx.client.show_window("inventory", true);
        }
        _ => return false,
    }
    true
}

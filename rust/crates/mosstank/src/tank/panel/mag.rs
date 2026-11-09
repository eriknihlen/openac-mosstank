//! The Mag tab: Mag-Tools' tracker pages (Combat, Mana, Inv. Items), one
//! sub-tab each, bound to the Mag features' state in
//! [`mosstank_mag::MagParts`], and the button that opens the Inventory
//! window; and the Inventory window's own bindings.

use ac_plugin_abi::ui::UiValue;
use mosstank_mag::MagSubPage;

use super::{Arg, TankTab, b, i, t, tl};
use crate::tank::TankCx;
use crate::tank::mag::INVENTORY_WINDOW;

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

        // The Mana page (pushed only while it is shown).
        "MagManaIcons" => Some(UiValue::UintList(cx.mag.mana.view().object_ids())),
        "MagManaNames" => tl(cx.mag.mana.view().names()),
        "MagManaStates" => Some(UiValue::UintList(cx.mag.mana.view().state_icons())),
        "MagManaAmounts" => tl(cx.mag.mana.view().mana_texts()),
        "MagManaTimes" => tl(cx.mag.mana.view().time_texts()),
        "MagManaNeeded" => t(cx.mag.mana.view().mana_needed_text()),
        "MagManaUnretained" => t(cx.mag.mana.view().unretained_text()),
        // Mag-Tools' list selects nothing.
        "SelectedMagManaRow" => i(-1),

        // The Inventory window.
        "MagInventoryShown" => b(cx.client.mag().ui_view_visible(INVENTORY_WINDOW)),
        "MagInventorySearchText" => t(cx.mag.inventory_search.pattern()),
        "MagInventoryCharacters" => tl(cx.mag.inventory_search.characters()),
        "MagInventoryClasses" => tl(cx.mag.inventory_search.classes()),
        "SelectedMagInventoryCharacter" => t(cx.mag.inventory_search.character_filter()),
        "SelectedMagInventoryClass" => t(cx.mag.inventory_search.class_filter()),
        "MagInventoryRowIcons" => Some(UiValue::UintList(cx.mag.inventory_search.row_icons())),
        "MagInventoryRowCharacters" => tl(cx.mag.inventory_search.row_characters()),
        "MagInventoryRowNames" => tl(cx.mag.inventory_search.row_names()),
        "MagInventoryRowCounts" => tl(cx.mag.inventory_search.row_counts()),
        "MagInventoryRowPlaces" => tl(cx.mag.inventory_search.row_places()),
        "SelectedMagInventoryRow" => i(cx.mag.inventory_search.selected_row()),
        "MagInventoryInfo" => t(cx.mag.inventory_search.info()),
        "MagInventoryStatus" => t(cx.mag.inventory_search.status()),
        _ => None,
    }
}

pub(super) fn act(cx: &mut TankCx<'_>, name: &str, arg: &Arg) -> bool {
    match name {
        "ShowMagCombat" => cx.mag.sub_page = MagSubPage::Combat,
        "ShowMagMana" => {
            cx.mag.sub_page = MagSubPage::Mana;
            cx.refresh_mana_page();
        }
        "ShowMagInvItems" => cx.mag.sub_page = MagSubPage::InvItems,
        // A click on the mana list: Mag-Tools' list did nothing with it.
        "SelectMagManaRow" => {}
        "OpenMagInventory" => {
            cx.client.show_window("inventory", true);
        }

        // The Inventory window.
        "SetMagInventorySearchText" => cx.mag.inventory_search.set_pattern(arg.text()),
        "SelectMagInventoryCharacter" => cx.mag.inventory_search.set_character_filter(arg.text()),
        "SelectMagInventoryClass" => cx.mag.inventory_search.set_class_filter(arg.text()),
        "SelectMagInventoryRow" => {
            let settings = cx.inventory_info_settings();
            let search = &mut cx.mag.inventory_search;
            search.click(cx.client.mag(), arg.int(), settings);
        }
        _ => return false,
    }
    true
}

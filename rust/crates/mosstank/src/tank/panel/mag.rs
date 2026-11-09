//! The Mag tab: Mag-Tools' tracker pages (Combat, Mana, Inv. Items), one
//! sub-tab each, bound to the Mag features' state in
//! [`mosstank_mag::MagParts`], the item info line's two boxes, and the
//! button that opens the Inventory window; and the Inventory window's own
//! bindings.

use ac_plugin_abi::ui::UiValue;
use mosstank_mag::MagSubPage;
use mosstank_mag::combat::gui::CombatPage;
use mosstank_mag::settings;
use mosstank_settings::ub::UbSettingValue;
use mosstank_ub::catalog::UbCatalog;

use super::{Arg, TankTab, b, i, t, tl};
use crate::tank::TankCx;
use crate::tank::mag::INVENTORY_WINDOW;

pub(super) fn value(cx: &mut TankCx<'_>, name: &str) -> Option<UiValue> {
    let page = cx.mag.sub_page;
    let shown = cx.core.panel.active_tab == TankTab::Mag;
    let combat_page = cx.mag.combat_page;
    let combat_shown = shown && page == MagSubPage::Combat;
    match name {
        "MagCombatSelected" => b(page == MagSubPage::Combat),
        "MagManaSelected" => b(page == MagSubPage::Mana),
        "MagInvItemsSelected" => b(page == MagSubPage::InvItems),
        "MagCombatVisible" => b(combat_shown),
        "MagManaVisible" => b(shown && page == MagSubPage::Mana),
        "MagInvItemsVisible" => b(shown && page == MagSubPage::InvItems),
        // The item info line's boxes, beside the sub-tabs.
        "MagItemInfoOnIdent" => b(flag(cx, settings::ITEM_INFO_ENABLED)),
        "MagLeftClickIdent" => b(flag(cx, settings::ITEM_INFO_LEFT_CLICK)),

        // The Combat page: its three tabs.
        "MagCombatCurrentSelected" => b(combat_page == CombatPage::Current),
        "MagCombatPersistentSelected" => b(combat_page == CombatPage::Persistent),
        "MagCombatOptionsSelected" => b(combat_page == CombatPage::Options),
        "MagCombatStatsVisible" => b(combat_shown && combat_page != CombatPage::Options),
        "MagCombatOptionsVisible" => b(combat_shown && combat_page == CombatPage::Options),
        // The stats tab's two lists (pushed only while it is shown).
        "MagCombatMarks" => tl(cx.combat_list().column(0).to_vec()),
        "MagCombatNames" => tl(cx.combat_list().column(1).to_vec()),
        "MagCombatKills" => tl(cx.combat_list().column(2).to_vec()),
        "MagCombatReceived" => tl(cx.combat_list().column(3).to_vec()),
        "MagCombatGiven" => tl(cx.combat_list().column(4).to_vec()),
        "MagCombatDetailNames" => tl(cx.combat_list().detail_column(0)),
        "MagCombatDetailMelee" => tl(cx.combat_list().detail_column(1)),
        "MagCombatDetailMagic" => tl(cx.combat_list().detail_column(2)),
        "MagCombatDetailLabels" => tl(cx.combat_list().detail_column(3)),
        "MagCombatDetailValues" => tl(cx.combat_list().detail_column(4)),
        // Mag-Tools marks the selection with a `*`; its lists highlight
        // nothing.
        "SelectedMagCombatRow" | "SelectedMagCombatDetailRow" => i(-1),
        // The Options tab's boxes are the settings.
        "MagCombatExportOnLogOff" => b(flag(cx, settings::COMBAT_EXPORT_ON_LOGOFF)),
        "MagCombatPersistent" => b(flag(cx, settings::COMBAT_PERSISTENT)),
        "MagCombatSortAlphabetically" => b(flag(cx, settings::COMBAT_SORT_ALPHA)),

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
        "MagManaShowOverlay" => b(flag(cx, settings::MANA_SHOW_OVERLAY)),

        // The Inv. Items page (pushed only while it is shown).
        "MagInvItemsIcons" => Some(UiValue::UintList(cx.mag.inv_items.view().icons.clone())),
        "MagInvItemsNames" => tl(cx.mag.inv_items.view().names.clone()),
        "MagInvItemsCounts" => tl(cx.mag.inv_items.view().counts.clone()),
        "MagInvItemsFiveMinutes" => tl(cx.mag.inv_items.view().five_minutes.clone()),
        "MagInvItemsHour" => tl(cx.mag.inv_items.view().hour.clone()),
        "MagInvItemsHours" => tl(cx.mag.inv_items.view().hours.clone()),
        // Mag-Tools' list selects nothing.
        "SelectedMagInvItemsRow" => i(-1),

        // The Inventory window.
        "MagInventoryShown" => b(cx.client.mag().ui_view_visible(INVENTORY_WINDOW)),
        "MagInventorySearchText" => t(cx.mag.inventory_search.pattern()),
        "MagInventoryClasses" => tl(cx.mag.inventory_search.classes()),
        "SelectedMagInventoryClass" => t(cx.mag.inventory_search.class_filter()),
        "MagInventoryRowIcons" => Some(UiValue::UintList(cx.mag.inventory_search.row_icons())),
        "MagInventoryRowCharacters" => tl(cx.mag.inventory_search.row_characters()),
        "MagInventoryRowNames" => tl(cx.mag.inventory_search.row_names()),
        "MagInventoryRowCounts" => tl(cx.mag.inventory_search.row_counts()),
        "MagInventoryRowSlots" => tl(cx.mag.inventory_search.row_slots()),
        "MagInventoryRowWields" => tl(cx.mag.inventory_search.row_wields()),
        "MagInventoryRowWieldKeys" => tl(cx.mag.inventory_search.row_wield_keys()),
        "MagInventoryRowOds" => tl(cx.mag.inventory_search.row_ods()),
        "MagInventoryRowOdKeys" => tl(cx.mag.inventory_search.row_od_keys()),
        "MagInventoryRowValues" => tl(cx.mag.inventory_search.row_values()),
        "MagInventoryRowValueKeys" => tl(cx.mag.inventory_search.row_value_keys()),
        "MagInventoryRowPlaces" => tl(cx.mag.inventory_search.row_places()),
        "SelectedMagInventoryRow" => i(cx.mag.inventory_search.selected_row()),
        "MagInventoryInfo" => t(cx.mag.inventory_search.info()),
        "MagInventoryStatus" => t(cx.mag.inventory_search.status()),
        _ => None,
    }
}

/// A Mag setting's switch.
fn flag(cx: &TankCx<'_>, name: &str) -> bool {
    UbCatalog::flag(&cx.ub.catalog, name)
}

/// A Mag setting's switch, set from its box.
fn set_flag(cx: &mut TankCx<'_>, name: &str, value: bool) {
    let row = UbCatalog::require(&cx.ub.catalog, name);
    UbCatalog::set(&mut cx.ub.catalog, &row, UbSettingValue::from_bool(value));
}

pub(super) fn act(cx: &mut TankCx<'_>, name: &str, arg: &Arg) -> bool {
    match name {
        "ShowMagCombat" => {
            cx.mag.sub_page = MagSubPage::Combat;
            cx.refresh_combat_page();
        }
        "ShowMagMana" => {
            cx.mag.sub_page = MagSubPage::Mana;
            cx.refresh_mana_page();
        }
        "ShowMagInvItems" => {
            cx.mag.sub_page = MagSubPage::InvItems;
            cx.refresh_inv_items_page();
        }
        "SetMagItemInfoOnIdent" => set_flag(cx, settings::ITEM_INFO_ENABLED, arg.int() != 0),
        "SetMagLeftClickIdent" => set_flag(cx, settings::ITEM_INFO_LEFT_CLICK, arg.int() != 0),

        // The Combat page.
        "ShowMagCombatCurrent" | "ShowMagCombatPersistent" | "ShowMagCombatOptions" => {
            cx.mag.combat_page = match name {
                "ShowMagCombatCurrent" => CombatPage::Current,
                "ShowMagCombatPersistent" => CombatPage::Persistent,
                _ => CombatPage::Options,
            };
            cx.refresh_combat_page();
        }
        "SelectMagCombatRow" => {
            if let Ok(row) = usize::try_from(arg.int()) {
                cx.click_combat_row(row);
            }
        }
        // A click on the detail list: Mag-Tools' list did nothing with it.
        "SelectMagCombatDetailRow" => {}
        "ClearMagCombatCurrent" => {
            cx.mag.combat.clear_current();
            cx.refresh_combat_page();
        }
        "ExportMagCombatCurrent" => {
            cx.export_current_combat_stats();
        }
        "ClearMagCombatPersistent" => {
            cx.mag.combat.clear_persistent(cx.client.mag());
            cx.refresh_combat_page();
        }
        "SetMagCombatExportOnLogOff" => {
            set_flag(cx, settings::COMBAT_EXPORT_ON_LOGOFF, arg.int() != 0);
        }
        "SetMagCombatPersistent" => set_flag(cx, settings::COMBAT_PERSISTENT, arg.int() != 0),
        "SetMagCombatSortAlphabetically" => {
            set_flag(cx, settings::COMBAT_SORT_ALPHA, arg.int() != 0);
        }

        // A click on the mana list: Mag-Tools' list did nothing with it.
        "SelectMagManaRow" => {}
        // The Mana page's box: the overlay follows at once.
        "SetMagManaShowOverlay" => {
            set_flag(cx, settings::MANA_SHOW_OVERLAY, arg.int() != 0);
            cx.mana_overlay_frame();
        }
        // A click on the Inv. Items list: Mag-Tools' list did nothing with it.
        "SelectMagInvItemsRow" => {}
        "OpenMagInventory" => {
            cx.client.show_window("inventory", true);
        }
        // The Inventory window's Refresh: the saved files read again.
        "RefreshMagInventory" => cx.mag.inventory_search.refresh(),

        // The Inventory window.
        "SetMagInventorySearchText" => cx.mag.inventory_search.set_pattern(arg.text()),
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

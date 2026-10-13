//! The Tinker page (`MossTankPanel.Tinker`): bound to the UB tools'
//! [`TinkerPage`](mosstank_ub::tools::tinker_page::TinkerPage), which keeps
//! the page's state and runs the tinker and imbue plans.

use ac_plugin_abi::ui::UiValue;
use mosstank_ub::tools::tinker_page::TinkerPage;

use super::{Arg, i, t, tl};
use crate::tank::TankCx;

pub(super) fn value(cx: &mut TankCx<'_>, name: &str) -> Option<UiValue> {
    match name {
        "TinkerItemName" => t(cx.ub.commands.tools().tinker.item_name()),
        "TinkerNotice" => t(cx.ub.commands.tools().tinker.notice()),
        "TinkerSalvageNames" => tl(cx.ub.commands.tools().tinker.salvage_names().to_vec()),
        "SelectedTinkerSalvage" => t(cx.ub.commands.tools().tinker.selected_salvage()),
        "TinkerMinimumPercentText" => {
            let catalog = &cx.ub.catalog;
            t(cx.ub
                .commands
                .tools_mut()
                .tinker
                .minimum_percent_text(catalog))
        }
        "TinkerMaxTinksChoices" => tl(TinkerPage::max_tinks_choices()),
        "SelectedTinkerMaxTinks" => t(TinkerPage::selected_max_tinks(&cx.ub.catalog)),
        "TinkerRowNumbers" => tl(cx.ub.commands.tools().tinker.tinker_row_numbers()),
        "TinkerRowItems" => tl(cx.ub.commands.tools().tinker.tinker_row_items()),
        "TinkerRowSalvage" => tl(cx.ub.commands.tools().tinker.tinker_row_salvage()),
        "TinkerRowChances" => tl(cx.ub.commands.tools().tinker.tinker_row_chances()),
        "TinkerRowColors" => Some(UiValue::UintList(
            cx.ub.commands.tools().tinker.tinker_row_colors(),
        )),
        "SelectedTinkerRowIndex" => i(cx.ub.commands.tools().tinker.selected_tinker_row_index()),
        "ImbueDamageTypeNames" => tl(TinkerPage::imbue_damage_type_names()),
        "SelectedImbueDamageType" => t(cx.ub.commands.tools().tinker.selected_imbue_damage_type()),
        "ImbueSalvageNames" => tl(cx.ub.commands.tools().tinker.imbue_salvage_names().to_vec()),
        "SelectedImbueSalvage" => t(cx.ub.commands.tools().tinker.selected_imbue_salvage()),
        "ImbueRowNumbers" => tl(cx.ub.commands.tools().tinker.imbue_row_numbers()),
        "ImbueRowItems" => tl(cx.ub.commands.tools().tinker.imbue_row_items()),
        "ImbueRowSalvage" => tl(cx.ub.commands.tools().tinker.imbue_row_salvage()),
        "ImbueRowChances" => tl(cx.ub.commands.tools().tinker.imbue_row_chances()),
        "ImbueRowColors" => Some(UiValue::UintList(
            cx.ub.commands.tools().tinker.imbue_row_colors(),
        )),
        "SelectedImbueRowIndex" => i(cx.ub.commands.tools().tinker.selected_imbue_row_index()),
        _ => None,
    }
}

pub(super) fn act(cx: &mut TankCx<'_>, name: &str, arg: &Arg) -> bool {
    let text = arg.text().to_owned();
    let index = arg.int();
    match name {
        "AddSelectedTinkerItem" => cx.with_ub(|commands, call| {
            commands.tools_mut().tinker.add_selected_item(call);
        }),
        "SelectTinkerSalvage" => cx.ub.commands.tools_mut().tinker.select_salvage(&text),
        "SetTinkerMinimumPercentText" => cx.with_ub(|commands, call| {
            commands
                .tools_mut()
                .tinker
                .set_minimum_percent_text(call, &text);
        }),
        "SelectTinkerMaxTinks" => cx.with_ub(|commands, call| {
            commands.tools_mut().tinker.select_max_tinks(call, &text);
        }),
        "PopulateTinkerList" => cx.with_ub(|commands, call| {
            commands.tools_mut().tinker.populate_tinker_list(call);
        }),
        "StartTinker" => cx.with_ub(|commands, call| {
            commands.tools_mut().tinker.start_tinker(call);
        }),
        "StopTinker" | "StopImbue" => cx.ub.commands.tools_mut().tinker.stop(),
        "SelectTinkerRow" => cx.with_ub(|commands, call| {
            commands.tools_mut().tinker.select_tinker_row(call, index);
        }),
        "SelectImbueDamageType" => cx.with_ub(|commands, call| {
            commands
                .tools_mut()
                .tinker
                .select_imbue_damage_type(call, &text);
        }),
        "SelectImbueSalvage" => cx.with_ub(|commands, call| {
            commands
                .tools_mut()
                .tinker
                .select_imbue_salvage(call, &text);
        }),
        "RefreshImbueList" => cx.with_ub(|commands, call| {
            commands.tools_mut().tinker.refresh_imbue_list(call);
        }),
        "RendAllImbue" => cx.with_ub(|commands, call| {
            commands.tools_mut().tinker.rend_all(call);
        }),
        "StartImbue" => cx.with_ub(|commands, call| {
            commands.tools_mut().tinker.start_imbue(call);
        }),
        "SelectImbueRow" => cx.with_ub(|commands, call| {
            commands.tools_mut().tinker.select_imbue_row(call, index);
        }),
        _ => return false,
    }
    true
}

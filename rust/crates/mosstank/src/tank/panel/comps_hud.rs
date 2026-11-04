//! The Comps HUD's controls: its box on the Consumables page (Show HUD, the
//! layout, Edit items, the summary line, and Add to HUD beside Remove) and
//! the Edit items window. The HUD and the window's state are the UB
//! crate's ([`ComponentHudController`](mosstank_ub::comps_hud::ComponentHudController));
//! the only thing kept here is the window's notice line.

use ac_plugin_abi::ui::UiValue;
use mosstank_ub::comps_hud::{AddRefusal, CarriedKind, HudLayout};

use super::{Arg, b, i, t, tl};
use crate::tank::display::refusal_line;
use crate::tank::{Core, TankCx};

/// The layouts in the order the Consumables page offers them.
const LAYOUTS: [HudLayout; 2] = [HudLayout::Column, HudLayout::Line];

/// What the page says when the layout is picked out of the world.
const NOT_IN_WORLD: &str = "Comps HUD: not in world.";

pub(super) fn value(cx: &mut TankCx<'_>, name: &str) -> Option<UiValue> {
    // The carried rows the window is about to show are the ones its clicks
    // name until the next push (a click carries only a row number).
    if name == "CompsHudCarriedNames" {
        return tl(cx.ub.comps.note_shown_carried());
    }
    let comps = &cx.ub.comps;
    match name {
        "CompsHudShown" => b(comps.enabled()),
        "CompsHudLayoutIndex" => i(LAYOUTS
            .iter()
            .position(|layout| *layout == comps.layout())
            .map_or(0, |index| i32::try_from(index).unwrap_or(0))),
        "CompsHudSummary" => t(comps.summary()),
        "CompsHudEditorVisible" => b(comps.editor_visible()),
        "CompsHudEditorNotice" => t(cx.core.panel.comps_hud_notice.clone()),
        "CompsHudCarriedHeading" => t(comps.carried_heading()),
        "CompsHudRowsHeading" => t(comps.entries_heading()),
        "CompsHudSearch" => t(comps.search()),
        "CompsHudKindNames" => tl(CarriedKind::ALL
            .iter()
            .map(|kind| kind.label().to_owned())
            .collect()),
        "CompsHudKind" => t(comps.kind().label()),
        "SelectedCompsHudCarriedIndex" => i(row_index(comps.selected_carried())),
        "CompsHudCarriedListed" => Some(UiValue::BoolList(
            comps
                .carried_rows()
                .iter()
                .map(|item| comps.is_listed(&item.name))
                .collect(),
        )),
        "CompsHudCarriedIcons" => Some(UiValue::UintList(
            comps
                .carried_rows()
                .iter()
                .map(|item| item.object_id)
                .collect(),
        )),
        "CompsHudCarriedCounts" => tl(comps
            .carried_rows()
            .iter()
            .map(|item| item.count.to_string())
            .collect()),
        "SelectedCompsHudRowIndex" => i(row_index(comps.selected_entry())),
        // The windows draw an item's composed icon from its object; one the
        // character carries none of shows none.
        "CompsHudRowIcons" => Some(UiValue::UintList(
            comps
                .entries()
                .iter()
                .map(|entry| comps.drawn_object(&entry.name).unwrap_or(0))
                .collect(),
        )),
        "CompsHudRowNames" => tl(comps
            .entries()
            .iter()
            .map(|entry| entry.name.clone())
            .collect()),
        "CompsHudRowWarns" => tl(comps
            .entries()
            .iter()
            .map(|entry| {
                if entry.warn > 0 {
                    format!("warn {}", entry.warn)
                } else {
                    String::new()
                }
            })
            .collect()),
        "CompsHudSelectedWarn" => i(comps
            .selected_entry()
            .and_then(|row| comps.entries().get(row))
            .map_or(0, |entry| entry.warn)),
        _ => None,
    }
}

pub(super) fn act(cx: &mut TankCx<'_>, name: &str, arg: &Arg) -> bool {
    let row = usize::try_from(arg.int()).ok();
    let selected = cx.ub.comps.selected_entry();
    match name {
        "SetCompsHudShown" => cx.ub.comps.set_enabled(cx.client.ub(), arg.int() != 0),
        "SetCompsHudLayout" => {
            if let Some(layout) = row.and_then(|index| LAYOUTS.get(index))
                && !cx.ub.comps.set_layout(cx.client.ub(), *layout)
            {
                NOT_IN_WORLD.clone_into(&mut cx.core.panel.profile_notice);
            }
        }
        "ShowCompsHudEditor" => {
            cx.core.panel.comps_hud_notice.clear();
            cx.ub.comps.show_editor(cx.client.ub());
        }
        "HideCompsHudEditor" => cx.ub.comps.hide_editor(),
        "SetCompsHudSearch" => cx.ub.comps.set_search(arg.text()),
        "SelectCompsHudKind" => {
            if let Some(kind) = CarriedKind::from_label(arg.text()) {
                cx.ub.comps.set_kind(kind);
            }
        }
        "SelectCompsHudCarriedRow" => {
            if let Some(row) = row {
                cx.ub.comps.select_carried(row);
            }
        }
        "ToggleCompsHudCarriedAt" => {
            if let Some(row) = row {
                cx.core.panel.comps_hud_notice.clear();
                cx.ub.comps.toggle_carried(cx.client.ub(), row);
            }
        }
        "AddSelectedToCompsHud" => {
            cx.core.panel.comps_hud_notice = match cx.ub.comps.add_selection(cx.client.ub()) {
                Ok(added) => format!("Added {added}."),
                Err(refusal) => refusal_text(&refusal),
            };
        }
        "SelectCompsHudRow" => {
            if let Some(row) = row {
                cx.ub.comps.select_entry(row);
            }
        }
        "SetCompsHudSelectedWarn" => {
            if let Some(selected) = selected {
                cx.ub.comps.set_warn(cx.client.ub(), selected, arg.int());
            }
        }
        "MoveCompsHudRowUp" | "MoveCompsHudRowDown" => {
            if let Some(selected) = selected {
                let up = name == "MoveCompsHudRowUp";
                cx.ub.comps.move_entry(cx.client.ub(), selected, up);
            }
        }
        "RemoveCompsHudRow" => {
            if let Some(selected) = selected {
                cx.ub.comps.remove_at(cx.client.ub(), selected);
            }
        }
        "AddConsumableToCompsHud" => cx.add_selection_to_comps_hud(),
        _ => return false,
    }
    true
}

/// A list's row as the window binds it; -1 selects none.
fn row_index(row: Option<usize>) -> i32 {
    row.and_then(|row| i32::try_from(row).ok()).unwrap_or(-1)
}

/// A refusal as the window's notice says it (the chat line without its
/// "Comps HUD:" prefix).
fn refusal_text(refusal: &AddRefusal) -> String {
    let line = refusal_line(refusal);
    line.strip_prefix("Comps HUD: ")
        .map(|rest| {
            let mut chars = rest.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .unwrap_or(line)
}

impl TankCx<'_> {
    /// Add to HUD on the Consumables page: the item selected in the game,
    /// as `/vt compshud add` and Edit items' Add selected item take it (a
    /// click on a Consumables row takes that row off the page, so no row
    /// stays selected to add). The page's notice says what happened; a
    /// refusal is said in chat too.
    fn add_selection_to_comps_hud(&mut self) {
        let notice = match self.ub.comps.add_selection(self.client.ub()) {
            Ok(added) => format!("Added {added} to the Comps HUD."),
            Err(refusal) => {
                Core::write_vtank(self.client, &refusal_line(&refusal));
                refusal_text(&refusal)
            }
        };
        self.core.panel.profile_notice = notice;
    }
}

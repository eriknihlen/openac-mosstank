//! The Comps HUD's controls: its box on the Consumables page (Show HUD, the
//! layout, Edit items, the summary line, and Add to HUD beside Remove) and
//! the Edit items window. The HUD and the window's state are the UB
//! crate's ([`ComponentHudController`](mosstank_ub::comps_hud::ComponentHudController));
//! the only thing kept here is the window's notice line.

use ac_plugin_abi::ui::UiValue;
use mosstank_loot::crafting::ALL_PEAS;
use mosstank_ub::comps_hud::{AddRefusal, CarriedKind, HudLayout};

use super::{Arg, b, i, t, tl};
use crate::tank::TankCx;
use crate::tank::display::refusal_line;

/// The layouts in the order the Consumables page offers them.
const LAYOUTS: [HudLayout; 2] = [HudLayout::Column, HudLayout::Line];

pub(super) fn value(cx: &mut TankCx<'_>, name: &str) -> Option<UiValue> {
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
                .map(|item| item.icon_id)
                .collect(),
        )),
        "CompsHudCarriedNames" => tl(comps
            .carried_rows()
            .iter()
            .map(|item| item.name.clone())
            .collect()),
        "CompsHudCarriedCounts" => tl(comps
            .carried_rows()
            .iter()
            .map(|item| item.count.to_string())
            .collect()),
        "SelectedCompsHudRowIndex" => i(row_index(comps.selected_entry())),
        "CompsHudRowIcons" => Some(UiValue::UintList(
            comps.entries().iter().map(|entry| entry.icon_id).collect(),
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
            .entries()
            .get(comps.selected_entry())
            .map_or(0, |entry| entry.warn)),
        _ => None,
    }
}

pub(super) fn act(cx: &mut TankCx<'_>, name: &str, arg: &Arg) -> bool {
    let row = usize::try_from(arg.int()).ok();
    match name {
        "SetCompsHudShown" => cx.ub.comps.set_enabled(cx.client.ub(), arg.int() != 0),
        "SetCompsHudLayout" => {
            if let Some(layout) = row.and_then(|index| LAYOUTS.get(index)) {
                cx.ub.comps.set_layout(cx.client.ub(), *layout);
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
            let selected = cx.ub.comps.selected_entry();
            cx.ub.comps.set_warn(cx.client.ub(), selected, arg.int());
        }
        "MoveCompsHudRowUp" | "MoveCompsHudRowDown" => {
            let selected = cx.ub.comps.selected_entry();
            let up = name == "MoveCompsHudRowUp";
            cx.ub.comps.move_entry(cx.client.ub(), selected, up);
        }
        "RemoveCompsHudRow" => {
            let selected = cx.ub.comps.selected_entry();
            cx.ub.comps.remove_at(cx.client.ub(), selected);
        }
        "AddConsumableToCompsHud" => {
            cx.core.panel.profile_notice = cx.add_consumable_to_comps_hud();
        }
        _ => return false,
    }
    true
}

/// A list's row as the window binds it.
fn row_index(row: usize) -> i32 {
    i32::try_from(row).unwrap_or(0)
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
    /// Add to HUD on the Consumables page: the selected row's item, or,
    /// with no row, the item selected in the game; the page's notice.
    fn add_consumable_to_comps_hud(&mut self) -> String {
        let panel = &self.core.panel;
        let selected = usize::try_from(panel.selected_consumable_row).ok();
        let row = selected.and_then(|at| panel.items.consumables.get(at).cloned());
        let Some(name) = row else {
            return match self.ub.comps.add_selection(self.client.ub()) {
                Ok(added) => format!("Added {added} to the Comps HUD."),
                Err(refusal) => refusal_text(&refusal),
            };
        };
        if name == ALL_PEAS {
            return format!("{ALL_PEAS} is every pea, not one item: add a pea by its name.");
        }
        // The icon of the first such item the character owns; one it owns
        // none of yet gets its icon when it is first counted.
        let icon = self
            .client
            .rules()
            .owned_items_shared()
            .iter()
            .find(|item| item.name == name)
            .map_or(0, |item| item.icon_id);
        match self.ub.comps.add(self.client.ub(), &name, icon) {
            Ok(()) => format!("Added {name} to the Comps HUD."),
            Err(refusal) => refusal_text(&refusal),
        }
    }
}

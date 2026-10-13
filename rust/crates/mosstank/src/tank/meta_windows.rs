//! What the meta's drawn windows read and do (`MetaViewBinding`,
//! `PanelMarkupBinding`, `StatusHudManager.StatusBinding`): a view's
//! per-control text and visibility by slot, and its slot's click; a
//! panel-markup view's `WindowAvailable`; the status window's rows. The
//! windows themselves are the plugin's ([`crate::meta_windows`]).

use ac_plugin_abi::ui::UiValue;
use mosstank_meta::view_manager::window_id;

use super::Tank;
use crate::client::TankClient;
use crate::meta_windows::STATUS_WINDOW_ID;

/// A slot binding's number: `Text12` → 12.
fn slot(name: &str, prefix: &str) -> Option<usize> {
    let digits = name.strip_prefix(prefix)?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

impl Tank {
    /// The view a meta window was drawn for, by its window id.
    fn meta_view_for(&mut self, window: &str) -> Option<String> {
        self.meta
            .views()
            .names()
            .find(|name| window_id(name) == window)
            .map(str::to_owned)
    }

    /// The value one of the meta's windows binds, as the C#'s binding
    /// object answers it; `None` for a name it has no value for.
    pub fn meta_window_value(&mut self, window: &str, name: &str) -> Option<UiValue> {
        if window == STATUS_WINDOW_ID {
            let hud = self.meta.expressions().status_hud();
            return match name {
                "WindowAvailable" => Some(UiValue::Bool(true)),
                "Rows" => Some(UiValue::TextList(hud.rows())),
                "RowColors" => Some(UiValue::UintList(hud.row_colors())),
                "SelectedRow" => Some(UiValue::Int(-1)),
                _ => None,
            };
        }
        let view = self.meta_view_for(window)?;
        let Some(parsed) = self.meta.views().find(&view) else {
            // A view drawn from the host's own markup binds only this.
            return (name == "WindowAvailable").then_some(UiValue::Bool(true));
        };
        let controls = parsed.controls();
        if let Some(index) = slot(name, "Text") {
            return Some(UiValue::Text(
                controls
                    .get(index)
                    .map(|control| control.text.clone())
                    .unwrap_or_default(),
            ));
        }
        if let Some(index) = slot(name, "Visible") {
            return Some(UiValue::Bool(
                controls.get(index).is_some_and(|control| control.visible),
            ));
        }
        None
    }

    /// A handler of one of the meta's windows fired: a view's slot click
    /// hits the control it was drawn from (`MetaViewBinding.Click`).
    pub fn meta_window_act(&mut self, client: &mut dyn TankClient, window: &str, handler: &str) {
        let Some(index) = slot(handler, "Click") else {
            return;
        };
        let Some(view) = self.meta_view_for(window) else {
            return;
        };
        self.begin_call(client);
        let (mut cx, _) = self.cx(client);
        cx.with_engine(|engine, host, services| {
            engine.click_view_slot(host, services, &view, index);
        });
        cx.core.write_out(cx.client);
        self.end_call(client);
    }
}

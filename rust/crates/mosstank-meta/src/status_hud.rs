//! VTank's meta status window (`StatusHudManager`): one shelf window whose
//! rows the `statushud` functions set.

use crate::host::{MetaHost, PanelDescriptor};

const DEFAULT_COLOR: u32 = 0x00E8_DEC3;

/// The window's markup; it binds `Rows`, `RowColors` and `SelectedRow`.
pub const MARKUP: &str = r##"<panel x="520" y="42" w="340" h="220" title="VTank Meta Status"
       visible="{WindowAvailable}" resize="none">
  <label x="8" y="25" text="VTank Meta" color="#FFE8DEC3" />
  <list x="8" y="45" w="324" h="166" rowheight="18"
        items="{Rows}" colors="{RowColors}" selected="{SelectedRow}" />
</panel>"##;

/// The rows and the window showing them.
#[derive(Debug, Clone, Default)]
pub struct StatusHud {
    entries: Vec<(String, String, u32)>,
    registration: Option<u32>,
}

impl StatusHud {
    /// How many rows.
    #[must_use]
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// The rows as the window lists them: "key: value".
    #[must_use]
    pub fn rows(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|(key, value, _)| format!("{key}: {value}"))
            .collect()
    }

    /// Each row's colour.
    #[must_use]
    pub fn row_colors(&self) -> Vec<u32> {
        self.entries.iter().map(|(_, _, color)| *color).collect()
    }

    /// The window's handle, once it is shown.
    #[must_use]
    pub fn registration(&self) -> Option<u32> {
        self.registration
    }

    /// Sets a row, showing the window the first time. No key, or no window
    /// host to show the status in, is a plain false.
    pub fn update(
        &mut self,
        host: &mut dyn MetaHost,
        key: &str,
        value: &str,
        color: Option<u32>,
    ) -> bool {
        if key.is_empty() || (self.registration.is_none() && !host.has_ui()) {
            return false;
        }
        let color = color.unwrap_or(DEFAULT_COLOR);
        match self.entries.iter_mut().find(|(name, _, _)| name == key) {
            Some(entry) => {
                value.clone_into(&mut entry.1);
                entry.2 = color;
            }
            None => self.entries.push((key.to_owned(), value.to_owned(), color)),
        }
        if self.registration.is_none() && host.has_ui() {
            self.registration = host.register_panel(
                &PanelDescriptor {
                    window_id: "vtank-meta-status".to_owned(),
                    title: "VTank Meta Status".to_owned(),
                    icon_text: "S".to_owned(),
                    start_visible: true,
                    show_in_side_panel: true,
                },
                MARKUP,
            );
        }
        true
    }

    /// Takes the window down and forgets the rows.
    pub fn destroy(&mut self, host: &mut dyn MetaHost) {
        if let Some(handle) = self.registration.take() {
            host.unregister_panel(handle);
        }
        self.entries.clear();
    }
}

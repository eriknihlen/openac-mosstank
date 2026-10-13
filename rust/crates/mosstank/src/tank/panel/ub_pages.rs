//! The UB page, the UB settings page and its colour and list windows, the
//! clients window, and the countryside and dungeon map windows
//! (`MossTankPanel.UbMain`, `UbSettings`, `UbDisplay`, `UbDungeon`). The
//! pages' state is the UB crate's ([`UbSettingsPage`]); the catalogue and
//! the profiles come in on each call.

use ac_plugin_abi::ui::UiValue;
use ac_plugin_abi::world::ObjectClass;
use mosstank_stores::storage::{RecordingLog, Root};
use mosstank_ub::catalog::CatalogDisplays;
use mosstank_ub::dungeon::controller::{CatalogMapSettings, ROTATE_STEP_DEGREES};
use mosstank_ub::landscape::controller::PAN_STEP_PIXELS;
use mosstank_ub::live::UbProfiles;
use mosstank_ub::main_page;

use super::{Arg, b, clamp_row, f, i, t, tl};
use crate::client::TankClient;
use crate::tank::TankCx;
use crate::tank::ub::{CallCatalog, OwnerReach};

/// The clients window's own state.
#[derive(Debug, Clone, Default)]
pub(crate) struct UbPageState {
    pub(crate) client_names: Vec<String>,
    pub(crate) selected_client: i32,
    pub(crate) clients_notice: String,
}

/// The kinds of object a remote follow takes (`PlayerClasses`).
pub(crate) fn is_player_class(class: ObjectClass) -> bool {
    class == ObjectClass::Player
}

/// The UB settings profiles over the shell's store and the client's
/// storage, for one call.
pub(crate) struct ShellUbProfiles<'a> {
    store: &'a std::cell::RefCell<mosstank_stores::ub_store::UbSettingStore>,
    client: &'a mut dyn TankClient,
}

impl ShellUbProfiles<'_> {
    fn forward(&mut self, log: &RecordingLog) {
        crate::tank::ub::forward_log(self.client, log);
    }
}

impl UbProfiles for ShellUbProfiles<'_> {
    fn profile_name(&self) -> String {
        self.store.borrow().profile_name().to_owned()
    }

    fn available_profiles(&mut self) -> Vec<String> {
        let storage = self.client.stores().storage(Root::Profiles);
        let names = self.store.borrow_mut().available_profiles(storage);
        names.unwrap_or_else(|error| {
            self.client
                .rules()
                .log_warn(&format!("MossTank could not list the UB profiles: {error}"));
            vec![self.profile_name()]
        })
    }

    fn select_profile(&mut self, name: &str) {
        let mut log = RecordingLog::default();
        {
            let storage = self.client.stores().storage(Root::Profiles);
            self.store
                .borrow_mut()
                .select_profile(storage, &mut log, name);
        }
        self.forward(&log);
    }

    fn create_profile(&mut self, name: &str, copy_current: bool) -> (bool, String) {
        let mut log = RecordingLog::default();
        let made = {
            let storage = self.client.stores().storage(Root::Profiles);
            self.store
                .borrow_mut()
                .create_profile(storage, &mut log, name, copy_current)
        };
        self.forward(&log);
        made.unwrap_or_else(|error| (false, format!("The profile could not be made: {error}")))
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn value(cx: &mut TankCx<'_>, name: &str) -> Option<UiValue> {
    if let Some(button) = name.strip_suffix("ButtonText").and_then(switch_for) {
        return t(main_page::caption(&cx.ub.catalog, button));
    }
    if let Some(button) = name.strip_suffix("ButtonColor").and_then(switch_for) {
        return Some(UiValue::Uint(main_page::color(&cx.ub.catalog, button)));
    }
    let page = &cx.ub.page;
    match name {
        "UbSettingNotice" => t(page.notice()),
        "UbCategoryNames" => tl(page.category_names().to_vec()),
        "SelectedUbCategoryIndex" => i(i32::try_from(page.selected_category()).unwrap_or(0)),
        "UbSettingNames" => tl(page.setting_names().to_vec()),
        "UbSettingValues" => tl(page.setting_values().to_vec()),
        "SelectedUbSettingIndex" => i(i32::try_from(page.selected_setting()).unwrap_or(0)),
        "UbFilterText" => t(page.filter_text()),
        "UbSettingDescription" => t(cx.ub.page.description_line(0)),
        "UbSettingDescriptionLine2" => t(cx.ub.page.description_line(1)),
        "UbSettingDescriptionLine3" => t(cx.ub.page.description_line(2)),
        "UbSettingDescriptionLine4" => t(cx.ub.page.description_line(3)),
        "UbSettingScopeText" => {
            let profiles = ShellUbProfiles {
                store: &cx.ub.store,
                client: &mut *cx.client,
            };
            t(cx.ub.page.scope_text(&profiles))
        }
        "UbSettingValueDraft" => t(page.value_draft()),
        "UbProfileNames" => {
            let mut profiles = ShellUbProfiles {
                store: &cx.ub.store,
                client: &mut *cx.client,
            };
            tl(profiles.available_profiles())
        }
        "SelectedUbProfile" => t(cx.ub.store.borrow().profile_name()),
        "UbProfileNameDraft" => t(page.profile_name_draft()),

        // The colour window.
        "UbColorVisible" => b(page.color_visible()),
        "UbColorSwatch" => Some(UiValue::Uint(page.color_swatch())),
        "UbColorTarget" => t(page.color_target()),
        "UbColorAlpha" => f(page.color_channel(24)),
        "UbColorRed" => f(page.color_channel(16)),
        "UbColorGreen" => f(page.color_channel(8)),
        "UbColorBlue" => f(page.color_channel(0)),
        "UbColorAlphaText" => t(page.color_channel_text(24)),
        "UbColorRedText" => t(page.color_channel_text(16)),
        "UbColorGreenText" => t(page.color_channel_text(8)),
        "UbColorBlueText" => t(page.color_channel_text(0)),
        "UbColorHex" => t(page.color_hex()),

        // The list window.
        "UbListVisible" => b(page.list_visible()),
        "UbListTitle" => t(page.list_title()),
        "UbListRows" => tl(page.list_rows().to_vec()),
        "SelectedUbListIndex" => i(i32::try_from(page.selected_list_row()).unwrap_or(0)),
        "UbListDraft" => t(page.list_draft()),
        "UbListNotice" => t(page.list_notice()),

        // The clients window.
        "UbClientsVisible" => b(cx.ub.hud.shown()),
        "UbClientsTagText" => t(cx.ub.hud.tag_text()),
        "UbClientNames" => tl(cx.core.panel.ub.client_names.clone()),
        "SelectedUbClientIndex" => i(cx.core.panel.ub.selected_client),
        "UbClientsNotice" => t(cx.core.panel.ub.clients_notice.clone()),

        // The map window.
        "UbMapVisible" => b(cx.ub.landscape.shown()),
        "UbMapFollowText" => t(cx.ub.landscape.follow_text()),
        "UbMapReadout" => t(cx.ub.landscape.readout()),
        "UbMapMarkerCountText" => t(cx.ub.landscape.marker_count_text()),

        // The dungeon window.
        "UbDungeonVisible" => b(cx.ub.dungeon.shown()),
        "UbDungeonFollowText" => t(cx.ub.dungeon.follow_text()),
        "UbDungeonLayersText" => t(cx.ub.dungeon.layers_text()),
        #[allow(clippy::cast_precision_loss)]
        "UbDungeonHeight" => f(cx.ub.dungeon.height_percent() as f32),
        "UbDungeonStatusText" => t(cx.ub.dungeon.status()),
        _ => None,
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn act(cx: &mut TankCx<'_>, name: &str, arg: &Arg) -> bool {
    // The catalogue as a window action writes it: the rows kept in storage
    // are written at once, through the client.
    macro_rules! writes {
        ($parts:ident) => {
            &mut CallCatalog {
                catalog: &mut $parts.catalog,
                reach: OwnerReach::Direct(&mut *cx.client, &mut *cx.core),
            }
        };
    }
    if let Some(button) = main_page::find(name) {
        let parts = &mut *cx.ub;
        main_page::press(writes!(parts), &mut parts.page, button);
        return true;
    }
    let text = arg.text().to_owned();
    let index = i64::from(arg.int());
    let parts = &mut *cx.ub;
    match name {
        "SelectUbCategory" => parts.page.select_category(&parts.catalog, index),
        "SelectUbSetting" => parts.page.select_setting(&parts.catalog, index),
        "ClickUbSettingValue" => parts.page.click_value(writes!(parts), index),
        "SetUbFilterText" => parts.page.set_filter_text(&parts.catalog, &text),
        "SetUbSettingValueDraft" => parts.page.set_value_draft(&text),
        "SubmitUbSetting" => parts.page.submit(writes!(parts), &text),
        "SubmitUbSettingDraft" => parts.page.submit_draft(writes!(parts)),
        "ResetUbSettingToDefault" => parts.page.reset_to_default(writes!(parts)),
        "SelectUbProfile" | "CreateNamedUbProfile" | "CreateUbProfile" | "CopyUbProfile" => {
            let mut profiles = ShellUbProfiles {
                store: &parts.store,
                client: &mut *cx.client,
            };
            match name {
                "SelectUbProfile" => {
                    parts
                        .page
                        .select_profile(&parts.catalog, &mut profiles, &text)
                }
                "CreateNamedUbProfile" => {
                    parts
                        .page
                        .create_named_profile(&parts.catalog, &mut profiles, &text);
                }
                "CreateUbProfile" => parts.page.create_profile(&parts.catalog, &mut profiles),
                _ => parts.page.copy_profile(&parts.catalog, &mut profiles),
            }
        }
        "SetUbProfileNameDraft" => parts.page.set_profile_name_draft(&text),

        "SetUbColorAlpha" => parts.page.set_color_channel(24, arg.float()),
        "SetUbColorRed" => parts.page.set_color_channel(16, arg.float()),
        "SetUbColorGreen" => parts.page.set_color_channel(8, arg.float()),
        "SetUbColorBlue" => parts.page.set_color_channel(0, arg.float()),
        "SetUbColorHex" | "SubmitUbColorHex" => parts.page.set_color_hex(&text),
        "ApplyUbColor" => parts.page.apply_color(writes!(parts)),
        "HideUbColorEditor" => parts.page.hide_color_editor(),
        "ShowUbColorEditor" => parts.page.show_color_editor(&parts.catalog),
        "ResetUbColorToDefault" => parts.page.reset_color_to_default(&parts.catalog),

        "SelectUbListRow" => parts.page.select_list_row(index),
        "SetUbListDraft" => parts.page.set_list_draft(&text),
        "AddUbListEntry" => parts.page.add_list_entry(writes!(parts), &text),
        "AddUbListEntryFromDraft" => parts.page.add_list_entry_from_draft(writes!(parts)),
        "ReplaceUbListEntry" => parts.page.replace_list_entry(writes!(parts)),
        "RemoveUbListEntry" => parts.page.remove_list_entry(writes!(parts)),
        "HideUbListEditor" => parts.page.hide_list_editor(),
        "ShowUbListEditor" => parts.page.show_list_editor(&parts.catalog),

        "ShowUbClients" => parts.hud.set_shown(true),
        "HideUbClients" => parts.hud.set_shown(false),
        "SelectUbClient" => {
            let count = cx.core.panel.ub.client_names.len();
            cx.core.panel.ub.selected_client = clamp_row(arg.int(), count);
            let picked = cx.ub.hud.select_row(cx.client.ub(), arg.int());
            cx.core.panel.ub.clients_notice =
                picked.map_or_else(String::new, |name| format!("Selected {name}."));
        }

        "ShowUbMap" => {
            let storage = cx.client.stores().storage(Root::Profiles);
            let loaded = cx.ub.landscape.load_markers(storage);
            if let Err(error) = loaded {
                cx.client
                    .rules()
                    .log_warn(&format!("MossTank could not read the map markers: {error}"));
            }
            cx.ub.landscape.set_shown(true);
        }
        "HideUbMap" => parts.landscape.set_shown(false),
        "ToggleUbMapFollow" => {
            let player = mosstank_ub::landscape::controller::self_position(cx.client.meta());
            cx.ub.landscape.toggle_follow(player);
        }
        "UbMapZoomIn" => parts.landscape.zoom_in(),
        "UbMapZoomOut" => parts.landscape.zoom_out(),
        "UbMapPanLeft" => parts.landscape.pan(PAN_STEP_PIXELS, 0.0),
        "UbMapPanRight" => parts.landscape.pan(-PAN_STEP_PIXELS, 0.0),
        "UbMapPanUp" => parts.landscape.pan(0.0, PAN_STEP_PIXELS),
        "UbMapPanDown" => parts.landscape.pan(0.0, -PAN_STEP_PIXELS),

        "ShowUbDungeon" => parts.dungeon.set_shown(true),
        "HideUbDungeon" => parts.dungeon.set_shown(false),
        "ToggleUbDungeonFollow" => parts.dungeon.toggle_follow(),
        "ToggleUbDungeonLayers" => {
            let all = parts.dungeon.show_all_layers();
            parts.dungeon.set_show_all_layers(!all);
        }
        "UbDungeonZoomIn" => {
            let mut settings = CatalogMapSettings(&mut parts.catalog);
            parts.dungeon.zoom_in(&mut settings);
        }
        "UbDungeonZoomOut" => {
            let mut settings = CatalogMapSettings(&mut parts.catalog);
            parts.dungeon.zoom_out(&mut settings);
        }
        "UbDungeonTurnLeft" => parts.dungeon.rotate(-ROTATE_STEP_DEGREES),
        "UbDungeonTurnRight" => parts.dungeon.rotate(ROTATE_STEP_DEGREES),
        "UbDungeonPanLeft" => parts.dungeon.pan(dungeon_step(), 0.0),
        "UbDungeonPanRight" => parts.dungeon.pan(-dungeon_step(), 0.0),
        "UbDungeonPanUp" => parts.dungeon.pan(0.0, dungeon_step()),
        "UbDungeonPanDown" => parts.dungeon.pan(0.0, -dungeon_step()),
        #[allow(clippy::cast_possible_truncation)]
        "SetUbDungeonHeight" => parts.dungeon.set_height_percent(arg.float().round() as i32),
        _ => return false,
    }
    true
}

/// The dungeon map's pan step, pixels.
fn dungeon_step() -> f64 {
    f64::from(mosstank_ub::dungeon::controller::PAN_STEP_PIXELS)
}

impl TankCx<'_> {
    /// A UtilityBelt line (`WriteUb`).
    pub(crate) fn write_ub(&mut self, text: &str) {
        mosstank_ub::chat::post(
            self.client.meta(),
            &CatalogDisplays(&*self.ub.reader),
            &mosstank_ub::chat::line(text),
        );
    }

    /// A UtilityBelt error (`WriteUbError`).
    pub(crate) fn write_ub_error(&mut self, text: &str) {
        mosstank_ub::chat::post(
            self.client.meta(),
            &CatalogDisplays(&*self.ub.reader),
            &mosstank_ub::chat::error(text),
        );
    }

    /// The clients window's names follow the readout's rows
    /// (`TickDisplayTools`, that part).
    pub(crate) fn follow_client_rows(&mut self) {
        let names: Vec<String> = self
            .ub
            .hud
            .rows()
            .iter()
            .map(|row| row.name.clone())
            .collect();
        let state = &mut self.core.panel.ub;
        if names != state.client_names {
            state.selected_client = clamp_row(state.selected_client, names.len());
            state.client_names = names;
        }
    }
}

/// The UB page's switch a caption or colour binding stem names: the stem is
/// the handler's without `Toggle`, and the test-mode button's handler adds
/// `Mode` (`UbAutoVendorTestButtonText` and `ToggleUbAutoVendorTestMode`).
fn switch_for(stem: &str) -> Option<&'static main_page::UbSwitch> {
    main_page::find(&format!("Toggle{stem}"))
        .or_else(|| main_page::find(&format!("Toggle{stem}Mode")))
}

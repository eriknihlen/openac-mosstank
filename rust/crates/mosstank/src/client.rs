//! The client as the macro shell reaches it during one call
//! ([`TankClient`]): every view the macro's parts take, over one borrow of
//! the SDK host ([`crate::automation::HostAutomation`] implements them all).

use mosstank_loot::host::LootHost;
use mosstank_meta::host::MetaHost;
use mosstank_stores::storage::StoreHost;
use mosstank_ub::host::UbHost;
use mosstank_vendor::fellow_command::FellowCommandAutomation;
use mosstank_vendor::{FellowshipAutomation, VendorAutomation};

/// The client for one call: the rules' view ([`mosstank_automation::Automation`],
/// through the looter's and the route's), the looter's, the route's and the
/// fellowship's, the meta's and the UB tab's, and the profile stores'.
pub trait TankClient:
    LootHost
    + FellowshipAutomation
    + FellowCommandAutomation
    + VendorAutomation
    + UbHost
    + StoreHost
    + SessionClient
{
    /// The client as the rules see it.
    fn rules(&mut self) -> &mut dyn mosstank_automation::Automation;
    /// The client as the looter sees it.
    fn looter(&mut self) -> &mut dyn LootHost;
    /// The client as the route sees it.
    fn route(&mut self) -> &mut dyn mosstank_nav::NavigationAutomation;
    /// The client as the fellowship manager sees it.
    fn fellows(&mut self) -> &mut dyn FellowshipAutomation;
    /// The client as `/ub fellow` sees it.
    fn fellow_commands(&mut self) -> &mut dyn FellowCommandAutomation;
    /// The client as the vendor run sees it.
    fn vendor(&mut self) -> &mut dyn VendorAutomation;
    /// The client as the meta sees it.
    fn meta(&mut self) -> &mut dyn MetaHost;
    /// The client as the UB tab sees it.
    fn ub(&mut self) -> &mut dyn UbHost;
    /// The client as the profile stores see it.
    fn stores(&mut self) -> &mut dyn StoreHost;
}

impl<T> TankClient for T
where
    T: LootHost
        + FellowshipAutomation
        + FellowCommandAutomation
        + VendorAutomation
        + UbHost
        + StoreHost
        + SessionClient,
{
    fn rules(&mut self) -> &mut dyn mosstank_automation::Automation {
        self
    }

    fn looter(&mut self) -> &mut dyn LootHost {
        self
    }

    fn route(&mut self) -> &mut dyn mosstank_nav::NavigationAutomation {
        self
    }

    fn fellows(&mut self) -> &mut dyn FellowshipAutomation {
        self
    }

    fn fellow_commands(&mut self) -> &mut dyn FellowCommandAutomation {
        self
    }

    fn vendor(&mut self) -> &mut dyn VendorAutomation {
        self
    }

    fn meta(&mut self) -> &mut dyn MetaHost {
        self
    }

    fn ub(&mut self) -> &mut dyn UbHost {
        self
    }

    fn stores(&mut self) -> &mut dyn StoreHost {
        self
    }
}

/// What the shell reads of the session itself: the client's clocks and the
/// settings the session was started with.
pub trait SessionClient {
    /// The client's clocks and a fresh random seed.
    fn clock(&mut self) -> ac_plugin_abi::host::Clock;
    /// The plugin's session settings (autostart reads them).
    fn session_settings(&mut self) -> Vec<(String, String)>;
    /// How each kind of UB line is shown, from the UB settings (the four
    /// kinds in `UbLineKind` order).
    fn set_ub_displays(&mut self, displays: [mosstank_meta::host::UbDisplay; 4]);
    /// Shows or hides one of the plugin's windows; whether the client took
    /// it (`IUiRegistry.ShowPanel` and `HidePanel`).
    fn show_window(&mut self, window_id: &str, visible: bool) -> bool;
    /// The loot classifiers the client's plugins offer
    /// (`LootClassifiers.Available`).
    fn loot_classifiers(&mut self) -> Vec<ac_plugin_abi::items::ClassifierInfo>;
    /// Sends an HTTP request: its handle; the reply comes back as an event.
    ///
    /// # Errors
    /// The request cannot be sent (the host is not allowed, say).
    fn http_request(&mut self, request: ac_plugin_abi::net::HttpRequest) -> Result<u32, String>;
    /// Forgets a request.
    fn net_close(&mut self, handle: u32);
    /// Publishes a line on the host's status board, for other plugins to
    /// read; whether it was taken.
    fn status_publish(&mut self, key: &str, value: Option<&str>) -> bool;
    /// Registers a canvas, listening for its pointer when it takes one.
    ///
    /// # Errors
    /// The client refused it.
    fn canvas_register(
        &mut self,
        spec: &mosstank_ub::canvas::MapCanvasSpec,
    ) -> Result<openac_plugin::ui::Canvas, String>;
    /// Shows or hides a canvas.
    fn canvas_set_visible(&mut self, canvas: openac_plugin::ui::Canvas, visible: bool);
    /// Removes a canvas.
    fn canvas_remove(&mut self, canvas: openac_plugin::ui::Canvas);
    /// Replaces what a canvas shows.
    ///
    /// # Errors
    /// Clips that do not balance, or the canvas is gone.
    fn canvas_paint(
        &mut self,
        canvas: openac_plugin::ui::Canvas,
        commands: &[crate::canvases::MapPaint],
    ) -> Result<(), String>;
    /// How big a line of text is in the interface font: width and height.
    fn measure_text(&mut self, text: &str) -> (f32, f32);
    /// An object's icon, or no image.
    fn image_object_icon(&mut self, object_id: u32) -> mosstank_ub::canvas::MapImage;
    /// A piece of the client's own art, or no image.
    fn image_client_art(&mut self, surface_id: u32) -> mosstank_ub::canvas::MapImage;
    /// Lets go of one busy reference the client holds
    /// (`Recovery.ClearOneBusyReference`).
    fn clear_one_busy_reference(&mut self) -> ac_plugin_abi::social::RecoveryResult;
    /// Lets go of an image.
    fn image_release(&mut self, image: mosstank_ub::canvas::MapImage);
    /// The bytes of text read from storage through this view so far.
    fn storage_bytes_read(&mut self) -> usize;
    /// The keys this view wrote, deleted or made a folder of in the profile
    /// folder since the last time they were taken.
    fn take_profile_writes(&mut self) -> Vec<String>;
    /// A new, empty layer of lines in the world
    /// (`IPluginWorldLines.CreateLayer`).
    fn create_line_layer(&mut self) -> openac_plugin::ui::LineLayer;
    /// Replaces a layer's lines (`IPluginWorldLineLayer.SetLines`); whether
    /// the client took them.
    fn set_world_lines(
        &mut self,
        layer: openac_plugin::ui::LineLayer,
        lines: &[ac_plugin_abi::ui::WorldLine],
    ) -> bool;
}

impl SessionClient for crate::automation::HostAutomation<'_, '_> {
    fn clock(&mut self) -> ac_plugin_abi::host::Clock {
        self.service().clock()
    }

    fn session_settings(&mut self) -> Vec<(String, String)> {
        self.service().settings()
    }

    fn set_ub_displays(&mut self, displays: [mosstank_meta::host::UbDisplay; 4]) {
        self.ub_displays = Some(displays);
    }

    fn show_window(&mut self, window_id: &str, visible: bool) -> bool {
        self.service().show_panel(window_id, visible)
    }

    fn loot_classifiers(&mut self) -> Vec<ac_plugin_abi::items::ClassifierInfo> {
        self.service().loot_classifiers()
    }

    // The network is the plugin host's own service: a request changes
    // nothing of the game, so it keeps the call's reads.
    fn http_request(&mut self, request: ac_plugin_abi::net::HttpRequest) -> Result<u32, String> {
        self.service().http_request(request)
    }

    fn net_close(&mut self, handle: u32) {
        self.service().net_close(handle);
    }

    fn status_publish(&mut self, key: &str, value: Option<&str>) -> bool {
        self.service().status_publish(key, value)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn canvas_register(
        &mut self,
        spec: &mosstank_ub::canvas::MapCanvasSpec,
    ) -> Result<openac_plugin::ui::Canvas, String> {
        let mut sdk = openac_plugin::ui::CanvasSpec::new(&spec.canvas_id, spec.width, spec.height);
        sdk.offset = (spec.offset.0 as f32, spec.offset.1 as f32);
        sdk.start_visible = spec.start_visible;
        sdk.accepts_pointer_input = spec.accepts_pointer_input;
        let canvas = self.service().register_canvas(&sdk)?;
        if spec.accepts_pointer_input {
            canvas.listen_pointer(self.service(), true);
        }
        Ok(canvas)
    }

    fn canvas_set_visible(&mut self, canvas: openac_plugin::ui::Canvas, visible: bool) {
        canvas.set_visible(self.service(), visible);
    }

    fn canvas_remove(&mut self, canvas: openac_plugin::ui::Canvas) {
        canvas.remove(self.service());
    }

    fn canvas_paint(
        &mut self,
        canvas: openac_plugin::ui::Canvas,
        commands: &[crate::canvases::MapPaint],
    ) -> Result<(), String> {
        canvas.paint(self.service(), |painter| {
            crate::canvases::replay(commands, painter)
        })
    }

    fn measure_text(&mut self, text: &str) -> (f32, f32) {
        self.service().measure_text(text)
    }

    fn image_object_icon(&mut self, object_id: u32) -> mosstank_ub::canvas::MapImage {
        map_image(self.service().image_object_icon(object_id))
    }

    fn image_client_art(&mut self, surface_id: u32) -> mosstank_ub::canvas::MapImage {
        map_image(self.service().image_client_art(surface_id))
    }

    fn clear_one_busy_reference(&mut self) -> ac_plugin_abi::social::RecoveryResult {
        self.host.clear_one_busy_reference()
    }

    fn image_release(&mut self, image: mosstank_ub::canvas::MapImage) {
        self.service().image_release(openac_plugin::ui::Image {
            handle: image.handle,
            width: image.width,
            height: image.height,
        });
    }

    fn storage_bytes_read(&mut self) -> usize {
        self.storage_bytes_read
    }

    fn take_profile_writes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.profiles_written)
    }

    // The lines are the plugin host's own drawing: they change nothing of
    // the game, so the call keeps its reads.
    fn create_line_layer(&mut self) -> openac_plugin::ui::LineLayer {
        self.service().create_line_layer()
    }

    fn set_world_lines(
        &mut self,
        layer: openac_plugin::ui::LineLayer,
        lines: &[ac_plugin_abi::ui::WorldLine],
    ) -> bool {
        layer.set_lines(self.service(), lines)
    }
}

fn map_image(image: openac_plugin::ui::Image) -> mosstank_ub::canvas::MapImage {
    mosstank_ub::canvas::MapImage {
        handle: image.handle,
        width: image.width,
        height: image.height,
    }
}

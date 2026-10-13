//! The display tools' frame (`MossTankPanel.UbDisplay` and `UbDungeon`):
//! the network labels, the countryside map, the other clients' readout and
//! the dungeon map, each ticked and, when it asked, repainted.

use std::cell::RefCell;

use mosstank_ub::catalog::UbCatalog;
use mosstank_ub::dungeon::controller::CatalogMapSettings;
use mosstank_ub::hud::CatalogHudSettings;
use mosstank_ub::landscape::controller::{CatalogLandscapeSettings, self_position};

use super::TankCx;
use crate::canvases::{PaintUi, RecordingPainter};
use crate::client::TankClient;

impl TankCx<'_> {
    /// `TickDisplayTools`: the labels pushed when they changed, the map and
    /// the readout ticked, and the clients window's rows following the
    /// readout's.
    pub(crate) fn tick_display_tools(&mut self, elapsed_seconds: f64) {
        let tags = self.ub.catalog.value("Networking.Tags").items;
        self.ub.tags.publish(self.client.ub(), tags, false);
        let player = self_position(self.client.meta());
        {
            let parts = &mut *self.ub;
            let mut ui = parts.canvases.recorder();
            parts.landscape.on_tick(
                &CatalogLandscapeSettings(&parts.catalog),
                &mut ui,
                player,
                elapsed_seconds,
            );
            parts.hud.on_tick(
                self.client.ub(),
                &CatalogHudSettings(&parts.catalog),
                &mut ui,
                elapsed_seconds,
            );
        }
        self.follow_client_rows();
    }

    /// `TickDungeonMap`.
    pub(crate) fn tick_dungeon_map(&mut self, elapsed_seconds: f64) {
        let parts = &mut *self.ub;
        let mut ui = parts.canvases.recorder();
        parts.dungeon.on_tick(
            self.client.ub(),
            &CatalogMapSettings(&mut parts.catalog),
            &mut ui,
            elapsed_seconds,
        );
    }

    /// Carries out the tools' canvas requests and repaints the canvases
    /// that asked (the host's paint on the frame after an invalidate).
    pub(crate) fn paint_display_tools(&mut self) {
        let parts = &mut *self.ub;
        parts.canvases.flush(self.client);
        let player = {
            let snapshot = self.client.ub().navigation();
            snapshot.is_available.then_some(snapshot)
        };
        let client = RefCell::new(&mut *self.client);
        if let Some(canvas) = parts.canvases.take_paint_due(parts.landscape.canvas()) {
            let mut painter = RecordingPainter::new(&client, canvas);
            parts.landscape.paint(
                &mut painter,
                &mut PaintUi::new(&client, &mut parts.canvases),
            );
            submit(&client, canvas, &painter);
        }
        if let Some(canvas) = parts.canvases.take_paint_due(parts.hud.canvas()) {
            let mut painter = RecordingPainter::new(&client, canvas);
            parts.hud.paint(&mut painter);
            submit(&client, canvas, &painter);
        }
        if let Some(canvas) = parts.canvases.take_paint_due(parts.dungeon.canvas()) {
            let mut painter = RecordingPainter::new(&client, canvas);
            parts.dungeon.paint(
                &mut painter,
                &mut PaintUi::new(&client, &mut parts.canvases),
                player.as_ref(),
            );
            submit(&client, canvas, &painter);
        }
        // What a paint released or asked for is carried out now too.
        let client = client.into_inner();
        parts.canvases.flush(client);
    }

    /// Pointer input on one of the tools' canvases.
    pub(crate) fn on_canvas_pointer(&mut self, event: &ac_plugin_abi::ui::PointerEvent) {
        let parts = &mut *self.ub;
        let Some(handle) = parts.canvases.handle_of(event.canvas) else {
            return;
        };
        if parts.landscape.canvas() == Some(handle) {
            parts.landscape.on_pointer(event);
        } else if parts.hud.canvas() == Some(handle) {
            parts.hud.on_pointer(self.client.ub(), event);
        } else if parts.dungeon.canvas() == Some(handle) {
            parts
                .dungeon
                .on_pointer(&mut CatalogMapSettings(&mut parts.catalog), event);
        }
    }

    /// `ResetDisplayTools` and `ResetDungeonMap`: the session's images went
    /// with it.
    pub(crate) fn reset_display_tools(&mut self) {
        self.ub.landscape.reset();
        self.ub.hud.reset();
        self.ub.dungeon.reset();
    }

    /// `DisposeDisplayTools` and `DisposeDungeonMap`: images and canvases
    /// given back.
    pub(crate) fn dispose_display_tools(&mut self) {
        let parts = &mut *self.ub;
        {
            let mut ui = parts.canvases.recorder();
            parts.landscape.dispose(&mut ui);
            parts.hud.dispose(&mut ui);
            parts.dungeon.dispose(&mut ui);
        }
        parts.canvases.flush(self.client);
    }
}

fn submit(
    client: &RefCell<&mut (dyn TankClient + '_)>,
    canvas: openac_plugin::ui::Canvas,
    painter: &RecordingPainter<'_, '_, '_>,
) {
    let commands = painter.commands.clone();
    let mut client = client.borrow_mut();
    if let Err(error) = client.canvas_paint(canvas, &commands) {
        client
            .rules()
            .log_warn(&format!("MossTank canvas paint was refused: {error}"));
    }
}

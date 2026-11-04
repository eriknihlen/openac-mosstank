//! The display tools' frame (`MossTankPanel.UbDisplay` and `UbDungeon`):
//! the network labels, the countryside map, the other clients' readout,
//! the Comps HUD and the dungeon map, each ticked and, when it asked,
//! repainted; and `/vt compshud`.

use std::cell::RefCell;

use mosstank_dotnet::text::casing;
use mosstank_ub::catalog::UbCatalog;
use mosstank_ub::commands::split_head;
use mosstank_ub::comps_hud::{AddRefusal, HudLayout};
use mosstank_ub::dungeon::controller::CatalogMapSettings;
use mosstank_ub::help::COMPONENT_HUD_SYNTAX;
use mosstank_ub::hud::CatalogHudSettings;
use mosstank_ub::landscape::controller::{CatalogLandscapeSettings, self_position};

use super::{Core, TankCx};
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
                &CatalogHudSettings(&mut parts.catalog),
                &mut ui,
                elapsed_seconds,
            );
            parts
                .comps
                .on_tick(self.client.ub(), &mut ui, elapsed_seconds);
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
        if let Some(canvas) = parts.canvases.take_paint_due(parts.comps.canvas()) {
            let mut painter = RecordingPainter::new(&client, canvas);
            parts.comps.paint(
                &mut painter,
                &mut PaintUi::new(&client, &mut parts.canvases),
            );
            submit(&client, canvas, &painter);
        }
        if let Some(canvas) = parts.canvases.take_paint_due(parts.comps.tip_canvas()) {
            let mut painter = RecordingPainter::new(&client, canvas);
            parts.comps.paint_tip(
                &mut painter,
                &mut PaintUi::new(&client, &mut parts.canvases),
            );
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
            parts.hud.on_pointer(
                self.client.ub(),
                &mut CatalogHudSettings(&mut parts.catalog),
                &mut parts.canvases.recorder(),
                event,
            );
        } else if parts.comps.canvas() == Some(handle) {
            parts
                .comps
                .on_pointer(self.client.ub(), &mut parts.canvases.recorder(), event);
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
        let parts = &mut *self.ub;
        parts.comps.reset(&mut parts.canvases.recorder());
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
            parts.comps.dispose(&mut ui);
            parts.dungeon.dispose(&mut ui);
        }
        parts.canvases.flush(self.client);
    }

    /// `/vt compshud [on | off | toggle | line | column | reset | add |
    /// remove <name>]` (`HandleComponentHudCommand`): the Comps HUD from the
    /// keyboard. Bare, it says whether the HUD is on and what it counts.
    pub(crate) fn component_hud_command(&mut self, arguments: &str) {
        let (typed, rest) = split_head(arguments);
        let rest = rest.trim();
        let line = match casing::to_lower_invariant(&typed).as_str() {
            "" => self.component_hud_summary(),
            "on" => {
                self.ub.comps.set_enabled(self.client.ub(), true);
                self.component_hud_summary()
            }
            "off" => {
                self.ub.comps.set_enabled(self.client.ub(), false);
                "Comps HUD: off.".to_owned()
            }
            "toggle" => {
                let enabled = !self.ub.comps.enabled();
                self.ub.comps.set_enabled(self.client.ub(), enabled);
                if enabled {
                    self.component_hud_summary()
                } else {
                    "Comps HUD: off.".to_owned()
                }
            }
            "line" | "column" | "reset" if !self.ub.comps.is_bound() => {
                "Comps HUD: not in world.".to_owned()
            }
            "reset" => {
                self.ub.comps.reset_place(self.client.ub());
                "Comps HUD: back at its first place.".to_owned()
            }
            "line" => {
                self.ub.comps.set_layout(self.client.ub(), HudLayout::Line);
                "Comps HUD: laid out as a line.".to_owned()
            }
            "column" => {
                self.ub
                    .comps
                    .set_layout(self.client.ub(), HudLayout::Column);
                "Comps HUD: laid out as a column.".to_owned()
            }
            "add" => match self.ub.comps.add_selection(self.client.ub()) {
                Ok(added) => format!("Comps HUD: added {added}."),
                Err(refusal) => refusal_line(&refusal),
            },
            "remove" if !rest.is_empty() => match self.ub.comps.remove(self.client.ub(), rest) {
                Some(removed) => format!("Comps HUD: removed {removed}."),
                None => format!("Comps HUD: {rest} is not listed."),
            },
            _ => format!("Syntax: {COMPONENT_HUD_SYNTAX}"),
        };
        Core::write_vtank(self.client, &line);
    }

    /// Whether the Comps HUD is on, and each item it counts with its count.
    fn component_hud_summary(&self) -> String {
        let comps = &self.ub.comps;
        let state = if comps.enabled() { "on" } else { "off" };
        let entries = comps.entries();
        if entries.is_empty() {
            return format!(
                "Comps HUD: {state}, nothing listed (add items in Edit items: the HUD's pencil or the Consumables page; or select an item and /vt compshud add)."
            );
        }
        let counts = comps.counts();
        let listed: Vec<String> = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| match counts.get(index) {
                Some(count) => format!("{} {count}", entry.name),
                None => entry.name.clone(),
            })
            .collect();
        format!("Comps HUD: {state}; {}.", listed.join(", "))
    }
}

/// What the Comps HUD says when an item was not added.
pub(crate) fn refusal_line(refusal: &AddRefusal) -> String {
    match refusal {
        AddRefusal::NotInWorld => "Comps HUD: not in world.".to_owned(),
        AddRefusal::NothingSelected => "Comps HUD: select an item first.".to_owned(),
        AddRefusal::NoName => "Comps HUD: that object has no name to count.".to_owned(),
        AddRefusal::Listed(name) => format!("Comps HUD: {name} is listed already."),
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

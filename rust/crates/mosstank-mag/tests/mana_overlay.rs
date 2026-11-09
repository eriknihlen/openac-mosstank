//! The mana overlay: the Mana page's rows drawn straight on the game, one
//! row per worn item, its canvas as tall as its rows; dragged anywhere,
//! closed with the × shown while the pointer is over it, painted again
//! only when what it shows changed.

use ac_plugin_abi::ui::{Color, PointerEvent};
use mosstank_mag::mana::{ICON_ACTIVE, ICON_NOT_ACTIVE, ManaRow, ManaState};
use mosstank_mag::mana_overlay::{
    CANVAS_ID, CLOSE_BOX, ManaOverlay, OverlayAction, ROW_HEIGHT, WIDTH, height_for,
};
use mosstank_ub::canvas::{MapCanvasSpec, MapImage, MapPainter, MapUi};

#[derive(Debug, Clone)]
struct FakeCanvas {
    spec: MapCanvasSpec,
    visible: bool,
    offset: (f64, f64),
    removed: bool,
}

#[derive(Debug, Default)]
struct FakeUi {
    canvases: Vec<FakeCanvas>,
    invalidated: usize,
    object_icons: Vec<u32>,
    art: Vec<u32>,
    released: Vec<MapImage>,
}

impl FakeUi {
    fn canvas(&mut self, canvas: u32) -> &mut FakeCanvas {
        &mut self.canvases[usize::try_from(canvas).unwrap() - 1]
    }

    /// The canvas in use (the last registered and not removed).
    fn live(&self) -> &FakeCanvas {
        self.canvases
            .iter()
            .rev()
            .find(|canvas| !canvas.removed)
            .expect("a canvas")
    }
}

impl MapUi for FakeUi {
    fn register_canvas(&mut self, spec: &MapCanvasSpec) -> Option<u32> {
        self.canvases.push(FakeCanvas {
            spec: spec.clone(),
            visible: spec.start_visible,
            offset: spec.offset,
            removed: false,
        });
        Some(u32::try_from(self.canvases.len()).unwrap())
    }

    fn set_canvas_visible(&mut self, canvas: u32, visible: bool) {
        self.canvas(canvas).visible = visible;
    }

    fn set_canvas_offset(&mut self, canvas: u32, offset: (f64, f64)) {
        self.canvas(canvas).offset = offset;
    }

    fn invalidate_canvas(&mut self, _canvas: u32) {
        self.invalidated += 1;
    }

    fn remove_canvas(&mut self, canvas: u32) {
        self.canvas(canvas).removed = true;
    }

    fn object_icon(&mut self, object_id: u32) -> MapImage {
        self.object_icons.push(object_id);
        MapImage {
            handle: object_id,
            width: 32,
            height: 32,
        }
    }

    fn client_art(&mut self, surface_id: u32) -> MapImage {
        self.art.push(surface_id);
        MapImage {
            handle: surface_id,
            width: 32,
            height: 32,
        }
    }

    fn release_image(&mut self, image: MapImage) {
        self.released.push(image);
    }
}

#[derive(Debug, Default)]
struct Painter {
    height: i32,
    texts: Vec<(String, (f64, f64), bool)>,
    images: Vec<(u32, (f64, f64, f64, f64))>,
    lines: usize,
    fills: usize,
}

impl MapPainter for Painter {
    fn width(&self) -> i32 {
        WIDTH
    }

    fn height(&self) -> i32 {
        self.height
    }

    fn clear(&mut self, _color: &Color) {}

    fn fill_rect(&mut self, _rect: (f64, f64, f64, f64), _color: &Color) {
        self.fills += 1;
    }

    fn stroke_rect(&mut self, _rect: (f64, f64, f64, f64), _color: &Color, _thickness: f32) {}

    fn draw_line(&mut self, _from: (f64, f64), _to: (f64, f64), _color: &Color, _thickness: f32) {
        self.lines += 1;
    }

    fn draw_text(&mut self, text: &str, at: (f64, f64), _color: &Color, outline: bool) {
        self.texts.push((text.to_owned(), at, outline));
    }

    #[allow(clippy::cast_precision_loss)]
    fn measure_text(&mut self, text: &str) -> (f64, f64) {
        (text.len() as f64 * 6.0, 14.0)
    }

    fn draw_image(&mut self, image: MapImage, rect: (f64, f64, f64, f64), _tint: &Color) {
        self.images.push((image.handle, rect));
    }

    fn push_clip(&mut self, _rect: (f64, f64, f64, f64)) {}

    fn pop_clip(&mut self) {}
}

fn row(object_id: u32, name: &str, state: ManaState, mana: &str, time: &str) -> ManaRow {
    ManaRow {
        object_id,
        name: name.to_owned(),
        state,
        mana_text: mana.to_owned(),
        time_text: time.to_owned(),
        sort_key: 0.0,
    }
}

fn two_rows() -> Vec<ManaRow> {
    vec![
        row(11, "Bracelet", ManaState::NotActive, "0 / 200", "0h00m"),
        row(10, "Test Ring", ManaState::Active, "100 / 200", "1h06m"),
    ]
}

const PLACE: (i32, i32) = (300, 120);

fn pointer(kind: u8, at: (f64, f64)) -> PointerEvent {
    #[allow(clippy::cast_possible_truncation)]
    PointerEvent {
        canvas: 1,
        kind,
        x: at.0 as f32,
        y: at.1 as f32,
        button: if kind == PointerEvent::HOVER || kind == PointerEvent::LEAVE {
            0
        } else {
            1
        },
        modifiers: 0,
        wheel_delta: 0,
    }
}

fn paint(overlay: &mut ManaOverlay, ui: &mut FakeUi) -> Painter {
    let mut painter = Painter {
        height: ui.live().spec.height,
        ..Painter::default()
    };
    overlay.paint(&mut painter, ui);
    painter
}

#[test]
fn it_is_shown_while_wanted_at_its_place_and_as_tall_as_its_rows() {
    let mut ui = FakeUi::default();
    let mut overlay = ManaOverlay::default();
    overlay.on_frame(&mut ui, false, PLACE, &two_rows());
    assert!(ui.canvases.is_empty(), "nothing registered while off");

    overlay.on_frame(&mut ui, true, PLACE, &two_rows());
    let canvas = ui.live().clone();
    assert_eq!(canvas.spec.canvas_id, CANVAS_ID);
    assert!(canvas.visible);
    assert!(canvas.spec.accepts_pointer_input);
    assert_eq!(canvas.offset, (300.0, 120.0));
    assert_eq!(
        (canvas.spec.width, canvas.spec.height),
        (WIDTH, height_for(2))
    );
    assert_eq!(height_for(2), 2 * ROW_HEIGHT + 2);

    // A third row: a taller canvas in its place, at the same spot.
    let mut three = two_rows();
    three.push(row(12, "Amulet", ManaState::Active, "50 / 50", "0h33m"));
    overlay.on_frame(&mut ui, true, PLACE, &three);
    assert_eq!(ui.live().spec.height, height_for(3));
    assert_eq!(ui.live().offset, (300.0, 120.0));
    assert_eq!(ui.canvases.iter().filter(|c| !c.removed).count(), 1);
    // None: still one row's height, saying so.
    overlay.on_frame(&mut ui, true, PLACE, &[]);
    assert_eq!(ui.live().spec.height, height_for(1));

    // Off: hidden.
    overlay.on_frame(&mut ui, false, PLACE, &three);
    assert!(!ui.live().visible);
}

#[test]
fn each_row_is_its_icons_and_outlined_texts_with_no_background() {
    let mut ui = FakeUi::default();
    let mut overlay = ManaOverlay::default();
    overlay.on_frame(&mut ui, true, PLACE, &two_rows());
    let painter = paint(&mut overlay, &mut ui);
    let texts: Vec<&str> = painter.texts.iter().map(|(t, ..)| t.as_str()).collect();
    assert_eq!(
        texts,
        [
            "Bracelet",
            "0 / 200",
            "0h00m",
            "Test Ring",
            "100 / 200",
            "1h06m"
        ]
    );
    assert!(painter.texts.iter().all(|(.., outline)| *outline));
    // The second row sits one row lower.
    assert!((painter.texts[3].1.1 - painter.texts[0].1.1 - f64::from(ROW_HEIGHT)).abs() < 1e-9);
    let images: Vec<u32> = painter.images.iter().map(|(handle, _)| *handle).collect();
    assert_eq!(images, [11, ICON_NOT_ACTIVE, 10, ICON_ACTIVE]);
    assert_eq!(painter.fills, 0, "no background");
    assert_eq!(painter.lines, 0, "no × until the pointer is over it");
}

#[test]
fn it_is_painted_again_only_when_what_it_shows_changed() {
    let mut ui = FakeUi::default();
    let mut overlay = ManaOverlay::default();
    overlay.on_frame(&mut ui, true, PLACE, &two_rows());
    assert_eq!(ui.invalidated, 1);
    paint(&mut overlay, &mut ui);
    overlay.on_frame(&mut ui, true, PLACE, &two_rows());
    overlay.on_frame(&mut ui, true, PLACE, &two_rows());
    assert_eq!(ui.invalidated, 1, "the same rows: no paint");
    let mut later = two_rows();
    later[1].time_text = "1h05m".to_owned();
    overlay.on_frame(&mut ui, true, PLACE, &later);
    assert_eq!(ui.invalidated, 2);
    // The icons were asked for once.
    paint(&mut overlay, &mut ui);
    assert_eq!(ui.object_icons, [11, 10]);
}

#[test]
fn the_close_box_shows_while_the_pointer_is_over_it_and_closes_it() {
    let mut ui = FakeUi::default();
    let mut overlay = ManaOverlay::default();
    overlay.on_frame(&mut ui, true, PLACE, &two_rows());
    paint(&mut overlay, &mut ui);
    assert_eq!(
        overlay.on_pointer(&mut ui, &pointer(PointerEvent::HOVER, (40.0, 5.0))),
        None
    );
    overlay.on_frame(&mut ui, true, PLACE, &two_rows());
    assert_eq!(ui.invalidated, 2, "hovered: painted with the ×");
    assert!(paint(&mut overlay, &mut ui).lines > 0);

    let (x, y, w, h) = CLOSE_BOX;
    let middle = (x + w / 2.0, y + h / 2.0);
    assert_eq!(
        overlay.on_pointer(&mut ui, &pointer(PointerEvent::DOWN, middle)),
        None
    );
    assert_eq!(
        overlay.on_pointer(&mut ui, &pointer(PointerEvent::UP, middle)),
        Some(OverlayAction::Closed)
    );

    // Leaving: the × goes.
    overlay.on_pointer(&mut ui, &pointer(PointerEvent::LEAVE, (0.0, 0.0)));
    overlay.on_frame(&mut ui, true, PLACE, &two_rows());
    assert_eq!(paint(&mut overlay, &mut ui).lines, 0);
}

#[test]
fn a_drag_moves_it_and_its_place_is_kept_once_when_let_go() {
    let mut ui = FakeUi::default();
    let mut overlay = ManaOverlay::default();
    overlay.on_frame(&mut ui, true, PLACE, &two_rows());
    assert_eq!(
        overlay.on_pointer(&mut ui, &pointer(PointerEvent::DOWN, (40.0, 5.0))),
        None
    );
    // The host measures each event from where the canvas was last put.
    assert_eq!(
        overlay.on_pointer(&mut ui, &pointer(PointerEvent::MOVE, (60.0, 15.0))),
        None
    );
    assert_eq!(ui.live().offset, (320.0, 130.0));
    // A frame mid-drag keeps the dragged place, not the saved one.
    overlay.on_frame(&mut ui, true, PLACE, &two_rows());
    assert_eq!(ui.live().offset, (320.0, 130.0));
    assert_eq!(
        overlay.on_pointer(&mut ui, &pointer(PointerEvent::UP, (45.0, 5.0))),
        Some(OverlayAction::Moved(325, 130))
    );
    // A press let go where it went down is no move.
    overlay.on_pointer(&mut ui, &pointer(PointerEvent::DOWN, (40.0, 5.0)));
    assert_eq!(
        overlay.on_pointer(&mut ui, &pointer(PointerEvent::UP, (40.0, 5.0))),
        None
    );
    // Never off the screen's top or left.
    overlay.on_pointer(&mut ui, &pointer(PointerEvent::DOWN, (40.0, 5.0)));
    overlay.on_pointer(&mut ui, &pointer(PointerEvent::MOVE, (-900.0, -900.0)));
    assert_eq!(
        overlay.on_pointer(&mut ui, &pointer(PointerEvent::UP, (40.0, 5.0))),
        Some(OverlayAction::Moved(0, 0))
    );
}

#[test]
fn the_session_end_hides_it_and_lets_its_icons_go() {
    let mut ui = FakeUi::default();
    let mut overlay = ManaOverlay::default();
    overlay.on_frame(&mut ui, true, PLACE, &two_rows());
    paint(&mut overlay, &mut ui);
    overlay.reset(&mut ui);
    assert!(!ui.live().visible);
    assert_eq!(ui.released.len(), 4, "two item icons, two state icons");
    // The next session: shown again, painted afresh.
    overlay.on_frame(&mut ui, true, PLACE, &two_rows());
    assert!(ui.live().visible);
}

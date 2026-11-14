//! Tests of `scaffold_with` against measured rectangles.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use rustle_foundation::{LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier, canvas};
use rustle_geometry::{Offset, Rect, Size};
use rustle_text::{LineLayout, StyledSpan, TextLayout, TextMeasurer, TextStyle};
use rustle_ui::{composition, display_list};
use rustle_unit::Constraints;

use super::{ScaffoldPadding, ScaffoldSlots, scaffold_with};

/// A measurer for a tree without text.
struct NoText;

impl TextMeasurer for NoText {
    /// Text does not occur in these tests.
    fn measure(&mut self, _: &str, _: &TextStyle, _: f32) -> Size {
        Size::default()
    }

    /// Text does not occur in these tests.
    fn measure_spans(&mut self, _: &[StyledSpan], _: &TextStyle, _: f32) -> Size {
        Size::default()
    }

    /// Text does not occur in these tests.
    fn layout(&mut self, spans: &[StyledSpan], style: &TextStyle, _: f32) -> TextLayout {
        let line = LineLayout {
            start: 0,
            end: 0,
            top: 0.0,
            height: 0.0,
            baseline: 0.0,
            width: 0.0,
            rtl: false,
            hard_break: true,
            clusters: Vec::new(),
        };
        TextLayout::new(String::new(), spans.to_vec(), style.font_size, vec![line])
    }
}

/// The rectangles probes recorded, by name.
type Rects = Rc<RefCell<HashMap<&'static str, Rect>>>;

/// The rectangle at `(x, y)` of `width` by `height`.
fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect::new(Offset::new(x, y), Size::new(width, height))
}

/// Emits an empty area of `modifier` that records its rectangle under `name`.
fn probe(rects: &Rects, name: &'static str, modifier: Modifier) {
    let rects = Rc::clone(rects);
    canvas(
        modifier.on_globally_positioned(move |rect| {
            rects.borrow_mut().insert(name, rect);
        }),
        |_| {},
    );
}

/// Lays out a scaffold with every slot in a 300 by 400 window and returns the
/// rectangles and the padding the content was told.
fn scaffolded(direction: LayoutDirection) -> (HashMap<&'static str, Rect>, ScaffoldPadding) {
    let rects = Rects::default();
    let padding = Rc::new(Cell::new(ScaffoldPadding::default()));
    let (kept, told) = (Rc::clone(&rects), Rc::clone(&padding));
    let mut composition = composition(move || {
        let (kept, told) = (Rc::clone(&kept), Rc::clone(&told));
        LOCAL_LAYOUT_DIRECTION.provide(direction, || {
            let (top, bottom, fab, snack, body) = (
                Rc::clone(&kept),
                Rc::clone(&kept),
                Rc::clone(&kept),
                Rc::clone(&kept),
                Rc::clone(&kept),
            );
            let slots = ScaffoldSlots::new()
                .top_bar(move || probe(&top, "top", Modifier::new().fill_max_width().height(40.0)))
                .bottom_bar(move || {
                    probe(
                        &bottom,
                        "bottom",
                        Modifier::new().fill_max_width().height(50.0),
                    )
                })
                .floating_action_button(move || {
                    probe(&fab, "fab", Modifier::new().size(56.0, 56.0))
                })
                .snackbar_host(move || probe(&snack, "snack", Modifier::new().size(100.0, 30.0)));
            scaffold_with(Modifier::new(), slots, move |padding| {
                told.set(padding);
                probe(&body, "body", Modifier::new().fill_max_size());
            });
        });
    });
    composition.compose();
    let window = Constraints::loose(Size::new(300.0, 400.0));
    display_list(composition.output().unwrap(), window, &mut NoText);
    let found = rects.borrow().clone();
    (found, padding.get())
}

/// Bars sit at the edges, the content fills everything and learns the bar heights.
#[test]
fn bars_content_and_padding() {
    let (found, padding) = scaffolded(LayoutDirection::Ltr);
    assert_eq!(
        padding,
        ScaffoldPadding {
            top: 40.0,
            bottom: 50.0
        }
    );
    assert_eq!(found["body"], rect(0.0, 0.0, 300.0, 400.0));
    assert_eq!(found["top"], rect(0.0, 0.0, 300.0, 40.0));
    assert_eq!(found["bottom"], rect(0.0, 350.0, 300.0, 50.0));
}

/// The button floats at the bottom end and the snackbar at the bottom center, above the bottom bar.
#[test]
fn floating_button_and_snackbar() {
    let (found, _) = scaffolded(LayoutDirection::Ltr);
    assert_eq!(
        found["fab"],
        rect(300.0 - 56.0 - 16.0, 350.0 - 56.0 - 16.0, 56.0, 56.0)
    );
    assert_eq!(
        found["snack"],
        rect(100.0, 350.0 - 30.0 - 16.0, 100.0, 30.0)
    );
}

/// In a right-to-left layout the button floats at the left edge.
#[test]
fn floating_button_mirrors_in_rtl() {
    let (found, _) = scaffolded(LayoutDirection::Rtl);
    assert_eq!(found["fab"].origin.x, 16.0);
    assert_eq!(found["snack"].origin.x, 100.0);
}

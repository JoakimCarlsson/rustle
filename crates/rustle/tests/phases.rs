//! Changes to state read only while drawing or laying out an [`Embedded`] interface draw or lay
//! it out again without composing it, as Compose's draw and layout phases do.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rustle::prelude::*;
use rustle::{Constraints, DrawCommand, Embedded, Offset, Placement, Size};

/// What a test root hands back: how often it composed and the state it made.
struct Made {
    /// How many times the root ran.
    composed: Rc<Cell<usize>>,
    /// The width the root's canvas reads while drawing.
    drawn: Rc<RefCell<Option<State<f32>>>>,
    /// The width the root's layout reads while measuring.
    laid: Rc<RefCell<Option<State<f32>>>>,
}

/// A headless interface whose root draws a bar as wide as one state says, inside a box as
/// wide as another says, with what it made.
fn rig() -> (Embedded, Made) {
    let made = Made {
        composed: Rc::default(),
        drawn: Rc::default(),
        laid: Rc::default(),
    };
    let (composed, drawn, laid) = (
        Rc::clone(&made.composed),
        Rc::clone(&made.drawn),
        Rc::clone(&made.laid),
    );
    let mut embedded = Embedded::without_renderer(
        move || {
            composed.set(composed.get() + 1);
            let width = remember(|| 10.0_f32);
            let room = remember(|| 50.0_f32);
            *drawn.borrow_mut() = Some(width.clone());
            *laid.borrow_mut() = Some(room.clone());
            let measured = room.clone();
            canvas(
                Modifier::new().layout(move |measurable, _| {
                    let side = measured.get();
                    measurable.measure(Constraints::tight(Size::new(side, side)));
                    Placement::new(Size::new(side, side), Offset::new(0.0, 0.0))
                }),
                move |canvas| {
                    canvas.rect(0.0, 0.0, width.get(), 4.0, BAR);
                    canvas.rect(0.0, 0.0, 2.0, canvas.height(), POST);
                },
            );
        },
        || {},
    );
    embedded.resize((200, 200), 1.0);
    embedded.update();
    (embedded, made)
}

/// The color of the bar as wide as the drawing state says.
const BAR: Color = Color::rgb(255, 0, 0);

/// The color of the post as tall as the box the layout state sizes.
const POST: Color = Color::rgb(0, 0, 255);

/// The size of the rectangle drawn in `color`.
fn drawn_in(embedded: &Embedded, color: Color) -> Option<Size> {
    embedded
        .commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::Rect {
                rect, color: drawn, ..
            } if *drawn == color => Some(rect.size),
            _ => None,
        })
}

/// A width read only by a drawing closure draws again when it changes, without composing.
#[test]
fn drawing_state_draws_again_without_composing() {
    let (mut embedded, made) = rig();
    let composed = made.composed.get();
    assert_eq!(drawn_in(&embedded, BAR).map(|size| size.width), Some(10.0));
    made.drawn.borrow().clone().expect("made").set(30.0);
    assert!(embedded.update(), "a drawing changed");
    assert_eq!(drawn_in(&embedded, BAR).map(|size| size.width), Some(30.0));
    assert_eq!(made.composed.get(), composed, "nothing composed");
}

/// A size read only by a layout closure lays out again when it changes, without composing.
#[test]
fn layout_state_lays_out_again_without_composing() {
    let (mut embedded, made) = rig();
    let composed = made.composed.get();
    assert_eq!(
        drawn_in(&embedded, POST).map(|size| size.height),
        Some(50.0)
    );
    made.laid.borrow().clone().expect("made").set(80.0);
    assert!(embedded.update(), "the layout changed");
    assert_eq!(
        drawn_in(&embedded, POST).map(|size| size.height),
        Some(80.0)
    );
    assert_eq!(made.composed.get(), composed, "nothing composed");
    assert!(!embedded.update(), "nothing is left to do");
}

/// An angle read by [`Modifier::rotate_with`] turns the layer when it changes, without
/// composing.
#[test]
fn a_turning_layer_draws_again_without_composing() {
    let composed = Rc::new(Cell::new(0));
    let made: Rc<RefCell<Option<State<f32>>>> = Rc::default();
    let (runs, slot) = (Rc::clone(&composed), Rc::clone(&made));
    let mut embedded = Embedded::without_renderer(
        move || {
            runs.set(runs.get() + 1);
            let angle = remember(|| 0.0_f32);
            *slot.borrow_mut() = Some(angle.clone());
            canvas(
                Modifier::new()
                    .size(20.0, 20.0)
                    .rotate_with(move || angle.get()),
                |canvas| canvas.rect(0.0, 0.0, 20.0, 20.0, BAR),
            );
        },
        || {},
    );
    embedded.resize((100, 100), 1.0);
    embedded.update();
    let turned = |embedded: &Embedded| {
        embedded
            .commands()
            .iter()
            .find_map(|command| match command {
                DrawCommand::PushLayer { rotation, .. } => Some(*rotation),
                _ => None,
            })
    };
    assert_eq!(turned(&embedded), Some(0.0));
    let before = composed.get();
    made.borrow().clone().expect("made").set(90.0);
    assert!(embedded.update());
    assert_eq!(turned(&embedded), Some(90.0_f32.to_radians()));
    assert_eq!(composed.get(), before, "nothing composed");
}

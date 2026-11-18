//! Hovers, presses and focuses an area of an [`Embedded`] interface drawn by its interaction
//! state, and checks that each change is painted again without composing or measuring.

use std::cell::Cell;
use std::rc::Rc;

use rustle::prelude::*;
use rustle::{Embedded, InteractionColors, Key, KeyEvent, Modifiers, Offset, PointerButton};
use rustle_graphics::DrawCommand;

/// The colors of the area in each state.
const COLORS: InteractionColors = InteractionColors {
    idle: Color::rgba(0, 0, 0, 0),
    hovered: Color::rgba(200, 0, 0, 255),
    pressed: Color::rgba(0, 200, 0, 255),
    focused: Color::rgba(0, 0, 200, 255),
    corner_radius: 4.0,
};

/// How often the root composed and the area was measured.
#[derive(Default)]
struct Counts {
    composed: Cell<u32>,
    measured: Cell<u32>,
}

/// A root with one 100 by 40 area at (10, 10) filled by its interaction state, which takes
/// focus when `focusable`.
fn root(counts: Rc<Counts>, focusable: bool) -> impl FnMut() {
    move || {
        counts.composed.set(counts.composed.get() + 1);
        let focus = remember_focus();
        let measured = Rc::clone(&counts);
        let modifier = Modifier::new()
            .padding(10.0)
            .size(100.0, 40.0)
            .on_size_changed(move |_| measured.measured.set(measured.measured.get() + 1))
            .interaction_background(COLORS);
        let modifier = if focusable {
            modifier.focusable(&focus, |_| false)
        } else {
            modifier
        };
        canvas(modifier, |_| {});
    }
}

/// The color the area is filled with now, if it is filled.
fn fill(embedded: &Embedded) -> Option<Color> {
    embedded
        .commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::Rect { color, .. } => Some(*color),
            _ => None,
        })
}

/// A headless interface of `root`, laid out and settled.
fn rig(counts: &Rc<Counts>, focusable: bool) -> Embedded {
    let mut embedded = Embedded::without_renderer(root(Rc::clone(counts), focusable), || {});
    embedded.resize((300, 300), 1.0);
    embedded.update();
    embedded
}

/// Hovering and pressing repaint the area without composing or measuring, and `update`
/// says a frame is needed only when its state changed.
#[test]
fn hover_and_press_repaint_without_composing() {
    let counts = Rc::new(Counts::default());
    let mut embedded = rig(&counts, false);
    let (composed, measured) = (counts.composed.get(), counts.measured.get());
    assert_eq!(fill(&embedded), None, "idle is transparent");
    assert!(!embedded.update(), "nothing changed");

    assert!(embedded.pointer_moved(Offset::new(30.0, 20.0)));
    assert!(embedded.update(), "a hover change needs a frame");
    assert_eq!(fill(&embedded), Some(COLORS.hovered));
    embedded.pointer_moved(Offset::new(40.0, 25.0));
    assert!(!embedded.update(), "moving inside changes nothing");

    embedded.pointer_button(PointerButton::Primary, true);
    assert!(embedded.update());
    assert_eq!(fill(&embedded), Some(COLORS.pressed));
    embedded.pointer_button(PointerButton::Primary, false);
    embedded.update();
    assert_eq!(fill(&embedded), Some(COLORS.hovered));

    embedded.pointer_left();
    assert!(embedded.update());
    assert_eq!(fill(&embedded), None);

    assert_eq!(
        counts.composed.get(),
        composed,
        "hover and press do not compose"
    );
    assert_eq!(
        counts.measured.get(),
        measured,
        "hover and press do not measure"
    );
}

/// A focusable area is drawn focused while it holds focus, after the pointer has left.
#[test]
fn focus_is_resolved_when_painting() {
    let counts = Rc::new(Counts::default());
    let mut embedded = rig(&counts, true);
    embedded.pointer_moved(Offset::new(30.0, 20.0));
    embedded.pointer_button(PointerButton::Primary, true);
    embedded.pointer_button(PointerButton::Primary, false);
    embedded.pointer_left();
    assert!(embedded.update());
    assert!(embedded.has_focus());
    assert_eq!(
        fill(&embedded),
        Some(COLORS.focused),
        "the press focused the area"
    );
    assert!(embedded.key_event(KeyEvent::press(Key::Escape, Modifiers::default())));
    assert!(embedded.update());
    assert_eq!(fill(&embedded), None);
}

/// A focused state the application keeps itself, read when painting, repaints without
/// composing when it changes, and `update` says a frame is needed.
#[test]
fn focus_from_the_application_repaints_without_composing() {
    let counts = Rc::new(Counts::default());
    let focused = Rc::new(Cell::new(false));
    let root = {
        let counts = Rc::clone(&counts);
        let focused = Rc::clone(&focused);
        move || {
            counts.composed.set(counts.composed.get() + 1);
            let focused = Rc::clone(&focused);
            canvas(
                Modifier::new()
                    .padding(10.0)
                    .size(100.0, 40.0)
                    .interaction_focused_with(move || focused.get())
                    .interaction_background(COLORS),
                |_| {},
            );
        }
    };
    let mut embedded = Embedded::without_renderer(root, || {});
    embedded.resize((300, 300), 1.0);
    embedded.update();
    let composed = counts.composed.get();
    assert_eq!(fill(&embedded), None);
    focused.set(true);
    assert!(embedded.update(), "a focus change needs a frame");
    assert_eq!(fill(&embedded), Some(COLORS.focused));
    assert!(!embedded.update());
    focused.set(false);
    assert!(embedded.update());
    assert_eq!(fill(&embedded), None);
    assert_eq!(
        counts.composed.get(),
        composed,
        "the focus change did not compose"
    );
}

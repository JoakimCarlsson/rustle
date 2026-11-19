//! Drags and drops inside the window: a source starts a drag once the pointer has moved past
//! the touch slop, the targets that accept it hear it start and end, and the one under the
//! pointer hears it enter, move, leave and drop; no click follows a drag.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rustle::prelude::*;
use rustle::{
    DragAndDropEvent, DragAndDropTarget, DragAndDropTransferData, Embedded, Modifiers, Offset,
    PointerButton,
};

/// What the targets heard, as (target, what, payload) in order.
type Heard = Rc<RefCell<Vec<(&'static str, &'static str, u32)>>>;

/// A target that records what it hears under its name.
struct Recorder {
    /// Its name in the record.
    name: &'static str,
    /// The record.
    heard: Heard,
}

impl Recorder {
    /// Records `what` with the payload of `event`.
    fn note(&self, what: &'static str, event: &DragAndDropEvent) {
        let payload = event.payload::<u32>().copied().unwrap_or_default();
        self.heard.borrow_mut().push((self.name, what, payload));
    }
}

impl DragAndDropTarget for Recorder {
    /// Records the drop and takes it.
    fn on_drop(&self, event: &DragAndDropEvent) -> bool {
        self.note("drop", event);
        true
    }

    /// Records the start.
    fn on_started(&self, event: &DragAndDropEvent) {
        self.note("started", event);
    }

    /// Records the pointer coming.
    fn on_entered(&self, event: &DragAndDropEvent) {
        self.note("entered", event);
    }

    /// Records the pointer leaving.
    fn on_exited(&self, event: &DragAndDropEvent) {
        self.note("exited", event);
    }

    /// Records the end.
    fn on_ended(&self, event: &DragAndDropEvent) {
        self.note("ended", event);
    }
}

/// A row of a source carrying 7 (clicks counted in `clicks`), two targets "a" and "b"
/// accepting it and one "c" refusing it, each 50 by 50.
fn root(heard: Heard, clicks: Rc<Cell<u32>>) -> impl FnMut() {
    move || {
        let (heard, clicks) = (Rc::clone(&heard), Rc::clone(&clicks));
        row(|| {
            spacer(
                Modifier::new()
                    .size(50.0, 50.0)
                    .drag_and_drop_source(|_| Some(DragAndDropTransferData::new(7_u32)))
                    .clickable(move || clicks.set(clicks.get() + 1)),
            );
            for (name, accepts) in [("a", true), ("b", true), ("c", false)] {
                let heard = Rc::clone(&heard);
                spacer(Modifier::new().size(50.0, 50.0).drag_and_drop_target(
                    move |event| accepts && event.payload::<u32>().is_some(),
                    Recorder { name, heard },
                ));
            }
        });
    }
}

/// A drag starts past the threshold, visits targets in order and drops on the last one.
#[test]
fn a_drag_visits_targets_and_drops_on_the_one_under_the_pointer() {
    let heard: Heard = Rc::default();
    let clicks = Rc::new(Cell::new(0));
    let mut ui = Embedded::without_renderer(root(Rc::clone(&heard), Rc::clone(&clicks)), || {});
    ui.resize((300, 100), 1.0);
    ui.update();
    ui.pointer_moved(Offset::new(25.0, 25.0));
    ui.pointer_button(PointerButton::Primary, true);
    ui.pointer_moved(Offset::new(29.0, 28.0));
    assert!(heard.borrow().is_empty(), "5 px is not a drag yet");
    ui.pointer_moved(Offset::new(75.0, 25.0));
    ui.pointer_moved(Offset::new(125.0, 25.0));
    ui.pointer_moved(Offset::new(175.0, 25.0));
    ui.set_modifiers(Modifiers::NONE);
    ui.pointer_moved(Offset::new(125.0, 25.0));
    ui.pointer_button(PointerButton::Primary, false);
    ui.update();
    assert_eq!(
        *heard.borrow(),
        [
            ("a", "started", 7),
            ("b", "started", 7),
            ("a", "entered", 7),
            ("a", "exited", 7),
            ("b", "entered", 7),
            ("b", "exited", 7),
            ("b", "entered", 7),
            ("b", "drop", 7),
            ("a", "ended", 7),
            ("b", "ended", 7),
        ]
    );
    assert_eq!(clicks.get(), 0, "no click follows a drag");
    ui.pointer_moved(Offset::new(25.0, 25.0));
    ui.pointer_button(PointerButton::Primary, true);
    ui.pointer_button(PointerButton::Primary, false);
    ui.update();
    assert_eq!(clicks.get(), 1, "a press without a drag still clicks");
}

/// A source takes its slop from the view configuration provided around it: with a slop of
/// 20 pixels, a move of 10 is not a drag yet and one of 30 is.
#[test]
fn a_source_takes_the_touch_slop_provided_around_it() {
    let heard: Heard = Rc::default();
    let clicks = Rc::new(Cell::new(0));
    let mut inner = root(Rc::clone(&heard), Rc::clone(&clicks));
    let configuration = rustle::ViewConfiguration {
        touch_slop: 20.0,
        ..rustle::ViewConfiguration::default()
    };
    let mut ui = Embedded::without_renderer(
        move || rustle::LOCAL_VIEW_CONFIGURATION.provide(configuration, &mut inner),
        || {},
    );
    ui.resize((300, 100), 1.0);
    ui.update();
    ui.pointer_moved(Offset::new(25.0, 25.0));
    ui.pointer_button(PointerButton::Primary, true);
    ui.pointer_moved(Offset::new(35.0, 25.0));
    assert!(heard.borrow().is_empty(), "10 px is not a drag yet");
    ui.pointer_moved(Offset::new(55.0, 25.0));
    assert_eq!(
        heard.borrow()[..2],
        [("a", "started", 7), ("b", "started", 7)]
    );
}

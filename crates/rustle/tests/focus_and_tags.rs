//! Focus targets, focus-change observers and test tags of an [`Embedded`] interface: Tab
//! walks focus targets in paint order, a press does not give a focus target focus, an
//! observer hears focus come and go (also when its area leaves the composition), and a test
//! finds an area by its tag and gives it focus.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rustle::prelude::*;
use rustle::{Embedded, Focus, Key, KeyEvent, Modifiers, Offset, PointerButton, remember_focus};

/// What the observers of the areas heard, as (name, focused) in order.
type Heard = Rc<RefCell<Vec<(&'static str, bool)>>>;

/// The focuses the areas hold, in the order they were composed.
type Focuses = Rc<RefCell<Vec<Focus>>>;

/// An interface 300 by 200 logical pixels of `root`, composed once.
fn interface(root: impl FnMut() + 'static) -> Embedded {
    let mut embedded = Embedded::without_renderer(root, || {});
    embedded.resize((300, 200), 1.0);
    embedded.update();
    embedded
}

/// Presses `key` with no modifiers, then composes.
fn press(embedded: &mut Embedded, key: Key) {
    embedded.key_event(KeyEvent::press(key, Modifiers::NONE));
    embedded.update();
}

/// Clicks the primary button at `at`, then composes.
fn click(embedded: &mut Embedded, at: Offset) {
    embedded.pointer_moved(at);
    embedded.pointer_button(PointerButton::Primary, true);
    embedded.pointer_button(PointerButton::Primary, false);
    embedded.update();
}

/// A box 100 by 40 named `name` for tests, a focus target whose observer records into
/// `heard`, its focus handed out through `out`.
#[track_caller]
fn target(name: &'static str, heard: &Heard, out: &Focuses) {
    let focus = remember_focus();
    out.borrow_mut().push(focus.clone());
    let heard = Rc::clone(heard);
    stack_with(
        Modifier::new()
            .size(100.0, 40.0)
            .test_tag(name)
            .on_focus_changed(move |focused| heard.borrow_mut().push((name, focused)))
            .focus_target(&focus),
        || {},
    );
}

/// Tab walks focus targets in paint order and their observers hear focus move; a press on
/// the one holding focus keeps it, and a press on another does not focus it but takes
/// focus away.
#[test]
fn focus_targets_take_tab_but_not_presses() {
    let heard: Heard = Rc::default();
    let focuses: Focuses = Rc::default();
    let (seen, out) = (Rc::clone(&heard), Rc::clone(&focuses));
    let mut ui = interface(move || {
        out.borrow_mut().clear();
        column(|| {
            target("first", &seen, &out);
            target("second", &seen, &out);
        });
    });
    press(&mut ui, Key::Tab);
    assert_eq!(*heard.borrow(), [("first", true)]);
    press(&mut ui, Key::Tab);
    assert_eq!(heard.borrow()[1..], [("first", false), ("second", true)]);
    let second = ui.node_with_tag("second").expect("tagged").center();
    click(&mut ui, second);
    assert!(
        focuses.borrow()[1].is_focused(),
        "a press keeps focus where it is"
    );
    let first = ui.node_with_tag("first").expect("tagged").center();
    click(&mut ui, first);
    assert!(!focuses.borrow()[0].is_focused());
    assert!(!focuses.borrow()[1].is_focused());
    assert_eq!(heard.borrow().last(), Some(&("second", false)));
}

/// An observer hears its area lose focus when the area leaves the composition, and focus
/// is taken away rather than kept by something no longer shown.
#[test]
fn focus_leaves_with_its_area() {
    let heard: Heard = Rc::default();
    let shown = Rc::new(Cell::new(true));
    let focuses: Focuses = Rc::default();
    let (seen, out, showing) = (Rc::clone(&heard), Rc::clone(&focuses), Rc::clone(&shown));
    let mut ui = interface(move || {
        out.borrow_mut().clear();
        if showing.get() {
            target("gone", &seen, &out);
        }
    });
    press(&mut ui, Key::Tab);
    assert!(ui.has_focus());
    shown.set(false);
    ui.invalidate();
    ui.update();
    assert!(!ui.has_focus());
    assert_eq!(*heard.borrow(), [("gone", true), ("gone", false)]);
}

/// A test finds areas by tag, where they were laid out and whether they are seen, and gives
/// a tagged area focus; focus requested by the program is heard by the observers.
#[test]
fn tags_find_areas_and_take_focus() {
    let heard: Heard = Rc::default();
    let focuses: Focuses = Rc::default();
    let (seen, out) = (Rc::clone(&heard), Rc::clone(&focuses));
    let mut ui = interface(move || {
        out.borrow_mut().clear();
        column(|| {
            target("top", &seen, &out);
            target("bottom", &seen, &out);
            stack_with(Modifier::new().size(10.0, 10.0).test_tag("plain"), || {});
        });
    });
    let bottom = ui.node_with_tag("bottom").expect("tagged");
    assert_eq!(bottom.bounds.origin, Offset::new(0.0, 40.0));
    assert!(bottom.is_displayed());
    assert_eq!(ui.nodes_with_tag("top").len(), 1);
    assert!(ui.node_with_tag("missing").is_none());
    assert!(!ui.request_focus_on_tag("plain"));
    assert!(ui.request_focus_on_tag("bottom"));
    ui.update();
    assert!(focuses.borrow()[1].is_focused());
    assert_eq!(*heard.borrow(), [("bottom", true)]);
    let top = focuses.borrow()[0].clone();
    top.request_focus();
    ui.update();
    assert_eq!(heard.borrow()[1..], [("bottom", false), ("top", true)]);
}

/// Keys of what holds focus in content raised with a z-index, such as a popup, reach the
/// key handlers it was declared inside, though it is painted after them.
#[test]
fn raised_content_keys_reach_the_handlers_around_its_declaration() {
    let seen: Rc<RefCell<Vec<String>>> = Rc::default();
    let heard: Heard = Rc::default();
    let focuses: Focuses = Rc::default();
    let (keys, observed, out) = (Rc::clone(&seen), Rc::clone(&heard), Rc::clone(&focuses));
    let mut ui = interface(move || {
        out.borrow_mut().clear();
        let keys = Rc::clone(&keys);
        column_with(
            Modifier::new().on_preview_key_event(move |event| {
                keys.borrow_mut().push(format!("{:?}", event.key));
                false
            }),
            || {
                stack_with(Modifier::new().z_index_unclipped(1), || {
                    target("raised", &observed, &out);
                });
            },
        );
    });
    assert!(ui.request_focus_on_tag("raised"));
    ui.update();
    press(&mut ui, Key::Enter);
    assert_eq!(*seen.borrow(), ["Enter"]);
}

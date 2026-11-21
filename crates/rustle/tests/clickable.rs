//! Clickables of an [`Embedded`] interface as Compose's `Modifier.clickable` behaves: Tab
//! reaches them, a press does not focus them, Enter and the space bar run their click when
//! the key comes up, a focus requester gives them its focus, and an area inside one that
//! listens to the primary button takes the press from it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rustle::prelude::*;
use rustle::{
    Embedded, Focus, Key, KeyEvent, Modifiers, MutableInteractionSource, Offset, PointerButton,
    PointerPhase,
};

/// The clicks of the clickables of a test, by name, in order.
type Clicks = Rc<RefCell<Vec<&'static str>>>;

/// An interface 300 by 200 logical pixels of `root`, composed once.
fn interface(root: impl FnMut() + 'static) -> Embedded {
    let mut embedded = Embedded::without_renderer(root, || {});
    embedded.resize((300, 200), 1.0);
    embedded.update();
    embedded
}

/// Sends `key` going down, or coming up when not `pressed`, then composes.
fn key(embedded: &mut Embedded, key: Key, pressed: bool) {
    embedded.key_event(KeyEvent {
        pressed,
        ..KeyEvent::press(key, Modifiers::NONE)
    });
    embedded.update();
}

/// Presses and lets go of `pressed`, then composes.
fn stroke(embedded: &mut Embedded, pressed: Key) {
    key(embedded, pressed.clone(), true);
    key(embedded, pressed, false);
}

/// Clicks the primary button at `at`, then composes.
fn click(embedded: &mut Embedded, at: Offset) {
    embedded.pointer_moved(at);
    embedded.pointer_button(PointerButton::Primary, true);
    embedded.pointer_button(PointerButton::Primary, false);
    embedded.update();
}

/// A box 100 by 40 named `name` for tests that records its clicks into `clicks`.
#[track_caller]
fn clickable_box(name: &'static str, clicks: &Clicks) {
    let clicks = Rc::clone(clicks);
    stack_with(
        Modifier::new()
            .size(100.0, 40.0)
            .test_tag(name)
            .clickable(move || clicks.borrow_mut().push(name)),
        || {},
    );
}

/// Tab focuses a clickable, the key going down does not click it, coming up does, and the
/// space bar does the same; other keys and a key coming up that went down elsewhere do not.
#[test]
fn enter_and_space_click_the_focused_clickable_when_they_come_up() {
    let clicks: Clicks = Rc::default();
    let seen = Rc::clone(&clicks);
    let mut ui = interface(move || {
        column(|| {
            clickable_box("first", &seen);
            clickable_box("second", &seen);
        });
    });
    stroke(&mut ui, Key::Enter);
    assert!(clicks.borrow().is_empty(), "nothing holds focus yet");
    stroke(&mut ui, Key::Tab);
    assert!(ui.has_focus());
    key(&mut ui, Key::Enter, true);
    assert!(clicks.borrow().is_empty(), "the key went down only");
    key(&mut ui, Key::Enter, false);
    assert_eq!(*clicks.borrow(), ["first"]);
    stroke(&mut ui, Key::Tab);
    stroke(&mut ui, Key::Character(" ".into()));
    stroke(&mut ui, Key::Character("a".into()));
    key(&mut ui, Key::Enter, false);
    assert_eq!(*clicks.borrow(), ["first", "second"]);
}

/// A repeat of the held key is left to the handlers around the clickable, and the clickable
/// is clicked once when the key comes up.
#[test]
fn a_held_key_clicks_once() {
    let clicks: Clicks = Rc::default();
    let repeats = Rc::new(Cell::new(0));
    let (seen, heard) = (Rc::clone(&clicks), Rc::clone(&repeats));
    let mut ui = interface(move || {
        let heard = Rc::clone(&heard);
        let clicks = Rc::clone(&seen);
        stack_with(
            Modifier::new()
                .on_key_event(move |event| {
                    heard.set(heard.get() + usize::from(event.repeat));
                    event.repeat
                })
                .size(100.0, 40.0)
                .clickable(move || clicks.borrow_mut().push("held")),
            || {},
        );
    });
    stroke(&mut ui, Key::Tab);
    key(&mut ui, Key::Enter, true);
    ui.key_event(KeyEvent {
        repeat: true,
        ..KeyEvent::press(Key::Enter, Modifiers::NONE)
    });
    ui.update();
    key(&mut ui, Key::Enter, false);
    assert_eq!((repeats.get(), clicks.borrow().len()), (1, 1));
}

/// A press clicks a clickable without focusing it, and takes focus from what held it.
#[test]
fn a_press_clicks_without_focusing() {
    let clicks: Clicks = Rc::default();
    let seen = Rc::clone(&clicks);
    let mut ui = interface(move || {
        column(|| {
            clickable_box("first", &seen);
            clickable_box("second", &seen);
        });
    });
    stroke(&mut ui, Key::Tab);
    let second = ui.node_with_tag("second").expect("tagged").center();
    click(&mut ui, second);
    assert_eq!(*clicks.borrow(), ["second"]);
    assert!(!ui.has_focus(), "a press does not focus a clickable");
}

/// A focus requester before a clickable gives it its focus: requesting it focuses the
/// clickable, which then reads as focused through the requester and takes its keys.
#[test]
fn a_focus_requester_gives_a_clickable_its_focus() {
    let focus = Focus::new();
    let clicks: Clicks = Rc::default();
    let (requester, seen) = (focus.clone(), Rc::clone(&clicks));
    let mut ui = interface(move || {
        let clicks = Rc::clone(&seen);
        let source = MutableInteractionSource::new();
        stack_with(
            Modifier::new()
                .size(100.0, 40.0)
                .focus_requester(&requester)
                .clickable_with(&source, None, move || clicks.borrow_mut().push("requested")),
            || {},
        );
    });
    focus.request_focus();
    ui.update();
    assert!(focus.is_focused());
    stroke(&mut ui, Key::Enter);
    assert_eq!(*clicks.borrow(), ["requested"]);
}

/// An area inside a clickable that listens to the primary button takes a press on it, so
/// the clickable is not clicked; a press beside that area clicks the clickable.
#[test]
fn a_button_inside_takes_the_press() {
    let clicks: Clicks = Rc::default();
    let seen = Rc::clone(&clicks);
    let mut ui = interface(move || {
        let (outer, inner) = (Rc::clone(&seen), Rc::clone(&seen));
        stack_with(
            Modifier::new()
                .size(200.0, 40.0)
                .clickable(move || outer.borrow_mut().push("outer")),
            move || {
                stack_with(
                    Modifier::new().size(50.0, 40.0).on_pointer_button(
                        PointerButton::Primary,
                        move |event| {
                            if event.phase == PointerPhase::Click {
                                inner.borrow_mut().push("inner");
                            }
                        },
                    ),
                    || {},
                );
            },
        );
    });
    click(&mut ui, Offset::new(25.0, 20.0));
    assert_eq!(*clicks.borrow(), ["inner"]);
    click(&mut ui, Offset::new(150.0, 20.0));
    assert_eq!(*clicks.borrow(), ["inner", "outer"]);
}

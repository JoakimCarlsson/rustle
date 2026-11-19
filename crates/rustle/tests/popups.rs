//! Popups placed by a position provider against the layout they are declared in, taking the
//! presses on them, dismissed by a press outside them (which reaches what it landed on unless
//! they are focusable) or by Escape when they are focusable, and tooltips shown once the
//! pointer has rested on their box.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use rustle::prelude::*;
use rustle::{
    DropdownMenuPositionProvider, Embedded, Key, KeyEvent, Modifiers, Offset, PointerButton,
    PopupProperties, Rect, Size, popup_position_provider_at_position, popup_with, tooltip_area,
};

/// An interface 300 by 200 logical pixels of `root`, composed once at frame time nought.
fn interface(root: impl FnMut() + 'static) -> Embedded {
    let mut embedded = Embedded::without_renderer(root, || {});
    embedded.resize((300, 200), 1.0);
    embedded.advance_to(Duration::ZERO);
    embedded.update();
    embedded
}

/// Clicks the primary button at `at`, then composes.
fn click(embedded: &mut Embedded, at: Offset) {
    embedded.pointer_moved(at);
    embedded.pointer_button(PointerButton::Primary, true);
    embedded.pointer_button(PointerButton::Primary, false);
    embedded.update();
}

/// Where the area tagged `tag` was laid out.
fn bounds(embedded: &Embedded, tag: &str) -> Rect {
    embedded.node_with_tag(tag).expect(tag).bounds
}

/// A menu 60 by 30 named "menu", opened from an anchor 100 by 40 named "anchor" placed at
/// (`x`, `y`), below it or above it where the window has no room.
fn anchored_menu(x: f32, y: f32) -> impl FnMut() {
    move || {
        stack_with(Modifier::new().fill_max_size(), || {
            stack_with(
                Modifier::new()
                    .offset(x, y)
                    .size(100.0, 40.0)
                    .test_tag("anchor"),
                || {
                    popup_with(
                        DropdownMenuPositionProvider {
                            content_offset: Offset::new(0.0, 4.0),
                            vertical_margin: 0.0,
                        },
                        None,
                        PopupProperties::default(),
                        || spacer(Modifier::new().size(60.0, 30.0).test_tag("menu")),
                    );
                },
            );
        });
    }
}

/// A popup goes where its provider puts it against the box it is declared in, without
/// taking room there, and flips above its anchor near the window's bottom.
#[test]
fn a_popup_is_placed_against_its_anchor() {
    let ui = interface(anchored_menu(20.0, 30.0));
    assert_eq!(bounds(&ui, "anchor").origin, Offset::new(20.0, 30.0));
    assert_eq!(bounds(&ui, "anchor").size, Size::new(100.0, 40.0));
    assert_eq!(bounds(&ui, "menu").origin, Offset::new(20.0, 74.0));
    let ui = interface(anchored_menu(20.0, 150.0));
    assert_eq!(bounds(&ui, "menu").origin, Offset::new(20.0, 116.0));
}

/// A popup at a point keeps its margin from the window's edges and is over what comes after
/// it in the tree, taking the presses on it whether it is dismissible or not.
#[test]
fn a_popup_at_a_point_stays_inside_its_margin_and_on_top() {
    for dismissible in [true, false] {
        let pressed = Rc::new(Cell::new(0));
        let count = Rc::clone(&pressed);
        let mut ui = interface(move || {
            let count = Rc::clone(&count);
            column(|| {
                popup_with(
                    popup_position_provider_at_position(Offset::new(280.0, 10.0), 8.0),
                    dismissible.then(|| Rc::new(|| {}) as Rc<dyn Fn()>),
                    PopupProperties {
                        dismiss_on_click_outside: dismissible,
                        ..PopupProperties::default()
                    },
                    || spacer(Modifier::new().size(60.0, 30.0).test_tag("menu")),
                );
                spacer(
                    Modifier::new()
                        .size(300.0, 200.0)
                        .clickable(move || count.set(count.get() + 1)),
                );
            });
        });
        assert_eq!(bounds(&ui, "menu").origin, Offset::new(232.0, 10.0));
        click(&mut ui, Offset::new(250.0, 20.0));
        assert_eq!(pressed.get(), 0);
    }
}

/// A focusable popup takes the presses outside it, which ask to dismiss it but do not reach
/// what they landed on; one that is not focusable lets them through.
#[test]
fn a_focusable_popup_takes_the_presses_outside_it() {
    for focusable in [true, false] {
        let dismissed = Rc::new(Cell::new(0));
        let clicked = Rc::new(Cell::new(0));
        let (dismiss, click_count) = (Rc::clone(&dismissed), Rc::clone(&clicked));
        let mut ui = interface(move || {
            let (dismiss, click_count) = (Rc::clone(&dismiss), Rc::clone(&click_count));
            column(|| {
                popup_with(
                    popup_position_provider_at_position(Offset::new(10.0, 10.0), 0.0),
                    Some(Rc::new(move || dismiss.set(dismiss.get() + 1))),
                    PopupProperties {
                        focusable,
                        ..PopupProperties::default()
                    },
                    || spacer(Modifier::new().size(60.0, 30.0)),
                );
                spacer(
                    Modifier::new()
                        .size(300.0, 200.0)
                        .clickable(move || click_count.set(click_count.get() + 1)),
                );
            });
        });
        click(&mut ui, Offset::new(200.0, 150.0));
        assert_eq!(dismissed.get(), 1, "a press outside asks to dismiss it");
        assert_eq!(clicked.get(), u32::from(!focusable));
    }
}

/// A press outside a popup asks to dismiss it and still clicks what it landed on; a press
/// inside does neither.
#[test]
fn a_press_outside_dismisses_and_goes_on() {
    let dismissed = Rc::new(Cell::new(0));
    let clicked = Rc::new(Cell::new(0));
    let (dismiss, click_count) = (Rc::clone(&dismissed), Rc::clone(&clicked));
    let mut ui = interface(move || {
        let (dismiss, click_count) = (Rc::clone(&dismiss), Rc::clone(&click_count));
        column(|| {
            stack_with(Modifier::new().size(100.0, 40.0), || {
                popup_with(
                    DropdownMenuPositionProvider {
                        content_offset: Offset::default(),
                        vertical_margin: 0.0,
                    },
                    Some(Rc::new(move || dismiss.set(dismiss.get() + 1))),
                    PopupProperties::default(),
                    || spacer(Modifier::new().size(60.0, 30.0).clickable(|| {})),
                );
            });
            spacer(
                Modifier::new()
                    .offset(150.0, 0.0)
                    .size(100.0, 40.0)
                    .clickable(move || click_count.set(click_count.get() + 1)),
            );
        });
    });
    click(&mut ui, Offset::new(20.0, 55.0));
    assert_eq!(dismissed.get(), 0, "a press inside keeps it");
    click(&mut ui, Offset::new(200.0, 60.0));
    assert_eq!(dismissed.get(), 1, "a press outside asks to dismiss it");
    assert_eq!(clicked.get(), 1, "and the press still reaches the button");
}

/// Escape dismisses a focusable popup before the window's own key handlers see it, and does
/// nothing to one that is not focusable.
#[test]
fn escape_dismisses_a_focusable_popup_first() {
    for focusable in [true, false] {
        let dismissed = Rc::new(Cell::new(false));
        let previewed = Rc::new(Cell::new(false));
        let (dismiss, preview) = (Rc::clone(&dismissed), Rc::clone(&previewed));
        let mut ui = interface(move || {
            let (dismiss, preview) = (Rc::clone(&dismiss), Rc::clone(&preview));
            stack_with(
                Modifier::new()
                    .fill_max_size()
                    .on_preview_key_event(move |_| {
                        preview.set(true);
                        false
                    }),
                || {
                    popup_with(
                        DropdownMenuPositionProvider::default(),
                        Some(Rc::new(move || dismiss.set(true))),
                        PopupProperties {
                            focusable,
                            ..PopupProperties::default()
                        },
                        || spacer(Modifier::new().size(60.0, 30.0)),
                    );
                },
            );
        });
        ui.key_event(KeyEvent::press(Key::Escape, Modifiers::NONE));
        assert_eq!(dismissed.get(), focusable);
        assert_eq!(previewed.get(), !focusable);
    }
}

/// A tooltip area shows its tooltip once the pointer has rested on it for the delay, on the
/// frame clock (counted from the first frame after the pointer came), and hides it when the
/// pointer leaves; with no delay it shows at once.
#[test]
fn a_tooltip_shows_after_the_pointer_rests() {
    for delay in [300, 0] {
        let mut ui = interface(move || {
            column(|| {
                tooltip_area(
                    || spacer(Modifier::new().size(50.0, 20.0).test_tag("tip")),
                    Modifier::new().offset(100.0, 100.0),
                    delay,
                    rustle::PlainTooltipPositionProvider { spacing: 4.0 },
                    || spacer(Modifier::new().size(40.0, 20.0)),
                );
            });
        });
        ui.pointer_moved(Offset::new(120.0, 110.0));
        ui.update();
        if delay > 0 {
            assert!(ui.node_with_tag("tip").is_none(), "not before the delay");
            ui.advance_to(Duration::from_millis(299));
            assert!(
                ui.node_with_tag("tip").is_none(),
                "not while the delay runs"
            );
            ui.advance_to(Duration::from_millis(700));
            ui.update();
        }
        let tip = bounds(&ui, "tip");
        assert_eq!(tip.origin, Offset::new(95.0, 76.0), "centred above the box");
        ui.pointer_moved(Offset::new(10.0, 10.0));
        ui.update();
        ui.advance_to(Duration::from_millis(400));
        ui.update();
        assert!(
            ui.node_with_tag("tip").is_none(),
            "gone once the pointer left"
        );
    }
}

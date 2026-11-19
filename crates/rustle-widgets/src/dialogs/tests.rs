//! Tests of `dialog_with` and its `DialogProperties` against the regions it paints.

use std::cell::Cell;
use std::rc::Rc;

use rustle_foundation::{BoxAlignment, Modifier, canvas, stack_aligned};
use rustle_geometry::{Offset, Size};
use rustle_text::{LineLayout, StyledSpan, TextLayout, TextMeasurer, TextStyle};
use rustle_ui::{DisplayList, Key, KeyEvent, Modifiers, PointerButton, composition, display_list};
use rustle_unit::Constraints;

use super::{DialogProperties, dialog_with};

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

/// Lays out, in a 300 by 200 window, a box answering the primary button beneath a dialog
/// with `properties` holding a 100 by 50 box; returns the display list and how many times
/// the dialog was asked to be dismissed.
fn dialogued(properties: DialogProperties) -> (DisplayList, Rc<Cell<u32>>) {
    let dismissed = Rc::new(Cell::new(0));
    let counted = Rc::clone(&dismissed);
    let mut composition = composition(move || {
        let counted = Rc::clone(&counted);
        stack_aligned(
            Modifier::new().fill_max_size(),
            BoxAlignment::TopStart,
            move || {
                canvas(
                    Modifier::new()
                        .fill_max_size()
                        .on_pointer_button(PointerButton::Primary, |_| {}),
                    |_| {},
                );
                dialog_with(
                    move || counted.set(counted.get() + 1),
                    properties,
                    || canvas(Modifier::new().size(100.0, 50.0), |_| {}),
                );
            },
        );
    });
    composition.compose();
    let window = Constraints::loose(Size::new(300.0, 200.0));
    let list = display_list(composition.output().unwrap(), window, &mut NoText);
    (list, dismissed)
}

/// A click on the dimmed layer asks to dismiss; one on the content does not.
#[test]
fn click_outside_dismisses() {
    let (list, dismissed) = dialogued(DialogProperties::default());
    (list.hit_test(Offset::new(10.0, 10.0)).unwrap().on_click)();
    assert_eq!(dismissed.get(), 1);
    (list.hit_test(Offset::new(150.0, 100.0)).unwrap().on_click)();
    assert_eq!(dismissed.get(), 1);
}

/// Without `dismiss_on_click_outside` a click on the dimmed layer does nothing.
#[test]
fn click_outside_can_be_kept() {
    let (list, dismissed) = dialogued(DialogProperties {
        dismiss_on_click_outside: false,
        ..DialogProperties::default()
    });
    (list.hit_test(Offset::new(10.0, 10.0)).unwrap().on_click)();
    assert_eq!(dismissed.get(), 0);
}

/// Escape asks to dismiss while `dismiss_on_back_press` is on, and not otherwise.
#[test]
fn escape_follows_back_press() {
    let escape = KeyEvent::press(Key::Escape, Modifiers::NONE);
    let (list, dismissed) = dialogued(DialogProperties::default());
    assert!(list.dispatch_key(None, &escape));
    assert_eq!(dismissed.get(), 1);
    let (list, dismissed) = dialogued(DialogProperties {
        dismiss_on_back_press: false,
        ..DialogProperties::default()
    });
    assert!(!list.dispatch_key(None, &escape));
    assert_eq!(dismissed.get(), 0);
}

/// Pointer buttons over the dimmed layer stop there and never reach the box beneath it.
#[test]
fn nothing_beneath_takes_presses() {
    let (list, _) = dialogued(DialogProperties::default());
    let region = list
        .button_at(Offset::new(10.0, 10.0), PointerButton::Primary)
        .unwrap();
    let topmost = list
        .buttons
        .iter()
        .rposition(|region| region.button == PointerButton::Primary)
        .unwrap();
    assert!(std::ptr::eq(region, &list.buttons[topmost]));
    assert!(
        list.buttons
            .iter()
            .filter(|region| region.button == PointerButton::Primary)
            .count()
            >= 2
    );
}

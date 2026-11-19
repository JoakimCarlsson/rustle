//! Drives an [`Embedded`] interface and checks what Compose's platform-facing APIs do there:
//! text input sessions of areas that draw their own text, interaction sources reported by
//! `clickable_with`, `hoverable` and `focusable_with`, and indications drawn from them.

use std::cell::RefCell;
use std::rc::Rc;

use rustle::prelude::*;
use rustle::{
    Embedded, Indication, IndicationNodeFactory, InteractionSource, LOCAL_INDICATION,
    MutableInteractionSource, Offset, PlatformTextInputMethodRequest, PlatformTextInputSession,
    PointerButton, Preedit, Rect, Size,
};
use rustle_graphics::DrawCommand;

/// A headless interface of 300 by 300 logical pixels with `root` composed in it.
fn rig(root: impl FnMut() + 'static) -> Embedded {
    let mut embedded = Embedded::without_renderer(root, || {});
    embedded.resize((300, 300), 1.0);
    embedded.update();
    embedded
}

/// What a terminal-like area heard from the input method.
#[derive(Default)]
struct Heard {
    preedits: Vec<Option<String>>,
    commits: Vec<String>,
}

/// A request that records what it hears and reports a caret at (4, 6), 2 by 16.
struct Recorder(Rc<RefCell<Heard>>);

impl PlatformTextInputMethodRequest for Recorder {
    fn on_preedit(&self, preedit: Option<&Preedit>) {
        self.0
            .borrow_mut()
            .preedits
            .push(preedit.map(|preedit| preedit.text.clone()));
    }

    fn on_commit(&self, text: &str) {
        self.0.borrow_mut().commits.push(text.to_string());
    }

    fn focused_rect(&self) -> Option<Rect> {
        Some(Rect::new(Offset::new(4.0, 6.0), Size::new(2.0, 16.0)))
    }
}

#[test]
fn a_session_takes_the_input_method_while_its_area_is_focused() {
    let heard = Rc::new(RefCell::new(Heard::default()));
    let session = PlatformTextInputSession::new();
    let held = session.clone();
    let mut ui = rig(move || {
        let focus = remember_focus();
        canvas(
            Modifier::new()
                .padding(20.0)
                .size(200.0, 100.0)
                .focusable(&focus, |_| false)
                .platform_text_input(&held),
            |_| {},
        );
    });
    session.start_input_method(Recorder(Rc::clone(&heard)));
    ui.preedit("ka", Some((0, 2)));
    assert!(heard.borrow().preedits.is_empty(), "not focused yet");

    ui.pointer_moved(Offset::new(50.0, 50.0));
    ui.pointer_button(PointerButton::Primary, true);
    ui.pointer_button(PointerButton::Primary, false);
    ui.update();
    ui.preedit("か", Some((0, 3)));
    assert!(ui.commit("火"));
    assert_eq!(heard.borrow().preedits, vec![Some("か".to_string())]);
    assert_eq!(heard.borrow().commits, vec!["火".to_string()]);
    assert_eq!(
        ui.ime_area(),
        Some(Rect::new(Offset::new(24.0, 26.0), Size::new(2.0, 16.0)))
    );

    session.stop_input_method();
    assert!(!session.is_active());
    ui.preedit("x", None);
    assert_eq!(heard.borrow().preedits.len(), 1);
}

/// An indication that draws a black overlay of alpha 40 while pressed and 20 while hovered.
struct Overlay;

impl IndicationNodeFactory for Overlay {
    fn create(&self, interaction_source: &InteractionSource) -> Modifier {
        let pressed = interaction_source.collect_is_pressed_as_state();
        let hovered = interaction_source.collect_is_hovered_as_state();
        Modifier::new().draw_with_content(move |canvas| {
            canvas.draw_content();
            let alpha = if pressed.get() {
                40
            } else if hovered.get() {
                20
            } else {
                0
            };
            if alpha > 0 {
                canvas.rect(
                    0.0,
                    0.0,
                    canvas.width(),
                    canvas.height(),
                    Color::rgba(0, 0, 0, alpha),
                );
            }
        })
    }
}

/// The alpha of the overlay drawn now, if any.
fn overlay(ui: &Embedded) -> Option<u8> {
    ui.commands().iter().find_map(|command| match command {
        DrawCommand::Rect { color, .. } if color.r == 0 && color.g == 0 => Some(color.a),
        _ => None,
    })
}

#[test]
fn clickable_reports_hover_and_press_to_its_source_and_indication() {
    let source = MutableInteractionSource::new();
    let clicks = Rc::new(RefCell::new(0));
    let (held, counted) = (source.clone(), Rc::clone(&clicks));
    let indication = Indication::new(Overlay);
    let mut ui = rig(move || {
        let counted = Rc::clone(&counted);
        let held = held.clone();
        LOCAL_INDICATION.provide(Some(indication.clone()), || {
            let indication = LOCAL_INDICATION.current();
            canvas(
                Modifier::new().size(100.0, 50.0).clickable_with(
                    &held,
                    indication.as_ref(),
                    move || *counted.borrow_mut() += 1,
                ),
                |_| {},
            );
        });
    });
    let (pressed, hovered) = (
        source.collect_is_pressed_as_state(),
        source.collect_is_hovered_as_state(),
    );
    assert_eq!(overlay(&ui), None);
    ui.pointer_moved(Offset::new(10.0, 10.0));
    assert!(hovered.get() && !pressed.get());
    ui.update();
    assert_eq!(overlay(&ui), Some(20));
    ui.pointer_button(PointerButton::Primary, true);
    assert!(pressed.get());
    ui.update();
    assert_eq!(overlay(&ui), Some(40));
    ui.pointer_button(PointerButton::Primary, false);
    assert!(!pressed.get());
    assert_eq!(*clicks.borrow(), 1);
    ui.pointer_left();
    assert!(!hovered.get());
    ui.update();
    assert_eq!(overlay(&ui), None);
}

#[test]
fn focusable_with_reports_focus() {
    let source = MutableInteractionSource::new();
    let held = source.clone();
    let mut ui = rig(move || {
        let focus = remember_focus();
        canvas(
            Modifier::new()
                .size(100.0, 50.0)
                .focusable_with(&focus, &held, |_| false)
                .test_tag("field"),
            |_| {},
        );
    });
    let focused = source.collect_is_focused_as_state();
    assert!(!focused.get());
    assert!(ui.request_focus_on_tag("field"));
    ui.update();
    assert!(focused.get());
}

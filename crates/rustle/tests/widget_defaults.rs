//! Composes the Compose-shaped widgets that take their whole look as parameters (surfaces,
//! cards, dividers, check boxes and switches) in a headless [`Embedded`] interface, and checks
//! their sizes, what they draw and how they answer clicks.

use std::cell::Cell;
use std::rc::Rc;

use rustle::prelude::*;
use rustle::{
    BorderStroke, CheckboxDefaults, Embedded, Offset, PointerButton, SwitchDefaults,
    ToggleableState, checkbox_with, horizontal_divider, outlined_card, surface_custom, switch_with,
    tri_state_checkbox, vertical_divider,
};
use rustle_graphics::DrawCommand;

/// An interface 300 by 200 logical pixels of `root`, composed once.
fn interface(root: impl FnMut() + 'static) -> Embedded {
    let mut embedded = Embedded::without_renderer(root, || {});
    embedded.resize((300, 200), 1.0);
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

/// The middle of `rect`.
fn middle(rect: rustle::Rect) -> Offset {
    Offset::new(
        rect.origin.x + rect.size.width / 2.0,
        rect.origin.y + rect.size.height / 2.0,
    )
}

/// The bounds of the area tagged `tag`.
fn bounds(embedded: &Embedded, tag: &str) -> rustle::Rect {
    embedded.node_with_tag(tag).expect("tagged").bounds
}

/// The paths the interface draws.
fn paths(embedded: &Embedded) -> usize {
    embedded
        .commands()
        .iter()
        .filter(|command| matches!(command, DrawCommand::Path { .. }))
        .count()
}

/// A surface given a size hands it to content that has none of its own, as Compose's
/// `Surface` does, and draws its fill and border.
#[test]
fn a_surface_gives_its_size_to_its_content() {
    let fill = Color::rgb(10, 20, 30);
    let ui = interface(move || {
        surface_custom(
            Modifier::new().size(120.0, 40.0),
            Shape::rounded(6.0),
            fill,
            Color::rgb(255, 255, 255),
            Some(BorderStroke::new(1.0, Color::rgb(0, 0, 0))),
            || spacer(Modifier::new().test_tag("content")),
        );
    });
    let content = bounds(&ui, "content");
    assert_eq!((content.size.width, content.size.height), (120.0, 40.0));
    assert!(ui.commands().iter().any(|command| matches!(
        command,
        DrawCommand::Rect { color, .. } if *color == fill
    )));
}

/// An outlined card holds its content one under another, and dividers take the thickness
/// they are given across the room they have.
#[test]
fn cards_stack_their_content_and_dividers_span_their_room() {
    let ui = interface(|| {
        outlined_card(
            Modifier::new().width(200.0),
            Shape::rounded(8.0),
            rustle::CardDefaults::outlined_card_colors(),
            rustle::CardDefaults::outlined_card_border(),
            || {
                spacer(Modifier::new().size(50.0, 10.0).test_tag("first"));
                spacer(Modifier::new().size(50.0, 10.0).test_tag("second"));
            },
        );
        row_with(Modifier::new().height(30.0), || {
            vertical_divider(
                Modifier::new().test_tag("vertical"),
                2.0,
                Color::rgb(0, 0, 0),
            );
        });
        column_with(Modifier::new().width(80.0), || {
            horizontal_divider(
                Modifier::new().test_tag("horizontal"),
                3.0,
                Color::rgb(0, 0, 0),
            );
        });
    });
    assert_eq!(
        bounds(&ui, "second").origin.y,
        bounds(&ui, "first").origin.y + 10.0
    );
    let vertical = bounds(&ui, "vertical");
    assert_eq!((vertical.size.width, vertical.size.height), (2.0, 30.0));
    let horizontal = bounds(&ui, "horizontal");
    assert_eq!((horizontal.size.width, horizontal.size.height), (80.0, 3.0));
}

/// A check box given a change handler toggles on a click; one given none, as inside a larger
/// control, leaves the click to what is around it. It fills the size its modifier gives it
/// and draws a mark only while ticked or dashed.
#[test]
fn check_boxes_answer_clicks_only_with_a_handler() {
    let checked = rustle::mutable_state(false);
    let around = Rc::new(Cell::new(0));
    let (held, heard) = (checked.clone(), Rc::clone(&around));
    let mut ui = interface(move || {
        let held = held.clone();
        let heard = Rc::clone(&heard);
        column_with(Modifier::new(), move || {
            let set = held.clone();
            checkbox_with(
                Modifier::new().test_tag("handled"),
                held.get(),
                Some(Box::new(move |on| set.set(on))),
                true,
                CheckboxDefaults::colors(),
            );
            stack_with(
                Modifier::new().clickable(move || heard.set(heard.get() + 1)),
                || {
                    tri_state_checkbox(
                        Modifier::new().size(24.0, 24.0).test_tag("inside"),
                        ToggleableState::Indeterminate,
                        None,
                        true,
                        CheckboxDefaults::colors(),
                    );
                },
            );
        });
    });
    let handled = bounds(&ui, "handled");
    assert_eq!(handled.size.width, CheckboxDefaults::SIZE);
    assert_eq!(bounds(&ui, "inside").size.width, 24.0);
    assert_eq!(paths(&ui), 1);
    click(&mut ui, middle(handled));
    assert!(checked.get());
    assert_eq!(paths(&ui), 2);
    let inside = middle(bounds(&ui, "inside"));
    click(&mut ui, inside);
    assert_eq!(around.get(), 1);
}

/// A switch toggles on a click with a handler, keeps its default size, and a switched off
/// one ignores clicks.
#[test]
fn switches_toggle_when_enabled() {
    let on = Rc::new(Cell::new(false));
    let held = Rc::clone(&on);
    let mut ui = interface(move || {
        let held = Rc::clone(&held);
        column_with(Modifier::new(), || {
            let set = Rc::clone(&held);
            switch_with(
                Modifier::new().test_tag("live"),
                held.get(),
                Some(Box::new(move |value| set.set(value))),
                true,
                SwitchDefaults::colors(),
            );
            let set = Rc::clone(&held);
            switch_with(
                Modifier::new().test_tag("off"),
                false,
                Some(Box::new(move |_| set.set(false))),
                false,
                SwitchDefaults::colors(),
            );
        });
    });
    let live = bounds(&ui, "live");
    assert_eq!(
        (live.size.width, live.size.height),
        (SwitchDefaults::WIDTH, SwitchDefaults::HEIGHT)
    );
    click(&mut ui, middle(live));
    assert!(on.get());
    let off = middle(bounds(&ui, "off"));
    click(&mut ui, off);
    assert!(on.get());
}

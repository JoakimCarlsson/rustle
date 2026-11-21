//! Focus in a lazy list of an [`Embedded`] interface, as Compose's lazy layouts and focus
//! targets behave: Tab moves on to the item just past those in view and the list scrolls it
//! into view, so Tab walks every item; and the item holding focus stays composed, and keeps
//! focus, while it is scrolled out of view.

use rustle::prelude::*;
use rustle::{
    Embedded, Key, KeyEvent, LazyListState, Modifiers, Offset, ScrollState, remember_focus,
};

/// An interface 200 by 100 logical pixels of `root`, composed once.
fn interface(root: impl FnMut() + 'static) -> Embedded {
    let mut embedded = Embedded::without_renderer(root, || {});
    embedded.resize((200, 100), 1.0);
    embedded.update();
    embedded
}

/// Presses `key` with no modifiers, then composes twice, so a scroll it asked for is laid out.
fn press(embedded: &mut Embedded, key: Key) {
    embedded.key_event(KeyEvent::press(key, Modifiers::NONE));
    embedded.update();
    embedded.update();
}

/// A list 100 high of `count` items 40 high, each a focus target named `item <index>`.
fn list(state: LazyListState, count: usize) -> impl FnMut() {
    move || {
        lazy_list(Modifier::new().size(200.0, 100.0), &state, count, |index| {
            let focus = remember_focus();
            stack_with(
                Modifier::new()
                    .fill_max_width()
                    .height(40.0)
                    .test_tag(format!("item {index}"))
                    .focus_target(&focus),
                || {},
            );
        });
    }
}

/// Where the item tagged `tag` is, when it is composed.
fn top(embedded: &Embedded, tag: &str) -> Option<f32> {
    embedded.node_with_tag(tag).map(|node| node.bounds.origin.y)
}

/// Tab walks every item of a list longer than its view, each brought wholly into view as it
/// takes focus.
#[test]
fn tab_walks_past_the_items_in_view() {
    let state = LazyListState::new(0, 0.0);
    let mut ui = interface(list(state.clone(), 10));
    for index in 0..10 {
        press(&mut ui, Key::Tab);
        let at = top(&ui, &format!("item {index}")).expect("the focused item is composed");
        assert!(
            (0.0..=60.0).contains(&at),
            "item {index} is at {at}, outside the view"
        );
    }
    assert!(state.first_visible_item().is_some_and(|first| first >= 7));
}

/// The item holding focus stays composed, and keeps focus, while the wheel scrolls it out of
/// view.
#[test]
fn the_focused_item_stays_composed_out_of_view() {
    let state = LazyListState::new(0, 0.0);
    let mut ui = interface(list(state.clone(), 30));
    press(&mut ui, Key::Tab);
    ui.pointer_moved(Offset::new(100.0, 50.0));
    for _ in 0..10 {
        ui.wheel(Offset::new(0.0, -100.0));
        ui.update();
    }
    assert!(state.first_visible_item().is_some_and(|first| first > 5));
    assert!(top(&ui, "item 0").is_some(), "the focused item left");
    assert!(ui.has_focus());
}

/// An area taking focus in a scrolling column is scrolled into view the shorter way.
#[test]
fn focus_brings_an_area_into_view_in_a_scrolling_column() {
    let scroll = ScrollState::new(0.0);
    let state = scroll.clone();
    let mut ui = interface(move || {
        column_with(
            Modifier::new().size(200.0, 100.0).vertical_scroll(&state),
            || {
                for _ in 0..5 {
                    let focus = remember_focus();
                    stack_with(
                        Modifier::new()
                            .fill_max_width()
                            .height(40.0)
                            .focus_target(&focus),
                        || {},
                    );
                }
            },
        );
    });
    for _ in 0..3 {
        press(&mut ui, Key::Tab);
    }
    assert_eq!(scroll.offset(), 20.0);
}

/// From the item holding focus scrolled out of view, Shift+Tab moves to the item before it,
/// which the list composes beside it and scrolls into view.
#[test]
fn focus_moves_on_from_a_pinned_item() {
    let state = LazyListState::new(0, 0.0);
    let mut ui = interface(list(state.clone(), 30));
    for _ in 0..20 {
        press(&mut ui, Key::Tab);
    }
    state.scroll_to_item_offset(0, 0.0);
    ui.update();
    ui.update();
    assert!(top(&ui, "item 19").is_some(), "the focused item is pinned");
    ui.key_event(KeyEvent::press(Key::Tab, Modifiers::SHIFT));
    ui.update();
    ui.update();
    let at = top(&ui, "item 18").expect("the item before is composed");
    assert!((0.0..=60.0).contains(&at), "item 18 is at {at}");
}

/// Focus given while the interface has no size yet scrolls nothing, since nothing of the
/// scrolling area is in view to bring the focused area into.
#[test]
fn focus_before_a_size_scrolls_nothing() {
    let scroll = ScrollState::new(0.0);
    let state = scroll.clone();
    let focus = rustle::Focus::new();
    let held = focus.clone();
    let mut ui = Embedded::without_renderer(
        move || {
            column_with(
                Modifier::new().fill_max_size().vertical_scroll(&state),
                || {
                    spacer(Modifier::new().height(300.0));
                    stack_with(
                        Modifier::new()
                            .fill_max_width()
                            .height(40.0)
                            .focus_target(&held),
                        || {},
                    );
                },
            );
        },
        || {},
    );
    focus.request_focus();
    ui.update();
    assert!(focus.is_focused());
    assert_eq!(scroll.offset(), 0.0);
}

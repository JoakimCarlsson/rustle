//! Gesture input: drag, anchored drag, pan and zoom with several pointers, and nested scrolling.

use rustle::prelude::*;

use crate::ui::panel;

/// A knob dragged along a track that settles on one of three stops when let go.
#[composable]
fn anchored_track() {
    let theme = color_scheme();
    let state = remember_anchored_draggable_state(vec![0.0, 80.0, 160.0], 0);
    let stop = state.target();
    stack_with(
        Modifier::new()
            .width(196.0)
            .height(36.0)
            .background_rounded(theme.surface_variant, 18.0)
            .anchored_draggable(&state, Axis::Horizontal),
        || {
            stack_with(
                Modifier::new()
                    .offset(state.offset() + 2.0, 2.0)
                    .size(32.0, 32.0)
                    .background_rounded(theme.primary, 16.0),
                || {},
            );
        },
    );
    muted(&format!("Settled on stop {}", stop + 1));
}

/// A square dragged freely in one axis, with how far and how fast it was let go.
#[composable]
fn draggable_box() {
    let theme = color_scheme();
    let state = remember_draggable_state();
    stack_with(
        Modifier::new()
            .width(260.0)
            .height(44.0)
            .background_rounded(theme.surface_variant, 8.0)
            .clip(),
        || {
            stack_with(
                Modifier::new()
                    .offset(state.offset().clamp(0.0, 220.0), 4.0)
                    .size(36.0, 36.0)
                    .background_rounded(theme.primary, 8.0)
                    .draggable(&state, Axis::Horizontal),
                || {},
            );
        },
    );
    muted(&format!(
        "Offset {:.0} px, let go at {:.0} px/s",
        state.offset(),
        state.velocity()
    ));
}

/// An area panned with one pointer, zoomed and turned with two (Ctrl and the wheel zoom).
#[composable]
fn transform_area() {
    let theme = color_scheme();
    let state = remember_transformable_state();
    stack_with(
        Modifier::new()
            .width(260.0)
            .height(120.0)
            .background_rounded(theme.surface_variant, 8.0)
            .clip()
            .transformable(&state),
        || {
            stack_with(
                Modifier::new()
                    .offset(100.0 + state.offset().x, 30.0 + state.offset().y)
                    .size(60.0, 60.0)
                    .graphics_layer(1.0, state.rotation().to_degrees(), state.zoom())
                    .background_rounded(theme.primary, 10.0),
                || {},
            );
        },
    );
    muted(&format!(
        "Pan {:.0}, {:.0}  zoom {:.2}  turn {:.0} deg",
        state.offset().x,
        state.offset().y,
        state.zoom(),
        state.rotation().to_degrees()
    ));
}

/// A list inside a list: the inner one scrolls first, the outer takes what is left over.
#[composable]
fn nested_lists() {
    let theme = color_scheme();
    let outer_position = remember(|| 0.0_f32);
    let outer = remember_scrollable_state({
        let position = outer_position.clone();
        move |delta| {
            let before = position.get();
            let after = (before + delta).clamp(0.0, 200.0);
            position.set(after);
            after - before
        }
    });
    let inner_position = remember(|| 0.0_f32);
    let inner = remember_scrollable_state({
        let position = inner_position.clone();
        move |delta| {
            let before = position.get();
            let after = (before + delta).clamp(0.0, 160.0);
            position.set(after);
            after - before
        }
    });
    stack_with(
        Modifier::new()
            .width(260.0)
            .height(140.0)
            .background_rounded(theme.surface_variant, 8.0)
            .clip()
            .scrollable(&outer, Axis::Vertical),
        || {
            column_with(Modifier::new().offset(0.0, -outer_position.get()), || {
                text_with(Modifier::new().padding(10.0), "Outer list, first row");
                stack_with(
                    Modifier::new()
                        .width(240.0)
                        .height(80.0)
                        .padding_xy(10.0, 0.0)
                        .background_rounded(theme.surface, 8.0)
                        .clip()
                        .scrollable_nested(&inner, Axis::Vertical, &outer.connection()),
                    || {
                        column_with(Modifier::new().offset(0.0, -inner_position.get()), || {
                            for row in 1..=8 {
                                text_with(Modifier::new().padding(8.0), format!("Inner row {row}"));
                            }
                        });
                    },
                );
                for row in 2..=8 {
                    text_with(Modifier::new().padding(10.0), format!("Outer row {row}"));
                }
            });
        },
    );
    muted(&format!(
        "Inner {:.0}, outer {:.0}",
        inner_position.get(),
        outer_position.get()
    ));
}

/// Shows `draggable`, `anchored_draggable`, `transformable` and nested `scrollable`.
#[composable]
pub(super) fn gestures() {
    panel(
        "Draggable",
        "Drag the square along its track; the offset and the release speed are state.",
        draggable_box,
    );
    panel(
        "Anchored draggable",
        "Let go of the knob and it settles on the nearest stop, or the next one after a flick.",
        anchored_track,
    );
    panel(
        "Transformable",
        "One pointer pans; two zoom and turn about their midpoint. Ctrl and the wheel zoom.",
        transform_area,
    );
    panel(
        "Nested scroll",
        "Scroll the inner list; once it reaches its end the outer list takes over, flings included.",
        nested_lists,
    );
}

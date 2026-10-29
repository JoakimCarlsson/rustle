//! An accordion that animates open and closed.

use rustle::prelude::*;

use crate::ui::panel;

#[composable]
pub(super) fn accordion_item(
    heading: &str,
    body: &str,
    open: bool,
    on_toggle: impl Fn() + 'static,
) {
    let theme = theme();
    let turn = animate_float_as_state(if open { 180.0 } else { 0.0 }, tween(220)).get();
    column_with(Modifier::new().fill_max_width(), || {
        row_arranged(
            Modifier::new()
                .fill_max_width()
                .semantics(
                    Semantics::new(Role::Button)
                        .label(heading.to_string())
                        .toggled(open)
                        .merging_descendants(),
                )
                .clickable(on_toggle)
                .padding_xy(0.0, 12.0),
            Arrangement::SpaceBetween,
            Alignment::Center,
            || {
                title(heading, 14.0);
                stack_aligned(
                    Modifier::new().size(16.0, 16.0).rotate(turn),
                    BoxAlignment::Center,
                    || icon(phosphor::CARET_DOWN, 16.0, theme.on_surface_muted),
                );
            },
        );
        animated_visibility(open, || {
            column_with(
                Modifier::new()
                    .fill_max_width()
                    .padding_each(0.0, 0.0, 0.0, 12.0),
                || muted_sized(body, 13.0),
            );
        });
        spacer(
            Modifier::new()
                .fill_max_width()
                .height(1.0)
                .background(theme.outline),
        );
    });
}

#[composable]
pub(super) fn accordion() {
    let open = remember(|| Some(0_usize));
    panel(
        "Accordion",
        "One section open at a time; the chevron turns as it opens.",
        || {
            let items = [
                (
                    "Is it accessible?",
                    "Yes. Every control carries a role and a label.",
                ),
                (
                    "Is it animated?",
                    "Yes. Sections expand and fade, and the chevron turns.",
                ),
                (
                    "Is it fast?",
                    "Only what changed is composed again, and the rest is skipped.",
                ),
            ];
            for (index, (heading, body)) in items.into_iter().enumerate() {
                key(index, || {
                    let state = open.clone();
                    accordion_item(heading, body, open.get() == Some(index), move || {
                        let next = if state.get() == Some(index) {
                            None
                        } else {
                            Some(index)
                        };
                        state.set(next);
                    });
                });
            }
        },
    );
}

//! A list whose rows swipe away to dismiss.

use rustle::prelude::*;

use super::replay_button;
use crate::ctx::Ctx;
use crate::ui::panel;

#[composable]
pub(super) fn swipe_list(ctx: &Ctx) {
    let theme = color_scheme();
    let start = || {
        vec![
            (0_usize, "Eddie Lake commented on Design"),
            (1, "Cover page is ready for review"),
            (2, "Weekly report was generated"),
            (3, "Sofia Davis joined the team"),
        ]
    };
    let notes = remember_state_list(start);
    panel(
        "Swipe to dismiss",
        "Drag a row sideways; past a third of its width it goes.",
        || {
            if notes.is_empty() {
                stack_aligned(
                    Modifier::new().fill_max_width().height(80.0),
                    BoxAlignment::Center,
                    || muted_sized("All caught up", 13.0),
                );
            }
            for (id, message) in notes.to_vec() {
                key(id, || {
                    let (notes, ctx) = (notes.clone(), ctx.clone());
                    swipe_to_dismiss(
                        move || {
                            notes.update(|list| list.retain(|(other, _)| *other != id));
                            ctx.say("Dismissed");
                        },
                        || {
                            row_arranged(
                                Modifier::new()
                                    .fill_max_width()
                                    .background_rounded(theme.secondary_container, 10.0)
                                    .border(1.0, theme.outline, 10.0)
                                    .padding_xy(14.0, 12.0),
                                Arrangement::SpacedBy(10.0),
                                Alignment::Center,
                                || {
                                    icon(phosphor::BELL, 15.0, theme.on_surface_variant);
                                    text(message.to_string());
                                },
                            );
                        },
                    );
                    spacer(Modifier::new().height(8.0));
                });
            }
            let again = notes.clone();
            replay_button("Restore", move || again.update(|list| *list = start()));
        },
    );
}

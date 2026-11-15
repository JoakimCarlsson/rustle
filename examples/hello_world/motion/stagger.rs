//! Staggered entrances that can be replayed.

use std::time::Duration;

use rustle::prelude::*;

use super::{linear, replay_button};
use crate::ui::panel;

#[composable]
pub(super) fn stagger_row(index: usize, run: u64, glyph: Icon, label: &str, detail: &str) {
    let theme = color_scheme();
    let progress = remember_animatable(0.0_f32);
    let driven = progress.clone();
    launched_effect(run, move |scope| async move {
        driven.snap_to(0.0);
        scope.delay(Duration::from_millis(index as u64 * 90)).await;
        driven
            .animate_to(&scope, 1.0, linear(520, Easing::EaseOut))
            .await;
    });
    let shown = progress.value();
    row_arranged(
        Modifier::new()
            .fill_max_width()
            .alpha(shown)
            .offset((1.0 - shown) * 32.0, 0.0)
            .background_rounded(theme.secondary_container, 10.0)
            .border(1.0, theme.outline, 10.0)
            .padding_xy(12.0, 10.0),
        Arrangement::SpacedBy(12.0),
        Alignment::Center,
        || {
            stack_aligned(
                Modifier::new()
                    .size(32.0, 32.0)
                    .background_rounded(theme.surface_variant, 8.0),
                BoxAlignment::Center,
                || icon(glyph, 15.0, theme.on_surface),
            );
            column(|| {
                title(label, 13.0);
                muted_sized(detail, 12.0);
            });
        },
    );
    spacer(Modifier::new().height(8.0));
}

#[composable]
pub(super) fn stagger() {
    let run = remember(|| 0_u64);
    panel(
        "Staggered entrance",
        "Each row waits a beat longer, then slides and fades in.",
        || {
            let rows = [
                (phosphor::FOLDER, "Projects", "4 in flight"),
                (phosphor::USERS, "Team", "3 members online"),
                (phosphor::CHART_BAR, "Reports", "Weekly summary is ready"),
                (phosphor::BELL, "Alerts", "Nothing needs you"),
                (phosphor::GEAR, "Settings", "All synced"),
            ];
            for (index, (glyph, label, detail)) in rows.into_iter().enumerate() {
                key(index, || {
                    stagger_row(index, run.get(), glyph, label, detail)
                });
            }
            let again = run.clone();
            replay_button("Replay", move || again.update(|n| *n += 1));
        },
    );
}

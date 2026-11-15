//! Loading indicators and skeletons.

use std::f32::consts::TAU;
use std::time::Duration;

use rustle::prelude::*;

use super::{clock, linear};
use crate::ui::panel;

#[composable]
pub(super) fn loader_cell(label: &str, content: impl FnOnce()) {
    column_arranged(
        Modifier::new().weight(1.0),
        Arrangement::SpacedBy(8.0),
        Alignment::Center,
        || {
            stack_aligned(
                Modifier::new().size(44.0, 36.0),
                BoxAlignment::Center,
                content,
            );
            muted_sized(label, 11.0);
        },
    );
}

#[composable]
pub(super) fn bouncing_dots() {
    let theme = color_scheme();
    let t = clock(1.1).get();
    row_arranged(
        Modifier::new(),
        Arrangement::SpacedBy(5.0),
        Alignment::Center,
        || {
            for i in 0..3 {
                let lift = (t * TAU - i as f32 * 0.9).sin().max(0.0);
                spacer(
                    Modifier::new()
                        .offset(0.0, -lift * 9.0)
                        .size(8.0, 8.0)
                        .background_rounded(theme.primary, 4.0),
                );
            }
        },
    );
}

#[composable]
pub(super) fn pulse() {
    let theme = color_scheme();
    let t = infinite_float(0.0, 1.0, Duration::from_millis(900), Easing::EaseInOut).get();
    stack_aligned(
        Modifier::new().size(30.0, 30.0),
        BoxAlignment::Center,
        || {
            spacer(
                Modifier::new()
                    .size(30.0, 30.0)
                    .alpha(1.0 - t)
                    .scale(0.4 + t * 0.6)
                    .background_rounded(theme.primary, 15.0),
            );
            spacer(
                Modifier::new()
                    .size(10.0, 10.0)
                    .background_rounded(theme.primary, 5.0),
            );
        },
    );
}

#[composable]
pub(super) fn mini_bars() {
    let theme = color_scheme();
    let t = clock(1.0).get();
    row_arranged(
        Modifier::new().height(28.0),
        Arrangement::SpacedBy(3.0),
        Alignment::Center,
        || {
            for i in 0..5 {
                let height = 8.0 + ((t * TAU + i as f32 * 0.8).sin() * 0.5 + 0.5) * 20.0;
                spacer(
                    Modifier::new()
                        .size(4.0, height)
                        .background_rounded(theme.primary, 2.0),
                );
            }
        },
    );
}

#[composable]
pub(super) fn sweep_bar() {
    let theme = color_scheme();
    let t = clock(1.5).get();
    let (track, fill) = (theme.surface_variant, theme.primary);
    canvas(
        Modifier::new().fill_max_width().height(6.0),
        move |canvas| {
            let width = canvas.width();
            canvas.rounded_rect(0.0, 0.0, width, 6.0, 3.0, track);
            let length = width * 0.35;
            let x = -length + t * (width + length);
            canvas.push_clip(0.0, 0.0, width, 6.0);
            canvas.rounded_rect(x, 0.0, length, 6.0, 3.0, fill);
            canvas.pop_clip();
        },
    );
}

#[composable]
pub(super) fn skeleton() {
    let theme = color_scheme();
    let strength = infinite_float(0.35, 1.0, Duration::from_millis(900), Easing::EaseInOut).get();
    let shade = Color::rgba(
        theme.on_surface_variant.r,
        theme.on_surface_variant.g,
        theme.on_surface_variant.b,
        (strength * 60.0) as u8,
    );
    row_arranged(
        Modifier::new().fill_max_width(),
        Arrangement::SpacedBy(12.0),
        Alignment::Center,
        || {
            spacer(
                Modifier::new()
                    .size(40.0, 40.0)
                    .background_rounded(shade, 20.0),
            );
            column_with(Modifier::new().weight(1.0), || {
                spacer(
                    Modifier::new()
                        .fill_max_width()
                        .height(12.0)
                        .background_rounded(shade, 6.0),
                );
                spacer(Modifier::new().height(8.0));
                spacer(
                    Modifier::new()
                        .width(160.0)
                        .height(12.0)
                        .background_rounded(shade, 6.0),
                );
            });
        },
    );
}

#[composable]
pub(super) fn loaders() {
    let theme = color_scheme();
    let target = remember(|| 0.25_f32);
    let progress = animate_float_as_state(target.get(), linear(800, Easing::EaseInOut)).get();
    panel(
        "Loaders",
        "Spinners, pulses and skeletons that never stop moving.",
        || {
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpaceBetween,
                Alignment::Start,
                || {
                    loader_cell("Spinner", || {
                        circular_progress_indicator_indeterminate(28.0)
                    });
                    loader_cell("Dots", bouncing_dots);
                    loader_cell("Pulse", pulse);
                    loader_cell("Level", mini_bars);
                },
            );
            spacer(Modifier::new().height(16.0));
            sweep_bar();
            spacer(Modifier::new().height(18.0));
            skeleton();
            spacer(Modifier::new().height(18.0));
            row_arranged(
                Modifier::new().fill_max_width(),
                Arrangement::SpacedBy(16.0),
                Alignment::Center,
                || {
                    stack_aligned(
                        Modifier::new().size(84.0, 84.0),
                        BoxAlignment::Center,
                        || {
                            spacer(Modifier::new().size(84.0, 84.0).ring(
                                theme.surface_variant,
                                8.0,
                                0.0,
                                1.0,
                            ));
                            spacer(Modifier::new().size(84.0, 84.0).ring(
                                theme.primary,
                                8.0,
                                0.0,
                                progress,
                            ));
                            title(&format!("{:.0}%", progress * 100.0), 16.0);
                        },
                    );
                    column_with(Modifier::new().weight(1.0), || {
                        muted_sized("Determinate ring, animated to each target.", 12.0);
                        spacer(Modifier::new().height(8.0));
                        row_arranged(
                            Modifier::new(),
                            Arrangement::SpacedBy(6.0),
                            Alignment::Center,
                            || {
                                for (label, value) in [("25%", 0.25), ("60%", 0.6), ("100%", 1.0)] {
                                    key(label, || {
                                        let target = target.clone();
                                        styled_button(
                                            Modifier::new(),
                                            ButtonStyle::Secondary,
                                            label,
                                            None,
                                            None,
                                            move || target.set(value),
                                        );
                                    });
                                }
                            },
                        );
                    });
                },
            );
        },
    );
}

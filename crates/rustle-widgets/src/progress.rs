//! Linear and circular progress indicators.

use std::time::Duration;

use rustle_animation::{
    Easing, animate_float_as_state, infinite_float, infinite_float_repeating, tween,
};
use rustle_foundation::{
    BoxAlignment, Modifier, Role, Semantics, spacer, stack_aligned, stack_with,
};

use crate::theme::color_scheme;

/// Emits a progress bar `width` wide filled to `progress`, from 0 to 1.
#[track_caller]
pub fn linear_progress_indicator(width: f32, progress: f32) {
    let theme = color_scheme();
    let fill = animate_float_as_state(progress.clamp(0.0, 1.0) * width, tween(200));
    stack_aligned(
        Modifier::new()
            .size(width, 8.0)
            .semantics(Semantics::new(Role::ProgressBar).value(format!("{:.0}%", progress * 100.0)))
            .background_rounded(theme.surface_variant, 4.0),
        BoxAlignment::CenterStart,
        || {
            spacer(
                Modifier::new()
                    .width(fill.get())
                    .height(8.0)
                    .background_rounded(theme.primary, 4.0),
            );
        },
    );
}

/// Emits a circle `size` wide that is filled around to `progress`, from 0 to 1.
#[track_caller]
pub fn circular_progress_indicator(size: f32, progress: f32) {
    let theme = color_scheme();
    let thickness = (size / 8.0).max(3.0);
    stack_with(
        Modifier::new()
            .size(size, size)
            .semantics(Semantics::new(Role::ProgressBar).value(format!("{:.0}%", progress * 100.0)))
            // The first ring is drawn last, so the filled part is on top of the track.
            .ring(theme.primary, thickness, 0.0, progress)
            .ring(theme.outline, thickness, 0.0, 1.0),
        || {},
    );
}

/// Emits a spinning circle `size` wide, for work of unknown length.
#[track_caller]
pub fn circular_progress_indicator_indeterminate(size: f32) {
    let theme = color_scheme();
    let thickness = (size / 8.0).max(3.0);
    let spin = infinite_float_repeating(0.0, 360.0, Duration::from_millis(1100), Easing::Linear);
    let grow = infinite_float(0.12, 0.7, Duration::from_millis(700), Easing::EaseInOut);
    stack_with(
        Modifier::new()
            .size(size, size)
            .semantics(Semantics::new(Role::ProgressBar))
            .ring(theme.primary, thickness, spin.get(), grow.get()),
        || {},
    );
}

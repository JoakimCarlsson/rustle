//! Linear and circular progress indicators.

use std::time::Duration;

use rustle_animation::{
    AnimationSpec, Easing, RepeatMode, animate_float_as_state, infinite_float,
    infinite_float_repeating, infinite_repeatable, remember_infinite_transition, tween,
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

/// The default look of progress indicators: Compose's `ProgressIndicatorDefaults`.
pub struct ProgressIndicatorDefaults;

impl ProgressIndicatorDefaults {
    /// How wide a linear indicator is unless its modifier says: Compose's 240 dp.
    pub const LINEAR_INDICATOR_WIDTH: f32 = 240.0;

    /// How tall a linear indicator is unless its modifier says: Compose's 4 dp.
    pub const LINEAR_INDICATOR_HEIGHT: f32 = 4.0;

    /// The color of the filled part: the theme's primary, as `linearColor`.
    pub fn linear_color() -> rustle_graphics::Color {
        color_scheme().primary
    }

    /// The color of the track: the theme's surface variant, as `linearTrackColor`.
    pub fn linear_track_color() -> rustle_graphics::Color {
        color_scheme().surface_variant
    }

    /// How thick a circular indicator's ring is: Compose's `CircularStrokeWidth`, 4 dp.
    pub const CIRCULAR_STROKE_WIDTH: f32 = 4.0;

    /// How wide a circular indicator is unless its modifier says: Compose's 40 dp.
    pub const CIRCULAR_INDICATOR_DIAMETER: f32 = 40.0;

    /// The color of a circular indicator's ring: the theme's primary, as `circularColor`.
    pub fn circular_color() -> rustle_graphics::Color {
        color_scheme().primary
    }

    /// The track under an indeterminate circular indicator: none, as Compose's
    /// `circularIndeterminateTrackColor`.
    pub fn circular_indeterminate_track_color() -> rustle_graphics::Color {
        rustle_graphics::Color::TRANSPARENT
    }

    /// The track under a determinate circular indicator: the theme's surface variant, as
    /// `circularDeterminateTrackColor`.
    pub fn circular_determinate_track_color() -> rustle_graphics::Color {
        color_scheme().surface_variant
    }
}

/// The ring a circular indicator draws in a canvas of `width` by `height`: a track all round in
/// `track_color`, then `sweep` of the circle (0 to 1) from `start` degrees in `color`.
fn draw_ring(
    canvas: &mut rustle_foundation::Canvas,
    stroke_width: f32,
    track_color: rustle_graphics::Color,
    color: rustle_graphics::Color,
    start: f32,
    sweep: f32,
) {
    let side = canvas.width().min(canvas.height());
    let (x, y) = (canvas.width() / 2.0, canvas.height() / 2.0);
    let radius = ((side - stroke_width) / 2.0).max(0.0);
    if track_color.a > 0 {
        canvas.arc(x, y, radius, stroke_width, 0.0, 1.0, track_color);
    }
    if sweep > 0.0 {
        canvas.arc(x, y, radius, stroke_width, start, sweep, color);
    }
}

/// Emits an indeterminate circular progress indicator, Compose's
/// `CircularProgressIndicator(modifier, color, strokeWidth, trackColor)`: an arc in `color`,
/// `stroke_width` thick over a ring in `track_color`, turning and growing and shrinking for as
/// long as it is in the composition. Its angles are read when it is drawn, so it turns without
/// composing again. Its size is what `modifier` says,
/// [`ProgressIndicatorDefaults::CIRCULAR_INDICATOR_DIAMETER`] on a side where it says nothing.
///
/// For a spinner of another look, such as an icon that turns, animate an angle with
/// `remember_infinite_transition` and draw it with `Modifier::rotate_with`.
#[track_caller]
pub fn circular_progress_indicator_with(
    modifier: Modifier,
    color: rustle_graphics::Color,
    stroke_width: f32,
    track_color: rustle_graphics::Color,
) {
    let transition = remember_infinite_transition();
    let linear = |millis: u64, mode: RepeatMode| {
        infinite_repeatable(
            AnimationSpec::Tween {
                duration: Duration::from_millis(millis),
                easing: Easing::Linear,
            },
            mode,
        )
    };
    let turn = transition.animate_float(0.0, 360.0, linear(1100, RepeatMode::Restart));
    let grow = transition.animate_float(0.1, 0.75, linear(700, RepeatMode::Reverse));
    rustle_foundation::canvas(
        modifier.semantics(Semantics::new(Role::ProgressBar)).size(
            ProgressIndicatorDefaults::CIRCULAR_INDICATOR_DIAMETER,
            ProgressIndicatorDefaults::CIRCULAR_INDICATOR_DIAMETER,
        ),
        move |canvas| {
            draw_ring(
                canvas,
                stroke_width,
                track_color,
                color,
                turn.get(),
                grow.get(),
            );
        },
    );
}

/// Emits a determinate circular progress indicator, Compose's
/// `CircularProgressIndicator(progress, modifier, color, strokeWidth, trackColor)`: a ring in
/// `track_color` with the share `progress` reads, from 0 to 1, filled clockwise from the top
/// in `color`. `progress` is read when the indicator is drawn. Its size is what `modifier`
/// says, [`ProgressIndicatorDefaults::CIRCULAR_INDICATOR_DIAMETER`] on a side where it says
/// nothing.
#[track_caller]
pub fn circular_progress_indicator_with_progress(
    progress: impl Fn() -> f32 + 'static,
    modifier: Modifier,
    color: rustle_graphics::Color,
    stroke_width: f32,
    track_color: rustle_graphics::Color,
) {
    rustle_foundation::canvas(
        modifier.semantics(Semantics::new(Role::ProgressBar)).size(
            ProgressIndicatorDefaults::CIRCULAR_INDICATOR_DIAMETER,
            ProgressIndicatorDefaults::CIRCULAR_INDICATOR_DIAMETER,
        ),
        move |canvas| {
            let share = progress();
            let share = if share.is_finite() {
                share.clamp(0.0, 1.0)
            } else {
                0.0
            };
            draw_ring(canvas, stroke_width, track_color, color, 0.0, share);
        },
    );
}

/// Emits a determinate linear progress indicator as Compose's `LinearProgressIndicator(progress,
/// modifier, color, trackColor)` with round stroke caps and no gap or stop indicator: a track
/// in `track_color` with the share `progress` reads, from 0 to 1, filled from the start in
/// `color`. `progress` is read when the indicator is drawn. Its size is what `modifier` says,
/// [`ProgressIndicatorDefaults::LINEAR_INDICATOR_WIDTH`] by
/// [`ProgressIndicatorDefaults::LINEAR_INDICATOR_HEIGHT`] where it says nothing.
#[track_caller]
pub fn linear_progress_indicator_with(
    progress: impl Fn() -> f32 + 'static,
    modifier: Modifier,
    color: rustle_graphics::Color,
    track_color: rustle_graphics::Color,
) {
    rustle_foundation::canvas(
        modifier.semantics(Semantics::new(Role::ProgressBar)).size(
            ProgressIndicatorDefaults::LINEAR_INDICATOR_WIDTH,
            ProgressIndicatorDefaults::LINEAR_INDICATOR_HEIGHT,
        ),
        move |canvas| {
            let (width, height) = (canvas.width(), canvas.size().height);
            let radius = height / 2.0;
            canvas.rounded_rect(0.0, 0.0, width, height, radius, track_color);
            let share = progress();
            let share = if share.is_finite() {
                share.clamp(0.0, 1.0)
            } else {
                0.0
            };
            if share > 0.0 {
                canvas.rounded_rect(0.0, 0.0, width * share, height, radius, color);
            }
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

/// Emits concentric rings in the largest square `modifier` gives, the first outermost, each
/// `width` pixels thick and `gap` apart. Each ring is filled clockwise from the top to its
/// share, from 0 to 1, in its color, and the rest of it is drawn in `track`. Rings that no
/// longer fit are left out. One ring is a circular gauge; several show related quantities,
/// such as how much of each limit has been used.
#[track_caller]
pub fn progress_rings(
    modifier: Modifier,
    width: f32,
    gap: f32,
    track: rustle_graphics::Color,
    filled: &[(f32, rustle_graphics::Color)],
) {
    let filled: Vec<(f32, rustle_graphics::Color)> = filled
        .iter()
        .map(|(share, color)| (share.clamp(0.0, 1.0), *color))
        .collect();
    rustle_foundation::canvas(modifier, move |canvas| {
        let side = canvas.width().min(canvas.height());
        let (x, y) = (canvas.width() / 2.0, canvas.height() / 2.0);
        for (index, (share, color)) in filled.iter().enumerate() {
            let radius = side / 2.0 - index as f32 * (width + gap);
            if radius - width < 0.0 {
                break;
            }
            canvas.stroke_circle(x, y, radius, width, track);
            if *share > 0.0 {
                canvas.arc(x, y, radius, width, 0.0, *share, *color);
            }
        }
    });
}

/// How many dashes the ring of [`dashed_spinner`] is broken into.
const SPINNER_DASHES: usize = 8;

/// How much of the ring each dash of [`dashed_spinner`] covers, as a share of the whole ring.
const SPINNER_DASH_SHARE: f32 = 0.07;

/// Emits a ring of short dashes `side` pixels across in `color`, turned `degrees` clockwise:
/// a spinner whose caller turns it from a clock of its own, such as one a snapshot holds
/// still, read when the ring is painted so it can turn without composing again.
#[track_caller]
pub fn dashed_spinner(
    side: f32,
    color: rustle_graphics::Color,
    degrees: impl Fn() -> f32 + 'static,
) {
    rustle_foundation::canvas(Modifier::new().size(side, side), move |canvas| {
        let turned = degrees();
        let thickness = (side * 16.0 / 256.0).max(1.0);
        let radius = side * 96.0 / 256.0;
        let (x, y) = (side / 2.0, side / 2.0);
        for dash in 0..SPINNER_DASHES {
            let start = turned + dash as f32 * 360.0 / SPINNER_DASHES as f32;
            canvas.arc(x, y, radius, thickness, start, SPINNER_DASH_SHARE, color);
        }
    });
}

#[cfg(test)]
mod tests;

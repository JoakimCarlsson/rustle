use std::time::Duration;

use crate::easing::*;

/// How a value travels to its target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimationSpec {
    /// A fixed duration, shaped by an easing curve.
    Tween { duration: Duration, easing: Easing },
    /// A damped spring. `damping_ratio` below 1 overshoots and bounces; 1 is
    /// the quickest without overshoot. A higher `stiffness` is faster.
    Spring { damping_ratio: f32, stiffness: f32 },
    /// Jumps to the target on the first frame.
    Snap,
    /// Passes through `frames` over `duration`. Progress 0 is the start value
    /// and 1 the target, and a frame may go beyond either to overshoot.
    /// Without a frame at 0 or 1, the animation starts at 0 and ends at 1.
    Keyframes {
        duration: Duration,
        frames: &'static [Keyframe],
    },
    /// Runs a tween `iterations` times, or for ever when `iterations` is
    /// `None`, restarting or reversing as `mode` says.
    Repeat {
        iterations: Option<u32>,
        mode: RepeatMode,
        duration: Duration,
        easing: Easing,
    },
}

/// What a repeating animation does at the end of each run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepeatMode {
    /// Jumps back to the start and runs again.
    #[default]
    Restart,
    /// Runs backwards, so it goes back and forth.
    Reverse,
}

/// One stop of a [`AnimationSpec::Keyframes`] animation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe {
    /// How far through the duration this stop is, from 0 to 1.
    pub at: f32,
    /// How far from the start value to the target the animation is here.
    pub progress: f32,
    /// Shapes the way from the previous stop to this one.
    pub easing: Easing,
}

impl Keyframe {
    /// A stop at `at` of the duration, at `progress` of the way to the target,
    /// reached linearly.
    pub const fn new(at: f32, progress: f32) -> Self {
        Self {
            at,
            progress,
            easing: Easing::Linear,
        }
    }

    /// The same stop, reached along `easing`.
    pub const fn with_easing(self, easing: Easing) -> Self {
        Self { easing, ..self }
    }
}

/// Keyframes over `milliseconds`, stops ordered by `at`.
pub const fn keyframes(milliseconds: u64, frames: &'static [Keyframe]) -> AnimationSpec {
    AnimationSpec::Keyframes {
        duration: Duration::from_millis(milliseconds),
        frames,
    }
}

/// An animation that arrives at once.
pub const fn snap() -> AnimationSpec {
    AnimationSpec::Snap
}

/// Runs `animation`, a tween, `iterations` times. Any other kind of spec is
/// returned as it is.
pub fn repeatable(iterations: u32, animation: AnimationSpec, mode: RepeatMode) -> AnimationSpec {
    repeat_of(Some(iterations.max(1)), animation, mode)
}

/// Runs `animation`, a tween, for ever. Any other kind of spec is returned as it is.
pub fn infinite_repeatable(animation: AnimationSpec, mode: RepeatMode) -> AnimationSpec {
    repeat_of(None, animation, mode)
}

/// Builds a repeating spec from a tween.
fn repeat_of(iterations: Option<u32>, animation: AnimationSpec, mode: RepeatMode) -> AnimationSpec {
    match animation {
        AnimationSpec::Tween { duration, easing } => AnimationSpec::Repeat {
            iterations,
            mode,
            duration,
            easing,
        },
        other => other,
    }
}

/// A tween of `milliseconds` with the standard easing.
pub fn tween(milliseconds: u64) -> AnimationSpec {
    AnimationSpec::Tween {
        duration: Duration::from_millis(milliseconds),
        easing: Easing::FAST_OUT_SLOW_IN,
    }
}

/// A spring that bounces a little.
pub fn spring() -> AnimationSpec {
    AnimationSpec::Spring {
        damping_ratio: 0.6,
        stiffness: 400.0,
    }
}

impl Default for AnimationSpec {
    fn default() -> Self {
        tween(300)
    }
}

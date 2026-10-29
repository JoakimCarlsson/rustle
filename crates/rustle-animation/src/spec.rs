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

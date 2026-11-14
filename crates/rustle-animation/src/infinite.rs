use std::time::Duration;

use rustle_runtime::{State, launched_effect};

use crate::animatable::*;
use crate::easing::*;
use crate::spec::*;

/// Returns a float that runs from `from` to `to` in `duration`, then jumps back and
/// runs again, for as long as the call stays in the composition. For spinning things.
#[track_caller]
pub fn infinite_float_repeating(
    from: f32,
    to: f32,
    duration: Duration,
    easing: Easing,
) -> State<f32> {
    let animatable = remember_animatable(from);
    let driven = animatable.clone();
    launched_effect((), move |scope| async move {
        let tween = AnimationSpec::Tween { duration, easing };
        let spec = infinite_repeatable(tween, RepeatMode::Restart);
        driven.animate_to(&scope, to, spec).await;
    });
    animatable.state().clone()
}

/// Returns a float that runs from `from` to `to` and back, over and over, for
/// as long as the call stays in the composition. Each way takes `duration`.
#[track_caller]
pub fn infinite_float(from: f32, to: f32, duration: Duration, easing: Easing) -> State<f32> {
    let animatable = remember_animatable(from);
    let driven = animatable.clone();
    launched_effect((), move |scope| async move {
        let tween = AnimationSpec::Tween { duration, easing };
        let spec = infinite_repeatable(tween, RepeatMode::Reverse);
        driven.animate_to(&scope, to, spec).await;
    });
    animatable.state().clone()
}

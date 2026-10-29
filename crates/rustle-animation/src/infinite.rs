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
        let spec = AnimationSpec::Tween { duration, easing };
        loop {
            driven.snap_to(from);
            driven.animate_to(&scope, to, spec).await;
        }
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
        let spec = AnimationSpec::Tween { duration, easing };
        loop {
            driven.animate_to(&scope, to, spec).await;
            driven.animate_to(&scope, from, spec).await;
        }
    });
    animatable.state().clone()
}

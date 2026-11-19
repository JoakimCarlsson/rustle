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

/// Values that animate for as long as they stay in the composition: Compose's
/// `InfiniteTransition`, made with [`remember_infinite_transition`].
///
/// Each value is given an [`infinite_repeatable`] spec, so it never settles:
///
/// ```ignore
/// let transition = remember_infinite_transition();
/// let angle = transition.animate_float(
///     0.0,
///     360.0,
///     infinite_repeatable(AnimationSpec::Tween { duration: Duration::from_secs(1), easing: Easing::Linear }, RepeatMode::Restart),
/// );
/// icon(Modifier::new().rotate_with(move || angle.get()), ...);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InfiniteTransition {
    _private: (),
}

/// An infinite transition for the calling composable: Compose's `rememberInfiniteTransition`.
#[track_caller]
pub fn remember_infinite_transition() -> InfiniteTransition {
    InfiniteTransition::default()
}

impl InfiniteTransition {
    /// A float that starts at `initial_value` and runs toward `target_value` as
    /// `animation_spec` says, for as long as the call stays in the composition: Compose's
    /// `InfiniteTransition.animateFloat`. Give it an [`infinite_repeatable`] spec; any other
    /// settles at the target. Read the state while drawing (`rotate_with`, a canvas) to
    /// animate without composing again.
    #[track_caller]
    pub fn animate_float(
        &self,
        initial_value: f32,
        target_value: f32,
        animation_spec: AnimationSpec,
    ) -> State<f32> {
        let animatable = remember_animatable(initial_value);
        let driven = animatable.clone();
        launched_effect((), move |scope| async move {
            driven
                .animate_to(&scope, target_value, animation_spec)
                .await;
        });
        animatable.state().clone()
    }
}

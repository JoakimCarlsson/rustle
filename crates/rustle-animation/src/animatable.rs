use rustle_runtime::{State, TaskScope, remember};

use crate::lerp::*;
use crate::spec::*;
use crate::state::*;

/// A value that tasks animate with [`Animatable::animate_to`].
///
/// Where [`animate_as_state`] follows a target, an `Animatable` is driven
/// step by step from a [`launched_effect`](rustle_runtime::launched_effect),
/// so animations can be sequenced and mixed with other waiting.
#[derive(Clone)]
pub struct Animatable<T> {
    value: State<T>,
}

impl<T: Lerp + Clone + 'static> Animatable<T> {
    /// The state holding the current value, to read while composing.
    pub fn state(&self) -> &State<T> {
        &self.value
    }

    pub fn value(&self) -> T {
        self.value.get()
    }

    /// Jumps to `value` without animating.
    pub fn snap_to(&self, value: T) {
        self.value.set(value);
    }

    /// Moves to `target` as `spec` says, finishing when it arrives. Dropping
    /// the future, which cancelling its task does, stops it where it is.
    pub async fn animate_to(&self, scope: &TaskScope, target: T, spec: AnimationSpec) {
        let from = self.value.get();
        let mut stepper = Stepper::new(spec);
        loop {
            let now = scope.next_frame().await;
            let done = stepper.step(now);
            self.value.set(if done {
                target.clone()
            } else {
                from.lerp(&target, stepper.progress)
            });
            if done {
                break;
            }
        }
    }
}

/// Remembers an [`Animatable`] at this call site, starting at `initial`.
#[track_caller]
pub fn remember_animatable<T: Clone + 'static>(initial: T) -> Animatable<T> {
    Animatable {
        value: remember(|| initial),
    }
}

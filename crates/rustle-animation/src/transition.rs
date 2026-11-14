use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rustle_graphics::Color;
use rustle_runtime::{State, remember};

use crate::lerp::*;
use crate::spec::*;
use crate::state::*;

/// Several animations that follow one target state, made by [`update_transition`].
///
/// Each value is described by a function of the state, and moves whenever the
/// target changes, so a whole look (color, size, offset) changes together.
#[derive(Clone)]
pub struct Transition<S> {
    from: S,
    target: S,
    active: Rc<Cell<usize>>,
}

impl<S: Clone + PartialEq + 'static> Transition<S> {
    /// The state the transition is heading for.
    pub fn target_state(&self) -> &S {
        &self.target
    }

    /// The state the transition left when the target last changed. It equals
    /// the target until the first change.
    pub fn current_state(&self) -> &S {
        &self.from
    }

    /// Whether the target changed and the values are still on their way to it.
    pub fn is_changing(&self) -> bool {
        self.from != self.target
    }

    /// Returns a value that follows `value(target)`, moving as `spec` says
    /// when the target state changes. `spec` is given the state moved from and
    /// the state moved to.
    #[track_caller]
    pub fn animate_value<T: Lerp + Clone + PartialEq + 'static>(
        &self,
        spec: impl Fn(&S, &S) -> AnimationSpec,
        value: impl Fn(&S) -> T,
    ) -> State<T> {
        animate_counted(
            value(&self.target),
            spec(&self.from, &self.target),
            Some(Rc::clone(&self.active)),
        )
    }

    /// [`Transition::animate_value`] for a float.
    #[track_caller]
    pub fn animate_float(
        &self,
        spec: impl Fn(&S, &S) -> AnimationSpec,
        value: impl Fn(&S) -> f32,
    ) -> State<f32> {
        self.animate_value(spec, value)
    }

    /// [`Transition::animate_value`] for a color.
    #[track_caller]
    pub fn animate_color(
        &self,
        spec: impl Fn(&S, &S) -> AnimationSpec,
        value: impl Fn(&S) -> Color,
    ) -> State<Color> {
        self.animate_value(spec, value)
    }
}

/// Starts a transition at this call site that follows `target`. Animate the
/// values that depend on the state with [`Transition::animate_float`] and its
/// siblings, each on its own spec.
#[track_caller]
pub fn update_transition<S: Clone + PartialEq + 'static>(target: S) -> Transition<S> {
    let (pair, active) = remember({
        let target = target.clone();
        move || {
            (
                Rc::new(RefCell::new((target.clone(), target))),
                Rc::new(Cell::new(0)),
            )
        }
    })
    .get();
    let mut pair = pair.borrow_mut();
    if pair.1 != target {
        pair.0 = std::mem::replace(&mut pair.1, target);
    } else if active.get() == 0 && pair.0 != pair.1 {
        pair.0 = pair.1.clone();
    }
    Transition {
        from: pair.0.clone(),
        target: pair.1.clone(),
        active,
    }
}

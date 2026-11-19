//! Reads of state made while a host lays out or draws what a composition emitted, so a change
//! to state read only there lays out or draws again without composing.
//!
//! As in Compose, where state is read decides what a change to it costs: state read while
//! composing recomposes what read it, state read only while measuring (a scroll offset read
//! by a layout closure) lays out again, and state read only while drawing (the angle of a
//! spinner read by a drawing closure) draws again. A host runs its measuring and its drawing
//! through a [`PhaseObserver`], then asks it after state changed what is to be done again.

use std::cell::RefCell;
use std::rc::Rc;

use crate::state::Invalidation;

/// What a host does with the tree a composition emitted, outside composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Measuring and placing it.
    Layout,
    /// Turning it into drawing commands.
    Drawing,
}

thread_local! {
    /// The phase running now and the composition it belongs to, innermost last.
    static OBSERVING: RefCell<Vec<(Phase, Rc<Invalidation>)>> = const { RefCell::new(Vec::new()) };
}

/// The phase running now outside composition, with the invalidation of its composition.
pub(crate) fn observing() -> Option<(Phase, Rc<Invalidation>)> {
    OBSERVING.with(|stack| {
        stack
            .borrow()
            .last()
            .map(|(phase, invalidation)| (*phase, Rc::clone(invalidation)))
    })
}

/// Pops the phase pushed by [`PhaseObserver::run`] again, also when it panics.
struct PhaseGuard {
    /// How many phases were open before.
    depth: usize,
}

impl Drop for PhaseGuard {
    fn drop(&mut self) {
        OBSERVING.with(|stack| stack.borrow_mut().truncate(self.depth));
    }
}

/// A handle of a composition through which a host lays out and draws its tree, so that state
/// read there is known to be read in that phase, and through which it learns what a change
/// to such state asks of it. Every clone refers to the same composition.
#[derive(Clone)]
pub struct PhaseObserver {
    /// The invalidation of the composition the phases belong to.
    invalidation: Rc<Invalidation>,
}

impl PhaseObserver {
    /// An observer of the composition whose invalidation is `invalidation`.
    pub(crate) fn new(invalidation: Rc<Invalidation>) -> Self {
        Self { invalidation }
    }

    /// Runs `body`, counting the state it reads outside composition as read in `phase` of
    /// this composition. A later change to state read only so, and by nothing composing,
    /// does not mark the composition dirty: it asks for a layout ([`take_relayout`]) or a
    /// drawing ([`take_redraw`]) instead and runs the composition's waker.
    ///
    /// [`take_relayout`]: PhaseObserver::take_relayout
    /// [`take_redraw`]: PhaseObserver::take_redraw
    pub fn run<R>(&self, phase: Phase, body: impl FnOnce() -> R) -> R {
        let depth = OBSERVING.with(|stack| {
            let mut stack = stack.borrow_mut();
            stack.push((phase, Rc::clone(&self.invalidation)));
            stack.len() - 1
        });
        let _guard = PhaseGuard { depth };
        body()
    }

    /// Takes whether state read while laying out, and by nothing composing, changed since
    /// this was last asked: the tree is to be measured and drawn again, not composed.
    pub fn take_relayout(&self) -> bool {
        self.invalidation.take_phase(Phase::Layout)
    }

    /// Takes whether state read only while drawing changed since this was last asked: the
    /// laid out tree is to be drawn again, neither composed nor measured.
    pub fn take_redraw(&self) -> bool {
        self.invalidation.take_phase(Phase::Drawing)
    }
}

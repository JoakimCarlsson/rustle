//! Composition lifecycle: remembered state slots and full-root recomposition.

use std::any::Any;
use std::cell::RefCell;
use std::mem;
use std::rc::Rc;

use crate::Node;
use crate::state::{Invalidation, State};

/// Per-pass view of a composition's slots, installed while its root runs.
struct Runtime {
    slots: Vec<Box<dyn Any>>,
    cursor: usize,
    /// True until the first pass completes; only then may slots be added.
    initial: bool,
    invalidation: Rc<Invalidation>,
}

thread_local! {
    static RUNTIME: RefCell<Option<Runtime>> = const { RefCell::new(None) };
}

/// Hands the slots back to their composition and reinstates the previously
/// active runtime, also when the root panics.
struct PassGuard<'a> {
    previous: Option<Runtime>,
    slots: &'a mut Vec<Box<dyn Any>>,
}

impl Drop for PassGuard<'_> {
    fn drop(&mut self) {
        let finished = RUNTIME.with(|runtime| runtime.replace(self.previous.take()));
        if let Some(runtime) = finished {
            *self.slots = runtime.slots;
        }
    }
}

/// A root composable together with the state it remembers.
///
/// Every pass runs the whole root again and replaces the previous tree.
/// State changes only mark the composition dirty; call [`Composition::recompose`]
/// to apply them.
pub struct Composition {
    root: Box<dyn FnMut()>,
    slots: Vec<Box<dyn Any>>,
    composed: bool,
    invalidation: Rc<Invalidation>,
    tree: Node,
}

impl Composition {
    pub fn new(root: impl FnMut() + 'static) -> Self {
        Self {
            root: Box::new(root),
            slots: Vec::new(),
            composed: false,
            invalidation: Invalidation::new(),
            tree: Node::Column(Vec::new()),
        }
    }

    /// Registers a callback run when the composition becomes dirty.
    ///
    /// It runs once per clean-to-dirty transition, so repeated state updates
    /// before the next recomposition notify only once. It must not touch state.
    pub fn on_invalidate(&mut self, waker: impl Fn() + 'static) {
        self.invalidation.set_waker(waker);
    }

    /// The tree produced by the latest pass.
    pub fn tree(&self) -> &Node {
        &self.tree
    }

    /// True if state changed since the latest pass, or none has run yet.
    pub fn is_dirty(&self) -> bool {
        self.invalidation.is_dirty()
    }

    /// Runs the root unconditionally and replaces the tree.
    ///
    /// Changing state while composing marks the composition dirty again.
    ///
    /// # Panics
    ///
    /// Panics if `remember` calls changed in number, order or type since the
    /// first pass. Remembered state is matched by call order, so conditional
    /// `remember` calls, keyed identity and dynamic lists are not supported.
    pub fn compose(&mut self) {
        self.invalidation.clear();
        let runtime = Runtime {
            slots: mem::take(&mut self.slots),
            cursor: 0,
            initial: !self.composed,
            invalidation: Rc::clone(&self.invalidation),
        };
        let previous = RUNTIME.with(|current| current.replace(Some(runtime)));
        let guard = PassGuard {
            previous,
            slots: &mut self.slots,
        };

        let tree = crate::compose(&mut self.root);
        RUNTIME.with(|runtime| {
            let runtime = runtime.borrow();
            let runtime = runtime.as_ref().expect("runtime missing after pass");
            assert_eq!(
                runtime.cursor,
                runtime.slots.len(),
                "composition structure changed: fewer `remember` calls than in the first pass"
            );
        });

        drop(guard);
        self.tree = tree;
        self.composed = true;
    }

    /// Composes again if dirty. Returns whether a pass ran.
    pub fn recompose(&mut self) -> bool {
        let dirty = self.is_dirty();
        if dirty {
            self.compose();
        }
        dirty
    }
}

/// Returns the state of the current `remember` slot, creating it with `init`
/// on the first composition.
///
/// The slot is identified by the order of `remember` calls within its
/// composition, so every pass must call `remember` the same number of times in
/// the same order. A change panics instead of silently mixing up state.
///
/// # Panics
///
/// Panics when called outside a composition pass, or if `init` itself calls
/// `remember`.
pub fn remember<T: 'static>(init: impl FnOnce() -> T) -> State<T> {
    RUNTIME.with(|runtime| {
        let mut runtime = runtime
            .try_borrow_mut()
            .expect("`remember` initializer must not call composables");
        let runtime = runtime
            .as_mut()
            .expect("`remember` can only be called during composition");

        let index = runtime.cursor;
        runtime.cursor += 1;
        if let Some(slot) = runtime.slots.get(index) {
            return slot
                .downcast_ref::<State<T>>()
                .unwrap_or_else(|| {
                    panic!(
                        "composition structure changed: `remember` slot {index} has another type"
                    )
                })
                .clone();
        }
        assert!(
            runtime.initial,
            "composition structure changed: new `remember` call after the first pass"
        );
        let state = State::new(init(), Rc::clone(&runtime.invalidation));
        runtime.slots.push(Box::new(state.clone()));
        state
    })
}

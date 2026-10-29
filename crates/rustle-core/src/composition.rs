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

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::{column, text};

    type Handle = Rc<RefCell<Option<State<i32>>>>;

    /// A counter root that publishes its state handle through `handle`.
    fn counter(handle: &Handle) -> Composition {
        let handle = Rc::clone(handle);
        Composition::new(move || {
            let count = remember(|| 0);
            *handle.borrow_mut() = Some(count.clone());
            column(|| text(format!("Count: {}", count.get())));
        })
    }

    fn state(handle: &Handle) -> State<i32> {
        handle.borrow().clone().expect("not composed")
    }

    /// The number shown by a `counter` tree.
    fn state_of(composition: &Composition) -> String {
        match composition.tree() {
            Node::Column(children) => match &children[0] {
                Node::Text(t) => t.trim_start_matches("Count: ").to_string(),
                other => panic!("unexpected {other:?}"),
            },
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn remember_initializes_once() {
        let inits = Rc::new(Cell::new(0));
        let seen = Rc::clone(&inits);
        let mut composition = Composition::new(move || {
            let value = remember(|| {
                seen.set(seen.get() + 1);
                7
            });
            value.update(|v| *v += 1);
            text(value.get().to_string());
        });
        composition.compose();
        composition.compose();
        composition.compose();
        assert_eq!(inits.get(), 1);
        assert_eq!(composition.tree(), &Node::Text("10".into()));
    }

    #[test]
    fn state_set_and_update() {
        let handle = Handle::default();
        let mut composition = counter(&handle);
        composition.compose();
        let count = state(&handle);
        count.set(5);
        count.update(|n| *n += 1);
        assert_eq!(count.get(), 6);
    }

    #[test]
    fn clones_share_the_value() {
        let handle = Handle::default();
        let mut composition = counter(&handle);
        composition.compose();
        let a = state(&handle);
        let b = a.clone();
        b.set(42);
        assert_eq!(a.get(), 42);
        assert_eq!(a.with(|n| *n), 42);
    }

    #[test]
    fn state_does_not_need_clone() {
        struct Opaque(u8);
        let mut composition = Composition::new(|| {
            let state = remember(|| Opaque(1));
            state.update(|o| o.0 += 1);
            text(state.with(|o| o.0.to_string()));
        });
        composition.compose();
        assert_eq!(composition.tree(), &Node::Text("2".into()));
    }

    #[test]
    fn recomposition_rebuilds_the_tree() {
        let handle = Handle::default();
        let mut composition = counter(&handle);
        composition.compose();
        assert_eq!(state_of(&composition), "0");

        state(&handle).set(3);
        assert!(composition.recompose());
        assert_eq!(
            composition.tree(),
            &Node::Column(vec![Node::Text("Count: 3".into())])
        );
    }

    #[test]
    fn updates_are_coalesced() {
        let passes = Rc::new(Cell::new(0));
        let wakes = Rc::new(Cell::new(0));
        let handle = Handle::default();
        let mut composition = {
            let passes = Rc::clone(&passes);
            let handle = Rc::clone(&handle);
            Composition::new(move || {
                passes.set(passes.get() + 1);
                *handle.borrow_mut() = Some(remember(|| 0));
            })
        };
        let counted = Rc::clone(&wakes);
        composition.on_invalidate(move || counted.set(counted.get() + 1));

        composition.compose();
        assert!(!composition.is_dirty());

        let count = state(&handle);
        count.set(1);
        count.update(|n| *n += 1);
        count.set(10);
        assert_eq!(wakes.get(), 1);
        assert!(composition.is_dirty());
        assert_eq!(passes.get(), 1, "setting state must not compose");

        assert!(composition.recompose());
        assert_eq!(passes.get(), 2);
        assert!(!composition.recompose(), "clean composition stays idle");
        assert_eq!(passes.get(), 2);
    }

    #[test]
    fn compositions_do_not_share_state() {
        let (h1, h2) = (Handle::default(), Handle::default());
        let (mut c1, mut c2) = (counter(&h1), counter(&h2));
        c1.compose();
        c2.compose();
        state(&h1).set(1);
        assert!(c1.is_dirty());
        assert!(!c2.is_dirty());
        c1.recompose();
        c2.recompose();
        assert_eq!(state(&h2).get(), 0);
        assert_eq!(state_of(&c1), "1");
        assert_eq!(state_of(&c2), "0");
    }

    #[test]
    fn multiple_remembered_values_stay_distinct() {
        let mut composition = Composition::new(|| {
            let a = remember(|| 1);
            let b = remember(|| String::from("b"));
            let c = remember(|| 3.5_f64);
            a.update(|v| *v += 1);
            b.update(|v| v.push('!'));
            text(format!("{} {} {}", a.get(), b.get(), c.get()));
        });
        composition.compose();
        composition.compose();
        assert_eq!(composition.tree(), &Node::Text("3 b!! 3.5".into()));
    }

    struct DropFlag(Rc<Cell<bool>>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    #[test]
    fn dropping_a_composition_releases_its_state() {
        let dropped = Rc::new(Cell::new(false));
        let escaped: Rc<RefCell<Option<State<DropFlag>>>> = Rc::default();
        let mut composition = {
            let (dropped, escaped) = (Rc::clone(&dropped), Rc::clone(&escaped));
            Composition::new(move || {
                let flag = remember(|| DropFlag(Rc::clone(&dropped)));
                *escaped.borrow_mut() = Some(flag);
            })
        };
        composition.compose();

        // An external handle keeps the value alive and usable.
        let handle = escaped.borrow_mut().take().unwrap();
        drop(composition);
        assert!(!dropped.get());
        handle.set(DropFlag(Rc::clone(&dropped)));
        assert!(dropped.get(), "replaced value is dropped");

        dropped.set(false);
        drop(handle);
        assert!(dropped.get(), "last handle releases the value");
    }

    #[test]
    fn dropping_a_composition_drops_unshared_state() {
        let dropped = Rc::new(Cell::new(false));
        let mut composition = {
            let dropped = Rc::clone(&dropped);
            Composition::new(move || {
                remember(|| DropFlag(Rc::clone(&dropped)));
            })
        };
        composition.compose();
        assert!(!dropped.get());
        drop(composition);
        assert!(dropped.get());
    }

    #[test]
    #[should_panic(expected = "during composition")]
    fn remember_outside_composition_panics() {
        remember(|| 0);
    }

    #[test]
    #[should_panic(expected = "fewer `remember` calls")]
    fn dropping_a_remember_call_is_detected() {
        let enabled = Rc::new(Cell::new(true));
        let flag = Rc::clone(&enabled);
        let mut composition = Composition::new(move || {
            if flag.get() {
                remember(|| 0);
            }
        });
        composition.compose();
        enabled.set(false);
        composition.compose();
    }

    #[test]
    #[should_panic(expected = "new `remember` call")]
    fn adding_a_remember_call_is_detected() {
        let enabled = Rc::new(Cell::new(false));
        let flag = Rc::clone(&enabled);
        let mut composition = Composition::new(move || {
            if flag.get() {
                remember(|| 0);
            }
        });
        composition.compose();
        enabled.set(true);
        composition.compose();
    }

    #[test]
    fn panic_in_root_restores_the_runtime() {
        let mut broken = Composition::new(|| {
            remember(|| 0);
            panic!("boom");
        });
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| broken.compose())).is_err()
        );
        let result = std::panic::catch_unwind(|| remember(|| 0));
        assert!(result.is_err(), "runtime must be uninstalled after a panic");

        let mut healthy = Composition::new(|| text("ok"));
        healthy.compose();
        assert_eq!(healthy.tree(), &Node::Text("ok".into()));
    }
}

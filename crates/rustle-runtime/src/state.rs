//! Observable state and the invalidation flag it raises.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Shared dirty flag of one composition.
///
/// Raising it is cheap and idempotent: the waker only runs when the flag goes
/// from clean to dirty, so any number of updates before the next recomposition
/// produce a single wake-up.
pub(crate) struct Invalidation {
    dirty: Cell<bool>,
    waker: RefCell<Option<Box<dyn Fn()>>>,
}

impl Invalidation {
    /// Starts dirty, because a new composition has not composed yet.
    pub(crate) fn new() -> Rc<Self> {
        Rc::new(Self {
            dirty: Cell::new(true),
            waker: RefCell::new(None),
        })
    }

    pub(crate) fn invalidate(&self) {
        if !self.dirty.replace(true)
            && let Some(waker) = &*self.waker.borrow()
        {
            waker();
        }
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty.get()
    }

    pub(crate) fn clear(&self) {
        self.dirty.set(false);
    }

    pub(crate) fn set_waker(&self, waker: impl Fn() + 'static) {
        *self.waker.borrow_mut() = Some(Box::new(waker));
    }
}

struct Inner<T> {
    value: RefCell<T>,
    invalidation: Rc<Invalidation>,
}

/// A shared, observable value that survives recomposition.
///
/// Cloning is cheap and every clone refers to the same value. Changing the
/// value invalidates the owning composition but never recomposes by itself.
///
/// The value must not be accessed from inside the closure passed to
/// [`State::update`] or [`State::with`]; doing so panics.
pub struct State<T> {
    inner: Rc<Inner<T>>,
}

impl<T> State<T> {
    pub(crate) fn new(value: T, invalidation: Rc<Invalidation>) -> Self {
        Self {
            inner: Rc::new(Inner {
                value: RefCell::new(value),
                invalidation,
            }),
        }
    }

    /// Reads the value without cloning it.
    pub fn with<R>(&self, read: impl FnOnce(&T) -> R) -> R {
        read(&self.inner.value.try_borrow().expect(REENTRANT))
    }

    /// Replaces the value and invalidates the composition.
    pub fn set(&self, value: T) {
        *self.inner.value.try_borrow_mut().expect(REENTRANT) = value;
        self.inner.invalidation.invalidate();
    }

    /// Modifies the value in place and invalidates the composition.
    pub fn update(&self, change: impl FnOnce(&mut T)) {
        change(&mut self.inner.value.try_borrow_mut().expect(REENTRANT));
        self.inner.invalidation.invalidate();
    }
}

impl<T: Clone> State<T> {
    /// Returns a clone of the current value.
    pub fn get(&self) -> T {
        self.with(T::clone)
    }
}

// Manual impl: cloning the handle must not require `T: Clone`.
impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Rc::clone(&self.inner),
        }
    }
}

const REENTRANT: &str = "State accessed re-entrantly from inside `update` or `with`";

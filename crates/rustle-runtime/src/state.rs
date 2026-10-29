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
    /// Counts changes, so what read the value can tell it has moved on.
    version: Cell<u64>,
}

/// Something whose version can be compared with what was read earlier.
pub(crate) trait Versioned {
    fn version(&self) -> u64;
}

impl<T> Versioned for Inner<T> {
    fn version(&self) -> u64 {
        self.version.get()
    }
}

/// Something a skippable block read, which can be checked for having changed.
#[derive(Clone)]
pub(crate) enum Read {
    /// A state and the version it had.
    State(Rc<dyn Versioned>, u64),
    /// A check that is true while what was read, such as a composition local, still is.
    Same(Rc<dyn Fn() -> bool>),
}

impl Read {
    /// Whether what was read is as it was.
    pub(crate) fn is_current(&self) -> bool {
        match self {
            Self::State(state, version) => state.version() == *version,
            Self::Same(check) => check(),
        }
    }
}

/// What one run of a skippable block read.
#[derive(Default)]
pub(crate) struct Recording {
    pub(crate) reads: Vec<Read>,
    /// False once the block read something whose changes are not tracked.
    pub(crate) unskippable: bool,
}

thread_local! {
    static RECORDINGS: RefCell<Vec<Recording>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn begin_recording() {
    RECORDINGS.with(|stack| stack.borrow_mut().push(Recording::default()));
}

pub(crate) fn end_recording() -> Recording {
    RECORDINGS.with(|stack| stack.borrow_mut().pop().unwrap_or_default())
}

/// Counts what the enclosing skippable block read, and marks it unskippable if it could not be tracked.
pub(crate) fn merge_recording(reads: Vec<Read>, unskippable: bool) {
    RECORDINGS.with(|stack| {
        if let Some(top) = stack.borrow_mut().last_mut() {
            top.reads.extend(reads);
            top.unskippable |= unskippable;
        }
    });
}

/// Notes something the enclosing skippable block read, as a check that it is unchanged.
pub(crate) fn record_check(check: Rc<dyn Fn() -> bool>) {
    RECORDINGS.with(|stack| {
        if let Some(top) = stack.borrow_mut().last_mut() {
            top.reads.push(Read::Same(check));
        }
    });
}

/// Pops the recording again if the block panicked before ending it.
pub(crate) struct RecordingGuard {
    pub(crate) depth: usize,
}

impl Drop for RecordingGuard {
    fn drop(&mut self) {
        RECORDINGS.with(|stack| stack.borrow_mut().truncate(self.depth));
    }
}

pub(crate) fn recording_depth() -> usize {
    RECORDINGS.with(|stack| stack.borrow().len())
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

impl<T: 'static> State<T> {
    pub(crate) fn new(value: T, invalidation: Rc<Invalidation>) -> Self {
        Self {
            inner: Rc::new(Inner {
                value: RefCell::new(value),
                invalidation,
                version: Cell::new(0),
            }),
        }
    }

    /// Reads the value without cloning it.
    pub fn with<R>(&self, read: impl FnOnce(&T) -> R) -> R {
        RECORDINGS.with(|stack| {
            if let Some(top) = stack.borrow_mut().last_mut() {
                let inner: Rc<dyn Versioned> = self.inner.clone();
                top.reads.push(Read::State(inner, self.inner.version.get()));
            }
        });
        read(&self.inner.value.try_borrow().expect(REENTRANT))
    }

    /// Replaces the value without invalidating the composition.
    ///
    /// For values written while composing, such as one that mirrors an
    /// argument, where recomposing for the change would only repeat the pass.
    pub(crate) fn replace_quietly(&self, value: T) {
        *self.inner.value.try_borrow_mut().expect(REENTRANT) = value;
        self.inner.version.set(self.inner.version.get() + 1);
    }

    /// Replaces the value and invalidates the composition.
    pub fn set(&self, value: T) {
        *self.inner.value.try_borrow_mut().expect(REENTRANT) = value;
        self.inner.version.set(self.inner.version.get() + 1);
        self.inner.invalidation.invalidate();
    }

    /// Modifies the value in place and invalidates the composition.
    pub fn update(&self, change: impl FnOnce(&mut T)) {
        change(&mut self.inner.value.try_borrow_mut().expect(REENTRANT));
        self.inner.version.set(self.inner.version.get() + 1);
        self.inner.invalidation.invalidate();
    }
}

impl<T: Clone + 'static> State<T> {
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

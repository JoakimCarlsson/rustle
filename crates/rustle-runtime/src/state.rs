//! Observable state and the invalidation flag it raises.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::mem;
use std::rc::{Rc, Weak};

use crate::scope::{self, ScopeInner};
use crate::snapshot;

/// Shared dirty flag of one composition.
///
/// Raising it is cheap and idempotent: the waker only runs when the flag goes
/// from clean to dirty, so any number of updates before the next recomposition
/// produce a single wake-up.
pub(crate) struct Invalidation {
    dirty: Cell<bool>,
    /// Whether the whole root has to run again, not only the scopes in `queue`.
    root: Cell<bool>,
    /// Scopes that read state which changed, waiting to be recomposed.
    queue: RefCell<Vec<Weak<ScopeInner>>>,
    waker: RefCell<Option<Box<dyn Fn()>>>,
}

impl Invalidation {
    /// Starts dirty, because a new composition has not composed yet.
    pub(crate) fn new() -> Rc<Self> {
        Rc::new(Self {
            dirty: Cell::new(true),
            root: Cell::new(true),
            queue: RefCell::new(Vec::new()),
            waker: RefCell::new(None),
        })
    }

    /// Marks the whole root as needing to run again.
    pub(crate) fn invalidate(&self) {
        self.root.set(true);
        self.wake();
    }

    /// Marks `scope` as needing to run again, without the root.
    pub(crate) fn invalidate_scope(&self, scope: &Rc<ScopeInner>) {
        scope.mark_dirty();
        if !scope.queued.replace(true) {
            self.queue.borrow_mut().push(Rc::downgrade(scope));
        }
        self.wake();
    }

    /// Raises the dirty flag, running the waker on the clean-to-dirty change.
    fn wake(&self) {
        if !self.dirty.replace(true)
            && let Some(waker) = &*self.waker.borrow()
        {
            waker();
        }
    }

    /// Whether the root has to run again.
    pub(crate) fn root_dirty(&self) -> bool {
        self.root.get()
    }

    /// Takes the scopes waiting to be recomposed.
    pub(crate) fn take_queue(&self) -> Vec<Weak<ScopeInner>> {
        mem::take(&mut *self.queue.borrow_mut())
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty.get()
    }

    pub(crate) fn clear(&self) {
        self.dirty.set(false);
        self.root.set(false);
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
    /// Copies a value, for state that takes part in snapshots. `None` for plain state.
    cloner: Option<fn(&T) -> T>,
    /// The global generation of the latest write, to detect conflicting snapshot writes.
    written: Cell<u64>,
    /// Values overwritten while a snapshot was open, each with the generation of its overwrite.
    retained: RefCell<Vec<(u64, T)>>,
    /// The scopes that read the value, each with the run of the scope that read it.
    readers: RefCell<Vec<(Weak<ScopeInner>, u64)>>,
    /// Whether something composing outside a scope read the value.
    root_read: Cell<bool>,
}

impl<T: 'static> Inner<T> {
    /// Stores `value` globally: bumps the version, records the write for open snapshots
    /// and invalidates the composition. Returns nothing; observers are told by the caller.
    fn commit(&self, value: T) {
        let old = mem::replace(&mut *self.value.try_borrow_mut().expect(REENTRANT), value);
        self.finish_write(old);
    }

    /// Bookkeeping after the global value changed from `old`.
    fn finish_write(&self, old: T) {
        let generation = snapshot::next_generation();
        self.written.set(generation);
        let mut retained = self.retained.borrow_mut();
        if snapshot::has_open() && self.cloner.is_some() {
            retained.push((generation, old));
        } else {
            retained.clear();
        }
        drop(retained);
        self.version.set(self.version.get() + 1);
        self.invalidate_readers();
    }

    /// Invalidates what read the value: the scopes that did, or the whole root when the
    /// root read it or nothing composing is known to have.
    fn invalidate_readers(&self) {
        if self.root_read.get() {
            self.invalidation.invalidate();
            return;
        }
        let mut live = Vec::new();
        self.readers.borrow_mut().retain(|(weak, run)| {
            match weak.upgrade().filter(|scope| scope.run() == *run) {
                Some(scope) => {
                    live.push(scope);
                    true
                }
                None => false,
            }
        });
        if live.is_empty() {
            self.invalidation.invalidate();
        }
        for scope in &live {
            self.invalidation.invalidate_scope(scope);
        }
    }

    /// Remembers that whatever composes now read the value.
    fn note_read(&self) {
        match scope::reader() {
            scope::Reader::Scope(scope) => {
                let run = scope.run();
                let mut readers = self.readers.borrow_mut();
                let known = readers.iter().any(|(weak, seen)| {
                    *seen == run && std::ptr::eq(weak.as_ptr(), Rc::as_ptr(&scope))
                });
                if !known {
                    readers.push((Rc::downgrade(&scope), run));
                }
            }
            scope::Reader::Root => self.root_read.set(true),
            scope::Reader::Nobody => {}
        }
    }

    /// The value as `snapshot` sees it, or `None` when it sees the live one.
    fn visible_in(&self, snapshot: &snapshot::SnapshotData) -> Option<T> {
        let cloner = self.cloner?;
        let key = std::ptr::from_ref(self) as usize;
        if let Some(pending) = snapshot.peek(key, |value| value.downcast_ref::<T>().map(cloner)) {
            return pending;
        }
        if snapshot.generation() >= self.written.get() {
            return None;
        }
        let retained = self.retained.borrow();
        retained
            .iter()
            .find(|(generation, _)| *generation > snapshot.generation())
            .map(|(_, value)| cloner(value))
    }
}

impl<T: 'static> snapshot::Object for Inner<T> {
    fn written(&self) -> u64 {
        self.written.get()
    }

    fn apply(&self, value: Box<dyn Any>) {
        if let Ok(value) = value.downcast::<T>() {
            self.commit(*value);
        }
    }
}

/// Something whose version can be compared with what was read earlier.
pub(crate) trait Versioned {
    fn version(&self) -> u64;

    /// Counts the value as read by whatever composes now, for a read that is replayed
    /// from a cache without running the code that made it.
    fn note_read(&self);
}

impl<T: 'static> Versioned for Inner<T> {
    fn version(&self) -> u64 {
        self.version.get()
    }

    fn note_read(&self) {
        Inner::note_read(self);
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
///
/// The state among `reads` also counts as read by whatever composes now, so a scope that
/// reuses a cached result keeps being invalidated by the state that result came from.
pub(crate) fn merge_recording(reads: Vec<Read>, unskippable: bool) {
    for read in &reads {
        if let Read::State(state, _) = read {
            state.note_read();
        }
    }
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

/// Marks the enclosing skippable block as unable to be skipped or cached, because
/// something it did cannot be tracked.
pub(crate) fn mark_untracked() {
    RECORDINGS.with(|stack| {
        if let Some(top) = stack.borrow_mut().last_mut() {
            top.unskippable = true;
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
        Self::build(value, invalidation, None)
    }

    /// State that takes part in snapshots, which needs to copy its value.
    pub(crate) fn new_snapshotted(value: T, invalidation: Rc<Invalidation>) -> Self
    where
        T: Clone,
    {
        Self::build(value, invalidation, Some(T::clone))
    }

    fn build(value: T, invalidation: Rc<Invalidation>, cloner: Option<fn(&T) -> T>) -> Self {
        Self {
            inner: Rc::new(Inner {
                value: RefCell::new(value),
                invalidation,
                version: Cell::new(0),
                cloner,
                written: Cell::new(0),
                retained: RefCell::new(Vec::new()),
                readers: RefCell::new(Vec::new()),
                root_read: Cell::new(false),
            }),
        }
    }

    /// Identifies the value, for snapshot bookkeeping.
    pub(crate) fn key(&self) -> usize {
        Rc::as_ptr(&self.inner) as *const () as usize
    }

    /// Reads the value without cloning it.
    ///
    /// Inside a [`Snapshot`](crate::Snapshot), state made by
    /// [`mutable_state_of`](crate::mutable_state_of) reads as the snapshot sees it.
    pub fn with<R>(&self, read: impl FnOnce(&T) -> R) -> R {
        RECORDINGS.with(|stack| {
            if let Some(top) = stack.borrow_mut().last_mut() {
                let inner: Rc<dyn Versioned> = self.inner.clone();
                top.reads.push(Read::State(inner, self.inner.version.get()));
            }
        });
        self.inner.note_read();
        if let Some(view) = snapshot::current().and_then(|data| self.inner.visible_in(&data)) {
            return read(&view);
        }
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
    ///
    /// Inside a mutable snapshot, state made by [`mutable_state_of`](crate::mutable_state_of)
    /// keeps the write to the snapshot until it is applied.
    pub fn set(&self, value: T) {
        if let Some(data) = self.snapshot_target() {
            self.write_to(&data, value);
            return;
        }
        self.inner.commit(value);
        snapshot::notify(&[self.key()]);
    }

    /// Modifies the value in place and invalidates the composition.
    pub fn update(&self, change: impl FnOnce(&mut T)) {
        if let Some(data) = self.snapshot_target() {
            let cloner = self.inner.cloner.expect("snapshot state clones");
            let mut value = self
                .inner
                .visible_in(&data)
                .unwrap_or_else(|| cloner(&self.inner.value.try_borrow().expect(REENTRANT)));
            change(&mut value);
            self.write_to(&data, value);
            return;
        }
        let old = self
            .inner
            .cloner
            .filter(|_| snapshot::has_open())
            .map(|clone| clone(&self.inner.value.try_borrow().expect(REENTRANT)));
        change(&mut self.inner.value.try_borrow_mut().expect(REENTRANT));
        match old {
            Some(old) => self.inner.finish_write(old),
            None => {
                self.inner.written.set(snapshot::next_generation());
                self.inner.version.set(self.inner.version.get() + 1);
                self.inner.invalidate_readers();
            }
        }
        snapshot::notify(&[self.key()]);
    }

    /// The innermost entered mutable snapshot, if this state takes part in snapshots.
    fn snapshot_target(&self) -> Option<Rc<snapshot::SnapshotData>> {
        self.inner.cloner?;
        snapshot::current().filter(|data| data.is_mutable())
    }

    /// Keeps `value` as this state's pending write in `data`.
    fn write_to(&self, data: &snapshot::SnapshotData, value: T) {
        let object: Rc<dyn snapshot::Object> = self.inner.clone();
        data.write(self.key(), object, Box::new(value));
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

//! Snapshot state: isolated views of state, writes applied together, and observers of what changed.
//!
//! State made by [`mutable_state_of`](crate::mutable_state_of) takes part in
//! snapshots. A [`Snapshot`] freezes what such state holds; a
//! [`MutableSnapshot`] also keeps the writes made inside it away from everyone
//! else until [`MutableSnapshot::apply`] publishes them in one step, or
//! reports that something else wrote the same state meanwhile.
//! State made any other way is always read and written live.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::state::{Read, State, begin_recording, end_recording};

/// A state inside a snapshot's bookkeeping, whatever its value type.
pub(crate) trait Object {
    /// The global generation of the state's latest write.
    fn written(&self) -> u64;
    /// Publishes a pending value, which must be of the state's type.
    fn apply(&self, value: Box<dyn Any>);
}

/// A pending write: the state written and the value waiting to be published.
type PendingWrite = (Rc<dyn Object>, Box<dyn Any>);

/// A callback told of the changes a snapshot applied, with the id it is removed by.
type Observer = (u64, Rc<dyn Fn(&AppliedChanges)>);

/// The writes and view of one snapshot.
pub(crate) struct SnapshotData {
    generation: u64,
    mutable: bool,
    parent: Option<Rc<SnapshotData>>,
    writes: RefCell<HashMap<usize, PendingWrite>>,
    live: Cell<bool>,
}

impl SnapshotData {
    /// The global generation the snapshot was taken at.
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether writes made inside are kept back.
    pub(crate) fn is_mutable(&self) -> bool {
        self.mutable
    }

    /// Runs `read` on the pending value of `key`, in this snapshot or an enclosing one.
    pub(crate) fn peek<R>(&self, key: usize, read: impl FnOnce(&dyn Any) -> R) -> Option<R> {
        let mut current = Some(self);
        while let Some(data) = current {
            if let Some((_, value)) = data.writes.borrow().get(&key) {
                return Some(read(value.as_ref()));
            }
            current = data.parent.as_deref();
        }
        None
    }

    /// Records a pending write of `key`.
    pub(crate) fn write(&self, key: usize, object: Rc<dyn Object>, value: Box<dyn Any>) {
        self.writes.borrow_mut().insert(key, (object, value));
    }

    /// Stops the snapshot counting as open.
    fn close(&self) {
        if self.live.replace(false) {
            OPEN.with(|open| open.set(open.get() - 1));
        }
    }
}

thread_local! {
    static GENERATION: Cell<u64> = const { Cell::new(0) };
    static OPEN: Cell<usize> = const { Cell::new(0) };
    static CURRENT: RefCell<Vec<Rc<SnapshotData>>> = const { RefCell::new(Vec::new()) };
    static OBSERVERS: RefCell<Vec<Observer>> = const { RefCell::new(Vec::new()) };
    static NEXT_OBSERVER: Cell<u64> = const { Cell::new(0) };
}

/// Advances the global write generation and returns it.
pub(crate) fn next_generation() -> u64 {
    GENERATION.with(|generation| {
        generation.set(generation.get() + 1);
        generation.get()
    })
}

/// Whether any snapshot is open, so overwritten values must be kept.
pub(crate) fn has_open() -> bool {
    OPEN.with(|open| open.get() > 0)
}

/// The innermost snapshot being run, if any.
pub(crate) fn current() -> Option<Rc<SnapshotData>> {
    CURRENT.with(|stack| stack.borrow().last().cloned())
}

/// Tells the apply observers that `keys` were written globally.
pub(crate) fn notify(keys: &[usize]) {
    let observers: Vec<_> = OBSERVERS.with(|observers| {
        observers
            .borrow()
            .iter()
            .map(|(_, observer)| Rc::clone(observer))
            .collect()
    });
    if observers.is_empty() {
        return;
    }
    let changes = AppliedChanges {
        keys: keys.to_vec(),
    };
    for observer in observers {
        observer(&changes);
    }
}

/// Pops the entered snapshot again, also when the block panics.
struct EnterGuard;

impl Drop for EnterGuard {
    fn drop(&mut self) {
        CURRENT.with(|stack| stack.borrow_mut().pop());
    }
}

/// Runs `block` with `data` as the innermost snapshot.
fn enter<R>(data: &Rc<SnapshotData>, block: impl FnOnce() -> R) -> R {
    CURRENT.with(|stack| stack.borrow_mut().push(Rc::clone(data)));
    let _pop = EnterGuard;
    block()
}

/// Opens a snapshot at the current generation, nested in the one being run.
fn open(mutable: bool) -> Rc<SnapshotData> {
    OPEN.with(|open| open.set(open.get() + 1));
    Rc::new(SnapshotData {
        generation: GENERATION.with(Cell::get),
        mutable,
        parent: current(),
        writes: RefCell::new(HashMap::new()),
        live: Cell::new(true),
    })
}

/// The states written by an applied snapshot or a global write.
pub struct AppliedChanges {
    keys: Vec<usize>,
}

impl AppliedChanges {
    /// Whether `state` is among the changed ones.
    pub fn contains<T: 'static>(&self, state: &State<T>) -> bool {
        self.keys.contains(&state.key())
    }

    /// How many states changed.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether nothing changed.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// Keeps an apply observer registered until dropped.
pub struct ObserverHandle {
    id: u64,
}

impl Drop for ObserverHandle {
    fn drop(&mut self) {
        OBSERVERS.with(|observers| observers.borrow_mut().retain(|(id, _)| *id != self.id));
    }
}

/// A frozen view of snapshot-aware state.
///
/// Inside [`Snapshot::enter`], state made by [`mutable_state_of`](crate::mutable_state_of)
/// reads as it was when the snapshot was taken, however it changes after.
/// Dispose it, or drop it, when done: while one is open, overwritten values are kept.
pub struct Snapshot {
    data: Rc<SnapshotData>,
}

impl Snapshot {
    /// Takes a read-only snapshot of the state as it is now.
    pub fn take_snapshot() -> Self {
        Self { data: open(false) }
    }

    /// Takes a snapshot whose writes stay inside it until applied.
    pub fn take_mutable_snapshot() -> MutableSnapshot {
        MutableSnapshot {
            data: open(true),
            applied: false,
        }
    }

    /// Runs `block` in a mutable snapshot and applies it, running `block` again
    /// from a fresh snapshot if another write got in between. Returns what the
    /// last run returned.
    pub fn with_mutable_snapshot<R>(mut block: impl FnMut() -> R) -> R {
        loop {
            let mut snapshot = Self::take_mutable_snapshot();
            let result = snapshot.enter(&mut block);
            if snapshot.apply() == ApplyResult::Success {
                return result;
            }
        }
    }

    /// Registers `observer` to run after every apply of a mutable snapshot and
    /// every write to snapshot-aware state outside one. It must not write state.
    pub fn register_apply_observer(observer: impl Fn(&AppliedChanges) + 'static) -> ObserverHandle {
        let id = NEXT_OBSERVER.with(|next| {
            next.set(next.get() + 1);
            next.get()
        });
        OBSERVERS.with(|observers| observers.borrow_mut().push((id, Rc::new(observer))));
        ObserverHandle { id }
    }

    /// Runs `block` seeing the state as this snapshot does.
    pub fn enter<R>(&self, block: impl FnOnce() -> R) -> R {
        enter(&self.data, block)
    }

    /// Releases what the snapshot kept. Reading in it afterwards sees the live state.
    pub fn dispose(self) {}
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        self.data.close();
    }
}

/// Whether [`MutableSnapshot::apply`] published the snapshot's writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyResult {
    /// The writes are now visible outside the snapshot.
    Success,
    /// Something wrote a state the snapshot wrote too, so nothing was applied.
    Failure,
}

/// A snapshot that holds back its writes until [`MutableSnapshot::apply`].
pub struct MutableSnapshot {
    data: Rc<SnapshotData>,
    applied: bool,
}

impl MutableSnapshot {
    /// Runs `block` seeing and writing this snapshot's state.
    pub fn enter<R>(&mut self, block: impl FnOnce() -> R) -> R {
        enter(&self.data, block)
    }

    /// Publishes the snapshot's writes in one step: into the snapshot it was
    /// taken in, or globally, where each written state invalidates its
    /// composition and the apply observers run once for all of them.
    ///
    /// Nothing is applied when a state it wrote was also written outside it
    /// since it was taken. The snapshot is closed either way.
    pub fn apply(&mut self) -> ApplyResult {
        if self.applied {
            return ApplyResult::Success;
        }
        self.applied = true;
        let writes = std::mem::take(&mut *self.data.writes.borrow_mut());
        self.data.close();
        if let Some(parent) = self.data.parent.as_ref().filter(|parent| parent.mutable) {
            for (key, (object, value)) in writes {
                parent.write(key, object, value);
            }
            return ApplyResult::Success;
        }
        let conflict = writes
            .values()
            .any(|(object, _)| object.written() > self.data.generation);
        if conflict {
            return ApplyResult::Failure;
        }
        let keys: Vec<usize> = writes.keys().copied().collect();
        for (_, (object, value)) in writes {
            object.apply(value);
        }
        if !keys.is_empty() {
            notify(&keys);
        }
        ApplyResult::Success
    }

    /// Drops the snapshot and its writes without applying them.
    pub fn dispose(self) {}
}

impl Drop for MutableSnapshot {
    fn drop(&mut self) {
        self.data.close();
    }
}

/// Turns a calculation over state into a sequence of values, one for each time
/// what it read changed. See [`snapshot_flow`].
pub struct SnapshotFlow<T> {
    block: Box<dyn FnMut() -> T>,
    reads: Vec<Read>,
    last: Option<T>,
}

impl<T: PartialEq + Clone> SnapshotFlow<T> {
    /// Returns the calculation's value if it is the first call, or if state it
    /// read has changed and the result differs from the previous one.
    /// Returns `None` otherwise, without running the calculation again when
    /// nothing it read has changed.
    pub fn poll(&mut self) -> Option<T> {
        if self.last.is_some() && self.reads.iter().all(Read::is_current) {
            return None;
        }
        begin_recording();
        let value = (self.block)();
        self.reads = end_recording().reads;
        if self.last.as_ref() == Some(&value) {
            return None;
        }
        self.last = Some(value.clone());
        Some(value)
    }
}

/// Creates a [`SnapshotFlow`] of what `block` computes from state.
///
/// The flow yields the first value on its first [`SnapshotFlow::poll`], and
/// after that a value only when state `block` read has changed and the new
/// result is different, such as to drive an effect from derived state.
pub fn snapshot_flow<T: PartialEq + Clone + 'static>(
    block: impl FnMut() -> T + 'static,
) -> SnapshotFlow<T> {
    SnapshotFlow {
        block: Box::new(block),
        reads: Vec::new(),
        last: None,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::{Composition, derived_state_of, mutable_state_of};

    /// Composes once and returns the snapshot-aware state the root made.
    fn make_state(initial: i32) -> (Composition<()>, State<i32>) {
        let slot = Rc::new(RefCell::new(None));
        let mut composition = Composition::new({
            let slot = Rc::clone(&slot);
            move || *slot.borrow_mut() = Some(mutable_state_of(initial))
        });
        composition.compose();
        let state = slot.borrow().clone().expect("state made");
        (composition, state)
    }

    /// A read-only snapshot keeps seeing the value it was taken at.
    #[test]
    fn read_snapshot_is_frozen() {
        let (_composition, state) = make_state(1);
        let snapshot = Snapshot::take_snapshot();
        state.set(2);
        assert_eq!(snapshot.enter(|| state.get()), 1);
        assert_eq!(state.get(), 2);
        snapshot.dispose();
        state.update(|value| *value += 1);
        assert_eq!(state.get(), 3);
    }

    /// Writes in a mutable snapshot stay inside it until applied, then notify observers once.
    #[test]
    fn mutable_snapshot_applies_atomically() {
        let (composition, state) = make_state(1);
        let seen = Rc::new(Cell::new(0));
        let _observer = Snapshot::register_apply_observer({
            let (seen, state) = (Rc::clone(&seen), state.clone());
            move |changes| {
                assert!(changes.contains(&state));
                seen.set(seen.get() + 1);
            }
        });
        let mut snapshot = Snapshot::take_mutable_snapshot();
        snapshot.enter(|| {
            state.set(5);
            state.update(|value| *value += 1);
            assert_eq!(state.get(), 6);
        });
        assert_eq!(state.get(), 1);
        assert!(!composition.is_dirty() || state.get() == 1);
        assert_eq!(snapshot.apply(), ApplyResult::Success);
        assert_eq!(state.get(), 6);
        assert_eq!(seen.get(), 1);
    }

    /// A global write to the same state makes the apply fail, and the helper applies.
    #[test]
    fn conflicting_apply_fails_and_helper_applies() {
        let (_composition, state) = make_state(1);
        let mut snapshot = Snapshot::take_mutable_snapshot();
        snapshot.enter(|| state.set(10));
        state.set(2);
        assert_eq!(snapshot.apply(), ApplyResult::Failure);
        assert_eq!(state.get(), 2);
        Snapshot::with_mutable_snapshot(|| {
            let base = state.get();
            state.set(base + 100);
        });
        assert_eq!(state.get(), 102);
    }

    /// Nested mutable snapshots apply into their parent first.
    #[test]
    fn nested_snapshot_applies_into_parent() {
        let (_composition, state) = make_state(1);
        let mut outer = Snapshot::take_mutable_snapshot();
        outer.enter(|| {
            Snapshot::with_mutable_snapshot(|| state.set(7));
            assert_eq!(state.get(), 7);
        });
        assert_eq!(state.get(), 1);
        assert_eq!(outer.apply(), ApplyResult::Success);
        assert_eq!(state.get(), 7);
    }

    /// The flow yields on the first poll and then only when the result changes.
    #[test]
    fn flow_yields_on_change_only() {
        let (_composition, state) = make_state(1);
        let mut flow = snapshot_flow({
            let state = state.clone();
            move || state.get() / 10
        });
        assert_eq!(flow.poll(), Some(0));
        assert_eq!(flow.poll(), None);
        state.set(5);
        assert_eq!(flow.poll(), None);
        state.set(25);
        assert_eq!(flow.poll(), Some(2));
    }

    /// A derived state only recalculates when what it read changes.
    #[test]
    fn derived_state_skips_unchanged_inputs() {
        let slot = Rc::new(RefCell::new(None));
        let runs = Rc::new(Cell::new(0));
        let other = Rc::new(RefCell::new(None));
        let mut composition = Composition::new({
            let (slot, runs, other) = (Rc::clone(&slot), Rc::clone(&runs), Rc::clone(&other));
            move || {
                let source = mutable_state_of(1);
                let unrelated = mutable_state_of(0);
                *slot.borrow_mut() = Some(source.clone());
                *other.borrow_mut() = Some(unrelated);
                let runs = Rc::clone(&runs);
                derived_state_of(move || {
                    runs.set(runs.get() + 1);
                    source.get() * 2
                })
                .get()
            }
        });
        composition.compose();
        let source: State<i32> = slot.borrow().clone().expect("source");
        let unrelated: State<i32> = other.borrow().clone().expect("unrelated");
        unrelated.set(1);
        composition.compose();
        assert_eq!((runs.get(), composition.output().copied()), (1, Some(2)));
        source.set(4);
        composition.compose();
        assert_eq!((runs.get(), composition.output().copied()), (2, Some(8)));
    }
}

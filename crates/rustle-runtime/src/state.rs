//! Observable state and the invalidation flag it raises.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::mem;
use std::rc::{Rc, Weak};

use crate::phase::Phase;
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
    /// Whether state read only while laying out changed, so the tree is to be laid out again.
    relayout: Cell<bool>,
    /// Whether state read only while drawing changed, so the tree is to be drawn again.
    redraw: Cell<bool>,
}

impl Invalidation {
    /// Starts dirty, because a new composition has not composed yet.
    pub(crate) fn new() -> Rc<Self> {
        Rc::new(Self {
            dirty: Cell::new(true),
            root: Cell::new(true),
            queue: RefCell::new(Vec::new()),
            waker: RefCell::new(None),
            relayout: Cell::new(false),
            redraw: Cell::new(false),
        })
    }

    /// Asks for `phase` to run again over the tree as it is, without composing it, and runs
    /// the waker when that was not asked for already.
    pub(crate) fn invalidate_phase(&self, phase: Phase) {
        let flag = match phase {
            Phase::Layout => &self.relayout,
            Phase::Drawing => &self.redraw,
        };
        if !flag.replace(true)
            && let Some(waker) = &*self.waker.borrow()
        {
            waker();
        }
    }

    /// Takes whether `phase` was asked to run again since this was last asked.
    pub(crate) fn take_phase(&self, phase: Phase) -> bool {
        match phase {
            Phase::Layout => self.relayout.replace(false),
            Phase::Drawing => self.redraw.replace(false),
        }
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

/// The compositions among `readers` that still exist, dropping those that do not.
fn live_invalidations(readers: &RefCell<Vec<Weak<Invalidation>>>) -> Vec<Rc<Invalidation>> {
    let mut live = Vec::new();
    readers.borrow_mut().retain(|weak| match weak.upgrade() {
        Some(invalidation) => {
            live.push(invalidation);
            true
        }
        None => false,
    });
    live
}

/// A scope that read a value: the scope, the run of it that read, and the composition it belongs to.
type Reader = (Weak<ScopeInner>, u64, Weak<Invalidation>);

struct Inner<T> {
    value: RefCell<T>,
    /// The composition that remembered the value, which a write invalidates when
    /// nothing is known to have read it. `None` for state made outside composition.
    invalidation: Option<Rc<Invalidation>>,
    /// Counts changes, so what read the value can tell it has moved on.
    version: Cell<u64>,
    /// Copies a value, for state that takes part in snapshots. `None` for plain state.
    cloner: Option<fn(&T) -> T>,
    /// The global generation of the latest write, to detect conflicting snapshot writes.
    written: Cell<u64>,
    /// Values overwritten while a snapshot was open, each with the generation of its overwrite.
    retained: RefCell<Vec<(u64, T)>>,
    /// The scopes that read the value, each with the run of the scope that read it and
    /// the composition it belongs to.
    readers: RefCell<Vec<Reader>>,
    /// The compositions in which something composing outside a scope read the value.
    root_readers: RefCell<Vec<Weak<Invalidation>>>,
    /// The compositions whose tree was laid out reading the value, outside composition.
    layout_readers: RefCell<Vec<Weak<Invalidation>>>,
    /// The compositions whose tree was drawn reading the value, outside composition.
    draw_readers: RefCell<Vec<Weak<Invalidation>>>,
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

    /// Invalidates what read the value, in every composition that read it: the whole root
    /// of each composition whose root read it, the scopes that did in the others, the layout
    /// or else the drawing of the compositions none of which composed with it but whose tree
    /// was laid out or drawn reading it, and the root of the composition that remembered it
    /// when nothing is known to have read it.
    fn invalidate_readers(&self) {
        let mut roots = Vec::new();
        self.root_readers
            .borrow_mut()
            .retain(|weak| match weak.upgrade() {
                Some(invalidation) => {
                    roots.push(invalidation);
                    true
                }
                None => false,
            });
        let mut live = Vec::new();
        self.readers.borrow_mut().retain(|(weak, run, owner)| {
            match (
                weak.upgrade().filter(|scope| scope.run() == *run),
                owner.upgrade(),
            ) {
                (Some(scope), Some(owner)) => {
                    live.push((scope, owner));
                    true
                }
                _ => false,
            }
        });
        let laying = live_invalidations(&self.layout_readers);
        let drawing = live_invalidations(&self.draw_readers);
        let composing = |invalidation: &Rc<Invalidation>| {
            roots.iter().any(|root| Rc::ptr_eq(root, invalidation))
                || live
                    .iter()
                    .any(|(_, owner)| Rc::ptr_eq(owner, invalidation))
        };
        for invalidation in laying.iter().filter(|laid| !composing(laid)) {
            invalidation.invalidate_phase(Phase::Layout);
        }
        for invalidation in drawing
            .iter()
            .filter(|drawn| !composing(drawn) && !laying.iter().any(|laid| Rc::ptr_eq(laid, drawn)))
        {
            invalidation.invalidate_phase(Phase::Drawing);
        }
        if roots.is_empty()
            && live.is_empty()
            && laying.is_empty()
            && drawing.is_empty()
            && let Some(invalidation) = &self.invalidation
        {
            invalidation.invalidate();
        }
        for root in &roots {
            root.invalidate();
        }
        for (scope, owner) in &live {
            if !roots.iter().any(|root| Rc::ptr_eq(root, owner)) {
                owner.invalidate_scope(scope);
            }
        }
    }

    /// Remembers that whatever composes now read the value, in the composition it belongs to,
    /// or, outside composition, that the phase a host observes read it.
    fn note_read(&self) {
        if is_untracked() {
            return;
        }
        let reader = scope::reader();
        if matches!(reader, scope::Reader::Nobody) {
            if let Some((phase, invalidation)) = crate::phase::observing() {
                let readers = match phase {
                    Phase::Layout => &self.layout_readers,
                    Phase::Drawing => &self.draw_readers,
                };
                let mut readers = readers.borrow_mut();
                if !readers
                    .iter()
                    .any(|weak| std::ptr::eq(weak.as_ptr(), Rc::as_ptr(&invalidation)))
                {
                    readers.push(Rc::downgrade(&invalidation));
                }
            }
            return;
        }
        let Some(composing) = crate::composition::current_invalidation() else {
            return;
        };
        match reader {
            scope::Reader::Scope(scope) => {
                let run = scope.run();
                let mut readers = self.readers.borrow_mut();
                let known = readers.iter().any(|(weak, seen, _)| {
                    *seen == run && std::ptr::eq(weak.as_ptr(), Rc::as_ptr(&scope))
                });
                if !known {
                    readers.push((Rc::downgrade(&scope), run, Rc::downgrade(&composing)));
                }
            }
            scope::Reader::Root => {
                let mut roots = self.root_readers.borrow_mut();
                if !roots
                    .iter()
                    .any(|weak| std::ptr::eq(weak.as_ptr(), Rc::as_ptr(&composing)))
                {
                    roots.push(Rc::downgrade(&composing));
                }
            }
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
    /// A check that a composition local, named by its address, still has the value read.
    Local(usize, Rc<dyn Fn() -> bool>),
}

impl Read {
    /// What tells this read apart from others of the same thing: the address and version of a
    /// state, or the address of a composition local; nothing for a check, which is kept as is.
    fn key(&self) -> Option<(usize, u64)> {
        match self {
            Self::State(state, version) => {
                Some((Rc::as_ptr(state).cast::<()>() as usize, *version))
            }
            Self::Local(local, _) => Some((*local, u64::MAX)),
            Self::Same(_) => None,
        }
    }

    /// Whether what was read is as it was.
    pub(crate) fn is_current(&self) -> bool {
        match self {
            Self::State(state, version) => state.version() == *version,
            Self::Same(check) | Self::Local(_, check) => check(),
        }
    }
}

/// What one run of a skippable block read.
#[derive(Default)]
pub(crate) struct Recording {
    pub(crate) reads: Vec<Read>,
    /// False once the block read something whose changes are not tracked.
    pub(crate) unskippable: bool,
    /// The states, by address and version, and the composition locals, by address, among
    /// `reads`, so each is noted once however often the block and the blocks it reused read it.
    seen: std::collections::HashSet<(usize, u64), std::hash::BuildHasherDefault<AddressHasher>>,
}

impl Recording {
    /// How many reads a recording holds before it keeps a set of them to look duplicates up
    /// in; fewer are looked through.
    const SCANNED: usize = 24;

    /// Notes `read`, unless it is a state at the same version or a composition local already
    /// noted.
    fn push(&mut self, read: Read) {
        let Some(key) = read.key() else {
            self.reads.push(read);
            return;
        };
        if self.seen.is_empty() && self.reads.len() < Self::SCANNED {
            if self.reads.iter().any(|noted| noted.key() == Some(key)) {
                return;
            }
        } else {
            if self.seen.is_empty() {
                self.seen = self.reads.iter().filter_map(Read::key).collect();
            }
            if !self.seen.insert(key) {
                return;
            }
        }
        self.reads.push(read);
    }
}

/// Hashes addresses and versions by mixing their bits, which is all a set of them needs.
#[derive(Default)]
struct AddressHasher(u64);

impl std::hash::Hasher for AddressHasher {
    /// The mixed bits.
    fn finish(&self) -> u64 {
        self.0
    }

    /// Mixes in `bytes`, eight at a time.
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.write_u64(u64::from_le_bytes(word));
        }
    }

    /// Mixes in `value`.
    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }

    /// Mixes in `value`.
    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }
}

thread_local! {
    static RECORDINGS: RefCell<Vec<Recording>> = const { RefCell::new(Vec::new()) };
    /// How many [`untracked`] blocks are open; while any is, reads are not counted.
    static UNTRACKED: Cell<usize> = const { Cell::new(0) };
}

/// Runs `body` with the state it reads not counted as read by whatever composes,
/// lays out or draws now: a change to that state invalidates nothing on its account.
///
/// For a value that is wanted as it is now but whose every change must not compose
/// the reader again, such as a lazy list's scroll offset, which matters to its
/// composition only when the items in view change.
pub fn untracked<R>(body: impl FnOnce() -> R) -> R {
    struct Leave;
    impl Drop for Leave {
        fn drop(&mut self) {
            UNTRACKED.with(|depth| depth.set(depth.get() - 1));
        }
    }
    UNTRACKED.with(|depth| depth.set(depth.get() + 1));
    let _leave = Leave;
    body()
}

/// Whether an [`untracked`] block is open.
fn is_untracked() -> bool {
    UNTRACKED.with(|depth| depth.get() > 0)
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
    let rest = RECORDINGS.with(|stack| {
        let mut stack = stack.borrow_mut();
        let Some(top) = stack.last_mut() else {
            return reads;
        };
        top.unskippable |= unskippable;
        top.reads.reserve(reads.len());
        let mut noted = Vec::new();
        for read in reads {
            let before = top.reads.len();
            top.push(read);
            if top.reads.len() > before
                && let Some(Read::State(state, _)) = top.reads.last()
            {
                noted.push(Rc::clone(state));
            }
        }
        noted
            .into_iter()
            .map(|state| Read::State(state, 0))
            .collect()
    });
    for read in &rest {
        if let Read::State(state, _) = read {
            state.note_read();
        }
    }
}

/// Notes something the enclosing skippable block read, as a check that it is unchanged.
pub(crate) fn record_check(check: Rc<dyn Fn() -> bool>) {
    RECORDINGS.with(|stack| {
        if let Some(top) = stack.borrow_mut().last_mut() {
            top.push(Read::Same(check));
        }
    });
}

/// Notes that the enclosing skippable block read the composition local at address `local`,
/// as a check that it still has the value read.
pub(crate) fn record_local_check(local: usize, check: Rc<dyn Fn() -> bool>) {
    RECORDINGS.with(|stack| {
        if let Some(top) = stack.borrow_mut().last_mut() {
            top.push(Read::Local(local, check));
        }
    });
}

/// Runs `content`, which a provider of the composition local at address `local` composes
/// with its value, and counts what it read for the enclosing skippable blocks but for the
/// reads of that local: inside `content` it has the provided value whatever it has
/// outside, so a block around the provider must not be checked against it.
pub(crate) fn shadowing_local<R>(local: usize, content: impl FnOnce() -> R) -> R {
    if recording_depth() == 0 {
        return content();
    }
    begin_recording();
    let guard = RecordingGuard {
        depth: recording_depth() - 1,
    };
    let result = content();
    let recording = end_recording();
    drop(guard);
    let reads = recording
        .reads
        .into_iter()
        .filter(|read| !matches!(read, Read::Local(id, _) if *id == local))
        .collect();
    merge_recording(reads, recording.unskippable);
    result
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
        Self::build(value, Some(invalidation), None)
    }

    /// State that takes part in snapshots, which needs to copy its value.
    pub(crate) fn new_snapshotted(value: T, invalidation: Rc<Invalidation>) -> Self
    where
        T: Clone,
    {
        Self::build(value, Some(invalidation), Some(T::clone))
    }

    /// State that belongs to no composition, for [`mutable_state`]. It invalidates
    /// whatever composition read it.
    pub(crate) fn new_detached(value: T) -> Self
    where
        T: Clone,
    {
        Self::build(value, None, Some(T::clone))
    }

    fn build(
        value: T,
        invalidation: Option<Rc<Invalidation>>,
        cloner: Option<fn(&T) -> T>,
    ) -> Self {
        Self {
            inner: Rc::new(Inner {
                value: RefCell::new(value),
                invalidation,
                version: Cell::new(0),
                cloner,
                written: Cell::new(0),
                retained: RefCell::new(Vec::new()),
                readers: RefCell::new(Vec::new()),
                root_readers: RefCell::new(Vec::new()),
                layout_readers: RefCell::new(Vec::new()),
                draw_readers: RefCell::new(Vec::new()),
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
            if is_untracked() {
                return;
            }
            if let Some(top) = stack.borrow_mut().last_mut() {
                let inner: Rc<dyn Versioned> = self.inner.clone();
                top.push(Read::State(inner, self.inner.version.get()));
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

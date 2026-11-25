//! Restartable scopes: blocks that read state and are composed again alone when it changes.
//!
//! A scope remembers what it read. A change to that state marks only the
//! scopes that read it, and the composition runs those again in place, keeping
//! everything around them. State read outside every scope still recomposes the
//! whole root.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::state::{Read, begin_recording, end_recording, mark_untracked};

/// Who is reading state right now.
pub(crate) enum Reader {
    /// A restartable scope running.
    Scope(Rc<ScopeInner>),
    /// A composition pass outside every scope, so the whole root depends on it.
    Root,
    /// Nothing is composing.
    Nobody,
}

thread_local! {
    /// The composing context of each open pass or scope, innermost last. `None` is a pass.
    static READERS: RefCell<Vec<Option<Rc<ScopeInner>>>> = const { RefCell::new(Vec::new()) };
}

/// Who reads state at this moment.
pub(crate) fn reader() -> Reader {
    READERS.with(|stack| match stack.borrow().last() {
        Some(Some(scope)) => Reader::Scope(Rc::clone(scope)),
        Some(None) => Reader::Root,
        None => Reader::Nobody,
    })
}

/// Pushes a reading context and pops it again on drop, also when composing panics.
pub(crate) struct ReaderGuard {
    depth: usize,
}

impl ReaderGuard {
    /// Enters `scope`, or a plain composition pass for `None`.
    pub(crate) fn enter(scope: Option<Rc<ScopeInner>>) -> Self {
        let depth = READERS.with(|stack| {
            let mut stack = stack.borrow_mut();
            stack.push(scope);
            stack.len() - 1
        });
        Self { depth }
    }
}

impl Drop for ReaderGuard {
    fn drop(&mut self) {
        READERS.with(|stack| stack.borrow_mut().truncate(self.depth));
    }
}

/// Runs a scope's content and returns what it produced.
type Content = Rc<dyn Fn() -> Rc<dyn Any>>;

/// What a restartable group remembers.
pub(crate) struct ScopeInner {
    pub(crate) deps: RefCell<Box<dyn Any>>,
    pub(crate) content: RefCell<Option<Content>>,
    output: RefCell<Option<Rc<dyn Any>>>,
    pub(crate) reads: RefCell<Vec<Read>>,
    pub(crate) unskippable: Cell<bool>,
    version: Cell<u64>,
    dirty: Cell<bool>,
    pub(crate) queued: Cell<bool>,
    run: Cell<u64>,
    pub(crate) locals: RefCell<Vec<(usize, Rc<dyn Any>)>>,
    user: RefCell<Option<Box<dyn Any>>>,
}

/// The slot value that identifies a group as a scope.
pub(crate) struct ScopeSlot(pub(crate) Rc<ScopeInner>);

impl ScopeInner {
    /// A scope that has not run yet.
    pub(crate) fn new() -> Rc<Self> {
        Rc::new(Self {
            deps: RefCell::new(Box::new(())),
            content: RefCell::new(None),
            output: RefCell::new(None),
            reads: RefCell::new(Vec::new()),
            unskippable: Cell::new(false),
            version: Cell::new(0),
            dirty: Cell::new(false),
            queued: Cell::new(false),
            run: Cell::new(0),
            locals: RefCell::new(Vec::new()),
            user: RefCell::new(None),
        })
    }

    /// Counts the runs, so subscriptions of an earlier run can be told from current ones.
    pub(crate) fn run(&self) -> u64 {
        self.run.get()
    }

    pub(crate) fn mark_dirty(&self) {
        self.dirty.set(true);
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty.get()
    }

    /// Whether the scope has produced output yet.
    pub(crate) fn has_run(&self) -> bool {
        self.output.borrow().is_some()
    }

    /// Whether the scope can keep its output: nothing it read changed, it was not marked dirty,
    /// and it did not read anything untracked.
    pub(crate) fn is_current(&self) -> bool {
        self.has_run()
            && !self.dirty.get()
            && !self.unskippable.get()
            && self.reads.borrow().iter().all(Read::is_current)
    }

    /// Runs the content now, in the group that is open, and keeps what it produced.
    ///
    /// Returns the checks on locals it read and whether it read something untracked, which
    /// the enclosing block must also take into account.
    pub(crate) fn execute(self: &Rc<Self>) -> (Vec<Read>, bool) {
        self.dirty.set(false);
        self.run.set(self.run.get() + 1);
        let content = self.content.borrow().clone().expect("scope has no content");
        begin_recording();
        let guard = crate::state::RecordingGuard {
            depth: crate::state::recording_depth() - 1,
        };
        let output = {
            let _reader = ReaderGuard::enter(Some(Rc::clone(self)));
            content()
        };
        let recording = end_recording();
        drop(guard);
        *self.output.borrow_mut() = Some(output);
        self.version.set(self.version.get() + 1);
        self.unskippable.set(recording.unskippable);
        let checks = recording
            .reads
            .iter()
            .filter(|read| matches!(read, Read::Same(_) | Read::Local(..)))
            .cloned()
            .collect();
        *self.reads.borrow_mut() = recording.reads;
        (checks, recording.unskippable)
    }
}

/// A handle to a restartable block: its latest output, which changes when only that
/// block is composed again, and a place for a layer above to cache work on it.
#[derive(Clone)]
pub struct Scope(pub(crate) Rc<ScopeInner>);

impl Scope {
    /// Counts the times the block has run; it changes whenever the output does.
    pub fn version(&self) -> u64 {
        self.0.version.get()
    }

    /// The latest output, if the block produces a `T`.
    pub fn output<T: Clone + 'static>(&self) -> Option<T> {
        self.0
            .output
            .borrow()
            .as_ref()
            .and_then(|output| output.downcast_ref::<T>().cloned())
    }

    /// Whether both handles are the same block.
    pub fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    /// A value a layer above keeps with the block, such as work cached for its output.
    /// It lives as long as the block is in the composition.
    pub fn user_data(&self) -> &RefCell<Option<Box<dyn Any>>> {
        &self.0.user
    }

    /// Notes in the enclosing [`track_reads`] that its result depends on this block's
    /// output, so it is stale once the block runs again.
    pub fn record_read(&self) {
        let scope = Rc::clone(&self.0);
        let version = scope.version.get();
        crate::state::record_check(Rc::new(move || scope.version.get() == version));
    }
}

/// What a stretch of code read, which can be checked for having changed since.
#[derive(Clone, Default)]
pub struct Reads {
    reads: Vec<Read>,
    untracked: bool,
}

impl Reads {
    /// Whether everything read is as it was.
    pub fn is_current(&self) -> bool {
        self.reads.iter().all(Read::is_current)
    }

    /// Whether the stretch only read things whose changes are noticed, so its result can be
    /// kept while [`Reads::is_current`] holds. False after something with effects that cannot
    /// be tracked, such as composing a [`Subcomposition`](crate::Subcomposition).
    pub fn is_trackable(&self) -> bool {
        !self.untracked
    }

    /// Counts these reads as made by the enclosing [`track_reads`], for a result reused
    /// without running the code again.
    pub fn merge(&self) {
        crate::state::merge_recording(self.reads.clone(), self.untracked);
    }
}

/// Runs `body` and returns its result with everything it read: state, composition locals
/// and the blocks recorded with [`Scope::record_read`]. The reads also count for any
/// enclosing [`track_reads`] or skippable block.
pub fn track_reads<R>(body: impl FnOnce() -> R) -> (R, Reads) {
    begin_recording();
    let guard = crate::state::RecordingGuard {
        depth: crate::state::recording_depth() - 1,
    };
    let result = body();
    let recording = end_recording();
    drop(guard);
    crate::state::merge_recording(recording.reads.clone(), recording.unskippable);
    (
        result,
        Reads {
            reads: recording.reads,
            untracked: recording.unskippable,
        },
    )
}

/// Marks the enclosing [`track_reads`] as untrackable, for work whose effects cannot be
/// replayed from a cache.
pub fn mark_unskippable() {
    mark_untracked();
}

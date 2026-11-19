//! The composition: a tree of groups holding remembered values, and full-root recomposition.
//!
//! Every composable call site opens a *group* identified by its source
//! location (and by a [`key`], if given). A group owns the values remembered
//! inside it, so state follows its call site: a conditional call enters and
//! leaves the composition without disturbing its siblings. Calls from the same
//! site, such as in a loop, are told apart by their order or by [`key`].
//! A group that is not reached in a pass has left the composition and is
//! dropped, which runs its cleanups.

use std::any::Any;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::mem;
use std::panic::Location;
use std::rc::Rc;

use crate::frame::{FrameClock, FrameQueue};
use crate::phase::PhaseObserver;
use crate::scope::{ReaderGuard, Scope, ScopeInner, ScopeSlot};
use crate::state::{
    Invalidation, Read, RecordingGuard, State, begin_recording, end_recording, mark_untracked,
    merge_recording, recording_depth,
};
use crate::task::Executor;

/// What a composable call site left behind.
#[derive(Default)]
struct Group {
    key: u64,
    /// The value remembered directly by this group, if any.
    slot: Option<Box<dyn Any>>,
    children: VecDeque<Group>,
}

/// A group being composed. The first `unclaimed` children belong to the previous
/// pass; completed children from this pass collect behind them in the same buffer.
struct Frame {
    key: u64,
    slot: Option<Box<dyn Any>>,
    children: VecDeque<Group>,
    unclaimed: usize,
}

impl Frame {
    /// Opens `group`, reusing its child buffer for both pending and completed children.
    fn new(group: Group) -> Self {
        Self {
            key: group.key,
            slot: group.slot,
            unclaimed: group.children.len(),
            children: group.children,
        }
    }

    /// Claims the earliest pending child with `key`, or creates a new empty group.
    /// In-order matches pop the front in constant time; reorders use an ordered
    /// linear search restricted to children not yet claimed during this pass.
    fn claim(&mut self, key: u64) -> Group {
        let index = if self.unclaimed != 0 && self.children.front().is_some_and(|g| g.key == key) {
            Some(0)
        } else {
            self.children
                .iter()
                .take(self.unclaimed)
                .position(|group| group.key == key)
        };
        if let Some(index) = index {
            self.unclaimed -= 1;
            self.children.remove(index).expect("pending child missing")
        } else {
            Group {
                key,
                ..Group::default()
            }
        }
    }

    /// Closes the frame, handing unclaimed children to `removed` unless the pass
    /// was cut short. Completed children precede preserved pending children.
    fn finish(mut self, keep_unclaimed: bool, removed: &mut Vec<Group>) -> Group {
        if keep_unclaimed {
            self.children.rotate_left(self.unclaimed);
        } else {
            for _ in 0..self.unclaimed {
                removed.push(self.children.pop_front().expect("pending child missing"));
            }
        }
        Group {
            key: self.key,
            slot: self.slot,
            children: self.children,
        }
    }
}

/// State of the pass in progress.
pub(crate) struct Runtime {
    frames: Vec<Frame>,
    /// Effects to run once the pass has finished.
    effects: Vec<Box<dyn FnOnce()>>,
    /// Groups that left the composition. Dropped after the pass, outside any borrow.
    removed: Vec<Group>,
    /// Values provided to composition locals, innermost last.
    pub(crate) locals: Vec<(usize, Rc<dyn Any>)>,
    pub(crate) invalidation: Rc<Invalidation>,
    clock: Rc<FrameQueue>,
    pub(crate) executor: Rc<Executor>,
}

impl Runtime {
    /// The slot of the group being composed.
    pub(crate) fn slot(&mut self) -> &mut Option<Box<dyn Any>> {
        &mut self.frames.last_mut().expect("no open group").slot
    }

    pub(crate) fn defer(&mut self, effect: Box<dyn FnOnce()>) {
        self.effects.push(effect);
    }
}

thread_local! {
    static RUNTIME: RefCell<Option<Runtime>> = const { RefCell::new(None) };
}

/// Whether a composition pass is running on this thread, so composables and
/// [`CompositionLocal::current`](crate::CompositionLocal::current) can be called. Code
/// that also runs outside composition, such as measuring text for a host, checks it to
/// fall back to its own defaults.
pub fn is_composing() -> bool {
    RUNTIME.with(|runtime| {
        runtime
            .try_borrow()
            .map_or(true, |runtime| runtime.is_some())
    })
}

/// The invalidation of the composition whose pass is in progress, if one is.
pub(crate) fn current_invalidation() -> Option<Rc<Invalidation>> {
    RUNTIME.with(|runtime| {
        runtime
            .try_borrow()
            .ok()?
            .as_ref()
            .map(|runtime| Rc::clone(&runtime.invalidation))
    })
}

/// Runs `access` on the pass in progress. Never run user code inside it.
pub(crate) fn with_runtime<R>(access: impl FnOnce(&mut Runtime) -> R) -> R {
    RUNTIME.with(|runtime| {
        let mut runtime = runtime.borrow_mut();
        let runtime = runtime
            .as_mut()
            .expect("composables can only be called during composition");
        access(runtime)
    })
}

/// Reinstates the previously active runtime, also when the root panics, and
/// hands the finished pass back to its composition.
struct PassGuard<'a> {
    previous: Option<Runtime>,
    tree: &'a mut Group,
    effects: &'a mut Vec<Box<dyn FnOnce()>>,
    removed: &'a mut Vec<Group>,
}

impl Drop for PassGuard<'_> {
    fn drop(&mut self) {
        let finished = RUNTIME.with(|runtime| runtime.replace(self.previous.take()));
        if let Some(mut runtime) = finished {
            let root = runtime.frames.pop().expect("root group missing");
            *self.tree = root.finish(std::thread::panicking(), &mut runtime.removed);
            *self.effects = mem::take(&mut runtime.effects);
            *self.removed = mem::take(&mut runtime.removed);
        }
    }
}

/// A root composable together with everything it remembered.
///
/// Every pass runs the whole root again and keeps what it returned, replacing
/// the previous output. The runtime does not care what that is; a UI layer
/// returns its tree. State changes only mark the composition dirty; call
/// [`Composition::recompose`] to apply them.
pub struct Composition<T> {
    root: Box<dyn FnMut() -> T>,
    tree: Group,
    invalidation: Rc<Invalidation>,
    frames: Rc<FrameQueue>,
    executor: Rc<Executor>,
    output: Option<T>,
}

impl<T> Composition<T> {
    pub fn new(root: impl FnMut() -> T + 'static) -> Self {
        Self {
            root: Box::new(root),
            tree: Group::default(),
            invalidation: Invalidation::new(),
            frames: Rc::default(),
            executor: Rc::default(),
            output: None,
        }
    }

    /// Registers a callback run when the composition becomes dirty.
    ///
    /// It runs once per clean-to-dirty transition, so repeated state updates
    /// before the next recomposition notify only once. It must not touch state.
    pub fn on_invalidate(&mut self, waker: impl Fn() + 'static) {
        self.invalidation.set_waker(waker);
    }

    /// Registers a callback run when something first asks for a frame, such as
    /// an animation. The host should then call [`Composition::run_frame`] at
    /// its next frame. It must not touch state.
    pub fn on_frame_request(&mut self, waker: impl Fn() + 'static) {
        self.frames.set_waker(waker);
    }

    /// True if something asked for a frame that has not run yet.
    pub fn has_pending_frame(&self) -> bool {
        self.frames.has_pending()
    }

    /// Runs the frame callbacks requested so far, with `now` as the time since
    /// the host started its clock. They typically change state, which marks
    /// the composition dirty. Callbacks that request again wait for the next frame.
    pub fn run_frame(&mut self, now: std::time::Duration) {
        self.frames.run(now);
        self.executor.poll_woken();
    }

    /// Gives the composition the host's clock: the time since the host started it, the
    /// same clock [`run_frame`](Composition::run_frame) is given. A task's
    /// [`delay`](crate::TaskScope::delay) is then measured on it from when the task waits,
    /// and waits on a timer instead of on frames, so a host does not draw while a task
    /// waits. Without it, a delay starts at the next frame.
    pub fn set_time_source(&mut self, source: impl Fn() -> std::time::Duration + 'static) {
        self.frames.set_time_source(source);
    }

    /// The earliest time on the host's clock a task waits for with
    /// [`delay`](crate::TaskScope::delay), if one waits. The host calls
    /// [`run_timers`](Composition::run_timers) once its clock reaches it, as Compose's
    /// dispatcher resumes a coroutine whose `delay` ended.
    pub fn next_timer(&self) -> Option<std::time::Duration> {
        self.frames.next_timer()
    }

    /// Resumes the tasks whose delays ended by `now` on the host's clock, without running
    /// a frame. Returns whether any did; what they changed marks the composition dirty.
    pub fn run_timers(&mut self, now: std::time::Duration) -> bool {
        let fired = self.frames.run_timers(now);
        if fired {
            self.executor.poll_woken();
        }
        fired
    }

    /// A handle through which the host lays out and draws this composition's output, so that
    /// state read only while laying out or drawing asks for a layout or a drawing when it
    /// changes, instead of composing the root again (see [`PhaseObserver`]).
    pub fn phase_observer(&self) -> PhaseObserver {
        PhaseObserver::new(Rc::clone(&self.invalidation))
    }

    /// Marks the composition dirty, so the next [`Composition::recompose`]
    /// runs. For changes the host knows about that no state records.
    pub fn invalidate(&self) {
        self.invalidation.invalidate();
    }

    /// What the latest pass returned, or `None` before the first pass.
    pub fn output(&self) -> Option<&T> {
        self.output.as_ref()
    }

    /// True if state changed since the latest pass, or none has run yet.
    pub fn is_dirty(&self) -> bool {
        self.invalidation.is_dirty()
    }

    /// Runs the root unconditionally and replaces the output.
    ///
    /// Groups the root did not reach are dropped, then the effects it
    /// scheduled run. Changing state while composing marks the composition
    /// dirty again.
    pub fn compose(&mut self) {
        self.invalidation.clear();
        let tree = mem::take(&mut self.tree);
        let runtime = Runtime {
            frames: vec![Frame::new(tree)],
            effects: Vec::new(),
            removed: Vec::new(),
            locals: Vec::new(),
            invalidation: Rc::clone(&self.invalidation),
            clock: Rc::clone(&self.frames),
            executor: Rc::clone(&self.executor),
        };
        let previous = RUNTIME.with(|current| current.replace(Some(runtime)));
        let mut effects = Vec::new();
        let mut removed = Vec::new();
        let guard = PassGuard {
            previous,
            tree: &mut self.tree,
            effects: &mut effects,
            removed: &mut removed,
        };

        let output = {
            let _reader = ReaderGuard::enter(None);
            (self.root)()
        };

        drop(guard);
        self.output = Some(output);
        drop(removed);
        for effect in effects {
            effect();
        }
        self.executor.poll_woken();
        self.run_scopes();
    }

    /// Composes again if dirty. Returns whether a pass ran.
    ///
    /// When only state read inside [`restartable`] blocks changed, just those blocks run
    /// again, in place; the root and everything around them is kept. State the root read
    /// itself, or that nothing composing is known to have read, runs the root.
    pub fn recompose(&mut self) -> bool {
        if !self.is_dirty() {
            return false;
        }
        if self.invalidation.root_dirty() {
            self.compose();
        } else {
            self.invalidation.clear();
            self.run_scopes();
        }
        true
    }

    /// Runs the scopes whose state changed and that nothing has composed since.
    fn run_scopes(&mut self) {
        for weak in self.invalidation.take_queue() {
            let Some(scope) = weak.upgrade() else {
                continue;
            };
            scope.queued.set(false);
            if scope.is_dirty() {
                self.rerun(&scope);
            }
        }
    }

    /// Runs `scope` again inside its own group, leaving the rest of the tree alone. A scope
    /// that is no longer in the tree is skipped.
    fn rerun(&mut self, scope: &Rc<ScopeInner>) {
        let mut path = Vec::new();
        if !find_scope(&self.tree, scope, &mut path) {
            return;
        }
        let mut parent = &mut self.tree;
        for index in &path {
            parent = &mut parent.children[*index];
        }
        let group = mem::take(parent);
        let runtime = Runtime {
            frames: vec![Frame::new(group)],
            effects: Vec::new(),
            removed: Vec::new(),
            locals: scope.locals.borrow().clone(),
            invalidation: Rc::clone(&self.invalidation),
            clock: Rc::clone(&self.frames),
            executor: Rc::clone(&self.executor),
        };
        let previous = RUNTIME.with(|current| current.replace(Some(runtime)));
        let mut effects = Vec::new();
        let mut removed = Vec::new();
        let mut detached = Group::default();
        let guard = PassGuard {
            previous,
            tree: &mut detached,
            effects: &mut effects,
            removed: &mut removed,
        };
        scope.execute();
        drop(guard);
        let mut parent = &mut self.tree;
        for index in &path {
            parent = &mut parent.children[*index];
        }
        *parent = detached;
        drop(removed);
        for effect in effects {
            effect();
        }
        self.executor.poll_woken();
    }
}

/// Finds the group of `scope` below `group`, as the child indices leading to it.
fn find_scope(group: &Group, scope: &Rc<ScopeInner>, path: &mut Vec<usize>) -> bool {
    for (index, child) in group.children.iter().enumerate() {
        path.push(index);
        let is_scope = child
            .slot
            .as_ref()
            .and_then(|slot| slot.downcast_ref::<ScopeSlot>())
            .is_some_and(|slot| Rc::ptr_eq(&slot.0, scope));
        if is_scope || find_scope(child, scope, path) {
            return true;
        }
        path.pop();
    }
    false
}

fn site_key(site: &Location<'_>) -> u64 {
    let mut hasher = DefaultHasher::new();
    (site.file(), site.line(), site.column()).hash(&mut hasher);
    hasher.finish()
}

/// Closes the group opened by [`run_group`], also when `content` panics.
struct LeaveGuard;

impl Drop for LeaveGuard {
    /// Returns the completed group to its parent, preserving pending children on panic.
    fn drop(&mut self) {
        let keep = std::thread::panicking();
        with_runtime(|runtime| {
            let frame = runtime.frames.pop().expect("no open group");
            let group = frame.finish(keep, &mut runtime.removed);
            runtime
                .frames
                .last_mut()
                .expect("root group missing")
                .children
                .push_back(group);
        });
    }
}

/// Runs `content` in the earliest matching pending group and closes it on unwind.
///
/// # Panics
///
/// Panics outside a composition pass and propagates panics from `content`.
fn run_group<R>(key: u64, content: impl FnOnce() -> R) -> R {
    with_runtime(|runtime| {
        let parent = runtime.frames.last_mut().expect("no open group");
        let group = parent.claim(key);
        runtime.frames.push(Frame::new(group));
    });
    let _leave = LeaveGuard;
    content()
}

/// Runs `content` in a group identified by the call site of this function.
///
/// `#[composable]` wraps a function's body in one. Calling the function from
/// two sites gives two groups; calling it repeatedly from one site, such as in
/// a loop, matches the groups by order unless [`key`] tells them apart.
#[track_caller]
pub fn group<R>(content: impl FnOnce() -> R) -> R {
    run_group(site_key(Location::caller()), content)
}

/// Runs `content` in a group identified by the call site and by `key`.
///
/// Use it when a loop calls composables that remember values or run effects,
/// and its items can be inserted, removed or reordered: each item keeps its
/// state wherever it moves. A key only has to be unique among the calls from
/// the same site.
#[track_caller]
pub fn key<R>(key: impl Hash, content: impl FnOnce() -> R) -> R {
    let mut hasher = DefaultHasher::new();
    (site_key(Location::caller()), key).hash(&mut hasher);
    run_group(hasher.finish(), content)
}

/// What a skippable block remembers: what it was given, what it read, and what it produced.
struct Memo<D, T> {
    deps: D,
    reads: Vec<Read>,
    output: T,
}

/// Runs `content` and returns what it produced, unless it can be skipped:
/// when `deps` equals what it was last given, and none of the state it read
/// has changed since, `content` does not run, nothing it composed is
/// disturbed, and its previous output is returned instead.
///
/// Composition locals are checked against the values read during the previous
/// run. Effects inside a skipped block do not run again, and what it remembered
/// is kept.
#[track_caller]
pub fn memo<D: PartialEq + 'static, T: Clone + 'static>(deps: D, content: impl FnOnce() -> T) -> T {
    run_group(site_key(Location::caller()), || {
        let candidate = with_runtime(|runtime| {
            runtime
                .slot()
                .as_ref()
                .and_then(|slot| slot.downcast_ref::<Memo<D, T>>())
                .filter(|memo| memo.deps == deps)
                .map(|memo| (memo.output.clone(), memo.reads.clone()))
        });
        let cached = candidate.filter(|(_, reads)| reads.iter().all(Read::is_current));
        if let Some((output, reads)) = cached {
            with_runtime(|runtime| {
                let frame = runtime.frames.last_mut().expect("no open group");
                frame.unclaimed = 0;
            });
            merge_recording(reads, false);
            return output;
        }

        begin_recording();
        let _guard = RecordingGuard {
            depth: recording_depth() - 1,
        };
        let output = content();
        let recording = end_recording();
        with_runtime(|runtime| {
            *runtime.slot() = (!recording.unskippable).then(|| {
                Box::new(Memo {
                    deps,
                    reads: recording.reads.clone(),
                    output: output.clone(),
                }) as Box<dyn Any>
            });
        });
        merge_recording(recording.reads, recording.unskippable);
        output
    })
}

/// Runs `content` as a restartable block and returns a handle to what it produced.
///
/// The block remembers what it read. When state it read changes, the composition runs
/// only this block again, keeping the root and everything around it, and the handle
/// shows the new output ([`Scope::version`] changes). When the surrounding code runs it
/// again with equal `deps`, and nothing it read has changed, it is not run: what it
/// remembered is kept and the output is the same. Pass everything `content` takes from
/// outside in `deps`.
///
/// Inside a restartable block, state is read by the block alone; state read directly
/// by the root still recomposes the root. Effects inside a block that is not run again
/// do not run again.
///
/// # Panics
///
/// Panics when called outside a composition pass.
#[track_caller]
pub fn restartable<D: PartialEq + 'static, T: Clone + 'static>(
    deps: D,
    content: impl Fn() -> T + 'static,
) -> Scope {
    run_group(site_key(Location::caller()), || {
        let existing = with_runtime(|runtime| {
            runtime
                .slot()
                .as_ref()
                .and_then(|slot| slot.downcast_ref::<ScopeSlot>())
                .map(|slot| Rc::clone(&slot.0))
        });
        let scope = existing.unwrap_or_else(|| {
            let scope = ScopeInner::new();
            let stored = Rc::clone(&scope);
            with_runtime(|runtime| *runtime.slot() = Some(Box::new(ScopeSlot(stored))));
            scope
        });
        let unchanged = scope
            .deps
            .borrow()
            .downcast_ref::<D>()
            .is_some_and(|previous| *previous == deps);
        if unchanged && scope.is_current() {
            with_runtime(|runtime| {
                runtime.frames.last_mut().expect("no open group").unclaimed = 0;
            });
            let checks = scope
                .reads
                .borrow()
                .iter()
                .filter(|read| matches!(read, Read::Same(_)))
                .cloned()
                .collect();
            merge_recording(checks, false);
            return Scope(scope);
        }
        *scope.deps.borrow_mut() = Box::new(deps);
        *scope.content.borrow_mut() = Some(Rc::new(move || Rc::new(content()) as Rc<dyn Any>));
        with_runtime(|runtime| *scope.locals.borrow_mut() = runtime.locals.clone());
        let (checks, unskippable) = scope.execute();
        merge_recording(checks, unskippable);
        Scope(scope)
    })
}

/// Returns the state remembered at this call site, creating it with `init` on
/// the first composition.
///
/// The state takes part in snapshots, like [`mutable_state_of`]: inside a
/// [`Snapshot`](crate::Snapshot) it reads as the snapshot saw it, and inside a
/// mutable snapshot its writes are kept until the snapshot is applied. Values that
/// cannot be cloned use [`remember_live`].
///
/// The value lives as long as the call site stays in the composition. If the
/// call is skipped in a pass, for example in an untaken `if`, its state is
/// dropped and `init` runs again when the call returns.
///
/// # Panics
///
/// Panics when called outside a composition pass.
#[track_caller]
pub fn remember<T: Clone + 'static>(init: impl FnOnce() -> T) -> State<T> {
    remember_with(Location::caller(), |invalidation| {
        State::new_snapshotted(init(), invalidation)
    })
}

/// Like [`remember`], for a value that cannot be cloned. The state is always read and
/// written live: it does not take part in snapshots.
///
/// # Panics
///
/// Panics when called outside a composition pass.
#[track_caller]
pub fn remember_live<T: 'static>(init: impl FnOnce() -> T) -> State<T> {
    remember_with(Location::caller(), |invalidation| {
        State::new(init(), invalidation)
    })
}

/// Like [`remember`], but the state takes part in snapshots: inside a
/// [`Snapshot`](crate::Snapshot) it reads as the snapshot saw it, and inside a
/// mutable snapshot its writes are kept until the snapshot is applied.
///
/// # Panics
///
/// Panics when called outside a composition pass.
#[track_caller]
pub fn mutable_state_of<T: Clone + 'static>(initial: T) -> State<T> {
    remember_with(Location::caller(), |invalidation| {
        State::new_snapshotted(initial, invalidation)
    })
}

/// A state that belongs to no composition, like Compose's `mutableStateOf` called
/// outside `remember`: for state a state holder, such as a view model, keeps outside
/// the tree and hands to the composables that show it.
///
/// It can be made and written anywhere on the thread, also before any composition
/// exists. Every composition that reads it is invalidated when it is written: the whole
/// root of one whose root read it, the reading `restartable` scopes of the others. It
/// takes part in snapshots like [`mutable_state_of`].
pub fn mutable_state<T: Clone + 'static>(initial: T) -> State<T> {
    State::new_detached(initial)
}

/// Returns the state remembered at `site`, making it with `make` the first time.
fn remember_with<T: 'static>(
    site: &Location<'_>,
    make: impl FnOnce(Rc<Invalidation>) -> State<T>,
) -> State<T> {
    run_group(site_key(site), || {
        let existing = with_runtime(|runtime| {
            runtime
                .slot()
                .as_ref()
                .and_then(|slot| slot.downcast_ref::<State<T>>())
                .cloned()
        });
        if let Some(state) = existing {
            return state;
        }
        let invalidation = with_runtime(|runtime| Rc::clone(&runtime.invalidation));
        let state = make(invalidation);
        let stored = state.clone();
        with_runtime(|runtime| *runtime.slot() = Some(Box::new(stored)));
        state
    })
}

/// Runs `content` in a group of its own, for the effects module.
pub(crate) fn group_at<R>(site: &Location<'_>, content: impl FnOnce() -> R) -> R {
    run_group(site_key(site), content)
}

/// The frame clock of the composition being composed.
///
/// # Panics
///
/// Panics when called outside a composition pass.
pub fn frame_clock() -> FrameClock {
    FrameClock::new(with_runtime(|runtime| Rc::clone(&runtime.clock)))
}

/// The frame clock and the executor of the composition being composed.
pub(crate) fn clock_and_executor() -> (FrameClock, Rc<Executor>) {
    with_runtime(|runtime| {
        (
            FrameClock::new(Rc::clone(&runtime.clock)),
            Rc::clone(&runtime.executor),
        )
    })
}

/// A composition of its own that a composable owns and runs on demand, outside
/// a pass, such as while a layout measures.
///
/// It keeps what its content remembers between runs, like the root does, and
/// shares the state invalidation, frame clock and executor of the composition
/// it was made in, so state changed inside it recomposes the whole interface.
/// Composition locals are copied from the pass that last called
/// [`Subcomposition::capture`].
pub struct Subcomposition {
    tree: RefCell<Group>,
    locals: RefCell<Vec<(usize, Rc<dyn Any>)>>,
    invalidation: Rc<Invalidation>,
    clock: Rc<FrameQueue>,
    executor: Rc<Executor>,
}

impl Subcomposition {
    /// Creates an empty subcomposition tied to the composition being composed.
    ///
    /// # Panics
    ///
    /// Panics when called outside a composition pass.
    pub fn new() -> Self {
        with_runtime(|runtime| Self {
            tree: RefCell::new(Group::default()),
            locals: RefCell::new(runtime.locals.clone()),
            invalidation: Rc::clone(&runtime.invalidation),
            clock: Rc::clone(&runtime.clock),
            executor: Rc::clone(&runtime.executor),
        })
    }

    /// Creates an empty subcomposition that shares this one's composition
    /// and locals. Unlike [`Subcomposition::new`] it can be called outside a
    /// pass, so a layout can start one while it measures.
    pub fn fork(&self) -> Self {
        Self {
            tree: RefCell::new(Group::default()),
            locals: RefCell::new(self.locals.borrow().clone()),
            invalidation: Rc::clone(&self.invalidation),
            clock: Rc::clone(&self.clock),
            executor: Rc::clone(&self.executor),
        }
    }

    /// Copies the composition locals in effect now, so content run later sees
    /// the values of the place that owns the subcomposition.
    ///
    /// # Panics
    ///
    /// Panics when called outside a composition pass.
    pub fn capture(&self) {
        *self.locals.borrow_mut() = with_runtime(|runtime| runtime.locals.clone());
    }

    /// Runs `content` as a pass of this subcomposition and returns what it
    /// returned. What it remembered in earlier runs is still there, groups it
    /// does not reach are dropped, and its effects run when it has finished.
    ///
    /// # Panics
    ///
    /// Propagates a panic from `content`. Running this subcomposition from
    /// inside its own content panics.
    pub fn compose<R>(&self, content: impl FnOnce() -> R) -> R {
        let mut tree = self.tree.borrow_mut();
        let runtime = Runtime {
            frames: vec![Frame::new(mem::take(&mut *tree))],
            effects: Vec::new(),
            removed: Vec::new(),
            locals: self.locals.borrow().clone(),
            invalidation: Rc::clone(&self.invalidation),
            clock: Rc::clone(&self.clock),
            executor: Rc::clone(&self.executor),
        };
        let previous = RUNTIME.with(|current| current.replace(Some(runtime)));
        let mut effects = Vec::new();
        let mut removed = Vec::new();
        let guard = PassGuard {
            previous,
            tree: &mut tree,
            effects: &mut effects,
            removed: &mut removed,
        };
        mark_untracked();
        let output = {
            let _reader = ReaderGuard::enter(None);
            content()
        };
        drop(guard);
        drop(tree);
        drop(removed);
        for effect in effects {
            effect();
        }
        self.executor.poll_woken();
        output
    }
}

impl Default for Subcomposition {
    /// Creates an empty subcomposition; see [`Subcomposition::new`].
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::*;
    use crate::{CompositionLocal, disposable_effect, side_effect};

    /// A tracked local used to check memo invalidation and provider unwinding.
    static LOCAL_VALUE: CompositionLocal<usize> = CompositionLocal::new(|| 0);

    /// Returns a unique number remembered by the current occurrence of this call.
    fn remembered_id(next: &Cell<usize>) -> usize {
        remember(|| {
            let id = next.get();
            next.set(id + 1);
            id
        })
        .get()
    }

    /// A composition pass is running only while the root runs.
    #[test]
    fn composing_is_known_only_inside_a_pass() {
        assert!(!is_composing());
        let seen = Rc::new(Cell::new(false));
        let inside = Rc::clone(&seen);
        let mut composition = Composition::new(move || inside.set(is_composing()));
        composition.compose();
        assert!(seen.get());
        assert!(!is_composing());
    }

    /// Checks that in-order traversal retains the existing child allocation.
    #[test]
    fn in_order_claims_reuse_the_child_buffer() {
        let count = 2048;
        let children: Vec<Group> = (0..count)
            .map(|key| Group {
                key,
                ..Group::default()
            })
            .collect();
        let mut group = Group {
            children: children.into(),
            ..Group::default()
        };
        let capacity = group.children.capacity();
        let start = group.children.as_slices().0.as_ptr() as usize;
        let end = start + capacity * mem::size_of::<Group>();
        for _ in 0..8 {
            let mut frame = Frame::new(group);
            for key in 0..count {
                let child = frame.claim(key);
                assert_eq!(child.key, key);
                frame.children.push_back(child);
            }
            assert_eq!(frame.unclaimed, 0);
            let mut removed = Vec::new();
            group = frame.finish(false, &mut removed);
            assert!(removed.is_empty());
            assert_eq!(group.children.capacity(), capacity);
            assert!(group.children.iter().all(|child| {
                let address = std::ptr::from_ref(child) as usize;
                (start..end).contains(&address)
            }));
        }
    }

    /// Checks ordered matching against a simple removal model, including duplicates.
    #[test]
    fn reordered_duplicate_claims_match_the_ordered_model() {
        for encoded in 0..256 {
            let mut expected = vec![(1, 0usize), (2, 1), (1, 2), (3, 3)];
            let mut frame = Frame::new(Group {
                children: expected
                    .iter()
                    .map(|&(key, id)| Group {
                        key,
                        slot: Some(Box::new(id)),
                        ..Group::default()
                    })
                    .collect(),
                ..Group::default()
            });
            let mut code = encoded;
            let mut completed = Vec::new();
            for _ in 0..4 {
                let key = code % 4;
                code /= 4;
                let expected_id = expected
                    .iter()
                    .position(|&(old_key, _)| old_key == key)
                    .map(|index| expected.remove(index).1);
                let child = frame.claim(key);
                let actual_id = child
                    .slot
                    .as_ref()
                    .map(|slot| *slot.downcast_ref::<usize>().expect("remembered id missing"));
                assert_eq!(actual_id, expected_id);
                completed.push((key, expected_id));
                frame.children.push_back(child);
            }
            let mut removed = Vec::new();
            let group = frame.finish(false, &mut removed);
            let actual: Vec<_> = group
                .children
                .iter()
                .map(|child| {
                    (
                        child.key,
                        child.slot.as_ref().map(|slot| {
                            *slot.downcast_ref::<usize>().expect("remembered id missing")
                        }),
                    )
                })
                .collect();
            assert_eq!(actual, completed);
            let removed: Vec<_> = removed
                .iter()
                .map(|child| {
                    (
                        child.key,
                        *child
                            .slot
                            .as_ref()
                            .and_then(|slot| slot.downcast_ref::<usize>())
                            .expect("removed id missing"),
                    )
                })
                .collect();
            assert_eq!(removed, expected);
        }
    }

    /// Checks duplicate keys, new occurrences, removal, and repeated unkeyed sites.
    #[test]
    fn keyed_and_same_site_occurrences_keep_their_identity() {
        let keys = Rc::new(RefCell::new(vec![1, 2, 1, 3]));
        let count = Rc::new(Cell::new(3));
        let mut composition = Composition::new({
            let keys = Rc::clone(&keys);
            let count = Rc::clone(&count);
            let next = Cell::new(0);
            move || {
                let keyed: Vec<_> = keys
                    .borrow()
                    .iter()
                    .map(|&id| key(id, || remembered_id(&next)))
                    .collect();
                let positional: Vec<_> = (0..count.get())
                    .map(|_| group(|| remembered_id(&next)))
                    .collect();
                (keyed, positional)
            }
        });
        composition.compose();
        assert_eq!(
            composition.output(),
            Some(&(vec![0, 1, 2, 3], vec![4, 5, 6]))
        );
        *keys.borrow_mut() = vec![3, 1, 4, 1, 1];
        count.set(2);
        composition.compose();
        assert_eq!(
            composition.output(),
            Some(&(vec![3, 0, 7, 2, 8], vec![4, 5]))
        );
        *keys.borrow_mut() = vec![1, 3, 1];
        count.set(3);
        composition.compose();
        assert_eq!(composition.output(), Some(&(vec![0, 3, 2], vec![4, 5, 9])));
    }

    /// Checks that departing effects clean up in old order, outside runtime borrows.
    #[test]
    fn removed_siblings_clean_up_outside_borrows() {
        let keys = Rc::new(RefCell::new(vec![1, 2, 1, 3]));
        let cleanups = Rc::new(RefCell::new(Vec::new()));
        let mut composition = Composition::new({
            let keys = Rc::clone(&keys);
            let cleanups = Rc::clone(&cleanups);
            let next = Cell::new(0);
            move || {
                for &id in keys.borrow().iter() {
                    key(id, || {
                        let occurrence = remembered_id(&next);
                        let cleanups = Rc::clone(&cleanups);
                        disposable_effect((), move || {
                            move || {
                                RUNTIME.with(|runtime| assert!(runtime.try_borrow_mut().is_ok()));
                                let mut nested =
                                    Composition::new(|| group(|| remember(|| 7).get()));
                                nested.compose();
                                assert_eq!(nested.output(), Some(&7));
                                cleanups.borrow_mut().push(occurrence);
                            }
                        });
                    });
                }
            }
        });
        composition.compose();
        *keys.borrow_mut() = vec![3, 1];
        composition.compose();
        assert_eq!(*cleanups.borrow(), vec![1, 2]);
        keys.borrow_mut().clear();
        composition.compose();
        assert_eq!(*cleanups.borrow(), vec![1, 2, 3, 0]);
    }

    /// Checks skipped children survive and propagate nested state and local reads.
    #[test]
    fn skipped_nested_memos_preserve_children_reads_and_effects() {
        let state = State::new(10, Invalidation::new());
        let local = Rc::new(Cell::new(20));
        let outer_dep = Rc::new(Cell::new(0));
        let outer_runs = Rc::new(Cell::new(0));
        let inner_runs = Rc::new(Cell::new(0));
        let effects = Rc::new(Cell::new(0));
        let cleanups = Rc::new(Cell::new(0));
        let mut composition = Composition::new({
            let state = state.clone();
            let local = Rc::clone(&local);
            let outer_dep = Rc::clone(&outer_dep);
            let outer_runs = Rc::clone(&outer_runs);
            let inner_runs = Rc::clone(&inner_runs);
            let effects = Rc::clone(&effects);
            let cleanups = Rc::clone(&cleanups);
            move || {
                LOCAL_VALUE.provide(local.get(), || {
                    memo(outer_dep.get(), || {
                        outer_runs.set(outer_runs.get() + 1);
                        memo((), || {
                            inner_runs.set(inner_runs.get() + 1);
                            let remembered = remember(|| 100).get();
                            let effects = Rc::clone(&effects);
                            side_effect(move || effects.set(effects.get() + 1));
                            let cleanups = Rc::clone(&cleanups);
                            disposable_effect((), move || move || cleanups.set(cleanups.get() + 1));
                            (remembered, state.get(), LOCAL_VALUE.current())
                        })
                    })
                })
            }
        });
        composition.compose();
        composition.compose();
        assert_eq!(
            (outer_runs.get(), inner_runs.get(), effects.get()),
            (1, 1, 1)
        );
        outer_dep.set(1);
        composition.compose();
        assert_eq!(
            (outer_runs.get(), inner_runs.get(), effects.get()),
            (2, 1, 1)
        );
        state.set(11);
        composition.compose();
        assert_eq!(composition.output(), Some(&(100, 11, 20)));
        assert_eq!(
            (outer_runs.get(), inner_runs.get(), effects.get()),
            (3, 2, 2)
        );
        local.set(21);
        composition.compose();
        assert_eq!(composition.output(), Some(&(100, 11, 21)));
        assert_eq!(
            (outer_runs.get(), inner_runs.get(), effects.get()),
            (4, 3, 3)
        );
        assert_eq!(cleanups.get(), 0);
        drop(composition);
        assert_eq!(cleanups.get(), 1);
    }

    /// Checks skipped keyed subtrees remain distinct across duplicate reorders and removal.
    #[test]
    fn skipped_keyed_subtrees_keep_duplicate_occurrences() {
        let keys = Rc::new(RefCell::new(vec![1, 2, 1]));
        let runs = Rc::new(Cell::new(0));
        let cleanups = Rc::new(RefCell::new(Vec::new()));
        let mut composition = Composition::new({
            let keys = Rc::clone(&keys);
            let runs = Rc::clone(&runs);
            let cleanups = Rc::clone(&cleanups);
            let next = Cell::new(0);
            move || {
                keys.borrow()
                    .iter()
                    .map(|&id| {
                        key(id, || {
                            memo((), || {
                                runs.set(runs.get() + 1);
                                let occurrence = group(|| remembered_id(&next));
                                let cleanups = Rc::clone(&cleanups);
                                disposable_effect((), move || {
                                    move || cleanups.borrow_mut().push(occurrence)
                                });
                                occurrence
                            })
                        })
                    })
                    .collect::<Vec<_>>()
            }
        });
        composition.compose();
        assert_eq!(composition.output(), Some(&vec![0, 1, 2]));
        *keys.borrow_mut() = vec![2, 1, 1];
        composition.compose();
        assert_eq!(composition.output(), Some(&vec![1, 0, 2]));
        assert_eq!(runs.get(), 3);
        *keys.borrow_mut() = vec![1];
        composition.compose();
        assert_eq!(composition.output(), Some(&vec![0]));
        assert_eq!(*cleanups.borrow(), vec![1, 2]);
        *keys.borrow_mut() = vec![1, 1];
        composition.compose();
        assert_eq!(composition.output(), Some(&vec![0, 3]));
        assert_eq!(runs.get(), 4);
    }

    /// Checks panic unwinding preserves unvisited siblings, effects, and local scopes.
    #[test]
    fn panic_preserves_pending_groups_and_restores_the_runtime() {
        let keys = Rc::new(RefCell::new(vec![1, 2, 1, 3]));
        let should_panic = Rc::new(Cell::new(false));
        let cleanups = Rc::new(RefCell::new(Vec::new()));
        let mut composition = Composition::new({
            let keys = Rc::clone(&keys);
            let should_panic = Rc::clone(&should_panic);
            let cleanups = Rc::clone(&cleanups);
            let next = Cell::new(0);
            move || {
                LOCAL_VALUE.provide(9, || {
                    group(|| {
                        keys.borrow()
                            .iter()
                            .enumerate()
                            .map(|(index, &id)| {
                                key(id, || {
                                    assert_eq!(LOCAL_VALUE.current(), 9);
                                    let occurrence = remembered_id(&next);
                                    let cleanups = Rc::clone(&cleanups);
                                    disposable_effect((), move || {
                                        move || {
                                            RUNTIME.with(|runtime| {
                                                assert!(runtime.try_borrow_mut().is_ok());
                                            });
                                            cleanups.borrow_mut().push(occurrence);
                                        }
                                    });
                                    assert!(
                                        !(should_panic.get() && index == 0),
                                        "interrupted pass"
                                    );
                                    occurrence
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                })
            }
        });
        composition.compose();
        assert_eq!(composition.output(), Some(&vec![0, 1, 2, 3]));
        *keys.borrow_mut() = vec![3, 1, 2, 1];
        should_panic.set(true);
        assert!(catch_unwind(AssertUnwindSafe(|| composition.compose())).is_err());
        RUNTIME.with(|runtime| assert!(runtime.borrow().is_none()));
        assert!(cleanups.borrow().is_empty());
        should_panic.set(false);
        *keys.borrow_mut() = vec![1, 2, 1, 3];
        composition.compose();
        assert_eq!(composition.output(), Some(&vec![0, 1, 2, 3]));
        let mut another = Composition::new(|| LOCAL_VALUE.current());
        another.compose();
        assert_eq!(another.output(), Some(&0));
        keys.borrow_mut().clear();
        composition.compose();
        assert_eq!(*cleanups.borrow(), vec![0, 1, 2, 3]);
    }

    /// Checks that a change to state read inside a scope runs only that scope.
    #[test]
    fn scope_state_changes_recompose_only_the_scope() {
        let root_runs = Rc::new(Cell::new(0));
        let inner_runs = Rc::new(Cell::new(0));
        let sibling_runs = Rc::new(Cell::new(0));
        let shared = Rc::new(RefCell::new(None::<State<i32>>));
        let (roots, inners, siblings, slot) = (
            Rc::clone(&root_runs),
            Rc::clone(&inner_runs),
            Rc::clone(&sibling_runs),
            Rc::clone(&shared),
        );
        let mut composition = Composition::new(move || {
            roots.set(roots.get() + 1);
            let value = remember(|| 0);
            *slot.borrow_mut() = Some(value.clone());
            let (inners, siblings) = (Rc::clone(&inners), Rc::clone(&siblings));
            let shown = restartable((), move || {
                inners.set(inners.get() + 1);
                value.get()
            });
            let other = restartable((), move || {
                siblings.set(siblings.get() + 1);
                7
            });
            (shown, other)
        });
        composition.compose();
        let value = shared.borrow().clone().unwrap();
        value.set(5);
        assert!(composition.recompose());
        assert_eq!(
            (root_runs.get(), inner_runs.get(), sibling_runs.get()),
            (1, 2, 1)
        );
        let (shown, other) = composition.output().unwrap();
        assert_eq!(shown.output::<i32>(), Some(5));
        assert_eq!(other.output::<i32>(), Some(7));
        assert!(!composition.recompose());
    }

    /// Checks that state the root reads recomposes the root, and that equal deps skip a scope.
    #[test]
    fn root_reads_recompose_the_root_and_scopes_skip_on_equal_deps() {
        let root_runs = Rc::new(Cell::new(0));
        let scope_runs = Rc::new(Cell::new(0));
        let shared = Rc::new(RefCell::new(None::<State<i32>>));
        let (roots, scopes, slot) = (
            Rc::clone(&root_runs),
            Rc::clone(&scope_runs),
            Rc::clone(&shared),
        );
        let mut composition = Composition::new(move || {
            roots.set(roots.get() + 1);
            let value = remember(|| 0);
            *slot.borrow_mut() = Some(value.clone());
            let seen = value.get();
            let scopes = Rc::clone(&scopes);
            restartable(seen % 2, move || {
                scopes.set(scopes.get() + 1);
                seen
            })
        });
        composition.compose();
        let value = shared.borrow().clone().unwrap();
        value.set(2);
        composition.recompose();
        assert_eq!((root_runs.get(), scope_runs.get()), (2, 1));
        value.set(3);
        composition.recompose();
        assert_eq!((root_runs.get(), scope_runs.get()), (3, 2));
        assert_eq!(composition.output().unwrap().output::<i32>(), Some(3));
    }

    /// Checks that a nested scope reruns alone and that removed scopes are ignored.
    #[test]
    fn nested_scopes_rerun_alone_and_cleanup_survives() {
        let outer_runs = Rc::new(Cell::new(0));
        let inner_runs = Rc::new(Cell::new(0));
        let cleaned = Rc::new(Cell::new(0));
        let shared = Rc::new(RefCell::new(None::<State<i32>>));
        let (outers, inners, cleans, slot) = (
            Rc::clone(&outer_runs),
            Rc::clone(&inner_runs),
            Rc::clone(&cleaned),
            Rc::clone(&shared),
        );
        let mut composition = Composition::new(move || {
            let value = remember(|| 0);
            *slot.borrow_mut() = Some(value.clone());
            let (outers, inners, cleans) =
                (Rc::clone(&outers), Rc::clone(&inners), Rc::clone(&cleans));
            restartable((), move || {
                outers.set(outers.get() + 1);
                let (inners, cleans, value) =
                    (Rc::clone(&inners), Rc::clone(&cleans), value.clone());
                restartable((), move || {
                    inners.set(inners.get() + 1);
                    let cleans = Rc::clone(&cleans);
                    disposable_effect((), move || move || cleans.set(cleans.get() + 1));
                    value.get()
                })
            })
        });
        composition.compose();
        shared.borrow().clone().unwrap().set(1);
        composition.recompose();
        assert_eq!(
            (outer_runs.get(), inner_runs.get(), cleaned.get()),
            (1, 2, 0)
        );
    }
}

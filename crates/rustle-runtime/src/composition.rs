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
use std::hash::{DefaultHasher, Hash, Hasher};
use std::mem;
use std::panic::Location;
use std::rc::Rc;

use crate::frame::{FrameClock, FrameQueue};
use crate::state::{
    Invalidation, Read, RecordingGuard, State, begin_recording, end_recording, merge_recording,
    recording_depth,
};
use crate::task::Executor;

/// What a composable call site left behind.
#[derive(Default)]
struct Group {
    key: u64,
    /// The value remembered directly by this group, if any.
    slot: Option<Box<dyn Any>>,
    children: Vec<Group>,
}

/// A group being composed. Its children from the previous pass wait in
/// `old` until a call claims them; claimed and new ones collect in `next`.
struct Frame {
    key: u64,
    slot: Option<Box<dyn Any>>,
    old: Vec<Group>,
    next: Vec<Group>,
}

impl Frame {
    /// Closes the frame. Children not claimed this pass are handed to
    /// `removed`, unless `keep_unclaimed` says the pass was cut short.
    fn finish(mut self, keep_unclaimed: bool, removed: &mut Vec<Group>) -> Group {
        if keep_unclaimed {
            self.next.append(&mut self.old);
        } else {
            removed.append(&mut self.old);
        }
        Group {
            key: self.key,
            slot: self.slot,
            children: self.next,
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
            frames: vec![Frame {
                key: tree.key,
                slot: tree.slot,
                old: tree.children,
                next: Vec::new(),
            }],
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

        let output = (self.root)();

        drop(guard);
        self.output = Some(output);
        drop(removed);
        for effect in effects {
            effect();
        }
        self.executor.poll_woken();
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

fn site_key(site: &Location<'_>) -> u64 {
    let mut hasher = DefaultHasher::new();
    (site.file(), site.line(), site.column()).hash(&mut hasher);
    hasher.finish()
}

/// Closes the group opened by [`run_group`], also when `content` panics.
struct LeaveGuard;

impl Drop for LeaveGuard {
    fn drop(&mut self) {
        let keep = std::thread::panicking();
        with_runtime(|runtime| {
            let frame = runtime.frames.pop().expect("no open group");
            let group = frame.finish(keep, &mut runtime.removed);
            runtime
                .frames
                .last_mut()
                .expect("root group missing")
                .next
                .push(group);
        });
    }
}

fn run_group<R>(key: u64, content: impl FnOnce() -> R) -> R {
    with_runtime(|runtime| {
        let parent = runtime.frames.last_mut().expect("no open group");
        let group = match parent.old.iter().position(|group| group.key == key) {
            Some(index) => parent.old.remove(index),
            None => Group {
                key,
                ..Group::default()
            },
        };
        runtime.frames.push(Frame {
            key,
            slot: group.slot,
            old: group.children,
            next: Vec::new(),
        });
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
/// Blocks that read a composition local cannot be skipped, because changes to
/// locals are not tracked. Effects inside a skipped block do not run again,
/// and what it remembered is kept.
#[track_caller]
pub fn memo<D: PartialEq + 'static, T: Clone + 'static>(deps: D, content: impl FnOnce() -> T) -> T {
    run_group(site_key(Location::caller()), || {
        // The reads are checked outside the borrow, since checking one may read a local.
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
            // Keep everything the block composed last time, as it was.
            with_runtime(|runtime| {
                let frame = runtime.frames.last_mut().expect("no open group");
                let mut kept = std::mem::take(&mut frame.old);
                frame.next.append(&mut kept);
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

/// Returns the state remembered at this call site, creating it with `init` on
/// the first composition.
///
/// The value lives as long as the call site stays in the composition. If the
/// call is skipped in a pass, for example in an untaken `if`, its state is
/// dropped and `init` runs again when the call returns.
///
/// # Panics
///
/// Panics when called outside a composition pass.
#[track_caller]
pub fn remember<T: 'static>(init: impl FnOnce() -> T) -> State<T> {
    run_group(site_key(Location::caller()), || {
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
        // `init` runs outside the runtime borrow, so it may call composables.
        let invalidation = with_runtime(|runtime| Rc::clone(&runtime.invalidation));
        let state = State::new(init(), invalidation);
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

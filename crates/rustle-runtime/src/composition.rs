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

#[cfg(test)]
mod tests {
    use std::cell::Cell;
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
}

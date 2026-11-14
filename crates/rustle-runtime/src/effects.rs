//! Effects: work that runs after a composition pass, not during it.

use std::cell::RefCell;
use std::future::Future;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::panic::Location;
use std::rc::Rc;
use std::str::FromStr;

use crate::composition::{clock_and_executor, group_at, remember, with_runtime};
use crate::state::{
    Read, RecordingGuard, State, begin_recording, end_recording, merge_recording, recording_depth,
};
use crate::task::{TaskHandle, TaskScope};

type Cleanup = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

/// Runs `effect` after every pass that reaches this call, once the pass has
/// finished. Use it to publish state to things outside the composition.
pub fn side_effect(effect: impl FnOnce() + 'static) {
    with_runtime(|runtime| runtime.defer(Box::new(effect)));
}

/// What a `disposable_effect` call site remembers.
struct Disposable {
    key: u64,
    cleanup: Cleanup,
}

impl Drop for Disposable {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            cleanup();
        }
    }
}

/// Runs `effect` after the pass in which this call first appears, and again
/// after any pass in which `key` changed.
///
/// `effect` returns a cleanup. It runs before the next run of the effect and
/// when the call leaves the composition.
#[track_caller]
pub fn disposable_effect<C: FnOnce() + 'static>(
    key: impl Hash,
    effect: impl FnOnce() -> C + 'static,
) {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    let key = hasher.finish();

    group_at(Location::caller(), || {
        let cleanup = with_runtime(|runtime| {
            let slot = runtime.slot();
            match slot
                .as_mut()
                .and_then(|slot| slot.downcast_mut::<Disposable>())
            {
                Some(current) if current.key == key => None,
                Some(current) => {
                    current.key = key;
                    Some(Rc::clone(&current.cleanup))
                }
                None => {
                    let cleanup = Cleanup::default();
                    *slot = Some(Box::new(Disposable {
                        key,
                        cleanup: Rc::clone(&cleanup),
                    }));
                    Some(cleanup)
                }
            }
        });
        if let Some(cleanup) = cleanup {
            side_effect(move || {
                if let Some(previous) = cleanup.take() {
                    previous();
                }
                *cleanup.borrow_mut() = Some(Box::new(effect()));
            });
        }
    });
}

/// What a `launched_effect` call site remembers. Dropping it, when the call
/// leaves the composition or its key changes, cancels the task.
struct Launched {
    key: u64,
    task: Rc<RefCell<Option<TaskHandle>>>,
}

/// Starts `block` as a task after the pass in which this call first appears,
/// and restarts it after any pass in which `key` changed.
///
/// The task is cancelled when the call leaves the composition or `key`
/// changes. It can wait on frames and time through its [`TaskScope`], which is
/// how it animates or delays; it cannot wait on other threads.
#[track_caller]
pub fn launched_effect<F: Future<Output = ()> + 'static>(
    key: impl Hash,
    block: impl FnOnce(TaskScope) -> F + 'static,
) {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    let key = hasher.finish();

    group_at(Location::caller(), || {
        let task = with_runtime(|runtime| {
            let slot = runtime.slot();
            match slot
                .as_mut()
                .and_then(|slot| slot.downcast_mut::<Launched>())
            {
                Some(current) if current.key == key => None,
                Some(current) => {
                    current.key = key;
                    Some(Rc::clone(&current.task))
                }
                None => {
                    let task = Rc::new(RefCell::new(None));
                    *slot = Some(Box::new(Launched {
                        key,
                        task: Rc::clone(&task),
                    }));
                    Some(task)
                }
            }
        });
        if let Some(task) = task {
            let (clock, executor) = clock_and_executor();
            side_effect(move || {
                // Cancels the previous run, if any, before the new one starts.
                drop(task.borrow_mut().take());
                let handle = executor.spawn(block(TaskScope::new(clock)));
                *task.borrow_mut() = Some(handle);
            });
        }
    });
}

/// Returns state that always holds the latest `value`, without recomposing
/// when it changes. Read it from an effect that should see new values without
/// restarting, for example a callback passed to a long-lived `launched_effect`.
#[track_caller]
pub fn remember_updated_state<T: Clone + 'static>(value: T) -> State<T> {
    let state = remember(|| value.clone());
    state.replace_quietly(value);
    state
}

/// What a derived state keeps between passes: its state and what the calculation read.
struct Derived<T> {
    state: State<T>,
    reads: Vec<Read>,
    unskippable: bool,
}

/// Returns state holding what `calculation` computes from other state.
///
/// The calculation runs again only when state it read has changed (or a
/// composition local it read has); otherwise the previous result is returned
/// without running it. The state it read also counts as read by the caller, so
/// a skippable block using a derived state is recomposed exactly when the
/// inputs of the derivation change. The result's version moves only when it
/// is recalculated.
#[track_caller]
pub fn derived_state_of<T: Clone + 'static>(calculation: impl FnOnce() -> T) -> State<T> {
    group_at(Location::caller(), || {
        let cached = with_runtime(|runtime| {
            runtime
                .slot()
                .as_ref()
                .and_then(|slot| slot.downcast_ref::<Derived<T>>())
                .filter(|derived| !derived.unskippable)
                .filter(|derived| derived.reads.iter().all(Read::is_current))
                .map(|derived| (derived.state.clone(), derived.reads.clone()))
        });
        if let Some((state, reads)) = cached {
            merge_recording(reads, false);
            return state;
        }
        begin_recording();
        let _guard = RecordingGuard {
            depth: recording_depth() - 1,
        };
        let value = calculation();
        let recording = end_recording();
        let previous = with_runtime(|runtime| {
            runtime
                .slot()
                .as_ref()
                .and_then(|slot| slot.downcast_ref::<Derived<T>>())
                .map(|derived| derived.state.clone())
        });
        let state = match previous {
            Some(state) => {
                state.replace_quietly(value);
                state
            }
            None => {
                let invalidation = with_runtime(|runtime| Rc::clone(&runtime.invalidation));
                State::new(value, invalidation)
            }
        };
        let derived = Derived {
            state: state.clone(),
            reads: recording.reads.clone(),
            unskippable: recording.unskippable,
        };
        with_runtime(|runtime| *runtime.slot() = Some(Box::new(derived)));
        merge_recording(recording.reads, recording.unskippable);
        state
    })
}

/// Returns state that starts as `initial` and is set by a task `producer`
/// runs, such as one that waits and then publishes a result. The task restarts
/// when `key` changes and is cancelled when the call leaves the composition.
#[track_caller]
pub fn produce_state<T: Clone + 'static, F: Future<Output = ()> + 'static>(
    initial: T,
    key: impl Hash,
    producer: impl FnOnce(TaskScope, State<T>) -> F + 'static,
) -> State<T> {
    let state = remember(|| initial);
    let target = state.clone();
    launched_effect(key, move |scope| producer(scope, target));
    state
}

/// Stores values as text across runs of the application. The host provides one.
pub trait Saver {
    fn load(&self, key: &str) -> Option<String>;
    fn store(&self, key: &str, value: &str);
}

thread_local! {
    static SAVER: RefCell<Option<Rc<dyn Saver>>> = const { RefCell::new(None) };
}

/// Installs where [`remember_saveable`] keeps its values. The host does this at startup.
pub fn set_saver(saver: Rc<dyn Saver>) {
    SAVER.with(|current| *current.borrow_mut() = Some(saver));
}

/// Like [`remember`], but the value also survives restarts of the application.
///
/// It starts as the value stored under `key` if there is one that parses, and
/// as what `init` returns otherwise, and is stored again after every pass in
/// which it changed. Without a [`Saver`] it behaves like `remember`.
#[track_caller]
pub fn remember_saveable<T: ToString + FromStr + Clone + 'static>(
    key: &str,
    init: impl FnOnce() -> T,
) -> State<T> {
    let saver = SAVER.with(|current| current.borrow().clone());
    let state = remember(|| {
        saver
            .as_ref()
            .and_then(|saver| saver.load(key))
            .and_then(|stored| stored.parse().ok())
            .unwrap_or_else(init)
    });
    if let Some(saver) = saver {
        let (key, state) = (key.to_string(), state.clone());
        side_effect(move || saver.store(&key, &state.get().to_string()));
    }
    state
}

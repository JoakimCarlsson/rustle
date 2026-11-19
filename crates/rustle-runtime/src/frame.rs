//! The frame clock: callbacks that run once at the start of the next frame.

use std::cell::{Cell, RefCell};
use std::mem;
use std::rc::Rc;
use std::time::Duration;

type Callback = Box<dyn FnOnce(Duration)>;

/// What runs when a timer is due.
type Alarm = Box<dyn FnOnce()>;

#[derive(Default)]
pub(crate) struct FrameQueue {
    callbacks: RefCell<Vec<Callback>>,
    waker: RefCell<Option<Box<dyn Fn()>>>,
    /// The time of the latest frame that ran.
    latest: Cell<Duration>,
    /// What runs once the host's clock reaches a time, with that time.
    timers: RefCell<Vec<(Duration, Alarm)>>,
    /// The host's clock, when it gave one, which timers are set against.
    time_source: RefCell<Option<Box<dyn Fn() -> Duration>>>,
}

impl FrameQueue {
    pub(crate) fn set_waker(&self, waker: impl Fn() + 'static) {
        *self.waker.borrow_mut() = Some(Box::new(waker));
    }

    pub(crate) fn latest(&self) -> Duration {
        self.latest.get()
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.callbacks.borrow().is_empty()
    }

    /// Gives the host's clock, which [`now`](Self::now) reads.
    pub(crate) fn set_time_source(&self, source: impl Fn() -> Duration + 'static) {
        *self.time_source.borrow_mut() = Some(Box::new(source));
    }

    /// The time on the host's clock, if it gave one.
    pub(crate) fn now(&self) -> Option<Duration> {
        self.time_source.borrow().as_ref().map(|source| source())
    }

    /// Runs `alarm` once the host's clock reaches `at`.
    pub(crate) fn add_timer(&self, at: Duration, alarm: Alarm) {
        self.timers.borrow_mut().push((at, alarm));
    }

    /// The earliest time a timer waits for.
    pub(crate) fn next_timer(&self) -> Option<Duration> {
        self.timers.borrow().iter().map(|(at, _)| *at).min()
    }

    /// Runs the timers due at `now`, and says whether any was.
    pub(crate) fn run_timers(&self, now: Duration) -> bool {
        let due: Vec<Alarm> = {
            let mut timers = self.timers.borrow_mut();
            let (due, waiting) = mem::take(&mut *timers)
                .into_iter()
                .partition::<Vec<_>, _>(|(at, _)| *at <= now);
            *timers = waiting;
            due.into_iter().map(|(_, alarm)| alarm).collect()
        };
        let fired = !due.is_empty();
        for alarm in due {
            alarm();
        }
        fired
    }

    pub(crate) fn request(&self, callback: Callback) {
        let first = {
            let mut callbacks = self.callbacks.borrow_mut();
            callbacks.push(callback);
            callbacks.len() == 1
        };
        if first && let Some(waker) = &*self.waker.borrow() {
            waker();
        }
    }

    /// Runs the callbacks requested so far. Callbacks that request again are
    /// kept for the following frame.
    pub(crate) fn run(&self, now: Duration) {
        self.latest.set(now);
        let callbacks = mem::take(&mut *self.callbacks.borrow_mut());
        for callback in callbacks {
            callback(now);
        }
        self.run_timers(now);
    }
}

/// A handle to the frame clock of the composition it was created in.
///
/// Cheap to clone and usable after the composition pass, for example from an
/// animation that keeps asking for the next frame.
#[derive(Clone)]
pub struct FrameClock {
    queue: Rc<FrameQueue>,
}

impl FrameClock {
    pub(crate) fn new(queue: Rc<FrameQueue>) -> Self {
        Self { queue }
    }

    /// The time of the latest frame that ran, since the host started its clock:
    /// zero before the first. It only moves when frames run, so while nothing
    /// asks for one it is how long ago the last was.
    pub fn now(&self) -> Duration {
        self.queue.latest()
    }

    /// Runs `callback` at the start of the next frame, with the time since the
    /// host started its clock. It runs once; request again for the frame after.
    pub fn request_frame(&self, callback: impl FnOnce(Duration) + 'static) {
        self.queue.request(Box::new(callback));
    }

    /// The time on the host's clock now, when the host gave its clock to the composition
    /// ([`Composition::set_time_source`](crate::Composition::set_time_source)).
    pub(crate) fn host_now(&self) -> Option<Duration> {
        self.queue.now()
    }

    /// Runs `alarm` once the host's clock reaches `at`, without asking for a frame.
    pub(crate) fn at(&self, at: Duration, alarm: impl FnOnce() + 'static) {
        self.queue.add_timer(at, Box::new(alarm));
    }
}

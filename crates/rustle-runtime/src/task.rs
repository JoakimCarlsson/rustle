//! A single-threaded executor for effects that suspend, driven by the frame clock.
//!
//! Tasks are polled when they start and whenever a frame has woken them, so
//! they can wait for frames and time, but not for other threads.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Wake, Waker};
use std::time::Duration;

use crate::frame::FrameClock;

/// Marks a task as ready to be polled when its waker is woken.
struct Flag(AtomicBool);

impl Wake for Flag {
    fn wake(self: Arc<Self>) {
        self.0.store(true, Ordering::SeqCst);
    }
}

struct Task {
    future: RefCell<Option<Pin<Box<dyn Future<Output = ()>>>>>,
    woken: Arc<Flag>,
    /// Set once the task finished or was cancelled.
    done: Cell<bool>,
}

#[derive(Default)]
pub(crate) struct Executor {
    tasks: RefCell<Vec<Rc<Task>>>,
}

impl Executor {
    /// Starts `future`. It first runs at the next [`Executor::poll_woken`].
    pub(crate) fn spawn(&self, future: impl Future<Output = ()> + 'static) -> TaskHandle {
        let task = Rc::new(Task {
            future: RefCell::new(Some(Box::pin(future))),
            woken: Arc::new(Flag(AtomicBool::new(true))),
            done: Cell::new(false),
        });
        self.tasks.borrow_mut().push(Rc::clone(&task));
        TaskHandle { task }
    }

    /// Polls the tasks that are ready until none is.
    pub(crate) fn poll_woken(&self) {
        loop {
            let ready: Vec<Rc<Task>> = self
                .tasks
                .borrow()
                .iter()
                .filter(|task| task.woken.0.swap(false, Ordering::SeqCst))
                .cloned()
                .collect();
            if ready.is_empty() {
                break;
            }
            for task in ready {
                // Taken out while it runs, so it can cancel itself.
                let Some(mut future) = task.future.borrow_mut().take() else {
                    continue;
                };
                let waker = Waker::from(Arc::clone(&task.woken));
                match future.as_mut().poll(&mut Context::from_waker(&waker)) {
                    Poll::Pending if !task.done.get() => *task.future.borrow_mut() = Some(future),
                    _ => task.done.set(true),
                }
            }
            self.tasks.borrow_mut().retain(|task| !task.done.get());
        }
    }
}

/// A running task. Dropping it cancels the task.
pub(crate) struct TaskHandle {
    task: Rc<Task>,
}

impl Drop for TaskHandle {
    fn drop(&mut self) {
        self.task.done.set(true);
        // Dropped outside the borrow, so the future's own drops may do anything.
        let future = self.task.future.borrow_mut().take();
        drop(future);
    }
}

/// What a suspending effect can wait on.
#[derive(Clone)]
pub struct TaskScope {
    clock: FrameClock,
}

impl TaskScope {
    pub(crate) fn new(clock: FrameClock) -> Self {
        Self { clock }
    }

    /// Waits for the next frame and returns its time.
    pub fn next_frame(&self) -> NextFrame {
        NextFrame {
            clock: self.clock.clone(),
            frame: Rc::new(Cell::new(None)),
            requested: false,
        }
    }

    /// Waits for at least `duration`, as Compose's `delay` suspends a coroutine: on a timer
    /// set against the host's clock, without asking for frames while it waits. When the host
    /// gave the composition no clock, the wait starts at the next frame.
    pub fn delay(&self, duration: Duration) -> Delay {
        Delay {
            clock: self.clock.clone(),
            duration,
            done: Rc::new(Cell::new(false)),
            requested: false,
        }
    }
}

/// Resolves with the time of the next frame.
pub struct NextFrame {
    clock: FrameClock,
    frame: Rc<Cell<Option<Duration>>>,
    requested: bool,
}

impl Future for NextFrame {
    type Output = Duration;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Duration> {
        if let Some(time) = self.frame.get() {
            return Poll::Ready(time);
        }
        if !self.requested {
            self.requested = true;
            let (frame, waker) = (Rc::clone(&self.frame), cx.waker().clone());
            self.clock.request_frame(move |now| {
                frame.set(Some(now));
                waker.wake();
            });
        }
        Poll::Pending
    }
}

/// Resolves once a duration has passed.
pub struct Delay {
    clock: FrameClock,
    duration: Duration,
    done: Rc<Cell<bool>>,
    requested: bool,
}

impl Future for Delay {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.done.get() {
            return Poll::Ready(());
        }
        if !self.requested {
            self.requested = true;
            let (done, waker, duration) =
                (Rc::clone(&self.done), cx.waker().clone(), self.duration);
            let alarm = move || {
                done.set(true);
                waker.wake();
            };
            match self.clock.host_now() {
                Some(now) => self.clock.at(now + duration, alarm),
                None => {
                    let clock = self.clock.clone();
                    self.clock
                        .request_frame(move |now| clock.at(now + duration, alarm));
                }
            }
        }
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    use crate::{Composition, launched_effect};

    /// A composition whose task waits `delay` and then marks `done`, timed by `clock`.
    fn waiting(
        done: &Rc<Cell<bool>>,
        delay: Duration,
        clock: &Rc<Cell<Duration>>,
    ) -> Composition<()> {
        let done = Rc::clone(done);
        let mut composition = Composition::new(move || {
            let done = Rc::clone(&done);
            launched_effect((), move |scope| async move {
                scope.delay(delay).await;
                done.set(true);
            });
        });
        let time = Rc::clone(clock);
        composition.set_time_source(move || time.get());
        composition
    }

    /// With the host's clock, a delay waits on a timer from when the task waits, asking for
    /// no frame, and ends when the host runs the timers at its end.
    #[test]
    fn a_delay_waits_on_a_timer_without_frames() {
        let done = Rc::new(Cell::new(false));
        let clock = Rc::new(Cell::new(Duration::from_secs(4)));
        let mut composition = waiting(&done, Duration::from_millis(500), &clock);
        composition.compose();
        assert!(!composition.has_pending_frame());
        assert_eq!(composition.next_timer(), Some(Duration::from_millis(4_500)));
        assert!(!composition.run_timers(Duration::from_millis(4_499)));
        assert!(!done.get());
        assert!(composition.run_timers(Duration::from_millis(4_500)));
        assert!(done.get());
        assert_eq!(composition.next_timer(), None);
    }

    /// Without the host's clock, a delay starts at the next frame and ends at the frame
    /// that reaches its end.
    #[test]
    fn without_a_clock_a_delay_starts_at_the_next_frame() {
        let done = Rc::new(Cell::new(false));
        let done_in = Rc::clone(&done);
        let mut composition = Composition::new(move || {
            let done = Rc::clone(&done_in);
            launched_effect((), move |scope| async move {
                scope.delay(Duration::from_millis(100)).await;
                done.set(true);
            });
        });
        composition.compose();
        assert!(composition.has_pending_frame());
        composition.run_frame(Duration::from_secs(1));
        assert!(!composition.has_pending_frame());
        composition.run_frame(Duration::from_millis(1_099));
        assert!(!done.get());
        composition.run_frame(Duration::from_millis(1_100));
        assert!(done.get());
    }
}

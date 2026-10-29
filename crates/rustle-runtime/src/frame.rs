//! The frame clock: callbacks that run once at the start of the next frame.

use std::cell::RefCell;
use std::mem;
use std::rc::Rc;
use std::time::Duration;

type Callback = Box<dyn FnOnce(Duration)>;

#[derive(Default)]
pub(crate) struct FrameQueue {
    callbacks: RefCell<Vec<Callback>>,
    waker: RefCell<Option<Box<dyn Fn()>>>,
}

impl FrameQueue {
    pub(crate) fn set_waker(&self, waker: impl Fn() + 'static) {
        *self.waker.borrow_mut() = Some(Box::new(waker));
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.callbacks.borrow().is_empty()
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
        let callbacks = mem::take(&mut *self.callbacks.borrow_mut());
        for callback in callbacks {
            callback(now);
        }
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

    /// Runs `callback` at the start of the next frame, with the time since the
    /// host started its clock. It runs once; request again for the frame after.
    pub fn request_frame(&self, callback: impl FnOnce(Duration) + 'static) {
        self.queue.request(Box::new(callback));
    }
}

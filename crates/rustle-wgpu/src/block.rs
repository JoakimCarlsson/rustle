//! A minimal executor that waits for one future on the current thread.

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

/// Wakes the thread that is blocked on a future.
struct ThreadWaker(Thread);

impl Wake for ThreadWaker {
    /// Unparks the waiting thread.
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    /// Unparks the waiting thread without consuming the waker.
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

/// Runs `future` to completion on the current thread, parking it while the
/// future is pending, and returns its output.
pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut context = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => thread::park(),
        }
    }
}

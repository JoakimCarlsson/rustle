//! How long each frame of the main window took to build and to draw, told to an observer the app installs.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// How long one frame of the main window took, measured on the CPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameTiming {
    /// Running the frame clock, composing, laying out and turning the tree into draw commands.
    pub scene: Duration,
    /// Acquiring the surface, encoding the commands and submitting them.
    pub render: Duration,
}

/// How long the latest [`Embedded::update`](crate::Embedded::update) took, measured on the CPU:
/// what a host reads to see where a frame of an embedded interface went.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UpdateTiming {
    /// Running the frame clock and composing what state changes dirtied, including the passes
    /// that state changed during layout asked for.
    pub compose: Duration,
    /// Measuring the composed tree and turning it into draw commands and regions.
    pub layout: Duration,
}

/// A callback told of each frame's timing.
type FrameObserver = Rc<dyn Fn(FrameTiming)>;

thread_local! {
    /// The observer told of each frame drawn, when the app installed one.
    static OBSERVER: RefCell<Option<FrameObserver>> = const { RefCell::new(None) };
}

/// Calls `observer` with the timing of every frame the main window submits from now on, replacing any observer installed before.
pub fn set_frame_observer(observer: impl Fn(FrameTiming) + 'static) {
    OBSERVER.with(|current| *current.borrow_mut() = Some(Rc::new(observer)));
}

/// Tells the installed observer, if any, that a frame took `timing`.
pub(crate) fn observe(timing: FrameTiming) {
    let observer = OBSERVER.with(|current| current.borrow().clone());
    if let Some(observer) = observer {
        observer(timing);
    }
}

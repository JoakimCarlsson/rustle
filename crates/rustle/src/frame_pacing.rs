//! Paces a native window's frames to the refresh of the display it is on.
//!
//! On macOS every window gets a `CADisplayLink` from its content view: the link follows the
//! window from display to display, ticks at the display's own rate (up to 120 Hz on ProMotion
//! displays) and asks winit for a redraw at most once per tick, when something asked for a
//! frame since the last one. Ticks with nothing to draw for [`IDLE_GRACE`] seconds after the
//! last frame pause the link, so an idle window costs no wake-ups, while input arriving at a
//! rate other than the display's, such as a trackpad's scrolls, keeps the link running and
//! is drawn at the next refresh instead of waiting for the link to resume. Elsewhere a frame request is winit's `request_redraw`.
//!
//! Input arriving at a high rate, as a trackpad's scrolls do, keeps a frame drawn at every
//! refresh for [`SUSTAIN`] seconds after it, whether or not anything changed, as Zed's
//! windows do: a display that varies its rate slows down after a refresh with no frame, and
//! the frames after it then come late, so a scroll that pauses for a refresh between its
//! events would seem to stop and then catch up.
//!
//! `RUSTLE_FRAME_PACING_LOG=1` prints when a link resumes, requests a frame and pauses.

use std::sync::Arc;

use winit::window::Window;

/// Whether `RUSTLE_FRAME_PACING_LOG=1` asked for frame pacing to be logged.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn logging() -> bool {
    static LOGGING: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *LOGGING
        .get_or_init(|| std::env::var("RUSTLE_FRAME_PACING_LOG").is_ok_and(|value| value == "1"))
}

/// How long, in seconds, a display link keeps ticking after its last frame before a tick
/// with nothing to draw pauses it.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const IDLE_GRACE: f64 = 0.1;

/// How many inputs in [`RATE_WINDOW`] seconds count as input at a high rate.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const HIGH_RATE_INPUTS: usize = 6;

/// How far back, in seconds, inputs are counted to tell whether they come at a high rate.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const RATE_WINDOW: f64 = 0.1;

/// How long, in seconds, a frame is drawn at every refresh after input at a high rate.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const SUSTAIN: f64 = 1.0;

/// Paces one window's frames: through a display link on macOS, through winit elsewhere.
pub(crate) struct FramePacer {
    /// The window's display link, when one could be made for it.
    #[cfg(target_os = "macos")]
    link: Option<macos::Link>,
}

impl FramePacer {
    /// A pacer for `window`. On macOS it makes the window's display link, paused until the
    /// first frame request; if the window has no AppKit view, or this is not the main thread,
    /// frames are requested from winit directly.
    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
    pub(crate) fn new(window: &Arc<dyn Window>) -> Self {
        Self {
            #[cfg(target_os = "macos")]
            link: macos::Link::new(window),
        }
    }

    /// Asks for `window`, the window this pacer was made for, to be drawn at the next refresh
    /// of its display, resuming the display link if it was paused.
    pub(crate) fn request(&self, window: &dyn Window) {
        #[cfg(target_os = "macos")]
        if let Some(link) = &self.link {
            link.request();
            return;
        }
        window.request_redraw();
    }

    /// Notes that input arrived for the window, so that input at a high rate keeps a frame
    /// drawn at every refresh for a while.
    pub(crate) fn note_input(&self) {
        #[cfg(target_os = "macos")]
        if let Some(link) = &self.link {
            link.note_input();
        }
    }

    /// The moment of the refresh the frame about to be drawn was asked for at, when the
    /// display link asked for it; `None` when frames are not paced by a display link.
    pub(crate) fn frame_time(&self) -> Option<std::time::Instant> {
        #[cfg(target_os = "macos")]
        if let Some(link) = &self.link {
            return link.frame_time();
        }
        None
    }
}

/// The macOS display link behind [`FramePacer`].
#[cfg(target_os = "macos")]
mod macos {
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::sync::{Arc, Weak};
    use std::time::{Duration, Instant};

    use objc2::rc::Retained;
    use objc2::runtime::{NSObject, NSObjectProtocol};
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
    use objc2_app_kit::NSView;
    use objc2_foundation::{NSRunLoop, NSRunLoopCommonModes};
    use objc2_quartz_core::{CACurrentMediaTime, CADisplayLink, CAFrameRateRange};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    /// What a [`Target`] keeps: the window it asks frames of and whether one is wanted.
    struct Ivars {
        /// The window to redraw; weak, so the link does not keep a closed window alive.
        window: Weak<dyn Window>,
        /// Whether a frame was requested since the link last asked for one.
        wanted: Cell<bool>,
        /// When the link last asked for a frame, in host seconds; zero after a pause, so the
        /// first frame after idle is not counted as late.
        last: Cell<f64>,
        /// When inputs arrived lately, in host seconds, the oldest first.
        inputs: RefCell<VecDeque<f64>>,
        /// Until when, in host seconds, a frame is drawn at every refresh.
        sustain_until: Cell<f64>,
        /// The refresh the link last asked for a frame at.
        tick: Cell<Option<Instant>>,
    }

    define_class!(
        /// The object a display link calls on every refresh of the window's display.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "RustleFramePacingTarget"]
        #[ivars = Ivars]
        struct Target;

        impl Target {
            /// Called by `link` at a refresh of the display: asks winit for a frame when one is
            /// wanted, and otherwise, once no frame was asked for in [`super::IDLE_GRACE`]
            /// seconds, pauses the link until one is.
            #[unsafe(method(step:))]
            fn step(&self, link: &CADisplayLink) {
                let ivars = self.ivars();
                let sustained = link.timestamp() < ivars.sustain_until.get();
                if !ivars.wanted.replace(false) && !sustained {
                    let last = ivars.last.get();
                    if last > 0.0 && link.timestamp() - last < super::IDLE_GRACE {
                        return;
                    }
                    link.setPaused(true);
                    ivars.last.set(0.0);
                    if super::logging() {
                        eprintln!("rustle frame pacing: idle, display link paused");
                    }
                    return;
                }
                match ivars.window.upgrade() {
                    Some(window) => {
                        let now = link.timestamp();
                        let previous = ivars.last.replace(now);
                        let behind = (CACurrentMediaTime() - now).clamp(0.0, 0.1);
                        ivars.tick.set(
                            Instant::now().checked_sub(Duration::from_secs_f64(behind)),
                        );
                        if super::logging() {
                            eprintln!(
                                "rustle frame pacing: display link requested a frame (target {:.4}s, {})",
                                link.targetTimestamp(),
                                missed_refreshes(previous, now, link.duration())
                            );
                        }
                        window.request_redraw();
                    }
                    None => link.setPaused(true),
                }
            }
        }
    );

    /// Describes how many refreshes passed unused between two frames the link asked for,
    /// `previous` and `now` in host seconds (`previous` zero for the first frame after idle)
    /// and `duration` the length of one refresh.
    fn missed_refreshes(previous: f64, now: f64, duration: f64) -> String {
        if previous <= 0.0 || duration <= 0.0 {
            return "first after idle".to_string();
        }
        let missed = ((now - previous) / duration).round() as i64 - 1;
        format!(
            "missed {} refreshes at {:.0} Hz",
            missed.max(0),
            1.0 / duration
        )
    }

    impl Target {
        /// A target that asks `window` for frames, wanting none yet.
        fn new(window: Weak<dyn Window>, mtm: MainThreadMarker) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(Ivars {
                window,
                wanted: Cell::new(false),
                last: Cell::new(0.0),
                inputs: RefCell::default(),
                sustain_until: Cell::new(0.0),
                tick: Cell::new(None),
            });
            unsafe { msg_send![super(this), init] }
        }
    }

    /// A window's display link and the target it calls; invalidated when dropped.
    pub(super) struct Link {
        /// The link, scheduled on the main run loop in its common modes.
        link: Retained<CADisplayLink>,
        /// The target the link calls.
        target: Retained<Target>,
    }

    impl Link {
        /// A paused display link made from the content view of `window`, ticking at up to the
        /// display's highest rate. `None` when this is not the main thread or the window has
        /// no AppKit view, or AppKit is older than macOS 14 and views make no display links.
        pub(super) fn new(window: &Arc<dyn Window>) -> Option<Self> {
            let mtm = MainThreadMarker::new()?;
            let RawWindowHandle::AppKit(handle) = window.window_handle().ok()?.as_raw() else {
                return None;
            };
            let view: &NSView = unsafe { handle.ns_view.cast().as_ref() };
            if !view.respondsToSelector(sel!(displayLinkWithTarget:selector:)) {
                return None;
            }
            let target = Target::new(Arc::downgrade(window), mtm);
            let link = unsafe { view.displayLinkWithTarget_selector(&target, sel!(step:)) };
            link.setPaused(true);
            link.setPreferredFrameRateRange(CAFrameRateRange {
                minimum: 60.0,
                maximum: 120.0,
                preferred: 120.0,
            });
            unsafe { link.addToRunLoop_forMode(&NSRunLoop::mainRunLoop(), NSRunLoopCommonModes) };
            Some(Self { link, target })
        }

        /// Wants a frame at the next refresh, resuming the link if it was paused.
        pub(super) fn request(&self) {
            self.target.ivars().wanted.set(true);
            if self.link.isPaused() {
                if super::logging() {
                    eprintln!("rustle frame pacing: frame wanted, display link resumed");
                }
                self.link.setPaused(false);
            }
        }
    }

    impl Link {
        /// Counts an input arriving now, and once [`super::HIGH_RATE_INPUTS`] arrived within
        /// [`super::RATE_WINDOW`] seconds, keeps a frame drawn at every refresh for
        /// [`super::SUSTAIN`] seconds, resuming the link if it was paused.
        pub(super) fn note_input(&self) {
            let ivars = self.target.ivars();
            let now = CACurrentMediaTime();
            let high_rate = {
                let mut inputs = ivars.inputs.borrow_mut();
                inputs.push_back(now);
                while inputs
                    .front()
                    .is_some_and(|at| now - at > super::RATE_WINDOW)
                {
                    inputs.pop_front();
                }
                inputs.len() >= super::HIGH_RATE_INPUTS
            };
            if high_rate {
                ivars.sustain_until.set(now + super::SUSTAIN);
                if self.link.isPaused() {
                    self.link.setPaused(false);
                }
            }
        }

        /// The refresh the link last asked for a frame at.
        pub(super) fn frame_time(&self) -> Option<Instant> {
            self.target.ivars().tick.get()
        }
    }

    impl Drop for Link {
        /// Stops the link and lets it release its target.
        fn drop(&mut self) {
            self.link.invalidate();
        }
    }
}

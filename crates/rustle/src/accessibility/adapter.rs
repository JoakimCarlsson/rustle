//! The bridge between a native window and the platform's accessibility API.
//!
//! AccessKit's own winit adapter follows winit 0.30, so this drives AccessKit's platform
//! adapters directly: AT-SPI on Linux, UI Automation on Windows and `NSAccessibility` on
//! macOS. What the platform asks for arrives on other threads; it is queued as a
//! [`Request`] and the event loop is woken to answer it.

use std::sync::Arc;
use std::sync::mpsc::Sender;

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, TreeUpdate};
use winit::event::WindowEvent;
use winit::window::{Window, WindowId};

/// What the platform's accessibility API asked of a window.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Request {
    /// An assistive technology started listening and needs the whole tree.
    InitialTreeRequested,
    /// An assistive technology asked for an action, such as a click or focus.
    ActionRequested(ActionRequest),
    /// Nothing listens any more.
    AccessibilityDeactivated,
}

/// Where requests go: a queue the event loop drains, and what wakes the loop to drain it.
#[derive(Clone)]
pub(crate) struct Requests {
    /// The queue, read by the thread that runs the event loop.
    pub(crate) queue: Sender<(WindowId, Request)>,
    /// Wakes the event loop from any thread.
    pub(crate) wake: Arc<dyn Fn() + Send + Sync>,
}

impl Requests {
    /// Queues `request` for the window `window_id` and wakes the event loop.
    fn send(&self, window_id: WindowId, request: Request) {
        if self.queue.send((window_id, request)).is_ok() {
            (self.wake)();
        }
    }
}

/// Hands the platform's requests for one window to the event loop.
struct Handler {
    /// The window the requests are for.
    window_id: WindowId,
    /// Where they go.
    requests: Requests,
}

impl ActivationHandler for Handler {
    /// Asks the event loop for the tree, which it sends once it has laid the window out.
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.requests
            .send(self.window_id, Request::InitialTreeRequested);
        None
    }
}

impl ActionHandler for Handler {
    /// Passes the action on to the event loop.
    fn do_action(&mut self, request: ActionRequest) {
        self.requests
            .send(self.window_id, Request::ActionRequested(request));
    }
}

impl DeactivationHandler for Handler {
    /// Tells the event loop that nothing listens any more.
    fn deactivate_accessibility(&mut self) {
        self.requests
            .send(self.window_id, Request::AccessibilityDeactivated);
    }
}

/// The accessibility adapter of one native window.
pub(crate) struct Adapter {
    /// The platform's adapter.
    inner: platform::Adapter,
}

impl Adapter {
    /// The adapter of `window`, whose requests go to `requests`.
    ///
    /// # Panics
    ///
    /// Panics if the window has been shown already: the platform adapters have to be in place
    /// before the window first appears.
    pub(crate) fn new(window: &dyn Window, requests: &Requests) -> Self {
        assert!(
            window.is_visible() != Some(true),
            "the accessibility adapter must be created before the window is shown"
        );
        let handler = || Handler {
            window_id: window.id(),
            requests: requests.clone(),
        };
        Self {
            inner: platform::Adapter::new(window, handler(), handler(), handler()),
        }
    }

    /// Tells the adapter what happened to the window, so it knows where the window is and
    /// whether it has focus.
    pub(crate) fn process_event(&mut self, window: &dyn Window, event: &WindowEvent) {
        self.inner.process_event(window, event);
    }

    /// Sends the tree `updater` makes, if anything is listening.
    pub(crate) fn update_if_active(&mut self, updater: impl FnOnce() -> TreeUpdate) {
        self.inner.update_if_active(updater);
    }
}

/// AT-SPI, through `accesskit_unix`.
#[cfg(target_os = "linux")]
mod platform {
    use accesskit::{ActionHandler, ActivationHandler, DeactivationHandler, Rect, TreeUpdate};
    use winit::dpi::{PhysicalPosition, PhysicalSize};
    use winit::event::WindowEvent;
    use winit::window::Window;

    /// The AT-SPI adapter of one window.
    pub(super) struct Adapter {
        /// AccessKit's adapter.
        adapter: accesskit_unix::Adapter,
    }

    impl Adapter {
        /// The adapter of a window, answering through the given handlers.
        pub(super) fn new(
            _window: &dyn Window,
            activation_handler: impl 'static + ActivationHandler + Send,
            action_handler: impl 'static + ActionHandler + Send,
            deactivation_handler: impl 'static + DeactivationHandler + Send,
        ) -> Self {
            Self {
                adapter: accesskit_unix::Adapter::new(
                    activation_handler,
                    action_handler,
                    deactivation_handler,
                ),
            }
        }

        /// Sends the tree `updater` makes, if anything is listening.
        pub(super) fn update_if_active(&mut self, updater: impl FnOnce() -> TreeUpdate) {
            self.adapter.update_if_active(updater);
        }

        /// Follows the window's bounds and focus.
        pub(super) fn process_event(&mut self, window: &dyn Window, event: &WindowEvent) {
            match event {
                WindowEvent::Moved(outer) => {
                    self.set_bounds(window, *outer, window.surface_size());
                }
                WindowEvent::SurfaceResized(surface) => {
                    let outer = window.outer_position().unwrap_or_default();
                    self.set_bounds(window, outer, *surface);
                }
                WindowEvent::Focused(focused) => self.adapter.update_window_focus_state(*focused),
                _ => {}
            }
        }

        /// Tells AT-SPI where the window is, with its frame at `outer` and its content
        /// `surface` pixels large.
        fn set_bounds(
            &mut self,
            window: &dyn Window,
            outer: PhysicalPosition<i32>,
            surface: PhysicalSize<u32>,
        ) {
            let offset = window.surface_position();
            let rect = |x: i32, y: i32, size: PhysicalSize<u32>| {
                Rect::from_origin_size(
                    (f64::from(x), f64::from(y)),
                    (f64::from(size.width), f64::from(size.height)),
                )
            };
            self.adapter.set_root_window_bounds(
                rect(outer.x, outer.y, window.outer_size()),
                rect(outer.x + offset.x, outer.y + offset.y, surface),
            );
        }
    }
}

/// UI Automation, through `accesskit_windows`.
#[cfg(target_os = "windows")]
mod platform {
    use accesskit::{ActionHandler, ActivationHandler, DeactivationHandler, TreeUpdate};
    use accesskit_windows::{HWND, SubclassingAdapter};
    use winit::event::WindowEvent;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    /// The UI Automation adapter of one window.
    pub(super) struct Adapter {
        /// AccessKit's adapter, which subclasses the window.
        adapter: SubclassingAdapter,
    }

    impl Adapter {
        /// The adapter of `window`, answering through the given handlers.
        ///
        /// # Panics
        ///
        /// Panics if the window is not a Win32 window.
        pub(super) fn new(
            window: &dyn Window,
            activation_handler: impl 'static + ActivationHandler,
            action_handler: impl 'static + ActionHandler + Send,
            _deactivation_handler: impl 'static + DeactivationHandler,
        ) -> Self {
            let hwnd = match window.window_handle().map(|handle| handle.as_raw()) {
                Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get() as *mut _,
                _ => panic!("a window on Windows has a Win32 handle"),
            };
            Self {
                adapter: SubclassingAdapter::new(HWND(hwnd), activation_handler, action_handler),
            }
        }

        /// Sends the tree `updater` makes, if anything is listening.
        pub(super) fn update_if_active(&mut self, updater: impl FnOnce() -> TreeUpdate) {
            if let Some(events) = self.adapter.update_if_active(updater) {
                events.raise();
            }
        }

        /// UI Automation follows the window itself.
        pub(super) fn process_event(&mut self, _window: &dyn Window, _event: &WindowEvent) {}
    }
}

/// `NSAccessibility`, through `accesskit_macos`.
#[cfg(target_os = "macos")]
mod platform {
    use accesskit::{ActionHandler, ActivationHandler, DeactivationHandler, TreeUpdate};
    use accesskit_macos::SubclassingAdapter;
    use winit::event::WindowEvent;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    /// The `NSAccessibility` adapter of one window.
    pub(super) struct Adapter {
        /// AccessKit's adapter, which subclasses the window's view.
        adapter: SubclassingAdapter,
    }

    impl Adapter {
        /// The adapter of `window`, answering through the given handlers.
        ///
        /// # Panics
        ///
        /// Panics if the window is not an AppKit window.
        pub(super) fn new(
            window: &dyn Window,
            activation_handler: impl 'static + ActivationHandler,
            action_handler: impl 'static + ActionHandler,
            _deactivation_handler: impl 'static + DeactivationHandler,
        ) -> Self {
            let view = match window.window_handle().map(|handle| handle.as_raw()) {
                Ok(RawWindowHandle::AppKit(handle)) => handle.ns_view.as_ptr(),
                _ => panic!("a window on macOS has an AppKit handle"),
            };
            // SAFETY: the view belongs to a live window, which outlives its adapter.
            let adapter =
                unsafe { SubclassingAdapter::new(view, activation_handler, action_handler) };
            Self { adapter }
        }

        /// Sends the tree `updater` makes, if anything is listening.
        pub(super) fn update_if_active(&mut self, updater: impl FnOnce() -> TreeUpdate) {
            if let Some(events) = self.adapter.update_if_active(updater) {
                events.raise();
            }
        }

        /// Follows the window's focus.
        pub(super) fn process_event(&mut self, _window: &dyn Window, event: &WindowEvent) {
            if let WindowEvent::Focused(focused) = event
                && let Some(events) = self.adapter.update_view_focus_state(*focused)
            {
                events.raise();
            }
        }
    }
}

/// Platforms without an accessibility adapter.
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
mod platform {
    use accesskit::{ActionHandler, ActivationHandler, DeactivationHandler, TreeUpdate};
    use winit::event::WindowEvent;
    use winit::window::Window;

    /// An adapter that tells nobody anything.
    pub(super) struct Adapter;

    impl Adapter {
        /// An adapter that drops the handlers.
        pub(super) fn new(
            _window: &dyn Window,
            _activation_handler: impl 'static + ActivationHandler,
            _action_handler: impl 'static + ActionHandler,
            _deactivation_handler: impl 'static + DeactivationHandler,
        ) -> Self {
            Self
        }

        /// Nothing listens.
        pub(super) fn update_if_active(&mut self, _updater: impl FnOnce() -> TreeUpdate) {}

        /// Nothing follows the window.
        pub(super) fn process_event(&mut self, _window: &dyn Window, _event: &WindowEvent) {}
    }
}

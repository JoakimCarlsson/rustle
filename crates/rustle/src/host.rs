//! Hosting a rustle application inside an event loop the caller owns.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::app::{App, Wake, adapter_with, provided};
use crate::saved;
use crate::window::WindowOptions;

/// A rustle application driven by an event loop the caller owns.
///
/// [`run`](crate::run) owns the event loop. A program that needs its own, for its own
/// user events, extra windows or platform surfaces, makes a `Host` instead and forwards
/// the events it receives:
///
/// - [`resumed`](Host::resumed) from `ApplicationHandler::resumed`;
/// - [`window_event`](Host::window_event) from `ApplicationHandler::window_event`, for the
///   events of the host's windows;
/// - [`about_to_wait`](Host::about_to_wait) from `ApplicationHandler::about_to_wait`;
/// - [`wake`](Host::wake) when the loop receives the user event that [`Wake`] was turned into
///   and when it receives an [`AccessibilityEvent`](crate::AccessibilityEvent), after
///   [`accessibility_event`](Host::accessibility_event) for the latter.
pub struct Host {
    app: App,
}

impl Host {
    /// A host that composes `root`, with the window it opens described by `options`.
    ///
    /// `proxy` belongs to the caller's event loop. Its user event type `T` is what the loop
    /// is woken with when state changed or a frame was requested (`From<Wake>`) and what the
    /// platform's accessibility API sends (`From<AccessibilityEvent>`).
    pub fn new<T>(
        root: impl FnMut() + 'static,
        options: WindowOptions,
        proxy: EventLoopProxy<T>,
    ) -> Self
    where
        T: From<Wake> + From<accesskit_winit::Event> + Send + 'static,
    {
        rustle_runtime::set_saver(Rc::new(saved::FileSaver::open()));
        let dark = Rc::new(Cell::new(true));
        let provided = provided(root, Rc::clone(&dark));
        let wake = {
            let proxy = proxy.clone();
            move || {
                let _ = proxy.send_event(T::from(Wake));
            }
        };
        Self {
            app: App::new(provided, dark, options, Some(adapter_with(proxy)), wake),
        }
    }

    /// Opens the window and its renderer unless they are open. Call it from `resumed`.
    pub fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.app.resumed(event_loop);
    }

    /// Handles an event of one of the host's windows, the main one or one opened with
    /// [`window`](crate::window). Events of windows the host does not own are ignored, so the
    /// caller can forward every event it gets for its other windows too. Call it from
    /// `window_event`.
    pub fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        self.app.window_event_for(event_loop, window_id, event);
    }

    /// Whether `window_id` is one of the host's windows.
    pub fn owns_window(&self, window_id: WindowId) -> bool {
        self.app.slot_of(window_id).is_some()
    }

    /// Runs what is due, such as a long press. Call it from `about_to_wait`.
    pub fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.app.about_to_wait(event_loop);
    }

    /// Composes and lays out again what state changes dirtied. Call it when the loop receives
    /// the user event made from [`Wake`].
    pub fn wake(&mut self) {
        self.app.woken();
    }

    /// Answers a request of the platform's accessibility API.
    pub fn accessibility_event(&mut self, event: accesskit_winit::Event) {
        self.app
            .accessibility_event_for(event.window_id, event.window_event);
        self.app.woken();
    }

    /// The main window, once it is open. Windows opened with [`window`](crate::window) are
    /// controlled through a [`WindowHandle`](crate::WindowHandle).
    pub fn window(&self) -> Option<&Arc<Window>> {
        self.app.main_window()
    }

    /// Why the window or the renderer could not be set up or drawn, if it could not.
    pub fn error(&self) -> Option<&str> {
        self.app.error.as_deref()
    }
}

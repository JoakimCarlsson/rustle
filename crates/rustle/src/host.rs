//! Hosting a rustle application inside an event loop the caller owns.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::app::{App, provided, waker};
use crate::saved;
use crate::window::WindowOptions;

/// A rustle application driven by an event loop the caller owns.
///
/// [`run`](crate::run) owns the event loop. A program that needs its own, for extra windows
/// or platform surfaces, makes a `Host` instead and forwards what its `ApplicationHandler`
/// receives:
///
/// - [`can_create_surfaces`](Host::can_create_surfaces) from
///   `ApplicationHandler::can_create_surfaces`;
/// - [`window_event`](Host::window_event) from `ApplicationHandler::window_event`, for the
///   events of the host's windows;
/// - [`about_to_wait`](Host::about_to_wait) from `ApplicationHandler::about_to_wait`;
/// - [`wake`](Host::wake) from `ApplicationHandler::proxy_wake_up`.
pub struct Host {
    app: App,
}

impl Host {
    /// A host that composes `root`, with the window it opens described by `options`.
    ///
    /// `proxy` belongs to the caller's event loop. It is woken when state changed, a frame was
    /// requested or the platform's accessibility API asked for something; the caller then calls
    /// [`wake`](Host::wake).
    pub fn new(
        root: impl FnMut() + 'static,
        options: WindowOptions,
        proxy: EventLoopProxy,
    ) -> Self {
        rustle_runtime::set_saver(Rc::new(saved::FileSaver::open()));
        let dark = Rc::new(Cell::new(true));
        let info = crate::window_info::WindowInfo::new();
        let provided = provided(root, Rc::clone(&dark), info.clone());
        Self {
            app: App::new(provided, dark, info, options, true, waker(proxy)),
        }
    }

    /// Opens the window and its renderer unless they are open. Call it from
    /// `can_create_surfaces`.
    pub fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.app.can_create_surfaces(event_loop);
    }

    /// Handles an event of one of the host's windows, the main one or one opened with
    /// [`window`](crate::window). Events of windows the host does not own are ignored, so the
    /// caller can forward every event it gets for its other windows too. Call it from
    /// `window_event`.
    pub fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        self.app.window_event_for(event_loop, window_id, event);
    }

    /// Composes the root again, and the windows it opens, for what the root reads that is
    /// not rustle state, such as a model the caller keeps and changes outside composition.
    /// The event loop is woken through the proxy, so the next [`wake`](Host::wake) composes,
    /// lays out and redraws.
    pub fn invalidate(&mut self) {
        self.app.composition.invalidate();
    }

    /// Whether `window_id` is one of the host's windows.
    pub fn owns_window(&self, window_id: WindowId) -> bool {
        self.app.slot_of(window_id).is_some()
    }

    /// Runs what is due, such as a long press. Call it from `about_to_wait`.
    pub fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.app.about_to_wait(event_loop);
    }

    /// Answers the platform's accessibility API and composes and lays out again what state
    /// changes dirtied. Call it from `proxy_wake_up`; a wake-up meant for the caller's own
    /// work does no harm.
    pub fn wake(&mut self) {
        self.app.woken();
    }

    /// The main window, once it is open. Windows opened with [`window`](crate::window) are
    /// controlled through a [`WindowHandle`](crate::WindowHandle).
    pub fn window(&self) -> Option<&Arc<dyn Window>> {
        self.app.main_window()
    }

    /// Why the window or the renderer could not be set up or drawn, if it could not.
    pub fn error(&self) -> Option<&str> {
        self.app.error.as_deref()
    }
}

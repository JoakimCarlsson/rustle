//! The event loop: the entry point, the application state and frame running.

use std::cell::{Cell, RefCell};
use std::process;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use rustle_geometry::Size;
use rustle_ui::Composition;
use rustle_unit::Constraints;
use rustle_wgpu::Renderer;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::accessibility::{Adapter, Request, Requests};
use crate::file_drops::FileDrag;
use crate::interface::Interface;
use crate::preview::snapshot;
use crate::saved;
use crate::window::WindowOptions;

/// The proxy of the loop [`run`] and [`run_with`] own, for [`external_waker`].
static EXTERNAL_PROXY: OnceLock<EventLoopProxy> = OnceLock::new();

/// Whether [`external_waker`] was called since the loop last ran the external handler.
static EXTERNAL_PENDING: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// What the loop's thread runs when [`external_waker`] is called.
    static EXTERNAL_HANDLER: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
    /// What the loop's thread asks before the main window closes, set with [`set_close_handler`].
    static CLOSE_HANDLER: RefCell<Option<Rc<dyn Fn() -> bool>>> = const { RefCell::new(None) };
}

/// Sets what is asked when the main window is about to close, by the desktop or by
/// [`WindowHandle::close`](crate::WindowHandle::close): the window closes, ending the
/// application, only when `handler` returns `true`. Replaces the previous handler. Call it
/// from the thread that runs the loop, such as from a composable.
pub fn set_close_handler(handler: impl Fn() -> bool + 'static) {
    CLOSE_HANDLER.with(|slot| *slot.borrow_mut() = Some(Rc::new(handler)));
}

/// Whether the main window may close now: what the handler set with [`set_close_handler`]
/// says, or `true` when none is set.
pub(crate) fn main_may_close() -> bool {
    let handler = CLOSE_HANDLER.with(|slot| slot.borrow().clone());
    handler.is_none_or(|handler| handler())
}

/// A function any thread may call to make the loop that [`run`] or [`run_with`] owns run the handler set with [`set_external_handler`], and then compose again if that changed state.
///
/// It does nothing before the loop exists or when there is no window loop, as in [`preview`](crate::preview).
pub fn external_waker() -> Arc<dyn Fn() + Send + Sync> {
    Arc::new(|| {
        if let Some(proxy) = EXTERNAL_PROXY.get() {
            EXTERNAL_PENDING.store(true, Ordering::Release);
            proxy.wake_up();
        }
    })
}

/// Sets what the loop's thread runs when [`external_waker`] is called, replacing the previous handler. Call it from the thread that runs the loop, such as from a composable.
pub fn set_external_handler(handler: impl Fn() + 'static) {
    EXTERNAL_HANDLER.with(|slot| *slot.borrow_mut() = Some(Rc::new(handler)));
}

/// Runs the handler set with [`set_external_handler`] if [`external_waker`] was called since
/// it last ran.
pub(crate) fn run_external_handler() {
    if !EXTERNAL_PENDING.swap(false, Ordering::AcqRel) {
        return;
    }
    let handler = EXTERNAL_HANDLER.with(|slot| slot.borrow().clone());
    if let Some(handler) = handler {
        handler();
    }
}

/// Wakes the event loop of `proxy`, as the function [`App::new`] is given.
pub(crate) fn waker(proxy: EventLoopProxy) -> impl Fn() + Clone + Send + Sync + 'static {
    move || proxy.wake_up()
}

/// Composes `root` and shows it in a native window until closed.
///
/// The whole root is composed again, on the event loop, whenever state it
/// remembers has changed. Animations and tasks that wait on frames keep the
/// loop drawing for as long as they run; otherwise it sleeps until something happens.
///
/// Exits the process with status 1 if the window or renderer cannot be set up.
pub fn run(root: impl FnMut() + 'static) {
    run_with(WindowOptions::default(), root);
}

/// Like [`run`], with the first window opened as `options` describes.
///
/// Further windows are opened from the composition with [`window`](crate::window).
pub fn run_with(options: WindowOptions, root: impl FnMut() + 'static) {
    // Developer tool: draw one frame without a window and write it as a PNG.
    //   RUSTLE_SNAPSHOT=out.png [RUSTLE_SNAPSHOT_SIZE=1280x800] [RUSTLE_SNAPSHOT_LIGHT=1]
    //   [RUSTLE_SNAPSHOT_SCALE=2] [RUSTLE_SNAPSHOT_INPUT="click:x,y;type:text;key:Backspace"] replays input first
    if let Some(path) = std::env::var_os("RUSTLE_SNAPSHOT") {
        rustle_runtime::set_saver(Rc::new(saved::FileSaver::open()));
        let size = std::env::var("RUSTLE_SNAPSHOT_SIZE")
            .ok()
            .and_then(|size| {
                let (width, height) = size.split_once('x')?;
                Some((width.parse().ok()?, height.parse().ok()?))
            })
            .unwrap_or((1280, 800));
        let dark = std::env::var_os("RUSTLE_SNAPSHOT_LIGHT").is_none();
        if let Err(error) = snapshot(root, dark, Some(size), None, path.as_ref()) {
            eprintln!("rustle: {error}");
            process::exit(1);
        }
        return;
    }
    rustle_runtime::set_saver(Rc::new(saved::FileSaver::open()));
    let dark = Rc::new(Cell::new(true));
    let provided = provided(root, Rc::clone(&dark));
    let result = EventLoop::new().and_then(|event_loop| {
        let proxy = event_loop.create_proxy();
        let _ = EXTERNAL_PROXY.set(proxy.clone());
        let app = App::new(provided, dark, options, true, waker(proxy.clone()));
        // A new composition is dirty, so the first wake-up also composes it.
        proxy.wake_up();
        event_loop.set_control_flow(ControlFlow::Wait);
        event_loop.run_app(crate::handler::Running(app))
    });
    if let Err(error) = result {
        eprintln!("rustle: {error}");
        process::exit(1);
    }
}

/// Wraps `root` so it sees the main window's handle and the system appearance in `dark`.
pub(crate) fn provided(
    mut root: impl FnMut() + 'static,
    dark: Rc<Cell<bool>>,
) -> impl FnMut() + 'static {
    move || {
        crate::window::in_main_window(|| {
            rustle_foundation::LOCAL_SYSTEM_DARK.provide(dark.get(), &mut root)
        })
    }
}

pub(crate) struct App {
    /// Whether the system is set to a dark appearance, as the root sees it.
    pub(crate) dark: Rc<Cell<bool>>,
    pub(crate) composition: Composition,
    pub(crate) started: Instant,
    /// What the window being served shows, and what input did to it.
    pub(crate) ui: Interface,
    /// How the window looks when it opens.
    pub(crate) options: WindowOptions,
    /// Where the accessibility adapters of native windows send the platform's requests, or
    /// `None` when this application has no native windows.
    pub(crate) requests: Option<Requests>,
    /// The platform's accessibility requests, as the adapters queued them.
    pub(crate) requested: Receiver<(WindowId, Request)>,
    /// The files being dragged over a window, from the moment they enter it.
    pub(crate) file_drag: Option<FileDrag>,
    pub(crate) window: Option<WindowState>,
    /// The registry key of the window the fields above belong to.
    pub(crate) key: u64,
    /// The state of every window. The active one's is in the fields above and its
    /// entry here is empty (see `crate::windows`).
    pub(crate) slots: Vec<crate::windows::WindowCtx>,
    /// Which of `slots` is in the fields above. The main window is always slot 0.
    pub(crate) active: usize,
    pub(crate) error: Option<String>,
}

pub(crate) struct WindowState {
    pub(crate) adapter: Adapter,
    pub(crate) renderer: Renderer,
    pub(crate) window: Arc<dyn Window>,
}

impl App {
    /// An application that composes `root`, with no window until the event loop resumes.
    ///
    /// `wake` is called, from any thread, when state changed, a frame was requested or the
    /// platform's accessibility API asked for something, and must make the event loop call
    /// [`App::woken`]. `native` says whether the application opens native windows, which
    /// get accessibility adapters.
    pub(crate) fn new(
        root: impl FnMut() + 'static,
        dark: Rc<Cell<bool>>,
        options: WindowOptions,
        native: bool,
        wake: impl Fn() + Clone + Send + Sync + 'static,
    ) -> Self {
        let mut composition = rustle_ui::composition(root);
        let (queue, requested) = mpsc::channel();
        let requests = native.then(|| Requests {
            queue,
            wake: Arc::new(wake.clone()),
        });
        // State changes wake the event loop instead of being polled for.
        let invalidated = wake.clone();
        composition.on_invalidate(invalidated);
        // Animations ask for frames; only then does the loop keep redrawing.
        crate::window::set_wake(Rc::new(wake.clone()));
        composition.on_frame_request(wake);
        Self {
            dark,
            composition,
            started: Instant::now(),
            ui: Interface::new(),
            options,
            requests,
            requested,
            file_drag: None,
            window: None,
            key: crate::window::MAIN,
            slots: vec![crate::windows::WindowCtx::default()],
            active: 0,
            error: None,
        }
    }

    /// Lays the latest tree out for the current window and asks for a redraw.
    ///
    /// Text is measured by the renderer, so this waits until the window exists
    /// and the first pass has produced a tree.
    pub(crate) fn relayout(&mut self) {
        let tree = if self.key == crate::window::MAIN {
            self.composition.output().cloned()
        } else {
            crate::window::REGISTRY.with(|r| {
                r.borrow()
                    .entries
                    .get(&self.key)
                    .and_then(|e| e.tree.clone())
            })
        };
        let (Some(state), Some(tree)) = (&mut self.window, tree.as_ref()) else {
            return;
        };
        let scale_factor = state.window.scale_factor() as f32;
        let size = state.window.surface_size();
        let constraints = Constraints::loose(Size::new(
            size.width as f32 / scale_factor,
            size.height as f32 / scale_factor,
        ));
        self.ui.display = rustle_ui::display_list(tree, constraints, &mut state.renderer);
        self.ui.redraw = false;
        state.window.request_redraw();
        self.ui.update_hover();
        self.refresh_cursor();
        self.sync_ime_area();
        self.update_accessibility();
    }

    /// Records the system appearance and recomposes if it changed.
    pub(crate) fn set_dark(&mut self, dark: bool) {
        if self.dark.replace(dark) != dark {
            self.composition.invalidate();
        }
    }

    /// Runs what animations asked for at this frame and applies the state
    /// changes it caused, so the frame about to be drawn shows them.
    pub(crate) fn run_frame(&mut self) {
        if self.composition.has_pending_frame() {
            self.composition.run_frame(self.started.elapsed());
        }
        if self.composition.recompose() {
            self.relayout_all();
        }
    }

    /// Keeps frames coming for as long as something wants one.
    pub(crate) fn request_redraw_for_frames(&self) {
        if self.composition.has_pending_frame() {
            for state in self.native_windows() {
                state.window.request_redraw();
            }
        }
    }

    /// Records why the application cannot go on and stops the event loop.
    pub(crate) fn fail(&mut self, event_loop: &dyn ActiveEventLoop, error: impl ToString) {
        self.error = Some(error.to_string());
        event_loop.exit();
    }
}

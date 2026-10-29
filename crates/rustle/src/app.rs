//! The event loop: the entry point, the application state and frame running.

use std::cell::Cell;
use std::process;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use rustle_foundation::{Focus, Modifiers};
use rustle_geometry::{Offset, Rect, Size};
use rustle_ui::{Composition, DisplayList};
use rustle_unit::Constraints;
use rustle_wgpu::Renderer;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::Window;

use crate::preview::snapshot;
use crate::saved;

/// What wakes the event loop.
pub(crate) enum UserEvent {
    /// State changed or a frame was requested.
    Wake,
    /// The platform's accessibility API asked for something.
    Accessibility(accesskit_winit::Event),
}

impl From<accesskit_winit::Event> for UserEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        Self::Accessibility(event)
    }
}

/// Composes `root` and shows it in a native window until closed.
///
/// The whole root is composed again, on the event loop, whenever state it
/// remembers has changed. Animations and tasks that wait on frames keep the
/// loop drawing for as long as they run; otherwise it sleeps until something happens.
///
/// Exits the process with status 1 if the window or renderer cannot be set up.
pub fn run(mut root: impl FnMut() + 'static) {
    // Developer tool: draw one frame without a window and write it as a PNG.
    //   RUSTLE_SNAPSHOT=out.png [RUSTLE_SNAPSHOT_SIZE=1280x800] [RUSTLE_SNAPSHOT_LIGHT=1]
    //   [RUSTLE_SNAPSHOT_INPUT="click:x,y;type:text;key:Backspace"] replays input first
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
        if let Err(error) = snapshot(root, dark, size, path.as_ref()) {
            eprintln!("rustle: {error}");
            process::exit(1);
        }
        return;
    }
    rustle_runtime::set_saver(Rc::new(saved::FileSaver::open()));
    let dark = Rc::new(Cell::new(true));
    let provided = {
        let dark = Rc::clone(&dark);
        move || rustle_foundation::LOCAL_SYSTEM_DARK.provide(dark.get(), &mut root)
    };
    let mut app = App {
        dark,
        composition: rustle_ui::composition(provided),
        started: Instant::now(),
        display: DisplayList::default(),
        pointer: None,
        focused: None,
        typed: None,
        debug_input: std::env::var_os("RUSTLE_DEBUG_INPUT").is_some(),
        long_press: None,
        hovered: Vec::new(),
        following: Vec::new(),
        dragging: None,
        carrying: None,
        modifiers: Modifiers::default(),
        last_press: None,
        pressed: None,
        proxy: None,
        window: None,
        error: None,
    };

    let result = EventLoop::<UserEvent>::with_user_event()
        .build()
        .and_then(|event_loop| {
            // State changes wake the event loop instead of being polled for. A
            // new composition is dirty, so the first wake-up also composes it.
            let proxy = event_loop.create_proxy();
            let wake = proxy.clone();
            app.composition.on_invalidate(move || {
                let _ = wake.send_event(UserEvent::Wake);
            });
            // Animations ask for frames; only then does the loop keep redrawing.
            let wake = proxy.clone();
            app.composition.on_frame_request(move || {
                let _ = wake.send_event(UserEvent::Wake);
            });
            app.proxy = Some(proxy.clone());
            let _ = proxy.send_event(UserEvent::Wake);
            event_loop.set_control_flow(ControlFlow::Wait);
            event_loop.run_app(&mut app)
        });
    if let Err(error) = result {
        app.error = Some(error.to_string());
    }
    if let Some(error) = app.error {
        eprintln!("rustle: {error}");
        process::exit(1);
    }
}

/// An area following the pointer, and what to tell it when the pointer leaves.
pub(crate) type FollowEntry = (Rect, Rc<dyn Fn(Option<Offset>)>);

/// An area the pointer is over, and what to tell it when the pointer leaves.
pub(crate) type HoverEntry = (Rect, Rc<dyn Fn(bool)>);

/// A press that becomes a long press if it is held: when, what, and where it began.
pub(crate) type LongPress = (Instant, Rc<dyn Fn()>, Offset);

/// Something picked up by a drag source: what it carries, where the press began, and
/// whether the pointer has moved far enough for it to count as a drag.
pub(crate) struct Carrying {
    pub(crate) payload: Rc<dyn std::any::Any>,
    pub(crate) began: Offset,
    pub(crate) active: bool,
    /// Where the picked-up area is, and which of the display list's commands draw it.
    pub(crate) rect: Rect,
    pub(crate) commands: std::ops::Range<usize>,
}

/// A drag in progress: its handler, and where its area's corner is.
pub(crate) type Drag = (Rc<dyn Fn(Offset)>, Offset, Option<Rc<dyn Fn()>>);

pub(crate) struct App {
    /// Whether the system is set to a dark appearance, as the root sees it.
    pub(crate) dark: Rc<Cell<bool>>,
    pub(crate) composition: Composition,
    pub(crate) started: Instant,
    pub(crate) display: DisplayList,
    /// Where the pointer is, in logical pixels, while it is over the window.
    pub(crate) pointer: Option<Offset>,
    /// What has keyboard focus.
    pub(crate) focused: Option<Focus>,
    /// Text a key press just typed, so the same text arriving again from an input method is not typed twice.
    pub(crate) typed: Option<(Instant, String)>,
    /// Whether to print the input the window receives (`RUSTLE_DEBUG_INPUT`).
    pub(crate) debug_input: bool,
    /// A long press waiting for its time: when it is due, what to run, and where it began.
    pub(crate) long_press: Option<LongPress>,
    /// The hover handlers of the areas the pointer is over.
    pub(crate) hovered: Vec<HoverEntry>,
    /// The areas whose pointer-move handlers have been told where the pointer is.
    pub(crate) following: Vec<FollowEntry>,
    /// The handler following a drag, and where its area's corner is.
    pub(crate) dragging: Option<Drag>,
    /// What was picked up with `drag_source`, and where, until the button is released.
    pub(crate) carrying: Option<Carrying>,
    pub(crate) modifiers: Modifiers,
    /// When and where the primary button last went down, and how many in a row.
    pub(crate) last_press: Option<(Instant, Offset, u32)>,
    /// The handler of the region the primary button went down on.
    /// Where the primary button went down on something clickable. Areas are
    /// told apart by where they are, not by their handlers, because every
    /// recomposition makes new handlers, and an animation recomposes every frame.
    pub(crate) pressed: Option<Rect>,
    pub(crate) proxy: Option<EventLoopProxy<UserEvent>>,
    pub(crate) window: Option<WindowState>,
    pub(crate) error: Option<String>,
}

pub(crate) struct WindowState {
    pub(crate) adapter: accesskit_winit::Adapter,
    pub(crate) renderer: Renderer,
    pub(crate) window: Arc<Window>,
}

impl App {
    /// Lays the latest tree out for the current window and asks for a redraw.
    ///
    /// Text is measured by the renderer, so this waits until the window exists
    /// and the first pass has produced a tree.
    pub(crate) fn relayout(&mut self) {
        let (Some(state), Some(tree)) = (&mut self.window, self.composition.output()) else {
            return;
        };
        let scale_factor = state.window.scale_factor() as f32;
        let size = state.window.inner_size();
        let constraints = Constraints::loose(Size::new(
            size.width as f32 / scale_factor,
            size.height as f32 / scale_factor,
        ));
        self.display = rustle_ui::display_list(tree, constraints, &mut state.renderer);
        state.window.request_redraw();
        self.update_hover();
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
            self.relayout();
        }
    }

    /// Keeps frames coming for as long as something wants one.
    pub(crate) fn request_redraw_for_frames(&self) {
        if self.composition.has_pending_frame()
            && let Some(state) = &self.window
        {
            state.window.request_redraw();
        }
    }

    pub(crate) fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl ToString) {
        self.error = Some(error.to_string());
        event_loop.exit();
    }
}

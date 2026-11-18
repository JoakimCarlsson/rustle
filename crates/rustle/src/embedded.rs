//! The interface drawn into a texture of a host that owns the window, the device and the frame.

use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustle_foundation::{KeyEvent, Modifiers};
use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::DrawCommand;
use rustle_ui::{
    Composition, CursorIcon, PointerButton, PointerId, PointerInputPhase, PointerKind,
};
use rustle_unit::Constraints;
use rustle_wgpu::{Renderer, TextMetrics};

use crate::app::provided;
use crate::input;
use crate::interface::Interface;

/// How many times a layout that changed state is laid out again before it is taken as it is.
const SETTLE_ROUNDS: usize = 8;

/// What lays the interface out, and what draws it if it is drawn here.
enum Backend {
    Drawing(Box<Renderer>),
    Metrics(TextMetrics),
}

/// What wakes the host.
type Waker = Arc<dyn Fn() + Send + Sync>;

/// A clock the host sets: the time of the frame [`Embedded::update`] runs, since a start the
/// host chooses.
type Clock = Box<dyn Fn() -> Duration>;

/// A rustle application inside a host that owns the window, the GPU device and the frame.
///
/// [`Host`](crate::Host) opens its own window on an event loop the caller owns. An `Embedded`
/// opens nothing: the host tells it how big its area is, gives it the input that lands there,
/// and draws it into a texture of its own device. Windows opened with
/// [`window`](crate::window) from its root are not shown.
///
/// The host calls, in a frame:
///
/// 1. [`resize`](Embedded::resize) when the area or its scale factor changed;
/// 2. the input methods ([`pointer_moved`](Embedded::pointer_moved),
///    [`pointer_button`](Embedded::pointer_button), [`wheel`](Embedded::wheel),
///    [`key_event`](Embedded::key_event), and for an input method
///    [`preedit`](Embedded::preedit) and [`commit`](Embedded::commit)), each of which says
///    whether the interface used the event, so the host can keep it from whatever lies beneath;
/// 3. [`update`](Embedded::update) to run what animations asked for and lay out what state
///    changes dirtied;
/// 4. [`render_into`](Embedded::render_into) to draw the interface.
///
/// Composition, layout and drawing all happen on the calling thread, because the state of a
/// composition is not `Send`.
///
/// # Several on one thread
///
/// Any number of `Embedded` can live on a thread, each with its own renderer and root. Their
/// renderers share one font system, so a font added with
/// [`register_font`](rustle_text::register_font) reaches every one of them and the system's
/// fonts are loaded once. What is global to the thread is shared by all of them: the saver
/// that backs [`remember_saveable`](crate::remember_saveable), the clipboard, and the
/// composition locals the root reads, which start out as their defaults in each.
pub struct Embedded {
    composition: Composition,
    ui: Interface,
    backend: Backend,
    size: Size,
    scale_factor: f32,
    started: Instant,
    clock: Option<Clock>,
    dark: Rc<Cell<bool>>,
    wake: Waker,
}

/// A composition of `root` that calls `wake` when state changed or a frame was requested.
fn composition_of(
    root: impl FnMut() + 'static,
    dark: &Rc<Cell<bool>>,
    wake: &Waker,
) -> Composition {
    let mut composition = rustle_ui::composition(provided(root, Rc::clone(dark)));
    let invalidated = Arc::clone(wake);
    composition.on_invalidate(move || invalidated());
    let requested = Arc::clone(wake);
    composition.on_frame_request(move || requested());
    composition
}

impl Embedded {
    /// An interface that composes `root` and draws with `renderer`, which was made with
    /// [`Renderer::with_device`].
    ///
    /// `wake` is called, from any thread, when state changed or an animation wants a frame,
    /// and must make the host call [`update`](Embedded::update) soon.
    pub fn new(
        root: impl FnMut() + 'static,
        renderer: Renderer,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self::with_backend(root, Backend::Drawing(Box::new(renderer)), wake)
    }

    /// An interface that composes `root` and lays it out, but does not draw it: the host takes
    /// [`commands`](Embedded::commands) and draws them with a renderer of its own, which may be
    /// on another thread. [`render_into`](Embedded::render_into) returns an error.
    ///
    /// Text is measured with the fonts of this thread, without a GPU. `wake` is as for
    /// [`new`](Embedded::new).
    pub fn without_renderer(
        root: impl FnMut() + 'static,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self::with_backend(root, Backend::Metrics(TextMetrics::new()), wake)
    }

    /// An interface that lays out with `backend`.
    fn with_backend(
        root: impl FnMut() + 'static,
        backend: Backend,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let dark = Rc::new(Cell::new(true));
        let wake: Waker = Arc::new(wake);
        rustle_ui::set_clipboard(Rc::new(input::SystemClipboard::new()));
        let mut composition = composition_of(root, &dark, &wake);
        composition.compose();
        let mut ui = Interface::new();
        ui.apply_focus_request();
        Self {
            composition,
            ui,
            backend,
            size: Size::new(0.0, 0.0),
            scale_factor: 1.0,
            started: Instant::now(),
            clock: None,
            dark,
            wake,
        }
    }

    /// Replaces the root composable with `root`, keeping the renderer, its fonts and its
    /// caches, and the size, scale and appearance the interface was given.
    ///
    /// The interface starts again from nothing: the old composition is dropped, which runs the
    /// cleanups of its effects, so the state it remembered and its focus are gone and the
    /// new root composes with fresh state. Where the pointer is and which modifier keys are
    /// held are kept. A host that reloads the library `root` came from calls this with the
    /// new one before it unloads the old, because dropping the old composition runs code from it.
    pub fn set_root(&mut self, root: impl FnMut() + 'static) {
        let mut composition = composition_of(root, &self.dark, &self.wake);
        composition.compose();
        self.composition = composition;
        self.ui.clear();
        self.started = Instant::now();
        self.lay_out();
        self.ui.apply_focus_request();
    }

    /// Tells the interface its area is `size` physical pixels with `scale_factor` physical
    /// pixels to a logical one, and lays it out.
    pub fn resize(&mut self, size: (u32, u32), scale_factor: f32) {
        if let Backend::Drawing(renderer) = &mut self.backend {
            renderer.resize(size.0, size.1);
        }
        self.scale_factor = scale_factor;
        self.size = Size::new(size.0 as f32 / scale_factor, size.1 as f32 / scale_factor);
        self.lay_out();
        self.ui.apply_focus_request();
    }

    /// Composes the interface again on the next [`update`](Embedded::update), for a change the
    /// host knows about that no state records, such as data the root reads from the host.
    pub fn invalidate(&self) {
        self.composition.invalidate();
    }

    /// The size of the area in logical pixels, as given to [`resize`](Embedded::resize).
    pub fn size(&self) -> Size {
        self.size
    }

    /// The physical pixels to a logical one, as given to [`resize`](Embedded::resize).
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// Sets whether the interface looks dark, and composes it again if that changed.
    pub fn set_dark(&mut self, dark: bool) {
        if self.dark.replace(dark) != dark {
            self.composition.invalidate();
        }
    }

    /// Runs what animations asked for at this frame and composes and lays out again what state
    /// changes dirtied. Call it once per frame and whenever the `wake` given to
    /// [`new`](Embedded::new) was called.
    ///
    /// Returns whether the interface needs to be drawn again: it changed, input changed how it
    /// looks, or an animation wants another frame.
    pub fn update(&mut self) -> bool {
        if self.composition.has_pending_frame() {
            self.composition.run_frame(self.frame_time());
        }
        let changed = self.composition.recompose();
        if changed {
            self.lay_out();
        }
        self.ui.apply_focus_request();
        let redraw = self.ui.take_redraw();
        changed || redraw || self.composition.has_pending_frame()
    }

    /// Sets the clock that gives the time of each frame [`update`](Embedded::update) runs,
    /// which is what animations and the delays of tasks started with
    /// [`launched_effect`](crate::launched_effect) measure.
    ///
    /// By default frames are timed with the real clock, from when the interface (or its latest
    /// root) was made. A host that draws frames of its own timing, such as an offscreen
    /// snapshot "500 ms after opening", gives a clock here so the result does not depend on how
    /// long the work took. `clock` must never go backwards.
    pub fn set_clock(&mut self, clock: impl Fn() -> Duration + 'static) {
        self.clock = Some(Box::new(clock));
    }

    /// Times frames with the real clock again, from now, undoing
    /// [`set_clock`](Embedded::set_clock) and [`advance_to`](Embedded::advance_to).
    pub fn use_real_clock(&mut self) {
        self.clock = None;
        self.started = Instant::now();
    }

    /// Stops the clock at `time` and runs [`update`](Embedded::update) at it.
    ///
    /// The clock stays at `time` until it is set again, so calling this with growing times
    /// steps the interface through frames deterministically, as tests and snapshots need.
    /// `time` must not be earlier than the frame before. Returns what `update` returns.
    pub fn advance_to(&mut self, time: Duration) -> bool {
        self.set_clock(move || time);
        self.update()
    }

    /// The time the next frame [`update`](Embedded::update) runs would be given: the host's
    /// clock if it set one, else the real time since the interface (or its latest root) was made.
    pub fn frame_time(&self) -> Duration {
        self.clock
            .as_ref()
            .map_or_else(|| self.started.elapsed(), |clock| clock())
    }

    /// Whether an animation or a task wants another frame.
    pub fn wants_frame(&self) -> bool {
        self.composition.has_pending_frame()
    }

    /// Sets the modifier keys that are held, for pointer events and the keys that follow.
    pub fn set_modifiers(&mut self, modifiers: Modifiers) {
        self.ui.modifiers = modifiers;
    }

    /// The pointer moved to `position`, in logical pixels from the area's top-left corner.
    ///
    /// Returns whether the pointer is over the interface: something that reacts to it, or an
    /// area that blocks it (see [`Modifier::blocks_pointer`](crate::Modifier::blocks_pointer)).
    pub fn pointer_moved(&mut self, position: Offset) -> bool {
        self.ui.pointer_moved(position);
        self.ui.apply_focus_request();
        self.ui.display.interactive_at(position)
    }

    /// The pointer left the area.
    pub fn pointer_left(&mut self) {
        self.ui.pointer_left();
    }

    /// A pointer button went down or up where the pointer is.
    ///
    /// Returns whether the pointer is over the interface, in which case the button belongs to
    /// it, as for [`pointer_moved`](Embedded::pointer_moved).
    pub fn pointer_button(&mut self, button: PointerButton, pressed: bool) -> bool {
        let over = self.ui.pointer_over_interface();
        self.ui.button_input(button, pressed);
        self.ui.apply_focus_request();
        over
    }

    /// A contact (a finger or a pen, or a pointer the host tracks itself) changed: `id` tells
    /// contacts apart, `phase` is [`Down`](PointerInputPhase::Down),
    /// [`Move`](PointerInputPhase::Move), [`Up`](PointerInputPhase::Up) or
    /// [`Cancel`](PointerInputPhase::Cancel), `position` is in logical pixels from the area's
    /// top-left corner and `pressure` runs from 0 to 1 (0.5 when the device does not say).
    ///
    /// Several contacts can be down at once, which is what multi-touch gestures (see
    /// [`Modifier::transformable`](rustle_ui::Modifier::transformable)) follow. The first to
    /// go down also acts as the primary button, as for [`pointer_button`](Embedded::pointer_button),
    /// until it is lifted. Returns whether the interface used it: the contact is over an area
    /// that reacts to the pointer or blocks it, or one that went down on such an area was moved.
    pub fn contact(
        &mut self,
        id: u64,
        kind: PointerKind,
        phase: PointerInputPhase,
        position: Offset,
        pressure: f32,
    ) -> bool {
        let over = self.ui.display.interactive_at(position)
            || self
                .ui
                .pointers
                .iter()
                .any(|pointer| pointer.id == PointerId(id));
        self.ui
            .contact(PointerId(id), kind, phase, position, pressure);
        self.ui.apply_focus_request();
        over
    }

    /// The wheel or a trackpad scrolled by `delta` logical pixels where the pointer is.
    ///
    /// Returns whether the interface took it: a scrolling area under the pointer scrolled, or
    /// the pointer is over an area that blocks it.
    pub fn wheel(&mut self, delta: Offset) -> bool {
        let taken = self.ui.wheel(delta);
        taken
            || self
                .ui
                .pointer
                .is_some_and(|pointer| self.ui.display.blocks_pointer_at(pointer))
    }

    /// A key went down or up, or text was typed.
    ///
    /// Returns whether the interface used it: an area handled it, or it moved or dropped
    /// focus. A host gives keys to the game only when this is `false`.
    pub fn key_event(&mut self, event: KeyEvent) -> bool {
        let typed = event.text.is_some();
        let used = self.ui.keyboard(event);
        self.ui.apply_focus_request();
        used || (self.ui.focused.is_some() && typed)
    }

    /// The input method is composing `text`, with the part it highlights at the byte range
    /// `cursor`. The focused text field shows it at its caret, underlined, and keeps it out
    /// of its value. An empty `text` ends the composition.
    pub fn preedit(&mut self, text: &str, cursor: Option<(usize, usize)>) {
        self.ui.preedit(text.to_string(), cursor);
    }

    /// The input method finished composing and produced `text`, which ends the composition and
    /// is typed into the focused field.
    ///
    /// Returns whether the interface took it, which it does while something has focus.
    pub fn commit(&mut self, text: &str) -> bool {
        let used = self.ui.commit(text.to_string());
        self.ui.apply_focus_request();
        used
    }

    /// Where the caret of the focused text field is, in logical pixels from the area's top-left
    /// corner, for placing the input method's candidate popup. `None` while no field is edited.
    /// Ask after [`update`](Embedded::update), because the caret moves when the layout does.
    pub fn ime_area(&self) -> Option<Rect> {
        self.ui.caret_area()
    }

    /// Whether something in the interface has keyboard focus, such as a text field, so the
    /// host should send it the keyboard and allow the input method, and not give the keys to
    /// the game.
    pub fn has_focus(&self) -> bool {
        self.ui.focused.is_some()
    }

    /// The cursor the area under the pointer asks for.
    pub fn cursor(&self) -> CursorIcon {
        self.ui.cursor()
    }

    /// Everything that is drawn this frame, including the feedback over the area under the
    /// pointer and what is being carried.
    pub fn commands(&self) -> Cow<'_, [DrawCommand]> {
        self.ui.commands()
    }

    /// Draws the interface into `view`, a texture of the renderer's format and the size given
    /// to [`resize`](Embedded::resize), on `encoder`, which the host submits.
    ///
    /// `clear` is the color to clear `view` to first, or `None` to draw over what it holds.
    ///
    /// # Errors
    ///
    /// Returns an error if the renderer could not prepare what the frame draws.
    pub fn render_into(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size: (u32, u32),
        clear: Option<wgpu::Color>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let Backend::Drawing(renderer) = &mut self.backend else {
            return Err("this interface was made without a renderer".into());
        };
        let commands = self.ui.commands().into_owned();
        renderer.render_into(encoder, view, size, &commands, self.scale_factor, clear)
    }

    /// Lays the latest tree out for the area's size, again if laying out changed state.
    fn lay_out(&mut self) {
        let constraints = Constraints::loose(self.size);
        for _ in 0..SETTLE_ROUNDS {
            let Some(tree) = self.composition.output().cloned() else {
                return;
            };
            let measure: &mut dyn rustle_text::TextMeasurer = match &mut self.backend {
                Backend::Drawing(renderer) => renderer.as_mut(),
                Backend::Metrics(metrics) => metrics,
            };
            self.ui.display = rustle_ui::display_list(&tree, constraints, measure);
            if !self.composition.recompose() {
                break;
            }
        }
        self.ui.update_hover();
    }
}

//! Several native windows driven by one composition.
//!
//! Everything about the window an event is for, such as its display list, pointer and
//! focus, lives in `App`'s own fields, because the input handling is written against them.
//! To serve several windows, the fields of the window not being served are parked in
//! `App::slots` and swapped in when one of its events arrives ([`App::activate`]). Anything
//! that is per window and added to `App` belongs in [`WindowCtx`] and [`App::exchange`].

use std::mem;
use std::sync::Arc;

use rustle_wgpu::Renderer;
use winit::event_loop::ActiveEventLoop;
use winit::window::{
    ImeCapabilities, ImeEnableRequest, ImeRequest, ImeRequestData, Theme as WinitTheme, Window,
    WindowId,
};

use crate::accessibility::{Adapter, Request};
use crate::app::{App, NativeWindow};
use crate::interface::Interface;
use crate::window::{Entry, MAIN, REGISTRY, WindowOptions, WindowTheme};

/// The state of one window while another is being served.
#[derive(Default)]
pub(crate) struct WindowCtx {
    pub(crate) key: u64,
    pub(crate) ui: Interface,
    pub(crate) options: WindowOptions,
    pub(crate) window: Option<NativeWindow>,
}

impl App {
    /// Swaps the per-window fields with `ctx`.
    fn exchange(&mut self, ctx: &mut WindowCtx) {
        mem::swap(&mut self.key, &mut ctx.key);
        self.ui.exchange(&mut ctx.ui);
        mem::swap(&mut self.options, &mut ctx.options);
        mem::swap(&mut self.window, &mut ctx.window);
    }

    /// Makes the window in slot `index` the one the per-window fields belong to.
    pub(crate) fn activate(&mut self, index: usize) {
        if index == self.active {
            return;
        }
        let mut ctx = mem::take(&mut self.slots[index]);
        self.exchange(&mut ctx);
        self.slots[self.active] = ctx;
        self.active = index;
    }

    /// Moves focus as the application asked, in the window the asked-for area was painted
    /// in (the active one when none painted it), after taking it from what the application
    /// asked to free in whichever window holds that, so a focus requested for a control of
    /// another window is given there and not to the window that happens to be active.
    pub(crate) fn apply_focus_request(&mut self) {
        let release = rustle_ui::take_focus_release();
        let request = rustle_ui::take_focus_request();
        let before = self.active;
        if let Some(freed) = release {
            for index in 0..self.slots.len() {
                self.activate(index);
                self.ui.apply_focus(Some(freed), None);
            }
            self.activate(before);
        }
        let Some(focus) = request else {
            return;
        };
        let target = (0..self.slots.len())
            .find(|&index| {
                if index == self.active {
                    self.ui.shows_focus(&focus)
                } else {
                    self.slots[index].ui.shows_focus(&focus)
                }
            })
            .unwrap_or(before);
        self.activate(target);
        self.ui.apply_focus(None, Some(focus));
        self.activate(before);
    }

    /// The window in slot `index`, if it is open.
    pub(crate) fn slot_window(&self, index: usize) -> Option<&NativeWindow> {
        if index == self.active {
            self.window.as_ref()
        } else {
            self.slots[index].window.as_ref()
        }
    }

    /// The registry key of the window in slot `index`.
    fn slot_key(&self, index: usize) -> u64 {
        if index == self.active {
            self.key
        } else {
            self.slots[index].key
        }
    }

    /// The main window, which is always slot 0.
    pub(crate) fn main_window(&self) -> Option<&Arc<dyn Window>> {
        self.slot_window(0).map(|state| &state.window)
    }

    /// Every window that is open.
    pub(crate) fn native_windows(&self) -> impl Iterator<Item = &NativeWindow> {
        (0..self.slots.len()).filter_map(|index| self.slot_window(index))
    }

    /// The slot of the window with `id`.
    pub(crate) fn slot_of(&self, id: WindowId) -> Option<usize> {
        (0..self.slots.len()).find(|&index| {
            self.slot_window(index)
                .is_some_and(|state| state.window.id() == id)
        })
    }

    /// Lays every window out again, leaving the active window as it was.
    pub(crate) fn relayout_all(&mut self) {
        let before = self.active;
        for index in 0..self.slots.len() {
            self.activate(index);
            self.relayout();
        }
        self.activate(before);
    }

    /// Paints every window again from the tree it laid out last, without composing or
    /// measuring, and asks each for a redraw.
    pub(crate) fn repaint_all(&mut self) {
        let before = self.active;
        for index in 0..self.slots.len() {
            self.activate(index);
            self.ui.repaint();
            if let Some(state) = &self.window {
                state.window.request_redraw();
            }
        }
        self.activate(before);
    }

    /// Creates a native window with its renderer and accessibility adapter.
    pub(crate) fn open_native(
        &self,
        event_loop: &dyn ActiveEventLoop,
        options: &WindowOptions,
    ) -> Result<NativeWindow, String> {
        let parent = options.parent.and_then(|parent| {
            REGISTRY.with(|r| r.borrow().entries.get(&parent.key)?.window.clone())
        });
        let attributes = options.attributes(parent.as_deref());
        let attributes = options.named(attributes, event_loop);
        let window: Arc<dyn Window> = Arc::from(
            event_loop
                .create_window(attributes)
                .map_err(|error| error.to_string())?,
        );
        let instance = shared_instance(event_loop);
        let requests = self
            .requests
            .as_ref()
            .ok_or("this application has no native windows")?;
        let adapter = Adapter::new(&*window, requests);
        if options.visible {
            window.set_visible(true);
            if options.modal {
                window.focus_window();
            }
        }
        let size = window.surface_size();
        let renderer = Renderer::with_transparency(
            instance,
            window.clone(),
            (size.width, size.height),
            options.transparent,
        )
        .map_err(|error| error.to_string())?;
        Ok(NativeWindow {
            adapter,
            renderer,
            window,
        })
    }

    /// Records the main window in the registry, where [`WindowHandle`](crate::WindowHandle)s find it.
    pub(crate) fn register_main(&self) {
        let window = self.window.as_ref().map(|state| Arc::clone(&state.window));
        REGISTRY.with(|r| {
            r.borrow_mut().entries.insert(
                MAIN,
                Entry {
                    options: self.options.clone(),
                    tree: None,
                    on_close: None,
                    window,
                    closed: false,
                    info: self.main_info.clone(),
                },
            );
        });
    }

    /// Does what composition and the window handles asked for: opens windows that appeared,
    /// closes ones that left, applies changed options, forgets presses the system took over
    /// and runs close requests.
    pub(crate) fn sync_windows(&mut self, event_loop: &dyn ActiveEventLoop) {
        let (cancelled, closes) = REGISTRY.with(|r| {
            let mut registry = r.borrow_mut();
            (
                mem::take(&mut registry.cancel_press),
                mem::take(&mut registry.close_requests),
            )
        });
        for key in cancelled {
            if let Some(index) = (0..self.slots.len()).find(|&i| self.slot_key(i) == key) {
                self.activate(index);
                self.ui.pressed = None;
                self.ui.dragging = None;
                self.ui.carrying = None;
                self.ui.long_press = None;
            }
        }
        for key in closes {
            if key == MAIN {
                if crate::app::main_may_close() {
                    event_loop.exit();
                }
                continue;
            }
            close_requested(key);
        }
        self.apply_window_states();
        if !REGISTRY.with(|r| mem::take(&mut r.borrow_mut().dirty)) {
            return;
        }
        let wanted = wanted_windows();
        // Windows that left the composition. Slot 0, the main window, never does.
        self.activate(0);
        let mut index = 1;
        while index < self.slots.len() {
            let key = self.slots[index].key;
            if wanted.iter().any(|(wanted, _)| *wanted == key) {
                index += 1;
            } else {
                self.slots.remove(index);
            }
        }
        for (key, options) in wanted {
            if let Some(index) = (1..self.slots.len()).find(|&i| self.slots[i].key == key) {
                self.apply_options(index, options);
                continue;
            }
            match self.open_native(event_loop, &options) {
                Ok(state) => {
                    REGISTRY.with(|r| {
                        if let Some(entry) = r.borrow_mut().entries.get_mut(&key) {
                            entry.window = Some(Arc::clone(&state.window));
                        }
                    });
                    self.slots.push(WindowCtx {
                        key,
                        options,
                        window: Some(state),
                        ..WindowCtx::default()
                    });
                    self.activate(self.slots.len() - 1);
                    self.set_dark(
                        self.window.as_ref().and_then(|state| state.window.theme())
                            != Some(WinitTheme::Light),
                    );
                    self.relayout();
                    self.activate(0);
                }
                Err(error) => return self.fail(event_loop, error),
            }
        }
    }

    /// Changes each open window whose [`WindowState`](crate::WindowState) the application
    /// changed since it was last applied or read.
    pub(crate) fn apply_window_states(&self) {
        for index in 0..self.slots.len() {
            let options = if index == self.active {
                &self.options
            } else {
                &self.slots[index].options
            };
            if let (Some(state), Some(open)) = (&options.state, self.slot_window(index)) {
                state.apply_to(&*open.window);
            }
        }
    }

    /// Reads what the active window is now into its [`WindowState`](crate::WindowState),
    /// after the user or the system moved, resized, maximized or minimized it.
    pub(crate) fn observe_window_state(&self) {
        if let (Some(state), Some(open)) = (&self.options.state, &self.window) {
            state.observe(&*open.window);
        }
    }

    /// Applies the options of an open secondary window that changed.
    fn apply_options(&mut self, index: usize, options: WindowOptions) {
        let ctx = &mut self.slots[index];
        if ctx.options == options {
            return;
        }
        if let Some(state) = &ctx.window {
            let window: &dyn Window = &*state.window;
            if ctx.options.title != options.title {
                window.set_title(&options.title);
            }
            if ctx.options.decorations != options.decorations {
                window.set_decorations(options.decorations);
            }
            if ctx.options.resizable != options.resizable {
                window.set_resizable(options.resizable);
            }
            if ctx.options.min_size != options.min_size {
                window.set_min_surface_size(Some(
                    winit::dpi::LogicalSize::new(options.min_size.0, options.min_size.1).into(),
                ));
            }
            if ctx.options.fullscreen != options.fullscreen {
                window.set_fullscreen(
                    options
                        .fullscreen
                        .then_some(winit::monitor::Fullscreen::Borderless(None)),
                );
            }
            if ctx.options.position != options.position
                && let Some((x, y)) = options.position
            {
                window.set_outer_position(winit::dpi::LogicalPosition::new(x, y).into());
            }
            if ctx.options.theme != options.theme {
                window.set_theme(options.theme.map(WindowTheme::winit));
            }
            if ctx.options.always_on_top != options.always_on_top {
                window.set_window_level(if options.always_on_top {
                    winit::window::WindowLevel::AlwaysOnTop
                } else {
                    winit::window::WindowLevel::Normal
                });
            }
            if !ctx.options.modal && options.modal {
                window.focus_window();
            }
            if !options.ime_allowed {
                let _ = window.request_ime_update(ImeRequest::Disable);
            }
        }
        ctx.options = options;
    }

    /// Handles an event of the window `id`, whichever it is.
    pub(crate) fn window_event_for(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        id: WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(index) = self.slot_of(id) else {
            return;
        };
        if self.file_drag_event(event_loop, index, id, &event) {
            self.sync_windows(event_loop);
            return;
        }
        if let Some(modal) = self.blocking_modal(index)
            && is_input(&event)
        {
            if let Some(window) = self.slot_window(modal) {
                window.window.focus_window();
            }
            return;
        }
        self.activate(index);
        if self.key != MAIN && matches!(event, winit::event::WindowEvent::CloseRequested) {
            // The composition decides whether the window goes.
            REGISTRY.with(|r| r.borrow_mut().close_requests.push(self.key));
        } else {
            self.window_event(event_loop, event);
        }
        self.sync_windows(event_loop);
    }

    /// The slot of an open modal window that blocks input to the window in slot `index`;
    /// see [`modal_over`].
    pub(crate) fn blocking_modal(&self, index: usize) -> Option<usize> {
        let open: Vec<Option<(u64, &WindowOptions)>> = (0..self.slots.len())
            .map(|slot| {
                self.slot_window(slot)?;
                let options = if slot == self.active {
                    &self.options
                } else {
                    &self.slots[slot].options
                };
                Some((self.slot_key(slot), options))
            })
            .collect();
        modal_over(&open, index)
    }

    /// Answers a request of the platform's accessibility API for the window `id`.
    pub(crate) fn accessibility_event_for(&mut self, id: WindowId, event: Request) {
        if let Some(index) = self.slot_of(id) {
            self.activate(index);
            self.accessibility_event(event);
        }
    }

    /// Whether input methods may be used in the active window.
    pub(crate) fn ime_allowed(&self) -> bool {
        self.options.ime_allowed
    }

    /// Shows the cursor the area under a stationary pointer asks for, after a new layout.
    pub(crate) fn refresh_cursor(&self) {
        if self.ui.pointer.is_some()
            && let Some(state) = &self.window
            && !self.ui.carrying.as_ref().is_some_and(|c| c.active)
        {
            state
                .window
                .set_cursor(winit_cursor(self.ui.cursor()).into());
        }
    }

    /// Does what input asked of the window: redraws, shows the cursor, and allows or forbids
    /// the input method as focus moves.
    pub(crate) fn apply_effects(&mut self) {
        self.ui.refresh_interactions();
        if self.ui.take_focus_changed() {
            // An input method only gets keys while something wants text, so it
            // cannot take keys from the rest of the application.
            self.ui.ime_area = None;
            let allowed = self.ui.focused.is_some() && self.ime_allowed();
            if let Some(state) = &self.window {
                let _ = state.window.request_ime_update(ImeRequest::Disable);
                if allowed {
                    let _ = state.window.request_ime_update(ime_enable());
                }
            }
            self.sync_ime_area();
        }
        if self.ui.take_redraw()
            && let Some(state) = &self.window
        {
            state.window.request_redraw();
        }
    }

    /// Shows the cursor for a pointer that just moved.
    pub(crate) fn show_cursor(&self) {
        if let Some(state) = &self.window {
            state
                .window
                .set_cursor(winit_cursor(self.ui.cursor()).into());
        }
    }

    /// Tells the window where the caret is, once per move, so the input method's
    /// candidate popup appears next to it.
    pub(crate) fn sync_ime_area(&mut self) {
        let area = self.ui.caret_area();
        if area == self.ui.ime_area {
            return;
        }
        self.ui.ime_area = area;
        if let (Some(area), Some(state)) = (area, &self.window) {
            let scale = state.window.scale_factor();
            let _ = state.window.request_ime_update(ImeRequest::Update(
                ImeRequestData::default()
                    .with_cursor_area(ime_position(area, scale), ime_size(area, scale)),
            ));
        }
    }
}

/// The request that turns the input method on, before the caret it serves is known.
fn ime_enable() -> ImeRequest {
    let data = ImeRequestData::default().with_cursor_area(
        winit::dpi::LogicalPosition::new(0.0, 0.0).into(),
        winit::dpi::LogicalSize::new(0.0, 0.0).into(),
    );
    let capabilities = ImeCapabilities::new().with_cursor_area();
    ImeEnableRequest::new(capabilities, data).map_or(ImeRequest::Disable, ImeRequest::Enable)
}

/// Where the caret `area` is, for the input method, in physical pixels.
fn ime_position(area: rustle_geometry::Rect, scale_factor: f64) -> winit::dpi::Position {
    winit::dpi::LogicalPosition::new(f64::from(area.origin.x), f64::from(area.origin.y))
        .to_physical::<f64>(scale_factor)
        .into()
}

/// How large the caret `area` is, for the input method, in physical pixels.
fn ime_size(area: rustle_geometry::Rect, scale_factor: f64) -> winit::dpi::Size {
    winit::dpi::LogicalSize::new(f64::from(area.size.width), f64::from(area.size.height))
        .to_physical::<f64>(scale_factor)
        .into()
}

/// Runs the close request of the secondary window `key`: its `on_close_request` runs, and a
/// window that closes itself is marked closed, so it goes and is not opened again until its
/// call leaves the composition.
pub(crate) fn close_requested(key: u64) {
    let on_close = REGISTRY.with(|r| r.borrow().entries.get(&key)?.on_close.clone());
    if let Some(on_close) = on_close {
        on_close();
    }
    REGISTRY.with(|r| {
        let mut registry = r.borrow_mut();
        if let Some(entry) = registry.entries.get_mut(&key)
            && entry.options.closes_itself
        {
            entry.closed = true;
            registry.dirty = true;
        }
    });
}

/// The secondary windows that should be open, by key with their options: every one the
/// composition asks for except those that closed themselves.
pub(crate) fn wanted_windows() -> Vec<(u64, WindowOptions)> {
    REGISTRY.with(|r| {
        r.borrow()
            .entries
            .iter()
            .filter(|(key, entry)| **key != MAIN && !entry.closed)
            .map(|(key, entry)| (*key, entry.options.clone()))
            .collect()
    })
}

/// The index in `windows` of a modal window that blocks input to the window at `index`.
///
/// `windows` lists every slot as its registry key and options, or `None` where the window is
/// not open. A modal window with a parent blocks that parent; one without blocks every window
/// except itself and the windows that descend from it, which are its own dialogs.
pub(crate) fn modal_over(windows: &[Option<(u64, &WindowOptions)>], index: usize) -> Option<usize> {
    let (key, _) = windows.get(index).copied().flatten()?;
    let parent_of = |key: u64| {
        windows
            .iter()
            .flatten()
            .find(|(candidate, _)| *candidate == key)
            .and_then(|(_, options)| options.parent)
            .map(|parent| parent.key)
    };
    let descends_from = |mut key: u64, ancestor: u64| {
        for _ in 0..windows.len() {
            match parent_of(key) {
                Some(parent) if parent == ancestor => return true,
                Some(parent) => key = parent,
                None => return false,
            }
        }
        false
    };
    (1..windows.len()).find(|&other| {
        let Some((modal_key, options)) = windows[other] else {
            return false;
        };
        other != index
            && options.modal
            && match options.parent {
                Some(parent) => parent.key == key,
                None => !descends_from(key, modal_key),
            }
    })
}

/// Whether `event` is the user working with a window's content, which a modal window
/// elsewhere blocks.
fn is_input(event: &winit::event::WindowEvent) -> bool {
    use winit::event::WindowEvent as E;
    matches!(
        event,
        E::PointerButton { .. } | E::MouseWheel { .. } | E::KeyboardInput { .. } | E::Ime(_)
    )
}

/// Turns Rustle's cursor icon into winit's.
fn winit_cursor(icon: rustle_ui::CursorIcon) -> winit::cursor::CursorIcon {
    use winit::cursor::CursorIcon as W;
    match icon {
        rustle_ui::CursorIcon::Default => W::Default,
        rustle_ui::CursorIcon::Pointer => W::Pointer,
        rustle_ui::CursorIcon::Text => W::Text,
        rustle_ui::CursorIcon::Crosshair => W::Crosshair,
        rustle_ui::CursorIcon::Grab => W::Grab,
        rustle_ui::CursorIcon::Grabbing => W::Grabbing,
        rustle_ui::CursorIcon::NotAllowed => W::NotAllowed,
        rustle_ui::CursorIcon::Wait => W::Wait,
        rustle_ui::CursorIcon::Move => W::Move,
        rustle_ui::CursorIcon::EwResize => W::EwResize,
        rustle_ui::CursorIcon::NsResize => W::NsResize,
        rustle_ui::CursorIcon::NeswResize => W::NeswResize,
        rustle_ui::CursorIcon::NwseResize => W::NwseResize,
        rustle_ui::CursorIcon::ColResize => W::ColResize,
        rustle_ui::CursorIcon::RowResize => W::RowResize,
    }
}

thread_local! {
    /// The one wgpu instance every window draws through.
    static INSTANCE: std::cell::OnceCell<wgpu::Instance> = const { std::cell::OnceCell::new() };
}

/// The wgpu instance shared by all windows, made on first use.
///
/// Each window used to make its own. Dropping one window's instance while another's was in use
/// crashed some Vulkan drivers (NVIDIA on Wayland) inside the next frame's swapchain acquire.
fn shared_instance(event_loop: &dyn ActiveEventLoop) -> wgpu::Instance {
    INSTANCE.with(|cell| {
        cell.get_or_init(|| {
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(Box::new(
                event_loop.owned_display_handle(),
            )))
        })
        .clone()
    })
}

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
use winit::window::{Theme as WindowTheme, Window, WindowId};

use crate::app::{App, WindowState};
use crate::interface::Interface;
use crate::window::{Entry, MAIN, REGISTRY, WindowOptions};

/// The state of one window while another is being served.
#[derive(Default)]
pub(crate) struct WindowCtx {
    pub(crate) key: u64,
    pub(crate) ui: Interface,
    pub(crate) options: WindowOptions,
    pub(crate) window: Option<WindowState>,
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

    /// The window in slot `index`, if it is open.
    fn slot_window(&self, index: usize) -> Option<&WindowState> {
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
    pub(crate) fn main_window(&self) -> Option<&Arc<Window>> {
        self.slot_window(0).map(|state| &state.window)
    }

    /// Every window that is open.
    pub(crate) fn native_windows(&self) -> impl Iterator<Item = &WindowState> {
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

    /// Creates a native window with its renderer and accessibility adapter.
    pub(crate) fn open_native(
        &self,
        event_loop: &ActiveEventLoop,
        options: &WindowOptions,
    ) -> Result<WindowState, String> {
        let window = Arc::new(
            event_loop
                .create_window(options.attributes())
                .map_err(|error| error.to_string())?,
        );
        let instance = shared_instance(event_loop);
        let make_adapter = self
            .make_adapter
            .as_ref()
            .ok_or("this application has no native windows")?;
        let adapter = make_adapter(event_loop, &window);
        if options.visible {
            window.set_visible(true);
        }
        let size = window.inner_size();
        let renderer = Renderer::new(instance, window.clone(), (size.width, size.height))
            .map_err(|error| error.to_string())?;
        Ok(WindowState {
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
                },
            );
        });
    }

    /// Does what composition and the window handles asked for: opens windows that appeared,
    /// closes ones that left, applies changed options, forgets presses the system took over
    /// and runs close requests.
    pub(crate) fn sync_windows(&mut self, event_loop: &ActiveEventLoop) {
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
                event_loop.exit();
                continue;
            }
            let on_close = REGISTRY.with(|r| r.borrow().entries.get(&key)?.on_close.clone());
            if let Some(on_close) = on_close {
                on_close();
            }
        }
        if !REGISTRY.with(|r| mem::take(&mut r.borrow_mut().dirty)) {
            return;
        }
        let wanted: Vec<(u64, WindowOptions)> = REGISTRY.with(|r| {
            r.borrow()
                .entries
                .iter()
                .filter(|(key, _)| **key != MAIN)
                .map(|(key, entry)| (*key, entry.options.clone()))
                .collect()
        });
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
                            != Some(WindowTheme::Light),
                    );
                    self.relayout();
                    self.activate(0);
                }
                Err(error) => return self.fail(event_loop, error),
            }
        }
    }

    /// Applies the options of an open secondary window that changed.
    fn apply_options(&mut self, index: usize, options: WindowOptions) {
        let ctx = &mut self.slots[index];
        if ctx.options == options {
            return;
        }
        if let Some(state) = &ctx.window {
            let window: &Window = &state.window;
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
                window.set_min_inner_size(Some(winit::dpi::LogicalSize::new(
                    options.min_size.0,
                    options.min_size.1,
                )));
            }
            if !options.ime_allowed {
                window.set_ime_allowed(false);
            }
        }
        ctx.options = options;
    }

    /// Handles an event of the window `id`, whichever it is.
    pub(crate) fn window_event_for(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(index) = self.slot_of(id) else {
            return;
        };
        self.activate(index);
        if self.key != MAIN && matches!(event, winit::event::WindowEvent::CloseRequested) {
            // The composition decides whether the window goes.
            REGISTRY.with(|r| r.borrow_mut().close_requests.push(self.key));
        } else {
            self.window_event(event_loop, event);
        }
        self.sync_windows(event_loop);
    }

    /// Answers a request of the platform's accessibility API for the window `id`.
    pub(crate) fn accessibility_event_for(
        &mut self,
        id: WindowId,
        event: accesskit_winit::WindowEvent,
    ) {
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
            state.window.set_cursor(winit_cursor(self.ui.cursor()));
        }
    }

    /// Does what input asked of the window: redraws, shows the cursor, and allows or forbids
    /// the input method as focus moves.
    pub(crate) fn apply_effects(&mut self) {
        if self.ui.take_focus_changed() {
            // An input method only gets keys while something wants text, so it
            // cannot take keys from the rest of the application.
            self.ui.ime_area = None;
            let allowed = self.ui.focused.is_some() && self.ime_allowed();
            if let Some(state) = &self.window {
                state.window.set_ime_allowed(allowed);
            }
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
            state.window.set_cursor(winit_cursor(self.ui.cursor()));
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
            state.window.set_ime_cursor_area(
                winit::dpi::LogicalPosition::new(area.origin.x as f64, area.origin.y as f64)
                    .to_physical::<f64>(scale),
                winit::dpi::LogicalSize::new(area.size.width as f64, area.size.height as f64)
                    .to_physical::<f64>(scale),
            );
        }
    }
}

/// Turns Rustle's cursor icon into winit's.
fn winit_cursor(icon: rustle_ui::CursorIcon) -> winit::window::CursorIcon {
    use winit::window::CursorIcon as W;
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
fn shared_instance(event_loop: &ActiveEventLoop) -> wgpu::Instance {
    INSTANCE.with(|cell| {
        cell.get_or_init(|| {
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(Box::new(
                event_loop.owned_display_handle(),
            )))
        })
        .clone()
    })
}

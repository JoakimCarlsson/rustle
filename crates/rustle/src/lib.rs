//! Rustle: a tiny declarative UI framework.

use std::process;
use std::rc::Rc;
use std::sync::Arc;

use rustle_geometry::{Offset, Size};
use rustle_ui::{Composition, DisplayList};
use rustle_unit::Constraints;
use rustle_wgpu::Renderer;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{CursorIcon, Window, WindowId};

pub use rustle_foundation::{
    Modifier, column, column_with, row, row_with, spacer, stack, stack_with, text, text_with,
};
pub use rustle_graphics::Color;
pub use rustle_macros::composable;
pub use rustle_runtime::{State, remember};
pub use rustle_unit::{Dp, dp};
pub use rustle_widgets::{button, button_with};

pub mod prelude {
    pub use crate::{
        Color, Dp, Modifier, State, button, button_with, column, column_with, composable, dp,
        remember, row, row_with, spacer, stack, stack_with, text, text_with,
    };
}

/// Composes `root` and shows it in a native window until closed.
///
/// The whole root is composed again, on the event loop, whenever state it
/// remembers has changed.
///
/// Exits the process with status 1 if the window or renderer cannot be set up.
pub fn run(root: impl FnMut() + 'static) {
    let mut app = App {
        composition: rustle_ui::composition(root),
        display: DisplayList::default(),
        pointer: None,
        pressed: None,
        window: None,
        error: None,
    };

    let result = EventLoop::new().and_then(|event_loop| {
        // State changes wake the event loop instead of being polled for. A
        // new composition is dirty, so the first wake-up also composes it.
        let proxy = event_loop.create_proxy();
        let wake = proxy.clone();
        app.composition.on_invalidate(move || {
            let _ = wake.send_event(());
        });
        let _ = proxy.send_event(());
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

struct App {
    composition: Composition,
    display: DisplayList,
    /// Where the pointer is, in logical pixels, while it is over the window.
    pointer: Option<Offset>,
    /// The handler of the region the primary button went down on.
    pressed: Option<Rc<dyn Fn()>>,
    window: Option<WindowState>,
    error: Option<String>,
}

struct WindowState {
    renderer: Renderer,
    window: Arc<Window>,
}

impl App {
    /// Lays the latest tree out for the current window and asks for a redraw.
    ///
    /// Text is measured by the renderer, so this waits until the window exists
    /// and the first pass has produced a tree.
    fn relayout(&mut self) {
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
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl ToString) {
        self.error = Some(error.to_string());
        event_loop.exit();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Rustle")
            .with_inner_size(LogicalSize::new(1920.0, 1080.0));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => return self.fail(event_loop, error),
        };

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(event_loop.owned_display_handle()),
        ));
        let size = window.inner_size();
        match Renderer::new(instance, window.clone(), (size.width, size.height)) {
            Ok(renderer) => {
                self.window = Some(WindowState { renderer, window });
                self.relayout();
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, _: ()) {
        if self.composition.recompose() {
            self.relayout();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(state) = &mut self.window else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                state.renderer.resize(size.width, size.height);
                self.relayout();
            }
            WindowEvent::ScaleFactorChanged { .. } => self.relayout(),
            WindowEvent::CursorMoved { position, .. } => {
                let scale_factor = state.window.scale_factor();
                let pointer = Offset::new(
                    (position.x / scale_factor) as f32,
                    (position.y / scale_factor) as f32,
                );
                let clickable = self.display.hit_test(pointer).is_some();
                state.window.set_cursor(if clickable {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                });
                self.pointer = Some(pointer);
            }
            WindowEvent::CursorLeft { .. } => self.pointer = None,
            WindowEvent::MouseInput {
                state: button_state,
                button: MouseButton::Left,
                ..
            } => {
                let target = self
                    .pointer
                    .and_then(|pointer| self.display.hit_test(pointer))
                    .map(|region| Rc::clone(&region.on_click));
                match button_state {
                    ElementState::Pressed => self.pressed = target,
                    // A click is a press and a release on the same region.
                    ElementState::Released => {
                        let pressed = self.pressed.take();
                        if let (Some(pressed), Some(target)) = (pressed, target)
                            && Rc::ptr_eq(&pressed, &target)
                        {
                            target();
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let scale_factor = state.window.scale_factor() as f32;
                match state.renderer.render(&self.display.commands, scale_factor) {
                    Ok(true) => {}
                    Ok(false) => state.window.request_redraw(),
                    Err(error) => self.fail(event_loop, error),
                }
            }
            _ => {}
        }
    }
}

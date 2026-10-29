//! Rustle: a tiny Compose-inspired declarative UI framework.

use std::process;
use std::sync::Arc;

use rustle_core::{Composition, DrawCommand};
use rustle_wgpu::Renderer;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

pub use rustle_core::{State, column, remember, text};
pub use rustle_macros::composable;

pub mod prelude {
    pub use crate::{State, column, composable, remember, text};
}

/// Composes `root` and shows it in a native window until closed.
///
/// The whole root is composed again, on the event loop, whenever state it
/// remembers has changed.
///
/// Exits the process with status 1 if the window or renderer cannot be set up.
pub fn run(root: impl FnMut() + 'static) {
    let mut app = App {
        composition: Composition::new(root),
        commands: Vec::new(),
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
    commands: Vec<DrawCommand>,
    window: Option<WindowState>,
    error: Option<String>,
}

struct WindowState {
    renderer: Renderer,
    window: Arc<Window>,
}

impl App {
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
                window.request_redraw();
                self.window = Some(WindowState { renderer, window });
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, _: ()) {
        if self.composition.recompose() {
            self.commands = rustle_core::layout(self.composition.tree());
            if let Some(state) = &self.window {
                state.window.request_redraw();
            }
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
                state.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => state.window.request_redraw(),
            WindowEvent::RedrawRequested => {
                let scale_factor = state.window.scale_factor() as f32;
                match state.renderer.render(&self.commands, scale_factor) {
                    Ok(true) => {}
                    Ok(false) => state.window.request_redraw(),
                    Err(error) => self.fail(event_loop, error),
                }
            }
            _ => {}
        }
    }
}

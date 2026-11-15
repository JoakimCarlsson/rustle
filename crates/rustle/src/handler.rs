//! The window's event handler: creating the window, and reacting to what happens to it.

use std::rc::Rc;
use std::time::Instant;

use rustle_geometry::Offset;
use rustle_ui::{PointerId, PointerInputPhase, PointerKind};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, Ime, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{Theme as WindowTheme, WindowId};

use crate::app::{App, UserEvent};
use crate::input;

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.resumed(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.about_to_wait(event_loop);
    }

    fn user_event(&mut self, _: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Wake => {}
            UserEvent::External => crate::app::run_external_handler(),
            UserEvent::Accessibility(event) => {
                self.accessibility_event_for(event.window_id, event.window_event)
            }
        }
        self.woken();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.window_event_for(event_loop, id, event);
    }
}

impl App {
    /// Opens the window and its renderer, unless they are open already.
    pub(crate) fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let options = self.options.clone();
        let state = match self.open_native(event_loop, &options) {
            Ok(state) => state,
            Err(error) => return self.fail(event_loop, error),
        };
        self.set_dark(state.window.theme() != Some(WindowTheme::Light));
        rustle_ui::set_clipboard(Rc::new(input::SystemClipboard::new()));
        self.window = Some(state);
        self.register_main();
        self.relayout();
        self.sync_windows(event_loop);
    }

    /// Waits for the next long press that is due, or for the next event.
    pub(crate) fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.sync_windows(event_loop);
        let Some((due, _, _)) = &self.ui.long_press else {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        };
        if Instant::now() >= *due {
            if let Some((_, on_long_press, _)) = self.ui.long_press.take() {
                // The press has become a long press, so it is no longer a click.
                self.ui.pressed = None;
                on_long_press();
            }
            event_loop.set_control_flow(ControlFlow::Wait);
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(*due));
        }
    }

    /// Composes and lays out again what state changes dirtied, and keeps frames coming.
    pub(crate) fn woken(&mut self) {
        if self.composition.recompose() {
            self.relayout_all();
        }
        self.ui.apply_focus_request();
        self.apply_effects();
        self.request_redraw_for_frames();
    }

    /// Reacts to what happened to the window.
    pub(crate) fn window_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        let started = Instant::now();
        if matches!(event, WindowEvent::RedrawRequested) {
            self.run_frame();
        }
        let Some(state) = &mut self.window else {
            return;
        };
        if self.ui.debug_input
            && !matches!(
                event,
                WindowEvent::RedrawRequested | WindowEvent::CursorMoved { .. }
            )
        {
            eprintln!("rustle input: window event {event:?}");
        }
        state.adapter.process_event(&state.window, &event);
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
                self.ui.pointer_moved(pointer);
                self.show_cursor();
            }
            WindowEvent::Touch(touch) => {
                let scale_factor = state.window.scale_factor();
                let at = Offset::new(
                    (touch.location.x / scale_factor) as f32,
                    (touch.location.y / scale_factor) as f32,
                );
                let phase = match touch.phase {
                    TouchPhase::Started => PointerInputPhase::Down,
                    TouchPhase::Moved => PointerInputPhase::Move,
                    TouchPhase::Ended => PointerInputPhase::Up,
                    TouchPhase::Cancelled => PointerInputPhase::Cancel,
                };
                let pressure = touch
                    .force
                    .map_or(0.5, |force| force.normalized().clamp(0.0, 1.0) as f32);
                self.ui
                    .contact(PointerId(touch.id), PointerKind::Touch, phase, at, pressure);
                self.show_cursor();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scale_factor = state.window.scale_factor() as f32;
                // A line of a mouse wheel scrolls this many logical pixels.
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * 48.0, y * 48.0),
                    MouseScrollDelta::PixelDelta(delta) => {
                        (delta.x as f32 / scale_factor, delta.y as f32 / scale_factor)
                    }
                };
                self.ui.wheel(Offset::new(dx, dy));
            }
            WindowEvent::CursorLeft { .. } => self.ui.pointer_left(),
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => self.ui.button_input(
                input::pointer_button(button),
                button_state == ElementState::Pressed,
            ),
            WindowEvent::DroppedFile(path) => self.ui.file_dropped(path),
            WindowEvent::PinchGesture { delta, .. } => self.ui.pinch(delta as f32),
            WindowEvent::ThemeChanged(theme) => self.set_dark(theme == WindowTheme::Dark),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.ui.modifiers = input::modifiers(modifiers.state());
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let event = input::key_event(&event, self.ui.modifiers);
                if self.ui.debug_input {
                    eprintln!("rustle input: key {event:?}");
                }
                self.ui.keyboard(event);
            }
            WindowEvent::Ime(Ime::Preedit(text, cursor)) => self.ui.preedit(text, cursor),
            WindowEvent::Ime(Ime::Enabled | Ime::Disabled) => {
                self.ui.preedit(String::new(), None);
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.ui.commit(text);
            }
            WindowEvent::RedrawRequested => {
                crate::shadow::fit(&state.window, self.options.shadow.as_ref());
                let scale_factor = state.window.scale_factor() as f32;
                let commands = self.ui.commands();
                let built = Instant::now();
                match state.renderer.render(&commands, scale_factor) {
                    Ok(true) => crate::frame_timing::observe(crate::FrameTiming {
                        scene: built - started,
                        render: built.elapsed(),
                    }),
                    Ok(false) => state.window.request_redraw(),
                    Err(error) => return self.fail(event_loop, error),
                }
                self.request_redraw_for_frames();
            }
            _ => {}
        }
        self.ui.apply_focus_request();
        self.apply_effects();
    }
}

//! The window's event handler: creating the window, and reacting to what happens to it.

use std::borrow::Cow;
use std::rc::Rc;
use std::time::Instant;

use rustle_foundation::KeyEvent;
use rustle_geometry::Offset;
use rustle_ui::PointerButton;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, Ime, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{Theme as WindowTheme, WindowId};

use crate::app::{App, UserEvent};
use crate::input;
use crate::overlay::{carrying_overlay, feedback};

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
        let Some((due, _, _)) = &self.long_press else {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        };
        if Instant::now() >= *due {
            if let Some((_, on_long_press, _)) = self.long_press.take() {
                // The press has become a long press, so it is no longer a click.
                self.pressed = None;
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
        self.apply_focus_request();
        self.request_redraw_for_frames();
    }

    /// Reacts to what happened to the window.
    pub(crate) fn window_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        if matches!(event, WindowEvent::RedrawRequested) {
            self.run_frame();
        }
        let Some(state) = &mut self.window else {
            return;
        };
        if self.debug_input
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
                self.pointer_moved(pointer);
            }
            WindowEvent::Touch(touch) => {
                let scale_factor = state.window.scale_factor();
                let at = Offset::new(
                    (touch.location.x / scale_factor) as f32,
                    (touch.location.y / scale_factor) as f32,
                );
                // A finger is a pointer that is only there while it touches.
                match touch.phase {
                    TouchPhase::Started => {
                        self.pointer_moved(at);
                        self.button_input(PointerButton::Primary, true);
                    }
                    TouchPhase::Moved => self.pointer_moved(at),
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        self.pointer_moved(at);
                        self.button_input(PointerButton::Primary, false);
                        self.pointer = None;
                    }
                }
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
                if let Some(pointer) = self.pointer {
                    for (vertical, amount) in [(true, dy), (false, dx)] {
                        if amount != 0.0
                            && let Some(region) = self.display.scroll_at(pointer, vertical)
                        {
                            region.state.scroll_by(-amount);
                        }
                    }
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer = None;
                state.window.request_redraw();
                self.update_following();
                self.update_hover();
            }
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => self.button_input(
                input::pointer_button(button),
                button_state == ElementState::Pressed,
            ),
            WindowEvent::DroppedFile(path) => {
                // The platform gives no position, so the file goes where the pointer last was.
                if let Some(pointer) = self.pointer
                    && let Some(region) = self.display.file_drop_at(pointer)
                {
                    let on_drop = Rc::clone(&region.on_drop);
                    on_drop(path);
                }
            }
            WindowEvent::PinchGesture { delta, .. } => {
                if let Some(pointer) = self.pointer
                    && let Some(region) = self.display.pinch_at(pointer)
                {
                    let on_pinch = Rc::clone(&region.on_pinch);
                    on_pinch(delta as f32);
                }
            }
            WindowEvent::ThemeChanged(theme) => self.set_dark(theme == WindowTheme::Dark),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = input::modifiers(modifiers.state());
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let event = input::key_event(&event, self.modifiers);
                if self.debug_input {
                    eprintln!("rustle input: key {event:?}");
                }
                if event.pressed
                    && let Some(text) = &event.text
                {
                    self.typed = Some((Instant::now(), text.clone()));
                }
                self.key_event(event);
            }
            WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
                if self.debug_input {
                    eprintln!("rustle input: input method composing {text:?} {cursor:?}");
                }
                self.set_preedit(text, cursor);
            }
            WindowEvent::Ime(Ime::Enabled | Ime::Disabled) => {
                self.set_preedit(String::new(), None);
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                // The composition ends with the commit.
                self.set_preedit(String::new(), None);
                if self.debug_input {
                    eprintln!("rustle input: input method committed {text:?}");
                }
                // Some input methods deliver text a key press already typed.
                let repeated = self.typed.as_ref().is_some_and(|(when, typed)| {
                    *typed == text && when.elapsed() < std::time::Duration::from_millis(60)
                });
                if !repeated {
                    self.key_event(KeyEvent::typed(text, self.modifiers));
                }
            }
            WindowEvent::RedrawRequested => {
                let scale_factor = state.window.scale_factor() as f32;
                let carried = carrying_overlay(&self.display, self.carrying.as_ref(), self.pointer);
                let feedback = if carried.is_some() {
                    None
                } else {
                    feedback(&self.display, self.pointer, &self.pressed)
                };
                let commands = match (feedback, carried) {
                    (None, None) => Cow::Borrowed(self.display.commands.as_slice()),
                    (feedback, carried) => {
                        let mut commands = self.display.commands.clone();
                        commands.extend(feedback);
                        commands.extend(carried.into_iter().flatten());
                        Cow::Owned(commands)
                    }
                };
                match state.renderer.render(&commands, scale_factor) {
                    Ok(true) => {}
                    Ok(false) => state.window.request_redraw(),
                    Err(error) => return self.fail(event_loop, error),
                }
                self.request_redraw_for_frames();
            }
            _ => {}
        }
        self.apply_focus_request();
    }
}

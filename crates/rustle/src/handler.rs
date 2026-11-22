//! The window's event handler: creating the window, and reacting to what happens to it.

use std::process;
use std::time::Instant;

use rustle_geometry::Offset;
use rustle_ui::PointerInputPhase::{Cancel, Move};
use rustle_ui::{PointerId, PointerInputPhase, PointerKind};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{
    ButtonSource, ElementState, FingerId, Force, Ime, MouseScrollDelta, PointerKind as WinitKind,
    PointerSource, TabletToolButton, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{Theme as WindowTheme, WindowId};

use crate::app::App;
use crate::input;

/// The application [`run`](crate::run) hands to the event loop it owns.
pub(crate) struct Running(pub(crate) App);

impl ApplicationHandler for Running {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.0.can_create_surfaces(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.0.about_to_wait(event_loop);
    }

    fn proxy_wake_up(&mut self, _: &dyn ActiveEventLoop) {
        crate::app::run_external_handler();
        self.0.woken();
    }

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.0.window_event_for(event_loop, id, event);
    }
}

impl Drop for Running {
    /// Reports why the event loop stopped, if it stopped because the window or the renderer
    /// failed, and exits the process with status 1 then.
    fn drop(&mut self) {
        if let Some(error) = self.0.error.take() {
            eprintln!("rustle: {error}");
            process::exit(1);
        }
    }
}

impl App {
    /// Opens the window and its renderer, unless they are open already.
    pub(crate) fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let options = self.options.clone();
        let state = match self.open_native(event_loop, &options) {
            Ok(state) => state,
            Err(error) => return self.fail(event_loop, error),
        };
        self.set_dark(state.window.theme() != Some(WindowTheme::Light));
        input::install_system_clipboard();
        self.window = Some(state);
        self.register_main();
        self.relayout();
        self.sync_windows(event_loop);
    }

    /// Resumes the tasks whose delays ended and waits for the next long press or delay that
    /// is due, or for the next event.
    pub(crate) fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.sync_windows(event_loop);
        if self.composition.run_timers(self.started.elapsed()) {
            self.woken();
        }
        let mut due = self.composition.next_timer().map(|at| self.started + at);
        if let Some((pressed_until, ..)) = &self.ui.long_press {
            if Instant::now() >= *pressed_until {
                if let Some((_, on_long_press, ..)) = self.ui.long_press.take() {
                    // The press has become a long press, so it is no longer a click.
                    self.ui.pressed = None;
                    on_long_press();
                }
            } else {
                due = Some(due.map_or(*pressed_until, |at| at.min(*pressed_until)));
            }
        }
        event_loop.set_control_flow(match due {
            Some(at) => ControlFlow::WaitUntil(at),
            None => ControlFlow::Wait,
        });
    }

    /// Answers what the platform's accessibility API asked, composes and lays out again what
    /// state changes dirtied, and keeps frames coming.
    pub(crate) fn woken(&mut self) {
        while let Ok((window, request)) = self.requested.try_recv() {
            self.accessibility_event_for(window, request);
        }
        if self.composition.recompose() {
            self.relayout_all();
        }
        self.follow_phases();
        self.apply_focus_request();
        self.apply_effects();
        self.request_redraw_for_frames();
    }

    /// Reacts to what happened to the window.
    pub(crate) fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, event: WindowEvent) {
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
                WindowEvent::RedrawRequested | WindowEvent::PointerMoved { .. }
            )
        {
            eprintln!("rustle input: window event {event:?}");
        }
        state.adapter.process_event(&*state.window, &event);
        match event {
            WindowEvent::CloseRequested => {
                if crate::app::main_may_close() {
                    event_loop.exit();
                }
            }
            WindowEvent::SurfaceResized(size) => {
                state.renderer.resize(size.width, size.height);
                self.relayout();
                self.observe_window_state();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.relayout();
                self.observe_window_state();
            }
            WindowEvent::Moved(_) | WindowEvent::Occluded(_) => self.observe_window_state(),
            WindowEvent::Focused(focused) => {
                if let Some(info) = crate::window::window_info(self.key) {
                    info.set_focused(focused);
                }
                if !focused {
                    self.ui.release_all_buttons();
                }
                self.observe_window_state();
            }
            WindowEvent::PointerEntered {
                position,
                kind: WinitKind::Mouse | WinitKind::TabletTool(_) | WinitKind::Unknown,
                ..
            } => {
                let pointer = logical(position, state.window.scale_factor());
                self.ui.pointer_moved(pointer);
                self.show_cursor();
            }
            WindowEvent::PointerMoved {
                position, source, ..
            } => {
                let at = logical(position, state.window.scale_factor());
                match source {
                    PointerSource::Touch { finger_id, force } => {
                        self.contact(finger(finger_id), PointerKind::Touch, Move, at, force);
                    }
                    PointerSource::TabletTool { data, .. }
                        if self.ui.primary_contact == Some(PEN) =>
                    {
                        self.contact(PEN, PointerKind::Pen, Move, at, data.force);
                    }
                    _ => self.ui.pointer_moved(at),
                }
                self.show_cursor();
            }
            WindowEvent::PointerButton {
                state: button_state,
                position,
                button,
                ..
            } => {
                let at = logical(position, state.window.scale_factor());
                let phase = match button_state {
                    ElementState::Pressed => PointerInputPhase::Down,
                    ElementState::Released => PointerInputPhase::Up,
                };
                match button {
                    ButtonSource::Touch { finger_id, force } => {
                        self.contact(finger(finger_id), PointerKind::Touch, phase, at, force);
                        self.show_cursor();
                    }
                    ButtonSource::TabletTool {
                        button: TabletToolButton::Contact,
                        data,
                        ..
                    } => {
                        self.contact(PEN, PointerKind::Pen, phase, at, data.force);
                        self.show_cursor();
                    }
                    button => {
                        if let Some(button) = button.mouse_button() {
                            let pressed = button_state == ElementState::Pressed;
                            self.ui.button_input(input::pointer_button(button), pressed);
                        }
                    }
                }
            }
            WindowEvent::PointerLeft { position, kind, .. } => {
                let at = position.map_or_else(
                    || self.ui.pointer.unwrap_or_default(),
                    |position| logical(position, state.window.scale_factor()),
                );
                match kind {
                    WinitKind::Touch(finger_id) => {
                        self.contact(finger(finger_id), PointerKind::Touch, Cancel, at, None);
                    }
                    WinitKind::TabletTool(_) if self.ui.primary_contact == Some(PEN) => {
                        self.contact(PEN, PointerKind::Pen, Cancel, at, None);
                        self.ui.pointer_left();
                    }
                    _ => self.ui.pointer_left(),
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
                    _ => (0.0, 0.0),
                };
                self.ui.wheel(Offset::new(dx, dy));
            }
            WindowEvent::PinchGesture { delta, .. } => self.ui.pinch(delta as f32),
            WindowEvent::ThemeChanged(theme) => self.set_dark(theme == WindowTheme::Dark),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.ui.modifiers = input::modifiers(modifiers.state());
                if let Some(info) = crate::window::window_info(self.key) {
                    info.set_modifiers(self.ui.modifiers);
                }
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
            WindowEvent::Ime(_) => {}
            WindowEvent::RedrawRequested => {
                crate::shadow::fit(&*state.window, self.options.shadow.as_ref());
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
        self.apply_focus_request();
        self.apply_effects();
    }

    /// A finger or pen `id` of `kind` touched, moved, lifted or was taken away at `at`,
    /// pressing with `force`.
    fn contact(
        &mut self,
        id: PointerId,
        kind: PointerKind,
        phase: PointerInputPhase,
        at: Offset,
        force: Option<Force>,
    ) {
        let pressure = force.map_or(0.5, |force| force.normalized(None).clamp(0.0, 1.0) as f32);
        self.ui.contact(id, kind, phase, at, pressure);
    }
}

/// The contact a pen makes. Winit tells pens apart from fingers but not from each other, so
/// every pen is this one.
const PEN: PointerId = PointerId(u64::MAX - 1);

/// The contact `finger` makes.
fn finger(finger: FingerId) -> PointerId {
    PointerId(finger.into_raw() as u64)
}

/// `position`, in physical pixels, in logical pixels on a screen with `scale_factor`.
fn logical(position: PhysicalPosition<f64>, scale_factor: f64) -> Offset {
    Offset::new(
        (position.x / scale_factor) as f32,
        (position.y / scale_factor) as f32,
    )
}

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
        self.flush_pending_input(event_loop);
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

    /// Answers what the platform's accessibility API asked and keeps frames coming.
    ///
    /// What state changes dirtied is composed and laid out once, by the next frame: the
    /// windows are asked for a redraw and [`App::run_frame`] does the work then, however many
    /// wake-ups came before it. Without an open window there is no frame to wait for, so the
    /// work is done at once.
    pub(crate) fn woken(&mut self) {
        while let Ok((window, request)) = self.requested.try_recv() {
            self.accessibility_event_for(window, request);
        }
        if self.native_windows().next().is_none() {
            if self.composition.recompose() {
                self.relayout_all();
            }
            self.follow_phases();
        } else {
            let phases = self.composition.phase_observer();
            let relayout = phases.take_relayout();
            let redraw = phases.take_redraw();
            self.deferred_phases.0 |= relayout;
            self.deferred_phases.1 |= redraw;
            if self.composition.is_dirty() || relayout || redraw {
                for state in self.native_windows() {
                    state.request_frame();
                }
            }
        }
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
                self.defer_relayout();
                self.observe_window_state();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.defer_relayout();
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
                self.ui
                    .wheel(wheel_delta(delta, state.window.scale_factor()));
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
                let renderer = &mut state.renderer;
                let (built, rendered) = self.ui.with_commands(|commands| {
                    let built = Instant::now();
                    (built, renderer.render(commands, scale_factor))
                });
                match rendered {
                    Ok(true) => crate::frame_timing::observe(crate::FrameTiming {
                        scene: built - started,
                        render: built.elapsed(),
                    }),
                    Ok(false) => state.request_frame(),
                    Err(error) => return self.fail(event_loop, error),
                }
                rustle_ui::drop_retired();
                self.request_redraw_for_frames();
            }
            _ => {}
        }
        self.apply_focus_request();
        self.apply_effects();
    }

    /// Holds back `event` of the window `id`, the active one, if it is a mouse move or a
    /// scroll, to be handled with the ones like it that follow before the next frame: the
    /// latest move stands for the moves before it, and scrolls are summed. A move held back
    /// is handled before a scroll is held, and the other way round, so they keep their order.
    /// Returns whether `event` was held back.
    pub(crate) fn coalesce(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        id: WindowId,
        event: &WindowEvent,
    ) -> bool {
        if !coalescible(event) {
            return false;
        }
        let Some(scale_factor) = self
            .window
            .as_ref()
            .map(|state| state.window.scale_factor())
        else {
            return false;
        };
        if let Some(state) = &self.window {
            state.pacer.note_input();
        }
        if let WindowEvent::MouseWheel {
            delta: precise @ MouseScrollDelta::PixelDelta(_),
            ..
        } = event
        {
            self.resample_wheel(event_loop, id, wheel_delta(*precise, scale_factor));
            return true;
        }
        if let WindowEvent::MouseWheel { delta, .. } = event {
            let delta = wheel_delta(*delta, scale_factor);
            if self.pending_move.is_some()
                || self
                    .pending_wheel
                    .as_ref()
                    .is_some_and(|(window, _)| *window != id)
            {
                self.flush_pending_input(event_loop);
            }
            match &mut self.pending_wheel {
                Some((_, sum)) => *sum = Offset::new(sum.x + delta.x, sum.y + delta.y),
                None => self.pending_wheel = Some((id, delta)),
            }
        } else {
            if self.pending_wheel.is_some()
                || self
                    .pending_move
                    .as_ref()
                    .is_some_and(|(window, _)| *window != id)
            {
                self.flush_pending_input(event_loop);
            }
            self.pending_move = Some((id, event.clone()));
        }
        if let Some(state) = &self.window {
            state.request_frame();
        }
        true
    }

    /// Holds back a trackpad's scroll by `delta` in the window `id`, to be shown spread
    /// evenly over the frames that follow, as [`App::show_resampled_wheel`] hands it out.
    /// A mouse move held back is handled first, and the scrolls of another window are all
    /// handled at once.
    fn resample_wheel(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, delta: Offset) {
        if self.pending_move.is_some() || self.pending_wheel.is_some() {
            self.flush_pending_input(event_loop);
        }
        if self.resampled_wheel.1.is_some_and(|window| window != id) {
            self.drain_resampled_wheel();
        }
        self.resampled_wheel.0.push(Instant::now(), delta);
        self.resampled_wheel.1 = Some(id);
        if let Some(state) = &self.window {
            state.request_frame();
        }
    }

    /// Scrolls the window `id` by the part of a trackpad's scrolls that its frame about to
    /// be drawn shows, sampled at the refresh the frame is for, and asks for another frame
    /// while some are still to be shown.
    pub(crate) fn show_resampled_wheel(&mut self, id: WindowId) {
        if self.resampled_wheel.1 != Some(id) {
            return;
        }
        let Some(index) = self.slot_of(id) else {
            self.resampled_wheel = Default::default();
            return;
        };
        let frame = self
            .slot_window(index)
            .and_then(|state| state.pacer.frame_time())
            .unwrap_or_else(Instant::now);
        let delta = self.resampled_wheel.0.take(frame);
        self.scroll_window(index, delta);
        if self.resampled_wheel.0.pending() {
            if let Some(state) = self.slot_window(index) {
                state.request_frame();
            }
        } else {
            self.resampled_wheel.1 = None;
        }
    }

    /// Scrolls by every trackpad scroll not shown yet at once, before an event that must
    /// come after them.
    pub(crate) fn drain_resampled_wheel(&mut self) {
        let Some(id) = self.resampled_wheel.1.take() else {
            return;
        };
        let delta = self.resampled_wheel.0.drain();
        if let Some(index) = self.slot_of(id) {
            self.scroll_window(index, delta);
        }
    }

    /// Scrolls what is under the pointer in the window in slot `index` by `delta`, leaving
    /// the active window as it was.
    fn scroll_window(&mut self, index: usize, delta: Offset) {
        if delta.x == 0.0 && delta.y == 0.0 {
            return;
        }
        let before = self.active;
        self.activate(index);
        self.ui.wheel(delta);
        self.apply_focus_request();
        self.apply_effects();
        self.activate(before);
    }

    /// Handles the mouse move and the summed scroll held back by [`App::coalesce`], in the
    /// windows they happened in, leaving the active window as it was.
    pub(crate) fn flush_pending_input(&mut self, event_loop: &dyn ActiveEventLoop) {
        let before = self.active;
        if let Some((id, event)) = self.pending_move.take()
            && let Some(index) = self.slot_of(id)
        {
            self.activate(index);
            self.window_event(event_loop, event);
        }
        if let Some((id, delta)) = self.pending_wheel.take()
            && let Some(index) = self.slot_of(id)
        {
            self.activate(index);
            self.ui.wheel(delta);
            self.apply_focus_request();
            self.apply_effects();
        }
        self.activate(before);
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

/// Whether `event` is a mouse move or a scroll, which [`App::coalesce`] may hold back.
pub(crate) fn coalescible(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::PointerMoved {
            source: PointerSource::Mouse,
            ..
        } | WindowEvent::MouseWheel { .. }
    )
}

/// How far `delta` scrolls, in logical pixels, on a screen with `scale_factor`. A line of a
/// mouse wheel scrolls 48 logical pixels.
fn wheel_delta(delta: MouseScrollDelta, scale_factor: f64) -> Offset {
    let scale_factor = scale_factor as f32;
    match delta {
        MouseScrollDelta::LineDelta(x, y) => Offset::new(x * 48.0, y * 48.0),
        MouseScrollDelta::PixelDelta(delta) => {
            Offset::new(delta.x as f32 / scale_factor, delta.y as f32 / scale_factor)
        }
        _ => Offset::new(0.0, 0.0),
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

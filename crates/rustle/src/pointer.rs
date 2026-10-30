//! Pointer, touch and press handling: hover, drag, drop and clicks.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use rustle_geometry::{Offset, Rect};
use rustle_ui::{DisplayList, PointerButton, PointerEvent, PointerPhase, PressEvent};
use winit::window::CursorIcon;

use crate::app::{App, Carrying, FollowEntry, HoverEntry};

impl App {
    /// Tells the areas that follow the pointer where it is, or that it left.
    pub(crate) fn update_following(&mut self) {
        let mut now: Vec<(FollowEntry, Offset)> = Vec::new();
        if let Some(pointer) = self.pointer {
            for region in self.display.pointer_moves_at(pointer) {
                let local = Offset::new(pointer.x - region.origin.x, pointer.y - region.origin.y);
                now.push(((region.rect, Rc::clone(&region.on_move)), local));
            }
        }
        for (rect, on_move) in &self.following {
            if !now
                .iter()
                .any(|((current, _), _)| same_area(*current, *rect))
            {
                on_move(None);
            }
        }
        for ((_, on_move), local) in &now {
            on_move(Some(*local));
        }
        self.following = now.into_iter().map(|(entry, _)| entry).collect();
    }

    /// Tells the areas the pointer entered or left.
    pub(crate) fn update_hover(&mut self) {
        let now: Vec<HoverEntry> = match self.pointer {
            Some(pointer) => self
                .display
                .hovers_at(pointer)
                .map(|region| (region.rect, Rc::clone(&region.on_hover)))
                .collect(),
            None => Vec::new(),
        };
        for (rect, on_hover) in &self.hovered {
            if !now.iter().any(|(current, _)| same_area(*current, *rect)) {
                on_hover(false);
            }
        }
        for (rect, on_hover) in &now {
            if !self.hovered.iter().any(|(old, _)| same_area(*old, *rect)) {
                on_hover(true);
            }
        }
        self.hovered = now;
    }

    /// The pointer moved to `pointer`, in logical pixels.
    pub(crate) fn pointer_moved(&mut self, pointer: Offset) {
        let Some(window) = self.window.as_ref().map(|state| Arc::clone(&state.window)) else {
            return;
        };
        let before = hover_target(&self.display, self.pointer);
        window.set_cursor(self.cursor_icon_at(pointer));
        self.pointer = Some(pointer);
        self.update_following();
        if let Some(carrying) = &mut self.carrying
            && !carrying.active
            && ((carrying.began.x - pointer.x).abs() > 6.0
                || (carrying.began.y - pointer.y).abs() > 6.0)
        {
            carrying.active = true;
        }
        if self
            .carrying
            .as_ref()
            .is_some_and(|carrying| carrying.active)
        {
            window.set_cursor(CursorIcon::Grabbing);
            window.request_redraw();
        }
        // Moving away from where the press began is not a long press.
        if let Some((_, _, began)) = &self.long_press
            && ((began.x - pointer.x).abs() > 8.0 || (began.y - pointer.y).abs() > 8.0)
        {
            self.long_press = None;
        }
        self.update_hover();
        self.drag_buttons(pointer);
        if let Some((on_drag, origin, _)) = self.dragging.clone() {
            on_drag(Offset::new(pointer.x - origin.x, pointer.y - origin.y));
        }
        if before != hover_target(&self.display, self.pointer) {
            window.request_redraw();
        }
    }

    /// A pointer button went down or up where the pointer is: tells the areas that react to
    /// `button` (see [`Modifier::on_pointer_button`](rustle_ui::Modifier::on_pointer_button)),
    /// and for the primary button also does clicks, focus, drags and long presses.
    pub(crate) fn button_input(&mut self, button: PointerButton, pressed: bool) {
        let Some(pointer) = self.pointer else {
            if button == PointerButton::Primary {
                self.primary_button(pressed);
            }
            return;
        };
        if pressed {
            if button == PointerButton::Primary {
                self.primary_button(true);
            }
            let click_count = match (button, self.last_press) {
                (PointerButton::Primary, Some((_, _, count))) => count,
                _ => 1,
            };
            if let Some(region) = self.display.button_at(pointer, button) {
                let (rect, on_event) = (region.rect, Rc::clone(&region.on_event));
                let event = self.pointer_event(button, PointerPhase::Press, pointer, region.origin);
                self.held_buttons.retain(|held| held.button != button);
                self.held_buttons.push(HeldButton {
                    button,
                    rect,
                    click_count,
                });
                on_event(&PointerEvent {
                    click_count,
                    ..event
                });
            }
            return;
        }
        if let Some(index) = self
            .held_buttons
            .iter()
            .position(|held| held.button == button)
        {
            let held = self.held_buttons.remove(index);
            // The handler that runs is the current one for the area, not the one from the press.
            let current = self
                .display
                .buttons
                .iter()
                .rev()
                .find(|region| region.button == button && same_area(region.rect, held.rect))
                .map(|region| (Rc::clone(&region.on_event), region.origin));
            if let Some((on_event, origin)) = current {
                let event = |phase| PointerEvent {
                    click_count: held.click_count,
                    ..self.pointer_event(button, phase, pointer, origin)
                };
                on_event(&event(PointerPhase::Release));
                let over = self
                    .display
                    .button_at(pointer, button)
                    .is_some_and(|region| same_area(region.rect, held.rect));
                if over {
                    on_event(&event(PointerPhase::Click));
                }
            }
        }
        if button == PointerButton::Primary {
            self.primary_button(false);
        }
    }

    /// An event for `button` at `pointer` in an area whose top-left corner is `origin`.
    fn pointer_event(
        &self,
        button: PointerButton,
        phase: PointerPhase,
        pointer: Offset,
        origin: Offset,
    ) -> PointerEvent {
        PointerEvent {
            button,
            phase,
            position: Offset::new(pointer.x - origin.x, pointer.y - origin.y),
            window_position: pointer,
            modifiers: self.modifiers,
            click_count: 1,
        }
    }

    /// Tells the areas whose buttons are held that the pointer moved.
    fn drag_buttons(&self, pointer: Offset) {
        for held in &self.held_buttons {
            let current =
                self.display.buttons.iter().rev().find(|region| {
                    region.button == held.button && same_area(region.rect, held.rect)
                });
            if let Some(region) = current {
                let event = PointerEvent {
                    click_count: held.click_count,
                    ..self.pointer_event(held.button, PointerPhase::Drag, pointer, region.origin)
                };
                (Rc::clone(&region.on_event))(&event);
            }
        }
    }

    /// The primary button, or a finger, went down or up where the pointer is.
    pub(crate) fn primary_button(&mut self, pressed: bool) {
        if self.debug_input {
            let here = self
                .pointer
                .and_then(|pointer| self.display.hit_test(pointer));
            eprintln!(
                "rustle input: button {} at {:?}; clickable here: {}; clickable areas: {}, focusable: {}",
                if pressed { "down" } else { "up" },
                self.pointer,
                here.is_some(),
                self.display.regions.len(),
                self.display.focusables.len(),
            );
        }
        let target = self
            .pointer
            .and_then(|pointer| self.display.hit_test(pointer))
            .map(|region| region.rect);
        if let Some(state) = &self.window {
            state.window.request_redraw();
        }
        if !pressed {
            // Something carried over a drop target is dropped on it.
            let carrying = self.carrying.take();
            if let Some(carrying) = carrying
                && carrying.active
                && let Some(pointer) = self.pointer
                && let Some(region) = self.display.drop_target_at(pointer)
            {
                let on_drop = Rc::clone(&region.on_drop);
                on_drop(carrying.payload.as_ref());
                self.pressed = None;
            }
            // A click is a press and a release on the same region.
            if let Some((_, _, Some(on_end))) = self.dragging.take() {
                on_end();
            }
            self.long_press = None;
            // The handler that runs is the current one for the area, not the one
            // from when the button went down, which may be out of date.
            let pressed = self.pressed.take();
            if let (Some(pressed), Some(pointer)) = (pressed, self.pointer)
                && let Some(region) = self.display.hit_test(pointer)
                && same_area(pressed, region.rect)
            {
                let on_click = Rc::clone(&region.on_click);
                on_click();
            }
            return;
        }
        self.pressed = target;
        self.carrying = self.pointer.and_then(|pointer| {
            let region = self.display.drag_source_at(pointer)?;
            Some(Carrying {
                payload: Rc::clone(&region.payload),
                began: pointer,
                active: false,
                rect: region.rect,
                commands: region.commands.clone(),
            })
        });
        self.long_press = self.pointer.and_then(|pointer| {
            let region = self.display.long_press_at(pointer)?;
            Some((
                Instant::now() + std::time::Duration::from_millis(500),
                Rc::clone(&region.on_long_press),
                pointer,
            ))
        });
        let focus = self
            .pointer
            .and_then(|pointer| self.display.focus_at(pointer))
            .map(|region| region.focus.clone());
        self.set_focus(focus);
        let Some(pointer) = self.pointer else {
            return;
        };
        if let Some(region) = self.display.drag_at(pointer) {
            let (on_drag, origin) = (Rc::clone(&region.on_drag), region.origin);
            on_drag(Offset::new(pointer.x - origin.x, pointer.y - origin.y));
            self.dragging = Some((on_drag, origin, region.on_end.clone()));
        }
        let click_count = self.count_click(pointer);
        if let Some(region) = self.display.press_at(pointer) {
            let position = Offset::new(pointer.x - region.origin.x, pointer.y - region.origin.y);
            let on_press = Rc::clone(&region.on_press);
            on_press(PressEvent {
                position,
                click_count,
                button: PointerButton::Primary,
                modifiers: self.modifiers,
            });
        }
    }

    /// How many presses in a row this is: later ones close in time and place add up.
    pub(crate) fn count_click(&mut self, at: Offset) -> u32 {
        let now = Instant::now();
        let count = match self.last_press {
            Some((time, place, count))
                if now.duration_since(time) < std::time::Duration::from_millis(450)
                    && (place.x - at.x).abs() < 5.0
                    && (place.y - at.y).abs() < 5.0 =>
            {
                count % 3 + 1
            }
            _ => 1,
        };
        self.last_press = Some((now, at, count));
        count
    }
}

/// A pointer button that is down on an area that reacts to it.
pub(crate) struct HeldButton {
    pub(crate) button: PointerButton,
    /// The area it went down on, which is how its handler is found again after relayout.
    pub(crate) rect: Rect,
    pub(crate) click_count: u32,
}

/// Identifies the clickable region under `pointer`, to tell when it changes.
pub(crate) fn hover_target(display: &DisplayList, pointer: Option<Offset>) -> Option<Rect> {
    Some(display.hit_test(pointer?)?.rect)
}

/// Whether two areas are the same one, laid out again: where they are and how big.
pub(crate) fn same_area(a: Rect, b: Rect) -> bool {
    (a.origin.x - b.origin.x).abs() < 0.5
        && (a.origin.y - b.origin.y).abs() < 0.5
        && (a.size.width - b.size.width).abs() < 0.5
        && (a.size.height - b.size.height).abs() < 0.5
}

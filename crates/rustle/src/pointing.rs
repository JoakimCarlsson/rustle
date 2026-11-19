//! Pointer, touch and press handling: hover, drag, drop and clicks.

use std::rc::Rc;
use std::time::Instant;

use rustle_geometry::{Offset, Rect};
use rustle_ui::{
    DisplayList, PointerButton, PointerEvent, PointerId, PointerInputPhase, PointerKind,
    PointerPhase, PressEvent,
};

use crate::interface::{Carrying, FollowEntry, HoverEntry, Interface};

impl Interface {
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
        let before = hover_target(&self.display, self.pointer);
        self.pointer = Some(pointer);
        self.update_following();
        if let Some(carrying) = &mut self.carrying
            && !carrying.active
            && (carrying.began.x - pointer.x).hypot(carrying.began.y - pointer.y)
                > carrying.touch_slop
        {
            carrying.active = true;
        }
        if self
            .carrying
            .as_ref()
            .is_some_and(|carrying| carrying.active)
        {
            self.redraw = true;
        }
        // Moving away from where the press began is not a long press.
        if let Some((_, _, began, slop)) = &self.long_press
            && (began.x - pointer.x).hypot(began.y - pointer.y) > *slop
        {
            self.long_press = None;
        }
        self.update_hover();
        self.move_drag_and_drop(pointer);
        self.pointer_input_move(PointerId::MOUSE, pointer, MOUSE_PRESSURE);
        self.drag_buttons(pointer);
        if let Some((on_drag, origin, _)) = self.dragging.clone() {
            on_drag(Offset::new(pointer.x - origin.x, pointer.y - origin.y));
        }
        if before != hover_target(&self.display, self.pointer) {
            self.redraw = true;
        }
        self.pointer_event_moved();
    }

    /// A pointer button went down or up where the pointer is: tells the areas that react to
    /// `button` (see [`Modifier::on_pointer_button`](rustle_ui::Modifier::on_pointer_button)),
    /// and for the primary button also does clicks, focus, drags and long presses.
    pub(crate) fn button_input(&mut self, button: PointerButton, pressed: bool) {
        self.button_input_from(button, pressed, true);
    }

    /// Like [`button_input`](Self::button_input); `mouse` says whether the button belongs to
    /// the mouse, as opposed to a contact that stands in for the primary button.
    pub(crate) fn button_input_from(&mut self, button: PointerButton, pressed: bool, mouse: bool) {
        let held = if self.pointer.is_some() || !pressed {
            self.pointer_event_button(button, pressed)
        } else {
            None
        };
        self.built_in_button(button, pressed, mouse);
        self.finish_pointer_event(held);
    }

    /// The built-in handling of `button` going down or up where the pointer is: the areas
    /// that react to it and, for the primary button, clicks, focus, drags and long presses.
    /// `mouse` is as for [`button_input_from`](Self::button_input_from).
    fn built_in_button(&mut self, button: PointerButton, pressed: bool, mouse: bool) {
        let Some(pointer) = self.pointer else {
            if !pressed {
                self.release_without_pointer(button, mouse);
            }
            if button == PointerButton::Primary {
                self.primary_button(pressed);
            }
            return;
        };
        if pressed {
            if button == PointerButton::Primary && self.grab_scrollbar(pointer) {
                return;
            }
            for dismiss in self.display.popups_dismissed_by_press_at(pointer) {
                dismiss();
            }
            if button == PointerButton::Primary {
                self.primary_button(true);
                if mouse {
                    self.pointer_input_down(
                        PointerId::MOUSE,
                        PointerKind::Mouse,
                        pointer,
                        MOUSE_PRESSURE,
                    );
                }
            } else if self.carrying.is_none() {
                self.carrying = self.begin_carrying(button);
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
        if button == PointerButton::Primary && mouse {
            self.pointer_input_up(PointerId::MOUSE, pointer, PointerInputPhase::Up);
        }
        if button != PointerButton::Primary {
            self.finish_carrying(button);
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

    /// A button was released after the pointer left the area, so nothing says where: the
    /// raw pointer stream of the mouse ends where it last was, and what `button` carried is
    /// put down without being dropped.
    fn release_without_pointer(&mut self, button: PointerButton, mouse: bool) {
        if button == PointerButton::Primary && mouse {
            let last = self
                .pointers
                .iter()
                .find(|pointer| pointer.id == PointerId::MOUSE)
                .map(|pointer| pointer.position);
            if let Some(last) = last {
                self.pointer_input_up(PointerId::MOUSE, last, PointerInputPhase::Up);
            }
        }
        if button != PointerButton::Primary {
            self.finish_carrying(button);
            self.held_buttons.retain(|held| held.button != button);
        }
    }

    /// Picks up what the drag source under the pointer that listens to `button` carries.
    fn begin_carrying(&self, button: PointerButton) -> Option<Carrying> {
        let pointer = self.pointer?;
        let region = self.display.drag_source_at(pointer, button)?;
        Some(Carrying {
            payload: Rc::clone(&region.payload),
            button,
            began: pointer,
            active: false,
            rect: region.rect,
            commands: region.commands.clone(),
            touch_slop: region.touch_slop,
        })
    }

    /// Puts down what `button` was carrying, onto the drop target under the pointer if it was
    /// dragged. Returns whether it was dropped.
    fn finish_carrying(&mut self, button: PointerButton) -> bool {
        if self
            .carrying
            .as_ref()
            .is_none_or(|carrying| carrying.button != button)
        {
            return false;
        }
        let Some(carrying) = self.carrying.take() else {
            return false;
        };
        self.redraw = true;
        if carrying.active
            && let Some(pointer) = self.pointer
            && let Some(region) = self.display.drop_target_at(pointer)
        {
            let on_drop = Rc::clone(&region.on_drop);
            on_drop(carrying.payload.as_ref());
            return true;
        }
        false
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

    /// Starts dragging the scrollbar at `pointer`, if a press there belongs to one alone, as
    /// a scrollbar drawn over its content takes the pointer before the content in Compose;
    /// says whether it did. Nothing beneath is pressed, focused or clicked.
    fn grab_scrollbar(&mut self, pointer: Offset) -> bool {
        let Some(region) = self
            .display
            .drag_at(pointer)
            .filter(|region| region.takes_pointer)
        else {
            return false;
        };
        let (on_drag, origin) = (Rc::clone(&region.on_drag), region.origin);
        on_drag(Offset::new(pointer.x - origin.x, pointer.y - origin.y));
        self.dragging = Some((on_drag, origin, region.on_end.clone()));
        self.redraw = true;
        true
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
        self.redraw = true;
        if !pressed {
            self.pressed_at = None;
            // Something carried over a drop target is dropped on it.
            if self.finish_carrying(PointerButton::Primary) | self.finish_drag_and_drop() {
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
        self.pressed_at = self.pointer;
        self.carrying = self.begin_carrying(PointerButton::Primary);
        if let Some(pointer) = self.pointer {
            self.press_drag_and_drop(pointer);
        }
        self.long_press = self.pointer.and_then(|pointer| {
            let region = self.display.long_press_at(pointer)?;
            Some((
                Instant::now()
                    + std::time::Duration::from_millis(
                        region.view_configuration.long_press_timeout_millis,
                    ),
                Rc::clone(&region.on_long_press),
                pointer,
                region.view_configuration.touch_slop,
            ))
        });
        let focus = self
            .pointer
            .and_then(|pointer| self.display.focus_at(pointer))
            .filter(|region| {
                region.takes_press
                    || self
                        .focused
                        .as_ref()
                        .is_some_and(|held| held.id() == region.focus.id())
            })
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

    /// How many presses in a row this is: later ones add up when they come within the
    /// [`double_tap_timeout_millis`](rustle_ui::ViewConfiguration::double_tap_timeout_millis)
    /// and [`touch_slop`](rustle_ui::ViewConfiguration::touch_slop) of the one before, as the
    /// view configuration of the area pressed says, or the default where no area counts presses.
    pub(crate) fn count_click(&mut self, at: Offset) -> u32 {
        let now = Instant::now();
        let configuration = self
            .display
            .press_at(at)
            .map(|region| region.view_configuration)
            .unwrap_or_default();
        let timeout = std::time::Duration::from_millis(configuration.double_tap_timeout_millis);
        let count = match self.last_press {
            Some((time, place, count))
                if now.duration_since(time) < timeout
                    && (place.x - at.x).hypot(place.y - at.y) < configuration.touch_slop =>
            {
                count % 3 + 1
            }
            _ => 1,
        };
        self.last_press = Some((now, at, count));
        count
    }
}

/// The pressure reported for a mouse button, which cannot tell.
pub(crate) const MOUSE_PRESSURE: f32 = 0.5;

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

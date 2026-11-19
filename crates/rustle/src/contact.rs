//! Pointer contacts: several mouse, finger and pen pointers at once, and the raw event
//! stream of [`Modifier::pointer_input`](rustle_ui::Modifier::pointer_input).

use std::time::Instant;

use rustle_geometry::{Offset, Rect};
use rustle_ui::{
    PointerButton, PointerId, PointerInput, PointerInputPhase, PointerKind, PointerSample,
};

use crate::interface::Interface;
use crate::pointing::same_area;

/// A pointer that is down on an area that watches the raw stream.
pub(crate) struct ActivePointer {
    pub(crate) id: PointerId,
    pub(crate) kind: PointerKind,
    /// The area it went down on, which is how its handler is found again after relayout.
    pub(crate) rect: Rect,
    /// Where it is in the window.
    pub(crate) position: Offset,
    pub(crate) pressure: f32,
}

impl Interface {
    /// A contact (a finger, a pen, or any other pointer a host tracks itself) changed.
    ///
    /// The first contact to go down drives clicks, focus, drags and long presses as the
    /// primary button does, for as long as it is down; every contact is also shown to the
    /// areas made with [`Modifier::pointer_input`](rustle_ui::Modifier::pointer_input), with
    /// `pressure` from 0 to 1.
    pub(crate) fn contact(
        &mut self,
        id: PointerId,
        kind: PointerKind,
        phase: PointerInputPhase,
        at: Offset,
        pressure: f32,
    ) {
        let primary = self.primary_contact == Some(id);
        match phase {
            PointerInputPhase::Down => {
                if self.primary_contact.is_none() {
                    self.primary_contact = Some(id);
                    self.pointer_moved(at);
                    self.button_input_from(PointerButton::Primary, true, false);
                }
                self.pointer_input_down(id, kind, at, pressure);
            }
            PointerInputPhase::Move => {
                if primary {
                    self.pointer_moved(at);
                }
                self.pointer_input_move(id, at, pressure);
            }
            PointerInputPhase::Up | PointerInputPhase::Cancel => {
                if primary {
                    self.pointer_moved(at);
                }
                self.pointer_input_up(id, at, phase);
                if primary {
                    self.button_input_from(PointerButton::Primary, false, false);
                    self.pointer = None;
                    self.primary_contact = None;
                    self.update_hover();
                    self.update_following();
                }
            }
            PointerInputPhase::Scroll => {}
        }
    }

    /// A pointer went down at `at` (window position): the topmost watching area takes it.
    pub(crate) fn pointer_input_down(
        &mut self,
        id: PointerId,
        kind: PointerKind,
        at: Offset,
        pressure: f32,
    ) {
        self.pointers.retain(|pointer| pointer.id != id);
        let Some(region) = self.display.pointer_inputs_at(at).next() else {
            return;
        };
        self.pointers.push(ActivePointer {
            id,
            kind,
            rect: region.rect,
            position: at,
            pressure,
        });
        self.deliver(id, PointerInputPhase::Down);
    }

    /// A pointer that is down moved to `at`.
    pub(crate) fn pointer_input_move(&mut self, id: PointerId, at: Offset, pressure: f32) {
        let Some(pointer) = self.pointers.iter_mut().find(|pointer| pointer.id == id) else {
            return;
        };
        if pointer.position == at && pointer.pressure == pressure {
            return;
        }
        pointer.position = at;
        pointer.pressure = pressure;
        self.deliver(id, PointerInputPhase::Move);
    }

    /// A pointer that is down was lifted at `at`, or cancelled.
    pub(crate) fn pointer_input_up(&mut self, id: PointerId, at: Offset, phase: PointerInputPhase) {
        let Some(index) = self.pointers.iter().position(|pointer| pointer.id == id) else {
            return;
        };
        self.follow_area(self.pointers[index].rect);
        self.pointers[index].position = at;
        let pointer = self.pointers.remove(index);
        self.deliver_for(&pointer, phase, Offset::default(), false);
    }

    /// Offers a wheel turn of `delta` to the watching areas under the pointer, innermost first.
    /// Returns whether one used it.
    pub(crate) fn pointer_input_scroll(&mut self, delta: Offset) -> bool {
        let Some(at) = self.pointer else {
            return false;
        };
        let handlers: Vec<_> = self
            .display
            .pointer_inputs_at(at)
            .map(|region| (std::rc::Rc::clone(&region.on_event), region.origin))
            .collect();
        for (on_event, origin) in handlers {
            let event = PointerInput {
                id: PointerId::MOUSE,
                kind: PointerKind::Mouse,
                phase: PointerInputPhase::Scroll,
                button: PointerButton::Primary,
                position: Offset::new(at.x - origin.x, at.y - origin.y),
                window_position: at,
                pressure: 0.0,
                scroll_delta: delta,
                modifiers: self.modifiers,
                pointers: Vec::new(),
                time: Instant::now(),
            };
            if on_event(&event) {
                return true;
            }
        }
        false
    }

    /// Finds where the area the pointers went down on is now, when it moved or changed size
    /// since (a draggable that places itself with the offset it is dragged to, for one), and
    /// points every pointer that was on it to the new place. The area is the one of the same
    /// size nearest to where it was, and only when the interface still has one.
    fn follow_area(&mut self, old: Rect) {
        let exact = self
            .display
            .pointer_inputs
            .iter()
            .any(|region| same_area(region.rect, old));
        if exact {
            return;
        }
        let distance =
            |rect: Rect| (rect.origin.x - old.origin.x).hypot(rect.origin.y - old.origin.y);
        let moved = self
            .display
            .pointer_inputs
            .iter()
            .filter(|region| {
                (region.rect.size.width - old.size.width).abs() < 0.5
                    && (region.rect.size.height - old.size.height).abs() < 0.5
            })
            .min_by(|a, b| distance(a.rect).total_cmp(&distance(b.rect)))
            .map(|region| region.rect);
        if let Some(rect) = moved {
            for pointer in &mut self.pointers {
                if same_area(pointer.rect, old) {
                    pointer.rect = rect;
                }
            }
        }
    }

    /// Delivers `phase` for the pointer `id` that is down.
    fn deliver(&mut self, id: PointerId, phase: PointerInputPhase) {
        if let Some(rect) = self
            .pointers
            .iter()
            .find(|pointer| pointer.id == id)
            .map(|pointer| pointer.rect)
        {
            self.follow_area(rect);
        }
        if let Some(pointer) = self.pointers.iter().find(|pointer| pointer.id == id) {
            self.deliver_for(pointer, phase, Offset::default(), true);
        }
    }

    /// Delivers `phase` for `pointer` to the current handler of the area it went down on,
    /// with every pointer on that area. `listed` says whether `pointer` is among them.
    fn deliver_for(
        &self,
        pointer: &ActivePointer,
        phase: PointerInputPhase,
        scroll_delta: Offset,
        listed: bool,
    ) {
        let Some(region) = self
            .display
            .pointer_inputs
            .iter()
            .rev()
            .find(|region| same_area(region.rect, pointer.rect))
        else {
            return;
        };
        let local = |at: Offset| Offset::new(at.x - region.origin.x, at.y - region.origin.y);
        let sample = |other: &ActivePointer| PointerSample {
            id: other.id,
            position: local(other.position),
            window_position: other.position,
            pressure: other.pressure,
        };
        let mut pointers: Vec<PointerSample> = self
            .pointers
            .iter()
            .filter(|other| same_area(other.rect, pointer.rect))
            .map(sample)
            .collect();
        if !listed {
            pointers.retain(|other| other.id != pointer.id);
        }
        let event = PointerInput {
            id: pointer.id,
            kind: pointer.kind,
            phase,
            button: PointerButton::Primary,
            position: local(pointer.position),
            window_position: pointer.position,
            pressure: pointer.pressure,
            scroll_delta,
            modifiers: self.modifiers,
            pointers,
            time: Instant::now(),
        };
        (region.on_event.clone())(&event);
    }
}

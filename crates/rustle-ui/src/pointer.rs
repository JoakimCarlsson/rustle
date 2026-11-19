//! Pointer events as Compose's `Modifier.onPointerEvent` sees them: every press, release,
//! move, enter, exit and scroll over an area, consumed or not, in three passes.
//!
//! An event goes to the areas under the pointer that listen with
//! [`Modifier::on_pointer_event`](crate::Modifier::on_pointer_event), from the outermost to
//! the innermost, in the order Compose dispatches it: the [`Initial`](PointerEventPass::Initial)
//! pass from the outermost area in, the [`Main`](PointerEventPass::Main) pass from the
//! innermost out, then the [`Final`](PointerEventPass::Final) pass from the outermost in.
//! Each area hears the event in the pass it chose. A change consumed in one pass is seen as
//! consumed by every handler after it, in that pass and the ones after.

use std::cell::Cell;
use std::rc::Rc;

use rustle_geometry::{Offset, Rect};

use crate::gesture::{PointerId, PointerKind};
use crate::input::{Modifiers, PointerButton};
use crate::layout::DisplayList;
use crate::layout::regions::contains;

/// What happened to the pointer: Compose's `PointerEventType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerEventType {
    /// Something the other types do not describe.
    Unknown,
    /// A button went down.
    Press,
    /// A button went up.
    Release,
    /// The pointer moved.
    Move,
    /// The pointer came over the area.
    Enter,
    /// The pointer left the area.
    Exit,
    /// The wheel or a trackpad scrolled; see [`PointerInputChange::scroll_delta`].
    Scroll,
}

/// Which of the three passes of an event a handler hears it in: Compose's `PointerEventPass`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerEventPass {
    /// First, from the outermost area under the pointer to the innermost, so a parent can
    /// see an event (and consume it) before its children.
    Initial,
    /// Second, from the innermost area out: where most handlers react.
    Main,
    /// Last, from the outermost area in, after everything else had its turn.
    Final,
}

impl PointerEventPass {
    /// The three passes, in the order an event goes through them.
    pub const ALL: [PointerEventPass; 3] = [
        PointerEventPass::Initial,
        PointerEventPass::Main,
        PointerEventPass::Final,
    ];
}

/// The buttons held when an event happened: Compose's `PointerButtons`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct PointerButtons {
    /// The primary (left) button, a touch or a pen tip.
    pub is_primary_pressed: bool,
    /// The secondary (right) button.
    pub is_secondary_pressed: bool,
    /// The tertiary (middle, wheel) button.
    pub is_tertiary_pressed: bool,
    /// The back thumb button.
    pub is_back_pressed: bool,
    /// The forward thumb button.
    pub is_forward_pressed: bool,
}

impl PointerButtons {
    /// Whether any button is held: Compose's `areAnyPressed`.
    pub fn are_any_pressed(&self) -> bool {
        self.is_primary_pressed
            || self.is_secondary_pressed
            || self.is_tertiary_pressed
            || self.is_back_pressed
            || self.is_forward_pressed
    }

    /// Whether `button` is held. Buttons other than the five named ones are never held.
    pub fn is_pressed(&self, button: PointerButton) -> bool {
        match button {
            PointerButton::Primary => self.is_primary_pressed,
            PointerButton::Secondary => self.is_secondary_pressed,
            PointerButton::Middle => self.is_tertiary_pressed,
            PointerButton::Back => self.is_back_pressed,
            PointerButton::Forward => self.is_forward_pressed,
            PointerButton::Other(_) => false,
        }
    }

    /// These buttons with `button` held or not as `pressed` says.
    pub fn with(mut self, button: PointerButton, pressed: bool) -> Self {
        match button {
            PointerButton::Primary => self.is_primary_pressed = pressed,
            PointerButton::Secondary => self.is_secondary_pressed = pressed,
            PointerButton::Middle => self.is_tertiary_pressed = pressed,
            PointerButton::Back => self.is_back_pressed = pressed,
            PointerButton::Forward => self.is_forward_pressed = pressed,
            PointerButton::Other(_) => {}
        }
        self
    }
}

/// One pointer's part in an event: Compose's `PointerInputChange`.
///
/// Clones share whether the change is consumed, so a handler that consumes it is seen by
/// every handler that hears the event after it.
#[derive(Debug, Clone)]
pub struct PointerInputChange {
    /// Which pointer.
    pub id: PointerId,
    /// What kind of device it is: Compose's `type`.
    pub kind: PointerKind,
    /// Where the pointer is, relative to the top-left corner of the area hearing the event.
    pub position: Offset,
    /// Where it was at the event before, in the same coordinates.
    pub previous_position: Offset,
    /// Where it is in the window, in logical pixels.
    pub window_position: Offset,
    /// Whether a button (or the contact) is down now.
    pub pressed: bool,
    /// Whether it was down at the event before.
    pub previous_pressed: bool,
    /// For a [`Scroll`](PointerEventType::Scroll), how far the wheel turned, in logical
    /// pixels, positive down and right as Compose has it; zero otherwise.
    pub scroll_delta: Offset,
    /// When the event happened, in milliseconds since the interface started handling input.
    pub uptime_millis: u64,
    consumed: Rc<Cell<bool>>,
}

impl PointerInputChange {
    /// A change of pointer `id` that is not consumed, with nothing moved, pressed or scrolled.
    pub fn new(id: PointerId, kind: PointerKind, window_position: Offset) -> Self {
        Self {
            id,
            kind,
            position: window_position,
            previous_position: window_position,
            window_position,
            pressed: false,
            previous_pressed: false,
            scroll_delta: Offset::default(),
            uptime_millis: 0,
            consumed: Rc::new(Cell::new(false)),
        }
    }

    /// Whether a handler consumed the change: Compose's `isConsumed`.
    pub fn is_consumed(&self) -> bool {
        self.consumed.get()
    }

    /// Marks the change as used, so the handlers that hear it after this one, and the
    /// interface's own handling of scrolling, leave it alone: Compose's `consume()`.
    pub fn consume(&self) {
        self.consumed.set(true);
    }

    /// Whether the pointer went down in this change: Compose's `changedToDown`.
    pub fn changed_to_down(&self) -> bool {
        self.pressed && !self.previous_pressed
    }

    /// Whether the pointer went up in this change: Compose's `changedToUp`.
    pub fn changed_to_up(&self) -> bool {
        !self.pressed && self.previous_pressed
    }

    /// How far the pointer moved since the event before: Compose's `positionChange()`.
    pub fn position_change(&self) -> Offset {
        Offset::new(
            self.position.x - self.previous_position.x,
            self.position.y - self.previous_position.y,
        )
    }

    /// This change with its positions moved `by` back, sharing whether it is consumed.
    fn moved_by(&self, by: Offset) -> Self {
        let shift = |at: Offset| Offset::new(at.x - by.x, at.y - by.y);
        Self {
            position: shift(self.position),
            previous_position: shift(self.previous_position),
            ..self.clone()
        }
    }
}

/// An event an [`on_pointer_event`](crate::Modifier::on_pointer_event) handler hears:
/// Compose's `PointerEvent`.
#[derive(Debug, Clone)]
pub struct PointerEvent {
    /// The pointers the event is about, with where they are relative to the area hearing it.
    pub changes: Vec<PointerInputChange>,
    /// The buttons held after the event.
    pub buttons: PointerButtons,
    /// The modifier keys held.
    pub keyboard_modifiers: Modifiers,
    /// What happened.
    pub event_type: PointerEventType,
    /// The button that went down or up, for a [`Press`](PointerEventType::Press) or a
    /// [`Release`](PointerEventType::Release).
    pub button: Option<PointerButton>,
}

impl PointerEvent {
    /// Whether any of the changes was consumed.
    pub fn is_consumed(&self) -> bool {
        self.changes.iter().any(PointerInputChange::is_consumed)
    }

    /// This event with every position relative to `origin`, sharing whether each change is
    /// consumed.
    fn relative_to(&self, origin: Offset) -> Self {
        Self {
            changes: self
                .changes
                .iter()
                .map(|change| change.moved_by(origin))
                .collect(),
            ..self.clone()
        }
    }
}

/// What an [`on_pointer_event`](crate::Modifier::on_pointer_event) handler runs.
pub type PointerEventHandler = Rc<dyn Fn(&PointerEvent)>;

/// An area that listens to one type of pointer event in one pass, cut to what is visible.
pub struct PointerEventRegion {
    /// The part of the area that is visible, in the window.
    pub rect: Rect,
    /// The area's top-left corner in the window, which the event's positions are relative to.
    pub origin: Offset,
    /// The listening area this one was declared inside, as an index into
    /// [`DisplayList::pointer_events`], or `None` at the top.
    pub parent: Option<usize>,
    /// How many listening areas it is inside.
    pub depth: usize,
    /// The type of event it hears.
    pub event_type: PointerEventType,
    /// The pass it hears it in.
    pub pass: PointerEventPass,
    /// What it runs.
    pub on_event: PointerEventHandler,
}

impl DisplayList {
    /// The listening areas the pointer at `point` is over, as indices into
    /// [`pointer_events`](Self::pointer_events), from the outermost to the innermost: the
    /// topmost area that contains the point and the areas it was declared inside that do
    /// too, as Compose's hit test finds the nodes along one path of the tree.
    pub fn pointer_event_path(&self, point: Offset) -> Vec<usize> {
        let Some(top) = self
            .pointer_events
            .iter()
            .rposition(|region| contains(region.rect, point))
        else {
            return Vec::new();
        };
        let mut path = Vec::new();
        let mut at = Some(top);
        while let Some(index) = at {
            let region = &self.pointer_events[index];
            if contains(region.rect, point) {
                path.push(index);
            }
            at = region.parent;
        }
        path.reverse();
        path
    }

    /// Gives `event`, with positions in the window, to the areas of `path` (indices into
    /// [`pointer_events`](Self::pointer_events), outermost first) that listen to its type:
    /// those of the initial pass outermost first, then those of the main pass innermost
    /// first, then those of the final pass outermost first, each with positions relative to
    /// itself. Returns whether a handler consumed a change.
    pub fn dispatch_pointer_event(&self, path: &[usize], event: &PointerEvent) -> bool {
        self.dispatch_pointer_event_in(path, event, &PointerEventPass::ALL)
    }

    /// Gives `event` to the areas of `path` as [`dispatch_pointer_event`](Self::dispatch_pointer_event)
    /// does, in the passes of `passes` only, in their order. A host that runs gestures of its
    /// own between the passes, as Compose's built-in gestures hear an event in the main pass
    /// before the final pass reaches the areas around them, sends the initial and main passes
    /// first and the final pass after its gestures, with the same `event`, whose consumed
    /// changes the passes share. Returns whether a handler consumed a change.
    pub fn dispatch_pointer_event_in(
        &self,
        path: &[usize],
        event: &PointerEvent,
        passes: &[PointerEventPass],
    ) -> bool {
        let handlers: Vec<(PointerEventPass, Offset, PointerEventHandler)> = path
            .iter()
            .filter_map(|index| self.pointer_events.get(*index))
            .filter(|region| region.event_type == event.event_type)
            .map(|region| (region.pass, region.origin, Rc::clone(&region.on_event)))
            .collect();
        for pass in passes.iter().copied() {
            let in_pass = handlers.iter().filter(|(heard, ..)| *heard == pass);
            let ordered: Vec<_> = if pass == PointerEventPass::Main {
                in_pass.rev().collect()
            } else {
                in_pass.collect()
            };
            for (_, origin, on_event) in ordered {
                on_event(&event.relative_to(*origin));
            }
        }
        event.is_consumed()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use rustle_geometry::Size;

    use super::*;

    /// A region of `rect` inside `parent` that records `name` and the pass when it hears an
    /// event, and consumes it when `consumes`.
    fn region(
        rect: Rect,
        parent: Option<usize>,
        pass: PointerEventPass,
        name: &'static str,
        log: &Rc<RefCell<Vec<String>>>,
        consumes: bool,
    ) -> PointerEventRegion {
        let log = Rc::clone(log);
        PointerEventRegion {
            rect,
            origin: rect.origin,
            parent,
            depth: parent.map_or(0, |_| 1),
            event_type: PointerEventType::Press,
            pass,
            on_event: Rc::new(move |event| {
                let change = &event.changes[0];
                log.borrow_mut().push(format!(
                    "{name} {pass:?} {} {}",
                    change.position.x,
                    change.is_consumed()
                ));
                if consumes {
                    change.consume();
                }
            }),
        }
    }

    /// A press at `at` in the window.
    fn press(at: Offset) -> PointerEvent {
        PointerEvent {
            changes: vec![PointerInputChange::new(
                PointerId::MOUSE,
                PointerKind::Mouse,
                at,
            )],
            buttons: PointerButtons::default().with(PointerButton::Primary, true),
            keyboard_modifiers: Modifiers::default(),
            event_type: PointerEventType::Press,
            button: Some(PointerButton::Primary),
        }
    }

    #[test]
    fn passes_go_down_up_and_down_again_and_see_what_was_consumed() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let outer = Rect::new(Offset::new(0.0, 0.0), Size::new(100.0, 100.0));
        let inner = Rect::new(Offset::new(10.0, 0.0), Size::new(50.0, 50.0));
        let list = DisplayList {
            pointer_events: vec![
                region(outer, None, PointerEventPass::Initial, "outer", &log, false),
                region(
                    Rect::new(outer.origin, outer.size),
                    Some(0),
                    PointerEventPass::Main,
                    "outer",
                    &log,
                    false,
                ),
                region(
                    Rect::new(outer.origin, outer.size),
                    Some(1),
                    PointerEventPass::Final,
                    "outer",
                    &log,
                    false,
                ),
                region(
                    inner,
                    Some(2),
                    PointerEventPass::Initial,
                    "inner",
                    &log,
                    false,
                ),
                region(inner, Some(3), PointerEventPass::Main, "inner", &log, true),
                region(
                    inner,
                    Some(4),
                    PointerEventPass::Final,
                    "inner",
                    &log,
                    false,
                ),
            ],
            ..DisplayList::default()
        };
        let path = list.pointer_event_path(Offset::new(20.0, 5.0));
        assert_eq!(path, vec![0, 1, 2, 3, 4, 5]);
        assert!(list.dispatch_pointer_event(&path, &press(Offset::new(20.0, 5.0))));
        assert_eq!(
            *log.borrow(),
            [
                "outer Initial 20 false",
                "inner Initial 10 false",
                "inner Main 10 false",
                "outer Main 20 true",
                "outer Final 20 true",
                "inner Final 10 true",
            ]
        );
    }

    #[test]
    fn the_path_follows_the_topmost_area_and_skips_siblings() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let whole = Rect::new(Offset::new(0.0, 0.0), Size::new(100.0, 100.0));
        let left = Rect::new(Offset::new(0.0, 0.0), Size::new(60.0, 100.0));
        let right = Rect::new(Offset::new(40.0, 0.0), Size::new(60.0, 100.0));
        let list = DisplayList {
            pointer_events: vec![
                region(whole, None, PointerEventPass::Main, "root", &log, false),
                region(left, Some(0), PointerEventPass::Main, "left", &log, false),
                region(right, Some(0), PointerEventPass::Main, "right", &log, false),
            ],
            ..DisplayList::default()
        };
        assert_eq!(list.pointer_event_path(Offset::new(50.0, 10.0)), vec![0, 2]);
        assert_eq!(list.pointer_event_path(Offset::new(10.0, 10.0)), vec![0, 1]);
        assert!(list.pointer_event_path(Offset::new(150.0, 10.0)).is_empty());
    }

    #[test]
    fn handlers_hear_only_their_type() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let whole = Rect::new(Offset::new(0.0, 0.0), Size::new(100.0, 100.0));
        let list = DisplayList {
            pointer_events: vec![region(
                whole,
                None,
                PointerEventPass::Main,
                "root",
                &log,
                false,
            )],
            ..DisplayList::default()
        };
        let mut moved = press(Offset::new(1.0, 1.0));
        moved.event_type = PointerEventType::Move;
        assert!(!list.dispatch_pointer_event(&[0], &moved));
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn buttons_are_set_and_read() {
        let buttons = PointerButtons::default()
            .with(PointerButton::Secondary, true)
            .with(PointerButton::Back, true);
        assert!(buttons.is_secondary_pressed && buttons.is_back_pressed);
        assert!(buttons.are_any_pressed());
        assert!(!buttons.is_pressed(PointerButton::Primary));
        assert!(
            !buttons
                .with(PointerButton::Secondary, false)
                .with(PointerButton::Back, false)
                .are_any_pressed()
        );
    }
}

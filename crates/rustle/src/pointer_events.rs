//! The pointer events of [`Modifier::on_pointer_event`](rustle_ui::Modifier::on_pointer_event):
//! presses, releases, moves, enters, exits and scrolls, sent to the listening areas under the
//! pointer in Compose's three passes.

use std::sync::OnceLock;
use std::time::Instant;

use rustle_geometry::{Offset, Rect};
use rustle_ui::{
    PointerButton, PointerEventPass, PointerEventType, PointerId, PointerInputChange, PointerKind,
    pointer::PointerEvent,
};

use crate::interface::Interface;
use crate::pointing::same_area;

/// A listening area told apart from the others across layouts: where it is, how many
/// listening areas it is inside and its place among them in the display list, since handlers
/// are made anew at each composition. An area that moved keeps its place in a tree whose
/// shape did not change, as a node keeps its identity in Compose wherever it is laid out.
pub(crate) type ListenerKey = (Rect, usize, usize);

/// A pointer event sent in its initial and main passes and held back for its final pass,
/// with the listening areas it goes to.
pub(crate) struct HeldPointerEvent {
    /// The listening areas, as indices into the display list's, outermost first.
    path: Vec<usize>,
    /// The event, whose consumed changes the passes share.
    event: PointerEvent,
}

/// When the first pointer event of the process was made, which event times count from.
static STARTED: OnceLock<Instant> = OnceLock::new();

impl Interface {
    /// The key of the listening area at `index` of the display list.
    fn listener_key(&self, index: usize) -> ListenerKey {
        let region = &self.display.pointer_events[index];
        (region.rect, region.depth, index)
    }

    /// Where the listening area of `key` is in the display list now, if it still is: the
    /// area at the same depth in the same place, else the one at the same depth at the same
    /// place in the list, which is the same area moved when the tree kept its shape.
    fn listener_at(&self, key: ListenerKey) -> Option<usize> {
        let regions = &self.display.pointer_events;
        let (rect, depth, index) = key;
        regions
            .get(index)
            .filter(|region| region.depth == depth && same_area(region.rect, rect))
            .map(|_| index)
            .or_else(|| {
                regions
                    .iter()
                    .position(|region| region.depth == depth && same_area(region.rect, rect))
            })
            .or_else(|| {
                regions
                    .get(index)
                    .filter(|region| region.depth == depth)
                    .map(|_| index)
            })
    }

    /// Finds the areas the pointer is over and those that captured it in the display list
    /// just painted, so each keeps hearing the pointer as the same area wherever it moved,
    /// and forgets those that are gone.
    pub(crate) fn follow_listeners(&mut self) {
        let follow = |interface: &Self, keys: &[ListenerKey]| -> Vec<ListenerKey> {
            keys.iter()
                .filter_map(|key| interface.listener_at(*key))
                .map(|index| interface.listener_key(index))
                .collect()
        };
        self.pointer_event_hovered = follow(self, &self.pointer_event_hovered);
        if let Some(captured) = &self.pointer_event_captured {
            self.pointer_event_captured = Some(follow(self, captured));
        }
    }

    /// The listening areas under the pointer, outermost first, or those the pointer was
    /// captured by while a button is held.
    fn listener_path(&self) -> Vec<usize> {
        if let Some(captured) = &self.pointer_event_captured {
            return captured
                .iter()
                .filter_map(|key| self.listener_at(*key))
                .collect();
        }
        self.pointer
            .map(|pointer| self.display.pointer_event_path(pointer))
            .unwrap_or_default()
    }

    /// The event of `event_type` about the mouse that the listening areas of `path` are sent,
    /// or none when there are none or the pointer was never in the area. `button` is the
    /// button that went down or up and `scroll_delta` how far the wheel turned; `was_pressed`
    /// says whether a button was held before the event.
    fn pointer_event_of(
        &self,
        path: &[usize],
        event_type: PointerEventType,
        button: Option<PointerButton>,
        scroll_delta: Offset,
        was_pressed: bool,
    ) -> Option<PointerEvent> {
        if path.is_empty() {
            return None;
        }
        let at = self.pointer.or(self.pointer_event_previous)?;
        let started = *STARTED.get_or_init(Instant::now);
        let mut change = PointerInputChange::new(PointerId::MOUSE, PointerKind::Mouse, at);
        change.previous_position = self.pointer_event_previous.unwrap_or(at);
        change.pressed = self.pointer_buttons.are_any_pressed();
        change.previous_pressed = was_pressed;
        change.scroll_delta = scroll_delta;
        change.uptime_millis = started.elapsed().as_millis() as u64;
        Some(PointerEvent {
            changes: vec![change],
            buttons: self.pointer_buttons,
            keyboard_modifiers: self.modifiers,
            event_type,
            button,
        })
    }

    /// Sends an event of `event_type` about the mouse to the listening areas of `path`, in
    /// every pass, and returns whether one consumed it. The arguments are as for
    /// [`pointer_event_of`](Self::pointer_event_of).
    fn send_pointer_event(
        &self,
        path: &[usize],
        event_type: PointerEventType,
        button: Option<PointerButton>,
        scroll_delta: Offset,
        was_pressed: bool,
    ) -> bool {
        self.pointer_event_of(path, event_type, button, scroll_delta, was_pressed)
            .is_some_and(|event| self.display.dispatch_pointer_event(path, &event))
    }

    /// Sends an event of `event_type` to the listening areas of `path` in its initial and
    /// main passes, and holds it back for its final pass, which
    /// [`finish_pointer_event`](Self::finish_pointer_event) sends once the built-in gestures
    /// have heard the same input, as Compose's gestures hear it in the main pass before the
    /// areas around them hear the final pass. The arguments are as for
    /// [`pointer_event_of`](Self::pointer_event_of).
    fn begin_pointer_event(
        &self,
        path: Vec<usize>,
        event_type: PointerEventType,
        button: Option<PointerButton>,
        scroll_delta: Offset,
        was_pressed: bool,
    ) -> Option<HeldPointerEvent> {
        let event = self.pointer_event_of(&path, event_type, button, scroll_delta, was_pressed)?;
        self.display.dispatch_pointer_event_in(
            &path,
            &event,
            &[PointerEventPass::Initial, PointerEventPass::Main],
        );
        Some(HeldPointerEvent { path, event })
    }

    /// Sends the final pass of `held`, the event [`begin_pointer_event`](Self::begin_pointer_event)
    /// held back, if there is one.
    fn finish_held_pointer_event(&self, held: Option<HeldPointerEvent>) {
        if let Some(held) = held {
            self.display.dispatch_pointer_event_in(
                &held.path,
                &held.event,
                &[PointerEventPass::Final],
            );
        }
    }

    /// Tells the listening areas the pointer left that it did, innermost first, and those it
    /// came over that it did, outermost first. Nothing changes while a button is held.
    pub(crate) fn update_pointer_event_hover(&mut self) {
        if self.pointer_event_captured.is_some() {
            return;
        }
        let now: Vec<ListenerKey> = self
            .pointer
            .map(|pointer| self.display.pointer_event_path(pointer))
            .unwrap_or_default()
            .into_iter()
            .map(|index| self.listener_key(index))
            .collect();
        let held =
            |keys: &[ListenerKey], key: &ListenerKey| keys.iter().any(|other| other.2 == key.2);
        let pressed = self.pointer_buttons.are_any_pressed();
        let before = std::mem::take(&mut self.pointer_event_hovered);
        for key in before.iter().rev().filter(|key| !held(&now, key)) {
            if let Some(index) = self.listener_at(*key) {
                self.send_pointer_event(
                    &[index],
                    PointerEventType::Exit,
                    None,
                    Offset::default(),
                    pressed,
                );
            }
        }
        for key in now.iter().filter(|key| !held(&before, key)) {
            if let Some(index) = self.listener_at(*key) {
                self.send_pointer_event(
                    &[index],
                    PointerEventType::Enter,
                    None,
                    Offset::default(),
                    pressed,
                );
            }
        }
        self.pointer_event_hovered = now;
    }

    /// The pointer moved to where [`pointer`](Interface::pointer) now says: tells the areas it
    /// left and came over, then those under it (or that captured it) that it moved.
    pub(crate) fn pointer_event_moved(&mut self) {
        self.update_pointer_event_hover();
        let path = self.listener_path();
        let pressed = self.pointer_buttons.are_any_pressed();
        self.send_pointer_event(
            &path,
            PointerEventType::Move,
            None,
            Offset::default(),
            pressed,
        );
        self.pointer_event_previous = self.pointer;
    }

    /// `button` went down or up where the pointer is: tells the listening areas in the initial
    /// and main passes, and returns the event held back for its final pass, which
    /// [`finish_pointer_event`](Self::finish_pointer_event) sends after the built-in handling
    /// of the button. The first button to go down captures the pointer for the areas under
    /// it until the last one is released.
    pub(crate) fn pointer_event_button(
        &mut self,
        button: PointerButton,
        pressed: bool,
    ) -> Option<HeldPointerEvent> {
        let was_pressed = self.pointer_buttons.are_any_pressed();
        if pressed && self.pointer_buttons.is_pressed(button) {
            return None;
        }
        self.pointer_buttons = self.pointer_buttons.with(button, pressed);
        if pressed && !was_pressed {
            let path = self
                .pointer
                .map(|pointer| self.display.pointer_event_path(pointer))
                .unwrap_or_default();
            self.pointer_event_captured =
                Some(path.iter().map(|index| self.listener_key(*index)).collect());
        }
        let path = self.listener_path();
        let event_type = if pressed {
            PointerEventType::Press
        } else {
            PointerEventType::Release
        };
        self.begin_pointer_event(
            path,
            event_type,
            Some(button),
            Offset::default(),
            was_pressed,
        )
    }

    /// Sends the final pass of `held`, the event a button or the wheel began, once the built-in
    /// handling of that input has run, and lets the pointer go from the areas that captured
    /// it once no button is held.
    pub(crate) fn finish_pointer_event(&mut self, held: Option<HeldPointerEvent>) {
        self.finish_held_pointer_event(held);
        if !self.pointer_buttons.are_any_pressed() {
            self.pointer_event_captured = None;
            self.update_pointer_event_hover();
        }
    }

    /// The wheel turned by `delta` logical pixels (the way the content should move) where the
    /// pointer is: tells the areas under it, with Compose's sign, positive down, in the
    /// initial and main passes. Returns whether one consumed the scroll, and the event held
    /// back for its final pass, which [`finish_pointer_event`](Self::finish_pointer_event)
    /// sends after the scrolling areas had their turn.
    pub(crate) fn pointer_event_scroll(
        &mut self,
        delta: Offset,
    ) -> (bool, Option<HeldPointerEvent>) {
        let path = self
            .pointer
            .map(|pointer| self.display.pointer_event_path(pointer))
            .unwrap_or_default();
        let pressed = self.pointer_buttons.are_any_pressed();
        let held = self.begin_pointer_event(
            path,
            PointerEventType::Scroll,
            None,
            Offset::new(-delta.x, -delta.y),
            pressed,
        );
        let consumed = held.as_ref().is_some_and(|held| held.event.is_consumed());
        (consumed, held)
    }

    /// The pointer left the interface: every area it was over hears it leave, and a capture
    /// without a button held ends.
    pub(crate) fn pointer_event_left(&mut self) {
        if !self.pointer_buttons.are_any_pressed() {
            self.pointer_event_captured = None;
        }
        self.update_pointer_event_hover();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use rustle_ui::{Modifier, PointerEventPass};

    use crate::Embedded;
    use crate::testing::{embedded, stack_sized};

    use super::*;

    /// Records each event an area hears as "name type pass consumed".
    type Log = Rc<RefCell<Vec<String>>>;

    /// `modifier` with handlers for every type in every pass that log under `name`.
    fn listening(
        modifier: Modifier,
        name: &'static str,
        log: &Log,
        consume_main: bool,
    ) -> Modifier {
        let mut modifier = modifier;
        for pass in PointerEventPass::ALL {
            for event_type in [
                PointerEventType::Press,
                PointerEventType::Release,
                PointerEventType::Move,
                PointerEventType::Enter,
                PointerEventType::Exit,
                PointerEventType::Scroll,
            ] {
                let log = Rc::clone(log);
                modifier = modifier.on_pointer_event(event_type, pass, move |event| {
                    let change = &event.changes[0];
                    log.borrow_mut().push(format!(
                        "{name} {event_type:?} {pass:?} {}",
                        change.is_consumed()
                    ));
                    if consume_main && pass == PointerEventPass::Main {
                        change.consume();
                    }
                });
            }
        }
        modifier
    }

    /// An interface 200 by 200 with an outer listening area over all of it and an inner one
    /// over its top-left quarter, which consumes in the main pass.
    fn nested(log: &Log) -> Embedded {
        let log = Rc::clone(log);
        embedded(200, 200, move || {
            let inner_log = Rc::clone(&log);
            stack_sized(
                listening(Modifier::new().size(200.0, 200.0), "outer", &log, false),
                move || {
                    stack_sized(
                        listening(
                            Modifier::new().size(100.0, 100.0),
                            "inner",
                            &inner_log,
                            true,
                        ),
                        || {},
                    );
                },
            );
        })
    }

    /// The entries of `log` about `event_type`, emptying it.
    fn take(log: &Log, event_type: &str) -> Vec<String> {
        std::mem::take(&mut *log.borrow_mut())
            .into_iter()
            .filter(|entry| entry.split(' ').nth(1) == Some(event_type))
            .collect()
    }

    #[test]
    fn a_press_goes_through_the_passes_and_is_seen_consumed() {
        let log: Log = Rc::default();
        let mut ui = nested(&log);
        ui.pointer_moved(Offset::new(20.0, 20.0));
        log.borrow_mut().clear();
        ui.pointer_button(PointerButton::Primary, true);
        assert_eq!(
            take(&log, "Press"),
            [
                "outer Press Initial false",
                "inner Press Initial false",
                "inner Press Main false",
                "outer Press Main true",
                "outer Press Final true",
                "inner Press Final true",
            ]
        );
    }

    #[test]
    fn enter_and_exit_follow_the_pointer_and_wait_for_a_release() {
        let log: Log = Rc::default();
        let mut ui = nested(&log);
        ui.pointer_moved(Offset::new(150.0, 150.0));
        assert_eq!(take(&log, "Enter").len(), 3);
        ui.pointer_moved(Offset::new(20.0, 20.0));
        assert!(
            take(&log, "Enter")
                .iter()
                .all(|entry| entry.starts_with("inner"))
        );
        ui.pointer_button(PointerButton::Primary, true);
        ui.pointer_moved(Offset::new(150.0, 150.0));
        let moved = take(&log, "Move");
        assert!(moved.iter().any(|entry| entry.starts_with("inner")));
        ui.pointer_button(PointerButton::Primary, false);
        let exits = take(&log, "Exit");
        assert_eq!(exits.len(), 3);
        assert!(exits.iter().all(|entry| entry.starts_with("inner")));
        ui.pointer_left();
        assert_eq!(take(&log, "Exit").len(), 3);
    }

    #[test]
    fn a_consumed_scroll_does_not_scroll_and_buttons_are_reported() {
        let log: Log = Rc::default();
        let buttons = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&buttons);
        let mut ui = embedded(100, 100, move || {
            let seen = Rc::clone(&seen);
            stack_sized(
                Modifier::new().size(100.0, 100.0).on_pointer_event(
                    PointerEventType::Press,
                    PointerEventPass::Main,
                    move |event| {
                        seen.borrow_mut().push((event.button, event.buttons));
                    },
                ),
                || {},
            );
        });
        ui.pointer_moved(Offset::new(10.0, 10.0));
        ui.pointer_button(PointerButton::Secondary, true);
        ui.pointer_button(PointerButton::Back, true);
        let seen = buttons.borrow();
        assert_eq!(seen[0].0, Some(PointerButton::Secondary));
        assert!(seen[0].1.is_secondary_pressed && !seen[0].1.is_back_pressed);
        assert!(seen[1].1.is_secondary_pressed && seen[1].1.is_back_pressed);
        drop(log);
    }

    #[test]
    fn the_scroll_delta_has_composes_sign_and_consuming_takes_it() {
        let deltas = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&deltas);
        let mut ui = embedded(100, 100, move || {
            let seen = Rc::clone(&seen);
            stack_sized(
                Modifier::new().size(100.0, 100.0).on_pointer_event(
                    PointerEventType::Scroll,
                    PointerEventPass::Main,
                    move |event| {
                        seen.borrow_mut().push(event.changes[0].scroll_delta);
                        event.changes[0].consume();
                    },
                ),
                || {},
            );
        });
        ui.pointer_moved(Offset::new(10.0, 10.0));
        assert!(ui.wheel(Offset::new(0.0, 48.0)));
        assert_eq!(deltas.borrow()[0], Offset::new(-0.0, -48.0));
    }

    /// The final pass of a press and a release reaches an area around a clickable after the
    /// clickable heard them, as Compose's gestures hear an event in the main pass, while its
    /// initial pass comes before.
    #[test]
    fn the_final_pass_comes_after_the_built_in_gestures() {
        let log: Log = Rc::default();
        let outer = Rc::clone(&log);
        let mut ui = embedded(100, 100, move || {
            let mut modifier = Modifier::new().size(100.0, 100.0);
            for pass in [PointerEventPass::Initial, PointerEventPass::Final] {
                for event_type in [PointerEventType::Press, PointerEventType::Release] {
                    let log = Rc::clone(&outer);
                    modifier = modifier.on_pointer_event(event_type, pass, move |_| {
                        log.borrow_mut()
                            .push(format!("outer {event_type:?} {pass:?}"));
                    });
                }
            }
            let inner = Rc::clone(&outer);
            stack_sized(modifier, move || {
                let pressed = Rc::clone(&inner);
                let clicked = Rc::clone(&inner);
                stack_sized(
                    Modifier::new()
                        .size(50.0, 50.0)
                        .on_press(move |_| pressed.borrow_mut().push("pressed".to_owned()))
                        .clickable(move || clicked.borrow_mut().push("clicked".to_owned())),
                    || {},
                );
            });
        });
        ui.pointer_moved(Offset::new(10.0, 10.0));
        ui.pointer_button(PointerButton::Primary, true);
        ui.pointer_button(PointerButton::Primary, false);
        assert_eq!(
            *log.borrow(),
            [
                "outer Press Initial",
                "pressed",
                "outer Press Final",
                "outer Release Initial",
                "clicked",
                "outer Release Final",
            ]
        );
    }

    /// An area that moves while it holds the pointer, as a handle dragged along does, hears
    /// it leave once it is let go and the pointer goes elsewhere: every enter it heard has its
    /// exit, wherever the area was when the pointer came over it.
    #[test]
    fn an_area_moved_during_a_drag_hears_the_pointer_leave() {
        let log: Log = Rc::default();
        let x = rustle_runtime::mutable_state(0.0_f32);
        let (root_log, root_x) = (Rc::clone(&log), x.clone());
        let mut ui = embedded(200, 200, move || {
            let moved = root_x.clone();
            let modifier = Modifier::new()
                .offset(root_x.get(), 0.0)
                .size(20.0, 20.0)
                .on_pointer_event(
                    PointerEventType::Move,
                    PointerEventPass::Main,
                    move |event| {
                        if event.changes[0].pressed {
                            moved.set(event.changes[0].position.x + moved.get() - 10.0);
                        }
                    },
                );
            stack_sized(listening(modifier, "handle", &root_log, false), || {});
        });
        ui.pointer_moved(Offset::new(10.0, 10.0));
        ui.pointer_button(PointerButton::Primary, true);
        for step in 1..=4 {
            ui.pointer_moved(Offset::new(10.0 + 10.0 * step as f32, 10.0));
            ui.update();
        }
        assert!(x.get() > 0.0, "the area did not follow the drag");
        ui.pointer_button(PointerButton::Primary, false);
        ui.update();
        ui.pointer_moved(Offset::new(150.0, 150.0));
        ui.update();
        let heard = std::mem::take(&mut *log.borrow_mut());
        let count = |kind: &str| {
            heard
                .iter()
                .filter(|entry| entry.split(' ').nth(1) == Some(kind) && entry.contains("Main"))
                .count()
        };
        assert_eq!(count("Enter"), count("Exit"), "{heard:?}");
    }

    /// The final pass of a scroll reaches the listening areas after the scrolling area under
    /// them scrolled.
    #[test]
    fn the_final_pass_of_a_scroll_comes_after_scrolling() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let state = rustle_ui::ScrollState::new(0.0);
        let (heard, scrolled) = (Rc::clone(&seen), state.clone());
        let mut ui = embedded(100, 100, move || {
            let (heard, scrolled) = (Rc::clone(&heard), scrolled.clone());
            let watched = scrolled.clone();
            stack_sized(
                Modifier::new().size(100.0, 100.0).on_pointer_event(
                    PointerEventType::Scroll,
                    PointerEventPass::Final,
                    move |_| heard.borrow_mut().push(watched.offset()),
                ),
                move || {
                    rustle_foundation::column_with(
                        Modifier::new()
                            .size(100.0, 100.0)
                            .vertical_scroll(&scrolled),
                        || {
                            rustle_foundation::spacer(Modifier::new().size(100.0, 400.0));
                        },
                    );
                },
            );
        });
        ui.pointer_moved(Offset::new(10.0, 10.0));
        assert!(ui.wheel(Offset::new(0.0, -48.0)));
        assert_eq!(*seen.borrow(), [48.0]);
    }
}

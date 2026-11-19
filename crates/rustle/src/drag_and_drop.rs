//! Drags and drops inside the window: a source picked up past the threshold, the targets
//! that accept it told as it starts, enters, moves over, leaves, is dropped and ends.

use std::any::Any;
use std::rc::Rc;

use rustle_geometry::{Offset, Rect};
use rustle_ui::{DragAndDropEvent, DragAndDropTarget, TransferDataFn};

use crate::interface::Interface;
use crate::pointer::same_area;

/// A press on a drag-and-drop source, and the drag it became once the pointer moved far
/// enough.
pub(crate) struct DragAndDrop {
    /// Where the press began, in the window.
    began: Offset,
    /// Where the source's top-left corner was.
    origin: Offset,
    /// What the source carries, asked when the drag starts.
    transfer_data: TransferDataFn,
    /// How far the pointer moves from where the press began before the drag starts.
    touch_slop: f32,
    /// What is carried, once the drag started.
    payload: Option<Rc<dyn Any>>,
    /// The targets that accepted the drag when it started.
    started: Vec<Rc<dyn DragAndDropTarget>>,
    /// The target the pointer is over, and where it is.
    over: Option<(Rect, Rc<dyn DragAndDropTarget>)>,
}

impl Interface {
    /// Notes a press of the primary button at `pointer` on a drag-and-drop source, if it is
    /// on one, so moving far enough starts a drag.
    pub(crate) fn press_drag_and_drop(&mut self, pointer: Offset) {
        self.drag_and_drop =
            self.display
                .drag_and_drop_source_at(pointer)
                .map(|region| DragAndDrop {
                    began: pointer,
                    origin: region.origin,
                    transfer_data: Rc::clone(&region.transfer_data),
                    touch_slop: region.touch_slop,
                    payload: None,
                    started: Vec::new(),
                    over: None,
                });
    }

    /// What a target is told with the pointer at `pointer`, carrying `payload`.
    fn drag_and_drop_event(&self, payload: &Rc<dyn Any>, pointer: Offset) -> DragAndDropEvent {
        DragAndDropEvent {
            payload: Rc::clone(payload),
            position: pointer,
            modifiers: self.modifiers,
        }
    }

    /// Follows the pointer to `pointer` while a drag-and-drop source is held: starts the drag
    /// once it is far enough from the press, then tells the targets it leaves, enters and
    /// moves over.
    pub(crate) fn move_drag_and_drop(&mut self, pointer: Offset) {
        let Some(drag) = self.drag_and_drop.as_mut() else {
            return;
        };
        if drag.payload.is_none() {
            let (dx, dy) = (pointer.x - drag.began.x, pointer.y - drag.began.y);
            if dx.hypot(dy) < drag.touch_slop {
                return;
            }
            let at = Offset::new(drag.began.x - drag.origin.x, drag.began.y - drag.origin.y);
            let Some(data) = (drag.transfer_data)(at) else {
                self.drag_and_drop = None;
                return;
            };
            drag.payload = Some(data.payload);
            self.start_drag_and_drop(pointer);
        }
        self.redraw = true;
        let Some(payload) = self
            .drag_and_drop
            .as_ref()
            .and_then(|drag| drag.payload.clone())
        else {
            return;
        };
        let event = self.drag_and_drop_event(&payload, pointer);
        let under = self.target_under(&event);
        let Some(drag) = self.drag_and_drop.as_mut() else {
            return;
        };
        let same = match (&drag.over, &under) {
            (Some((was, _)), Some((now, _))) => same_area(*was, *now),
            (None, None) => true,
            _ => false,
        };
        if !same {
            if let Some((_, left)) = drag.over.take() {
                left.on_exited(&event);
            }
            if let Some((_, entered)) = &under {
                entered.on_entered(&event);
            }
        }
        drag.over = under;
        if let Some((_, over)) = &drag.over {
            over.on_moved(&event);
        }
    }

    /// Tells every target that accepts the drag just picked up that it started.
    fn start_drag_and_drop(&mut self, pointer: Offset) {
        let Some(payload) = self
            .drag_and_drop
            .as_ref()
            .and_then(|drag| drag.payload.clone())
        else {
            return;
        };
        let event = self.drag_and_drop_event(&payload, pointer);
        let started: Vec<_> = self
            .display
            .drag_and_drop_targets
            .iter()
            .filter(|region| (region.should_start)(&event))
            .map(|region| Rc::clone(&region.target))
            .collect();
        for target in &started {
            target.on_started(&event);
        }
        if let Some(drag) = self.drag_and_drop.as_mut() {
            drag.started = started;
        }
    }

    /// The topmost target under the pointer that accepts what `event` carries, and where it is.
    fn target_under(&self, event: &DragAndDropEvent) -> Option<(Rect, Rc<dyn DragAndDropTarget>)> {
        self.display
            .drag_and_drop_targets
            .iter()
            .rev()
            .filter(|region| {
                let rect = region.rect;
                event.position.x >= rect.origin.x
                    && event.position.y >= rect.origin.y
                    && event.position.x < rect.origin.x + rect.size.width
                    && event.position.y < rect.origin.y + rect.size.height
            })
            .find(|region| (region.should_start)(event))
            .map(|region| (region.rect, Rc::clone(&region.target)))
    }

    /// Ends the drag and drop under way when the primary button is let go: the target the
    /// pointer is over is given the drop, and every target that accepted the drag hears it
    /// ended. Says whether there was a drag, so the press it began with clicks nothing.
    pub(crate) fn finish_drag_and_drop(&mut self) -> bool {
        let Some(drag) = self.drag_and_drop.take() else {
            return false;
        };
        let Some(payload) = drag.payload else {
            return false;
        };
        self.redraw = true;
        let at = self.pointer.unwrap_or(drag.began);
        let event = self.drag_and_drop_event(&payload, at);
        if self.pointer.is_some()
            && let Some((_, over)) = &drag.over
        {
            over.on_drop(&event);
        }
        for target in &drag.started {
            target.on_ended(&event);
        }
        true
    }
}

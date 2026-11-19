//! Drags and drops: a source inside the window picked up past the threshold, or files
//! dragged in from another application, and the targets that accept it told as it starts,
//! enters, moves over, leaves, is dropped and ends.

use std::any::Any;
use std::path::PathBuf;
use std::rc::Rc;

use rustle_geometry::{Offset, Rect};
use rustle_ui::{DragAndDropEvent, DragAndDropTarget, DragData, TransferDataFn};

use crate::interface::Interface;
use crate::pointing::same_area;

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

/// The target under the pointer during a drag, and where it is.
type Over = Option<(Rect, Rc<dyn DragAndDropTarget>)>;

/// Files from another application dragged over the area, from when they come over it until
/// they are let go or leave.
pub(crate) struct FileDragAndDrop {
    /// The targets that accepted the drag when it came over the area.
    started: Vec<Rc<dyn DragAndDropTarget>>,
    /// The target the pointer is over, and where it is.
    over: Over,
}

/// Tells the target `over` was over that the pointer left it and the one `under` is over now
/// that the pointer came over it, when they are not the same area, then tells the one under
/// the pointer that it moved, and keeps it in `over`.
fn follow_targets(over: &mut Over, under: Over, event: &DragAndDropEvent) {
    let same = match (&*over, &under) {
        (Some((was, _)), Some((now, _))) => same_area(*was, *now),
        (None, None) => true,
        _ => false,
    };
    if !same {
        if let Some((_, left)) = over.take() {
            left.on_exited(event);
        }
        if let Some((_, entered)) = &under {
            entered.on_entered(event);
        }
    }
    *over = under;
    if let Some((_, target)) = &*over {
        target.on_moved(event);
    }
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
        if let Some(drag) = self.drag_and_drop.as_mut() {
            follow_targets(&mut drag.over, under, &event);
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
        let started = self.start_targets(&event);
        if let Some(drag) = self.drag_and_drop.as_mut() {
            drag.started = started;
        }
    }

    /// Tells every target that accepts the drag `event` is about that it started, and gives
    /// them back.
    fn start_targets(&self, event: &DragAndDropEvent) -> Vec<Rc<dyn DragAndDropTarget>> {
        let started: Vec<_> = self
            .display
            .drag_and_drop_targets
            .iter()
            .filter(|region| (region.should_start)(event))
            .map(|region| Rc::clone(&region.target))
            .collect();
        for target in &started {
            target.on_started(event);
        }
        started
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

    /// What a target is told of files from another application at `pointer`, carrying
    /// `files`.
    fn file_drag_event(&self, files: Vec<PathBuf>, pointer: Offset) -> DragAndDropEvent {
        let payload: Rc<dyn Any> = Rc::new(DragData::FilesList(files));
        self.drag_and_drop_event(&payload, pointer)
    }

    /// Whether something at `pointer` would take files from another application let go there:
    /// a [`drag_and_drop_target`](rustle_ui::Modifier::drag_and_drop_target) that accepts
    /// them, or a file drop area.
    pub(crate) fn takes_files_at(&self, pointer: Offset) -> bool {
        let event = self.file_drag_event(Vec::new(), pointer);
        self.target_under(&event).is_some() || self.display.file_drop_at(pointer).is_some()
    }

    /// Follows files from another application held at `pointer`: the targets that accept them
    /// hear the drag started the first time, then the targets the pointer leaves, enters and
    /// moves over are told. Says whether letting go there would be taken.
    pub(crate) fn files_dragged_over(&mut self, pointer: Offset) -> bool {
        let event = self.file_drag_event(Vec::new(), pointer);
        if self.file_drag_and_drop.is_none() {
            let started = self.start_targets(&event);
            self.file_drag_and_drop = Some(FileDragAndDrop {
                started,
                over: None,
            });
        }
        let under = self.target_under(&event);
        let taken = under.is_some() || self.display.file_drop_at(pointer).is_some();
        if let Some(drag) = self.file_drag_and_drop.as_mut() {
            follow_targets(&mut drag.over, under, &event);
        }
        self.redraw = true;
        taken
    }

    /// Ends the drag of files from another application without a drop: the target the pointer
    /// was over hears it left, and every target that accepted the drag that it ended.
    pub(crate) fn files_drag_left(&mut self) {
        let Some(drag) = self.file_drag_and_drop.take() else {
            return;
        };
        let event = self.file_drag_event(Vec::new(), self.pointer.unwrap_or_default());
        if let Some((_, over)) = &drag.over {
            over.on_exited(&event);
        }
        for target in &drag.started {
            target.on_ended(&event);
        }
        self.redraw = true;
    }

    /// Hands `paths`, files from another application let go at `pointer`, to the target that
    /// accepts them there, with a [`DragData::FilesList`] of them all, or else one by one to
    /// the file drop area there; every target that accepted the drag hears it ended. Says
    /// whether anything took them.
    pub(crate) fn files_let_go(&mut self, pointer: Offset, paths: Vec<PathBuf>) -> bool {
        self.files_dragged_over(pointer);
        let Some(drag) = self.file_drag_and_drop.take() else {
            return false;
        };
        let event = self.file_drag_event(paths.clone(), pointer);
        let taken = match &drag.over {
            Some((_, over)) => over.on_drop(&event),
            None => match self.display.file_drop_at(pointer) {
                Some(region) => {
                    let on_drop = Rc::clone(&region.on_drop);
                    for path in paths {
                        on_drop(path);
                    }
                    true
                }
                None => false,
            },
        };
        for target in &drag.started {
            target.on_ended(&event);
        }
        self.redraw = true;
        taken
    }
}

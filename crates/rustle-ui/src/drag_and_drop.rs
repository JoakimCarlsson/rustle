//! Drag and drop inside the window: Compose's `dragAndDropSource` and `dragAndDropTarget`
//! with their `DragAndDropTarget` callbacks.

use std::any::Any;
use std::rc::Rc;

use rustle_geometry::{Offset, Rect};

use crate::input::Modifiers;
use crate::modifier::{Element, Modifier};

/// What a drag and drop carries: Compose's `DragAndDropTransferData`, holding any value the
/// targets downcast.
#[derive(Clone)]
pub struct DragAndDropTransferData {
    /// What is carried.
    pub payload: Rc<dyn Any>,
}

impl DragAndDropTransferData {
    /// Transfer data carrying `payload`.
    pub fn new(payload: impl Any) -> Self {
        Self {
            payload: Rc::new(payload),
        }
    }
}

/// A moment of a drag and drop, as a target sees it: Compose's `DragAndDropEvent`.
#[derive(Clone)]
pub struct DragAndDropEvent {
    /// What is carried; downcast it to the type the source gave.
    pub payload: Rc<dyn Any>,
    /// Where the pointer is, in the window, in logical pixels: Compose's `positionInRoot`.
    pub position: Offset,
    /// The modifier keys held, such as the one that turns a move into a copy.
    pub modifiers: Modifiers,
}

impl DragAndDropEvent {
    /// What is carried, if it is a `T`.
    pub fn payload<T: Any>(&self) -> Option<&T> {
        self.payload.downcast_ref()
    }
}

/// What a drop target is told during a drag and drop: Compose's `DragAndDropTarget`. Every
/// target that said yes to a drag when it started hears `on_started` and `on_ended`; the
/// topmost of them under the pointer hears `on_entered`, `on_moved` and `on_exited` as the
/// pointer comes, moves over it and leaves, and `on_drop` when the button is let go over it.
pub trait DragAndDropTarget {
    /// The carried thing was let go over the target; says whether the target took it.
    fn on_drop(&self, event: &DragAndDropEvent) -> bool;

    /// A drag and drop the target accepts started.
    fn on_started(&self, _event: &DragAndDropEvent) {}

    /// The pointer came over the target.
    fn on_entered(&self, _event: &DragAndDropEvent) {}

    /// The pointer moved over the target.
    fn on_moved(&self, _event: &DragAndDropEvent) {}

    /// The pointer left the target.
    fn on_exited(&self, _event: &DragAndDropEvent) {}

    /// The drag and drop ended, dropped or not.
    fn on_ended(&self, _event: &DragAndDropEvent) {}
}

/// Works out what a drag starting at a point of the source (from its top-left corner)
/// carries, or `None` for no drag.
pub type TransferDataFn = Rc<dyn Fn(Offset) -> Option<DragAndDropTransferData>>;

/// Says whether a target takes part in a drag and drop that starts.
pub type ShouldStartFn = Rc<dyn Fn(&DragAndDropEvent) -> bool>;

/// An area a drag and drop can start from, cut to what is visible.
pub struct DragAndDropSourceRegion {
    /// The part of the area that is visible, in the window.
    pub rect: Rect,
    /// Where the area's top-left corner is, before it was cut to what is visible.
    pub origin: Offset,
    /// What a drag starting here carries.
    pub transfer_data: TransferDataFn,
    /// How far the pointer moves with the primary button held before a drag starts, in
    /// logical pixels: the [`ViewConfiguration::touch_slop`](crate::ViewConfiguration::touch_slop)
    /// where the area was made.
    pub touch_slop: f32,
}

/// An area that takes part in drags and drops, cut to what is visible.
pub struct DragAndDropTargetRegion {
    /// The part of the area that is visible, in the window.
    pub rect: Rect,
    /// Whether it takes part in a drag that starts.
    pub should_start: ShouldStartFn,
    /// What it is told.
    pub target: Rc<dyn DragAndDropTarget>,
}

impl Modifier {
    /// Makes the area a place a drag and drop starts from: once the pointer has moved
    /// the [`touch_slop`](crate::ViewConfiguration::touch_slop) of the
    /// [`LOCAL_VIEW_CONFIGURATION`](crate::LOCAL_VIEW_CONFIGURATION) with the primary button
    /// held on it, `transfer_data` is
    /// asked, with where the press began in the area, what the drag carries, and the
    /// [`drag_and_drop_target`](Self::drag_and_drop_target)s hear of it until the button is
    /// let go. Compose's `dragAndDropSource(transferData)`. The press still reaches the area,
    /// but no click follows a drag. Nothing is drawn under the pointer: the targets show what
    /// the drag would do.
    pub fn drag_and_drop_source(
        self,
        transfer_data: impl Fn(Offset) -> Option<DragAndDropTransferData> + 'static,
    ) -> Self {
        self.then(Element::DragAndDropSource(
            Rc::new(transfer_data),
            crate::view_configuration::current_view_configuration().touch_slop,
        ))
    }

    /// Makes the area take part in the drags and drops `should_start_drag_and_drop` says yes
    /// to when they start, telling `target` of them: Compose's
    /// `dragAndDropTarget(shouldStartDragAndDrop, target)`.
    pub fn drag_and_drop_target(
        self,
        should_start_drag_and_drop: impl Fn(&DragAndDropEvent) -> bool + 'static,
        target: impl DragAndDropTarget + 'static,
    ) -> Self {
        self.then(Element::DragAndDropTarget(
            Rc::new(should_start_drag_and_drop),
            Rc::new(target),
        ))
    }
}

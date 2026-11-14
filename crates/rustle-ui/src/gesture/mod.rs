//! Gesture recognition built on the low-level pointer stream: the events
//! [`Modifier::pointer_input`](crate::Modifier::pointer_input) receives, velocity tracking,
//! drags, anchored drags, multi-touch transforms and scrolling with nested scroll and fling.

mod anchored;
mod drag;
mod scrolling;
mod transform;

use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};

use rustle_geometry::Offset;

use crate::input::{Modifiers, PointerButton};

pub use anchored::{AnchoredDraggableState, remember_anchored_draggable_state};
pub(crate) use drag::{DragRecognizer, drag_hooks};
pub use drag::{DraggableState, remember_draggable_state};
pub(crate) use scrolling::ScrollableCore;
pub use scrolling::{NestedScrollConnection, ScrollableState, remember_scrollable_state};
pub use transform::{TransformableState, remember_transformable_state};

/// The handler of [`Modifier::pointer_input`](crate::Modifier::pointer_input): sees each
/// event and returns whether it used it.
pub type PointerInputHandler = Rc<dyn Fn(&PointerInput) -> bool>;

/// The axis a gesture follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Left and right.
    Horizontal,
    /// Up and down.
    Vertical,
}

impl Axis {
    /// The part of `offset` along this axis.
    pub fn of(self, offset: Offset) -> f32 {
        match self {
            Axis::Horizontal => offset.x,
            Axis::Vertical => offset.y,
        }
    }
}

/// Identifies one pointer (the mouse, or one finger or pen contact) while it is down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PointerId(pub u64);

impl PointerId {
    /// The id of the mouse.
    pub const MOUSE: PointerId = PointerId(u64::MAX);
}

/// What kind of device a pointer is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerKind {
    /// A mouse or a trackpad cursor.
    Mouse,
    /// A finger.
    Touch,
    /// A pen or stylus.
    Pen,
}

/// What happened to a pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerInputPhase {
    /// The pointer went down on the area (the primary button, or a contact).
    Down,
    /// The pointer moved while down. Also delivered outside the area.
    Move,
    /// The pointer was lifted.
    Up,
    /// The system took the pointer away: the gesture is over without a result.
    Cancel,
    /// The wheel or a trackpad scrolled where the pointer is; see [`PointerInput::scroll_delta`].
    Scroll,
}

/// One pointer that is down on an area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerSample {
    /// Which pointer.
    pub id: PointerId,
    /// Where it is, relative to the area's top-left corner.
    pub position: Offset,
    /// Where it is in the window, in logical pixels.
    pub window_position: Offset,
    /// How hard it presses, from 0 to 1; 0.5 for a mouse button and when the device does not say.
    pub pressure: f32,
}

/// An event in the stream [`Modifier::pointer_input`](crate::Modifier::pointer_input) sees.
#[derive(Debug, Clone, PartialEq)]
pub struct PointerInput {
    /// The pointer the event is about.
    pub id: PointerId,
    /// The kind of device it is.
    pub kind: PointerKind,
    /// What happened.
    pub phase: PointerInputPhase,
    /// The button that went down: [`PointerButton::Primary`] for contacts.
    pub button: PointerButton,
    /// Where the pointer is, relative to the area's top-left corner.
    pub position: Offset,
    /// Where the pointer is in the window, in logical pixels.
    pub window_position: Offset,
    /// How hard the pointer presses, from 0 to 1; 0.5 for a mouse and when the device does not say.
    pub pressure: f32,
    /// For [`PointerInputPhase::Scroll`], how far the wheel turned in logical pixels (the
    /// content should move with it); zero otherwise.
    pub scroll_delta: Offset,
    /// The modifier keys held.
    pub modifiers: Modifiers,
    /// Every pointer that is down on the area, this one included unless it was just lifted.
    pub pointers: Vec<PointerSample>,
    /// When the event happened.
    pub time: Instant,
}

/// Estimates how fast a pointer moves from its latest positions.
#[derive(Debug, Default)]
pub struct VelocityTracker {
    samples: VecDeque<(Instant, Offset)>,
}

impl VelocityTracker {
    /// How far back samples count.
    const HORIZON: Duration = Duration::from_millis(100);
    /// The largest speed reported, in logical pixels per second.
    const MAX: f32 = 8000.0;

    /// An empty tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets every sample.
    pub fn reset(&mut self) {
        self.samples.clear();
    }

    /// Records that the pointer was at `position` at `time`.
    pub fn add(&mut self, time: Instant, position: Offset) {
        self.samples.push_back((time, position));
        while self
            .samples
            .front()
            .is_some_and(|(at, _)| time.saturating_duration_since(*at) > Self::HORIZON)
        {
            self.samples.pop_front();
        }
    }

    /// The speed over the recorded samples, in logical pixels per second.
    pub fn velocity(&self) -> Offset {
        let (Some((first_time, first)), Some((last_time, last))) =
            (self.samples.front(), self.samples.back())
        else {
            return Offset::default();
        };
        let seconds = last_time
            .saturating_duration_since(*first_time)
            .as_secs_f32();
        if seconds <= 0.0 {
            return Offset::default();
        }
        Offset::new(
            ((last.x - first.x) / seconds).clamp(-Self::MAX, Self::MAX),
            ((last.y - first.y) / seconds).clamp(-Self::MAX, Self::MAX),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn velocity_is_distance_over_time() {
        let start = Instant::now();
        let mut tracker = VelocityTracker::new();
        tracker.add(start, Offset::new(0.0, 0.0));
        tracker.add(start + Duration::from_millis(50), Offset::new(10.0, -5.0));
        let velocity = tracker.velocity();
        assert!((velocity.x - 200.0).abs() < 0.5);
        assert!((velocity.y + 100.0).abs() < 0.5);
    }

    #[test]
    fn old_samples_are_dropped() {
        let start = Instant::now();
        let mut tracker = VelocityTracker::new();
        tracker.add(start, Offset::new(0.0, 0.0));
        tracker.add(start + Duration::from_millis(500), Offset::new(100.0, 0.0));
        tracker.add(start + Duration::from_millis(550), Offset::new(110.0, 0.0));
        assert!((tracker.velocity().x - 200.0).abs() < 0.5);
    }
}

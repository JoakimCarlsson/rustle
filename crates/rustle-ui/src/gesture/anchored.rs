//! A value dragged between fixed positions that it settles on when let go.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use rustle_runtime::{FrameClock, State, frame_clock, group, remember};

use super::DragRecognizer;

/// A fling at least this fast (logical pixels per second) moves on to the next anchor.
const VELOCITY_THRESHOLD: f32 = 125.0;
/// Stiffness of the spring that settles on an anchor.
const STIFFNESS: f32 = 600.0;
/// Time step of the spring, in seconds.
const STEP: f32 = 1.0 / 240.0;

/// What an [`AnchoredDraggableState`] shares between the composition and a settling animation.
struct Inner {
    anchors: RefCell<Vec<f32>>,
    generation: Cell<u64>,
    velocity: Cell<f32>,
    clock: FrameClock,
    pub(crate) recognizer: Rc<DragRecognizer>,
}

/// An offset that is dragged along an axis and settles on one of a set of anchors when
/// let go: the nearest, or the next one in the direction of a fling. Anchors are numbered
/// by their place in the list given to [`set_anchors`](Self::set_anchors).
#[derive(Clone)]
pub struct AnchoredDraggableState {
    offset: State<f32>,
    target: State<usize>,
    dragging: State<bool>,
    settling: State<bool>,
    inner: Rc<Inner>,
}

impl AnchoredDraggableState {
    /// Where the dragged thing is now, between the lowest and highest anchor.
    pub fn offset(&self) -> f32 {
        self.offset.get()
    }

    /// The anchor it is settled on or settling to.
    pub fn target(&self) -> usize {
        self.target.get()
    }

    /// The position of anchor `index`, if there is one.
    pub fn anchor_offset(&self, index: usize) -> Option<f32> {
        self.inner.anchors.borrow().get(index).copied()
    }

    /// Whether a drag is going on.
    pub fn is_dragging(&self) -> bool {
        self.dragging.get()
    }

    /// Whether it is animating towards an anchor.
    pub fn is_settling(&self) -> bool {
        self.settling.get()
    }

    /// How far it is from its lowest anchor to its target, from 0 to 1.
    pub fn progress(&self) -> f32 {
        let anchors = self.inner.anchors.borrow();
        let (Some(low), Some(high)) = (
            anchors.iter().copied().reduce(f32::min),
            anchors.iter().copied().reduce(f32::max),
        ) else {
            return 0.0;
        };
        if high <= low {
            return 0.0;
        }
        ((self.offset() - low) / (high - low)).clamp(0.0, 1.0)
    }

    /// Replaces the anchors. When they differ from the current ones, the offset jumps to
    /// where the target anchor now is, unless a drag or an animation is under way.
    pub fn set_anchors(&self, anchors: Vec<f32>) {
        if *self.inner.anchors.borrow() == anchors {
            return;
        }
        *self.inner.anchors.borrow_mut() = anchors;
        if !self.is_dragging()
            && !self.is_settling()
            && let Some(position) = self.anchor_offset(self.target())
        {
            self.offset.set(position);
        }
    }

    /// Moves the offset by `delta` from code or a drag, staying between the outer anchors.
    pub fn drag_by(&self, delta: f32) {
        self.inner.generation.set(self.inner.generation.get() + 1);
        self.settling.set(false);
        let (low, high) = self.bounds();
        self.offset
            .set((self.offset.get() + delta).clamp(low, high));
    }

    /// Jumps to anchor `index` without animating.
    pub fn snap_to(&self, index: usize) {
        self.inner.generation.set(self.inner.generation.get() + 1);
        self.settling.set(false);
        if let Some(position) = self.anchor_offset(index) {
            self.target.set(index);
            self.offset.set(position);
        }
    }

    /// Animates to anchor `index`, starting at `velocity` logical pixels per second.
    pub fn animate_to(&self, index: usize, velocity: f32) {
        let Some(position) = self.anchor_offset(index) else {
            return;
        };
        let generation = self.inner.generation.get() + 1;
        self.inner.generation.set(generation);
        self.target.set(index);
        self.settling.set(true);
        self.inner.velocity.set(velocity);
        self.step(generation, position, None);
    }

    /// Chooses where to settle after a drag let go at `velocity` and animates there: the
    /// next anchor in the direction of a fast fling, otherwise the nearest.
    pub fn settle(&self, velocity: f32) {
        let offset = self.offset();
        let anchors = self.inner.anchors.borrow().clone();
        let nearest = |candidates: &mut dyn Iterator<Item = (usize, f32)>| {
            candidates
                .min_by(|a, b| (a.1 - offset).abs().total_cmp(&(b.1 - offset).abs()))
                .map(|(index, _)| index)
        };
        let all = || anchors.iter().copied().enumerate();
        let ahead = if velocity.abs() >= VELOCITY_THRESHOLD {
            nearest(&mut all().filter(|(_, anchor)| {
                if velocity > 0.0 {
                    *anchor > offset + 0.5
                } else {
                    *anchor < offset - 0.5
                }
            }))
        } else {
            None
        };
        if let Some(index) = ahead.or_else(|| nearest(&mut all())) {
            self.animate_to(index, velocity);
        }
    }

    /// The lowest and highest anchor, or 0 and 0 with none.
    fn bounds(&self) -> (f32, f32) {
        let anchors = self.inner.anchors.borrow();
        (
            anchors.iter().copied().reduce(f32::min).unwrap_or(0.0),
            anchors.iter().copied().reduce(f32::max).unwrap_or(0.0),
        )
    }

    /// Advances the settling spring one frame, and asks for the next until it rests.
    fn step(&self, generation: u64, target: f32, last: Option<Duration>) {
        let this = self.clone();
        self.inner.clock.request_frame(move |now| {
            if this.inner.generation.get() != generation {
                return;
            }
            let seconds = last.map_or(1.0 / 60.0, |last| {
                now.saturating_sub(last).as_secs_f32().min(0.05)
            });
            let damping = 2.0 * STIFFNESS.sqrt();
            let mut position = this.offset.get();
            let mut velocity = this.inner.velocity.get();
            let mut remaining = seconds;
            while remaining > 0.0 {
                let dt = remaining.min(STEP);
                velocity += (-STIFFNESS * (position - target) - damping * velocity) * dt;
                position += velocity * dt;
                remaining -= dt;
            }
            let (low, high) = this.bounds();
            if position < low || position > high {
                position = position.clamp(low, high);
                velocity = 0.0;
            }
            if (position - target).abs() < 0.5 && velocity.abs() < 5.0 {
                this.offset.set(target);
                this.inner.velocity.set(0.0);
                this.settling.set(false);
            } else {
                this.offset.set(position);
                this.inner.velocity.set(velocity);
                this.step(generation, target, Some(now));
            }
        });
    }

    pub(crate) fn recognizer(&self) -> Rc<DragRecognizer> {
        Rc::clone(&self.inner.recognizer)
    }

    pub(crate) fn set_dragging(&self, dragging: bool) {
        self.dragging.set(dragging);
    }
}

/// Remembers an [`AnchoredDraggableState`] at this call site, with `anchors` and settled on
/// the anchor `initial`. Later calls keep the state and replace its anchors with `anchors`.
#[track_caller]
pub fn remember_anchored_draggable_state(
    anchors: Vec<f32>,
    initial: usize,
) -> AnchoredDraggableState {
    group(|| {
        let start = anchors.get(initial).copied().unwrap_or(0.0);
        let offset = remember(|| start);
        let target = remember(|| initial);
        let dragging = remember(|| false);
        let settling = remember(|| false);
        let inner = remember(|| {
            Rc::new(Inner {
                anchors: RefCell::new(anchors.clone()),
                generation: Cell::new(0),
                velocity: Cell::new(0.0),
                clock: frame_clock(),
                recognizer: Rc::new(DragRecognizer::default()),
            })
        });
        let state = AnchoredDraggableState {
            offset,
            target,
            dragging,
            settling,
            inner: inner.get(),
        };
        state.set_anchors(anchors);
        state
    })
}

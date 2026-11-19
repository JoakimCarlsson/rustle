//! Drags along an axis: the recognizer every drag-like modifier shares, and the state of
//! [`Modifier::draggable`](crate::Modifier::draggable).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rustle_geometry::Offset;
use rustle_runtime::{State, group, remember};

use super::{Axis, PointerId, PointerInput, PointerInputPhase, VelocityTracker};

/// What a drag tells whoever follows it.
pub(crate) struct DragHooks<'a> {
    /// The pointer went down, before any drag started.
    pub pressed: &'a dyn Fn(),
    /// The pointer moved past the touch slop.
    pub started: &'a dyn Fn(),
    /// The pointer moved by this much along the axis.
    pub delta: &'a dyn Fn(f32),
    /// The pointer was lifted at this speed along the axis (zero when cancelled).
    pub stopped: &'a dyn Fn(f32),
}

/// Follows one pointer through a drag along an axis.
#[derive(Default)]
pub(crate) struct DragRecognizer {
    pointer: Cell<Option<PointerId>>,
    start: Cell<Offset>,
    last: Cell<Offset>,
    started: Cell<bool>,
    tracker: RefCell<VelocityTracker>,
}

impl DragRecognizer {
    /// Feeds one event of the stream, starting a drag once the pointer has moved `touch_slop`
    /// logical pixels along `axis`. Returns whether it was part of a drag that started.
    pub(crate) fn handle(
        &self,
        event: &PointerInput,
        axis: Axis,
        touch_slop: f32,
        hooks: &DragHooks,
    ) -> bool {
        match event.phase {
            PointerInputPhase::Down => {
                if let Some(held) = self.pointer.get()
                    && (held == event.id || !event.pointers.iter().any(|sample| sample.id == held))
                {
                    self.pointer.set(None);
                    if self.started.replace(false) {
                        (hooks.stopped)(0.0);
                    }
                }
                if self.pointer.get().is_none() {
                    (hooks.pressed)();
                    self.pointer.set(Some(event.id));
                    self.start.set(event.window_position);
                    self.last.set(event.window_position);
                    self.started.set(false);
                    let mut tracker = self.tracker.borrow_mut();
                    tracker.reset();
                    tracker.add(event.time, event.window_position);
                }
                false
            }
            PointerInputPhase::Move if self.pointer.get() == Some(event.id) => {
                self.tracker
                    .borrow_mut()
                    .add(event.time, event.window_position);
                if !self.started.get() {
                    let moved = axis.of(event.window_position) - axis.of(self.start.get());
                    if moved.abs() < touch_slop {
                        return false;
                    }
                    self.started.set(true);
                    (hooks.started)();
                }
                let delta = axis.of(event.window_position) - axis.of(self.last.get());
                self.last.set(event.window_position);
                if delta != 0.0 {
                    (hooks.delta)(delta);
                }
                true
            }
            PointerInputPhase::Up | PointerInputPhase::Cancel
                if self.pointer.get() == Some(event.id) =>
            {
                self.pointer.set(None);
                let was = self.started.replace(false);
                if was {
                    let velocity = if event.phase == PointerInputPhase::Up {
                        let mut tracker = self.tracker.borrow_mut();
                        tracker.add(event.time, event.window_position);
                        axis.of(tracker.velocity())
                    } else {
                        0.0
                    };
                    (hooks.stopped)(velocity);
                }
                was
            }
            _ => false,
        }
    }
}

/// How far something was dragged along one axis, how fast it was let go, and whether a
/// drag is going on. Observable, so what depends on it recomposes as it moves.
#[derive(Clone)]
pub struct DraggableState {
    offset: State<f32>,
    dragging: State<bool>,
    velocity: Rc<Cell<f32>>,
    pub(crate) recognizer: Rc<DragRecognizer>,
}

impl DraggableState {
    /// The distance dragged so far along the axis, from where it started.
    pub fn offset(&self) -> f32 {
        self.offset.get()
    }

    /// Sets the distance dragged, for resetting it or moving it from code.
    pub fn set_offset(&self, offset: f32) {
        self.offset.set(offset);
    }

    /// Whether a drag is going on.
    pub fn is_dragging(&self) -> bool {
        self.dragging.get()
    }

    /// How fast the last drag ended, in logical pixels per second along the axis.
    pub fn velocity(&self) -> f32 {
        self.velocity.get()
    }

    /// Moves the offset by `delta`.
    pub fn drag_by(&self, delta: f32) {
        self.offset.update(|offset| *offset += delta);
    }

    pub(crate) fn hooks_apply<R>(
        &self,
        auto_apply: bool,
        on_delta: &dyn Fn(f32),
        on_stopped: &dyn Fn(f32),
        run: impl FnOnce(&DragHooks) -> R,
    ) -> R {
        let started = || self.dragging.set(true);
        let delta = |delta: f32| {
            if auto_apply {
                self.drag_by(delta);
            }
            on_delta(delta);
        };
        let stopped = |velocity: f32| {
            self.velocity.set(velocity);
            self.dragging.set(false);
            on_stopped(velocity);
        };
        let pressed = || {};
        run(&DragHooks {
            pressed: &pressed,
            started: &started,
            delta: &delta,
            stopped: &stopped,
        })
    }
}

/// Remembers a [`DraggableState`] at this call site, starting at offset 0.
#[track_caller]
pub fn remember_draggable_state() -> DraggableState {
    group(|| {
        let offset = remember(|| 0.0);
        let dragging = remember(|| false);
        let inner = remember(|| {
            (
                Rc::new(Cell::new(0.0_f32)),
                Rc::new(DragRecognizer::default()),
            )
        });
        let (velocity, recognizer) = inner.get();
        DraggableState {
            offset,
            dragging,
            velocity,
            recognizer,
        }
    })
}

/// Bundles three closures as the hooks of a drag.
pub(crate) fn drag_hooks<'a>(
    pressed: &'a dyn Fn(),
    started: &'a dyn Fn(),
    delta: &'a dyn Fn(f32),
    stopped: &'a dyn Fn(f32),
) -> DragHooks<'a> {
    DragHooks {
        pressed,
        started,
        delta,
        stopped,
    }
}

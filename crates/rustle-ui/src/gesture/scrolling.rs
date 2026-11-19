//! Scrolling that is not tied to a [`ScrollState`](crate::ScrollState): a state with a
//! custom way to consume scroll, nested scrolling between areas, and fling.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use rustle_runtime::{FrameClock, frame_clock, group, is_composing, remember};

use super::DragRecognizer;

/// How quickly a fling slows, per second: the speed is multiplied by `e^-DECAY` each second.
const DECAY: f32 = 3.0;
/// A fling slower than this (logical pixels per second) has stopped.
const MIN_SPEED: f32 = 20.0;

/// What a fling needs to survive the composition that made it.
pub(crate) struct ScrollableCore {
    generation: Cell<u64>,
    active: Cell<bool>,
    scrolling: Cell<bool>,
    /// The frame clock flings run on: the composition's that made the core, or that of
    /// the first composition to show a core made outside one.
    clock: std::cell::RefCell<Option<FrameClock>>,
    pub(crate) recognizer: DragRecognizer,
}

impl ScrollableCore {
    /// A core that runs flings on the current frame clock, or, made outside composition,
    /// on the clock it is given later with [`attach_clock`](Self::attach_clock).
    pub(crate) fn new() -> Self {
        Self {
            generation: Cell::new(0),
            active: Cell::new(false),
            scrolling: Cell::new(false),
            clock: std::cell::RefCell::new(is_composing().then(frame_clock)),
            recognizer: DragRecognizer::default(),
        }
    }

    /// Takes the frame clock of the composition being composed, if it has none yet.
    pub(crate) fn attach_clock(&self) {
        if self.clock.borrow().is_none() && is_composing() {
            *self.clock.borrow_mut() = Some(frame_clock());
        }
    }
}

type Pre = Rc<dyn Fn(f32) -> f32>;
type Post = Rc<dyn Fn(f32, f32) -> f32>;

/// Takes part in the scrolling of the areas inside it: it can take a share of the scroll
/// before a child gets it, and the leftover after. All amounts are along the scrolling axis,
/// in logical pixels (or pixels per second for flings), and positive towards the end of the
/// content. Each callback returns how much it consumed.
#[derive(Clone, Default)]
pub struct NestedScrollConnection {
    pre_scroll: Option<Pre>,
    post_scroll: Option<Post>,
    pre_fling: Option<Pre>,
    post_fling: Option<Post>,
}

impl NestedScrollConnection {
    /// A connection that consumes nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs `on_pre_scroll` with the scroll a child is about to apply, before it does.
    pub fn on_pre_scroll(mut self, on_pre_scroll: impl Fn(f32) -> f32 + 'static) -> Self {
        self.pre_scroll = Some(Rc::new(on_pre_scroll));
        self
    }

    /// Runs `on_post_scroll` with how much the child consumed and how much was left over.
    pub fn on_post_scroll(mut self, on_post_scroll: impl Fn(f32, f32) -> f32 + 'static) -> Self {
        self.post_scroll = Some(Rc::new(on_post_scroll));
        self
    }

    /// Runs `on_pre_fling` with the speed of a fling before the child starts it.
    pub fn on_pre_fling(mut self, on_pre_fling: impl Fn(f32) -> f32 + 'static) -> Self {
        self.pre_fling = Some(Rc::new(on_pre_fling));
        self
    }

    /// Runs `on_post_fling` with the speed the child consumed and the speed left when it
    /// reached its end.
    pub fn on_post_fling(mut self, on_post_fling: impl Fn(f32, f32) -> f32 + 'static) -> Self {
        self.post_fling = Some(Rc::new(on_post_fling));
        self
    }

    /// How much of `available` the connection takes before a child scrolls.
    pub fn pre_scroll(&self, available: f32) -> f32 {
        self.pre_scroll.as_ref().map_or(0.0, |f| f(available))
    }

    /// How much of `available` the connection takes after a child scrolled by `consumed`.
    pub fn post_scroll(&self, consumed: f32, available: f32) -> f32 {
        self.post_scroll
            .as_ref()
            .map_or(0.0, |f| f(consumed, available))
    }

    /// How much of the fling `available` the connection takes before a child flings.
    pub fn pre_fling(&self, available: f32) -> f32 {
        self.pre_fling.as_ref().map_or(0.0, |f| f(available))
    }

    /// How much of the fling `available` the connection takes after a child flung by `consumed`.
    pub fn post_fling(&self, consumed: f32, available: f32) -> f32 {
        self.post_fling
            .as_ref()
            .map_or(0.0, |f| f(consumed, available))
    }
}

/// Something that scrolls by being given distances, whatever it scrolls: it applies as much
/// of a distance as it can and says how much that was. Used by
/// [`Modifier::scrollable`](crate::Modifier::scrollable) for wheel, drag and fling.
#[derive(Clone)]
pub struct ScrollableState {
    scroll: Rc<dyn Fn(f32) -> f32>,
    pub(crate) core: Rc<ScrollableCore>,
}

impl ScrollableState {
    pub(crate) fn from_parts(scroll: Rc<dyn Fn(f32) -> f32>, core: Rc<ScrollableCore>) -> Self {
        Self { scroll, core }
    }

    /// Scrolls by `delta` (positive towards the end) and returns how much it consumed.
    pub fn scroll_by(&self, delta: f32) -> f32 {
        (self.scroll)(delta)
    }

    /// Whether a drag or a fling is moving it.
    pub fn is_scroll_in_progress(&self) -> bool {
        self.core.scrolling.get() || self.core.active.get()
    }

    /// Whether a fling is running.
    pub fn is_flinging(&self) -> bool {
        self.core.active.get()
    }

    pub(crate) fn set_scrolling(&self, scrolling: bool) {
        self.core.scrolling.set(scrolling);
    }

    /// Stops a fling that is running.
    pub fn stop_fling(&self) {
        self.core.generation.set(self.core.generation.get() + 1);
        self.core.active.set(false);
    }

    /// Keeps scrolling at `velocity` (logical pixels per second, positive towards the end),
    /// slowing down until it stops. When it reaches the end of what can be scrolled with
    /// speed left, the leftover is offered to `parent`'s
    /// [`post_fling`](NestedScrollConnection::post_fling).
    pub fn fling(&self, velocity: f32, parent: Option<NestedScrollConnection>) {
        self.stop_fling();
        if velocity.abs() < MIN_SPEED {
            return;
        }
        let generation = self.core.generation.get();
        self.core.active.set(true);
        self.fling_step(generation, velocity, parent, None);
    }

    fn fling_step(
        &self,
        generation: u64,
        velocity: f32,
        parent: Option<NestedScrollConnection>,
        last: Option<Duration>,
    ) {
        let this = self.clone();
        let Some(clock) = self.core.clock.borrow().clone() else {
            return;
        };
        clock.request_frame(move |now| {
            if this.core.generation.get() != generation {
                return;
            }
            let seconds = last.map_or(1.0 / 60.0, |last| {
                now.saturating_sub(last).as_secs_f32().min(0.05)
            });
            let delta = velocity / DECAY * (1.0 - (-DECAY * seconds).exp());
            let consumed = this.scroll_by(delta);
            let slowed = velocity * (-DECAY * seconds).exp();
            let blocked = delta != 0.0 && consumed.abs() < delta.abs() * 0.5;
            if blocked {
                this.core.active.set(false);
                if let Some(parent) = &parent {
                    parent.post_fling(0.0, velocity);
                }
            } else if slowed.abs() < MIN_SPEED {
                this.core.active.set(false);
            } else {
                this.fling_step(generation, slowed, parent, Some(now));
            }
        });
    }

    /// A connection that gives what its children leave over to this state: scroll and fling
    /// that a nested child could not use. Pass it to
    /// [`Modifier::scrollable_nested`](crate::Modifier::scrollable_nested) on the child.
    pub fn connection(&self) -> NestedScrollConnection {
        let for_scroll = self.clone();
        let for_fling = self.clone();
        NestedScrollConnection::new()
            .on_post_scroll(move |_, available| for_scroll.scroll_by(available))
            .on_post_fling(move |_, available| {
                for_fling.fling(available, None);
                available
            })
    }
}

/// Remembers a [`ScrollableState`] at this call site that scrolls by calling `scroll_by`, which
/// is given a distance (positive towards the end) and returns how much it used.
#[track_caller]
pub fn remember_scrollable_state(scroll_by: impl Fn(f32) -> f32 + 'static) -> ScrollableState {
    group(|| {
        let slot = remember(|| {
            let current: Rc<RefCell<Pre>> = Rc::new(RefCell::new(Rc::new(|_| 0.0)));
            (current, Rc::new(ScrollableCore::new()))
        });
        let (current, core) = slot.get();
        *current.borrow_mut() = Rc::new(scroll_by);
        ScrollableState::from_parts(Rc::new(move |delta| current.borrow().clone()(delta)), core)
    })
}

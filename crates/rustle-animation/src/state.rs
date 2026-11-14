use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use rustle_graphics::Color;
use rustle_runtime::{FrameClock, State, TaskScope, frame_clock, remember};

use crate::lerp::*;
use crate::spec::*;

/// Steps an animation frame by frame, tracking its progress from 0 to 1.
pub(crate) struct Stepper {
    spec: AnimationSpec,
    /// When the animation began, set by its first frame.
    started: Option<Duration>,
    last_frame: Duration,
    /// Eased progress from 0 (at the start) to 1 (at the target).
    pub(crate) progress: f32,
    /// How fast `progress` is changing, for springs.
    velocity: f32,
}

impl Stepper {
    pub(crate) fn new(spec: AnimationSpec) -> Self {
        Self {
            spec,
            started: None,
            last_frame: Duration::ZERO,
            progress: 0.0,
            velocity: 0.0,
        }
    }

    /// Advances to the frame at `now` and returns whether the animation finished.
    pub(crate) fn step(&mut self, now: Duration) -> bool {
        let started = *self.started.get_or_insert(now);
        let done = match self.spec {
            AnimationSpec::Tween { duration, easing } => {
                let t = if duration.is_zero() {
                    1.0
                } else {
                    ((now - started).as_secs_f32() / duration.as_secs_f32()).min(1.0)
                };
                self.progress = easing.apply(t);
                t >= 1.0
            }
            AnimationSpec::Snap => {
                self.progress = 1.0;
                true
            }
            AnimationSpec::Keyframes { duration, frames } => {
                let t = if duration.is_zero() {
                    1.0
                } else {
                    ((now - started).as_secs_f32() / duration.as_secs_f32()).min(1.0)
                };
                self.progress = keyframe_progress(frames, t);
                t >= 1.0
            }
            AnimationSpec::Repeat {
                iterations,
                mode,
                duration,
                easing,
            } => {
                if duration.is_zero() {
                    let finished = iterations.is_some();
                    self.progress = easing.apply(1.0);
                    self.last_frame = now;
                    return finished;
                }
                let cycles = (now - started).as_secs_f32() / duration.as_secs_f32();
                let finished = iterations.is_some_and(|count| cycles >= count as f32);
                let cycles = iterations.map_or(cycles, |count| cycles.min(count as f32));
                let whole = if finished {
                    cycles - 1.0
                } else {
                    cycles.floor()
                };
                let mut t = if finished { 1.0 } else { cycles - whole };
                if mode == RepeatMode::Reverse && whole as u32 % 2 == 1 {
                    t = 1.0 - t;
                }
                self.progress = easing.apply(t);
                finished
            }
            AnimationSpec::Spring {
                damping_ratio,
                stiffness,
            } => {
                let elapsed = if now == started {
                    0.0
                } else {
                    (now - self.last_frame).as_secs_f32().min(0.05)
                };
                let damping = 2.0 * damping_ratio * stiffness.sqrt();
                // Small fixed steps keep the integration stable.
                let steps = (elapsed / (1.0 / 240.0)).ceil().max(1.0) as u32;
                let step = elapsed / steps as f32;
                for _ in 0..steps {
                    let force = -stiffness * (self.progress - 1.0) - damping * self.velocity;
                    self.velocity += force * step;
                    self.progress += self.velocity * step;
                }
                (self.progress - 1.0).abs() < 0.001 && self.velocity.abs() < 0.01
            }
        };
        self.last_frame = now;
        done
    }
}

impl Stepper {
    /// The value for the current frame: `to` once a finished animation ends at
    /// its target, otherwise `from` blended by the progress. A reversing
    /// repeat with an even count ends where it began.
    pub(crate) fn resolve<T: Lerp + Clone>(&self, from: &T, to: &T, done: bool) -> T {
        let at_start = matches!(
            self.spec,
            AnimationSpec::Repeat {
                iterations: Some(count),
                mode: RepeatMode::Reverse,
                ..
            } if count % 2 == 0
        );
        if done && !at_start {
            to.clone()
        } else {
            from.lerp(to, self.progress)
        }
    }
}

/// The progress at `t` (0 to 1 of the duration) along `frames`.
fn keyframe_progress(frames: &[Keyframe], t: f32) -> f32 {
    let mut previous = Keyframe::new(0.0, 0.0);
    for frame in frames {
        if t <= frame.at {
            let span = frame.at - previous.at;
            let local = if span <= 0.0 {
                1.0
            } else {
                ((t - previous.at) / span).clamp(0.0, 1.0)
            };
            return previous.progress
                + (frame.progress - previous.progress) * frame.easing.apply(local);
        }
        previous = *frame;
    }
    let span = 1.0 - previous.at;
    let local = if span <= 0.0 {
        1.0
    } else {
        ((t - previous.at) / span).clamp(0.0, 1.0)
    };
    previous.progress + (1.0 - previous.progress) * local
}

pub(crate) struct Inner<T> {
    value: State<T>,
    from: T,
    target: T,
    stepper: Stepper,
    running: bool,
    /// Counts the running animations of a transition, which this one is part of.
    active: Option<Rc<Cell<usize>>>,
}

impl<T> Inner<T> {
    /// Records whether this animation runs, keeping the transition's count.
    fn set_running(&mut self, running: bool) {
        if self.running == running {
            return;
        }
        self.running = running;
        if let Some(active) = &self.active {
            active.set(if running {
                active.get() + 1
            } else {
                active.get().saturating_sub(1)
            });
        }
    }
}

/// What `animate_as_state` remembers. Dropping it, when the call leaves the
/// composition, stops the animation.
pub(crate) struct Animation<T> {
    inner: Rc<RefCell<Inner<T>>>,
    clock: FrameClock,
    alive: Rc<Cell<bool>>,
}

impl<T> Drop for Animation<T> {
    fn drop(&mut self) {
        self.alive.set(false);
        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            inner.set_running(false);
        }
    }
}

impl<T: Lerp + Clone + PartialEq + 'static> Animation<T> {
    fn retarget(&self, target: T, spec: AnimationSpec) {
        let start = {
            let mut inner = self.inner.borrow_mut();
            if inner.target == target {
                return;
            }
            inner.from = inner.value.get();
            inner.target = target;
            inner.stepper = Stepper::new(spec);
            let was_running = inner.running;
            inner.set_running(true);
            !was_running
        };
        if start {
            request_tick(&self.inner, &self.clock, &self.alive);
        }
    }
}

pub(crate) fn request_tick<T: Lerp + Clone + PartialEq + 'static>(
    inner: &Rc<RefCell<Inner<T>>>,
    clock: &FrameClock,
    alive: &Rc<Cell<bool>>,
) {
    let (inner, next_clock, alive) = (Rc::clone(inner), clock.clone(), Rc::clone(alive));
    clock.request_frame(move |now| {
        if !alive.get() {
            return;
        }
        let (value, state, done) = {
            let mut inner = inner.borrow_mut();
            let done = inner.stepper.step(now);
            let value = inner.stepper.resolve(&inner.from, &inner.target, done);
            if done {
                inner.set_running(false);
            }
            (value, inner.value.clone(), done)
        };
        state.set(value);
        if !done {
            request_tick(&inner, &next_clock, &alive);
        }
    });
}

/// Returns state that follows `target`: it starts at `target`, and each time
/// `target` changes it moves there as `spec` says. Changing the target midway
/// starts from wherever the value is.
#[track_caller]
pub fn animate_as_state<T: Lerp + Clone + PartialEq + 'static>(
    target: T,
    spec: AnimationSpec,
) -> State<T> {
    animate_counted(target, spec, None)
}

/// [`animate_as_state`] that adds one to `active` while it is moving.
#[track_caller]
pub(crate) fn animate_counted<T: Lerp + Clone + PartialEq + 'static>(
    target: T,
    spec: AnimationSpec,
    active: Option<Rc<Cell<usize>>>,
) -> State<T> {
    let value = remember(|| target.clone());
    let animation = rustle_runtime::remember_live(|| Animation {
        inner: Rc::new(RefCell::new(Inner {
            value: value.clone(),
            from: target.clone(),
            target: target.clone(),
            stepper: Stepper::new(spec),
            running: false,
            active,
        })),
        clock: frame_clock(),
        alive: Rc::new(Cell::new(true)),
    });
    animation.with(|animation| animation.retarget(target, spec));
    value
}

/// [`animate_as_state`] for a float.
#[track_caller]
pub fn animate_float_as_state(target: f32, spec: AnimationSpec) -> State<f32> {
    animate_as_state(target, spec)
}

/// [`animate_as_state`] for a color.
#[track_caller]
pub fn animate_color_as_state(target: Color, spec: AnimationSpec) -> State<Color> {
    animate_as_state(target, spec)
}

/// Moves a number from `from` to `to` as `spec` says, calling `apply` with each value
/// on the way, the last being `to`. For things other than state, such as a scroll position.
pub async fn animate_value(
    scope: &TaskScope,
    from: f32,
    to: f32,
    spec: AnimationSpec,
    mut apply: impl FnMut(f32),
) {
    let mut stepper = Stepper::new(spec);
    loop {
        let now = scope.next_frame().await;
        let done = stepper.step(now);
        apply(stepper.resolve(&from, &to, done));
        if done {
            break;
        }
    }
}

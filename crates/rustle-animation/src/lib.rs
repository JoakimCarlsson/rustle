//! Animation: values that move toward a target over time.
//!
//! [`animate_as_state`] returns state that follows a target value. Whenever the
//! target changes, the state moves to it frame by frame, driven by the frame
//! clock, so frames are only requested while something is moving.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use rustle_graphics::Color;
use rustle_runtime::{FrameClock, State, TaskScope, frame_clock, launched_effect, remember};

/// Values that can be blended.
pub trait Lerp {
    /// The value `t` of the way from `self` to `to`, where `t` is 0 at `self` and 1 at `to`.
    fn lerp(&self, to: &Self, t: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        self + (to - self) * t
    }
}

impl Lerp for Color {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        let channel = |from: u8, to: u8| f32::from(from).lerp(&f32::from(to), t).round() as u8;
        Color::rgba(
            channel(self.r, to.r),
            channel(self.g, to.g),
            channel(self.b, to.b),
            channel(self.a, to.a),
        )
    }
}

/// How progress from 0 to 1 is shaped over a tween's duration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
    Linear,
    /// Starts slowly.
    EaseIn,
    /// Ends slowly.
    EaseOut,
    /// Starts and ends slowly.
    EaseInOut,
    /// A CSS-style cubic Bezier through (0, 0), (x1, y1), (x2, y2) and (1, 1).
    CubicBezier(f32, f32, f32, f32),
}

impl Easing {
    /// Material's standard curve: quick start, long settle.
    pub const FAST_OUT_SLOW_IN: Self = Self::CubicBezier(0.4, 0.0, 0.2, 1.0);

    fn apply(self, t: f32) -> f32 {
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t * t,
            Self::EaseOut => 1.0 - (1.0 - t).powi(3),
            Self::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Self::CubicBezier(x1, y1, x2, y2) => cubic_bezier(x1, y1, x2, y2, t),
        }
    }
}

/// Solves for the curve parameter whose x is `x`, then returns its y.
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let at = |a: f32, b: f32, s: f32| {
        let inverse = 1.0 - s;
        3.0 * inverse * inverse * s * a + 3.0 * inverse * s * s * b + s * s * s
    };
    let (mut low, mut high) = (0.0_f32, 1.0_f32);
    for _ in 0..24 {
        let middle = (low + high) / 2.0;
        if at(x1, x2, middle) < x {
            low = middle;
        } else {
            high = middle;
        }
    }
    at(y1, y2, (low + high) / 2.0)
}

/// How a value travels to its target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimationSpec {
    /// A fixed duration, shaped by an easing curve.
    Tween { duration: Duration, easing: Easing },
    /// A damped spring. `damping_ratio` below 1 overshoots and bounces; 1 is
    /// the quickest without overshoot. A higher `stiffness` is faster.
    Spring { damping_ratio: f32, stiffness: f32 },
}

/// A tween of `milliseconds` with the standard easing.
pub fn tween(milliseconds: u64) -> AnimationSpec {
    AnimationSpec::Tween {
        duration: Duration::from_millis(milliseconds),
        easing: Easing::FAST_OUT_SLOW_IN,
    }
}

/// A spring that bounces a little.
pub fn spring() -> AnimationSpec {
    AnimationSpec::Spring {
        damping_ratio: 0.6,
        stiffness: 400.0,
    }
}

impl Default for AnimationSpec {
    fn default() -> Self {
        tween(300)
    }
}

/// Steps an animation frame by frame, tracking its progress from 0 to 1.
struct Stepper {
    spec: AnimationSpec,
    /// When the animation began, set by its first frame.
    started: Option<Duration>,
    last_frame: Duration,
    /// Eased progress from 0 (at the start) to 1 (at the target).
    progress: f32,
    /// How fast `progress` is changing, for springs.
    velocity: f32,
}

impl Stepper {
    fn new(spec: AnimationSpec) -> Self {
        Self {
            spec,
            started: None,
            last_frame: Duration::ZERO,
            progress: 0.0,
            velocity: 0.0,
        }
    }

    /// Advances to the frame at `now` and returns whether the animation finished.
    fn step(&mut self, now: Duration) -> bool {
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

struct Inner<T> {
    value: State<T>,
    from: T,
    target: T,
    stepper: Stepper,
    running: bool,
}

/// What `animate_as_state` remembers. Dropping it, when the call leaves the
/// composition, stops the animation.
struct Animation<T> {
    inner: Rc<RefCell<Inner<T>>>,
    clock: FrameClock,
    alive: Rc<Cell<bool>>,
}

impl<T> Drop for Animation<T> {
    fn drop(&mut self) {
        self.alive.set(false);
    }
}

impl<T: Lerp + Clone + PartialEq + 'static> Animation<T> {
    fn retarget(&self, target: T, spec: AnimationSpec) {
        let start = {
            let mut inner = self.inner.borrow_mut();
            if inner.target == target {
                inner.stepper.spec = spec;
                return;
            }
            inner.from = inner.value.get();
            inner.target = target;
            inner.stepper = Stepper::new(spec);
            !std::mem::replace(&mut inner.running, true)
        };
        if start {
            request_tick(&self.inner, &self.clock, &self.alive);
        }
    }
}

fn request_tick<T: Lerp + Clone + PartialEq + 'static>(
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
            let value = if done {
                inner.target.clone()
            } else {
                inner.from.lerp(&inner.target, inner.stepper.progress)
            };
            if done {
                inner.running = false;
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
    let value = remember(|| target.clone());
    let animation = remember(|| Animation {
        inner: Rc::new(RefCell::new(Inner {
            value: value.clone(),
            from: target.clone(),
            target: target.clone(),
            stepper: Stepper::new(spec),
            running: false,
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

/// A value that tasks animate with [`Animatable::animate_to`].
///
/// Where [`animate_as_state`] follows a target, an `Animatable` is driven
/// step by step from a [`launched_effect`](rustle_runtime::launched_effect),
/// so animations can be sequenced and mixed with other waiting.
#[derive(Clone)]
pub struct Animatable<T> {
    value: State<T>,
}

impl<T: Lerp + Clone + 'static> Animatable<T> {
    /// The state holding the current value, to read while composing.
    pub fn state(&self) -> &State<T> {
        &self.value
    }

    pub fn value(&self) -> T {
        self.value.get()
    }

    /// Jumps to `value` without animating.
    pub fn snap_to(&self, value: T) {
        self.value.set(value);
    }

    /// Moves to `target` as `spec` says, finishing when it arrives. Dropping
    /// the future, which cancelling its task does, stops it where it is.
    pub async fn animate_to(&self, scope: &TaskScope, target: T, spec: AnimationSpec) {
        let from = self.value.get();
        let mut stepper = Stepper::new(spec);
        loop {
            let now = scope.next_frame().await;
            let done = stepper.step(now);
            self.value.set(if done {
                target.clone()
            } else {
                from.lerp(&target, stepper.progress)
            });
            if done {
                break;
            }
        }
    }
}

/// Remembers an [`Animatable`] at this call site, starting at `initial`.
#[track_caller]
pub fn remember_animatable<T: Clone + 'static>(initial: T) -> Animatable<T> {
    Animatable {
        value: remember(|| initial),
    }
}

/// Returns a float that runs from `from` to `to` in `duration`, then jumps back and
/// runs again, for as long as the call stays in the composition. For spinning things.
#[track_caller]
pub fn infinite_float_repeating(
    from: f32,
    to: f32,
    duration: Duration,
    easing: Easing,
) -> State<f32> {
    let animatable = remember_animatable(from);
    let driven = animatable.clone();
    launched_effect((), move |scope| async move {
        let spec = AnimationSpec::Tween { duration, easing };
        loop {
            driven.snap_to(from);
            driven.animate_to(&scope, to, spec).await;
        }
    });
    animatable.state().clone()
}

/// Returns a float that runs from `from` to `to` and back, over and over, for
/// as long as the call stays in the composition. Each way takes `duration`.
#[track_caller]
pub fn infinite_float(from: f32, to: f32, duration: Duration, easing: Easing) -> State<f32> {
    let animatable = remember_animatable(from);
    let driven = animatable.clone();
    launched_effect((), move |scope| async move {
        let spec = AnimationSpec::Tween { duration, easing };
        loop {
            driven.animate_to(&scope, to, spec).await;
            driven.animate_to(&scope, from, spec).await;
        }
    });
    animatable.state().clone()
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
        apply(if done {
            to
        } else {
            from.lerp(&to, stepper.progress)
        });
        if done {
            break;
        }
    }
}

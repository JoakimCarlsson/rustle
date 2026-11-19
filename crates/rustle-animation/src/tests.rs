//! Tests of motion over time: a composition whose frame clock the tests advance frame by frame.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use rustle_graphics::{Color, FillRule, Path, PathCommand};
use rustle_runtime::{Composition, State, launched_effect};

use super::*;

/// A composition driven by hand: each [`Rig::advance`] runs one frame and recomposes.
struct Rig<T: 'static> {
    composition: Composition<T>,
    now: Duration,
}

impl<T: 'static> Rig<T> {
    /// Composes `root` once at time zero.
    fn new(root: impl FnMut() -> T + 'static) -> Self {
        let mut composition = Composition::new(root);
        composition.recompose();
        Self {
            composition,
            now: Duration::ZERO,
        }
    }

    /// Runs one frame `milliseconds` after the previous one, then recomposes.
    fn advance(&mut self, milliseconds: u64) {
        self.now += Duration::from_millis(milliseconds);
        self.composition.run_frame(self.now);
        self.composition.recompose();
    }

    /// Advances in 16 ms frames until `milliseconds` have passed in total from now.
    fn run(&mut self, milliseconds: u64) -> Vec<f32>
    where
        T: Clone + Into<f32>,
    {
        let end = self.now + Duration::from_millis(milliseconds);
        let mut seen = Vec::new();
        while self.now < end {
            self.advance(16.min((end - self.now).as_millis() as u64));
            seen.push(self.composition.output().unwrap().clone().into());
        }
        seen
    }
}

/// A float state read in the root, so a change recomposes it.
fn read(state: &State<f32>) -> f32 {
    state.get()
}

/// Asserts `a` is within `epsilon` of `b`.
fn near(a: f32, b: f32, epsilon: f32) {
    assert!((a - b).abs() <= epsilon, "{a} is not near {b}");
}

/// A rig that animates to the target held in `target`.
fn follower(spec: AnimationSpec, target: Rc<RefCell<f32>>) -> Rig<f32> {
    Rig::new(move || {
        let value = animate_float_as_state(*target.borrow(), spec);
        read(&value)
    })
}

#[test]
fn tween_moves_linearly_and_lands_on_the_target() {
    let target = Rc::new(RefCell::new(0.0));
    let spec = AnimationSpec::Tween {
        duration: Duration::from_millis(100),
        easing: Easing::Linear,
    };
    let mut rig = follower(spec, target.clone());
    *target.borrow_mut() = 10.0;
    rig.composition.invalidate();
    rig.composition.recompose();
    assert_eq!(*rig.composition.output().unwrap(), 0.0);
    rig.advance(0);
    assert_eq!(*rig.composition.output().unwrap(), 0.0);
    rig.advance(50);
    near(*rig.composition.output().unwrap(), 5.0, 1e-4);
    rig.advance(25);
    near(*rig.composition.output().unwrap(), 7.5, 1e-4);
    rig.advance(500);
    assert_eq!(*rig.composition.output().unwrap(), 10.0);
    assert!(!rig.composition.has_pending_frame());
}

#[test]
fn retargeting_midway_starts_from_where_the_value_is() {
    let target = Rc::new(RefCell::new(0.0));
    let spec = AnimationSpec::Tween {
        duration: Duration::from_millis(100),
        easing: Easing::Linear,
    };
    let mut rig = follower(spec, target.clone());
    *target.borrow_mut() = 10.0;
    rig.composition.invalidate();
    rig.composition.recompose();
    rig.advance(0);
    rig.advance(50);
    near(*rig.composition.output().unwrap(), 5.0, 1e-4);
    *target.borrow_mut() = 0.0;
    rig.composition.invalidate();
    rig.composition.recompose();
    rig.advance(0);
    rig.advance(50);
    near(*rig.composition.output().unwrap(), 2.5, 1e-4);
    rig.advance(100);
    assert_eq!(*rig.composition.output().unwrap(), 0.0);
}

#[test]
fn spring_overshoots_then_settles_on_the_target() {
    let target = Rc::new(RefCell::new(0.0));
    let mut rig = follower(spring(), target.clone());
    *target.borrow_mut() = 100.0;
    rig.composition.invalidate();
    rig.composition.recompose();
    let seen = rig.run(3000);
    assert!(seen.iter().any(|value| *value > 100.0), "no overshoot");
    assert_eq!(*seen.last().unwrap(), 100.0);
    assert!(!rig.composition.has_pending_frame());
}

#[test]
fn snap_arrives_on_the_first_frame() {
    let target = Rc::new(RefCell::new(0.0));
    let mut rig = follower(snap(), target.clone());
    *target.borrow_mut() = 7.0;
    rig.composition.invalidate();
    rig.composition.recompose();
    rig.advance(16);
    assert_eq!(*rig.composition.output().unwrap(), 7.0);
}

#[test]
fn keyframes_pass_through_each_stop() {
    static FRAMES: [Keyframe; 3] = [
        Keyframe::new(0.25, 0.5),
        Keyframe::new(0.5, 1.5),
        Keyframe::new(0.75, 0.5),
    ];
    let target = Rc::new(RefCell::new(0.0));
    let mut rig = follower(keyframes(100, &FRAMES), target.clone());
    *target.borrow_mut() = 10.0;
    rig.composition.invalidate();
    rig.composition.recompose();
    rig.advance(0);
    for (at, expected) in [(25, 5.0), (25, 15.0), (25, 5.0), (12, 7.5)] {
        rig.advance(at);
        let value = *rig.composition.output().unwrap();
        near(value, expected, 0.2);
    }
    rig.advance(100);
    assert_eq!(*rig.composition.output().unwrap(), 10.0);
}

#[test]
fn keyframe_easing_shapes_the_way_in() {
    static FRAMES: [Keyframe; 1] = [Keyframe::new(0.5, 1.0).with_easing(Easing::EaseOut)];
    let mut stepper = Stepper::new(keyframes(100, &FRAMES));
    stepper.step(Duration::ZERO);
    stepper.step(Duration::from_millis(25));
    near(stepper.progress, Easing::EaseOut.apply(0.5), 1e-4);
    stepper.step(Duration::from_millis(50));
    near(stepper.progress, 1.0, 1e-4);
    stepper.step(Duration::from_millis(75));
    near(stepper.progress, 1.0, 1e-4);
    assert!(stepper.step(Duration::from_millis(100)));
}

#[test]
fn repeat_restart_runs_the_count_and_ends_on_the_target() {
    let spec = repeatable(
        3,
        AnimationSpec::Tween {
            duration: Duration::from_millis(100),
            easing: Easing::Linear,
        },
        RepeatMode::Restart,
    );
    let mut stepper = Stepper::new(spec);
    assert!(!stepper.step(Duration::ZERO));
    for (ms, expected) in [(50, 0.5), (100, 0.0), (150, 0.5), (250, 0.5)] {
        assert!(!stepper.step(Duration::from_millis(ms)));
        near(stepper.progress, expected, 1e-4);
    }
    assert!(stepper.step(Duration::from_millis(300)));
    assert_eq!(stepper.resolve(&0.0, &1.0, true), 1.0);
    assert!(stepper.step(Duration::from_millis(900)));
    assert_eq!(stepper.progress, 1.0);
}

#[test]
fn repeat_reverse_goes_back_and_forth() {
    let spec = repeatable(
        2,
        AnimationSpec::Tween {
            duration: Duration::from_millis(100),
            easing: Easing::Linear,
        },
        RepeatMode::Reverse,
    );
    let mut stepper = Stepper::new(spec);
    stepper.step(Duration::ZERO);
    stepper.step(Duration::from_millis(75));
    near(stepper.progress, 0.75, 1e-4);
    stepper.step(Duration::from_millis(125));
    near(stepper.progress, 0.75, 1e-4);
    stepper.step(Duration::from_millis(175));
    near(stepper.progress, 0.25, 1e-4);
    assert!(stepper.step(Duration::from_millis(200)));
    assert_eq!(stepper.resolve(&0.0, &1.0, true), 0.0);
}

#[test]
fn infinite_repeat_never_finishes_even_with_no_duration() {
    let tween = AnimationSpec::Tween {
        duration: Duration::ZERO,
        easing: Easing::Linear,
    };
    let mut stepper = Stepper::new(infinite_repeatable(tween, RepeatMode::Reverse));
    stepper.step(Duration::ZERO);
    assert!(!stepper.step(Duration::from_millis(40)));
    assert!(stepper.progress.is_finite());
    let tween = AnimationSpec::Tween {
        duration: Duration::from_millis(100),
        easing: Easing::Linear,
    };
    let mut stepper = Stepper::new(infinite_repeatable(tween, RepeatMode::Reverse));
    stepper.step(Duration::ZERO);
    for (ms, expected) in [(50, 0.5), (150, 0.5), (190, 0.1), (230, 0.3), (10_090, 0.9)] {
        assert!(!stepper.step(Duration::from_millis(ms)));
        near(stepper.progress, expected, 1e-3);
    }
}

#[test]
fn transition_values_follow_the_state_each_on_their_own_spec() {
    let on = Rc::new(RefCell::new(false));
    let flag = on.clone();
    let mut rig = Rig::new(move || {
        let transition = update_transition(*flag.borrow());
        let linear = AnimationSpec::Tween {
            duration: Duration::from_millis(100),
            easing: Easing::Linear,
        };
        let size = transition.animate_float(|_, _| linear, |on| if *on { 100.0 } else { 0.0 });
        let color = transition.animate_color(
            |_, _| snap(),
            |on| {
                if *on {
                    Color::rgb(255, 255, 255)
                } else {
                    Color::rgb(0, 0, 0)
                }
            },
        );
        (size.get(), color.get(), transition.is_changing())
    });
    assert_eq!(rig.composition.output().unwrap().0, 0.0);
    *on.borrow_mut() = true;
    rig.composition.invalidate();
    rig.composition.recompose();
    rig.advance(0);
    rig.advance(50);
    let (size, color, _) = *rig.composition.output().unwrap();
    near(size, 50.0, 1e-3);
    assert_eq!(color, Color::rgb(255, 255, 255));
    rig.advance(100);
    assert_eq!(rig.composition.output().unwrap().0, 100.0);
}

#[test]
fn transition_reports_changing_only_while_it_runs() {
    let on = Rc::new(RefCell::new(false));
    let flag = on.clone();
    let mut rig = Rig::new(move || {
        let transition = update_transition(*flag.borrow());
        let size = transition.animate_float(
            |_, _| AnimationSpec::Tween {
                duration: Duration::from_millis(100),
                easing: Easing::Linear,
            },
            |on| if *on { 1.0 } else { 0.0 },
        );
        (size.get(), transition.is_changing())
    });
    assert!(!rig.composition.output().unwrap().1);
    *on.borrow_mut() = true;
    rig.composition.invalidate();
    rig.composition.recompose();
    rig.advance(0);
    rig.advance(50);
    assert!(rig.composition.output().unwrap().1);
    rig.advance(100);
    assert_eq!(rig.composition.output().unwrap().0, 1.0);
    assert!(!rig.composition.output().unwrap().1);
}

#[test]
fn animatable_tweens_snaps_and_sequences() {
    let log: Rc<RefCell<Vec<f32>>> = Rc::default();
    let seen = log.clone();
    let mut rig = Rig::new(move || {
        let animatable = remember_animatable(0.0f32);
        let driven = animatable.clone();
        let seen = seen.clone();
        launched_effect((), move |scope| async move {
            let linear = AnimationSpec::Tween {
                duration: Duration::from_millis(100),
                easing: Easing::Linear,
            };
            driven.animate_to(&scope, 10.0, linear).await;
            seen.borrow_mut().push(driven.value());
            driven.animate_to(&scope, 20.0, snap()).await;
            seen.borrow_mut().push(driven.value());
            driven.snap_to(3.0);
            driven.animate_to(&scope, 4.0, linear).await;
            seen.borrow_mut().push(driven.value());
        });
        animatable.value()
    });
    for _ in 0..30 {
        rig.advance(16);
    }
    assert_eq!(*log.borrow(), vec![10.0, 20.0, 4.0]);
}

#[test]
fn decay_coasts_to_the_closed_form_distance_and_rests() {
    let spec = DecaySpec::default();
    let last: Rc<RefCell<f32>> = Rc::default();
    let out: Rc<RefCell<Option<f32>>> = Rc::default();
    let (value, ended) = (last.clone(), out.clone());
    let mut rig = Rig::new(move || {
        let (value, ended) = (value.clone(), ended.clone());
        launched_effect((), move |scope| async move {
            let left = animate_decay(&scope, 5.0, 840.0, spec, |v| *value.borrow_mut() = v).await;
            *ended.borrow_mut() = Some(left);
        });
    });
    rig.advance(16);
    rig.advance(1000);
    let expected = 5.0 + 840.0 / 4.2 * (1.0 - (-4.2f32).exp());
    near(*last.borrow(), expected, 0.5);
    for _ in 0..200 {
        rig.advance(16);
    }
    assert_eq!(*out.borrow(), Some(0.0));
    near(*last.borrow(), 5.0 + spec.distance(840.0), 1.0);
}

#[test]
fn decay_stops_at_a_bound_with_the_velocity_left() {
    let spec = DecaySpec::default().bounded(0.0, 50.0);
    let last: Rc<RefCell<f32>> = Rc::default();
    let out: Rc<RefCell<Option<f32>>> = Rc::default();
    let (value, ended) = (last.clone(), out.clone());
    let mut rig = Rig::new(move || {
        let (value, ended) = (value.clone(), ended.clone());
        launched_effect((), move |scope| async move {
            let left = animate_decay(&scope, 0.0, 2000.0, spec, |v| *value.borrow_mut() = v).await;
            *ended.borrow_mut() = Some(left);
        });
    });
    for _ in 0..40 {
        rig.advance(16);
    }
    assert_eq!(*last.borrow(), 50.0);
    assert!(out.borrow().unwrap() > 100.0);
}

#[test]
fn fling_moves_the_animatable_and_comes_to_rest() {
    let log: Rc<RefCell<Vec<f32>>> = Rc::default();
    let seen = log.clone();
    let mut rig = Rig::new(move || {
        let animatable = remember_animatable(0.0f32);
        let driven = animatable.clone();
        let seen = seen.clone();
        launched_effect((), move |scope| async move {
            driven.fling(&scope, -420.0).await;
            seen.borrow_mut().push(driven.value());
        });
        animatable.value()
    });
    for _ in 0..300 {
        rig.advance(16);
    }
    let end = log.borrow()[0];
    near(end, -100.0, 1.0);
    assert_eq!(*rig.composition.output().unwrap(), end);
}

#[test]
fn infinite_floats_wrap_and_bounce_smoothly() {
    let mut rig = Rig::new(|| {
        let spin =
            infinite_float_repeating(0.0, 360.0, Duration::from_millis(1000), Easing::Linear);
        let swing = infinite_float(0.0, 10.0, Duration::from_millis(1000), Easing::Linear);
        (spin.get(), swing.get())
    });
    rig.advance(0);
    let mut spins = Vec::new();
    let mut swings = Vec::new();
    for _ in 0..250 {
        rig.advance(16);
        let (spin, swing) = *rig.composition.output().unwrap();
        spins.push(spin);
        swings.push(swing);
    }
    assert!(spins.iter().all(|v| (0.0..=360.0).contains(v)));
    assert!(swings.iter().all(|v| (0.0..=10.0).contains(v)));
    let max_jump = spins
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .filter(|jump| *jump < 300.0)
        .fold(0.0f32, f32::max);
    assert!(max_jump < 360.0 * 0.016 * 1.6, "spin stutters: {max_jump}");
    assert!(
        spins.windows(2).any(|pair| pair[1] < pair[0]),
        "never wrapped"
    );
    assert!(swings.iter().any(|v| *v > 9.5) && swings.iter().any(|v| *v < 0.5));
    let stalled = swings.windows(3).any(|w| w[0] == w[1] && w[1] == w[2]);
    assert!(!stalled, "swing stalls");
}

/// A path of two lines and a curve around `x`.
fn shape(x: f32) -> Path {
    Path::new(
        vec![
            PathCommand::MoveTo(x, 0.0),
            PathCommand::LineTo(x + 10.0, 10.0),
            PathCommand::CubicTo(x, 1.0, x, 2.0, x, 3.0),
            PathCommand::Close,
        ],
        x + 10.0,
        10.0,
        FillRule::NonZero,
    )
}

#[test]
fn paths_morph_point_by_point_and_end_on_the_target() {
    let a = shape(0.0);
    let b = shape(100.0);
    let half = a.lerp(&b, 0.5);
    assert_eq!(half.commands()[0], PathCommand::MoveTo(50.0, 0.0));
    assert_eq!(half.commands()[1], PathCommand::LineTo(60.0, 10.0));
    assert_eq!(half.commands()[3], PathCommand::Close);
    assert_eq!(half.size(), (60.0, 10.0));
    let incompatible = Path::new(
        vec![PathCommand::MoveTo(0.0, 0.0)],
        1.0,
        1.0,
        FillRule::NonZero,
    );
    assert_eq!(a.lerp(&incompatible, 0.4).commands(), a.commands());
    assert_eq!(
        a.lerp(&incompatible, 0.6).commands(),
        incompatible.commands()
    );

    let target = Rc::new(RefCell::new(0.0f32));
    let x = target.clone();
    let mut rig = Rig::new(move || {
        let path = animate_as_state(shape(*x.borrow()), tween(100));
        path.get().commands()[0]
    });
    *target.borrow_mut() = 100.0;
    rig.composition.invalidate();
    rig.composition.recompose();
    rig.advance(0);
    rig.advance(50);
    match rig.composition.output().unwrap() {
        PathCommand::MoveTo(x, _) => assert!(*x > 0.0 && *x < 100.0),
        _ => unreachable!(),
    }
    rig.advance(200);
    assert_eq!(
        *rig.composition.output().unwrap(),
        PathCommand::MoveTo(100.0, 0.0)
    );
}

#[test]
fn an_infinite_transition_runs_its_value_over_and_over() {
    let mut rig = Rig::new(|| {
        let transition = remember_infinite_transition();
        let tween = AnimationSpec::Tween {
            duration: Duration::from_millis(100),
            easing: Easing::Linear,
        };
        transition
            .animate_float(0.0, 360.0, infinite_repeatable(tween, RepeatMode::Restart))
            .get()
    });
    rig.advance(0);
    let mut angles = Vec::new();
    for _ in 0..30 {
        rig.advance(16);
        angles.push(*rig.composition.output().unwrap());
    }
    assert!(angles.iter().all(|angle| (0.0..=360.0).contains(angle)));
    assert!(angles.iter().any(|angle| *angle > 180.0));
    assert!(
        angles.windows(2).any(|pair| pair[1] < pair[0]),
        "never restarted"
    );
}

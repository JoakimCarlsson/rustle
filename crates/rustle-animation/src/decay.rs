use rustle_runtime::TaskScope;

use crate::animatable::*;

/// How a value coasts to a stop after being flung: its speed falls
/// exponentially.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecaySpec {
    /// How quickly the speed falls, per second. Higher stops sooner.
    pub friction: f32,
    /// The speed, in units per second, under which the value is at rest.
    pub rest_velocity: f32,
    /// The lowest and highest value the coasting may reach. It stops at a bound.
    pub bounds: Option<(f32, f32)>,
}

impl DecaySpec {
    /// The same decay, stopping at `min` or `max` if it gets there.
    pub fn bounded(self, min: f32, max: f32) -> Self {
        Self {
            bounds: Some((min, max)),
            ..self
        }
    }

    /// How far a fling at `velocity` goes before it rests.
    pub fn distance(&self, velocity: f32) -> f32 {
        velocity / self.friction
    }
}

impl Default for DecaySpec {
    fn default() -> Self {
        Self {
            friction: 4.2,
            rest_velocity: 1.0,
            bounds: None,
        }
    }
}

/// Coasts a number from `from` at `velocity` (units per second) as `spec`
/// says, calling `apply` with each value on the way. Returns the velocity at
/// the end, which is zero unless a bound stopped it first.
pub async fn animate_decay(
    scope: &TaskScope,
    from: f32,
    velocity: f32,
    spec: DecaySpec,
    mut apply: impl FnMut(f32),
) -> f32 {
    let friction = spec.friction.max(f32::EPSILON);
    let mut started = None;
    loop {
        let now = scope.next_frame().await;
        let begin = *started.get_or_insert(now);
        let elapsed = (now - begin).as_secs_f32();
        let decay = (-friction * elapsed).exp();
        let current_velocity = velocity * decay;
        let mut value = from + velocity / friction * (1.0 - decay);
        if let Some((min, max)) = spec.bounds
            && (value < min || value > max)
        {
            value = value.clamp(min, max);
            apply(value);
            return current_velocity;
        }
        if current_velocity.abs() < spec.rest_velocity {
            apply(value);
            return 0.0;
        }
        apply(value);
    }
}

impl Animatable<f32> {
    /// Coasts from the current value at `velocity` until it rests, as `spec`
    /// says. Returns the velocity left when a bound stopped it, otherwise zero.
    pub async fn animate_decay(&self, scope: &TaskScope, velocity: f32, spec: DecaySpec) -> f32 {
        let state = self.state().clone();
        animate_decay(scope, self.value(), velocity, spec, move |value| {
            state.set(value)
        })
        .await
    }

    /// Flings from the current value at `velocity` with the default decay.
    pub async fn fling(&self, scope: &TaskScope, velocity: f32) -> f32 {
        self.animate_decay(scope, velocity, DecaySpec::default())
            .await
    }
}

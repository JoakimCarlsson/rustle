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

    pub(crate) fn apply(self, t: f32) -> f32 {
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
pub(crate) fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
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

use rustle_graphics::Color;

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

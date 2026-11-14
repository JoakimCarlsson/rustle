use rustle_graphics::{Color, Path, PathCommand};

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

impl Lerp for (f32, f32) {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        (self.0.lerp(&to.0, t), self.1.lerp(&to.1, t))
    }
}

/// Whether two paths have the same commands in the same order, so that one
/// can morph into the other point by point.
pub fn paths_compatible(a: &Path, b: &Path) -> bool {
    a.commands().len() == b.commands().len()
        && a.commands()
            .iter()
            .zip(b.commands())
            .all(|(a, b)| std::mem::discriminant(a) == std::mem::discriminant(b))
}

/// Morphs between compatible paths (see [`paths_compatible`]) by blending each
/// point and the box they are in. Paths that are not compatible cannot be
/// blended, so the result is `self` for the first half and `to` for the rest.
impl Lerp for Path {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        if !paths_compatible(self, to) {
            return if t < 0.5 { self.clone() } else { to.clone() };
        }
        let mix = |a: f32, b: f32| a.lerp(&b, t);
        let commands = self
            .commands()
            .iter()
            .zip(to.commands())
            .map(|pair| match pair {
                (PathCommand::MoveTo(ax, ay), PathCommand::MoveTo(bx, by)) => {
                    PathCommand::MoveTo(mix(*ax, *bx), mix(*ay, *by))
                }
                (PathCommand::LineTo(ax, ay), PathCommand::LineTo(bx, by)) => {
                    PathCommand::LineTo(mix(*ax, *bx), mix(*ay, *by))
                }
                (PathCommand::QuadTo(a1, a2, a3, a4), PathCommand::QuadTo(b1, b2, b3, b4)) => {
                    PathCommand::QuadTo(mix(*a1, *b1), mix(*a2, *b2), mix(*a3, *b3), mix(*a4, *b4))
                }
                (
                    PathCommand::CubicTo(a1, a2, a3, a4, a5, a6),
                    PathCommand::CubicTo(b1, b2, b3, b4, b5, b6),
                ) => PathCommand::CubicTo(
                    mix(*a1, *b1),
                    mix(*a2, *b2),
                    mix(*a3, *b3),
                    mix(*a4, *b4),
                    mix(*a5, *b5),
                    mix(*a6, *b6),
                ),
                _ => PathCommand::Close,
            })
            .collect();
        let (aw, ah) = self.size();
        let (bw, bh) = to.size();
        Path::new(commands, mix(aw, bw), mix(ah, bh), self.fill_rule())
    }
}

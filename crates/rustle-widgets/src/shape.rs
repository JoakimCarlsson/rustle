//! [`Shape`]: the outline of a surface, as Compose's `RectangleShape`, `RoundedCornerShape`,
//! `CutCornerShape` and `CircleShape`, and the modifiers that fill, outline and clip with one.

use rustle_foundation::Modifier;
use rustle_graphics::{Color, FillRule, Path, PathCommand};

/// The size of each corner of a [`Shape`], in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CornerSizes {
    /// Top left.
    pub top_left: f32,
    /// Top right.
    pub top_right: f32,
    /// Bottom right.
    pub bottom_right: f32,
    /// Bottom left.
    pub bottom_left: f32,
}

impl CornerSizes {
    /// The same `size` at every corner.
    pub const fn uniform(size: f32) -> Self {
        Self {
            top_left: size,
            top_right: size,
            bottom_right: size,
            bottom_left: size,
        }
    }

    /// The four sizes, clockwise from the top left.
    pub const fn new(top_left: f32, top_right: f32, bottom_right: f32, bottom_left: f32) -> Self {
        Self {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        }
    }

    /// The sizes as `[top left, top right, bottom right, bottom left]`.
    fn array(self) -> [f32; 4] {
        [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ]
    }

    /// The same sizes, each made `by` smaller and never below zero.
    fn shrunk(self, by: f32) -> Self {
        let [a, b, c, d] = self.array().map(|size| (size - by).max(0.0));
        Self::new(a, b, c, d)
    }

    /// The same sizes scaled down together so that two corners never meet beyond the middle of a
    /// `width` by `height` box.
    fn fitted(self, width: f32, height: f32) -> Self {
        let [tl, tr, br, bl] = self.array();
        let mut scale = 1.0_f32;
        for (sum, room) in [
            (tl + tr, width),
            (bl + br, width),
            (tl + bl, height),
            (tr + br, height),
        ] {
            if sum > room && sum > 0.0 {
                scale = scale.min(room / sum);
            }
        }
        Self::new(tl * scale, tr * scale, br * scale, bl * scale)
    }
}

/// The outline of a surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// Square corners: Compose's `RectangleShape`.
    Rectangle,
    /// Corners rounded by a radius each: Compose's `RoundedCornerShape`.
    Rounded(CornerSizes),
    /// Corners cut off in a straight line by a size each: Compose's `CutCornerShape`.
    Cut(CornerSizes),
    /// Ends as round as can be: Compose's `CircleShape`, a circle on a square and a pill on a
    /// rectangle.
    Circle,
}

impl Shape {
    /// The radius that rounds a corner into a half circle whatever the size.
    pub const FULL: f32 = 10_000.0;

    /// Rounded corners of one `radius`.
    pub const fn rounded(radius: f32) -> Self {
        Self::Rounded(CornerSizes::uniform(radius))
    }

    /// Rounded corners of a radius each, clockwise from the top left.
    pub const fn rounded_corners(
        top_left: f32,
        top_right: f32,
        bottom_right: f32,
        bottom_left: f32,
    ) -> Self {
        Self::Rounded(CornerSizes::new(
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        ))
    }

    /// Cut corners of one `size`.
    pub const fn cut(size: f32) -> Self {
        Self::Cut(CornerSizes::uniform(size))
    }

    /// Cut corners of a size each, clockwise from the top left.
    pub const fn cut_corners(
        top_left: f32,
        top_right: f32,
        bottom_right: f32,
        bottom_left: f32,
    ) -> Self {
        Self::Cut(CornerSizes::new(
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        ))
    }

    /// The largest corner size: what to give anything that takes a single radius, such as a
    /// shadow.
    pub fn radius(&self) -> f32 {
        match self {
            Self::Rectangle => 0.0,
            Self::Circle => Self::FULL,
            Self::Rounded(sizes) | Self::Cut(sizes) => {
                sizes.array().into_iter().fold(0.0, f32::max)
            }
        }
    }

    /// The radius of a shape with rounded corners that are all the same, which the renderer
    /// draws without a path.
    fn uniform_radius(&self) -> Option<f32> {
        match self {
            Self::Rectangle => Some(0.0),
            Self::Circle => Some(Self::FULL),
            Self::Rounded(sizes) => {
                let [a, b, c, d] = sizes.array();
                (a == b && b == c && c == d).then_some(a)
            }
            Self::Cut(sizes) => (sizes.array().into_iter().all(|size| size == 0.0)).then_some(0.0),
        }
    }

    /// The outline in a `width` by `height` box, inset by `inset` on every side.
    pub fn path(&self, width: f32, height: f32, inset: f32) -> Path {
        let (w, h) = (
            (width - 2.0 * inset).max(0.0),
            (height - 2.0 * inset).max(0.0),
        );
        let mut commands = Vec::new();
        self.outline(&mut commands, w, h, inset, inset);
        Path::new(commands, width, height, FillRule::NonZero)
    }

    /// The area between the outline and the outline inset by `thickness`: a border.
    pub fn ring(&self, width: f32, height: f32, thickness: f32) -> Path {
        let mut commands = Vec::new();
        self.outline_into(&mut commands, width, height, 0.0, 0.0, 0.0);
        let inner = (
            (width - 2.0 * thickness).max(0.0),
            (height - 2.0 * thickness).max(0.0),
        );
        self.outline_into(
            &mut commands,
            inner.0,
            inner.1,
            thickness,
            thickness,
            thickness,
        );
        Path::new(commands, width, height, FillRule::EvenOdd)
    }

    /// Adds the outline of a `w` by `h` box at (`x`, `y`) to `commands`.
    fn outline(&self, commands: &mut Vec<PathCommand>, w: f32, h: f32, x: f32, y: f32) {
        self.outline_into(commands, w, h, x, y, x);
    }

    /// Adds the outline of a `w` by `h` box at (`x`, `y`) to `commands`, with every corner
    /// `shrink` smaller than the shape says.
    fn outline_into(
        &self,
        commands: &mut Vec<PathCommand>,
        w: f32,
        h: f32,
        x: f32,
        y: f32,
        shrink: f32,
    ) {
        let (sizes, cut) = match self {
            Self::Rectangle => (CornerSizes::default(), false),
            Self::Circle => (CornerSizes::uniform(w.min(h) / 2.0 + shrink), false),
            Self::Rounded(sizes) => (*sizes, false),
            Self::Cut(sizes) => (*sizes, true),
        };
        let [tl, tr, br, bl] = sizes.shrunk(shrink).fitted(w, h).array();
        let (right, bottom) = (x + w, y + h);
        const KAPPA: f32 = 0.552_284_8;
        let corner = |commands: &mut Vec<PathCommand>,
                      size: f32,
                      from: (f32, f32),
                      to: (f32, f32),
                      control: (f32, f32)| {
            if size <= 0.0 {
                commands.push(PathCommand::LineTo(control.0, control.1));
            } else if cut {
                commands.push(PathCommand::LineTo(from.0, from.1));
                commands.push(PathCommand::LineTo(to.0, to.1));
            } else {
                let c1 = (
                    from.0 + (control.0 - from.0) * KAPPA,
                    from.1 + (control.1 - from.1) * KAPPA,
                );
                let c2 = (
                    to.0 + (control.0 - to.0) * KAPPA,
                    to.1 + (control.1 - to.1) * KAPPA,
                );
                commands.push(PathCommand::LineTo(from.0, from.1));
                commands.push(PathCommand::CubicTo(c1.0, c1.1, c2.0, c2.1, to.0, to.1));
            }
        };
        commands.push(PathCommand::MoveTo(x + tl, y));
        corner(commands, tr, (right - tr, y), (right, y + tr), (right, y));
        corner(
            commands,
            br,
            (right, bottom - br),
            (right - br, bottom),
            (right, bottom),
        );
        corner(
            commands,
            bl,
            (x + bl, bottom),
            (x, bottom - bl),
            (x, bottom),
        );
        corner(commands, tl, (x, y + tl), (x + tl, y), (x, y));
        commands.push(PathCommand::Close);
    }
}

/// Modifiers that take a [`Shape`] where Rustle's own take a radius.
pub trait ShapeModifier: Sized {
    /// Fills the area with `color` in `shape`.
    fn background_shape(self, color: Color, shape: Shape) -> Self;

    /// Draws a border of `width` and `color` along `shape`.
    fn border_shape(self, width: f32, color: Color, shape: Shape) -> Self;

    /// Cuts off what the content draws outside `shape`. Only rounded corners cut: the content
    /// of a cut-corner shape is clipped to its rectangle.
    fn clip_shape(self, shape: Shape) -> Self;
}

impl ShapeModifier for Modifier {
    fn background_shape(self, color: Color, shape: Shape) -> Self {
        match shape.uniform_radius() {
            Some(0.0) => self.background(color),
            Some(radius) => self.background_rounded(color, radius),
            None => self.draw_behind(move |canvas| {
                let (w, h) = (canvas.width(), canvas.size().height);
                canvas.path(&shape.path(w, h, 0.0), 0.0, 0.0, w, h, color);
            }),
        }
    }

    fn border_shape(self, width: f32, color: Color, shape: Shape) -> Self {
        match shape.uniform_radius() {
            Some(radius) => self.border(width, color, radius),
            None => self.draw_with_content(move |canvas| {
                let (w, h) = (canvas.width(), canvas.size().height);
                canvas.path(&shape.ring(w, h, width), 0.0, 0.0, w, h, color);
            }),
        }
    }

    fn clip_shape(self, shape: Shape) -> Self {
        match shape {
            Shape::Rectangle => self.clip(),
            Shape::Circle => self.clip_rounded(Shape::FULL),
            Shape::Rounded(sizes) => self.clip_rounded_corners(
                sizes.top_left,
                sizes.top_right,
                sizes.bottom_right,
                sizes.bottom_left,
            ),
            Shape::Cut(_) => self.clip(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The points a path visits, in order.
    fn points(path: &Path) -> Vec<(f32, f32)> {
        path.commands()
            .iter()
            .filter_map(|command| match *command {
                PathCommand::MoveTo(x, y) | PathCommand::LineTo(x, y) => Some((x, y)),
                PathCommand::CubicTo(.., x, y) => Some((x, y)),
                _ => None,
            })
            .collect()
    }

    /// A rectangle is its four corners, and its radius is zero.
    #[test]
    fn rectangle_has_square_corners() {
        let path = Shape::Rectangle.path(10.0, 6.0, 0.0);
        let corners = points(&path);
        for corner in [(10.0, 0.0), (10.0, 6.0), (0.0, 6.0), (0.0, 0.0)] {
            assert!(corners.contains(&corner), "{corner:?} in {corners:?}");
        }
        assert_eq!(Shape::Rectangle.radius(), 0.0);
    }

    /// A cut corner joins two points on the edges with a straight line.
    #[test]
    fn cut_corners_are_straight_lines() {
        let path = Shape::cut(2.0).path(10.0, 10.0, 0.0);
        assert!(
            path.commands()
                .iter()
                .all(|c| !matches!(c, PathCommand::CubicTo(..)))
        );
        let corners = points(&path);
        assert!(corners.contains(&(8.0, 0.0)) && corners.contains(&(10.0, 2.0)));
        assert!(!corners.contains(&(10.0, 0.0)));
    }

    /// Rounded corners curve, and sizes too big for the box shrink together.
    #[test]
    fn rounded_corners_curve_and_fit_the_box() {
        let path = Shape::rounded_corners(4.0, 0.0, 8.0, 0.0).path(10.0, 10.0, 0.0);
        let curves = path
            .commands()
            .iter()
            .filter(|c| matches!(c, PathCommand::CubicTo(..)))
            .count();
        assert_eq!(curves, 2);
        let sizes = CornerSizes::uniform(10.0).fitted(10.0, 10.0);
        assert_eq!(sizes.top_left, 5.0);
    }

    /// A pill rounds as far as half the short side, and a ring is two outlines filled
    /// even-odd.
    #[test]
    fn circle_is_a_pill_and_a_ring_has_two_outlines() {
        let path = Shape::Circle.path(40.0, 20.0, 0.0);
        let corners = points(&path);
        assert!(corners.contains(&(30.0, 0.0)) && corners.contains(&(40.0, 10.0)));
        let ring = Shape::rounded_corners(4.0, 0.0, 4.0, 0.0).ring(20.0, 20.0, 2.0);
        assert_eq!(ring.fill_rule(), FillRule::EvenOdd);
        let closes = ring
            .commands()
            .iter()
            .filter(|c| matches!(c, PathCommand::Close))
            .count();
        assert_eq!(closes, 2);
    }
}

//! A shadow a shape casts, described as a value.

use rustle_geometry::Offset;

use crate::{BlendMode, Color};

/// A shadow a shape casts behind itself, as Compose's
/// `androidx.compose.ui.graphics.shadow.Shadow`: the shape grown by `spread`, moved by
/// `offset`, blurred over `radius` and filled with `color` at `alpha`, put on what is below as
/// `blend_mode` says.
///
/// Drawn with `Modifier::drop_shadow_with` (and `drop_shadow_shape` for a widget `Shape`);
/// several shadows are layered by chaining one modifier per shadow, the first drawn lowest.
///
/// ```
/// use rustle_graphics::{Color, Shadow};
/// use rustle_geometry::Offset;
///
/// let shadow = Shadow::new(12.0)
///     .with_offset(Offset::new(0.0, 4.0))
///     .with_spread(2.0)
///     .with_color(Color::rgb(0, 0, 0))
///     .with_alpha(0.25);
/// assert_eq!(shadow.radius, 12.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    /// How far the edge of the shadow fades, in logical pixels: Compose's `radius`.
    pub radius: f32,
    /// How far the shape is grown on every side before it is blurred, in logical pixels;
    /// negative shrinks it.
    pub spread: f32,
    /// How far the shadow is moved from the shape, in logical pixels.
    pub offset: Offset,
    /// The color of the shadow where it is solid.
    pub color: Color,
    /// How opaque the shadow is, from 0 to 1, on top of the color's own alpha.
    pub alpha: f32,
    /// How the shadow combines with what is drawn below it.
    pub blend_mode: BlendMode,
}

impl Shadow {
    /// A black shadow blurred over `radius` logical pixels, not grown or moved, fully opaque
    /// and drawn over what is below: Compose's `Shadow(radius)` with its defaults.
    pub const fn new(radius: f32) -> Self {
        Self {
            radius,
            spread: 0.0,
            offset: Offset::new(0.0, 0.0),
            color: Color::rgb(0, 0, 0),
            alpha: 1.0,
            blend_mode: BlendMode::SrcOver,
        }
    }

    /// This shadow grown by `spread` logical pixels on every side.
    pub const fn with_spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }

    /// This shadow moved by `offset` logical pixels.
    pub const fn with_offset(mut self, offset: Offset) -> Self {
        self.offset = offset;
        self
    }

    /// This shadow in `color`.
    pub const fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// This shadow at `alpha`, from 0 to 1.
    pub const fn with_alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }

    /// This shadow combined with what is below as `blend_mode` says.
    pub const fn with_blend_mode(mut self, blend_mode: BlendMode) -> Self {
        self.blend_mode = blend_mode;
        self
    }

    /// The color the shadow is filled with: its color with its alpha multiplied by
    /// [`alpha`](Self::alpha).
    pub fn effective_color(&self) -> Color {
        let alpha = (f32::from(self.color.a) * self.alpha.clamp(0.0, 1.0)).round() as u8;
        Color {
            a: alpha,
            ..self.color
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_composes() {
        let shadow = Shadow::new(8.0);
        assert_eq!(shadow.spread, 0.0);
        assert_eq!(shadow.offset, Offset::default());
        assert_eq!(shadow.color, Color::rgb(0, 0, 0));
        assert_eq!(shadow.alpha, 1.0);
        assert_eq!(shadow.blend_mode, BlendMode::SrcOver);
    }

    #[test]
    fn alpha_multiplies_the_colors_own() {
        let shadow = Shadow::new(4.0)
            .with_color(Color::rgba(10, 20, 30, 200))
            .with_alpha(0.5);
        assert_eq!(shadow.effective_color(), Color::rgba(10, 20, 30, 100));
    }
}
